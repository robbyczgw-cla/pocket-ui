//! The game card as a game box (X on a game; SELECT switches to the page and back, and it is kept): the game in a box in the style of its
//! system's case, almost the whole screen. The front is the case as it stands on the shelf (`media::front`, the
//! cover in it), opened: the cartridge, card or disc slides out of it as the box comes up (0.1.4), as large beside
//! it as it really is (`media::Game::size_beside`); LEFT or RIGHT turns it over. The back is laid
//! out like a real box's: a band with the title, the screenshot and title screen beside the description, the facts,
//! and the player's own line (status, play time, rating, achievements). It scrolls with up and down (L1/R1 a page).
//! Whatever is missing gets a plain place of its own (a dark screen with the console, "No description yet"), never a
//! hole.
//!
//! The frames of the backs (and of the cardboard box's front) are pixel art from PixelLab (`assets/media/back`, how
//! they were made in `tools/`), drawn nine-slice: corners as drawn, edges stretched, the middle the back's paper that
//! is painted here. The card (`game_card`) keeps its modes over the box: Y the status, X the note, START the emulator,
//! and SELECT shows the card as a page.

use std::time::{Duration, Instant};

use crate::cabinet::{ink_on, luma};
use crate::cabinet_art::{self as art, Look, Pic};
use crate::game_card::{paint_mark, paint_stars, CardData, CardPicture, Summary};
use crate::gfx::{mix, Canvas};
use crate::image::Image;
use crate::library_meta::Status;
use crate::media::Medium;
use crate::palette::*;
use crate::system_art::Sprite;
use crate::text::{Face, Text};
use crate::type_scale;

/// The kinds of box a game comes in.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Hull {
    /// The cardboard box of a cartridge (SNES, N64, NES, Game Boy, GBA and the rest).
    CartBox,
    /// The Mega Drive's and Master System's plastic clamshell.
    Clamshell,
    /// The CD jewel case (PlayStation, Saturn, Dreamcast, Mega-CD, other discs).
    Jewel,
    /// The DVD keep case (PS2, GameCube, Wii, Xbox, Curator's), in the system's colour.
    Dvd,
    Umd,
    /// The DS's and 3DS's small case.
    Card,
}

/// The template pictures: the back of every kind, and the cardboard box's front.
const BACKS: [(Hull, &[u8]); 6] = [
    (Hull::CartBox, include_bytes!("../assets/media/back/cartbox.png")),
    (Hull::Clamshell, include_bytes!("../assets/media/back/clamshell.png")),
    (Hull::Jewel, include_bytes!("../assets/media/back/jewel.png")),
    (Hull::Dvd, include_bytes!("../assets/media/back/dvd.png")),
    (Hull::Umd, include_bytes!("../assets/media/back/umd.png")),
    (Hull::Card, include_bytes!("../assets/media/back/card.png")),
];
const CARTBOX_FRONT: &[u8] = include_bytes!("../assets/media/back/cartbox-front.png");

/// The neutral grey the cardboard box and the DVD case are drawn in, coloured at run time (as `media`'s cartridges).
const GREY: u32 = 156;

/// How long turning the box over takes (Settings → Look & feel → Motion: Off turns it at once).
pub const FLIP: Duration = Duration::from_millis(280);
/// How long the game takes to slide out of its case when the box comes up (Motion: Off at once).
pub const OPEN: Duration = Duration::from_millis(300);

/// How far a game `gw` wide is out of its case `cw` wide once it is open: all the way, a little apart from it, so its
/// label (cover or title) shows whole.
fn out_of(gw: f32, cw: f32) -> f32 {
    gw + cw * 0.05
}

impl Hull {
    pub fn of(m: Medium) -> Hull {
        match m {
            Medium::Snes | Medium::N64 | Medium::Nes | Medium::Gb | Medium::Gba => Hull::CartBox,
            Medium::MegaDrive | Medium::MasterSystem => Hull::Clamshell,
            Medium::Ps1 | Medium::SegaDisc => Hull::Jewel,
            Medium::Ps2 | Medium::GameCube | Medium::Wii | Medium::Keep | Medium::Xbox => Hull::Dvd,
            // pocket-ui's own case is plastic, as a DVD case is.
            Medium::GenericCart | Medium::GenericDisc | Medium::GenericDigital => Hull::Dvd,
            Medium::Umd => Hull::Umd,
            Medium::Ds | Medium::N3ds | Medium::Vita => Hull::Card,
        }
    }

    /// Template pixels of frame on every side (`draw_backs.py`'s BAND).
    fn band(self) -> i32 {
        match self {
            Hull::Jewel | Hull::Umd => 7,
            _ => 6,
        }
    }

    fn back(self) -> &'static Sprite {
        static DECODED: [std::sync::OnceLock<Sprite>; 6] = [const { std::sync::OnceLock::new() }; 6];
        let i = BACKS.iter().position(|(h, _)| *h == self).unwrap_or(0);
        DECODED[i].get_or_init(|| crate::system_art::decode(BACKS[i].1).unwrap_or(Sprite { w: 1, h: 1, px: vec![0] }))
    }
}

fn cartbox_front() -> &'static Sprite {
    static DECODED: std::sync::OnceLock<Sprite> = std::sync::OnceLock::new();
    DECODED.get_or_init(|| crate::system_art::decode(CARTBOX_FRONT).unwrap_or(Sprite { w: 1, h: 1, px: vec![0] }))
}

/// The colour a grey frame takes: the cardboard the cover's colour, a DVD case its system's plastic.
fn frame_tint(hull: Hull, m: Medium, look: Look) -> Option<u32> {
    match hull {
        Hull::CartBox => Some(look.color),
        Hull::Dvd => Some(match m {
            Medium::Wii => 0xe6e6ea,
            Medium::Xbox => 0x1b1d1c,
            Medium::Ps2 => 0x1d1e24,
            Medium::GameCube => 0x24242a,
            Medium::GenericCart | Medium::GenericDisc | Medium::GenericDigital => 0x2b2724,
            _ => look.plastic.max(0x30_30_36),
        }),
        _ => None,
    }
}

#[inline]
fn tinted(p: u32, tint: Option<u32>) -> u32 {
    let Some(t) = tint else { return p & 0xffffff };
    let l = luma(p & 0xffffff);
    let ch = |s: u32| (((t >> s & 0xff) * l / GREY).min(255)) << s;
    ch(16) | ch(8) | ch(0)
}

/// A frame `w` x `h` from template `s` drawn nine-slice at `k` screen pixels a template pixel: the corners and the
/// band as drawn, the band's middle stretched along each edge; the middle (the paper) left clear.
fn nine(s: &Sprite, band: i32, k: i32, w: i32, h: i32, tint: Option<u32>) -> Pic {
    let mut p = Pic::new(w, h, 0);
    let (sw, sh) = (s.w as i32, s.h as i32);
    let b = band * k;
    // The template column (row) of a screen column (row): the ends as drawn, the middle of the template between.
    let map = |v: i32, len: i32, src: i32| -> Option<i32> {
        if v < b {
            Some(v / k)
        } else if v >= len - b {
            Some(src - (len - v + k - 1) / k)
        } else {
            None
        }
    };
    for y in 0..h {
        let ty = map(y, h, sh);
        for x in 0..w {
            let tx = map(x, w, sw);
            let (sx, sy) = match (tx, ty) {
                (None, None) => continue,
                (Some(tx), None) => (tx, sh / 2),
                (None, Some(ty)) => (sw / 2, ty),
                (Some(tx), Some(ty)) => (tx, ty),
            };
            let px = s.px[(sy.clamp(0, sh - 1) * sw + sx.clamp(0, sw - 1)) as usize];
            if px >> 24 != 0 {
                p.px[(y * w + x) as usize] = 0xff00_0000 | tinted(px, tint);
            }
        }
    }
    p
}

/// What the app gives the box to paint: the case's look and front (made with the shelf's machinery,
/// `app_card::paint_card`), the game inside, the cover, the console's picture.
pub struct Inputs<'a> {
    pub medium: Medium,
    pub look: Look,
    /// The case's front at the height asked for (`case_height`), with its cover; None until it is made (a plain
    /// cardboard box with the cover stands in).
    pub front: Option<&'a Pic>,
    /// The cartridge or card out of the case, at its size beside it (`media::Game::size_beside`); None for a disc,
    /// painted round here.
    pub game: Option<&'a Pic>,
    pub cover: Option<&'a Image>,
    pub console: Option<&'a Sprite>,
}

/// The box's state on an open card.
#[derive(Default)]
pub struct BoxView {
    /// The back is up (or turning to it).
    pub back: bool,
    flip_at: Option<Instant>,
    /// When the box came up: the game slides out of its case from then (`OPEN`).
    opened_at: Option<Instant>,
    pub scroll: i32,
    max_scroll: i32,
    /// The frame as made last: (hull, front?, w, h, tint) and the picture.
    frames: Vec<((Hull, bool, i32, i32, Option<u32>), Pic)>,
    /// The cardboard box's front as made last, per size and cover.
    cart_front: Option<((i32, i32, bool, u32), Pic)>,
    /// A canvas to paint a face on while the box turns.
    scratch: Vec<u32>,
    /// The fact on the back the selection is on (an index into `CardData::links`, Connections); None: A plays.
    pub link: Option<usize>,
    /// Where those facts are in the back's scrolling part (top, height, unscrolled) and how tall its view is, from the
    /// last paint: a fact is picked only while it is in view.
    link_at: Vec<(i32, i32)>,
    view_h: i32,
}

/// Where the box goes on a screen `w` x `h`: (top, bottom) of its room.
pub(crate) fn room(w: i32, h: i32) -> (i32, i32, i32) {
    let top = (h / 9).clamp(52, 104);
    // Above the hint bar, which is as tall on every screen.
    let bottom = h - 104;
    let side = if w < 900 { 24 } else { MARGIN };
    (top, bottom, side)
}

/// The baseline of the card's top line (the system and the title, the status corner) over the box.
pub fn top_line(h: i32) -> i32 {
    (room(1, h).0 - 26).min(72)
}

/// How tall the box is on a screen `w` x `h`: the back is this tall.
pub fn front_height(w: i32, h: i32) -> i32 {
    let (top, bottom, _) = room(w, h);
    bottom - top
}

/// How tall the case of `m` is made on a screen `w` x `h`: the box's height, or less where the case and its game half
/// out beside it would be wider than the screen (a SNES box lying wide on 640 x 480).
pub fn case_height(w: i32, h: i32, m: Medium) -> i32 {
    let (_, _, side) = room(w, h);
    let hh = front_height(w, h);
    let t = m.template_size();
    let game = m.game();
    let (case_w, case_h) = m.mm();
    let per_h = t.front as f32 / t.front_h as f32 + out_of(game.mm().0, case_w) / case_h;
    hh.min(((w - 2 * side) as f32 / per_h) as i32).max(1)
}

/// The game beside a case `case_h` tall: its size, and how far it is out of the case's right edge when open.
fn game_beside(m: Medium, (case_w, case_h): (i32, i32), game: Option<&Pic>) -> (i32, i32, i32) {
    let (gw, gh) = match game {
        Some(p) => (p.w, p.h),
        None => m.game().size_beside(m, case_h),
    };
    (gw, gh, out_of(gw as f32, case_w as f32) as i32)
}

/// The front's and the back's width.
fn widths(w: i32, h: i32, front_w: i32) -> (i32, i32) {
    let (_, _, side) = room(w, h);
    let avail = w - 2 * side;
    let hh = front_height(w, h);
    let front = front_w.min(avail);
    let back = avail.min(front.max((hh as f32 * 0.95) as i32).max(type_scale::wide(720, 28)));
    (front, back)
}

/// The front's width for a cartridge's cardboard box `h` pixels tall: as wide as its cover is (a SNES box lies on
/// its side, a Game Boy's stands), 5:7 without one.
fn cartbox_w(h: i32, cover: Option<&Image>) -> i32 {
    match cover {
        Some(c) if c.h > 0 => (h as f32 * (c.w as f32 / c.h as f32).clamp(0.6, 1.5)) as i32,
        _ => h * 5 / 7,
    }
}

impl BoxView {
    pub fn new() -> BoxView {
        let dur = crate::motion::span(crate::motion::Kind::Move, OPEN);
        BoxView {
            opened_at: (!dur.is_zero()).then(Instant::now),
            ..BoxView::default()
        }
    }

    /// How far the game is out of its case, 0 (inside) to 1 (half out, as it stays).
    fn openness(&self, now: Instant) -> f32 {
        let dur = crate::motion::span(crate::motion::Kind::Move, OPEN);
        match self.opened_at {
            Some(at) if !dur.is_zero() => crate::cabinet::ease_out(now.duration_since(at).as_secs_f32() / dur.as_secs_f32()),
            _ => 1.0,
        }
    }

    /// LEFT or RIGHT: turns the box over (from wherever a turn has got to).
    pub fn flip(&mut self) {
        let now = Instant::now();
        let dur = crate::motion::span(crate::motion::Kind::Move, FLIP);
        let start = match self.flip_at {
            // Turned back halfway: as far back as it had come.
            Some(at) if now.duration_since(at) < dur => now - (dur - now.duration_since(at)),
            _ => now,
        };
        self.back = !self.back;
        self.link = None;
        self.flip_at = (!dur.is_zero()).then_some(start);
    }

    /// For a snapshot: the last turn done, or `POCKETUI_SNAPSHOT_FLIP_MS` into it; the game out of its case, or
    /// `POCKETUI_SNAPSHOT_OPEN_MS` into its slide.
    pub fn snapshot_settle(&mut self) {
        let open: Option<u64> = std::env::var("POCKETUI_SNAPSHOT_OPEN_MS").ok().and_then(|v| v.parse().ok());
        self.opened_at = open.and_then(|ms| Instant::now().checked_sub(Duration::from_millis(ms)));
        let ms: Option<u64> = std::env::var("POCKETUI_SNAPSHOT_FLIP_MS").ok().and_then(|v| v.parse().ok());
        self.flip_at = match (self.flip_at, ms) {
            (Some(_), Some(ms)) => Instant::now().checked_sub(Duration::from_millis(ms)),
            _ => None,
        };
    }

    pub fn animating(&self, now: Instant) -> bool {
        self.flip_at.is_some_and(|at| now.duration_since(at) < crate::motion::span(crate::motion::Kind::Move, FLIP))
            || self.opened_at.is_some_and(|at| now.duration_since(at) < crate::motion::span(crate::motion::Kind::Move, OPEN))
    }

    /// Up/Down/L1/R1 on the back: scrolls it; false when there was nothing to scroll (the front, the end).
    pub fn scroll_by(&mut self, d: i32) -> bool {
        if !self.back {
            return false;
        }
        let to = (self.scroll + d).clamp(0, self.max_scroll);
        let moved = to != self.scroll;
        self.scroll = to;
        moved
    }

    pub fn scrollable(&self) -> bool {
        self.back && self.max_scroll > 0
    }

    /// Whether fact `i` of the back's links is wholly in view; before the back was painted, as if it were (the paint
    /// then scrolls to it, `paint_back`).
    fn link_visible(&self, i: usize) -> bool {
        self.view_h == 0 || self.link_at.get(i).is_some_and(|&(top, h)| top >= self.scroll && top + h <= self.scroll + self.view_h)
    }

    /// Up or Down on the back with `n` facts that lead somewhere: onto the next one in view, else a scroll of `step`
    /// (and off a fact that went out of view), as the card's rows are picked.
    pub fn link_step(&mut self, down: bool, n: usize, step: i32) {
        match (down, self.link) {
            (true, Some(i)) if i + 1 < n && self.link_visible(i + 1) => self.link = Some(i + 1),
            (true, None) if (0..n).any(|j| self.link_visible(j)) => self.link = (0..n).find(|&j| self.link_visible(j)),
            (false, Some(i)) if i > 0 && self.link_visible(i - 1) => self.link = Some(i - 1),
            (false, Some(_)) => self.link = None,
            (_, at) => {
                self.scroll_by(if down { step } else { -step });
                if at.is_some_and(|i| !self.link_visible(i)) {
                    self.link = None;
                }
            }
        }
    }

    /// Paints the box, front or back (or turning) under the card's top line.
    pub fn paint(&mut self, c: &mut Canvas, text: &mut Text, d: &CardData, pictures: &[CardPicture], inp: &Inputs) {
        let now = Instant::now();
        let dur = crate::motion::span(crate::motion::Kind::Move, FLIP);
        let t = match self.flip_at {
            Some(at) if !dur.is_zero() && now.duration_since(at) < dur => now.duration_since(at).as_secs_f32() / dur.as_secs_f32(),
            _ => {
                self.flip_at = None;
                1.0
            }
        };
        if t >= 1.0 {
            self.paint_face(c, text, d, pictures, inp, self.back, None);
            return;
        }
        // Turning: the face going away narrows to its edge, the other comes round from it.
        let showing_back = if t < 0.5 { !self.back } else { self.back };
        let (w, h) = (c.w, c.h);
        let mut scratch = std::mem::take(&mut self.scratch);
        scratch.clear();
        scratch.resize((w * h) as usize, pal().void);
        let rect = {
            let mut off = Canvas::new(&mut scratch, w as u32, h as u32);
            self.paint_face(&mut off, text, d, pictures, inp, showing_back, Some(()))
        };
        let (fw0, bw0) = widths(w, h, self.front_w(inp, h));
        let (from_w, to_w) = if self.back { (fw0, bw0) } else { (bw0, fw0) };
        let along = from_w as f32 + (to_w - from_w) as f32 * t;
        let squeeze = (std::f32::consts::PI * t).cos().abs();
        let dw = ((along * squeeze) as i32).max(2);
        let (rx, ry, rw, rh) = rect;
        let dh = (rh as f32 * (1.0 - 0.05 * (std::f32::consts::PI * t).sin())) as i32;
        let (dx, dy) = (w / 2 - dw / 2, ry + (rh - dh) / 2);
        for y in 0..dh {
            let sy = ry + y * rh / dh.max(1);
            let yy = dy + y;
            if yy < 0 || yy >= h || sy < 0 || sy >= h {
                continue;
            }
            for x in 0..dw {
                let sx = rx + x * rw / dw;
                let xx = dx + x;
                if xx < 0 || xx >= w || sx < 0 || sx >= w {
                    continue;
                }
                // The face darkens as it turns away from the light.
                let p = scratch[(sy * w + sx) as usize];
                c.buf[(yy * w + xx) as usize] = mix(p, 0x000000, ((1.0 - squeeze) * 120.0) as u32);
            }
        }
        self.scratch = scratch;
    }

    /// The front face's width: the case and its game half out beside it.
    fn front_w(&self, inp: &Inputs, h: i32) -> i32 {
        let hh = front_height(1, h).max(1);
        match inp.front {
            Some(f) => f.w + game_beside(inp.medium, (f.w, f.h), inp.game).2,
            None => cartbox_w(hh, inp.cover),
        }
    }

    /// Paints one face; returns its rectangle. `off`: painted off screen for the turn (no scroll bar).
    fn paint_face(&mut self, c: &mut Canvas, text: &mut Text, d: &CardData, pictures: &[CardPicture], inp: &Inputs, back: bool, off: Option<()>) -> (i32, i32, i32, i32) {
        let (top, bottom, _) = room(c.w, c.h);
        let hh = bottom - top;
        let (fw, bw) = widths(c.w, c.h, self.front_w(inp, c.h));
        let w = if back { bw } else { fw };
        let x = (c.w - w) / 2;
        if back {
            // A soft shadow under the box, as under a case out of its shelf.
            art::soft_shadow(c, x + 12, top + 14, w, hh, 26, 0.5, (0, c.h));
            self.paint_back(c, text, d, pictures, inp, (x, top, w, hh), off.is_none());
            return (x, top, w, hh);
        }
        let Some(front) = inp.front else {
            art::soft_shadow(c, x + 12, top + 14, w, hh, 26, 0.5, (0, c.h));
            self.paint_front(c, text, d, inp, (x, top, w, hh));
            return (x, top, w, hh);
        };
        // The case, opened: its game half out of it at its right, both in the middle of the room.
        let (cw, ch) = (front.w, front.h);
        let (gw, gh, out) = game_beside(inp.medium, (cw, ch), inp.game);
        let x = (c.w - (cw + out)) / 2;
        let y = top + (hh - ch) / 2;
        let open = if off.is_some() { 1.0 } else { self.openness(Instant::now()) };
        // Inside, its right edge at the case's; open, `out` past it.
        let gx = x + cw - gw + (out as f32 * open) as i32;
        let gy = y + (ch - gh) / 2;
        if open > 0.0 {
            art::soft_shadow(c, gx + gw / 2 + 8, gy + 10, gw / 2, gh, 20, 0.4, (0, c.h));
            match inp.game {
                Some(pic) => art::blit(c, pic, gx, gy, 256, (0, c.h)),
                None if inp.medium.game() == crate::media::Game::Umd => art::umd(c, gx + gw / 2, gy + gh / 2, gh / 2, inp.look.color, c.h),
                None => art::disc(c, gx + gw / 2, gy + gh / 2, gh / 2, inp.look.color, c.h),
            }
        }
        art::soft_shadow(c, x + 12, y + 14, cw, ch, 26, 0.5, (0, c.h));
        art::blit(c, front, x, y, 256, (0, c.h));
        (x, top, cw + out, hh)
    }

    fn frame(&mut self, hull: Hull, front: bool, w: i32, h: i32, tint: Option<u32>) -> &Pic {
        let key = (hull, front, w, h, tint);
        if let Some(i) = self.frames.iter().position(|(k, _)| *k == key) {
            return &self.frames[i].1;
        }
        let s = if front { cartbox_front() } else { hull.back() };
        // Template pixels as big as the box allows, a whole number of screen pixels each (pixel art stays sharp).
        let k = ((h as f32 / s.h as f32 * 0.6).round() as i32).clamp(2, 6);
        let pic = nine(s, hull.band(), k, w, h, tint);
        if self.frames.len() >= 4 {
            self.frames.remove(0);
        }
        self.frames.push((key, pic));
        &self.frames.last().expect("just pushed").1
    }

    /// The band of the frame, in screen pixels, for a box `h` tall.
    fn band_px(hull: Hull, front: bool, h: i32) -> i32 {
        let s = if front { cartbox_front() } else { hull.back() };
        let k = ((h as f32 / s.h as f32 * 0.6).round() as i32).clamp(2, 6);
        hull.band() * k
    }

    /// The front while the case's own is not made yet: a cardboard box with the cover in its printed frame, or a plain
    /// label with the console (never an empty place).
    fn paint_front(&mut self, c: &mut Canvas, text: &mut Text, d: &CardData, inp: &Inputs, (x, y, w, h): (i32, i32, i32, i32)) {
        let key = (w, h, inp.cover.is_some(), inp.look.color);
        if self.cart_front.as_ref().is_none_or(|(k, _)| *k != key) {
            let hull = Hull::CartBox;
            let mut pic = self.frame(hull, true, w, h, Some(inp.look.color)).clone();
            let b = Self::band_px(hull, true, h);
            let (ix, iy, iw, ih) = (b, b, w - 2 * b, h - 2 * b);
            match inp.cover {
                Some(img) => {
                    let fitted = art::cover_fit(img, iw, ih);
                    pic.fill(ix, iy, iw, ih, 0xff00_0000 | mix(inp.look.color, 0x000000, 140));
                    let (cw, ch) = (fitted.w as i32, fitted.h as i32);
                    let (ox, oy) = (ix + (iw - cw) / 2, iy + (ih - ch) / 2);
                    for yy in 0..ch {
                        for xx in 0..cw {
                            pic.px[((oy + yy) * w + ox + xx) as usize] = 0xff00_0000 | fitted.px[(yy * cw + xx) as usize];
                        }
                    }
                }
                None => art::plain_label(text, &mut pic, (ix, iy, iw, ih), inp.look, &d.title, inp.console),
            }
            self.cart_front = Some((key, pic));
        }
        if let Some((_, pic)) = &self.cart_front {
            art::blit(c, pic, x, y, 256, (0, c.h));
        }
    }

    fn paint_back(&mut self, c: &mut Canvas, text: &mut Text, d: &CardData, pictures: &[CardPicture], inp: &Inputs, (x, y, w, h): (i32, i32, i32, i32), bar: bool) {
        let hull = Hull::of(inp.medium);
        let tint = frame_tint(hull, inp.medium, inp.look);
        let frame = self.frame(hull, false, w, h, tint).clone();
        art::blit(c, &frame, x, y, 256, (0, c.h));
        let b = Self::band_px(hull, false, h);
        let (px, py, pw, ph) = (x + b, y + b, w - 2 * b, h - 2 * b);
        // The back's paper: the cover's colour, deep, so the white type reads on every game.
        let paper = mix(inp.look.color, 0x0c0c0e, 165);
        c.rect(px, py, pw, ph, paper);
        let ink = ink_on(paper);
        let soft = mix(ink, paper, 95);
        let pad = (pw / 28).clamp(14, 28);
        // The band at the top: the title (an Xbox case's band is its green).
        let band_c = if inp.medium == Medium::Xbox { 0x4a8a2a } else { d.stripe };
        let small = ph < 520;
    let band_h = type_scale::row(if small { 56 } else { 70 }, 37);
        c.rect(px, py, pw, band_h, band_c);
        let band_ink = ink_on(band_c);
        let year = d.year.map(|y| y.to_string()).unwrap_or_default();
        let year_w = if year.is_empty() { 0 } else { text.width(Face::Body, 28, &year) + pad };
        let title = text.fit(Face::Title, 37, &d.title, pw - 2 * pad - year_w);
        let mid = py + band_h / 2 + type_scale::mid(37);
        text.draw(c, Face::Title, 37, px + pad, mid, band_ink, &title);
        if !year.is_empty() {
            text.draw_right(c, Face::Body, 28, px + pw - pad, mid, band_ink, &year);
        }
        // The strip at the foot: the system, as a box's back names its console.
        let foot_h = type_scale::row(if small { 38 } else { 48 }, 24);
        let foot_y = py + ph - foot_h;
        c.rect(px, foot_y, pw, foot_h, mix(paper, 0x000000, 90));
        let system = text.fit(Face::Body, 24, &d.system, pw - 2 * pad);
        text.draw(c, Face::Body, 24, px + pad, foot_y + foot_h / 2 + type_scale::mid(24), soft, &system);
        // The rest scrolls between them.
        let (top, bot) = ((py + band_h).clamp(0, c.h), foot_y.clamp(0, c.h));
        let view_h = bot - top;
        if view_h <= 0 {
            return;
        }
        let full_w = c.w;
        self.view_h = view_h;
        self.link_at = vec![(-1, 0); d.links.len()];
        let height = {
            let mut body = c.rows(top, bot);
            let links = (self.link, &mut self.link_at);
            paint_back_body(&mut body, text, d, pictures, inp, (px - 0, pw, pad), -self.scroll, ink, soft, paper, links)
        };
        let _ = full_w;
        self.max_scroll = (height - view_h).max(0);
        self.scroll = self.scroll.min(self.max_scroll);
        // A fact picked before the back was painted (or out of view): scrolled into view for the next frame.
        if let Some(&(t, h)) = self.link.and_then(|l| self.link_at.get(l)).filter(|(t, _)| *t >= 0) {
            if t < self.scroll || t + h > self.scroll + view_h {
                self.scroll = (t + h - view_h / 2).clamp(0, self.max_scroll);
            }
        }
        // Text running under the band or the strip fades out there instead of being cut mid-line (as on the page).
        let fade = (view_h / 8).min(36);
        for k in 0..fade {
            let amount = (255 - k * 255 / fade.max(1)) as u8;
            if self.scroll > 0 {
                c.shade_rect(px, top + k, pw, 1, paper, amount);
            }
            if self.scroll < self.max_scroll {
                c.shade_rect(px, bot - 1 - k, pw, 1, paper, amount);
            }
        }
        if bar && self.max_scroll > 0 {
            let bar_h = (view_h * view_h / (view_h + self.max_scroll)).max(30);
            let bar_y = top + (view_h - bar_h) * self.scroll / self.max_scroll.max(1);
            c.rect(px + pw - 8, top, 4, view_h, mix(paper, ink, 40));
            c.rect(px + pw - 8, bar_y, 4, bar_h, soft);
        }
    }
}

/// The scrolling part of the back, `y0` the top (negative when scrolled), inside the paper from `x` `w` wide; returns
/// its height.
#[allow(clippy::too_many_arguments)]
fn paint_back_body(c: &mut Canvas, text: &mut Text, d: &CardData, pictures: &[CardPicture], inp: &Inputs, (x, w, pad): (i32, i32, i32), y0: i32, ink: u32, soft: u32, paper: u32, (link, link_at): (Option<usize>, &mut Vec<(i32, i32)>)) -> i32 {
    let inner_x = x + pad;
    let inner_w = w - 2 * pad - 10;
    let line = type_scale::line(28, 8);
    let row = type_scale::row(44, 28);
    let mut y = y0 + pad;
    if let Some(why) = &d.broken {
        y = crate::game_card::paint_problem(c, text, why, (inner_x, y + type_scale::grow(28) + 22, inner_w));
    }
    // Two pictures: the screenshot and the title screen, or a plain screen in their place.
    let wide = inner_w >= 900;
    let shot_w = if wide { inner_w * 3 / 8 } else { (inner_w - pad) / 2 };
    let shot_h = shot_w * 3 / 4;
    let caption = type_scale::line(24, 10);
    let slots: Vec<(&str, Option<&Image>)> = ["In the game", "Title screen"]
        .iter()
        .map(|label| (*label, pictures.iter().find(|p| p.label == *label).and_then(|p| p.image.as_deref())))
        .collect();
    let mut shots_bottom = y;
    if slots.iter().all(|(_, img)| img.is_none()) {
        // Neither picture exists: one screen with the console and what to do, not the same clip-art twice.
        let w = if wide { shot_w } else { inner_w };
        let h = if wide { shot_h } else { shot_h * 2 / 3 };
        paint_shot(c, text, (inner_x, y, w, h), None, inp.console, paper, "No pictures yet. HOME + A in the game takes one.");
        shots_bottom = y + h + caption;
    } else {
        for (i, (label, img)) in slots.iter().enumerate() {
            let (sx, sy) = if wide { (inner_x + if i == 1 { pad } else { 0 }, y + i as i32 * (shot_h + caption + pad)) } else { (inner_x + i as i32 * (shot_w + pad), y) };
            paint_shot(c, text, (sx, sy, shot_w, shot_h), *img, inp.console, paper, "No picture yet");
            text.draw(c, Face::Body, 24, sx, sy + shot_h + caption - 6, soft, label);
            shots_bottom = shots_bottom.max(sy + shot_h + caption);
        }
    }
    // The description: beside the pictures on a wide back, under them on a narrow one.
    let (tx, tw, mut ty) = if wide { (inner_x + shot_w + 2 * pad, inner_w - shot_w - 2 * pad, y) } else { (inner_x, inner_w, shots_bottom + pad) };
    let (about, credit) = about(d);
    ty += type_scale::grow(28) + 22;
    match about {
        Some(about) => {
            for l in text.wrap(Face::Body, 28, &about, tw) {
                text.draw(c, Face::Body, 28, tx, ty, ink, &l);
                ty += line;
            }
        }
        None => {
            let fitted = text.fit(Face::Body, 28, "No description yet.", tw);
            text.draw(c, Face::Body, 28, tx, ty, soft, &fitted);
            ty += line;
        }
    }
    if let Some(credit) = credit {
        for l in text.wrap(Face::Body, 24, &credit, tw) {
            text.draw(c, Face::Body, 24, tx, ty, soft, &l);
            ty += type_scale::line(24, 8);
        }
    }
    // A rule, then the facts and the player's game: on a wide back in the column of the description, under it; on a
    // narrow one under everything, in two columns when there is room, one after the other if not.
    let (inner_x, inner_w) = if wide { (tx, tw) } else { (inner_x, inner_w) };
    y = if wide { ty + pad } else { ty.max(shots_bottom) + pad };
    c.rect(inner_x, y, inner_w, 2, mix(paper, ink, 50));
    y += pad + type_scale::grow(37);
    let two = !wide && inner_w >= type_scale::wide(760, 28);
    let col_w = if two { (inner_w - pad) / 2 } else { inner_w };
    let key_w = type_scale::wide(170, 28).min(col_w / 2);
    let mut ly = y + 30;
    text.draw(c, Face::Title, 37, inner_x, ly, soft, "About the game");
    ly += row;
    let mut facts: Vec<(&str, String)> = Vec::new();
    facts.extend(d.year.map(|y| ("Year", y.to_string())));
    // Where `d.facts` start among them, for the ones that lead to the player's other games (Connections).
    let first_fact = facts.len();
    facts.extend(d.facts.iter().map(|(k, v)| (*k, v.clone())));
    facts.extend(d.players.clone().map(|p| ("Players", p)));
    facts.extend(d.scores.clone().map(|s| ("Reviews", s)));
    if facts.is_empty() {
        for l in text.wrap(Face::Body, 28, "Not in the offline game list, so there are no facts to show.", col_w) {
            text.draw(c, Face::Body, 28, inner_x, ly, soft, &l);
            ly += line;
        }
    }
    for (i, (k, v)) in facts.into_iter().enumerate() {
        text.draw(c, Face::Body, 28, inner_x, ly, soft, k);
        // A developer, series or genre the player has other games of: one line, its arrow, a pill when selected.
        if let Some(l) = i.checked_sub(first_fact).and_then(|f| d.links.iter().position(|(x, _)| *x == f)) {
            crate::kit::link_value(c, text, (inner_x + key_w, ly, col_w - key_w), &v, link == Some(l), (ink, soft, ink, paper));
            let h = type_scale::row(44, 28);
            link_at[l] = (ly - type_scale::mid(28) - h / 2 - y0, h);
            ly += row;
            continue;
        }
        let lines = text.wrap(Face::Body, 28, &v, col_w - key_w);
        for (i, l) in lines.iter().take(2).enumerate() {
            // A word longer than the column (a long name) is shortened, not cut by the paper's edge.
            let l = if i == 1 && lines.len() > 2 { lines[1..].join(" ") } else { l.clone() };
            let l = text.fit(Face::Body, 28, &l, col_w - key_w);
            text.draw(c, Face::Body, 28, inner_x + key_w, ly + i as i32 * line, ink, &l);
        }
        ly += row + line * (lines.len().min(2) as i32 - 1);
    }
    let (rx, mut ry) = if two { (inner_x + col_w + pad, y + 30) } else { (inner_x, ly + pad) };
    text.draw(c, Face::Title, 37, rx, ry, soft, "Your game");
    ry += row;
    let rkey = key_w;
    let rw = col_w - rkey;
    text.draw(c, Face::Body, 28, rx, ry, soft, "Status");
    let mut sx = rx + rkey;
    if d.status != Status::NotStarted {
        paint_mark(c, sx + 10, ry - 10, d.status, ink);
        sx += 32;
    }
    let fitted = text.fit(Face::Body, 28, d.status.label(), rx + rkey + rw - sx);
    text.draw(c, Face::Body, 28, sx, ry, ink, &fitted);
    ry += row;
    text.draw(c, Face::Body, 28, rx, ry, soft, "Played");
    let played = d.played.map_or("Not yet".to_string(), |(_, secs)| crate::recents::duration(secs));
    let fitted = text.fit(Face::Body, 28, &played, rw);
    text.draw(c, Face::Body, 28, rx + rkey, ry, ink, &fitted);
    ry += row;
    text.draw(c, Face::Body, 28, rx, ry, soft, "Rating");
    if d.rating > 0 {
        paint_stars(c, rx + rkey + 12, ry - 10, 11, d.rating, ink, mix(paper, ink, 60));
    } else {
        let fitted = text.fit(Face::Body, 28, "None. Press Y", rw);
        text.draw(c, Face::Body, 28, rx + rkey, ry, soft, &fitted);
    }
    ry += row;
    if let Some(a) = &d.achievements {
        text.draw(c, Face::Body, 28, rx, ry, soft, "Unlocked");
        let fitted = text.fit(Face::Body, 28, &format!("{a} achievements"), rw);
        text.draw(c, Face::Body, 28, rx + rkey, ry, ink, &fitted);
        ry += row;
    }
    // Its save slots on their medium, read-only, as on the info page (`save_media`; pocket-core's games).
    if let Some(saves) = &d.saves {
        let n = saves.filled.iter().filter(|f| **f).count();
        text.draw(c, Face::Body, 28, rx, ry, soft, "Save slots");
        let (value, color) = if n > 0 { (format!("{n} of {}", saves.filled.len()), ink) } else { ("All empty".to_string(), soft) };
        let fitted = text.fit(Face::Body, 28, &value, rw);
        text.draw(c, Face::Body, 28, rx + rkey, ry, color, &fitted);
        let kind = crate::save_media::Kind::of(&saves.system);
        let l = crate::save_media::Layout::fitted(kind, saves.filled.len(), col_w, 2 * row - 12);
        let slots = crate::save_media::Slots { filled: &saves.filled, ring: None, icon: crate::save_media::Icon::None, stripe: saves.stripe, numbers: l.block >= 20 };
        let top = ry + row / 2 - 4;
        crate::save_media::paint(c, text, kind, rx, top, &l, &slots);
        // When the newest was saved, beside it (left out where there is no room).
        if let Some(at) = saves.newest.filter(|_| n > 0) {
            let ago = crate::recents::ago(at);
            if text.width(Face::Body, 28, &ago) <= col_w - l.w - 24 {
                text.draw(c, Face::Body, 28, rx + l.w + 24, top + l.h / 2 + type_scale::mid(28), soft, &ago);
            }
        }
        ry += 3 * row;
    }
    // A PortMaster port: its performance and fan, the busy-core offer and the note of a newer version, as on the flat card.
    if let Some((speed, hint)) = &d.port {
        text.draw(c, Face::Body, 28, rx, ry, soft, "Speed");
        let fitted = text.fit(Face::Body, 28, speed, rw);
        text.draw(c, Face::Body, 28, rx + rkey, ry, ink, &fitted);
        ry += row;
        for (shown, line) in [(*hint, "Busy core? Press START"), (d.port_update, "Update available")] {
            if shown {
                let fitted = text.fit(Face::Body, 28, line, rw);
                text.draw(c, Face::Body, 28, rx + rkey, ry - 6, soft, &fitted);
                ry += row;
            }
        }
    }
    ly.max(ry).max(shots_bottom + row) - row + line + pad - y0 + type_scale::grow(28)
}

/// The description and the line that credits it, as the back shows them (and the info page, `game_info`); None when
/// there is no description to show.
pub(crate) fn about(d: &CardData) -> (Option<String>, Option<String>) {
    let (about, credit) = match &d.summary {
        Summary::Wikipedia { text: t, title } => (Some(t.clone()), Some(format!("Text: Wikipedia, “{title}”, CC BY-SA 4.0"))),
        Summary::Loading => (d.sentence.clone(), Some("Loading the description from Wikipedia…".to_string())),
        Summary::Unavailable(why) => (d.sentence.clone(), Some(format!("No description from Wikipedia: {why}"))),
        Summary::Gamelist(t) => (Some(t.clone()), Some("From your EmulationStation gamelist".to_string())),
        Summary::None => (d.sentence.clone(), None),
    };
    // The sentence made of the facts (`game_card::sentence`) is English only, and the facts under it say the same in
    // the player's language: in German the back has no description then rather than an English one.
    let made_up = matches!(d.summary, Summary::None | Summary::Loading | Summary::Unavailable(_));
    let about = about.filter(|_| !(made_up && crate::i18n::current() == crate::i18n::Lang::De));
    (about.filter(|a| !a.trim().is_empty()), credit)
}

/// A picture of the game on the back in a dark screen's bezel; without one, the screen stays dark with its console on
/// it and a word.
fn paint_shot(c: &mut Canvas, text: &mut Text, (x, y, w, h): (i32, i32, i32, i32), img: Option<&Image>, console: Option<&Sprite>, paper: u32, none: &str) {
    c.rect(x - 2, y - 2, w + 4, h + 4, mix(paper, 0x000000, 160));
    c.rect(x, y, w, h, 0x15171b);
    match img {
        Some(img) => {
            let (iw, ih) = (img.w as i32, img.h as i32);
            let (fw, fh) = if iw * h > ih * w { (w, ih * w / iw.max(1)) } else { (iw * h / ih.max(1), h) };
            c.blit_scaled(img, x + (w - fw) / 2, y + (h - fh) / 2, fw, fh);
        }
        None => {
            if let Some(s) = console {
                let s = s.fitted(w * 3 / 5, h / 2);
                c.blit_sprite(&s, x + (w - s.w as i32) / 2, y + h / 2 - s.h as i32 / 2 - 14, 40);
            }
            let lines = text.wrap_at_most(Face::Body, 24, none, w - 16, 2);
            let step = type_scale::line(24, 6);
            for (i, l) in lines.iter().enumerate() {
                let ww = text.width(Face::Body, 24, l);
                text.draw(c, Face::Body, 24, x + (w - ww) / 2, y + h - 16 - (lines.len() - 1 - i) as i32 * step, 0x8a8f99, l);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_medium_has_a_box_and_every_template_has_room_for_its_band() {
        for m in Medium::ALL {
            let hull = Hull::of(m);
            let s = hull.back();
            assert!(s.w as i32 > 4 * hull.band() && s.h as i32 > 4 * hull.band(), "{m:?} {hull:?} {}x{}", s.w, s.h);
        }
        let f = cartbox_front();
        assert!(f.w > 20 && f.h > 20);
        assert_eq!(Hull::of(Medium::Xbox), Hull::Dvd);
        assert_eq!(Hull::of(Medium::Snes), Hull::CartBox);
        assert_eq!(Hull::of(Medium::Ps1), Hull::Jewel);
    }

    #[test]
    fn a_nine_slice_frame_keeps_its_corners_and_leaves_the_middle_clear() {
        let s = Hull::Dvd.back();
        let p = nine(s, Hull::Dvd.band(), 4, 400, 600, None);
        assert_eq!((p.w, p.h), (400, 600));
        assert_eq!(p.get(200, 300) >> 24, 0, "the paper is the app's");
        assert_ne!(p.get(2, 300) >> 24, 0, "the left edge is there");
        assert_ne!(p.get(200, 2) >> 24, 0, "the top edge is there");
    }

    #[test]
    fn the_back_is_wide_enough_to_read_on_every_screen() {
        for (w, h) in [(1240, 1080), (1920, 1080), (1280, 960), (640, 480)] {
            let (front, back) = widths(w, h, cartbox_w(front_height(w, h), None));
            assert!(back >= front && back <= w, "{w}x{h}: {front} {back}");
            assert!(back >= 560.min(w - 48), "{w}x{h}: back {back}");
        }
    }

    #[test]
    fn turning_over_and_back_ends_where_it_started() {
        let mut b = BoxView::new();
        assert!(!b.back);
        b.flip();
        assert!(b.back);
        b.flip();
        assert!(!b.back);
        assert!(!b.scroll_by(40), "the front doesn't scroll");
    }
}
