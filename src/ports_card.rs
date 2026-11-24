//! PortMaster games from the card, for handhelds without Wi-Fi (the RG28XX, the RG35XX 2024, the R36S) or with Wi-Fi
//! off: the player downloads a game's zip on a computer (portmaster.games), with the runtime it needs, and puts them in
//! the folder PortMaster's own guide names for ROCKNIX, `ports/autoinstall` (docs/user/extras.md "Without Wi-Fi").
//! Nothing is downloaded or hosted by us.
//!
//! What PortMaster does with that folder when its own menu opens (`PortMaster.sh`, which pocket-ui never opens), done
//! here the same way: runtime files (`*.squashfs`) and runtime packages ("All Runtimes.zip": a zip of `*.squashfs`,
//! the architecture in the zip's comment) go into `PortMaster/libs`; each game's zip is installed by PortMaster's
//! `harbourmaster install <file>` (`ports::install_file`), which needs no network for a file. The file is deleted from
//! the card after a good install (as PortMaster does); a file that can't be installed stays, with the reason.
//!
//! Checked before anything is installed, from the zip's `port.json` alone: a game built for another kind of handheld
//! (`attr.arch` without aarch64), and a runtime it needs (`attr.runtime`) that is neither installed nor on the card.
//! harbourmaster itself installs such a game and says nothing (its runtime would be fetched from the internet at the
//! game's first start), so the game would not start without telling why.
//!
//! Where it looks (`dirs_in`): `ports/autoinstall` (and `roms/ports/autoinstall`) in each place the update from the card
//! looks (`update::card::places`: the first part of the card, a games card in the second slot, `/storage/roms`, which
//! is the RG28XX's GAMES drive), and PortMaster's own `PortMaster/autoinstall`.

use std::path::{Path, PathBuf};

/// The folder, as the player is told it.
pub const FOLDER: &str = "ports/autoinstall";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A game's zip: its title (`attr.title`, else the file's name), the runtimes it needs as file names, and the script
    /// that starts it (its first `.sh` item), which is there once it is installed.
    Game { title: String, runtimes: Vec<String>, launcher: Option<String> },
    /// One runtime file.
    Runtime,
    /// A runtime package: the runtime files in it.
    Runtimes(Vec<String>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Problem {
    /// Runtimes the game needs that are neither installed nor on the card (file names).
    NeedsRuntime(Vec<String>),
    /// Built for another kind of handheld (a PC, or 32-bit ones).
    OtherHandheld,
    /// Not a zip that can be read, or a runtime file that isn't one: cut off on the way, most likely.
    Damaged,
    /// A zip that is neither a PortMaster game nor a runtime package (PortMaster itself, a theme, something else).
    NotAGame,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    pub path: PathBuf,
    pub kind: Kind,
    pub problem: Option<Problem>,
    /// Bytes it takes once installed (unpacked).
    pub size: u64,
}

impl Item {
    pub fn file_name(&self) -> String {
        self.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
    }

    /// What the list shows as its name.
    pub fn title(&self) -> String {
        match &self.kind {
            Kind::Game { title, .. } => title.clone(),
            _ => self.file_name(),
        }
    }

    pub fn ready(&self) -> bool {
        self.problem.is_none()
    }

    /// The short word on its row's right.
    pub fn label(&self) -> &'static str {
        match (&self.problem, &self.kind) {
            (Some(Problem::NeedsRuntime(_)), _) => "Needs a runtime",
            (Some(Problem::OtherHandheld), _) => "Not for this handheld",
            (Some(Problem::Damaged), _) => "Damaged",
            (Some(Problem::NotAGame), _) => "Not a game",
            (None, Kind::Game { .. }) => "Ready to install",
            (None, _) => "Runtime",
        }
    }

    /// The line under the list for the selected row.
    pub fn detail(&self) -> String {
        match &self.problem {
            Some(Problem::NeedsRuntime(names)) => format!("Needs {}. Grab it on portmaster.games under Runtimes and drop it into ports/autoinstall.", names.join(", ")),
            Some(Problem::OtherHandheld) => "Built for a different handheld. Download the aarch64 version instead.".into(),
            Some(Problem::Damaged) => "Incomplete or damaged file. Copy it onto the card again.".into(),
            Some(Problem::NotAGame) => "Not a PortMaster game or runtime zip. Left on the card.".into(),
            None => match &self.kind {
                Kind::Game { runtimes, .. } if !runtimes.is_empty() => format!("Ready to install. Uses {}.", runtimes.join(", ")),
                Kind::Game { .. } => "Ready to install.".into(),
                Kind::Runtime => "Runtime needed by some games. Installs into PortMaster.".into(),
                Kind::Runtimes(names) => format!("Pack of {} runtimes. Installs into PortMaster.", names.len()),
            },
        }
    }
}

/// The folders to look in, for these places of the card and PortMaster's folder; each once.
pub fn dirs_in(places: &[PathBuf], pm_dir: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let candidates = places.iter().flat_map(|p| [p.join(FOLDER), p.join("roms").join(FOLDER)]).chain([pm_dir.join("autoinstall")]);
    for dir in candidates {
        use std::os::unix::fs::MetadataExt;
        let Ok(meta) = std::fs::metadata(&dir) else { continue };
        if meta.is_dir() && seen.insert((meta.dev(), meta.ino())) {
            out.push(dir);
        }
    }
    out
}

/// This handheld's folders.
pub fn dirs() -> Vec<PathBuf> {
    dirs_in(&crate::update::card::places(), &crate::ports::pm_dir())
}

/// Whether this handheld installs from the card: where pocket-ui runs PortMaster itself (ROCKNIX). The TrimUI card
/// gets its PortMaster over Wi-Fi; on dArkOS PortMaster's own menu does this.
pub fn offered() -> bool {
    crate::ports::offered() && !crate::ports::trimui_standalone() && !crate::device::darkos_port()
}

fn lower_name(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default()
}

/// A runtime's name as PortMaster writes it in a port.json ("frt_3.5.2"), as its file name ("frt_3.5.2.squashfs").
fn runtime_file(name: &str) -> String {
    let name = name.trim();
    if name.to_lowercase().ends_with(".squashfs") { name.to_string() } else { format!("{name}.squashfs") }
}

/// PortMaster names the downloads of other architectures `<runtime>.x86_64.squashfs` / `.armhf.squashfs`, and a few
/// put it in the name ("mono-6.12.0.122-aarch64").
fn other_arch(name: &str) -> bool {
    let n = name.to_lowercase();
    ["x86_64", "x86-64", "armhf", "i386", "i686"].iter().any(|a| n.contains(a))
}

/// What is in `dirs`, with what `libs` (PortMaster's runtime folder) already holds. Sorted: games first, by title.
pub fn scan(dirs: &[PathBuf], libs: &Path) -> Vec<Item> {
    let mut files: Vec<PathBuf> = dirs.iter()
        .filter_map(|d| std::fs::read_dir(d).ok())
        .flat_map(|list| list.flatten().map(|e| e.path()).collect::<Vec<_>>())
        .filter(|p| p.is_file() && !lower_name(p).starts_with('.'))
        .filter(|p| { let n = lower_name(p); n.ends_with(".zip") || n.ends_with(".squashfs") })
        .collect();
    files.sort();
    let mut items: Vec<Item> = files.into_iter().map(|p| look_at(&p)).collect();
    // Runtimes on the card count as there for the games beside them.
    let on_card: std::collections::HashSet<String> = items.iter().filter(|i| i.ready()).flat_map(|i| match &i.kind {
        Kind::Runtime => vec![i.file_name().to_lowercase()],
        Kind::Runtimes(names) => names.iter().map(|n| n.to_lowercase()).collect(),
        Kind::Game { .. } => vec![],
    }).collect();
    for item in &mut items {
        if let (None, Kind::Game { runtimes, .. }) = (&item.problem, &item.kind) {
            let missing: Vec<String> = runtimes.iter()
                .filter(|r| !libs.join(r.as_str()).is_file() && !on_card.contains(&r.to_lowercase()))
                .cloned().collect();
            if !missing.is_empty() { item.problem = Some(Problem::NeedsRuntime(missing)); }
        }
    }
    items.sort_by_key(|i| (!matches!(i.kind, Kind::Game { .. }), i.title().to_lowercase()));
    items
}

fn look_at(path: &Path) -> Item {
    let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    let item = |kind, problem, size| Item { path: path.to_path_buf(), kind, problem, size };
    if lower_name(path).ends_with(".squashfs") {
        let mut magic = [0u8; 4];
        let whole = std::fs::File::open(path).and_then(|mut f| std::io::Read::read_exact(&mut f, &mut magic)).is_ok() && &magic == b"hsqs";
        let problem = if !whole { Some(Problem::Damaged) } else if other_arch(&lower_name(path)) { Some(Problem::OtherHandheld) } else { None };
        return item(Kind::Runtime, problem, size);
    }
    let port_json = |name: &str| {
        let n = name.to_lowercase();
        n.matches('/').count() == 1 && n.ends_with("port.json")
    };
    let listing = match crate::archive::listing(path, port_json) {
        Ok(Some(l)) => l,
        _ => return item(Kind::Game { title: file_title(path), runtimes: vec![], launcher: None }, Some(Problem::Damaged), size),
    };
    let unpacked: u64 = listing.entries.iter().map(|(_, s)| s).sum();
    let Some((_, json)) = &listing.picked else {
        let files: Vec<&str> = listing.entries.iter().map(|(n, _)| n.as_str()).filter(|n| !n.ends_with('/')).collect();
        let runtimes: Vec<String> = files.iter().filter(|n| n.to_lowercase().ends_with(".squashfs")).map(|n| n.rsplit('/').next().unwrap_or(n).to_string()).collect();
        // A runtime package: runtime files and their checksums, nothing else.
        if !runtimes.is_empty() && files.iter().all(|n| { let n = n.to_lowercase(); n.ends_with(".squashfs") || n.ends_with(".md5") }) {
            let arch = listing.comment.split_whitespace().next().unwrap_or("").to_lowercase();
            let other = (!arch.is_empty() && arch != "aarch64") || other_arch(&lower_name(path));
            return item(Kind::Runtimes(runtimes), other.then_some(Problem::OtherHandheld), unpacked);
        }
        return item(Kind::Game { title: file_title(path), runtimes: vec![], launcher: None }, Some(Problem::NotAGame), size);
    };
    let info: serde_json::Value = match serde_json::from_slice(json) {
        Ok(v) => v,
        Err(_) => return item(Kind::Game { title: file_title(path), runtimes: vec![], launcher: None }, Some(Problem::Damaged), size),
    };
    let attr = &info["attr"];
    let title = attr["title"].as_str().map(str::trim).filter(|t| !t.is_empty()).map(String::from).unwrap_or_else(|| file_title(path));
    let runtimes: Vec<String> = match &attr["runtime"] {
        serde_json::Value::String(s) if !s.trim().is_empty() => vec![runtime_file(s)],
        serde_json::Value::Array(list) => list.iter().filter_map(|v| v.as_str()).filter(|s| !s.trim().is_empty()).map(runtime_file).collect(),
        _ => vec![],
    };
    let arch: Vec<String> = attr["arch"].as_array().map(|a| a.iter().filter_map(|v| v.as_str()).map(str::to_lowercase).collect()).unwrap_or_default();
    let other = !arch.is_empty() && !arch.iter().any(|a| a == "aarch64" || a == "arm64");
    let launcher = info["items"].as_array().into_iter().flatten().filter_map(|v| v.as_str())
        .find(|i| i.to_lowercase().ends_with(".sh") && !i.contains('/')).map(String::from);
    item(Kind::Game { title, runtimes, launcher }, other.then_some(Problem::OtherHandheld), unpacked)
}

fn file_title(path: &Path) -> String {
    path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
}

/// The ways out of this module, so the tests can stand in for PortMaster and the card.
pub struct Installer<'a> {
    /// Sets PortMaster up when it isn't yet (`ports::ensure_ready`). First of all: its setup replaces a PortMaster folder
    /// without PortMaster in it, so a runtime copied into `libs` before it was lost (R36S test.7, 23.11.2026).
    pub prepare: &'a (dyn Fn() -> Result<(), String> + Sync),
    /// PortMaster's runtime folder.
    pub libs: PathBuf,
    /// Where the games are installed (their start scripts).
    pub ports: PathBuf,
    /// Installs a game's zip (`ports::install_file`).
    pub game: &'a (dyn Fn(&Path) -> Result<(), String> + Sync),
    /// Unpacks a runtime package into a folder, its files side by side (`unzip`, as PortMaster.sh does).
    pub package: &'a (dyn Fn(&Path, &Path) -> Result<(), String> + Sync),
    /// Free bytes where `libs` and the games are.
    pub free: &'a (dyn Fn(&Path) -> Option<u64> + Sync),
    /// Takes the file off the card (`update::card::remove_after_use`).
    pub remove: &'a (dyn Fn(&Path) -> bool + Sync),
}

/// What happened to one file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Outcome {
    pub title: String,
    pub game: bool,
    pub result: Result<(), String>,
}

/// Installs the ready items: runtimes first, so the games after them find theirs. Items with a problem are left alone.
/// `progress`: file `done` of `total` and its title, as each starts, for the page.
/// The order: PortMaster set up, then the runtimes, then the games. Files leave the card only at the end, once the
/// runtimes are checked to be in PortMaster still.
pub fn install(items: &[Item], how: &Installer, progress: &dyn Fn(usize, usize, &str)) -> Vec<Outcome> {
    let mut todo: Vec<&Item> = items.iter().filter(|i| i.ready()).collect();
    todo.sort_by_key(|i| matches!(i.kind, Kind::Game { .. }));
    let total = todo.len();
    let outcome = |item: &Item, result| Outcome { title: item.title(), game: matches!(item.kind, Kind::Game { .. }), result };
    if total == 0 { return Vec::new(); }
    // (The page already says "Installing 1 of …" while PortMaster is set up.)
    if let Err(e) = (how.prepare)() {
        progress(total, total, "");
        return todo.into_iter().map(|i| outcome(i, Err(e.clone()))).collect();
    }
    let mut results = Vec::new();
    for (n, item) in todo.iter().enumerate() {
        progress(n, total, &item.title());
        results.push(install_one(item, how));
    }
    // A runtime that isn't where the games look for it after all of it: said, and its file stays on the card.
    let mut out = Vec::new();
    for (item, result) in todo.into_iter().zip(results) {
        let result = result.and_then(|()| if runtimes_in_place(item, &how.libs) { Ok(()) } else { Err(NOT_IN_PORTMASTER.into()) });
        let result = result.and_then(|()| if (how.remove)(&item.path) { Ok(()) } else { Err("Installed, but couldn't remove the file from the card.".into()) });
        out.push(outcome(item, result));
    }
    progress(total, total, "");
    out
}

/// A runtime that is not in PortMaster's folder after the install.
pub const NOT_IN_PORTMASTER: &str = "Couldn't install the runtime into PortMaster. Still on the card; press A to try again.";

/// Whether a runtime item's files are in `libs`, whole (a game's are its own business).
fn runtimes_in_place(item: &Item, libs: &Path) -> bool {
    match &item.kind {
        Kind::Runtime => std::fs::metadata(libs.join(item.file_name())).is_ok_and(|m| m.len() == item.size),
        Kind::Runtimes(names) => names.iter().all(|n| libs.join(n).is_file()),
        Kind::Game { .. } => true,
    }
}

fn install_one(item: &Item, how: &Installer) -> Result<(), String> {
    let enough = |at: &Path, need: u64| match (how.free)(at) {
        Some(free) if free < need.saturating_add(16 << 20) => Err(format!("Not enough space on the SD card: {} MiB free, {} MiB needed.", free >> 20, need.div_ceil(1 << 20))),
        _ => Ok(()),
    };
    match &item.kind {
        Kind::Runtime => {
            std::fs::create_dir_all(&how.libs).map_err(|e| e.to_string())?;
            enough(&how.libs, item.size)?;
            let target = how.libs.join(item.file_name());
            let part = how.libs.join(format!(".{}.part", item.file_name()));
            let copied = std::fs::copy(&item.path, &part).and_then(|_| std::fs::rename(&part, &target));
            if copied.is_err() { let _ = std::fs::remove_file(&part); }
            copied.map_err(|e| format!("Couldn't copy the runtime: {e}"))?;
            crate::fsutil::sync_dir(&how.libs);
            Ok(())
        }
        Kind::Runtimes(_) => {
            std::fs::create_dir_all(&how.libs).map_err(|e| e.to_string())?;
            enough(&how.libs, item.size)?;
            (how.package)(&item.path, &how.libs)
        }
        Kind::Game { .. } => {
            // The games' folder is the one over PortMaster's: the same part of the card.
            enough(&how.libs, item.size)?;
            (how.game)(&item.path)?;
            match &item.kind {
                Kind::Game { launcher: Some(l), .. } if !how.ports.join(l).exists() => Err("PortMaster finished, but the game is missing.".into()),
                _ => Ok(()),
            }
        }
    }
}

/// The line the page shows when an install has ended.
pub fn summary(outcomes: &[Outcome]) -> (String, bool) {
    let failed: Vec<&Outcome> = outcomes.iter().filter(|o| o.result.is_err()).collect();
    if let Some(first) = failed.first() {
        let e = first.result.as_ref().err().cloned().unwrap_or_default();
        // Two sentences, so the reason is translated whole: "{s} wasn't installed. {t}".
        return (if failed.len() == 1 { format!("{} wasn't installed. {e}", first.title) } else { format!("{} files weren't installed. {e}", failed.len()) }, true);
    }
    let games = outcomes.iter().filter(|o| o.game).count();
    match games {
        0 => ("Runtimes installed.".into(), false),
        1 => (format!("{} is installed. Find it under Ports.", outcomes.iter().find(|o| o.game).map(|o| o.title.as_str()).unwrap_or("")), false),
        n => (format!("{n} games installed. Find them under Ports."), false),
    }
}

/// The real ways: PortMaster's harbourmaster, `unzip` for a runtime package, the card's free space, and the removal the
/// update from the card uses (it makes the read-only first part writable for that moment).
pub fn install_here(items: &[Item], progress: &dyn Fn(usize, usize, &str)) -> Vec<Outcome> {
    let game = |p: &Path| crate::ports::install_file(p);
    let package = |zip: &Path, libs: &Path| {
        let out = std::process::Command::new("unzip").args(["-o", "-j", "-qq"]).arg(zip).arg("-d").arg(libs).output().map_err(|e| e.to_string())?;
        crate::fsutil::sync_dir(libs);
        if out.status.success() { Ok(()) } else { Err(String::from_utf8_lossy(&out.stderr).lines().last().unwrap_or("Couldn't unpack the runtimes.").trim().to_string()) }
    };
    let free = |p: &Path| free_bytes(p);
    let remove = |p: &Path| crate::update::card::remove_after_use(p);
    install(items, &Installer { prepare: &crate::ports::ensure_ready, libs: crate::ports::pm_dir().join("libs"), ports: crate::ports::PORTS_DIR.into(), game: &game, package: &package, free: &free, remove: &remove }, progress)
}

fn free_bytes(path: &Path) -> Option<u64> {
    use std::os::unix::ffi::OsStrExt;
    let c = std::ffi::CString::new(path.as_os_str().as_bytes()).ok()?;
    // SAFETY: statvfs fills the zeroed struct for a valid C string path.
    unsafe {
        let mut st: libc::statvfs = std::mem::zeroed();
        (libc::statvfs(c.as_ptr(), &mut st) == 0).then(|| st.f_bavail as u64 * st.f_frsize as u64)
    }
}

#[cfg(test)]
#[path = "ports_card_tests.rs"]
mod tests;
