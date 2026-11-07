//! A failed start may try the system emulator once. No automatic choice survives the retry or a UI restart;
//! an earlier version's unverified fallback list is retired so it cannot hide Continue moments.

use std::collections::BTreeSet;
use std::path::Path;

#[derive(Default)]
pub struct Fallback {
    games: BTreeSet<String>,
}

impl Fallback {
    pub fn load(data_dir: &Path) -> Fallback {
        let file = data_dir.join("fallback.json");
        // Old entries only prove an error in the first 30 s, possibly AFTER the game had shown frames.
        // Retire the primary and its recovery copy on the existing card writer, without fsync on the UI thread.
        if crate::fsutil::read_json_or_backup::<Vec<serde_json::Value>>(&file).is_some_and(|v| !v.is_empty()) {
            crate::fsutil::write_json_soon(&file, &Vec::<String>::new());
        }
        Fallback::default()
    }

    pub fn contains(&self, rom: &Path) -> bool {
        self.games.contains(rom.to_string_lossy().as_ref())
    }

    pub fn remove(&mut self, rom: &Path) {
        self.games.remove(rom.to_string_lossy().as_ref());
    }

    /// Only while resolving and launching the immediate retry. Never written to disk.
    pub fn add(&mut self, rom: &Path) {
        self.games.insert(rom.to_string_lossy().into_owned());
    }
}

/// Positive launch evidence from the per-process pipe. Missing/broken instrumentation is unknown, not a failure.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum StartOutcome {
    #[default]
    Unknown,
    NoFrame,
    Shown,
}

/// Includes Auto, crash-recovery copies, manual slots, undo and Earlier: automatic fallback must not hide any of them.
/// Called on the process waiter after a failed launch, never during drawing or a normal game start.
pub fn has_moments(base: &Path) -> bool {
    let set = crate::states::StateSet::pocket_core(base);
    set.auto.iter().chain(&set.slots).chain(std::iter::once(&set.undo))
        .any(|p| p.is_file())
        || !crate::frozen_phone::kept(base).is_empty()
        || crate::timeline::Name::all().any(|n| crate::timeline::path(base, n).is_file())
}

/// Whether a pocket-core run that just ended should be retried with RetroArch: it failed before any game frame,
/// nobody asked it to quit, and the reason isn't something RetroArch would miss as well (a BIOS) or do worse
/// with (a zip with several games, a damaged one: `archive::is_refusal`; a patch next to the game that doesn't apply,
/// where RetroArch would start the original: `softpatch::is_refusal`; the player is told what to do instead).
pub fn should_retry(exit_ok: bool, start: StartOutcome, quit_requested: bool, reason: Option<&str>) -> bool {
    let missing_bios = reason.is_some_and(|r| r.to_lowercase().contains("bios"));
    let refused = reason.is_some_and(|r| crate::archive::is_refusal(r) || crate::softpatch::is_refusal(r));
    !exit_ok && start == StartOutcome::NoFrame && !quit_requested && !missing_bios && !refused
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
    not_started_notice_on("", name, missing_bios)
}

/// Systems whose games never need a BIOS file: for them "damaged file" is the whole hint (new-user test 2026-10-30: a
/// Game Boy game was told to look for a BIOS file).
const NO_BIOS: &[&str] = &[
    "gb", "gbc", "gba", "nes", "famicom", "snes", "sfc", "megadrive", "genesis", "mastersystem", "gamegear", "sg-1000", "sg1000", "pcengine", "tg16", "atarilynx",
    "atari2600", "ngp", "ngpc", "wonderswan", "wonderswancolor", "virtualboy", "n64", "nds",
];

/// [`not_started_notice`] knowing the game's system: a system that needs no BIOS doesn't mention one.
pub fn not_started_notice_on(system: &str, name: &str, missing_bios: Option<&str>) -> String {
    match missing_bios {
        Some(bios) => format!("{name} didn't start. {bios}. BIOS files come from your own console."),
        None if NO_BIOS.contains(&system) => format!("{name} didn't start. The file may be damaged: copy the game to the card again."),
        None => format!("{name} didn't start. The file may be damaged, or the system needs a BIOS file (Settings › Games › Add games)."),
    }
}

/// A pocket-core run that failed at once where no other emulator is listed for the system: the likeliest reason, and what to do.
pub fn core_missing_notice(name: &str) -> String {
    format!("{name} didn't start. The system's core may not be installed: add it in RetroArch (Flatpak) > Online Updater > Core Downloader.")
}

/// The Steam Deck, the core's file is not there: pocket-core's own words ("load /usr/lib/libretro/x_libretro.so: cannot open
/// shared object file") name it. Returns the core's name from such a line.
pub fn missing_core_in(reason: &str) -> Option<String> {
    let rest = reason.strip_prefix("load ")?;
    let file = rest.split(": ").next()?.rsplit('/').next()?;
    reason.contains("cannot open shared object file").then(|| file.strip_suffix("_libretro.so").unwrap_or(file).to_string())
}

/// `missing_core_in`'s core, said to the player of a Steam Deck.
pub fn core_missing_named(name: &str, core: &str) -> String {
    format!("{name} didn't start. The {core} core isn't installed: get it in RetroArch (Flatpak) > Online Updater > Core Downloader, or run fetch-cores.sh.")
}

/// pocket-core's core file is not there on this device (not the Steam Deck, which has its own words above): the game is fine.
/// Said instead of "the file may be damaged" (2026-11-02: a GBA game was called broken when only the emulator was missing).
pub fn core_missing_here(name: &str, core: &str) -> String {
    format!("{name} didn't start. Its emulator, the {core} core, isn't on this device. The game itself is fine.")
}

/// A Flatpak emulator of the Steam Deck is not installed.
pub fn flatpak_missing_notice(name: &str, app_id: &str) -> String {
    format!("{name} didn't start. {app_id} isn't installed: install it from Discover (Flatpak).")
}

/// What pocket-ui tells the player when a game goes to the system's default emulator (`emulator`, as in
/// es_systems.cfg) for this launch.
pub fn notice(emulator: &str) -> String {
    let name = match emulator {
        "retroarch" => "RetroArch",
        "yabasanshiro" => "YabaSanshiro",
        "scummvm-sa" => "ScummVM",
        "pico-8" => "PICO-8",
        _ => "the system's emulator",
    };
    format!("This game didn't start on pocket-core, so it runs with {name} for this launch. Next time pocket-core will try again.")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;

    #[test]
    fn a_system_without_a_bios_never_sends_the_player_looking_for_one() {
        let gb = not_started_notice_on("gb", "Tetris", None);
        assert!(!gb.contains("BIOS") && gb.contains("damaged"), "{gb}");
        assert!(not_started_notice_on("psx", "Crash", None).contains("BIOS"), "a system that may need one keeps the hint");
        assert!(not_started_notice_on("gb", "X", Some("Sega CD needs a BIOS file in roms/bios: a")).contains("needs a BIOS"));
    }

    #[test]
    fn clock_regression_crash_at_seven_seconds_is_not_a_failed_start() {
        // The device already displayed the game after 0.4 s, then pocket-core was killed at 7 s.
        assert!(!should_retry(false, StartOutcome::Shown, false, None), "a played game must keep its Continue route");
    }

    #[test]
    fn retries_only_confirmed_unrequested_start_failures() {
        assert!(should_retry(false, StartOutcome::NoFrame, false, None), "crash right at the start");
        assert!(should_retry(false, StartOutcome::NoFrame, false, Some("The core couldn't load this game.")));
        assert!(!should_retry(true, StartOutcome::NoFrame, false, None), "a clean exit");
        assert!(!should_retry(false, StartOutcome::Unknown, false, None), "missing evidence is never a start failure");
        assert!(!should_retry(false, StartOutcome::NoFrame, true, None), "killed because the player chose Quit");
        assert!(!should_retry(false, StartOutcome::NoFrame, false, Some("Sega CD needs a BIOS file in roms/bios")), "RetroArch needs it too");
        let zip = crate::archive::extract_rom(Path::new("/nonexistent.zip"), &[]).unwrap_err();
        assert!(should_retry(false, StartOutcome::NoFrame, false, Some(&zip)), "a zip pocket-core can't read at all may open in RetroArch");
        for refused in [
            "Game.zip holds 2 game files. Unpack it on a computer and keep only the one you want.",
            "Game.zip is damaged: the game inside doesn't match its checksum. Copy the file to the SD card again.",
        ] {
            assert!(!should_retry(false, StartOutcome::NoFrame, false, Some(refused)), "the zip's own problem is told, not passed on");
        }
    }

    #[test]
    fn the_deck_names_the_missing_core_and_the_missing_flatpak() {
        let raw = "load /usr/lib/libretro/gearsystem_libretro.so: /usr/lib/libretro/gearsystem_libretro.so: cannot open shared object file: No such file or directory";
        assert_eq!(missing_core_in(raw).as_deref(), Some("gearsystem"));
        assert_eq!(missing_core_in("The core couldn't load this game."), None);
        assert_eq!(
            core_missing_named("Test Cartridge", "gearsystem"),
            "Test Cartridge didn't start. The gearsystem core isn't installed: get it in RetroArch (Flatpak) > Online Updater > Core Downloader, or run fetch-cores.sh."
        );
        assert_eq!(
            flatpak_missing_notice("Space Courier", "org.DolphinEmu.dolphin-emu"),
            "Space Courier didn't start. org.DolphinEmu.dolphin-emu isn't installed: install it from Discover (Flatpak)."
        );
    }

    #[test]
    fn a_missing_core_is_the_emulator_not_the_game() {
        let raw = "load /usr/lib/libretro/mgba_libretro.so: /usr/lib/libretro/mgba_libretro.so: cannot open shared object file: No such file or directory";
        assert_eq!(missing_core_in(raw).as_deref(), Some("mgba"));
        let said = core_missing_here("Test Game", "mgba");
        assert!(said.contains("emulator") && said.contains("mgba") && said.contains("game itself is fine") && !said.contains("damaged"), "{said}");
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
            "Pocket CD Demo didn't start. Sega CD needs a BIOS file in roms/bios: bios_CD_U.bin, bios_CD_E.bin or bios_CD_J.bin. BIOS files come from your own console."
        );
        let de = crate::i18n::german().translate(&text).expect("a German line");
        assert_eq!(
            de,
            "Pocket CD Demo wurde nicht gestartet. Sega CD braucht eine BIOS-Datei in roms/bios: bios_CD_U.bin, bios_CD_E.bin oder bios_CD_J.bin. BIOS-Dateien stammen von deiner eigenen Konsole."
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
        assert_eq!(notice("retroarch"), "This game didn't start on pocket-core, so it runs with RetroArch for this launch. Next time pocket-core will try again.");
        assert_eq!(notice("yabasanshiro"), "This game didn't start on pocket-core, so it runs with YabaSanshiro for this launch. Next time pocket-core will try again.");
        assert_eq!(notice("scummvm-sa"), "This game didn't start on pocket-core, so it runs with ScummVM for this launch. Next time pocket-core will try again.");
        assert_eq!(notice(""),"This game didn't start on pocket-core, so it runs with the system's emulator for this launch. Next time pocket-core will try again.");
    }

    #[test]
    fn fallback_is_only_for_the_immediate_retry_and_old_entries_are_retired() {
        let dir = TestDir::new("fallback-session");
        let rom = dir.join("Game.gba");
        let mut f = Fallback::load(&dir);
        f.add(&rom);
        assert!(f.contains(&rom));
        assert!(!Fallback::load(&dir).contains(&rom), "a restart returns to pocket-core");
        f.remove(&rom);
        assert!(!f.contains(&rom), "next launch tries pocket-core again");
        for old in [serde_json::json!([rom.to_string_lossy()]),
            serde_json::json!([{"path":rom.to_string_lossy(), "size":0, "mtime_ms":0}])] {
            crate::fsutil::write_json(&dir.join("fallback.json"), &old).unwrap();
            assert!(!Fallback::load(&dir).contains(&rom), "old 30-second guesses cannot hide progress");
            crate::fsutil::settle();
            assert_eq!(std::fs::read_to_string(dir.join("fallback.json")).unwrap().trim(), "[]");
        }
    }

    #[test]
    fn every_kind_of_pocket_moment_prevents_an_automatic_route_change() {
        let dir = TestDir::new("fallback-moments");
        let base = dir.join("Game");
        assert!(!has_moments(&base));
        let set = crate::states::StateSet::pocket_core(&base);
        let auto = crate::slots::auto_path(&base);
        let kept = [Path::new(&format!("{}.kept0", auto.display())).to_path_buf()];
        let paths = set.auto.into_iter().chain(set.slots).chain([set.undo]).chain(kept)
            .chain(crate::timeline::Name::all().map(|n| crate::timeline::path(&base, n)));
        for path in paths {
            std::fs::write(&path, b"moment").unwrap();
            assert!(has_moments(&base), "{} must remain reachable", path.display());
            assert_eq!(std::fs::read(&path).unwrap(), b"moment");
            std::fs::remove_file(path).unwrap();
        }
    }
}
