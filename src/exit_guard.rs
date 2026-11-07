//! A core that never answers must not make Quit impossible. PPSSPP on OpenGL ES runs its emulation on a thread of its own;
//! when that thread waits for a frame of the main thread while the main thread waits (in `retro_serialize_size`) for it to
//! pause, neither moves again (Wild Arms XF, Load then Quit, Odin 2 Mini 04.11.2026). pocket-core marks the calls into the
//! core that the exit save makes (`CoreCalls::during`) and a watcher thread (`CoreCalls::watch`) ends the process when
//! one takes longer than the limit.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// A process-wide progress clock. Unlike the state-call guard, this is running before loading the core and stays
/// alive through teardown. Requests are recorded by the FIFO reader (or the signal watcher), never by the blocked
/// emulation thread. Its terminal callback must not call the core or change any published moment.
pub struct Progress {
    state: Mutex<ProgressState>,
}

struct ProgressState {
    last: Instant,
    request: Option<Instant>,
    /// The main thread is in work of its own that can take seconds (starting, a state save or load, a Continue),
    /// not in a frame: a Pause or Quit waits for it up to `Limits::busy`.
    busy: bool,
    /// The main thread has begun the exit: its save has its own guards and pocket-ui's patience
    /// (`quit::SAVE_LIMIT`); another Quit or SIGTERM must not end it halfway.
    exiting: bool,
    /// The main thread waits for the display to take a frame (`Progress::display`), not for the core.
    display: bool,
    stopped: bool,
}

/// How long the player may go without an answer before `Progress::watch` ends it.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    /// No completed main-loop turn at all (a core stuck in a frame, a load or teardown).
    pub stall: Duration,
    /// A Pause or Quit not picked up while the main thread runs frames or waits in the pause.
    pub unanswered: Duration,
    /// The same while the main thread is busy (`Progress::busy`).
    pub busy: Duration,
    /// `stall` once the exit has begun (`Progress::exiting`).
    pub exiting: Duration,
}

impl Progress {
    /// Busy from the start: loading the core and the first Continue are no frames.
    pub fn new() -> Arc<Self> {
        Arc::new(Self { state: Mutex::new(ProgressState { last: Instant::now(), request: None, busy: true, exiting: false, display: false, stopped: false }) })
    }

    /// A completed event-loop turn, or a request the main thread has begun handling. Repeated incoming requests
    /// cannot postpone recovery: only actual main-thread progress clears the first outstanding request.
    pub fn answered(&self) {
        let mut s = self.state.lock().unwrap();
        s.last = Instant::now();
        s.request = None;
    }

    /// A completed frame keeps the general stall clock alive, but cannot acknowledge a command that arrived while
    /// that frame was running. Only the code that handles that command may do that.
    pub fn checkpoint(&self) {
        self.state.lock().unwrap().last = Instant::now();
    }

    pub fn request(&self) {
        let mut s = self.state.lock().unwrap();
        if !s.exiting {
            s.request.get_or_insert_with(Instant::now);
        }
    }

    /// The main thread enters (`true`) or leaves work of its own that is no frame; returns what it was, so nested work
    /// can put it back. Not a checkpoint: a state load that never returns still ends at `Limits::stall`.
    pub fn busy(&self, on: bool) -> bool {
        std::mem::replace(&mut self.state.lock().unwrap().busy, on)
    }

    /// Work outside a frame, including shortcuts and Earlier captures that do not come through the command FIFO.
    pub fn work<T>(&self, call: impl FnOnce() -> T) -> T {
        struct Restore<'a>(&'a Progress, bool);
        impl Drop for Restore<'_> {
            fn drop(&mut self) {
                let mut s = self.0.state.lock().unwrap();
                s.busy = self.1;
                s.last = Instant::now();
                // Completing slow work is progress, not an acknowledgement of the command queued meanwhile.
                // Give that command its frame-time patience from here, rather than charge the save's time twice.
                if s.request.is_some() { s.request = Some(s.last); }
            }
        }
        let _restore = Restore(self, self.busy(true));
        call()
    }

    /// `present` hands a frame to the display (the swap), which blocks until the compositor wants the next one. With the
    /// panel's output switched off (SM8550 screen off, pocket-ui's light sleep powering it off before pocket-core has
    /// taken its `sleep`) that is never: the game stands still, the core is fine, and the frame goes on once the output
    /// is back (Odin 2 Mini 05.11.2026: ended after 120 s of screen off as a stall). No verdict while it waits; afterwards
    /// the clocks start again, and a Pause or Quit that came meanwhile gets its full patience from then.
    pub fn display<T>(&self, present: impl FnOnce() -> T) -> T {
        self.state.lock().unwrap().display = true;
        let out = present();
        let mut s = self.state.lock().unwrap();
        s.display = false;
        s.last = Instant::now();
        if s.request.is_some() {
            s.request = Some(s.last);
        }
        out
    }

    /// The main thread has taken the Quit (or SIGTERM): from here on only a stall of `Limits::exiting` ends it.
    pub fn exiting(&self) {
        let mut s = self.state.lock().unwrap();
        s.last = Instant::now();
        s.request = None;
        s.exiting = true;
    }

    /// `terminate` is written by the async-signal-safe SIGTERM handler; all locks and I/O remain on this thread.
    pub fn watch(self: &Arc<Self>, limits: Limits, poll: Duration, terminate: &'static AtomicBool, on_stuck: impl FnOnce() + Send + 'static) {
        let me = Arc::clone(self);
        std::thread::spawn(move || {
            let mut signal_seen = false;
            loop {
                std::thread::sleep(poll);
                let mut s = me.state.lock().unwrap();
                if s.stopped { return; }
                if s.display { continue; }
                if !signal_seen && !s.exiting && terminate.load(Ordering::Relaxed) {
                    s.request.get_or_insert_with(Instant::now);
                    signal_seen = true;
                }
                let stall = if s.exiting { limits.exiting } else { limits.stall };
                let patience = if s.busy { limits.busy } else { limits.unanswered };
                if s.last.elapsed() >= stall || s.request.is_some_and(|at| at.elapsed() >= patience) {
                    // The callback ends the process. Keep the lock: a returning main thread cannot acknowledge
                    // the request and resume the game after recovery has started.
                    on_stuck();
                    return;
                }
            }
        });
    }

    pub fn stop(&self) {
        self.state.lock().unwrap().stopped = true;
    }
}

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
