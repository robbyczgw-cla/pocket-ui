//! Credits & links (X on a console page): who designed and made the console, its chips, its family, its best-selling
//! game, and the links that lead on from there inside the player's own library: the consoles before and after it,
//! the consoles with the same chip, and the games by the console's makers. From `console_credits`
//! (assets/data/console-credits.json); the App resolves the links against the library before it opens the page
//! (`app_console`), so painting and pressing do no lookups.
//!
//! D-pad: up and down move between the links that are on screen and scroll the page past the rest; A opens the
//! selected link, B (or X) goes back to the console page. Drawn like the console page: the console's stripe down the
//! left edge, Title 37 headings, label/value rows in Dust and Chalk, and the links as spines that turn to paper when
//! selected, as the game card's "More by" rows.

use crate::console_credits::{Credits, Relation};
use crate::console_page::{heading, paint_hints, row, scrolled, BODY_TOP, HEAD_BASE, STEP, TITLE_BASE};
use crate::gfx::{mix, Canvas};
use crate::pad::Button;
use crate::palette::*;
use crate::text::{Face, Text};
use crate::touch::Area;

/// Where a link leads; indices into the App's systems and their games.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Link {
    /// The console page of a system in the library.
    Console(usize),
    /// A system in the library without a console page: its games.
    Games(usize),
    /// A game: its card.
    Game(usize, usize),
}

/// One link as a spine: the name, a quiet note at its right end, the colour of its system.
#[derive(Clone, Debug, PartialEq)]
pub struct LinkRow {
    pub name: String,
    pub note: String,
    pub stripe: u32,
    pub link: Link,
}

/// What the page shows, with the links already found in the library.
#[derive(Clone, Debug)]
pub struct CreditsData {
    pub credits: &'static Credits,
    pub console_name: String,
    pub stripe: u32,
    /// Consoles of the library with one of this console's chips ("Game Boy Color · Sharp SM83").
    pub same_chip: Vec<LinkRow>,
    /// Relatives that are systems of the library.
    pub family_links: Vec<LinkRow>,
    /// The other relatives, by relation, in the file's order.
    pub family_text: Vec<(Relation, Vec<String>)>,
    /// Games of the library by the console's makers, oldest first, at most `GAMES_MAX`.
    pub games: Vec<LinkRow>,
    /// How many more there are.
    pub games_more: usize,
    /// False while Curator's catalog (where the games' developers come from) is still being built.
    pub games_ready: bool,
}

/// Where the player is on the page: scroll, selected link, whether the games are there.
pub type Mark = (i32, Option<usize>, bool);

/// Games listed under "By the makers".
pub const GAMES_MAX: usize = 8;

const LINK_H: i32 = 64;
/// The label column of the rows: a name ("Shigeru Miyamoto") or a role ("Manufacturer").
const KEY_W: i32 = 300;
const LINK_GAP: i32 = 8;
const BOTTOM_GAP: i32 = 130;

impl CreditsData {
    /// The links in the order they are painted (and selected): same chip, family, games.
    pub fn links(&self) -> Vec<&LinkRow> {
        self.same_chip.iter().chain(&self.family_links).chain(&self.games).collect()
    }

    /// The relatives that aren't links, grouped by relation in the order Before, After, Models, Related.
    pub fn group_text(names: &[(Relation, String)]) -> Vec<(Relation, Vec<String>)> {
        let mut out: Vec<(Relation, Vec<String>)> = Vec::new();
        for rel in [Relation::Before, Relation::After, Relation::Model, Relation::Family] {
            let n: Vec<String> = names.iter().filter(|(r, _)| *r == rel).map(|(_, n)| n.clone()).collect();
            if !n.is_empty() {
                out.push((rel, n));
            }
        }
        out
    }
}

/// What a press asks the console page (and the App) to do.
#[derive(Debug, PartialEq)]
pub enum Out {
    None,
    /// B or X: back to the console page.
    Close,
    Open(Link),
}

pub struct CreditsPage {
    pub data: CreditsData,
    pub scroll: i32,
    max_scroll: i32,
    view_h: i32,
    /// The selected link (index into `data.links()`); always one on screen, or none when no link is.
    pub sel: Option<usize>,
    /// Each link's top and bottom in the page, as last painted.
    spans: Vec<(i32, i32)>,
    painted: bool,
}

impl CreditsPage {
    pub fn new(data: CreditsData) -> CreditsPage {
        CreditsPage {
            data,
            scroll: 0,
            max_scroll: 0,
            view_h: 0,
            sel: None,
            spans: Vec::new(),
            painted: false,
        }
    }

    /// The games by the makers, once they are known; the selection and the scroll stay (the games are the last links).
    pub fn set_games(&mut self, games: Vec<LinkRow>, more: usize) {
        self.data.games = games;
        self.data.games_more = more;
        self.data.games_ready = true;
    }

    /// Where the player is (for the UI sounds).
    pub fn mark(&self) -> Mark {
        (self.scroll, self.sel, self.data.games_ready)
    }

    fn visible(&self, k: usize) -> bool {
        self.spans
            .get(k)
            .is_some_and(|&(top, bottom)| bottom > self.scroll && top < self.scroll + self.view_h)
    }

    /// Scrolls just enough for link `k` to be whole on screen and clear of the fades at the edges.
    fn reveal(&mut self, k: usize) {
        const CLEAR: i32 = 44;
        if let Some(&(top, bottom)) = self.spans.get(k) {
            if top - CLEAR < self.scroll {
                self.scroll = (top - CLEAR).max(0);
            } else if bottom + CLEAR > self.scroll + self.view_h {
                self.scroll = (bottom + CLEAR - self.view_h).min(self.max_scroll);
            }
        }
    }

    /// Keeps the selection on screen: the selected link if it still is, else the nearest one that is.
    fn settle(&mut self, from_below: bool) {
        if self.sel.is_some_and(|k| self.visible(k)) {
            return;
        }
        let shown: Vec<usize> = (0..self.spans.len()).filter(|&k| self.visible(k)).collect();
        self.sel = if from_below { shown.last().copied() } else { shown.first().copied() };
    }

    pub fn press(&mut self, b: Button) -> Out {
        match b {
            Button::South | Button::North => return Out::Close,
            Button::East => {
                return match self.sel.and_then(|k| self.data.links().get(k).map(|l| l.link)) {
                    Some(link) => Out::Open(link),
                    None => Out::None,
                };
            }
            Button::Down => {
                let next = self.sel.map_or(0, |k| k + 1);
                if self.sel.is_some() && self.visible(next) {
                    self.sel = Some(next);
                    self.reveal(next);
                } else {
                    self.scroll = (self.scroll + STEP).min(self.max_scroll);
                    self.settle(false);
                }
            }
            Button::Up => match self.sel {
                Some(k) if k > 0 && self.visible(k - 1) => {
                    self.sel = Some(k - 1);
                    self.reveal(k - 1);
                }
                _ => {
                    self.scroll = (self.scroll - STEP).max(0);
                    self.settle(true);
                }
            },
            Button::L1 | Button::R1 => {
                let step = if b == Button::L1 { -4 * STEP } else { 4 * STEP };
                self.scroll = (self.scroll + step).clamp(0, self.max_scroll);
                self.settle(b == Button::L1);
            }
            _ => {}
        }
        Out::None
    }

    pub fn hints(&self) -> Vec<(&'static str, &'static str)> {
        let mut h = vec![("B", "Back")];
        if self.sel.is_some() {
            h.push(("A", "Open"));
        }
        if self.max_scroll > 0 {
            h.push(("L1 R1", "Scroll"));
        }
        h
    }

    /// Paints the page; `corner` is the status corner. Returns the hint keys and, for touch, the visible links as
    /// (area, link index), the list position and its length (one step per press of up/down).
    pub fn paint(&mut self, c: &mut Canvas, text: &mut Text, corner: Option<&crate::status_corner::Corner>, bg: u32) -> Paint {
        c.clear(bg);
        c.rect(0, 0, STRIPE, c.h, self.data.stripe);
        let full_w = c.w - 2 * MARGIN;
        let left = corner.map_or(c.w - MARGIN, |k| k.paint(c, text, c.w - MARGIN, 80, pal().dust));
        let corner_w = c.w - MARGIN - crate::status_corner::room_left_of(left, c.w - MARGIN);
        let title = text.fit(Face::Title, 67, "Credits & links", full_w - corner_w);
        text.draw(c, Face::Title, 67, MARGIN, TITLE_BASE, pal().text, &title);
        let sub = text.fit(Face::Body, 28, &self.data.console_name, full_w);
        text.draw(c, Face::Body, 28, MARGIN, HEAD_BASE, pal().quiet, &sub);
        let bottom = c.h - BOTTOM_GAP;
        let mut view = 0;
        // Twice only on the first paint, when it selects a link (the links' places are known after a paint).
        for _ in 0..2 {
            let mut spans = Vec::new();
            let sel = self.sel;
            let data = &self.data;
            let (height, v) = scrolled(c, BODY_TOP, bottom, self.scroll, bg, |body, y0| {
                paint_body(body, text, data, sel, y0, &mut spans)
            });
            view = v;
            self.view_h = view;
            self.max_scroll = (height - view).max(0);
            self.scroll = self.scroll.min(self.max_scroll);
            self.spans = spans;
            if self.painted {
                break;
            }
            // The first link on the first screen is selected from the start, so A works at once.
            self.painted = true;
            self.settle(false);
            if self.sel == sel {
                break;
            }
        }
        // The credit line: where the facts come from.
        let credit = text.fit(Face::Body, 21, "Facts: Wikidata (CC0), Wikipedia · summaries written for pocket-ui", full_w);
        text.draw_right(c, Face::Body, 21, c.w - MARGIN, c.h - 104, pal().dust, &credit);
        if self.max_scroll > 0 {
            let total = view + self.max_scroll;
            let bar_h = (view * view / total).max(40);
            let bar_y = BODY_TOP + (view - bar_h) * self.scroll / self.max_scroll.max(1);
            c.rect(c.w - 28, BODY_TOP, 4, view, pal().spine);
            c.rect(c.w - 28, bar_y, 4, bar_h, pal().dust);
        }
        let mut out = Paint {
            buttons: paint_hints(c, text, &self.hints()),
            ..Paint::default()
        };
        // Touch: the links on screen as rows of a list whose position counts the scroll steps first, so a swipe can
        // scroll past the last link and a tap on a link moves the selection there (and opens it when it is there).
        let base = (self.scroll / STEP) as usize;
        let links = self.spans.len();
        out.list = (base + self.sel.unwrap_or(0), (self.max_scroll / STEP) as usize + links + 2, STEP as f32);
        // Rows only with a selection: without one there is no link on screen to tap.
        if self.sel.is_some() {
            for (k, &(top, bot)) in self.spans.iter().enumerate() {
                let (y0, y1) = ((top - self.scroll).max(0), (bot - self.scroll).min(view));
                if y1 > y0 {
                    out.rows.push((Area::new(MARGIN, BODY_TOP + y0, full_w, y1 - y0), base + k));
                }
            }
        }
        out
    }
}

/// What a paint offers to touch.
#[derive(Default, Debug)]
pub struct Paint {
    pub buttons: Vec<(Area, Button)>,
    /// Visible links: their area and their row in the list.
    pub rows: Vec<(Area, usize)>,
    /// (position, length, pitch) of the list.
    pub list: (usize, usize, f32),
}

/// A link as a spine at `y` (its top); selected, it is paper.
fn link(c: &mut Canvas, text: &mut Text, l: &LinkRow, y: i32, selected: bool) {
    let (x, w) = (MARGIN, c.w - 2 * MARGIN);
    crate::kit::shelf_row(c, x, y, w, LINK_H, selected);
    c.rect(x, y, STRIPE, LINK_H, if selected { l.stripe } else { mix(l.stripe, pal().spine, 150) });
    let (fg, dim) = if selected { (pal().ink, pal().label_dust) } else { (pal().chalk, pal().dust) };
    let note_w = if l.note.is_empty() { 0 } else { text.width(Face::Body, 21, &l.note) + 40 };
    let name = text.fit(Face::Body, 28, &l.name, w - 56 - note_w);
    text.draw(c, Face::Body, 28, x + 32, y + LINK_H / 2 + crate::type_scale::base(10, 28), fg, &name);
    if !l.note.is_empty() {
        text.draw_right(c, Face::Body, 21, x + w - 24, y + LINK_H / 2 + 8, dim, &l.note);
    }
}

/// Link rows from `y` on (the top of the first); pushes their spans and returns the baseline of a text line after
/// them, as `row` does.
#[allow(clippy::too_many_arguments)]
fn links(c: &mut Canvas, text: &mut Text, rows: &[LinkRow], first: usize, sel: Option<usize>, y0: i32, mut y: i32, spans: &mut Vec<(i32, i32)>) -> i32 {
    for (i, l) in rows.iter().enumerate() {
        link(c, text, l, y, sel == Some(first + i));
        spans.push((y - y0, y - y0 + LINK_H));
        y += LINK_H + LINK_GAP;
    }
    y - LINK_GAP + 40
}

/// A line of Dust 21 at the baseline `y` before a group of links ("On this Pocket"); returns the top of the first.
fn lead_in(c: &mut Canvas, text: &mut Text, y: i32, s: &str) -> i32 {
    let s = text.fit(Face::Body, 21, s, c.w - 2 * MARGIN);
    text.draw(c, Face::Body, 21, MARGIN, y, pal().dust, &s);
    y + 18
}

/// The page under the headline, drawn at `y0` (negative when scrolled). Pushes the links' spans (relative to the
/// page) in their order and returns the page's height.
fn paint_body(c: &mut Canvas, text: &mut Text, d: &CreditsData, sel: Option<usize>, y0: i32, spans: &mut Vec<(i32, i32)>) -> i32 {
    let cr = d.credits;
    let x = MARGIN;
    let w = c.w - 2 * MARGIN;
    let mut y = y0 + 34;
    if let Some(s) = &cr.summary {
        for line in text.wrap(Face::Body, 28, s, w - 40) {
            text.draw(c, Face::Body, 28, x, y, pal().chalk, &line);
            y += crate::type_scale::line(28, 12);
        }
        y += 30;
    }
    let mut first = 0;
    if !cr.people.is_empty() {
        heading(c, text, x, y, "People");
        y += 46;
        for p in &cr.people {
            y = row(c, text, x, KEY_W, w, y, &p.name, &p.role);
        }
        y += 34;
    }
    if !cr.makers.is_empty() {
        heading(c, text, x, y, "Made by");
        y += 46;
        for m in &cr.makers {
            y = row(c, text, x, KEY_W, w, y, &m.role, &m.name);
        }
        y += 34;
    }
    if !cr.chips.is_empty() {
        heading(c, text, x, y, "Chips");
        y += 46;
        for ch in &cr.chips {
            y = row(c, text, x, KEY_W, w, y, &ch.role, &ch.text());
        }
        if !d.same_chip.is_empty() {
            y = lead_in(c, text, y + 6, "The same chips, in other consoles on this Pocket");
            y = links(c, text, &d.same_chip, first, sel, y0, y, spans);
            first += d.same_chip.len();
        }
        y += 34;
    }
    if !d.family_links.is_empty() || !d.family_text.is_empty() {
        heading(c, text, x, y, "Family");
        y += 46;
        if !d.family_links.is_empty() {
            y = lead_in(c, text, y - 10, "On this Pocket");
            y = links(c, text, &d.family_links, first, sel, y0, y, spans) + 6;
        }
        first += d.family_links.len();
        for (rel, names) in &d.family_text {
            y = row(c, text, x, KEY_W, w, y, rel.label(), &names.join(", "));
        }
        y += 34;
    }
    if !cr.best_selling.is_empty() {
        heading(
            c,
            text,
            x,
            y,
            if cr.best_selling.len() == 1 {
                "Best-selling game"
            } else {
                "Best-selling games"
            },
        );
        y += 46;
        for b in &cr.best_selling {
            for line in text.wrap(Face::Body, 28, &b.text(), w) {
                text.draw(c, Face::Body, 28, x, y, pal().chalk, &line);
                y += crate::type_scale::line(28, 12);
            }
        }
        y += 34;
    }
    if !d.games_ready && !cr.game_makers.is_empty() {
        heading(c, text, x, y, "By the makers, in your library");
        y += 46;
        text.draw(c, Face::Body, 21, x, y - 10, pal().dust, "Getting ready…");
        y += 52;
    } else if !d.games.is_empty() {
        heading(c, text, x, y, "By the makers, in your library");
        y += 46;
        y = lead_in(c, text, y - 10, &format!("Games by {} you have", cr.makers_text()));
        y = links(c, text, &d.games, first, sel, y0, y, spans);
        if d.games_more > 0 {
            let more = if d.games_more == 1 {
                "and 1 more".to_string()
            } else {
                format!("and {} more", d.games_more)
            };
            text.draw(c, Face::Body, 21, x, y - 6, pal().dust, &more);
            y += 28;
        }
        y += 34;
    }
    y - y0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::console_credits;

    fn row(name: &str, link: Link) -> LinkRow {
        LinkRow {
            name: name.into(),
            note: "after".into(),
            stripe: 0x4466aa,
            link,
        }
    }

    fn data(id: &str, games: usize) -> CreditsData {
        CreditsData {
            credits: console_credits::for_system(id).unwrap(),
            console_name: "Game Boy".into(),
            stripe: 0x8c8c8c,
            same_chip: vec![row("Game Boy Color", Link::Console(1))],
            family_links: vec![row("Game Boy Color", Link::Console(1)), row("Game Boy Advance", Link::Console(2))],
            family_text: CreditsData::group_text(&[(Relation::Model, "Game Boy Pocket".into()), (Relation::Before, "Game & Watch".into())]),
            games: (0..games).map(|i| row(&format!("Game {i}"), Link::Game(0, i))).collect(),
            games_more: 3,
            games_ready: true,
        }
    }

    fn paint(p: &mut CreditsPage) -> Paint {
        let (w, h) = (1240, 1080);
        let mut px = vec![0u32; (w * h) as usize];
        let mut text = Text::new();
        p.paint(&mut Canvas::new(&mut px, w as u32, h as u32), &mut text, Some(&crate::status_corner::Corner::battery_only("64%")), 0)
    }

    #[test]
    fn b_and_x_go_back_a_opens_the_selected_link() {
        let mut p = CreditsPage::new(data("gb", 4));
        paint(&mut p);
        assert_eq!(p.press(Button::South), Out::Close);
        assert_eq!(p.press(Button::North), Out::Close, "X again: back to the facts");
        // The first link on screen is selected once the page is painted, if one is on the first screen.
        if let Some(k) = p.sel {
            assert_eq!(p.press(Button::East), Out::Open(data("gb", 4).links()[k].link));
        }
    }

    #[test]
    fn the_first_paint_already_shows_the_first_link_selected() {
        let mut p = CreditsPage::new(data("gb", 4));
        let (w, h) = (1240, 1080);
        let mut px = vec![0u32; (w * h) as usize];
        let mut text = Text::new();
        p.paint(&mut Canvas::new(&mut px, w as u32, h as u32), &mut text, None, 0);
        let k = p.sel.expect("a link on the first screen");
        let (top, _) = p.spans[k];
        let y = (BODY_TOP + top - p.scroll + LINK_H / 2) as usize;
        // Paper under the selected link (not the spine colour), in the same frame that selected it.
        assert_eq!(px[y * w as usize + (MARGIN + 700) as usize], pal().label);
    }

    #[test]
    fn down_walks_every_link_in_order_and_scrolls_to_the_end() {
        let mut p = CreditsPage::new(data("gb", 4));
        paint(&mut p);
        let n = p.data.links().len();
        let mut seen = Vec::new();
        for _ in 0..200 {
            p.press(Button::Down);
            paint(&mut p);
            if let Some(k) = p.sel {
                if seen.last() != Some(&k) {
                    seen.push(k);
                }
            }
        }
        assert_eq!(p.scroll, p.max_scroll, "the end of the page");
        assert_eq!(
            seen,
            (seen[0]..n).collect::<Vec<_>>(),
            "every link after the first one selected, in order, none skipped"
        );
        assert_eq!(p.sel, Some(n - 1));
        for _ in 0..200 {
            p.press(Button::Up);
            paint(&mut p);
        }
        assert_eq!(p.scroll, 0);
        assert!(p.sel.is_none_or(|k| p.visible(k)));
    }

    #[test]
    fn the_selection_is_always_on_screen_and_touch_rows_match_it() {
        let mut p = CreditsPage::new(data("gb", 8));
        let first = paint(&mut p);
        for _ in 0..30 {
            p.press(Button::R1);
            let out = paint(&mut p);
            if let Some(k) = p.sel {
                assert!(p.visible(k));
                let (pos, len, _) = out.list;
                assert!(pos < len);
                // The selected link's touch row is the list's position: a tap on it is "activate".
                let me = out.rows.iter().find(|(_, r)| *r == pos);
                assert!(me.is_some(), "the selected link can be tapped: {:?} at {pos}", out.rows);
            }
        }
        assert!(!first.buttons.is_empty());
    }

    #[test]
    fn a_page_without_links_has_no_selection_and_no_open_hint() {
        let mut d = data("n64", 0);
        d.same_chip.clear();
        d.family_links.clear();
        d.games_more = 0;
        let mut p = CreditsPage::new(d);
        paint(&mut p);
        assert!(p.sel.is_none());
        assert_eq!(p.press(Button::East), Out::None);
        assert!(!p.hints().contains(&("A", "Open")));
    }

    #[test]
    fn every_console_paints_and_family_text_keeps_its_order() {
        for c in console_credits::all() {
            let mut d = data(&c.id, 2);
            d.console_name = c.id.clone();
            let mut p = CreditsPage::new(d);
            paint(&mut p);
            for _ in 0..40 {
                p.press(Button::R1);
            }
            paint(&mut p);
            assert_eq!(p.scroll, p.max_scroll, "{}", c.id);
        }
        let g = CreditsData::group_text(&[
            (Relation::Family, "Super Game Boy".into()),
            (Relation::Model, "Game Boy Pocket".into()),
            (Relation::Model, "Game Boy Light".into()),
            (Relation::Before, "Game & Watch".into()),
        ]);
        assert_eq!(
            g,
            vec![
                (Relation::Before, vec!["Game & Watch".to_string()]),
                (Relation::Model, vec!["Game Boy Pocket".to_string(), "Game Boy Light".to_string()]),
                (Relation::Family, vec!["Super Game Boy".to_string()]),
            ]
        );
    }
}
