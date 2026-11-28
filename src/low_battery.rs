//! The low-battery safeguard (review B12 / SOL-05): a device left dark in light sleep (a suspend that keeps
//! failing, or `<data>/no-autosuspend` while testing) or played on until the cell is empty used to run until the
//! battery browned out, with the game in memory lost. Now a reading every [`EVERY`]:
//!
//! - at [`WARN_AT`] % or less on battery: one warning (a notice on the shelf, the level pill over a pocket-core
//!   game), again only after the battery has been charged above it;
//! - at [`CRITICAL_AT`] % or less on battery, [`CONFIRM`] readings in a row without a voltage veto: the game is saved and quit,
//!   the resume note left and the device powered off, as the power menu's Power off does (`Sleep::shut_down`).
//!
//! On external power nothing happens, so a device on the charger stays up as long as it likes
//! (`no-autosuspend` overnight on the charger). The guard uses the same reading as the Battery page.
//!
//! The reading runs on its own thread; the UI thread only hears of a crossing.

#[cfg(not(all(target_os = "android", not(feature = "handheld"))))]
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
/// The Brick and RG35XX Plus logged at most 3.425 V and 3.427 V at 2% or less on battery.
/// The Mini V2's ROCKNIX device tree gives 3.60 V as its design minimum. Leave margin above
/// these values and use the voltage only to veto a critical reading (see docs/SLEEP.md).
const CRITICAL_VETO_UV: u32 = 3_700_000;
/// An R36S V22 on ROCKNIX 20261101 reported 3.598 V (voltage_avg) at 48 % while discharging at 375 mA.
/// The R36S device tree's open-circuit table puts 3.30 V between 10 % (3.28 V) and 15 % (3.33 V).
const R36S_CRITICAL_VETO_UV: u32 = 3_300_000;

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

/// Reads the device battery's capacity and whether it has external power.
#[cfg(not(all(target_os = "android", not(feature = "handheld"))))]
pub fn read(dir: &Path) -> Option<Level> {
    let capacity = std::fs::read_to_string(dir.join("capacity")).ok()?;
    let percent = capacity.trim().parse::<u8>().ok().filter(|&percent| percent <= 100)?;
    Some(Level {
        percent,
        external_power: crate::battery::read_charge(dir) != crate::system_ctl::Charge::Discharging,
    })
}

/// A critical percentage contradicted by the cell voltage is unreadable for the guard.
#[cfg(not(all(target_os = "android", not(feature = "handheld"))))]
pub fn read_for_guard(dir: &Path, model: Option<crate::device::Model>) -> Option<Level> {
    let level = read(dir)?;
    if level.external_power || level.percent > CRITICAL_AT {
        return Some(level);
    }
    let threshold = if model.is_some_and(crate::device::Model::is_rk3326) {
        R36S_CRITICAL_VETO_UV
    } else {
        CRITICAL_VETO_UV
    };

    let voltage_uv = |name| {
        let value = std::fs::read_to_string(dir.join(name)).ok()?.trim().parse::<u32>().ok()?;
        // power_supply uses µV. Accept mV as well in the plausible range for a single cell.
        match value {
            2_000_000..=5_000_000 => Some(value),
            2_000..=5_000 => Some(value * 1_000),
            _ => None,
        }
    };
    // The RK817 driver exports voltage_avg on ROCKNIX.
    let voltage = voltage_uv("voltage_now").or_else(|| voltage_uv("voltage_avg"));
    if voltage.is_some_and(|uv| uv >= threshold) {
        return None;
    }
    Some(level)
}

/// What the shelf says at the warning.
pub fn warning_text(percent: u8) -> String {
    format!("Battery low ({percent}%). At {CRITICAL_AT}% the game is saved and the Pocket switches off.")
}

/// The warning in Android's player: an app can't switch the handheld off, so at [`CRITICAL_AT`] % the game is saved and
/// the player goes back to the shelf (`android/src/player_battery.rs`).
pub fn closing_warning_text(percent: u8) -> String {
    format!("Battery low ({percent}%). At {CRITICAL_AT}% the game is saved and closed.")
}

/// What the player says over the game in the moment before it saves and closes at [`CRITICAL_AT`] % (the words
/// pocket-core says before it switches off, `sleep.rs`).
pub const EMPTY_TEXT: &str = "Battery empty. Saving…";

/// A guard's news from its reading thread to the thread that acts on it (Android's player: the GL thread takes it once
/// a frame, one atomic swap). The newest alert wins; `clear` drops one not yet taken (a charger plugged in meanwhile).
#[derive(Debug, Default)]
pub struct News(std::sync::atomic::AtomicU32);

impl News {
    const LOW: u32 = 0x100;
    const CRITICAL: u32 = 0x200;

    pub const fn new() -> News {
        News(std::sync::atomic::AtomicU32::new(0))
    }

    pub fn put(&self, alert: Alert) {
        let word = match alert {
            Alert::Low(p) => Self::LOW | p as u32,
            Alert::Critical(p) => Self::CRITICAL | p as u32,
        };
        self.0.store(word, std::sync::atomic::Ordering::SeqCst);
    }

    pub fn clear(&self) {
        self.0.store(0, std::sync::atomic::Ordering::SeqCst);
    }

    /// The alert put since the last `take`, once.
    pub fn take(&self) -> Option<Alert> {
        let word = self.0.swap(0, std::sync::atomic::Ordering::SeqCst);
        let percent = (word & 0xff) as u8;
        match word & !0xff {
            Self::LOW => Some(Alert::Low(percent)),
            Self::CRITICAL => Some(Alert::Critical(percent)),
            _ => None,
        }
    }
}

/// What Android's player does with an alert (`android/src/player_battery.rs`).
#[derive(Debug, PartialEq)]
pub enum PlayerAct {
    /// pocket-core's pill over the game says this.
    Say(String),
    /// The pill says [`EMPTY_TEXT`]; at the next frame the game is saved and the player goes back to the shelf.
    SaveAndClose,
}

pub fn player_act(alert: Alert) -> PlayerAct {
    match alert {
        Alert::Low(p) => PlayerAct::Say(closing_warning_text(p)),
        Alert::Critical(_) => PlayerAct::SaveAndClose,
    }
}

/// Android's reading (`PlayerActivity.batteryLevel`: `"<percent>,<plugged>"`, from the sticky `ACTION_BATTERY_CHANGED`;
/// `plugged` is 1 for any `EXTRA_PLUGGED` source). `None` for no battery (`-1`) or an answer it can't read, which
/// never counts as low.
pub fn parse_android(text: &str) -> Option<Level> {
    let (percent, plugged) = text.trim().split_once(',')?;
    let percent = percent.trim().parse::<u8>().ok().filter(|&p| p <= 100)?;
    let external_power = match plugged.trim() {
        "0" => false,
        "1" => true,
        _ => return None,
    };
    Some(Level { percent, external_power })
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
    fn reads_androids_battery_line() {
        assert_eq!(parse_android("5,0"), bat(5));
        assert_eq!(parse_android("2,1\n"), plugged(2));
        assert_eq!(parse_android("100,0"), bat(100));
        for unreadable in ["", "-1,0", "101,0", "5", "5,2", "x,0", "5,"] {
            assert_eq!(parse_android(unreadable), None, "{unreadable:?}");
        }
    }

    #[test]
    fn androids_player_warns_at_5_and_saves_and_closes_at_2_percent() {
        // The whole way of Android's player: readings as Java gives them, the guard, the news to the GL thread, the act.
        let news = News::new();
        let mut guard = Guard::default();
        let mut acts = Vec::new();
        for line in ["40,0", "6,0", "5,0", "4,0", "3,0", "2,0", "2,0", "1,0"] {
            if let Some(alert) = guard.observe(parse_android(line)) {
                news.put(alert);
            }
            if let Some(alert) = news.take() {
                acts.push((line, player_act(alert)));
            }
        }
        assert_eq!(acts, vec![
            ("5,0", PlayerAct::Say("Battery low (5%). At 2% the game is saved and closed.".into())),
            ("2,0", PlayerAct::SaveAndClose),
        ]);
        assert_eq!(news.take(), None, "taken once");
        // A charger plugged in before the GL thread took the news: nothing closes.
        let mut guard = Guard::default();
        for line in ["2,0", "2,0"] {
            if let Some(alert) = guard.observe(parse_android(line)) { news.put(alert); }
        }
        if parse_android("2,1").is_some_and(|l| l.external_power) { news.clear(); }
        assert_eq!(news.take(), None);
    }

    #[test]
    fn androids_warning_says_the_game_closes_not_that_the_pocket_switches_off() {
        let line = closing_warning_text(WARN_AT);
        assert!(line.contains("5%") && line.contains("2%") && line.contains("closed"), "{line}");
        assert!(!line.contains("switches off"), "{line}");
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
            assert_eq!(read(&battery), bat(3), "{status} with the charger offline");
        }
        std::fs::write(battery.join("status"), "Charging").unwrap();
        for invalid in [b"".as_slice(), b"16", b"\xff"] {
            std::fs::write(usb.join("online"), invalid).unwrap();
            assert_eq!(read(&battery), bat(3), "invalid online value {invalid:?}");
        }
        std::fs::remove_file(usb.join("online")).unwrap();
        for status in ["Charging", "Full", "Not charging"] {
            std::fs::write(battery.join("status"), status).unwrap();
            assert_eq!(read(&battery), plugged(3), "{status} without an online attribute");
        }
        for invalid in ["garbage", "-1", "101"] {
            std::fs::write(battery.join("capacity"), invalid).unwrap();
            assert_eq!(read(&battery), None, "{invalid}");
        }
        assert_eq!(read(&root.join("nothing")), None);
    }

    fn check_voltage_veto(
        battery_name: &str, usb_name: &str, model: Option<crate::device::Model>,
        voltage_file: &str, high: &str, low: &str,
    ) {
        let root = TestDir::new("lowbat-voltage");
        let battery = root.join(battery_name);
        let usb = root.join(usb_name);
        std::fs::create_dir(&battery).unwrap();
        std::fs::create_dir(&usb).unwrap();
        std::fs::write(battery.join("type"), "Battery").unwrap();
        std::fs::write(battery.join("status"), "Discharging").unwrap();
        std::fs::write(usb.join("type"), "USB").unwrap();
        std::fs::write(usb.join("online"), "0").unwrap();

        let voltage = battery.join(voltage_file);
        if voltage_file == "voltage_avg" {
            std::fs::write(battery.join("voltage_now"), "0").unwrap();
        }
        let mut guard = Guard::default();
        std::fs::write(battery.join("capacity"), "0").unwrap();
        std::fs::write(&voltage, high).unwrap();
        let alerts: Vec<_> = (0..CONFIRM)
            .filter_map(|_| guard.observe(read_for_guard(&battery, model)))
            .collect();
        assert!(!alerts.contains(&Alert::Critical(0)),
            "{battery_name}: false 0% at {high} triggered shutdown");
        assert!(alerts.is_empty(), "{battery_name}: false 0% at {high} warned");

        std::fs::write(battery.join("capacity"), "1").unwrap();
        std::fs::write(&voltage, low).unwrap();
        assert_eq!(guard.observe(read_for_guard(&battery, model)), Some(Alert::Low(1)));
        // A high-voltage reading breaks the sequence of critical readings.
        std::fs::write(&voltage, high).unwrap();
        assert_eq!(guard.observe(read_for_guard(&battery, model)), None);
        std::fs::write(&voltage, low).unwrap();
        assert_eq!(guard.observe(read_for_guard(&battery, model)), None);
        assert_eq!(guard.observe(read_for_guard(&battery, model)), Some(Alert::Critical(1)));

        std::fs::remove_file(voltage).unwrap();
        let mut guard = Guard::default();
        assert_eq!(guard.observe(read_for_guard(&battery, model)), Some(Alert::Low(1)));
        assert_eq!(guard.observe(read_for_guard(&battery, model)), Some(Alert::Critical(1)));
    }

    #[test]
    fn brick_ignores_a_false_zero_at_4_10_volts() {
        // The standalone card also needs the safeguard without a device profile.
        check_voltage_veto("axp2202-battery", "axp2202-usb", None, "voltage_now", "4100000", "3400000");
    }

    #[test]
    fn rg35xx_plus_and_sp_ignore_a_false_zero_at_3_70_volts() {
        for (name, compatible) in [
            ("Anbernic RG35XX Plus", b"anbernic,rg35xx-plus\0".as_slice()),
            ("Anbernic RG35XX SP", b"anbernic,rg35xx-sp\0".as_slice()),
        ] {
            let model = crate::device::identify(name, compatible).model;
            check_voltage_veto("battery", "axp20x-usb", model, "voltage_avg", "3700", "3699");
        }
    }

    #[test]
    fn mini_v2_ignores_a_false_zero_at_4_135_volts() {
        let model = crate::device::identify("Retroid Pocket Mini V2", b"retroidpocket,rpminiv2\0").model;
        check_voltage_veto("battery", "pm8150b-charger", model, "voltage_now", "4135000", "3400000");
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
        for invalid in [b"".as_slice(), b"0", b"400", b"-4000000", b"400?", b"\xff"] {
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
