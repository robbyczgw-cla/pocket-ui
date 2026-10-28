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

/// `/storage/roms/savestates/<system>/<stem>`: the common start of every file of one game.
pub fn base(system: &str, stem: &str) -> PathBuf {
    Path::new(STATES_DIR).join(system).join(stem)
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
    with_suffix(base, ".pstate.auto")
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
    let Ok(data) = std::fs::read(state) else { return Loaded::Unreadable };
    let undo = undo_path(base);
    let undo_written = keep(&undo);
    match load(&data) {
        Ok(()) => Loaded::Loaded,
        Err((reached, why)) => {
            if reached && undo_written {
                if let Ok(before) = std::fs::read(&undo) {
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
    let _ = std::fs::remove_file(taken_auto_path(auto));
    let _ = std::fs::remove_file(retry_auto_path(auto));
}

/// An empty file beside the auto-resume state: the exit that wrote it couldn't write the battery save (a full or
/// failing card), so the state holds a newer one than the card (`libretro::Core::resume` lets it win then).
fn battery_unsaved_path(auto: &Path) -> PathBuf {
    with_suffix(auto, ".battery-unsaved")
}

/// After the auto-resume state `auto` was written: notes whether the battery save of that exit failed. Best effort
/// (an empty file needs no room for data); a note that can't be made leaves the old behaviour.
pub fn battery_unsaved(auto: &Path, unsaved: bool) {
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
        assert_eq!(auto_path(b), Path::new("/storage/roms/savestates/gbc/uCity.pstate.auto"));
        assert_eq!(undo_path(b), Path::new("/storage/roms/savestates/gbc/uCity.pstate.undo"));
        assert_eq!(taken_auto_path(&auto_path(b)), Path::new("/storage/roms/savestates/gbc/uCity.pstate.auto.taken"));
        assert_eq!(retry_auto_path(&auto_path(b)), Path::new("/storage/roms/savestates/gbc/uCity.pstate.auto.taken.retry"));
        assert_eq!(preview_path(&state_path(b, 2)), Path::new("/storage/roms/savestates/gbc/uCity.pstate2.png"));
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
