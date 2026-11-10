//! Save-state slots of pocket-core games, shared by pocket-ui's pause menu and pocket-core's hotkeys.
//!
//! Eight slots per game plus the auto-resume state. Slot 1 is the file pocket-core has always
//! written (`<stem>.pstate`), so existing saves stay where they are; slots 2 to 8 are
//! `<stem>.pstate1` … `<stem>.pstate7` (RetroArch counts its slots the same way: `.state`, `.state1`).
//! Every state has a `<file>.png` preview beside it. The slot a game last used is remembered in
//! `<data>/slots/<system>/<stem>`, so the menu and the hotkeys always agree.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::image::{self, Image};

pub const SLOTS: usize = 8;
pub const STATES_DIR: &str = "/storage/roms/savestates";

/// The folder of every game's states: `STATES_DIR` on the handhelds. `POCKETUI_STATES` moves it where there is no
/// `/storage` (the Steam Deck launcher points it into pocket-ui's data folder; docs/STEAMDECK.md), so pocket-ui and
/// pocket-core keep writing beside each other instead of into a folder that does not exist there.
pub fn states_dir() -> PathBuf {
    static DIR: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    DIR.get_or_init(|| dir_from(std::env::var("POCKETUI_STATES").ok().as_deref())).clone()
}

fn dir_from(value: Option<&str>) -> PathBuf {
    match value.map(str::trim) {
        Some(v) if !v.is_empty() => PathBuf::from(v),
        _ => PathBuf::from(STATES_DIR),
    }
}

/// Whether the launcher moved the states folder (`POCKETUI_STATES` set to a real path): pocket-core's moments then
/// live in pocket-ui's own data even where the host owns the emulators, so `frozen::available` can use them there.
pub fn states_moved() -> bool {
    moved_from(std::env::var("POCKETUI_STATES").ok().as_deref())
}

fn moved_from(value: Option<&str>) -> bool {
    value.map(str::trim).is_some_and(|v| !v.is_empty())
}

/// `<states>/<system>/<stem>`: the common start of every file of one game.
pub fn base(system: &str, stem: &str) -> PathBuf {
    states_dir().join(system).join(stem)
}

fn with_suffix(base: &Path, suffix: &str) -> PathBuf {
    let mut p = base.as_os_str().to_owned();
    p.push(suffix);
    PathBuf::from(p)
}

/// State file of a slot (0-based; slot 0 is shown as "Slot 1").
pub fn state_path(base: &Path, slot: usize) -> PathBuf {
    match slot {
        0 => with_suffix(base, ".pstate"),
        n => with_suffix(base, &format!(".pstate{n}")),
    }
}

/// The auto-resume state, written on every exit.
pub fn auto_path(base: &Path) -> PathBuf {
    with_suffix(base, ".pstate.auto.z")
}

/// Auto, taken after a start, and optionally the retired retry. Newest actual quit wins across a downgrade;
/// current quits precede crash recovery points. The same order drives Continue, Load and the shelf's preview.
pub fn auto_candidates(auto: &Path, include_retry: bool) -> Vec<PathBuf> {
    let mut groups = vec![auto.to_path_buf()];
    if let Some(old) = crate::state_file::legacy_path(auto) { groups.push(old); }
    let mut out = Vec::new();
    for stage in 0..if include_retry { 3 } else { 2 } {
        let mut paths: Vec<PathBuf> = groups.iter().map(|p| match stage {
            0 => p.clone(), 1 => taken_auto_path(p), _ => retry_auto_path(p),
        }).collect();
        paths.sort_by_key(|p| std::cmp::Reverse(std::fs::metadata(p).and_then(|m| m.modified()).ok()));
        out.extend(paths);
    }
    out
}

/// The ordinary Auto name of a taken/retry moment; the preview stays there. Keeps non-UTF-8 game names intact.
pub fn auto_root(path: &Path) -> PathBuf {
    use std::os::unix::ffi::{OsStrExt, OsStringExt};
    let bytes = path.as_os_str().as_bytes();
    let root = bytes.strip_suffix(b".taken.retry").or_else(|| bytes.strip_suffix(b".taken")).unwrap_or(bytes);
    std::ffi::OsString::from_vec(root.to_vec()).into()
}

/// Preview names in the same order as actual resume candidates, each namespace only once.
pub fn auto_previews(auto: &Path) -> Vec<PathBuf> {
    let mut seen = std::collections::HashSet::new();
    auto_candidates(auto, false).into_iter().map(|p| preview_path(&auto_root(&p))).filter(|p| seen.insert(p.clone())).collect()
}

/// "Before load": the state a Load replaced, kept until the next Quit (`clear_undo`).
pub fn undo_path(base: &Path) -> PathBuf {
    with_suffix(base, ".pstate.undo")
}

/// Drops the "Before load" state and its preview: a Quit ends it.
pub fn clear_undo(base: &Path) {
    let undo = undo_path(base);
    let _ = std::fs::remove_file(preview_path(&undo));
    let _ = std::fs::remove_file(undo);
}

/// What a Load came to (`load_keeping_undo`).
#[derive(Debug, PartialEq)]
pub enum Loaded {
    /// The file couldn't be read: nothing changed.
    Unreadable,
    /// Nothing was loaded: the running moment could not be kept for undo.
    UndoUnavailable,
    Loaded,
    /// The core refused it, why; the game is as it was (or back at Before load).
    Refused(String),
}

/// pocket-core's Load of any state file (a slot, Auto, Before load, an earlier state): the file is read first, what
/// runs now is kept as Before load (`keep` writes the running game's state and picture to the path it is given), then
/// the state goes in (`load`). A core that gave up after putting part of the state in place (its battery save among
/// it) goes back to the Before load just written (`back`), review SOL-02. Loading Before load itself swaps the two:
/// its file was read before it was replaced.
pub fn load_keeping_undo(
    base: &Path,
    state: &Path,
    keep: impl FnOnce(&Path) -> bool,
    load: impl FnOnce(&[u8]) -> Result<(), (bool, String)>,
    back: impl FnOnce(&[u8]),
) -> Loaded {
    let Ok(data) = crate::state_file::read(state) else { return Loaded::Unreadable };
    let undo = undo_path(base);
    let undo_written = keep(&undo);
    if !undo_written {
        return Loaded::UndoUnavailable;
    }
    match load(&data) {
        Ok(()) => Loaded::Loaded,
        Err((reached, why)) => {
            if reached && undo_written {
                if let Ok(before) = crate::state_file::read(&undo) {
                    back(&before);
                }
            }
            Loaded::Refused(why)
        }
    }
}

/// Where the auto-resume state goes once a start has loaded it (`libretro::Core::resume`), so a crash in
/// that session can't bring it back.
pub fn taken_auto_path(auto: &Path) -> PathBuf {
    with_suffix(auto, ".taken")
}

/// Where a taken state goes when the start after a crash tries it once more (`libretro::Core::resume`).
pub fn retry_auto_path(auto: &Path) -> PathBuf {
    with_suffix(auto, ".taken.retry")
}

/// A new auto-resume state `auto` is on disk: the taken ones are history now (and up to 40 MB each).
pub fn auto_written(auto: &Path) {
    let mut retired = false;
    for path in [taken_auto_path(auto), retry_auto_path(auto)] {
        crate::fsutil::crash_point("retiring an older moment");
        retired |= std::fs::remove_file(&path).is_ok();
    }
    // A downgraded player must start from its battery save, never a stale raw quit point. Its own later raw writes
    // cannot touch the separate compressed moment. Only retire raw Auto after the new durable save succeeded.
    if let Some(old) = crate::state_file::legacy_path(auto) {
        for path in [old.clone(), taken_auto_path(&old), retry_auto_path(&old)] {
            retired |= std::fs::remove_file(path).is_ok();
        }
        let _ = std::fs::remove_file(battery_unsaved_path(&old));
        retired |= std::fs::remove_file(preview_path(&old)).is_ok();
    }
    if retired {
        if let Some(dir) = auto.parent() {
            if let Ok(dir) = std::fs::File::open(dir) { let _ = dir.sync_all(); }
        }
    }
}

/// An empty file beside the auto-resume state: the exit that wrote it couldn't write the battery save (a full or
/// failing card), so the state holds a newer one than the card (`libretro::Core::resume` lets it win then).
fn battery_unsaved_path(auto: &Path) -> PathBuf {
    with_suffix(auto, ".battery-unsaved")
}

/// After the auto-resume state `auto` was written: notes whether the battery save of that exit failed. Best effort
/// (an empty file needs no room for data); a note that can't be made leaves the old behaviour.
pub fn battery_unsaved(auto: &Path, unsaved: bool) {
    crate::fsutil::crash_point("battery note");
    let note = battery_unsaved_path(auto);
    if unsaved {
        let _ = std::fs::File::create(note);
    } else {
        let _ = std::fs::remove_file(note);
    }
}

/// Whether the exit that wrote `auto` couldn't write the battery save; the note is gone afterwards.
pub fn take_battery_unsaved(auto: &Path) -> bool {
    std::fs::remove_file(battery_unsaved_path(auto)).is_ok()
}

/// The newest state of the game that is already on the card (a slot, Before load, an Earlier or session-start state, the
/// one this session started from), for an exit that cannot write a new one (`exit_guard`). Empty files don't count.
pub fn newest_written(base: &Path) -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = (0..SLOTS).map(|n| state_path(base, n)).collect();
    candidates.push(undo_path(base));
    // Earlier states exist only where pocket-core's timeline does (not in the Android player).
    #[cfg(not(target_os = "android"))]
    candidates.extend(crate::timeline::Name::all().map(|n| crate::timeline::path(base, n)));
    let auto = auto_path(base);
    candidates.push(taken_auto_path(&auto));
    if let Some(old) = crate::state_file::legacy_path(&auto) { candidates.push(taken_auto_path(&old)); }
    // A retry may be a failed or deliberately retired resume point. Never revive it.
    candidates
        .into_iter()
        .filter_map(|p| {
            let m = std::fs::metadata(&p).ok().filter(|m| m.is_file() && m.len() > 0)?;
            Some((m.modified().ok()?, p))
        })
        .max_by_key(|(at, _)| *at)
        .map(|(_, p)| p)
}

/// An exit save that cannot be written: the newest moment already on the card becomes the auto-resume state (a copy; the
/// slot stays what it is), unless that is the state the session started from, which the next start finds anyway
/// (`taken_auto_path`). Returns the file chosen.
pub fn resume_from_written(base: &Path) -> Option<PathBuf> {
    let auto = auto_path(base);
    if crate::state_file::exists(&auto) {
        return None;
    }
    let from = newest_written(base)?;
    if from == taken_auto_path(&auto) || from == retry_auto_path(&auto)
        || crate::state_file::legacy_path(&auto).is_some_and(|old| from == taken_auto_path(&old)) {
        return None;
    }
    // Open once: the background Earlier writer can atomically replace this path.
    // Bytes and timestamp must come from the same inode.
    use std::io::Read;
    let mut source = std::fs::File::open(&from).ok()?;
    let modified = source.metadata().ok()?.modified().ok()?;
    let mut data = Vec::new();
    source.read_to_end(&mut data).ok()?;
    data = crate::state_file::unpack(&from, data).ok()?;
    if data.is_empty() { return None; }
    // A stale failure marker must never make an older fallback authoritative.
    battery_unsaved(&auto, false);
    crate::state_file::write(&auto, &data).ok()?;
    // A copy is not a new capture. Preserve the clock used by Core::resume when
    // protecting newer battery saves. Keep the original resume as a fallback.
    use std::os::unix::fs::OpenOptionsExt;
    let stamp = std::fs::File::options().write(true).custom_flags(libc::O_NOFOLLOW).open(&auto)
        .and_then(|f| { f.set_modified(modified)?; f.sync_all() });
    if stamp.is_err() {
        let _ = std::fs::remove_file(&auto);
        return None;
    }
    if let Ok(png) = std::fs::read(preview_path(&from)) {
        let _ = crate::fsutil::replace_file(&preview_path(&auto), &png);
    }
    Some(from)
}

pub fn preview_path(state: &Path) -> PathBuf {
    with_suffix(state, ".png")
}

/// When the slot was last written; None for an empty slot.
pub fn saved_at(base: &Path, slot: usize) -> Option<SystemTime> {
    std::fs::metadata(state_path(base, slot)).and_then(|m| m.modified()).ok()
}

/// Which of the slots hold a state.
pub fn filled(base: &Path) -> [bool; SLOTS] {
    std::array::from_fn(|i| state_path(base, i).exists())
}

/// A slot's preview image (None when it has none) and when it was saved; None for an empty slot.
pub fn load_preview(base: &Path, slot: usize) -> Option<(Option<Image>, SystemTime)> {
    let when = saved_at(base, slot)?;
    Some((image::load_png(&preview_path(&state_path(base, slot))), when))
}

/// `slot` moved by `delta`, wrapping around the eight slots.
pub fn step(slot: usize, delta: i32) -> usize {
    (slot as i32 + delta).rem_euclid(SLOTS as i32) as usize
}

fn current_file(data_dir: &Path, system: &str, stem: &str) -> PathBuf {
    data_dir.join("slots").join(system).join(stem)
}

/// The slot this game used last (0-based); slot 1 for a game without one.
pub fn current(data_dir: &Path, system: &str, stem: &str) -> usize {
    std::fs::read_to_string(current_file(data_dir, system, stem)).ok().and_then(|s| s.trim().parse::<usize>().ok()).filter(|n| *n < SLOTS).unwrap_or(0)
}

pub fn set_current(data_dir: &Path, system: &str, stem: &str, slot: usize) {
    let file = current_file(data_dir, system, stem);
    let _ = crate::fsutil::write_atomic(&file, slot.to_string().as_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;

    #[test]
    fn slot_one_is_the_file_that_always_existed() {
        let b = Path::new("/storage/roms/savestates/gbc/uCity");
        assert_eq!(state_path(b, 0), Path::new("/storage/roms/savestates/gbc/uCity.pstate"));
        assert_eq!(state_path(b, 1), Path::new("/storage/roms/savestates/gbc/uCity.pstate1"));
        assert_eq!(state_path(b, 7), Path::new("/storage/roms/savestates/gbc/uCity.pstate7"));
        assert_eq!(auto_path(b), Path::new("/storage/roms/savestates/gbc/uCity.pstate.auto.z"));
        assert_eq!(undo_path(b), Path::new("/storage/roms/savestates/gbc/uCity.pstate.undo"));
        assert_eq!(taken_auto_path(&auto_path(b)), Path::new("/storage/roms/savestates/gbc/uCity.pstate.auto.z.taken"));
        assert_eq!(retry_auto_path(&auto_path(b)), Path::new("/storage/roms/savestates/gbc/uCity.pstate.auto.z.taken.retry"));
        assert_eq!(preview_path(&state_path(b, 2)), Path::new("/storage/roms/savestates/gbc/uCity.pstate2.png"));
    }

    #[test]
    fn a_stuck_exit_resumes_from_the_newest_moment_on_the_card() {
        use std::time::{Duration, SystemTime};
        let dir = TestDir::new("stuck-exit");
        let b = dir.join("Game");
        let put = |p: PathBuf, bytes: &[u8], ago: u64| {
            std::fs::write(&p, bytes).unwrap();
            std::fs::File::options().write(true).open(&p).unwrap().set_modified(SystemTime::now() - Duration::from_secs(ago)).unwrap();
        };
        // The state this session started from (old), a slot saved in the session, an empty file (doesn't count).
        put(taken_auto_path(&auto_path(&b)), b"started", 600);
        put(state_path(&b, 0), b"slot1", 60);
        put(state_path(&b, 1), b"", 5);
        put(preview_path(&state_path(&b, 0)), b"png", 60);
        assert_eq!(newest_written(&b), Some(state_path(&b, 0)));
        assert_eq!(resume_from_written(&b), Some(state_path(&b, 0)));
        assert_eq!(crate::state_file::read(&auto_path(&b)).unwrap(), b"slot1");
        assert_eq!(std::fs::read(preview_path(&auto_path(&b))).unwrap(), b"png");
        // Both original recovery points stay; a second call changes nothing.
        assert!(state_path(&b, 0).exists() && taken_auto_path(&auto_path(&b)).exists());
        assert_eq!(std::fs::metadata(auto_path(&b)).unwrap().modified().unwrap(),
            std::fs::metadata(state_path(&b, 0)).unwrap().modified().unwrap());
        assert_eq!(resume_from_written(&b), None);
    }

    #[test]
    fn a_stuck_exit_does_not_revive_a_retired_retry_or_temporary_write() {
        let dir = TestDir::new("stuck-exit-retired");
        let b = dir.join("Game");
        std::fs::write(retry_auto_path(&auto_path(&b)), b"retired").unwrap();
        std::fs::write(dir.join("Game.pstate.123-0.tmp"), b"partial").unwrap();
        assert_eq!(newest_written(&b), None);
        assert_eq!(resume_from_written(&b), None);
    }

    #[test]
    fn a_stuck_exit_leaves_the_state_the_session_started_from_to_the_next_start() {
        let dir = TestDir::new("stuck-exit-taken");
        let b = dir.join("Game");
        std::fs::write(taken_auto_path(&auto_path(&b)), b"started").unwrap();
        assert_eq!(resume_from_written(&b), None);
        assert!(taken_auto_path(&auto_path(&b)).exists() && !auto_path(&b).exists());
        // Nothing at all: nothing to do.
        assert_eq!(resume_from_written(&dir.join("Other")), None);
    }

    #[test]
    fn a_quit_ends_before_load_but_leaves_every_slot_and_the_auto_state() {
        let dir = TestDir::new("undo");
        let b = dir.join("Game");
        for f in [undo_path(&b), preview_path(&undo_path(&b)), state_path(&b, 0), preview_path(&state_path(&b, 0)), auto_path(&b)] {
            std::fs::write(f, b"x").unwrap();
        }
        clear_undo(&b);
        assert!(!undo_path(&b).exists() && !preview_path(&undo_path(&b)).exists());
        assert!(state_path(&b, 0).exists() && preview_path(&state_path(&b, 0)).exists() && auto_path(&b).exists());
        clear_undo(&b);
    }

    /// A made-up machine: its state is a byte string; `refuse` makes the next load fail after (`true`) or before taking
    /// part of the state.
    struct Machine {
        state: std::cell::RefCell<Vec<u8>>,
        refuse: Option<bool>,
    }

    impl Machine {
        fn load_file(&self, base: &Path, file: &Path) -> Loaded {
            load_keeping_undo(
                base,
                file,
                |undo| std::fs::write(undo, &*self.state.borrow()).is_ok(),
                |data| match self.refuse {
                    Some(reached) => {
                        if reached {
                            self.state.borrow_mut().truncate(1);
                        }
                        Err((reached, "the core refused it".into()))
                    }
                    None => {
                        *self.state.borrow_mut() = data.to_vec();
                        Ok(())
                    }
                },
                |before| *self.state.borrow_mut() = before.to_vec(),
            )
        }
    }

    #[test]
    fn save_safety_failed_undo_capture_blocks_any_mutating_load() {
        let dir = TestDir::new("load-no-undo");
        let target = dir.join("old");
        std::fs::write(&target, b"old progress").unwrap();
        let state = std::cell::RefCell::new(b"current progress".to_vec());
        let result = load_keeping_undo(&dir.join("Game"), &target, |_| false,
            |data| { *state.borrow_mut() = data.to_vec(); Ok(()) }, |_| panic!("nothing was loaded"));
        assert_eq!(*state.borrow(), b"current progress", "a full card must never permit an irreversible load");
        assert_ne!(result, Loaded::Loaded);
    }

    #[test]
    fn every_load_keeps_what_ran_as_before_load_an_earlier_state_too() {
        let dir = TestDir::new("load-undo");
        let b = dir.join("Game");
        let earlier = crate::timeline::path(&b, crate::timeline::Name { kind: crate::timeline::Kind::Every, n: 2 });
        std::fs::write(&earlier, b"twelve minutes ago").unwrap();
        let m = Machine { state: std::cell::RefCell::new(b"now".to_vec()), refuse: None };
        assert_eq!(m.load_file(&b, &earlier), Loaded::Loaded);
        assert_eq!(*m.state.borrow(), b"twelve minutes ago");
        assert_eq!(std::fs::read(undo_path(&b)).unwrap(), b"now", "Before load holds what ran");
        // Load → Before load goes back, and the earlier moment is Before load now (a swap).
        assert_eq!(m.load_file(&b, &undo_path(&b)), Loaded::Loaded);
        assert_eq!(*m.state.borrow(), b"now");
        assert_eq!(std::fs::read(undo_path(&b)).unwrap(), b"twelve minutes ago");
        // A core that gives up half-way goes back to the moment just kept; one that refuses at once is left alone.
        let m = Machine { state: std::cell::RefCell::new(b"playing".to_vec()), refuse: Some(true) };
        assert_eq!(m.load_file(&b, &earlier), Loaded::Refused("the core refused it".into()));
        assert_eq!(*m.state.borrow(), b"playing");
        // A file that can't be read changes nothing, Before load included.
        let before = std::fs::read(undo_path(&b)).unwrap();
        assert_eq!(m.load_file(&b, &dir.join("gone")), Loaded::Unreadable);
        assert_eq!(std::fs::read(undo_path(&b)).unwrap(), before);
    }

    #[test]
    fn the_states_folder_moves_only_with_pocketui_states() {
        assert_eq!(dir_from(None), Path::new("/storage/roms/savestates"), "the handhelds keep their folder");
        assert_eq!(dir_from(Some("  ")), Path::new("/storage/roms/savestates"), "an empty value moves nothing");
        assert_eq!(dir_from(Some("/data/pocket-ui/savestates")), Path::new("/data/pocket-ui/savestates"));
        assert_eq!(states_dir(), Path::new(STATES_DIR), "no variable in the test process");
        assert!(!moved_from(None) && !moved_from(Some("  ")), "unset or empty: not moved");
        assert!(moved_from(Some("/data/pocket-ui/savestates")), "the Deck launcher's value: moved");
    }

    #[test]
    fn dots_in_game_names_stay_in_the_name() {
        let b = base("snes", "Super Mario World (U) [!].v1.1");
        assert_eq!(state_path(&b, 3).file_name().unwrap(), "Super Mario World (U) [!].v1.1.pstate3");
    }

    #[test]
    fn stepping_wraps_both_ways() {
        assert_eq!(step(0, -1), 7);
        assert_eq!(step(7, 1), 0);
        assert_eq!(step(3, 1), 4);
        assert_eq!(step(3, -1), 2);
    }

    #[test]
    fn current_slot_survives_and_bad_files_mean_slot_one() {
        let dir = TestDir::new("slots");
        assert_eq!(current(&dir, "gbc", "Game"), 0);
        set_current(&dir, "gbc", "Game", 5);
        assert_eq!(current(&dir, "gbc", "Game"), 5);
        assert_eq!(current(&dir, "gba", "Game"), 0, "per console");
        assert_eq!(current(&dir, "gbc", "Other"), 0, "per game");
        std::fs::write(dir.join("slots/gbc/Game"), "42").unwrap();
        assert_eq!(current(&dir, "gbc", "Game"), 0, "out of range");
        std::fs::write(dir.join("slots/gbc/Game"), "junk").unwrap();
        assert_eq!(current(&dir, "gbc", "Game"), 0);
    }

    #[test]
    fn filled_and_saved_at_follow_the_files() {
        let dir = TestDir::new("slotfiles");
        let b = dir.join("Game");
        assert_eq!(filled(&b), [false; SLOTS]);
        assert!(saved_at(&b, 0).is_none() && load_preview(&b, 0).is_none());
        std::fs::write(state_path(&b, 0), b"x").unwrap();
        std::fs::write(state_path(&b, 4), b"x").unwrap();
        let f = filled(&b);
        assert!(f[0] && f[4] && !f[1] && !f[7]);
        assert!(saved_at(&b, 4).is_some());
        let (img, _) = load_preview(&b, 4).expect("a filled slot loads even without a preview");
        assert!(img.is_none());
    }
}
