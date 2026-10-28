//! "Surprise me" (docs/CURATOR.md "Surprise me"): one game of the library picked for the player, with one line why.
//!
//! Every work is a candidate once (its preferred version). Its chance is the product of:
//! - **status:** not started and started-but-unfinished (Playing, Paused) count in full, Completed a sixth, Dropped a
//!   twentieth: finished and given-up games come up, but rarely;
//! - **recency:** played in the last day a tenth, the last week a third, the last month two thirds, longer ago or never
//!   in full; an unfinished game left for over a month a little more (it is waiting for you);
//! - **rating:** 4 or 5 stars a little more, 1 or 2 stars much less;
//! - **taste:** with a taste profile, × (1 + affinity), at least a third: games like the ones you play come up more.
//!
//! The draw is a seeded generator ([`Rng`]), so a test gets the same pick every time; the app seeds it from the clock.
//! The reason is made from what is true of the pick, not from what made the draw: how long since it was played (or
//! that it never was, or that it is unfinished), then the rating or the taste ("you like platformers").

use crate::library_meta::Status;

use super::features::Axis;
use super::index::Index;

const DAY: u64 = 24 * 3600;
/// An affinity at least this large is said in the reason ("you like …").
const TASTE_SAID_FROM: f32 = 0.2;

/// A small, fast generator (SplitMix64): good enough for a draw, the same sequence for the same seed.
#[derive(Clone, Debug)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng(seed)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A number in 0..1.
    pub fn unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// The game "Surprise me" picked and why.
#[derive(Clone, Debug, PartialEq)]
pub struct Pick {
    /// The catalog's number of the game.
    pub game: usize,
    /// "Not played for 3 months · you like platformers".
    pub reason: String,
}

impl Index {
    /// How likely game `g` is to be picked (0: never). See the module's comment.
    pub fn surprise_weight(&self, g: usize) -> f64 {
        let u = &self.user.games[g];
        let status = match u.status {
            Status::NotStarted | Status::Playing | Status::Paused => 1.0,
            Status::Completed => 1.0 / 6.0,
            Status::Dropped => 0.05,
        };
        let age = u.last_played.map(|t| self.user.now.saturating_sub(t));
        let unfinished = matches!(u.status, Status::Playing | Status::Paused);
        let recency = match age {
            Some(a) if a < DAY => 0.1,
            Some(a) if a < 7 * DAY => 1.0 / 3.0,
            Some(a) if a < 30 * DAY => 2.0 / 3.0,
            Some(_) if unfinished => 1.3,
            _ => 1.0,
        };
        let rating = match u.rating {
            4 | 5 => 1.3,
            1 => 0.2,
            2 => 0.4,
            _ => 1.0,
        };
        let taste = if self.taste_norm > 0.0 { (1.0 + self.affinity(g) as f64).max(1.0 / 3.0) } else { 1.0 };
        status * recency * rating * taste
    }

    /// Picks a game: each work once, by [`Index::surprise_weight`], leaving out `skip` (the picks shown already, so
    /// "Another" brings something new) unless nothing else is left. None for an empty library.
    pub fn surprise(&self, rng: &mut Rng, skip: &[usize]) -> Option<Pick> {
        let candidates: Vec<(usize, f64)> = (0..self.catalog.works.len())
            .map(|w| self.preferred_game[w] as usize)
            .map(|g| (g, self.surprise_weight(g)))
            .filter(|&(_, w)| w > 0.0)
            .collect();
        let fresh: Vec<(usize, f64)> = candidates.iter().copied().filter(|(g, _)| !skip.iter().any(|s| self.catalog.work_of[*s] == self.catalog.work_of[*g])).collect();
        let pool = if fresh.is_empty() { &candidates } else { &fresh };
        let total: f64 = pool.iter().map(|(_, w)| w).sum();
        if pool.is_empty() || total <= 0.0 {
            return None;
        }
        let mut at = rng.unit() * total;
        let game = pool.iter().find(|(_, w)| {
            at -= w;
            at < 0.0
        });
        let game = game.unwrap_or(&pool[pool.len() - 1]).0;
        Some(Pick { game, reason: self.surprise_reason(game) })
    }

    /// The one line why: when it was last played (or that it never was), then a rating of 4 or 5 or what the taste
    /// profile likes in it. "Never played · you like platformers", "Started, not played for 3 months", "Not played for
    /// 2 years · you gave it 5 stars".
    pub fn surprise_reason(&self, g: usize) -> String {
        let u = &self.user.games[g];
        let when = match (u.last_played.map(|t| self.user.now.saturating_sub(t)), u.secs) {
            (None, 0) => "Never played".to_string(),
            (None, _) => "Not played for a long time".to_string(),
            (Some(a), _) => since(a),
        };
        let first = match u.status {
            Status::Playing | Status::Paused if u.last_played.is_some() || u.secs > 0 => format!("Started, {}", lower_first(&when)),
            Status::Completed => format!("Finished, {}", lower_first(&when)),
            _ => when,
        };
        let second = match u.rating {
            5 => Some("you gave it 5 stars".to_string()),
            4 => Some("you gave it 4 stars".to_string()),
            _ => self.liked_kind(g).map(|k| format!("you like {}", kind_of_games(&k))),
        };
        match second {
            Some(s) => format!("{first} · {s}"),
            None => first,
        }
    }

    /// The kind of game (a family or sub-genre label) of `g` that the taste profile leans to most, when it leans to `g`
    /// at all: "platformer", "Japanese RPG".
    fn liked_kind(&self, g: usize) -> Option<String> {
        if self.taste_norm == 0.0 || self.affinity(g) < TASTE_SAID_FROM {
            return None;
        }
        let cat = &self.catalog;
        cat.vectors[g]
            .iter()
            .filter(|(t, _)| matches!(cat.term_axes[*t as usize], Some(Axis::Family | Axis::SubGenre)))
            .map(|(t, w)| (*t, self.taste[*t as usize] * w))
            .filter(|(_, x)| *x > 0.0)
            .max_by(|a, b| a.1.total_cmp(&b.1).then(b.0.cmp(&a.0)))
            .map(|(t, _)| cat.term_labels[t as usize].clone())
    }
}

/// "Played today", "Played 3 days ago", "Not played for 2 weeks", "Not played for 3 months", "Not played for 2 years".
pub fn since(secs: u64) -> String {
    let days = secs / DAY;
    match days {
        0 => "Played today".to_string(),
        1 => "Played yesterday".to_string(),
        2..=13 => format!("Played {days} days ago"),
        14..=59 => format!("Not played for {} weeks", days / 7),
        60..=729 => format!("Not played for {} months", days / 30),
        _ => format!("Not played for {} years", days / 365),
    }
}

/// "platformer games", "Japanese RPGs", "party games": a kind of game as the end of "you like …".
pub fn kind_of_games(label: &str) -> String {
    if label.ends_with("RPG") || label.ends_with(" game") {
        format!("{label}s")
    } else {
        format!("{label} games")
    }
}

fn lower_first(s: &str) -> String {
    let mut c = s.chars();
    c.next().map_or_else(String::new, |f| f.to_lowercase().chain(c).collect())
}
