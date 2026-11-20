//! Painting the photo album in a game's case (`album`): the case lies open (its inside as PixelLab pixel art,
//! `assets/media/album`, SOURCES.md there), the instant photos lie in it fanned out, the chosen one on top; A holds it up
//! close with its note written on its white strip; X asks before one goes. An empty case says why in a sentence.
//!
//! Everything lies on the case (`album::layout`, checked by `tests/scene_rests.rs`): no photo floats beside it. The one
//! held up close is the one exception, held in the hand over the dimmed case.

use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Instant;

use crate::album::{self, AlbumData, AlbumView, Mode, Photo};
use crate::cabinet_art::{self as art, Pic};
use crate::gfx::{mix, Canvas};
use crate::image::Image;
use crate::palette::*;
use crate::text::{Face, Text};

const POLAROID: &[u8] = include_bytes!("../assets/media/album/polaroid.png");
const PLASTIC: &[u8] = include_bytes!("../assets/media/album/case-plastic.png");
const CARDBOARD: &[u8] = include_bytes!("../assets/media/album/case-cardboard.png");

use crate::album::{FRAME, WINDOW};
/// The inside of the case's art.
pub const CASE: (i32, i32) = (224, 154);

/// Which inside the case has: a cardboard box's (cartridges) or a plastic case's (discs, cards).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Inside {
    Cardboard,
    Plastic,
}

fn decode(bytes: &[u8]) -> Pic {
    match crate::system_art::decode(bytes) {
        Some(s) => Pic::of(s.w as i32, s.h as i32, s.px),
        None => Pic::new(1, 1, 0xff80_8080),
    }
}

fn frame_pic() -> &'static Pic {
    static P: std::sync::OnceLock<Pic> = std::sync::OnceLock::new();
    P.get_or_init(|| decode(POLAROID))
}

fn inside_pic(inside: Inside) -> &'static Pic {
    static A: std::sync::OnceLock<Pic> = std::sync::OnceLock::new();
    static B: std::sync::OnceLock<Pic> = std::sync::OnceLock::new();
    match inside {
        Inside::Cardboard => A.get_or_init(|| decode(CARDBOARD)),
        Inside::Plastic => B.get_or_init(|| decode(PLASTIC)),
    }
}

/// The decoded art, for tests (it must decode, at its size).
pub fn art_sizes() -> [(i32, i32); 3] {
    let f = frame_pic();
    let (a, b) = (inside_pic(Inside::Cardboard), inside_pic(Inside::Plastic));
    [(f.w, f.h), (a.w, a.h), (b.w, b.h)]
}

/// What the painter keeps between frames: the photos made up as instant photos at a size (frame, picture in its window).
#[derive(Default)]
pub struct Made {
    pics: HashMap<(PathBuf, i32), Pic>,
}

impl Made {
    /// The photo as an instant photo, `s` times the frame's art: the game's picture filling its window (cut to the
    /// window's shape, as a camera's square frame cuts), a grey window while the picture isn't decoded yet.
    fn get(&mut self, photo: &Photo, s: i32, picture: Option<&Image>) -> &Pic {
        let key = (photo.file.clone(), s);
        // A picture that arrived since its grey stand-in was made replaces it.
        if picture.is_some() && self.pics.get(&key).is_some_and(|p| p.px.first() == Some(&0)) {
            self.pics.remove(&key);
        }
        if self.pics.len() > 24 {
            self.pics.clear();
        }
        self.pics.entry(key).or_insert_with(|| make_photo(s, photo.size, picture))
    }
}

/// The instant photo at `s` times its art. The first pixel is a marker: 0 while the picture is missing (see `get`).
pub fn make_photo(s: i32, size: (u32, u32), picture: Option<&Image>) -> Pic {
    let frame = shaped_frame(size);
    let mut p = frame.scaled(frame.w * s, frame.h * s);
    let (wx, wy, ww, wh) = (WINDOW.0 * s, WINDOW.1 * s, crate::album::window_w(size) * s, WINDOW.3 * s);
    match picture.filter(|i| i.w > 0 && i.h > 0) {
        Some(img) => {
            // Cover the window: the picture's middle, as wide as the window's shape allows.
            let (iw, ih) = (img.w as f32, img.h as f32);
            let scale = (ww as f32 / iw).max(wh as f32 / ih);
            let (ox, oy) = ((iw * scale - ww as f32) / 2.0, (ih * scale - wh as f32) / 2.0);
            for y in 0..wh {
                let sy = (((y as f32 + 0.5 + oy) / scale) as u32).min(img.h - 1);
                for x in 0..ww {
                    let sx = (((x as f32 + 0.5 + ox) / scale) as u32).min(img.w - 1);
                    let v = img.px[(sy * img.w + sx) as usize] & 0xff_ffff;
                    p.px[((wy + y) * p.w + wx + x) as usize] = 0xff00_0000 | v;
                }
            }
            // A soft inner edge, as the print sits a little under the frame.
            for x in 0..ww {
                let i = (wy * p.w + wx + x) as usize;
                p.px[i] = 0xff00_0000 | mix(p.px[i] & 0xff_ffff, 0x000000, 90);
            }
            for y in 0..wh {
                let i = ((wy + y) * p.w + wx) as usize;
                p.px[i] = 0xff00_0000 | mix(p.px[i] & 0xff_ffff, 0x000000, 70);
            }
        }
        None => p.fill(wx, wy, ww, wh, 0xff3a_3a3e),
    }
    if picture.is_none() {
        // The marker: the top left corner is see-through anyway on a real print's rounded corner.
        p.px[0] = 0;
    } else if p.px[0] == 0 {
        p.px[0] = 1;
    }
    p.changed();
    p
}

/// The frame for a picture of `size` at the art's scale: its sides as drawn, the part over and under the window
/// stretched or squeezed to the window's width (nearest column, so the paper's grain stays pixel art).
pub fn shaped_frame(size: (u32, u32)) -> Pic {
    let f = frame_pic();
    let ww = crate::album::window_w(size);
    let fw = crate::album::frame_w(size);
    if ww == WINDOW.2 {
        return f.clone();
    }
    let right = f.w - WINDOW.0 - WINDOW.2;
    let mut out = Pic::new(fw, f.h, 0);
    for x in 0..fw {
        let sx = if x < WINDOW.0 {
            x
        } else if x >= fw - right {
            f.w - (fw - x)
        } else {
            WINDOW.0 + ((x - WINDOW.0) * WINDOW.2 / ww).min(WINDOW.2 - 1)
        };
        for y in 0..f.h {
            out.px[(y * fw + x) as usize] = f.px[(y * f.w + sx) as usize];
        }
    }
    out.changed();
    out
}

/// How big things are: the case's art times `k`, a photo's frame times `s`.
pub struct Plan {
    pub case: (i32, i32, i32, i32),
    pub k: i32,
    pub s: i32,
    /// The photo's height as a share of the case's (`album::layout`).
    pub photo_h: f32,
    pub caption_y: i32,
}

/// Where the open case goes in `room` (x, y, w, h), with a line under it for the chosen photo's note.
pub fn plan(room: (i32, i32, i32, i32), caption_h: i32) -> Plan {
    let (x, y, w, h) = room;
    let avail_h = (h - caption_h).max(CASE.1);
    let k = (w / CASE.0).min(avail_h / CASE.1).max(1);
    let (cw, ch) = (CASE.0 * k, CASE.1 * k);
    let cx = x + (w - cw) / 2;
    let cy = y + (avail_h - ch) / 2;
    // A real instant photo is about 0.56 of a DVD case's height: the nearest whole multiple of the frame's art.
    let s = ((ch as f32 * 0.6 / FRAME.1 as f32).round() as i32).max(1);
    let photo_h = (s * FRAME.1) as f32 / ch as f32;
    Plan { case: (cx, cy, cw, ch), k, s, photo_h, caption_y: cy + ch }
}

/// Paints the open case in `room` (the box's room under the card's top line).
#[allow(clippy::too_many_arguments)]
pub fn paint(c: &mut Canvas, text: &mut Text, view: &AlbumView, data: &AlbumData, inside: Inside, tint: Option<u32>, pictures: &HashMap<PathBuf, Option<Rc<Image>>>, made: &mut Made, room: (i32, i32, i32, i32), now: Instant) {
    let caption_px = crate::type_scale::px(22);
    let caption_h = caption_px as i32 * 3;
    let p = plan(room, caption_h);
    let (cx, cy, cw, ch) = p.case;
    let open = crate::cabinet::ease_out(view.openness(now));
    // The case comes up from a little lower and lies down open.
    let lift = ((1.0 - open) * ch as f32 * 0.12) as i32;
    let clip = (0, c.h);
    art::soft_shadow(c, cx + 10, cy + 14 + lift, cw, ch, 24, 0.55 * open, clip);
    let inside_art = inside_pic(inside);
    let shown = match tint {
        // A coloured plastic (a Wii's white, an Xbox's green-black): the grey art takes the system's colour.
        Some(t) if inside == Inside::Plastic => tinted(inside_art, t),
        _ => inside_art.clone(),
    };
    art::blit_scaled(c, &shown, cx, cy + lift, cw, ch, 255, clip);
    let photos = &data.photos;
    if photos.is_empty() {
        paint_empty(c, text, data, p.case, lift);
        return;
    }
    let aspect = cw as f32 / ch as f32;
    let (t, way) = view.slide(now);
    let from = view.sel as f32 - way as f32 * (1.0 - crate::cabinet::ease_out(t));
    let placed = album::layout_at(photos, from, aspect, p.photo_h);
    for pl in &placed {
        let photo = &photos[pl.index];
        let pic = pictures.get(&photo.file).and_then(|p| p.as_deref());
        let pc = made.get(photo, p.s, pic).clone();
        let (x, y) = (cx as f32 + pl.cx * cw as f32, (cy + lift) as f32 + pl.cy * ch as f32);
        let a = pl.deg.to_radians();
        // Its shadow on the case, a little down and right, then the photo.
        art::blit_pose(c, &pc, x + p.s as f32 * 1.5, y + p.s as f32 * 2.0, 1.0, a, 70, Some(0x000000), clip);
        art::blit_pose(c, &pc, x, y, 1.0, a, 255, None, clip);
        // Taken since the case was last opened: a small "New" tab on its top right corner.
        if photo.at > data.seen && view.mode == Mode::Case {
            let tr = album::corners(pl, photo, aspect, p.photo_h)[1];
            new_tab(c, text, (cx as f32 + tr.0 * cw as f32, (cy + lift) as f32 + tr.1 * ch as f32), p.s);
        }
        // For the navigation explorer: each photo is something Left and Right reach; the one on top has the focus.
        if crate::nav_probe::on() {
            let (hw, hh) = (pc.w / 2, pc.h / 2);
            let rect = (x as i32 - hw, y as i32 - hh, pc.w, pc.h);
            crate::nav_probe::row(c, rect);
            if pl.index == view.sel.min(photos.len() - 1) {
                crate::nav_probe::focus(c, rect);
            }
        }
    }
    if view.mode == Mode::Case || matches!(view.mode, Mode::Delete { from_close: false }) {
        paint_caption(c, text, &photos[view.sel.min(photos.len() - 1)], view.sel.min(photos.len() - 1), photos.len(), (room.0, p.caption_y + caption_px as i32 / 2, room.2), caption_px);
    }
    if matches!(view.mode, Mode::Close | Mode::Delete { from_close: true }) {
        paint_close(c, text, &photos[view.sel.min(photos.len() - 1)], pictures, room);
    }
    if let Mode::Delete { .. } = view.mode {
        paint_question(c, text, room);
    }
}

/// The small "New" tab at a photo's corner `(x, y)`: a strip of red paper with the word, pinned over the corner, its
/// middle on the corner (it lies on the photo, never beside it in the air).
fn new_tab(c: &mut Canvas, text: &mut Text, (x, y): (f32, f32), s: i32) {
    let px = crate::type_scale::px(14).max(10 + 2 * s as u32);
    let word = crate::i18n::tr("New");
    let tw = text.literal(|t| t.width(Face::Body, px, &word));
    let (w, h) = (tw + px as i32, px as i32 * 3 / 2);
    let (bx, by) = (x as i32 - w / 2 - s * 4, y as i32 - h / 2 + s * 3);
    c.fill_rects_at(&[(bx + 2, by + 3, w, h)], 0x000000, 70);
    c.round_rect(bx, by, w, h, h / 4, 0xd8483c);
    text.literal(|t| t.draw(c, Face::Body, px, bx + (w - tw) / 2, by + h / 2 + px as i32 * 3 / 8, 0xfff4ea, &word));
}

/// The grey art in the system's plastic colour: its light kept, its hue the colour's.
fn tinted(p: &Pic, color: u32) -> Pic {
    let mut out = p.clone();
    for v in out.px.iter_mut() {
        if *v >> 24 == 0 {
            continue;
        }
        let l = ((*v >> 16 & 0xff) * 3 + (*v >> 8 & 0xff) * 5 + (*v & 0xff) * 2) / 10;
        let ch = |sh: u32| ((color >> sh & 0xff) * (l + 40) / 170).min(255);
        *v = (*v & 0xff00_0000) | ch(16) << 16 | ch(8) << 8 | ch(0);
    }
    out.changed();
    out
}

/// The note under the case: what happened and when, and where in the album ("3 of 7").
fn paint_caption(c: &mut Canvas, text: &mut Text, photo: &Photo, i: usize, n: usize, (x, y, w): (i32, i32, i32), px: u32) {
    let ink = pal().text;
    let place = format!("{} of {}", i + 1, n);
    let pw = text.width(Face::Body, px, &place);
    let base = y + px as i32;
    // The note is in the interface's language already (`caption`): fitted and painted as it is.
    let line = caption(photo);
    text.literal(|text| {
        let line = text.fit(Face::Body, px, &line, w - pw - px as i32 * 2);
        text.draw(c, Face::Body, px, x, base, ink, &line);
    });
    text.draw(c, Face::Body, px, x + w - pw, base, pal().soft, &place);
}

/// "Badge 3 · 4 h 12 min · 18 Nov 2026": the note and the day, in the interface's language.
pub fn caption(photo: &Photo) -> String {
    let mut line = photo.note_line();
    let day = date(photo.at);
    if !line.is_empty() {
        line.push_str(" · ");
    }
    line.push_str(&crate::i18n::tr(&day));
    line
}

const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/// The day a photo was taken, local time ("18 Nov 2026", as the gallery writes it).
pub fn date(at: i64) -> String {
    date_in(at, crate::timezone::utc_offset_now(at))
}

/// [`date`] in a zone `tz` seconds east of UTC.
pub fn date_in(at: i64, tz: i32) -> String {
    let (y, m, d) = crate::civil_date::civil(crate::civil_date::local_day(at.max(0) as u64, tz));
    format!("{d} {} {y}", MONTHS[(m as usize).clamp(1, 12) - 1])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Lead 18.11.2026: the prototype's notes said "18 Nov 2025" (its made-up photos were a year old). The day
    /// itself: the right year, and the device's zone decides the day at a year's end.
    #[test]
    fn a_photos_day_is_the_local_day_with_its_year() {
        assert_eq!(date_in(1_795_003_200, 0), "18 Nov 2026"); // 2026-11-18 12:00 UTC
        assert_eq!(date_in(1_798_759_800, 0), "31 Dec 2026"); // 2026-12-31 23:30 UTC
        assert_eq!(date_in(1_798_759_800, 3600), "1 Jan 2027", "Vienna is already in the new year");
        assert_eq!(date_in(1_798_759_800 + 3600, -5 * 3600), "31 Dec 2026", "New York still in the old one");
    }

    #[test]
    fn the_art_decodes_at_its_size() {
        assert_eq!(art_sizes(), [(84, 99), (224, 154), (224, 154)]);
    }
}

/// An empty case: the sentence in the middle of its left half (the lid's inside), on a paper note lying there.
fn paint_empty(c: &mut Canvas, text: &mut Text, data: &AlbumData, (cx, cy, cw, ch): (i32, i32, i32, i32), lift: i32) {
    let px = crate::type_scale::px(22);
    let (nw, nh) = (cw * 5 / 6, ch * 2 / 5);
    let (nx, ny) = (cx + (cw - nw) / 2, cy + lift + (ch - nh) / 2);
    // A slip of paper lying on the case, its shadow under it.
    art::soft_shadow(c, nx + 6, ny + 8, nw, nh, 10, 0.45, (0, c.h));
    c.rect(nx, ny, nw, nh, 0xf2ecdc);
    let lines = text.wrap_prose(Face::Body, px, data.empty_line(), nw - px as i32 * 2);
    let step = px as i32 * 13 / 10;
    let mut y = ny + (nh - step * lines.len() as i32) / 2 + px as i32;
    // The lines are the sentence already translated and wrapped: painted as they are, not looked up again.
    text.literal(|text| {
        for l in lines {
            let lw = text.width(Face::Body, px, &l);
            text.draw(c, Face::Body, px, nx + (nw - lw) / 2, y, 0x2a2a30, &l);
            y += step;
        }
    });
}

/// One photo held up close: the case dimmed under it, the photo straight and large, its note written on its strip.
fn paint_close(c: &mut Canvas, text: &mut Text, photo: &Photo, pictures: &HashMap<PathBuf, Option<Rc<Image>>>, (x, y, w, h): (i32, i32, i32, i32)) {
    c.darken_rect(0, y - 8, c.w, h + 16, 150);
    let s = (h * 94 / 100 / FRAME.1).min(w * 9 / 10 / crate::album::frame_w(photo.size)).max(1);
    let pic = pictures.get(&photo.file).and_then(|p| p.as_deref());
    let mut p = make_photo(s, photo.size, pic);
    if p.px[0] == 0 {
        p.px[0] = p.px[1];
    }
    // The note on the strip under the picture, in marker, and the day under it.
    let strip_y = (WINDOW.1 + WINDOW.3) * s;
    let strip_h = p.h - strip_y;
    let note = photo.note_line();
    let day = crate::i18n::tr(&date(photo.at));
    let (pad, pw) = (s * 4, p.w);
    if !note.is_empty() {
        crate::tape_label::write_on(text, &mut p, (pad, strip_y + strip_h / 12, pw - 2 * pad, strip_h * 7 / 12), &note, 1);
    }
    crate::tape_label::write_on(text, &mut p, (pw / 3, strip_y + strip_h * 7 / 12, pw - pw / 3 - pad, strip_h * 5 / 12), &day, 1);
    p.changed();
    let (px0, py0) = (x + (w - p.w) / 2, y + (h - p.h) / 2);
    art::soft_shadow(c, px0 + 10, py0 + 14, p.w, p.h, 22, 0.6, (0, c.h));
    art::blit(c, &p, px0, py0, 255, (0, c.h));
}

/// The question before a photo goes, over what is shown.
fn paint_question(c: &mut Canvas, text: &mut Text, (x, y, w, h): (i32, i32, i32, i32)) {
    let px = crate::type_scale::px(26);
    let q = "Delete this photo?";
    let qw = text.width(Face::Body, px, q).min(w - 40);
    let (bw, bh) = (qw + px as i32 * 3, px as i32 * 3);
    let (bx, by) = (x + (w - bw) / 2, y + h - bh - px as i32);
    c.round_rect(bx, by, bw, bh, px as i32 / 2, pal().pill);
    let q = text.fit(Face::Body, px, q, w - 40);
    text.draw(c, Face::Body, px, bx + (bw - qw) / 2, by + bh / 2 + px as i32 * 3 / 8, pal().text, &q);
}
