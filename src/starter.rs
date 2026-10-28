//! The starter pack: a few free homebrew games a new player can download at once, for a card with nothing on it
//! (docs/STARTER-PACK.md). The list is `assets/starter-pack.json`, compiled in: each game with its author, licence
//! (the words of the author's own licence notice) and the exact file to fetch, with its SHA-256. No ROM is in this
//! repository or in the image; the file comes from the author's own release page (or a pinned mirror) when the
//! player asks.
//!
//! `install` does one game: refuses when the file is already on the card (never overwrites, whatever the case of the
//! name), when the card has no room, or when the network is not there; downloads with the Store's curl (limits,
//! progress), checks the SHA-256 before anything is unpacked, takes the ROM out of a zip when the author ships one,
//! checks the ROM's own SHA-256, and only then writes it into the system's folder in one step (`fsutil::write_atomic`).
//! Whatever fails leaves the folder as it was and the download removed. Nothing here touches the UI: the pages run
//! it on a thread (`app_starter.rs`).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use serde::Deserialize;

use crate::store::{self, DownloadFailure};

/// `assets/starter-pack.json`.
const DATA: &str = include_str!("../assets/starter-pack.json");

/// The biggest file the pack may name (also what a download is allowed to grow to).
pub const MAX_FILE: u64 = 16 * 1024 * 1024;

/// Room left on the card beyond the game itself, so the card is never filled to the last byte.
const SLACK: u64 = 2 * 1024 * 1024;

#[derive(Debug, Deserialize)]
pub struct Pack {
    pub format: u32,
    /// The day the licences were last read at their sources.
    pub checked: String,
    pub games: Vec<Game>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Game {
    pub id: String,
    /// The ES system id (`gb`, `gbc`, `gba`, `nes`, `snes`, `megadrive`): the folder under `/storage/roms` comes from
    /// ES's `es_systems.cfg`.
    pub system: String,
    pub title: String,
    pub author: String,
    pub year: u16,
    pub description: String,
    /// The licence in full, for docs/STARTER-PACK.md.
    pub license: String,
    /// The licence in a few words, for the game card.
    pub license_short: String,
    pub license_url: String,
    /// The author's own words about the licence, from the page `license_url` points to.
    pub license_text: String,
    pub source_url: String,
    pub homepage: String,
    pub version: String,
    /// Where the file is hosted when that is not the author's own release page.
    #[serde(default)]
    pub mirror: Option<String>,
    pub file: File,
}

#[derive(Clone, Debug, Deserialize)]
pub struct File {
    pub url: String,
    /// Of the downloaded file.
    pub sha256: String,
    pub size: u64,
    /// The file name in the system's folder (it is what the shelf shows, minus the extension).
    pub name_on_disk: String,
    /// `"zip"`: the download is a zip with the ROM inside.
    #[serde(default)]
    pub archive: Option<String>,
    /// Of the ROM taken out of the archive.
    #[serde(default)]
    pub rom_sha256: Option<String>,
    #[serde(default)]
    pub rom_size: Option<u64>,
}

impl File {
    /// Bytes the ROM takes on the card.
    pub fn rom_bytes(&self) -> u64 {
        self.rom_size.unwrap_or(self.size)
    }
}

/// The pack, read once. A damaged list (it is compiled in, and a test reads it) is an empty pack and a line on stderr.
pub fn pack() -> &'static Pack {
    static PACK: OnceLock<Pack> = OnceLock::new();
    PACK.get_or_init(|| {
        serde_json::from_str(DATA).unwrap_or_else(|e| {
            eprintln!("starter pack: the list is damaged: {e}");
            Pack { format: 0, checked: String::new(), games: Vec::new() }
        })
    })
}

pub fn games() -> &'static [Game] {
    &pack().games
}

/// The starter game that `file_name` in the folder of `system` is (the game card shows its author and licence).
pub fn find(system: &str, file_name: &str) -> Option<&'static Game> {
    games().iter().find(|g| g.system == system && g.file.name_on_disk.eq_ignore_ascii_case(file_name))
}

/// "12 KB", "3.4 MB".
pub fn size_label(bytes: u64) -> String {
    match bytes {
        0..=1023 => format!("{bytes} bytes"),
        1024..=1_048_575 => format!("{} KB", bytes.div_ceil(1024)),
        _ => format!("{:.1} MB", bytes as f64 / 1_048_576.0),
    }
}

/// Why a game was not added. `message` is what the page says: calm, and what to do about it.
#[derive(Clone, Debug, PartialEq)]
pub enum Failure {
    /// No network: nothing was tried further.
    Offline,
    /// The card has too little room (bytes free, bytes needed).
    NoSpace { free: u64, need: u64 },
    /// The download broke off (or the server did not have the file).
    Broke(String),
    /// The file's SHA-256 (or size) is not the one the list names.
    Damaged,
    /// The zip did not hold a ROM that matches.
    BadArchive,
    /// The card refused the write.
    Disk(String),
}

impl Failure {
    pub fn message(&self) -> String {
        match self {
            Failure::Offline => "The Pocket isn't online. Connect to Wi-Fi in Settings, then try again.".into(),
            Failure::NoSpace { free, need } => format!("Not enough space on the SD card: {} MB free, {} MB needed.", free / 1_048_576, need.div_ceil(1_048_576)),
            Failure::Broke(m) => m.clone(),
            Failure::Damaged => "The download is damaged or was changed. Nothing was added.".into(),
            Failure::BadArchive => "The download doesn't contain the game it should. Nothing was added.".into(),
            Failure::Disk(e) => format!("Couldn't save the game: {e}"),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Outcome {
    Added(PathBuf),
    /// A file with that name is on the card already; it was left alone.
    AlreadyThere(PathBuf),
}

/// What `install` needs from outside, so a test can stand in for the card and the network.
pub struct Env<'a> {
    /// The system's folder, such as `/storage/roms/gb`.
    pub dest_dir: &'a Path,
    /// A folder for the download in progress (removed again).
    pub work_dir: &'a Path,
    /// Bytes free where `dest_dir` and `work_dir` are (None: unknown, then the game is tried).
    pub free_dest: Option<u64>,
    pub free_work: Option<u64>,
    /// Whether the Pocket has a network address at all (a cheap early "offline"; the download tells the rest).
    pub online: bool,
}

/// The name in `dir` that equals `name` apart from its case, if there is one (a card may be case-insensitive).
pub fn on_card(dir: &Path, name: &str) -> Option<PathBuf> {
    std::fs::read_dir(dir).ok()?.flatten().find(|e| e.file_name().to_string_lossy().eq_ignore_ascii_case(name)).map(|e| e.path())
}

/// Bytes available to the player on the file system holding `path` (or the nearest folder above it that exists).
pub fn free_bytes(path: &Path) -> Option<u64> {
    let mut at = Some(path);
    while let Some(p) = at {
        if p.exists() {
            let c = std::ffi::CString::new(p.as_os_str().as_encoded_bytes()).ok()?;
            // SAFETY: statvfs only writes into the struct we pass.
            let mut st: libc::statvfs = unsafe { std::mem::zeroed() };
            return (unsafe { libc::statvfs(c.as_ptr(), &mut st) } == 0).then(|| st.f_bavail as u64 * st.f_frsize as u64);
        }
        at = p.parent();
    }
    None
}

/// The download function `install` calls: url, where to put it, the most it may grow to, progress in bytes.
pub type Fetch<'a> = &'a dyn Fn(&str, &Path, Option<u64>, &dyn Fn(u64)) -> Result<(), DownloadFailure>;

/// The Store's curl (`store::download_with_cause`).
pub fn curl(url: &str, dest: &Path, max: Option<u64>, progress: &dyn Fn(u64)) -> Result<(), DownloadFailure> {
    store::download_with_cause(url, dest, max, progress)
}

/// Removes the download in progress when it goes out of scope, whatever happened.
struct Work(PathBuf);

impl Drop for Work {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Adds one game. See the module comment for the order; nothing is written into `dest_dir` before every check passed.
pub fn install(game: &Game, env: &Env, fetch: Fetch, progress: &dyn Fn(u64)) -> Result<Outcome, Failure> {
    let f = &game.file;
    if let Some(p) = on_card(env.dest_dir, &f.name_on_disk) {
        return Ok(Outcome::AlreadyThere(p));
    }
    if !env.online {
        return Err(Failure::Offline);
    }
    if f.size > MAX_FILE || f.rom_bytes() > MAX_FILE {
        return Err(Failure::Damaged);
    }
    // The download and the ROM may both exist for a moment; the ROM is the last to stay.
    for (free, need) in [(env.free_work, f.size + SLACK), (env.free_dest, f.rom_bytes() + SLACK)] {
        if let Some(free) = free.filter(|free| *free < need) {
            return Err(Failure::NoSpace { free, need });
        }
    }
    std::fs::create_dir_all(env.work_dir).map_err(|e| Failure::Disk(e.to_string()))?;
    let work = Work(env.work_dir.join(format!("{}.download", game.id)));
    fetch(&f.url, &work.0, Some(f.size), progress).map_err(|e| match e {
        DownloadFailure::Unreachable => Failure::Offline,
        DownloadFailure::Broke(m) => Failure::Broke(m),
    })?;
    let got = std::fs::metadata(&work.0).map_err(|e| Failure::Disk(e.to_string()))?.len();
    let hash = store::sha256_file(&work.0).map_err(|e| Failure::Disk(e.to_string()))?;
    if got != f.size || !hash.eq_ignore_ascii_case(&f.sha256) {
        return Err(Failure::Damaged);
    }
    let rom = match f.archive.as_deref() {
        None => std::fs::read(&work.0).map_err(|e| Failure::Disk(e.to_string()))?,
        Some("zip") => {
            let ext = Path::new(&f.name_on_disk).extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
            let (_, data) = crate::archive::extract_rom(&work.0, &[ext.as_str()]).map_err(|_| Failure::BadArchive)?;
            let ok = f.rom_size.is_none_or(|n| n == data.len() as u64) && f.rom_sha256.as_deref().is_some_and(|h| store::sha256_hex(&data).eq_ignore_ascii_case(h));
            if !ok {
                return Err(Failure::BadArchive);
            }
            data
        }
        Some(_) => return Err(Failure::BadArchive),
    };
    std::fs::create_dir_all(env.dest_dir).map_err(|e| Failure::Disk(e.to_string()))?;
    // Looked at again: the player may have copied the same file in while it downloaded.
    if let Some(p) = on_card(env.dest_dir, &f.name_on_disk) {
        return Ok(Outcome::AlreadyThere(p));
    }
    let target = env.dest_dir.join(&f.name_on_disk);
    crate::fsutil::write_atomic(&target, &rom).map_err(|e| Failure::Disk(e.to_string()))?;
    Ok(Outcome::Added(target))
}

/// Which games of the pack are on the card: for each system, its folder (None: the shelf has no such system), and the
/// ids of the games whose file is there.
pub fn scan(folders: &BTreeMap<String, PathBuf>) -> BTreeMap<String, bool> {
    games().iter().filter_map(|g| folders.get(&g.system).map(|d| (g.id.clone(), on_card(d, &g.file.name_on_disk).is_some()))).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;
    use std::cell::Cell;

    fn temp(name: &str) -> crate::testutil::TestDir {
        TestDir::new(&format!("starter-{name}"))
    }

    fn sha(bytes: &[u8]) -> String {
        store::sha256_hex(bytes)
    }

    /// A game whose file is `bytes`.
    fn game_of(bytes: &[u8]) -> Game {
        Game {
            id: "test".into(),
            system: "gb".into(),
            title: "Test".into(),
            author: "Someone".into(),
            year: 2020,
            description: "d".into(),
            license: "MIT".into(),
            license_short: "MIT".into(),
            license_url: "https://x/license".into(),
            license_text: "MIT".into(),
            source_url: "https://x".into(),
            homepage: "https://x".into(),
            version: "1".into(),
            mirror: None,
            file: File { url: "https://x/t.gb".into(), sha256: sha(bytes), size: bytes.len() as u64, name_on_disk: "Test.gb".into(), archive: None, rom_sha256: None, rom_size: None },
        }
    }

    fn env<'a>(dest: &'a Path, work: &'a Path) -> Env<'a> {
        Env { dest_dir: dest, work_dir: work, free_dest: None, free_work: None, online: true }
    }

    type Network = Box<dyn Fn(&str, &Path, Option<u64>, &dyn Fn(u64)) -> Result<(), DownloadFailure>>;

    /// A "network" that writes `bytes`.
    fn serve(bytes: Vec<u8>) -> Network {
        Box::new(move |_, dest, _, progress| {
            std::fs::write(dest, &bytes).unwrap();
            progress(bytes.len() as u64);
            Ok(())
        })
    }

    fn names(dir: &Path) -> Vec<String> {
        let mut v: Vec<String> = std::fs::read_dir(dir).map(|d| d.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect()).unwrap_or_default();
        v.sort();
        v
    }

    #[test]
    fn the_list_is_complete_and_every_game_is_free_to_share() {
        let p = pack();
        assert_eq!(p.format, 1);
        assert!(p.checked.len() == 10, "the day the licences were read: {}", p.checked);
        assert!(p.games.len() >= 6, "a pack of at least six games");
        let systems: std::collections::BTreeSet<&str> = p.games.iter().map(|g| g.system.as_str()).collect();
        assert!(systems.len() >= 5, "varied systems: {systems:?}");
        let mut ids = std::collections::BTreeSet::new();
        let mut files = std::collections::BTreeSet::new();
        for g in &p.games {
            assert!(ids.insert(&g.id), "id twice: {}", g.id);
            assert!(files.insert((&g.system, g.file.name_on_disk.to_lowercase())), "file twice: {}", g.file.name_on_disk);
            assert!(g.id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'), "{}: id", g.id);
            assert!(["gb", "gbc", "gba", "nes", "snes", "megadrive"].contains(&g.system.as_str()), "{}: system {}", g.id, g.system);
            for (what, v) in [("title", &g.title), ("author", &g.author), ("description", &g.description), ("license", &g.license), ("license_short", &g.license_short), ("license_text", &g.license_text), ("version", &g.version)] {
                assert!(!v.trim().is_empty(), "{}: {what} is empty", g.id);
            }
            // The licence is a known free one, named in the licence text the author published.
            let free = ["MIT", "GPL-3.0", "zlib", "CC BY", "CC0", "Apache-2.0", "BSD", "Unlicense", "MPL-2.0"];
            assert!(free.iter().any(|l| g.license.contains(l)), "{}: licence {} is not one of the free ones", g.id, g.license);
            assert!(!g.license.to_lowercase().contains("noncommercial") && !g.license.contains("NC") && !g.license.contains("ND"), "{}: NonCommercial/NoDerivatives licences are not shareable here", g.id);
            assert!(!g.license_text.contains("NonCommercial") && !g.license_text.to_lowercase().contains("non-commercial"), "{}: licence text", g.id);
            for (what, u) in [("license_url", &g.license_url), ("source_url", &g.source_url), ("homepage", &g.homepage), ("file url", &g.file.url)] {
                assert!(u.starts_with("https://"), "{}: {what} {u}", g.id);
            }
            assert!((2000..=2030).contains(&g.year), "{}: year", g.id);
            let f = &g.file;
            assert_eq!(f.sha256.len(), 64, "{}: sha256", g.id);
            assert!(f.sha256.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()), "{}: sha256 is lower-case hex", g.id);
            assert!(f.size > 0 && f.size <= MAX_FILE, "{}: size", g.id);
            assert!(!f.name_on_disk.contains('/') && !f.name_on_disk.starts_with('.'), "{}: name_on_disk", g.id);
            match f.archive.as_deref() {
                None => assert!(f.rom_sha256.is_none() && f.rom_size.is_none(), "{}: a plain file has no inner ROM", g.id),
                Some("zip") => {
                    assert!(f.rom_sha256.as_deref().is_some_and(|h| h.len() == 64), "{}: rom_sha256 for the zip", g.id);
                    assert!(f.rom_size.is_some_and(|n| n > 0 && n <= MAX_FILE), "{}: rom_size", g.id);
                }
                Some(other) => panic!("{}: archive {other}", g.id),
            }
            // A URL that can change under us would break the hash: no branch names in file URLs.
            assert!(!f.url.contains("/master/") && !f.url.contains("/main/") && !f.url.contains("/latest/"), "{}: {} is not a fixed address", g.id, f.url);
        }
    }

    #[test]
    fn the_card_finds_its_author_and_licence() {
        let g = find("gb", "tobu tobu girl.GB").expect("case does not matter");
        assert_eq!(g.id, "tobu-tobu-girl");
        assert!(find("gba", "Tobu Tobu Girl.gb").is_none(), "the system has to be the right one");
        assert!(find("gb", "Tetris.gb").is_none());
    }

    #[test]
    fn a_good_download_is_placed_and_the_work_file_is_gone() {
        let (dest, work) = (temp("ok-dest"), temp("ok-work"));
        let bytes = b"ROM-bytes-of-a-test-game".to_vec();
        let g = game_of(&bytes);
        let seen = Cell::new(0);
        let out = install(&g, &env(&dest, &work), &serve(bytes.clone()), &|n| seen.set(n)).unwrap();
        assert_eq!(out, Outcome::Added(dest.join("Test.gb")));
        assert_eq!(std::fs::read(dest.join("Test.gb")).unwrap(), bytes);
        assert_eq!(seen.get(), bytes.len() as u64, "progress was reported");
        assert!(names(&work).is_empty(), "the download is removed: {:?}", names(&work));
        assert_eq!(names(&dest), ["Test.gb"], "no temporary file is left in the system's folder");
    }

    #[test]
    fn a_download_with_the_wrong_hash_leaves_nothing() {
        let (dest, work) = (temp("bad-dest"), temp("bad-work"));
        let g = game_of(b"the real game");
        // Same size, other bytes: only the SHA-256 tells.
        let out = install(&g, &env(&dest, &work), &serve(b"the fake game".to_vec()), &|_| {});
        assert_eq!(out, Err(Failure::Damaged));
        assert!(names(&dest).is_empty(), "nothing in the system's folder: {:?}", names(&dest));
        assert!(names(&work).is_empty(), "the download is removed: {:?}", names(&work));
        // A short download is refused too.
        let out = install(&g, &env(&dest, &work), &serve(b"short".to_vec()), &|_| {});
        assert_eq!(out, Err(Failure::Damaged));
        assert!(names(&dest).is_empty() && names(&work).is_empty());
        assert!(Failure::Damaged.message().contains("Nothing was added"));
    }

    #[test]
    fn a_file_that_is_there_is_never_overwritten() {
        let (dest, work) = (temp("keep-dest"), temp("keep-work"));
        let g = game_of(b"new bytes");
        // The player's own file, with another case, and with other content.
        std::fs::write(dest.join("TEST.GB"), b"mine").unwrap();
        let called = Cell::new(false);
        let net = |_: &str, _: &Path, _: Option<u64>, _: &dyn Fn(u64)| {
            called.set(true);
            Ok(())
        };
        let out = install(&g, &env(&dest, &work), &net, &|_| {}).unwrap();
        assert_eq!(out, Outcome::AlreadyThere(dest.join("TEST.GB")));
        assert!(!called.get(), "nothing is downloaded for a game that is there");
        assert_eq!(std::fs::read(dest.join("TEST.GB")).unwrap(), b"mine");
        assert_eq!(names(&dest), ["TEST.GB"]);
    }

    #[test]
    fn a_full_card_is_refused_before_anything_is_downloaded() {
        let (dest, work) = (temp("full-dest"), temp("full-work"));
        let g = game_of(b"some bytes");
        let called = Cell::new(false);
        let net = |_: &str, _: &Path, _: Option<u64>, _: &dyn Fn(u64)| {
            called.set(true);
            Ok(())
        };
        let mut e = env(&dest, &work);
        e.free_dest = Some(1000);
        let out = install(&g, &e, &net, &|_| {});
        assert!(matches!(out, Err(Failure::NoSpace { free: 1000, .. })), "{out:?}");
        assert!(!called.get());
        assert!(names(&dest).is_empty() && names(&work).is_empty());
        assert!(out.unwrap_err().message().starts_with("Not enough space on the SD card"));
    }

    #[test]
    fn offline_is_said_calmly_and_leaves_nothing() {
        let (dest, work) = (temp("off-dest"), temp("off-work"));
        let g = game_of(b"some bytes");
        // No network address at all.
        let mut e = env(&dest, &work);
        e.online = false;
        assert_eq!(install(&g, &e, &serve(vec![]), &|_| {}), Err(Failure::Offline));
        // An address, but the host can't be reached.
        let down = |_: &str, _: &Path, _: Option<u64>, _: &dyn Fn(u64)| Err(DownloadFailure::Unreachable);
        assert_eq!(install(&g, &env(&dest, &work), &down, &|_| {}), Err(Failure::Offline));
        // A download that broke off keeps curl's words.
        let broke = |_: &str, _: &Path, _: Option<u64>, _: &dyn Fn(u64)| Err(DownloadFailure::Broke("The download didn't finish. Check Wi-Fi and try again.".into()));
        assert_eq!(install(&g, &env(&dest, &work), &broke, &|_| {}), Err(Failure::Broke("The download didn't finish. Check Wi-Fi and try again.".into())));
        assert!(names(&dest).is_empty() && names(&work).is_empty());
        let m = Failure::Offline.message();
        assert!(m.contains("isn't online") && m.contains("Wi-Fi"), "{m}");
    }

    /// A stored zip with one file, enough for `archive::extract_rom`.
    fn zip_of(name: &str, data: &[u8]) -> Vec<u8> {
        let crc = {
            let mut c = !0u32;
            for &b in data {
                c ^= b as u32;
                for _ in 0..8 {
                    c = if c & 1 != 0 { (c >> 1) ^ 0xEDB8_8320 } else { c >> 1 };
                }
            }
            !c
        };
        let mut out = Vec::new();
        out.extend_from_slice(b"PK\x03\x04\x14\0\0\0\0\0\0\0\0\0");
        out.extend_from_slice(&crc.to_le_bytes());
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(&(name.len() as u16).to_le_bytes());
        out.extend_from_slice(&[0, 0]);
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(data);
        let cd_at = out.len() as u32;
        let mut cd = Vec::new();
        cd.extend_from_slice(b"PK\x01\x02\x14\0\x14\0\0\0\0\0\0\0\0\0");
        cd.extend_from_slice(&crc.to_le_bytes());
        cd.extend_from_slice(&(data.len() as u32).to_le_bytes());
        cd.extend_from_slice(&(data.len() as u32).to_le_bytes());
        cd.extend_from_slice(&(name.len() as u16).to_le_bytes());
        cd.extend_from_slice(&[0; 12]);
        cd.extend_from_slice(&0u32.to_le_bytes());
        cd.extend_from_slice(name.as_bytes());
        out.extend_from_slice(&cd);
        out.extend_from_slice(b"PK\x05\x06\0\0\0\0\x01\0\x01\0");
        out.extend_from_slice(&(cd.len() as u32).to_le_bytes());
        out.extend_from_slice(&cd_at.to_le_bytes());
        out.extend_from_slice(&[0, 0]);
        out
    }

    #[test]
    fn a_zip_gives_up_its_rom_and_the_roms_own_hash_is_checked() {
        let (dest, work) = (temp("zip-dest"), temp("zip-work"));
        let rom = vec![7u8; 4096];
        let zip = zip_of("game (v1)/Game (v1).sfc", &rom);
        let mut g = game_of(&zip);
        g.system = "snes".into();
        g.file.name_on_disk = "Game.sfc".into();
        g.file.archive = Some("zip".into());
        g.file.rom_sha256 = Some(sha(&rom));
        g.file.rom_size = Some(rom.len() as u64);
        let out = install(&g, &env(&dest, &work), &serve(zip.clone()), &|_| {}).unwrap();
        assert_eq!(out, Outcome::Added(dest.join("Game.sfc")));
        assert_eq!(std::fs::read(dest.join("Game.sfc")).unwrap(), rom, "the ROM itself, not the zip");
        assert_eq!(names(&dest), ["Game.sfc"]);
        assert!(names(&work).is_empty());
        // A zip with another ROM inside (the zip's own hash is fine, the ROM's is not): nothing is added.
        let _ = std::fs::remove_file(dest.join("Game.sfc"));
        g.file.rom_sha256 = Some(sha(b"another rom"));
        assert_eq!(install(&g, &env(&dest, &work), &serve(zip.clone()), &|_| {}), Err(Failure::BadArchive));
        assert!(names(&dest).is_empty() && names(&work).is_empty());
        // Not a zip at all, but with the hash the list names.
        let junk = b"this is not a zip file".to_vec();
        let mut h = game_of(&junk);
        h.file.archive = Some("zip".into());
        h.file.rom_sha256 = Some(sha(b"x"));
        assert_eq!(install(&h, &env(&dest, &work), &serve(junk), &|_| {}), Err(Failure::BadArchive));
        assert!(names(&dest).is_empty() && names(&work).is_empty());
    }

    /// The Store's real curl, with a file:// address standing for the server.
    #[test]
    fn the_stores_curl_downloads_and_tells_a_missing_network_from_a_broken_download() {
        let (dest, work, src) = (temp("curl-dest"), temp("curl-work"), temp("curl-src"));
        let bytes = vec![42u8; 5000];
        std::fs::write(src.join("g.gb"), &bytes).unwrap();
        let mut g = game_of(&bytes);
        g.file.url = format!("file://{}", src.join("g.gb").display());
        let seen = Cell::new(0);
        let out = install(&g, &env(&dest, &work), &curl, &|n| seen.set(n)).unwrap();
        assert_eq!(out, Outcome::Added(dest.join("Test.gb")));
        assert_eq!(std::fs::read(dest.join("Test.gb")).unwrap(), bytes);
        let _ = std::fs::remove_file(dest.join("Test.gb"));
        // A file that is not there: curl says so, the download "didn't finish".
        g.file.url = format!("file://{}", src.join("nope.gb").display());
        let out = install(&g, &env(&dest, &work), &curl, &|_| {});
        assert!(matches!(out, Err(Failure::Broke(_))), "{out:?}");
        // Nobody listening: no network as far as the game is concerned.
        g.file.url = "http://127.0.0.1:1/g.gb".into();
        assert_eq!(install(&g, &env(&dest, &work), &curl, &|_| {}), Err(Failure::Offline));
        // A host that does not exist.
        g.file.url = "http://starter-pack-test.invalid/g.gb".into();
        assert_eq!(install(&g, &env(&dest, &work), &curl, &|_| {}), Err(Failure::Offline));
        assert!(names(&dest).is_empty() && names(&work).is_empty());
    }

    #[test]
    fn free_bytes_looks_at_the_nearest_folder_that_exists() {
        let d = temp("free");
        assert!(free_bytes(&d).is_some());
        assert!(free_bytes(&d.join("not").join("there")).is_some());
    }

    #[test]
    fn the_scan_tells_which_games_are_on_the_card() {
        let gb = temp("scan-gb");
        let mut folders = BTreeMap::new();
        folders.insert("gb".to_string(), gb.to_path_buf());
        let before = scan(&folders);
        assert_eq!(before.get("tobu-tobu-girl"), Some(&false));
        assert!(!before.contains_key("pipes"), "a system the shelf doesn't have has no entry");
        std::fs::write(gb.join("tobu tobu girl.gb"), b"x").unwrap();
        assert_eq!(scan(&folders).get("tobu-tobu-girl"), Some(&true));
    }
}
