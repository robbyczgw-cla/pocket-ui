//! Which RetroAchievements console a ROCKNIX system is (`RC_CONSOLE_*` in vendor/rcheevos/include/rc_consoles.h).

/// The RetroAchievements console id of a ROCKNIX system name (es_systems.cfg), None for systems without achievements.
pub fn console_id(system: &str) -> Option<u32> {
    Some(match system {
        "megadrive" | "megadrive-japan" | "genesis" | "genh" | "megadriveh" => 1,
        "n64" => 2,
        "snes" | "sfc" | "snesh" | "sneshd" | "satellaview" | "sufami" => 3,
        "gb" | "gbh" => 4,
        "gba" | "gbah" | "gbav" => 5,
        "gbc" | "gbch" => 6,
        "nes" | "famicom" | "nesh" => 7,
        // RetroAchievements files SuperGrafx games under the PC Engine.
        "pcengine" | "tg16" | "supergrafx" => 8,
        "segacd" | "megacd" => 9,
        "sega32x" => 10,
        "mastersystem" => 11,
        "psx" => 12,
        "atarilynx" => 13,
        "ngp" | "ngpc" => 14,
        "gamegear" | "ggh" => 15,
        "gamecube" => 16,
        "atarijaguar" => 17,
        "nds" => 18,
        "wii" => 19,
        "ps2" => 21,
        "videopac" | "odyssey2" => 23,
        "pokemini" => 24,
        "atari2600" => 25,
        "pc" => 26,
        // Arcade: by the romset's file name; NAOMI and Atomiswave games are MAME romsets too.
        "arcade" | "fbn" | "fbneo" | "neogeo" | "cps1" | "cps2" | "cps3" | "mame" | "naomi" | "atomiswave" => 27,
        "virtualboy" => 28,
        "msx" | "msx1" | "msx2" => 29,
        "sg-1000" => 33,
        "saturn" => 39,
        "dreamcast" => 40,
        "psp" => 41,
        "3do" => 43,
        "colecovision" => 44,
        "intellivision" => 45,
        "vectrex" => 46,
        "pc-8800" | "pc88" => 47,
        "pcfx" => 49,
        "atari5200" => 50,
        "atari7800" => 51,
        "wonderswan" | "wonderswancolor" => 53,
        "neocd" | "neogeocd" => 56,
        "channelf" => 57,
        "zxspectrum" => 59,
        "gameandwatch" => 60,
        "supervision" => 63,
        "tic-80" => 65,
        "megaduck" => 69,
        "arduboy" => 71,
        "wasm4" => 72,
        "arcadia" => 73,
        "pcenginecd" | "tg16cd" => 76,
        "jaguarcd" => 77,
        "uzebox" => 80,
        "fds" => 81,
        _ => return None,
    })
}

/// Consoles RetroAchievements identifies by the file name rather than the contents (arcade sets): their zip is hashed
/// as it is, never unpacked.
pub fn hashes_file_name(console: u32) -> bool {
    console == 27
}

/// Consoles whose zip files are the game itself (MS-DOS: rcheevos hashes the files inside).
pub fn hashes_zip(console: u32) -> bool {
    console == 26
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_system_pocket_core_hosts_has_its_console() {
        for (system, id) in [
            ("gb", 4),
            ("gbc", 6),
            ("gba", 5),
            ("nes", 7),
            ("snes", 3),
            ("megadrive", 1),
            ("psx", 12),
            ("n64", 2),
            ("nds", 18),
            ("psp", 41),
        ] {
            assert_eq!(console_id(system), Some(id), "{system}");
        }
        for h in crate::hostsys::SYSTEMS {
            for n in h.names {
                // PICO-8 and ScummVM have no achievements on RetroAchievements.
                if !matches!(*n, "pico-8" | "scummvm") {
                    assert!(console_id(n).is_some(), "{n} is hosted by pocket-core but has no console id");
                }
            }
        }
        assert_eq!(console_id("pico-8"), None);
        assert!(hashes_file_name(27) && !hashes_file_name(7));
    }
}
