//! When the game's own save (the battery save, `.srm`, and the cartridge clock, `.rtc`) did not reach the card: a
//! full card, one that turned read-only, one that fails. pocket-core keeps writing every 10 s and keeps the last good
//! file in place (`fsutil::write_atomic` replaces it only once the new one is whole), so nothing on the card gets
//! worse; but the player has to know that what they do now is not kept. The same goes for a save that is on the card
//! but couldn't be read at the start (`Unreadable`): the game runs without it and nothing is written over it.
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
    /// The game's save is on the card but couldn't be read at the start (an input/output error): the game runs without
    /// it, and nothing is written over it this session (`libretro::Core`), so the save on the card stays as it was.
    Unreadable,
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
            Trouble::Unreadable => "unreadable",
        }
    }

    fn from_word(s: &str) -> Option<Trouble> {
        match s.trim() {
            "full" => Some(Trouble::Full),
            "unwritable" => Some(Trouble::Unwritable),
            "unreadable" => Some(Trouble::Unreadable),
            _ => None,
        }
    }

    /// The line over the game and in the pause menu while saves fail: one line at 1024x768, so the pause menu's rows
    /// keep their rhythm under it (taste pass 0.2). It tries again in both cases.
    pub fn line(self) -> &'static str {
        match self {
            Trouble::Full => "Couldn't save: the SD card is full. Free some space.",
            Trouble::Unwritable => "Couldn't save to the SD card. Trying again.",
            Trouble::Unreadable => "Couldn't read this game's save. It stays as it was.",
        }
    }

    /// The explanation under the pause menu's question before Quit: what is at stake, and what is safe.
    pub fn risk(self) -> &'static str {
        match self {
            Trouble::Full => "SD card is full. Quitting now loses progress since your last good save (that save stays safe). Free some space first if you can.",
            Trouble::Unwritable => "SD card can't be written to. Quitting now loses progress since your last good save (that save stays safe).",
            Trouble::Unreadable => "Couldn't read this game's save, so it booted fresh. Your card save stays untouched; progress made now isn't kept.",
        }
    }

    /// The notice on the shelf when a game ended while its save still couldn't be written.
    pub fn after_quit(self) -> &'static str {
        match self {
            Trouble::Full => "Couldn't save latest progress: SD card is full. Your previous save is safe.",
            Trouble::Unwritable => "Couldn't save latest progress: SD card can't be written to. Your previous save is safe.",
            Trouble::Unreadable => "Couldn't read your save from the SD card, so nothing overwrote it. It stays intact.",
        }
    }
}

/// The notice on the shelf when pocket-ui's own settings or lists (Settings, Favourites, Recently played, play time)
/// couldn't be saved because the card is full ([`CardRefusals`]).
pub const CHANGES_FULL: &str = "Couldn't save changes: SD card is full. Free some space or they'll be lost on restart.";
/// The same when the card can't be written to at all (read-only after card errors, failing).
pub const CHANGES_UNWRITABLE: &str = "Couldn't save changes: SD card can't be written to. They won't survive a restart.";

impl Trouble {
    /// [`CHANGES_FULL`] or [`CHANGES_UNWRITABLE`].
    pub fn changes_lost(self) -> &'static str {
        if self == Trouble::Full { CHANGES_FULL } else { CHANGES_UNWRITABLE }
    }
}

/// pocket-ui: its own settings and lists that the card refused (`fsutil::take_write_trouble`), said on the shelf once
/// a run for each kind. They used to be logged and dropped: the change stayed until the next start and was then gone
/// without a word. Once is enough: on a read-only card every later change fails the same way.
#[derive(Default)]
pub struct CardRefusals {
    told: Vec<Trouble>,
}

impl CardRefusals {
    /// The notice to put up now, if a write under `data_dir` was refused since the last look and its kind wasn't said
    /// yet this run.
    pub fn next_notice(&mut self, data_dir: &Path) -> Option<&'static str> {
        let trouble = crate::fsutil::take_write_trouble(data_dir)?;
        if self.told.contains(&trouble) {
            return None;
        }
        self.told.push(trouble);
        Some(trouble.changes_lost())
    }
}

/// A moment that couldn't be read or put back: the game goes on as it was.
pub const LOAD_FAILED: &str = "Couldn't load save state. Your game is untouched.";
/// The pause menu's Screenshot couldn't be written.
pub const SCREENSHOT_FAILED: &str = "Couldn't save the screenshot.";
/// A setting of the pause menu's Options couldn't be kept for the next start.
pub const SETTING_FAILED: &str = "Couldn't save this setting for next launch.";
/// The player couldn't start the game (its emulator, its folders, its file).
pub const START_FAILED: &str = "The emulator couldn't start this game.";
/// Earlier before the first moment it keeps.
pub const NOTHING_EARLIER: &str = "Nothing earlier yet: a moment is kept every 5 min of play.";

/// What the player shows for an error: a sentence of ours as it is (one the interface translates), anything else (an
/// OS error, a core's or a library's words) as `instead` (the caller logs the raw text). Android's player showed "No
/// such file or directory (os error 2)" for Load on an empty slot (13.11.2026); every error that reaches its screen
/// goes through here (`android/src/player.rs`, `said`).
pub fn shown(error: &str, instead: &'static str) -> String {
    // A line of the table as it stands, or with numbers in it ("No save in slot {n} yet"); never a pattern with a
    // value in it: "{s} failed" would take "load libmgba.so: dlopen failed".
    static SHAPES: std::sync::OnceLock<std::collections::HashSet<String>> = std::sync::OnceLock::new();
    let shapes = SHAPES.get_or_init(|| crate::i18n::german().entries().into_iter().map(|(english, _)| english).collect());
    let mut shape = String::new();
    for (i, c) in error.char_indices() {
        if !c.is_ascii_digit() {
            shape.push(c);
        } else if !error[..i].ends_with(|p: char| p.is_ascii_digit()) {
            shape.push_str("{n}");
        }
    }
    if shapes.contains(error) || shapes.contains(&shape) { error.to_string() } else { instead.to_string() }
}

/// A failed state write leaves older saves alone; shown after exit and over the game.
pub const MOMENT_FAILED: &str = "Couldn't save this state. Earlier saves are safe.";
/// Load is refused before calling the core when its undo point could not be kept.
pub const UNDO_FAILED: &str = "Load cancelled: couldn't back up current state. Game untouched.";

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

/// Keep the autosave result for the shelf. A successful retry clears only our own notice,
/// leaving crash/start errors alone. Both players use this after a quit or sleep save.
pub fn report_moment(file: &Path, complete: bool, trouble: Option<Trouble>) {
    if !complete {
        let _ = std::fs::write(file, trouble.map_or(MOMENT_FAILED, Trouble::after_quit));
    } else if std::fs::read_to_string(file).is_ok_and(|s| is_moment_notice(&s)) {
        let _ = std::fs::remove_file(file);
    }
}

fn is_moment_notice(s: &str) -> bool {
    s == MOMENT_FAILED || [Trouble::Full, Trouble::Unwritable, Trouble::Unreadable].iter().any(|t| s == t.after_quit())
}

/// The Android shell consumes its player's notice from private app storage once on return.
pub fn take_moment(file: &Path) -> Option<String> {
    let line = std::fs::read_to_string(file).ok()?;
    if !is_moment_notice(&line) { return None; }
    let _ = std::fs::remove_file(file);
    Some(line)
}

/// Android's player couldn't start a game from the Handheld view: it goes back at once and leaves why in `file`
/// (`player-start-error`), and the shelf says it in its notice, as pocket-ui says a pocket-core start that failed
/// (docs/user/android-playing.md.inc "Starting a game"); no Android dialog. Only our own sentences: anything else is [`START_FAILED`].
pub fn report_start_failed(file: &Path, sentence: &str) {
    let _ = std::fs::write(file, shown(sentence, START_FAILED));
}

/// The shelf, back in front: the start failure the player left, once (see [`report_start_failed`]).
pub fn take_start_failed(file: &Path) -> Option<String> {
    let line = std::fs::read_to_string(file).ok()?;
    let _ = std::fs::remove_file(file);
    Some(shown(line.trim(), START_FAILED))
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

/// A save state that pocket-ui moves aside while a standalone emulator saves or loads through its slot (Azahar's quick
/// slot, Dolphin's `.s00`, `standalone::azahar::park`) and couldn't put back: it is kept beside the slot
/// (`<slot>.pocket-ui`) and goes back at the next start of the game (`standalone::azahar::recover`).
pub const STATE_NOT_BACK: &str = "Couldn't move a save state back to the SD card. It's safe and will restore on next launch.";

// Not on Android: no standalone emulators there (`standalone` is in linux_modules.rs).
#[cfg(not(all(target_os = "android", not(feature = "handheld"))))]
static STATE_NOT_BACK_NOTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// pocket-ui: that a parked save state didn't go back (the rename failed), for the shelf after the game.
#[cfg(not(all(target_os = "android", not(feature = "handheld"))))]
pub(crate) fn note_state_not_back(e: &io::Error) {
    eprintln!("save_trouble: a parked save state did not go back: {e}");
    STATE_NOT_BACK_NOTED.store(true, std::sync::atomic::Ordering::Relaxed);
}

/// pocket-ui: [`STATE_NOT_BACK`] once after a standalone game when its parked save state didn't go back.
#[cfg(not(all(target_os = "android", not(feature = "handheld"))))]
pub(crate) fn take_state_not_back() -> Option<&'static str> {
    STATE_NOT_BACK_NOTED.swap(false, std::sync::atomic::Ordering::Relaxed).then_some(STATE_NOT_BACK)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The player's words for what went wrong: a sentence of ours as it is, an OS error or a core's or library's words
    /// never (Load on an empty slot showed "No such file or directory (os error 2)", 13.11.2026).
    #[test]
    fn the_player_shows_our_sentences_and_never_raw_error_text() {
        let missing = std::fs::read("/nonexistent/pocket-ui/state.pstate2").unwrap_err().to_string();
        let full = io::Error::from_raw_os_error(libc::ENOSPC).to_string();
        let json = serde_json::from_str::<serde_json::Value>("{").unwrap_err().to_string();
        for raw in [missing.as_str(), full.as_str(), json.as_str(), "Unwritable", "load /data/app/x/libmgba.so: dlopen failed", "No frame yet"] {
            assert_eq!(shown(raw, LOAD_FAILED), LOAD_FAILED, "{raw:?} is not ours");
        }
        let slot = crate::slots::load_refusal(&[false; 8], 1).unwrap();
        for ours in [LOAD_FAILED, SCREENSHOT_FAILED, SETTING_FAILED, START_FAILED, NOTHING_EARLIER, MOMENT_FAILED, UNDO_FAILED,
            Trouble::Full.line(), Trouble::Unwritable.line(), Trouble::Unreadable.line(), slot.as_str(),
            "Could not load or restore the game. Load another moment or return to the library. Your earlier saves are kept.",
            "A kept moment can only be loaded: pick a slot to save", "The emulator's files could not be prepared.", "The game's document could not be opened"] {
            assert_eq!(shown(ours, START_FAILED), ours, "our sentence {ours:?} stays (and is translated)");
        }
    }

    /// Every way an error reaches the screen of Android's player goes through `said` (`shown` and a log line): the
    /// pause menu's line (`message`), the start dialog. A new raw path fails here.
    #[test]
    fn every_error_on_the_android_player_s_screen_goes_through_said() {
        const PLAYER: &str = include_str!("../android/src/player.rs");
        let mut raw = vec![];
        for (n, line) in PLAYER.lines().enumerate() {
            let Some(at) = line.find("message = ").or_else(|| line.find("message=")) else { continue };
            let rhs = &line[at..];
            let ours = rhs.contains("said(")
                || rhs.contains("pocket_ui::i18n::tr(\"")
                // The Ok sentences of the pause menu; its Err arm is checked below.
                || rhs.starts_with("message = match (result, self.row)")
                // A pad's note (`android_pads::Pads::roster`): our sentence with the pad's name.
                || rhs.contains("note.clone()")
                // Restore defaults' question: our sentence, Linux's too (`pause_options`), no error.
                || rhs.contains("pause_options::RESTORE_ASK");
            if !ours { raw.push(format!("{}: {}", n + 1, line.trim())); }
        }
        assert!(raw.is_empty(), "an error reaches the player's screen as it is:\n{}", raw.join("\n"));
        let arms = &PLAYER[PLAYER.find("message = match (result, self.row)").unwrap()..];
        let arm = &arms[arms.find("(Err(e)").expect("the Err arm")..];
        assert!(arm[..arm.find(')').unwrap() + 30].contains("said(e"), "the pause menu's errors: {}", &arm[..80]);
        // Load on an empty slot never reads the missing file: refused with the slot's line, its button grey.
        assert!(PLAYER.contains("2 => match pocket_ui::slots::load_refusal(&self.filled_slots, self.slot)"), "Load asks first");
        assert!(PLAYER.contains("grey: [pocket_ui::slots::load_refusal(&self.filled_slots, self.slot).is_some()"), "Load is grey");
        let start = &PLAYER[PLAYER.find("match Player::new(").unwrap()..];
        let start = &start[start.find("Err(e) =>").expect("the start's error")..];
        assert!(start.lines().next().unwrap().contains("said(e"), "the start dialog: {}", start.lines().next().unwrap());
    }
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
        for t in [Trouble::Full, Trouble::Unwritable, Trouble::Unreadable] {
            publish_to(&file, Some(t));
            assert_eq!(read_from(&file), Some(t));
        }
        publish_to(&file, None);
        assert!(!file.exists(), "a save that got through clears it");
        std::fs::write(&file, "garbled").unwrap();
        assert_eq!(read_from(&file), None, "a note it can't read is no reason to stop a Quit");
    }

    #[test]
    fn a_failed_moment_is_said_once_and_a_success_only_clears_save_errors() {
        let dir = TestDir::new("moment-trouble");
        let file = dir.join("error");
        for trouble in [None, Some(Trouble::Full), Some(Trouble::Unwritable), Some(Trouble::Unreadable)] {
            report_moment(&file, false, trouble);
            assert_eq!(take_moment(&file).as_deref(), Some(trouble.map_or(MOMENT_FAILED, |t| t.after_quit())));
            assert_eq!(take_moment(&file), None);
            report_moment(&file, false, trouble);
            report_moment(&file, true, None);
            assert!(!file.exists());
        }
        std::fs::write(&file, "another error").unwrap();
        report_moment(&file, true, None);
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "another error");
        assert_eq!(take_moment(&file), None);
    }

    #[test]
    fn every_kind_has_its_words() {
        for t in [Trouble::Full, Trouble::Unwritable, Trouble::Unreadable] {
            assert_eq!(Trouble::from_word(t.word()), Some(t));
            assert!(!t.line().is_empty() && !t.risk().is_empty() && !t.after_quit().is_empty());
            assert!(!t.line().contains('!') && !t.risk().contains('!'), "calm");
        }
        assert!(Trouble::Full.line().contains("Free some space"));
    }
    #[test]
    fn a_start_failure_reaches_the_shelf_once_and_only_in_our_words() {
        let dir = crate::testutil::TestDir::new("start-failed");
        let file = dir.join("player-start-error");
        assert_eq!(take_start_failed(&file), None, "nothing left: nothing said");
        report_start_failed(&file, START_FAILED);
        assert_eq!(take_start_failed(&file).as_deref(), Some(START_FAILED));
        assert_eq!(take_start_failed(&file), None, "said once");
        report_start_failed(&file, "load libmgba.so: dlopen failed");
        assert_eq!(take_start_failed(&file).as_deref(), Some(START_FAILED), "a library's words never reach the shelf");
        std::fs::write(&file, "anything a file may hold").unwrap();
        assert_eq!(take_start_failed(&file).as_deref(), Some(START_FAILED));
    }

}
