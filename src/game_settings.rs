//! Where a game's settings live, for both players: `<data>/game-settings/<system>.json` for the console and
//! `<data>/game-settings/<system>/<rom stem>.json` for one game, each a map of setting keys to values (Linux's words:
//! `scaling: native`, `effect: crt`, `frame_pocket: …`). The game's file wins over the console's; neither means the
//! defaults. pocket-ui and pocket-core on Linux read and write them through `options::GameSettings`, which adds the
//! setting rows, the defaults and Settings' globals on top; Android's player reads and writes the very same files with
//! this alone (the Handheld view's, so a change in its pause menu is the view's This game page's too).
//!
//! Until 18.11.2026 Android's player kept its own `picture/systems|games/*.json` (indexes) and the Handheld view translated
//! them back and forth (`android_handheld::picture_choice`, `picture_back`); the player's `picture::migrate_old` takes such a file over.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Where the values in use came from.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Level {
    Defaults,
    Console,
    Game,
}

/// One game's two files.
#[derive(Clone, Debug)]
pub struct Files {
    dir: PathBuf,
    system: String,
    game: String,
}

impl Files {
    pub fn new(data_dir: &Path, system: &str, rom: &Path) -> Self {
        let game = rom.file_stem().and_then(|s| s.to_str()).unwrap_or("game").to_string();
        Files { dir: data_dir.join("game-settings"), system: system.to_string(), game }
    }

    pub fn system(&self) -> &str {
        &self.system
    }

    pub fn game_file(&self) -> PathBuf {
        self.dir.join(&self.system).join(format!("{}.json", self.game))
    }

    pub fn console_file(&self) -> PathBuf {
        self.dir.join(format!("{}.json", self.system))
    }

    /// The values saved for the game, else for its console, and which; with the renames every reader needs.
    pub fn read(&self) -> (BTreeMap<String, String>, Level) {
        let read = |p: PathBuf| crate::fsutil::read_json_or_backup::<BTreeMap<String, String>>(&p);
        let (mut values, level) = match (read(self.game_file()), read(self.console_file())) {
            (Some(v), _) => (v, Level::Game),
            (None, Some(v)) => (v, Level::Console),
            (None, None) => (BTreeMap::new(), Level::Defaults),
        };
        // "Aspect screen" was dropped on 2026-10-24 (it always matched Fullscreen): an old choice shows as Fullscreen.
        if values.get("scaling").map(String::as_str) == Some("aspect-screen") {
            values.insert("scaling".into(), "fullscreen".into());
        }
        (values, level)
    }

    /// The values become the console's; the game's own file goes. False when the card refused.
    pub fn save_for_console(&self, values: &BTreeMap<String, String>) -> bool {
        crate::fsutil::remove_json(&self.game_file());
        crate::fsutil::write_json(&self.console_file(), values).is_ok()
    }

    /// The values become this game's own. False when the card refused.
    pub fn save_for_game(&self, values: &BTreeMap<String, String>) -> bool {
        crate::fsutil::write_json(&self.game_file(), values).is_ok()
    }

    /// Keeps the values where `level` says after a change (the console's file for `Console`, else the game's), written
    /// soon by the card writer so a menu doesn't wait. Returns the level they are kept at now.
    pub fn keep(&self, level: Level, values: &BTreeMap<String, String>) -> Level {
        if level == Level::Console {
            crate::fsutil::write_json_soon(&self.console_file(), values);
            Level::Console
        } else {
            crate::fsutil::write_json_soon(&self.game_file(), values);
            Level::Game
        }
    }

    /// The game's own file goes (its console's, or the defaults, count again).
    pub fn forget_game(&self) {
        crate::fsutil::remove_json(&self.game_file());
    }

    /// Back to the built-in defaults: both files go.
    pub fn restore_defaults(&self) {
        crate::fsutil::remove_json(&self.game_file());
        crate::fsutil::remove_json(&self.console_file());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_game_file_wins_and_keep_follows_the_level() {
        let dir = crate::testutil::TestDir::new("game-settings-files");
        let f = Files::new(&dir, "gba", Path::new("/roms/gba/Golden Sun.gba"));
        assert_eq!(f.read().1, Level::Defaults);
        let v: BTreeMap<String, String> = [("effect".to_string(), "crt".to_string())].into_iter().collect();
        assert!(f.save_for_console(&v));
        assert_eq!(f.read(), (v.clone(), Level::Console));
        let mut w = v.clone();
        w.insert("scaling".into(), "aspect-screen".into());
        assert!(f.save_for_game(&w));
        let (read, level) = f.read();
        assert_eq!((read["scaling"].as_str(), level), ("fullscreen", Level::Game), "the game's file, renamed value");
        assert_eq!(f.game_file(), dir.join("game-settings/gba/Golden Sun.json"));
        f.forget_game();
        assert_eq!(f.read().1, Level::Console);
        f.restore_defaults();
        assert_eq!(f.read().1, Level::Defaults);
    }
}
