//! Settings → System → Storage: the state of the page and what a press asks for. The App measures the card and does
//! the clean-ups on threads (`storage::measure`, `storage::clean`) and reports back with an [`Event`]; painting is in the
//! binary (`app_storage.rs`).
//!
//! One list of what takes the room, each row with its size: the free space first, then Games (A: by system), Save
//! states, Earlier states (what the pause menu's Earlier keeps, with its caps), Saves, Screenshots, Box art and pictures, Logs, Backups, Everything else, and Hidden games (A: the list, A on
//! one shows it again). Four rows can free room with A, then A again: Save states (the automatic states of games not
//! played for 3, 6 or 12 months, left/right, Earlier's among them), Box art and pictures, Logs (earlier starts), Backups (all but the newest).
//! The text under the list says what the selected row holds, and before the second A exactly what goes.

use std::collections::HashMap;
use std::path::PathBuf;

use crate::pad::Button;
use crate::card_health::Health;
use crate::storage::{self, Cleanup, Kind, Played, Roots, Selection, Usage};

/// A row of the main list.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Row {
    /// Free space on the card.
    Free,
    Kind(Kind),
    /// What the pause menu's Earlier keeps (`timeline`), with its caps; read-only (its states go with Save states'
    /// clean-up).
    Earlier,
    Hidden,
    /// Does the system say anything is wrong with the card's file system (`card_health`); read-only.
    Health,
}

/// The main list, in order.
pub const ROWS: [Row; 12] = [
    Row::Free,
    Row::Kind(Kind::Games),
    Row::Kind(Kind::States),
    Row::Earlier,
    Row::Kind(Kind::Saves),
    Row::Kind(Kind::Screenshots),
    Row::Kind(Kind::Pictures),
    Row::Kind(Kind::Logs),
    Row::Kind(Kind::Backups),
    Row::Kind(Kind::Other),
    Row::Hidden,
    Row::Health,
];

/// Which list the page shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum View {
    Main,
    /// Games by system, largest first.
    Systems,
    /// The hidden games.
    Hidden,
}

/// A game hidden from the shelf, as the Hidden games list shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct HiddenGame {
    pub path: PathBuf,
    pub name: String,
    /// The system's short name ("PS1").
    pub system: String,
    /// The file is still on the card.
    pub there: bool,
}

/// A hidden game's name in the list: the shelf's, made from the file name, or the EmulationStation gamelist's
/// (`gamelist_name`) for a cryptic one, as the shelf shows it (`gamelist::better_name`): "sf2ce" was hidden as
/// "Street Fighter II'…" and is listed so.
pub fn hidden_name(path: &std::path::Path, gamelist_name: Option<&str>) -> String {
    let stem = path.file_stem().or_else(|| path.file_name()).map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let cleaned = crate::library::shelf_name(&stem);
    // `better_name` takes the uncased name: a lowercase file name without digits is cryptic only as it is.
    let raw = crate::library::display_name(&stem);
    gamelist_name.and_then(|g| crate::gamelist::better_name(&raw, g)).unwrap_or(cleaned)
}

#[derive(Debug)]
pub enum Event {
    Measured(Box<Usage>),
    /// The card check was read (`card_health::read`).
    Health(Health),
    Cleaned(Cleanup, Result<u64, String>),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    Nothing,
    Close,
    Clean(Cleanup, Selection),
    /// Show this game again (Hidden games).
    Unhide(PathBuf),
}

pub struct StoragePage {
    /// What the last measurement found; None until the first is in.
    pub usage: Option<Usage>,
    /// A measurement runs.
    pub measuring: bool,
    pub view: View,
    /// The selected row of the main list.
    pub sel: usize,
    /// The selected row of the Systems or Hidden list.
    pub sub_sel: usize,
    /// "Not played for": months.
    pub months: u32,
    /// A was pressed once on this row: the next A does it.
    pub confirm: Option<Row>,
    /// A clean-up runs: what to say.
    pub busy: Option<String>,
    /// The last result: text, is an error.
    pub message: Option<(String, bool)>,
    pub hidden: Vec<HiddenGame>,
    /// What the card check found; None until it is in ("…").
    pub health: Option<Health>,
    played: Vec<Played>,
    now: u64,
    roots: Roots,
    /// What each clean-up would take now (worked out when a measurement comes in or the months change).
    selections: HashMap<Cleanup, Selection>,
}

impl StoragePage {
    /// `played`: the recent games (their states count as played); `now`: Unix seconds.
    pub fn new(roots: Roots, played: Vec<Played>, hidden: Vec<HiddenGame>, now: u64) -> StoragePage {
        StoragePage {
            usage: None,
            measuring: false,
            view: View::Main,
            sel: 0,
            sub_sel: 0,
            months: storage::DEFAULT_MONTHS,
            confirm: None,
            busy: None,
            message: None,
            hidden,
            health: None,
            played,
            now,
            roots,
            selections: HashMap::new(),
        }
    }

    /// The rows of the main list.
    pub fn rows(&self) -> &'static [Row] {
        &ROWS
    }

    pub fn row(&self) -> Row {
        self.rows()[self.sel.min(self.rows().len() - 1)]
    }

    /// The clean-up a row's A starts, if it has one.
    pub fn cleanup(&self, row: Row) -> Option<Cleanup> {
        match row {
            Row::Kind(Kind::States) => Some(Cleanup::OldStates(self.months)),
            Row::Kind(Kind::Pictures) => Some(Cleanup::Pictures),
            Row::Kind(Kind::Logs) => Some(Cleanup::OldLogs),
            Row::Kind(Kind::Backups) => Some(Cleanup::OldBackups),
            _ => None,
        }
    }

    /// What `cleanup` would delete now; empty before the first measurement.
    pub fn selection(&self, cleanup: Cleanup) -> Selection {
        self.selections.get(&cleanup).cloned().unwrap_or_default()
    }

    fn select_all(&mut self) {
        self.selections.clear();
        let Some(u) = &self.usage else { return };
        for c in [Cleanup::OldStates(self.months), Cleanup::Pictures, Cleanup::OldLogs, Cleanup::OldBackups] {
            self.selections.insert(c, storage::selection(u, c, &self.played, self.now, &self.roots));
        }
    }

    /// Free space is under [`storage::LOW`].
    pub fn low(&self) -> bool {
        self.usage.as_ref().and_then(|u| u.free).is_some_and(|(free, _)| free < storage::LOW)
    }

    fn sub_len(&self) -> usize {
        match self.view {
            View::Main => self.rows().len(),
            View::Systems => self.usage.as_ref().map_or(0, |u| u.systems.len()),
            View::Hidden => self.hidden.len(),
        }
    }

    pub fn press(&mut self, b: Button, confirm: Button, cancel: Button) -> Action {
        if self.view != View::Main {
            return self.press_list(b, confirm, cancel);
        }
        let n = self.rows().len();
        let before = (self.sel, self.months);
        let action = match b {
            b if b == cancel && self.confirm.take().is_some() => Action::Nothing,
            b if b == cancel => Action::Close,
            Button::Up => {
                self.sel = (self.sel + n - 1) % n;
                Action::Nothing
            }
            Button::Down => {
                self.sel = (self.sel + 1) % n;
                Action::Nothing
            }
            Button::L1 => {
                self.sel = self.sel.saturating_sub(5);
                Action::Nothing
            }
            Button::R1 => {
                self.sel = (self.sel + 5).min(n - 1);
                Action::Nothing
            }
            Button::Left | Button::Right if self.row() == Row::Kind(Kind::States) && self.busy.is_none() => {
                let i = storage::MONTHS.iter().position(|m| *m == self.months).unwrap_or(1);
                let i = if b == Button::Left { i.saturating_sub(1) } else { (i + 1).min(storage::MONTHS.len() - 1) };
                self.months = storage::MONTHS[i];
                self.select_all();
                Action::Nothing
            }
            _ if self.busy.is_some() => Action::Nothing,
            b if b == confirm => self.confirm_row(),
            _ => Action::Nothing,
        };
        if (self.sel, self.months) != before {
            self.confirm = None;
            self.message = None;
        }
        action
    }

    fn confirm_row(&mut self) -> Action {
        let row = self.row();
        match row {
            Row::Kind(Kind::Games) => {
                self.view = View::Systems;
                self.sub_sel = 0;
                Action::Nothing
            }
            Row::Hidden => {
                self.view = View::Hidden;
                self.sub_sel = 0;
                Action::Nothing
            }
            _ => {
                let Some(c) = self.cleanup(row) else { return Action::Nothing };
                let sel = self.selection(c);
                if self.usage.is_none() || sel.paths.is_empty() {
                    return Action::Nothing;
                }
                // Deleting asks twice.
                if self.confirm != Some(row) {
                    self.confirm = Some(row);
                    return Action::Nothing;
                }
                self.confirm = None;
                self.busy = Some(if c == Cleanup::Pictures { "Clearing…".into() } else { "Deleting…".into() });
                Action::Clean(c, sel)
            }
        }
    }

    fn press_list(&mut self, b: Button, confirm: Button, cancel: Button) -> Action {
        let n = self.sub_len();
        match b {
            b if b == cancel => {
                self.view = View::Main;
                self.message = None;
            }
            Button::Up if n > 0 => self.sub_sel = (self.sub_sel + n - 1) % n,
            Button::Down if n > 0 => self.sub_sel = (self.sub_sel + 1) % n,
            Button::L1 => self.sub_sel = self.sub_sel.saturating_sub(5),
            Button::R1 if n > 0 => self.sub_sel = (self.sub_sel + 5).min(n - 1),
            b if b == confirm && self.view == View::Hidden && n > 0 => {
                let g = self.hidden.remove(self.sub_sel.min(n - 1));
                self.sub_sel = self.sub_sel.min(self.hidden.len().saturating_sub(1));
                self.message = Some((
                    if g.there { format!("Shown again: {}", g.name) } else { format!("Taken off the list: {}", g.name) },
                    false,
                ));
                return Action::Unhide(g.path);
            }
            _ => {}
        }
        Action::Nothing
    }

    pub fn event(&mut self, e: Event) {
        match e {
            Event::Measured(u) => {
                self.usage = Some(*u);
                self.measuring = false;
                self.select_all();
            }
            Event::Health(h) => self.health = Some(h),
            Event::Cleaned(c, result) => {
                self.busy = None;
                self.message = Some(match result {
                    Ok(bytes) => (format!("{} freed.", storage::size(bytes)), false),
                    Err(e) => (e, true),
                });
                // Nothing of it is left to delete; the next measurement says what is.
                self.selections.insert(c, Selection::default());
            }
        }
    }

    /// The hint for A on the main list; None where A does nothing.
    pub fn a_hint(&self) -> Option<&'static str> {
        let row = self.row();
        match row {
            Row::Kind(Kind::Games) | Row::Hidden => Some("Open"),
            _ if self.busy.is_some() => None,
            r if self.cleanup(r).is_some_and(|c| !self.selection(c).paths.is_empty()) => Some(match (self.confirm == Some(r), r) {
                (true, Row::Kind(Kind::Pictures)) => "Clear now",
                (true, _) => "Delete now",
                (false, Row::Kind(Kind::Pictures)) => "Clear",
                (false, _) => "Delete old",
            }),
            _ => None,
        }
    }

    /// The value at the right of a main row.
    pub fn value(&self, row: Row) -> String {
        if row == Row::Hidden {
            // Known without counting.
            return format!("{} >", self.hidden.len());
        }
        if row == Row::Health {
            return health_value(self.health).into();
        }
        let Some(u) = &self.usage else { return "…".into() };
        match row {
            Row::Free => u.free.map_or_else(|| "unknown".to_string(), |(free, total)| format!("{} of {}", storage::size(free), storage::size(total))),
            Row::Kind(Kind::Games) => format!("{} >", storage::size(u.bytes(Kind::Games))),
            Row::Kind(k) => storage::size(u.bytes(k)),
            Row::Earlier => storage::size(u.earlier),
            Row::Hidden => format!("{} >", self.hidden.len()),
            Row::Health => unreachable!("answered above"),
        }
    }

    /// The text under the list: a heading, its lines, whether it warns.
    pub fn details(&self) -> (String, Vec<String>, bool) {
        if let Some(b) = &self.busy {
            return (b.clone(), vec!["This takes a moment.".into()], false);
        }
        if let Some((m, err)) = &self.message {
            return (if *err { "That did not work".into() } else { "Done".into() }, vec![m.clone()], *err);
        }
        let row = self.row();
        if let (Some(c), true) = (self.cleanup(row), self.confirm == Some(row)) {
            return confirm_text(c, &self.selection(c));
        }
        if row == Row::Health {
            return health_details(self.health);
        }
        let Some(u) = &self.usage else {
            return ("Counting…".into(), vec!["Adding up what is on the SD card. This takes a few seconds.".into()], false);
        };
        let label = match row {
            Row::Free => "Free on the SD card".to_string(),
            Row::Kind(k) => k.label().to_string(),
            Row::Earlier => "Earlier states".to_string(),
            Row::Hidden => "Hidden games".to_string(),
            Row::Health => unreachable!("answered above"),
        };
        let mut lines = Vec::new();
        let mut warn = false;
        match row {
            Row::Free => {
                if self.low() {
                    warn = true;
                    lines.push("Less than 1 GB is left. Once the SD card is full, states can't be written and saves can fail. Delete what you don't need below.".into());
                } else {
                    lines.push("pocket-ui says so on the shelf once less than 1 GB is left.".into());
                }
                if u.unreadable > 0 {
                    lines.push(format!("{} folders could not be read; what is in them is not counted.", u.unreadable));
                }
            }
            Row::Kind(Kind::Games) => {
                lines.push(format!("On {} systems. A lists them by size.", u.systems.len()));
                lines.push("To delete a game, open its card (X on the game), then Hide or delete.".into());
            }
            Row::Kind(Kind::States) => {
                let sel = self.selection(Cleanup::OldStates(self.months));
                if sel.paths.is_empty() {
                    lines.push(format!("No automatic states of games not played for {} months.", self.months));
                } else {
                    lines.push(match sel.count {
                        1 => format!("A deletes the automatic states of 1 game not played for {} months: {}.", self.months, storage::size(sel.bytes)),
                        n => format!("A deletes the automatic states of {n} games not played for {} months: {}.", self.months, storage::size(sel.bytes)),
                    });
                }
                lines.push("Slots you saved into and your saves stay. Left/right: 3, 6 or 12 months.".into());
            }
            Row::Kind(Kind::Saves) => lines.push("Battery saves and memory cards. Never deleted here.".into()),
            // Earlier (`timeline`): the most its states may take; nothing to delete here.
            Row::Earlier => {
                lines.push(format!(
                    "Kept by the pause menu's Earlier: at most {} in all, {} a game. The oldest go first.",
                    storage::size(crate::timeline::ALL_CAP),
                    storage::size(crate::timeline::GAME_CAP)
                ));
                lines.push(if u.earlier == 0 { "None yet: one every 5 min of play.".into() } else { "The Save states row above deletes those of old games.".into() });
            }
            Row::Kind(Kind::Screenshots) => lines.push("In the screenshots folder of the games share. Delete them from your computer.".into()),
            Row::Kind(Kind::Pictures) => {
                lines.push("A clears them. Box art and game pictures are downloaded again when the Pocket is online.".into());
            }
            Row::Kind(Kind::Logs) => {
                let sel = self.selection(Cleanup::OldLogs);
                if sel.paths.is_empty() {
                    lines.push("Only this start's logs. pocket-ui keeps those of three starts at most.".into());
                } else {
                    lines.push(format!("A deletes the logs of earlier starts: {}. This start's logs stay.", storage::size(sel.bytes)));
                }
            }
            Row::Kind(Kind::Backups) => {
                let sel = self.selection(Cleanup::OldBackups);
                if sel.paths.is_empty() {
                    lines.push("Nothing old to delete: one backup of pocket-ui's settings or none.".into());
                } else {
                    lines.push(match sel.count {
                        1 => format!("A deletes 1 older backup of pocket-ui's settings: {}. The newest stays.", storage::size(sel.bytes)),
                        n => format!("A deletes {n} older backups of pocket-ui's settings: {}. The newest stays.", storage::size(sel.bytes)),
                    });
                }
            }
            Row::Kind(Kind::Other) => lines.push("BIOS files, music, the data of ports and anything else on the SD card.".into()),
            Row::Hidden if self.hidden.is_empty() => lines.push("No hidden games. Hide one from its card: X on a game, then Hide or delete.".into()),
            Row::Hidden => lines.push("Games you hid from the shelf. A lists them; A on one shows it again.".into()),
            Row::Health => unreachable!("answered above"),
        }
        (label, lines, warn)
    }
}

/// The value of the Card check row.
pub fn health_value(h: Option<Health>) -> &'static str {
    match h {
        None => "…",
        Some(Health::Fine) => "No errors seen",
        Some(Health::Errors) => "Errors seen",
        Some(Health::ReadOnly) => "Read-only",
        Some(Health::Unknown) => "Can't tell",
    }
}

/// The text under the list for the Card check row: a heading, its lines, whether it warns. Never a repair: it says what
/// to do on a computer.
pub fn health_details(h: Option<Health>) -> (String, Vec<String>, bool) {
    // Three lines at most in Large text, in German too: what to do comes in the second.
    match h {
        None => ("SD card check".into(), vec!["Looking at what the system says about the SD card…".into()], false),
        Some(Health::Fine) => (
            "SD card check".into(),
            vec!["The system logged no file system errors. That is not a full test of the SD card.".into()],
            false,
        ),
        Some(Health::Errors) => (
            "The system saw errors on the SD card".into(),
            vec!["Games or saves may go missing.".into(), "Copy your saves to a computer, then check the SD card there.".into()],
            true,
        ),
        Some(Health::ReadOnly) => (
            "The SD card is read-only".into(),
            vec!["Saves and states can't be kept.".into(), "Restart once. If it keeps happening, copy your saves to a computer and check the SD card there.".into()],
            true,
        ),
        Some(Health::Unknown) => (
            "SD card check".into(),
            vec!["The Pocket could not read what the system says about the SD card. That does not mean it is fine.".into()],
            false,
        ),
    }
}

/// What the second A of a clean-up deletes, in words.
pub fn confirm_text(c: Cleanup, sel: &Selection) -> (String, Vec<String>, bool) {
    let size = storage::size(sel.bytes);
    let (head, line) = match c {
        Cleanup::OldStates(months) => (
            format!("Delete {size} of automatic states?"),
            match sel.count {
                1 => format!("The resume states, Before load and earlier states of 1 game not played for {months} months. Slots and saves stay. Press A again; B keeps them."),
                n => format!("The resume states, Before load and earlier states of {n} games not played for {months} months. Slots and saves stay. Press A again; B keeps them."),
            },
        ),
        Cleanup::Pictures => (
            format!("Clear {size} of pictures?"),
            "Box art and game pictures, including the ones you put there. They are downloaded again when the Pocket is online. Press A again; B keeps them.".into(),
        ),
        Cleanup::OldLogs => (format!("Delete {size} of old logs?"), "The logs of earlier starts. Press A again; B keeps them.".into()),
        Cleanup::OldBackups => (
            if sel.count == 1 { "Delete 1 old backup?".to_string() } else { format!("Delete {} old backups?", sel.count) },
            format!("{size} of pocket-ui's settings backups. The newest stays. Press A again; B keeps them."),
        ),
    };
    (head, vec![line], true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{AutoState, Folder};

    const A: Button = Button::East;
    const B: Button = Button::South;

    /// VDEV-ROUND-2026-10-25: a game hidden from a shelf that showed its gamelist name is listed under that name.
    #[test]
    fn a_hidden_game_keeps_the_name_the_shelf_gave_it() {
        let sf2 = std::path::Path::new("/storage/roms/mame/sf2ce.zip");
        assert_eq!(hidden_name(sf2, Some("Street Fighter II': Champion Edition (World 920513)")), "Street Fighter II': Champion Edition (World 920513)");
        assert_eq!(hidden_name(sf2, None), "sf2ce", "no gamelist: the file's name");
        let tetris = std::path::Path::new("/storage/roms/gb/Tetris (World) (Rev 1).gb");
        assert_eq!(hidden_name(tetris, Some("Tetris (World)")), crate::library::display_name("Tetris (World) (Rev 1)"), "a readable file name keeps the shelf's name");
    }

    fn roots() -> Roots {
        Roots { roms: "/r".into(), data: "/d".into(), backups: "/r/backup/pocket-ui".into() }
    }

    fn usage() -> Usage {
        let old = storage::now() - 400 * 24 * 3600;
        let mut u = Usage::default();
        u.auto_states = vec![AutoState { path: "/r/savestates/gba/A.pstate.auto".into(), system: "gba".into(), base: "A".into(), bytes: 2_000_000, modified: old }];
        u.old_logs = vec![Folder { path: "/d/logs/boot-0001".into(), bytes: 300_000 }];
        u.backups = vec![Folder { path: "/r/backup/pocket-ui/new".into(), bytes: 5_000 }];
        u.systems = vec![("PlayStation 2".into(), 9_000_000_000), ("Game Boy Advance".into(), 4_000_000)];
        u.free = Some((800_000_000, 64_000_000_000));
        u
    }

    fn page() -> StoragePage {
        let hidden = vec![HiddenGame { path: "/r/gba/Demo.gba".into(), name: "Demo".into(), system: "GBA".into(), there: true }];
        let mut p = StoragePage::new(roots(), Vec::new(), hidden, storage::now());
        p.event(Event::Measured(Box::new(usage())));
        p
    }

    fn at(p: &mut StoragePage, row: Row) {
        p.sel = p.rows().iter().position(|r| *r == row).unwrap();
    }

    /// The numbers of the last visit stay on screen while the new walk runs (never "…" or an empty gap), and the new
    /// ones replace them when they are in.
    #[test]
    fn old_numbers_stay_while_the_page_counts_again() {
        let mut p = page();
        let before = p.value(Row::Kind(Kind::Games));
        assert_ne!(before, "…");
        p.measuring = true;
        assert_eq!(p.value(Row::Kind(Kind::Games)), before);
        assert_ne!(p.details().0, "Counting…", "the details are the old ones too");
        let mut u = usage();
        u.free = Some((1_000_000_000, 64_000_000_000));
        p.event(Event::Measured(Box::new(u)));
        assert!(!p.measuring, "the walk is in");
        assert!(p.value(Row::Free).starts_with("1.0 GB of "), "{}", p.value(Row::Free));
    }

    /// VDEV-ROUND-2026-10-25: "1 games", "1 alte Sicherungen" read wrong; one of a kind is said in the singular.
    #[test]
    fn one_game_or_one_backup_is_said_in_the_singular() {
        let sel = |count| Selection { paths: vec![PathBuf::from("/x")], bytes: 2_000_000, count };
        assert_eq!(confirm_text(Cleanup::OldBackups, &sel(1)).0, "Delete 1 old backup?");
        assert_eq!(confirm_text(Cleanup::OldBackups, &sel(3)).0, "Delete 3 old backups?");
        assert!(confirm_text(Cleanup::OldStates(6), &sel(1)).1[0].contains("states of 1 game not played"));
        assert!(confirm_text(Cleanup::OldStates(6), &sel(2)).1[0].contains("states of 2 games not played"));
    }

    #[test]
    fn deleting_asks_twice_and_says_exactly_what_goes() {
        let mut p = page();
        at(&mut p, Row::Kind(Kind::States));
        assert_eq!(p.a_hint(), Some("Delete old"));
        assert_eq!(p.press(A, A, B), Action::Nothing, "the first A only asks");
        let (head, lines, warn) = p.details();
        assert_eq!(head, "Delete 2 MB of automatic states?");
        assert!(warn && lines[0].contains("of 1 game not played for 6 months") && lines[0].contains("Slots and saves stay"), "{lines:?}");
        assert_eq!(p.a_hint(), Some("Delete now"));
        assert_eq!(p.press(B, A, B), Action::Nothing, "B takes it back");
        assert_eq!(p.press(B, A, B), Action::Close, "then B leaves");

        let mut p = page();
        at(&mut p, Row::Kind(Kind::States));
        p.press(A, A, B);
        let Action::Clean(Cleanup::OldStates(6), sel) = p.press(A, A, B) else { panic!("the second A deletes") };
        assert_eq!(sel.paths, vec![PathBuf::from("/r/savestates/gba/A.pstate.auto")]);
        assert_eq!(p.press(A, A, B), Action::Nothing, "one at a time");
        p.event(Event::Cleaned(Cleanup::OldStates(6), Ok(2_000_000)));
        assert_eq!(p.details().1, vec!["2 MB freed.".to_string()]);
        assert_eq!(p.press(A, A, B), Action::Nothing, "nothing left to delete until it is measured again");
    }

    #[test]
    fn the_earlier_row_says_what_it_takes_and_the_most_it_may() {
        // None yet: it says so, with the caps.
        let mut p = page();
        at(&mut p, Row::Earlier);
        assert_eq!(p.value(Row::Earlier), "0 MB");
        let (head, lines, warn) = p.details();
        assert_eq!((head.as_str(), warn), ("Earlier states", false));
        assert_eq!(lines, ["Kept by the pause menu's Earlier: at most 1.0 GB in all, 100 MB a game. The oldest go first.", "None yet: one every 5 min of play."]);
        assert_eq!((p.a_hint(), p.press(A, A, B)), (None, Action::Nothing), "nothing to delete here");
        let mut u = usage();
        u.earlier = 84_000_000;
        p.event(Event::Measured(Box::new(u)));
        assert_eq!(p.value(Row::Earlier), "84 MB");
        assert_eq!(p.details().1[1], "The Save states row above deletes those of old games.");
    }

    #[test]
    fn moving_or_changing_the_months_takes_the_question_back() {
        let mut p = page();
        at(&mut p, Row::Kind(Kind::States));
        p.press(A, A, B);
        p.press(Button::Left, A, B);
        assert_eq!((p.confirm, p.months), (None, 3));
        p.press(Button::Right, A, B);
        p.press(Button::Right, A, B);
        p.press(Button::Right, A, B);
        assert_eq!(p.months, 12, "3, 6 or 12");
        p.press(A, A, B);
        p.press(Button::Down, A, B);
        assert_eq!(p.confirm, None);
    }

    #[test]
    fn saves_screenshots_and_the_rest_have_no_delete() {
        let mut p = page();
        for row in [Row::Free, Row::Kind(Kind::Saves), Row::Kind(Kind::Screenshots), Row::Kind(Kind::Other)] {
            at(&mut p, row);
            assert_eq!(p.a_hint(), None, "{row:?}");
            assert_eq!(p.press(A, A, B), Action::Nothing);
            assert_eq!(p.press(A, A, B), Action::Nothing, "{row:?}: no second A either");
        }
        at(&mut p, Row::Kind(Kind::Saves));
        assert!(p.details().1[0].contains("Never deleted"), "{:?}", p.details());
    }

    #[test]
    fn low_space_is_said_on_the_free_row() {
        let mut p = page();
        assert!(p.low());
        let (_, lines, warn) = p.details();
        assert!(warn && lines[0].starts_with("Less than 1 GB"));
        assert_eq!(p.value(Row::Free), "800 MB of 64.0 GB");
        let mut u = usage();
        u.free = Some((20_000_000_000, 64_000_000_000));
        p.event(Event::Measured(Box::new(u)));
        assert!(!p.low() && !p.details().2);
    }

    #[test]
    fn before_the_first_measurement_the_page_says_it_is_counting() {
        let mut p = StoragePage::new(roots(), Vec::new(), Vec::new(), storage::now());
        assert_eq!(p.value(Row::Kind(Kind::Games)), "…");
        assert_eq!(p.details().0, "Counting…");
        at(&mut p, Row::Kind(Kind::Pictures));
        assert_eq!(p.press(A, A, B), Action::Nothing, "nothing to clear before it is known");
        at(&mut p, Row::Hidden);
        p.press(A, A, B);
        assert_eq!(p.view, View::Hidden, "the hidden games are known without counting");
        assert_eq!(p.press(A, A, B), Action::Nothing, "an empty list");
    }

    #[test]
    fn hidden_games_come_back_one_press_each_and_games_open_by_system() {
        let mut p = page();
        at(&mut p, Row::Hidden);
        assert_eq!(p.value(Row::Hidden), "1 >");
        p.press(A, A, B);
        assert_eq!(p.press(A, A, B), Action::Unhide(PathBuf::from("/r/gba/Demo.gba")));
        assert!(p.hidden.is_empty());
        assert_eq!(p.message.as_ref().map(|m| m.0.as_str()), Some("Shown again: Demo"));
        p.press(B, A, B);
        assert_eq!((p.view, p.row()), (View::Main, Row::Hidden), "back on its row");
        assert!(p.details().1[0].starts_with("No hidden games"));

        at(&mut p, Row::Kind(Kind::Games));
        assert_eq!(p.value(Row::Kind(Kind::Games)), "0 MB >");
        p.press(A, A, B);
        assert_eq!(p.view, View::Systems);
        p.press(Button::Down, A, B);
        assert_eq!(p.sub_sel, 1);
    }
}
