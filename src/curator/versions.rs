//! Versions: one game across systems and files. Games are grouped by work (today the gamedb QID: the same QID on
//! several systems is the same work; later gamedb's "work" key, which also joins ports with their own item). The
//! player's preferred version is what mixed lists show and start.

use super::index::Index;

impl Index {
    /// The versions of the work `game` belongs to, the game itself included, in library order.
    pub fn versions(&self, game: usize) -> &[u32] {
        &self.catalog.works[self.catalog.work_of[game] as usize]
    }

    /// Whether the player owns two or more versions of this game.
    pub fn has_versions(&self, game: usize) -> bool {
        self.versions(game).len() > 1
    }

    /// The version of `game`'s work that lists show and start.
    pub fn preferred(&self, game: usize) -> usize {
        self.preferred_game[self.catalog.work_of[game] as usize] as usize
    }

    /// Whether the player chose the preferred version themselves (Y on the Versions page).
    pub fn preferred_is_chosen(&self, game: usize) -> bool {
        let g = self.game(self.preferred(game));
        g.work.as_ref().and_then(|w| self.user.preferred.get(w)).is_some_and(|p| *p == g.path)
    }

    /// The preferred version of work `w`: the player's choice while that file is in the library, else the version
    /// played longest, then the one played last, then one in a language from the file name with English first
    /// (a fan translation counts), then the first in the library.
    pub(crate) fn choose_preferred(&self, w: usize) -> u32 {
        let members = &self.catalog.works[w];
        let first = members[0];
        if members.len() == 1 {
            return first;
        }
        if let Some(path) = self.catalog.games[first as usize].work.as_ref().and_then(|k| self.user.preferred.get(k)) {
            if let Some(&m) = members.iter().find(|&&m| self.catalog.games[m as usize].path == *path) {
                return m;
            }
        }
        *members
            .iter()
            .max_by_key(|&&m| {
                let u = &self.user.games[m as usize];
                let english = self.catalog.games[m as usize].rom.readable_in("en") == Some(true);
                (u.secs, u.last_played.unwrap_or(0), english, std::cmp::Reverse(m))
            })
            .unwrap_or(&first)
    }

    /// Games in the library that are a remake or remaster of `game`, or that `game` remakes (gamedb's `related`):
    /// "Related: remake". Each work once, its preferred version, in library order.
    pub fn remakes(&self, game: usize) -> Vec<usize> {
        let own = self.catalog.work_of[game];
        let mut works: Vec<u32> = self.catalog.games[game].remakes.iter().filter_map(|q| self.catalog.by_qid.get(q).copied()).collect();
        // The link may be on the other side only: a remake that names this game.
        let mine: Vec<&String> = self.catalog.games[game].qid.iter().chain(self.catalog.games[game].work.iter()).collect();
        for (i, g) in self.catalog.games.iter().enumerate() {
            if g.remakes.iter().any(|q| mine.contains(&q)) {
                works.push(self.catalog.work_of[i]);
            }
        }
        works.sort_unstable();
        works.dedup();
        works.into_iter().filter(|w| *w != own).map(|w| self.preferred_game[w as usize] as usize).collect()
    }

    /// A list that mixes systems, with each work once: the preferred version when it is in the list, else the
    /// list's first entry of that work (a shelf filtered to the DS shows the DS version, Favorites the file the
    /// player starred). Keeps the order of first appearance.
    pub fn merge_versions(&self, games: &[usize]) -> Vec<usize> {
        let mut listed = super::bits::Bitset::new(self.len());
        games.iter().for_each(|&g| listed.set(g));
        let mut seen = vec![false; self.catalog.works.len()];
        let mut out = Vec::with_capacity(games.len());
        for &g in games {
            let w = self.catalog.work_of[g] as usize;
            if std::mem::replace(&mut seen[w], true) {
                continue;
            }
            let pref = self.preferred_game[w] as usize;
            out.push(if listed.get(pref) { pref } else { g });
        }
        out
    }
}
