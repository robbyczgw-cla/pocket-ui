//! When the game's own save (the battery save, `.srm`, and the cartridge clock, `.rtc`) did not reach the card: a
//! full card, one that turned read-only, one that fails. pocket-core keeps writing every 10 s and keeps the last good
//! file in place (`fsutil::write_atomic` replaces it only once the new one is whole), so nothing on the card gets
//! worse; but the player has to know that what they do now is not kept.
//!
//! pocket-core finds out at its 10 s flush (`libretro::Core::save_trouble`, a look at what the writer thread left,
//! no waiting) and at the saves of a quit or a sleep. It shows a calm line over the game while it lasts
//! (`overlay::Hud::save_warning`) and leaves the kind in [`FILE`] (tmpfs) for pocket-ui: written when it starts,
//! removed once a save gets through again. pocket-ui reads that file when the pause menu opens (a line there, and a
//! question before Quit, `app_save_trouble`) and after a game ended (a notice on the shelf).
//!
//! Nothing blocks on it. Quit asks once; a sleep, the critical-battery power-off and the game switcher try the save
//! once, as always, and go on (docs/HOST.md "When a save can't be written").

use std::io;
use std::path::Path;

/// Where pocket-core leaves the kind of trouble while its game's save can't be written (tmpfs).
pub const FILE: &str = "/tmp/pocket-core.save-trouble";

/// Why the save didn't reach the card, as far as the player can do something about it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Trouble {
    /// No space left (ENOSPC, EDQUOT): freeing some lets the next try through.
    Full,
    /// The card can't be written at all right now: read-only (EROFS, after card errors), refused (EACCES, EPERM), or
    /// an input/output error.
    Unwritable,
}

impl Trouble {
    /// The kind of an error from writing a save.
    pub fn of(e: &io::Error) -> Trouble {
        match e.raw_os_error() {
            Some(libc::ENOSPC) | Some(libc::EDQUOT) => Trouble::Full,
            _ if e.kind() == io::ErrorKind::StorageFull => Trouble::Full,
            _ => Trouble::Unwritable,
        }
    }

    fn word(self) -> &'static str {
        match self {
            Trouble::Full => "full",
            Trouble::Unwritable => "unwritable",
        }
    }

    fn from_word(s: &str) -> Option<Trouble> {
        match s.trim() {
            "full" => Some(Trouble::Full),
            "unwritable" => Some(Trouble::Unwritable),
            _ => None,
        }
    }

    /// The line over the game and in the pause menu while saves fail.
    pub fn line(self) -> &'static str {
        match self {
            Trouble::Full => "Your progress couldn't be saved to the SD card. Free some space; it will try again.",
            Trouble::Unwritable => "Your progress couldn't be saved: the SD card can't be written to right now. It will try again.",
        }
    }

    /// The explanation under the pause menu's question before Quit: what is at stake, and what is safe.
    pub fn risk(self) -> &'static str {
        match self {
            Trouble::Full => "The SD card is full. Quitting now loses what you did since the last save that reached the SD card; that save is kept. Free some space first if you can.",
            Trouble::Unwritable => "The SD card can't be written to. Quitting now loses what you did since the last save that reached the SD card; that save is kept.",
        }
    }

    /// The notice on the shelf when a game ended while its save still couldn't be written.
    pub fn after_quit(self) -> &'static str {
        match self {
            Trouble::Full => "The game's last progress couldn't be saved: the SD card is full. The save before it is kept.",
            Trouble::Unwritable => "The game's last progress couldn't be saved: the SD card can't be written to. The save before it is kept.",
        }
    }
}

/// The pause menu's question before Quit while saves fail, and its two answers (Keep playing first: B and A on the
/// first row both leave things as they are).
pub const QUESTION: &str = "Couldn't save. Quit anyway?";
pub const KEEP_PLAYING: &str = "Keep playing";
pub const QUIT_ANYWAY: &str = "Quit anyway";

/// pocket-core: says how things stand in `file` (`FILE`): the kind while saves fail, no file once one got through.
/// Only called when that changes; a failure to write the note itself is ignored (it is tmpfs, and the line over the
/// game says it anyway).
pub fn publish_to(file: &Path, trouble: Option<Trouble>) {
    match trouble {
        Some(t) => {
            let _ = std::fs::write(file, t.word());
        }
        None => {
            let _ = std::fs::remove_file(file);
        }
    }
}

/// pocket-ui: what pocket-core left in `file`; None when its saves are fine (or it said nothing).
pub fn read_from(file: &Path) -> Option<Trouble> {
    Trouble::from_word(&std::fs::read_to_string(file).ok()?)
}

/// [`read_from`] [`FILE`], then removes it: after a game ended, so the shelf says it once.
pub fn take() -> Option<Trouble> {
    let t = read_from(Path::new(FILE));
    let _ = std::fs::remove_file(FILE);
    t
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;

    #[test]
    fn a_full_card_is_told_apart_from_one_that_cannot_be_written() {
        assert_eq!(Trouble::of(&io::Error::from_raw_os_error(libc::ENOSPC)), Trouble::Full);
        assert_eq!(Trouble::of(&io::Error::from_raw_os_error(libc::EDQUOT)), Trouble::Full);
        assert_eq!(Trouble::of(&io::Error::from_raw_os_error(libc::EROFS)), Trouble::Unwritable);
        assert_eq!(Trouble::of(&io::Error::from_raw_os_error(libc::EIO)), Trouble::Unwritable);
        assert_eq!(Trouble::of(&io::Error::from_raw_os_error(libc::EACCES)), Trouble::Unwritable);
        assert_eq!(Trouble::of(&io::Error::other("anything else")), Trouble::Unwritable);
    }

    #[test]
    fn the_note_comes_and_goes_with_the_trouble() {
        let dir = TestDir::new("save-trouble");
        let file = dir.join("note");
        assert_eq!(read_from(&file), None, "no note: nothing to say");
        for t in [Trouble::Full, Trouble::Unwritable] {
            publish_to(&file, Some(t));
            assert_eq!(read_from(&file), Some(t));
        }
        publish_to(&file, None);
        assert!(!file.exists(), "a save that got through clears it");
        std::fs::write(&file, "garbled").unwrap();
        assert_eq!(read_from(&file), None, "a note it can't read is no reason to stop a Quit");
    }

    #[test]
    fn every_kind_has_its_words() {
        for t in [Trouble::Full, Trouble::Unwritable] {
            assert_eq!(Trouble::from_word(t.word()), Some(t));
            assert!(!t.line().is_empty() && !t.risk().is_empty() && !t.after_quit().is_empty());
            assert!(!t.line().contains('!') && !t.risk().contains('!'), "calm");
        }
        assert!(Trouble::Full.line().contains("Free some space"));
    }
}
