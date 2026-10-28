//! The index: one bitset per facet value over the games of the library, the Echoes vectors and the works.
//!
//! Two parts, built off the UI thread:
//! - [`Catalog`]: what the data says about each game (facets from gamedb and the file name, IDF-weighted term
//!   vectors, works). Built once after the library is read, again when the library changes.
//! - [`Index`]: the catalog plus what the player did (status, time, stars, stickers, preferred versions) and the
//!   taste profile. Rebuilt in a few milliseconds after a session, a new status or a sticker.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::Arc;

use crate::library_meta::Status;

use super::bits::Bitset;
use super::features::{Axis, Facet, Features};

/// A system of the library, named as the shelf names it.
#[derive(Clone, Debug, Default)]
pub struct SystemName {
    pub id: String,
    /// "Super Nintendo Entertainment System".
    pub name: String,
    /// "SNES".
    pub short: String,
}

/// A value of a facet: its key (stable, saved in smart shelves), its labels and the games that have it.
#[derive(Clone, Debug)]
pub struct Value {
    pub key: String,
    /// "Game Boy Advance", "Racing", "Not started".
    pub label: String,
    /// For suggested shelf names: "GBA", "Racing", "not started".
    pub short: String,
    pub bits: Bitset,
}

#[derive(Clone, Debug)]
pub struct FacetValues {
    pub facet: Facet,
    pub values: Vec<Value>,
    /// key -> position in `values` (also the keys of values merged into another).
    by_key: HashMap<String, usize>,
}

impl FacetValues {
    fn new(facet: Facet, values: Vec<Value>) -> FacetValues {
        let by_key = values.iter().enumerate().map(|(i, v)| (v.key.clone(), i)).collect();
        FacetValues { facet, values, by_key }
    }

    pub fn find(&self, key: &str) -> Option<&Value> {
        self.by_key.get(key).map(|&i| &self.values[i])
    }

    pub fn position(&self, key: &str) -> Option<usize> {
        self.by_key.get(key).copied()
    }
}

/// What the data says about the games of the library.
pub struct Catalog {
    pub games: Vec<Features>,
    pub systems: Vec<SystemName>,
    /// Labels and axes of the terms, by term id.
    pub term_labels: Vec<String>,
    pub term_axes: Vec<Option<Axis>>,
    /// Terms that only say what kind of game it is ("racing", a family): reasons use them last.
    pub term_plain: Vec<bool>,
    /// Per game: (term id, weight × IDF), sorted by term id, and its length.
    pub vectors: Vec<Vec<(u32, f32)>>,
    pub norms: Vec<f32>,
    /// Per game: its work (an index into `works`); a game without a work is a work of its own.
    pub work_of: Vec<u32>,
    pub works: Vec<Vec<u32>>,
    /// Per game: its first series as a number, for the cap on one series in Echoes.
    pub series_of: Vec<Option<u32>>,
    /// The facets that come from the data.
    data_facets: Vec<FacetValues>,
    /// File -> game.
    by_path: HashMap<PathBuf, u32>,
    /// QID or work key -> work, for "Related: remake".
    pub(crate) by_qid: HashMap<String, u32>,
}

/// Facets whose values keep a fixed order (others are sorted by how many games have them).
fn fixed_order(facet: Facet) -> &'static [&'static str] {
    match facet {
        Facet::Players => &["solo", "multi", "couch-coop", "couch-versus", "party-4p", "pass-and-play", "link-play"],
        Facet::Length => &["length-short", "length-medium", "length-long", "length-epic", "length-endless"],
        Facet::Session => &["session-pickup", "session-sitdown"],
        Facet::Pace => &["pace-calm", "pace-measured", "pace-fast", "pace-frantic"],
        Facet::Difficulty => &["difficulty-easy", "difficulty-moderate", "difficulty-hard", "difficulty-brutal"],
        Facet::Story => &["story-driven", "story-some", "cinematic", "lore", "story-little"],
        Facet::Grinding => &["grinding", "grinding-not-noted"],
        Facet::Reading => &["text-light", "text-heavy"],
        Facet::Status => &["not_started", "playing", "paused", "completed", "dropped"],
        Facet::Rating => &["5", "4", "3", "2", "1", "unrated"],
        Facet::Played => &["never", "month", "long_ago"],
        Facet::Controls => &["dpad-friendly", "simple-controls", "needs-analog", "touch-screen", "two-screens"],
        _ => &[],
    }
}

impl Catalog {
    /// Builds the catalog from the games' features (in the library's order) and the library's systems.
    pub fn build(games: Vec<Features>, systems: Vec<SystemName>) -> Catalog {
        let n = games.len();
        // Terms and their document frequency.
        let mut term_ids: HashMap<String, u32> = HashMap::new();
        let mut term_labels = Vec::new();
        let mut term_axes = Vec::new();
        let mut term_plain = Vec::new();
        let mut df: Vec<u32> = Vec::new();
        let mut raw: Vec<Vec<(u32, f32)>> = Vec::with_capacity(n);
        for g in &games {
            let mut v = Vec::with_capacity(g.terms.len());
            for t in &g.terms {
                let id = *term_ids.entry(t.key.clone()).or_insert_with(|| {
                    term_labels.push(t.label.clone());
                    term_axes.push(t.axis);
                    // Only says what kind of game it is: a family.
                    term_plain.push(t.axis == Some(Axis::Family));
                    df.push(0);
                    (term_labels.len() - 1) as u32
                });
                df[id as usize] += 1;
                v.push((id, t.weight));
            }
            v.sort_by_key(|x| x.0);
            raw.push(v);
        }
        // IDF: a term every game has says little, a rare one a lot.
        let idf: Vec<f32> = df.iter().map(|&d| (1.0 + n as f32 / d.max(1) as f32).ln()).collect();
        let vectors: Vec<Vec<(u32, f32)>> = raw.into_iter().map(|v| v.into_iter().map(|(t, w)| (t, w * idf[t as usize])).collect()).collect();
        let norms = vectors.iter().map(|v| v.iter().map(|(_, w)| w * w).sum::<f32>().sqrt()).collect();

        // Works: the same work key on several files is one game.
        let mut work_ids: HashMap<&str, u32> = HashMap::new();
        let mut works: Vec<Vec<u32>> = Vec::new();
        let mut work_of = Vec::with_capacity(n);
        for (i, g) in games.iter().enumerate() {
            let id = match g.work.as_deref() {
                Some(w) => *work_ids.entry(w).or_insert_with(|| {
                    works.push(Vec::new());
                    (works.len() - 1) as u32
                }),
                None => {
                    works.push(Vec::new());
                    (works.len() - 1) as u32
                }
            };
            works[id as usize].push(i as u32);
            work_of.push(id);
        }
        let mut series_ids: HashMap<&str, u32> = HashMap::new();
        let series_of = games
            .iter()
            .map(|g| {
                g.series.first().map(|s| {
                    let next = series_ids.len() as u32;
                    *series_ids.entry(s.as_str()).or_insert(next)
                })
            })
            .collect();

        // The facets from the data: one bitset per value.
        let mut data_facets = Vec::new();
        for facet in Facet::ALL {
            if matches!(facet, Facet::Status | Facet::Rating | Facet::Played | Facet::Stickers | Facet::Favourites) {
                continue;
            }
            let mut values: Vec<Value> = Vec::new();
            let mut at: HashMap<String, usize> = HashMap::new();
            // Values that read the same are one value ("Kart racing" from Wikidata and from a feel tag); every key
            // still finds it, so a smart shelf saved with either key keeps working.
            let mut by_label: HashMap<String, usize> = HashMap::new();
            let mut aliases: Vec<(String, String)> = Vec::new();
            for (i, g) in games.iter().enumerate() {
                for (f, key, label) in &g.facets {
                    if *f != facet {
                        continue;
                    }
                    let pos = match at.get(key) {
                        Some(&p) => p,
                        None => {
                            let (label, short) = value_labels(facet, key, label, &systems);
                            let p = match by_label.get(&label.to_lowercase()) {
                                Some(&p) if facet != Facet::System => {
                                    aliases.push((key.clone(), values[p].key.clone()));
                                    p
                                }
                                _ => {
                                    by_label.insert(label.to_lowercase(), values.len());
                                    values.push(Value { key: key.clone(), label, short, bits: Bitset::new(n) });
                                    values.len() - 1
                                }
                            };
                            at.insert(key.clone(), p);
                            p
                        }
                    };
                    values[pos].bits.set(i);
                }
            }
            sort_values(facet, &mut values, &systems);
            let mut fv = FacetValues::new(facet, values);
            for (alias, key) in aliases {
                if let Some(p) = fv.position(&key) {
                    fv.by_key.insert(alias, p);
                }
            }
            data_facets.push(fv);
        }
        let by_path = games.iter().enumerate().map(|(i, g)| (g.path.clone(), i as u32)).collect();
        let mut by_qid = HashMap::new();
        for (i, g) in games.iter().enumerate() {
            for q in g.qid.iter().chain(g.work.iter()) {
                by_qid.entry(q.clone()).or_insert(work_of[i]);
            }
        }
        Catalog { games, systems, term_labels, term_axes, term_plain, vectors, norms, work_of, works, series_of, data_facets, by_path, by_qid }
    }

    pub fn len(&self) -> usize {
        self.games.len()
    }

    pub fn is_empty(&self) -> bool {
        self.games.is_empty()
    }

    /// Cosine similarity of two games' term vectors (0 when either has none).
    pub fn cosine(&self, a: usize, b: usize) -> f32 {
        let (na, nb) = (self.norms[a], self.norms[b]);
        if na == 0.0 || nb == 0.0 {
            return 0.0;
        }
        dot(&self.vectors[a], &self.vectors[b]) / (na * nb)
    }

    /// The short name of a system ("SNES").
    pub fn system_short(&self, id: &str) -> String {
        self.systems.iter().find(|s| s.id == id).map_or_else(|| id.to_string(), |s| s.short.clone())
    }
}

/// Dot product of two sparse vectors sorted by term id.
pub(crate) fn dot(a: &[(u32, f32)], b: &[(u32, f32)]) -> f32 {
    let (mut i, mut j, mut sum) = (0, 0, 0.0);
    while i < a.len() && j < b.len() {
        match a[i].0.cmp(&b[j].0) {
            std::cmp::Ordering::Equal => {
                sum += a[i].1 * b[j].1;
                i += 1;
                j += 1;
            }
            std::cmp::Ordering::Less => i += 1,
            std::cmp::Ordering::Greater => j += 1,
        }
    }
    sum
}

/// The label and short label of a value of the data's facets.
fn value_labels(facet: Facet, key: &str, label: &str, systems: &[SystemName]) -> (String, String) {
    match facet {
        Facet::System => match systems.iter().find(|s| s.id == key) {
            Some(s) => (s.name.clone(), s.short.clone()),
            None => (key.to_string(), key.to_string()),
        },
        // Shelf names read "GBA · Action · under 5 hours": these values in lower case after the first.
        Facet::Length
        | Facet::Session
        | Facet::Players
        | Facet::Mood
        | Facet::Controls
        | Facet::Language
        | Facet::Pace
        | Facet::Difficulty
        | Facet::Mechanics
        | Facet::Setting
        | Facet::Story
        | Facet::Grinding
        | Facet::Reading => {
            (label.to_string(), label.to_lowercase())
        }
        _ => (label.to_string(), label.to_string()),
    }
}

/// Values in the order the Lens shows them: systems as the shelf has them, fixed scales in their order, eras by
/// time, everything else by how many games have it.
fn sort_values(facet: Facet, values: &mut [Value], systems: &[SystemName]) {
    let order = fixed_order(facet);
    match facet {
        Facet::System => values.sort_by_key(|v| systems.iter().position(|s| s.id == v.key).unwrap_or(usize::MAX)),
        Facet::Era => values.sort_by(|a, b| a.key.cmp(&b.key)),
        _ if !order.is_empty() => values.sort_by_key(|v| (order.iter().position(|o| *o == v.key).unwrap_or(order.len()), std::cmp::Reverse(v.bits.count()))),
        _ => values.sort_by(|a, b| b.bits.count().cmp(&a.bits.count()).then_with(|| a.label.to_lowercase().cmp(&b.label.to_lowercase()))),
    }
}

/// A sticker as the index needs it.
#[derive(Clone, Debug, PartialEq)]
pub struct StickerName {
    pub id: u32,
    pub name: String,
}

/// What the player did with one game.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UserGame {
    pub status: Status,
    /// Seconds played (the larger of the library's count and the recent games').
    pub secs: u64,
    /// Unix seconds of the last session.
    pub last_played: Option<u64>,
    pub starred: bool,
    pub stickers: Vec<u32>,
    /// The player's rating, 1 to 5; 0: none.
    pub rating: u8,
}

/// The player's side, one entry per game of the catalog, plus the stickers and preferred versions.
#[derive(Clone, Debug, Default)]
pub struct UserState {
    pub games: Vec<UserGame>,
    pub stickers: Vec<StickerName>,
    /// Work key -> the file the player prefers.
    pub preferred: BTreeMap<String, PathBuf>,
    /// Unix seconds, for "played this month".
    pub now: u64,
}

/// Play time that counts in full for the taste profile; more counts no more.
const TASTE_TIME_CAP: f32 = 20.0 * 3600.0;
/// Games of the taste profile kept for naming in reasons.
const TASTE_GAMES: usize = 256;
/// A month, for the Played facet.
const MONTH: u64 = 30 * 24 * 3600;

/// The catalog and the player's state: everything the Lens, smart shelves, Echoes and Versions read.
pub struct Index {
    pub catalog: Arc<Catalog>,
    pub user: UserState,
    pub facets: Vec<FacetValues>,
    /// The taste profile: a weight per term id (negative for what was dropped), and its length.
    pub(crate) taste: Vec<f32>,
    pub(crate) taste_norm: f32,
    /// Games that shaped the profile, with how much (for "Like X, which you finished").
    pub(crate) taste_games: Vec<(u32, f32)>,
    /// Per work: the version lists and Echoes show and start.
    pub(crate) preferred_game: Vec<u32>,
    /// Games of works owned more than once, for counting games rather than files.
    pub(crate) multi: Bitset,
}

/// How much one game counts in the taste profile: time played (capped), the status, a star, the rating (5 stars as much
/// as a star, 4 half that, 3 nothing, 2 and 1 against).
pub fn taste_weight(u: &UserGame) -> f32 {
    let time = (u.secs as f32).min(TASTE_TIME_CAP) / TASTE_TIME_CAP;
    let status = match u.status {
        Status::Completed => 1.0,
        Status::Playing => 0.5,
        Status::Paused => 0.25,
        Status::Dropped => -1.0,
        Status::NotStarted => 0.0,
    };
    let rating = if u.rating == 0 { 0.0 } else { (u.rating.min(5) as f32 - 3.0) * 0.5 };
    time + status + if u.starred { 1.0 } else { 0.0 } + rating
}

impl Index {
    /// Puts the catalog and the player's state together. `user.games` must be as long as the catalog (missing
    /// entries count as untouched games).
    pub fn build(catalog: Arc<Catalog>, mut user: UserState) -> Index {
        let n = catalog.len();
        user.games.resize(n, UserGame::default());
        let mut facets: Vec<FacetValues> = Vec::with_capacity(Facet::ALL.len());
        for facet in Facet::ALL {
            let fv = match facet {
                Facet::Status => {
                    let values = Status::ALL
                        .iter()
                        .map(|s| {
                            let mut bits = Bitset::new(n);
                            user.games.iter().enumerate().filter(|(_, u)| u.status == *s).for_each(|(i, _)| bits.set(i));
                            Value { key: status_key(*s).into(), label: s.label().into(), short: s.label().to_lowercase(), bits }
                        })
                        .filter(|v| !v.bits.is_empty())
                        .collect();
                    FacetValues::new(facet, values)
                }
                Facet::Rating => {
                    // "5 stars" down to "1 star", then the games without one; a value nobody has is left out, except
                    // "Not rated", so the facet is there (with its note) before the first rating.
                    let values = (1..=5u8)
                        .rev()
                        .map(|r| (r.to_string(), if r == 1 { "1 star".to_string() } else { format!("{r} stars") }, format!("rated {r}"), r))
                        .chain(std::iter::once(("unrated".to_string(), "Not rated".to_string(), "not rated".to_string(), 0)))
                        .map(|(key, label, short, r)| {
                            let mut bits = Bitset::new(n);
                            user.games.iter().enumerate().filter(|(_, u)| u.rating.min(5) == r).for_each(|(i, _)| bits.set(i));
                            Value { key, label, short, bits }
                        })
                        .filter(|v| !v.bits.is_empty() || v.key == "unrated" && n > 0)
                        .collect();
                    FacetValues::new(facet, values)
                }
                Facet::Played => {
                    let mut never = Bitset::new(n);
                    let mut month = Bitset::new(n);
                    let mut long_ago = Bitset::new(n);
                    for (i, u) in user.games.iter().enumerate() {
                        match u.last_played {
                            Some(t) if user.now.saturating_sub(t) <= MONTH => month.set(i),
                            Some(_) => long_ago.set(i),
                            None if u.secs > 0 => long_ago.set(i),
                            None => never.set(i),
                        }
                    }
                    let values = [("never", "Never", "never played", never), ("month", "This month", "played this month", month), ("long_ago", "Long ago", "played long ago", long_ago)]
                        .into_iter()
                        .filter(|(_, _, _, b)| !b.is_empty())
                        .map(|(k, l, s, bits)| Value { key: k.into(), label: l.into(), short: s.into(), bits })
                        .collect();
                    FacetValues::new(facet, values)
                }
                Facet::Stickers => {
                    let values = user
                        .stickers
                        .iter()
                        .map(|st| {
                            let mut bits = Bitset::new(n);
                            user.games.iter().enumerate().filter(|(_, u)| u.stickers.contains(&st.id)).for_each(|(i, _)| bits.set(i));
                            Value { key: st.id.to_string(), label: st.name.clone(), short: st.name.clone(), bits }
                        })
                        .collect();
                    FacetValues::new(facet, values)
                }
                Facet::Favourites => {
                    let mut bits = Bitset::new(n);
                    user.games.iter().enumerate().filter(|(_, u)| u.starred).for_each(|(i, _)| bits.set(i));
                    let values = if bits.is_empty() { Vec::new() } else { vec![Value { key: "starred".into(), label: "Starred".into(), short: "starred".into(), bits }] };
                    FacetValues::new(facet, values)
                }
                _ => catalog.data_facets.iter().find(|f| f.facet == facet).cloned().unwrap_or_else(|| FacetValues::new(facet, Vec::new())),
            };
            facets.push(fv);
        }

        // The taste profile: each game's unit vector times how much it counts.
        let mut taste: BTreeMap<u32, f32> = BTreeMap::new();
        let mut taste_games = Vec::new();
        for (i, u) in user.games.iter().enumerate() {
            let w = taste_weight(u);
            if w == 0.0 || catalog.norms[i] == 0.0 {
                continue;
            }
            taste_games.push((i as u32, w));
            for (t, x) in &catalog.vectors[i] {
                *taste.entry(*t).or_default() += w * x / catalog.norms[i];
            }
        }
        let mut dense = vec![0.0f32; catalog.term_labels.len()];
        for (t, w) in taste {
            dense[t as usize] = w;
        }
        let taste = dense;
        // For "Like X": the games that count most are enough.
        taste_games.sort_by(|a: &(u32, f32), b| b.1.total_cmp(&a.1));
        taste_games.truncate(TASTE_GAMES);
        let taste_norm = taste.iter().map(|w| w * w).sum::<f32>().sqrt();

        let mut multi = Bitset::new(n);
        for w in catalog.works.iter().filter(|w| w.len() > 1) {
            w.iter().for_each(|&g| multi.set(g as usize));
        }
        let mut index = Index { catalog, user, facets, taste, taste_norm, taste_games, preferred_game: Vec::new(), multi };
        index.preferred_game = (0..index.catalog.works.len()).map(|w| index.choose_preferred(w)).collect();
        index
    }

    pub fn len(&self) -> usize {
        self.catalog.len()
    }

    pub fn is_empty(&self) -> bool {
        self.catalog.is_empty()
    }

    pub fn facet(&self, facet: Facet) -> &FacetValues {
        &self.facets[facet.index()]
    }

    /// The facets that have something to offer, in the Lens's order.
    pub fn shown_facets(&self) -> Vec<Facet> {
        self.facets.iter().filter(|f| !f.values.is_empty()).map(|f| f.facet).collect()
    }

    /// How much the player's taste leans towards a game: cosine of the profile and the game's vector, -1..1.
    pub fn affinity(&self, game: usize) -> f32 {
        let norm = self.catalog.norms[game];
        if self.taste_norm == 0.0 || norm == 0.0 {
            return 0.0;
        }
        self.catalog.vectors[game].iter().map(|(t, w)| self.taste[*t as usize] * w).sum::<f32>() / (self.taste_norm * norm)
    }

    pub fn game(&self, i: usize) -> &Features {
        &self.catalog.games[i]
    }

    /// Finds a game by its file.
    pub fn find(&self, path: &std::path::Path) -> Option<usize> {
        self.catalog.by_path.get(path).map(|&i| i as usize)
    }
}

/// The key of a status in the Status facet.
pub fn status_key(s: Status) -> &'static str {
    match s {
        Status::NotStarted => "not_started",
        Status::Playing => "playing",
        Status::Paused => "paused",
        Status::Completed => "completed",
        Status::Dropped => "dropped",
    }
}
