//! What the series card says of the biggest game series (docs/SERIES-SHELF.md "The series card"): who made it, since
//! when, how many main games it has, its most important ones and a short history. Written by us from the sources in
//! docs/SERIES-SOURCES.md, compiled in, nothing read or downloaded. A series of the player's that is not here gets a
//! card of its facts from the library alone (`series_shelf`), never an empty one.
//!
//! The data is `series_facts_data.rs`, one [`Facts`] per series, alphabetical. Its `story` is painted through the
//! interface's tables like every other line (assets/i18n/*.tsv); the names and titles stay as they were released.

use std::collections::HashMap;
use std::sync::OnceLock;

use crate::connections::{key, Kind};

/// One series as the card shows it.
#[derive(Debug, PartialEq, Eq)]
pub struct Facts {
    /// "The Legend of Zelda".
    pub name: &'static str,
    /// Other names the game data or titles give it ("Zelda"; "Super Mario Land" for Super Mario): their games join this
    /// series' shelf. Matched as [`key`] matches series names.
    pub aliases: &'static [&'static str],
    /// The first game's maker ("Nintendo").
    pub by: &'static str,
    /// The first game's year.
    pub since: u16,
    /// Main series games as the source counts them; 0: no clear count, the card leaves the number out.
    pub games: u16,
    /// Its most important games, as released.
    pub key: &'static [&'static str],
    /// Two to four sentences of history, English (translated through the tables).
    pub story: &'static str,
}

/// Every series with a card of its own.
pub const ALL: &[Facts] = &include!("series_facts_data.rs");

/// The series whose name or alias has this [`key`] (`Kind::Series`): its place in [`ALL`].
pub fn find(series_key: &str) -> Option<usize> {
    static KEYS: OnceLock<HashMap<String, usize>> = OnceLock::new();
    KEYS.get_or_init(|| {
        let mut m = HashMap::new();
        for (i, f) in ALL.iter().enumerate() {
            for name in std::iter::once(&f.name).chain(f.aliases) {
                m.entry(key(Kind::Series, name)).or_insert(i);
            }
        }
        m
    })
    .get(series_key)
    .copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_series_has_its_facts_and_a_story_a_player_can_read() {
        assert!(ALL.len() >= 40, "the biggest series: {}", ALL.len());
        for f in ALL {
            assert!(!f.name.is_empty() && !f.by.is_empty(), "{f:?}");
            assert!((1970..=2026).contains(&f.since), "{}: {}", f.name, f.since);
            assert!(!f.key.is_empty() && f.key.len() <= 4, "{}: 1 to 4 key games", f.name);
            let sentences = f.story.matches(". ").count() + 1;
            assert!((2..=5).contains(&sentences), "{}: 2 to 4 sentences, {sentences}", f.name);
            assert!(f.story.ends_with('.'), "{}", f.name);
            // Public texts: never a dash as punctuation (rules: no — or –), no tab or line break for the tables.
            for t in std::iter::once(f.story).chain(f.key.iter().copied()).chain([f.name]) {
                assert!(!t.contains(['—', '–', '\t', '\n']), "{}: {t}", f.name);
            }
        }
    }

    #[test]
    fn names_and_aliases_find_their_series_and_no_alias_is_two_series() {
        let mut seen: HashMap<String, &str> = HashMap::new();
        for (i, f) in ALL.iter().enumerate() {
            for name in std::iter::once(&f.name).chain(f.aliases) {
                let k = key(Kind::Series, name);
                assert!(!k.is_empty(), "{name}");
                if let Some(other) = seen.insert(k.clone(), f.name) {
                    assert_eq!(other, f.name, "{name} is in two series");
                }
                assert_eq!(find(&k), Some(i), "{name}");
            }
        }
        assert_eq!(find(&key(Kind::Series, "The Legend of Zelda series")).map(|i| ALL[i].name), Some("The Legend of Zelda"));
        assert_eq!(find("no such series"), None);
    }

    /// The stories are skipped by scripts/i18n-extract.py with the names around them (`series_facts_data.rs`): German, which
    /// the gate requires of every painted line, is held here.
    #[test]
    fn every_story_has_its_german_line() {
        let _l = crate::i18n::for_this_thread(crate::i18n::Lang::De);
        let missing: Vec<&str> = ALL.iter().filter(|f| crate::i18n::tr(f.story) == f.story).map(|f| f.name).collect();
        assert!(missing.is_empty(), "stories without a German line in assets/i18n/de.tsv: {missing:?}");
    }

    /// The release check: every story in every language the Pocket offers.
    #[test]
    #[ignore = "release check: every language complete (scripts/release-check.sh)"]
    fn every_story_is_in_every_language() {
        for lang in crate::i18n::Lang::ALL.into_iter().filter(|l| *l != crate::i18n::Lang::En) {
            let _l = crate::i18n::for_this_thread(lang);
            let missing: Vec<&str> = ALL.iter().filter(|f| crate::i18n::tr(f.story) == f.story).map(|f| f.name).collect();
            assert!(missing.is_empty(), "{lang:?}: {missing:?}");
        }
    }
}
