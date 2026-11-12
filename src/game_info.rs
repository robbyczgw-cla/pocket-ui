//! The game's info page (X on a game in the Minimal, Cards and Split looks; Shelf and Cabinet open the box,
//! `game_box`): everything about the game on one screen, nothing to turn over. The cover at the left (on top on a
//! narrow screen) with the player's own lines under it (status, time played, last played, rating, achievements, a
//! port's speed and update note); at the right the title, the system and year, the facts and description, which
//! share one scroll area with up and down (L1/R1 a page). The cover, title, system and year stay fixed.
//!
//! It reads the same `CardData` as the box's back and shows the same things, in the shelf's calm Chalk and Dust on the
//! page. Whatever is missing gets words of its own ("No description yet.", "Not yet"), never a gap. A plays, B goes
//! back, Y sets the status and rating, X the note, START "This game"; SELECT shows the card's full page (Curator's rows,
//! screenshots, Hide or delete) and back (`game_card`).

use crate::game_card::{lift_shadow, paint_mark, paint_stars, CardData, CardSaves};
use crate::gfx::{mix, Canvas};
use crate::image::Image;
use crate::library_meta::Status;
use crate::palette::*;
use crate::system_art::Sprite;
use crate::text::{Face, Text};
use crate::type_scale;
use std::rc::Rc;

/// The scroll of facts and description; the cover and heading stand still.
#[derive(Default, Debug)]
pub struct InfoView {
    pub scroll: i32,
    max_scroll: i32,
    view_h: i32,
    /// Link rows in the scroll area's unscrolled content: keep a newly selected fact visible.
    link_offsets: Vec<(i32, i32)>,
    /// The fact the selection is on (an index into `CardData::links`): A opens its list of games (Connections). None:
    /// A plays.
    pub link: Option<usize>,
    /// Where each of `CardData::links`' facts was painted last, (x, y, w, h) in the canvas it was painted on: for a
    /// touch screen's taps (the Android app).
    pub link_rects: Vec<(i32, i32, i32, i32)>,
}

/// A value in the player's lines under the cover.
enum Val {
    Text(String),
    /// Something to do or not there yet, in Dust ("None. Press Y").
    Quiet(&'static str),
    Status(Status),
    Stars(u8),
    /// The save slots on their medium (`save_media`), two lines high.
    Medium(CardSaves),
}

impl Val {
    /// Lines it takes.
    fn lines(&self) -> i32 {
        if matches!(self, Val::Medium(_)) {
            2
        } else {
            1
        }
    }
}

/// The save medium as wide as `w` allows and two of the player's lines high.
fn medium_layout(s: &CardSaves, w: i32) -> (crate::save_media::Kind, crate::save_media::Layout) {
    let kind = crate::save_media::Kind::of(&s.system);
    (kind, crate::save_media::Layout::fitted(kind, s.filled.len(), w, 2 * type_scale::row(44, 28) - 12))
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
    if d.plain {
        return match d.played {
            Some((last, secs)) => vec![("Played", Val::Text(crate::recents::duration(secs))), ("Last", Val::Text(crate::recents::ago(last)))],
            None => vec![("Played", Val::Quiet("Not yet"))],
        };
    }
    // The status first, across the column: its mark says what it is, and "Wird gespielt" needs the room.
    let mut rows = vec![("", Val::Status(d.status))];
    rows.push(match d.played {
        Some((_, secs)) => ("Played", Val::Text(crate::recents::duration(secs))),
        None => ("Played", Val::Quiet("Not yet")),
    });
    if let Some((last, _)) = d.played {
        rows.push(("Last", Val::Text(crate::recents::ago(last))));
    }
    rows.push(("Rating", if d.rating > 0 { Val::Stars(d.rating) } else { Val::Quiet(crate::game_card_model::no_rating()) }));
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
    // The save slots (pocket-core's games): how many hold a save, and the slots on the game's save medium.
    if let Some(saves) = &d.saves {
        let n = saves.filled.iter().filter(|f| **f).count();
        rows.push(("Save slots", if n > 0 { Val::Text(format!("{n} of {}", saves.filled.len())) } else { Val::Quiet("All empty") }));
        rows.push(("", Val::Medium(saves.clone())));
    }
    if !d.note.is_empty() {
        rows.push(("Note", Val::Text(format!("“{}”", d.note))));
    }
    rows
}

/// The facts at the right: the box back's, year and system aside (they are in the line under the title).
fn facts(d: &CardData) -> Vec<(&'static str, String)> {
    if d.plain {
        // Where the sentence is English only and none is on hand (`game_box::about`): the genre says what kind of game it is.
        let no_sentence = crate::game_box::about(d).0.is_none();
        let genre = d.facts.iter().find(|(k, _)| *k == "Genre").filter(|_| no_sentence).cloned();
        return genre.into_iter().chain(d.players.clone().map(|p| ("Players", p))).collect();
    }
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
    let rows: i32 = your_rows(d).iter().map(|(_, v)| v.lines()).sum();
    let under = rows * type_scale::row(44, 28) + 24;
    let cw = left_w(w, side);
    (cw, (bottom - top - under).clamp(120, cw * 3 / 2))
}

impl InfoView {
    /// Down on the page: onto the next fact that leads somewhere (of `n`), then the description scrolls; Up back
    /// (the description to its top first), off the first fact to none. False: not a move between facts.
    pub fn link_step(&mut self, down: bool, n: usize) -> bool {
        match (down, self.link) {
            (true, None) if n > 0 => self.link = Some(0),
            (true, Some(i)) if i + 1 < n => self.link = Some(i + 1),
            (false, Some(i)) if self.scroll == 0 => self.link = i.checked_sub(1),
            _ => return false,
        }
        if let Some(&(top, h)) = self.link.and_then(|i| self.link_offsets.get(i)) {
            if top < self.scroll {
                self.scroll = top.max(0);
            } else if top + h > self.scroll + self.view_h {
                self.scroll = (top + h - self.view_h).clamp(0, self.max_scroll);
            }
        }
        true
    }

    pub fn scroll_by(&mut self, d: i32) {
        self.scroll = (self.scroll + d).clamp(0, self.max_scroll);
    }

    pub fn scrollable(&self) -> bool {
        self.max_scroll > 0
    }

    /// Paints the page under the top line (the status corner is the card's); `cover` is fitted into [`cover_room`],
    /// `console` the system's picture for a game without one.
    pub fn paint(&mut self, c: &mut Canvas, text: &mut Text, d: &CardData, cover: Option<&Rc<Image>>, console: Option<&Sprite>) {
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
        let your = your_rows(d);
        // The player's lines: under the cover on a wide screen, under the facts on a narrow one.
        let (tx, mut ty, tw) = if narrow(c.w) {
            (side, top + fh + 24, c.w - 2 * side)
        } else {
            // Beside the cover as it came out (a tall box is narrower than its room), or the player's lines if they
            // are wider.
            let rx = side + fw.max(your_w(text, &your)).min(cw) + 48;
            (rx, top, c.w - side - rx)
        };
        let (yx, yw) = if narrow(c.w) { (side, tw) } else { (side, tx - 48 - side) };
        // Reuse the shelf's main/subtitle rule inside the existing title band. The cover, metadata
        // and facts keep their old positions, including headings whose old wrap used two lines.
        let title_step = type_scale::line(50, 6);
        ty += 44 + type_scale::grow(50);
        let old_lines = text.literal(|text| text.wrap(Face::Title, 50, &d.title, tw).len().clamp(1, 2));
        let meta_y = ty + title_step * (old_lines as i32 - 1) + type_scale::line(28, 22);
        let title_top = ty - type_scale::px(50) as i32;
        let room = meta_y - type_scale::px(28) as i32 - 12 - title_top;
        let lines = crate::title_block::name_layout_in(text, Face::Title, 50, &d.title, tw, room);
        let first = title_top + type_scale::px(lines.first().map_or(50, |l| l.px)) as i32;
        crate::title_block::paint(text, c, Face::Title, &lines, tx, first, pal().chalk, pal().dust, false);
        ty = meta_y;
        let mut meta = vec![d.system.clone()];
        meta.extend(d.year.map(|y| y.to_string()));
        // Retro's 28 and 24 share a pixel grid at Large: let the Favourite mark take the next visible step before losing the year.
        let fav_px = if d.favorite && text.literal(|text| text.width(Face::Body, 24, &meta.join(" · "))) + text.width(Face::Body, 28, "Favourite") + 24 > tw { 21 } else { 28 };
        let fav_w = if d.favorite { text.width(Face::Body, fav_px, "Favourite") + 24 } else { 0 };
        let sizes: &[u32] = if crate::retro::on() { &[28, 24, 21] } else { &[28, 24] };
        let room = tw - fav_w;
        let least = *sizes.last().unwrap();
        if text.literal(|text| text.width(Face::Body, least, &meta.join(" · "))) > room {
            // Use the console's existing short name before cutting its label; never lose the game's year.
            #[cfg(not(target_os = "android"))]
            if let Some(console) = crate::consoles::all().iter().find(|c| c.name == d.system || c.aka.contains(&d.system)) {
                if let Some(short) = console.aka.iter().filter(|name| name.len() < d.system.len()).min_by_key(|name| name.len()) { meta[0] = short.clone(); }
            }
            if let Some(year) = d.year {
                let suffix = format!(" · {year}");
                let name_room = room - text.width(Face::Body, least, &suffix);
                meta[0] = text.literal(|text| text.fit(Face::Body, least, &meta[0], name_room));
            }
        }
        let mw = text.literal(|text| {
            let (meta_px, meta) = text.fit_sizes(Face::Body, sizes, &meta.join(" · "), room);
            text.draw(c, Face::Body, meta_px, tx, ty, pal().dust, &meta)
        });
        if d.favorite {
            text.draw(c, Face::Body, fav_px, tx + mw + 24, ty, pal().amber, "Favourite");
        }
        ty += row;
        if let Some(why) = &d.broken {
            ty = crate::game_card::paint_problem(c, text, why, (tx, ty, tw));
        }
        // Keep the title/year and cover fixed; facts and description share the room below the metadata.
        let dtop = (ty - type_scale::px(28) as i32).clamp(0, bottom);
        let dbot = bottom.clamp(dtop, c.h);
        let view_h = dbot - dtop;
        self.view_h = view_h;
        if view_h <= 0 {
            return;
        }
        if !narrow(c.w) {
            paint_your(c, text, &your, cover, (yx, yw), top + fh + 24 + 30 + type_scale::grow(28));
        }
        let height = {
            let mut body = c.rows(dtop, dbot);
            let mut y = self.paint_facts(&mut body, text, d, (tx, tw), ty - dtop - self.scroll);
            if narrow(body.w) {
                y = paint_your(&mut body, text, &your, cover, (yx, yw), y);
            }
            let about_y = y - row + 60 + type_scale::grow(28);
            about_y + paint_about(&mut body, text, d, (tx, tw - 16), about_y) + self.scroll
        };
        self.max_scroll = (height - view_h).max(0);
        self.scroll = self.scroll.min(self.max_scroll);
        self.link_offsets = self.link_rects.iter().map(|&(_, y, _, h)| (y + self.scroll, h)).collect();
        // A tap only opens the visible part of a link, never a row clipped behind the fixed heading.
        for (_, y, _, h) in &mut self.link_rects {
            let end = (*y + *h).min(view_h);
            *y = (*y).max(0);
            *h = (end - *y).max(0);
            *y += dtop;
        }
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

    /// Facts in the scrolling column, from baseline `ty`; returns the baseline after the last row.
    fn paint_facts(&mut self, c: &mut Canvas, text: &mut Text, d: &CardData, (tx, tw): (i32, i32), mut ty: i32) -> i32 {
        let row = type_scale::row(44, 28);
        // The facts, one line each, the key in Dust.
        let facts = facts(d);
        self.link_rects = vec![(0, 0, 0, 0); d.links.len()];
        if facts.is_empty() && d.plain {
            // Nothing known: nothing said.
        } else if facts.is_empty() {
            for l in text.wrap_prose_at_most(Face::Body, 28, "Not in the offline game list, so there are no facts to show.", tw, 2) {
                text.draw(c, Face::Body, 28, tx, ty, pal().dust, &l);
                ty += type_scale::line(28, 8);
            }
            ty += row - type_scale::line(28, 8);
        } else {
            let key_w = keys_w(text, facts.iter().map(|(k, _)| *k), tw / 2);
            for (i, (k, v)) in facts.iter().enumerate() {
                // A key wider than its column (German at Large) is shortened, never run into its value.
                let (px, k) = text.fit_sizes(Face::Body, &[28, 24], k, key_w - 24);
                text.draw(c, Face::Body, px, tx, ty, pal().dust, &k);
                // A developer, series or genre the player has other games of: an arrow after it, a pill when selected.
                match d.links.iter().position(|(f, _)| *f == i) {
                    Some(l) => {
                        let colors = (pal().chalk, pal().dust, pal().select, pal().ink);
                        // As a plain value: one that doesn't fit goes on a second line (the arrow and the pill on the last).
                        let room = tw - key_w - type_scale::wide(10, 28) - 26;
                        let lines = text.wrap_prose_at_most(Face::Body, 28, v, room, 2);
                        if let [first, last] = lines.as_slice() {
                            let step = type_scale::line(28, 8);
                            text.draw(c, Face::Body, 28, tx + key_w, ty, pal().chalk, first);
                            ty += step;
                            self.link_rects[l] = (tx + key_w - 14, ty - row * 3 / 4, tw - key_w + 14, row);
                            crate::kit::link_value(c, text, (tx + key_w, ty, tw - key_w), last, self.link == Some(l), colors);
                        } else {
                            self.link_rects[l] = (tx + key_w - 14, ty - row * 3 / 4, tw - key_w + 14, row);
                            crate::kit::link_value(c, text, (tx + key_w, ty, tw - key_w), v, self.link == Some(l), colors);
                        }
                    }
                    None => {
                        // A long value ("Nintendo Research & Development 1, Intelligent Systems") takes a second line
                        // rather than ending in "…" (new-user test 2026-10-30); only the second may be cut.
                        let lines = text.wrap_prose_at_most(Face::Body, 28, v, tw - key_w, 2);
                        for (n, line) in lines.iter().enumerate() {
                            text.draw(c, Face::Body, 28, tx + key_w, ty + n as i32 * type_scale::line(28, 8), pal().chalk, line);
                        }
                        ty += (lines.len().max(1) as i32 - 1) * type_scale::line(28, 8);
                    }
                }
                ty += row;
            }
        }
        ty
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
                // As wide as it comes out in a column of the room's width (never wider than that).
                Val::Medium(s) => medium_layout(s, 430).1.w,
            }
        })
        .max()
        .unwrap_or(0)
}

/// The player's lines from baseline `y` in the column (x, w); returns the baseline after them.
fn paint_your(c: &mut Canvas, text: &mut Text, rows: &[(&'static str, Val)], cover: Option<&Rc<Image>>, (x, w): (i32, i32), mut y: i32) -> i32 {
    let row = type_scale::row(44, 28);
    let key_w = keys_w(text, rows.iter().map(|(k, _)| *k), w / 2);
    // Retro's 28/24 can share a pixel grid: 21 is then the next visibly smaller step. Keep the value column intact.
    let sizes: &[u32] = if crate::retro::on() { &[28, 24, 21] } else { &[28, 24] };
    for (k, v) in rows {
        // A line without a key (a port's notes) runs under the value before it, across the column.
        let (vx, vw) = if k.is_empty() { (x, w) } else { (x + key_w, w - key_w) };
        if !k.is_empty() {
            let (px, k) = text.fit_sizes(Face::Body, sizes, k, key_w - 20);
            text.draw(c, Face::Body, px, x, y, pal().dust, &k);
        }
        match v {
            Val::Text(s) => {
                let s = text.fit(Face::Body, 28, s, vw);
                text.draw(c, Face::Body, 28, vx, y, pal().chalk, &s);
            }
            Val::Quiet(s) => {
                // Keep the rating's action whole even when Retro's next size shares the same pixel grid.
                let source = if *s == "None. Press Y" && text.width(Face::Body, 24, s) > vw { "None · Y" } else { s };
                let (px, s) = text.fit_sizes(Face::Body, sizes, source, vw);
                text.draw(c, Face::Body, px, vx, y, pal().dust, &s);
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
            // Read-only: no slot ringed; the cover in the slots that hold a save (the system's colour without one).
            Val::Medium(s) => {
                let (kind, l) = medium_layout(s, vw);
                let icon = cover.map_or(crate::save_media::Icon::None, crate::save_media::Icon::Cover);
                let slots = crate::save_media::Slots { filled: &s.filled, ring: None, icon, stripe: s.stripe, numbers: l.block >= 20 };
                let top = y - type_scale::mid(28) - row / 2 + 6;
                crate::save_media::paint(c, text, kind, vx, top, &l, &slots);
                // When the newest was saved, beside it (left out where there is no room).
                if let Some(at) = s.newest.filter(|_| s.filled.contains(&true)) {
                    let ago = crate::recents::ago(at);
                    let room = vw - l.w - 24;
                    if text.width(Face::Body, 28, &ago) <= room {
                        text.draw(c, Face::Body, 28, vx + l.w + 24, top + l.h / 2 + type_scale::mid(28), pal().dust, &ago);
                    }
                }
            }
        }
        y += row * v.lines();
    }
    y
}

/// The description and its credit from baseline `y0` (negative when scrolled) in (x, w); returns its height.
fn paint_about(c: &mut Canvas, text: &mut Text, d: &CardData, (x, w): (i32, i32), y0: i32) -> i32 {
    let mut y = y0;
    let (about, credit) = crate::game_box::about(d);
    match about {
        Some(about) => {
            for l in text.wrap_prose(Face::Body, 28, &about, w) {
                text.draw(c, Face::Body, 28, x, y, pal().chalk, &l);
                y += type_scale::line(28, 12);
            }
        }
        None if d.plain => {}
        None => {
            let l = text.fit(Face::Body, 28, "No description yet.", w);
            text.draw(c, Face::Body, 28, x, y, pal().dust, &l);
            y += type_scale::line(28, 12);
        }
    }
    if let Some(credit) = credit {
        for l in text.wrap_prose(Face::Body, 24, &credit, w) {
            text.draw(c, Face::Body, 24, x, y, pal().dust, &l);
            y += type_scale::line(24, 8);
        }
    }
    y - y0 + 30
}

/// A game without box art: a printed title card, the console over the name (docs/DESIGN.md, "Fronts without a
/// cover"), on the shelf's spine colour with the system's stripe.
/// The fallback cover when the game has no box art.
pub fn paint_title_card(c: &mut Canvas, text: &mut Text, d: &CardData, console: Option<&Sprite>, (x, y, w, h): (i32, i32, i32, i32)) {
    text.literal(|text| {
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
    });
}
