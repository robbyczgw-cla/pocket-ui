//! The series card (X on a series' shelf when Home is by series, docs/SERIES-SHELF.md "The series card"): in place of
//! the console page, what the series is (who made it, since when, how many main games, its key games, a short
//! history from `series_facts`) and what of it is on this Pocket (the player's games of it in release order). A series
//! without facts of its own shows what the library knows (its games, their years and consoles), One of a kind says
//! what it holds; no card is ever empty. B goes back, A opens the shelf's games, Up/Down (L1/R1 a page) scroll.
//!
//! Drawn as the console page is (`console_page`): the colour as a stripe down the left edge, Chalk and Dust, section
//! headings in the title face; names and titles painted as they are (never through the tables), the story translated.

use crate::console_page::{heading, paint_hints, row, scrolled, Look, BODY_TOP, HEAD_BASE, STEP, TITLE_BASE};
use crate::gfx::Canvas;
use crate::pad::Button;
use crate::palette::*;
use crate::series_facts::Facts;
use crate::status_corner::Corner;
use crate::text::{Face, Text};
use crate::touch::Area;

const BODY_BOTTOM_GAP: i32 = 130;
/// The line of where the facts come from, over the hint bar.
const SOURCE_ROOM: i32 = 36;

/// One of the player's games on the card.
#[derive(Clone, Debug, PartialEq)]
pub struct Game {
    pub name: String,
    pub year: Option<u16>,
    /// Its console, short ("SNES").
    pub system: String,
}

/// Everything the card shows.
#[derive(Clone, Debug)]
pub struct Data {
    /// The shelf's name: the series', or One of a kind's.
    pub name: String,
    pub facts: Option<&'static Facts>,
    /// One of a kind: the games of no series.
    pub alone: bool,
    /// The colour down the left edge.
    pub stripe: u32,
    /// The player's games on the shelf, in its order.
    pub games: Vec<Game>,
    /// Per key game of `facts`: whether the player has it.
    pub key_owned: Vec<bool>,
    /// One of a kind while no game of the library is on a series' shelf (a library of single games, or one Curator
    /// knows nothing of yet): the card says so.
    pub no_series: bool,
}

impl Data {
    /// Distinct titles among the player's games: the same game on two consoles counts once.
    pub fn titles(&self) -> usize {
        let mut names: Vec<String> = self.games.iter().map(|g| g.name.to_lowercase()).collect();
        names.sort();
        names.dedup();
        names.len()
    }

    /// The headline under the name: "Nintendo · since 1986", or the library's years for a series without facts.
    pub fn headline(&self) -> String {
        if self.alone {
            return crate::consoles::games_text(self.games.len());
        }
        match self.facts {
            Some(f) => format!("{} · since {}", f.by, f.since),
            None => match self.years() {
                Some((a, b)) if a != b => format!("{} · {a} to {b}", crate::consoles::games_text(self.games.len())),
                Some((a, _)) => format!("{} · {a}", crate::consoles::games_text(self.games.len())),
                None => crate::consoles::games_text(self.games.len()),
            },
        }
    }

    /// The first and last year of the player's games of the series.
    pub fn years(&self) -> Option<(u16, u16)> {
        let years = self.games.iter().filter_map(|g| g.year);
        let (lo, hi) = years.fold((u16::MAX, 0), |(lo, hi), y| (lo.min(y), hi.max(y)));
        (hi > 0).then_some((lo, hi))
    }

    /// The consoles of the player's games, in the order they first appear on the shelf.
    pub fn consoles(&self) -> Vec<&str> {
        let mut out: Vec<&str> = Vec::new();
        for g in &self.games {
            if !out.contains(&g.system.as_str()) {
                out.push(&g.system);
            }
        }
        out
    }

    /// "You have" on the card and the Split panel: "5 games", or "5 of 21 main games" when the series' count is known
    /// and not smaller than the player's (who may have spin-offs too).
    pub fn yours(&self) -> String {
        let n = self.titles();
        match self.facts.map(|f| f.games as usize) {
            Some(m) if m > 0 && n <= m => format!("{n} of {m} main games"),
            _ => crate::consoles::games_text(n),
        }
    }

    /// The lines that say what the card is when there is no story: One of a kind's, a series without facts'.
    pub fn about(&self) -> &'static str {
        match (self.alone, self.no_series) {
            (true, true) => "No series found among your games yet. Games of one series join their own shelf once two of them are on your Pocket.",
            (true, false) => "Games that are not part of a series, or the only one of theirs on your Pocket.",
            (false, _) => "Your games of this series, in release order.",
        }
    }
}

/// What a press asks the app to do.
#[derive(Debug, PartialEq, Eq)]
pub enum Out {
    None,
    Back,
    /// A: the shelf's games.
    Games,
}

/// What the last paint offers to touch.
#[derive(Default, Debug)]
pub struct Hits {
    pub buttons: Vec<(Area, Button)>,
    pub scroll: Option<(usize, usize, f32)>,
}

pub struct Page {
    pub data: Data,
    pub scroll: i32,
    max_scroll: i32,
}

impl Page {
    pub fn new(data: Data) -> Page {
        Page { data, scroll: 0, max_scroll: 0 }
    }

    pub fn press(&mut self, b: Button) -> Out {
        let step = match b {
            Button::Up => -STEP,
            Button::Down => STEP,
            Button::L1 => -4 * STEP,
            Button::R1 => 4 * STEP,
            _ => 0,
        };
        match b {
            Button::South => Out::Back,
            Button::East => Out::Games,
            _ if step != 0 => {
                self.scroll = (self.scroll + step).clamp(0, self.max_scroll);
                Out::None
            }
            _ => Out::None,
        }
    }

    pub fn hints(&self) -> Vec<(&'static str, &'static str)> {
        let mut h = vec![("B", "Back"), ("A", "Games")];
        if self.max_scroll > 0 {
            h.push(("L1 R1", "Scroll"));
        }
        h
    }

    pub fn paint(&mut self, c: &mut Canvas, text: &mut Text, corner: Option<&Corner>, look: Look) -> Hits {
        c.clear(look.bg);
        c.rect(0, 0, STRIPE, c.h, self.data.stripe);
        let left = corner.map_or(c.w - MARGIN, |k| k.paint(c, text, c.w - MARGIN, 80, pal().dust));
        let corner_w = c.w - MARGIN - crate::status_corner::room_left_of(left, c.w - MARGIN);
        let full_w = c.w - 2 * MARGIN;
        // The name as it is ("The Legend of Zelda"); One of a kind is ours and translated.
        let name = self.data.name.clone();
        let alone = self.data.alone;
        let draw_title = |text: &mut Text, c: &mut Canvas| {
            let (px, title) = text.fit_sizes(Face::Title, &[67, 50], &name, full_w - corner_w);
            text.draw(c, Face::Title, px, MARGIN, TITLE_BASE, pal().text, &title);
        };
        if alone {
            draw_title(text, c);
        } else {
            text.literal(|t| draw_title(t, c));
        }
        let head = self.data.headline();
        let head = text.fit(Face::Body, 28, &head, full_w);
        text.draw(c, Face::Body, 28, MARGIN, HEAD_BASE, look.dim, &head);
        let sources = self.data.facts.is_some();
        let gap = BODY_BOTTOM_GAP + if sources { SOURCE_ROOM } else { 0 };
        let data = &self.data;
        let (height, view) = scrolled(c, BODY_TOP, c.h - gap, self.scroll, look.bg, |body, y0| paint_body(body, text, data, y0));
        self.max_scroll = (height - view).max(0);
        self.scroll = self.scroll.min(self.max_scroll);
        if sources {
            let line = text.fit(Face::Body, 21, "Facts: Wikipedia, written for pocket-ui", full_w);
            text.draw(c, Face::Body, 21, MARGIN, crate::kit::hint_bar_top(c.h) - 12, pal().dust, &line);
        }
        if self.max_scroll > 0 {
            let (top, bottom) = (BODY_TOP, c.h - gap);
            let view_h = bottom - top;
            let total = view_h + self.max_scroll;
            let bar_h = (view_h * view_h / total).max(40);
            let bar_y = top + (view_h - bar_h) * self.scroll / self.max_scroll.max(1);
            c.rect(c.w - 28, top, 4, view_h, pal().spine);
            c.rect(c.w - 28, bar_y, 4, bar_h, pal().dust);
        }
        let mut hits = Hits { scroll: Some(((self.scroll / STEP) as usize, (self.max_scroll + STEP - 1) as usize / STEP as usize + 1, STEP as f32)), ..Default::default() };
        hits.buttons.extend(paint_hints(c, text, &self.hints()));
        hits
    }
}

/// One piece of the card's body, top to bottom (`parts`).
#[derive(Clone, Debug, PartialEq)]
pub enum Part {
    /// The story, or what the card is.
    Text(String),
    Heading(&'static str),
    /// A label and its value; `names`: the value is names (consoles), painted as they are.
    Row(&'static str, String, bool),
    /// A key game and whether the player has it.
    Key(&'static str, bool),
    /// One of the player's games (index into `Data::games`: a shelf of thousands is not copied each frame).
    Game(usize),
    /// A quiet line where a list has nothing.
    Quiet(&'static str),
}

/// What the card says, in order: the story (or what it is), the series' facts, its key games, the player's games.
pub fn parts(d: &Data) -> Vec<Part> {
    let mut out = vec![Part::Text(d.facts.map_or(d.about(), |f| f.story).to_string())];
    if !d.alone {
        out.push(Part::Heading("The series"));
        match d.facts {
            Some(f) => {
                out.push(Part::Row("Since", f.since.to_string(), false));
                if f.games > 0 {
                    out.push(Part::Row("Main games", f.games.to_string(), false));
                }
            }
            None => {
                if let Some((a, b)) = d.years() {
                    out.push(Part::Row("Years", if a == b { a.to_string() } else { format!("{a} to {b}") }, false));
                }
            }
        }
        out.push(Part::Row("You have", d.yours(), false));
        let consoles = d.consoles().join(", ");
        if !consoles.is_empty() {
            out.push(Part::Row("Consoles", consoles, true));
        }
        if let Some(f) = d.facts {
            out.push(Part::Heading("Key games"));
            out.extend(f.key.iter().enumerate().map(|(i, t)| Part::Key(t, d.key_owned.get(i).copied().unwrap_or(false))));
        }
    }
    out.push(Part::Heading(if d.alone { "Your games" } else { "Your games of this series" }));
    if d.games.is_empty() {
        out.push(Part::Quiet("No games yet"));
    }
    out.extend((0..d.games.len()).map(Part::Game));
    out
}

/// The body's parts at `y0`. Returns the height.
fn paint_body(c: &mut Canvas, text: &mut Text, d: &Data, y0: i32) -> i32 {
    let x = MARGIN;
    let w = c.w - 2 * MARGIN - 40;
    let key_w = 220;
    let line = crate::type_scale::line(28, 12);
    let value_line = crate::type_scale::line(28, 8);
    let mut y = y0 + 34;
    let mut first = true;
    for part in parts(d) {
        match part {
            Part::Text(s) => {
                for l in text.wrap_prose(Face::Body, 28, &s, w) {
                    text.draw(c, Face::Body, 28, x, y, pal().chalk, &l);
                    y += line;
                }
            }
            Part::Heading(h) => {
                y += if first { 0 } else { 34 };
                heading(c, text, x, y, h);
                y += 46 + crate::type_scale::grow(37);
            }
            Part::Row(label, value, false) => y = row(c, text, x, key_w, w, y, label, &value),
            Part::Row(label, value, true) => {
                let (kw, lines, stacked) = crate::console_page::row_layout(text, key_w, w, label, &value);
                text.draw(c, Face::Body, 28, x, y, pal().dust, label);
                let (vx, vy) = if stacked { (x + 24, y + value_line) } else { (x + kw, y) };
                text.literal(|t| {
                    for (i, l) in lines.iter().enumerate() {
                        t.draw(c, Face::Body, 28, vx, vy + i as i32 * value_line, pal().chalk, l);
                    }
                });
                y += crate::type_scale::row(40, 28) + (lines.len().max(1) as i32 - 1) * value_line + if stacked { value_line } else { 0 };
            }
            Part::Key(title, owned) => {
                let mark = "On your Pocket";
                let mark_w = if owned { text.width(Face::Body, 24, mark) } else { 0 };
                text.literal(|t| {
                    let shown = t.fit(Face::Body, 28, title, w - mark_w - 24);
                    t.draw(c, Face::Body, 28, x, y, if owned { pal().chalk } else { pal().dust }, &shown);
                });
                if owned {
                    text.draw(c, Face::Body, 24, x + w - mark_w, y, pal().dust, mark);
                }
                y += line;
            }
            // Only the rows in view are measured and painted; the rest only count their height (One of a kind can hold
            // thousands, and every scroll paints again).
            Part::Game(_) if y + line < 0 || y - line > c.h => y += line,
            Part::Game(i) => {
                let g = &d.games[i];
                let year_w = text.width_plain(Face::Body, 28, "8888") + 24;
                let sys = text.literal(|t| t.fit(Face::Body, 24, &g.system, w / 3));
                let sys_w = text.width_plain(Face::Body, 24, &sys);
                text.literal(|t| {
                    if let Some(year) = g.year {
                        t.draw(c, Face::Body, 28, x, y, pal().dust, &year.to_string());
                    }
                    let name = t.fit(Face::Body, 28, &g.name, w - year_w - sys_w - 24);
                    t.draw(c, Face::Body, 28, x + year_w, y, pal().chalk, &name);
                    t.draw(c, Face::Body, 24, x + w - sys_w, y, pal().dust, &sys);
                });
                y += line;
            }
            Part::Quiet(s) => {
                text.draw(c, Face::Body, 28, x, y, pal().dust, s);
                y += line;
            }
        }
        first = false;
    }
    y + 34 - y0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn game(name: &str, year: Option<u16>, system: &str) -> Game {
        Game { name: name.into(), year, system: system.into() }
    }

    fn data(facts: Option<&'static Facts>, alone: bool, games: Vec<Game>) -> Data {
        let key_owned = facts.map_or(Vec::new(), |f| vec![false; f.key.len()]);
        Data { name: if alone { crate::series_shelf::ONE_OF_A_KIND.into() } else { "Skyrunner".into() }, facts, alone, stripe: 0x336699, games, key_owned, no_series: false }
    }

    /// Paints the page at `w` x `h` (nothing may panic or run off) and returns what it says.
    fn paint(page: &mut Page, w: u32, h: u32) -> Vec<Part> {
        let mut px = vec![0u32; (w * h) as usize];
        let mut text = Text::new();
        page.paint(&mut Canvas::new(&mut px, w, h), &mut text, None, Look::plain());
        parts(&page.data)
    }

    fn says(parts: &[Part], want: &str) -> bool {
        parts.iter().any(|p| match p {
            Part::Text(s) => s.starts_with(want),
            Part::Heading(h) | Part::Quiet(h) => *h == want,
            Part::Row(l, v, _) => *l == want || v == want,
            Part::Key(t, _) => *t == want,
            Part::Game(_) => false,
        })
    }

    #[test]
    fn a_series_without_facts_shows_what_the_library_knows_never_an_empty_card() {
        let d = data(None, false, vec![game("Skyrunner", Some(2001), "PS2"), game("Skyrunner 2", Some(2004), "PS2"), game("Skyrunner 2", Some(2004), "PSP")]);
        assert_eq!(d.headline(), "3 games · 2001 to 2004");
        assert_eq!(d.yours(), "2 games", "one title on two consoles is one");
        assert_eq!(d.consoles(), ["PS2", "PSP"]);
        let shown = paint(&mut Page::new(d), 1280, 720);
        for want in ["Your games of this series, in release order.", "The series", "Years", "2001 to 2004", "You have", "Your games of this series"] {
            assert!(says(&shown, want), "{want} in {shown:?}");
        }
        assert_eq!(shown.iter().filter(|p| matches!(p, Part::Game(_))).count(), 3, "every game listed: {shown:?}");
    }

    #[test]
    fn a_series_with_facts_shows_its_story_count_and_key_games() {
        let f = &crate::series_facts::ALL[0];
        let mut d = data(Some(f), false, vec![game("A game", Some(f.since), "NES")]);
        d.key_owned[0] = true;
        assert!(d.headline().starts_with(f.by));
        if f.games > 0 {
            assert_eq!(d.yours(), format!("1 of {} main games", f.games));
        }
        let shown = paint(&mut Page::new(d), 1280, 1080);
        assert!(says(&shown, "Key games") && says(&shown, f.story), "{shown:?}");
        assert!(shown.contains(&Part::Key(f.key[0], true)), "the owned key game is marked: {shown:?}");
    }

    #[test]
    fn one_of_a_kind_says_what_it_holds_and_when_no_series_was_found() {
        let mut d = data(None, true, vec![game("Tetris", Some(1989), "GB")]);
        assert_eq!(d.headline(), "1 game");
        let shown = paint(&mut Page::new(d.clone()), 640, 480);
        assert!(says(&shown, "Games that are not part"), "{shown:?}");
        assert!(!says(&shown, "The series"), "no series facts for the games of none");
        d.no_series = true;
        let shown = paint(&mut Page::new(d.clone()), 640, 480);
        assert!(says(&shown, "No series found"), "{shown:?}");
        d.games.clear();
        assert!(says(&paint(&mut Page::new(d), 1024, 768), "No games yet"), "an empty list says so");
    }

    /// Sol's review 26.11.2026: X on One of a kind measured and painted every game at every scroll. Only the rows in view
    /// cost: a card of 20,000 games paints about as fast as one of 20.
    #[test]
    fn a_card_of_thousands_of_games_paints_only_what_is_in_view() {
        let page = |n: u16| Page::new(data(None, true, (0..n).map(|i| game(&format!("Game {i}"), Some(1980 + i % 40), "SNES")).collect()));
        let time = |p: &mut Page| {
            paint(p, 1280, 720);
            let mut best = std::time::Duration::MAX;
            for _ in 0..3 {
                let t = std::time::Instant::now();
                paint(p, 1280, 720);
                best = best.min(t.elapsed());
            }
            best
        };
        let (small, big) = (time(&mut page(20)), time(&mut page(20_000)));
        assert!(big < small * 5 + std::time::Duration::from_millis(20), "20 games {small:?}, 20,000 games {big:?}");
    }

    #[test]
    fn b_goes_back_a_opens_the_games_and_a_long_card_scrolls() {
        let games = (0..60).map(|i| game(&format!("Skyrunner {i}"), Some(1990 + i), "SNES")).collect();
        let mut page = Page::new(data(None, false, games));
        paint(&mut page, 640, 480);
        assert_eq!(page.press(Button::South), Out::Back);
        assert_eq!(page.press(Button::East), Out::Games);
        assert!(page.hints().iter().any(|(k, _)| *k == "L1 R1"), "{:?}", page.hints());
        page.press(Button::Down);
        assert!(page.scroll > 0);
    }
}
