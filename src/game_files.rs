//! What "Delete from the card" on the game card takes away (docs/user/playing.md, "Hide or delete a game"): every file
//! of the game, and, only when the player asks for it, its saves and save states.
//!
//! * **The game.** One file, or the set a sheet holds together, as the shelf reads it (`library::scan`): a `.cue` or
//!   `.gdi` with the tracks it names, an `.m3u` with its discs and their tracks. Names are matched in the sheet's own
//!   folder, ignoring case, as the shelf does; a name with a folder in it is not followed. A game that is a folder
//!   (DOS, ScummVM) is the folder. A folder that only held the game (`Xenogears/Xenogears.m3u`) goes when it is empty
//!   afterwards; anything else in it stays.
//! * **Saves**: the files beside the game with its name and a save type (`.srm`, `.sav`, PS1 memory cards …,
//!   [`SAVE_EXTS`]). Kept unless asked for. Shared memory cards (PS2, GameCube) are never a game's.
//! * **States**: `savestates/<system>/` files named after the game (pocket-core's slots, auto and Before load,
//!   RetroArch's, Dolphin's and Azahar's own, with their pictures), [`is_state_of`]. PS2 states are named after the
//!   disc's serial and can't be told from the file; they stay. Kept unless asked for.
//! * Its box art, game pictures and kept frame (pocket-ui's own copies) always go with it.
//!
//! [`plan`] only looks (sizes included) and runs on a thread; [`delete`] does what a plan says, and refuses a path that
//! is a system's folder or above it.

use std::fs;
use std::path::{Path, PathBuf};

/// Save types beside the games: the ones Sync shares (`sync::SAVES`) and the clock file of a cartridge with one.
pub const SAVE_EXTS: &[&str] = &["srm", "sav", "eep", "fla", "sra", "mcr", "mcd", "dsv", "rtc"];

/// Whether `path` is a save the Storage page and the card leave alone: a save type, a PS2 memory card
/// (`Mcd001.ps2`), a GameCube card (`MemoryCard A.USA.raw`, a `.gci`) or Azahar's save data.
pub fn is_save(path: &Path) -> bool {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or_default();
    let ext = path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).unwrap_or_default();
    SAVE_EXTS.contains(&ext.as_str())
        || ext == "gci"
        || (ext == "ps2" && name.starts_with("Mcd"))
        || (ext == "raw" && name.starts_with("MemoryCard"))
        || path.components().any(|c| c.as_os_str() == "azahar")
}

/// A file (or folder) and what it takes on the card.
#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub path: PathBuf,
    pub bytes: u64,
}

/// What deleting a game would take away.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Plan {
    /// The game's own files (or its folder).
    pub game: Vec<Item>,
    /// Its saves beside it.
    pub saves: Vec<Item>,
    /// Its save states.
    pub states: Vec<Item>,
    /// pocket-ui's pictures of it (box art, game pictures, kept frame): always deleted with the game.
    pub pictures: Vec<PathBuf>,
    /// The folder the game had to itself, removed when nothing else is left in it.
    pub own_folder: Option<PathBuf>,
}

impl Plan {
    pub fn game_bytes(&self) -> u64 {
        self.game.iter().map(|i| i.bytes).sum()
    }

    /// Saves and states together.
    pub fn kept_bytes(&self) -> u64 {
        self.saves.iter().chain(&self.states).map(|i| i.bytes).sum()
    }

    /// What the delete frees: the game, and the saves and states when they go too.
    pub fn freed(&self, with_saves: bool) -> u64 {
        self.game_bytes() + if with_saves { self.kept_bytes() } else { 0 }
    }

    /// Files it deletes (a folder counts as one).
    pub fn files(&self, with_saves: bool) -> usize {
        self.game.len() + if with_saves { self.saves.len() + self.states.len() } else { 0 }
    }
}

/// Where a game's other files are: the states folder (`/storage/roms/savestates`), pocket-ui's data folder, and the
/// folders that must never go (the ROM folders of the systems and the folder above them).
pub struct Places<'a> {
    pub states: &'a Path,
    pub data: &'a Path,
    pub protected: &'a [PathBuf],
}

/// What deleting the game `path` of `system` would take, with sizes. Reads a sheet or two and lists two folders: run
/// it on a thread.
pub fn plan(system: &str, path: &Path, places: &Places) -> Result<Plan, String> {
    if is_protected(path, places.protected) {
        return Err("This is a folder on the SD card, not a game.".into());
    }
    let meta = fs::symlink_metadata(path).map_err(|_| "The game's file is no longer on the SD card.".to_string())?;
    let mut plan = Plan::default();
    if meta.is_dir() {
        plan.game.push(Item { path: path.to_path_buf(), bytes: tree_bytes(path) });
    } else {
        for f in files_of(path) {
            let bytes = fs::symlink_metadata(&f).map_or(0, |m| m.len());
            plan.game.push(Item { path: f, bytes });
        }
        plan.saves = saves_of(path).into_iter().map(|p| item(&p)).collect();
    }
    plan.states = states_of(system, path, places.states).into_iter().map(|p| item(&p)).collect();
    plan.pictures = pictures_of(system, path, places.data).into_iter().filter(|p| p.exists()).collect();
    plan.own_folder = own_folder(path, places.protected);
    Ok(plan)
}

fn item(p: &Path) -> Item {
    Item { path: p.to_path_buf(), bytes: fs::symlink_metadata(p).map_or(0, |m| m.len()) }
}

/// The game is one of the folders that must stay, or above one of them.
fn is_protected(path: &Path, protected: &[PathBuf]) -> bool {
    path.parent().is_none() || protected.iter().any(|p| p.starts_with(path))
}

/// The file and every file its sheet holds together: `.cue`/`.gdi` tracks, `.m3u` discs and their tracks, matched in
/// the same folder ignoring case. The game's own file first.
pub fn files_of(path: &Path) -> Vec<PathBuf> {
    let mut out = vec![path.to_path_buf()];
    let Some(dir) = path.parent() else { return out };
    let here: Vec<PathBuf> = fs::read_dir(dir).map(|r| r.flatten().map(|e| e.path()).filter(|p| p.is_file()).collect()).unwrap_or_default();
    let find = |name: &str| here.iter().find(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.eq_ignore_ascii_case(name))).cloned();
    let mut i = 0;
    // A playlist names sheets, a sheet names tracks: two levels, never round in circles.
    while i < out.len() && i < 512 {
        for name in sheet_names(&out[i]) {
            if let Some(f) = find(&name).filter(|f| !out.contains(f)) {
                out.push(f);
            }
        }
        i += 1;
    }
    out
}

/// The file names a `.cue`, `.gdi` or `.m3u` names in its own folder; nothing for any other file.
fn sheet_names(path: &Path) -> Vec<String> {
    let kind = path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).unwrap_or_default();
    if !matches!(kind.as_str(), "cue" | "m3u" | "gdi") || fs::metadata(path).map_or(true, |m| m.len() > 1 << 20) {
        return Vec::new();
    }
    let Ok(bytes) = fs::read(path) else { return Vec::new() };
    crate::library::sheet_references(&kind, &String::from_utf8_lossy(&bytes))
        .into_iter()
        .map(|n| n.strip_prefix("./").map(str::to_string).unwrap_or(n))
        .filter(|n| !n.contains(['/', '\\']))
        .collect()
}

/// The saves beside the game: its name with a save type (`Game.srm`, `Game.sav`, `Game.mcr`), any case.
pub fn saves_of(path: &Path) -> Vec<PathBuf> {
    let (Some(dir), Some(stem)) = (path.parent(), path.file_stem().and_then(|s| s.to_str())) else { return Vec::new() };
    let mut out: Vec<PathBuf> = fs::read_dir(dir)
        .map(|r| r.flatten().map(|e| e.path()).collect::<Vec<PathBuf>>())
        .unwrap_or_default()
        .into_iter()
        .filter(|p: &PathBuf| p.is_file() && *p != path)
        .filter(|p| {
            let ext = p.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).unwrap_or_default();
            p.file_stem().and_then(|s| s.to_str()) == Some(stem) && SAVE_EXTS.contains(&ext.as_str())
        })
        .collect();
    out.sort();
    out
}

/// The game's states in `<states>/<system>/`: files named after its file name without (pocket-core, RetroArch) or with
/// its type (Dolphin, Azahar), followed by a state's ending ([`is_state_of`]).
pub fn states_of(system: &str, path: &Path, states: &Path) -> Vec<PathBuf> {
    let (Some(stem), Some(file)) = (path.file_stem().and_then(|s| s.to_str()), path.file_name().and_then(|s| s.to_str())) else { return Vec::new() };
    let dir = states.join(system);
    let mut out: Vec<PathBuf> = fs::read_dir(&dir)
        .map(|r| r.flatten().map(|e| e.path()).collect::<Vec<PathBuf>>())
        .unwrap_or_default()
        .into_iter()
        .filter(|p: &PathBuf| p.is_file())
        .filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| is_state_of(n, stem) || is_state_of(n, file)))
        .collect();
    out.sort();
    out
}

/// Whether `name` is a state of the game whose name (stem or file name) is `base`: `base` and then exactly a state's
/// ending, so "Mario.pstate" is not a state of "Mario. Bros". Endings: `.pstate`, `.pstate1`…, `.pstate.auto`
/// (`.taken`, `.retry`), `.pstate.undo`; `.state`, `.state1`…, `.state.auto`; `.s00`…`.s99`, `.pocket-auto`,
/// `.pocket-undo`; `.auto.cst`, `.undo.cst`; each also with `.png`.
pub fn is_state_of(name: &str, base: &str) -> bool {
    let Some(rest) = name.strip_prefix(base) else { return false };
    let rest = rest.strip_suffix(".png").unwrap_or(rest);
    let digits = |s: &str| s.chars().all(|c| c.is_ascii_digit());
    if let Some(r) = rest.strip_prefix(".pstate") {
        let r = r.trim_start_matches(|c: char| c.is_ascii_digit());
        return matches!(r, "" | ".auto" | ".auto.taken" | ".auto.taken.retry" | ".undo");
    }
    if let Some(r) = rest.strip_prefix(".state") {
        let r = r.trim_start_matches(|c: char| c.is_ascii_digit());
        return matches!(r, "" | ".auto");
    }
    if let Some(n) = rest.strip_prefix(".s") {
        return n.len() == 2 && digits(n);
    }
    matches!(rest, ".pocket-auto" | ".pocket-undo" | ".auto.cst" | ".undo.cst")
}

/// pocket-ui's pictures of the game: box art, screenshot and title screen, the frame kept at Quit.
fn pictures_of(system: &str, path: &Path, data: &Path) -> Vec<PathBuf> {
    let mut out = vec![crate::covers::cover_path(data, system, path)];
    out.extend(crate::game_pictures::Kind::ALL.iter().map(|k| crate::game_pictures::path(data, *k, system, path)));
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("unknown");
    out.push(data.join("frames").join(system).join(format!("{stem}.png")));
    out
}

/// The folder a game sits in when it is not a system's folder itself: removed afterwards if nothing is left in it.
fn own_folder(path: &Path, protected: &[PathBuf]) -> Option<PathBuf> {
    let dir = path.parent()?;
    (!protected.iter().any(|p| p.starts_with(dir))).then(|| dir.to_path_buf())
}

/// Bytes of every file under `dir` (links not followed).
fn tree_bytes(dir: &Path) -> u64 {
    let mut total = 0;
    let mut todo = vec![(dir.to_path_buf(), 0)];
    while let Some((d, depth)) = todo.pop() {
        for e in fs::read_dir(&d).into_iter().flatten().flatten() {
            let Ok(m) = e.metadata() else { continue };
            if m.is_dir() && depth < 16 {
                todo.push((e.path(), depth + 1));
            } else if m.is_file() {
                total += m.len();
            }
        }
    }
    total
}

/// Deletes what `plan` lists: the game (a folder with everything in it, except files that look like saves unless
/// `with_saves`), its pictures, and with `with_saves` its saves and states; then its own folder when it is empty.
/// Returns the bytes freed. Stops at the first file that can't be deleted, saying which; what went before is gone.
pub fn delete(plan: &Plan, with_saves: bool, protected: &[PathBuf]) -> Result<u64, String> {
    let mut freed = 0;
    for it in &plan.game {
        if is_protected(&it.path, protected) {
            return Err(format!("{} is a folder on the SD card, not a game. Nothing more was deleted.", it.path.display()));
        }
        let gone = if it.path.is_dir() { remove_folder(&it.path, with_saves) } else { fs::remove_file(&it.path).map(|()| it.bytes) };
        match gone {
            Ok(n) => freed += n,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(format!("{} could not be deleted ({e}).", name(&it.path))),
        }
    }
    if with_saves {
        for it in plan.saves.iter().chain(&plan.states) {
            match fs::remove_file(&it.path) {
                Ok(()) => freed += it.bytes,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(format!("{} could not be deleted ({e}).", name(&it.path))),
            }
        }
    }
    for p in &plan.pictures {
        let _ = fs::remove_file(p);
    }
    if let Some(dir) = &plan.own_folder {
        // Only when empty: remove_dir refuses anything else.
        let _ = fs::remove_dir(dir);
    }
    Ok(freed)
}

/// A game's folder: every file but saves (unless `with_saves`), then the folders left empty. The bytes it freed.
fn remove_folder(dir: &Path, with_saves: bool) -> std::io::Result<u64> {
    if with_saves {
        let bytes = tree_bytes(dir);
        return fs::remove_dir_all(dir).map(|()| bytes);
    }
    let mut freed = 0;
    let mut dirs = vec![dir.to_path_buf()];
    let mut i = 0;
    while i < dirs.len() && dirs.len() < 100_000 {
        for e in fs::read_dir(&dirs[i])?.flatten() {
            let (p, m) = (e.path(), e.metadata()?);
            if m.is_dir() {
                dirs.push(p);
            } else if !is_save(&p) {
                fs::remove_file(&p)?;
                freed += m.len();
            }
        }
        i += 1;
    }
    // Deepest first; a folder with a save left in it stays.
    for d in dirs.iter().rev() {
        let _ = fs::remove_dir(d);
    }
    Ok(freed)
}

fn name(p: &Path) -> String {
    p.file_name().map_or_else(|| p.display().to_string(), |n| n.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;

    struct Tree(crate::testutil::TestDir);

    impl Tree {
        fn new(tag: &str) -> Tree {
            let d = TestDir::new(&format!("gamefiles-{tag}"));
            fs::create_dir_all(&d).unwrap();
            Tree(d)
        }
        fn put(&self, rel: &str, bytes: usize) -> PathBuf {
            let p = self.0.join(rel);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(&p, vec![b'x'; bytes]).unwrap();
            p
        }
        fn text(&self, rel: &str, text: &str) -> PathBuf {
            let p = self.0.join(rel);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(&p, text).unwrap();
            p
        }
        fn has(&self, rel: &str) -> bool {
            self.0.join(rel).exists()
        }
        fn protected(&self) -> Vec<PathBuf> {
            ["roms", "roms/psx", "roms/gba", "roms/dos", "roms/savestates"].iter().map(|r| self.0.join(r)).collect()
        }
    }


    fn names(items: &[Item]) -> Vec<String> {
        items.iter().map(|i| name(&i.path)).collect()
    }

    #[test]
    fn states_are_matched_by_their_exact_ending_never_by_a_longer_name() {
        for n in [
            "Mario.pstate",
            "Mario.pstate3",
            "Mario.pstate.auto",
            "Mario.pstate.auto.png",
            "Mario.pstate.auto.taken",
            "Mario.pstate.auto.taken.retry",
            "Mario.pstate.undo.png",
            "Mario.state",
            "Mario.state2.png",
            "Mario.state.auto",
            "Mario.s01",
            "Mario.pocket-auto",
            "Mario.auto.cst",
        ] {
            assert!(is_state_of(n, "Mario"), "{n}");
        }
        for n in ["Mario. Bros.pstate", "Mario Kart.pstate", "Mario.srm", "Mario.pstatex", "Mario.s1", "Mario.s001", "Mario.png", "Mario"] {
            assert!(!is_state_of(n, "Mario"), "{n}");
        }
    }

    #[test]
    fn a_cue_goes_with_its_tracks_and_an_m3u_with_its_discs_and_their_tracks() {
        let t = Tree::new("multi");
        let cue = t.text("roms/psx/Racer.cue", "FILE \"Racer (Track 1).bin\" BINARY\nFILE \"RACER (TRACK 2).BIN\" BINARY\n");
        t.put("roms/psx/Racer (Track 1).bin", 1000);
        t.put("roms/psx/Racer (Track 2).bin", 500);
        t.put("roms/psx/Other.bin", 70);
        let got: Vec<String> = files_of(&cue).iter().map(|p| name(p)).collect();
        assert_eq!(got, ["Racer.cue", "Racer (Track 1).bin", "Racer (Track 2).bin"], "case ignored, the other game untouched");

        let m3u = t.text("roms/psx/Saga/Saga.m3u", "Saga (Disc 1).cue\n# note\nSaga (Disc 2).chd\nCD3/elsewhere.cue\n");
        t.text("roms/psx/Saga/Saga (Disc 1).cue", "FILE \"Saga (Disc 1).bin\" BINARY\n");
        t.put("roms/psx/Saga/Saga (Disc 1).bin", 300);
        t.put("roms/psx/Saga/Saga (Disc 2).chd", 200);
        let got: Vec<String> = files_of(&m3u).iter().map(|p| name(p)).collect();
        assert_eq!(got, ["Saga.m3u", "Saga (Disc 1).cue", "Saga (Disc 2).chd", "Saga (Disc 1).bin"], "a name with a folder is not followed");
    }

    #[test]
    fn delete_keeps_saves_and_states_unless_asked_and_takes_every_disc() {
        let t = Tree::new("delete");
        let m3u = t.text("roms/psx/Saga/Saga.m3u", "Saga (Disc 1).cue\nSaga (Disc 2).cue\n");
        for d in ["1", "2"] {
            t.text(&format!("roms/psx/Saga/Saga (Disc {d}).cue"), &format!("FILE \"Saga (Disc {d}).bin\" BINARY\n"));
            t.put(&format!("roms/psx/Saga/Saga (Disc {d}).bin"), 1000);
        }
        t.put("roms/psx/Saga/Saga.srm", 8);
        t.put("roms/psx/Saga/Saga.mcr", 128);
        t.put("roms/savestates/psx/Saga.pstate", 50);
        t.put("roms/savestates/psx/Saga.pstate.auto", 60);
        t.put("roms/savestates/psx/Saga.pstate.auto.png", 5);
        t.put("roms/savestates/psx/Saga II.pstate", 50);
        t.put("data/covers/psx/Saga.png", 3);
        let protected = t.protected();
        let places = Places { states: &t.0.join("roms/savestates"), data: &t.0.join("data"), protected: &protected };
        let plan = plan("psx", &m3u, &places).unwrap();
        assert_eq!(plan.game.len(), 5);
        assert_eq!(plan.game_bytes(), 2000 + fs::metadata(&m3u).unwrap().len() + 2 * fs::metadata(t.0.join("roms/psx/Saga/Saga (Disc 1).cue")).unwrap().len());
        assert_eq!(names(&plan.saves), ["Saga.mcr", "Saga.srm"]);
        assert_eq!(names(&plan.states), ["Saga.pstate", "Saga.pstate.auto", "Saga.pstate.auto.png"], "not Saga II's");
        assert_eq!(plan.kept_bytes(), 8 + 128 + 50 + 60 + 5);
        assert_eq!(plan.own_folder.as_deref(), Some(t.0.join("roms/psx/Saga").as_path()));

        let freed = delete(&plan, false, &protected).unwrap();
        assert_eq!(freed, plan.game_bytes());
        for gone in ["roms/psx/Saga/Saga.m3u", "roms/psx/Saga/Saga (Disc 2).bin", "data/covers/psx/Saga.png"] {
            assert!(!t.has(gone), "{gone}");
        }
        for kept in ["roms/psx/Saga/Saga.srm", "roms/psx/Saga/Saga.mcr", "roms/savestates/psx/Saga.pstate", "roms/savestates/psx/Saga.pstate.auto", "roms/savestates/psx/Saga II.pstate"] {
            assert!(t.has(kept), "{kept} kept");
        }
        assert!(t.has("roms/psx/Saga"), "the folder stays while a save is in it");
    }

    #[test]
    fn with_saves_everything_of_the_game_goes_and_its_empty_folder_too() {
        let t = Tree::new("all");
        let cue = t.text("roms/psx/Solo/Solo.cue", "FILE \"Solo.bin\" BINARY\n");
        t.put("roms/psx/Solo/Solo.bin", 400);
        t.put("roms/psx/Solo/Solo.srm", 8);
        t.put("roms/savestates/psx/Solo.state.auto", 30);
        let gba = t.put("roms/gba/Tiny.gba", 100);
        t.put("roms/gba/Tiny.sav", 64);
        t.put("roms/gba/Other.gba", 100);
        let protected = t.protected();
        let places = Places { states: &t.0.join("roms/savestates"), data: &t.0.join("data"), protected: &protected };
        let plan = plan("psx", &cue, &places).unwrap();
        assert_eq!(plan.freed(true), plan.game_bytes() + 8 + 30);
        delete(&plan, true, &protected).unwrap();
        assert!(!t.has("roms/psx/Solo"), "nothing left: the folder goes");
        assert!(!t.has("roms/savestates/psx/Solo.state.auto"));
        assert!(t.has("roms/psx"), "the system's folder stays");

        let plan = super::plan("gba", &gba, &places).unwrap();
        assert_eq!(plan.own_folder, None, "a game in the system's folder leaves the folder alone");
        delete(&plan, false, &protected).unwrap();
        assert!(!t.has("roms/gba/Tiny.gba") && t.has("roms/gba/Tiny.sav") && t.has("roms/gba/Other.gba"));
    }

    #[test]
    fn a_folder_game_goes_whole_but_its_saves_stay_unless_asked() {
        let t = Tree::new("folder");
        t.put("roms/dos/Keen/KEEN.EXE", 900);
        t.put("roms/dos/Keen/data/LEVEL1.DAT", 300);
        t.put("roms/dos/Keen/SAVEGAM1.sav", 10);
        let protected = t.protected();
        let places = Places { states: &t.0.join("roms/savestates"), data: &t.0.join("data"), protected: &protected };
        let plan = plan("dos", &t.0.join("roms/dos/Keen"), &places).unwrap();
        assert_eq!(plan.game_bytes(), 1210);
        assert_eq!(delete(&plan, false, &protected).unwrap(), 1200);
        assert!(!t.has("roms/dos/Keen/KEEN.EXE") && !t.has("roms/dos/Keen/data") && t.has("roms/dos/Keen/SAVEGAM1.sav"));
        assert_eq!(delete(&super::plan("dos", &t.0.join("roms/dos/Keen"), &places).unwrap(), true, &protected).unwrap(), 10);
        assert!(!t.has("roms/dos/Keen") && t.has("roms/dos"));
    }

    #[test]
    fn a_system_folder_or_a_missing_game_is_refused() {
        let t = Tree::new("refuse");
        t.put("roms/gba/A.gba", 1);
        let protected = t.protected();
        let places = Places { states: &t.0.join("roms/savestates"), data: &t.0.join("data"), protected: &protected };
        assert!(plan("gba", &t.0.join("roms/gba"), &places).is_err());
        assert!(plan("gba", &t.0.join("roms"), &places).is_err());
        assert!(plan("gba", &t.0.join("roms/gba/Gone.gba"), &places).unwrap_err().contains("no longer"));
        let bad = Plan { game: vec![Item { path: t.0.join("roms/gba"), bytes: 1 }], ..Default::default() };
        assert!(delete(&bad, true, &protected).is_err());
        assert!(t.has("roms/gba/A.gba"));
    }

    #[test]
    fn saves_and_cards_are_recognised() {
        for p in ["/r/gba/A.srm", "/r/nds/B.SAV", "/r/psx/C.mcr", "/r/ps2/Mcd001.ps2", "/r/bios/GC/MemoryCard A.USA.raw", "/r/bios/GC/USA/01-GALE.gci", "/r/3ds/azahar/sdmc/x.bin"] {
            assert!(is_save(Path::new(p)), "{p}");
        }
        for p in ["/r/gba/A.gba", "/r/ps2/Game.iso", "/r/psx/C.bin", "/r/bios/scph5501.bin"] {
            assert!(!is_save(Path::new(p)), "{p}");
        }
    }
}
