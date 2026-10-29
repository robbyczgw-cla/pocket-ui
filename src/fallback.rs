//! Safety net for the libretro host: a game pocket-core can't start (the core refuses it, crashes or
//! the host lacks something it needs) is started again right away through the system's default emulator
//! (ROCKNIX's RetroArch for most, standalone YabaSanshiro for the Saturn), and
//! remembered so it goes there directly from then on. Stored in `<data>/fallback.json`.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// pocket-core runs that end this fast with an error count as "didn't start".
pub const START_WINDOW_SECS: u64 = 30;

pub struct Fallback {
    file: PathBuf,
    games: BTreeSet<String>,
}

impl Fallback {
    pub fn load(data_dir: &Path) -> Fallback {
        let file = data_dir.join("fallback.json");
        let games = crate::fsutil::read_json_or_backup(&file).unwrap_or_default();
        Fallback { file, games }
    }

    pub fn contains(&self, rom: &Path) -> bool {
        self.games.contains(&rom.to_string_lossy().into_owned())
    }

    /// The player chose pocket-core for the game again ("Runs on"): it gets another try.
    pub fn remove(&mut self, rom: &Path) {
        if self.games.remove(&rom.to_string_lossy().into_owned()) {
            let _ = crate::fsutil::write_json(&self.file, &self.games);
        }
    }

    pub fn add(&mut self, rom: &Path) {
        if self.games.insert(rom.to_string_lossy().into_owned()) {
            let _ = crate::fsutil::write_json(&self.file, &self.games);
        }
    }
}

/// Whether a pocket-core run that just ended should be retried with RetroArch: it failed, it failed
/// early, nobody asked it to quit, and the reason isn't something RetroArch would miss as well (a BIOS) or do worse
/// with (a zip with several games, a damaged one: `archive::is_refusal`; a patch next to the game that doesn't apply,
/// where RetroArch would start the original: `softpatch::is_refusal`; the player is told what to do instead).
pub fn should_retry(exit_ok: bool, secs: u64, quit_requested: bool, reason: Option<&str>) -> bool {
    let missing_bios = reason.is_some_and(|r| r.to_lowercase().contains("bios"));
    let refused = reason.is_some_and(|r| crate::archive::is_refusal(r) || crate::softpatch::is_refusal(r));
    !exit_ok && secs < START_WINDOW_SECS && !quit_requested && !missing_bios && !refused
}

/// A run that ends with an error this soon never got the game on the screen: a damaged file, a missing BIOS.
pub const NEVER_STARTED_SECS: u64 = 5;

/// Whether a run that just ended never started the game: it failed within `NEVER_STARTED_SECS` and nobody asked it
/// to quit. Such a run is no play session (it is not "last played", not in the switcher, not a game to continue).
pub fn never_started(exit_ok: bool, secs: u64, quit_requested: bool) -> bool {
    !exit_ok && secs < NEVER_STARTED_SECS && !quit_requested
}

/// Whether a PortMaster port run that just ended never started: its launch script usually exits 0 even when the game
/// died at once (a library missing), so the exit code says nothing; a port gone within `NEVER_STARTED_SECS` without Quit is no session.
pub fn port_never_started(secs: u64, quit_requested: bool) -> bool {
    secs < NEVER_STARTED_SECS && !quit_requested
}

/// What pocket-ui says when a port ended at once: files or libraries missing is the usual reason, not a damaged file or a BIOS.
pub fn port_not_started_notice(name: &str) -> String {
    format!("{name} didn't start. It may need files or libraries that aren't installed: reinstall it in Ports, or check log.txt in its folder.")
}

/// What pocket-ui says when a game that pocket-core doesn't host (RetroArch, a standalone emulator, the next choice
/// after pocket-core failed) never started: no one else tells the player, the screen just goes back to the shelf.
/// `missing_bios`: what `hostsys::HostSystem::missing_bios` says for the game's system. Then the files are known and
/// named, as Add games names them, instead of a guess between a damaged file and a BIOS.
pub fn not_started_notice(name: &str, missing_bios: Option<&str>) -> String {
    match missing_bios {
        Some(bios) => format!("{name} didn't start. {bios}."),
        None => format!("{name} didn't start. The file may be damaged, or the system needs a BIOS file (Settings › Games › Add games)."),
    }
}

/// A pocket-core run that failed at once where no other emulator is listed for the system: the likeliest reason, and what to do.
pub fn core_missing_notice(name: &str) -> String {
    format!("{name} didn't start. The system's core may not be installed: add it in RetroArch (Flatpak) > Online Updater > Core Downloader.")
}

/// What pocket-ui tells the player when a game goes to the system's default emulator (`emulator`, as in
/// es_systems.cfg) from now on.
pub fn notice(emulator: &str) -> String {
    let name = match emulator {
        "retroarch" => "RetroArch",
        "yabasanshiro" => "YabaSanshiro",
        "scummvm-sa" => "ScummVM",
        "pico-8" => "PICO-8",
        _ => "the system's emulator",
    };
    format!("This game didn't start on pocket-core, so it runs with {name} from now on.")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;

    #[test]
    fn retries_only_early_unrequested_failures() {
        assert!(should_retry(false, 2, false, None), "crash right at the start");
        assert!(should_retry(false, 1, false, Some("The core couldn't load this game.")));
        assert!(!should_retry(true, 2, false, None), "a clean exit");
        assert!(!should_retry(false, 600, false, None), "a crash after playing a while");
        assert!(!should_retry(false, 3, true, None), "killed because the player chose Quit");
        assert!(!should_retry(false, 1, false, Some("Sega CD needs a BIOS file in roms/bios")), "RetroArch needs it too");
        let zip = crate::archive::extract_rom(Path::new("/nonexistent.zip"), &[]).unwrap_err();
        assert!(should_retry(false, 1, false, Some(&zip)), "a zip pocket-core can't read at all may open in RetroArch");
        for refused in [
            "Game.zip holds 2 game files. Unpack it on a computer and keep only the one you want.",
            "Game.zip is damaged: the game inside doesn't match its checksum. Copy the file to the SD card again.",
        ] {
            assert!(!should_retry(false, 1, false, Some(refused)), "the zip's own problem is told, not passed on");
        }
    }

    #[test]
    fn a_missing_core_is_told_in_words() {
        assert_eq!(
            core_missing_notice("Test Cartridge"),
            "Test Cartridge didn't start. The system's core may not be installed: add it in RetroArch (Flatpak) > Online Updater > Core Downloader."
        );
    }

    #[test]
    fn a_run_that_fails_at_once_never_started() {
        assert!(never_started(false, 0, false), "a damaged file: the emulator gives up at once");
        assert!(!never_started(true, 0, false), "a clean exit is a short play session, not a failure");
        assert!(!never_started(false, NEVER_STARTED_SECS, false), "a crash after a while was played");
        assert!(!never_started(false, 1, true), "Quit in the menu is not a failed start");
        assert_eq!(
            not_started_notice("Final Quest", None),
            "Final Quest didn't start. The file may be damaged, or the system needs a BIOS file (Settings › Games › Add games)."
        );
    }

    #[test]
    fn a_port_that_ends_at_once_never_started_whatever_its_exit_code() {
        assert!(port_never_started(1, false), "SDL Bomber without libmodplug: the script exits 0 after a second");
        assert!(!port_never_started(NEVER_STARTED_SECS, false), "played a while");
        assert!(!port_never_started(1, true), "Quit is not a failed start");
        assert_eq!(
            port_not_started_notice("SDL Bomber"),
            "SDL Bomber didn't start. It may need files or libraries that aren't installed: reinstall it in Ports, or check log.txt in its folder."
        );
    }

    /// A Sega CD game on a card without its BIOS (the first-day run, 2026-10-26): the start failed and the notice
    /// guessed "damaged, or a BIOS", while pocket-ui knew which files were missing. It names them now, in German too.
    #[test]
    fn a_missing_bios_is_named_not_guessed() {
        let dir = TestDir::new("fallback-bios");
        let missing = crate::hostsys::find("segacd").unwrap().missing_bios(&dir);
        let text = not_started_notice("Pocket CD Demo", missing.as_deref());
        assert_eq!(
            text,
            "Pocket CD Demo didn't start. Sega CD needs a BIOS file in roms/bios: bios_CD_U.bin, bios_CD_E.bin or bios_CD_J.bin."
        );
        let de = crate::i18n::german().translate(&text).expect("a German line");
        assert_eq!(
            de,
            "Pocket CD Demo wurde nicht gestartet. Sega CD braucht eine BIOS-Datei in roms/bios: bios_CD_U.bin, bios_CD_E.bin oder bios_CD_J.bin."
        );
        // Add games names the same files: in German with "oder" there as well.
        assert_eq!(
            crate::i18n::german().translate(missing.as_deref().unwrap()).as_deref(),
            Some("Sega CD braucht eine BIOS-Datei in roms/bios: bios_CD_U.bin, bios_CD_E.bin oder bios_CD_J.bin")
        );
        // A system that needs none (or has it) keeps the general line.
        assert!(crate::hostsys::find("megadrive").unwrap().missing_bios(&dir).is_none());
    }

    #[test]
    fn notice_names_the_emulator_taking_over() {
        assert_eq!(notice("retroarch"), "This game didn't start on pocket-core, so it runs with RetroArch from now on.");
        assert_eq!(notice("yabasanshiro"), "This game didn't start on pocket-core, so it runs with YabaSanshiro from now on.");
        assert_eq!(notice("scummvm-sa"), "This game didn't start on pocket-core, so it runs with ScummVM from now on.");
        assert_eq!(notice(""),"This game didn't start on pocket-core, so it runs with the system's emulator from now on.");
    }

    #[test]
    fn remembers_games() {
        let dir = TestDir::new("fallback");
        let rom = Path::new("/storage/roms/sms/Game.sms");
        let mut f = Fallback::load(&dir);
        assert!(!f.contains(rom));
        f.add(rom);
        assert!(Fallback::load(&dir).contains(rom), "kept across restarts");
        f.remove(rom);
        assert!(!Fallback::load(&dir).contains(rom), "chosen again: another try");
    }
}
