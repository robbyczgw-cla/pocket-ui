//! Frozen games on a phone (docs/ANDROID-MVP.md, "Frozen games"): the rules are `frozen`'s, this is where they meet
//! Android's files. A game's states are `states/<key>/<stem>` under the app's data, by its document's identity, and the
//! core plays a copy of the document (`content/<key>/<name>`), made at every start, or kept between starts for a big game
//! whose document hasn't changed (`GameCopy`). So the launcher can only compare the
//! document's size with the mark ([`stand_open`]); what the file holds is judged in the player, after the copy and before
//! the core sees a state ([`at_start`]). A moment that isn't loaded goes to a free slot, or behind the slots when all are
//! taken: Load finds it either way and no later Quit can overwrite it ([`set_aside`]). In-game saves aren't touched by
//! anything here.

use std::path::{Path, PathBuf};

use crate::frozen::{self, AtLaunch, Game, Mark};
use crate::libretro::Resume;
use crate::slots;

/// The only runner that can stand open on a phone: games handed to another app have no moment of ours.
pub const EMULATOR: &str = "pocket-core";

const CHANGED: &str = "The game's file changed: started fresh";
const OTHER_EMULATOR: &str = "Started fresh: its saved moment belongs to its other emulator";
const SAVE_NEWER: &str = "In-game save is newer: not resumed";
/// After a session that ended without an exit save (a crash, a dead battery, a moment that didn't reach the card).
pub const AGAIN: &str = "The game didn't close normally: back at your last saved moment";
const FULL: &str = "Its moment is kept under Load, after the last slot";
/// A handheld's Load has no places after the slots: the moment waits beside them for the first free one (`restore_kept`).
const FULL_HANDHELD: &str = "Its moment is kept until it can move into a free slot";
const STUCK: &str = "Its moment couldn't be moved and stays where it is";

/// `states/<key>/<stem>`, the common start of a game's files (`slots`).
pub fn base(data: &Path, key: &str, name: &str) -> PathBuf {
    data.join("states").join(key).join(Path::new(name).file_stem().unwrap_or_default())
}

/// The player's copy of the document.
pub fn staged_rom(data: &Path, key: &str, name: &str) -> PathBuf {
    data.join("content").join(key).join(name)
}

/// "mgba" of `.../libmgba_libretro.so`.
pub fn core_of(path: &Path) -> String {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or_default();
    name.strip_prefix("lib").and_then(|n| n.strip_suffix("_libretro.so")).unwrap_or(name).to_string()
}

/// The files the next start would resume from, as `frozen::resume_files` for pocket-core.
pub fn files(base: &Path) -> Vec<PathBuf> {
    let auto = slots::auto_path(base);
    slots::auto_candidates(&auto, false)
}

fn mark_path(base: &Path) -> PathBuf {
    let mut p = slots::auto_path(base).into_os_string();
    p.push(".mark");
    p.into()
}

/// The mark beside the state. A mark that can't be read trusts nothing: it matches no runner, so the state isn't loaded.
pub fn read_mark(base: &Path) -> Option<Mark> {
    let bytes = std::fs::read(crate::state_file::resolve(&mark_path(base))).ok()?;
    Some(serde_json::from_slice(&bytes).unwrap_or(Mark { emulator: "unreadable".into(), core: String::new(), rom_bytes: 0, rom_mtime: 0, rom_hash: 0, patches: None, auto: None }))
}

/// Written before the state it describes: a kill in between leaves a state that is at worst set aside, never one taken
/// for another file's.
pub fn write_mark(base: &Path, mark: &Mark) {
    if read_mark(base).as_ref() == Some(mark) {
        return;
    }
    if let Ok(json) = serde_json::to_vec(mark) {
        if let Err(e) = crate::fsutil::write_atomic(&mark_path(base), &json) {
            eprintln!("frozen: mark of {}: {e}", base.display());
        }
    }
}

/// One game the launcher started, newest first.
#[derive(Clone, Debug)]
pub struct Candidate {
    /// The document's identity (the launcher's `Game::key`).
    pub key: String,
    pub game: Game,
    pub base: PathBuf,
    /// The document's size now; negative when its provider doesn't say.
    pub size: i64,
}

/// The keys of the newest [`frozen::MOST`] games that stand open. On a thread: it reads the copies' samples.
pub fn stand_open(games: &[Candidate]) -> Vec<String> {
    let open = |c: &&Candidate| {
        let same_size = c.size < 0 || c.game.mark.as_ref().is_none_or(|m| m.rom_bytes == c.size as u64);
        same_size && frozen::is_frozen(&c.game, &files(&c.base))
    };
    games.iter().filter(open).take(frozen::MOST).map(|c| c.key.clone()).collect()
}

/// Where a moment that isn't loaded went.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Aside {
    /// This slot (0-based), where Load finds it.
    Slot(usize),
    /// Waiting beside the slots (all taken, or conversion failed), under a name no Quit removes; Load lists it after the slots ([`kept`]).
    Kept,
    /// It couldn't be moved.
    Stuck,
}

fn kept_path(base: &Path, from: &Path) -> PathBuf {
    let auto = slots::auto_path(base);
    // Keep the source format so a full card needs only a rename, never a new compressed/raw copy.
    let auto = if crate::state_file::legacy_path(from).is_none() {
        crate::state_file::legacy_path(&auto).unwrap_or(auto)
    } else {
        auto
    };
    (0u32..)
        .map(|n| {
            let mut p = auto.clone().into_os_string();
            p.push(format!(".kept{n}"));
            PathBuf::from(p)
        })
        .find(|p| !p.exists())
        .unwrap_or(auto)
}

/// The moments set aside with every slot taken, oldest first: Load shows them after the slots.
pub fn kept(base: &Path) -> Vec<PathBuf> {
    let auto = slots::auto_path(base);
    let Some(dir) = auto.parent() else { return Vec::new() };
    let mut prefixes = vec![format!("{}.kept", auto.file_name().unwrap_or_default().to_string_lossy())];
    if let Some(old) = crate::state_file::legacy_path(&auto) { prefixes.push(format!("{}.kept", old.file_name().unwrap_or_default().to_string_lossy())); }
    let mut found: Vec<(u32, PathBuf)> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| Some((prefixes.iter().find_map(|prefix| e.file_name().to_str()?.strip_prefix(prefix).and_then(|s| s.parse().ok()))?, e.path())))
        .collect();
    found.sort();
    found.into_iter().map(|(_, p)| p).collect()
}

/// A kept moment moves into a slot as soon as one is free, oldest first.
pub fn restore_kept(base: &Path) {
    let mut free = (0..slots::SLOTS).filter(|i| !slots::state_path(base, *i).exists());
    for moment in kept(base) {
        let Some(slot) = free.next() else { return };
        let to = slots::state_path(base, slot);
        if crate::state_file::move_file(&moment, &to).is_ok() {
            let _ = std::fs::rename(slots::preview_path(&moment), slots::preview_path(&to));
        }
    }
}

/// Moves the state `from` out of what a start or a Quit touches, with the auto state's picture when there is one.
pub fn set_aside(base: &Path, from: &Path) -> Aside {
    let from = crate::state_file::resolve(from);
    let free = (0..slots::SLOTS).find(|i| !slots::state_path(base, *i).exists());
    let mut to = free.map_or_else(|| kept_path(base, &from), |i| slots::state_path(base, i));
    let aside = if crate::state_file::move_file(&from, &to).is_ok() {
        free.map_or(Aside::Kept, Aside::Slot)
    } else {
        // ENOSPC while expanding a compressed moment into a raw slot must not leave it in .taken.retry:
        // a later successful Quit removes that name. A same-format rename keeps it outside that cleanup.
        to = kept_path(base, &from);
        if std::fs::rename(&from, &to).is_err() {
            return Aside::Stuck;
        }
        Aside::Kept
    };
    let _ = std::fs::rename(slots::preview_path(&slots::auto_root(&from)), slots::preview_path(&to));
    aside
}

/// A handheld's start that doesn't load the moment (`frozen::SKIP_RESUME_ENV`: another file's, another emulator's): every
/// moment the start would have continued from goes aside. Where the first went, None when there was none.
pub fn set_aside_all(base: &Path) -> Option<Aside> {
    let mut first = None;
    for from in files(base).into_iter().filter(|p| p.is_file()) {
        let aside = set_aside(base, &from);
        first.get_or_insert(aside);
    }
    first
}

/// After a handheld's `Core::resume` failed: the moment the core refused (`Core::resumed_from`) goes aside when it can be
/// read (another core's, another version's: it may load again elsewhere). One that can't be read stays where `resume` left it.
pub fn set_aside_refused(base: &Path, moment: Option<&Path>) -> Option<Aside> {
    let moment = moment.filter(|p| crate::state_file::read(p).is_ok())?;
    Some(set_aside(base, moment))
}

fn aside_line(aside: Aside) -> String {
    match aside {
        Aside::Slot(i) => format!("Its moment is in slot {}", i + 1),
        Aside::Kept => FULL.into(),
        Aside::Stuck => STUCK.into(),
    }
}

/// [`aside_line`] on a handheld (pocket-core), whose Load lists no kept moments.
pub fn aside_line_handheld(aside: Aside) -> String {
    match aside {
        Aside::Kept => FULL_HANDHELD.into(),
        other => aside_line(other),
    }
}

/// What the player does about the moment before the core sees it.
#[derive(Debug, PartialEq, Eq)]
pub struct Start {
    /// `Core::resume` may look for the state.
    pub resume: bool,
    /// Lines to show over the game's first moments.
    pub note: Vec<String>,
}

/// `rom`: the copy just made of the document. `shown`: the shelf showed the game as frozen. Holds with the Frozen games
/// switch off too: the switch closes the cases, it doesn't make a state load for another file.
pub fn at_start(base: &Path, rom: &Path, core: &str, shown: bool) -> Start {
    restore_kept(base);
    let game = Game { system: String::new(), path: rom.into(), emulator: EMULATOR.into(), core: core.into(), mark: read_mark(base) };
    let found: Vec<PathBuf> = files(base).into_iter().filter(|f| f.exists()).collect();
    match frozen::at_launch_with(&game, shown, !found.is_empty()) {
        AtLaunch::Normal => Start { resume: true, note: Vec::new() },
        AtLaunch::Note(note) => Start { resume: true, note: vec![note.into()] },
        AtLaunch::Fresh(why) => {
            let why = if why == frozen::CHANGED { CHANGED } else { OTHER_EMULATOR };
            let asides: Vec<Aside> = found.iter().map(|f| set_aside(base, f)).collect();
            let stuck = asides.contains(&Aside::Stuck);
            if !stuck {
                let _ = std::fs::remove_file(mark_path(base));
                let _ = slots::take_battery_unsaved(&slots::auto_path(base));
            }
            let line = if stuck { Aside::Stuck } else { asides[0] };
            Start { resume: false, note: vec![why.into(), aside_line(line)] }
        }
    }
}

/// After `Core::resume`: what to say about how it went. A moment it didn't go back to is set aside: the next Quit would
/// remove it where it lies (`slots::auto_written`). `moment` is `Core::resumed_from`: another namespace may contain
/// an older crash point, so a compressed-name fallback alone cannot identify the state this start rejected.
pub fn after_resume(base: &Path, resume: Resume, moment: Option<&Path>) -> Vec<String> {
    let auto = slots::auto_path(base);
    let (first, lies) = match resume {
        Resume::NoState | Resume::Resumed => return Vec::new(),
        Resume::ResumedAgain => return vec![AGAIN.into()],
        Resume::Failed => (frozen::REFUSED, slots::retry_auto_path(&auto)),
        Resume::SaveIsNewer => (SAVE_NEWER, slots::taken_auto_path(&auto)),
    };
    let aside = set_aside(base, moment.unwrap_or(&lies));
    if aside != Aside::Stuck {
        let _ = std::fs::remove_file(mark_path(base));
    }
    vec![first.into(), aside_line(aside)]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;

    /// A played game: its copy, its state and the mark of that session.
    fn played(tag: &str) -> (TestDir, PathBuf, PathBuf, PathBuf) {
        let d = TestDir::new(&format!("frozen-phone-{tag}"));
        let rom = staged_rom(&d, "abc", "Game.gba");
        std::fs::create_dir_all(rom.parent().unwrap()).unwrap();
        std::fs::write(&rom, vec![7u8; 20000]).unwrap();
        let b = base(&d, "abc", "Game.gba");
        std::fs::create_dir_all(b.parent().unwrap()).unwrap();
        let auto = slots::auto_path(&b);
        crate::state_file::write(&auto, b"moment").unwrap();
        write_mark(&b, &Mark::new(EMULATOR, "mgba", &rom, None).unwrap());
        (d, rom, b, auto)
    }

    fn candidate(rom: &Path, b: &Path, key: &str) -> Candidate {
        let game = Game { system: "gba".into(), path: rom.into(), emulator: EMULATOR.into(), core: "mgba".into(), mark: read_mark(b) };
        Candidate { key: key.into(), game, base: b.into(), size: std::fs::metadata(rom).map_or(-1, |m| m.len() as i64) }
    }

    #[test]
    fn the_same_file_and_core_stand_open_and_load() {
        let (_d, rom, b, auto) = played("same");
        assert_eq!(stand_open(&[candidate(&rom, &b, "k")]), ["k"]);
        assert_eq!(at_start(&b, &rom, "mgba", true), Start { resume: true, note: vec![] });
        assert!(auto.exists(), "nothing moved: Core::resume takes it");
    }

    #[test]
    fn a_changed_file_starts_fresh_says_so_and_keeps_the_moment_in_a_slot() {
        let (_d, rom, b, auto) = played("changed");
        // Patched, same size: only what the copy holds can tell.
        let mut patched = vec![7u8; 20000];
        patched[0] = 8;
        std::fs::write(&rom, patched).unwrap();
        assert!(stand_open(&[candidate(&rom, &b, "k")]).is_empty(), "the copy changed: closed");
        let start = at_start(&b, &rom, "mgba", true);
        assert!(!start.resume, "never loaded for another file");
        assert_eq!(start.note, [CHANGED.to_string(), "Its moment is in slot 1".to_string()]);
        assert_eq!(std::fs::read(slots::state_path(&b, 0)).unwrap(), b"moment");
        assert!(!auto.exists());
        assert_eq!(read_mark(&b), None, "the mark went with its state");
        // The next Quit writes a new auto state: the old moment stays.
        crate::state_file::write(&auto, b"new").unwrap();
        slots::auto_written(&auto);
        assert_eq!(std::fs::read(slots::state_path(&b, 0)).unwrap(), b"moment");
    }

    #[test]
    fn a_document_of_another_size_stands_closed_before_it_is_copied() {
        let (_d, rom, b, _auto) = played("size");
        let mut c = candidate(&rom, &b, "k");
        c.size += 1;
        assert!(stand_open(std::slice::from_ref(&c)).is_empty());
        c.size = -1;
        assert_eq!(stand_open(&[c]), ["k"], "a provider that doesn't say: the start decides");
    }

    #[test]
    fn another_core_starts_fresh_and_a_taken_state_goes_along() {
        let (_d, rom, b, auto) = played("core");
        let taken = slots::taken_auto_path(&auto);
        crate::state_file::write(&taken, b"older").unwrap();
        assert!(stand_open(&[Candidate { game: Game { core: "gpsp".into(), ..candidate(&rom, &b, "k").game }, ..candidate(&rom, &b, "k") }]).is_empty());
        let start = at_start(&b, &rom, "gpsp", false);
        assert!(!start.resume);
        assert_eq!(start.note[0], OTHER_EMULATOR);
        assert!(!auto.exists() && !taken.exists(), "nothing left a later start would load");
        assert_eq!(std::fs::read(slots::state_path(&b, 0)).unwrap(), b"moment");
        assert_eq!(std::fs::read(slots::state_path(&b, 1)).unwrap(), b"older");
    }

    #[test]
    fn a_state_that_is_gone_says_so_only_when_the_shelf_promised_it() {
        let (_d, rom, b, auto) = played("gone");
        std::fs::remove_file(&auto).unwrap();
        assert!(stand_open(&[candidate(&rom, &b, "k")]).is_empty());
        assert_eq!(at_start(&b, &rom, "mgba", false), Start { resume: true, note: vec![] }, "a closed case says nothing");
        assert_eq!(at_start(&b, &rom, "mgba", true).note, ["Its saved moment is gone: started fresh"]);
    }

    #[test]
    fn the_phone_copy_has_no_patches_so_the_patch_field_never_closes_a_case() {
        let (_d, rom, b, _auto) = played("patches");
        assert_eq!(read_mark(&b).unwrap().patches, Some(0), "a mark made now records none");
        assert_eq!(stand_open(&[candidate(&rom, &b, "k")]), ["k"]);
        // A mark from before the field (0.1.9 and 0.1.10 phones) is not held against anything.
        let now = read_mark(&b).unwrap();
        let old = format!(r#"{{"emulator":"pocket-core","core":"mgba","rom_bytes":{},"rom_mtime":{},"rom_hash":{}}}"#, now.rom_bytes, now.rom_mtime, now.rom_hash);
        std::fs::write(mark_path(&b), old).unwrap();
        assert_eq!(read_mark(&b).unwrap().patches, None);
        assert_eq!(stand_open(&[candidate(&rom, &b, "k")]), ["k"]);
        assert_eq!(at_start(&b, &rom, "mgba", true), Start { resume: true, note: vec![] });
    }

    #[test]
    fn a_copy_that_is_gone_stands_closed() {
        let (_d, rom, b, _auto) = played("copy");
        std::fs::remove_file(&rom).unwrap();
        assert!(stand_open(&[candidate(&rom, &b, "k")]).is_empty());
    }

    #[test]
    fn a_mark_that_cannot_be_read_never_loads_the_state() {
        let (_d, rom, b, auto) = played("garbled");
        std::fs::write(mark_path(&b), b"{not json").unwrap();
        let start = at_start(&b, &rom, "mgba", true);
        assert!(!start.resume);
        assert!(!auto.exists());
        assert_eq!(std::fs::read(slots::state_path(&b, 0)).unwrap(), b"moment");
    }

    #[test]
    fn a_state_from_before_the_marks_goes_by_its_state_as_the_start_always_did() {
        let (_d, rom, b, _auto) = played("old");
        std::fs::remove_file(mark_path(&b)).unwrap();
        assert_eq!(stand_open(&[candidate(&rom, &b, "k")]), ["k"]);
        assert!(at_start(&b, &rom, "mgba", true).resume);
    }

    /// An app update keeps `files/`; a core that still takes the state goes on, one that refuses it is `after_resume`.
    #[test]
    fn after_an_update_a_refused_moment_is_kept_and_said() {
        let (_d, rom, b, auto) = played("update");
        assert!(at_start(&b, &rom, "mgba", true).resume, "the same core id and file after the update");
        let retry = slots::retry_auto_path(&auto);
        std::fs::rename(&auto, &retry).unwrap(); // What `Core::resume` leaves when the core refuses.
        assert_eq!(after_resume(&b, Resume::Failed, None), [frozen::REFUSED.to_string(), "Its moment is in slot 1".to_string()]);
        assert_eq!(std::fs::read(slots::state_path(&b, 0)).unwrap(), b"moment");
        assert_eq!(read_mark(&b), None);
    }

    #[test]
    fn a_newer_in_game_save_says_so_and_the_moment_stays() {
        let (_d, _rom, b, auto) = played("newer");
        std::fs::rename(&auto, slots::taken_auto_path(&auto)).unwrap();
        assert_eq!(after_resume(&b, Resume::SaveIsNewer, None)[0], SAVE_NEWER);
        assert_eq!(std::fs::read(slots::state_path(&b, 0)).unwrap(), b"moment");
        assert!(after_resume(&b, Resume::NoState, None).is_empty());
        assert_eq!(after_resume(&b, Resume::ResumedAgain, None), [AGAIN]);
    }

    fn fill_slots(b: &Path) {
        for i in 0..slots::SLOTS {
            std::fs::write(slots::state_path(b, i), format!("slot{i}")).unwrap();
        }
    }

    #[test]
    fn a_kept_moment_is_listed_for_load_oldest_first_without_its_picture() {
        let (_d, _rom, b, auto) = played("kept-listed");
        fill_slots(&b);
        assert!(kept(&b).is_empty());
        let mut names = Vec::new();
        for n in 0..12 {
            crate::state_file::write(&auto, format!("moment{n}").as_bytes()).unwrap();
            std::fs::write(slots::preview_path(&auto), b"picture").unwrap();
            assert_eq!(set_aside(&b, &auto), Aside::Kept);
            names.push(format!("moment{n}"));
        }
        // Number 10 after number 2, and no picture among them: Load reads each of these as a state.
        let listed: Vec<String> = kept(&b).iter().map(|p| String::from_utf8(crate::state_file::read(p).unwrap()).unwrap()).collect();
        assert_eq!(listed, names);
        assert_eq!(aside_line(Aside::Kept), FULL);
    }

    #[test]
    fn a_raw_moment_from_an_older_version_is_kept_raw_and_survives_a_new_quit() {
        let (_d, _rom, b, auto) = played("kept-raw");
        std::fs::remove_file(&auto).unwrap();
        let raw = crate::state_file::legacy_path(&auto).unwrap();
        std::fs::write(&raw, b"old progress").unwrap();
        fill_slots(&b);
        assert_eq!(set_aside(&b, &raw), Aside::Kept);
        let saved = kept(&b);
        assert_eq!(saved.len(), 1);
        assert_eq!(std::fs::read(&saved[0]).unwrap(), b"old progress", "no format conversion while setting aside");
        crate::state_file::write(&auto, b"new session").unwrap();
        slots::auto_written(&auto);
        assert_eq!(crate::state_file::read(&saved[0]).unwrap(), b"old progress");
        std::fs::remove_file(slots::state_path(&b, 0)).unwrap();
        restore_kept(&b);
        assert_eq!(std::fs::read(slots::state_path(&b, 0)).unwrap(), b"old progress");
    }

    #[test]
    fn a_kept_moment_moves_into_a_slot_that_came_free_and_nothing_else_is_touched() {
        let (_d, rom, b, auto) = played("kept-back");
        fill_slots(&b);
        for moment in ["first", "second"] {
            crate::state_file::write(&auto, moment.as_bytes()).unwrap();
            assert_eq!(set_aside(&b, &auto), Aside::Kept);
        }
        crate::state_file::write(&auto, b"moment").unwrap();
        // Still full: they stay where Load finds them, and the game's own moment goes on.
        assert!(at_start(&b, &rom, "mgba", true).resume);
        assert_eq!(kept(&b).len(), 2);
        std::fs::remove_file(slots::state_path(&b, 3)).unwrap();
        assert!(at_start(&b, &rom, "mgba", true).resume);
        assert_eq!(std::fs::read(slots::state_path(&b, 3)).unwrap(), b"first");
        assert_eq!(kept(&b).len(), 1);
        assert_eq!(crate::state_file::read(&kept(&b)[0]).unwrap(), b"second");
        assert_eq!(std::fs::read(slots::state_path(&b, 0)).unwrap(), b"slot0");
        assert_eq!(crate::state_file::read(&auto).unwrap(), b"moment");
    }

    #[test]
    fn with_every_slot_taken_nothing_is_overwritten() {
        let (_d, rom, b, auto) = played("full");
        for i in 0..slots::SLOTS {
            std::fs::write(slots::state_path(&b, i), format!("slot{i}")).unwrap();
        }
        std::fs::write(&rom, b"another").unwrap();
        let start = at_start(&b, &rom, "mgba", false);
        assert_eq!(start.note[1], FULL);
        for i in 0..slots::SLOTS {
            assert_eq!(std::fs::read_to_string(slots::state_path(&b, i)).unwrap(), format!("slot{i}"));
        }
        let mut kept = auto.clone().into_os_string();
        kept.push(".kept0");
        assert_eq!(crate::state_file::read(Path::new(&kept)).unwrap(), b"moment");
    }

    #[test]
    fn five_stand_open_newest_first_and_the_file_names_survive() {
        let d = TestDir::new("frozen-phone-five");
        let games: Vec<Candidate> = (0..7)
            .map(|i| {
                let key = format!("k{i}");
                let name = format!("G {i}.v1.gba");
                let rom = staged_rom(&d, &key, &name);
                std::fs::create_dir_all(rom.parent().unwrap()).unwrap();
                std::fs::write(&rom, b"rom").unwrap();
                let b = base(&d, &key, &name);
                std::fs::create_dir_all(b.parent().unwrap()).unwrap();
                crate::state_file::write(&slots::auto_path(&b), b"s").unwrap();
                candidate(&rom, &b, &key)
            })
            .collect();
        assert_eq!(stand_open(&games), ["k0", "k1", "k2", "k3", "k4"]);
        assert_eq!(base(Path::new("/d"), "k", "G.v1.gba"), PathBuf::from("/d/states/k/G.v1"));
        assert_eq!(core_of(Path::new("/lib/libmgba_libretro.so")), "mgba");
    }
}
