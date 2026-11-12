//! Periodic Android Earlier writes: at most one active job and one newer waiting job.
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
type Job = Box<dyn FnOnce() + Send>;
#[derive(Default)]
struct Pending { job: Option<Job>, busy: bool, quit: bool }
struct Shared { pending: Mutex<Pending>, changed: Condvar }
impl Shared {
    fn lock(&self) -> std::sync::MutexGuard<'_, Pending> {
        self.pending.lock().unwrap_or_else(|e| e.into_inner())
    }
}
pub struct Writer { shared: Arc<Shared>, thread: Option<JoinHandle<()>> }
impl Writer {
    pub fn start() -> std::io::Result<Self> {
        let shared = Arc::new(Shared { pending: Mutex::new(Pending::default()), changed: Condvar::new() });
        let next = shared.clone();
        let thread = std::thread::Builder::new().name("android-earlier".into()).spawn(move || run(next))?;
        Ok(Self { shared, thread: Some(thread) })
    }
    /// A periodic capture supersedes the waiting one; an active durable write finishes.
    pub fn submit(&self, job: impl FnOnce() + Send + 'static) {
        self.shared.lock().job = Some(Box::new(job));
        self.shared.changed.notify_all();
    }
    #[cfg(test)]
    fn wait(&self) {
        let mut p = self.shared.lock();
        while p.busy || p.job.is_some() {
            p = self.shared.changed.wait(p).unwrap_or_else(|e| e.into_inner());
        }
    }
}
fn run(shared: Arc<Shared>) {
    let mut p = shared.lock();
    loop {
        while p.job.is_none() && !p.quit { p = shared.changed.wait(p).unwrap_or_else(|e| e.into_inner()); }
        if p.quit { return; }
        let Some(job) = p.job.take() else { continue; };
        p.busy = true;
        drop(p);
        job();
        p = shared.lock();
        p.busy = false;
        shared.changed.notify_all();
    }
}
impl Drop for Writer {
    fn drop(&mut self) {
        { let mut p = self.shared.lock(); p.quit = true; p.job = None; }
        self.shared.changed.notify_all();
        if let Some(t) = self.thread.take() {
            if t.join().is_err() { eprintln!("Earlier writer failed before shutdown"); }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    // A virtual slow card advances the clock of the calling thread. This avoids scheduler
    // timing thresholds: only a card write on the game thread drains its PCM reserve.
    thread_local! { static BLOCKED_MS: Cell<usize> = const { Cell::new(0) }; }
    #[test]
    fn a_slow_earlier_write_does_not_starve_game_audio() {
        BLOCKED_MS.with(|t| t.set(0));
        let done = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let written = done.clone();
        let writer = Writer::start().unwrap();
        writer.submit(move || {
            BLOCKED_MS.with(|t| t.set(t.get() + 100));
            written.store(true, std::sync::atomic::Ordering::SeqCst);
        });
        let withheld_frames = BLOCKED_MS.with(|t| t.get() * 48);
        let buffered_frames = 48 * 48 + 800;
        let underrun_frames = withheld_frames.saturating_sub(buffered_frames);
        writer.wait();
        assert!(done.load(std::sync::atomic::Ordering::SeqCst), "the state was silently dropped");
        assert_eq!(underrun_frames, 0, "a slow save write consumed the game's audio reserve");
    }
    #[test]
    fn a_busy_writer_keeps_only_the_newest_waiting_capture() {
        let writer = Writer::start().unwrap();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let first = seen.clone();
        writer.submit(move || {
            started_tx.send(()).unwrap();
            release_rx.recv_timeout(std::time::Duration::from_secs(2)).unwrap();
            first.lock().unwrap().push(1);
        });
        started_rx.recv_timeout(std::time::Duration::from_secs(2)).unwrap();
        let second = seen.clone();
        writer.submit(move || second.lock().unwrap().push(2));
        let third = seen.clone();
        writer.submit(move || third.lock().unwrap().push(3));
        release_tx.send(()).unwrap();
        writer.wait();
        assert_eq!(*seen.lock().unwrap(), [1, 3], "a slow card accumulated stale Earlier snapshots");
    }

    #[test]
    fn shutdown_finishes_active_work_and_discards_the_waiting_older_capture() {
        let writer = Writer::start().unwrap();
        let done = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let first = done.clone();
        writer.submit(move || {
            started_tx.send(()).unwrap();
            release_rx.recv_timeout(std::time::Duration::from_secs(2)).unwrap();
            first.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        });
        started_rx.recv_timeout(std::time::Duration::from_secs(2)).unwrap();
        let waiting = done.clone();
        writer.submit(move || { waiting.fetch_add(10, std::sync::atomic::Ordering::SeqCst); });
        // Drop marks shutdown before the blocked active job can return.
        let shared = writer.shared.clone();
        let release = std::thread::spawn(move || {
            let mut p = shared.lock();
            while !p.quit { p = shared.changed.wait(p).unwrap(); }
            drop(p);
            release_tx.send(()).unwrap();
        });
        drop(writer);
        release.join().unwrap();
        assert_eq!(done.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

}
