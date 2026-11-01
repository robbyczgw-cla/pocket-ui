//! The shelves of a phone's Games page: the recent games, the favourites and one shelf per system, in the Cabinet's
//! order (`cabinet::Cabinet::build`). A game can stand on several shelves, so the page's flat list ([`Shelves::games`])
//! can hold it more than once and a position in that list says which shelf is meant. On Linux only so the gate tests
//! its rules.

use crate::cabinet::{Cabinet, Case, FormFactor, ShelfKey};
use crate::media::Medium;
use crate::shelf::Item;

/// How many recent games their shelf holds.
pub const RECENT_MOST: usize = 12;

/// One shelf: positions `start..end` of the flat list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Row {
    pub key: ShelfKey,
    pub start: usize,
    pub end: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Shelves {
    pub rows: Vec<Row>,
    /// Every shelf's games one after the other (indexes into the library).
    pub games: Vec<usize>,
    /// The name of `ShelfKey::System(i)`.
    pub systems: Vec<String>,
}

/// The shelves of `games` ((system id, system name) per game, in the page's order): `recent` newest first, at most
/// [`RECENT_MOST`] of them; the systems by name. Empty shelves are left out.
pub fn build(games: &[(&str, &str)], recent: &[usize], favourites: &[usize]) -> Shelves {
    let case = |item: Item, game: usize| Case { item, ff: FormFactor::of(games[game].0), medium: Medium::of(games[game].0) };
    let mut systems: Vec<&str> = games.iter().map(|g| g.1).collect();
    systems.sort_unstable();
    systems.dedup();
    let cases = |s: usize| -> Vec<Case> {
        games.iter().enumerate().filter(|(_, g)| g.1 == systems[s]).map(|(i, _)| case(Item::Game(s, i), i)).collect()
    };
    let cabinet = Cabinet::build(
        recent.iter().take(RECENT_MOST).map(|&g| case(Item::Recent(g), g)).collect(),
        favourites.iter().map(|&g| case(Item::Game(0, g), g)).collect(),
        Vec::new(),
        (0..systems.len()).map(|s| (s, cases(s))).collect(),
    );
    let mut out = Shelves { systems: systems.iter().map(|s| s.to_string()).collect(), ..Default::default() };
    for shelf in &cabinet.shelves {
        let start = out.games.len();
        out.games.extend(shelf.cases.iter().filter_map(|c| match c.item {
            Item::Recent(g) | Item::Game(_, g) => Some(g),
            _ => None,
        }));
        out.rows.push(Row { key: shelf.key, start, end: out.games.len() });
    }
    out
}

impl Shelves {
    /// The shelf position `pos` of the flat list stands on.
    pub fn row_of(&self, pos: usize) -> Option<usize> {
        self.rows.iter().position(|r| (r.start..r.end).contains(&pos))
    }

    /// Where `game` stands: on the shelf `key` while it is there, otherwise its first place (after the shelves changed,
    /// the selection stays on its game).
    pub fn place_of(&self, game: usize, key: Option<ShelfKey>) -> Option<usize> {
        let on = |r: &Row| (r.start..r.end).find(|&p| self.games[p] == game);
        key.and_then(|k| self.rows.iter().find(|r| r.key == k)).and_then(on).or_else(|| self.rows.iter().find_map(on))
    }

    /// The name of shelf `row`, untranslated (a system's name stays as it is).
    pub fn title(&self, row: usize) -> &str {
        match self.rows[row].key {
            ShelfKey::Continue => "Recent games",
            ShelfKey::Favorites => "Favourites",
            ShelfKey::System(s) => &self.systems[s],
            _ => "",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GAMES: [(&str, &str); 4] = [("snes", "Super Nintendo"), ("gba", "Game Boy Advance"), ("gba", "Game Boy Advance"), ("psp", "PlayStation Portable")];

    fn keys(s: &Shelves) -> Vec<ShelfKey> {
        s.rows.iter().map(|r| r.key).collect()
    }

    #[test]
    fn a_library_nothing_was_played_on_has_one_shelf_per_system_by_name() {
        let s = build(&GAMES, &[], &[]);
        assert_eq!(keys(&s), [ShelfKey::System(0), ShelfKey::System(1), ShelfKey::System(2)], "no empty shelves");
        assert_eq!(s.systems, ["Game Boy Advance", "PlayStation Portable", "Super Nintendo"]);
        assert_eq!(s.games, [1, 2, 3, 0]);
        assert_eq!(s.title(2), "Super Nintendo");
    }

    #[test]
    fn recent_games_then_favourites_stand_on_top_and_a_game_keeps_its_systems_shelf() {
        let s = build(&GAMES, &[2, 0], &[3]);
        assert_eq!(s.rows[0], Row { key: ShelfKey::Continue, start: 0, end: 2 });
        assert_eq!(s.rows[1], Row { key: ShelfKey::Favorites, start: 2, end: 3 });
        assert_eq!(s.games, [2, 0, 3, 1, 2, 3, 0]);
        assert_eq!((s.title(0), s.title(1)), ("Recent games", "Favourites"));
    }

    #[test]
    fn the_recent_shelf_holds_a_dozen() {
        let games = vec![("gb", "Game Boy"); 20];
        let recent: Vec<usize> = (0..20).rev().collect();
        let s = build(&games, &recent, &[]);
        assert_eq!(s.rows[0].end, RECENT_MOST);
        assert_eq!(s.games[0], 19, "newest first");
    }

    #[test]
    fn a_position_says_which_shelf_is_meant() {
        let s = build(&GAMES, &[2], &[]);
        assert_eq!(s.row_of(0), Some(0));
        assert_eq!(s.row_of(1), Some(1));
        assert_eq!(s.row_of(s.games.len()), None);
    }

    #[test]
    fn the_selection_stays_on_its_game_and_its_shelf_while_it_can() {
        let s = build(&GAMES, &[2], &[]);
        assert_eq!(s.place_of(2, Some(ShelfKey::System(0))), Some(2), "on the system's shelf it was on");
        assert_eq!(s.place_of(2, Some(ShelfKey::Favorites)), Some(0), "that shelf is gone: its first place");
        assert_eq!(s.place_of(2, None), Some(0));
        assert_eq!(build(&GAMES, &[], &[]).place_of(9, None), None);
    }

    #[test]
    fn no_games_no_shelves() {
        assert!(build(&[], &[], &[]).rows.is_empty());
    }

    #[test]
    fn favourites_stand_by_case_as_in_the_cabinet() {
        let s = build(&GAMES, &[], &[3, 1]);
        let rank = |g: usize| FormFactor::of(GAMES[g].0);
        let row = s.rows[0];
        let got = &s.games[row.start..row.end];
        assert_eq!(got.len(), 2);
        let sorted = {
            let mut c = vec![3usize, 1];
            let cab = Cabinet::build(Vec::new(), c.iter().map(|&g| Case { item: Item::Game(0, g), ff: rank(g), medium: Medium::of(GAMES[g].0) }).collect(), Vec::new(), Vec::new());
            c = cab.shelves[0].cases.iter().map(|x| if let Item::Game(_, g) = x.item { g } else { 0 }).collect();
            c
        };
        assert_eq!(got, sorted.as_slice());
    }

    /// Android's Favourite (`android/src/ui.rs`, which has no host tests): the new shelf moves every position, so the game
    /// is read before the rows are rebuilt, in `reshelve`. A `filter` first made it another game.
    #[test]
    fn android_favourite_keeps_the_game_by_reading_it_before_the_rows_change() {
        let ui = include_str!("../android/src/ui.rs");
        let west = &ui[ui.find("if b == Button::West {").unwrap()..];
        let west = &west[..west.find("Command::Persist").unwrap()];
        assert!(west.find("self.reshelve()").unwrap() < west.find("self.filter()").unwrap(), "{west}");
    }
}
