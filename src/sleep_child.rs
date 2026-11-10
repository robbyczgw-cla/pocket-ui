//! Child commands used by the host suspend watcher.

/// The logind watcher and the delay inhibitor that gives it time, on the host itself or out of our Flatpak
/// (`flatpak-spawn --host`, as `runemu.sh` reaches the other Flatpaks).
pub fn commands(in_flatpak: bool) -> (Vec<&'static str>, Vec<&'static str>) {
    let mut monitor = vec!["dbus-monitor", "--system", "type='signal',sender='org.freedesktop.login1',interface='org.freedesktop.login1.Manager',member='PrepareForSleep'"];
    let mut inhibit = vec!["systemd-inhibit", "--what=sleep", "--mode=delay", "--who=pocket-ui", "--why=Saves the game before sleeping", "cat"];
    if in_flatpak {
        monitor.splice(0..0, ["flatpak-spawn", "--host", "--watch-bus"]);
        inhibit.splice(0..0, ["flatpak-spawn", "--host", "--watch-bus"]);
    }
    (monitor, inhibit)
}

#[cfg(target_os = "linux")]
pub use linux::{monitor, pipe_inhibitor, spawn};

#[cfg(target_os = "linux")]
mod linux {
    use std::{io, ops::{Deref, DerefMut}, os::unix::process::CommandExt, process::{Child, Command, Stdio}};

    /// Rust's Child does not kill on drop. The watcher owns its monitor and delay lock.
    pub struct OwnedChild(Child);
    impl Deref for OwnedChild { type Target = Child; fn deref(&self) -> &Child { &self.0 } }
    impl DerefMut for OwnedChild { fn deref_mut(&mut self) -> &mut Child { &mut self.0 } }
    impl Drop for OwnedChild {
        fn drop(&mut self) {
            // try_wait reaps an already finished child, avoiding a kill of a reused PID.
            if matches!(self.0.try_wait(), Ok(None)) { let _ = self.0.kill(); }
            let _ = self.0.wait();
        }
    }

    pub fn monitor(in_flatpak: bool) -> io::Result<OwnedChild> {
        let (monitor, _) = super::commands(in_flatpak);
        spawn(Command::new(monitor[0]).args(&monitor[1..]).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()))
    }

    pub fn spawn(command: &mut Command) -> io::Result<OwnedChild> {
        protect(command)?;
        command.spawn().map(OwnedChild)
    }

    /// The power-key inhibitor is handed from a startup thread to the UI.
    pub fn pipe_inhibitor(command: &mut Command) -> io::Result<Child> {
        // Its startup thread returns this handle to the UI. Tie lifetime to stdin
        // (cat ends at EOF), not to that short thread's death.
        configure(command, false)?;
        command.spawn()
    }

    /// Bind helpers to the starting thread even on abrupt exit, and close all unrelated
    /// descriptors at exec. CLOEXEC preserves Rust's internal exec-error pipe until exec.
    fn protect(command: &mut Command) -> io::Result<()> { configure(command, true) }

    fn configure(command: &mut Command, thread_owned: bool) -> io::Result<()> {
        // Read the fallback limit before fork; pre_exec must not allocate or take locks.
        let mut limit = libc::rlimit { rlim_cur: 0, rlim_max: 0 };
        unsafe {
            if libc::getrlimit(libc::RLIMIT_NOFILE, &mut limit) != 0 { return Err(io::Error::last_os_error()); }
            let parent = libc::getpid();
            command.pre_exec(move || {
                if thread_owned {
                    if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) != 0 { return Err(io::Error::last_os_error()); }
                    // Close the fork/prctl race if the process died before its signal was armed.
                    if libc::getppid() != parent { libc::_exit(1); }
                }
                const CLOSE_RANGE_CLOEXEC: libc::c_uint = 4;
                if libc::syscall(libc::SYS_close_range, 3u32, u32::MAX, CLOSE_RANGE_CLOEXEC) == 0 { return Ok(()); }
                // Older handheld kernels lack close_range. fcntl is async-signal-safe.
                for fd in 3..limit.rlim_cur.min(libc::c_int::MAX as libc::rlim_t) as libc::c_int {
                    let flags = libc::fcntl(fd, libc::F_GETFD);
                    if flags >= 0 && libc::fcntl(fd, libc::F_SETFD, flags | libc::FD_CLOEXEC) < 0 {
                        return Err(io::Error::last_os_error());
                    }
                }
                Ok(())
            });
        }
        Ok(())
    }
}
