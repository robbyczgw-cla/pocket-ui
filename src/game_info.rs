//! The game's info page (X on a game in the Minimal, Cards and Split looks; Shelf and Cabinet open the box,
//! `game_box`): everything about the game on one screen, nothing to turn over. The cover at the left (on top on a
//! narrow screen) with the player's own lines under it (status, time played, last played, rating, achievements, a
//! port's speed and update note); at the right the title, the system and year, the facts, and the description, which
//! scrolls in its own place with up and down (L1/R1 a page) when it is longer than the room left for it.
//!
//! It reads the same `CardData` as the box's back and shows the same things, in the shelf's calm Chalk and Dust on the
//! page. Whatever is missing gets words of its own ("No description yet.", "Not yet"), never a gap. A plays, B goes
//! back, Y sets the status and rating, X the note, START "This game"; SELECT shows the card's full page (Curator's rows,
//! screenshots, Hide or delete) and back (`game_card`).

use crate::game_card::{lift_shadow, paint_mark, paint_stars, CardData};
use crate::gfx::{mix, Canvas};
use crate::image::Image;
use crate::library_meta::Status;
use crate::palette::*;
use crate::system_art::Sprite;
use crate::text::{Face, Text};
use crate::type_scale;

/// The scroll of the description; the rest of the page stands still.
#[derive(Default, Debug)]
pub struct InfoView {
    pub scroll: i32,
    max_scroll: i32,
}

/// A value in the player's lines under the cover.
enum Val {
    Text(String),
    /// Something to do or not there yet, in Dust ("None. Press Y").
    Quiet(&'static str),
    Status(Status),
    Stars(u8),
}

/// From the top of the page to the hints: (top, bottom, side margin) of the page's room on a screen `w` x `h`.
fn room(w: i32, h: i32) -> (i32, i32, i32) {
    let (top, bottom, side) = crate::game_box::room(w, h);
    (top, bottom - 8, side)
}

/// The cover goes on top and everything else under it on a screen narrower than this.
fn narrow(w: i32) -> bool {
    w < 900
}

/// The width of the left column (the cover and the player's lines) on a wide screen.
fn left_w(w: i32, side: i32) -> i32 {
    (w - 2 * side) * 40 / 100
}

/// The player's own lines: what the box's back says under "Your game", and a note when there is one.
fn your_rows(d: &CardData) -> Vec<(&'static str, Val)> {
    // The status first, across the column: its mark says what it is, and "Wird gespielt" needs the room.
    let mut rows = vec![("", Val::Status(d.status))];
    rows.push(match d.played {
        Some((_, secs)) => ("Played", Val::Text(crate::recents::duration(secs))),
        None => ("Played", Val::Quiet("Not yet")),
    });
    if let Some((last, _)) = d.played {
        rows.push(("Last", Val::Text(crate::recents::ago(last))));
    }
    rows.push(("Rating", if d.rating > 0 { Val::Stars(d.rating) } else { Val::Quiet("None. Press Y") }));
    if let Some(a) = &d.achievements {
        rows.push(("Unlocked", Val::Text(format!("{a} achievements"))));
    }
    if let Some((speed, hint)) = &d.port {
        rows.push(("Speed", Val::Text(speed.clone())));
        if *hint {
            rows.push(("", Val::Quiet("Busy core? Press START")));
        }
        if d.port_update {
            rows.push(("", Val::Quiet("Update available, in Extras")));
        }
    }
    if !d.note.is_empty() {
        rows.push(("Note", Val::Text(format!("“{}”", d.note))));
    }
    rows
}

/// The facts at the right: the box back's, year and system aside (they are in the line under the title).
fn facts(d: &CardData) -> Vec<(&'static str, String)> {
    let mut f: Vec<(&str, String)> = d.facts.clone();
    f.extend(d.players.clone().map(|p| ("Players", p)));
    f.extend(d.scores.clone().map(|s| ("Reviews", s)));
    f
}

/// Where the cover goes on a screen `w` x `h`: the app fits the box art into this.
pub fn cover_room(w: i32, h: i32, d: &CardData) -> (i32, i32) {
    let (top, bottom, side) = room(w, h);
    if narrow(w) {
        return (w - 2 * side, (bottom - top) * 3 / 10);
    }
    let rows = your_rows(d).len() as i32;
    let under = rows * type_scale::row(44, 28) + 24;
    let cw = left_w(w, side);
    (cw, (bottom - top - under).clamp(120, cw * 3 / 2))
}

impl InfoView {
    pub fn scroll_by(&mut self, d: i32) {
        self.scroll = (self.scroll + d).clamp(0, self.max_scroll);
    }

    pub fn scrollable(&self) -> bool {
        self.max_scroll > 0
    }

    /// Paints the page under the top line (the status corner is the card's); `cover` is fitted into [`cover_room`],
    /// `console` the system's picture for a game without one.
    pub fn paint(&mut self, c: &mut Canvas, text: &mut Text, d: &CardData, cover: Option<&Image>, console: Option<&Sprite>) {
        let (top, bottom, side) = room(c.w, c.h);
        let (cw, ch) = cover_room(c.w, c.h, d);
        let row = type_scale::row(44, 28);
        // The cover, or a printed title card in its place.
        let (fw, fh) = match cover {
            Some(img) => {
                let (iw, ih) = (img.w as i32, img.h as i32);
                c.blit_scaled(img, side, top, iw, ih);
                (iw, ih)
            }
            None => {
                let (fw, fh) = ((ch * 5 / 7).min(cw), ch);
                paint_title_card(c, text, d, console, (side, top, fw, fh));
                (fw, fh)
            }
        };
        lift_shadow(c, side, top + fh, fw, pal().t);
        // The player's lines: under the cover on a wide screen, under the facts on a narrow one.
        let (tx, mut ty, tw) = if narrow(c.w) {
            (side, top + fh + 24, c.w - 2 * side)
        } else {
            // Beside the cover as it came out (a tall box is narrower than its room), or the player's lines if they
            // are wider.
            let rx = side + fw.max(your_w(text, &your_rows(d))).min(cw) + 48;
            (rx, top, c.w - side - rx)
        };
        let your = your_rows(d);
        let (yx, yw) = if narrow(c.w) { (side, tw) } else { (side, tx - 48 - side) };
        // The right column: the title on two lines at most, then the system and year (and the Favourites mark).
        let title_step = type_scale::line(50, 6);
        ty += 44 + type_scale::grow(50);
        let lines = text.wrap(Face::Title, 50, &d.title, tw);
        for (i, line) in lines.iter().take(2).enumerate() {
            let line = if i == 1 && lines.len() > 2 { format!("{line} {}", lines[2..].join(" ")) } else { line.clone() };
            let line = text.fit(Face::Title, 50, &line, tw);
            text.draw(c, Face::Title, 50, tx, ty + i as i32 * title_step, pal().chalk, &line);
        }
        ty += title_step * (lines.len().clamp(1, 2) as i32 - 1) + type_scale::line(28, 22);
        let mut meta = vec![d.system.clone()];
        meta.extend(d.year.map(|y| y.to_string()));
        let fav_w = if d.favorite { text.width(Face::Body, 28, "Favourite") + 24 } else { 0 };
        let meta = text.fit(Face::Body, 28, &meta.join(" · "), tw - fav_w);
        let mw = text.draw(c, Face::Body, 28, tx, ty, pal().dust, &meta);
        if d.favorite {
            text.draw(c, Face::Body, 28, tx + mw + 24, ty, pal().amber, "Favourite");
        }
        ty += row;
        // The facts, one line each, the key in Dust.
        let facts = facts(d);
        if facts.is_empty() {
            for l in text.wrap_at_most(Face::Body, 28, "Not in the offline game list, so there are no facts to show.", tw, 2) {
                text.draw(c, Face::Body, 28, tx, ty, pal().dust, &l);
                ty += type_scale::line(28, 8);
            }
            ty += row - type_scale::line(28, 8);
        } else {
            let key_w = keys_w(text, facts.iter().map(|(k, _)| *k), tw / 2);
            for (k, v) in &facts {
                // A key wider than its column (German at Large) is shortened, never run into its value.
                let k = text.fit(Face::Body, 28, k, key_w - 24);
                text.draw(c, Face::Body, 28, tx, ty, pal().dust, &k);
                let v = text.fit(Face::Body, 28, v, tw - key_w);
                text.draw(c, Face::Body, 28, tx + key_w, ty, pal().chalk, &v);
                ty += row;
            }
        }
        if narrow(c.w) {
            ty = paint_your(c, text, &your, (yx, yw), ty);
        } else {
            paint_your(c, text, &your, (yx, yw), top + fh + 24 + 30 + type_scale::grow(28));
        }
        // The description in the room that is left, scrolling there.
        let dtop = (ty - row + 30).clamp(0, c.h);
        let dbot = bottom.clamp(dtop, c.h);
        let view_h = dbot - dtop;
        if view_h <= 0 {
            return;
        }
        let height = {
            let mut body = c.rows(dtop, dbot);
            paint_about(&mut body, text, d, (tx, tw - 16), 30 + type_scale::grow(28) - self.scroll)
        };
        self.max_scroll = (height - view_h).max(0);
        self.scroll = self.scroll.min(self.max_scroll);
        // Text running under an edge fades out there, as on the card's page; a thin bar says where the view is.
        let fade = (view_h / 6).min(36);
        for k in 0..fade {
            let amount = (255 - k * 255 / fade.max(1)) as u8;
            if self.scroll > 0 {
                c.shade_rect(tx, dtop + k, tw, 1, pal().void, amount);
            }
            if self.scroll < self.max_scroll {
                c.shade_rect(tx, dbot - 1 - k, tw, 1, pal().void, amount);
            }
        }
        if self.max_scroll > 0 {
            let bar_h = (view_h * view_h / (view_h + self.max_scroll)).max(30);
            let bar_y = dtop + (view_h - bar_h) * self.scroll / self.max_scroll.max(1);
            c.rect(tx + tw - 4, dtop, 4, view_h, pal().spine);
            c.rect(tx + tw - 4, bar_y, 4, bar_h, pal().dust);
        }
    }
}

/// How wide a column of `keys` is (the widest and a gap), at most `max`.
fn keys_w<'a>(text: &mut Text, keys: impl Iterator<Item = &'a str>, max: i32) -> i32 {
    keys.map(|k| text.width(Face::Body, 28, k)).max().unwrap_or(0).saturating_add(24).min(max)
}

/// How wide the player's lines are with nothing cut short.
fn your_w(text: &mut Text, rows: &[(&'static str, Val)]) -> i32 {
    let key_w = keys_w(text, rows.iter().map(|(k, _)| *k), i32::MAX);
    rows.iter()
        .map(|(k, v)| {
            let at = if k.is_empty() { 0 } else { key_w };
            at + match v {
                Val::Text(s) => text.width(Face::Body, 28, s),
                Val::Quiet(s) => text.width(Face::Body, 28, s),
                Val::Status(s) => 32 + text.width(Face::Body, 28, s.label()),
                Val::Stars(_) => type_scale::wide(13, 28) * 10,
            }
        })
        .max()
        .unwrap_or(0)
}

/// The player's lines from baseline `y` in the column (x, w); returns the baseline after them.
fn paint_your(c: &mut Canvas, text: &mut Text, rows: &[(&'static str, Val)], (x, w): (i32, i32), mut y: i32) -> i32 {
    let row = type_scale::row(44, 28);
    let key_w = keys_w(text, rows.iter().map(|(k, _)| *k), w / 2);
    for (k, v) in rows {
        // A line without a key (a port's notes) runs under the value before it, across the column.
        let (vx, vw) = if k.is_empty() { (x, w) } else { (x + key_w, w - key_w) };
        if !k.is_empty() {
            let k = text.fit(Face::Body, 28, k, key_w - 24);
            text.draw(c, Face::Body, 28, x, y, pal().dust, &k);
        }
        match v {
            Val::Text(s) => {
                let s = text.fit(Face::Body, 28, s, vw);
                text.draw(c, Face::Body, 28, vx, y, pal().chalk, &s);
            }
            Val::Quiet(s) => {
                let s = text.fit(Face::Body, 28, s, vw);
                text.draw(c, Face::Body, 28, vx, y, pal().dust, &s);
            }
            Val::Status(s) => {
                let mut sx = vx;
                if *s != Status::NotStarted {
                    paint_mark(c, sx + 10, y - type_scale::mid(28), *s, pal().chalk);
                    sx += 32;
                }
                let l = text.fit(Face::Body, 28, s.label(), vx + vw - sx);
                text.draw(c, Face::Body, 28, sx, y, pal().chalk, &l);
            }
            Val::Stars(n) => {
                let r = type_scale::wide(13, 28);
                paint_stars(c, vx + r, y - type_scale::mid(28), r, *n, pal().chalk, mix(pal().dust, pal().void, 150));
            }
        }
        y += row;
    }
    y
}

/// The description and its credit from baseline `y0` (negative when scrolled) in (x, w); returns its height.
fn paint_about(c: &mut Canvas, text: &mut Text, d: &CardData, (x, w): (i32, i32), y0: i32) -> i32 {
    let mut y = y0;
    let (about, credit) = crate::game_box::about(d);
    match about {
        Some(about) => {
            for l in text.wrap(Face::Body, 28, &about, w) {
                text.draw(c, Face::Body, 28, x, y, pal().chalk, &l);
                y += type_scale::line(28, 12);
            }
        }
        None => {
            let l = text.fit(Face::Body, 28, "No description yet.", w);
            text.draw(c, Face::Body, 28, x, y, pal().dust, &l);
            y += type_scale::line(28, 12);
        }
    }
    if let Some(credit) = credit {
        for l in text.wrap(Face::Body, 24, &credit, w) {
            text.draw(c, Face::Body, 24, x, y, pal().dust, &l);
            y += type_scale::line(24, 8);
        }
    }
    y - y0 + 30
}

/// A game without box art: a printed title card, the console over the name (docs/DESIGN.md, "Fronts without a
/// cover"), on the shelf's spine colour with the system's stripe.
fn paint_title_card(c: &mut Canvas, text: &mut Text, d: &CardData, console: Option<&Sprite>, (x, y, w, h): (i32, i32, i32, i32)) {
    c.rect(x, y, w, h, pal().spine);
    c.rect(x, y, w, 12, d.stripe);
    let pad = 18;
    let lines = text.wrap_at_most(Face::Title, 37, &d.title, w - 2 * pad, 2);
    let step = type_scale::line(37, 6);
    let mut cy = y + h / 2;
    if let Some(s) = console {
        let s = s.fitted(w * 3 / 5, h / 3);
        c.blit_sprite(&s, x + (w - s.w as i32) / 2, y + h / 2 - s.h as i32 - 12, 0);
        cy = y + h / 2 + 24;
    }
    for (i, l) in lines.iter().enumerate() {
        let lw = text.width(Face::Title, 37, l);
        text.draw(c, Face::Title, 37, x + (w - lw) / 2, cy + type_scale::grow(37) + 24 + i as i32 * step, pal().chalk, l);
    }
}
