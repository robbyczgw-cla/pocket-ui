//! Curator: Roon-style discovery for the games on the device (docs/CURATOR.md). Pure logic, no drawing and no
//! files of its own beyond `curator.json` (`prefs`); the binary's `app_curator.rs` runs it on a thread and shows it.
//!
//! - [`features`]: THE ADAPTER, `features_for`, from a game of the library (gamedb, file name, feel tags) to the
//!   engine's features. The feel tags of `assets/curatordb.bin` come through [`features::CuratorDbFeel`]; the
//!   file-name facts are `crate::romfacts`.
//! - [`index`]: bitsets per facet value, term vectors, works, the taste profile.
//! - [`lens`]: AND across facets, OR within one, live counts, suggested names.
//! - [`echoes`]: "more like this" with reason lines.
//! - [`versions`]: one game on several systems, the preferred version, lists with each game once.
//! - [`sort`]: the orders of the Lens's games and of a smart shelf (name, last played, play time, rating, year, length).
//! - [`surprise`]: "Surprise me", one game picked for you with the reason.
//! - [`today`]: what Home's Curator row offers: the day's pick and the games waiting for you.
//! - [`prefs`]: Settings → Curator and the smart shelves.
//! - `fixture` (tests only): a few games of hand-made tags, so some tests don't move when curatordb is rebuilt.

pub mod bits;
pub mod echoes;
pub mod features;
#[cfg(test)]
pub mod fixture;
pub mod index;
pub mod lens;
pub mod prefs;
pub mod sort;
pub mod surprise;
pub mod today;
pub mod versions;

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;

pub use features::{features_for, CuratorDbFeel, Facet, FeelSource, Features, GameRef, NoFeel};
pub use index::{Catalog, Index, SystemName, UserGame, UserState};
pub use lens::Lens;

use crate::gamedb::{Choice, GameDb};
use crate::library_meta::{GameMeta, Sticker};

/// A game of the library as the catalog's thread gets it.
#[derive(Clone, Debug)]
pub struct LibraryGame {
    /// (system, game) in the app's library.
    pub at: (usize, usize),
    pub system: String,
    pub path: PathBuf,
    pub name: String,
    /// "Wrong game? Choose another" from library.json.
    pub choice: Option<Choice>,
    /// The name its checksum gave (`rom_id::known`), matched before the file name.
    pub identified: Option<String>,
}

/// Builds the catalog: each game matched in gamedb (the player's choice first), its feel tags from `feel`, through
/// [`features_for`]. Runs on a background thread (332 games: 136 ms on the Mini V2, most of it unpacking gamedb).
pub fn build_catalog(games: &[LibraryGame], systems: Vec<SystemName>, db: &mut GameDb, feel: &dyn FeelSource) -> Catalog {
    let features = games
        .iter()
        .map(|g| {
            let stem = g.path.file_stem().and_then(|s| s.to_str()).unwrap_or_default();
            let info = db.resolve_rom(&g.system, stem, g.identified.as_deref(), g.choice.as_ref());
            let tags = info.as_ref().map(|i| feel.tags(&g.system, i)).unwrap_or_default();
            features_for(GameRef { at: g.at, system: &g.system, path: &g.path, name: &g.name }, info.as_ref(), &tags)
        })
        .collect();
    Catalog::build(features, systems)
}

/// What the player did, gathered for the index's thread.
#[derive(Clone, Debug, Default)]
pub struct Player {
    /// library.json's games, keyed by path.
    pub meta: BTreeMap<String, GameMeta>,
    pub stickers: Vec<Sticker>,
    pub preferred: BTreeMap<String, String>,
    pub favorites: HashSet<PathBuf>,
    /// The recent games: path -> (last played, seconds played).
    pub recents: HashMap<PathBuf, (u64, u64)>,
    pub now: u64,
}

/// The player's state lined up with the catalog's games.
pub fn user_state(catalog: &Catalog, p: &Player) -> UserState {
    let games = catalog
        .games
        .iter()
        .map(|g| {
            let meta = p.meta.get(&g.path.to_string_lossy().into_owned());
            let recent = p.recents.get(&g.path);
            UserGame {
                status: meta.map(|m| m.status).unwrap_or_default(),
                secs: meta.map_or(0, |m| m.secs).max(recent.map_or(0, |r| r.1)),
                last_played: recent.map(|r| r.0),
                starred: p.favorites.contains(&g.path),
                stickers: meta.map(|m| m.stickers.clone()).unwrap_or_default(),
                rating: meta.map_or(0, |m| m.rating),
            }
        })
        .collect();
    UserState {
        games,
        stickers: p.stickers.iter().map(|s| index::StickerName { id: s.id, name: s.name.clone() }).collect(),
        preferred: p.preferred.iter().map(|(k, v)| (k.clone(), PathBuf::from(v))).collect(),
        now: p.now,
    }
}

#[cfg(test)]
mod tests;
