//! Unpacking store packages and app releases, which runs as root (B4 in the internal review).
//!
//! - Every header is read here before tar sees the archive: only regular files and folders with
//!   relative paths and no `..` pass. Symlinks, hardlinks, devices, FIFOs, sparse and unknown
//!   entries refuse the whole archive.
//! - tar unpacks exactly the bytes that were checked: a .tar.gz is decompressed here and fed to
//!   `tar -xf -`, so a second gzip member or a header tar reads differently can't slip through.
//! - `--no-same-owner --no-same-permissions`, into a folder only root can enter (0700).
//! - Then every mode is set: folders and executables 0755, other files 0644, never setuid/setgid.
//!   ROCKNIX's BusyBox tar 1.36 creates a setuid file even with `--no-same-permissions` (checked on
//!   the device 2026-10-22), so this step is what actually removes the bits.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

/// Upper limit for an unpacked archive held in memory (app releases are a few MB, cores less).
pub const MAX_UNPACKED: usize = 512 << 20;

/// One entry of a checked archive.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    /// Relative path, `/`-separated, without `.` or empty components.
    pub path: String,
    pub dir: bool,
}

const UNSAFE: &str = "The download contains links, devices or unsafe paths. Nothing was installed.";

/// `data` decompressed when it is gzip, as is otherwise.
pub fn decompress(data: &[u8]) -> Result<std::borrow::Cow<'_, [u8]>, String> {
    if data.starts_with(&[0x1f, 0x8b]) {
        gunzip(data, MAX_UNPACKED).map(std::borrow::Cow::Owned)
    } else {
        Ok(std::borrow::Cow::Borrowed(data))
    }
}

/// All members of a gzip file, concatenated (as `gzip -dc` gives them).
pub fn gunzip(data: &[u8], limit: usize) -> Result<Vec<u8>, String> {
    let damaged = || "The download is not a valid .tar.gz.".to_string();
    let mut out = Vec::new();
    let mut pos = 0;
    while pos < data.len() {
        let rest = &data[pos..];
        // gzip ignores zero padding at the end; anything else has to be another member.
        if rest.iter().all(|&b| b == 0) {
            break;
        }
        if rest.len() < 18 || rest[0] != 0x1f || rest[1] != 0x8b || rest[2] != 8 {
            return Err(damaged());
        }
        let flags = rest[3];
        let mut at = 10;
        if flags & 4 != 0 {
            let xlen = u16::from_le_bytes([*rest.get(at).ok_or_else(damaged)?, *rest.get(at + 1).ok_or_else(damaged)?]) as usize;
            at += 2 + xlen;
        }
        for bit in [8u8, 16] {
            if flags & bit != 0 {
                at += rest.get(at..).and_then(|r| r.iter().position(|&b| b == 0)).ok_or_else(damaged)? + 1;
            }
        }
        if flags & 2 != 0 {
            at += 2;
        }
        let used = inflate_raw(rest.get(at..).ok_or_else(damaged)?, &mut out, limit)?;
        // CRC32 and length follow the deflate stream.
        pos += at + used + 8;
        if pos > data.len() {
            return Err(damaged());
        }
    }
    Ok(out)
}

/// Inflates one raw deflate stream onto `out`; returns how many input bytes it took.
fn inflate_raw(input: &[u8], out: &mut Vec<u8>, limit: usize) -> Result<usize, String> {
    use miniz_oxide::inflate::stream::{inflate, InflateState};
    use miniz_oxide::{DataFormat, MZFlush, MZStatus};
    let mut state = InflateState::new_boxed(DataFormat::Raw);
    let mut buf = vec![0u8; 1 << 16];
    let mut pos = 0;
    loop {
        let r = inflate(&mut state, &input[pos..], &mut buf, MZFlush::None);
        pos += r.bytes_consumed;
        out.extend_from_slice(&buf[..r.bytes_written]);
        if out.len() > limit {
            return Err("The download is too big to unpack.".into());
        }
        match r.status {
            Ok(MZStatus::StreamEnd) => return Ok(pos),
            Ok(_) if r.bytes_consumed > 0 || r.bytes_written > 0 => {}
            _ => return Err("The download is not a valid .tar.gz.".into()),
        }
    }
}

fn octal(field: &[u8]) -> Option<u64> {
    let s: Vec<u8> = field.iter().copied().take_while(|&b| b != 0).filter(|&b| b != b' ').collect();
    if s.is_empty() {
        return Some(0);
    }
    u64::from_str_radix(std::str::from_utf8(&s).ok()?, 8).ok()
}

fn c_string(field: &[u8]) -> Result<String, String> {
    let end = field.iter().position(|&b| b == 0).unwrap_or(field.len());
    String::from_utf8(field[..end].to_vec()).map_err(|_| UNSAFE.to_string())
}

/// `name` as a safe relative path ("./a//b/" → "a/b"); None for the archive root itself.
fn clean_path(name: &str) -> Result<Option<String>, String> {
    if name.starts_with('/') || name.contains('\\') {
        return Err(UNSAFE.into());
    }
    let parts: Vec<&str> = name.split('/').filter(|c| !c.is_empty() && *c != ".").collect();
    if parts.contains(&"..") {
        return Err(UNSAFE.into());
    }
    Ok((!parts.is_empty()).then(|| parts.join("/")))
}

/// Records of a pax extended header ("<len> key=value\n"). Values can be binary (macOS tar stores
/// xattrs this way), so they stay bytes.
fn pax_records(mut data: &[u8]) -> Result<Vec<(String, Vec<u8>)>, String> {
    let mut out = Vec::new();
    while !data.is_empty() {
        let space = data.iter().position(|&b| b == b' ').ok_or(UNSAFE)?;
        let len: usize = std::str::from_utf8(&data[..space]).ok().and_then(|s| s.parse().ok()).ok_or(UNSAFE)?;
        if len <= space + 1 || len > data.len() || data[len - 1] != b'\n' {
            return Err(UNSAFE.into());
        }
        let record = &data[space + 1..len - 1];
        let eq = record.iter().position(|&b| b == b'=').ok_or(UNSAFE)?;
        let key = std::str::from_utf8(&record[..eq]).map_err(|_| UNSAFE)?;
        out.push((key.to_string(), record[eq + 1..].to_vec()));
        data = &data[len..];
    }
    Ok(out)
}

fn utf8(v: Vec<u8>) -> Result<String, String> {
    String::from_utf8(v).map_err(|_| UNSAFE.to_string())
}

/// The entries of an uncompressed tar, or an error if any of them is not a plain file or folder
/// inside the target, or the archive can't be read the same way by every tar.
pub fn entries(tar: &[u8]) -> Result<Vec<Entry>, String> {
    let mut out = Vec::new();
    let mut pos = 0;
    let mut ended = false;
    // Set by a GNU long-name ('L') or pax ('x') header for the entry that follows.
    let mut long_name: Option<String> = None;
    let mut pax_path: Option<String> = None;
    let mut pax_size: Option<u64> = None;
    while pos < tar.len() {
        let header = tar.get(pos..pos + 512).ok_or(UNSAFE)?;
        if header.iter().all(|&b| b == 0) {
            ended = true;
            pos += 512;
            continue;
        }
        // Tars stop at the end marker; something hidden behind it is refused rather than ignored.
        if ended {
            return Err(UNSAFE.into());
        }
        let sum: u64 = header.iter().enumerate().map(|(i, &b)| if (148..156).contains(&i) { 32 } else { b as u64 }).sum();
        // A size in base-256 (files over 8 GB) or a bad checksum isn't anything we publish.
        if header[124] & 0x80 != 0 || octal(&header[148..156]) != Some(sum) {
            return Err(UNSAFE.into());
        }
        let size = octal(&header[124..136]).ok_or(UNSAFE)?;
        let data_start = pos + 512;
        let data_end = data_start.checked_add(usize::try_from(size).map_err(|_| UNSAFE)?).ok_or(UNSAFE)?;
        let data = tar.get(data_start..data_end).ok_or(UNSAFE)?;
        pos = data_start + data.len().div_ceil(512) * 512;
        let kind = header[156];
        match kind {
            b'L' => {
                if long_name.is_some() {
                    return Err(UNSAFE.into());
                }
                long_name = Some(c_string(data)?);
                continue;
            }
            b'x' | b'g' => {
                for (k, v) in pax_records(data)? {
                    match k.as_str() {
                        // A global path or size would apply to every entry: never in our packages.
                        "path" | "size" if kind == b'g' => return Err(UNSAFE.into()),
                        "path" => pax_path = Some(utf8(v)?),
                        // Not every tar reads a pax size; the header has to say the same.
                        "size" => pax_size = Some(utf8(v)?.parse().map_err(|_| UNSAFE)?),
                        _ => {}
                    }
                }
                continue;
            }
            _ => {}
        }
        if pax_size.take().is_some_and(|s| s != size) {
            return Err(UNSAFE.into());
        }
        let ustar = &header[257..263] == b"ustar\0";
        let mut name = c_string(&header[..100])?;
        if ustar && header[345] != 0 {
            name = format!("{}/{name}", c_string(&header[345..500])?);
        }
        let name = match (pax_path.take(), long_name.take()) {
            (Some(_), Some(_)) => return Err(UNSAFE.into()),
            (Some(n), None) | (None, Some(n)) => n,
            (None, None) => name,
        };
        let dir = match kind {
            // Old tars mark folders with a trailing slash only.
            b'0' | 0 | b'7' => name.ends_with('/'),
            b'5' => true,
            // '1' hardlink, '2' symlink, '3'/'4' devices, '6' FIFO, 'K' long link name, 'S' sparse, …
            _ => return Err(UNSAFE.into()),
        };
        if dir && size != 0 {
            return Err(UNSAFE.into());
        }
        match clean_path(&name)? {
            Some(path) => out.push(Entry { path, dir }),
            None if dir => {}
            None => return Err(UNSAFE.into()),
        }
    }
    if long_name.is_some() || pax_path.is_some() || pax_size.is_some() {
        return Err(UNSAFE.into());
    }
    Ok(out)
}

/// Sets folders and executables to 0755 and every other file to 0644 below `dir` (and `dir`
/// itself); anything that is neither a file nor a folder is an error.
pub fn normalise_modes(dir: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let meta = std::fs::symlink_metadata(dir).map_err(|e| e.to_string())?;
    if !meta.is_dir() {
        return Err(UNSAFE.into());
    }
    for e in std::fs::read_dir(dir).map_err(|e| e.to_string())? {
        let path = e.map_err(|e| e.to_string())?.path();
        let meta = std::fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
        if meta.is_dir() {
            normalise_modes(&path)?;
        } else if meta.is_file() {
            let mode = if meta.permissions().mode() & 0o111 != 0 { 0o755 } else { 0o644 };
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode)).map_err(|e| e.to_string())?;
        } else {
            return Err(UNSAFE.into());
        }
    }
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o755)).map_err(|e| e.to_string())
}

/// Checks `archive` (.tar or .tar.gz bytes) and unpacks it into `into`, which must not exist yet.
/// On success `into` holds only plain files and folders with normalised modes; on failure it is gone.
pub fn unpack(archive: &[u8], into: &Path) -> Result<Vec<Entry>, String> {
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
    let tar = decompress(archive)?;
    let list = entries(&tar)?;
    std::fs::DirBuilder::new().mode(0o700).create(into).map_err(|e| format!("{}: {e}", into.display()))?;
    // The umask may have left more than 0700 open.
    let result = std::fs::set_permissions(into, std::fs::Permissions::from_mode(0o700)).map_err(|e| e.to_string()).and_then(|_| {
        let mut child = Command::new("tar")
            .args(["--no-same-owner", "--no-same-permissions", "-xf", "-", "-C"])
            .arg(into)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| e.to_string())?;
        // A write error means tar stopped early; its exit status says so.
        let _ = child.stdin.take().map(|mut s| s.write_all(&tar));
        let ok = child.wait().is_ok_and(|s| s.success());
        if !ok {
            return Err("Couldn't unpack the download.".to_string());
        }
        normalise_modes(into)
    });
    if result.is_err() {
        let _ = std::fs::remove_dir_all(into);
    }
    result.map(|_| list)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::testutil::TestDir;

    /// A ustar header block for `name` of type `kind` with `size` bytes of data.
    pub(crate) fn header(name: &str, kind: u8, mode: u32, size: usize, link: &str) -> Vec<u8> {
        let mut h = vec![0u8; 512];
        h[..name.len()].copy_from_slice(name.as_bytes());
        h[100..108].copy_from_slice(format!("{mode:07o}\0").as_bytes());
        h[108..116].copy_from_slice(b"0000000\0");
        h[116..124].copy_from_slice(b"0000000\0");
        h[124..136].copy_from_slice(format!("{size:011o}\0").as_bytes());
        h[136..148].copy_from_slice(b"00000000000\0");
        h[156] = kind;
        h[157..157 + link.len()].copy_from_slice(link.as_bytes());
        h[257..263].copy_from_slice(b"ustar\0");
        h[263..265].copy_from_slice(b"00");
        h[148..156].copy_from_slice(b"        ");
        let sum: u32 = h.iter().map(|&b| b as u32).sum();
        h[148..156].copy_from_slice(format!("{sum:06o}\0 ").as_bytes());
        h
    }

    /// A tar from (name, kind, mode, data, link) entries, with the end marker.
    pub(crate) fn tar(items: &[(&str, u8, u32, &[u8], &str)]) -> Vec<u8> {
        let mut t = Vec::new();
        for (name, kind, mode, data, link) in items {
            t.extend(header(name, *kind, *mode, data.len(), link));
            t.extend_from_slice(data);
            t.resize(t.len().div_ceil(512) * 512, 0);
        }
        t.extend(vec![0u8; 1024]);
        t
    }

    fn pax(records: &[(&str, &str)]) -> Vec<u8> {
        let mut out = Vec::new();
        for (k, v) in records {
            let body = format!(" {k}={v}\n");
            let mut len = body.len() + 1;
            while format!("{len}{body}").len() != len {
                len += 1;
            }
            out.extend(format!("{len}{body}").into_bytes());
        }
        out
    }

    fn gz(data: &[u8]) -> Vec<u8> {
        let mut out = vec![0x1f, 0x8b, 8, 0, 0, 0, 0, 0, 0, 3];
        out.extend(miniz_oxide::deflate::compress_to_vec(data, 6));
        let mut crc = !0u32;
        for &b in data {
            crc ^= b as u32;
            for _ in 0..8 {
                crc = if crc & 1 != 0 { (crc >> 1) ^ 0xedb8_8320 } else { crc >> 1 };
            }
        }
        out.extend((!crc).to_le_bytes());
        out.extend((data.len() as u32).to_le_bytes());
        out
    }

    #[test]
    fn plain_files_and_folders_pass() {
        let t = tar(&[("./", b'5', 0o755, b"", ""), ("./pk/", b'5', 0o2777, b"", ""), ("./pk/a", b'0', 0o4755, b"aa", ""), ("pk/old/", b'0', 0o755, b"", "")]);
        let list = entries(&t).unwrap();
        assert_eq!(
            list,
            vec![
                Entry { path: "pk".into(), dir: true },
                Entry { path: "pk/a".into(), dir: false },
                Entry { path: "pk/old".into(), dir: true }
            ]
        );
        assert_eq!(entries(&gunzip(&gz(&t), MAX_UNPACKED).unwrap()).unwrap(), list);
    }

    #[test]
    fn links_devices_and_escapes_are_refused() {
        for (name, kind, link) in [
            ("pk/l", b'2', "/etc/passwd"),
            ("pk/h", b'1', "pk/a"),
            ("pk/c", b'3', ""),
            ("pk/b", b'4', ""),
            ("pk/f", b'6', ""),
            ("pk/s", b'S', ""),
            ("/etc/x", b'0', ""),
            ("pk/../../x", b'0', ""),
            ("..", b'5', ""),
            ("", b'0', ""),
        ] {
            let t = tar(&[("pk/a", b'0', 0o644, b"a", ""), (name, kind, 0o644, b"", link)]);
            assert!(entries(&t).is_err(), "{name} ({})", kind as char);
        }
    }

    #[test]
    fn long_names_and_pax_paths_are_checked_too() {
        let long = format!("pk/{}/f", "d".repeat(120));
        let ok = tar(&[("././@LongLink", b'L', 0o644, format!("{long}\0").as_bytes(), ""), ("short", b'0', 0o644, b"x", "")]);
        assert_eq!(entries(&ok).unwrap(), vec![Entry { path: long, dir: false }]);
        let bad = tar(&[("././@LongLink", b'L', 0o644, b"../../etc/x\0", ""), ("short", b'0', 0o644, b"x", "")]);
        assert!(entries(&bad).is_err());
        let bad = tar(&[("pax", b'x', 0o644, &pax(&[("path", "/etc/shadow")]), ""), ("short", b'0', 0o644, b"x", "")]);
        assert!(entries(&bad).is_err());
        let xattr = tar(&[("pax", b'x', 0o644, &pax(&[("SCHILY.xattr.user.x", "1"), ("path", "pk/b")]), ""), ("short", b'0', 0o644, b"x", "")]);
        assert_eq!(entries(&xattr).unwrap(), vec![Entry { path: "pk/b".into(), dir: false }]);
        // A pax size some tars would follow and others not.
        let bad = tar(&[("pax", b'x', 0o644, &pax(&[("size", "0")]), ""), ("short", b'0', 0o644, b"x", "")]);
        assert!(entries(&bad).is_err());
        let bad = tar(&[("pax", b'g', 0o644, &pax(&[("path", "x")]), ""), ("short", b'0', 0o644, b"x", "")]);
        assert!(entries(&bad).is_err());
    }

    #[test]
    fn hidden_data_is_refused() {
        // A symlink behind the end marker, a broken checksum, a second gzip member with a symlink.
        let mut t = tar(&[("a", b'0', 0o644, b"a", "")]);
        t.extend(header("evil", b'2', 0o777, 0, "/etc"));
        assert!(entries(&t).is_err());
        let mut t = tar(&[("a", b'0', 0o644, b"a", "")]);
        t[0] = b'b';
        assert!(entries(&t).is_err());
        let mut two = gz(&tar(&[("a", b'0', 0o644, b"a", "")]));
        two.extend(gz(&tar(&[("evil", b'2', 0o777, b"", "/etc")])));
        assert!(entries(&gunzip(&two, MAX_UNPACKED).unwrap()).is_err());
        let mut junk = gz(&tar(&[("a", b'0', 0o644, b"a", "")]));
        junk.extend(b"junk");
        assert!(gunzip(&junk, MAX_UNPACKED).is_err());
        assert!(gunzip(&gz(&[0u8; 4096]), 1024).is_err(), "size limit");
    }

    #[test]
    fn unpacks_with_normal_modes() {
        use std::os::unix::fs::PermissionsExt;
        let dir = TestDir::new("untar");
        let t = tar(&[("pk/", b'5', 0o2777, b"", ""), ("pk/run", b'0', 0o4755, b"#!/bin/sh\n", ""), ("pk/data", b'0', 0o6600, b"d", "")]);
        let into = dir.join("into");
        let list = unpack(&gz(&t), &into).unwrap();
        assert_eq!(list.len(), 3);
        let mode = |p: &str| std::fs::metadata(into.join(p)).unwrap().permissions().mode() & 0o7777;
        assert_eq!((mode("pk"), mode("pk/run"), mode("pk/data"), mode("")), (0o755, 0o755, 0o644, 0o755));
        assert!(unpack(&t, &into).is_err(), "the target must be new");
        let bad = dir.join("bad");
        assert!(unpack(&tar(&[("l", b'2', 0o777, b"", "/")]), &bad).is_err());
        assert!(!bad.exists(), "nothing is left after a refusal");
    }

    /// The packages in store/test-catalog, made by macOS tar on the build machine, pass.
    #[test]
    fn test_catalog_packages_pass() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("store/test-catalog/packages");
        let mut seen = 0;
        for e in std::fs::read_dir(dir).unwrap().flatten() {
            if e.file_name().to_string_lossy().ends_with(".tar.gz") {
                let data = std::fs::read(e.path()).unwrap();
                let list = entries(&decompress(&data).unwrap()).unwrap();
                assert!(!list.is_empty());
                seen += 1;
            }
        }
        assert!(seen >= 2);
    }
}
