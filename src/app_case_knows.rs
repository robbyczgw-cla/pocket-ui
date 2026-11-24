//! Part of the pocket-ui binary: "the case knows where you are" (`pocket_ui::case_knows`, `case_knows_read`). The line
//! read from a game's own battery save stands on the back of its box and on its card ("Your game"), beside Continue on
//! the shelf (the meta line of a game whose moment waits) and on "Where was I?". It is asked for when one of those is
//! painted, read on its own thread, and painted once it is there: never a wait, and nothing where nothing is sure.

use std::cell::{OnceCell, RefCell};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use std::sync::Arc;

use pocket_ui::case_knows::{Progress, TeamArt};
use pocket_ui::case_knows_art::Art;
use pocket_ui::case_knows_read::{self, Reader};
use pocket_ui::cozy_extras::{self, Extra};

use super::{App, UserEvent};

/// What the app knows of the lines, and its question thread.
#[derive(Default)]
pub(crate) struct CaseKnows {
    reader: OnceCell<Reader>,
    /// Games asked about since their last answer was made stale (a game ended).
    asked: RefCell<HashSet<PathBuf>>,
    /// The answers: None for a game with nothing to show.
    known: HashMap<PathBuf, Option<Progress>>,
    /// Pictures from the player's own ROM for the answers that have them.
    art: HashMap<PathBuf, Arc<Art>>,
    /// `POCKETUI_SNAPSHOT_ART_ROM`: the snapshot's made-up team pictured from that ROM on this computer.
    pub(crate) made_up_art: Option<Arc<Art>>,
    /// `POCKETUI_SNAPSHOT_PROGRESS`: every game says this (snapshots only; the reading has its own tests).
    pub(crate) made_up: Option<Progress>,
    /// The game (system, ROM) whose "Where was I?" card is up without its line yet: an answer for it goes onto the card.
    pub(crate) card_waits: Option<(String, PathBuf)>,
}

impl CaseKnows {
    /// The thread has answered for `path` and has no line (its save says nothing pocket-ui reads): the place is free for
    /// the RetroAchievements line. Not before the answer, so a line from the save never replaces one shown first.
    fn says_nothing(&self, path: &Path) -> bool {
        match &self.made_up {
            Some(p) => !snapshot_game(p, path),
            None => matches!(self.known.get(path), Some(None)),
        }
    }

    pub(crate) fn has_progress(&self, path: &Path) -> bool {
        self.known.get(path).is_some_and(Option::is_some) || self.made_up.as_ref().is_some_and(|p| snapshot_game(p, path))
    }

    /// Saves came from another Pocket (Pocket Sync): every case asks again at its next look, as after a game's end. The
    /// thread reads a save again only when its size or time changed, so the others cost a look at their files.
    pub(crate) fn saves_came(&self) {
        self.asked.borrow_mut().clear();
    }

    /// The answer for `path` so far; the first look at it asks the thread (`start` makes it at the very first question).
    fn look(&self, system: &str, path: &Path, start: impl FnOnce() -> Reader) -> Option<&Progress> {
        if self.asked.borrow_mut().insert(path.to_path_buf()) {
            self.reader.get_or_init(start).request(system, path);
        }
        self.known.get(path)?.as_ref()
    }
}

impl App {
    /// Where the player is in `path`, once read; the first look asks the thread. With Cozy extras → Progress from saves
    /// off: nothing, and the thread is not even started (no save, no ROM read).
    pub(crate) fn case_progress(&self, system: &str, path: &Path) -> Option<&Progress> {
        if !cozy_extras::on(Extra::ProgressFromSaves) {
            return None;
        }
        if let Some(p) = &self.case_knows.made_up {
            return snapshot_game(p, path).then_some(p);
        }
        self.case_knows.look(system, path, || {
            let proxy = std::sync::Mutex::new(self.proxy.clone());
            Reader::start(self.data_dir.clone(), move |(_, rom), progress, art| {
                if let Ok(proxy) = proxy.lock() {
                    let _ = proxy.send_event(UserEvent::CaseKnows(rom, progress, art.map(Arc::new)));
                }
            })
        })
    }

    /// The line in the interface's language ("8 Orden · Glurak Lv. 45 · 31:12 Std."), or None.
    pub(crate) fn case_line(&self, system: &str, path: &Path) -> Option<String> {
        self.case_progress(system, path).map(case_knows_read::line)
    }

    /// The line for the case: from the game's own save first; when that has answered and says nothing, the player's
    /// RetroAchievements tally ("34 of 60 achievements · 56%", `app_achievements::achievements_line`). The second of the
    /// pair says which: true for the achievements line.
    pub(crate) fn case_or_achievements(&self, system: &str, path: &Path) -> Option<(String, bool)> {
        if let Some(line) = self.case_line(system, path) {
            return Some((line, false));
        }
        // A snapshot with only made-up achievements has no thread answer to wait for.
        let free = self.case_knows.says_nothing(path) || (self.case_knows.made_up.is_none() && self.achievements.made_up.is_some());
        free.then(|| self.achievements_line(system, path)).flatten().map(|l| (l, true))
    }

    /// [`App::case_or_achievements`]'s line alone.
    pub(crate) fn case_or_achievements_line(&self, system: &str, path: &Path) -> Option<String> {
        self.case_or_achievements(system, path).map(|(l, _)| l)
    }

    /// The pictures for `path` from the player's own ROM, once read (`case_knows_art`).
    pub(crate) fn case_art(&self, path: &Path) -> Option<Arc<Art>> {
        if !cozy_extras::on(Extra::ProgressFromSaves) {
            return None;
        }
        if let (Some(p), Some(a)) = (&self.case_knows.made_up, &self.case_knows.made_up_art) {
            return snapshot_game(p, path).then(|| a.clone());
        }
        self.case_knows.art.get(path).cloned()
    }

    /// The thread's answer: painted at the next frame when it changed.
    pub(crate) fn case_answer(&mut self, rom: PathBuf, progress: Option<Progress>, art: Option<Arc<Art>>) {
        let changed = self.case_knows.known.get(&rom) != Some(&progress) || self.case_knows.art.get(&rom) != art.as_ref();
        match art {
            Some(a) => self.case_knows.art.insert(rom.clone(), a),
            None => self.case_knows.art.remove(&rom),
        };
        // "Where was I?" came up before the answer was in: it takes the line now, while it is up.
        let waits = self.case_knows.card_waits.clone().filter(|(_, r)| r == &rom);
        if let (Some((system, _)), true) = (waits, cozy_extras::on(Extra::ProgressFromSaves)) {
            if let (Some(card), Some(p)) = (self.where_was_i.as_mut(), progress.as_ref()) {
                card.progress = Some(case_knows_read::line(p));
                card.art = self.case_knows.art.get(&rom).cloned();
                self.case_knows.card_waits = None;
            } else if progress.is_none() {
                // Nothing from the save: the RetroAchievements tally, when there is one (`case_or_achievements`).
                let line = self.achievements_line(&system, &rom);
                if let (Some(card), Some(line)) = (self.where_was_i.as_mut(), line) {
                    card.progress = Some(line);
                    card.art = None;
                }
                self.case_knows.card_waits = None;
            }
        }
        if progress.is_some() {
            self.played_files.insert(rom.clone());
            self.cases_look_changed();
            self.curator.touch();
        }
        self.case_knows.known.insert(rom, progress);
        if changed {
            self.request_redraw();
        }
    }

    /// Pocket Sync wrote saves from another Pocket here. Only a game's end used to make the cases look again, so on
    /// Android (the saves in the player's `saves/<key>/`, nothing beside the game) the open case of a game not played here
    /// showed the save that came only after the game had run once. Now the cases read their saves again and the shelf
    /// looks again at which games have a save ("Not here yet" instead of "Not played yet").
    pub(crate) fn case_saves_came(&mut self) {
        self.case_knows.saves_came();
        self.played_refresh();
        self.cases_look_changed();
        self.request_redraw();
    }

    /// A game ended: its save may have changed. The old line stays on the screen until the new answer replaces it.
    pub(crate) fn case_forget(&mut self, rom: &Path) {
        self.case_knows.asked.borrow_mut().remove(rom);
        // So may its achievements: read again, and asked fresh on Wi-Fi.
        self.achievements_forget(rom);
    }
}

/// Whether a snapshot's made-up `p` belongs to the game at `path`: Pokémon's to a Pokémon game, Super Metroid's to it,
/// and so on, so the other games of the snapshot say nothing, as they would.
fn snapshot_game(p: &Progress, path: &Path) -> bool {
    let name = path.to_string_lossy().to_lowercase();
    match p {
        Progress::Pokemon { .. } => name.contains("pokemon"),
        Progress::Items { .. } => name.contains("super metroid"),
        Progress::Exits { .. } => name.contains("super mario world") || name.contains("all stars & world"),
        Progress::Stars { .. } => name.contains("super mario 64"),
        Progress::Spirit { .. } => name.contains("ocarina of time"),
        Progress::StarSpirits { .. } => name.contains("paper mario"),
        Progress::GoldCups { .. } => name.contains("mario kart 64"),
        Progress::Balloons { .. } => name.contains("diddy kong racing"),
        Progress::Medals { .. } => name.contains("star fox 64") || name.contains("lylat wars"),
        Progress::Quest { .. } => name.contains("link to the past"),
        Progress::Hero { .. } => name.contains("final fantasy") || name.contains("chrono trigger") || name.contains("golden sun") || name.contains("lufia") || name.contains("mystery dungeon") || name.contains("tactics ogre") || name.contains("superstar saga") || name.contains("mother 3"),
        Progress::Party { .. } => name.contains("earthbound") || name.contains("mario rpg") || name.contains("breath of fire"),
        Progress::Instruments { .. } => name.contains("awakening"),
        Progress::Elements { .. } => name.contains("minish cap"),
        Progress::Essences { .. } => name.contains("oracle of"),
        Progress::Remains { .. } => name.contains("majora"),
        Progress::Chapter { .. } => name.contains("fire emblem"),
        Progress::Complete { .. } => name.contains("donkey kong country"),
        Progress::Shards { .. } => name.contains("amazing mirror"),
        Progress::Jiggies { .. } => name.contains("banjo"),
        Progress::Mission { .. } => name.contains("goldeneye"),
    }
}

/// `POCKETUI_SNAPSHOT_PROGRESS=<pokemon|pokemon-de|pokemon-ds|pokemon-bw|pokemon-start|metroid|mario|zelda|zelda-dark|…>`: the made-up line of
/// that game in a snapshot, in the language of the snapshot.
pub(crate) fn snapshot_progress(case: &str) -> Option<Progress> {
    Some(match case {
        // A team as Emerald numbers it: Charizard, Pikachu, Blaziken, Swampert, Gardevoir's line, Metagross's line.
        "pokemon" => Progress::Pokemon { badges: 6, lead: Some(("Charizard".into(), 45)), hours: 31, minutes: 12, art: TeamArt { team: vec![6, 25, 282, 285, 394, 400], badges: 0b0011_1111 } },
        "pokemon-de" => Progress::Pokemon { badges: 6, lead: Some(("Glurak".into(), 45)), hours: 31, minutes: 12, art: TeamArt { team: vec![6, 25, 282, 285, 394, 400], badges: 0b0011_1111 } },
        "pokemon-gold" => Progress::Pokemon { badges: 6, lead: Some(("Typhlosion".into(), 41)), hours: 27, minutes: 5, art: TeamArt { team: vec![157, 181, 25, 130, 214, 0xFD], badges: 0b0011_1111 } },
        "pokemon-gold-de" => Progress::Pokemon { badges: 6, lead: Some(("Tornupto".into(), 41)), hours: 27, minutes: 5, art: TeamArt { team: vec![157, 181, 25, 130, 214, 0xFD], badges: 0b0011_1111 } },
        // HeartGold (the DS games number species as the national dex, 494 the egg): Typhlosion, Ampharos, Pikachu,
        // Gyarados, Lucario, an egg; 8 Johto badges and 3 of Kanto's.
        "pokemon-ds" => Progress::Pokemon { badges: 11, lead: Some(("Typhlosion".into(), 52)), hours: 48, minutes: 20, art: TeamArt { team: vec![157, 181, 25, 130, 448, 494], badges: 0xFF } },
        "pokemon-ds-de" => Progress::Pokemon { badges: 11, lead: Some(("Tornupto".into(), 52)), hours: 48, minutes: 20, art: TeamArt { team: vec![157, 181, 25, 130, 448, 494], badges: 0xFF } },
        // Black: the line alone (no pictures read for Black and White).
        "pokemon-bw" => Progress::Pokemon { badges: 7, lead: Some(("Samurott".into(), 48)), hours: 39, minutes: 2, art: TeamArt::default() },
        "pokemon-start" => Progress::Pokemon { badges: 0, lead: Some(("Mudkip".into(), 7)), hours: 0, minutes: 48, art: TeamArt { team: vec![283], badges: 0 } },
        "metroid" => Progress::Items { percent: 62, hours: 4, minutes: 31 },
        "mario" => Progress::Exits { done: 43, of: 96 },
        "mario64" => Progress::Stars { done: 62, of: 120 },
        "ocarina" => Progress::Spirit { stones: 3, medallions: 2, hearts: 11 },
        "dkr" => Progress::Balloons { done: 34, of: 47 },
        "star-fox" => Progress::Medals { normal: 9, expert: 2 },
        "mario-kart" => Progress::GoldCups { done: 7, of: 16 },
        "paper-mario" => Progress::StarSpirits { rescued: 3, level: 9, hours: 12, minutes: 3 },
        "earthbound" => Progress::Party { lead: ("Ness".into(), 23), friends: 3 },
        "ff6" => Progress::Hero { lead: Some(("Terra".into(), 32)), hours: 18, minutes: 44 },
        "zelda" => Progress::Quest { pendants: 2, crystals: 0, hearts: 9, dark_world: false },
        "ladx" => Progress::Instruments { done: 5, hearts: 9 },
        "minish" => Progress::Elements { done: 3, hearts: 11 },
        "oracle" => Progress::Essences { done: 5, hearts: 10 },
        "majora" => Progress::Remains { done: 2, hearts: 9 },
        "fire-emblem" => Progress::Chapter { chapter: Some("Chapter 17".into()), lord: Some("Hector"), mode: Some("Hard mode"), hours: 14, minutes: 52 },
        "sacred-stones" => Progress::Chapter { chapter: Some("Chapter 5x".into()), lord: None, mode: Some("Normal mode"), hours: 3, minutes: 7 },
        "dkc" => Progress::Complete { percent: 62, of: 101, clock: None },
        "dkc2" => Progress::Complete { percent: 87, of: 102, clock: Some((9, 41)) },
        "chrono" => Progress::Hero { lead: Some(("Crono".into(), 24)), hours: 11, minutes: 38 },
        "golden-sun" => Progress::Hero { lead: Some(("Isaac".into(), 23)), hours: 12, minutes: 5 },
        "kirby" => Progress::Shards { done: 5 },
        "goldeneye" => Progress::Mission { mission: Some("Mission 6.iii: Streets".into()), difficulty: "Secret Agent" },
        "banjo" => Progress::Jiggies { done: 42, of: 100, notes: 512, hours: 7, minutes: 18 },
        "golden-sun-2" => Progress::Hero { lead: Some(("Felix".into(), 31)), hours: 21, minutes: 47 },
        "dkc3" => Progress::Complete { percent: 41, of: 105, clock: Some((6, 3)) },
        "mario-rpg" => Progress::Party { lead: ("Mario".into(), 17), friends: 0 },
        "lufia-2" => Progress::Hero { lead: Some(("Maxim".into(), 23)), hours: 12, minutes: 40 },
        "mystery-dungeon" => Progress::Hero { lead: Some(("Pikachu".into(), 23)), hours: 14, minutes: 8 },
        "tactics-ogre" => Progress::Hero { lead: Some(("Alphonse".into(), 14)), hours: 9, minutes: 27 },
        "breath-of-fire-2" => Progress::Party { lead: ("Ryu".into(), 19), friends: 0 },
        "superstar-saga" => Progress::Hero { lead: Some(("Mario".into(), 14)), hours: 8, minutes: 52 },
        "mother-3" => Progress::Hero { lead: Some(("Lucas".into(), 27)), hours: 16, minutes: 2 },
        "zelda-dark" => Progress::Quest { pendants: 3, crystals: 5, hearts: 15, dark_world: true },
        _ => return None,
    })
}

/// `POCKETUI_SNAPSHOT_ART_ROM=<rom>`: the made-up Pokémon progress pictured from that ROM (the developer's own copy of a
/// Pokémon game, never in the repository): its places found and the team and badges made, as the thread would.
pub(crate) fn snapshot_art(progress: Option<&Progress>, rom: &str) -> Option<Arc<Art>> {
    // A DS game (64 to 512 MiB): found part by part as the thread does; `POCKETUI_SNAPSHOT_DS_PALETTES=<file>` (the
    // developer's own 496 palette numbers, never in the repository) for a ROM whose code doesn't have the games' table.
    if rom.to_ascii_lowercase().ends_with(".nds") {
        let Some(Progress::Pokemon { art: team, .. }) = progress else { return None };
        use pocket_ui::case_knows::Kind;
        let path = Path::new(rom);
        let places = [Kind::HeartGoldSoulSilver, Kind::Platinum, Kind::DiamondPearl].into_iter().find_map(|k| pocket_ui::case_knows_art::find_nds(path, k)).or_else(|| {
            let numbers = std::fs::read(std::env::var("POCKETUI_SNAPSHOT_DS_PALETTES").ok()?).ok()?;
            [Kind::HeartGoldSoulSilver, Kind::Platinum, Kind::DiamondPearl].into_iter().find_map(|k| pocket_ui::case_knows_art::snapshot_nds_places(path, k, numbers.clone()))
        })?;
        return Some(Arc::new(pocket_ui::case_knows_art::nds_art(path, &places, team)));
    }
    let bytes = std::fs::read(rom).ok()?;
    // A Link to the Past: its pause menu's pendants or crystals (a SNES file may have a copier header).
    if let Some(quest @ Progress::Quest { .. }) = progress {
        let header = if bytes.len() % 1024 == 512 { 512 } else { 0 };
        let places = pocket_ui::case_knows_art::find_zelda(&bytes[header..])?;
        return pocket_ui::case_knows_art::zelda_art(Path::new(rom), places, header as u64, quest).map(Arc::new);
    }
    let Some(Progress::Pokemon { art: team, .. }) = progress else { return None };
    let release = pocket_ui::case_knows::release(crc32fast::hash(&bytes), None)?;
    if matches!(release.kind, pocket_ui::case_knows::Kind::GoldSilver | pocket_ui::case_knows::Kind::Crystal) {
        let places = pocket_ui::case_knows_art::find_gb(&bytes)?;
        return Some(Arc::new(pocket_ui::case_knows_art::gb_art(Path::new(rom), &places, team)));
    }
    let places = pocket_ui::case_knows_art::find(&bytes, release.kind)?;
    Some(Arc::new(pocket_ui::case_knows_art::art(Path::new(rom), &places, team)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Progress from saves off (Cozy extras): `App::case_progress` answers before it comes to the thread, so the thread
    /// is never made and no save or ROM is asked for. On, the first look makes it and asks, once per game.
    #[test]
    fn the_first_look_starts_the_thread_and_asks_once() {
        let k = CaseKnows::default();
        let starts = std::cell::Cell::new(0);
        let dir = tempfile::Builder::new().prefix("pocketui-case-knows-unit-").tempdir().unwrap();
        let start = || {
            starts.set(starts.get() + 1);
            Reader::start(dir.path().to_path_buf(), |_, _, _| {})
        };
        assert!(k.look("gba", Path::new("/roms/gba/Emerald.gba"), start).is_none(), "nothing read yet");
        assert!(k.look("gba", Path::new("/roms/gba/Emerald.gba"), || unreachable!("started once")).is_none());
        assert_eq!(starts.get(), 1);
        assert!(k.reader.get().is_some());
        assert_eq!(k.asked.borrow().len(), 1);
    }

    /// Found in the 0.6 video (20.11.2026): on Android the open case of a game never played there showed nothing of the
    /// save Pocket Sync had just brought from the RG35XX Plus; the line came only after the game had run once, because
    /// only a game's end made the case ask again. A save that comes from another Pocket is read at the next look.
    #[test]
    fn a_save_from_another_pocket_is_read_at_the_next_look_without_playing() {
        let k = CaseKnows::default();
        let dir = tempfile::Builder::new().prefix("pocketui-case-knows-sync-").tempdir().unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        let start = || {
            let tx = std::sync::Mutex::new(tx);
            Reader::start(dir.path().to_path_buf(), move |(_, rom), _, _| {
                let _ = tx.lock().unwrap().send(rom);
            })
        };
        let rom = Path::new("/roms/snes/Zelda - A Link to the Past.sfc");
        assert!(k.look("snes", rom, start).is_none());
        assert_eq!(rx.recv_timeout(std::time::Duration::from_secs(5)).as_deref(), Ok(rom), "the first look reads the save");
        // The case is looked at again and again while nothing changed: one read.
        k.look("snes", rom, || unreachable!("started once"));
        assert!(rx.recv_timeout(std::time::Duration::from_millis(300)).is_err(), "nothing changed, nothing read again");
        // Pocket Sync wrote the other Pocket's save: the next look reads it, no game started.
        k.saves_came();
        k.look("snes", rom, || unreachable!("started once"));
        assert_eq!(rx.recv_timeout(std::time::Duration::from_secs(5)).as_deref(), Ok(rom), "the save that came is read at the next look");
    }
}
