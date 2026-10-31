//! The shelf's game list as the last scan found it, kept in `<data>/library-cache.json`, so that a start with a cold
//! page cache (every power-on) shows the shelf without first listing every ROM folder on the card (205 ms of the
//! Mini V2's 319 ms to the first frame, 332 games in 23 systems; docs/ARCHITECTURE.md, Start).
//!
//! * [`start`]: the list for the first frame: the cache when it belongs to this es_systems.cfg (path, mtime, size),
//!   this cache format ([`VERSION`]) and this version of pocket-ui; otherwise the folders are read as before
//!   (`library::load`), which is also what a first boot does.
//! * [`rescan`]: after the first frame the folders are read again on a thread; only a list that differs from the
//!   one on screen comes back (the app swaps it in, `app_library.rs`), and only then is the cache written. Equal
//!   lists change nothing: no frame, no write.
//! * [`write`] / [`store_soon`]: the file is replaced atomically (`fsutil::write_atomic`), never on the UI thread, in
//!   the order the lists were handed in.
//!
//! The lists are without the games the player hid (`library::load_shown`): the cache holds what the shelf shows, so a
//! rescan compares like with like, and a game hidden or shown again is a change like any other.
//!
//! A damaged, cut-off or foreign file is ignored (logged) and the folders are read. The first line is the [`Key`]
//! alone, so a stale cache is turned away without parsing the list. `POCKETUI_LIBRARY_CACHE=0` switches the cache
//! off (every start reads the folders before the first frame, as before 2026-10-24), for comparisons on the device.

use crate::library::{self, System};
use std::io::{self, BufRead};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

/// The cache's file in the data dir.
pub const FILE: &str = "library-cache.json";
/// The cache format; a file of another version is read as missing. 2 (2026-10-24): names of the same game twice in a
/// system carry what tells them apart, versions are taken off names ("Pipes.v1.02"), and a disc whose track file is
/// missing is left out: a list of format 1 would show the old names until the rescan swapped them.
/// 3 (2026-10-27): a file name with no capital letter is cased for the shelf ("Libbet and the Magic Floor").
pub const VERSION: u32 = 3;
/// `0` switches the cache off.
pub const ENV: &str = "POCKETUI_LIBRARY_CACHE";

/// What a cached list belongs to: when any of it differs, the list is not used.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Key {
    pub version: u32,
    /// pocket-ui's version: an update may read the folders differently.
    pub app: String,
    pub es_systems: String,
    pub mtime: i64,
    pub mtime_ns: i64,
    pub len: u64,
}

impl Key {
    /// The key of `es_systems` as it is now; None when it can't be read (then there is nothing to cache either).
    pub fn now(es_systems: &Path) -> Option<Key> {
        use std::os::unix::fs::MetadataExt;
        let m = std::fs::metadata(es_systems).ok()?;
        Some(Key {
            version: VERSION,
            app: crate::update::app_version().to_string(),
            es_systems: es_systems.to_string_lossy().into_owned(),
            mtime: m.mtime(),
            mtime_ns: m.mtime_nsec(),
            len: m.len(),
        })
    }
}

/// Whether the cache is used (`POCKETUI_LIBRARY_CACHE` is not `0`).
pub fn enabled() -> bool {
    std::env::var(ENV).map_or(true, |v| v.trim() != "0")
}

pub fn path(data_dir: &Path) -> PathBuf {
    data_dir.join(FILE)
}

/// The cached list if it belongs to `key`; Err says why not (no file, another key, damaged), for the log.
pub fn read(data_dir: &Path, key: &Key) -> Result<Vec<System>, String> {
    let file = std::fs::File::open(path(data_dir)).map_err(|e| if e.kind() == io::ErrorKind::NotFound { "none yet".to_string() } else { e.to_string() })?;
    let mut r = io::BufReader::new(file);
    let mut first = String::new();
    r.read_line(&mut first).map_err(|e| e.to_string())?;
    let found: Key = serde_json::from_str(first.trim_end()).map_err(|e| format!("damaged ({e})"))?;
    if found != *key {
        return Err(stale_reason(&found, key));
    }
    let systems: Vec<System> = serde_json::from_reader(r).map_err(|e| format!("damaged ({e})"))?;
    library::learn_names(&systems);
    Ok(systems)
}

fn stale_reason(found: &Key, now: &Key) -> String {
    if found.version != now.version {
        format!("cache format {} (this version reads {})", found.version, now.version)
    } else if found.app != now.app {
        format!("written by version {}", found.app)
    } else if found.es_systems != now.es_systems {
        format!("for {}", found.es_systems)
    } else {
        "es_systems.cfg changed".to_string()
    }
}

/// Serializes the writes: per cache file, the number of the list written last; the newest list handed in is the one
/// that stays (`store_soon` and a rescan on two threads).
static WRITTEN: Mutex<Vec<(PathBuf, u64)>> = Mutex::new(Vec::new());
static SEQ: AtomicU64 = AtomicU64::new(0);

/// Replaces the cache with `systems` for `key`, atomically. Call off the UI thread (two `fsync`s on the card).
pub fn write(data_dir: &Path, key: &Key, systems: &[System]) -> io::Result<()> {
    write_numbered(data_dir, key, systems, SEQ.fetch_add(1, Ordering::SeqCst) + 1)
}

fn write_numbered(data_dir: &Path, key: &Key, systems: &[System], seq: u64) -> io::Result<()> {
    let mut bytes = serde_json::to_vec(key).map_err(io::Error::other)?;
    bytes.push(b'\n');
    serde_json::to_writer(&mut bytes, systems).map_err(io::Error::other)?;
    let file = path(data_dir);
    let mut written = WRITTEN.lock().unwrap_or_else(|e| e.into_inner());
    if written.iter().any(|(p, n)| *p == file && *n > seq) {
        // A newer list was written while this one waited.
        return Ok(());
    }
    crate::fsutil::write_atomic(&file, &bytes)?;
    written.retain(|(p, _)| *p != file);
    written.push((file, seq));
    Ok(())
}

/// [`write`] on a thread of its own, for a list read on the UI thread (Add games, an upload). The key is taken now.
pub fn store_soon(data_dir: &Path, es_systems: &Path, systems: Vec<System>) {
    if !enabled() {
        return;
    }
    let seq = SEQ.fetch_add(1, Ordering::SeqCst) + 1;
    let (data_dir, key) = (data_dir.to_path_buf(), Key::now(es_systems));
    let _ = std::thread::Builder::new().name("library-cache".into()).spawn(move || {
        if let Some(key) = key {
            if let Err(e) = write_numbered(&data_dir, &key, &systems, seq) {
                eprintln!("library: cache not written: {e}");
            }
        }
    });
}

/// The list for the first frame.
#[derive(Debug)]
pub enum Start {
    /// From the cache, which belongs to `key`: the folders are read again after the first frame ([`rescan`]).
    Cached { systems: Vec<System>, key: Key },
    /// Read from the folders (no usable cache): the cache is written after the first frame, for `key`.
    Scanned { systems: Vec<System>, key: Option<Key> },
}

impl Start {
    pub fn systems(&self) -> &[System] {
        match self {
            Start::Cached { systems: s, .. } | Start::Scanned { systems: s, .. } => s,
        }
    }
}

/// The cache when it is good for `es_systems`, else the folders (`library::load`, the error as it gives it). Logs
/// why a cache was not used.
pub fn start(data_dir: &Path, es_systems: &Path) -> Result<Start, String> {
    if !enabled() {
        return library::load_shown(es_systems, data_dir).map(|systems| Start::Scanned { systems, key: None });
    }
    let key = Key::now(es_systems);
    if let Some(k) = &key {
        match read(data_dir, k) {
            Ok(systems) => return Ok(Start::Cached { systems, key: k.clone() }),
            Err(why) => println!("library: cache not used: {why}"),
        }
    }
    library::load_shown(es_systems, data_dir).map(|systems| Start::Scanned { systems, key })
}

/// What reading the folders again found.
#[derive(Debug, PartialEq)]
pub enum Rescan {
    /// The same list as on screen: nothing to do (the cache is rewritten only if es_systems.cfg's key moved on).
    Same,
    /// A different list (games added, removed or renamed): the cache holds it already, for `key`.
    Changed { systems: Vec<System>, key: Option<Key> },
    Failed(String),
}

/// Reads the folders again and compares with `shown`, the list on screen (which the cache holds, or held when
/// `cached_key` was current). Writes the cache only when something changed.
pub fn rescan(data_dir: &Path, es_systems: &Path, shown: &[System], cached_key: Option<&Key>) -> Rescan {
    let key = Key::now(es_systems);
    let fresh = match library::load_shown(es_systems, data_dir) {
        Ok(s) => s,
        Err(e) => return Rescan::Failed(e),
    };
    let same = fresh == shown;
    if same && key.as_ref() == cached_key {
        return Rescan::Same;
    }
    if let (Some(k), true) = (&key, enabled()) {
        if let Err(e) = write(data_dir, k, &fresh) {
            eprintln!("library: cache not written: {e}");
        }
    }
    if same {
        Rescan::Same
    } else {
        Rescan::Changed { systems: fresh, key }
    }
}

/// "3 games added, 1 removed" between two lists, for the log.
pub fn difference(old: &[System], new: &[System]) -> String {
    use std::collections::HashSet;
    let paths = |l: &[System]| l.iter().flat_map(|s| s.games.iter().map(|g| g.path.clone())).collect::<HashSet<_>>();
    let (a, b) = (paths(old), paths(new));
    let (added, removed) = (b.difference(&a).count(), a.difference(&b).count());
    format!("{added} games added, {removed} removed, {} systems before, {} now", old.len(), new.len())
}

/// The message for a game whose file is gone (deleted or moved since the list was read); None while it is there.
pub fn missing_game(name: &str, path: &Path) -> Option<String> {
    if path.exists() || path.as_os_str().is_empty() {
        return None;
    }
    let file = path.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_else(|| path.display().to_string());
    Some(format!("{name} can't start: its file \"{file}\" is no longer there. The list of games is being read again."))
}

pub use crate::fsutil::is_empty_file;

/// What the player is told about a game whose file is empty (the start, the game's card); None for any other game.
pub fn empty_game(name: &str, path: &Path) -> Option<String> {
    is_empty_file(path).then(|| format!("{name} can't start: its file is empty (0 bytes), so the copy didn't finish. Copy the game to the card again."))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::Game;
    use crate::testutil::TestDir;

    #[test]
    fn an_empty_game_file_is_named_as_such_and_a_healthy_or_missing_one_is_not() {
        let dir = TestDir::new("libcache-empty");
        let (empty, full, gone) = (dir.join("Tetris.gb"), dir.join("Mario.gb"), dir.join("Nothing.gb"));
        std::fs::write(&empty, b"").unwrap();
        std::fs::write(&full, b"x").unwrap();
        let msg = empty_game("Tetris", &empty).unwrap();
        assert!(msg.contains("Tetris") && msg.contains("empty") && !msg.contains("BIOS"), "{msg}");
        assert_eq!(empty_game("Mario", &full), None);
        assert_eq!(empty_game("Nothing", &gone), None, "a missing file has its own message (missing_game)");
        assert_eq!(empty_game("Dir", &dir.join(".")), None, "a folder game is no empty file");
    }

    struct Fixture {
        dir: crate::testutil::TestDir,
    }

    impl Fixture {
        fn new(tag: &str) -> Fixture {
            let dir = TestDir::new(&format!("libcache-{tag}"));
            std::fs::create_dir_all(dir.join("roms/gba")).unwrap();
            std::fs::create_dir_all(dir.join("data")).unwrap();
            for g in ["Alpha.gba", "Beta.gba"] {
                std::fs::write(dir.join("roms/gba").join(g), b"x").unwrap();
            }
            std::fs::write(
                dir.join("es_systems.cfg"),
                format!(
                    "<systemList><system><name>gba</name><fullname>Game Boy Advance</fullname><path>{}</path><extension>.gba</extension>\
                     <emulators><emulator name=\"retroarch\"><cores><core default=\"true\">mgba</core></cores></emulator></emulators></system></systemList>",
                    dir.join("roms/gba").display()
                ),
            )
            .unwrap();
            Fixture { dir }
        }
        fn es(&self) -> PathBuf {
            self.dir.join("es_systems.cfg")
        }
        fn data(&self) -> PathBuf {
            self.dir.join("data")
        }
        fn names(systems: &[System]) -> Vec<String> {
            systems.iter().flat_map(|s| s.games.iter().map(|g| g.name.clone())).collect()
        }
    }


    #[test]
    fn a_scanned_list_comes_back_from_the_cache_unchanged() {
        let f = Fixture::new("roundtrip");
        let Ok(Start::Scanned { systems, key: Some(key) }) = start(&f.data(), &f.es()) else { panic!("no cache yet: read from the folders") };
        assert_eq!(Fixture::names(&systems), ["Alpha", "Beta"]);
        assert_eq!(systems[0].core.as_deref(), Some("mgba"));
        write(&f.data(), &key, &systems).unwrap();
        // The folders change; the cache still says what was there (that's the point: they aren't read).
        std::fs::remove_file(f.dir.join("roms/gba/Beta.gba")).unwrap();
        match start(&f.data(), &f.es()).unwrap() {
            Start::Cached { systems: cached, key: k } => {
                assert_eq!(cached, systems, "every field, emulators and paths included");
                assert_eq!(k, key);
            }
            other => panic!("expected the cache, got {other:?}"),
        }
        // The first line is the key alone.
        let text = std::fs::read_to_string(path(&f.data())).unwrap();
        let first: Key = serde_json::from_str(text.lines().next().unwrap()).unwrap();
        assert_eq!(first, key);
    }

    #[test]
    fn a_changed_es_systems_a_new_format_or_version_is_not_used() {
        let f = Fixture::new("invalid");
        let key = Key::now(&f.es()).unwrap();
        let systems = library::load(&f.es()).unwrap();
        for (what, other) in [
            ("format", Key { version: VERSION + 1, ..key.clone() }),
            ("app version", Key { app: "0.0.0-old".into(), ..key.clone() }),
            ("size", Key { len: key.len + 1, ..key.clone() }),
            ("mtime", Key { mtime: key.mtime - 10, ..key.clone() }),
            ("another es_systems.cfg", Key { es_systems: "/elsewhere/es_systems.cfg".into(), ..key.clone() }),
        ] {
            write(&f.data(), &other, &systems).unwrap();
            assert!(read(&f.data(), &key).is_err(), "{what}");
            assert!(matches!(start(&f.data(), &f.es()), Ok(Start::Scanned { .. })), "{what}: the folders are read");
        }
        // es_systems.cfg rewritten (a different size): the cache of the old one is stale.
        write(&f.data(), &key, &systems).unwrap();
        assert!(read(&f.data(), &key).is_ok());
        let cfg = std::fs::read_to_string(f.es()).unwrap();
        std::fs::write(f.es(), format!("{cfg}\n")).unwrap();
        assert_eq!(read(&f.data(), &Key::now(&f.es()).unwrap()), Err("es_systems.cfg changed".into()));
    }

    #[test]
    fn a_damaged_cache_is_ignored_and_the_folders_are_read() {
        let f = Fixture::new("corrupt");
        let key = Key::now(&f.es()).unwrap();
        let systems = library::load(&f.es()).unwrap();
        write(&f.data(), &key, &systems).unwrap();
        let good = std::fs::read(path(&f.data())).unwrap();
        for (what, bytes) in [
            ("empty", Vec::new()),
            ("cut off", good[..good.len() - 7].to_vec()),
            ("cut off in the key", good[..10].to_vec()),
            ("garbage", b"\x00\xffnot json at all".to_vec()),
            ("no list", [&good[..good.iter().position(|b| *b == b'\n').unwrap() + 1], b"{}"].concat()),
        ] {
            std::fs::write(path(&f.data()), &bytes).unwrap();
            assert!(read(&f.data(), &key).is_err(), "{what}");
            match start(&f.data(), &f.es()) {
                Ok(Start::Scanned { systems: s, .. }) => assert_eq!(s, systems, "{what}"),
                other => panic!("{what}: {other:?}"),
            }
        }
    }

    /// Hide on the card: every list is made from what `start` and `rescan` give, so the game is gone from the shelf,
    /// the search and Curator (both built from that list) and Favorites (resolved against it) at once, and back the
    /// same way. The cache holds the shown list, so a start from it shows what the shelf showed.
    #[test]
    fn a_hidden_game_is_left_out_of_the_start_the_rescan_the_cache_and_the_search() {
        let f = Fixture::new("hidden");
        let beta = f.dir.join("roms/gba/Beta.gba");
        let mut meta = crate::library_meta::LibraryMeta::load(&f.data());
        meta.set_hidden(&beta, true).unwrap();
        let Ok(Start::Scanned { systems, key: Some(key) }) = start(&f.data(), &f.es()) else { panic!("read from the folders") };
        assert_eq!(Fixture::names(&systems), ["Alpha"]);
        let games: Vec<(usize, usize, &str)> = systems.iter().enumerate().flat_map(|(s, sys)| sys.games.iter().enumerate().map(move |(g, x)| (s, g, x.name.as_str()))).collect();
        assert!(crate::search::Index::build(games).search("beta", &|_| true).is_empty(), "the search is made from the list");
        write(&f.data(), &key, &systems).unwrap();
        assert_eq!(rescan(&f.data(), &f.es(), &systems, Some(&key)), Rescan::Same, "a hidden game is no change by itself");
        meta.set_hidden(&beta, false).unwrap();
        let Rescan::Changed { systems: fresh, .. } = rescan(&f.data(), &f.es(), &systems, Some(&key)) else { panic!("shown again is a change") };
        assert_eq!(Fixture::names(&fresh), ["Alpha", "Beta"]);
        assert!(matches!(start(&f.data(), &f.es()), Ok(Start::Cached { systems, .. }) if Fixture::names(&systems) == ["Alpha", "Beta"]));
    }

    #[test]
    fn the_rescan_reports_and_writes_only_a_change() {
        let f = Fixture::new("rescan");
        let key = Key::now(&f.es()).unwrap();
        let shown = library::load(&f.es()).unwrap();
        // Equal: nothing is written (no cache file appears) and nothing comes back.
        assert_eq!(rescan(&f.data(), &f.es(), &shown, Some(&key)), Rescan::Same);
        assert!(!path(&f.data()).exists(), "an equal list writes nothing");
        write(&f.data(), &key, &shown).unwrap();
        let before = std::fs::metadata(path(&f.data())).unwrap().modified().unwrap();
        assert_eq!(rescan(&f.data(), &f.es(), &shown, Some(&key)), Rescan::Same);
        assert_eq!(std::fs::metadata(path(&f.data())).unwrap().modified().unwrap(), before);
        // A game added and one deleted: the new list, and the cache holds it.
        std::fs::write(f.dir.join("roms/gba/Gamma.gba"), b"x").unwrap();
        std::fs::remove_file(f.dir.join("roms/gba/Alpha.gba")).unwrap();
        let Rescan::Changed { systems: fresh, key: Some(k) } = rescan(&f.data(), &f.es(), &shown, Some(&key)) else { panic!("a change") };
        assert_eq!(k, key);
        assert_eq!(Fixture::names(&fresh), ["Beta", "Gamma"]);
        assert_eq!(read(&f.data(), &key).unwrap(), fresh);
        assert_eq!(difference(&shown, &fresh), "1 games added, 1 removed, 1 systems before, 1 now");
        // es_systems.cfg gone: the rescan fails and the list on screen stays.
        std::fs::remove_file(f.es()).unwrap();
        assert!(matches!(rescan(&f.data(), &f.es(), &fresh, Some(&key)), Rescan::Failed(_)));
    }

    /// A gamelist.xml that appears next to the games: the rescan finds the new names (a changed list, which the cache
    /// then holds), and a start from the cache shows them.
    #[test]
    fn a_gamelist_that_appears_is_a_change_the_rescan_brings_in() {
        let f = Fixture::new("gamelist");
        std::fs::write(f.dir.join("roms/gba/sf2ce.gba"), b"x").unwrap();
        let Ok(Start::Scanned { systems, key: Some(key) }) = start(&f.data(), &f.es()) else { panic!("read from the folders") };
        assert_eq!(Fixture::names(&systems), ["Alpha", "Beta", "sf2ce"]);
        write(&f.data(), &key, &systems).unwrap();
        std::fs::write(f.dir.join("roms/gba/gamelist.xml"), "<gameList><game><path>./sf2ce.gba</path><name>Street Fighter II</name></game></gameList>").unwrap();
        let Rescan::Changed { systems: fresh, .. } = rescan(&f.data(), &f.es(), &systems, Some(&key)) else { panic!("a gamelist is a change") };
        assert_eq!(Fixture::names(&fresh), ["Alpha", "Beta", "Street Fighter II"]);
        assert!(matches!(start(&f.data(), &f.es()), Ok(Start::Cached { systems, .. }) if Fixture::names(&systems)[2] == "Street Fighter II"));
        // The same list again: nothing changes.
        assert_eq!(rescan(&f.data(), &f.es(), &fresh, Some(&key)), Rescan::Same);
        // The gamelist goes: the file names are back.
        std::fs::remove_file(f.dir.join("roms/gba/gamelist.xml")).unwrap();
        let Rescan::Changed { systems: back, .. } = rescan(&f.data(), &f.es(), &fresh, Some(&key)) else { panic!("the names come back") };
        assert_eq!(Fixture::names(&back), ["Alpha", "Beta", "sf2ce"]);
    }

    #[test]
    fn the_newest_list_handed_in_stays() {
        let f = Fixture::new("order");
        let key = Key::now(&f.es()).unwrap();
        let one = vec![System { name: "gba".into(), fullname: "GBA".into(), emulator: None, core: None, emulators: vec![], games: vec![Game { name: "One".into(), path: "/r/one.gba".into() }] }];
        let mut two = one.clone();
        two[0].games[0].name = "Two".into();
        // Numbered 2 written first, then 1 (handed in earlier, slower thread): 2 stays.
        let base = SEQ.fetch_add(10, Ordering::SeqCst) + 1;
        write_numbered(&f.data(), &key, &two, base + 2).unwrap();
        write_numbered(&f.data(), &key, &one, base + 1).unwrap();
        assert_eq!(read(&f.data(), &key).unwrap(), two);
    }

    #[test]
    fn a_missing_game_file_gets_a_clear_message() {
        let f = Fixture::new("missing");
        let rom = f.dir.join("roms/gba/Alpha.gba");
        assert_eq!(missing_game("Alpha", &rom), None);
        std::fs::remove_file(&rom).unwrap();
        let msg = missing_game("Alpha", &rom).unwrap();
        assert!(msg.starts_with("Alpha can't start") && msg.contains("\"Alpha.gba\"") && msg.contains("read again"), "{msg}");
        // A folder game (DOS) is there as a folder.
        assert_eq!(missing_game("GBA", &f.dir.join("roms/gba")), None);
    }
}
