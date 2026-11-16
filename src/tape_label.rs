//! The home-made label of a game without box art (romhacks, homebrew, unknown dumps; 0.5): a strip of masking tape
//! on the blank cartridge, case front and spine with the game's name written on it in marker, a burned CD-R with the
//! name on it, a plain insert in the disc case. Real art always wins: only a game for which pocket-ui finds no cover
//! file at all is drawn this way (`Look::tape`, set by the Cabinet's `case_look`); once a cover arrives the case is
//! made again with it.
//!
//! The tape is a photo (`assets/media/tape/tape.png`, the CC0 photo and how it was cut in `assets/media/tape/SOURCES.md`),
//! the writing Caveat Brush (`assets/fonts/CaveatBrush-Regular.ttf`, SIL OFL 1.1, Latin cut). Every game has its own
//! tilt, tape length, torn ends, writing jitter and pen (black or blue), worked out from its name (`seed`), so the same
//! game looks the same on every screen and after every start. A name the hand font has no letters for (Japanese) is
//! written in the interface's title face on the same tape: never an empty label.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use crate::cabinet_art::Pic;
use crate::gfx::{mix, Canvas};
use crate::text::{Face, Text};

const FONT: &[u8] = include_bytes!("../assets/fonts/CaveatBrush-Regular.ttf");
const TAPE: &[u8] = include_bytes!("../assets/media/tape/tape.png");

/// Masking tape's cream where the photo is not decoded (never in practice; tests check it decodes).
const TAPE_CREAM: u32 = 0xe7d8b0;
/// The two marker pens a game is written with: black (most), dark blue.
const INKS: [u32; 2] = [0x1f1e24, 0x1d2a6e];

/// The plain colours of a home-made game's case (`media::case_look`): a cardboard box of plain kraft, the paper of a
/// disc case's insert. The band of pocket-ui's own case keeps the system's colour.
pub const KRAFT: u32 = 0xa58b66;
pub const INSERT: u32 = 0xe9e5da;
/// The blank label recess of a cartridge with no label: the plastic a little darker (`media::game_pic`).
pub const RECESS_DARKEN: u32 = 26;

fn font() -> &'static fontdue::Font {
    static F: OnceLock<fontdue::Font> = OnceLock::new();
    F.get_or_init(|| fontdue::Font::from_bytes(FONT, fontdue::FontSettings::default()).expect("Caveat Brush"))
}

fn tape() -> Option<&'static crate::system_art::Sprite> {
    static T: OnceLock<Option<crate::system_art::Sprite>> = OnceLock::new();
    T.get_or_init(|| crate::system_art::decode(TAPE)).as_ref()
}

/// The number every random choice of a game's label comes from: its name, case and spaces at the ends aside.
pub fn seed(title: &str) -> u32 {
    let mut h: u32 = 0x811c_9dc5;
    for b in title.trim().to_lowercase().bytes() {
        h ^= b as u32;
        h = h.wrapping_mul(0x0100_0193);
    }
    h
}

/// A small, fixed sequence of numbers from a seed (xorshift): the same name gives the same label.
struct Rng(u32);

impl Rng {
    fn new(seed: u32, salt: u32) -> Rng {
        Rng((seed ^ salt.wrapping_mul(0x9e37_79b9)).max(1))
    }
    fn next(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        x
    }
    /// From 0 to 1.
    fn f(&mut self) -> f32 {
        (self.next() >> 8) as f32 / (1u32 << 24) as f32
    }
    /// From `a` to `b`.
    fn range(&mut self, a: f32, b: f32) -> f32 {
        a + (b - a) * self.f()
    }
}

/// What a game's label looks like, all from its name: for tests and the docs' "stable per game".
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hand {
    /// The tilt of the tape (degrees, clockwise).
    pub tilt: f32,
    /// How much of the label's length the tape covers.
    pub length: f32,
    /// The tape's middle off the label's middle, as a part of the label (across, along).
    pub shift: (f32, f32),
    pub ink: u32,
}

pub fn hand(title: &str) -> Hand {
    let mut r = Rng::new(seed(title), 1);
    let tilt = r.range(-3.2, 3.2);
    let length = r.range(0.84, 0.95);
    let shift = (r.range(-0.05, 0.05), r.range(-0.04, 0.04));
    let ink = if r.f() < 0.25 { INKS[1] } else { INKS[0] };
    Hand { tilt, length, shift, ink }
}

// ---------------------------------------------------------------------------------------------------------------
// Writing

type Glyph = (fontdue::Metrics, Vec<u8>);

fn glyph(c: char, px: u32) -> std::sync::Arc<Glyph> {
    static CACHE: OnceLock<Mutex<HashMap<(char, u32), std::sync::Arc<Glyph>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut map = cache.lock().unwrap_or_else(|e| e.into_inner());
    if map.len() > 6000 {
        map.clear();
    }
    map.entry((c, px)).or_insert_with(|| std::sync::Arc::new(font().rasterize(c, px as f32))).clone()
}

/// Whether the hand font has a letter for `c` (spaces count as written).
pub fn hand_covers(c: char) -> bool {
    c.is_whitespace() || font().lookup_glyph_index(c) != 0
}

/// The pen a name is written with: the hand font (its letters folded to the nearest it has: "Ōkami" as "Okami"), or
/// the interface's title face when even that leaves letters out.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Pen {
    Hand,
    /// Printed in this face of the interface (`Text`).
    Print(Face),
}

pub fn pen_for(title: &str) -> (Pen, String) {
    let mut covers = hand_covers;
    let folded = crate::latinise::fold(title, &mut covers).into_owned();
    if folded.chars().all(hand_covers) {
        (Pen::Hand, folded)
    } else {
        (Pen::Print(Face::Title), title.to_string())
    }
}

fn width(text: &mut Text, pen: Pen, px: u32, s: &str) -> i32 {
    match pen {
        Pen::Hand => {
            let f = font();
            let mut w = 0.0f32;
            let mut prev: Option<char> = None;
            for c in s.chars() {
                if let Some(p) = prev {
                    w += f.horizontal_kern(p, c, px as f32).unwrap_or(0.0);
                }
                w += f.metrics(c, px as f32).advance_width;
                prev = Some(c);
            }
            w.ceil() as i32
        }
        Pen::Print(face) => text.literal(|text| {
            let _fixed = crate::type_scale::fixed();
            text.width(face, px, s)
        }),
    }
}

/// The height of a written line at `px`, and where its baseline is from its top.
fn line_box(pen: Pen, px: u32) -> (i32, i32) {
    match pen {
        Pen::Hand => {
            let m = font().horizontal_line_metrics(px as f32);
            let (asc, desc) = m.map_or((px as f32 * 0.8, px as f32 * 0.25), |m| (m.ascent, -m.descent));
            // Caveat's ascent leaves room for accents over capitals: lines stand a little closer than its line gap.
            ((asc * 0.92 + desc * 0.8).ceil() as i32, (asc * 0.92).ceil() as i32)
        }
        Pen::Print(_) => ((px as i32 * 5 / 4).max(1), px as i32),
    }
}

/// A single line along a narrow spine: the box of its capitals and descenders only (an accent over a capital may
/// reach onto the tape's edge), and its baseline.
fn tight_box(pen: Pen, px: u32) -> (i32, i32) {
    match pen {
        Pen::Hand => ((px as f32 * 0.96).ceil() as i32, (px as f32 * 0.70).ceil() as i32),
        Pen::Print(_) => line_box(pen, px),
    }
}

/// `s` written into `alpha` (`bw` wide) with its left at `x` and baseline at `base`; each letter a little up or down
/// and the line's letters a little apart, as a hand writes (from `r`).
fn write_line(text: &mut Text, pen: Pen, px: u32, s: &str, alpha: &mut [u8], bw: i32, x: i32, base: i32, r: &mut Rng) {
    let bh = alpha.len() as i32 / bw.max(1);
    match pen {
        Pen::Hand => {
            let f = font();
            let mut pen_x = x as f32;
            let mut prev: Option<char> = None;
            for c in s.chars() {
                if let Some(p) = prev {
                    pen_x += f.horizontal_kern(p, c, px as f32).unwrap_or(0.0);
                }
                let g = glyph(c, px);
                let (m, bmp) = (&g.0, &g.1);
                let jitter = (r.range(-0.045, 0.045) * px as f32).round() as i32;
                let gx = (pen_x + m.xmin as f32).round() as i32;
                let gy = base - m.height as i32 - m.ymin + jitter;
                for yy in 0..m.height as i32 {
                    let ty = gy + yy;
                    if ty < 0 || ty >= bh {
                        continue;
                    }
                    for xx in 0..m.width as i32 {
                        let tx = gx + xx;
                        if tx < 0 || tx >= bw {
                            continue;
                        }
                        let a = bmp[(yy * m.width as i32 + xx) as usize];
                        let d = &mut alpha[(ty * bw + tx) as usize];
                        *d = (*d).max(a);
                    }
                }
                pen_x += m.advance_width;
                prev = Some(c);
            }
        }
        Pen::Print(face) => text.literal(|text| {
            let _fixed = crate::type_scale::fixed();
            let mut buf = vec![0u32; (bw * bh).max(1) as usize];
            {
                let mut c = Canvas::new(&mut buf, bw.max(1) as u32, bh.max(1) as u32);
                text.draw(&mut c, face, px, x, base, 0xffffff, s);
            }
            for (d, s) in alpha.iter_mut().zip(&buf) {
                *d = (*d).max((s & 0xff) as u8);
            }
        }),
    }
}

/// How a name is set in a room `max_w` x `max_h`: its lines (at most `max_lines`) and their size, the largest of
/// `from ..= to` where the whole name fits in whole words; else without its subtitle; else at the smallest, the last
/// line cut with "…".
pub fn set(text: &mut Text, pen: Pen, title: &str, max_w: i32, max_h: i32, max_lines: usize, (from, to): (u32, u32)) -> (Vec<String>, u32) {
    set_in(text, pen, title, max_w, max_h, max_lines, (from, to), line_box)
}

/// `set` with the line box `lb` (`line_box`, or `tight_box` for one line on a spine).
#[allow(clippy::too_many_arguments)]
fn set_in(text: &mut Text, pen: Pen, title: &str, max_w: i32, max_h: i32, max_lines: usize, (from, to): (u32, u32), lb: fn(Pen, u32) -> (i32, i32)) -> (Vec<String>, u32) {
    let mut tries = vec![title.to_string()];
    if let Some(main) = crate::media::main_part(title) {
        tries.push(main.to_string());
    }
    let to = to.max(from);
    // A name and its subtitle on lines of their own, as a hand writes them ("Pokémon Übersetzung" / "Ação Edição", the
    // dash left out; a colon stays with the name), where there are lines enough.
    let parts = (max_lines >= 2).then(|| crate::media::main_part(title)).flatten().map(|main| {
        let rest = title[main.len()..].trim_start();
        let (colon, sub) = match rest.strip_prefix(':') {
            Some(sub) => (true, sub),
            None => (false, rest.trim_start_matches('-')),
        };
        (if colon { format!("{main}:") } else { main.to_string() }, sub.trim().to_string())
    });
    for (i, t) in tries.iter().enumerate() {
        let mut px = to;
        loop {
            let split = parts.as_ref().filter(|_| i == 0).and_then(|(main, sub)| {
                let mut a = wrap(text, pen, px, main, max_w, max_lines)?;
                a.extend(wrap(text, pen, px, sub, max_w, max_lines)?);
                Some(a)
            });
            let lines = split.filter(|l| l.len() <= max_lines).or_else(|| wrap(text, pen, px, t, max_w, max_lines));
            if let Some(lines) = lines {
                if lines.len() as i32 * lb(pen, px).0 <= max_h {
                    return (lines, px);
                }
            }
            if px <= from {
                break;
            }
            // Steps of about 6 %: a size per frame size, not one per pixel (fewer glyph sizes cached).
            px = (px - (px / 16).max(1)).max(from);
        }
    }
    // The smallest size: as many lines as the room takes, the last one cut.
    let rows = ((max_h / lb(pen, from).0.max(1)) as usize).clamp(1, max_lines.max(1));
    let words: Vec<&str> = tries.last().map_or(title, |t| t.as_str()).split_whitespace().collect();
    let mut lines: Vec<String> = Vec::new();
    let mut cur = String::new();
    for w in words {
        let cand = if cur.is_empty() { w.to_string() } else { format!("{cur} {w}") };
        if width(text, pen, from, &cand) <= max_w || cur.is_empty() {
            cur = cand;
        } else {
            lines.push(std::mem::take(&mut cur));
            cur = w.to_string();
        }
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    if lines.len() > rows {
        let rest = lines.split_off(rows - 1).join(" ");
        lines.push(rest);
    }
    if let Some(last) = lines.last_mut() {
        if width(text, pen, from, last) > max_w {
            // At a word where whole words fit ("The Legend of…"), else letter by letter.
            let words: Vec<&str> = last.split_whitespace().collect();
            let whole = (1..words.len()).rev().map(|n| format!("{}…", words[..n].join(" "))).find(|t| width(text, pen, from, t) <= max_w);
            *last = whole.unwrap_or_else(|| {
                let mut cut: String = last.clone();
                while !cut.is_empty() && width(text, pen, from, &format!("{cut}…")) > max_w {
                    cut.pop();
                }
                format!("{}…", cut.trim_end())
            });
        }
    }
    (lines, from)
}

/// `s` in whole words on at most `max_lines` lines no wider than `max_w`; None when it doesn't go.
fn wrap(text: &mut Text, pen: Pen, px: u32, s: &str, max_w: i32, max_lines: usize) -> Option<Vec<String>> {
    let mut lines: Vec<String> = Vec::new();
    let mut cur = String::new();
    for w in s.split_whitespace() {
        let cand = if cur.is_empty() { w.to_string() } else { format!("{cur} {w}") };
        if width(text, pen, px, &cand) <= max_w {
            cur = cand;
        } else {
            if cur.is_empty() || width(text, pen, px, w) > max_w {
                return None;
            }
            lines.push(std::mem::take(&mut cur));
            cur = w.to_string();
        }
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    // Two lines of about the same width read better than a long one and a lone word.
    if lines.len() == 2 && max_lines >= 2 {
        let words: Vec<&str> = s.split_whitespace().collect();
        let mut best: Option<(i32, String, String)> = None;
        for i in 1..words.len() {
            let (a, b) = (words[..i].join(" "), words[i..].join(" "));
            let (wa, wb) = (width(text, pen, px, &a), width(text, pen, px, &b));
            if wa <= max_w && wb <= max_w && best.as_ref().is_none_or(|(d, _, _)| (wa - wb).abs() < *d) {
                best = Some(((wa - wb).abs(), a, b));
            }
        }
        if let Some((_, a, b)) = best {
            lines = vec![a, b];
        }
    }
    (!lines.is_empty() && lines.len() <= max_lines).then_some(lines)
}

// ---------------------------------------------------------------------------------------------------------------
// The strip

/// What is put down: `w` x `h` in its own pixels, a strip of tape (or nothing: writing straight on the surface) and
/// the writing's cover (`ink_a`, one byte per pixel) in `ink`.
struct Piece {
    w: i32,
    h: i32,
    tape: Option<Tape>,
    ink_a: Vec<u8>,
    ink: u32,
}

/// A strip of tape: how deep each end is torn per row across it, and where along the photo it was cut.
struct Tape {
    left: Vec<f32>,
    right: Vec<f32>,
    off: f32,
}

impl Piece {
    fn bare(w: i32, h: i32) -> Piece {
        let (w, h) = (w.max(1), h.max(1));
        Piece { w, h, tape: None, ink_a: vec![0; (w * h) as usize], ink: 0 }
    }
}

/// A piece of tape `len` x `thick`, its two ends torn (a ragged line, different at each end and for each game).
fn strip(len: i32, thick: i32, r: &mut Rng) -> Piece {
    let (len, thick) = (len.max(4), thick.max(4));
    let tw = tape().map_or(1.0, |t| t.w as f32);
    // Along the photo from a place of the game's own.
    let off = r.range(0.0, tw);
    // The torn ends: the tear's depth across the tape, a ragged walk.
    let tear = |r: &mut Rng| -> Vec<f32> {
        // A tear runs about a sixth of the tape's width deep, never more than a few letters' room on a wide strip.
        let depth = (thick as f32 * 0.16).min(len as f32 * 0.035).max(2.0);
        let steps = (thick / 3).max(4) as usize;
        let mut pts = Vec::with_capacity(steps + 1);
        let mut d = r.range(0.2, 0.8) * depth;
        for _ in 0..=steps {
            d = (d + r.range(-0.45, 0.45) * depth).clamp(0.0, depth);
            pts.push(d);
        }
        (0..thick)
            .map(|v| {
                let t = v as f32 / thick as f32 * steps as f32;
                let i = (t as usize).min(steps - 1);
                let f = t - i as f32;
                // A tear's fibres: a pixel's jitter on the walk.
                pts[i] * (1.0 - f) + pts[i + 1] * f + r.range(-0.6, 0.6)
            })
            .collect()
    };
    let left = tear(r);
    let right = tear(r);
    Piece { w: len, h: thick, tape: Some(Tape { left, right, off }), ink_a: vec![0; (len * thick) as usize], ink: 0 }
}

/// `lines` written into `piece` in `ink`, centred in it.
#[allow(clippy::too_many_arguments)]
fn write_into(text: &mut Text, piece: &mut Piece, pen: Pen, px: u32, lines: &[String], ink: u32, r: &mut Rng, lb: fn(Pen, u32) -> (i32, i32)) {
    let (lh, base) = lb(pen, px);
    let block = lines.len() as i32 * lh;
    let mut top = (piece.h - block) / 2;
    piece.ink = ink;
    for line in lines {
        let lw = width(text, pen, px, line);
        // Each line a little off the middle, as written by hand.
        let x = (piece.w - lw) / 2 + (r.range(-0.03, 0.03) * piece.w as f32) as i32;
        write_line(text, pen, px, line, &mut piece.ink_a, piece.w, x.max(1), top + base, r);
        top += lh;
    }
}

/// `Pic::blend` without its bookkeeping per pixel (`put` marks the picture changed once): `rgb` over (x, y) with
/// cover `a`, the pixel at least that solid.
#[inline]
fn over(p: &mut Pic, x: i32, y: i32, rgb: u32, a: u32) {
    if x < 0 || y < 0 || x >= p.w || y >= p.h || a == 0 {
        return;
    }
    let d = &mut p.px[(y * p.w + x) as usize];
    let da = *d >> 24;
    let out = if da == 0 { rgb & 0xffffff } else { mix(*d & 0xffffff, rgb & 0xffffff, a.min(255)) };
    *d = da.max(a.min(255)) << 24 | out;
}

/// Puts `piece` down on `p`, its middle at (`cx`, `cy`), turned `deg` degrees clockwise, with a soft shadow under
/// it (`shadow`: how dark); only inside `clip` (x, y, w, h) when given. One pass over the picture: each pixel's place
/// on the piece moves by a fixed step along a row; the tape's colour is the photo's there, its edges worked out from
/// that place (smooth), the writing's cover taken between its pixels.
fn put(p: &mut Pic, piece: &Piece, (cx, cy): (f32, f32), deg: f32, shadow: f32, clip: Option<(i32, i32, i32, i32)>) {
    p.changed();
    let (s, c) = deg.to_radians().sin_cos();
    let (hw, hh) = (piece.w as f32 / 2.0, piece.h as f32 / 2.0);
    let ex = (hw * c.abs() + hh * s.abs()).ceil() as i32 + 2;
    let ey = (hw * s.abs() + hh * c.abs()).ceil() as i32 + 2;
    let off = if shadow > 0.0 && piece.tape.is_some() { (piece.h as f32 * 0.05).clamp(1.0, 4.0).round() as i32 } else { 0 };
    let (x0, x1) = ((cx as i32 - ex).max(0), (cx as i32 + ex + 2 + off).min(p.w));
    let (y0, y1) = ((cy as i32 - ey).max(0), (cy as i32 + ey + 3 + off).min(p.h));
    let (x0, x1, y0, y1) = match clip {
        Some((qx, qy, qw, qh)) => (x0.max(qx), x1.min(qx + qw), y0.max(qy), y1.min(qy + qh)),
        None => (x0, x1, y0, y1),
    };
    if x1 <= x0 || y1 <= y0 {
        return;
    }
    let (bw, bh) = ((x1 - x0) as usize, (y1 - y0) as usize);
    let (pw, ph) = (piece.w, piece.h);
    let (fw, fh) = (pw as f32, ph as f32);
    let tex = tape();
    let scale = tex.map_or(1.0, |t| t.h as f32 / fh);
    // The writing's cover at (u, v), between its pixels (0 to 255); 0 at once where its four pixels are bare.
    let ink_at = |u: f32, v: f32| -> u32 {
        let (u, v) = (u - 0.5, v - 0.5);
        let (u0, v0) = (u.floor() as i32, v.floor() as i32);
        if u0 < -1 || v0 < -1 || u0 >= pw || v0 >= ph {
            return 0;
        }
        let (a, b, c2, d) = if u0 >= 0 && v0 >= 0 && u0 + 1 < pw && v0 + 1 < ph {
            // Inside: the four pixels straight from the row and the next.
            let i = (v0 * pw + u0) as usize;
            let (r0, r1) = (&piece.ink_a[i..i + 2], &piece.ink_a[i + pw as usize..i + pw as usize + 2]);
            (r0[0] as u32, r0[1] as u32, r1[0] as u32, r1[1] as u32)
        } else {
            let at = |x: i32, y: i32| if x < 0 || y < 0 || x >= pw || y >= ph { 0 } else { piece.ink_a[(y * pw + x) as usize] as u32 };
            (at(u0, v0), at(u0 + 1, v0), at(u0, v0 + 1), at(u0 + 1, v0 + 1))
        };
        if a | b | c2 | d == 0 {
            return 0;
        }
        let fu = ((u - u0 as f32) * 256.0) as u32;
        let fv = ((v - v0 as f32) * 256.0) as u32;
        let top = a * (256 - fu) + b * fu;
        let bot = c2 * (256 - fu) + d * fu;
        (top * (256 - fv) + bot * fv) >> 16
    };
    let inked = piece.ink_a.iter().any(|&a| a != 0);
    let mut rgb = vec![0u32; bw * bh];
    let mut cov = vec![0u8; bw * bh];
    for (j, y) in (y0..y1).enumerate() {
        let dy = y as f32 + 0.5 - cy;
        let dx0 = x0 as f32 + 0.5 - cx;
        // (u, v): the pixel's middle on the piece, from its top left.
        let mut u = dx0 * c + dy * s + hw;
        let mut v = -dx0 * s + dy * c + hh;
        for i in 0..bw {
            if u > -1.0 && v > -1.0 && u < fw + 1.0 && v < fh + 1.0 {
                let k = j * bw + i;
                match &piece.tape {
                    Some(t) => {
                        let row = (v as i32).clamp(0, ph - 1) as usize;
                        let (l, r) = (t.left[row], t.right[row]);
                        let (du, dv) = ((u - l).min(fw - r - u), v.min(fh - v));
                        // Cover: inside the long edges and the two torn ends, smooth over a pixel.
                        let a = if du >= 2.0 && dv >= 1.5 { 1.0 } else { (v + 0.5).clamp(0.0, 1.0) * (fh - v + 0.5).clamp(0.0, 1.0) * (u - l).clamp(0.0, 1.0) * (fw - r - u).clamp(0.0, 1.0) };
                        if a > 0.0 {
                            let mut col = match tex {
                                Some(photo) => {
                                    let mut tx = u.max(0.0) * scale + t.off;
                                    while tx >= photo.w as f32 {
                                        tx -= photo.w as f32;
                                    }
                                    let ty = ((v.max(0.0) * scale) as u32).min(photo.h - 1);
                                    photo.px[(ty * photo.w + (tx as u32).min(photo.w - 1)) as usize] & 0xffffff
                                }
                                None => TAPE_CREAM,
                            };
                            if dv < 1.5 {
                                // The long edges a shade darker (the slit edge catches less light).
                                col = mix(col, 0x6a5a40, ((1.0 - dv / 1.5).clamp(0.0, 1.0) * 70.0) as u32);
                            }
                            if du < 2.0 {
                                // The torn ends a little lighter (fibres).
                                col = mix(col, 0xfff6e0, ((1.0 - du / 2.0).clamp(0.0, 1.0) * 90.0) as u32);
                            }
                            let ia = if inked { ink_at(u, v) } else { 0 };
                            if ia > 0 {
                                // Marker on tape: nearly opaque, the tape's grain a little through it.
                                col = mix(col, piece.ink, ia * 235 / 255);
                            }
                            rgb[k] = col;
                            // Masking tape lets a little of what is under it through.
                            cov[k] = (a * 240.0) as u8;
                        }
                    }
                    None => {
                        let ia = ink_at(u, v);
                        if ia > 0 {
                            rgb[k] = piece.ink;
                            cov[k] = (ia * 235 / 255) as u8;
                        }
                    }
                }
            }
            u += c;
            v -= s;
        }
    }
    // The shadow: the same cover a little down and right, soft; then the piece over it.
    if off > 0 {
        let k = (shadow * 256.0) as u32;
        for j in off as usize..bh {
            for i in off as usize..bw {
                let a = cov[(j - off as usize) * bw + i - off as usize] as u32;
                // Under the tape itself the shadow hardly shows: only where the tape leaves the surface bare.
                if a > 0 && cov[j * bw + i] < 200 {
                    over(p, x0 + i as i32, y0 + j as i32, 0x000000, (a * k) >> 8);
                }
            }
        }
    }
    for j in 0..bh {
        let row = ((y0 + j as i32) * p.w + x0) as usize;
        for i in 0..bw {
            let a = cov[j * bw + i] as u32;
            if a == 0 {
                continue;
            }
            let d = &mut p.px[row + i];
            let da = *d >> 24;
            let out = if da == 0 { rgb[j * bw + i] } else { mix(*d & 0xffffff, rgb[j * bw + i], a) };
            *d = da.max(a) << 24 | out;
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------
// The three kinds of label

/// The smallest a name is written (picture pixels): readable on a 640x480 panel, where the design is halved.
pub const MIN_PX: u32 = 22;
/// The smallest on a spine (one line along a narrow tape), as the printed spines' smallest step (`media::spine_fit`).
pub const SPINE_MIN_PX: u32 = 16;

/// A strip of tape across a label `(x, y, w, h)` of `p` (a blank cartridge's label, a plain box's front) with `title`
/// on it in marker, one to three lines, as large as the label allows.
pub fn on_label(text: &mut Text, p: &mut Pic, (x, y, w, h): (i32, i32, i32, i32), title: &str) {
    if w < 8 || h < 8 {
        return;
    }
    let g = hand(title);
    let mut r = Rng::new(seed(title), 2);
    let (pen, name) = pen_for(title);
    let len = (w as f32 * g.length) as i32;
    // Clear of the torn ends (`strip`) and of the slant's reach.
    let pad_x = (len / 12).max((len as f32 * 0.035) as i32 + 6);
    let max_px = ((h as f32 * 0.30) as u32).clamp(MIN_PX, 150);
    // A small label (a HuCard, a NES cartridge's narrow one, a cartridge coming out of its box): a little smaller
    // before a word is cut, as the printed labels go (`media::LABEL_PX`).
    let min_px = if h < 80 { 14 } else if h < 160 { 18 } else { MIN_PX };
    let (mut lines, mut px) = set(text, pen, &name, len - 2 * pad_x, (h as f32 * 0.70) as i32, 3, (min_px, max_px));
    // A name cut at the smallest size (one long word on a small label): a third smaller still, if that writes it whole.
    if lines.iter().any(|l| l.ends_with('…')) && min_px > 12 {
        let (l2, p2) = set(text, pen, &name, len - 2 * pad_x, (h as f32 * 0.70) as i32, 3, ((min_px * 2 / 3).max(12), min_px));
        if !l2.iter().any(|l| l.ends_with('…')) {
            (lines, px) = (l2, p2);
        }
    }
    let (lh, _) = line_box(pen, px);
    let thick = (lines.len() as i32 * lh + lh * 2 / 3).max((h as f32 * 0.34) as i32).min((h as f32 * 0.86) as i32);
    let mut piece = strip(len, thick, &mut r);
    write_into(text, &mut piece, pen, px, &lines, g.ink, &mut r, line_box);
    let cx = x as f32 + w as f32 / 2.0 + g.shift.1 * w as f32;
    let cy = y as f32 + h as f32 / 2.0 + g.shift.0 * h as f32;
    put(p, &piece, (cx, cy), g.tilt, 0.28, Some((x - w / 20, y - h / 20, w + w / 10, h + h / 10)));
}

/// A strip of tape along a spine's label `(x, y, w, h)` (taller than wide) with a name on it, read from top to bottom
/// as a spine's print is (the letters' tops face right). `lines` are what the spine shows of the name, as
/// `media::spine_fit` chose them for the printed spine (the part that tells it apart, whole, never cut) and `px`,
/// `face` how it would print them: written by hand as large as the tape takes, 16 px at the least; where the hand
/// can't take them that size, printed as the spine would. Without `tape` (a disc case's insert) the name is written
/// straight on the label.
#[allow(clippy::too_many_arguments)]
pub fn on_spine(text: &mut Text, p: &mut Pic, (x, y, w, h): (i32, i32, i32, i32), title: &str, lines: &[String], (px, face): (u32, Face), tape: bool) {
    if w < 6 || h < 12 {
        return;
    }
    let g = hand(title);
    let mut r = Rng::new(seed(title), 3);
    let len = (h as f32 * (g.length + 0.03).min(0.97)) as i32;
    let thick = (w as f32 * 0.92).round() as i32;
    let pad = (len / 16).max(3);
    let folded: Vec<(Pen, String)> = lines.iter().map(|l| pen_for(l)).collect();
    let hand_ok = folded.iter().all(|(pen, _)| *pen == Pen::Hand);
    let hand_lines: Vec<String> = folded.into_iter().map(|(_, l)| l).collect();
    let lb: fn(Pen, u32) -> (i32, i32) = if lines.len() > 1 { line_box } else { tight_box };
    let max_px = ((thick as f32 * 0.9 / 0.96 / lines.len().max(1) as f32) as u32).max(SPINE_MIN_PX);
    let fits = |text: &mut Text, s: u32| {
        hand_lines.iter().all(|l| width(text, Pen::Hand, s, l) <= len - 2 * pad) && hand_lines.len() as i32 * lb(Pen::Hand, s).0 <= thick + 2
    };
    let hand_px = hand_ok.then(|| (SPINE_MIN_PX..=max_px).rev().find(|&s| fits(text, s))).flatten();
    let (pen, px, shown) = match hand_px {
        Some(s) => (Pen::Hand, s, hand_lines),
        None => (Pen::Print(face), px, lines.to_vec()),
    };
    let lb = if matches!(pen, Pen::Print(_)) { line_box } else { lb };
    // Printed: the tape as long as the print, the whole spine's label at the most (what `spine_fit` fitted it to).
    let len = if matches!(pen, Pen::Print(_)) {
        let need = shown.iter().map(|l| width(text, pen, px, l)).max().unwrap_or(0) + 2 * pad;
        len.max(need).min(h)
    } else {
        len
    };
    let mut piece = if tape { strip(len, thick, &mut r) } else { Piece::bare(len, thick) };
    write_into(text, &mut piece, pen, px, &shown, g.ink, &mut r, lb);
    let cx = x as f32 + w as f32 / 2.0;
    let cy = y as f32 + h as f32 / 2.0 + g.shift.1 * h as f32 * 0.6;
    // Along the spine, turned a quarter clockwise: the writing's top faces right. A spine is narrow: little tilt.
    put(p, &piece, (cx, cy), 90.0 + g.tilt * 0.35, if tape { 0.22 } else { 0.0 }, Some((x - 1, y, w + 2, h)));
}

/// `title` written straight onto a surface in `(x, y, w, h)` of `p` (the paper insert of a disc case, a CD-R's silver)
/// in marker, a little tilted, up to `max_lines` lines. `p` keeps what is under the writing.
pub fn write_on(text: &mut Text, p: &mut Pic, (x, y, w, h): (i32, i32, i32, i32), title: &str, max_lines: usize) {
    if w < 8 || h < 8 {
        return;
    }
    let g = hand(title);
    let mut r = Rng::new(seed(title), 4);
    let (pen, name) = pen_for(title);
    let max_px = ((h as f32 * 0.42) as u32).clamp(MIN_PX, 140);
    let (lines, px) = set(text, pen, &name, w * 9 / 10, h, max_lines, (MIN_PX.min(max_px), max_px));
    write_lines(text, p, (x, y, w, h), pen, px, &lines, g.tilt * 0.8, g.ink, &mut r);
}

/// `lines` written at `px` in the middle of `(x, y, w, h)`, turned `deg`, see-through around the ink.
#[allow(clippy::too_many_arguments)]
fn write_lines(text: &mut Text, p: &mut Pic, (x, y, w, h): (i32, i32, i32, i32), pen: Pen, px: u32, lines: &[String], deg: f32, ink: u32, r: &mut Rng) {
    let mut piece = Piece::bare(w, h);
    write_into(text, &mut piece, pen, px, lines, ink, r, line_box);
    put(p, &piece, (x as f32 + w as f32 / 2.0, y as f32 + h as f32 / 2.0), deg, 0.0, None);
}

/// `title` written on a disc by hand: above the hole (`top`), its second line, if it needs one, below it (`bottom`),
/// as a name is written on a CD-R.
pub fn on_disc(text: &mut Text, p: &mut Pic, top: (i32, i32, i32, i32), bottom: (i32, i32, i32, i32), title: &str) {
    if top.2 < 8 || top.3 < 8 {
        return;
    }
    let g = hand(title);
    let mut r = Rng::new(seed(title), 5);
    let (pen, name) = pen_for(title);
    let max_px = ((top.3 as f32 * 0.8) as u32).clamp(18, 140);
    let w = top.2.min(bottom.2);
    let (lines, px) = set(text, pen, &name, w, top.3 + bottom.3, 2, (18.min(max_px), max_px));
    match lines.as_slice() {
        [one] => write_lines(text, p, top, pen, px, std::slice::from_ref(one), g.tilt, g.ink, &mut r),
        [first, second, ..] => {
            write_lines(text, p, top, pen, px, std::slice::from_ref(first), g.tilt, g.ink, &mut r);
            write_lines(text, p, bottom, pen, px, std::slice::from_ref(second), g.tilt * 0.6, g.ink, &mut r);
        }
        [] => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_photo_and_the_font_load() {
        let t = tape().expect("tape.png decodes");
        assert!(t.w > 200 && t.h > 40, "{}x{}", t.w, t.h);
        // Cream: warm, light.
        let mid = t.px[(t.h / 2 * t.w + t.w / 3) as usize] & 0xffffff;
        let (r, g, b) = (mid >> 16 & 0xff, mid >> 8 & 0xff, mid & 0xff);
        assert!(r > g && g > b && r > 180, "{mid:06x}");
        for c in "ÄÖÜäöüßáéíóúñÑ¿¡ãõâêôçÇàèëîïûùœ&:!?-'0123456789".chars() {
            assert!(hand_covers(c), "the hand font writes {c}");
        }
    }

    #[test]
    fn a_name_always_gets_the_same_label_and_names_differ() {
        assert_eq!(hand("Super Mario Bros. 3 Mix"), hand("Super Mario Bros. 3 Mix"));
        assert_eq!(seed("  Celeste Classic "), seed("celeste classic"), "spaces at the ends and case don't change it");
        let all: Vec<Hand> = ["Celeste Classic", "Pokémon Prism", "Zelda Fan Remake", "2048", "Tobu Tobu Girl", "Micro Mages"].iter().map(|t| hand(t)).collect();
        let tilts: std::collections::BTreeSet<i32> = all.iter().map(|h| (h.tilt * 10.0) as i32).collect();
        assert!(tilts.len() >= 5, "games differ: {all:?}");
        assert!(all.iter().all(|h| h.tilt.abs() <= 3.2 && (0.84..=0.95).contains(&h.length)));
    }

    #[test]
    fn letters_the_hand_lacks_are_folded_or_printed() {
        assert_eq!(pen_for("Ōkami Demake"), (Pen::Hand, "Ōkami Demake".to_string()), "Latin Extended-A is in the cut");
        assert_eq!(pen_for("Pokémon Ação").0, Pen::Hand);
        // Japanese: written as the app reads it out elsewhere (romanised), else printed; never an empty label.
        let (pen, name) = pen_for("ドラゴンクエスト");
        assert!(!name.trim().is_empty() && (matches!(pen, Pen::Print(_)) || name.chars().all(hand_covers)), "{pen:?} {name}");
    }

    use crate::image::Image;

    /// Pixels of masking tape (its cream) in a picture.
    fn cream(p: &Pic) -> usize {
        p.px.iter()
            .filter(|&&q| q >> 24 == 255)
            .map(|q| (q >> 16 & 0xff, q >> 8 & 0xff, q & 0xff))
            .filter(|&(r, g, b)| r > 195 && g > 180 && b > 130 && r > b + 28 && r >= g)
            .count()
    }

    /// Pixels of marker ink (dark) in a picture.
    fn ink(p: &Pic) -> usize {
        p.px.iter().filter(|&&q| q >> 24 == 255 && crate::cabinet::luma(q & 0xffffff) < 60).count()
    }

    fn looks(sys: &str) -> (crate::cabinet_art::Look, crate::cabinet_art::Look) {
        let m = crate::media::Medium::of(sys);
        let plain = crate::media::case_look(sys, 0x8040c0, m, None);
        (plain, crate::media::homemade(plain, m))
    }

    /// No box art: the front, the spine and the cartridge carry tape with writing on it; a printed one has none.
    #[test]
    fn no_art_gets_tape_and_writing_on_every_kind_of_case() {
        use crate::media::{self, Game, Medium};
        let mut text = Text::new();
        for sys in ["snes", "gb", "gba", "nes", "n64", "megadrive", "pcengine", "nds", "pico8"] {
            let m = Medium::of(sys);
            let (plain, home) = looks(sys);
            assert!(home.tape && !plain.tape);
            let front = media::front(&mut text, m, 1.0, home, "Celeste Classic", None, None, sys);
            let printed = media::front(&mut text, m, 1.0, plain, "Celeste Classic", None, None, sys);
            let area = (front.w * front.h) as usize;
            assert!(cream(&front) > area / 12, "{sys}: tape on the front ({} of {area})", cream(&front));
            assert!(cream(&printed) < area / 50, "{sys}: no tape on a printed front");
            assert!(ink(&front) > 150, "{sys}: the name is written on it");
            let spine = media::spine(&mut text, m, 1.0, home, "Celeste Classic");
            assert!(cream(&spine) > (spine.w * spine.h) as usize / 6, "{sys}: tape on the spine");
            if let Some(cart) = media::game_pic(&mut text, Game::of(sys, "x"), 220, home, "Celeste Classic", None, None) {
                assert!(cream(&cart) > 500, "{sys}: tape on the cartridge's label");
            }
        }
    }

    /// A disc system's game without art: a silver disc with the name written on it, the case's insert written on.
    #[test]
    fn no_art_disc_is_a_written_cd_r() {
        use crate::media::{self, DiscKind, Medium};
        let mut text = Text::new();
        for sys in ["psx", "ps2", "saturn", "gc"] {
            let (plain, home) = looks(sys);
            let disc = media::disc_pic(&mut text, DiscKind::of(sys), 260, home, "Mein erstes Rennspiel", None);
            let printed = media::disc_pic(&mut text, DiscKind::of(sys), 260, plain, "Mein erstes Rennspiel", None);
            let silver = |p: &Pic| p.px.iter().filter(|&&q| q >> 24 == 255).filter(|&&q| { let (r, g, b) = (q >> 16 & 0xff, q >> 8 & 0xff, q & 0xff); r > 170 && g > 170 && b > 170 && r.abs_diff(b) < 30 }).count();
            assert!(silver(&disc) > silver(&printed) + 5000, "{sys}: a silver CD-R");
            assert!(ink(&disc) > 200, "{sys}: written on");
            let m = Medium::of(sys);
            let front = media::front(&mut text, m, 1.0, home, "Mein erstes Rennspiel", None, None, sys);
            assert!(ink(&front) > 300, "{sys}: the insert is written on");
        }
    }

    /// Real art always wins: a front, cartridge or disc with a cover shows the cover, whatever the look says.
    #[test]
    fn art_wins_over_tape() {
        use crate::media::{self, DiscKind, Game, Medium};
        let mut text = Text::new();
        for sys in ["snes", "gba", "psx"] {
            let m = Medium::of(sys);
            let (lx, ly, lw, lh) = media::label_rect(m, 1.0);
            let cover = Image { w: lw as u32, h: lh as u32, px: vec![0x1060c0; (lw * lh) as usize] };
            let (plain, home) = looks(sys);
            let a = media::front(&mut text, m, 1.0, plain, "Golden Sun", Some(&cover), None, sys);
            let b = media::front(&mut text, m, 1.0, home, "Golden Sun", Some(&cover), None, sys);
            let mid = ((ly + lh / 2) * a.w + lx + lw / 2) as usize;
            assert_eq!(a.px[mid], b.px[mid], "{sys}: the cover, not tape");
            assert!(cream(&b) < (b.w * b.h) as usize / 50, "{sys}: no tape over a cover");
            let g = Game::of(sys, "x");
            let pic = if g.is_round() { Some(media::disc_pic(&mut text, DiscKind::of(sys), 220, home, "Golden Sun", Some(&cover))) } else { media::game_pic(&mut text, g, 220, home, "Golden Sun", Some(&cover), None) };
            if let Some(pic) = pic {
                assert!(cream(&pic) < 200, "{sys}: the cartridge or disc shows its cover");
            }
        }
    }

    /// The same name gives the same picture; another name another one (its own slant and tear).
    #[test]
    fn stable_per_name() {
        use crate::media::{self, Medium};
        let mut text = Text::new();
        let (_, home) = looks("snes");
        let one = media::front(&mut text, Medium::Snes, 1.0, home, "Celeste Classic", None, None, "snes");
        let again = media::front(&mut text, Medium::Snes, 1.0, home, "Celeste Classic", None, None, "snes");
        assert!(one.px == again.px, "the same game looks the same every time");
        let other = media::front(&mut text, Medium::Snes, 1.0, home, "Micro Puzzle", None, None, "snes");
        assert!(one.px != other.px);
        assert_ne!(hand("Celeste Classic"), hand("Micro Puzzle"));
    }

    /// A long name goes on more lines and smaller, then without its subtitle; only then is it cut, at a word; and every
    /// line fits the tape.
    #[test]
    fn long_names_wrap_shrink_and_never_overflow() {
        let mut text = Text::new();
        let long = "The Legend of Zelda - Fan Remake of the Very First Dungeon";
        let (lines, px) = set(&mut text, Pen::Hand, long, 300, 200, 3, (MIN_PX, 80));
        assert!(lines.len() >= 2 && lines.len() <= 3 && px >= MIN_PX, "{lines:?} {px}");
        assert_eq!(lines.join(" "), long.replace(" - ", " "), "the whole name where it fits, its subtitle on a line of its own");
        for l in &lines {
            assert!(width(&mut text, Pen::Hand, px, l) <= 300, "{l}");
        }
        let (lines, _) = set(&mut text, Pen::Hand, long, 260, 40, 1, (MIN_PX, 40));
        assert_eq!(lines, vec!["The Legend of Zelda".to_string()], "without its subtitle on one short line");
        let (lines, _) = set(&mut text, Pen::Hand, "Supercalifragilistic Adventures Deluxe", 120, 30, 1, (MIN_PX, 30));
        assert!(lines[0].ends_with('…') && width(&mut text, Pen::Hand, MIN_PX, &lines[0]) <= 120, "{lines:?}");
    }

    /// German, Spanish, Portuguese and French letters are written by hand, none dropped.
    #[test]
    fn accents_are_written() {
        let mut text = Text::new();
        for name in ["Übersetzung für Spieler", "Año de la Ñandú", "Ação e Coração", "Crème brûlée Œuvre"] {
            let (pen, written) = pen_for(name);
            assert_eq!((pen, written.as_str()), (Pen::Hand, name));
            let (lines, _) = set(&mut text, pen, name, 400, 100, 2, (MIN_PX, 60));
            assert_eq!(lines.join(" "), name);
        }
        // The dots and marks above the capitals are drawn: ink above the capitals' height that plain ones have not.
        let above = |text: &mut Text, s: &str| {
            let mut alpha = vec![0u8; 200 * 60];
            write_line(text, Pen::Hand, 40, s, &mut alpha, 200, 4, 50, &mut Rng::new(1, 1));
            alpha[..200 * 22].iter().filter(|&&a| a > 128).count()
        };
        assert!(above(&mut text, "ÄÖÜ ÑÃ") > above(&mut text, "AOU NA") + 10, "accents over the capitals");
    }

    /// The empty-data rule: a game with no name (or only spaces) still gets its tape, never an empty gap; a label too
    /// small for writing gets the tape alone.
    #[test]
    fn no_name_still_has_its_tape() {
        use crate::media::{self, Game, Medium};
        let mut text = Text::new();
        let (_, home) = looks("gba");
        for name in ["", "   "] {
            let front = media::front(&mut text, Medium::Gba, 1.0, home, name, None, None, "gba");
            assert!(cream(&front) > (front.w * front.h) as usize / 12, "tape for {name:?}");
            let spine = media::spine(&mut text, Medium::Gba, 1.0, home, name);
            assert!(cream(&spine) > 50);
        }
        let small = media::game_pic(&mut text, Game::Gba, 60, home, "Golden Sun", None, None).unwrap();
        assert!(cream(&small) > 30, "a small cartridge: tape without writing");
    }

    /// A contact sheet of home-made labels beside the printed ones (`POCKETUI_TAPE_SHEET=<file.png>` writes it): fronts,
    /// spines, cartridges and discs of every kind of case.
    #[test]
    fn contact_sheet() {
        use crate::media::{self, DiscKind, Game, Medium};
        let Some(out) = std::env::var_os("POCKETUI_TAPE_SHEET") else { return };
        let mut text = Text::new();
        let titles = ["Celeste Classic", "Pokémon Übersetzung: Ação Edição", "The Legend of Zelda - Fan Remake of the Very First Dungeon", "ドラゴンクエスト", "2048"];
        let systems = ["snes", "gb", "gba", "nes", "n64", "megadrive", "pcengine", "psx", "ps2", "saturn", "gc", "psp", "nds", "pico8"];
        let (sw, sh) = (2400, 4600);
        let mut sheet = Pic::new(sw, sh, 0xff30_2c28);
        let mut x = 10;
        let mut y = 10;
        let mut row_h = 0;
        let put_pic = |sheet: &mut Pic, p: &Pic, x: &mut i32, y: &mut i32, row_h: &mut i32| {
            if *x + p.w > sw {
                *x = 10;
                *y += *row_h + 10;
                *row_h = 0;
            }
            for yy in 0..p.h {
                for xx in 0..p.w {
                    let q = p.px[(yy * p.w + xx) as usize];
                    if q >> 24 != 0 {
                        sheet.blend(*x + xx, *y + yy, q & 0xffffff, q >> 24);
                    }
                }
            }
            *x += p.w + 8;
            *row_h = (*row_h).max(p.h);
        };
        for (i, sys) in systems.iter().enumerate() {
            let m = Medium::of(sys);
            let title = titles[i % titles.len()];
            let plain = media::case_look(sys, 0x8040c0, m, None);
            for look in [plain, media::homemade(plain, m)] {
                let spine = media::spine(&mut text, m, 1.0, look, title);
                put_pic(&mut sheet, &spine, &mut x, &mut y, &mut row_h);
                let front = media::front(&mut text, m, 1.0, look, title, None, None, sys);
                put_pic(&mut sheet, &front, &mut x, &mut y, &mut row_h);
                let g = Game::of(sys, "x");
                let pic = if g.is_round() { Some(media::disc_pic(&mut text, DiscKind::of(sys), 220, look, title, None)) } else { media::game_pic(&mut text, g, 200, look, title, None, None) };
                if let Some(pic) = pic {
                    put_pic(&mut sheet, &pic, &mut x, &mut y, &mut row_h);
                }
            }
            x = 10;
            y += row_h + 14;
            row_h = 0;
            if y > sh - 300 {
                break;
            }
        }
        let used = (y + row_h + 10).min(sh);
        let mut bytes = Vec::with_capacity((sw * used * 4) as usize);
        for q in &sheet.px[..(sw * used) as usize] {
            bytes.extend_from_slice(&[(q >> 16) as u8, (q >> 8) as u8, *q as u8, 255]);
        }
        let f = std::fs::File::create(out).unwrap();
        let mut e = png::Encoder::new(std::io::BufWriter::new(f), sw as u32, used as u32);
        e.set_color(png::ColorType::Rgba);
        e.set_depth(png::BitDepth::Eight);
        e.write_header().unwrap().write_image_data(&bytes).unwrap();
    }
}
