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
    /// Display name of the folder the player selected; paths below it need not repeat a system name.
    #[serde(default)]
    pub root_name: String,
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
    /// A nearer system folder wins; otherwise the chosen root supplies the same alias-aware context.
    pub fn folder_system(&self) -> Option<(&'static str, &'static str, &'static str, u32)> {
        folder_system(&self.relative).or_else(|| system(&self.root_name))
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
    /// Total bytes of the entry and its known parts, for space checks; -1 if any size is unknown.
    pub size: i64,
    /// Only the selected entry's bytes (`Document::size`), matching `frozen::Mark::rom_bytes`.
    pub document_size: i64,
    /// Read-only provider pictures, best cover first. Never filesystem paths.
    pub covers: Vec<String>,
    pub pictures: Vec<String>,
    /// A same-name .cht beside this game, read on the scan worker.
    pub cheats: String,
    /// A disc sheet's files (its tracks, a playlist's discs and theirs), each once: what the start copies beside it.
    /// Empty for a game of one file.
    pub parts: Vec<Part>,
}

/// A file a game's sheet names: its document, and where it goes beside the sheet's copy (the name as the sheet writes it,
/// from the sheet's folder, `/` between folders). A name that leaves that folder starts with `../`: the start refuses it.
#[derive(Clone, Debug, PartialEq)]
pub struct Part {
    pub uri: String,
    pub name: String,
}

/// Large disc systems exempt from the 512 MiB cartridge cap: their loaders stream the game's path. Other systems
/// retain `document_check::MAX_IN_MEMORY` as a product cap, even when their core also sets `need_fullpath`.
/// `GameCopy.PLAYED_FROM_FILE` names these cores: change them together.
pub fn plays_from_its_file(system: &str) -> bool {
    matches!(system, "psp" | "psx" | "saturn" | "dreamcast")
}

/// The size of a PlayStation BIOS file.
pub const PS1_BIOS_BYTES: i64 = 512 << 10;
/// A PlayStation BIOS by its usual name (`scph5501.bin`, PSP's `psxonpsp660.bin`), in any letters. Never a game.
pub fn is_ps1_bios_name(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    let Some(stem) = name.strip_suffix(".bin") else { return false };
    if stem == "psxonpsp660" {
        return true;
    }
    let Some(code) = stem.strip_prefix("scph") else { return false };
    let digits = code.trim_end_matches(|c: char| c.is_ascii_lowercase());
    (3..=5).contains(&digits.len()) && digits.bytes().all(|b| b.is_ascii_digit()) && code.len() - digits.len() <= 1
}
/// Whether the player's private BIOS folder holds a PlayStation BIOS (`PocketActivity.copyBios` fills it).
pub fn has_ps1_bios(folder: &Path) -> bool {
    std::fs::read_dir(folder).is_ok_and(|entries| {
        entries.flatten().any(|e| is_ps1_bios_name(&e.file_name().to_string_lossy()) && e.metadata().is_ok_and(|m| m.is_file() && m.len() == PS1_BIOS_BYTES as u64))
    })
}
/// Missing-BIOS guidance before a start. Saturn requires one; PS1 and Dreamcast can use their built-in replacements.
/// The copy worker checks real lengths and readability again, before the core starts.
pub fn bios_hint(system: &str, among_folders: &[Part], kept: &Path) -> Option<&'static str> {
    let found = among_folders.iter().any(|p| bios_file(&p.name).is_some_and(|b| b.0 == system && (system != "dreamcast" || b.1 == "dc/dc_boot.bin")));
    let kept = match system {
        "psx" => has_ps1_bios(kept),
        "saturn" => ["saturn_bios.bin", "sega_101.bin", "mpr-17933.bin"].iter().any(|n| kept.join(n).metadata().is_ok_and(|m| m.is_file() && m.len() == PS1_BIOS_BYTES as u64)),
        "dreamcast" => kept.join("dc/dc_boot.bin").metadata().is_ok_and(|m| m.is_file() && m.len() == 2 << 20),
        _ => return None,
    };
    if found || kept { return None; }
    match system {
        "psx" => Some(PS1_BIOS_MISSING),
        "saturn" => Some(SATURN_BIOS_MISSING),
        "dreamcast" => Some(DREAMCAST_BIOS_MISSING),
        _ => None,
    }
}
pub const PS1_BIOS_MISSING: &str = "No PlayStation BIOS. If a game hangs, put one in a folder named bios.";
/// The PlayStation BIOS files among the chosen folders, in any folder there, the size of one: what the player's private
/// BIOS folder gets before a PlayStation game starts. Named in lower case, as PCSX ReARMed looks for them first; the
/// first of a name wins.
pub fn ps1_bios(documents: &[Document]) -> Vec<Part> {
    let mut out: Vec<Part> = Vec::new();
    for d in documents {
        let name = d.relative.rsplit('/').next().unwrap_or_default().to_ascii_lowercase();
        if is_ps1_bios_name(&name) && (d.size == PS1_BIOS_BYTES || d.size < 0) && !out.iter().any(|p| p.name == name) {
            out.push(Part { uri: d.uri.clone(), name });
        }
    }
    out
}

/// A BIOS accepted by our bundled players: system, private relative name and exact length. No ROMs are bundled.
/// Names are matched without case; Dreamcast's files go in Flycast's `dc/` system subfolder.
pub fn bios_file(name: &str) -> Option<(&'static str, String, i64)> {
    let name = name.to_ascii_lowercase();
    let leaf = name.strip_prefix("dc/").unwrap_or(&name);
    let (system, dest, size) = match leaf {
        "saturn_bios.bin" | "sega_101.bin" | "mpr-17933.bin" => ("saturn", leaf.to_string(), 512 << 10),
        "dc_boot.bin" | "dc_bios.bin" => ("dreamcast", "dc/dc_boot.bin".into(), 2 << 20),
        "dc_flash.bin" => ("dreamcast", "dc/dc_flash.bin".into(), 128 << 10),
        _ if is_ps1_bios_name(&name) => ("psx", name, PS1_BIOS_BYTES),
        _ => return None,
    };
    Some((system, dest, size))
}
pub const SATURN_BIOS_MISSING: &str = "Saturn needs a BIOS. Put saturn_bios.bin, sega_101.bin or mpr-17933.bin in a selected folder, then run Game check.";
pub const DREAMCAST_BIOS_MISSING: &str = "No Dreamcast BIOS. Flycast uses its built-in replacement. For better compatibility, add dc_boot.bin and dc_flash.bin to a selected bios folder.";
/// All supported BIOS files among the granted folders. Metadata of an unknown size is checked again while copying.
pub fn player_bios(documents: &[Document]) -> Vec<Part> {
    let mut out: Vec<Part> = Vec::new();
    for d in documents {
        let Some((_, name, size)) = bios_file(d.relative.rsplit('/').next().unwrap_or_default()) else { continue };
        if (d.size == size || d.size < 0) && !out.iter().any(|p| p.name == name) {
            out.push(Part { uri: d.uri.clone(), name });
        }
    }
    out
}

/// Common ES-DE and Daijisho aliases. An ancestor system wins over a generic disc/archive extension.
pub fn system(id: &str) -> Option<(&'static str, &'static str, &'static str, u32)> {
    let normalized = id.to_ascii_lowercase();
    let id = normalized.as_str();
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
        "3ds" | "n3ds" | "cci" | "cxi" | "3dsx" => ("3ds", "Nintendo 3DS", "3ds", 0xa49bc7),
        // No file type of its own: an Xbox disc is an `.iso` (or `.xiso.iso`) in a folder named for it.
        "xbox" => ("xbox", "Xbox", "xbox", 0x80a878),
        "vita" | "psvita" => ("psvita", "PlayStation Vita", "vita", 0x6999bf),
        "saturn" => ("saturn", "Sega Saturn", "retroarch:yabause", 0x6999bf),
        "dreamcast" | "cdi" => ("dreamcast", "Dreamcast", "retroarch:flycast", 0xb8a6cc),
        "segacd" | "megacd" => ("segacd", "Mega CD", "retroarch:clownmdemu", 0x6999bf),
        "pcengine" | "pce" => ("pcengine", "PC Engine", "retroarch:mednafen_pce_fast", 0xc97468),
        "ws" | "wsc" | "wswan" | "wonderswan" => ("wonderswan", "WonderSwan", "retroarch:mednafen_wswan", 0xa49bc7),
        "wonderswancolor" | "wswanc" => ("wonderswancolor", "WonderSwan Color", "retroarch:mednafen_wswan", 0xa49bc7),
        "ngp" | "neogeopocket" => ("ngp", "Neo Geo Pocket", "retroarch:mednafen_ngp", 0x80a878),
        "ngc" | "ngpc" | "neogeopocketcolor" => ("ngpc", "Neo Geo Pocket Color", "retroarch:mednafen_ngp", 0x80a878),
        "lnx" | "lynx" | "atarilynx" => ("atarilynx", "Atari Lynx", "retroarch:handy", 0xc97468),
        "a26" | "atari2600" => ("atari2600", "Atari 2600", "retroarch:stella", 0xc97468),
        "neogeo" | "arcade" | "fbneo" => ("neogeo", "Neo Geo / Arcade", "retroarch:mame", 0x80a878),
        _ => {
            let id = crate::layouts::system_ids(id).into_iter().find(|alias| *alias != id && SYSTEM_IDS.contains(alias))?;
            return system(id);
        },
    })
}
/// The canonical id of every system above.
pub const SYSTEM_IDS: [&str; 28] = [
    "gb", "gbc", "gba", "nes", "snes", "mastersystem", "gamegear", "megadrive", "n64", "nds", "ps2", "psp", "psx", "gamecube", "3ds", "psvita", "saturn", "dreamcast", "segacd", "pcengine", "wonderswan",
    "wonderswancolor", "ngp", "ngpc", "atarilynx", "atari2600", "neogeo", "xbox",
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
        "psp" => &["iso", "cso", "pbp", "chd"],
        "psx" => &["cue", "chd", "pbp", "iso", "bin", "m3u"],
        "gamecube" => &["rvz", "gcm", "iso"],
        "3ds" => &["3ds", "cci", "cxi", "3dsx"],
        "xbox" => &["iso"],
        "psvita" => &["vpk"],
        "saturn" => &["cue", "chd", "iso", "m3u"],
        "dreamcast" => &["gdi", "cue", "chd", "cdi", "m3u"],
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
    matches!(ext, "gb" | "gbc" | "gba" | "nes" | "sfc" | "smc" | "sms" | "gg" | "md" | "gen" | "z64" | "n64" | "v64" | "nds" | "zip" | "iso" | "chd" | "cue" | "gdi" | "cdi" | "m3u" | "pbp" | "cso" | "rvz" | "gcm" | "3ds" | "cci" | "cxi" | "3dsx" | "vpk" | "ngp" | "ngc" | "ngpc" | "lnx" | "pce" | "bin" | "ws" | "wsc" | "a26")
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
/// Where the files are: by their path, and by their path in lower case when only one file has it.
struct Paths<'a> {
    exact: HashSet<&'a str>,
    folded: HashMap<String, Option<&'a str>>,
}
impl<'a> Paths<'a> {
    fn new(documents: &'a [Document]) -> Self {
        let mut folded = HashMap::new();
        for d in documents {
            folded.entry(d.relative.to_lowercase()).and_modify(|e| *e = None).or_insert(Some(d.relative.as_str()));
        }
        Paths { exact: documents.iter().map(|d| d.relative.as_str()).collect(), folded }
    }
    /// The file at `path`. A sheet made on Windows may name its tracks in other letters than the files have; the one file
    /// that differs only so is it.
    fn find(&self, path: &str) -> Option<String> {
        if self.exact.contains(path) {
            return Some(path.to_string());
        }
        self.folded.get(&path.to_lowercase()).copied().flatten().map(str::to_string)
    }
}
/// Every file a disc sheet (m3u, cue, gdi) names: the sheet, the file as the sheet writes it, and where that file is among
/// `documents` (None: it is not there).
pub fn sheet_files(documents: &[Document]) -> Vec<(String, String, Option<String>)> {
    let paths = Paths::new(documents);
    let mut out = Vec::new();
    for d in documents {
        for file in crate::library_names::sheet_references(&extension(&d.relative), &d.sheet) {
            let path = reference(&d.relative, &file).and_then(|path| paths.find(&path));
            out.push((d.relative.clone(), file, path));
        }
    }
    out
}
/// `path` from `folder`, both from the chosen folder's top: `../` for each step up.
fn relative_to(folder: &str, path: &str) -> String {
    let from: Vec<&str> = folder.split('/').filter(|s| !s.is_empty()).collect();
    let to: Vec<&str> = path.split('/').collect();
    let same = from.iter().zip(&to).take_while(|(a, b)| a == b).count();
    let mut parts = vec![".."; from.len() - same];
    parts.extend(&to[same..]);
    parts.join("/")
}
/// A sheet's files, down through a playlist's sheets, each once, the sheet itself left out, and their sizes added to its
/// own (-1 when one isn't known). Names as the sheets write them: the core looks for them by those names.
fn parts_of(sheet: &Document, by_path: &HashMap<&str, &Document>, paths: &Paths) -> (Vec<Part>, i64) {
    let folder = sheet.relative.rsplit_once('/').map_or("", |(f, _)| f);
    let mut parts = Vec::new();
    let mut size = sheet.size;
    let mut seen = HashSet::from([sheet.relative.clone()]);
    // (the sheet's document, its path as written by the sheet above), three deep as `frozen::fingerprint` reads.
    let mut pending = vec![(sheet, sheet.relative.clone(), 0)];
    while let Some((d, written, depth)) = pending.pop() {
        if depth > 0 {
            parts.push(Part { uri: d.uri.clone(), name: relative_to(folder, &written) });
            size = if size < 0 || d.size < 0 { -1 } else { size + d.size };
        }
        if depth >= 3 {
            continue;
        }
        let mut found = Vec::new();
        for file in crate::library_names::sheet_references(&extension(&d.relative), &d.sheet) {
            let (Some(as_written), Some(path)) = (reference(&written, &file), reference(&d.relative, &file).and_then(|p| paths.find(&p))) else { continue };
            let Some(document) = by_path.get(path.as_str()) else { continue };
            if seen.insert(path) {
                found.push((*document, as_written, depth + 1));
            }
        }
        // Depth first, in the sheet's order: a playlist's first disc with its tracks, then the next.
        pending.extend(found.into_iter().rev());
    }
    (parts, size)
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
    let art = crate::document_art::Index::new(&documents);
    let paths = Paths::new(&documents);
    let by_path: HashMap<&str, &Document> = documents.iter().map(|d| (d.relative.as_str(), d)).collect();
    let mut sheets: HashMap<String, (Vec<Part>, i64)> = HashMap::new();
    for d in &documents {
        if matches!(extension(&d.relative).as_str(), "cue" | "m3u" | "gdi") && !dependencies.contains(&d.relative) {
            sheets.insert(d.relative.clone(), parts_of(d, &by_path, &paths));
        }
    }
    drop(by_path);
    drop(paths);
    let mut seen = HashSet::new();
    let mut games = Vec::new();
    for d in documents {
        let key = d.key();
        if !seen.insert(key.clone()) || dependencies.contains(&d.relative) || in_bios_folder(&d.relative) || d.root_name.eq_ignore_ascii_case("bios") || bios_file(d.relative.rsplit('/').next().unwrap_or_default()).is_some() {
            continue;
        }
        let ext = extension(&d.relative);
        if !shelf_type(&ext) {
            continue;
        }
        let Some((system_id, system, profile, color)) = d.folder_system().or_else(|| system(&ext)) else {
            continue;
        };
        let stem = Path::new(&d.relative).file_stem().unwrap_or_default().to_string_lossy();
        let name = crate::library_names::title_case(&crate::library_names::display_name(&stem.replace('_', " ")));
        let (parts, size) = sheets.remove(&d.relative).unwrap_or((Vec::new(), d.size));
        games.push(Game {
            name,
            key,
            uri: d.uri.clone(),
            size,
            document_size: d.size,
            parts,
            missing: missing.remove(&d.relative).unwrap_or_default(),
            covers: art.pictures(&d, false),
            pictures: art.pictures(&d, true),
            cheats: art.cheats(&d),
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
            root_name: String::new(),
            sheet: sheet.into(),
        }
    }
    #[test]
    fn selecting_a_system_folder_keeps_its_system_and_identity() {
        for (folder, expected, file) in [("PSP", "psp", "Game.chd"), ("playstationportable", "psp", "Game.chd"), ("ps1", "psx", "Game.chd"), ("GENESIS", "megadrive", "Game.bin")] {
            let selected: Document = serde_json::from_value(serde_json::json!({
                "authority":"provider", "id":"unchanged", "uri":"content://provider/document/unchanged",
                "relative":"@pocket-root-second/".to_string()+file, "root_name":folder
            })).unwrap();
            let selected = scan(vec![selected]);
            assert_eq!(selected.len(), 1, "a directly selected {folder} folder must not lose its games");
            assert_eq!(selected[0].system_id, expected, "{folder}/{file}");
            let parent = scan(vec![doc("unchanged", &format!("{folder}/{file}"), "")]);
            assert_eq!(selected[0].key, parent[0].key, "changing the grant's root must keep the same saves");
        }
    }
    #[test]
    fn selected_root_context_preserves_sets_and_nearer_system_folders() {
        let mut cue = doc("cue", "@root/RPG/Game.cue", "FILE \"Track.bin\" BINARY");
        cue.root_name = "ps1".into();
        let mut track = doc("track", "@root/RPG/Track.bin", "");
        track.root_name = "ps1".into();
        let games = scan(vec![cue, track]);
        assert_eq!(games.len(), 1);
        assert_eq!(games[0].system_id, "psx");
        assert_eq!(games[0].parts[0].name, "Track.bin");
        assert!(games[0].missing.is_empty());
        let mut child = doc("child", "gba/Game.gba", "");
        child.root_name = "psp".into();
        assert_eq!(scan(vec![child])[0].system_id, "gba", "the nearest named system stays authoritative");
        let legacy: Document = serde_json::from_value(serde_json::json!({"authority":"provider", "id":"old", "uri":"content://provider/old", "relative":"Unknown/Game.chd"})).unwrap();
        assert!(scan(vec![legacy]).is_empty(), "an unknown or older root must not guess a disc system");
        let mut bios = doc("bios", "firmware.gba", "");
        bios.root_name = "BIOS".into();
        assert!(scan(vec![bios]).is_empty(), "a directly selected BIOS folder still holds no games");
    }
    #[test]
    fn selected_sega_bios_files_are_normalised_and_wrong_sizes_do_not_count() {
        let docs = vec![sized("sat", "bios/SEGA_101.BIN", "", 512 << 10), sized("boot", "bios/dc/DC_BOOT.BIN", "", 2 << 20),
            sized("flash", "bios/dc_flash.bin", "", 128 << 10), sized("short", "bios/saturn_bios.bin", "", 2),
            sized("game", "saturn/Play.iso", "", 400 << 20)];
        let bios = player_bios(&docs);
        assert_eq!(bios.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), ["sega_101.bin", "dc/dc_boot.bin", "dc/dc_flash.bin"]);
        assert_eq!(scan(docs).len(), 1, "BIOS files never become games");
        let kept = tempfile::tempdir().unwrap();
        assert_eq!(bios_hint("saturn", &bios, kept.path()), None);
        assert_eq!(bios_hint("dreamcast", &bios, kept.path()), None);
        assert_eq!(bios_hint("dreamcast", &bios[2..], kept.path()), Some(DREAMCAST_BIOS_MISSING), "Flash without boot still needs guidance");
        std::fs::create_dir(kept.path().join("dc")).unwrap();
        std::fs::write(kept.path().join("dc/dc_boot.bin"), vec![0; 2 << 20]).unwrap();
        assert_eq!(bios_hint("dreamcast", &[], kept.path()), None);
        std::fs::write(kept.path().join("saturn_bios.bin"), [0; 10]).unwrap();
        assert_eq!(bios_hint("saturn", &[], kept.path()), Some(SATURN_BIOS_MISSING), "An incomplete kept BIOS cannot unlock the start");
    }

    #[test]
    fn android_sega_discs_without_bios_have_a_clear_hint() {
        let kept = tempfile::tempdir().unwrap();
        for system in ["saturn", "dreamcast"] {
            let hint = bios_hint(system, &[], kept.path());
            assert!(hint.is_some_and(|s| s.contains("BIOS")), "{system}: a missing BIOS must be explained before the player starts");
        }
    }

    #[test]
    fn android_sega_discs_are_streamed_from_their_staged_files() {
        for system in ["saturn", "dreamcast"] {
            assert!(plays_from_its_file(system), "{system}: a full disc must not be loaded into memory");
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
    fn xbox_discs_and_3ds_homebrew_are_on_the_shelf() {
        let games = scan(vec![doc("a", "roms/xbox/Top Game (USA).xiso.iso", ""), doc("b", "roms/3ds/Yahtzee3DS.3dsx", ""), doc("c", "Downloads/Other.3dsx", "")]);
        let got: Vec<_> = games.iter().map(|g| (g.relative.as_str(), g.system_id, g.profile)).collect();
        assert_eq!(got, [("Downloads/Other.3dsx", "3ds", "3ds"), ("roms/xbox/Top Game (USA).xiso.iso", "xbox", "xbox"), ("roms/3ds/Yahtzee3DS.3dsx", "3ds", "3ds")]);
        assert_eq!(games.iter().find(|g| g.system_id == "xbox").map(|g| g.name.as_str()), Some("Top Game"));
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
    fn sized(id: &str, path: &str, sheet: &str, size: i64) -> Document {
        Document { size, ..doc(id, path, sheet) }
    }
    #[test]
    fn a_cue_game_brings_every_track_to_its_copy_once() {
        let games = scan(vec![
            sized("cue", "psx/Game/Game.cue", "FILE \"Game (Track 1).bin\" BINARY\n  TRACK 01 MODE2/2352\nFILE \"Game (Track 2).bin\" BINARY\n  TRACK 02 AUDIO\nFILE \"Game (Track 1).bin\" BINARY", 200),
            sized("t1", "psx/Game/Game (Track 1).bin", "", 300 << 20),
            sized("t2", "psx/Game/Game (Track 2).bin", "", 40 << 20),
            sized("other", "psx/Game/Readme.txt", "", 10),
        ]);
        assert_eq!(games.len(), 1, "the tracks are no games of their own");
        let names: Vec<_> = games[0].parts.iter().map(|p| (p.uri.rsplit('/').next().unwrap(), p.name.as_str())).collect();
        assert_eq!(names, [("t1", "Game (Track 1).bin"), ("t2", "Game (Track 2).bin")], "each track once, under the name the sheet gives it");
        // What the copy holds, sheet and tracks: the size a moment's mark is made with (`frozen::fingerprint`).
        assert_eq!(games[0].size, 200 + (300 << 20) + (40 << 20));
        assert!(plays_from_its_file(games[0].system_id), "a disc is played from its file, never loaded whole");
    }
    #[test]
    fn a_playlist_brings_its_discs_with_their_tracks_in_their_folders() {
        let games = scan(vec![
            sized("m3u", "psx/RPG/RPG.m3u", "Disc 1/RPG (Disc 1).cue\nDisc 2/RPG (Disc 2).chd", 60),
            sized("c1", "psx/RPG/Disc 1/RPG (Disc 1).cue", "FILE \"RPG (Disc 1).bin\" BINARY", 80),
            sized("b1", "psx/RPG/Disc 1/RPG (Disc 1).bin", "", 600 << 20),
            sized("d2", "psx/RPG/Disc 2/RPG (Disc 2).chd", "", 400 << 20),
        ]);
        assert_eq!(games.len(), 1);
        let names: Vec<_> = games[0].parts.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["Disc 1/RPG (Disc 1).cue", "Disc 1/RPG (Disc 1).bin", "Disc 2/RPG (Disc 2).chd"]);
        assert_eq!(games[0].size, 60 + 80 + (600 << 20) + (400 << 20));
        // One unknown size: the whole is unknown, as the provider's -1 for one file.
        let games = scan(vec![sized("cue", "psx/G.cue", "FILE \"G.bin\" BINARY", 50), sized("bin", "psx/G.bin", "", -1)]);
        assert_eq!(games[0].size, -1);
    }
    #[test]
    fn a_track_named_in_other_letters_is_found_and_copied_under_the_sheets_name() {
        // A sheet written on Windows: its tracks' names in other letters than the files'. The core on the phone reads
        // the copy by the sheet's name, as the file on a case-blind card was found.
        let games = scan(vec![doc("cue", "psx/Game.cue", "FILE \"GAME.BIN\" BINARY"), doc("bin", "psx/game.bin", "")]);
        assert_eq!(games.len(), 1, "the track is no game of its own");
        assert!(games[0].missing.is_empty(), "{:?}", games[0].missing);
        assert_eq!(games[0].parts.len(), 1);
        assert_eq!(games[0].parts[0].name, "GAME.BIN");
        assert!(games[0].parts[0].uri.ends_with("/bin"));
    }
    #[test]
    fn a_cartridge_has_no_parts_and_its_own_size() {
        let games = scan(vec![sized("a", "gba/A.gba", "", 4 << 20), sized("b", "psx/Single.chd", "", 300 << 20), sized("c", "psx/Eboot.pbp", "", 200 << 20)]);
        assert!(games.iter().all(|g| g.parts.is_empty()));
        assert_eq!(games.iter().find(|g| g.system_id == "psx" && g.relative.ends_with(".pbp")).map(|g| g.size), Some(200 << 20), "a PS1 PBP in a psx folder is a PlayStation game");
    }
    #[test]
    fn a_track_outside_the_sheets_folder_is_named_so_the_start_can_refuse_it() {
        let games = scan(vec![doc("cue", "psx/Game/Game.cue", "FILE \"../Shared.bin\" BINARY"), doc("bin", "psx/Shared.bin", "")]);
        let game = games.iter().find(|g| g.relative.ends_with(".cue")).unwrap();
        assert_eq!(game.parts[0].name, "../Shared.bin", "PocketActivity refuses a name that leaves the copy's folder");
    }
    #[test]
    fn a_playstation_bios_is_no_game_wherever_it_lies_and_is_found_for_the_player() {
        let documents = vec![
            sized("b1", "bios/SCPH5501.BIN", "", 524288),
            sized("b2", "psx/scph1001.bin", "", 524288),
            sized("b3", "psx/scph7001.bin", "", 1000),
            sized("g", "psx/Game.bin", "", 300 << 20),
            sized("s", "bios/sega_101.bin", "", 524288),
        ];
        let games = scan(documents.clone());
        let left: Vec<_> = games.iter().map(|g| g.relative.as_str()).collect();
        assert_eq!(left, ["psx/Game.bin"], "a BIOS by its name is no game, whatever its size");
        // Only one of a BIOS' size is taken for the player.
        let found: Vec<_> = ps1_bios(&documents).into_iter().map(|p| (p.name, p.uri.rsplit('/').next().unwrap().to_string())).collect();
        // Lower case, as the core looks for them first; another console's BIOS of the same size isn't taken.
        assert_eq!(found, [("scph5501.bin".to_string(), "b1".to_string()), ("scph1001.bin".to_string(), "b2".to_string())]);
        assert!(ps1_bios(&[sized("x", "psx/Game.bin", "", 524288)]).is_empty());
    }
    #[test]
    fn a_playstation_start_without_a_bios_says_so_and_with_one_says_nothing() {
        let kept = tempfile::tempdir().unwrap();
        assert_eq!(bios_hint("psx", &[], kept.path()), Some(PS1_BIOS_MISSING), "nothing found, nothing kept");
        assert_eq!(bios_hint("psx", &[], &kept.path().join("never made")), Some(PS1_BIOS_MISSING));
        let found = [Part { uri: "content://x/b".into(), name: "scph5501.bin".into() }];
        assert_eq!(bios_hint("psx", &found, kept.path()), None, "one among the folders is copied for this start");
        // A file of the wrong size, or another console's, isn't one.
        std::fs::write(kept.path().join("scph5501.bin"), [0u8; 1000]).unwrap();
        std::fs::write(kept.path().join("sega_101.bin"), vec![0u8; 512 << 10]).unwrap();
        assert_eq!(bios_hint("psx", &[], kept.path()), Some(PS1_BIOS_MISSING));
        std::fs::write(kept.path().join("scph5501.bin"), vec![0u8; 512 << 10]).unwrap();
        assert_eq!(bios_hint("psx", &[], kept.path()), None, "one kept from an earlier start still counts");
        assert_eq!(bios_hint("gba", &[], &kept.path().join("never made")), None);
        // Every language says it, and whole: Android shows a toast in two lines and cuts the rest, which was what to do
        // (seen on the Odin 2 Mini). About 35 letters a line there.
        assert!(PS1_BIOS_MISSING.chars().count() <= 72, "{}", PS1_BIOS_MISSING.chars().count());
        for table in [include_str!("../assets/i18n/de.tsv"), include_str!("../assets/i18n/es.tsv")] {
            let line = table.lines().find_map(|l| l.split_once('\t').filter(|(en, _)| *en == PS1_BIOS_MISSING)).map(|(_, t)| t);
            assert!(line.is_some_and(|t| t.chars().count() <= 72), "{line:?}");
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
