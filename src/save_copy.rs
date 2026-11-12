//! Copies of the app's saves in a folder that outlives the app (Android; docs/user/android-mvp.md, "Saves copied").
//!
//! The saves live inside the app (`saves/<key>/`, `states/<key>/`) and Android removes them with it. After a game ends
//! (Save & quit, or the game closing) a copy of that game's files goes into `pocket-ui saves/` inside the folder the
//! player chose for their games, one folder per game, and after a new install the copies are found again. Android's
//! side only moves bytes (`SaveCopy.java`, a [`Store`]); every decision is here, so the gate tests it.
//!
//! - **Never while playing.** The files are copied after the game is over, in the background (the player leaves a note,
//!   [`mark_pending`]); the launcher does the rest.
//! - **Nothing newer is replaced by anything older.** Each game's copy carries a list (`copy.json`) of its files with
//!   time and SHA-256. A copy, a restore and the phone's own files are compared three ways with what this phone
//!   copied last time (the *base*, `copy-state.json`): a side that moved on alone wins; when both moved, or the base is
//!   unknown and the two differ, it is a conflict: nothing is overwritten and the game is named (`Report::conflicts`).
//!   A file is replaced only when it is different and its time is later than the one it replaces.
//! - **A half copy is never taken for a whole one.** Files go first, the list last; a restore checks every file against
//!   the list's size and hash and moves a game's files in only when all of them are right.
//! - **Nothing fails silently.** No access, a folder that is gone or a failing write end in a [`Trouble`] the page says.

use crate::save_transfer::{files_under, is_key, is_scrap, written_ms, Known, FOLDERS};
use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

/// The visible folder inside the player's chosen folder.
pub const FOLDER: &str = "pocket-ui saves";
/// A game's list of files, inside its folder.
pub const MANIFEST: &str = "copy.json";
/// What this phone copied last time, under the app's data.
pub const INDEX: &str = "copy-state.json";
/// Notes left by the player: one empty file per game that ended, named by its key.
pub const PENDING: &str = "copy-pending";
const FORMAT: u32 = 1;
const MAX_DEPTH: usize = 8;

pub const NO_ACCESS: &str = "pocket-ui may not write into your folder. Choose the folder again and allow changes to copy your saves.";
pub const GONE: &str = "The folder for your saves' copies can't be reached. Is the card in, and the folder still there?";
pub const FAILED: &str = "Your saves could not be copied to your folder.";

/// Why copying or restoring did not work.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Trouble {
    /// No permission to write (or read) the folder.
    NoAccess,
    /// The folder or the card is gone.
    Gone,
    Failed(String),
}

impl Trouble {
    /// A sentence for the player (English; the page translates it).
    pub fn sentence(&self) -> &'static str {
        match self {
            Trouble::NoAccess => NO_ACCESS,
            Trouble::Gone => GONE,
            Trouble::Failed(_) => FAILED,
        }
    }
}

/// The folder with the copies, as Android gives it (`SaveCopy.java`); in the tests a plain directory.
pub trait Store {
    /// The names of the folders inside `pocket-ui saves` (empty when there is none yet).
    fn games(&self) -> Result<Vec<String>, Trouble>;
    /// A game's list, or None when there is none.
    fn manifest(&self, dir: &str) -> Result<Option<String>, Trouble>;
    /// Writes the bytes of `from` as `rel` (`saves/x.srm`) in the game's folder, replacing a file of that name.
    fn put(&self, dir: &str, rel: &str, from: &Path) -> Result<(), Trouble>;
    /// Writes the game's list (last).
    fn put_manifest(&self, dir: &str, text: &str) -> Result<(), Trouble>;
    /// Reads `rel` of the game into the file `to`.
    fn get(&self, dir: &str, rel: &str, to: &Path) -> Result<(), Trouble>;
}

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct FileInfo {
    /// Time of writing in Unix milliseconds.
    pub ms: u64,
    pub size: u64,
    pub sha: String,
}

/// A game's list in its folder.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct CopyManifest {
    pub format: u32,
    pub app: String,
    /// Unix seconds.
    pub made: u64,
    pub key: String,
    pub system: String,
    pub file: String,
    pub title: String,
    pub core: String,
    /// `saves/<name>` and `states/<name>` (the part below the game's key).
    pub files: BTreeMap<String, FileInfo>,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Synced {
    pub dir: String,
    pub files: BTreeMap<String, FileInfo>,
}

/// What this phone knows of its copies: per game what the folder held when both last agreed, and the games that differ.
#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Index {
    pub games: BTreeMap<String, Synced>,
    /// Key → name of the game, for the games whose phone files and copy differ.
    pub conflicts: BTreeMap<String, String>,
    /// Unix seconds of the last copy that went through.
    pub last: u64,
}

impl Index {
    pub fn load(data: &Path) -> Index {
        std::fs::read(data.join(INDEX)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
    }
    pub fn save(&self, data: &Path) -> std::io::Result<()> {
        crate::fsutil::write_atomic(&data.join(INDEX), &serde_json::to_vec_pretty(self).unwrap_or_default())
    }
}

/// What a run did. `log` lines are for logcat: every conflict and refused overwrite is among them.
#[derive(Debug, Default)]
pub struct Report {
    pub copied_games: usize,
    pub copied_files: usize,
    pub bytes: u64,
    pub restored_games: usize,
    /// Games whose phone files and copy differ: named, nothing overwritten.
    pub conflicts: Vec<String>,
    /// Games whose copy is newer than the phone's files (another phone wrote it); nothing was overwritten.
    pub copy_newer: Vec<String>,
    /// Copies in the folder that no game here fits, or that could not be read or are incomplete.
    pub unmatched: usize,
    pub unreadable: usize,
    pub failed: usize,
    pub trouble: Option<Trouble>,
    pub log: Vec<String>,
}

impl Report {
    /// Adds what `other` did.
    pub fn merge(&mut self, other: Report) {
        self.copied_games += other.copied_games;
        self.copied_files += other.copied_files;
        self.bytes += other.bytes;
        self.restored_games += other.restored_games;
        for name in other.conflicts {
            if !self.conflicts.contains(&name) {
                self.conflicts.push(name);
            }
        }
        self.copy_newer.extend(other.copy_newer);
        self.unmatched += other.unmatched;
        self.unreadable += other.unreadable;
        self.failed += other.failed;
        self.trouble = self.trouble.take().or(other.trouble);
        self.log.extend(other.log);
    }
    /// What to say to the player when a run ends; None when there is nothing to say (everything was already in place).
    pub fn sentence(&self) -> Option<String> {
        if let Some(trouble) = &self.trouble {
            return Some(trouble.sentence().to_string());
        }
        let mut lines = Vec::new();
        match self.restored_games {
            0 => {}
            1 => lines.push("1 game has its saves back from your folder.".to_string()),
            n => lines.push(format!("{n} games have their saves back from your folder.")),
        }
        match self.copied_games {
            0 => {}
            1 => lines.push("Copied the saves of 1 game to your folder.".to_string()),
            n => lines.push(format!("Copied the saves of {n} games to your folder.")),
        }
        match self.conflicts.len() {
            0 => {}
            1 => lines.push("1 game's saves differ from the copy in your folder. Nothing was overwritten.".to_string()),
            n => lines.push(format!("{n} games' saves differ from the copies in your folder. Nothing was overwritten.")),
        }
        if self.failed > 0 {
            lines.push(FAILED.to_string());
        }
        (!lines.is_empty()).then(|| lines.join("\n"))
    }
}

// ---- Names ---------------------------------------------------------------------------------------------------------

/// The game's folder: its name for people, and its key to find it again.
pub fn dir_name(key: &str, title: &str) -> String {
    let clean: String = title.chars().map(|c| if c.is_control() || "/\\:*?\"<>|[]".contains(c) { '_' } else { c }).collect();
    let clean: String = clean.trim().trim_matches('.').chars().take(60).collect();
    let clean = clean.trim();
    format!("{} [{key}]", if clean.is_empty() { "Game" } else { clean })
}

/// The key a folder's name ends with.
pub fn key_of_dir(dir: &str) -> Option<&str> {
    let inner = dir.strip_suffix(']')?;
    let key = inner.get(inner.len().checked_sub(16)?..)?;
    (is_key(key) && inner[..inner.len() - 16].ends_with(" [")).then_some(key)
}

/// `saves/a.srm`: plain names, one of the two folders, nothing outside.
fn valid_rel(rel: &str) -> bool {
    if rel.len() > 512 || rel.contains(['\\', '\0']) {
        return false;
    }
    let mut parts = rel.split('/');
    let first = parts.next().unwrap_or("");
    let rest: Vec<&str> = parts.collect();
    FOLDERS.contains(&first) && !rest.is_empty() && rest.len() < MAX_DEPTH && rest.iter().all(|p| !p.is_empty() && *p != "." && *p != ".." && !is_scrap(p))
}

// ---- The phone's files ---------------------------------------------------------------------------------------------

fn sha256(path: &Path) -> std::io::Result<String> {
    use sha2::{Digest, Sha256};
    let mut file = std::fs::File::open(path)?;
    let mut hash = Sha256::new();
    let mut buffer = vec![0u8; 1 << 16];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    Ok(hash.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// A game's files here by the names the copy uses (`saves/x`), with their time, size and path.
fn local_files(data: &Path, key: &str) -> BTreeMap<String, (FileInfo, PathBuf)> {
    let mut found = BTreeMap::new();
    for folder in FOLDERS {
        let mut all = Vec::new();
        files_under(&data.join(folder).join(key), &mut Vec::new(), &mut all);
        for (parts, path) in all {
            let size = std::fs::metadata(&path).map_or(0, |m| m.len());
            found.insert(format!("{folder}/{}", parts.join("/")), (FileInfo { ms: written_ms(&path), size, sha: String::new() }, path));
        }
    }
    found
}

/// [`local_files`] with the hashes: a file whose time and size are those of the base keeps the base's hash unread.
fn local_infos(data: &Path, key: &str, base: Option<&BTreeMap<String, FileInfo>>) -> BTreeMap<String, (FileInfo, PathBuf)> {
    let mut files = local_files(data, key);
    files.retain(|rel, (info, path)| {
        if let Some(same) = base.and_then(|b| b.get(rel)).filter(|f| f.ms == info.ms && f.size == info.size) {
            info.sha = same.sha.clone();
            return true;
        }
        match sha256(path) {
            Ok(sha) => {
                info.sha = sha;
                true
            }
            Err(_) => false, // vanished or unreadable meanwhile: not part of this copy
        }
    });
    files
}

/// Which games have files here.
pub fn keys_with_files(data: &Path) -> BTreeSet<String> {
    let mut keys = BTreeSet::new();
    for folder in FOLDERS {
        let Ok(read) = std::fs::read_dir(data.join(folder)) else { continue };
        for entry in read.flatten() {
            let Some(key) = entry.file_name().to_str().map(str::to_string) else { continue };
            if is_key(&key) && !local_files(data, &key).is_empty() {
                keys.insert(key);
            }
        }
    }
    keys
}

/// Removes a folder and what is in it; a folder that is not there is fine.
fn remove_tree(path: &Path) -> std::io::Result<()> {
    match std::fs::remove_dir_all(path) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

/// Leaves the note that a game has ended and its files are whole (the player; also when the folder is not there). A note
/// that can't be written is not lost for good: the start looks at the files' times as well (`stale`).
pub fn mark_pending(data: &Path, key: &str) -> std::io::Result<()> {
    if !is_key(key) {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "not a game's key"));
    }
    let dir = data.join(PENDING);
    std::fs::create_dir_all(&dir)?;
    std::fs::write(dir.join(key), b"")
}

/// The games with a note.
pub fn pending(data: &Path) -> Vec<String> {
    let Ok(read) = std::fs::read_dir(data.join(PENDING)) else { return Vec::new() };
    let mut keys: Vec<String> = read.flatten().filter_map(|e| e.file_name().to_str().map(str::to_string)).filter(|k| is_key(k)).collect();
    keys.sort();
    keys
}

/// Takes the notes away once their games are copied (a note that arrived meanwhile stays: it is rewritten).
pub fn clear_pending(data: &Path, keys: &[String]) -> std::io::Result<()> {
    for key in keys.iter().filter(|k| is_key(k)) {
        match std::fs::remove_file(data.join(PENDING).join(key)) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e),
            _ => {}
        }
    }
    Ok(())
}

/// Games whose files here are not what this phone last copied (or never were): the start's look, from the files' times
/// alone, with no access to the folder.
pub fn stale(data: &Path) -> Vec<String> {
    let index = Index::load(data);
    keys_with_files(data)
        .into_iter()
        .filter(|key| {
            let local = local_files(data, key);
            match index.games.get(key) {
                None => true,
                Some(synced) => local.iter().any(|(rel, (info, _))| synced.files.get(rel).is_none_or(|f| f.ms != info.ms || f.size != info.size)),
            }
        })
        .collect()
}

// ---- Copying -------------------------------------------------------------------------------------------------------

#[derive(Debug, PartialEq, Eq)]
enum Outcome {
    Nothing,
    InSync,
    Copied,
    Conflict,
    CopyNewer,
}

fn same_sha(a: &BTreeMap<String, FileInfo>, b: &BTreeMap<String, FileInfo>) -> bool {
    a.len() == b.len() && a.iter().all(|(rel, f)| b.get(rel).is_some_and(|g| g.sha == f.sha))
}

fn parse_manifest(text: &str) -> Option<CopyManifest> {
    let manifest: CopyManifest = serde_json::from_str(text).ok()?;
    (manifest.app == "pocket-ui" && manifest.format >= 1 && manifest.format <= FORMAT).then_some(manifest)
}

fn name_of(known: Option<&Known>, key: &str) -> String {
    known.map_or_else(|| key.to_string(), |k| if k.title.is_empty() { k.file.clone() } else { k.title.clone() })
}

#[allow(clippy::too_many_arguments)]
fn copy_game(
    data: &Path,
    index: &mut Index,
    key: &str,
    known: Option<&Known>,
    listing: &[String],
    store: &dyn Store,
    now: u64,
    report: &mut Report,
) -> Result<Outcome, Trouble> {
    let base = index.games.get(key).map(|s| s.files.clone());
    let local = local_infos(data, key, base.as_ref());
    if local.is_empty() {
        return Ok(Outcome::Nothing);
    }
    let name = name_of(known, key);
    let dir = listing.iter().find(|d| key_of_dir(d) == Some(key)).cloned().unwrap_or_else(|| dir_name(key, &name));
    let remote: Option<CopyManifest> = if listing.contains(&dir) {
        match store.manifest(&dir)? {
            Some(text) => parse_manifest(&text).or_else(|| {
                report.unreadable += 1;
                report.log.push(format!("save copy: the list of {dir} can't be read; it is written again"));
                None
            }),
            None => None,
        }
    } else {
        None
    };
    let local_sha: BTreeMap<String, FileInfo> = local.iter().map(|(rel, (info, _))| (rel.clone(), info.clone())).collect();
    let mut manifest = remote.clone().unwrap_or_default();
    let mut to_put: Vec<&String> = local.keys().collect();
    if let Some(remote) = &remote {
        let equal = local_sha.iter().all(|(rel, f)| remote.files.get(rel).is_some_and(|r| r.sha == f.sha));
        if equal {
            index.games.insert(key.to_string(), Synced { dir, files: remote.files.clone() });
            index.conflicts.remove(key);
            return Ok(Outcome::InSync);
        }
        let remote_unchanged = base.as_ref().is_some_and(|b| same_sha(b, &remote.files));
        let local_unchanged = base.as_ref().is_some_and(|b| local_sha.iter().all(|(rel, f)| b.get(rel).is_some_and(|x| x.sha == f.sha)));
        if !remote_unchanged {
            if local_unchanged {
                report.log.push(format!("save copy: the copy of {name} ({dir}) is newer than this phone's files; nothing overwritten"));
                return Ok(Outcome::CopyNewer);
            }
            report.log.push(format!("save copy: conflict for {name} ({dir}): phone and copy both differ from the last agreed state; nothing overwritten"));
            index.conflicts.insert(key.to_string(), name);
            return Ok(Outcome::Conflict);
        }
        // The copy is as this phone left it: what changed here goes over, but never a file older than the one it replaces.
        to_put.retain(|rel| remote.files.get(*rel).is_none_or(|r| r.sha != local[*rel].0.sha));
        if let Some(rel) = to_put.iter().find(|rel| remote.files.get(**rel).is_some_and(|r| local[**rel].0.ms <= r.ms)) {
            report.log.push(format!(
                "save copy: conflict for {name} ({dir}): {rel} on the phone is not newer than the copy's; nothing overwritten"
            ));
            index.conflicts.insert(key.to_string(), name);
            return Ok(Outcome::Conflict);
        }
    }
    // Files first, the list last.
    let mut first_trouble: Option<Trouble> = None;
    let mut put = 0usize;
    for rel in to_put {
        let (info, path) = &local[rel];
        match store.put(&dir, rel, path) {
            Ok(()) => {
                // A file that changed while it was copied is not the one the list would describe: next time.
                if written_ms(path) != info.ms || std::fs::metadata(path).map_or(0, |m| m.len()) != info.size {
                    report.log.push(format!("save copy: {rel} changed while it was copied; it follows next time"));
                    report.failed += 1;
                    continue;
                }
                manifest.files.insert(rel.clone(), info.clone());
                put += 1;
                report.copied_files += 1;
                report.bytes += info.size;
            }
            Err(trouble) => {
                report.log.push(format!("save copy: {rel} of {name} not written: {trouble:?}"));
                first_trouble = Some(trouble);
                break;
            }
        }
    }
    if put > 0 {
        manifest.format = FORMAT;
        manifest.app = "pocket-ui".into();
        manifest.made = now;
        manifest.key = key.to_string();
        if let Some(k) = known {
            manifest.system = k.system.clone();
            manifest.file = k.file.clone();
            manifest.title = k.title.clone();
            manifest.core = k.core.clone();
        }
        let text = serde_json::to_string_pretty(&manifest).unwrap_or_default();
        match store.put_manifest(&dir, &text) {
            Ok(()) => {
                index.games.insert(key.to_string(), Synced { dir, files: manifest.files.clone() });
                index.conflicts.remove(key);
                report.copied_games += 1;
            }
            Err(trouble) => return Err(first_trouble.unwrap_or(trouble)),
        }
    }
    match first_trouble {
        Some(trouble) => Err(trouble),
        None if put == 0 => Err(Trouble::Failed("changed while it was copied".into())),
        None => Ok(Outcome::Copied),
    }
}

/// Copies the games in `keys` (all that have files when None). `now` is Unix seconds. A trouble with the folder itself
/// ends the run (`Report::trouble`); one game's failed write only counts.
pub fn copy_games(data: &Path, known: &[Known], keys: Option<&[String]>, store: &dyn Store, now: u64) -> Report {
    let mut report = Report::default();
    let mut index = Index::load(data);
    let listing = match store.games() {
        Ok(listing) => listing,
        Err(trouble) => {
            report.trouble = Some(trouble);
            return report;
        }
    };
    let keys: Vec<String> = keys.map_or_else(|| keys_with_files(data).into_iter().collect(), <[String]>::to_vec);
    for key in keys.iter().filter(|k| is_key(k)) {
        let entry = known.iter().find(|k| &k.key == key);
        match copy_game(data, &mut index, key, entry, &listing, store, now, &mut report) {
            Ok(Outcome::Conflict) => report.conflicts.push(name_of(entry, key)),
            Ok(Outcome::CopyNewer) => report.copy_newer.push(name_of(entry, key)),
            Ok(_) => {}
            Err(Trouble::Failed(why)) => {
                report.log.push(format!("save copy: {key}: {why}"));
                report.failed += 1;
            }
            Err(trouble) => {
                report.trouble = Some(trouble);
                break;
            }
        }
    }
    if report.trouble.is_none() && report.failed == 0 {
        index.last = now;
    }
    if let Err(e) = index.save(data) {
        report.log.push(format!("save copy: the state could not be kept: {e}"));
    }
    report
}

// ---- Restoring -----------------------------------------------------------------------------------------------------

/// The key a copy's game has here: its own when the library has it, else the one game of the same console, file and core.
fn target_key(manifest: &CopyManifest, known: &[Known], claimed: &BTreeSet<String>) -> Option<String> {
    if known.iter().any(|k| k.key == manifest.key) {
        return Some(manifest.key.clone());
    }
    if manifest.system.is_empty() || manifest.file.is_empty() {
        return None;
    }
    let fits: Vec<&Known> = known.iter().filter(|k| k.system == manifest.system && k.file == manifest.file && k.core == manifest.core).collect();
    match fits.as_slice() {
        [one] if !claimed.contains(&one.key) => Some(one.key.clone()),
        _ => None,
    }
}

fn bring_back(data: &Path, store: &dyn Store, dir: &str, target: &str, manifest: &CopyManifest) -> Result<(), Trouble> {
    let part = data.join("copy-restore.part");
    remove_tree(&part).map_err(|e| Trouble::Failed(e.to_string()))?;
    let result = (|| {
        for (rel, info) in &manifest.files {
            let to = part.join(rel);
            std::fs::create_dir_all(to.parent().unwrap_or(&part)).map_err(|e| Trouble::Failed(e.to_string()))?;
            store.get(dir, rel, &to)?;
            let size = std::fs::metadata(&to).map_or(u64::MAX, |m| m.len());
            if size != info.size || sha256(&to).ok().as_deref() != Some(info.sha.as_str()) {
                return Err(Trouble::Failed(format!("{rel} of the copy is not whole")));
            }
            if let Ok(file) = std::fs::File::options().write(true).open(&to) {
                let _ = file.set_modified(UNIX_EPOCH + std::time::Duration::from_millis(info.ms));
            }
        }
        for folder in FOLDERS {
            let from = part.join(folder);
            if !from.is_dir() {
                continue;
            }
            let to = data.join(folder).join(target);
            // Only an empty folder is here (a start makes them): no file of the phone's is replaced.
            if local_files(data, target).keys().any(|rel| rel.starts_with(&format!("{folder}/"))) {
                return Err(Trouble::Failed("the phone has files for this game now".into()));
            }
            remove_tree(&to).map_err(|e| Trouble::Failed(e.to_string()))?;
            std::fs::create_dir_all(to.parent().unwrap_or(data)).map_err(|e| Trouble::Failed(e.to_string()))?;
            std::fs::rename(&from, &to).map_err(|e| Trouble::Failed(e.to_string()))?;
        }
        Ok(())
    })();
    // What is left of a refused game is removed at the next start of a restore too.
    if let Err(e) = remove_tree(&part) {
        eprintln!("save copy: {} not removed: {e}", part.display());
    }
    result
}

/// Looks for copies in the folder and brings back the games that have no saves here. A game that has some is left
/// alone: the same files are in sync, different ones are a conflict. `now` is Unix seconds.
pub fn restore(data: &Path, known: &[Known], store: &dyn Store, now: u64) -> Report {
    let mut report = Report::default();
    let mut index = Index::load(data);
    let listing = match store.games() {
        Ok(listing) => listing,
        Err(trouble) => {
            report.trouble = Some(trouble);
            return report;
        }
    };
    let mut claimed = BTreeSet::new();
    for dir in &listing {
        // A game this phone has played and copied before has its copy pass: no need to read its list at every start.
        if let Some(key) = key_of_dir(dir) {
            if index.games.get(key).is_some_and(|s| &s.dir == dir) && known.iter().any(|k| k.key == key) && !local_files(data, key).is_empty() {
                claimed.insert(key.to_string());
                continue;
            }
        }
        let text = match store.manifest(dir) {
            Ok(Some(text)) => text,
            Ok(None) => continue, // not one of ours
            Err(Trouble::Failed(why)) => {
                report.log.push(format!("save copy: {dir}: list not read: {why}"));
                report.unreadable += 1;
                continue;
            }
            Err(trouble) => {
                report.trouble = Some(trouble);
                break;
            }
        };
        let Some(manifest) = parse_manifest(&text).filter(|m| is_key(&m.key)) else {
            report.log.push(format!("save copy: the list of {dir} is not readable or is from a newer pocket-ui"));
            report.unreadable += 1;
            continue;
        };
        if let Some(rel) = manifest.files.keys().find(|rel| !valid_rel(rel)) {
            report.log.push(format!("save copy: {dir} names {rel}, which is no save file; skipped"));
            report.unreadable += 1;
            continue;
        }
        let Some(target) = target_key(&manifest, known, &claimed) else {
            report.unmatched += 1;
            continue;
        };
        claimed.insert(target.clone());
        let entry = known.iter().find(|k| k.key == target);
        let name = name_of(entry, &target);
        let local = local_infos(data, &target, index.games.get(&target).map(|s| &s.files));
        let local_sha: BTreeMap<String, FileInfo> = local.iter().map(|(rel, (info, _))| (rel.clone(), info.clone())).collect();
        if local.is_empty() {
            match bring_back(data, store, dir, &target, &manifest) {
                Ok(()) => {
                    index.games.insert(target.clone(), Synced { dir: dir.clone(), files: manifest.files.clone() });
                    index.conflicts.remove(&target);
                    report.restored_games += 1;
                    report.log.push(format!("save copy: {name} has its saves back from {dir}"));
                }
                Err(Trouble::Failed(why)) => {
                    report.log.push(format!("save copy: {name} not restored from {dir}: {why}"));
                    report.failed += 1;
                }
                Err(trouble) => {
                    report.trouble = Some(trouble);
                    break;
                }
            }
        } else if same_sha(&local_sha, &manifest.files) {
            index.games.insert(target.clone(), Synced { dir: dir.clone(), files: manifest.files.clone() });
            index.conflicts.remove(&target);
        } else {
            let base = index.games.get(&target).map(|s| &s.files);
            let phone_unchanged = base.is_some_and(|b| local_sha.iter().all(|(rel, f)| b.get(rel).is_some_and(|x| x.sha == f.sha)));
            if phone_unchanged && !base.is_some_and(|b| same_sha(b, &manifest.files)) {
                report.log.push(format!("save copy: the copy of {name} ({dir}) is newer than this phone's files; nothing overwritten"));
                report.copy_newer.push(name);
            } else {
                report.log.push(format!("save copy: conflict for {name} ({dir}): the phone has saves that differ from the copy; nothing overwritten"));
                index.conflicts.insert(target.clone(), name.clone());
                report.conflicts.push(name);
            }
        }
    }
    if report.trouble.is_none() {
        index.last = now.max(index.last);
    }
    if let Err(e) = index.save(data) {
        report.log.push(format!("save copy: the state could not be kept: {e}"));
    }
    report
}

/// For the Settings page: what the copies' state is in words (the page translates them).
pub fn state_words(data: &Path) -> Option<String> {
    let index = Index::load(data);
    match index.conflicts.len() {
        0 => None,
        1 => Some("1 game's saves differ from the copy in your folder. Nothing was overwritten.".to_string()),
        n => Some(format!("{n} games' saves differ from the copies in your folder. Nothing was overwritten.")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};

    const ZELDA: &str = "00000000000000aa";
    const MARIO: &str = "00000000000000bb";

    /// A folder as Android shows it: plain directories under a root, one folder per game.
    struct Dir {
        root: PathBuf,
        /// Reads that fail, to see what is said.
        trouble: RefCell<Option<Trouble>>,
        puts: Cell<usize>,
        /// Stop (as a killed app would) after this many files.
        die_after: Cell<Option<usize>>,
    }
    impl Dir {
        fn new(root: &Path) -> Dir {
            Dir { root: root.join(FOLDER), trouble: RefCell::new(None), puts: Cell::new(0), die_after: Cell::new(None) }
        }
        fn check(&self) -> Result<(), Trouble> {
            self.trouble.borrow().clone().map_or(Ok(()), Err)
        }
    }
    impl Store for Dir {
        fn games(&self) -> Result<Vec<String>, Trouble> {
            self.check()?;
            let Ok(read) = std::fs::read_dir(&self.root) else { return Ok(Vec::new()) };
            Ok(read.flatten().filter(|e| e.path().is_dir()).filter_map(|e| e.file_name().to_str().map(str::to_string)).collect())
        }
        fn manifest(&self, dir: &str) -> Result<Option<String>, Trouble> {
            self.check()?;
            Ok(std::fs::read_to_string(self.root.join(dir).join(MANIFEST)).ok())
        }
        fn put(&self, dir: &str, rel: &str, from: &Path) -> Result<(), Trouble> {
            self.check()?;
            if self.die_after.get() == Some(self.puts.get()) {
                // Half a file, then nothing more.
                let to = self.root.join(dir).join(rel);
                std::fs::create_dir_all(to.parent().unwrap()).unwrap();
                let bytes = std::fs::read(from).unwrap();
                std::fs::write(to, &bytes[..bytes.len() / 2]).unwrap();
                return Err(Trouble::Failed("killed".into()));
            }
            self.puts.set(self.puts.get() + 1);
            let to = self.root.join(dir).join(rel);
            std::fs::create_dir_all(to.parent().unwrap()).map_err(|e| Trouble::Failed(e.to_string()))?;
            std::fs::copy(from, to).map(|_| ()).map_err(|e| Trouble::Failed(e.to_string()))
        }
        fn put_manifest(&self, dir: &str, text: &str) -> Result<(), Trouble> {
            self.check()?;
            std::fs::create_dir_all(self.root.join(dir)).map_err(|e| Trouble::Failed(e.to_string()))?;
            std::fs::write(self.root.join(dir).join(MANIFEST), text).map_err(|e| Trouble::Failed(e.to_string()))
        }
        fn get(&self, dir: &str, rel: &str, to: &Path) -> Result<(), Trouble> {
            self.check()?;
            std::fs::copy(self.root.join(dir).join(rel), to).map(|_| ()).map_err(|e| Trouble::Failed(e.to_string()))
        }
    }

    fn write(data: &Path, name: &str, bytes: &[u8], written_s: u64) {
        let path = data.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, bytes).unwrap();
        std::fs::File::options().write(true).open(&path).unwrap().set_modified(UNIX_EPOCH + std::time::Duration::from_secs(written_s)).unwrap();
    }
    fn read(data: &Path, name: &str) -> Option<Vec<u8>> {
        std::fs::read(data.join(name)).ok()
    }
    fn known() -> Vec<Known> {
        vec![
            Known { key: ZELDA.into(), system: "gba".into(), file: "Zelda.gba".into(), title: "Zelda: Minish Cap".into(), core: String::new() },
            Known { key: MARIO.into(), system: "snes".into(), file: "Mario.sfc".into(), title: "Mario".into(), core: String::new() },
        ]
    }
    /// A phone that played Zelda (battery save, a moment, a slot with its picture) and Mario (a moment).
    fn played() -> tempfile::TempDir {
        let d = tempfile::tempdir().unwrap();
        write(d.path(), &format!("saves/{ZELDA}/Zelda.srm"), b"hearts", 1_000);
        write(d.path(), &format!("states/{ZELDA}/Zelda.pstate.auto.z"), b"moment", 2_000);
        write(d.path(), &format!("states/{ZELDA}/Zelda.pstate1"), b"slot one", 1_500);
        write(d.path(), &format!("states/{ZELDA}/Zelda.pstate1.png"), b"picture", 1_500);
        write(d.path(), &format!("states/{MARIO}/Mario.pstate.auto.z"), b"world 4", 3_000);
        // What a half-written file and a game that was only started leave.
        write(d.path(), &format!("states/{MARIO}/Mario.pstate2.tmp"), b"half", 3_000);
        std::fs::create_dir_all(d.path().join("saves/00000000000000cc")).unwrap();
        d
    }
    fn folder() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }
    fn dir_of(store: &Dir, key: &str) -> String {
        store.games().unwrap().into_iter().find(|d| key_of_dir(d) == Some(key)).unwrap()
    }

    #[test]
    fn a_game_that_ended_is_copied_into_a_folder_of_its_own() {
        let (phone, shared) = (played(), folder());
        let store = Dir::new(shared.path());
        let report = copy_games(phone.path(), &known(), None, &store, 5_000);
        assert_eq!((report.copied_games, report.copied_files, report.failed), (2, 5, 0), "{report:?}");
        assert!(report.sentence().unwrap().contains("Copied the saves of 2 games"));
        let zelda = dir_of(&store, ZELDA);
        assert_eq!(zelda, format!("Zelda_ Minish Cap [{ZELDA}]"), "a name for people; the key finds it again");
        assert_eq!(read(&store.root, &format!("{zelda}/saves/Zelda.srm")).as_deref(), Some(&b"hearts"[..]));
        assert_eq!(read(&store.root, &format!("{zelda}/states/Zelda.pstate1.png")).as_deref(), Some(&b"picture"[..]));
        assert!(!store.root.join(dir_of(&store, MARIO)).join("states/Mario.pstate2.tmp").exists(), "a half-written file is not a save");
        let manifest = parse_manifest(&std::fs::read_to_string(store.root.join(&zelda).join(MANIFEST)).unwrap()).unwrap();
        assert_eq!((manifest.key.as_str(), manifest.system.as_str(), manifest.files.len()), (ZELDA, "gba", 4));
        assert_eq!(manifest.files["saves/Zelda.srm"].ms, 1_000_000);
    }

    #[test]
    fn nothing_is_written_when_nothing_changed_and_a_new_save_goes_over_the_old_copy() {
        let (phone, shared) = (played(), folder());
        let store = Dir::new(shared.path());
        copy_games(phone.path(), &known(), None, &store, 5_000);
        let puts = store.puts.get();
        let again = copy_games(phone.path(), &known(), None, &store, 6_000);
        assert_eq!((again.copied_games, store.puts.get()), (0, puts), "no file is written again");
        assert!(again.sentence().is_none(), "nothing to say");
        // Zelda is played on: a newer battery save.
        write(phone.path(), &format!("saves/{ZELDA}/Zelda.srm"), b"more hearts", 7_000);
        let report = copy_games(phone.path(), &known(), Some(&[ZELDA.to_string()]), &store, 8_000);
        assert_eq!((report.copied_games, report.copied_files, report.conflicts.len()), (1, 1, 0), "{report:?}");
        assert_eq!(read(&store.root, &format!("{}/saves/Zelda.srm", dir_of(&store, ZELDA))).as_deref(), Some(&b"more hearts"[..]));
    }

    #[test]
    fn saves_come_back_after_uninstalling_the_app_and_a_moment_stays_newer_than_the_battery_save() {
        let (phone, shared) = (played(), folder());
        let store = Dir::new(shared.path());
        copy_games(phone.path(), &known(), None, &store, 5_000);
        // The app is removed and installed again: no saves, no state, the same folder chosen again.
        let fresh = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(fresh.path().join(format!("saves/{ZELDA}"))).unwrap(); // a start made the folders
        let report = restore(fresh.path(), &known(), &store, 9_000);
        assert_eq!((report.restored_games, report.conflicts.len(), report.failed), (2, 0, 0), "{report:?}");
        assert_eq!(report.sentence().unwrap(), "2 games have their saves back from your folder.");
        for (name, bytes) in [
            (format!("saves/{ZELDA}/Zelda.srm"), &b"hearts"[..]),
            (format!("states/{ZELDA}/Zelda.pstate.auto.z"), b"moment"),
            (format!("states/{ZELDA}/Zelda.pstate1"), b"slot one"),
            (format!("states/{MARIO}/Mario.pstate.auto.z"), b"world 4"),
        ] {
            assert_eq!(read(fresh.path(), &name).as_deref(), Some(bytes), "{name}");
        }
        let at = |name: String| std::fs::metadata(fresh.path().join(name)).unwrap().modified().unwrap().duration_since(UNIX_EPOCH).unwrap().as_secs();
        assert_eq!((at(format!("saves/{ZELDA}/Zelda.srm")), at(format!("states/{ZELDA}/Zelda.pstate.auto.z"))), (1_000, 2_000), "their order in time comes back");
        assert!(!fresh.path().join("copy-restore.part").exists());
        // Playing on copies again without a conflict: the restore is the base.
        write(fresh.path(), &format!("saves/{ZELDA}/Zelda.srm"), b"later hearts", 12_000);
        let next = copy_games(fresh.path(), &known(), Some(&[ZELDA.to_string()]), &store, 13_000);
        assert_eq!((next.copied_files, next.conflicts.len()), (1, 0), "{next:?}");
        // And a restore that finds everything in place does nothing and says nothing.
        assert!(restore(fresh.path(), &known(), &store, 14_000).sentence().is_none());
    }

    #[test]
    fn a_game_with_the_same_file_name_gets_its_saves_under_its_new_key() {
        // The folder's identity differs on the new phone: the game is found by console and file.
        let (phone, shared) = (played(), folder());
        let store = Dir::new(shared.path());
        copy_games(phone.path(), &known(), None, &store, 5_000);
        let moved: Vec<Known> = known().into_iter().map(|mut k| { k.key = k.key.replace("00000000000000", "11111111111111"); k }).collect();
        let fresh = tempfile::tempdir().unwrap();
        let report = restore(fresh.path(), &moved, &store, 9_000);
        assert_eq!(report.restored_games, 2, "{report:?}");
        assert_eq!(read(fresh.path(), "saves/11111111111111aa/Zelda.srm").as_deref(), Some(&b"hearts"[..]));
    }

    #[test]
    fn a_copy_is_never_overwritten_by_older_saves_and_the_conflict_is_logged() {
        let (phone, shared) = (played(), folder());
        let store = Dir::new(shared.path());
        copy_games(phone.path(), &known(), None, &store, 5_000);
        let copy_before = read(&store.root, &format!("{}/saves/Zelda.srm", dir_of(&store, ZELDA))).unwrap();
        // The app was installed again and Zelda played a while before the folder was chosen: a fresh, shorter save.
        let fresh = tempfile::tempdir().unwrap();
        write(fresh.path(), &format!("saves/{ZELDA}/Zelda.srm"), b"new game", 4_000);
        let report = restore(fresh.path(), &known(), &store, 9_000);
        assert_eq!(report.conflicts, vec!["Zelda: Minish Cap".to_string()], "{report:?}");
        assert!(report.log.iter().any(|l| l.contains("conflict") && l.contains("nothing overwritten")), "{:?}", report.log);
        assert_eq!(read(fresh.path(), &format!("saves/{ZELDA}/Zelda.srm")).as_deref(), Some(&b"new game"[..]), "the phone's file stays");
        assert!(read(fresh.path(), &format!("states/{ZELDA}/Zelda.pstate1")).is_none(), "no file of the copy is mixed into a game that has its own");
        // Copying must not push the new save over the copy either, even though it is the newer one by time.
        write(fresh.path(), &format!("saves/{ZELDA}/Zelda.srm"), b"new game", 99_000);
        let report = copy_games(fresh.path(), &known(), Some(&[ZELDA.to_string()]), &store, 100_000);
        assert_eq!((report.conflicts.len(), report.copied_files), (1, 0), "{report:?}");
        assert_eq!(read(&store.root, &format!("{}/saves/Zelda.srm", dir_of(&store, ZELDA))).unwrap(), copy_before, "the copy is as it was");
        assert!(report.sentence().unwrap().contains("Nothing was overwritten"));
        assert_eq!(state_words(fresh.path()).as_deref(), Some("1 game's saves differ from the copy in your folder. Nothing was overwritten."));
    }

    #[test]
    fn an_older_save_on_the_phone_never_replaces_a_newer_copy() {
        // This phone copied, the copy is as it left it, but a file here is older than the copy's (a restored card's clock).
        let (phone, shared) = (played(), folder());
        let store = Dir::new(shared.path());
        copy_games(phone.path(), &known(), None, &store, 5_000);
        write(phone.path(), &format!("saves/{ZELDA}/Zelda.srm"), b"other", 500);
        let report = copy_games(phone.path(), &known(), Some(&[ZELDA.to_string()]), &store, 6_000);
        assert_eq!((report.conflicts.len(), report.copied_files), (1, 0), "{report:?}");
        assert_eq!(read(&store.root, &format!("{}/saves/Zelda.srm", dir_of(&store, ZELDA))).as_deref(), Some(&b"hearts"[..]));
    }

    #[test]
    fn a_copy_that_another_phone_wrote_is_left_alone() {
        let (phone, shared) = (played(), folder());
        let store = Dir::new(shared.path());
        copy_games(phone.path(), &known(), None, &store, 5_000);
        // Another phone put a newer save into the shared folder (its list says so).
        let zelda = dir_of(&store, ZELDA);
        let mut manifest = parse_manifest(&std::fs::read_to_string(store.root.join(&zelda).join(MANIFEST)).unwrap()).unwrap();
        std::fs::write(store.root.join(&zelda).join("saves/Zelda.srm"), b"elsewhere").unwrap();
        manifest.files.get_mut("saves/Zelda.srm").unwrap().sha = sha256(&store.root.join(&zelda).join("saves/Zelda.srm")).unwrap();
        manifest.files.get_mut("saves/Zelda.srm").unwrap().ms = 50_000_000;
        std::fs::write(store.root.join(&zelda).join(MANIFEST), serde_json::to_string(&manifest).unwrap()).unwrap();
        let report = copy_games(phone.path(), &known(), Some(&[ZELDA.to_string()]), &store, 60_000);
        assert_eq!((report.copy_newer.len(), report.conflicts.len(), report.copied_files), (1, 0, 0), "{report:?}");
        assert_eq!(read(&store.root, &format!("{zelda}/saves/Zelda.srm")).as_deref(), Some(&b"elsewhere"[..]));
    }

    #[test]
    fn without_access_nothing_is_copied_and_the_player_is_told() {
        let (phone, shared) = (played(), folder());
        let store = Dir::new(shared.path());
        *store.trouble.borrow_mut() = Some(Trouble::NoAccess);
        let report = copy_games(phone.path(), &known(), None, &store, 5_000);
        assert_eq!(report.trouble, Some(Trouble::NoAccess));
        assert_eq!(report.sentence().unwrap(), NO_ACCESS, "a sentence, not silence");
        assert!(!store.root.exists());
        assert!(Index::load(phone.path()).games.is_empty(), "a failed copy is not remembered as done");
        assert_eq!(stale(phone.path()).len(), 2, "so it is tried again");
        // The folder is gone (the card was pulled).
        *store.trouble.borrow_mut() = Some(Trouble::Gone);
        let report = restore(tempfile::tempdir().unwrap().path(), &known(), &store, 5_000);
        assert_eq!(report.sentence().unwrap(), GONE);
    }

    #[test]
    fn a_copy_cut_short_is_not_taken_for_a_whole_one_and_is_finished_next_time() {
        let (phone, shared) = (played(), folder());
        let store = Dir::new(shared.path());
        store.die_after.set(Some(2));
        let report = copy_games(phone.path(), &known(), Some(&[ZELDA.to_string()]), &store, 5_000);
        assert_eq!(report.failed, 1, "{report:?}");
        // What a restore finds: the list holds the files that went through, and each is checked against it.
        store.die_after.set(None);
        let fresh = tempfile::tempdir().unwrap();
        let restored = restore(fresh.path(), &known(), &store, 6_000);
        assert!(restored.failed + restored.restored_games <= 1, "{restored:?}");
        let half = read(fresh.path(), &format!("saves/{ZELDA}/Zelda.srm"));
        assert!(half.is_none() || half.as_deref() == Some(&b"hearts"[..]), "never half a file: {half:?}");
        // The next copy writes everything again.
        let next = copy_games(phone.path(), &known(), Some(&[ZELDA.to_string()]), &store, 7_000);
        assert_eq!((next.failed, next.conflicts.len()), (0, 0), "{next:?}");
        let fresh = tempfile::tempdir().unwrap();
        assert_eq!(restore(fresh.path(), &known(), &store, 8_000).restored_games, 1);
        assert_eq!(read(fresh.path(), &format!("states/{ZELDA}/Zelda.pstate.auto.z")).as_deref(), Some(&b"moment"[..]));
    }

    #[test]
    fn a_file_that_does_not_match_its_list_stops_that_games_restore_and_nothing_stays_behind() {
        let (phone, shared) = (played(), folder());
        let store = Dir::new(shared.path());
        copy_games(phone.path(), &known(), None, &store, 5_000);
        std::fs::write(store.root.join(dir_of(&store, ZELDA)).join("states/Zelda.pstate1"), b"slo").unwrap();
        let fresh = tempfile::tempdir().unwrap();
        let report = restore(fresh.path(), &known(), &store, 6_000);
        assert_eq!((report.restored_games, report.failed), (1, 1), "Mario comes back, Zelda is refused: {report:?}");
        assert!(read(fresh.path(), &format!("saves/{ZELDA}/Zelda.srm")).is_none(), "not half of a game");
        assert!(!fresh.path().join("copy-restore.part").exists());
    }

    #[test]
    fn a_list_with_names_that_lead_outside_is_refused() {
        let shared = folder();
        let store = Dir::new(shared.path());
        let dir = format!("Evil [{ZELDA}]");
        let manifest = CopyManifest {
            format: 1,
            app: "pocket-ui".into(),
            key: ZELDA.into(),
            files: [("saves/../../escape".to_string(), FileInfo::default())].into(),
            ..Default::default()
        };
        store.put_manifest(&dir, &serde_json::to_string(&manifest).unwrap()).unwrap();
        let fresh = tempfile::tempdir().unwrap();
        let report = restore(fresh.path(), &known(), &store, 1);
        assert_eq!((report.restored_games, report.unreadable), (0, 1));
    }

    #[test]
    fn games_whose_documents_are_not_copied_into_the_app_are_copied_the_same() {
        // A CHD or disc image that Android hands over as a document: the game's key is its document's identity and the
        // saves live in the same two folders; the copy never looks at the game's own file.
        let phone = tempfile::tempdir().unwrap();
        let key = "0123456789abcdef";
        write(phone.path(), &format!("saves/{key}/Wild Arms XF (USA).srm"), b"memory card", 100);
        write(phone.path(), &format!("states/{key}/Wild Arms XF (USA).pstate.auto.z"), b"psp moment", 200);
        std::fs::create_dir_all(phone.path().join(format!("direct-content/{key}"))).unwrap();
        std::fs::write(phone.path().join(format!("direct-content/{key}/source.json")), b"{}").unwrap();
        let games = vec![Known { key: key.into(), system: "psp".into(), file: "Wild Arms XF (USA).chd".into(), title: "Wild Arms XF".into(), core: String::new() }];
        let shared = folder();
        let store = Dir::new(shared.path());
        let report = copy_games(phone.path(), &games, None, &store, 1_000);
        assert_eq!((report.copied_games, report.copied_files), (1, 2), "{report:?}");
        let names: Vec<String> = store.games().unwrap();
        assert_eq!(names, vec![format!("Wild Arms XF [{key}]")]);
        assert!(!store.root.join(&names[0]).join("direct-content").exists() && !store.root.join(&names[0]).join("source.json").exists());
    }

    #[test]
    fn the_start_sees_what_was_not_copied_from_the_files_alone() {
        let (phone, shared) = (played(), folder());
        let store = Dir::new(shared.path());
        assert_eq!(stale(phone.path()).len(), 2, "never copied");
        mark_pending(phone.path(), ZELDA).unwrap();
        assert!(mark_pending(phone.path(), "../escape").is_err());
        assert_eq!(pending(phone.path()), vec![ZELDA.to_string()]);
        copy_games(phone.path(), &known(), None, &store, 5_000);
        clear_pending(phone.path(), &[ZELDA.to_string()]).unwrap();
        assert!(stale(phone.path()).is_empty() && pending(phone.path()).is_empty());
        write(phone.path(), &format!("states/{MARIO}/Mario.pstate.auto.z"), b"world 5", 9_000);
        assert_eq!(stale(phone.path()), vec![MARIO.to_string()]);
    }

    #[test]
    fn folder_names_hold_the_key_and_no_character_a_provider_refuses() {
        let name = dir_name(ZELDA, "AC/DC: [Live] \"x\"?");
        assert_eq!(name, format!("AC_DC_ _Live_ _x__ [{ZELDA}]"));
        assert_eq!(key_of_dir(&name), Some(ZELDA));
        assert_eq!(dir_name(ZELDA, "  "), format!("Game [{ZELDA}]"));
        assert_eq!(key_of_dir("Zelda"), None);
        assert_eq!(key_of_dir("Zelda [nothex000000000a]"), None);
    }
}
