//! ROCKNIX's `system.cfg` (`/storage/.config/system/configs/system.cfg`): every write pocket-ui makes to it goes
//! through here, so none of them loses a change another program made at the same moment.
//!
//! ROCKNIX's own writers (`set_setting`, `del_setting`, `sort_settings` in `/etc/profile.d/001-functions`, used by
//! runemu.sh, setsettings.sh, the boot scripts and wifictl) take the lock file `/tmp/.system.cfg.lock` before they
//! touch the file: `wait_lock` creates it with `set -o noclobber` (create-exclusive) and writes its process id into
//! it, retries once a second while it exists, and removes it when done (ROCKNIX 20261001, `rocknix/profile.d/
//! 001-functions`, `J_CONF_LOCK`, `wait_lock`, `set_setting`). runemu.sh writes `cooling.profile` at the start and the
//! end of a standalone game, exactly when pocket-ui saves the pause menu's options (docs/ES-REPLACEMENT-GAPS, 4.1).
//!
//! Here the same lock is taken the same way (create-exclusive, our process id in it), the file is read, changed and
//! written atomically (`fsutil::write_atomic`: a temporary file renamed over it) while the lock is held, and the lock
//! is removed only if it still holds our id. Read-modify-write under the lock never loses a change of a ROCKNIX
//! writer: they hold it for each of their own steps. Two differences, both for robustness:
//! - the wait is bounded (`WAIT`): a write that cannot get the lock fails instead of hanging a thread for ever;
//! - a lock whose process is gone (killed with SIGKILL, so its `trap` never ran) is removed. ROCKNIX's scripts would
//!   wait for it for ever, games included.
//! Threads of this process take turns on a mutex first, so they never race each other for the file.
//!
//! ROCKNIX main (after 20261001) changed `wait_lock`: it opens the same file for appending (`exec {fd}>>lock`), takes
//! `flock -x` on it and never removes it. The create-exclusive lock above would then find the file there for ever and
//! time out on every write, and ROCKNIX's flock would not see it. So the style is read from the running system's
//! 001-functions once (`LockStyle::of_device`): with `flock` in its wait_lock, the lock here is a `flock` on that file
//! too (`LockStyle::Flock`), kept open while the file is written and closed after; the file stays, as ROCKNIX's does.

use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// ROCKNIX's settings file.
pub const PATH: &str = "/storage/.config/system/configs/system.cfg";
/// ROCKNIX's lock for it (`J_CONF_LOCK`).
pub const LOCK: &str = "/tmp/.system.cfg.lock";
/// ROCKNIX's default, copied into place when there is no system.cfg yet, as `set_setting` does.
const SEED: &str = "/usr/config/system/configs/system.cfg";
/// How long a write waits for the lock. ROCKNIX's writers hold it for one `sed` or `echo`.
pub const WAIT: Duration = Duration::from_secs(10);
/// How often the lock is tried while someone else holds it (ROCKNIX's `wait_lock` sleeps 1 s).
const POLL: Duration = Duration::from_millis(50);

/// Threads of this process wait here before they try the lock file.
static IN_PROCESS: Mutex<()> = Mutex::new(());

/// ROCKNIX's shell functions, whose `wait_lock` says how the lock is taken.
pub const FUNCTIONS: &str = "/etc/profile.d/001-functions";

/// How ROCKNIX's `wait_lock` takes the lock (see the top of this file).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LockStyle {
    /// ROCKNIX 20261001: the file is created exclusively with the holder's process id in it and removed when done.
    CreateExclusive,
    /// ROCKNIX main after 20261001: `flock -x` on the file, which stays.
    Flock,
}

impl LockStyle {
    /// The running system's, read once from 001-functions (CreateExclusive when it can't be read).
    pub fn of_device() -> LockStyle {
        static STYLE: std::sync::OnceLock<LockStyle> = std::sync::OnceLock::new();
        *STYLE.get_or_init(|| {
            let text = crate::update::read_small(Path::new(FUNCTIONS), 1 << 20).unwrap_or_default();
            if uses_flock(&text) { LockStyle::Flock } else { LockStyle::CreateExclusive }
        })
    }
}

/// Whether the `wait_lock` of 001-functions text `functions` takes the lock with flock.
pub fn uses_flock(functions: &str) -> bool {
    let mut body = false;
    for line in functions.lines() {
        if crate::os_compat::defines_function(line, "wait_lock") {
            body = true;
        } else if body && line.starts_with('}') {
            return false;
        }
        if body && line.split(|c: char| !c.is_ascii_alphanumeric() && c != '_').any(|w| w == "flock") {
            return true;
        }
    }
    false
}

/// A system.cfg and its lock.
#[derive(Clone, Debug)]
pub struct SystemCfg {
    pub path: PathBuf,
    pub lock: PathBuf,
    /// Copied to `path` before the first write when `path` is missing.
    pub seed: Option<PathBuf>,
    pub wait: Duration,
    pub style: LockStyle,
}

impl SystemCfg {
    /// The device's file, ROCKNIX's lock.
    pub fn rocknix() -> SystemCfg {
        SystemCfg { path: PATH.into(), lock: LOCK.into(), seed: Some(SEED.into()), wait: WAIT, style: LockStyle::of_device() }
    }

    /// The file at `path`: ROCKNIX's lock for ROCKNIX's file, otherwise `<path>.lock` beside it (tests and the
    /// copies the unit tests of `emu_settings` work on).
    pub fn at(path: &Path) -> SystemCfg {
        if path == Path::new(PATH) {
            return SystemCfg::rocknix();
        }
        let mut lock = path.as_os_str().to_os_string();
        lock.push(".lock");
        SystemCfg { path: path.to_path_buf(), lock: lock.into(), seed: None, wait: WAIT, style: LockStyle::CreateExclusive }
    }

    /// The whole file ("" when it can't be read).
    pub fn read(&self) -> String {
        std::fs::read_to_string(&self.path).unwrap_or_default()
    }

    /// The value of the first `key=` line, as ROCKNIX's `get_setting key` reads a plain key.
    pub fn get(&self, key: &str) -> Option<String> {
        value(&self.read(), key).map(str::to_string)
    }

    /// Changes the file under ROCKNIX's lock: `change` gets the current text and returns the new one (None: leave it).
    /// Returns whether the file was written.
    pub fn edit(&self, change: impl FnOnce(&str) -> Option<String>) -> io::Result<bool> {
        let _turn = IN_PROCESS.lock().unwrap_or_else(|e| e.into_inner());
        let _held = Held::take(&self.lock, self.wait, self.style)?;
        let current = match std::fs::read_to_string(&self.path) {
            Ok(t) => t,
            Err(e) if e.kind() == io::ErrorKind::NotFound => self.seed.as_deref().and_then(|s| std::fs::read_to_string(s).ok()).unwrap_or_default(),
            Err(e) => return Err(e),
        };
        let Some(new) = change(&current) else { return Ok(false) };
        if new == current && self.path.exists() {
            return Ok(false);
        }
        crate::fsutil::write_atomic(&self.path, new.as_bytes())?;
        Ok(true)
    }

    /// ROCKNIX's `set_setting key value`: every `key=` line goes, `key=value` is added at the end; the value
    /// `default` only removes the key. Keys have to start with a letter or digit, as there.
    pub fn set(&self, key: &str, value: &str) -> io::Result<()> {
        if !valid_key(key) || value.contains('\n') {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, format!("not a system.cfg setting: {key:?}")));
        }
        self.edit(|cfg| Some(with_setting(cfg, key, value))).map(|_| ())
    }
}

/// `set_setting` on the device's file; failures are logged. Returns whether it worked.
pub fn set(key: &str, value: &str) -> bool {
    match SystemCfg::rocknix().set(key, value) {
        Ok(()) => true,
        Err(e) => {
            eprintln!("system.cfg: couldn't set {key}: {e}");
            false
        }
    }
}

/// The value of `key` in the device's file.
pub fn get(key: &str) -> Option<String> {
    SystemCfg::rocknix().get(key)
}

/// `SystemCfg::edit` on the device's file.
pub fn edit(change: impl FnOnce(&str) -> Option<String>) -> io::Result<bool> {
    SystemCfg::rocknix().edit(change)
}

/// The value of the first `key=` line of `cfg`.
pub fn value<'a>(cfg: &'a str, key: &str) -> Option<&'a str> {
    cfg.lines().find_map(|l| l.strip_prefix(key)?.strip_prefix('='))
}

/// ROCKNIX's `[[ "$1" =~ ^[[:alnum:]] ]]`, and no line breaks or `=` in the key.
fn valid_key(key: &str) -> bool {
    key.chars().next().is_some_and(|c| c.is_ascii_alphanumeric()) && !key.contains(['\n', '='])
}

/// `cfg` after `set_setting key value`.
pub fn with_setting(cfg: &str, key: &str, value: &str) -> String {
    let mut out: String = cfg.lines().filter(|l| !l.strip_prefix(key).is_some_and(|r| r.starts_with('='))).flat_map(|l| [l, "\n"]).collect();
    if value != "default" {
        out.push_str(key);
        out.push('=');
        out.push_str(value);
        out.push('\n');
    }
    out
}

/// The lock file while we hold it.
struct Held<'a> {
    path: &'a Path,
    id: String,
    /// `LockStyle::Flock`: the open file the lock is on (closing it lets go).
    flocked: Option<std::fs::File>,
}

impl<'a> Held<'a> {
    fn take(path: &'a Path, wait: Duration, style: LockStyle) -> io::Result<Held<'a>> {
        let id = std::process::id().to_string();
        let deadline = Instant::now() + wait;
        if style == LockStyle::Flock {
            return Held::flock(path, id, deadline);
        }
        loop {
            match std::fs::OpenOptions::new().write(true).create_new(true).open(path) {
                Ok(mut f) => {
                    // As `echo "$$" > lock`: the id and a line break.
                    let _ = writeln!(f, "{id}");
                    return Ok(Held { path, id, flocked: None });
                }
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                    if let Some(dead) = holder_gone(path) {
                        // Removed only if it still names the same dead process: another waiter may have taken it since.
                        if std::fs::read_to_string(path).is_ok_and(|t| t.trim() == dead) {
                            eprintln!("system.cfg: removed a lock left by process {dead}, which has ended");
                            let _ = std::fs::remove_file(path);
                        }
                        continue;
                    }
                    if Instant::now() >= deadline {
                        let who = std::fs::read_to_string(path).unwrap_or_default();
                        return Err(io::Error::new(io::ErrorKind::TimedOut, format!("{} is held by process {}", path.display(), who.trim())));
                    }
                    std::thread::sleep(POLL);
                }
                Err(e) => return Err(e),
            }
        }
    }
}

impl<'a> Held<'a> {
    /// ROCKNIX main's `wait_lock`: `exec {fd}>>lock; flock -x $fd`, here without blocking, tried until `deadline`.
    fn flock(path: &'a Path, id: String, deadline: Instant) -> io::Result<Held<'a>> {
        use std::os::unix::io::AsRawFd;
        let file = std::fs::OpenOptions::new().append(true).create(true).open(path)?;
        loop {
            // SAFETY: flock(2) on a descriptor this function owns.
            if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 {
                return Ok(Held { path, id, flocked: Some(file) });
            }
            let e = io::Error::last_os_error();
            if e.raw_os_error() != Some(libc::EWOULDBLOCK) && e.kind() != io::ErrorKind::Interrupted {
                return Err(e);
            }
            if Instant::now() >= deadline {
                return Err(io::Error::new(io::ErrorKind::TimedOut, format!("{} is locked (flock) by another program", path.display())));
            }
            std::thread::sleep(POLL);
        }
    }
}

impl Drop for Held<'_> {
    /// Like ROCKNIX's trap: only a lock that is still ours. A flock goes with its file, which stays.
    fn drop(&mut self) {
        if self.flocked.take().is_some() {
            return;
        }
        if std::fs::read_to_string(self.path).is_ok_and(|t| t.trim() == self.id) {
            let _ = std::fs::remove_file(self.path);
        }
    }
}

/// The process id in the lock, if that process has ended. An empty file is a lock being taken right now
/// (`noclobber` creates it, then `echo` writes the id): not gone.
fn holder_gone(lock: &Path) -> Option<String> {
    let text = std::fs::read_to_string(lock).ok()?;
    let pid: i32 = text.trim().parse().ok()?;
    if pid <= 0 {
        return None;
    }
    // ESRCH: no such process. EPERM means it exists.
    let gone = unsafe { libc::kill(pid, 0) } != 0 && io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH);
    gone.then(|| pid.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;
    use std::process::Command;

    fn tmp(name: &str) -> crate::testutil::TestDir {
        TestDir::new(&format!("syscfg-{name}"))
    }

    #[test]
    fn set_works_like_rocknixs_set_setting() {
        let cfg = "a=1\nwifi.key=old\nb=2\nwifi.key=older\n";
        assert_eq!(with_setting(cfg, "wifi.key", "new"), "a=1\nb=2\nwifi.key=new\n");
        assert_eq!(with_setting(cfg, "wifi.key", "default"), "a=1\nb=2\n");
        // A key that is a prefix of another is not that other key.
        assert_eq!(with_setting("wifi.keyx=1\n", "wifi.key", "2"), "wifi.keyx=1\nwifi.key=2\n");
        assert_eq!(with_setting("", "x", ""), "x=\n");
        assert_eq!(with_setting("no newline at end", "k", "v"), "no newline at end\nk=v\n");
        assert_eq!(value("a=1\nwifi.key=pass=word\n", "wifi.key"), Some("pass=word"));
        assert_eq!(value("a=1\n", "wifi.key"), None);
    }

    #[test]
    fn set_refuses_what_set_setting_would_not_write() {
        let d = tmp("refuse");
        let c = SystemCfg::at(&d.join("system.cfg"));
        assert!(c.set(".hidden", "1").is_err());
        assert!(c.set("", "1").is_err());
        assert!(c.set("a\nb", "1").is_err());
        assert!(c.set("a", "1\nevil=1").is_err());
        assert!(!c.path.exists());
    }

    #[test]
    fn the_device_file_uses_rocknixs_lock_and_others_a_lock_beside_them() {
        assert_eq!(SystemCfg::at(Path::new(PATH)).lock, Path::new(LOCK));
        assert_eq!(SystemCfg::at(Path::new("/x/system.cfg")).lock, Path::new("/x/system.cfg.lock"));
    }

    #[test]
    fn a_missing_file_starts_from_the_seed_and_the_lock_is_gone_afterwards() {
        let d = tmp("seed");
        std::fs::write(d.join("default.cfg"), "rocknix=1\n").unwrap();
        let mut c = SystemCfg::at(&d.join("system.cfg"));
        c.seed = Some(d.join("default.cfg"));
        c.set("wifi.enabled", "1").unwrap();
        assert_eq!(std::fs::read_to_string(&c.path).unwrap(), "rocknix=1\nwifi.enabled=1\n");
        assert!(!c.lock.exists());
        // Nothing to change: not written.
        assert!(!c.edit(|t| Some(t.to_string())).unwrap());
        assert!(!c.edit(|_| None).unwrap());
    }

    #[test]
    fn a_lock_held_by_a_live_process_is_waited_for_then_the_write_fails() {
        let d = tmp("held");
        let mut c = SystemCfg::at(&d.join("system.cfg"));
        c.wait = Duration::from_millis(300);
        // Process 1 (init) is always there.
        std::fs::write(&c.lock, "1\n").unwrap();
        let t = Instant::now();
        let e = c.set("a", "1").unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::TimedOut);
        assert!(t.elapsed() >= Duration::from_millis(300));
        assert!(!c.path.exists(), "nothing written without the lock");
        assert_eq!(std::fs::read_to_string(&c.lock).unwrap(), "1\n", "someone else's lock stays");
    }

    #[test]
    fn a_lock_taken_while_we_wait_is_honoured_and_used_after_its_release() {
        let d = tmp("release");
        let c = SystemCfg::at(&d.join("system.cfg"));
        std::fs::write(&c.lock, "1\n").unwrap();
        let lock = c.lock.clone();
        let release = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(200));
            std::fs::remove_file(lock).unwrap();
        });
        c.set("a", "1").unwrap();
        release.join().unwrap();
        assert_eq!(c.get("a").as_deref(), Some("1"));
    }

    #[test]
    fn a_lock_left_by_an_ended_process_is_removed() {
        let d = tmp("stale");
        let c = SystemCfg::at(&d.join("system.cfg"));
        let mut child = Command::new("true").spawn().unwrap();
        let pid = child.id();
        child.wait().unwrap();
        std::fs::write(&c.lock, format!("{pid}\n")).unwrap();
        c.set("a", "1").unwrap();
        assert_eq!(c.get("a").as_deref(), Some("1"));
        assert!(!c.lock.exists());
    }

    /// ROCKNIX main's `wait_lock`, `release_lock`, `del_setting` and `set_setting` (001-functions @ 8b70f69), paths in
    /// the test folder.
    fn rocknix_main_functions(cfg: &Path, lock: &Path) -> String {
        format!(
            r#"J_CONF="{cfg}"
J_CONF_LOCK="{lock}"
function wait_lock() {{
  # flock is dropped automatically if the script dies
  exec {{J_CONF_LOCK_FD}}>>"${{J_CONF_LOCK}}"
  flock -x "${{J_CONF_LOCK_FD}}"
}}

function release_lock() {{
  exec {{J_CONF_LOCK_FD}}>&-
}}

function del_setting() {{
  wait_lock
  if [[ "${{1}}" =~ ^[[:alnum:]] ]]
  then
    sed -i.sedtmp "/^${{1}}=/d" "${{J_CONF}}" && rm -f "${{J_CONF}}.sedtmp"
  fi
  release_lock
}}

function set_setting() {{
  if [[ "${{1}}" =~ ^[[:alnum:]] ]]
  then
    del_setting "${{1}}"
    if [ ! "${{2}}" = "default" ]
    then
      wait_lock
      echo "${{1}}=${{2}}" >> "${{J_CONF}}"
      release_lock
    fi
  fi
}}
"#,
            cfg = cfg.display(),
            lock = lock.display()
        )
    }

    #[test]
    fn the_lock_style_is_read_from_wait_lock() {
        let (cfg, lock) = (Path::new("/x/system.cfg"), Path::new("/x/lock"));
        assert!(!uses_flock(&rocknix_functions(cfg, lock)), "20261001");
        assert!(uses_flock(&rocknix_main_functions(cfg, lock)), "ROCKNIX main");
        assert!(!uses_flock("function wait_lock() {\n  sleep 1\n}\nfunction other() {\n  flock -x 3\n}\n"), "flock elsewhere");
        assert!(!uses_flock(""));
    }

    fn flock_cfg(d: &Path) -> SystemCfg {
        SystemCfg { style: LockStyle::Flock, wait: Duration::from_millis(300), ..SystemCfg::at(&d.join("system.cfg")) }
    }

    #[test]
    fn flock_style_waits_for_another_holder_and_leaves_the_file() {
        use std::os::unix::io::AsRawFd;
        let d = tmp("flock");
        let c = flock_cfg(&d);
        // Another program's flock (another open file description, so it counts even in this process).
        let other = std::fs::OpenOptions::new().append(true).create(true).open(&c.lock).unwrap();
        assert_eq!(unsafe { libc::flock(other.as_raw_fd(), libc::LOCK_EX) }, 0);
        let e = c.set("a", "1").unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::TimedOut);
        assert!(!c.path.exists(), "nothing written without the lock");
        drop(other);
        c.set("a", "1").unwrap();
        assert_eq!(c.get("a").as_deref(), Some("1"));
        assert!(c.lock.exists(), "the lock file stays, as ROCKNIX's flock leaves it");
        c.set("b", "2").unwrap();
        assert_eq!(c.read(), "a=1\nb=2\n", "an existing lock file is no obstacle");
    }

    #[test]
    fn flock_style_loses_nothing_beside_rocknix_mains_set_setting() {
        if !Command::new("sh").args(["-c", "command -v flock && command -v bash"]).output().is_ok_and(|o| o.status.success()) {
            eprintln!("skipped: no flock or bash here (scripts/test-linux.sh runs it)");
            return;
        }
        let d = tmp("flock-race");
        let c = SystemCfg { wait: WAIT, ..flock_cfg(&d) };
        std::fs::write(&c.path, "").unwrap();
        let script = format!("{}\nfor i in $(seq 1 12); do set_setting \"shell.k$i\" \"$i\"; done\n", rocknix_main_functions(&c.path, &c.lock));
        let shell = std::thread::spawn(move || Command::new("bash").arg("-c").arg(script).status().unwrap());
        for i in 1..=12 {
            c.set(&format!("ours.k{i}"), &i.to_string()).unwrap();
        }
        assert!(shell.join().unwrap().success());
        let text = c.read();
        for i in 1..=12 {
            assert!(text.contains(&format!("ours.k{i}={i}\n")) && text.contains(&format!("shell.k{i}={i}\n")), "lost a write: {text}");
        }
    }

    /// ROCKNIX's `wait_lock`, `del_setting` and `set_setting` (20261001, rocknix/profile.d/001-functions), with the
    /// paths pointed at the test folder. bash, as on the device.
    fn rocknix_functions(cfg: &Path, lock: &Path) -> String {
        format!(
            r#"J_CONF="{cfg}"
J_CONF_LOCK="{lock}"
function wait_lock() {{
  while true
  do
    if (set -o noclobber; echo "$$" > "${{J_CONF_LOCK}}") 2>/dev/null
    then
      trap 'rc=$?; [ "$(cat "${{J_CONF_LOCK}}" 2>/dev/null)" = "$$" ] && rm -f "${{J_CONF_LOCK}}"; exit "${{rc}}"' INT TERM EXIT
      break
    else
     sleep 1
    fi
  done
}}
function del_setting() {{
  wait_lock
  if [[ "${{1}}" =~ ^[[:alnum:]] ]]
  then
    sed -i.sedtmp "/^${{1}}=/d" "${{J_CONF}}" && rm -f "${{J_CONF}}.sedtmp"
  fi
  rm -f "${{J_CONF_LOCK}}"
}}
function set_setting() {{
  if [[ "${{1}}" =~ ^[[:alnum:]] ]]
  then
    del_setting "${{1}}"
    if [ ! "${{2}}" = "default" ]
    then
      wait_lock
      echo "${{1}}=${{2}}" >> "${{J_CONF}}"
      rm -f "${{J_CONF_LOCK}}"
    fi
  fi
}}
"#,
            cfg = cfg.display(),
            lock = lock.display()
        )
    }

    #[test]
    fn concurrent_writers_here_and_in_rocknixs_shell_lose_nothing() {
        let d = tmp("concurrent");
        let c = SystemCfg::at(&d.join("system.cfg"));
        std::fs::write(&c.path, "start=1\n").unwrap();
        // runemu.sh's side: ROCKNIX's own set_setting, twelve keys one after another, in a separate process.
        let script = format!("{}\nfor i in $(seq 1 12); do set_setting \"shell.k$i\" \"$i\"; done\n", rocknix_functions(&c.path, &c.lock));
        let shell = Command::new("bash").arg("-c").arg(script).spawn().unwrap();
        // pocket-ui's side: four threads, twelve read-modify-writes each, at the same time.
        let threads: Vec<_> = (0..4)
            .map(|t| {
                let c = c.clone();
                std::thread::spawn(move || {
                    for i in 1..=12 {
                        c.set(&format!("ui{t}.k{i}"), &i.to_string()).unwrap();
                        // A per-game line written the way emu_settings writes it (replaced in place).
                        c.edit(|cfg| Some(format!("{cfg}game{t}[\"x.iso\"].k{i}=1\n"))).unwrap();
                    }
                })
            })
            .collect();
        for t in threads {
            t.join().unwrap();
        }
        let status = shell.wait_with_output().unwrap().status;
        assert!(status.success());
        let cfg = std::fs::read_to_string(&c.path).unwrap();
        assert!(cfg.contains("start=1\n"));
        for i in 1..=12 {
            assert_eq!(value(&cfg, &format!("shell.k{i}")), Some(i.to_string().as_str()), "the shell's key {i} in:\n{cfg}");
            for t in 0..4 {
                assert_eq!(value(&cfg, &format!("ui{t}.k{i}")), Some(i.to_string().as_str()), "key ui{t}.k{i}");
                assert!(cfg.contains(&format!("game{t}[\"x.iso\"].k{i}=1\n")));
            }
        }
        assert_eq!(cfg.lines().count(), 1 + 12 + 4 * 12 * 2);
        assert!(!c.lock.exists(), "everyone let go of the lock");
    }
}
