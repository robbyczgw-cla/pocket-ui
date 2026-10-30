//! Recently played games, newest first, stored as JSON in the data directory.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

const KEEP: usize = 20;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Recent {
    pub system: String,
    pub name: String,
    pub path: PathBuf,
    /// Unix seconds of the last session's end.
    pub last_played: u64,
    pub play_secs: u64,
}

pub struct Recents {
    file: PathBuf,
    pub list: Vec<Recent>,
}

pub fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

impl Recents {
    pub fn load(data_dir: &Path) -> Self {
        let file = data_dir.join("recents.json");
        let mut list: Vec<Recent> = crate::fsutil::read_json_or_backup(&file).unwrap_or_default();
        // Names follow the current display rules, also for entries saved by older versions. (The names that tell two
        // versions apart come with the library: `follow_library`.)
        for r in &mut list {
            r.name = plain_name(r);
        }
        Self { file, list }
    }

    /// Gives every entry the name its game has in `systems` (the library), which tells games of one system with the
    /// same name apart ("Tobu Tobu Girl (Europe)" and "(World)", `library::tell_apart`): Continue, the recent-games
    /// list and the game switcher then show what the system list shows for that file, and a game started from them
    /// is started under that name. An entry whose game is not in the library (deleted, other system) goes back to
    /// the plain name of its file. Memory only: no disk, no change to the path every entry is kept by. Call with the
    /// list that was cached or scanned at start, and whenever the library is swapped in. One pass over the games of
    /// the systems the entries are of: nothing a start or a frame would notice.
    pub fn follow_library(&mut self, systems: &[crate::library::System]) {
        use std::collections::HashMap;
        let wanted: HashMap<&Path, usize> = self.list.iter().enumerate().map(|(i, r)| (r.path.as_path(), i)).collect();
        let mut found: Vec<Option<&str>> = vec![None; self.list.len()];
        for sys in systems.iter().filter(|s| self.list.iter().any(|r| r.system == s.name)) {
            for g in &sys.games {
                if let Some(&i) = wanted.get(g.path.as_path()) {
                    if self.list[i].system == sys.name {
                        found[i] = Some(g.name.as_str());
                    }
                }
            }
        }
        let names: Vec<String> = self.list.iter().zip(&found).map(|(r, f)| f.map_or_else(|| plain_name(r), str::to_string)).collect();
        for (r, name) in self.list.iter_mut().zip(names) {
            r.name = name;
        }
    }

    /// Moves the game to the front, adds the session time and saves.
    pub fn record(&mut self, system: &str, name: &str, path: &Path, session_secs: u64) {
        self.note(system, name, path, session_secs);
        self.save();
    }

    /// `record` without the save, which syncs to the card: a game ending in the switcher writes the list after
    /// the next game has started (`save`).
    pub fn note(&mut self, system: &str, name: &str, path: &Path, session_secs: u64) {
        let previous = self.list.iter().position(|r| r.path == path).map(|i| self.list.remove(i));
        self.list.insert(0, Recent {
            system: system.to_string(),
            name: name.to_string(),
            path: path.to_path_buf(),
            last_played: now_secs(),
            play_secs: previous.map_or(0, |r| r.play_secs) + session_secs,
        });
        self.list.truncate(KEEP);
    }

    /// Takes the game off the list (X in the game switcher) and saves; false if it wasn't on it.
    pub fn remove(&mut self, path: &Path) -> bool {
        let before = self.list.len();
        self.list.retain(|r| r.path != path);
        let removed = self.list.len() != before;
        if removed {
            self.save();
        }
        removed
    }

    pub fn save(&self) {
        let _ = crate::fsutil::write_json(&self.file, &self.list);
    }

    /// `save` on the card writer (`fsutil::write_json_soon`): after a game, so the shelf doesn't wait for the card.
    pub fn save_soon(&self) {
        crate::fsutil::write_json_soon(&self.file, &self.list);
    }
}

/// The name a file gets by the display rules alone, without the library to tell it from a twin.
/// (A game that lives in a folder of its own is named after the folder, `library::scan`: that stays.)
fn plain_name(r: &Recent) -> String {
    let folder = r.path.parent().and_then(|p| p.file_name()).and_then(|n| n.to_str()).map(crate::library::shelf_name);
    if folder.as_deref() == Some(r.name.as_str()) {
        return r.name.clone();
    }
    match r.path.file_stem().and_then(|s| s.to_str()) {
        Some(stem) => crate::library::shelf_name(stem),
        None => r.name.clone(),
    }
}

/// "just now", "5 min ago", "3 h ago", "2 days ago".
pub fn ago(then: u64) -> String {
    ago_from(then, now_secs())
}

/// `ago` as seen at `now` (Unix seconds).
pub fn ago_from(then: u64, now: u64) -> String {
    let d = now.saturating_sub(then);
    match d {
        0..=59 => "just now".into(),
        60..=3599 => format!("{} min ago", d / 60),
        3600..=86_399 => format!("{} h ago", d / 3600),
        _ if d < 2 * 86_400 => "yesterday".into(),
        _ => format!("{} days ago", d / 86_400),
    }
}

/// The real time of a moment in the device's zone, for a save that must still be told apart next week: "today 14:02",
/// "yesterday 21:10", "23 Oct 2026, 14:02". (`ago` goes stale: "just now" on a screenshot means nothing later.)
pub fn when(then: u64) -> String {
    let now = now_secs();
    // The zone's offset east of UTC at that moment, from the C library (the device's own zone).
    let t = then as libc::time_t;
    // SAFETY: localtime_r only writes into the struct handed in.
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    let tz = if unsafe { libc::localtime_r(&t, &mut tm) }.is_null() { 0 } else { tm.tm_gmtoff as i32 };
    when_at(then, now, tz)
}

/// `when` for a given `now` and zone (`tz` seconds east of UTC).
pub fn when_at(then: u64, now: u64, tz: i32) -> String {
    use crate::play_history::{civil, local_day};
    const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    let clock = crate::timezone::clock_at(then as i64, tz);
    match local_day(now, tz) - local_day(then, tz) {
        ..=0 => format!("today {clock}"),
        1 => format!("yesterday {clock}"),
        _ => {
            let (y, m, d) = civil(local_day(then, tz));
            format!("{d} {} {y}, {clock}", MONTHS[(m as usize).saturating_sub(1).min(11)])
        }
    }
}

/// "under a minute", "12 min", "1 h 05 min".
pub fn duration(secs: u64) -> String {
    let m = secs / 60;
    match m {
        0 => "under a minute".into(),
        1..=59 => format!("{m} min"),
        _ => format!("{} h {:02} min", m / 60, m % 60),
    }
}

#[cfg(test)]
mod tests {
    use super::Recents;
    use crate::testutil::TestDir;
    use std::path::Path;

    fn data_dir(tag: &str) -> crate::testutil::TestDir {
        TestDir::new(&format!("recents-{tag}"))
    }

    #[test]
    fn a_title_shared_by_two_systems_is_two_entries() {
        // Blockamok Remix exists for the 3DS and the Wii: same title, two files, two systems.
        let dir = data_dir("shared");
        let mut r = Recents::load(&dir);
        r.record("3ds", "BlockamokRemix", Path::new("/storage/roms/3ds/BlockamokRemix.3dsx"), 60);
        r.record("wii", "BlockamokRemix", Path::new("/storage/roms/wii/BlockamokRemix.dol"), 90);
        r.record("wii", "BlockamokRemix", Path::new("/storage/roms/wii/BlockamokRemix.dol"), 30);
        assert_eq!(r.list.len(), 2, "playing the Wii one again moves it, it does not add a third or replace the 3DS one");
        assert_eq!((r.list[0].system.as_str(), r.list[0].play_secs), ("wii", 120));
        assert_eq!((r.list[1].system.as_str(), r.list[1].play_secs), ("3ds", 60));
        assert_eq!(Recents::load(&dir).list.len(), 2, "and both come back from disk");
    }

    #[test]
    fn a_game_named_after_its_folder_keeps_that_name_on_load() {
        // psx/Gran Turismo 2 Plus/GT2.cue is "Gran Turismo 2 Plus"; psx/Pack/Disc1.cue is "Disc1".
        let dir = data_dir("folder");
        let mut r = Recents::load(&dir);
        r.record("psx", "Gran Turismo 2 Plus", Path::new("/storage/roms/psx/Gran Turismo 2 Plus/GT2.cue"), 5);
        r.record("psx", "Disc1", Path::new("/storage/roms/psx/Pack/Disc1.cue"), 5);
        r.record("gba", "Tetris (World)", Path::new("/storage/roms/gba/Tetris (World).gba"), 5);
        let names: Vec<String> = Recents::load(&dir).list.into_iter().map(|e| e.name).collect();
        assert_eq!(names, ["Tetris", "Disc1", "Gran Turismo 2 Plus"], "folder names stay, other names follow the display rules");
    }

    /// A temp ROM tree with `files` (system folder, file name) and the es_systems.cfg for it, read the way the app does.
    fn library_of(tag: &str, files: &[(&str, &str)]) -> (crate::testutil::TestDir, Vec<crate::library::System>) {
        let dir = data_dir(tag);
        let mut systems = String::new();
        for sys in files.iter().map(|f| f.0).collect::<std::collections::BTreeSet<_>>() {
            systems += &format!("<system><name>{sys}</name><path>{}</path><extension>.gb .gba</extension></system>", dir.join(sys).display());
        }
        for (sys, file) in files {
            std::fs::create_dir_all(dir.join(sys)).unwrap();
            std::fs::write(dir.join(sys).join(file), b"x").unwrap();
        }
        let cfg = dir.join("es_systems.cfg");
        std::fs::write(&cfg, format!("<systemList>{systems}</systemList>")).unwrap();
        (dir, crate::library::load(&cfg).unwrap())
    }

    fn names(r: &Recents) -> Vec<&str> {
        r.list.iter().map(|e| e.name.as_str()).collect()
    }

    #[test]
    fn two_versions_of_a_game_in_recents_show_the_names_the_system_list_shows() {
        let files = [("gb", "Tobu Tobu Girl (Europe).gb"), ("gb", "Tobu Tobu Girl (World).gb"), ("gb", "Tetris (World).gb")];
        let (dir, systems) = library_of("twins", &files);
        let games: Vec<&str> = systems[0].games.iter().map(|g| g.name.as_str()).collect();
        assert!(games.contains(&"Tobu Tobu Girl (Europe)") && games.contains(&"Tobu Tobu Girl (World)"), "the system list tells them apart: {games:?}");
        let mut r = Recents::load(&dir);
        for f in files {
            r.note("gb", "whatever", &dir.join(f.0).join(f.1), 60);
        }
        r.save();
        // Start from the library of the cache: the first frame is right.
        let mut r = Recents::load(&dir);
        assert_eq!(names(&r), ["Tetris", "Tobu Tobu Girl", "Tobu Tobu Girl"], "before the library is there: the plain names");
        r.follow_library(&systems);
        assert_eq!(names(&r), ["Tetris", "Tobu Tobu Girl (World)", "Tobu Tobu Girl (Europe)"]);
        assert_eq!(r.list[1].path, dir.join("gb").join("Tobu Tobu Girl (World).gb"), "kept by the file, as before");
        assert_eq!(Recents::load(&dir).list.len(), 3, "and the file on disk still has every entry once");
    }

    #[test]
    fn a_game_whose_twin_was_deleted_goes_back_to_the_plain_name() {
        let both = [("gb", "Tobu Tobu Girl (Europe).gb"), ("gb", "Tobu Tobu Girl (World).gb")];
        let (dir, systems) = library_of("deleted", &both);
        let mut r = Recents::load(&dir);
        r.note("gb", "x", &dir.join("gb").join("Tobu Tobu Girl (Europe).gb"), 5);
        r.follow_library(&systems);
        assert_eq!(names(&r), ["Tobu Tobu Girl (Europe)"]);
        // The (World) one is deleted; the library is read again.
        let (dir2, systems) = library_of("deleted2", &both[..1]);
        r.list[0].path = dir2.join("gb").join("Tobu Tobu Girl (Europe).gb");
        r.follow_library(&systems);
        assert_eq!(names(&r), ["Tobu Tobu Girl"], "alone, it is just the game");
        // A game that is not in the library at all (its file was deleted, or its system is gone) is named by its file.
        r.follow_library(&[]);
        assert_eq!(names(&r), ["Tobu Tobu Girl"], "no library: the plain name");
    }

    #[test]
    fn a_missing_library_leaves_the_plain_names_and_loses_no_entry() {
        let (dir, systems) = library_of("nolib", &[("gb", "Tetris (World).gb")]);
        let mut r = Recents::load(&dir);
        r.note("gb", "Tetris", &dir.join("gb").join("Tetris (World).gb"), 5);
        r.note("gba", "Disc1", Path::new("/storage/roms/gba/Pack/Disc1.gba"), 5);
        r.follow_library(&[]);
        assert_eq!(names(&r), ["Disc1", "Tetris"], "an empty library (not read, or no game): plain names, nothing lost");
        r.follow_library(&systems);
        assert_eq!(names(&r), ["Disc1", "Tetris"], "a game the library does not have keeps its plain name");
        assert_eq!(r.list.len(), 2);
    }

    #[test]
    fn a_save_is_told_by_its_real_time_not_by_how_long_ago() {
        use super::when_at;
        // 2026-10-30 14:00 UTC; the zone is UTC+2.
        let now = 1_793_368_800;
        let tz = 7200;
        assert_eq!(when_at(now - 30, now, tz), "today 15:59");
        assert_eq!(when_at(now - 3 * 3600, now, tz), "today 13:00");
        assert_eq!(when_at(now - 20 * 3600, now, tz), "yesterday 20:00");
        assert_eq!(when_at(now - 7 * 86_400, now, tz), "23 Oct 2026, 16:00");
        // Just after midnight in the zone, 23:30 the evening before is yesterday, though under an hour ago.
        let night = 1_793_313_000; // 2026-10-30 00:30 local
        assert_eq!(when_at(night - 3600, night, tz), "yesterday 23:30");
    }
}
