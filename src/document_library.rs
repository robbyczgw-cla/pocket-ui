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
/// Sheets are the launchable game; referenced discs/tracks are hidden, missing dependencies remain visible as advice.
pub fn scan(documents: Vec<Document>) -> Vec<Game> {
    let paths: HashSet<_> = documents.iter().map(|d| d.relative.as_str()).collect();
    let mut dependencies = HashSet::new();
    let mut missing = HashMap::<String, Vec<String>>::new();
    for d in &documents {
        for file in crate::library_names::sheet_references(&extension(&d.relative), &d.sheet) {
            if let Some(path) = reference(&d.relative, &file) {
                if paths.contains(path.as_str()) {
                    dependencies.insert(path);
                } else {
                    missing.entry(d.relative.clone()).or_default().push(file);
                }
            } else {
                missing.entry(d.relative.clone()).or_default().push(file);
            }
        }
    }
    let mut seen = HashSet::new();
    let mut games = Vec::new();
    for d in documents {
        let key = d.key();
        if !seen.insert(key.clone()) || dependencies.contains(&d.relative) {
            continue;
        }
        let ext = extension(&d.relative);
        if !matches!(
            ext.as_str(),
            "gb" | "gbc"
                | "gba"
                | "nes"
                | "sfc"
                | "smc"
                | "sms"
                | "gg"
                | "md"
                | "gen"
                | "z64"
                | "n64"
                | "v64"
                | "nds"
                | "zip"
                | "iso"
                | "chd"
                | "cue"
                | "gdi"
                | "m3u"
                | "pbp"
                | "cso"
                | "rvz"
                | "gcm"
                | "3ds"
                | "cci"
                | "cxi"
                | "nsp"
                | "xci"
                | "vpk"
                | "ngp"
                | "ngc"
                | "ngpc"
                | "lnx"
                | "pce"
                | "bin"
                | "ws"
                | "wsc"
                | "a26"
        ) {
            continue;
        }
        let folder = Path::new(&d.relative).parent().and_then(|p| {
            p.ancestors()
                .filter_map(|p| p.file_name()?.to_str())
                .find_map(|s| system(&s.to_ascii_lowercase()))
        });
        let Some((system_id, system, profile, color)) = folder.or_else(|| system(&ext)) else {
            continue;
        };
        let stem = Path::new(&d.relative).file_stem().unwrap_or_default().to_string_lossy();
        let name = crate::library_names::title_case(&crate::library_names::display_name(&stem.replace('_', " ")));
        games.push(Game {
            name,
            key,
            uri: d.uri,
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
#[cfg(test)]
mod tests {
    use super::*;
    fn doc(id: &str, path: &str, sheet: &str) -> Document {
        Document {
            id: id.into(),
            authority: "provider".into(),
            uri: format!("content://provider/tree/x/document/{id}"),
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
    fn provider_identity_survives_rename_and_disambiguates_providers() {
        let a = scan(vec![doc("opaque-id", "sms/Game.sms", "")]);
        let b = scan(vec![doc("opaque-id", "mastersystem/Renamed.sms", "")]);
        assert_eq!(a[0].key, b[0].key);
        assert_ne!(a[0].name, b[0].name);
    }
}
