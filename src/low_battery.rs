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
//! The reading runs on its own thread; the UI thread only hears of a crossing.

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
/// An R36S V22 on ROCKNIX 20261101 reported 3.598 V (voltage_avg) at 48 % while discharging at 375 mA.
/// The R36S device tree's open-circuit table puts 3.30 V between 10 % (3.28 V) and 15 % (3.33 V).
const R36S_CRITICAL_VETO_UV: i64 = 3_300_000;

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
    let percent = get(&dir.join("capacity"))?.parse::<u8>().ok().filter(|&percent| percent <= 100)?;
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

/// A critical R36S reading with high battery voltage is treated as unreadable by the guard.
pub fn read_for_guard(dir: &Path, model: Option<crate::device::Model>) -> Option<Level> {
    let level = read(dir)?;
    if !level.external_power && level.percent <= CRITICAL_AT && model.is_some_and(crate::device::Model::is_rk3326) {
        let voltage = |name| {
            std::fs::read_to_string(dir.join(name)).ok()
                .and_then(|s| s.trim().parse::<i64>().ok())
                .filter(|&uv| uv > 0)
        };
        // The RK817 driver exports voltage_avg on ROCKNIX.
        if voltage("voltage_now").or_else(|| voltage("voltage_avg")).is_some_and(|uv| uv >= R36S_CRITICAL_VETO_UV) {
            return None;
        }
    }
    Some(level)
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
        for invalid in ["garbage", "-1", "101"] {
            std::fs::write(battery.join("capacity"), invalid).unwrap();
            assert_eq!(read(&battery), None, "{invalid}");
        }
        assert_eq!(read(&root.join("nothing")), None);
    }

    #[test]
    fn r36s_does_not_switch_off_when_critical_percent_disagrees_with_voltage() {
        let root = TestDir::new("lowbat-r36s");
        let battery = root.join("battery");
        std::fs::create_dir(&battery).unwrap();
        std::fs::write(battery.join("status"), "Discharging\n").unwrap();
        let model = crate::device::identify("Game Console R36S", b"gameconsole,r36s\0").model;
        let mut guard = Guard::default();
        for (percent, voltage_file) in [(0, "voltage_avg"), (1, "voltage_now"), (2, "voltage_avg")] {
            std::fs::write(battery.join("capacity"), percent.to_string()).unwrap();
            std::fs::write(battery.join(voltage_file), "4000000\n").unwrap();
            for _ in 0..CONFIRM {
                let alert = guard.observe(read_for_guard(&battery, model));
                assert!(!matches!(alert, Some(Alert::Critical(_))), "{percent}% at 4.0 V triggered shutdown");
            }
            std::fs::remove_file(battery.join(voltage_file)).unwrap();
        }
        std::fs::write(battery.join("voltage_avg"), "4000000\n").unwrap();
        for invalid in [b"".as_slice(), b"0", b"400?", b"\xff"] {
            std::fs::write(battery.join("voltage_now"), invalid).unwrap();
            assert_eq!(read_for_guard(&battery, model), None, "fall back to the high average voltage");
        }
        std::fs::remove_file(battery.join("voltage_now")).unwrap();
        std::fs::write(battery.join("voltage_avg"), "3240000\n").unwrap();
        assert_eq!(guard.observe(read_for_guard(&battery, model)), Some(Alert::Low(2)));
        assert_eq!(guard.observe(read_for_guard(&battery, model)), Some(Alert::Critical(2)));
        std::fs::remove_file(battery.join("voltage_avg")).unwrap();
        assert_eq!(read_for_guard(&battery, model), bat(2));
    }

    #[test]
    fn r36s_distinguishes_a_false_zero_from_a_nearly_empty_battery() {
        let root = TestDir::new("lowbat-r36s-voltage-avg");
        let battery = root.join("battery");
        std::fs::create_dir(&battery).unwrap();
        std::fs::write(battery.join("status"), "Discharging\n").unwrap();
        let model = crate::device::identify("Game Console R36S", b"gameconsole,r36s\0").model;
        let mut guard = Guard::default();

        std::fs::write(battery.join("capacity"), "0\n").unwrap();
        std::fs::write(battery.join("voltage_avg"), "3600000\n").unwrap();
        for _ in 0..CONFIRM {
            let alert = guard.observe(read_for_guard(&battery, model));
            assert!(!matches!(alert, Some(Alert::Critical(_))), "0% at 3.60 V triggered shutdown");
        }

        std::fs::write(battery.join("capacity"), "1\n").unwrap();
        std::fs::write(battery.join("voltage_avg"), "3200000\n").unwrap();
        assert_eq!(guard.observe(read_for_guard(&battery, model)), Some(Alert::Low(1)));
        assert_eq!(guard.observe(read_for_guard(&battery, model)), Some(Alert::Critical(1)));
    }
}
