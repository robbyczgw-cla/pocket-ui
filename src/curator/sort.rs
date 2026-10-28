//! The order of a Lens's games and of a smart shelf (docs/CURATOR.md "Sort orders and My rating"). Name is the order
//! lists always had; the others put first what the player asks for and every game without the figure last, in name
//! order, so a game is never left out for want of data. Only the list is sorted, after the Lens has picked it: the
//! Lens's keypresses (counts, bitsets) don't change.

use serde::{Deserialize, Serialize};

use super::bits::Bitset;
use super::features::Facet;
use super::index::Index;

/// How a list of games is sorted.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Order {
    /// By name, then system (as every list of the shelf).
    #[default]
    Name,
    /// The last played first; never played after them.
    LastPlayed,
    /// The most played first; not played after them.
    PlayTime,
    /// The player's best rated first; not rated after them.
    Rating,
    /// The oldest first, by Wikidata's year; without a year after them.
    Year,
    /// The shortest first, by Curator's length estimate (endless and score-attack games after the longest); without an
    /// estimate after them.
    Length,
}

impl Order {
    pub const ALL: [Order; 6] = [Order::Name, Order::LastPlayed, Order::PlayTime, Order::Rating, Order::Year, Order::Length];

    /// The choice in the order sheet.
    pub fn label(self) -> &'static str {
        match self {
            Order::Name => "Name",
            Order::LastPlayed => "Last played first",
            Order::PlayTime => "Most played first",
            Order::Rating => "Best rated first",
            Order::Year => "Oldest first",
            Order::Length => "Shortest first",
        }
    }

    /// What a list's title adds ("Short RPGs · last played first"); nothing for the usual order by name.
    pub fn suffix(self) -> Option<&'static str> {
        match self {
            Order::Name => None,
            Order::LastPlayed => Some("last played first"),
            Order::PlayTime => Some("most played first"),
            Order::Rating => Some("best rated first"),
            Order::Year => Some("oldest first"),
            Order::Length => Some("shortest first"),
        }
    }

    pub fn is_name(&self) -> bool {
        *self == Order::Name
    }
}

/// Length's buckets, shortest first; endless games after the longest.
const LENGTHS: [&str; 5] = ["length-short", "length-medium", "length-long", "length-epic", "length-endless"];

impl Index {
    /// The games of a bitset as lists show them ([`Index::list`]: by name, each work once with `merge`), then in `order`.
    /// Ties and games without the figure keep the order by name.
    pub fn sorted(&self, bits: &Bitset, merge: bool, order: Order) -> Vec<usize> {
        let mut games = self.list(bits, merge);
        self.sort_games(&mut games, order, merge);
        games
    }

    /// Sorts games of the catalog in `order` (stable: games that compare the same keep their places). With `merge` a game
    /// stands for its work: last played is the latest of its versions, time played their sum, rating the highest.
    pub fn sort_games(&self, games: &mut [usize], order: Order, merge: bool) {
        use std::cmp::Reverse;
        match order {
            Order::Name => {}
            Order::LastPlayed => games.sort_by_cached_key(|&g| {
                let t = self.played(g, merge).0;
                (t.is_none(), Reverse(t.unwrap_or(0)))
            }),
            Order::PlayTime => games.sort_by_cached_key(|&g| {
                let secs = self.played(g, merge).1;
                (secs == 0, Reverse(secs))
            }),
            Order::Rating => games.sort_by_cached_key(|&g| {
                let r = self.played(g, merge).2;
                (r == 0, Reverse(r))
            }),
            Order::Year => games.sort_by_cached_key(|&g| {
                let y = self.game(g).year;
                (y.is_none(), y.unwrap_or(0))
            }),
            Order::Length => games.sort_by_cached_key(|&g| {
                let at = self.game(g).facets.iter().filter(|(f, _, _)| *f == Facet::Length).filter_map(|(_, k, _)| LENGTHS.iter().position(|l| l == k)).min();
                (at.is_none(), at.unwrap_or(0))
            }),
        }
    }

    /// (last played, seconds played, rating) of a game, or with `merge` of all versions of its work together.
    fn played(&self, g: usize, merge: bool) -> (Option<u64>, u64, u8) {
        let one = |g: usize| {
            let u = &self.user.games[g];
            (u.last_played, u.secs, u.rating)
        };
        if !merge || !self.multi.get(g) {
            return one(g);
        }
        self.catalog.works[self.catalog.work_of[g] as usize].iter().map(|&v| one(v as usize)).fold((None, 0, 0), |a, b| (a.0.max(b.0), a.1 + b.1, a.2.max(b.2)))
    }
}
