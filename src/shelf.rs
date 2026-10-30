//! What the shelf lists, which hints its bottom bar shows, and what START does there.
//!
//! Settings is a button, not an entry: START opens it. The list is the same in every look (Shelf, Minimal, Split, Cards): they only paint `Item`s differently.

use crate::pad::Button;

/// One entry of the shelf, the lists of a system or Favorites.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Item {
    /// Index into the recent games.
    Recent(usize),
    /// Index into the systems.
    System(usize),
    /// (system, game)
    Game(usize, usize),
    /// The starred games, on Home after the recent ones while there are any.
    Favorites,
    /// "PC streaming" (Moonlight), on Home while Moonlight is installed. An app, not a game: no pause menu,
    /// no recents, no star.
    Streaming,
    /// "Curator" on Home after Favorites: the Lens, the smart shelves and the stickers (docs/CURATOR.md).
    Curator,
    /// The Lens, first in the Curator list.
    Lens,
    /// "Surprise me", under the Lens in the Curator list: opens the card of a game picked for the player.
    Surprise,
    /// A smart shelf (index into curator.json's shelves): in the Curator list, and on Home when pinned.
    SmartShelf(usize),
    /// A sticker (its id in library.json): in the Curator list, and on Home when pinned.
    Sticker(u32),
    /// Curator → History (`app_history`): in the Curator list; on Home when the Lens is off and something was played.
    History,
    /// A year's review: in the Curator list only (it never shows up by itself).
    Review(i32),
    /// Curator → Rediscover (`app_connections`): the games played a year ago today and those not played for a year; in
    /// the Curator list while there are any.
    Rediscover,
    /// "Search", the first row of Home in the list looks (`with_search`): opens the game search (`search_page`).
    Search,
}

impl Item {
    /// Adds the apps to Home's list: "PC streaming" after the recent games and Favorites, before the systems,
    /// when `streaming` (Moonlight is installed).
    pub fn with_apps(mut items: Vec<Item>, streaming: bool) -> Vec<Item> {
        if streaming && !items.contains(&Item::Streaming) {
            let at = items.iter().position(|i| matches!(i, Item::System(_))).unwrap_or(items.len());
            items.insert(at, Item::Streaming);
        }
        items
    }

    /// Adds Curator's rows to Home's list: `rows` (the Curator row, then pinned shelves and stickers) right after
    /// Favorites, or after the recent games when there is no Favorites row. `top` (the list looks): the Curator row
    /// itself goes first instead, above Search and Continue, where it is seen at once (docs/DESIGN.md "Home
    /// panels"); Home still opens on Continue (`first_row`). The cabinet keeps it as a shelf after Favorites.
    pub fn with_curator(mut items: Vec<Item>, mut rows: Vec<Item>, top: bool) -> Vec<Item> {
        if top && rows.first() == Some(&Item::Curator) {
            rows.remove(0);
            items.insert(0, Item::Curator);
        }
        let at = items
            .iter()
            .rposition(|i| matches!(i, Item::Favorites | Item::Recent(_) | Item::Curator))
            .map_or(0, |i| i + 1);
        items.splice(at..at, rows);
        items
    }

    /// One of Curator's rows (they open a list or the Lens, they don't start anything).
    pub fn is_curator(self) -> bool {
        matches!(self, Item::Curator | Item::Lens | Item::Surprise | Item::SmartShelf(_) | Item::Sticker(_) | Item::History | Item::Review(_) | Item::Rediscover)
    }

    /// Puts "Search" above everything else on Home but a Curator row on top, when there are games to search
    /// (`games`). Home still opens on the row after it (`first_row`), so Continue stays one press away and Search is
    /// one press up.
    pub fn with_search(mut items: Vec<Item>, games: bool) -> Vec<Item> {
        if games && !items.contains(&Item::Search) {
            items.insert(usize::from(items.first() == Some(&Item::Curator)), Item::Search);
        }
        items
    }

    /// The row a list opens on: the one after the rows on top of Home (Curator, Search), else the first.
    pub fn first_row(items: &[Item]) -> usize {
        let top = items.iter().take_while(|i| matches!(i, Item::Curator | Item::Search)).count();
        if top < items.len() {
            top
        } else {
            0
        }
    }

    /// Home: the recent games, Favorites (once there is a starred game), then one entry per system.
    /// `systems`: the indices of the systems on the shelf (`visible_systems`).
    pub fn home(recents: impl Iterator<Item = usize>, favorites: bool, systems: impl IntoIterator<Item = usize>) -> Vec<Item> {
        recents.map(Item::Recent).chain(favorites.then_some(Item::Favorites)).chain(systems.into_iter().map(Item::System)).collect()
    }
}

/// What a tick in Settings > Look > Systems on the shelf did.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Toggle {
    Hidden,
    Shown,
    /// Refused: it is the last system left on the shelf.
    LastOne,
}

/// Indices (into the library's systems, given by id) of the systems the shelf shows: all but the hidden ones.
pub fn visible_systems<'a>(ids: impl Iterator<Item = &'a str>, hidden: &[String]) -> Vec<usize> {
    ids.enumerate().filter(|(_, id)| !hidden.iter().any(|h| h == id)).map(|(i, _)| i).collect()
}

/// A recent game shows on Home and in the game switcher unless its system is hidden. Favourites are not
/// filtered: a star is a choice made for that one game.
pub fn recent_shown(system: &str, hidden: &[String]) -> bool {
    !hidden.iter().any(|h| h == system)
}

/// Hides `id`, or shows it again if it was hidden. `with_games`: the systems that have games; the last one of
/// them still on the shelf can't be hidden, or the shelf would be empty.
pub fn toggle_hidden(hidden: &mut Vec<String>, id: &str, with_games: &[&str]) -> Toggle {
    if let Some(i) = hidden.iter().position(|h| h == id) {
        hidden.remove(i);
        return Toggle::Shown;
    }
    let others_shown = with_games.iter().any(|s| *s != id && !hidden.iter().any(|h| h == s));
    if !others_shown {
        return Toggle::LastOne;
    }
    hidden.push(id.to_string());
    Toggle::Hidden
}

/// The key hints of the bottom bar as (key, label); `home` is false in a system's list and in Favourites.
/// `system_page`: the selected system has a console page (`consoles::for_system`).
/// For a smart shelf or sticker, `starred` says whether it is pinned to Home.
pub fn hints(selected: Option<Item>, home: bool, starred: bool, system_page: bool) -> Vec<(&'static str, &'static str)> {
    let action = match selected {
        Some(Item::System(_) | Item::Favorites | Item::Search | Item::Curator | Item::Lens | Item::SmartShelf(_) | Item::Sticker(_) | Item::History | Item::Review(_) | Item::Rediscover) => "Open",
        Some(Item::Surprise) => "Pick",
        Some(Item::Streaming) => "Start",
        _ => "Play",
    };
    let mut hints = vec![("A", action)];
    if !home {
        hints.push(("B", "Back"));
    }
    if let Some(Item::Recent(_) | Item::Game(..)) = selected {
        hints.push(("Y", if starred { "Unstar" } else { "Star" }));
        // X opens the game's card (src/game_card.rs).
        hints.push(("X", "Details"));
    }
    if let Some(Item::SmartShelf(_) | Item::Sticker(_)) = selected {
        hints.push(("Y", if starred { "Unpin" } else { "Pin" }));
        hints.push(("X", "Options"));
    }
    hints.push(("SELECT", "Recent games"));
    // Settings is not an entry of the list: START opens it (as the menu button does in NextUI).
    hints.push(("START", "Settings"));
    if let (Some(Item::System(_)), true) = (selected, system_page) {
        // X opens the console page (src/console_page.rs); last, as in the approved mockup.
        hints.push(("X", "Details"));
    }
    hints
}

/// START pressed and let go on its own opens Settings. SELECT + START is the exit combo and START + Vol
/// is brightness, so START counts only when nothing else was pressed with it.
#[derive(Default, Debug)]
pub struct StartTap {
    armed: bool,
}

impl StartTap {
    /// A button went down on the shelf; `select_held`: SELECT was already down.
    pub fn pressed(&mut self, b: Button, select_held: bool) {
        self.armed = b == Button::Start && !select_held;
    }

    /// Something else used START as a modifier (a volume key).
    pub fn disarm(&mut self) {
        self.armed = false;
    }

    /// A button went up on the shelf: true when that was START alone, which opens Settings.
    pub fn released(&mut self, b: Button) -> bool {
        b == Button::Start && std::mem::take(&mut self.armed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Theme;

    fn names(items: &[Item]) -> Vec<String> {
        items.iter().map(|i| format!("{i:?}")).collect()
    }

    #[test]
    fn the_shelf_list_has_no_settings_entry_in_any_look() {
        for theme in Theme::ALL {
            for favorites in [false, true] {
                let items = Item::home([0usize, 1].into_iter(), favorites, 0..3);
                let names = names(&items);
                assert!(!names.iter().any(|n| n.contains("Settings")), "{theme:?}, favorites {favorites}: {names:?}");
                assert_eq!(items.len(), 2 + favorites as usize + 3, "{theme:?}: recents, favorites, systems and nothing else");
                assert_eq!(items.last(), Some(&Item::System(2)), "{theme:?}: the last system ends the list");
            }
        }
    }

    #[test]
    fn search_is_above_everything_on_home_but_home_opens_on_the_row_after_it() {
        let home = Item::home([0usize, 1].into_iter(), true, 0..3);
        let with = Item::with_search(home.clone(), true);
        assert_eq!(with[0], Item::Search);
        assert_eq!(&with[1..], &home[..], "nothing else moved");
        assert_eq!(Item::first_row(&with), 1, "Continue is selected, Search one press up");
        assert_eq!(Item::with_search(with.clone(), true), with, "not twice");
        assert_eq!(Item::with_search(home.clone(), false), home, "no games, nothing to search");
        assert_eq!(Item::first_row(&home), 0);
        assert_eq!(Item::first_row(&[Item::Search]), 0);
        assert_eq!(Item::first_row(&[]), 0);
        let hints = hints(Some(Item::Search), true, false, false);
        assert!(hints.contains(&("A", "Open")), "{hints:?}");
        assert!(!hints.iter().any(|(k, _)| *k == "Y" || *k == "X"), "nothing to star or show: {hints:?}");
    }

    #[test]
    fn pc_streaming_is_on_home_only_when_moonlight_is_installed() {
        for favorites in [false, true] {
            let base = Item::home([0usize, 1].into_iter(), favorites, 0..3);
            assert_eq!(Item::with_apps(base.clone(), false), base, "no Moonlight, nothing changes (favorites {favorites})");
            let with = Item::with_apps(base.clone(), true);
            assert_eq!(with.len(), base.len() + 1);
            assert_eq!(with.iter().filter(|i| **i == Item::Streaming).count(), 1);
            let at = with.iter().position(|i| *i == Item::Streaming).unwrap();
            assert_eq!(with[at - 1], if favorites { Item::Favorites } else { Item::Recent(1) }, "right after the recent games and Favorites");
            assert_eq!(with[at + 1], Item::System(0), "before the systems");
            let mut without = with.clone();
            without.remove(at);
            assert_eq!(without, base, "nothing else moved");
        }
    }

    #[test]
    fn pc_streaming_is_still_there_when_there_are_no_systems_and_not_twice() {
        assert_eq!(Item::with_apps(Vec::new(), true), vec![Item::Streaming]);
        assert_eq!(Item::with_apps(vec![Item::Recent(0)], true), vec![Item::Recent(0), Item::Streaming]);
        let once = Item::with_apps(vec![Item::System(0)], true);
        assert_eq!(Item::with_apps(once.clone(), true), once);
        assert!(Item::with_apps(Vec::new(), false).is_empty());
    }

    #[test]
    fn pc_streaming_is_started_not_played_and_cannot_be_starred() {
        let hints = hints(Some(Item::Streaming), true, false, false);
        assert!(hints.contains(&("A", "Start")), "{hints:?}");
        assert!(!hints.iter().any(|(k, _)| *k == "Y"), "{hints:?}");
        assert!(hints.contains(&("START", "Settings")));
    }

    #[test]
    fn curator_rows_come_right_after_favorites_or_the_recent_games() {
        let rows = vec![Item::Curator, Item::SmartShelf(0), Item::Sticker(3)];
        let with = Item::with_curator(Item::with_apps(Item::home([0usize, 1].into_iter(), true, 0..2), true), rows.clone(), false);
        assert_eq!(
            with,
            vec![Item::Recent(0), Item::Recent(1), Item::Favorites, Item::Curator, Item::SmartShelf(0), Item::Sticker(3), Item::Streaming, Item::System(0), Item::System(1)]
        );
        let no_favs = Item::with_curator(Item::home([0usize].into_iter(), false, 0..1), rows.clone(), false);
        assert_eq!(no_favs, vec![Item::Recent(0), Item::Curator, Item::SmartShelf(0), Item::Sticker(3), Item::System(0)]);
        assert_eq!(Item::with_curator(Item::home(std::iter::empty(), false, 0..1), rows.clone(), false)[0], Item::Curator, "first on a new device");
        assert_eq!(Item::with_curator(Item::home([0usize].into_iter(), true, 0..1), Vec::new(), false), Item::home([0usize].into_iter(), true, 0..1), "switched off: nothing");
        let h = hints(Some(Item::SmartShelf(0)), true, true, false);
        assert!(h.contains(&("A", "Open")) && h.contains(&("Y", "Unpin")) && h.contains(&("X", "Options")), "{h:?}");
        let h = hints(Some(Item::Curator), true, false, false);
        assert!(h.contains(&("A", "Open")) && !h.iter().any(|(k, _)| *k == "Y"), "{h:?}");
    }

    #[test]
    fn in_the_list_looks_curator_is_on_top_above_search_and_home_still_opens_on_continue() {
        let rows = vec![Item::Curator, Item::SmartShelf(0), Item::Sticker(3)];
        let home = Item::home([0usize, 1].into_iter(), true, 0..2);
        let with = Item::with_search(Item::with_curator(home.clone(), rows.clone(), true), true);
        assert_eq!(
            with,
            vec![Item::Curator, Item::Search, Item::Recent(0), Item::Recent(1), Item::Favorites, Item::SmartShelf(0), Item::Sticker(3), Item::System(0), Item::System(1)],
            "the recent games, Favorites and the pinned rows keep their places"
        );
        assert_eq!(with[Item::first_row(&with)], Item::Recent(0), "Home opens on Continue: Search one press up, Curator two");
        // No recent games and no Favorites: the pinned rows under the rows on top, then the systems.
        let fresh = Item::with_search(Item::with_curator(Item::home(std::iter::empty(), false, 0..1), rows.clone(), true), true);
        assert_eq!(fresh, vec![Item::Curator, Item::Search, Item::SmartShelf(0), Item::Sticker(3), Item::System(0)]);
        assert_eq!(Item::first_row(&fresh), 2);
        // Lens switched off (History stands on Home instead): nothing on top but Search.
        let off = Item::with_search(Item::with_curator(home.clone(), vec![Item::History], true), true);
        assert_eq!(off[..2], [Item::Search, Item::Recent(0)]);
        assert_eq!(off[4], Item::History, "after Favorites, as before");
        // Only the rows on top: open on the first.
        assert_eq!(Item::first_row(&[Item::Curator, Item::Search]), 0);
    }

    #[test]
    fn hidden_systems_leave_the_shelf_but_keep_their_index() {
        let ids = ["gb", "gba", "ports", "snes"];
        assert_eq!(visible_systems(ids.iter().copied(), &[]), vec![0, 1, 2, 3], "all on by default");
        let hidden = vec!["ports".to_string(), "gb".to_string()];
        assert_eq!(visible_systems(ids.iter().copied(), &hidden), vec![1, 3], "indices still point into the library");
        let items = Item::home([0usize].into_iter(), false, visible_systems(ids.iter().copied(), &hidden));
        assert_eq!(items, vec![Item::Recent(0), Item::System(1), Item::System(3)]);
    }

    #[test]
    fn recents_of_a_hidden_system_are_left_out_favorites_are_not_filtered() {
        let hidden = vec!["ports".to_string()];
        assert!(!recent_shown("ports", &hidden), "the switcher and Home's recent rows skip it");
        assert!(recent_shown("gba", &hidden));
        assert!(recent_shown("ports", &[]));
    }

    #[test]
    fn toggling_hides_and_shows_but_never_the_last_system() {
        let with_games = ["gba", "snes"];
        let mut hidden = Vec::new();
        assert_eq!(toggle_hidden(&mut hidden, "gba", &with_games), Toggle::Hidden);
        assert_eq!(hidden, vec!["gba".to_string()]);
        assert_eq!(toggle_hidden(&mut hidden, "snes", &with_games), Toggle::LastOne, "one system stays on the shelf");
        assert_eq!(hidden, vec!["gba".to_string()]);
        assert_eq!(toggle_hidden(&mut hidden, "gba", &with_games), Toggle::Shown);
        assert!(hidden.is_empty());
        // A hidden system that has no games any more doesn't count as one on the shelf.
        let mut hidden = vec!["psx".to_string()];
        assert_eq!(toggle_hidden(&mut hidden, "gba", &with_games), Toggle::Hidden);
        assert_eq!(toggle_hidden(&mut hidden, "snes", &with_games), Toggle::LastOne);
    }

    #[test]
    fn the_hint_bar_offers_start_for_settings_everywhere() {
        for selected in [None, Some(Item::Recent(0)), Some(Item::System(0)), Some(Item::Favorites), Some(Item::Game(0, 0))] {
            for home in [true, false] {
                let hints = hints(selected, home, false, true);
                assert!(hints.contains(&("START", "Settings")), "{selected:?}, home {home}: {hints:?}");
                assert!(hints.contains(&("SELECT", "Recent games")), "SELECT stays: {hints:?}");
            }
        }
    }

    #[test]
    fn games_and_systems_with_a_console_page_offer_x_and_nothing_else_does() {
        for selected in [Some(Item::Recent(0)), Some(Item::Game(0, 0))] {
            for page in [false, true] {
                let h = hints(selected, true, false, page);
                assert_eq!(h.iter().filter(|h| **h == ("X", "Details")).count(), 1, "{selected:?}: {h:?}");
            }
        }
        assert!(hints(Some(Item::System(0)), true, false, true).contains(&("X", "Details")), "a system with a fact sheet");
        assert!(!hints(Some(Item::System(0)), true, false, false).iter().any(|h| h.0 == "X"), "a system without one (Ports)");
        for selected in [None, Some(Item::Favorites), Some(Item::Streaming)] {
            assert!(!hints(selected, true, false, true).iter().any(|h| h.0 == "X"), "{selected:?}");
        }
    }

    #[test]
    fn start_alone_opens_settings_when_let_go() {
        let mut t = StartTap::default();
        t.pressed(Button::Start, false);
        assert!(!t.released(Button::Select), "another button letting go is not START");
        assert!(t.released(Button::Start));
        assert!(!t.released(Button::Start), "once");
    }

    #[test]
    fn start_with_select_or_another_button_is_a_combo_not_settings() {
        let mut t = StartTap::default();
        // SELECT first, then START: the exit combo.
        t.pressed(Button::Select, false);
        t.pressed(Button::Start, true);
        assert!(!t.released(Button::Start));
        // START first, then SELECT.
        t.pressed(Button::Start, false);
        t.pressed(Button::Select, true);
        assert!(!t.released(Button::Start));
        // START + Vol (brightness).
        t.pressed(Button::Start, false);
        t.disarm();
        assert!(!t.released(Button::Start));
    }
}
