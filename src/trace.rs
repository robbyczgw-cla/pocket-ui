//! Latency trace for the speed guard (`scripts/latency.sh`, docs/LATENCY.md).
//!
//! Off unless `POCKETUI_TRACE=<file>` is set. Then pocket-ui and pocket-core (which inherits the variable) append
//! one line per event to that file:
//!
//! ```text
//! <CLOCK_MONOTONIC in microseconds> <program> <pid> <event> [detail...]
//! ```
//!
//! Both programs read the same clock, and so does the harness (Python's `time.monotonic()`), so an input the
//! harness writes and the frame that shows it can be subtracted directly. Each line is a single `write` to a file
//! opened with O_APPEND, so the two processes never interleave inside a line.
//!
//! Cost when off: [`on`] is one relaxed atomic load after the first call, and every call site checks it before
//! it formats anything. The first call reads the environment once.

use std::io::Write;
use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};
use std::sync::{Mutex, OnceLock};

/// 0 not looked yet, 1 off, 2 on.
static STATE: AtomicU8 = AtomicU8::new(0);
static FILE: OnceLock<Option<Mutex<std::fs::File>>> = OnceLock::new();
/// Pad events pocket-ui has taken in (presses and releases), so a frame can say which input it already shows.
static INPUTS: AtomicU64 = AtomicU64::new(0);

/// Whether tracing is on. Cheap enough for every frame and every input.
#[inline]
pub fn on() -> bool {
    match STATE.load(Ordering::Relaxed) {
        1 => false,
        2 => true,
        _ => init(),
    }
}

#[cold]
fn init() -> bool {
    let file = FILE.get_or_init(|| {
        let path = std::env::var_os("POCKETUI_TRACE").filter(|p| !p.is_empty())?;
        std::fs::OpenOptions::new().create(true).append(true).open(path).ok().map(Mutex::new)
    });
    STATE.store(if file.is_some() { 2 } else { 1 }, Ordering::Relaxed);
    file.is_some()
}

/// Microseconds on CLOCK_MONOTONIC (the clock `Instant` uses on Linux, and Python's `time.monotonic()`).
pub fn now_us() -> u64 {
    let mut ts = libc::timespec { tv_sec: 0, tv_nsec: 0 };
    // SAFETY: clock_gettime only writes the timespec it is given.
    unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut ts) };
    ts.tv_sec as u64 * 1_000_000 + ts.tv_nsec as u64 / 1_000
}

/// One trace line: `event` and what follows it. Call through [`mark!`](crate::mark) so nothing is formatted
/// while tracing is off.
pub fn write(event: &str, detail: std::fmt::Arguments) {
    let Some(Some(file)) = FILE.get() else { return };
    let line = format!("{} {} {} {event} {detail}\n", now_us(), program(), std::process::id());
    if let Ok(mut f) = file.lock() {
        let _ = f.write_all(line.as_bytes());
    }
}

fn program() -> &'static str {
    static NAME: OnceLock<String> = OnceLock::new();
    NAME.get_or_init(|| {
        std::env::args_os()
            .next()
            .and_then(|a| std::path::Path::new(&a).file_name().map(|n| n.to_string_lossy().into_owned()))
            .unwrap_or_else(|| "?".into())
    })
}

/// `step <what>`: one step inside a longer moment (a Quit until the shelf, the pause menu opening), so
/// `scripts/latency.py phases` can say where its time went. The `switch-time` lines of `fast_switch::mark` come
/// here too. Nothing happens while tracing is off.
pub fn step(what: &str) {
    if on() {
        write("step", format_args!("{what}"));
    }
}

/// pocket-ui took in a pad event; returns its number (from 1) for the trace line.
pub fn input() -> u64 {
    INPUTS.fetch_add(1, Ordering::Relaxed) + 1
}

/// The number of the last pad event taken in (0: none yet).
pub fn inputs() -> u64 {
    INPUTS.load(Ordering::Relaxed)
}

/// `mark!("event", "detail {}", x)`: a trace line when `POCKETUI_TRACE` is set, nothing (not even the formatting)
/// otherwise.
#[macro_export]
macro_rules! mark {
    ($event:expr) => {
        if $crate::trace::on() {
            $crate::trace::write($event, format_args!(""));
        }
    };
    ($event:expr, $($arg:tt)*) => {
        if $crate::trace::on() {
            $crate::trace::write($event, format_args!($($arg)*));
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_clock_goes_forward_in_microseconds() {
        let a = now_us();
        std::thread::sleep(std::time::Duration::from_millis(5));
        let b = now_us();
        assert!(b > a && b - a >= 4_000 && b - a < 1_000_000, "{a} {b}");
    }

    #[test]
    fn inputs_are_numbered_from_one() {
        let before = inputs();
        let n = input();
        assert!(n > before);
        assert!(inputs() >= n);
    }

    // The test binary never sets POCKETUI_TRACE: tracing stays off and a mark writes nothing.
    #[test]
    fn off_without_the_variable() {
        if std::env::var_os("POCKETUI_TRACE").is_none() {
            assert!(!on());
            mark!("test", "{}", 1);
        }
    }
}
