//! `net.log`, the logger's fifth stream (`logs.rs`): what the network daemons said this boot. ROCKNIX keeps the journal
//! in RAM, so after a reboot nobody could tell why a saved Wi-Fi was missing (2026-10-27).
//!
//! * **Journal.** One `journalctl -f -o short-monotonic -u NetworkManager -u iwd -u tailscaled -u wpa_supplicant -u
//!   connman` child (a unit that isn't there matches nothing), `-b -n all` so the boot so far comes first. Its pipe is
//!   read without blocking on the logger's tick, so the logger never waits for it and no thread is added. A child
//!   that dies is started again after 5 s, doubling up to 5 min; a restart replays the boot, and what is already on the
//!   card (the newest time stamp in the file) is skipped.
//! * **Snapshots.** At the start and whenever it changes (checked every [`CHECK_EVERY`] by a `read_dir`, no
//!   process): the names of the saved NetworkManager connections (`nmcli -t -f NAME,UUID,TYPE con show`, run only
//!   then) and the *file names* in iwd's folder (`/storage/.cache/iwd`). Never the content of any file: they hold
//!   the passwords.
//! * **Secrets.** The daemons don't log passwords at their default level; [`scrub`] still blanks `psk=`/`password=`-like
//!   values, key dumps, Tailscale auth keys and sign-in links before a line is written. Report a problem cleans the
//!   file again like all the others (`report.rs`: Wi-Fi names, MACs, addresses).

use std::collections::BTreeSet;
use std::fs;
use std::io::Read;
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// How often the folders are looked at for a change.
pub const CHECK_EVERY: Duration = Duration::from_secs(5);
const RESTART_MIN: Duration = Duration::from_secs(5);
const RESTART_MAX: Duration = Duration::from_secs(300);
/// A child that ran this long was healthy: the next failure starts the back-off anew.
const HEALTHY: Duration = Duration::from_secs(60);
const NMCLI_WAIT: Duration = Duration::from_secs(3);
/// One read of the pipe at most; the rest follows on the next ones.
const READ_MAX: usize = 256 * 1024;
const UNITS: [&str; 5] = ["NetworkManager", "iwd", "tailscaled", "wpa_supplicant", "connman"];

/// Where the net log reads from; tests point these at fakes.
#[derive(Debug, Clone)]
pub struct NetSources {
    /// The journal follower (program and arguments); empty: none.
    pub journal: Vec<String>,
    /// The saved NetworkManager connections (a folder: only the file names are looked at).
    pub nm_dir: PathBuf,
    /// `nmcli -t -f NAME,UUID,TYPE con show` (program and arguments); empty: none.
    pub nmcli: Vec<String>,
    /// iwd's folder of known networks (only the file names are looked at).
    pub iwd_dir: PathBuf,
}

impl NetSources {
    pub fn system() -> NetSources {
        let mut journal: Vec<String> = ["journalctl", "-b", "-q", "--no-pager", "-f", "-n", "all", "-o", "short-monotonic"]
            .map(String::from)
            .into();
        for u in UNITS {
            journal.extend(["-u".into(), u.into()]);
        }
        NetSources {
            journal,
            nm_dir: "/storage/.config/NetworkManager/system-connections".into(),
            nmcli: ["nmcli", "-t", "-f", "NAME,UUID,TYPE", "con", "show"].map(String::from).into(),
            iwd_dir: "/storage/.cache/iwd".into(),
        }
    }

    pub fn none() -> NetSources {
        NetSources {
            journal: Vec::new(),
            nm_dir: PathBuf::new(),
            nmcli: Vec::new(),
            iwd_dir: PathBuf::new(),
        }
    }
}

pub struct NetFeed {
    src: NetSources,
    child: Option<Running>,
    partial: Vec<u8>,
    /// Lines up to this time stamp (µs) are already in the file: skipped while the replay of the boot goes by.
    replay_until: Option<u64>,
    restart_at: Instant,
    backoff: Duration,
    spawn_error_logged: bool,
    next_check: Instant,
    nm_names: Option<BTreeSet<String>>,
    nm_snapshot: Option<String>,
    nm_retry_at: Option<Instant>,
    iwd_names: Option<BTreeSet<String>>,
}

struct Running {
    child: Child,
    started: Instant,
}

impl NetFeed {
    /// `last_usec`: the newest time stamp already in this boot's `net.log`.
    pub fn new(src: NetSources, last_usec: Option<u64>, now: Instant) -> NetFeed {
        NetFeed {
            src,
            child: None,
            partial: Vec::new(),
            replay_until: last_usec,
            restart_at: now,
            backoff: RESTART_MIN,
            spawn_error_logged: false,
            next_check: now,
            nm_names: None,
            nm_snapshot: None,
            nm_retry_at: None,
            iwd_names: None,
        }
    }

    /// The text (whole lines) that is new since the last call.
    pub fn poll(&mut self, now: Instant, uptime: f64) -> String {
        let mut out = String::new();
        self.read_journal(now, uptime, &mut out);
        if now >= self.next_check {
            self.next_check = now + CHECK_EVERY;
            self.snapshots(now, uptime, &mut out);
        }
        out
    }

    fn read_journal(&mut self, now: Instant, uptime: f64, out: &mut String) {
        if self.src.journal.is_empty() {
            return;
        }
        if self.child.is_none() {
            if now < self.restart_at {
                return;
            }
            match self.spawn() {
                Ok(c) => {
                    self.child = Some(Running { child: c, started: now });
                    self.spawn_error_logged = false;
                }
                Err(e) => {
                    if !self.spawn_error_logged {
                        out.push_str(&format!("[{uptime:>9.3}] --- journal not readable: {e} ---\n"));
                        self.spawn_error_logged = true;
                    }
                    self.restart_at = now + self.backoff;
                    self.backoff = (self.backoff * 2).min(RESTART_MAX);
                    return;
                }
            }
        }
        let Some(run) = self.child.as_mut() else { return };
        let started = run.started;
        let mut eof = false;
        let mut got = 0;
        let mut buf = [0u8; 16 * 1024];
        while got < READ_MAX {
            let Some(pipe) = run.child.stdout.as_mut() else { break };
            match pipe.read(&mut buf) {
                Ok(0) => {
                    eof = true;
                    break;
                }
                Ok(n) => {
                    got += n;
                    self.partial.extend_from_slice(&buf[..n]);
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(_) => {
                    eof = true;
                    break;
                }
            }
        }
        while let Some(i) = self.partial.iter().position(|&b| b == b'\n') {
            let line: Vec<u8> = self.partial.drain(..=i).collect();
            if let Some(l) = self.accept(&String::from_utf8_lossy(&line[..line.len() - 1])) {
                out.push_str(&l);
                out.push('\n');
            }
        }
        if self.partial.len() > 64 * 1024 {
            self.partial.clear();
        }
        if eof {
            let ran = started.elapsed();
            let status = self.child.as_mut().map_or("?".to_string(), |r| r.child.wait().map_or("?".to_string(), |s| s.to_string()));
            self.child = None;
            self.partial.clear();
            self.backoff = if ran >= HEALTHY { RESTART_MIN } else { (self.backoff * 2).min(RESTART_MAX) };
            self.restart_at = now + self.backoff;
            out.push_str(&format!("[{uptime:>9.3}] --- journal follower ended ({status}), starting again in {} s ---\n", self.backoff.as_secs()));
        }
    }

    /// One journal line, ready for the file; None for the ones to drop (already on the card, the journal's own notes).
    fn accept(&mut self, line: &str) -> Option<String> {
        if line.trim().is_empty() || line.starts_with("-- ") {
            return None;
        }
        if let (Some(until), Some(t)) = (self.replay_until, stamp_usec(line)) {
            if t <= until {
                return None;
            }
            self.replay_until = None;
        }
        Some(scrub(line))
    }

    fn spawn(&self) -> std::io::Result<Child> {
        let (prog, args) = self.src.journal.split_first().expect("journal command checked by the caller");
        let child = Command::new(prog).args(args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn()?;
        if let Some(out) = &child.stdout {
            // SAFETY: fcntl on the pipe this child object owns.
            unsafe {
                let fd = out.as_raw_fd();
                let flags = libc::fcntl(fd, libc::F_GETFL);
                libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK);
            }
        }
        Ok(child)
    }

    fn snapshots(&mut self, now: Instant, uptime: f64, out: &mut String) {
        let nm = file_names(&self.src.nm_dir);
        let nm_changed = self.nm_names.as_ref() != Some(&nm);
        let retry = self.nm_retry_at.is_some_and(|t| now >= t);
        if (nm_changed || retry) && !self.src.nmcli.is_empty() {
            self.nm_names = Some(nm);
            self.nm_retry_at = None;
            let (prog, args) = self.src.nmcli.split_first().expect("not empty");
            match crate::proc::run_with_timeout(Command::new(prog).args(args), NMCLI_WAIT) {
                Ok(o) if o.status.success() => {
                    let text = String::from_utf8_lossy(&o.stdout).trim().to_string();
                    if self.nm_snapshot.as_deref() != Some(&text) {
                        out.push_str(&format!("[{uptime:>9.3}] --- saved connections (nmcli) ---\n"));
                        for l in text.lines() {
                            out.push_str(&format!("[{uptime:>9.3}]   {}\n", scrub(l)));
                        }
                        self.nm_snapshot = Some(text);
                    }
                }
                other => {
                    // NetworkManager may just not be up yet: look again in a minute, and say it once.
                    if self.nm_snapshot.is_none() {
                        let why = match other {
                            Ok(o) => format!("exit {}", o.status),
                            Err(e) => e.to_string(),
                        };
                        out.push_str(&format!("[{uptime:>9.3}] --- saved connections: nmcli gave nothing ({why}) ---\n"));
                        self.nm_snapshot = Some(String::new());
                    }
                    self.nm_retry_at = Some(now + Duration::from_secs(60));
                }
            }
        } else if nm_changed {
            self.nm_names = Some(nm);
        }
        let iwd = file_names(&self.src.iwd_dir);
        if self.iwd_names.as_ref() != Some(&iwd) {
            let list: Vec<&str> = iwd.iter().map(String::as_str).collect();
            out.push_str(&format!("[{uptime:>9.3}] --- iwd known networks ({}): {} ---\n", list.len(), list.join(", ")));
            self.iwd_names = Some(iwd);
        }
    }

    /// Stops the follower.
    pub fn stop(&mut self) {
        if let Some(mut r) = self.child.take() {
            let _ = r.child.kill();
            let _ = r.child.wait();
        }
    }
}

impl Drop for NetFeed {
    fn drop(&mut self) {
        self.stop();
    }
}

/// The names (never the content) of what is in `dir`; empty where there is no such folder.
fn file_names(dir: &Path) -> BTreeSet<String> {
    fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| !n.starts_with('.'))
        .collect()
}

/// The time stamp (µs) of a `short-monotonic` line: `[ 5140.900432] host unit[pid]: text`.
pub fn stamp_usec(line: &str) -> Option<u64> {
    let inner = line.strip_prefix('[')?.split_once(']')?.0.trim();
    let (secs, micros) = inner.split_once('.')?;
    Some(secs.trim().parse::<u64>().ok()? * 1_000_000 + micros.parse::<u64>().ok()?)
}

const SECRET_KEYS: [&str; 9] = ["psk", "passphrase", "password", "passwd", "pmk", "ptk", "gtk", "authkey", "secret"];
const HIDDEN: &str = "[hidden]";

/// Blanks what must never reach the card: `psk=…`/`password: …`-like values, whole lines that dump key bytes, Tailscale
/// auth keys (`tskey-…`) and sign-in links (`…/a/<code>`).
pub fn scrub(line: &str) -> String {
    let lower = line.to_ascii_lowercase();
    if lower.contains("hexdump") && SECRET_KEYS.iter().chain(["key"].iter()).any(|k| lower.contains(k)) {
        let at = lower.find("hexdump").unwrap_or(0);
        let keep = line[at..].find("): ").map_or(at, |i| at + i + 3);
        return format!("{}[key dump hidden]", &line[..keep]);
    }
    let mut out = String::with_capacity(line.len());
    for (i, tok) in line.split(' ').enumerate() {
        if i > 0 {
            out.push(' ');
        }
        out.push_str(&scrub_token(tok));
    }
    // "password: hunter2" / "psk = x": the value is the next word.
    let words: Vec<&str> = out.split(' ').collect();
    let mut fixed: Vec<String> = Vec::with_capacity(words.len());
    let mut hide_next = false;
    for w in words {
        let wl = w.to_ascii_lowercase();
        if hide_next && !w.is_empty() {
            let w = if w == "=" || w == ":" { w } else { HIDDEN };
            hide_next = w == "=" || w == ":";
            fixed.push(w.to_string());
            continue;
        }
        hide_next = SECRET_KEYS.iter().any(|k| wl.trim_end_matches([':', '=']) == *k) && (wl.ends_with(':') || wl.ends_with('='));
        fixed.push(w.to_string());
    }
    fixed.join(" ")
}

fn scrub_token(tok: &str) -> String {
    let lower = tok.to_ascii_lowercase();
    if lower.contains("tskey-") {
        let at = lower.find("tskey-").unwrap();
        return format!("{}{HIDDEN}", &tok[..at]);
    }
    if let Some(at) = lower.find("login.tailscale.com/a/") {
        return format!("{}{HIDDEN}", &tok[..at + "login.tailscale.com/a/".len()]);
    }
    for sep in ['=', ':'] {
        if let Some((k, v)) = tok.split_once(sep) {
            let key = k.trim_matches(|c: char| !c.is_ascii_alphanumeric()).to_ascii_lowercase();
            if !v.is_empty() && SECRET_KEYS.iter().any(|s| key == *s || key.ends_with(&format!("_{s}")) || key.ends_with(&format!("-{s}"))) {
                return format!("{k}{sep}{HIDDEN}");
            }
        }
    }
    tok.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;
    use std::fs;

    fn sh(script: &str) -> Vec<String> {
        vec!["sh".into(), "-c".into(), script.into()]
    }

    fn feed(name: &str, script: &str) -> (TestDir, NetFeed) {
        let d = TestDir::new(&format!("netlog-{name}"));
        fs::create_dir_all(&d).unwrap();
        fs::create_dir_all(d.join("nm")).unwrap();
        fs::create_dir_all(d.join("iwd")).unwrap();
        let src = NetSources {
            journal: sh(script),
            nm_dir: d.join("nm"),
            nmcli: sh("printf 'Heim:1111-2222:802-11-wireless\\nlo:3333:loopback\\n'"),
            iwd_dir: d.join("iwd"),
        };
        let f = NetFeed::new(src, None, Instant::now());
        (d, f)
    }

    /// Polls until `done(text)` or two seconds are over.
    fn collect(f: &mut NetFeed, done: impl Fn(&str) -> bool) -> String {
        let mut all = String::new();
        let t0 = Instant::now();
        while t0.elapsed() < Duration::from_secs(2) && !done(&all) {
            all += &f.poll(Instant::now(), 12.0);
            std::thread::sleep(Duration::from_millis(20));
        }
        all
    }

    #[test]
    fn the_journal_lines_arrive_and_a_dead_follower_is_said_and_restarted() {
        let (_d, mut f) = feed("lines", "printf '[    1.000000] host NetworkManager[1]: <info> device (wlan0): state change\\n-- No entries --\\n[    2.000000] host iwd[2]: Connected\\n'");
        let text = collect(&mut f, |t| t.contains("starting again"));
        assert!(text.contains("[    1.000000] host NetworkManager[1]: <info> device (wlan0): state change\n"), "{text}");
        assert!(text.contains("host iwd[2]: Connected\n"), "{text}");
        assert!(!text.contains("No entries"), "the journal's own notes are dropped: {text}");
        assert!(text.contains("journal follower ended"), "{text}");
    }

    #[test]
    fn a_replay_after_a_restart_does_not_write_a_line_twice() {
        let d = TestDir::new("netlog-replay");
        fs::create_dir_all(&d).unwrap();
        let src = NetSources {
            journal: sh("printf '[    1.000000] a\\n[    2.000000] b\\n[    3.000000] c\\n'"),
            ..NetSources::none()
        };
        let mut f = NetFeed::new(src, Some(2_000_000), Instant::now());
        let text = collect(&mut f, |t| t.contains("ended"));
        assert!(text.contains("] c\n") && !text.contains("] a\n") && !text.contains("] b\n"), "{text}");
    }

    #[test]
    fn no_journalctl_is_said_once() {
        let src = NetSources {
            journal: vec!["/nonexistent/journalctl".into()],
            ..NetSources::none()
        };
        let mut f = NetFeed::new(src, None, Instant::now());
        let t0 = Instant::now();
        let a = f.poll(t0, 1.0);
        assert!(a.contains("journal not readable"), "{a}");
        assert_eq!(f.poll(t0 + Duration::from_secs(6), 2.0), "", "said once");
    }

    #[test]
    fn snapshots_list_names_only_and_come_again_only_on_a_change() {
        let (d, mut f) = feed("snap", "sleep 5");
        fs::write(d.join("nm/Heim.nmconnection"), "psk=geheim\n").unwrap();
        fs::write(d.join("iwd/Heim.psk"), "[Security]\nPassphrase=geheim\n").unwrap();
        let t0 = Instant::now();
        let a = f.poll(t0, 3.0);
        assert!(a.contains("saved connections (nmcli)") && a.contains("Heim:1111-2222:802-11-wireless"), "{a}");
        assert!(a.contains("iwd known networks (1): Heim.psk"), "{a}");
        assert!(!a.contains("geheim"), "never the content: {a}");
        assert_eq!(f.poll(t0 + Duration::from_secs(6), 4.0), "", "nothing changed");
        fs::write(d.join("iwd/Cafe.open"), "").unwrap();
        let b = f.poll(t0 + Duration::from_secs(12), 5.0);
        assert!(b.contains("iwd known networks (2): Cafe.open, Heim.psk"), "{b}");
        assert!(!b.contains("nmcli"), "NetworkManager's list is untouched: {b}");
    }

    #[test]
    fn a_missing_nmcli_is_said_once_and_tried_again_later() {
        let (d, mut f) = feed("nonm", "sleep 5");
        f.src.nmcli = sh("exit 1");
        let t0 = Instant::now();
        let a = f.poll(t0, 1.0);
        assert!(a.contains("nmcli gave nothing"), "{a}");
        f.src.nmcli = sh("printf 'Heim:1:802-11-wireless\\n'");
        let b = f.poll(t0 + Duration::from_secs(70), 2.0);
        assert!(b.contains("Heim:1:802-11-wireless"), "{b}");
        let _ = d;
    }

    #[test]
    fn secrets_are_blanked_before_a_line_is_written() {
        let cases = [
            ("[ 1.0] iwd[2]: psk=0123456789abcdef", "[ 1.0] iwd[2]: psk=[hidden]"),
            ("[ 1.0] nm[2]: Password: hunter2 for wlan0", "[ 1.0] nm[2]: Password: [hidden] for wlan0"),
            (concat!("[ 1.0] tailscaled[3]: authkey tsk", "ey-auth-abc123-xyz"), "[ 1.0] tailscaled[3]: authkey [hidden]"),
            ("[ 1.0] tailscaled[3]: To authenticate, visit: https://login.tailscale.com/a/1a2b3c4d", "[ 1.0] tailscaled[3]: To authenticate, visit: https://login.tailscale.com/a/[hidden]"),
            ("[ 1.0] wpa_supplicant[4]: PMK - hexdump(len=32): 00 11 22", "[ 1.0] wpa_supplicant[4]: PMK - hexdump(len=32): [key dump hidden]"),
            ("[ 1.0] NetworkManager[1]: <info> device (wlan0): state change: config -> ip-config", "[ 1.0] NetworkManager[1]: <info> device (wlan0): state change: config -> ip-config"),
        ];
        for (inp, want) in cases {
            assert_eq!(scrub(inp), want);
        }
    }
}
