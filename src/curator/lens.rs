//! Lens: filters combined across facets. AND across facets, OR within one; the count for every value is a popcount
//! of its bitset with the games the other facets let through, so it says what picking that value would leave.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::bits::Bitset;
use super::features::Facet;
use super::index::Index;

/// The picked values: facet id -> value keys. Saved as is in a smart shelf.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lens {
    #[serde(default)]
    pub picks: BTreeMap<String, Vec<String>>,
}

impl Lens {
    pub fn is_empty(&self) -> bool {
        self.picks.values().all(Vec::is_empty)
    }

    pub fn picked(&self, facet: Facet, key: &str) -> bool {
        self.picks.get(facet.id()).is_some_and(|v| v.iter().any(|k| k == key))
    }

    /// The keys picked in a facet.
    pub fn keys(&self, facet: Facet) -> &[String] {
        self.picks.get(facet.id()).map_or(&[], Vec::as_slice)
    }

    /// Picks a value, or drops it when it was picked.
    pub fn toggle(&mut self, facet: Facet, key: &str) {
        let keys = self.picks.entry(facet.id().to_string()).or_default();
        match keys.iter().position(|k| k == key) {
            Some(i) => {
                keys.remove(i);
            }
            None => keys.push(key.to_string()),
        }
        if keys.is_empty() {
            self.picks.remove(facet.id());
        }
    }

    /// Y: drops every pick of the facet.
    pub fn clear(&mut self, facet: Facet) {
        self.picks.remove(facet.id());
    }

    /// Drops a value everywhere it is picked (a sticker that was deleted).
    pub fn forget(&mut self, facet: Facet, key: &str) {
        if let Some(keys) = self.picks.get_mut(facet.id()) {
            keys.retain(|k| k != key);
            if keys.is_empty() {
                self.picks.remove(facet.id());
            }
        }
    }
}

impl Index {
    /// The games one facet's picks let through: the OR of their bitsets. None when the facet has no picks. A key the
    /// index doesn't know (a sticker since deleted) matches nothing.
    fn facet_bits(&self, lens: &Lens, facet: Facet) -> Option<Bitset> {
        let keys = lens.keys(facet);
        if keys.is_empty() {
            return None;
        }
        let fv = self.facet(facet);
        let mut bits = Bitset::new(self.len());
        for k in keys {
            if let Some(v) = fv.find(k) {
                bits.or_with(&v.bits);
            }
        }
        Some(bits)
    }

    /// The games the Lens lets through, leaving out the picks of `skip` (for that facet's counts).
    fn matches_skipping(&self, lens: &Lens, skip: Option<Facet>) -> Bitset {
        let mut out = Bitset::full(self.len());
        for facet in Facet::ALL {
            if Some(facet) == skip {
                continue;
            }
            if let Some(bits) = self.facet_bits(lens, facet) {
                out.and_with(&bits);
            }
        }
        out
    }

    /// The games the Lens lets through.
    pub fn matches(&self, lens: &Lens) -> Bitset {
        self.matches_skipping(lens, None)
    }

    /// For each value of `facet`: how many games there would be with that value picked too (OR within the facet, so
    /// the other facets' picks only). `merge`: a game owned on several systems counts once, as lists show it.
    pub fn counts(&self, lens: &Lens, facet: Facet, merge: bool) -> Vec<u32> {
        let base = self.matches_skipping(lens, Some(facet));
        self.facet(facet)
            .values
            .iter()
            .map(|v| {
                if merge {
                    let mut both = v.bits.clone();
                    both.and_with(&base);
                    self.count(&both, true)
                } else {
                    v.bits.and_count(&base)
                }
            })
            .collect()
    }

    /// How many games a bitset holds: files, or with `merge` each game owned on several systems once.
    pub fn count(&self, bits: &Bitset, merge: bool) -> u32 {
        if !merge {
            return bits.count();
        }
        // Files of games owned once count as they are; of the others, each work once.
        let mut single = bits.clone();
        let mut versions = bits.clone();
        versions.and_with(&self.multi);
        let doubled: std::collections::HashSet<u32> = versions.ones().map(|g| self.catalog.work_of[g]).collect();
        single.and_not(&self.multi);
        single.count() + doubled.len() as u32
    }

    /// The games of a bitset in the order lists show them: by name, then system; each work once when `merge`.
    pub fn list(&self, bits: &Bitset, merge: bool) -> Vec<usize> {
        let mut games: Vec<usize> = bits.ones().collect();
        games.sort_by_cached_key(|&g| {
            let f = self.game(g);
            (f.name.to_lowercase(), f.system.clone())
        });
        if merge {
            self.merge_versions(&games)
        } else {
            games
        }
    }

    /// A name for the Lens as a smart shelf: the picked values in facet order, "GBA · Racing · 5–15 h".
    pub fn suggest_name(&self, lens: &Lens) -> String {
        let mut parts: Vec<String> = Vec::new();
        for facet in Facet::ALL {
            let words = self.pick_words(lens, facet);
            if !words.is_empty() {
                parts.push(words.join(" or "));
            }
        }
        if parts.is_empty() {
            return "All games".into();
        }
        let mut name = super::features::capitalise(&parts.iter().take(3).cloned().collect::<Vec<_>>().join(" · "));
        if parts.len() > 3 {
            name.push_str(" …");
        }
        name
    }

    /// One line for a facet's picks on the Lens's left column: "GBA, SNES" or "".
    pub fn picks_line(&self, lens: &Lens, facet: Facet) -> String {
        self.pick_words(lens, facet).join(", ")
    }

    /// A facet's picks as words, in the facet's order: each value's short label, and Length's neighbouring steps
    /// as one range ("under 5 hours" and "5–15 hours" read "under 15 hours").
    fn pick_words(&self, lens: &Lens, facet: Facet) -> Vec<String> {
        let fv = self.facet(facet);
        let mut at: Vec<usize> = lens.keys(facet).iter().filter_map(|k| fv.position(k)).collect();
        at.sort_unstable();
        at.dedup();
        let keys: Vec<&str> = at.iter().map(|&i| fv.values[i].key.as_str()).collect();
        if facet == Facet::Length {
            if let Some(range) = length_range(&keys) {
                let endless = keys.contains(&"length-endless").then(|| fv.find("length-endless").map(|v| v.short.clone())).flatten();
                return std::iter::once(range).chain(endless).collect();
            }
        }
        if facet == Facet::Rating {
            if let Some(range) = rating_range(&keys) {
                let unrated = keys.contains(&"unrated").then(|| fv.find("unrated").map(|v| v.short.clone())).flatten();
                return std::iter::once(range).chain(unrated).collect();
            }
        }
        at.iter().map(|&i| fv.values[i].short.clone()).collect()
    }
}

/// Two or more ratings picked from 5 stars down without a gap ("Not rated" aside) as one: "rated 4 and up". None
/// otherwise.
pub fn rating_range(keys: &[&str]) -> Option<String> {
    let mut stars: Vec<u8> = keys.iter().filter_map(|k| k.parse().ok()).collect();
    stars.sort_unstable();
    stars.dedup();
    let low = *stars.first()?;
    (stars.len() >= 2 && stars.last() == Some(&5) && stars.len() == (6 - low) as usize).then(|| format!("rated {low} and up"))
}

/// Length's steps in order, with the hours each starts at.
const LENGTH_STEPS: [(&str, u32); 4] = [("length-short", 0), ("length-medium", 5), ("length-long", 15), ("length-epic", 40)];

/// What the Lens row shows while nothing is picked: the games it looks through and the filters it offers,
/// "332 games · 9 filters" (instead of a blank where a count goes once something is picked).
pub fn overview(games: u32, filters: usize) -> String {
    let games = if games == 1 { "1 game".to_string() } else { format!("{games} games") };
    match filters {
        0 => games,
        1 => format!("{games} · 1 filter"),
        n => format!("{games} · {n} filters"),
    }
}

/// Two or more neighbouring Length steps (endless aside) as one range: "under 15 hours", "5–40 hours", "15+ hours".
/// None for a single step or steps with a gap.
pub fn length_range(keys: &[&str]) -> Option<String> {
    let mut at: Vec<usize> = keys.iter().filter_map(|k| LENGTH_STEPS.iter().position(|(id, _)| id == k)).collect();
    at.sort_unstable();
    at.dedup();
    let (&first, &last) = (at.first()?, at.last()?);
    if at.len() < 2 || last - first + 1 != at.len() {
        return None;
    }
    let from = LENGTH_STEPS[first].1;
    let to = LENGTH_STEPS.get(last + 1).map(|s| s.1);
    Some(match (from, to) {
        (0, Some(to)) => format!("under {to} hours"),
        (0, None) => "any length".to_string(),
        (from, None) => format!("{from}+ hours"),
        (from, Some(to)) => format!("{from}–{to} hours"),
    })
}
