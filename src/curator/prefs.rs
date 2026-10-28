//! Settings → Curator and the smart shelves, in `curator.json` in the data directory.

use std::path::Path;

use serde::{Deserialize, Serialize};

use super::lens::Lens;
use super::sort::Order;

pub const FILE: &str = "curator.json";

fn yes() -> bool {
    true
}

/// A Lens saved under a name. It updates itself: what it lists is worked out from the index each time.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SmartShelf {
    pub name: String,
    pub lens: Lens,
    /// Shown as a row on Home (a new shelf is).
    #[serde(default = "yes")]
    pub pinned: bool,
    /// The order of its games (by name unless the player chose another); written only when it isn't by name.
    #[serde(default, skip_serializing_if = "Order::is_name")]
    pub order: Order,
}

/// The switches of Settings → Curator (defaults from docs/CURATOR.md) and the smart shelves.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Prefs {
    /// Lens & smart shelves: the Curator row on Home, pinned shelves.
    #[serde(default = "yes")]
    pub lens: bool,
    /// Echoes on the game card.
    #[serde(default = "yes")]
    pub echoes: bool,
    /// Up next: after a game, one line on the shelf suggests an Echo of it (`app_curator.rs`). Never starts anything
    /// by itself, but for a few seconds A plays the suggestion, and A is the button still being pressed when a game
    /// ends: Off by default since 2026-10-25 (a `curator.json` that has the key keeps its value; Enthusiast sets On).
    #[serde(default)]
    pub up_next: bool,
    /// Lists that mix systems show a game owned on several systems once.
    #[serde(default = "yes")]
    pub merge_versions: bool,
    #[serde(default = "yes")]
    pub stickers: bool,
    /// Echoes also names games that are not on this device.
    #[serde(default)]
    pub not_on_device: bool,
    /// Pixel Path in the pause menu (`app_pixel_path.rs`); pocket-ui keeps a copy in `App::pixel_path`.
    #[serde(default)]
    pub pixel_path: bool,
    /// "Pixel Path over every game" until 2026-10-25, now Settings → Playing → Over the game (`over_game`): read so
    /// pocket-ui's start can move it there once (`over_game::adopt_curator`), never written again.
    #[serde(default, skip_serializing)]
    pub pixel_live: bool,
    #[serde(default)]
    pub shelves: Vec<SmartShelf>,
    /// Play history (`play_log`, Curator → History, the Year in review): on by default. Off records nothing new.
    #[serde(default = "yes")]
    pub history: bool,
}

impl Default for Prefs {
    fn default() -> Prefs {
        Prefs { lens: true, echoes: true, up_next: false, merge_versions: true, stickers: true, not_on_device: false, pixel_path: false, pixel_live: false, shelves: Vec::new(), history: true }
    }
}

/// Characters a shelf's name may have.
pub const SHELF_NAME_MAX: usize = 40;

impl Prefs {
    pub fn load(data_dir: &Path) -> Prefs {
        crate::fsutil::read_json_or_backup(&data_dir.join(FILE)).unwrap_or_default()
    }

    pub fn save(&self, data_dir: &Path) -> bool {
        match crate::fsutil::write_json(&data_dir.join(FILE), self) {
            Ok(()) => true,
            Err(e) => {
                eprintln!("curator: can't save {FILE}: {e}");
                false
            }
        }
    }

    /// Adds a shelf (pinned) with a name not taken yet ("Racing", "Racing 2"); returns its index.
    pub fn add_shelf(&mut self, name: &str, lens: Lens) -> usize {
        let base: String = name.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(SHELF_NAME_MAX).collect();
        let base = if base.is_empty() { "Shelf".to_string() } else { base };
        let mut name = base.clone();
        let mut n = 2;
        while self.shelves.iter().any(|s| s.name.eq_ignore_ascii_case(&name)) {
            name = format!("{base} {n}");
            n += 1;
        }
        self.shelves.push(SmartShelf { name, lens, pinned: true, order: Order::Name });
        self.shelves.len() - 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::curator::features::Facet;
    use crate::testutil::TestDir;

    #[test]
    fn defaults_survive_a_restart_and_shelves_get_free_names() {
        let d = TestDir::new("curator-prefs");
        let p = Prefs::load(&d);
        assert_eq!(p, Prefs::default());
        assert!(p.lens && p.echoes && !p.up_next && p.merge_versions && p.stickers && !p.not_on_device && !p.pixel_path && p.history);
        let mut p = p;
        let mut lens = Lens::default();
        lens.toggle(Facet::System, "gba");
        assert_eq!(p.add_shelf("GBA", lens.clone()), 0);
        assert_eq!(p.add_shelf("gba", lens.clone()), 1);
        assert_eq!(p.shelves[1].name, "gba 2");
        p.echoes = false;
        assert!(p.save(&d));
        let again = Prefs::load(&d);
        assert_eq!(again, p);
        assert!(again.shelves[0].pinned, "a new shelf is on Home");
        // A shelf's order is kept with it; by name it isn't written at all, so older files and readers see nothing new.
        let mut p = again;
        p.shelves[1].order = Order::LastPlayed;
        assert!(p.save(&d));
        let text = std::fs::read_to_string(d.join(FILE)).unwrap();
        assert_eq!(text.matches("\"order\"").count(), 1, "{text}");
        assert!(text.contains("\"last_played\""), "{text}");
        let again = Prefs::load(&d);
        assert_eq!((again.shelves[0].order, again.shelves[1].order), (Order::Name, Order::LastPlayed));
        // A file from before a setting existed takes that setting's default.
        std::fs::write(d.join(FILE), r#"{"echoes": false}"#).unwrap();
        let p = Prefs::load(&d);
        assert!(!p.echoes && p.lens && p.merge_versions && p.history, "Play history is on for a file from before it");
    }

    #[test]
    fn up_next_is_off_unless_the_file_says_otherwise() {
        assert!(!Prefs::default().up_next, "A must not play a suggestion right after a game unless it was asked for");
        let p: Prefs = serde_json::from_str(r#"{"echoes": true}"#).unwrap();
        assert!(!p.up_next, "a file from before the key existed");
        let p: Prefs = serde_json::from_str(r#"{"up_next": true}"#).unwrap();
        assert!(p.up_next, "a file that has the key keeps it");
        let p: Prefs = serde_json::from_str(r#"{"up_next": false}"#).unwrap();
        assert!(!p.up_next);
    }
}
