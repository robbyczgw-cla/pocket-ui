//! Safe child command for the power-key inhibitor.

#[cfg(target_os = "linux")]
pub use linux::pipe_inhibitor;

#[cfg(target_os = "linux")]
mod linux {
    use std::{io, os::unix::process::CommandExt, process::{Child, Command}};

    /// The power-key inhibitor is handed from a startup thread to the UI.
    pub fn pipe_inhibitor(command: &mut Command) -> io::Result<Child> {
        // Its startup thread returns this handle to the UI. Tie lifetime to stdin
        // (cat ends at EOF), not to that short thread's death.
        configure(command)?;
        command.spawn()
    }


    fn configure(command: &mut Command) -> io::Result<()> {
        // Read the fallback limit before fork; pre_exec must not allocate or take locks.
        let mut limit = libc::rlimit { rlim_cur: 0, rlim_max: 0 };
        unsafe {
            if libc::getrlimit(libc::RLIMIT_NOFILE, &mut limit) != 0 { return Err(io::Error::last_os_error()); }
            command.pre_exec(move || {
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
