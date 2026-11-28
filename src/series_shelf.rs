//! The shelf by series (Settings › Look › Layout › Order: By series; docs/SERIES-SHELF.md): Home's systems make way
//! for one shelf per game series, every game of it from every console in release order, and one more shelf, "One of a
//! kind", for the games of no series and the only game of theirs on the Pocket. Pure logic; Connections' index
//! ([`crate::connections::Index`]) arranges its games so when it is built, on Curator's thread, and the App only
//! filters that for the systems and games the shelf shows ([`Arrangement::shown`]).
//!
//! Which series a game is on:
//! 1. The series of [`crate::series_facts`] among the game's series (its data's, ScreenScraper's family from its
//!    cache, the one its title gives), the first of them: their aliases put a series' sub-series on its shelf ("Super
//!    Mario Land" on Super Mario's).
//! 2. Else the first of its series that another title of the player's has too.
//! 3. Else none: One of a kind. So is a series of one title once the others are counted: the same game on two
//!    systems is one title (`connections`' rule), and a series needs two.
//!
//! On a shelf: by release year, then by the number in the series (ScreenScraper's), then by name; a game without a
//! year after the ones with one. The shelves by name as Home sorts its systems ("The" doesn't count), One of a kind
//! last, its games by name.

use std::collections::{HashMap, HashSet};

use crate::connections::{key, parts, Kind};

/// The name of the shelf of the games of no series, as Home and the cabinet's sign show it.
pub const ONE_OF_A_KIND: &str = "One of a kind";

/// One series' shelf: its name, its card's facts (an index into `series_facts::ALL`) and its games (indexes into
/// the index's games) in shelf order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Shelf {
    pub name: String,
    pub facts: Option<usize>,
    pub games: Vec<usize>,
}

/// The library by series: the series' shelves in Home's order, then the games of none.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Arrangement {
    pub shelves: Vec<Shelf>,
    pub alone: Vec<usize>,
}

/// What a game brings to the arrangement.
pub struct Member<'a> {
    pub name: &'a str,
    pub year: Option<u16>,
    /// Its place in its series, where ScreenScraper says it.
    pub number: Option<u32>,
    /// Its series as the data writes them, most particular first: the game data's, ScreenScraper's, then its title's.
    pub series: &'a [String],
    /// Its title as `connections::key` gives it: one title on two systems is one.
    pub title: &'a str,
}

/// Where a game stands: a series with a card, or another series by its key.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
enum Place {
    Known(usize),
    Plain(String),
}

fn shelf_order(members: &[Member], games: &mut [usize]) {
    games.sort_by_cached_key(|&i| {
        let m = &members[i];
        (m.year.is_none(), m.year, m.number.unwrap_or(u32::MAX), m.name.to_lowercase(), i)
    });
}

fn alone_order(members: &[Member], games: &mut [usize]) {
    games.sort_by_cached_key(|&i| (members[i].name.to_lowercase(), i));
}

/// The arrangement of `members`, the games of the library in the index's order. One pass over their series to count
/// the titles per series, one to place them, one sort per shelf: a few milliseconds for 5,000 games
/// (`speed_for_5000_games`).
pub fn arrange(members: &[Member]) -> Arrangement {
    // Each game's series as keys, with the name as first written.
    let keys: Vec<Vec<(String, &str)>> = members
        .iter()
        .map(|m| {
            let mut ks: Vec<(String, &str)> = Vec::new();
            for v in m.series {
                for p in parts(v) {
                    let k = key(Kind::Series, p);
                    if !k.is_empty() && !ks.iter().any(|(x, _)| *x == k) {
                        ks.push((k, p));
                    }
                }
            }
            ks
        })
        .collect();
    // Titles per plain series key, to tell a series of two titles from one game on two systems.
    let mut titles: HashMap<&str, HashSet<&str>> = HashMap::new();
    for (m, ks) in members.iter().zip(&keys) {
        for (k, _) in ks {
            titles.entry(k.as_str()).or_default().insert(m.title);
        }
    }
    let mut names: HashMap<Place, String> = HashMap::new();
    let mut groups: HashMap<Place, Vec<usize>> = HashMap::new();
    let mut alone = Vec::new();
    for (i, ks) in keys.iter().enumerate() {
        let known = ks.iter().find_map(|(k, _)| crate::series_facts::find(k)).map(Place::Known);
        let place = known.or_else(|| {
            ks.iter().find(|(k, _)| titles.get(k.as_str()).is_some_and(|t| t.len() >= 2)).map(|(k, written)| {
                let p = Place::Plain(k.clone());
                names.entry(p.clone()).or_insert_with(|| written.trim().to_string());
                p
            })
        });
        match place {
            Some(p) => groups.entry(p).or_default().push(i),
            None => alone.push(i),
        }
    }
    let mut shelves = Vec::new();
    for (place, mut games) in groups {
        let distinct: HashSet<&str> = games.iter().map(|&i| members[i].title).collect();
        if distinct.len() < 2 {
            alone.extend(games);
            continue;
        }
        shelf_order(members, &mut games);
        let (name, facts) = match &place {
            Place::Known(f) => (crate::series_facts::ALL[*f].name.to_string(), Some(*f)),
            Place::Plain(_) => (names.remove(&place).unwrap_or_default(), None),
        };
        shelves.push(Shelf { name, facts, games });
    }
    shelves.sort_by_cached_key(|s| (key(Kind::Series, &s.name), s.name.clone()));
    alone_order(members, &mut alone);
    Arrangement { shelves, alone }
}

/// A shelf as Home shows it: the series' shelves, One of a kind last (`alone`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Shown {
    pub name: String,
    pub facts: Option<usize>,
    pub games: Vec<usize>,
    pub alone: bool,
}

impl Arrangement {
    /// The shelves with only the games `keep` lets on (a hidden system's, a game in Shelf space's crate are not): a
    /// series left with one title joins One of a kind, an empty one goes. `title`: each game's title key. One of a
    /// kind is there while it has a game.
    pub fn shown(&self, keep: impl Fn(usize) -> bool, members: &[Member]) -> Vec<Shown> {
        let mut out = Vec::new();
        let mut alone: Vec<usize> = self.alone.iter().copied().filter(|&i| keep(i)).collect();
        for s in &self.shelves {
            let games: Vec<usize> = s.games.iter().copied().filter(|&i| keep(i)).collect();
            let distinct: HashSet<&str> = games.iter().map(|&i| members[i].title).collect();
            if distinct.len() >= 2 {
                out.push(Shown { name: s.name.clone(), facts: s.facts, games, alone: false });
            } else {
                alone.extend(games);
            }
        }
        if !alone.is_empty() {
            alone_order(members, &mut alone);
            out.push(Shown { name: ONE_OF_A_KIND.into(), facts: None, games: alone, alone: true });
        }
        out
    }
}

/// "ii", "iv", "x" as the cleaned key writes a sequel's number.
fn roman_numeral(w: &str) -> bool {
    !w.is_empty() && w.len() <= 5 && w.chars().all(|c| matches!(c, 'i' | 'v' | 'x'))
}

/// A game's name on a series' shelf, where the series is said already: what tells it from the others. The part after
/// the series and its separator ("The Legend of Zelda - A Link to the Past" is "A Link to the Past", "Zelda II - The
/// Adventure of Link" "II: The Adventure of Link"), the year for the game named as its series ("The Legend of Zelda", 1986),
/// else the whole name ("Super Mario World", "Mario Kart 64"). `series`: the series' name and aliases.
pub fn short_title(series: &[&str], name: &str, year: Option<u16>) -> String {
    let whole = key(Kind::Series, name);
    let keys: Vec<String> = series.iter().map(|s| key(Kind::Series, s)).filter(|k| !k.is_empty()).collect();
    // The game named as the series itself: its year ("The Legend of Zelda", 1986). A sub-series' name ("Mega Man X" on
    // Mega Man's shelf) says enough as it is.
    if keys.first() == Some(&whole) {
        return year.map_or_else(|| name.to_string(), |y| y.to_string());
    }
    if keys.contains(&whole) {
        return name.to_string();
    }
    // The series and a number: the number ("Sonic the Hedgehog 2" is "2", "Final Fantasy VII" "VII").
    let last = name.split_whitespace().last().unwrap_or("");
    let numeral = |w: &str| !w.is_empty() && (w.chars().all(|c| c.is_ascii_digit()) || roman_numeral(&w.to_ascii_lowercase()));
    if numeral(last) && keys.iter().any(|k| whole.strip_prefix(k.as_str()).and_then(|t| t.strip_prefix(' ')) == Some(key(Kind::Series, last).as_str())) {
        return last.to_string();
    }
    let cut = [" - ", ": ", ":"].iter().filter_map(|sep| name.find(sep).map(|at| (at, sep.len()))).min();
    if let Some((at, len)) = cut {
        let (head, rest) = (key(Kind::Series, &name[..at]), name[at + len..].trim());
        // The series itself before the separator, with at most its number or a moved "The" ("Zelda II", "Legend of
        // Zelda, The"): not a longer name that starts with it ("Mario Kart" is not the Mario series' part).
        let starts = |k: &String| {
            head == *k
                || head.strip_prefix(k.as_str()).and_then(|t| t.strip_prefix(' ')).is_some_and(|t| t == "the" || t.chars().all(|c| c.is_ascii_digit()) || roman_numeral(t))
        };
        // A sub-series' word before the separator goes with the series ("Ace Attorney Investigations: Miles Edgeworth" is
        // "Miles Edgeworth", "Mario Kart - Super Circuit" on a Mario shelf "Super Circuit").
        let longer = |k: &String| head.starts_with(&format!("{k} "));
        if !rest.is_empty() && (keys.iter().any(starts) || keys.iter().any(longer)) {
            // A leading "The" says nothing on a tile ("Minish Cap"); a word or more must stay.
            let rest = match rest.split_once(' ') {
                Some((the, more)) if the.eq_ignore_ascii_case("the") && !more.trim().is_empty() => more.trim(),
                _ => rest,
            };
            // Its number stays: "Final Fantasy IV - Advance" and "Final Fantasy V - Advance" are "IV: Advance" and "V: Advance".
            let last = name[..at].split_whitespace().last().unwrap_or("");
            return if numeral(last) && head != key(Kind::Series, last) { format!("{last}: {rest}") } else { rest.to_string() };
        }
    }
    // The series' words first, then the rest ("Star Fox Adventures" is "Adventures", "Kirby & the Amazing Mirror"
    // "Amazing Mirror", "The King of Fighters '98" "'98").
    // Only a name without a subtitle ("Star Fox Adventures"); one with a subtitle was decided above.
    let words: Vec<&str> = if cut.is_some() { Vec::new() } else { name.split_whitespace().collect() };
    for n in 1..words.len() {
        if keys.iter().any(|k| key(Kind::Series, &words[..n].join(" ")) == *k) {
            let mut rest = &words[n..];
            while rest.len() > 1 && matches!(rest[0].to_ascii_lowercase().as_str(), "&" | "and" | "the" | "-" | ":") {
                rest = &rest[1..];
            }
            return rest.join(" ");
        }
    }
    name.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    struct G {
        name: &'static str,
        year: Option<u16>,
        number: Option<u32>,
        series: Vec<String>,
        title: String,
    }

    fn g(name: &'static str, year: Option<u16>, series: &[&str]) -> G {
        G { name, year, number: None, series: series.iter().map(|s| s.to_string()).collect(), title: key(Kind::Series, name) }
    }

    fn members(gs: &[G]) -> Vec<Member<'_>> {
        gs.iter().map(|x| Member { name: x.name, year: x.year, number: x.number, series: &x.series, title: &x.title }).collect()
    }

    fn names(gs: &[G], games: &[usize]) -> Vec<&'static str> {
        games.iter().map(|&i| gs[i].name).collect()
    }

    #[test]
    fn a_series_from_every_console_stands_in_release_order_and_its_sub_series_join_it() {
        let gs = [
            g("Super Mario World", Some(1990), &["Super Mario", "Mario"]),
            g("Super Mario Land 2: 6 Golden Coins", Some(1992), &["Super Mario Land", "Super Mario"]),
            g("Super Mario 64", Some(1996), &["Super Mario"]),
            g("Super Mario Bros.", Some(1985), &["Super Mario"]),
            g("Mario Kart 64", Some(1996), &["Mario Kart"]),
            g("Super Mario Kart", Some(1992), &["Mario Kart"]),
            g("Tetris", Some(1989), &[]),
        ];
        let a = arrange(&members(&gs));
        let mario = a.shelves.iter().find(|s| s.name == "Super Mario").expect("Super Mario's shelf");
        assert!(mario.facts.is_some(), "a series with a card");
        assert_eq!(names(&gs, &mario.games), ["Super Mario Bros.", "Super Mario World", "Super Mario Land 2: 6 Golden Coins", "Super Mario 64"]);
        let kart = a.shelves.iter().find(|s| s.name == "Mario Kart").expect("Mario Kart is its own series");
        assert_eq!(names(&gs, &kart.games), ["Super Mario Kart", "Mario Kart 64"]);
        assert_eq!(names(&gs, &a.alone), ["Tetris"]);
    }

    #[test]
    fn one_title_is_no_series_and_the_same_game_on_two_systems_is_one_title() {
        let gs = [
            g("Burnout 3 - Takedown", Some(2004), &["Burnout"]),
            g("Burnout 3 - Takedown", Some(2004), &["Burnout"]),
            g("Ikaruga", Some(2001), &["Ikaruga Series"]),
            g("Advance Wars", Some(2001), &[]),
        ];
        let a = arrange(&members(&gs));
        assert!(a.shelves.is_empty(), "{:?}", a.shelves);
        assert_eq!(names(&gs, &a.alone), ["Advance Wars", "Burnout 3 - Takedown", "Burnout 3 - Takedown", "Ikaruga"], "every game somewhere, by name");
    }

    #[test]
    fn a_series_without_a_card_takes_the_first_of_the_games_series_that_another_title_shares() {
        let gs = [
            g("Skyrunner 3", Some(2004), &["Northwind racers", "Skyrunner"]),
            g("Skyrunner Revenge", Some(2005), &["Skyrunner"]),
            g("Skyrunner 2", None, &["Skyrunner series"]),
        ];
        let a = arrange(&members(&gs));
        assert_eq!(a.shelves.len(), 1, "{:?}", a.shelves);
        assert_eq!(a.shelves[0].name, "Skyrunner");
        assert_eq!(a.shelves[0].facts, None);
        assert_eq!(names(&gs, &a.shelves[0].games), ["Skyrunner 3", "Skyrunner Revenge", "Skyrunner 2"], "no year: last");
    }

    #[test]
    fn the_number_in_the_series_orders_games_of_one_year() {
        let mut gs = [g("Pokemon Gold", Some(1999), &["Pokémon"]), g("Pokemon Silver", Some(1999), &["Pokémon"]), g("Pokemon Red", Some(1996), &["Pokémon"])];
        gs[0].number = Some(3);
        gs[1].number = Some(2);
        let a = arrange(&members(&gs));
        assert_eq!(names(&gs, &a.shelves[0].games), ["Pokemon Red", "Pokemon Silver", "Pokemon Gold"]);
    }

    #[test]
    fn shelves_by_name_without_the_and_what_the_shelf_hides_moves_a_series_to_one_of_a_kind() {
        let gs = [
            g("Zelda II", Some(1987), &["The Legend of Zelda"]),
            g("The Legend of Zelda", Some(1986), &["The Legend of Zelda"]),
            g("Burnout", Some(2001), &["Burnout"]),
            g("Burnout Revenge", Some(2005), &["Burnout"]),
            g("Aero Fighters", Some(1992), &["Aero Fighters"]),
            g("Aero Fighters 2", Some(1994), &["Aero Fighters"]),
        ];
        let m = members(&gs);
        let a = arrange(&m);
        assert_eq!(a.shelves.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["Aero Fighters", "Burnout", "The Legend of Zelda"]);
        // Burnout Revenge's system is hidden: Burnout is one title now.
        let shown = a.shown(|i| i != 3, &m);
        assert_eq!(shown.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["Aero Fighters", "The Legend of Zelda", ONE_OF_A_KIND]);
        assert!(shown.last().unwrap().alone);
        assert_eq!(names(&gs, &shown.last().unwrap().games), ["Burnout"]);
        // Nothing hidden and no game of no series: no One of a kind.
        assert!(a.shown(|_| true, &m).iter().all(|s| !s.alone));
        // Nothing on the shelf: no shelf at all (Home says so itself).
        assert!(a.shown(|_| false, &m).is_empty());
    }

    #[test]
    fn a_shelfs_titles_leave_the_series_out_and_stay_apart_in_every_series_with_a_card() {
        let zelda = ["The Legend of Zelda", "Zelda"];
        assert_eq!(short_title(&zelda, "The Legend of Zelda - A Link to the Past", Some(1991)), "A Link to the Past");
        assert_eq!(short_title(&zelda, "Legend of Zelda, The - Ocarina of Time", Some(1998)), "Ocarina of Time");
        assert_eq!(short_title(&zelda, "Zelda II - The Adventure of Link", Some(1987)), "II: Adventure of Link");
        assert_eq!(short_title(&zelda, "The Legend of Zelda", Some(1986)), "1986", "the first game: its year");
        assert_eq!(short_title(&zelda, "The Legend of Zelda", None), "The Legend of Zelda");
        assert_eq!(short_title(&["Mario Kart"], "Mario Kart - Super Circuit", None), "Super Circuit");
        assert_eq!(short_title(&["Mario Kart"], "Mario Kart 64", None), "64");
        assert_eq!(short_title(&["Mario"], "Mario Kart - Super Circuit", None), "Super Circuit", "a sub-series' word goes with the series");
        assert_eq!(short_title(&["Donkey Kong"], "Donkey Kong Country 2: Diddy's Kong Quest", None), "2: Diddy's Kong Quest");
        assert_eq!(short_title(&["The Legend of Zelda"], "Legend of Zelda, The - The Minish Cap", None), "Minish Cap", "the moved The, and the subtitle's");
        assert_eq!(short_title(&["Sonic the Hedgehog"], "Sonic the Hedgehog 2", Some(1992)), "2", "Neon's review: \"Sonic the Hedgeho…\" twice");
        assert_eq!(short_title(&["Sonic the Hedgehog"], "Sonic the Hedgehog", Some(1991)), "1991");
        assert_eq!(short_title(&["Mega Man", "Mega Man X"], "Mega Man X", Some(1993)), "Mega Man X", "a sub-series keeps its name");
        assert_eq!(short_title(&["Final Fantasy"], "Final Fantasy VII", Some(1997)), "VII");
        assert_eq!(short_title(&["Star Fox"], "Star Fox Adventures", None), "Adventures");
        assert_eq!(short_title(&["Kirby"], "Kirby & the Amazing Mirror", None), "Amazing Mirror");
        assert_eq!(short_title(&["The King of Fighters"], "The King of Fighters '98", None), "'98");
        assert_eq!(short_title(&["Super Mario"], "Super Mario World", None), "World");
        assert_eq!(short_title(&["Bomberman"], "Super Bomberman", None), "Super Bomberman", "the series' word inside, not first");
        assert_eq!(short_title(&["Final Fantasy"], "Final Fantasy IV - Advance", None), "IV: Advance", "the number tells it from V: Advance");
        // The visible titles of a shelf with every key game of each series and the one named as the series: all apart.
        for f in crate::series_facts::ALL {
            let names: Vec<&str> = std::iter::once(f.name).chain(f.aliases.iter().copied()).collect();
            let mut seen = std::collections::HashSet::new();
            for (i, t) in std::iter::once(f.name).chain(f.key.iter().copied()).enumerate() {
                let shown = short_title(&names, t, Some(1980 + i as u16));
                assert!(!shown.is_empty() && seen.insert(shown.to_lowercase()), "{}: {t} shows as {shown}, twice", f.name);
            }
        }
    }

    /// The lead's review 26.11.2026: "II: The Adventu…", "The Minish …". On a cover tile of the Brick's 1024x768 (107 px,
    /// 16 of them margin, five rows; `app_home_panel::paint_title_tile`, the name under the console) no key game of any series with
    /// a card is cut.
    #[test]
    fn no_key_games_tile_title_is_cut_on_the_bricks_tiles() {
        let mut text = crate::text::Text::builtin();
        text.set_translate(false);
        let text = std::cell::RefCell::new(text);
        let mut cut = Vec::new();
        for f in crate::series_facts::ALL {
            let names: Vec<&str> = std::iter::once(f.name).chain(f.aliases.iter().copied()).collect();
            for t in f.key {
                let shown = short_title(&names, t, Some(1990));
                let lines = crate::home_panel::tile_name(
                    &shown,
                    107 - 16,
                    5,
                    &mut |s, px| text.borrow_mut().width(crate::text::Face::Title, px, s),
                    &mut |s, px, max| text.borrow_mut().fit(crate::text::Face::Title, px, s, max),
                )
                .map_or_else(|| vec!["…".to_string()], |(_, l)| l);
                if lines.iter().any(|l| l.contains('…')) {
                    cut.push(format!("{}: {t} as {lines:?}", f.name));
                }
            }
        }
        assert!(cut.is_empty(), "{} cut:\n{}", cut.len(), cut.join("\n"));
    }

    #[test]
    fn speed_for_5000_games() {
        let gs: Vec<G> = (0..5000)
            .map(|i| G { name: "Game", year: Some(1990 + (i % 30) as u16), number: None, series: vec![format!("Series {}", i / 7)], title: format!("game {i}") })
            .collect();
        let m = members(&gs);
        let mut best = std::time::Duration::MAX;
        for _ in 0..5 {
            let t = crate::testutil::clock::CpuInstant::now();
            let a = arrange(&m);
            let _ = a.shown(|i| i % 2 == 0, &m);
            best = best.min(t.elapsed());
            assert_eq!(a.shelves.len(), 715);
        }
        println!("series shelves of 5000 games: {best:?}");
        assert!(best < std::time::Duration::from_millis(200), "{best:?}");
    }
}
