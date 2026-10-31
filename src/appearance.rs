//! Settings > Look > Appearance: dark, light, or automatic by the time of day, and the short crossfade
//! between the two. The palettes themselves are in `palette`.
//!
//! Automatic is light from 07:00 to 20:00 local time and dark the rest of the day, at fixed times: the device
//! has no location, only a time zone, and a guess of sunrise from a zone name is not worth being wrong
//! about. Local time is the device clock in the zone chosen at first start (`localtime_r`).
//! It never switches under the player's hands: only when nothing was pressed for `QUIET`, or while a game
//! hides the menus.

use std::time::{Duration, Instant};

use crate::settings::Appearance;

/// Automatic turns light at this minute of the day (07:00) ...
pub const LIGHT_FROM: u32 = 7 * 60;
/// ... and dark again at this one (20:00).
pub const DARK_FROM: u32 = 20 * 60;
/// How long the crossfade takes.
pub const FADE: Duration = Duration::from_millis(600);
/// Automatic waits for this long without input before it switches a screen that is on view.
pub const QUIET: Duration = Duration::from_secs(8);

/// Minute of the day (0..1440) at `unix` seconds, `utc_offset` seconds east of UTC.
pub fn local_minute(unix: i64, utc_offset: i32) -> u32 {
    ((unix + utc_offset as i64).rem_euclid(86_400) / 60) as u32
}

/// True when `minute` lies in the window from `from` (included) to `to` (excluded); a window whose end is
/// before its start runs over midnight.
pub fn in_window(minute: u32, from: u32, to: u32) -> bool {
    if from <= to {
        (from..to).contains(&minute)
    } else {
        minute >= from || minute < to
    }
}

/// Whether `choice` means light at `unix` seconds in a zone `utc_offset` seconds east of UTC.
pub fn wants_light(choice: Appearance, unix: i64, utc_offset: i32) -> bool {
    match choice {
        Appearance::Dark => false,
        Appearance::Light => true,
        Appearance::Auto => in_window(local_minute(unix, utc_offset), LIGHT_FROM, DARK_FROM),
    }
}

/// Whether Automatic may switch now: the menus are hidden behind a game, or nobody pressed anything for `QUIET`.
pub fn may_switch(idle: Duration, on_view: bool) -> bool {
    !on_view || idle >= QUIET
}

/// Seconds east of UTC in the device's zone right now, as libc sees it (after `timezone::tzset`).
pub fn utc_offset_now(unix: i64) -> i32 {
    let t = unix as libc::time_t;
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    crate::timezone::tzset();
    // SAFETY: localtime_r only writes into the provided struct.
    if unsafe { libc::localtime_r(&t, &mut tm) }.is_null() {
        return 0;
    }
    tm.tm_gmtoff as i32
}

/// Seconds since 1970 as this device's clock says them, moved by `POCKETUI_CLOCK_SHIFT` (seconds, may be
/// negative) to try Automatic appearance and the night light on the virtual device without waiting for the evening.
pub fn shifted_unix_now() -> i64 {
    let shift = std::env::var("POCKETUI_CLOCK_SHIFT").ok().and_then(|s| s.trim().parse::<i64>().ok()).unwrap_or(0);
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64) + shift
}

/// Whether `choice` means light now on this device (`shifted_unix_now`).
pub fn wants_light_now(choice: Appearance) -> bool {
    if choice != Appearance::Auto {
        return choice == Appearance::Light;
    }
    let unix = shifted_unix_now();
    wants_light(choice, unix, utc_offset_now(unix))
}

/// Where the palette is between dark (0) and light (255), and where it is going.
#[derive(Clone, Copy, Debug)]
pub struct Fader {
    light: bool,
    from: u8,
    since: Option<Instant>,
}

impl Fader {
    /// Already at `light`, no fade.
    pub fn at(light: bool) -> Self {
        Fader { light, from: if light { 255 } else { 0 }, since: None }
    }

    pub fn light(&self) -> bool {
        self.light
    }

    /// Goes to `light` from wherever it is now, over `FADE`; `fade` false: at once.
    pub fn go(&mut self, light: bool, now: Instant, fade: bool) {
        if light == self.light && (fade || self.since.is_none()) {
            return;
        }
        self.from = if fade { self.value(now) } else if light { 255 } else { 0 };
        self.light = light;
        self.since = fade.then_some(now);
    }

    /// The blend at `now` (0 dark .. 255 light).
    pub fn value(&self, now: Instant) -> u8 {
        let to = if self.light { 255.0 } else { 0.0 };
        let Some(since) = self.since else { return to as u8 };
        // Settings > Sound & feel > Motion: Off switches at once.
        let fade = crate::motion::span(crate::motion::Kind::Fade, FADE);
        if fade.is_zero() {
            return to as u8;
        }
        let p = (now.saturating_duration_since(since).as_secs_f32() / fade.as_secs_f32()).min(1.0);
        // Ease in and out: no visible jump at either end.
        let e = p * p * (3.0 - 2.0 * p);
        (self.from as f32 + (to - self.from as f32) * e).round() as u8
    }

    pub fn animating(&self, now: Instant) -> bool {
        self.since.is_some_and(|s| now.saturating_duration_since(s) < crate::motion::span(crate::motion::Kind::Fade, FADE))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 2026-10-22 00:00 UTC.
    const DAY: i64 = 1_792_627_200;
    const H: i64 = 3600;
    const VIENNA_SUMMER: i32 = 2 * 3600;
    const VIENNA_WINTER: i32 = 3600;
    const NEW_YORK_WINTER: i32 = -5 * 3600;

    #[test]
    fn local_minute_follows_the_zone() {
        assert_eq!(local_minute(DAY + 6 * H + 30 * 60, 0), 6 * 60 + 30);
        assert_eq!(local_minute(DAY + 6 * H + 30 * 60, VIENNA_SUMMER), 8 * 60 + 30);
        assert_eq!(local_minute(DAY + 6 * H + 30 * 60, NEW_YORK_WINTER), 60 + 30);
        // Half-hour zones (India, +5:30).
        assert_eq!(local_minute(DAY, 5 * 3600 + 1800), 5 * 60 + 30);
    }

    #[test]
    fn local_minute_wraps_over_midnight_both_ways() {
        // 23:30 UTC is already 00:30 the next day in Vienna in winter ...
        assert_eq!(local_minute(DAY + 23 * H + 30 * 60, VIENNA_WINTER), 30);
        // ... and 02:00 UTC is still 21:00 the day before in New York.
        assert_eq!(local_minute(DAY + 2 * H, NEW_YORK_WINTER), 21 * 60);
        // Before 1970 too.
        assert_eq!(local_minute(-60, 0), 1439);
    }

    #[test]
    fn automatic_is_light_from_seven_to_eight_in_the_evening() {
        let at = |h: i64, m: i64| DAY + h * H + m * 60;
        assert!(!wants_light(Appearance::Auto, at(6, 59), 0));
        assert!(wants_light(Appearance::Auto, at(7, 0), 0));
        assert!(wants_light(Appearance::Auto, at(12, 0), 0));
        assert!(wants_light(Appearance::Auto, at(19, 59), 0));
        assert!(!wants_light(Appearance::Auto, at(20, 0), 0));
        assert!(!wants_light(Appearance::Auto, at(23, 59), 0));
        assert!(!wants_light(Appearance::Auto, at(0, 0), 0));
    }

    #[test]
    fn automatic_uses_the_local_time_not_utc() {
        // 05:30 UTC: 07:30 in Vienna in summer (light), 06:30 in winter (dark), 00:30 in New York (dark).
        let t = DAY + 5 * H + 30 * 60;
        assert!(wants_light(Appearance::Auto, t, VIENNA_SUMMER));
        assert!(!wants_light(Appearance::Auto, t, VIENNA_WINTER));
        assert!(!wants_light(Appearance::Auto, t, NEW_YORK_WINTER));
        // 18:30 UTC: 19:30 in Vienna in winter (still light), 20:30 in summer (dark).
        let t = DAY + 18 * H + 30 * 60;
        assert!(wants_light(Appearance::Auto, t, VIENNA_WINTER));
        assert!(!wants_light(Appearance::Auto, t, VIENNA_SUMMER));
    }

    #[test]
    fn dark_and_light_ignore_the_clock() {
        for h in 0..24 {
            assert!(!wants_light(Appearance::Dark, DAY + h * H, 0));
            assert!(wants_light(Appearance::Light, DAY + h * H, 0));
        }
    }

    #[test]
    fn a_window_can_run_over_midnight() {
        assert!(in_window(23 * 60, 20 * 60, 7 * 60));
        assert!(in_window(0, 20 * 60, 7 * 60));
        assert!(in_window(6 * 60 + 59, 20 * 60, 7 * 60));
        assert!(!in_window(7 * 60, 20 * 60, 7 * 60));
        assert!(!in_window(12 * 60, 20 * 60, 7 * 60));
        assert!(in_window(LIGHT_FROM, LIGHT_FROM, DARK_FROM));
        assert!(!in_window(DARK_FROM, LIGHT_FROM, DARK_FROM));
    }

    #[test]
    fn automatic_waits_for_a_quiet_moment() {
        assert!(!may_switch(Duration::from_secs(1), true), "not while someone is pressing buttons");
        assert!(may_switch(QUIET, true));
        assert!(may_switch(Duration::ZERO, false), "behind a game nobody sees it switch");
    }

    #[test]
    fn the_fade_runs_from_where_it_is_and_ends_at_the_target() {
        let t0 = Instant::now();
        let mut f = Fader::at(false);
        assert_eq!(f.value(t0), 0);
        f.go(true, t0, true);
        assert!(f.animating(t0));
        let mid = f.value(t0 + FADE / 2);
        assert!(mid > 0 && mid < 255, "{mid}");
        assert_eq!(f.value(t0 + FADE), 255);
        assert!(!f.animating(t0 + FADE));
        // Turned back halfway: it starts from the middle, not from the far end.
        let mut g = Fader::at(false);
        g.go(true, t0, true);
        g.go(false, t0 + FADE / 2, true);
        let back = g.value(t0 + FADE / 2);
        assert!(back > 0 && back < 255, "{back}");
        assert_eq!(g.value(t0 + FADE / 2 + FADE), 0);
        // At once.
        let mut h = Fader::at(true);
        h.go(false, t0, false);
        assert_eq!(h.value(t0), 0);
        assert!(!h.animating(t0));
    }
}
