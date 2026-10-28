//! What people bring from EmulationStation: the per-system `gamelist.xml` (ES, ROCKNIX ES, KNULLI/Batocera, ES-DE) with
//! scraped names, descriptions, ratings, release dates, developer, genre, players, favourites and the paths of the
//! pictures. pocket-ui reads them and never writes them (docs/ARCHITECTURE.md "EmulationStation gamelists").
//!
//! * **Where:** `<ROM folder>/gamelist.xml`, then `gamelists/<system>/gamelist.xml` in ES's config folders
//!   (`/storage/.config/emulationstation`, `/storage/.emulationstation`) and in `/storage/ES-DE`. A game found in
//!   several is taken from the first. ES-DE keeps its pictures by convention instead of in the XML
//!   (`ES-DE/downloaded_media/<system>/covers|3dboxes|miximages|screenshots|titlescreens/<file name>.png|jpg`).
//! * **Parsing is tolerant** ([`parse`]): a scanner for `<game>` blocks, not an XML parser, so a file with a bare `&`
//!   in a description, a stray control character, a Latin-1 or UTF-16 encoding, or one that is cut off (the card was
//!   full when ES wrote it) still gives every complete game before the damage. Files over [`MAX_BYTES`] are skipped, a
//!   file gives at most [`MAX_GAMES`] games, texts are cut to [`DESC_MAX`] and [`FACT_MAX`] characters. Nothing here
//!   panics on any input (the tests feed it garbage), and a failure is a line in the log, never a missing library.
//! * **Cached:** every file is parsed once per change (path, mtime, size, ROM folder). The result is kept in memory and
//!   in `<data>/gamelist-cache.json` beside the library cache (`library_cache`), so a start reads one file of what the
//!   games have to say, not megabytes of XML. Everything runs on the scan's threads (`library::load_shown`).
//! * **Used by:** `library::load_shown` for names ([`better_name`]), the App for the game card's description and facts
//!   ([`fill_card`]), covers that pocket-ui has none of ([`Index::cover_candidates`]) and the one-time offer of the
//!   favourites and ratings (`gamelist_page`).

use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use serde::{Deserialize, Serialize};

/// The cache in the data dir.
pub const FILE: &str = "gamelist-cache.json";
/// The cache's format; another one is read as missing. Raise it when `parse` or `Entry` gives other results for the same
/// file. (pocket-ui's own version is not in the key: an update must not make the first start parse every gamelist again
/// before its first frame.)
const VERSION: u32 = 1;
/// A gamelist larger than this is not read (a real one with 10 000 scraped games is about 8 MB).
pub const MAX_BYTES: u64 = 32 * 1024 * 1024;
/// Games taken from one file.
pub const MAX_GAMES: usize = 20_000;
/// Characters of a description kept.
pub const DESC_MAX: usize = 1500;
/// Characters of a name, developer, genre, player count kept.
pub const FACT_MAX: usize = 100;
/// `1.0` in ES's `<rating>` is five stars; kept as 0 to 100.
pub const RATING_FULL: u8 = 100;
/// Pictures of a game kept from one entry.
const IMAGES_MAX: usize = 3;
/// `<tag>`s that name a picture, best box art first: `cover` and `boxart` (ES-DE, Batocera), `thumbnail` (the box in
/// Batocera, a small `image` in ES), `image` (a screenshot or the box). Wide logos (`marquee`) and `video` are no cover.
const IMAGE_TAGS: [&str; 4] = ["cover", "boxart", "thumbnail", "image"];

/// One `<game>` of a gamelist.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    /// The ROM file, absolute (`./Sonic.zip` is made `<ROM folder>/Sonic.zip`, `..` and `.` resolved on paper).
    pub path: PathBuf,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub desc: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub developer: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub publisher: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub genre: String,
    /// "1-2", "4": as written.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub players: String,
    /// The release year; 0 when there is none.
    #[serde(default, skip_serializing_if = "is_zero_u16")]
    pub year: u16,
    /// ES's rating 0.0 to 1.0 as 0 to [`RATING_FULL`]; 0 when there is none (a game ES rated 0 has none either).
    #[serde(default, skip_serializing_if = "is_zero_u8")]
    pub rating: u8,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub favorite: bool,
    /// Picture files, best cover first, as the XML names them (not checked to be there: they may still be copying).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub images: Vec<PathBuf>,
}

fn is_zero_u16(n: &u16) -> bool {
    *n == 0
}

fn is_zero_u8(n: &u8) -> bool {
    *n == 0
}

impl Entry {
    /// ES's rating as the player's 1 to 5 stars (0.1 is one star, 1.0 five); 0 when ES has none.
    pub fn stars(&self) -> u8 {
        if self.rating == 0 {
            return 0;
        }
        ((self.rating as u32 * 5 + 50) / 100).clamp(1, 5) as u8
    }
}

/// Where the gamelists are looked for. Only the device's places exist outside tests; `POCKETUI_ES_HOME` moves them
/// (`/storage` by default) for the virtual device and the snapshots.
#[derive(Clone, Debug, PartialEq)]
pub struct Locations {
    /// ES's config folders; a system's list is `<dir>/gamelists/<system>/gamelist.xml`.
    pub es_dirs: Vec<PathBuf>,
    /// ES-DE's folder: `gamelists/<system>/gamelist.xml` and `downloaded_media/<system>/…`.
    pub es_de: PathBuf,
}

impl Locations {
    pub fn device() -> Locations {
        let root = std::env::var_os("POCKETUI_ES_HOME").map(PathBuf::from).filter(|p| p.is_absolute()).unwrap_or_else(|| PathBuf::from("/storage"));
        Locations { es_dirs: vec![root.join(".config/emulationstation"), root.join(".emulationstation")], es_de: root.join("ES-DE") }
    }

    /// Nothing is looked for outside the ROM folders (tests).
    pub fn none() -> Locations {
        Locations { es_dirs: Vec::new(), es_de: PathBuf::new() }
    }

    /// The gamelist files of a system, the first one wins for a game.
    fn files(&self, system: &str, rom_dir: &Path) -> Vec<PathBuf> {
        let mut v = vec![rom_dir.join("gamelist.xml")];
        v.extend(self.es_dirs.iter().map(|d| d.join("gamelists").join(system).join("gamelist.xml")));
        if !self.es_de.as_os_str().is_empty() {
            v.push(self.es_de.join("gamelists").join(system).join("gamelist.xml"));
        }
        v
    }

    /// ES-DE's picture folder of the system, when it is there.
    fn media(&self, system: &str) -> Option<PathBuf> {
        if self.es_de.as_os_str().is_empty() {
            return None;
        }
        let dir = self.es_de.join("downloaded_media").join(system);
        dir.is_dir().then_some(dir)
    }
}

// ---- parsing -------------------------------------------------------------------------------------------------

/// The text of a file as `&str`: UTF-8 (with or without a BOM), UTF-16 with a BOM, else Windows-1252 (a Latin-1 file
/// that declares itself so, or a hand-edited one). Characters XML 1.0 does not allow are dropped.
pub fn decode(bytes: &[u8]) -> String {
    let text: String = if let Some(b) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        char::decode_utf16(b.as_chunks::<2>().0.iter().map(|p| u16::from_le_bytes(*p))).map(|c| c.unwrap_or('\u{FFFD}')).collect()
    } else if let Some(b) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        char::decode_utf16(b.as_chunks::<2>().0.iter().map(|p| u16::from_be_bytes(*p))).map(|c| c.unwrap_or('\u{FFFD}')).collect()
    } else {
        let b = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
        match std::str::from_utf8(b) {
            Ok(s) => s.to_string(),
            Err(_) => b.iter().map(|&x| cp1252(x)).collect(),
        }
    };
    if text.chars().all(allowed) {
        text
    } else {
        text.chars().filter(|&c| allowed(c)).collect()
    }
}

fn allowed(c: char) -> bool {
    !(c < ' ' && !matches!(c, '\t' | '\n' | '\r')) && c != '\u{FFFE}' && c != '\u{FFFF}'
}

/// Windows-1252: Latin-1, with the printable characters in 0x80 to 0x9F.
fn cp1252(b: u8) -> char {
    const HIGH: [char; 32] = [
        '€', '\u{FFFD}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{FFFD}', 'Ž', '\u{FFFD}', '\u{FFFD}', '‘', '’', '“', '”', '•', '–', '—', '˜', '™', 'š', '›', 'œ',
        '\u{FFFD}', 'ž', 'Ÿ',
    ];
    match b {
        0x80..=0x9F => HIGH[(b - 0x80) as usize],
        _ => b as char,
    }
}

/// The text between `<tag>` and `</tag>` in `block` (the first such pair), entities and CDATA resolved. None when the tag is
/// not there or not closed.
fn field(block: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let start = block.find(&open)? + open.len();
    let end = start + block[start..].find(&format!("</{tag}>"))?;
    Some(unescape(&block[start..end]).into_owned())
}

/// `&amp; &lt; &gt; &quot; &apos; &#233; &#xE9;` and `<![CDATA[…]]>` resolved; what is not an entity (a bare `&`) stays.
fn unescape(s: &str) -> Cow<'_, str> {
    if !s.contains('&') && !s.contains("<![CDATA[") {
        return Cow::Borrowed(s);
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find(['&', '<']) {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        if let Some(inner) = rest.strip_prefix("<![CDATA[") {
            match inner.find("]]>") {
                Some(end) => {
                    out.push_str(&inner[..end]);
                    rest = &inner[end + 3..];
                }
                None => {
                    out.push_str(inner);
                    rest = "";
                }
            }
        } else if rest.starts_with('&') {
            let entity = rest[1..].find(';').filter(|&n| n <= 10).map(|n| &rest[1..1 + n]);
            let decoded = entity.and_then(|e| match e {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                _ => {
                    let num = e.strip_prefix('#')?;
                    let code = match num.strip_prefix(['x', 'X']) {
                        Some(hex) => u32::from_str_radix(hex, 16).ok()?,
                        None => num.parse().ok()?,
                    };
                    char::from_u32(code).filter(|&c| allowed(c))
                }
            });
            match (decoded, entity) {
                (Some(c), Some(e)) => {
                    out.push(c);
                    rest = &rest[e.len() + 2..];
                }
                _ => {
                    out.push('&');
                    rest = &rest[1..];
                }
            }
        } else {
            out.push('<');
            rest = &rest[1..];
        }
    }
    out.push_str(rest);
    Cow::Owned(out)
}

/// One line of text: whitespace runs (the line breaks of a description included) become one space, at most `max`
/// characters, cut at a word with "…".
fn tidy(s: &str, max: usize) -> String {
    let one = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if one.chars().count() <= max {
        return one;
    }
    let cut: String = one.chars().take(max.saturating_sub(1)).collect();
    let cut = match cut.rfind(' ') {
        Some(i) if i > max / 2 => cut[..i].to_string(),
        _ => cut,
    };
    format!("{}…", cut.trim_end_matches(|c: char| c.is_whitespace() || ",;:-".contains(c)))
}

/// `path` with `.` and `..` resolved on paper (no file is looked at, so a missing one and a link are no different).
fn lexical(path: &Path) -> PathBuf {
    use std::path::Component;
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    out.push("..");
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// A path as a gamelist writes it, made absolute against the system's ROM folder. `~/…` (ES's home) is not understood.
fn resolve(text: &str, rom_dir: &Path) -> Option<PathBuf> {
    let t = text.trim();
    if t.is_empty() || t.contains('\0') || t.starts_with('~') {
        return None;
    }
    let p = Path::new(t);
    Some(lexical(&if p.is_absolute() { p.to_path_buf() } else { rom_dir.join(p) }))
}

/// "19910623T000000", "1991-06-23", "1991": the year, when it is a plausible one.
fn year(s: &str) -> u16 {
    s.trim().get(..4).and_then(|y| y.parse::<u16>().ok()).filter(|y| (1950..=2100).contains(y)).unwrap_or(0)
}

/// "0.8" -> 80; ES's scale is 0 to 1. Anything else is no rating.
fn rating(s: &str) -> u8 {
    s.trim().parse::<f32>().ok().filter(|r| r.is_finite() && (0.0..=1.0).contains(r)).map_or(0, |r| (r * RATING_FULL as f32).round() as u8)
}

/// The `<game>` entries of a gamelist's text. Never fails: what can't be read is left out. A game without a path is no
/// game; the last one of a file that ends inside a `<game>` is left out as it is probably incomplete.
pub fn parse(text: &str, rom_dir: &Path) -> Vec<Entry> {
    let mut out = Vec::new();
    let mut rest = text;
    while out.len() < MAX_GAMES {
        let Some(at) = rest.find("<game") else { break };
        let after = &rest[at + "<game".len()..];
        // `<gameList>` and `<game_x>` are not a game; `<game/>` has nothing in it.
        let Some(first) = after.chars().next() else { break };
        if !(first.is_whitespace() || first == '>') {
            rest = after;
            continue;
        }
        let Some(open_end) = after.find('>') else { break };
        if after[..open_end].ends_with('/') {
            rest = &after[open_end + 1..];
            continue;
        }
        let body = &after[open_end + 1..];
        let Some(close) = body.find("</game>") else { break };
        rest = &body[close + "</game>".len()..];
        if let Some(e) = entry(&body[..close], rom_dir) {
            out.push(e);
        }
    }
    out
}

fn entry(block: &str, rom_dir: &Path) -> Option<Entry> {
    let path = resolve(&field(block, "path")?, rom_dir)?;
    let get = |tag: &str, max: usize| field(block, tag).map(|v| tidy(&v, max)).unwrap_or_default();
    let mut images: Vec<PathBuf> = Vec::new();
    for tag in IMAGE_TAGS {
        if let Some(p) = field(block, tag).and_then(|v| resolve(&v, rom_dir)) {
            if !images.contains(&p) && images.len() < IMAGES_MAX {
                images.push(p);
            }
        }
    }
    Some(Entry {
        path,
        name: get("name", FACT_MAX),
        desc: get("desc", DESC_MAX),
        developer: get("developer", FACT_MAX),
        publisher: get("publisher", FACT_MAX),
        genre: get("genre", FACT_MAX),
        players: get("players", 12),
        year: field(block, "releasedate").map_or(0, |d| year(&d)),
        rating: field(block, "rating").map_or(0, |r| rating(&r)),
        favorite: field(block, "favorite").is_some_and(|f| matches!(f.trim().to_ascii_lowercase().as_str(), "true" | "1" | "yes")),
        images,
    })
}

// ---- names ---------------------------------------------------------------------------------------------------

/// A file name that says little: one word (no space) that has a digit, or no capital, or no small letter, like
/// "sf2ce", "kof98", "1943", "DKC2". "Tetris" or "Super Mario Land" are names already.
fn cryptic(name: &str) -> bool {
    !name.chars().any(char::is_whitespace)
        && (name.chars().any(|c| c.is_ascii_digit()) || !name.chars().any(char::is_uppercase) || !name.chars().any(char::is_lowercase))
}

/// The name the gamelist gives a game, when it should replace `cleaned` (pocket-ui's name made from the file name):
/// only for a cryptic file name ([`cryptic`]), and only when the gamelist's name is another name once case, spaces,
/// punctuation and the (…) […] groups are ignored ("Tetris (World)" is no better than "Tetris"). A file name that
/// already reads as a name keeps pocket-ui's cleaned name: its regions and versions are the same for every list, and
/// `tell_apart` relies on them for games that share a name.
pub fn better_name(cleaned: &str, gamelist: &str) -> Option<String> {
    let name = tidy(gamelist, FACT_MAX);
    let key = crate::covers::normalize(&name);
    (!key.is_empty() && cryptic(cleaned) && key != crate::covers::normalize(cleaned)).then_some(name)
}

// ---- reading files, the memo and the cache ---------------------------------------------------------------------

/// What identifies a file's content for the cache.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Sig {
    mtime: i64,
    mtime_ns: i64,
    len: u64,
    rom_dir: PathBuf,
}

impl Sig {
    fn of(file: &Path, rom_dir: &Path) -> Option<Sig> {
        use std::os::unix::fs::MetadataExt;
        let m = std::fs::metadata(file).ok().filter(|m| m.is_file())?;
        Some(Sig { mtime: m.mtime(), mtime_ns: m.mtime_nsec(), len: m.len(), rom_dir: rom_dir.to_path_buf() })
    }
}

#[derive(Clone)]
struct Parsed {
    sig: Sig,
    entries: Arc<[Arc<Entry>]>,
}

#[derive(Serialize, Deserialize)]
struct DiskFile {
    file: PathBuf,
    sig: Sig,
    entries: Vec<Entry>,
}

#[derive(Serialize, Deserialize)]
struct Disk {
    version: u32,
    files: Vec<DiskFile>,
}

static MEMO: Mutex<BTreeMap<PathBuf, Parsed>> = Mutex::new(BTreeMap::new());
static LOADED: OnceLock<()> = OnceLock::new();
static DIRTY: AtomicBool = AtomicBool::new(false);
static WRITE: Mutex<()> = Mutex::new(());

fn memo() -> std::sync::MutexGuard<'static, BTreeMap<PathBuf, Parsed>> {
    MEMO.lock().unwrap_or_else(|e| e.into_inner())
}

/// The disk cache into the memo, once per run. A damaged, foreign or older file is ignored.
fn load_disk(data_dir: &Path) {
    LOADED.get_or_init(|| {
        let Ok(bytes) = std::fs::read(data_dir.join(FILE)) else { return };
        match serde_json::from_slice::<Disk>(&bytes) {
            Ok(d) if d.version == VERSION => {
                let mut m = memo();
                for f in d.files {
                    let entries: Arc<[Arc<Entry>]> = f.entries.into_iter().map(Arc::new).collect();
                    m.entry(f.file).or_insert(Parsed { sig: f.sig, entries });
                }
            }
            Ok(_) => println!("gamelist: cache of another format, read again"),
            Err(e) => println!("gamelist: cache not used: damaged ({e})"),
        }
    });
}

/// The memo as the cache file: only files that are still there are kept.
fn disk_bytes() -> std::io::Result<Vec<u8>> {
    let files: Vec<DiskFile> = memo()
        .iter()
        .filter(|(f, _)| f.is_file())
        .map(|(f, p)| DiskFile { file: f.clone(), sig: p.sig.clone(), entries: p.entries.iter().map(|e| (**e).clone()).collect() })
        .collect();
    serde_json::to_vec(&Disk { version: VERSION, files }).map_err(std::io::Error::other)
}

/// Writes the cache from the memo now (one writer at a time; the memo is read under the lock, so the last writer holds
/// the newest).
fn write_disk(data_dir: &Path) -> std::io::Result<()> {
    let _one_at_a_time = WRITE.lock().unwrap_or_else(|e| e.into_inner());
    crate::fsutil::write_atomic(&data_dir.join(FILE), &disk_bytes()?)
}

/// The cache file on a thread of its own. Nothing happens when nothing was parsed since the last write.
pub fn flush(data_dir: &Path) {
    if !DIRTY.swap(false, Ordering::SeqCst) {
        return;
    }
    let dir = data_dir.to_path_buf();
    #[cfg(test)]
    WRITERS.fetch_add(1, Ordering::SeqCst);
    let spawned = std::thread::Builder::new().name("gamelist-cache".into()).spawn(move || {
        if let Err(e) = write_disk(&dir) {
            eprintln!("gamelist: cache not written: {e}");
        }
        #[cfg(test)]
        WRITERS.fetch_sub(1, Ordering::SeqCst);
    });
    if spawned.is_err() {
        #[cfg(test)]
        WRITERS.fetch_sub(1, Ordering::SeqCst);
        DIRTY.store(true, Ordering::SeqCst);
    }
}

/// How many cache writers `flush` has started and not finished, for the tests to wait for.
#[cfg(test)]
static WRITERS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Waits for the cache writers (any test's: they are short). `TestDir` does it before it removes its folder, so a late
/// writer doesn't make the folder again.
#[cfg(test)]
pub(crate) fn join_writers() {
    let end = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while WRITERS.load(Ordering::SeqCst) > 0 && std::time::Instant::now() < end {
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}

/// The games of one gamelist file: from the memo when its content is the one seen before, else read and parsed. None
/// when the file isn't there, is too large or can't be read.
fn read_file(file: &Path, rom_dir: &Path) -> Option<Parsed> {
    let sig = Sig::of(file, rom_dir)?;
    if let Some(hit) = memo().get(file).filter(|p| p.sig == sig) {
        return Some(hit.clone());
    }
    if sig.len > MAX_BYTES {
        println!("gamelist: {} is {} MB, not read", file.display(), sig.len >> 20);
        return None;
    }
    let bytes = match std::fs::read(file) {
        Ok(b) => b,
        Err(e) => {
            println!("gamelist: {}: {e}", file.display());
            return None;
        }
    };
    let t0 = std::time::Instant::now();
    let entries: Arc<[Arc<Entry>]> = parse(&decode(&bytes), rom_dir).into_iter().map(Arc::new).collect();
    println!("gamelist: {}: {} games in {:?}", file.display(), entries.len(), t0.elapsed());
    let parsed = Parsed { sig, entries };
    memo().insert(file.to_path_buf(), parsed.clone());
    DIRTY.store(true, Ordering::SeqCst);
    Some(parsed)
}

// ---- a system's games and the index ----------------------------------------------------------------------------

/// What the gamelists say about the games of one system.
#[derive(Clone, Default)]
pub struct SystemData {
    by_path: HashMap<PathBuf, Arc<Entry>>,
    /// ES-DE's picture folder for the system.
    media: Option<PathBuf>,
    /// Gamelist files read.
    pub files: usize,
}

impl SystemData {
    pub fn get(&self, rom: &Path) -> Option<&Arc<Entry>> {
        self.by_path.get(rom)
    }

    pub fn is_empty(&self) -> bool {
        self.by_path.is_empty() && self.media.is_none()
    }
}

/// The gamelists of `system`, whose games are in `rom_dir`. Reads from the memo or the cache where it can.
pub fn system(data_dir: &Path, loc: &Locations, system: &str, rom_dir: &Path) -> SystemData {
    load_disk(data_dir);
    let mut data = SystemData { media: loc.media(system), ..Default::default() };
    for file in loc.files(system, rom_dir) {
        if let Some(p) = read_file(&file, rom_dir) {
            data.files += 1;
            for e in p.entries.iter() {
                data.by_path.entry(e.path.clone()).or_insert_with(|| e.clone());
            }
        }
    }
    data
}

/// Every system's gamelist data, what the App keeps.
#[derive(Clone, Default)]
pub struct Index {
    systems: BTreeMap<String, SystemData>,
}

impl std::fmt::Debug for Index {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Index({} systems, {} files)", self.systems.len(), self.files())
    }
}

impl Index {
    /// Reads the gamelists of every `(system, ROM folder)`. On the scan's thread; writes the cache when something
    /// was parsed.
    pub fn load(data_dir: &Path, loc: &Locations, sources: &[(String, PathBuf)]) -> Index {
        let mut systems = BTreeMap::new();
        for (id, dir) in sources {
            let d = system(data_dir, loc, id, dir);
            if !d.is_empty() {
                systems.insert(id.clone(), d);
            }
        }
        flush(data_dir);
        Index { systems }
    }

    pub fn get(&self, system: &str, rom: &Path) -> Option<&Arc<Entry>> {
        self.systems.get(system)?.get(rom)
    }

    /// Gamelist files read.
    pub fn files(&self) -> usize {
        self.systems.values().map(|s| s.files).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.systems.is_empty()
    }

    /// Whether `other` holds the same (same files, same games), so that nothing on screen needs to change.
    pub fn same_as(&self, other: &Index) -> bool {
        self.systems.len() == other.systems.len()
            && self.systems.iter().zip(&other.systems).all(|((a, x), (b, y))| {
                a == b
                    && x.media == y.media
                    && x.by_path.len() == y.by_path.len()
                    && x.by_path.iter().all(|(p, e)| y.by_path.get(p).is_some_and(|o| Arc::ptr_eq(e, o) || **e == **o))
            })
    }

    /// Picture files that may be the game's box art, best first: the XML's own, then ES-DE's folders by file name. The
    /// caller takes the first that exists; none is checked here (this runs for every game on the shelf).
    pub fn cover_candidates(&self, system: &str, rom: &Path) -> Vec<PathBuf> {
        let Some(sd) = self.systems.get(system) else { return Vec::new() };
        let mut v: Vec<PathBuf> = sd.get(rom).map(|e| e.images.clone()).unwrap_or_default();
        if let (Some(media), Some(stem)) = (&sd.media, rom.file_stem().and_then(|s| s.to_str())) {
            for kind in ["covers", "3dboxes", "miximages", "screenshots", "titlescreens"] {
                for ext in ["png", "jpg"] {
                    v.push(media.join(kind).join(format!("{stem}.{ext}")));
                }
            }
        }
        v
    }

    /// The games of `games` (system id, ROM) the gamelists mark as favourites.
    pub fn favorites<'a>(&self, games: impl Iterator<Item = (&'a str, &'a Path)>) -> Vec<(&'a str, &'a Path)> {
        games.filter(|(s, p)| self.get(s, p).is_some_and(|e| e.favorite)).collect()
    }

    /// The games of `games` the gamelists give stars, with the stars (1 to 5).
    pub fn ratings<'a>(&self, games: impl Iterator<Item = (&'a str, &'a Path)>) -> Vec<(&'a Path, u8)> {
        games.filter_map(|(s, p)| Some((p, self.get(s, p)?.stars())).filter(|(_, n)| *n > 0)).collect()
    }
}

/// A picture over this size is not used as a cover: decoding it while the shelf scrolls would stall it.
pub const COVER_MAX_BYTES: u64 = 1 << 20;

/// The first of `candidates` that is a file of at most [`COVER_MAX_BYTES`] (a few `stat`s; the caller keeps the answer).
pub fn usable_cover(candidates: Vec<PathBuf>) -> Option<PathBuf> {
    candidates.into_iter().find(|p| std::fs::metadata(p).is_ok_and(|m| m.is_file() && m.len() <= COVER_MAX_BYTES))
}

// ---- the game card -------------------------------------------------------------------------------------------

/// Fills what the card's own data (Wikidata, the starter pack) left empty from the gamelist's entry: year, players,
/// Developer, Publisher and Genre, a line with ES's rating when there is no score, and the description when there is
/// no Wikipedia text on the card. Nothing that is there is replaced. `wikipedia`: the card has the article's text.
pub fn fill_card(data: &mut crate::game_card::CardData, e: &Entry, wikipedia: bool) {
    use crate::game_card::Summary;
    if data.year.is_none() && e.year != 0 {
        data.year = Some(e.year);
        data.from_gamelist = true;
    }
    if data.players.is_none() && !e.players.is_empty() {
        data.players = Some(players_text(&e.players));
        data.from_gamelist = true;
    }
    for (label, value) in [("Developer", &e.developer), ("Publisher", &e.publisher), ("Genre", &e.genre)] {
        if !value.is_empty() && !data.facts.iter().any(|(l, _)| *l == label) {
            data.facts.push((label, value.clone()));
            data.from_gamelist = true;
        }
    }
    if data.scores.is_none() && e.stars() > 0 {
        data.scores = Some(format!("{} of 5 in EmulationStation", e.stars()));
        data.from_gamelist = true;
    }
    if !wikipedia && !e.desc.is_empty() && matches!(data.summary, Summary::None | Summary::Unavailable(_) | Summary::Loading) {
        data.summary = Summary::Gamelist(e.desc.clone());
    }
}

/// "2" -> "2 players", "1-4" -> "1–4 players" (the dash `gamedb::players_text` writes, which the German table knows:
/// "1-2 players" stayed English on a German card), "1" -> "1 player".
fn players_text(p: &str) -> String {
    let p = p.trim();
    match p.split_once('-') {
        Some((a, b)) => format!("{}–{} players", a.trim(), b.trim()),
        None if p == "1" => "1 player".into(),
        None => format!("{p} players"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;

    const DIR: &str = "/storage/roms/gba";

    fn games(xml: &str) -> Vec<Entry> {
        parse(xml, Path::new(DIR))
    }

    const GOOD: &str = r#"<?xml version="1.0"?>
<gameList>
  <provider><System>GBA</System></provider>
  <game id="1" source="ScreenScraper">
    <path>./Advance Wars (USA).gba</path>
    <name>Advance Wars</name>
    <desc>Command troops &amp; win.
  Two lines &#8212; and a &lt;tag&gt;.</desc>
    <image>./media/images/Advance Wars (USA).png</image>
    <thumbnail>./media/box/Advance Wars (USA).png</thumbnail>
    <rating>0.8</rating>
    <releasedate>20010921T000000</releasedate>
    <developer>Intelligent Systems</developer>
    <publisher>Nintendo</publisher>
    <genre>Strategy</genre>
    <players>1-4</players>
    <favorite>true</favorite>
    <playcount>12</playcount>
    <lastplayed>20240101T120000</lastplayed>
  </game>
  <folder><path>./sub</path><name>Sub</name></folder>
  <game><path>sf2ce.gba</path><name><![CDATA[Street Fighter II' Champion Edition]]></name></game>
</gameList>"#;

    #[test]
    fn a_normal_gamelist_gives_every_field() {
        let g = games(GOOD);
        assert_eq!(g.len(), 2, "folders are no games");
        let a = &g[0];
        assert_eq!(a.path, Path::new("/storage/roms/gba/Advance Wars (USA).gba"));
        assert_eq!(a.name, "Advance Wars");
        assert_eq!(a.desc, "Command troops & win. Two lines — and a <tag>.");
        assert_eq!((a.developer.as_str(), a.publisher.as_str(), a.genre.as_str(), a.players.as_str()), ("Intelligent Systems", "Nintendo", "Strategy", "1-4"));
        assert_eq!((a.year, a.rating, a.stars(), a.favorite), (2001, 80, 4, true));
        assert_eq!(
            a.images,
            [Path::new("/storage/roms/gba/media/box/Advance Wars (USA).png"), Path::new("/storage/roms/gba/media/images/Advance Wars (USA).png")],
            "the thumbnail (the box in Batocera) before the image"
        );
        assert_eq!(g[1].name, "Street Fighter II' Champion Edition", "CDATA");
        assert_eq!(g[1].path, Path::new("/storage/roms/gba/sf2ce.gba"), "no ./ is relative too");
        assert!(!g[1].favorite && g[1].stars() == 0 && g[1].year == 0);
    }

    #[test]
    fn paths_are_made_absolute_on_paper() {
        let p = |x: &str| games(&format!("<game><path>{x}</path></game>")).first().map(|e| e.path.clone());
        assert_eq!(p("./a/../b.gba"), Some(PathBuf::from("/storage/roms/gba/b.gba")));
        assert_eq!(p("/storage/roms/gba/x.gba"), Some(PathBuf::from("/storage/roms/gba/x.gba")));
        assert_eq!(p("../gb/x.gb"), Some(PathBuf::from("/storage/roms/gb/x.gb")));
        assert_eq!(p("~/roms/x.gba"), None, "ES's home is not understood");
        assert_eq!(p("  "), None);
        assert_eq!(games("<game><name>No path</name></game>"), []);
    }

    #[test]
    fn damaged_files_give_what_is_complete() {
        // A bare & in a description (the usual hand-edit): the strict XML parsers refuse the file, this one doesn't.
        let g = games("<gameList><game><path>./a.gba</path><desc>Tom & Jerry &nbsp; AT&T</desc></game></gameList>");
        assert_eq!(g[0].desc, "Tom & Jerry &nbsp; AT&T");
        // Cut off in the middle of the third game (card full): the two before it are there.
        let cut = format!("{}<game><path>./c.gba</path><name>Cut", "<game><path>./a.gba</path></game><game><path>./b.gba</path></game>");
        assert_eq!(games(&cut).len(), 2);
        // Not XML at all, empty, binary, a tag that never closes, a self-closing game.
        for junk in ["", "hello", "<<<<>>>>", "<game", "<game>", "<game><path>", "<game/><gameList/>", "\u{0}\u{1}\u{2}", "<game><path>./a.gba</game>"] {
            let _ = games(junk);
        }
        assert_eq!(games("<game/><gameList></gameList>"), []);
        // An unclosed <path> does not swallow the next game.
        assert_eq!(games("<game><path>./a.gba</game><game><path>./b.gba</path></game>").len(), 1);
    }

    #[test]
    fn encodings_are_decoded() {
        let xml = "<game><path>./a.gba</path><name>Pokémon – Rubí</name></game>";
        assert_eq!(games(&decode(xml.as_bytes()))[0].name, "Pokémon – Rubí");
        let mut bom = vec![0xEF, 0xBB, 0xBF];
        bom.extend(xml.as_bytes());
        assert_eq!(games(&decode(&bom))[0].name, "Pokémon – Rubí");
        // UTF-16 with a BOM, both orders.
        let le: Vec<u8> = [0xFF, 0xFE].into_iter().chain(xml.encode_utf16().flat_map(u16::to_le_bytes)).collect();
        let be: Vec<u8> = [0xFE, 0xFF].into_iter().chain(xml.encode_utf16().flat_map(u16::to_be_bytes)).collect();
        assert_eq!(games(&decode(&le))[0].name, "Pokémon – Rubí");
        assert_eq!(games(&decode(&be))[0].name, "Pokémon – Rubí");
        // A Windows-1252 file that says ISO-8859-1: é is 0xE9 and the dash 0x96.
        let latin = b"<?xml version=\"1.0\" encoding=\"ISO-8859-1\"?><game><path>./a.gba</path><name>Pok\xE9mon \x96 Rub\xED</name></game>";
        assert_eq!(games(&decode(latin))[0].name, "Pokémon – Rubí");
        // Control characters XML does not allow are dropped, not fatal.
        assert_eq!(games(&decode(b"<game><path>./a.gba</path><name>A\x00\x08B</name></game>"))[0].name, "AB");
        // Garbage bytes never panic.
        let garbage: Vec<u8> = (0..=255u8).cycle().take(5000).collect();
        let _ = games(&decode(&garbage));
    }

    #[test]
    fn long_texts_and_many_games_are_cut() {
        let desc = "word ".repeat(1000);
        let g = games(&format!("<game><path>./a.gba</path><desc>{desc}</desc></game>"));
        assert!(g[0].desc.chars().count() <= DESC_MAX && g[0].desc.ends_with("word…"), "{}", &g[0].desc[g[0].desc.len() - 12..]);
        let many = "<game><path>./a.gba</path></game>".repeat(MAX_GAMES + 50);
        assert_eq!(games(&many).len(), MAX_GAMES);
    }

    #[test]
    fn ratings_years_and_favourites_are_read_strictly() {
        let one = |extra: &str| games(&format!("<game><path>./a.gba</path>{extra}</game>")).remove(0);
        assert_eq!(one("<rating>1</rating>").stars(), 5);
        assert_eq!(one("<rating>0.5</rating>").stars(), 3);
        assert_eq!(one("<rating>0.1</rating>").stars(), 1);
        assert_eq!(one("<rating>0.04</rating>").stars(), 1, "rounds up to the smallest rating that is one");
        assert_eq!(one("<rating>0</rating>").stars(), 0);
        for bad in ["8", "-1", "NaN", "abc", "", "inf"] {
            assert_eq!(one(&format!("<rating>{bad}</rating>")).stars(), 0, "{bad}");
        }
        assert_eq!(one("<releasedate>1991-06-23</releasedate>").year, 1991);
        assert_eq!(one("<releasedate>00000000T000000</releasedate>").year, 0);
        assert_eq!(one("<releasedate>x</releasedate>").year, 0);
        assert!(one("<favorite>True</favorite>").favorite && one("<favorite>1</favorite>").favorite);
        assert!(!one("<favorite>false</favorite>").favorite && !one("<favorite></favorite>").favorite);
    }

    #[test]
    fn a_name_is_taken_only_for_a_cryptic_file_and_only_when_it_says_more() {
        assert_eq!(better_name("sf2ce", "Street Fighter II' Champion Edition").as_deref(), Some("Street Fighter II' Champion Edition"));
        assert_eq!(better_name("kof98", "The King of Fighters '98").as_deref(), Some("The King of Fighters '98"));
        assert_eq!(better_name("1943", "1943: The Battle of Midway").as_deref(), Some("1943: The Battle of Midway"));
        assert_eq!(better_name("MSLUG", "Metal Slug").as_deref(), Some("Metal Slug"));
        // Already a name: pocket-ui's cleaned one stays (regions, versions and tell_apart rely on it).
        assert_eq!(better_name("Tetris", "Tetris (World) (Rev 1)"), None);
        assert_eq!(better_name("Super Mario Land", "Super Mario Land 2"), None);
        assert_eq!(better_name("Sonic", "Sonic the Hedgehog"), None);
        // The same name in other letters, an empty one, one of only punctuation.
        assert_eq!(better_name("sf2", "SF2"), None);
        assert_eq!(better_name("1943", "1943 (USA)"), None);
        assert_eq!(better_name("sf2ce", "  "), None);
        assert_eq!(better_name("sf2ce", "!!!"), None);
        // Line breaks and runs of spaces are one space; very long ones are cut.
        assert_eq!(better_name("kof98", "The King of\n  Fighters").as_deref(), Some("The King of Fighters"));
        assert!(better_name("kof98", &"x".repeat(500)).unwrap().chars().count() <= FACT_MAX);
    }

    struct Tmp(crate::testutil::TestDir);

    impl Tmp {
        fn new(tag: &str) -> Tmp {
            let d = TestDir::new(&format!("gamelist-{tag}"));
            std::fs::create_dir_all(d.join("roms/gba")).unwrap();
            std::fs::create_dir_all(d.join("data")).unwrap();
            Tmp(d)
        }
        fn rom_dir(&self) -> PathBuf {
            self.0.join("roms/gba")
        }
        fn data(&self) -> PathBuf {
            self.0.join("data")
        }
        fn xml(&self, body: &str) {
            std::fs::write(self.rom_dir().join("gamelist.xml"), format!("<gameList>{body}</gameList>")).unwrap();
        }
    }


    #[test]
    fn a_file_is_parsed_once_per_change_and_comes_back_from_the_cache() {
        let t = Tmp::new("memo");
        t.xml("<game><path>./a.gba</path><name>First</name><favorite>true</favorite></game>");
        let src = vec![("gba".to_string(), t.rom_dir())];
        let a = Index::load(&t.data(), &Locations::none(), &src);
        let rom = t.rom_dir().join("a.gba");
        assert_eq!(a.get("gba", &rom).map(|e| e.name.as_str()), Some("First"));
        assert_eq!(a.files(), 1);
        // Same file: the same entries (no new parse: the Arc is the memo's).
        let b = Index::load(&t.data(), &Locations::none(), &src);
        assert!(Arc::ptr_eq(a.get("gba", &rom).unwrap(), b.get("gba", &rom).unwrap()));
        assert!(a.same_as(&b));
        // A change (another size) is read again.
        t.xml("<game><path>./a.gba</path><name>Second one</name></game>");
        let c = Index::load(&t.data(), &Locations::none(), &src);
        assert_eq!(c.get("gba", &rom).map(|e| e.name.as_str()), Some("Second one"));
        assert!(!a.same_as(&c));
        // The disk cache holds the file; a damaged copy of it is ignored, a good one comes back with its entries.
        write_disk(&t.data()).unwrap();
        let disk: Disk = serde_json::from_slice(&std::fs::read(t.data().join(FILE)).unwrap()).unwrap();
        assert_eq!(disk.version, VERSION);
        let mine = disk.files.iter().find(|f| f.file == t.rom_dir().join("gamelist.xml")).unwrap();
        assert_eq!(mine.entries.len(), 1);
        assert_eq!(mine.entries[0].name, "Second one");
    }

    #[test]
    fn missing_huge_and_unreadable_files_are_no_gamelist() {
        let t = Tmp::new("bad");
        let src = vec![("gba".to_string(), t.rom_dir())];
        // No file at all: an empty index, nothing logged as an error.
        let none = Index::load(&t.data(), &Locations::none(), &src);
        assert!(none.is_empty() && none.files() == 0);
        // A folder where the file should be.
        std::fs::create_dir_all(t.rom_dir().join("gamelist.xml")).unwrap();
        assert!(Index::load(&t.data(), &Locations::none(), &src).is_empty());
        std::fs::remove_dir(t.rom_dir().join("gamelist.xml")).unwrap();
        // Garbage, a binary file, an empty file: no games, no panic.
        for body in [&b""[..], b"\x00\x01\x02\xff\xfe", &[0xAB; 100_000][..], b"<gameList><game><path>"] {
            std::fs::write(t.rom_dir().join("gamelist.xml"), body).unwrap();
            let i = Index::load(&t.data(), &Locations::none(), &src);
            assert!(i.get("gba", &t.rom_dir().join("a.gba")).is_none());
        }
        // Over the limit: not read (a sparse file of that size costs nothing).
        let big = std::fs::File::create(t.rom_dir().join("gamelist.xml")).unwrap();
        big.set_len(MAX_BYTES + 1).unwrap();
        assert!(Index::load(&t.data(), &Locations::none(), &src).is_empty());
        // A damaged cache file is ignored.
        std::fs::write(t.data().join(FILE), b"{ not json").unwrap();
        assert!(Index::load(&t.data(), &Locations::none(), &src).is_empty());
    }

    #[test]
    fn es_de_and_es_config_folders_are_read_and_the_first_file_wins() {
        let t = Tmp::new("places");
        let es_de = t.0.join("ES-DE");
        let es = t.0.join("es");
        std::fs::create_dir_all(es_de.join("gamelists/gba")).unwrap();
        std::fs::create_dir_all(es.join("gamelists/gba")).unwrap();
        std::fs::create_dir_all(es_de.join("downloaded_media/gba/covers")).unwrap();
        std::fs::write(es_de.join("gamelists/gba/gamelist.xml"), "<gameList><game><path>./a.gba</path><name>From ES-DE</name><genre>Puzzle</genre></game><game><path>./b.gba</path><name>Only ES-DE</name></game></gameList>").unwrap();
        std::fs::write(es.join("gamelists/gba/gamelist.xml"), "<gameList><game><path>./a.gba</path><name>From ES</name></game></gameList>").unwrap();
        t.xml("<game><path>./a.gba</path><name>From the ROM folder</name></game>");
        let loc = Locations { es_dirs: vec![es], es_de };
        let src = vec![("gba".to_string(), t.rom_dir())];
        let i = Index::load(&t.data(), &loc, &src);
        assert_eq!(i.files(), 3);
        assert_eq!(i.get("gba", &t.rom_dir().join("a.gba")).unwrap().name, "From the ROM folder");
        assert_eq!(i.get("gba", &t.rom_dir().join("b.gba")).unwrap().name, "Only ES-DE");
        // ES-DE's pictures by file name, also for a game without an entry.
        let c = i.cover_candidates("gba", &t.rom_dir().join("zzz.gba"));
        assert_eq!(c[0], t.0.join("ES-DE/downloaded_media/gba/covers/zzz.png"));
        assert!(i.cover_candidates("snes", Path::new("/x.sfc")).is_empty());
    }

    #[test]
    fn a_cover_is_the_first_picture_that_is_there_and_not_too_large() {
        let t = Tmp::new("cover");
        let (small, big, folder) = (t.0.join("small.png"), t.0.join("big.png"), t.0.join("folder.png"));
        std::fs::write(&small, b"x").unwrap();
        std::fs::File::create(&big).unwrap().set_len(COVER_MAX_BYTES + 1).unwrap();
        std::fs::create_dir(&folder).unwrap();
        let missing = t.0.join("missing.png");
        assert_eq!(usable_cover(vec![missing.clone(), folder.clone(), big.clone(), small.clone()]), Some(small.clone()), "not there, a folder and too large are passed over");
        assert_eq!(usable_cover(vec![big, missing, folder]), None);
        assert_eq!(usable_cover(Vec::new()), None);
    }

    #[test]
    fn favourites_and_ratings_are_picked_from_the_library() {
        let t = Tmp::new("fav");
        t.xml(
            "<game><path>./a.gba</path><favorite>true</favorite><rating>0.6</rating></game>\
             <game><path>./b.gba</path><rating>0</rating></game>\
             <game><path>./gone.gba</path><favorite>true</favorite></game>",
        );
        let i = Index::load(&t.data(), &Locations::none(), &[("gba".to_string(), t.rom_dir())]);
        let (a, b) = (t.rom_dir().join("a.gba"), t.rom_dir().join("b.gba"));
        let library = [("gba", a.as_path()), ("gba", b.as_path())];
        assert_eq!(i.favorites(library.iter().copied()), [("gba", a.as_path())], "only games the library has");
        assert_eq!(i.ratings(library.iter().copied()), [(a.as_path(), 3)]);
    }

    fn card() -> crate::game_card::CardData {
        crate::game_card::CardData { title: "Advance Wars".into(), ..Default::default() }
    }

    #[test]
    fn the_card_takes_what_it_lacks_and_keeps_what_it_has() {
        use crate::game_card::Summary;
        let e = games(GOOD).remove(0);
        let mut d = card();
        d.facts.clear();
        d.year = None;
        d.players = None;
        d.scores = None;
        d.summary = Summary::None;
        fill_card(&mut d, &e, false);
        assert_eq!(d.year, Some(2001));
        assert_eq!(d.players.as_deref(), Some("1–4 players"), "the dash the game list and the German table use");
        let labels: Vec<&str> = d.facts.iter().map(|(l, _)| *l).collect();
        assert_eq!(labels, ["Developer", "Publisher", "Genre"]);
        assert_eq!(d.scores.as_deref(), Some("4 of 5 in EmulationStation"));
        assert!(matches!(&d.summary, Summary::Gamelist(t) if t.starts_with("Command troops")));
        // What pocket-ui has stays: its year, developer, score and the Wikipedia text.
        let mut d = card();
        d.facts = vec![("Developer", "Wikidata's".into())];
        d.year = Some(1999);
        d.scores = Some("Metacritic 92 · via Wikidata".into());
        d.summary = Summary::Wikipedia { text: "t".into(), title: "T".into() };
        fill_card(&mut d, &e, true);
        assert_eq!(d.year, Some(1999));
        assert_eq!(d.facts.iter().find(|(l, _)| *l == "Developer").unwrap().1, "Wikidata's");
        assert_eq!(d.scores.as_deref(), Some("Metacritic 92 · via Wikidata"));
        assert!(matches!(d.summary, Summary::Wikipedia { .. }));
        // A Wikipedia text still on its way or failed: the gamelist's description is there meanwhile.
        for s in [Summary::Loading, Summary::Unavailable("offline".into())] {
            let mut d = card();
            d.summary = s;
            fill_card(&mut d, &e, false);
            assert!(matches!(d.summary, Summary::Gamelist(_)));
        }
    }
}
