//! The console page (X on a system): what the console was, from `consoles` (assets/data/consoles.json), and what
//! it is on this Pocket (games, time, the emulator, the BIOS). B goes back, A opens the system's games, X its
//! Credits & links (`credits_page`), Y lists the sources, up/down (L1/R1 a page) scroll when the page is longer
//! than the screen.
//!
//! Drawn like the game card: Chalk and Dust on the page, the system's colour as a stripe down the left edge and
//! behind its picture, section headings in the title face. Also the short facts the Split look shows under a
//! selected system (`paint_facts`).

use crate::consoles::{self, Console, Row};
use crate::credits_page::{self, CreditsData, CreditsPage, Link};
use crate::gfx::{mix, Canvas};
use crate::pad::Button;
use crate::palette::*;
use crate::status_corner::Corner;
use crate::system_art::Sprite;
use crate::text::{Face, Text};
use crate::touch::{hint_button, Area};

pub(crate) const TITLE_BASE: i32 = 118;
pub(crate) const HEAD_BASE: i32 = 162;
/// The scrolling part: under the headline, above the credit line and the hints.
pub(crate) const BODY_TOP: i32 = 184;
const BODY_BOTTOM_GAP: i32 = 130;
const ROW: i32 = 40;
const SMALL_LINE: i32 = 28;

/// `ROW`, `SMALL_LINE` and the 36 px between the lines of a value at the text size in use (Settings → Text size).
fn row_step() -> i32 {
    crate::type_scale::line(28, ROW - 28)
}

fn small_line() -> i32 {
    crate::type_scale::line(21, SMALL_LINE - 21)
}

fn value_line() -> i32 {
    crate::type_scale::line(28, 8)
}
/// Where the second column starts, and the console's picture box at the top right.
const RIGHT_X: i32 = 700;
const ART_W: i32 = 420;
const ART_H: i32 = 276;
/// One press of up/down scrolls this far.
pub(crate) const STEP: i32 = 2 * ROW;

/// Everything the page shows; the console's facts and what the app found out about it on this device.
#[derive(Clone, Debug)]
pub struct PageData {
    pub console: &'static Console,
    pub stripe: u32,
    /// Games of the system in the library.
    pub games: usize,
    /// Seconds played on the system, as far as recorded; 0 leaves the row out.
    pub played_secs: u64,
    /// The game played last and when (unix seconds).
    pub last: Option<(String, u64)>,
    /// "Flycast on pocket-core" (`consoles::runner_text`).
    pub runs_on: Option<String>,
    /// `consoles::bios_status`.
    pub bios: Option<String>,
    /// RetroAchievements over the console's games with a set: "35 of 120 achievements in 3 games" (`achievements::snapshot`).
    pub achievements: Option<String>,
}

/// The page's colours that depend on the look: the page itself and its quieter text (the Split look's are its own).
#[derive(Clone, Copy, Debug)]
pub struct Look {
    pub bg: u32,
    pub dim: u32,
}

impl Look {
    pub fn plain() -> Look {
        Look {
            bg: pal().void,
            dim: pal().quiet,
        }
    }

    pub fn split() -> Look {
        Look {
            bg: pal().split_bg,
            dim: pal().split_dim,
        }
    }
}

/// What a button press on the page asks the app to do.
#[derive(Debug, PartialEq)]
pub enum Out {
    None,
    Back,
    /// A: the system's list of games.
    Games,
    /// START: which emulator plays the console's games ("Runs on", `emu_choice_page`).
    Emulator,
    /// X: Credits & links; the App finds the links in the library and calls `open_credits`.
    Credits,
    /// A on a link of Credits & links.
    Open(Link),
}

/// What the last paint offers to touch: the hint keys, and the scroll position as a list (row, rows, pitch).
#[derive(Default, Debug)]
pub struct Hits {
    pub buttons: Vec<(Area, Button)>,
    pub scroll: Option<(usize, usize, f32)>,
    /// Credits & links: the links on screen as rows of the list in `scroll`.
    pub rows: Vec<(Area, usize)>,
}

pub struct Page {
    pub data: PageData,
    pub scroll: i32,
    max_scroll: i32,
    /// Y: the sources instead of the facts, and how far they are scrolled.
    pub sources: Option<i32>,
    sources_max: i32,
    /// X: Credits & links, over the facts.
    pub credits: Option<CreditsPage>,
}

impl Page {
    pub fn new(data: PageData) -> Page {
        Page {
            data,
            scroll: 0,
            max_scroll: 0,
            sources: None,
            sources_max: 0,
            credits: None,
        }
    }

    /// Where the player is on the page (for the UI sounds).
    pub fn mark(&self) -> (i32, Option<i32>, Option<credits_page::Mark>) {
        (self.scroll, self.sources, self.credits.as_ref().map(CreditsPage::mark))
    }

    /// Whether the console has Credits & links (X).
    pub fn has_credits(&self) -> bool {
        crate::console_credits::for_system(&self.data.console.id).is_some()
    }

    /// Shows Credits & links with the links the App found.
    pub fn open_credits(&mut self, data: CreditsData) {
        self.credits = Some(CreditsPage::new(data));
    }

    fn scrollable(&self) -> bool {
        if self.sources.is_some() {
            self.sources_max > 0
        } else {
            self.max_scroll > 0
        }
    }

    pub fn press(&mut self, b: Button) -> Out {
        if let Some(credits) = self.credits.as_mut() {
            return match credits.press(b) {
                credits_page::Out::None => Out::None,
                credits_page::Out::Close => {
                    self.credits = None;
                    Out::None
                }
                credits_page::Out::Open(link) => Out::Open(link),
            };
        }
        let step = match b {
            Button::Up => -STEP,
            Button::Down => STEP,
            Button::L1 => -4 * STEP,
            Button::R1 => 4 * STEP,
            _ => 0,
        };
        if let Some(at) = self.sources.as_mut() {
            match b {
                Button::South | Button::West => self.sources = None,
                _ if step != 0 => *at = (*at + step).clamp(0, self.sources_max),
                _ => {}
            }
            return Out::None;
        }
        match b {
            Button::South => Out::Back,
            Button::East => Out::Games,
            Button::Start => Out::Emulator,
            Button::North if self.has_credits() => Out::Credits,
            Button::West => {
                self.sources = Some(0);
                Out::None
            }
            _ if step != 0 => {
                self.scroll = (self.scroll + step).clamp(0, self.max_scroll);
                Out::None
            }
            _ => Out::None,
        }
    }

    pub fn hints(&self) -> Vec<(&'static str, &'static str)> {
        if let Some(credits) = &self.credits {
            return credits.hints();
        }
        let mut h = if self.sources.is_some() {
            vec![("B", "Back")]
        } else {
            vec![("B", "Back"), ("A", "Games"), ("X", "Credits"), ("Y", "Sources"), ("START", "Emulator")]
        };
        if self.sources.is_none() && !self.has_credits() {
            h.retain(|(k, _)| *k != "X");
        }
        if self.scrollable() {
            h.push(("L1 R1", "Scroll"));
        }
        h
    }

    /// Paints the page. `art`: the console's picture, already fitted into `art_box`; `corner`: the status corner.
    pub fn paint(&mut self, c: &mut Canvas, text: &mut Text, art: Option<&Sprite>, corner: Option<&Corner>, look: Look) -> Hits {
        if let Some(credits) = self.credits.as_mut() {
            let p = credits.paint(c, text, corner, look.bg);
            return Hits {
                buttons: p.buttons,
                scroll: Some(p.list),
                rows: p.rows,
            };
        }
        c.clear(look.bg);
        c.rect(0, 0, STRIPE, c.h, self.data.stripe);
        let console = self.data.console;
        let left = corner.map_or(c.w - MARGIN, |k| k.paint(c, text, c.w - MARGIN, 80, pal().dust));
        let corner_w = c.w - MARGIN - crate::status_corner::room_left_of(left, c.w - MARGIN);
        let full_w = c.w - 2 * MARGIN;
        let mut hits = Hits::default();
        if self.sources.is_some() {
            let title = text.fit(Face::Title, 67, "Sources", full_w - corner_w);
            text.draw(c, Face::Title, 67, MARGIN, TITLE_BASE, pal().text, &title);
            let sub = text.fit(Face::Body, 28, &console.name, full_w);
            text.draw(c, Face::Body, 28, MARGIN, HEAD_BASE, look.dim, &sub);
            let at = self.sources.unwrap_or(0);
            let (height, view) = scrolled(c, BODY_TOP, c.h - 100, at, look.bg, |body, y0| paint_sources(body, text, console, y0));
            self.sources_max = (height - view).max(0);
            let at = at.min(self.sources_max);
            self.sources = Some(at);
            hits.scroll = Some(scroll_list(at, self.sources_max));
        } else {
            let title = text.fit(Face::Title, 67, &console.name, full_w - corner_w);
            text.draw(c, Face::Title, 67, MARGIN, TITLE_BASE, pal().text, &title);
            if let Some(h) = &console.lines.headline {
                let h = text.fit(Face::Body, 28, h, full_w);
                text.draw(c, Face::Body, 28, MARGIN, HEAD_BASE, look.dim, &h);
            }
            let data = &self.data;
            let (height, view) = scrolled(c, BODY_TOP, c.h - BODY_BOTTOM_GAP, self.scroll, look.bg, |body, y0| {
                paint_body(body, text, data, art, look, y0)
            });
            self.max_scroll = (height - view).max(0);
            self.scroll = self.scroll.min(self.max_scroll);
            hits.scroll = Some(scroll_list(self.scroll, self.max_scroll));
            // Where the facts come from; Y lists the pages.
            let names = console.source_names().join(" · ");
            let names = text.fit(Face::Body, 21, &names, full_w);
            text.draw_right(c, Face::Body, 21, c.w - MARGIN, c.h - 104, pal().dust, &names);
        }
        let (at, max, top, bottom) = match self.sources {
            Some(at) => (at, self.sources_max, BODY_TOP, c.h - 100),
            None => (self.scroll, self.max_scroll, BODY_TOP, c.h - BODY_BOTTOM_GAP),
        };
        if max > 0 {
            // A thin bar on the right: where in the page the view is.
            let view_h = bottom - top;
            let total = view_h + max;
            let bar_h = (view_h * view_h / total).max(40);
            let bar_y = top + (view_h - bar_h) * at / max.max(1);
            c.rect(c.w - 28, top, 4, view_h, pal().spine);
            c.rect(c.w - 28, bar_y, 4, bar_h, pal().dust);
        }
        hits.buttons = paint_hints(c, text, &self.hints());
        hits
    }
}

/// The size the console's picture is fitted into (its box is a little larger).
pub fn art_box() -> (i32, i32) {
    (ART_W - 40, ART_H - 32)
}

/// The scroll position as a list for touch swipes: one row per press of up/down.
fn scroll_list(at: i32, max: i32) -> (usize, usize, f32) {
    ((at / STEP) as usize, (max + STEP - 1) as usize / STEP as usize + 1, STEP as f32)
}

/// Paints `f` into the strip from `top` to `bottom`, moved up by `scroll`; text running under an edge fades out
/// there. Returns the height `f` painted and the strip's height.
pub(crate) fn scrolled(c: &mut Canvas, top: i32, bottom: i32, scroll: i32, bg: u32, f: impl FnOnce(&mut Canvas, i32) -> i32) -> (i32, i32) {
    let w = c.w;
    let view_h = bottom - top;
    let mut body = Canvas::new(&mut c.buf[(top * w) as usize..(bottom * w) as usize], w as u32, view_h as u32);
    let height = f(&mut body, -scroll);
    let max = (height - view_h).max(0);
    const FADE: i32 = 36;
    for k in 0..FADE {
        let amount = (255 - k * 255 / FADE) as u8;
        // (Not over the stripe down the left edge.)
        if scroll > 0 {
            body.shade_rect(STRIPE, k, w - STRIPE, 1, bg, amount);
        }
        if scroll < max {
            body.shade_rect(STRIPE, view_h - 1 - k, w - STRIPE, 1, bg, amount);
        }
    }
    (height, view_h)
}

/// A section heading: "Timeline", "Hardware", "On this Pocket".
pub(crate) fn heading(c: &mut Canvas, text: &mut Text, x: i32, y: i32, s: &str) {
    text.draw(c, Face::Title, 37, x, y, pal().text, s);
}

/// A label/value row; the value wraps in its column. Returns the y of the next row.
#[allow(clippy::too_many_arguments)]
pub(crate) fn row(c: &mut Canvas, text: &mut Text, x: i32, key_w: i32, w: i32, y: i32, label: &str, value: &str) -> i32 {
    // The label column is as wide as its labels need at the text size in use.
    let key_w = crate::type_scale::wide(key_w, 28);
    text.draw(c, Face::Body, 28, x, y, pal().dust, label);
    let lines = text.wrap(Face::Body, 28, value, w - key_w);
    let n = lines.len().max(1) as i32;
    for (i, line) in lines.iter().enumerate() {
        text.draw(c, Face::Body, 28, x + key_w, y + i as i32 * value_line(), pal().chalk, line);
    }
    y + row_step() + (n - 1) * value_line()
}

/// Small print under a row, in Dust.
fn small(c: &mut Canvas, text: &mut Text, x: i32, w: i32, y: i32, s: &str) -> i32 {
    let mut y = y - 8;
    for line in text.wrap(Face::Body, 21, s, w) {
        text.draw(c, Face::Body, 21, x, y, pal().dust, &line);
        y += small_line();
    }
    y + 12
}

/// A region as a label of `key_w`: in words when they fit, else the codes ("UK, IE, IT, AU").
fn region_label(text: &mut Text, key: &str, key_w: i32) -> String {
    let name = consoles::region_name(key);
    if text.width(Face::Body, 28, &name) <= crate::type_scale::wide(key_w, 28) - 16 {
        name
    } else {
        consoles::region_codes(key)
    }
}

/// The facts and the player's side, in two columns, then the family and the note on the sales figure across the
/// page; drawn at `y0` (negative when scrolled). Returns the height.
fn paint_body(c: &mut Canvas, text: &mut Text, d: &PageData, art: Option<&Sprite>, look: Look, y0: i32) -> i32 {
    let con = d.console;
    let (lx, rx) = (MARGIN, RIGHT_X);
    let lw = rx - 28 - lx;
    let rw = c.w - MARGIN - rx;
    // Right column: the console on its colour (a system without a picture has no box), then the hardware and the
    // launch prices.
    let art_x = c.w - MARGIN - ART_W;
    let mut ry = y0 + 34;
    if let Some(a) = art {
        c.rect(art_x, y0, ART_W, ART_H, mix(d.stripe, look.bg, 90));
        let (aw, ah) = (a.w as i32, a.h as i32);
        c.blit_sprite(a, art_x + (ART_W - aw) / 2, y0 + (ART_H - ah) / 2, 0);
        ry = y0 + ART_H + 56;
    }
    // The summary runs beside the picture, or within its column when there is none.
    let summary_w = if art.is_some() { art_x - 40 - lx } else { lw };
    let hw = con.hardware();
    if !hw.is_empty() {
        heading(c, text, rx, ry, "Hardware");
        ry += 46 + crate::type_scale::grow(37);
        for (k, v) in hw {
            ry = row(c, text, rx, 150, rw, ry, k, &v);
        }
        ry += 34;
    }
    if !con.launch_price.is_empty() {
        heading(c, text, rx, ry, "Launch price");
        ry += 46 + crate::type_scale::grow(37);
        for (region, price) in &con.launch_price {
            let label = region_label(text, region, 230);
            ry = row(c, text, rx, 230, rw, ry, &label, price);
        }
        ry += 34;
    }

    // Left column: what it was, when, and what it is here.
    let mut ly = y0 + 34;
    if let Some(s) = &con.summary {
        for line in text.wrap(Face::Body, 28, s, summary_w) {
            text.draw(c, Face::Body, 28, lx, ly, pal().chalk, &line);
            ly += crate::type_scale::line(28, 12);
        }
    }
    if !con.aka.is_empty() {
        ly = small(c, text, lx, summary_w, ly + 8, &format!("Also known as {}", con.aka.join(", ")));
    }
    ly += 66;
    let key_w = 200;
    heading(c, text, lx, ly, "Timeline");
    ly += 46 + crate::type_scale::grow(37);
    for (region, date) in con.releases() {
        let label = region_label(text, &region, key_w);
        ly = row(c, text, lx, key_w, lw, ly, &label, &consoles::readable_date(&date));
    }
    if let Some(end) = &con.discontinued {
        ly = row(c, text, lx, key_w, lw, ly, "Discontinued", &consoles::readable_date(end));
        if let Some(note) = &con.lines.ended_note {
            ly = small(c, text, lx + key_w, lw - key_w, ly, note);
        }
    }
    if let Some(sold) = &con.lines.sold {
        ly = row(c, text, lx, key_w, lw, ly, "Sold", sold);
    }
    ly += 34;
    heading(c, text, lx, ly, "On this Pocket");
    ly += 46 + crate::type_scale::grow(37);
    let mut yours = consoles::games_text(d.games);
    if let Some((name, _)) = &d.last {
        yours = format!("{yours} · last: {name}");
    }
    ly = row(c, text, lx, key_w, lw, ly, "Your games", &yours);
    let played = match (d.played_secs, &d.last) {
        (0, _) => None,
        (secs, Some((_, at))) => Some(format!("{}, last {}", crate::recents::duration(secs), crate::recents::ago(*at))),
        (secs, None) => Some(crate::recents::duration(secs)),
    };
    if let Some(p) = played {
        ly = row(c, text, lx, key_w, lw, ly, "Played", &p);
    }
    if let Some(r) = &d.runs_on {
        ly = row(c, text, lx, key_w, lw, ly, "Emulator", r);
    }
    if let Some(b) = &d.bios {
        ly = row(c, text, lx, key_w, lw, ly, "BIOS", b);
    }
    if let Some(a) = &d.achievements {
        ly = row(c, text, lx, key_w, lw, ly, "Unlocked", a);
    }

    // Across the page, under both columns: the consoles before and after, and what the sales figure means.
    let full_w = c.w - 2 * MARGIN;
    let mut y = ly.max(ry) + 34;
    if con.predecessor.is_some() || con.successor.is_some() {
        heading(c, text, lx, y, "Family");
        y += 46 + crate::type_scale::grow(37);
        if let Some(p) = &con.predecessor {
            y = row(c, text, lx, key_w, full_w, y, "Preceded by", p);
        }
        if let Some(s) = &con.successor {
            y = row(c, text, lx, key_w, full_w, y, "Followed by", s);
        }
        y += 34;
    }
    if let Some(note) = con.sold_note() {
        heading(c, text, lx, y, "Sales figure");
        y += 46 + crate::type_scale::grow(37);
        y = small(c, text, lx, full_w, y, &note) + 20;
    }
    y - y0
}

/// Y: where the facts come from, every page in full.
fn paint_sources(c: &mut Canvas, text: &mut Text, con: &Console, y0: i32) -> i32 {
    let w = c.w - 2 * MARGIN - 40;
    let mut y = y0 + 34;
    if let Some(l) = consoles::license() {
        for line in text.wrap(Face::Body, 21, l, w) {
            text.draw(c, Face::Body, 21, MARGIN, y, pal().dust, &line);
            y += small_line();
        }
        y += 30;
    }
    for url in &con.sources {
        text.draw(c, Face::Body, 21, MARGIN, y, pal().dust, consoles::source_name(url));
        y += 34;
        let shown = url.strip_prefix("https://").or_else(|| url.strip_prefix("http://")).unwrap_or(url);
        for line in wrap_url(text, shown, w) {
            text.draw(c, Face::Body, 24, MARGIN, y, pal().chalk, &line);
            y += 32;
        }
        y += 22;
    }
    y - y0
}

/// An address broken into lines of at most `max` pixels, after a '/', '?', '&' or '=' where one is close.
pub fn wrap_url(text: &mut Text, url: &str, max: i32) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for ch in url.chars() {
        line.push(ch);
        if text.width(Face::Body, 24, &line) > max && line.chars().count() > 1 {
            let last = line.pop().unwrap_or_default();
            let cut = line.rfind(['/', '?', '&', '=']).filter(|&i| i > line.len() / 2).map_or(line.len(), |i| i + 1);
            let rest = line.split_off(cut);
            lines.push(std::mem::take(&mut line));
            line = rest;
            line.push(last);
        }
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

/// The hint keys at the bottom left, as on the shelf; returns where each key can be tapped.
pub fn paint_hints(c: &mut Canvas, text: &mut Text, hints: &[(&str, &str)]) -> Vec<(Area, Button)> {
    use crate::type_scale;
    let (mut x, base) = (MARGIN, c.h - 52);
    let mut areas = Vec::new();
    // The keys are 36 px around Body 21, taller with larger text (`type_scale`).
    let key_h = type_scale::row(36, 21);
    let key_top = base - 9 - key_h / 2 - type_scale::grow(21) / 3;
    // One letter in a round key, a word in a key as wide as it.
    let widths: Vec<(i32, i32)> = hints.iter().map(|(k, l)| (text.width(Face::Body, 21, k), text.width(Face::Body, 21, l))).collect();
    let shown = type_scale::hint_labels_that_fit(&widths.iter().map(|(kw, lw)| ((kw + 24).max(key_h) + 10, lw + 36)).collect::<Vec<_>>(), c.w - MARGIN - 24 + 36);
    for (k, (key, label)) in hints.iter().enumerate() {
        let kw = widths[k].0;
        let key_w = (kw + 24).max(key_h);
        let start = x;
        c.round_rect(x, key_top, key_w, key_h, key_h / 2, pal().keycap);
        text.draw(c, Face::Body, 21, x + (key_w - kw) / 2, base - 1, pal().chalk, key);
        x += key_w + 10;
        if k < shown {
            x += text.draw(c, Face::Body, 21, x, base, pal().dust, label);
        }
        x += 36;
        if let Some(b) = hint_button(key) {
            areas.push((Area::new(start - 8, base - 46, x - start - 20, 64), b));
        }
    }
    // For the snapshot checks of Settings → Text size: nothing else may run into the hint bar.
    crate::text::boxes::area(c, (MARGIN - 4, key_top, x - 36 - MARGIN + 8, key_h), "‹hints›");
    areas
}

/// Row height of the short facts on the shelf.
pub const FACT_ROW: i32 = 46;
/// Width of their label column.
const FACT_KEY: i32 = 190;

/// The short facts under a selected system (the Split look): label/value rows from `first` (the first baseline)
/// down to `bottom` (the lowest a baseline may be), as many as fit (`consoles::rows_that_fit`), each value at 28 px, or 24 when it doesn't fit,
/// or shortened. Returns the number of rows drawn.
#[allow(clippy::too_many_arguments)]
pub fn paint_facts(c: &mut Canvas, text: &mut Text, rows: &[Row], x: i32, first: i32, w: i32, bottom: i32, label: u32, value: u32) -> usize {
    // Taller rows and a wider label column with larger text (Settings → Text size, `type_scale`).
    let (step, key) = (crate::type_scale::row(FACT_ROW, 28), crate::type_scale::wide(FACT_KEY, 28));
    let room = bottom - first + step;
    let shown = consoles::rows_that_fit(rows, room, step);
    for (i, r) in shown.iter().enumerate() {
        let y = first + i as i32 * step;
        text.draw(c, Face::Body, 28, x, y, label, r.label);
        let (v, px) = consoles::pick_value(&r.values, w - key, 28, 24, |s, px| text.width(Face::Body, px, s));
        text.draw(c, Face::Body, px, x + key, y, value, &v);
    }
    shown.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data(id: &str) -> PageData {
        PageData {
            console: consoles::for_system(id).unwrap(),
            stripe: crate::bezel::system_colour(id),
            games: 1,
            played_secs: 3600,
            last: Some(("Sonic Adventure 2".into(), 0)),
            runs_on: Some("Flycast on pocket-core".into()),
            bios: None,
            achievements: None,
        }
    }

    fn paint(page: &mut Page) -> (Vec<u32>, Hits) {
        let (w, h) = (1240, 1080);
        let mut px = vec![0u32; (w * h) as usize];
        let mut text = Text::new();
        let hits = page.paint(&mut Canvas::new(&mut px, w as u32, h as u32), &mut text, None, Some(&crate::status_corner::Corner::battery_only("64%")), Look::plain());
        (px, hits)
    }

    #[test]
    fn b_goes_back_a_opens_the_games_y_the_sources() {
        let mut page = Page::new(data("dreamcast"));
        assert_eq!(page.press(Button::East), Out::Games);
        assert_eq!(page.press(Button::South), Out::Back);
        assert_eq!(page.press(Button::Start), Out::Emulator, "START: Runs on");
        assert_eq!(page.press(Button::West), Out::None);
        assert_eq!(page.sources, Some(0));
        assert_eq!(page.hints()[0], ("B", "Back"));
        assert_eq!(page.press(Button::East), Out::None, "A does nothing over the sources");
        assert_eq!(page.press(Button::South), Out::None, "B closes the sources, not the page");
        assert!(page.sources.is_none());
        page.press(Button::West);
        page.press(Button::West);
        assert!(page.sources.is_none(), "Y closes them too");
        assert_eq!(page.hints()[..4], [("B", "Back"), ("A", "Games"), ("X", "Credits"), ("Y", "Sources")]);
    }

    fn credits(id: &str) -> CreditsData {
        CreditsData {
            credits: crate::console_credits::for_system(id).unwrap(),
            console_name: "Dreamcast".into(),
            stripe: 0xe2711d,
            same_chip: Vec::new(),
            family_links: vec![credits_page::LinkRow {
                name: "Sega Saturn".into(),
                note: "before".into(),
                stripe: 0x334455,
                link: Link::Console(3),
            }],
            family_text: Vec::new(),
            games: Vec::new(),
            games_more: 0,
            games_ready: true,
        }
    }

    #[test]
    fn x_opens_credits_and_b_or_x_comes_back_to_the_facts() {
        let mut page = Page::new(data("dreamcast"));
        assert_eq!(page.press(Button::North), Out::Credits, "the App finds the links, then opens them");
        page.open_credits(credits("dreamcast"));
        let (_, hits) = paint(&mut page);
        assert_eq!(page.hints()[0], ("B", "Back"));
        assert!(page.hints().contains(&("A", "Open")), "the one link is on the first screen and selected");
        assert_eq!(hits.rows.len(), 1, "it can be tapped");
        assert_eq!(page.press(Button::East), Out::Open(Link::Console(3)));
        assert_eq!(page.press(Button::South), Out::None, "B closes Credits, not the page");
        assert!(page.credits.is_none());
        page.open_credits(credits("dreamcast"));
        assert_eq!(page.press(Button::North), Out::None);
        assert!(page.credits.is_none(), "X closes it too");
        assert_eq!(page.press(Button::South), Out::Back);
    }

    #[test]
    fn every_console_page_paints_and_scrolls_within_its_length() {
        for con in consoles::all() {
            let mut page = Page::new(data(&con.id));
            let (_, hits) = paint(&mut page);
            assert!(hits.buttons.iter().any(|(_, b)| *b == Button::West), "{}: Y can be tapped", con.id);
            for _ in 0..40 {
                page.press(Button::R1);
            }
            assert_eq!(page.scroll, page.max_scroll, "{}", con.id);
            paint(&mut page);
            page.press(Button::West);
            let (_, hits) = paint(&mut page);
            let (row, rows, _) = hits.scroll.unwrap();
            assert!(row < rows, "{}", con.id);
        }
    }

    #[test]
    fn the_stripe_runs_down_the_left_edge() {
        let mut page = Page::new(data("dreamcast"));
        let (px, _) = paint(&mut page);
        for y in [0, 540, 1079] {
            assert_eq!(px[y * 1240 + 4], 0xe2711d, "row {y}");
        }
    }

    #[test]
    fn long_addresses_break_into_lines_that_fit() {
        let mut text = Text::new();
        let url = "en.wikipedia.org/w/api.php?action=parse&page=Atari_2600&prop=wikitext&section=0&format=json";
        let lines = wrap_url(&mut text, url, 400);
        assert!(lines.len() > 1);
        assert_eq!(lines.concat(), url, "nothing lost or added");
        for l in &lines {
            assert!(text.width(Face::Body, 24, l) <= 400, "{l}");
        }
    }

    #[test]
    fn the_short_facts_keep_to_their_room() {
        let (w, h) = (1240, 1080);
        let mut px = vec![0u32; (w * h) as usize];
        let mut text = Text::new();
        let here = consoles::Here {
            games: 1,
            last: Some("Sonic Adventure 2".into()),
            runs_on: Some("Flycast on pocket-core".into()),
        };
        let rows = consoles::shelf_rows(consoles::for_system("dreamcast").unwrap(), &here);
        let mut c = Canvas::new(&mut px, w as u32, h as u32);
        assert_eq!(
            paint_facts(&mut c, &mut text, &rows, 588, 728, 604, 970, 0x777777, 0xdddddd),
            6,
            "all six in the Split look"
        );
        assert_eq!(paint_facts(&mut c, &mut text, &rows, 588, 728, 604, 830, 0x777777, 0xdddddd), 3);
    }
}
