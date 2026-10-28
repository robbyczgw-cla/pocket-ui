//! The power menu (holding the power button, or the quick settings' "Power…"): what it offers, the line under each
//! entry, the battery at its top, and what each entry does. Pure data and mapping; the menu itself lives in `quick.rs`
//! (`Screen::Power`) and is drawn by `quick_paint.rs`, each entry with a small drawn mark (`Choice::mark`).

use crate::battery_estimate::Estimate;
use crate::system_ctl::PowerAction;

/// One entry of the power menu.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Choice {
    Sleep,
    Restart,
    PowerOff,
    /// pocket-ui alone starts afresh, the system stays up: a quick way out of a stuck screen without a restart.
    RestartUi,
    /// Restart into the bootloader, where the boot mode can be switched to Android.
    BootMenu,
}

/// The small drawn mark at the left of an entry (docs/DESIGN.md: marks are drawn, not glyphs).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Mark {
    /// A crescent moon.
    Moon,
    /// A ring with an arrow head: round again.
    Round,
    /// The power symbol: a ring open at the top with a bar through the gap.
    Power,
    /// Three spines of a shelf: pocket-ui.
    Shelf,
    /// Three lines: a menu.
    Lines,
}

/// Shown under "Restart to boot menu".
pub const BOOT_MENU_HINT: &str = "Choose Switch boot mode \u{203a} Android there";
/// Shown instead when the kernel can't restart into the bootloader.
pub const NO_BOOT_MENU_HINT: &str = "To start Android: Power off, then Vol Down + Power \u{203a} Switch boot mode";

/// The entries, in order: the everyday ones first, Restart pocket-ui for when something is stuck, "Restart to boot
/// menu" last and only where the device can do it.
pub fn choices(boot_menu: bool) -> Vec<Choice> {
    let mut v = vec![Choice::Sleep, Choice::Restart, Choice::PowerOff, Choice::RestartUi];
    if boot_menu {
        v.push(Choice::BootMenu);
    }
    v
}

/// What the menu knows when it opens: a game is running (it is saved first), ROCKNIX was started from a computer
/// (a restart goes back to Android).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Context {
    pub game: bool,
    pub ram_booted: bool,
}

impl Choice {
    pub fn label(self) -> &'static str {
        match self {
            Choice::Sleep if crate::device::current().screen_off_only() => "Screen off",
            Choice::Sleep => "Sleep",
            Choice::Restart => "Restart",
            Choice::PowerOff => "Power off",
            Choice::RestartUi => "Restart pocket-ui",
            Choice::BootMenu => "Restart to boot menu",
        }
    }

    /// The line under the entry: what happens, in a few words.
    pub fn hint(self, ctx: Context) -> &'static str {
        match self {
            Choice::Sleep if crate::device::current().screen_off_only() => crate::sleep_settings::NO_SLEEP,
            Choice::Sleep if ctx.game => "Saves the game. The power button wakes it.",
            Choice::Sleep => "The power button wakes it.",
            Choice::Restart if ctx.ram_booted => "Started from a computer: this goes back to Android.",
            Choice::Restart if ctx.game => "Saves the game, then restarts the Pocket.",
            Choice::Restart => "Restarts the whole Pocket.",
            Choice::PowerOff if ctx.game => "Saves the game, then switches off.",
            Choice::PowerOff => "Switches the Pocket off.",
            Choice::RestartUi if ctx.game => "Saves the game. Only pocket-ui starts again.",
            Choice::RestartUi => "Only pocket-ui starts again, in seconds.",
            Choice::BootMenu => BOOT_MENU_HINT,
        }
    }

    pub fn mark(self) -> Mark {
        match self {
            Choice::Sleep => Mark::Moon,
            Choice::Restart => Mark::Round,
            Choice::PowerOff => Mark::Power,
            Choice::RestartUi => Mark::Shelf,
            Choice::BootMenu => Mark::Lines,
        }
    }

    /// What the entry does (Restart pocket-ui is no system command: `PowerAction::RestartUi`).
    pub fn action(self) -> PowerAction {
        match self {
            Choice::Sleep => PowerAction::Sleep,
            Choice::Restart => PowerAction::Reboot,
            Choice::PowerOff => PowerAction::PowerOff,
            Choice::RestartUi => PowerAction::RestartUi,
            Choice::BootMenu => PowerAction::RebootBootloader,
        }
    }
}

/// The battery at the top of the menu: the level ("73 %", None without a reading) and what it means in time, from
/// the Battery page's estimate (`battery::headline`): "About 4 h 20 min left", "Charging", "Full", "Not enough data
/// yet"; "Loading…" while the history is still being read, "No battery found" without a level.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BatteryLine {
    pub level: Option<String>,
    pub time: String,
}

/// `percent`: the level now (`status_corner`), None without one; `loaded`: the history is read
/// (`battery_history::loaded`); `est`: its estimate.
pub fn battery_line(percent: Option<u8>, loaded: bool, est: Option<&Estimate>) -> BatteryLine {
    match percent {
        None => BatteryLine { level: None, time: "No battery found".into() },
        Some(p) => BatteryLine {
            level: Some(format!("{p}%")),
            time: if loaded { crate::battery::headline(est) } else { crate::battery_history::LOADING.into() },
        },
    }
}

/// The battery line as it is now, from memory only (the status corner's level, the sampler's history): asked once
/// when the menu opens, never a file read.
pub fn battery_now() -> BatteryLine {
    let percent = crate::status_corner::status().battery.map(|b| b.percent);
    let now = crate::recents::now_secs();
    let history = crate::battery_history::view();
    let est = crate::battery_estimate::estimate(history.recent(now, crate::battery_history::KEEP), now);
    battery_line(percent, crate::battery_history::loaded(), est.as_ref())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHELF: Context = Context { game: false, ram_booted: false };
    const GAME: Context = Context { game: true, ram_booted: false };

    #[test]
    fn the_menu_offers_five_entries_and_drops_the_boot_menu_where_it_cannot_work() {
        assert_eq!(choices(true), [Choice::Sleep, Choice::Restart, Choice::PowerOff, Choice::RestartUi, Choice::BootMenu]);
        assert_eq!(choices(false), [Choice::Sleep, Choice::Restart, Choice::PowerOff, Choice::RestartUi]);
    }

    #[test]
    fn labels_are_the_ones_the_owner_asked_for() {
        let labels: Vec<_> = choices(true).into_iter().map(Choice::label).collect();
        assert_eq!(labels, ["Sleep", "Restart", "Power off", "Restart pocket-ui", "Restart to boot menu"]);
    }

    #[test]
    fn each_entry_maps_to_its_system_command_and_restart_pocket_ui_to_none() {
        let commands: Vec<String> = choices(true).into_iter().map(|c| c.action().argv().join(" ")).collect();
        assert_eq!(commands, ["suspend", "reboot", "poweroff", "", "reboot --reboot-argument=bootloader"]);
        assert_eq!(Choice::RestartUi.action(), PowerAction::RestartUi);
    }

    #[test]
    fn every_entry_has_a_line_and_a_mark_of_its_own() {
        for ctx in [SHELF, GAME, Context { game: true, ram_booted: true }] {
            for c in choices(true) {
                assert!(!c.hint(ctx).is_empty(), "{c:?}");
            }
        }
        let marks: std::collections::HashSet<_> = choices(true).into_iter().map(Choice::mark).collect();
        assert_eq!(marks.len(), 5);
        // In a game the game is saved first, and the lines say so; on the shelf they don't promise a save.
        for c in [Choice::Sleep, Choice::Restart, Choice::PowerOff, Choice::RestartUi] {
            assert!(c.hint(GAME).contains("Saves the game"), "{c:?}");
            assert!(!c.hint(SHELF).contains("Saves"), "{c:?}");
        }
        assert!(Choice::Restart.hint(Context { game: true, ram_booted: true }).contains("Android"));
        assert!(Choice::RestartUi.hint(SHELF).contains("Only pocket-ui"));
    }

    #[test]
    fn the_hints_say_how_to_get_to_android() {
        assert!(BOOT_MENU_HINT.contains("Switch boot mode") && BOOT_MENU_HINT.contains("Android"));
        assert!(NO_BOOT_MENU_HINT.contains("Power off") && NO_BOOT_MENU_HINT.contains("Vol Down + Power") && NO_BOOT_MENU_HINT.contains("Switch boot mode"));
    }

    #[test]
    fn the_battery_line_says_something_understandable_also_without_data() {
        let left = Estimate::Left { per_hour: 12.0, secs: 4 * 3600 + 20 * 60, over: 1800 };
        assert_eq!(
            battery_line(Some(73), true, Some(&left)),
            BatteryLine {
                level: Some("73%".into()),
                time: "About 4 h 20 min left".into()
            }
        );
        assert_eq!(
            battery_line(Some(73), true, Some(&Estimate::Charging { full_in: None, per_hour: 20.0 })).time,
            "Charging"
        );
        assert_eq!(battery_line(Some(100), true, Some(&Estimate::Full)).time, "Full");
        assert_eq!(battery_line(Some(40), true, None).time, "Not enough data yet");
        assert_eq!(battery_line(Some(40), true, Some(&Estimate::NotEnough { have: 60 })).time, "Not enough data yet");
        assert_eq!(battery_line(Some(40), false, Some(&left)).time, "Loading…", "the history not read yet");
        assert_eq!(battery_line(None, true, Some(&left)), BatteryLine { level: None, time: "No battery found".into() });
    }

    #[test]
    fn the_headline_is_the_battery_pages() {
        let s = crate::battery::Sample { t: 0, capacity: 50, charging: false, full: false, mv: 0, ma: 0, temp: None, tag: 0 };
        for est in [
            None,
            Some(Estimate::Left { per_hour: 0.5, secs: 90_000, over: 1800 }),
            Some(Estimate::Charging { full_in: Some(2400), per_hour: 30.0 }),
            Some(Estimate::Charging { full_in: None, per_hour: 30.0 }),
            Some(Estimate::Full),
            Some(Estimate::NotEnough { have: 0 }),
        ] {
            assert_eq!(crate::battery::headline(est.as_ref()), crate::battery::describe(Some(&s), est.as_ref()).0, "{est:?}");
        }
    }
}
