//! Player covers are kept beside downloaded covers, keyed by the actual game file. The original is still available
//! when the player switches back to automatic, even if another game in the system has the same file stem.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

pub const FOLDER: &str = ".custom";
pub const MAX_BYTES: u64 = 8 << 20;
pub const MAX_SIDE: u32 = 1024;

const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";
const PNG_END: &[u8] = b"\0\0\0\0IEND\xaeB\x60\x82";

/// Validate the file and its dimensions without decoding pixels on the web worker. The browser does the resizing.
pub fn valid_png(bytes: &[u8]) -> bool {
    if bytes.len() < 57 || bytes.len() as u64 > MAX_BYTES
        || !bytes.starts_with(PNG_SIGNATURE) || !bytes.ends_with(PNG_END)
        || &bytes[8..16] != b"\0\0\0\rIHDR"
    {
        return false;
    }
    let w = u32::from_be_bytes(bytes[16..20].try_into().unwrap());
    let h = u32::from_be_bytes(bytes[20..24].try_into().unwrap());
    if !(1..=MAX_SIDE).contains(&w) || !(1..=MAX_SIDE).contains(&h) {
        return false;
    }

    let mut rest = &bytes[PNG_SIGNATURE.len()..];
    let mut has_pixels = false;
    while rest.len() >= 12 {
        let len = u32::from_be_bytes(rest[..4].try_into().unwrap()) as usize;
        if len > rest.len() - 12 {
            return false;
        }
        let (chunk, remaining) = rest.split_at(len + 12);
        let crc = u32::from_be_bytes(chunk[len + 8..].try_into().unwrap());
        if crc32fast::hash(&chunk[4..len + 8]) != crc {
            return false;
        }
        match &chunk[4..8] {
            b"IDAT" => has_pixels |= len > 0,
            b"IEND" => return has_pixels && len == 0 && remaining.is_empty(),
            _ => {}
        }
        rest = remaining;
    }
    false
}

/// Why a picture chosen on the device (Android's photo picker, `app_cover_pick`) is not taken as a cover. Its words are
/// what the card says; the cover stays as it was.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refused {
    /// Over [`MAX_BYTES`], the web companion's limit for a cover.
    TooBig,
    /// Not a PNG or JPEG, or one that doesn't read to the end (a HEIC, a WebP, a GIF, a broken file).
    NotAPicture,
    /// More pixels than any picture pocket-ui reads (`image::size_ok`): a camera's 50-megapixel photo.
    TooManyPixels,
}

impl Refused {
    pub fn message(self) -> &'static str {
        match self {
            Refused::TooBig => "That picture is over 8 MB. Pick a smaller one.",
            Refused::NotAPicture => "Couldn't read that picture. Pick a PNG or JPEG.",
            Refused::TooManyPixels => "That picture is too large. Pick a smaller resolution.",
        }
    }
}

/// The width and height a PNG or JPEG says it has, read before any pixel (`image::size_ok` decides then).
fn picture_size(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.starts_with(PNG_SIGNATURE) {
        let head = bytes.get(16..24)?;
        return Some((u32::from_be_bytes(head[..4].try_into().ok()?), u32::from_be_bytes(head[4..].try_into().ok()?)));
    }
    if !bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        return None;
    }
    let mut decoder = jpeg_decoder::Decoder::new(io::Cursor::new(bytes));
    decoder.read_info().ok()?;
    decoder.info().map(|i| (u32::from(i.width), u32::from(i.height)))
}

/// A picture chosen on the device as this game's cover: a PNG or JPEG of up to [`MAX_BYTES`], scaled down to
/// [`MAX_SIDE`] on its longest side as the web companion's browser does, and given back as the PNG [`valid_png`] takes,
/// ready for [`apply`].
pub fn from_picture(bytes: &[u8]) -> Result<Vec<u8>, Refused> {
    if bytes.len() as u64 > MAX_BYTES {
        return Err(Refused::TooBig);
    }
    let (w, h) = picture_size(bytes).ok_or(Refused::NotAPicture)?;
    if !crate::image::size_ok(w, h) {
        return Err(Refused::TooManyPixels);
    }
    let img = if bytes.starts_with(PNG_SIGNATURE) {
        crate::image::read_png(io::Cursor::new(bytes))
    } else {
        crate::image::read_jpeg(io::Cursor::new(bytes))
    }
    .filter(|i| i.w > 0 && i.h > 0 && i.px.len() == i.w as usize * i.h as usize)
    .ok_or(Refused::NotAPicture)?;
    let img = if img.w > MAX_SIDE || img.h > MAX_SIDE {
        let (w, h) = img.fit(MAX_SIDE as i32, MAX_SIDE as i32);
        img.resized(w, h)
    } else {
        img
    };
    let rgb: Vec<u8> = img.px.iter().flat_map(|p| [(p >> 16) as u8, (p >> 8) as u8, *p as u8]).collect();
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, img.w, img.h);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        let mut writer = enc.write_header().map_err(|_| Refused::NotAPicture)?;
        writer.write_image_data(&rgb).map_err(|_| Refused::NotAPicture)?;
    }
    // A photo's PNG at 1024 pixels is far below the limit; one that isn't is refused as the web companion would.
    if !valid_png(&out) {
        return Err(Refused::TooBig);
    }
    Ok(out)
}

pub fn path(data: &Path, system: &str, rom: &Path) -> PathBuf {
    // Keyed by the game's own system: a game in `genesis/` keeps its cover on the merged Mega Drive shelf (`region_pairs`).
    let system = crate::region_pairs::own_system(system, rom);
    let mut key = Sha256::new();
    key.update(system.as_bytes());
    key.update([0]);
    key.update(rom.as_os_str().as_encoded_bytes());
    data.join("covers").join(FOLDER).join(format!("{:x}.png", key.finalize()))
}

pub fn existing(data: &Path, system: &str, rom: &Path) -> Option<PathBuf> {
    let covers = data.join("covers");
    if ![covers.join(FOLDER), covers].iter().all(|p| fs::symlink_metadata(p).is_ok_and(|m| m.is_dir())) {
        return None;
    }
    let file = path(data, system, rom);
    fs::symlink_metadata(&file).ok()
        .filter(|m| m.is_file() && (1..=MAX_BYTES).contains(&m.len()))
        .map(|_| file)
}

fn plain_dir(path: &Path) -> io::Result<()> {
    match fs::create_dir(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists && fs::symlink_metadata(path)?.is_dir() => Ok(()),
        Err(e) => Err(e),
    }
}

/// Must run on the App's event loop. When there's a previous custom cover, keep it before replacing or removing it.
pub fn apply(data: &Path, system: &str, rom: &Path, png: Option<&[u8]>) -> io::Result<()> {
    let target = path(data, system, rom);
    let dir = target.parent().unwrap();
    let covers = dir.parent().unwrap();
    if png.is_none() && !covers.exists() {
        return Ok(());
    }
    plain_dir(covers)?;
    if png.is_none() && !dir.exists() {
        return Ok(());
    }
    plain_dir(dir)?;

    match fs::symlink_metadata(&target) {
        Ok(meta) if meta.is_file() && meta.len() <= MAX_BYTES => {
            let backups = dir.join("backups");
            plain_dir(&backups)?;
            let now = crate::recents::now_secs();
            let mut seq = 0;
            // A successful copy remains even if the write fails. Retry with a fresh name if the clock went back.
            loop {
                let name = format!("{}-{now}-{seq}.png", target.file_stem().unwrap().to_string_lossy());
                let backup = backups.join(name);
                if png.is_none() {
                    match crate::fsutil::rename_no_replace(&target, &backup) {
                        Ok(()) => {
                            crate::fsutil::sync_dir(&backups);
                            crate::fsutil::sync_dir(dir);
                            return Ok(());
                        }
                        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
                        Err(e) => return Err(e),
                    }
                } else if crate::fsutil::copy_if_absent(&target, &backup)? {
                    crate::fsutil::sync_dir(&backups);
                    crate::fsutil::sync_dir(dir);
                    break;
                }
                seq += 1;
            }
        }
        Ok(_) => return Err(io::Error::from(io::ErrorKind::InvalidData)),
        Err(e) if e.kind() == io::ErrorKind::NotFound && png.is_none() => return Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }
    // A reset has already moved the old file, or found that there is nothing to reset.
    png.map_or(Ok(()), |bytes| crate::fsutil::write_atomic(&target, bytes))
}

#[cfg(test)]
#[path = "custom_tests.rs"]
mod tests;
