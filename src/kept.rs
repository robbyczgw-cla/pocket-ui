//! A value the UI shows but must never read itself (a process, a file, a syscall that can stall on the card): read on
//! a thread of its own and kept in memory. The UI thread asks [`Kept::get`] (memory only) and [`Kept::refresh`]
//! (starts a read when the last one is older than `every` and none is under way; two atomics and a lock otherwise);
//! the thread calls `notify` when what it read differs from what was kept, so the screen can be drawn again.
//! Until the first read is in, `get` is None and the screen shows a neutral placeholder.
//!
//! Users: the About page's free space and address (`App`), Settings' Bluetooth row (`app_bluetooth`), `wifi::ip`.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub struct Kept<T> {
    /// The value and when its read was started.
    inner: Mutex<(Option<T>, Option<Instant>)>,
    reading: AtomicBool,
    name: &'static str,
}

impl<T: Clone + PartialEq + Send + 'static> Kept<T> {
    /// `name`: the reading thread's name (in `top -H` and crash reports).
    pub fn new(name: &'static str) -> Arc<Kept<T>> {
        Arc::new(Kept { inner: Mutex::new((None, None)), reading: AtomicBool::new(false), name })
    }

    /// What the last read found; None before the first one is in. Memory only.
    pub fn get(&self) -> Option<T> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).0.clone()
    }

    /// Puts `value` in place as if a read had just found it (a write we know the result of, snapshots).
    pub fn set(&self, value: T) {
        let mut g = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        *g = (Some(value), Some(Instant::now()));
    }

    /// The next `refresh` reads again, whatever the age of the value (a change we caused but don't know the result
    /// of). The value stays shown until then.
    pub fn stale(&self) {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).1 = None;
    }

    /// Whether a read is due at `now`: none started yet, or the last one `every` ago and none under way.
    pub fn due(&self, every: Duration, now: Instant) -> bool {
        !self.reading.load(Ordering::SeqCst)
            && self.inner.lock().unwrap_or_else(|e| e.into_inner()).1.is_none_or(|at| now.duration_since(at) >= every)
    }

    /// Starts `read` on a thread when one is due (`due`); returns at once either way. `notify` is called from that
    /// thread when the value changed (the first value counts as a change). True when a read was started.
    pub fn refresh(self: &Arc<Self>, every: Duration, read: impl FnOnce() -> T + Send + 'static, notify: impl FnOnce() + Send + 'static) -> bool {
        let now = Instant::now();
        {
            let mut g = self.inner.lock().unwrap_or_else(|e| e.into_inner());
            if self.reading.load(Ordering::SeqCst) || g.1.is_some_and(|at| now.duration_since(at) < every) {
                return false;
            }
            g.1 = Some(now);
            self.reading.store(true, Ordering::SeqCst);
        }
        let me = self.clone();
        let spawned = std::thread::Builder::new().name(self.name.into()).spawn(move || {
            let value = read();
            let changed = {
                let mut g = me.inner.lock().unwrap_or_else(|e| e.into_inner());
                let changed = g.0.as_ref() != Some(&value);
                g.0 = Some(value);
                changed
            };
            me.reading.store(false, Ordering::SeqCst);
            if changed {
                notify();
            }
        });
        if spawned.is_err() {
            self.reading.store(false, Ordering::SeqCst);
        }
        spawned.is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::channel;

    #[test]
    fn the_caller_never_reads_and_never_waits() {
        let kept: Arc<Kept<u32>> = Kept::new("kept-test");
        let caller = std::thread::current().id();
        let (tx, rx) = channel();
        let (done_tx, done_rx) = channel();
        let (release_tx, release_rx) = channel();
        let started = kept.refresh(
            Duration::from_secs(5),
            move || {
                tx.send(std::thread::current().id()).unwrap();
                release_rx.recv_timeout(Duration::from_secs(5)).expect("caller released the read");
                7
            },
            move || done_tx.send(()).unwrap(),
        );
        assert!(started, "returned while the read is still blocked");
        assert_eq!(kept.get(), None, "nothing before the first read: the screen shows a placeholder");
        assert!(!kept.refresh(Duration::ZERO, || 8, || {}), "one read at a time");
        assert_ne!(rx.recv_timeout(Duration::from_secs(5)).unwrap(), caller, "read on another thread");
        release_tx.send(()).unwrap();
        done_rx.recv_timeout(Duration::from_secs(5)).expect("notified");
        assert_eq!(kept.get(), Some(7));
        assert!(!kept.refresh(Duration::from_secs(5), || 9, || {}), "not due again yet");
        assert!(kept.due(Duration::ZERO, Instant::now()));
    }

    #[test]
    fn notify_only_on_a_change_and_stale_reads_again() {
        let kept: Arc<Kept<&'static str>> = Kept::new("kept-test");
        kept.set("On");
        assert!(!kept.due(Duration::from_secs(5), Instant::now()), "a known write counts as a read");
        kept.stale();
        let (tx, rx) = channel();
        assert!(kept.refresh(Duration::from_secs(5), || "On", move || tx.send(()).unwrap()));
        let until = Instant::now() + Duration::from_secs(5);
        while kept.reading.load(Ordering::SeqCst) && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(rx.try_recv().is_err(), "the same value: no frame for it");
        assert_eq!(kept.get(), Some("On"));
    }
}
