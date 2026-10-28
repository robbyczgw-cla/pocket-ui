//! What Home's Curator row offers today (docs/CURATOR.md "Curator on Home"): the day's pick with its reason, and the
//! games waiting for you. Worked out on the index thread with every new index (`app_curator`), never while painting.
//!
//! - **The day's pick** is a Surprise me draw ([`Index::surprise`]) seeded by the day, so it stays the same all day and
//!   between starts, and changes at midnight (UTC) or when the library or the player's state changes the weights.
//! - **Waiting**: the unfinished games you haven't touched for a month ("to rediscover"); when there are none, the
//!   games never played. Counted and shown each work once (its preferred version), the most likely picks first; the
//!   day's pick is not among the covers, so no cover shows twice, but it is in the count, so "15 never played" is the
//!   number the Lens gives for Played → Never (the first-day run, 2026-10-26, saw 14 on Home and 15 there).

use crate::library_meta::Status;

use super::index::Index;
use super::surprise::{Pick, Rng};

const DAY: u64 = 24 * 3600;
/// An unfinished game untouched this long is "to rediscover".
pub const REDISCOVER_AFTER: u64 = 30 * DAY;
/// Waiting games whose covers are shown.
pub const WAITING_SHOWN: usize = 3;

/// Which games are waiting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Waiting {
    /// Started, not finished, not played for [`REDISCOVER_AFTER`].
    Rediscover,
    /// Never played.
    NotStarted,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Today {
    /// The day's pick; `None` with no games.
    pub pick: Option<Pick>,
    /// Which games wait, how many works (the day's pick too, when it is one of them), and the first [`WAITING_SHOWN`]
    /// of the others (catalog numbers).
    pub waiting: Option<(Waiting, usize, Vec<usize>)>,
}

impl Index {
    /// The day's pick and the waiting games, for the player's clock (`user.now`).
    pub fn today(&self) -> Today {
        let day = self.user.now / DAY;
        let mut rng = Rng::new(day.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0x0074_6f64_6179);
        let pick = self.surprise(&mut rng, &[]);
        let picked_work = pick.as_ref().map(|p| self.catalog.work_of[p.game]);
        let works: Vec<usize> =
            (0..self.catalog.works.len()).filter(|w| Some(*w as u32) != picked_work).map(|w| self.preferred_game[w] as usize).collect();
        let waiting = [Waiting::Rediscover, Waiting::NotStarted].into_iter().find_map(|kind| {
            let mut games: Vec<(usize, f64)> = works.iter().copied().filter(|&g| self.waits(g, kind)).map(|g| (g, self.surprise_weight(g))).collect();
            if games.is_empty() {
                return None;
            }
            games.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
            let pick_waits = pick.as_ref().is_some_and(|p| self.waits(p.game, kind));
            Some((kind, games.len() + usize::from(pick_waits), games.iter().take(WAITING_SHOWN).map(|(g, _)| *g).collect()))
        });
        Today { pick, waiting }
    }

    fn waits(&self, g: usize, kind: Waiting) -> bool {
        let u = &self.user.games[g];
        let played = u.secs > 0 || u.last_played.is_some();
        match kind {
            Waiting::Rediscover => {
                let unfinished = matches!(u.status, Status::Playing | Status::Paused) || (u.status == Status::NotStarted && played);
                let long_ago = u.last_played.is_none_or(|t| self.user.now.saturating_sub(t) >= REDISCOVER_AFTER);
                unfinished && played && long_ago
            }
            Waiting::NotStarted => u.status == Status::NotStarted && !played,
        }
    }
}
