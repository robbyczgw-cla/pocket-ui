//! **Frozen games** (docs/user/playing.md "Frozen games"): the last games with a moment to go back to stand a little open
//! on the shelf, and A on one is back in that moment without a word in between. Nothing here runs or keeps a game: it is
//! the moment Quit or sleep already wrote (`states`, Settings > Continue where I quit), the recent games and the normal
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
pub const GONE: &str = "Its saved moment is gone: started fresh";
pub const OTHER_EMULATOR: &str = "Started fresh: its saved moment belongs to its other emulator";
pub const CHANGED: &str = "The game's file changed: started fresh. The moment is still under Load";
/// A start of a game that shares its save states with another of its file name ([`shared_moment_is_own`]).
pub const SHARED: &str = "Another game has this file name and shares its save states. Saving here can overwrite its saves. Rename one to keep them apart";

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
    /// The soft patches that were in effect ([`patch_stack`], 0 for none). None in a mark from before it: not known, so not
    /// held against the moment.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub patches: Option<u64>,
    /// The auto state's file where its name can't be worked out without the game running (ARMSX2's serial and CRC).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto: Option<PathBuf>,
}

fn is_zero(n: &u64) -> bool {
    *n == 0
}

/// How much of a file is read to tell it from another one of the same size: its start, middle and end.
const SAMPLE: u64 = 4096;
/// Up to this size Android reads the whole file instead: a patch in the middle would look the same to samples.
/// Larger fallback copies and direct document aliases carry the whole-file CRC in small `.stage` metadata
/// (`GameCopy.COPIED_EVERY_START` and `android_content`, change the limits together).
const WHOLE_UP_TO: u64 = if cfg!(target_os = "android") { 64 << 20 } else { 0 };

/// A game's size and a hash of what it holds: for a disc sheet (`.cue`, `.m3u`, `.gdi`) the sheet and every file it names,
/// down to the tracks, so a swapped BIN or CHD counts as another game. The time isn't in it: a card copied on a PC, or a
/// patched file with the old time, is judged by what is in the file. None when something can't be read.
pub fn fingerprint(rom: &Path) -> Option<(u64, u64)> {
    #[cfg(test)]
    if PHONE_FINGERPRINTS.with(|phone| phone.get()) { return fingerprint_up_to(rom, 64 << 20); }
    fingerprint_up_to(rom, WHOLE_UP_TO)
}

#[cfg(test)]
thread_local! { static PHONE_FINGERPRINTS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) }; }
/// Exercise Android's file identity policy in host regressions without changing other tests' threads.
#[cfg(test)]
pub(crate) fn on_phone<T>(test: impl FnOnce() -> T) -> T {
    struct Restore(bool);
    impl Drop for Restore { fn drop(&mut self) { PHONE_FINGERPRINTS.with(|phone| phone.set(self.0)); } }
    let _restore = Restore(PHONE_FINGERPRINTS.with(|phone| phone.replace(true)));
    test()
}

fn fingerprint_up_to(rom: &Path, whole_up_to: u64) -> Option<(u64, u64)> {
    let mut hash = 0xcbf29ce484222325u64;
    let mut bytes = 0u64;
    let mut seen = Vec::new();
    mix_file(rom, 0, whole_up_to, &mut hash, &mut bytes, &mut seen)?;
    Some((bytes, hash | 1))
}

fn mix_file(path: &Path, depth: u32, whole_up_to: u64, hash: &mut u64, bytes: &mut u64, seen: &mut Vec<PathBuf>) -> Option<()> {
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
                mix_file(&target, depth + 1, whole_up_to, hash, bytes, seen)?;
            }
        }
        return Some(());
    }
    let mut buf = vec![0u8; SAMPLE as usize];
    if len <= whole_up_to {
        loop {
            let n = file.read(&mut buf).ok()?;
            if n == 0 {
                return Some(());
            }
            feed(&buf[..n], hash);
        }
    }
    // A phone's big copy (`GameCopy`, whole_up_to is only set there): what the whole file came to when it was copied, so
    // that a patch between the samples is still another game. Without it (a copy nothing could vouch for) the samples say.
    if whole_up_to > 0 {
        // Its first line: a set of files (a cue and its tracks) has one line per file after it.
        if let Some(line) = std::fs::read_to_string(path.with_file_name(".stage")).ok().as_deref().and_then(|s| s.trim().lines().next()?.split(' ').nth(2).map(str::to_owned)) {
            feed(line.as_bytes(), hash);
        }
    }
    for at in [0, (len / 2).saturating_sub(SAMPLE / 2), len.saturating_sub(SAMPLE)] {
        file.seek(SeekFrom::Start(at)).ok()?;
        let n = file.read(&mut buf).ok()?;
        feed(&buf[..n], hash);
    }
    Some(())
}

/// What says which soft patches a start would apply now (`softpatch::find`): each one's name, size and time, in their order.
/// Another patch makes another game, whose state the old one isn't. 0 with no patch. Nothing is read, a look at each file is
/// all a launch pays; a patch copied back with a new time counts as another one, and the game starts fresh once.
pub fn patch_stack(rom: &Path) -> u64 {
    let patches = crate::softpatch::find(rom);
    if patches.is_empty() {
        return 0;
    }
    let mut hash = 0xcbf29ce484222325u64;
    let mut feed = |data: &[u8]| data.iter().for_each(|b| hash = (hash ^ u64::from(*b)).wrapping_mul(0x100000001b3));
    for patch in &patches {
        feed(patch.file_name().map_or(&[][..], |n| n.as_encoded_bytes()));
        let (bytes, time) = rom_stamp(patch).unwrap_or((u64::MAX, u64::MAX));
        feed(&bytes.to_le_bytes());
        feed(&time.to_le_bytes());
    }
    hash | 1
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
        Some(Mark { emulator: emulator.into(), core: core.into(), rom_bytes, rom_mtime, rom_hash, patches: Some(patch_stack(rom)), auto })
    }

    fn same_runner(&self, emulator: &str, core: &str) -> bool {
        self.emulator == emulator && self.core == core
    }

    fn same_file(&self, rom: &Path) -> bool {
        if self.patches.is_some_and(|recorded| recorded != patch_stack(rom)) {
            return false;
        }
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
/// moment to go back to (ports, ScummVM's, a game on an emulator without states). On a phone: `frozen_phone::files`.
#[cfg(not(target_os = "android"))]
pub fn resume_files(g: &Game) -> Vec<PathBuf> {
    let stem = g.path.file_stem().and_then(|s| s.to_str()).unwrap_or_default();
    if g.emulator == "pocket-core" {
        // Not the state a start already gave up on (`slots::retry_auto_path`): no start loads it by itself.
        let auto = crate::slots::auto_path(&crate::slots::base(&g.system, stem));
        return crate::slots::auto_candidates(&auto, false);
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
#[cfg(not(target_os = "android"))]
pub fn scan(games: &[Game]) -> Vec<PathBuf> {
    scan_timed(games).into_iter().map(|m| m.0).collect()
}

/// A game that stands open and when its moment was written (Unix seconds; None when the file's time can't be read).
pub type Moment = (PathBuf, Option<u64>);

/// [`is_frozen`] and, for a game that stands open, the time of the file a start would load: the first that exists, as
/// `resume_files` lists them (the one the core takes, not the newest of them). None for a game that stands closed.
pub fn open_moment(g: &Game, files: &[PathBuf]) -> Option<Moment> {
    if !is_frozen(g, files) {
        return None;
    }
    let at = files.iter().find(|f| f.exists()).and_then(|f| std::fs::metadata(f).and_then(|m| m.modified()).ok()).and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_secs());
    Some((g.path.clone(), at))
}

/// What the shelf's reads last found, for the card's "Continue" line: the open case's read (normal looks) and Easy's and the
/// big shelf's (that look). Only the active look's read counts, and none while a newer one is out, so the card never shows
/// an older state as current. It holds no files and does no work: the card asks it each time it is painted.
#[derive(Default)]
pub struct MomentReads {
    shelf: Vec<Moment>,
    shelf_out: bool,
    big: Vec<Moment>,
    big_out: bool,
}

impl MomentReads {
    /// A new read of the open case's games starts (or it won't, `false`: nothing is open then).
    pub fn shelf_started(&mut self, runs: bool) {
        self.shelf_out = runs;
        if !runs {
            self.shelf.clear();
        }
    }
    pub fn shelf_got(&mut self, list: Vec<Moment>) {
        self.shelf = list;
        self.shelf_out = false;
    }
    pub fn big_started(&mut self) {
        self.big_out = true;
    }
    pub fn big_got(&mut self, list: Vec<Moment>) {
        self.big = list;
        self.big_out = false;
    }
    /// When the moment a start of `path` would load was written, in the read of the look that is on; None while that look's
    /// read is out, and for a game it doesn't list.
    pub fn time(&self, big_look: bool, path: &Path) -> Option<u64> {
        let (list, out) = if big_look { (&self.big, self.big_out) } else { (&self.shelf, self.shelf_out) };
        if out {
            return None;
        }
        list.iter().find(|m| m.0 == path).and_then(|m| m.1)
    }
}

/// [`scan`] with each moment's time: what the card's "Continue" line takes from the shelf's own read, so it adds no file access.
#[cfg(not(target_os = "android"))]
pub fn scan_timed(games: &[Game]) -> Vec<Moment> {
    games.iter().filter_map(|g| open_moment(g, &resume_files(g))).take(MOST).collect()
}

/// [`moments`] with each moment's time (Easy's and the big shelf's read).
#[cfg(not(target_os = "android"))]
pub fn moments_timed(games: &[Game]) -> Vec<Moment> {
    games.iter().filter_map(|g| open_moment(g, &resume_files(g))).collect()
}

/// Every game of `games` the next start would resume, without [`MOST`]'s limit: what Easy's word and open case say.
#[cfg(not(target_os = "android"))]
pub fn moments(games: &[Game]) -> Vec<PathBuf> {
    moments_timed(games).into_iter().map(|m| m.0).collect()
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
#[cfg(not(target_os = "android"))]
pub fn at_launch(g: &Game, shown: bool) -> AtLaunch {
    if g.emulator == "retroarch" {
        return AtLaunch::Normal;
    }
    at_launch_with(g, shown, resume_files(g).iter().any(|f| f.exists()))
}

/// [`at_launch`] for a game whose state is there or not (`has_state`).
pub fn at_launch_with(g: &Game, shown: bool, has_state: bool) -> AtLaunch {
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

/// States are named after the file name, so two games of one system with one file name in two folders share them. Whether
/// the moment on the card is this game's: its session wrote one (a mark), and no session of the other came after it. `own`:
/// this game's last session and whether it has a mark, if it is among the recent games; `others`: the last session of each other
/// game of that name, None for one that isn't among them: it may have written since (taken off the list), so nothing is proven.
pub fn shared_moment_is_own(own: Option<(u64, bool)>, others: impl IntoIterator<Item = Option<u64>>) -> bool {
    own.is_some_and(|(last, marked)| marked && others.into_iter().all(|other| other.is_some_and(|other| last > other)))
}

/// The other installed games with the file name of `path`: those of the shelf, and hidden ones (`hidden`, with no system of
/// their own to go by) that are still on the card and in this system (`in_system`). A hidden game keeps sharing its states.
pub fn twins<'a>(path: &Path, shown: impl Iterator<Item = &'a Path>, hidden: impl Iterator<Item = &'a Path>, in_system: impl Fn(&Path) -> bool) -> Vec<PathBuf> {
    let stem = path.file_stem();
    let same = |p: &&Path| *p != path && p.file_stem() == stem;
    let mut out: Vec<PathBuf> = shown.filter(same).map(Path::to_path_buf).collect();
    for p in hidden.filter(same) {
        if !out.iter().any(|o| o == p) && p.exists() && in_system(p) {
            out.push(p.to_path_buf());
        }
    }
    out
}

/// Frozen games are available on every supported Linux handheld.
#[cfg(not(target_os = "android"))]
pub fn available() -> bool {
    true
}


#[cfg(test)]
mod tests {
    use super::*;


    /// The sixth game with a moment: it stands open for Easy, the others' limit of five stays theirs.
    #[test]
    fn every_usable_moment_counts_past_the_five_that_stand_open() {
        let d = tempfile::tempdir().unwrap();
        let games: Vec<Game> = (0..7)
            .map(|i| {
                let (rom, auto) = (d.path().join(format!("{i}.iso")), d.path().join(format!("{i}.state")));
                std::fs::write(&rom, b"game").unwrap();
                // The last one has no state on the card.
                if i < 6 {
                    std::fs::write(&auto, b"state").unwrap();
                }
                let mark = Mark::new("emu", "core", &rom, Some(auto));
                Game { system: "ps2".into(), path: rom, emulator: "emu".into(), core: "core".into(), mark }
            })
            .collect();
        assert_eq!(scan(&games).len(), MOST);
        assert_eq!(moments(&games).len(), 6);
        assert!(!moments(&games).contains(&games[6].path));
        // The card takes its line from those reads: the sixth moment is not in the shelf's five, but in Easy's read; the
        // seventh game (no state) is in neither. The big shelf offers a moment without a recent game too.
        let (shelf, all) = (scan_timed(&games), moments_timed(&games));
        let mut reads = MomentReads::default();
        reads.shelf_got(shelf);
        reads.big_got(all);
        assert!(reads.time(false, &games[5].path).is_none(), "not among the shelf's five");
        assert!(reads.time(true, &games[5].path).is_some(), "the sixth moment, from the big shelf's read");
        assert!(reads.time(true, &games[6].path).is_none() && reads.time(false, &games[6].path).is_none());
    }

    use crate::testutil::TestDir;

    /// An open card follows the shelf's reads: a read that is out shows nothing instead of the old time, and each look has its own.
    #[test]
    fn the_cards_line_follows_late_answers_and_the_looks_own_read() {
        let p = PathBuf::from("/roms/Game.gba");
        let mut r = MomentReads::default();
        r.shelf_got(vec![(p.clone(), Some(100))]);
        assert_eq!(r.time(false, &p), Some(100));
        // After a Quit: a new read starts, the card is open; the old time is no longer shown.
        r.shelf_started(true);
        assert_eq!(r.time(false, &p), None, "a read is out: no old state as current");
        r.shelf_got(vec![(p.clone(), Some(200))]);
        assert_eq!(r.time(false, &p), Some(200), "the answer reaches the open card");
        // A new moment appearing, and one going away (Continue where I quit off, after a Load).
        r.shelf_started(true);
        r.shelf_got(Vec::new());
        assert_eq!(r.time(false, &p), None, "no moment any more: no line");
        // Easy/big: its own read decides, whatever the open case's read still holds.
        r.shelf_got(vec![(p.clone(), Some(100))]);
        r.big_got(Vec::new());
        assert_eq!(r.time(true, &p), None, "the big shelf's new state, not the old shelf entry");
        assert_eq!(r.time(false, &p), Some(100));
        r.big_started();
        r.big_got(vec![(p.clone(), Some(300))]);
        assert_eq!(r.time(true, &p), Some(300));
        r.big_started();
        assert_eq!(r.time(true, &p), None, "big read out");
        assert_eq!(r.time(false, &p), Some(100), "the other look is not held up by it");
    }

    #[test]
    fn the_cards_continue_time_is_of_the_file_a_start_loads_and_only_for_a_waiting_moment() {
        let (_d, rom, state) = setup("moment-time");
        let taken = _d.join("Game.pstate.taken");
        let g = game("pocket-core", "mgba", rom.clone(), None);
        let files = vec![state.clone(), taken.clone()];
        assert_eq!(open_moment(&g, &files), None, "no moment, no line");
        let stamp = |f: &Path, ago: u64| {
            let t = std::time::SystemTime::now() - std::time::Duration::from_secs(ago);
            std::fs::File::create(f).unwrap().set_modified(t).unwrap();
            t.duration_since(UNIX_EPOCH).unwrap().as_secs()
        };
        // Both there (a restore, a sync): the start takes the first, so its time is the one shown, though the other is newer.
        let (first, _) = (stamp(&state, 7200), stamp(&taken, 60));
        assert_eq!(open_moment(&g, &files), Some((rom.clone(), Some(first))));
        // Made by another core: the start would not take it, so the card does not promise it.
        let mark = Mark::new("pocket-core", "gpsp", &rom, None);
        assert_eq!(open_moment(&game("pocket-core", "mgba", rom, mark), &files), None);
    }

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

    /// Continue where I quit is off: a Quit leaves the older moment at `slots::retry_auto_path` (`Core::keep_resume_point`),
    /// where Load finds it and no start takes it. So the shelf must not stand the game open for it, nor say Continue.
    #[cfg(not(target_os = "android"))]
    #[test]
    fn a_moment_left_for_load_is_not_a_waiting_moment() {
        let g = game("pocket-core", "mgba", PathBuf::from("/storage/roms/gba/Game.gba"), None);
        let files = resume_files(&g);
        let auto = crate::slots::auto_path(&crate::slots::base("gba", "Game"));
        assert!(files.contains(&auto) && files.contains(&crate::slots::taken_auto_path(&auto)), "what a start takes: {files:?}");
        assert!(!files.contains(&crate::slots::retry_auto_path(&auto)), "what Load lists and no start takes: {files:?}");
        // Load still lists it as Where I quit.
        let set = crate::states::StateSet::pocket_core(&crate::slots::base("gba", "Game"));
        assert!(set.auto.contains(&crate::slots::retry_auto_path(&auto)));
    }

    #[test]
    fn a_big_copy_is_told_apart_by_its_whole_file_check_on_a_phone() {
        let d = crate::testutil::TestDir::new("frozen-big-copy");
        let rom = d.join("Game.iso");
        std::fs::write(&rom, vec![7u8; 200_000]).unwrap();
        let of = |up_to| fingerprint_up_to(&rom, up_to).unwrap();
        let bare = of(10);
        // Beside the copy after a copy; the same content and another time: the same game.
        std::fs::write(d.join(".stage"), "200000 111 3735928559\n").unwrap();
        let checked = of(10);
        assert_ne!(checked, bare);
        std::fs::write(d.join(".stage"), "200000 222 3735928559\n").unwrap();
        assert_eq!(of(10), checked);
        // The same samples, patched between them.
        std::fs::write(d.join(".stage"), "200000 222 1234\n").unwrap();
        assert_ne!(of(10), checked);
        assert_eq!(of(0), bare_linux(&rom), "no check beside a handheld's file counts");
    }

    #[test]
    fn a_copied_disc_with_its_tracks_is_told_apart_by_the_whole_sets_check() {
        let d = crate::testutil::TestDir::new("frozen-big-set");
        let cue = d.join("Game.cue");
        std::fs::write(&cue, "FILE \"Game (Track 1).bin\" BINARY\n  TRACK 01 MODE2/2352\n").unwrap();
        std::fs::write(d.join("Game (Track 1).bin"), vec![7u8; 200_000]).unwrap();
        let of = || fingerprint_up_to(&cue, 10).unwrap();
        // `GameCopy.prepareSet`'s stage: the whole set's line, then one per file with its own time.
        std::fs::write(d.join(".stage"), "200050 111 3735928559\n50 111 Game.cue\n200000 111 Game (Track 1).bin\n").unwrap();
        let checked = of();
        assert_eq!(checked.0, 200_000 + std::fs::metadata(&cue).unwrap().len(), "the sheet and its track");
        std::fs::write(d.join(".stage"), "200050 222 3735928559\n50 222 Game.cue\n200000 0 Game (Track 1).bin\n").unwrap();
        assert_eq!(of(), checked, "copied again, the same bytes: the same game");
        std::fs::write(d.join(".stage"), "200050 222 1234\n50 222 Game.cue\n200000 222 Game (Track 1).bin\n").unwrap();
        assert_ne!(of(), checked, "a track patched between the samples is another game");
    }

    fn bare_linux(rom: &Path) -> (u64, u64) {
        fingerprint_up_to(rom, 0).unwrap()
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
    fn read_whole_a_patch_between_the_samples_shows() {
        let d = TestDir::new("frozen-whole");
        let rom = d.join("Game.gb");
        std::fs::write(&rom, vec![7u8; 64 * 1024]).unwrap();
        let (sampled, whole) = (fingerprint_up_to(&rom, 0), fingerprint_up_to(&rom, u64::MAX));
        let mut patched = vec![7u8; 64 * 1024];
        patched[8192] = 8;
        std::fs::write(&rom, patched).unwrap();
        assert_eq!(fingerprint_up_to(&rom, 0), sampled, "the samples don't see it");
        assert_ne!(fingerprint_up_to(&rom, u64::MAX), whole);
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
        let m = |c: &str| Mark { emulator: "pocket-core".into(), core: c.into(), rom_bytes: 1, rom_mtime: 1, rom_hash: 0, patches: None, auto: None };
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
        r.list[0].frozen = Some(Mark { emulator: "pocket-core".into(), core: "mgba".into(), rom_bytes: 3, rom_mtime: 4, rom_hash: 0, patches: None, auto: None });
        r.save();
        let again = crate::recents::Recents::load(&d);
        assert_eq!(again.list[0].frozen, r.list[0].frozen);
    }

    #[test]
    fn a_patch_added_replaced_or_removed_invalidates_the_moment_and_it_stays() {
        let (_d, rom, state) = setup("patch");
        std::fs::write(&state, b"s").unwrap();
        let files = vec![state.clone()];
        let at = |mark: &Option<Mark>| at_launch_with(&game("pocket-core", "mgba", rom.clone(), mark.clone()), true, state.exists());
        let ips = rom.with_extension("ips");
        let write = |p: &Path, data: &[u8], secs: u64| {
            std::fs::write(p, data).unwrap();
            std::fs::File::options().write(true).open(p).unwrap().set_modified(UNIX_EPOCH + std::time::Duration::from_secs(secs)).unwrap();
        };
        let mark = Mark::new("pocket-core", "mgba", &rom, None);
        assert_eq!(mark.as_ref().unwrap().patches, Some(0));
        assert_eq!(at(&mark), AtLaunch::Normal);
        write(&ips, b"PATCH\x00\x00\x00\x00\x01XEOF", 1000);
        assert_eq!(at(&mark), AtLaunch::Fresh(CHANGED), "added");
        assert!(!is_frozen(&game("pocket-core", "mgba", rom.clone(), mark.clone()), &files));
        let patched = Mark::new("pocket-core", "mgba", &rom, None);
        assert_eq!(at(&patched), AtLaunch::Normal);
        write(&ips, b"PATCH\x00\x00\x00\x00\x01YEOF", 2000);
        assert_eq!(at(&patched), AtLaunch::Fresh(CHANGED), "replaced, the same size");
        write(&ips, b"PATCH\x00\x00\x00\x00\x01XEOF", 1000);
        assert_eq!(at(&patched), AtLaunch::Normal, "the same patch as it was");
        write(&rom.with_extension("ips1"), b"PATCH\x00\x00\x01\x00\x01ZEOF", 1000);
        assert_eq!(at(&patched), AtLaunch::Fresh(CHANGED), "stacked");
        std::fs::remove_file(rom.with_extension("ips1")).unwrap();
        std::fs::remove_file(&ips).unwrap();
        assert_eq!(at(&patched), AtLaunch::Fresh(CHANGED), "removed");
        assert_eq!(at(&mark), AtLaunch::Normal, "back as it was made");
        assert!(state.exists(), "the state stays for Load");
    }

    #[test]
    fn a_patch_is_known_by_name_size_and_time_and_never_read() {
        let (_d, rom, _state) = setup("patch-stamp");
        let ips = rom.with_extension("ips");
        let when = UNIX_EPOCH + std::time::Duration::from_secs(5000);
        let write = |data: &[u8]| {
            std::fs::write(&ips, data).unwrap();
            std::fs::File::options().write(true).open(&ips).unwrap().set_modified(when).unwrap();
        };
        write(&[1; 1024]);
        let before = patch_stack(&rom);
        assert_ne!(before, 0);
        write(&[2; 1024]);
        assert_eq!(patch_stack(&rom), before, "the same name, size and time: its bytes aren't looked at");
        // A restore that gives it a new time starts fresh once, no worse than that.
        std::fs::File::options().write(true).open(&ips).unwrap().set_modified(when + std::time::Duration::from_secs(60)).unwrap();
        assert_ne!(patch_stack(&rom), before);
        // Another name of the same size and time.
        std::fs::File::options().write(true).open(&ips).unwrap().set_modified(when).unwrap();
        std::fs::rename(&ips, rom.with_extension("ups")).unwrap();
        assert_ne!(patch_stack(&rom), before);
    }

    #[test]
    fn a_mark_from_before_patches_is_not_held_against_a_patched_game() {
        let (_d, rom, state) = setup("oldmark");
        std::fs::write(&state, b"s").unwrap();
        let now = Mark::new("pocket-core", "mgba", &rom, None).unwrap();
        let old = format!(r#"{{"emulator":"pocket-core","core":"mgba","rom_bytes":{},"rom_mtime":{},"rom_hash":{}}}"#, now.rom_bytes, now.rom_mtime, now.rom_hash);
        let mark: Mark = serde_json::from_str(&old).unwrap();
        assert_eq!(mark.patches, None);
        let launch = |mark: &Mark| at_launch_with(&game("pocket-core", "mgba", rom.clone(), Some(mark.clone())), false, true);
        assert_eq!(launch(&mark), AtLaunch::Normal);
        // The 0.1.10 game that has played with its patch all along.
        std::fs::write(rom.with_extension("ips"), b"PATCH\x00\x00\x00\x00\x01XEOF").unwrap();
        assert_eq!(launch(&mark), AtLaunch::Normal, "unknown: compared with nothing");
        assert!(is_frozen(&game("pocket-core", "mgba", rom.clone(), Some(mark.clone())), &[state]));
        // A new mark has it on record, none included, and is held to it.
        std::fs::remove_file(rom.with_extension("ips")).unwrap();
        let new = Mark::new("pocket-core", "mgba", &rom, None).unwrap();
        let written = serde_json::to_string(&new).unwrap();
        assert!(written.contains("\"patches\":0"), "{written}");
        let back: Mark = serde_json::from_str(&written).unwrap();
        std::fs::write(rom.with_extension("ips"), b"PATCH\x00\x00\x00\x00\x01XEOF").unwrap();
        assert_eq!(launch(&back), AtLaunch::Fresh(CHANGED));
    }

    #[test]
    fn a_shared_moment_is_only_the_game_s_that_wrote_it_last() {
        let own = |game, others: &[Option<u64>]| shared_moment_is_own(game, others.iter().copied());
        assert!(own(Some((100, true)), &[Some(50)]), "this game wrote last");
        assert!(own(Some((100, true)), &[Some(50), Some(99)]));
        assert!(!own(Some((100, true)), &[Some(150)]), "the other wrote after it: its mark says nothing any more");
        assert!(!own(Some((100, true)), &[Some(100)]), "can't tell");
        assert!(!own(Some((100, false)), &[Some(50)]), "a session that wrote no moment");
        assert!(!own(None, &[Some(50)]), "first start of this game");
        // The other game wrote later and was taken off the recent games, or never played: no entry is no proof.
        assert!(!own(Some((10, true)), &[None]));
        assert!(!own(Some((100, true)), &[Some(50), None]));
    }

    #[test]
    fn a_hidden_game_still_shares_its_states_while_it_is_on_the_card() {
        let d = TestDir::new("frozen-twins");
        let put = |rel: &str| {
            let p = d.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(&p, b"r").unwrap();
            p
        };
        let (a, b, gba, gone) = (put("gb/A/Game.gb"), put("gb/B/Game.gb"), put("gba/Game.gba"), d.join("gb/Gone/Game.gb"));
        let other = put("gb/Other.gb");
        let gb = d.join("gb");
        let in_gb = |p: &Path| p.starts_with(&gb);
        let of = |shown: &[&Path], hidden: &[&Path]| twins(&a, shown.iter().copied(), hidden.iter().copied(), in_gb);
        assert_eq!(of(&[&a, &b, &other], &[]), [b.clone()], "on the shelf");
        assert_eq!(of(&[&a, &other], &[&b]), [b.clone()], "hidden, the ROM and its states still there");
        assert_eq!(of(&[&a, &b], &[&b]), [b.clone()], "once");
        assert!(of(&[&a, &other], &[&gone]).is_empty(), "hidden and since deleted");
        assert!(of(&[&a, &other], &[&gba]).is_empty(), "another system's");
        assert!(of(&[&a, &other], &[&a]).is_empty(), "itself hidden");
    }
}
