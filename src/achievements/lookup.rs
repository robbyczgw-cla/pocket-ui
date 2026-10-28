//! "12 of 40 achievements" on the game card for a game that hasn't been played with achievements yet: the ROM's hash
//! names the game (`r=gameid`), and one request per console brings the player's numbers for all of its games
//! (`r=allprogress`, kept for an hour in `<data>/achievements/progress/<console>.json`). The result is the game's
//! `Summary`, as pocket-core writes it after playing. Runs on a background thread when a card opens; asks the server at
//! most once an hour per game (once a week for a game RetroAchievements doesn't know).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::account::Account;
use super::api::{self, Progress};
use super::consoles;
use super::http::Transport;
use super::snapshot::Summary;

/// How long a game's numbers are trusted before the card asks again.
pub const FRESH_FOR: u64 = 3600;
/// How long "RetroAchievements doesn't know this game" is trusted.
pub const UNKNOWN_FOR: u64 = 7 * 24 * 3600;

#[derive(Serialize, Deserialize, Default)]
struct ConsoleProgress {
    updated: u64,
    games: Vec<(u32, u32, u32)>,
}

fn progress_path(data_dir: &Path, console: u32) -> PathBuf {
    super::dir(data_dir).join("progress").join(format!("{console}.json"))
}

/// Whether the card should look the game up now (nothing known, or known for too long).
pub fn wanted(summary: Option<&Summary>, now: u64) -> bool {
    match summary {
        None => true,
        Some(s) if s.game_id == 0 => now.saturating_sub(s.updated) > UNKNOWN_FOR,
        Some(s) => now.saturating_sub(s.updated) > FRESH_FOR,
    }
}

/// The console's progress list, from the hour-old copy or the server.
fn console_progress(transport: &dyn Transport, data_dir: &Path, account: &Account, console: u32, now: u64) -> Option<Vec<Progress>> {
    let path = progress_path(data_dir, console);
    let kept: Option<ConsoleProgress> = std::fs::read(&path).ok().and_then(|b| serde_json::from_slice(&b).ok());
    if let Some(k) = kept.as_ref().filter(|k| now.saturating_sub(k.updated) <= FRESH_FOR) {
        return Some(
            k.games
                .iter()
                .map(|&(game_id, total, unlocked)| Progress { game_id, total, unlocked })
                .collect(),
        );
    }
    match api::all_progress(transport, account, console) {
        Ok(list) => {
            let k = ConsoleProgress {
                updated: now,
                games: list.iter().map(|p| (p.game_id, p.total, p.unlocked)).collect(),
            };
            if let Ok(bytes) = serde_json::to_vec(&k) {
                let _ = crate::fsutil::write_atomic(&path, &bytes);
            }
            Some(list)
        }
        Err(e) => {
            eprintln!("achievements: no progress list for console {console}: {e}");
            kept.map(|k| {
                k.games
                    .iter()
                    .map(|&(game_id, total, unlocked)| Progress { game_id, total, unlocked })
                    .collect()
            })
        }
    }
}

/// Looks the game up and keeps the answer. None when the system has no achievements, the ROM can't be hashed or the
/// server couldn't be asked; Some(summary with game_id 0) when RetroAchievements doesn't know the game.
pub fn lookup(transport: &dyn Transport, data_dir: &Path, account: &Account, system: &str, rom: &Path, now: u64) -> Option<Summary> {
    let console = consoles::console_id(system)?;
    let known = Summary::load(data_dir, system, rom);
    // The hash and the game's id don't change: only the numbers are asked again.
    let (hash, game_id) = match known.as_ref().filter(|s| !s.hash.is_empty() && s.game_id != 0) {
        Some(s) => (s.hash.clone(), s.game_id),
        None => {
            let hash = super::hash_rom(console, rom)?;
            (hash.clone(), api::resolve_hash(transport, &hash).ok()?)
        }
    };
    if game_id == 0 {
        let s = Summary {
            game_id: 0,
            hash,
            updated: now,
            ..Default::default()
        };
        s.save(data_dir, system, rom);
        return Some(s);
    }
    let list = console_progress(transport, data_dir, account, console, now)?;
    let p = list.iter().find(|p| p.game_id == game_id);
    let s = Summary {
        game_id,
        title: known.map(|k| k.title).unwrap_or_default(),
        hash,
        total: p.map_or(0, |p| p.total),
        unlocked: p.map_or(0, |p| p.unlocked),
        updated: now,
    };
    s.save(data_dir, system, rom);
    Some(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::achievements::api::tests::Mock;
    use crate::achievements::http::Response;
    use crate::testutil::TestDir;

    #[test]
    fn a_game_is_looked_up_once_and_its_numbers_refreshed_hourly() {
        let dir = TestDir::new("ra-lookup");
        let rom = dir.join("Game.gb");
        std::fs::write(&rom, vec![7u8; 4096]).unwrap();
        let account = Account {
            username: "Alex".into(),
            token: "Tk".into(),
            display_name: String::new(),
        };
        let mock = Mock::new(vec![
            ("gameid", Response::ok(r#"{"Success":true,"GameID":1234}"#)),
            (
                "allprogress",
                Response::ok(r#"{"Success":true,"Response":{"1234":{"Achievements":40,"Unlocked":12,"UnlockedHardcore":0}}}"#),
            ),
        ]);
        assert!(wanted(None, 1000));
        let s = lookup(&mock, &dir, &account, "gb", &rom, 1000).unwrap();
        assert_eq!(s.text().as_deref(), Some("12 of 40"));
        assert!(!wanted(Some(&s), 1000 + FRESH_FOR));
        assert!(wanted(Some(&s), 1001 + FRESH_FOR));
        // An hour later: no new hash lookup, and the console's list comes from the server again.
        lookup(&mock, &dir, &account, "gb", &rom, 1001 + FRESH_FOR).unwrap();
        let asked: Vec<String> = mock.asked.lock().unwrap().iter().filter_map(|r| r.post_data.clone()).collect();
        assert_eq!(asked.iter().filter(|p| p.starts_with("r=gameid")).count(), 1, "{asked:?}");
        assert_eq!(asked.iter().filter(|p| p.starts_with("r=allprogress")).count(), 2, "{asked:?}");
        // A second game of the same console within the hour uses the kept list.
        let other = dir.join("Other.gb");
        std::fs::write(&other, vec![9u8; 4096]).unwrap();
        lookup(&mock, &dir, &account, "gb", &other, 1002 + FRESH_FOR).unwrap();
        assert_eq!(
            mock.asked
                .lock()
                .unwrap()
                .iter()
                .filter(|r| r.post_data.as_deref().is_some_and(|p| p.starts_with("r=allprogress")))
                .count(),
            2
        );
        // No achievements for PICO-8; nothing asked.
        assert_eq!(lookup(&mock, &dir, &account, "pico-8", &rom, 0), None);
        // Unknown game: kept as such for a week.
        let unknown = Mock::new(vec![("gameid", Response::ok(r#"{"Success":true,"GameID":0}"#))]);
        let u = lookup(&unknown, &dir, &account, "gbc", &rom, 50).unwrap();
        assert_eq!((u.game_id, u.text()), (0, None));
        assert!(!wanted(Some(&u), 50 + UNKNOWN_FOR) && wanted(Some(&u), 51 + UNKNOWN_FOR));
        // Offline: nothing new, nothing written.
        assert_eq!(lookup(&Mock::new(vec![]), &dir, &account, "nes", &rom, 0), None);
    }
}
