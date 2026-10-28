//! The game search: what the screen shows and what a press on it does. The App opens it (the Search row at the top of
//! Home), starts, stars and opens the games it asks for, saves the recent searches and paints it (`app_search.rs`).
//!
//! Two halves, one in focus at a time:
//! - **Keys**: the gamepad keyboard (`Keyboard::for_search`) under the query and the first results. A types, B deletes
//!   (and closes the search when nothing is typed), START or the keyboard's Done key goes to the results.
//! - **Results**: the keyboard steps aside and the list takes the screen. Up/Down and L1/R1 move, A starts the game,
//!   Y stars it, X opens its card, B (or START) goes back to typing. With nothing typed the list is the recent
//!   searches; A on one searches it again.
//!
//! Every change of the query filters the whole library at once (`search::Index::search`), so the list follows each
//! keystroke. The index is made on a thread after the library loads; until it is there the page says so and lists
//! nothing (`set_index` fills it in).

use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::keyboard::{Keyboard, Outcome};
use crate::pad::Button;
use crate::search::{fold, Hit, Index, Recent};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Focus {
    Keys,
    Results,
}

/// What a press asks the App to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Nothing,
    /// Not possible here (START with nothing to list): the refuse sound.
    Refuse,
    Close,
    Play(Hit),
    Star(Hit),
    Details(Hit),
}

/// What a row of the list is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Row {
    /// A recent search (index into `recent.list`), listed while nothing is typed.
    Recent(usize),
    /// A game that matches (index into `hits`).
    Hit(usize),
}

pub struct SearchPage {
    pub kb: Keyboard,
    pub focus: Focus,
    /// The selected row of the list (Results).
    pub sel: usize,
    /// The games matching the query, best first.
    pub hits: Vec<Hit>,
    /// `hits` are only the closest matches: nothing matched as typed, so a typo in the first letter was forgiven
    /// (`Index::search_closest`). The page says so quietly.
    pub closest: bool,
    pub recent: Recent,
    /// A search was remembered; the App saves `recent` and clears this.
    pub recent_changed: bool,
    /// How long the last filtering took (the log reports it; a keystroke has a frame).
    pub took: Duration,
    index: Option<Arc<Index>>,
    /// Per system of the library: shown on the shelf (hidden systems are left out of the search too).
    shown: Vec<bool>,
    /// The query `hits` were found for.
    query: String,
}

impl SearchPage {
    pub fn new(recent: Recent, index: Option<Arc<Index>>, shown: Vec<bool>) -> SearchPage {
        SearchPage {
            kb: Keyboard::for_search(),
            focus: Focus::Keys,
            sel: 0,
            hits: Vec::new(),
            closest: false,
            recent,
            recent_changed: false,
            took: Duration::ZERO,
            index,
            shown,
            query: String::new(),
        }
    }

    /// The index is ready (or was made again for a changed library): the results follow.
    pub fn set_index(&mut self, index: Arc<Index>) {
        self.index = Some(index);
        self.query.clear();
        self.hits.clear();
        self.closest = false;
        self.refilter();
    }

    /// The index isn't made yet.
    pub fn waiting(&self) -> bool {
        self.index.is_none()
    }

    /// Nothing typed that a name could match: the list shows the recent searches.
    pub fn showing_recent(&self) -> bool {
        fold(&self.kb.text).is_empty()
    }

    /// The rows of the list, top to bottom.
    pub fn rows(&self) -> Vec<Row> {
        if self.showing_recent() {
            (0..self.recent.list.len()).map(Row::Recent).collect()
        } else {
            (0..self.hits.len()).map(Row::Hit).collect()
        }
    }

    fn len(&self) -> usize {
        if self.showing_recent() {
            self.recent.list.len()
        } else {
            self.hits.len()
        }
    }

    /// The game of the selected row, in Results.
    pub fn selected_hit(&self) -> Option<Hit> {
        match self.rows().get(self.sel) {
            Some(Row::Hit(i)) if self.focus == Focus::Results => self.hits.get(*i).copied(),
            _ => None,
        }
    }

    /// Filters the library again when the query changed since the last time.
    fn refilter(&mut self) {
        let query = fold(&self.kb.text);
        if query == self.query {
            return;
        }
        let Some(index) = self.index.clone() else { return };
        let started = Instant::now();
        let shown = &self.shown;
        (self.hits, self.closest) = index.search_closest(&query, &|s| shown.get(s).copied().unwrap_or(true));
        self.took = started.elapsed();
        self.query = query;
        self.sel = 0;
    }

    fn remember(&mut self) {
        let text = self.kb.text.clone();
        if !self.showing_recent() && self.recent.remember(&text) {
            self.recent_changed = true;
        }
    }

    /// To the list, if it has anything.
    fn open_results(&mut self) -> Action {
        if self.len() == 0 {
            return Action::Refuse;
        }
        self.remember();
        self.focus = Focus::Results;
        self.sel = 0;
        Action::Nothing
    }

    pub fn press(&mut self, b: Button, confirm: Button, cancel: Button) -> Action {
        match self.focus {
            Focus::Keys => self.press_keys(b, confirm, cancel),
            Focus::Results => self.press_results(b, confirm, cancel),
        }
    }

    fn press_keys(&mut self, b: Button, confirm: Button, cancel: Button) -> Action {
        match b {
            Button::Start => self.open_results(),
            b if b == cancel => {
                if self.kb.text.pop().is_none() {
                    return Action::Close;
                }
                self.refilter();
                Action::Nothing
            }
            b if b == confirm => match self.kb.press(Button::East) {
                Outcome::Done(_) => self.open_results(),
                _ => {
                    self.refilter();
                    Action::Nothing
                }
            },
            Button::Up | Button::Down | Button::Left | Button::Right => {
                self.kb.press(b);
                Action::Nothing
            }
            _ => Action::Nothing,
        }
    }

    fn press_results(&mut self, b: Button, confirm: Button, cancel: Button) -> Action {
        let last = self.len().saturating_sub(1);
        match b {
            Button::Up => self.sel = self.sel.saturating_sub(1),
            Button::Down => self.sel = (self.sel + 1).min(last),
            Button::L1 => self.sel = self.sel.saturating_sub(5),
            Button::R1 => self.sel = (self.sel + 5).min(last),
            Button::Start => self.focus = Focus::Keys,
            b if b == cancel => self.focus = Focus::Keys,
            b if b == confirm => match self.rows().get(self.sel).copied() {
                // A recent search: typed again, and its games listed.
                Some(Row::Recent(i)) => {
                    self.kb.text = self.recent.list[i].clone();
                    self.refilter();
                    self.remember();
                    self.sel = 0;
                    if self.hits.is_empty() {
                        self.focus = Focus::Keys;
                    }
                }
                Some(Row::Hit(i)) => {
                    self.remember();
                    return Action::Play(self.hits[i]);
                }
                None => {}
            },
            // Y (the left face button) stars, X (the top one) opens the card, as on the shelf.
            Button::West => return self.selected_hit().map_or(Action::Nothing, Action::Star),
            Button::North => {
                let Some(hit) = self.selected_hit() else { return Action::Nothing };
                self.remember();
                return Action::Details(hit);
            }
            _ => {}
        }
        Action::Nothing
    }

    /// A key was tapped: it is typed as if the cursor were on it.
    pub fn tap_key(&mut self, row: usize, col: usize, confirm: Button, cancel: Button) -> Action {
        let rows = self.kb.rows();
        let Some(n) = rows.get(row).map(|r| r.len()) else { return Action::Nothing };
        if col >= n {
            return Action::Nothing;
        }
        self.focus = Focus::Keys;
        self.kb.row = row;
        self.kb.col = col;
        self.press(confirm, confirm, cancel)
    }

    /// A row was tapped: selected, or (when it was already) chosen as A would.
    pub fn tap_row(&mut self, i: usize, confirm: Button, cancel: Button) -> Action {
        if i >= self.len() {
            return Action::Nothing;
        }
        if self.focus == Focus::Results && self.sel == i {
            return self.press(confirm, confirm, cancel);
        }
        self.focus = Focus::Results;
        self.sel = i;
        Action::Nothing
    }

    /// A swipe moved the list by `steps` rows (positive: further down); it stops at the ends.
    pub fn scroll(&mut self, steps: i32) {
        let n = self.len();
        if n == 0 {
            return;
        }
        if self.focus == Focus::Keys {
            self.focus = Focus::Results;
        }
        self.sel = (self.sel as i32 + steps).clamp(0, n as i32 - 1) as usize;
    }

    /// Where the page stands, for the UI sounds and the haptic tick.
    pub fn mark(&self) -> (Focus, usize, usize, usize, usize, usize) {
        (self.focus, self.sel, self.kb.row, self.kb.col, self.kb.text.len(), self.hits.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: Button = Button::East;
    const B: Button = Button::South;

    fn index() -> Arc<Index> {
        let names = ["Mario Kart - Double Dash!!", "Super Mario Kart", "Mario Kart 64", "Metroid Prime", "Kirby's Dream Land", "Zelda II"];
        Arc::new(Index::build(names.iter().enumerate().map(|(g, n)| (g % 3, g, *n))))
    }

    fn page() -> SearchPage {
        SearchPage::new(Recent { list: vec!["zelda".into(), "kirby".into()] }, Some(index()), vec![true; 3])
    }

    /// Types `text` by moving the cursor to each key and pressing A.
    fn type_text(p: &mut SearchPage, text: &str) {
        for ch in text.chars() {
            let rows = p.kb.rows();
            let (r, c) = rows
                .iter()
                .enumerate()
                .find_map(|(r, row)| row.iter().position(|k| *k == crate::keyboard::Key::Char(ch)).map(|c| (r, c)))
                .or_else(|| (ch == ' ').then_some((4, 0)))
                .unwrap();
            // Walk there with the D-pad, as a player would.
            while p.kb.row != r {
                p.press(if p.kb.row < r { Button::Down } else { Button::Up }, A, B);
            }
            while p.kb.col != c {
                p.press(if p.kb.col < c { Button::Right } else { Button::Left }, A, B);
            }
            assert_eq!(p.press(A, A, B), Action::Nothing);
        }
    }

    #[test]
    fn it_opens_on_the_keyboard_with_the_recent_searches() {
        let p = page();
        assert_eq!(p.focus, Focus::Keys);
        assert!(p.showing_recent());
        assert_eq!(p.rows(), vec![Row::Recent(0), Row::Recent(1)]);
        assert_eq!(p.selected_hit(), None);
    }

    #[test]
    fn every_keystroke_filters_the_list() {
        let mut p = page();
        type_text(&mut p, "m");
        assert_eq!(p.hits.len(), 4, "Mario Kart three times and Metroid");
        type_text(&mut p, "ario k");
        assert_eq!(p.hits.len(), 3);
        assert_eq!(p.rows().len(), 3);
        p.press(B, A, B);
        p.press(B, A, B);
        assert_eq!(p.kb.text, "mario");
        assert_eq!(p.hits.len(), 3, "deleting filters again");
        type_text(&mut p, "zzz");
        assert!(p.hits.is_empty());
        assert_eq!(p.press(Button::Start, A, B), Action::Refuse, "no results: START stays on the keyboard");
        assert_eq!(p.focus, Focus::Keys);
    }

    #[test]
    fn a_typo_in_the_first_letter_lists_the_closest_games_as_the_last_resort() {
        let mut p = page();
        type_text(&mut p, "metroid");
        assert!(!p.closest && p.hits.len() == 1);
        for _ in 0.."metroid".len() {
            p.press(B, A, B);
        }
        // "netroid": nothing matches as typed, so the first letter may be wrong.
        type_text(&mut p, "netroid");
        assert_eq!(p.hits.len(), 1, "Metroid Prime");
        assert!(p.closest);
        assert_eq!(p.press(Button::Start, A, B), Action::Nothing, "the closest games can be opened like any result");
        assert_eq!(p.press(A, A, B), Action::Play(p.hits[0]));
        // Typing on until nothing is near: no hits, and not "closest" either.
        p.press(B, A, B);
        type_text(&mut p, "zzzzz");
        assert!(p.hits.is_empty() && !p.closest);
    }

    #[test]
    fn b_deletes_and_closes_when_nothing_is_typed() {
        let mut p = page();
        type_text(&mut p, "ki");
        assert_eq!(p.press(B, A, B), Action::Nothing);
        assert_eq!(p.press(B, A, B), Action::Nothing);
        assert_eq!(p.press(B, A, B), Action::Close);
    }

    #[test]
    fn start_and_done_go_to_the_results_where_a_y_and_x_act_on_the_game() {
        let mut p = page();
        type_text(&mut p, "mario kart");
        assert_eq!(p.press(Button::Start, A, B), Action::Nothing);
        assert_eq!(p.focus, Focus::Results);
        assert_eq!(p.recent.list[0], "mario kart", "remembered");
        assert!(p.recent_changed);
        let first = p.hits[0];
        assert_eq!(p.selected_hit(), Some(first));
        p.press(Button::Down, A, B);
        let second = p.hits[1];
        assert_eq!(p.press(Button::West, A, B), Action::Star(second));
        assert_eq!(p.press(Button::North, A, B), Action::Details(second));
        assert_eq!(p.press(A, A, B), Action::Play(second));
        p.press(Button::R1, A, B);
        assert_eq!(p.sel, 2, "R1 stops at the end");
        p.press(Button::L1, A, B);
        assert_eq!(p.sel, 0);
        p.press(Button::Up, A, B);
        assert_eq!(p.sel, 0, "Up stops at the top");
        // B goes back to typing, the text stays.
        p.press(B, A, B);
        assert_eq!((p.focus, p.kb.text.as_str()), (Focus::Keys, "mario kart"));
        assert_eq!(p.selected_hit(), None);
        // The keyboard's Done key goes to the results as START does.
        p.kb.row = 4;
        p.kb.col = 2;
        p.press(A, A, B);
        assert_eq!(p.focus, Focus::Results);
        assert_eq!(p.press(Button::Start, A, B), Action::Nothing);
        assert_eq!(p.focus, Focus::Keys, "START goes back too");
    }

    #[test]
    fn a_recent_search_is_searched_again() {
        let mut p = page();
        assert_eq!(p.press(Button::Start, A, B), Action::Nothing);
        assert_eq!(p.focus, Focus::Results);
        p.press(Button::Down, A, B);
        assert_eq!(p.press(Button::West, A, B), Action::Nothing, "Y does nothing on a recent search");
        assert_eq!(p.press(A, A, B), Action::Nothing);
        assert_eq!(p.kb.text, "kirby");
        assert_eq!(p.hits.len(), 1);
        assert_eq!((p.focus, p.sel), (Focus::Results, 0));
        assert_eq!(p.recent.list, vec!["kirby", "zelda"], "moved up");
        assert_eq!(p.press(A, A, B), Action::Play(p.hits[0]));
        // A recent search that finds nothing any more goes back to the keyboard.
        let mut p = SearchPage::new(Recent { list: vec!["gone".into()] }, Some(index()), vec![true; 3]);
        p.press(Button::Start, A, B);
        p.press(A, A, B);
        assert_eq!((p.focus, p.kb.text.as_str()), (Focus::Keys, "gone"));
        // Nothing typed and nothing searched before: nothing to go to.
        let mut p = SearchPage::new(Recent::default(), Some(index()), vec![true; 3]);
        assert_eq!(p.press(Button::Start, A, B), Action::Refuse);
    }

    #[test]
    fn hidden_systems_are_not_searched() {
        let mut p = SearchPage::new(Recent::default(), Some(index()), vec![true, false, true]);
        type_text(&mut p, "mario kart");
        assert_eq!(p.hits.len(), 2, "Super Mario Kart is on a hidden system");
        assert!(p.hits.iter().all(|h| h.system != 1));
    }

    #[test]
    fn until_the_index_is_ready_the_page_waits_and_then_fills_in() {
        let mut p = SearchPage::new(Recent::default(), None, vec![true; 3]);
        assert!(p.waiting());
        type_text(&mut p, "kirby");
        assert!(p.hits.is_empty());
        p.set_index(index());
        assert!(!p.waiting());
        assert_eq!(p.hits.len(), 1, "what was typed is searched once the index is there");
    }

    #[test]
    fn touch_types_selects_and_plays() {
        let mut p = page();
        // "k" is on the fourth row (asdfghjkl), eighth key.
        assert_eq!(p.tap_key(2, 7, A, B), Action::Nothing);
        assert_eq!(p.kb.text, "k");
        assert_eq!(p.tap_key(9, 0, A, B), Action::Nothing, "outside the keyboard");
        assert_eq!(p.tap_key(2, 20, A, B), Action::Nothing);
        assert_eq!(p.kb.text, "k");
        assert_eq!(p.tap_row(0, A, B), Action::Nothing, "the first tap selects");
        assert_eq!(p.focus, Focus::Results);
        let hit = p.hits[0];
        assert_eq!(p.tap_row(0, A, B), Action::Play(hit), "the second starts");
        assert_eq!(p.tap_row(40, A, B), Action::Nothing);
        p.scroll(3);
        assert_eq!(p.sel, p.hits.len() - 1, "a swipe stops at the end");
        p.scroll(-10);
        assert_eq!(p.sel, 0);
        // Tapping a key while in the results goes back to typing.
        p.tap_key(2, 0, A, B);
        assert_eq!((p.focus, p.kb.text.as_str()), (Focus::Keys, "ka"));
    }
}
