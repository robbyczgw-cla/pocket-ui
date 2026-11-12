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
    /// Just Play on Home (A starts a suitable game, X shows it first). Its own switch since the Lens one stopped being the
    /// way to find a game to play; a `curator.json` from before it has no key and follows the Lens it had (`Prefs::load`).
    #[serde(default = "yes")]
    pub just_play: bool,
    /// Echoes on the game card.
    #[serde(default = "yes")]
    pub echoes: bool,
    /// Lists that mix systems show a game owned on several systems once.
    #[serde(default = "yes")]
    pub merge_versions: bool,
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
    /// A new Pocket: the Lens, Echoes and Merge versions are Enthusiast's (the shelf has Just Play for "pick me something").
    /// A Pocket that was set up before (`Prefs::load`) keeps them on without a file.
    fn default() -> Prefs {
        Prefs { lens: false, just_play: true, echoes: false, merge_versions: false, not_on_device: false, pixel_path: false, pixel_live: false, shelves: Vec::new(), history: true }
    }
}

/// Characters a shelf's name may have.
pub const SHELF_NAME_MAX: usize = 40;

impl Prefs {
    pub fn load(data_dir: &Path) -> Prefs {
        let mut p: Prefs = crate::fsutil::read_json_or_backup(&data_dir.join(FILE)).unwrap_or_else(|| Prefs::when_nothing_is_saved(data_dir));
        // Just Play went with the Lens switch until it got its own: a player who had the Lens off had no Just Play, and
        // keeps none until they switch it on (a file that has the key says what it says).
        if !p.lens && p.saved_without_just_play(data_dir) {
            p.just_play = false;
        }
        p
    }

    /// Without a `curator.json`: a Pocket whose first start ran (`settings.json` exists, it is only written after) always had
    /// the Lens, Echoes and Merge versions on by default, and keeps them; a new one starts as Simple does.
    fn when_nothing_is_saved(data_dir: &Path) -> Prefs {
        if data_dir.join("settings.json").exists() {
            Prefs { lens: true, echoes: true, merge_versions: true, ..Prefs::default() }
        } else {
            Prefs::default()
        }
    }

    /// Whether there is a `curator.json` that has no `just_play` key (it was saved by a version without the switch).
    fn saved_without_just_play(&self, data_dir: &Path) -> bool {
        std::fs::read_to_string(data_dir.join(FILE))
            .ok()
            .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
            .is_some_and(|v| v.is_object() && v.get("just_play").is_none())
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
    fn retired_suggestions_and_labels_keep_saved_smart_shelves() {
        let p: Prefs = serde_json::from_str(r#"{"up_next":true,"stickers":true,"echoes":false,"shelves":[{"name":"My shelf","lens":{},"pinned":true}]}"#).unwrap();
        assert!(!p.echoes);
        assert_eq!(p.shelves.len(), 1);
        assert_eq!(p.shelves[0].name, "My shelf");
        let saved = serde_json::to_value(&p).unwrap();
        assert!(saved.get("up_next").is_none() && saved.get("stickers").is_none());
        assert_eq!(serde_json::from_value::<Prefs>(saved).unwrap().shelves, p.shelves);
    }

    #[test]
    fn defaults_survive_a_restart_and_shelves_get_free_names() {
        let d = TestDir::new("curator-prefs");
        let p = Prefs::load(&d);
        assert_eq!(p, Prefs::default());
        assert!(!p.lens && !p.echoes && !p.merge_versions && !p.not_on_device && !p.pixel_path && p.history, "a new Pocket: the depth is Enthusiast's");
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

    /// The Lens, Echoes and Merge versions became Enthusiast's (Streichliste 0.4): a Pocket whose first start ran, and that
    /// saved no `curator.json`, had them on and keeps them; one that saved a file keeps what the file says.
    #[test]
    fn a_pocket_set_up_before_keeps_the_curator_it_had() {
        let fresh = TestDir::new("curator-prefs-fresh");
        let p = Prefs::load(&fresh);
        assert!(!p.lens && !p.echoes && !p.merge_versions, "no settings.json: a new Pocket starts as Simple does");
        let old = TestDir::new("curator-prefs-old");
        std::fs::write(old.join("settings.json"), r#"{"onboarded":true}"#).unwrap();
        let p = Prefs::load(&old);
        assert!(p.lens && p.echoes && p.merge_versions && p.just_play, "set up before, nothing saved: the Curator it always had");
        // A saved file says what it says, switched off or on.
        std::fs::write(old.join(FILE), r#"{"lens":false,"echoes":true}"#).unwrap();
        let p = Prefs::load(&old);
        assert!(!p.lens && p.echoes && p.merge_versions, "{p:?}");
    }

    /// Just Play and the Lens were one switch. A saved file from before keeps what the player saw: no Lens, no Just Play.
    #[test]
    fn just_play_keeps_the_lens_choice_of_an_older_file_and_then_has_its_own() {
        let d = TestDir::new("curator-prefs-just-play");
        assert!(Prefs::load(&d).just_play, "no file: on");
        std::fs::write(d.join(FILE), r#"{"lens": true}"#).unwrap();
        assert!(Prefs::load(&d).just_play, "Lens on: Just Play was there");
        std::fs::write(d.join(FILE), r#"{"lens": false}"#).unwrap();
        let mut p = Prefs::load(&d);
        assert!(!p.lens && !p.just_play, "Lens off: Just Play was not on Home, it stays off");
        // Switched on again, it is written and kept whatever the Lens says.
        p.just_play = true;
        assert!(p.save(&d));
        let again = Prefs::load(&d);
        assert!(!again.lens && again.just_play, "its own key wins");
        // And a file from before the Lens existed has neither key.
        std::fs::write(d.join(FILE), r#"{"echoes": false}"#).unwrap();
        assert!(Prefs::load(&d).just_play);
    }

}
