//! Stickers on a game's case (0.1.4): the collector's marks "Completed", "100 %", "Favourite" (a star),
//! "Playing now" and "Game of the year <year>" (the player's year), stuck slightly crooked on the case's front and,
//! small, on its spine. The player puts them on from the game card's Stickers row (the game box, the info page and
//! the page: X on a game, then Stickers), together with their own Curator stickers; kept per game in `library.json`
//! (`GameMeta::case`), synced (`library_sync`, field `case`) and backed up with it. The Shelf look, the Cabinet and
//! the game box show them; the other looks don't (their games are not cases).
//!
//! The pictures are PixelLab pixel art (`assets/media/stickers`, prompts in `pixellab-assets.json` there): a printed
//! sticker with a die-cut white edge, no lettering; the words "100 %" and "Game of the year" with the year are
//! painted on them in Barlow. Painting happens where the case's front is made (on the `cabinet-art` thread or once
//! per box), never per frame: a front with stickers is a front of its own in the caches, keyed by [`CaseStickers::key`].

use serde::{Deserialize, Serialize};

/// One kind of case sticker, in the order the sheet lists them and the front stacks them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    Completed,
    Hundred,
    Favourite,
    Playing,
    Goty,
}

impl Kind {
    pub const ALL: [Kind; 5] = [Kind::Completed, Kind::Hundred, Kind::Favourite, Kind::Playing, Kind::Goty];

    /// The sticker's name on the sheet (the year is added to "Game of the year" there).
    pub fn label(self) -> &'static str {
        match self {
            Kind::Completed => "Completed",
            Kind::Hundred => "100 %",
            Kind::Favourite => "Favourite",
            Kind::Playing => "Playing now",
            Kind::Goty => "Game of the year",
        }
    }

    /// The picture's file name in `assets/media/stickers`.
    pub fn file(self) -> &'static str {
        match self {
            Kind::Completed => "completed",
            Kind::Hundred => "hundred",
            Kind::Favourite => "favourite",
            Kind::Playing => "playing",
            Kind::Goty => "goty",
        }
    }
}

/// The stickers on one game's case. Written only when there is one (an older pocket-ui skips the field).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CaseStickers {
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub completed: bool,
    #[serde(default, rename = "100", skip_serializing_if = "std::ops::Not::not")]
    pub hundred: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub favourite: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub playing: bool,
    /// "Game of the year" and the year the player gave it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub goty: Option<u16>,
}

/// The years the Game of the year sticker can carry.
pub const YEAR_FIRST: u16 = 1970;
pub const YEAR_LAST: u16 = 2099;

impl CaseStickers {
    pub fn is_empty(&self) -> bool {
        *self == CaseStickers::default()
    }

    pub fn has(&self, k: Kind) -> bool {
        match k {
            Kind::Completed => self.completed,
            Kind::Hundred => self.hundred,
            Kind::Favourite => self.favourite,
            Kind::Playing => self.playing,
            Kind::Goty => self.goty.is_some(),
        }
    }

    /// Puts `k` on or takes it off; the Game of the year sticker takes `year` when it goes on.
    pub fn set(&mut self, k: Kind, on: bool, year: u16) {
        match k {
            Kind::Completed => self.completed = on,
            Kind::Hundred => self.hundred = on,
            Kind::Favourite => self.favourite = on,
            Kind::Playing => self.playing = on,
            Kind::Goty => self.goty = on.then_some(year.clamp(YEAR_FIRST, YEAR_LAST)),
        }
    }

    /// The stickers on the case, in their order.
    pub fn on(&self) -> Vec<Kind> {
        Kind::ALL.iter().copied().filter(|k| self.has(*k)).collect()
    }

    /// Names for the card's Stickers row: "Completed, Game of the year 1998".
    pub fn names(&self) -> Vec<String> {
        self.on().into_iter().map(|k| self.name(k)).collect()
    }

    /// One sticker's name as the sheet and the card say it.
    pub fn name(&self, k: Kind) -> String {
        match (k, self.goty) {
            (Kind::Goty, Some(y)) => format!("Game of the year {y}"),
            _ => k.label().to_string(),
        }
    }

    /// A small number that changes whenever what is painted changes: part of a cached front's key.
    pub fn key(&self) -> u32 {
        Kind::ALL.iter().enumerate().fold(0, |a, (i, k)| a | (u32::from(self.has(*k)) << i)) | (u32::from(self.goty.unwrap_or(0)) << 8)
    }
}

/// The year offered first for the Game of the year sticker: the game's release year when it is known, else this year.
pub fn default_year(release: Option<u16>, now: u16) -> u16 {
    release.unwrap_or(now).clamp(YEAR_FIRST, YEAR_LAST)
}

/// A slight, stable tilt for each sticker of a game, in degrees (-9 to 9, never nearly straight, never twice the
/// same way in a row): the same every time the case is drawn.
pub fn tilt(seed: u64, k: Kind) -> f32 {
    let i = Kind::ALL.iter().position(|x| *x == k).unwrap_or(0) as u64;
    let mut h = seed ^ (i + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    h ^= h >> 31;
    h = h.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    h ^= h >> 29;
    let deg = 3.0 + (h % 600) as f32 / 100.0;
    if i.is_multiple_of(2) {
        -deg
    } else {
        deg
    }
}

// ---- painting -----------------------------------------------------------------------------------------------------

use crate::cabinet_art::Pic;
use crate::system_art::Sprite;
use crate::text::{Face, Text};

const PNGS: [&[u8]; 5] = [
    include_bytes!("../assets/media/stickers/completed.png"),
    include_bytes!("../assets/media/stickers/hundred.png"),
    include_bytes!("../assets/media/stickers/favourite.png"),
    include_bytes!("../assets/media/stickers/playing.png"),
    include_bytes!("../assets/media/stickers/goty.png"),
];

/// The plain print where the words go, in sticker pixels (`assets/media/stickers/stickers.json`).
const HUNDRED_PLAIN: (i32, i32, i32, i32) = (10, 13, 16, 10);
const GOTY_PLAIN: (i32, i32, i32, i32) = (26, 6, 26, 16);
/// The ink of the words on the gold print.
const GOLD_INK: u32 = 0x4a2a0c;
/// A round sticker's height on a front, as a share of the front's width (a 20 mm sticker on a 135 mm case).
const SHARE: f32 = 0.15;
/// The round stickers' height in sticker pixels (the pictures are 28 to 37).
const ROUND: f32 = 30.0;

impl Kind {
    fn sprite(self) -> &'static Sprite {
        static DECODED: [std::sync::OnceLock<Sprite>; 5] = [const { std::sync::OnceLock::new() }; 5];
        let i = Kind::ALL.iter().position(|k| *k == self).unwrap_or(0);
        DECODED[i].get_or_init(|| crate::system_art::decode(PNGS[i]).unwrap_or(Sprite { w: 1, h: 1, px: vec![0] }))
    }
}

/// Decodes the pictures now (on the caller's thread: the Cabinet's reader), so no frame does.
pub fn warm() {
    for k in Kind::ALL {
        k.sprite();
    }
}

/// One sticker upright, `f` screen pixels a sticker pixel (nearest, so it stays pixel art), with its words or without.
fn sticker_pic(text: &mut Text, k: Kind, f: f32, year: Option<u16>, with_words: bool) -> Pic {
    let s = k.sprite();
    let (w, h) = (((s.w as f32 * f).round() as i32).max(1), ((s.h as f32 * f).round() as i32).max(1));
    let mut p = Pic::new(w, h, 0);
    for y in 0..h {
        let sy = ((y as f32 / f) as u32).min(s.h - 1);
        for x in 0..w {
            let sx = ((x as f32 / f) as u32).min(s.w - 1);
            p.px[(y * w + x) as usize] = s.px[(sy * s.w + sx) as usize];
        }
    }
    if !with_words {
        return p;
    }
    let _fixed = crate::type_scale::fixed();
    let rect = |(x, y, rw, rh): (i32, i32, i32, i32)| ((x as f32 * f) as i32, (y as f32 * f) as i32, (rw as f32 * f) as i32, (rh as f32 * f) as i32);
    match k {
        Kind::Hundred => {
            let (x, y, rw, rh) = rect(HUNDRED_PLAIN);
            words(text, &mut p, Face::Title, "100 %", (x - rw / 6, y, rw + rw / 3, rh), GOLD_INK, 7);
        }
        Kind::Goty => {
            let (x, y, rw, rh) = rect(GOTY_PLAIN);
            let year = year.map(|y| y.to_string()).unwrap_or_default();
            // "Game of the year" over the year where there is room for both (a front out of the shelf, the box);
            // the year alone on a small one.
            let top = rh * 2 / 5;
            if words(text, &mut p, Face::Body, "Game of the year", (x, y, rw, top), GOLD_INK, 8) {
                words(text, &mut p, Face::Title, &year, (x, y + top, rw, rh - top), GOLD_INK, 7);
            } else {
                words(text, &mut p, Face::Title, &year, (x, y, rw, rh), GOLD_INK, 7);
            }
        }
        _ => {}
    }
    p
}

/// Paints `s` centred in the rectangle, as large as it fits (its letters' height about the rectangle's), never under
/// `least` px: false (nothing painted) when it doesn't fit then.
fn words(text: &mut Text, p: &mut Pic, face: Face, s: &str, (x, y, w, h): (i32, i32, i32, i32), ink: u32, least: u32) -> bool {
    let mut px = ((h as f32) * 1.25) as u32;
    while px >= least && text.width(face, px, s) > w {
        px -= 1;
    }
    if px < least || s.is_empty() {
        return false;
    }
    let tw = text.width(face, px, s);
    let bh = (px * 3 / 2) as i32;
    let mut buf = vec![0u32; (tw.max(1) * bh) as usize];
    {
        let mut c = crate::gfx::Canvas::new(&mut buf, tw.max(1) as u32, bh as u32);
        text.draw(&mut c, face, px, 0, px as i32, 0xffffff, s);
    }
    // The capitals' middle on the rectangle's middle (Barlow's capitals are 0.7 of the size).
    let base = y + h / 2 + (px as f32 * 0.35) as i32;
    let left = x + (w - tw) / 2;
    for ty in 0..bh {
        for tx in 0..tw {
            let a = buf[(ty * tw + tx) as usize] & 0xff;
            if a > 0 && p.get(left + tx, base - px as i32 + ty) >> 24 != 0 {
                p.blend(left + tx, base - px as i32 + ty, ink, a);
            }
        }
    }
    true
}

/// `src` turned by `deg` around its middle (nearest pixel, the picture grown to hold the corners).
fn rotated(src: &Pic, deg: f32) -> Pic {
    let (sin, cos) = deg.to_radians().sin_cos();
    let (w, h) = (src.w as f32, src.h as f32);
    let nw = (w * cos.abs() + h * sin.abs()).ceil() as i32;
    let nh = (w * sin.abs() + h * cos.abs()).ceil() as i32;
    let mut out = Pic::new(nw, nh, 0);
    let (cx, cy, ncx, ncy) = (w / 2.0, h / 2.0, nw as f32 / 2.0, nh as f32 / 2.0);
    for y in 0..nh {
        for x in 0..nw {
            let (dx, dy) = (x as f32 + 0.5 - ncx, y as f32 + 0.5 - ncy);
            let sx = (dx * cos + dy * sin + cx).floor() as i32;
            let sy = (-dx * sin + dy * cos + cy).floor() as i32;
            if sx >= 0 && sy >= 0 && sx < src.w && sy < src.h {
                out.px[(y * nw + x) as usize] = src.px[(sy * src.w + sx) as usize];
            }
        }
    }
    out
}

/// Lays `s` over `p` at (x, y), solid pixels only, with a one-pixel shadow under its lower edge (the sticker's edge
/// on the case).
fn stick(p: &mut Pic, s: &Pic, x: i32, y: i32) {
    for sy in 0..s.h {
        for sx in 0..s.w {
            let v = s.px[(sy * s.w + sx) as usize];
            if v >> 24 == 0 {
                continue;
            }
            let below = sy + 1 >= s.h || s.px[((sy + 1) * s.w + sx) as usize] >> 24 == 0;
            if below && p.get(x + sx, y + sy + 1) >> 24 != 0 {
                p.blend(x + sx, y + sy + 1, 0x000000, 90);
            }
            p.blend(x + sx, y + sy, v & 0xffffff, 255);
        }
    }
}

/// Where each sticker goes on a front whose cover is `label` (x, y, w, h), as (kind, x, y) of its turned picture's
/// top left: the round ones in a row from the label's top right corner leftwards, side by side and not quite on one
/// line; Game of the year at the bottom left. `sizes` are the turned pictures' sizes, in [`Kind::ALL`]'s order.
fn layout_plain(on: &[Kind], label: Rect, sizes: &[(i32, i32)], seed: u64) -> Vec<(Kind, i32, i32)> {
    let (lx, ly, lw, lh) = label;
    let inset = (lw / 24).max(2);
    let mut out = Vec::new();
    let mut right = lx + lw - inset;
    let mut row_y = ly + inset;
    let mut row_h = 0;
    for (n, k) in on.iter().filter(|k| **k != Kind::Goty).enumerate() {
        let (w, h) = sizes[Kind::ALL.iter().position(|x| x == k).unwrap_or(0)];
        if right - w < lx + inset && n > 0 {
            // A second row under the first.
            right = lx + lw - inset - w / 3;
            row_y += row_h * 9 / 10;
            row_h = 0;
        }
        let jitter = ((seed >> (n * 7)) % 5) as i32 - 2;
        let y = (row_y + jitter * h / 24).clamp(ly, (ly + lh - h).max(ly));
        out.push((*k, right - w, y));
        right -= w + inset / 2;
        row_h = row_h.max(h);
    }
    if on.contains(&Kind::Goty) {
        let h = sizes[4].1;
        out.push((Kind::Goty, lx + inset, (ly + lh - h - inset).max(ly)));
    }
    out
}

/// The places of the round stickers and the Game of the year one on a cover (`layout_plain`), nothing kept clear.
pub fn layout(on: &[Kind], label: (i32, i32, i32, i32), sizes: &[(i32, i32)], seed: u64) -> Vec<(Kind, i32, i32)> {
    layout_around(on, label, sizes, seed, &[])
}

/// A rectangle: x, y, width, height.
pub type Rect = (i32, i32, i32, i32);

/// [`layout`] with places kept clear: a sticker that would land on one of `keep` (the printed name and console of a
/// case with no cover) moves to the nearest place in the label that touches none of them (all first, then the name
/// alone), and is left off when there is no such place: a sticker never hides the name.
pub fn layout_around(on: &[Kind], label: Rect, sizes: &[(i32, i32)], seed: u64, keep: &[Rect]) -> Vec<(Kind, i32, i32)> {
    let placed = layout_plain(on, label, sizes, seed);
    if keep.is_empty() {
        return placed;
    }
    placed
        .into_iter()
        .filter_map(|(k, x, y)| {
            let (w, h) = sizes[Kind::ALL.iter().position(|q| *q == k).unwrap_or(0)];
            clear_of((x, y, w, h), label, keep).map(|(x, y)| (k, x, y))
        })
        .collect()
}

fn touches(a: Rect, b: Rect) -> bool {
    a.0 < b.0 + b.2 && b.0 < a.0 + a.2 && a.1 < b.1 + b.3 && b.1 < a.1 + a.3
}

/// Where a `w` x `h` sticker at (`x`, `y`) goes so that it touches none of `keep`, inside `label`: where it is, or the
/// nearest place above, below or beside a kept rectangle. `keep[0]` is the name, which is kept clear at any price.
fn clear_of((x, y, w, h): Rect, label: Rect, keep: &[Rect]) -> Option<(i32, i32)> {
    let (lx, ly, lw, lh) = label;
    let inside = |x: i32, y: i32| (x.clamp(lx, (lx + lw - w).max(lx)), y.clamp(ly, (ly + lh - h).max(ly)));
    for set in [keep, &keep[..1]] {
        let free = |x: i32, y: i32| !set.iter().any(|k| touches((x, y, w, h), *k));
        if free(x, y) {
            return Some((x, y));
        }
        let mut best: Option<(i32, (i32, i32))> = None;
        for k in set {
            for (cx, cy) in [(x, k.1 - h - 1), (x, k.1 + k.3 + 1), (k.0 - w - 1, y), (k.0 + k.2 + 1, y)] {
                let (cx, cy) = inside(cx, cy);
                if free(cx, cy) {
                    let d = (cx - x).abs() + (cy - y).abs();
                    if best.is_none_or(|(b, _)| d < b) {
                        best = Some((d, (cx, cy)));
                    }
                }
            }
        }
        if let Some((_, at)) = best {
            return Some(at);
        }
    }
    None
}

/// The stickers on a case's front `p` (made by `media::front`), over its cover `label`; `seed` (the game's file)
/// keeps each one's tilt and place the same every time.
pub fn on_front(text: &mut Text, p: &mut Pic, case: &CaseStickers, label: (i32, i32, i32, i32), seed: u64) {
    on_front_around(text, p, case, label, seed, &[]);
}

/// [`on_front`] for a front made by `media::front` at `k` with `title` (as printed) and `console`: with no `cover`
/// it is the printed title card, and the stickers keep off its name and console.
pub fn on_front_of(text: &mut Text, p: &mut Pic, case: &CaseStickers, m: crate::media::Medium, k: f32, title: &str, cover: bool, console: Option<&crate::system_art::Sprite>, seed: u64) {
    let label = crate::media::label_rect(m, k);
    let keep = kept_clear(text, case, label, title, cover, console);
    on_front_around(text, p, case, label, seed, &keep);
}

/// What the stickers of `case` keep off on a front whose cover area is `label`: the printed name and console of a title
/// card (no `cover`), nothing on a cover.
pub fn kept_clear(text: &mut Text, case: &CaseStickers, label: Rect, title: &str, cover: bool, console: Option<&crate::system_art::Sprite>) -> Vec<Rect> {
    if cover || case.is_empty() {
        Vec::new()
    } else {
        crate::cabinet_art::plain_zones(text, label, title, console)
    }
}

/// [`on_front`] with `keep` clear of stickers ([`layout_around`]).
pub fn on_front_around(text: &mut Text, p: &mut Pic, case: &CaseStickers, label: Rect, seed: u64, keep: &[Rect]) {
    on_front_but(text, p, case, label, seed, keep, &[]);
}

/// [`on_front_around`] with the places of all of `case`'s stickers, but those in `skip` not stuck on: the case just before
/// they land on it (`sticker_press`), where every other one already sits as it will.
pub fn on_front_but(text: &mut Text, p: &mut Pic, case: &CaseStickers, label: Rect, seed: u64, keep: &[Rect], skip: &[Kind]) {
    if case.is_empty() {
        return;
    }
    let (pics, places) = front_stickers(text, p.w, case, label, seed, keep);
    for (k, x, y) in places {
        if !skip.contains(&k) {
            stick(p, &pics[Kind::ALL.iter().position(|x| *x == k).unwrap_or(0)], x, y);
        }
    }
}

/// The turned pictures of `case`'s stickers (in [`Kind::ALL`]'s order, a 1x1 clear one for each that is not on) and
/// where each goes on a front `front_w` wide whose cover is `label`, with `keep` clear.
fn front_stickers(text: &mut Text, front_w: i32, case: &CaseStickers, label: Rect, seed: u64, keep: &[Rect]) -> (Vec<Pic>, Vec<(Kind, i32, i32)>) {
    let f = (front_w as f32 * SHARE / ROUND).max(0.5);
    let pics: Vec<Pic> = Kind::ALL.iter().map(|k| if case.has(*k) { rotated(&sticker_pic(text, *k, f, case.goty, true), tilt(seed, *k)) } else { Pic::new(1, 1, 0) }).collect();
    let sizes: Vec<(i32, i32)> = pics.iter().map(|q| (q.w, q.h)).collect();
    let places = layout_around(&case.on(), label, &sizes, seed, keep);
    (pics, places)
}

/// One sticker of `case` as it is stuck on a front `front_w` wide (`on_front_around`'s picture and place): its turned
/// picture and its top left on the front; None when `case` doesn't have it or it found no place clear of `keep`. What
/// lands on the case or lifts off it (`sticker_press`).
pub fn placed(text: &mut Text, front_w: i32, case: &CaseStickers, label: Rect, seed: u64, keep: &[Rect], k: Kind) -> Option<(Pic, i32, i32)> {
    if !case.has(k) {
        return None;
    }
    let (mut pics, places) = front_stickers(text, front_w, case, label, seed, keep);
    let (_, x, y) = places.into_iter().find(|(q, _, _)| *q == k)?;
    Some((pics.swap_remove(Kind::ALL.iter().position(|x| *x == k).unwrap_or(0)), x, y))
}

/// The side its tilt leans to, +1 or -1 (the sign of [`tilt`]).
pub fn lean(seed: u64, k: Kind) -> f32 {
    tilt(seed, k).signum()
}

/// `stick` for the tests of the press (`sticker_press`): the case's own way of laying a sticker on.
#[cfg(test)]
pub fn stick_for_test(p: &mut Pic, s: &Pic, x: i32, y: i32) {
    stick(p, s, x, y);
}

/// The one small sticker on a spine `p`, in `room` (x, y, w, h: the spine above its title's label, where a collector
/// puts one so the title stays readable), as large as the room allows, without words: the first of Game of the
/// year (as the gold seal: its banner would be a stripe across a spine), 100 %, Completed, Playing now, Favourite.
pub fn on_spine(text: &mut Text, p: &mut Pic, case: &CaseStickers, room: (i32, i32, i32, i32), seed: u64) {
    let Some(k) = [Kind::Goty, Kind::Hundred, Kind::Completed, Kind::Playing, Kind::Favourite].into_iter().find(|k| case.has(*k)) else { return };
    let (x, y, w, h) = room;
    let shown = if k == Kind::Goty { Kind::Hundred } else { k };
    let s = shown.sprite();
    let f = ((w - 2).min(h - 2) as f32 / s.w.max(s.h) as f32).min(2.0);
    if f * (s.h as f32) < 8.0 {
        return;
    }
    let q = rotated(&sticker_pic(text, shown, f, None, false), tilt(seed, k) * 0.6);
    stick(p, &q, x + (w - q.w) / 2, y + (h - q.h) / 2);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every sticker's rectangle on a front, for every kind of case, against what it prints (title cards).
    fn sticker_rects(text: &mut Text, m: crate::media::Medium, k: f32, title: &str, console: Option<&crate::system_art::Sprite>, case: &CaseStickers) -> (Vec<(Kind, Rect)>, Vec<Rect>) {
        let label = crate::media::label_rect(m, k);
        let keep = kept_clear(text, case, label, title, false, console);
        let (pics, places) = front_stickers(text, crate::media::front_size(m, k).0, case, label, 7, &keep);
        let rects = places.into_iter().map(|(kind, x, y)| { let q = &pics[Kind::ALL.iter().position(|a| *a == kind).unwrap()]; (kind, (x, y, q.w, q.h)) }).collect();
        (rects, keep)
    }

    #[test]
    fn no_sticker_covers_the_title_of_a_title_card_on_any_kind_of_case() {
        let mut text = Text::new();
        let all = CaseStickers { completed: true, hundred: true, favourite: true, playing: true, goty: Some(2001) };
        let one = CaseStickers { goty: Some(2001), ..Default::default() };
        let console = crate::system_art::get("snes");
        let mut seen = 0;
        for m in crate::media::Medium::ALL {
            for k in [1.0f32, 0.7, 0.45] {
                for title in ["Zelda", "Gran Turismo 4", "The Legend of Zelda: Ocarina of Time Master Quest", "Überraschung"] {
                    for case in [&all, &one] {
                        let (rects, keep) = sticker_rects(&mut text, m, k, title, console, case);
                        let words = keep[0];
                        for (kind, r) in &rects {
                            seen += 1;
                            assert!(!touches(*r, words), "{m:?} at {k}, \"{title}\": the {kind:?} sticker {r:?} lies on the name {words:?}");
                        }
                    }
                }
            }
        }
        assert!(seen > 200, "the stickers are still put on, not all dropped: {seen}");
    }

    #[test]
    fn stickers_go_on_and_off_and_name_themselves() {
        let mut s = CaseStickers::default();
        assert!(s.is_empty());
        s.set(Kind::Completed, true, 0);
        s.set(Kind::Goty, true, 1998);
        assert_eq!(s.on(), vec![Kind::Completed, Kind::Goty]);
        assert_eq!(s.names(), vec!["Completed".to_string(), "Game of the year 1998".to_string()]);
        let k = s.key();
        s.set(Kind::Goty, true, 1999);
        assert_ne!(s.key(), k, "another year is another picture");
        s.set(Kind::Goty, false, 0);
        s.set(Kind::Completed, false, 0);
        assert!(s.is_empty());
        s.set(Kind::Goty, true, 1800);
        assert_eq!(s.goty, Some(YEAR_FIRST), "a year out of range is clamped");
    }

    #[test]
    fn the_file_keeps_only_what_is_on() {
        let s = CaseStickers { hundred: true, goty: Some(2004), ..Default::default() };
        let json = serde_json::to_string(&s).unwrap();
        assert_eq!(json, r#"{"100":true,"goty":2004}"#);
        assert_eq!(serde_json::from_str::<CaseStickers>(&json).unwrap(), s);
        assert_eq!(serde_json::from_str::<CaseStickers>("{}").unwrap(), CaseStickers::default());
    }

    #[test]
    fn tilts_are_slight_stable_and_alternate() {
        for seed in [0u64, 7, 0xDEAD_BEEF, u64::MAX] {
            let t: Vec<f32> = Kind::ALL.iter().map(|k| tilt(seed, *k)).collect();
            assert_eq!(t, Kind::ALL.iter().map(|k| tilt(seed, *k)).collect::<Vec<_>>());
            for (i, d) in t.iter().enumerate() {
                assert!((3.0..=9.0).contains(&d.abs()), "{d}");
                assert_eq!(*d < 0.0, i % 2 == 0);
            }
        }
    }

    #[test]
    fn the_year_offered_is_the_games_or_this_one() {
        assert_eq!(default_year(Some(1991), 2026), 1991);
        assert_eq!(default_year(None, 2026), 2026);
    }

    #[test]
    fn every_sticker_has_its_picture_solid_or_clear() {
        for k in Kind::ALL {
            let s = k.sprite();
            assert!(s.w >= 20 && s.h >= 20, "{k:?}: {}x{}", s.w, s.h);
            assert!(s.px.iter().all(|p| p >> 24 == 0 || p >> 24 == 0xff), "{k:?}");
        }
        let (gx, gy, gw, gh) = GOTY_PLAIN;
        assert!(gx + gw <= Kind::Goty.sprite().w as i32 && gy + gh <= Kind::Goty.sprite().h as i32);
    }

    #[test]
    fn stickers_stay_on_the_cover_and_do_not_pile_up() {
        let all = Kind::ALL.to_vec();
        for label in [(10, 20, 200, 280), (6, 6, 290, 200), (4, 8, 120, 120)] {
            let d = (label.2 as f32 * 0.24) as i32;
            let sizes = vec![(d, d), (d, d), (d, d), (d, d), (d * 2, d)];
            let at = layout(&all, label, &sizes, 42);
            assert_eq!(at.len(), 5);
            let (lx, ly, lw, lh) = label;
            for (i, (k, x, y)) in at.iter().enumerate() {
                let (w, h) = sizes[Kind::ALL.iter().position(|q| q == k).unwrap()];
                assert!(*x >= lx && *y >= ly && x + w <= lx + lw && y + h <= ly + lh, "{k:?} off the cover {label:?}: {x},{y}");
                for (k2, x2, y2) in &at[i + 1..] {
                    let (w2, h2) = sizes[Kind::ALL.iter().position(|q| q == k2).unwrap()];
                    let ox = (x + w).min(x2 + w2) - x.max(x2);
                    let oy = (y + h).min(y2 + h2) - y.max(y2);
                    assert!(ox <= 0 || oy <= 0 || ox * oy < w * h / 3, "{k:?} and {k2:?} cover each other");
                }
            }
        }
    }

    #[test]
    fn a_sticker_that_lands_on_a_title_card_lands_clear_of_the_name_where_the_case_keeps_it() {
        // The press in the game box (`sticker_press`) puts a sticker where the resting case has it: on a title card that is
        // beside or under the name, never on it.
        let mut text = Text::new();
        // A Super Nintendo case at its shelf size: its Game of the year ticket lies on the name unless it is kept clear.
        let m = crate::media::Medium::Snes;
        let (fw, fh) = crate::media::front_size(m, 1.0);
        let bare = Pic::new(fw, fh, 0xff20_4060);
        let label = crate::media::label_rect(m, 1.0);
        let case = CaseStickers { completed: true, hundred: true, favourite: true, goty: Some(2001), ..Default::default() };
        let keep = kept_clear(&mut text, &case, label, "Gran Turismo 4", false, crate::system_art::get("snes"));
        let words = keep[0];
        let mut moved = 0;
        for k in [Kind::Completed, Kind::Hundred, Kind::Favourite, Kind::Goty] {
            let (_, bx, by) = placed(&mut text, bare.w, &case, label, 1, &[], k).expect("on the cover");
            let Some((pic, x, y)) = placed(&mut text, bare.w, &case, label, 1, &keep, k) else { continue };
            assert!(!touches((x, y, pic.w, pic.h), words), "{k:?} lands on the name");
            moved += ((x, y) != (bx, by)) as i32;
            // The front the box paints under the landing sticker has every other one where the resting case has it.
            let (pics, places) = front_stickers(&mut text, bare.w, &case, label, 1, &keep);
            let at = places.iter().find(|(q, _, _)| *q == k).map(|(_, x, y)| (*x, *y));
            assert_eq!(at, Some((x, y)), "{k:?}: the press and the resting case agree");
            assert_eq!(pics.len(), Kind::ALL.len());
        }
        assert!(moved > 0, "on a title card at least one place differs from the cover's");
    }

    #[test]
    fn a_front_without_one_sticker_plus_that_sticker_is_the_whole_front() {
        // What the box paints while a sticker lands (`sticker_press`): the front without it, then it over the front, which
        // must end as the case's own picture, where the stickers do not overlap (one under another differs by the order).
        let mut text = Text::new();
        let bare = Pic::new(240, 340, 0xff20_4060);
        let label = (10, 10, 220, 320);
        let case = CaseStickers { completed: true, hundred: true, goty: Some(1998), ..Default::default() };
        let mut whole = bare.clone();
        on_front(&mut text, &mut whole, &case, label, 1);
        let (pics, places) = front_stickers(&mut text, bare.w, &case, label, 1, &[]);
        let rect = |k: Kind| {
            let (_, x, y) = places.iter().find(|(q, _, _)| *q == k).copied().unwrap();
            let q = &pics[Kind::ALL.iter().position(|a| *a == k).unwrap()];
            (x, y, x + q.w, y + q.h)
        };
        for k in [Kind::Completed, Kind::Hundred, Kind::Goty] {
            let mut part = bare.clone();
            on_front_but(&mut text, &mut part, &case, label, 1, &[], &[k]);
            assert_ne!(part.px, whole.px, "{k:?} is missing from the front without it");
            let (pic, x, y) = placed(&mut text, bare.w, &case, label, 1, &[], k).expect("it is on");
            assert_eq!((x, y), (rect(k).0, rect(k).1));
            stick(&mut part, &pic, x, y);
            let r = rect(k);
            let alone = [Kind::Completed, Kind::Hundred, Kind::Goty].iter().filter(|o| **o != k).all(|o| {
                let q = rect(*o);
                q.2 <= r.0 || r.2 <= q.0 || q.3 <= r.1 || r.3 <= q.1
            });
            if alone {
                assert_eq!(part.px, whole.px, "{k:?} over the rest is the whole front");
            }
        }
        assert!(placed(&mut text, bare.w, &case, label, 1, &[], Kind::Favourite).is_none(), "a sticker the case does not have");
        assert_eq!(lean(1, Kind::Completed), tilt(1, Kind::Completed).signum());
    }

    #[test]
    fn a_front_gets_its_stickers_and_a_bare_one_stays_as_it_was() {
        let mut text = Text::new();
        let bare = Pic::new(240, 340, 0xff20_4060);
        let mut p = bare.clone();
        on_front(&mut text, &mut p, &CaseStickers::default(), (10, 10, 220, 320), 1);
        assert_eq!(p.px, bare.px);
        let case = CaseStickers { completed: true, goty: Some(1998), ..Default::default() };
        on_front(&mut text, &mut p, &case, (10, 10, 220, 320), 1);
        let changed = p.px.iter().zip(&bare.px).filter(|(a, b)| a != b).count();
        assert!(changed > 2000, "{changed} pixels");
        let mut again = bare.clone();
        on_front(&mut text, &mut again, &case, (10, 10, 220, 320), 1);
        assert_eq!(again.px, p.px, "the same every time");
    }
}
