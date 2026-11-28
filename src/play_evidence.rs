//! One meaning of "ever played": recorded use, any save/state belonging to the game, or known save progress.
//! File names are indexed on a worker, once per folder, never from a paint call. No time or date is invented.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, Default)]
pub struct Evidence {
    pub recorded: bool,
    pub files: bool,
    pub progress: bool,
}

impl Evidence {
    pub fn ever_played(self) -> bool { self.recorded || self.files || self.progress }

    /// No measured time here; a save or other evidence still means this is not a fresh game.
    pub fn untimed(self) -> &'static str {
        if self.ever_played() { "Not here yet" } else { "Not played yet" }
    }
}

/// A scan's directory listings. A 5,000-game system lists and indexes each shared folder once.
#[cfg(any(target_os = "linux", target_os = "emscripten", all(target_os = "android", feature = "handheld")))]
#[derive(Default)]
pub struct Files {
    dirs: HashMap<PathBuf, Folder>,
}

#[cfg(any(target_os = "linux", target_os = "emscripten", all(target_os = "android", feature = "handheld")))]
#[derive(Default)]
struct Folder {
    any: bool,
    directories: Vec<PathBuf>,
    saves: HashSet<String>,
    states: HashSet<String>,
}

#[cfg(any(target_os = "linux", target_os = "emscripten", all(target_os = "android", feature = "handheld")))]
impl Files {
    fn folder(&mut self, dir: &Path) -> &Folder {
        self.dirs.entry(dir.to_path_buf()).or_insert_with(|| {
            let mut out = Folder::default();
            for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
                let Ok(kind) = entry.file_type() else { continue };
                let path = entry.path();
                if kind.is_dir() { out.directories.push(path); continue; }
                if !kind.is_file() && !kind.is_symlink() { continue; }
                out.any = true;
                if path.extension().and_then(|s| s.to_str()).is_some_and(|e| crate::game_files::SAVE_EXTS.iter().any(|ext| e.eq_ignore_ascii_case(ext))) {
                    if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) { out.saves.insert(stem.to_lowercase()); }
                }
                if let Some(name) = path.file_name().and_then(|s| s.to_str()).filter(|n| !n.ends_with(".png")) {
                    if let Some(base) = crate::timeline::base_of(name) { out.states.insert(base.to_string()); }
                    for (at, _) in name.match_indices('.') {
                        let base = &name[..at];
                        let kept = name[at..].strip_prefix(".pstate.auto.z.kept").or_else(|| name[at..].strip_prefix(".pstate.auto.kept"));
                        if crate::game_files::is_state_of(name, base) || kept.is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit())) {
                            out.states.insert(base.to_string());
                        }
                    }
                }
            }
            out
        })
    }

    fn any_private_file(&mut self, dir: &Path) -> bool {
        let folder = self.folder(dir);
        if folder.any { return true; }
        let children = folder.directories.clone();
        children.iter().any(|child| self.any_private_file(child))
    }

    /// Explicit base permits the Android resolver's game-private directory as well as Linux's shared folder.
    pub fn of(&mut self, system: &str, rom: &Path, base: &Path) -> bool {
        let (Some(stem), Some(file)) = (rom.file_stem().and_then(|s| s.to_str()), rom.file_name().and_then(|s| s.to_str())) else { return false };
        let private = base.parent().filter(|key| key.parent().is_some_and(|p| p.file_name().is_some_and(|n| n == "states")));
        if let Some(key) = private {
            if let Some(player) = key.parent().and_then(Path::parent) {
                // Every actual file in this game-private save folder belongs to it, including a core's memory card.
                if self.any_private_file(&player.join("saves").join(key.file_name().unwrap())) { return true; }
            }
        }
        let mut saves = vec![crate::showcase::save_dir(system, rom)];
        if let Some(dir) = rom.parent() { saves.push(dir.to_path_buf()); }
        if saves.into_iter().any(|dir| self.folder(&dir).saves.contains(&stem.to_lowercase())) { return true; }
        let mut states = vec![base.parent().unwrap_or(Path::new("")).to_path_buf()];
        if let Some(dir) = rom.parent() { states.push(dir.to_path_buf()); }
        states.into_iter().any(|dir| {
            let folder = self.folder(&dir);
            folder.states.contains(stem) || folder.states.contains(file)
        })
    }
}

#[cfg(any(target_os = "linux", target_os = "emscripten", all(target_os = "android", feature = "handheld")))]
pub fn scan(systems: &[crate::library::System]) -> HashSet<PathBuf> {
    let mut files = Files::default();
    // Each game as its folder's system (`region_pairs`: a game in `genesis/` keeps its states there).
    systems.iter().flat_map(|s| s.games.iter().map(move |g| (s.own_id(&g.path), &g.path)))
        .filter_map(|(system, rom)| {
            let base = crate::slots::base(system, rom.file_stem().and_then(|s| s.to_str()).unwrap_or_default());
            (files.of(system, rom, &base) || (system == "nds" && files.of(system, rom, &crate::ds::state_base(&base, "dsperate")))).then(|| rom.clone())
        }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saves_states_clocks_and_memory_cards_are_play_evidence_without_recents() {
        for name in ["Game.srm", "Game.SAV", "Game.eep", "Game.rtc", "Game.mcr", "Game.mcd", "Game.state", "Game.state3", "Game.pstate7", "Game.pstate.earlier2", "Game.pstate.started1", "Game.pstate.auto.z.kept123", "Game.pstate.auto.z.taken", "Game.sfc.s01"] {
            let d = crate::testutil::TestDir::new("play-evidence");
            let rom = d.join("Game.sfc");
            std::fs::write(&rom, b"rom").unwrap();
            let base = d.join("states/snes/Game");
            std::fs::create_dir_all(base.parent().unwrap()).unwrap();
            let dir = if name.contains("state") || name.ends_with("s01") { base.parent().unwrap() } else { &d };
            std::fs::write(dir.join(name), if name.ends_with("rtc") { &b""[..] } else { &b"save"[..] }).unwrap();
            let evidence = Evidence { files: Files::default().of("snes", &rom, &base), ..Default::default() };
            assert!(evidence.ever_played(), "{name}");
            assert_ne!(evidence.untimed(), "Not played yet", "{name}");
        }
    }

    #[test]
    fn a_fresh_game_and_unrelated_saves_or_preview_alone_are_still_fresh() {
        let d = crate::testutil::TestDir::new("fresh-evidence");
        let rom = d.join("Game.sfc");
        for name in ["Other.srm", "Game Two.state", "Game.pstate.png"] { std::fs::write(d.join(name), b"other").unwrap(); }
        assert!(!Files::default().of("snes", &rom, &d.join("states/snes/Game")));
        assert!(!Evidence::default().ever_played());
        assert_eq!(Evidence::default().untimed(), "Not played yet");
        assert!(Evidence { progress: true, ..Default::default() }.ever_played());
    }

    #[test]
    fn android_private_save_and_state_folders_count_without_history() {
        for folder in ["saves", "states", "saves/nested"] {
            let d = crate::testutil::TestDir::new("private-evidence");
            let base = d.join("states/0123456789abcdef/Game");
            let dir = if folder == "saves/nested" { d.join("saves/0123456789abcdef/core/cards") } else { d.join(format!("{folder}/0123456789abcdef")) };
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join(if folder.starts_with("saves") { "memory-card.mcd" } else { "Game.pstate1" }), b"save").unwrap();
            assert!(Files::default().of("psx", &d.join("roms/Game.chd"), &base), "{folder}");
        }
    }
}
