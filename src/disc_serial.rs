//! PlayStation discs recognised by the **serial on the disc** instead of its file name: a PlayStation 1 or 2 image
//! whose file is called by its serial or by junk ("SLES_556.73.Something.iso", "SLUS_209.46.iso") shows the game's
//! name ("Final Fantasy VII"), gets its facts and is looked up in the libretro thumbnail collection under that name —
//! the same flow as a cartridge recognised by its checksum (`rom_id`).
//!
//! **The serial.** A PlayStation disc says which program it boots in its ISO 9660 file `SYSTEM.CNF`:
//! `BOOT = cdrom0:\SLES_556.73;1` (PS1) or `BOOT2 = cdrom0:\SLUS_209.46;1` (PS2), which is `SLES-55673` the way the
//! lists write it ([`normalize`]). Read from the image with a few sector reads: the layout of the image (one 37 KB
//! read at the start), the volume descriptor at sector 16, the root directory and that one file — tens of KB, never
//! the whole image, no matter how big it is. Supported are a plain `.iso`, a `.bin` (2048-byte sectors, or a
//! MODE2/2352 PlayStation dump with its 2048 bytes of user data inside the sector), a `.cue` and an `.m3u` that
//! name them. A `.chd`, `.cso` or `.pbp` is not opened (its packing would have to be undone first): such a game
//! keeps its file name, cleaned up ([`clean_stem`]). Done by the thread that recognises the cartridges
//! ([`crate::rom_id::run_until`]), never on the UI thread, and kept in `<data>/identify/discs.tsv` by path, size and
//! modification time, so each image is read once.
//!
//! **The name.** libretro-database's Redump DAT of the system ("Sony - PlayStation", "Sony - PlayStation 2") names
//! every release with its serial (`game ( name "…" serial "SLUS-20946" … )`). It is downloaded once when the Pocket
//! is online, kept as `<data>/identify/<list>.serials.tsv` (serial, name) and fetched again after
//! [`DAT_MAX_AGE`], exactly like the No-Intro lists of the cartridge systems. The name it gives is the one the
//! thumbnail collection files the pictures under (`covers::pick`) and the one the offline game list matches best
//! (`gamedb`); the shelf shows it without its region tags ([`shelf_name`]). A serial no list knows leaves the file
//! name, without a serial prefix in front of it.

use std::collections::HashMap;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::rom_id::{Names, DAT_MAX_AGE};

/// The libretro-database Redump DAT of a disc system — also the name of its folder in the libretro thumbnail
/// collection — or None for a system this reader does not know.
pub fn dat(system: &str) -> Option<&'static str> {
    match system {
        "psx" => Some("Sony - PlayStation"),
        "ps2" => Some("Sony - PlayStation 2"),
        _ => None,
    }
}

/// Whether `dat` is one of the Redump lists (which the serial reader maps, while `rom_id` maps the No-Intro ones).
pub fn is_disc_dat(dat: &str) -> bool {
    ["Sony - PlayStation", "Sony - PlayStation 2"].contains(&dat)
}

const SOURCE: &str = "https://raw.githubusercontent.com/libretro/libretro-database/master/metadat/redump";

/// Where libretro-database keeps that list.
pub fn dat_url(dat: &str) -> String {
    format!("{SOURCE}/{}.dat", crate::covers::percent_encode(dat))
}

/// The 2048 bytes of a sector that hold the file system.
const SECTOR_DATA: usize = 2048;
/// The root directory is read up to here (a real one is a few sectors; a thousand games fit).
const DIR_MAX: usize = 64 << 10;
/// SYSTEM.CNF is a few hundred bytes; this is the most that is read of it.
const CNF_MAX: usize = 8 << 10;
/// A `.cue` or `.m3u` is a few hundred bytes; one bigger than this is not a sheet.
const SHEET_MAX_BYTES: u64 = 1 << 20;
/// How many sheets deep an `.m3u` → `.cue` → `.bin` chain is followed.
const SHEETS_MAX: u8 = 3;

// ---- reading the serial off a disc image -------------------------------------------------------------------------

/// How a disc image stores its sectors: the sector size and where the 2048 bytes of user data start inside one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Layout {
    sector: u64,
    data: u64,
}

impl Layout {
    /// The layout of the image, told by where the file system's first volume descriptor stands (`CD001` at sector
    /// 16); None for a file that is no ISO 9660 image at all.
    fn detect(f: &mut std::fs::File) -> io::Result<Option<Layout>> {
        // Enough for sector 16 in every layout, with its descriptor a whole sector in.
        let mut buf = vec![0u8; 17 * 2352 + 64];
        let n = read_at(f, 0, &mut buf)?;
        let buf = &buf[..n];
        // A plain ISO (2048-byte sectors), a PlayStation MODE2/2352 dump, and MODE1/2352 for completeness.
        for layout in [Layout { sector: 2048, data: 0 }, Layout { sector: 2352, data: 24 }, Layout { sector: 2352, data: 16 }] {
            let at = (16 * layout.sector + layout.data + 1) as usize;
            if buf.get(at..at + 5) == Some(b"CD001".as_slice()) {
                return Ok(Some(layout));
            }
        }
        Ok(None)
    }

    /// User data from sector `lba` on, `len` bytes at most ([`DIR_MAX`]), sector by sector (the headers of a raw
    /// dump are skipped, so what comes back is the file system as a plain ISO has it).
    fn read(&self, f: &mut std::fs::File, lba: u64, len: usize) -> io::Result<Vec<u8>> {
        let len = len.min(DIR_MAX);
        let mut out = Vec::with_capacity(len.min(SECTOR_DATA));
        let mut raw = vec![0u8; self.sector as usize];
        let mut lba = lba;
        while out.len() < len {
            if read_at(f, lba * self.sector, &mut raw)? < raw.len() {
                break;
            }
            let take = (len - out.len()).min(SECTOR_DATA);
            out.extend_from_slice(&raw[self.data as usize..self.data as usize + take]);
            lba += 1;
        }
        Ok(out)
    }
}

/// Fills `buf` from `at` on as far as the file goes.
fn read_at(f: &mut std::fs::File, at: u64, buf: &mut [u8]) -> io::Result<usize> {
    f.seek(SeekFrom::Start(at))?;
    let mut n = 0;
    while n < buf.len() {
        match f.read(&mut buf[n..])? {
            0 => break,
            k => n += k,
        }
    }
    Ok(n)
}

/// A little-endian 32-bit number of an ISO 9660 record (the small half of its both-endian pair).
fn le32(b: &[u8]) -> u64 {
    u64::from(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

/// `SYSTEM.CNF`'s text from the image, and the volume identifier of the image as a fallback for a disc whose boot
/// file is missing; either may be empty. `l` is the layout [`Layout::detect`] found.
fn boot_text(f: &mut std::fs::File, l: Layout) -> io::Result<(String, String)> {
    let pvd = l.read(f, 16, SECTOR_DATA)?;
    if pvd.len() < SECTOR_DATA || pvd[0] != 1 || &pvd[1..6] != b"CD001" {
        return Ok((String::new(), String::new()));
    }
    let volume = String::from_utf8_lossy(&pvd[40..72]).trim().to_string();
    // The root directory's record: its extent and how long it is.
    let (extent, size) = (le32(&pvd[156 + 2..]), le32(&pvd[156 + 10..]) as usize);
    let dir = l.read(f, extent, size.min(DIR_MAX))?;
    let mut found = None;
    let mut at = 0;
    while at < dir.len() {
        let len = dir[at] as usize;
        // No record here: the next sector starts the next block of records.
        if len == 0 {
            at = (at / SECTOR_DATA + 1) * SECTOR_DATA;
            continue;
        }
        if let Some(rec) = dir.get(at..at + len) {
            let name = rec.get(33..33 + rec[32] as usize).unwrap_or_default();
            if rec[25] & 0x02 == 0 && name_is(name, b"SYSTEM.CNF") {
                found = Some((le32(&rec[2..]), le32(&rec[10..]) as usize));
                break;
            }
        }
        at += len;
    }
    let text = match found {
        Some((extent, size)) => String::from_utf8_lossy(&l.read(f, extent, size.min(CNF_MAX))?).into_owned(),
        None => String::new(),
    };
    Ok((text, volume))
}

/// Whether an ISO 9660 file identifier names `wanted` ("SYSTEM.CNF;1" is "SYSTEM.CNF", ignoring case).
fn name_is(name: &[u8], wanted: &[u8]) -> bool {
    let stem = name.split(|b| *b == b';').next().unwrap_or_default();
    stem.len() == wanted.len() && stem.iter().zip(wanted).all(|(a, b)| a.eq_ignore_ascii_case(b))
}

/// The serial on the disc: `SYSTEM.CNF`'s `BOOT`/`BOOT2` line, else the image's volume identifier when that is a
/// serial. None for a disc without either.
fn serial_on(f: &mut std::fs::File) -> io::Result<Option<String>> {
    let Some(layout) = Layout::detect(f)? else { return Ok(None) };
    let (cnf, volume) = boot_text(f, layout)?;
    Ok(parse_cnf(&cnf).or_else(|| normalize(&volume)))
}

/// The file the serial is read from: an image itself, or the first file of its `.cue`/`.m3u` (None for what this
/// reader does not open: a CHD, a CSO, a PBP).
fn image_of(rom: &Path, depth: u8) -> Option<PathBuf> {
    let ext = rom.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).unwrap_or_default();
    match ext.as_str() {
        "iso" | "bin" | "img" => Some(rom.to_path_buf()),
        "cue" | "m3u" if depth < SHEETS_MAX => {
            let meta = std::fs::metadata(rom).ok()?;
            if meta.len() > SHEET_MAX_BYTES {
                return None;
            }
            let text = String::from_utf8_lossy(&std::fs::read(rom).ok()?).into_owned();
            let first = crate::library::sheet_references(&ext, &text).into_iter().next()?;
            let next = rom.parent().unwrap_or(Path::new(".")).join(first.replace('\\', "/"));
            image_of(&next, depth + 1)
        }
        _ => None,
    }
}

/// The serial of the disc image at `rom` as the lists write it without dashes and dots ("SLES55673"), read with a
/// few sector reads. None for an image this reader does not open (`.chd`, `.cso`, `.pbp`), for a disc without a
/// serial, and for a file that is no ISO 9660 image — those keep their file name. An error is a file that could not
/// be read (tried again next time).
pub fn serial_of(rom: &Path) -> io::Result<Option<String>> {
    let Some(image) = image_of(rom, 0) else { return Ok(None) };
    let mut f = match std::fs::File::open(&image) {
        Ok(f) => f,
        // The sheet is there but its track file is not (yet): not this game's serial, no error either.
        Err(e) if e.kind() == io::ErrorKind::NotFound && image != rom => return Ok(None),
        Err(e) => return Err(e),
    };
    serial_on(&mut f)
}

/// The serial in a `SYSTEM.CNF`: its `BOOT` (PlayStation 1) or `BOOT2` (PlayStation 2) line,
/// `BOOT = cdrom0:\SLES_556.73;1`.
pub fn parse_cnf(text: &str) -> Option<String> {
    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else { continue };
        let key = key.trim();
        if (key.eq_ignore_ascii_case("BOOT") || key.eq_ignore_ascii_case("BOOT2")) && normalise(value).is_some() {
            return normalise(value);
        }
    }
    None
}

/// The serial in a boot path: the file name of `cdrom0:\SLES_556.73;1`, without the `;1` version.
fn normalise(value: &str) -> Option<String> {
    let tail = value.rsplit(['\\', '/']).next().unwrap_or(value);
    let name = tail.split(';').next().unwrap_or(tail).trim();
    normalize(name)
}

/// A serial the way the lists write it without its dashes, dots and blanks — "SLES_556.73", "SLES-55673" and
/// "sles 556.73" are all "SLES55673" — or None for a string that is not serial-shaped (a volume name, "PSX.EXE",
/// "PLAYSTATION"). Two to four letters, at least three digits, six to sixteen characters.
pub fn normalize(s: &str) -> Option<String> {
    let up: String = s.chars().filter(char::is_ascii_alphanumeric).map(|c| c.to_ascii_uppercase()).collect();
    let letters = up.bytes().take_while(u8::is_ascii_alphabetic).count();
    let digits = up.bytes().filter(u8::is_ascii_digit).count();
    ((2..=4).contains(&letters) && digits >= 3 && (6..=16).contains(&up.len())).then_some(up)
}

// ---- naming: the file name, and what a list says about a serial ---------------------------------------------------

/// The serial at the start of a file name, and what follows it: "SLES_556.73.Something" is "" + "Something" when
/// [`serial_prefix_rest`] has taken "_556.73." off. None when `rest` is not the rest of a serial.
fn serial_prefix_rest(rest: &str) -> Option<&str> {
    let b = rest.as_bytes();
    let mut i = 0;
    if matches!(b.first(), Some(b'_') | Some(b'-') | Some(b' ')) {
        i += 1;
    }
    let digits = b[i..].iter().take_while(|c| c.is_ascii_digit()).count();
    if digits != 3 || !matches!(b.get(i + digits), Some(b'.') | Some(b'-') | Some(b'_') | Some(b' ')) {
        return None;
    }
    i += digits + 1;
    let last = b[i..].iter().take_while(|c| c.is_ascii_digit()).count();
    (last == 2).then(|| &rest[i + 2..])
}

/// A file name that starts with a disc's serial loses it when a title follows: "SLES_556.73.Something" is
/// "Something". Without a title after it ("SLUS_209.46") the name stays as it is — it is all there is to show.
pub fn clean_stem(stem: &str) -> String {
    let letters = stem.bytes().take_while(u8::is_ascii_alphabetic).count();
    if (2..=4).contains(&letters) {
        if let Some(rest) = serial_prefix_rest(&stem[letters..]) {
            let title = rest.trim_start_matches([' ', '.', '_', '-']);
            if title.chars().any(char::is_alphanumeric) {
                return title.to_string();
            }
        }
    }
    stem.to_string()
}

/// The name for the shelf from a list's name: the region and version tags go ("Final Fantasy VII (Europe)" is
/// "Final Fantasy VII"), a disc tag stays ("Final Fantasy VII (Disc 2)"), so the discs of one game tell apart.
pub fn shelf_name(dat_name: &str) -> String {
    let disc = sheets_tag(dat_name);
    let base = crate::library::display_name(dat_name);
    match disc {
        Some(tag) if !base.contains(&tag) => format!("{base} {tag}"),
        _ => base,
    }
}

/// The "(Disc 2)" tag of a list's name, if it has one.
fn sheets_tag(name: &str) -> Option<String> {
    let mut depth = 0;
    let mut tag = String::new();
    for ch in name.chars() {
        match ch {
            '(' | '[' => {
                depth += 1;
                tag.clear();
            }
            ')' | ']' if depth > 0 => {
                depth -= 1;
                if depth == 0 && tag.to_ascii_lowercase().starts_with("disc ") {
                    return Some(format!("({tag})"));
                }
            }
            _ if depth > 0 => tag.push(ch),
            _ => {}
        }
    }
    None
}

// ---- the lists, and the cache of what was read ---------------------------------------------------------------

/// A Redump DAT (clrmamepro format): serial → the game's name. A serial several releases share (another region, a
/// re-release) keeps the cleanest name, as the covers rank them.
pub fn parse_dat(text: &str) -> HashMap<String, String> {
    let mut out: HashMap<String, String> = HashMap::new();
    let mut game: Option<String> = None;
    for line in text.lines() {
        let line = line.trim_start();
        if line.starts_with("game (") || line.starts_with("game(") {
            game = None;
        } else if let Some(rest) = line.strip_prefix("name \"") {
            if game.is_none() {
                game = rest.split('"').next().map(str::to_string);
            }
        } else if let Some(rest) = line.strip_prefix("serial \"") {
            let (Some(name), Some(serial)) = (&game, rest.split('"').next().and_then(normalize)) else { continue };
            match out.get(&serial) {
                Some(have) if crate::covers::rank(have) <= crate::covers::rank(name) => {}
                _ => {
                    out.insert(serial, name.clone());
                }
            }
        }
    }
    out
}

/// What the cache knows of one image.
#[derive(Clone, Debug, PartialEq)]
struct Known {
    system: String,
    size: u64,
    /// Modification time in nanoseconds since 1970.
    mtime: u64,
    /// None: looked at, no serial this reader finds (a CHD, a disc without SYSTEM.CNF).
    serial: Option<String>,
}

fn dir(data_dir: &Path) -> PathBuf {
    data_dir.join("identify")
}

/// The size and modification time a cache line is valid for.
fn stamp(rom: &Path) -> io::Result<(u64, u64)> {
    let m = std::fs::metadata(rom)?;
    let mtime = m.modified()?.duration_since(SystemTime::UNIX_EPOCH).map_or(0, |d| d.as_nanos() as u64);
    Ok((m.len(), mtime))
}

/// `<data>/identify/discs.tsv`: one line per game, `system  size  mtime  serial  path` (`-` for none), the size and
/// time of the image the serial came from (of the track a `.cue` or `.m3u` names, not of the sheet).
#[derive(Default)]
pub struct Cache {
    files: HashMap<PathBuf, Known>,
}

impl Cache {
    pub fn load(data_dir: &Path) -> Cache {
        let text = std::fs::read_to_string(dir(data_dir).join("discs.tsv")).unwrap_or_default();
        let files = text
            .lines()
            .filter_map(|l| {
                let f: Vec<&str> = l.splitn(5, '\t').collect();
                let [system, size, mtime, serial, path] = f[..] else { return None };
                let serial = (!serial.is_empty() && serial != "-").then(|| serial.to_string());
                Some((PathBuf::from(path), Known { system: system.into(), size: size.parse().ok()?, mtime: mtime.parse().ok()?, serial }))
            })
            .collect();
        Cache { files }
    }

    pub fn save(&self, data_dir: &Path) -> io::Result<()> {
        let mut lines: Vec<String> = self
            .files
            .iter()
            .map(|(p, k)| format!("{}\t{}\t{}\t{}\t{}\n", k.system, k.size, k.mtime, k.serial.as_deref().unwrap_or("-"), p.display()))
            .collect();
        lines.sort();
        std::fs::create_dir_all(dir(data_dir))?;
        crate::fsutil::write_atomic(&dir(data_dir).join("discs.tsv"), lines.concat().as_bytes())
    }

    /// The serial kept for `rom`, if the line is still the file's (same size and time).
    fn valid(&self, rom: &Path, at: (u64, u64)) -> Option<Option<&str>> {
        self.files.get(rom).filter(|k| (k.size, k.mtime) == at).map(|k| k.serial.as_deref())
    }

    pub fn len(&self) -> usize {
        self.files.len()
    }

    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }
}

fn table_path(data_dir: &Path, dat: &str) -> PathBuf {
    dir(data_dir).join(format!("{dat}.serials.tsv"))
}

/// The serial → name table of `dat` kept on the card, if any.
pub fn load_table(data_dir: &Path, dat: &str) -> Option<HashMap<String, String>> {
    let text = std::fs::read_to_string(table_path(data_dir, dat)).ok()?;
    Some(text.lines().filter_map(|l| l.split_once('\t')).map(|(s, n)| (s.to_string(), n.to_string())).collect())
}

fn store_table(data_dir: &Path, dat: &str, table: &HashMap<String, String>) -> io::Result<()> {
    let mut lines: Vec<String> = table.iter().map(|(s, n)| format!("{s}\t{n}\n")).collect();
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

/// Downloads a Redump DAT from libretro-database (several MB as text). None offline.
pub fn fetch_dat(dat: &str) -> Option<String> {
    crate::covers::fetch_text(&dat_url(dat))
}

/// The names the card already has for `games` (system id, image path): the serial cached in `<data>/identify/` and
/// the list on the card. Nothing is read through or downloaded, so it is quick enough for any background thread
/// (the scan that makes the shelf, the cover download, the game card); an image changed since is caught by the next
/// [`run_until`].
pub fn known(data_dir: &Path, games: &[(String, PathBuf)]) -> Names {
    let mut out = Names::new();
    let mut cache: Option<Cache> = None;
    let mut tables: HashMap<&'static str, Option<HashMap<String, String>>> = HashMap::new();
    for (sys, rom) in games {
        let Some(d) = dat(sys) else { continue };
        let Some(serial) = cache.get_or_insert_with(|| Cache::load(data_dir)).files.get(rom).and_then(|k| k.serial.clone()) else {
            continue;
        };
        let table = tables.entry(d).or_insert_with(|| load_table(data_dir, d));
        if let Some(name) = table.as_ref().and_then(|t| t.get(&serial)) {
            out.insert(rom.clone(), name.clone());
        }
    }
    out
}

/// What a [`run_until`] did with the discs.
#[derive(Debug, Default)]
pub struct Named {
    /// Every disc whose serial a list knows, by path.
    pub names: Names,
    /// Images read this time.
    pub read: usize,
    /// A system's list could not be had (offline, a failed download): its discs keep their file names until then.
    pub lists_missing: bool,
    /// A game started before the run was through; the rest waits.
    pub stopped: bool,
}

/// Reads the serial of every PlayStation image in `games` that the cache doesn't have, gets the Redump lists of
/// their systems (`fetch`: the DAT's text, None offline; called only for a list missing or old) and names every disc
/// they know. `stop` is asked before each image. A disc named for the first time is taken off the `.missing` lists
/// so its pictures are looked up again under its proper name. `prune`: `games` is the whole library, so cache lines
/// of other files go.
pub fn run_until(
    data_dir: &Path,
    games: &[(String, PathBuf)],
    fetch: &dyn Fn(&str) -> Option<String>,
    prune: bool,
    stop: impl Fn() -> bool,
) -> Named {
    let wanted: Vec<&(String, PathBuf)> = games.iter().filter(|(sys, _)| dat(sys).is_some()).collect();
    let mut out = Named::default();
    if wanted.is_empty() {
        return out;
    }
    let mut cache = Cache::load(data_dir);
    let mut fresh: std::collections::HashSet<PathBuf> = Default::default();
    let mut dirty = false;
    for (sys, rom) in &wanted {
        // The image the serial comes from (the track a `.cue` or `.m3u` names), so a changed track is read again.
        let Some(image) = image_of(rom, 0) else { continue };
        let Ok(at) = stamp(&image) else { continue };
        if cache.valid(rom, at).is_some() {
            continue;
        }
        if stop() {
            out.stopped = true;
            break;
        }
        // An image that can't be read now (card busy, removed) is tried again next time.
        let Ok(serial) = serial_of(rom) else { continue };
        cache.files.insert(rom.clone(), Known { system: sys.clone(), size: at.0, mtime: at.1, serial });
        out.read += 1;
        dirty = true;
        fresh.insert(rom.clone());
    }
    if prune && !out.stopped {
        let all: std::collections::HashSet<&PathBuf> = games.iter().map(|g| &g.1).collect();
        let before = cache.files.len();
        cache.files.retain(|p, _| all.contains(p));
        dirty |= cache.files.len() != before;
    }
    if dirty {
        let _ = cache.save(data_dir);
    }
    // The lists of the systems the discs are on, fetched when missing or old; a list fetched now names its discs for
    // the first time.
    let mut tables: HashMap<&str, HashMap<String, String>> = HashMap::new();
    let mut fetched: Vec<&str> = Vec::new();
    let mut dats: Vec<&str> = wanted.iter().filter_map(|(sys, _)| dat(sys)).collect();
    dats.sort_unstable();
    dats.dedup();
    for d in dats {
        let mut table = load_table(data_dir, d);
        // Not while a game runs: the download waits for the next run.
        if !out.stopped && table_due(data_dir, d) && !stop() {
            if let Some(new) = fetch(d).map(|t| parse_dat(&t)).filter(|t| !t.is_empty()) {
                let _ = store_table(data_dir, d, &new);
                if table.is_none() {
                    fetched.push(d);
                }
                table = Some(new);
            }
        }
        match table {
            Some(t) => {
                tables.insert(d, t);
            }
            None => out.lists_missing = true,
        }
    }
    for (sys, rom) in &wanted {
        let Some(d) = dat(sys) else { continue };
        let (Some(table), Some(Some(serial))) = (tables.get(d), cache.files.get(rom).map(|k| k.serial.clone())) else { continue };
        if let Some(name) = table.get(&serial) {
            out.names.insert(rom.clone(), name.clone());
            if fresh.contains(rom) || fetched.contains(&d) {
                crate::rom_id::unlist_everywhere(data_dir, sys, rom);
            }
        }
    }
    out
}

/// "Look up again" on the card for a disc: its serial is forgotten, so the next [`run_until`] reads the image again.
pub fn forget(data_dir: &Path, rom: &Path) {
    let mut cache = Cache::load(data_dir);
    if cache.files.remove(rom).is_some() {
        let _ = cache.save(data_dir);
    }
}

// ---- building discs: the fixtures of the tests here and in `library` ------------------------------------------

/// A directory record of an ISO 9660 image (both-endian fields as the standard has them).
#[cfg(test)]
fn record(name: &[u8], extent: u32, size: u32, dir: bool) -> Vec<u8> {
    let mut r = vec![0u8; 33 + name.len()];
    let len = r.len();
    r[0] = len as u8;
    for (at, field) in [(2, extent), (10, size)] {
        r[at..at + 4].copy_from_slice(&field.to_le_bytes());
        r[at + 4..at + 8].copy_from_slice(&field.to_be_bytes());
    }
    r[25] = if dir { 0x02 } else { 0 };
    r[32] = name.len() as u8;
    r[33..].copy_from_slice(name);
    r
}

/// The ISO 9660 image as its 2048-byte sectors: sector 16 the volume descriptor, sector 18 the root directory,
/// sector 20 SYSTEM.CNF.
#[cfg(test)]
fn iso_sectors(cnf: &str) -> Vec<Vec<u8>> {
    let mut sectors = vec![vec![0u8; SECTOR_DATA]; 21];
    let pvd = &mut sectors[16];
    pvd[0] = 1;
    pvd[1..6].copy_from_slice(b"CD001");
    pvd[6] = 1;
    pvd[40..40 + 11].copy_from_slice(b"PLAYSTATION");
    let root = record(&[0], 18, SECTOR_DATA as u32, true);
    pvd[156..156 + root.len()].copy_from_slice(&root);
    let mut at = 0;
    for r in [record(&[0], 18, SECTOR_DATA as u32, true), record(&[1], 18, SECTOR_DATA as u32, true), record(b"SYSTEM.CNF;1", 20, cnf.len() as u32, false)] {
        sectors[18][at..at + r.len()].copy_from_slice(&r);
        at += r.len();
    }
    sectors[20][..cnf.len()].copy_from_slice(cnf.as_bytes());
    sectors
}

/// The sectors as a plain ISO, and as a PlayStation MODE2/2352 dump (12-byte sync, 3-byte address, 1 byte mode,
/// 8 bytes of subheader, 2048 bytes of data, 276 bytes of error correction and spare).
#[cfg(test)]
pub(crate) fn test_image(cnf: &str, sector_size: usize) -> Vec<u8> {
    let mut out = Vec::new();
    for sector in iso_sectors(cnf) {
        if sector_size == SECTOR_DATA {
            out.extend_from_slice(&sector);
            continue;
        }
        let mut raw = vec![0u8; sector_size];
        raw[0] = 0;
        raw[1..11].fill(0xFF);
        raw[11] = 0;
        raw[15] = 2;
        raw[16..20].copy_from_slice(&[0, 0, 8, 0]);
        raw[24..24 + SECTOR_DATA].copy_from_slice(&sector);
        out.extend_from_slice(&raw);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;

    // The ISO 9660 fixtures are built by `test_image` above.

    const PS1_CNF: &str = "BOOT = cdrom:\\SLES_556.73;1\r\nTCB = 4\r\nEVENT = 10\r\nSTACK = 801FFF00\r\n";
    const PS2_CNF: &str = "BOOT2 = cdrom0:\\SLUS_209.46;1\r\nVER = 1.00\r\nVMODE = NTSC\r\n";

    fn dir(name: &str) -> TestDir {
        TestDir::new(&format!("discserial-{name}"))
    }

    fn write(path: &Path, bytes: &[u8]) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }

    // ---- reading the serial -------------------------------------------------------------------------------

    #[test]
    fn the_boot_line_of_system_cnf_is_the_serial() {
        assert_eq!(parse_cnf(PS1_CNF).as_deref(), Some("SLES55673"));
        assert_eq!(parse_cnf(PS2_CNF).as_deref(), Some("SLUS20946"));
        // Spacing, case, a missing cdrom prefix, a slash, and a boot path without a version.
        assert_eq!(parse_cnf("boot=cdrom:\\sles_556.73;1").as_deref(), Some("SLES55673"));
        assert_eq!(parse_cnf("BOOT2=cdrom0:/SLPS_01204").as_deref(), Some("SLPS01204"));
        // Nothing serial-shaped in it.
        assert_eq!(parse_cnf("TCB = 4\nEVENT = 10\n"), None);
        assert_eq!(parse_cnf("BOOT = cdrom:\\PSX.EXE;1"), None);
        assert_eq!(parse_cnf("BOOT = <<<garbage>>>"), None);
        assert_eq!(parse_cnf(""), None);
    }

    #[test]
    fn serials_are_compared_without_their_dashes_dots_and_case() {
        for s in ["SLES_556.73", "SLES-55673", "sles 556.73", "SLES55673"] {
            assert_eq!(normalize(s).as_deref(), Some("SLES55673"), "{s}");
        }
        assert_eq!(normalize("SLUS_209.46").as_deref(), Some("SLUS20946"));
        assert_eq!(normalize("SLPM-65002-0").as_deref(), Some("SLPM650020"));
        // Not a serial.
        for s in ["PLAYSTATION", "PSX.EXE", "SLES", "12345678", "Euro", ""] {
            assert_eq!(normalize(s), None, "{s}");
        }
    }

    #[test]
    fn a_plain_iso_and_a_mode2_2352_dump_give_the_same_serial() {
        let d = dir("formats");
        let iso = d.join("roms/psx/Game.iso");
        write(&iso, &test_image(PS1_CNF, 2048));
        assert_eq!(serial_of(&iso).unwrap().as_deref(), Some("SLES55673"));
        let bin = d.join("roms/psx/Game (Track 1).bin");
        write(&bin, &test_image(PS1_CNF, 2352));
        assert_eq!(serial_of(&bin).unwrap().as_deref(), Some("SLES55673"));
        // A PS2 disc: the BOOT2 line.
        let ps2 = d.join("roms/ps2/Game.iso");
        write(&ps2, &test_image(PS2_CNF, 2048));
        assert_eq!(serial_of(&ps2).unwrap().as_deref(), Some("SLUS20946"));
    }

    #[test]
    fn a_cue_and_an_m3u_are_followed_to_the_track_that_holds_the_file_system() {
        let d = dir("sheets");
        let bin = d.join("roms/psx/Some Game (Track 1).bin");
        write(&bin, &test_image(PS1_CNF, 2352));
        let cue = d.join("roms/psx/Some Game.cue");
        write(
            &cue,
            b"FILE \"Some Game (Track 1).bin\" BINARY\r\n  TRACK 01 MODE2/2352\r\n    INDEX 01 00:00:00\r\nFILE \"Some Game (Track 2).bin\" BINARY\r\n  TRACK 02 AUDIO\r\n    INDEX 00 00:00:00\r\n",
        );
        assert_eq!(serial_of(&cue).unwrap().as_deref(), Some("SLES55673"));
        let m3u = d.join("roms/psx/Some Game.m3u");
        write(&m3u, b"Some Game.cue\n");
        assert_eq!(serial_of(&m3u).unwrap().as_deref(), Some("SLES55673"));
        // A sheet whose track is not there is no serial, and no error.
        let lost = d.join("roms/psx/Lost.cue");
        write(&lost, b"FILE \"Gone.bin\" BINARY\n");
        assert_eq!(serial_of(&lost).unwrap(), None);
    }

    #[test]
    fn what_this_reader_does_not_open_has_no_serial() {
        let d = dir("unsupported");
        for (name, bytes) in [("Game.chd", b"MComprHD".as_slice()), ("Game.cso", b"CISO"), ("EBOOT.pbp", b"\0PBP")] {
            let p = d.join("roms/psx").join(name);
            write(&p, bytes);
            assert_eq!(serial_of(&p).unwrap(), None, "{name}");
        }
        // A file that is no ISO 9660 image (a text file, an empty one): no serial either.
        let junk = d.join("roms/psx/Game.iso");
        write(&junk, b"not a disc at all");
        assert_eq!(serial_of(&junk).unwrap(), None);
        assert!(serial_of(&d.join("roms/psx/gone.iso")).is_err(), "an unreadable image is tried again later");
    }

    #[test]
    fn a_disc_without_system_cnf_falls_back_to_its_volume_identifier() {
        // A PS1 image whose volume identifier is the serial (as many masters have it).
        let d = dir("volume");
        let mut sectors = iso_sectors("TCB = 4\n");
        sectors[16][40..72].fill(0);
        sectors[16][40..40 + 10].copy_from_slice(b"SLES-55673");
        let p = d.join("roms/psx/Game.iso");
        write(&p, &sectors.concat());
        assert_eq!(serial_of(&p).unwrap().as_deref(), Some("SLES55673"));
        // A volume identifier that is no serial: none.
        let mut sectors = iso_sectors("TCB = 4\n");
        sectors[16][40..72].fill(0);
        sectors[16][40..40 + 11].copy_from_slice(b"PLAYSTATION");
        let p = d.join("roms/psx/Other.iso");
        write(&p, &sectors.concat());
        assert_eq!(serial_of(&p).unwrap(), None);
    }

    // ---- the file name, and the name the shelf shows ------------------------------------------------------

    #[test]
    fn a_serial_in_front_of_a_title_is_left_out_of_the_file_name() {
        assert_eq!(clean_stem("SLES_556.73.Something"), "Something");
        assert_eq!(clean_stem("SLUS_209.46.Game (Europe)"), "Game (Europe)");
        assert_eq!(clean_stem("SLES-556.73-Crash Bandicoot"), "Crash Bandicoot");
        // Nothing behind it: the name stays as it is (it is all there is to show).
        assert_eq!(clean_stem("SLUS_209.46"), "SLUS_209.46");
        assert_eq!(clean_stem("SLUS_209.46."), "SLUS_209.46.");
        // Not a serial in front: left alone.
        assert_eq!(clean_stem("Tetris"), "Tetris");
        assert_eq!(clean_stem("Final Fantasy VII"), "Final Fantasy VII");
        assert_eq!(clean_stem("Super Mario 64"), "Super Mario 64");
        assert_eq!(clean_stem(""), "");
    }

    #[test]
    fn the_shelf_shows_the_lists_name_without_its_regions_but_with_its_disc() {
        assert_eq!(shelf_name("Final Fantasy VII (Europe)"), "Final Fantasy VII");
        assert_eq!(shelf_name("Final Fantasy VII (Europe) (Disc 2)"), "Final Fantasy VII (Disc 2)");
        assert_eq!(shelf_name("Metal Gear Solid (USA) (Rev 1)"), "Metal Gear Solid");
        assert_eq!(shelf_name("Legend of Zelda, The - A Link to the Past (USA)"), "The Legend of Zelda - A Link to the Past");
    }

    // ---- the list, and the cache -------------------------------------------------------------------------

    const PS1_DAT: &str = r#"clrmamepro (
	name "Sony - PlayStation"
)

game (
	name "Final Fantasy VII (Europe)"
	region "Europe"
	serial "SLES-55673"
	rom ( name "Final Fantasy VII (Europe) (Disc 1).bin" size 4 crc 11111111 serial "SLES-55673" )
)
game (
	name "Final Fantasy VII (Europe) (Disc 2)"
	region "Europe"
	serial "SLES-55674"
	rom ( name "Final Fantasy VII (Europe) (Disc 2).bin" size 4 crc 22222222 serial "SLES-55674" )
)
game (
	name "Crash Bandicoot (Europe)"
	region "Europe"
	serial "SCES-00344"
	rom ( name "Crash Bandicoot (Europe).bin" size 4 crc 33333333 serial "SCES-00344" )
)
game (
	name "Crash Bandicoot (USA)"
	region "USA"
	serial "SCUS-94900"
	rom ( name "Crash Bandicoot (USA).bin" size 4 crc 33333333 serial "SCUS-94900" )
)
"#;

    #[test]
    fn the_list_maps_a_serial_to_the_cleanest_release_name() {
        let t = parse_dat(PS1_DAT);
        assert_eq!(t.get("SLES55673").map(String::as_str), Some("Final Fantasy VII (Europe)"));
        assert_eq!(t.get("SCES00344").map(String::as_str), Some("Crash Bandicoot (Europe)"));
        assert!(parse_dat("<html>not a dat</html>").is_empty());
        // A serial two releases share (a re-release): the cleaner name wins.
        let shared = parse_dat("game (\n\tname \"Game (Europe) (Rev 1)\"\n\tserial \"SLES-12345\"\n)\ngame (\n\tname \"Game (Europe)\"\n\tserial \"SLES-12345\"\n)\n");
        assert_eq!(shared.get("SLES12345").map(String::as_str), Some("Game (Europe)"));
    }

    #[test]
    fn a_disc_named_by_its_serial_gets_the_name_the_list_knows() {
        let d = dir("names");
        let rom = d.join("roms/psx/SLES_556.73.Something.iso");
        write(&rom, &test_image(PS1_CNF, 2048));
        let games = vec![("psx".to_string(), rom.clone())];
        let asked = std::cell::RefCell::new(Vec::new());
        let fetch = |dat: &str| {
            asked.borrow_mut().push(dat.to_string());
            Some(PS1_DAT.to_string())
        };
        let run = run_until(&d, &games, &fetch, true, || false);
        assert_eq!(run.names.get(&rom).map(String::as_str), Some("Final Fantasy VII (Europe)"));
        assert_eq!((run.read, run.lists_missing, run.stopped), (1, false, false));
        assert_eq!(*asked.borrow(), vec!["Sony - PlayStation".to_string()]);
        assert_eq!(shelf_name(&run.names[&rom]), "Final Fantasy VII");
        // Later: from the card alone, no image read, nothing fetched.
        assert_eq!(known(&d, &games), run.names);
        let again = run_until(&d, &games, &|_| panic!("the list is on the SD card"), true, || false);
        assert_eq!((again.read, again.names), (0, run.names));
        // A changed image is read again, and one that left the library leaves the cache.
        let other = d.join("roms/psx/SCES_003.44.iso");
        write(&other, &test_image("BOOT = cdrom:\\SCES_003.44;1\n", 2048));
        let games = vec![("psx".to_string(), rom.clone()), ("psx".to_string(), other.clone())];
        let run = run_until(&d, &games, &|_| panic!("the list is on the SD card"), true, || false);
        assert_eq!((run.read, run.names[&other].as_str()), (1, "Crash Bandicoot (Europe)"));
        run_until(&d, &games[..1], &|_| panic!("the list is on the SD card"), true, || false);
        assert_eq!(Cache::load(&d).len(), 1);
    }

    #[test]
    fn without_the_list_the_disc_keeps_its_file_name() {
        let d = dir("offline");
        let rom = d.join("roms/psx/SLES_556.73.Something.iso");
        write(&rom, &test_image(PS1_CNF, 2048));
        let games = vec![("psx".to_string(), rom.clone())];
        // Offline: the serial is kept, no name, and the list is said to be missing.
        let run = run_until(&d, &games, &|_| None, true, || false);
        assert!(run.names.is_empty() && run.lists_missing);
        assert_eq!(run.read, 1);
        assert!(known(&d, &games).is_empty());
        // A download that is no DAT is not kept as the list.
        let run = run_until(&d, &games, &|_| Some("<html>sign in</html>".into()), true, || false);
        assert!(run.names.is_empty() && run.lists_missing && !table_path(&d, "Sony - PlayStation").exists());
        // Online later: named without reading the image again.
        let run = run_until(&d, &games, &|_| Some(PS1_DAT.into()), true, || false);
        assert_eq!((run.read, run.names.len(), run.lists_missing), (0, 1, false));
        // A serial no list knows: nothing.
        assert!(known(&d, &[("psx".to_string(), d.join("roms/psx/SCES_999.99.iso"))]).is_empty());
        // Another system is not the serial reader's.
        assert!(dat("gba").is_none() && dat("psx").is_some() && dat("ps2").is_some());
        assert!(known(&d, &[("gba".to_string(), rom.clone())]).is_empty());
    }

    #[test]
    fn a_game_starting_stops_the_run_and_keeps_what_it_did() {
        let d = dir("stop");
        let games: Vec<(String, PathBuf)> = ["SLES_556.73", "SCES_003.44", "SLUS_209.46"]
            .iter()
            .map(|serial| {
                let p = d.join(format!("roms/psx/{serial}.iso"));
                write(&p, &test_image(&format!("BOOT = cdrom:\\{serial};1\n"), 2048));
                ("psx".to_string(), p)
            })
            .collect();
        let n = std::cell::Cell::new(0);
        let run = run_until(&d, &games, &|_| None, true, || {
            n.set(n.get() + 1);
            n.get() > 2
        });
        assert!(run.stopped);
        assert_eq!((run.read, Cache::load(&d).len()), (2, 2), "two read and kept, none pruned");
        assert_eq!(run_until(&d, &games, &|_| None, true, || false).read, 1, "the rest after the game");
    }

    #[test]
    fn look_up_again_forgets_the_serial_and_only_that_disc() {
        let d = dir("forget");
        let (one, two) = (d.join("roms/psx/One.iso"), d.join("roms/psx/Two.iso"));
        write(&one, &test_image(PS1_CNF, 2048));
        write(&two, &test_image("BOOT = cdrom:\\SCES_003.44;1\n", 2048));
        let games = vec![("psx".to_string(), one.clone()), ("psx".to_string(), two.clone())];
        run_until(&d, &games, &|_| Some(PS1_DAT.into()), true, || false);
        forget(&d, &one);
        assert_eq!(Cache::load(&d).len(), 1, "only that disc's serial goes");
        assert_eq!(run_until(&d, &games, &|_| panic!("the list is on the SD card"), true, || false).read, 1);
        forget(&d, &d.join("roms/psx/Never.iso"));
    }

    #[test]
    fn a_named_disc_comes_off_the_missing_lists_once() {
        let d = dir("unlist");
        let rom = d.join("roms/psx/SLES_556.73.Something.iso");
        write(&rom, &test_image(PS1_CNF, 2048));
        let missing = d.join("covers/psx/.missing");
        write(&missing, b"Other\nSLES_556.73.Something\n");
        run_until(&d, &[("psx".to_string(), rom.clone())], &|_| Some(PS1_DAT.into()), true, || false);
        assert_eq!(std::fs::read_to_string(&missing).unwrap(), "Other\n");
    }
}
