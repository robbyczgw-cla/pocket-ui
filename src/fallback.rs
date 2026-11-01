//! Safety net for the libretro host: a game pocket-core can't start (the core refuses it, crashes or
//! the host lacks something it needs) is started again right away through the system's default emulator
//! (ROCKNIX's RetroArch for most, standalone YabaSanshiro for the Saturn), and
//! remembered so it goes there directly from then on. Stored in `<data>/fallback.json`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// pocket-core runs that end this fast with an error count as "didn't start".
pub const START_WINDOW_SECS: u64 = 30;

/// What the file looked like when pocket-core failed on it: a file copied over later differs.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Stamp {
    size: u64,
    mtime_ms: u64,
}

impl Stamp {
    fn of(path: &Path) -> Option<Stamp> {
        let meta = std::fs::metadata(path).ok()?;
        let mtime_ms = meta.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_millis() as u64;
        Some(Stamp { size: meta.len(), mtime_ms })
    }
}

/// An entry of `fallback.json`: a plain path (written before the stamp existed: kept as it was) or the path with its stamp.
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(untagged)]
enum Entry {
    Path(String),
    Stamped { path: String, size: u64, mtime_ms: u64 },
}

pub struct Fallback {
    file: PathBuf,
    games: BTreeMap<String, Option<Stamp>>,
}

impl Fallback {
    /// Entries whose file is gone or is another file now (copied over after the failure) are dropped: pocket-core gets a
    /// new try. Entries without a stamp stay until the player changes the game's emulator. A choice made under
    /// "Runs on" lives in system.cfg and is not touched here.
    pub fn load(data_dir: &Path) -> Fallback {
        let file = data_dir.join("fallback.json");
        let entries: Vec<Entry> = crate::fsutil::read_json_or_backup(&file).unwrap_or_default();
        let total = entries.len();
        let games: BTreeMap<String, Option<Stamp>> = entries
            .into_iter()
            .map(|e| match e {
                Entry::Path(path) => (path, None),
                Entry::Stamped { path, size, mtime_ms } => (path, Some(Stamp { size, mtime_ms })),
            })
            .filter(|(path, stamp)| match stamp {
                Some(stamp) => Stamp::of(Path::new(path)) == Some(*stamp),
                None => true,
            })
            .collect();
        let fallback = Fallback { file, games };
        if fallback.games.len() != total {
            fallback.save();
        }
        fallback
    }

    fn save(&self) {
        let entries: Vec<Entry> = self
            .games
            .iter()
            .map(|(path, stamp)| match stamp {
                Some(stamp) => Entry::Stamped { path: path.clone(), size: stamp.size, mtime_ms: stamp.mtime_ms },
                None => Entry::Path(path.clone()),
            })
            .collect();
        let _ = crate::fsutil::write_json(&self.file, &entries);
    }

    pub fn contains(&self, rom: &Path) -> bool {
        self.games.contains_key(&rom.to_string_lossy().into_owned())
    }

    /// The player chose pocket-core for the game again ("Runs on"): it gets another try.
    pub fn remove(&mut self, rom: &Path) {
        if self.games.remove(&rom.to_string_lossy().into_owned()).is_some() {
            self.save();
        }
    }

    pub fn add(&mut self, rom: &Path) {
        let stamp = Stamp::of(rom);
        if self.games.insert(rom.to_string_lossy().into_owned(), stamp) != Some(stamp) {
            self.save();
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

/// A Flatpak emulator of the Steam Deck is not installed.
pub fn flatpak_missing_notice(name: &str, app_id: &str) -> String {
    format!("{name} didn't start. {app_id} isn't installed: install it from Discover (Flatpak).")
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
    fn a_system_without_a_bios_never_sends_the_player_looking_for_one() {
        let gb = not_started_notice_on("gb", "Tetris", None);
        assert!(!gb.contains("BIOS") && gb.contains("damaged"), "{gb}");
        assert!(not_started_notice_on("psx", "Crash", None).contains("BIOS"), "a system that may need one keeps the hint");
        assert!(not_started_notice_on("gb", "X", Some("Sega CD needs a BIOS file in roms/bios: a")).contains("needs a BIOS"));
    }

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
        assert_eq!(notice("retroarch"), "This game didn't start on pocket-core, so it runs with RetroArch from now on.");
        assert_eq!(notice("yabasanshiro"), "This game didn't start on pocket-core, so it runs with YabaSanshiro from now on.");
        assert_eq!(notice("scummvm-sa"), "This game didn't start on pocket-core, so it runs with ScummVM from now on.");
        assert_eq!(notice(""),"This game didn't start on pocket-core, so it runs with the system's emulator from now on.");
    }

    #[test]
    fn remembers_games() {
        let dir = TestDir::new("fallback");
        let rom = dir.join("Game.sms");
        std::fs::write(&rom, b"rom").unwrap();
        let mut f = Fallback::load(&dir);
        assert!(!f.contains(&rom));
        f.add(&rom);
        assert!(Fallback::load(&dir).contains(&rom), "kept across restarts");
        f.remove(&rom);
        assert!(!Fallback::load(&dir).contains(&rom), "chosen again: another try");
    }

    #[test]
    fn a_game_copied_over_gets_pocket_core_again_at_the_next_start() {
        let dir = TestDir::new("fallback-recopy");
        let rom = dir.join("Game.gba");
        std::fs::write(&rom, vec![0u8; 100]).unwrap();
        let mut f = Fallback::load(&dir);
        f.add(&rom);
        assert!(Fallback::load(&dir).contains(&rom), "same file: stays");
        std::fs::write(&rom, vec![1u8; 4096]).unwrap();
        assert!(!Fallback::load(&dir).contains(&rom), "another file: dropped");
        assert!(!Fallback::load(&dir).contains(&rom), "and the drop is written");
        assert!(!std::fs::read_to_string(dir.join("fallback.json")).unwrap().contains("Game.gba"));
    }

    #[test]
    fn same_size_but_new_mtime_counts_as_another_file() {
        let dir = TestDir::new("fallback-mtime");
        let rom = dir.join("Game.gba");
        std::fs::write(&rom, vec![0u8; 100]).unwrap();
        let mut f = Fallback::load(&dir);
        f.add(&rom);
        let file = std::fs::File::options().write(true).open(&rom).unwrap();
        file.set_modified(std::time::SystemTime::now() + std::time::Duration::from_secs(3600)).unwrap();
        assert!(!Fallback::load(&dir).contains(&rom));
    }

    #[test]
    fn a_game_that_is_gone_is_dropped_and_old_entries_without_a_stamp_stay() {
        let dir = TestDir::new("fallback-old");
        let old = dir.join("Old.gba");
        std::fs::write(&old, b"x").unwrap();
        let gone = dir.join("Gone.gba");
        std::fs::write(&gone, b"x").unwrap();
        let mut f = Fallback::load(&dir);
        f.add(&gone);
        std::fs::remove_file(&gone).unwrap();
        let mut json: Vec<String> = vec![old.to_string_lossy().into_owned(), "/nowhere/Missing.gba".into()];
        let stamped = std::fs::read_to_string(dir.join("fallback.json")).unwrap();
        let mut parsed: Vec<serde_json::Value> = serde_json::from_str(&stamped).unwrap();
        parsed.extend(json.drain(..).map(serde_json::Value::from));
        std::fs::write(dir.join("fallback.json"), serde_json::to_string(&parsed).unwrap()).unwrap();
        let f = Fallback::load(&dir);
        assert!(f.contains(&old), "a plain path from before the stamp keeps working, as today");
        assert!(f.contains(Path::new("/nowhere/Missing.gba")), "no stamp: nothing to compare");
        assert!(!f.contains(&gone), "stamped and gone: dropped");
    }
}
