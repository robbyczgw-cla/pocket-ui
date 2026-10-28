//! Settings → Games → From EmulationStation: what the player's EmulationStation gamelists (`gamelist`) hold that
//! pocket-ui can take over, and the two offers: the favorites into Favourites, the ratings into My rating. Nothing is
//! imported unasked and nothing is written to EmulationStation's files. This is the state of the page and the words;
//! the App does the work (`app_gamelist.rs`).
//!
//! Both offers only add: a game that is a favorite already stays one, a game the player rated themselves keeps their
//! stars. Ask again any time; the second time there is nothing new to add and the page says so.

use std::path::Path;

use crate::gamelist::Index;
use crate::pad::Button;

/// What the gamelists hold for the games on the shelf.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Found {
    /// Gamelist files read (0: none found).
    pub files: usize,
    /// Games on the shelf the gamelists mark as favorites, and those of them that aren't in Favourites yet.
    pub favorites: usize,
    pub new_favorites: usize,
    /// Games on the shelf the gamelists give stars, and those of them the player has not rated.
    pub ratings: usize,
    pub new_ratings: usize,
}

impl Found {
    /// Counts for `games` (system id, ROM) of the shelf; `is_favorite` and `own_rating` are the player's own data.
    pub fn count(index: &Index, games: &[(&str, &Path)], is_favorite: &dyn Fn(&Path) -> bool, own_rating: &dyn Fn(&Path) -> u8) -> Found {
        let favorites = index.favorites(games.iter().copied());
        let ratings = index.ratings(games.iter().copied());
        Found {
            files: index.files(),
            favorites: favorites.len(),
            new_favorites: favorites.iter().filter(|(_, p)| !is_favorite(p)).count(),
            ratings: ratings.len(),
            new_ratings: ratings.iter().filter(|(p, _)| own_rating(p) == 0).count(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Row {
    Favorites,
    Ratings,
}

pub const ROWS: [Row; 2] = [Row::Favorites, Row::Ratings];

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    Nothing,
    Close,
    AddFavorites,
    AddRatings,
}

#[derive(Debug)]
pub struct Page {
    pub sel: usize,
    pub found: Found,
    /// The last result: text, is an error.
    pub message: Option<(String, bool)>,
}

/// "1 game", "42 games".
pub fn games(n: usize) -> String {
    if n == 1 {
        "1 game".into()
    } else {
        format!("{n} games")
    }
}

impl Page {
    pub fn new(found: Found) -> Page {
        Page { sel: 0, found, message: None }
    }

    pub fn row(&self) -> Row {
        ROWS[self.sel.min(ROWS.len() - 1)]
    }

    /// New games the selected row would add.
    pub fn to_add(&self) -> usize {
        match self.row() {
            Row::Favorites => self.found.new_favorites,
            Row::Ratings => self.found.new_ratings,
        }
    }

    pub fn press(&mut self, b: Button, confirm: Button, cancel: Button) -> Action {
        let before = self.sel;
        let action = match b {
            b if b == cancel => Action::Close,
            Button::Up => {
                self.sel = (self.sel + ROWS.len() - 1) % ROWS.len();
                Action::Nothing
            }
            Button::Down => {
                self.sel = (self.sel + 1) % ROWS.len();
                Action::Nothing
            }
            b if b == confirm && self.to_add() > 0 => match self.row() {
                Row::Favorites => Action::AddFavorites,
                Row::Ratings => Action::AddRatings,
            },
            _ => Action::Nothing,
        };
        if self.sel != before {
            self.message = None;
        }
        action
    }

    /// The row's value at the right.
    pub fn value(&self, row: Row) -> String {
        let (all, new) = match row {
            Row::Favorites => (self.found.favorites, self.found.new_favorites),
            Row::Ratings => (self.found.ratings, self.found.new_ratings),
        };
        match (self.found.files, all, new) {
            (0, _, _) => "No gamelist".into(),
            (_, 0, _) => "None found".into(),
            (_, _, 0) => "All added".into(),
            (_, _, n) => format!("{n} to add"),
        }
    }

    /// After an offer was taken: the page counts again (`found`) and says what happened.
    pub fn done(&mut self, found: Found, message: String) {
        self.found = found;
        self.message = Some((message, false));
    }

    pub fn failed(&mut self, why: String) {
        self.message = Some((why, true));
    }

    /// The hint for A; None when A does nothing (nothing new, or no gamelist).
    pub fn a_hint(&self) -> Option<&'static str> {
        (self.to_add() > 0).then_some(match self.row() {
            Row::Favorites => "Add to Favourites",
            Row::Ratings => "Use as My rating",
        })
    }

    /// The details under the list: a heading and up to two lines.
    pub fn details(&self) -> (String, Vec<String>) {
        if self.found.files == 0 {
            return (
                "No gamelist found".into(),
                vec![
                    "pocket-ui reads the gamelist.xml of each system from its ROM folder, like roms/gba/gamelist.xml, and from ES-DE's folder. Copy yours there, then open this page again.".into(),
                    "Your files are only read, never changed.".into(),
                ],
            );
        }
        let f = &self.found;
        match self.row() {
            Row::Favorites => (
                format!("Your EmulationStation favourites: {}", games(f.favorites)),
                vec![match (f.favorites, f.new_favorites) {
                    (0, _) => "No game on your shelf is marked as a favourite in your gamelists.".to_string(),
                    (_, 0) => "All of them are in Favourites already.".to_string(),
                    _ => format!("Add {} to Favourites? Games that are there already stay.", games(f.new_favorites)),
                }],
            ),
            Row::Ratings => (
                format!("Your EmulationStation ratings: {}", games(f.ratings)),
                vec![match (f.ratings, f.new_ratings) {
                    (0, _) => "No game on your shelf has a rating in your gamelists.".to_string(),
                    (_, 0) => "Every one of them has your rating already.".to_string(),
                    _ => format!("Use them as My rating, in stars, for {}? Games you rated yourself keep your rating.", games(f.new_ratings)),
                }],
            ),
        }
    }
}

/// What the page says after the favorites were added: `added` new, `before` of the marked games were there already.
pub fn favorites_added(added: usize, before: usize) -> String {
    match (added, before) {
        (0, _) => "Nothing new to add.".into(),
        (1, 0) => "Added 1 game to Favourites.".into(),
        (n, 0) => format!("Added {n} games to Favourites."),
        (1, _) => "Added 1 game to Favourites. The others were there already.".into(),
        (n, _) => format!("Added {n} games to Favourites. The others were there already."),
    }
}

/// What the page says after the ratings were given.
pub fn ratings_added(given: usize) -> String {
    match given {
        0 => "Nothing new to add.".into(),
        1 => "Gave stars to 1 game.".into(),
        n => format!("Gave stars to {n} games."),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gamelist::Locations;
    use crate::testutil::TestDir;
    use std::path::PathBuf;

    const A: Button = Button::East;
    const B: Button = Button::South;

    fn found(files: usize, favorites: usize, new_favorites: usize, ratings: usize, new_ratings: usize) -> Found {
        Found { files, favorites, new_favorites, ratings, new_ratings }
    }

    #[test]
    fn a_adds_only_what_is_new_and_b_goes_back() {
        let mut p = Page::new(found(2, 5, 3, 4, 0));
        assert_eq!(p.a_hint(), Some("Add to Favourites"));
        assert_eq!(p.press(A, A, B), Action::AddFavorites);
        p.press(Button::Down, A, B);
        assert_eq!(p.row(), Row::Ratings);
        assert_eq!(p.a_hint(), None, "every rated game has the player's rating: nothing to give");
        assert_eq!(p.press(A, A, B), Action::Nothing);
        assert_eq!(p.press(B, A, B), Action::Close);
        let mut none = Page::new(found(0, 0, 0, 0, 0));
        assert_eq!((none.press(A, A, B), none.a_hint()), (Action::Nothing, None), "no gamelist: A does nothing");
    }

    #[test]
    fn every_state_says_something() {
        for f in [found(0, 0, 0, 0, 0), found(3, 0, 0, 0, 0), found(3, 4, 4, 2, 2), found(3, 4, 0, 2, 0), found(1, 1, 1, 1, 1)] {
            for sel in 0..ROWS.len() {
                let mut p = Page::new(f);
                p.sel = sel;
                let (head, lines) = p.details();
                assert!(!head.is_empty() && lines.iter().all(|l| !l.is_empty()) && !lines.is_empty(), "{f:?} row {sel}");
                for row in ROWS {
                    assert!(!p.value(row).is_empty());
                }
            }
        }
        let p = Page::new(found(3, 42, 40, 0, 0));
        assert_eq!(p.value(Row::Favorites), "40 to add");
        assert_eq!(p.value(Row::Ratings), "None found");
        assert_eq!(p.details().0, "Your EmulationStation favourites: 42 games");
        assert_eq!(Page::new(found(0, 0, 0, 0, 0)).value(Row::Favorites), "No gamelist");
        assert_eq!(Page::new(found(3, 5, 0, 0, 0)).value(Row::Favorites), "All added");
    }

    #[test]
    fn the_words_after_an_offer() {
        assert_eq!(favorites_added(0, 3), "Nothing new to add.");
        assert_eq!(favorites_added(1, 0), "Added 1 game to Favourites.");
        assert_eq!(favorites_added(38, 4), "Added 38 games to Favourites. The others were there already.");
        assert_eq!(ratings_added(15), "Gave stars to 15 games.");
        let mut p = Page::new(found(1, 2, 2, 0, 0));
        p.done(found(1, 2, 0, 0, 0), favorites_added(2, 0));
        assert_eq!(p.message, Some(("Added 2 games to Favourites.".into(), false)));
        assert_eq!(p.a_hint(), None);
        p.failed("no room".into());
        assert!(p.message.as_ref().is_some_and(|m| m.1));
        p.press(Button::Down, A, B);
        assert_eq!(p.message, None, "moving away clears it");
    }

    #[test]
    fn counts_come_from_the_gamelists_the_shelf_and_the_players_own_data() {
        let dir = TestDir::new("gamelist-page");
        std::fs::create_dir_all(dir.join("roms/gba")).unwrap();
        std::fs::write(
            dir.join("roms/gba/gamelist.xml"),
            "<gameList><game><path>./a.gba</path><favorite>true</favorite><rating>1</rating></game>\
             <game><path>./b.gba</path><favorite>true</favorite><rating>0.5</rating></game>\
             <game><path>./c.gba</path><rating>0.3</rating></game></gameList>",
        )
        .unwrap();
        let index = Index::load(&dir.join("data"), &Locations::none(), &[("gba".to_string(), dir.join("roms/gba"))]);
        let paths: Vec<PathBuf> = ["a.gba", "b.gba", "c.gba", "d.gba"].iter().map(|n| dir.join("roms/gba").join(n)).collect();
        let games: Vec<(&str, &Path)> = paths.iter().map(|p| ("gba", p.as_path())).collect();
        let a = paths[0].clone();
        let f = Found::count(&index, &games, &|p| p == a, &|p| u8::from(p.ends_with("b.gba")));
        assert_eq!(f, Found { files: 1, favorites: 2, new_favorites: 1, ratings: 3, new_ratings: 2 });
    }
}
