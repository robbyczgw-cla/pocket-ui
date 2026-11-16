//! **Sealed new games** (Cozy extras → "Sealed new games", `cozy_extras::Extra::ShrinkWrap`): a game that was never
//! played stands in the shop's clear plastic film on the shelf, in the Cabinet, in Easy, on Split's box art and on its
//! card, and the film tears off the first time it goes into its console. After that the case is unwrapped for good.
//!
//! - **The film's look** comes from a photo of real shrink film (public domain, `assets/media/shrink-wrap/SOURCES.md`):
//!   its long glare band measured across (it jumps to white at one edge and fades out softly at the other) and along
//!   (how its sharp edge strays from a straight line), `glare.png`. [`wrap`] lays two such bands slanted over a picture,
//!   the long one and a thin one beside it, in screen pixels so they are crisp at any size; a hint of milkiness; a fine
//!   light rim where the film turns round the case's edge; the heat-sealed seam near the top with its creases into the
//!   top corners. Elsewhere the film is clear as glass: the cover reads as before. Each game's bands sit a little
//!   elsewhere ([`band_at`]).
//! - **It is baked into each picture**, with glare masks shared across covers of the same outline, rim and one of 19
//!   band positions. A bounded 16 MiB of mask payload survives eviction of a game's large front. Flat covers blend the
//!   mask in one pass; the box keeps its finished back while it is read, including film. [`layer`] is the film alone, for the tearing and the box's back.
//! - **The tear** ([`TEAR`], [`piece`]): at the first start the film whole over the case, a tug, then it rips along a
//!   ragged line and its two halves fly off, turning and fading, while the launch goes on under them. The sound
//!   (`ui_sound::Sound::WrapTear`, a CC0 recording) rips at [`RIP`].
//! - **Which games** (`App::play_evidence`, `play_evidence::Evidence`): no recorded use, save, state, RTC, memory
//!   card or known progress, and not torn open before (`library_meta::GameMeta::unwrapped`).

use std::sync::OnceLock;
use std::time::Duration;

use crate::cabinet_art::Pic;
use crate::gfx::mix;
use crate::image::Image;

/// The film's colour: a cool, slightly blue white.
pub const FILM: u32 = 0xf2f6fa;
/// The film's own faint milkiness over the whole case (of 255): a hint, the cover stays clear.
const HAZE: u32 = 8;
/// The two glare bands' brightest (of 255) on a front: the long one and the thin one beside it.
const GLARE: [u32; 2] = [130, 85];
/// The rim where the film turns round the case's outline: its outermost pixel and the one inside it (of 255).
const RIM: [u32; 2] = [120, 38];
/// The heat-sealed seam near the top and the creases running into the top corners (of 255).
const SEAM: u32 = 85;
/// The bands' slant: they run uphill to the right, as glare on a case lit from above.
const SLANT: f32 = 0.62;

/// The whole tear, from A to the last piece gone.
pub const TEAR: Duration = Duration::from_millis(560);
/// When the film rips (the sound's crack, `ui_sound::hit_at(Sound::WrapTear)`): before it the film is pulled taut.
pub const RIP: Duration = Duration::from_millis(203);
/// How long the launch waits for the tear before its own scene starts: the rip and a moment of the pieces flying;
/// the rest flies over the scene. Only at a game's first start. Twenty 60 Hz frames exactly (`frame_clock::frames`), so a
/// frame of the scene after a tear is the same picture as that frame of a start without one.
pub const LEAD: Duration = Duration::from_nanos(20 * 16_666_667);

const GLARE_PNG: &[u8] = include_bytes!("../assets/media/shrink-wrap/glare.png");

/// The photo's glare (`assets/media/shrink-wrap/glare.png`, `tools/prep.py`): its profile across the band (0-255, 256
/// steps from one side to the other; it jumps to white at one edge and fades out at the other) and how its sharp edge
/// strays from a straight line along the band (photo pixels, 96 to the band's width).
struct Glow {
    profile: [u8; 256],
    edge: [f32; 256],
}

fn glow() -> &'static Glow {
    static G: OnceLock<Glow> = OnceLock::new();
    G.get_or_init(|| {
        let mut g = Glow { profile: [0; 256], edge: [0.0; 256] };
        if let Some(s) = crate::system_art::decode(GLARE_PNG).filter(|s| s.w == 256 && s.h >= 2) {
            let grey = |i: usize| ((s.px[i] >> 16) & 0xff) as f32;
            // The band from its sharp edge on (the photo's first white) to the end of its fade: a clean edge, a soft tail.
            let jump = (0..256).find(|i| grey(*i) >= 250.0).unwrap_or(0);
            for (i, p) in g.profile.iter_mut().enumerate() {
                let at = jump as f32 + i as f32 * (255 - jump) as f32 / 255.0;
                let (k, f) = (at as usize, at.fract());
                *p = (grey(k) * (1.0 - f) + grey((k + 1).min(255)) * f).round() as u8;
            }
            // The edge's stray, smoothed over a sixteenth of the band's length and a quarter as strong: it bends gently.
            let raw: Vec<f32> = (0..256).map(|i| (grey(256 + i) - 128.0) / 4.0 / 96.0).collect();
            for (i, e) in g.edge.iter_mut().enumerate() {
                let (a, b) = (i.saturating_sub(16), (i + 16).min(255));
                *e = raw[a..=b].iter().sum::<f32>() / (b - a + 1) as f32 * 0.25;
            }
        }
        g
    })
}

/// Decodes the glare now (on the caller's thread), so no frame does.
pub fn warm() {
    glow();
}

/// Where a game's long glare band crosses its case (0 to 1 across it), the same each time, another for each game.
fn band_at(seed: u64) -> f32 {
    0.26 + (seed % 19) as f32 / 100.0
}

/// How strong the photo's glare is on a picture: a front's cover takes it whole; a spine, whose title has to stay
/// readable from across the room, and a box's back, which is mostly text, take part of it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Glare {
    Front,
    Spine,
    Back,
}

impl Glare {
    /// Of 100.
    fn share(self) -> u32 {
        match self {
            Glare::Front => 100,
            Glare::Spine => 50,
            Glare::Back => 40,
        }
    }
}

/// The film's alpha (0-255) at each solid pixel of `shape` (index, alpha), `unit` screen pixels a template pixel: two
/// slanted glare bands with the photo's profile and edge, in screen pixels (sharp at any size); a hint of haze; a fine
/// rim round the outline (each row's and column's span); the sealed seam near the top with its creases into the top
/// corners. [`layer`] and [`wrap`] are this in one pass.
#[derive(PartialEq, Eq)]
struct Outline {
    w: i32,
    h: i32,
    rows: Vec<(i32, i32)>,
    cols: Vec<(i32, i32)>,
}

impl Outline {
    fn of(shape: &Pic) -> Self {
        let (w, h) = (shape.w, shape.h);
        let rows = shape.px.chunks(w as usize).map(|row| match row.iter().position(|p| p >> 24 >= 128) {
            Some(a) => (a as i32, row.iter().rposition(|p| p >> 24 >= 128).unwrap() as i32), None => (w, -1),
        }).collect();
        let cols = (0..w).map(|x| {
            let mut a = 0;
            while a < h && shape.px[(a * w + x) as usize] >> 24 < 128 { a += 1; }
            if a == h { return (h, -1); }
            let mut b = h - 1;
            while b > a && shape.px[(b * w + x) as usize] >> 24 < 128 { b -= 1; }
            (a, b)
        }).collect();
        Self { w, h, rows, cols }
    }
}

/// The same film serves every cover with this outline and glare position (there are only 19 positions).
/// Keep masks, not every game's large front: browsing a library must not recompute glare after front eviction.
#[derive(Default)]
struct Films {
    entries: std::collections::VecDeque<(Outline, bool, u64, Glare, std::rc::Rc<Vec<u8>>)>,
    bytes: usize,
}

const FILM_BYTES: usize = 16 * 1024 * 1024;

impl Films {
    fn get(&mut self, shape: &Pic, unit: f32, seed: u64, glare: Glare) -> std::rc::Rc<Vec<u8>> {
        self.outline(Outline::of(shape), unit, seed, glare)
    }

    fn outline(&mut self, outline: Outline, unit: f32, seed: u64, glare: Glare) -> std::rc::Rc<Vec<u8>> {
        let seed = seed % 19;
        let wide_rim = unit >= 2.5;
        if let Some(i) = self.entries.iter().position(|(o, rim, s, g, _)| *rim == wide_rim && *s == seed && *g == glare && *o == outline) {
            let entry = self.entries.remove(i).unwrap();
            let mask = entry.4.clone();
            self.entries.push_back(entry);
            return mask;
        }
        let mut mask = vec![0; (outline.w * outline.h) as usize];
        each_alpha(&outline, unit, seed, glare, |i, a| mask[i] = a as u8);
        let mask = std::rc::Rc::new(mask);
        if mask.len() <= FILM_BYTES {
            while self.bytes + mask.len() > FILM_BYTES {
                self.bytes -= self.entries.pop_front().unwrap().4.len();
            }
            self.bytes += mask.len();
            self.entries.push_back((outline, wide_rim, seed, glare, mask.clone()));
        }
        mask
    }
}

thread_local! { static FILMS: std::cell::RefCell<Films> = std::cell::RefCell::new(Films::default()); }

fn film(shape: &Pic, unit: f32, seed: u64, glare: Glare) -> std::rc::Rc<Vec<u8>> {
    FILMS.with(|f| f.borrow_mut().get(shape, unit, seed, glare))
}

fn each_alpha(outline: &Outline, unit: f32, seed: u64, glare: Glare, mut put: impl FnMut(usize, u32)) {
    let g = glow();
    let share = glare.share();
    let (w, h) = (outline.w, outline.h);
    let (rows, cols) = (&outline.rows, &outline.cols);
    // The rim: one screen pixel, two from about three a template pixel (a TV's 1080 lines) on.
    let rim = if unit >= 2.5 { 2 } else { 1 };
    // The case's top edge (its first solid row) and the seam a little below it.
    let top = (0..h).find(|y| rows[*y as usize].1 >= 0).unwrap_or(0);
    let seam = top + ((h as f32 * 0.03).round() as i32).max(rim + 2);
    let crease = ((w.min(h) as f32) * 0.09).round().max(3.0) as i32;
    // Across the bands (0 to 1 over the case) and along them (0 to 1).
    let (sn, cs) = (SLANT.sin(), SLANT.cos());
    let across_len = (w as f32 * sn + h as f32 * cs).max(1.0);
    let along_len = (w as f32 * cs + h as f32 * sn).max(1.0);
    let c0 = band_at(seed);
    let bands = [(c0, 0.15f32, GLARE[0]), (c0 + 0.27, 0.05f32, GLARE[1])];
    for y in 0..h {
        let (r0, r1) = rows[y as usize];
        for x in r0.max(0)..=r1.min(w - 1) {
            let v = (x as f32 * sn + y as f32 * cs) / across_len;
            let s = ((x as f32 * cs - y as f32 * sn + h as f32 * sn) / along_len).clamp(0.0, 1.0);
            let wig = g.edge[(s * 255.0) as usize];
            let mut a = HAZE;
            for (c, bw, peak) in bands {
                // 0 at the band's sharp edge, 1 where its fade ends.
                let t = (v - c) / bw + wig;
                if (0.0..1.0).contains(&t) {
                    a += u32::from(g.profile[(t * 255.0) as usize]) * peak * share / (255 * 100);
                }
            }
            let (c0y, c1y) = cols[x as usize];
            // The reusable mask also spans transparent gaps; their colour is never applied.
            let d = (x - r0).min(r1 - x).min(y - c0y).min(c1y - y).max(0);
            if d < 2 * rim {
                a += RIM[(d / rim).min(1) as usize];
            }
            // The seam: a crisp line with a softer one under it, and creases from it into the top corners.
            let dy = y - seam;
            if dy == 0 || dy == 1 {
                a += if dy == 0 { SEAM } else { SEAM / 3 } * (60 + (wig.abs() * 400.0).min(40.0) as u32) / 100;
            }
            let into = y - top;
            if (0..crease).contains(&into) && (x - r0 == into || r1 - x == into) {
                a += SEAM * 2 / 3;
            }
            put((y * w + x) as usize, a.min(255));
        }
    }
}

/// The film alone over the shape of `shape` (its solid pixels), `unit` screen pixels a template pixel: the film's
/// colour with its alpha, see-through where `shape` is. What [`wrap`] lays over a picture.
pub fn layer(shape: &Pic, unit: f32, seed: u64, glare: Glare) -> Pic {
    let mut out = Pic::new(shape.w, shape.h, 0);
    let mask = film(shape, unit, seed, glare);
    for ((out, src), a) in out.px.iter_mut().zip(&shape.px).zip(mask.iter()) {
        if src >> 24 >= 128 { *out = (u32::from(*a) << 24) | FILM; }
    }
    out
}

/// `p` in its film: [`layer`] laid over it (what a straight-alpha blit of the layer over `p` gives, so a case and its
/// film painted apart look the same as the case made wrapped). One pass, nothing allocated per pixel.
pub fn wrap(p: &mut Pic, unit: f32, seed: u64, glare: Glare) {
    let mask = film(p, unit, seed, glare);
    coat_pixels(&mut p.px, &mask);
    // Its half copies for native painting (`Pic::half`), if any were made, are of the case without film.
    p.changed();
}

/// Four pixels together on x86-64's baseline SSE2. Its packed 16-bit products cover the full blend (at most
/// 255*255); the scalar packed red/blue products in `coated` need 32-bit multiplies, which SSE2 cannot do in four
/// lanes. LLVM emulates those with multiple products and shuffles, adding work to every newly selected Easy
/// case. AArch64 already handles the ordinary loop efficiently. Both paths use the exact same division by 255.
fn coat_pixels(pixels: &mut [u32], mask: &[u8]) {
    let len = pixels.len().min(mask.len());
    #[cfg(not(target_arch = "x86_64"))]
    let done = 0;
    #[cfg(target_arch = "x86_64")]
    let done = {
        use std::arch::x86_64::*;
        let mut done = 0;
        // SAFETY: SSE2 is part of x86-64. Each unaligned load/store covers four pixels inside these slices;
        // the mask load covers their four bytes. The remaining zero to three pixels use the scalar path.
        unsafe {
            let zero = _mm_setzero_si128();
            let one = _mm_set1_epi16(1);
            let full = _mm_set1_epi16(255);
            let rgb = _mm_set1_epi32(0x00ff_ffff);
            let film = _mm_unpacklo_epi8(_mm_set1_epi32(FILM as i32), zero);
            let film255 = _mm_mullo_epi16(film, full);
            while done + 4 <= len {
                let src = _mm_loadu_si128(pixels.as_ptr().add(done).cast());
                let solid = _mm_srai_epi32::<31>(src);
                let alpha = _mm_cvtsi32_si128(i32::from_ne_bytes(mask[done..done + 4].try_into().unwrap()));
                let alpha = _mm_unpacklo_epi8(alpha, zero);
                let alpha = _mm_unpacklo_epi16(alpha, alpha);
                let lo = _mm_and_si128(_mm_unpacklo_epi32(alpha, alpha), _mm_unpacklo_epi32(solid, solid));
                let hi = _mm_and_si128(_mm_unpackhi_epi32(alpha, alpha), _mm_unpackhi_epi32(solid, solid));
                let blend = |p, a| {
                    // F*255 + (p-F)*(255-a) is p*(255-a)+F*a, with one product. Intermediate 16-bit
                    // wrapping cancels; the sum is in 0..=255*255. In that range high((q+1)*257) is floor(q/255).
                    let q = _mm_add_epi16(film255, _mm_mullo_epi16(_mm_sub_epi16(p, film), _mm_sub_epi16(full, a)));
                    _mm_mulhi_epu16(_mm_add_epi16(q, one), _mm_set1_epi16(257))
                };
                let lo = blend(_mm_unpacklo_epi8(src, zero), lo);
                let hi = blend(_mm_unpackhi_epi8(src, zero), hi);
                let out = _mm_or_si128(_mm_and_si128(_mm_packus_epi16(lo, hi), rgb), _mm_andnot_si128(rgb, src));
                _mm_storeu_si128(pixels.as_mut_ptr().add(done).cast(), out);
                done += 4;
            }
        }
        done
    };
    for (d, a) in pixels[done..len].iter_mut().zip(&mask[done..len]) {
        *d = coated(*d, *a);
    }
}

/// Exactly `mix` with the film, keeping the case's alpha. Branchless, with red and blue in parallel: the compiler
/// can blend a row with SIMD rather than branching and dividing three channels at every pixel of a new cover.
#[inline]
fn coated(d: u32, alpha: u8) -> u32 {
    let a = u32::from(alpha) & 0u32.wrapping_sub(d >> 31);
    let t = 255 - a;
    let rb = (d & 0xff00ff) * t + (FILM & 0xff00ff) * a;
    let g = ((d >> 8) & 255) * t + ((FILM >> 8) & 255) * a;
    // Exact floor division by 255 for products up to 255*255, independently in each 16-bit lane.
    let rb = ((rb + ((rb >> 8) & 0xff00ff) + 0x010001) >> 8) & 0xff00ff;
    let g = (g + (g >> 8) + 1) >> 8;
    (d & 0xff00_0000) | rb | (g << 8)
}

/// A cover picture in its film (Split's box art, the card's cover): the whole rectangle, its rim at the edges.
pub fn wrap_image(img: &Image, unit: f32, seed: u64) -> Image {
    let (w, h) = (img.w as i32, img.h as i32);
    let outline = Outline { w, h, rows: vec![(0, w - 1); h as usize], cols: vec![(0, h - 1); w as usize] };
    let mask = FILMS.with(|f| f.borrow_mut().outline(outline, unit, seed, Glare::Front));
    let px = img.px.iter().zip(mask.iter()).map(|(p, a)| coated(0xff00_0000 | p, *a) & 0xffffff).collect();
    Image { w: img.w, h: img.h, px }
}

/// The film torn in two along a ragged line from the top's right part to the left's lower part (a game's own): the
/// upper left half and the lower right one, each the size of `film` and see-through elsewhere.
pub fn halves(film: &Pic, seed: u64) -> (Pic, Pic) {
    let (w, h) = (film.w.max(1), film.h.max(1));
    let mut a = Pic::new(w, h, 0);
    let mut b = Pic::new(w, h, 0);
    // The line: where it crosses each row, with a jag every few pixels from the seed.
    let jag = (w / 24).max(2);
    let mut s = seed | 1;
    for y in 0..h {
        if y % jag == 0 {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
        }
        let t = y as f32 / h as f32;
        let x_line = (w as f32 * (0.78 - 0.62 * t)) as i32 + ((s % (2 * jag as u64 + 1)) as i32 - jag);
        for x in 0..w {
            let i = (y * w + x) as usize;
            if x < x_line {
                a.px[i] = film.px[i];
            } else {
                b.px[i] = film.px[i];
            }
        }
    }
    (a, b)
}

/// Where a half of the film is at `secs` into the tear: its offset from its place in units of the case's height, its
/// scale, its turn (radians) and its alpha (0-255). `first`: the upper left half (it goes up and left), else the lower
/// right (down and right). Before [`RIP`] both halves sit, pulled a little taut; after it they fly and fade, gone at
/// [`TEAR`].
pub fn piece(secs: f32, first: bool) -> Piece {
    let rip = RIP.as_secs_f32();
    if secs < rip {
        // The tug: a slight squeeze towards the corner being pulled.
        let t = (secs / rip).clamp(0.0, 1.0);
        let tug = (t * std::f32::consts::PI).sin() * 0.012;
        return Piece { dx: if first { -tug } else { 0.0 }, dy: if first { -tug } else { 0.0 }, scale: 1.0, turn: 0.0, alpha: 255 };
    }
    let t = ((secs - rip) / (TEAR.as_secs_f32() - rip)).clamp(0.0, 1.0);
    // Fast off, slowing (ease out), fading in its second half.
    let e = 1.0 - (1.0 - t) * (1.0 - t);
    let side = if first { -1.0 } else { 1.0 };
    Piece {
        dx: side * 0.34 * e,
        dy: if first { -0.30 * e } else { 0.22 * e + 0.10 * t * t },
        scale: 1.0 + 0.10 * e,
        turn: side * 0.30 * e,
        alpha: if t >= 1.0 { 0 } else { (255.0 * (1.0 - ((t - 0.35).max(0.0) / 0.65))).round() as u32 },
    }
}

/// Paints one half of the film (`half`, from [`halves`]) at its moment `p` over a case standing at `rect` (x, y, width,
/// height on the canvas): canvas calls only. While it only sits or tugs it is the film exactly (`cabinet_art::blit`, the
/// same picture as the sealed case); flying, each pixel with its own alpha times the half's, nearest pixel (pixel art).
pub fn paint_piece(c: &mut crate::gfx::Canvas, half: &Pic, p: Piece, (x, y, w, h): (i32, i32, i32, i32)) {
    if p.alpha == 0 {
        return;
    }
    let k = w as f32 / half.w.max(1) as f32;
    let (dx, dy) = (p.dx * h as f32, p.dy * h as f32);
    if p.turn == 0.0 && p.scale == 1.0 && (k - 1.0).abs() < 1e-3 && p.alpha == 255 {
        crate::cabinet_art::blit(c, half, x + dx.round() as i32, y + dy.round() as i32, 256, (0, c.h));
        return;
    }
    let scale = k * p.scale;
    let (cx, cy) = (x as f32 + w as f32 / 2.0 + dx, y as f32 + h as f32 / 2.0 + dy);
    let (sin, cos) = p.turn.sin_cos();
    let (pcx, pcy) = (half.w as f32 / 2.0, half.h as f32 / 2.0);
    let (hw, hh) = ((half.w as f32 * cos.abs() + half.h as f32 * sin.abs()) * scale / 2.0, (half.w as f32 * sin.abs() + half.h as f32 * cos.abs()) * scale / 2.0);
    let (x0, x1) = (((cx - hw).floor() as i32).max(0), ((cx + hw).ceil() as i32).min(c.w));
    let (y0, y1) = (((cy - hh).floor() as i32).max(0), ((cy + hh).ceil() as i32).min(c.h));
    let inv = 1.0 / scale;
    for yy in y0..y1 {
        let ry = yy as f32 + 0.5 - cy;
        for xx in x0..x1 {
            let rx = xx as f32 + 0.5 - cx;
            let sx = ((rx * cos + ry * sin) * inv + pcx).floor() as i32;
            let sy = ((-rx * sin + ry * cos) * inv + pcy).floor() as i32;
            if sx < 0 || sy < 0 || sx >= half.w || sy >= half.h {
                continue;
            }
            let v = half.px[(sy * half.w + sx) as usize];
            let a = (v >> 24) * p.alpha / 255;
            if a != 0 {
                c.blend(xx, yy, v & 0xff_ffff, a as u8);
            }
        }
    }
}

/// One half of the film at one moment of the tear ([`piece`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Piece {
    pub dx: f32,
    pub dy: f32,
    pub scale: f32,
    pub turn: f32,
    pub alpha: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn case(w: i32, h: i32, rgb: u32) -> Pic {
        // A case with a transparent margin round it, as the templates have.
        let mut p = Pic::new(w, h, 0);
        for y in 2..h - 2 {
            for x in 2..w - 2 {
                p.px[(y * w + x) as usize] = 0xff00_0000 | rgb;
            }
        }
        p
    }

    #[test]
    fn a_flat_cover_and_a_case_keep_exactly_the_same_film_pixels() {
        for (w, h) in [(17, 21), (90, 130)] {
            let img = Image { w, h, px: (0..w * h).map(|i| i.wrapping_mul(0x13579) & 0xffffff).collect() };
            for unit in [1.5, 3.0] {
                for seed in [0, 5, 18, 19] {
                    let mut pic = Pic::of(w as i32, h as i32, img.px.iter().map(|p| p | 0xff00_0000).collect());
                    wrap(&mut pic, unit, seed, Glare::Front);
                    let expected: Vec<_> = pic.px.iter().map(|p| p & 0xffffff).collect();
                    assert_eq!(wrap_image(&img, unit, seed).px, expected);
                }
            }
        }
    }

    #[test]
    fn fast_film_mix_is_pixel_exact_and_preserves_every_alpha() {
        for alpha in 0..=255u32 {
            for channel in 0..=255u32 {
                let rgb = (channel << 16) | ((255 - channel) << 8) | channel;
                assert_eq!(coated(0xff00_0000 | rgb, alpha as u8), 0xff00_0000 | mix(rgb, FILM, alpha));
                assert_eq!(coated(0x7f00_0000 | rgb, alpha as u8), 0x7f00_0000 | rgb);
            }
        }
    }

    #[test]
    fn a_batch_of_film_keeps_every_colour_alpha_and_partial_group_exact() {
        let mut pixels = Vec::new();
        let mut mask = Vec::new();
        for alpha in 0..=255u32 {
            for channel in 0..=255u32 {
                let rgb = (channel << 16) | ((255 - channel) << 8) | channel;
                // Adjacent pixels have different coverage and film strength, including transparent and partly
                // transparent case edges. Blending the colour must never change the case's own alpha.
                for coverage in [0, 127, 128, 255] {
                    pixels.push((coverage << 24) | (rgb ^ (coverage * 0x010101)));
                    mask.push((alpha + coverage) as u8);
                }
            }
        }
        let expected: Vec<_> = pixels.iter().zip(&mask).map(|(p, a)| coated(*p, *a)).collect();
        // Cover all possible tails, including a slice too short for one group, and unaligned starting places.
        for skip in 0..4 {
            let mut actual = pixels[skip..].to_vec();
            coat_pixels(&mut actual, &mask[skip..]);
            assert_eq!(actual, expected[skip..]);
            for len in 0..8 {
                let mut short = pixels[skip..skip + len].to_vec();
                coat_pixels(&mut short, &mask[skip..skip + len]);
                assert_eq!(short, expected[skip..skip + len]);
            }
        }
    }

    #[test]
    fn film_follows_notched_cases_and_keeps_their_holes_clear() {
        // A row can span a cutout above that column's first solid pixel (a notched case or sprite).
        let shape = Pic::of(3, 3, vec![0xff334455, 0, 0xff334455, 0xff334455, 0, 0xff334455, 0xff334455, 0xff334455, 0xff334455]);
        let mut wrapped = shape.clone();
        wrap(&mut wrapped, 1.0, 5, Glare::Front);
        let film = layer(&shape, 1.0, 5, Glare::Front);
        for (i, src) in shape.px.iter().enumerate() {
            if *src == 0 { assert_eq!(wrapped.px[i], 0); assert_eq!(film.px[i], 0); }
            else { assert_ne!(wrapped.px[i], *src, "solid parts show the film"); }
        }
    }

    fn luma(p: u32) -> u32 {
        (((p >> 16) & 0xff) * 77 + ((p >> 8) & 0xff) * 150 + (p & 0xff) * 29) >> 8
    }

    #[test]
    fn the_film_comes_from_the_photo_and_shows_on_every_game() {
        let g = glow();
        let peak = g.profile.iter().position(|v| *v == 255).expect("the photo's glare reaches white");
        // The photo's band: a sharp edge on one side (white within a few steps), a soft fade on the other.
        assert!(peak < 8 && g.profile[255] < 80 && g.profile[96] > 60, "the band starts at its sharp white edge and fades out softly: {:?}", &g.profile[..]);
        // Every game shows a clear glare band on a dark case, and most of the case stays clear.
        for seed in (0..400u64).map(|i| i.wrapping_mul(0x9e37_79b9_7f4a_7c15)) {
            let base = case(90, 130, 0x101010);
            let mut p = base.clone();
            wrap(&mut p, 1.5, seed, Glare::Front);
            let solid = p.px.iter().filter(|px| *px >> 24 != 0).count();
            let bright = p.px.iter().filter(|px| *px >> 24 != 0 && luma(**px) > 90).count();
            let clear = p.px.iter().zip(&base.px).filter(|(a, b)| *a >> 24 != 0 && luma(**a) <= luma(**b) + 12).count();
            assert!(bright * 25 >= solid, "seed {seed}: the glare shows on at least 4 % ({bright} of {solid})");
            assert!(clear * 2 >= solid, "seed {seed}: at least half the case is as clear as glass ({clear} of {solid})");
        }
    }

    #[test]
    fn the_film_keeps_the_cases_outline_and_lightens_it_never_darkens() {
        let base = case(60, 80, 0x2a4a8a);
        let mut p = base.clone();
        wrap(&mut p, 2.0, 7, Glare::Front);
        for (a, b) in base.px.iter().zip(&p.px) {
            assert_eq!(a >> 24, b >> 24, "see-through stays see-through, solid stays solid");
            if a >> 24 != 0 {
                assert!(luma(*b) >= luma(*a), "the film is light over the case");
            }
        }
        // The rim is lighter than the case under it, and so is the seam near the top.
        let rim = p.px[(40 * 60 + 2) as usize];
        assert!(luma(rim) > luma(base.px[(40 * 60 + 2) as usize]) + 20, "the film's edge catches the light");
        let seam_lit = (3..12).any(|y| luma(p.px[(y * 60 + 30) as usize]) > luma(base.px[(y * 60 + 30) as usize]) + 15);
        assert!(seam_lit, "the sealed seam shows near the top");
    }

    #[test]
    fn the_film_alone_blitted_over_the_case_is_the_wrapped_case() {
        // The tear lays the film over the unwrapped case: on its first frame nothing may jump.
        let base = case(50, 70, 0x8a2a2a);
        let mut wrapped = base.clone();
        wrap(&mut wrapped, 1.0, 99, Glare::Front);
        let film = layer(&base, 1.0, 99, Glare::Front);
        for i in 0..base.px.len() {
            let (d, f) = (base.px[i], film.px[i]);
            let a = f >> 24;
            let blitted = if a == 0 { d } else { (d & 0xff00_0000) | mix(d & 0xff_ffff, f & 0xff_ffff, a) };
            assert_eq!(blitted, wrapped.px[i]);
        }
    }

    #[test]
    fn the_halves_are_the_whole_film_and_each_has_some() {
        let film = layer(&case(80, 110, 0), 1.0, 3, Glare::Front);
        let (a, b) = halves(&film, 3);
        let mut ca = 0;
        let mut cb = 0;
        for i in 0..film.px.len() {
            assert!(a.px[i] == 0 || b.px[i] == 0, "a pixel is in one half only");
            assert_eq!(a.px[i] | b.px[i], film.px[i]);
            ca += usize::from(a.px[i] >> 24 != 0);
            cb += usize::from(b.px[i] >> 24 != 0);
        }
        assert!(ca > film.px.len() / 5 && cb > film.px.len() / 5, "both halves are big pieces: {ca} {cb}");
    }

    #[test]
    fn the_film_sits_until_the_rip_then_flies_off_and_is_gone_at_the_end() {
        for first in [true, false] {
            let start = piece(0.0, first);
            assert_eq!((start.dx, start.dy, start.scale, start.turn, start.alpha), (0.0, 0.0, 1.0, 0.0, 255), "on the first frame the film is exactly where the wrapped case had it");
            let before = piece(RIP.as_secs_f32() - 0.01, first);
            assert_eq!(before.alpha, 255);
            assert!(before.dx.abs() < 0.02, "before the rip it only tugs");
            let after = piece(RIP.as_secs_f32() + 0.12, first);
            assert!(after.dx.abs() > 0.1, "after the rip it flies");
            assert_eq!(piece(TEAR.as_secs_f32(), first).alpha, 0, "gone at the end");
        }
        assert!(piece(0.4, true).dx < 0.0 && piece(0.4, false).dx > 0.0, "the halves go apart");
        assert!(LEAD > RIP && LEAD < TEAR, "the launch's own scene starts after the rip, while the pieces still fly");
    }

    /// The cost of a case's film, made once where its picture is made: `cargo test --lib shrink_wrap::tests::cost --
    /// --ignored --nocapture` (shelf spine, Shelf front at 640x480, the game box's front at 1080 lines, a Split cover).
    #[test]
    #[ignore]
    fn cost() {
        for (what, w, h, unit) in [("spine 640", 24, 150, 1.5), ("front 640", 150, 200, 1.5), ("box front 1080", 420, 600, 4.0), ("cover 1080", 560, 560, 4.7)] {
            let base = case(w, h, 0x334455);
            let t = std::time::Instant::now();
            let n = 50;
            for i in 0..n {
                let mut p = base.clone();
                wrap(&mut p, unit, i, if w < 40 { Glare::Spine } else { Glare::Front });
                std::hint::black_box(&p);
            }
            println!("shrink-wrap cost {what} {w}x{h}: {:.3} ms", t.elapsed().as_secs_f64() * 1000.0 / n as f64);
        }
    }

    #[test]
    fn a_flying_piece_is_as_see_through_as_the_film_never_a_white_sheet() {
        // 13.11. first try: the pieces were painted solid white over the case.
        let film = layer(&case(40, 50, 0), 1.0, 5, Glare::Front);
        let (a, _) = halves(&film, 5);
        for secs in [0.0, 0.1, 0.3, 0.45] {
            let mut buf = vec![0x0020_2020u32; 120 * 120];
            let mut c = crate::gfx::Canvas::new(&mut buf, 120, 120);
            paint_piece(&mut c, &a, piece(secs, true), (40, 35, 40, 50));
            let white = buf.iter().filter(|p| ((*p >> 16) & 0xff) > 0xe0).count();
            let touched = buf.iter().filter(|p| **p != 0x0020_2020).count();
            assert!(touched > 0, "{secs}: the piece is there");
            assert!(white * 4 < touched, "{secs}: mostly see-through film, not a white sheet ({white} of {touched})");
        }
    }

    #[test]
    fn the_sound_cracks_when_the_film_rips() {
        use crate::ui_sound::{hit_at, is_case_sound, Sound};
        assert_eq!(hit_at(Sound::WrapTear), RIP, "the tear's sound starts with the tear and cracks at the rip");
        assert!(is_case_sound(Sound::WrapTear), "Case sounds off keeps it quiet too");
    }
}
