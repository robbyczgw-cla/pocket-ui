//! A ZIP written while it is sent (`http::Response::stream`): the files stored as they are (no compression: saves are
//! small, states are already packed, and the Pocket's CPU belongs to its games), each file's CRC-32 and size in a data
//! descriptor after it, so nothing is read twice and nothing is held in memory. No ZIP64: at most [`MAX_BYTES`] and
//! [`MAX_FILES`], which the caller checks before it starts ([`fits`]). Names are UTF-8 (flag bit 11).
//!
//! A file that is gone when its turn comes is left out; one that can't be read to its end, or `stop` saying so between
//! two blocks, ends the ZIP with an error (the connection then closes without its last chunk, and the browser reports
//! the download as failed rather than keeping half a ZIP).

use std::io::{self, Read, Write};
use std::path::PathBuf;

/// What one ZIP may hold without ZIP64 (offsets and sizes are 32 bits), with room for the headers.
pub const MAX_BYTES: u64 = (4 << 30) - (64 << 20);
/// Entries in one ZIP without ZIP64.
pub const MAX_FILES: usize = 65_000;

/// A file and its place in the ZIP.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    /// Its path in the ZIP, with `/` between folders, never starting with one.
    pub name: String,
    pub path: PathBuf,
    /// Unix seconds, for the time the ZIP shows.
    pub modified: u64,
}

/// Whether files of these sizes fit one ZIP.
pub fn fits(count: usize, bytes: u64) -> bool {
    count <= MAX_FILES && bytes <= MAX_BYTES
}

/// The MS-DOS time and date of a Unix time (UTC; DOS has no zone). Before 1980 is 1980-01-01.
pub fn dos_time(unix: u64) -> (u16, u16) {
    let days = (unix / 86_400) as i64;
    let secs = unix % 86_400;
    // Howard Hinnant's days-to-civil.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + if month <= 2 { 1 } else { 0 };
    if year < 1980 {
        return (0, (1 << 5) | 1);
    }
    let year = year.min(2107);
    let time = ((secs / 3600) << 11) | (((secs / 60) % 60) << 5) | ((secs % 60) / 2);
    let date = (((year - 1980) as u64) << 9) | ((month as u64) << 5) | day as u64;
    (time as u16, date as u16)
}

/// Counts what went through, for the offsets.
struct Counted<'a> {
    w: &'a mut dyn Write,
    at: u64,
}

impl Write for Counted<'_> {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        let n = self.w.write(b)?;
        self.at += n as u64;
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.w.flush()
    }
}

struct Done {
    name: Vec<u8>,
    time: (u16, u16),
    crc: u32,
    size: u32,
    offset: u32,
}

const FLAGS: u16 = (1 << 3) | (1 << 11);

fn too_big() -> io::Error {
    io::Error::other("the ZIP would be too large")
}

fn u32_of(n: u64) -> io::Result<u32> {
    u32::try_from(n).map_err(|_| too_big())
}

/// Writes `files` as one ZIP to `out`. `stop` is asked between blocks: true ends it with an error.
pub fn write(out: &mut dyn Write, files: &[Entry], stop: &dyn Fn() -> bool) -> io::Result<()> {
    if files.len() > MAX_FILES {
        return Err(too_big());
    }
    let mut w = Counted { w: out, at: 0 };
    let mut done: Vec<Done> = Vec::with_capacity(files.len());
    let mut buf = vec![0u8; 64 * 1024];
    for e in files {
        if stop() {
            return Err(io::Error::other("stopped"));
        }
        // Gone meanwhile (a game deleted on the Pocket): left out, before a byte of it is written.
        let Ok(mut f) = std::fs::File::open(&e.path) else { continue };
        if !f.metadata().is_ok_and(|m| m.is_file()) {
            continue;
        }
        let name = e.name.trim_start_matches('/').as_bytes().to_vec();
        let time = dos_time(e.modified);
        let offset = u32_of(w.at)?;
        let mut head = Vec::with_capacity(30 + name.len());
        head.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        head.extend_from_slice(&20u16.to_le_bytes());
        head.extend_from_slice(&FLAGS.to_le_bytes());
        head.extend_from_slice(&0u16.to_le_bytes());
        head.extend_from_slice(&time.0.to_le_bytes());
        head.extend_from_slice(&time.1.to_le_bytes());
        head.extend_from_slice(&[0u8; 12]);
        head.extend_from_slice(&(name.len() as u16).to_le_bytes());
        head.extend_from_slice(&0u16.to_le_bytes());
        head.extend_from_slice(&name);
        w.write_all(&head)?;
        let mut crc = crc32fast::Hasher::new();
        let mut size = 0u64;
        loop {
            if stop() {
                return Err(io::Error::other("stopped"));
            }
            let n = match f.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => n,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(e),
            };
            crc.update(&buf[..n]);
            w.write_all(&buf[..n])?;
            size += n as u64;
            if w.at > MAX_BYTES {
                return Err(too_big());
            }
        }
        let (crc, size) = (crc.finalize(), u32_of(size)?);
        let mut desc = Vec::with_capacity(16);
        desc.extend_from_slice(&0x0807_4b50u32.to_le_bytes());
        desc.extend_from_slice(&crc.to_le_bytes());
        desc.extend_from_slice(&size.to_le_bytes());
        desc.extend_from_slice(&size.to_le_bytes());
        w.write_all(&desc)?;
        done.push(Done { name, time, crc, size, offset });
    }
    let start = u32_of(w.at)?;
    for d in &done {
        let mut c = Vec::with_capacity(46 + d.name.len());
        c.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
        // Made by: Unix, 3.0 (so the file mode below counts).
        c.extend_from_slice(&0x031eu16.to_le_bytes());
        c.extend_from_slice(&20u16.to_le_bytes());
        c.extend_from_slice(&FLAGS.to_le_bytes());
        c.extend_from_slice(&0u16.to_le_bytes());
        c.extend_from_slice(&d.time.0.to_le_bytes());
        c.extend_from_slice(&d.time.1.to_le_bytes());
        c.extend_from_slice(&d.crc.to_le_bytes());
        c.extend_from_slice(&d.size.to_le_bytes());
        c.extend_from_slice(&d.size.to_le_bytes());
        c.extend_from_slice(&(d.name.len() as u16).to_le_bytes());
        c.extend_from_slice(&[0u8; 8]);
        c.extend_from_slice(&((0o100644u32) << 16).to_le_bytes());
        c.extend_from_slice(&d.offset.to_le_bytes());
        c.extend_from_slice(&d.name);
        w.write_all(&c)?;
    }
    let size = u32_of(w.at - start as u64)?;
    let mut end = Vec::with_capacity(22);
    end.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    end.extend_from_slice(&[0u8; 4]);
    end.extend_from_slice(&(done.len() as u16).to_le_bytes());
    end.extend_from_slice(&(done.len() as u16).to_le_bytes());
    end.extend_from_slice(&size.to_le_bytes());
    end.extend_from_slice(&start.to_le_bytes());
    end.extend_from_slice(&0u16.to_le_bytes());
    w.write_all(&end)?;
    w.flush()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    fn u16_at(b: &[u8], at: usize) -> u16 {
        u16::from_le_bytes([b[at], b[at + 1]])
    }
    fn u32_at(b: &[u8], at: usize) -> u32 {
        u32::from_le_bytes(b[at..at + 4].try_into().unwrap())
    }

    /// The ZIP's files as its central directory names them, each read at its local header: (name, bytes).
    pub fn read_back(zip: &[u8]) -> Vec<(String, Vec<u8>)> {
        let end = zip.len() - 22;
        assert_eq!(u32_at(zip, end), 0x0605_4b50);
        let count = u16_at(zip, end + 10) as usize;
        let mut at = u32_at(zip, end + 16) as usize;
        let mut out = Vec::new();
        for _ in 0..count {
            assert_eq!(u32_at(zip, at), 0x0201_4b50);
            let (crc, size, nlen) = (u32_at(zip, at + 16), u32_at(zip, at + 24) as usize, u16_at(zip, at + 28) as usize);
            let local = u32_at(zip, at + 42) as usize;
            let name = String::from_utf8(zip[at + 46..at + 46 + nlen].to_vec()).unwrap();
            assert_eq!(u32_at(zip, local), 0x0403_4b50);
            let data_at = local + 30 + u16_at(zip, local + 26) as usize + u16_at(zip, local + 28) as usize;
            let data = zip[data_at..data_at + size].to_vec();
            assert_eq!(crc32fast::hash(&data), crc, "{name}: CRC");
            assert_eq!(u32_at(zip, data_at + size), 0x0807_4b50, "{name}: the descriptor follows the data");
            out.push((name, data));
            at += 46 + nlen;
        }
        out
    }

    #[test]
    fn a_zip_holds_each_file_whole_under_its_name() {
        let dir = std::env::temp_dir().join(format!("pocketui-zip-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.srm"), vec![7u8; 70_000]).unwrap();
        std::fs::write(dir.join("b.pstate"), b"state").unwrap();
        let files = vec![
            Entry { name: "roms/gba/Pokémon.srm".into(), path: dir.join("a.srm"), modified: 1_763_650_000 },
            Entry { name: "roms/savestates/gba/b.pstate".into(), path: dir.join("b.pstate"), modified: 0 },
            Entry { name: "gone.srm".into(), path: dir.join("gone.srm"), modified: 0 },
        ];
        let mut out = Vec::new();
        write(&mut out, &files, &|| false).unwrap();
        let back = read_back(&out);
        assert_eq!(back.len(), 2, "a file that is gone is left out");
        assert_eq!(back[0], ("roms/gba/Pokémon.srm".to_string(), vec![7u8; 70_000]));
        assert_eq!(back[1], ("roms/savestates/gba/b.pstate".to_string(), b"state".to_vec()));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn an_empty_zip_is_still_a_zip() {
        let mut out = Vec::new();
        write(&mut out, &[], &|| false).unwrap();
        assert_eq!(out.len(), 22);
        assert!(read_back(&out).is_empty());
    }

    #[test]
    fn stop_ends_the_zip_with_an_error() {
        let dir = std::env::temp_dir().join(format!("pocketui-zip-stop-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.srm"), b"x").unwrap();
        let files = vec![Entry { name: "a.srm".into(), path: dir.join("a.srm"), modified: 0 }];
        assert!(write(&mut Vec::new(), &files, &|| true).is_err());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn dos_times_count_from_1980() {
        // 2026-11-20 14:05:08 UTC.
        let (time, date) = dos_time(1_795_183_508);
        assert_eq!((date >> 9) + 1980, 2026);
        assert_eq!((date >> 5) & 15, 11);
        assert_eq!(date & 31, 20);
        assert_eq!((time >> 11, (time >> 5) & 63, (time & 31) * 2), (14, 5, 8));
        assert_eq!(dos_time(0), (0, (1 << 5) | 1));
    }
}
