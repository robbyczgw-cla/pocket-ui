//! "Two versions": the page that lets the player choose which version of a save to keep (`sync_conflicts`). Opened
//! from a game's card (only that game's saves, straight on its versions when there is one) or from Settings →
//! Connections → Sync (every save waiting, as a list first). The state and what a press asks for; painting is in the
//! binary (`app_sync_conflicts.rs`).

use crate::pad::Button;
use crate::sync_conflicts::{self, Group, Kind, Owner};

/// A save waiting for a choice, as the page lists it.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub group: Group,
    pub owner: Owner,
    /// The game's name, or the file's for a memory card or save data no game of the library owns.
    pub title: String,
}

/// What the page shows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum View {
    /// Every entry; A opens one.
    List,
    /// The versions of entry `n`; A keeps the highlighted one.
    Versions(usize),
}

/// What a press asks the App to do.
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    Nothing,
    Close,
    /// Keep version `pick` (`Group::versions`) of entry `entry`, move the others aside (on a thread).
    Keep { entry: usize, pick: usize },
}

pub struct ConflictPage {
    pub entries: Vec<Entry>,
    pub view: View,
    /// The highlighted row of the view.
    pub sel: usize,
    /// The list's row while a version view is open.
    list_sel: usize,
    /// Opened on one game's versions (from its card): B closes the page instead of going to the list.
    pub direct: bool,
    /// A choice is being carried out.
    pub busy: bool,
    /// The last choice's outcome: "Kept. The other version is in …", or what went wrong.
    pub message: Option<(String, bool)>,
}

/// One version as a row: what it is, and when and how big.
#[derive(Clone, Debug, PartialEq)]
pub struct VersionRow {
    /// "In use now", "Other version", "Other version 2".
    pub label: String,
    /// "24 Oct 2026, 18:02".
    pub when: String,
    /// "32 KB".
    pub size: String,
    /// The most recently changed version (only when the times differ).
    pub newest: bool,
}

impl ConflictPage {
    /// The page for `entries`. `direct`: from a card; with one entry it opens on its versions.
    pub fn new(entries: Vec<Entry>, direct: bool) -> ConflictPage {
        let view = if direct && entries.len() == 1 { View::Versions(0) } else { View::List };
        ConflictPage {
            entries,
            view,
            sel: 0,
            list_sel: 0,
            direct,
            busy: false,
            message: None,
        }
    }

    pub fn len(&self) -> usize {
        match self.view {
            View::List => self.entries.len(),
            View::Versions(n) => self.entries.get(n).map_or(0, |e| e.group.versions().len()),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The entry whose versions are shown.
    pub fn entry(&self) -> Option<&Entry> {
        match self.view {
            View::Versions(n) => self.entries.get(n),
            View::List => None,
        }
    }

    pub fn press(&mut self, b: Button, confirm: Button, cancel: Button) -> Action {
        let n = self.len();
        if b == Button::Up && n > 0 {
            self.sel = (self.sel + n - 1) % n;
        } else if b == Button::Down && n > 0 {
            self.sel = (self.sel + 1) % n;
        } else if b == cancel {
            if self.busy {
                return Action::Nothing;
            }
            return match self.view {
                View::Versions(_) if !self.direct || self.entries.len() > 1 => {
                    self.view = View::List;
                    self.sel = self.list_sel.min(self.entries.len().saturating_sub(1));
                    Action::Nothing
                }
                _ => Action::Close,
            };
        } else if b == confirm && !self.busy {
            match self.view {
                View::List if n > 0 => {
                    self.list_sel = self.sel;
                    self.view = View::Versions(self.sel);
                    self.message = None;
                    // The version in use is the first row.
                    self.sel = 0;
                }
                View::Versions(entry) if n > 0 => {
                    self.busy = true;
                    return Action::Keep { entry, pick: self.sel };
                }
                _ => {}
            }
        }
        Action::Nothing
    }

    /// The choice is made (or failed). Done, the entry leaves the list and the page shows the list (what is left) with
    /// the outcome on top, where the player reads it before B closes the page.
    pub fn kept(&mut self, entry: usize, result: Result<String, String>) {
        self.busy = false;
        match result {
            Ok(text) => {
                if entry < self.entries.len() {
                    self.entries.remove(entry);
                }
                self.message = Some((text, false));
                self.view = View::List;
                self.sel = self.list_sel.min(self.entries.len().saturating_sub(1));
            }
            Err(e) => self.message = Some((e, true)),
        }
    }

    /// A new search came in while the page is open: its list follows, the open entry stays open while it is there.
    pub fn update(&mut self, entries: Vec<Entry>) {
        if self.busy {
            return;
        }
        let open = self.entry().map(|e| e.group.original.clone());
        self.entries = entries;
        match open.and_then(|o| self.entries.iter().position(|e| e.group.original == o)) {
            Some(i) => {
                self.view = View::Versions(i);
                self.sel = self.sel.min(self.len().saturating_sub(1));
            }
            None => {
                self.view = View::List;
                self.sel = self.sel.min(self.entries.len().saturating_sub(1));
            }
        }
    }

    /// Where the page stands, for the menu sounds.
    pub fn mark(&self) -> (usize, usize, bool, usize) {
        let view = match self.view {
            View::List => 0,
            View::Versions(n) => n + 1,
        };
        (view, self.sel, self.busy, self.entries.len())
    }
}

/// The rows of a group's versions, the one in use first.
pub fn version_rows(g: &Group) -> Vec<VersionRow> {
    let versions = g.versions();
    let newest = versions.iter().map(|v| v.mtime).max().unwrap_or(0);
    let differ = versions.iter().any(|v| v.mtime != newest);
    let several = g.others.len() > 1;
    let mut other = 0;
    versions
        .iter()
        .map(|v| {
            let in_use = g.current.as_ref().is_some_and(|c| c.path == v.path);
            let label = if in_use {
                "In use now".to_string()
            } else {
                other += 1;
                if several {
                    format!("Other version {other}")
                } else {
                    "Other version".to_string()
                }
            };
            VersionRow {
                label,
                when: sync_conflicts::when(v.mtime),
                size: crate::ports::size_label(v.size),
                newest: differ && v.mtime == newest,
            }
        })
        .collect()
}

/// The line under the versions: what A does and where the other version goes.
pub fn keep_text(folder: &str) -> String {
    format!("A keeps the highlighted version in its place. The other one is not deleted: it moves to {folder}.")
}

/// For a memory card: the whole card is chosen, with every game's saves on it.
pub fn card_warning(kind: Kind) -> Option<&'static str> {
    (kind == Kind::Card).then_some("This is the whole memory card. The version you keep holds the saves of every game on it.")
}

/// The one-time notice on the shelf for groups that appeared since the last one. It says where to choose: the game's
/// card when one game's save is meant, Settings → Connections → Sync otherwise (several, a memory card, a save no game
/// of the library owns: those have no card to open).
pub fn notice(new: &[&Entry]) -> Option<String> {
    let first = new.first()?;
    let (card, title, more) = (first.group.kind == Kind::Card, &first.title, new.len() - 1);
    Some(match (card, more, matches!(first.owner, Owner::Game(_))) {
        (true, 0, true) => format!("A memory card has two versions: {title} – open the game's card to choose"),
        (true, 0, false) => format!("A memory card has two versions: {title} – choose in Settings › Connections › Sync"),
        (true, _, _) => format!("A memory card has two versions: {title} and {more} more – choose in Settings › Connections › Sync"),
        (false, 0, true) => format!("A save has two versions: {title} – open the game's card to choose"),
        (false, 0, false) => format!("A save has two versions: {title} – choose in Settings › Connections › Sync"),
        (false, _, _) => format!("A save has two versions: {title} and {more} more – choose in Settings › Connections › Sync"),
    })
}

/// The value of the row in Settings → Sync.
pub fn waiting_text(n: usize) -> String {
    match n {
        0 => "Nothing waiting".into(),
        n => format!("{n} to choose"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sync_conflicts::Version;
    use std::path::PathBuf;

    const CONFIRM: Button = Button::East;
    const CANCEL: Button = Button::South;

    fn entry(name: &str, kind: Kind, others: usize) -> Entry {
        let original = PathBuf::from(format!("/storage/roms/gba/{name}.srm"));
        let v = |path: PathBuf, mtime| Version { path, mtime, size: 32 * 1024 };
        Entry {
            group: Group {
                current: Some(v(original.clone(), 2_000_000_000)),
                others: (0..others).map(|i| v(PathBuf::from(format!("/storage/roms/gba/{name}.sync-conflict-2026102{i}-101010-ABCDEF7.srm")), 1_900_000_000)).collect(),
                original,
                kind,
                previews: Vec::new(),
            },
            owner: Owner::Nobody,
            title: name.to_string(),
        }
    }

    #[test]
    fn from_a_card_with_one_save_it_opens_on_the_versions_and_b_closes() {
        let mut p = ConflictPage::new(vec![entry("Game", Kind::Save, 1)], true);
        assert_eq!(p.view, View::Versions(0));
        assert_eq!(p.len(), 2);
        assert_eq!(p.press(Button::Down, CONFIRM, CANCEL), Action::Nothing);
        assert_eq!(p.press(CONFIRM, CONFIRM, CANCEL), Action::Keep { entry: 0, pick: 1 });
        assert!(p.busy);
        assert_eq!(p.press(CONFIRM, CONFIRM, CANCEL), Action::Nothing, "one choice at a time");
        assert_eq!(p.press(CANCEL, CONFIRM, CANCEL), Action::Nothing, "not left while it works");
        p.kept(0, Ok("Kept.".into()));
        // The outcome stays on screen until B.
        assert_eq!((p.view, p.entries.len(), p.message.clone()), (View::List, 0, Some(("Kept.".to_string(), false))));
        assert_eq!(p.press(CONFIRM, CONFIRM, CANCEL), Action::Nothing);
        assert_eq!(p.press(CANCEL, CONFIRM, CANCEL), Action::Close);
        let mut p = ConflictPage::new(vec![entry("Game", Kind::Save, 1)], true);
        assert_eq!(p.press(CANCEL, CONFIRM, CANCEL), Action::Close);
    }

    #[test]
    fn from_settings_the_list_comes_first_and_b_goes_back_to_it() {
        let mut p = ConflictPage::new(vec![entry("A", Kind::Save, 1), entry("B", Kind::Card, 1)], false);
        assert_eq!(p.view, View::List);
        p.press(Button::Down, CONFIRM, CANCEL);
        p.press(CONFIRM, CONFIRM, CANCEL);
        assert_eq!(p.view, View::Versions(1));
        assert_eq!(p.sel, 0, "on the version in use");
        assert!(card_warning(p.entry().unwrap().group.kind).is_some());
        p.press(CANCEL, CONFIRM, CANCEL);
        assert_eq!((p.view, p.sel), (View::List, 1));
        p.press(CONFIRM, CONFIRM, CANCEL);
        assert_eq!(p.press(CONFIRM, CONFIRM, CANCEL), Action::Keep { entry: 1, pick: 0 });
        p.kept(1, Ok("Kept.".into()));
        assert_eq!((p.view, p.entries.len(), p.sel), (View::List, 1, 0));
        assert_eq!(p.message, Some(("Kept.".to_string(), false)));
        assert_eq!(p.press(CANCEL, CONFIRM, CANCEL), Action::Close);
    }

    #[test]
    fn a_failed_choice_keeps_the_entry_and_says_why() {
        let mut p = ConflictPage::new(vec![entry("A", Kind::Save, 1)], true);
        p.press(CONFIRM, CONFIRM, CANCEL);
        p.kept(0, Err("That version is not there any more.".into()));
        assert_eq!(p.entries.len(), 1);
        assert_eq!(p.message.as_ref().map(|m| m.1), Some(true));
        assert!(!p.busy);
    }

    #[test]
    fn with_nothing_waiting_the_page_is_empty_and_presses_do_nothing_but_close() {
        let mut p = ConflictPage::new(Vec::new(), false);
        assert!(p.is_empty());
        assert_eq!(p.press(Button::Down, CONFIRM, CANCEL), Action::Nothing);
        assert_eq!(p.press(CONFIRM, CONFIRM, CANCEL), Action::Nothing);
        assert_eq!(p.press(CANCEL, CONFIRM, CANCEL), Action::Close);
        assert_eq!(waiting_text(0), "Nothing waiting");
        assert_eq!(waiting_text(2), "2 to choose");
        assert_eq!(notice(&[]), None);
    }

    #[test]
    fn a_new_search_keeps_the_open_entry_or_falls_back_to_the_list() {
        let mut p = ConflictPage::new(vec![entry("A", Kind::Save, 1), entry("B", Kind::Save, 1)], false);
        p.press(Button::Down, CONFIRM, CANCEL);
        p.press(CONFIRM, CONFIRM, CANCEL);
        p.update(vec![entry("B", Kind::Save, 1)]);
        assert_eq!(p.view, View::Versions(0));
        p.update(vec![entry("C", Kind::Save, 1)]);
        assert_eq!(p.view, View::List);
        p.update(Vec::new());
        assert_eq!((p.view, p.sel, p.len()), (View::List, 0, 0));
    }

    #[test]
    fn the_rows_name_each_version_with_its_time_and_size_and_mark_the_newest() {
        let e = entry("Game", Kind::Save, 1);
        let rows = version_rows(&e.group);
        assert_eq!(rows.iter().map(|r| r.label.as_str()).collect::<Vec<_>>(), ["In use now", "Other version"]);
        assert_eq!(rows[0].size, "32 KB");
        assert!(rows[0].newest && !rows[1].newest);
        assert!(rows[1].when.contains("2030") || rows[1].when.contains("2029"), "{}", rows[1].when);
        let three = version_rows(&entry("Game", Kind::Save, 2).group);
        assert_eq!(three.iter().map(|r| r.label.as_str()).collect::<Vec<_>>(), ["In use now", "Other version 1", "Other version 2"]);
        // The same time everywhere: none is marked.
        let mut same = entry("Game", Kind::Save, 1);
        same.group.others[0].mtime = 2_000_000_000;
        assert!(version_rows(&same.group).iter().all(|r| !r.newest));
    }

    #[test]
    fn the_notice_names_the_game_counts_the_rest_and_says_where_to_choose() {
        let (mut a, b, c) = (entry("Pokémon Emerald", Kind::Save, 1), entry("Mcd001.ps2", Kind::Card, 1), entry("Zelda", Kind::Save, 1));
        // One game's save has a card to open; a save nobody owns, a memory card and several have only Settings → Sync.
        assert_eq!(notice(&[&a]).as_deref(), Some("A save has two versions: Pokémon Emerald – choose in Settings › Connections › Sync"));
        a.owner = Owner::Game(PathBuf::from("/storage/roms/gba/Pokemon Emerald.gba"));
        assert_eq!(notice(&[&a]).as_deref(), Some("A save has two versions: Pokémon Emerald – open the game's card to choose"));
        assert_eq!(notice(&[&b]).as_deref(), Some("A memory card has two versions: Mcd001.ps2 – choose in Settings › Connections › Sync"));
        assert_eq!(notice(&[&a, &c, &b]).as_deref(), Some("A save has two versions: Pokémon Emerald and 2 more – choose in Settings › Connections › Sync"));
    }

    /// Every sentence of the notice has its German line.
    #[test]
    fn the_notice_is_german_too() {
        let (t, _) = crate::i18n::Table::parse(include_str!("../assets/i18n/de.tsv"));
        let mut a = entry("Pokémon Emerald", Kind::Save, 1);
        a.owner = Owner::Game(PathBuf::from("/storage/roms/gba/Pokemon Emerald.gba"));
        let b = entry("Mcd001.ps2", Kind::Card, 1);
        let one = notice(&[&a]).unwrap();
        assert_eq!(t.translate(&one).as_deref(), Some("Ein Spielspeicher hat zwei Versionen: Pokémon Emerald – wähle auf der Spielkarte"));
        for text in [notice(&[&b]).unwrap(), notice(&[&a, &b]).unwrap(), notice(&[&b, &a]).unwrap()] {
            let de = t.translate(&text).unwrap_or_else(|| panic!("no German for {text:?}"));
            assert!(de.contains("Einstellungen › Verbindungen › Sync"), "{de}");
        }
    }
}
