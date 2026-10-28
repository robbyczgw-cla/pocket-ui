//! What pocket-core tells pocket-ui about achievements, through files:
//!
//! - [`Snapshot`] in `SNAPSHOT_FILE`: the running game's list for the pause menu, rewritten when something changes
//!   (an unlock, progress, the game becoming ready) and when the menu opens.
//! - [`Summary`] per game in `<data>/achievements/games/<system>/<rom stem>.json`: "12 of 40" for the game card and
//!   the console page, kept after the game ends. The game card can also look it up itself (`lookup`).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Where pocket-core leaves the running game's achievements for the pause menu.
pub const SNAPSHOT_FILE: &str = "/tmp/pocket-core.achievements.json";

/// How far the running game got with RetroAchievements.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum State {
    /// Signing in and identifying the game.
    #[default]
    Loading,
    Ready,
    /// RetroAchievements doesn't know this game (or it has no achievements).
    NoSet,
    /// The server couldn't be reached and nothing is known from an earlier session.
    Offline,
    /// The account needs signing in again (the token was refused).
    SignedOut,
    /// Something else went wrong; `message` says what.
    Failed,
}

/// One achievement of the running game.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Entry {
    pub id: u32,
    pub title: String,
    pub description: String,
    pub points: u32,
    pub unlocked: bool,
    /// Progress towards it when the set measures one ("3/10"), else empty.
    #[serde(default)]
    pub progress: String,
    #[serde(default)]
    pub percent: f32,
    /// This rcheevos can't run it.
    #[serde(default)]
    pub unsupported: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Snapshot {
    /// The ROM as pocket-ui passed it, so the menu reads the snapshot of its own game only.
    pub rom: String,
    pub state: State,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub user: String,
    #[serde(default)]
    pub game: String,
    #[serde(default)]
    pub game_id: u32,
    #[serde(default)]
    pub unlocked: u32,
    #[serde(default)]
    pub total: u32,
    #[serde(default)]
    pub points: u32,
    #[serde(default)]
    pub points_unlocked: u32,
    /// Unlocks not confirmed by the server yet (offline); they are sent later.
    #[serde(default)]
    pub waiting: u32,
    #[serde(default)]
    pub achievements: Vec<Entry>,
}

impl Snapshot {
    /// The snapshot of `rom`, if pocket-core wrote one for that game.
    pub fn read(path: &Path, rom: &Path) -> Option<Snapshot> {
        let s: Snapshot = serde_json::from_slice(&std::fs::read(path).ok()?).ok()?;
        (Path::new(&s.rom) == rom).then_some(s)
    }

    pub fn write(&self, path: &Path) {
        match serde_json::to_vec(self) {
            Ok(bytes) => {
                if let Err(e) = crate::fsutil::write_atomic(path, &bytes) {
                    eprintln!("achievements: can't write {}: {e}", path.display());
                }
            }
            Err(e) => eprintln!("achievements: {e}"),
        }
    }

    /// "12 of 40 unlocked · 85 of 400 points" for the top of the list.
    pub fn headline(&self) -> String {
        match self.state {
            State::Ready if self.total > 0 => {
                let mut s = format!(
                    "{} of {} unlocked · {} of {} points",
                    self.unlocked, self.total, self.points_unlocked, self.points
                );
                if self.waiting > 0 {
                    s.push_str(&format!(" · {} to send", self.waiting));
                }
                s
            }
            _ => self.state_text(),
        }
    }

    /// What the state means, in a sentence.
    pub fn state_text(&self) -> String {
        match self.state {
            State::Loading => "Looking up this game on RetroAchievements…".into(),
            State::Ready if self.total == 0 => "This game has no achievements yet.".into(),
            State::Ready => format!("{} of {} unlocked", self.unlocked, self.total),
            State::NoSet => "RetroAchievements has no achievements for this version of the game.".into(),
            State::Offline => "Couldn't reach RetroAchievements. Unlocks count once the Pocket is online at the start of a game.".into(),
            State::SignedOut => "RetroAchievements didn't accept the sign-in. Sign in again under Settings › Playing › RetroAchievements.".into(),
            State::Failed if self.message.is_empty() => "RetroAchievements isn't available for this game.".into(),
            State::Failed => format!("RetroAchievements: {}", self.message),
        }
    }
}

/// The player's tally for one game, kept for the game card and the console page.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Default)]
pub struct Summary {
    /// 0: RetroAchievements doesn't know the game.
    pub game_id: u32,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub hash: String,
    #[serde(default)]
    pub total: u32,
    #[serde(default)]
    pub unlocked: u32,
    /// Unix seconds of the last update.
    #[serde(default)]
    pub updated: u64,
}

impl Summary {
    pub fn path(data_dir: &Path, system: &str, rom: &Path) -> PathBuf {
        let stem = rom.file_stem().and_then(|s| s.to_str()).unwrap_or("game");
        data_dir.join("achievements").join("games").join(system).join(format!("{stem}.json"))
    }

    pub fn load(data_dir: &Path, system: &str, rom: &Path) -> Option<Summary> {
        serde_json::from_slice(&std::fs::read(Self::path(data_dir, system, rom)).ok()?).ok()
    }

    pub fn save(&self, data_dir: &Path, system: &str, rom: &Path) {
        if let Ok(bytes) = serde_json::to_vec(self) {
            if let Err(e) = crate::fsutil::write_atomic(&Self::path(data_dir, system, rom), &bytes) {
                eprintln!("achievements: can't keep the tally: {e}");
            }
        }
    }

    /// "12 of 40" when the game has a set; None otherwise.
    pub fn text(&self) -> Option<String> {
        (self.game_id != 0 && self.total > 0).then(|| format!("{} of {}", self.unlocked, self.total))
    }
}

/// The tallies of a console's games that have a set: (unlocked, total, games), for the console page. Reads the
/// console's folder of tallies once (only games played or looked up have one), not a file per game.
pub fn console_tally<'a>(data_dir: &Path, system: &str, roms: impl Iterator<Item = &'a Path>) -> Option<(u32, u32, usize)> {
    let dir = data_dir.join("achievements").join("games").join(system);
    let mut kept = std::collections::HashMap::new();
    for e in std::fs::read_dir(&dir).ok()?.flatten() {
        let path = e.path();
        if let (Some(stem), Ok(bytes)) = (path.file_stem().and_then(|s| s.to_str()).map(str::to_string), std::fs::read(&path)) {
            if let Ok(s) = serde_json::from_slice::<Summary>(&bytes) {
                kept.insert(stem, s);
            }
        }
    }
    let (mut unlocked, mut total, mut games) = (0, 0, 0);
    for rom in roms {
        let stem = rom.file_stem().and_then(|s| s.to_str()).unwrap_or_default();
        if let Some(s) = kept.get(stem).filter(|s| s.game_id != 0 && s.total > 0) {
            unlocked += s.unlocked.min(s.total);
            total += s.total;
            games += 1;
        }
    }
    (games > 0).then_some((unlocked, total, games))
}

/// "35 of 120 achievements in 3 games" for the console page.
pub fn console_text(t: (u32, u32, usize)) -> String {
    let (unlocked, total, games) = t;
    format!("{unlocked} of {total} achievements in {games} {}", if games == 1 { "game" } else { "games" })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;

    #[test]
    fn the_menu_reads_only_its_own_game() {
        let dir = TestDir::new("ra-snap");
        let path = dir.join("snap.json");
        let s = Snapshot {
            rom: "/roms/nes/a.nes".into(),
            state: State::Ready,
            unlocked: 1,
            total: 2,
            points: 7,
            points_unlocked: 5,
            ..Default::default()
        };
        s.write(&path);
        assert_eq!(Snapshot::read(&path, Path::new("/roms/nes/a.nes")), Some(s.clone()));
        assert_eq!(Snapshot::read(&path, Path::new("/roms/nes/b.nes")), None);
        assert_eq!(s.headline(), "1 of 2 unlocked · 5 of 7 points");
        let waiting = Snapshot { waiting: 1, ..s };
        assert!(waiting.headline().ends_with("· 1 to send"));
    }

    #[test]
    fn every_state_has_words() {
        for state in [State::Loading, State::Ready, State::NoSet, State::Offline, State::SignedOut, State::Failed] {
            let s = Snapshot { state, ..Default::default() };
            assert!(!s.headline().is_empty() && !s.state_text().is_empty(), "{state:?}");
        }
        let failed = Snapshot {
            state: State::Failed,
            message: "Unknown game".into(),
            ..Default::default()
        };
        assert_eq!(failed.state_text(), "RetroAchievements: Unknown game");
    }

    #[test]
    fn tallies_for_the_card_and_the_console_page() {
        let dir = TestDir::new("ra-tally");
        let (a, b, c) = (Path::new("/r/nes/A.nes"), Path::new("/r/nes/B.zip"), Path::new("/r/nes/C.nes"));
        assert_eq!(console_tally(&dir, "nes", [a, b, c].into_iter()), None);
        Summary {
            game_id: 1,
            total: 20,
            unlocked: 5,
            ..Default::default()
        }
        .save(&dir, "nes", a);
        Summary {
            game_id: 2,
            total: 10,
            unlocked: 10,
            ..Default::default()
        }
        .save(&dir, "nes", b);
        // Known to RetroAchievements as having no set: not counted, and no line on the card.
        let none = Summary {
            game_id: 0,
            ..Default::default()
        };
        none.save(&dir, "nes", c);
        assert_eq!(none.text(), None);
        assert_eq!(Summary::load(&dir, "nes", a).unwrap().text().as_deref(), Some("5 of 20"));
        let t = console_tally(&dir, "nes", [a, b, c].into_iter()).unwrap();
        assert_eq!(t, (15, 30, 2));
        assert_eq!(console_text(t), "15 of 30 achievements in 2 games");
        assert_eq!(console_text((1, 5, 1)), "1 of 5 achievements in 1 game");
    }
}
