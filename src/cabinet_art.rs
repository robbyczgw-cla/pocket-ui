//! Painting for the Cabinet look (`cabinet`): the wooden cabinet, the cases' spines and fronts, a case turning
//! towards you, and the cartridge or disc going into the console. Everything is drawn here in code; the only
//! pictures are the user's own box art and the console sprites of `system_art`.
//!
//! Spines and fronts are made once per game and kept (`app_cabinet.rs`); a frame copies them, so only the
//! case being turned is drawn pixel by pixel.

use crate::cabinet::{ink_on, luma, FormFactor, Size, BOARD};
use crate::gfx::{mix, Canvas};
use crate::image::Image;
use crate::system_art::Sprite;
use crate::text::{Face, Text};

/// A picture with straight alpha, 0xAARRGGBB.
#[derive(Clone)]
pub struct Pic {
    pub w: i32,
    pub h: i32,
    pub px: Vec<u32>,
}

const OPAQUE: u32 = 0xff00_0000;

impl Pic {
    pub fn new(w: i32, h: i32, argb: u32) -> Pic {
        let (w, h) = (w.max(1), h.max(1));
        Pic {
            w,
            h,
            px: vec![argb; (w * h) as usize],
        }
    }

    #[inline]
    pub fn get(&self, x: i32, y: i32) -> u32 {
        self.px[(y.clamp(0, self.h - 1) * self.w + x.clamp(0, self.w - 1)) as usize]
    }

    /// Paints `rgb` over (x, y) with coverage `a` (0-255); the pixel becomes at least that solid.
    #[inline]
    pub fn blend(&mut self, x: i32, y: i32, rgb: u32, a: u32) {
        if x < 0 || y < 0 || x >= self.w || y >= self.h || a == 0 {
            return;
        }
        let i = (y * self.w + x) as usize;
        let d = self.px[i];
        let da = d >> 24;
        let out = if da == 0 {
            rgb & 0xffffff
        } else {
            mix(d & 0xffffff, rgb & 0xffffff, a.min(255))
        };
        self.px[i] = (da.max(a.min(255))) << 24 | out;
    }

    pub fn fill(&mut self, x: i32, y: i32, w: i32, h: i32, argb: u32) {
        for yy in y.max(0)..(y + h).min(self.h) {
            for xx in x.max(0)..(x + w).min(self.w) {
                self.px[(yy * self.w + xx) as usize] = argb;
            }
        }
    }

    /// Mixes `rgb` into the solid pixels of a rectangle with `a` (0-255).
    pub fn tint(&mut self, x: i32, y: i32, w: i32, h: i32, rgb: u32, a: u32) {
        for yy in y.max(0)..(y + h).min(self.h) {
            for xx in x.max(0)..(x + w).min(self.w) {
                let i = (yy * self.w + xx) as usize;
                let d = self.px[i];
                if d >> 24 != 0 {
                    self.px[i] = (d & 0xff00_0000) | mix(d & 0xffffff, rgb, a);
                }
            }
        }
    }

    /// Cuts the corners round (anti-aliased): radii top-left, top-right, bottom-right, bottom-left.
    pub fn round(&mut self, radii: [f32; 4]) {
        let (w, h) = (self.w as f32, self.h as f32);
        for (k, r) in radii.into_iter().enumerate() {
            if r <= 0.0 {
                continue;
            }
            let ri = r.ceil() as i32;
            for dy in 0..ri {
                for dx in 0..ri {
                    // Distance of the pixel centre from the corner circle's centre.
                    let (fx, fy) = (dx as f32 + 0.5, dy as f32 + 0.5);
                    let d = ((r - fx).powi(2) + (r - fy).powi(2)).sqrt();
                    let cover = (r - d + 0.5).clamp(0.0, 1.0);
                    if cover >= 1.0 {
                        continue;
                    }
                    let (x, y) = match k {
                        0 => (dx, dy),
                        1 => (w as i32 - 1 - dx, dy),
                        2 => (w as i32 - 1 - dx, h as i32 - 1 - dy),
                        _ => (dx, h as i32 - 1 - dy),
                    };
                    let i = (y * self.w + x) as usize;
                    let a = ((self.px[i] >> 24) as f32 * cover) as u32;
                    self.px[i] = a << 24 | (self.px[i] & 0xffffff);
                }
            }
        }
    }

    /// A copy `w` x `h` (nearest pixel), for a case's front shrinking into the console.
    pub fn scaled(&self, w: i32, h: i32) -> Pic {
        let (w, h) = (w.max(1), h.max(1));
        let mut out = Pic::new(w, h, 0);
        for y in 0..h {
            let sy = (y * self.h / h).min(self.h - 1);
            for x in 0..w {
                let sx = (x * self.w / w).min(self.w - 1);
                out.px[(y * w + x) as usize] = self.px[(sy * self.w + sx) as usize];
            }
        }
        out
    }
}

/// Multiplies the three channels of `p` by `shade`/256 (`shade` up to 256).
#[inline]
fn shaded(p: u32, shade: u32) -> u32 {
    if shade >= 256 {
        return p & 0xffffff;
    }
    ((p & 0xff00ff) * shade >> 8 & 0xff00ff) | ((p & 0x00ff00) * shade >> 8 & 0x00ff00)
}

/// Copies `p` onto the canvas at (x, y), its colours multiplied by `shade`/256, only rows from `clip.0` to
/// `clip.1` (exclusive).
pub fn blit(c: &mut Canvas, p: &Pic, x: i32, y: i32, shade: u32, clip: (i32, i32)) {
    let (y0, y1) = (y.max(clip.0).max(0), (y + p.h).min(clip.1).min(c.h));
    let (x0, x1) = (x.max(0), (x + p.w).min(c.w));
    if x0 >= x1 {
        return;
    }
    for row in y0..y1 {
        let src = &p.px[((row - y) * p.w + (x0 - x)) as usize..((row - y) * p.w + (x1 - x)) as usize];
        let dst = &mut c.buf[(row * c.w + x0) as usize..(row * c.w + x1) as usize];
        for (d, &s) in dst.iter_mut().zip(src) {
            let a = s >> 24;
            if a == 255 {
                *d = shaded(s, shade);
            } else if a != 0 {
                *d = mix(*d, shaded(s, shade), a);
            }
        }
    }
}

/// Darkens every pixel of rows `y0..y1` to `keep`/256 of its brightness.
pub fn dim(c: &mut Canvas, y0: i32, y1: i32, keep: u32) {
    let (y0, y1) = (y0.clamp(0, c.h), y1.clamp(0, c.h));
    for p in &mut c.buf[(y0 * c.w) as usize..(y1 * c.w) as usize] {
        *p = shaded(*p, keep);
    }
}

/// `fade_band` towards `page` instead of black (the light page); black is `fade_band` itself.
pub fn fade_band_to(c: &mut Canvas, y0: i32, y1: i32, page: u32) {
    if page == 0 {
        return fade_band(c, y0, y1);
    }
    let n = (y1 - y0).abs().max(1);
    for k in 0..n {
        let row = if y1 > y0 { y0 + k } else { y0 - k };
        if row < 0 || row >= c.h {
            continue;
        }
        let keep = 256 - (k * 256 / n) as u32;
        let keep = keep * keep / 256;
        for p in &mut c.buf[(row * c.w) as usize..((row + 1) * c.w) as usize] {
            *p = mix(*p, page, 255 - keep.min(255));
        }
    }
}

/// Black rising over rows `y0..y1`: none at `y0`, all at `y1` (or the other way round when `y0 > y1`).
pub fn fade_band(c: &mut Canvas, y0: i32, y1: i32) {
    let n = (y1 - y0).abs().max(1);
    for k in 0..n {
        let row = if y1 > y0 { y0 + k } else { y0 - k };
        if row < 0 || row >= c.h {
            continue;
        }
        let keep = 256 - (k * 256 / n) as u32;
        let keep = keep * keep / 256;
        for p in &mut c.buf[(row * c.w) as usize..((row + 1) * c.w) as usize] {
            *p = shaded(*p, keep);
        }
    }
}

/// A soft shadow: the rectangle (x, y, w, h) darkened by `strength` (0-1), fading out over `blur` pixels.
/// The fall-off is the product of one across and one down, so a frame pays a multiply per pixel.
pub fn soft_shadow(c: &mut Canvas, x: i32, y: i32, w: i32, h: i32, blur: i32, strength: f32, clip: (i32, i32)) {
    if strength <= 0.0 || w <= 0 || h <= 0 {
        return;
    }
    let blur = blur.max(1);
    let fall = |d: i32| {
        let t = 1.0 - d as f32 / blur as f32;
        if t <= 0.0 {
            0.0
        } else {
            t * t
        }
    };
    let (x0, x1) = ((x - blur).max(0), (x + w + blur).min(c.w));
    let cols: Vec<f32> = (x0..x1)
        .map(|col| {
            fall(if col < x {
                x - col
            } else if col >= x + w {
                col - (x + w) + 1
            } else {
                0
            })
        })
        .collect();
    for row in (y - blur).max(clip.0).max(0)..(y + h + blur).min(clip.1).min(c.h) {
        let fy = strength
            * fall(if row < y {
                y - row
            } else if row >= y + h {
                row - (y + h) + 1
            } else {
                0
            });
        if fy <= 0.0 {
            continue;
        }
        let line = &mut c.buf[(row * c.w + x0) as usize..(row * c.w + x1) as usize];
        for (p, fx) in line.iter_mut().zip(&cols) {
            let a = fy * fx;
            if a > 0.004 {
                *p = shaded(*p, 256 - (a * 256.0) as u32);
            }
        }
    }
}

// ---- wood ---------------------------------------------------------------------------------------------

/// Width of the cabinet's side walls as seen from the front.
pub const WALL: i32 = 30;

fn hash(n: u32) -> u32 {
    let mut x = n.wrapping_mul(0x9E37_79B9) ^ 0x85EB_CA6B;
    x ^= x >> 15;
    x = x.wrapping_mul(0x2C1B_3C6D);
    x ^= x >> 12;
    x = x.wrapping_mul(0x297A_2D39);
    x ^ (x >> 15)
}

/// Smooth value noise, 0-1.
fn noise(x: f32, seed: u32) -> f32 {
    let i = x.floor();
    let f = x - i;
    let a = (hash(i as i32 as u32 ^ seed.wrapping_mul(7919)) & 0xffff) as f32 / 65535.0;
    let b = (hash((i as i32 + 1) as u32 ^ seed.wrapping_mul(7919)) & 0xffff) as f32 / 65535.0;
    let s = f * f * (3.0 - 2.0 * f);
    a + (b - a) * s
}

/// Grain brightness across the grain, one value per pixel: about 0.86-1.06, with the odd dark streak.
fn grain_table(len: usize, seed: u32) -> Vec<f32> {
    (0..len)
        .map(|u| {
            let u = u as f32;
            let broad = noise(u * 0.035, seed);
            let fine = noise(u * 0.23, seed ^ 11);
            let streak = noise(u * 0.9, seed ^ 17);
            0.86 + 0.13 * broad + 0.07 * fine + if streak > 0.86 { -0.06 } else { 0.0 }
        })
        .collect()
}

/// How far the grain wanders sideways at `v` along it: wood is never quite straight.
fn wander(v: f32, seed: u32) -> f32 {
    5.0 * (v * 0.011 + seed as f32).sin() + 9.0 * noise(v * 0.003, seed ^ 5) + 16.0
}

/// The grain at `u` across, shifted by how far it wanders there.
#[inline]
fn grain(table: &[f32], u: f32, shift: f32) -> f32 {
    let i = (u + shift).max(0.0) as usize;
    table[i.min(table.len() - 1)]
}

fn scale_rgb(c: u32, k: f32) -> u32 {
    let ch = |s: u32| (((c >> s & 0xff) as f32 * k).round().clamp(0.0, 255.0) as u32) << s;
    ch(16) | ch(8) | ch(0)
}

const BACK: u32 = 0x2c2019;
const BOARD_TOP: u32 = 0x6e523d;
const BOARD_EDGE: u32 = 0x56402f;
const WALL_FACE: u32 = 0x4f3a2b;

/// A wood colour in the appearance in use: the dark walnut above, or a light oak on the light page (the same
/// grain and shading, so it reads as the same cabinet in a lighter wood).
fn wood(walnut: u32) -> u32 {
    let oak = match walnut {
        BACK => 0xb09372,
        BOARD_TOP => 0xe2cca8,
        BOARD_EDGE => 0xc9a87f,
        _ => 0xc2a07a,
    };
    crate::palette::pal().pick(walnut, oak)
}

/// One compartment of the cabinet, `w` wide: the back panel `inner` pixels high (shaded under the board above
/// and towards the walls), the walls at both sides, then the board the cases stand on (`BOARD` high).
pub fn compartment(w: i32, inner: i32, seed: u32) -> Pic {
    let h = inner + BOARD;
    let mut p = Pic::new(w, h, OPAQUE);
    let (back_c, wall_face, board_top, board_edge) = (wood(BACK), wood(WALL_FACE), wood(BOARD_TOP), wood(BOARD_EDGE));
    let top_face = 9;
    let back = grain_table(w as usize + 64, seed);
    let walls = grain_table(4 * WALL as usize + 64, seed ^ 0x51);
    let board = grain_table(3 * BOARD as usize + 64, seed ^ 0x77);
    let drift: Vec<f32> = (0..w)
        .map(|x| 3.0 * (x as f32 * 0.004 + seed as f32).sin() + 4.0 + 2.0 * noise(x as f32 * 0.02, seed))
        .collect();
    // Per column: how far from a wall (darker near them), and whether it is the wall's inner side.
    let from_wall: Vec<i32> = (0..w).map(|x| (x - WALL).min(w - WALL - 1 - x)).collect();
    let col_ao: Vec<f32> = from_wall.iter().map(|&d| 0.72 + 0.28 * (d as f32 / 60.0).clamp(0.0, 1.0)).collect();
    for y in 0..h {
        let shift = wander(y as f32 + seed as f32 * 300.0, seed);
        let row = &mut p.px[(y * w) as usize..((y + 1) * w) as usize];
        // The walls' front edges: vertical grain, a lit edge on the inside, a dark one outside.
        for x in (0..WALL).chain(w - WALL..w) {
            let local = if x < WALL { x } else { w - 1 - x };
            let side = if x < WALL { 0.0 } else { WALL as f32 * 1.7 };
            let edge = if local == WALL - 1 {
                1.22
            } else if local == 0 {
                0.7
            } else {
                1.0
            };
            row[x as usize] = OPAQUE | fast(wall_face, grain(&walls, local as f32 * 1.6 + side, shift * 0.5) * edge);
        }
        if y < inner {
            // The back panel, in shadow under the board above, near the walls and at the floor.
            let top = (y as f32 / 70.0).min(1.0);
            let row_ao = (0.42 + 0.58 * top * top) * if y > inner - 10 { 0.86 } else { 1.0 };
            let side_k = 0.62 * (0.75 + 0.25 * top);
            let i = shift.max(0.0) as usize;
            for x in WALL..w - WALL {
                let k = back[(x as usize + i).min(back.len() - 1)];
                row[x as usize] = OPAQUE
                    | if from_wall[x as usize] < 10 {
                        // The wall's inner side, seen at an angle.
                        fast(wall_face, side_k * k)
                    } else {
                        fast(back_c, k * row_ao * col_ao[x as usize])
                    };
            }
        } else if y < inner + top_face {
            // The board's top, lighter towards its front.
            let t = 0.82 + 0.18 * (y - inner) as f32 / top_face as f32;
            for x in WALL..w - WALL {
                row[x as usize] = OPAQUE | fast(board_top, grain(&board, (y - inner) as f32 * 3.0, drift[x as usize]) * t);
            }
        } else {
            // The board's front edge: a lit top line, grain along it, dark underneath.
            let e = y - inner - top_face;
            let edge = match e {
                0 => 1.32,
                1 => 1.14,
                _ if e >= BOARD - top_face - 2 => 0.62,
                _ => 1.0 - (e as f32 / (BOARD - top_face) as f32) * 0.12,
            };
            for x in WALL..w - WALL {
                row[x as usize] = OPAQUE | fast(board_edge, grain(&board, (y - inner) as f32 * 2.6, drift[x as usize] * 1.5) * edge);
            }
        }
    }
    p
}

/// `scale_rgb` in whole numbers, for the hundreds of thousands of pixels of a compartment.
#[inline]
fn fast(c: u32, k: f32) -> u32 {
    let k = (k.max(0.0) * 256.0) as u32;
    let ch = |s: u32| (((c >> s & 0xff) * k) >> 8).min(255) << s;
    ch(16) | ch(8) | ch(0)
}

/// The cabinet's top board, `h` high, above the first shelf.
pub fn top_board(w: i32, h: i32) -> Pic {
    let mut p = Pic::new(w, h, OPAQUE);
    let board_edge = wood(BOARD_EDGE);
    let table = grain_table(3 * h as usize + 64, 0x1234);
    for y in 0..h {
        for x in 0..w {
            let drift = 3.0 * (x as f32 * 0.004).sin() + 4.0 + 2.0 * noise(x as f32 * 0.02, 9);
            let k = grain(&table, y as f32 * 2.2, drift)
                * if y == 0 {
                    1.3
                } else if y >= h - 3 {
                    0.66
                } else {
                    1.0
                };
            p.px[(y * w + x) as usize] = OPAQUE | scale_rgb(board_edge, k);
        }
    }
    p
}

// ---- cases ----------------------------------------------------------------------------------------------

/// The colours of one game's case: the spine and label colour from its cover, its system's colour for the
/// band at the top of a disc case's spine, and the plastic of a cartridge.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Look {
    pub color: u32,
    pub band: u32,
    pub plastic: u32,
}

/// Cartridge plastic per system, muted; others get a dark grey.
pub fn plastic(system: &str) -> u32 {
    match system {
        "gb" | "gbh" | "virtualboy" => 0xa9a6a0,
        "gbc" | "gbch" => 0x8d9097,
        "gba" | "gbah" => 0x3f3e48,
        "nes" | "nesh" | "famicom" | "fds" => 0x8b8a87,
        "snes" | "snesh" | "sfc" => 0x9c99a2,
        "n64" => 0x3a3a3f,
        "megadrive" | "genesis" | "megadrive-japan" | "megadriveh" | "genh" | "sega32x" | "mastersystem" | "sg-1000" => 0x1f1f21,
        "pico-8" | "pico8" | "tic-80" => 0x2c2a35,
        _ => 0x2e2e33,
    }
}

/// The title along a spine, read top to bottom: `len` pixels from `y0`, centred across `x0 .. x0 + thick`.
#[allow(clippy::too_many_arguments)]
pub fn spine_title(text: &mut Text, p: &mut Pic, title: &str, px: u32, x0: i32, thick: i32, y0: i32, len: i32, ink: u32) {
    // As thick as the case: the design's size whatever Settings → Text size says (`type_scale`).
    let _fixed = crate::type_scale::fixed();
    let title = text.fit(Face::Title, px, title, len);
    let tw = text.width(Face::Title, px, &title).max(1);
    let th = (px as i32 * 3 / 2).max(1);
    let mut buf = vec![0u32; (tw + 4) as usize * th as usize];
    {
        let mut c = Canvas::new(&mut buf, (tw + 4) as u32, th as u32);
        text.draw(&mut c, Face::Title, px, 2, px as i32, 0xffffff, &title);
    }
    let bw = tw + 4;
    let rows: Vec<i32> = (0..th).filter(|&r| (0..bw).any(|x| buf[(r * bw + x) as usize] & 0xff != 0)).collect();
    let (Some(&top), Some(&bottom)) = (rows.first(), rows.last()) else { return };
    // Centred on the capitals and the lower case, not on the box with room for descenders.
    let ink_h = bottom - top + 1;
    let right = x0 + (thick + ink_h) / 2 - 1;
    let start = y0 + (len - tw) / 2;
    for ty in top..=bottom {
        for tx in 0..bw {
            let a = buf[(ty * bw + tx) as usize] & 0xff;
            if a > 0 {
                p.blend(right - (ty - top), start + tx - 2, ink, a);
            }
        }
    }
}

/// A vertical light gradient over the solid part: a little lighter at the top, darker at the bottom.
fn light(p: &mut Pic, top: f32, bottom: f32) {
    for y in 0..p.h {
        let k = top + (bottom - top) * y as f32 / (p.h - 1).max(1) as f32;
        for x in 0..p.w {
            let i = (y * p.w + x) as usize;
            let d = p.px[i];
            if d >> 24 != 0 {
                p.px[i] = (d & 0xff00_0000) | scale_rgb(d & 0xffffff, k);
            }
        }
    }
}

/// The spine of a case as it stands on the shelf, `k` times its shelf size.
pub fn spine(text: &mut Text, ff: FormFactor, k: f32, look: Look, title: &str) -> Pic {
    let size = ff.size();
    let (w, h) = ((size.spine as f32 * k).round() as i32, (size.h as f32 * k).round() as i32);
    let s = |v: f32| (v * k).round() as i32;
    let ink = ink_on(look.color);
    // A Game Boy cartridge's label is short: a size smaller gets more of the name on it.
    let title_px = ((if ff == FormFactor::HandheldCart { 21.0 } else { 24.0 }) * k).round() as u32;
    let mut p = Pic::new(w, h, 0);
    match ff {
        FormFactor::HandheldCart | FormFactor::ConsoleCart => {
            let console = ff == FormFactor::ConsoleCart;
            p.fill(0, 0, w, h, OPAQUE | look.plastic);
            // Grip grooves at the top, then a paper label with the title.
            let (g0, grooves, gap) = if console { (s(9.0), 6, s(6.0)) } else { (s(6.0), 2, s(5.0)) };
            for g in 0..grooves {
                let y = g0 + g * gap;
                p.tint(s(3.0), y, w - s(6.0), s(2.0).max(1), 0x000000, 110);
                p.tint(s(3.0), y + s(2.0).max(1), w - s(6.0), 1, 0xffffff, 40);
            }
            let inset = if console { s(5.0) } else { s(4.0) };
            let (ly, lh) = (g0 + grooves * gap + s(6.0), h - (g0 + grooves * gap + s(6.0)) - s(10.0));
            p.fill(inset, ly, w - 2 * inset, lh, OPAQUE | look.color);
            p.tint(inset, ly, w - 2 * inset, 1, 0xffffff, 50);
            p.tint(inset, ly + lh - 1, w - 2 * inset, 1, 0x000000, 70);
            spine_title(text, &mut p, title, title_px, inset, w - 2 * inset, ly + s(6.0), lh - s(12.0), ink);
            // The plastic's edges catch the light on the left and fall off on the right.
            p.tint(0, 0, s(2.0).max(1), h, 0xffffff, 45);
            p.tint(w - s(2.0).max(1), 0, s(2.0).max(1), h, 0x000000, 90);
            light(&mut p, 1.06, 0.84);
            let r = if console { 7.0 } else { 5.0 } * k;
            p.round([r, r, 1.5 * k, 1.5 * k]);
        }
        FormFactor::JewelCase => {
            // A paper spine behind clear plastic; the hinge's ridges at both ends.
            p.fill(0, 0, w, h, OPAQUE | look.color);
            let band = s(20.0);
            p.fill(0, 0, w, band, OPAQUE | look.band);
            let ridge = s(9.0);
            for (y, hh) in [(0, ridge), (h - ridge, ridge)] {
                p.tint(0, y, w, hh, 0xc8ccd2, 90);
                for k2 in (0..hh).step_by(s(3.0).max(2) as usize) {
                    p.tint(0, y + k2, w, 1, 0x000000, 60);
                }
            }
            spine_title(text, &mut p, title, title_px, 0, w, band + s(8.0), h - band - ridge - s(16.0), ink);
            p.tint(s(4.0), 0, s(2.0).max(1), h, 0xffffff, 55);
            p.tint(0, 0, 1, h, 0xffffff, 70);
            p.tint(w - 1, 0, 1, h, 0x000000, 90);
            light(&mut p, 1.06, 0.86);
            p.round([1.5 * k; 4]);
        }
        FormFactor::DvdCase | FormFactor::UmdCase | FormFactor::CardBox => {
            // The plastic case's rim around the insert's spine, a band in the system's colour at the top.
            let rim_color = if ff == FormFactor::CardBox { 0x6d6f74 } else { 0x141416 };
            p.fill(0, 0, w, h, OPAQUE | rim_color);
            let rim = s(2.5).max(2);
            let band = match ff {
                FormFactor::DvdCase => s(34.0),
                FormFactor::UmdCase => s(28.0),
                _ => s(24.0),
            };
            p.fill(rim, rim, w - 2 * rim, h - 2 * rim, OPAQUE | look.color);
            p.fill(rim, rim, w - 2 * rim, band, OPAQUE | look.band);
            p.tint(rim, rim + band, w - 2 * rim, 1, 0x000000, 60);
            spine_title(
                text,
                &mut p,
                title,
                title_px,
                rim,
                w - 2 * rim,
                rim + band + s(8.0),
                h - 2 * rim - band - s(16.0),
                ink,
            );
            // Gloss of the clear sleeve.
            p.tint(rim + s(4.0), rim, s(2.0).max(1), h - 2 * rim, 0xffffff, 50);
            p.tint(0, 0, 1, h, 0xffffff, 40);
            light(&mut p, 1.05, 0.85);
            p.round([3.0 * k; 4]);
        }
    }
    p
}

/// Where the cover sits on a case's front, `w` x `h`: (x, y, width, height).
pub fn label_rect(ff: FormFactor, w: i32, h: i32) -> (i32, i32, i32, i32) {
    let f = |v: f32, of: i32| (v * of as f32).round() as i32;
    match ff {
        FormFactor::ConsoleCart => (f(0.1, w), f(0.25, h), w - 2 * f(0.1, w), h - f(0.25, h) - f(0.08, h)),
        FormFactor::HandheldCart => (f(0.12, w), f(0.19, h), w - 2 * f(0.12, w), f(0.56, h)),
        FormFactor::JewelCase => (f(0.07, w), 0, w - f(0.07, w), h),
        FormFactor::DvdCase | FormFactor::UmdCase => (f(0.035, w), f(0.02, h), w - f(0.035, w) - f(0.02, w), h - 2 * f(0.02, h)),
        FormFactor::CardBox => (f(0.04, w), f(0.03, h), w - f(0.04, w) - f(0.03, w), h - 2 * f(0.03, h)),
    }
}

/// The size a case's front is made at for `k` times its shelf size.
pub fn front_size(ff: FormFactor, k: f32) -> (i32, i32) {
    let size = ff.size();
    ((size.front as f32 * k).round() as i32, (size.h as f32 * k).round() as i32)
}

/// `img` cut to the shape `w` x `h` (centred) and box-filtered to that size: how a cover fills a label.
pub fn cover_fill(img: &Image, w: i32, h: i32) -> Image {
    let (w, h) = (w.max(1) as u32, h.max(1) as u32);
    if img.w == 0 || img.h == 0 {
        return Image {
            w,
            h,
            px: vec![0; (w * h) as usize],
        };
    }
    // The part of the picture with the label's shape.
    let (sw, sh) = if img.w as u64 * h as u64 > img.h as u64 * w as u64 {
        ((img.h as u64 * w as u64 / h as u64).max(1) as u32, img.h)
    } else {
        (img.w, (img.w as u64 * h as u64 / w as u64).max(1) as u32)
    };
    let (ox, oy) = ((img.w - sw) / 2, (img.h - sh) / 2);
    let mut px = Vec::with_capacity((w * h) as usize);
    for ty in 0..h {
        let (y0, y1) = (oy + ty * sh / h, oy + ((ty + 1) * sh / h).max(ty * sh / h + 1));
        for tx in 0..w {
            let (x0, x1) = (ox + tx * sw / w, ox + ((tx + 1) * sw / w).max(tx * sw / w + 1));
            let (mut r, mut g, mut b, mut n) = (0u32, 0u32, 0u32, 0u32);
            for sy in y0..y1.min(img.h) {
                for sx in x0..x1.min(img.w) {
                    let p = img.px[(sy * img.w + sx) as usize];
                    r += p >> 16 & 0xff;
                    g += p >> 8 & 0xff;
                    b += p & 0xff;
                    n += 1;
                }
            }
            let n = n.max(1);
            px.push((r / n) << 16 | (g / n) << 8 | b / n);
        }
    }
    Image { w, h, px }
}

/// A label for a game without box art: its colour, its console and its name, clearly not a real cover.
pub fn plain_label(text: &mut Text, p: &mut Pic, (x, y, w, h): (i32, i32, i32, i32), look: Look, title: &str, art: Option<&Sprite>) {
    for row in 0..h {
        let c = mix(look.color, 0x000000, 20 + (row as u32 * 90 / h.max(1) as u32));
        p.fill(x, y + row, w, 1, OPAQUE | c);
    }
    let ink = ink_on(mix(look.color, 0x000000, 60));
    let mut text_top = y + h / 8;
    if let Some(art) = art.filter(|_| h > 120) {
        let fitted = art.fitted(w * 3 / 4, h * 2 / 5);
        let (ax, ay) = (x + (w - fitted.w as i32) / 2, y + h / 10);
        for sy in 0..fitted.h as i32 {
            for sx in 0..fitted.w as i32 {
                let s = fitted.px[(sy * fitted.w as i32 + sx) as usize];
                if s >> 24 != 0 {
                    p.blend(ax + sx, ay + sy, s & 0xffffff, 255);
                }
            }
        }
        text_top = ay + fitted.h as i32 + h / 14;
    }
    let px = ((w as f32 / 6.0).clamp(22.0, 50.0)) as u32;
    // Printed on the cover: its size follows the case, not Settings → Text size.
    let _fixed = crate::type_scale::fixed();
    let lines = text.wrap(Face::Title, px, title, w - w / 6);
    let mut base = text_top + px as i32;
    for line in lines.iter().take(3) {
        let line = text.fit(Face::Title, px, line, w - w / 6);
        let mut buf = vec![0u32; (w * (px as i32 * 3 / 2)) as usize];
        {
            let mut c = Canvas::new(&mut buf, w as u32, (px * 3 / 2) as u32);
            text.draw(&mut c, Face::Title, px, w / 12, px as i32, 0xffffff, &line);
        }
        for ty in 0..px as i32 * 3 / 2 {
            for tx in 0..w {
                let a = buf[(ty * w + tx) as usize] & 0xff;
                p.blend(x + tx, base - px as i32 + ty, ink, a);
            }
        }
        base += px as i32 * 11 / 10;
        if base > y + h {
            break;
        }
    }
}

/// The front of a case, `k` times its shelf size: the cover (`cover`, already `cover_fill`ed to the label's
/// size) in the case's frame, or a plain label with the console (`art`) and the title.
pub fn front(text: &mut Text, ff: FormFactor, k: f32, look: Look, title: &str, cover: Option<&Image>, art: Option<&Sprite>) -> Pic {
    let (w, h) = front_size(ff, k);
    let s = |v: f32| (v * k).round() as i32;
    let mut p = Pic::new(w, h, 0);
    let label = label_rect(ff, w, h);
    let base = match ff {
        FormFactor::HandheldCart | FormFactor::ConsoleCart => look.plastic,
        FormFactor::CardBox => 0xbfc1c5,
        FormFactor::JewelCase => look.color,
        _ => 0x121214,
    };
    p.fill(0, 0, w, h, OPAQUE | base);
    if ff.is_cart() {
        // Grip grooves above the label.
        let (g0, n, gap) = if ff == FormFactor::ConsoleCart {
            (h / 18, 6, h / 34)
        } else {
            (h / 26, 4, h / 40)
        };
        for g in 0..n {
            let y = g0 + g * gap.max(3);
            p.tint(w / 8, y, w - w / 4, s(1.5).max(1), 0x000000, 100);
            p.tint(w / 8, y + s(1.5).max(1), w - w / 4, 1, 0xffffff, 36);
        }
        // The label sits in a recess: dark above and left, light below and right.
        let (lx, ly, lw, lh) = label;
        let e = s(2.0).max(1);
        p.tint(lx - e, ly - e, lw + 2 * e, e, 0x000000, 110);
        p.tint(lx - e, ly, e, lh, 0x000000, 90);
        p.tint(lx - e, ly + lh, lw + 2 * e, e, 0xffffff, 40);
        p.tint(lx + lw, ly, e, lh, 0xffffff, 30);
    }
    match cover {
        Some(img) => {
            let (lx, ly, lw, lh) = label;
            for y in 0..lh.min(img.h as i32) {
                for x in 0..lw.min(img.w as i32) {
                    p.px[((ly + y) * w + lx + x) as usize] = OPAQUE | img.px[(y as u32 * img.w + x as u32) as usize];
                }
            }
        }
        None => plain_label(text, &mut p, label, look, title, art),
    }
    match ff {
        FormFactor::JewelCase => {
            // The hinge strip at the left, clear plastic with ridges; a sheen across the lid.
            let hw = label.0;
            p.fill(0, 0, hw, h, OPAQUE | mix(look.color, 0x000000, 150));
            for x in (0..hw).step_by(s(3.0).max(2) as usize) {
                p.tint(x, 0, 1, h, 0xffffff, 40);
            }
            p.tint(hw, 0, 1, h, 0x000000, 120);
            p.tint(hw + 1, 0, 1, h, 0xffffff, 60);
            sheen(&mut p, 26);
            p.tint(0, 0, w, 1, 0xffffff, 80);
            p.tint(w - 1, 0, 1, h, 0xffffff, 50);
            p.round([2.0 * k; 4]);
        }
        FormFactor::DvdCase | FormFactor::UmdCase | FormFactor::CardBox => {
            sheen(&mut p, 18);
            p.tint(0, 0, w, s(1.0).max(1), 0xffffff, 40);
            let r = if ff == FormFactor::UmdCase { 9.0 } else { 4.0 } * k;
            p.round([r, r, r, r]);
        }
        FormFactor::ConsoleCart | FormFactor::HandheldCart => {
            light(&mut p, 1.05, 0.9);
            p.tint(0, 0, w, s(1.5).max(1), 0xffffff, 50);
            if ff == FormFactor::HandheldCart {
                // The Game Boy cartridge's cut corner.
                let cut = w / 7;
                for y in 0..cut {
                    for x in w - cut + y..w {
                        p.px[(y * w + x) as usize] = 0;
                    }
                }
                p.round([4.0 * k, 0.0, 3.0 * k, 3.0 * k]);
            } else {
                p.round([9.0 * k, 9.0 * k, 3.0 * k, 3.0 * k]);
            }
        }
    }
    p
}

/// A soft diagonal reflection of clear plastic over the front.
pub fn sheen(p: &mut Pic, strength: u32) {
    let (w, h) = (p.w as f32, p.h as f32);
    for y in 0..p.h {
        for x in 0..p.w {
            let d = (x as f32 / w) * 0.8 + (y as f32 / h) * 0.6;
            let band = (1.0 - ((d - 0.42) / 0.16).abs()).max(0.0);
            if band > 0.0 {
                let i = (y * p.w + x) as usize;
                let px = p.px[i];
                if px >> 24 != 0 {
                    p.px[i] = (px & 0xff00_0000) | mix(px & 0xffffff, 0xffffff, (band * strength as f32) as u32);
                }
            }
        }
    }
}

// ---- a case turning towards you ---------------------------------------------------------------------------

/// Where a case is drawn while it comes out: its middle on screen, how big (screen pixels per shelf pixel),
/// how far it has turned (0 = spine to you, 1 = front to you), and how lit (0-256).
#[derive(Clone, Copy, Debug)]
pub struct Turn {
    pub cx: f32,
    pub cy: f32,
    pub scale: f32,
    pub turned: f32,
    pub shade: u32,
}

const FOCAL: f32 = 1700.0;

/// Draws the case of `size` with its `spine` and `front` pictures (made at any scale) as `turn` says, in a
/// simple perspective: each visible face is a run of columns whose height follows its distance. Returns the
/// screen rectangle it covered.
pub fn draw_turning(c: &mut Canvas, spine: &Pic, front: &Pic, size: Size, turn: Turn, clip: (i32, i32)) -> (i32, i32, i32, i32) {
    let th = turn.turned.clamp(0.0, 1.0) * std::f32::consts::FRAC_PI_2;
    let (sin, cos) = th.sin_cos();
    let (d, w, h) = (size.spine as f32 * turn.scale, size.front as f32 * turn.scale, size.h as f32 * turn.scale);
    // Top view, box centre at the origin, z away from you: the spine is the near face, the front its right side.
    let rot = |x: f32, z: f32| (x * cos + z * sin, -x * sin + z * cos);
    let p1 = rot(-d / 2.0, -w / 2.0);
    let p2 = rot(d / 2.0, -w / 2.0);
    let p3 = rot(d / 2.0, w / 2.0);
    // The corner between spine and front is always the nearest: it keeps its size, the rest recedes.
    let zref = p2.1;
    let project = |(x, z): (f32, f32)| {
        let k = (FOCAL + zref) / (FOCAL + z);
        (turn.cx + x * k, k)
    };
    let (a, b, e) = (project(p1), project(p2), project(p3));
    let mut covered = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
    // The spine faces you until the turn is a right angle; the front from the first degree on.
    if cos > 0.001 {
        face(c, spine, a, b, turn.cy, h, shade_for(cos, turn.shade), clip, &mut covered);
    }
    if sin > 0.001 {
        face(c, front, b, e, turn.cy, h, shade_for(sin, turn.shade), clip, &mut covered);
    }
    (covered.0, covered.1, covered.2 - covered.0, covered.3 - covered.1)
}

fn shade_for(facing: f32, shade: u32) -> u32 {
    ((0.5 + 0.5 * facing) * shade as f32) as u32
}

/// One face from edge `a` to edge `b`, each (screen x, size factor); the picture runs left to right along it.
#[allow(clippy::too_many_arguments)]
fn face(c: &mut Canvas, tex: &Pic, a: (f32, f32), b: (f32, f32), cy: f32, h: f32, shade: u32, clip: (i32, i32), covered: &mut (i32, i32, i32, i32)) {
    if b.0 - a.0 < 0.5 {
        return;
    }
    let (x0, x1) = (a.0.floor() as i32, b.0.ceil() as i32);
    for col in x0.max(0)..x1.min(c.w) {
        let t = ((col as f32 + 0.5 - a.0) / (b.0 - a.0)).clamp(0.0, 1.0);
        // Perspective-correct position across the face.
        let u = t * b.1 / ((1.0 - t) * a.1 + t * b.1);
        let k = a.1 + (b.1 - a.1) * t;
        let ch = h * k;
        let top = cy - ch / 2.0;
        let tx = ((u * tex.w as f32) as i32).clamp(0, tex.w - 1);
        let (r0, r1) = ((top.floor() as i32).max(clip.0).max(0), ((top + ch).ceil() as i32).min(clip.1).min(c.h));
        if r0 >= r1 {
            continue;
        }
        let step = tex.h as f32 / ch;
        covered.0 = covered.0.min(col);
        covered.2 = covered.2.max(col + 1);
        covered.1 = covered.1.min(r0);
        covered.3 = covered.3.max(r1);
        for row in r0..r1 {
            let ty = (((row as f32 + 0.5 - top) * step) as i32).clamp(0, tex.h - 1);
            let s = tex.px[(ty * tex.w + tx) as usize];
            let alpha = s >> 24;
            if alpha == 0 {
                continue;
            }
            let i = (row * c.w + col) as usize;
            // Edge columns are only partly covered.
            let edge = if col == x0 {
                1.0 - (a.0 - x0 as f32)
            } else if col == x1 - 1 {
                1.0 - (x1 as f32 - b.0)
            } else {
                1.0
            };
            let alpha = (alpha as f32 * edge.clamp(0.0, 1.0)) as u32;
            c.buf[i] = if alpha >= 255 {
                shaded(s, shade)
            } else {
                mix(c.buf[i], shaded(s, shade), alpha)
            };
        }
    }
}

// ---- into the console ---------------------------------------------------------------------------------

/// A compact disc seen from its data side, radius `r`, a hint of the cover's colour in the sheen.
pub fn disc(c: &mut Canvas, cx: i32, cy: i32, r: i32, tint: u32, clip_bottom: i32) {
    let rf = r as f32;
    for dy in -r..=r {
        let row = cy + dy;
        if row < 0 || row >= c.h || row >= clip_bottom {
            continue;
        }
        for dx in -r..=r {
            let col = cx + dx;
            if col < 0 || col >= c.w {
                continue;
            }
            let d = ((dx * dx + dy * dy) as f32).sqrt();
            let cover = (rf - d + 0.5).clamp(0.0, 1.0);
            if cover <= 0.0 || d < rf * 0.075 {
                continue;
            }
            let ang = (dy as f32).atan2(dx as f32);
            let color = if d < rf * 0.2 {
                0x8e939a
            } else if d < rf * 0.3 {
                0xb9bec5
            } else {
                // Silver with a rainbow arc where the light falls.
                let sheen = (ang * 2.0 + 0.6).cos().max(0.0).powi(3);
                let hue = ((ang + std::f32::consts::PI) * 3.0 + d * 0.05) % 6.0;
                let rainbow = match hue as i32 {
                    0 => 0xe8a0c0,
                    1 => 0xa8a0e8,
                    2 => 0x90d0e8,
                    3 => 0xa0e8b0,
                    4 => 0xe8e0a0,
                    _ => 0xe8b890,
                };
                let base = mix(0xc4c8ce, tint, 18);
                mix(mix(base, 0x7c8088, ((d / rf) * 60.0) as u32), rainbow, (sheen * 120.0) as u32)
            };
            let i = (row * c.w + col) as usize;
            c.buf[i] = mix(c.buf[i], color, (cover * 255.0) as u32);
        }
    }
}

/// A UMD: a disc in a small shell with a window.
pub fn umd(c: &mut Canvas, cx: i32, cy: i32, r: i32, tint: u32, clip_bottom: i32) {
    let (w, h) = (r * 2, r * 2 + r / 5);
    let mut p = Pic::new(w, h, OPAQUE | 0x2a2b30);
    p.tint(0, 0, w, 2, 0xffffff, 60);
    p.round([r as f32 * 0.5, r as f32 * 0.5, r as f32 * 0.25, r as f32 * 0.25]);
    blit(c, &p, cx - w / 2, cy - h / 2, 256, (0, clip_bottom));
    disc(c, cx, cy + r / 12, r * 4 / 5, tint, clip_bottom);
}

/// Luma of a colour as a factor for picking a readable plate colour.
pub fn plate_ink(bg: u32) -> u32 {
    if luma(bg) > 120 {
        0x2a1f16
    } else {
        0xd9cbb4
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// CPU time the calling thread has used.
    fn thread_cpu() -> std::time::Duration {
        std::time::Duration::from_secs_f64(crate::run_ahead::thread_cpu_ms() / 1000.0)
    }

    #[test]
    fn the_wood_is_quick_to_make_and_darker_under_the_board_above() {
        // Wood is made on this thread only; its CPU time is not stretched by other work on a busy machine.
        let started = thread_cpu();
        let p = compartment(1208, 266, 1);
        let took = thread_cpu() - started;
        assert_eq!((p.w, p.h), (1208, 266 + BOARD));
        let lum = |y: i32| {
            (WALL + 100..p.w - WALL - 100)
                .map(|x| luma(p.px[(y * p.w + x) as usize] & 0xffffff))
                .sum::<u32>()
                / (p.w - 2 * WALL - 200) as u32
        };
        assert!(lum(4) < lum(150), "shadow under the board above: {} vs {}", lum(4), lum(150));
        assert!(p.px.iter().all(|px| px >> 24 == 0xff), "opaque");
        // A debug build is far slower than the release build the device runs; this only catches a regression by orders of magnitude.
        assert!(
            took < std::time::Duration::from_millis(if cfg!(debug_assertions) { 400 } else { 40 }),
            "{took:?}"
        );
    }

    #[test]
    fn a_case_facing_you_keeps_its_size_and_turned_shows_its_front_only() {
        let mut buf = vec![0u32; 800 * 600];
        let mut c = Canvas::new(&mut buf, 800, 600);
        let ff = FormFactor::DvdCase;
        let size = ff.size();
        let spine = Pic::new(size.spine, size.h, OPAQUE | 0xff0000);
        let front = Pic::new(size.front, size.h, OPAQUE | 0x00ff00);
        let r = draw_turning(
            &mut c,
            &spine,
            &front,
            size,
            Turn {
                cx: 400.0,
                cy: 300.0,
                scale: 1.0,
                turned: 0.0,
                shade: 256,
            },
            (0, 600),
        );
        assert!((r.2 - size.spine).abs() <= 1 && (r.3 - size.h).abs() <= 1, "spine at its shelf size: {r:?}");
        let green = c.buf.iter().filter(|p| **p & 0xffffff == 0x00ff00).count();
        assert_eq!(green, 0, "no front yet");
        c.buf.fill(0);
        let r = draw_turning(
            &mut c,
            &spine,
            &front,
            size,
            Turn {
                cx: 400.0,
                cy: 300.0,
                scale: 1.5,
                turned: 1.0,
                shade: 256,
            },
            (0, 600),
        );
        assert!((r.2 - (size.front as f32 * 1.5) as i32).abs() <= 2, "front at 1.5x: {r:?}");
        assert_eq!(c.buf.iter().filter(|p| **p & 0xff0000 == 0xff0000).count(), 0, "the spine has turned away");
        // Halfway, both show and the whole thing is narrower than spine plus front.
        c.buf.fill(0);
        let r = draw_turning(
            &mut c,
            &spine,
            &front,
            size,
            Turn {
                cx: 400.0,
                cy: 300.0,
                scale: 1.0,
                turned: 0.5,
                shade: 256,
            },
            (0, 600),
        );
        assert!(c.buf.iter().any(|p| *p & 0xff0000 != 0) && c.buf.iter().any(|p| *p & 0x00ff00 != 0));
        assert!(r.2 < size.spine + size.front, "{r:?}");
    }

    #[test]
    fn a_cover_fills_its_label_cut_to_shape() {
        // 4 x 2: left half red, right half blue; into a square label the middle is kept.
        let img = Image {
            w: 4,
            h: 2,
            px: vec![0xff0000, 0xff0000, 0x0000ff, 0x0000ff, 0xff0000, 0xff0000, 0x0000ff, 0x0000ff],
        };
        let out = cover_fill(&img, 2, 2);
        assert_eq!((out.w, out.h), (2, 2));
        assert_eq!(out.px[0], 0xff0000);
        assert_eq!(out.px[1], 0x0000ff);
    }

    #[test]
    fn blit_shades_and_respects_transparency() {
        let mut buf = vec![0x101010u32; 4];
        let mut c = Canvas::new(&mut buf, 2, 2);
        let mut p = Pic::new(2, 2, OPAQUE | 0x808080);
        p.px[1] = 0;
        blit(&mut c, &p, 0, 0, 128, (0, 2));
        assert_eq!(c.buf[0], 0x404040);
        assert_eq!(c.buf[1], 0x101010, "transparent pixel left alone");
    }
}
