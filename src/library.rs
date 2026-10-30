//! Game library read from ROCKNIX's es_systems.cfg: systems, their ROM folders and default cores.

use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Game {
    pub name: String,
    pub path: PathBuf,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct System {
    /// Short id, also the runemu platform (`-P`), e.g. "gbc".
    pub name: String,
    pub fullname: String,
    pub emulator: Option<String>,
    pub core: Option<String>,
    /// Every emulator and core es_systems.cfg lists for the system: what "Runs on" offers (`emu_choice`).
    pub emulators: Vec<crate::emu_choice::EsEmulator>,
    pub games: Vec<Game>,
}

/// ROCKNIX entries that are apps rather than game systems; they belong in settings, not on the shelf.
/// ("moonlight" is Home's "PC streaming" entry, `moonlight.rs`: the `.sh` scripts ROCKNIX's menu writes for it
/// don't start through runemu.sh. "imageviewer" is ES's "Screenshots" on `/storage/roms/screenshots`, where HOME + A
/// keeps its pictures: they are Settings → Games → Screenshots, `gallery.rs`, never games.)
const NOT_GAMES: &[&str] = &["tools", "music", "moonlight", "imageviewer"];

/// Loads every system from `es_systems` that has at least one game, sorted by full name.
pub fn load(es_systems: &Path) -> Result<Vec<System>, String> {
    load_with(es_systems, None)
}

/// Whether the checksum cache may be read for cartridge names (`names_ready`).
static NAMES_READY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// The app has read its games' checksums once: lists made from now on show a cartridge by the name its checksum gave
/// ("Supar Mario Land 2" is "Super Mario Land 2"). Before, a list shows file names, as it did before the checksums.
pub fn names_ready(on: bool) {
    NAMES_READY.store(on, std::sync::atomic::Ordering::Relaxed);
}

/// Where the EmulationStation gamelists are read from and cached while the list is made (`gamelist`).
pub struct Gamelists<'a> {
    pub data_dir: &'a Path,
    pub places: &'a crate::gamelist::Locations,
}

/// [`load`], with the names a game's `gamelist.xml` gives a cryptic file name (`gamelist::better_name`) when
/// `gamelists` says where to look. The names are in the list before it is sorted and told apart, and so in the cache,
/// the search, Recent and Favorites, which all follow the list.
pub fn load_with(es_systems: &Path, gamelists: Option<&Gamelists>) -> Result<Vec<System>, String> {
    let xml = fs::read_to_string(es_systems).map_err(|e| format!("{}: {e}", es_systems.display()))?;
    let doc = roxmltree::Document::parse(&xml).map_err(|e| format!("{}: {e}", es_systems.display()))?;
    let child_text = |node: roxmltree::Node, tag: &str| {
        node.children().find(|n| n.has_tag_name(tag)).and_then(|n| n.text()).map(|t| t.trim().to_string())
    };

    let mut systems: Vec<System> = doc
        .descendants()
        .filter(|n| n.has_tag_name("system"))
        .filter_map(|sys| {
            let name = child_text(sys, "name").filter(|n| !NOT_GAMES.contains(&n.as_str()))?;
            let path = PathBuf::from(child_text(sys, "path")?);
            let mut exts: Vec<String> = child_text(sys, "extension")
                .unwrap_or_default()
                .split_whitespace()
                .map(|e| e.trim_start_matches('.').to_ascii_lowercase())
                .collect();
            let (emulator, core) = default_core(sys);
            // Systems pocket-core hosts may list more: DOS zips, game folders (`hostsys`).
            exts.extend(crate::hostsys::shelf_extensions(&name).iter().map(|e| e.to_string()));
            let mut games = scan_for(&name, &path, &exts);
            games.retain(|g| !crate::hostsys::not_a_game(&name, &g.path));
            let listed: Vec<PathBuf> = games.iter().map(|g| g.path.clone()).collect();
            let roms = Path::new(crate::hostsys::BIOS_DIR).parent().unwrap_or(Path::new("/storage/roms"));
            for dir in crate::hostsys::folder_games(&name, &path, roms, &listed) {
                let title = display_name(dir.file_name().and_then(|n| n.to_str()).unwrap_or("?"));
                games.push(Game { name: title, path: dir });
            }
            // The names the gamelist gives are kept as they are; the others are file names, cased for the shelf below.
            let mut from_list = std::collections::HashSet::new();
            if let Some(g) = gamelists {
                let data = crate::gamelist::system(g.data_dir, g.places, &name, &path);
                for game in games.iter_mut() {
                    if let Some(better) = data.get(&game.path).and_then(|e| crate::gamelist::better_name(&game.name, &e.name)) {
                        game.name = better;
                        from_list.insert(game.path.clone());
                    }
                }
            }
            // A game the checksum (cartridges, `rom_id`) or the serial (PlayStation discs, `disc_serial`) named shows that
            // name: "SLES_556.73.Something" is "Final Fantasy VII", "Supar Mario Land 2" is "Super Mario Land 2" and
            // "Metroid - Fusoin" that is really Advance Wars says so (new-user test 2026-10-30: the cover said one
            // game, the title another). A gamelist's name comes first; the name is read from the card only (no image,
            // nothing downloaded), so the scan stays quick. A file name that only differs in case, tags and
            // punctuation keeps its own spelling.
            // A cartridge's name waits until the app has had its checksums read once (`names_ready`): the first list of a
            // start stays as quick as ever and never waits for the checksum file.
            let identifiable = crate::disc_serial::dat(&name).is_some() || (crate::rom_id::dat_name(&name).is_some() && NAMES_READY.load(std::sync::atomic::Ordering::Relaxed));
            if let Some(g) = gamelists.filter(|_| identifiable) {
                let identifiable: Vec<(String, PathBuf)> = games.iter().filter(|g| !from_list.contains(&g.path)).map(|g| (name.clone(), g.path.clone())).collect();
                for (path, title) in crate::rom_id::known(g.data_dir, &identifiable) {
                    if let Some(game) = games.iter_mut().find(|g| g.path == path) {
                        let proper = crate::disc_serial::shelf_name(&title);
                        if crate::covers::normalize(&proper) != crate::covers::normalize(&game.name) {
                            game.name = proper;
                        }
                    }
                }
            }
            games.sort_by_key(|g| g.name.to_lowercase());
            tell_apart(&mut games);
            for game in games.iter_mut().filter(|g| !from_list.contains(&g.path)) {
                game.name = title_case(&game.name);
            }
            (!games.is_empty()).then(|| System {
                fullname: child_text(sys, "fullname").unwrap_or_else(|| name.clone()),
                name,
                emulator,
                core,
                emulators: crate::emu_choice::parse_emulators(sys),
                games,
            })
        })
        .collect();
    systems.sort_by_key(|s| s.fullname.to_lowercase());
    learn_names(&systems);
    Ok(systems)
}

/// Tells the interface that these are names of games (`i18n::learn_names`): a game called "Black" or "Time to Play"
/// is painted as it is in German, not as the table's word. Every list of games is made by `load_with` or read by
/// `library_cache::read`, and both call this; the names made from a file name elsewhere (`shelf_name`) learn their own.
pub fn learn_names(systems: &[System]) {
    crate::i18n::learn_names(systems.iter().flat_map(|s| s.games.iter().map(|g| g.name.as_str())));
}

/// `display_name`, for a name that is going to be painted (the recents, the hidden games, the gallery, the history):
/// the result is learned as a name (`i18n::learn_names`). Not for a name only compared (`gamedb`'s matching).
pub fn shelf_name(stem: &str) -> String {
    let name = title_case(&display_name(stem));
    crate::i18n::learn_names([&name]);
    name
}

/// Takes the games the player hid (`library_meta`, "Hide from the shelf" on the card) out of `systems`, and a system
/// left without games with them, as `load` leaves out a system without games. Every list, the search, Curator and the
/// web companion are made from what is left, so a hidden game is gone from all of them at once.
pub fn without_hidden(systems: &mut Vec<System>, hidden: &std::collections::HashSet<PathBuf>) {
    if hidden.is_empty() {
        return;
    }
    for s in systems.iter_mut() {
        s.games.retain(|g| !hidden.contains(&g.path));
    }
    systems.retain(|s| !s.games.is_empty());
}

/// `load` with the names of EmulationStation's gamelists (`load_with`), without the games `data_dir`'s library.json
/// says are hidden.
pub fn load_shown(es_systems: &Path, data_dir: &Path) -> Result<Vec<System>, String> {
    let places = crate::gamelist::Locations::device();
    let mut systems = load_with(es_systems, Some(&Gamelists { data_dir, places: &places }))?;
    crate::gamelist::flush(data_dir);
    without_hidden(&mut systems, &crate::library_meta::hidden_paths(data_dir));
    Ok(systems)
}

/// A system pocket-ui can start, as es_systems.cfg describes it, without looking at its games.
#[derive(Clone, Debug, PartialEq)]
pub struct SysInfo {
    /// ROCKNIX id, e.g. "gba".
    pub id: String,
    /// Full name, e.g. "Game Boy Advance".
    pub name: String,
    /// The ROM folder as people see it over the share: "roms/gba".
    pub folder: String,
    /// The folder itself.
    pub path: PathBuf,
    /// File types it takes, lowercase without the dot, in es_systems order, each once.
    pub extensions: Vec<String>,
}

/// Every game system in `es_systems` (not Tools or Music), sorted by full name.
pub fn all_systems(es_systems: &Path) -> Result<Vec<SysInfo>, String> {
    let xml = fs::read_to_string(es_systems).map_err(|e| format!("{}: {e}", es_systems.display()))?;
    let doc = roxmltree::Document::parse(&xml).map_err(|e| format!("{}: {e}", es_systems.display()))?;
    let child_text = |node: roxmltree::Node, tag: &str| {
        node.children().find(|n| n.has_tag_name(tag)).and_then(|n| n.text()).map(|t| t.trim().to_string())
    };
    let mut all: Vec<SysInfo> = doc
        .descendants()
        .filter(|n| n.has_tag_name("system"))
        .filter_map(|sys| {
            let id = child_text(sys, "name").filter(|n| !NOT_GAMES.contains(&n.as_str()))?;
            let path = child_text(sys, "path")?;
            // The share starts at /storage/roms; a few apps keep their files elsewhere and show their own path.
            let path = path.trim_end_matches('/');
            let folder = path.strip_prefix("/storage/").filter(|p| p.starts_with("roms/")).unwrap_or(path).to_string();
            let mut extensions: Vec<String> = Vec::new();
            for e in child_text(sys, "extension").unwrap_or_default().split_whitespace() {
                let e = e.trim_start_matches('.').to_ascii_lowercase();
                if !e.is_empty() && !extensions.contains(&e) {
                    extensions.push(e);
                }
            }
            Some(SysInfo { name: child_text(sys, "fullname").unwrap_or_else(|| id.clone()), id, folder, path: PathBuf::from(path), extensions })
        })
        .collect();
    all.sort_by_key(|s| s.name.to_lowercase());
    Ok(all)
}

/// How many games `dir` holds for the system `system` taking `exts`, counted the way the shelf finds them.
pub fn count_games(system: &str, dir: &Path, exts: &[String]) -> usize {
    scan_for(system, dir, exts).len()
}

/// A disc sheet (`.cue`, `.gdi`) the shelf leaves out because a file it names is not there.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BrokenDisc {
    /// The sheet.
    pub sheet: PathBuf,
    /// The first file it names that is missing, as the sheet writes it ("Lost Bin (Europe).bin").
    pub missing: String,
}

/// `count_games`, and the disc sheets of the folder left out because a file they name is missing, for Add games.
pub fn look_at_folder(system: &str, dir: &Path, exts: &[String]) -> (usize, Vec<BrokenDisc>) {
    let mut broken = Vec::new();
    let games = if crate::hostsys::folders_are_games(system) { scan_flat_into(dir, exts, &mut broken) } else { scan_into(dir, exts, &mut broken) };
    (games.len(), broken)
}

/// A folder in `roms` that is no system's folder, so nothing in it is read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StrayFolder {
    /// Its name as it is on the card: "Sega Genesis".
    pub name: String,
    /// The folder ROCKNIX reads for what it probably holds, when that can be guessed: "megadrive".
    pub meant: Option<String>,
}

/// Folders of `roms` that ROCKNIX keeps for itself or that hold no games, never reported as strays.
const NOT_STRAYS: &[&str] = &[
    "bios", "savestates", "screenshots", "videos", "music", "tools", "themes", "splash", "backups", "backup", "logs",
    "lost+found", "system volume information", "moonlight", "port-files", "ports_scripts", "bgm", "mplayer", "manuals",
    "downloads", "media", "images", "covers",
    // Settings → System → About → Report a problem (`report`).
    "reports",
];

/// Common names people give system folders (lowercase, letters and digits only) and the ROCKNIX folder they mean;
/// a guess is offered only for a folder this device has.
const FOLDER_ALIASES: &[(&str, &str)] = &[
    ("segagenesis", "megadrive"),
    ("genesis", "megadrive"),
    ("md", "megadrive"),
    ("segamegadrive", "megadrive"),
    ("megadrivegenesis", "megadrive"),
    ("ps1", "psx"),
    ("ps", "psx"),
    ("playstation", "psx"),
    ("playstation1", "psx"),
    ("sonyplaystation", "psx"),
    ("ps2", "ps2"),
    ("playstation2", "ps2"),
    ("snes", "snes"),
    ("supernintendo", "snes"),
    ("supernes", "snes"),
    ("superfamicom", "sfc"),
    ("nes", "nes"),
    ("nintendo", "nes"),
    ("famicom", "famicom"),
    ("gameboy", "gb"),
    ("gameboycolor", "gbc"),
    ("gameboycolour", "gbc"),
    ("gameboyadvance", "gba"),
    ("gba", "gba"),
    ("n64", "n64"),
    ("nintendo64", "n64"),
    ("nds", "nds"),
    ("ds", "nds"),
    ("nintendods", "nds"),
    ("gamecube", "gamecube"),
    ("gc", "gamecube"),
    ("ngc", "gamecube"),
    ("dreamcast", "dreamcast"),
    ("dc", "dreamcast"),
    ("saturn", "saturn"),
    ("segasaturn", "saturn"),
    ("segacd", "segacd"),
    ("megacd", "segacd"),
    ("sega32x", "sega32x"),
    ("32x", "sega32x"),
    ("mastersystem", "mastersystem"),
    ("segamastersystem", "mastersystem"),
    ("sms", "mastersystem"),
    ("gamegear", "gamegear"),
    ("segagamegear", "gamegear"),
    ("gg", "gamegear"),
    ("pcengine", "pcengine"),
    ("turbografx16", "pcengine"),
    ("turbografx", "pcengine"),
    ("tg16", "pcengine"),
    ("psp", "psp"),
    ("playstationportable", "psp"),
    ("psvita", "psvita"),
    ("playstationvita", "psvita"),
    ("arcade", "arcade"),
    ("mame", "mame"),
    ("neogeo", "neogeo"),
    ("atari2600", "atari2600"),
    ("2600", "atari2600"),
    ("lynx", "atarilynx"),
    ("atarilynx", "atarilynx"),
    ("wonderswan", "wonderswan"),
    ("neogeopocket", "ngp"),
    ("dos", "pc"),
    ("msdos", "pc"),
];

/// The folder the systems' folders are in (`/storage/roms` on the device): the one most of them share.
pub fn roms_root(catalog: &[SysInfo]) -> Option<PathBuf> {
    let mut seen: Vec<(&Path, usize)> = Vec::new();
    for parent in catalog.iter().filter_map(|s| s.path.parent()) {
        match seen.iter_mut().find(|(p, _)| *p == parent) {
            Some((_, n)) => *n += 1,
            None => seen.push((parent, 1)),
        }
    }
    seen.into_iter().max_by_key(|(_, n)| *n).map(|(p, _)| p.to_path_buf())
}

/// Folders directly in `roms` that no system of `catalog` reads and that hold at least one file a system takes,
/// sorted by name, each with the folder it probably should be. Hidden folders, those starting with `_` and ROCKNIX's
/// own ([`NOT_STRAYS`]) are left out. A "GBA" is a stray too: ext4 tells it from "gba", and only "gba" is read.
/// Reads `roms` and up to two levels of each other folder; call it on a thread.
pub fn stray_folders(roms: &Path, catalog: &[SysInfo]) -> Vec<StrayFolder> {
    let Ok(entries) = fs::read_dir(roms) else { return Vec::new() };
    let known: std::collections::HashSet<PathBuf> = catalog.iter().map(|s| s.path.clone()).collect();
    let exts: std::collections::HashSet<&str> = catalog.iter().flat_map(|s| s.extensions.iter().map(String::as_str)).collect();
    let mut out: Vec<StrayFolder> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir() && !known.contains(p))
        .filter_map(|p| {
            let name = p.file_name()?.to_str()?.to_string();
            if name.starts_with(['.', '_']) || NOT_STRAYS.contains(&name.to_lowercase().as_str()) || !holds_games(&p, &exts) {
                return None;
            }
            Some(StrayFolder { meant: guess_folder(&name, catalog), name })
        })
        .collect();
    out.sort_by_key(|s| s.name.to_lowercase());
    out
}

/// Whether `dir` or one of its subfolders holds a visible file with one of `exts`.
fn holds_games(dir: &Path, exts: &std::collections::HashSet<&str>) -> bool {
    let takes = |p: &Path| {
        let hidden = p.file_name().and_then(|n| n.to_str()).is_none_or(|n| n.starts_with('.'));
        !hidden && p.extension().and_then(|e| e.to_str()).is_some_and(|e| exts.contains(e.to_ascii_lowercase().as_str()))
    };
    let Ok(entries) = fs::read_dir(dir) else { return false };
    let mut subs = Vec::new();
    for p in entries.flatten().map(|e| e.path()) {
        if p.is_dir() {
            subs.push(p);
        } else if takes(&p) {
            return true;
        }
    }
    subs.iter().any(|s| fs::read_dir(s).is_ok_and(|e| e.flatten().any(|e| e.path().is_file() && takes(&e.path()))))
}

/// The system folder a stray folder `name` probably should be: a system's folder or full name written another way
/// ("GBA", "Game Boy Advance"), else a common name ([`FOLDER_ALIASES`]) of a system `catalog` has.
fn guess_folder(name: &str, catalog: &[SysInfo]) -> Option<String> {
    let plain = |s: &str| s.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect::<String>();
    let wanted = plain(name);
    if wanted.is_empty() {
        return None;
    }
    let folder = |s: &SysInfo| s.path.file_name().and_then(|n| n.to_str()).unwrap_or(&s.id).to_string();
    if let Some(s) = catalog.iter().find(|s| plain(&folder(s)) == wanted || plain(&s.id) == wanted) {
        return Some(folder(s));
    }
    if let Some(s) = catalog.iter().find(|s| plain(&s.name) == wanted) {
        return Some(folder(s));
    }
    let id = FOLDER_ALIASES.iter().find(|(alias, _)| *alias == wanted).map(|(_, id)| *id)?;
    catalog.iter().find(|s| s.id == id).map(folder)
}

/// The emulator/core pair marked `default="true"`, else the first one listed.
fn default_core(sys: roxmltree::Node) -> (Option<String>, Option<String>) {
    let emulators: Vec<_> = sys.descendants().filter(|n| n.has_tag_name("emulator")).collect();
    let pair = |emu: roxmltree::Node, core: Option<roxmltree::Node>| {
        (emu.attribute("name").map(str::to_string), core.and_then(|c| c.text()).map(|t| t.trim().to_string()))
    };
    for emu in &emulators {
        if let Some(core) = emu.descendants().find(|n| n.has_tag_name("core") && n.attribute("default") == Some("true")) {
            return pair(*emu, Some(core));
        }
    }
    match emulators.first() {
        Some(emu) => pair(*emu, emu.descendants().find(|n| n.has_tag_name("core"))),
        None => (None, None),
    }
}

/// The games of a system's ROM folder: `scan`, except that a system whose game folders pocket-core starts itself
/// (DOS, ScummVM: `hostsys::folder_games`) is read flat.
fn scan_for(system: &str, dir: &Path, exts: &[String]) -> Vec<Game> {
    if crate::hostsys::folders_are_games(system) {
        scan_flat(dir, exts)
    } else {
        scan(dir, exts)
    }
}

/// `scan_flat` plus one level of subfolders, the way ES-DE and RetroArch read a folder per game. A subfolder with an
/// `.m3u` is one game (that playlist); one with a single launchable file left after the duplicate rules is one game
/// named after the folder; otherwise each launchable file is a game of its own. Hidden folders and those starting
/// with `_` ("_saves-import") are skipped, and nothing is read two levels down.
fn scan(dir: &Path, exts: &[String]) -> Vec<Game> {
    scan_into(dir, exts, &mut Vec::new())
}

/// `scan`, adding the disc sheets it left out for a missing file to `broken`.
fn scan_into(dir: &Path, exts: &[String], broken: &mut Vec<BrokenDisc>) -> Vec<Game> {
    let mut games = scan_flat_into(dir, exts, broken);
    let Ok(entries) = fs::read_dir(dir) else { return games };
    for sub in entries.flatten().map(|e| e.path()).filter(|p| p.is_dir()) {
        let folder = sub.file_name().and_then(|n| n.to_str()).unwrap_or_default();
        if folder.starts_with(['.', '_']) {
            continue;
        }
        let mut inside = scan_flat_into(&sub, exts, broken);
        let is_m3u = |g: &Game| g.path.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("m3u"));
        let playlists: Vec<Game> = inside.iter().filter(|g| is_m3u(g)).cloned().collect();
        if playlists.len() == 1 {
            inside = playlists;
        }
        if inside.len() == 1 {
            inside[0].name = display_name(folder);
        }
        games.extend(inside);
    }
    games.sort_by_key(|g| g.name.to_lowercase());
    games
}

/// Files directly in `dir` whose extension is in `exts`, sorted by name; hidden files are skipped.
fn scan_flat(dir: &Path, exts: &[String]) -> Vec<Game> {
    scan_flat_into(dir, exts, &mut Vec::new())
}

/// `scan_flat`, adding the disc sheets it left out for a missing file to `broken`.
fn scan_flat_into(dir: &Path, exts: &[String], broken: &mut Vec<BrokenDisc>) -> Vec<Game> {
    let Ok(entries) = fs::read_dir(dir) else { return Vec::new() };
    let files: Vec<PathBuf> = entries.flatten().map(|e| e.path()).filter(|p| p.is_file()).collect();
    let mut games: Vec<Game> = files
        .iter()
        .filter(|p| {
            let ext = p.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase);
            let hidden = p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with('.'));
            !hidden && ext.is_some_and(|e| exts.contains(&e))
        })
        .map(|p| Game { name: display_name(p.file_stem().and_then(|s| s.to_str()).unwrap_or("?")), path: p.clone() })
        .collect();
    // A disc image that a sheet or playlist of the folder names is part of that game, not a game of its own
    // (RetroArch's playlist scan does the same): Twelve Snakes.cue + Twelve Snakes.iso is one entry, the .cue.
    let sheets = read_sheets(&games);
    let referenced: std::collections::HashSet<String> = sheets.iter().flat_map(|(_, names)| names.iter().map(|n| n.to_lowercase())).collect();
    games.retain(|g| !g.path.file_name().and_then(|n| n.to_str()).is_some_and(|n| referenced.contains(&n.to_lowercase())));
    // A cue or gdi sheet whose track file is missing can't start (finding #11 of the first-run test, 2026-10-24): it
    // is left out, and Add games names it (`look_at_folder`). A sheet that names nothing, or a file in a subfolder
    // we don't look into, stays. Names are compared ignoring case (a cue written on Windows), as above.
    let here: std::collections::HashSet<String> = if sheets.is_empty() {
        Default::default()
    } else {
        files.iter().filter_map(|p| p.file_name().and_then(|n| n.to_str())).map(str::to_lowercase).collect()
    };
    for (sheet, names) in &sheets {
        let kind = sheet.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).unwrap_or_default();
        if !matches!(kind.as_str(), "cue" | "gdi") {
            continue;
        }
        if let Some(missing) = names.iter().find(|n| !here.contains(&n.to_lowercase())) {
            games.retain(|g| g.path != *sheet);
            broken.push(BrokenDisc { sheet: sheet.clone(), missing: missing.clone() });
        }
    }
    games.sort_by_key(|g| g.name.to_lowercase());
    games
}

/// Sheets and playlists are read up to this size; a real one is a few hundred bytes.
const SHEET_MAX_BYTES: u64 = 1 << 20;

/// The `.cue`, `.m3u` and `.gdi` sheets among `games`, each with the file names (no folder, as the sheet writes them)
/// it points at, not counting a file that names itself. Only sheets that are in the list count: a system that does not
/// list `.cue` keeps its `.bin`.
fn read_sheets(games: &[Game]) -> Vec<(PathBuf, Vec<String>)> {
    let mut out = Vec::new();
    for g in games {
        let Some(own) = g.path.file_name().and_then(|n| n.to_str()).map(str::to_lowercase) else { continue };
        let kind = g.path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).unwrap_or_default();
        if !matches!(kind.as_str(), "cue" | "m3u" | "gdi") || fs::metadata(&g.path).map_or(true, |m| m.len() > SHEET_MAX_BYTES) {
            continue;
        }
        let Ok(bytes) = fs::read(&g.path) else { continue };
        let text = String::from_utf8_lossy(&bytes);
        let names: Vec<String> = sheet_references(&kind, &text)
            .into_iter()
            // Only files next to the sheet are in this folder's list.
            .map(|name| name.strip_prefix("./").map(str::to_string).unwrap_or(name))
            .filter(|name| !name.contains(['/', '\\']) && name.to_lowercase() != own)
            .collect();
        out.push((g.path.clone(), names));
    }
    out
}

/// The files a disc sheet names: `FILE "x.bin" BINARY` lines of a cue sheet, the lines of an m3u playlist, the
/// file column of a gdi.
pub(crate) fn sheet_references(kind: &str, text: &str) -> Vec<String> {
    let lines = text.lines().map(|l| l.trim().trim_start_matches('\u{feff}')).filter(|l| !l.is_empty());
    match kind {
        "m3u" => lines.filter(|l| !l.starts_with('#')).map(str::to_string).collect(),
        "cue" => lines
            .filter(|l| l.len() > 5 && l.is_char_boundary(5) && l[..5].eq_ignore_ascii_case("FILE "))
            .filter_map(|l| {
                let rest = l[5..].trim();
                match rest.strip_prefix('"') {
                    Some(quoted) => quoted.split('"').next().map(str::to_string),
                    // Unquoted: everything but the format word at the end ("FILE track01.bin BINARY").
                    None => rest.rsplit_once(char::is_whitespace).map(|(file, _)| file.trim().to_string()),
                }
            })
            .collect(),
        // "1 0 4 2352 track01.bin 0": the first line is the track count; the name may be quoted and hold spaces.
        "gdi" => lines
            .skip(1)
            .filter_map(|l| {
                let mut rest = l;
                for _ in 0..4 {
                    rest = rest.split_once(char::is_whitespace)?.1.trim_start();
                }
                match rest.strip_prefix('"') {
                    Some(quoted) => quoted.split('"').next().map(str::to_string),
                    None => rest.split_whitespace().next().map(str::to_string),
                }
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// Title as people say it: release tags like "(World) (Rev 1)" and "[!]" dropped, and a trailing
/// article moved to the front ("Legend of Zelda, The - Link's Awakening" → "The Legend of Zelda - …").
pub fn display_name(stem: &str) -> String {
    let stem = strip_inner_ext(stem);
    // "Legend of Zelda,The-Oracle of Ages": the article after a comma with no space, and a dash with none either.
    let glued = fix_glued_article(stem);
    let stem = glued.as_str();
    let mut out = String::new();
    let mut depth = 0;
    for ch in stem.chars() {
        match ch {
            '(' | '[' => depth += 1,
            ')' | ']' => depth = (depth - 1).max(0),
            _ if depth == 0 => out.push(ch),
            _ => {}
        }
    }
    let name = out.split_whitespace().collect::<Vec<_>>().join(" ");
    let name = split_version(&name).0.to_string();
    // A disc named by its serial ("SLES_556.73.Something"): the serial in front goes, the title stays (`disc_serial`).
    let name = crate::disc_serial::clean_stem(&name);
    let (head, tail) = match name.split_once(" - ") {
        Some((h, t)) => (h.to_string(), format!(" - {t}")),
        None => (name.clone(), String::new()),
    };
    for article in ["The", "A", "An", "Die", "Der", "Das"] {
        if let Some(base) = head.strip_suffix(&format!(", {article}")) {
            return format!("{article} {base}{tail}");
        }
    }
    if name.is_empty() { stem.to_string() } else { name }
}

/// "Zelda,The-Oracle" → "Zelda, The - Oracle": a trailing article written without the spaces around it. Names that
/// don't have one are returned as they are (the hyphen of "Spider-Man" stays).
fn fix_glued_article(stem: &str) -> String {
    for article in ["The", "A", "An"] {
        for sep in [",", ", "] {
            let Some(at) = stem.find(&format!("{sep}{article}")).filter(|&at| at > 0) else { continue };
            let after = &stem[at + sep.len() + article.len()..];
            let glued_dash = after.starts_with('-');
            let ends_there = after.is_empty() || after.starts_with(" -") || after.starts_with(" (") || after.starts_with(" [");
            // Something must be glued, or the name is fine as it is; and the article must really end there ("Anna").
            if !(sep == "," || glued_dash) || !(glued_dash || ends_there) {
                continue;
            }
            let rest = if glued_dash { format!(" - {}", after[1..].trim_start()) } else { after.to_string() };
            return format!("{}, {article}{rest}", &stem[..at]);
        }
    }
    stem.to_string()
}

/// Small words that stay lower case in a title made by [`title_case`], unless they are its first word or the first
/// after " - " or ":".
const TITLE_SMALL_WORDS: &[&str] = &["a", "an", "and", "as", "at", "by", "for", "from", "in", "of", "on", "or", "the", "to", "vs", "with"];
/// Roman numerals, upper case in a title made by [`title_case`] ("Super Mario Land II").
const TITLE_ROMAN: &[&str] = &["i", "ii", "iii", "iv", "v", "vi", "vii", "viii", "ix", "x"];

/// A name for the shelf, cased when the file name had no capital letter at all: "libbet and the magic floor" is
/// "Libbet and the Magic Floor", "concentration room" is "Concentration Room". A name with any capital letter stays as
/// it is ("Super mario" is not changed), and so does a word with a digit ("sf2ce", "2x"). Only the display uses this;
/// the matching of games keeps the name as the file gives it (`display_name`).
pub fn title_case(name: &str) -> String {
    if name.chars().any(char::is_uppercase) {
        return name.to_string();
    }
    let mut words = Vec::new();
    let mut phrase_start = true;
    for token in name.split_whitespace() {
        let bare = token.trim_matches(|c: char| !c.is_alphanumeric());
        let cased = if token.chars().any(|c| c.is_ascii_digit()) {
            token.to_string()
        } else if !phrase_start && TITLE_SMALL_WORDS.contains(&bare) {
            token.to_string()
        } else if TITLE_ROMAN.contains(&bare) {
            token.to_uppercase()
        } else {
            capitalise(token)
        };
        phrase_start = token == "-" || token.ends_with(':');
        words.push(cased);
    }
    words.join(" ")
}

/// `token` with its first letter capitalised, and the first letter after each hyphen ("spider-man" → "Spider-Man").
fn capitalise(token: &str) -> String {
    let mut out = String::with_capacity(token.len());
    let mut word_start = true;
    for c in token.chars() {
        if word_start && c.is_alphabetic() {
            out.extend(c.to_uppercase());
            word_start = false;
        } else {
            out.push(c);
            if c == '-' {
                word_start = true;
            } else if c.is_alphabetic() {
                word_start = false;
            }
        }
    }
    out
}

/// A version at the end of a name, the way homebrew and scene files carry it: "Pipes.v1.02" is ("Pipes",
/// Some("v1.02")). Taken off after a dot, underscore or dash ("_v1.2", "-v3"), and after a space when it has a dot
/// ("Game V1.0") or a small v ("Game v2"); "Robo V2" (a capital V and a whole number after a space) may be the title
/// and stays, and a name that is nothing but a version stays as it is.
pub fn split_version(name: &str) -> (&str, Option<&str>) {
    let Some(v) = name.rfind(['v', 'V']) else { return (name, None) };
    let number = &name[v + 1..];
    let digits_and_dots = !number.is_empty()
        && number.split('.').all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()));
    let Some(sep) = name[..v].chars().next_back() else { return (name, None) };
    let taken = digits_and_dots
        && match sep {
            '.' | '_' | '-' => true,
            ' ' => number.contains('.') || name[v..].starts_with('v'),
            _ => false,
        };
    let head = name[..v].trim_end_matches([' ', '.', '_', '-']);
    if !taken || !head.chars().any(char::is_alphanumeric) {
        return (name, None);
    }
    (head, Some(&name[v..]))
}

/// What `display_name` leaves out of a name, in the order it stands: the version ("v1.02") and the tags, each with
/// its brackets ("(Europe)", "[b1]"). Two games of a system with the same name get back what tells them apart
/// (`tell_apart`).
pub fn name_extras(stem: &str) -> Vec<String> {
    let stem = strip_inner_ext(stem);
    let (mut tags, mut plain) = (Vec::new(), String::new());
    let (mut depth, mut tag) = (0, String::new());
    for ch in stem.chars() {
        match ch {
            '(' | '[' => {
                depth += 1;
                tag.push(ch);
            }
            ')' | ']' if depth > 0 => {
                depth -= 1;
                tag.push(ch);
                if depth == 0 {
                    tags.push(std::mem::take(&mut tag));
                }
            }
            _ if depth == 0 => plain.push(ch),
            _ => tag.push(ch),
        }
    }
    let plain = plain.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut out: Vec<String> = split_version(&plain).1.map(str::to_string).into_iter().collect();
    out.extend(tags.into_iter().map(|t| t.split_whitespace().collect::<Vec<_>>().join(" ")).filter(|t| t.len() > 2));
    out
}

/// What a game's name was made from: its file name without the extension, or for a game named after its folder
/// (a folder per game, `scan`; a DOS folder) that folder's name.
fn name_source(g: &Game) -> String {
    let stem = g.path.file_stem().and_then(|s| s.to_str()).unwrap_or_default();
    if display_name(stem) == g.name {
        return stem.to_string();
    }
    let folder = g.path.parent().and_then(|p| p.file_name()).and_then(|n| n.to_str()).unwrap_or_default();
    if display_name(folder) == g.name {
        folder.to_string()
    } else {
        stem.to_string()
    }
}

/// The ways `tell_apart` may lengthen a game's name, in its order: what `display_name` left out of it, then
/// the file type ("(ZIP)"), then the folder it is in ("(Hacks)").
fn name_suffixes(g: &Game) -> [Vec<String>; 3] {
    let ext = g.path.extension().and_then(|e| e.to_str());
    let folder = g.path.parent().and_then(|p| p.file_name()).and_then(|n| n.to_str());
    [
        name_extras(&name_source(g)),
        ext.map(|e| format!("({})", e.to_uppercase())).into_iter().collect(),
        folder.map(|f| format!("({f})")).into_iter().collect(),
    ]
}

/// Games of one system with the same name ("Tobu Tobu Girl (World)" and "(Europe)" are both "Tobu Tobu Girl") get
/// back what tells them apart: first the tags and version only one of them has ("Tobu Tobu Girl (Europe)"), then,
/// if that is not enough, the file type and the folder. `games` is sorted by name; each group keeps its place, its
/// games ordered by their new names, and every other game is left as it is. Only the name changes: what is kept
/// about a game (favourites, play time, covers, Curator) goes by its path.
fn tell_apart(games: &mut [Game]) {
    let same = |a: &str, b: &str| a.len() == b.len() && a.chars().flat_map(char::to_lowercase).eq(b.chars().flat_map(char::to_lowercase));
    let mut start = 0;
    while start < games.len() {
        let end = start + games[start..].iter().take_while(|g| same(&g.name, &games[start].name)).count();
        if end - start > 1 {
            let group = &mut games[start..end];
            let suffixes: Vec<[Vec<String>; 3]> = group.iter().map(name_suffixes).collect();
            let mut names: Vec<String> = group.iter().map(|g| g.name.clone()).collect();
            // `level` picks the same kind of suffix of every game of the group.
            #[allow(clippy::needless_range_loop)]
            for level in 0..3 {
                let unique = |names: &[String]| {
                    let mut seen = std::collections::HashSet::new();
                    names.iter().all(|n| seen.insert(n.to_lowercase()))
                };
                if unique(&names) {
                    break;
                }
                let lower: Vec<String> = names.iter().map(|n| n.to_lowercase()).collect();
                let mut next = names.clone();
                for (i, name) in names.iter().enumerate() {
                    // Only names that are still doubled grow, and only by what the others of the double lack.
                    let twins: Vec<usize> = (0..names.len()).filter(|&j| lower[j] == lower[i]).collect();
                    if twins.len() < 2 {
                        continue;
                    }
                    let own: Vec<&String> =
                        suffixes[i][level].iter().filter(|s| !twins.iter().all(|&j| suffixes[j][level].contains(s))).collect();
                    if !own.is_empty() {
                        next[i] = format!("{name} {}", own.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(" "));
                    }
                }
                names = next;
            }
            let mut order: Vec<usize> = (0..group.len()).collect();
            order.sort_by(|&a, &b| names[a].to_lowercase().cmp(&names[b].to_lowercase()).then_with(|| group[a].path.cmp(&group[b].path)));
            let sorted: Vec<Game> = order.iter().map(|&i| Game { name: names[i].clone(), path: group[i].path.clone() }).collect();
            group.clone_from_slice(&sorted);
        }
        start = end;
    }
}

/// Drops a second extension that is part of the format, e.g. "Game (USA).nkit" from "Game (USA).nkit.iso".
pub fn strip_inner_ext(stem: &str) -> &str {
    [".nkit", ".NKIT", ".p8"].iter().find_map(|e| stem.strip_suffix(e)).unwrap_or(stem)
}

#[cfg(test)]
mod tests {
    use super::{all_systems, display_name, SysInfo};
    use crate::testutil::TestDir;

    #[test]
    fn all_systems_lists_every_game_system_without_games() {
        let dir = TestDir::new("allsys");
        let cfg = dir.join("es_systems.cfg");
        std::fs::write(
            &cfg,
            r#"<systemList>
  <system><name>tools</name><fullname>Tools</fullname><path>/storage/roms/tools</path><extension>.sh</extension></system>
  <system><name>snes</name><fullname>Super Nintendo</fullname><path>/storage/roms/snes</path><extension>.smc .SFC .sfc .zip</extension></system>
  <system><name>gba</name><fullname>Game Boy Advance</fullname><path>/storage/roms/gba</path><extension>.gba .GBA .zip .7z</extension></system>
  <system><name>sfc</name><fullname>Super Famicom</fullname><path>/storage/roms/sfc</path></system>
</systemList>"#,
        )
        .unwrap();
        let all = all_systems(&cfg).unwrap();
        assert_eq!(
            all,
            vec![
                SysInfo { id: "gba".into(), name: "Game Boy Advance".into(), folder: "roms/gba".into(), path: "/storage/roms/gba".into(), extensions: vec!["gba".into(), "zip".into(), "7z".into()] },
                SysInfo { id: "sfc".into(), name: "Super Famicom".into(), folder: "roms/sfc".into(), path: "/storage/roms/sfc".into(), extensions: vec![] },
                SysInfo { id: "snes".into(), name: "Super Nintendo".into(), folder: "roms/snes".into(), path: "/storage/roms/snes".into(), extensions: vec!["smc".into(), "sfc".into(), "zip".into()] },
            ],
            "no folder needed, Tools left out, extensions lowercase and once"
        );
    }

    /// EmulationStation's gamelist.xml in a ROM folder: a cryptic file name gets the name it gives, a file name that
    /// reads as a name keeps pocket-ui's own, and a list made without gamelists (`load`) is what it was.
    #[test]
    fn a_gamelist_names_a_cryptic_file_and_leaves_the_other_names_alone() {
        use super::{load, load_with, Gamelists};
        let dir = TestDir::new("gamelist-lib");
        let roms = dir.join("arcade");
        std::fs::create_dir_all(&roms).unwrap();
        for f in ["sf2ce.zip", "kof98.zip", "Tetris (World).zip", "mslug.zip"] {
            std::fs::write(roms.join(f), b"x").unwrap();
        }
        std::fs::write(
            roms.join("gamelist.xml"),
            "<gameList><game><path>./sf2ce.zip</path><name>Street Fighter II' &amp; Co</name></game>\
             <game><path>./kof98.zip</path><name>kof98</name></game>\
             <game><path>./Tetris (World).zip</path><name>Tetris The Soviet Mind Game</name></game>\
             <game><path>./mslug.zip</path><name>Metal Slug</name></game></gameList>",
        )
        .unwrap();
        let cfg = dir.join("es_systems.cfg");
        std::fs::write(&cfg, format!("<systemList><system><name>arcade</name><fullname>Arcade</fullname><path>{}</path><extension>.zip</extension></system></systemList>", roms.display())).unwrap();
        let names = |l: Vec<super::System>| l[0].games.iter().map(|g| g.name.clone()).collect::<Vec<_>>();
        assert_eq!(names(load(&cfg).unwrap()), ["kof98", "Mslug", "sf2ce", "Tetris"], "without gamelists: a lowercase file name is cased for the shelf");
        let places = crate::gamelist::Locations::none();
        let data = dir.join("data");
        let with = load_with(&cfg, Some(&Gamelists { data_dir: &data, places: &places })).unwrap();
        assert_eq!(names(with), ["kof98", "Metal Slug", "Street Fighter II' & Co", "Tetris"], "sorted by the new names; the same name as the file and a real name stay");
        // A broken file changes nothing and breaks nothing.
        std::fs::write(roms.join("gamelist.xml"), b"\xff\xfe<<<not xml & garbage").unwrap();
        let broken = load_with(&cfg, Some(&Gamelists { data_dir: &data, places: &places })).unwrap();
        assert_eq!(names(broken), ["kof98", "Mslug", "sf2ce", "Tetris"]);
    }

    /// PlayStation 1/2 discs: the shelf shows the title the disc's serial gave (`disc_serial`), a gamelist's name
    /// comes first, and a disc no list knows keeps its file name without the serial in front.
    #[test]
    fn a_disc_shows_the_title_of_its_serial_and_a_gamelist_name_comes_first() {
        use super::{load_with, Gamelists};
        let dir = TestDir::new("disc-lib");
        let roms = dir.join("psx");
        std::fs::create_dir_all(&roms).unwrap();
        let cnf = |serial: &str| crate::disc_serial::test_image(&format!("BOOT = cdrom:\\{serial};1\r\nTCB = 4\r\n"), 2048);
        let files = [
            ("SLES_556.73.Something.iso", cnf("SLES_556.73")), // the list knows it: "Final Fantasy VII"
            ("SLES_556.74.iso", cnf("SLES_556.74")),           // its second disc: "... (Disc 2)"
            ("SCES_003.44.Crash Bandicoot.iso", cnf("SCES_003.44")), // a title in the file name too
            ("SCUS_999.99.iso", cnf("SCUS_999.99")),           // the gamelist names this one
            ("SCES_999.98.iso", cnf("SCES_999.98")),           // a serial no list knows
        ];
        for (name, bytes) in &files {
            std::fs::write(roms.join(name), bytes).unwrap();
        }
        // A gamelist's name is the player's own: it comes before the serial's title.
        std::fs::write(
            roms.join("gamelist.xml"),
            "<gameList><game><path>./SCUS_999.99.iso</path><name>My own disc name</name></game></gameList>",
        )
        .unwrap();
        let cfg = dir.join("es_systems.cfg");
        std::fs::write(&cfg, format!("<systemList><system><name>psx</name><fullname>PlayStation</fullname><path>{}</path><extension>.iso</extension></system></systemList>", roms.display())).unwrap();
        let data = dir.join("data");
        let games: Vec<(String, std::path::PathBuf)> = files.iter().map(|(name, _)| ("psx".to_string(), roms.join(name))).collect();
        let dat = "clrmamepro (\n\tname \"Sony - PlayStation\"\n)\n\
                   game (\n\tname \"Final Fantasy VII (Europe)\"\n\tserial \"SLES-55673\"\n)\n\
                   game (\n\tname \"Final Fantasy VII (Europe) (Disc 2)\"\n\tserial \"SLES-55674\"\n)\n\
                   game (\n\tname \"Crash Bandicoot (Europe)\"\n\tserial \"SCES-00344\"\n)\n";
        let run = crate::rom_id::run_until(&data, &games, &|_| Some(dat.to_string()), true, || false);
        assert_eq!(run.names.len(), 3, "three discs named, the two unknown serials not");
        let names = |l: Vec<super::System>| l[0].games.iter().map(|g| g.name.clone()).collect::<Vec<_>>();
        let places = crate::gamelist::Locations::none();
        let with = load_with(&cfg, Some(&Gamelists { data_dir: &data, places: &places })).unwrap();
        assert_eq!(
            names(with),
            ["Crash Bandicoot", "Final Fantasy VII", "Final Fantasy VII (Disc 2)", "My own disc name", "SCES_999.98"],
            "the serial's title (without its region, with its disc), the gamelist's name, and the file name of the disc no list knows"
        );
        // Before the serial was read (or without the list), the file name stays, without the serial in front.
        let empty = dir.join("fresh-data");
        let plain = load_with(&cfg, Some(&Gamelists { data_dir: &empty, places: &places })).unwrap();
        assert_eq!(names(plain), ["Crash Bandicoot", "My own disc name", "SCES_999.98", "SLES_556.74", "Something"]);
        // Two discs of one game that share a title keep what tells them apart, and a system that is no disc system is
        // left to its names (`disc_serial::dat`).
        assert_eq!(crate::disc_serial::dat("ps2"), Some("Sony - PlayStation 2"));
        assert_eq!(crate::disc_serial::dat("gba"), None);
    }

    /// New-user test 2026-10-30: "Supar Mario Land 2" got the right cover (by checksum) but kept the typo as its title,
    /// and a file called "Metroid - Fusoin" that is Advance Wars showed Metroid's name over Advance Wars' box.
    #[test]
    fn a_cartridge_the_checksum_names_shows_its_proper_title_not_the_typo() {
        use super::{load_with, Gamelists};
        super::names_ready(true);
        let dir = TestDir::new("cart-names");
        let roms = dir.join("gba");
        std::fs::create_dir_all(&roms).unwrap();
        let files: [(&str, &[u8]); 4] = [
            ("Supar Mario Land 2.gba", b"mario-land-two"),
            ("Metroid - Fusoin (Europe).gba", b"advance-wars-content"),
            ("Tetris (World).gba", b"tetris-bytes"),
            ("Homebrew Thing.gba", b"nobody knows this"),
        ];
        for (name, bytes) in &files {
            std::fs::write(roms.join(name), bytes).unwrap();
        }
        let cfg = dir.join("es_systems.cfg");
        std::fs::write(&cfg, format!("<systemList><system><name>gba</name><fullname>Game Boy Advance</fullname><path>{}</path><extension>.gba</extension></system></systemList>", roms.display())).unwrap();
        let data = dir.join("data");
        let games: Vec<(String, std::path::PathBuf)> = files.iter().map(|(name, _)| ("gba".to_string(), roms.join(name))).collect();
        let crc = |name: &str| format!("{:08X}", crate::rom_id::checksums("gba", &roms.join(name)).unwrap().unwrap().crc);
        let dat = format!(
            "clrmamepro (\n\tname \"Nintendo - Game Boy Advance\"\n)\n\
             game (\n\tname \"Super Mario Land 2 (USA)\"\n\trom ( name \"a.gba\" size 1 crc {} )\n)\n\
             game (\n\tname \"Advance Wars (USA)\"\n\trom ( name \"b.gba\" size 1 crc {} )\n)\n\
             game (\n\tname \"Tetris (World)\"\n\trom ( name \"c.gba\" size 1 crc {} )\n)\n",
            crc("Supar Mario Land 2.gba"),
            crc("Metroid - Fusoin (Europe).gba"),
            crc("Tetris (World).gba"),
        );
        let run = crate::rom_id::run_until(&data, &games, &|_| Some(dat.clone()), true, || false);
        assert_eq!(run.names.len(), 3);
        let names = |l: Vec<super::System>| l[0].games.iter().map(|g| g.name.clone()).collect::<Vec<_>>();
        let places = crate::gamelist::Locations::none();
        let with = load_with(&cfg, Some(&Gamelists { data_dir: &data, places: &places })).unwrap();
        assert_eq!(names(with), ["Advance Wars", "Homebrew Thing", "Super Mario Land 2", "Tetris"]);
        // Before the checksum is worked out, the file names show.
        let fresh = dir.join("fresh-data");
        let plain = load_with(&cfg, Some(&Gamelists { data_dir: &fresh, places: &places })).unwrap();
        assert_eq!(names(plain), ["Homebrew Thing", "Metroid - Fusoin", "Supar Mario Land 2", "Tetris"]);
    }

    #[test]
    fn an_article_glued_to_its_title_by_missing_spaces_is_still_moved_to_the_front() {
        assert_eq!(display_name("Legend of Zelda,The-Oracle of Ages"), "The Legend of Zelda - Oracle of Ages");
        assert_eq!(display_name("Legend of Zelda,The-Oracle of Ages (USA)"), "The Legend of Zelda - Oracle of Ages");
        assert_eq!(display_name("Legend of Zelda, The-Oracle of Seasons"), "The Legend of Zelda - Oracle of Seasons");
        assert_eq!(display_name("Legend of Zelda,The"), "The Legend of Zelda");
        assert_eq!(display_name("Spider-Man"), "Spider-Man");
        assert_eq!(display_name("Hunter,Anna-X"), "Hunter,Anna-X", "an article must end where it ends");
    }

    #[test]
    fn title_case_names_a_file_name_without_capitals_and_leaves_every_other_name_alone() {
        use super::title_case;
        let cases = [
            // The two names from the real run.
            ("libbet and the magic floor", "Libbet and the Magic Floor"),
            ("concentration room", "Concentration Room"),
            // Small words stay small, except first and after a subtitle dash or colon.
            ("a link to the past", "A Link to the Past"),
            ("lord of the rings - the two towers", "Lord of the Rings - The Two Towers"),
            ("legend of zelda: link's awakening", "Legend of Zelda: Link's Awakening"),
            ("in and out", "In and Out"),
            // Roman numerals stay upper case.
            ("super mario land ii", "Super Mario Land II"),
            ("final fantasy iv", "Final Fantasy IV"),
            // Words with a digit are kept as they are.
            ("sf2ce", "sf2ce"),
            ("tetris 2x", "Tetris 2x"),
            // Hyphens start a new word, punctuation stays.
            ("spider-man", "Spider-Man"),
            ("dr. mario & friends", "Dr. Mario & Friends"),
            // A name with any capital letter is left alone, and so is an empty one.
            ("Super mario", "Super mario"),
            ("Tetris", "Tetris"),
            ("", ""),
        ];
        for (name, cased) in cases {
            assert_eq!(title_case(name), cased, "{name:?}");
        }
    }

    #[test]
    fn a_lowercase_file_shows_cased_and_a_gamelist_name_is_kept_as_it_is() {
        use super::{load_with, Gamelists};
        let dir = TestDir::new("title-lib");
        let roms = dir.join("gb");
        std::fs::create_dir_all(&roms).unwrap();
        for f in ["libbet and the magic floor.gb", "concentration room.gb", "sf2ce.gb"] {
            std::fs::write(roms.join(f), b"x").unwrap();
        }
        std::fs::write(
            roms.join("gamelist.xml"),
            "<gameList><game><path>./sf2ce.gb</path><name>street fighter ii</name></game></gameList>",
        )
        .unwrap();
        let cfg = dir.join("es_systems.cfg");
        std::fs::write(&cfg, format!("<systemList><system><name>gb</name><fullname>Game Boy</fullname><path>{}</path><extension>.gb</extension></system></systemList>", roms.display())).unwrap();
        let names = |l: Vec<super::System>| l[0].games.iter().map(|g| g.name.clone()).collect::<Vec<_>>();
        let places = crate::gamelist::Locations::none();
        let data = dir.join("data");
        let with = load_with(&cfg, Some(&Gamelists { data_dir: &data, places: &places })).unwrap();
        assert_eq!(names(with), ["Concentration Room", "Libbet and the Magic Floor", "street fighter ii"], "the gamelist's name is not cased");
    }

    #[test]
    fn moonlights_scripts_are_not_games_on_the_shelf() {
        let dir = TestDir::new("moonlight-lib");
        for (sys, file) in [("moonlight", "Steam.sh"), ("gba", "Game.gba")] {
            std::fs::create_dir_all(dir.join(sys)).unwrap();
            std::fs::write(dir.join(sys).join(file), b"x").unwrap();
        }
        let cfg = dir.join("es_systems.cfg");
        let entry = |name: &str, ext: &str| format!("<system><name>{name}</name><path>{}</path><extension>{ext}</extension></system>", dir.join(name).display());
        std::fs::write(&cfg, format!("<systemList>{}{}</systemList>", entry("moonlight", ".sh"), entry("gba", ".gba"))).unwrap();
        let names: Vec<String> = super::load(&cfg).unwrap().into_iter().map(|s| s.name).collect();
        assert_eq!(names, vec!["gba".to_string()]);
        assert_eq!(all_systems(&cfg).unwrap().len(), 1, "nor a tile in Add games");
    }

    /// VDEV-ROUND-2026-10-25: ROCKNIX's es_systems.cfg has "imageviewer" ("Screenshots") on the folder HOME + A writes
    /// to, with .png among its extensions: the screenshots were a system of "games" on Home after the next scan.
    #[test]
    fn screenshots_are_not_a_system_of_games() {
        let dir = TestDir::new("imageviewer-lib");
        for (sys, file) in [("screenshots/gba", "Game 2026-10-25 18-10-00.png"), ("gba", "Game.gba")] {
            std::fs::create_dir_all(dir.join(sys)).unwrap();
            std::fs::write(dir.join(sys).join(file), b"x").unwrap();
        }
        let cfg = dir.join("es_systems.cfg");
        let entry = |name: &str, folder: &str, ext: &str| format!("<system><name>{name}</name><fullname>Screenshots</fullname><path>{}</path><extension>{ext}</extension></system>", dir.join(folder).display());
        std::fs::write(&cfg, format!("<systemList>{}{}</systemList>", entry("imageviewer", "screenshots", ".jpg .png .mp4"), entry("gba", "gba", ".gba"))).unwrap();
        let names: Vec<String> = super::load(&cfg).unwrap().into_iter().map(|s| s.name).collect();
        assert_eq!(names, vec!["gba".to_string()]);
        assert_eq!(all_systems(&cfg).unwrap().len(), 1, "nor a tile in Add games");
    }

    #[test]
    fn hidden_games_leave_the_list_and_a_system_left_empty_goes_with_them() {
        let dir = TestDir::new("hidden-lib");
        for (sys, file) in [("gba", "Alpha.gba"), ("gba", "Beta.gba"), ("gb", "Solo.gb")] {
            std::fs::create_dir_all(dir.join(sys)).unwrap();
            std::fs::write(dir.join(sys).join(file), b"x").unwrap();
        }
        let cfg = dir.join("es_systems.cfg");
        let entry = |name: &str, ext: &str| format!("<system><name>{name}</name><path>{}</path><extension>{ext}</extension></system>", dir.join(name).display());
        std::fs::write(&cfg, format!("<systemList>{}{}</systemList>", entry("gba", ".gba"), entry("gb", ".gb"))).unwrap();
        let data = dir.join("data");
        assert_eq!(super::load_shown(&cfg, &data).unwrap(), super::load(&cfg).unwrap(), "nothing hidden, no library.json: the same list");
        let mut meta = crate::library_meta::LibraryMeta::load(&data);
        meta.set_hidden(&dir.join("gba/Beta.gba"), true).unwrap();
        meta.set_hidden(&dir.join("gb/Solo.gb"), true).unwrap();
        let shown = super::load_shown(&cfg, &data).unwrap();
        let names: Vec<(String, Vec<String>)> = shown.iter().map(|s| (s.name.clone(), s.games.iter().map(|g| g.name.clone()).collect())).collect();
        assert_eq!(names, vec![("gba".to_string(), vec!["Alpha".to_string()])], "Beta and the Game Boy (its only game) are gone");
        meta.set_hidden(&dir.join("gb/Solo.gb"), false).unwrap();
        assert_eq!(super::load_shown(&cfg, &data).unwrap().len(), 2, "shown again");
    }

    fn disc_dir(tag: &str, files: &[(&str, &str)]) -> crate::testutil::TestDir {
        let dir = TestDir::new(&format!("disc-{tag}"));
        for (name, text) in files {
            std::fs::write(dir.join(name), text).unwrap();
        }
        dir
    }

    fn names(games: &[super::Game]) -> Vec<String> {
        games.iter().map(|g| g.path.file_name().unwrap().to_str().unwrap().to_string()).collect()
    }

    fn exts(list: &[&str]) -> Vec<String> {
        list.iter().map(|e| e.to_string()).collect()
    }

    #[test]
    fn a_disc_image_a_cue_sheet_points_at_is_not_listed_twice() {
        // The Saturn homebrew of the virtual device: Twelve Snakes.cue + Twelve Snakes.iso are one game.
        let dir = disc_dir(
            "cue",
            &[
                ("Twelve Snakes.cue", "FILE \"Twelve Snakes.iso\" BINARY\n  TRACK 01 MODE1/2048\n    INDEX 01 00:00:00\n"),
                ("Twelve Snakes.iso", "x"),
                ("Lone Disc.iso", "x"),
                ("Other.chd", "x"),
            ],
        );
        let games = super::scan(&dir, &exts(&["cue", "iso", "chd"]));
        assert_eq!(names(&games), ["Lone Disc.iso", "Other.chd", "Twelve Snakes.cue"], "the sheet stays, the image it names goes, unreferenced files stay");
        assert_eq!(super::count_games("saturn", &dir, &exts(&["cue", "iso", "chd"])), 3, "Add games counts them the same way");
    }

    #[test]
    fn m3u_playlists_hide_their_discs_and_the_tracks_of_those() {
        let dir = disc_dir(
            "m3u",
            &[
                ("Game.m3u", "Game (Disc 1).cue\r\n# a comment\n\nGame (Disc 2).cue\n"),
                ("Game (Disc 1).cue", "FILE \"Game (Disc 1).bin\" BINARY\n  TRACK 01 MODE2/2352\n"),
                ("Game (Disc 2).cue", "FILE \"game (disc 2).BIN\" BINARY\n  TRACK 01 MODE2/2352\n"),
                ("Game (Disc 1).bin", "x"),
                ("Game (Disc 2).bin", "x"),
                ("Other.cue", "FILE \"Other.bin\" BINARY\n"),
                ("Other.bin", "x"),
            ],
        );
        let games = super::scan(&dir, &exts(&["m3u", "cue", "bin"]));
        assert_eq!(names(&games), ["Game.m3u", "Other.cue"], "the playlist hides its cues, and the cues (hidden or not) hide their bins; matching ignores case");
    }

    #[test]
    fn gdi_sheets_hide_their_tracks_and_a_sheet_the_system_does_not_list_hides_nothing() {
        let dir = disc_dir(
            "gdi",
            &[
                ("Crazy.gdi", "3\n1 0 4 2352 track01.bin 0\n2 756 0 2352 \"track 02.raw\" 0\n3 45000 4 2352 track03.bin 0\n"),
                ("track01.bin", "x"),
                ("track 02.raw", "x"),
                ("track03.bin", "x"),
                ("Solo.cue", "FILE \"Solo.bin\" BINARY\n"),
                ("Solo.bin", "x"),
            ],
        );
        let games = super::scan(&dir, &exts(&["gdi", "bin", "raw"]));
        assert_eq!(names(&games), ["Crazy.gdi", "Solo.bin"], ".cue is not a type of this system: nothing may vanish because of a file it cannot list");
    }

    /// A /storage/roms/psx with a folder per game.
    fn psx_folders(tag: &str) -> crate::testutil::TestDir {
        let dir = TestDir::new(&format!("psxdirs-{tag}"));
        let put = |rel: &str, text: &str| {
            let p = dir.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, text).unwrap();
        };
        put("Gran Turismo 2 Plus/Gran Turismo 2 Plus.bin", "x");
        put("Gran Turismo 2 Plus/Gran Turismo 2 Plus.cue", "FILE \"Gran Turismo 2 Plus.bin\" BINARY\n  TRACK 01 MODE2/2352\n    INDEX 01 00:00:00\n");
        put("Xenogears/Xenogears.m3u", "Xenogears_PW_CD1.cue\nXenogears_PW_CD2.cue\n");
        for cd in ["CD1", "CD2"] {
            put(&format!("Xenogears/Xenogears_PW_{cd}.bin"), "x");
            put(&format!("Xenogears/Xenogears_PW_{cd}.cue"), &format!("FILE \"Xenogears_PW_{cd}.bin\" BINARY\n"));
        }
        // Not games: a hidden folder, an import folder, nothing launchable, and a folder two levels down.
        put(".thumbnails/Junk.cue", "");
        put("_saves-import/Old.cue", "");
        put("Manuals/readme.txt", "x");
        put("Deep/Deeper/Hidden Game.cue", "");
        // Two unrelated games in one folder keep their own names; a loose game next to the folders stays as it is.
        put("Pack/Alpha.cue", "FILE \"Alpha.bin\" BINARY\n");
        put("Pack/Alpha.bin", "x");
        put("Pack/Beta.chd", "x");
        put("Loose.chd", "x");
        dir
    }

    #[test]
    fn game_folders_one_level_down_are_games() {
        let dir = psx_folders("scan");
        let games = super::scan(&dir, &exts(&["bin", "cue", "chd", "iso", "m3u", "pbp"]));
        let listed: Vec<(String, String)> = games
            .iter()
            .map(|g| (g.name.clone(), g.path.strip_prefix(&dir).unwrap().to_str().unwrap().to_string()))
            .collect();
        let want = [
            ("Alpha", "Pack/Alpha.cue"),
            ("Beta", "Pack/Beta.chd"),
            ("Gran Turismo 2 Plus", "Gran Turismo 2 Plus/Gran Turismo 2 Plus.cue"),
            ("Loose", "Loose.chd"),
            ("Xenogears", "Xenogears/Xenogears.m3u"),
        ];
        assert_eq!(listed, want.map(|(n, p)| (n.to_string(), p.to_string())), "one entry per game: its m3u, or its only disc file, named after the folder");
        assert_eq!(super::count_games("psx", &dir, &exts(&["bin", "cue", "chd", "iso", "m3u", "pbp"])), 5, "Add games counts the same");
    }

    #[test]
    fn folder_named_games_of_dos_and_scummvm_are_left_to_hostsys() {
        let dir = TestDir::new("pcdirs");
        std::fs::create_dir_all(dir.join("pc/Doom")).unwrap();
        std::fs::write(dir.join("pc/Doom/doom.zip"), b"x").unwrap();
        let cfg = dir.join("es_systems.cfg");
        std::fs::write(&cfg, format!("<systemList><system><name>pc</name><path>{}</path><extension>.zip</extension></system></systemList>", dir.join("pc").display())).unwrap();
        let systems = super::load(&cfg).unwrap();
        let paths: Vec<_> = systems[0].games.iter().map(|g| g.path.clone()).collect();
        assert_eq!(paths, [dir.join("pc/Doom")], "the folder once, started by DOSBox Pure, not also its zip");
    }

    #[test]
    fn cleans_release_names() {
        assert_eq!(display_name("Tetris (World) (Rev 1)"), "Tetris");
        assert_eq!(display_name("Legend of Zelda, The - Link's Awakening DX (USA, Europe) (Rev 2)"), "The Legend of Zelda - Link's Awakening DX");
        assert_eq!(display_name("Super Metroid (Japan, USA) (En,Ja) [!]"), "Super Metroid");
        assert_eq!(display_name("uCity"), "uCity");
        assert_eq!(display_name("(Proto)"), "(Proto)");
        assert_eq!(display_name("Mario Kart - Double Dash!! (USA).nkit"), "Mario Kart - Double Dash!!");
    }

    /// First-run test 2026-10-24, finding #17: "Pipes.v1.02.zip" was shown as "Pipes.v1.02".
    #[test]
    fn versions_at_the_end_of_a_name_are_left_out_and_titles_that_only_look_like_one_stay() {
        for (file, shown) in [
            ("Pipes.v1.02", "Pipes"),
            ("Pipes_v1.2", "Pipes"),
            ("Pipes-v3", "Pipes"),
            ("Pipes v1.02", "Pipes"),
            ("Pipes V1.0 (World)", "Pipes"),
            ("Pipes v2", "Pipes"),
            ("Legend of Zelda, The v1.1", "The Legend of Zelda"),
            ("uCity v1.3 [!]", "uCity"),
            // Titles: no number after the V, a capital V and a whole number after a space, a version alone.
            ("Final Fantasy V", "Final Fantasy V"),
            ("Mega Man V (USA)", "Mega Man V"),
            ("Robo V2", "Robo V2"),
            ("Revolution X", "Revolution X"),
            ("Driv3r", "Driv3r"),
            ("Elite v", "Elite v"),
            ("v1.0", "v1.0"),
            ("Tetris v1.x", "Tetris v1.x"),
            ("Dev2", "Dev2"),
        ] {
            assert_eq!(display_name(file), shown, "{file}");
        }
        assert_eq!(super::split_version("Pipes.v1.02"), ("Pipes", Some("v1.02")));
        assert_eq!(super::name_extras("Pipes.v1.02 (World) [b1]"), ["v1.02", "(World)", "[b1]"]);
    }

    fn put(dir: &std::path::Path, rel: &str, text: &str) {
        let p = dir.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }

    /// First-run test 2026-10-24, finding #8: "Tobu Tobu Girl (World)" and "(Europe)" were two rows "Tobu Tobu Girl".
    #[test]
    fn games_of_the_same_name_get_back_what_tells_them_apart_and_the_rest_of_the_list_stays() {
        let dir = TestDir::new("twins");
        for f in [
            "Alpha.gb",
            "Tobu Tobu Girl (World).gb",
            "Tobu Tobu Girl (Europe).gb",
            "Tobu Tobu Girl 2.gb",
            "Pipes.v1.02.gb",
            "Pipes.v1.03.gb",
            "Game (USA).gb",
            "Game (USA) (Rev 1).gb",
            "Same (World).gb",
            "Same (World).zip",
            "Zeta.gb",
            // Two folders with several games each: the same file name twice, told apart by the folder.
            "Pack A/Tetris.gb",
            "Pack A/Other A.gb",
            "Pack B/Tetris.gb",
            "Pack B/Other B.gb",
        ] {
            put(&dir, &format!("gb/{f}"), "x");
        }
        let cfg = dir.join("es_systems.cfg");
        std::fs::write(&cfg, format!("<systemList><system><name>gb</name><path>{}</path><extension>.gb .zip</extension></system></systemList>", dir.join("gb").display())).unwrap();
        let systems = super::load(&cfg).unwrap();
        let listed: Vec<(String, String)> =
            systems[0].games.iter().map(|g| (g.name.clone(), g.path.strip_prefix(dir.join("gb")).unwrap().to_str().unwrap().to_string())).collect();
        let want = [
            ("Alpha", "Alpha.gb"),
            ("Game", "Game (USA).gb"),
            ("Game (Rev 1)", "Game (USA) (Rev 1).gb"),
            ("Other A", "Pack A/Other A.gb"),
            ("Other B", "Pack B/Other B.gb"),
            ("Pipes v1.02", "Pipes.v1.02.gb"),
            ("Pipes v1.03", "Pipes.v1.03.gb"),
            ("Same (GB)", "Same (World).gb"),
            ("Same (ZIP)", "Same (World).zip"),
            ("Tetris (Pack A)", "Pack A/Tetris.gb"),
            ("Tetris (Pack B)", "Pack B/Tetris.gb"),
            ("Tobu Tobu Girl (Europe)", "Tobu Tobu Girl (Europe).gb"),
            ("Tobu Tobu Girl (World)", "Tobu Tobu Girl (World).gb"),
            ("Tobu Tobu Girl 2", "Tobu Tobu Girl 2.gb"),
            ("Zeta", "Zeta.gb"),
        ];
        assert_eq!(listed, want.map(|(n, p)| (n.to_string(), p.to_string())), "each double lengthened by what it alone has, in its place");
    }

    /// First-run test 2026-10-24, finding #11: a .cue whose .bin is missing was listed as a game and failed to start.
    #[test]
    fn a_disc_whose_track_file_is_missing_is_left_out_and_reported() {
        let dir = disc_dir(
            "lost",
            &[
                ("Lost Bin (Europe).cue", "FILE \"Lost Bin (Europe).bin\" BINARY\n  TRACK 01 MODE2/2352\n"),
                ("Here.cue", "FILE \"Here.bin\" BINARY\n"),
                ("Here.bin", "x"),
                // Written on Windows: the case differs. Kept (and its bin is not a game of its own).
                ("Cased.cue", "FILE \"CASED.BIN\" BINARY\n"),
                ("cased.bin", "x"),
                // Says nothing, or names a file in a folder we don't look into: kept, nothing is known to be missing.
                ("Empty.cue", ""),
                ("Deep.cue", "FILE \"tracks/Deep.bin\" BINARY\n"),
                ("Two.gdi", "2\n1 0 4 2352 track01.bin 0\n2 756 0 2352 track02.raw 0\n"),
                ("track01.bin", "x"),
            ],
        );
        let types = exts(&["cue", "bin", "gdi", "raw"]);
        let games = super::scan(&dir, &types);
        assert_eq!(names(&games), ["Cased.cue", "Deep.cue", "Empty.cue", "Here.cue"], "the bins of the sheets are part of them; the broken sheets are gone");
        let (n, broken) = super::look_at_folder("psx", &dir, &types);
        assert_eq!(n, 4, "Add games counts what the shelf shows");
        let report: Vec<(String, String)> = broken.iter().map(|b| (b.sheet.file_name().unwrap().to_str().unwrap().to_string(), b.missing.clone())).collect();
        let mut report = report;
        report.sort();
        assert_eq!(report, [("Lost Bin (Europe).cue".to_string(), "Lost Bin (Europe).bin".to_string()), ("Two.gdi".to_string(), "track02.raw".to_string())]);
        // The track arrives (still copying): the game is there at the next look.
        std::fs::write(dir.join("Lost Bin (Europe).bin"), "x").unwrap();
        assert!(names(&super::scan(&dir, &types)).contains(&"Lost Bin (Europe).cue".to_string()));
    }

    /// First-run test 2026-10-24, finding #10: a "Sega Genesis" folder next to megadrive was ignored without a word.
    #[test]
    fn folders_of_no_system_are_found_with_the_folder_they_should_be() {
        let dir = TestDir::new("strays");
        let sys = |id: &str, name: &str, exts: &[&str]| SysInfo {
            id: id.into(),
            name: name.into(),
            folder: format!("roms/{id}"),
            path: dir.join(id),
            extensions: exts.iter().map(|e| e.to_string()).collect(),
        };
        let catalog = vec![
            sys("gba", "Game Boy Advance", &["gba", "zip"]),
            sys("megadrive", "Sega Mega Drive", &["md", "bin", "zip"]),
            sys("psx", "Sony PlayStation", &["cue", "chd"]),
            sys("snes", "Super Nintendo", &["sfc", "smc"]),
        ];
        put(&dir, "gba/Known.gba", "x");
        put(&dir, "Sega Genesis/Sonic.md", "x");
        put(&dir, "GBA/Upper.gba", "x");
        put(&dir, "PS1/Game/Game.cue", "x");
        put(&dir, "Super Nintendo/Mario.sfc", "x");
        put(&dir, "Atari Jaguar/game.zip", "x");
        // Not strays: ROCKNIX's own, hidden, "_", empty, nothing a system takes, games two levels down only.
        put(&dir, "bios/scph1001.bin", "x");
        put(&dir, "savestates/gba/Known.state", "x");
        put(&dir, ".Trash/Sonic.md", "x");
        put(&dir, "_import/Sonic.md", "x");
        std::fs::create_dir_all(dir.join("Empty")).unwrap();
        put(&dir, "Stuff/readme.txt", "x");
        put(&dir, "Deep/a/b/Game.gba", "x");
        let found: Vec<(String, Option<String>)> = super::stray_folders(&dir, &catalog).into_iter().map(|s| (s.name, s.meant)).collect();
        let want = [
            ("Atari Jaguar", None),
            ("GBA", Some("gba")),
            ("PS1", Some("psx")),
            ("Sega Genesis", Some("megadrive")),
            ("Super Nintendo", Some("snes")),
        ];
        assert_eq!(found, want.map(|(n, m)| (n.to_string(), m.map(str::to_string))));
        assert_eq!(super::roms_root(&catalog), Some(dir.to_path_buf()));
        assert!(super::stray_folders(&dir.join("missing"), &catalog).is_empty(), "no roms folder: nothing to report");
        // An alias only points at a folder this device has.
        assert_eq!(super::guess_folder("Dreamcast", &catalog), None);
    }
}
