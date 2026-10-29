//! Helper programs (`grim`, `swaymsg`, `ip`, `nmcli` …) that the UI thread has to wait for. A helper that
//! hangs must cost a bounded delay, never the whole UI (the internal review, B10).

use std::io::{self, Read};
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, ExitStatus, Output, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

/// What a short helper (`ip`, `pactl`, `pw-metadata` …) gets when the UI thread, or a game start, waits for it.
/// They answer in milliseconds; this is for the day one doesn't.
pub const HELPER: Duration = Duration::from_secs(3);

/// Time one user action may spend waiting on helpers; later helpers get what is left.
pub struct Budget {
    end: Instant,
}

impl Budget {
    pub fn new(total: Duration) -> Budget {
        Budget { end: Instant::now() + total }
    }

    /// What is left, zero once it has run out.
    pub fn left(&self) -> Duration {
        self.end.saturating_duration_since(Instant::now())
    }

    /// The smaller of `cap` and what is left.
    pub fn cap(&self, cap: Duration) -> Duration {
        self.left().min(cap)
    }
}

/// Runs `cmd` to completion like `Command::output`, but gives up after `timeout`: the child (and its
/// process group, so `sh -c` children too) is killed and the error has kind `TimedOut`. stdin is closed.
/// A zero timeout (a spent `Budget`) doesn't start the program at all.
pub fn run_with_timeout(cmd: &mut Command, timeout: Duration) -> io::Result<Output> {
    if timeout.is_zero() {
        return Err(timed_out());
    }
    cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = spawn(cmd)?;
    let (stdout, stderr) = (Pipe::drain(child.stdout.take()), Pipe::drain(child.stderr.take()));
    let status = wait(&mut child, Instant::now() + timeout, None)?;
    // The child is gone. A grandchild it left behind may still hold the pipes open; whatever was written
    // before the child exited is already in them, so they get a moment, not the grandchild's lifetime.
    let group = child.id() as i32;
    Ok(Output {
        status,
        stdout: stdout.take(GRACE, group),
        stderr: stderr.take(GRACE, group),
    })
}

/// Like `Command::status` with stdin, stdout and stderr closed, under the same timeout.
pub fn status_with_timeout(cmd: &mut Command, timeout: Duration) -> io::Result<ExitStatus> {
    if timeout.is_zero() {
        return Err(timed_out());
    }
    cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    let mut child = spawn(cmd)?;
    wait(&mut child, Instant::now() + timeout, None)
}

/// Like `run_with_timeout`, and also ends the child (with its group) as soon as `stop` is set; that ends with an
/// error of kind `Interrupted`. For a helper that runs as long as it is wanted (`bluetoothctl --timeout 15 scan on`).
pub fn run_until(cmd: &mut Command, timeout: Duration, stop: &AtomicBool) -> io::Result<Output> {
    if timeout.is_zero() {
        return Err(timed_out());
    }
    cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = spawn(cmd)?;
    let (stdout, stderr) = (Pipe::drain(child.stdout.take()), Pipe::drain(child.stderr.take()));
    let status = wait(&mut child, Instant::now() + timeout, Some(stop))?;
    let group = child.id() as i32;
    Ok(Output {
        status,
        stdout: stdout.take(GRACE, group),
        stderr: stderr.take(GRACE, group),
    })
}

/// How long the pipes get while something of the child's process group still lives (a daemon it started, `sleep 5 &`,
/// holding them).
const GRACE: Duration = Duration::from_millis(100);
/// The guard for a pipe that a daemon holds after leaving the group (it `setsid`ed): the end-of-file then never comes.
/// Not a budget to decide the output with — see `Pipe::take`.
const EOF_PATIENCE: Duration = Duration::from_secs(30);

fn timed_out() -> io::Error {
    io::Error::new(io::ErrorKind::TimedOut, "helper program timed out")
}

fn spawn(cmd: &mut Command) -> io::Result<Child> {
    // Its own process group: a timeout kills the shell's children with it, and nothing else.
    cmd.process_group(0).spawn()
}

/// Polls for the child's exit; at the deadline (or when `stop` is set) kills its group and reports `TimedOut`
/// (or `Interrupted`).
fn wait(child: &mut Child, deadline: Instant, stop: Option<&AtomicBool>) -> io::Result<ExitStatus> {
    let mut nap = Duration::from_millis(1);
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(status);
        }
        let now = Instant::now();
        let stopped = stop.is_some_and(|s| s.load(Ordering::Relaxed));
        if now >= deadline || stopped {
            // SAFETY: kill(2) with a negative pid signals only the group `spawn` created for this child.
            unsafe { libc::kill(-(child.id() as i32), libc::SIGKILL) };
            let _ = child.kill();
            let _ = child.wait();
            return Err(if stopped { io::Error::new(io::ErrorKind::Interrupted, "stopped") } else { timed_out() });
        }
        std::thread::sleep(nap.min(deadline - now));
        nap = (nap * 2).min(Duration::from_millis(10));
    }
}

/// One of the child's output pipes, read on its own thread so a full pipe never blocks the child.
struct Pipe {
    data: Arc<Mutex<Vec<u8>>>,
    eof: mpsc::Receiver<()>,
}

impl Pipe {
    fn drain(from: Option<impl Read + Send + 'static>) -> Pipe {
        let data = Arc::new(Mutex::new(Vec::new()));
        let (tx, eof) = mpsc::channel();
        if let Some(mut from) = from {
            let data = data.clone();
            std::thread::spawn(move || {
                let mut chunk = [0u8; 8192];
                while let Ok(n) = from.read(&mut chunk) {
                    if n == 0 {
                        break;
                    }
                    data.lock().unwrap_or_else(|e| e.into_inner()).extend_from_slice(&chunk[..n]);
                }
                let _ = tx.send(());
            });
        }
        Pipe { data, eof }
    }

    /// Everything the command wrote: the pipe has ended, or `grace` is over while something of its group still lives.
    ///
    /// The child has exited, so its end of the pipe is closed and everything it wrote is in there: the end-of-file now
    /// only waits for the reader thread to be scheduled. A loaded machine can take longer than any fixed grace for
    /// that (the gate read a `bluetoothctl trust` that succeeded as empty and told the player to pair again), so it is
    /// waited for as long as it takes. `grace` only applies while the group still has processes — a daemon the command
    /// started may write more later, and the caller isn't waiting for it — and `EOF_PATIENCE` guards the pipe a daemon
    /// holds after leaving the group.
    fn take(self, grace: Duration, group: i32) -> Vec<u8> {
        let started = Instant::now();
        loop {
            // SAFETY: kill(2) with signal 0 on a negative pid only asks whether the group exists.
            let alive = unsafe { libc::kill(-group, 0) } == 0;
            let limit = if alive { grace } else { EOF_PATIENCE };
            if started.elapsed() >= limit {
                break;
            }
            let nap = (limit - started.elapsed()).min(Duration::from_millis(20));
            match self.eof.recv_timeout(nap) {
                Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
        }
        std::mem::take(&mut *self.data.lock().unwrap_or_else(|e| e.into_inner()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sh(script: &str) -> Command {
        let mut c = Command::new("sh");
        c.args(["-c", script]);
        c
    }

    #[test]
    fn a_hanging_command_is_cut_off_at_the_timeout() {
        let t0 = Instant::now();
        let err = run_with_timeout(Command::new("sleep").arg("5"), Duration::from_millis(200)).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::TimedOut);
        assert!(t0.elapsed() < Duration::from_secs(2), "waited {:?}", t0.elapsed());
    }

    #[test]
    fn a_shell_script_that_hangs_is_killed_with_its_children() {
        let t0 = Instant::now();
        let err = run_with_timeout(&mut sh("sleep 5; echo late"), Duration::from_millis(200)).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::TimedOut);
        assert!(t0.elapsed() < Duration::from_secs(2), "waited {:?}", t0.elapsed());
    }

    #[test]
    fn a_quick_command_returns_its_output_and_status() {
        let out = run_with_timeout(&mut sh("echo hi; echo oops >&2; exit 3"), Duration::from_secs(5)).unwrap();
        assert_eq!(out.stdout, b"hi\n");
        assert_eq!(out.stderr, b"oops\n");
        assert_eq!(out.status.code(), Some(3));
    }

    #[test]
    fn output_larger_than_a_pipe_does_not_deadlock() {
        let out = run_with_timeout(&mut sh("head -c 300000 /dev/zero"), Duration::from_secs(5)).unwrap();
        assert_eq!(out.stdout.len(), 300_000);
    }

    #[test]
    fn a_background_child_holding_the_pipe_does_not_hold_us_up() {
        // `sh` exits at once, but `sleep` keeps stdout open: Command::output would wait the full 5 s.
        let t0 = Instant::now();
        let out = run_with_timeout(&mut sh("sleep 5 & echo done"), Duration::from_secs(3)).unwrap();
        assert_eq!(out.stdout, b"done\n");
        assert!(t0.elapsed() < Duration::from_secs(2), "waited {:?}", t0.elapsed());
    }

    #[test]
    fn a_missing_program_is_an_error_not_a_hang() {
        let err = run_with_timeout(&mut Command::new("pocket-ui-no-such-program"), Duration::from_secs(1)).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::NotFound);
    }

    #[test]
    fn status_variant_times_out_too() {
        let t0 = Instant::now();
        let err = status_with_timeout(Command::new("sleep").arg("5"), Duration::from_millis(200)).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::TimedOut);
        assert!(t0.elapsed() < Duration::from_secs(2));
        assert!(status_with_timeout(&mut sh("exit 0"), Duration::from_secs(5)).unwrap().success());
        assert!(!status_with_timeout(&mut sh("exit 1"), Duration::from_secs(5)).unwrap().success());
    }

    #[test]
    fn a_stopped_command_ends_at_once_and_one_left_alone_returns_its_output() {
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(150));
            flag.store(true, Ordering::Relaxed);
        });
        let t0 = Instant::now();
        let err = run_until(&mut sh("sleep 5"), Duration::from_secs(10), &stop).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::Interrupted);
        assert!(t0.elapsed() < Duration::from_secs(2), "waited {:?}", t0.elapsed());
        let out = run_until(&mut sh("echo found"), Duration::from_secs(5), &AtomicBool::new(false)).unwrap();
        assert_eq!(out.stdout, b"found\n");
        let err = run_until(&mut sh("sleep 5"), Duration::from_millis(150), &AtomicBool::new(false)).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::TimedOut);
    }

    #[test]
    fn a_budget_is_shared_and_runs_out() {
        let mut b = Budget::new(Duration::from_secs(60));
        assert!(b.left() <= Duration::from_secs(60));
        assert_eq!(b.cap(Duration::from_millis(100)), Duration::from_millis(100));
        // Advance the deadline instead of sleeping on the build machine.
        b.end = Instant::now() - Duration::from_millis(1);
        assert_eq!(b.left(), Duration::ZERO);
        assert_eq!(b.cap(Duration::from_millis(100)), Duration::ZERO);
    }
}
