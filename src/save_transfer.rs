//! Saves out of the Android app and back (Settings → Saves; docs/ANDROID-MVP.md, "Saves out and back").
//!
//! The app's saves are private to it: `saves/<key>/` (what the game itself saves) and `states/<key>/` (moments, slots and
//! their pictures), `<key>` being the game's identity in the chosen folder (`ui::game_key`). Android removes them with
//! the app. A backup is one zip the player puts where they like: those two folders as they are, and
//! [`MANIFEST`], which says what each key was (console and file name) and when each file was written.
//!
//! Android's side only moves bytes (`PocketActivity.exportSaves`, `importSaves`); what goes in, what comes back and where
//! it lands is decided here, so it is tested on Linux:
//! - [`export_plan`]: every file of every game that has any.
//! - [`read_backup`]: what a picked zip holds. A game is found again by its key, or, on another phone or card where the
//!   folder's identity differs, by console and file name when exactly one game here fits. Otherwise it keeps its key and
//!   waits for its folder.
//! - [`Backup::plan`]: a game without saves here gets the backup's. A game that has saves here keeps them unless the
//!   player says to use the backup's, and then all of that game's files here go first, so two sets never mix.
//! - File times come back as they were: a moment older than the in-game save is not resumed (`frozen`), so their order
//!   has to survive.
//!
//! Names from a zip are data: only `saves/<16 hex>/…` and `states/<16 hex>/…` with plain path parts are taken.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

/// The zip's entry that says what it holds.
pub const MANIFEST: &str = "pocket-ui-saves.json";
/// The manifest's format. A newer one is refused rather than half understood.
pub const FORMAT: u32 = 1;
/// The folders a backup holds, under the app's data.
pub const FOLDERS: [&str; 2] = ["saves", "states"];
const MAX_FILES: usize = 20_000;
const MAX_DEPTH: usize = 8;

pub const NOT_A_BACKUP: &str = "This file is not a pocket-ui save backup.";
pub const NEWER_BACKUP: &str = "This backup was made by a newer pocket-ui. Update the app first.";
pub const EMPTY_BACKUP: &str = "This backup holds no saves.";
pub const NOTHING_YET: &str = "No saves yet. They appear here after you have played.";

/// A game of the library under one of its keys (Game Boy games have a second one for the other core).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Known {
    pub key: String,
    pub system: String,
    /// The game's file name in its folder.
    pub file: String,
    pub title: String,
    /// "" or the other core's name: its saves are kept apart.
    pub core: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct GameEntry {
    pub key: String,
    pub system: String,
    pub file: String,
    pub title: String,
    pub core: String,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Manifest {
    pub format: u32,
    pub app: String,
    /// Unix seconds.
    pub made: u64,
    pub games: Vec<GameEntry>,
    /// Each file's time of writing in Unix milliseconds, by its name in the zip.
    pub files: BTreeMap<String, u64>,
}

/// What an export writes: the manifest's text and each file with its name in the zip.
#[derive(Debug, Default)]
pub struct ExportPlan {
    pub manifest: String,
    pub files: Vec<(String, PathBuf)>,
    pub games: usize,
    pub bytes: u64,
}

impl ExportPlan {
    /// For `PocketActivity.exportSaves`: `{"manifest": "...", "files": [["saves/…", "/data/…"], …]}`.
    pub fn json(&self) -> String {
        serde_json::json!({
            "manifest": self.manifest,
            "files": self.files.iter().map(|(name, path)| (name.clone(), path.to_string_lossy().into_owned())).collect::<Vec<_>>(),
        })
        .to_string()
    }
}

/// `0123456789abcdef`: a game's folder name.
pub(crate) fn is_key(name: &str) -> bool {
    name.len() == 16 && name.bytes().all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}

/// A half-written or temporary file: never part of a backup.
pub(crate) fn is_scrap(name: &str) -> bool {
    name.ends_with(".tmp") || name.ends_with(".part") || name.contains(".tmp.")
}

/// Every regular file under `dir`, as path parts below it. Links are not followed.
pub(crate) fn files_under(dir: &Path, parts: &mut Vec<String>, found: &mut Vec<(Vec<String>, PathBuf)>) {
    if parts.len() >= MAX_DEPTH || found.len() >= MAX_FILES {
        return;
    }
    let Ok(read) = std::fs::read_dir(dir) else { return };
    let mut entries: Vec<_> = read.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let Some(name) = entry.file_name().to_str().map(str::to_string) else { continue };
        let Ok(kind) = entry.file_type() else { continue };
        if kind.is_dir() {
            parts.push(name);
            files_under(&entry.path(), parts, found);
            parts.pop();
        } else if kind.is_file() && !is_scrap(&name) && found.len() < MAX_FILES {
            let mut all = parts.clone();
            all.push(name);
            found.push((all, entry.path()));
        }
    }
}

/// The keys that have at least one file under `saves/` or `states/`, each with its files (zip name, path).
fn saved(data: &Path) -> BTreeMap<String, Vec<(String, PathBuf)>> {
    let mut games: BTreeMap<String, Vec<(String, PathBuf)>> = BTreeMap::new();
    for folder in FOLDERS {
        let Ok(read) = std::fs::read_dir(data.join(folder)) else { continue };
        for entry in read.flatten() {
            let Some(key) = entry.file_name().to_str().map(str::to_string) else { continue };
            if !is_key(&key) || !entry.file_type().is_ok_and(|t| t.is_dir()) {
                continue;
            }
            let mut found = Vec::new();
            files_under(&entry.path(), &mut Vec::new(), &mut found);
            for (parts, path) in found {
                games.entry(key.clone()).or_default().push((format!("{folder}/{key}/{}", parts.join("/")), path));
            }
        }
    }
    games
}

/// How many games have saves here: the Settings row's value.
pub fn games_with_saves(data: &Path) -> usize {
    saved(data).len()
}

pub(crate) fn written_ms(path: &Path) -> u64 {
    std::fs::metadata(path).and_then(|m| m.modified()).ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map_or(0, |d| d.as_millis() as u64)
}

/// Everything a backup of this phone holds. `now` is Unix seconds.
pub fn export_plan(data: &Path, known: &[Known], now: u64) -> ExportPlan {
    let games = saved(data);
    let mut manifest = Manifest { format: FORMAT, app: "pocket-ui".into(), made: now, ..Default::default() };
    let mut plan = ExportPlan { games: games.len(), ..Default::default() };
    for (key, files) in games {
        let entry = known.iter().find(|k| k.key == key).map_or_else(
            || GameEntry { key: key.clone(), ..Default::default() },
            |k| GameEntry { key: key.clone(), system: k.system.clone(), file: k.file.clone(), title: k.title.clone(), core: k.core.clone() },
        );
        manifest.games.push(entry);
        for (name, path) in files {
            manifest.files.insert(name.clone(), written_ms(&path));
            plan.bytes += std::fs::metadata(&path).map_or(0, |m| m.len());
            plan.files.push((name, path));
        }
    }
    plan.manifest = serde_json::to_string_pretty(&manifest).unwrap_or_default();
    plan
}

/// `saves/<key>/a/b.srm` as (folder, key, rest), when every part is a plain name.
fn entry_parts(name: &str) -> Option<(&'static str, &str, &str)> {
    if name.len() > 512 || name.contains(['\\', '\0']) {
        return None;
    }
    let mut parts = name.splitn(3, '/');
    let first = parts.next()?;
    let folder = FOLDERS.into_iter().find(|f| *f == first)?;
    let key = parts.next().filter(|k| is_key(k))?;
    let rest = parts.next()?;
    let plain = |part: &str| !part.is_empty() && part != "." && part != ".." && !is_scrap(part);
    (rest.split('/').count() <= MAX_DEPTH && rest.split('/').all(plain)).then_some((folder, key, rest))
}

/// One file to take out of the zip: its entry, where it goes under the app's data, and its time of writing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Take {
    pub entry: String,
    pub to: String,
    pub written_ms: u64,
}

/// What a picked zip holds, seen from this phone.
#[derive(Debug, Default)]
pub struct Backup {
    /// Per game of the backup: the key its files go to here, and its files.
    games: BTreeMap<String, Vec<Take>>,
    /// The keys (here) that already have saves on this phone.
    here: BTreeSet<String>,
}

/// What to do with a backup: `PocketActivity.importApply` does exactly this.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ImportPlan {
    pub take: Vec<Take>,
    /// Folders under the app's data that go before the backup's files move in (a game's whole set here).
    pub remove: Vec<String>,
    /// Folders that move in, each as a whole.
    pub place: Vec<String>,
    /// Games that get the backup's saves, and games that keep the phone's.
    pub restored: usize,
    pub kept: usize,
}

impl ImportPlan {
    pub fn json(&self) -> String {
        serde_json::json!({
            "take": self.take.iter().map(|t| serde_json::json!({"entry": t.entry, "to": t.to, "written": t.written_ms})).collect::<Vec<_>>(),
            "remove": self.remove,
            "place": self.place,
        })
        .to_string()
    }
    /// What happened, for the player.
    pub fn sentence(&self) -> String {
        let back = match self.restored {
            0 => String::new(),
            1 => "1 game has its saves back.".to_string(),
            n => format!("{n} games have their saves back."),
        };
        let kept = match self.kept {
            0 => String::new(),
            1 => "1 game kept the saves from this phone.".to_string(),
            n => format!("{n} games kept the saves from this phone."),
        };
        [back, kept].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join("\n")
    }
}

/// Reads a backup's manifest and entry names. Err is a sentence for the player.
pub fn read_backup(data: &Path, manifest: Option<&str>, names: &[String], known: &[Known]) -> Result<Backup, &'static str> {
    let manifest: Manifest = manifest.and_then(|m| serde_json::from_str(m).ok()).ok_or(NOT_A_BACKUP)?;
    if manifest.app != "pocket-ui" || manifest.format == 0 {
        return Err(NOT_A_BACKUP);
    }
    if manifest.format > FORMAT {
        return Err(NEWER_BACKUP);
    }
    let local: BTreeSet<&str> = known.iter().map(|k| k.key.as_str()).collect();
    let mut by_key: BTreeMap<String, Vec<(String, &'static str, String)>> = BTreeMap::new();
    for name in names.iter().take(MAX_FILES) {
        if let Some((folder, key, rest)) = entry_parts(name) {
            by_key.entry(key.to_string()).or_default().push((name.clone(), folder, rest.to_string()));
        }
    }
    if by_key.is_empty() {
        return Err(EMPTY_BACKUP);
    }
    // Where each game of the backup lives here. Its own key first; a key of this phone is never given to two games.
    let mut target: BTreeMap<String, String> = BTreeMap::new();
    let mut taken: BTreeSet<String> = BTreeSet::new();
    for key in by_key.keys().filter(|k| local.contains(k.as_str())) {
        target.insert(key.clone(), key.clone());
        taken.insert(key.clone());
    }
    for key in by_key.keys() {
        if target.contains_key(key) {
            continue;
        }
        let entry = manifest.games.iter().find(|g| &g.key == key);
        let fits: Vec<&Known> = entry
            .filter(|e| !e.system.is_empty() && !e.file.is_empty())
            .map(|e| known.iter().filter(|k| k.system == e.system && k.file == e.file && k.core == e.core).collect())
            .unwrap_or_default();
        let to = match fits.as_slice() {
            [one] if !taken.contains(&one.key) && !by_key.contains_key(&one.key) => one.key.clone(),
            _ => key.clone(),
        };
        taken.insert(to.clone());
        target.insert(key.clone(), to);
    }
    let have = saved(data);
    let mut backup = Backup::default();
    for (key, files) in by_key {
        let to = &target[&key];
        if have.contains_key(to) {
            backup.here.insert(to.clone());
        }
        let takes = files
            .into_iter()
            .map(|(entry, folder, rest)| Take { written_ms: manifest.files.get(&entry).copied().unwrap_or(0), to: format!("{folder}/{to}/{rest}"), entry })
            .collect();
        backup.games.insert(to.clone(), takes);
    }
    Ok(backup)
}

impl Backup {
    /// Games in the backup.
    pub fn games(&self) -> usize {
        self.games.len()
    }
    /// Games of the backup that already have saves on this phone.
    pub fn here(&self) -> usize {
        self.here.len()
    }
    /// `replace`: the backup's saves also for the games that have saves here.
    pub fn plan(&self, replace: bool) -> ImportPlan {
        let mut plan = ImportPlan::default();
        for (key, takes) in &self.games {
            let here = self.here.contains(key);
            if here && !replace {
                plan.kept += 1;
                continue;
            }
            plan.restored += 1;
            if here {
                plan.remove.extend(FOLDERS.iter().map(|f| format!("{f}/{key}")));
            }
            for folder in FOLDERS {
                if takes.iter().any(|t| t.to.starts_with(&format!("{folder}/{key}/"))) {
                    plan.place.push(format!("{folder}/{key}"));
                }
            }
            plan.take.extend(takes.iter().cloned());
        }
        plan
    }
}

/// `pocket-ui-saves-2026-11-09.zip`: the name the export suggests (`days` since 1970, local).
pub fn file_name(days: i64) -> String {
    let (y, m, d) = crate::civil_date::civil(days);
    format!("pocket-ui-saves-{y:04}-{m:02}-{d:02}.zip")
}

/// What an export says when it is done.
pub fn exported_sentence(games: usize) -> String {
    if games == 1 { "Backup written: the saves of 1 game.".to_string() } else { format!("Backup written: the saves of {games} games.") }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ZELDA: &str = "00000000000000aa";
    const MARIO: &str = "00000000000000bb";
    const TETRIS: &str = "00000000000000cc";

    fn write(data: &Path, name: &str, bytes: &[u8], written_s: u64) {
        let path = data.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, bytes).unwrap();
        let file = std::fs::File::options().write(true).open(&path).unwrap();
        file.set_modified(UNIX_EPOCH + std::time::Duration::from_secs(written_s)).unwrap();
    }
    fn known(key: &str, system: &str, file: &str) -> Known {
        Known { key: key.into(), system: system.into(), file: file.into(), title: file.into(), core: String::new() }
    }
    /// A phone on which Zelda (in-game save, moment, slot with its picture) and Mario (a moment) were played.
    fn played() -> (tempfile::TempDir, Vec<Known>) {
        let d = tempfile::tempdir().unwrap();
        write(d.path(), &format!("saves/{ZELDA}/Zelda.srm"), b"hearts", 1_000);
        write(d.path(), &format!("states/{ZELDA}/Zelda.pstate.auto.z"), b"moment", 2_000);
        write(d.path(), &format!("states/{ZELDA}/Zelda.pstate1"), b"slot one", 1_500);
        write(d.path(), &format!("states/{ZELDA}/Zelda.pstate1.png"), b"picture", 1_500);
        write(d.path(), &format!("states/{MARIO}/Mario.pstate.auto.z"), b"world 4", 3_000);
        // What a start leaves for a game that was never saved, and what a kill leaves mid-write.
        std::fs::create_dir_all(d.path().join(format!("saves/{TETRIS}"))).unwrap();
        write(d.path(), &format!("states/{MARIO}/Mario.pstate2.tmp"), b"half", 3_000);
        (d, vec![known(ZELDA, "gba", "Zelda.gba"), known(MARIO, "snes", "Mario.sfc"), known(TETRIS, "gb", "Tetris.gb")])
    }
    /// What Android does with a plan, on plain files: the zip is `from`'s tree, the phone is `to`.
    fn apply(plan: &ImportPlan, zip: &BTreeMap<String, Vec<u8>>, to: &Path) {
        let part = to.join("import.part");
        for take in &plan.take {
            let path = part.join(&take.to);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, &zip[&take.entry]).unwrap();
            std::fs::File::options().write(true).open(&path).unwrap().set_modified(UNIX_EPOCH + std::time::Duration::from_millis(take.written_ms)).unwrap();
        }
        for dir in &plan.remove {
            let _ = std::fs::remove_dir_all(to.join(dir));
        }
        for dir in &plan.place {
            let _ = std::fs::remove_dir_all(to.join(dir));
            std::fs::create_dir_all(to.join(dir).parent().unwrap()).unwrap();
            std::fs::rename(part.join(dir), to.join(dir)).unwrap();
        }
        let _ = std::fs::remove_dir_all(part);
    }
    /// The zip an export of `data` makes: every planned file by its name, and the manifest.
    fn zipped(data: &Path, games: &[Known]) -> (String, BTreeMap<String, Vec<u8>>) {
        let plan = export_plan(data, games, 1_762_000_000);
        (plan.manifest.clone(), plan.files.iter().map(|(name, path)| (name.clone(), std::fs::read(path).unwrap())).collect())
    }
    fn names(zip: &BTreeMap<String, Vec<u8>>) -> Vec<String> {
        zip.keys().cloned().collect()
    }
    fn read(data: &Path, name: &str) -> Option<Vec<u8>> {
        std::fs::read(data.join(name)).ok()
    }

    #[test]
    fn saves_survive_uninstalling_the_app_when_they_were_exported_and_imported() {
        let (phone, games) = played();
        let (manifest, zip) = zipped(phone.path(), &games);
        assert_eq!(games_with_saves(phone.path()), 2, "Zelda and Mario have saves; Tetris was only started");
        // Uninstalled and installed again: nothing is left, the same folder is chosen again.
        let fresh = tempfile::tempdir().unwrap();
        let backup = read_backup(fresh.path(), Some(&manifest), &names(&zip), &games).unwrap();
        assert_eq!((backup.games(), backup.here()), (2, 0));
        let plan = backup.plan(false);
        apply(&plan, &zip, fresh.path());
        assert_eq!(plan.sentence(), "2 games have their saves back.");
        for (name, bytes) in [
            (format!("saves/{ZELDA}/Zelda.srm"), &b"hearts"[..]),
            (format!("states/{ZELDA}/Zelda.pstate.auto.z"), b"moment"),
            (format!("states/{ZELDA}/Zelda.pstate1"), b"slot one"),
            (format!("states/{ZELDA}/Zelda.pstate1.png"), b"picture"),
            (format!("states/{MARIO}/Mario.pstate.auto.z"), b"world 4"),
        ] {
            assert_eq!(read(fresh.path(), &name).as_deref(), Some(bytes), "{name}");
        }
        assert!(!fresh.path().join(format!("states/{MARIO}/Mario.pstate2.tmp")).exists(), "a half-written file is not a save");
        assert!(!fresh.path().join("import.part").exists());
    }

    #[test]
    fn a_moment_stays_newer_than_the_in_game_save_it_was_made_after() {
        // `frozen` doesn't go back to a moment that is older than the in-game save: their order has to come back too.
        let (phone, games) = played();
        let (manifest, zip) = zipped(phone.path(), &games);
        let fresh = tempfile::tempdir().unwrap();
        apply(&read_backup(fresh.path(), Some(&manifest), &names(&zip), &games).unwrap().plan(false), &zip, fresh.path());
        let at = |name: String| std::fs::metadata(fresh.path().join(name)).unwrap().modified().unwrap().duration_since(UNIX_EPOCH).unwrap().as_secs();
        assert_eq!(at(format!("saves/{ZELDA}/Zelda.srm")), 1_000);
        assert_eq!(at(format!("states/{ZELDA}/Zelda.pstate.auto.z")), 2_000);
    }

    #[test]
    fn a_game_played_since_keeps_its_saves_unless_the_player_takes_the_backups() {
        let (phone, games) = played();
        let (manifest, zip) = zipped(phone.path(), &games);
        // Installed again, and Zelda was played for a while before the backup was remembered.
        let later = tempfile::tempdir().unwrap();
        write(later.path(), &format!("saves/{ZELDA}/Zelda.srm"), b"new hearts", 9_000);
        write(later.path(), &format!("states/{ZELDA}/Zelda.pstate3"), b"new slot", 9_000);
        let backup = read_backup(later.path(), Some(&manifest), &names(&zip), &games).unwrap();
        assert_eq!((backup.games(), backup.here()), (2, 1), "the page asks about the one game");
        let keep = backup.plan(false);
        assert_eq!((keep.restored, keep.kept), (1, 1));
        assert_eq!(keep.sentence(), "1 game has its saves back.\n1 game kept the saves from this phone.");
        apply(&keep, &zip, later.path());
        assert_eq!(read(later.path(), &format!("saves/{ZELDA}/Zelda.srm")).as_deref(), Some(&b"new hearts"[..]), "what was played since is not overwritten");
        assert_eq!(read(later.path(), &format!("states/{MARIO}/Mario.pstate.auto.z")).as_deref(), Some(&b"world 4"[..]));
        assert!(read(later.path(), &format!("states/{ZELDA}/Zelda.pstate1")).is_none(), "no file of the backup is mixed into a game that keeps its own");
        // The other answer: the backup's, whole. Nothing of the phone's set stays beside it.
        let backup = read_backup(later.path(), Some(&manifest), &names(&zip), &games).unwrap();
        let replace = backup.plan(true);
        assert_eq!((replace.restored, replace.kept), (2, 0));
        apply(&replace, &zip, later.path());
        assert_eq!(read(later.path(), &format!("saves/{ZELDA}/Zelda.srm")).as_deref(), Some(&b"hearts"[..]));
        assert_eq!(read(later.path(), &format!("states/{ZELDA}/Zelda.pstate1")).as_deref(), Some(&b"slot one"[..]));
        assert!(read(later.path(), &format!("states/{ZELDA}/Zelda.pstate3")).is_none(), "the phone's newer slot would sit beside an older in-game save");
    }

    #[test]
    fn on_another_phone_a_game_is_found_again_by_its_console_and_file_name() {
        let (phone, games) = played();
        let (manifest, zip) = zipped(phone.path(), &games);
        // Another phone: the same files in another folder have other identities there. Mario isn't on it.
        let other = tempfile::tempdir().unwrap();
        let there = vec![known("11111111111111aa", "gba", "Zelda.gba"), known("11111111111111dd", "nes", "Zelda.gba")];
        let backup = read_backup(other.path(), Some(&manifest), &names(&zip), &there).unwrap();
        apply(&backup.plan(false), &zip, other.path());
        assert_eq!(read(other.path(), "saves/11111111111111aa/Zelda.srm").as_deref(), Some(&b"hearts"[..]), "Zelda's saves are under the key it has here");
        assert_eq!(read(other.path(), "states/11111111111111aa/Zelda.pstate1").as_deref(), Some(&b"slot one"[..]));
        assert!(!other.path().join("saves/11111111111111dd").exists(), "a file of the same name on another console is another game");
        assert_eq!(read(other.path(), &format!("states/{MARIO}/Mario.pstate.auto.z")).as_deref(), Some(&b"world 4"[..]), "a game that isn't here yet waits under its own key");
        // Two games here with that name on that console: nobody can say which, so it waits too.
        let twins = tempfile::tempdir().unwrap();
        let two = vec![known("11111111111111aa", "gba", "Zelda.gba"), known("22222222222222aa", "gba", "Zelda.gba")];
        apply(&read_backup(twins.path(), Some(&manifest), &names(&zip), &two).unwrap().plan(false), &zip, twins.path());
        assert!(twins.path().join(format!("saves/{ZELDA}/Zelda.srm")).exists());
        assert!(!twins.path().join("saves/11111111111111aa").exists() && !twins.path().join("saves/22222222222222aa").exists());
    }

    #[test]
    fn the_other_game_boy_cores_saves_stay_apart() {
        let d = tempfile::tempdir().unwrap();
        write(d.path(), &format!("saves/{ZELDA}/Tetris.srm"), b"mgba", 1_000);
        write(d.path(), &format!("saves/{MARIO}/Tetris.srm"), b"gambatte", 1_000);
        let here = vec![known(ZELDA, "gb", "Tetris.gb"), Known { core: "gambatte".into(), ..known(MARIO, "gb", "Tetris.gb") }];
        let (manifest, zip) = zipped(d.path(), &here);
        let other = tempfile::tempdir().unwrap();
        let there = vec![known("11111111111111aa", "gb", "Tetris.gb"), Known { core: "gambatte".into(), ..known("11111111111111bb", "gb", "Tetris.gb") }];
        apply(&read_backup(other.path(), Some(&manifest), &names(&zip), &there).unwrap().plan(false), &zip, other.path());
        assert_eq!(read(other.path(), "saves/11111111111111aa/Tetris.srm").as_deref(), Some(&b"mgba"[..]));
        assert_eq!(read(other.path(), "saves/11111111111111bb/Tetris.srm").as_deref(), Some(&b"gambatte"[..]));
    }

    #[test]
    fn a_zip_cannot_write_outside_the_two_save_folders() {
        let manifest = serde_json::to_string(&Manifest { format: FORMAT, app: "pocket-ui".into(), ..Default::default() }).unwrap();
        let wicked: Vec<String> = [
            "saves/00000000000000aa/../../launcher.json",
            "saves/00000000000000aa/../00000000000000bb/x.srm",
            "/data/data/xyz.pocketui.app/files/launcher.json",
            "states/../launcher.json",
            "launcher.json",
            "content/00000000000000aa/game.gba",
            "saves/not-a-key/x.srm",
            "saves/00000000000000AA/x.srm",
            "saves/00000000000000aa/",
            "saves/00000000000000aa/a\\..\\b",
            "saves/00000000000000aa//x.srm",
            "saves/00000000000000aa/./x.srm",
            "saves/00000000000000aa/good.srm",
        ]
        .map(String::from)
        .to_vec();
        let d = tempfile::tempdir().unwrap();
        let plan = read_backup(d.path(), Some(&manifest), &wicked, &[]).unwrap().plan(true);
        assert_eq!(plan.take.iter().map(|t| t.to.as_str()).collect::<Vec<_>>(), ["saves/00000000000000aa/good.srm"]);
        assert_eq!(plan.place, ["saves/00000000000000aa"]);
        assert!(plan.remove.is_empty());
    }

    #[test]
    fn a_file_that_is_no_backup_is_refused_with_a_sentence() {
        let d = tempfile::tempdir().unwrap();
        let some = vec![format!("saves/{ZELDA}/Zelda.srm")];
        assert_eq!(read_backup(d.path(), None, &some, &[]).unwrap_err(), NOT_A_BACKUP, "a zip of something else");
        assert_eq!(read_backup(d.path(), Some("{}"), &some, &[]).unwrap_err(), NOT_A_BACKUP);
        assert_eq!(read_backup(d.path(), Some("not json"), &some, &[]).unwrap_err(), NOT_A_BACKUP);
        assert_eq!(read_backup(d.path(), Some(r#"{"format":2,"app":"pocket-ui"}"#), &some, &[]).unwrap_err(), NEWER_BACKUP);
        assert_eq!(read_backup(d.path(), Some(r#"{"format":1,"app":"pocket-ui"}"#), &[], &[]).unwrap_err(), EMPTY_BACKUP);
        // A phone nothing was played on has nothing to write, and says so instead of making an empty file.
        assert_eq!(export_plan(d.path(), &[], 0).games, 0);
        assert_eq!(games_with_saves(d.path()), 0);
    }

    #[test]
    fn the_backup_is_named_by_its_day() {
        assert_eq!(file_name(20_401), "pocket-ui-saves-2025-11-09.zip");
        assert_eq!(exported_sentence(1), "Backup written: the saves of 1 game.");
        assert_eq!(exported_sentence(12), "Backup written: the saves of 12 games.");
    }
}
