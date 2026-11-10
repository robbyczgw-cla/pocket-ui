//! Settings → Backup & reset: the player's saves and save states copied to a dated folder and put back from one. Flashing
//! a card again is the only way back from a bad system update, and it erases what is on that card.
//!
//! What goes in is decided by one function, [`wanted`], for a backup and for a restore alike: battery saves and states
//! beside the games, `savestates/`, and the standalone emulators' save folders as Sync knows them (`sync::Share`). No game
//! and no BIOS file. A folder holds `files/` and a `saves.json` listing every file's place and time. That list has its
//! own name and the folders their own place, so settings backups don't list them and Storage's clean-up never deletes one.
//! A file name the backup's file system can't hold (a second card in exFAT) gets another name there, the list has the real one.
//!
//! A restore never overwrites what is newer: that file stays, and the backup's copy stays in the backup. What it does replace
//! is copied to a backup of its own first (`Reason::BeforeRestore`). Every call reads and writes files: run them on a thread.

use std::collections::HashSet;
use std::io::Read;
#[cfg(not(feature = "brick"))]
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, UNIX_EPOCH};

use crate::backup::Reason;

/// Where the folders go: beside the settings backups, on the games part of the card. A second card holds it, if there is one.
pub const ROOT: &str = "/storage/roms/backup/pocket-ui-saves";
/// The list in every folder. Not `backup::MANIFEST`: an older pocket-ui must not take this for a backup of its settings.
pub const MANIFEST: &str = "saves.json";
/// Paths in a backup are relative to this.
pub const STORAGE: &str = "/storage";
const FILES: &str = "files";
const MAKING: &str = ".making-saves-";
/// File times closer than this are the same time: FAT family file systems and copies through a computer round them.
const SAME_TIME: u64 = 2;
/// Beyond the files themselves.
const SPARE: u64 = 16 << 20;
/// Written to the card this often, so the count on the page follows the card and not its cache, which a slow card would
/// otherwise empty for minutes at the end, with the page at the last file.
const FLUSH_EVERY: u64 = 32 << 20;
/// Top folders of the games part that are not walked: backups of backups, and the versions Sync set aside.
const NEVER: &[&str] = &["backup", "backups", "sync-backups"];

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Item {
    /// Relative to `/storage`: `roms/gba/Game.srm`.
    pub path: String,
    /// Relative to the backup's folder.
    pub stored: String,
    pub bytes: u64,
    /// Unix time in seconds.
    pub modified: u64,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Manifest {
    pub version: u32,
    pub created: u64,
    pub reason: Reason,
    pub app: String,
    pub items: Vec<Item>,
    /// What could not be read when it was made, by place.
    #[serde(default)]
    pub unreadable: Vec<String>,
}

/// A saves backup on the card.
#[derive(Clone, Debug, PartialEq)]
pub struct SavesBackup {
    pub dir: PathBuf,
    pub manifest: Manifest,
}

impl SavesBackup {
    pub fn bytes(&self) -> u64 {
        self.manifest.items.iter().map(|i| i.bytes).sum()
    }
}

/// What a job has done, for the page to read while it paints; `cancel` stops it between two files.
#[derive(Debug, Default)]
pub struct Progress {
    pub done: AtomicUsize,
    pub total: AtomicUsize,
    pub cancel: AtomicBool,
}

/// `pattern` of a `.stignore` line, from the start of `path`: Syncthing's `*` stays within a folder, and a folder that matches
/// holds what is in it.
fn line_matches(pattern: &str, path: &str) -> bool {
    fn glob(p: &str, s: &str) -> bool {
        match p.split_once('*') {
            None => p == s,
            Some((a, b)) => s.len() >= a.len() + b.len() && s.starts_with(a) && s.ends_with(b),
        }
    }
    let mut parts = path.split('/');
    pattern.trim_start_matches('/').split('/').all(|p| parts.next().is_some_and(|s| glob(p, s)))
}

/// A `Dir`'s `.stignore` asked for one file below it: does it share it?
fn shared(ignore: &[&str], sub: &str) -> bool {
    for line in ignore {
        let (keep, pattern) = match line.strip_prefix('!') {
            Some(p) => (true, p),
            None => (false, *line),
        };
        if pattern == "*" || line_matches(pattern, sub) {
            return keep;
        }
    }
    true
}

fn is_save_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.contains(".pstate") || crate::sync::save_extensions().any(|e| lower.ends_with(&format!(".{e}")))
}

/// Whether a backup holds, and a restore writes, this path relative to `/storage`. A list edited over the file share can't
/// reach anything else.
pub fn wanted(rel: &str) -> bool {
    let plain = !rel.is_empty() && !rel.contains('\\') && Path::new(rel).components().all(|c| matches!(c, std::path::Component::Normal(_)));
    if !plain {
        return false;
    }
    use crate::sync::Share;
    for dir in [Share::Ps2Cards, Share::Dolphin, Share::Azahar].iter().flat_map(|s| s.dirs()) {
        let base = dir.path.trim_start_matches(STORAGE).trim_start_matches('/');
        let Some(sub) = rel.strip_prefix(base).and_then(|s| s.strip_prefix('/')) else { continue };
        if shared(dir.ignore, sub) {
            return true;
        }
    }
    // ScummVM's saves have many endings (`monkey.s01`) and nothing else lives in that folder.
    if rel.strip_prefix(scummvm_saves()).is_some_and(|sub| sub.starts_with('/') && sub.len() > 1) {
        return true;
    }
    let Some(rest) = rel.strip_prefix("roms/") else { return false };
    let (top, below) = rest.split_once('/').unwrap_or(("", rest));
    if top == "savestates" {
        return true;
    }
    // Beside the games. BIOS files and the 3DS's installed games have their own rule above, and nothing else of them counts.
    if NEVER.contains(&top) || top == "bios" || top.starts_with('.') || rest.starts_with("3ds/azahar/") {
        return false;
    }
    is_save_name(Path::new(below).file_name().and_then(|n| n.to_str()).unwrap_or(""))
}

/// ScummVM's save folder relative to `/storage`, outside the games.
fn scummvm_saves() -> &'static str {
    crate::hostsys::SCUMMVM_SAVES.trim_start_matches(STORAGE).trim_start_matches('/')
}

fn seconds(m: &std::fs::Metadata) -> u64 {
    m.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs())
}

/// All that `wanted` takes under `storage`, sorted, and how many entries could not be looked at or named. Names only, a
/// `stat` for what is taken: a card full of games costs its folder listings.
fn walk(storage: &Path) -> (Vec<Item>, usize) {
    let mut stack: Vec<String> = vec!["roms".into()];
    stack.extend(crate::sync::Share::Dolphin.dirs().iter().map(|d| d.path.trim_start_matches(STORAGE).trim_start_matches('/').to_string()).filter(|p| !p.starts_with("roms/")));
    stack.push(scummvm_saves().to_string());
    let (mut out, mut skipped) = (Vec::new(), 0);
    while let Some(rel) = stack.pop() {
        let rd = match std::fs::read_dir(storage.join(&rel)) {
            Ok(rd) => rd,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => {
                skipped += 1;
                continue;
            }
        };
        for e in rd {
            let Ok(e) = e else {
                skipped += 1;
                continue;
            };
            let name = e.file_name();
            let Some(name) = name.to_str() else {
                skipped += 1;
                continue;
            };
            let child = format!("{rel}/{name}");
            let Ok(kind) = e.file_type() else { continue };
            if kind.is_dir() {
                if rel != "roms" || !(NEVER.contains(&name) || name.starts_with('.')) {
                    stack.push(child);
                }
            } else if kind.is_file() && wanted(&child) {
                match e.metadata() {
                    Ok(m) => out.push(Item { path: child, stored: String::new(), bytes: m.len(), modified: seconds(&m) }),
                    Err(_) => skipped += 1,
                }
            }
        }
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    (out, skipped)
}

/// Free bytes and the size of a cluster on the file system that will hold `path`, the nearest folder that exists asked.
fn room(path: &Path) -> Option<(u64, u64)> {
    let there = path.ancestors().find(|p| p.exists())?;
    let (free, _) = crate::storage::free_space(there)?;
    let c = std::ffi::CString::new(std::os::unix::ffi::OsStrExt::as_bytes(there.as_os_str())).ok()?;
    // SAFETY: statvfs only writes into the struct we pass.
    let mut st: libc::statvfs = unsafe { std::mem::zeroed() };
    (unsafe { libc::statvfs(c.as_ptr(), &mut st) } == 0).then_some((free, (st.f_bsize as u64).max(512)))
}

/// What the files take on a file system of this cluster size, with their folders' entries and a little spare.
fn needed(items: &[Item], cluster: u64) -> u64 {
    items.iter().map(|i| i.bytes.div_ceil(cluster) * cluster + cluster).sum::<u64>() + SPARE
}

/// The page's first step: what a backup would hold and whether it fits.
#[derive(Clone, Debug, PartialEq)]
pub struct Plan {
    pub items: Vec<Item>,
    pub bytes: u64,
    /// None: the file system didn't say.
    pub free: Option<u64>,
    pub needed: u64,
    pub skipped: usize,
    /// The games part is a second card: flashing the system card leaves the backup alone.
    pub second_card: bool,
}

impl Plan {
    pub fn fits(&self) -> bool {
        self.free.is_none_or(|f| f >= self.needed)
    }
}

/// A games card in the second slot, as the mount table says.
pub fn second_card() -> bool {
    let mounts = std::fs::read_to_string("/proc/mounts").unwrap_or_default();
    matches!(crate::games_card::card_of(&mounts, |p| p.exists()), crate::games_card::Card::In(_))
}

pub fn measure(storage: &Path, root: &Path, second_card: bool) -> Plan {
    let (items, skipped) = walk(storage);
    let bytes = items.iter().map(|i| i.bytes).sum();
    let (free, needed) = match room(root) {
        Some((free, cluster)) => (Some(free), needed(&items, cluster)),
        None => (None, 0),
    };
    Plan { items, bytes, free, needed, skipped, second_card }
}

/// The name of `rel` on any file system: what FAT and exFAT refuse becomes `_`, and two that differ only there or in case
/// get a number.
fn stored_name(rel: &str, used: &mut HashSet<String>) -> String {
    let clean = |part: &str| -> String {
        let s: String = part.chars().map(|c| if c.is_control() || "<>:\"|?*\\".contains(c) { '_' } else { c }).collect();
        let t = s.trim_end_matches(['.', ' ']);
        if t.is_empty() { "_".to_string() } else { t.to_string() }
    };
    let base = format!("{FILES}/{}", rel.split('/').map(clean).collect::<Vec<_>>().join("/"));
    let mut name = base.clone();
    let mut n = 2;
    while !used.insert(name.to_lowercase()) {
        name = format!("{base}~{n}");
        n += 1;
    }
    name
}

/// What stopped a copy: the file we read, or the place we write. The first is one file lost, the second ends the backup.
enum CopyError {
    Source(std::io::Error),
    Target(std::io::Error),
}

/// Copies one file and gives the copy the time `modified`, or the source's. Returns the source's size and time as read.
fn copy_file(from: &Path, to: &Path, modified: Option<u64>) -> Result<(u64, u64), CopyError> {
    #[cfg(feature = "brick")]
    let _idle = crate::update::brick_io::IdleIo::enter().map_err(CopyError::Target)?;
    #[cfg(feature = "brick")]
    let mut pacer = crate::update::brick_io::Pacer::new();
    let mut src = std::fs::File::open(from).map_err(CopyError::Source)?;
    let meta = src.metadata().map_err(CopyError::Source)?;
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent).map_err(CopyError::Target)?;
    }
    let mut dst = std::fs::File::create(to).map_err(CopyError::Target)?;
    let mut buf = vec![0u8; 256 * 1024];
    let mut total = 0;
    loop {
        let n = match src.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => {
                drop(dst);
                let _ = std::fs::remove_file(to);
                return Err(CopyError::Source(e));
            }
        };
        #[cfg(feature = "brick")]
        let written = pacer.write(&buf[..n], &mut dst);
        #[cfg(not(feature = "brick"))]
        let written = dst.write_all(&buf[..n]);
        written.map_err(CopyError::Target)?;
        total += n as u64;
    }
    let time = modified.unwrap_or_else(|| seconds(&meta));
    // A file system that can't keep the time only makes the next comparison ask for the bytes.
    let _ = dst.set_modified(UNIX_EPOCH + Duration::from_secs(time));
    Ok((total, seconds(&meta)))
}

/// Everything written to the file system of `path` is on the card.
fn flush(path: &Path) {
    if let Ok(f) = std::fs::File::open(path) {
        // SAFETY: the descriptor is open for the length of the call.
        #[cfg(not(target_os = "emscripten"))] unsafe { libc::syncfs(std::os::fd::AsRawFd::as_raw_fd(&f)) };
    }
}

/// Counts what was written and flushes `path`'s file system every `FLUSH_EVERY`.
#[derive(Default)]
struct Pace(u64);

impl Pace {
    fn wrote(&mut self, bytes: u64, path: &Path) {
        self.0 += bytes;
        if self.0 >= FLUSH_EVERY {
            flush(path);
            self.0 = 0;
        }
    }
}

/// Copies `items` from `storage` into a new dated folder under `root`. Whole or not at all: the folder gets its name when
/// everything is in it and flushed. A file that can't be read is left out and named in the list, so a dying card still
/// gives what it can; a place that can't be written ends it with nothing left behind.
pub fn create(storage: &Path, root: &Path, items: &[Item], reason: Reason, now: u64, app: &str, progress: &Progress) -> Result<SavesBackup, String> {
    if let Some((free, cluster)) = room(root) {
        let need = needed(items, cluster);
        if free < need {
            return Err(format!("Not enough space on the card: this needs {}, {} are free.", crate::storage::size(need), crate::storage::size(free)));
        }
    }
    // An older version's `.stignore` doesn't know the backup folder yet: told before the first copy, or Syncthing sends
    // these copies to every device (a file of the player's own is left alone).
    crate::sync::refresh_saves_ignore_in(&storage.join("roms"));
    // What a power cut left of an earlier one.
    for e in std::fs::read_dir(root).into_iter().flatten().flatten() {
        if e.file_name().to_string_lossy().starts_with(MAKING) {
            let _ = std::fs::remove_dir_all(e.path());
        }
    }
    let tmp = root.join(format!("{MAKING}{}", std::process::id()));
    std::fs::create_dir_all(&tmp).map_err(|e| format!("The backup couldn't be written ({e}). Nothing was changed."))?;
    progress.total.store(items.len(), Ordering::Relaxed);
    progress.done.store(0, Ordering::Relaxed);
    let made = (|| {
        let mut used = HashSet::new();
        let (mut held, mut unreadable) = (Vec::with_capacity(items.len()), Vec::new());
        let mut pace = Pace::default();
        for item in items {
            if progress.cancel.load(Ordering::Relaxed) {
                return Err("Stopped. No backup was made.".to_string());
            }
            let stored = stored_name(&item.path, &mut used);
            match copy_file(&storage.join(&item.path), &tmp.join(&stored), None) {
                Ok((bytes, modified)) => {
                    pace.wrote(bytes, &tmp);
                    held.push(Item { path: item.path.clone(), stored, bytes, modified });
                }
                Err(CopyError::Source(e)) => {
                    eprintln!("backup: {}: {e}", item.path);
                    unreadable.push(item.path.clone());
                }
                Err(CopyError::Target(e)) => return Err(format!("The backup couldn't be written ({e}). Nothing was changed.")),
            }
            progress.done.fetch_add(1, Ordering::Relaxed);
        }
        let manifest = Manifest { version: 1, created: now, reason, app: app.to_string(), items: held, unreadable };
        let json = serde_json::to_vec_pretty(&manifest).map_err(|e| format!("{MANIFEST}: {e}"))?;
        crate::fsutil::write_atomic(&tmp.join(MANIFEST), &json).map_err(|e| format!("The backup couldn't be written ({e}). Nothing was changed."))?;
        flush(&tmp);
        let dir = crate::backup::folder_name(root, now);
        std::fs::rename(&tmp, &dir).map_err(|e| format!("The backup couldn't be written ({e}). Nothing was changed."))?;
        flush(root);
        Ok(SavesBackup { dir, manifest })
    })();
    if made.is_err() {
        let _ = std::fs::remove_dir_all(&tmp);
    }
    made
}

/// The saves backups under `root`, newest first; a folder without a readable list is none.
pub fn list(root: &Path) -> Vec<SavesBackup> {
    let Ok(rd) = std::fs::read_dir(root) else { return Vec::new() };
    let mut out: Vec<SavesBackup> = rd
        .flatten()
        .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
        .filter_map(|e| Some(SavesBackup { manifest: serde_json::from_slice(&std::fs::read(e.path().join(MANIFEST)).ok()?).ok()?, dir: e.path() }))
        .collect();
    out.sort_by(|a, b| b.manifest.created.cmp(&a.manifest.created).then(b.dir.cmp(&a.dir)));
    out
}

/// The page's first step of a restore: every file of the backup against what is on the card now.
#[derive(Clone, Debug, PartialEq)]
pub struct RestorePlan {
    pub dir: PathBuf,
    /// Not on the card.
    pub new: Vec<Item>,
    /// Older on the card than in the backup: replaced, after a copy.
    pub replace: Vec<Item>,
    /// Newer on the card, or just as old and different: stays. The backup keeps its copy.
    pub keep: Vec<String>,
    pub same: usize,
    /// Listed, and missing from the folder, changed in size, or a place a backup never writes.
    pub damaged: usize,
    pub free: Option<u64>,
    pub needed: u64,
}

impl RestorePlan {
    pub fn fits(&self) -> bool {
        self.free.is_none_or(|f| f >= self.needed)
    }

    /// Nothing would change.
    pub fn nothing_to_do(&self) -> bool {
        self.new.is_empty() && self.replace.is_empty()
    }
}

fn same_bytes(a: &Path, b: &Path) -> bool {
    let (Ok(mut a), Ok(mut b)) = (std::fs::File::open(a), std::fs::File::open(b)) else { return false };
    let (mut x, mut y) = (vec![0u8; 64 * 1024], vec![0u8; 64 * 1024]);
    loop {
        let (Ok(n), true) = (a.read(&mut x), true) else { return false };
        if n == 0 {
            return b.read(&mut y).is_ok_and(|m| m == 0);
        }
        if b.read_exact(&mut y[..n]).is_err() || x[..n] != y[..n] {
            return false;
        }
    }
}

pub fn compare(backup: &Path, storage: &Path, root: &Path) -> Result<RestorePlan, String> {
    let manifest: Manifest = serde_json::from_slice(&std::fs::read(backup.join(MANIFEST)).map_err(|e| format!("{MANIFEST}: {e}"))?)
        .map_err(|e| format!("The backup's list can't be read ({e}). Nothing was changed."))?;
    let mut plan = RestorePlan { dir: backup.to_path_buf(), new: Vec::new(), replace: Vec::new(), keep: Vec::new(), same: 0, damaged: 0, free: None, needed: 0 };
    for item in manifest.items {
        let stored_ok = item.stored.starts_with("files/") && Path::new(&item.stored).components().all(|c| matches!(c, std::path::Component::Normal(_)));
        let held = backup.join(&item.stored);
        if !wanted(&item.path) || !stored_ok || !held.symlink_metadata().is_ok_and(|m| m.is_file() && m.len() == item.bytes) {
            plan.damaged += 1;
            continue;
        }
        let here = storage.join(&item.path);
        let Ok(m) = here.symlink_metadata() else {
            plan.new.push(item);
            continue;
        };
        let now = seconds(&m);
        if !m.is_file() {
            plan.keep.push(item.path);
        } else if m.len() == item.bytes && (now.abs_diff(item.modified) <= SAME_TIME || same_bytes(&here, &held)) {
            plan.same += 1;
        } else if now + SAME_TIME >= item.modified {
            plan.keep.push(item.path);
        } else {
            plan.replace.push(item);
        }
    }
    // The copies of what is replaced go to `root`, the new versions in place. Both on one card is the common case, counted so.
    if let Some((free, cluster)) = room(root) {
        plan.free = Some(free);
        plan.needed = needed(&plan.new, cluster) + 2 * needed(&plan.replace, cluster);
    }
    Ok(plan)
}

/// Whether the file at `to` is no older than the backup's copy now, by `compare`'s rule (or isn't a file any more): it is kept.
/// One that went away is no reason to keep anything.
fn newer_here(to: &Path, item: &Item) -> bool {
    to.symlink_metadata().is_ok_and(|m| !m.is_file() || seconds(&m) + SAME_TIME >= item.modified)
}

/// What a restore did.
#[derive(Clone, Debug, PartialEq)]
pub struct Restored {
    pub put: usize,
    pub replaced: usize,
    pub kept: usize,
    pub failed: Vec<String>,
    pub stopped: bool,
    /// Holds what was replaced.
    pub before: Option<SavesBackup>,
}

/// Puts a compared backup back. A file goes in under a temporary name and is renamed, so a power cut leaves the old or
/// the new one. A file that can't be written (a name exFAT refuses) is named in the result and the others go on.
pub fn restore(plan: &RestorePlan, storage: &Path, root: &Path, now: u64, app: &str, progress: &Progress) -> Result<Restored, String> {
    // The save may have changed since the comparison (Syncthing, another game): what is newer now stays.
    let mut replace: Vec<Item> = plan.replace.iter().filter(|i| !newer_here(&storage.join(&i.path), i)).cloned().collect();
    let before = if replace.is_empty() {
        None
    } else {
        let b = create(storage, root, &replace, Reason::BeforeRestore, now, app, progress)?;
        // What could not be copied away is not overwritten.
        replace.retain(|i| !b.manifest.unreadable.contains(&i.path));
        Some(b)
    };
    let kept = plan.keep.len() + plan.replace.len() - replace.len();
    let all: Vec<&Item> = plan.new.iter().chain(&replace).collect();
    progress.total.store(all.len(), Ordering::Relaxed);
    progress.done.store(0, Ordering::Relaxed);
    let (mut put, mut replaced, mut failed, mut stopped) = (0, 0, Vec::new(), false);
    let mut kept = kept;
    let mut pace = Pace::default();
    // Names this restore has put back: one that is "there already" because of them is a twin on a file system without case.
    let mut back: HashSet<String> = HashSet::new();
    for (i, item) in all.iter().enumerate() {
        if progress.cancel.load(Ordering::Relaxed) {
            stopped = true;
            break;
        }
        let to = storage.join(&item.path);
        let tmp = to.with_file_name(format!(".pocket-restore-{}", std::process::id()));
        let is_new = i < plan.new.len();
        // Compared as missing, and there now: a save that arrived (Syncthing) stays. On a file system without case it may be
        // this file's twin that just came back: that one is named, as this file didn't get back.
        if is_new && to.symlink_metadata().is_ok() {
            if back.contains(&item.path.to_lowercase()) {
                failed.push(item.path.clone());
            } else {
                kept += 1;
            }
            progress.done.fetch_add(1, Ordering::Relaxed);
            continue;
        }
        let result = match copy_file(&plan.dir.join(&item.stored), &tmp, Some(item.modified)) {
            Ok((bytes, _)) => {
                pace.wrote(bytes, to.parent().unwrap_or(storage));
                // Looked at again as late as it can be: the copy took time, and a save may have arrived during it.
                if !is_new && newer_here(&to, item) {
                    let _ = std::fs::remove_file(&tmp);
                    kept += 1;
                    progress.done.fetch_add(1, Ordering::Relaxed);
                    continue;
                }
                // A file compared as missing has no copy in Before restore: what arrived during the copy can't be replaced at all.
                if is_new {
                    crate::fsutil::rename_no_replace(&tmp, &to)
                } else {
                    std::fs::rename(&tmp, &to)
                }
            }
            Err(CopyError::Source(e)) | Err(CopyError::Target(e)) => Err(e),
        };
        match result {
            Ok(()) if is_new => {
                put += 1;
                back.insert(item.path.to_lowercase());
            }
            Ok(()) => replaced += 1,
            Err(e) if is_new && e.kind() == std::io::ErrorKind::AlreadyExists => {
                let _ = std::fs::remove_file(&tmp);
                if back.contains(&item.path.to_lowercase()) {
                    failed.push(item.path.clone());
                } else {
                    kept += 1;
                }
            }
            Err(e) => {
                let _ = std::fs::remove_file(&tmp);
                eprintln!("backup: {}: {e}", item.path);
                failed.push(item.path.clone());
            }
        }
        progress.done.fetch_add(1, Ordering::Relaxed);
    }
    // Saves can lie on two file systems (the Wii's, a second card's).
    crate::fsutil::flush_all();
    Ok(Restored { put, replaced, kept, failed, stopped, before })
}

/// How old a file on the card is made, for the tests.
#[cfg(test)]
fn set_age(path: &Path, unix: u64) {
    std::fs::File::options().write(true).open(path).unwrap().set_modified(UNIX_EPOCH + Duration::from_secs(unix)).unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;

    const WHEN: u64 = 1_792_744_800;

    fn put(root: &Path, rel: &str, text: &str, time: u64) {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, text).unwrap();
        set_age(&p, time);
    }

    /// `/storage` as on a device with some of everything, what must go and what must never.
    fn sample(storage: &Path) {
        for rel in SAVES {
            put(storage, rel, rel, WHEN);
        }
        for rel in NOT_SAVES {
            put(storage, rel, "not a save", WHEN);
        }
    }

    const SAVES: &[&str] = &[
        "roms/gba/Game.srm",
        "roms/gb/Sub/Other Game.sav",
        "roms/gb/Pokemon.rtc",
        "roms/nds/Game.dsv",
        "roms/psx/Game.pstate",
        "roms/snes/Game.pstate.auto",
        "roms/savestates/gba/Game.state1",
        "roms/savestates/gba/Game.pstate1",
        "roms/savestates/gba/Game.pstate.earlier-3",
        "roms/ps2/Mcd001.ps2",
        "roms/ps2/Mcd-Multitap1-1.ps2",
        "roms/bios/GC/MemoryCardA.USA.raw",
        "roms/bios/GC/USA/01-GZLE-save.gci",
        "roms/3ds/azahar/sdmc/Nintendo 3DS/a/b/title/00040000/0011/data/00000001.sav",
        "roms/3ds/azahar/sdmc/Nintendo 3DS/a/b/extdata/00000000/0022/x.bin",
        ".config/dolphin-emu/Wii/title/00010000/52534245/data/save.bin",
        ".config/dolphin-emu/Wii/title/00010004/52534245/data/banner.bin",
        ".local/share/scummvm/saves/monkey.s01",
        ".local/share/scummvm/saves/atlantis.012",
    ];
    const NOT_SAVES: &[&str] = &[
        "roms/gba/Game.gba",
        "roms/ps2/Game.iso",
        "roms/ps2/sub/Mcd001.ps2",
        "roms/bios/scph5501.bin",
        "roms/bios/GC/IPL.bin",
        "roms/bios/GC/USA/IPL.bin",
        "roms/bios/GC/dsp_rom.sav",
        "roms/backup/pocket-ui/2026/settings.srm",
        "roms/backup/pocket-ui-saves/2026/files/roms/gba/Game.srm",
        "roms/sync-backups/2026/gba/Game.srm",
        "roms/.hidden/Game.srm",
        "roms/3ds/azahar/sdmc/Nintendo 3DS/a/b/title/00040000/0011/content/game.app",
        "roms/3ds/azahar/old.sav",
        "roms/3ds/Game.3ds",
        ".config/dolphin-emu/Wii/title/00000001/00000002/data/system.sav",
        ".config/dolphin-emu/Wii/title/00010000/52534245/content/game.wad",
        ".config/pocket-ui/settings.sav",
        ".local/share/scummvm/scummvm.log",
    ];

    #[test]
    fn saves_and_states_go_in_and_games_and_bios_files_never() {
        let d = TestDir::new("saves-walk");
        sample(&d);
        let (items, skipped) = walk(&d);
        let mut got: Vec<&str> = items.iter().map(|i| i.path.as_str()).collect();
        let mut want = SAVES.to_vec();
        got.sort();
        want.sort();
        assert_eq!(got, want);
        assert_eq!(skipped, 0);
        assert!(!wanted("../x.srm") && !wanted("roms/../../x.srm") && !wanted("/roms/x.srm") && !wanted(""));
    }

    #[test]
    fn links_are_not_followed() {
        let d = TestDir::new("saves-links");
        put(&d, "elsewhere/Secret.srm", "x", WHEN);
        std::fs::create_dir_all(d.join("roms/gba")).unwrap();
        std::os::unix::fs::symlink(d.join("elsewhere"), d.join("roms/gba/link")).unwrap();
        std::os::unix::fs::symlink(d.join("elsewhere/Secret.srm"), d.join("roms/gba/Link.srm")).unwrap();
        assert!(walk(&d).0.is_empty());
    }

    #[test]
    fn a_plan_says_the_size_first_and_whether_it_fits() {
        let d = TestDir::new("saves-plan");
        sample(&d);
        let p = measure(&d, &d.join("roms/backup/pocket-ui-saves"), true);
        assert_eq!(p.items.len(), SAVES.len());
        assert_eq!(p.bytes, SAVES.iter().map(|s| s.len() as u64).sum::<u64>());
        assert!(p.needed > p.bytes && p.second_card);
        assert!(p.free.is_some());
        let mut full = p.clone();
        full.free = Some(p.needed - 1);
        assert!(!full.fits());
        full.free = Some(p.needed);
        assert!(full.fits());
    }

    #[test]
    fn a_backup_is_whole_keeps_times_and_comes_back_after_flashing() {
        let d = TestDir::new("saves-round");
        let (storage, root) = (d.join("a"), d.join("backups"));
        sample(&storage);
        let plan = measure(&storage, &root, false);
        let b = create(&storage, &root, &plan.items, Reason::Manual, WHEN, "1.2.3", &Progress::default()).unwrap();
        assert_eq!(list(&root), vec![b.clone()]);
        assert_eq!(b.bytes(), plan.bytes);
        assert_eq!(std::fs::read_to_string(b.dir.join("files/roms/gba/Game.srm")).unwrap(), "roms/gba/Game.srm");
        assert_eq!(seconds(&std::fs::metadata(b.dir.join("files/roms/gba/Game.srm")).unwrap()), WHEN);
        assert!(!std::fs::read_dir(&root).unwrap().flatten().any(|e| e.file_name().to_string_lossy().starts_with('.')));
        assert!(crate::backup::list(&root).is_empty(), "the settings page's list has no business with it");

        // A card flashed again: nothing but the backup, put back by way of a computer.
        let fresh = d.join("fresh");
        std::fs::create_dir_all(&fresh).unwrap();
        let plan = compare(&b.dir, &fresh, &root).unwrap();
        assert_eq!((plan.new.len(), plan.replace.len(), plan.keep.len(), plan.damaged), (SAVES.len(), 0, 0, 0));
        let done = restore(&plan, &fresh, &root, WHEN + 9, "1.2.3", &Progress::default()).unwrap();
        assert_eq!((done.put, done.replaced, done.kept, done.failed.len(), done.before), (SAVES.len(), 0, 0, 0, None));
        for rel in SAVES {
            assert_eq!(std::fs::read_to_string(fresh.join(rel)).unwrap(), *rel);
            assert_eq!(seconds(&std::fs::metadata(fresh.join(rel)).unwrap()), WHEN);
        }
        for rel in NOT_SAVES {
            assert!(!fresh.join(rel).exists(), "{rel}");
        }
        assert!(compare(&b.dir, &fresh, &root).unwrap().nothing_to_do(), "a second restore has nothing to do");
    }

    #[test]
    fn a_newer_save_is_never_overwritten_and_an_older_one_is_kept_in_a_backup() {
        let d = TestDir::new("saves-newer");
        let (storage, root) = (d.join("s"), d.join("backups"));
        for rel in ["roms/gba/Newer.srm", "roms/gba/Older.srm", "roms/gba/Same.srm", "roms/gba/Touched.srm", "roms/gba/Clash.srm", "roms/gba/Gone.srm"] {
            put(&storage, rel, "from the backup", WHEN);
        }
        let plan = measure(&storage, &root, false);
        let b = create(&storage, &root, &plan.items, Reason::Manual, WHEN, "1", &Progress::default()).unwrap();
        put(&storage, "roms/gba/Newer.srm", "played on", WHEN + 500);
        put(&storage, "roms/gba/Older.srm", "older one", WHEN - 500);
        put(&storage, "roms/gba/Touched.srm", "from the backup", WHEN + 500); // Same bytes, other time.
        put(&storage, "roms/gba/Clash.srm", "other bytes at the same time", WHEN + 1);
        std::fs::remove_file(storage.join("roms/gba/Gone.srm")).unwrap();

        let plan = compare(&b.dir, &storage, &root).unwrap();
        assert_eq!(plan.new.iter().map(|i| i.path.as_str()).collect::<Vec<_>>(), ["roms/gba/Gone.srm"]);
        assert_eq!(plan.replace.iter().map(|i| i.path.as_str()).collect::<Vec<_>>(), ["roms/gba/Older.srm"]);
        assert_eq!(plan.keep, ["roms/gba/Clash.srm", "roms/gba/Newer.srm"]);
        assert_eq!(plan.same, 2);

        let done = restore(&plan, &storage, &root, WHEN + 900, "1", &Progress::default()).unwrap();
        assert_eq!((done.put, done.replaced, done.kept), (1, 1, 2));
        let read = |rel: &str| std::fs::read_to_string(storage.join(rel)).unwrap();
        assert_eq!(read("roms/gba/Newer.srm"), "played on");
        assert_eq!(read("roms/gba/Clash.srm"), "other bytes at the same time");
        assert_eq!(read("roms/gba/Older.srm"), "from the backup");
        assert_eq!(read("roms/gba/Gone.srm"), "from the backup");
        // Both versions are still there: the kept one's in the backup, the replaced one in a backup made for it.
        assert_eq!(std::fs::read_to_string(b.dir.join("files/roms/gba/Newer.srm")).unwrap(), "from the backup");
        let before = done.before.unwrap();
        assert_eq!(before.manifest.reason, Reason::BeforeRestore);
        assert_eq!(std::fs::read_to_string(before.dir.join("files/roms/gba/Older.srm")).unwrap(), "older one");
        assert_eq!(before.manifest.items.len(), 1);
        assert_eq!(list(&root).len(), 2);
    }

    #[test]
    fn a_changed_backup_writes_nothing_outside_and_damage_is_counted() {
        let d = TestDir::new("saves-hostile");
        let (storage, root) = (d.join("s"), d.join("backups"));
        put(&storage, "roms/gba/Game.srm", "save", WHEN);
        put(&storage, "roms/gba/Two.srm", "save", WHEN);
        let plan = measure(&storage, &root, false);
        let b = create(&storage, &root, &plan.items, Reason::Manual, WHEN, "1", &Progress::default()).unwrap();
        let mut m = b.manifest.clone();
        let item = |path: &str, stored: &str| Item { path: path.into(), stored: stored.into(), bytes: 4, modified: WHEN };
        put(&b.dir, "files/x.bin", "evil", WHEN);
        m.items.push(item("../../evil.srm", "files/x.bin"));
        m.items.push(item("roms/gba/Game.gba", "files/x.bin"));
        m.items.push(item(".config/pocket-ui/settings.json", "files/x.bin"));
        m.items.push(item("roms/gba/Stolen.srm", "../../../secret"));
        std::fs::write(b.dir.join(MANIFEST), serde_json::to_vec(&m).unwrap()).unwrap();
        std::fs::remove_file(b.dir.join("files/roms/gba/Two.srm")).unwrap();
        let fresh = d.join("fresh");
        std::fs::create_dir_all(&fresh).unwrap();
        let plan = compare(&b.dir, &fresh, &root).unwrap();
        assert_eq!(plan.damaged, 5, "four from the edited list, one file gone");
        assert_eq!(plan.new.iter().map(|i| i.path.as_str()).collect::<Vec<_>>(), ["roms/gba/Game.srm"]);
        std::fs::write(b.dir.join(MANIFEST), "not json").unwrap();
        assert!(compare(&b.dir, &fresh, &root).is_err());
        assert!(list(&root).is_empty());
    }

    #[test]
    fn names_a_small_file_system_refuses_get_another_one_and_stay_apart() {
        let mut used = HashSet::new();
        assert_eq!(stored_name("roms/gba/Plain Name.srm", &mut used), "files/roms/gba/Plain Name.srm");
        assert_eq!(stored_name("roms/gba/Zelda: Oot?.srm", &mut used), "files/roms/gba/Zelda_ Oot_.srm");
        assert_eq!(stored_name("roms/gba/Zelda_ Oot_.srm", &mut used), "files/roms/gba/Zelda_ Oot_.srm~2");
        assert_eq!(stored_name("roms/gba/ZELDA_ OOT_.SRM", &mut used), "files/roms/gba/ZELDA_ OOT_.SRM~3");
        assert_eq!(stored_name("roms/odd. /x.sav", &mut used), "files/roms/odd/x.sav");
    }

    #[test]
    fn a_file_that_cant_be_read_is_named_and_the_rest_is_saved() {
        let d = TestDir::new("saves-unreadable");
        let (storage, root) = (d.join("s"), d.join("backups"));
        put(&storage, "roms/gba/Good.srm", "good", WHEN);
        let mut items = measure(&storage, &root, false).items;
        items.insert(0, Item { path: "roms/gba/Bad.srm".into(), stored: String::new(), bytes: 4, modified: WHEN });
        let b = create(&storage, &root, &items, Reason::Manual, WHEN, "1", &Progress::default()).unwrap();
        assert_eq!(b.manifest.unreadable, ["roms/gba/Bad.srm"]);
        assert_eq!(b.manifest.items.len(), 1);
    }

    #[test]
    fn a_place_that_cant_be_written_leaves_nothing_behind_and_stopping_too() {
        let d = TestDir::new("saves-nowrite");
        let storage = d.join("s");
        put(&storage, "roms/gba/Game.srm", "save", WHEN);
        let root = d.join("backups");
        let items = measure(&storage, &root, false).items;
        let stop = Progress::default();
        stop.cancel.store(true, Ordering::Relaxed);
        assert!(create(&storage, &root, &items, Reason::Manual, WHEN, "1", &stop).is_err());
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 0, "stopped: nothing left");
        // What a power cut left goes with the next one.
        std::fs::create_dir_all(root.join(format!("{MAKING}1/files"))).unwrap();
        create(&storage, &root, &items, Reason::Manual, WHEN, "1", &Progress::default()).unwrap();
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
        // The folder is a file: nothing can go there.
        let file = d.join("file");
        std::fs::write(&file, "x").unwrap();
        assert!(create(&storage, &file.join("backups"), &items, Reason::Manual, WHEN, "1", &Progress::default()).is_err());
    }

    #[test]
    fn a_file_that_appeared_since_the_comparison_is_not_overwritten() {
        let d = TestDir::new("saves-appeared");
        let (storage, root) = (d.join("s"), d.join("backups"));
        put(&storage, "roms/gba/A.srm", "backup", WHEN);
        let plan = measure(&storage, &root, false);
        let b = create(&storage, &root, &plan.items, Reason::Manual, WHEN, "1", &Progress::default()).unwrap();
        let fresh = d.join("fresh");
        std::fs::create_dir_all(&fresh).unwrap();
        let plan = compare(&b.dir, &fresh, &root).unwrap();
        put(&fresh, "roms/gba/A.srm", "new", WHEN + 9);
        let done = restore(&plan, &fresh, &root, WHEN, "1", &Progress::default()).unwrap();
        assert_eq!((done.put, done.kept, done.failed.len()), (0, 1, 0));
        assert_eq!(std::fs::read_to_string(fresh.join("roms/gba/A.srm")).unwrap(), "new");
    }

    /// A save arrives while the backup's copy of a file compared as missing is on its way: it stays, for real, with a large
    /// file so the copy takes time. The arrival can miss the window on a loaded machine, so it tries again; a restore that
    /// overwrites fails the content check whichever way the race went.
    #[test]
    fn a_save_that_arrives_during_the_copy_of_a_missing_file_stays() {
        let d = TestDir::new("saves-race");
        for attempt in 0..5 {
            let (backup, storage, root) = (d.join(format!("{attempt}/b")), d.join(format!("{attempt}/s")), d.join(format!("{attempt}/r")));
            let item = Item { path: "roms/gba/A.srm".into(), stored: "files/A.srm".into(), bytes: 64 << 20, modified: 1000 };
            std::fs::create_dir_all(backup.join("files")).unwrap();
            std::fs::write(backup.join("files/A.srm"), vec![b'B'; 64 << 20]).unwrap();
            set_age(&backup.join("files/A.srm"), 1000);
            let manifest = Manifest { version: 1, created: 1000, reason: Reason::Manual, app: "1".into(), items: vec![item], unreadable: Vec::new() };
            std::fs::write(backup.join(MANIFEST), serde_json::to_vec(&manifest).unwrap()).unwrap();
            std::fs::create_dir_all(&storage).unwrap();
            let plan = compare(&backup, &storage, &root).unwrap();
            assert_eq!(plan.new.len(), 1);
            let (s, r) = (storage.clone(), root.clone());
            let worker = std::thread::spawn(move || restore(&plan, &s, &r, 2000, "1", &Progress::default()).unwrap());
            let target = storage.join("roms/gba/A.srm");
            let tmp = target.with_file_name(format!(".pocket-restore-{}", std::process::id()));
            while !tmp.exists() && !worker.is_finished() {
                std::thread::yield_now();
            }
            put(&storage, "roms/gba/A.srm", "NEW PROGRESS", 3000);
            let done = worker.join().unwrap();
            assert_eq!(std::fs::read_to_string(&target).unwrap(), "NEW PROGRESS", "{done:?}");
            assert!(!tmp.exists(), "nothing left behind");
            if done.kept == 1 {
                assert_eq!((done.put, done.replaced, done.failed.len(), done.before.is_none()), (0, 0, 0, true), "{done:?}");
                return;
            }
        }
        panic!("the arrival never landed during the copy");
    }

    /// The commit itself, whichever way it is done: what is there stays, and the file on its way stays too until it is dealt with.
    #[test]
    fn the_commit_of_a_new_file_cannot_replace_anything() {
        let d = TestDir::new("saves-commit");
        std::fs::create_dir_all(&*d).unwrap();
        let (a, b, c) = (d.join("a"), d.join("b"), d.join("c"));
        std::fs::write(&a, "backup").unwrap();
        std::fs::write(&b, "arrived").unwrap();
        let e = crate::fsutil::rename_no_replace(&a, &b).unwrap_err();
        assert_eq!(e.kind(), std::io::ErrorKind::AlreadyExists);
        assert_eq!((std::fs::read_to_string(&a).unwrap().as_str(), std::fs::read_to_string(&b).unwrap().as_str()), ("backup", "arrived"));
        crate::fsutil::rename_no_replace(&a, &c).unwrap();
        assert!(!a.exists());
        assert_eq!(std::fs::read_to_string(&c).unwrap(), "backup");
    }

    /// Syncthing brings newer progress while the confirmation is open: the older backup doesn't replace it.
    #[test]
    fn a_save_that_got_newer_after_the_comparison_is_kept() {
        let d = TestDir::new("saves-late");
        let (storage, root) = (d.join("s"), d.join("backups"));
        put(&storage, "roms/gba/A.srm", "backup", WHEN);
        let items = measure(&storage, &root, false).items;
        let b = create(&storage, &root, &items, Reason::Manual, WHEN, "1", &Progress::default()).unwrap();
        put(&storage, "roms/gba/A.srm", "old", WHEN - 3600);
        let plan = compare(&b.dir, &storage, &root).unwrap();
        assert_eq!(plan.replace.len(), 1, "older at the comparison");
        put(&storage, "roms/gba/A.srm", "from the other handheld", WHEN + 3600);
        let done = restore(&plan, &storage, &root, WHEN, "1", &Progress::default()).unwrap();
        assert_eq!((done.replaced, done.kept), (0, 1));
        assert_eq!(std::fs::read_to_string(storage.join("roms/gba/A.srm")).unwrap(), "from the other handheld");
        assert!(done.before.is_none(), "nothing was replaced, so nothing is put aside");
        assert!(done.failed.is_empty());
    }

    /// A newer save caught only at the last look, after the copy of the backup's file: `newer_here` is that look.
    #[test]
    fn the_last_look_keeps_what_is_newer_or_a_folder_and_lets_a_gone_file_come_back() {
        let d = TestDir::new("saves-look");
        let item = Item { path: "A.srm".into(), bytes: 1, modified: WHEN, stored: "A.srm".into() };
        put(&d, "A.srm", "x", WHEN + 60);
        assert!(newer_here(&d.join("A.srm"), &item));
        put(&d, "A.srm", "x", WHEN - 60);
        assert!(!newer_here(&d.join("A.srm"), &item));
        assert!(!newer_here(&d.join("gone.srm"), &item));
        std::fs::create_dir_all(d.join("dir")).unwrap();
        assert!(newer_here(&d.join("dir"), &item));
    }

    /// After an update from a version whose `.stignore` lacks `/backup`: told before the first copy; the player's own stays.
    #[test]
    fn the_first_backup_after_an_update_stays_off_the_sync_peers() {
        let d = TestDir::new("saves-ignore");
        let (storage, root) = (d.join("s"), d.join("backups"));
        put(&storage, "roms/gba/A.srm", "a", WHEN);
        let ignore = storage.join("roms/.stignore");
        std::fs::write(&ignore, "// Written by pocket-ui\n!**/*.srm\n*\n").unwrap();
        let items = measure(&storage, &root, false).items;
        create(&storage, &root, &items, Reason::Manual, WHEN, "1", &Progress::default()).unwrap();
        assert!(std::fs::read_to_string(&ignore).unwrap().lines().any(|l| l == "/backup"));
        std::fs::write(&ignore, "mine\n").unwrap();
        create(&storage, &root, &items, Reason::Manual, WHEN + 1, "1", &Progress::default()).unwrap();
        assert_eq!(std::fs::read_to_string(&ignore).unwrap(), "mine\n");
    }

    #[test]
    fn stopping_a_restore_says_so_and_the_finished_files_stay() {
        let d = TestDir::new("saves-stop");
        let (storage, root) = (d.join("s"), d.join("backups"));
        put(&storage, "roms/gba/A.srm", "a", WHEN);
        let plan = measure(&storage, &root, false);
        let b = create(&storage, &root, &plan.items, Reason::Manual, WHEN, "1", &Progress::default()).unwrap();
        let fresh = d.join("fresh");
        std::fs::create_dir_all(&fresh).unwrap();
        let plan = compare(&b.dir, &fresh, &root).unwrap();
        let stop = Progress::default();
        stop.cancel.store(true, Ordering::Relaxed);
        let done = restore(&plan, &fresh, &root, WHEN, "1", &stop).unwrap();
        assert!(done.stopped && done.put == 0);
    }

    #[test]
    fn its_folder_is_counted_with_the_backups_but_never_on_the_clean_up_list() {
        assert_eq!(Path::new(ROOT).parent(), Path::new(crate::backup::ROOT).parent());
        assert_ne!(ROOT, crate::backup::ROOT);
        assert_ne!(MANIFEST, crate::backup::MANIFEST);
    }

    /// How long the steps take on a real card, with a card's worth of game names to walk past. `BACKUP_SAVES_TIMED` is an
    /// empty folder that may be written.
    #[test]
    #[ignore]
    fn timed_on_a_sample_tree() {
        let base = PathBuf::from(std::env::var("BACKUP_SAVES_TIMED").unwrap());
        let (storage, root) = (base.join("s"), base.join("b"));
        for system in 0..40 {
            let dir = storage.join(format!("roms/system{system}"));
            std::fs::create_dir_all(&dir).unwrap();
            for game in 0..500 {
                std::fs::write(dir.join(format!("Game {game}.rom")), "").unwrap();
            }
            for save in 0..10 {
                std::fs::write(dir.join(format!("Game {save}.srm")), vec![7u8; 1 << 20]).unwrap();
            }
        }
        let t = std::time::Instant::now();
        let plan = measure(&storage, &root, false);
        println!("{} files, {} MB: counted among 20000 names in {:?}", plan.items.len(), plan.bytes >> 20, t.elapsed());
        let t = std::time::Instant::now();
        let b = create(&storage, &root, &plan.items, Reason::Manual, WHEN, "1", &Progress::default()).unwrap();
        println!("backed up in {:?}", t.elapsed());
        let fresh = base.join("fresh");
        std::fs::create_dir_all(&fresh).unwrap();
        let t = std::time::Instant::now();
        let plan = compare(&b.dir, &fresh, &root).unwrap();
        println!("compared in {:?}", t.elapsed());
        let t = std::time::Instant::now();
        let done = restore(&plan, &fresh, &root, WHEN, "1", &Progress::default()).unwrap();
        println!("restored {} in {:?}", done.put, t.elapsed());
        assert_eq!(done.put, 400);
    }

    /// The same on a real exFAT file system. `BACKUP_SAVES_EXFAT` is a mounted one; see docs/SYSTEM-PAGES.md, "Backup & reset".
    #[test]
    #[ignore]
    fn on_exfat() {
        let d = TestDir::new("saves-exfat");
        let exfat = PathBuf::from(std::env::var("BACKUP_SAVES_EXFAT").unwrap());
        let (storage, root) = (d.join("s"), exfat.join("roms/backup/pocket-ui-saves"));
        put(&storage, "roms/gba/Zelda: Oot?.srm", "colon", WHEN);
        put(&storage, "roms/gba/Plain.srm", "plain", WHEN);
        put(&storage, "roms/gba/PLAIN.srm", "case apart", WHEN);
        let plan = measure(&storage, &root, true);
        assert!(plan.fits());
        let b = create(&storage, &root, &plan.items, Reason::Manual, WHEN, "1", &Progress::default()).unwrap();
        assert_eq!(b.manifest.items.len(), 3, "{:?}", b.manifest);
        // Saves on the same card as the backup, as with a games card. A name exFAT refuses, and the second of two names it
        // takes for one, come back named; the first of the two is not overwritten by the second.
        let on_card = exfat.join("card");
        let plan = compare(&b.dir, &on_card, &root).unwrap();
        assert_eq!(plan.new.len(), 3);
        let done = restore(&plan, &on_card, &root, WHEN, "1", &Progress::default()).unwrap();
        assert_eq!(done.failed, ["roms/gba/Plain.srm", "roms/gba/Zelda: Oot?.srm"], "{done:?}");
        assert_eq!(done.put, 1, "{done:?}");
        assert_eq!(std::fs::read_to_string(on_card.join("roms/gba/PLAIN.srm")).unwrap(), "case apart");
        // Times survive within the rule.
        let t = seconds(&std::fs::metadata(on_card.join("roms/gba/PLAIN.srm")).unwrap());
        assert!(t.abs_diff(WHEN) <= SAME_TIME, "{t}");
        let again = compare(&b.dir, &on_card, &root).unwrap();
        assert_eq!(again.same, 1, "{again:?}");
        assert!(again.replace.is_empty(), "{again:?}");
    }

    #[test]
    fn scummvm_saves_go_in_with_any_ending_and_nothing_beside_them() {
        for name in ["monkey.s01", "atlantis.012", "no-ending"] {
            assert!(wanted(&format!(".local/share/scummvm/saves/{name}")), "{name}");
        }
        assert!(!wanted(".local/share/scummvm/saves") && !wanted(".local/share/scummvm/savesx/monkey.s01") && !wanted(".local/share/scummvm/monkey.s01"));
    }
}
