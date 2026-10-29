//! Games recognised by their checksum, not only by their file name: the CRC-32 of a cartridge game looked up in the
//! No-Intro list of its system gives the game's proper name ("Mega Man & Bass (USA)" for "0902 - MM&B (U)(Venom)"),
//! the name the libretro thumbnail collection files its pictures under (`covers`, `game_pictures`) and the one the
//! offline game list matches best (`gamedb`, Curator). A game the list does not know keeps its file name, as before.
//!
//! **The checksum.** For a zip, the CRC-32 its own directory states (`archive::rom_crc`: nothing is unpacked). For a
//! loose file up to [`MAX_HASH`], the whole file read once, with a second CRC without a copier header (an iNES header,
//! a 512-byte SNES or PC Engine copier header, a Lynx header) or in big-endian order (N64 `.v64`/`.n64`), so either form matches.
//! Disc systems, 7z and bigger files are left to their file name. Done by a thread of the lowest CPU and I/O priority
//! that stops when a game starts ([`run_until`]); the answers are kept in `<data>/identify/checksums.tsv` by path,
//! size and modification time, so each file is read once and again only when it changes.
//!
//! **The lists.** libretro-database's No-Intro DAT of the system (CC BY-SA 4.0, upstream No-Intro), downloaded once
//! when the Pocket is online, kept as `<data>/identify/<system list>.tsv` (CRC, name) and fetched again after
//! [`DAT_MAX_AGE`]. pocket-ui ships none of it (NOTICE.md). Offline and before the first download the
//! checksum waits, and the file name is used.
//!
//! **Discs.** PlayStation 1 and 2 images are too big for a checksum, so their serial is read instead — the `BOOT`
//! line of the ISO 9660 file `SYSTEM.CNF` — and looked up in libretro-database's Redump DAT of the system, which
//! names every release by its serial (CC BY-SA 4.0 as a repository). That is [`crate::disc_serial`], run and cached
//! with the checksums here (`identify/discs.tsv`, `identify/<list>.serials.tsv`); its names come back from [`known`]
//! and [`run_until`] just like the cartridges', so the covers, pictures, facts and Curator need no special case.
//!
//! **Look up again** on the game card ([`forget`]): the game's box art, screenshot and title screen, its lines in
//! the three `.missing` lists and its checksum are dropped, and the app looks it up afresh, checksum first, then name.

use std::collections::HashMap;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// The proper name of each game the lists know, by ROM path.
pub type Names = HashMap<PathBuf, String>;

/// Loose files bigger than this are not read through (big DS games, disc images): seconds of the card each.
pub const MAX_HASH: u64 = 64 << 20;

/// A list older than this is fetched again (No-Intro adds a few releases a month, mostly to newer systems).
pub const DAT_MAX_AGE: Duration = Duration::from_secs(180 * 24 * 3600);

/// The cache is written after this many new checksums too, so a run cut short keeps what it did.
const SAVE_EVERY: usize = 50;

const SOURCE: &str = "https://raw.githubusercontent.com/libretro/libretro-database/master/metadat/no-intro";

/// Cartridge systems with a No-Intro list. Disc systems (Redump) are left to the file name: their images are
/// hundreds of MB, and a CHD or a cue sheet has no checksum of the disc to compare.
const CARTRIDGE: [&str; 26] = [
    "gb", "gbc", "gba", "nes", "famicom", "fds", "snes", "sfc", "n64", "nds", "virtualboy", "megadrive", "genesis", "mastersystem", "gamegear", "sega32x", "pcengine", "tg16",
    "ngp", "ngpc", "atarilynx", "atari2600", "wonderswan", "wonderswancolor", "sg-1000", "sg1000",
];

/// The No-Intro list of a system, named as libretro names it ("Nintendo - Game Boy"); None for disc systems (their
/// serial names them, `disc_serial`) and systems without one.
pub fn dat_name(system: &str) -> Option<&'static str> {
    CARTRIDGE.contains(&system).then(|| crate::covers::playlist(system)).flatten()
}

/// Where libretro-database keeps that list.
pub fn dat_url(dat: &str) -> String {
    format!("{SOURCE}/{}.dat", crate::covers::percent_encode(dat))
}

/// A game's checksums: the file as it is, and without its copier header or in big-endian order when it has one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sums {
    pub crc: u32,
    pub alt: Option<u32>,
}

/// How a loose file differs from the list's form: a header to leave out, or N64 bytes to put in big-endian order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Form {
    Skip(usize),
    /// `.v64`: every two bytes swapped.
    Swap16,
    /// `.n64`: every four bytes reversed.
    Swap32,
}

fn form(system: &str, first: &[u8], len: u64) -> Option<Form> {
    match system {
        "nes" | "famicom" if first.starts_with(b"NES\x1a") => Some(Form::Skip(16)),
        "fds" if first.starts_with(b"FDS\x1a") => Some(Form::Skip(16)),
        "snes" | "sfc" if len % 1024 == 512 => Some(Form::Skip(512)),
        "pcengine" | "tg16" if len % 8192 == 512 => Some(Form::Skip(512)),
        "atarilynx" if first.starts_with(b"LYNX") => Some(Form::Skip(64)),
        "n64" if first.starts_with(&[0x37, 0x80, 0x40, 0x12]) => Some(Form::Swap16),
        "n64" if first.starts_with(&[0x40, 0x12, 0x37, 0x80]) => Some(Form::Swap32),
        _ => None,
    }
}

/// The checksums of a game file, or None when it is not worked out: a disc system, a 7z, a zip of several games, a
/// file over [`MAX_HASH`]. An error is a file that could not be read (tried again next time).
pub fn checksums(system: &str, rom: &Path) -> io::Result<Option<Sums>> {
    if dat_name(system).is_none() {
        return Ok(None);
    }
    let ext = rom.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).unwrap_or_default();
    match ext.as_str() {
        "zip" => return Ok(crate::archive::rom_crc(rom, &[]).map(|(crc, _)| Sums { crc, alt: None })),
        "7z" | "rar" => return Ok(None),
        _ => {}
    }
    let mut f = std::fs::File::open(rom)?;
    let len = f.metadata()?.len();
    if len > MAX_HASH {
        return Ok(None);
    }
    let (mut full, mut alt) = (crc32fast::Hasher::new(), crc32fast::Hasher::new());
    let mut buf = vec![0; 1 << 20];
    let mut shape = None;
    let mut first = true;
    loop {
        let n = read_full(&mut f, &mut buf)?;
        if n == 0 {
            break;
        }
        let chunk = &mut buf[..n];
        full.update(chunk);
        if first {
            shape = form(system, chunk, len);
            first = false;
        }
        match shape {
            Some(Form::Skip(skip)) => alt.update(chunk.get(skip..).unwrap_or_default()),
            Some(Form::Swap16) => {
                chunk.as_chunks_mut::<2>().0.iter_mut().for_each(|p| p.swap(0, 1));
                alt.update(chunk);
            }
            Some(Form::Swap32) => {
                chunk.as_chunks_mut::<4>().0.iter_mut().for_each(|p| p.reverse());
                alt.update(chunk);
            }
            None => {}
        }
        // Skip applies to the start of the file only.
        if let Some(Form::Skip(_)) = shape {
            shape = Some(Form::Skip(0));
        }
    }
    Ok(Some(Sums { crc: full.finalize(), alt: shape.map(|_| alt.finalize()) }))
}

/// Fills `buf` as far as the file goes (whole chunks keep the N64 byte pairs together).
fn read_full(f: &mut std::fs::File, buf: &mut [u8]) -> io::Result<usize> {
    let mut n = 0;
    while n < buf.len() {
        match f.read(&mut buf[n..])? {
            0 => break,
            k => n += k,
        }
    }
    Ok(n)
}

/// What the cache knows of one file.
#[derive(Clone, Debug, PartialEq)]
struct Known {
    system: String,
    size: u64,
    /// Modification time in nanoseconds since 1970.
    mtime: u64,
    /// None: looked at, nothing to compare (too big, several games in a zip).
    sums: Option<Sums>,
}

/// `<data>/identify/checksums.tsv`: one line per file, `system  size  mtime  crc  alt  path` (`-` for none).
#[derive(Default)]
pub struct Cache {
    files: HashMap<PathBuf, Known>,
}

fn dir(data_dir: &Path) -> PathBuf {
    data_dir.join("identify")
}

fn hex(v: Option<u32>) -> String {
    v.map_or("-".into(), |v| format!("{v:08X}"))
}

fn unhex(s: &str) -> Option<u32> {
    u32::from_str_radix(s, 16).ok()
}

/// The size and modification time a cache line is valid for.
fn stamp(rom: &Path) -> io::Result<(u64, u64)> {
    let m = std::fs::metadata(rom)?;
    let mtime = m.modified()?.duration_since(SystemTime::UNIX_EPOCH).map_or(0, |d| d.as_nanos() as u64);
    Ok((m.len(), mtime))
}

impl Cache {
    pub fn load(data_dir: &Path) -> Cache {
        let text = std::fs::read_to_string(dir(data_dir).join("checksums.tsv")).unwrap_or_default();
        let files = text
            .lines()
            .filter_map(|l| {
                let f: Vec<&str> = l.splitn(6, '\t').collect();
                let [system, size, mtime, crc, alt, path] = f[..] else { return None };
                let sums = unhex(crc).map(|crc| Sums { crc, alt: unhex(alt) });
                Some((PathBuf::from(path), Known { system: system.into(), size: size.parse().ok()?, mtime: mtime.parse().ok()?, sums }))
            })
            .collect();
        Cache { files }
    }

    pub fn save(&self, data_dir: &Path) -> io::Result<()> {
        let mut lines: Vec<String> = self
            .files
            .iter()
            .map(|(p, k)| format!("{}\t{}\t{}\t{}\t{}\t{}\n", k.system, k.size, k.mtime, hex(k.sums.map(|s| s.crc)), hex(k.sums.and_then(|s| s.alt)), p.display()))
            .collect();
        lines.sort();
        std::fs::create_dir_all(dir(data_dir))?;
        crate::fsutil::write_atomic(&dir(data_dir).join("checksums.tsv"), lines.concat().as_bytes())
    }

    /// The checksums kept for `rom`, if they are still the file's (same size and time).
    fn valid(&self, rom: &Path, at: (u64, u64)) -> Option<Option<Sums>> {
        self.files.get(rom).filter(|k| (k.size, k.mtime) == at).map(|k| k.sums)
    }

    pub fn len(&self) -> usize {
        self.files.len()
    }

    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }
}

/// The CRC → name table of a No-Intro DAT (clrmamepro format). A checksum several entries share (a re-release of the
/// same ROM) keeps the cleanest name, as the covers rank them.
pub fn parse_dat(text: &str) -> HashMap<u32, String> {
    let mut out: HashMap<u32, String> = HashMap::new();
    let mut game: Option<String> = None;
    for line in text.lines() {
        let line = line.trim_start();
        if line.starts_with("game (") || line.starts_with("game(") {
            game = None;
        } else if let Some(rest) = line.strip_prefix("name \"") {
            if game.is_none() {
                game = rest.split('"').next().map(str::to_string);
            }
        } else if line.starts_with("rom (") || line.starts_with("rom(") {
            let (Some(name), Some(crc)) = (&game, line.split_whitespace().skip_while(|w| *w != "crc").nth(1).and_then(unhex)) else { continue };
            match out.get(&crc) {
                Some(have) if crate::covers::rank(have) <= crate::covers::rank(name) => {}
                _ => {
                    out.insert(crc, name.clone());
                }
            }
        }
    }
    out
}

fn table_path(data_dir: &Path, dat: &str) -> PathBuf {
    dir(data_dir).join(format!("{dat}.tsv"))
}

/// The table of `dat` kept on the card, if any.
pub fn load_table(data_dir: &Path, dat: &str) -> Option<HashMap<u32, String>> {
    let text = std::fs::read_to_string(table_path(data_dir, dat)).ok()?;
    Some(text.lines().filter_map(|l| l.split_once('\t')).filter_map(|(c, n)| Some((unhex(c)?, n.to_string()))).collect())
}

fn store_table(data_dir: &Path, dat: &str, table: &HashMap<u32, String>) -> io::Result<()> {
    let mut lines: Vec<String> = table.iter().map(|(c, n)| format!("{c:08X}\t{n}\n")).collect();
    lines.sort();
    std::fs::create_dir_all(dir(data_dir))?;
    crate::fsutil::write_atomic(&table_path(data_dir, dat), lines.concat().as_bytes())
}

/// Whether the table of `dat` should be fetched (none yet, or older than [`DAT_MAX_AGE`]).
fn table_due(data_dir: &Path, dat: &str) -> bool {
    std::fs::metadata(table_path(data_dir, dat))
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.elapsed().ok())
        .is_none_or(|age| age > DAT_MAX_AGE)
}

/// Downloads a DAT from libretro-database (several MB as text, a fifth of that compressed). None offline. The disc
/// systems' lists are the Redump ones (`disc_serial`), the others the No-Intro ones.
pub fn fetch_dat(dat: &str) -> Option<String> {
    if crate::disc_serial::is_disc_dat(dat) {
        return crate::disc_serial::fetch_dat(dat);
    }
    crate::covers::fetch_text(&dat_url(dat))
}

/// The name of a game with these checksums in `table`.
fn name_in(table: &HashMap<u32, String>, sums: Sums) -> Option<&String> {
    table.get(&sums.crc).or_else(|| sums.alt.and_then(|a| table.get(&a)))
}

/// The names the card already has for `games` (system id, ROM path): the cached checksums in the tables on the card.
/// Nothing is read through or downloaded and no file is looked at, so it is quick enough for any background thread
/// (Curator's catalog, the cover download); a file changed since is caught by the next [`run_until`].
pub fn known(data_dir: &Path, games: &[(String, PathBuf)]) -> Names {
    let mut out = crate::disc_serial::known(data_dir, games);
    let cache = Cache::load(data_dir);
    if cache.is_empty() {
        return out;
    }
    let mut tables: HashMap<&str, Option<HashMap<u32, String>>> = HashMap::new();
    out.extend(games.iter().filter_map(|(sys, rom)| {
        let dat = dat_name(sys)?;
        let sums = cache.files.get(rom)?.sums?;
        let table = tables.entry(dat).or_insert_with(|| load_table(data_dir, dat)).as_ref()?;
        Some((rom.clone(), name_in(table, sums)?.clone()))
    }));
    out
}

/// What a [`run_until`] did.
#[derive(Debug, Default)]
pub struct Identified {
    /// Every game of the run the lists know, by path.
    pub names: Names,
    /// Files read through, whose zip directory was read this time, or whose disc serial was read.
    pub hashed: usize,
    /// A system's list could not be had (offline, a failed download): its games keep their file names until then.
    pub lists_missing: bool,
    /// A game started before the run was through; the rest waits.
    pub stopped: bool,
}

/// Works out the checksums of `games` (system id, ROM path) that the cache doesn't have, gets the lists of their
/// systems (`fetch`: the DAT's text, None offline; called only for a list missing or old) and names every game they
/// know. `stop` is asked before each file. A game named for the first time is taken off the `.missing` lists so the
/// pictures are looked up again under its proper name. `prune`: `games` is the whole library, so cache lines of
/// other files go.
pub fn run_until(data_dir: &Path, games: &[(String, PathBuf)], fetch: &dyn Fn(&str) -> Option<String>, prune: bool, stop: impl Fn() -> bool) -> Identified {
    let mut cache = Cache::load(data_dir);
    let mut out = Identified::default();
    let mut fresh: std::collections::HashSet<&PathBuf> = Default::default();
    let mut unsaved = 0;
    let wanted: Vec<&(String, PathBuf)> = games.iter().filter(|(sys, _)| dat_name(sys).is_some()).collect();
    for game in &wanted {
        let (sys, rom) = (&game.0, &game.1);
        let Ok(at) = stamp(rom) else { continue };
        if cache.valid(rom, at).is_some() {
            continue;
        }
        if stop() {
            out.stopped = true;
            break;
        }
        // A file that can't be read now (card busy, removed) is tried again next time.
        let Ok(sums) = checksums(sys, rom) else { continue };
        cache.files.insert(rom.clone(), Known { system: sys.clone(), size: at.0, mtime: at.1, sums });
        out.hashed += 1;
        fresh.insert(rom);
        unsaved += 1;
        if unsaved >= SAVE_EVERY {
            let _ = cache.save(data_dir);
            unsaved = 0;
        }
    }
    if prune && !out.stopped {
        let all: std::collections::HashSet<&PathBuf> = games.iter().map(|g| &g.1).collect();
        let before = cache.files.len();
        cache.files.retain(|p, _| all.contains(p));
        unsaved += before - cache.files.len();
    }
    if unsaved > 0 {
        let _ = cache.save(data_dir);
    }
    // The lists of the systems the games are on, fetched when missing or old; a list fetched now names its games
    // for the first time.
    let mut tables: HashMap<&str, HashMap<u32, String>> = HashMap::new();
    let mut fetched: Vec<&str> = Vec::new();
    let mut dats: Vec<&str> = wanted.iter().filter_map(|(sys, _)| dat_name(sys)).collect();
    dats.sort_unstable();
    dats.dedup();
    for dat in dats {
        let mut table = load_table(data_dir, dat);
        // Not while a game runs: the download waits for the next run.
        if !out.stopped && table_due(data_dir, dat) && !stop() {
            if let Some(new) = fetch(dat).map(|t| parse_dat(&t)).filter(|t| !t.is_empty()) {
                let _ = store_table(data_dir, dat, &new);
                if table.is_none() {
                    fetched.push(dat);
                }
                table = Some(new);
            }
        }
        match table {
            Some(t) => {
                tables.insert(dat, t);
            }
            None => out.lists_missing = true,
        }
    }
    for game in &wanted {
        let (sys, rom) = (&game.0, &game.1);
        let Some(dat) = dat_name(sys) else { continue };
        let (Some(table), Some(Some(sums))) = (tables.get(dat), cache.files.get(rom).map(|k| k.sums)) else { continue };
        if let Some(name) = name_in(table, sums) {
            out.names.insert(rom.clone(), name.clone());
            if fresh.contains(rom) || fetched.contains(&dat) {
                unlist_everywhere(data_dir, sys, rom);
            }
        }
    }
    // PlayStation discs: their serial, read from the image and looked up in the Redump list (`disc_serial`).
    let discs = crate::disc_serial::run_until(data_dir, games, fetch, prune, stop);
    out.hashed += discs.read;
    out.lists_missing |= discs.lists_missing;
    out.stopped |= discs.stopped;
    out.names.extend(discs.names);
    out
}

/// The three `.missing` lists a game can be on: box art, screenshot, title screen.
fn missing_lists(data_dir: &Path, system: &str) -> [PathBuf; 3] {
    [
        data_dir.join("covers").join(system).join(".missing"),
        data_dir.join(crate::game_pictures::Kind::Snap.folder()).join(system).join(".missing"),
        data_dir.join(crate::game_pictures::Kind::Title.folder()).join(system).join(".missing"),
    ]
}

/// Takes the game off its system's `.missing` lists (they name games by file stem). `pub(crate)` for the disc reader
/// (`disc_serial`).
pub(crate) fn unlist_everywhere(data_dir: &Path, system: &str, rom: &Path) {
    let stem = rom.file_stem().and_then(|s| s.to_str()).unwrap_or_default();
    for file in missing_lists(data_dir, system) {
        let Ok(now) = std::fs::read_to_string(&file) else { continue };
        if now.lines().any(|l| l == stem) {
            let rest: String = now.lines().filter(|l| *l != stem).map(|l| format!("{l}\n")).collect();
            let _ = crate::fsutil::write_atomic(&file, rest.as_bytes());
        }
    }
}

/// "Look up again" on the game card, the part on the card: the game's box art, screenshot and title screen go (a
/// PNG of the player's own under that name too, which the card says), it comes off the `.missing` lists and its
/// checksum is forgotten, so the next [`run_until`] reads it again and the next cover download asks for it afresh.
pub fn forget(data_dir: &Path, system: &str, rom: &Path) {
    let _ = std::fs::remove_file(crate::covers::cover_path(data_dir, system, rom));
    for kind in crate::game_pictures::Kind::ALL {
        let _ = std::fs::remove_file(crate::game_pictures::path(data_dir, kind, system, rom));
    }
    unlist_everywhere(data_dir, system, rom);
    let mut cache = Cache::load(data_dir);
    if cache.files.remove(rom).is_some() {
        let _ = cache.save(data_dir);
    }
    crate::disc_serial::forget(data_dir, rom);
}

/// How "Look up again" came out, for the line the card shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Again {
    /// It runs.
    Working,
    /// Not online: nothing was changed.
    Offline,
    /// The automatic download runs; the card asks to try again after it.
    Busy,
    /// New pictures: how many, and the name the checksum gave if it did.
    Found { pictures: usize, name: Option<String> },
    /// The collection has nothing for the game (under its checksum's name or its file name).
    NothingFound { name: Option<String> },
    /// The collection could not be reached: nothing is marked as missing.
    Unreachable,
}

impl Again {
    /// The card's line (a toast).
    pub fn message(&self) -> String {
        match self {
            Again::Working => "Looking up this game again…".into(),
            Again::Offline => "Connect to Wi-Fi first to look up pictures.".into(),
            Again::Busy => "Pictures are being looked up. Try again in a moment.".into(),
            // The count first: a long name is what gets shortened.
            Again::Found { pictures: 1, name: Some(n) } => format!("Found 1 picture for {n}"),
            Again::Found { pictures, name: Some(n) } => format!("Found {pictures} pictures for {n}"),
            Again::Found { pictures: 1, name: None } => "Found 1 picture".into(),
            Again::Found { pictures, name: None } => format!("Found {pictures} pictures"),
            Again::NothingFound { name: Some(n) } => format!("No pictures found for {n}"),
            Again::NothingFound { name: None } => "No pictures found for this game".into(),
            Again::Unreachable => "Couldn't reach the picture collection. Try again later.".into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;

    fn dir(name: &str) -> crate::testutil::TestDir {
        TestDir::new(&format!("romid-{name}"))
    }

    const GBA_DAT: &str = r#"clrmamepro (
	name "Nintendo - Game Boy Advance"
)

game (
	name "Mega Man & Bass (USA)"
	region "USA"
	rom ( name "Mega Man & Bass (USA).gba" size 4 crc 36C08112 md5 0 sha1 0 )
)
game (
	name "Mega Man & Bass (USA) (Virtual Console)"
	rom ( name "Mega Man & Bass (USA) (Virtual Console).gba" size 4 crc 36C08112 )
)
game (
	name "Advance Wars (USA)"
	rom ( name "Advance Wars (USA).gba" size 3 crc 352441C2 )
)
"#;

    fn write(path: &Path, bytes: &[u8]) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }

    #[test]
    fn the_dat_gives_the_cleanest_name_for_a_checksum() {
        let t = parse_dat(GBA_DAT);
        assert_eq!(crc32fast::hash(b"mmbs"), 0x36C0_8112);
        assert_eq!(t.get(&0x36C0_8112).map(String::as_str), Some("Mega Man & Bass (USA)"), "not the re-release");
        assert_eq!(t.get(&crc32fast::hash(b"abc")).map(String::as_str), Some("Advance Wars (USA)"));
        assert_eq!(t.len(), 2);
        assert!(parse_dat("<html>not a dat</html>").is_empty());
    }

    #[test]
    fn a_scene_named_file_gets_its_proper_name_from_the_checksum() {
        let d = dir("names");
        let rom = d.join("roms/gba/0902 - MM&B (U)(Venom).gba");
        write(&rom, b"mmbs");
        let games = vec![("gba".to_string(), rom.clone())];
        let asked = std::cell::RefCell::new(Vec::new());
        let fetch = |dat: &str| {
            asked.borrow_mut().push(dat.to_string());
            Some(GBA_DAT.to_string())
        };
        let run = run_until(&d, &games, &fetch, true, || false);
        assert_eq!(run.names.get(&rom).map(String::as_str), Some("Mega Man & Bass (USA)"));
        assert_eq!((run.hashed, run.lists_missing, run.stopped), (1, false, false));
        assert_eq!(*asked.borrow(), vec!["Nintendo - Game Boy Advance".to_string()]);
        // Later: from the card alone, no file read, nothing fetched.
        assert_eq!(known(&d, &games), run.names);
        let again = run_until(&d, &games, &|_| panic!("the list is on the SD card"), true, || false);
        assert_eq!((again.hashed, again.names), (0, run.names));
    }

    #[test]
    fn a_changed_file_is_read_again_and_an_unchanged_one_is_not() {
        let d = dir("cache");
        let rom = d.join("roms/gba/game.gba");
        write(&rom, b"abc");
        let games = vec![("gba".to_string(), rom.clone())];
        let fetch = |_: &str| Some(GBA_DAT.to_string());
        assert_eq!(run_until(&d, &games, &fetch, true, || false).names[&rom], "Advance Wars (USA)");
        assert_eq!(run_until(&d, &games, &fetch, true, || false).hashed, 0, "same size and time: cached");
        // Another game copied over it (another size): read again, named anew.
        std::thread::sleep(Duration::from_millis(20));
        write(&rom, b"mmbs");
        let run = run_until(&d, &games, &fetch, true, || false);
        assert_eq!((run.hashed, run.names[&rom].as_str()), (1, "Mega Man & Bass (USA)"));
        // A game that left the library leaves the cache too.
        run_until(&d, &[], &fetch, true, || false);
        assert!(Cache::load(&d).is_empty());
    }

    #[test]
    fn without_the_list_the_file_name_is_used() {
        let d = dir("offline");
        let rom = d.join("roms/gba/Advance Wars (USA).gba");
        write(&rom, b"abc");
        let games = vec![("gba".to_string(), rom.clone())];
        // Offline: the checksum is kept, no names, and the list is said to be missing.
        let run = run_until(&d, &games, &|_| None, true, || false);
        assert!(run.names.is_empty() && run.lists_missing);
        assert_eq!(run.hashed, 1);
        assert!(known(&d, &games).is_empty());
        // A download that is no DAT (a captive portal's page) is not kept as the list.
        let run = run_until(&d, &games, &|_| Some("<html>sign in</html>".into()), true, || false);
        assert!(run.names.is_empty() && run.lists_missing && !table_path(&d, "Nintendo - Game Boy Advance").exists());
        // Online later: named without reading the file again.
        let run = run_until(&d, &games, &|_| Some(GBA_DAT.into()), true, || false);
        assert_eq!((run.hashed, run.names.len(), run.lists_missing), (0, 1, false));
    }

    #[test]
    fn disc_systems_big_files_and_7z_keep_their_file_names() {
        let d = dir("skip");
        assert_eq!(dat_name("psx"), None);
        assert_eq!(dat_name("gba"), Some("Nintendo - Game Boy Advance"));
        assert_eq!(dat_name("megadrive"), Some("Sega - Mega Drive - Genesis"));
        let iso = d.join("roms/psx/Game.bin");
        write(&iso, b"disc");
        assert_eq!(checksums("psx", &iso).unwrap(), None);
        let seven = d.join("roms/gba/Game.7z");
        write(&seven, b"7z");
        assert_eq!(checksums("gba", &seven).unwrap(), None);
        // Too big: a sparse file over the limit is not read.
        let big = d.join("roms/nds/Big.nds");
        std::fs::create_dir_all(big.parent().unwrap()).unwrap();
        let f = std::fs::File::create(&big).unwrap();
        f.set_len(MAX_HASH + 1).unwrap();
        assert_eq!(checksums("nds", &big).unwrap(), None);
        assert!(checksums("gba", &d.join("gone.gba")).is_err(), "unreadable: tried again later");
        // The cache remembers "nothing to compare" so the file is not opened every start.
        let games = vec![("nds".to_string(), big.clone())];
        assert_eq!(run_until(&d, &games, &|_| None, true, || false).hashed, 1);
        assert_eq!(run_until(&d, &games, &|_| None, true, || false).hashed, 0);
    }

    #[test]
    fn headers_and_byte_orders_also_match_the_lists_form() {
        let d = dir("forms");
        let body: Vec<u8> = (0..4096u32).map(|i| (i * 13 % 251) as u8).collect();
        let want = crc32fast::hash(&body);
        let mut nes = b"NES\x1a\x02\x01\0\0\0\0\0\0\0\0\0\0".to_vec();
        nes.extend(&body);
        let p = d.join("x.nes");
        write(&p, &nes);
        assert_eq!(checksums("nes", &p).unwrap(), Some(Sums { crc: crc32fast::hash(&nes), alt: Some(want) }));
        let mut smc = vec![0; 512];
        smc.extend(&body);
        let p = d.join("x.smc");
        write(&p, &smc);
        assert_eq!(checksums("snes", &p).unwrap().unwrap().alt, Some(want));
        // N64 in big-endian order (.z64) is the list's; .v64 and .n64 are turned into it.
        let mut z64 = vec![0x80, 0x37, 0x12, 0x40];
        z64.extend(&body);
        let v64: Vec<u8> = z64.chunks(2).flat_map(|p| [p[1], p[0]]).collect();
        let n64: Vec<u8> = z64.chunks(4).flat_map(|p| [p[3], p[2], p[1], p[0]]).collect();
        for (name, bytes) in [("x.v64", v64), ("x.n64", n64)] {
            let p = d.join(name);
            write(&p, &bytes);
            assert_eq!(checksums("n64", &p).unwrap().unwrap().alt, Some(crc32fast::hash(&z64)), "{name}");
        }
        let p = d.join("x.z64");
        write(&p, &z64);
        assert_eq!(checksums("n64", &p).unwrap(), Some(Sums { crc: crc32fast::hash(&z64), alt: None }));
    }

    #[test]
    fn a_named_game_comes_off_the_missing_lists_once() {
        let d = dir("unlist");
        let rom = d.join("roms/gba/0902 - MM&B (U).gba");
        write(&rom, b"mmbs");
        // The name-only lookup of an older version found nothing and wrote the game down.
        for f in missing_lists(&d, "gba") {
            write(&f, b"Other\n0902 - MM&B (U)\n");
        }
        let games = vec![("gba".to_string(), rom.clone())];
        run_until(&d, &games, &|_| Some(GBA_DAT.into()), true, || false);
        for f in missing_lists(&d, "gba") {
            assert_eq!(std::fs::read_to_string(&f).unwrap(), "Other\n", "{}", f.display());
        }
        // Found nothing under its proper name either: written down again by the download, and left there.
        for f in missing_lists(&d, "gba") {
            write(&f, b"Other\n0902 - MM&B (U)\n");
        }
        run_until(&d, &games, &|_| Some(GBA_DAT.into()), true, || false);
        assert!(std::fs::read_to_string(&missing_lists(&d, "gba")[0]).unwrap().contains("MM&B"));
    }

    #[test]
    fn look_up_again_forgets_the_game_and_only_that_game() {
        let d = dir("again");
        let rom = d.join("roms/gba/Wrong Art.gba");
        let other = d.join("roms/gba/Other.gba");
        write(&rom, b"abc");
        write(&other, b"mmbs");
        let games = vec![("gba".to_string(), rom.clone()), ("gba".to_string(), other.clone())];
        run_until(&d, &games, &|_| Some(GBA_DAT.into()), true, || false);
        let cover = crate::covers::cover_path(&d, "gba", &rom);
        let snap = crate::game_pictures::path(&d, crate::game_pictures::Kind::Snap, "gba", &rom);
        let kept = crate::covers::cover_path(&d, "gba", &other);
        for f in [&cover, &snap, &kept] {
            write(f, b"png");
        }
        write(&missing_lists(&d, "gba")[2], b"Wrong Art\nOther\n");
        forget(&d, "gba", &rom);
        assert!(!cover.exists() && !snap.exists(), "its pictures go");
        assert!(kept.exists(), "another game's stay");
        assert_eq!(std::fs::read_to_string(&missing_lists(&d, "gba")[2]).unwrap(), "Other\n");
        assert_eq!(Cache::load(&d).len(), 1, "its checksum is forgotten");
        // The lookup after it reads the file again (checksum first) and names it.
        let run = run_until(&d, &games, &|_| panic!("the list is on the SD card"), true, || false);
        assert_eq!((run.hashed, run.names[&rom].as_str()), (1, "Advance Wars (USA)"));
        // A game never looked at: nothing to forget, nothing breaks.
        forget(&d, "gba", &d.join("roms/gba/Never.gba"));
    }

    #[test]
    fn a_game_starting_stops_the_run_and_keeps_what_it_did() {
        let d = dir("stop");
        let games: Vec<(String, PathBuf)> = (0..3)
            .map(|i| {
                let p = d.join(format!("roms/gba/{i}.gba"));
                write(&p, format!("game {i}").as_bytes());
                ("gba".to_string(), p)
            })
            .collect();
        let n = std::cell::Cell::new(0);
        let run = run_until(&d, &games, &|_| None, true, || {
            n.set(n.get() + 1);
            n.get() > 2
        });
        assert!(run.stopped);
        assert_eq!((run.hashed, Cache::load(&d).len()), (2, 2), "two read and kept, none pruned");
        assert_eq!(run_until(&d, &games, &|_| None, true, || false).hashed, 1, "the rest after the game");
    }

    #[test]
    fn the_cards_line_says_what_happened() {
        assert_eq!(Again::Found { pictures: 3, name: Some("Tetris (World)".into()) }.message(), "Found 3 pictures for Tetris (World)");
        assert_eq!(Again::Found { pictures: 1, name: None }.message(), "Found 1 picture");
        assert_eq!(Again::NothingFound { name: None }.message(), "No pictures found for this game");
        assert_eq!(Again::Offline.message(), "Connect to Wi-Fi first to look up pictures.");
    }
}
