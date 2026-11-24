//! A start that failed, shown (Animations: Cozy, Motion Full): the medium comes back out of the console and goes into
//! its case, the case goes back on the shelf, and one friendly sentence says what is missing and what to do. The
//! pictures are the launch animation backwards (`eject::Way::failed_moment`, `App::begin_failed_start`); this file is
//! the sentence.
//!
//! The reasons are the ones `fallback` and `library_cache` already found (a missing core, a missing BIOS, a damaged
//! or missing file, a port without its files); [`sentence`] only says them in a player's words: what is missing
//! first, what to do second, and never a core's file name, `log.txt` or a menu of another program. A reason it doesn't
//! know (pocket-core's own line about a zip, a patch) is already plain and stays as it is. Without the animation the
//! notice keeps its old words (`big_shelf::failure` reads them).

/// What the failed start came to, from the notice's words.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Why {
    /// The system needs a file from the player's own console: `system` and the file names it lists.
    Bios { system: String, files: Vec<String> },
    /// The emulator's core isn't on this device (`core` when its file name was found).
    Core { core: Option<String> },
    /// The game file is damaged, on a system that never needs a BIOS.
    Damaged,
    /// The game file is damaged, or the system may need a BIOS (nothing said which).
    DamagedOrBios,
    /// The emulator crashed while it loaded the game (`fallback::core_crashed_notice`).
    Crashed,
    /// A PortMaster port without its files.
    Port,
    /// Anything else: said already in plain words.
    Other,
}

/// Reads the reason out of a notice that `fallback` made for `name`.
pub fn why(name: &str, notice: &str) -> Why {
    let Some(rest) = notice.strip_prefix(name).and_then(|r| r.strip_prefix(" didn't start. ")) else { return Why::Other };
    if let Some(bios) = rest.strip_suffix(". BIOS files come from your own console.") {
        if let Some((system, files)) = bios.split_once(" needs a BIOS file in roms/bios: ") {
            // "a, b or c": the last one after " or ".
            let mut list: Vec<String> = match files.rsplit_once(" or ") {
                Some((head, last)) => head.split(", ").chain(std::iter::once(last)).map(String::from).collect(),
                None => vec![files.to_string()],
            };
            list.retain(|f| !f.is_empty());
            if !list.is_empty() {
                return Why::Bios { system: system.to_string(), files: list };
            }
        }
        return Why::DamagedOrBios;
    }
    if let Some(core) = rest.strip_prefix("Its emulator, the ").and_then(|r| r.strip_suffix(" core, isn't on this device. The game itself is fine.")) {
        return Why::Core { core: Some(core.to_string()) };
    }
    if rest.starts_with("The system's core may not be installed") {
        return Why::Core { core: None };
    }
    if rest.starts_with("Its emulator crashed while loading the game.") {
        return Why::Crashed;
    }
    if rest.starts_with("It may need files or libraries") {
        return Why::Port;
    }
    if rest == "The file may be damaged: copy the game to the card again." {
        return Why::Damaged;
    }
    if rest.starts_with("The file may be damaged, or the system needs a BIOS file") {
        return Why::DamagedOrBios;
    }
    Why::Other
}

/// The sentence a player reads: what is missing, then what to do. `notice` is the text the shelf would have shown.
pub fn sentence(name: &str, notice: &str) -> String {
    match why(name, notice) {
        Why::Bios { system, files } => match files.split_last() {
            Some((last, head)) if !head.is_empty() => {
                let head = head.join(", ");
                format!("{name} can't start yet: {system} needs a BIOS file ({head} or {last}). Drop one into roms/bios and try again.")
            }
            _ => {
                let file = files.first().map(String::as_str).unwrap_or_default();
                format!("{name} can't start yet: {system} needs a BIOS file ({file}). Drop it into roms/bios and try again.")
            }
        },
        Why::Core { core: Some(core) } => {
            // The emulator by the name players know (mGBA), not the core's file name.
            let emulator = crate::consoles::core_name(&core);
            format!("{name} can't start: emulator ({emulator}) is missing. Your game is fine. Check Settings › System › Updates.")
        }
        Why::Core { core: None } => {
            format!("{name} can't start: emulator missing for this system. Your ROM is fine. Check Settings › System › Updates.")
        }
        Why::Crashed => format!("{name} can't start: the emulator crashed while loading. Try again, or recopy the ROM to your SD card."),
        Why::Damaged => format!("{name} can't start. The ROM file may be corrupt. Recopy it to your SD card."),
        Why::DamagedOrBios => {
            format!("{name} can't start. The ROM may be corrupt, or a BIOS is missing. Recopy the game or check Settings › Games › Add games.")
        }
        Why::Port => format!("{name} can't start. Check its game card (X) for missing files, or reinstall via Settings › Games › Box art & downloads › PortMaster games."),
        Why::Other => notice.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fallback::{core_crashed_notice, core_missing_here, core_missing_notice, not_started_notice, not_started_notice_on, port_not_started_notice};
    use crate::testutil::TestDir;

    /// What the player must not be sent to: another program's menus, a log file, a core's file name.
    fn plain(s: &str) {
        for bad in ["RetroArch", "Flatpak", "log.txt", "libretro", ".so", "didn't start", "pocket-core", "Core Downloader", " core"] {
            assert!(!s.contains(bad), "{bad:?} in {s:?}");
        }
    }

    #[test]
    fn a_missing_bios_names_the_files_and_the_folder() {
        let dir = TestDir::new("start-failed-bios");
        let missing = crate::hostsys::find("segacd").unwrap().missing_bios(&dir);
        let notice = not_started_notice("Pocket CD Demo", missing.as_deref());
        assert!(matches!(why("Pocket CD Demo", &notice), Why::Bios { .. }), "{notice}");
        let said = sentence("Pocket CD Demo", &notice);
        assert_eq!(
            said,
            "Pocket CD Demo can't start yet: Sega CD needs a BIOS file (bios_CD_U.bin, bios_CD_E.bin or bios_CD_J.bin). Drop one into roms/bios and try again."
        );
        plain(&said);
        // One file only: "it", not "one of them".
        let one = not_started_notice_on("psx", "Crash", Some("PlayStation needs a BIOS file in roms/bios: scph5501.bin"));
        let said = sentence("Crash", &one);
        assert!(said.contains("(scph5501.bin). Drop it into roms/bios"), "{said}");
    }

    #[test]
    fn a_missing_core_says_the_emulator_is_missing_and_the_game_is_fine() {
        let named = sentence("Metroid", &core_missing_here("Metroid", "mgba"));
        assert!(named.contains("emulator (mGBA) is missing") && named.contains("game is fine") && named.contains("Updates") && !named.contains("core"), "{named}");
        plain(&named);
        let unnamed = sentence("Metroid", &core_missing_notice("Metroid"));
        assert!(unnamed.contains("emulator missing") && unnamed.contains("ROM is fine") && !unnamed.contains("core"), "{unnamed}");
        plain(&unnamed);
    }

    #[test]
    fn a_crash_says_the_emulator_crashed_and_what_to_do() {
        let notice = core_crashed_notice("Metroid");
        assert_eq!(why("Metroid", &notice), Why::Crashed);
        let said = sentence("Metroid", &notice);
        assert!(said.contains("crashed") && said.contains("Try again") && !said.contains("missing"), "{said}");
        plain(&said);
    }

    #[test]
    fn a_damaged_file_says_copy_it_again_and_only_a_system_that_may_need_one_mentions_a_bios() {
        let gb = sentence("Tetris", &not_started_notice_on("gb", "Tetris", None));
        assert_eq!(gb, "Tetris can't start. The ROM file may be corrupt. Recopy it to your SD card.");
        let psx = sentence("Crash", &not_started_notice_on("psx", "Crash", None));
        assert!(psx.contains("corrupt") && psx.contains("Add games") && psx.contains("BIOS"), "{psx}");
        plain(&psx);
        plain(&gb);
    }

    #[test]
    fn a_port_is_told_to_reinstall_without_a_log_file() {
        let said = sentence("SDL Bomber", &port_not_started_notice("SDL Bomber"));
        assert_eq!(said, "SDL Bomber can't start. Check its game card (X) for missing files, or reinstall via Settings › Games › Box art & downloads › PortMaster games.");
    }

    #[test]
    fn what_is_plain_already_stays_as_it_is() {
        for already in [
            "Game.zip holds 2 game files. Unpack it on a computer and keep only the one you want.",
            "Game.zip is damaged: the game inside doesn't match its checksum. Copy the file to the SD card again.",
            "Doom can't start: its file \"Doom.wad\" is no longer there. The list of games is being read again.",
            "Doom can't start: its file is empty (0 bytes), so the copy didn't finish. Copy the game to the card again.",
            "The core couldn't load this game.",
        ] {
            assert_eq!(why("Doom", already), Why::Other, "{already}");
            assert_eq!(sentence("Doom", already), already);
        }
        // Another game's name in front: not read as this one's reason.
        assert_eq!(why("Doom", "Quake didn't start. The file may be damaged: copy the game to the card again."), Why::Other);
    }

    /// Every sentence has its German, Spanish, Portuguese and French line, whole (not one the table takes apart), with the game, the system and the
    /// files in their places.
    #[test]
    fn every_sentence_is_told_in_every_language_too() {
        let notices = [
            ("Metroid", core_missing_here("Metroid", "mgba")),
            ("Metroid", core_missing_notice("Metroid")),
            ("Metroid", core_crashed_notice("Metroid")),
            ("Metroid", not_started_notice_on("gb", "Metroid", None)),
            ("Metroid", not_started_notice_on("psx", "Metroid", None)),
            ("Metroid", not_started_notice_on("segacd", "Metroid", Some("Sega CD needs a BIOS file in roms/bios: a.bin, b.bin or c.bin"))),
            ("Metroid", not_started_notice_on("psx", "Metroid", Some("PlayStation needs a BIOS file in roms/bios: scph5501.bin"))),
            ("Metroid", port_not_started_notice("Metroid")),
        ];
        for (name, notice) in notices {
            let said = sentence(name, &notice);
            for lang in [crate::i18n::Lang::De, crate::i18n::Lang::Es, crate::i18n::Lang::Pt, crate::i18n::Lang::Fr, crate::i18n::Lang::It, crate::i18n::Lang::Ko] {
                let told = lang.table().and_then(|t| t.translate(&said)).unwrap_or_else(|| panic!("{lang:?}: no line for {said:?}"));
                assert!(told.contains("Metroid") && told != said.as_str(), "{lang:?}: {told}");
                assert!(!told.contains("can't start") && !told.contains("Copy ") && !told.contains("{s"), "{lang:?}: left in English: {told}");
            }
        }
        // The files and the system come through as they are.
        let said = sentence("Metroid", &not_started_notice_on("segacd", "Metroid", Some("Sega CD needs a BIOS file in roms/bios: a.bin, b.bin or c.bin")));
        let de = crate::i18n::german().translate(&said).unwrap();
        assert!(de.contains("Sega CD") && de.contains("a.bin, b.bin oder c.bin") || de.contains("a.bin, b.bin") && de.contains("c.bin"), "{de}");
    }

    #[test]
    fn every_sentence_says_what_and_what_to_do() {
        // Whatever the reason, the friendly line starts with the game's name and ends with something to do.
        for notice in [
            core_missing_here("Metroid", "mgba"),
            core_missing_notice("Metroid"),
            core_crashed_notice("Metroid"),
            not_started_notice_on("gb", "Metroid", None),
            not_started_notice_on("psx", "Metroid", None),
            not_started_notice_on("segacd", "Metroid", Some("Sega CD needs a BIOS file in roms/bios: a or b")),
            port_not_started_notice("Metroid"),
        ] {
            let said = sentence("Metroid", &notice);
            assert!(said.starts_with("Metroid can't start"), "{said}");
            assert!(["Copy", "Recopy", "Reinstall", "reinstall via Settings", "Check Settings", "Look for", "try again", "Try again", "add the file"].iter().any(|w| said.contains(w)), "no next step in {said}");
        }
    }
}
