//! What a system is called on a shelf spine, a chip or a row that has little room: the short name people use for the
//! console ("GBA", "N64", "PS3", "Master System"), the same kind of name for every system. Until the polish review
//! (12.11.2026) only a dozen systems had one, so a shelf mixed "GBA, N64, PS1" with "Game Boy Color, Sega Master
//! System, PlayStation 3", and a ROCKNIX name like "Sega Dreamcast (experimental)" was squeezed onto a spine in small
//! letters. The long name and its "(experimental)" stay where there is room: the system's own page and the game card.

/// The short name of the system with id `id` (a folder name of ES-DE and ROCKNIX), if the table has one.
pub fn short(id: &str) -> Option<&'static str> {
    // A console sold under regional names: the name chosen under Look (`region_pairs`, "Mega Drive" or "Genesis").
    if let Some(n) = crate::region_pairs::current(id) {
        return Some(n.short);
    }
    Some(match id {
        "nes" | "famicom" => "NES",
        "fds" => "FDS",
        "snes" | "sfc" | "satellaview" | "sufami" => "SNES",
        "n64" | "n64dd" => "N64",
        "gamecube" | "gc" => "GameCube",
        "wii" => "Wii",
        "wiiu" => "Wii U",
        "gb" => "GB",
        "gbc" => "GBC",
        "gba" => "GBA",
        "nds" | "ds" => "DS",
        "3ds" | "n3ds" => "3DS",
        "virtualboy" => "Virtual Boy",
        "pokemini" => "Pokémon Mini",
        "psx" | "ps1" | "playstation" => "PS1",
        "ps2" => "PS2",
        "ps3" => "PS3",
        "psp" => "PSP",
        "psvita" | "vita" => "Vita",
        "megadrive" | "genesis" | "megadrivejp" => "Mega Drive",
        "mastersystem" | "sms" => "Master System",
        "gamegear" | "gg" => "Game Gear",
        "sg-1000" | "sg1000" => "SG-1000",
        "segacd" | "megacd" | "megacdjp" => "Sega CD",
        "sega32x" | "32x" | "sega32xjp" | "sega32xna" => "32X",
        "saturn" | "saturnjp" => "Saturn",
        "dreamcast" => "Dreamcast",
        "naomi" | "naomi2" => "Naomi",
        "atomiswave" => "Atomiswave",
        "neogeo" => "Neo Geo",
        "arcade" | "mame" | "fbneo" | "cps1" | "cps2" | "cps3" => "Arcade",
        "ngp" => "NGP",
        "ngpc" => "NGPC",
        "pcengine" | "tg16" => "PC Engine",
        "pcenginecd" | "tg-cd" | "tg16cd" => "PC Engine CD",
        "pcfx" => "PC-FX",
        "supergrafx" => "SuperGrafx",
        "wonderswan" => "WonderSwan",
        "wonderswancolor" => "WSC",
        "atarilynx" | "lynx" => "Lynx",
        "atari2600" => "Atari 2600",
        "atari5200" => "Atari 5200",
        "atari7800" => "Atari 7800",
        "atari800" => "Atari 800",
        "atarijaguar" | "jaguar" => "Jaguar",
        "atarist" => "Atari ST",
        "colecovision" => "ColecoVision",
        "intellivision" => "Intellivision",
        "vectrex" => "Vectrex",
        "odyssey2" | "videopac" => "Odyssey 2",
        "channelf" => "Channel F",
        "3do" => "3DO",
        "cdi" => "CD-i",
        "xbox" => "Xbox",
        "xbox360" => "Xbox 360",
        "amiga" | "amiga600" | "amiga1200" => "Amiga",
        "amigacd32" => "CD32",
        "c64" => "C64",
        "zxspectrum" => "ZX Spectrum",
        "msx" | "msx1" | "msx2" | "msx2+" | "msxturbor" => "MSX",
        "amstradcpc" => "Amstrad CPC",
        "dos" | "pc" => "DOS",
        "scummvm" => "ScummVM",
        "pico8" | "pico-8" => "PICO-8",
        "j2me" => "J2ME",
        "tic80" => "TIC-80",
        "ports" => "Ports",
        "openbor" => "OpenBOR",
        "lutro" => "Lutro",
        "moonlight" | "streaming" => "Moonlight",
        _ => return None,
    })
}

/// A system's own name for a place with room, without what is only for the card: "Sega Dreamcast (experimental)" is
/// "Sega Dreamcast". A name with no brackets is returned as it is.
pub fn without_note(fullname: &str) -> String {
    match fullname.find(" (") {
        Some(at) if fullname.ends_with(')') => fullname[..at].trim_end().to_string(),
        _ => fullname.to_string(),
    }
}

/// pocket-ui's own long name for a system whose ES name says little to a player (ROCKNIX's bare "J2ME"): the system's
/// page and the game card show it, in the player's language (`library`).
pub fn own_long(id: &str) -> Option<&'static str> {
    match id {
        "j2me" => Some("Java phone games (J2ME)"),
        _ => None,
    }
}

/// The name for a spine: the table's short name; else the system's own name without its note and, for a long one, without
/// the maker in front ("Sega Dreamcast" is "Dreamcast", but never "Atari 8-bit family" down to "8-bit family": what is left
/// must name a console, so a rest that starts with a digit keeps the maker); else its id in capitals when it is a short id.
pub fn spine_name(id: &str, fullname: &str) -> String {
    if let Some(s) = short(id) {
        return s.to_string();
    }
    let name = without_note(fullname);
    for maker in ["Sega ", "Sony ", "Nintendo ", "Microsoft ", "Atari ", "NEC ", "SNK "] {
        if let Some(rest) = name.strip_prefix(maker).filter(|r| !r.is_empty() && !r.starts_with(|c: char| c.is_ascii_digit()) && name.chars().count() > 12) {
            return rest.to_string();
        }
    }
    name
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phone_games_are_named_for_players_not_by_their_folder() {
        assert_eq!(spine_name("j2me", "j2me"), "J2ME", "never the folder's lower-case id");
        assert_eq!(own_long("j2me"), Some("Java phone games (J2ME)"));
        assert_eq!(own_long("gba"), None, "everything else keeps ES's name");
    }

    /// The whole class: every system pocket-ui knows by id has a short name, a spine never shows a note in brackets or
    /// the maker's long form, and the names are of one kind (a few letters or a word or two, none long).
    #[test]
    fn every_known_system_has_one_kind_of_short_name() {
        for id in crate::document_library::SYSTEM_IDS {
            let short = short(id).unwrap_or_else(|| panic!("{id} has no short name"));
            let full = crate::document_library::system_name(id);
            assert_ne!(full, id, "{id}: the long name is a name, not the folder");
            assert!(short.chars().count() <= 14, "{id}: {short}");
            assert!(!short.contains('('), "{id}: {short}");
            assert_eq!(spine_name(id, &format!("{full} (experimental)")), short, "{id}: a note never decides the spine");
        }
        // The shelf the review saw: short and long names were mixed.
        for (id, full, want) in [("gbc", "Game Boy Color", "GBC"), ("mastersystem", "Sega Master System", "Master System"), ("ps3", "PlayStation 3", "PS3"), ("3ds", "Nintendo 3DS", "3DS"), ("dreamcast", "Sega Dreamcast (experimental)", "Dreamcast")] {
            assert_eq!(spine_name(id, full), want);
        }
    }

    #[test]
    fn a_note_in_brackets_is_for_the_card_not_the_spine() {
        assert_eq!(without_note("Sega Dreamcast (experimental)"), "Sega Dreamcast");
        assert_eq!(without_note("Super Nintendo"), "Super Nintendo");
        assert_eq!(spine_name("somethingnew", "Sega Something Rather Long (experimental)"), "Something Rather Long");
        assert_eq!(spine_name("dreamcast", "Sega Dreamcast (experimental)"), "Dreamcast");
        assert_eq!(spine_name("gbc", "Game Boy Color"), "GBC");
        assert_eq!(spine_name("mastersystem", "Sega Master System"), "Master System");
    }

    /// Red before 13.11.: the Atari 800's title card said "8-bit family" (the maker was cut off a name that is not a name
    /// without it). Whatever the table lacks keeps a rest that starts with a digit whole.
    #[test]
    fn a_name_is_never_cut_down_to_a_number_or_a_family() {
        assert_eq!(spine_name("atari800", "Atari 8-bit family"), "Atari 800");
        assert_eq!(spine_name("somethingnew", "Atari 8-bit family"), "Atari 8-bit family");
        assert_eq!(spine_name("somethingnew", "Sega 32X Add-on Deck"), "Sega 32X Add-on Deck");
    }
}
