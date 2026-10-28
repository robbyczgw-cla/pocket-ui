//! Is the system clock clearly wrong? After a flat battery or a long time off, the Pocket's clock can be far in the
//! past until a time server sets it. Play history, the night light, the dates on saves and Sync's ordering all follow
//! that clock, so Settings → System → Time says so, and the shelf says so once ([`SHELF_NOTICE`]).
//!
//! The rule is pure, so the page and the tests share it:
//! - **Wrong**: before 1 January 2026, or before the day this version was built (a clock can't be behind the software
//!   running on it). Needs nothing but the clock.
//! - **Unsynced**: the clock looks plausible, but network time hasn't set it since the start and Wi-Fi is off, so it
//!   may have drifted. Only the Time page says this; the shelf stays quiet for a Pocket that is simply offline. A
//!   clock set by hand since the start (Settings › System › Time › Date and time, `manual_time`) is not Unsynced:
//!   the player has just said what time it is.
//!
//! Both lines name the way forward that works offline, the Date and time row, as well as Wi-Fi.

use crate::system_ctl::NtpState;

/// 1 January 2026, 00:00 UTC: no clock of a working Pocket is earlier.
pub const FLOOR_UNIX: i64 = 1_767_225_600;
const DAY: i64 = 86_400;

/// When this was built (build.rs), 0 if unknown.
pub fn built_unix() -> i64 {
    option_env!("POCKETUI_BUILD_UNIX").and_then(|s| s.parse().ok()).unwrap_or(0)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Clock {
    Fine,
    Wrong,
    Unsynced,
}

/// The earliest time that can be right: the floor, or the build day (less a day for the zone) if that is later and a
/// believable date (a build machine with a broken clock must not make every Pocket complain).
pub fn earliest(built: i64) -> i64 {
    if built > FLOOR_UNIX && built < FLOOR_UNIX + 20 * 365 * DAY {
        FLOOR_UNIX.max(built - DAY)
    } else {
        FLOOR_UNIX
    }
}

/// Only the clock: before the earliest possible time is wrong.
pub fn looks_wrong(now: i64, built: i64) -> bool {
    now < earliest(built)
}

/// The clock against network time and Wi-Fi (`wifi`: None where the Pocket has no switch to ask). `by_hand`: set by
/// hand since pocket-ui started.
pub fn check(now: i64, built: i64, ntp: NtpState, wifi: Option<bool>, by_hand: bool) -> Clock {
    if looks_wrong(now, built) {
        Clock::Wrong
    } else if matches!(ntp, NtpState::Waiting | NtpState::Off) && wifi == Some(false) && !by_hand {
        Clock::Unsynced
    } else {
        Clock::Fine
    }
}

/// The line on the Time page; None when the clock is fine. `wifi`: Wi-Fi is on (a time server may answer soon).
pub fn page_line(clock: Clock, wifi: Option<bool>) -> Option<&'static str> {
    match (clock, wifi) {
        (Clock::Fine, _) => None,
        (Clock::Wrong, Some(true)) => Some("The clock looks wrong. Play history, the night light and the dates on saves follow it. Set it under Date and time, or wait for network time to set it over Wi-Fi."),
        (Clock::Wrong, _) => Some("The clock looks wrong. Play history, the night light and the dates on saves follow it. Set it under Date and time, or turn Wi-Fi on."),
        (Clock::Unsynced, _) => Some("Wi-Fi is off, and no time server has set the clock since it started, so it may be a little off. Set it under Date and time, or turn Wi-Fi on."),
    }
}

/// The notice on the shelf, once, when the clock is wrong.
pub const SHELF_NOTICE: &str = "The clock looks wrong. Play history and the dates on saves will be off until it is set. Set it in Settings › System › Time, or turn Wi-Fi on.";

#[cfg(test)]
mod tests {
    use super::*;

    // 2026-10-25 12:00 UTC
    const BUILT: i64 = 1_792_929_600;

    #[test]
    fn a_clock_before_2026_or_before_the_build_is_wrong() {
        assert!(looks_wrong(0, BUILT), "the epoch: a dead RTC");
        assert!(looks_wrong(FLOOR_UNIX - 1, 0), "2025 without a build date");
        assert!(!looks_wrong(FLOOR_UNIX, 0));
        assert!(looks_wrong(BUILT - 2 * DAY, BUILT), "older than the software");
        assert!(!looks_wrong(BUILT - DAY / 2, BUILT), "the zone's slack: not complained about");
        assert!(!looks_wrong(BUILT + 400 * DAY, BUILT));
    }

    #[test]
    fn a_build_date_that_cannot_be_right_is_ignored() {
        assert_eq!(earliest(0), FLOOR_UNIX);
        assert_eq!(earliest(5), FLOOR_UNIX, "a build computer whose clock was at the start of 1970");
        assert_eq!(earliest(FLOOR_UNIX + 30 * 365 * DAY), FLOOR_UNIX, "or in 2056");
        assert_eq!(earliest(BUILT), BUILT - DAY);
    }

    #[test]
    fn offline_with_a_plausible_clock_is_only_unsynced() {
        let now = BUILT + 3 * DAY;
        assert_eq!(check(now, BUILT, NtpState::Synced, Some(true), false), Clock::Fine);
        assert_eq!(check(now, BUILT, NtpState::Synced, Some(false), false), Clock::Fine, "it was set; Wi-Fi off now is fine");
        assert_eq!(check(now, BUILT, NtpState::Waiting, Some(true), false), Clock::Fine, "Wi-Fi is on: it will be set");
        assert_eq!(check(now, BUILT, NtpState::Waiting, Some(false), false), Clock::Unsynced);
        assert_eq!(check(now, BUILT, NtpState::Off, Some(false), false), Clock::Unsynced);
        assert_eq!(check(now, BUILT, NtpState::Unknown, None, false), Clock::Fine, "nothing known: nothing said");
        assert_eq!(check(5, BUILT, NtpState::Synced, Some(true), false), Clock::Wrong, "wrong wins over everything");
    }

    #[test]
    fn a_clock_set_by_hand_is_fine_offline_unless_it_is_still_wrong() {
        let now = BUILT + 3 * DAY;
        assert_eq!(check(now, BUILT, NtpState::Waiting, Some(false), true), Clock::Fine, "the player said what time it is");
        assert_eq!(check(now, BUILT, NtpState::Off, Some(false), true), Clock::Fine);
        assert_eq!(check(5, BUILT, NtpState::Waiting, Some(false), true), Clock::Wrong, "set by hand, but still before the build");
    }

    #[test]
    fn every_problem_has_a_line_with_a_way_forward_offline() {
        assert_eq!(page_line(Clock::Fine, Some(false)), None);
        for wifi in [Some(false), None, Some(true)] {
            assert!(page_line(Clock::Wrong, wifi).unwrap().contains("Set it under Date and time"), "{wifi:?}");
        }
        assert!(page_line(Clock::Wrong, Some(false)).unwrap().contains("turn Wi-Fi on"));
        assert!(!page_line(Clock::Wrong, Some(true)).unwrap().contains("turn Wi-Fi on"), "it is on already");
        let unsynced = page_line(Clock::Unsynced, Some(false)).unwrap();
        assert!(unsynced.contains("Wi-Fi is off") && unsynced.contains("Date and time"));
        assert!(SHELF_NOTICE.contains("Settings › System › Time") && SHELF_NOTICE.contains("Wi-Fi"));
    }
}
