//! Curator's pages (docs/CURATOR.md "Screens"): what a press on them does. The App opens them, keeps them, acts on
//! what they ask for and paints them (`app_curator.rs`); nothing here touches files or the screen, so every path is
//! a plain test.

use crate::curator::echoes::Echo;
use crate::curator::sort::Order;
use crate::curator::{Facet, Index, Lens};
use crate::pad::Button;

const CONFIRM: Button = Button::East;
const CANCEL: Button = Button::South;
/// L1/R1 move this many rows.
const PAGE: usize = 6;

/// Which column of the Lens has the selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Col {
    Facets,
    Values,
}

/// What a press on the Lens asks for.
#[derive(Debug, PartialEq, Eq)]
pub enum LensOut {
    None,
    Back,
    /// START: the games.
    Show,
    /// X: save as a smart shelf.
    Save,
    /// SELECT: the order of the games (a sheet of [`Order::ALL`]).
    Order,
}

/// The Lens: facets on the left, the selected facet's values on the right with their counts.
#[derive(Clone, Debug, Default)]
pub struct LensPage {
    pub lens: Lens,
    pub col: Option<Col>,
    /// Index into the shown facets, and into the selected facet's values.
    pub facet: usize,
    pub value: usize,
    /// Editing this smart shelf (X saves over it).
    pub shelf: Option<usize>,
    /// The order START shows the games in, and X saves with a shelf.
    pub order: Order,
}

fn step(at: usize, b: Button, len: usize) -> usize {
    let last = len.saturating_sub(1);
    match b {
        Button::Up => at.saturating_sub(1),
        Button::Down => (at + 1).min(last),
        Button::L1 => at.saturating_sub(PAGE),
        Button::R1 => (at + PAGE).min(last),
        _ => at,
    }
}

impl LensPage {
    pub fn new(lens: Lens, shelf: Option<usize>) -> LensPage {
        LensPage { lens, col: Some(Col::Facets), facet: 0, value: 0, shelf, order: Order::Name }
    }

    pub fn column(&self) -> Col {
        self.col.unwrap_or(Col::Facets)
    }

    /// The facet the selection is on, among those `index` shows.
    pub fn selected_facet(&self, index: &Index) -> Option<Facet> {
        index.shown_facets().get(self.facet).copied()
    }

    /// Keeps the selection inside what the index offers (it may have been rebuilt with fewer facets or values).
    pub fn clamp(&mut self, index: &Index) {
        let facets = index.shown_facets();
        self.facet = self.facet.min(facets.len().saturating_sub(1));
        let n = facets.get(self.facet).map_or(0, |f| index.facet(*f).values.len());
        self.value = self.value.min(n.saturating_sub(1));
    }

    pub fn press(&mut self, b: Button, index: &Index) -> LensOut {
        self.clamp(index);
        let facets = index.shown_facets();
        let values = facets.get(self.facet).map_or(0, |f| index.facet(*f).values.len());
        match (b, self.column()) {
            (b, _) if b == CANCEL => return LensOut::Back,
            (Button::Start, _) => return LensOut::Show,
            (Button::North, _) => return LensOut::Save,
            (Button::Select, _) => return LensOut::Order,
            (Button::West, _) => {
                if let Some(f) = facets.get(self.facet) {
                    self.lens.clear(*f);
                }
            }
            (Button::Up | Button::Down | Button::L1 | Button::R1, Col::Facets) => {
                let to = step(self.facet, b, facets.len());
                if to != self.facet {
                    self.facet = to;
                    self.value = 0;
                }
            }
            (Button::Up | Button::Down | Button::L1 | Button::R1, Col::Values) => self.value = step(self.value, b, values),
            (Button::Right, Col::Facets) | (CONFIRM, Col::Facets) if values > 0 => self.col = Some(Col::Values),
            (Button::Left, Col::Values) => self.col = Some(Col::Facets),
            (CONFIRM, Col::Values) => {
                if let Some(f) = facets.get(self.facet) {
                    if let Some(v) = index.facet(*f).values.get(self.value) {
                        let key = v.key.clone();
                        self.lens.toggle(*f, &key);
                    }
                }
            }
            _ => {}
        }
        LensOut::None
    }

    /// Where the page stands, for the UI sounds and haptics.
    pub fn mark(&self) -> (usize, usize, Col, usize) {
        (self.facet, self.value, self.column(), self.lens.picks.values().map(Vec::len).sum())
    }

    pub fn hints(&self) -> Vec<(&'static str, &'static str)> {
        let a = if self.column() == Col::Values { "Pick" } else { "Open" };
        vec![("A", a), ("START", "Show"), ("X", "Save"), ("Y", "Clear"), ("SELECT", "Order"), ("B", "Back")]
    }
}

/// What a press on a list of games (Echoes, Versions) asks for.
#[derive(Debug, PartialEq, Eq)]
pub enum ListOut {
    None,
    Back,
    /// A: start this entry's game.
    Play(usize),
    /// X on Echoes: the entry's card.
    Details(usize),
    /// Y on Versions: this entry is the preferred version.
    Prefer(usize),
}

/// The full Echoes of a game: up to 12 with covers and reasons.
#[derive(Clone, Debug, Default)]
pub struct EchoesPage {
    /// The seed, as the catalog numbers it, and its name.
    pub seed: usize,
    pub seed_name: String,
    pub echoes: Vec<Echo>,
    pub sel: usize,
    /// "Not on this device" (Settings → Curator): names only, from gamedb.
    pub elsewhere: Vec<String>,
    /// "Why these?": the games of your taste profile that changed the order (`Index::why`), if any.
    pub why: Option<String>,
}

impl EchoesPage {
    pub fn press(&mut self, b: Button) -> ListOut {
        match b {
            b if b == CANCEL => ListOut::Back,
            b if b == CONFIRM && !self.echoes.is_empty() => ListOut::Play(self.sel),
            Button::North if !self.echoes.is_empty() => ListOut::Details(self.sel),
            Button::Up | Button::Down | Button::L1 | Button::R1 => {
                self.sel = step(self.sel, b, self.echoes.len());
                ListOut::None
            }
            _ => ListOut::None,
        }
    }

    pub fn hints(&self) -> Vec<(&'static str, &'static str)> {
        if self.echoes.is_empty() {
            return vec![("B", "Back")];
        }
        vec![("A", "Play"), ("X", "Details"), ("B", "Back")]
    }
}

/// One version of a game on the Versions page.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct VersionRow {
    /// The catalog's number of the version.
    pub game: usize,
    /// System id, for the stripe.
    pub system: String,
    /// "Super Nintendo".
    pub system_name: String,
    /// The file's name as the shelf shows it, when it differs from the game's.
    pub title: String,
    /// "USA · English", "Japan · English (fan translation)".
    pub release: String,
    /// "Snes9x on pocket-core".
    pub runs: String,
    /// "2 save states · last played 3 days ago · 4 h 10 min played".
    pub saves: String,
    pub preferred: bool,
    /// Not a version: a remake of the game, or the game this one remakes ("Related: remake"). A plays it; it can't
    /// be the preferred version.
    pub related: bool,
}

/// One page per game across systems.
#[derive(Clone, Debug, Default)]
pub struct VersionsPage {
    pub title: String,
    pub rows: Vec<VersionRow>,
    pub sel: usize,
}

impl VersionsPage {
    pub fn press(&mut self, b: Button) -> ListOut {
        match b {
            b if b == CANCEL => ListOut::Back,
            b if b == CONFIRM && !self.rows.is_empty() => ListOut::Play(self.sel),
            Button::West if self.rows.get(self.sel).is_some_and(|r| !r.related) => ListOut::Prefer(self.sel),
            Button::Up | Button::Down | Button::L1 | Button::R1 => {
                self.sel = step(self.sel, b, self.rows.len());
                ListOut::None
            }
            _ => ListOut::None,
        }
    }

    /// Marks the preferred version (after the App saved it).
    pub fn prefer(&mut self, i: usize) {
        for (k, r) in self.rows.iter_mut().enumerate() {
            r.preferred = k == i;
        }
    }

    pub fn hints(&self) -> Vec<(&'static str, &'static str)> {
        match self.rows.get(self.sel) {
            Some(r) if r.related => vec![("A", "Play"), ("B", "Back")],
            Some(r) if r.preferred => vec![("A", "Play"), ("Y", "Preferred"), ("B", "Back")],
            _ => vec![("A", "Play"), ("Y", "Prefer"), ("B", "Back")],
        }
    }
}

/// What a press on a picker sheet asks for.
#[derive(Debug, PartialEq, Eq)]
pub enum SheetOut {
    None,
    Back,
    /// A on entry `i`.
    Pick(usize),
    /// Left (-1) or Right (+1) on the sheet's adjustable row ([`Sheet::adjust`]): the Game of the year sticker's year.
    Adjust(usize, i32),
}

/// A short list over the dimmed page (the stickers of a game, a shelf's options), as the status picker is.
#[derive(Clone, Debug, Default)]
pub struct Sheet {
    pub title: String,
    /// Label, and a tick (None: no tick box, an action such as "New sticker…").
    pub rows: Vec<(String, Option<bool>)>,
    pub sel: usize,
    /// One choice of several (the order of a list): the tick marks the one chosen, A chooses another.
    pub single: bool,
    /// A row whose value Left and Right change (its label says the value), while Up and Down still move.
    pub adjust: Option<usize>,
}

impl Sheet {
    /// A sheet of one choice among `labels`, the `chosen` one ticked and selected.
    pub fn choice(title: &str, labels: &[&str], chosen: usize) -> Sheet {
        let rows = labels.iter().enumerate().map(|(i, l)| (l.to_string(), Some(i == chosen))).collect();
        Sheet { title: title.to_string(), rows, sel: chosen.min(labels.len().saturating_sub(1)), single: true, adjust: None }
    }

    pub fn press(&mut self, b: Button) -> SheetOut {
        match b {
            b if b == CANCEL => SheetOut::Back,
            b if b == CONFIRM && !self.rows.is_empty() => SheetOut::Pick(self.sel),
            Button::Left | Button::Right if self.adjust == Some(self.sel) => SheetOut::Adjust(self.sel, if b == Button::Left { -1 } else { 1 }),
            Button::Up | Button::Left => {
                self.sel = self.sel.saturating_sub(1);
                SheetOut::None
            }
            Button::Down | Button::Right => {
                self.sel = (self.sel + 1).min(self.rows.len().saturating_sub(1));
                SheetOut::None
            }
            _ => SheetOut::None,
        }
    }

    pub fn hints(&self) -> Vec<(&'static str, &'static str)> {
        let a = match self.rows.get(self.sel) {
            _ if self.single => "Choose",
            Some((_, Some(true))) => "Take off",
            Some((_, Some(false))) => "Put on",
            _ => "Choose",
        };
        if self.adjust == Some(self.sel) {
            return vec![("A", a), ("< >", "Year"), ("B", "Done")];
        }
        vec![("A", a), ("B", "Done")]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::curator::features::{features_for, GameRef};
    use crate::curator::{Catalog, SystemName, UserState};
    use crate::gamedb::GameInfo;
    use std::path::Path;
    use std::sync::Arc;

    fn index() -> Index {
        let games: Vec<_> = [("snes", "racing video game", 1992), ("gba", "racing video game", 2001), ("gba", "role-playing video game", 2003)]
            .iter()
            .enumerate()
            .map(|(i, (sys, genre, year))| {
                let info = GameInfo { qid: format!("Q{i}"), label: format!("G{i}"), year: Some(*year), genres: vec![genre.to_string()], ..Default::default() };
                let path = format!("/roms/{sys}/G{i} (USA).bin");
                features_for(GameRef { at: (0, i), system: sys, path: Path::new(&path), name: &format!("G{i}") }, Some(&info), &[])
            })
            .collect();
        let systems = vec![
            SystemName { id: "gba".into(), name: "Game Boy Advance".into(), short: "GBA".into() },
            SystemName { id: "snes".into(), name: "Super Nintendo".into(), short: "SNES".into() },
        ];
        let cat = Arc::new(Catalog::build(games, systems));
        Index::build(cat, UserState::default())
    }

    #[test]
    fn the_lens_moves_between_its_columns_and_picks_with_a() {
        let ix = index();
        let mut p = LensPage::new(Lens::default(), None);
        assert_eq!(p.selected_facet(&ix), Some(Facet::System));
        assert_eq!(p.hints()[0], ("A", "Open"));
        p.press(Button::Right, &ix);
        assert_eq!(p.column(), Col::Values);
        assert_eq!(p.hints()[0], ("A", "Pick"));
        // The System facet's values follow the shelf's order: GBA, then SNES.
        p.press(CONFIRM, &ix);
        assert!(p.lens.picked(Facet::System, "gba"));
        assert_eq!(ix.matches(&p.lens).count(), 2);
        p.press(Button::Down, &ix);
        p.press(CONFIRM, &ix);
        assert_eq!(ix.matches(&p.lens).count(), 3, "GBA or SNES");
        p.press(CONFIRM, &ix);
        assert!(!p.lens.picked(Facet::System, "snes"), "a second A drops it");
        // Left back to the facets; down to Family; Y clears only that facet.
        p.press(Button::Left, &ix);
        p.press(Button::Down, &ix);
        assert_eq!(p.selected_facet(&ix), Some(Facet::Family));
        assert_eq!(p.value, 0, "a new facet starts at its first value");
        p.press(CONFIRM, &ix);
        p.press(CONFIRM, &ix);
        assert_eq!(ix.matches(&p.lens).count(), 1, "GBA and Racing");
        p.press(Button::West, &ix);
        assert!(p.lens.keys(Facet::Family).is_empty() && p.lens.picked(Facet::System, "gba"));
        assert_eq!(p.press(Button::Start, &ix), LensOut::Show);
        assert_eq!(p.press(Button::North, &ix), LensOut::Save);
        assert_eq!(p.press(CANCEL, &ix), LensOut::Back);
    }

    #[test]
    fn the_lens_keeps_its_selection_inside_a_smaller_index() {
        let ix = index();
        let mut p = LensPage::new(Lens::default(), None);
        p.facet = 40;
        p.value = 40;
        p.clamp(&ix);
        assert!(p.facet < ix.shown_facets().len());
        let f = p.selected_facet(&ix).unwrap();
        assert!(p.value < ix.facet(f).values.len());
        for _ in 0..30 {
            p.press(Button::R1, &ix);
        }
        assert_eq!(p.facet, ix.shown_facets().len() - 1, "L1/R1 stop at the ends");
    }

    #[test]
    fn lists_and_sheets() {
        let mut e = EchoesPage { echoes: vec![Echo { game: 4, score: 1.0, reason: "Same series".into(), lift: 0.0 }, Echo { game: 7, score: 0.5, reason: String::new(), lift: 0.0 }], ..Default::default() };
        assert_eq!(e.press(Button::Down), ListOut::None);
        assert_eq!(e.press(CONFIRM), ListOut::Play(1));
        assert_eq!(e.press(Button::North), ListOut::Details(1));
        assert_eq!(e.press(Button::R1), ListOut::None);
        assert_eq!(e.sel, 1);
        assert_eq!(e.press(CANCEL), ListOut::Back);
        assert_eq!(EchoesPage::default().press(CONFIRM), ListOut::None, "nothing to play on an empty list");
        let mut v = VersionsPage { rows: vec![VersionRow { preferred: true, ..Default::default() }, VersionRow::default()], ..Default::default() };
        assert_eq!(v.hints()[1], ("Y", "Preferred"));
        v.press(Button::Down);
        assert_eq!(v.hints()[1], ("Y", "Prefer"));
        assert_eq!(v.press(Button::West), ListOut::Prefer(1));
        v.prefer(1);
        assert!(!v.rows[0].preferred && v.rows[1].preferred);
        v.rows.push(VersionRow { related: true, ..Default::default() });
        v.press(Button::Down);
        assert_eq!(v.press(Button::West), ListOut::None, "a remake is not a version to prefer");
        assert_eq!(v.hints(), [("A", "Play"), ("B", "Back")]);
        let mut s = Sheet { title: "Stickers".into(), rows: vec![("Couch co-op".into(), Some(true)), ("New sticker…".into(), None)], sel: 0, single: false, adjust: None };
        assert_eq!(s.hints()[0], ("A", "Take off"));
        s.press(Button::Down);
        s.press(Button::Down);
        assert_eq!(s.press(CONFIRM), SheetOut::Pick(1));
        assert_eq!(s.hints()[0], ("A", "Choose"));
        assert_eq!(s.press(CANCEL), SheetOut::Back);
        // The Game of the year sticker's row: Left and Right change its year, Up and Down still move.
        let mut g = Sheet { rows: vec![("Completed".into(), Some(false)), ("Game of the year 1998".into(), Some(true))], adjust: Some(1), ..Default::default() };
        assert_eq!(g.press(Button::Right), SheetOut::None, "elsewhere Right moves");
        assert_eq!(g.sel, 1);
        assert_eq!(g.hints()[1], ("< >", "Year"));
        assert_eq!(g.press(Button::Left), SheetOut::Adjust(1, -1));
        assert_eq!(g.press(Button::Right), SheetOut::Adjust(1, 1));
        assert_eq!(g.press(Button::Up), SheetOut::None);
        assert_eq!(g.sel, 0);
        // The order of a list: one choice, the current one ticked and selected.
        let labels: Vec<&str> = Order::ALL.iter().map(|o| o.label()).collect();
        let mut o = Sheet::choice("Order", &labels, 1);
        assert_eq!((o.sel, o.rows[1].1, o.rows[0].1), (1, Some(true), Some(false)));
        assert_eq!(o.hints()[0], ("A", "Choose"));
        o.press(Button::Down);
        assert_eq!(o.press(CONFIRM), SheetOut::Pick(2));
    }

    #[test]
    fn select_on_the_lens_asks_for_the_order_and_the_hints_say_so() {
        let ix = index();
        let mut p = LensPage::new(Lens::default(), None);
        assert_eq!(p.order, Order::Name, "by name until the player picks another");
        assert_eq!(p.press(Button::Select, &ix), LensOut::Order);
        assert!(p.hints().contains(&("SELECT", "Order")));
    }
}
