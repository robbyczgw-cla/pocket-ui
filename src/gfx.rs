//! Minimal software canvas: filled shapes and an 8x8 bitmap font, drawn into a 0RGB u32 buffer.

use font8x8::legacy::BASIC_LEGACY;

pub const BG: u32 = 0x0d0d10;
pub const PANEL: u32 = 0x1c1c22;
pub const DIM: u32 = 0x5a5a66;
pub const FG: u32 = 0xf2f2f2;
pub const ACCENT: u32 = 0xffffff;
pub const ACTIVE: u32 = 0x3ddc84;

/// Linear mix of two 0RGB colors; `t` is 0–255 (0 = `a`, 255 = `b`).
pub fn mix(a: u32, b: u32, t: u32) -> u32 {
    let ch = |shift: u32| {
        let (x, y) = ((a >> shift) & 0xff, (b >> shift) & 0xff);
        ((x * (255 - t) + y * t) / 255) << shift
    };
    ch(16) | ch(8) | ch(0)
}

pub struct Canvas<'a> {
    pub buf: &'a mut [u32],
    pub w: i32,
    pub h: i32,
}

impl<'a> Canvas<'a> {
    pub fn new(buf: &'a mut [u32], w: u32, h: u32) -> Self {
        Self { buf, w: w as i32, h: h as i32 }
    }

    pub fn clear(&mut self, color: u32) {
        self.buf.fill(color);
    }

    /// Rows `y0..y1` as a canvas of their own (its y 0 is `y0`): what is painted into it is cut at those edges.
    /// A list that scrolls smoothly slides under its top and bottom edges this way.
    pub fn rows(&mut self, y0: i32, y1: i32) -> Canvas<'_> {
        let (y0, y1) = (y0.clamp(0, self.h), y1.clamp(0, self.h));
        let y1 = y1.max(y0);
        let w = self.w as usize;
        Canvas { buf: &mut self.buf[y0 as usize * w..y1 as usize * w], w: self.w, h: y1 - y0 }
    }

    pub fn rect(&mut self, x: i32, y: i32, w: i32, h: i32, color: u32) {
        // Everything outside the canvas is cut off, including a rectangle that starts past its right or bottom edge
        // (pages are laid out for taller screens: at 640x480 a "row below the fold" can be entirely off-screen).
        let (x0, x1) = (x.clamp(0, self.w), (x + w).clamp(0, self.w));
        let (y0, y1) = (y.clamp(0, self.h), (y + h).clamp(0, self.h));
        if x0 >= x1 || y0 >= y1 {
            return;
        }
        for row in y0..y1 {
            let start = (row * self.w) as usize;
            self.buf[start + x0 as usize..start + x1 as usize].fill(color);
        }
    }

    /// Mixes `color` over the pixel at (x, y) with coverage `alpha` (0–255).
    pub fn blend(&mut self, x: i32, y: i32, color: u32, alpha: u8) {
        if x < 0 || y < 0 || x >= self.w || y >= self.h {
            return;
        }
        let i = (y * self.w + x) as usize;
        self.buf[i] = mix(self.buf[i], color, alpha as u32);
    }

    /// Darkens the whole canvas towards black; `amount` 0 = unchanged, 255 = black.
    pub fn fade(&mut self, amount: u8) {
        // Nothing to fade (the launch's frames before its fade starts): not a pass over the whole screen.
        if amount == 0 {
            return;
        }
        for px in self.buf.iter_mut() {
            *px = mix(*px, 0, amount as u32);
        }
    }

    /// Darkens one rectangle towards black; `amount` 0 = unchanged, 255 = black.
    pub fn darken_rect(&mut self, x: i32, y: i32, w: i32, h: i32, amount: u8) {
        self.shade_rect(x, y, w, h, 0, amount);
    }

    /// Mixes one rectangle towards `toward` (the page colour: dims on a light page without going grey).
    pub fn shade_rect(&mut self, x: i32, y: i32, w: i32, h: i32, toward: u32, amount: u8) {
        for row in y.max(0)..(y + h).min(self.h) {
            for col in x.max(0)..(x + w).min(self.w) {
                let i = (row * self.w + col) as usize;
                self.buf[i] = mix(self.buf[i], toward, amount as u32);
            }
        }
    }

    pub fn frame(&mut self, x: i32, y: i32, w: i32, h: i32, t: i32, color: u32) {
        self.rect(x, y, w, t, color);
        self.rect(x, y + h - t, w, t, color);
        self.rect(x, y, t, h, color);
        self.rect(x + w - t, y, t, h, color);
    }

    pub fn disc(&mut self, cx: i32, cy: i32, r: i32, color: u32) {
        for dy in -r..=r {
            let half = ((r * r - dy * dy) as f32).sqrt() as i32;
            self.rect(cx - half, cy + dy, half * 2 + 1, 1, color);
        }
    }

    /// Filled rectangle with quarter-circle corners of radius `r`.
    pub fn round_rect(&mut self, x: i32, y: i32, w: i32, h: i32, r: i32, color: u32) {
        let r = r.min(w / 2).min(h / 2);
        self.rect(x, y + r, w, h - 2 * r, color);
        for dy in 0..r {
            // Distance from the corner circle's centre row decides how far each edge row is inset.
            let yy = r - dy;
            let inset = r - ((r * r - yy * yy) as f32).sqrt() as i32;
            self.rect(x + inset, y + dy, w - 2 * inset, 1, color);
            self.rect(x + inset, y + h - 1 - dy, w - 2 * inset, 1, color);
        }
    }

    pub fn ring(&mut self, cx: i32, cy: i32, r: i32, t: i32, color: u32) {
        let (outer, inner) = (r * r, (r - t) * (r - t));
        for dy in -r..=r {
            for dx in -r..=r {
                let d = dx * dx + dy * dy;
                if d <= outer && d >= inner {
                    self.rect(cx + dx, cy + dy, 1, 1, color);
                }
            }
        }
    }

    /// Nearest-neighbour blit of `img` into the rectangle (x, y, w, h); pixel art stays crisp.
    pub fn blit_scaled(&mut self, img: &crate::image::Image, x: i32, y: i32, w: i32, h: i32) {
        if img.w == 0 || img.h == 0 || w <= 0 || h <= 0 {
            return;
        }
        // The visible columns and their source columns, worked out once: no division per pixel (the pause menu
        // draws the whole screen this way).
        let (c0, c1) = (x.max(0), (x + w).min(self.w));
        if c0 >= c1 {
            return;
        }
        let cols: Vec<usize> = (c0..c1).map(|col| ((col - x) as u64 * img.w as u64 / w as u64) as usize).collect();
        let iw = img.w as usize;
        for dy in 0..h {
            let row = y + dy;
            if row < 0 || row >= self.h {
                continue;
            }
            let sy = (dy as u64 * img.h as u64 / h as u64) as usize;
            let src = &img.px[sy * iw..(sy + 1) * iw];
            let at = (row * self.w + c0) as usize;
            let dst = &mut self.buf[at..at + cols.len()];
            if img.w as i32 == w && c0 == x {
                // 1:1 across: a copy.
                dst.copy_from_slice(&src[..dst.len()]);
            } else {
                for (d, &sx) in dst.iter_mut().zip(&cols) {
                    *d = src[sx];
                }
            }
        }
    }

    /// One picture fading into another, each at its own place (x, y) and 1:1 (pictures are made in the size they
    /// are drawn): `t` (0-255) of `to`, the rest of `from`; where a picture is not, `bg` shows (the pictures differ
    /// in shape, and the box one leaves behind must not stay). Both `None`: nothing is drawn.
    pub fn crossfade(&mut self, from: Option<(&crate::image::Image, i32, i32)>, to: Option<(&crate::image::Image, i32, i32)>, bg: u32, t: u32) {
        let rect = |p: Option<(&crate::image::Image, i32, i32)>| p.map(|(i, x, y)| (x, y, x + i.w as i32, y + i.h as i32));
        let (a, b) = (rect(from), rect(to));
        let Some((x0, y0, x1, y1)) = [a, b].into_iter().flatten().reduce(|p, q| (p.0.min(q.0), p.1.min(q.1), p.2.max(q.2), p.3.max(q.3))) else { return };
        let at = |p: Option<(&crate::image::Image, i32, i32)>, x: i32, y: i32| match p {
            Some((img, ix, iy)) if x >= ix && y >= iy && x < ix + img.w as i32 && y < iy + img.h as i32 => img.px[((y - iy) as u32 * img.w + (x - ix) as u32) as usize],
            _ => bg,
        };
        for y in y0.max(0)..y1.min(self.h) {
            for x in x0.max(0)..x1.min(self.w) {
                self.buf[(y * self.w + x) as usize] = mix(at(from, x, y), at(to, x, y), t);
            }
        }
    }

    /// 1:1 blit of a sprite with binary transparency at (x, y); `dim` darkens it towards black (0 = as is).
    pub fn blit_sprite(&mut self, s: &crate::system_art::Sprite, x: i32, y: i32, dim: u8) {
        self.blit_sprite_to(s, x, y, dim, 0);
    }

    /// `blit_sprite`, dimmed towards `toward` instead of black.
    pub fn blit_sprite_to(&mut self, s: &crate::system_art::Sprite, x: i32, y: i32, dim: u8, toward: u32) {
        for sy in 0..s.h as i32 {
            let row = y + sy;
            if row < 0 || row >= self.h {
                continue;
            }
            for sx in 0..s.w as i32 {
                let col = x + sx;
                let p = s.px[(sy * s.w as i32 + sx) as usize];
                if col < 0 || col >= self.w || p >> 24 == 0 {
                    continue;
                }
                let rgb = p & 0x00ff_ffff;
                self.buf[(row * self.w + col) as usize] = if dim == 0 { rgb } else { mix(rgb, toward, dim as u32) };
            }
        }
    }

    /// Draws ASCII text with the 8x8 font scaled by `scale`; returns the width drawn.
    pub fn text(&mut self, x: i32, y: i32, scale: i32, color: u32, s: &str) -> i32 {
        let mut cx = x;
        for ch in s.chars() {
            let glyph = BASIC_LEGACY.get(ch as usize).copied().unwrap_or(BASIC_LEGACY[b'?' as usize]);
            for (row, bits) in glyph.iter().enumerate() {
                for col in 0..8 {
                    if bits & (1 << col) != 0 {
                        self.rect(cx + col * scale, y + row as i32 * scale, scale, scale, color);
                    }
                }
            }
            cx += 8 * scale;
        }
        cx - x
    }

    pub fn text_centered(&mut self, cx: i32, y: i32, scale: i32, color: u32, s: &str) {
        let w = s.chars().count() as i32 * 8 * scale;
        self.text(cx - w / 2, y, scale, color, s);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image::Image;

    #[test]
    fn a_crossfade_between_two_shapes_leaves_nothing_of_the_old_one_behind() {
        let (tall, wide) = (Image { w: 4, h: 8, px: vec![0xff0000; 32] }, Image { w: 8, h: 4, px: vec![0x0000ff; 32] });
        let mut buf = vec![0x101010u32; 12 * 12];
        let mut c = Canvas::new(&mut buf, 12, 12);
        // Both centred on the same point: the tall one at (4,2), the wide one at (2,4).
        c.crossfade(Some((&tall, 4, 2)), Some((&wide, 2, 4)), 0x101010, 0);
        assert_eq!(c.buf[2 * 12 + 4], 0xff0000, "all of the old one at 0");
        assert_eq!(c.buf[0], 0x101010, "outside both: untouched");
        c.crossfade(Some((&tall, 4, 2)), Some((&wide, 2, 4)), 0x101010, 255);
        assert_eq!(c.buf[4 * 12 + 2], 0x0000ff, "all of the new one at 255");
        assert_eq!(c.buf[2 * 12 + 4], 0x101010, "where only the old one was: the background, not its pixels");
        c.crossfade(Some((&tall, 4, 2)), Some((&wide, 2, 4)), 0x101010, 128);
        let mid = c.buf[5 * 12 + 4];
        assert!(mid != 0xff0000 && mid != 0x0000ff, "halfway is a mix: {mid:06x}");
        // Nothing to draw.
        c.crossfade(None, None, 0, 100);
    }

    /// `blit_scaled` as it was: a division per pixel, the reference for the faster one.
    #[allow(clippy::too_many_arguments)]
    fn blit_reference(buf: &mut [u32], cw: i32, ch: i32, img: &Image, x: i32, y: i32, w: i32, h: i32) {
        for dy in 0..h {
            let row = y + dy;
            if row < 0 || row >= ch {
                continue;
            }
            let sy = (dy as u64 * img.h as u64 / h as u64) as usize;
            for dx in 0..w {
                let col = x + dx;
                if col < 0 || col >= cw {
                    continue;
                }
                let sx = (dx as u64 * img.w as u64 / w as u64) as usize;
                buf[(row * cw + col) as usize] = img.px[sy * img.w as usize + sx];
            }
        }
    }

    #[test]
    fn a_rectangle_past_the_edge_is_cut_off_and_never_slices_past_the_buffer() {
        let (w, h) = (4i32, 3i32);
        let mut buf = vec![0u32; (w * h) as usize];
        let mut c = Canvas::new(&mut buf, w as u32, h as u32);
        // Starts past the right edge and past the bottom edge: nothing, and no panic (640x480 pages draw rows and
        // panels that a taller screen would show).
        c.rect(10, 1, 5, 1, 0xffffff);
        c.rect(1, 10, 2, 5, 0xffffff);
        c.rect(10, 10, 5, 5, 0xffffff);
        assert!(c.buf.iter().all(|&px| px == 0), "nothing was in the picture");
        // Entirely to the left or above: clipped to nothing, not to a negative index.
        c.rect(-10, -10, 5, 5, 0xffffff);
        // Reaches over the edges: only the part inside.
        c.rect(2, 1, 10, 10, 0xabcdef);
        let at = |x: i32, y: i32| c.buf[(y * w + x) as usize];
        assert_eq!(at(2, 1), 0xabcdef);
        assert_eq!(at(3, 2), 0xabcdef);
        assert_eq!(at(1, 1), 0, "left of it");
        assert_eq!(at(3, 0), 0, "above it");
        assert_eq!(at(0, 0), 0);
    }

    #[test]
    fn the_scaled_blit_puts_every_pixel_where_it_did() {
        let img = Image { w: 7, h: 5, px: (0..35).map(|i| i * 0x030507 + 1).collect() };
        let (cw, ch) = (23, 17);
        for (x, y, w, h) in [(0, 0, 7, 5), (2, 3, 7, 5), (-3, -2, 7, 5), (0, 0, 23, 17), (-5, 4, 30, 9), (20, 15, 6, 6), (1, 1, 3, 2), (30, 0, 4, 4)] {
            let (mut a, mut b) = (vec![0u32; (cw * ch) as usize], vec![0u32; (cw * ch) as usize]);
            Canvas::new(&mut a, cw as u32, ch as u32).blit_scaled(&img, x, y, w, h);
            blit_reference(&mut b, cw, ch, &img, x, y, w, h);
            assert_eq!(a, b, "at {x},{y} size {w}x{h}");
        }
    }
}
