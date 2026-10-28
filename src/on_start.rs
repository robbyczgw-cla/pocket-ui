//! Settings > On start: what pocket-ui opens when the Pocket starts, Home (the default) or the game played last,
//! like a console woken from standby. The decision is `decide`, a plain function of what the start found, so every
//! rule is a test:
//!
//! 1. The resume note of a power off or a battery that ran out while asleep (`sleep::take_resume`) always wins, as
//!    before this setting existed, whatever it says.
//! 2. "Home" is Home.
//! 3. Only the first start of pocket-ui in a boot continues a game. A start after that is the launcher bringing it back
//!    after a crash, an update or a deploy, or EmulationStation handing back: Home. The boot is told by the kernel's
//!    boot id, kept in `<data>/started-boot` (`first_start_this_boot`).
//! 4. A crash notice waiting (pocket-ui panicked, or the last boot ended without a clean shutdown, `crash.rs`): Home.
//!    A game that froze the device must not be started straight into again.
//! 5. The last game must have ended well. `<data>/last-game.json` says which game was started last and how it ended
//!    (`note_started`, `note_ended`): still "playing" (pocket-ui or the device went down under it) or "failed" (the
//!    emulator ended with an error nobody asked for, the case that also sends a pocket-core game to RetroArch,
//!    `fallback.rs`) means Home. So does a record of a different game than the one to continue, or none at all.
//! 6. The game to continue is the first recent game Home shows (it still exists, its system isn't hidden).

use std::path::{Path, PathBuf};

/// Settings > On start.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OnStart {
    #[default]
    Home,
    /// The game played last, from where it was left.
    Continue,
}

impl OnStart {
    /// The value on the Settings row.
    pub fn label(self) -> &'static str {
        match self {
            OnStart::Home => "Home",
            OnStart::Continue => "Last game",
        }
    }

    pub fn toggled(self) -> OnStart {
        match self {
            OnStart::Home => OnStart::Continue,
            OnStart::Continue => OnStart::Home,
        }
    }
}

/// How the game started last ended, as far as pocket-ui saw it.
#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Ended {
    /// Started and not seen ending: pocket-ui or the device went down while it ran.
    Playing,
    /// Ended normally (Quit, the emulator's own exit, a save and quit before sleep or power off).
    Ok,
    /// Ended with an error nobody asked for.
    Failed,
}

#[derive(Clone, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub struct LastGame {
    pub path: PathBuf,
    pub ended: Ended,
}

const LAST_GAME_FILE: &str = "last-game.json";
const BOOT_FILE: &str = "started-boot";

/// Notes that the game `path` has just been started. A plain write, without waiting for the card: a note lost to a
/// power cut leaves an older one, and a power cut leaves a crash notice that sends the start to Home anyway (rule 4).
pub fn note_started(data_dir: &Path, path: &Path) {
    write(data_dir, &LastGame { path: path.to_path_buf(), ended: Ended::Playing });
}

/// Notes how the game `path` ended. `failed`: it ended with an error and nobody asked it to quit.
pub fn note_ended(data_dir: &Path, path: &Path, failed: bool) {
    write(data_dir, &LastGame { path: path.to_path_buf(), ended: if failed { Ended::Failed } else { Ended::Ok } });
}

fn write(data_dir: &Path, last: &LastGame) {
    if let Ok(json) = serde_json::to_vec(last) {
        let _ = std::fs::write(data_dir.join(LAST_GAME_FILE), json);
    }
}

/// The game started last and how it ended; None when nothing was noted or the note can't be read.
pub fn last_game(data_dir: &Path) -> Option<LastGame> {
    serde_json::from_slice(&std::fs::read(data_dir.join(LAST_GAME_FILE)).ok()?).ok()
}

/// Whether this is pocket-ui's first start in the boot `boot_id`, and notes that it has started in it. Without a boot
/// id (not Linux) every start counts as the first.
pub fn first_start_this_boot(data_dir: &Path, boot_id: Option<&str>) -> bool {
    let Some(id) = boot_id else { return true };
    let file = data_dir.join(BOOT_FILE);
    let before = std::fs::read_to_string(&file).unwrap_or_default();
    if before.trim() == id {
        return false;
    }
    let _ = std::fs::write(&file, id);
    true
}

/// What the start found.
pub struct Facts<'a> {
    pub setting: OnStart,
    /// A power off or a sleep that ran out of battery left a resume note (`sleep::take_resume`).
    pub resume_note: bool,
    /// The first-start setup is done (it runs instead of Home otherwise).
    pub onboarded: bool,
    /// See `first_start_this_boot`.
    pub first_start_this_boot: bool,
    /// A crash or unclean shutdown notice is waiting to be shown.
    pub crash_notice: bool,
    pub last_game: Option<&'a LastGame>,
    /// The first recent game Home shows.
    pub recent: Option<&'a Path>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Decision {
    /// Open the game of the resume note.
    Resume,
    /// Open the most recent game.
    Continue,
    /// Stay on Home, and why (for the log).
    Home(&'static str),
}

pub fn decide(f: &Facts) -> Decision {
    if f.resume_note {
        return Decision::Resume;
    }
    if f.setting == OnStart::Home {
        return Decision::Home("On start is Home");
    }
    if !f.onboarded {
        return Decision::Home("the first-start setup runs");
    }
    if !f.first_start_this_boot {
        return Decision::Home("pocket-ui was started before in this boot (a crash, an update or a restart)");
    }
    if f.crash_notice {
        return Decision::Home("the last run ended in a crash or without a clean shutdown");
    }
    let Some(recent) = f.recent else { return Decision::Home("no recent game") };
    match f.last_game {
        Some(l) if l.path == recent && l.ended == Ended::Ok => Decision::Continue,
        Some(l) if l.path == recent && l.ended == Ended::Failed => Decision::Home("the last game ended with an error"),
        Some(l) if l.path == recent => Decision::Home("the last game was still running when pocket-ui ended"),
        Some(_) => Decision::Home("the last game started is not the last one played"),
        None => Decision::Home("no record of how the last game ended"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;

    fn dir(name: &str) -> crate::testutil::TestDir {
        TestDir::new(&format!("onstart-{name}"))
    }

    const ROM: &str = "/storage/roms/gbc/uCity.gbc";

    fn ended(ended: Ended) -> LastGame {
        LastGame { path: PathBuf::from(ROM), ended }
    }

    /// Everything in favour of continuing; each test changes one thing.
    fn facts(last: &LastGame) -> Facts<'_> {
        Facts {
            setting: OnStart::Continue,
            resume_note: false,
            onboarded: true,
            first_start_this_boot: true,
            crash_notice: false,
            last_game: Some(last),
            recent: Some(Path::new(ROM)),
        }
    }

    #[test]
    fn continues_the_last_game_on_the_first_start_after_a_clean_end() {
        let ok = ended(Ended::Ok);
        assert_eq!(decide(&facts(&ok)), Decision::Continue);
    }

    #[test]
    fn home_is_the_default_and_stays_home() {
        assert_eq!(OnStart::default(), OnStart::Home);
        let ok = ended(Ended::Ok);
        let f = Facts { setting: OnStart::Home, ..facts(&ok) };
        assert!(matches!(decide(&f), Decision::Home(_)));
    }

    #[test]
    fn the_resume_note_of_a_power_off_wins_whatever_the_setting_and_the_crash_signs() {
        let playing = ended(Ended::Playing);
        for setting in [OnStart::Home, OnStart::Continue] {
            let f = Facts { setting, resume_note: true, first_start_this_boot: false, crash_notice: true, recent: None, last_game: Some(&playing), ..facts(&playing) };
            assert_eq!(decide(&f), Decision::Resume);
        }
    }

    #[test]
    fn a_restart_in_the_same_boot_goes_home() {
        // The launcher brought pocket-ui back after a crash or an update, or it was deployed again.
        let ok = ended(Ended::Ok);
        let f = Facts { first_start_this_boot: false, ..facts(&ok) };
        assert!(matches!(decide(&f), Decision::Home(_)));
    }

    #[test]
    fn a_crash_notice_goes_home() {
        let ok = ended(Ended::Ok);
        let f = Facts { crash_notice: true, ..facts(&ok) };
        assert!(matches!(decide(&f), Decision::Home(_)));
    }

    #[test]
    fn a_game_that_failed_or_never_ended_is_not_started_again() {
        for e in [Ended::Failed, Ended::Playing] {
            let last = ended(e);
            assert!(matches!(decide(&facts(&last)), Decision::Home(_)), "{e:?}");
        }
    }

    #[test]
    fn without_a_matching_clean_record_it_stays_home() {
        let ok = ended(Ended::Ok);
        let f = Facts { last_game: None, ..facts(&ok) };
        assert!(matches!(decide(&f), Decision::Home(_)), "no record");
        let other = LastGame { path: PathBuf::from("/storage/roms/gba/other.gba"), ended: Ended::Ok };
        let f = Facts { last_game: Some(&other), ..facts(&ok) };
        assert!(matches!(decide(&f), Decision::Home(_)), "a record of another game");
        let f = Facts { recent: None, ..facts(&ok) };
        assert!(matches!(decide(&f), Decision::Home(_)), "nothing played");
        let f = Facts { onboarded: false, ..facts(&ok) };
        assert!(matches!(decide(&f), Decision::Home(_)), "setup");
    }

    #[test]
    fn the_record_follows_start_and_end() {
        let d = dir("record");
        assert_eq!(last_game(&d), None);
        note_started(&d, Path::new(ROM));
        assert_eq!(last_game(&d), Some(ended(Ended::Playing)));
        note_ended(&d, Path::new(ROM), false);
        assert_eq!(last_game(&d), Some(ended(Ended::Ok)));
        note_ended(&d, Path::new(ROM), true);
        assert_eq!(last_game(&d), Some(ended(Ended::Failed)));
        // pocket-core failed to start it, RetroArch takes over (`fallback`): playing again, then its own end counts.
        note_started(&d, Path::new(ROM));
        note_ended(&d, Path::new(ROM), false);
        assert_eq!(last_game(&d), Some(ended(Ended::Ok)));
        std::fs::write(d.join(LAST_GAME_FILE), b"{garbage").unwrap();
        assert_eq!(last_game(&d), None);
    }

    #[test]
    fn only_the_first_start_of_a_boot_counts_as_first() {
        let d = dir("boot");
        assert!(first_start_this_boot(&d, Some("boot-a")));
        assert!(!first_start_this_boot(&d, Some("boot-a")), "restarted in the same boot");
        assert!(!first_start_this_boot(&d, Some("boot-a")));
        assert!(first_start_this_boot(&d, Some("boot-b")), "a new boot");
        assert!(first_start_this_boot(&d, None), "no boot id: always the first");
    }

    #[test]
    fn the_setting_reads_and_writes_as_words() {
        assert_eq!(serde_json::to_string(&OnStart::Continue).unwrap(), "\"continue\"");
        assert_eq!(serde_json::from_str::<OnStart>("\"home\"").unwrap(), OnStart::Home);
        assert_eq!(OnStart::Home.toggled(), OnStart::Continue);
        assert_eq!(OnStart::Continue.toggled().label(), "Home");
    }
}
