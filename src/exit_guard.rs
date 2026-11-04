//! A core that never answers must not make Quit impossible. PPSSPP on OpenGL ES runs its emulation on a thread of its own;
//! when that thread waits for a frame of the main thread while the main thread waits (in `retro_serialize_size`) for it to
//! pause, neither moves again (Wild Arms XF, Load then Quit, Odin 2 Mini 04.11.2026). pocket-core marks the calls into the
//! core that the exit save makes (`CoreCalls::during`) and a watcher thread (`CoreCalls::watch`) ends the process when
//! one takes longer than the limit.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Default)]
pub struct CoreCalls {
    since: Mutex<Option<Instant>>,
    stopped: AtomicBool,
}

impl CoreCalls {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Runs a call into the core, which the watcher times.
    pub fn during<T>(&self, call: impl FnOnce() -> T) -> T {
        *self.since.lock().unwrap() = Some(Instant::now());
        let out = call();
        *self.since.lock().unwrap() = None;
        out
    }

    /// Calls `on_stuck` (with how long the call has taken) once, when a call has run for more than `limit`. Checks
    /// every `poll`. Ends with `stop` or after it fired.
    pub fn watch(self: &Arc<Self>, limit: Duration, poll: Duration, on_stuck: impl FnOnce(Duration) + Send + 'static) {
        let me = Arc::clone(self);
        std::thread::spawn(move || {
            while !me.stopped.load(Ordering::Relaxed) {
                std::thread::sleep(poll);
                // Keep the lock until the terminal callback returns: a core call that
                // finishes at the deadline must not proceed to write files concurrently.
                let active = me.since.lock().unwrap();
                if me.stopped.load(Ordering::Relaxed) { return; }
                if let Some(since) = active.filter(|s| s.elapsed() > limit) {
                    on_stuck(since.elapsed());
                    return;
                }
            }
        });
    }

    pub fn stop(&self) {
        self.stopped.store(true, Ordering::Relaxed);
    }
}
