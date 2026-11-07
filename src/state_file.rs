//! Durable automatic states, compressed outside the legacy namespace. Manual/undo/Earlier files stay raw so older
//! players can still use them. Old players never open `.pstate.auto.z`; their quit cannot replace the saved moment.
//! Linux (including Deck and shared handheld players) and Android use these same readers and writers.

use std::io::{self, Read};
use std::path::{Path, PathBuf};

const MAGIC: &[u8; 8] = b"PCKSTZ\x01\0";
const HEADER: usize = 32;
pub const MAX_STATE: usize = 256 * 1024 * 1024;
const MAX_FILE: usize = MAX_STATE + MAX_STATE / 16 + HEADER;

fn bad(why: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, why)
}

/// The old name of an automatic state (also its taken/retry names, preview and Android mark).
pub fn legacy_path(path: &Path) -> Option<PathBuf> {
    use std::os::unix::ffi::{OsStrExt, OsStringExt};
    let bytes = path.file_name()?.as_bytes();
    let needle = b".pstate.auto.z";
    let at = bytes.windows(needle.len()).rposition(|w| w == needle)?;
    let rest = &bytes[at + needle.len()..];
    let state_rest = rest.strip_suffix(b".png").unwrap_or(rest);
    let kept = state_rest.strip_prefix(b".kept").is_some_and(|n| !n.is_empty() && n.iter().all(u8::is_ascii_digit));
    if ![b"".as_slice(), b".taken", b".taken.retry"].contains(&state_rest)
        && !kept && rest != b".mark" && rest != b".battery-unsaved" {
        return None;
    }
    let mut raw = bytes[..at + needle.len() - 2].to_vec();
    raw.extend_from_slice(rest);
    Some(path.with_file_name(std::ffi::OsString::from_vec(raw)))
}

/// Existing compressed name first, then the old raw name. Does not guess formats from arbitrary core bytes.
pub fn resolve(path: &Path) -> PathBuf {
    if path.exists() { path.to_path_buf() } else { legacy_path(path).filter(|p| p.exists()).unwrap_or_else(|| path.to_path_buf()) }
}

pub fn exists(path: &Path) -> bool {
    resolve(path).is_file()
}

/// Whether `path` starts with this player's own compressed header (magic and version), whatever follows. A read that fails
/// on such a file is damage; a file of another version (a newer player's, after a rollback) is not this one's to judge.
pub fn is_own_format(path: &Path) -> bool {
    let mut head = [0u8; MAGIC.len()];
    std::fs::File::open(path).and_then(|mut f| f.read_exact(&mut head)).is_ok() && &head == MAGIC
}

fn encode(raw: &[u8]) -> io::Result<Vec<u8>> {
    if raw.is_empty() || raw.len() > MAX_STATE {
        return Err(bad("save state size is unsupported"));
    }
    let packed = miniz_oxide::deflate::compress_to_vec(raw, 1);
    let mut file = Vec::with_capacity(HEADER + packed.len());
    file.extend_from_slice(MAGIC);
    file.extend_from_slice(&(raw.len() as u64).to_le_bytes());
    file.extend_from_slice(&(packed.len() as u64).to_le_bytes());
    file.extend_from_slice(&crc32fast::hash(raw).to_le_bytes());
    file.extend_from_slice(&crc32fast::hash(&packed).to_le_bytes());
    file.extend_from_slice(&packed);
    Ok(file)
}

fn decode(file: &[u8]) -> io::Result<Vec<u8>> {
    if file.len() < HEADER || &file[..8] != MAGIC {
        return Err(bad("compressed save state header is damaged or unsupported"));
    }
    let raw_len = u64::from_le_bytes(file[8..16].try_into().unwrap());
    let packed_len = u64::from_le_bytes(file[16..24].try_into().unwrap());
    if raw_len == 0 || raw_len > MAX_STATE as u64 || packed_len != (file.len() - HEADER) as u64 {
        return Err(bad("compressed save state size is damaged"));
    }
    let packed = &file[HEADER..];
    if crc32fast::hash(packed) != u32::from_le_bytes(file[28..32].try_into().unwrap()) {
        return Err(bad("compressed save state checksum failed"));
    }
    let raw = miniz_oxide::inflate::decompress_to_vec_with_limit(packed, raw_len as usize)
        .map_err(|_| bad("compressed save state is damaged or truncated"))?;
    if raw.len() as u64 != raw_len || crc32fast::hash(&raw) != u32::from_le_bytes(file[24..28].try_into().unwrap()) {
        return Err(bad("save state checksum failed"));
    }
    Ok(raw)
}

/// Reads Auto or a raw slot. A damaged compressed file is never passed to a core as raw bytes.
pub fn read(path: &Path) -> io::Result<Vec<u8>> {
    let actual = resolve(path);
    let mut file = std::fs::File::open(&actual)?;
    if file.metadata()?.len() > MAX_FILE as u64 {
        return Err(bad("save state is too large"));
    }
    let mut bytes = Vec::new();
    file.by_ref().take(MAX_FILE as u64 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > MAX_FILE {
        return Err(bad("save state is too large"));
    }
    unpack(&actual, bytes)
}

/// Decodes bytes read from one already-open inode (fallback copies keep its matching timestamp).
pub fn unpack(path: &Path, bytes: Vec<u8>) -> io::Result<Vec<u8>> {
    if legacy_path(path).is_some() { decode(&bytes) } else { Ok(bytes) }
}

/// Atomic state write, retaining file and directory fsync. Only the new Auto namespace is compressed.
pub fn write(path: &Path, raw: &[u8]) -> io::Result<()> {
    if legacy_path(path).is_some() {
        let encoded = encode(raw)?;
        crate::trace::step("save: state compressed");
        crate::fsutil::write_atomic(path, &encoded)
    } else {
        crate::fsutil::write_atomic(path, raw)
    }
}

/// Moves a moment, converting when it crosses the raw/compressed namespace (Android's set-aside slots, old Auto).
/// A failed conversion leaves the source intact. Preserve its capture time, including for battery-save protection.
pub fn move_file(from: &Path, to: &Path) -> io::Result<()> {
    let from = resolve(from);
    if legacy_path(&from).is_some() == legacy_path(to).is_some() {
        return std::fs::rename(from, to);
    }
    let modified = std::fs::metadata(&from)?.modified()?;
    write(to, &read(&from)?)?;
    use std::os::unix::fs::OpenOptionsExt;
    let out = std::fs::File::options().write(true).custom_flags(libc::O_NOFOLLOW).open(to)?;
    out.set_modified(modified)?;
    out.sync_all()?;
    std::fs::remove_file(from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;

    #[test]
    fn automatic_moment_round_trips_and_old_raw_states_still_load() {
        let d = TestDir::new("state-codec-roundtrip");
        let auto = d.join("Game.pstate.auto.z");
        let raw = (0..400_000).map(|i| (i % 173) as u8).collect::<Vec<_>>();
        write(&auto, &raw).unwrap();
        assert!(std::fs::metadata(&auto).unwrap().len() < raw.len() as u64 / 5);
        assert_eq!(read(&auto).unwrap(), raw);
        let kept = std::fs::read(&auto).unwrap();
        crate::fsutil::fail_next_write();
        assert!(write(&auto, b"newer progress on a full card").is_err());
        assert_eq!(std::fs::read(&auto).unwrap(), kept, "a failed durable write cannot replace the previous moment");
        let old = legacy_path(&auto).unwrap();
        std::fs::remove_file(&auto).unwrap();
        std::fs::write(&old, &raw).unwrap();
        assert_eq!(read(&auto).unwrap(), raw, "an upgrade reads raw Auto too");
        let slot = d.join("Game.pstate");
        write(&slot, &raw).unwrap();
        assert_eq!(std::fs::read(&slot).unwrap(), raw, "manual slots remain readable by older players");
        let named = d.join("Game.pstate.auto.z.pstate");
        write(&named, &raw).unwrap();
        assert_eq!(std::fs::read(named).unwrap(), raw, "a game name containing the format suffix is still a raw manual slot");
    }

    #[test]
    fn damaged_or_truncated_moment_is_rejected_before_a_core_can_see_it() {
        let d = TestDir::new("state-codec-corrupt");
        let auto = d.join("Game.pstate.auto.z");
        let good = encode(&vec![41; 200_000]).unwrap();
        for cut in [0, 1, 7, 8, 16, 24, 31, 32, good.len() - 1] {
            std::fs::write(&auto, &good[..cut]).unwrap();
            assert!(read(&auto).is_err(), "cut at {cut}");
        }
        for at in [0, 7, 8, 15, 16, 23, 24, 28, 32, good.len() - 1] {
            let mut corrupt = good.clone(); corrupt[at] ^= 0x80;
            std::fs::write(&auto, &corrupt).unwrap();
            assert!(read(&auto).is_err(), "damage at {at}");
        }
        let mut extra = good.clone(); extra.push(1);
        std::fs::write(&auto, &extra).unwrap();
        assert!(read(&auto).is_err());
        let mut huge = good; huge[8..16].copy_from_slice(&u64::MAX.to_le_bytes());
        std::fs::write(&auto, huge).unwrap();
        assert!(read(&auto).is_err(), "bounded before allocating decoded size");
        let mut bomb = encode(&vec![0; 200_000]).unwrap();
        bomb[8..16].copy_from_slice(&1u64.to_le_bytes());
        std::fs::write(&auto, bomb).unwrap();
        assert!(read(&auto).is_err(), "inflate cannot exceed the declared decoded length");
    }

    #[test]
    fn moving_auto_into_a_manual_slot_gives_old_players_raw_bytes() {
        let d = TestDir::new("state-codec-aside");
        let auto = d.join("Game.pstate.auto.z");
        let slot = d.join("Game.pstate");
        let raw = vec![71; 300_000];
        write(&auto, &raw).unwrap();
        let time = std::fs::metadata(&auto).unwrap().modified().unwrap();
        move_file(&auto, &slot).unwrap();
        assert!(!auto.exists());
        assert_eq!(std::fs::read(&slot).unwrap(), raw);
        assert_eq!(std::fs::metadata(&slot).unwrap().modified().unwrap(), time);
        move_file(&slot, &auto).unwrap();
        assert_eq!(read(&auto).unwrap(), raw);
        assert!(!slot.exists());
    }
}
