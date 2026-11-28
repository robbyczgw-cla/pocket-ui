//! What ScreenScraper says of a game besides its pictures (docs/SCREENSCRAPER.md "Facts and text"): the description in
//! many languages, the genre, the players, developer and publisher, the release dates per region and the rating.
//!
//! * **No request of its own.** The `jeuInfos` answer the picture run asks for anyway carries all of it
//!   (`screenscraper::run_until` keeps it here, [`save`]): the facts cost the quota nothing.
//! * **Kept per game:** `<data>/screenscraper/facts/<system>/<rom stem>.json`, only the languages pocket-ui speaks.
//! * **Only what is missing.** [`fill_card`] adds to the game card and the box's back what neither pocket-ui's own data
//!   nor the player's EmulationStation gamelist has; it never replaces a fact. The description in the interface's
//!   language (else English) comes before Wikipedia's English one and RomM's; the player's own gamelist text stays.
//! * **Read on a thread** when a card opens (`App::screenscraper_card_facts`), never in a frame. What was kept shows
//!   with ScreenScraper On or Off, as the downloaded spines do (Off only sends nothing); a game without a file has
//!   exactly today's card.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::game_card_model::{CardData, Summary};

/// The longest description kept (characters): a box's back, not an article.
const MAX_TEXT: usize = 4000;

/// The languages a description, a genre is kept in: the interface's (`i18n::Lang`), as ScreenScraper names them.
const LANGS: [&str; 10] = ["en", "de", "es", "pt", "fr", "it", "ko", "zh", "vi", "ja"];

/// What `jeuInfos` says of the game, as the card shows it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Facts {
    /// (language, text): the description.
    #[serde(default)]
    pub synopsis: Vec<(String, String)>,
    /// (language, name): the main genre.
    #[serde(default)]
    pub genre: Vec<(String, String)>,
    /// "1", "1-4".
    #[serde(default)]
    pub players: String,
    #[serde(default)]
    pub developer: String,
    #[serde(default)]
    pub publisher: String,
    /// (region, year): when it came out where.
    #[serde(default)]
    pub released: Vec<(String, u16)>,
    /// ScreenScraper's rating, out of 20.
    #[serde(default)]
    pub rating: Option<u8>,
}

impl Facts {
    pub fn is_empty(&self) -> bool {
        self == &Facts::default()
    }

    /// The description in `lang`, else in English.
    pub fn text_in(&self, lang: &str) -> Option<&str> {
        let pick = |l: &str| self.synopsis.iter().find(|(k, _)| k == l).map(|(_, t)| t.as_str());
        pick(lang).or_else(|| pick("en"))
    }

    /// The genre in `lang`, else in English, else the first one there is.
    pub fn genre_in(&self, lang: &str) -> Option<&str> {
        let pick = |l: &str| self.genre.iter().find(|(k, _)| k == l).map(|(_, t)| t.as_str());
        pick(lang).or_else(|| pick("en")).or_else(|| self.genre.first().map(|(_, t)| t.as_str()))
    }

    /// The year it came out where the ROM is from (`region_order` of its file name), else the first year it came out.
    pub fn year_for(&self, stem: &str) -> Option<u16> {
        let order = crate::screenscraper::region_order(&[], stem);
        order
            .iter()
            .find_map(|r| self.released.iter().find(|(k, _)| k == r).map(|(_, y)| *y))
            .or_else(|| self.released.iter().map(|(_, y)| *y).min())
    }

    /// "JP 1990 · US 1991 · EU 1992": when the regions' years differ, else None (the Year says it all).
    pub fn released_line(&self) -> Option<String> {
        let mut by_year: Vec<(u16, String)> = Vec::new();
        for (region, year) in &self.released {
            // `ss` is ScreenScraper's own name for a game, no region.
            if region == "ss" || by_year.iter().any(|(_, r)| r.eq_ignore_ascii_case(region)) {
                continue;
            }
            by_year.push((*year, region.to_ascii_uppercase()));
        }
        by_year.sort();
        let first = by_year.first()?.0;
        if by_year.iter().all(|(y, _)| *y == first) {
            return None;
        }
        // One line on the back: the first year of each, at most four regions.
        let mut seen: Vec<u16> = Vec::new();
        let mut parts = Vec::new();
        for (y, r) in &by_year {
            if parts.len() == 4 {
                break;
            }
            if seen.contains(y) {
                // Same year as one before it: its region joins that one ("US/EU 1991").
                if let Some(p) = parts.iter_mut().find(|p: &&mut String| p.ends_with(&format!(" {y}"))) {
                    let at = p.len() - 5;
                    p.insert_str(at, &format!("/{r}"));
                }
                continue;
            }
            seen.push(*y);
            parts.push(format!("{r} {y}"));
        }
        Some(parts.join(" · "))
    }
}

fn text(v: &Value, key: &str) -> String {
    match v.get(key) {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        _ => String::new(),
    }
}

/// A text as the card shows it: entities decoded, line breaks and runs of spaces one space, at most [`MAX_TEXT`].
fn clean(s: &str) -> String {
    let s = crate::html_form::unescape(s);
    let mut out = String::with_capacity(s.len());
    for word in s.split_whitespace() {
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(word);
    }
    if out.chars().count() > MAX_TEXT {
        // Cut after the last sentence that fits.
        let cut: String = out.chars().take(MAX_TEXT).collect();
        let end = cut.rfind(['.', '!', '?']).map_or(cut.len(), |i| i + 1);
        out = cut[..end].to_string();
    }
    out
}

/// The facts of a `jeuInfos` answer (`response.jeu`); empty for one without any.
pub fn parse(body: &[u8]) -> Facts {
    let Ok(v) = serde_json::from_slice::<Value>(body) else { return Facts::default() };
    let Some(jeu) = v.get("response").and_then(|r| r.get("jeu")) else { return Facts::default() };
    let named = |key: &str| jeu.get(key).map(|x| clean(&text(x, "text"))).unwrap_or_default();
    let synopsis = jeu
        .get("synopsis")
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .map(|s| (text(s, "langue").to_ascii_lowercase(), clean(&text(s, "text"))))
                .filter(|(l, t)| LANGS.contains(&l.as_str()) && !t.is_empty())
                .collect()
        })
        .unwrap_or_default();
    // The main genre (`principale`), else the first.
    let genres = jeu.get("genres").and_then(Value::as_array);
    let main = genres.and_then(|g| g.iter().find(|x| text(x, "principale") == "1").or_else(|| g.first()));
    let genre = main
        .and_then(|g| g.get("noms").and_then(Value::as_array))
        .map(|names| {
            names
                .iter()
                .map(|n| (text(n, "langue").to_ascii_lowercase(), clean(&text(n, "text"))))
                .filter(|(l, t)| LANGS.contains(&l.as_str()) && !t.is_empty())
                .collect()
        })
        .unwrap_or_default();
    let released = jeu
        .get("dates")
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(|d| {
                    let year: u16 = text(d, "text").get(..4)?.parse().ok()?;
                    (1970..=2100).contains(&year).then(|| (text(d, "region").to_ascii_lowercase(), year))
                })
                .collect()
        })
        .unwrap_or_default();
    let rating = jeu.get("note").map(|n| text(n, "text")).and_then(|t| t.trim().parse::<u8>().ok()).filter(|n| (1..=20).contains(n));
    // "1-4", "1 - 4" as one form; anything else (a word) is left out.
    let players: String = named("joueurs").chars().filter(|c| !c.is_whitespace()).collect();
    let players = if players.chars().all(|c| c.is_ascii_digit() || c == '-') && players.starts_with(|c: char| c.is_ascii_digit()) { players } else { String::new() };
    Facts { synopsis, genre, players, developer: named("developpeur"), publisher: named("editeur"), released, rating }
}

fn file(data_dir: &Path, system: &str, rom: &Path) -> PathBuf {
    let stem = rom.file_stem().and_then(|s| s.to_str()).unwrap_or_default();
    data_dir.join(crate::screenscraper::DIR).join("facts").join(system).join(format!("{stem}.json"))
}

/// Keeps a game's facts (an empty answer keeps nothing). Failing to write costs only the facts.
pub fn save(data_dir: &Path, system: &str, rom: &Path, facts: &Facts) {
    if facts.is_empty() {
        return;
    }
    let path = file(data_dir, system, rom);
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(json) = serde_json::to_vec(facts) {
        let _ = crate::fsutil::write_atomic(&path, &json);
    }
}

/// A game's facts on the card; None without (never asked, not known, a broken file). Threads only.
pub fn load(data_dir: &Path, system: &str, rom: &Path) -> Option<Facts> {
    let text = crate::background_io::read_to_string(file(data_dir, system, rom)).ok()?;
    serde_json::from_str::<Facts>(&text).ok().filter(|f| !f.is_empty())
}

/// Adds to the card what it lacks (module docs); `lang` the interface's language ("de"), `stem` the ROM's file name
/// for the region of its year. True when the card changed.
/// `ours`: the facts from pocket-ui's own offline game list (English, `OpenCard::source_facts`): its Genre gives way to
/// ScreenScraper's in the player's language (Wikidata's "Role-playing, turn-based" on a German card, lead 26.11.2026);
/// a genre from the player's gamelist stays.
pub fn fill_card(data: &mut CardData, f: &Facts, lang: &str, stem: &str, ours: &[(&'static str, String)]) -> bool {
    let before = data.clone();
    let mut fact = false;
    if data.year.is_none() {
        if let Some(y) = f.year_for(stem) {
            data.year = Some(y);
            fact = true;
        }
    }
    if let Some(line) = f.released_line() {
        if !data.facts.iter().any(|(l, _)| *l == "Released") {
            data.facts.push(("Released", line));
            fact = true;
        }
    }
    if data.players.is_none() && !f.players.is_empty() {
        data.players = Some(crate::gamelist::players_text(&f.players));
        fact = true;
    }
    // The genre in the player's language, where ScreenScraper has it in just that one, over our English one.
    let in_lang = f.genre.iter().find(|(l, _)| l == lang).map(|(_, g)| g.clone()).filter(|_| lang != "en");
    if let Some(genre) = in_lang {
        if let Some(slot) = data.facts.iter_mut().find(|(l, v)| *l == "Genre" && ours.iter().any(|(ol, ov)| *ol == "Genre" && ov == v)) {
            if slot.1 != genre {
                slot.1 = genre;
                fact = true;
            }
        }
    }
    for (label, value) in [("Developer", f.developer.as_str()), ("Publisher", f.publisher.as_str()), ("Genre", f.genre_in(lang).unwrap_or_default())] {
        if !value.is_empty() && !data.facts.iter().any(|(l, _)| *l == label) {
            data.facts.push((label, value.to_string()));
            fact = true;
        }
    }
    // Its players' rating, named apart from the player's own "Rating" (Neon 26.11.2026); not where reviews are known.
    if data.scores.is_none() && !data.facts.iter().any(|(l, _)| *l == "Community rating") {
        if let Some(n) = f.rating {
            data.facts.push(("Community rating", format!("{} of 20 on ScreenScraper", n)));
            fact = true;
        }
    }
    data.from_screenscraper |= fact;
    // The description: the player's own gamelist text stays; pocket-ui's (Wikipedia, English only; RomM's) gives way
    // to the one in the player's language, else to ScreenScraper's English one.
    if let Some(t) = f.text_in(lang) {
        if matches!(data.summary, Summary::None | Summary::Loading | Summary::Unavailable(_) | Summary::Wikipedia { .. } | Summary::Romm(_)) {
            data.summary = Summary::ScreenScraper(t.to_string());
        }
    }
    *data != before
}

#[cfg(test)]
#[path = "screenscraper_facts_tests.rs"]
mod tests;
