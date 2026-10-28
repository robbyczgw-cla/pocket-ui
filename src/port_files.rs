//! PortMaster ports that need the player's own game files ("bring your own files"): where the files go,
//! whether they are there, and the page that says how to copy them.
//!
//! PortMaster describes the files only in prose (`attr.inst` in the catalog and in the installed port's
//! port.json), e.g. "Purchase the game from Steam … place the game files in sprout/assets". [`expect`] reads
//! the folder and the file names out of that text; [`check`] decides before a start whether the files are
//! surely missing. It only says so when it is confident, and lets the port start whenever it is unsure.
//!
//! File sharing hides `roms/ports` (the Pocket runs programs from there, see `wifi::SHARE_VETOED`). So that
//! files can still be copied over the network, the share has a drop folder per port, `roms/port-files/<dir>`;
//! pocket-ui moves what is in it into the port's folder when the player starts the port ([`take_dropped`]).
//! Only into a sub-folder the instructions name (`assets`, `gamedata`, …), or only the files the instructions
//! name when they go into the port folder itself; never over a file that is already there, never a link.
//! pocket-ui never downloads game files itself.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use crate::gfx::Canvas;
use crate::pad::Button;
use crate::palette::*;
use crate::text::{Face, Text};

/// The drop folders, one per port: `\\<address>\games-roms\port-files\<dir>` over the share.
pub const DROP_DIR: &str = "/storage/roms/port-files";
/// The drop folders' name inside the share (the share is /storage/roms).
pub const DROP_SHARE_NAME: &str = "port-files";

/// Folder names that instructions use for "the folder in the port where your files go".
const FOLDER_WORDS: [&str; 10] = [
    "assets",
    "gamedata",
    "game_data",
    "gamefiles",
    "data",
    "roms",
    "bios",
    "install",
    "game",
    "files",
];
/// Extensions of files that instructions name as the thing to copy (data.win, Balatro.love, arco.exe).
const FILE_EXTS: [&str; 30] = [
    "win", "unx", "ios", "droid", "pck", "rpg", "exe", "love", "jar", "zip", "7z", "rar", "dat", "paq", "pak", "pk3", "wad", "grp", "rts", "con", "rgss3a",
    "rgss2a", "rgssad", "bin", "iso", "cue", "gob", "lfd", "ags", "vox",
];
/// Verbs that put a file somewhere; a file name counts only after one of them.
const PLACE_VERBS: [&str; 11] = ["copy", "place", "put", "move", "add", "transfer", "drop", "paste", "extract", "unzip", "rename"];
/// The port fetches or ships what it needs when nothing is copied: never block it.
const SELF_SUFFICIENT: [&str; 7] = [
    "downloaded automatically",
    "automatically downloaded",
    "download automatically",
    "includes the demo",
    "demo is included",
    "demo files are included",
    "shareware",
];
/// A file newer than the port's port.json by more than this means the port (or the player) has been at work in
/// its folder since it was installed.
const SETTLE: Duration = Duration::from_secs(120);
/// Files looked at in one port folder before giving up (and letting the port start).
const WALK_LIMIT: usize = 20_000;

/// What the instructions of a port say about its files.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Expect {
    /// Folders inside the port's folder the files go into, in the order the text names them ("assets",
    /// "gamedata/planet_strike"). Empty when the text names the port folder itself or nothing.
    pub folders: Vec<String>,
    /// The first folder comes from a path that names the port's own folder ("ports/sprout/assets"), not
    /// just from a phrase ("into the assets folder").
    pub sure: bool,
    /// The text names the port folder itself as the place ("copy arco.exe to ports/arco").
    pub root: bool,
    /// Files the text says to copy; any one of them is enough ("Balatro.love" or "Balatro.exe").
    pub files: Vec<String>,
    /// The port does without (downloads its data, or ships a demo): never blocked.
    pub optional: bool,
}

/// Reads [`Expect`] out of a port's instructions. `dir` is the port's folder in `roms/ports` ("sprout").
pub fn expect(inst: &str, dir: &str) -> Expect {
    let mut e = Expect::default();
    let lower = inst.to_lowercase();
    e.optional = SELF_SUFFICIENT.iter().any(|p| lower.contains(p));
    // What the player must do, without what they may do ("optionally add RCT1 to …", "you can also …").
    let required: Vec<String> = sentences(inst)
        .iter()
        .map(|s| required_part(s).to_string())
        .filter(|s| !s.trim().is_empty())
        .collect();
    for sentence in &required {
        for token in sentence.split(|c: char| c.is_whitespace() || "\"`()[]<>,;*“”".contains(c)) {
            path_target(token, dir, &mut e);
        }
    }
    if e.folders.is_empty() && !e.root {
        phrase_target(&required.join(". "), dir, &mut e);
    }
    // Only optional files named ("Includes the demo files. You can add your own … to ports/x/data"): that
    // is still where they go, but nothing is missing without them.
    if e.folders.is_empty() && !e.root {
        for token in inst.split(|c: char| c.is_whitespace() || "\"`()[]<>,;*“”".contains(c)) {
            path_target(token, dir, &mut e);
        }
        e.optional |= (!e.folders.is_empty() || e.root) && named_files(&required).is_empty();
    }
    // A sub-folder is the more precise place.
    e.root &= e.folders.is_empty();
    e.files = named_files(&required);
    e
}

/// Words after which a sentence turns to what the player may do as well.
const OPTIONAL_FROM: [&str; 5] = ["optional", "you can also", "you can add", "if you want", "if you like"];

/// The part of a sentence before it turns optional.
fn required_part(sentence: &str) -> &str {
    let lower = sentence.to_lowercase();
    let cut = OPTIONAL_FROM.iter().filter_map(|w| lower.find(w)).min().unwrap_or(sentence.len());
    // `to_lowercase` keeps byte offsets for the ASCII words searched; anything else falls back to the whole.
    sentence.get(..cut).unwrap_or(sentence)
}

/// A path such as "ports/sprout/assets", "/roms/ports/counter-strike/valve" or "apondbeyond/assets.".
fn path_target(token: &str, dir: &str, e: &mut Expect) {
    let token = token.trim_end_matches(|c: char| ".:!?".contains(c));
    // Half of a quoted path with spaces ("'ports/duke3dawo/Place Duke3D AWT files here.txt'").
    let quote = |c: char| "'‘’".contains(c);
    if token.starts_with(quote) != token.ends_with(quote) {
        return;
    }
    let token = token.trim_matches(|c: char| quote(c) || ".:!?".contains(c));
    if !token.contains('/') || token.contains("://") || token.starts_with("www.") {
        return;
    }
    let segs: Vec<&str> = token.split('/').filter(|s| !s.is_empty() && *s != ".").collect();
    // Web addresses without a scheme ("itch.io/…") and folders of the game's own release are not the port.
    if segs
        .iter()
        .any(|s| [".io", ".com", ".net", ".org"].iter().any(|d| s.to_lowercase().contains(d)))
    {
        return;
    }
    let dir_l = dir.to_lowercase();
    // The port's folder by another spelling: "theswordwitchsapprentice" for "swordwitchsapprentice",
    // "steamworlddig" for "steamworlddig2".
    let alike = |s: &str| {
        let s = s.to_lowercase();
        s.len() >= 5 && dir_l.len() >= 5 && !s.contains('.') && (s.contains(&dir_l) || dir_l.contains(&s))
    };
    let (at, exact) = match segs.iter().position(|s| s.eq_ignore_ascii_case(dir)) {
        Some(i) => (i, true),
        // "ports/<another name>/assets": a typo of the folder ("alientdeathmob"), or a folder the port is
        // known by elsewhere; the sub-folder still counts, the port's own folder is used.
        None => match segs.iter().position(|s| s.eq_ignore_ascii_case("ports")) {
            Some(i) if i + 1 < segs.len() && !segs[i + 1].eq_ignore_ascii_case("portmaster") => (i + 1, false),
            _ => match segs.iter().position(|s| alike(s)) {
                Some(i) => (i, false),
                None => return,
            },
        },
    };
    // Something odd inside the path: not a place to rely on.
    if segs[at + 1..].iter().any(|s| !is_file_name(s) && !plain_segment(s)) {
        return;
    }
    let after = &segs[at + 1..];
    // A dotted name before the end ("Balatro.app/Contents"): not the port's folders.
    if after.iter().rev().skip(1).any(|s| s.contains('.')) {
        return;
    }
    // "ports/arco/arco.exe", "sdlpal/sdlpal.cfg": a file is not a folder.
    let rest: Vec<&str> = after.iter().copied().filter(|s| !s.contains('.')).collect();
    if rest.is_empty() {
        e.root = true;
        return;
    }
    let folder = rest.join("/");
    if !e.folders.iter().any(|f| f.eq_ignore_ascii_case(&folder)) {
        if e.folders.is_empty() {
            e.sure = exact;
        }
        e.folders.push(folder);
    }
}

/// A port folder's name as the drop folders take it (see `plain_segment`): for the web companion.
pub fn plain_name(s: &str) -> bool {
    plain_segment(s)
}

/// A folder name we are willing to look up or create: letters, digits, `_ - . +` and spaces, not `..`.
fn plain_segment(s: &str) -> bool {
    !s.is_empty() && s != ".." && s.chars().all(|c| c.is_alphanumeric() || "_-.+ ".contains(c))
}

/// "into the assets folder", "this port's gamedata folder", "the 'assets' folder", "the port folder",
/// "the portfolder", "the port (bluerevolver) folder", "the gamefiles directory in the port".
fn phrase_target(inst: &str, dir: &str, e: &mut Expect) {
    let words: Vec<String> = inst
        .split_whitespace()
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric() && c != '_' && c != '-').to_string())
        .filter(|w| !w.is_empty())
        .collect();
    let lower: Vec<String> = words.iter().map(|w| w.to_lowercase()).collect();
    for (k, w) in lower.iter().enumerate() {
        if w == "portfolder" {
            e.root = true;
            continue;
        }
        // Not "folders": "copy the Packs and Assets folders into …" names what to copy, not where.
        if !matches!(w.as_str(), "folder" | "directory" | "dir") || k == 0 {
            continue;
        }
        // Only where the text puts something: "into/to/in … the <x> folder", "place it the <x> folder".
        let near = &lower[k.saturating_sub(5)..k];
        if !near
            .iter()
            .any(|p| matches!(p.as_str(), "into" | "to" | "in" | "inside" | "onto") || PLACE_VERBS.contains(&p.as_str()))
        {
            continue;
        }
        let p = lower[k - 1].trim_end_matches("'s");
        let before = lower.get(k.wrapping_sub(2)).map(|q| q.trim_end_matches("'s"));
        if p == dir.to_lowercase() || matches!(p, "port" | "ports") || (p == "game" && matches!(before, Some("the" | "port"))) {
            e.root = true;
        } else if FOLDER_WORDS.contains(&p) && p != "game" && p != "files" {
            let folder = words[k - 1].trim_end_matches("'s").to_string();
            if !e.folders.iter().any(|f| f.eq_ignore_ascii_case(&folder)) {
                e.folders.push(folder);
            }
        }
    }
    // "copy the antecrypt.app into gamedata": a folder word right after "into", no "folder" after it.
    if e.folders.is_empty() && !e.root {
        for (k, w) in lower.iter().enumerate().skip(1) {
            if lower[k - 1] == "into" && matches!(w.as_str(), "assets" | "gamedata" | "gamefiles") {
                e.folders.push(words[k].clone());
                break;
            }
        }
    }
}

/// A file name with one of [`FILE_EXTS`]: a letter somewhere in the name, no wildcards, no "..".
fn is_file_name(s: &str) -> bool {
    let Some((stem, ext)) = s.rsplit_once('.') else { return false };
    FILE_EXTS.contains(&ext.to_lowercase().as_str())
        && stem.chars().any(|c| c.is_ascii_alphabetic())
        && !s.contains("..")
        && s.chars().all(|c| c.is_ascii_alphanumeric() || "_-.+".contains(c))
}

/// File names the text tells the player to copy: in a sentence with a placing verb, after it, not in an
/// "alternatively" sentence and not in a list of optional files.
fn named_files(sentences: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for sentence in sentences {
        let lower = sentence.to_lowercase();
        if lower.contains("alternativ") || lower.contains("optional") {
            continue;
        }
        let words: Vec<&str> = sentence
            .split(|c: char| c.is_whitespace() || "\"`()[]<>,;*“”‘'".contains(c))
            .filter(|w| !w.is_empty())
            .collect();
        let Some(verb) = words.iter().position(|w| PLACE_VERBS.contains(&w.to_lowercase().trim_end_matches([':', '.']))) else {
            continue;
        };
        for w in &words[verb + 1..] {
            if w.contains("://") {
                continue;
            }
            let name = w.rsplit('/').next().unwrap_or(w).trim_end_matches(|c: char| ".:!?".contains(c));
            if is_file_name(name) && !out.iter().any(|f| f.eq_ignore_ascii_case(name)) {
                out.push(name.to_string());
            }
        }
    }
    out
}

/// Sentences: at line breaks, at ";" and at ". " (a dot inside a file name does not end one).
fn sentences(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        let ends = c == '\n' || c == ';' || (matches!(c, '.' | '!' | '?') && chars.peek().is_none_or(|n| n.is_whitespace()));
        if ends {
            if !cur.trim().is_empty() {
                out.push(std::mem::take(&mut cur));
            }
            cur.clear();
        } else {
            cur.push(c);
        }
    }
    if !cur.trim().is_empty() {
        out.push(cur);
    }
    out
}

/// A folder under `base` named like `rel` ("Assets/Data"), letter case as it is on disk where it exists.
pub fn resolve(base: &Path, rel: &str) -> PathBuf {
    let mut at = base.to_path_buf();
    for seg in rel.split('/').filter(|s| plain_segment(s)) {
        let found = std::fs::read_dir(&at)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .find(|n| n.eq_ignore_ascii_case(seg));
        at = at.join(found.unwrap_or_else(|| seg.to_string()));
    }
    at
}

/// Where the files go: the first folder the text names inside the port's folder, else the port's folder.
pub fn place(port: &Path, e: &Expect) -> PathBuf {
    e.folders.first().map_or_else(|| port.to_path_buf(), |f| resolve(port, f))
}

/// The other folders the text names (Counter-Strike: "valve", then "cstrike").
pub fn also(port: &Path, e: &Expect) -> Vec<PathBuf> {
    e.folders.iter().skip(1).map(|f| resolve(port, f)).collect()
}

/// Files that hold no game data: what ports ship to mark the folder ("place Pengu Never Left.exe here",
/// ".gitkeep", a README) and empty files.
fn placeholder(name: &str, len: u64) -> bool {
    let n = name.to_lowercase();
    len == 0 || n.starts_with('.') || n.starts_with("readme") || n.ends_with(".txt") || n.ends_with(".md") || n.starts_with("place ") || n.contains(" here")
}

/// The folder holds nothing but placeholders (also in its sub-folders). `None` when it is too big to tell.
fn only_placeholders(dir: &Path) -> Option<bool> {
    let mut stack = vec![dir.to_path_buf()];
    let mut seen = 0;
    while let Some(d) = stack.pop() {
        for entry in std::fs::read_dir(&d).ok()?.flatten() {
            seen += 1;
            if seen > WALK_LIMIT {
                return None;
            }
            let Ok(meta) = std::fs::symlink_metadata(entry.path()) else { continue };
            if meta.is_dir() {
                stack.push(entry.path());
            } else if !placeholder(&entry.file_name().to_string_lossy(), meta.len()) {
                return Some(false);
            }
        }
    }
    Some(true)
}

/// Walks the port's folder: whether a file other than a log is newer than `since`, and whether one of `names`
/// is anywhere in it (`names_match`). `None` when the folder is too big to tell.
fn survey(port: &Path, since: Option<SystemTime>, names: &[String]) -> Option<(bool, bool)> {
    let (mut touched, mut found) = (false, false);
    let mut stack = vec![port.to_path_buf()];
    let mut seen = 0;
    while let Some(d) = stack.pop() {
        for entry in std::fs::read_dir(&d).ok()?.flatten() {
            seen += 1;
            if seen > WALK_LIMIT {
                return None;
            }
            let Ok(meta) = std::fs::symlink_metadata(entry.path()) else { continue };
            let name = entry.file_name().to_string_lossy().into_owned();
            if meta.is_dir() {
                stack.push(entry.path());
                continue;
            }
            found |= names_match(names, &name);
            let log = name.eq_ignore_ascii_case("log.txt") || name.to_lowercase().ends_with(".log");
            if let (Some(since), Ok(modified)) = (since, meta.modified()) {
                touched |= !log && modified > since + SETTLE;
            }
        }
    }
    Some((touched, found))
}

/// Before a not-ready-to-run port starts.
#[derive(Clone, Debug, PartialEq)]
pub enum Verdict {
    Launch,
    /// The files are surely not there; the folder they belong in.
    Missing(PathBuf),
}

/// Whether the port in `port` (its folder, e.g. /storage/roms/ports/sprout) may start. `installed`: when its
/// port.json was written (PortMaster writes it last when it installs); `played`: pocket-ui has seen it run
/// for a minute or more. Says `Missing` only when sure:
/// - the port never ran long, nothing in its folder has changed since it was installed, and
/// - the folder the instructions name is missing (named with the port's own folder) or holds only
///   placeholders (and none of the named files is anywhere in the port), or, when the instructions name the
///   port folder itself, none of the files they name is anywhere in it.
///
/// Everything else (no clear folder, files of any kind in it, a port that prepared its files and cleaned up,
/// a folder too big to look through) lets it start.
pub fn check(port: &Path, installed: Option<SystemTime>, e: &Expect, played: bool) -> Verdict {
    if played || e.optional || !port.is_dir() {
        return Verdict::Launch;
    }
    let Some((touched, found)) = survey(port, installed, &e.files) else {
        return Verdict::Launch;
    };
    if touched {
        return Verdict::Launch;
    }
    if let Some(first) = e.folders.first() {
        let dir = resolve(port, first);
        return match std::fs::symlink_metadata(&dir) {
            Ok(m) if m.is_dir() => match only_placeholders(&dir) {
                Some(true) if !found => Verdict::Missing(dir),
                _ => Verdict::Launch,
            },
            Ok(_) => Verdict::Launch,
            Err(_) if e.sure && !found => Verdict::Missing(dir),
            Err(_) => Verdict::Launch,
        };
    }
    if e.root && !e.files.is_empty() && !found {
        return Verdict::Missing(port.to_path_buf());
    }
    Verdict::Launch
}

/// `name` is one of the named files. Names with spaces come out of the text as their last word ("Pengu Never
/// Left.exe" as "Left.exe"), so the end of the name is enough; any letter case.
fn names_match(names: &[String], name: &str) -> bool {
    let name = name.to_lowercase();
    names.iter().any(|n| name.ends_with(&n.to_lowercase()))
}

/// Where the drop folder's contents go, if the port has one: the one sub-folder the instructions name
/// (everything goes there); with several, the port folder, but only folders of those names; or the port folder
/// with only the files the instructions name.
pub fn drop_target(port: &Path, e: &Expect) -> Option<(PathBuf, DropRule)> {
    match e.folders.len() {
        0 => (e.root && !e.files.is_empty()).then(|| (port.to_path_buf(), DropRule::Named(e.files.clone()))),
        1 => Some((place(port, e), DropRule::Everything)),
        _ => {
            let mut tops: Vec<String> = Vec::new();
            for f in &e.folders {
                let top = f.split('/').next().unwrap_or(f).to_string();
                if !tops.iter().any(|t| t.eq_ignore_ascii_case(&top)) {
                    tops.push(top);
                }
            }
            Some((port.to_path_buf(), DropRule::Folders(tops)))
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum DropRule {
    /// Files and folders, as they are.
    Everything,
    /// Only files with these names (`names_match`), from the top of the drop folder.
    Named(Vec<String>),
    /// Only folders with these names (any letter case) from the top of the drop folder, with all in them
    /// (Counter-Strike: "valve" and "cstrike").
    Folders(Vec<String>),
}

/// What [`take_dropped`] did.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Moved {
    pub moved: usize,
    /// Left in the drop folder: already in the port, a link, or not a file the instructions name.
    pub kept: usize,
}

/// Moves what the player put into `drop` into `dest` (the same file system, so a rename): never a link, never
/// over a file that is already there (a placeholder excepted), and under `DropRule::Named` only those files.
/// Folders that are emptied are removed; `drop` itself stays for the next time.
pub fn take_dropped(drop: &Path, dest: &Path, rule: &DropRule) -> std::io::Result<Moved> {
    let mut moved = Moved::default();
    if !drop.is_dir() {
        return Ok(moved);
    }
    std::fs::create_dir_all(dest)?;
    merge(drop, dest, rule, &mut moved, true)?;
    Ok(moved)
}

fn merge(from: &Path, to: &Path, rule: &DropRule, n: &mut Moved, top: bool) -> std::io::Result<()> {
    for entry in std::fs::read_dir(from)?.flatten() {
        let src = entry.path();
        let name = entry.file_name();
        let meta = std::fs::symlink_metadata(&src)?;
        let wanted = match rule {
            DropRule::Named(names) if top => meta.is_file() && names_match(names, &name.to_string_lossy()),
            DropRule::Folders(names) if top => meta.is_dir() && names.iter().any(|f| f.eq_ignore_ascii_case(&name.to_string_lossy())),
            _ => true,
        };
        if !wanted {
            n.kept += 1;
            continue;
        }
        // Into the port's own folder of that name, as it is spelt there ("Assets" for "assets").
        let dst = if top && matches!(rule, DropRule::Folders(_)) {
            resolve(to, &name.to_string_lossy())
        } else {
            to.join(&name)
        };
        if meta.file_type().is_symlink() {
            n.kept += 1;
        } else if meta.is_dir() {
            match std::fs::symlink_metadata(&dst) {
                // A whole new folder goes over in one rename, unless a link hides inside it.
                Err(_) if !has_link(&src) => {
                    std::fs::rename(&src, &dst)?;
                    n.moved += 1;
                }
                Err(_) => {
                    std::fs::create_dir(&dst)?;
                    merge(&src, &dst, rule, n, false)?;
                    let _ = std::fs::remove_dir(&src);
                }
                Ok(m) if m.is_dir() => {
                    merge(&src, &dst, rule, n, false)?;
                    let _ = std::fs::remove_dir(&src);
                }
                Ok(_) => n.kept += 1,
            }
        } else if meta.is_file() {
            let free = match std::fs::symlink_metadata(&dst) {
                Err(_) => true,
                Ok(m) => m.is_file() && placeholder(&name.to_string_lossy(), m.len()),
            };
            if free {
                std::fs::rename(&src, &dst)?;
                n.moved += 1;
            } else {
                n.kept += 1;
            }
        } else {
            n.kept += 1;
        }
    }
    Ok(())
}

fn has_link(dir: &Path) -> bool {
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for entry in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            match std::fs::symlink_metadata(entry.path()) {
                Ok(m) if m.file_type().is_symlink() => return true,
                Ok(m) if m.is_dir() => stack.push(entry.path()),
                Ok(_) => {}
                Err(_) => return true,
            }
        }
    }
    false
}

/// Makes the port's drop folder, writable for the share's user (it would otherwise be root's, and sharing
/// may already be on: the share only takes over folders when it is switched on).
pub fn make_drop(drop_root: &Path, dir: &str) -> Option<PathBuf> {
    if !plain_segment(dir) {
        return None;
    }
    let path = drop_root.join(dir);
    std::fs::create_dir_all(&path).ok()?;
    #[cfg(target_os = "linux")]
    if unsafe { libc::geteuid() } == 0 {
        for p in [drop_root, path.as_path()] {
            let _ = std::os::unix::fs::lchown(p, Some(crate::wifi::SHARE_UID), Some(crate::wifi::SHARE_GID));
        }
    }
    Some(path)
}

/// Whether the page itself switches file sharing (X, in Settings > Extras) or only says where to.
#[derive(Clone, Debug, PartialEq)]
pub enum Toggle {
    Here { busy: bool, dots: String },
    InSettings,
}

/// File sharing as the page shows it.
#[derive(Clone, Debug, Default)]
pub struct Share {
    pub on: bool,
    pub ip: Option<String>,
    pub password: Option<String>,
}

/// "Copy your game files": the page after installing such a port, from the browser (X), and instead of
/// starting one whose files are missing.
#[derive(Clone, Debug)]
pub struct FilesPage {
    pub title: String,
    /// The port's folder name in roms/ports ("sprout").
    pub dir: String,
    /// PortMaster's instructions, as they are.
    pub inst: String,
    pub place: PathBuf,
    pub also: Vec<PathBuf>,
    /// The drop folder's name under port-files, when the port has one; and whether only named files go.
    pub drop: Option<(String, DropRule)>,
    /// Shown instead of a start: the files are missing.
    pub blocked: bool,
    scroll: usize,
    /// Lines of the instructions that fit, and how many there are (from the last paint).
    room: usize,
    lines: usize,
}

impl FilesPage {
    pub fn new(title: &str, dir: &str, inst: &str, ports_dir: &Path) -> FilesPage {
        let e = expect(inst, dir);
        let port = ports_dir.join(dir);
        let drop = drop_target(&port, &e).map(|(_, rule)| (dir.to_string(), rule));
        FilesPage {
            title: title.to_string(),
            dir: dir.to_string(),
            inst: inst.trim().to_string(),
            place: place(&port, &e),
            also: also(&port, &e),
            drop,
            blocked: false,
            scroll: 0,
            room: 0,
            lines: 0,
        }
    }

    pub fn blocked(mut self) -> Self {
        self.blocked = true;
        self
    }

    /// Up/Down and L1/R1 scroll the instructions; true when the page is to close (A or B).
    pub fn press(&mut self, b: Button) -> bool {
        let max = self.lines.saturating_sub(self.room);
        match b {
            Button::Up => self.scroll = self.scroll.saturating_sub(1),
            Button::Down => self.scroll = (self.scroll + 1).min(max),
            Button::L1 => self.scroll = self.scroll.saturating_sub(self.room.max(1)),
            Button::R1 => self.scroll = (self.scroll + self.room.max(1)).min(max),
            Button::East | Button::South => return true,
            _ => {}
        }
        false
    }

    pub fn scroll(&self) -> usize {
        self.scroll
    }

    /// The instructions run past the page: Up/Down are worth a hint.
    pub fn scrolls(&self) -> bool {
        self.lines > self.room
    }

    /// Hints for the page; the caller paints them (the shelf also makes them touchable).
    pub fn hints(&self, share: &Share, toggle: &Toggle) -> Vec<(&'static str, &'static str)> {
        let mut h = vec![("A", "OK")];
        if matches!(toggle, Toggle::Here { busy: false, .. }) && self.drop.is_some() && (share.on || share.ip.is_some()) {
            h.push(("X", if share.on { "Turn off file sharing" } else { "Turn on file sharing" }));
        }
        if self.scrolls() {
            h.push(("Up/Down", "Scroll"));
        }
        h.push(("B", "Back"));
        h
    }

    /// The address of the drop folder on a computer, Windows style and as a URL.
    pub fn addresses(&self, ip: &str) -> Option<(String, String)> {
        let (dir, _) = self.drop.as_ref()?;
        Some((
            format!("\\\\{ip}\\games-roms\\{DROP_SHARE_NAME}\\{dir}"),
            format!("smb://{ip}/games-roms/{DROP_SHARE_NAME}/{dir}"),
        ))
    }

    pub fn paint(&mut self, c: &mut Canvas, text: &mut Text, share: &Share, toggle: &Toggle) {
        c.clear(pal().void);
        let w = c.w - 2 * MARGIN;
        text.draw(c, Face::Title, 67, MARGIN, 124, pal().chalk, "Copy your game files");
        let lead = if self.blocked {
            format!(
                "{} can't start yet: it needs files from your own copy of the game, and they aren't there.",
                self.title
            )
        } else {
            format!("{} needs files from your own copy of the game. pocket-ui doesn't download them.", self.title)
        };
        let mut y = 176;
        for l in text.wrap(Face::Body, 28, &lead, w).iter().take(2) {
            text.draw(c, Face::Body, 28, MARGIN, y, pal().dust, l);
            y += crate::type_scale::line(28, 10);
        }

        y += 22;
        text.draw(c, Face::Body, 24, MARGIN, y, pal().dust, "On this Pocket");
        let place = text.fit(Face::Title, 37, &self.place.to_string_lossy(), w);
        text.draw(c, Face::Title, 37, MARGIN, y + 42, pal().chalk, &place);
        y += 42;
        if let Some(more) = self.also.first() {
            let line = format!("and {}", more.to_string_lossy());
            let line = text.fit(Face::Body, 24, &line, w);
            text.draw(c, Face::Body, 24, MARGIN, y + 34, pal().chalk, &line);
            y += 34;
        }

        y += 56;
        text.draw(c, Face::Body, 24, MARGIN, y, pal().dust, "From a computer");
        // What goes into the drop folder: the files, or the folders the instructions name.
        let what = match &self.drop {
            Some((_, DropRule::Folders(names))) if names.len() == 1 => format!("the {} folder", names[0]),
            Some((_, DropRule::Folders(names))) => format!("the folders {}", and_list(names)),
            _ => "the files".to_string(),
        };
        let named_only = matches!(self.drop, Some((_, DropRule::Named(_))));
        match (&self.drop, share.on, share.ip.as_deref()) {
            (Some(_), true, Some(ip)) => {
                let (win, url) = self.addresses(ip).unwrap_or_default();
                let win = text.fit(Face::Title, 37, &win, w);
                text.draw(c, Face::Title, 37, MARGIN, y + 42, pal().chalk, &win);
                let url = text.fit(Face::Body, 24, &url, w);
                text.draw(c, Face::Body, 24, MARGIN, y + 78, pal().chalk, &url);
                let login = format!(
                    "User {}   Password {}",
                    crate::wifi::SHARE_LOGIN,
                    share.password.as_deref().unwrap_or("(not set yet)")
                );
                text.draw(c, Face::Body, 24, MARGIN, y + 112, pal().chalk, &login);
                y += 34;
                let how = if named_only {
                    "Put the files there. pocket-ui moves the ones PortMaster names into place when you start the game.".to_string()
                } else {
                    format!(
                        "Put {what} there. pocket-ui moves {} into place when you start the game.",
                        if what.starts_with("the folders") || what == "the files" {
                            "them"
                        } else {
                            "it"
                        }
                    )
                };
                let how = text.fit(Face::Body, 24, &how, w);
                text.draw(c, Face::Body, 24, MARGIN, y + 112, pal().dust, &how);
                y += 112;
            }
            (Some((dir, _)), _, _) => {
                let first = match (toggle, share.ip.is_some()) {
                    (Toggle::Here { busy: true, dots }, _) => format!("Switching file sharing{dots}"),
                    (_, false) => "File sharing needs Wi-Fi. Connect first, then turn it on.".to_string(),
                    (Toggle::Here { .. }, true) => "File sharing is off. Press X to turn it on.".to_string(),
                    (Toggle::InSettings, true) => "File sharing is off. Turn it on in Settings › Games › Add games (X).".to_string(),
                };
                let lines = [
                    first,
                    format!("Then put {what} into games-roms\\{DROP_SHARE_NAME}\\{dir}. pocket-ui moves them into place when you start the game."),
                ];
                let wrapped: Vec<String> = lines.iter().flat_map(|l| text.wrap(Face::Body, 24, l, w)).take(3).collect();
                for l in wrapped {
                    text.draw(c, Face::Body, 24, MARGIN, y + 40, pal().chalk, &l);
                    y += 34;
                }
                y += 6;
            }
            (None, _, _) => {
                let l = "File sharing can't reach this folder. Copy the files with Remote access (SSH), or with the SD card in a computer that reads Linux (ext4) drives.";
                for l in text.wrap(Face::Body, 24, l, w).iter().take(2) {
                    text.draw(c, Face::Body, 24, MARGIN, y + 40, pal().chalk, l);
                    y += crate::type_scale::line(24, 10);
                }
                y += 6;
            }
        }

        // PortMaster's own words, as they are, in what is left of the page.
        y += 56;
        text.draw(c, Face::Body, 24, MARGIN, y, pal().dust, "PortMaster says");
        let lines = inst_lines(text, &self.inst, w - 24);
        let (top, bottom, pitch) = (y + 40, c.h - 110, 32);
        self.room = ((bottom - top) / pitch).max(1) as usize;
        self.lines = lines.len();
        self.scroll = self.scroll.min(self.lines.saturating_sub(self.room));
        for (i, l) in lines.iter().skip(self.scroll).take(self.room).enumerate() {
            text.draw(c, Face::Body, 24, MARGIN, top + i as i32 * pitch, pal().chalk, l);
        }
        if self.lines > self.room {
            // A scroll bar along the right edge, as on the game card.
            let track = self.room as i32 * pitch;
            let thumb = (track * self.room as i32 / self.lines as i32).max(24);
            let at = (track - thumb) * self.scroll as i32 / (self.lines - self.room) as i32;
            c.rect(c.w - MARGIN - 4, top - 24, 4, track, pal().pill);
            c.rect(c.w - MARGIN - 4, top - 24 + at, 4, thumb, pal().chip);
        }
    }
}

/// "valve and cstrike", "a, b and c".
fn and_list(names: &[String]) -> String {
    match names {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// Instructions broken into lines no wider than `w`: their own line breaks kept, blank lines at most one in a
/// row, and a word too long for a line (a web address) broken where the line ends, so nothing is cut off.
pub fn inst_lines(text: &mut Text, inst: &str, w: i32) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for para in inst.lines() {
        let para = para.trim();
        if para.is_empty() {
            if out.last().is_some_and(|l| !l.is_empty()) {
                out.push(String::new());
            }
            continue;
        }
        for line in text.wrap(Face::Body, 24, para, w) {
            let mut rest = line.as_str();
            while text.width(Face::Body, 24, rest) > w {
                // The longest start of `rest` that fits, at least one character.
                let mut cut = rest.char_indices().nth(1).map_or(rest.len(), |(i, _)| i);
                for (i, _) in rest.char_indices().skip(1) {
                    if text.width(Face::Body, 24, &rest[..i]) > w {
                        break;
                    }
                    cut = i;
                }
                out.push(rest[..cut].to_string());
                rest = &rest[cut..];
            }
            out.push(rest.to_string());
        }
    }
    while out.last().is_some_and(String::is_empty) {
        out.pop();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;

    fn tmp(name: &str) -> crate::testutil::TestDir {
        TestDir::new(&format!("portfiles-{name}"))
    }

    /// (port folder, PortMaster's `inst` as in the catalog, folders, root, files).
    type Case = (&'static str, &'static str, &'static [&'static str], bool, &'static [&'static str]);

    /// Real instructions from PortMaster-New's ports.json (October 2026), typos and all.
    const REAL: &[Case] = &[
        ("sprout", "Purchase the game from Steam. Download the game and place the game files in sprout/assets.", &["assets"], false, &[]),
        ("apondbeyond", "Download A Pond Beyond.zip (August 16, 2024) from https://npjarcade.itch.io/a-pond-beyond. Copy the data.win from the zip over to apondbeyond/assets.", &["assets"], false, &["data.win"]),
        ("aestheta", "Install port. \nPurchase the game on Steam (https://store.steampowered.com/app/3730600/Aestheta/) or Itch (https://aplovestudio.itch.io/aestheta). \nCopy all game files into the ports/aestheta/assets folder.", &["assets"], false, &[]),
        ("air", "Add your game assets to `ports/air/gamedata`.", &["gamedata"], false, &[]),
        ("aliendeathmob", "Purchase the game from Steam.\nDownload the latest version of the game and place the game files in ports/alientdeathmob/assets", &["assets"], false, &[]),
        ("adventurerush", "Purchase the game on Steam or download the free demo from Itch.io and copy all game files into the assets folder.", &["assets"], false, &[]),
        ("alllcall", "Download free at itch.io https://nizakashii.itch.io/alllcall and copy the data.win file into the assets folder.", &["assets"], false, &["data.win"]),
        ("absolutereflex", "Download the game from https://jukio.itch.io/absolute-reflex and copy the exe file into the port folder.", &[], true, &[]),
        ("ambidextro", "Copy the file Ambidextro.pck into the folder:  /roms/ports/ambidextro/\n", &[], true, &["Ambidextro.pck"]),
        ("annulus", "Download Annulus 2.31.exe from here - https://toq-games.itch.io/annulus - rename exe to zip and extract contents to ports/annulus/assets folder. Launch the game and Enjoy!", &["assets"], false, &[]),
        ("antecrypt", "Purchase the game on Steam or Itch.io. For Itch.io, download the MacOS version and copy the antecrypt.app into gamedata. For Steam, download the MacOS version using the following Steam console command: download_depot 1659920 1659922 959787945917647264, and then copy the files into gamedata.", &["gamedata"], false, &[]),
        ("arco", "Copy arco.exe from your Steam download to ports/arco. A D-pad/L-stick toggle patch is available during installation.", &[], true, &["arco.exe"]),
        ("arx", "Purchase the game and copy the GoG offline installer (2 files, setup_arx...exe and setup_arx...bin) to arx/gamedata. They will be extracted on first run. Alternatively, copy Steam or Windows CD-ROM files so that copy them so that data.pak and surrounding files and folders are in arx/gamedata.", &["gamedata"], false, &[]),
        ("aquaria", "Install the port. Buy the game on Steam or GoG and copy the game files (data/ and surrounding files) into your ports/aquaria folder.", &[], true, &[]),
        ("augustus", "You will need the original files of the PC release copied to the ports/augustus/data folder.", &["data"], false, &[]),
        ("balatro", "Purchase the game from Steam: https://store.steampowered.com/app/2379780/Balatro/. On macOS, copy Balatro.love from Balatro.app/Contents/Resources into the balatro folder. On Windows, copy Balatro.exe into the balatro folder.", &[], true, &["Balatro.love", "Balatro.exe"]),
        ("beatblock", "Download the Linux version of the game from https://bubbletabby.itch.io/beatblock and place Beatblock-linux.zip into the beatblock/gamedata/ directory on your device", &["gamedata"], false, &["Beatblock-linux.zip"]),
        ("blacksouls", "Copy Audio/, Graphics/, Fonts/, Game.ini and Game.rgss3a from your own installation directly into the blacksouls/ folder, alongside the engine. The Steam release ships a heavily reduced build; apply the publisher's free official patch to it first, or the port will run that reduced build.", &[], true, &["Game.rgss3a"]),
        ("blowfly2", "Download the game from Steam https://store.steampowered.com/app/2831590/BLOWFLY2OVERLIMIT/\nand copy all game files to the 'assets' folder. Patching will take place on first run.", &["assets"], false, &[]),
        ("bluerevolver", "Download the game from https://store.steampowered.com/app/439490/BLUE_REVOLVER/ and copy the .exe or .love file into the port (bluerevolver) folder. Switch to the [1.12 - Old pre-DA version] in steam betas, as that is the only version supported for now.", &[], true, &[]),
        ("boxspace", "Purchase the game on Steam https://store.steampowered.com/app/1613150/BOX_Space_Station/ and copy data.win, audiogroup1.dat and _223235__dambient__8_bit_loop.ogg into the port folder.", &[], true, &["data.win", "audiogroup1.dat"]),
        ("bradhasapain", "Download the game on Itch 'https://scary-lemon.itch.io/brad-has-a-pain' download BraHasPain.rar and extract contents of BradHasPain folder to the portfolder", &[], true, &[]),
        ("brokenpipes", "Download the game from https://cocawsugar-gj2020.itch.io/broken-pipes and extract the data.win file into the game folder. ", &[], true, &["data.win"]),
        ("bstone-ps", "You'll need to add your own full version Blake Stone Planet Strike (*.VSI) files to the 'ports/bstone-ps/gamedata/planet_strike' folder.  The Steam version of these files can be found by browsing local files and entering the 'Blake Stone - Planet Strike' directory.", &["gamedata/planet_strike"], false, &[]),
        ("butterflysoup", "Get the game from itch.io here; https://brianna-lei.itch.io/butterfly-soup (Please consider buying!). Put the contents of the `game` directory into the gamefiles directory in the port.", &["gamefiles"], false, &[]),
        ("canabalt-hf", "If your device has an internet connection, assets will be downloaded automatically. If not, download https://github.com/ninjamuffin99/canabalt-assets/archive/refs/heads/main.zip and put it in ports/canabalt/gamedata/.", &["gamedata"], false, &[]),
        ("carriesorderup", "Purchase the game from: [Steam](https://store.steampowered.com/app/522490/Carries_Order_Up/) OR [Itch.io](https://inkdragonworks.itch.io/carries-order-up) \n\nFrom Itch:\nDownload CarriesOrderUp.zip from and unzip the contents to /ports/cariesorderup/assets\n\nFor Steam:\nDownload the game and copy the contents from /Carrie's Order Up! to /ports/cariesorderup/assets", &["assets"], false, &[]),
        ("catacomb", "Copy your *.CA2 files into ports/catacomb/ and filenames should be uppercase.  If game files are lowercase, the launcher will attempt to convert them to uppercase automatically.  Both the floppy disk and GOG versions are compatible.", &[], true, &[]),
        ("celeste", "- Install the port via PortMaster.\n- Download the DRM-Free version of the game:\n  - https://maddymakesgamesinc.itch.io/celeste (linux)\n  - https://store.epicgames.com/en-US/p/celeste (windows)\n- Copy the game's data into \"celeste/gamedata\".\n- Run the port, the first time setup will take a few minutes.\n- Enjoy!", &["gamedata"], false, &[]),
        ("counter-strike", "1. Buy the game on Steam https://store.steampowered.com/app/10/CounterStrike.\n\n2. Copy your valve folder into: /roms/ports/counter-strike/valve\n\n3. Copy your cstrike folder into: /roms/ports/counter-strike/cstrike\n\n4. Do not overwrite existing port files when copying game data. If prompted, choose Skip / Keep existing files.\n\n5. Launch Counter-Strike from PortMaster.", &["valve", "cstrike"], false, &[]),
        ("crashlands", "To download the game, open the Steam console and paste the folowing text to download a compatible version: *download_depot 391730 391732 5160663558593528303*\n\nPlace all game files from the Resouces folder into port's Assets folder. ", &["Assets"], false, &[]),
        ("crimsonland", "Place crimson.paq and sfx.paq in crimsonland/assets; music.paq or loose music/*.ogg files are optional. \n\nControls\nLeft stick: movement\nRight stick: aim/fire", &["assets"], false, &["crimson.paq", "sfx.paq"]),
        ("cavebuster", "1.Download 1 of the zip files from https://jontopielski.itch.io/cave-buster\n2. Copy the \"**.pck\" file from your downloaded zip into the gamedata folder of the cavebuster folder in your ports. Then rename it to \"cavebuster.pck\".", &["gamedata"], false, &["cavebuster.pck"]),
        ("deadcells", "See the README.md for installation instructions", &[], false, &[]),
        ("demonizer", "You need to copy the game file (demonizer-demo.love or demonizer-full.love or demonizer demo.exe or ...) in gamedata directory and named it demonizer.love.", &["gamedata"], false, &["demonizer-demo.love", "demonizer-full.love", "demo.exe", "demonizer.love"]),
        ("corsairs", "Place the original Corsairs JAR as corsairs.jar into ports/corsairs/. The game JAR can be found in J2ME game archives.", &[], true, &["corsairs.jar"]),
        ("blood", "You'll need to add your own full version Blood 1.21 files to the ports/Blood folder.", &[], true, &[]),
        ("annalynn", "Purchase the game from steam: https://store.steampowered.com/app/1508460/Annalynn/\nRun steam console via Windows+R: Type: steam://open/console, download the 1.3 version by typing: download_depot 1508460 1508461 5365853075598347936 in the console. When the download is complete, it will tell you the location of the folder, copy all files to /ports/annalynn/assets", &["assets"], false, &[]),
        ("circlebreaker", "Purchase the game on Itch.io and copy the place Circle Breaker Madness 1.0.1.exe here.txt into the assets folder.", &["assets"], false, &[]),
        ("prehistorik2", "Includes the demo files. You can add your own full game files to ports/prehistorik2/data.", &["data"], false, &[]),
        ("bulletbeat", "Purchase the game from Itch.io. Download Bullet Beat.exe and place it in ports/bulletbeat/assets.\nYou can add your own music files in the .ogg format to ports/bulletbeat/saves.", &["assets"], false, &[]),
        ("duke3dawo", "Obtain game files from your Steam copy of Alien World Tour.  See 'ports/duke3dawo/Place Duke3D AWT files here.txt'  for instructions to place your Alien World Tour contents into the duke3dawo directory.", &[], true, &[]),
        ("hexen2", "You must copy your Hexen 2 pak0.pak and pak1.pak files into hexen2/data1 and your pak3.pak into hexen2/portals and make sure they're lowercase.  Thanks to sezero (Ozkan Sezer) for the HoT source port", &["data1", "portals"], false, &["pak0.pak", "pak1.pak", "pak3.pak"]),
        ("lisathehopeful", "download the game from itchio, https://taco-salad.itch.io/lisa-the-hopeful tested with Windows version. Copy all assetsto the gamedata folder.\n\nNote: This game is not ideal for devices with low resolution displays and/or absent joysticks. ", &["gamedata"], false, &[]),
        ("moonstriker", "Download the MoonStriker.exe from Itch.io and place it the assets folder.", &["assets"], false, &[]),
        ("openhexagon", "Download the game on Steam (Paid) or Itch.io (Paid), or the game files on Github (Free) and copy the \"Packs\" and \"Assets\" folders into the \"openhexagon\" directory.", &[], true, &[]),
        ("openrct2", "Add your Roller Coaster Tycoon 2 game files to the ports/openrct2/RCT2 directory, and optionally add your Roller Coaster Tycoon 1 files to ports/openrct2/RCT1.", &["RCT2"], false, &[]),
        ("penguneverleft", "Purchase the game from itch.io (https://schwegg.itch.io/pengu-never-left). Download the Windows version and copy 'Pengu Never Left.exe' to the penguneverleft/gamedata folder.", &["gamedata"], false, &["Left.exe"]),
        ("steamworlddig2", "Westonpack runtime is required to run this port, install it via Portmaster before running. \nPlace the Linux Version of the GOG installer within /steamworlddig/installer/, then just run the game. This will take longer on first startup.", &["installer"], false, &[]),
        ("sdlpal", "Purchase the game from Steam: https://store.steampowered.com/app/1546570/Sword_and_Fairy/ and put all files into the sdlpal/gamedata folder. Add the english translation into the sdlpal/gamedata from: https://www.romhacking.net/translations/2441/. If you want to play in other language, modify the variable 'MessageFileName' at the file sdlpal/sdlpal.cfg with the one you want or empty for original language.", &["gamedata"], false, &[]),
        ("swordwitchsapprentice", "Download the game free at itch.io (https://tinycactus.itch.io/swordwitchsapprentice), extract, and place the game files in theswordwitchsapprentice/gamedata.", &["gamedata"], false, &[]),
        ("vnds-love", "Check VNDS Wiki \"https://github.com/BASLQC/vnds/wiki\" to find VNDS ready novels. Place your novel folder into ./vnds-love/conf/love/VNDS-LOVE/novels/  \nMore info in README.", &["conf/love/VNDS-LOVE/novels"], false, &[]),
    ];

    #[test]
    fn reads_folders_and_files_from_real_instructions() {
        for (dir, inst, folders, root, files) in REAL {
            let e = expect(inst, dir);
            assert_eq!(e.folders, folders.iter().map(|s| s.to_string()).collect::<Vec<_>>(), "folders of {dir}: {inst}");
            assert_eq!(e.root, *root, "root of {dir}: {inst}");
            assert_eq!(e.files, files.iter().map(|s| s.to_string()).collect::<Vec<_>>(), "files of {dir}: {inst}");
        }
    }

    #[test]
    fn knows_when_the_folder_is_named_with_the_port() {
        assert!(expect("place the game files in sprout/assets.", "sprout").sure);
        assert!(!expect("copy all game files into the assets folder.", "sprout").sure, "a phrase is not a path");
        assert!(
            !expect("place the game files in ports/alientdeathmob/assets", "aliendeathmob").sure,
            "another name for the folder"
        );
    }

    #[test]
    fn ports_that_fetch_or_ship_their_data_are_optional() {
        assert!(expect(REAL.iter().find(|r| r.0 == "canabalt-hf").unwrap().1, "canabalt-hf").optional);
        assert!(
            expect(
                "Includes the demo files. You can add your own full game files to ports/prehistorik2/data.",
                "prehistorik2"
            )
            .optional
        );
        assert!(!expect("Copy the data.win into the assets folder.", "x").optional);
    }

    #[test]
    fn web_addresses_and_wildcards_are_not_places_or_files() {
        let e = expect(
            "Get it at https://example.itch.io/game/download or itch.io/game/x and copy *.CA2 and music/*.ogg into ports/game/",
            "game",
        );
        assert!(e.folders.is_empty() && e.root, "{e:?}");
        assert!(e.files.is_empty(), "{e:?}");
        let e = expect("Download Annulus 2.31.exe and copy 1.0.1.exe into the port folder.", "annulus");
        assert!(e.files.is_empty(), "a version number is not a name, a download is not a copy: {e:?}");
    }

    /// A port folder as PortMaster leaves it: its files, port.json written last.
    fn installed_port(root: &Path, dir: &str, files: &[&str]) -> (PathBuf, SystemTime) {
        let port = root.join(dir);
        for f in files {
            let p = port.join(f);
            if f.ends_with('/') {
                std::fs::create_dir_all(&p).unwrap();
            } else {
                std::fs::create_dir_all(p.parent().unwrap()).unwrap();
                std::fs::write(&p, if f.contains("place ") || f.ends_with(".gitkeep") { "" } else { "x" }).unwrap();
            }
        }
        std::fs::write(port.join("port.json"), "{}").unwrap();
        let at = std::fs::metadata(port.join("port.json")).unwrap().modified().unwrap();
        (port, at)
    }

    fn set_age(path: &Path, when: SystemTime) {
        let f = std::fs::File::options().write(true).open(path).unwrap();
        f.set_modified(when).unwrap();
    }

    #[test]
    fn blocks_an_untouched_port_whose_folder_holds_only_placeholders() {
        let root = tmp("guard-empty");
        let (port, at) = installed_port(&root, "sprout", &["assets/.gitkeep", "game.port", "gmloader.json"]);
        let e = expect(
            "Purchase the game from Steam. Download the game and place the game files in sprout/assets.",
            "sprout",
        );
        assert_eq!(check(&port, Some(at), &e, false), Verdict::Missing(port.join("assets")));
        assert_eq!(check(&port, Some(at), &e, true), Verdict::Launch, "it ran for a minute before: trust it");

        std::fs::write(port.join("assets/data.win"), "x").unwrap();
        assert_eq!(check(&port, Some(at), &e, false), Verdict::Launch, "files are there");
    }

    #[test]
    fn a_port_that_prepared_its_files_and_cleaned_up_starts() {
        // Pengu Never Left: the first start packs gamedata/*.exe into a .love, empties gamedata and puts the
        // placeholder back. The folder looks like the day it was installed, but the port has run.
        let root = tmp("guard-cleaned");
        let (port, at) = installed_port(&root, "penguneverleft", &["gamedata/place Pengu Never Left.exe here", "main.lua"]);
        let e = expect("Download the game and copy the .exe into the gamedata folder.", "penguneverleft");
        assert!(matches!(check(&port, Some(at), &e, false), Verdict::Missing(_)));
        std::fs::write(port.join("PenguNeverLeft.love"), "x").unwrap();
        set_age(&port.join("PenguNeverLeft.love"), at + Duration::from_secs(3600));
        assert_eq!(check(&port, Some(at), &e, false), Verdict::Launch);
        // A log written by a start that failed is not work done.
        std::fs::remove_file(port.join("PenguNeverLeft.love")).unwrap();
        std::fs::write(port.join("log.txt"), "x").unwrap();
        set_age(&port.join("log.txt"), at + Duration::from_secs(3600));
        assert!(matches!(check(&port, Some(at), &e, false), Verdict::Missing(_)));
    }

    #[test]
    fn a_missing_folder_blocks_only_when_the_text_names_it_with_the_port() {
        let root = tmp("guard-absent");
        let (port, at) = installed_port(&root, "augustus", &["augustus.gptk"]);
        let sure = expect(
            "You will need the original files of the PC release copied to the ports/augustus/data folder.",
            "augustus",
        );
        assert_eq!(check(&port, Some(at), &sure, false), Verdict::Missing(port.join("data")));
        let phrase = expect("Copy the files into the data folder.", "augustus");
        assert_eq!(
            check(&port, Some(at), &phrase, false),
            Verdict::Launch,
            "maybe the launcher makes it, maybe it is somewhere else"
        );
    }

    #[test]
    fn named_files_count_wherever_they_are_and_in_any_case() {
        let root = tmp("guard-named");
        let (port, at) = installed_port(&root, "arco", &["arco.gptk", "libs/"]);
        let e = expect("Copy arco.exe from your Steam download to ports/arco.", "arco");
        assert_eq!(check(&port, Some(at), &e, false), Verdict::Missing(port.clone()));
        std::fs::write(port.join("libs/ARCO.EXE"), "x").unwrap();
        assert_eq!(check(&port, Some(at), &e, false), Verdict::Launch);

        let (port, at) = installed_port(&root, "balatro", &["balatro.gptk"]);
        let e = expect(REAL.iter().find(|r| r.0 == "balatro").unwrap().1, "balatro");
        assert!(matches!(check(&port, Some(at), &e, false), Verdict::Missing(_)));
        std::fs::write(port.join("Balatro.exe"), "x").unwrap();
        assert_eq!(check(&port, Some(at), &e, false), Verdict::Launch, "either of the two is enough");

        // "place The Sun and Moon.exe into ports/thesunandmoon/" reads as "Moon.exe": the end of the name counts.
        let (port, at) = installed_port(&root, "thesunandmoon", &["sunandmoon.gptk"]);
        let e = expect(
            "Download and install the game and place The Sun and Moon.exe into ports/thesunandmoon/",
            "thesunandmoon",
        );
        assert_eq!(e.files, ["Moon.exe"]);
        assert!(matches!(check(&port, Some(at), &e, false), Verdict::Missing(_)));
        std::fs::write(port.join("The Sun and Moon.exe"), "x").unwrap();
        assert_eq!(check(&port, Some(at), &e, false), Verdict::Launch);

        // A file named only as where to get the data from, with no place for it: unsure, start.
        let (port, at) = installed_port(&root, "ctp2", &["ctp2.gptk"]);
        let e = expect("You need to copy the \"ctp2_data\", \"ctp2_program\", and \"Scenarios\" folders from either your original Call to Power 2 installation directory or extract them from the GoG version's setup file (setup_call_to_power2_2.0.0.13.exe) using innoextract.", "ctp2");
        assert!(!e.root && e.folders.is_empty() && !e.files.is_empty(), "{e:?}");
        assert_eq!(check(&port, Some(at), &e, false), Verdict::Launch);
    }

    #[test]
    fn unsure_means_start() {
        let root = tmp("guard-unsure");
        let (port, at) = installed_port(&root, "aquaria", &["aquaria.gptk"]);
        let e = expect(
            "Buy the game on Steam or GoG and copy the game files (data/ and surrounding files) into your ports/aquaria folder.",
            "aquaria",
        );
        assert_eq!(check(&port, Some(at), &e, false), Verdict::Launch, "the port folder, no file named");
        let e = expect("See the README.md for installation instructions", "aquaria");
        assert_eq!(check(&port, Some(at), &e, false), Verdict::Launch, "nothing said");
        let e = expect(
            "If your device has an internet connection, assets will be downloaded automatically. Or put them in ports/aquaria/gamedata/.",
            "aquaria",
        );
        assert_eq!(check(&port, Some(at), &e, false), Verdict::Launch, "it fetches them itself");
        assert_eq!(
            check(&root.join("gone"), Some(at), &expect("place files in gone/assets", "gone"), false),
            Verdict::Launch,
            "no folder at all"
        );
    }

    #[test]
    fn dropped_files_go_into_the_named_folder_without_replacing_anything() {
        let root = tmp("drop");
        let (port, _) = installed_port(&root, "sprout", &["assets/.gitkeep", "assets/options.ini"]);
        let drop = root.join("port-files/sprout");
        std::fs::create_dir_all(drop.join("sounds")).unwrap();
        std::fs::write(drop.join("data.win"), "game").unwrap();
        std::fs::write(drop.join("sounds/a.ogg"), "a").unwrap();
        std::fs::write(drop.join("options.ini"), "theirs").unwrap();
        std::fs::write(drop.join(".gitkeep"), "x").unwrap();
        std::os::unix::fs::symlink("/etc", drop.join("etc")).unwrap();
        let e = expect("place the game files in sprout/assets.", "sprout");
        let (dest, rule) = drop_target(&port, &e).unwrap();
        assert_eq!((dest.clone(), rule.clone()), (port.join("assets"), DropRule::Everything));
        let n = take_dropped(&drop, &dest, &rule).unwrap();
        assert_eq!(
            n,
            Moved { moved: 3, kept: 2 },
            "data.win, sounds/ (in one go), the placeholder replaced; options.ini and the link stay"
        );
        assert_eq!(std::fs::read_to_string(port.join("assets/data.win")).unwrap(), "game");
        assert_eq!(std::fs::read_to_string(port.join("assets/sounds/a.ogg")).unwrap(), "a");
        assert_eq!(
            std::fs::read_to_string(port.join("assets/options.ini")).unwrap(),
            "x",
            "the port's own file stays"
        );
        assert!(!port.join("assets/etc").exists());
        assert!(drop.is_dir(), "the drop folder stays for next time");
    }

    #[test]
    fn into_the_port_folder_only_the_named_files_go() {
        let root = tmp("drop-named");
        let (port, _) = installed_port(&root, "arco", &["gmloader.json", "lib/libc++_shared.so"]);
        let drop = root.join("port-files/arco");
        std::fs::create_dir_all(drop.join("lib")).unwrap();
        std::fs::write(drop.join("Arco.exe"), "game").unwrap();
        std::fs::write(drop.join("gmloader.json"), "evil").unwrap();
        std::fs::write(drop.join("lib/libSDL2-2.0.so.0"), "evil").unwrap();
        let e = expect("Copy arco.exe from your Steam download to ports/arco.", "arco");
        let (dest, rule) = drop_target(&port, &e).unwrap();
        assert_eq!(dest, port);
        assert_eq!(take_dropped(&drop, &dest, &rule).unwrap(), Moved { moved: 1, kept: 2 });
        assert!(port.join("Arco.exe").exists());
        assert_eq!(std::fs::read_to_string(port.join("gmloader.json")).unwrap(), "x");
        assert!(
            !port.join("lib/libSDL2-2.0.so.0").exists(),
            "nothing else reaches the folder the port loads libraries from"
        );
        // Nothing to say where: no drop folder at all.
        assert_eq!(drop_target(&port, &expect("copy all game files into the port folder.", "arco")), None);
    }

    #[test]
    fn with_several_folders_only_those_folders_go_in() {
        let root = tmp("drop-folders");
        let (port, _) = installed_port(&root, "counter-strike", &["valve/liblist.gam", "xash3d"]);
        let e = expect(REAL.iter().find(|r| r.0 == "counter-strike").unwrap().1, "counter-strike");
        let (dest, rule) = drop_target(&port, &e).unwrap();
        assert_eq!(
            (dest.as_path(), &rule),
            (port.as_path(), &DropRule::Folders(vec!["valve".into(), "cstrike".into()]))
        );
        let drop = root.join("port-files/counter-strike");
        for f in ["VALVE/pak0.pak", "valve/liblist.gam", "cstrike/maps/de_dust.bsp"] {
            std::fs::create_dir_all(drop.join(f).parent().unwrap()).unwrap();
            std::fs::write(drop.join(f), "theirs").unwrap();
        }
        std::fs::write(drop.join("xash3d"), "evil").unwrap();
        let n = take_dropped(&drop, &dest, &rule).unwrap();
        assert_eq!(
            n,
            Moved { moved: 2, kept: 2 },
            "pak0.pak and cstrike/; the port's liblist.gam and the stray file stay"
        );
        assert_eq!(std::fs::read_to_string(port.join("valve/pak0.pak")).unwrap(), "theirs", "VALVE went into valve");
        assert_eq!(std::fs::read_to_string(port.join("valve/liblist.gam")).unwrap(), "x");
        assert!(port.join("cstrike/maps/de_dust.bsp").exists());
        assert_eq!(std::fs::read_to_string(port.join("xash3d")).unwrap(), "x");
    }

    #[test]
    fn a_dropped_folder_with_a_link_inside_is_taken_apart() {
        let root = tmp("drop-link");
        let dest = root.join("assets");
        let drop = root.join("drop");
        std::fs::create_dir_all(drop.join("music")).unwrap();
        std::fs::write(drop.join("music/a.ogg"), "a").unwrap();
        std::os::unix::fs::symlink("/etc/passwd", drop.join("music/b.ogg")).unwrap();
        let n = take_dropped(&drop, &dest, &DropRule::Everything).unwrap();
        assert_eq!(n, Moved { moved: 1, kept: 1 });
        assert!(dest.join("music/a.ogg").exists());
        assert!(std::fs::symlink_metadata(dest.join("music/b.ogg")).is_err());
    }

    #[test]
    fn page_addresses_name_the_drop_folder() {
        let p = FilesPage::new("-SPROUT-", "sprout", "place the game files in sprout/assets.", Path::new("/storage/roms/ports"));
        assert_eq!(p.place, PathBuf::from("/storage/roms/ports/sprout/assets"));
        assert_eq!(
            p.addresses("192.0.2.5"),
            Some((
                "\\\\192.0.2.5\\games-roms\\port-files\\sprout".to_string(),
                "smb://192.0.2.5/games-roms/port-files/sprout".to_string()
            ))
        );
        let p = FilesPage::new(
            "Aquaria",
            "aquaria",
            "copy the game files into your ports/aquaria folder.",
            Path::new("/storage/roms/ports"),
        );
        assert_eq!(p.place, PathBuf::from("/storage/roms/ports/aquaria"));
        assert_eq!(p.addresses("192.0.2.5"), None, "sharing can't reach it");
        let mut p = p;
        assert!(!p.press(Button::Down));
        assert!(p.press(Button::South));
    }
}
