//! Android's first start, the part both views share (Mobile: `android/src/onboarding.rs`; Handheld: `setup.rs`'s "Your
//! games"): where games already are, which emulator apps are on the device, and which BIOS a found system still lacks.
//!
//! Plain data and a `Disk` to read folders through, so every rule has a test on Linux with a fake disk and a fake list of
//! apps; the hosts read the device (`RealDisk`, `installedApps`) and hand it in.
//!
//! - **Folders** (`find_game_folders`): the usual places other apps keep games, on internal storage and every card
//!   (`ROMs`, `Emulation/roms`, `RetroArch/roms`, `Games`, a card's top level, ...), any case. A place counts when it
//!   has a system's folder by a name ES-DE, Daijisho or ROCKNIX use (`document_library::system`). Without the right to
//!   read files there (no All files access) Android still lists the folders: the systems are known, their games not
//!   (`games: None`).
//! - **Apps** (`app_offers`): for each system pocket-ui's player doesn't have (PS2, GameCube / Wii, 3DS, Xbox), the
//!   first installed app that takes it, unless one is chosen already. Taking an offer is the same choice as Settings →
//!   Plays with (`take_offers`, `plays_with` in launcher.json).
//! - **BIOS** (`missing_bios`): Saturn needs one; PlayStation and Dreamcast play without and only say where one goes.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::play_route::{Phone, PhoneApp};

/// What a host lets this module see of its folders: the names in a folder and whether each is a folder. None: it is not
/// there or can't be listed.
pub trait Disk {
    fn list(&self, dir: &Path) -> Option<Vec<(String, bool)>>;
}

/// The device's own folders.
pub struct RealDisk;

impl Disk for RealDisk {
    fn list(&self, dir: &Path) -> Option<Vec<(String, bool)>> {
        let entries = std::fs::read_dir(dir).ok()?;
        Some(
            entries
                .filter_map(|e| e.ok())
                .filter_map(|e| Some((e.file_name().into_string().ok()?, e.file_type().ok()?.is_dir())))
                .collect(),
        )
    }
}

/// The places other apps and guides keep games under a storage volume, as path parts matched in any case. The volume's
/// own top level is looked at on cards only (internal storage's top level is Android's own folders).
pub const PLACES: &[&[&str]] = &[
    &["roms"],
    &["emulation", "roms"],
    &["retroarch", "roms"],
    &["retroarch", "games"],
    &["games"],
    &["emulators", "roms"],
    &["download", "roms"],
    &["downloads", "roms"],
];

/// A place with games found on the device.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FoundFolder {
    /// Its real path, the names as they are on the device ("/storage/1234-ABCD/Roms").
    pub path: PathBuf,
    /// On a card (`/storage/XXXX-XXXX`), not internal storage.
    pub card: bool,
    /// The systems found there (the Mobile view's ids, "psx"), and how many games each has (None: files can't be read).
    pub systems: Vec<(&'static str, Option<usize>)>,
    /// All its games (None: files can't be read here).
    pub games: Option<usize>,
    /// A `ROMs` folder right under a volume: the Handheld view reads it without being asked (`android_handheld::roms_roots`).
    pub usual: bool,
}

impl FoundFolder {
    /// The path under its volume, "Emulation/roms"; "" for a card's top level.
    pub fn inner_path(&self) -> String {
        volume_and_inner(&self.path).map_or_else(|| self.path.to_string_lossy().into_owned(), |(_, inner)| inner)
    }

    /// The storage provider's document of the folder ("primary:ROMs", "1234-ABCD:Roms"): where the Mobile view's folder
    /// picker opens, so taking it is one more press.
    pub fn document_uri(&self) -> Option<String> {
        let (volume, inner) = volume_and_inner(&self.path)?;
        Some(format!("content://com.android.externalstorage.documents/document/{}", encode(&format!("{volume}:{inner}"))))
    }

    /// The row's words: "2341 games in ROMs", "12 games in Roms on the SD card" (counted), "12 systems in ..." (folders only). Made of
    /// whole patterns of the table (`{n}` a number, `{s}` the folder's name as it is), so each language says it in one piece.
    pub fn row(&self) -> String {
        let inner = self.inner_path();
        let (n, games) = match self.games {
            Some(n) => (n, true),
            None => (self.systems.len(), false),
        };
        match (n == 1, games, inner.is_empty(), self.card) {
            (true, true, true, _) => "1 game on the SD card".to_string(),
            (true, true, false, true) => format!("1 game in {inner} on the SD card"),
            (true, true, false, false) => format!("1 game in {inner}"),
            (false, true, true, _) => format!("{n} games on the SD card"),
            (false, true, false, true) => format!("{n} games in {inner} on the SD card"),
            (false, true, false, false) => format!("{n} games in {inner}"),
            (true, false, true, _) => "1 system on the SD card".to_string(),
            (true, false, false, true) => format!("1 system in {inner} on the SD card"),
            (true, false, false, false) => format!("1 system in {inner}"),
            (false, false, true, _) => format!("{n} systems on the SD card"),
            (false, false, false, true) => format!("{n} systems in {inner} on the SD card"),
            (false, false, false, false) => format!("{n} systems in {inner}"),
        }
    }
}

/// The storage volume of a path ("primary" for internal storage's `emulated/0`, "1234-ABCD" for a card) and the path
/// under it, wherever the storage folder is (`/storage` on a device, a test's folder elsewhere).
fn volume_and_inner(path: &Path) -> Option<(String, String)> {
    let parts: Vec<String> = path.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect();
    let join = |from: usize| parts.get(from..).map(|p| p.join("/")).unwrap_or_default();
    for i in 0..parts.len() {
        if parts[i] == "emulated" && parts.get(i + 1).map(String::as_str) == Some("0") {
            return Some(("primary".into(), join(i + 2)));
        }
        if is_card(&parts[i]) {
            return Some((parts[i].clone(), join(i + 1)));
        }
    }
    None
}

fn encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// A storage volume's name on a card: "1234-ABCD".
fn is_card(volume: &str) -> bool {
    let b = volume.as_bytes();
    b.len() == 9 && b[4] == b'-' && b.iter().enumerate().all(|(i, c)| i == 4 || c.is_ascii_hexdigit())
}

/// The folder named `name` (any case) in `dir`.
fn child(disk: &dyn Disk, dir: &Path, name: &str) -> Option<PathBuf> {
    disk.list(dir)?.into_iter().find(|(n, is_dir)| *is_dir && n.eq_ignore_ascii_case(name)).map(|(n, _)| dir.join(n))
}

/// Folders that are never a system's, whatever an alias table says: BIOS, saves, art and Android's own.
fn never_a_system(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "bios" | "saves" | "states" | "media" | "images" | "downloaded_media" | "screenshots" | "music" | "movies" | "pictures" | "android" | "dcim" | "documents" | "download" | "ports"
    )
}

/// How many games `dir` of `system` holds: its files of a type the system takes, and those one folder down (a disc's
/// folder, an Xbox game's). None when no file in it can be seen at all.
fn count_games(disk: &dyn Disk, dir: &Path, system: &str) -> Option<usize> {
    let takes = crate::document_library::takes(system);
    let is_game = |name: &str| {
        let ext = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
        if takes.is_empty() { crate::document_library::shelf_type(&ext) } else { takes.contains(&ext.as_str()) }
    };
    let entries = disk.list(dir)?;
    let mut seen_file = false;
    let mut games = 0;
    for (name, is_dir) in &entries {
        if *is_dir {
            if never_a_system(name) {
                continue;
            }
            // One folder down: a multi-disc game's folder counts once, an Xbox game is a folder with its disc.
            let inner = disk.list(&dir.join(name)).unwrap_or_default();
            if inner.iter().any(|(n, d)| !d && is_game(n)) {
                seen_file = true;
                games += if system == "xbox" || inner.iter().any(|(n, _)| n.to_ascii_lowercase().ends_with(".m3u")) { 1 } else { inner.iter().filter(|(n, d)| !d && is_game(n) && !n.to_ascii_lowercase().ends_with(".bin")).count().max(1) };
            }
        } else {
            seen_file = true;
            if is_game(name) {
                games += 1;
            }
        }
    }
    // A folder listed with nothing in it says nothing: on Android 11+ without All files access every folder looks like that.
    (seen_file || entries.is_empty()).then_some(games).filter(|_| seen_file)
}

/// The systems in `dir`, by folder name: (id, its folder, its games).
fn systems_in(disk: &dyn Disk, dir: &Path, can_read: bool) -> Vec<(&'static str, Option<usize>)> {
    let mut out: Vec<(&'static str, Option<usize>)> = Vec::new();
    for (name, is_dir) in disk.list(dir).unwrap_or_default() {
        if !is_dir || never_a_system(&name) {
            continue;
        }
        let Some((id, ..)) = crate::document_library::system(&name) else { continue };
        let games = if can_read { count_games(disk, &dir.join(&name), id) } else { None };
        // "gc" and "gamecube" both there: one system, its games added up.
        match out.iter_mut().find(|(i, _)| *i == id) {
            Some((_, n)) => *n = match (*n, games) { (Some(a), Some(b)) => Some(a + b), (a, b) => a.or(b) },
            None => out.push((id, games)),
        }
    }
    // A folder of a system with no game in it is no reason to offer the place ("12 systems" of empty folders).
    if can_read {
        out.retain(|(_, n)| n.is_some_and(|n| n > 0));
    }
    out
}

/// Every place with games under `storage` ("/storage"): internal storage first, then each card in name order; within a
/// volume in the order of `PLACES`. `can_read`: the files there can be read (All files access, or storage read before
/// Android 11); without it only the system folders are known.
pub fn find_game_folders(disk: &dyn Disk, storage: &Path, can_read: bool) -> Vec<FoundFolder> {
    let mut volumes: Vec<(PathBuf, bool)> = vec![(storage.join("emulated/0"), false)];
    let mut cards: Vec<String> = disk.list(storage).unwrap_or_default().into_iter().filter(|(n, d)| *d && is_card(n)).map(|(n, _)| n).collect();
    cards.sort();
    volumes.extend(cards.into_iter().map(|c| (storage.join(c), true)));
    let mut found: Vec<FoundFolder> = Vec::new();
    for (volume, card) in volumes {
        let mut places: Vec<(PathBuf, bool)> = Vec::new();
        for parts in PLACES {
            let mut at = Some(volume.clone());
            for part in *parts {
                at = at.and_then(|d| child(disk, &d, part));
            }
            if let Some(p) = at {
                places.push((p, parts.len() == 1 && parts[0] == "roms"));
            }
        }
        // A card with the systems' folders at its top: the card itself.
        if card {
            places.push((volume.clone(), false));
        }
        for (path, usual) in places {
            if found.iter().any(|f| f.path == path) {
                continue;
            }
            let systems = systems_in(disk, &path, can_read);
            // A card's top level needs two systems: one folder named "gb" beside the photos is no games folder.
            if systems.is_empty() || (path == volume && systems.len() < 2) {
                continue;
            }
            let games = if can_read { Some(systems.iter().filter_map(|(_, n)| *n).sum()) } else { None };
            found.push(FoundFolder { path, card, systems, games, usual });
        }
    }
    found
}

/// The systems with games in any of `folders`, once each.
pub fn systems_found(folders: &[FoundFolder]) -> Vec<&'static str> {
    let mut out: Vec<&'static str> = Vec::new();
    for f in folders {
        for (id, _) in &f.systems {
            if !out.contains(id) {
                out.push(id);
            }
        }
    }
    out
}

// ---- emulator apps ----------------------------------------------------------------------------------------------------

/// An installed app to play a system pocket-ui's player doesn't have.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppOffer {
    /// The Mobile view's id of the system, the key of `plays_with`.
    pub system: &'static str,
    pub app: PhoneApp,
}

impl AppOffer {
    /// The row's words: "Dolphin (GameCube / Wii)", names only.
    pub fn row(&self) -> String {
        format!("{} ({})", self.app.label, crate::document_library::system_name(self.system))
    }
}

/// The line about the offers: which app plays which system, and where that is changed later.
pub fn apps_note(offers: &[AppOffer]) -> String {
    let list: Vec<String> = offers.iter().map(AppOffer::row).collect();
    format!("Found on this device: {}. They play what pocket-ui doesn't; you can change it per system later.", list.join(", "))
}

/// The systems only another app plays on Android: no core of pocket-ui's for them.
pub fn app_only_systems() -> impl Iterator<Item = &'static str> {
    crate::document_library::SYSTEM_IDS.into_iter().filter(|s| crate::play_route::bundled_core(s).is_none())
}

/// For each system with games (`found`, in its order) that only an app plays, the first installed app that takes it,
/// unless a choice for that system is made (and its app still installed). A system without games is offered nothing: the
/// first start talks about what is there (the Mini V2 offered RetroArch twice for systems it had no game of, 18.11.).
pub fn app_offers(phone: &Phone, plays_with: &BTreeMap<String, String>, found: &[&str]) -> Vec<AppOffer> {
    found
        .iter()
        .filter(|s| crate::play_route::bundled_core(s).is_none())
        .filter_map(|&system| {
            let system = crate::document_library::SYSTEM_IDS.into_iter().find(|id| *id == system)?;
            let apps = phone.apps_for(system);
            let chosen = plays_with.get(system).is_some_and(|p| apps.iter().any(|a| &a.package == p));
            (!chosen).then(|| apps.first().map(|app| AppOffer { system, app: app.clone() })).flatten()
        })
        .collect()
}

/// The names of the offered apps, once each, in order ("ARMSX2, Dolphin"): one app may play several systems.
pub fn offer_names(offers: &[AppOffer]) -> Vec<&str> {
    let mut names: Vec<&str> = Vec::new();
    for o in offers {
        if !names.contains(&o.app.label.as_str()) {
            names.push(&o.app.label);
        }
    }
    names
}

/// The offers taken: each system plays with its app, as if chosen in Settings → Plays with. Returns whether anything changed.
pub fn take_offers(plays_with: &mut BTreeMap<String, String>, offers: &[AppOffer]) -> bool {
    let mut changed = false;
    for o in offers {
        if plays_with.get(o.system) != Some(&o.app.package) {
            plays_with.insert(o.system.to_string(), o.app.package.clone());
            changed = true;
        }
    }
    changed
}

/// Where each app for a system pocket-ui doesn't play is offered by its own project: the one list of these links. Only
/// apps pocket-ui can start (`EmulatorIntents.java`), each system once; the page is opened in the browser, pocket-ui
/// downloads and installs nothing itself.
pub const APP_DOWNLOADS: &[(&str, &str, &str)] = &[
    ("ps2", "ARMSX2", "https://github.com/ARMSX2/ARMSX2/releases"),
    ("gamecube", "Dolphin", "https://dolphin-emu.org/download/"),
    ("3ds", "Azahar", "https://github.com/azahar-emu/azahar/releases"),
    ("xbox", "hakuX", "https://github.com/rfandango/hakuX/releases"),
];

/// A system with games that no installed app plays, and where its app comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AppToGet {
    pub system: &'static str,
    pub app: &'static str,
    pub url: &'static str,
}

impl AppToGet {
    /// The row's words: "Get ARMSX2 (PlayStation 2)".
    pub fn row(&self) -> String {
        format!("Get {} ({})", self.app, crate::document_library::system_name(self.system))
    }
}

/// What the row about an app to get says over it.
pub const GET_NOTE: &str = "Opens the app's official page in your browser. pocket-ui downloads and installs nothing itself.";

/// The systems among `found` (with games) that only an app plays and no installed app takes: its app's official page.
pub fn apps_to_get(phone: &Phone, found: &[&str]) -> Vec<AppToGet> {
    found
        .iter()
        .filter(|s| crate::play_route::bundled_core(s).is_none() && phone.apps_for(s).is_empty())
        .filter_map(|s| APP_DOWNLOADS.iter().find(|(sys, ..)| sys == s).map(|(system, app, url)| AppToGet { system, app, url }))
        .collect()
}

/// Whether `url` is one of the official pages above: the only pages the views open for this.
pub fn official_page(url: &str) -> bool {
    APP_DOWNLOADS.iter().any(|(.., u)| *u == url)
}

/// The apps installed for systems pocket-ui plays itself (PPSSPP, DuckStation, RetroArch): named once, "also on this
/// device", so the player knows Plays with can use them. Names once each, in the order of the systems.
pub fn other_apps(phone: &Phone) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for system in crate::document_library::SYSTEM_IDS {
        if crate::play_route::bundled_core(system).is_none() {
            continue;
        }
        for app in phone.apps_for(system) {
            if !out.contains(&app.label) {
                out.push(app.label.clone());
            }
        }
    }
    out
}

// ---- BIOS -------------------------------------------------------------------------------------------------------------

/// A found system that would like a BIOS it hasn't got.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BiosNeed {
    pub system: &'static str,
    /// It doesn't start without one (Saturn); the others play and only some games want one.
    pub required: bool,
}

impl BiosNeed {
    /// The line for it, the folder said as the view has it: the ROMs folders' `bios` (Handheld) or a folder named bios
    /// among the chosen ones (Mobile, `in_chosen`). Whole sentences of the table, no placeholder.
    pub fn line(&self, in_chosen: bool) -> &'static str {
        match (self.system, in_chosen) {
            ("saturn", false) => "Saturn needs a BIOS: put saturn_bios.bin, sega_101.bin or mpr-17933.bin in ROMs/bios.",
            ("saturn", true) => "Saturn needs a BIOS: put saturn_bios.bin, sega_101.bin or mpr-17933.bin in a folder named bios beside your games.",
            ("dreamcast", false) => "Dreamcast plays without a BIOS. For better compatibility, put dc_boot.bin and dc_flash.bin in ROMs/bios.",
            ("dreamcast", true) => "Dreamcast plays without a BIOS. For better compatibility, put dc_boot.bin and dc_flash.bin in a folder named bios beside your games.",
            (_, false) => "PlayStation plays without a BIOS. If a game hangs, put scph5501.bin in ROMs/bios.",
            (_, true) => "PlayStation plays without a BIOS. If a game hangs, put scph5501.bin in a folder named bios beside your games.",
        }
    }
}

/// The systems among `found` that want a BIOS, in the order needed first: Saturn, then PlayStation and Dreamcast.
/// `has(system)`: a BIOS for it is there already (in the games' bios folder or the player's own copy).
pub fn missing_bios(found: &[&str], has: impl Fn(&str) -> bool) -> Vec<BiosNeed> {
    [("saturn", true), ("psx", false), ("dreamcast", false)]
        .into_iter()
        .filter(|(s, _)| found.contains(s) && !has(s))
        .map(|(system, required)| BiosNeed { system, required })
        .collect()
}

/// Whether the file names of a folder hold a BIOS for `system` (`document_library::bios_file`, Dreamcast's boot file).
pub fn bios_among<'a>(system: &str, names: impl IntoIterator<Item = &'a str>) -> bool {
    names.into_iter().any(|n| crate::document_library::bios_file(n).is_some_and(|b| b.0 == system && (system != "dreamcast" || b.1 == "dc/dc_boot.bin")))
}

/// The names of the files in the `bios` folder (any case) of each of `roots`.
pub fn bios_names(disk: &dyn Disk, roots: &[PathBuf]) -> Vec<String> {
    let mut out = Vec::new();
    for root in roots {
        let Some(dir) = child(disk, root, "bios") else { continue };
        for (name, is_dir) in disk.list(&dir).unwrap_or_default() {
            if is_dir {
                // Dreamcast's files may be in bios/dc.
                if name.eq_ignore_ascii_case("dc") {
                    out.extend(disk.list(&dir.join(&name)).unwrap_or_default().into_iter().filter(|(_, d)| !d).map(|(n, _)| n));
                }
            } else {
                out.push(name);
            }
        }
    }
    out
}

/// The fixed words the views paint with these facts: one list, so the translation test covers them all.
pub const WORDS: &[&str] = &[
    "Emulator apps",
    GET_NOTE,
    "Another app keeps games here. A adds the folder to your shelf; the files stay where they are.",
    "Games found on this device. Choose them below, or another folder.",
    "No games found on this device yet. Choose the folder you put them in, or add games later.",
    "Everything is ready. Covers and pictures download on unmetered Wi-Fi as you browse; Settings can turn that off.",
];

// ---- folders taken ----------------------------------------------------------------------------------------------------

/// The Handheld view's file of the games folders taken on the first start, one path a line, beside its es_systems.cfg.
const KEPT: &str = "games-folders";

/// The games folders taken, as they were written (a folder gone since is left out).
pub fn kept_folders(data: &Path) -> Vec<PathBuf> {
    std::fs::read_to_string(data.join(KEPT)).unwrap_or_default().lines().filter(|l| !l.trim().is_empty()).map(PathBuf::from).filter(|p| p.is_dir()).collect()
}

/// Adds `folder` to the folders taken (once).
pub fn keep_folder(data: &Path, folder: &Path) {
    let mut all: Vec<PathBuf> = std::fs::read_to_string(data.join(KEPT)).unwrap_or_default().lines().filter(|l| !l.trim().is_empty()).map(PathBuf::from).collect();
    if all.iter().any(|p| p == folder) {
        return;
    }
    all.push(folder.to_path_buf());
    let text: String = all.iter().map(|p| format!("{}\n", p.display())).collect();
    if let Err(e) = crate::fsutil::write_atomic(&data.join(KEPT), text.as_bytes()) {
        eprintln!("games folders: {e}");
    }
}

// ---- what the host found ----------------------------------------------------------------------------------------------

/// The first start's facts, as the host last read them: shared between the host's thread and the pages.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Facts {
    /// None: still looking.
    pub folders: Option<Vec<FoundFolder>>,
    /// The files in the games can be read (All files access).
    pub can_read: bool,
    /// The names in the found folders' `bios`.
    pub bios: Vec<String>,
}

static FACTS: std::sync::RwLock<Option<Facts>> = std::sync::RwLock::new(None);

/// Looks at the device's folders (on the caller's thread: a few hundred folder reads at most) and keeps what it found.
pub fn look(storage: &Path, can_read: bool) -> Facts {
    let folders = find_game_folders(&RealDisk, storage, can_read);
    let roots: Vec<PathBuf> = folders.iter().map(|f| f.path.clone()).collect();
    let facts = Facts { bios: bios_names(&RealDisk, &roots), folders: Some(folders), can_read };
    *FACTS.write().unwrap_or_else(|e| e.into_inner()) = Some(facts.clone());
    facts
}

/// What `look` found last; None before it ran (and on Linux, where there is no Android).
pub fn facts() -> Option<Facts> {
    FACTS.read().unwrap_or_else(|e| e.into_inner()).clone()
}

/// For tests and snapshots: facts as if looked up.
pub fn set_facts(facts: Option<Facts>) {
    *FACTS.write().unwrap_or_else(|e| e.into_inner()) = facts;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap as Map;

    /// A disk made of paths: "a/b/" a folder, "a/b/c.gba" a file (its folders made on the way). `hide_files`: Android 11+
    /// without All files access (folders listed, files not).
    #[derive(Default)]
    struct FakeDisk {
        dirs: Map<PathBuf, Vec<(String, bool)>>,
        hide_files: bool,
    }

    impl FakeDisk {
        fn with(paths: &[&str]) -> FakeDisk {
            let mut d = FakeDisk::default();
            d.dirs.insert(PathBuf::from("/storage"), vec![]);
            for p in paths {
                let is_dir = p.ends_with('/');
                let path = PathBuf::from(p.trim_end_matches('/'));
                let mut at = PathBuf::from("/");
                let parts: Vec<_> = path.components().skip(1).collect();
                for (i, part) in parts.iter().enumerate() {
                    let name = part.as_os_str().to_string_lossy().into_owned();
                    let last = i + 1 == parts.len();
                    let entry_dir = !last || is_dir;
                    let list = d.dirs.entry(at.clone()).or_default();
                    if !list.iter().any(|(n, _)| *n == name) {
                        list.push((name.clone(), entry_dir));
                    }
                    at = at.join(&name);
                    if entry_dir {
                        d.dirs.entry(at.clone()).or_default();
                    }
                }
            }
            d
        }
    }

    impl Disk for FakeDisk {
        fn list(&self, dir: &Path) -> Option<Vec<(String, bool)>> {
            let l = self.dirs.get(dir)?.clone();
            Some(if self.hide_files { l.into_iter().filter(|(_, d)| *d).collect() } else { l })
        }
    }

    fn phone(apps: &str) -> Phone {
        Phone::from_json(&format!(r#"{{"apps": {apps}}}"#))
    }

    #[test]
    fn the_usual_folders_of_other_apps_are_found_in_any_case_on_internal_storage_and_every_card() {
        let disk = FakeDisk::with(&[
            "/storage/emulated/0/ROMs/gba/Golden Sun.gba",
            "/storage/emulated/0/ROMs/gba/Mother 3.gba",
            "/storage/emulated/0/ROMs/gba/readme.txt",
            "/storage/emulated/0/ROMs/bios/scph5501.bin",
            "/storage/emulated/0/Emulation/roms/snes/Chrono.sfc",
            "/storage/emulated/0/Music/song.mp3",
            "/storage/1234-ABCD/Roms/psx/Final Fantasy VII/FF7 (Disc 1).chd",
            "/storage/1234-ABCD/Roms/psx/Final Fantasy VII/FF7 (Disc 2).chd",
            "/storage/1234-ABCD/Roms/psx/Final Fantasy VII/FF7.m3u",
            "/storage/1234-ABCD/Roms/psx/Crash.chd",
            "/storage/1234-ABCD/Roms/gc/Wind Waker.rvz",
            "/storage/1234-ABCD/Roms/gamecube/Melee.iso",
            "/storage/self/primary/",
        ]);
        let found = find_game_folders(&disk, Path::new("/storage"), true);
        let rows: Vec<String> = found.iter().map(FoundFolder::row).collect();
        assert_eq!(rows, ["2 games in ROMs", "1 game in Emulation/roms", "4 games in Roms on the SD card"]);
        assert!(found[0].usual && !found[1].usual && found[2].usual && found[2].card);
        // A disc game's folder with its .m3u is one game; "gc" and "gamecube" are one system.
        assert_eq!(found[2].systems, vec![("psx", Some(2)), ("gamecube", Some(2))]);
        assert_eq!(systems_found(&found), ["gba", "snes", "psx", "gamecube"]);
        assert_eq!(found[2].document_uri().as_deref(), Some("content://com.android.externalstorage.documents/document/1234-ABCD%3ARoms"));
        assert_eq!(found[1].document_uri().as_deref(), Some("content://com.android.externalstorage.documents/document/primary%3AEmulation%2Froms"));
    }

    #[test]
    fn without_all_files_access_the_systems_are_known_and_the_games_not() {
        let mut disk = FakeDisk::with(&["/storage/emulated/0/ROMs/gba/A.gba", "/storage/emulated/0/ROMs/snes/B.sfc", "/storage/emulated/0/ROMs/bios/"]);
        disk.hide_files = true;
        let found = find_game_folders(&disk, Path::new("/storage"), false);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].games, None);
        assert_eq!(found[0].systems, vec![("gba", None), ("snes", None)]);
        assert_eq!(found[0].row(), "2 systems in ROMs");
    }

    #[test]
    fn empty_system_folders_a_lone_folder_on_a_card_and_android_s_own_folders_are_no_games_folder() {
        let disk = FakeDisk::with(&[
            // ES-DE made every system's folder, nothing was copied yet.
            "/storage/emulated/0/ROMs/gba/",
            "/storage/emulated/0/ROMs/snes/",
            // A card with photos and one folder that happens to be called "gb".
            "/storage/ABCD-1234/DCIM/photo.jpg",
            "/storage/ABCD-1234/gb/notes.txt",
            "/storage/emulated/0/Download/game.gba",
        ]);
        assert_eq!(find_game_folders(&disk, Path::new("/storage"), true), vec![]);
        // Nothing on the device at all.
        assert_eq!(find_game_folders(&FakeDisk::default(), Path::new("/storage"), true), vec![]);
    }

    #[test]
    fn a_card_with_system_folders_at_its_top_is_offered_as_the_card_itself() {
        let disk = FakeDisk::with(&["/storage/0A1B-2C3D/gba/A.gba", "/storage/0A1B-2C3D/nes/B.nes", "/storage/0A1B-2C3D/nes/C.zip"]);
        let found = find_game_folders(&disk, Path::new("/storage"), true);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].path, PathBuf::from("/storage/0A1B-2C3D"));
        assert_eq!(found[0].row(), "3 games on the SD card");
        assert_eq!(found[0].document_uri().as_deref(), Some("content://com.android.externalstorage.documents/document/0A1B-2C3D%3A"));
    }

    #[test]
    fn apps_are_offered_only_for_systems_the_player_lacks_found_ones_first_and_never_over_a_choice() {
        let p = phone(
            r#"{"dolphin": [{"p": "org.dolphinemu.dolphinemu", "l": "Dolphin Emulator"}], "ps2": [{"p": "com.armsx2", "l": "ARMSX2"}],
                "3ds": [{"p": "org.azahar_emu.azahar", "l": "Azahar"}], "psp": [{"p": "org.ppsspp.ppsspp", "l": "PPSSPP"}],
                "psx": [{"p": "com.github.stenzek.duckstation", "l": "DuckStation"}],
                "retroarch:mgba": [{"p": "com.retroarch.aarch64", "l": "RetroArch"}]}"#,
        );
        let offers = app_offers(&p, &Map::new(), &["3ds", "gba", "ps2", "gamecube", "psp"]);
        let rows: Vec<String> = offers.iter().map(AppOffer::row).collect();
        assert_eq!(rows, ["Azahar (Nintendo 3DS)", "ARMSX2 (PlayStation 2)", "Dolphin Emulator (GameCube / Wii)"]);
        // Systems without games get no offer, however many apps are installed (RetroArch takes many).
        assert!(app_offers(&p, &Map::new(), &["gba"]).is_empty());
        let ra = phone(r#"{"retroarch:clownmdemu": [{"p": "com.retroarch.aarch64", "l": "RetroArch"}], "retroarch:mame": [{"p": "com.retroarch.aarch64", "l": "RetroArch"}]}"#);
        let both = app_offers(&ra, &Map::new(), &["segacd", "neogeo"]);
        assert_eq!(both.len(), 2);
        assert_eq!(offer_names(&both), ["RetroArch"], "{both:?}");
        // PSP and PlayStation play in pocket-ui itself: named as "also on this device", never offered over the own player.
        assert_eq!(other_apps(&p), ["RetroArch", "PPSSPP", "DuckStation"]);
        // Taken: Plays with has them, and they are not offered again.
        let mut plays = Map::new();
        assert!(take_offers(&mut plays, &offers));
        assert_eq!(plays.get("ps2").map(String::as_str), Some("com.armsx2"));
        assert_eq!(plays.get("gamecube").map(String::as_str), Some("org.dolphinemu.dolphinemu"));
        assert!(!take_offers(&mut plays, &offers));
        assert!(app_offers(&p, &plays, &["3ds", "ps2", "gamecube"]).is_empty());
        // A choice whose app is gone from the device is no choice: the installed one is offered.
        plays.insert("ps2".into(), "xyz.aethersx2.android".into());
        assert_eq!(app_offers(&p, &plays, &["3ds", "ps2", "gamecube"]).iter().map(|o| o.system).collect::<Vec<_>>(), ["ps2"]);
        // No apps at all: nothing to offer, nothing to name.
        assert!(app_offers(&Phone::default(), &Map::new(), &["ps2"]).is_empty());
        assert!(other_apps(&Phone::default()).is_empty());
    }

    /// Every system only an app plays and pocket-ui can start in an app has exactly one official page, a plain https link
    /// of the project's own site (no mirror, no APK file, no tracking).
    #[test]
    fn every_app_only_system_has_one_official_download_page_and_it_is_offered_only_without_an_app() {
        for (system, app, url) in APP_DOWNLOADS {
            assert!(url.starts_with("https://") && !url.contains(' ') && !url.contains('?') && !url.ends_with(".apk"), "{app}: {url}");
            assert!(crate::play_route::bundled_core(system).is_none(), "{system} plays in pocket-ui: no app to get");
            assert_eq!(APP_DOWNLOADS.iter().filter(|(s, ..)| s == system).count(), 1, "{system} listed once");
            assert!(official_page(url));
        }
        // Every system only an app plays whose app pocket-ui can start (the profiles `EmulatorIntents.apps` knows) has its entry.
        let startable = ["ps2", "dolphin", "3ds", "xbox"];
        for system in app_only_systems().filter(|s| crate::play_route::profile_of(s).is_some_and(|p| startable.contains(&p))) {
            assert!(APP_DOWNLOADS.iter().any(|(s, ..)| *s == system), "{system} has no download page");
        }
        assert_eq!(APP_DOWNLOADS.len(), startable.len());
        let p = phone(r#"{"dolphin": [{"p": "org.dolphinemu.dolphinemu", "l": "Dolphin"}]}"#);
        let get = apps_to_get(&p, &["gba", "ps2", "gamecube", "3ds"]);
        assert_eq!(get.iter().map(AppToGet::row).collect::<Vec<_>>(), ["Get ARMSX2 (PlayStation 2)", "Get Azahar (Nintendo 3DS)"]);
        // No such game found: nothing to get.
        assert!(apps_to_get(&Phone::default(), &["gba"]).is_empty());
        assert!(!official_page("https://example.com/armsx2.apk"));
    }

    #[test]
    fn a_missing_bios_is_said_for_found_systems_only_saturn_as_needed_the_others_as_optional() {
        let none = |_: &str| false;
        assert_eq!(missing_bios(&["gba", "snes"], none), vec![]);
        assert_eq!(
            missing_bios(&["psx", "dreamcast", "saturn"], none),
            vec![BiosNeed { system: "saturn", required: true }, BiosNeed { system: "psx", required: false }, BiosNeed { system: "dreamcast", required: false }]
        );
        let names = ["SCPH5501.BIN", "dc_boot.bin", "notes.txt"];
        let has = |s: &str| bios_among(s, names.iter().copied());
        assert_eq!(missing_bios(&["psx", "dreamcast", "saturn"], has), vec![BiosNeed { system: "saturn", required: true }]);
        // Dreamcast's flash alone is no boot BIOS.
        assert!(!bios_among("dreamcast", ["dc_flash.bin"]));
        let line = BiosNeed { system: "saturn", required: true }.line(false);
        assert!(line.contains("ROMs/bios") && line.contains("saturn_bios.bin"), "{line}");
        // The bios folder is found in any case, Dreamcast's in bios/dc.
        let disk = FakeDisk::with(&["/r/BIOS/scph5501.bin", "/r/BIOS/dc/dc_boot.bin"]);
        assert_eq!(bios_names(&disk, &[PathBuf::from("/r")]), ["scph5501.bin", "dc_boot.bin"]);
    }

    /// The folders file under every fault (missing, empty, cut short, flipped bits, another encoding): never a panic, never a
    /// folder that isn't there, the valid file's folder kept, and a new folder can always be added after it.
    #[test]
    fn file_source_rule_first_start_games_folders() {
        let data = tempfile::tempdir().unwrap();
        let games = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        let seed = format!("{}\n", games.path().display()).into_bytes();
        for (label, bytes) in crate::file_faults::faults(&seed) {
            let _ = std::fs::remove_file(data.path().join(KEPT));
            if let Some(bytes) = bytes {
                std::fs::write(data.path().join(KEPT), bytes).unwrap();
            }
            let kept = kept_folders(data.path());
            assert!(kept.iter().all(|p| p.is_dir()), "{label}: {kept:?}");
            if label == "valid" {
                assert_eq!(kept, [games.path().to_path_buf()]);
            }
            keep_folder(data.path(), other.path());
            assert!(kept_folders(data.path()).contains(&other.path().to_path_buf()), "{label}");
        }
    }

    #[test]
    fn a_taken_folder_is_kept_once_and_one_gone_since_is_left_out() {
        let data = tempfile::tempdir().unwrap();
        let games = tempfile::tempdir().unwrap();
        assert!(kept_folders(data.path()).is_empty());
        keep_folder(data.path(), games.path());
        keep_folder(data.path(), games.path());
        keep_folder(data.path(), Path::new("/nowhere/roms"));
        assert_eq!(kept_folders(data.path()), [games.path().to_path_buf()]);
    }

    /// Every line this module makes has its pattern in all five tables (the painter translates by the English words).
    #[test]
    fn every_line_of_the_first_start_is_translated_in_every_language() {
        let found = FoundFolder { path: PathBuf::from("/storage/emulated/0/ROMs"), card: false, systems: vec![("gba", Some(12))], games: Some(12), usual: true };
        let card = FoundFolder { path: PathBuf::from("/storage/1234-ABCD/Roms"), card: true, systems: vec![("gba", None), ("nes", None)], games: None, usual: true };
        let one = FoundFolder { games: Some(1), ..found.clone() };
        let top = FoundFolder { path: PathBuf::from("/storage/1234-ABCD"), ..card.clone() };
        let mut lines = vec![
            found.row(),
            card.row(),
            one.row(),
            top.row(),
            FoundFolder { games: Some(1), ..top.clone() }.row(),
            FoundFolder { games: Some(1), ..card.clone() }.row(),
            FoundFolder { games: None, systems: vec![("gba", None)], ..found.clone() }.row(),
            FoundFolder { games: None, systems: vec![("gba", None)], ..top.clone() }.row(),
            apps_note(&[AppOffer { system: "ps2", app: PhoneApp { package: "com.armsx2".into(), label: "ARMSX2".into() } }]),
        ];
        for system in ["saturn", "psx", "dreamcast"] {
            for in_chosen in [false, true] {
                lines.push(BiosNeed { system, required: system == "saturn" }.line(in_chosen).to_string());
            }
        }
        // The words the two views add around them (`setup.rs` "Your games", `android/src/onboarding.rs`).
        lines.extend(WORDS.iter().map(|w| w.to_string()));
        lines.extend(APP_DOWNLOADS.iter().map(|(s, a, u)| AppToGet { system: s, app: a, url: u }.row()));
        for lang in crate::i18n::Lang::ALL.into_iter().filter(|l| *l != crate::i18n::Lang::En) {
            for line in &lines {
                assert_ne!(&crate::i18n::tr_in(lang, line), line, "{lang:?}: {line}");
            }
        }
    }
}
