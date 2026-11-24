//! Where the time to the first shelf frame goes: `mark` notes a step of pocket-ui's start, `report` gives them as one
//! log line next to "first frame after" (docs/dev/start-time.md, Start time). A handful of `Instant`s in a mutex, written
//! only during the start; costs nothing measurable.

use std::sync::{Mutex, OnceLock, atomic::{AtomicBool, Ordering}};
use std::time::{Duration, Instant};

static MARKS: Mutex<Vec<(&'static str, Instant)>> = Mutex::new(Vec::new());
static N64_START: OnceLock<Instant> = OnceLock::new();
static N64_PRESENTED: AtomicBool = AtomicBool::new(false);

pub fn is_r36s_n64(system: &str) -> bool {
    cfg!(target_os = "linux") && system == "n64" && crate::device::current().model.is_some_and(|m| m.is_rk3326())
}

/// The usual summary only reaches the log after a swap. On the R36S, log N64 phases live, and sample Linux's
/// counters during a long wait so the report still has evidence if the player has to quit or reboot.
pub fn watch_r36s_n64(started: Instant) {
    if N64_START.set(started).is_err() { return; }
    let _ = std::thread::Builder::new().name("n64-start-watch".into()).spawn(move || {
        std::thread::sleep(Duration::from_secs(3));
        while n64_start_pending() && started.elapsed() < Duration::from_secs(120) {
            let (phase, since) = MARKS.lock().ok().and_then(|m| m.last().copied()).unwrap_or(("process start", started));
            let sample = n64_wait_sample(std::path::Path::new("/proc"));
            println!("n64 start: waiting phase={phase} phase_ms={} {sample}", since.elapsed().as_millis());
            std::thread::sleep(Duration::from_secs(5));
        }
    });
}

fn n64_wait_sample(proc: &std::path::Path) -> String {
    let stat = std::fs::read_to_string(proc.join("self/stat")).unwrap_or_default();
    let state = stat.rsplit_once(')').and_then(|(_, rest)| rest.split_whitespace().next())
        .filter(|s| matches!(*s, "R" | "S" | "D" | "Z" | "T" | "t" | "X" | "x" | "K" | "W" | "P" | "I")).unwrap_or("?");
    let io = std::fs::read_to_string(proc.join("self/io")).unwrap_or_default();
    let memory = std::fs::read_to_string(proc.join("meminfo")).unwrap_or_default();
    format!("main={state} rchar={} read_bytes={} mem_available_kib={}",
        proc_field(&io, "rchar:"), proc_field(&io, "read_bytes:"), proc_field(&memory, "MemAvailable:"))
}

fn proc_field<'a>(text: &'a str, key: &str) -> &'a str {
    text.lines().find_map(|l| l.strip_prefix(key)?.split_whitespace().next())
        .filter(|v| v.parse::<u64>().is_ok()).unwrap_or("?")
}

pub fn n64_start_pending() -> bool {
    N64_START.get().is_some() && !N64_PRESENTED.load(Ordering::Relaxed)
}

/// Only a successful swap containing the game's picture ends the watch (early blank swaps don't).
pub fn n64_game_presented() {
    if n64_start_pending() {
        mark("first game picture");
        N64_PRESENTED.store(true, Ordering::Relaxed);
    }
}

/// Notes that the step `label` of the start has just ended.
pub fn mark(label: &'static str) {
    let now = Instant::now();
    if let Ok(mut m) = MARKS.lock() {
        if m.len() < 64 {
            m.push((label, now));
        }
    }
    if let Some(started) = N64_START.get().filter(|_| n64_start_pending()) {
        println!("n64 start: {label} at {} ms", now.saturating_duration_since(*started).as_millis());
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

/// A slow-load signal on the runtime tmpfs, shared by pocket-core and pocket-ui.
pub fn loading_path() -> std::path::PathBuf {
    std::env::temp_dir().join("pocket-core.loading")
}

/// Keeps the UI informed if loading a core/game exceeds three seconds. Dropping the guard cancels the
/// worker immediately and removes its marker, including when loading fails.
pub struct LoadWatch {
    stop: Option<std::sync::mpsc::Sender<()>>,
    worker: Option<std::thread::JoinHandle<()>>,
}

pub fn watch_load() -> LoadWatch {
    watch_load_at(loading_path(), Duration::from_secs(3))
}

fn watch_load_at(path: std::path::PathBuf, delay: Duration) -> LoadWatch {
    let (stop, rx) = std::sync::mpsc::channel::<()>();
    let worker = std::thread::Builder::new().name("loading".into()).spawn(move || {
        if rx.recv_timeout(delay) == Err(std::sync::mpsc::RecvTimeoutError::Timeout) {
            let _ = std::fs::write(&path, std::process::id().to_string());
            let _ = rx.recv();
        }
        let _ = std::fs::remove_file(&path);
    }).ok();
    LoadWatch { stop: Some(stop), worker }
}

impl Drop for LoadWatch {
    fn drop(&mut self) {
        drop(self.stop.take());
        if let Some(worker) = self.worker.take() { let _ = worker.join(); }
    }
}

/// Environment variable with `clock_ms` at the moment the player started the game (pocket-ui sets it for pocket-core).
pub const LAUNCHED_AT: &str = "POCKET_LAUNCHED_AT";

/// Environment variable with the `clock_ms` until which pocket-core keeps its window back: Cozy's launch animation
/// (Settings > Look > Game start animation) plays over the shelf meanwhile. Unset in Quick.
pub const HOLD_UNTIL: &str = "POCKET_LAUNCH_HOLD_UNTIL";

/// pocket-core's window is still to be held back (`HOLD_UNTIL`). A visible reminder waits for a button; only an animation uses a deadline.
pub fn held() -> bool {
    // A slow or broken preview must never buy an invisible reminder. The UI opts into
    // this handshake; a core that reaches play before the card is ready simply starts as usual.
    if hold_still() && std::env::var_os(HOLD_READY).is_some_and(|p| !std::path::Path::new(&p).exists()) {
        return false;
    }
    let Some(until) = std::env::var(HOLD_UNTIL).ok().and_then(|v| v.parse::<f64>().ok()) else { return false };
    held_at(until, clock_ms(), hold_still())
}

/// `held` at `now` (`clock_ms`) for a deadline `until`.
pub fn held_at(until: f64, now: f64, still: bool) -> bool {
    let left = until - now;
    // A reminder remains readable even after a slow load or a long pause to read the note.
    still || (left > 0.0 && left <= 2000.0)
}

/// pocket-core: whether this refresh runs no frame of the game. Only in a hold that keeps the game still
/// (`HOLD_STILL`), and only once its moment is loaded (`resume_pending`: a 3D core loads it after its first frames,
/// which run hidden as in any hold).
pub fn stands_still(hold: bool, still: bool, resume_pending: bool) -> bool {
    hold && still && !resume_pending
}

/// Environment variable: "1" while the hold is "Where was I?"'s, the game stands still at its moment until a button ends the hold (no frame run, no sound), so nothing of it is missed while the card is read. Without it the game runs silently
/// behind the launch animation, as before.
pub const HOLD_STILL: &str = "POCKET_LAUNCH_HOLD_STILL";

/// Environment variable: "1" when the game stands still through a hold with a deadline, no button awaited: A on the
/// Cabinet's playing TV, whose picture grows to the full screen meanwhile; the game then goes on from that very frame
/// (`showcase`). Its sound comes with the game. Also an open case started on Cozy: its moment waits behind the scene.
pub const HOLD_PAUSE: &str = "POCKET_LAUNCH_HOLD_PAUSE";

/// pocket-core: this start's hold keeps the game still (`HOLD_PAUSE`).
pub fn hold_pause() -> bool {
    std::env::var_os(HOLD_PAUSE).is_some_and(|v| v == "1")
}

/// Optional file set by pocket-ui once the reminder has a decoded launch picture. Missing: no extra hold.
pub const HOLD_READY: &str = "POCKET_LAUNCH_HOLD_READY";

pub fn ready_path() -> std::path::PathBuf {
    skip_path().with_extension("ready")
}

/// pocket-core: the hold keeps the game still (`HOLD_STILL`).
pub fn hold_still() -> bool {
    std::env::var(HOLD_STILL).is_ok_and(|v| v == "1")
}

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
    let _ = std::fs::remove_file(ready_path());
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
    fn n64_wait_samples_survive_missing_and_damaged_proc_fields() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("self")).unwrap();
        let inputs: [(&str, &[u8], &str); 3] = [
            ("self/stat", b"234 (core) test) D 1 22 22 0 0 0\n", "main=D"),
            ("self/io", b"rchar: 2000\nread_bytes: 1000\n", "rchar=2000 read_bytes=1000"),
            ("meminfo", b"MemAvailable: 432100 kB\n", "mem_available_kib=432100"),
        ];
        for (name, seed, expected) in inputs {
            for (fault, bytes) in crate::file_faults::faults(seed) {
                let file = dir.path().join(name);
                if let Some(bytes) = bytes {
                    std::fs::write(&file, bytes).unwrap();
                } else {
                    std::fs::remove_file(&file).unwrap();
                }
                let sample = n64_wait_sample(dir.path());
                assert_eq!(sample.split_whitespace().count(), 4, "{name} {fault}: {sample}");
                if fault == "valid" { assert!(sample.contains(expected), "{sample}"); }
                if matches!(fault.as_str(), "missing" | "empty" | "encoding") {
                    let unknown = match name {
                        "self/stat" => "main=?",
                        "self/io" => "rchar=? read_bytes=?",
                        _ => "mem_available_kib=?",
                    };
                    assert!(sample.contains(unknown), "{name} {fault}: {sample}");
                }
            }
        }
    }

    #[test]
    #[ignore = "timing tier: scripts/test-timing.sh (serial)"]
    fn timing_a_blocked_load_signals_the_ui_and_finishing_or_failing_clears_it() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("loading");
        let guard = watch_load_at(path.clone(), Duration::from_millis(20));
        let until = Instant::now() + crate::testutil::timeout(Duration::from_secs(2));
        while !path.exists() && Instant::now() < until { std::thread::sleep(Duration::from_millis(5)); }
        assert!(path.exists(), "a blocked read tells the UI it is loading");
        drop(guard);
        assert!(!path.exists(), "completion or an error removes the loading note");
        drop(watch_load_at(path.clone(), Duration::from_secs(3)));
        assert!(!path.exists(), "a quick load never flashes a loading note");
    }

    #[test]
    fn where_was_i_holds_after_cozys_animation_and_a_still_hold_waits_for_the_moment() {
        for now in [1000.0, 5000.0, 60_000.0, 3_600_000.0] {
            assert!(held_at(2600.0, now, true), "the reminder waits for a button at {now} ms");
        }
        assert!(!held_at(999.0, 1000.0, false), "animation over");
        assert!(!held_at(3001.0, 1000.0, false), "ordinary launches keep the 2 s safety bound");
        assert!(held_at(2600.0, 1000.0, false), "Cozy's ordinary 1.6 s deadline still works");
        // Still only in "Where was I?"'s hold, and only once the moment is in.
        assert!(stands_still(true, true, false));
        assert!(!stands_still(true, false, false), "Cozy's hold alone: the game runs silently, as before");
        assert!(!stands_still(true, true, true), "a 3D core's moment not loaded yet: its first frames run");
        assert!(!stands_still(false, true, false), "a button ended it: the game plays");
    }

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
    #[ignore = "timing tier: scripts/test-timing.sh (serial)"]
    fn timing_jobs_wait_for_the_first_frame_and_later_ones_start_at_once() {
        use std::sync::mpsc::channel;
        let (tx, rx) = channel();
        let tx2 = tx.clone();
        after_first_frame("test-early", move || tx.send("early").unwrap());
        assert!(rx.recv_timeout(Duration::from_millis(50)).is_err(), "nothing before the first frame");
        first_frame_done();
        assert_eq!(rx.recv_timeout(crate::testutil::timeout(Duration::from_secs(5))), Ok("early"));
        after_first_frame("test-late", move || tx2.send("late").unwrap());
        assert_eq!(rx.recv_timeout(crate::testutil::timeout(Duration::from_secs(5))), Ok("late"));
    }

    #[test]
    #[ignore = "timing tier: scripts/test-timing.sh (serial)"]
    fn timing_later_is_read_on_its_thread_after_start_or_at_once_when_used_before() {
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
        while late.loaded().is_none() && t0.elapsed() < crate::testutil::timeout(Duration::from_secs(5)) {
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
