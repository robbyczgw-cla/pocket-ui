//! Settings › System › Time › Date and time: the clock set by hand, for a Pocket with no Wi-Fi and a wrong clock (a flat
//! RTC battery, the very first start). Without it the Time page could only say "turn Wi-Fi on": a dead end offline,
//! and play history, today's pick, the dates on saves and the night light all follow the clock.
//!
//! - **The picker** ([`Picker`], pure, tested): year, month, day, hour and minute in local time (the chosen time
//!   zone: the offset comes from libc's `localtime_r`, as everywhere else), 24 h as every clock of pocket-ui. Left and
//!   Right move between the fields, Up and Down change one (wrapping; the year stops at its ends); the day is kept
//!   inside its month, leap years included. It starts at the clock, or at the day this version was built when the
//!   clock is clearly wrong (`clock_check`), so a Pocket at 1 January 2000 is a few presses from today, not 26 years.
//! - **Setting it** ([`set_system_clock`], [`write_rtc`]): the system clock with `clock_settime(CLOCK_REALTIME)`
//!   (pocket-ui runs as root on ROCKNIX), then the hardware clock with the `RTC_SET_TIME` ioctl on `/dev/rtc0`, in UTC
//!   (the kernel reads rtc0 as UTC at boot, `CONFIG_RTC_HCTOSYS`), so it survives a restart. The Mini V2's RTC (the
//!   PM8150's) keeps its offset in the PMIC's SDAM since ROCKNIX's 2025-12 device tree, so it can be set; where an
//!   RTC refuses, the clock is still set until the next start and the page says so. Only `system_ctl::RealHw` calls
//!   these; everything else goes through `Hw::set_clock`, so no test ever touches the build machine's clock.
//! - **Network time still wins**: nothing here switches systemd-timesyncd off. When Wi-Fi reaches a time server later,
//!   it sets the clock again (and the kernel's 11-minute sync writes the RTC). While network time has set the clock
//!   (`NtpState::Synced`) the row is not offered: there is nothing to correct.

use crate::play_history::{civil, day_of};

/// The fields, left to right.
pub const FIELDS: [&str; 5] = ["Year", "Month", "Day", "Hour", "Minute"];
/// The year can't go below the floor of `clock_check` (an earlier clock is "wrong" anyway), nor past 2099.
pub const FIRST_YEAR: i32 = 2026;
pub const LAST_YEAR: i32 = 2099;
const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
const DAYS: [&str; 7] = ["Thu", "Fri", "Sat", "Sun", "Mon", "Tue", "Wed"];
const DAY: i64 = 86_400;

pub fn is_leap(y: i32) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

pub fn days_in_month(y: i32, m: u32) -> u32 {
    match m {
        2 if is_leap(y) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// The date and time being chosen, in local time, and the field the D-pad is on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Picker {
    pub year: i32,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    /// 0 year … 4 minute.
    pub sel: usize,
}

impl Picker {
    /// The local date and time of `unix` in a zone `offset` seconds east of UTC, kept inside the years offered.
    pub fn at(unix: i64, offset: i32) -> Picker {
        let local = unix + offset as i64;
        let (year, month, day) = civil(local.div_euclid(DAY));
        let secs = local.rem_euclid(DAY);
        let mut p = Picker { year, month, day, hour: (secs / 3600) as u32, minute: (secs % 3600 / 60) as u32, sel: 0 };
        if !(FIRST_YEAR..=LAST_YEAR).contains(&year) {
            p.year = year.clamp(FIRST_YEAR, LAST_YEAR);
            p.day = p.day.min(days_in_month(p.year, p.month));
        }
        p
    }

    /// Where the picker opens: the clock, or the day this version was built (`clock_check::built_unix`, at the same
    /// local time of day) when the clock is clearly wrong. `offset_at`: the zone's offset at a moment.
    pub fn start(now: i64, built: i64, offset_at: impl Fn(i64) -> i32) -> Picker {
        let from = if crate::clock_check::looks_wrong(now, built) {
            let base = built.max(crate::clock_check::FLOOR_UNIX);
            // The build day's date with the wrong clock's time of day: the minutes are as good a guess as any.
            base - base.rem_euclid(DAY) + now.rem_euclid(DAY)
        } else {
            now
        };
        Picker::at(from, offset_at(from))
    }

    /// The local time as seconds since 1970 in UTC: the offset at that moment (twice, so a date across a change of
    /// summer time takes the offset it has itself). A time a summer-time change skips comes out an hour on.
    pub fn unix(&self, offset_at: impl Fn(i64) -> i32) -> i64 {
        let naive = day_of(self.year, self.month, self.day) * DAY + self.hour as i64 * 3600 + self.minute as i64 * 60;
        let first = naive - offset_at(naive) as i64;
        naive - offset_at(first) as i64
    }

    /// A D-pad press: Left/Right move between the fields, Up/Down change the selected one. False for other buttons.
    pub fn press(&mut self, b: crate::pad::Button) -> bool {
        use crate::pad::Button;
        match b {
            Button::Left => self.sel = (self.sel + FIELDS.len() - 1) % FIELDS.len(),
            Button::Right => self.sel = (self.sel + 1) % FIELDS.len(),
            Button::Up => self.step(1),
            Button::Down => self.step(-1),
            _ => return false,
        }
        true
    }

    /// Changes the selected field by `dir`: month, day, hour and minute wrap round, the year stops at its ends. The
    /// day then stays inside its month (31 January to February is 28 or 29 February).
    pub fn step(&mut self, dir: i32) {
        let wrap = |v: u32, lo: u32, hi: u32| -> u32 { ((v as i32 - lo as i32 + dir).rem_euclid((hi - lo + 1) as i32) as u32) + lo };
        match self.sel {
            0 => self.year = (self.year + dir).clamp(FIRST_YEAR, LAST_YEAR),
            1 => self.month = wrap(self.month, 1, 12),
            2 => self.day = wrap(self.day, 1, days_in_month(self.year, self.month)),
            3 => self.hour = wrap(self.hour, 0, 23),
            _ => self.minute = wrap(self.minute, 0, 59),
        }
        self.day = self.day.min(days_in_month(self.year, self.month));
    }

    /// A field as painted: "2026", "Oct", "26", "14", "05".
    pub fn value(&self, field: usize) -> String {
        match field {
            0 => self.year.to_string(),
            1 => MONTHS[(self.month as usize).clamp(1, 12) - 1].to_string(),
            2 => self.day.to_string(),
            3 => format!("{:02}", self.hour),
            _ => format!("{:02}", self.minute),
        }
    }

    /// "Mon 26 Oct 2026", the date with its weekday (the Time page's own form).
    pub fn date_label(&self) -> String {
        let weekday = DAYS[day_of(self.year, self.month, self.day).rem_euclid(7) as usize];
        format!("{weekday} {} {} {}", self.day, self.value(1), self.year)
    }

    /// "14:05".
    pub fn time_label(&self) -> String {
        format!("{:02}:{:02}", self.hour, self.minute)
    }
}

/// Sets the system clock (CLOCK_REALTIME) to `unix`. Needs CAP_SYS_TIME (root on ROCKNIX). Only `RealHw` calls it.
pub fn set_system_clock(unix: i64) -> Result<(), String> {
    let ts = libc::timespec { tv_sec: unix as libc::time_t, tv_nsec: 0 };
    // SAFETY: clock_settime only reads the struct we hand in.
    if unsafe { libc::clock_settime(libc::CLOCK_REALTIME, &ts) } == 0 {
        Ok(())
    } else {
        Err(format!("clock_settime: {}", std::io::Error::last_os_error()))
    }
}

/// The kernel's `struct rtc_time` (linux/rtc.h): the fields of a `struct tm`, nothing more.
#[repr(C)]
struct RtcTime {
    tm_sec: libc::c_int,
    tm_min: libc::c_int,
    tm_hour: libc::c_int,
    tm_mday: libc::c_int,
    tm_mon: libc::c_int,
    tm_year: libc::c_int,
    tm_wday: libc::c_int,
    tm_yday: libc::c_int,
    tm_isdst: libc::c_int,
}

/// `_IOW('p', 0x0a, struct rtc_time)`: 36 bytes written, the same number on arm64 and x86-64.
const RTC_SET_TIME: u64 = 0x4024_700a;

/// The fields `RTC_SET_TIME` wants for `unix`, in UTC: month 0-11, years since 1900.
fn rtc_fields(unix: i64) -> RtcTime {
    let (y, m, d) = civil(unix.div_euclid(DAY));
    let secs = unix.rem_euclid(DAY) as libc::c_int;
    RtcTime {
        tm_sec: secs % 60,
        tm_min: secs / 60 % 60,
        tm_hour: secs / 3600,
        tm_mday: d as libc::c_int,
        tm_mon: m as libc::c_int - 1,
        tm_year: y - 1900,
        tm_wday: 0,
        tm_yday: 0,
        tm_isdst: 0,
    }
}

/// Writes `unix` (UTC) into the hardware clock `dev` (`/dev/rtc0`), so the time survives a restart. Only `RealHw`
/// calls it.
pub fn write_rtc(dev: &std::path::Path, unix: i64) -> Result<(), String> {
    use std::os::fd::AsRawFd;
    let file = std::fs::OpenOptions::new().read(true).open(dev).map_err(|e| format!("{}: {e}", dev.display()))?;
    let tm = rtc_fields(unix);
    // SAFETY: the fd is open for the call, and RTC_SET_TIME only reads the struct, whose layout is the kernel's.
    if unsafe { libc::ioctl(file.as_raw_fd(), RTC_SET_TIME as _, &tm as *const RtcTime) } == 0 {
        Ok(())
    } else {
        Err(format!("RTC_SET_TIME on {}: {}", dev.display(), std::io::Error::last_os_error()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pad::Button;
    use crate::testutil::TestDir;

    // 2026-10-26 13:05 UTC, a Monday.
    const MON: i64 = 1_793_019_900;

    fn utc(_: i64) -> i32 {
        0
    }

    /// Vienna's rule: CET, and CEST from the last Sunday of March 01:00 UTC to the last Sunday of October 01:00 UTC.
    fn vienna(at: i64) -> i32 {
        // The last Sunday of a month (day 3, 4 January 1970, was a Sunday).
        let sunday = |y: i32, m: u32| {
            let d = day_of(y, m, days_in_month(y, m));
            d - (d - 3).rem_euclid(7)
        };
        let (y, _, _) = civil(at.div_euclid(DAY));
        let (start, end) = (sunday(y, 3) * DAY + 3600, sunday(y, 10) * DAY + 3600);
        if (start..end).contains(&at) {
            7200
        } else {
            3600
        }
    }

    #[test]
    fn the_picker_shows_the_local_time_of_the_chosen_zone() {
        let p = Picker::at(MON, 0);
        assert_eq!((p.year, p.month, p.day, p.hour, p.minute), (2026, 10, 26, 13, 5));
        assert_eq!(p.date_label(), "Mon 26 Oct 2026");
        assert_eq!(p.time_label(), "13:05");
        let ny = Picker::at(MON, -4 * 3600);
        assert_eq!((ny.day, ny.hour), (26, 9));
        let tokyo = Picker::at(MON + 12 * 3600, 9 * 3600);
        assert_eq!((tokyo.day, tokyo.hour, tokyo.minute), (27, 10, 5), "past midnight: the next day");
        assert_eq!((0..5).map(|f| p.value(f)).collect::<Vec<_>>(), ["2026", "Oct", "26", "13", "05"]);
    }

    #[test]
    fn local_time_set_is_utc_less_the_zones_offset() {
        let p = Picker::at(MON, 0);
        assert_eq!(p.unix(utc), MON, "on a whole minute: the seconds are 0");
        // 14:05 in Vienna on 26 October is 13:05 UTC (CET, the day after summer time ended).
        let mut v = Picker { year: 2026, month: 10, day: 26, hour: 14, minute: 5, sel: 0 };
        assert_eq!(v.unix(vienna), MON);
        // In July (CEST) the same wall time is an hour earlier in UTC.
        v.month = 7;
        assert_eq!(v.unix(vienna), day_of(2026, 7, 26) * DAY + 12 * 3600 + 5 * 60);
        // Round trip at every hour of a day across the change.
        for h in 0..48 {
            let t = day_of(2026, 10, 24) * DAY + h * 3600;
            let p = Picker::at(t, vienna(t));
            let back = p.unix(vienna);
            assert!(back == t || back == t + 3600 || back == t - 3600, "{h}: {t} -> {back}");
        }
        // New York, five hours behind: 09:30 there is 14:30 UTC.
        let p = Picker { year: 2026, month: 12, day: 1, hour: 9, minute: 30, sel: 0 };
        assert_eq!(p.unix(|_| -5 * 3600), day_of(2026, 12, 1) * DAY + 14 * 3600 + 30 * 60);
    }

    #[test]
    fn the_dpad_moves_between_fields_and_changes_them() {
        let mut p = Picker::at(MON, 0);
        assert!(p.press(Button::Left));
        assert_eq!(p.sel, 4, "Left from the year wraps to the minute");
        assert!(p.press(Button::Up));
        assert_eq!(p.minute, 6);
        for _ in 0..54 {
            p.press(Button::Up);
        }
        assert_eq!(p.minute, 0, "59 → 0 wraps, the hour stays");
        assert_eq!(p.hour, 13);
        p.press(Button::Down);
        assert_eq!(p.minute, 59);
        p.press(Button::Left);
        p.press(Button::Down);
        assert_eq!((p.sel, p.hour), (3, 12));
        p.sel = 3;
        p.hour = 0;
        p.press(Button::Down);
        assert_eq!(p.hour, 23);
        assert!(!p.press(Button::East), "A is the page's, not the picker's");
        assert!(p.press(Button::Right));
        assert_eq!(p.sel, 4);
        p.press(Button::Right);
        assert_eq!(p.sel, 0, "Right from the minute wraps to the year");
    }

    #[test]
    fn days_stay_inside_their_month_leap_years_included() {
        assert_eq!(days_in_month(2026, 2), 28);
        assert_eq!(days_in_month(2028, 2), 29);
        assert_eq!(days_in_month(2100, 2), 28, "a century is no leap year");
        assert_eq!(days_in_month(2000, 2), 29, "every 400 years it is");
        assert_eq!(days_in_month(2026, 4), 30);
        assert_eq!(days_in_month(2026, 12), 31);
        // 31 January, then the month up: 28 February.
        let mut p = Picker { year: 2026, month: 1, day: 31, hour: 0, minute: 0, sel: 1 };
        p.step(1);
        assert_eq!((p.month, p.day), (2, 28));
        // 29 February 2028, then the year up: 28 February 2029.
        let mut p = Picker { year: 2028, month: 2, day: 29, hour: 0, minute: 0, sel: 0 };
        p.step(1);
        assert_eq!((p.year, p.day), (2029, 28));
        // The day wraps within its own month.
        let mut p = Picker { year: 2028, month: 2, day: 29, hour: 0, minute: 0, sel: 2 };
        p.step(1);
        assert_eq!(p.day, 1);
        p.step(-1);
        assert_eq!(p.day, 29);
        let mut p = Picker { year: 2026, month: 2, day: 1, hour: 0, minute: 0, sel: 2 };
        p.step(-1);
        assert_eq!(p.day, 28);
        // Months wrap, the year doesn't.
        let mut p = Picker { year: 2026, month: 12, day: 31, hour: 0, minute: 0, sel: 1 };
        p.step(1);
        assert_eq!((p.year, p.month), (2026, 1));
        p.sel = 0;
        p.step(-1);
        assert_eq!(p.year, FIRST_YEAR, "no year before the floor");
        p.year = LAST_YEAR;
        p.step(1);
        assert_eq!(p.year, LAST_YEAR);
    }

    #[test]
    fn a_wrong_clock_opens_the_picker_at_the_build_day() {
        // 2026-10-25 12:00 UTC.
        let built = 1_792_929_600;
        let dead_rtc = 946_684_800 + 8 * 3600 + 20 * 60; // 1 January 2000, 08:20
        let p = Picker::start(dead_rtc, built, utc);
        assert_eq!((p.year, p.month, p.day, p.hour, p.minute), (2026, 10, 25, 8, 20));
        // A plausible clock opens as it is.
        let p = Picker::start(MON, built, utc);
        assert_eq!((p.day, p.hour, p.minute), (26, 13, 5));
        // No build date known: the floor, 1 January 2026.
        let p = Picker::start(dead_rtc, 0, utc);
        assert_eq!((p.year, p.month, p.day), (2026, 1, 1));
        // A clock far past the last year is kept inside the years offered.
        let p = Picker::at(day_of(2150, 2, 28) * DAY, 0);
        assert_eq!(p.year, LAST_YEAR);
    }

    #[test]
    fn the_rtc_gets_utc_in_the_kernels_fields() {
        let tm = rtc_fields(MON);
        assert_eq!((tm.tm_year, tm.tm_mon, tm.tm_mday, tm.tm_hour, tm.tm_min, tm.tm_sec), (126, 9, 26, 13, 5, 0));
        assert_eq!(std::mem::size_of::<RtcTime>(), 36, "the ioctl number's size");
        assert_eq!(RTC_SET_TIME >> 16 & 0x3fff, 36);
        // Not an RTC: an error, never a panic, and nothing else happens.
        let dir = TestDir::new("not-an-rtc");
        let file = dir.join("rtc0");
        std::fs::write(&file, "").unwrap();
        assert!(write_rtc(&file, MON).is_err());
        assert!(write_rtc(std::path::Path::new("/nonexistent/rtc0"), MON).is_err());
    }
}
