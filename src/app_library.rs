//! Part of the pocket-ui binary: the game list from the cache at start and the scan after the first frame
//! (`pocket_ui::library_cache`, docs/ARCHITECTURE.md "Start"). The first frame shows the list the last scan found;
//! once it is on screen, a thread reads the folders again and sends a list back only when it differs. That list is
//! swapped in the way Add games does it (`finish_setup`), at a moment when nothing on screen holds a game or system
//! by its place in the list (no game, no game card or console page, no menu, no search, no Curator page; the other
//! Settings pages count from the list as they open or paint, so a list read again after "Show again" is in them at
//! once, `settings_page_holds_games`), with the selection kept on the same game or system. Nothing of this runs on the UI thread but the comparison of the selection.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use pocket_ui::library::System;
use pocket_ui::library_cache::{self, Key, Rescan, Start};
use pocket_ui::shelf::Item;

use super::{App, Screen, SettingsPage, UserEvent};

/// What happens with the list after the first frame.
#[derive(Debug, Default)]
pub(crate) enum AfterStart {
    /// Nothing (the cache is off, or the list couldn't be read).
    #[default]
    Nothing,
    /// The list came from the cache: read the folders again.
    Rescan,
    /// The list was read from the folders: write the cache (key as it was before the read).
    Store(Key),
}

#[derive(Default)]
pub(crate) struct LibraryUi {
    pub after_start: AfterStart,
    /// The key of the cache the list on screen came from (a rescan that finds the same list writes nothing).
    pub key: Option<Key>,
    /// A rescan runs (on its thread).
    rescanning: Arc<AtomicBool>,
    /// A changed list that waits for a moment it can be swapped in (`library_apply_pending`).
    pending: Option<Vec<System>>,
}

impl LibraryUi {
    /// From what the start's library thread read.
    pub fn from_start(start: &Start, enabled: bool) -> LibraryUi {
        let (after_start, key) = match start {
            Start::Cached { key, .. } => (AfterStart::Rescan, Some(key.clone())),
            Start::Scanned { key: Some(k), .. } if enabled => (AfterStart::Store(k.clone()), None),
            Start::Scanned { .. } => (AfterStart::Nothing, None),
        };
        LibraryUi { after_start, key, ..Default::default() }
    }
}

/// Whether a Settings page keeps a game or a system by its place in the list (an index): then the list waits until it
/// is left. The pages count and list from `App::systems` when they open or paint, or keep games by path or name, so
/// none does but Curator's (its own pages also block, `Curator::page`). A new page is decided here: the `match` has
/// no `_`.
pub(crate) fn settings_page_holds_games(page: &SettingsPage) -> bool {
    match page {
        SettingsPage::Curator => true,
        SettingsPage::Groups(_)
        | SettingsPage::List(_)
        | SettingsPage::Store
        | SettingsPage::Starter
        | SettingsPage::Updates
        | SettingsPage::Battery(_)
        | SettingsPage::Manual
        | SettingsPage::About
        | SettingsPage::Sync
        | SettingsPage::Leds(_)
        | SettingsPage::NightLight(_)
        | SettingsPage::Tailscale
        | SettingsPage::Bluetooth
        | SettingsPage::Web
        | SettingsPage::PerfFan
        | SettingsPage::Achievements
        | SettingsPage::Tools
        | SettingsPage::PcStreaming
        | SettingsPage::Backup
        | SettingsPage::Storage
        | SettingsPage::Gamelist
        | SettingsPage::Screenshots
        | SettingsPage::SleepPower => false,
    }
}

/// The selected entry by what it is rather than where it is: found again in the new list.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Picked {
    System(String),
    Game(PathBuf),
    Other(Item),
}

/// `picked` in `items` of the new list `systems`.
pub(crate) fn find_picked(picked: &Picked, items: &[Item], systems: &[System]) -> Option<usize> {
    items.iter().position(|i| match (picked, i) {
        (Picked::System(name), Item::System(s)) => systems.get(*s).is_some_and(|s| &s.name == name),
        (Picked::Game(path), Item::Game(s, g)) => systems.get(*s).and_then(|s| s.games.get(*g)).is_some_and(|g| &g.path == path),
        (Picked::Other(a), b) => a == b,
        _ => false,
    })
}

/// `item` of the list `systems` as a `Picked`.
pub(crate) fn picked(item: Item, systems: &[System]) -> Picked {
    match item {
        Item::System(s) => systems.get(s).map_or(Picked::Other(item), |s| Picked::System(s.name.clone())),
        Item::Game(s, g) => systems.get(s).and_then(|s| s.games.get(g)).map_or(Picked::Other(item), |g| Picked::Game(g.path.clone())),
        other => Picked::Other(other),
    }
}

impl App {
    /// The Japanese (CJK) letters of the games' names, drawn ahead on a thread from the system's font
    /// (`Text::prewarm_names`): once the first frame is on screen and whenever the list changes
    /// (`find_library_changed`). A list of plain ASCII names starts nothing.
    pub(crate) fn library_prewarm_names(&mut self) {
        self.text.prewarm_names(self.systems.iter().flat_map(|s| s.games.iter().map(|g| g.name.as_str())));
    }

    /// The first frame is on screen (or the start took too long for it): the scan or the cache write the start put
    /// off. Once.
    pub(crate) fn library_after_first_frame(&mut self) {
        // The player's EmulationStation gamelists, on a thread of their own (`app_gamelist`).
        self.gamelist_refresh("after the start");
        // And the screenshots folder, for the card's row and Settings (`app_gallery`).
        self.gallery_refresh("after the start");
        match std::mem::take(&mut self.library_ui.after_start) {
            AfterStart::Nothing => {}
            // A game started at once (On start, a resume after power off): its start has the card to itself; the
            // folders are read when it has ended (`library_apply_pending`).
            AfterStart::Rescan if self.running.is_some() => self.library_ui.after_start = AfterStart::Rescan,
            AfterStart::Rescan => self.library_rescan("after the start"),
            AfterStart::Store(key) => {
                let (data_dir, systems) = (self.data_dir.clone(), self.systems.clone());
                self.library_ui.key = Some(key.clone());
                let _ = std::thread::Builder::new().name("library-cache".into()).spawn(move || {
                    let t0 = Instant::now();
                    match library_cache::write(&data_dir, &key, &systems) {
                        Ok(()) => println!("library: cache written in {:?}", t0.elapsed()),
                        Err(e) => eprintln!("library: cache not written: {e}"),
                    }
                });
            }
        }
    }

    /// Reads the folders again on a thread; a different list comes back as `UserEvent::LibraryRescanned`. One at a
    /// time; nothing while the cache is off.
    pub(crate) fn library_rescan(&mut self, why: &'static str) {
        if !library_cache::enabled() || self.library_ui.rescanning.swap(true, Ordering::SeqCst) {
            return;
        }
        // A gamelist may have come with the games that were added.
        self.gamelist_refresh(why);
        let busy = self.library_ui.rescanning.clone();
        let (proxy, data_dir, es_systems, key) = (self.proxy.clone(), self.data_dir.clone(), self.es_systems.clone(), self.library_ui.key.clone());
        // What is on screen, or about to be (a list still waiting to be swapped in).
        let shown = self.library_ui.pending.clone().unwrap_or_else(|| self.systems.clone());
        let spawned = std::thread::Builder::new().name("library-scan".into()).spawn(move || {
            let t0 = Instant::now();
            let result = library_cache::rescan(&data_dir, &es_systems, &shown, key.as_ref());
            let took = t0.elapsed();
            match &result {
                Rescan::Same => println!("library: read again ({why}) in {took:?}, nothing changed"),
                Rescan::Changed { systems: fresh, .. } => println!("library: read again ({why}) in {took:?}: {}", library_cache::difference(&shown, fresh)),
                Rescan::Failed(e) => eprintln!("library: reading again ({why}) failed after {took:?}: {e}"),
            }
            busy.store(false, Ordering::SeqCst);
            if let Rescan::Changed { systems, key } = result {
                let _ = proxy.send_event(UserEvent::LibraryRescanned(systems, key));
            }
        });
        if spawned.is_err() {
            self.library_ui.rescanning.store(false, Ordering::SeqCst);
        }
    }

    /// A list that waits to be swapped in (`library_rescanned`), if any.
    pub(crate) fn library_pending(&self) -> Option<&Vec<System>> {
        self.library_ui.pending.as_ref()
    }

    /// A rescan found another list: it goes in now or at the next moment it can (`library_apply_pending`).
    pub(crate) fn library_rescanned(&mut self, fresh: Vec<System>, key: Option<Key>) {
        self.library_ui.key = key;
        self.library_ui.pending = Some(fresh);
        self.library_apply_pending();
    }

    /// Whether the list may be swapped now: nothing on screen keeps a game or a system by its place in the list. A
    /// Settings page does not (`settings_page_holds_games`): a game shown again in Storage is in Games' counts and
    /// From EmulationStation's offers at once, not when Settings is left.
    fn library_swap_allowed(&self) -> bool {
        self.running.is_none()
            && self.card.is_none()
            && self.console.is_none()
            && self.curator.page.is_none()
            && self.port_files.is_none()
            && !self.emu_choice_on_shelf()
            && self.leave.is_none()
            && self.menu.is_none()
            && self.switcher.is_none()
            && self.setup.is_none()
            && !self.settings_page.as_ref().is_some_and(settings_page_holds_games)
            && !self.find.is_open()
            && self.cabinet.launch_due().is_none()
    }

    /// Swaps a waiting list in when it may (called after each event, `about_to_wait`). Cheap when nothing waits.
    pub(crate) fn library_apply_pending(&mut self) {
        if matches!(self.library_ui.after_start, AfterStart::Rescan) && self.after_first_done && self.running.is_none() {
            self.library_ui.after_start = AfterStart::Nothing;
            self.library_rescan("after the game the start began");
        }
        if self.library_ui.pending.is_none() || !self.library_swap_allowed() {
            return;
        }
        let Some(fresh) = self.library_ui.pending.take() else { return };
        let was = self.items.get(self.sel).map(|i| picked(*i, &self.systems));
        let home_item = self.home_item.map(|i| picked(i, &self.systems));
        let screen_system = match self.screen {
            Screen::Games(s) => Some(self.systems.get(s).map(|s| s.name.clone())),
            _ => None,
        };
        self.systems = fresh;
        if let Some(name) = screen_system {
            match name.and_then(|n| self.systems.iter().position(|s| s.name == n)) {
                Some(s) => self.screen = Screen::Games(s),
                // The system has no games any more: back to Home, on its place if it is still there.
                None => self.screen = Screen::Home,
            }
        }
        self.home_item = home_item.and_then(|p| match p {
            Picked::System(name) => self.systems.iter().position(|s| s.name == name).map(Item::System),
            Picked::Other(i) => Some(i),
            Picked::Game(_) => None,
        });
        self.recents.follow_library(&self.systems);
        self.curator.library_changed();
        self.find_library_changed();
        // A Settings page open under the swap counts from the list: what it worked out is made again (the offers of
        // From EmulationStation for a game shown again in Storage).
        self.gamelist_library_changed();
        self.rebuild();
        if let Some(at) = was.and_then(|p| find_picked(&p, &self.items, &self.systems)) {
            self.sel = at;
            self.prev_sel = at;
        }
        println!("library: the list read again is on the shelf");
        self.request_redraw();
        // New games get their box art the way Add games does.
        self.auto_covers("library read again");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pocket_ui::library::Game;

    fn sys(name: &str, games: &[&str]) -> System {
        System {
            name: name.into(),
            fullname: name.into(),
            emulator: None,
            core: None,
            emulators: vec![],
            games: games.iter().map(|g| Game { name: g.to_string(), path: format!("/roms/{name}/{g}").into() }).collect(),
        }
    }

    #[test]
    fn the_selection_stays_on_the_same_game_or_system_when_the_list_moves() {
        let old = vec![sys("gb", &["a", "b"]), sys("gba", &["c", "d", "e"])];
        // A system before gba appeared, and a game before d.
        let new = vec![sys("gb", &["a", "b"]), sys("gbc", &["x"]), sys("gba", &["c", "cc", "d", "e"])];
        let home_new = [Item::Search, Item::Recent(0), Item::System(0), Item::System(1), Item::System(2)];
        assert_eq!(find_picked(&picked(Item::System(1), &old), &home_new, &new), Some(4), "gba moved one down");
        assert_eq!(find_picked(&picked(Item::Recent(0), &old), &home_new, &new), Some(1));
        let games_new: Vec<Item> = (0..4).map(|g| Item::Game(2, g)).collect();
        assert_eq!(find_picked(&picked(Item::Game(1, 1), &old), &games_new, &new), Some(2), "d is third now");
        // A game that was deleted is not found: the selection stays where it was (clamped by `rebuild`).
        let gone = vec![sys("gb", &["a", "b"]), sys("gba", &["c", "e"])];
        let games_gone: Vec<Item> = (0..2).map(|g| Item::Game(1, g)).collect();
        assert_eq!(find_picked(&picked(Item::Game(1, 1), &old), &games_gone, &gone), None);
    }

    #[test]
    fn only_a_cached_start_rescans_and_only_a_scanned_one_writes() {
        let key = Key { version: 1, app: "x".into(), es_systems: "/e".into(), mtime: 1, mtime_ns: 2, len: 3 };
        let cached = LibraryUi::from_start(&Start::Cached { systems: vec![], key: key.clone() }, true);
        assert!(matches!(cached.after_start, AfterStart::Rescan));
        assert_eq!(cached.key.as_ref(), Some(&key), "a rescan that finds the same list writes nothing");
        assert!(matches!(LibraryUi::from_start(&Start::Scanned { systems: vec![], key: Some(key.clone()) }, true).after_start, AfterStart::Store(_)));
        assert!(matches!(LibraryUi::from_start(&Start::Scanned { systems: vec![], key: Some(key) }, false).after_start, AfterStart::Nothing), "cache off");
        assert!(matches!(LibraryUi::from_start(&Start::Scanned { systems: vec![], key: None }, true).after_start, AfterStart::Nothing));
    }

    /// VDEV-ROUND-2026-10-25 #12: a game shown again in Storage was missing from Games' counts and From
    /// EmulationStation's offers until Settings was left, because any Settings page held the new list back.
    #[test]
    fn a_settings_page_does_not_hold_the_new_list_back_but_curators_does() {
        for page in [SettingsPage::Groups(0), SettingsPage::List(0), SettingsPage::Storage, SettingsPage::Gamelist, SettingsPage::Screenshots, SettingsPage::Starter, SettingsPage::About] {
            assert!(!settings_page_holds_games(&page));
        }
        assert!(settings_page_holds_games(&SettingsPage::Curator));
    }
}
