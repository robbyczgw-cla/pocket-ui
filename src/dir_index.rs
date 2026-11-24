//! Which pictures a folder on the card holds, listed once: the covers' look-ups ask the list instead of the card.
//!
//! Every game on the shelf is looked for in the card's picture folders (the gamelists' pictures, ES-DE's folders, the
//! folders beside the games: `gamelist::usable_cover`), and on a big card most of those names are not there. On exFAT a
//! name that is not there costs a walk through the whole folder (it has no index): with 13,500 games in a folder, ten
//! misses per game made a big card's shelf stall (a player's R36S card of 179,305 files, 21.11.2026). Here a folder is
//! read once (`readdir`), and a name the list does not have is not asked for. A name the list has is still looked at
//! (`metadata`: its size, and a case-sensitive card's spelling).
//!
//! Only for folders pocket-ui does not write into: its own covers (downloads, the web page's cover editor, RomM) are
//! asked directly, so a picture it just wrote is found at once.
//!
//! The lists stay until [`changed`] (new covers, a gamelist read again, a rescan): then each folder is looked at once
//! more when it is next asked, and read again only when its modification time moved.
use std::collections::{HashMap, HashSet};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::SystemTime;

static LOOKUPS: AtomicUsize = AtomicUsize::new(0);
static LISTINGS: AtomicUsize = AtomicUsize::new(0);
static GENERATION: AtomicU64 = AtomicU64::new(0);
static FOLDERS: Mutex<Option<Folders>> = Mutex::new(None);

/// A folder's names (lower case: exFAT and FAT find "Game.PNG" as "game.png"; None: it could not be read, so every
/// name is asked), the modification time and when they were read, and the generation (`changed`) they were last
/// checked in.
struct Listing {
    names: Option<HashSet<OsString>>,
    modified: Option<SystemTime>,
    generation: u64,
    read_at: SystemTime,
}

/// FAT keeps a folder's time in 2-second steps: a list read within that long of it may miss a file written in the
/// same step, and is read again at the next check.
const COARSE: std::time::Duration = std::time::Duration::from_secs(2);

impl Listing {
    fn still_fresh(&self, now: Option<SystemTime>) -> bool {
        now.is_some() && now == self.modified && now.is_some_and(|m| self.read_at > m + COARSE)
    }
}

/// The folders listed so far. The app has one (`may_exist`); a test its own.
#[derive(Default)]
pub struct Folders {
    lists: HashMap<PathBuf, Listing>,
    /// Look-ups and `readdir`s this one made.
    pub lookups: usize,
    pub listings: usize,
}

impl Folders {
    fn metadata(&mut self, path: &Path) -> std::io::Result<std::fs::Metadata> {
        self.lookups += 1;
        metadata(path)
    }

    /// `dir` read, with its modification time (`modified`: already looked at).
    fn read(&mut self, dir: &Path, generation: u64, modified: Option<Option<SystemTime>>) -> Listing {
        let modified = modified.unwrap_or_else(|| self.modified(dir));
        self.listings += 1;
        LISTINGS.fetch_add(1, Ordering::Relaxed);
        let names = match crate::background_io::read_dir(dir) {
            Ok(entries) => Some(entries.flatten().map(|e| lower(&e.file_name())).collect()),
            // No folder: nothing in it.
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Some(HashSet::new()),
            Err(_) => None,
        };
        Listing { names, modified, generation, read_at: SystemTime::now() }
    }

    /// Whether `path` may be there, as of `generation`: false only when its folder's list does not have the name.
    pub fn may_exist(&mut self, path: &Path, generation: u64) -> bool {
        if crate::background_io::cancelled() { return false; }
        let (Some(dir), Some(name)) = (path.parent(), path.file_name()) else { return true };
        if dir.as_os_str().is_empty() {
            return true;
        }
        let stale = match self.lists.get(dir).map(|l| l.generation == generation) {
            None => Some(None),
            Some(true) => None,
            Some(false) => {
                let now = self.modified(dir);
                let listing = self.lists.get_mut(dir).expect("listed");
                if listing.still_fresh(now) {
                    listing.generation = generation;
                    None
                } else {
                    Some(Some(now))
                }
            }
        };
        if let Some(modified) = stale {
            let listing = self.read(dir, generation, modified);
            if crate::background_io::cancelled() { return false; }
            self.lists.insert(dir.to_path_buf(), listing);
        }
        self.lists[dir].names.as_ref().is_none_or(|names| names.contains(&lower(name)))
    }

    fn modified(&mut self, dir: &Path) -> Option<SystemTime> {
        self.metadata(dir).ok().and_then(|m| m.modified().ok())
    }

    /// `path` is a file there: its metadata, or None without asking the card when its folder's list does not have it.
    pub fn file_metadata(&mut self, path: &Path, generation: u64) -> Option<std::fs::Metadata> {
        if !self.may_exist(path, generation) {
            return None;
        }
        self.metadata(path).ok().filter(|m| m.is_file())
    }
}

/// How many files the covers' look-ups asked the file system about (`metadata` and the folder checks), since the start.
pub fn lookups() -> usize {
    LOOKUPS.load(Ordering::Relaxed)
}

/// How many folders were read (`readdir`), since the start.
pub fn listings() -> usize {
    LISTINGS.load(Ordering::Relaxed)
}

/// Files may have come or gone: each folder is checked again (its modification time) the next time it is asked.
pub fn changed() {
    GENERATION.fetch_add(1, Ordering::Relaxed);
}

/// `std::fs::metadata`, counted.
pub fn metadata(path: &Path) -> std::io::Result<std::fs::Metadata> {
    LOOKUPS.fetch_add(1, Ordering::Relaxed);
    crate::background_io::metadata(path)
}

fn lower(name: &std::ffi::OsStr) -> OsString {
    match name.to_str() {
        Some(s) => OsString::from(s.to_lowercase()),
        None => name.to_os_string(),
    }
}

/// Whether `path` may be there: false only when its folder's list does not have the name (no file system call then,
/// once the folder is listed). True asks the caller to look (`metadata`).
pub fn may_exist(path: &Path) -> bool {
    let generation = GENERATION.load(Ordering::Relaxed);
    FOLDERS.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(Folders::default).may_exist(path, generation)
}

/// `path` is a file there: its metadata, or None without asking the card when its folder's list does not have it.
pub fn file_metadata(path: &Path) -> Option<std::fs::Metadata> {
    let generation = GENERATION.load(Ordering::Relaxed);
    FOLDERS.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(Folders::default).file_metadata(path, generation)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_folder_is_read_once_and_names_it_lacks_cost_nothing() {
        let tmp = tempfile::Builder::new().prefix("pocketui-dir-index-").tempdir().unwrap();
        let dir = tmp.path().to_path_buf();
        std::fs::write(dir.join("Found.PNG"), b"x").unwrap();
        // A folder last written a while ago (one written just now is read again at each check, `COARSE`).
        let past = |d: &Path, secs: u64| std::fs::File::open(d).unwrap().set_modified(SystemTime::now() - std::time::Duration::from_secs(secs)).unwrap();
        past(&dir, 60);
        let mut f = Folders::default();
        let mut g = 0;
        for i in 0..1000 {
            assert!(!f.may_exist(&dir.join(format!("missing {i}.png")), g));
        }
        assert!(f.may_exist(&dir.join("found.png"), g), "the case of a FAT or exFAT card");
        assert!(f.file_metadata(&dir.join("Found.PNG"), g).is_some());
        assert!(f.file_metadata(&dir.join("gone.png"), g).is_none());
        assert_eq!(f.listings, 1, "one readdir for 1,003 names");
        assert_eq!(f.lookups, 2, "the folder's time once, the file found once");
        // Unchanged folder: checked once more after `changed`, not read again.
        g += 1;
        assert!(!f.may_exist(&dir.join("missing 1.png"), g));
        assert!(!f.may_exist(&dir.join("missing 2.png"), g));
        assert_eq!((f.listings, f.lookups), (1, 3));
        // A cover written into it: read again.
        std::fs::write(dir.join("new.png"), b"x").unwrap();
        past(&dir, 30);
        g += 1;
        assert!(f.may_exist(&dir.join("new.png"), g));
        assert_eq!(f.listings, 2);
        // Written within FAT's 2-second step of the reading: read again at the next check even with the same time.
        std::fs::write(dir.join("newer.png"), b"x").unwrap();
        g += 1;
        assert!(f.may_exist(&dir.join("newer.png"), g));
        std::fs::write(dir.join("newest.png"), b"x").unwrap();
        g += 1;
        assert!(f.may_exist(&dir.join("newest.png"), g), "a file of the same 2-second step");
        // No check without `changed`: the list stands (the covers' own answers are kept until then too).
        let reads = f.listings;
        assert!(!f.may_exist(&dir.join("later.png"), g));
        std::fs::write(dir.join("later.png"), b"x").unwrap();
        assert!(!f.may_exist(&dir.join("later.png"), g));
        assert_eq!(f.listings, reads);
        g += 1;
        assert!(f.may_exist(&dir.join("later.png"), g));
        // A folder that is not there: listed as empty, once; made later, found after `changed`.
        let none = dir.join("nothing-here");
        let reads = f.listings;
        assert!(!f.may_exist(&none.join("a.png"), g));
        assert!(!f.may_exist(&none.join("b.png"), g));
        assert_eq!(f.listings - reads, 1);
        std::fs::create_dir(&none).unwrap();
        std::fs::write(none.join("a.png"), b"x").unwrap();
        g += 1;
        assert!(f.may_exist(&none.join("a.png"), g));
        // An empty folder: nothing in it.
        let empty = dir.join("empty");
        std::fs::create_dir(&empty).unwrap();
        assert!(!f.may_exist(&empty.join("a.png"), g));
        // A path that is a file, not a folder: it cannot be listed, so the name is asked (as main did).
        assert!(f.may_exist(&dir.join("Found.PNG").join("a.png"), g));
        // A bare name has no folder to list: asked as before.
        assert!(f.may_exist(Path::new("cover.png"), g));
    }
}
