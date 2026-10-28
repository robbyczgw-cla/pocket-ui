//! What the pause menu's Save and Load know about a game's states, whatever the emulator. Three kinds:
//!
//! - **Manual slots**, numbered from 1 as players know them: Save writes only these, Load lists them.
//!   pocket-core games and PS2 and Wii/GameCube have eight, Azahar one (its quick slot: the other slots have
//!   menu entries but no shortcuts).
//! - **Auto**: what Quit and sleep write and the next start resumes from. It has a place of its own, never a
//!   manual slot, so a state the player saved is never replaced by it (and the other way round).
//! - **Before load** (undo): the state a Load is about to replace, kept until the next Quit.
//!
//! Load lists Auto first, then Before load, then the manual slots; Save only the slots. Where each emulator
//! keeps them is in the constructors below, the way each one is told to save or load them in `standalone.rs`
//! and `pocket_core.rs`.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::image::{self, Image};

/// Which state: what Load can pick from, and what Save writes.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Target {
    /// Written on Quit and before sleep, loaded by the next start.
    Auto,
    /// "Before load".
    Undo,
    /// A manual slot, counted from 1.
    Slot(u8),
}

/// Where one game's states are, and which of them exist.
#[derive(Clone, Debug, PartialEq)]
pub struct StateSet {
    /// The auto state: the first of these that exists (pocket-core's `.auto.taken` is the auto state a start
    /// has loaded, which is still the state to go back to).
    pub auto: Vec<PathBuf>,
    pub undo: PathBuf,
    /// Manual slots 1, 2, …
    pub slots: Vec<PathBuf>,
}

impl StateSet {
    /// pocket-core: `<stem>.pstate` (slot 1) to `.pstate7` (slot 8), `.pstate.auto` and `.pstate.undo`.
    pub fn pocket_core(base: &Path) -> StateSet {
        let auto = crate::slots::auto_path(base);
        StateSet {
            auto: vec![auto.clone(), crate::slots::taken_auto_path(&auto), crate::slots::retry_auto_path(&auto)],
            undo: crate::slots::undo_path(base),
            slots: (0..crate::slots::SLOTS).map(|i| crate::slots::state_path(base, i)).collect(),
        }
    }

    /// ARMSX2 (PCSX2's names: `<serial> (<CRC>).NN.p2s`). Slots 1 to 8 are ARMSX2's own slots 1 to 8, which is where
    /// a player coming from the Android app has them; the auto and undo states sit at 99 and 98, which no menu of
    /// ARMSX2 shows (it has 1 to 10) and PINE takes any number for.
    pub fn armsx2(base: &str) -> StateSet {
        let file = |n: u8| PathBuf::from(format!("/storage/roms/savestates/ps2/{base}.{n:02}.p2s"));
        StateSet { auto: vec![file(ARMSX2_AUTO)], undo: file(ARMSX2_UNDO), slots: (1..=8).map(file).collect() }
    }

    /// Dolphin (GameCube and Wii): `<game>.s01` to `.s08` are its slots 1 to 8, as its own menu and the Android app
    /// count them. `.s00` is the slot ROCKNIX's hotkeys start on, outside those: states pass through it
    /// (`standalone`), and the auto and undo states are files of ours beside the others.
    pub fn dolphin(system: &str, base: &str) -> StateSet {
        let dir = Path::new(crate::slots::STATES_DIR).join(system);
        StateSet {
            auto: vec![dir.join(format!("{base}.pocket-auto"))],
            undo: dir.join(format!("{base}.pocket-undo")),
            slots: (1..=8).map(|n| dir.join(format!("{base}.s{n:02}"))).collect(),
        }
    }

    /// Dolphin's transit slot (`.s00`).
    pub fn dolphin_transit(system: &str, base: &str) -> PathBuf {
        Path::new(crate::slots::STATES_DIR).join(system).join(format!("{base}.s00"))
    }

    /// Azahar: one manual slot, its quick slot `<title id>.00.cst`; the others are ours, named after the game file.
    pub fn azahar(title: &str, rom_file: &str) -> StateSet {
        let dir = Path::new(crate::slots::STATES_DIR).join("3ds");
        StateSet {
            auto: vec![dir.join(format!("{rom_file}.auto.cst"))],
            undo: dir.join(format!("{rom_file}.undo.cst")),
            slots: vec![dir.join(format!("{title}.00.cst"))],
        }
    }

    pub fn slot_count(&self) -> usize {
        self.slots.len()
    }

    /// What Save can write: the slots.
    pub fn save_targets(&self) -> Vec<Target> {
        (1..=self.slots.len() as u8).map(Target::Slot).collect()
    }

    /// What Load lists, in this order: Auto and Before load when they exist, then every slot (an empty one says so).
    pub fn load_targets(&self) -> Vec<Target> {
        let mut v = Vec::new();
        if self.path(Target::Auto).is_some() {
            v.push(Target::Auto);
        }
        if self.path(Target::Undo).is_some() {
            v.push(Target::Undo);
        }
        v.extend(self.save_targets());
        v
    }

    /// The file of `target` if it exists (Auto: the first that does).
    pub fn path(&self, target: Target) -> Option<&Path> {
        match target {
            Target::Auto => self.auto.iter().map(PathBuf::as_path).find(|p| p.exists()),
            Target::Undo => Some(self.undo.as_path()).filter(|p| p.exists()),
            Target::Slot(n) => self.slots.get(usize::from(n).checked_sub(1)?).map(PathBuf::as_path).filter(|p| p.exists()),
        }
    }

    /// Where `target` is or would be written.
    pub fn target_path(&self, target: Target) -> Option<&Path> {
        match target {
            Target::Auto => self.auto.first().map(PathBuf::as_path),
            Target::Undo => Some(self.undo.as_path()),
            Target::Slot(n) => self.slots.get(usize::from(n).checked_sub(1)?).map(PathBuf::as_path),
        }
    }

    /// Which slots hold a state.
    pub fn filled(&self) -> Vec<bool> {
        self.save_targets().into_iter().map(|t| self.path(t).is_some()).collect()
    }

    /// When `target` was written; None when it doesn't exist.
    pub fn saved_at(&self, target: Target) -> Option<SystemTime> {
        std::fs::metadata(self.path(target)?).and_then(|m| m.modified()).ok()
    }

    /// The preview and age of `target` (`<state>.png` beside it); None when the state doesn't exist.
    pub fn preview(&self, target: Target) -> Option<(Option<Image>, SystemTime)> {
        let path = self.path(target)?;
        // The auto state's picture stays beside its first name, whichever of its names the state has now.
        let picture = if target == Target::Auto { self.auto.first().map_or(path, PathBuf::as_path) } else { path };
        Some((image::load_png(&crate::slots::preview_path(picture)), self.saved_at(target)?))
    }
}

/// ARMSX2's slot numbers for the auto and undo states (`StateSet::armsx2`).
pub const ARMSX2_AUTO: u8 = 99;
pub const ARMSX2_UNDO: u8 = 98;

/// ARMSX2's states before the autosave moved: Quit wrote slot 0, which is a player's slot 0 too.
pub const ARMSX2_OLD_AUTO: u8 = 0;

/// The short name in the slot chip: "Auto", "Before load", or the number.
pub fn chip(target: Target) -> String {
    match target {
        Target::Auto => "Auto".into(),
        Target::Undo => "Before load".into(),
        Target::Slot(n) => n.to_string(),
    }
}

/// The line under the preview. `ago`: when it was written ("3 min ago"), None for an empty slot; `saving`: it is Save's list.
pub fn note(target: Target, ago: Option<&str>, saving: bool) -> String {
    match (target, ago) {
        (Target::Auto, Some(ago)) => format!("Auto · saved when you quit · {ago}"),
        (Target::Auto, None) => "Auto · saved when you quit · none yet".into(),
        (Target::Undo, Some(ago)) => format!("Before load · {ago}"),
        (Target::Undo, None) => "Before load · none".into(),
        (Target::Slot(n), Some(ago)) if saving => format!("Slot {n} · replaces the save from {ago}"),
        (Target::Slot(n), Some(ago)) => format!("Slot {n} · saved {ago}"),
        (Target::Slot(n), None) => format!("Slot {n} · empty"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;

    fn dir(tag: &str) -> crate::testutil::TestDir {
        TestDir::new(&format!("states-{tag}"))
    }

    fn set(root: &Path) -> StateSet {
        StateSet { auto: vec![root.join("auto")], undo: root.join("undo"), slots: (1..=3).map(|n| root.join(format!("s{n}"))).collect() }
    }

    #[test]
    fn load_lists_auto_first_then_before_load_then_the_slots() {
        let d = dir("order");
        let s = set(&d);
        assert_eq!(s.load_targets(), [Target::Slot(1), Target::Slot(2), Target::Slot(3)], "nothing saved yet: just the slots");
        std::fs::write(d.join("s2"), b"x").unwrap();
        std::fs::write(d.join("undo"), b"x").unwrap();
        assert_eq!(s.load_targets(), [Target::Undo, Target::Slot(1), Target::Slot(2), Target::Slot(3)]);
        std::fs::write(d.join("auto"), b"x").unwrap();
        assert_eq!(s.load_targets(), [Target::Auto, Target::Undo, Target::Slot(1), Target::Slot(2), Target::Slot(3)]);
        assert_eq!(s.save_targets(), [Target::Slot(1), Target::Slot(2), Target::Slot(3)], "Save never offers Auto or Before load");
        assert_eq!(s.filled(), [false, true, false], "and the dots count slots only");
    }

    #[test]
    fn the_auto_state_is_a_place_of_its_own_in_every_emulator() {
        let manual = |s: &StateSet| s.slots.clone();
        // pocket-core
        let pc = StateSet::pocket_core(Path::new("/storage/roms/savestates/gbc/uCity"));
        assert_eq!(pc.slots.len(), 8);
        assert_eq!(pc.slots[0], Path::new("/storage/roms/savestates/gbc/uCity.pstate"), "slot 1 is the file that always was");
        assert_eq!(pc.auto[0], Path::new("/storage/roms/savestates/gbc/uCity.pstate.auto"));
        assert_eq!(pc.undo, Path::new("/storage/roms/savestates/gbc/uCity.pstate.undo"));
        // ARMSX2: slots 1-8 are its slots 1-8, auto 99 and undo 98 (its menu has 1-10; the old auto was slot 0)
        let ps2 = StateSet::armsx2("SLES-12345 (ABCD1234)");
        assert_eq!(ps2.slots[0], Path::new("/storage/roms/savestates/ps2/SLES-12345 (ABCD1234).01.p2s"));
        assert_eq!(ps2.slots[7], Path::new("/storage/roms/savestates/ps2/SLES-12345 (ABCD1234).08.p2s"));
        assert_eq!(ps2.auto[0], Path::new("/storage/roms/savestates/ps2/SLES-12345 (ABCD1234).99.p2s"));
        assert_eq!(ps2.undo, Path::new("/storage/roms/savestates/ps2/SLES-12345 (ABCD1234).98.p2s"));
        // Dolphin: s01-s08, own files for auto and undo, s00 only a way through
        let gc = StateSet::dolphin("gamecube", "Mario Kart.iso");
        assert_eq!(gc.slots[0], Path::new("/storage/roms/savestates/gamecube/Mario Kart.iso.s01"));
        assert_eq!(gc.slots.len(), 8);
        assert_eq!(StateSet::dolphin_transit("gamecube", "Mario Kart.iso"), Path::new("/storage/roms/savestates/gamecube/Mario Kart.iso.s00"));
        // Azahar: one slot
        let ds = StateSet::azahar("00040000001B5000", "Game.3ds");
        assert_eq!(ds.slot_count(), 1);
        assert_eq!(ds.slots[0], Path::new("/storage/roms/savestates/3ds/00040000001B5000.00.cst"));
        for s in [&pc, &ps2, &gc, &ds] {
            for a in s.auto.iter().chain(std::iter::once(&s.undo)) {
                assert!(!manual(s).contains(a), "{a:?} is a manual slot");
            }
            assert_ne!(s.auto[0], s.undo);
        }
    }

    #[test]
    fn the_taken_auto_state_of_pocket_core_still_counts_as_auto() {
        let d = dir("taken");
        let base = d.join("Game");
        let s = StateSet::pocket_core(&base);
        assert!(s.path(Target::Auto).is_none());
        std::fs::write(crate::slots::taken_auto_path(&crate::slots::auto_path(&base)), b"x").unwrap();
        assert_eq!(s.load_targets()[0], Target::Auto, "a start took the auto state to resume from it: it is where this session began");
        std::fs::write(crate::slots::auto_path(&base), b"y").unwrap();
        assert_eq!(s.path(Target::Auto), Some(crate::slots::auto_path(&base).as_path()), "a newer one wins");
    }

    #[test]
    fn the_auto_picture_belongs_to_the_auto_name_even_when_the_state_is_the_taken_one() {
        let d = dir("autopng");
        let base = d.join("Game");
        let s = StateSet::pocket_core(&base);
        let auto = crate::slots::auto_path(&base);
        std::fs::write(crate::slots::taken_auto_path(&auto), b"x").unwrap();
        let mut png = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut png, 2, 2);
            enc.set_color(png::ColorType::Rgb);
            enc.set_depth(png::BitDepth::Eight);
            enc.write_header().unwrap().write_image_data(&[255u8; 12]).unwrap();
        }
        std::fs::write(crate::slots::preview_path(&auto), png).unwrap();
        let (img, _) = s.preview(Target::Auto).unwrap();
        assert!(img.is_some(), "the picture saved with the auto state shows for the taken one");
    }

    #[test]
    fn chips_and_notes_say_what_the_entry_is() {
        assert_eq!([Target::Auto, Target::Undo, Target::Slot(3)].map(chip), ["Auto", "Before load", "3"]);
        assert_eq!(note(Target::Auto, Some("3 min ago"), false), "Auto · saved when you quit · 3 min ago");
        assert_eq!(note(Target::Undo, Some("1 min ago"), false), "Before load · 1 min ago");
        assert_eq!(note(Target::Slot(2), Some("2 h ago"), true), "Slot 2 · replaces the save from 2 h ago");
        assert_eq!(note(Target::Slot(2), Some("2 h ago"), false), "Slot 2 · saved 2 h ago");
        assert_eq!(note(Target::Slot(2), None, false), "Slot 2 · empty");
    }
}
