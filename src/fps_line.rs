//! Over the game → FPS: "60 FPS   CPU 23%" in the top left corner, twice a second. The words are Linux's
//! (`overlay::Hud::frame`, which formats with [`text`]); [`Counter`] is the count for Android's player, which has no `overlay`.
//!
//! FPS is the pictures the screen got in the last half second, EMU (only when it differs by more than a tenth) the frames the
//! core emulated, CPU the process's CPU time over the wall time (more than 100 % when it uses several cores, as on Linux).

use std::time::{Duration, Instant};

/// How often the line changes.
pub const EVERY: Duration = Duration::from_millis(500);

/// The line for `fps` pictures and `emulated` core frames a second and `cpu` per cent.
pub fn text(fps: f64, emulated: f64, cpu: f64) -> String {
    if (emulated - fps).abs() > fps * 0.1 {
        format!("{fps:.0} FPS   EMU {emulated:.0}   CPU {cpu:.0}%")
    } else {
        format!("{fps:.0} FPS   CPU {cpu:.0}%")
    }
}

/// Fields 14 and 15 of a /proc/<pid>/stat line, user and system time in clock ticks (the name in field 2 may contain
/// spaces and parentheses).
pub fn parse_ticks(stat: &str) -> Option<u64> {
    let rest = &stat[stat.rfind(')')? + 1..];
    let f: Vec<&str> = rest.split_whitespace().collect();
    Some(f.get(11)?.parse::<u64>().ok()? + f.get(12)?.parse::<u64>().ok()?)
}

/// This process's CPU time in clock ticks.
pub fn process_ticks() -> u64 {
    std::fs::read_to_string("/proc/self/stat").ok().and_then(|s| parse_ticks(&s)).unwrap_or(0)
}

/// Clock ticks a second.
pub fn ticks_per_second() -> f64 {
    // SAFETY: sysconf only reads a system constant.
    (unsafe { libc::sysconf(libc::_SC_CLK_TCK) }).max(1) as f64
}

/// The count behind the line: pictures shown, core frames, CPU ticks since the last line.
#[derive(Debug)]
pub struct Counter {
    since: Instant,
    frames: u32,
    emulated: u32,
    cpu_ticks: u64,
    line: String,
}

impl Default for Counter {
    fn default() -> Self {
        Counter { since: Instant::now(), frames: 0, emulated: 0, cpu_ticks: process_ticks(), line: String::new() }
    }
}

impl Counter {
    /// A picture went to the screen after `emulated` core frames. True when the line changed (paint it again).
    pub fn frame(&mut self, now: Instant, emulated: u32) -> bool {
        self.frames += 1;
        self.emulated += emulated;
        let elapsed = now.saturating_duration_since(self.since);
        if elapsed < EVERY {
            return false;
        }
        let ticks = process_ticks();
        let secs = elapsed.as_secs_f64();
        let cpu = ticks.saturating_sub(self.cpu_ticks) as f64 / ticks_per_second() / secs * 100.0;
        let line = text(self.frames as f64 / secs, self.emulated as f64 / secs, cpu);
        self.frames = 0;
        self.emulated = 0;
        self.cpu_ticks = ticks;
        self.since = now;
        let changed = line != self.line;
        self.line = line;
        changed
    }

    /// After a pause: the time paused is no time played.
    pub fn restart(&mut self) {
        *self = Counter { line: std::mem::take(&mut self.line), ..Counter::default() };
    }

    /// The line to draw ("" before the first half second).
    pub fn line(&self) -> &str {
        &self.line
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_line_is_linux_s() {
        assert_eq!(text(60.0, 60.0, 23.4), "60 FPS   CPU 23%");
        assert_eq!(text(60.0, 120.0, 80.0), "60 FPS   EMU 120   CPU 80%");
        assert_eq!(parse_ticks("123 (pocket (core) x) S 1 123 123 0 -1 4194560 100 0 0 0 250 30 0 0 20 0 5 0 1000 1000 1 18446744073709551615"), Some(280));
        assert_eq!(parse_ticks("garbage"), None);
    }

    /// Sixty pictures of one core frame each in a second read "60 FPS", twice a second, and the line only changes when it
    /// says something else.
    #[test]
    fn the_counter_says_the_pictures_a_second_twice_a_second() {
        let start = Instant::now();
        let mut c = Counter { since: start, ..Counter::default() };
        let mut changes = 0;
        for i in 1..=60u64 {
            if c.frame(start + Duration::from_micros(i * 16_667), 1) {
                changes += 1;
            }
        }
        assert!(changes >= 1, "{changes}");
        assert!(c.line().starts_with("60 FPS   CPU "), "{}", c.line());
        // A doubled core frame every picture shows the core's rate beside the screen's.
        for i in 61..=120u64 {
            c.frame(start + Duration::from_micros(i * 16_667), 2);
        }
        assert!(c.line().starts_with("60 FPS   EMU 120   CPU "), "{}", c.line());
    }
}
