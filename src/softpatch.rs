//! Soft-patching, as RetroArch does it (docs/user/patches-msu1.md, docs/HOST.md "Patches"): a fan translation or ROM
//! hack as an `.ips`, `.bps` or `.ups` file next to the game, with the game's name (`Game.sfc` and `Game.ips`), is
//! applied in memory when pocket-core starts the game, before the core sees it. The file on the card stays as it is.
//!
//! - Which patch: the first of `<name>.ips`, `<name>.bps`, `<name>.ups` that exists (RetroArch's order; the ending in
//!   lower or upper case), then `<name>.ips1`, `<name>.ips2`... of the same kind, applied in that order ([`find`]).
//!   `<name>` is the file's own name without its ending; for a zip that is the zip's name.
//! - The formats: IPS with its run-length records and the truncate extension, BPS and UPS with their three CRC-32
//!   checks ([`ips`], [`bps`], [`ups`]). A BPS or UPS made for another version of the game is refused before anything
//!   is written ([`Error::OtherVersion`]).
//! - A patch that doesn't apply stops the start with a sentence for the player ([`message`]); the game never starts
//!   unpatched without a word. pocket-ui shows it where a game that didn't start is shown (`notice`), and doesn't hand
//!   the game to RetroArch after it ([`is_refusal`], `fallback::should_retry`).
//! - Cores that take the game's bytes get the patched bytes ([`for_core`]). Cores that open the file themselves
//!   (`need_fullpath`) get a patched copy in `/tmp/pocket-core-patched-<pid>/` under the game's file name ([`TempDir`]),
//!   removed when the game ends; [`clean_stale`] removes what a killed run left. Disc images, playlists, folders and
//!   zips a core opens itself (arcade sets) are not patched: they are refused with the reason.
//! - Saves keep the ROM file's name, as in RetroArch: a hack next to its base game shares that game's save.
//! - RetroAchievements hashes the patched game, as RetroArch does (`achievements::hash_rom` calls [`patched_content`]):
//!   the unpatched game's hash is never sent for a patched one.

use std::path::{Path, PathBuf};

/// Largest patched game pocket-core builds (a BPS or UPS names its size; a bigger one is refused before allocating).
pub const MAX_OUTPUT: usize = 128 << 20;
/// Largest patch file read.
const MAX_PATCH: u64 = 64 << 20;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Ips,
    Bps,
    Ups,
}

impl Format {
    /// In the order RetroArch looks for them.
    pub const ALL: [Format; 3] = [Format::Ips, Format::Bps, Format::Ups];

    pub fn ext(self) -> &'static str {
        match self {
            Format::Ips => "ips",
            Format::Bps => "bps",
            Format::Ups => "ups",
        }
    }

    /// The format of a patch file by its ending, a stacked one's number left off ("ips2").
    pub fn of(path: &Path) -> Option<Format> {
        let ext = path.extension()?.to_str()?.trim_end_matches(|c: char| c.is_ascii_digit()).to_ascii_lowercase();
        Format::ALL.into_iter().find(|f| f.ext() == ext)
    }
}

/// Why a patch didn't apply.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// BPS, UPS: the game is not the one the patch was made for (its size or checksum differs).
    OtherVersion,
    /// Not a patch of its kind, cut short, a checksum of the patch or of the result that doesn't match, a copy from
    /// outside the game. The text is for the log.
    Damaged(&'static str),
    /// The patched game would be bigger than [`MAX_OUTPUT`].
    TooBig,
}

/// Applies `patch` of `format` to `source`.
pub fn apply(format: Format, patch: &[u8], source: &[u8]) -> Result<Vec<u8>, Error> {
    match format {
        Format::Ips => ips(patch, source),
        Format::Bps => bps(patch, source),
        Format::Ups => ups(patch, source),
    }
}

fn be(b: &[u8]) -> usize {
    b.iter().fold(0, |n, &x| n << 8 | x as usize)
}

/// IPS: "PATCH", records of a 3-byte offset, a 2-byte size and that many bytes (size 0: a 2-byte count and one byte
/// repeated, run-length), "EOF". Three more bytes after "EOF" are the size to cut the game to (Lunar IPS's truncate
/// extension). A record past the end makes the game longer, filled with zeros. IPS has no checksums.
pub fn ips(patch: &[u8], source: &[u8]) -> Result<Vec<u8>, Error> {
    let cut = Error::Damaged("the IPS patch is cut short");
    if !patch.starts_with(b"PATCH") {
        return Err(Error::Damaged("not an IPS patch"));
    }
    let mut out = source.to_vec();
    let mut at = 5;
    loop {
        let head = patch.get(at..at + 3).ok_or(cut)?;
        if head == b"EOF" {
            at += 3;
            break;
        }
        let offset = be(head);
        let size = be(patch.get(at + 3..at + 5).ok_or(cut)?);
        at += 5;
        let (len, bytes) = if size == 0 {
            let rle = patch.get(at..at + 3).ok_or(cut)?;
            at += 3;
            (be(&rle[..2]), None)
        } else {
            let bytes = patch.get(at..at + size).ok_or(cut)?;
            at += size;
            (size, Some(bytes))
        };
        let end = offset + len;
        if end > MAX_OUTPUT {
            return Err(Error::TooBig);
        }
        if end > out.len() {
            out.resize(end, 0);
        }
        match bytes {
            Some(b) => out[offset..end].copy_from_slice(b),
            None => out[offset..end].fill(patch[at - 1]),
        }
    }
    if let Some(size) = patch.get(at..at + 3).filter(|_| patch.len() == at + 3) {
        out.truncate(be(size));
    }
    Ok(out)
}

/// The variable-length numbers of BPS and UPS, read from a patch.
struct Reader<'a> {
    data: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn byte(&mut self) -> Result<u8, Error> {
        let b = *self.data.get(self.at).ok_or(Error::Damaged("the patch is cut short"))?;
        self.at += 1;
        Ok(b)
    }

    fn take(&mut self, n: usize) -> Result<&[u8], Error> {
        let end = self.at.checked_add(n).ok_or(Error::Damaged("the patch is cut short"))?;
        let b = self.data.get(self.at..end).ok_or(Error::Damaged("the patch is cut short"))?;
        self.at = end;
        Ok(b)
    }

    /// beat's number: 7 bits a byte, the high bit set on the last one, each further byte adding one more step.
    fn number(&mut self) -> Result<usize, Error> {
        let big = Error::Damaged("a number in the patch is too big");
        let (mut n, mut shift) = (0u64, 1u64);
        loop {
            let x = self.byte()?;
            n = ((x & 0x7f) as u64).checked_mul(shift).and_then(|v| n.checked_add(v)).ok_or(big)?;
            if x & 0x80 != 0 {
                return usize::try_from(n).map_err(|_| big);
            }
            shift = shift.checked_mul(128).ok_or(big)?;
            n = n.checked_add(shift).ok_or(big)?;
        }
    }

    /// A relative offset of BPS: the lowest bit is the sign.
    fn offset(&mut self) -> Result<(bool, usize), Error> {
        let n = self.number()?;
        Ok((n & 1 != 0, n >> 1))
    }
}

/// The three CRC-32s at the end of a BPS or UPS (game before, game after, the patch up to its own), and the part of
/// the patch before them. The patch's own checksum is checked here.
fn footer<'a>(patch: &'a [u8], magic: &[u8]) -> Result<(&'a [u8], u32, u32), Error> {
    if !patch.starts_with(magic) || patch.len() < magic.len() + 12 {
        return Err(Error::Damaged("not a patch of its kind"));
    }
    let le = |at: usize| u32::from_le_bytes(patch[at..at + 4].try_into().unwrap());
    let n = patch.len();
    if crc32fast::hash(&patch[..n - 4]) != le(n - 4) {
        return Err(Error::Damaged("the patch doesn't match its checksum"));
    }
    Ok((&patch[..n - 12], le(n - 12), le(n - 8)))
}

fn moved(at: usize, (back, by): (bool, usize)) -> Result<usize, Error> {
    if back { at.checked_sub(by) } else { at.checked_add(by) }.ok_or(Error::Damaged("the patch copies from outside the game"))
}

/// BPS (beat): sizes and metadata, then commands that copy from the game, from the patch, or from what is already
/// written; checksums of the game before and after and of the patch itself.
pub fn bps(patch: &[u8], source: &[u8]) -> Result<Vec<u8>, Error> {
    let (body, source_crc, target_crc) = footer(patch, b"BPS1")?;
    let mut r = Reader { data: body, at: 4 };
    let (source_size, target_size) = (r.number()?, r.number()?);
    let meta = r.number()?;
    r.take(meta)?;
    if source_size != source.len() || crc32fast::hash(source) != source_crc {
        return Err(Error::OtherVersion);
    }
    if target_size > MAX_OUTPUT {
        return Err(Error::TooBig);
    }
    let outside = Error::Damaged("the patch copies from outside the game");
    let mut out = Vec::with_capacity(target_size);
    let (mut from_source, mut from_target) = (0usize, 0usize);
    while r.at < body.len() {
        let n = r.number()?;
        let len = (n >> 2) + 1;
        if len > target_size - out.len() {
            return Err(Error::Damaged("the patch writes past the end of the game"));
        }
        match n & 3 {
            // SourceRead: the game's bytes where the output is.
            0 => {
                let at = out.len();
                out.extend_from_slice(source.get(at..at + len).ok_or(outside)?);
            }
            // TargetRead: bytes from the patch.
            1 => out.extend_from_slice(r.take(len)?),
            // SourceCopy: from anywhere in the game.
            2 => {
                from_source = moved(from_source, r.offset()?)?;
                out.extend_from_slice(source.get(from_source..from_source + len).ok_or(outside)?);
                from_source += len;
            }
            // TargetCopy: from what is written, byte by byte (it may overlap, as a run).
            _ => {
                from_target = moved(from_target, r.offset()?)?;
                for _ in 0..len {
                    let b = *out.get(from_target).ok_or(outside)?;
                    out.push(b);
                    from_target += 1;
                }
            }
        }
    }
    if out.len() != target_size || crc32fast::hash(&out) != target_crc {
        return Err(Error::Damaged("the patched game doesn't match the patch's checksum"));
    }
    Ok(out)
}

/// UPS: sizes, then runs of bytes to XOR into the game, each after a gap and ended by a zero; the same checksums as
/// BPS. Only forwards (the game the patch was made for, to the patched one), as RetroArch applies it.
pub fn ups(patch: &[u8], source: &[u8]) -> Result<Vec<u8>, Error> {
    let (body, source_crc, target_crc) = footer(patch, b"UPS1")?;
    let mut r = Reader { data: body, at: 4 };
    let (source_size, target_size) = (r.number()?, r.number()?);
    if source_size != source.len() || crc32fast::hash(source) != source_crc {
        return Err(Error::OtherVersion);
    }
    if target_size > MAX_OUTPUT {
        return Err(Error::TooBig);
    }
    let mut out = vec![0u8; target_size];
    let n = source.len().min(target_size);
    out[..n].copy_from_slice(&source[..n]);
    let mut pos = 0usize;
    while r.at < body.len() {
        pos = pos.checked_add(r.number()?).ok_or(Error::Damaged("a number in the patch is too big"))?;
        loop {
            let x = r.byte()?;
            // Bytes past the end of a game that shrinks are skipped, as in beat's reference; the checksum decides.
            if let Some(b) = out.get_mut(pos) {
                *b ^= x;
            }
            pos = pos.saturating_add(1);
            if x == 0 {
                break;
            }
        }
    }
    if crc32fast::hash(&out) != target_crc {
        return Err(Error::Damaged("the patched game doesn't match the patch's checksum"));
    }
    Ok(out)
}

/// The patches for the game in `rom`, in the order they are applied; empty when there is none. A few file checks,
/// no reading.
pub fn find(rom: &Path) -> Vec<PathBuf> {
    let (Some(dir), Some(stem)) = (rom.parent(), rom.file_stem().and_then(|s| s.to_str())) else {
        return Vec::new();
    };
    for f in Format::ALL {
        for ext in [f.ext().to_string(), f.ext().to_ascii_uppercase()] {
            let first = dir.join(format!("{stem}.{ext}"));
            if !first.is_file() {
                continue;
            }
            let mut list = vec![first];
            for i in 1.. {
                let next = dir.join(format!("{stem}.{ext}{i}"));
                if !next.is_file() {
                    break;
                }
                list.push(next);
            }
            return list;
        }
    }
    Vec::new()
}

/// A patch's file name, as the player sees it.
pub fn name(patch: &Path) -> String {
    patch.file_name().map_or_else(|| patch.display().to_string(), |n| n.to_string_lossy().into_owned())
}

/// The refusals' kinds: `message` makes the sentence, the snapshots (`notice:patch-<kind>`) show each.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    OtherVersion,
    Damaged,
    TooBig,
    /// A disc image, a playlist, a folder, a zip the core opens itself: nothing pocket-core can patch.
    Kind,
}

impl Refusal {
    pub fn named(kind: &str) -> Option<Refusal> {
        Some(match kind {
            "version" => Refusal::OtherVersion,
            "damaged" => Refusal::Damaged,
            "big" => Refusal::TooBig,
            "kind" => Refusal::Kind,
            _ => return None,
        })
    }
}

impl From<Error> for Refusal {
    fn from(e: Error) -> Refusal {
        match e {
            Error::OtherVersion => Refusal::OtherVersion,
            Error::Damaged(_) => Refusal::Damaged,
            Error::TooBig => Refusal::TooBig,
        }
    }
}

/// What follows the first sentence of each refusal: what the player can do.
const ADVICE: [&str; 4] = [
    "Use the version it was made for, or take the patch out of the folder to play the original.",
    "Copy it to the SD card again, or take it out of the folder to play the original.",
    "Patch the game on a computer instead.",
    "Patch the game on a computer, or take the patch out of the folder to play the original.",
];

/// The sentence for the player when `patch` (its file name) can't be applied.
pub fn message(kind: Refusal, patch: &str) -> String {
    match kind {
        Refusal::OtherVersion => format!("The patch {patch} is for another version of the game. {}", ADVICE[0]),
        Refusal::Damaged => format!("The patch {patch} is damaged or not a patch. {}", ADVICE[1]),
        Refusal::TooBig => format!("The patch {patch} makes the game too big for pocket-core. {}", ADVICE[2]),
        Refusal::Kind => format!("pocket-core can't apply the patch {patch} to this kind of game file. {}", ADVICE[3]),
    }
}

/// Whether `message` (why pocket-core didn't start a game) is a patch refusal: RetroArch would load the original
/// instead, so the game isn't handed to it (`fallback::should_retry`).
pub fn is_refusal(message: &str) -> bool {
    ADVICE.iter().any(|a| message.ends_with(a))
}

/// Endings of games that are not one file of the game's bytes: disc images and their sheets, playlists, ScummVM's
/// game files. A patch next to one is refused rather than written into a copy that the rest of the disc wouldn't fit.
const NOT_PATCHABLE: [&str; 16] = ["cue", "m3u", "chd", "gdi", "cdi", "ccd", "toc", "pbp", "cso", "iso", "img", "mds", "rvz", "wbfs", "scummvm", "zar"];

/// Whether pocket-core can patch the game in `content` (with a core of extensions `exts`).
fn patchable(content: &Path, exts: &[&str]) -> bool {
    let ext = content.extension().and_then(|e| e.to_str()).unwrap_or_default().to_ascii_lowercase();
    !content.is_dir() && !NOT_PATCHABLE.contains(&ext.as_str()) && !(ext == "zip" && exts.contains(&"zip"))
}

/// The game's own bytes: the file, or the game inside a zip (`archive::extract_rom`), with the path the core sees.
fn source(rom: &Path, exts: &[&str], first: &str) -> Result<(PathBuf, Vec<u8>), String> {
    if rom.extension().is_some_and(|e| e.eq_ignore_ascii_case("zip")) {
        let (name, data) = crate::archive::extract_rom(rom, exts)?;
        return Ok((rom.parent().unwrap_or(Path::new("")).join(name), data));
    }
    let size = std::fs::metadata(rom).map_err(|e| format!("{}: {e}", rom.display()))?.len();
    if size > MAX_OUTPUT as u64 {
        return Err(message(Refusal::TooBig, first));
    }
    Ok((rom.to_path_buf(), std::fs::read(rom).map_err(|e| format!("{}: {e}", rom.display()))?))
}

/// Applies `patches` to `data` in order; Err is the sentence for the player.
pub fn apply_files(patches: &[PathBuf], mut data: Vec<u8>) -> Result<Vec<u8>, String> {
    for p in patches {
        let shown = name(p);
        let damaged = || message(Refusal::Damaged, &shown);
        let format = Format::of(p).ok_or_else(damaged)?;
        let size = std::fs::metadata(p).map_err(|_| damaged())?.len();
        if size > MAX_PATCH {
            return Err(damaged());
        }
        let patch = std::fs::read(p).map_err(|_| damaged())?;
        data = apply(format, &patch, &data).map_err(|e| {
            eprintln!("softpatch: {shown}: {e:?}");
            message(e.into(), &shown)
        })?;
    }
    Ok(data)
}

/// The patched game for `rom`, for its hash (`achievements::hash_rom`): None when no patch is next to it, Err when
/// one doesn't apply (the game doesn't start then either).
pub fn patched_content(rom: &Path) -> Option<Result<(PathBuf, Vec<u8>), String>> {
    let patches = find(rom);
    let first = name(patches.first()?);
    Some((|| {
        if !patchable(rom, &[]) {
            return Err(message(Refusal::Kind, &first));
        }
        let (path, data) = source(rom, &[], &first)?;
        Ok((path, apply_files(&patches, data)?))
    })())
}

/// The folder in /tmp (RAM on ROCKNIX) with a patched copy for a core that opens the game itself; removed with
/// everything in it when dropped (the game ended, or it didn't load).
#[derive(Debug)]
pub struct TempDir(PathBuf);

const TEMP_PREFIX: &str = "pocket-core-patched-";

impl TempDir {
    fn new(base: &Path) -> std::io::Result<TempDir> {
        let dir = base.join(format!("{TEMP_PREFIX}{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir)?;
        Ok(TempDir(dir))
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Removes the patched copies in `base` of pocket-core runs that no longer run (killed, or ended through `exit`):
/// pocket-core's `--restore-system` after every game, and before a new copy is written.
pub fn clean_stale(base: &Path) {
    let Ok(list) = std::fs::read_dir(base) else { return };
    for e in list.flatten() {
        let name = e.file_name();
        let Some(pid) = name.to_str().and_then(|n| n.strip_prefix(TEMP_PREFIX)).and_then(|p| p.parse::<u32>().ok()) else {
            continue;
        };
        if pid != std::process::id() && !Path::new(&format!("/proc/{pid}")).exists() {
            let _ = std::fs::remove_dir_all(e.path());
        }
    }
}

/// The patched game as `libretro::Core::load` hands it to the core.
#[derive(Debug)]
pub struct Patched {
    /// The path the core gets: the game's own (or the file inside its zip), or the patched copy in /tmp for a core
    /// that opens the file itself.
    pub path: PathBuf,
    /// The patched bytes for a core that takes them; empty for one that opens the file.
    pub data: Vec<u8>,
    /// The folder of the patched copy, kept while the game runs.
    pub temp: Option<TempDir>,
    /// The patches applied, by file name, in order.
    pub names: Vec<String>,
}

/// Applies `patches` (from [`find`], not empty) to the game in `rom` for a core with extensions `exts`;
/// `need_fullpath`: the core opens the file itself, so it gets a patched copy in `temp_base`. Err is the sentence for
/// the player.
pub fn for_core(rom: &Path, patches: &[PathBuf], exts: &[&str], need_fullpath: bool, temp_base: &Path) -> Result<Patched, String> {
    let first = patches.first().map(|p| name(p)).unwrap_or_default();
    if !patchable(rom, exts) {
        return Err(message(Refusal::Kind, &first));
    }
    let (path, data) = source(rom, exts, &first)?;
    let data = apply_files(patches, data)?;
    let names = patches.iter().map(|p| name(p)).collect();
    if !need_fullpath {
        return Ok(Patched { path, data, temp: None, names });
    }
    clean_stale(temp_base);
    let file_name = path.file_name().map(|n| n.to_os_string()).unwrap_or_else(|| "game".into());
    let temp = TempDir::new(temp_base).map_err(|e| format!("{}: {e}", temp_base.display()))?;
    let copy = temp.path().join(file_name);
    std::fs::write(&copy, &data).map_err(|e| format!("{}: {e}", copy.display()))?;
    Ok(Patched { path: copy, data: Vec::new(), temp: Some(temp), names })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;

    // ---- writing small patches, as Floating IPS and beat would ----

    fn ips_record(offset: usize, bytes: &[u8]) -> Vec<u8> {
        let mut r = vec![(offset >> 16) as u8, (offset >> 8) as u8, offset as u8, (bytes.len() >> 8) as u8, bytes.len() as u8];
        r.extend_from_slice(bytes);
        r
    }

    fn ips_rle(offset: usize, count: usize, value: u8) -> Vec<u8> {
        vec![(offset >> 16) as u8, (offset >> 8) as u8, offset as u8, 0, 0, (count >> 8) as u8, count as u8, value]
    }

    fn ips_of(records: &[Vec<u8>], truncate: Option<usize>) -> Vec<u8> {
        let mut p = b"PATCH".to_vec();
        for r in records {
            p.extend_from_slice(r);
        }
        p.extend_from_slice(b"EOF");
        if let Some(t) = truncate {
            p.extend_from_slice(&[(t >> 16) as u8, (t >> 8) as u8, t as u8]);
        }
        p
    }

    fn number(mut n: u64) -> Vec<u8> {
        let mut out = Vec::new();
        loop {
            let x = (n & 0x7f) as u8;
            n >>= 7;
            if n == 0 {
                out.push(x | 0x80);
                return out;
            }
            out.push(x);
            n -= 1;
        }
    }

    fn with_footer(mut p: Vec<u8>, source: &[u8], target: &[u8]) -> Vec<u8> {
        p.extend_from_slice(&crc32fast::hash(source).to_le_bytes());
        p.extend_from_slice(&crc32fast::hash(target).to_le_bytes());
        let own = crc32fast::hash(&p);
        p.extend_from_slice(&own.to_le_bytes());
        p
    }

    /// A BPS of the given commands (already encoded) from `source` to `target`.
    fn bps_of(source: &[u8], target: &[u8], commands: &[u8]) -> Vec<u8> {
        let mut p = b"BPS1".to_vec();
        p.extend(number(source.len() as u64));
        p.extend(number(target.len() as u64));
        p.extend(number(3));
        p.extend_from_slice(b"m=1");
        p.extend_from_slice(commands);
        with_footer(p, source, target)
    }

    fn cmd(kind: u64, len: u64) -> Vec<u8> {
        number(((len - 1) << 2) | kind)
    }

    fn rel(back: bool, by: u64) -> Vec<u8> {
        number(by << 1 | back as u64)
    }

    /// The simplest UPS: one run XOR-ing every byte that differs, as beat's encoder writes it.
    fn ups_of(source: &[u8], target: &[u8]) -> Vec<u8> {
        let mut p = b"UPS1".to_vec();
        p.extend(number(source.len() as u64));
        p.extend(number(target.len() as u64));
        let at = |d: &[u8], i: usize| d.get(i).copied().unwrap_or(0);
        let len = source.len().max(target.len());
        let (mut i, mut last) = (0, 0);
        while i < len {
            if at(source, i) == at(target, i) {
                i += 1;
                continue;
            }
            p.extend(number((i - last) as u64));
            while i < len && at(source, i) != at(target, i) {
                p.push(at(source, i) ^ at(target, i));
                i += 1;
            }
            p.push(0);
            i += 1;
            last = i;
        }
        with_footer(p, source, target)
    }

    // ---- IPS ----

    #[test]
    fn ips_writes_its_records_and_grows_the_game() {
        let source = b"HELLO WORLD".to_vec();
        let p = ips_of(&[ips_record(0, b"J"), ips_record(6, b"THERE"), ips_record(14, b"!!")], None);
        assert_eq!(ips(&p, &source).unwrap(), b"JELLO THERE\0\0\0!!", "a record past the end fills the gap with zeros");
        assert_eq!(ips(b"PATCHEOF", &source).unwrap(), source, "an empty patch changes nothing");
    }

    #[test]
    fn ips_run_length_records_repeat_one_byte() {
        let p = ips_of(&[ips_rle(2, 5, 0xAA), ips_record(0, &[1])], None);
        assert_eq!(ips(&p, &[0; 10]).unwrap(), [1, 0, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0, 0, 0]);
        let grow = ips_of(&[ips_rle(4, 300, 7)], None);
        let out = ips(&grow, &[0; 4]).unwrap();
        assert_eq!(out.len(), 304);
        assert!(out[4..].iter().all(|b| *b == 7));
    }

    #[test]
    fn ips_truncates_with_lunar_ips_extension() {
        let p = ips_of(&[ips_record(1, b"Z")], Some(3));
        assert_eq!(ips(&p, b"ABCDEFG").unwrap(), b"AZC");
        // Three bytes after EOF and no more: only then is it a size (anything longer is junk and left alone).
        let mut junk = ips_of(&[], None);
        junk.extend_from_slice(b"1234");
        assert_eq!(ips(&junk, b"ABCDEFG").unwrap(), b"ABCDEFG");
    }

    #[test]
    fn ips_that_is_cut_short_or_no_ips_is_damaged() {
        assert!(matches!(ips(b"PATCH\x00\x00\x01\x00\x05ab", b"xxxx"), Err(Error::Damaged(_))), "record cut short");
        assert!(matches!(ips(b"PATCH\x00\x00\x01", b"xxxx"), Err(Error::Damaged(_))), "no EOF");
        assert!(matches!(ips(b"BPS1....", b"xxxx"), Err(Error::Damaged(_))));
        assert!(matches!(ips(b"", b"xxxx"), Err(Error::Damaged(_))));
    }

    #[test]
    fn ips_record_at_the_largest_offset_stays_in_bounds() {
        // The largest offset that isn't "EOF" plus the largest run: about 16 MB, well under MAX_OUTPUT.
        let p = ips_of(&[ips_rle(0xFF_FFFF, 0xFFFF, 1)], None);
        assert_eq!(ips(&p, b"").unwrap().len(), 0xFF_FFFF + 0xFFFF);
    }

    // ---- BPS ----

    #[test]
    fn bps_applies_all_four_commands() {
        let source = b"ABCDEFGHIJ";
        let target = b"ABCxyzHIJzzzzxyzABC";
        let mut c = Vec::new();
        c.extend(cmd(0, 3)); // SourceRead "ABC"
        c.extend(cmd(1, 3)); // TargetRead "xyz"
        c.extend_from_slice(b"xyz");
        c.extend(cmd(2, 3)); // SourceCopy "HIJ" from 7
        c.extend(rel(false, 7));
        c.extend(cmd(1, 1)); // TargetRead "z"
        c.push(b'z');
        c.extend(cmd(3, 3)); // TargetCopy from that z: "zzz", each byte one it has just written (a run)
        c.extend(rel(false, 9));
        c.extend(cmd(3, 3)); // TargetCopy "xyz": back from 12 to 3
        c.extend(rel(true, 9));
        c.extend(cmd(2, 3)); // SourceCopy "ABC": back from 10 to 0
        c.extend(rel(true, 10));
        let p = bps_of(source, target, &c);
        assert_eq!(bps(&p, source).unwrap(), target);
    }

    #[test]
    fn bps_for_another_version_is_refused_before_writing() {
        let (source, target) = (b"ORIGINAL".as_slice(), b"ORIGINAL!".as_slice());
        let mut c = cmd(0, 8);
        c.extend(cmd(1, 1));
        c.push(b'!');
        let p = bps_of(source, target, &c);
        assert_eq!(bps(&p, source).unwrap(), target);
        assert_eq!(bps(&p, b"ORIGINAX"), Err(Error::OtherVersion), "same size, other checksum");
        assert_eq!(bps(&p, b"ORIGINAL2"), Err(Error::OtherVersion), "other size");
    }

    #[test]
    fn bps_with_a_wrong_checksum_of_its_own_or_of_the_result_is_damaged() {
        let (source, target) = (b"ABCD".as_slice(), b"ABCE".as_slice());
        let mut c = cmd(0, 3);
        c.extend(cmd(1, 1));
        c.push(b'E');
        let mut p = bps_of(source, target, &c);
        let n = p.len();
        p[n - 13] ^= 1; // a byte of the commands: the patch's own checksum no longer fits
        assert_eq!(bps(&p, source), Err(Error::Damaged("the patch doesn't match its checksum")));
        // A patch whose result isn't what it promises: target checksum of other bytes, own checksum fixed up.
        let mut p = b"BPS1".to_vec();
        p.extend(number(4));
        p.extend(number(4));
        p.extend(number(0));
        p.extend(cmd(0, 4));
        let p = with_footer(p, source, target);
        assert_eq!(bps(&p, source), Err(Error::Damaged("the patched game doesn't match the patch's checksum")));
        assert!(matches!(bps(b"BPS1", source), Err(Error::Damaged(_))), "too short for its checksums");
    }

    #[test]
    fn bps_copies_from_outside_the_game_are_damaged_not_a_crash() {
        let source = b"ABCD";
        let target = b"ABCDABCD";
        // SourceCopy that starts before the game.
        let mut c = cmd(2, 4);
        c.extend(rel(true, 1));
        assert!(matches!(bps(&bps_of(source, target, &c), source), Err(Error::Damaged(_))));
        // TargetCopy of bytes not written yet.
        let mut c = cmd(3, 4);
        c.extend(rel(false, 0));
        assert!(matches!(bps(&bps_of(source, b"AAAA", &c), source), Err(Error::Damaged(_))));
        // A command longer than the game it makes.
        let c = cmd(0, 9);
        assert!(matches!(bps(&bps_of(source, target, &c), source), Err(Error::Damaged(_))));
        // SourceRead past the game's end (the game grows: there is no source byte there).
        let mut c = cmd(0, 4);
        c.extend(cmd(0, 4));
        assert!(matches!(bps(&bps_of(source, target, &c), source), Err(Error::Damaged(_))));
    }

    #[test]
    fn oversized_output_is_refused_before_allocating() {
        let source = b"tiny";
        // BPS naming a terabyte game.
        let mut p = b"BPS1".to_vec();
        p.extend(number(source.len() as u64));
        p.extend(number(1 << 40));
        p.extend(number(0));
        let p = with_footer(p, source, b"");
        assert_eq!(bps(&p, source), Err(Error::TooBig));
        // UPS one byte over the limit.
        let mut p = b"UPS1".to_vec();
        p.extend(number(source.len() as u64));
        p.extend(number(MAX_OUTPUT as u64 + 1));
        let p = with_footer(p, source, b"");
        assert_eq!(ups(&p, source), Err(Error::TooBig));
        // A number that doesn't fit at all.
        let mut p = b"BPS1".to_vec();
        p.extend([0u8; 12]);
        p.push(0x80);
        let p = with_footer(p, source, b"");
        assert!(matches!(bps(&p, source), Err(Error::Damaged(_))));
    }

    // ---- UPS ----

    #[test]
    fn ups_xors_its_runs_and_changes_the_size() {
        let source = b"The quick brown fox";
        for target in [b"The quick red fox!!".as_slice(), b"The slow".as_slice(), b"The quick brown fox jumps over".as_slice()] {
            let p = ups_of(source, target);
            assert_eq!(ups(&p, source).unwrap(), target, "{}", String::from_utf8_lossy(target));
        }
        assert_eq!(ups(&ups_of(source, source), source).unwrap(), source, "no runs");
    }

    #[test]
    fn ups_for_another_version_or_damaged_is_refused() {
        let (source, target) = (b"ABCDEFGH".as_slice(), b"ABCxEFGH".as_slice());
        let p = ups_of(source, target);
        assert_eq!(ups(&p, b"ABCDEFGI"), Err(Error::OtherVersion));
        assert_eq!(ups(&p, b"ABCDEFG"), Err(Error::OtherVersion));
        let mut bad = p.clone();
        bad[6] ^= 0x10;
        assert!(matches!(ups(&bad, source), Err(Error::Damaged(_))));
        assert!(matches!(ups(b"UPS1", source), Err(Error::Damaged(_))));
        // A run past the end of the game it makes is skipped, as in beat's reference; a result that isn't the promised
        // game is damaged.
        let mut p = b"UPS1".to_vec();
        p.extend(number(8));
        p.extend(number(8));
        p.extend(number(7));
        p.extend([1, 1, 0]);
        let p = with_footer(p, source, target);
        assert_eq!(ups(&p, source), Err(Error::Damaged("the patched game doesn't match the patch's checksum")));
        let mut p = b"UPS1".to_vec();
        p.extend(number(8));
        p.extend(number(4));
        p.extend(number(3));
        p.extend([b'D' ^ b'x', 9, 9, 0]);
        assert_eq!(ups(&with_footer(p, source, b"ABCx"), source).unwrap(), b"ABCx", "the game shrinks; bytes past it are skipped");
    }

    // ---- files next to the game ----

    fn scratch(name: &str) -> crate::testutil::TestDir {
        TestDir::new(&format!("softpatch-{name}"))
    }

    #[test]
    fn the_patch_has_the_games_name_and_ips_comes_first_then_its_stack() {
        let d = scratch("find");
        let rom = d.join("Game (USA).sfc");
        std::fs::write(&rom, b"x").unwrap();
        assert!(find(&rom).is_empty(), "no patch: nothing to do");
        std::fs::write(d.join("Game (USA).ups"), b"").unwrap();
        assert_eq!(find(&rom), [d.join("Game (USA).ups")]);
        std::fs::write(d.join("Game (USA).BPS"), b"").unwrap();
        assert_eq!(find(&rom), [d.join("Game (USA).BPS")], "BPS before UPS, the ending in capitals too");
        for n in ["Game (USA).ips", "Game (USA).ips1", "Game (USA).ips2", "Game (USA).ips4", "Other.ips"] {
            std::fs::write(d.join(n), b"").unwrap();
        }
        assert_eq!(find(&rom), [d.join("Game (USA).ips"), d.join("Game (USA).ips1"), d.join("Game (USA).ips2")], "the stack ends at the first gap");
        assert_eq!(find(&d.join("Game (USA).zip")), find(&rom), "a zip's patch has the zip's name");
        assert_eq!(Format::of(Path::new("a.ips2")), Some(Format::Ips));
        assert_eq!(Format::of(Path::new("a.BPS")), Some(Format::Bps));
        assert_eq!(Format::of(Path::new("a.txt")), None);
    }

    #[test]
    fn stacked_patches_apply_in_order() {
        let d = scratch("stack");
        let rom = d.join("Game.gb");
        std::fs::write(&rom, b"AAAA").unwrap();
        std::fs::write(d.join("Game.ips"), ips_of(&[ips_record(0, b"B")], None)).unwrap();
        std::fs::write(d.join("Game.ips1"), ips_of(&[ips_record(1, b"C")], None)).unwrap();
        let p = for_core(&rom, &find(&rom), &["gb"], false, &d).unwrap();
        assert_eq!(p.data, b"BCAA");
        assert_eq!(p.names, ["Game.ips", "Game.ips1"]);
        assert_eq!(p.path, rom, "a core that takes the bytes sees the game's own path");
        assert!(p.temp.is_none());
    }

    #[test]
    fn refusals_say_why_and_what_to_do() {
        assert_eq!(
            message(Refusal::OtherVersion, "Game.bps"),
            "The patch Game.bps is for another version of the game. Use the version it was made for, or take the patch out of the folder to play the original."
        );
        for kind in ["version", "damaged", "big", "kind"] {
            let m = message(Refusal::named(kind).unwrap(), "Game.ips");
            assert!(is_refusal(&m), "{m}");
            assert!(m.contains("Game.ips"), "{m}");
            assert!(!crate::fallback::should_retry(false, 1, false, Some(&m)), "RetroArch would start the original: {m}");
        }
        assert!(!is_refusal("The core couldn't load this game."));
    }

    #[test]
    fn a_patch_for_another_version_stops_the_start_with_its_reason() {
        let d = scratch("mismatch");
        let rom = d.join("Game.sfc");
        std::fs::write(&rom, b"USA 1.1").unwrap();
        let mut c = cmd(0, 7);
        c.extend(cmd(1, 1));
        c.push(b'!');
        std::fs::write(d.join("Game.bps"), bps_of(b"USA 1.0", b"USA 1.0!", &c)).unwrap();
        let err = for_core(&rom, &find(&rom), &["sfc"], false, &d).unwrap_err();
        assert_eq!(err, message(Refusal::OtherVersion, "Game.bps"));
        assert!(matches!(patched_content(&rom), Some(Err(_))), "no hash of the original for achievements");
        std::fs::write(d.join("Game.bps"), b"BPS1 not really").unwrap();
        assert_eq!(for_core(&rom, &find(&rom), &["sfc"], false, &d).unwrap_err(), message(Refusal::Damaged, "Game.bps"));
    }

    #[test]
    fn discs_folders_and_arcade_zips_are_not_patched_but_refused() {
        let d = scratch("kinds");
        for (game, exts) in [("Disc.cue", &["cue"][..]), ("Set.zip", &["zip"][..]), ("Playlist.m3u", &["m3u"][..])] {
            let rom = d.join(game);
            std::fs::write(&rom, b"x").unwrap();
            let patch = Path::new(game).with_extension("ips");
            std::fs::write(d.join(&patch), ips_of(&[], None)).unwrap();
            let err = for_core(&rom, &find(&rom), exts, true, &d).unwrap_err();
            assert_eq!(err, message(Refusal::Kind, &patch.to_string_lossy()), "{game}");
        }
        let folder = d.join("Monkey");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(d.join("Monkey.ips"), ips_of(&[], None)).unwrap();
        assert!(for_core(&folder, &find(&folder), &[], true, &d).is_err());
    }

    #[test]
    fn a_core_that_opens_the_file_gets_a_patched_copy_removed_afterwards() {
        let d = scratch("fullpath");
        let tmp = d.join("tmp");
        std::fs::create_dir_all(&tmp).unwrap();
        let rom = d.join("Game.n64");
        std::fs::write(&rom, b"ROM!").unwrap();
        std::fs::write(d.join("Game.ips"), ips_of(&[ips_record(3, b"?")], None)).unwrap();
        let p = for_core(&rom, &find(&rom), &["n64", "z64"], true, &tmp).unwrap();
        assert!(p.data.is_empty(), "the core reads the file");
        assert_eq!(p.path.file_name().unwrap(), "Game.n64", "the copy keeps the game's file name");
        assert!(p.path.starts_with(&tmp));
        assert_eq!(std::fs::read(&p.path).unwrap(), b"ROM?");
        assert_eq!(std::fs::read(&rom).unwrap(), b"ROM!", "the game on the SD card is never changed");
        let dir = p.temp.as_ref().unwrap().path().to_path_buf();
        drop(p);
        assert!(!dir.exists(), "the copy goes with the game");
        // What a killed run left: removed by the next clean-up; this run's own copy stays.
        let dead = tmp.join(format!("{TEMP_PREFIX}{}", u32::MAX - 1));
        std::fs::create_dir_all(&dead).unwrap();
        std::fs::write(dead.join("Game.n64"), b"old").unwrap();
        let mine = TempDir::new(&tmp).unwrap();
        clean_stale(&tmp);
        assert!(!dead.exists());
        assert!(mine.path().exists());
        drop(mine);
    }

    #[test]
    fn a_zipped_game_is_unpacked_then_patched() {
        let d = scratch("zip");
        let zip = d.join("Game.zip");
        // One stored entry, as `archive` reads it.
        let (name, data) = (b"Game (USA).gb", b"GBGB");
        let crc = crc32fast::hash(data);
        let mut z = Vec::new();
        let local = |z: &mut Vec<u8>| {
            z.extend_from_slice(&[0x50, 0x4b, 3, 4, 20, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
            z.extend_from_slice(&crc.to_le_bytes());
            z.extend_from_slice(&(data.len() as u32).to_le_bytes());
            z.extend_from_slice(&(data.len() as u32).to_le_bytes());
            z.extend_from_slice(&(name.len() as u16).to_le_bytes());
            z.extend_from_slice(&[0, 0]);
            z.extend_from_slice(name);
            z.extend_from_slice(data);
        };
        local(&mut z);
        let dir_at = z.len();
        z.extend_from_slice(&[0x50, 0x4b, 1, 2, 20, 0, 20, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        z.extend_from_slice(&crc.to_le_bytes());
        z.extend_from_slice(&(data.len() as u32).to_le_bytes());
        z.extend_from_slice(&(data.len() as u32).to_le_bytes());
        z.extend_from_slice(&(name.len() as u16).to_le_bytes());
        z.extend_from_slice(&[0; 12]);
        z.extend_from_slice(&0u32.to_le_bytes());
        z.extend_from_slice(name);
        let dir_len = z.len() - dir_at;
        z.extend_from_slice(&[0x50, 0x4b, 5, 6, 0, 0, 0, 0, 1, 0, 1, 0]);
        z.extend_from_slice(&(dir_len as u32).to_le_bytes());
        z.extend_from_slice(&(dir_at as u32).to_le_bytes());
        z.extend_from_slice(&[0, 0]);
        std::fs::write(&zip, z).unwrap();
        std::fs::write(d.join("Game.ips"), ips_of(&[ips_record(0, b"X")], None)).unwrap();
        let p = for_core(&zip, &find(&zip), &["gb"], false, &d).unwrap();
        assert_eq!(p.data, b"XBGB");
        assert_eq!(p.path, d.join("Game (USA).gb"), "the core sees the file inside, as without a patch");
        let (path, data) = patched_content(&zip).unwrap().unwrap();
        assert_eq!((path, data), (d.join("Game (USA).gb"), b"XBGB".to_vec()), "achievements hash the patched game");
    }

    #[test]
    fn no_patch_means_nothing_changes() {
        let d = scratch("none");
        let rom = d.join("Game.gba");
        std::fs::write(&rom, b"x").unwrap();
        assert!(patched_content(&rom).is_none());
    }
}
