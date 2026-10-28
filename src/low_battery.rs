//! The low-battery safeguard (review B12 / SOL-05): a device left dark in light sleep (a suspend that keeps
//! failing, or `<data>/no-autosuspend` while testing) or played on until the cell is empty used to run until the
//! battery browned out, with the game in memory lost. Now a reading every [`EVERY`]:
//!
//! - at [`WARN_AT`] % or less on battery: one warning (a notice on the shelf, the level pill over a pocket-core
//!   game), again only after the battery has been charged above it;
//! - at [`CRITICAL_AT`] % or less on battery, [`CONFIRM`] readings in a row: the game is saved and quit, the resume
//!   note left and the device powered off, as the power menu's Power off does (`Sleep::shut_down`).
//!
//! On external power (charging, full, or "Not charging" with a charger in, or any USB/mains supply online) nothing
//! happens, so a device on the charger stays up as long as it likes (`no-autosuspend` overnight on the charger).
//!
//! The reading runs on its own thread (two small sysfs files); the UI thread only hears of a crossing.

use std::path::Path;
use std::time::Duration;

/// At or below this percentage (on battery) the player is warned once.
pub const WARN_AT: u8 = 5;
/// At or below this percentage (on battery) the game is saved and the device switched off.
pub const CRITICAL_AT: u8 = 2;
/// How often the battery is read.
pub const EVERY: Duration = Duration::from_secs(30);
/// Critical readings in a row before switching off: one stray reading of the gauge is no reason.
pub const CONFIRM: u32 = 2;
/// The warning comes again only after the battery was above `WARN_AT` by this much.
const REARM: u8 = 2;

/// One reading of the battery.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Level {
    pub percent: u8,
    /// A charger or other supply is connected (the battery may still be full or not charging).
    pub external_power: bool,
}

/// What a reading calls for.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Alert {
    /// Warn the player: the battery is at this percentage.
    Low(u8),
    /// Save the game and switch off now.
    Critical(u8),
}

/// The decision from one reading to the next (unit-tested; no I/O).
#[derive(Debug, Default)]
pub struct Guard {
    warned: bool,
    critical_in_a_row: u32,
    fired: bool,
}

impl Guard {
    /// One reading (None: the battery couldn't be read, which never counts as low).
    pub fn observe(&mut self, level: Option<Level>) -> Option<Alert> {
        let Some(level) = level else {
            self.critical_in_a_row = 0;
            return None;
        };
        if level.external_power {
            *self = Guard::default();
            return None;
        }
        if level.percent > WARN_AT.saturating_add(REARM) {
            self.warned = false;
        }
        if level.percent <= CRITICAL_AT {
            self.critical_in_a_row += 1;
            if self.critical_in_a_row >= CONFIRM && !self.fired {
                self.fired = true;
                return Some(Alert::Critical(level.percent));
            }
        } else {
            self.critical_in_a_row = 0;
            self.fired = false;
        }
        if level.percent <= WARN_AT && !self.warned {
            self.warned = true;
            return Some(Alert::Low(level.percent));
        }
        None
    }
}

/// Reads the battery in `dir` (`battery::dir()`): its capacity, and whether external power is connected (the
/// battery's own status, or another supply next to it in /sys/class/power_supply that is online).
pub fn read(dir: &Path) -> Option<Level> {
    let get = |p: &Path| std::fs::read_to_string(p).ok().map(|s| s.trim().to_string());
    let percent = get(&dir.join("capacity"))?.parse::<i64>().ok()?.clamp(0, 100) as u8;
    let status = get(&dir.join("status")).unwrap_or_default();
    let charger = matches!(status.as_str(), "Charging" | "Full" | "Not charging")
        || dir.parent().is_some_and(|supplies| {
            std::fs::read_dir(supplies).into_iter().flatten().flatten().any(|e| {
                let p = e.path();
                p != dir && get(&p.join("type")).is_some_and(|t| t != "Battery") && get(&p.join("online")).as_deref() == Some("1")
            })
        });
    Some(Level {
        percent,
        external_power: charger,
    })
}

/// What the shelf says at the warning.
pub fn warning_text(percent: u8) -> String {
    format!("Battery low ({percent}%). At {CRITICAL_AT}% the game is saved and the Pocket switches off.")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;

    fn bat(percent: u8) -> Option<Level> {
        Some(Level {
            percent,
            external_power: false,
        })
    }

    fn plugged(percent: u8) -> Option<Level> {
        Some(Level {
            percent,
            external_power: true,
        })
    }

    #[test]
    fn warns_once_then_switches_off_at_the_critical_level() {
        let mut g = Guard::default();
        assert_eq!(g.observe(bat(40)), None);
        assert_eq!(g.observe(bat(6)), None);
        assert_eq!(g.observe(bat(5)), Some(Alert::Low(5)));
        assert_eq!(g.observe(bat(4)), None, "warned once");
        assert_eq!(g.observe(bat(3)), None);
        // One critical reading is not enough (the gauge can dip under load).
        assert_eq!(g.observe(bat(2)), None);
        assert_eq!(g.observe(bat(2)), Some(Alert::Critical(2)));
        assert_eq!(g.observe(bat(1)), None, "asked once");
    }

    #[test]
    fn a_stray_critical_reading_does_not_switch_off() {
        let mut g = Guard::default();
        assert_eq!(g.observe(bat(2)), Some(Alert::Low(2)));
        assert_eq!(g.observe(bat(3)), None);
        assert_eq!(g.observe(bat(2)), None);
        assert_eq!(g.observe(None), None, "an unreadable gauge breaks the run");
        assert_eq!(g.observe(bat(2)), None);
        assert_eq!(g.observe(bat(1)), Some(Alert::Critical(1)));
    }

    #[test]
    fn on_the_charger_nothing_happens_however_low() {
        let mut g = Guard::default();
        for _ in 0..10 {
            assert_eq!(g.observe(plugged(0)), None);
            assert_eq!(g.observe(plugged(1)), None);
        }
        // Unplugged at 1 %: warned, then switched off after the second reading.
        assert_eq!(g.observe(bat(1)), Some(Alert::Low(1)));
        assert_eq!(g.observe(bat(1)), Some(Alert::Critical(1)));
        // Plugged in during the power-off: everything starts over.
        assert_eq!(g.observe(plugged(1)), None);
        assert_eq!(g.observe(bat(1)), Some(Alert::Low(1)));
    }

    #[test]
    fn the_warning_comes_again_after_charging_above_it() {
        let mut g = Guard::default();
        assert_eq!(g.observe(bat(5)), Some(Alert::Low(5)));
        assert_eq!(g.observe(bat(7)), None, "a gauge wobbling around the line warns once");
        assert_eq!(g.observe(bat(5)), None);
        assert_eq!(g.observe(bat(8)), None);
        assert_eq!(g.observe(bat(5)), Some(Alert::Low(5)));
    }

    #[test]
    fn reads_the_level_and_the_charger_from_sysfs() {
        let root = TestDir::new("lowbat");
        let battery = root.join("battery");
        let usb = root.join("usb");
        std::fs::create_dir_all(&battery).unwrap();
        std::fs::create_dir_all(&usb).unwrap();
        std::fs::write(battery.join("type"), "Battery\n").unwrap();
        std::fs::write(battery.join("capacity"), "3\n").unwrap();
        std::fs::write(battery.join("status"), "Discharging\n").unwrap();
        std::fs::write(usb.join("type"), "USB\n").unwrap();
        std::fs::write(usb.join("online"), "0\n").unwrap();
        assert_eq!(read(&battery), bat(3));
        // A charger the battery doesn't say "Charging" for (full, too warm): still external power.
        std::fs::write(usb.join("online"), "1\n").unwrap();
        assert_eq!(read(&battery), plugged(3));
        std::fs::write(usb.join("online"), "0\n").unwrap();
        for status in ["Charging", "Full", "Not charging"] {
            std::fs::write(battery.join("status"), status).unwrap();
            assert_eq!(read(&battery), plugged(3), "{status}");
        }
        std::fs::write(battery.join("capacity"), "garbage").unwrap();
        assert_eq!(read(&battery), None);
        assert_eq!(read(&root.join("nothing")), None);
    }
}
