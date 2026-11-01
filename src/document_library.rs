//! A document-provider library. Identity comes from the provider, names only choose a system and presentation.
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    path::Path,
};

#[derive(Clone, Deserialize, Serialize)]
pub struct Document {
    pub uri: String,
    pub id: String,
    pub authority: String,
    pub relative: String,
    #[serde(default)]
    pub sheet: String,
    /// As its provider says; negative when it doesn't.
    #[serde(default = "unknown_size")]
    pub size: i64,
}
fn unknown_size() -> i64 {
    -1
}
impl Document {
    /// Tree-independent identity survives rescans and renames supported by the provider.
    pub fn key(&self) -> String {
        format!("{}:{}", self.authority, self.id)
    }
}
#[derive(Clone)]
pub struct Game {
    pub name: String,
    pub key: String,
    pub uri: String,
    pub relative: String,
    pub system: &'static str,
    pub system_id: &'static str,
    pub profile: &'static str,
    pub color: u32,
    pub missing: Vec<String>,
    /// The document's size (`Document::size`).
    pub size: i64,
}

/// Common ES-DE and Daijisho aliases. An ancestor system wins over a generic disc/archive extension.
pub fn system(id: &str) -> Option<(&'static str, &'static str, &'static str, u32)> {
    Some(match id {
        "gb" | "gameboy" => ("gb", "Game Boy", "retroarch:gambatte", 0x80a878),
        "gbc" | "gameboycolor" => ("gbc", "Game Boy Color", "retroarch:gambatte", 0x80a878),
        "gba" | "gameboyadvance" => ("gba", "Game Boy Advance", "retroarch:mgba", 0xa49bc7),
        "nes" | "famicom" => ("nes", "Nintendo Entertainment System", "retroarch:nestopia", 0xc97468),
        "sfc" | "smc" | "snes" | "snesna" => ("snes", "Super Nintendo", "retroarch:bsnes", 0xb8a6cc),
        "sms" | "mastersystem" => ("mastersystem", "Master System", "retroarch:gearsystem", 0x7ca9c1),
        "gg" | "gamegear" => ("gamegear", "Game Gear", "retroarch:gearsystem", 0x7ca9c1),
        "md" | "gen" | "genesis" | "megadrive" => ("megadrive", "Mega Drive", "retroarch:clownmdemu", 0x6999bf),
        "z64" | "n64" | "v64" => ("n64", "Nintendo 64", "retroarch:mupen64plus_next", 0xa49bc7),
        "nds" => ("nds", "Nintendo DS", "retroarch:melonds", 0xa49bc7),
        "ps2" => ("ps2", "PlayStation 2", "ps2", 0x6999bf),
        "psp" | "cso" | "pbp" => ("psp", "PlayStation Portable", "psp", 0x6999bf),
        "psx" | "ps1" => ("psx", "PlayStation", "psx", 0x6999bf),
        "rvz" | "gcm" | "wii" | "gc" | "gamecube" => ("gamecube", "GameCube / Wii", "dolphin", 0xa49bc7),
        "3ds" | "n3ds" | "cci" | "cxi" => ("3ds", "Nintendo 3DS", "3ds", 0xa49bc7),
        "vita" | "psvita" => ("psvita", "PlayStation Vita", "vita", 0x6999bf),
        "nsp" | "xci" | "switch" => ("switch", "Nintendo Switch", "switch", 0xc97468),
        "saturn" => ("saturn", "Sega Saturn", "retroarch:yabause", 0x6999bf),
        "dreamcast" => ("dreamcast", "Dreamcast", "retroarch:flycast", 0xb8a6cc),
        "segacd" | "megacd" => ("segacd", "Mega CD", "retroarch:clownmdemu", 0x6999bf),
        "pcengine" | "pce" => ("pcengine", "PC Engine", "retroarch:mednafen_pce_fast", 0xc97468),
        "ws" | "wsc" | "wswan" | "wonderswan" => ("wonderswan", "WonderSwan", "retroarch:mednafen_wswan", 0xa49bc7),
        "wonderswancolor" | "wswanc" => ("wonderswancolor", "WonderSwan Color", "retroarch:mednafen_wswan", 0xa49bc7),
        "ngp" | "neogeopocket" => ("ngp", "Neo Geo Pocket", "retroarch:mednafen_ngp", 0x80a878),
        "ngc" | "ngpc" | "neogeopocketcolor" => ("ngpc", "Neo Geo Pocket Color", "retroarch:mednafen_ngp", 0x80a878),
        "lnx" | "lynx" | "atarilynx" => ("atarilynx", "Atari Lynx", "retroarch:handy", 0xc97468),
        "a26" | "atari2600" => ("atari2600", "Atari 2600", "retroarch:stella", 0xc97468),
        "neogeo" | "arcade" | "fbneo" => ("neogeo", "Neo Geo / Arcade", "retroarch:mame", 0x80a878),
        _ => return None,
    })
}
/// The canonical id of every system above.
pub const SYSTEM_IDS: [&str; 28] = [
    "gb", "gbc", "gba", "nes", "snes", "mastersystem", "gamegear", "megadrive", "n64", "nds", "ps2", "psp", "psx", "gamecube", "3ds", "psvita", "switch", "saturn", "dreamcast", "segacd", "pcengine", "wonderswan",
    "wonderswancolor", "ngp", "ngpc", "atarilynx", "atari2600", "neogeo",
];
/// The file types a system's players open, as far as the shelf lists them (`shelf_type`): its bundled core's or its
/// emulator app's. Used by the Game check: another type in its folder won't start.
pub fn takes(id: &str) -> &'static [&'static str] {
    match id {
        "gb" | "gbc" => &["gb", "gbc", "zip"],
        "gba" => &["gba", "gb", "gbc", "zip"],
        "nes" => &["nes", "zip"],
        "snes" => &["sfc", "smc", "zip"],
        "mastersystem" | "gamegear" => &["sms", "gg", "bin", "zip"],
        "megadrive" => &["md", "gen", "bin", "zip"],
        "n64" => &["z64", "n64", "v64", "zip"],
        "nds" => &["nds", "zip"],
        "ps2" => &["iso", "chd", "cso", "bin", "m3u"],
        "psp" => &["iso", "cso", "pbp"],
        "psx" => &["cue", "chd", "pbp", "iso", "bin", "m3u"],
        "gamecube" => &["rvz", "gcm", "iso"],
        "3ds" => &["3ds", "cci", "cxi"],
        "psvita" => &["vpk"],
        "switch" => &["nsp", "xci"],
        "saturn" => &["cue", "chd", "iso", "m3u"],
        "dreamcast" => &["gdi", "cue", "chd", "m3u"],
        "segacd" => &["cue", "chd", "iso", "bin", "m3u"],
        "pcengine" => &["pce", "cue", "chd", "zip"],
        "wonderswan" | "wonderswancolor" => &["ws", "wsc", "zip"],
        "ngp" | "ngpc" => &["ngp", "ngc", "ngpc", "zip"],
        "atarilynx" => &["lnx", "zip"],
        "atari2600" => &["a26", "bin", "zip"],
        "neogeo" => &["zip"],
        _ => &[],
    }
}
/// A file type the shelf lists at all.
pub fn shelf_type(ext: &str) -> bool {
    matches!(ext, "gb" | "gbc" | "gba" | "nes" | "sfc" | "smc" | "sms" | "gg" | "md" | "gen" | "z64" | "n64" | "v64" | "nds" | "zip" | "iso" | "chd" | "cue" | "gdi" | "m3u" | "pbp" | "cso" | "rvz" | "gcm" | "3ds" | "cci" | "cxi" | "nsp" | "xci" | "vpk" | "ngp" | "ngc" | "ngpc" | "lnx" | "pce" | "bin" | "ws" | "wsc" | "a26")
}
fn extension(path: &str) -> String {
    Path::new(path).extension().unwrap_or_default().to_string_lossy().to_ascii_lowercase()
}
fn reference(sheet: &str, file: &str) -> Option<String> {
    let mut parts: Vec<&str> = sheet.rsplit_once('/').map_or(Vec::new(), |(p, _)| p.split('/').collect());
    if file.starts_with('/') || file.contains(':') {
        return None;
    }
    let file = file.replace('\\', "/");
    for part in file.split('/') {
        match part {
            "" | "." => (),
            ".." => {
                parts.pop()?;
            }
            s => parts.push(s),
        }
    }
    Some(parts.join("/"))
}
/// Every file a disc sheet (m3u, cue, gdi) names: the sheet, the file as the sheet writes it, and where that file is among
/// `documents` (None: it is not there).
pub fn sheet_files(documents: &[Document]) -> Vec<(String, String, Option<String>)> {
    let paths: HashSet<_> = documents.iter().map(|d| d.relative.as_str()).collect();
    let mut out = Vec::new();
    for d in documents {
        for file in crate::library_names::sheet_references(&extension(&d.relative), &d.sheet) {
            let path = reference(&d.relative, &file).filter(|path| paths.contains(path.as_str()));
            out.push((d.relative.clone(), file, path));
        }
    }
    out
}
/// A folder called bios holds a console's BIOS files, none of them a game (a PC Engine card's `syscard3.pce` looks like one).
pub fn in_bios_folder(relative: &str) -> bool {
    Path::new(relative).parent().is_some_and(|p| p.iter().any(|c| c.eq_ignore_ascii_case("bios")))
}
/// The system a game's folders name; the nearest wins.
pub fn folder_system(relative: &str) -> Option<(&'static str, &'static str, &'static str, u32)> {
    Path::new(relative).parent().and_then(|p| p.ancestors().filter_map(|p| p.file_name()?.to_str()).find_map(|s| system(&s.to_ascii_lowercase())))
}
/// Sheets are the launchable game; referenced discs/tracks are hidden, missing dependencies remain visible as advice.
pub fn scan(documents: Vec<Document>) -> Vec<Game> {
    let mut dependencies = HashSet::new();
    let mut missing = HashMap::<String, Vec<String>>::new();
    for (sheet, file, path) in sheet_files(&documents) {
        match path {
            Some(path) => {
                dependencies.insert(path);
            }
            None => missing.entry(sheet).or_default().push(file),
        }
    }
    let mut seen = HashSet::new();
    let mut games = Vec::new();
    for d in documents {
        let key = d.key();
        if !seen.insert(key.clone()) || dependencies.contains(&d.relative) || in_bios_folder(&d.relative) {
            continue;
        }
        let ext = extension(&d.relative);
        if !shelf_type(&ext) {
            continue;
        }
        let Some((system_id, system, profile, color)) = folder_system(&d.relative).or_else(|| system(&ext)) else {
            continue;
        };
        let stem = Path::new(&d.relative).file_stem().unwrap_or_default().to_string_lossy();
        let name = crate::library_names::title_case(&crate::library_names::display_name(&stem.replace('_', " ")));
        games.push(Game {
            name,
            key,
            uri: d.uri,
            size: d.size,
            missing: missing.remove(&d.relative).unwrap_or_default(),
            relative: d.relative,
            system,
            system_id,
            profile,
            color,
        });
    }
    games.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()).then(a.key.cmp(&b.key)));
    games
}
/// What the shelf holds that a new scan could change: each game with the disc files it lacks.
pub fn shelf_state<'a>(games: impl Iterator<Item = &'a Game>) -> HashMap<String, Vec<String>> {
    games.map(|g| (g.key.clone(), g.missing.clone())).collect()
}

/// Whether a scan has nothing new for the shelf. A track copied in adds no game, but Play goes by `missing`.
pub fn same_shelf(known: &HashMap<String, Vec<String>>, games: &[Game]) -> bool {
    games.len() == known.len() && games.iter().all(|g| known.get(&g.key) == Some(&g.missing))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn doc(id: &str, path: &str, sheet: &str) -> Document {
        Document {
            id: id.into(),
            authority: "provider".into(),
            uri: format!("content://provider/tree/x/document/{id}"),
            size: -1,
            relative: path.into(),
            sheet: sheet.into(),
        }
    }
    #[test]
    fn nested_sets_hide_dependencies_and_keep_missing_advice() {
        let games = scan(vec![
            doc("a", "roms/psx/RPG/Set.m3u", "Disc 1.cue\nDisc 2.chd"),
            doc("b", "roms/psx/RPG/Disc 1.cue", "FILE \"Track.bin\" BINARY"),
            doc("c", "roms/psx/RPG/Track.bin", ""),
            doc("d", "roms/psx/RPG/Disc 2.chd", ""),
        ]);
        assert_eq!(games.len(), 1);
        assert_eq!(games[0].profile, "psx");
        assert!(games[0].missing.is_empty());
        let games = scan(vec![doc("a", "psx/Set.m3u", "../../escape.chd\nmissing.cue")]);
        assert_eq!(games[0].missing.len(), 2);
    }
    #[test]
    fn a_repaired_disc_is_news_for_the_shelf_though_no_game_came() {
        let sheet = doc("a", "psx/Game.cue", "FILE \"Track.bin\" BINARY");
        let before = scan(vec![sheet.clone()]);
        assert_eq!(before[0].missing, ["Track.bin"]);
        let known = shelf_state(before.iter());
        let after = scan(vec![sheet.clone(), doc("b", "psx/Track.bin", "")]);
        assert_eq!(after.len(), 1);
        assert!(after[0].missing.is_empty());
        assert!(!same_shelf(&known, &after), "Play would stay blocked on the old list");
        assert!(same_shelf(&known, &before));
        assert!(same_shelf(&shelf_state(after.iter()), &after));
        // The other way round too: a track that went blocks Play instead of failing in the core.
        assert!(!same_shelf(&shelf_state(after.iter()), &before));
    }
    #[test]
    fn a_bios_folder_holds_no_games() {
        let games = scan(vec![doc("a", "roms/bios/syscard3.pce", ""), doc("b", "roms/pcengine/Game.pce", ""), doc("c", "Biosphere/Game.pce", "")]);
        let mut left: Vec<_> = games.iter().map(|g| g.relative.as_str()).collect();
        left.sort();
        assert_eq!(left, ["Biosphere/Game.pce", "roms/pcengine/Game.pce"]);
    }
    #[test]
    fn every_system_has_its_id_listed_and_takes_what_the_shelf_lists() {
        for id in SYSTEM_IDS {
            assert_eq!(system(id).map(|s| s.0), Some(id));
            assert!(!takes(id).is_empty(), "{id}");
            for ext in takes(id) {
                assert!(shelf_type(ext), "{id} takes .{ext}, which the shelf doesn't list");
            }
        }
    }
    #[test]
    fn provider_identity_survives_rename_and_disambiguates_providers() {
        let a = scan(vec![doc("opaque-id", "sms/Game.sms", "")]);
        let b = scan(vec![doc("opaque-id", "mastersystem/Renamed.sms", "")]);
        assert_eq!(a[0].key, b[0].key);
        assert_ne!(a[0].name, b[0].name);
    }
}
