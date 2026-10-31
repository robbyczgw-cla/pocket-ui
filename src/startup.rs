//! Where the time to the first shelf frame goes: `mark` notes a step of pocket-ui's start, `report` gives them as one
//! log line next to "first frame after" (docs/DEVELOPING.md, Start time). A handful of `Instant`s in a mutex, written
//! only during the start; costs nothing measurable.

use std::sync::Mutex;
use std::time::{Duration, Instant};

static MARKS: Mutex<Vec<(&'static str, Instant)>> = Mutex::new(Vec::new());

/// Notes that the step `label` of the start has just ended.
pub fn mark(label: &'static str) {
    if let Ok(mut m) = MARKS.lock() {
        // Only the start is traced; later calls (a second window, a test) are ignored.
        if m.len() < 64 {
            m.push((label, Instant::now()));
        }
    }
}

/// "logs 1.2, event loop 3.4, ..." in milliseconds, each step since the one before (the first since `started`).
pub fn report(started: Instant) -> String {
    let Ok(m) = MARKS.lock() else { return String::new() };
    steps(started, &m).iter().map(|(l, d)| format!("{l} {:.1}", d.as_secs_f32() * 1000.0)).collect::<Vec<_>>().join(", ")
}

/// The length of each step.
fn steps(started: Instant, marks: &[(&'static str, Instant)]) -> Vec<(&'static str, Duration)> {
    let mut last = started;
    marks
        .iter()
        .map(|(l, t)| {
            let d = t.saturating_duration_since(last);
            last = (*t).max(last);
            (*l, d)
        })
        .collect()
}

/// CLOCK_MONOTONIC in milliseconds: the same clock in every process, so pocket-ui can hand the moment a game was
/// asked for to pocket-core (`LAUNCHED_AT`) and pocket-core can tell how long it took to its first frame.
pub fn clock_ms() -> f64 {
    let mut ts = libc::timespec { tv_sec: 0, tv_nsec: 0 };
    // SAFETY: clock_gettime only writes the timespec it is given.
    unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut ts) };
    ts.tv_sec as f64 * 1000.0 + ts.tv_nsec as f64 / 1e6
}

/// Environment variable with `clock_ms` at the moment the player started the game (pocket-ui sets it for pocket-core).
pub const LAUNCHED_AT: &str = "POCKET_LAUNCHED_AT";

/// Environment variable with the `clock_ms` until which pocket-core keeps its window back: Cozy's launch animation
/// (Settings > Sound & feel > Animations) plays over the shelf meanwhile. Unset in Quick.
pub const HOLD_UNTIL: &str = "POCKET_LAUNCH_HOLD_UNTIL";

/// pocket-core's window is still to be held back (`HOLD_UNTIL`). A deadline further away than the longest hold pocket-ui
/// asks for is a mistake, not a hold.
pub fn held() -> bool {
    let Some(until) = std::env::var(HOLD_UNTIL).ok().and_then(|v| v.parse::<f64>().ok()) else { return false };
    let left = until - clock_ms();
    left > 0.0 && left <= MAX_HOLD_MS
}

/// The longest hold pocket-ui asks for (Cozy's fade has run by then).
pub const MAX_HOLD_MS: f64 = 2000.0;

/// Environment variable with a path: once it exists the launch animation was skipped and the window stops waiting. A
/// button pressed before pocket-core has its pads open is seen by pocket-ui alone.
pub const HOLD_SKIP: &str = "POCKET_LAUNCH_HOLD_SKIP";

/// Where [`HOLD_SKIP`] points, on the runtime's tmpfs: nothing from an earlier start is left there ([`prepare_skip`]).
pub fn skip_path() -> std::path::PathBuf {
    std::env::temp_dir().join("pocket-launch-skip")
}

/// Before a game with a hold starts.
pub fn prepare_skip() {
    let _ = std::fs::remove_file(skip_path());
}

/// pocket-ui: the player skipped the launch animation, the window needn't wait.
pub fn skip_hold() {
    let _ = std::fs::File::create(skip_path());
}

/// pocket-core: whether the player skipped it. One `stat` a frame, while it holds.
pub fn hold_skipped() -> bool {
    std::env::var_os(HOLD_SKIP).is_some_and(|p| std::path::Path::new(&p).exists())
}

/// Whether a button went down since the last call. Buttons already down at the first call don't count: the finger still
/// on the A that started the game. A button let go and pressed again does.
pub fn new_press<const N: usize>(now: &[u16; N], before: &mut Option<[u16; N]>) -> bool {
    let Some(b) = before else {
        *before = Some(*now);
        return false;
    };
    let pressed = now.iter().zip(b.iter()).any(|(n, b)| n & !b != 0);
    *b = *now;
    pressed
}

/// Milliseconds since `LAUNCHED_AT`, if pocket-ui set it.
pub fn since_launch_ms() -> Option<f64> {
    let at: f64 = std::env::var(LAUNCHED_AT).ok()?.parse().ok()?;
    Some(clock_ms() - at)
}

/// `POCKETUI_STARTUP_PROBE=1`: [`preload_display_libs`] before the event loop, as a step of its own.
pub const PROBE_ENV: &str = "POCKETUI_STARTUP_PROBE";

/// Loads the libraries winit opens itself while it builds the event loop (libwayland-client, libxkbcommon), so that
/// the start's log line tells their loading ("display libs", a read from the card when the page cache is cold) from
/// the connection to sway and the rest ("event loop"). They stay loaded; winit's own `dlopen` then finds them. Only
/// with [`PROBE_ENV`]: a measurement aid, it changes nothing else.
#[cfg(target_os = "linux")]
pub fn preload_display_libs() {
    for lib in ["libwayland-client.so.0", "libxkbcommon.so.0"] {
        let t0 = Instant::now();
        // SAFETY: loads a system library as winit's dlopen of it does a moment later; nothing is called from it here.
        match unsafe { libloading::Library::new(lib) } {
            Ok(l) => {
                println!("startup: {lib} loaded in {:?}", t0.elapsed());
                // Never unloaded: the event loop uses it for the life of the process.
                std::mem::forget(l);
            }
            Err(e) => println!("startup: {lib} not loaded: {e}"),
        }
    }
}

/// Jobs that wait for the first frame: None once it was there (later jobs start at once).
type Job = Box<dyn FnOnce() + Send>;
static AFTER_FIRST_FRAME: Mutex<Option<Vec<(&'static str, Job)>>> = Mutex::new(Some(Vec::new()));

/// Runs `job` on a thread of its own once the first frame is on screen ([`first_frame_done`]), or at once when it was
/// already. For what the start used to begin before it that the first frame doesn't need: with a cold page cache
/// (every power-on) each program it starts (`nmcli`, `pactl`) and each file it reads waits for the card in the same
/// queue as the first frame's own reads.
pub fn after_first_frame(name: &'static str, job: impl FnOnce() + Send + 'static) {
    let mut jobs = AFTER_FIRST_FRAME.lock().unwrap_or_else(|e| e.into_inner());
    match jobs.as_mut() {
        Some(list) => list.push((name, Box::new(job))),
        None => {
            drop(jobs);
            spawn_job(name, Box::new(job));
        }
    }
}

/// The first frame is on screen (or won't come): every job [`after_first_frame`] holds starts now. Once.
pub fn first_frame_done() {
    let jobs = AFTER_FIRST_FRAME.lock().unwrap_or_else(|e| e.into_inner()).take().unwrap_or_default();
    for (name, job) in jobs {
        spawn_job(name, job);
    }
}

fn spawn_job(name: &'static str, job: Job) {
    // A job is a closure run once; the slot lets it run here when no thread can be had.
    let slot = std::sync::Arc::new(Mutex::new(Some(job)));
    let theirs = slot.clone();
    let spawned = std::thread::Builder::new().name(name.into()).spawn(move || {
        if let Some(job) = theirs.lock().unwrap_or_else(|e| e.into_inner()).take() {
            job();
        }
    });
    if spawned.is_err() {
        if let Some(job) = slot.lock().unwrap_or_else(|e| e.into_inner()).take() {
            job();
        }
    }
}

/// A value read on a thread once the first frame is on screen (`start`), for what the first frame doesn't show
/// (Settings pages, the Store, the web companion). Used before that, or before its thread is done, it is read or
/// waited for right there, so it is never missing, only later; such an early use is logged
/// ("startup: … needed before the first frame"), which says the move gained nothing for that start.
pub struct Later<T> {
    name: &'static str,
    value: std::cell::OnceCell<T>,
    pending: std::cell::RefCell<Pending<T>>,
}

enum Pending<T> {
    Waiting(Box<dyn FnOnce() -> T + Send>),
    Running(std::thread::JoinHandle<T>),
    Done,
}

impl<T: Send + 'static> Later<T> {
    /// `load` runs on a thread at `start`, or here at the first use before that.
    pub fn new(name: &'static str, load: impl FnOnce() -> T + Send + 'static) -> Later<T> {
        Later { name, value: std::cell::OnceCell::new(), pending: std::cell::RefCell::new(Pending::Waiting(Box::new(load))) }
    }

    /// Already there (snapshots, tests).
    pub fn ready(name: &'static str, value: T) -> Later<T> {
        let value = std::cell::OnceCell::from(value);
        Later { name, value, pending: std::cell::RefCell::new(Pending::Done) }
    }

    /// Starts reading it on a thread of its own (once; nothing when it is there already).
    pub fn start(&self) {
        let mut p = self.pending.borrow_mut();
        if let Pending::Waiting(_) = &*p {
            let Pending::Waiting(load) = std::mem::replace(&mut *p, Pending::Done) else { unreachable!() };
            // A shared slot for the closure, so that a failed spawn can still run it here.
            let slot = std::sync::Arc::new(std::sync::Mutex::new(Some(load)));
            let theirs = slot.clone();
            let spawned = std::thread::Builder::new().name(format!("later-{}", self.name)).spawn(move || {
                let load = theirs.lock().unwrap_or_else(|e| e.into_inner()).take().expect("loaded once");
                load()
            });
            *p = match spawned {
                Ok(h) => Pending::Running(h),
                Err(_) => Pending::Waiting(slot.lock().unwrap_or_else(|e| e.into_inner()).take().expect("not started")),
            };
        }
    }

    /// The value once its thread is done, without waiting; None before.
    pub fn loaded(&self) -> Option<&T> {
        if self.value.get().is_none() && matches!(&*self.pending.borrow(), Pending::Running(h) if h.is_finished()) {
            return Some(self.get());
        }
        self.value.get()
    }

    /// The value: read now, or its thread waited for, if need be.
    pub fn get(&self) -> &T {
        self.value.get_or_init(|| match std::mem::replace(&mut *self.pending.borrow_mut(), Pending::Done) {
            Pending::Waiting(load) => {
                let t0 = Instant::now();
                let v = load();
                println!("startup: {} needed before the first frame, read in {:?}", self.name, t0.elapsed());
                v
            }
            Pending::Running(h) => {
                let t0 = Instant::now();
                let v = h.join().unwrap_or_else(|e| std::panic::resume_unwind(e));
                let waited = t0.elapsed();
                if waited >= Duration::from_millis(1) {
                    println!("startup: waited {waited:?} for {}", self.name);
                }
                v
            }
            Pending::Done => unreachable!("Later::{}: value taken", self.name),
        })
    }

    pub fn get_mut(&mut self) -> &mut T {
        self.get();
        self.value.get_mut().expect("just read")
    }
}

impl<T: Send + 'static> std::ops::Deref for Later<T> {
    type Target = T;
    fn deref(&self) -> &T {
        self.get()
    }
}

impl<T: Send + 'static> std::ops::DerefMut for Later<T> {
    fn deref_mut(&mut self) -> &mut T {
        self.get_mut()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_finger_on_the_start_button_is_no_new_press() {
        let mut before = None;
        assert!(!new_press(&[0b01u16, 0], &mut before), "A still down from the start");
        assert!(!new_press(&[0b01, 0], &mut before));
        assert!(new_press(&[0b11, 0], &mut before), "another button");
        assert!(!new_press(&[0b10, 0], &mut before), "letting go is no press");
        assert!(new_press(&[0b11, 0], &mut before), "A again");
        let mut before = None;
        assert!(!new_press(&[0u16, 0], &mut before));
        assert!(new_press(&[0, 1], &mut before), "the second player's too");
    }

    use super::*;

    #[test]
    fn jobs_wait_for_the_first_frame_and_later_ones_start_at_once() {
        use std::sync::mpsc::channel;
        let (tx, rx) = channel();
        let tx2 = tx.clone();
        after_first_frame("test-early", move || tx.send("early").unwrap());
        assert!(rx.recv_timeout(Duration::from_millis(50)).is_err(), "nothing before the first frame");
        first_frame_done();
        assert_eq!(rx.recv_timeout(Duration::from_secs(5)), Ok("early"));
        after_first_frame("test-late", move || tx2.send("late").unwrap());
        assert_eq!(rx.recv_timeout(Duration::from_secs(5)), Ok("late"));
    }

    #[test]
    fn later_is_read_on_its_thread_after_start_or_at_once_when_used_before() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;
        let reads = Arc::new(AtomicUsize::new(0));
        let r = reads.clone();
        let mut early = Later::new("early", move || {
            r.fetch_add(1, Ordering::SeqCst);
            std::thread::current().name().map(str::to_string)
        });
        assert_eq!(reads.load(Ordering::SeqCst), 0, "nothing is read before start or the first use");
        assert!(early.loaded().is_none());
        // Used before the first frame: read here, on this thread, once.
        let here = std::thread::current().name().map(str::to_string);
        assert_eq!(*early, here);
        early.start();
        assert_eq!(*early.get_mut(), here);
        assert_eq!(reads.load(Ordering::SeqCst), 1);

        let r = reads.clone();
        let late = Later::new("late", move || {
            r.fetch_add(1, Ordering::SeqCst);
            std::thread::current().name().map(str::to_string)
        });
        late.start();
        late.start();
        let t0 = Instant::now();
        while late.loaded().is_none() && t0.elapsed() < Duration::from_secs(5) {
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(late.loaded().cloned(), Some(Some("later-late".to_string())), "read on its own thread");
        assert_eq!(reads.load(Ordering::SeqCst), 2, "once each");
        assert_eq!(*Later::ready("there", 7), 7);
    }

    #[test]
    fn steps_are_measured_from_the_one_before() {
        let t0 = Instant::now();
        let marks = [("a", t0 + Duration::from_millis(5)), ("b", t0 + Duration::from_millis(12)), ("c", t0 + Duration::from_millis(12))];
        let s = steps(t0, &marks);
        assert_eq!(s, vec![("a", Duration::from_millis(5)), ("b", Duration::from_millis(7)), ("c", Duration::ZERO)]);
    }
}
