//! **Frozen games** (docs/user/playing.md "Frozen games"): the last games with a moment to go back to stand a little open
//! on the shelf, and A on one is back in that moment without a word in between. Nothing here runs or keeps a game: it is
//! the moment Quit or sleep already wrote (`states`, Settings > Remember where I quit), the recent games and the normal
//! start, which loads it as it always does. So there is no new way to load a state, and nothing new on the card but a
//! few words beside the game in `recents.json` ([`Mark`]).
//!
//! What may show as frozen is decided by plain functions over what a thread found ([`scan`]), so every rule is a test:
//! a game with no state, one that now runs on another emulator or core, and one whose file changed are normal cases. A
//! start of a game like that says why it starts fresh ([`at_launch`]) and doesn't load a state made for another file.

use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde::{Deserialize, Serialize};

/// The most games that stand open at a time.
pub const MOST: usize = 5;

/// pocket-core: don't load the auto state, keep it where Load finds it ([`at_launch`]).
pub const SKIP_RESUME_ENV: &str = "POCKET_SKIP_RESUME";
/// pocket-core: a line to show over the game's first moments.
pub const NOTE_ENV: &str = "POCKET_START_NOTE";

/// What pocket-core says when the core refuses the state.
pub const REFUSED: &str = "Couldn't go back to that moment: started fresh";
const GONE: &str = "Its saved moment is gone: started fresh";
const OTHER_EMULATOR: &str = "Started fresh: its saved moment belongs to its other emulator";
const CHANGED: &str = "The game's file changed: started fresh. The moment is still under Load";

/// What the moment of a recent game was written with, kept in its `recents.json` entry when a session ends with a new
/// state.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mark {
    pub emulator: String,
    pub core: String,
    pub rom_bytes: u64,
    pub rom_mtime: u64,
    /// What the file(s) hold, sampled ([`fingerprint`]); 0 in a mark from before it, which goes by size and time.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub rom_hash: u64,
    /// The auto state's file where its name can't be worked out without the game running (ARMSX2's serial and CRC).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto: Option<PathBuf>,
}

fn is_zero(n: &u64) -> bool {
    *n == 0
}

/// How much of a file is read to tell it from another one of the same size: its start, middle and end.
const SAMPLE: u64 = 4096;

/// A game's size and a hash of what it holds: for a disc sheet (`.cue`, `.m3u`, `.gdi`) the sheet and every file it names,
/// down to the tracks, so a swapped BIN or CHD counts as another game. The time isn't in it: a card copied on a PC, or a
/// patched file with the old time, is judged by what is in the file. None when something can't be read.
pub fn fingerprint(rom: &Path) -> Option<(u64, u64)> {
    let mut hash = 0xcbf29ce484222325u64;
    let mut bytes = 0u64;
    let mut seen = Vec::new();
    mix_file(rom, 0, &mut hash, &mut bytes, &mut seen)?;
    Some((bytes, hash | 1))
}

fn mix_file(path: &Path, depth: u32, hash: &mut u64, bytes: &mut u64, seen: &mut Vec<PathBuf>) -> Option<()> {
    use std::io::{Read, Seek, SeekFrom};
    if seen.iter().any(|p| p == path) {
        return Some(());
    }
    seen.push(path.to_path_buf());
    let mut file = std::fs::File::open(path).ok()?;
    let len = file.metadata().ok()?.len();
    let feed = |data: &[u8], hash: &mut u64| {
        for b in data {
            *hash = (*hash ^ *b as u64).wrapping_mul(0x100000001b3);
        }
    };
    feed(&len.to_le_bytes(), hash);
    *bytes += len;
    let kind = path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).unwrap_or_default();
    if matches!(kind.as_str(), "cue" | "m3u" | "gdi") && len <= 1 << 20 && depth < 3 {
        let mut text = Vec::new();
        file.read_to_end(&mut text).ok()?;
        feed(&text, hash);
        let text = String::from_utf8_lossy(&text).into_owned();
        let dir = path.parent().unwrap_or(Path::new("."));
        for name in crate::library::sheet_references(&kind, &text) {
            let name = name.strip_prefix("./").map(str::to_string).unwrap_or(name);
            // A file the sheet names that isn't there (a sheet that names nothing real) leaves it as it is.
            let target = dir.join(name.replace('\\', "/"));
            if target.is_file() {
                mix_file(&target, depth + 1, hash, bytes, seen)?;
            }
        }
        return Some(());
    }
    let mut buf = vec![0u8; SAMPLE as usize];
    for at in [0, (len / 2).saturating_sub(SAMPLE / 2), len.saturating_sub(SAMPLE)] {
        file.seek(SeekFrom::Start(at)).ok()?;
        let n = file.read(&mut buf).ok()?;
        feed(&buf[..n], hash);
    }
    Some(())
}

/// The file's size and time; None when it can't be read.
pub fn rom_stamp(rom: &Path) -> Option<(u64, u64)> {
    let m = std::fs::metadata(rom).ok()?;
    Some((m.len(), m.modified().ok()?.duration_since(UNIX_EPOCH).ok()?.as_secs()))
}

impl Mark {
    pub fn new(emulator: &str, core: &str, rom: &Path, auto: Option<PathBuf>) -> Option<Mark> {
        let (rom_bytes, rom_mtime) = rom_stamp(rom)?;
        let rom_hash = fingerprint(rom).map_or(0, |(_, h)| h);
        Some(Mark { emulator: emulator.into(), core: core.into(), rom_bytes, rom_mtime, rom_hash, auto })
    }

    fn same_runner(&self, emulator: &str, core: &str) -> bool {
        self.emulator == emulator && self.core == core
    }

    fn same_file(&self, rom: &Path) -> bool {
        if self.rom_hash == 0 {
            return rom_stamp(rom) == Some((self.rom_bytes, self.rom_mtime));
        }
        // rom_bytes of a sheet is its own size; the hash has the rest.
        rom_stamp(rom).is_some_and(|(len, _)| len == self.rom_bytes) && fingerprint(rom).is_some_and(|(_, h)| h == self.rom_hash)
    }
}

/// One recent game as the app knows it: what would start it now.
#[derive(Clone, Debug)]
pub struct Game {
    pub system: String,
    pub path: PathBuf,
    pub emulator: String,
    pub core: String,
    pub mark: Option<Mark>,
}

/// The files the next start of this game would resume from, the first that exists; empty for an emulator without a
/// moment to go back to (ports, ScummVM's, a game on an emulator without states).
pub fn resume_files(g: &Game) -> Vec<PathBuf> {
    let stem = g.path.file_stem().and_then(|s| s.to_str()).unwrap_or_default();
    if g.emulator == "pocket-core" {
        // Not the state a start already gave up on (`slots::retry_auto_path`): no start loads it by itself.
        let auto = crate::slots::auto_path(&crate::slots::base(&g.system, stem));
        let taken = crate::slots::taken_auto_path(&auto);
        return vec![auto, taken];
    }
    if g.emulator == "retroarch" {
        let mut name = crate::slots::base(&g.system, stem).into_os_string();
        name.push(".state.auto");
        return vec![name.into()];
    }
    let own = g.mark.as_ref().filter(|m| m.same_runner(&g.emulator, &g.core)).and_then(|m| m.auto.clone());
    match own {
        Some(file) => vec![file],
        None => crate::standalone::auto_states(&g.system, &g.emulator, &g.core, &g.path).unwrap_or_default(),
    }
}

/// Whether the game stands open: there is a moment, and it was made by what would start the game and for this file. A
/// game from before the marks has only its state to go by, as its normal start has.
pub fn is_frozen(g: &Game, files: &[PathBuf]) -> bool {
    files.iter().any(|f| f.exists()) && g.mark.as_ref().is_none_or(|m| m.same_runner(&g.emulator, &g.core) && m.same_file(&g.path))
}

/// The paths of the newest [`MOST`] games that stand open, of the recent games given newest first. On a thread: it looks
/// at the card, and for GameCube and 3DS reads the game file's header.
pub fn scan(games: &[Game]) -> Vec<PathBuf> {
    games.iter().filter(|g| is_frozen(g, &resume_files(g))).take(MOST).map(|g| g.path.clone()).collect()
}

/// What `launch` does differently for a game with a moment it can't go back to. Everything else is the normal start.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AtLaunch {
    Normal,
    /// The normal start; this is why it doesn't continue.
    Note(&'static str),
    /// A start without the state, which stays: this is why.
    Fresh(&'static str),
}

/// `shown`: the shelf showed the game as frozen. RetroArch's own start loads what it finds, so it stays as it is.
pub fn at_launch(g: &Game, shown: bool) -> AtLaunch {
    if g.emulator == "retroarch" {
        return AtLaunch::Normal;
    }
    at_launch_with(g, shown, resume_files(g).iter().any(|f| f.exists()))
}

/// [`at_launch`] for a game whose state is there or not (`has_state`).
fn at_launch_with(g: &Game, shown: bool, has_state: bool) -> AtLaunch {
    match (&g.mark, has_state) {
        // The state is another emulator's or core's, or of another file: the normal start would load it, so it is skipped
        // (and stays). A mark outlives sessions that wrote no state (`next_mark`), so this holds after a detour too.
        (Some(m), true) if !m.same_runner(&g.emulator, &g.core) => AtLaunch::Fresh(OTHER_EMULATOR),
        (Some(m), true) if !m.same_file(&g.path) => AtLaunch::Fresh(CHANGED),
        (_, false) if shown => AtLaunch::Note(GONE),
        _ => AtLaunch::Normal,
    }
}

/// How the marks of a finished session go: a new one when this session wrote the state (`state_written`), else the mark
/// of the state that is still there: a Quit that kept no moment changes nothing about it, whatever emulator or core
/// ran. So the state on the card is never taken for another runner's or another file's ([`at_launch`]).
pub fn next_mark(previous: Option<Mark>, state_written: bool, new: impl FnOnce() -> Option<Mark>) -> Option<Mark> {
    if state_written {
        new().or(previous)
    } else {
        previous
    }
}

/// Whether Frozen games is on this device: not where the host owns the emulators and their states (the Steam Deck).
pub fn available() -> bool {
    !crate::device::current().host_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;

    fn game(emulator: &str, core: &str, path: PathBuf, mark: Option<Mark>) -> Game {
        Game { system: "gba".into(), path, emulator: emulator.into(), core: core.into(), mark }
    }

    /// A game on pocket-core whose state is at `state` (the real paths are under /storage), judged by `is_frozen`.
    fn setup(tag: &str) -> (TestDir, PathBuf, PathBuf) {
        let d = TestDir::new(&format!("frozen-{tag}"));
        let rom = d.join("Game.gba");
        std::fs::write(&rom, b"rom").unwrap();
        let state = d.join("Game.pstate.auto");
        (d, rom, state)
    }

    #[test]
    fn no_state_is_a_normal_case_and_a_state_is_frozen() {
        let (_d, rom, state) = setup("state");
        let g = game("pocket-core", "mgba", rom.clone(), None);
        let files = vec![state.clone()];
        assert!(!is_frozen(&g, &files), "no state, nothing frozen");
        std::fs::write(&state, b"s").unwrap();
        assert!(is_frozen(&g, &files), "a game from before the marks goes by its state");
    }

    #[test]
    fn another_emulator_or_core_or_a_changed_file_is_a_normal_case() {
        let (_d, rom, state) = setup("mark");
        std::fs::write(&state, b"s").unwrap();
        let files = vec![state];
        let mark = Mark::new("pocket-core", "mgba", &rom, None);
        assert!(is_frozen(&game("pocket-core", "mgba", rom.clone(), mark.clone()), &files));
        assert!(!is_frozen(&game("pocket-core", "gpsp", rom.clone(), mark.clone()), &files), "another core");
        assert!(!is_frozen(&game("retroarch", "mgba", rom.clone(), mark.clone()), &files), "another emulator");
        std::fs::write(&rom, b"a longer rom").unwrap();
        assert!(!is_frozen(&game("pocket-core", "mgba", rom, mark), &files), "the file changed");
    }

    #[test]
    fn at_launch_says_why_and_only_skips_the_state_for_a_changed_file() {
        let d = TestDir::new("frozen-launch");
        // Dolphin's names need the game's header; ARMSX2's come with the mark, so the state can be anywhere.
        let rom = d.join("Game.iso");
        std::fs::write(&rom, b"rom").unwrap();
        let state = d.join("SLES (ABC).99.p2s");
        let g = |mark: Option<Mark>, emulator: &str| Game { system: "ps2".into(), path: rom.clone(), emulator: emulator.into(), core: "armsx2".into(), mark };
        let mark = Mark::new("armsx2", "armsx2", &rom, Some(state.clone()));
        assert_eq!(at_launch(&g(None, "armsx2"), false), AtLaunch::Normal, "never played: nothing to say");
        assert_eq!(at_launch(&g(mark.clone(), "armsx2"), true), AtLaunch::Note(GONE), "shown as frozen, the state went since");
        assert_eq!(at_launch(&g(mark.clone(), "armsx2"), false), AtLaunch::Normal, "a normal case with no state says nothing");
        std::fs::write(&state, b"s").unwrap();
        assert_eq!(at_launch(&g(mark.clone(), "armsx2"), true), AtLaunch::Normal, "the moment is there: the normal start loads it");
        assert_eq!(at_launch(&g(mark.clone(), "retroarch"), true), AtLaunch::Normal, "RetroArch loads by itself");
        std::fs::write(&rom, b"a longer rom").unwrap();
        assert_eq!(at_launch(&g(mark, "armsx2"), true), AtLaunch::Fresh(CHANGED), "a state of another file isn't loaded, and stays");
        assert!(state.exists());
    }

    #[test]
    fn a_state_of_another_core_or_file_is_never_loaded_by_pocket_core_and_stays() {
        let (_d, rom, state) = setup("fresh");
        std::fs::write(&state, b"s").unwrap();
        let mark = Mark::new("pocket-core", "mgba", &rom, None);
        let at = |emulator: &str, core: &str, mark: Option<Mark>| at_launch_with(&game(emulator, core, rom.clone(), mark), true, state.exists());
        assert_eq!(at("pocket-core", "mgba", mark.clone()), AtLaunch::Normal);
        assert_eq!(at("pocket-core", "gpsp", mark.clone()), AtLaunch::Fresh(OTHER_EMULATOR), "another core");
        assert_eq!(at("armsx2", "mgba", mark.clone()), AtLaunch::Fresh(OTHER_EMULATOR), "another emulator");
        // Detour: another runner played without writing a state, the mark stays; the file was replaced meanwhile.
        let kept = next_mark(mark, false, || None);
        std::fs::write(&rom, b"a replaced rom").unwrap();
        assert_eq!(at("pocket-core", "mgba", kept), AtLaunch::Fresh(CHANGED));
        assert!(state.exists(), "the state stays");
    }

    #[test]
    fn a_swapped_track_or_a_patched_file_counts_as_another_game_but_a_copied_card_does_not() {
        let d = TestDir::new("frozen-fingerprint");
        let (cue, m3u, bin, rom) = (d.join("Game.cue"), d.join("Game.m3u"), d.join("Game.bin"), d.join("Plain.gba"));
        std::fs::write(&cue, "FILE \"Game.bin\" BINARY\n  TRACK 01 MODE2/2352\n").unwrap();
        std::fs::write(&m3u, "Game.cue\n").unwrap();
        std::fs::write(&bin, vec![1u8; 20000]).unwrap();
        std::fs::write(&rom, vec![7u8; 20000]).unwrap();
        let (mc, mm, mr) = (Mark::new("pocket-core", "pcsx", &cue, None).unwrap(), Mark::new("pocket-core", "pcsx", &m3u, None).unwrap(), Mark::new("pocket-core", "mgba", &rom, None).unwrap());
        assert!(mc.same_file(&cue) && mm.same_file(&m3u) && mr.same_file(&rom));
        // Another version of the BIN, the same size: the cue and the playlist both changed.
        std::fs::write(&bin, vec![2u8; 20000]).unwrap();
        assert!(!mc.same_file(&cue), "cue: its bin changed");
        assert!(!mm.same_file(&m3u), "m3u: the bin of its cue changed");
        // A patched file of the same size, the time as it was: it is another file.
        let before = std::fs::metadata(&rom).unwrap().modified().unwrap();
        let mut patched = vec![7u8; 20000];
        patched[0] = 8;
        std::fs::write(&rom, patched).unwrap();
        std::fs::File::options().write(true).open(&rom).unwrap().set_modified(before).unwrap();
        assert!(!mr.same_file(&rom), "patched, same size and time");
        // The same bytes with a new time (the card was copied on a PC): the same game.
        std::fs::write(&rom, vec![7u8; 20000]).unwrap();
        std::fs::File::options().write(true).open(&rom).unwrap().set_modified(before + std::time::Duration::from_secs(3600)).unwrap();
        assert!(mr.same_file(&rom), "a new time alone changes nothing");
    }

    #[test]
    fn games_without_states_never_show_and_the_newest_five_do() {
        let d = TestDir::new("frozen-five");
        let rom = d.join("Port.sh");
        std::fs::write(&rom, b"#!/bin/sh").unwrap();
        // A port, the Xbox emulator without states, Wii U, PS3, Vita: not an emulator pocket-ui resumes.
        for (system, emulator) in [("ports", "port"), ("wiiu", "cemu"), ("ps3", "rpcs3"), ("psvita", "vita3k"), ("scummvm", "scummvm")] {
            let g = Game { system: system.into(), path: rom.clone(), emulator: emulator.into(), core: String::new(), mark: None };
            assert!(resume_files(&g).is_empty(), "{system}");
            assert!(scan(std::slice::from_ref(&g)).is_empty(), "{system}");
        }
        // ARMSX2 without a mark has no name for its state yet: a normal case until a Quit told it.
        let ps2 = Game { system: "ps2".into(), path: rom.clone(), emulator: "armsx2".into(), core: "armsx2".into(), mark: None };
        assert!(!is_frozen(&ps2, &resume_files(&ps2)));
        // Seven ARMSX2 games with states, the newest first: five stand open, in that order.
        let games: Vec<Game> = (0..7)
            .map(|i| {
                let path = d.join(format!("g{i}.iso"));
                std::fs::write(&path, b"x").unwrap();
                let auto = d.join(format!("g{i}.99.p2s"));
                std::fs::write(&auto, b"s").unwrap();
                let mark = Mark::new("armsx2", "armsx2", &path, Some(auto));
                Game { system: "ps2".into(), path, emulator: "armsx2".into(), core: "armsx2".into(), mark }
            })
            .collect();
        let found = scan(&games);
        assert_eq!(found, games[..MOST].iter().map(|g| g.path.clone()).collect::<Vec<_>>());
    }

    #[test]
    fn a_session_that_wrote_no_state_keeps_the_mark_of_the_one_there() {
        let m = |c: &str| Mark { emulator: "pocket-core".into(), core: c.into(), rom_bytes: 1, rom_mtime: 1, rom_hash: 0, auto: None };
        let next = |previous, written, new: Option<Mark>| next_mark(previous, written, || new);
        assert_eq!(next(Some(m("old")), false, Some(m("new"))), Some(m("old")));
        assert_eq!(next(Some(m("old")), true, Some(m("new"))), Some(m("new")));
        assert_eq!(next(Some(m("old")), true, None), Some(m("old")), "a file that can't be read now");
        assert_eq!(next(None, false, Some(m("new"))), None);
    }

    #[test]
    fn a_mark_survives_recents_json_and_an_old_list_has_none() {
        let d = TestDir::new("frozen-json");
        std::fs::write(d.join("recents.json"), r#"[{"system":"gba","name":"G","path":"/roms/g.gba","last_played":1,"play_secs":2}]"#).unwrap();
        let mut r = crate::recents::Recents::load(&d);
        assert_eq!(r.list[0].frozen, None, "a list from before Frozen games");
        r.list[0].frozen = Some(Mark { emulator: "pocket-core".into(), core: "mgba".into(), rom_bytes: 3, rom_mtime: 4, rom_hash: 0, auto: None });
        r.save();
        let again = crate::recents::Recents::load(&d);
        assert_eq!(again.list[0].frozen, r.list[0].frozen);
    }
}
