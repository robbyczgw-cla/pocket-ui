//! The Brick's cheat database as ONE file (`app/cores/cht.pack`), built by `scripts/brick-cheats-data.py`.
//!
//! The same `.cht` files RetroArch reads, 23,000 of them, were 837 MB on the card's FAT (32 KB clusters) and made an
//! update take ten minutes. Here they are one file of about 35 MB; a lookup reads one folder's index and the few
//! entries that match, never the whole file. Every entry is the unchanged `.cht` text, deflated on its own and checked
//! against its CRC-32 when read.
//!
//! Layout (little endian):
//! ```text
//! "PUICHT01"
//! u32 folders
//! per folder: u16 name_len, name, u32 entries, u32 index_len, u64 index_offset
//! per folder index (at index_offset): per entry
//!     u16 name_len, name (the file name with ".cht"), u64 offset, u32 packed_len, u32 len, u32 crc32
//! the entries' zlib streams (at their offsets)
//! ```

use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

const MAGIC: &[u8; 8] = b"PUICHT01";
/// A `.cht` file is a few KB (the largest upstream file is about 1 MB); anything bigger is not one.
const MAX_ENTRY: u32 = 16 << 20;
/// One folder's index: 6,000 entries of about 100 bytes at most.
const MAX_INDEX: u32 = 8 << 20;
const MAX_FOLDERS: u32 = 4096;

/// One `.cht` file inside a pack.
#[derive(Clone, Debug)]
pub struct Entry {
    pub pack: PathBuf,
    /// The file name, e.g. `Tetris (World).cht`.
    pub name: String,
    offset: u64,
    packed: u32,
    len: u32,
    crc: u32,
}

fn u16_at(b: &[u8], at: usize) -> Option<usize> {
    Some(u16::from_le_bytes(b.get(at..at + 2)?.try_into().ok()?) as usize)
}
fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}
fn u64_at(b: &[u8], at: usize) -> Option<u64> {
    Some(u64::from_le_bytes(b.get(at..at + 8)?.try_into().ok()?))
}

/// The entries of database folder `folder` (e.g. "Nintendo - Game Boy"); `None` when `pack` is no readable pack or has
/// no such folder.
pub fn list(pack: &Path, folder: &str) -> Option<Vec<Entry>> {
    let mut f = std::fs::File::open(pack).ok()?;
    let mut head = [0u8; 12];
    f.read_exact(&mut head).ok()?;
    if &head[..8] != MAGIC {
        return None;
    }
    let folders = u32_at(&head, 8)?;
    if folders > MAX_FOLDERS {
        return None;
    }
    // The folder table is small (26 folders); read it entry by entry.
    for _ in 0..folders {
        let mut n = [0u8; 2];
        f.read_exact(&mut n).ok()?;
        let mut rest = vec![0u8; u16::from_le_bytes(n) as usize + 16];
        f.read_exact(&mut rest).ok()?;
        let nl = rest.len() - 16;
        if &rest[..nl] != folder.as_bytes() {
            continue;
        }
        let (count, ilen, ioff) = (u32_at(&rest, nl)?, u32_at(&rest, nl + 4)?, u64_at(&rest, nl + 8)?);
        if ilen > MAX_INDEX {
            return None;
        }
        f.seek(SeekFrom::Start(ioff)).ok()?;
        let mut index = vec![0u8; ilen as usize];
        f.read_exact(&mut index).ok()?;
        let mut out = Vec::with_capacity(count as usize);
        let mut at = 0;
        for _ in 0..count {
            let nl = u16_at(&index, at)?;
            let name = String::from_utf8_lossy(index.get(at + 2..at + 2 + nl)?).into_owned();
            at += 2 + nl;
            out.push(Entry {
                pack: pack.to_path_buf(),
                name,
                offset: u64_at(&index, at)?,
                packed: u32_at(&index, at + 8)?,
                len: u32_at(&index, at + 12)?,
                crc: u32_at(&index, at + 16)?,
            });
            at += 20;
        }
        return Some(out);
    }
    None
}

/// The text of one entry, `None` when the pack is cut off or the entry fails its length or checksum.
pub fn read(e: &Entry) -> Option<Vec<u8>> {
    if e.len > MAX_ENTRY || e.packed > MAX_ENTRY {
        return None;
    }
    let mut f = std::fs::File::open(&e.pack).ok()?;
    f.seek(SeekFrom::Start(e.offset)).ok()?;
    let mut packed = vec![0u8; e.packed as usize];
    f.read_exact(&mut packed).ok()?;
    let bytes = miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(&packed, e.len as usize).ok()?;
    (bytes.len() == e.len as usize && crc32fast::hash(&bytes) == e.crc).then_some(bytes)
}

/// A pack with `folders` of `(file name, text)`, for tests (the real one is made by `brick-cheats-data.py`).
#[cfg(test)]
pub fn build(folders: &[(&str, Vec<(String, Vec<u8>)>)]) -> Vec<u8> {
    let table: usize = folders.iter().map(|(n, _)| 2 + n.len() + 16).sum();
    let mut out = MAGIC.to_vec();
    out.extend((folders.len() as u32).to_le_bytes());
    let mut blobs: Vec<u8> = Vec::new();
    let mut indexes: Vec<Vec<u8>> = Vec::new();
    let mut packed_all: Vec<Vec<(u64, u32, u32, u32)>> = Vec::new();
    for (_, files) in folders {
        let mut ents = Vec::new();
        for (_, text) in files {
            let z = miniz_oxide::deflate::compress_to_vec_zlib(text, 6);
            ents.push((blobs.len() as u64, z.len() as u32, text.len() as u32, crc32fast::hash(text)));
            blobs.extend(z);
        }
        packed_all.push(ents);
    }
    let mut index_total = 0usize;
    for ((_, files), ents) in folders.iter().zip(&packed_all) {
        let mut ix = Vec::new();
        for ((name, _), (off, pl, l, crc)) in files.iter().zip(ents) {
            ix.extend((name.len() as u16).to_le_bytes());
            ix.extend(name.as_bytes());
            // Offsets of the blobs are made absolute below, once the index sizes are known.
            ix.extend(off.to_le_bytes());
            ix.extend(pl.to_le_bytes());
            ix.extend(l.to_le_bytes());
            ix.extend(crc.to_le_bytes());
        }
        index_total += ix.len();
        indexes.push(ix);
    }
    let blob_base = (12 + table + index_total) as u64;
    let mut at = (12 + table) as u64;
    for ((name, files), ix) in folders.iter().zip(&indexes) {
        out.extend((name.len() as u16).to_le_bytes());
        out.extend(name.as_bytes());
        out.extend((files.len() as u32).to_le_bytes());
        out.extend((ix.len() as u32).to_le_bytes());
        out.extend(at.to_le_bytes());
        at += ix.len() as u64;
    }
    for ((_, files), (ix, ents)) in folders.iter().zip(indexes.iter().zip(&packed_all)) {
        let mut ix = ix.clone();
        let mut p = 0;
        for ((name, _), (off, ..)) in files.iter().zip(ents) {
            p += 2 + name.len();
            ix[p..p + 8].copy_from_slice(&(blob_base + off).to_le_bytes());
            p += 20;
        }
        out.extend(ix);
    }
    out.extend(blobs);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files() -> Vec<(&'static str, Vec<(String, Vec<u8>)>)> {
        vec![
            ("Nintendo - Game Boy", vec![("Tetris (World).cht".into(), b"cheat0_desc = \"A\"\n".to_vec()), ("Zelda.cht".into(), vec![b'x'; 5000])]),
            ("Sega - Saturn", vec![("Nights.cht".into(), b"n".to_vec())]),
        ]
    }

    #[test]
    fn entries_come_back_unchanged_by_folder() {
        let dir = crate::testutil::TestDir::new("cheat-pack");
        let path = dir.join("cht.pack");
        std::fs::write(&path, build(&files())).unwrap();
        let gb = list(&path, "Nintendo - Game Boy").unwrap();
        assert_eq!(gb.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(), ["Tetris (World).cht", "Zelda.cht"]);
        assert_eq!(read(&gb[0]).unwrap(), b"cheat0_desc = \"A\"\n");
        assert_eq!(read(&gb[1]).unwrap(), vec![b'x'; 5000]);
        let saturn = list(&path, "Sega - Saturn").unwrap();
        assert_eq!(read(&saturn[0]).unwrap(), b"n");
        assert!(list(&path, "Sega - Dreamcast").is_none(), "a folder the pack does not have");
    }

    #[test]
    fn a_damaged_or_cut_off_pack_gives_nothing_instead_of_wrong_text() {
        let dir = crate::testutil::TestDir::new("cheat-pack-bad");
        let good = build(&files());
        let path = dir.join("cht.pack");
        // Not a pack at all, and a missing file.
        std::fs::write(&path, b"garbage garbage").unwrap();
        assert!(list(&path, "Nintendo - Game Boy").is_none());
        assert!(list(&dir.join("nope.pack"), "x").is_none());
        // Cut off in the middle of the entries: the index is there, the text of the last entry is not.
        std::fs::write(&path, &good[..good.len() - 4]).unwrap();
        let sat = list(&path, "Sega - Saturn").unwrap();
        assert!(read(&sat[0]).is_none());
        // One flipped byte in a stored entry fails the checksum (or the inflate), never returns altered text.
        let mut bad = good.clone();
        let n = bad.len() - 3;
        bad[n] ^= 0x55;
        std::fs::write(&path, &bad).unwrap();
        let sat = list(&path, "Sega - Saturn").unwrap();
        assert!(read(&sat[0]).is_none());
    }
}
