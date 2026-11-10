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
/// Largest zip read into memory to unpack it: the ROM inside is at most [`MAX_ROM`]; this leaves room for a manual or
/// pictures beside it. Checked by size before a byte is read (security review M5: a 9 GB "zip" was read whole first).
pub const MAX_ZIP: u64 = 128 << 20;
/// Largest game file a core that takes the game in memory gets (the largest cartridges, a Nintendo DS card, are 512 MB);
/// also checked before reading. Cores that read the file themselves (`need_fullpath`: discs) are not bounded here.
pub const MAX_IN_MEMORY: u64 = 512 << 20;

/// The whole of `path`, refused before anything is read when it is larger than `max` bytes.
pub fn read_bounded(path: &Path, max: u64) -> Result<Vec<u8>, String> {
    let len = std::fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?.len();
    if len > max {
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        return Err(format!("{name} is too big to load ({} MB; at most {} MB)", len >> 20, max >> 20));
    }
    std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))
}

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
    unix_mode: usize,
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
            unix_mode: le32(z, at + 38).ok_or_else(bad)? >> 16,
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
    directory_in(&mut std::fs::File::open(zip_path).map_err(|e| e.to_string())?)
}

/// `directory_of` over anything that reads and seeks: a file, or an open Android document.
fn directory_in(f: &mut (impl std::io::Read + std::io::Seek)) -> Result<Option<Vec<Entry>>, String> {
    use std::io::SeekFrom;
    let bad = |e: std::io::Error| e.to_string();
    let len = f.seek(SeekFrom::End(0)).map_err(bad)?;
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
    refusal_in(&mut std::fs::File::open(zip_path).ok()?, zip_name, wanted, only_cut_off)
}

/// `refusal_of` for a zip that is open already; `zip_name` is how the sentence calls it.
pub fn refusal_in(zip: &mut (impl std::io::Read + std::io::Seek), zip_name: &str, wanted: &[&str], only_cut_off: bool) -> Option<String> {
    let list = match directory_in(zip) {
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
    let z = read_bounded(zip_path, MAX_ZIP)?;
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

/// Extract the pinned Python runtime without relying on stock firmware's optional unzip applet.
/// The generic extractor keeps all regular files; the CLI uses the headless Python policy below.
/// Every policy validates all entry paths, headers and size limits before writing.
pub fn unpack_runtime(zip_path: &Path, destination: &Path) -> Result<(), String> {
    unpack_runtime_filtered(zip_path, destination, |_| false)
}

/// The headless store needs the standard library, but not Python's test suite, pip or GUI/development files.
pub fn unpack_python_runtime(zip_path: &Path, destination: &Path) -> Result<(), String> {
    unpack_runtime_filtered(zip_path, destination, |name| {
        ["include/", "share/", "lib/python3.11/site-packages/", "lib/python3.11/test/",
         "lib/python3.11/idlelib/", "lib/python3.11/turtledemo/", "lib/python3.11/ensurepip/",
         "lib/python3.11/ctypes/test/", "lib/python3.11/distutils/tests/", "lib/python3.11/unittest/test/",
         "lib/python3.11/lib2to3/tests/"].iter().any(|prefix| name.starts_with(prefix))
    })
}

const RUNTIME_MAX_ENTRY: usize = 128 << 20;
const RUNTIME_MAX_TOTAL: usize = 700 << 20;
const RUNTIME_MAX_OUTPUTS: usize = 100_000;
const RUNTIME_MAX_NAMES: usize = 16 << 20;

enum RuntimeNode { Directory, File(usize), Link(String) }

struct RuntimePlan<'a> {
    entries: &'a [Entry],
    outputs: Vec<(String, Option<usize>)>,
    expanded: usize,
    name_bytes: usize,
}

fn unpack_runtime_filtered(zip_path: &Path, destination: &Path, skip: impl Fn(&str) -> bool) -> Result<(), String> {
    use std::io::Write;
    let bytes = read_bounded(zip_path, MAX_ZIP)?;
    let bad = || "The runtime zip is damaged or unsupported.".to_string();
    let end = find_end(&bytes).ok_or_else(bad)?;
    let (count, directory_at, directory_size) = end_record(&bytes)?;
    if le16(&bytes, end + 4) != Some(0) || le16(&bytes, end + 6) != Some(0)
        || le16(&bytes, end + 8) != Some(count)
        || directory_at.checked_add(directory_size) != Some(end)
        || end.checked_add(22).and_then(|n| n.checked_add(le16(&bytes, end + 20)?)) != Some(bytes.len()) {
        return Err(bad());
    }
    if directory_size > MAX_DIRECTORY { return Err("The runtime zip directory is too big.".into()); }
    let list = entries(&bytes)?;
    let mut namespace_bytes = 0usize;
    let mut names = std::collections::HashMap::<String, String>::new();
    let mut explicit = std::collections::HashSet::new();
    let mut tree = std::collections::BTreeMap::new();
    tree.insert(String::new(), RuntimeNode::Directory);
    let mut total = 0usize;
    let mut existing = RuntimeDestination::default();
    existing.check(destination)?;
    // Validate the complete namespace and every header before creating any destination file.
    for (index, entry) in list.iter().enumerate() {
        let relative = runtime_path(&entry.name)?.to_string_lossy().into_owned();
        if !explicit.insert(relative.clone()) { return Err("The runtime zip repeats a path.".into()); }
        let kind = entry.unix_mode & 0o170000;
        if !matches!(kind, 0 | 0o100000 | 0o040000 | 0o120000) { return Err("The runtime zip contains a special file.".into()); }
        if entry.usize == u32::MAX as usize || entry.csize == u32::MAX as usize {
            return Err("Runtime ZIP64 files are unsupported; FAT32 files must be smaller than 4 GiB.".into());
        }
        if entry.usize > RUNTIME_MAX_ENTRY { return Err("A runtime file exceeds the 128 MiB unpacking size limit.".into()); }
        if entry.usize as u64 > (entry.csize as u64).saturating_mul(1000).saturating_add(65536) {
            return Err("The runtime zip exceeds the compression ratio limit.".into());
        }
        let directory = entry.name.ends_with('/') || kind == 0o040000;
        if entry.flags & 0x41 != 0 || !matches!(entry.method, 0 | 8)
            || directory && (entry.usize != 0 || kind == 0o100000 || kind == 0o120000) {
            return Err(bad());
        }
        total = total.checked_add(entry.usize).filter(|n| *n <= RUNTIME_MAX_TOTAL).ok_or("The runtime zip expands beyond its size limit.")?;
        runtime_raw(&bytes, entry, directory_at)?;
        let node = if directory { RuntimeNode::Directory } else if kind == 0o120000 {
            if entry.usize > 4096 { return Err("A runtime link target is too long.".into()); }
            let data = runtime_data(&bytes, entry, directory_at)?;
            let target = std::str::from_utf8(&data).map_err(|_| "The runtime zip contains an invalid link target.")?;
            RuntimeNode::Link(runtime_link_target(&relative, target)?)
        } else { RuntimeNode::File(index) };
        let parts: Vec<_> = relative.split('/').collect();
        let mut prefix = String::new();
        for (at, part) in parts.iter().enumerate() {
            if !prefix.is_empty() { prefix.push('/'); }
            prefix.push_str(part);
            let folded = runtime_case_key(&prefix);
            if let Some(previous) = names.get(&folded) {
                if previous != &prefix { return Err("The runtime zip contains conflicting case-insensitive names.".into()); }
            } else {
                namespace_bytes = namespace_bytes.checked_add(prefix.len()).filter(|n| *n <= RUNTIME_MAX_NAMES).ok_or("The runtime zip namespace exceeds its size limit.")?;
                names.insert(folded, prefix.clone());
            }
            if at + 1 < parts.len() {
                match tree.get(&prefix) {
                    Some(RuntimeNode::Directory) => {}
                    Some(_) => return Err("The runtime zip uses a file or link as a parent folder.".into()),
                    None => { tree.insert(prefix.clone(), RuntimeNode::Directory); }
                }
            }
        }
        if tree.get(&relative).is_some_and(|previous| !matches!((previous, &node), (RuntimeNode::Directory, RuntimeNode::Directory))) {
            return Err("The runtime zip uses one name as both a file and a folder.".into());
        }
        tree.insert(relative, node);
    }
    let mut children = std::collections::HashMap::<String, Vec<String>>::new();
    for name in tree.keys().filter(|name| !name.is_empty()) {
        let parent = name.rsplit_once('/').map_or("", |(parent, _)| parent);
        children.entry(parent.to_string()).or_default().push(name.clone());
    }
    let mut plan = RuntimePlan { entries: &list, outputs: Vec::new(), expanded: 0, name_bytes: 0 };
    runtime_walk("", "", &tree, &children, &mut std::collections::HashSet::new(), &mut plan)?;
    for (name, _) in &plan.outputs { existing.check(&destination.join(name))?; }
    std::fs::create_dir_all(destination).map_err(|e| e.to_string())?;
    for (name, source) in plan.outputs {
        if skip(&name) || source.is_none() && skip(&format!("{name}/")) { continue; }
        let path = destination.join(&name);
        let Some(index) = source else {
            std::fs::create_dir_all(path).map_err(|e| e.to_string())?;
            continue;
        };
        let data = runtime_data(&bytes, &list[index], directory_at)?;
        std::fs::create_dir_all(path.parent().ok_or_else(bad)?).map_err(|e| e.to_string())?;
        let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(&path).map_err(|e| e.to_string())?;
        file.write_all(&data).map_err(|e| e.to_string())?;
        // FAT/exFAT own modes and creation times. Never restore Unix modes or archive timestamps.
    }
    Ok(())
}

fn runtime_component(part: &str) -> bool {
    !part.is_empty() && !matches!(part, "." | "..") && !part.ends_with(['.', ' ']) && part.len() <= 255
        && !part.chars().any(|ch| ch.is_control() || "\\:*?\"<>|".contains(ch))
}

fn runtime_path(name: &str) -> Result<std::path::PathBuf, String> {
    let name = name.strip_suffix('/').unwrap_or(name);
    if name.len() > 4096 || !name.split('/').all(runtime_component) {
        return Err("The runtime zip contains an unsafe path or a name unsupported by FAT32/exFAT.".into());
    }
    Ok(name.into())
}

fn runtime_case_key(name: &str) -> String {
    name.chars().flat_map(char::to_lowercase).flat_map(char::to_uppercase).flat_map(char::to_lowercase).collect()
}

fn runtime_link_target(name: &str, target: &str) -> Result<String, String> {
    let mut parts: Vec<_> = name.split('/').collect();
    parts.pop();
    let target = target.strip_suffix('/').unwrap_or(target);
    if target.is_empty() || target.len() > 4096 || target.starts_with('/') { return Err("The runtime zip contains an external or invalid link.".into()); }
    for part in target.split('/') {
        match part {
            "." => {}
            ".." => { if parts.pop().is_none() { return Err("A runtime link escapes the archive.".into()); } }
            _ if runtime_component(part) => parts.push(part),
            _ => return Err("The runtime zip contains an external or invalid link.".into()),
        }
    }
    Ok(parts.join("/"))
}

/// A target may name a file below an archive directory link (copy/file, where copy -> data).
/// Resolve each linked prefix before looking up the remaining suffix, without touching the filesystem.
fn runtime_resolve_link(target: &str, tree: &std::collections::BTreeMap<String, RuntimeNode>) -> Result<String, String> {
    let mut path = target.to_string();
    let mut links = std::collections::HashSet::new();
    for _ in 0..128 {
        if path.len() > 4096 { return Err("A materialized runtime link path is too long.".into()); }
        if matches!(tree.get(&path), Some(RuntimeNode::File(_) | RuntimeNode::Directory)) { return Ok(path); }
        let parts: Vec<_> = path.split('/').collect();
        let mut prefix = String::new();
        let mut replacement = None;
        for (at, part) in parts.iter().enumerate() {
            if !prefix.is_empty() { prefix.push('/'); }
            prefix.push_str(part);
            match tree.get(&prefix) {
                Some(RuntimeNode::Link(next)) => {
                    if !links.insert(prefix.clone()) { return Err("The runtime zip contains a link cycle.".into()); }
                    let suffix = parts[at + 1..].join("/");
                    replacement = Some(if suffix.is_empty() { next.clone() } else if next.is_empty() { suffix } else { format!("{next}/{suffix}") });
                    break;
                }
                Some(RuntimeNode::Directory) => {}
                Some(RuntimeNode::File(_)) => return Err("A runtime link target uses a file as a folder.".into()),
                None => return Err("A runtime link target is missing from the archive.".into()),
            }
        }
        path = replacement.ok_or("A runtime link target is missing from the archive.")?;
    }
    Err("The runtime zip contains a link cycle or an excessive link depth.".into())
}

fn runtime_walk(source: &str, destination: &str, tree: &std::collections::BTreeMap<String, RuntimeNode>, children: &std::collections::HashMap<String, Vec<String>>, visiting: &mut std::collections::HashSet<String>, plan: &mut RuntimePlan<'_>) -> Result<(), String> {
    if visiting.len() >= 128 || !visiting.insert(source.to_string()) { return Err("The runtime zip contains a link cycle or an excessive folder depth.".into()); }
    match tree.get(source).ok_or("A runtime link target is missing from the archive.")? {
        RuntimeNode::Link(target) => runtime_walk(&runtime_resolve_link(target, tree)?, destination, tree, children, visiting, plan)?,
        node => {
            if !destination.is_empty() {
                if plan.outputs.len() >= RUNTIME_MAX_OUTPUTS { return Err("The runtime zip expands beyond its entry count limit.".into()); }
                let entry = if let RuntimeNode::File(index) = node {
                    plan.expanded = plan.expanded.checked_add(plan.entries[*index].usize).filter(|n| *n <= RUNTIME_MAX_TOTAL).ok_or("The runtime zip expands beyond its size limit.")?;
                    Some(*index)
                } else { None };
                plan.name_bytes = plan.name_bytes.checked_add(destination.len()).filter(|n| *n <= RUNTIME_MAX_NAMES).ok_or("The runtime zip materialized names exceed their size limit.")?;
                plan.outputs.push((destination.to_string(), entry));
            }
            if matches!(node, RuntimeNode::Directory) {
                for child in children.get(source).into_iter().flatten() {
                    let name = child.rsplit('/').next().unwrap_or(child);
                    let output = if destination.is_empty() { name.to_string() } else { format!("{destination}/{name}") };
                    if output.len() > 4096 { return Err("A materialized runtime link path is too long.".into()); }
                    runtime_walk(child, &output, tree, children, visiting, plan)?;
                }
            }
        }
    }
    visiting.remove(source);
    Ok(())
}

fn runtime_data(bytes: &[u8], entry: &Entry, directory_at: usize) -> Result<Vec<u8>, String> {
    let raw = runtime_raw(bytes, entry, directory_at)?;
    let data = if entry.method == 0 { raw.to_vec() } else {
        miniz_oxide::inflate::decompress_to_vec_with_limit(raw, entry.usize).map_err(|_| "The runtime zip is damaged or unsupported.")?
    };
    if data.len() != entry.usize || crc32fast::hash(&data) != entry.crc { return Err("The runtime zip failed its checksum check.".into()); }
    Ok(data)
}

/// A preflight snapshot of the existing namespace. Prefixes/directories are read once even for
/// thousands of Python files on a slow FAT card; the complete output plan has no aliases of its own.
#[derive(Default)]
struct RuntimeDestination {
    checked: std::collections::HashSet<std::path::PathBuf>,
    children: std::collections::HashMap<std::path::PathBuf, std::collections::HashMap<String, Option<std::ffi::OsString>>>,
}

impl RuntimeDestination {
    fn check(&mut self, path: &Path) -> Result<(), String> {
        let mut checked = std::path::PathBuf::new();
        for component in path.components() {
            if matches!(component, std::path::Component::ParentDir | std::path::Component::CurDir | std::path::Component::Prefix(_)) {
                return Err("The runtime destination is unsafe.".into());
            }
            checked.push(component.as_os_str());
            if !self.checked.insert(checked.clone()) { continue; }
            if let (Some(parent), Some(name)) = (checked.parent(), checked.file_name()) {
                if let std::collections::hash_map::Entry::Vacant(slot) = self.children.entry(parent.to_path_buf()) {
                    let mut names = std::collections::HashMap::new();
                    match std::fs::read_dir(if parent.as_os_str().is_empty() { Path::new(".") } else { parent }) {
                        Ok(entries) => for entry in entries {
                            let name = entry.map_err(|e| e.to_string())?.file_name();
                            let key = runtime_case_key(&name.to_string_lossy());
                            names.entry(key).and_modify(|existing| *existing = None).or_insert(Some(name));
                        },
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                        Err(error) => return Err(error.to_string()),
                    }
                    slot.insert(names);
                }
                if self.children[parent].get(&runtime_case_key(&name.to_string_lossy())).is_some_and(|existing| existing.as_deref() != Some(name)) {
                    return Err("The runtime destination contains a conflicting case-insensitive name.".into());
                }
            }
            match std::fs::symlink_metadata(&checked) {
                Ok(meta) if meta.file_type().is_symlink() || !meta.is_dir() => return Err("The runtime destination contains a symbolic link or existing file.".into()),
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.to_string()),
            }
        }
        Ok(())
    }
}

fn runtime_raw<'a>(bytes: &'a [u8], entry: &Entry, directory_at: usize) -> Result<&'a [u8], String> {
    let bad = || "The runtime zip is damaged or unsupported.".to_string();
    let at = entry.local;
    if !bytes.get(at..).is_some_and(|data| data.starts_with(b"PK\x03\x04"))
        || le16(bytes, at + 6) != Some(entry.flags) || le16(bytes, at + 8) != Some(entry.method) { return Err(bad()); }
    let name_len = le16(bytes, at + 26).ok_or_else(bad)?;
    let name_at = at.checked_add(30).ok_or_else(bad)?;
    let name_end = name_at.checked_add(name_len).ok_or_else(bad)?;
    if bytes.get(name_at..name_end) != Some(entry.name.as_bytes()) { return Err(bad()); }
    let start = name_end.checked_add(le16(bytes, at + 28).ok_or_else(bad)?).ok_or_else(bad)?;
    let end = start.checked_add(entry.csize).filter(|n| *n <= directory_at).ok_or_else(bad)?;
    if entry.flags & 8 == 0 && (le32(bytes, at + 14) != Some(entry.crc as usize) || le32(bytes, at + 18) != Some(entry.csize) || le32(bytes, at + 22) != Some(entry.usize)) {
        return Err(bad());
    }
    if entry.method == 0 && entry.csize != entry.usize { return Err(bad()); }
    bytes.get(start..end).ok_or_else(bad)
}

#[cfg(test)]
mod bounded_tests {
    use super::*;

    /// Security review M5: a huge file is refused by its size, before it is read into memory.
    #[test]
    fn a_huge_zip_or_rom_is_refused_before_it_is_read() {
        let dir = tempfile::tempdir().unwrap();
        let big = dir.path().join("Huge.zip");
        std::fs::File::create(&big).unwrap().set_len(MAX_ZIP + 1).unwrap();
        let e = extract_rom(&big, &["gba"]).unwrap_err();
        assert!(e.contains("too big to load"), "{e}");
        let rom = dir.path().join("Huge.gba");
        std::fs::File::create(&rom).unwrap().set_len(MAX_IN_MEMORY + 1).unwrap();
        assert!(read_bounded(&rom, MAX_IN_MEMORY).unwrap_err().contains("too big"));
        std::fs::write(dir.path().join("Small.gba"), b"rom").unwrap();
        assert_eq!(read_bounded(&dir.path().join("Small.gba"), MAX_IN_MEMORY).unwrap(), b"rom");
    }
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

    fn runtime_extract(files: &[F]) -> (TestDir, Result<(), String>) {
        let dir = TestDir::new("runtime-zip");
        let source = dir.join("runtime.zip");
        std::fs::write(&source, zip(files)).unwrap();
        let result = unpack_runtime(&source, &dir.join("out"));
        (dir, result)
    }

    #[test]
    fn runtime_zip_extracts_all_nested_python_files_and_checks_deflated_data() {
        let data = b"binary python runtime";
        let files = [stored("bin/", b""), stored("bin/python3", data), F { name: "lib/python3.11/os.py", method: 8, stored: miniz_oxide::deflate::compress_to_vec(b"import sys", 6), ..stored("lib/python3.11/os.py", b"import sys") }, stored("README", b"licence")];
        let (dir, result) = runtime_extract(&files);
        result.unwrap();
        assert_eq!(std::fs::read(dir.join("out/bin/python3")).unwrap(), data);
        assert_eq!(std::fs::read(dir.join("out/lib/python3.11/os.py")).unwrap(), b"import sys");
        assert_eq!(std::fs::read(dir.join("out/README")).unwrap(), b"licence");
    }

    #[test]
    fn headless_runtime_skips_tests_and_build_tools_but_keeps_standard_library_caches() {
        let dir = TestDir::new("runtime-headless");
        let source = dir.join("runtime.zip");
        let files = [stored("include/python.h", b"header"), stored("share/man/python", b"manual"),
            stored("lib/python3.11/test/__pycache__/test_os.pyc", b"tests"), stored("lib/python3.11/idlelib/editor.py", b"gui"),
            stored("lib/python3.11/site-packages/pip/main.py", b"pip"), stored("lib/python3.11/os.py", b"os"),
            stored("lib/python3.11/__pycache__/os.pyc", b"cached"), stored("bin/python3", b"binary")];
        std::fs::write(&source, zip(&files)).unwrap();
        unpack_python_runtime(&source, &dir.join("out")).unwrap();
        for skipped in ["include", "share", "lib/python3.11/test", "lib/python3.11/idlelib", "lib/python3.11/site-packages"] {
            assert!(!dir.join("out").join(skipped).exists(), "avoid writing and deleting {skipped} on FAT");
        }
        assert_eq!(std::fs::read(dir.join("out/lib/python3.11/__pycache__/os.pyc")).unwrap(), b"cached");
        assert_eq!(std::fs::read(dir.join("out/lib/python3.11/os.py")).unwrap(), b"os");
        std::fs::write(&source, zip(&[stored("include/../escape", b"bad")])).unwrap();
        assert!(unpack_python_runtime(&source, &dir.join("bad")).is_err(), "excluded trees still have their paths checked");
        let mut malformed = zip(&[stored("include/python.h", b"header")]);
        malformed[0] = 0;
        std::fs::write(&source, malformed).unwrap();
        assert!(unpack_python_runtime(&source, &dir.join("bad")).is_err(), "excluded entries still have their local headers checked");
    }

    #[test]
    fn runtime_zip_rejects_unsafe_paths_before_creating_any_files() {
        for name in ["../outside", "/absolute", "a/../../outside", "a\\outside", "a/./file", "a//file", "C:/file", "bad\0file"] {
            let (dir, result) = runtime_extract(&[stored("good", b"good"), stored(name, b"bad")]);
            assert!(result.is_err(), "reject {name:?}");
            assert!(!dir.join("out/good").exists(), "validate the entire directory before writing");
        }
    }

    #[test]
    fn runtime_zip_refuses_crc_damage_and_expansion_bombs() {
        let mut corrupt = stored("bin/python3", b"binary");
        corrupt.crc ^= 1;
        assert!(runtime_extract(&[corrupt]).1.is_err());
        let oversized = F { method: 8, size: (128 << 20) + 1, ..stored("large", b"tiny") };
        assert!(runtime_extract(&[oversized]).1.is_err());
        let files: Vec<_> = ["a", "b", "c", "d", "e", "f"].into_iter().map(|name| F { method: 8, size: 128 << 20, ..stored(name, b"tiny") }).collect();
        let (dir, result) = runtime_extract(&files);
        assert!(result.is_err());
        assert!(!dir.join("out/a").exists(), "reject the total expansion before unpacking any entry");
        let corrupt = F { method: 8, stored: vec![1, 2, 3], ..stored("broken", b"binary") };
        assert!(runtime_extract(&[corrupt]).1.is_err());
    }

    #[test]
    fn runtime_zip_refuses_external_links_and_symlinks_in_existing_destinations() {
        let dir = TestDir::new("runtime-link");
        let source = dir.join("runtime.zip");
        let mut bytes = zip(&[stored("bin/python3", b"../../outside")]);
        let central = bytes.windows(4).position(|p| p == b"PK\x01\x02").unwrap();
        bytes[central + 38..central + 42].copy_from_slice(&(0o120777u32 << 16).to_le_bytes());
        std::fs::write(&source, bytes).unwrap();
        assert!(unpack_runtime(&source, &dir.join("out")).is_err());
        std::fs::write(&source, zip(&[stored("bin/python3", b"binary")])).unwrap();
        std::fs::create_dir_all(dir.join("out")).unwrap();
        std::fs::create_dir_all(dir.join("outside")).unwrap();
        #[cfg(unix)] {
            std::os::unix::fs::symlink(dir.join("outside"), dir.join("out/bin")).unwrap();
            assert!(unpack_runtime(&source, &dir.join("out")).is_err());
            assert!(!dir.join("outside/python3").exists(), "never follow an existing destination symlink");
        }
    }

    fn portable_zip(files: &[F], links: &[usize]) -> Vec<u8> {
        let mut bytes = zip(files);
        let (_, mut at, _) = end_record(&bytes).unwrap();
        for (index, file) in files.iter().enumerate() {
            if links.contains(&index) { bytes[at + 38..at + 42].copy_from_slice(&(0o120777u32 << 16).to_le_bytes()); }
            at += 46 + file.name.len();
        }
        bytes
    }

    fn portable_extract(files: &[F], links: &[usize]) -> (TestDir, Result<(), String>) {
        let dir = TestDir::new("portable-runtime");
        let source = dir.join("runtime.zip");
        std::fs::write(&source, portable_zip(files, links)).unwrap();
        let result = unpack_runtime(&source, &dir.join("out"));
        (dir, result)
    }

    #[test]
    fn portable_runtime_materializes_internal_file_links_and_forward_chains() {
        let files = [stored("lib/alias.so", b"next.so"), stored("lib/next.so", b"real.so"), stored("lib/real.so", b"library"),
            stored("bin/python3", b"../lib/real.so")];
        let (dir, result) = portable_extract(&files, &[0, 1, 3]);
        result.unwrap();
        for name in ["lib/alias.so", "lib/next.so", "lib/real.so", "bin/python3"] {
            assert_eq!(std::fs::read(dir.join("out").join(name)).unwrap(), b"library");
            assert!(!std::fs::symlink_metadata(dir.join("out").join(name)).unwrap().file_type().is_symlink(), "{name} must work without filesystem links");
        }
    }

    #[test]
    fn portable_runtime_materializes_internal_directory_links() {
        let files = [stored("alias", b"real"), stored("real/lib/module.so", b"module"), stored("real/lib/config.ini", b"config")];
        let (dir, result) = portable_extract(&files, &[0]);
        result.unwrap();
        assert_eq!(std::fs::read(dir.join("out/alias/lib/module.so")).unwrap(), b"module");
        assert_eq!(std::fs::read(dir.join("out/alias/lib/config.ini")).unwrap(), b"config");
        assert!(!std::fs::symlink_metadata(dir.join("out/alias")).unwrap().file_type().is_symlink());
    }

    #[test]
    fn portable_runtime_resolves_files_below_directory_link_prefixes() {
        let files = [stored("game/data/file", b"game data"), stored("game/copy", b"data"), stored("game/chain", b"copy/file"),
            stored("game/lib/lib.so", b"lib.so.1"), stored("game/lib/lib.so.1", b"library")];
        let (dir, result) = portable_extract(&files, &[1, 2, 3]);
        result.unwrap();
        assert_eq!(std::fs::read(dir.join("out/game/chain")).unwrap(), b"game data");
        assert_eq!(std::fs::read(dir.join("out/game/copy/file")).unwrap(), b"game data");
        assert_eq!(std::fs::read(dir.join("out/game/lib/lib.so")).unwrap(), b"library");
        assert!(!std::fs::symlink_metadata(dir.join("out/game/chain")).unwrap().file_type().is_symlink());
        let (dir, result) = portable_extract(&[stored("good", b"good"), stored("a", b"b/file"), stored("b", b"a")], &[1, 2]);
        assert!(result.unwrap_err().contains("cycle"), "directory-prefix alias cycles remain bounded");
        assert!(!dir.join("out/good").exists());
    }

    #[test]
    fn portable_runtime_refuses_external_missing_cyclic_and_damaged_links_before_writing() {
        for target in ["/outside", "../../outside", "missing", "C:/outside", "..\\outside"] {
            let (dir, result) = portable_extract(&[stored("good", b"good"), stored("lib/link", target.as_bytes())], &[1]);
            assert!(result.is_err(), "reject link target {target}");
            assert!(!dir.join("out/good").exists(), "validate all links before copying files");
        }
        let (dir, result) = portable_extract(&[stored("good", b"good"), stored("a", b"b"), stored("b", b"a")], &[1, 2]);
        assert!(result.unwrap_err().to_lowercase().contains("cycle"));
        assert!(!dir.join("out/good").exists());
        let (dir, result) = portable_extract(&[stored("real/file", b"good"), stored("real/loop", b"..")], &[1]);
        assert!(result.is_err(), "a directory alias must not recurse into its own ancestor");
        assert!(!dir.join("out/real/file").exists());
        let mut bad = stored("link", b"real"); bad.crc ^= 1;
        let (dir, result) = portable_extract(&[stored("good", b"good"), bad, stored("real", b"payload")], &[1]);
        assert!(result.is_err()); assert!(!dir.join("out/good").exists());
    }

    #[test]
    fn portable_runtime_refuses_fat_names_and_case_aliases_before_writing() {
        for name in ["bad:name", "bad*name", "bad?name", "bad\"name", "bad<name", "bad>name", "bad|name", "bad.", "bad ", "dir./file", "dir /file", "bad\tname"] {
            let (dir, result) = portable_extract(&[stored("good", b"good"), stored(name, b"bad")], &[]);
            assert!(result.is_err(), "reject FAT name {name:?}");
            assert!(!dir.join("out/good").exists());
        }
        for files in [[stored("lib/A.so", b"one"), stored("lib/a.so", b"two")], [stored("LIB/one", b"one"), stored("lib/two", b"two")], [stored("lib/ß.so", b"one"), stored("lib/ẞ.so", b"two")]] {
            let (dir, result) = portable_extract(&files, &[]);
            assert!(result.unwrap_err().to_lowercase().contains("case"), "explain case-insensitive path conflicts");
            assert!(!dir.join("out").exists());
        }
    }

    #[test]
    fn portable_runtime_refuses_case_aliases_of_existing_destination_names() {
        for folder_alias in [true, false] {
            let dir = TestDir::new("runtime-existing-case");
            let source = dir.join("runtime.zip");
            let existing = if folder_alias { "LIB" } else { "lib" };
            std::fs::create_dir_all(dir.join("out").join(existing)).unwrap();
            std::fs::write(dir.join("out").join(existing).join("One.so"), b"keep").unwrap();
            let name = if folder_alias { "lib/new.so" } else { "lib/one.so" };
            std::fs::write(&source, zip(&[stored("good", b"good"), stored(name, b"replace")])).unwrap();
            let error = unpack_runtime(&source, &dir.join("out")).unwrap_err();
            assert!(error.to_lowercase().contains("case"), "explain the existing case alias: {error}");
            assert_eq!(std::fs::read(dir.join("out").join(existing).join("One.so")).unwrap(), b"keep");
            assert!(!dir.join("out/good").exists(), "refuse the collision before copying any new files");
        }
    }

    #[test]
    fn portable_runtime_explains_fat_and_compression_limits_before_writing() {
        let huge = F { method: 8, size: u32::MAX as usize, ..stored("huge", b"tiny") };
        let (dir, result) = portable_extract(&[stored("good", b"good"), huge], &[]);
        assert!(result.unwrap_err().contains("4 GiB"));
        assert!(!dir.join("out/good").exists());
        let bomb = F { method: 8, size: 1000 + 65536 + 1, stored: vec![0], ..stored("bomb", b"tiny") };
        let (dir, result) = portable_extract(&[stored("good", b"good"), bomb], &[]);
        assert!(result.unwrap_err().contains("compression ratio"));
        assert!(!dir.join("out/good").exists());
    }

    #[test]
    fn portable_runtime_limits_directory_metadata_before_building_its_link_graph() {
        let names: Vec<&'static str> = (0..70).map(|i| &*Box::leak(format!("file{i}").into_boxed_str())).collect();
        let files: Vec<_> = names.iter().map(|name| stored(name, b"tiny")).collect();
        let mut bytes = zip(&files);
        let (_, at, _) = end_record(&bytes).unwrap();
        let end = find_end(&bytes).unwrap();
        let mut central = Vec::new();
        let mut cursor = at;
        for file in &files {
            let mut entry = bytes[cursor..cursor + 46 + file.name.len()].to_vec();
            entry[32..34].copy_from_slice(&65535u16.to_le_bytes());
            central.extend(entry);
            central.extend(std::iter::repeat_n(b'x', 65535));
            cursor += 46 + file.name.len();
        }
        let mut tail = bytes[end..].to_vec();
        tail[12..16].copy_from_slice(&(central.len() as u32).to_le_bytes());
        bytes.truncate(at); bytes.extend(central); bytes.extend(tail);
        let dir = TestDir::new("runtime-metadata-limit");
        std::fs::write(dir.join("runtime.zip"), bytes).unwrap();
        let error = unpack_runtime(&dir.join("runtime.zip"), &dir.join("out")).unwrap_err();
        assert!(error.contains("directory"), "bound metadata allocations before expanding the namespace: {error}");
        assert!(!dir.join("out").exists());
    }

    #[test]
    fn portable_runtime_counts_materialized_link_copies_toward_the_expansion_limit() {
        let large = F { method: 8, size: 128 << 20, stored: vec![0; 135000], ..stored("real", b"tiny") };
        let files = [large, stored("alias1", b"real"), stored("alias2", b"real"), stored("alias3", b"real"), stored("alias4", b"real"), stored("alias5", b"real")];
        let (dir, result) = portable_extract(&files, &[1, 2, 3, 4, 5]);
        assert!(result.unwrap_err().contains("size limit"));
        assert!(!dir.join("out").exists());
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
