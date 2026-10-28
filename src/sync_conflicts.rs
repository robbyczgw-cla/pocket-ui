//! Save conflicts from Syncthing, made visible and recoverable (docs/user/sync.md "Two versions of a save").
//!
//! When a save changed on two devices before they synced, Syncthing keeps both: the file stays with one version and
//! the other goes beside it as `<name>.sync-conflict-<date>-<time>-<device>.<ext>`. Nobody would ever see that copy.
//!
//! * `find` walks the shared save folders (`roots`, a few levels deep, names only) for those copies and gathers them
//!   by the file they belong to (`Group`). It runs on a thread (`app_sync_conflicts.rs`), never per frame.
//! * `owner` says whose a group is: one game (its battery save or a state with the game's name), every game of a
//!   system (a memory card shared by them, `Kind::Card`), or nobody the library knows (Wii, 3DS, GameCube `.gci`).
//! * `keep` keeps the version the player picked in the file's place and moves the others into a dated folder
//!   under `/storage/roms/sync-backups`, which Syncthing never shares (`sync::SAVES` keeps it out). Nothing is
//!   deleted; run again it does nothing more.

use std::path::{Path, PathBuf};

use crate::sync::{Prefs, Share};

/// What Syncthing puts in the name of the copy it keeps.
const MARK: &str = ".sync-conflict-";
/// Where the versions not kept go: `<BACKUP_DIR>/<YYYY-MM-DD>/<the file's path below /storage/roms>`.
pub const BACKUP_DIR: &str = "/storage/roms/sync-backups";
/// At most this many directory entries are looked at per folder, so an odd folder never makes the walk long.
const BUDGET: usize = 40_000;
/// Battery save extensions of the Saves folder (`sync::SAVES`), but the memory cards.
const SAVE_EXTS: [&str; 6] = ["srm", "sav", "eep", "fla", "sra", "dsv"];
/// Memory cards: one file holding the saves of many games.
const CARD_EXTS: [&str; 4] = ["ps2", "raw", "mcd", "mcr"];

/// Where things are: the device's folders, or a test's.
#[derive(Clone, Debug)]
pub struct Layout {
    pub roms: PathBuf,
    pub states: PathBuf,
    /// `/storage/.config`: Dolphin's Wii saves are below it.
    pub config: PathBuf,
    pub backup: PathBuf,
}

impl Layout {
    pub fn device() -> Layout {
        Layout {
            roms: PathBuf::from(crate::sync::ROMS_DIR),
            states: PathBuf::from(crate::slots::STATES_DIR),
            config: PathBuf::from("/storage/.config"),
            backup: PathBuf::from(BACKUP_DIR),
        }
    }

    /// A layout under `root` with the device's names (tests, snapshots).
    pub fn under(root: &Path) -> Layout {
        let roms = root.join("roms");
        Layout {
            states: roms.join("savestates"),
            backup: roms.join("sync-backups"),
            config: root.join(".config"),
            roms,
        }
    }
}

/// What kind of file has two versions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A game's battery save (`.srm` and friends) next to the game.
    Save,
    /// A save state (or the state Quit writes).
    State,
    /// A memory card: many games' saves in one file. Keeping a version keeps the whole card.
    Card,
    /// Save data the library can't tie to a game (Wii, 3DS, a GameCube `.gci`).
    Other,
}

/// One version of the file: the one in its place, or a copy Syncthing kept.
#[derive(Clone, Debug, PartialEq)]
pub struct Version {
    pub path: PathBuf,
    /// Last changed, Unix seconds (Syncthing keeps the time of the version it moved aside).
    pub mtime: u64,
    pub size: u64,
}

/// A file with more than one version.
#[derive(Clone, Debug, PartialEq)]
pub struct Group {
    /// The file the versions are of (its place; it may be missing).
    pub original: PathBuf,
    pub kind: Kind,
    /// The version in the file's place now, if there is one.
    pub current: Option<Version>,
    /// The copies Syncthing kept, by name (oldest conflict first).
    pub others: Vec<Version>,
    /// A state's picture had a conflict too: its copies (of `<original>.png`), moved along with the state.
    pub previews: Vec<PathBuf>,
}

impl Group {
    /// Every version, the one in use first.
    pub fn versions(&self) -> Vec<&Version> {
        self.current.iter().chain(self.others.iter()).collect()
    }

    pub fn name(&self) -> String {
        self.original.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_string()
    }
}

/// The name of the file a conflict copy belongs to: `Game.sync-conflict-20261024-181502-ABCDEF7.srm` → `Game.srm`.
/// None for any other name.
pub fn original_name(name: &str) -> Option<String> {
    let (stem, _, ext) = split(name)?;
    Some(format!("{stem}{ext}"))
}

/// (what comes before the mark, `20261024-181502-ABCDEF7`, the extension with its dot or "").
fn split(name: &str) -> Option<(&str, &str, &str)> {
    let i = name.find(MARK)?;
    let rest = &name[i + MARK.len()..];
    let b = rest.as_bytes();
    let digits = |r: std::ops::Range<usize>| b.get(r).is_some_and(|d| d.iter().all(u8::is_ascii_digit));
    if !(digits(0..8) && b.get(8) == Some(&b'-') && digits(9..15) && b.get(15) == Some(&b'-')) {
        return None;
    }
    let id_len = rest[16..].find('.').unwrap_or(rest.len() - 16);
    if id_len == 0 || i == 0 {
        return None;
    }
    Some((&name[..i], &rest[..16 + id_len], &rest[16 + id_len..]))
}

/// The device part and the time of a conflict copy's name, for pairing a state's picture with its state.
fn tag(path: &Path) -> Option<(String, u64)> {
    let name = path.file_name()?.to_str()?;
    let (_, tag, _) = split(name)?;
    let time: u64 = tag[..15].replace('-', "").parse().ok()?;
    Some((tag[16..].to_string(), time))
}

/// The game's name in a state file's name: `uCity.pstate.auto` → `uCity`, `Zelda.state2.png` → `Zelda`,
/// `Metroid Prime.s03` → `Metroid Prime`. None for a name that isn't a state's.
pub fn state_stem(name: &str) -> Option<&str> {
    let name = name.strip_suffix(".png").unwrap_or(name);
    for long in [".pstate.auto.taken.retry", ".pstate.auto.taken", ".pstate.auto", ".pstate.undo", ".state.auto"] {
        if let Some(stem) = name.strip_suffix(long) {
            return Some(stem).filter(|s| !s.is_empty());
        }
    }
    let (stem, ext) = name.rsplit_once('.')?;
    let numbered = |prefix: &str| ext.strip_prefix(prefix).is_some_and(|n| n.bytes().all(|b| b.is_ascii_digit()));
    let dolphin = ext.len() == 3 && ext.starts_with('s') && ext[1..].bytes().all(|b| b.is_ascii_digit());
    let ok = numbered("pstate") || numbered("state") || dolphin || ext == "pocket-auto" || ext == "pocket-undo";
    Some(stem).filter(|s| ok && !s.is_empty())
}

/// A folder to look through, and how deep.
#[derive(Clone, Debug, PartialEq)]
pub struct Root {
    pub path: PathBuf,
    pub depth: usize,
}

/// The folders of the shares that are on (BIOS files and Library data are not saves and are left out), as deep as their
/// saves lie.
pub fn roots(prefs: &Prefs, layout: &Layout) -> Vec<Root> {
    let device = Layout::device();
    Share::ALL
        .into_iter()
        .filter(|s| s.saves() && prefs.shared(*s))
        .flat_map(Share::dirs)
        .map(|d| {
            // A test's layout moves the device's folders below its own root.
            let path = Path::new(d.path);
            let path = match (path.strip_prefix(&device.roms), path.strip_prefix(&device.config)) {
                (Ok(rel), _) => layout.roms.join(rel),
                (_, Ok(rel)) => layout.config.join(rel),
                _ => path.to_path_buf(),
            };
            let depth = match d.id {
                // <system>/<subfolder>/<game>.srm
                "pocket-saves" => 3,
                "pocket-states" => 2,
                "pocket-ps2-cards" => 1,
                "pocket-gc-cards" => 2,
                // <type>/<id>/data/<file>
                "pocket-wii-saves" => 5,
                // <id>/<id>/title/00040000/<id>/data/<…>
                _ => 9,
            };
            Root { path, depth }
        })
        .collect()
}

/// Every conflict copy below `roots`, gathered by the file it belongs to. Folders that are another root's, the
/// backup folder, BIOS files and hidden folders are not entered; what can't be read is left out.
pub fn find(roots: &[Root], layout: &Layout) -> Vec<Group> {
    let mut skip: Vec<PathBuf> = roots.iter().map(|r| r.path.clone()).collect();
    skip.push(layout.backup.clone());
    skip.push(layout.roms.join("bios"));
    let mut copies = Vec::new();
    for root in roots {
        let mut budget = BUDGET;
        walk(&root.path, root.depth, &skip, &mut copies, &mut budget);
    }
    copies.sort();
    copies.dedup();
    gather(&copies, layout)
}

fn walk(dir: &Path, depth: usize, skip: &[PathBuf], out: &mut Vec<PathBuf>, budget: &mut usize) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        if *budget == 0 {
            return;
        }
        *budget -= 1;
        let name = e.file_name();
        let Some(name) = name.to_str() else { continue };
        // The kind comes with the entry on the card's file systems: no stat per file.
        let Ok(t) = e.file_type() else { continue };
        let path = e.path();
        if t.is_dir() {
            if depth > 1 && !name.starts_with('.') && !skip.contains(&path) {
                walk(&path, depth - 1, skip, out, budget);
            }
        } else if t.is_file() && name.contains(MARK) && original_name(name).is_some() {
            out.push(path);
        }
    }
}

fn version(path: &Path) -> Option<Version> {
    let m = std::fs::metadata(path).ok().filter(|m| m.is_file())?;
    let mtime = m.modified().ok()?.duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    Some(Version {
        path: path.to_path_buf(),
        mtime,
        size: m.len(),
    })
}

fn ext_of(path: &Path) -> String {
    path.extension().and_then(|e| e.to_str()).unwrap_or_default().to_ascii_lowercase()
}

/// What kind of file `original` is.
pub fn kind(original: &Path, layout: &Layout) -> Kind {
    let ext = ext_of(original);
    if CARD_EXTS.contains(&ext.as_str()) {
        Kind::Card
    } else if original.starts_with(&layout.states) {
        Kind::State
    } else if original.starts_with(&layout.roms) && SAVE_EXTS.contains(&ext.as_str()) {
        Kind::Save
    } else {
        Kind::Other
    }
}

/// Conflict copies by the file they belong to. A state's picture goes with its state; a picture alone (only the
/// picture differs) is no choice worth asking about and is left as it is.
pub fn gather(copies: &[PathBuf], layout: &Layout) -> Vec<Group> {
    let mut groups: Vec<Group> = Vec::new();
    for copy in copies {
        let Some(name) = copy.file_name().and_then(|n| n.to_str()).and_then(original_name) else { continue };
        let Some(v) = version(copy) else { continue };
        let original = copy.with_file_name(name);
        match groups.iter_mut().find(|g| g.original == original) {
            Some(g) => g.others.push(v),
            None => groups.push(Group {
                kind: kind(&original, layout),
                current: version(&original),
                original,
                others: vec![v],
                previews: Vec::new(),
            }),
        }
    }
    // Pictures of states: `X.pstate.png` belongs to `X.pstate`.
    let mut pictures = Vec::new();
    groups.retain(|g| {
        let is_picture = g.kind == Kind::State && ext_of(&g.original) == "png";
        if is_picture {
            pictures.push(g.clone());
        }
        !is_picture
    });
    for p in pictures {
        let state = p.original.with_extension("");
        if let Some(g) = groups.iter_mut().find(|g| g.original == state) {
            g.previews.extend(p.others.into_iter().map(|v| v.path));
        }
    }
    groups
}

/// Whose a group is.
#[derive(Clone, Debug, PartialEq)]
pub enum Owner {
    /// One game, by its file.
    Game(PathBuf),
    /// A memory card every game of the system saves on.
    System(String),
    Nobody,
}

/// A game of the library, as `owner` needs it.
pub struct Game<'a> {
    pub system: &'a str,
    pub path: &'a Path,
}

fn stem(p: &Path) -> &str {
    p.file_stem().and_then(|s| s.to_str()).unwrap_or_default()
}

/// Whose `g` is among `games`: the game whose save or state it is (same folder and name), the system whose memory
/// card it is, or nobody.
pub fn owner(g: &Group, games: &[Game], layout: &Layout) -> Owner {
    let dir = g.original.parent().unwrap_or(Path::new(""));
    let same_place = |game: &&Game| game.path.parent() == Some(dir);
    match g.kind {
        Kind::Save => games
            .iter()
            .find(|game| same_place(game) && stem(game.path) == stem(&g.original))
            .map_or(Owner::Nobody, |game| Owner::Game(game.path.to_path_buf())),
        Kind::State => {
            let system = dir.strip_prefix(&layout.states).ok().and_then(|s| s.to_str()).unwrap_or_default();
            let name = g.name();
            let Some(game_stem) = state_stem(&name) else { return Owner::Nobody };
            games
                .iter()
                .find(|game| game.system == system && stem(game.path) == game_stem)
                .map_or(Owner::Nobody, |game| Owner::Game(game.path.to_path_buf()))
        }
        Kind::Card => {
            // A PlayStation core's card of one game: `<game>.mcd`, `<game>_1.mcr`.
            let card = stem(&g.original);
            let own = games.iter().find(|game| {
                let s = stem(game.path);
                same_place(game) && (card == s || card.strip_prefix(s).is_some_and(|r| r.starts_with('_')))
            });
            if let Some(game) = own {
                return Owner::Game(game.path.to_path_buf());
            }
            let rel = dir.strip_prefix(&layout.roms).unwrap_or(Path::new(""));
            let first = rel.components().next().and_then(|c| c.as_os_str().to_str()).unwrap_or_default();
            match first {
                // Dolphin's cards beside its BIOS files are the GameCube's.
                "bios" => Owner::System("gamecube".into()),
                "" => Owner::Nobody,
                s => Owner::System(s.to_string()),
            }
        }
        Kind::Other => Owner::Nobody,
    }
}

/// Where a file goes in the backup folder `folder`: its path below /storage/roms (or /storage/.config), a free name.
pub fn backup_path(path: &Path, folder: &Path, layout: &Layout) -> PathBuf {
    let rel = path
        .strip_prefix(&layout.roms)
        .or_else(|_| path.strip_prefix(&layout.config))
        .map(Path::to_path_buf)
        .unwrap_or_else(|_| PathBuf::from(path.file_name().unwrap_or_default()));
    let dir = folder.join(rel.parent().unwrap_or(Path::new("")));
    free_name(&dir, rel.file_name().and_then(|n| n.to_str()).unwrap_or("file"))
}

/// `dir/name`, or `dir/name (2).ext`, `(3)` … when that is taken.
pub fn free_name(dir: &Path, name: &str) -> PathBuf {
    let first = dir.join(name);
    if !first.exists() {
        return first;
    }
    let (base, ext) = match name.rfind('.') {
        Some(i) if i > 0 => (&name[..i], &name[i..]),
        _ => (name, ""),
    };
    (2..)
        .map(|n| dir.join(format!("{base} ({n}){ext}")))
        .find(|p| !p.exists())
        .expect("a free name")
}

/// Moves a file, also to another file system (copied, flushed, compared, then removed). Never replaces one.
pub fn move_file(from: &Path, to: &Path) -> std::io::Result<()> {
    if to.exists() {
        return Err(std::io::Error::new(std::io::ErrorKind::AlreadyExists, format!("{} is there", to.display())));
    }
    if let Some(dir) = to.parent() {
        std::fs::create_dir_all(dir)?;
    }
    match std::fs::rename(from, to) {
        Ok(()) => Ok(()),
        Err(e) if e.raw_os_error() == Some(libc::EXDEV) => {
            copy_flushed(from, to)?;
            std::fs::remove_file(from)
        }
        Err(e) => Err(e),
    }
}

/// A copy that is on the card when this returns, and as long as the original.
fn copy_flushed(from: &Path, to: &Path) -> std::io::Result<()> {
    if let Some(dir) = to.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let n = std::fs::copy(from, to)?;
    std::fs::File::open(to)?.sync_all()?;
    if n != std::fs::metadata(from)?.len() {
        let _ = std::fs::remove_file(to);
        return Err(std::io::Error::other("the copy came out short"));
    }
    Ok(())
}

/// What `keep` did.
#[derive(Debug, Default, PartialEq)]
pub struct Kept {
    /// (from, to) of every file that went to the backup folder.
    pub moved: Vec<(PathBuf, PathBuf)>,
}

/// Keeps version `pick` of `g` (an index into `g.versions()`) in the file's place and moves every other version
/// into `folder` (a dated folder under the backup folder). Nothing is deleted: the version in use that is replaced
/// is copied away first, and only then replaced in one step. Asked again after it was done, it finds nothing left
/// to move and does nothing; a version that has gone meanwhile is an error and nothing is touched.
pub fn keep(g: &Group, pick: usize, folder: &Path, layout: &Layout) -> Result<Kept, String> {
    let versions = g.versions();
    let chosen = versions.get(pick).ok_or("That version is not there any more.")?;
    let is_current = g.current.as_ref().is_some_and(|c| c.path == chosen.path);
    let copies: Vec<&Path> = g.others.iter().map(|v| v.path.as_path()).filter(|p| p.is_file()).collect();
    let mut kept = Kept::default();
    let away = |path: &Path, kept: &mut Kept| -> Result<(), String> {
        let to = backup_path(path, folder, layout);
        move_file(path, &to).map_err(|e| format!("{}: {e}", path.display()))?;
        kept.moved.push((path.to_path_buf(), to));
        Ok(())
    };
    let mut picture_copy = None;
    if is_current {
        if !g.original.is_file() {
            return Err("The version in use is not there any more. Nothing was moved.".into());
        }
    } else {
        if !chosen.path.is_file() {
            // Done before: the copy is in the file's place and no other copy is left.
            if copies.is_empty() {
                return Ok(kept);
            }
            return Err("That version is not there any more. Nothing was moved.".into());
        }
        if g.original.is_file() {
            let to = backup_path(&g.original, folder, layout);
            copy_flushed(&g.original, &to).map_err(|e| format!("{}: {e}", g.original.display()))?;
            if let Err(e) = std::fs::rename(&chosen.path, &g.original) {
                let _ = std::fs::remove_file(&to);
                return Err(format!("{}: {e}", g.original.display()));
            }
            kept.moved.push((g.original.clone(), to));
        } else {
            std::fs::rename(&chosen.path, &g.original).map_err(|e| format!("{}: {e}", g.original.display()))?;
        }
        // The state's picture of the same moment: the one from the same device closest in time.
        picture_copy = tag(&chosen.path).and_then(|(device, time)| {
            g.previews
                .iter()
                .filter(|p| p.is_file())
                .filter_map(|p| tag(p).filter(|(d, _)| *d == device).map(|(_, t)| (t.abs_diff(time), p)))
                .min_by_key(|(d, _)| *d)
                .map(|(_, p)| p.clone())
        });
    }
    for c in copies.into_iter().filter(|c| *c != chosen.path) {
        away(c, &mut kept)?;
    }
    if let Some(p) = &picture_copy {
        let picture = PathBuf::from(format!("{}.png", g.original.display()));
        if picture.is_file() {
            away(&picture, &mut kept)?;
        }
        std::fs::rename(p, &picture).map_err(|e| format!("{}: {e}", picture.display()))?;
    }
    for p in g.previews.iter().filter(|p| p.is_file() && Some(*p) != picture_copy.as_ref()) {
        away(p, &mut kept)?;
    }
    Ok(kept)
}

/// The dated backup folder for a day: `<backup>/2026-10-25`.
pub fn day_folder(layout: &Layout, unix: u64) -> PathBuf {
    let (y, m, d, _, _) = crate::backup::local(unix);
    layout.backup.join(format!("{y:04}-{m:02}-{d:02}"))
}

/// "24 Oct 2026, 18:02" in the device's time zone.
pub fn when(unix: u64) -> String {
    let t = crate::backup::local(unix);
    const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    format!("{} {} {}, {:02}:{:02}", t.2, MONTHS[(t.1 as usize).saturating_sub(1).min(11)], t.0, t.3, t.4)
}

/// The words for a group, wherever it is listed.
pub fn what(kind: Kind) -> &'static str {
    match kind {
        Kind::Save => "Game save",
        Kind::State => "Saved state",
        Kind::Card => "Memory card",
        Kind::Other => "Save data",
    }
}

/// The card's line for a game's groups: "Two versions of this game's save. Choose one…".
pub fn card_line(groups: &[&Group]) -> Option<String> {
    let first = groups.first()?;
    if groups.len() > 1 {
        return Some(format!("{} saves of this game have two versions. Choose…", groups.len()));
    }
    // Whole sentences, so that each has its line in the German table.
    Some(match (first.kind, first.versions().len()) {
        (Kind::Card, 2) => "Two versions of the memory card. Choose one…".to_string(),
        (Kind::Card, n) => format!("{n} versions of the memory card. Choose one…"),
        (Kind::State, 2) => "Two versions of a save state. Choose one…".to_string(),
        (Kind::State, n) => format!("{n} versions of a save state. Choose one…"),
        (_, 2) => "Two versions of this game's save. Choose one…".to_string(),
        (_, n) => format!("{n} versions of this game's save. Choose one…"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;

    /// Keeps the folder until the test ends (removed on drop).
    struct Tmp(#[allow(dead_code)] crate::testutil::TestDir);

    fn tmp(name: &str) -> (Tmp, Layout) {
        let root = TestDir::new(&format!("conflicts-{name}"));
        let layout = Layout::under(&root);
        (Tmp(root), layout)
    }

    fn put(path: &Path, text: &str, mtime: u64) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
        let t = std::time::UNIX_EPOCH + std::time::Duration::from_secs(mtime);
        std::fs::File::options().write(true).open(path).unwrap().set_modified(t).unwrap();
    }

    fn read(path: &Path) -> String {
        std::fs::read_to_string(path).unwrap_or_else(|_| panic!("{} missing", path.display()))
    }

    fn all_roots(layout: &Layout) -> Vec<Root> {
        let prefs = Prefs { enabled: true, ..Prefs::default() };
        roots(&prefs, layout)
    }

    /// Every file below `dir`, relative, sorted.
    fn files(dir: &Path) -> Vec<String> {
        fn go(d: &Path, base: &Path, out: &mut Vec<String>) {
            for e in std::fs::read_dir(d).into_iter().flatten().flatten() {
                let p = e.path();
                if p.is_dir() {
                    go(&p, base, out);
                } else {
                    out.push(p.strip_prefix(base).unwrap().display().to_string());
                }
            }
        }
        let mut out = Vec::new();
        go(dir, dir, &mut out);
        out.sort();
        out
    }

    #[test]
    fn syncthings_conflict_names_lead_back_to_the_file() {
        assert_eq!(original_name("Game.sync-conflict-20261024-181502-ABCDEF7.srm").as_deref(), Some("Game.srm"));
        assert_eq!(original_name("uCity.pstate.sync-conflict-20261024-181502-ABCDEF7.auto").as_deref(), Some("uCity.pstate.auto"));
        assert_eq!(original_name("Mcd001.sync-conflict-20261024-181502-ABCDEF7.ps2").as_deref(), Some("Mcd001.ps2"));
        assert_eq!(original_name("banner.sync-conflict-20261024-181502-ABCDEF7").as_deref(), Some("banner"));
        assert_eq!(original_name("Super Mario Bros. 3.sync-conflict-20261024-181502-ABCDEF7.srm").as_deref(), Some("Super Mario Bros. 3.srm"));
        // Look-alikes are not conflicts.
        assert_eq!(original_name("Game.srm"), None);
        assert_eq!(original_name("Game.sync-conflict-2026-ABC.srm"), None);
        assert_eq!(original_name(".sync-conflict-20261024-181502-ABCDEF7.srm"), None);
        assert_eq!(original_name("Game.sync-conflict-20261024-181502-.srm"), None);
    }

    #[test]
    fn state_names_give_the_games_name() {
        assert_eq!(state_stem("uCity.pstate"), Some("uCity"));
        assert_eq!(state_stem("uCity.pstate7"), Some("uCity"));
        assert_eq!(state_stem("uCity.pstate.auto"), Some("uCity"));
        assert_eq!(state_stem("uCity.pstate.auto.taken"), Some("uCity"));
        assert_eq!(state_stem("uCity.pstate2.png"), Some("uCity"));
        assert_eq!(state_stem("Super Mario Bros. 3.state.auto"), Some("Super Mario Bros. 3"));
        assert_eq!(state_stem("Metroid Prime.s03"), Some("Metroid Prime"));
        assert_eq!(state_stem("Metroid Prime.pocket-auto"), Some("Metroid Prime"));
        assert_eq!(state_stem("SLUS-20946 (8B4A4E26).01.p2s"), None);
        assert_eq!(state_stem("notes.txt"), None);
    }

    #[test]
    fn the_shared_folders_are_walked_and_conflicts_gathered_by_file() {
        let (_t, l) = tmp("find");
        let gba = l.roms.join("gba");
        put(&gba.join("Game.gba"), "rom", 1);
        put(&gba.join("Game.srm"), "here", 2_000_000_000);
        put(&gba.join("Game.sync-conflict-20261024-181502-ABCDEF7.srm"), "there", 1_900_000_000);
        put(&gba.join("Other.srm"), "fine", 5);
        put(&l.states.join("gba/Game.pstate.sync-conflict-20261024-181502-ABCDEF7.auto"), "s2", 5);
        put(&l.states.join("gba/Game.pstate.auto"), "s1", 6);
        put(&l.roms.join("ps2/Mcd001.ps2"), "card", 6);
        put(&l.roms.join("ps2/Mcd001.sync-conflict-20261024-181502-ABCDEF7.ps2"), "card2", 7);
        // Never looked at: the backup folder, BIOS files, hidden folders.
        put(&l.backup.join("2026-10-24/gba/Game.sync-conflict-20261020-101010-ABCDEF7.srm"), "old", 1);
        put(&l.roms.join("bios/x.sync-conflict-20261024-181502-ABCDEF7.bin"), "b", 1);
        put(&l.roms.join(".stversions/gba/Game.sync-conflict-20261024-181502-ABCDEF7.srm"), "v", 1);
        let groups = find(&all_roots(&l), &l);
        let names: Vec<(String, Kind)> = groups.iter().map(|g| (g.name(), g.kind)).collect();
        assert_eq!(
            names,
            [("Game.srm".to_string(), Kind::Save), ("Mcd001.ps2".to_string(), Kind::Card), ("Game.pstate.auto".to_string(), Kind::State)]
        );
        let save = &groups[0];
        assert_eq!(save.current.as_ref().map(|v| (v.size, v.mtime)), Some((4, 2_000_000_000)));
        assert_eq!(save.others.len(), 1);
        assert_eq!(save.others[0].size, 5);
        assert_eq!(save.versions().len(), 2);
    }

    #[test]
    fn nothing_to_find_is_an_empty_list_not_an_error() {
        let (_t, l) = tmp("none");
        // No folders at all: Syncthing never set up, a games card not in.
        assert!(find(&all_roots(&l), &l).is_empty());
        // Folders, but no conflicts.
        put(&l.roms.join("gba/Game.srm"), "x", 1);
        assert!(find(&all_roots(&l), &l).is_empty());
        // Shares switched off: their folders are not looked at.
        put(&l.roms.join("gba/Game.sync-conflict-20261024-181502-ABCDEF7.srm"), "y", 1);
        let none = Prefs { saves: false, states: false, ps2: false, dolphin: false, azahar: false, ..Prefs::default() };
        assert!(find(&roots(&none, &l), &l).is_empty());
        assert_eq!(find(&all_roots(&l), &l).len(), 1);
    }

    #[cfg(unix)]
    #[test]
    fn an_unreadable_folder_is_left_out_quietly() {
        use std::os::unix::fs::PermissionsExt;
        let (_t, l) = tmp("unreadable");
        let locked = l.roms.join("snes");
        put(&locked.join("A.sync-conflict-20261024-181502-ABCDEF7.srm"), "x", 1);
        put(&l.roms.join("gba/B.sync-conflict-20261024-181502-ABCDEF7.srm"), "y", 1);
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
        let groups = find(&all_roots(&l), &l);
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();
        // root reads it anyway; anyone else finds only the other one. Either way: no error, no empty entry.
        assert!(groups.iter().any(|g| g.name() == "B.srm"));
        assert!(groups.iter().all(|g| !g.others.is_empty()));
    }

    #[test]
    fn a_states_picture_goes_with_the_state_and_a_picture_alone_is_no_choice() {
        let (_t, l) = tmp("pictures");
        let gb = l.states.join("gbc");
        put(&gb.join("uCity.pstate"), "a", 1);
        put(&gb.join("uCity.sync-conflict-20261024-181502-ABCDEF7.pstate"), "b", 1);
        put(&gb.join("uCity.pstate.png"), "pa", 1);
        put(&gb.join("uCity.pstate.sync-conflict-20261024-181503-ABCDEF7.png"), "pb", 1);
        put(&gb.join("Zelda.pstate.sync-conflict-20261024-181503-ABCDEF7.png"), "pz", 1);
        let groups = find(&all_roots(&l), &l);
        assert_eq!(groups.len(), 1, "{groups:?}");
        assert_eq!(groups[0].previews, [gb.join("uCity.pstate.sync-conflict-20261024-181503-ABCDEF7.png")]);
    }

    fn group(l: &Layout, rel: &str) -> Group {
        let original = l.roms.join(rel);
        Group { kind: kind(&original, l), original, current: None, others: Vec::new(), previews: Vec::new() }
    }

    #[test]
    fn a_conflict_belongs_to_the_game_of_the_same_name_or_to_the_whole_system_for_a_card() {
        let l = Layout::under(Path::new("/x"));
        let paths: Vec<(&str, PathBuf)> = vec![
            ("gba", l.roms.join("gba/Game.gba")),
            ("gba", l.roms.join("gba/Game 2.gba")),
            ("psx", l.roms.join("psx/Crash.chd")),
            ("ps2", l.roms.join("ps2/Gran Turismo 4.iso")),
            ("gamecube", l.roms.join("gamecube/Melee.rvz")),
        ];
        let games: Vec<Game> = paths.iter().map(|(s, p)| Game { system: s, path: p }).collect();
        let of = |rel: &str| owner(&group(&l, rel), &games, &l);
        assert_eq!(of("gba/Game.srm"), Owner::Game(l.roms.join("gba/Game.gba")));
        assert_eq!(of("gba/Game 2.srm"), Owner::Game(l.roms.join("gba/Game 2.gba")));
        assert_eq!(of("gba/Unknown.srm"), Owner::Nobody, "no game of that name");
        assert_eq!(of("snes/Game.srm"), Owner::Nobody, "same name, other folder");
        assert_eq!(of("savestates/gba/Game.pstate.auto"), Owner::Game(l.roms.join("gba/Game.gba")));
        assert_eq!(of("savestates/gba/Game 2.state1"), Owner::Game(l.roms.join("gba/Game 2.gba")));
        assert_eq!(of("savestates/snes/Game.pstate"), Owner::Nobody, "the state of another system's game");
        assert_eq!(of("savestates/ps2/SLUS-20946 (8B4A4E26).01.p2s"), Owner::Nobody);
        assert_eq!(of("ps2/Mcd001.ps2"), Owner::System("ps2".into()));
        assert_eq!(of("bios/GC/MemoryCardA.USA.raw"), Owner::System("gamecube".into()));
        assert_eq!(of("psx/Crash_1.mcr"), Owner::Game(l.roms.join("psx/Crash.chd")), "a card of one game");
        assert_eq!(of("psx/shared_card_1.mcd"), Owner::System("psx".into()));
        let wii = Group {
            kind: Kind::Other,
            original: l.config.join("dolphin-emu/Wii/title/00010000/524d4745/data/banner.bin"),
            current: None,
            others: Vec::new(),
            previews: Vec::new(),
        };
        assert_eq!(owner(&wii, &games, &l), Owner::Nobody);
        assert_eq!(owner(&group(&l, "gba/Game.srm"), &[], &l), Owner::Nobody, "an empty library");
    }

    fn conflict(l: &Layout) -> Group {
        let gba = l.roms.join("gba");
        put(&gba.join("Game.srm"), "mine", 2_000_000_000);
        put(&gba.join("Game.sync-conflict-20261024-181502-ABCDEF7.srm"), "theirs", 1_900_000_000);
        let groups = find(&all_roots(l), l);
        assert_eq!(groups.len(), 1);
        groups.into_iter().next().unwrap()
    }

    #[test]
    fn keeping_the_version_in_use_moves_the_other_into_the_backup_folder() {
        let (_t, l) = tmp("keep-current");
        let g = conflict(&l);
        let folder = l.backup.join("2026-10-25");
        let kept = keep(&g, 0, &folder, &l).unwrap();
        assert_eq!(read(&l.roms.join("gba/Game.srm")), "mine");
        assert_eq!(read(&folder.join("gba/Game.sync-conflict-20261024-181502-ABCDEF7.srm")), "theirs");
        assert_eq!(kept.moved.len(), 1);
        assert!(find(&all_roots(&l), &l).is_empty(), "the backup folder is never searched");
        // Again: nothing left to do, nothing changes.
        assert_eq!(keep(&g, 0, &folder, &l), Ok(Kept::default()));
        assert_eq!(files(&l.backup), ["2026-10-25/gba/Game.sync-conflict-20261024-181502-ABCDEF7.srm"]);
    }

    #[test]
    fn keeping_the_other_version_puts_it_in_place_and_the_one_in_use_into_the_backup_folder() {
        let (_t, l) = tmp("keep-other");
        let g = conflict(&l);
        let folder = l.backup.join("2026-10-25");
        keep(&g, 1, &folder, &l).unwrap();
        assert_eq!(read(&l.roms.join("gba/Game.srm")), "theirs");
        assert_eq!(read(&folder.join("gba/Game.srm")), "mine");
        assert_eq!(files(&l.roms.join("gba")), ["Game.srm"]);
        // Done twice (a second press, a retry): nothing more happens and nothing is lost.
        assert_eq!(keep(&g, 1, &folder, &l), Ok(Kept::default()));
        assert_eq!(read(&l.roms.join("gba/Game.srm")), "theirs");
        assert_eq!(files(&l.backup), ["2026-10-25/gba/Game.srm"]);
    }

    #[test]
    fn a_name_taken_in_the_backup_folder_gets_a_number() {
        let (_t, l) = tmp("collide");
        let folder = l.backup.join("2026-10-25");
        // The same game resolved twice in one day.
        let g = conflict(&l);
        keep(&g, 1, &folder, &l).unwrap();
        let g = conflict(&l);
        keep(&g, 1, &folder, &l).unwrap();
        let g = conflict(&l);
        keep(&g, 1, &folder, &l).unwrap();
        assert_eq!(files(&folder), ["gba/Game (2).srm", "gba/Game (3).srm", "gba/Game.srm"]);
        let g = conflict(&l);
        keep(&g, 0, &folder, &l).unwrap();
        let g = conflict(&l);
        keep(&g, 0, &folder, &l).unwrap();
        assert!(files(&folder).contains(&"gba/Game.sync-conflict-20261024-181502-ABCDEF7 (2).srm".to_string()));
        assert_eq!(free_name(&folder, "noext"), folder.join("noext"));
    }

    #[test]
    fn a_version_gone_meanwhile_is_an_error_and_nothing_moves() {
        let (_t, l) = tmp("gone");
        let mut g = conflict(&l);
        // A third version that is gone by the time A is pressed.
        g.others.push(Version { path: l.roms.join("gba/Game.sync-conflict-20261025-090000-ZZZZZZZ.srm"), mtime: 1, size: 1 });
        let folder = l.backup.join("2026-10-25");
        assert!(keep(&g, 2, &folder, &l).is_err());
        assert!(keep(&g, 7, &folder, &l).is_err());
        assert_eq!(read(&l.roms.join("gba/Game.srm")), "mine");
        assert_eq!(files(&l.roms.join("gba")).len(), 2);
        assert!(!l.backup.exists());
        // The version in use gone: no choice is made for it.
        std::fs::remove_file(l.roms.join("gba/Game.srm")).unwrap();
        assert!(keep(&g, 0, &folder, &l).is_err());
    }

    #[test]
    fn three_versions_keep_one_and_move_two() {
        let (_t, l) = tmp("three");
        let gba = l.roms.join("gba");
        put(&gba.join("Game.srm"), "a", 3);
        put(&gba.join("Game.sync-conflict-20261024-181502-ABCDEF7.srm"), "b", 2);
        put(&gba.join("Game.sync-conflict-20261025-090000-QQQQQQQ.srm"), "c", 1);
        let g = find(&all_roots(&l), &l).remove(0);
        assert_eq!(g.versions().len(), 3);
        assert_eq!(card_line(&[&g]).as_deref(), Some("3 versions of this game's save. Choose one…"));
        let folder = l.backup.join("d");
        keep(&g, 2, &folder, &l).unwrap();
        assert_eq!(read(&gba.join("Game.srm")), "c");
        assert_eq!(files(&folder).len(), 2);
        assert_eq!(files(&gba), ["Game.srm"]);
    }

    #[test]
    fn a_kept_state_takes_its_own_picture_along() {
        let (_t, l) = tmp("state");
        let gb = l.states.join("gbc");
        put(&gb.join("uCity.pstate"), "a", 1);
        put(&gb.join("uCity.sync-conflict-20261024-181502-ABCDEF7.pstate"), "b", 1);
        put(&gb.join("uCity.pstate.png"), "pa", 1);
        put(&gb.join("uCity.pstate.sync-conflict-20261024-181503-ABCDEF7.png"), "pb", 1);
        put(&gb.join("uCity.pstate.sync-conflict-20261020-101010-OTHERID.png"), "px", 1);
        let g = find(&all_roots(&l), &l).remove(0);
        let folder = l.backup.join("d");
        keep(&g, 1, &folder, &l).unwrap();
        assert_eq!(read(&gb.join("uCity.pstate")), "b");
        assert_eq!(read(&gb.join("uCity.pstate.png")), "pb");
        assert_eq!(files(&gb), ["uCity.pstate", "uCity.pstate.png"]);
        assert_eq!(
            files(&folder),
            ["savestates/gbc/uCity.pstate", "savestates/gbc/uCity.pstate.png", "savestates/gbc/uCity.pstate.sync-conflict-20261020-101010-OTHERID.png"]
        );
    }

    #[test]
    fn a_card_is_chosen_whole_and_said_so() {
        let (_t, l) = tmp("card");
        put(&l.roms.join("ps2/Mcd001.ps2"), "here", 1);
        put(&l.roms.join("ps2/Mcd001.sync-conflict-20261024-181502-ABCDEF7.ps2"), "there", 1);
        let g = find(&all_roots(&l), &l).remove(0);
        assert_eq!(g.kind, Kind::Card);
        assert_eq!(card_line(&[&g]).as_deref(), Some("Two versions of the memory card. Choose one…"));
        let folder = l.backup.join("d");
        keep(&g, 1, &folder, &l).unwrap();
        assert_eq!(read(&l.roms.join("ps2/Mcd001.ps2")), "there");
        assert_eq!(read(&folder.join("ps2/Mcd001.ps2")), "here");
    }

    #[test]
    fn the_card_line_says_what_has_two_versions() {
        let (_t, l) = tmp("lines");
        let g = conflict(&l);
        assert_eq!(card_line(&[]), None, "no conflict, no line");
        assert_eq!(card_line(&[&g]).as_deref(), Some("Two versions of this game's save. Choose one…"));
        let mut s = g.clone();
        s.kind = Kind::State;
        assert_eq!(card_line(&[&s]).as_deref(), Some("Two versions of a save state. Choose one…"));
        assert_eq!(card_line(&[&g, &s]).as_deref(), Some("2 saves of this game have two versions. Choose…"));
    }

    #[test]
    fn a_move_never_replaces_a_file() {
        let (_t, l) = tmp("move");
        let (a, b) = (l.roms.join("a"), l.roms.join("x/b"));
        put(&a, "a", 1);
        put(&b, "b", 1);
        assert!(move_file(&a, &b).is_err());
        assert_eq!(read(&a), "a");
        assert_eq!(read(&b), "b");
        let c = l.roms.join("y/z/c");
        move_file(&a, &c).unwrap();
        assert_eq!(read(&c), "a");
        assert!(!a.exists());
    }

    #[test]
    fn the_backup_folder_is_dated_and_kept_out_of_sync() {
        let l = Layout::device();
        let day = day_folder(&l, 1_761_400_000);
        assert!(day.starts_with(BACKUP_DIR));
        let name = day.file_name().unwrap().to_str().unwrap();
        assert_eq!(name.len(), 10);
        assert_eq!(&name[4..5], "-");
        assert!(crate::sync::saves_ignore().contains("\n/sync-backups\n"), "{}", crate::sync::saves_ignore());
    }
}
