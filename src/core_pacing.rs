//! How many core frames a display refresh runs, Linux's rule for a player that is woken by the screen.
//!
//! pocket-core (Linux) runs a core once per display refresh when the core's rate is within 5 % of the screen's (a SNES at
//! 60.10 fps on a 60 Hz screen), and feeds its sound at the display rate so the pitch stays right (`audio_skew`); further
//! off (50 Hz games, 75 Hz arcade boards) frames are scheduled by time. Android's player is called once per refresh by
//! `GLSurfaceView`, but until 17.11.2026 it ran the core by the wall clock against the core's own rate: the deadline
//! drifted through the refresh (one frame in ten seconds for a SNES), and while it stood near the moment the call came,
//! the call's jitter of a millisecond decided between no core frame and two. Seconds of repeated and skipped pictures
//! every ten seconds, and the "two" refreshes ran bsnes twice in one refresh (Retroid Pocket Mini V2, 0.5.0-android.5:
//! "SNES lags"). [`Pacer`] counts refreshes instead, on a grid that follows the calls, so a late or early call by less
//! than half a refresh is still one refresh.
//!
//! [`Stats`] is the player's `player pacing:` log line: repeated and doubled pictures, the core's time per frame and the
//! gaps between calls, for a measurement on the device without a debug build.

use std::time::{Duration, Instant};

/// The band in which the core runs once per refresh (Linux's `sync_audio_rate`).
pub const LOCK_BAND: std::ops::Range<f64> = 0.95..1.05;
/// At most this many core frames in one refresh (a compositor far below the core's rate).
pub const MAX_PER_REFRESH: u32 = 4;

/// Whether the core runs once per display refresh: its rate within 5 % of the display's.
pub fn locked(core_fps: f64, display_hz: f64) -> bool {
    core_fps > 0.0 && display_hz > 0.0 && LOCK_BAND.contains(&(display_hz / core_fps))
}

/// The factor the core's sample rate is multiplied by: a core run once per refresh plays its samples at the display's
/// pace (Linux's `skew`); 1.0 when frames go by time.
pub fn audio_skew(core_fps: f64, display_hz: f64) -> f64 {
    if locked(core_fps, display_hz) {
        display_hz / core_fps
    } else {
        1.0
    }
}

/// The CPU the calling thread runs on (the player's line: a big or a little core), -1 where unknown.
pub fn current_cpu() -> i32 {
    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        // SAFETY: sched_getcpu takes no arguments and only reads the calling thread's CPU.
        unsafe { libc::sched_getcpu() }
    }
    #[cfg(not(any(target_os = "linux", target_os = "android")))]
    {
        -1
    }
}

/// The refresh grid and, for cores off the display's rate, the core time owed.
#[derive(Debug, Clone)]
pub struct Pacer {
    core: f64,
    refresh: f64,
    /// Where the last counted refresh was, in the call's time; follows the calls slowly.
    grid: Option<Instant>,
    owed: f64,
}

impl Pacer {
    pub fn new(core_fps: f64, display_hz: f64) -> Self {
        let mut p = Pacer { core: 60.0, refresh: 60.0, grid: None, owed: 0.0 };
        p.set_rates(core_fps, display_hz);
        p
    }

    /// A core that changes its rate, or a display that does.
    pub fn set_rates(&mut self, core_fps: f64, display_hz: f64) {
        self.core = core_fps.clamp(10.0, 240.0);
        self.refresh = display_hz.clamp(10.0, 480.0);
    }

    pub fn locked(&self) -> bool {
        locked(self.core, self.refresh)
    }

    /// After a pause, a menu or a reminder: the next call runs one frame and starts the grid again.
    pub fn restart(&mut self) {
        self.grid = None;
        self.owed = 0.0;
    }

    /// The core frames due at this call. One per refresh that passed since the last counted one (normally one; two after
    /// a missed refresh; none for a call that comes before half a refresh passed, as when the buffer queue fills at the
    /// start). Off the display's rate: the core's frames that fit in the refreshes that passed.
    pub fn due(&mut self, now: Instant) -> u32 {
        let period = 1.0 / self.refresh;
        let Some(grid) = self.grid else {
            self.grid = Some(now);
            self.owed = 0.0;
            return 1;
        };
        let since = now.saturating_duration_since(grid).as_secs_f64();
        let refreshes = (since / period + 0.5).floor();
        if refreshes < 1.0 {
            return 0;
        }
        // After a long stall (a second without calls) start again instead of running a burst.
        if refreshes > 60.0 {
            self.grid = Some(now);
            self.owed = 0.0;
            return 1;
        }
        let counted = grid + Duration::from_secs_f64(refreshes * period);
        // Follow the calls a little each time: the display's real rate is a little off the reported one.
        let drift = now.saturating_duration_since(counted).as_secs_f64() - counted.saturating_duration_since(now).as_secs_f64();
        let follow = drift * 0.1;
        self.grid = Some(if follow >= 0.0 { counted + Duration::from_secs_f64(follow) } else { counted - Duration::from_secs_f64(-follow) });
        let refreshes = refreshes as u32;
        if self.locked() {
            return refreshes.min(MAX_PER_REFRESH);
        }
        self.owed += refreshes as f64 * period;
        let frame = 1.0 / self.core;
        // A frame that is due exactly now is due (no rounding below it: that made a doubled picture next time).
        let n = (self.owed / frame + 1e-6).floor();
        self.owed -= n * frame;
        // Do not keep a debt beyond what one call can pay back.
        self.owed = self.owed.min(frame * MAX_PER_REFRESH as f64);
        (n as u32).min(MAX_PER_REFRESH)
    }
}

/// What the player measured since the last line: per call the core frames it ran, the core's time and the gap since the
/// previous call.
#[derive(Debug, Default, Clone)]
pub struct Stats {
    calls: u32,
    none: u32,
    doubled: u32,
    core_us: Vec<u32>,
    gap_us: Vec<u32>,
    last: Option<Instant>,
}

impl Stats {
    /// One call: `frames` core frames ran, taking `core` together.
    pub fn call(&mut self, now: Instant, frames: u32, core: Duration) {
        if let Some(last) = self.last {
            self.gap_us.push(now.saturating_duration_since(last).as_micros().min(u32::MAX as u128) as u32);
        }
        self.last = Some(now);
        self.calls += 1;
        if frames == 0 {
            self.none += 1;
        } else {
            if frames >= 2 {
                self.doubled += 1;
            }
            self.core_us.push((core.as_micros() / frames as u128).min(u32::MAX as u128) as u32);
        }
    }

    /// The pause: the gap over it is no gap between pictures.
    pub fn pause(&mut self) {
        self.last = None;
    }

    /// Calls since the last [`Stats::line`].
    pub fn calls(&self) -> u32 {
        self.calls
    }

    /// Repeated (no core frame) and doubled (two or more) pictures since the last line.
    pub fn repeated_and_doubled(&self) -> (u32, u32) {
        (self.none, self.doubled)
    }

    /// The log line's numbers, and a fresh count after it.
    pub fn line(&mut self, locked: bool, core_fps: f64, display_hz: f64, cpu: i32) -> String {
        let core = percentiles(&mut self.core_us);
        let gap = percentiles(&mut self.gap_us);
        let line = format!(
            "calls={} repeated={} doubled={} core_ms_p50={:.2} core_ms_p99={:.2} core_ms_max={:.2} gap_ms_p50={:.2} gap_ms_p99={:.2} gap_ms_max={:.2} mode={} core_fps={core_fps:.4} display_hz={display_hz:.2} cpu={cpu}",
            self.calls,
            self.none,
            self.doubled,
            core.0,
            core.1,
            core.2,
            gap.0,
            gap.1,
            gap.2,
            if locked { "per-refresh" } else { "by-time" },
        );
        let last = self.last;
        *self = Stats { last, ..Stats::default() };
        line
    }
}

/// p50, p99 and the largest, in ms.
fn percentiles(us: &mut [u32]) -> (f64, f64, f64) {
    if us.is_empty() {
        return (0.0, 0.0, 0.0);
    }
    us.sort_unstable();
    let at = |q: f64| us[((us.len() - 1) as f64 * q).round() as usize] as f64 / 1000.0;
    (at(0.5), at(0.99), *us.last().unwrap() as f64 / 1000.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A small deterministic jitter (no random crate): a call `jitter_us` early or late, in a pattern without a period
    /// near the drift's.
    fn jitter(i: u64, jitter_us: i64) -> i64 {
        let x = i.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((x >> 33) % (2 * jitter_us as u64 + 1)) as i64 - jitter_us
    }

    /// A minute of calls at `display_hz` with up to `jitter_us` of jitter each: (repeated, doubled, core frames run).
    fn minute(pacer: &mut dyn FnMut(Instant) -> u32, display_hz: f64, jitter_us: i64) -> (u32, u32, u64) {
        let start = Instant::now();
        let period_us = 1e6 / display_hz;
        let (mut none, mut two, mut frames) = (0, 0, 0u64);
        for i in 0..(display_hz * 60.0) as u64 {
            let at = (i as f64 * period_us) as i64 + 2_000 + jitter(i, jitter_us);
            let n = pacer(start + Duration::from_micros(at as u64));
            if n == 0 {
                none += 1;
            }
            if n >= 2 {
                two += 1;
            }
            frames += n as u64;
        }
        (none, two, frames)
    }

    /// The player's loop until 17.11.2026: run the core while the wall clock is past its deadline, at most four times.
    fn old_wall_clock(core_fps: f64) -> impl FnMut(Instant) -> u32 {
        let period = Duration::from_secs_f64(1.0 / core_fps);
        let mut next: Option<Instant> = None;
        move |now| {
            let next = next.get_or_insert(now);
            let mut ticks = 0;
            while now >= *next && ticks < 4 {
                ticks += 1;
                *next += period;
            }
            ticks
        }
    }

    /// Every console's NTSC/Japan rate our cores report (or near it): SNES and NES 60.10, Game Boy and GBA 59.73, Mega Drive
    /// 59.92, PlayStation 59.83 / 59.94 / 59.29 (interlaced), PC Engine 59.83, N64 and PSP 60.00 / 59.94, WonderSwan 75.47
    /// is off-rate (below), Neo Geo Pocket 60.25, Lynx 59.7 at its usual mode.
    const NEAR_60: &[f64] = &[60.0988, 59.7275, 59.9227, 59.826, 59.94, 59.29, 59.82, 60.0, 60.2534, 59.7];
    /// PAL games (SNES 50.007, Mega Drive 49.70, PlayStation 49.76, N64 50.0) and the cores far from 60 (WonderSwan 75.47,
    /// a 30 fps core).
    const OFF_60: &[f64] = &[50.007, 49.7015, 49.761, 50.0, 75.47, 30.0];

    /// The consoles near 60 Hz on the screens a handheld reports (60, 59.94, 60.10 Hz), with a millisecond of jitter in
    /// the calls: one core frame for every call, no repeated or doubled picture in a minute. The old loop repeated and
    /// doubled pictures in every case whose rates differ.
    #[test]
    fn near_the_display_rate_every_refresh_runs_one_core_frame() {
        let mut old_bad = 0;
        for &core_fps in NEAR_60 {
            for display_hz in [60.0, 59.94, 60.1] {
                let mut pacer = Pacer::new(core_fps, display_hz);
                let (none, two, frames) = minute(&mut |now| pacer.due(now), display_hz, 1_000);
                assert_eq!((none, two), (0, 0), "core {core_fps} on {display_hz} Hz: repeated {none}, doubled {two}");
                assert_eq!(frames, (display_hz * 60.0) as u64);
                let (none, two, _) = minute(&mut old_wall_clock(core_fps), display_hz, 1_000);
                if (core_fps - display_hz).abs() > 0.05 {
                    assert!(none + two > 10, "the old loop was smooth for core {core_fps} on {display_hz} Hz?");
                    old_bad += 1;
                }
            }
        }
        assert!(old_bad >= 10);
    }

    /// The SNES on the Mini V2's 60 Hz: the old loop's minute had dozens of repeated and doubled pictures.
    #[test]
    fn the_old_wall_clock_loop_judders_a_snes_on_a_60_hz_screen() {
        let (none, two, _) = minute(&mut old_wall_clock(60.0988), 60.0, 1_000);
        assert!(none >= 20 && two >= 20, "repeated {none}, doubled {two}");
        let mut pacer = Pacer::new(60.0988, 60.0);
        assert_eq!(minute(&mut |now| pacer.due(now), 60.0, 1_000).0, 0);
    }

    /// A missed refresh (a call two refreshes after the last) runs two core frames, so the game keeps its speed; a call
    /// right after another (the buffer queue filling) runs none.
    #[test]
    fn a_missed_refresh_runs_two_frames_and_an_early_call_none() {
        let start = Instant::now();
        let mut pacer = Pacer::new(60.0988, 60.0);
        let ms = |m: f64| start + Duration::from_secs_f64(m / 1000.0);
        assert_eq!(pacer.due(ms(0.0)), 1);
        assert_eq!(pacer.due(ms(1.0)), 0);
        assert_eq!(pacer.due(ms(16.7)), 1);
        assert_eq!(pacer.due(ms(50.0)), 2);
        assert_eq!(pacer.due(ms(66.7)), 1);
        // A long stall starts again with one frame, no burst.
        assert_eq!(pacer.due(ms(3000.0)), 1);
        assert_eq!(pacer.due(ms(3016.7)), 1);
    }

    /// PAL and other off-rate games on 60 Hz screens go by time, and their minute holds the core's own frames.
    #[test]
    fn off_the_display_rate_frames_go_by_time() {
        for &core_fps in OFF_60 {
            for display_hz in [60.0, 59.94, 60.1] {
                let mut pacer = Pacer::new(core_fps, display_hz);
                assert!(!pacer.locked());
                let (_, _, frames) = minute(&mut |now| pacer.due(now), display_hz, 1_000);
                let want = core_fps * 60.0;
                assert!((frames as f64 - want).abs() <= 2.0, "core {core_fps} on {display_hz}: {frames} frames, want {want}");
            }
        }
        assert!(locked(60.0988, 60.0) && locked(59.7275, 60.0) && !locked(50.007, 60.0) && !locked(75.47, 60.0));
        assert!((audio_skew(60.0988, 60.0) - 60.0 / 60.0988).abs() < 1e-12 && audio_skew(50.0, 60.0) == 1.0);
    }

    /// A PAL game on a 60 Hz screen keeps an even 5:6 pattern with the calls' jitter: five pictures, one repeated, never
    /// two core frames in one refresh and never two repeats closer than five refreshes (Linux: a repeated picture every
    /// sixth refresh).
    #[test]
    fn a_pal_game_on_a_60_hz_screen_repeats_every_sixth_picture_evenly() {
        for core_fps in [50.0, 50.007, 49.7015, 49.761] {
            let mut pacer = Pacer::new(core_fps, 60.0);
            let start = Instant::now();
            let mut last_repeat: Option<u64> = None;
            for i in 0..3600u64 {
                let at = (i as f64 * 1e6 / 60.0) as i64 + 2_000 + jitter(i, 1_500);
                let n = pacer.due(start + Duration::from_micros(at as u64));
                assert!(n <= 1, "core {core_fps}: {n} frames at refresh {i}");
                if n == 0 && i > 0 {
                    if let Some(before) = last_repeat {
                        let gap = i - before;
                        assert!((5..=7).contains(&gap), "core {core_fps}: repeats {gap} refreshes apart at {i}");
                        if core_fps == 50.0 {
                            assert_eq!(gap, 6, "exact 50 Hz: every sixth at {i}");
                        }
                    }
                    last_repeat = Some(i);
                }
            }
        }
    }

    #[test]
    fn stats_count_repeated_and_doubled_pictures_and_the_core_time() {
        let start = Instant::now();
        let mut stats = Stats::default();
        let at = |ms: u64| start + Duration::from_millis(ms);
        stats.call(at(0), 1, Duration::from_millis(5));
        stats.call(at(17), 0, Duration::ZERO);
        stats.call(at(33), 2, Duration::from_millis(12));
        assert_eq!(stats.repeated_and_doubled(), (1, 1));
        let line = stats.line(true, 60.0988, 60.0, 4);
        assert!(line.starts_with("calls=3 repeated=1 doubled=1 core_ms_p50=6.00"), "{line}");
        assert!(line.contains("gap_ms_max=17.00") && line.contains("mode=per-refresh") && line.contains("cpu=4"), "{line}");
        assert_eq!(stats.calls(), 0);
    }
}
