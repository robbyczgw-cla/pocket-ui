//! Game check on Android: "will my games start?" from a document provider's listing. Nothing here writes, downloads or
//! deletes, and no game's content is read: a listing, the zips' directories and the disc sheets the scan already has.
//!
//! The shared checks are `game_check`'s (empty files, zips, the same game twice, wrong file types); what is Android's own:
//! the player's folder grants, which player takes a game (`Host::bundled`, the APK's cores, or an emulator app), and the
//! limits of the in-app player. A system's emulator app keeps its BIOS in its own private folder, and the in-app player's
//! cores need none a player could give them, so a BIOS is never reported missing here: nobody could know.
use crate::document_library::{self, Document};
use crate::game_check::{self, Files, Finding, Findings, Group, Report, Words};
use crate::library::SysInfo;
use serde::Deserialize;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::{Read, Seek};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

/// The largest game a core that gets it in memory takes: the biggest DS card. PPSSPP reads its file itself and has no such
/// limit. `GameCopy.LARGEST_IN_MEMORY`; change them together.
pub const MAX_IN_MEMORY: u64 = 512 << 20;
/// From here a first start copies for a while and the copy stays: `GameCopy.COPIED_EVERY_START`, `frozen::WHOLE_UP_TO`.
const BIG: u64 = 64 << 20;
/// What a big game's copy leaves free: `GameCopy.KEEP_FREE`.
const KEEP_FREE: u64 = 256 << 20;

/// What `PocketActivity.checkListing` returns.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct Listing {
    pub roots: Vec<Root>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RootState {
    Ok,
    /// Android no longer lets the app read it (the grant is gone).
    Lost,
    /// The folder is not there (moved, deleted, its card out).
    Gone,
    /// More than the 50,000 entries a scan reads.
    Big,
}

/// A folder the player chose.
#[derive(Clone, Debug, Deserialize)]
pub struct Root {
    pub uri: String,
    pub name: String,
    pub state: RootState,
    /// What the scan puts before this folder's paths, so that alike names in two folders stay apart.
    #[serde(default)]
    pub prefix: String,
    #[serde(default)]
    pub files: Vec<Listed>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Listed {
    pub uri: String,
    pub id: String,
    pub authority: String,
    pub relative: String,
    #[serde(default)]
    pub sheet: String,
    /// -1: the provider doesn't say.
    #[serde(default = "unknown_size")]
    pub size: i64,
}

fn unknown_size() -> i64 {
    -1
}

pub trait ReadSeek: Read + Seek {}
impl<T: Read + Seek> ReadSeek for T {}

/// What the check asks the phone.
pub trait Host {
    /// The APK has a core for this system.
    fn bundled(&self, system: &str) -> bool;
    /// An app that can take this launch profile is installed (`EmulatorIntents`).
    fn installed(&self, profile: &str) -> bool;
    /// The document, open for reading; None when it can't be, or not with seeking (a cloud provider's pipe).
    fn open(&self, uri: &str) -> Option<Box<dyn ReadSeek>>;
    /// The room a first copy of this big game has, in bytes; None when it has its copy already or the phone doesn't say.
    fn room_for_copy(&self, game: &document_library::Game) -> Option<u64>;
}

/// A chosen folder that can't be read: how to name it, and how to forget it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unreadable {
    pub name: String,
    pub uri: String,
}

#[derive(Default)]
pub struct Outcome {
    pub report: Report,
    pub unreadable: Vec<Unreadable>,
    /// No folder chosen yet.
    pub no_folder: bool,
    /// The folders as they are now, for the shelf.
    pub documents: Vec<Document>,
}

const PHONE: Words = Words {
    copy_again: "Copy the game into this folder again.",
    empty_todo: "A copy or a download stopped early. Copy the game into this folder again.",
    twin_todo: "Both copies are on your shelf. To keep one, delete the other with your phone's file manager: pocket-ui never deletes a game.",
    twin_hides: false,
    short: |info| info.name.clone(),
    popular: &["gb", "gbc", "gba", "nes", "snes", "megadrive", "psx", "n64", "psp", "nds", "ps2", "gamecube", "dreamcast"],
};

/// The listing behind `game_check`'s seam.
struct Listings<'a> {
    host: &'a dyn Host,
    /// Path as the scan has it: the document and its size.
    files: HashMap<&'a Path, (&'a str, i64)>,
    /// Prefix, the folder's name.
    roots: Vec<(&'a str, &'a str)>,
}

impl Files for Listings<'_> {
    fn size(&self, path: &Path) -> Option<u64> {
        self.files.get(path).and_then(|(_, size)| u64::try_from(*size).ok())
    }

    fn zip_refusal(&self, zip: &Path, wanted: &[&str], only_cut_off: bool) -> Option<String> {
        let name = zip.file_name().and_then(|n| n.to_str()).unwrap_or("The zip file");
        let mut open = self.host.open(self.files.get(zip)?.0)?;
        crate::archive::refusal_in(&mut open, name, wanted, only_cut_off)
    }

    fn open(&self, path: &Path) -> Option<Box<dyn ReadSeek>> {
        self.host.open(self.files.get(path)?.0)
    }

    /// "Games/snes/Game.sfc": the folder's name, not the prefix a second folder's paths have.
    fn shown(&self, path: &Path) -> String {
        let text = path.to_string_lossy();
        let rest = self.roots.iter().filter(|(prefix, _)| !prefix.is_empty()).find_map(|(prefix, name)| text.strip_prefix(prefix).map(|r| format!("{name}/{r}")));
        rest.unwrap_or_else(|| match self.roots.iter().find(|(prefix, _)| prefix.is_empty()) {
            Some((_, name)) => format!("{name}/{text}"),
            None => text.into_owned(),
        })
    }
}

/// The emulator app's name, for what a game's launch profile asks for. None: nothing can start it from here yet.
fn app_name(profile: &str) -> Option<&'static str> {
    Some(match profile.split(':').next().unwrap_or("") {
        "retroarch" => "RetroArch",
        "ps2" => "NetherSX2",
        "dolphin" => "Dolphin",
        "psp" => "PPSSPP",
        "psx" => "DuckStation",
        "3ds" => "Azahar",
        "switch" => "Eden",
        _ => return None,
    })
}

/// A system that doesn't start without a console's file of its own: its emulator app asks for it and keeps it, out of
/// this app's sight.
fn needs_own_files(system: &str) -> bool {
    matches!(system, "psx" | "ps2" | "segacd" | "switch")
}

/// The catalog `game_check` looks up file types in.
fn catalog() -> Vec<SysInfo> {
    document_library::SYSTEM_IDS
        .iter()
        .filter_map(|id| {
            let (_, name, _, _) = document_library::system(id)?;
            Some(SysInfo { id: id.to_string(), name: name.to_string(), folder: format!("{id}/"), path: PathBuf::from(id), extensions: document_library::takes(id).iter().map(|e| e.to_string()).collect() })
        })
        .collect()
}

/// A folder's name as a system's name: "Game Boy" and "game_boy" for "gameboy".
fn plain(name: &str) -> String {
    name.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

fn folder_of(relative: &str) -> String {
    Path::new(relative).parent().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default()
}

fn last_part(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// Goes through the folders. None: stopped.
pub fn check(listing: &Listing, host: &dyn Host, stop: &AtomicBool, progress: &mut dyn FnMut(usize, usize, &str)) -> Option<Outcome> {
    if listing.roots.is_empty() {
        return Some(Outcome { no_folder: true, ..Outcome::default() });
    }
    let mut found = Findings::default();
    let mut unreadable = Vec::new();
    for root in &listing.roots {
        let (what, todo) = match root.state {
            RootState::Ok => continue,
            RootState::Lost => (format!("Android no longer lets pocket-ui read {}.", root.name), "Your games are safe. Choose this folder again and they come back."),
            RootState::Gone => (format!("{} is not there any more.", root.name), "It was moved or renamed, or its memory card is out. Choose it again, or forget it here."),
            RootState::Big => (format!("{} holds more than 50,000 files, so it isn't read.", root.name), "Choose a smaller folder, the one that holds your games' folders."),
        };
        unreadable.push(Unreadable { name: root.name.clone(), uri: root.uri.clone() });
        found.add(Finding { group: Group::Access, name: root.name.clone(), system: String::new(), what, todo: todo.into(), place: None, fix: None });
    }
    let listed: Vec<&Listed> = listing.roots.iter().flat_map(|r| &r.files).collect();
    let documents: Vec<Document> = listed.iter().map(|f| Document { uri: f.uri.clone(), id: f.id.clone(), authority: f.authority.clone(), relative: f.relative.clone(), sheet: f.sheet.clone(), size: f.size }).collect();
    let games = document_library::scan(documents.clone());
    let total = games.len();
    let catalog = catalog();
    let files = Listings {
        host,
        files: listed.iter().map(|f| (Path::new(f.relative.as_str()), (f.uri.as_str(), f.size))).collect(),
        roots: listing.roots.iter().map(|r| (r.prefix.as_str(), r.name.as_str())).collect(),
    };
    let mut blocked: HashSet<PathBuf> = HashSet::new();

    // Discs: a sheet's files. A sheet another one names (a cue in a playlist) isn't on the shelf, but its game won't start.
    let sheet_files = document_library::sheet_files(&documents);
    let tracks: HashSet<&str> = sheet_files.iter().filter_map(|(_, _, path)| path.as_deref()).collect();
    let mut broken: HashSet<&str> = HashSet::new();
    let lower: HashMap<String, &str> = listed.iter().map(|f| (f.relative.to_lowercase(), f.relative.as_str())).collect();
    for (sheet, file, path) in &sheet_files {
        let sheet_name = last_part(sheet);
        let system = document_library::folder_system(sheet).map_or(String::new(), |s| s.1.to_string());
        let place = Some(files.shown(Path::new(sheet)));
        let name = game_check::name_of(Path::new(sheet));
        match path {
            None => {
                broken.insert(sheet);
                // The same name but for capitals: the sheet's spelling is what is looked for.
                let in_folder = format!("{}/{}", folder_of(sheet), file.replace('\\', "/")).trim_start_matches('/').to_lowercase();
                let todo = match lower.get(&in_folder) {
                    Some(real) => {
                        // Not this sheet's track for the shelf, so a game of its own there; it won't start either.
                        blocked.insert(PathBuf::from(real));
                        format!("Rename {} to {file}.", last_part(real))
                    }
                    None => format!("Put {file} next to it, with exactly this name."),
                };
                found.add(Finding { group: Group::Discs, name, system, what: format!("Disc: {sheet_name} is there, its {file} is missing."), todo, place, fix: None });
            }
            Some(path) if files.size(Path::new(path)) == Some(0) => {
                broken.insert(sheet);
                found.add(Finding { group: Group::Discs, name, system, what: format!("Disc: {sheet_name} is there, its {file} is empty."), todo: format!("Copy {file} into this folder again."), place, fix: None });
            }
            Some(_) => {}
        }
    }
    for (sheet, _, path) in &sheet_files {
        if broken.contains(sheet.as_str()) || path.as_deref().is_some_and(|p| broken.contains(p)) {
            blocked.insert(PathBuf::from(sheet));
        }
    }

    // Files in a place or of a type that won't start.
    let on_shelf: HashMap<&str, &document_library::Game> = games.iter().map(|g| (g.relative.as_str(), g)).collect();
    let all_types: Vec<String> = catalog.iter().flat_map(|s| s.extensions.iter().cloned()).collect::<std::collections::BTreeSet<_>>().into_iter().collect();
    let nowhere = SysInfo { id: String::new(), name: "pocket-ui".into(), folder: String::new(), path: PathBuf::new(), extensions: all_types.clone() };
    // Per folder that names no system: the games the shelf can't place. An ordered map, so the findings don't depend on the listing.
    let mut unplaced: BTreeMap<String, usize> = BTreeMap::new();
    let mut bios_folder = false;
    let mut right_type: BTreeMap<&str, Vec<PathBuf>> = BTreeMap::new();
    for (n, f) in listed.iter().enumerate() {
        game_check::breath(stop, n)?;
        let relative = f.relative.as_str();
        let path = Path::new(relative);
        let folders = path.parent().map_or(Vec::new(), |p| p.iter().map(|c| c.to_string_lossy().to_lowercase()).collect::<Vec<_>>());
        if document_library::in_bios_folder(relative) {
            bios_folder = true;
            continue;
        }
        // An emulator's or a scraper's folder; "psp" is one of those beside a handheld's games, and a system's folder here.
        let in_data = folders.iter().any(|c| c.starts_with('.') || (game_check::DATA_FOLDERS.contains(&c.as_str()) && document_library::system(c).is_none()));
        if in_data || last_part(relative).starts_with('.') || tracks.contains(relative) {
            continue;
        }
        let ext = game_check::extension(path);
        let game = on_shelf.get(relative);
        let info = game.map(|g| g.system_id).or_else(|| document_library::folder_system(relative).map(|s| s.0)).and_then(|id| catalog.iter().find(|s| s.id == id));
        match info {
            Some(info) if info.extensions.contains(&ext) => {
                if game.is_some() {
                    right_type.entry(info.id.as_str()).or_default().push(path.to_path_buf());
                }
            }
            Some(info) => {
                if let Some((what, todo)) = game_check::wrong_file(&files, &PHONE, info, &catalog, &info.extensions, path, &ext) {
                    found.add(Finding { group: Group::Place, name: game_check::file_of(path), system: info.name.clone(), what, todo, place: Some(files.shown(path)), fix: None });
                    if game.is_some() {
                        blocked.insert(path.to_path_buf());
                    }
                }
            }
            None if matches!(ext.as_str(), "zip" | "iso" | "chd" | "cue" | "gdi" | "m3u" | "pbp" | "cso") => {
                *unplaced.entry(folder_of(relative)).or_default() += 1;
            }
            None => {
                if let Some((what, todo)) = game_check::wrong_file(&files, &PHONE, &nowhere, &catalog, &all_types, path, &ext) {
                    found.add(Finding { group: Group::Place, name: game_check::file_of(path), system: String::new(), what, todo, place: Some(files.shown(path)), fix: None });
                }
            }
        }
    }
    for (folder, n) in &unplaced {
        // The folder itself, or the chosen folder's own name when they lie in it.
        let root = listing.roots.iter().find(|r| !r.prefix.is_empty() && folder.starts_with(r.prefix.trim_end_matches('/')));
        let root = root.or_else(|| listing.roots.iter().find(|r| r.prefix.is_empty()));
        let inner = root.map_or(folder.as_str(), |r| folder.strip_prefix(r.prefix.trim_end_matches('/')).unwrap_or(folder).trim_start_matches('/'));
        let root_name = root.map_or("", |r| r.name.as_str());
        let name = if inner.is_empty() { root_name } else { last_part(inner) };
        let what = if *n == 1 { format!("pocket-ui can't tell which system the game in {name} is for, so it isn't on your shelf.") } else { format!("pocket-ui can't tell which system the {n} games in {name} are for, so they aren't on your shelf.") };
        let alias = Some(plain(name)).filter(|p| document_library::system(p).is_some() && p != name);
        let todo = if inner.is_empty() && document_library::system(&plain(name)).is_some() {
            format!("You chose {name} itself. Choose the folder above it, so that its name counts.")
        } else if let Some(alias) = alias {
            format!("Rename the folder to {alias}.")
        } else {
            "Name the folder after the system, for example psx, ps2 or segacd.".to_string()
        };
        let place = Some(if inner.is_empty() { root_name.to_string() } else { format!("{root_name}/{inner}") });
        found.add(Finding { group: Group::Place, name: name.to_string(), system: String::new(), what, todo, place, fix: None });
    }

    // Each system's games on the shelf, and whether anything can start them.
    let mut done = 0;
    let mut installed: HashMap<&str, bool> = HashMap::new();
    let mut waiting: BTreeMap<&str, (&str, &str, usize)> = BTreeMap::new();
    let mut tips: BTreeMap<&str, (&str, &str)> = BTreeMap::new();
    for (id, paths) in &right_type {
        let Some(info) = catalog.iter().find(|s| s.id == *id) else { continue };
        progress(done, total, &info.name);
        let mut between = |i: usize| {
            game_check::breath(stop, i)?;
            done += 1;
            if i.is_multiple_of(32) {
                progress(done, total, &info.name);
            }
            Some(())
        };
        game_check::check_system(&mut found, &files, &PHONE, info, paths, &mut blocked, &mut between)?;
        let bundled = host.bundled(id);
        for path in paths {
            let Some(game) = on_shelf.get(path.to_string_lossy().as_ref()) else { continue };
            let place = Some(files.shown(path));
            let not_here = |what: String, todo: &str| Finding { group: Group::NotHere, name: game.name.clone(), system: info.name.clone(), what, todo: todo.into(), place: place.clone(), fix: None };
            let ext = game_check::extension(path);
            if bundled {
                if *id == "pcengine" && matches!(ext.as_str(), "cue" | "chd") {
                    found.add(not_here("PC Engine CD games don't start in this app yet: its player has no way to get their BIOS and tracks.".into(), "The game stays on your shelf and nothing is changed."));
                    blocked.insert(path.clone());
                } else if let Some(len) = files.size(path) {
                    if *id != "psp" && len > MAX_IN_MEMORY {
                        found.add(not_here(format!("This game is {} MB. This app's player takes games up to {} MB on this system.", len >> 20, MAX_IN_MEMORY >> 20), "The game stays on your shelf and nothing is changed."));
                        blocked.insert(path.clone());
                    } else if len > BIG {
                        if let Some(room) = host.room_for_copy(game).filter(|room| *room < len + KEEP_FREE) {
                            found.add(not_here(format!("This game needs {} MB free on your phone for its first start, and {} MB is free.", (len + KEEP_FREE) >> 20, room.saturating_sub(KEEP_FREE) >> 20), "Make some room and start it. The game stays on your shelf."));
                            blocked.insert(path.clone());
                        }
                    }
                }
                continue;
            }
            let Some(app) = app_name(game.profile) else {
                found.add(not_here(format!("{} games can't be started from pocket-ui yet.", info.name), "The game stays on your shelf. Start it in its own emulator app."));
                blocked.insert(path.clone());
                continue;
            };
            if *installed.entry(game.profile).or_insert_with(|| host.installed(game.profile)) {
                if let Some(core) = game.profile.strip_prefix("retroarch:") {
                    tips.insert(id, ("retroarch", core));
                }
            } else {
                waiting.entry(id).or_insert((info.name.as_str(), app, 0)).2 += 1;
                blocked.insert(path.clone());
            }
            if needs_own_files(id) {
                tips.entry(id).or_insert((app, ""));
            }
        }
    }
    for (name, app, n) in waiting.values() {
        let what = if *n == 1 { format!("1 game needs the {app} app, which isn't installed.") } else { format!("{n} games need the {app} app, which isn't installed.") };
        let todo = if *n == 1 { format!("Install {app} from its official site or store and it starts from here. Until then it stays on your shelf.") } else { format!("Install {app} from its official site or store and they start from here. Until then they stay on your shelf.") };
        found.add(Finding { group: Group::Emulator, name: name.to_string(), system: app.to_string(), what, todo, place: None, fix: None });
    }
    for (id, (app, core)) in &tips {
        let Some(info) = catalog.iter().find(|s| s.id == *id) else { continue };
        let name = info.name.clone();
        let tip = |what: String, todo: String| Finding { group: Group::Tips, name: name.clone(), system: String::new(), what, todo, place: None, fix: None };
        if *app == "retroarch" {
            found.add(tip(format!("{name} starts in RetroArch, with its {core} core."), "Download that core once inside RetroArch. pocket-ui can't see into another app, so it can't say whether it is there.".to_string()));
        }
        if needs_own_files(id) {
            let Some(shown) = app_name(document_library::system(id).map_or("", |s| s.2)) else { continue };
            let what = if *id == "switch" { format!("{name} needs keys and firmware from your own console.") } else { format!("{name} needs a BIOS file from your own console.") };
            found.add(tip(what, format!("{shown} asks for it and keeps it in its own folder. pocket-ui can't see into another app, so it can't say whether it is there.")));
        }
    }
    if bios_folder {
        found.add(Finding {
            group: Group::Tips,
            name: "bios".into(),
            system: String::new(),
            what: "The files in your bios folder aren't read.".into(),
            todo: "pocket-ui's own player needs no BIOS, and each emulator app keeps its own: add the file there.".into(),
            place: None,
            fix: None,
        });
    }
    progress(total, total, "");
    let stuck = games.iter().filter(|g| blocked.contains(Path::new(&g.relative))).count();
    Some(Outcome { report: found.report(total, total - stuck), unreadable, no_folder: false, documents })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::archive::tests::zip_of;
    use std::io::Cursor;

    /// The real list of bundled cores, emulator apps by name, zips from memory.
    #[derive(Default)]
    struct Phone {
        apps: Vec<&'static str>,
        zips: HashMap<String, Vec<u8>>,
        /// What `room_for_copy` says, and the games that have their copy.
        room: Option<u64>,
        copied: Vec<&'static str>,
    }

    impl Host for Phone {
        fn bundled(&self, system: &str) -> bool {
            let lock: serde_json::Value = serde_json::from_str(include_str!("../android/cores.lock.json")).unwrap();
            lock["cores"].as_array().unwrap().iter().any(|c| c["systems"].as_array().unwrap().iter().any(|s| s == system))
        }
        fn installed(&self, profile: &str) -> bool {
            self.apps.contains(&profile.split(':').next().unwrap())
        }
        fn open(&self, uri: &str) -> Option<Box<dyn ReadSeek>> {
            Some(Box::new(Cursor::new(self.zips.get(uri)?.clone())))
        }
        fn room_for_copy(&self, game: &document_library::Game) -> Option<u64> {
            if self.copied.contains(&game.name.as_str()) { None } else { self.room }
        }
    }

    fn file(relative: &str, size: i64, sheet: &str) -> Listed {
        Listed { uri: format!("content://p/{relative}"), id: format!("id:{relative}"), authority: "p".into(), relative: relative.into(), sheet: sheet.into(), size }
    }

    fn root(files: Vec<Listed>) -> Root {
        Root { uri: "content://tree/Games".into(), name: "Games".into(), state: RootState::Ok, prefix: String::new(), files }
    }

    fn run(phone: &Phone, roots: Vec<Root>) -> Outcome {
        check(&Listing { roots }, phone, &AtomicBool::new(false), &mut |_, _, _| {}).unwrap()
    }

    fn of(o: &Outcome, g: Group) -> Vec<&Finding> {
        o.report.in_group(g)
    }

    #[test]
    fn no_folder_yet_is_said_and_no_games_is_not_a_fault() {
        let o = check(&Listing::default(), &Phone::default(), &AtomicBool::new(false), &mut |_, _, _| {}).unwrap();
        assert!(o.no_folder);
        let o = run(&Phone::default(), vec![root(vec![file("notes.txt", 10, "")])]);
        assert!(!o.no_folder);
        assert_eq!((o.report.total, o.report.findings.len()), (0, 0));
    }

    #[test]
    fn bundled_games_are_all_good_and_counted() {
        let o = run(&Phone::default(), vec![root(vec![file("gb/Libbet.gb", 32768, ""), file("gba/Voxelburg.gba", 900_000, ""), file("snes/Elevator.sfc", 524288, ""), file("Sonic Homebrew.md", 524288, ""), file("gb/cover.png", 9, "")])]);
        assert_eq!(o.report.findings, Vec::new());
        assert!(o.report.all_good());
        assert_eq!((o.report.total, o.report.ready), (4, 4));
    }

    #[test]
    fn the_faults_of_a_normal_copied_folder_each_have_their_place() {
        let mut phone = Phone::default();
        phone.zips.insert("content://p/gba/Only Text.zip".into(), zip_of(&[("readme.txt", b"hi")]));
        phone.zips.insert("content://p/gba/Fine.zip".into(), zip_of(&[("g.gba", b"g")]));
        let o = run(
            &phone,
            vec![root(vec![
                file("gb/Libbet.gb", 32768, ""),
                file("gb/Libbet (1).gb", 32768, ""),
                file("nes/Stopped.nes", 0, ""),
                file("gba/Only Text.zip", 200, ""),
                file("gba/Fine.zip", 200, ""),
                file("snes/Chrono.gba", 100, ""),
                file("snes/Typo.sfcc", 100, ""),
                file("snes/Half.sfc.part", 100, ""),
                file("psx/Lost Track.cue", 30, "FILE \"Lost Track.bin\" BINARY"),
                file("psx/Whole.cue", 30, "FILE \"Whole.bin\" BINARY"),
                file("psx/Whole.bin", 3000, ""),
                file("Odd Name/One.iso", 3000, ""),
                file("Odd Name/Two.iso", 3000, ""),
            ])],
        );
        let r = &o.report;
        assert_eq!(of(&o, Group::Empty)[0].what, "This file is empty.");
        assert!(of(&o, Group::Empty)[0].todo.contains("this folder"));
        assert_eq!(of(&o, Group::Zips).len(), 1, "{:?}", of(&o, Group::Zips));
        assert!(of(&o, Group::Zips)[0].what.contains("no game file in Only Text.zip"));
        assert_eq!(of(&o, Group::Discs)[0].what, "Disc: Lost Track.cue is there, its Lost Track.bin is missing.");
        assert_eq!(of(&o, Group::Discs).len(), 1);
        let twins = of(&o, Group::Twice);
        assert_eq!(twins.len(), 1);
        assert_eq!(twins[0].fix, None, "a phone has no hiding yet: it says so instead of offering it");
        assert_eq!(twins[0].place.as_deref(), Some("Games/gb/Libbet (1).gb"));
        let place = of(&o, Group::Place);
        let by = |s: &str| place.iter().find(|f| f.name.contains(s)).unwrap_or_else(|| panic!("{s}: {place:?}"));
        assert_eq!(by("Chrono").todo, "Move it to gba/.", "the shelf lists it under its folder, and bsnes can't open it");
        assert_eq!(by("Typo").todo, "Rename it to Typo.sfc.");
        assert!(by("Half").what.contains("not copied to the end"));
        assert!(by("Odd Name").what.contains("2 games in Odd Name"), "{:?}", by("Odd Name"));
        assert!(r.ready < r.total);
        // On the shelf: the two Libbets, Stopped, the two zips, Chrono, the two discs. Stopped, Only Text, Chrono, Lost Track
        // can't start, and PlayStation needs DuckStation.
        assert_eq!((r.total, r.ready), (8, 3), "{:#?}", r.findings);
    }

    #[test]
    fn no_handheld_words_in_anything_a_phone_says() {
        let mut phone = Phone::default();
        phone.zips.insert("content://p/gba/Only Text.zip".into(), zip_of(&[("readme.txt", b"hi")]));
        let o = run(
            &phone,
            vec![
                root(vec![
                    file("gb/Libbet.gb", 32768, ""),
                    file("gb/Libbet (1).gb", 32768, ""),
                    file("nes/Stopped.nes", 0, ""),
                    file("gba/Only Text.zip", 200, ""),
                    file("snes/Chrono.gba", 100, ""),
                    file("snes/Half.sfc.part", 100, ""),
                    file("snes/Odd.rar", 100, ""),
                    file("psx/Lost.cue", 30, "FILE \"Lost.bin\" BINARY"),
                    file("segacd/Disc.chd", 3000, ""),
                    file("pcengine/Cd.chd", 3000, ""),
                    file("psvita/Game.vpk", 3000, ""),
                    file("psp/Big.iso", 1 << 30, ""),
                    file("Odd Name/One.iso", 3000, ""),
                    file("bios/scph5501.bin", 524288, ""),
                ]),
                Root { uri: "u".into(), name: "Old".into(), state: RootState::Lost, prefix: "@pocket-root-1/".into(), files: vec![] },
            ],
        );
        assert!(o.report.findings.len() >= 12, "{:#?}", o.report.findings);
        for f in &o.report.findings {
            let all = format!("{} {} {} {}", f.what, f.todo, f.name, f.place.clone().unwrap_or_default());
            for word in ["Press ", "SD card", "/storage", "roms/", "the card", "handheld", "Web companion", "Add games", "pocket-core"] {
                assert!(!all.contains(word), "{word:?} in {all}");
            }
        }
    }

    #[test]
    fn everything_a_phone_says_is_in_german_too() {
        let mut phone = Phone::default();
        phone.zips.insert("content://p/gba/Only Text.zip".into(), zip_of(&[("readme.txt", b"hi")]));
        let faults = vec![
            file("gb/Libbet.gb", 32768, ""),
            file("gb/Libbet (1).gb", 32768, ""),
            file("nes/Stopped.nes", 0, ""),
            file("gba/Only Text.zip", 200, ""),
            file("snes/Chrono.gba", 100, ""),
            file("snes/Typo.sfcc", 100, ""),
            file("snes/Half.sfc.part", 100, ""),
            file("snes/Odd.rar", 100, ""),
            file("snes/Two.sfc.txt", 100, ""),
            file("psx/Lost.cue", 30, "FILE \"Lost.bin\" BINARY"),
            file("psx/Caps.cue", 30, "FILE \"Caps.bin\" BINARY"),
            file("psx/CAPS.BIN", 30, ""),
            file("psx/Empty.cue", 30, "FILE \"Empty.bin\" BINARY"),
            file("psx/Empty.bin", 0, ""),
            file("psx/Fine.chd", 3000, ""),
            file("segacd/Disc.chd", 3000, ""),
            file("switch/Game.nsp", 3000, ""),
            file("pcengine/Cd.chd", 3000, ""),
            file("psvita/Game.vpk", 3000, ""),
            file("psp/Big.iso", 1 << 30, ""),
            file("psp/Zipped.zip", 3000, ""),
            file("Odd Name/One.iso", 3000, ""),
            file("Odd Name/Two.iso", 3000, ""),
            file("Single/One.iso", 3000, ""),
            file("Game Boy/x.zip", 3000, ""),
            file("bios/scph5501.bin", 524288, ""),
        ];
        let states = [("Lost", RootState::Lost), ("Gone", RootState::Gone), ("Big", RootState::Big)].map(|(name, state)| Root { uri: name.into(), name: name.into(), state, prefix: format!("@pocket-root-{name}/"), files: vec![] });
        let mut roots = vec![root(faults)];
        roots.extend(states);
        let mut all = run(&phone, roots).report.findings;
        let with_apps = Phone { apps: vec!["retroarch", "psx", "ps2", "switch"], ..Phone::default() };
        all.extend(run(&with_apps, vec![root(vec![file("segacd/Disc.chd", 3000, ""), file("psx/Fine.chd", 3000, ""), file("switch/Game.nsp", 3000, "")])]).report.findings);
        let german = crate::i18n::german();
        for g in Group::ALL {
            assert!(german.translate(g.name()).is_some(), "{}", g.name());
        }
        for f in &all {
            for line in [&f.what, &f.todo] {
                assert!(line.is_empty() || german.translate(line).is_some(), "no German: {line}");
            }
        }
        assert!(all.len() > 20, "{}", all.len());
    }

    #[test]
    fn an_emulator_that_is_there_is_no_fault_and_one_that_is_not_says_which() {
        let folder = || vec![root(vec![file("psx/A.chd", 3000, ""), file("psx/B.chd", 3000, ""), file("dreamcast/C.chd", 3000, ""), file("gamecube/D.rvz", 3000, "")])];
        let o = run(&Phone::default(), folder());
        let apps = of(&o, Group::Emulator);
        assert_eq!(apps.len(), 3, "{apps:?}");
        let ps = apps.iter().find(|f| f.name == "PlayStation").unwrap();
        assert_eq!(ps.what, "2 games need the DuckStation app, which isn't installed.");
        assert!(apps.iter().any(|f| f.what.starts_with("1 game needs the RetroArch app")));
        assert_eq!((o.report.total, o.report.ready), (4, 0));
        assert!(!o.report.all_good());

        let phone = Phone { apps: vec!["psx", "retroarch", "dolphin"], ..Phone::default() };
        let o = run(&phone, folder());
        assert!(of(&o, Group::Emulator).is_empty());
        assert_eq!((o.report.total, o.report.ready), (4, 4));
        assert!(o.report.all_good(), "tips only: {:?}", o.report.findings);
        let tips = of(&o, Group::Tips);
        assert!(tips.iter().any(|t| t.what == "PlayStation needs a BIOS file from your own console." && t.todo.contains("DuckStation")), "{tips:?}");
        assert!(tips.iter().any(|t| t.what.contains("RetroArch, with its flycast core")), "{tips:?}");
    }

    #[test]
    fn a_bios_is_never_called_missing_or_a_fault() {
        // Sega CD, with and without the file beside it: it goes to RetroArch, whose folder this app can't read.
        for files in [vec![file("segacd/Sonic.chd", 3000, "")], vec![file("segacd/Sonic.chd", 3000, ""), file("bios/bios_CD_E.bin", 131072, "")]] {
            let phone = Phone { apps: vec!["retroarch"], ..Phone::default() };
            let o = run(&phone, vec![root(files)]);
            assert!(of(&o, Group::Bios).is_empty());
            assert!(o.report.all_good(), "{:?}", o.report.findings);
            assert!(of(&o, Group::Tips).iter().any(|t| t.what.contains("Mega CD needs a BIOS file")));
        }
        // No bundled system of the phone asks for one.
        let o = run(&Phone::default(), vec![root(vec![file("gba/A.gba", 3000, ""), file("nes/B.nes", 3000, ""), file("psp/C.iso", 3000, ""), file("nds/D.nds", 3000, ""), file("n64/E.z64", 3000, "")])]);
        assert_eq!(o.report.findings, Vec::new());
        // A file in the bios folder is no game, and it is said that nothing reads it.
        let o = run(&Phone::default(), vec![root(vec![file("gba/A.gba", 3000, ""), file("bios/syscard3.pce", 262144, ""), file("bios/gba_bios.bin", 16384, "")])]);
        assert_eq!(o.report.total, 1);
        assert!(of(&o, Group::Tips)[0].what.contains("bios folder"));
    }

    #[test]
    fn what_this_apps_player_cant_take_yet_is_said() {
        let o = run(&Phone::default(), vec![root(vec![file("psp/Small.iso", 100 << 20, ""), file("psp/Large.iso", 1843 << 20, ""), file("nds/Huge.nds", 600 << 20, ""), file("nds/Big.nds", 512 << 20, ""), file("pcengine/Card.pce", 1000, ""), file("pcengine/Disc.chd", 1000, ""), file("psvita/Vita.vpk", 1000, ""), file("psp/Zipped.zip", 1000, "")])]);
        let no = of(&o, Group::NotHere);
        assert!(no.iter().any(|f| f.name == "Huge" && f.what == "This game is 600 MB. This app's player takes games up to 512 MB on this system."), "{no:?}");
        assert!(no.iter().any(|f| f.name == "Disc" && f.what.contains("PC Engine CD")), "{no:?}");
        assert!(no.iter().any(|f| f.what.contains("PlayStation Vita games can't be started")), "{no:?}");
        assert!(!no.iter().any(|f| matches!(f.name.as_str(), "Small" | "Large" | "Big" | "Card")), "a PSP game of any size starts, the DS's biggest card too: {no:?}");
        assert_eq!(of(&o, Group::Place)[0].what, "PlayStation Portable doesn't take .zip files, so this one isn't read.");
        assert_eq!((o.report.total, o.report.ready), (8, 4));
    }

    #[test]
    fn a_big_game_says_when_the_phone_has_no_room_for_its_first_start() {
        let games = || vec![root(vec![file("psp/Large.iso", 1843 << 20, ""), file("psp/Small.iso", 60 << 20, ""), file("psp/Fits.iso", 900 << 20, "")])];
        let o = run(&Phone { room: Some(1200 << 20), ..Phone::default() }, games());
        let no = of(&o, Group::NotHere);
        assert_eq!(no.len(), 1, "{no:?}");
        assert_eq!(no[0].name, "Large");
        assert_eq!(no[0].what, "This game needs 2099 MB free on your phone for its first start, and 944 MB is free.");
        assert_eq!((o.report.total, o.report.ready), (3, 2));
        let o = run(&Phone { room: Some(1200 << 20), copied: vec!["Large"], ..Phone::default() }, games());
        assert!(of(&o, Group::NotHere).is_empty(), "its copy is there: no room needed");
        // A phone with room is no fault.
        let o = run(&Phone { room: Some(100 << 30), ..Phone::default() }, games());
        assert!(o.report.all_good(), "{:?}", o.report.findings);
    }

    #[test]
    fn a_lost_grant_says_so_keeps_the_other_folder_and_names_the_way_back() {
        let second = Root { uri: "content://tree/Old".into(), name: "Old Games".into(), state: RootState::Lost, prefix: "@pocket-root-ab/".into(), files: vec![] };
        let o = run(&Phone::default(), vec![root(vec![file("gb/A.gb", 3000, "")]), second.clone()]);
        let a = of(&o, Group::Access);
        assert_eq!(a[0].what, "Android no longer lets pocket-ui read Old Games.");
        assert!(a[0].todo.contains("Choose this folder again"));
        assert_eq!(o.unreadable, vec![Unreadable { name: "Old Games".into(), uri: "content://tree/Old".into() }]);
        assert_eq!((o.report.total, o.report.ready), (1, 1), "the game that can be read is ready");
        assert!(!o.report.all_good());
        assert_eq!(o.report.summary(), "1 thing to fix");

        let gone = Root { state: RootState::Gone, ..second.clone() };
        assert!(of(&run(&Phone::default(), vec![gone]), Group::Access)[0].what.contains("not there any more"));
        let big = Root { state: RootState::Big, ..second };
        assert!(of(&run(&Phone::default(), vec![big]), Group::Access)[0].what.contains("50,000"));
    }

    #[test]
    fn disc_sets_are_followed_as_the_shelf_does() {
        // A playlist whose cue lost its track: the cue isn't on the shelf, the playlist's game can't start.
        let o = run(
            &Phone { apps: vec!["psx"], ..Phone::default() },
            vec![root(vec![
                file("psx/Set/Set.m3u", 20, "Disc 1.cue\nDisc 2.chd"),
                file("psx/Set/Disc 1.cue", 30, "FILE \"Track.bin\" BINARY"),
                file("psx/Set/Disc 2.chd", 3000, ""),
                file("psx/Empty.cue", 30, "FILE \"Empty.bin\" BINARY"),
                file("psx/Empty.bin", 0, ""),
                file("psx/Caps.cue", 30, "FILE \"Caps.bin\" BINARY"),
                file("psx/CAPS.BIN", 3000, ""),
            ])],
        );
        let d = of(&o, Group::Discs);
        assert!(d.iter().any(|f| f.what == "Disc: Disc 1.cue is there, its Track.bin is missing."), "{d:?}");
        assert!(d.iter().any(|f| f.what == "Disc: Empty.cue is there, its Empty.bin is empty."), "{d:?}");
        assert_eq!(d.iter().find(|f| f.what.contains("Caps.cue")).unwrap().todo, "Rename CAPS.BIN to Caps.bin.");
        assert_eq!(o.report.ready, 0, "{} on the shelf, none can start: {:#?}", o.report.total, o.report.findings);
    }

    #[test]
    fn a_folder_named_like_a_system_gets_the_exact_advice() {
        let o = run(&Phone { apps: vec!["psx"], ..Phone::default() }, vec![root(vec![file("Game Boy/x.zip", 300, "")])]);
        assert_eq!(of(&o, Group::Place)[0].todo, "Rename the folder to gameboy.");
        // The folder that was chosen is psx itself: its name isn't part of the paths.
        let chosen = Root { name: "psx".into(), ..root(vec![file("Game.chd", 3000, ""), file("Other.cue", 30, "FILE \"Other.bin\" BINARY"), file("Other.bin", 3000, "")]) };
        let o = run(&Phone::default(), vec![chosen]);
        assert_eq!(of(&o, Group::Place)[0].todo, "You chose psx itself. Choose the folder above it, so that its name counts.");
        assert_eq!(o.report.total, 0);
    }

    #[test]
    fn a_save_beside_its_game_is_no_fault_on_a_phone_either() {
        let o = run(
            &Phone::default(),
            vec![root(vec![file("nds/Game.nds", 40000, ""), file("nds/Game.nds.sav", 512, ""), file("Loose.nds.sav", 512, ""), file("gba/Third.gba.txt", 100, "")])],
        );
        let place = of(&o, Group::Place);
        assert!(!place.iter().any(|f| f.name.contains(".sav") || f.todo.contains("Game.nds")), "{place:?}");
        assert!(place.iter().any(|f| f.todo == "Rename it to Third.gba."), "{place:?}");
    }

    #[test]
    fn two_folders_keep_their_names_apart() {
        let second = Root { uri: "u2".into(), name: "Second".into(), state: RootState::Ok, prefix: "@pocket-root-ab/".into(), files: vec![file("@pocket-root-ab/nes/Empty.nes", 0, "")] };
        let o = run(&Phone::default(), vec![root(vec![file("nes/Empty.nes", 0, "")]), second]);
        let places: Vec<_> = of(&o, Group::Empty).iter().filter_map(|f| f.place.clone()).collect();
        assert!(places.contains(&"Games/nes/Empty.nes".to_string()) && places.contains(&"Second/nes/Empty.nes".to_string()), "{places:?}");
    }

    #[test]
    fn a_listing_is_read_as_the_activity_writes_it() {
        let json = r#"{"roots":[{"uri":"content://t","name":"Games","state":"ok","files":[{"uri":"content://d","id":"1","authority":"a","relative":"gb/A.gb","size":-1,"modified":5}]},{"uri":"content://l","name":"Old","state":"lost","prefix":"@pocket-root-1/"}]}"#;
        let listing: Listing = serde_json::from_str(json).unwrap();
        let o = run(&Phone::default(), listing.roots);
        assert_eq!((o.report.total, o.report.ready), (1, 1), "a provider that gives no size is no empty file");
        assert_eq!(o.documents.len(), 1);
    }

    #[test]
    fn it_stops_at_once() {
        let stop = AtomicBool::new(true);
        assert!(check(&Listing { roots: vec![root(vec![file("gb/A.gb", 3, "")])] }, &Phone::default(), &stop, &mut |_, _, _| {}).is_none());
    }

    /// A big library, zips and all, stays quick and small: the seam adds no work per file beyond the listing.
    #[test]
    fn five_thousand_games_stay_within_bounds() {
        let files: Vec<Listed> = (0..5000).map(|i| {
            let (sys, ext) = [("gba", "gba"), ("snes", "sfc"), ("megadrive", "md"), ("nes", "nes")][i % 4];
            file(&format!("{sys}/Spiel Nummer {i} (Europe).{ext}"), if i % 500 == 0 { 0 } else { 100 }, "")
        }).collect();
        let t = std::time::Instant::now();
        let o = run(&Phone::default(), vec![root(files)]);
        println!("android game check, 5000 games: {:?}", t.elapsed());
        assert_eq!(o.report.total, 5000);
        assert!(t.elapsed() < std::time::Duration::from_secs(30));
    }
}
