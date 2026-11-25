//! The game card as a game box (X on a game; SELECT switches to the page and back, and it is kept): the game in a box in the style of its
//! system's case, almost the whole screen. The front is the case as it stands on the shelf (`media::front`, the
//! cover in it), opened: the cartridge, card or disc slides out of it as the box comes up (0.1.4), as large beside
//! it as it really is (`media::Game::size_beside`); LEFT or RIGHT turns it over. The back is laid
//! out like a real box's: a band with the title, under it where the player is from the game's save (`case_knows`, when
//! there is a line), the screenshot and title screen beside the description, the facts,
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
use crate::game_card_model::{paint_mark, paint_stars, CardData, CardPicture, Summary};
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

/// How long turning the box over takes (Settings → Look → Motion: Off turns it at once).
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
    /// Stickers landing on the case's front or lifting off it (`sticker_press`), painted over `front`, which then is the
    /// case without those that land; empty the rest of the time.
    pub moving: &'a [crate::sticker_press::Moving<'a>],
}

/// A canvas as the back's pixels depend on it: design size, design pixels per screen pixel, screen row length, screen
/// pixels, where its design pixel (0, 0) is.
type CanvasShape = (i32, i32, i32, usize, usize, (i32, i32, u8, u8));

fn shape(c: &Canvas) -> CanvasShape {
    (c.w, c.h, c.n(), c.stride(), c.pixels().len(), c.cell(0, 0))
}

/// A back is still while it is read. Keep its finished pixels (including film) until its content or view changes.
#[derive(PartialEq)]
struct BackKey {
    canvas: CanvasShape,
    rect: (i32, i32, i32, i32),
    scroll: i32,
    link: Option<usize>,
    bar: bool,
    medium: Medium,
    look: Look,
    sealed: Option<u64>,
    palette: crate::palette::Palette,
    font: crate::font_style::FontStyle,
    letters: crate::font_style::RetroLetters,
    sizes: (u32, u32, u32),
    lang: crate::i18n::Lang,
    rating_hint: &'static str,
    translating: bool,
    saved_ago: Option<String>,
    /// The cover on the mini cartridge's label: a cover that arrives later repaints the back.
    cover: Option<u64>,
}

struct BackPixels {
    key: BackKey,
    data: CardData,
    pictures: Vec<CardPicture>,
    console: Option<(u32, u32, Vec<u32>)>,
    area: (usize, usize, usize, usize),
    pixels: Vec<u32>,
}

impl BackPixels {
    fn matches(&self, key: &BackKey, data: &CardData, pictures: &[CardPicture], console: Option<&Sprite>) -> bool {
        self.key == *key && self.data == *data
            && self.pictures.len() == pictures.len()
            && self.pictures.iter().zip(pictures).all(|(a, b)| a.label == b.label && a.size == b.size && match (&a.image, &b.image) {
                (Some(a), Some(b)) => std::rc::Rc::ptr_eq(a, b), (None, None) => true, _ => false,
            })
            && match (&self.console, console) {
                (Some((w, h, px)), Some(s)) => *w == s.w && *h == s.h && *px == s.px,
                (None, None) => true, _ => false,
            }
    }

    fn copy(&self, c: &mut Canvas) {
        let stride = c.stride();
        let (x, y, w, h) = self.area;
        for row in 0..h {
            c.pixels_mut()[(y + row) * stride + x..][..w].copy_from_slice(&self.pixels[row * w..][..w]);
        }
    }
}

/// The box's state on an open card.
#[derive(Default)]
pub struct BoxView {
    /// A phone's start of the game takes the case from here: nothing of it is painted.
    pub hide_case: bool,
    /// Where the case's front was painted last.
    pub front_rect: Option<(i32, i32, i32, i32)>,
    /// The back is up (or turning to it).
    pub back: bool,
    flip_at: Option<Instant>,
    /// When the box came up: the game slides out of its case from then (`OPEN`).
    opened_at: Option<Instant>,
    /// Playful animations: the disc beside its case turns from then (the box came up or turned back to its front),
    /// for `playful::SPIN_FOR`; and whether the last paint showed a disc that turns.
    spin_at: Option<Instant>,
    spins: bool,
    /// When the box was painted last and how long that took: the turning disc gets a frame per refresh, or every
    /// other one where the box costs more than half a refresh (the H700's 17 ms, `spin_wake`).
    painted: Option<Instant>,
    cost: Duration,
    pub scroll: i32,
    max_scroll: i32,
    /// The frame as made last: (hull, front?, w, h, tint) and the picture.
    frames: Vec<((Hull, bool, i32, i32, Option<u32>), Pic)>,
    /// The cardboard box's front as made last, per size and cover.
    cart_front: Option<((i32, i32, bool, u32), Pic)>,
    /// A canvas to paint a face on while the box turns.
    scratch: Vec<u32>,
    /// The face on `scratch` (back?, canvas size, the turn's start) and where it is: kept for the half of the turn it shows.
    turn_face: Option<((bool, i32, i32, Option<Instant>), (i32, i32, i32, i32))>,
    /// The fact on the back the selection is on (an index into `CardData::links`, Connections); None: A plays.
    pub link: Option<usize>,
    /// Where those facts are in the back's scrolling part (top, height, unscrolled) and how tall its view is, from the
    /// last paint: a fact is picked only while it is in view.
    link_at: Vec<(i32, i32)>,
    view_h: i32,
    /// The back's scrolling part on its canvas as painted last: (top, x, w).
    body: (i32, i32, i32),
    /// A sealed game (`shrink_wrap`): its seed, and the back in the same film as the front (made once per size).
    pub sealed: Option<u64>,
    film: Option<((i32, i32, u64), Pic)>,
    /// The back's finished pixels: as the screen shows it at rest and as the turn paints it off screen (the same one
    /// where the screen is painted at design size, as on the Brick).
    back_pixels: Vec<BackPixels>,
    /// How often the back's content was painted (laid out, its pictures scaled), not copied from `back_pixels`.
    pub content_paints: u32,
    /// The case lying open with its photos (`album`, `album_paint`): Down on the front opens it, B shuts it.
    pub album: Option<crate::album::AlbumView>,
    /// The photos' pictures, decoded by the app (`app_card`): None for one that couldn't be read.
    pub album_pics: std::collections::HashMap<std::path::PathBuf, Option<std::rc::Rc<Image>>>,
    album_made: crate::album_paint::Made,
}

/// Where the box goes on a screen `w` x `h`: (top, bottom) of its room.
pub(crate) fn room(w: i32, h: i32) -> (i32, i32, i32) {
    let top = (h / 9).clamp(52, 104);
    // Above the hint bar, which is as tall on every screen.
    let bottom = crate::kit::content_bottom(h);
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
pub fn game_beside(m: Medium, (case_w, case_h): (i32, i32), game: Option<&Pic>) -> (i32, i32, i32) {
    let (gw, gh) = match game {
        Some(p) => (p.w, p.h),
        None => m.game().size_beside(m, case_h),
    };
    (gw, gh, out_of(gw as f32, case_w as f32) as i32)
}

/// The front's and the back's width.
/// One face of the turning box from screen edge `a` to `b`, each (x, size factor), its picture the rectangle `src` of the
/// off-screen canvas `buf` (`w` wide): each column where it really is (perspective-correct), as tall as its distance
/// makes it, darkened by `dark` (of 255).
#[allow(clippy::too_many_arguments)]
/// `skip`: the picture's background colour, left out (what is under it stays): a part of the face painted again over what
/// was drawn after the face (the game beside the case, in front of the case's side).
#[allow(clippy::too_many_arguments)]
fn face_columns(c: &mut Canvas, buf: &[u32], w: i32, (rx, ry, rw, rh): (i32, i32, i32, i32), a: (f32, f32), b: (f32, f32), cy: f32, h: f32, dark: u32, skip: Option<u32>) {
    let (l, r) = if a.0 <= b.0 { (a, b) } else { (b, a) };
    let flip = a.0 > b.0;
    if r.0 - l.0 < 0.5 || rw <= 0 || rh <= 0 {
        return;
    }
    let (c0, c1) = ((l.0.floor() as i32).max(0), (r.0.ceil() as i32).min(c.w));
    // Each column once: its rows, the picture's column (None: outside the picture), where its top is and the step down.
    let cols: Vec<(i32, i32, Option<i32>, f32, f32)> = (c0..c1.max(c0))
        .map(|col| {
            let t = ((col as f32 + 0.5 - l.0) / (r.0 - l.0)).clamp(0.0, 1.0);
            let u = t * r.1 / ((1.0 - t) * l.1 + t * r.1);
            let u = if flip { 1.0 - u } else { u };
            let k = l.1 + (r.1 - l.1) * t;
            let ch = h * k;
            let top = cy - ch / 2.0;
            let sx = (rx + (u * rw as f32) as i32).clamp(rx, rx + rw - 1);
            let (r0, r1) = ((top.floor() as i32).max(0), ((top + ch).ceil() as i32).min(c.h));
            (r0, r1, (0..w).contains(&sx).then_some(sx), top, rh as f32 / ch.max(1.0))
        })
        .collect();
    let paint = |col: i32, row: i32, bg: u32| {
        let (r0, r1, sx, top, step) = cols[(col - c0) as usize];
        let Some(sx) = sx.filter(|_| row >= r0 && row < r1) else { return bg };
        let sy = (ry + ((row as f32 + 0.5 - top) * step) as i32).clamp(ry, ry + rh - 1);
        if sy < 0 || sy * w + sx >= buf.len() as i32 {
            return bg;
        }
        let p = buf[(sy * w + sx) as usize];
        if skip == Some(p) {
            return bg;
        }
        mix(p, 0x000000, dark)
    };
    let (lo, hi) = cols.iter().filter(|k| k.2.is_some()).fold((i32::MAX, i32::MIN), |(lo, hi), k| (lo.min(k.0), hi.max(k.1)));
    if lo >= hi {
        return;
    }
    if c.n() != 1 {
        // Painted natively (docs/dev/native-paint.md): the four design pixels of each screen pixel as below, averaged.
        return art::cells_over(c, c0, c1, lo, hi, paint);
    }
    // Row by row: the screen and the face's picture are read and written along their rows. Column by column every
    // pixel was a row (4 KB) away from the last, and a turning frame took 150-260 ms on the Brick's A53 (15.11.2026).
    let c_w = c.w as usize;
    let px = c.pixels_mut();
    for row in lo..hi {
        let line = &mut px[row as usize * c_w..][..c_w];
        for (col, k) in (c0..).zip(&cols) {
            if row >= k.0 && row < k.1 {
                line[col as usize] = paint(col, row, line[col as usize]);
            }
        }
    }
}

/// The box's edge (its spine side) from screen edge `a` to `b`, each (x, size factor), in `color`.
fn edge_columns(c: &mut Canvas, a: (f32, f32), b: (f32, f32), cy: f32, h: f32, color: u32) {
    let (c0, c1) = ((a.0.floor() as i32).max(0), (b.0.ceil() as i32).min(c.w));
    let rows: Vec<(i32, i32)> = (c0..c1.max(c0))
        .map(|col| {
            let t = ((col as f32 + 0.5 - a.0) / (b.0 - a.0).max(0.01)).clamp(0.0, 1.0);
            let ch = h * (a.1 + (b.1 - a.1) * t);
            let top = cy - ch / 2.0;
            ((top.round() as i32).max(0), ((top + ch).round() as i32).min(c.h))
        })
        .collect();
    if c.n() != 1 {
        // Painted natively (docs/dev/native-paint.md): each screen pixel by how much of it the columns cover.
        let (lo, hi) = rows.iter().fold((i32::MAX, i32::MIN), |(lo, hi), k| (lo.min(k.0), hi.max(k.1)));
        return art::cells_over(c, c0, c1, lo, hi, |col, row, bg| {
            let (r0, r1) = rows[(col - c0) as usize];
            if row >= r0 && row < r1 { color } else { bg }
        });
    }
    // Row by row, as `face_columns`.
    let (lo, hi) = rows.iter().fold((i32::MAX, i32::MIN), |(lo, hi), k| (lo.min(k.0), hi.max(k.1)));
    let c_w = c.w as usize;
    let px = c.pixels_mut();
    for row in lo.max(0)..hi {
        let line = &mut px[row as usize * c_w..][..c_w];
        for (col, &(r0, r1)) in (c0..).zip(&rows) {
            if row >= r0 && row < r1 {
                line[col as usize] = color;
            }
        }
    }
}

/// The height a turning face is drawn at, `rh` the room's (its height at rest), `sin` how far round it is (0 at rest, 1 half
/// way), `near` the perspective's growth at its nearest edge: a touch smaller while it turns, and so small that the nearest
/// edge (`tall * near`) is no taller than the room. Rows above and below the face inside its picture are the page's ground:
/// past the room they painted over the title line and into the hint bar.
fn turn_height(rh: i32, sin: f32, near: f32) -> f32 {
    (rh as f32 * (1.0 - 0.08 * sin)).min(rh as f32 / near.max(1.0))
}

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
            opened_at: (!dur.is_zero()).then(crate::frame_clock::now),
            spin_at: Some(crate::frame_clock::now()),
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
        let now = crate::frame_clock::now();
        let dur = crate::motion::span(crate::motion::Kind::Move, FLIP);
        let start = match self.flip_at {
            // Turned back halfway: as far back as it had come.
            Some(at) if now.duration_since(at) < dur => now - (dur - now.duration_since(at)),
            _ => now,
        };
        self.back = !self.back;
        self.link = None;
        if !self.back {
            self.spin_at = Some(now);
        }
        self.flip_at = (!dur.is_zero()).then_some(start);
    }

    /// For a snapshot: the last turn done, or `POCKETUI_SNAPSHOT_FLIP_MS` into it; the game out of its case, or
    /// `POCKETUI_SNAPSHOT_OPEN_MS` into its slide.
    pub fn snapshot_settle(&mut self) {
        let spin: Option<u64> = std::env::var("POCKETUI_SNAPSHOT_SPIN_MS").ok().and_then(|v| v.parse().ok());
        self.spin_at = crate::frame_clock::now().checked_sub(Duration::from_millis(spin.unwrap_or(0)));
        let open: Option<u64> = std::env::var("POCKETUI_SNAPSHOT_OPEN_MS").ok().and_then(|v| v.parse().ok());
        self.opened_at = open.and_then(|ms| crate::frame_clock::now().checked_sub(Duration::from_millis(ms)));
        // The open case: `POCKETUI_SNAPSHOT_ALBUM_OPEN_MS` into its opening, `POCKETUI_SNAPSHOT_ALBUM_MOVE_MS` into the last
        // photo's slide; at rest without them.
        if let Some(view) = self.album.as_mut() {
            let at = |var: &str| std::env::var(var).ok().and_then(|v| v.parse::<u64>().ok()).and_then(|ms| crate::frame_clock::now().checked_sub(Duration::from_millis(ms)));
            view.opened_at = at("POCKETUI_SNAPSHOT_ALBUM_OPEN_MS");
            view.moved = view.moved.and_then(|(_, way)| at("POCKETUI_SNAPSHOT_ALBUM_MOVE_MS").map(|t| (t, way)));
        }
        let ms: Option<u64> = std::env::var("POCKETUI_SNAPSHOT_FLIP_MS").ok().and_then(|v| v.parse().ok());
        self.flip_at = match (self.flip_at, ms) {
            (Some(_), Some(ms)) => crate::frame_clock::now().checked_sub(Duration::from_millis(ms)),
            _ => None,
        };
    }

    /// The disc's turn as if the box had come up `by` ago (benches).
    pub fn age_spin(&mut self, by: Duration) {
        self.spin_at = crate::frame_clock::now().checked_sub(by);
    }

    /// The next frame for the turning disc (Playful animations), while it turns: not through `animating`, so a slow
    /// box doesn't ask for a frame at every refresh it can't keep.
    pub fn spin_wake(&self, now: Instant) -> Option<Instant> {
        let turning = self.spins && !self.back && crate::playful::on() && self.spin_at.is_some_and(|at| now.saturating_duration_since(at) < crate::playful::span(crate::playful::SPIN_FOR));
        if !turning {
            return None;
        }
        let refresh = Duration::from_micros(16_667);
        let every = if self.cost > refresh / 2 { refresh * 2 } else { refresh };
        Some(self.painted.map_or(now, |t| t + every - Duration::from_millis(2)))
    }

    pub fn animating(&self, now: Instant) -> bool {
        self.album_animating(now) || self.flip_at.is_some_and(|at| now.duration_since(at) < crate::motion::span(crate::motion::Kind::Move, FLIP))
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

    /// Where fact `i` of the back's links is on the canvas the back was painted on, (x, y, w, h), while it is in view:
    /// for a touch screen's taps (the Android app).
    pub fn link_rect(&self, i: usize) -> Option<(i32, i32, i32, i32)> {
        let (top, x, w) = self.body;
        let &(at, h) = self.link_at.get(i).filter(|(t, _)| *t >= 0)?;
        let y = top + at - self.scroll;
        (self.back && y >= top && y + h <= top + self.view_h).then_some((x, y, w, h))
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

    /// No turn is under way (Down opens the case only from a box at rest).
    pub fn flip_at_rest(&self) -> bool {
        self.flip_at.is_none_or(|at| crate::frame_clock::now().duration_since(at) >= crate::motion::span(crate::motion::Kind::Move, FLIP))
    }

    /// The case lies open with its photos (`album`), animating while it opens or a photo slides.
    pub fn album_animating(&self, now: Instant) -> bool {
        self.album.as_ref().is_some_and(|v| v.animating(now))
    }

    /// Paints the box, front or back (or turning) under the card's top line.
    pub fn paint(&mut self, c: &mut Canvas, text: &mut Text, d: &CardData, pictures: &[CardPicture], inp: &Inputs) {
        let t0 = Instant::now();
        if let (Some(view), Some(data)) = (self.album.as_ref(), d.album.as_ref()) {
            // The case lying open with its photos: the case's own inside, a DVD case in its system's plastic.
            let hull = Hull::of(inp.medium);
            let inside = if hull == Hull::CartBox { crate::album_paint::Inside::Cardboard } else { crate::album_paint::Inside::Plastic };
            let tint = if hull == Hull::Dvd { frame_tint(hull, inp.medium, inp.look) } else { None };
            let (top, bottom, side) = room(c.w, c.h);
            let now = crate::frame_clock::now();
            crate::album_paint::paint(c, text, view, data, inside, tint, &self.album_pics, &mut self.album_made, (side, top, c.w - 2 * side, bottom - top), now);
            (self.painted, self.cost) = (Some(t0), t0.elapsed());
            return;
        }
        self.paint_box(c, text, d, pictures, inp);
        (self.painted, self.cost) = (Some(t0), t0.elapsed());
    }

    fn paint_box(&mut self, c: &mut Canvas, text: &mut Text, d: &CardData, pictures: &[CardPicture], inp: &Inputs) {
        let now = crate::frame_clock::now();
        let dur = crate::motion::span(crate::motion::Kind::Move, FLIP);
        let t = match self.flip_at {
            Some(at) if !dur.is_zero() && now.duration_since(at) < dur => now.duration_since(at).as_secs_f32() / dur.as_secs_f32(),
            _ => {
                self.flip_at = None;
                self.turn_face = None;
                1.0
            }
        };
        if t >= 1.0 {
            self.paint_face(c, text, d, pictures, inp, self.back, None);
            // The box stands open on its front: its back is made now, so a turn only copies it (a turn that made it
            // stalled half way on the Brick, 15.11.).
            if !self.back && self.openness(now) >= 1.0 {
                self.prepare_back(c, text, d, pictures, inp);
            }
            return;
        }
        // Turning: the face going away narrows to its edge, the other comes round from it.
        let showing_back = if t < 0.5 { !self.back } else { self.back };
        let (w, h) = (c.w, c.h);
        let mut scratch = std::mem::take(&mut self.scratch);
        // The face is painted off screen once for each half of a turn: it doesn't change while it turns (a whole frame
        // cleared and painted again each frame was a third of a turning frame on the Brick, 15.11.2026).
        let face = (showing_back, w, h, self.flip_at);
        let rect = match self.turn_face {
            Some((key, rect)) if key == face && scratch.len() == (w * h) as usize => rect,
            _ => {
                scratch.clear();
                scratch.resize((w * h) as usize, pal().void);
                let mut off = Canvas::new(&mut scratch, w as u32, h as u32);
                let rect = self.paint_face(&mut off, text, d, pictures, inp, showing_back, Some(()));
                self.turn_face = Some((face, rect));
                rect
            }
        };
        let (fw0, bw0) = widths(w, h, self.front_w(inp, h));
        let (from_w, to_w) = if self.back { (fw0, bw0) } else { (bw0, fw0) };
        let along = from_w as f32 + (to_w - from_w) as f32 * t;
        // The box turns about its upright axis in perspective (as a case on the shelf turns, `draw_turning`): the face
        // going away recedes, its near edge taller than its far one, and the box's edge comes round between the two faces
        // in its spine colour, as thick as the case is, so it never thins to a line. 11.11.: it was squeezed flat.
        let size = inp.medium.size();
        // As thick as the case itself (its front, not the game beside it), at the case's own proportions.
        let case_w = inp.front.map_or(along * 0.7, |f| f.w as f32).min(along);
        let depth = case_w * (size.spine as f32 / size.front.max(1) as f32).clamp(0.05, 0.1);
        let focal = 2.5 * h as f32;
        let th = std::f32::consts::PI * t;
        let (sin, cos) = th.sin_cos();
        let (half_w, half_d) = (along / 2.0, depth / 2.0);
        // Top view, the box's middle at the origin, z away from you; a corner (x, z) after the turn.
        let rot = |x: f32, z: f32| (x * cos + z * sin, -x * sin + z * cos);
        let cx = (w / 2) as f32;
        let project = |(x, z): (f32, f32)| {
            let k = focal / (focal + z);
            (cx + x * k, k)
        };
        let (rx, ry, rw, rh) = rect;
        let cy = (ry + rh / 2) as f32;
        // The face toward you: the near face (z = -half_d) before half way, the far one (z = +half_d) seen from behind after.
        let (a, b) = if cos >= 0.0 {
            (project(rot(-half_w, -half_d)), project(rot(half_w, -half_d)))
        } else {
            (project(rot(half_w, half_d)), project(rot(-half_w, half_d)))
        };
        // The edge between them: the box's right side faces you all through a half turn (its normal's z is -sin). While the
        // front turns away, that side is the case's own, at the case's right edge, not at the far end of the game out beside
        // it (ritual review 14.11., finding 7: the side stood apart, beyond the cartridge), and the game stands in front of
        // it. Elsewhere it is at the face's edge: the back is the whole box, and a front coming round shows its case's
        // left side there. Half way (cos = 0) both are the same line.
        let front_away = !showing_back && cos >= 0.0;
        let case_part = inp.front.map_or(1.0, |f| f.w as f32 / rw.max(1) as f32).clamp(0.0, 1.0);
        let side_x = if front_away { -half_w + along * case_part } else { half_w };
        let (e0, e1) = (project(rot(side_x, -half_d)), project(rot(side_x, half_d)));
        // A touch smaller while it turns, and never taller at its nearer edge than the room it stands in (`turn_height`):
        // the perspective grew it past the room into the title line and the hint bar (the Brick's wider back, its shorter
        // focal length; the Plus's Game Boy and PSP boxes too: Brick review 20.11.).
        let near = a.1.max(b.1).max(e0.1).max(e1.1);
        let tall = turn_height(rh, sin, near);
        crate::shares::mark("box-room", rx, ry, rw, rh);
        crate::shares::mark("box-turn", rx, (cy - tall * near / 2.0).floor() as i32, rw, (tall * near).ceil() as i32);
        let dark = ((1.0 - cos.abs()) * 120.0) as u32;
        face_columns(c, &scratch, w, (rx, ry, rw, rh), a, b, cy, tall, dark, None);
        // As tall as the case (the back stands the room's height), from the face it leaves to the face it comes to.
        let case_h = inp.front.map_or(rh, |f| f.h.min(rh)) as f32;
        let (h_from, h_to) = if self.back { (case_h, rh as f32) } else { (rh as f32, case_h) };
        let edge_h = (h_from + (h_to - h_from) * t) * tall / rh.max(1) as f32;
        if e1.0 - e0.0 >= 0.5 {
            edge_columns(c, e0, e1, cy, edge_h, mix(inp.look.band, 0x000000, ((1.0 - sin) * 110.0) as u32 + 30));
            if front_away && case_part < 1.0 {
                // The game out beside the case is nearer than the case's side: painted again over it, its part of the face.
                let cut = (rw as f32 * case_part).round() as i32;
                face_columns(c, &scratch, w, (rx + cut, ry, rw - cut, rh), e0, b, cy, tall, dark, Some(pal().void));
            }
        }
        self.scratch = scratch;
    }

    /// The front face's width: the case and its game half out beside it.
    fn front_w(&self, inp: &Inputs, h: i32) -> i32 {
        let hh = front_height(1, h).max(1);
        match inp.front {
            // A sealed case (`shrink_wrap`) keeps its game inside: no room beside it.
            Some(f) if self.sealed.is_some() => f.w,
            Some(f) => f.w + game_beside(inp.medium, (f.w, f.h), inp.game).2,
            None => cartbox_w(hh, inp.cover),
        }
    }

    /// Paints one face; returns its rectangle. `off`: painted off screen for the turn (the game already out of its case,
    /// the disc still).
    fn paint_face(&mut self, c: &mut Canvas, text: &mut Text, d: &CardData, pictures: &[CardPicture], inp: &Inputs, back: bool, off: Option<()>) -> (i32, i32, i32, i32) {
        let (top, bottom, _) = room(c.w, c.h);
        let hh = bottom - top;
        let (fw, bw) = widths(c.w, c.h, self.front_w(inp, c.h));
        let w = if back { bw } else { fw };
        let x = (c.w - w) / 2;
        if back {
            // A soft shadow under the box, as under a case out of its shelf.
            art::soft_shadow(c, x + 12, top + 14, w, hh, 26, 0.5, (0, c.h));
            // The scroll bar turns with the back: the turn shows the very pixels the back has at rest.
            self.paint_back(c, text, d, pictures, inp, (x, top, w, hh), true);
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
        // A sealed case (`shrink_wrap`) stays shut in its film: its game stays inside.
        let out = if self.sealed.is_some() { 0 } else { out };
        let x = (c.w - (cw + out)) / 2;
        let y = top + (hh - ch) / 2;
        let open = if self.sealed.is_some() { 0.0 } else if off.is_some() { 1.0 } else { self.openness(crate::frame_clock::now()) };
        self.front_rect = Some((x, y, cw, ch));
        if self.hide_case {
            return (x, top, cw + out, hh);
        }
        // Inside, its right edge at the case's; open, `out` past it.
        let gx = x + cw - gw + (out as f32 * open) as i32;
        let gy = y + (ch - gh) / 2;
        // Playful animations: a disc out of its case turns slowly for a while (`playful::spin`); a UMD stays in its shell.
        self.spins = off.is_none() && matches!(inp.medium.game(), crate::media::Game::Disc | crate::media::Game::MiniDisc) && crate::playful::on();
        if open > 0.0 {
            art::soft_shadow(c, gx + gw / 2 + 8, gy + 10, gw / 2, gh, 20, 0.4, (0, c.h));
            match inp.game {
                Some(pic) if self.spins && open >= 1.0 => {
                    let turn = self.spin_at.map_or(0.0, |at| crate::playful::spin(crate::playful::secs(at, crate::frame_clock::now())));
                    art::blit_spun(c, pic, gx as f32 + pic.w as f32 / 2.0, gy as f32 + pic.h as f32 / 2.0, turn, (0, c.h));
                }
                Some(pic) => art::blit(c, pic, gx, gy, 256, (0, c.h)),
                None if inp.medium.game() == crate::media::Game::Umd => art::umd(c, gx + gw / 2, gy + gh / 2, gh / 2, inp.look.color, c.h),
                None => art::disc(c, gx + gw / 2, gy + gh / 2, gh / 2, inp.look.color, c.h),
            }
        }
        art::soft_shadow(c, x + 12, y + 14, cw, ch, 26, 0.5, (0, c.h));
        art::blit(c, front, x, y, 256, (0, c.h));
        for sticker in inp.moving {
            sticker.paint(c, (x, y), (0, c.h));
        }
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

    fn paint_back(&mut self, c: &mut Canvas, text: &mut Text, d: &CardData, pictures: &[CardPicture], inp: &Inputs, rect: (i32, i32, i32, i32), bar: bool) {
        // Diagnostics collect the text painted in this frame, not just its pixels.
        if crate::text::boxes::on() || crate::i18n::recording() {
            self.back_pixels.clear();
            self.paint_back_content(c, text, d, pictures, inp, rect, bar);
            return;
        }
        let key = self.back_key(shape(c), text, d, inp, rect, bar);
        if let Some(cache) = self.back_pixels.iter().find(|b| b.matches(&key, d, pictures, inp.console)) {
            cache.copy(c);
            return;
        }
        self.paint_back_content(c, text, d, pictures, inp, rect, bar);
        // A selected connection may just have scrolled into view; the next frame paints that new position.
        if self.scroll != key.scroll { self.back_pixels.clear(); return; }
        // Copy screen cells, including partial cells at the frame's edge: replaying them is exact even at odd places.
        let (x, y, w, h) = rect;
        let (x0, y0, _, _) = c.cell(x.max(0), y.max(0));
        let (x1, y1, _, _) = c.cell((x + w - 1).min(c.w - 1), (y + h - 1).min(c.h - 1));
        let stride = c.stride();
        let rows = c.pixels().len() / stride.max(1);
        let (x0, y0, x1, y1) = (x0.max(0) as usize, y0.max(0) as usize, (x1 + 1).max(0).min(stride as i32) as usize, (y1 + 1).max(0).min(rows as i32) as usize);
        let (w, h) = (x1.saturating_sub(x0), y1.saturating_sub(y0));
        let pixels = (y0..y0 + h).flat_map(|row| c.pixels()[row * stride + x0..][..w].iter().copied()).collect();
        // Two kept: the screen's and the turn's (one where they are the same canvas). The older one makes room.
        self.back_pixels.retain(|b| b.key.canvas != key.canvas);
        if self.back_pixels.len() >= 2 {
            self.back_pixels.remove(0);
        }
        self.back_pixels.push(BackPixels { key, data: d.clone(), pictures: pictures.to_vec(),
            console: inp.console.map(|s| (s.w, s.h, s.px.clone())), area: (x0, y0, w, h), pixels });
    }

    /// What the back's pixels on `c` depend on.
    fn back_key(&self, canvas: CanvasShape, text: &Text, d: &CardData, inp: &Inputs, rect: (i32, i32, i32, i32), bar: bool) -> BackKey {
        BackKey {
            canvas, rect,
            scroll: self.scroll, link: self.link, bar, medium: inp.medium, look: inp.look, sealed: self.sealed,
            palette: pal(), font: text.style(), letters: crate::font_style::retro_letters(),
            sizes: (type_scale::px(24), type_scale::px(28), type_scale::px(37)), lang: crate::i18n::current(),
            rating_hint: crate::game_card_model::no_rating(), translating: text.translating(),
            saved_ago: d.saves.as_ref().and_then(|s| s.newest).map(crate::recents::ago),
            cover: inp.cover.map(Image::fingerprint),
        }
    }

    /// While the box stands on its front: makes its back's pixels for the turn (off screen, at design size) and for the
    /// screen at rest, where they are not kept yet, so turning over never lays out the back or scales its pictures.
    fn prepare_back(&mut self, c: &Canvas, text: &mut Text, d: &CardData, pictures: &[CardPicture], inp: &Inputs) {
        // Diagnostics record only the text of what is shown.
        if crate::text::boxes::on() || crate::i18n::recording() {
            return;
        }
        let (top, bottom, _) = room(c.w, c.h);
        let (_, bw) = widths(c.w, c.h, self.front_w(inp, c.h));
        let rect = ((c.w - bw) / 2, top, bw, bottom - top);
        let (w, h) = (c.w, c.h);
        // The turn's canvas (`paint_box`), then the screen's (the same one where the screen is at design size).
        let turn: CanvasShape = (w, h, 1, w as usize, (w * h) as usize, (0, 0, 0, 0));
        for form in [turn, shape(c)] {
            let key = self.back_key(form, text, d, inp, rect, true);
            if self.back_pixels.iter().any(|b| b.matches(&key, d, pictures, inp.console)) {
                continue;
            }
            let (fw, fh, n, stride, len, cell) = form;
            // A part of a screen (a canvas of some of its rows) can't be stood in for.
            if cell != (0, 0, 0, 0) || stride == 0 || len % stride != 0 || (fw, fh) != ((stride * n as usize) as i32, (len / stride * n as usize) as i32) {
                continue;
            }
            let mut px = std::mem::take(&mut self.scratch);
            self.turn_face = None;
            px.clear();
            px.resize(len, pal().void);
            let mut canvas = Canvas::screen(&mut px, (stride as u32, (len / stride) as u32), n as u32);
            self.paint_back(&mut canvas, text, d, pictures, inp, rect, true);
            self.scratch = px;
        }
    }

    fn paint_back_content(&mut self, c: &mut Canvas, text: &mut Text, d: &CardData, pictures: &[CardPicture], inp: &Inputs, (x, y, w, h): (i32, i32, i32, i32), bar: bool) {
        self.content_paints += 1;
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
        self.body = (top, px, pw);
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
        let down: Vec<u8> = (0..fade.max(0)).map(|k| (255 - k * 255 / fade.max(1)) as u8).collect();
        if self.scroll > 0 {
            c.shade_rows(px, top, pw, paper, &down);
        }
        if self.scroll < self.max_scroll {
            let up: Vec<u8> = down.iter().rev().copied().collect();
            c.shade_rows(px, bot - fade.max(0), pw, paper, &up);
        }
        if bar && self.max_scroll > 0 {
            let bar_h = (view_h * view_h / (view_h + self.max_scroll)).max(30);
            let bar_y = top + (view_h - bar_h) * self.scroll / self.max_scroll.max(1);
            c.rect(px + pw - 8, top, 4, view_h, mix(paper, ink, 40));
            c.rect(px + pw - 8, bar_y, 4, bar_h, soft);
        }
        // A sealed game's back is in the same film as its front, over the frame and the print alike.
        if let Some(seed) = self.sealed {
            let key = (w, h, seed);
            if self.film.as_ref().is_none_or(|(k, _)| *k != key) {
                // The frame's middle is painted here, not in the frame's picture: the film covers it too.
                let mut shape = frame.clone();
                shape.fill(b, b, w - 2 * b, h - 2 * b, 0xff00_0000);
                // The grain of the front beside it (a template pixel about a 240th of the box), the glare softer: the back
                // is mostly text to read.
                self.film = Some((key, crate::shrink_wrap::layer(&shape, (h as f32 / 240.0).max(1.0), seed, crate::shrink_wrap::Glare::Back)));
            }
            if let Some((_, film)) = &self.film {
                art::blit(c, film, x, y, 256, (0, c.h));
            }
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
    // Where the player is, read from the game's own save (`case_knows`): right under the title, across the back, seen
    // without scrolling, like a label with the save written on it. Already in the interface's language: painted as it is.
    if let Some(progress) = &d.progress {
        y = paint_progress(c, text, progress, d, (inner_x, y + type_scale::grow(28) + 22, inner_w), ink, soft, paper);
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
        paint_shot(c, text, (inner_x, y, w, h), None, inp.console, paper, crate::platform::screenshot_hint());
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
    // The label column holds the widest label: a face wider than Barlow (Settings → Look → Style) or a longer German word.
    let gap = text.width(Face::Body, 28, "  ");
    let key_w = facts.iter().map(|(k, _)| text.width(Face::Body, 28, k) + gap).fold(key_w, i32::max).min(col_w / 2);
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
    let played = d.played.map_or(d.not_played().to_string(), |(_, secs)| crate::play_time::duration(secs));
    let fitted = text.fit(Face::Body, 28, &played, rw);
    text.draw(c, Face::Body, 28, rx + rkey, ry, ink, &fitted);
    ry += row;
    text.draw(c, Face::Body, 28, rx, ry, soft, "Rating");
    if d.rating > 0 {
        paint_stars(c, rx + rkey + 12, ry - 10, 11, d.rating, ink, mix(paper, ink, 60));
    } else {
        let fitted = text.fit(Face::Body, 28, crate::game_card_model::no_rating(), rw);
        text.draw(c, Face::Body, 28, rx + rkey, ry, soft, &fitted);
    }
    ry += row;
    if let Some(a) = d.achievements.as_ref().filter(|_| !d.progress_achievements) {
        text.draw(c, Face::Body, 28, rx, ry, soft, "Unlocked");
        let fitted = text.fit(Face::Body, 28, a, rw);
        text.draw(c, Face::Body, 28, rx + rkey, ry, ink, &fitted);
        ry += row;
    }
    // Its save slots on their medium, read-only, as on the info page (`save_media`; pocket-core's games); the cover on
    // a cartridge's label (the slots stay plain: at this size a thumbnail of the cover is only noise).
    if let Some(saves) = &d.saves {
        let n = saves.filled.iter().filter(|f| **f).count();
        text.draw(c, Face::Body, 28, rx, ry, soft, "Save slots");
        let (value, color) = if n > 0 { (format!("{n} of {}", saves.filled.len()), ink) } else { ("All empty".to_string(), soft) };
        let fitted = text.fit(Face::Body, 28, &value, rw);
        text.draw(c, Face::Body, 28, rx + rkey, ry, color, &fitted);
        let kind = crate::save_media::Kind::of(&saves.system);
        let l = crate::save_media::Layout::fitted(kind, saves.filled.len(), col_w, 2 * row - 12);
        let slots = crate::save_media::Slots { filled: &saves.filled, ring: None, icon: crate::save_media::Icon::None, label: inp.cover, stripe: saves.stripe, numbers: l.block >= 20 };
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

/// The case-knows line at `y` (its first baseline) and the team, badges or measure under it (`case_knows_art`); returns
/// where the pictures under it start.
#[allow(clippy::too_many_arguments)]
fn paint_progress(c: &mut Canvas, text: &mut Text, progress: &str, d: &CardData, (x, mut y, w): (i32, i32, i32), ink: u32, soft: u32, paper: u32) -> i32 {
    let line = type_scale::line(28, 8);
    for l in &crate::case_knows_read::lines(text, Face::Body, 28, progress, w) {
        text.draw_name(c, Face::Body, 28, x, y, ink, l);
        y += line;
    }
    let k = crate::case_knows_art::scale();
    let top = y - line + type_scale::base(10, 28) + 6 * k;
    let h = crate::case_knows_art::paint_strip(c, d.case_art.as_deref(), d.case_meter, (x, top, w), k, (ink, soft, paper));
    if h > 0 { top + h + 2 * k + line / 2 } else { y - line + type_scale::base(10, 28) + line / 2 }
}

/// The description and the line that credits it, as the back shows them (and the info page, `game_info`); None when
/// there is no description to show.
pub(crate) fn about(d: &CardData) -> (Option<String>, Option<String>) {
    // The short card: one sentence of the player's own description.
    if let (true, Summary::Gamelist(t) | Summary::Romm(t)) = (d.plain, &d.summary) {
        let first = t.split_inclusive(['.', '!', '?']).next().unwrap_or(t).trim().to_string();
        return (Some(first).filter(|f| !f.is_empty()), None);
    }
    let about = match &d.summary {
        Summary::Wikipedia { text: t, .. } | Summary::Gamelist(t) | Summary::Romm(t) => Some(t.clone()),
        _ => d.sentence.clone(),
    };
    // The short card has no line for a game that simply has no article.
    let credit = d.summary.note().filter(|_| !(d.plain && d.summary == Summary::None));
    // The sentence made of the facts (`game_card::sentence`) is English only, and the facts under it say the same in
    // the player's language: in any other language the back has no description then rather than an English one.
    let made_up = matches!(d.summary, Summary::None | Summary::Loading | Summary::Unavailable(_));
    let about = about.filter(|_| !(made_up && crate::i18n::current() != crate::i18n::Lang::En));
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
    fn a_turning_face_is_never_taller_than_its_room() {
        // At rest it is the room's height; half way round a little smaller; and however near its edge comes (the
        // Brick's focal length with its wider back grew it 1.22 times half way), that edge stays in the room.
        assert_eq!(turn_height(600, 0.0, 1.0), 600.0);
        assert!((turn_height(600, 1.0, 1.0) - 552.0).abs() < 0.01);
        for near in [1.0, 1.05, 1.16, 1.22, 1.5] {
            for sin in [0.0, 0.3, 0.7, 1.0] {
                assert!(turn_height(600, sin, near) * near <= 600.0 + 0.01, "near {near} sin {sin}");
            }
        }
        // Where the touch smaller is enough (the Plus's SNES box), it stays as it was.
        assert!((turn_height(776, 1.0, 1.05) - 776.0 * 0.92).abs() < 0.01);
    }

    #[test]
    fn a_box_back_keeps_up_with_descriptions_pictures_scrolling_and_unwrapping() {
        let mut text = Text::builtin();
        let mut data = CardData { title: "A new game".into(), system: "PlayStation".into(),
            summary: Summary::Gamelist("The first description. ".repeat(80)), ..Default::default() };
        let mut pictures = Vec::new();
        let inp = Inputs { medium: Medium::Ps1, look: Look { color: 0x304050, band: 0x8090a0, plastic: 0x202020, tape: false },
            front: None, game: None, cover: None, console: None, moving: &[] };
        let render = |view: &mut BoxView, text: &mut Text, data: &CardData, pictures: &[CardPicture], n| {
            let mut px = vec![pal().void; (1280 / n * (960 / n)) as usize];
            let mut c = Canvas::screen(&mut px, ((1280 / n) as u32, (960 / n) as u32), n as u32);
            view.paint_back(&mut c, text, data, pictures, &inp, (21, 31, 550, 800), true);
            px
        };
        for n in [1, 2] {
            let mut view = BoxView { back: true, sealed: Some(5), ..Default::default() };
            for change in 0..6 {
                match change {
                    1 => { data.title = "The corrected title".into(); data.summary = Summary::Gamelist("The downloaded description. ".repeat(80)); }
                    2 => { view.scroll_by(160); }
                    3 => pictures.push(CardPicture { label: "In the game", size: (80, 60),
                        image: Some(std::rc::Rc::new(Image { w: 80, h: 60, px: vec![0x80a0c0; 80 * 60] })) }),
                    4 => view.sealed = None,
                    5 => {
                        data.facts = vec![("Developer", "Other games".into())];
                        data.links = vec![(0, crate::game_card::CardLink { title: "Other games".into(), games: Vec::new() })];
                        view.link = Some(0);
                        // The first frame brings a connection far down the description into view.
                        render(&mut view, &mut text, &data, &pictures, n);
                    }
                    _ => {}
                }
                let mut fresh = BoxView { back: true, scroll: view.scroll, link: view.link, sealed: view.sealed, ..Default::default() };
                let expected = render(&mut fresh, &mut text, &data, &pictures, n);
                assert_eq!(render(&mut view, &mut text, &data, &pictures, n), expected, "change {change}, n={n}");
                assert_eq!(render(&mut view, &mut text, &data, &pictures, n), expected, "next frame, change {change}, n={n}");
                if change == 5 { assert!(view.link_rect(0).is_some(), "the selected connection remains visible and tappable after a cached frame"); }
            }
        }
    }

    /// A box art with a hue nothing else in the box uses: magenta and blue squares (not one flat colour, which the
    /// label's band trimming would take for a margin).
    fn magenta_cover() -> Image {
        let px = (0..160 * 120).map(|i| if (i % 160 / 8 + i / 160 / 8) % 2 == 0 { 0xd0_20_80 } else { 0x20_80_d0 }).collect();
        Image { w: 160, h: 120, px }
    }

    fn is_magenta(p: u32) -> bool {
        let (r, g, b) = (p >> 16 & 0xff, p >> 8 & 0xff, p & 0xff);
        r > 0xb0 && g < 0x50 && (0x60..0xa0).contains(&b)
    }

    /// The box's back with its save slots on the medium of `system`, scrolled to "Your game", at `n` x on a 1280x960
    /// panel; `cover` on the cartridge's label or none. Returns the pixels.
    fn back_with_saves(system: &str, medium: Medium, cover: Option<&Image>, n: i32) -> Vec<u32> {
        let mut text = Text::builtin();
        let data = CardData { title: "A game".into(), system: system.into(),
            saves: Some(crate::game_card_model::CardSaves { system: system.into(), stripe: 0x4a7a3a, filled: vec![true, false, true, false, false, true, false, false], newest: Some(1) }),
            ..Default::default() };
        let inp = Inputs { medium, look: Look { color: 0x304050, band: 0x8090a0, plastic: 0x206040, tape: false }, front: None, game: None, cover, console: None, moving: &[] };
        let (w, h) = ((1280 / n) as u32, (960 / n) as u32);
        let mut px = vec![pal().void; (w * h) as usize];
        let mut c = Canvas::screen(&mut px, (w, h), n as u32);
        let mut view = BoxView { back: true, scroll: i32::MAX / 2, ..Default::default() };
        // The first paint clamps the scroll to the end of the page, the second shows it.
        for _ in 0..2 {
            view.paint_back(&mut c, &mut text, &data, &[], &inp, (21, 31, 550, 800), true);
        }
        px
    }

    #[test]
    fn the_small_cartridge_beside_save_slots_wears_the_games_label() {
        // Every cartridge system with its own template, on both panel sizes: with box art its label is printed with it;
        // without, the stripe's plain label stays; only the cartridge's own pixels change.
        let cover = magenta_cover();
        for (system, medium) in [("gba", Medium::Gba), ("gbc", Medium::Gb), ("snes", Medium::Snes), ("n64", Medium::N64)] {
            for n in [1, 2] {
                let without = back_with_saves(system, medium, None, n);
                let with = back_with_saves(system, medium, Some(&cover), n);
                let printed = |px: &[u32]| px.iter().filter(|p| is_magenta(**p)).count();
                assert!(printed(&without) == 0, "{system} x{n}: no art, no art on the label");
                assert!(printed(&with) > 60 / n as usize, "{system} x{n}: the cover is on the cartridge's label ({} pixels)", printed(&with));
                // The cover changes the cartridge's label and nothing else: one small block, the slots stay as they were.
                let diff: Vec<usize> = (0..with.len()).filter(|i| with[*i] != without[*i]).collect();
                assert!(!diff.is_empty(), "{system} x{n}");
                let w = (1280 / n) as usize;
                let (x0, x1) = (diff.iter().map(|i| i % w).min().unwrap(), diff.iter().map(|i| i % w).max().unwrap());
                let (y0, y1) = (diff.iter().map(|i| i / w).min().unwrap(), diff.iter().map(|i| i / w).max().unwrap());
                assert!(x1 - x0 < 200 / n as usize && y1 - y0 < 160 / n as usize, "{system} x{n}: only the mini medium changes: {x0}..{x1} x {y0}..{y1}");
            }
        }
    }

    #[test]
    fn the_short_card_keeps_one_sentence_of_a_local_description() {
        let d = CardData { plain: true, summary: Summary::Gamelist("A kart race. Then more about it.".into()), ..Default::default() };
        assert_eq!(about(&d), (Some("A kart race.".to_string()), None));
    }

    #[test]
    fn no_article_and_offline_say_different_things() {
        let credit = |summary: Summary, plain: bool| about(&CardData { plain, summary, ..Default::default() }).1;
        // A game without an article: it says so, instead of a blank.
        assert_eq!(credit(Summary::None, false).as_deref(), Some("No Wikipedia article for this game"));
        assert_eq!(credit(Summary::Unavailable("no article".into()), false), credit(Summary::None, false));
        // A game with an article but no Wi-Fi yet: the text comes later.
        for why in ["offline", "no curl"] {
            assert_eq!(credit(Summary::Unavailable(why.into()), false).as_deref(), Some("Offline: the description comes with Wi-Fi"));
        }
        assert_eq!(credit(Summary::Unavailable("timeout".into()), false).as_deref(), Some("No description from Wikipedia: timeout"));
        // The short card (Easy, Kids) leaves the line out for a plain game without an article.
        assert_eq!(credit(Summary::None, true), None);
        // The description text is not touched: the facts' sentence is English only, so another language shows none.
        let d = CardData { sentence: Some("A game.".into()), summary: Summary::None, ..Default::default() };
        assert_eq!(about(&d).0.is_some(), crate::i18n::current() == crate::i18n::Lang::En);
    }

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

    /// The box on a 640x480 screen as the event loop paints it: a frame when a press comes, then frames while
    /// `rule(moving, shown_moving)` asks for them, each `cost` long (the frame clock frozen and stepped by it). Returns
    /// the screen when no more frames come and the view.
    fn turn_on_a_screen(medium: Medium, cost: Duration, presses: &[u64], rule: &dyn Fn(&crate::frame_pace::Settle, bool) -> bool) -> (Vec<u32>, BoxView, Instant) {
        let mut text = Text::builtin();
        // Blue Stinger: a Dreamcast disc, a long Wikipedia description and no picture for the back (dArkOS test-6).
        let data = CardData { title: "Blue Stinger".into(), system: "Dreamcast".into(),
            summary: Summary::Gamelist("Blue Stinger is a survival horror game set on Dinosaur Island. ".repeat(40)), ..Default::default() };
        let inp = Inputs { medium, look: Look { color: 0x304050, band: 0x8090a0, plastic: 0x202020, tape: false },
            front: None, game: None, cover: None, console: None, moving: &[] };
        let (w, h) = (640u32, 480u32);
        let mut px = vec![pal().void; (w * h) as usize];
        let mut view = BoxView::default();
        let t0 = crate::frame_clock::hold();
        let mut now = t0;
        let mut settle = crate::frame_pace::Settle::default();
        let mut next = 0;
        for _ in 0..400 {
            // A press is an event: it turns the box and asks for a frame.
            let mut pressed = false;
            while next < presses.len() && t0 + Duration::from_millis(presses[next]) <= now {
                view.flip();
                pressed = true;
                next += 1;
            }
            if !pressed && !rule(&settle, view.animating(now)) {
                // No frame asked for: the loop sleeps until the next press, or the screen stays as it is.
                match presses.get(next) {
                    Some(&ms) => { now = now.max(t0 + Duration::from_millis(ms)); crate::frame_clock::freeze(now); continue; }
                    None => break,
                }
            }
            let moving = view.animating(now);
            // The card paints its page under the box first.
            px.fill(pal().void);
            view.paint(&mut Canvas::new(&mut px, w, h), &mut text, &data, &[], &inp);
            settle.painted(moving);
            now += cost;
            crate::frame_clock::freeze(now);
        }
        crate::frame_clock::thaw();
        (px, view, now)
    }

    /// The tester's RK3566 (dArkOS test-6, 15.11.2026): the box stayed half turned, its back's text skewed, until the next
    /// button. The loop asked for frames only while the turn ran; a slow screen's last frame was painted half way and
    /// nothing painted the end. Every medium shape, fast to very slow paints, presses again while it turns: the screen
    /// always ends on a face at rest, the one the presses turned it to.
    #[test]
    fn on_a_slow_screen_the_turn_always_ends_on_a_face_at_rest() {
        let wants = |s: &crate::frame_pace::Settle, moving: bool| s.wants(moving);
        for medium in [Medium::SegaDisc, Medium::Snes, Medium::Umd] {
            for cost in [16, 45, 150, 260].map(Duration::from_millis) {
                // Once; twice (back again half way); three times quickly.
                for presses in [&[0][..], &[0, 100], &[0, 40, 120]] {
                    let (screen, view, now) = turn_on_a_screen(medium, cost, presses, &wants);
                    let what = format!("{medium:?}, {cost:?} a frame, presses at {presses:?} ms");
                    assert_eq!(view.back, presses.len() % 2 == 1, "{what}");
                    assert!(view.flip_at.is_none(), "{what}: the last frame shows the turn done");
                    // The same box at rest, painted at the same moment.
                    let mut rest = BoxView { back: view.back, spin_at: view.spin_at, ..Default::default() };
                    let mut text = Text::builtin();
                    let data = CardData { title: "Blue Stinger".into(), system: "Dreamcast".into(),
                        summary: Summary::Gamelist("Blue Stinger is a survival horror game set on Dinosaur Island. ".repeat(40)), ..Default::default() };
                    let inp = Inputs { medium, look: Look { color: 0x304050, band: 0x8090a0, plastic: 0x202020, tape: false },
                        front: None, game: None, cover: None, console: None, moving: &[] };
                    let mut px = vec![pal().void; 640 * 480];
                    crate::frame_clock::freeze(now - cost);
                    rest.paint(&mut Canvas::new(&mut px, 640, 480), &mut text, &data, &[], &inp);
                    crate::frame_clock::thaw();
                    assert!(screen == px, "{what}: the screen is the box at rest, not a frame of its turn");
                }
            }
        }
        // The old rule (frames only while it moves) leaves the slow screen on an in-between frame: what the tester saw.
        let (_, view, _) = turn_on_a_screen(Medium::SegaDisc, Duration::from_millis(150), &[0], &|_, moving| moving);
        assert!(view.flip_at.is_some(), "frames only while it moves: the last one shows the box half turned");
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

    /// Thread CPU time (a bench on a device doesn't count the other programs' time).
    fn thread_cpu() -> Duration {
        let mut ts = libc::timespec { tv_sec: 0, tv_nsec: 0 };
        // SAFETY: a valid out pointer for the call.
        unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &mut ts) };
        Duration::new(ts.tv_sec as u64, ts.tv_nsec as u32)
    }

    /// Kirby's Dream Course as the Brick's tester had it: a SNES box with its cover, two screenshots, a description and
    /// the year on its back.
    fn kirby() -> (CardData, Vec<CardPicture>, Image) {
        let data = CardData { title: "Kirby's Dream Course".into(), system: "Super Nintendo".into(), year: Some(1994),
            summary: Summary::Gamelist("Kirby's Dream Course is a golf game in which Kirby is the ball: shoot him across isometric courses floating in the sky, hit the enemies on the way and the last one becomes the hole. Copy abilities from the enemies help on the way. ".repeat(3)),
            facts: vec![("Developer", "HAL Laboratory".into()), ("Publisher", "Nintendo".into()), ("Genre", "Sports, Golf".into())],
            players: Some("1-2".into()), ..Default::default() };
        let (bw, bh) = crate::game_card::picture_box(1024);
        let (fw, fh) = crate::picture_loader::fitted(256, 224, bw, bh);
        let shot = |label, seed: u32| CardPicture { label, size: (fw, fh),
            image: Some(std::rc::Rc::new(Image { w: fw as u32, h: fh as u32, px: (0..(fw * fh) as u32).map(|i| i.wrapping_mul(2654435761) ^ seed).collect() })) };
        let cover = Image { w: 256, h: 180, px: (0..256 * 180u32).map(|i| i.wrapping_mul(40503) & 0xffffff).collect() };
        (data, vec![shot("In the game", 0x123456), shot("Title screen", 0x654321)], cover)
    }

    /// The box turned over `turns` times on a screen `w` x `h`, a still second between turns, the clock stepped by the
    /// refreshes each frame's paint took. Per turn: the thread CPU time of its frames (the one after the last frame that
    /// moved included) and how often the back was painted in them.
    fn turns_measured(w: u32, h: u32, turns: usize) -> Vec<(bool, Vec<Duration>, u32)> {
        turns_on((w, h, 1), turns)
    }

    /// As `turns_measured` on a panel `w` x `h` painted at `n` design pixels a screen pixel.
    fn turns_on((w, h, n): (u32, u32, u32), turns: usize) -> Vec<(bool, Vec<Duration>, u32)> {
        let mut text = Text::builtin();
        let (data, pictures, cover) = kirby();
        let inp = Inputs { medium: Medium::Snes, look: Look { color: 0x6a3a8a, band: 0xd0a0e0, plastic: 0x202020, tape: false },
            front: None, game: None, cover: Some(&cover), console: None, moving: &[] };
        let mut px = vec![pal().void; (w * h) as usize];
        crate::frame_clock::hold();
        let mut view = BoxView::new();
        let refresh = crate::frame_clock::frames(1);
        let mut paint = |view: &mut BoxView, px: &mut Vec<u32>| {
            px.fill(pal().void);
            let c0 = thread_cpu();
            view.paint(&mut Canvas::screen(px, (w, h), n), &mut text, &data, &pictures, &inp);
            let cost = thread_cpu().saturating_sub(c0);
            crate::frame_clock::step(crate::frame_clock::frames(1 + (cost.as_nanos() / refresh.as_nanos()) as u32));
            cost
        };
        // The box comes up and opens.
        while view.animating(crate::frame_clock::now()) {
            paint(&mut view, &mut px);
        }
        paint(&mut view, &mut px);
        let mut out = Vec::new();
        // POCKETUI_BENCH_IDLE_MS: a real pause before each turn, as a player's (a device's CPU clock drops meanwhile).
        let idle = std::env::var("POCKETUI_BENCH_IDLE_MS").ok().and_then(|v| v.parse().ok()).map(Duration::from_millis);
        let khz = || std::fs::read_to_string("/sys/devices/system/cpu/cpu0/cpufreq/scaling_cur_freq").ok().and_then(|v| v.trim().parse::<u32>().ok()).unwrap_or(0);
        for _ in 0..turns {
            crate::frame_clock::step(Duration::from_secs(1));
            if let Some(idle) = idle {
                std::thread::sleep(idle);
            }
            let clock = khz();
            view.flip();
            let before = view.content_paints;
            let mut times = Vec::new();
            loop {
                let moving = view.animating(crate::frame_clock::now());
                times.push(paint(&mut view, &mut px));
                if !moving {
                    break;
                }
            }
            if clock > 0 {
                println!("clock {} MHz at the press, {} MHz after the turn", clock / 1000, khz() / 1000);
            }
            out.push((view.back, times, view.content_paints - before));
        }
        crate::frame_clock::thaw();
        out
    }

    #[test]
    fn the_turn_paints_row_by_row_what_it_painted_column_by_column() {
        // The turn's faces and edge at n = 1, against the column-by-column loops they replaced (15.11.2026: 150-260 ms a
        // frame on the Brick's A53), for both faces, both sides of half way and a few widths and sizes.
        let (w, h) = (300, 220);
        let picture: Vec<u32> = (0..(w * h) as u32).map(|i| i.wrapping_mul(2654435761) & 0xffffff).collect();
        let old_face = |c: &mut Canvas, a: (f32, f32), b: (f32, f32), cy: f32, hh: f32, dark: u32| {
            let rect = (40, 20, 200, 180);
            let (l, r) = if a.0 <= b.0 { (a, b) } else { (b, a) };
            let flip = a.0 > b.0;
            let (c0, c1) = ((l.0.floor() as i32).max(0), (r.0.ceil() as i32).min(c.w));
            for col in c0..c1 {
                let t = ((col as f32 + 0.5 - l.0) / (r.0 - l.0)).clamp(0.0, 1.0);
                let u = t * r.1 / ((1.0 - t) * l.1 + t * r.1);
                let u = if flip { 1.0 - u } else { u };
                let ch = hh * (l.1 + (r.1 - l.1) * t);
                let top = cy - ch / 2.0;
                let sx = (rect.0 + (u * rect.2 as f32) as i32).clamp(rect.0, rect.0 + rect.2 - 1);
                let step = rect.3 as f32 / ch.max(1.0);
                for row in (top.floor() as i32).max(0)..((top + ch).ceil() as i32).min(c.h) {
                    let sy = (rect.1 + ((row as f32 + 0.5 - top) * step) as i32).clamp(rect.1, rect.1 + rect.3 - 1);
                    let i = (row * c.w + col) as usize;
                    c.pixels_mut()[i] = mix(picture[(sy * w + sx) as usize], 0x000000, dark);
                }
            }
        };
        let old_edge = |c: &mut Canvas, a: (f32, f32), b: (f32, f32), cy: f32, hh: f32, color: u32| {
            let (c0, c1) = ((a.0.floor() as i32).max(0), (b.0.ceil() as i32).min(c.w));
            for col in c0..c1 {
                let t = ((col as f32 + 0.5 - a.0) / (b.0 - a.0).max(0.01)).clamp(0.0, 1.0);
                let ch = hh * (a.1 + (b.1 - a.1) * t);
                let top = cy - ch / 2.0;
                for row in (top.round() as i32).max(0)..((top + ch).round() as i32).min(c.h) {
                    let i = (row * c.w + col) as usize;
                    c.pixels_mut()[i] = color;
                }
            }
        };
        for (a, b, hh, dark) in [((30.0, 1.0), (270.0, 1.0), 180.0, 0), ((52.3, 1.08), (201.7, 0.93), 190.0, 40), ((250.0, 0.9), (60.5, 1.1), 230.0, 100), ((140.2, 1.0), (140.9, 1.0), 100.0, 7)] {
            let bg: Vec<u32> = (0..(w * h) as u32).map(|i| i * 7 & 0xffffff).collect();
            let (mut want, mut got) = (bg.clone(), bg.clone());
            old_face(&mut Canvas::new(&mut want, w as u32, h as u32), a, b, 110.0, hh, dark);
            face_columns(&mut Canvas::new(&mut got, w as u32, h as u32), &picture, w, (40, 20, 200, 180), a, b, 110.0, hh, dark, None);
            assert!(got == want, "face {a:?} {b:?}");
            let (l, r) = if a.0 <= b.0 { (a, b) } else { (b, a) };
            old_edge(&mut Canvas::new(&mut want, w as u32, h as u32), l, r, 110.0, hh, 0x123456);
            edge_columns(&mut Canvas::new(&mut got, w as u32, h as u32), l, r, 110.0, hh, 0x123456);
            assert!(got == want, "edge {l:?} {r:?}");
        }
    }

    #[test]
    fn a_turn_copies_the_back_made_while_the_box_stood_open() {
        // The Brick (1024x768), a 720x720 screen and a 640x480 one painted natively: the turn's back is off screen at
        // design size, the back at rest on the screen. 15.11. (Brick, Kirby's Dream Course): every turn laid out the back
        // and scaled its screenshots in one of its frames, the first turn twice, half way through.
        for panel in [(1024, 768, 1), (720, 720, 1), (640, 480, 2)] {
            for (k, (back, times, paints)) in turns_on(panel, 4).into_iter().enumerate() {
                assert_eq!(paints, 0, "{panel:?}, turn {k} to the {}: the back laid out in its {} frames", if back { "back" } else { "front" }, times.len());
            }
        }
    }

    #[test]
    fn a_turn_from_kept_pixels_looks_as_one_painted_afresh() {
        let mut text = Text::builtin();
        let (data, pictures, cover) = kirby();
        let inp = Inputs { medium: Medium::Snes, look: Look { color: 0x6a3a8a, band: 0xd0a0e0, plastic: 0x202020, tape: false },
            front: None, game: None, cover: Some(&cover), console: None, moving: &[] };
        for (w, h, n) in [(1024u32, 768u32, 1u32), (640, 480, 2)] {
            let t0 = crate::frame_clock::hold();
            let mut view = BoxView::new();
            let mut px = vec![pal().void; (w * h) as usize];
            let paint = |view: &mut BoxView, text: &mut Text, px: &mut Vec<u32>| {
                px.fill(pal().void);
                view.paint(&mut Canvas::screen(px, (w, h), n), text, &data, &pictures, &inp);
            };
            crate::frame_clock::freeze(t0 + Duration::from_secs(1));
            paint(&mut view, &mut text, &mut px);
            // There and back again, a frame every 40 ms; each frame against a box that keeps nothing, at the same moment.
            for turn in 0..3 {
                crate::frame_clock::step(Duration::from_secs(1));
                view.flip();
                for ms in (0..=320).step_by(40) {
                    paint(&mut view, &mut text, &mut px);
                    let mut fresh = BoxView { back: view.back, flip_at: view.flip_at, spin_at: view.spin_at, scroll: view.scroll, ..Default::default() };
                    let mut want = vec![pal().void; (w * h) as usize];
                    paint(&mut fresh, &mut text, &mut want);
                    assert!(px == want, "{w}x{h}, turn {turn}, {ms} ms: the kept back differs from one painted afresh");
                    crate::frame_clock::step(Duration::from_millis(40));
                }
            }
            crate::frame_clock::thaw();
        }
    }

    /// `cargo test --release --lib game_box::tests::bench_turns -- --ignored --nocapture` (POCKETUI_BENCH_TURNS, default 20;
    /// POCKETUI_BENCH_SIZE, default the Brick's 1024x768): every turn's frames in ms.
    #[test]
    #[ignore]
    fn bench_turns() {
        let n = std::env::var("POCKETUI_BENCH_TURNS").ok().and_then(|v| v.parse().ok()).unwrap_or(20);
        let (w, h) = std::env::var("POCKETUI_BENCH_SIZE").ok().and_then(|v| v.split_once('x').and_then(|(a, b)| Some((a.parse().ok()?, b.parse().ok()?)))).unwrap_or((1024, 768));
        let turns = turns_measured(w, h, n);
        let ms = |d: &Duration| d.as_secs_f64() * 1000.0;
        let (mut first, mut later) = (Vec::new(), Vec::new());
        for (k, (back, times, paints)) in turns.iter().enumerate() {
            let list: Vec<String> = times.iter().map(|t| format!("{:.1}", ms(t))).collect();
            println!("turn {k} to {}: frames {} worst {:.1} ms, back painted {paints}x: [{}]", if *back { "back" } else { "front" }, times.len(), times.iter().map(ms).fold(0.0, f64::max), list.join(" "));
            if k == 0 { first.extend(times.iter().map(ms)) } else { later.extend(times.iter().map(ms)) }
        }
        for (what, mut v) in [("first turn", first), ("later turns", later)] {
            v.sort_by(f64::total_cmp);
            let at = |q: f64| v[((v.len() - 1) as f64 * q).round() as usize];
            println!("{what}: frames {} p50 {:.1} p95 {:.1} worst {:.1} ms, over 16.7 ms: {}", v.len(), at(0.5), at(0.95), at(1.0), v.iter().filter(|x| **x > 16.7).count());
        }
    }
}
