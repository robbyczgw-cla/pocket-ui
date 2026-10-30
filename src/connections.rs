//! Connections (docs/CURATOR.md "Connections"): the developer, publisher, series and genre of a game lead to the
//! player's own games with the same one. "More from Criterion Games", "Burnout series, in release order", "More racing
//! games". Pure logic; the app builds the [`Index`] on Curator's catalog thread when the library is read
//! (`app_connections.rs`), so opening a card only looks a few keys up.
//!
//! - Names are matched loosely ([`key`]): case, punctuation and a company's trailing "Games", "Studios", "Inc." don't
//!   count, so "Criterion", "Criterion Games" and "Criterion Studios" are one developer; a genre's "video game" and a
//!   few spellings ("Platformer", "Race") are one genre.
//! - A series comes from the data (Wikidata's series) or, where that has none, from the titles: "Burnout 3: Takedown"
//!   and "Burnout 2: Point of Impact" are the Burnout series because both are a number after "Burnout", and "Burnout
//!   Revenge" joins them because the player also has a game called just "Burnout". Only when two different titles
//!   meet; a title alone is never a series.
//! - A value leads somewhere only when another game of the player's has it: a card never opens an empty list.
//!
//! Also "Rediscover" ([`rediscover`]): the games played a year ago today, then those not played for a year.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// What a fact is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    Developer,
    Publisher,
    Series,
    Genre,
}

impl Kind {
    pub const ALL: [Kind; 4] = [Kind::Developer, Kind::Publisher, Kind::Series, Kind::Genre];

    /// The fact's name on the card (`CardData::facts`).
    pub fn label(self) -> &'static str {
        match self {
            Kind::Developer => "Developer",
            Kind::Publisher => "Publisher",
            Kind::Series => "Series",
            Kind::Genre => "Genre",
        }
    }

    pub fn of_label(label: &str) -> Option<Kind> {
        Kind::ALL.into_iter().find(|k| k.label() == label)
    }

    fn slot(self) -> usize {
        self as usize
    }
}

/// A game of the library as the index's thread gets it.
#[derive(Clone, Debug, Default)]
pub struct Entry {
    /// (system, game) in the app's library.
    pub at: (usize, usize),
    pub path: PathBuf,
    /// The shelf's name: "Burnout 3 - Takedown".
    pub name: String,
    pub year: Option<u16>,
    /// The values as the data writes them, by [`Kind`]: several, or one with commas ("Racing, Driving").
    pub values: [Vec<String>; 4],
}

/// One game in the index.
#[derive(Clone, Debug)]
pub struct Game {
    pub at: (usize, usize),
    pub path: PathBuf,
    pub name: String,
    pub year: Option<u16>,
}

/// Where a fact leads: its value as the card shows it and the player's games with it, in the list's order.
#[derive(Clone, Debug, PartialEq)]
pub struct Link {
    pub kind: Kind,
    /// "Criterion Games", "Burnout", "Racing".
    pub value: String,
    /// Indexes into [`Index::games`].
    pub games: Vec<usize>,
}

impl Link {
    /// The list's title: "More from Criterion Games", "Burnout series, in release order", "More racing games".
    pub fn title(&self) -> String {
        match self.kind {
            Kind::Developer => format!("More from {}", self.value),
            Kind::Publisher => format!("More published by {}", self.value),
            Kind::Series => format!("{} series, in release order", self.value),
            Kind::Genre => format!("More {} games", self.value.to_lowercase()),
        }
    }
}

/// The games of the library by developer, publisher, series and genre.
#[derive(Debug, Default)]
pub struct Index {
    pub games: Vec<Game>,
    by_path: HashMap<PathBuf, usize>,
    /// (kind, key) -> games, each once, in library order.
    by_key: HashMap<(Kind, String), Vec<usize>>,
    /// Per game: the series its title gives, where the data has none ("Burnout").
    derived: HashMap<usize, String>,
    /// Per game: its title as [`key`] gives it, to tell one title on two systems from two titles.
    title_keys: Vec<String>,
}

/// Words a company's name may end in without being another company.
const COMPANY_WORDS: &[&str] = &[
    "games", "game", "studio", "studios", "entertainment", "inc", "incorporated", "ltd", "limited", "co", "corp", "corporation", "company", "software",
    "interactive", "productions", "production", "llc", "gmbh", "sa", "srl", "kk", "plc", "digital", "publishing", "development", "dev",
];

/// Genre spellings that are one genre (after [`key`]'s cleaning).
const GENRE_SAME: &[(&str, &str)] = &[
    ("platformer", "platform"),
    ("platforming", "platform"),
    ("race", "racing"),
    ("driving", "racing"),
    ("racing driving", "racing"),
    ("rpg", "role playing"),
    ("roleplaying", "role playing"),
    ("role playing rpg", "role playing"),
    ("shmup", "shoot em up"),
    ("shootem up", "shoot em up"),
    ("beat em up", "beat em up"),
    ("fighter", "fighting"),
    ("puzzle game", "puzzle"),
    ("sport", "sports"),
];

/// Lower case, letters and digits, one space between words; "&" is "and", an apostrophe joins ("Shoot 'em up").
fn clean(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut space = false;
    for ch in s.replace('&', " and ").chars() {
        if ch == '\'' || ch == '’' || ch == '.' {
            continue;
        }
        if ch.is_alphanumeric() {
            if space && !out.is_empty() {
                out.push(' ');
            }
            space = false;
            out.extend(ch.to_lowercase());
        } else {
            space = true;
        }
    }
    out
}

/// The key a value is matched by: "Criterion Games" and "Criterion Studios" are "criterion", "racing video game" is
/// "racing". Empty for a value that is only such words ("Ltd.").
pub fn key(kind: Kind, value: &str) -> String {
    let c = clean(value);
    let mut words: Vec<&str> = c.split(' ').filter(|w| !w.is_empty()).collect();
    if words.first() == Some(&"the") && words.len() > 1 {
        words.remove(0);
    }
    match kind {
        Kind::Developer | Kind::Publisher => {
            let all = words.len();
            while words.len() > 1 && words.last().is_some_and(|w| COMPANY_WORDS.contains(w)) {
                words.pop();
            }
            // "Ltd." alone, the tail of "Square Co., Ltd.": nothing to match by.
            if all == 1 && COMPANY_WORDS.contains(&words[0]) {
                return String::new();
            }
        }
        Kind::Series => {
            if words.len() > 1 && words.last() == Some(&"series") {
                words.pop();
            }
        }
        Kind::Genre => {
            if words.ends_with(&["video", "game"]) && words.len() > 2 {
                words.truncate(words.len() - 2);
            } else if words.len() > 1 && words.last() == Some(&"game") {
                words.pop();
            }
        }
    }
    let k = words.join(" ");
    if kind == Kind::Genre {
        if let Some((_, same)) = GENRE_SAME.iter().find(|(a, _)| *a == k) {
            return same.to_string();
        }
    }
    k
}

/// The values in one written value: "Racing, Driving" and "Nintendo / Intelligent Systems" are two.
pub fn parts(value: &str) -> impl Iterator<Item = &str> {
    value.split([',', '/', ';', '|']).map(str::trim).filter(|p| !p.is_empty())
}

/// Roman numerals that number a sequel ("Final Fantasy VII", "Mega Man X"); "I" is a word too often.
fn roman(w: &str) -> bool {
    matches!(w, "II" | "III" | "IV" | "V" | "VI" | "VII" | "VIII" | "IX" | "X" | "XI" | "XII" | "XIII" | "XIV" | "XV")
}

/// What a title says about its series: (the name before a number or a subtitle, whether there was one). "Burnout 3 -
/// Takedown" is ("Burnout", true), "The Legend of Zelda: Ocarina of Time" ("The Legend of Zelda", true), "Burnout
/// Revenge" ("Burnout Revenge", false).
pub fn title_stem(name: &str) -> (String, bool) {
    let words: Vec<&str> = name.split_whitespace().collect();
    for (i, w) in words.iter().enumerate() {
        if i >= 1 && (*w == "-" || *w == "–" || *w == ":") {
            return (words[..i].join(" "), true);
        }
        let bare = w.trim_end_matches([':', ',']);
        if i >= 1 && (bare.chars().all(|c| c.is_ascii_digit()) && !bare.is_empty() || roman(bare)) {
            return (words[..i].join(" "), true);
        }
        if w.ends_with(':') && i + 1 < words.len() {
            return (words[..=i].join(" ").trim_end_matches(':').to_string(), true);
        }
    }
    (name.trim().to_string(), false)
}

/// Stems too plain to be a series by themselves.
const STEM_TOO_PLAIN: &[&str] = &["super", "star", "space", "world", "final", "pro", "new", "game", "the", "my", "mega", "street", "dragon", "dark"];

/// The series the titles give, per game: two different titles with the same stem before a number or subtitle
/// ("Burnout 2", "Burnout 3"), or a title that is a stem other games have plus a word ("Burnout" and "Burnout Revenge").
/// `title_keys`: each game's title as [`key`] gives it. One pass over the titles and their word boundaries, so 5,000
/// games cost a few milliseconds (`speed_of_the_index_for_5000_games`).
fn derive_series(games: &[Game], title_keys: &[String]) -> HashMap<usize, String> {
    // stem key -> (the stem as first written, the games in it)
    let mut stems: HashMap<String, (String, Vec<usize>)> = HashMap::new();
    // Titles that are a stem themselves ("Burnout"): they take the titles that start with them and a word.
    let mut bases: HashSet<String> = HashSet::new();
    for (i, g) in games.iter().enumerate() {
        let (stem, cut) = title_stem(&g.name);
        let k = key(Kind::Series, &stem);
        if k.len() < 3 || STEM_TOO_PLAIN.contains(&k.as_str()) {
            continue;
        }
        if !cut {
            bases.insert(k.clone());
        }
        stems.entry(k).or_insert_with(|| (stem.trim().to_string(), Vec::new())).1.push(i);
    }
    for (i, tk) in title_keys.iter().enumerate() {
        for (at, _) in tk.match_indices(' ') {
            if bases.contains(&tk[..at]) {
                if let Some((_, members)) = stems.get_mut(&tk[..at]) {
                    members.push(i);
                }
            }
        }
    }
    // A title in two stems' series ("Burnout" in its own and a longer one): the longer, more particular one wins.
    let mut out: HashMap<usize, (usize, String)> = HashMap::new();
    for (k, (stem, mut members)) in stems {
        members.sort_unstable();
        members.dedup();
        // Two titles at least: the same game on two systems is one.
        let titles: HashSet<&str> = members.iter().map(|&i| title_keys[i].as_str()).collect();
        if titles.len() < 2 {
            continue;
        }
        for i in members {
            if out.get(&i).is_none_or(|(len, _)| *len < k.len()) {
                out.insert(i, (k.len(), stem.clone()));
            }
        }
    }
    out.into_iter().map(|(i, (_, s))| (i, s)).collect()
}

impl Index {
    pub fn build(entries: Vec<Entry>) -> Index {
        let games: Vec<Game> = entries.iter().map(|e| Game { at: e.at, path: e.path.clone(), name: e.name.clone(), year: e.year }).collect();
        let title_keys: Vec<String> = games.iter().map(|g| key(Kind::Series, &g.name)).collect();
        let derived: HashMap<usize, String> = derive_series(&games, &title_keys).into_iter().filter(|(i, _)| entries[*i].values[Kind::Series.slot()].is_empty()).collect();
        let mut by_key: HashMap<(Kind, String), Vec<usize>> = HashMap::new();
        let mut add = |kind: Kind, value: &str, i: usize| {
            for p in parts(value) {
                let k = key(kind, p);
                if k.is_empty() {
                    continue;
                }
                let list = by_key.entry((kind, k)).or_default();
                if list.last() != Some(&i) {
                    list.push(i);
                }
            }
        };
        for (i, e) in entries.iter().enumerate() {
            for kind in Kind::ALL {
                for v in &e.values[kind.slot()] {
                    add(kind, v, i);
                }
            }
            if let Some(s) = derived.get(&i) {
                add(Kind::Series, s, i);
            }
        }
        let by_path = games.iter().enumerate().map(|(i, g)| (g.path.clone(), i)).collect();
        Index { games, by_path, by_key, derived, title_keys }
    }

    pub fn len(&self) -> usize {
        self.games.len()
    }

    pub fn is_empty(&self) -> bool {
        self.games.is_empty()
    }

    /// The series a game's title gives when the data has none ("Burnout"), for its card.
    pub fn derived_series(&self, path: &Path) -> Option<&str> {
        self.by_path.get(path).and_then(|i| self.derived.get(i)).map(String::as_str)
    }

    /// Where the fact `kind` with the value `shown` (as the card shows it: "Criterion Games, Acclaim") leads from the game
    /// at `path`: the first of its values another of the player's games has. None: nowhere, the fact is plain text.
    /// A series lists every game in it, this one too, in release order; the rest list the others, the developer's and
    /// publisher's by release, a genre's by name.
    pub fn link(&self, kind: Kind, shown: &str, path: &Path) -> Option<Link> {
        let this = self.by_path.get(path).copied();
        for p in parts(shown) {
            let Some(found) = self.by_key.get(&(kind, key(kind, p))) else { continue };
            let mut games: Vec<usize> = found.iter().copied().filter(|&i| kind == Kind::Series || Some(i) != this).collect();
            // Another title: the same game on two systems isn't a series of two.
            let others = match this {
                Some(t) => games.iter().filter(|&&i| i != t && (kind != Kind::Series || self.title_keys[i] != self.title_keys[t])).count(),
                None => games.len(),
            };
            if others == 0 {
                continue;
            }
            let name = |i: usize| self.games[i].name.to_lowercase();
            match kind {
                Kind::Genre => games.sort_by_cached_key(|&i| (name(i), i)),
                _ => games.sort_by_cached_key(|&i| (self.games[i].year.is_none(), self.games[i].year, name(i), i)),
            }
            return Some(Link { kind, value: p.to_string(), games });
        }
        None
    }
}

/// Why a game is in Rediscover.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Why {
    /// It was played on this day a year ago.
    YearAgoToday,
    /// Its last session is more than a year ago.
    NotForAYear,
}

/// A year, for "not played for a year".
pub const YEAR_SECS: u64 = 365 * 86_400;

/// Rediscover: from `played` (path, last played, seconds played in all) and `days` (path, the unix start and UTC offset
/// of each session the play history kept), the games played a year ago today (the same date, local time; 28 February
/// for the 29th), then the games not played for a year, the ones played longest first. Each game once; a game played
/// since a year ago today is only in the first group if that day was one of its days.
pub fn rediscover(played: &[(PathBuf, u64, u64)], days: &[(PathBuf, u64, i32)], now: u64, tz: i32) -> Vec<(PathBuf, Why)> {
    use crate::play_history::{civil, day_of, local_day};
    let (y, m, d) = civil(local_day(now, tz));
    let leap = |y: i32| (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let target = day_of(y - 1, m, if m == 2 && d == 29 && !leap(y - 1) { 28 } else { d });
    let mut seen: HashSet<&Path> = HashSet::new();
    let mut out = Vec::new();
    let mut today: Vec<(&Path, u64)> = Vec::new();
    for (p, at, tz) in days {
        if local_day(*at, *tz) == target && seen.insert(p) {
            today.push((p, played.iter().find(|x| x.0 == *p).map_or(0, |x| x.2)));
        }
    }
    for (p, last, secs) in played {
        // Future timestamps cannot be last year's date; skip before signed calendar arithmetic.
        if *last <= now && local_day(*last, tz) == target && seen.insert(p) {
            today.push((p, *secs));
        }
    }
    today.sort_by_key(|(p, secs)| (std::cmp::Reverse(*secs), p.to_path_buf()));
    out.extend(today.into_iter().map(|(p, _)| (p.to_path_buf(), Why::YearAgoToday)));
    let mut old: Vec<&(PathBuf, u64, u64)> = played.iter().filter(|(p, last, _)| *last > 0 && now.saturating_sub(*last) >= YEAR_SECS && !seen.contains(p.as_path())).collect();
    old.sort_by_key(|(p, _, secs)| (std::cmp::Reverse(*secs), p.clone()));
    out.extend(old.into_iter().map(|(p, _, _)| (p.clone(), Why::NotForAYear)));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(i: usize, name: &str, year: Option<u16>, dev: &[&str], series: &[&str], genre: &[&str]) -> Entry {
        Entry {
            at: (0, i),
            path: PathBuf::from(format!("/roms/{i} {name}.iso")),
            name: name.into(),
            year,
            values: [dev.iter().map(|s| s.to_string()).collect(), Vec::new(), series.iter().map(|s| s.to_string()).collect(), genre.iter().map(|s| s.to_string()).collect()],
        }
    }

    fn names(ix: &Index, l: &Link) -> Vec<String> {
        l.games.iter().map(|&i| ix.games[i].name.clone()).collect()
    }

    #[test]
    fn spelling_variants_of_a_company_or_genre_are_one() {
        assert_eq!(key(Kind::Developer, "Criterion Games"), "criterion");
        assert_eq!(key(Kind::Developer, "Criterion Studios"), "criterion");
        assert_eq!(key(Kind::Developer, "Criterion"), "criterion");
        assert_eq!(key(Kind::Developer, "Square Co., Ltd."), "square");
        assert_eq!(parts("Square Co., Ltd.").map(|p| key(Kind::Developer, p)).collect::<Vec<_>>(), ["square", ""]);
        assert_eq!(key(Kind::Developer, "Games"), "", "a company word alone matches nothing");
        assert_eq!(key(Kind::Developer, "The Bitmap Brothers"), "bitmap brothers");
        assert_eq!(key(Kind::Developer, "Intelligent Systems"), "intelligent systems");
        assert_eq!(key(Kind::Genre, "racing video game"), "racing");
        assert_eq!(key(Kind::Genre, "Race"), "racing");
        assert_eq!(key(Kind::Genre, "Platformer"), "platform");
        assert_eq!(key(Kind::Genre, "platform game"), "platform");
        assert_eq!(key(Kind::Genre, "Shoot 'em up"), key(Kind::Genre, "shoot em up"));
        assert_eq!(key(Kind::Series, "The Legend of Zelda"), "legend of zelda");
        assert_eq!(key(Kind::Series, "Burnout series"), "burnout");
    }

    #[test]
    fn titles_give_their_stem_before_a_number_or_subtitle() {
        assert_eq!(title_stem("Burnout 3 - Takedown"), ("Burnout".into(), true));
        assert_eq!(title_stem("Burnout 2: Point of Impact"), ("Burnout".into(), true));
        assert_eq!(title_stem("Final Fantasy VII"), ("Final Fantasy".into(), true));
        assert_eq!(title_stem("The Legend of Zelda: Ocarina of Time"), ("The Legend of Zelda".into(), true));
        assert_eq!(title_stem("Mario Kart - Double Dash!!"), ("Mario Kart".into(), true));
        assert_eq!(title_stem("Burnout Revenge"), ("Burnout Revenge".into(), false));
        assert_eq!(title_stem("1942"), ("1942".into(), false), "a number first is the title");
        assert_eq!(title_stem("Tetris"), ("Tetris".into(), false));
    }

    #[test]
    fn a_series_from_titles_needs_two_different_titles_and_takes_the_base_titles_followers() {
        let ix = Index::build(vec![
            entry(0, "Burnout", Some(2001), &[], &[], &[]),
            entry(1, "Burnout 3 - Takedown", Some(2004), &[], &[], &[]),
            entry(2, "Burnout Revenge", Some(2005), &[], &[], &[]),
            entry(3, "Burnout 2 - Point of Impact", Some(2002), &[], &[], &[]),
            entry(4, "Tetris", None, &[], &[], &[]),
            entry(5, "Gran Turismo 4", None, &[], &[], &[]),
            entry(6, "Gran Turismo 4", None, &[], &[], &[]),
            entry(7, "Super Mario World", None, &[], &[], &[]),
            entry(8, "Super Metroid", None, &[], &[], &[]),
        ]);
        let l = ix.link(Kind::Series, "Burnout", Path::new("/roms/2 Burnout Revenge.iso")).unwrap();
        assert_eq!(names(&ix, &l), ["Burnout", "Burnout 2 - Point of Impact", "Burnout 3 - Takedown", "Burnout Revenge"], "release order, this game too");
        assert_eq!(l.title(), "Burnout series, in release order");
        assert_eq!(ix.derived_series(Path::new("/roms/1 Burnout 3 - Takedown.iso")), Some("Burnout"));
        assert_eq!(ix.derived_series(Path::new("/roms/5 Gran Turismo 4.iso")), None, "one title on two systems is no series");
        assert_eq!(ix.derived_series(Path::new("/roms/4 Tetris.iso")), None);
        assert_eq!(ix.derived_series(Path::new("/roms/7 Super Mario World.iso")), None, "a shared first word is no series");
    }

    #[test]
    fn without_the_base_title_only_numbered_titles_make_a_series() {
        let ix = Index::build(vec![
            entry(0, "Burnout 3 - Takedown", None, &[], &[], &[]),
            entry(1, "Burnout Revenge", None, &[], &[], &[]),
            entry(2, "Burnout Dominator", None, &[], &[], &[]),
        ]);
        assert_eq!(ix.derived_series(Path::new("/roms/0 Burnout 3 - Takedown.iso")), None, "one numbered title alone");
        assert!(ix.link(Kind::Series, "Burnout", Path::new("/roms/1 Burnout Revenge.iso")).is_none());
    }

    #[test]
    fn data_series_and_title_series_meet_and_data_wins_for_its_game() {
        let ix = Index::build(vec![
            entry(0, "Burnout 3 - Takedown", Some(2004), &[], &["Burnout"], &[]),
            entry(1, "Burnout", Some(2001), &[], &[], &[]),
            entry(2, "Burnout Legends", Some(2005), &[], &[], &[]),
        ]);
        assert_eq!(ix.derived_series(Path::new("/roms/0 Burnout 3 - Takedown.iso")), None, "the data's own series");
        let l = ix.link(Kind::Series, "Burnout", Path::new("/roms/0 Burnout 3 - Takedown.iso")).unwrap();
        assert_eq!(names(&ix, &l), ["Burnout", "Burnout 3 - Takedown", "Burnout Legends"]);
    }

    #[test]
    fn a_developer_leads_to_the_others_under_any_spelling_and_never_to_an_empty_list() {
        let ix = Index::build(vec![
            entry(0, "Burnout 3 - Takedown", Some(2004), &["Criterion Games"], &[], &["Racing"]),
            entry(1, "Black", Some(2006), &["Criterion Studios"], &[], &["First-person shooter"]),
            entry(2, "AirBlade", Some(2001), &["Criterion"], &[], &["Sports"]),
            entry(3, "Advance Wars", Some(2001), &["Intelligent Systems"], &[], &["Strategy"]),
            entry(4, "Gran Turismo 4", Some(2004), &["Polyphony Digital"], &[], &["racing video game"]),
            entry(5, "Ridge Racer", None, &["Namco"], &[], &["Race, Driving"]),
        ]);
        let me = Path::new("/roms/0 Burnout 3 - Takedown.iso");
        let l = ix.link(Kind::Developer, "Criterion Games", me).unwrap();
        assert_eq!(l.title(), "More from Criterion Games");
        assert_eq!(names(&ix, &l), ["AirBlade", "Black"], "release order, without this game");
        assert!(ix.link(Kind::Developer, "Intelligent Systems", Path::new("/roms/3 Advance Wars.iso")).is_none(), "nobody else: plain text");
        let g = ix.link(Kind::Genre, "Racing", me).unwrap();
        assert_eq!(g.title(), "More racing games");
        assert_eq!(names(&ix, &g), ["Gran Turismo 4", "Ridge Racer"], "by name");
        // The card shows two values joined: the first that leads somewhere.
        let l = ix.link(Kind::Developer, "Acclaim, Criterion Games", me).unwrap();
        assert_eq!(l.value, "Criterion Games");
        assert!(ix.link(Kind::Publisher, "Criterion Games", me).is_none(), "a developer isn't a publisher");
        assert!(ix.link(Kind::Developer, "Criterion", Path::new("/roms/9 Not in the library.iso")).is_some());
    }

    #[test]
    fn speed_of_the_index_for_5000_games() {
        // 5,000 games in 500 series of ten numbered titles and a base title each, every one with a developer and genre.
        let word = |mut n: usize| {
            let mut w = String::from("Q");
            loop {
                w.push((b'a' + (n % 26) as u8) as char);
                n /= 26;
                if n == 0 {
                    return w;
                }
            }
        };
        let entries: Vec<Entry> = (0..5000)
            .map(|i| {
                let name = if i % 10 == 0 { format!("{} Quest", word(i / 10)) } else { format!("{} Quest {}", word(i / 10), i % 10) };
                entry(i, &name, Some(1990 + (i % 30) as u16), &[&format!("Studio {} Games", i % 300)], &[], &["Action video game"])
            })
            .collect();
        let t = std::time::Instant::now();
        let ix = Index::build(entries);
        let built = t.elapsed();
        let p = PathBuf::from("/roms/12 Qb Quest 2.iso");
        assert_eq!(ix.derived_series(&p), Some("Qb Quest"));
        let t = std::time::Instant::now();
        for kind in Kind::ALL {
            let _ = ix.link(kind, "Studio 12 Games, Qb Quest, Action", &p);
        }
        let looked = t.elapsed();
        println!("index of 5000 games: {built:?}, four lookups {looked:?}");
        // The build runs on Curator's thread; the lookups when a card opens (a genre's list has every game).
        assert!(built < std::time::Duration::from_millis(400), "{built:?}");
        assert!(looked < std::time::Duration::from_millis(20), "{looked:?}");
    }

    #[test]
    fn rediscover_puts_a_year_ago_today_first_then_long_unplayed_games_by_time_played() {
        use crate::play_history::day_of;
        let day = 86_400u64;
        let now = (day_of(2026, 10, 29) as u64) * day + 20 * 3600;
        let year_ago = (day_of(2025, 10, 29) as u64) * day + 18 * 3600;
        let p = |s: &str| PathBuf::from(s);
        let played = vec![
            (p("/a"), now - 3 * day, 100),
            (p("/b"), now - 400 * day, 5_000),
            (p("/c"), now - 500 * day, 90_000),
            (p("/d"), year_ago, 10),
            (p("/e"), now - 30 * day, 50),
            (p("/never"), 0, 0),
        ];
        // /e was played a year ago today too, and since.
        let days = vec![(p("/e"), year_ago - 3600, 0), (p("/e"), year_ago, 0)];
        let r = rediscover(&played, &days, now, 0);
        assert_eq!(r, vec![(p("/e"), Why::YearAgoToday), (p("/d"), Why::YearAgoToday), (p("/c"), Why::NotForAYear), (p("/b"), Why::NotForAYear)]);
        assert!(rediscover(&[], &[], now, 0).is_empty());
        // 29 February: the 28th a year before.
        let leap_now = (day_of(2028, 2, 29) as u64) * day + 3600;
        let feb28 = (day_of(2027, 2, 28) as u64) * day + 3600;
        assert_eq!(rediscover(&[(p("/f"), feb28, 1)], &[], leap_now, 0), vec![(p("/f"), Why::YearAgoToday)]);
    }
}
