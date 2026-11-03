//! "Just Play" (docs/CURATOR.md "Just Play"): one press on Home starts a suitable game, nothing to choose.
//!
//! Built on "Surprise me": the same chance per game ([`Index::surprise_weight`]: not launched a moment ago, a rating, the
//! taste profile), the same seeded generator and the same preferred version of a work. What is new is the order of who
//! may be picked, in four tiers; a later tier is looked at only when the ones before it have nobody:
//! 1. **Unfinished:** Playing or Paused, and games with play time that are still marked Not started.
//! 2. **Unplayed favourites:** starred, no play time.
//! 3. **Unplayed:** never started, no play time.
//! 4. **The rest:** Completed and Dropped games (never guessed from play time), only when nothing else is left.
//!
//! The game played last is left out while anybody else can be picked in tiers 1 to 3; it comes back (an open game is
//! better than a repeat of nothing) before Completed and Dropped games are looked at. Hidden games are not asked about at all (`allowed`).
//! In tiers 3 and 4 a game of a system this handheld runs slowly (`system_notes`) counts a fifth, so a press does not
//! throw a 20-second boot at somebody who has not chosen it; an unfinished or starred game was chosen already.
//! A pick that does not start (`startable`: the file, and the emulator and core its route names) is dropped and the draw
//! repeated, at most [`TRIES`] checks in all. A version that does not start does not hide another version of the same work
//! that does (the preferred one is tried first, then any other the player may be offered); a work is still one chance.

use crate::library_meta::Status;

use super::index::Index;
use super::surprise::Rng;

/// Startability checks (a file stat each) before it is said that nothing starts.
pub const TRIES: usize = 8;
/// What a slow system's game counts in tiers 3 and 4.
const HEAVY: f64 = 0.2;

/// Which tier the pick came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Why {
    Unfinished,
    Favorite,
    Unplayed,
    Other,
}

/// Why there is no pick.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Miss {
    /// No game the player may be offered (an empty library, or every game hidden).
    NoGames,
    /// There are games, but the ones drawn would not start (file gone, no route).
    NothingStarts,
}

/// What the library may offer, asked per game of the catalog.
pub struct Offer<'a> {
    /// Not hidden by the player (the game, its system).
    pub allowed: &'a dyn Fn(usize) -> bool,
    /// A system this handheld runs slowly.
    pub heavy: &'a dyn Fn(usize) -> bool,
}

impl Index {
    /// The game Just Play starts: see the module's comment. `startable` is asked of the drawn game only.
    pub fn just_play(&self, rng: &mut Rng, offer: &Offer, startable: &mut dyn FnMut(usize) -> bool) -> Result<(usize, Why), Miss> {
        let works = &self.catalog.works;
        let last_work = self.user.games.iter().enumerate().filter_map(|(g, u)| u.last_played.map(|t| (t, g))).max().map(|(_, g)| self.catalog.work_of[g]);
        // Per tier: (game, chance, work). Each work once, its preferred version, else the first it may offer.
        let mut tiers: [Vec<(usize, f64, u32)>; 4] = Default::default();
        for (w, members) in works.iter().enumerate() {
            let pref = self.preferred_game[w] as usize;
            let Some(g) = std::iter::once(pref).chain(members.iter().map(|&m| m as usize)).find(|&m| (offer.allowed)(m)) else { continue };
            let u = &self.user.games[g];
            let played = u.secs > 0 || u.last_played.is_some();
            let tier = match u.status {
                Status::Playing | Status::Paused => 0,
                Status::NotStarted if played => 0,
                Status::NotStarted if u.starred => 1,
                Status::NotStarted => 2,
                Status::Completed | Status::Dropped => 3,
            };
            let mut chance = self.surprise_weight(g);
            if tier >= 2 && (offer.heavy)(g) {
                chance *= HEAVY;
            }
            if chance > 0.0 {
                tiers[tier].push((g, chance, w as u32));
            }
        }
        if tiers.iter().all(Vec::is_empty) {
            return Err(Miss::NoGames);
        }
        let mut tries = 0;
        let mut bad: Vec<usize> = Vec::new();
        // The tiers 1-3 first, then 4 (Completed, Dropped); in each, without the game played last first, and only if that
        // leaves nobody with it: a repeat of an open game beats a game the player put away.
        for (open, keep_last) in [(0..3, false), (0..3, true), (3..4, false), (3..4, true)] {
            for t in open {
                let mut pool: Vec<(usize, f64, u32)> = tiers[t].iter().filter(|(g, _, w)| (keep_last || Some(*w) != last_work) && !bad.contains(g)).copied().collect();
                while !pool.is_empty() && tries < TRIES {
                    let total: f64 = pool.iter().map(|(_, c, _)| c).sum();
                    let mut at = rng.unit() * total;
                    let i = pool.iter().position(|(_, c, _)| {
                        at -= c;
                        at < 0.0
                    });
                    let i = i.unwrap_or(pool.len() - 1);
                    let (g, _, w) = pool[i];
                    let why = [Why::Unfinished, Why::Favorite, Why::Unplayed, Why::Other][t];
                    tries += 1;
                    if startable(g) {
                        return Ok((g, why));
                    }
                    bad.push(g);
                    // A version that does not start must not hide another one of the same work that does: the others it
                    // may offer, the preferred first, still one pick per work.
                    let pref = self.preferred_game[w as usize] as usize;
                    let others = std::iter::once(pref).chain(works[w as usize].iter().map(|&m| m as usize));
                    for alt in others.collect::<Vec<_>>() {
                        if tries >= TRIES {
                            break;
                        }
                        if bad.contains(&alt) || !(offer.allowed)(alt) {
                            continue;
                        }
                        tries += 1;
                        if startable(alt) {
                            return Ok((alt, why));
                        }
                        bad.push(alt);
                    }
                    pool.swap_remove(i);
                }
            }
        }
        Err(Miss::NothingStarts)
    }
}
