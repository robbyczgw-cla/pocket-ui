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
        Some(t) => format!("Back to the moment from {} · one is kept every 5 min of play", crate::play_time::ago_from(t, now)),
        None => "Nothing earlier yet · a moment is kept every 5 min of play".into(),
    }
}

