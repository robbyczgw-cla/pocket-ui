//! Remote access over SSH, switched in Settings (docs/SECURITY-HARDENING.md, B2 in
//! the internal review). Off unless the user turns it on; then root can log in with this
//! device's password (shown on screen, same kind as the file sharing password) or a key from
//! /storage/.ssh/authorized_keys. It turns itself off after 30 minutes without an SSH connection,
//! and after a restart. A developer device keeps SSH on for good with the `dev-ssh` marker file.

pub const IDLE_LIMIT_SECS: u64 = 30 * 60;
pub const SSH_PORT: u16 = 22;
/// Present on development devices: SSH stays on, the switch and the timeout leave it alone.
pub const DEV_MARKER: &str = "/storage/.config/pocket-ui/dev-ssh";
pub const PASSWORD_PATH: &str = "/storage/.config/pocket-ui/remote/password";
/// Exists while SSH runs because of our switch; on /run, so it is gone after a restart.
pub const OWNED_MARKER: &str = "/run/pocket-ui-ssh-on";
/// ROCKNIX's sshd.service only starts while this file exists (ConditionPathExists).
pub const SERVICE_FLAG: &str = "/storage/.cache/services/sshd.conf";

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Mode {
    Off,
    /// Turned on with our switch; the idle timeout applies.
    On,
    /// `dev-ssh` marker: always on, never touched by pocket-ui.
    Dev,
    /// sshd runs, but not because of our switch (ROCKNIX's `ssh.enabled`, the kernel command line).
    External,
}

pub fn mode(running: bool, owned: bool, dev: bool) -> Mode {
    match (dev, running, owned) {
        (true, _, _) => Mode::Dev,
        (false, true, true) => Mode::On,
        (false, true, false) => Mode::External,
        (false, false, _) => Mode::Off,
    }
}

/// Idle timeout: SSH started by our switch goes off after `IDLE_LIMIT_SECS` without a session.
pub struct IdleTimer {
    last_busy: u64,
}

impl IdleTimer {
    pub fn new(now: u64) -> Self {
        Self { last_busy: now }
    }

    /// Called about once a minute with the current mode and number of open SSH connections;
    /// true means: turn SSH off now.
    pub fn tick(&mut self, now: u64, mode: Mode, sessions: usize) -> bool {
        if mode != Mode::On || sessions > 0 {
            self.last_busy = now;
            return false;
        }
        now.saturating_sub(self.last_busy) >= IDLE_LIMIT_SECS
    }
}

#[cfg(target_os = "linux")]
mod imp {
    use super::*;
    use std::path::Path;
    use std::process::Command;

    fn sh(script: &str, args: &[&str]) -> Result<(), String> {
        let out = Command::new("sh").arg("-c").arg(script).arg("_").args(args).output().map_err(|e| e.to_string())?;
        if out.status.success() {
            Ok(())
        } else {
            Err(String::from_utf8_lossy(&out.stderr).lines().last().unwrap_or("failed").trim().to_string())
        }
    }

    pub fn current() -> Mode {
        let running = Command::new("systemctl").args(["is-active", "--quiet", "sshd"]).status().is_ok_and(|s| s.success());
        mode(running, Path::new(OWNED_MARKER).exists(), Path::new(DEV_MARKER).exists())
    }

    /// Sets this device's password (created once) as root's, then starts sshd for this session.
    /// ROCKNIX's `ssh.enabled` stays 0, so SSH is off again after a restart.
    pub fn turn_on() -> Result<String, String> {
        if Path::new(DEV_MARKER).exists() {
            return Err("SSH is always on on this developer device.".into());
        }
        let pw = crate::secret::load_or_create(Path::new(PASSWORD_PATH)).map_err(|e| e.to_string())?;
        // setrootpass writes /storage/.cache/shadow and root.password, which ROCKNIX applies again at
        // every boot (autostart 007-rootpw), so the password stays until the next change.
        sh(". /etc/profile >/dev/null 2>&1; setrootpass \"$1\"", &[&pw])?;
        // setrootpass also adds root to the smbpasswd file smb.conf names, which is ours: put it back.
        let _ = crate::wifi::ensure_share_login(Path::new(crate::wifi::SHARE_DIR));
        std::fs::write(OWNED_MARKER, b"").map_err(|e| e.to_string())?;
        sh("mkdir -p /storage/.cache/services && touch \"$1\" && systemctl start sshd", &[SERVICE_FLAG])?;
        Ok(pw)
    }

    /// Stops sshd (open sessions end with it) and keeps ROCKNIX from starting it at boot.
    pub fn turn_off() -> Result<(), String> {
        if Path::new(DEV_MARKER).exists() {
            return Err("SSH is always on on this developer device.".into());
        }
        let _ = std::fs::remove_file(OWNED_MARKER);
        // ROCKNIX's boot setting first, under its lock (`system_cfg`), only if it is on; then the service.
        if crate::system_cfg::get("ssh.enabled").is_some_and(|v| v.trim() == "1") {
            crate::system_cfg::set("ssh.enabled", "0");
        }
        sh("rm -f \"$1\"; systemctl stop sshd", &[SERVICE_FLAG])
    }

    pub fn password() -> Option<String> {
        crate::secret::load(Path::new(PASSWORD_PATH))
    }

    /// Runs forever on its own thread: once a minute, turns SSH off when our switch turned it on
    /// and no one has been connected for `IDLE_LIMIT_SECS`. Leaves dev and external SSH alone.
    pub fn watch() {
        let now = || std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
        let mut timer = IdleTimer::new(now());
        loop {
            std::thread::sleep(std::time::Duration::from_secs(60));
            let m = current();
            let sessions = if m == Mode::On { crate::sockets::established(&crate::sockets::read(), SSH_PORT) } else { 0 };
            if timer.tick(now(), m, sessions) {
                match turn_off() {
                    Ok(()) => println!("remote access: off after {} idle minutes", IDLE_LIMIT_SECS / 60),
                    Err(e) => eprintln!("remote access: couldn't turn off: {e}"),
                }
            }
        }
    }
}

#[cfg(not(target_os = "linux"))]
mod imp {
    //! Desktop preview: a switch that remembers its state.
    use super::Mode;
    use std::sync::atomic::{AtomicBool, Ordering};

    static ON: AtomicBool = AtomicBool::new(false);

    pub fn current() -> Mode {
        if ON.load(Ordering::Relaxed) { Mode::On } else { Mode::Off }
    }

    pub fn turn_on() -> Result<String, String> {
        ON.store(true, Ordering::Relaxed);
        Ok("otter-cedar-lamp-315".into())
    }

    pub fn turn_off() -> Result<(), String> {
        ON.store(false, Ordering::Relaxed);
        Ok(())
    }

    pub fn password() -> Option<String> {
        ON.load(Ordering::Relaxed).then(|| "otter-cedar-lamp-315".into())
    }

    pub fn watch() {}
}

pub use imp::{current, password, turn_off, turn_on, watch};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_from_service_state_and_markers() {
        assert_eq!(mode(false, false, false), Mode::Off);
        assert_eq!(mode(true, true, false), Mode::On);
        assert_eq!(mode(true, false, false), Mode::External);
        assert_eq!(mode(false, true, false), Mode::Off, "stale marker, sshd gone");
        assert_eq!(mode(true, false, true), Mode::Dev);
        assert_eq!(mode(false, false, true), Mode::Dev, "the marker wins even while sshd restarts");
    }

    #[test]
    fn turns_off_after_thirty_idle_minutes() {
        let mut t = IdleTimer::new(0);
        assert!(!t.tick(0, Mode::Off, 0));
        // Switched on at minute 1: counted from there, not from the timer's creation.
        assert!(!t.tick(60, Mode::Off, 0));
        assert!(!t.tick(120, Mode::On, 0));
        assert!(!t.tick(60 + IDLE_LIMIT_SECS - 1, Mode::On, 0));
        assert!(t.tick(60 + IDLE_LIMIT_SECS, Mode::On, 0));
    }

    #[test]
    fn an_open_session_keeps_it_on() {
        let mut t = IdleTimer::new(0);
        assert!(!t.tick(0, Mode::On, 1));
        assert!(!t.tick(2 * IDLE_LIMIT_SECS, Mode::On, 1), "a long key-based session is never cut off");
        // Idle time counts from the last check that still saw a session.
        assert!(!t.tick(2 * IDLE_LIMIT_SECS + 60, Mode::On, 0));
        assert!(!t.tick(3 * IDLE_LIMIT_SECS - 1, Mode::On, 0));
        assert!(t.tick(3 * IDLE_LIMIT_SECS, Mode::On, 0));
    }

    #[test]
    fn never_touches_dev_or_external_ssh() {
        let mut t = IdleTimer::new(0);
        for m in [Mode::Dev, Mode::External, Mode::Off] {
            assert!(!t.tick(10 * IDLE_LIMIT_SECS, m, 0), "{m:?}");
        }
        let mut t = IdleTimer::new(0);
        assert!(!t.tick(10 * IDLE_LIMIT_SECS, Mode::Dev, 0));
    }
}
