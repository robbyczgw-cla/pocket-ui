//! Night light: a warmer, less blue display in the evening, for the menus and for games (Settings → Look →
//! Night light, `docs/user/night-light.md`).
//!
//! It is not a second colour path. The display look (`display.rs`) is the one place that works out the final colours:
//! `Look::night` is the white point in kelvin while the night light is on (0 = off), and `Look::gains` /
//! `Look::ramps` / `Look::apply_soft` multiply it in with the temperature the player chose, so the two never fight.
//! The Gamma backend puts the ramps into the compositor (every game, RetroArch and the standalone emulators included),
//! the Soft backend tints pocket-ui's own frames and pocket-core's picture (`gamma.rs`, `overlay.rs`).
//!
//! This file has
//! - the choices (`Prefs`, saved in `night-light.json`) and the schedule: a fixed window in 30-minute steps, no
//!   location, no sunset. The functions are pure (`Prefs::level_at`, `Prefs::secs_to_switch`, `plan`) and tested
//!   with midnight and DST edges;
//! - the fade (`fade_levels`): the white point moves in mireds (what the eye follows evenly), eased, in 15 steps
//!   over 1.5 s, so a switch in the middle of a game is a slow change and never a jump;
//! - the thread (`Runner`) that makes it happen. It sleeps in `poll(2)` and wakes for exactly three reasons: the next
//!   switch time (a `CLOCK_REALTIME` timerfd that is also cancelled when the clock is set, which is what NTP does
//!   after boot, and that fires on resume from suspend), a poke (new choices, wake from sleep, a new time zone),
//!   and the next step while a fade runs. No polling timer. Each step goes out as one `emit(level)`; the App
//!   turns it into one event for the UI thread, which hands it to `Sys` (`Sys::set_night`). Nothing runs per frame.

use std::io::Read;
use std::os::fd::AsRawFd;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::appearance::{in_window, local_minute};

pub const FILE: &str = "night-light.json";

/// The schedule moves in these steps.
pub const STEP_MINUTES: u16 = 30;
const DAY_MINUTES: u16 = 24 * 60;
/// The white point of an unchanged display.
pub const NEUTRAL_KELVIN: u16 = 6500;
/// The white point of strength 1 to 5. Strength 3 is a lamp-lit room; 5 is candle-warm. (The display look's own
/// warmth reaches 3500 K at most; the two multiply, so both together can be warmer than either.)
pub const STRENGTH_KELVIN: [u16; 5] = [5600, 5000, 4400, 3800, 3200];
pub const STRENGTHS: u8 = STRENGTH_KELVIN.len() as u8;

/// A fade takes this long, in this many steps (`fade_levels`).
pub const FADE: Duration = Duration::from_millis(1500);
pub const FADE_STEPS: usize = 15;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    #[default]
    Off,
    On,
    Scheduled,
}

impl Mode {
    pub const ALL: [Mode; 3] = [Mode::Off, Mode::On, Mode::Scheduled];

    pub fn label(self) -> &'static str {
        match self {
            Mode::Off => "Off",
            Mode::On => "On",
            Mode::Scheduled => "Scheduled",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub struct Prefs {
    #[serde(default)]
    pub mode: Mode,
    /// Minute of the day the schedule switches on (21:00).
    #[serde(default = "default_from")]
    pub from: u16,
    /// Minute of the day it switches off (07:00); earlier than `from` runs over midnight.
    #[serde(default = "default_until")]
    pub until: u16,
    /// 1 ..= 5, `STRENGTH_KELVIN`.
    #[serde(default = "default_strength")]
    pub strength: u8,
}

fn default_from() -> u16 {
    21 * 60
}
fn default_until() -> u16 {
    7 * 60
}
fn default_strength() -> u8 {
    3
}

impl Default for Prefs {
    fn default() -> Self {
        Prefs {
            mode: Mode::Off,
            from: default_from(),
            until: default_until(),
            strength: default_strength(),
        }
    }
}

/// "21:00" for a minute of the day.
pub fn clock_label(minute: u16) -> String {
    format!("{:02}:{:02}", minute / 60 % 24, minute % 60)
}

/// "Slight" to "Very warm".
pub fn strength_label(strength: u8) -> &'static str {
    match strength.clamp(1, STRENGTHS) {
        1 => "Slight",
        2 => "Gentle",
        3 => "Medium",
        4 => "Warm",
        _ => "Very warm",
    }
}

impl Prefs {
    pub fn load(data_dir: &Path) -> Prefs {
        crate::fsutil::read_json_or_backup(&data_dir.join(FILE)).map(Prefs::clamped).unwrap_or_default()
    }

    pub fn save(&self, data_dir: &Path) -> bool {
        crate::fsutil::write_json(&data_dir.join(FILE), self).is_ok()
    }

    /// Times on the 30-minute grid inside the day, strength 1 to 5.
    pub fn clamped(mut self) -> Prefs {
        let snap = |m: u16| (m.min(DAY_MINUTES - 1) / STEP_MINUTES) * STEP_MINUTES;
        self.from = snap(self.from);
        self.until = snap(self.until);
        self.strength = self.strength.clamp(1, STRENGTHS);
        self
    }

    /// The white point of the night light when it is on.
    pub fn target_kelvin(&self) -> u16 {
        STRENGTH_KELVIN[self.strength.clamp(1, STRENGTHS) as usize - 1]
    }

    /// Whether the schedule has a window at all: `from == until` would be an empty one (or all day).
    pub fn has_window(&self) -> bool {
        self.from != self.until
    }

    /// Is the night light on at this minute of the day?
    pub fn on_at(&self, minute: u32) -> bool {
        match self.mode {
            Mode::Off => false,
            Mode::On => true,
            Mode::Scheduled => self.has_window() && in_window(minute, self.from as u32, self.until as u32),
        }
    }

    /// The white point to show at this minute of the day: 0 = unchanged.
    pub fn level_at(&self, minute: u32) -> u16 {
        if self.on_at(minute) {
            self.target_kelvin()
        } else {
            0
        }
    }

    /// Seconds from `second_of_day` to the next time `on_at` changes; None when nothing ever does (not Scheduled).
    /// Always at least 1: standing on a switch time the next one is meant.
    pub fn secs_to_switch(&self, second_of_day: u32) -> Option<u32> {
        if self.mode != Mode::Scheduled || !self.has_window() {
            return None;
        }
        let day = DAY_MINUTES as u32 * 60;
        [self.from, self.until]
            .iter()
            .map(|m| {
                let d = (*m as u32 * 60 + day - second_of_day % day) % day;
                if d == 0 {
                    day
                } else {
                    d
                }
            })
            .min()
    }

    /// The row's value on Settings → Look: "Off", "On", "21:00–07:00".
    pub fn summary(&self) -> String {
        match self.mode {
            Mode::Off => "Off".into(),
            Mode::On => "On".into(),
            Mode::Scheduled => format!("{}\u{2013}{}", clock_label(self.from), clock_label(self.until)),
        }
    }

    /// The page's line about what it does at this minute of the day.
    pub fn status(&self, minute: u32) -> String {
        match self.mode {
            Mode::Off => "The night light is off.".into(),
            Mode::On => "The night light is on.".into(),
            Mode::Scheduled if !self.has_window() => "Pick two different times.".into(),
            Mode::Scheduled if self.on_at(minute) => format!("On now, until {}.", clock_label(self.until)),
            Mode::Scheduled => format!("Off now, on from {}.", clock_label(self.from)),
        }
    }

    /// One step through the three modes.
    pub fn step_mode(&mut self, delta: i32) {
        let i = Mode::ALL.iter().position(|m| *m == self.mode).unwrap_or(0) as i32;
        self.mode = Mode::ALL[(i + delta).rem_euclid(Mode::ALL.len() as i32) as usize];
    }

    /// One 30-minute step of the switch-on time; it never lands on the switch-off time (that would be no window).
    pub fn step_from(&mut self, delta: i32) {
        self.from = step_minutes(self.from, delta, self.until);
    }

    pub fn step_until(&mut self, delta: i32) {
        self.until = step_minutes(self.until, delta, self.from);
    }

    /// Strength 1 to 5; it stops at the ends (a wrap from "Very warm" to "Slight" would be a jump).
    pub fn step_strength(&mut self, delta: i32) {
        self.strength = (self.strength as i32 + delta).clamp(1, STRENGTHS as i32) as u8;
    }
}

/// `m` moved by `delta` steps round the day, skipping `avoid`.
fn step_minutes(m: u16, delta: i32, avoid: u16) -> u16 {
    let step = |m: u16| ((m as i32 + delta.signum() * STEP_MINUTES as i32).rem_euclid(DAY_MINUTES as i32)) as u16;
    let next = step(m);
    if next == avoid {
        step(next)
    } else {
        next
    }
}

// ---- the schedule against the clock ---------------------------------------------------------------------------------

/// What the thread works out from the clock.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Plan {
    /// The white point to show now: 0 = unchanged.
    pub level: u16,
    /// Until the next switch (a timer), None when nothing is scheduled. At least one second.
    pub wake_in: Option<Duration>,
}

/// `unix` seconds in a zone whose offset east of UTC at a given moment is `offset_at(unix)`. The wake is corrected
/// for a change of the offset on the way (daylight saving): the switch is at 21:00 on the clock, not 24 h away.
pub fn plan(prefs: &Prefs, unix: i64, offset_at: &dyn Fn(i64) -> i32) -> Plan {
    let off = offset_at(unix);
    let local = unix + off as i64;
    let second = local.rem_euclid(86_400) as u32;
    let level = prefs.level_at(local_minute(unix, off));
    let wake_in = prefs.secs_to_switch(second).map(|d| {
        let guess = unix + d as i64;
        let corrected = d as i64 + off as i64 - offset_at(guess) as i64;
        Duration::from_secs(corrected.max(1) as u64)
    });
    Plan { level, wake_in }
}

/// Seconds since 1970, moved by `POCKETUI_CLOCK_SHIFT` like Automatic appearance (to try it without waiting).
pub fn clock_unix() -> i64 {
    crate::appearance::shifted_unix_now()
}

/// Seconds east of UTC at `unix`. Under the lock `apply_process_zone` takes to change `TZ`: no thread reads the
/// environment while another writes it.
fn offset_at(unix: i64) -> i32 {
    let _zone = crate::timezone::zone_lock();
    crate::appearance::utc_offset_now(unix)
}

/// The white point right now by the clock; at the start, before any thread runs. 0 = unchanged.
pub fn level_now(prefs: &Prefs) -> u16 {
    plan(prefs, clock_unix(), &offset_at).level
}

// ---- the fade ---------------------------------------------------------------------------------------------------------

fn mired(kelvin: u16) -> f32 {
    1_000_000.0 / if kelvin == 0 { NEUTRAL_KELVIN } else { kelvin } as f32
}

/// The white points on the way from `from` to `to` (0 = unchanged), `steps` of them, the last one exactly `to`.
/// Eased at both ends, evenly in mireds; steps that would show no difference are dropped, so a small change is a
/// short fade. `from == to`: nothing.
pub fn fade_levels(from: u16, to: u16, steps: usize) -> Vec<u16> {
    if from == to {
        return Vec::new();
    }
    let (a, b) = (mired(from), mired(to));
    let (from_k, to_k) = (if from == 0 { NEUTRAL_KELVIN } else { from }, if to == 0 { NEUTRAL_KELVIN } else { to });
    let mut out: Vec<u16> = Vec::with_capacity(steps);
    for i in 1..steps.max(1) {
        let t = i as f32 / steps as f32;
        let eased = t * t * (3.0 - 2.0 * t);
        let k = (1_000_000.0 / (a + (b - a) * eased)).round() as u16;
        // Rounded to 10 K: below that nobody sees the step.
        let k = (k + 5) / 10 * 10;
        if k != from_k && k != to_k && out.last() != Some(&k) {
            out.push(k);
        }
    }
    // The end is exactly `to`; for "unchanged" that is 0, not 6500 K.
    out.push(to);
    out
}

// ---- the thread -------------------------------------------------------------------------------------------------------

/// How long a step of a fade lasts.
fn step_every(fade: Duration, steps: usize) -> Duration {
    fade / steps.max(1) as u32
}

struct Shared {
    prefs: Mutex<Prefs>,
    /// Motion is off: a switch is made in one step.
    instant: AtomicBool,
}

/// The night light's thread and the ways to reach it.
pub struct Runner {
    shared: Arc<Shared>,
    poke: UnixStream,
}

impl Runner {
    /// Starts the thread. `shown` is the white point on the display now (what `level_now` said at the start), so
    /// nothing fades at the start. `emit(level)` is called from the thread for the first step of a change, every
    /// step of a fade and the last.
    pub fn start(prefs: Prefs, shown: u16, fade: Duration, emit: impl Fn(u16) + Send + 'static) -> std::io::Result<Runner> {
        let (poke, rx) = UnixStream::pair()?;
        poke.set_nonblocking(true)?;
        rx.set_nonblocking(true)?;
        let shared = Arc::new(Shared {
            prefs: Mutex::new(prefs),
            instant: AtomicBool::new(false),
        });
        let s = shared.clone();
        std::thread::Builder::new()
            .name("night-light".into())
            .spawn(move || run(&s, &rx, shown, fade, &emit))?;
        Ok(Runner { shared, poke })
    }

    /// New choices: the thread looks at the clock again.
    pub fn set_prefs(&self, prefs: Prefs) {
        *self.shared.prefs.lock().unwrap() = prefs;
        self.poke();
    }

    /// Look at the clock again (woke from sleep, a new time zone).
    pub fn poke(&self) {
        use std::io::Write;
        let _ = (&self.poke).write(&[1]);
    }

    /// Settings → Look → Motion: Off makes every switch one step.
    pub fn set_instant(&self, instant: bool) {
        self.shared.instant.store(instant, Ordering::Relaxed);
    }
}

fn pollfd(fd: i32) -> libc::pollfd {
    libc::pollfd { fd, events: libc::POLLIN, revents: 0 }
}

/// Waits for one of `fds` to become readable, at most `timeout` (None: for good). True when one did.
fn wait(fds: &mut [libc::pollfd], timeout: Option<Duration>) -> bool {
    let ms = timeout.map_or(-1, |d| d.as_millis().min(i32::MAX as u128) as i32);
    // SAFETY: fds is a valid slice of pollfd for the length given.
    unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, ms) > 0 }
}

fn drain(mut rx: &UnixStream) {
    let mut buf = [0u8; 64];
    while matches!(rx.read(&mut buf), Ok(n) if n > 0) {}
}

/// A `CLOCK_REALTIME` timer that fires at a wall-clock time, and is cancelled (it reads as fired) when the clock is
/// set. Other systems have none: the thread then waits with a timeout.
#[cfg(target_os = "linux")]
struct Timer(i32);

#[cfg(target_os = "linux")]
impl Timer {
    fn new() -> Option<Timer> {
        // SAFETY: timerfd_create returns a new descriptor or -1.
        let fd = unsafe { libc::timerfd_create(libc::CLOCK_REALTIME, libc::TFD_CLOEXEC | libc::TFD_NONBLOCK) };
        (fd >= 0).then_some(Timer(fd))
    }

    fn fd(&self) -> Option<i32> {
        Some(self.0)
    }

    /// Fires `wake_in` from now (on the real clock); None: never.
    fn arm(&self, wake_in: Option<Duration>) {
        let at = wake_in.map(|d| {
            let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
            now + d
        });
        let spec = libc::itimerspec {
            it_interval: libc::timespec { tv_sec: 0, tv_nsec: 0 },
            it_value: libc::timespec {
                tv_sec: at.map_or(0, |a| a.as_secs() as libc::time_t),
                tv_nsec: at.map_or(0, |a| a.subsec_nanos() as _),
            },
        };
        let flags = if at.is_some() { libc::TFD_TIMER_ABSTIME | libc::TFD_TIMER_CANCEL_ON_SET } else { 0 };
        // SAFETY: the descriptor is ours and spec is a valid itimerspec.
        unsafe { libc::timerfd_settime(self.0, flags, &spec, std::ptr::null_mut()) };
    }

    fn drain(&self) {
        let mut buf = [0u8; 8];
        // A fired timer reads 8 bytes, a cancelled one fails with ECANCELED: both are "look again".
        // SAFETY: reading into our own buffer from our own descriptor.
        unsafe { libc::read(self.0, buf.as_mut_ptr().cast(), buf.len()) };
    }
}

#[cfg(target_os = "linux")]
impl Drop for Timer {
    fn drop(&mut self) {
        // SAFETY: the descriptor is ours.
        unsafe { libc::close(self.0) };
    }
}

#[cfg(not(target_os = "linux"))]
struct Timer;

#[cfg(not(target_os = "linux"))]
impl Timer {
    fn new() -> Option<Timer> {
        None
    }
    fn fd(&self) -> Option<i32> {
        None
    }
    fn arm(&self, _: Option<Duration>) {}
    fn drain(&self) {}
}

fn run(shared: &Shared, rx: &UnixStream, mut shown: u16, fade: Duration, emit: &dyn Fn(u16)) {
    let timer = Timer::new();
    loop {
        let prefs = *shared.prefs.lock().unwrap();
        let plan = plan(&prefs, clock_unix(), &offset_at);
        if plan.level != shown {
            let steps = if shared.instant.load(Ordering::Relaxed) || fade.is_zero() { Vec::new() } else { fade_levels(shown, plan.level, FADE_STEPS) };
            let steps = if steps.is_empty() { vec![plan.level] } else { steps };
            let every = step_every(fade, FADE_STEPS);
            let n = steps.len();
            for (i, level) in steps.into_iter().enumerate() {
                shown = level;
                emit(level);
                // Between the steps; a poke (new choices) ends the fade here and the loop starts over from `shown`.
                if i + 1 < n && wait(&mut [pollfd(rx.as_raw_fd())], Some(every)) {
                    drain(rx);
                    break;
                }
            }
            // Whether it ran to its end or a poke ended it: look at the clock and the choices again.
            continue;
        }
        if let Some(t) = &timer {
            t.arm(plan.wake_in);
        }
        let mut fds = vec![pollfd(rx.as_raw_fd())];
        if let Some(fd) = timer.as_ref().and_then(Timer::fd) {
            fds.push(pollfd(fd));
        }
        // Without a timer fd the wait itself ends at the switch (and at least every half minute).
        let timeout = if fds.len() > 1 { None } else { Some(plan.wake_in.map_or(Duration::from_secs(30), |d| d.min(Duration::from_secs(30)))) };
        wait(&mut fds, timeout);
        drain(rx);
        if let Some(t) = &timer {
            t.drain();
        }
    }
}

// ---- the one night light of the process --------------------------------------------------------------------------------

static RUNNER: OnceLock<Runner> = OnceLock::new();

/// Starts the process's night light thread (once). `emit` gets each white point to show.
pub fn start(prefs: Prefs, shown: u16, emit: impl Fn(u16) + Send + 'static) {
    if let Ok(r) = Runner::start(prefs, shown, FADE, emit) {
        let _ = RUNNER.set(r);
    }
}

pub fn set_prefs(prefs: Prefs) {
    if let Some(r) = RUNNER.get() {
        r.set_prefs(prefs);
    }
}

/// The clock may have moved on without a timer saying so: woke from sleep, a new time zone. Does nothing before
/// `start` and without a night light that could change.
pub fn poke() {
    if let Some(r) = RUNNER.get() {
        r.poke();
    }
}

pub fn set_instant(instant: bool) {
    if let Some(r) = RUNNER.get() {
        r.set_instant(instant);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;
    use std::sync::mpsc;

    fn scheduled(from: u16, until: u16) -> Prefs {
        Prefs {
            mode: Mode::Scheduled,
            from,
            until,
            strength: 3,
        }
    }

    fn utc(_: i64) -> i32 {
        0
    }

    #[test]
    fn modes_and_the_window_over_midnight() {
        let s = scheduled(21 * 60, 7 * 60);
        assert!(s.on_at(21 * 60) && s.on_at(23 * 60 + 59) && s.on_at(0) && s.on_at(6 * 60 + 59));
        assert!(!s.on_at(7 * 60) && !s.on_at(12 * 60) && !s.on_at(20 * 60 + 59));
        // A window inside one day.
        let day = scheduled(9 * 60, 17 * 60);
        assert!(day.on_at(9 * 60) && !day.on_at(17 * 60) && !day.on_at(8 * 60 + 59));
        assert!(Prefs { mode: Mode::On, ..s }.on_at(12 * 60));
        assert!(!Prefs { mode: Mode::Off, ..s }.on_at(23 * 60));
        // No window, no night light (never "all day" by accident).
        assert!(!scheduled(8 * 60, 8 * 60).on_at(8 * 60));
        assert_eq!(s.level_at(22 * 60), 4400);
        assert_eq!(s.level_at(12 * 60), 0);
    }

    #[test]
    fn the_next_switch_wraps_midnight() {
        let s = scheduled(21 * 60, 7 * 60);
        let h = |hh: u32, mm: u32, ss: u32| hh * 3600 + mm * 60 + ss;
        assert_eq!(s.secs_to_switch(h(12, 0, 0)), Some(9 * 3600), "noon: on at 21:00");
        assert_eq!(s.secs_to_switch(h(21, 0, 0)), Some(10 * 3600), "standing on 21:00: off at 07:00");
        assert_eq!(s.secs_to_switch(h(23, 30, 0)), Some(7 * 3600 + 1800), "over midnight to 07:00");
        assert_eq!(s.secs_to_switch(h(0, 0, 0)), Some(7 * 3600));
        assert_eq!(s.secs_to_switch(h(6, 59, 59)), Some(1));
        assert_eq!(s.secs_to_switch(h(7, 0, 0)), Some(14 * 3600), "standing on 07:00: on at 21:00");
        assert_eq!(s.secs_to_switch(h(20, 59, 59)), Some(1));
        // A window that ends at midnight.
        let late = scheduled(22 * 60, 0);
        assert_eq!(late.secs_to_switch(h(23, 0, 0)), Some(3600));
        assert!(late.on_at(23 * 60) && !late.on_at(0));
        // Nothing to wait for.
        assert_eq!(Prefs::default().secs_to_switch(0), None);
        assert_eq!(Prefs { mode: Mode::On, ..s }.secs_to_switch(0), None);
        assert_eq!(scheduled(60, 60).secs_to_switch(0), None);
    }

    #[test]
    fn the_plan_reads_the_clock_in_the_zone() {
        let s = scheduled(21 * 60, 7 * 60);
        // 1970-01-02 20:59:30 UTC; in a zone 2 h east it is 22:59:30: on, and off in 8 h 0 min 30 s.
        let t = 86_400 + 20 * 3600 + 59 * 60 + 30;
        let p = plan(&s, t, &utc);
        assert_eq!((p.level, p.wake_in), (0, Some(Duration::from_secs(30))));
        let east = |_| 2 * 3600;
        let p = plan(&s, t, &east);
        assert_eq!((p.level, p.wake_in), (4400, Some(Duration::from_secs(8 * 3600 + 30))));
        // The zone switches to daylight time an hour before the wake: the clock then reads one hour more, so the
        // wake is an hour sooner than 24 h arithmetic says.
        let noon = 86_400 + 12 * 3600;
        let dst = |u: i64| if u >= noon + 3600 { 3600 } else { 0 };
        let p = plan(&s, noon, &dst);
        assert_eq!(p.level, 0);
        assert_eq!(p.wake_in, Some(Duration::from_secs(9 * 3600 - 3600)));
        // Never zero, even when the correction would overshoot.
        let odd = |u: i64| if u > noon { -86_400 * 2 } else { 0 };
        assert!(plan(&s, noon, &odd).wake_in.unwrap() >= Duration::from_secs(1));
    }

    #[test]
    fn the_strengths_get_warmer_and_the_labels_are_calm() {
        assert!(STRENGTH_KELVIN.windows(2).all(|w| w[1] < w[0]));
        assert!(STRENGTH_KELVIN[0] < NEUTRAL_KELVIN);
        let names: Vec<_> = (1..=5).map(strength_label).collect();
        assert_eq!(names, ["Slight", "Gentle", "Medium", "Warm", "Very warm"]);
        assert_eq!(strength_label(0), "Slight");
        assert_eq!(strength_label(9), "Very warm");
    }

    #[test]
    fn steps_stay_on_the_grid_and_skip_the_other_end() {
        let mut p = scheduled(21 * 60, 7 * 60);
        p.step_from(1);
        assert_eq!(p.from, 21 * 60 + 30);
        p.step_from(-1);
        p.step_from(-1);
        assert_eq!(p.from, 20 * 60 + 30);
        // Round the day, and past the other end.
        p.from = 23 * 60 + 30;
        p.step_from(1);
        assert_eq!(p.from, 0);
        p.from = 6 * 60 + 30;
        p.step_from(1);
        assert_eq!(p.from, 7 * 60 + 30, "07:00 is the switch-off: skipped");
        p.from = 7 * 60;
        p.until = 7 * 60 + 30;
        p.step_until(-1);
        assert_eq!(p.until, 6 * 60 + 30, "07:00 is the switch-on now");
        assert!(p.has_window());
        p.from = 0;
        p.until = 0;
        p.step_until(-1);
        assert_eq!(p.until, 23 * 60 + 30);
        // Strength stops at its ends.
        let mut q = Prefs::default();
        q.step_strength(-9);
        assert_eq!(q.strength, 1);
        q.step_strength(9);
        assert_eq!(q.strength, 5);
        // Modes go round.
        q.step_mode(-1);
        assert_eq!(q.mode, Mode::Scheduled);
        q.step_mode(1);
        assert_eq!(q.mode, Mode::Off);
    }

    #[test]
    fn labels_and_summary() {
        assert_eq!(clock_label(21 * 60), "21:00");
        assert_eq!(clock_label(6 * 60 + 30), "06:30");
        assert_eq!(Prefs::default().summary(), "Off");
        assert_eq!(scheduled(21 * 60, 7 * 60).summary(), "21:00\u{2013}07:00");
        assert_eq!(Prefs { mode: Mode::On, ..Prefs::default() }.summary(), "On");
    }

    #[test]
    fn the_status_line_says_what_happens_now() {
        let s = scheduled(21 * 60, 7 * 60);
        assert_eq!(s.status(22 * 60), "On now, until 07:00.");
        assert_eq!(s.status(12 * 60), "Off now, on from 21:00.");
        assert_eq!(scheduled(60, 60).status(0), "Pick two different times.");
        assert_eq!(Prefs::default().status(0), "The night light is off.");
        assert_eq!(Prefs { mode: Mode::On, ..s }.status(0), "The night light is on.");
    }

    #[test]
    fn the_file_roundtrips_and_a_wrong_one_is_pulled_in() {
        let dir = TestDir::new("night");
        assert_eq!(Prefs::load(&dir), Prefs::default(), "no file: off, 21:00 to 07:00, medium");
        let p = scheduled(22 * 60 + 30, 6 * 60 + 30);
        assert!(p.save(&dir));
        assert_eq!(Prefs::load(&dir), p);
        std::fs::write(dir.join(FILE), r#"{"mode":"scheduled","from":1301,"until":60000,"strength":9}"#).unwrap();
        let l = Prefs::load(&dir);
        assert_eq!((l.mode, l.from, l.until, l.strength), (Mode::Scheduled, 21 * 60 + 30, 23 * 60 + 30, 5));
        std::fs::write(dir.join(FILE), r#"{"mode":"on"}"#).unwrap();
        assert_eq!(Prefs::load(&dir), Prefs { mode: Mode::On, ..Prefs::default() }, "missing fields: the defaults");
    }

    #[test]
    fn a_fade_is_even_in_mireds_eased_and_ends_exactly() {
        let on = fade_levels(0, 4400, FADE_STEPS);
        assert_eq!(*on.last().unwrap(), 4400);
        assert!(on.len() >= 10 && on.len() <= FADE_STEPS, "{on:?}");
        assert!(on.windows(2).all(|w| w[1] < w[0]), "warmer at every step: {on:?}");
        assert!(on[0] > 6000, "starts gently: {on:?}");
        assert!(on.iter().all(|k| (4400..NEUTRAL_KELVIN).contains(k)), "{on:?}");
        // Eased: the first step moves less than a middle one.
        let m = |k: u16| 1_000_000.0 / k as f32;
        assert!(m(on[0]) - m(NEUTRAL_KELVIN) < m(on[on.len() / 2]) - m(on[on.len() / 2 - 1]), "{on:?}");
        let off = fade_levels(3200, 0, FADE_STEPS);
        assert_eq!(*off.last().unwrap(), 0, "ends at unchanged, not at 6500 K");
        assert!(off[..off.len() - 1].windows(2).all(|w| w[1] > w[0]) && off[0] > 3200, "{off:?}");
        // Strength 3 to 5 while on: only the difference is faded.
        let more = fade_levels(4400, 3200, FADE_STEPS);
        assert_eq!(*more.last().unwrap(), 3200);
        assert!(more.iter().all(|k| (3200..=4400).contains(k)));
        assert!(fade_levels(4400, 4400, FADE_STEPS).is_empty());
        assert_eq!(fade_levels(0, 5600, 1), vec![5600], "one step: the end");
    }

    #[test]
    fn a_thread_fades_on_a_poke_and_follows_new_choices() {
        let (tx, rx) = mpsc::channel();
        // Off at the start and nothing shown: nothing is sent.
        let run = Runner::start(Prefs::default(), 0, Duration::from_millis(150), move |l| {
            let _ = tx.send(l);
        })
        .unwrap();
        assert!(rx.recv_timeout(Duration::from_millis(200)).is_err(), "nothing changes by itself");
        run.set_prefs(Prefs { mode: Mode::On, strength: 4, ..Prefs::default() });
        let mut got = Vec::new();
        while let Ok(l) = rx.recv_timeout(Duration::from_secs(1)) {
            got.push(l);
            if l == 3800 {
                break;
            }
        }
        assert_eq!(got.last(), Some(&3800), "{got:?}");
        assert!(got.len() > 3 && got.windows(2).all(|w| w[1] < w[0]), "a fade, warmer at each step: {got:?}");
        // Motion off: one step.
        run.set_instant(true);
        run.set_prefs(Prefs { mode: Mode::Off, ..Prefs::default() });
        assert_eq!(rx.recv_timeout(Duration::from_secs(1)), Ok(0));
        assert!(rx.recv_timeout(Duration::from_millis(300)).is_err(), "and nothing else");
        // A poke with nothing changed does nothing.
        run.poke();
        assert!(rx.recv_timeout(Duration::from_millis(300)).is_err());
        // A new choice in the middle of a fade turns it round from where it is.
        run.set_instant(false);
        run.set_prefs(Prefs { mode: Mode::On, strength: 5, ..Prefs::default() });
        let first = rx.recv_timeout(Duration::from_secs(1)).unwrap();
        run.set_prefs(Prefs { mode: Mode::Off, ..Prefs::default() });
        let mut last = first;
        while let Ok(l) = rx.recv_timeout(Duration::from_millis(600)) {
            last = l;
            if l == 0 {
                break;
            }
        }
        assert_eq!(last, 0, "it ends where the last choice says");
    }
}
