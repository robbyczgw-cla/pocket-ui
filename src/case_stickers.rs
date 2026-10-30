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
pub fn layout(on: &[Kind], label: (i32, i32, i32, i32), sizes: &[(i32, i32)], seed: u64) -> Vec<(Kind, i32, i32)> {
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

/// The stickers on a case's front `p` (made by `media::front`), over its cover `label`; `seed` (the game's file)
/// keeps each one's tilt and place the same every time.
pub fn on_front(text: &mut Text, p: &mut Pic, case: &CaseStickers, label: (i32, i32, i32, i32), seed: u64) {
    let on = case.on();
    if on.is_empty() {
        return;
    }
    let f = (p.w as f32 * SHARE / ROUND).max(0.5);
    let pics: Vec<Pic> = Kind::ALL.iter().map(|k| if case.has(*k) { rotated(&sticker_pic(text, *k, f, case.goty, true), tilt(seed, *k)) } else { Pic::new(1, 1, 0) }).collect();
    let sizes: Vec<(i32, i32)> = pics.iter().map(|q| (q.w, q.h)).collect();
    for (k, x, y) in layout(&on, label, &sizes, seed) {
        stick(p, &pics[Kind::ALL.iter().position(|x| *x == k).unwrap_or(0)], x, y);
    }
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
