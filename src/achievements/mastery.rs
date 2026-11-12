//! A game with every achievement unlocked ("mastered"), as the player tells the launcher: a mark file per game, kept
//! under `<data>/achievements/mastered/<key>` (the 16-hex-digit game identity of the Android app). The launcher turns
//! each new mark into the golden "100 %" sticker on the game's case, once ([`done`] moves it to `mastered-done`), so
//! a sticker the player took off stays off, however often the game is played and mastered again. The sticker then
//! counts for the trophy on the shelf (`room_deco::earned`); there is no room of its own.
//!
//! The two sides only share files, because they run in two processes (`:player` and the launcher).

use std::path::{Path, PathBuf};

fn root(data_dir: &Path) -> PathBuf {
    super::dir(data_dir)
}

fn marks(data_dir: &Path) -> PathBuf {
    root(data_dir).join("mastered")
}

fn used(data_dir: &Path) -> PathBuf {
    root(data_dir).join("mastered-done")
}

/// A game identity as the Android app makes it: 16 hex digits (anything else never becomes a file name).
pub fn valid_key(key: &str) -> bool {
    key.len() == 16 && key.bytes().all(|b| b.is_ascii_hexdigit())
}

/// The player: every achievement of `key`'s game is unlocked. Leaves a mark unless the launcher has used one for
/// this game before. True when a mark was written now.
pub fn mark(data_dir: &Path, key: &str, title: &str) -> bool {
    if !valid_key(key) || used(data_dir).join(key).exists() {
        return false;
    }
    let path = marks(data_dir).join(key);
    if path.exists() {
        return false;
    }
    crate::fsutil::write_atomic(&path, title.as_bytes()).is_ok()
}

/// The launcher: the games marked and not yet used, as (key, the game's title on RetroAchievements).
pub fn pending(data_dir: &Path) -> Vec<(String, String)> {
    let Ok(entries) = std::fs::read_dir(marks(data_dir)) else { return Vec::new() };
    let mut out: Vec<(String, String)> = entries
        .flatten()
        .filter_map(|e| {
            let key = e.file_name().to_str()?.to_string();
            valid_key(&key).then(|| (key, std::fs::read_to_string(e.path()).unwrap_or_default()))
        })
        .collect();
    out.sort();
    out
}

/// The launcher has put the sticker on `key`'s case: the mark is used up for good.
pub fn done(data_dir: &Path, key: &str) {
    if !valid_key(key) {
        return;
    }
    let _ = crate::fsutil::write_atomic(&used(data_dir).join(key), b"");
    std::fs::remove_file(marks(data_dir).join(key)).ok();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;

    const KEY: &str = "0123456789abcdef";

    #[test]
    fn a_mastered_game_is_marked_once_and_a_used_mark_stays_used() {
        let d = TestDir::new("ra-mastery");
        assert!(pending(&d).is_empty(), "nothing before any game is mastered");
        assert!(mark(&d, KEY, "Sonic the Hedgehog"));
        assert!(!mark(&d, KEY, "Sonic the Hedgehog"), "the same mark is not written twice");
        assert_eq!(pending(&d), vec![(KEY.to_string(), "Sonic the Hedgehog".to_string())]);
        done(&d, KEY);
        assert!(pending(&d).is_empty());
        // The player masters it again in a later session: no new mark, so a sticker taken off stays off.
        assert!(!mark(&d, KEY, "Sonic the Hedgehog"));
        assert!(pending(&d).is_empty());
    }

    #[test]
    fn only_real_game_identities_become_files() {
        let d = TestDir::new("ra-mastery-keys");
        for bad in ["", "../etc", "0123456789abcde", "0123456789abcdeg", "0123456789abcdef0"] {
            assert!(!mark(&d, bad, "x"), "{bad:?}");
            done(&d, bad);
        }
        assert!(pending(&d).is_empty());
    }
}
