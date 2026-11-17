//! Painting for the Cabinet look (`cabinet`): the wooden cabinet, the cases' spines and fronts, a case turning
//! towards you, and the cartridge or disc going into the console. Everything is drawn here in code; the only
//! pictures are the user's own box art and the console sprites of `system_art`.
//!
//! Spines and fronts are made once per game and kept (`app_cabinet.rs`); a frame copies them, so only the
//! case being turned is drawn pixel by pixel.

use crate::cabinet::{ink_on, luma, Size, BOARD};
use crate::gfx::{avg4, mix, Canvas};
use crate::image::Image;
use crate::system_art::Sprite;
use crate::text::{Face, Text};

/// A picture with straight alpha, 0xAARRGGBB.
///
/// Painted natively at `n` = 2 (docs/dev/native-paint.md) a picture keeps a half copy for each of the four places it
/// can sit within a screen pixel (`half`), made the first time it is drawn there: a blit then reads one value per screen
/// pixel instead of four. The copy is dropped when a method of `Pic` changes it and is not cloned; code that writes
/// `px` itself does so before the picture is first drawn (making it), as everywhere today.
pub struct Pic {
    pub w: i32,
    pub h: i32,
    pub px: Vec<u32>,
    half: [std::sync::OnceLock<Vec<u32>>; 4],
}

impl Clone for Pic {
    fn clone(&self) -> Pic {
        Pic::of(self.w, self.h, self.px.clone())
    }
}

const OPAQUE: u32 = 0xff00_0000;

/// In a half copy (`Pic::half`): all four design pixels see-through (the screen pixel stays), and some solid and some
/// not (worked out from the four). Solid pixels keep their mean; four identical translucent pixels keep that
/// pixel too, so a flat film needs one alpha blend, not four identical ones.
const HALF_CLEAR: u32 = 0;
const HALF_MIXED: u32 = 0x0100_0000;

impl Pic {
    pub fn new(w: i32, h: i32, argb: u32) -> Pic {
        let (w, h) = (w.max(1), h.max(1));
        Pic::of(w, h, vec![argb; (w * h) as usize])
    }

    /// The picture `w` x `h` of `px` (row by row, `w * h` pixels).
    pub fn of(w: i32, h: i32, px: Vec<u32>) -> Pic {
        Pic { w, h, px, half: Default::default() }
    }

    /// Something of the picture changed: its half copies are out of date.
    pub fn changed(&mut self) {
        for h in &mut self.half {
            if h.get().is_some() {
                h.take();
            }
        }
    }

    /// Its half copy for a place `fx`, `fy` (0 or 1) design pixels into a screen pixel: `(fx + w).div_ceil(2)` x
    /// `(fy + h).div_ceil(2)` screen pixels, each the mean of its four design pixels when all are solid (`OPAQUE` set),
    /// `HALF_CLEAR` when none is, the original pixel when all four are identical and translucent, `HALF_MIXED`
    /// otherwise (also where the cell reaches past the picture's edge).
    pub fn half(&self, fx: u8, fy: u8) -> (&[u32], usize) {
        let (fx, fy) = (fx as i32 & 1, fy as i32 & 1);
        let (hw, hh) = ((fx + self.w + 1) / 2, (fy + self.h + 1) / 2);
        let v = self.half[(fy * 2 + fx) as usize].get_or_init(|| {
            let mut out = Vec::with_capacity((hw * hh) as usize);
            let at = |x: i32, y: i32| if x < 0 || y < 0 || x >= self.w || y >= self.h { None } else { Some(self.px[(y * self.w + x) as usize]) };
            for j in 0..hh {
                for i in 0..hw {
                    let (x, y) = (2 * i - fx, 2 * j - fy);
                    let q = [at(x, y), at(x + 1, y), at(x, y + 1), at(x + 1, y + 1)];
                    out.push(match q {
                        [Some(a), Some(b), Some(c), Some(d)] if (a & b & c & d) >> 24 == 255 => OPAQUE | avg4(a, b, c, d),
                        _ if q.iter().all(|p| p.is_none_or(|p| p >> 24 == 0)) => HALF_CLEAR,
                        [Some(a), Some(b), Some(c), Some(d)] if a == b && a == c && a == d => a,
                        _ => HALF_MIXED,
                    });
                }
            }
            out
        });
        (v, hw as usize)
    }

    #[inline]
    pub fn get(&self, x: i32, y: i32) -> u32 {
        self.px[(y.clamp(0, self.h - 1) * self.w + x.clamp(0, self.w - 1)) as usize]
    }

    /// Paints `rgb` over (x, y) with coverage `a` (0-255); the pixel becomes at least that solid.
    #[inline]
    pub fn blend(&mut self, x: i32, y: i32, rgb: u32, a: u32) {
        self.changed();
        self.blend_changed(x, y, rgb, a);
    }

    /// `blend` for a painter that called `changed` once before many pixels (`plain_label`: a front's name and console
    /// pixel by pixel, the check per pixel cost more than the mixing).
    fn blend_changed(&mut self, x: i32, y: i32, rgb: u32, a: u32) {
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
        self.changed();
        for yy in y.max(0)..(y + h).min(self.h) {
            for xx in x.max(0)..(x + w).min(self.w) {
                self.px[(yy * self.w + xx) as usize] = argb;
            }
        }
    }

    /// Mixes `rgb` into the solid pixels of a rectangle with `a` (0-255).
    pub fn tint(&mut self, x: i32, y: i32, w: i32, h: i32, rgb: u32, a: u32) {
        self.changed();
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
        self.changed();
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

impl Pic {
    /// A copy `w` x `h` sampled between pixels (bilinear, alpha-weighted): for a smooth picture, a printed disc, that
    /// must stay smooth on its way into the console.
    pub fn scaled_smooth(&self, w: i32, h: i32) -> Pic {
        let (w, h) = (w.max(1), h.max(1));
        let mut out = Pic::new(w, h, 0);
        let (kx, ky) = (self.w as f32 / w as f32, self.h as f32 / h as f32);
        for y in 0..h {
            let v = (y as f32 + 0.5) * ky - 0.5;
            let (v0, dv) = (v.floor() as i32, v - v.floor());
            for x in 0..w {
                let u = (x as f32 + 0.5) * kx - 0.5;
                let (u0, du) = (u.floor() as i32, u - u.floor());
                let (mut c, mut a) = ([0f32; 3], 0f32);
                for (ox, oy, wt) in [(0, 0, (1.0 - du) * (1.0 - dv)), (1, 0, du * (1.0 - dv)), (0, 1, (1.0 - du) * dv), (1, 1, du * dv)] {
                    let q = self.get(u0 + ox, v0 + oy);
                    let qa = (q >> 24) as f32 * wt;
                    a += qa;
                    for (ch, sh) in c.iter_mut().zip([16, 8, 0]) {
                        *ch += qa * (q >> sh & 0xff) as f32;
                    }
                }
                if a > 0.0 {
                    let ch = |i: usize| ((c[i] / a).round() as u32).min(255);
                    out.px[(y * w + x) as usize] = (a.round() as u32).min(255) << 24 | ch(0) << 16 | ch(1) << 8 | ch(2);
                }
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

/// At `n` = 2 (docs/dev/native-paint.md): every screen pixel under the design rectangle `x0..x1`, `y0..y1` (cut to the
/// canvas) becomes the mean of `f(x, y, bg)` at its four design pixels, `bg` being what the screen pixel shows now;
/// design pixels of the cell outside the rectangle stay `bg`. `f` returns the design pixel as the old way painted it
/// over `bg` (`bg` itself where nothing is painted). The 2:1 filter's picture bit for bit wherever the part painted over
/// was even within each screen pixel; one pass over the screen pixels, no design-size copy.
#[inline]
pub fn cells_over(c: &mut Canvas, x0: i32, x1: i32, y0: i32, y1: i32, mut f: impl FnMut(i32, i32, u32) -> u32) {
    let (x0, x1, y0, y1) = (x0.max(0), x1.min(c.w), y0.max(0), y1.min(c.h));
    if x0 >= x1 || y0 >= y1 {
        return;
    }
    let stride = c.stride();
    let rows = (c.pixels().len() / stride.max(1)) as i32;
    let (_, sr0, _, fy) = c.cell(0, y0);
    let (_, sr1, _, _) = c.cell(0, y1 - 1);
    let (sc0, sc1) = ((x0 / 2) as usize, (((x1 - 1) / 2 + 1) as usize).min(stride));
    let px = c.pixels_mut();
    for sr in sr0.max(0)..(sr1 + 1).min(rows) {
        // The cell's two design rows; the first or the last screen row may hold only one of the rectangle's.
        let top = y0 - fy as i32 + 2 * (sr - sr0);
        let (in0, in1) = (top >= y0, top + 1 < y1);
        let line = &mut px[sr as usize * stride..][..stride];
        for sc in sc0..sc1 {
            let x = sc as i32 * 2;
            let (l, r) = (x >= x0, x + 1 < x1);
            let bg = line[sc];
            let a = if in0 && l { f(x, top, bg) } else { bg };
            let b = if in0 && r { f(x + 1, top, bg) } else { bg };
            let cc = if in1 && l { f(x, top + 1, bg) } else { bg };
            let d = if in1 && r { f(x + 1, top + 1, bg) } else { bg };
            line[sc] = avg4(a, b, cc, d);
        }
    }
}

/// `cells_over` for an `f` that does not depend on the column (a band darkened or faded row by row): `f` is worked out
/// once per design row of a screen pixel instead of once per design pixel. Bit for bit what `cells_over` makes of it.
pub fn rows_over(c: &mut Canvas, x0: i32, x1: i32, y0: i32, y1: i32, mut f: impl FnMut(i32, u32) -> u32) {
    let (x0, x1, y0, y1) = (x0.max(0), x1.min(c.w), y0.max(0), y1.min(c.h));
    if x0 >= x1 || y0 >= y1 {
        return;
    }
    let stride = c.stride();
    let rows = (c.pixels().len() / stride.max(1)) as i32;
    let (_, sr0, _, fy) = c.cell(0, y0);
    let (_, sr1, _, _) = c.cell(0, y1 - 1);
    let (sc0, sc1) = ((x0 / 2) as usize, (((x1 - 1) / 2 + 1) as usize).min(stride));
    let px = c.pixels_mut();
    for sr in sr0.max(0)..(sr1 + 1).min(rows) {
        let top = y0 - fy as i32 + 2 * (sr - sr0);
        let (in0, in1) = (top >= y0, top + 1 < y1);
        let line = &mut px[sr as usize * stride..][..stride];
        for sc in sc0..sc1 {
            let x = sc as i32 * 2;
            let (l, r) = (x >= x0, x + 1 < x1);
            let bg = line[sc];
            let t = if in0 { f(top, bg) } else { bg };
            let u = if in1 { f(top + 1, bg) } else { bg };
            line[sc] = avg4(if l { t } else { bg }, if r { t } else { bg }, if l { u } else { bg }, if r { u } else { bg });
        }
    }
}

/// `px` (solid design pixels, `w` wide, whole rows) copied 1:1 to (x, y), cut to the canvas: `Canvas::copy_design`
/// without see-through, but at `n` = 2 straight from two rows at any place, odd ones included (the Shelf look's strip
/// sits wherever the row's tallest case puts it). Bit for bit the old picture; where the block covers only part of a
/// screen pixel, the rest is what the screen shows.
pub fn copy_solid(c: &mut Canvas, x: i32, y: i32, w: usize, px: &[u32]) {
    if c.n() == 1 || w == 0 {
        return c.copy_design(x, y, w, px, None);
    }
    let h = (px.len() / w) as i32;
    let wi = w as i32;
    let (x0, x1, y0, y1) = (x.max(0), (x + wi).min(c.w), y.max(0), (y + h).min(c.h));
    if x0 >= x1 || y0 >= y1 {
        return;
    }
    let stride = c.stride();
    let rows = (c.pixels().len() / stride.max(1)) as i32;
    let (_, sr0, _, gy) = c.cell(x0, y0);
    let (_, sr1, _, _) = c.cell(x0, y1 - 1);
    let (sc0, sc1) = (x0 / 2, ((x1 - 1) / 2 + 1).min(stride as i32));
    let line_of = |yy: i32| &px[(yy - y) as usize * w..][..w];
    let out = c.pixels_mut();
    for sr in sr0.max(0)..(sr1 + 1).min(rows) {
        let top = y0 - gy as i32 + 2 * (sr - sr0);
        let (a, b) = ((top >= y0).then(|| line_of(top)), (top + 1 < y1).then(|| line_of(top + 1)));
        let line = &mut out[sr as usize * stride..][..stride];
        // The screen columns whose two design columns are both inside: two rows, no tests.
        let (f0, f1) = ((x0 + 1) / 2, (x1 / 2).min(sc1));
        if let (Some(a), Some(b)) = (a, b) {
            let k0 = (2 * f0 - x) as usize;
            for (i, d) in line[f0 as usize..f1.max(f0) as usize].iter_mut().enumerate() {
                let k = k0 + 2 * i;
                *d = avg4(a[k], a[k + 1], b[k], b[k + 1]);
            }
        }
        for sc in sc0..sc1 {
            if let (Some(_), Some(_)) = (a, b) {
                if sc >= f0 && sc < f1 {
                    continue;
                }
            }
            let xx = 2 * sc;
            let bg = line[sc as usize];
            let at = |l: Option<&[u32]>, col: i32| match l {
                Some(l) if col >= x0 && col < x1 => l[(col - x) as usize],
                _ => bg,
            };
            line[sc as usize] = avg4(at(a, xx), at(a, xx + 1), at(b, xx), at(b, xx + 1));
        }
    }
}

/// A tile of solid design pixels (`w` wide, `px.len() / w` rows) at half size for a place `fx`, `fy` (0 or 1) into a
/// screen pixel: cell (i, j) is the mean of design columns `2i - fx`, `2i - fx + 1` and rows `2j - fy`, `2j - fy + 1`
/// (`(fx + w).div_ceil(2)` x `(fy + h).div_ceil(2)` cells). Cells reaching past the tile's edge hold 0 and are never read
/// (`paint_strip`). Made once per tile and place, kept beside the tile (the Shelf look's row).
pub fn half_tile(px: &[u32], w: usize, fx: u8, fy: u8) -> Vec<u32> {
    let h = px.len() / w.max(1);
    let (fx, fy) = (fx as usize & 1, fy as usize & 1);
    let (hw, hh) = ((fx + w).div_ceil(2), (fy + h).div_ceil(2));
    let mut out = vec![0u32; hw * hh];
    for j in 0..hh {
        let Some(y) = (2 * j).checked_sub(fy).filter(|y| y + 1 < h) else { continue };
        let (a, b) = (&px[y * w..][..w], &px[(y + 1) * w..][..w]);
        for i in 0..hw {
            let Some(x) = (2 * i).checked_sub(fx).filter(|x| x + 1 < w) else { continue };
            out[j * hw + i] = avg4(a[x], a[x + 1], b[x], b[x + 1]);
        }
    }
    out
}

/// At `n` = 2: a strip of solid tiles (`tiles[k]` is tile `first + k`, each `tile_w` design pixels wide and `room` rows
/// tall, `halves[k]` its `half_tile` for this place) shown from the strip's column `scroll` on, `view` design pixels wide
/// at design (`left`, `top`): what `copy_solid` makes of the strip put together in design pixels, bit for bit, without
/// putting it together. Cells inside one tile are copied from its half, row by row; a cell across two tiles or past the
/// strip's edge takes the mean of its four (the screen where it has none). `tile_w` is even.
#[allow(clippy::too_many_arguments)]
pub fn paint_strip(c: &mut Canvas, (left, top): (i32, i32), (view, room): (i32, i32), scroll: i32, first: i32, tile_w: i32, tiles: &[&[u32]], halves: &[&[u32]]) {
    let (x0, x1, y0, y1) = (left.max(0), (left + view).min(c.w), top.max(0), (top + room).min(c.h));
    if x0 >= x1 || y0 >= y1 || tiles.is_empty() {
        return;
    }
    let fx = (left - scroll).rem_euclid(2);
    let stride = c.stride();
    let rows = (c.pixels().len() / stride.max(1)) as i32;
    let (_, sr0, _, gy) = c.cell(x0, y0);
    let (_, sr1, _, _) = c.cell(x0, y1 - 1);
    let fy = (gy as i32 + y0 - top).rem_euclid(2) as usize;
    let hw = ((fx + tile_w + 1) / 2) as usize;
    let end = first + tiles.len() as i32;
    // The design pixel of the strip at (x, y), when the strip has one there.
    let at = |x: i32, y: i32| -> Option<u32> {
        if x < x0 || x >= x1 || y < y0 || y >= y1 {
            return None;
        }
        let col = x - left + scroll;
        let t = col.div_euclid(tile_w);
        if t < first || t >= end {
            return None;
        }
        Some(tiles[(t - first) as usize][((y - top) * tile_w + col - t * tile_w) as usize])
    };
    // Screen columns of cells inside one tile: (from, to, tile, the tile's first cell's screen column).
    let (sc0, sc1) = (x0 / 2, ((x1 - 1) / 2 + 1).min(stride as i32));
    let mut runs: Vec<(i32, i32, usize, i32)> = Vec::new();
    for k in 0..tiles.len() as i32 {
        let xa = left - scroll + (first + k) * tile_w;
        let (lo, hi) = (xa.max(x0), (xa + tile_w).min(x1));
        let (a, b) = ((lo + 1).div_euclid(2), hi.div_euclid(2));
        if a < b {
            runs.push((a, b, k as usize, (xa - fx).div_euclid(2)));
        }
    }
    let out = c.pixels_mut();
    for sr in sr0.max(0)..(sr1 + 1).min(rows) {
        let yy = y0 - gy as i32 + 2 * (sr - sr0);
        let line = &mut out[sr as usize * stride..][..stride];
        let mut cell = |line: &mut [u32], sc: i32| {
            let bg = line[sc as usize];
            let x = 2 * sc;
            let q = |x: i32, y: i32| at(x, y).unwrap_or(bg);
            line[sc as usize] = avg4(q(x, yy), q(x + 1, yy), q(x, yy + 1), q(x + 1, yy + 1));
        };
        if yy < y0 || yy + 1 >= y1 {
            for sc in sc0..sc1 {
                cell(line, sc);
            }
            continue;
        }
        let j = ((yy - top) as usize + fy) / 2;
        let mut next = sc0;
        for &(a, b, k, base) in &runs {
            for sc in next..a {
                cell(line, sc);
            }
            let h = &halves[k][j * hw..][..hw];
            line[a as usize..b as usize].copy_from_slice(&h[(a - base) as usize..(b - base) as usize]);
            next = b;
        }
        for sc in next..sc1 {
            cell(line, sc);
        }
    }
}

/// One source pixel over `bg` as `put` paints it, the result returned (`cells_over`).
#[inline]
fn over(bg: u32, s: u32, shade: u32) -> u32 {
    let a = s >> 24;
    if a == 255 {
        shaded(s, shade)
    } else if a != 0 {
        mix(bg, shaded(s, shade), a)
    } else {
        bg
    }
}

/// `p` with its colours multiplied by `shade`/256 as [`blit`] paints it, its alpha kept: a case taken off a page
/// (Android's start and way back) looks as it stood there.
pub fn lit(p: &Pic, shade: u32) -> Pic {
    if shade >= 256 {
        return p.clone();
    }
    Pic::of(p.w, p.h, p.px.iter().map(|&s| (s & 0xff00_0000) | shaded(s, shade)).collect())
}

/// Copies `p` onto the canvas at (x, y), its colours multiplied by `shade`/256, only rows from `clip.0` to
/// `clip.1` (exclusive).
pub fn blit(c: &mut Canvas, p: &Pic, x: i32, y: i32, shade: u32, clip: (i32, i32)) {
    blit_cols(c, p, x, y, shade, clip, (0, p.w));
}

/// `blit` of the picture's columns `cols.0..cols.1` only (a spine cut off at the cabinet's inner walls): nothing is made.
#[allow(clippy::too_many_arguments)]
pub fn blit_cols(c: &mut Canvas, p: &Pic, x: i32, y: i32, shade: u32, clip: (i32, i32), cols: (i32, i32)) {
    let (y0, y1) = (y.max(clip.0).max(0), (y + p.h).min(clip.1).min(c.h));
    let (x0, x1) = ((x + cols.0.max(0)).max(0), (x + cols.1.min(p.w)).min(c.w));
    if x0 >= x1 || y0 >= y1 {
        return;
    }
    if c.n() != 1 {
        return blit_half(c, p, (x, y), shade, (x0, x1, y0, y1));
    }
    let c_w = c.w;
    for row in y0..y1 {
        let src = &p.px[((row - y) * p.w + (x0 - x)) as usize..((row - y) * p.w + (x1 - x)) as usize];
        let dst = &mut c.pixels_mut()[(row * c_w + x0) as usize..(row * c_w + x1) as usize];
        blit_row(dst, src, shade);
    }
}

/// One row of `blit`: eight pixels at a time where all eight are solid (the wood of the walls and boards, a case's
/// printed face), with no test per pixel, which the compiler does four at a time; the rest pixel by pixel. The same
/// pixels either way. The Cabinet's walls, boards and spines are most of its frame: on the Brick's A53 the test per
/// pixel kept the walls at 2-2.5 ms of a moving frame (docs/TRIMUI-BRICK.md).
fn blit_row(dst: &mut [u32], src: &[u32], shade: u32) {
    let mut d8 = dst.chunks_exact_mut(8);
    let mut s8 = src.chunks_exact(8);
    for (d, s) in (&mut d8).zip(&mut s8) {
        if s.iter().fold(0xff, |m, &p| m & (p >> 24)) == 0xff {
            for (d, &s) in d.iter_mut().zip(s) {
                *d = shaded(s, shade);
            }
        } else {
            for (d, &s) in d.iter_mut().zip(s) {
                put(d, s, shade);
            }
        }
    }
    for (d, &s) in d8.into_remainder().iter_mut().zip(s8.remainder()) {
        put(d, s, shade);
    }
}

/// `blit` at `n` = 2 of the design rectangle `x0..x1`, `y0..y1` (inside the picture and the canvas): a screen pixel
/// the rectangle covers whole takes the picture's half copy (`Pic::half`), one value, shaded after the mean (within a
/// step of the old picture where `shade` is below 256, the same where it is 256); the rest, and pixels partly
/// see-through, the mean of the four as the old picture made it.
fn blit_half(c: &mut Canvas, p: &Pic, (x, y): (i32, i32), shade: u32, (x0, x1, y0, y1): (i32, i32, i32, i32)) {
    let (sx, sy, fx, fy) = c.cell(x, y);
    let (half, hw) = p.half(fx, fy);
    let stride = c.stride();
    let rows = (c.pixels().len() / stride.max(1)) as i32;
    let (_, sr0, _, gy) = c.cell(x0, y0);
    let (_, sr1, _, _) = c.cell(x0, y1 - 1);
    let (sc0, sc1) = (x0 / 2, ((x1 - 1) / 2 + 1).min(stride as i32));
    let pw = p.w;
    let sample = |xx: i32, yy: i32, bg: u32| over(bg, p.px[((yy - y) * pw + xx - x) as usize], shade);
    let px = c.pixels_mut();
    // The columns whose two design columns both lie inside x0..x1: in a row inside y0..y1 too they need no edge checks.
    let (ci0, ci1) = ((x0 + 1).div_euclid(2).clamp(sc0, sc1), x1.div_euclid(2).clamp(sc0, sc1));
    for sr in sr0.max(0)..(sr1 + 1).min(rows) {
        let top = y0 - gy as i32 + 2 * (sr - sr0);
        let (in0, in1) = (top >= y0, top + 1 < y1);
        let hrow = &half[(sr - sy) as usize * hw..][..hw];
        let line = &mut px[sr as usize * stride..][..stride];
        // One screen pixel the slow way: the mean of its four design pixels over what is there (outside: what is there).
        let general = |d: &mut u32, sc: i32| {
            let xx = sc * 2;
            let (l, r) = (xx >= x0, xx + 1 < x1);
            if in0 && in1 && l && r {
                let v = hrow[(sc - sx) as usize];
                if v >> 24 == 255 {
                    *d = shaded(v, shade);
                    return;
                }
                if v != HALF_MIXED {
                    put(d, v, shade);
                    return;
                }
            }
            let bg = *d;
            let a = if in0 && l { sample(xx, top, bg) } else { bg };
            let b = if in0 && r { sample(xx + 1, top, bg) } else { bg };
            let cc = if in1 && l { sample(xx, top + 1, bg) } else { bg };
            let e = if in1 && r { sample(xx + 1, top + 1, bg) } else { bg };
            *d = avg4(a, b, cc, e);
        };
        if in0 && in1 && ci0 < ci1 {
            // A whole row: solid cells straight from the half copy, see-through ones skipped, mixed ones the slow way.
            for sc in sc0..ci0 {
                general(&mut line[sc as usize], sc);
            }
            let src = &hrow[(ci0 - sx) as usize..(ci1 - sx) as usize];
            for (i, &v) in src.iter().enumerate() {
                let d = &mut line[ci0 as usize + i];
                if v >> 24 == 255 {
                    *d = shaded(v, shade);
                } else if v != HALF_MIXED {
                    put(d, v, shade);
                } else {
                    general(d, ci0 + i as i32);
                }
            }
            for sc in ci1..sc1 {
                general(&mut line[sc as usize], sc);
            }
        } else {
            for sc in sc0..sc1 {
                general(&mut line[sc as usize], sc);
            }
        }
    }
}

/// One source pixel onto the canvas (straight alpha), as `blit` does.
#[inline]
fn put(d: &mut u32, s: u32, shade: u32) {
    let a = s >> 24;
    if a == 255 {
        *d = shaded(s, shade);
    } else if a != 0 {
        *d = mix(*d, shaded(s, shade), a);
    }
}

/// A picture (a sticker) drawn with its middle at (cx, cy), `scale` times its size and turned by `angle` radians (the
/// direction of `case_stickers::rotated`'s positive degrees), nearest pixel so it stays pixel art. `alpha` 0 to 255 over
/// what is there; `flat` paints every solid pixel in that one colour instead (a shadow). Only the turned rectangle is
/// visited, rows inside `clip` (top, bottom). At scale 1 and angle 0 on a whole-pixel middle it is the picture exactly.
#[allow(clippy::too_many_arguments)]
pub fn blit_pose(c: &mut Canvas, p: &Pic, cx: f32, cy: f32, scale: f32, angle: f32, alpha: u32, flat: Option<u32>, clip: (i32, i32)) {
    if alpha == 0 || scale <= 0.0 {
        return;
    }
    let alpha = alpha.min(255) as u8;
    let (sin, cos) = angle.sin_cos();
    let (pcx, pcy) = (p.w as f32 / 2.0, p.h as f32 / 2.0);
    let (hw, hh) = ((p.w as f32 * cos.abs() + p.h as f32 * sin.abs()) * scale / 2.0, (p.w as f32 * sin.abs() + p.h as f32 * cos.abs()) * scale / 2.0);
    let (x0, x1) = (((cx - hw).floor() as i32).max(0), ((cx + hw).ceil() as i32).min(c.w));
    let (y0, y1) = (((cy - hh).floor() as i32).max(clip.0).max(0), ((cy + hh).ceil() as i32).min(clip.1).min(c.h));
    let inv = 1.0 / scale;
    if c.n() != 1 {
        // Painted natively: each design pixel as below, the four of a screen pixel averaged (`cells_over`).
        return cells_over(c, x0, x1, y0, y1, |x, y, bg| {
            let (dx, dy) = (x as f32 + 0.5 - cx, y as f32 + 0.5 - cy);
            let sx = ((dx * cos + dy * sin) * inv + pcx).floor() as i32;
            let sy = ((-dx * sin + dy * cos) * inv + pcy).floor() as i32;
            if sx < 0 || sy < 0 || sx >= p.w || sy >= p.h {
                return bg;
            }
            let v = p.px[(sy * p.w + sx) as usize];
            if v >> 24 != 0 {
                mix(bg, flat.unwrap_or(v & 0xff_ffff), alpha as u32)
            } else {
                bg
            }
        });
    }
    for y in y0..y1 {
        let dy = y as f32 + 0.5 - cy;
        for x in x0..x1 {
            let dx = x as f32 + 0.5 - cx;
            let sx = ((dx * cos + dy * sin) * inv + pcx).floor() as i32;
            let sy = ((-dx * sin + dy * cos) * inv + pcy).floor() as i32;
            if sx < 0 || sy < 0 || sx >= p.w || sy >= p.h {
                continue;
            }
            let v = p.px[(sy * p.w + sx) as usize];
            if v >> 24 != 0 {
                c.blend(x, y, flat.unwrap_or(v & 0xff_ffff), alpha);
            }
        }
    }
}

/// `blit` with the rows pushed sideways, the top row by `top_dx` and the bottom one not at all: the picture rocks on
/// its bottom edge (`playful::wiggle`) or leans. A copy with offsets, nothing is made.
pub fn blit_sheared(c: &mut Canvas, p: &Pic, x: i32, y: i32, top_dx: f32, shade: u32, clip: (i32, i32)) {
    if top_dx.abs() < 0.5 {
        return blit(c, p, x, y, shade, clip);
    }
    let (y0, y1) = (y.max(clip.0).max(0), (y + p.h).min(clip.1).min(c.h));
    let last = (p.h - 1).max(1) as f32;
    if c.n() != 1 {
        if y0 >= y1 {
            return;
        }
        // Painted natively: each row's push worked out once, the four design pixels of a screen pixel averaged.
        let dxs: Vec<i32> = (y0..y1).map(|row| (top_dx * (1.0 - (row - y) as f32 / last)).round() as i32).collect();
        let reach = top_dx.abs().ceil() as i32 + 1;
        return cells_over(c, x - reach, x + p.w + reach, y0, y1, |cx, cy, bg| {
            let sx = cx - x - dxs[(cy - y0) as usize];
            if sx < 0 || sx >= p.w {
                return bg;
            }
            over(bg, p.px[((cy - y) * p.w + sx) as usize], shade)
        });
    }
    let c_w = c.w;
    for row in y0..y1 {
        let dx = (top_dx * (1.0 - (row - y) as f32 / last)).round() as i32;
        let (x0, x1) = ((x + dx).max(0), (x + dx + p.w).min(c.w));
        if x0 >= x1 {
            continue;
        }
        let src = &p.px[((row - y) * p.w + (x0 - x - dx)) as usize..][..(x1 - x0) as usize];
        let dst = &mut c.pixels_mut()[(row * c_w + x0) as usize..(row * c_w + x1) as usize];
        for (d, &s) in dst.iter_mut().zip(src) {
            put(d, s, shade);
        }
    }
}

/// `p` drawn `w` x `h` at (x, y), nearest pixel, straight onto the canvas (a squash or a stretch of a few per cent:
/// nothing is made, unlike `Pic::scaled`).
#[allow(clippy::too_many_arguments)]
pub fn blit_scaled(c: &mut Canvas, p: &Pic, x: i32, y: i32, w: i32, h: i32, shade: u32, clip: (i32, i32)) {
    if w <= 0 || h <= 0 {
        return;
    }
    if (w, h) == (p.w, p.h) {
        return blit(c, p, x, y, shade, clip);
    }
    let (y0, y1) = (y.max(clip.0).max(0), (y + h).min(clip.1).min(c.h));
    let (x0, x1) = (x.max(0), (x + w).min(c.w));
    if c.n() != 1 {
        if x0 >= x1 || y0 >= y1 {
            return;
        }
        // Painted natively: the source column and row of each design pixel worked out once.
        let sxs: Vec<i32> = (x0..x1).map(|col| (((col - x) as i64 * p.w as i64 / w as i64) as i32).min(p.w - 1)).collect();
        let sys: Vec<i32> = (y0..y1).map(|row| (((row - y) as i64 * p.h as i64 / h as i64) as i32).min(p.h - 1)).collect();
        return cells_over(c, x0, x1, y0, y1, |cx, cy, bg| over(bg, p.px[(sys[(cy - y0) as usize] * p.w + sxs[(cx - x0) as usize]) as usize], shade));
    }
    let c_w = c.w;
    for row in y0..y1 {
        let sy = (((row - y) as i64 * p.h as i64 / h as i64) as i32).min(p.h - 1);
        let src = &p.px[(sy * p.w) as usize..][..p.w as usize];
        let dst = &mut c.pixels_mut()[(row * c_w) as usize..][..c_w as usize];
        for col in x0..x1 {
            let sx = (((col - x) as i64 * p.w as i64 / w as i64) as i32).min(p.w - 1);
            put(&mut dst[col as usize], src[sx as usize], shade);
        }
    }
}

/// A round picture (a disc) turned by `angle` radians about its middle, drawn with its middle at (cx, cy): each
/// pixel inside its circle is read from where the turn brings it, between the four nearest (a printed disc's title
/// stays smooth as it turns). Only the circle is touched.
pub fn blit_spun(c: &mut Canvas, p: &Pic, cx: f32, cy: f32, angle: f32, clip: (i32, i32)) {
    // Each row walks the picture in 16.16 fixed point, one step per screen pixel: nearest pixel, a few integer
    // operations per pixel (on the H700 the bilinear version cost 29 ms a frame for the box's disc).
    let r = p.w.min(p.h) as f32 / 2.0;
    let (pcx, pcy) = (p.w as f32 / 2.0, p.h as f32 / 2.0);
    let (sin, cos) = (-angle).sin_cos();
    let fx = |v: f32| (v * 65536.0) as i64;
    let (y0, y1) = (((cy - r).floor() as i32).max(clip.0).max(0), ((cy + r).ceil() as i32).min(clip.1).min(c.h));
    let (pw, ph) = (p.w as i64, p.h as i64);
    let (du, dv) = (fx(cos), fx(sin));
    let c_w = c.w;
    // Each row's columns inside the circle and where in the picture its first one reads.
    let span = |row: i32| {
        let dy = row as f32 + 0.5 - cy;
        let half = (r * r - dy * dy).max(0.0).sqrt();
        let (x0, x1) = (((cx - half).ceil() as i32).max(0), ((cx + half).floor() as i32).min(c_w));
        let dx0 = x0 as f32 + 0.5 - cx;
        (x0, x1, fx(pcx + dx0 * cos - dy * sin), fx(pcy + dx0 * sin + dy * cos))
    };
    if c.n() != 1 {
        if y0 >= y1 {
            return;
        }
        // Painted natively: the same steps (u + k·du is the walk's k-th step exactly), four design pixels a screen pixel.
        let spans: Vec<(i32, i32, i64, i64)> = (y0..y1).map(span).collect();
        let (lo, hi) = spans.iter().fold((i32::MAX, i32::MIN), |(lo, hi), s| (lo.min(s.0), hi.max(s.1)));
        return cells_over(c, lo, hi, y0, y1, |col, row, bg| {
            let (x0, x1, u, v) = spans[(row - y0) as usize];
            if col < x0 || col >= x1 {
                return bg;
            }
            let k = (col - x0) as i64;
            let (iu, iv) = ((u + k * du) >> 16, (v + k * dv) >> 16);
            if iu >= 0 && iv >= 0 && iu < pw && iv < ph {
                over(bg, p.px[(iv * pw + iu) as usize], 256)
            } else {
                bg
            }
        });
    }
    for row in y0..y1 {
        let (x0, x1, mut u, mut v) = span(row);
        if x0 >= x1 {
            continue;
        }
        let dst = &mut c.pixels_mut()[(row * c_w + x0) as usize..(row * c_w + x1) as usize];
        for d in dst.iter_mut() {
            let (iu, iv) = (u >> 16, v >> 16);
            if iu >= 0 && iv >= 0 && iu < pw && iv < ph {
                put(d, p.px[(iv * pw + iu) as usize], 256);
            }
            u += du;
            v += dv;
        }
    }
}

/// A console's disc tray `out` (0-1) of the drive whose slot is at `slot_y`, `w` wide and centred on `cx`: a dark
/// plate coming towards you under the slot, its top face foreshortened, with the round well the disc lies in.
/// Returns the well's middle (y) and how tall its top face is drawn.
pub fn tray(c: &mut Canvas, cx: i32, slot_y: i32, w: i32, out: f32, body: u32) -> (i32, i32) {
    let depth = (w as f32 * 0.42 * out.clamp(0.0, 1.0)).round() as i32;
    if depth <= 1 {
        return (slot_y, 0);
    }
    // The top face (seen from above at a slant, lit from above) with a dark outline, and its front edge lighter still.
    let top = mix(body, 0xffffff, 46);
    let edge = (w / 28).clamp(4, 12);
    c.rect(cx - w / 2 - 2, slot_y, w + 4, depth + edge + 2, 0x0a0a0b);
    c.rect(cx - w / 2, slot_y, w, depth, top);
    c.rect(cx - w / 2, slot_y + depth, w, edge, mix(body, 0xffffff, 90));
    // The well: an ellipse a disc lies in, as foreshortened as the face, its far rim in shadow.
    let (rx, ry) = ((w as f32 * 0.42) as i32, (depth as f32 * 0.42) as i32);
    let wy = slot_y + depth / 2;
    ellipse(c, cx, wy, rx, ry.max(1), mix(top, 0x000000, 60));
    ellipse(c, cx, wy + ry / 6, rx - 3, (ry - 2).max(1), mix(top, 0x000000, 30));
    (wy, depth)
}

/// A top-loader's lid over its round opening, middle (cx, cy), radii `rx` x `ry` as the console's picture shows it
/// lying shut, `open` 0 (shut: nothing is drawn, the picture shows it) to 1 (standing up past upright), hinged at its
/// back edge. Drawn in two parts so that the disc goes between them: `lid_behind` before the disc (the dark opening,
/// and the lid while it stands up behind it), `lid_over` after it (the lid on its way down over the disc).
pub fn lid_behind(c: &mut Canvas, (cx, cy, rx, ry): (i32, i32, i32, i32), open: f32, top: u32) {
    if open <= 0.0 {
        return;
    }
    ellipse(c, cx, cy, rx + 1, ry + 1, 0x0a0a0b);
    ellipse(c, cx, cy, rx - 1, (ry - 1).max(1), 0x141417);
    ellipse(c, cx, cy + ry / 5, rx * 3 / 5, (ry * 3 / 5).max(1), 0x1e1f23);
    // Standing up behind the opening: its underside, darker.
    if let Some((y0, y1)) = lid_span(cy, rx, ry, open).filter(|(_, y1)| *y1 <= cy - ry + 2) {
        lid_face(c, cx, rx, (y0, y1), mix(top, 0x000000, 80));
    }
}

/// The lid's second part (`lid_behind`): coming down over the opening, its top side to you.
pub fn lid_over(c: &mut Canvas, (cx, cy, rx, ry): (i32, i32, i32, i32), open: f32, top: u32) {
    if open <= 0.0 {
        return;
    }
    if let Some((y0, y1)) = lid_span(cy, rx, ry, open) {
        if y1 > cy - ry + 2 {
            lid_face(c, cx, rx, (y0, y1), top);
        }
    }
}

/// The rows the lid covers at `open`: from its back edge (the hinge, `cy - ry`) to its front edge, which swings from
/// `cy + ry` (shut) up over the hinge (standing, as tall as the lid is wide, a little foreshortened).
fn lid_span(cy: i32, rx: i32, ry: i32, open: f32) -> Option<(i32, i32)> {
    let th = open.clamp(0.0, 1.0) * 1.75;
    let hinge = (cy - ry) as f32;
    let front = hinge + 2.0 * ry as f32 * th.cos() - 1.7 * rx as f32 * th.sin();
    let (a, b) = (hinge.min(front).round() as i32, hinge.max(front).round() as i32);
    (b - a >= 2).then_some((a, b))
}

fn lid_face(c: &mut Canvas, cx: i32, rx: i32, (y0, y1): (i32, i32), color: u32) {
    let (my, hy) = ((y0 + y1) / 2, ((y1 - y0) / 2).max(1));
    ellipse(c, cx, my, rx + 1, hy + 1, 0x0a0a0b);
    ellipse(c, cx, my, rx, hy, color);
    // The light catching its rim.
    ellipse(c, cx, my - hy / 3, rx * 4 / 5, (hy / 3).max(1), mix(color, 0xffffff, 40));
    ellipse(c, cx, my, rx * 3 / 4, (hy * 3 / 4).max(1), color);
}

/// A filled ellipse.
fn ellipse(c: &mut Canvas, cx: i32, cy: i32, rx: i32, ry: i32, color: u32) {
    let half = |dy: i32| (rx as f32 * (1.0 - (dy as f32 / ry as f32).powi(2)).max(0.0).sqrt()) as i32;
    if c.n() != 1 {
        // Painted natively: the same spans, each screen pixel by how much of it they cover.
        let halves: Vec<i32> = (-ry..=ry).map(half).collect();
        return cells_over(c, cx - rx, cx + rx + 1, cy - ry, cy + ry + 1, |x, y, bg| {
            let h = halves[(y - cy + ry) as usize];
            if (cx - h..=cx + h).contains(&x) { color } else { bg }
        });
    }
    let c_w = c.w;
    for dy in -ry..=ry {
        let row = cy + dy;
        if row < 0 || row >= c.h {
            continue;
        }
        let half = half(dy);
        let (x0, x1) = ((cx - half).max(0), (cx + half + 1).min(c.w));
        if x0 < x1 {
            c.pixels_mut()[(row * c_w + x0) as usize..(row * c_w + x1) as usize].fill(color);
        }
    }
}

/// Darkens every pixel of rows `y0..y1` to `keep`/256 of its brightness.
pub fn dim(c: &mut Canvas, y0: i32, y1: i32, keep: u32) {
    if keep >= 256 {
        return;
    }
    let (y0, y1) = (y0.clamp(0, c.h), y1.clamp(0, c.h));
    if c.n() != 1 {
        // Painted natively: a screen pixel holding only part of the band is darkened by that part (`rows_over`). A
        // screen row wholly inside it has its design pixels darkened alike, and their mean is that one darkened pixel:
        // the same bits in one multiply (the launch darkens the whole screen every frame, H700 14.11.).
        let w = c.w;
        if y0 >= y1 {
            return;
        }
        let (_, sr0, _, fy0) = c.cell(0, y0);
        let (_, sr1, _, fy1) = c.cell(0, y1 - 1);
        // The screen rows the band covers whole: from the first whose top design row is `y0` or later, to the last whose
        // bottom one is before `y1`.
        let (full0, full1) = (if fy0 == 0 { sr0 } else { sr0 + 1 }, if fy1 as i32 == c.n() - 1 { sr1 } else { sr1 - 1 });
        let stride = c.stride();
        let rows = (c.pixels().len() / stride.max(1)) as i32;
        let cols = (w.max(0) as usize).div_ceil(2).min(stride);
        if (full0..=full1).is_empty() || cols * 2 != w as usize {
            return rows_over(c, 0, w, y0, y1, |_, bg| shaded(bg, keep));
        }
        if fy0 != 0 {
            rows_over(c, 0, w, y0, y0 + 1, |_, bg| shaded(bg, keep));
        }
        if fy1 as i32 != c.n() - 1 {
            rows_over(c, 0, w, y1 - 1, y1, |_, bg| shaded(bg, keep));
        }
        let (r0, r1) = (full0.max(0), (full1 + 1).min(rows));
        let px = c.pixels_mut();
        for r in r0..r1 {
            for p in &mut px[r as usize * stride..][..cols] {
                *p = shaded(*p, keep);
            }
        }
        return;
    }
    let c_w = c.w;
    for p in &mut c.pixels_mut()[(y0 * c_w) as usize..(y1 * c_w) as usize] {
        *p = shaded(*p, keep);
    }
}

/// A light at (`cx`, `cy`): an ellipse `rx` by `ry` brightening towards `color`, strongest in the middle, `strength`
/// 0-1. In four steps, not a smooth gradient, so it reads as pixel art beside the console (the launch animation's
/// glow as the cartridge or disc goes in).
pub fn glow(c: &mut Canvas, cx: i32, cy: i32, rx: i32, ry: i32, color: u32, strength: f32) {
    if strength <= 0.0 || rx <= 0 || ry <= 0 {
        return;
    }
    let (x0, x1) = ((cx - rx).max(0), (cx + rx).min(c.w));
    let (y0, y1) = ((cy - ry).max(0), (cy + ry).min(c.h));
    let max = (strength.min(1.0) * 200.0) as u32;
    let lit = |x: i32, y: i32, bg: u32| {
        let (dx, dy) = ((x - cx) as f32 / rx as f32, (y - cy) as f32 / ry as f32);
        let d = dx * dx + dy * dy;
        if d >= 1.0 {
            return bg;
        }
        // 1, 3/4, 1/2, 1/4 of the light in rings from the middle out.
        let step = ((1.0 - d) * 4.0).ceil() / 4.0;
        mix(bg, color, (max as f32 * step * step) as u32)
    };
    if c.n() != 1 {
        // Painted natively: the four design pixels of each screen pixel lit as below and averaged.
        return cells_over(c, x0, x1, y0, y1, lit);
    }
    let c_w = c.w;
    for y in y0..y1 {
        for x in x0..x1 {
            let i = (y * c_w + x) as usize;
            c.pixels_mut()[i] = lit(x, y, c.pixels()[i]);
        }
    }
}

/// The first row of a band of `n` rows from `y0` towards `y1` (`fade_band`).
#[inline]
fn band_top(y0: i32, y1: i32, n: i32) -> i32 {
    if y1 > y0 { y0 } else { y0 - n + 1 }
}

/// How much of row `row` a band from `y0` to `y1` (`n` rows) keeps, 0-256, as `fade_band` and `fade_band_to` fade it.
#[inline]
fn band_keep(y0: i32, y1: i32, n: i32, row: i32) -> u32 {
    let k = if y1 > y0 { row - y0 } else { y0 - row };
    let keep = 256 - (k * 256 / n) as u32;
    keep * keep / 256
}

/// `fade_band` towards `page` instead of black (the light page); black is `fade_band` itself.
pub fn fade_band_to(c: &mut Canvas, y0: i32, y1: i32, page: u32) {
    if page == 0 {
        return fade_band(c, y0, y1);
    }
    let n = (y1 - y0).abs().max(1);
    if c.n() != 1 {
        // Painted natively: each design row's share as below, the two of a screen row averaged.
        let w = c.w;
        return rows_over(c, 0, w, band_top(y0, y1, n), band_top(y0, y1, n) + n, |row, bg| mix(bg, page, 255 - band_keep(y0, y1, n, row).min(255)));
    }
    let c_w = c.w;
    for k in 0..n {
        let row = if y1 > y0 { y0 + k } else { y0 - k };
        if row < 0 || row >= c.h {
            continue;
        }
        let keep = 256 - (k * 256 / n) as u32;
        let keep = keep * keep / 256;
        for p in &mut c.pixels_mut()[(row * c_w) as usize..((row + 1) * c_w) as usize] {
            *p = mix(*p, page, 255 - keep.min(255));
        }
    }
}

/// Black rising over rows `y0..y1`: none at `y0`, all at `y1` (or the other way round when `y0 > y1`).
pub fn fade_band(c: &mut Canvas, y0: i32, y1: i32) {
    let n = (y1 - y0).abs().max(1);
    if c.n() != 1 {
        // Painted natively: each design row's share as below, the two of a screen row averaged.
        let w = c.w;
        return rows_over(c, 0, w, band_top(y0, y1, n), band_top(y0, y1, n) + n, |row, bg| shaded(bg, band_keep(y0, y1, n, row)));
    }
    let c_w = c.w;
    for k in 0..n {
        let row = if y1 > y0 { y0 + k } else { y0 - k };
        if row < 0 || row >= c.h {
            continue;
        }
        let keep = 256 - (k * 256 / n) as u32;
        let keep = keep * keep / 256;
        for p in &mut c.pixels_mut()[(row * c_w) as usize..((row + 1) * c_w) as usize] {
            *p = shaded(*p, keep);
        }
    }
}

/// A soft shadow: the rectangle (x, y, w, h) darkened by `strength` (0-1), fading out over `blur` pixels.
/// The fall-off is the product of one across and one down, so a frame pays a multiply per pixel.
pub fn soft_shadow(c: &mut Canvas, x: i32, y: i32, w: i32, h: i32, blur: i32, strength: f32, clip: (i32, i32)) {
    let c_w = c.w;
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
    // Wholly beside the canvas (a case's shadow past a tile of the Shelf look's row): nothing to darken.
    if x0 >= x1 {
        return;
    }
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
    let fy_at = |row: i32| {
        strength
            * fall(if row < y {
                y - row
            } else if row >= y + h {
                row - (y + h) + 1
            } else {
                0
            })
    };
    let (r0, r1) = ((y - blur).max(clip.0).max(0), (y + h + blur).min(clip.1).min(c.h));
    if c.n() != 1 {
        // Painted natively: the fall-off of each design pixel as below, the four of a screen pixel averaged.
        if r0 >= r1 {
            return;
        }
        // The fall-off is a product of one across and one down, so its mean over a screen pixel's four is the product
        // of the two means: one multiply per screen pixel (within a step of the old picture's rounding).
        let fx = |col: i32| if (x0..x1).contains(&col) { cols[(col - x0) as usize] } else { 0.0 };
        let fy = |row: i32| if (r0..r1).contains(&row) { fy_at(row) } else { 0.0 };
        let (sc0, sc1) = (x0 / 2, ((x1 - 1) / 2 + 1).min(c.stride() as i32));
        let across: Vec<f32> = (sc0..sc1).map(|sc| (fx(2 * sc) + fx(2 * sc + 1)) / 2.0).collect();
        let (_, sr0, _, gy) = c.cell(0, r0);
        let (_, sr1, _, _) = c.cell(0, r1 - 1);
        let stride = c.stride();
        let rows = (c.pixels().len() / stride.max(1)) as i32;
        let px = c.pixels_mut();
        for sr in sr0.max(0)..(sr1 + 1).min(rows) {
            let top = r0 - gy as i32 + 2 * (sr - sr0);
            let down = (fy(top) + fy(top + 1)) / 2.0;
            if down <= 0.0 {
                continue;
            }
            let line = &mut px[sr as usize * stride + sc0 as usize..][..across.len()];
            for (p, k) in line.iter_mut().zip(&across) {
                let a = down * k;
                if a > 0.004 {
                    *p = shaded(*p, 256 - (a * 256.0) as u32);
                }
            }
        }
        return;
    }
    for row in r0..r1 {
        let fy = fy_at(row);
        if fy <= 0.0 {
            continue;
        }
        let line = &mut c.pixels_mut()[(row * c_w + x0) as usize..(row * c_w + x1) as usize];
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
/// grain and shading, so it reads as the same cabinet in a lighter wood). Stained towards the colours in use
/// (Settings → Look → Theme → Colours, `palette::Scheme::wood`), each part as much lighter or darker than the board's
/// top as it was, so the shading stays.
fn wood(walnut: u32) -> u32 {
    let oak = match walnut {
        BACK => 0xb09372,
        BOARD_TOP => 0xe2cca8,
        BOARD_EDGE => 0xc9a87f,
        _ => 0xc2a07a,
    };
    let p = crate::palette::pal();
    let plain = p.pick(walnut, oak);
    let s = crate::palette::scheme();
    if s.stain == 0 {
        return plain;
    }
    let top = p.pick(BOARD_TOP, 0xe2cca8);
    let k = (crate::palette::luminance(plain) + 0.05) / (crate::palette::luminance(top) + 0.05);
    let stain = p.pick(s.wood[0], s.wood[1]);
    mix(plain, scale_rgb(stain, k.sqrt() as f32), s.stain as u32)
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
    /// A game without any box art: its case, cartridge and disc are home-made (masking tape and marker,
    /// `tape_label`), `color` the plain case's.
    pub tape: bool,
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

/// The title along a spine in `face`, read top to bottom: `len` pixels from `y0`, centred across `x0 .. x0 + thick`.
#[allow(clippy::too_many_arguments)]
pub fn spine_title(text: &mut Text, p: &mut Pic, face: Face, title: &str, px: u32, x0: i32, thick: i32, y0: i32, len: i32, ink: u32) {
    p.changed();
    // As thick as the case: the design's size whatever Settings → Text size says (`type_scale`).
    let _fixed = crate::type_scale::fixed();
    let title = text.fit(face, px, title, len);
    let tw = text.width(face, px, &title).max(1);
    let th = (px as i32 * 3 / 2).max(1);
    let mut buf = vec![0u32; (tw + 4) as usize * th as usize];
    {
        let mut c = Canvas::new(&mut buf, (tw + 4) as u32, th as u32);
        text.draw(&mut c, face, px, 2, px as i32, 0xffffff, &title);
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

/// The whole cover in a label `w` x `h`, undistorted: scaled down (averaging, so it stays smooth) until it fits, in
/// the middle, the rest of the label in the colour of the cover's edge a little darker, as a printed label's margin.
/// Box art rarely has its label's shape (a Game Boy Advance box is near square, its cartridge's label wide); filling
/// the label cut off the box's sides and top (Alex's photo, 2026-10-28).
pub fn cover_fit(img: &Image, w: i32, h: i32) -> Image {
    let (w, h) = (w.max(1) as u32, h.max(1) as u32);
    if img.w == 0 || img.h == 0 {
        return Image { w, h, px: vec![0; (w * h) as usize] };
    }
    // The cover's size in the label: its height or its width the label's.
    let (fw, fh) = if img.w as u64 * h as u64 > img.h as u64 * w as u64 {
        (w, ((img.h as u64 * w as u64 / img.w as u64) as u32).clamp(1, h))
    } else {
        (((img.w as u64 * h as u64 / img.h as u64) as u32).clamp(1, w), h)
    };
    let scaled = cover_fill(img, fw as i32, fh as i32);
    // The margin: the average of the cover's outermost pixels.
    let (mut r, mut g, mut b, mut n) = (0u64, 0u64, 0u64, 0u64);
    for (x, y) in (0..img.w).flat_map(|x| [(x, 0), (x, img.h - 1)]).chain((0..img.h).flat_map(|y| [(0, y), (img.w - 1, y)])) {
        let p = img.px[(y * img.w + x) as usize];
        r += (p >> 16 & 0xff) as u64;
        g += (p >> 8 & 0xff) as u64;
        b += (p & 0xff) as u64;
        n += 1;
    }
    let n = n.max(1);
    let margin = mix(((r / n) << 16 | (g / n) << 8 | b / n) as u32, 0x000000, 50);
    let mut px = vec![margin; (w * h) as usize];
    let (ox, oy) = ((w - fw) / 2, (h - fh) / 2);
    for y in 0..fh {
        let dst = ((oy + y) * w + ox) as usize;
        px[dst..dst + fw as usize].copy_from_slice(&scaled.px[(y * fw) as usize..((y + 1) * fw) as usize]);
    }
    Image { w, h, px }
}

/// A label for a game without box art: its colour, its console and its name, clearly not a real cover. Laid out as a
/// printed title card: the picture (the console, or the emblem of a box that isn't a game: Favourites' star, the
/// search's glass) and the name under it, centred, the pair a little above the label's middle (it read as an empty
/// box with both at the top, Muse's review of 2026-10-29).
pub fn plain_label(text: &mut Text, p: &mut Pic, (x, y, w, h): (i32, i32, i32, i32), look: Look, title: &str, art: Option<&Sprite>) {
    p.changed();
    // A game without any box art: plain, with the name on a strip of masking tape (`tape_label`).
    if look.tape {
        p.fill(x, y, w, h, OPAQUE | look.color);
        crate::tape_label::on_label(text, p, (x, y, w, h), title);
        return;
    }
    text.literal(|text| {
    for row in 0..h {
        let c = mix(look.color, 0x000000, 20 + (row as u32 * 90 / h.max(1) as u32));
        p.fill(x, y + row, w, 1, OPAQUE | c);
    }
    let ink = ink_on(mix(look.color, 0x000000, 60));
    // Printed on the cover: its size follows the case, not Settings → Text size.
    let _fixed = crate::type_scale::fixed();
    let plan = plain_plan(text, (w, h), title, art);
    if let Some(fitted) = &plan.fitted {
        let ax = x + (w - fitted.w as i32) / 2;
        for sy in 0..fitted.h as i32 {
            for sx in 0..fitted.w as i32 {
                let s = fitted.px[(sy * fitted.w as i32 + sx) as usize];
                if s >> 24 != 0 {
                    p.blend_changed(ax + sx, y + plan.art_top + sy, s & 0xffffff, 255);
                }
            }
        }
    }
    let px = plan.px;
    let mut base = y + plan.words_top + px as i32;
    for line in plan.lines.iter().take(3) {
        let line = text.fit(Face::Title, px, line, plan.max_w);
        let lw = text.width(Face::Title, px, &line);
        let mut buf = vec![0u32; (w * (px as i32 * 3 / 2)) as usize];
        {
            let mut c = Canvas::new(&mut buf, w as u32, (px * 3 / 2) as u32);
            text.draw(&mut c, Face::Title, px, (w - lw) / 2, px as i32, 0xffffff, &line);
        }
        for ty in 0..px as i32 * 3 / 2 {
            for tx in 0..w {
                let a = buf[(ty * w + tx) as usize] & 0xff;
                p.blend_changed(x + tx, base - px as i32 + ty, ink, a);
            }
        }
        base += plan.step;
        if base > y + h {
            break;
        }
    }
    })
}

/// How a plain label of `w` x `h` sets its picture and name: the same numbers paint it ([`plain_label`]) and say where
/// the printed words are ([`plain_zones`]).
struct PlainPlan {
    px: u32,
    lines: Vec<String>,
    step: i32,
    max_w: i32,
    /// The console, fitted, where it fits above the name.
    fitted: Option<Sprite>,
    /// Tops of the picture and of the name's first line, from the label's top.
    art_top: i32,
    words_top: i32,
}

fn plain_plan(text: &mut Text, (w, h): (i32, i32), title: &str, art: Option<&Sprite>) -> PlainPlan {
    // A game's name as it is, word for word: split over two lines, each is measured and painted as a string of its own,
    // and "Sonic Adventure" read "Sonic Aventura" in Spanish (new-player walk 2026-11-02).
    // A sixth of the label, or smaller until the name fits in two lines (never a lone word of it on a line of its
    // own), at the sizes the app paints anyway: a front made in a frame draws no letters of its own
    // (tests/shelf_moves.rs). 18 and 20 only for a long word on a small case ("Überrasch mich" at 1024x768 ended
    // "Überras…", taste pass 0.2). A name too long even then ends in "…".
    let sizes = [18u32, 20, 24, 28, 30, 37, 44, 50];
    let max_w = w - w / 6;
    let mut px = sizes.into_iter().rev().find(|&s| s >= 24 && s as i32 <= w / 6).unwrap_or(24);
    let mut lines = text.wrap(Face::Title, px, title, max_w);
    // Smaller also while a word is wider than the label on its own (a wide font: "Favorite" over "n").
    while lines.len() > 2 || !text.words_fit(Face::Title, px, title, max_w) {
        match sizes.into_iter().rev().find(|&s| s < px) {
            Some(smaller) => {
                px = smaller;
                lines = text.wrap(Face::Title, px, title, max_w);
            }
            // A word still too wide at the smallest size: the name on one line, ending in "…", not a word in pieces.
            None if !text.words_fit(Face::Title, px, title, max_w) => {
                lines = vec![title.to_string()];
                break;
            }
            None => {
                lines = text.wrap_at_most(Face::Title, px, title, max_w, 2);
                break;
            }
        }
    }
    if lines.len() == 2 {
        lines = balanced(text, px, title, max_w).unwrap_or(lines);
    }
    let step = px as i32 * 11 / 10;
    let words_h = lines.len() as i32 * step;
    // The console only where the whole name still fits under it: a short label gives its room to the name.
    let gap = h / 16;
    let fitted = art
        .filter(|_| h > 120 && h - h / 5 - words_h - gap >= h / 4)
        .map(|art| art.fitted_kept(w * 4 / 5, (h / 2).min(h - h / 5 - words_h - gap)))
        .filter(|f| f.w > 0 && f.h > 0);
    let block_h = fitted.as_ref().map_or(0, |f| f.h as i32 + gap) + words_h;
    // A little above the middle, where the eye puts the middle of a card.
    let art_top = ((h - block_h) * 2 / 5).max(h / 12);
    let words_top = art_top + fitted.as_ref().map_or(0, |f| f.h as i32 + gap);
    PlainPlan { px, lines, step, max_w, fitted, art_top, words_top }
}

/// What a plain label of `label` (x, y, w, h) prints, as rectangles on the front: the name (its lines, as wide as the
/// widest) and, where it fits, the console above it. Stickers keep off them (`case_stickers::layout_around`).
pub fn plain_zones(text: &mut Text, (x, y, w, h): (i32, i32, i32, i32), title: &str, art: Option<&Sprite>) -> Vec<(i32, i32, i32, i32)> {
    text.literal(|text| {
        let _fixed = crate::type_scale::fixed();
        let plan = plain_plan(text, (w, h), title, art);
        let mut widest = 0;
        for l in plan.lines.iter().take(3) {
            let l = text.fit(Face::Title, plan.px, l, plan.max_w);
            widest = widest.max(text.width(Face::Title, plan.px, &l));
        }
        let words = (x + (w - widest) / 2, y + plan.words_top, widest, plan.lines.len().min(3) as i32 * plan.step);
        let mut out = vec![words];
        if let Some(f) = &plan.fitted {
            out.push((x + (w - f.w as i32) / 2, y + plan.art_top, f.w as i32, f.h as i32));
        }
        out
    })
}

/// `title` on two lines of about the same width, at most `max` wide ("Gran" over "Turismo 4", not "Gran Turismo" over
/// a lone "4"): the narrowest width that still takes it in two lines. `None` when it needs more than two.
fn balanced(text: &mut Text, px: u32, title: &str, max: i32) -> Option<Vec<String>> {
    let (mut lo, mut hi) = (text.width(Face::Title, px, title) / 2, max);
    let whole = text.wrap(Face::Title, px, title, hi);
    if whole.len() > 2 {
        return None;
    }
    // Two lines of whole words: a narrower line would break a long word into pieces (`Text::wrap`).
    let words = whole.join(" ");
    while hi - lo > 4 {
        let mid = (lo + hi) / 2;
        let lines = text.wrap(Face::Title, px, title, mid);
        if lines.len() <= 2 && lines.join(" ") == words {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    Some(text.wrap(Face::Title, px, title, hi))
}


/// A soft diagonal reflection of clear plastic over the front.
pub fn sheen(p: &mut Pic, strength: u32) {
    p.changed();
    let (w, h) = (p.w as f32, p.h as f32);
    for y in 0..p.h {
        // Only the columns the band crosses on this row (d within 0.16 of 0.42).
        let dy = (y as f32 / h) * 0.6;
        let x0 = (((0.26 - dy) / 0.8 * w).floor() as i32).clamp(0, p.w);
        let x1 = (((0.58 - dy) / 0.8 * w).ceil() as i32 + 1).clamp(0, p.w);
        for x in x0..x1 {
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
    // A box standing on its short side (SNES, N64) has a spine taller than its front: the spine goes from its own height
    // to the front's as it turns (the box is laid down as it comes round), never taller than the room it turns in.
    let front_h = size.front_h as f32 * turn.scale;
    let spine_h = (h + (front_h - h) * turn.turned.clamp(0.0, 1.0)).min(((clip.1 - clip.0) as f32).max(front_h));
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
        face(c, spine, a, b, turn.cy, spine_h, shade_for(cos, turn.shade), clip, &mut covered);
    }
    if sin > 0.001 {
        face(c, front, b, e, turn.cy, front_h, shade_for(sin, turn.shade), clip, &mut covered);
    }
    (covered.0, covered.1, covered.2 - covered.0, covered.3 - covered.1)
}

/// A case's cover `tex` swinging open towards you about the hinge at its left edge (a jewel or DVD case): `open` 0 shut
/// (the picture as it is at `(x, y)`, `w` x `h`) to 1 at `max_deg`. In the same perspective as [`draw_turning`]: the edge
/// that lifts comes nearer and grows, each column of the picture where it really is, never just squeezed. Lit by `shade`
/// (0-256), darker as it turns from the light. Returns the screen rectangle it covered.
#[allow(clippy::too_many_arguments)]
pub fn draw_hinged(c: &mut Canvas, tex: &Pic, (x, y): (f32, f32), (w, h): (f32, f32), open: f32, max_deg: f32, shade: u32, clip: (i32, i32)) -> (i32, i32, i32, i32) {
    let th = open.clamp(0.0, 1.0) * max_deg.to_radians();
    let (sin, cos) = th.sin_cos();
    // The free edge, w from the hinge, comes towards you by w·sin: nearer is larger.
    let k = FOCAL / (FOCAL - w * sin).max(FOCAL * 0.25);
    let a = (x, 1.0);
    let b = (x + w * cos * k, k);
    let mut covered = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
    face(c, tex, a, b, y + h / 2.0, h, shade_for(cos, shade), clip, &mut covered);
    if covered.0 == i32::MAX {
        return (x as i32, y as i32, 0, 0);
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
    let (c0, c1) = (x0.max(0), x1.min(c.w));
    if c0 >= c1 {
        return;
    }
    // Each column once: its rows (r0..r1), the picture's column, where its top is, the step down the picture and how
    // much of the column the face covers (edge columns only partly).
    struct Col {
        r0: i32,
        r1: i32,
        tx: i32,
        top: f32,
        step: f32,
        edge: f32,
    }
    let cols: Vec<Col> = (c0..c1)
        .map(|col| {
            let t = ((col as f32 + 0.5 - a.0) / (b.0 - a.0)).clamp(0.0, 1.0);
            // Perspective-correct position across the face.
            let u = t * b.1 / ((1.0 - t) * a.1 + t * b.1);
            let k = a.1 + (b.1 - a.1) * t;
            let ch = h * k;
            let top = cy - ch / 2.0;
            let tx = ((u * tex.w as f32) as i32).clamp(0, tex.w - 1);
            let (r0, r1) = ((top.floor() as i32).max(clip.0).max(0), ((top + ch).ceil() as i32).min(clip.1).min(c.h));
            let edge = if col == x0 {
                1.0 - (a.0 - x0 as f32)
            } else if col == x1 - 1 {
                1.0 - (x1 as f32 - b.0)
            } else {
                1.0
            };
            Col { r0, r1, tx, top, step: tex.h as f32 / ch, edge: edge.clamp(0.0, 1.0) }
        })
        .collect();
    let (mut lo, mut hi) = (i32::MAX, i32::MIN);
    for (col, k) in (c0..).zip(&cols) {
        if k.r0 < k.r1 {
            covered.0 = covered.0.min(col);
            covered.2 = covered.2.max(col + 1);
            covered.1 = covered.1.min(k.r0);
            covered.3 = covered.3.max(k.r1);
            lo = lo.min(k.r0);
            hi = hi.max(k.r1);
        }
    }
    if lo >= hi {
        return;
    }
    let paint = |col: i32, row: i32, bg: u32| {
        let k = &cols[(col - c0) as usize];
        if row < k.r0 || row >= k.r1 {
            return bg;
        }
        let ty = (((row as f32 + 0.5 - k.top) * k.step) as i32).clamp(0, tex.h - 1);
        let s = tex.px[(ty * tex.w + k.tx) as usize];
        let alpha = s >> 24;
        if alpha == 0 {
            return bg;
        }
        let alpha = (alpha as f32 * k.edge) as u32;
        if alpha >= 255 {
            shaded(s, shade)
        } else {
            mix(bg, shaded(s, shade), alpha)
        }
    };
    if c.n() != 1 {
        // Painted natively: the four design pixels of each screen pixel as below, averaged.
        return cells_over(c, c0, c1, lo, hi, paint);
    }
    // Row by row: column by column every pixel was 4 KB from the last on the Brick's 1024x768, and the A53 paid a cache miss
    // for each (a turning case took 12.7 of a Shelf frame's 22 ms, BRICK-60FPS.md). Each pixel is the one `paint` gives, with
    // the column's numbers at hand and a whole column's alpha left alone.
    let c_w = c.w as usize;
    let (tw, last) = (tex.w as usize, tex.h - 1);
    let px = c.pixels_mut();
    for row in lo..hi {
        let line = &mut px[row as usize * c_w..][c0 as usize..c1 as usize];
        let at = row as f32 + 0.5;
        for (k, d) in cols.iter().zip(line.iter_mut()) {
            if row < k.r0 || row >= k.r1 {
                continue;
            }
            let ty = (((at - k.top) * k.step) as i32).clamp(0, last) as usize;
            let s = tex.px[ty * tw + k.tx as usize];
            let alpha = s >> 24;
            if alpha == 0 {
                continue;
            }
            let alpha = if k.edge >= 1.0 { alpha } else { (alpha as f32 * k.edge) as u32 };
            *d = if alpha >= 255 { shaded(s, shade) } else { mix(*d, shaded(s, shade), alpha) };
        }
    }
}

// ---- into the console ---------------------------------------------------------------------------------

/// A compact disc seen from its data side, radius `r`, a hint of the cover's colour in the sheen.
pub fn disc(c: &mut Canvas, cx: i32, cy: i32, r: i32, tint: u32, clip_bottom: i32) {
    let rf = r as f32;
    let base = mix(0xc4c8ce, tint, 18);
    let paint = |col: i32, row: i32, bg: u32| {
        let (dx, dy) = (col - cx, row - cy);
        let d = ((dx * dx + dy * dy) as f32).sqrt();
        let cover = (rf - d + 0.5).clamp(0.0, 1.0);
        if cover <= 0.0 || d < rf * 0.075 {
            return bg;
        }
        let color = if d < rf * 0.2 {
            0x8e939a
        } else if d < rf * 0.3 {
            0xb9bec5
        } else {
            // Silver with a rainbow arc where the light falls.
            let ang = (dy as f32).atan2(dx as f32);
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
            mix(mix(base, 0x7c8088, ((d / rf) * 60.0) as u32), rainbow, (sheen * 120.0) as u32)
        };
        mix(bg, color, (cover * 255.0) as u32)
    };
    let (y0, y1) = ((cy - r).max(0), (cy + r + 1).min(c.h).min(clip_bottom));
    let (x0, x1) = ((cx - r).max(0), (cx + r + 1).min(c.w));
    if c.n() != 1 {
        // Painted natively: the four design pixels of each screen pixel as below, averaged.
        return cells_over(c, x0, x1, y0, y1, paint);
    }
    let c_w = c.w;
    for row in y0..y1 {
        for col in x0..x1 {
            let i = (row * c_w + col) as usize;
            c.pixels_mut()[i] = paint(col, row, c.pixels()[i]);
        }
    }
}

/// A UMD: a disc in a small shell with a window. As a real one with its label upright (photos of "Pursuit Force UMD" and
/// "Cars UMD", Commons): the shell is straight at the top and rounded at the bottom, hugging the label.
pub fn umd(c: &mut Canvas, cx: i32, cy: i32, r: i32, tint: u32, clip_bottom: i32) {
    let (w, h) = (r * 2, r * 2 + r / 5);
    let mut p = Pic::new(w, h, OPAQUE | 0x2a2b30);
    p.tint(0, 0, w, 2, 0xffffff, 60);
    p.round([r as f32 * 0.25, r as f32 * 0.25, r as f32 * 0.5, r as f32 * 0.5]);
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
    #[test]
    fn dim_at_n2_is_rows_over_bit_for_bit_whatever_its_edges() {
        // Whole screen rows take one multiply (`dim`); the band's edges at odd rows still mix by their share.
        let back: Vec<u32> = (0..40 * 30).map(|i: u32| i.wrapping_mul(2654435761) & 0xffffff).collect();
        for (y0, y1) in [(0, 60), (5, 41), (1, 2), (1, 3), (2, 4), (7, 8), (-3, 70), (59, 60), (0, 1), (13, 58)] {
            for keep in [0, 1, 100, 153, 255] {
                let (mut a, mut b) = (back.clone(), back.clone());
                dim(&mut Canvas::screen(&mut a, (40, 30), 2), y0, y1, keep);
                rows_over(&mut Canvas::screen(&mut b, (40, 30), 2), 0, 80, y0, y1, |_, bg| shaded(bg, keep));
                assert_eq!(a, b, "band {y0}..{y1}, keep {keep}");
            }
        }
    }

    #[test]
    fn rows_over_is_cells_over_for_a_band_whatever_its_edges() {
        // A background that changes from screen pixel to screen pixel, bands with odd and even edges.
        let back: Vec<u32> = (0..40 * 30).map(|i: u32| i.wrapping_mul(2654435761) & 0xffffff).collect();
        for (x0, x1, y0, y1) in [(0, 80, 0, 60), (3, 77, 5, 41), (1, 2, 1, 2), (10, 11, 7, 8), (-4, 90, -3, 70)] {
            let f = |row: i32, bg: u32| shaded(bg, (row as u32 * 7) % 256);
            let (mut a, mut b) = (back.clone(), back.clone());
            cells_over(&mut Canvas::screen(&mut a, (40, 30), 2), x0, x1, y0, y1, |_, row, bg| f(row, bg));
            rows_over(&mut Canvas::screen(&mut b, (40, 30), 2), x0, x1, y0, y1, f);
            assert_eq!(a, b, "band {x0}..{x1} x {y0}..{y1}");
        }
    }

    use super::*;

    /// `blit`'s rows of eight solid pixels at a time paint what pixel by pixel painted: solid, see-through, half and
    /// mixed runs, rows not a multiple of eight long, cut at both ends, at full light and shaded.
    #[test]
    fn a_blit_paints_solid_runs_as_pixel_by_pixel_did() {
        let (w, h) = (53, 7);
        let px: Vec<u32> = (0..w * h)
            .map(|i| {
                let a = match (i / 5) % 7 { 0 | 1 | 2 => 255, 3 => 0, 4 => 128, 5 => 255, _ => (i * 37 % 256) as u32 };
                a << 24 | (i as u32).wrapping_mul(0x9e37_79b9) & 0xffffff
            })
            .collect();
        let pic = Pic::of(w, h, px);
        for shade in [256, 255, 200, 90] {
            for (x, cols) in [(3, (0, w)), (-4, (0, w)), (60 - w, (0, w)), (5, (7, 31))] {
                let mut got: Vec<u32> = (0..64 * 12).map(|i| 0x0010_2030 + i).collect();
                let mut want = got.clone();
                blit_cols(&mut Canvas::new(&mut got, 64, 12), &pic, x, 2, shade, (0, 12), cols);
                let (x0, x1) = ((x + cols.0).max(0), (x + cols.1).min(64));
                for row in 2..2 + h {
                    for col in x0..x1 {
                        put(&mut want[(row * 64 + col) as usize], pic.px[((row - 2) * w + col - x) as usize], shade);
                    }
                }
                assert_eq!(got, want, "shade {shade}, x {x}, cols {cols:?}");
            }
        }
    }

    /// 15.11.: the code-drawn UMD (a game without a picture of the real shell) is as a real one with its label upright: straight at
    /// the top, rounded at the bottom. It was round on top, a UMD upside down.
    #[test]
    fn the_drawn_umd_is_straight_at_the_top_and_rounded_at_the_bottom() {
        let mut buf = vec![0u32; 120 * 120];
        let mut c = Canvas::new(&mut buf, 120, 120);
        umd(&mut c, 60, 60, 40, 0x808080, 120);
        let row = |y: usize| buf[y * 120..(y + 1) * 120].iter().filter(|p| **p != 0).count();
        let (top, bottom) = (row(60 - 44), row(60 + 43));
        assert!(top > bottom + 10, "straight end up ({top} px wide), rounded end down ({bottom} px wide)");
    }

    /// One picture painted the old way (design pixels, then the 2:1 filter) and natively (`Canvas::screen`, `n` = 2),
    /// on a background that varies from screen pixel to screen pixel but is even within each (as a page is wherever
    /// these paint): the two must be the same, bit for bit (docs/dev/native-paint.md).
    fn old_and_new(w: u32, h: u32, paint: impl Fn(&mut Canvas)) -> (Vec<u32>, Vec<u32>) {
        let back = |x: u32, y: u32| 0x203040 + ((x * 7 + y * 13) % 32) * 0x010203;
        let mut design: Vec<u32> = (0..h * 2).flat_map(|y| (0..w * 2).map(move |x| back(x / 2, y / 2))).collect();
        paint(&mut Canvas::new(&mut design, w * 2, h * 2));
        let mut old = vec![0u32; (w * h) as usize];
        crate::scaled_layout::ScaledFrame::default().downscale(&design, (w * 2, h * 2), &mut old, (w, h));
        let mut new: Vec<u32> = (0..h).flat_map(|y| (0..w).map(move |x| back(x, y))).collect();
        paint(&mut Canvas::screen(&mut new, (w, h), 2));
        (old, new)
    }

    fn same(what: &str, (old, new): (Vec<u32>, Vec<u32>)) {
        let off = old.iter().zip(&new).filter(|(a, b)| a != b).count();
        assert_eq!(off, 0, "{what}: {off} of {} screen pixels differ from the old picture", old.len());
    }

    /// As `same`, but each channel may be `steps` apart (a mean shaded after it is taken, `blit_half`).
    fn close(what: &str, (old, new): (Vec<u32>, Vec<u32>), steps: u32) {
        let far = |a: u32, b: u32| [16, 8, 0].iter().map(|s| ((a >> s & 0xff) as i32 - (b >> s & 0xff) as i32).unsigned_abs()).max().unwrap();
        let worst = old.iter().zip(&new).map(|(a, b)| far(*a, *b)).max().unwrap_or(0);
        assert!(worst <= steps, "{what}: {worst} steps from the old picture, at most {steps}");
    }

    /// A picture with solid, see-through and half-solid pixels, in colours that change from pixel to pixel.
    fn sample_pic(w: i32, h: i32) -> Pic {
        let mut p = Pic::new(w, h, 0);
        for (i, px) in p.px.iter_mut().enumerate() {
            let i = i as u32;
            let a = match i % 7 {
                0 => 0,
                3 => 0x80,
                _ => 0xff,
            };
            *px = a << 24 | (i * 0x0a0b0c & 0xffffff);
        }
        p
    }

    #[test]
    fn the_shelf_strip_from_half_tiles_is_the_old_picture_at_every_scroll() {
        let (tw, room) = (16usize, 11usize);
        let tiles: Vec<Vec<u32>> = (0..5u32).map(|t| (0..(tw * room) as u32).map(|i| (i * 0x0a0b0c + t * 0x402010) & 0xffffff).collect()).collect();
        for (left, top) in [(6, 4), (7, 5), (6, 5), (7, 4)] {
            for scroll in [0, 1, 5, 16, 17, 30] {
                let (view, first) = (37, scroll / tw as i32);
                // The old way: the strip put together in design pixels, then copied.
                let strip: Vec<u32> = (0..room).flat_map(|y| (0..view).map(move |x| (x + scroll, y))).map(|(c, y)| tiles[c as usize / tw][y * tw + c as usize % tw]).collect();
                let fy = |c: &Canvas| c.cell(0, top).3;
                let at = format!("at ({left}, {top}), scroll {scroll}");
                same(&at, old_and_new(32, 12, |c| {
                    if c.n() == 1 {
                        return copy_solid(c, left, top, view as usize, &strip);
                    }
                    let fx = (left - scroll).rem_euclid(2) as u8;
                    let used: Vec<&[u32]> = tiles[first as usize..].iter().map(|t| t.as_slice()).collect();
                    let halves: Vec<Vec<u32>> = used.iter().map(|t| half_tile(t, tw, fx, fy(c))).collect();
                    let halves: Vec<&[u32]> = halves.iter().map(|h| h.as_slice()).collect();
                    paint_strip(c, (left, top), (view, room as i32), scroll, first, tw as i32, &used, &halves);
                }));
            }
        }
    }

    #[test]
    fn a_translucent_film_is_pixel_exact_at_every_place_and_shade() {
        for alpha in [1, 8, 38, 85, 128, 254] {
            let film = Pic::new(23, 17, (alpha << 24) | crate::shrink_wrap::FILM);
            for (x, y) in [(10, 8), (11, 9), (10, 9), (11, 8), (-3, -5), (90, 60)] {
                for shade in [80, 180, 256] {
                    same("flat film", old_and_new(48, 32, |c| blit(c, &film, x, y, shade, (y + 1, y + 16))));
                }
            }
        }
        // The mixed-cell sentinel can itself be a real (almost transparent black) pixel.
        let film = Pic::new(23, 17, HALF_MIXED);
        same("sentinel colour", old_and_new(48, 32, |c| blit(c, &film, 11, 9, 256, (0, c.h))));
    }

    #[test]
    fn a_solid_block_painted_natively_is_the_old_picture_at_every_place() {
        let block: Vec<u32> = (0..17 * 9).map(|i| i * 0x030507 & 0xffffff).collect();
        for (x, y) in [(10, 8), (11, 9), (10, 9), (11, 8), (-3, -5), (90, 60)] {
            same(&format!("copy_solid at ({x}, {y})"), old_and_new(48, 32, |c| copy_solid(c, x, y, 17, &block)));
        }
    }

    #[test]
    fn the_cases_and_the_cabinet_painted_natively_are_the_old_picture() {
        let p = sample_pic(23, 17);
        let disc_pic = sample_pic(30, 30);
        for (x, y) in [(10, 8), (11, 9), (10, 9), (-5, -3), (70, 40)] {
            let at = format!(" at ({x}, {y})");
            same(&format!("blit{at}"), old_and_new(48, 32, |c| blit(c, &p, x, y, 256, (0, c.h))));
            close(&format!("blit shaded, clipped{at}"), old_and_new(48, 32, |c| blit(c, &p, x, y, 180, (y + 3, y + 12))), 1);
            let solid = Pic::of(p.w, p.h, p.px.iter().map(|q| q | OPAQUE).collect());
            same(&format!("blit solid{at}"), old_and_new(48, 32, |c| blit(c, &solid, x, y, 256, (0, c.h))));
            close(&format!("blit solid, shaded{at}"), old_and_new(48, 32, |c| blit(c, &solid, x, y, 200, (0, c.h))), 1);
            same(&format!("blit_cols{at}"), old_and_new(48, 32, |c| blit_cols(c, &solid, x, y, 256, (0, c.h), (3, 18))));
            same(&format!("blit_sheared{at}"), old_and_new(48, 32, |c| blit_sheared(c, &p, x, y, 5.3, 230, (0, c.h))));
            same(&format!("blit_scaled{at}"), old_and_new(48, 32, |c| blit_scaled(c, &p, x, y, 31, 13, 256, (0, c.h))));
            same(&format!("blit_spun{at}"), old_and_new(48, 32, |c| blit_spun(c, &disc_pic, x as f32 + 15.3, y as f32 + 14.7, 0.7, (0, c.h))));
            same(&format!("blit_affine{at}"), old_and_new(48, 32, |c| blit_affine(c, &p, (x as f32 + 0.3, y as f32), (0.9, 0.2), (-0.3, 1.1), 240, (0, c.h))));
            same(&format!("blit_pose{at}"), old_and_new(48, 32, |c| blit_pose(c, &p, x as f32 + 12.0, y as f32 + 9.0, 1.3, 0.4, 200, None, (0, c.h))));
            same(&format!("ellipse{at}"), old_and_new(48, 32, |c| ellipse(c, x + 10, y + 8, 9, 5, 0x804020)));
            same(&format!("glow{at}"), old_and_new(48, 32, |c| glow(c, x + 10, y + 8, 13, 7, 0xffe0a0, 0.8)));
            close(&format!("soft_shadow{at}"), old_and_new(48, 32, |c| soft_shadow(c, x, y, 21, 11, 5, 0.6, (0, c.h))), 2);
            close(&format!("soft_shadow clipped{at}"), old_and_new(48, 32, |c| soft_shadow(c, x, y, 21, 11, 5, 0.6, (y + 3, y + 10))), 2);
            same(&format!("disc{at}"), old_and_new(48, 32, |c| disc(c, x + 14, y + 13, 12, 0x3060c0, y + 20)));
            same(&format!("dim{at}"), old_and_new(48, 32, |c| dim(c, y, y + 19, 150)));
            same(&format!("fade_band{at}"), old_and_new(48, 32, |c| fade_band(c, y, y + 15)));
            same(&format!("fade_band up{at}"), old_and_new(48, 32, |c| fade_band(c, y + 15, y)));
            same(&format!("fade_band_to{at}"), old_and_new(48, 32, |c| fade_band_to(c, y + 14, y + 1, 0xe8e0d0)));
            let turn = Turn { cx: x as f32 + 20.5, cy: y as f32 + 14.0, scale: 0.37, turned: 0.43, shade: 240 };
            // The front is painted over the spine's last screen column, which at `n` = 2 is already their mean: only
            // that one column of the seam may differ from the old picture.
            let (old, new) = old_and_new(48, 32, |c| {
                draw_turning(c, &p, &disc_pic, Size { spine: 14, front: 80, h: 60, front_h: 60 }, turn, (0, c.h));
            });
            let cols: std::collections::BTreeSet<usize> = old.iter().zip(&new).enumerate().filter(|(_, (a, b))| a != b).map(|(i, _)| i % 48).collect();
            assert!(cols.len() <= 1, "draw_turning{at}: columns {cols:?} differ from the old picture, only the seam may");
        }
    }

    #[test]
    fn a_shadow_beside_the_canvas_darkens_nothing() {
        let mut buf = vec![0xffffffu32; 64 * 32];
        let mut c = Canvas::new(&mut buf, 64, 32);
        soft_shadow(&mut c, 80, 4, 3, 20, 5, 0.3, (0, 32));
        soft_shadow(&mut c, -40, 4, 3, 20, 5, 0.3, (0, 32));
        assert!(buf.iter().all(|p| *p == 0xffffff));
    }

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

    /// A case turning on the Brick's 1024x768 (the Shelf's large front, the Cabinet's case coming out) was painted column by column:
    /// each pixel a row's width from the one before, a cache miss each on the A53. Its moving frames took 22 ms (Shelf) and 15 ms
    /// (Cabinet), 12.7 and 6.2 of them the turn (BRICK-60FPS.md, "Damage"). Painted row by row, the same turn costs the same on a canvas
    /// 16384 pixels wide as on one just wider than the case; column by column the wide one cost several times more on every machine.
    #[test]
    #[ignore = "timing tier: scripts/test-timing.sh (serial)"]
    fn timing_a_turning_case_costs_the_same_on_a_wide_screen() {
        let size = Size { spine: 40, front: 300, h: 420, front_h: 420 };
        let (spine, front) = (Pic::of(40, 420, sample_pic(40, 420).px.iter().map(|q| q | OPAQUE).collect()), Pic::of(300, 420, sample_pic(300, 420).px.iter().map(|q| q | OPAQUE).collect()));
        let turn = Turn { cx: 200.0, cy: 384.0, scale: 1.4, turned: 0.6, shade: 256 };
        let cost = |w: u32| {
            let mut buf = vec![0x203040u32; (w * 768) as usize];
            let mut c = Canvas::new(&mut buf, w, 768);
            let mut covered = (0, 0, 0, 0);
            let took = (0..15)
                .map(|_| {
                    let t = thread_cpu();
                    covered = draw_turning(&mut c, &spine, &front, size, turn, (0, 768));
                    thread_cpu() - t
                })
                .min()
                .unwrap();
            (took, covered)
        };
        let ((narrow, covered), (wide, _)) = (cost(400), cost(16384));
        assert!(covered.2 > 300 && covered.3 > 550 && covered.0 + covered.2 <= 400, "a large case inside the narrow canvas: {covered:?}");
        eprintln!("turning {covered:?}: {narrow:?} on 400 pixels wide, {wide:?} on 16384");
        // Measured: column by column 3.3x on the Air, 2.25x on x86; row by row 1.0x and 1.15x.
        assert!(wide * 5 < narrow * 8, "{wide:?} on a wide canvas against {narrow:?} on a narrow one");
    }

    #[test]
    fn a_case_facing_you_keeps_its_size_and_turned_shows_its_front_only() {
        let mut buf = vec![0u32; 800 * 600];
        let mut c = Canvas::new(&mut buf, 800, 600);
        let ff = crate::cabinet::FormFactor::DvdCase;
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
        let green = c.pixels().iter().filter(|p| **p & 0xffffff == 0x00ff00).count();
        assert_eq!(green, 0, "no front yet");
        c.clear(0);
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
        assert_eq!(c.pixels().iter().filter(|p| **p & 0xff0000 == 0xff0000).count(), 0, "the spine has turned away");
        // Halfway, both show and the whole thing is narrower than spine plus front.
        c.clear(0);
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
        assert!(c.pixels().iter().any(|p| *p & 0xff0000 != 0) && c.pixels().iter().any(|p| *p & 0x00ff00 != 0));
        assert!(r.2 < size.spine + size.front, "{r:?}");
    }

    #[test]
    fn a_cover_is_fitted_whole_into_a_label_of_another_shape() {
        // A tall cover (red with a blue top row) in a wide label: all of it, the top row included, in the middle.
        let mut px = vec![0xc02020u32; 40 * 80];
        px[..40].fill(0x2020c0);
        let img = Image { w: 40, h: 80, px };
        let out = cover_fit(&img, 60, 20);
        assert_eq!((out.w, out.h), (60, 20));
        let at = |x: u32, y: u32| out.px[(y * 60 + x) as usize];
        assert_eq!(at(30, 0) & 0xff, (0xc0 + 3 * 0x20) / 4, "the cover's top row is there, not cut off: {:06x}", at(30, 0));
        assert!(at(30, 10) >> 16 & 0xff > 0xa0, "its middle");
        assert!(at(2, 10) >> 16 & 0xff < 0xa0, "a margin beside it, darker: {:06x}", at(2, 10));
        // The same shape: no margin, the cover fills it.
        let same = cover_fit(&img, 20, 40);
        assert!(same.px[20 * 20..].iter().all(|p| *p == 0xc02020), "no margin where the shapes agree");
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
        assert_eq!(c.pixels()[0], 0x404040);
        assert_eq!(c.pixels()[1], 0x101010, "transparent pixel left alone");
    }
}

/// `p` drawn through an affine map, nearest pixel: its pixel (s, t) lands on the screen at `o + s·u + t·v`. A row walks
/// the picture in 16.16 fixed point and only over the columns the picture covers, so it costs little more than a
/// plain blit of the same pixels (a lid swinging, a disc lying down: `insert_rig`). Nothing is made.
pub fn blit_affine(c: &mut Canvas, p: &Pic, o: (f32, f32), u: (f32, f32), v: (f32, f32), shade: u32, clip: (i32, i32)) {
    blit_affine_rows(c, p, o, u, v, shade, clip, (0.0, p.h as f32));
}

/// [`blit_affine`] of the picture's rows `rows.0` to `rows.1` only (a cartridge half in a front-loader's opening: the part
/// still outside is drawn again over the console, `insert_scene`).
#[allow(clippy::too_many_arguments)]
pub fn blit_affine_rows(c: &mut Canvas, p: &Pic, o: (f32, f32), u: (f32, f32), v: (f32, f32), shade: u32, clip: (i32, i32), rows: (f32, f32)) {
    let (t_lo, t_hi) = (rows.0.max(0.0), rows.1.min(p.h as f32));
    if t_hi <= t_lo {
        return;
    }
    let det = u.0 * v.1 - u.1 * v.0;
    if det.abs() < 1e-3 || p.w <= 0 || p.h <= 0 {
        return;
    }
    // Screen to picture: s = (dx·v.y - dy·v.x) / det, t = (dy·u.x - dx·u.y) / det.
    let (ds_dx, ds_dy, dt_dx, dt_dy) = (v.1 / det, -v.0 / det, -u.1 / det, u.0 / det);
    let (pw, ph) = (p.w as f32, p.h as f32);
    let corners = [o, (o.0 + u.0 * pw, o.1 + u.1 * pw), (o.0 + v.0 * ph, o.1 + v.1 * ph), (o.0 + u.0 * pw + v.0 * ph, o.1 + u.1 * pw + v.1 * ph)];
    let (mut lo_y, mut hi_y) = (f32::MAX, f32::MIN);
    for k in corners {
        lo_y = lo_y.min(k.1);
        hi_y = hi_y.max(k.1);
    }
    let (y0, y1) = ((lo_y.floor() as i32).max(clip.0).max(0), (hi_y.ceil() as i32).min(clip.1).min(c.h));
    let c_w = c.w;
    const ONE: f32 = 65536.0;
    let (step_s, step_t) = ((ds_dx * ONE) as i64, (dt_dx * ONE) as i64);
    let (max_s, min_t, max_t) = ((p.w as i64) << 16, (t_lo * ONE) as i64, ((t_hi * ONE) as i64).min((p.h as i64) << 16));
    // Each row's columns and where in the picture its first one reads (16.16).
    let span = |row: i32| -> (i32, i32, i64, i64) {
        let dy = row as f32 + 0.5 - o.1;
        // At the row's first column (x = 0.5).
        let s0 = (0.5 - o.0) * ds_dx + dy * ds_dy;
        let t0 = (0.5 - o.0) * dt_dx + dy * dt_dy;
        // The columns where s stays in 0..w and t in 0..h.
        let (mut a, mut b) = (0.0f32, c_w as f32);
        for (v0, d, max) in [(s0, ds_dx, pw), (t0 - t_lo, dt_dx, t_hi - t_lo)] {
            if d.abs() < 1e-6 {
                if v0 < 0.0 || v0 >= max {
                    b = a;
                }
            } else {
                let (e0, e1) = (-v0 / d, (max - v0) / d);
                a = a.max(e0.min(e1));
                b = b.min(e0.max(e1));
            }
        }
        let (x0, x1) = ((a.ceil() as i32).max(0), (b.floor() as i32 + 1).min(c_w));
        (x0, x1, ((s0 + x0 as f32 * ds_dx) * ONE) as i64, ((t0 + x0 as f32 * dt_dx) * ONE) as i64)
    };
    if c.n() != 1 {
        // Painted natively: the same walk (s + k·step is its k-th step exactly), four design pixels a screen pixel.
        if y0 >= y1 {
            return;
        }
        let spans: Vec<(i32, i32, i64, i64)> = (y0..y1).map(span).collect();
        let (lo, hi) = spans.iter().fold((i32::MAX, i32::MIN), |(lo, hi), s| (lo.min(s.0), hi.max(s.1)));
        return cells_over(c, lo, hi, y0, y1, |col, row, bg| {
            let (x0, x1, s, t) = spans[(row - y0) as usize];
            if col < x0 || col >= x1 {
                return bg;
            }
            let k = (col - x0) as i64;
            let (s, t) = (s + k * step_s, t + k * step_t);
            if s >= 0 && t >= min_t && s < max_s && t < max_t {
                over(bg, p.px[((t >> 16) * p.w as i64 + (s >> 16)) as usize], shade)
            } else {
                bg
            }
        });
    }
    for row in y0..y1 {
        let (x0, x1, mut s, mut t) = span(row);
        if x0 >= x1 {
            continue;
        }
        let dst = &mut c.pixels_mut()[(row * c_w + x0) as usize..(row * c_w + x1) as usize];
        for d in dst {
            if s >= 0 && t >= min_t && s < max_s && t < max_t {
                put(d, p.px[((t >> 16) * p.w as i64 + (s >> 16)) as usize], shade);
            }
            s += step_s;
            t += step_t;
        }
    }
}
