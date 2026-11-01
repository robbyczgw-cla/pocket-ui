//! Remote access over SSH, switched in Settings (docs/SECURITY-HARDENING.md, B2 in
//! the internal review). Off unless the user turns it on; then root can log in with this
//! device's password (shown on screen, same kind as the file sharing password) or a key from
//! /storage/.ssh/authorized_keys. It stays on until switched off, and is off after a restart. A developer device keeps SSH on for good with the `dev-ssh` marker file.

pub const SSH_PORT: u16 = 22;
/// Present on development devices: SSH stays on, the switch leaves it alone.
pub const DEV_MARKER: &str = "/storage/.config/pocket-ui/dev-ssh";
pub const PASSWORD_PATH: &str = "/storage/.config/pocket-ui/remote/password";
/// Exists while SSH runs because of our switch; on /run, so it is gone after a restart.
pub const OWNED_MARKER: &str = "/run/pocket-ui-ssh-on";
/// ROCKNIX's sshd.service only starts while this file exists (ConditionPathExists).
pub const SERVICE_FLAG: &str = "/storage/.cache/services/sshd.conf";

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Mode {
    Off,
    /// Turned on with our switch.
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
}

pub use imp::{current, password, turn_off, turn_on};

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
}
