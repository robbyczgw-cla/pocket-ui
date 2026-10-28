//! ROCKNIX's own volume and brightness box. `input_sense` answers every Vol and HOME + Vol press with
//! `mako-notify "Volume: 25%" -no-es`: a black monospace box at the top of the screen, drawn by the `mako` daemon over
//! everything. `mako-notify` leaves it out only when EmulationStation is in front, and pocket-ui's `app_id` is not
//! that, so the box landed on top of pocket-ui's own display (`osd.rs`).
//!
//! We do not pose as EmulationStation. mako has modes instead: a criteria section scoped to a mode,
//!
//! ```text
//! # BEGIN pocket-ui
//! [mode=pocket-ui app-name=mako-notify]
//! invisible=1
//! # END pocket-ui
//! ```
//!
//! is appended once to `/storage/.config/mako/config` (the file `mako-notify` creates when it is missing and never
//! rewrites; each call only runs `makoctl reload`, which keeps the modes: they are the daemon's state, not the
//! config's). While the mode `pocket-ui` is on, the notifications of `mako-notify` are not shown. It is on while
//! pocket-ui or pocket-core is in front (pocket-ui draws the display, or the overlay in pocket-core), and off while
//! RetroArch, a standalone emulator or an app is in front: there we draw nothing, and ROCKNIX's box is the only
//! feedback. Screenshots ("Screenshot Saved", also from `mako-notify`) are hidden along with it while the mode is on.
//!
//! `set_hidden` is called from the UI thread; the `makoctl` runs happen on threads, the last wish wins. The mode lives
//! in the daemon: when mako restarts (sway reloading its config), it is off until the next call, and the box shows
//! until the next game start or end. To undo: delete the lines from `# BEGIN pocket-ui` to `# END pocket-ui` in the
//! config and `makoctl mode -r pocket-ui; makoctl reload` (`deploy/package/uninstall.sh` does it).

use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

pub const CONFIG: &str = "/storage/.config/mako/config";
/// `mako-notify` is what `input_sense` calls; without it there is no box to hide.
const NOTIFY: &str = "/usr/bin/mako-notify";
pub const MODE: &str = "pocket-ui";
const BEGIN: &str = "# BEGIN pocket-ui";
const END: &str = "# END pocket-ui";
/// The notification's `app_name` (what `mako-notify` sends over D-Bus).
const APP_NAME: &str = "mako-notify";
/// Where the session bus is on ROCKNIX when the environment doesn't say (`/etc/profile.d/050-sway.conf`).
const RUNTIME_DIR: &str = "/var/run/0-runtime-dir";

fn block() -> String {
    format!(
        "{BEGIN} (hides ROCKNIX's volume and brightness box while pocket-ui draws its own; removed by uninstall.sh)\n\
         [mode={MODE} app-name={APP_NAME}]\ninvisible=1\n{END}\n"
    )
}

/// `text` without pocket-ui's block (a block without its end is cut to the end of the file).
pub fn without_block(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut inside = false;
    for line in text.split_inclusive('\n') {
        if line.starts_with(BEGIN) {
            inside = true;
        }
        if !inside {
            out.push_str(line);
        } else if line.starts_with(END) {
            inside = false;
        }
    }
    out
}

/// `text` with pocket-ui's block as it is now at the end; None when it already is exactly that (idempotent).
pub fn with_block(text: &str) -> Option<String> {
    let mut base = without_block(text);
    if !base.is_empty() && !base.ends_with('\n') {
        base.push('\n');
    }
    base.push_str(&block());
    (base != text).then_some(base)
}

/// Puts the block into mako's config and tells mako. Slow (a process): call it from a thread. Does nothing without
/// `mako-notify`; a missing config is left to `mako-notify` (it writes its default at the first box), the next start
/// of pocket-ui tries again. Returns whether the file changed.
pub fn ensure() -> Result<bool, String> {
    if !Path::new(NOTIFY).exists() {
        return Ok(false);
    }
    let changed = ensure_in(Path::new(CONFIG))?;
    if changed {
        makoctl(Path::new("makoctl"), &["reload"]).map_err(|e| format!("wrote the config, but {e}"))?;
    }
    Ok(changed)
}

fn ensure_in(config: &Path) -> Result<bool, String> {
    let text = std::fs::read_to_string(config).map_err(|e| format!("no {} yet ({e}): tried again at the next start", config.display()))?;
    let Some(new) = with_block(&text) else { return Ok(false) };
    crate::fsutil::write_atomic(config, new.as_bytes()).map_err(|e| format!("couldn't write {}: {e}", config.display()))?;
    Ok(true)
}

fn makoctl(bin: &Path, args: &[&str]) -> Result<(), String> {
    let mut cmd = Command::new(bin);
    cmd.args(args).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    if std::env::var_os("XDG_RUNTIME_DIR").is_none() {
        cmd.env("XDG_RUNTIME_DIR", RUNTIME_DIR);
    }
    match cmd.status() {
        Ok(s) if s.success() => Ok(()),
        Ok(s) => Err(format!("makoctl {} failed ({s})", args.join(" "))),
        Err(e) => Err(format!("makoctl {}: {e}", args.join(" "))),
    }
}

fn set_mode(bin: &Path, hidden: bool) -> Result<(), String> {
    makoctl(bin, &["mode", if hidden { "-a" } else { "-r" }, MODE])
}

static WANT: AtomicBool = AtomicBool::new(false);
/// One `makoctl` at a time, so that two quick changes arrive in order.
static APPLY: Mutex<()> = Mutex::new(());
static COMPLAINED: AtomicBool = AtomicBool::new(false);

/// Hides (`true`) or shows ROCKNIX's box from now on. Returns at once; the work runs on a thread. Does nothing on a
/// system without `mako-notify`.
pub fn set_hidden(hidden: bool) {
    WANT.store(hidden, Ordering::SeqCst);
    if !Path::new(NOTIFY).exists() {
        return;
    }
    std::thread::spawn(|| {
        let _one_at_a_time = APPLY.lock().unwrap_or_else(|e| e.into_inner());
        // Whatever was wanted last, not what this call asked for.
        if let Err(e) = set_mode(Path::new("makoctl"), WANT.load(Ordering::SeqCst)) {
            if !COMPLAINED.swap(true, Ordering::Relaxed) {
                eprintln!("mako: {e}");
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;
    use std::os::unix::fs::PermissionsExt;

    const ROCKNIX_DEFAULT: &str = "max-visible=1\nlayer=overlay\nfont=monospace 30\nanchor=top-center\nwidth=500\n";

    #[test]
    fn the_block_is_added_once_and_goes_again() {
        let with = with_block(ROCKNIX_DEFAULT).unwrap();
        assert!(with.starts_with(ROCKNIX_DEFAULT), "ROCKNIX's lines stay as they were");
        assert!(with.contains("[mode=pocket-ui app-name=mako-notify]\ninvisible=1\n"));
        assert_eq!(with_block(&with), None, "idempotent");
        assert_eq!(without_block(&with), ROCKNIX_DEFAULT, "the undo restores the file exactly");
        assert_eq!(without_block(ROCKNIX_DEFAULT), ROCKNIX_DEFAULT);
    }

    #[test]
    fn a_file_without_final_newline_or_with_an_old_block_is_repaired() {
        let with = with_block("width=500").unwrap();
        assert!(with.starts_with("width=500\n# BEGIN pocket-ui"));
        let old = format!("a=1\n{BEGIN} old\n[mode=x]\ninvisible=0\n{END}\nb=2\n");
        let new = with_block(&old).unwrap();
        assert_eq!(new.matches(BEGIN).count(), 1);
        assert!(new.starts_with("a=1\nb=2\n") && !new.contains("invisible=0"));
        // No block yet in an empty file.
        assert_eq!(with_block("").unwrap(), block());
    }

    #[test]
    fn the_config_is_changed_in_place_and_a_missing_one_is_left_to_mako_notify() {
        let dir = TestDir::new("mako");
        let cfg = dir.join("mako/config");
        assert!(ensure_in(&cfg).unwrap_err().contains("no "), "mako-notify makes the file at the first box");
        assert!(!cfg.exists());
        std::fs::create_dir_all(cfg.parent().unwrap()).unwrap();
        std::fs::write(&cfg, ROCKNIX_DEFAULT).unwrap();
        assert!(ensure_in(&cfg).unwrap());
        assert!(!ensure_in(&cfg).unwrap(), "the second start changes nothing");
        assert_eq!(without_block(&std::fs::read_to_string(&cfg).unwrap()), ROCKNIX_DEFAULT);
    }

    #[test]
    fn the_mode_is_added_and_removed_with_makoctl() {
        let dir = TestDir::new("makoctl");
        let bin = dir.join("makoctl");
        let log = dir.join("calls");
        std::fs::write(&bin, format!("#!/bin/sh\necho \"$*\" >>{}\n[ \"$1\" != fail ]\n", log.display())).unwrap();
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        set_mode(&bin, true).unwrap();
        set_mode(&bin, false).unwrap();
        assert_eq!(std::fs::read_to_string(&log).unwrap(), "mode -a pocket-ui\nmode -r pocket-ui\n");
        assert!(makoctl(&bin, &["fail"]).is_err());
        assert!(set_mode(&dir.join("nothing"), true).is_err(), "no makoctl: an error, no panic");
    }
}
