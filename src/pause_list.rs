//! The rows of the handhelds' pause menu in Android's player when the game came from the Handheld view: what each row
//! does (`Item`), its words and value (`Row`), and Earlier's words. The page itself is painted by Linux's painter
//! (`pause_page`), the same code as Linux's menu; until 17.11.2026 this file held a copy of it that drifted (the
//! Retroid Pocket Mini V2 hand test: "the menus are still a bit different from Linux").

/// What a row of the main page does. The player matches its actions on these, never on the words.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Item {
    Continue,
    Save,
    Load,
    Earlier,
    /// Which disc is in the drive, for a game of several (Linux's `MainItem::Disc`, `disc::DiscRow`).
    Disc,
    Options,
    Achievements,
    Quit,
}

impl Item {
    /// Whether A opens a page (Options, Achievements): the row ends in the drawn angle on every row and A's hint says
    /// Open, as Linux's (`main.rs` `MainItem::opens_page`, 0.5.1).
    pub fn opens_page(self) -> bool {
        matches!(self, Item::Options | Item::Achievements)
    }

    /// The row's name, Linux's (`main.rs` `MainItem::label`).
    pub fn label(self) -> &'static str {
        match self {
            Item::Continue => "Continue",
            Item::Save => "Save",
            Item::Load => "Load",
            Item::Earlier => "Earlier",
            Item::Disc => "Disc",
            Item::Options => "Options",
            Item::Achievements => "Achievements",
            Item::Quit => "Quit",
        }
    }

    /// What the row says on screen: Quit reads "Save & quit" where it keeps the moment, as Linux's (`MainItem::shown`).
    pub fn shown(self, quit_saves: bool) -> &'static str {
        if self == Item::Quit && quit_saves { "Save & quit" } else { self.label() }
    }

    /// The row's picture in the Retro style, Linux's (`main.rs` `retro_icon`).
    pub fn icon(self) -> crate::retro::Icon {
        use crate::retro::Icon;
        match self {
            Item::Continue => Icon::Continue,
            Item::Save => Icon::Save,
            Item::Load | Item::Earlier => Icon::Load,
            Item::Disc => Icon::Games,
            Item::Options => Icon::Options,
            Item::Achievements => Icon::Playing,
            Item::Quit => Icon::Quit,
        }
    }
}

/// The main page's rows in Linux's order (`main.rs` `main_items`, `arrange_items`) for a game of Android's player:
/// Continue, Save, Load, Earlier while it has a row, Disc for a game of several, Options, Achievements while signed in,
/// Quit. The rows Linux has and the player hasn't (Zoom as a row of its own, Pixel Path, Players) are in the
/// comparison test's exceptions, each with why.
pub fn order(earlier: bool, disc: bool, achievements: bool) -> Vec<Item> {
    let mut v = vec![Item::Continue, Item::Save, Item::Load];
    if earlier {
        v.push(Item::Earlier);
    }
    if disc {
        v.push(Item::Disc);
    }
    v.push(Item::Options);
    if achievements {
        v.push(Item::Achievements);
    }
    v.push(Item::Quit);
    v
}

/// A row of the main page: its words, its value (the slot, Earlier's time) and whether Left/Right step that value.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub item: Item,
    pub label: String,
    pub chip: Option<String>,
    pub steps: bool,
    /// Grey: Load on an empty slot, Earlier with nothing yet, Hardcore.
    pub enabled: bool,
}

/// Load's last stop, after the slots and the kept moments: the game from its start (Linux's `QuickMenu::at_restart`,
/// pocket-core's `start_over`). The same words as Linux's: the value beside Load and in the empty window, the line
/// under it, and what shows over the game after it.
pub const FROM_START: &str = "From the start";
pub const FROM_START_NOTE: &str = "Starts over · Quit replaces it";
pub const FROM_START_DONE: &str = "From the start. Save & quit replaces the waiting moment";

/// One step of Load's value, as Linux's Load steps (`load_stops`): through the `places` (the slots, then the kept
/// moments), then From the start, round. `at` is the place the slot cursor is on, `start` whether Load is on From the
/// start. While Load is on From the start the cursor keeps its place, so Save stays on its slot (Linux keeps Save's
/// slot and Load's stop apart).
pub fn step_load(at: usize, start: bool, places: usize, back: bool) -> (usize, bool) {
    let places = places.max(1);
    let stops = places + 1;
    let now = if start { places } else { at.min(places - 1) };
    let next = if back { (now + stops - 1) % stops } else { (now + 1) % stops };
    if next == places { (at, true) } else { (next, false) }
}

/// Linux's slot blocks under the picture (`app_save_media`'s `ROW_BLOCK`, `ROW_GAP`).
pub const ROW_BLOCK: i32 = 30;
pub const ROW_GAP: i32 = 12;

/// Earlier's value beside the row: how long ago, said in full so the row explains itself ("12 min ago", "just now"),
/// never a bare "12 min" (the Odin hand test: "Earlier" wasn't understood, 15.11.).
pub fn earlier_chip(then: u64, now: u64) -> String {
    crate::play_time::ago_from(then, now)
}

/// The line under Earlier's picture: what A does, and how the moments come about.
pub fn earlier_note(then: Option<u64>, now: u64) -> String {
    match then {
        Some(t) => format!("Jump back to {} (auto-saved every 5 min of play)", crate::play_time::ago_from(t, now)),
        None => "No earlier backups yet (auto-saved every 5 min of play)".into(),
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    /// Load's value goes through the slots and the kept moments, then From the start, and round both ways (Linux's
    /// Load). Save's slot stays where it was while Load is on From the start.
    #[test]
    fn load_steps_to_from_the_start_after_the_slots_and_round() {
        // 4 slots and one kept moment: places 0..=4, then From the start.
        let mut at = (0, false);
        let mut seen = vec![];
        for _ in 0..7 {
            at = step_load(at.0, at.1, 5, false);
            seen.push(at);
        }
        assert_eq!(seen, [(1, false), (2, false), (3, false), (4, false), (4, true), (0, false), (1, false)]);
        // Back from the first slot: From the start, then the last place.
        assert_eq!(step_load(0, false, 5, true), (0, true));
        assert_eq!(step_load(2, true, 5, true), (4, false));
        // While on From the start the cursor keeps the slot Save is on.
        assert_eq!(step_load(2, false, 3, false), (2, true));
        // A single place still has its From the start.
        assert_eq!(step_load(0, false, 1, false), (0, true));
        assert_eq!(step_load(0, true, 1, false), (0, false));
    }

    /// The words are Linux's, so every language has them already (`assets/i18n`).
    #[test]
    fn from_the_start_says_what_linux_says() {
        let linux = include_str!("main.rs");
        let core = include_str!("bin/pocket_core.rs");
        assert!(linux.contains(&format!("\"{FROM_START}\"")) && linux.contains(&format!("\"{FROM_START_NOTE}\"")));
        assert!(core.contains(&format!("\"{FROM_START_DONE}\"")));
        let de = include_str!("../assets/i18n/de.tsv");
        for line in [FROM_START, FROM_START_NOTE, FROM_START_DONE] {
            assert!(de.lines().any(|l| l.split('\t').next() == Some(line)), "no German line for {line:?}");
        }
    }

    /// Android's player (a game from the Handheld view) has Load's last stop: L/R and Left/Right reach it on Load, the
    /// row and the window show "From the start", and A (or a tap on the row) starts the game again as pocket-core does:
    /// the console's reset with the cartridge clock kept, achievements waiting again, rewind's history gone, no Before
    /// load. Not in Hardcore. The player runs only on Android, so its source is checked here.
    #[test]
    fn android_pause_menu_has_from_the_start_on_load() {
        let p = include_str!("../android/src/player.rs");
        assert!(p.contains("pause_list::step_load(self.slot, self.from_start, self.places(), back)"), "Load must step to From the start");
        assert!(p.contains("Item::Load if self.at_start() => row(item, item.label(), Some(pause_list::FROM_START.into())"), "Load's row must show From the start");
        assert!(p.contains("empty: pause_list::FROM_START, note"), "the window must say From the start");
        let action = p.split("fn activate_list(&mut self)").nth(1).unwrap().split("let legacy").next().unwrap();
        assert!(action.contains("self.at_start()") && action.contains("self.start_over()") && action.contains("FROM_START_DONE"), "A on From the start must start over");
        let start = p.split("fn start_over(&mut self)").nth(1).unwrap().split("\n    fn ").next().unwrap();
        for (call, why) in [("self.core.restart()", "the console's reset, keeping the cartridge clock"), ("ra.reset()", "achievements wait again"), ("self.speed.forget()", "rewind forgets the old timeline")] {
            assert!(start.contains(call), "From the start: {why}");
        }
        assert!(!start.contains("undo"), "From the start writes no Before load (Linux's start_over)");
        let at = p.split("fn at_start(&self)").nth(1).unwrap().split("\n    }").next().unwrap();
        assert!(at.contains("!self.hardcore()"), "no From the start in Hardcore");
        // A tap on the row goes back to the game: the touch says so, so Java draws continuously again.
        let tap = p.split("fn touch(&mut self").nth(1).unwrap().split("if self.paused && self.menu == Menu::Zoom").next().unwrap();
        assert!(tap.contains("self.activate_list();") && tap.trim_end().ends_with("return !self.paused;\n        }"), "a tap back to the game must restart the drawing");
    }
}
