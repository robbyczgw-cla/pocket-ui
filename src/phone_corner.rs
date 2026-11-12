//! The status corner on Android: the same time and battery as on the handhelds (`status_corner::Corner`), from what
//! the Activity reads for it (`PocketActivity.statusCorner`: `DateFormat.is24HourFormat`, the local time, and the
//! sticky `ACTION_BATTERY_CHANGED`, no permission). Pure data and fitting, so a host test covers every case: the
//! launcher paints the result in the header of its pages (`android/src/phone.rs`), never in a game.

use std::time::Duration;

use crate::status_corner::{format_clock, Battery, Corner, TIME_SET_AFTER};

/// How often the Activity is asked again; the minute turning asks as well (`Reading::next_look`).
pub const EVERY: Duration = Duration::from_secs(5);

/// What the Activity said: "is24,hour,minute,percent,charging", for example "1,13,28,87,0". `percent` is -1 where the
/// device has no battery (or reports none); a part that is missing or not a number is simply unknown.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Reading {
    /// The system's choice: 24 hours (true) or 12 hours with AM/PM.
    pub is24: bool,
    /// The local hour (0-23) and minute.
    pub clock: Option<(u8, u8)>,
    pub battery: Option<Battery>,
}

impl Reading {
    /// Parses the Activity's line. Anything unreadable leaves that part unknown; an empty line is nothing known.
    pub fn parse(line: &str) -> Reading {
        let mut parts = line.trim().split(',').map(|p| p.trim().parse::<i32>().ok());
        let is24 = parts.next().flatten() == Some(1);
        let hour = parts.next().flatten().filter(|h| (0..24).contains(h));
        let minute = parts.next().flatten().filter(|m| (0..60).contains(m));
        let percent = parts.next().flatten().filter(|p| (0..=100).contains(p));
        let charging = parts.next().flatten() == Some(1);
        Reading {
            is24,
            clock: hour.zip(minute).map(|(h, m)| (h as u8, m as u8)),
            battery: percent.map(|p| Battery { percent: p as u8, charging }),
        }
    }

    /// The time as written: "13:28" on a 24-hour clock (as on the handhelds), "1:28 PM" on a 12-hour one. None while
    /// the clock was never set (`unix` before `TIME_SET_AFTER`: no "00:00" from 1970) or the hour is unknown.
    pub fn clock_text(&self, unix: Option<i64>) -> Option<String> {
        let (hour, minute) = self.clock?;
        if self.is24 {
            return format_clock(unix?, hour as i32, minute as i32);
        }
        (unix? >= TIME_SET_AFTER).then(|| {
            let shown = if hour % 12 == 0 { 12 } else { hour % 12 };
            format!("{shown}:{minute:02} {}", if hour < 12 { "AM" } else { "PM" })
        })
    }

    /// The corner from the widest to the narrowest form that is worth showing: time and battery with "charging", time
    /// and the figure alone, the time alone. Nothing known gives nothing.
    pub fn forms(&self, unix: Option<i64>) -> Vec<Corner> {
        let clock = self.clock_text(unix);
        let mut forms = Vec::with_capacity(3);
        if let Some(b) = self.battery {
            let word = if b.charging { format!(", {}", crate::i18n::tr("charging")) } else { String::new() };
            forms.push(Corner { clock: clock.clone(), battery: Some(format!("{}%{word}", b.percent)), ..Corner::default() });
            if b.charging {
                forms.push(Corner { clock: clock.clone(), battery: Some(format!("{}%", b.percent)), ..Corner::default() });
            }
        }
        // The time alone (with no time known and a battery, the first form was the figure alone already).
        if clock.is_some() {
            forms.push(Corner { clock, ..Corner::default() });
        }
        forms
    }

    /// The widest form that fits in `room` pixels at type size `px` (`width` measures a text), else an empty corner:
    /// the corner gives way before it cuts a word or covers the page's title.
    pub fn fit(&self, unix: Option<i64>, room: i32, px: u32, mut width: impl FnMut(&str) -> i32) -> Corner {
        self.forms(unix)
            .into_iter()
            .find(|corner| {
                let (placed, left) = corner.layout_at(0, px, &mut width);
                !placed.is_empty() && -left <= room
            })
            .unwrap_or_default()
    }

    /// When to ask the Activity again: after `EVERY`, or just after the minute turns when that comes first.
    pub fn next_look(since_epoch: Duration) -> Duration {
        EVERY.min(crate::status_corner::until_next_minute(since_epoch))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: Option<i64> = Some(TIME_SET_AFTER + 86_400);
    /// 10 px a character: easy to add up.
    fn ten(s: &str) -> i32 {
        s.chars().count() as i32 * 10
    }

    #[test]
    fn the_line_from_the_activity_is_read_part_by_part() {
        let r = Reading::parse("1,13,28,87,1");
        assert!(r.is24);
        assert_eq!(r.clock, Some((13, 28)));
        assert_eq!(r.battery, Some(Battery { percent: 87, charging: true }));
        assert_eq!(Reading::parse("0,0,5,100,0").battery, Some(Battery { percent: 100, charging: false }));
        // A device without a battery says -1: the time alone, no "-1%".
        let none = Reading::parse("1,9,5,-1,0");
        assert_eq!((none.clock, none.battery), (Some((9, 5)), None));
        // Rubbish or a missing part is unknown, never a broken figure.
        let bad = Reading::parse("1,25,61,101,1");
        assert_eq!((bad.clock, bad.battery), (None, None));
        assert_eq!(Reading::parse("1,13,28"), Reading { is24: true, clock: Some((13, 28)), battery: None });
        assert_eq!(Reading::parse("x,y,z"), Reading::default());
        assert_eq!(Reading::parse(""), Reading::default());
        assert!(Reading::default().forms(NOW).is_empty(), "nothing known, nothing shown");
    }

    #[test]
    fn the_system_decides_between_24_and_12_hours() {
        let at = |line: &str| Reading::parse(line).clock_text(NOW);
        assert_eq!(at("1,13,28,50,0").as_deref(), Some("13:28"));
        assert_eq!(at("1,9,5,50,0").as_deref(), Some("09:05"), "as on the handhelds");
        assert_eq!(at("0,13,28,50,0").as_deref(), Some("1:28 PM"));
        assert_eq!(at("0,0,5,50,0").as_deref(), Some("12:05 AM"), "midnight is 12, not 0");
        assert_eq!(at("0,12,0,50,0").as_deref(), Some("12:00 PM"), "noon is 12 PM");
        assert_eq!(at("0,23,59,50,0").as_deref(), Some("11:59 PM"));
        // A clock that was never set shows no time, in either format.
        assert_eq!(Reading::parse("1,0,0,50,0").clock_text(Some(0)), None);
        assert_eq!(Reading::parse("0,0,0,50,0").clock_text(Some(0)), None);
        assert_eq!(Reading::parse("0,0,0,50,0").clock_text(None), None);
    }

    #[test]
    fn charging_and_a_missing_battery_read_plainly() {
        let battery = |line: &str| Reading::parse(line).forms(NOW)[0].battery.clone();
        assert_eq!(battery("1,13,28,87,0").as_deref(), Some("87%"));
        assert_eq!(battery("1,13,28,87,1").as_deref(), Some("87%, charging"));
        assert_eq!(battery("1,13,28,0,0").as_deref(), Some("0%"));
        let no_battery = Reading::parse("1,13,28,-1,0").forms(NOW);
        assert_eq!(no_battery.len(), 1);
        assert_eq!(no_battery[0], Corner { clock: Some("13:28".into()), ..Corner::default() });
        // No time but a battery: the figure alone.
        let no_time = Reading::parse("1,x,x,42,0").forms(NOW);
        assert_eq!(no_time, vec![Corner::battery_only("42%")]);
    }

    #[test]
    fn the_corner_gives_way_a_step_at_a_time_and_never_overflows() {
        let r = Reading::parse("0,13,28,87,1");
        let full = "1:28 PM".len() as i32 * 10 + crate::status_corner::GAP + "87%, charging".len() as i32 * 10;
        assert_eq!(r.fit(NOW, full, 14, ten).battery.as_deref(), Some("87%, charging"));
        // One pixel short: the word "charging" goes, the figure stays.
        let figure = r.fit(NOW, full - 1, 14, ten);
        assert_eq!((figure.clock.as_deref(), figure.battery.as_deref()), (Some("1:28 PM"), Some("87%")));
        // Short of that: the time alone; short of the time: nothing.
        let short = "1:28 PM".len() as i32 * 10 + crate::status_corner::GAP + 3 * 10;
        let time = r.fit(NOW, short - 1, 14, ten);
        assert_eq!((time.clock.as_deref(), time.battery), (Some("1:28 PM"), None));
        assert_eq!(r.fit(NOW, 69, 14, ten), Corner::default());
        assert_eq!(r.fit(NOW, 0, 14, ten), Corner::default());
        // Whatever the room, what is returned fits it.
        for room in 0..400 {
            let c = r.fit(NOW, room, 14, ten);
            assert!(-c.layout_at(0, 14, ten).1 <= room, "room {room}");
        }
    }

    #[test]
    fn the_next_look_is_the_minute_turning_when_that_is_sooner() {
        // 58 s into a minute: the turn is 2 s (and the margin) away, sooner than the five seconds.
        let near = Reading::next_look(Duration::from_secs(120 + 58));
        assert!(near < EVERY && near >= Duration::from_secs(2), "{near:?}");
        // Early in the minute: the regular five seconds.
        assert_eq!(Reading::next_look(Duration::from_secs(120 + 10)), EVERY);
    }
}
