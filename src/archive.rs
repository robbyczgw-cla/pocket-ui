//! Reads one ROM out of a .zip, for cores that take ROM data but do not open archives themselves
//! (most 8/16-bit cores: the frontend is expected to unpack, as RetroArch does). Stored and deflated
//! entries; no ZIP64, no encryption. 7z is not supported: pocket-ui leaves those games to RetroArch.
//!
//! Which entry is the game is not decided by archive order: one file of the system's extensions is the game, and
//! readmes, pictures and checksum files next to it do not count; two or more candidates are refused. The unpacked
//! bytes are checked against the entry's CRC-32. These refusals (`is_refusal`) are plain sentences for the player, who
//! sees them where a game that did not start is shown (`notice`); RetroArch is not tried after them, it would only
//! pick by archive order or play a damaged file (`fallback::should_retry`).
//!
//! `rom_crc` reads only the directory at the end of a zip: the game's CRC-32 as the zip states it, for recognising the
//! game by its checksum (`rom_id`) without unpacking it.

use std::path::Path;

/// Largest ROM we unpack into memory.
const MAX_ROM: usize = 64 << 20;

fn le16(b: &[u8], at: usize) -> Option<usize> {
    Some(u16::from_le_bytes(b.get(at..at + 2)?.try_into().ok()?) as usize)
}

fn le32(b: &[u8], at: usize) -> Option<usize> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?) as usize)
}

struct Entry {
    name: String,
    flags: usize,
    method: usize,
    crc: u32,
    csize: usize,
    usize: usize,
    local: usize,
}

/// The end-of-central-directory record in `tail` (the last bytes of a zip): how many entries the directory has, where
/// it starts in the file and how long it is.
fn end_record(tail: &[u8]) -> Result<(usize, usize, usize), String> {
    let bad = || "not a valid zip file".to_string();
    let eocd = find_end(tail).ok_or_else(bad)?;
    Ok((le16(tail, eocd + 10).ok_or_else(bad)?, le32(tail, eocd + 16).ok_or_else(bad)?, le32(tail, eocd + 12).ok_or_else(bad)?))
}

/// Where the end-of-central-directory record is: the last 22+ bytes, signature PK\5\6. None for a file that was cut
/// off on the way (a copy or a download that stopped) or is no zip at all.
fn find_end(tail: &[u8]) -> Option<usize> {
    let from = tail.len().saturating_sub(22 + 65535);
    (from..tail.len().saturating_sub(21)).rev().find(|&i| tail[i..].starts_with(b"PK\x05\x06"))
}

/// The `count` entries of a central directory; `z` starts at its first entry.
fn directory(z: &[u8], count: usize) -> Result<Vec<Entry>, String> {
    let bad = || "not a valid zip file".to_string();
    let mut at = 0;
    let mut out = Vec::new();
    for _ in 0..count {
        if !z.get(at..).is_some_and(|s| s.starts_with(b"PK\x01\x02")) {
            return Err(bad());
        }
        let (name_len, extra, comment) = (
            le16(z, at + 28).ok_or_else(bad)?,
            le16(z, at + 30).ok_or_else(bad)?,
            le16(z, at + 32).ok_or_else(bad)?,
        );
        let name = String::from_utf8_lossy(z.get(at + 46..at + 46 + name_len).ok_or_else(bad)?).into_owned();
        out.push(Entry {
            name,
            flags: le16(z, at + 8).ok_or_else(bad)?,
            method: le16(z, at + 10).ok_or_else(bad)?,
            crc: le32(z, at + 16).ok_or_else(bad)? as u32,
            csize: le32(z, at + 20).ok_or_else(bad)?,
            usize: le32(z, at + 24).ok_or_else(bad)?,
            local: le32(z, at + 42).ok_or_else(bad)?,
        });
        at += 46 + name_len + extra + comment;
    }
    Ok(out)
}

fn entries(z: &[u8]) -> Result<Vec<Entry>, String> {
    let (count, at, _) = end_record(z)?;
    directory(z.get(at..).ok_or_else(|| "not a valid zip file".to_string())?, count)
}

/// What follows the first sentence of each refusal: what the player can do about it.
const ADVICE: [&str; 5] = [
    "Unpack it on a computer and keep only the one you want.",
    "Unpack it on a computer and copy the game file itself.",
    "Pack it again as a normal zip file, or copy the game file itself.",
    "Copy the file to the SD card again.",
    "Check that it is the right file.",
];

/// A refusal: `what` happened to the zip, then the matching advice.
fn refusal(what: String, advice: usize) -> String {
    format!("{what} {}", ADVICE[advice])
}

/// The refusals' sentences, by name: `extract_rom` returns them, and the snapshots (`notice:<kind>`) show them.
pub fn message(kind: &str, zip_name: &str) -> String {
    match kind {
        "several" => refusal(format!("{zip_name} holds 2 game files."), 0),
        "password" => refusal(format!("{zip_name} is password-protected, and pocket-core can't open it."), 1),
        "packing" => refusal(format!("{zip_name} is packed in a way pocket-core can't open."), 2),
        "checksum" => refusal(format!("{zip_name} is damaged: the game inside doesn't match its checksum."), 3),
        "incomplete" => refusal(format!("{zip_name} is damaged or not complete."), 3),
        "empty" => refusal(format!("{zip_name} is empty."), 4),
        _ => refusal(format!("There is no game file in {zip_name}."), 4),
    }
}

/// Whether `message` (from `extract_rom`) is one of its refusals: the zip was read and is not to be played, so
/// another emulator would not do better.
pub fn is_refusal(message: &str) -> bool {
    ADVICE.iter().any(|a| message.ends_with(a))
}

/// What other systems leave in a zip next to the files: never the game, whatever the extension ("._Sonic.md").
fn is_junk(path: &str) -> bool {
    let file = path.rsplit('/').next().unwrap_or(path);
    path.starts_with("__MACOSX/")
        || file.starts_with("._")
        || ["Thumbs.db", ".DS_Store", "desktop.ini"].iter().any(|n| file.eq_ignore_ascii_case(n))
}

/// Files in a zip that are not the game unless the core's list names their extension: texts, pictures, checksums.
pub(crate) fn is_clutter(ext: &str) -> bool {
    [
        "txt", "nfo", "diz", "doc", "rtf", "pdf", "jpg", "jpeg", "png", "gif", "bmp", "url", "htm", "html", "xml", "ini", "sfv", "md5", "sha1", "crc", "log", "lnk",
    ]
    .contains(&ext)
}

/// The game among a zip's entries: the one file whose extension is in `wanted`, else the one file that is not a readme,
/// picture or checksum; a refusal for the player otherwise (`extract_rom`).
fn pick<'a>(list: &'a [Entry], wanted: &[&str], zip_name: &str) -> Result<&'a Entry, String> {
    let ext_of = |n: &str| {
        Path::new(n)
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase)
            .unwrap_or_default()
    };
    let files: Vec<&Entry> = list.iter().filter(|e| !e.name.ends_with('/') && !is_junk(&e.name)).collect();
    let mut candidates: Vec<&Entry> = files.iter().copied().filter(|e| wanted.contains(&ext_of(&e.name).as_str())).collect();
    if candidates.is_empty() {
        candidates = files.iter().copied().filter(|e| !is_clutter(&ext_of(&e.name))).collect();
    }
    match candidates[..] {
        [] if files.is_empty() => Err(message("empty", zip_name)),
        [] => Err(message("none", zip_name)),
        [e] => Ok(e),
        _ => Err(refusal(format!("{zip_name} holds {} game files.", candidates.len()), 0)),
    }
}

/// The largest central directory `rom_crc` reads (a zip of one game has a few hundred bytes of it).
const MAX_DIRECTORY: usize = 4 << 20;

/// The CRC-32 and unpacked size of the game in `zip_path` as the zip's own directory states them, read from the end
/// of the file: nothing is unpacked, so a 30 MB zip costs one or two small reads (`rom_id`). The game is the entry
/// `extract_rom` would take; None when it would refuse (several games, no game) or the file is no zip it can read.
pub fn rom_crc(zip_path: &Path, wanted: &[&str]) -> Option<(u32, u64)> {
    let list = directory_of(zip_path).ok()??;
    let e = pick(&list, wanted, "").ok()?;
    Some((e.crc, e.usize as u64))
}

/// The entries of the zip at `zip_path`, read from the end of the file. Outer None: no end record, so the file was cut
/// off or is no zip. An error: it could not be read, or its directory doesn't read as one.
fn directory_of(zip_path: &Path) -> Result<Option<Vec<Entry>>, String> {
    use std::io::{Read, Seek, SeekFrom};
    let bad = |e: std::io::Error| e.to_string();
    let mut f = std::fs::File::open(zip_path).map_err(bad)?;
    let len = f.metadata().map_err(bad)?.len();
    let tail_len = len.min(22 + 65535);
    let mut tail = vec![0; tail_len as usize];
    f.seek(SeekFrom::Start(len - tail_len)).map_err(bad)?;
    f.read_exact(&mut tail).map_err(bad)?;
    if find_end(&tail).is_none() {
        return Ok(None);
    }
    let (count, at, size) = end_record(&tail)?;
    let tail_at = (len - tail_len) as usize;
    let cd = if at >= tail_at {
        tail.get(at - tail_at..).ok_or("not a valid zip file")?.to_vec()
    } else {
        if size > MAX_DIRECTORY {
            return Err("the zip's directory is too big".into());
        }
        let mut cd = vec![0; size];
        f.seek(SeekFrom::Start(at as u64)).map_err(bad)?;
        f.read_exact(&mut cd).map_err(bad)?;
        cd
    };
    directory(&cd, count).map(Some)
}

/// The sentence a zip would be refused with, from its directory alone (Settings → Games → Game check, `game_check`):
/// `extract_rom`'s refusals without unpacking anything, so the checksum of the game inside is not looked at. With
/// `only_cut_off` just a zip cut off on the way counts: an arcade set is many files by design. None: nothing against it,
/// or a rare kind of zip that is not this reader's to judge (RetroArch still tries those).
pub fn refusal_of(zip_path: &Path, wanted: &[&str], only_cut_off: bool) -> Option<String> {
    let zip_name = zip_path.file_name().and_then(|n| n.to_str()).unwrap_or("The zip file");
    let list = match directory_of(zip_path) {
        Ok(Some(list)) => list,
        Ok(None) => return Some(message("incomplete", zip_name)),
        Err(_) => return None,
    };
    if only_cut_off {
        return None;
    }
    let e = match pick(&list, wanted, zip_name) {
        Ok(e) => e,
        Err(why) => return Some(why),
    };
    if e.flags & 1 != 0 || e.method == 99 {
        return Some(message("password", zip_name));
    }
    (!matches!(e.method, 0 | 8)).then(|| message("packing", zip_name))
}

/// The ROM inside `zip_path`: the one file whose extension is in `wanted` (lower case, no dot), else the one file
/// that is not a readme, picture or checksum. Several candidates, an encrypted or unsupported entry, or bytes that do
/// not match the entry's CRC-32 are refused with a message for the player (`is_refusal`). Returns its name inside the
/// archive and its contents.
pub fn extract_rom(zip_path: &Path, wanted: &[&str]) -> Result<(String, Vec<u8>), String> {
    let z = std::fs::read(zip_path).map_err(|e| format!("{}: {e}", zip_path.display()))?;
    let zip_name = zip_path.file_name().and_then(|n| n.to_str()).unwrap_or("The zip file");
    // No end record: the file was cut off (a copy or a download that stopped). Nothing opens it, RetroArch neither (its
    // reader starts from that record too), and a fall back would keep the game on RetroArch after a good copy.
    if find_end(&z).is_none() {
        return Err(message("incomplete", zip_name));
    }
    let list = entries(&z)?;
    let e = pick(&list, wanted, zip_name)?;
    // Bit 0: encrypted; method 99: WinZip's AES. Neither can be read without a password.
    if e.flags & 1 != 0 || e.method == 99 {
        return Err(message("password", zip_name));
    }
    if !matches!(e.method, 0 | 8) {
        return Err(message("packing", zip_name));
    }
    if e.usize > MAX_ROM {
        return Err(format!("{} is too big to unpack", e.name));
    }
    let bad = || "not a valid zip file".to_string();
    let local = e.local;
    let start = local + 30 + le16(&z, local + 26).ok_or_else(bad)? + le16(&z, local + 28).ok_or_else(bad)?;
    let raw = z.get(start..start + e.csize).ok_or_else(bad)?;
    let data = match e.method {
        0 => raw.to_vec(),
        _ => miniz_oxide::inflate::decompress_to_vec_with_limit(raw, MAX_ROM).map_err(|_| "the zip file is damaged".to_string())?,
    };
    if data.len() != e.usize {
        return Err("the zip file is damaged".into());
    }
    // The data is in memory anyway; the check costs a few milliseconds even for the biggest ROM.
    if crc32fast::hash(&data) != e.crc {
        return Err(message("checksum", zip_name));
    }
    let name = Path::new(&e.name).file_name().and_then(|n| n.to_str()).unwrap_or(&e.name).to_string();
    Ok((name, data))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::testutil::TestDir;

    /// A zip of stored files, for other modules' tests.
    pub(crate) fn zip_of(files: &[(&'static str, &[u8])]) -> Vec<u8> {
        zip(&files.iter().map(|(n, d)| stored(n, d)).collect::<Vec<_>>())
    }

    /// One entry of a test zip: its stored bytes, original size and CRC-32 as the archive claims them.
    struct F {
        name: &'static str,
        method: u16,
        flags: u16,
        stored: Vec<u8>,
        size: usize,
        crc: u32,
    }

    /// A stored entry with the right size and CRC.
    fn stored(name: &'static str, data: &[u8]) -> F {
        F { name, method: 0, flags: 0, stored: data.to_vec(), size: data.len(), crc: crc32fast::hash(data) }
    }

    fn zip(files: &[F]) -> Vec<u8> {
        let (mut out, mut central) = (Vec::new(), Vec::new());
        for f in files {
            let off = out.len() as u32;
            out.extend_from_slice(b"PK\x03\x04\x14\0");
            out.extend_from_slice(&f.flags.to_le_bytes());
            out.extend_from_slice(&f.method.to_le_bytes());
            out.extend_from_slice(&[0; 4]);
            out.extend_from_slice(&f.crc.to_le_bytes());
            out.extend_from_slice(&(f.stored.len() as u32).to_le_bytes());
            out.extend_from_slice(&(f.size as u32).to_le_bytes());
            out.extend_from_slice(&(f.name.len() as u16).to_le_bytes());
            out.extend_from_slice(&[0, 0]);
            out.extend_from_slice(f.name.as_bytes());
            out.extend_from_slice(&f.stored);
            central.extend_from_slice(b"PK\x01\x02\x14\0\x14\0");
            central.extend_from_slice(&f.flags.to_le_bytes());
            central.extend_from_slice(&f.method.to_le_bytes());
            central.extend_from_slice(&[0; 4]);
            central.extend_from_slice(&f.crc.to_le_bytes());
            central.extend_from_slice(&(f.stored.len() as u32).to_le_bytes());
            central.extend_from_slice(&(f.size as u32).to_le_bytes());
            central.extend_from_slice(&(f.name.len() as u16).to_le_bytes());
            central.extend_from_slice(&[0; 12]);
            central.extend_from_slice(&off.to_le_bytes());
            central.extend_from_slice(f.name.as_bytes());
        }
        let cd_at = out.len() as u32;
        out.extend_from_slice(&central);
        out.extend_from_slice(b"PK\x05\x06\0\0\0\0");
        out.extend_from_slice(&(files.len() as u16).to_le_bytes());
        out.extend_from_slice(&(files.len() as u16).to_le_bytes());
        out.extend_from_slice(&(central.len() as u32).to_le_bytes());
        out.extend_from_slice(&cd_at.to_le_bytes());
        out.extend_from_slice(&[0, 0]);
        out
    }

    /// Writes the zip to a file called `name` of its own and reads it with `extract_rom`.
    fn run(name: &str, files: &[F], wanted: &[&str]) -> Result<(String, Vec<u8>), String> {
        let dir = TestDir::new(&format!("zip-{name}"));
        let p = dir.join(format!("{name}.zip"));
        std::fs::write(&p, zip(files)).unwrap();
        extract_rom(&p, wanted)
    }

    fn rom() -> Vec<u8> {
        (0..5000u32).map(|i| (i * 7 % 251) as u8).collect()
    }

    #[test]
    fn refusals_are_read_from_the_directory_alone() {
        let dir = TestDir::new("zip-refusal-of");
        let put = |name: &str, bytes: &[u8]| {
            let p = dir.join(name);
            std::fs::write(&p, bytes).unwrap();
            p
        };
        assert_eq!(refusal_of(&put("Good.zip", &zip(&[stored("Good.gba", b"ROM")])), &["gba"], false), None);
        let readme = put("Readme only.zip", &zip(&[stored("readme.txt", b"hi")]));
        assert_eq!(refusal_of(&readme, &["gba"], false).as_deref(), Some(message("none", "Readme only.zip").as_str()));
        assert_eq!(refusal_of(&readme, &["gba"], true), None, "an arcade set is many files by design");
        let cut = zip(&[stored("Cut.gba", &[7; 100])]);
        let cut = put("Cut.zip", &cut[..40]);
        assert_eq!(refusal_of(&cut, &["gba"], true).as_deref(), Some(message("incomplete", "Cut.zip").as_str()));
        assert!(refusal_of(&put("Two.zip", &zip(&[stored("A.gba", b"A"), stored("B.gba", b"B")])), &["gba"], false).unwrap().contains("holds 2 game files"));
    }

    #[test]
    fn a_valid_zip_with_one_rom_comes_out_whole() {
        let rom = rom();
        let packed = miniz_oxide::deflate::compress_to_vec(&rom, 6);
        let f = F { name: "Games/Sonic.md", method: 8, flags: 0, stored: packed, size: rom.len(), crc: crc32fast::hash(&rom) };
        let (name, data) = run("one", &[f], &["md", "bin"]).unwrap();
        assert_eq!((name.as_str(), data == rom), ("Sonic.md", true), "deflated, without its folder");
        assert_eq!(run("stored", &[stored("only.rom", b"abc")], &["md"]).unwrap(), ("only.rom".to_string(), b"abc".to_vec()), "stored; no wanted extension: the one file");
    }

    #[test]
    fn a_readme_or_pictures_next_to_the_rom_do_not_matter() {
        let rom = rom();
        let files = [
            stored("readme.txt", b"hi"),
            stored("__MACOSX/._Sonic.md", b"x"),
            stored("Sonic.md", &rom),
            stored("cover.JPG", b"jpeg"),
            stored("Sonic.nfo", b"info"),
        ];
        let (name, data) = run("readme", &files, &["md"]).unwrap();
        assert_eq!((name.as_str(), data == rom), ("Sonic.md", true));
        // The same when the core's list does not name the extension: the one file that is no readme.
        let (name, _) = run("readme2", &files, &["bin"]).unwrap();
        assert_eq!(name, "Sonic.md");
    }

    #[test]
    fn two_games_in_one_zip_are_refused_whatever_their_order() {
        let files = [stored("Game (USA).md", b"one"), stored("Game (Europe).md", b"two")];
        let e = run("multi", &files, &["md"]).unwrap_err();
        assert_eq!(e, "multi.zip holds 2 game files. Unpack it on a computer and keep only the one you want.");
        assert!(is_refusal(&e));
        // A readme does not make it two; a third game makes it three.
        let files = [stored("a.md", b"1"), stored("b.md", b"2"), stored("c.md", b"3"), stored("readme.txt", b"r")];
        assert!(run("multi3", &files, &["md"]).unwrap_err().starts_with("multi3.zip holds 3 game files."));
        // No wanted extension and two unknown files: still not a guess.
        let files = [stored("a.rom", b"1"), stored("b.rom", b"2")];
        assert!(is_refusal(&run("multi4", &files, &["md"]).unwrap_err()));
    }

    #[test]
    fn the_crc_comes_from_the_directory_without_unpacking() {
        let rom = rom();
        let packed = miniz_oxide::deflate::compress_to_vec(&rom, 6);
        let dir = TestDir::new("zipcrc");
        let p = dir.join("game.zip");
        // The stored CRC is what counts: a made-up one comes back as it is, so nothing was unpacked to work it out.
        let files = [stored("readme.txt", b"hi"), F { name: "Game (USA).gba", method: 8, flags: 0, stored: packed, size: rom.len(), crc: 0x1234_5678 }];
        std::fs::write(&p, zip(&files)).unwrap();
        assert_eq!(rom_crc(&p, &[]), Some((0x1234_5678, rom.len() as u64)));
        // A directory longer than the last 64 KB read first: 3,000 readmes with long names and the game.
        let names: Vec<&'static str> = (0..3000).map(|i| &*Box::leak(format!("docs/a-rather-long-folder-name/readme-{i:04}.txt").into_boxed_str())).collect();
        let mut files: Vec<F> = names.iter().map(|n| stored(n, b"r")).collect();
        files.push(stored("Game (USA).gba", &rom));
        let z = zip(&files);
        assert!(z.len() > 3 * 65536);
        std::fs::write(&p, z).unwrap();
        assert_eq!(rom_crc(&p, &[]), Some((crc32fast::hash(&rom), rom.len() as u64)));
        // Two games, no zip at all, no file: no checksum.
        std::fs::write(&p, zip(&[stored("a.gba", b"1"), stored("b.gba", b"2")])).unwrap();
        assert_eq!(rom_crc(&p, &[]), None);
        std::fs::write(&p, b"not a zip").unwrap();
        assert_eq!(rom_crc(&p, &[]), None);
        assert_eq!(rom_crc(&dir.join("gone.zip"), &[]), None);
    }

    #[test]
    fn a_zip_without_a_game_says_so() {
        let e = run("texts", &[stored("readme.txt", b"hi"), stored("cover.png", b"p")], &["md"]).unwrap_err();
        assert_eq!(e, "There is no game file in texts.zip. Check that it is the right file.");
        assert!(is_refusal(&e));
        let e = run("empty", &[], &["md"]).unwrap_err();
        assert_eq!(e, "empty.zip is empty. Check that it is the right file.");
    }

    #[test]
    fn bytes_that_do_not_match_the_crc_are_refused() {
        let mut f = stored("x.md", b"some game");
        f.crc ^= 1;
        let e = run("crc", &[f], &["md"]).unwrap_err();
        assert_eq!(e, "crc.zip is damaged: the game inside doesn't match its checksum. Copy the file to the SD card again.");
        assert!(is_refusal(&e));
        // One flipped byte in a stored entry, the claimed CRC being the original's.
        let mut f = stored("x.md", &rom());
        f.stored[100] ^= 0x40;
        assert!(run("crc2", &[f], &["md"]).unwrap_err().contains("checksum"));
        // Deflated data that unpacks to something else of the same length.
        let mut other = rom();
        other[7] ^= 1;
        let f = F { name: "x.md", method: 8, flags: 0, stored: miniz_oxide::deflate::compress_to_vec(&other, 6), size: other.len(), crc: crc32fast::hash(&rom()) };
        assert!(run("crc3", &[f], &["md"]).unwrap_err().contains("checksum"));
    }

    #[test]
    fn an_unsupported_method_is_named_not_guessed() {
        for method in [9, 12, 14, 93, 98] {
            let f = F { method, ..stored("x.md", b"abc") };
            let e = run("method", &[f], &["md"]).unwrap_err();
            assert_eq!(e, "method.zip is packed in a way pocket-core can't open. Pack it again as a normal zip file, or copy the game file itself.");
            assert!(is_refusal(&e));
        }
    }

    #[test]
    fn an_encrypted_entry_is_named_not_unpacked_to_noise() {
        // The traditional password flag, and WinZip's AES (method 99).
        for (flags, method) in [(1, 8), (1, 0), (0x41, 99), (0, 99)] {
            let f = F { flags, method, ..stored("x.md", b"abc") };
            let e = run("secret", &[f], &["md"]).unwrap_err();
            assert_eq!(e, "secret.zip is password-protected, and pocket-core can't open it. Unpack it on a computer and copy the game file itself.");
            assert!(is_refusal(&e));
        }
    }

    #[test]
    fn damaged_zips_are_errors_not_crashes() {
        let dir = TestDir::new("zip-bad");
        let p = dir.join("c.zip");
        std::fs::write(&p, b"PK\x03\x04 not really").unwrap();
        let e = extract_rom(&p, &[]).unwrap_err();
        assert_eq!(e, "c.zip is damaged or not complete. Copy the file to the SD card again.");
        assert!(is_refusal(&e), "no end record: cut off on the way, nothing would open it (VDEV-ROUND-2026-10-25)");
        // A good zip cut short, as an interrupted copy leaves it.
        let whole = zip(&[stored("Pipes.sfc", &[7; 4000])]);
        std::fs::write(&p, &whole[..whole.len() / 2]).unwrap();
        assert!(is_refusal(&extract_rom(&p, &["sfc"]).unwrap_err()));
        // An end record whose directory can't be read (ZIP64 and the like): RetroArch may still open it.
        let mut odd = whole.clone();
        let at = odd.len() - 22 + 16;
        odd[at..at + 4].copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
        std::fs::write(&p, &odd).unwrap();
        let e = extract_rom(&p, &["sfc"]).unwrap_err();
        assert_eq!(e, "not a valid zip file");
        assert!(!is_refusal(&e), "an unreadable zip may still open elsewhere (RetroArch)");
        let f = F { method: 8, stored: vec![1, 2, 3], size: 100, ..stored("x.md", b"") };
        let e = run("short", &[f], &["md"]).unwrap_err();
        assert_eq!(e, "the zip file is damaged");
        assert!(!is_refusal(&e));
        assert!(!is_refusal("Final Quest could not load /storage/roms/md/x.md"));
    }
}
