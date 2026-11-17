//! Minimal software canvas: filled shapes and an 8x8 bitmap font, drawn into a 0RGB u32 buffer.
//!
//! A canvas is laid out in design pixels (`w` x `h`, 1280x960 on a 640x480 panel) and paints into a buffer of screen
//! pixels. With `Canvas::new` the two are the same (`n` = 1: every test, every picture painted ahead, every screen at
//! or above 721 lines). `Canvas::screen` paints a design canvas `n` times the panel's size straight into the panel's
//! buffer (`n` = 2 on the H700, `POCKETUI_PAINT=native`, docs/dev/native-paint.md): edges are rounded to whole screen
//! pixels, shapes, pictures and text are averaged over the `n` x `n` design pixels of each screen pixel, as the 2:1
//! filter of `scaled_layout::ScaledFrame` averaged them before.

use font8x8::legacy::BASIC_LEGACY;

pub const BG: u32 = 0x0d0d10;
pub const PANEL: u32 = 0x1c1c22;
pub const DIM: u32 = 0x5a5a66;
pub const FG: u32 = 0xf2f2f2;
pub const ACCENT: u32 = 0xffffff;
pub const ACTIVE: u32 = 0x3ddc84;

/// Linear mix of two 0RGB colors; `t` is 0–255 (0 = `a`, 255 = `b`).
#[inline]
pub fn mix(a: u32, b: u32, t: u32) -> u32 {
    let ch = |shift: u32| {
        let (x, y) = ((a >> shift) & 0xff, (b >> shift) & 0xff);
        ((x * (255 - t) + y * t) / 255) << shift
    };
    ch(16) | ch(8) | ch(0)
}

/// The mean of four 0RGB pixels, rounded as `scaled_layout`'s 2:1 filter rounds it (bit for bit the same).
#[inline]
pub fn avg4(a: u32, b: u32, c: u32, d: u32) -> u32 {
    let rb = (a & 0xff00ff) + (b & 0xff00ff) + (c & 0xff00ff) + (d & 0xff00ff);
    let g = (a & 0x00ff00) + (b & 0x00ff00) + (c & 0x00ff00) + (d & 0x00ff00);
    (((rb + 0x020002) >> 2) & 0xff00ff) | (((g + 0x000200) >> 2) & 0x00ff00)
}

/// `color` over `bg` on `cover` of a screen pixel's four design pixels: what the 2:1 filter made of `cover` design
/// pixels of `color` next to `4 - cover` of `bg` (bit for bit).
#[inline]
fn cover4(bg: u32, color: u32, cover: u32) -> u32 {
    let k = 4 - cover;
    let rb = (bg & 0xff00ff) * k + (color & 0xff00ff) * cover;
    let g = (bg & 0x00ff00) * k + (color & 0x00ff00) * cover;
    (((rb + 0x020002) >> 2) & 0xff00ff) | (((g + 0x000200) >> 2) & 0x00ff00)
}

/// Where design pixel `v` lands on the screen at `n` design pixels per screen pixel: an edge between two design pixels
/// is rounded to the nearest edge between screen pixels, half up (both edges of a shape the same way, so an even size
/// keeps its size wherever it is).
#[inline]
fn edge(v: i32, n: i32) -> i32 {
    if n == 1 { v } else { (v + n / 2).div_euclid(n) }
}

thread_local! {
    static CLEARED: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// The pixels [`Canvas::clear`] filled on this thread since the last call (and starts counting again): what a frame
/// clears. A frame that cleared its whole page twice cost the Brick a millisecond for nothing
/// (docs/TRIMUI-BRICK.md); the Cabinet's snapshot line says it (`cleared=`).
pub fn take_cleared() -> u64 {
    CLEARED.with(|n| n.replace(0))
}

pub struct Canvas<'a> {
    /// Screen pixels: the whole rows of `stride` pixels this canvas reaches. Private: code outside `gfx` asks
    /// `pixels`/`design` (docs/dev/native-paint.md "Direct access").
    buf: &'a mut [u32],
    /// Design pixels, as the pages lay out (1280x960 on a 640x480 panel).
    pub w: i32,
    pub h: i32,
    /// Design pixels per screen pixel (1 or 2).
    n: i32,
    /// Screen pixels per row of `buf`.
    stride: usize,
    /// Design row of the whole picture this canvas's row 0 is (`rows` keeps it, so a band rounds as the screen does);
    /// always 0 at `n` = 1, where `buf` starts at row 0 itself.
    oy: i32,
    /// Screen row of the whole picture `buf` starts at (`n` > 1).
    sy0: i32,
    /// Where this canvas's design row 0 is, as an address of design pixels: the text checks tell canvases and bands
    /// apart by it (`text::boxes`). At `n` = 1 it is `buf`'s own address, as it always was.
    id: usize,
}

impl<'a> Canvas<'a> {
    pub fn new(buf: &'a mut [u32], w: u32, h: u32) -> Self {
        let id = buf.as_ptr() as usize;
        Self { buf, w: w as i32, h: h as i32, n: 1, stride: w as usize, oy: 0, sy0: 0, id }
    }

    /// The screen: `buf` holds `panel` (screen pixels), the canvas is `n` times as large in design pixels. `n` = 1
    /// is `Canvas::new`.
    pub fn screen(buf: &'a mut [u32], panel: (u32, u32), n: u32) -> Self {
        assert!(n == 1 || n == 2, "design pixels per screen pixel: 1 or 2, not {n}");
        assert_eq!(buf.len(), panel.0 as usize * panel.1 as usize, "a screen buffer of {}x{}", panel.0, panel.1);
        let id = buf.as_ptr() as usize;
        let n = n as i32;
        Self { buf, w: panel.0 as i32 * n, h: panel.1 as i32 * n, n, stride: panel.0 as usize, oy: 0, sy0: 0, id }
    }

    /// Design pixels per screen pixel: 1, or 2 on a 640x480 panel painted natively.
    pub fn n(&self) -> i32 {
        self.n
    }

    /// This canvas told apart from others (and where its row 0 is) for the text checks (`text::boxes`): at `n` = 1
    /// the address of its first pixel, as `buf.as_ptr()` was.
    pub fn id(&self) -> usize {
        self.id
    }

    /// Screen pixels per row of `pixels` (= `w` at `n` = 1).
    pub fn stride(&self) -> usize {
        self.stride
    }

    /// The screen pixels of this canvas, row by row (`stride` each). At `n` = 1 these are the design pixels, as the
    /// old public `buf` was: reads and writes that index them by design coordinates are only right there; for
    /// code not yet painted natively see `design`.
    pub fn pixels(&self) -> &[u32] {
        self.buf
    }

    pub fn pixels_mut(&mut self) -> &mut [u32] {
        self.buf
    }

    /// The screen row (of `pixels`) design row `y` of this canvas starts in, rounded as edges are; may lie outside.
    pub fn screen_y(&self, y: i32) -> i32 {
        edge(self.oy + y, self.n) - self.sy0
    }

    /// The screen column design column `x` starts in, rounded as edges are; may lie outside.
    pub fn screen_x(&self, x: i32) -> i32 {
        edge(x, self.n)
    }

    /// The colour shown at design pixel (x, y), black outside the canvas.
    pub fn get(&self, x: i32, y: i32) -> u32 {
        if x < 0 || y < 0 || x >= self.w || y >= self.h {
            return 0;
        }
        if self.n == 1 {
            return self.buf[(y * self.w + x) as usize];
        }
        let rows = (self.buf.len() / self.stride.max(1)) as i32;
        let sy = ((self.oy + y).div_euclid(self.n) - self.sy0).clamp(0, rows - 1);
        let sx = (x / self.n).min(self.stride as i32 - 1);
        if rows <= 0 || sx < 0 {
            return 0;
        }
        self.buf[sy as usize * self.stride + sx as usize]
    }

    /// The design pixels of this canvas as a canvas of their own at `n` = 1, for code that reads or writes the
    /// buffer itself by design coordinates (a band copied, a picture read back). At `n` = 1 that is this canvas;
    /// at `n` = 2 the screen is enlarged into a design-size copy, and what was painted there is averaged back
    /// into the screen when the guard goes (exactly the old 2:1 filter on that part): right, but two passes over
    /// the canvas each time. Packages 2–4 of docs/dev/native-paint.md replace the callers with native code.
    #[track_caller]
    pub fn design(&mut self) -> Design<'_, 'a> {
        let (w, h) = (self.w, self.h);
        self.design_area(0, 0, w, h)
    }

    /// `design` for code that touches only the design rectangle (x, y, w, h): only that part is enlarged and put
    /// back (the cost is its area). What the copy shows outside it is not the picture, and what is painted there
    /// is lost.
    #[track_caller]
    pub fn design_area(&mut self, x: i32, y: i32, w: i32, h: i32) -> Design<'_, 'a> {
        if self.n == 1 {
            return Design { c: self, scratch: Vec::new(), area: (0, 0, 0, 0) };
        }
        let n = self.n;
        let rows = (self.buf.len() / self.stride.max(1)) as i32;
        // The screen pixels holding any of the rectangle's design pixels.
        let (x0, x1) = (x.clamp(0, self.w), x.saturating_add(w).clamp(0, self.w));
        let (y0, y1) = (y.clamp(0, self.h), y.saturating_add(h).clamp(0, self.h));
        let (sc0, sc1) = (x0.div_euclid(n) as usize, ((x1 + n - 1).div_euclid(n) as usize).min(self.stride));
        let sr0 = ((self.oy + y0).div_euclid(n) - self.sy0).clamp(0, rows) as usize;
        let sr1 = ((self.oy + y1 + n - 1).div_euclid(n) - self.sy0).clamp(0, rows) as usize;
        DESIGN_PASSES.with(|d| d.set(d.get() + 1));
        let at = std::panic::Location::caller();
        DESIGN_SITES.with(|s| {
            let mut s = s.borrow_mut();
            let e = s.entry((at.file(), at.line())).or_insert((0, 0));
            *e = (e.0 + 1, e.1 + (sc1.saturating_sub(sc0) * sr1.saturating_sub(sr0)) as u64);
        });
        let mut scratch = SCRATCH.with(|s| std::mem::take(&mut *s.borrow_mut()));
        let (dw, dh) = (self.w as usize, self.h.max(0) as usize);
        // Grown once, never shrunk: a band's guard after a whole canvas's (and back) must not clear megabytes each time.
        if scratch.len() < dw * dh {
            scratch.resize(dw * dh, 0);
        }
        if sc0 < sc1 && sr0 < sr1 {
            // Every design row of those screen rows (the first one of a band that starts half way into a screen
            // row is that row too).
            let top = ((self.sy0 + sr0 as i32) * n - self.oy).max(0) as usize;
            let bottom = (((self.sy0 + sr1 as i32) * n - self.oy).max(0) as usize).min(dh);
            let (x0, x1) = (sc0 * 2, (sc1 * 2).min(dw));
            let mut last: Option<(usize, usize)> = None;
            for yy in top..bottom {
                let sy = ((self.oy + yy as i32).div_euclid(n) - self.sy0).clamp(0, rows - 1) as usize;
                match last {
                    // The second design row of the same screen row: the first one again.
                    Some((prev, at)) if at == sy => scratch.copy_within(prev * dw + x0..prev * dw + x1, yy * dw + x0),
                    _ => {
                        let src = &self.buf[sy * self.stride + sc0..sy * self.stride + sc1];
                        for (pair, &p) in scratch[yy * dw + x0..yy * dw + x1].chunks_exact_mut(2).zip(src) {
                            pair[0] = p;
                            pair[1] = p;
                        }
                    }
                }
                last = Some((yy, sy));
            }
        }
        Design { c: self, scratch, area: (sc0, sc1.max(sc0), sr0, sr1.max(sr0)) }
    }

    pub fn clear(&mut self, color: u32) {
        self.buf.fill(color);
        CLEARED.with(|n| n.set(n.get() + self.buf.len() as u64));
    }

    /// Rows `y0..y1` as a canvas of their own (its y 0 is `y0`): what is painted into it is cut at those edges.
    /// A list that scrolls smoothly slides under its top and bottom edges this way.
    pub fn rows(&mut self, y0: i32, y1: i32) -> Canvas<'_> {
        let (y0, y1) = (y0.clamp(0, self.h), y1.clamp(0, self.h));
        let y1 = y1.max(y0);
        if self.n == 1 {
            let w = self.w as usize;
            // The id moves with the band as its address did (a band of `design`'s copy keeps the screen's).
            let id = self.id + y0 as usize * w * 4;
            return Canvas { buf: &mut self.buf[y0 as usize * w..y1 as usize * w], w: self.w, h: y1 - y0, n: 1, stride: w, oy: 0, sy0: 0, id };
        }
        let rows = (self.buf.len() / self.stride) as i32;
        let (s0, s1) = ((edge(self.oy + y0, self.n) - self.sy0).clamp(0, rows), (edge(self.oy + y1, self.n) - self.sy0).clamp(0, rows));
        let s1 = s1.max(s0);
        let (stride, n, w) = (self.stride, self.n, self.w);
        let (oy, sy0, id) = (self.oy + y0, self.sy0 + s0, self.id + (y0 as usize) * (w as usize) * 4);
        Canvas { buf: &mut self.buf[s0 as usize * stride..s1 as usize * stride], w, h: y1 - y0, n, stride, oy, sy0, id }
    }

    /// The screen span of the design span `a..b` (already cut to the canvas) along one axis with design offset
    /// `off` and screen offset `soff`, `limit` screen pixels long: edges rounded, and a span that would round to
    /// nothing keeps the one screen pixel it starts in (a hairline stays a line).
    #[inline]
    fn span(&self, a: i32, b: i32, off: i32, soff: i32, limit: i32) -> (i32, i32) {
        let n = self.n;
        let (mut s0, mut s1) = (edge(off + a, n) - soff, edge(off + b, n) - soff);
        if s0 >= s1 && a < b {
            s0 = (off + a).div_euclid(n) - soff;
            s1 = s0 + 1;
        }
        (s0.clamp(0, limit), s1.clamp(0, limit))
    }

    /// The screen rectangle (columns, rows) of the design rectangle, cut to the canvas; `None` when nothing is left.
    #[inline]
    fn screen_rect(&self, x: i32, y: i32, w: i32, h: i32) -> Option<(usize, usize, usize, usize)> {
        let (x0, x1) = (x.clamp(0, self.w), (x.saturating_add(w)).clamp(0, self.w));
        let (y0, y1) = (y.clamp(0, self.h), (y.saturating_add(h)).clamp(0, self.h));
        if x0 >= x1 || y0 >= y1 {
            return None;
        }
        let rows = (self.buf.len() / self.stride.max(1)) as i32;
        let (c0, c1) = self.span(x0, x1, 0, 0, self.stride as i32);
        let (r0, r1) = self.span(y0, y1, self.oy, self.sy0, rows);
        (c0 < c1 && r0 < r1).then_some((c0 as usize, c1 as usize, r0 as usize, r1 as usize))
    }

    pub fn rect(&mut self, x: i32, y: i32, w: i32, h: i32, color: u32) {
        if self.n != 1 {
            let Some((c0, c1, r0, r1)) = self.screen_rect(x, y, w, h) else { return };
            for row in r0..r1 {
                self.buf[row * self.stride + c0..row * self.stride + c1].fill(color);
            }
            return;
        }
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

    /// Mixes `color` over the pixel at (x, y) with coverage `alpha` (0–255). At `n` = 2 a design pixel is a quarter
    /// of a screen pixel: the one at the top left of each screen pixel stands for it (as `rect` rounds a single
    /// pixel), the other three leave it alone, so a shape blended pixel by pixel keeps its colour.
    pub fn blend(&mut self, x: i32, y: i32, color: u32, alpha: u8) {
        if x < 0 || y < 0 || x >= self.w || y >= self.h {
            return;
        }
        if self.n != 1 {
            let (ax, ay) = (x, self.oy + y);
            if ax.rem_euclid(self.n) != 0 || ay.rem_euclid(self.n) != 0 {
                return;
            }
            let (sx, sy) = (ax / self.n, ay.div_euclid(self.n) - self.sy0);
            let rows = (self.buf.len() / self.stride) as i32;
            if sy < 0 || sy >= rows || sx as usize >= self.stride {
                return;
            }
            let i = sy as usize * self.stride + sx as usize;
            self.buf[i] = mix(self.buf[i], color, alpha as u32);
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
    ///
    /// At `n` > 1 a screen pixel the rectangle covers only in part is mixed by the share of its design pixels it covers
    /// (the mean the old filter made of mixed and unmixed ones), so a 1 px shaded line looks as before.
    pub fn shade_rect(&mut self, x: i32, y: i32, w: i32, h: i32, toward: u32, amount: u8) {
        self.shade_band(x, y, w, h, toward, &|_| amount);
    }

    /// Rows `y`, `y + 1`, … mixed towards `toward`, row `k` by `amounts[k]`: a soft edge made of 1 px rows (text running
    /// under the top or bottom of a scrolled view fades out). At `n` = 1 each row is `shade_rect(x, y + k, w, 1, …)`. At
    /// `n` > 1 a screen row is mixed by the mean of its design rows' amounts, which is the old filter's picture: one
    /// `shade_rect` per row would mix a screen row twice where two design rows land on it, twice as dark.
    pub fn shade_rows(&mut self, x: i32, y: i32, w: i32, toward: u32, amounts: &[u8]) {
        self.shade_band(x, y, w, amounts.len() as i32, toward, &|k| amounts[k as usize]);
    }

    fn shade_band(&mut self, x: i32, y: i32, w: i32, h: i32, toward: u32, amount: &dyn Fn(i32) -> u8) {
        if self.n == 1 {
            for row in y.max(0)..(y + h).min(self.h) {
                let t = amount(row - y) as u32;
                for col in x.max(0)..(x + w).min(self.w) {
                    let i = (row * self.w + col) as usize;
                    self.buf[i] = mix(self.buf[i], toward, t);
                }
            }
            return;
        }
        let (x0, x1) = (x.clamp(0, self.w), (x.saturating_add(w)).clamp(0, self.w));
        let (y0, y1) = (y.clamp(0, self.h), (y.saturating_add(h)).clamp(0, self.h));
        if x0 >= x1 || y0 >= y1 {
            return;
        }
        let n = self.n;
        let rows = (self.buf.len() / self.stride.max(1)) as i32;
        // Screen row r of this buffer shows design rows (r + sy0) * n - oy and on.
        let (a0, a1) = (self.oy + y0, self.oy + y1);
        let (r0, r1) = ((a0.div_euclid(n) - self.sy0).max(0), ((a1 - 1).div_euclid(n) + 1 - self.sy0).min(rows));
        let (c0, c1) = (x0 / n, ((x1 - 1) / n + 1).min(self.stride as i32));
        if c0 >= c1 {
            return;
        }
        let part = |a: i32, lo: i32, hi: i32| (((a + 1) * n).min(hi) - (a * n).max(lo)).max(0) as u32;
        let whole = (n * n) as u32;
        let (first_x, last_x) = (part(c0, x0, x1), part(c1 - 1, x0, x1));
        for r in r0..r1 {
            // The sum of the amounts of the design rows this screen row shows (rows outside the band: 0).
            let top = ((r + self.sy0) * n).max(a0);
            let bottom = ((r + self.sy0 + 1) * n).min(a1);
            let sum: u32 = (top..bottom).map(|a| amount(a - self.oy - y) as u32).sum();
            if sum == 0 {
                continue;
            }
            let t = |cols: u32| (sum * cols + whole / 2) / whole;
            let (first, last, inner) = (t(first_x), t(last_x), t(n as u32));
            let row = &mut self.buf[r as usize * self.stride..];
            for col in c0..c1 {
                let k = if col == c0 { first } else if col == c1 - 1 { last } else { inner };
                row[col as usize] = mix(row[col as usize], toward, k);
            }
        }
    }

    /// A border `t` thick inside the rectangle. At `n` = 2 each side is rounded as `rect` rounds it, and a side that
    /// would round to nothing keeps the screen pixel its line lies in (the hairline rule); the top and bottom reach
    /// across to the sides, so the frame is closed at all four corners (sides painted as four separate rectangles
    /// round a 1 px top one way and the sides another, and leave a corner open at odd places).
    pub fn frame(&mut self, x: i32, y: i32, w: i32, h: i32, t: i32, color: u32) {
        if self.n != 1 {
            if w <= 0 || h <= 0 || t <= 0 {
                return;
            }
            let n = self.n;
            let rows = (self.buf.len() / self.stride.max(1)) as i32;
            // Each side as `rect` rounds it; a side that rounds to nothing keeps the screen pixel its outer design
            // pixel lies in (the hairline rule). `near`: the side at `v`, `far`: the one ending at `v`, `t` thick.
            let near = |v: i32, t: i32| {
                let (a, b) = (edge(v, n), edge(v + t, n));
                if b > a { (a, b) } else { (v.div_euclid(n), v.div_euclid(n) + 1) }
            };
            let far = |v: i32, t: i32| {
                let (a, b) = (edge(v - t, n), edge(v, n));
                if b > a { (a, b) } else { ((v - 1).div_euclid(n), (v - 1).div_euclid(n) + 1) }
            };
            let (t, oy, sy0) = (t.min(w).min(h), self.oy, self.sy0);
            let ((l, li), (ri, r)) = (near(x, t), far(x + w, t));
            let ((top, ti), (bi, bottom)) = (near(oy + y, t), far(oy + y + h, t));
            let (top, ti, bi, bottom) = (top - sy0, ti - sy0, bi - sy0, bottom - sy0);
            let (stride, w_px) = (self.stride, self.stride as i32);
            let mut fill = |c0: i32, c1: i32, r0: i32, r1: i32| {
                let (c0, c1, r0, r1) = (c0.clamp(0, w_px), c1.clamp(0, w_px), r0.clamp(0, rows), r1.clamp(0, rows));
                for rr in r0..r1 {
                    if c0 < c1 {
                        self.buf[rr as usize * stride + c0 as usize..rr as usize * stride + c1 as usize].fill(color);
                    }
                }
            };
            fill(l, r, top, ti);
            fill(l, r, bi, bottom);
            fill(l, li, top, bottom);
            fill(ri, r, top, bottom);
            return;
        }
        self.rect(x, y, w, t, color);
        self.rect(x, y + h - t, w, t, color);
        self.rect(x, y, t, h, color);
        self.rect(x + w - t, y, t, h, color);
    }

    /// At `n` > 1: a shape given as spans of design pixels per design row (`row(y, push)` pushes `(x0, x1)` spans of
    /// row `y`, apart from each other), painted as the 2:1 filter painted it: each screen pixel `color` over what is
    /// there by how many of its design pixels the shape covers.
    fn spans(&mut self, y0: i32, y1: i32, color: u32, row: &mut dyn FnMut(i32, &mut dyn FnMut(i32, i32))) {
        self.spans_at(y0, y1, color, 255, row);
    }

    /// `spans` mixed over what is there at `alpha` (255: `color` itself): each screen pixel the mean of its covered
    /// design pixels mixed at `alpha` and the uncovered ones as they are, as the filter made of a shape blended pixel
    /// by pixel.
    fn spans_at(&mut self, y0: i32, y1: i32, color: u32, alpha: u8, row: &mut dyn FnMut(i32, &mut dyn FnMut(i32, i32))) {
        let (y0, y1) = (y0.max(0), y1.min(self.h));
        if y0 >= y1 {
            return;
        }
        let n = self.n;
        let rows = (self.buf.len() / self.stride) as i32;
        let (w, stride) = (self.w, self.stride);
        let mut cover = vec![0u8; stride];
        // Screen rows that hold design rows y0..y1.
        let (s0, s1) = (((self.oy + y0).div_euclid(n) - self.sy0).max(0), (((self.oy + y1 - 1).div_euclid(n)) - self.sy0 + 1).min(rows));
        for sy in s0..s1 {
            let (mut lo, mut hi) = (stride, 0usize);
            let first = (self.sy0 + sy) * n - self.oy;
            for y in first..first + n {
                if y < y0 || y >= y1 {
                    continue;
                }
                row(y, &mut |a, b| {
                    let (a, b) = (a.max(0), b.min(w));
                    if a >= b {
                        return;
                    }
                    let (ca, cb) = ((a / n) as usize, ((b - 1) / n) as usize);
                    lo = lo.min(ca);
                    hi = hi.max(cb + 1);
                    if ca == cb {
                        cover[ca] += (b - a) as u8;
                        return;
                    }
                    cover[ca] += (n - a % n) as u8;
                    cover[cb] += (b - cb as i32 * n) as u8;
                    for c in &mut cover[ca + 1..cb] {
                        *c += n as u8;
                    }
                });
            }
            if lo >= hi {
                continue;
            }
            let full = (n * n) as u8;
            let line = &mut self.buf[sy as usize * stride..(sy as usize + 1) * stride];
            for (px, c) in line[lo..hi].iter_mut().zip(&mut cover[lo..hi]) {
                let ink = if alpha == 255 { color } else { mix(*px, color, alpha as u32) };
                match *c {
                    0 => {}
                    k if k >= full => *px = ink,
                    k => *px = cover4(*px, ink, k as u32),
                }
                *c = 0;
            }
        }
    }

    pub fn disc(&mut self, cx: i32, cy: i32, r: i32, color: u32) {
        if self.n != 1 {
            self.spans(cy - r, cy + r + 1, color, &mut |y, push| {
                let dy = y - cy;
                let half = ((r * r - dy * dy) as f32).sqrt() as i32;
                push(cx - half, cx + half + 1);
            });
            return;
        }
        for dy in -r..=r {
            let half = ((r * r - dy * dy) as f32).sqrt() as i32;
            self.rect(cx - half, cy + dy, half * 2 + 1, 1, color);
        }
    }

    /// Filled rectangle with quarter-circle corners of radius `r`.
    pub fn round_rect(&mut self, x: i32, y: i32, w: i32, h: i32, r: i32, color: u32) {
        let r = r.min(w / 2).min(h / 2);
        if self.n != 1 {
            if w <= 0 || h <= 0 {
                return;
            }
            if r <= 0 {
                self.rect(x, y + r, w, h - 2 * r, color);
                return;
            }
            self.spans(y, y + h, color, &mut |yy, push| {
                let dy = if yy < y + r { yy - y } else if yy >= y + h - r { y + h - 1 - yy } else { r };
                let inset = if dy >= r { 0 } else { r - ((r * r - (r - dy) * (r - dy)) as f32).sqrt() as i32 };
                push(x + inset, x + w - inset);
            });
            return;
        }
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
        if self.n != 1 {
            self.spans(cy - r, cy + r + 1, color, &mut |y, push| {
                let dy = y - cy;
                let mut start = None;
                for dx in -r..=r + 1 {
                    let d = dx * dx + dy * dy;
                    let on = dx <= r && d <= outer && d >= inner;
                    match (on, start) {
                        (true, None) => start = Some(dx),
                        (false, Some(s)) => {
                            push(cx + s, cx + dx);
                            start = None;
                        }
                        _ => {}
                    }
                }
            });
            return;
        }
        for dy in -r..=r {
            for dx in -r..=r {
                let d = dx * dx + dy * dy;
                if d <= outer && d >= inner {
                    self.rect(cx + dx, cy + dy, 1, 1, color);
                }
            }
        }
    }

    /// Rectangles (x, y, w, h) painted as one shape in `color`: a small shape built from rows, columns or squares
    /// (an arrow, a triangle, a check mark's strokes, an angle's discs as their rows) where the pieces overlap. At
    /// `n` = 1 each rectangle as `rect` paints it. At `n` = 2 their union once, each screen pixel `color` over what is
    /// there by how many of its four design pixels the union covers: the 2:1 filter's picture. Painted piece by piece
    /// at `n` = 2, every 1 px piece would keep a whole screen pixel (the hairline rule) and the shape would come
    /// out a pixel too thick at odd places.
    pub fn fill_rects(&mut self, rects: &[(i32, i32, i32, i32)], color: u32) {
        self.fill_rects_at(rects, color, 255);
    }

    /// `fill_rects` mixed over what is there at `alpha`: each design pixel of the union once (pieces that overlap are
    /// not mixed twice). At `n` = 1 the union's rows, pixel by pixel as `blend` mixes them.
    pub fn fill_rects_at(&mut self, rects: &[(i32, i32, i32, i32)], color: u32, alpha: u8) {
        let rects: Vec<(i32, i32, i32, i32)> = rects.iter().copied().filter(|r| r.2 > 0 && r.3 > 0).collect();
        if rects.is_empty() || alpha == 0 {
            return;
        }
        if self.n == 1 && alpha == 255 {
            for &(x, y, w, h) in &rects {
                self.rect(x, y, w, h, color);
            }
            return;
        }
        let y0 = rects.iter().map(|r| r.1).min().unwrap_or(0);
        let y1 = rects.iter().map(|r| r.1 + r.3).max().unwrap_or(0);
        let mut row_spans = Vec::new();
        let mut row = |y: i32, push: &mut dyn FnMut(i32, i32)| {
            row_spans.clear();
            row_spans.extend(rects.iter().filter(|r| y >= r.1 && y < r.1 + r.3).map(|r| (r.0, r.0 + r.2)));
            row_spans.sort_unstable();
            let mut open: Option<(i32, i32)> = None;
            for &(a, b) in row_spans.iter() {
                match open {
                    Some((s, e)) if a <= e => open = Some((s, e.max(b))),
                    Some((s, e)) => {
                        push(s, e);
                        open = Some((a, b));
                    }
                    None => open = Some((a, b)),
                }
            }
            if let Some((s, e)) = open {
                push(s, e);
            }
        };
        if self.n == 1 {
            for y in y0.max(0)..y1.min(self.h) {
                let mut spans = Vec::new();
                row(y, &mut |a, b| spans.push((a, b)));
                for (a, b) in spans {
                    for x in a..b {
                        self.blend(x, y, color, alpha);
                    }
                }
            }
            return;
        }
        self.spans_at(y0, y1, color, alpha, &mut row);
    }

    /// A shape blended pixel by pixel in `color` (anti-aliased marks, soft fills): `paint` gets a `put(x, y, alpha)`
    /// and calls it for each design pixel inside `area` (x, y, w, h); pixels outside it are left out. At `n` = 1
    /// every call is `blend`, in the same order. At `n` = 2 the calls fill a coverage mask of `area` (a pixel put
    /// twice counts as mixed twice) that is painted averaged per screen pixel (`mask`): the shape's soft edges as the
    /// 2:1 filter gave them, within a step of rounding, where `blend` alone keeps only every fourth design pixel.
    pub fn blend_with(&mut self, (x, y, w, h): (i32, i32, i32, i32), color: u32, paint: impl FnOnce(&mut dyn FnMut(i32, i32, u8))) {
        if w <= 0 || h <= 0 {
            return;
        }
        if self.n == 1 {
            paint(&mut |px, py, a| {
                if px >= x && px < x + w && py >= y && py < y + h {
                    self.blend(px, py, color, a);
                }
            });
            return;
        }
        let (mw, mh) = (w as usize, h as usize);
        let mut alpha = vec![0u8; mw * mh];
        paint(&mut |px, py, a| {
            if px >= x && px < x + w && py >= y && py < y + h {
                let at = &mut alpha[(py - y) as usize * mw + (px - x) as usize];
                // Mixed twice: what is left of the page under the first mix is mixed again.
                *at = (*at as u32 + a as u32 - *at as u32 * a as u32 / 255).min(255) as u8;
            }
        });
        self.mask(x, y, mw, mh, &alpha, color);
    }

    /// At `n` = 2: the screen pixels under the design rectangle `x0..x1`, `y0..y1` (cut to the canvas), each the mean
    /// of its four design pixels as the 2:1 filter made it: `sample(x, y)` where the rectangle has a pixel of its own
    /// (`None`: see-through), what the screen shows where it has not. The tool for painting a picture natively that
    /// is made at design size (the blits, the Shelf look's strip): the same picture as before, a quarter of the writes.
    /// Not for `n` = 1 (there the caller copies the pixels itself).
    #[inline]
    pub fn cells(&mut self, x0: i32, x1: i32, y0: i32, y1: i32, mut sample: impl FnMut(i32, i32) -> Option<u32>) {
        let (x0, x1, y0, y1) = (x0.max(0), x1.min(self.w), y0.max(0), y1.min(self.h));
        if x0 >= x1 || y0 >= y1 {
            return;
        }
        let rows = (self.buf.len() / self.stride) as i32;
        let (sc0, sc1) = ((x0 / 2) as usize, ((x1 - 1) / 2 + 1).min(self.stride as i32) as usize);
        let sr0 = ((self.oy + y0).div_euclid(2) - self.sy0).max(0);
        let sr1 = ((self.oy + y1 - 1).div_euclid(2) - self.sy0 + 1).min(rows);
        let stride = self.stride;
        for sr in sr0..sr1 {
            let top = (self.sy0 + sr) * 2 - self.oy;
            let (in0, in1) = (top >= y0 && top < y1, top + 1 >= y0 && top + 1 < y1);
            let line = &mut self.buf[sr as usize * stride..(sr as usize + 1) * stride];
            for (sc, px) in line.iter_mut().enumerate().take(sc1).skip(sc0) {
                let x = sc as i32 * 2;
                let (l, r) = (x >= x0 && x < x1, x + 1 >= x0 && x + 1 < x1);
                let bg = *px;
                let mut any = false;
                let mut at = |on: bool, xx: i32, yy: i32| match on.then(|| sample(xx, yy)).flatten() {
                    Some(p) => {
                        any = true;
                        p
                    }
                    None => bg,
                };
                let (a, b, c, d) = (at(in0 && l, x, top), at(in0 && r, x + 1, top), at(in1 && l, x, top + 1), at(in1 && r, x + 1, top + 1));
                if any {
                    *px = avg4(a, b, c, d);
                }
            }
        }
    }

    /// `cells` at any `n`: each design pixel of the rectangle `x0..x1`, `y0..y1` (cut to the canvas) set to
    /// `sample(x, y)` (`None`: left as it is) at `n` = 1, each screen pixel the mean of its four at `n` = 2. For
    /// pictures worked out pixel by pixel (a gradient, a turning face): the old picture either way.
    #[inline]
    pub fn per_pixel(&mut self, x0: i32, x1: i32, y0: i32, y1: i32, mut sample: impl FnMut(i32, i32) -> Option<u32>) {
        if self.n != 1 {
            self.cells(x0, x1, y0, y1, sample);
            return;
        }
        let (x0, x1, y0, y1) = (x0.max(0), x1.min(self.w), y0.max(0), y1.min(self.h));
        for y in y0..y1 {
            let line = &mut self.buf[(y * self.w) as usize..((y + 1) * self.w) as usize];
            for x in x0..x1 {
                if let Some(p) = sample(x, y) {
                    line[x as usize] = p;
                }
            }
        }
    }

    /// `px` (design pixels, `w` wide, whole rows) copied 1:1 to (x, y), cut to the canvas; a pixel equal to `see_through`
    /// leaves what is there. At `n` = 2 each screen pixel is the mean of its four (the 2:1 filter's picture, bit for
    /// bit), straight from the rows where the block sits on even design pixels. For pictures made at design size each
    /// frame (the Shelf look's strip) and the blitters of docs/dev/native-paint.md's package 2.
    pub fn copy_design(&mut self, x: i32, y: i32, w: usize, px: &[u32], see_through: Option<u32>) {
        if w == 0 || px.is_empty() {
            return;
        }
        let h = (px.len() / w) as i32;
        let wi = w as i32;
        if self.n == 1 {
            let (x0, x1) = (x.max(0), (x + wi).min(self.w));
            if x0 >= x1 {
                return;
            }
            for row in y.max(0)..(y + h).min(self.h) {
                let src = &px[(row - y) as usize * w + (x0 - x) as usize..][..(x1 - x0) as usize];
                let dst = &mut self.buf[(row * self.w + x0) as usize..(row * self.w + x1) as usize];
                match see_through {
                    None => dst.copy_from_slice(src),
                    Some(key) => {
                        for (d, &s) in dst.iter_mut().zip(src) {
                            if s != key {
                                *d = s;
                            }
                        }
                    }
                }
            }
            return;
        }
        let at = |xx: i32, yy: i32| {
            let p = px[(yy - y) as usize * w + (xx - x) as usize];
            (Some(p) != see_through).then_some(p)
        };
        // On even design pixels the screen pixels inside the block take their four straight from two rows.
        if x.rem_euclid(2) != 0 || (self.oy + y).rem_euclid(2) != 0 {
            self.cells(x, x + wi, y, y + h, at);
            return;
        }
        let rows = (self.buf.len() / self.stride) as i32;
        let (sx0, sx1) = (x.max(0) / 2, ((x + wi).min(self.w) / 2).min(self.stride as i32));
        let (sy0, sy1) = (((self.oy + y.max(0)) / 2 - self.sy0).max(0), ((self.oy + (y + h).min(self.h)).div_euclid(2) - self.sy0).min(rows));
        for sr in sy0..sy1 {
            let top = ((self.sy0 + sr) * 2 - self.oy - y) as usize;
            let (a, b) = (&px[top * w..(top + 1) * w], &px[(top + 1) * w..(top + 2) * w]);
            let k0 = (sx0 * 2 - x) as usize;
            let line = &mut self.buf[sr as usize * self.stride + sx0 as usize..sr as usize * self.stride + sx1.max(sx0) as usize];
            for (i, d) in line.iter_mut().enumerate() {
                let k = k0 + 2 * i;
                let q = [a[k], a[k + 1], b[k], b[k + 1]];
                *d = match see_through {
                    Some(key) if q.contains(&key) => {
                        let bg = *d;
                        let q = q.map(|p| if p == key { bg } else { p });
                        avg4(q[0], q[1], q[2], q[3])
                    }
                    _ => avg4(q[0], q[1], q[2], q[3]),
                };
            }
        }
        // An odd width or height leaves half a screen pixel at the right or bottom: the mean with what is there.
        if wi % 2 != 0 || h % 2 != 0 {
            let (ex, ey) = (x + wi - wi % 2, y + h - h % 2);
            if wi % 2 != 0 {
                self.cells(ex, x + wi, y, y + h, at);
            }
            if h % 2 != 0 {
                self.cells(x, ex, ey, y + h, at);
            }
        }
    }

    /// Nearest-neighbour blit of `img` into the rectangle (x, y, w, h); pixel art stays crisp.
    pub fn blit_scaled(&mut self, img: &crate::image::Image, x: i32, y: i32, w: i32, h: i32) {
        if img.w == 0 || img.h == 0 || w <= 0 || h <= 0 {
            return;
        }
        if self.n != 1 {
            self.blit_part(img, (0, 0, img.w, img.h), x, y, w, h);
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

    /// At `n` = 2: the part (sx, sy, sw, sh) of `img` (already cut to it, not empty) into (x, y, w, h) as the old
    /// nearest-neighbour blit into the design canvas and the 2:1 filter made it.
    #[allow(clippy::too_many_arguments)]
    fn blit_part(&mut self, img: &crate::image::Image, (sx, sy, sw, sh): (u32, u32, u32, u32), x: i32, y: i32, w: i32, h: i32) {
        let (x0, x1, y0, y1) = (x.max(0), (x + w).min(self.w), y.max(0), (y + h).min(self.h));
        if x0 >= x1 || y0 >= y1 {
            return;
        }
        let cols: Vec<u32> = (x0..x1).map(|col| (sx as u64 + (col - x) as u64 * sw as u64 / w as u64) as u32).collect();
        let rows: Vec<u32> = (y0..y1).map(|row| (sy as u64 + (row - y) as u64 * sh as u64 / h as u64) as u32 * img.w).collect();
        let px = &img.px;
        self.cells(x0, x1, y0, y1, |xx, yy| Some(px[(rows[(yy - y0) as usize] + cols[(xx - x0) as usize]) as usize]));
    }

    /// Nearest-neighbour blit of the part (sx, sy, sw, sh) of `img` into the rectangle (x, y, w, h): the pause menu's
    /// Zoom (`zoom`). A part outside the picture is cut to it.
    pub fn blit_region(&mut self, img: &crate::image::Image, (sx, sy, sw, sh): (u32, u32, u32, u32), x: i32, y: i32, w: i32, h: i32) {
        let (sw, sh) = (sw.min(img.w.saturating_sub(sx)), sh.min(img.h.saturating_sub(sy)));
        if sw == 0 || sh == 0 || w <= 0 || h <= 0 {
            return;
        }
        if self.n != 1 {
            self.blit_part(img, (sx, sy, sw, sh), x, y, w, h);
            return;
        }
        let (c0, c1) = (x.max(0), (x + w).min(self.w));
        if c0 >= c1 {
            return;
        }
        let cols: Vec<usize> = (c0..c1).map(|col| (sx as u64 + (col - x) as u64 * sw as u64 / w as u64) as usize).collect();
        let iw = img.w as usize;
        for dy in 0..h {
            let row = y + dy;
            if row < 0 || row >= self.h {
                continue;
            }
            let src_y = (sy as u64 + dy as u64 * sh as u64 / h as u64) as usize;
            let src = &img.px[src_y * iw..(src_y + 1) * iw];
            let at = (row * self.w + c0) as usize;
            for (d, &src_x) in self.buf[at..at + cols.len()].iter_mut().zip(&cols) {
                *d = src[src_x];
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
        if self.n != 1 {
            self.cells(x0, x1, y0, y1, |x, y| Some(mix(at(from, x, y), at(to, x, y), t)));
            return;
        }
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
        if self.n != 1 {
            let sw = s.w as i32;
            self.cells(x, x + sw, y, y + s.h as i32, |xx, yy| {
                let p = s.px[((yy - y) * sw + (xx - x)) as usize];
                let rgb = p & 0x00ff_ffff;
                (p >> 24 != 0).then(|| if dim == 0 { rgb } else { mix(rgb, toward, dim as u32) })
            });
            return;
        }
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

    /// The screen pixel (of `pixels`) holding design pixel (x, y) and where in it (x, y) lies: (column, row, x and y
    /// within it). A glyph cached for that place (`half_mask`) is painted from there (`mask_screen`).
    pub fn cell(&self, x: i32, y: i32) -> (i32, i32, u8, u8) {
        let (ax, ay) = (x, self.oy + y);
        let (fx, fy) = (ax.rem_euclid(self.n), ay.rem_euclid(self.n));
        ((ax - fx) / self.n, (ay - fy) / self.n - self.sy0, fx as u8, fy as u8)
    }

    /// An alpha mask of screen pixels (`mw` x `mh`, from `half_mask`) at screen pixel (sx, sy) in `color`, cut to the
    /// canvas.
    #[allow(clippy::too_many_arguments)]
    pub fn mask_screen(&mut self, sx: i32, sy: i32, mw: usize, mh: usize, alpha: &[u8], color: u32) {
        let rows = (self.buf.len() / self.stride.max(1)) as i32;
        let (c0, c1) = (sx.max(0), (sx + mw as i32).min(self.stride as i32));
        let (r0, r1) = (sy.max(0), (sy + mh as i32).min(rows));
        if c0 >= c1 || r0 >= r1 {
            return;
        }
        let plain = color & 0xff_ffff;
        for r in r0..r1 {
            let cover = &alpha[(r - sy) as usize * mw + (c0 - sx) as usize..][..(c1 - c0) as usize];
            let at = r as usize * self.stride + c0 as usize;
            for (px, &a) in self.buf[at..at + cover.len()].iter_mut().zip(cover) {
                match a {
                    0 => {}
                    255 => *px = plain,
                    a => *px = mix(*px, color, a as u32),
                }
            }
        }
    }

    /// An alpha mask of design pixels (`mw` x `mh`, coverage 0–255) at (x, y) in `color`: each pixel blended at
    /// `n` = 1, averaged per screen pixel at `n` = 2 (a text's soft edge, `text::draw_clipped`).
    #[allow(clippy::too_many_arguments)]
    pub fn mask(&mut self, x: i32, y: i32, mw: usize, mh: usize, alpha: &[u8], color: u32) {
        if self.n == 1 {
            for row in 0..mh {
                for col in 0..mw {
                    let a = alpha[row * mw + col];
                    if a > 0 {
                        self.blend(x + col as i32, y + row as i32, color, a);
                    }
                }
            }
            return;
        }
        let (sx, sy, fx, fy) = self.cell(x, y);
        let (w2, h2, half) = half_mask(alpha, mw, mh, fx, fy);
        self.mask_screen(sx, sy, w2, h2, &half, color);
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

/// `alpha` (`w` x `h` design pixels) placed `fx`, `fy` design pixels into a screen pixel, as screen pixels: each the
/// mean of the coverage of its four design pixels (2:1). Returns (width, height, coverage). Text caches it per glyph
/// and place (`text::Text`), so a letter costs a quarter of its design pixels per frame.
pub fn half_mask(alpha: &[u8], w: usize, h: usize, fx: u8, fy: u8) -> (usize, usize, Vec<u8>) {
    let (fx, fy) = (fx as usize & 1, fy as usize & 1);
    let (w2, h2) = ((fx + w).div_ceil(2), (fy + h).div_ceil(2));
    let mut sum = vec![0u16; w2 * h2];
    for (row, line) in alpha.chunks_exact(w.max(1)).take(h).enumerate() {
        let r = (row + fy) / 2;
        let out = &mut sum[r * w2..(r + 1) * w2];
        for (col, &a) in line.iter().enumerate() {
            out[(col + fx) / 2] += a as u16;
        }
    }
    (w2, h2, sum.into_iter().map(|s| ((s + 2) / 4) as u8).collect())
}

thread_local! {
    /// The design-size copy `Canvas::design` lends, kept between frames (4.9 MB at 1280x960).
    static SCRATCH: std::cell::RefCell<Vec<u32>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// `Canvas::design`: the canvas's design pixels as a canvas at `n` = 1 (`canvas`), averaged back into the screen
/// when it goes.
pub struct Design<'c, 'a> {
    c: &'c mut Canvas<'a>,
    scratch: Vec<u32>,
    /// The screen pixels it was made for and puts back: columns, then rows of `pixels`.
    area: (usize, usize, usize, usize),
}

impl Design<'_, '_> {
    /// The design pixels as a canvas (`n` = 1, `w` x `h`, the same `id` for the text checks).
    pub fn canvas(&mut self) -> Canvas<'_> {
        let c = &mut *self.c;
        let (w, h, id) = (c.w, c.h, c.id);
        if c.n == 1 {
            return Canvas { buf: &mut *c.buf, w, h, n: 1, stride: w as usize, oy: 0, sy0: 0, id };
        }
        let len = w.max(0) as usize * h.max(0) as usize;
        Canvas { buf: &mut self.scratch[..len], w, h, n: 1, stride: w as usize, oy: 0, sy0: 0, id }
    }
}

impl Drop for Design<'_, '_> {
    fn drop(&mut self) {
        let c = &mut *self.c;
        if c.n == 1 {
            return;
        }
        let (w, h, n) = (c.w as usize, c.h.max(1), c.n);
        let (sc0, sc1, sr0, sr1) = self.area;
        for k in sr0..sr1 {
            // The two design rows of screen row k (a band's edge row: its own row twice).
            let top = (c.sy0 + k as i32) * n - c.oy;
            let (a, b) = (top.clamp(0, h - 1) as usize, (top + 1).clamp(0, h - 1) as usize);
            if self.scratch.len() < (b + 1) * w {
                break;
            }
            let (ra, rb) = (&self.scratch[a * w + sc0 * 2..a * w + sc1 * 2], &self.scratch[b * w + sc0 * 2..b * w + sc1 * 2]);
            let out = &mut c.buf[k * c.stride + sc0..k * c.stride + sc1];
            for ((px, p), q) in out.iter_mut().zip(ra.chunks_exact(2)).zip(rb.chunks_exact(2)) {
                *px = avg4(p[0], p[1], q[0], q[1]);
            }
        }
        let scratch = std::mem::take(&mut self.scratch);
        SCRATCH.with(|s| *s.borrow_mut() = scratch);
    }
}

thread_local! {
    /// How many times `Canvas::design` lent its copy on this thread: the benches say how much of a frame is not
    /// painted natively yet.
    static DESIGN_PASSES: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// `Canvas::design` and `design_area` calls on this thread so far (painted natively; 0 at `n` = 1).
pub fn design_passes() -> u64 {
    DESIGN_PASSES.with(|d| d.get())
}

thread_local! {
    /// Where those calls come from: (file, line) to (calls, screen pixels enlarged and put back).
    static DESIGN_SITES: std::cell::RefCell<std::collections::HashMap<(&'static str, u32), (u64, u64)>> = std::cell::RefCell::new(std::collections::HashMap::new());
}

/// The places that called `Canvas::design`/`design_area` on this thread so far, the most screen pixels first:
/// (file, line, calls, screen pixels). For the benches (`bench-shelf`, `POCKETUI_FRAME_BENCH`): what is left to paint natively.
pub fn design_sites() -> Vec<(&'static str, u32, u64, u64)> {
    let mut v: Vec<_> = DESIGN_SITES.with(|s| s.borrow().iter().map(|(k, v)| (k.0, k.1, v.0, v.1)).collect());
    v.sort_by(|a, b| b.3.cmp(&a.3));
    v
}

#[cfg(test)]
pub(crate) mod tests {
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

    #[test]
    fn a_region_blit_enlarges_only_that_part() {
        // 4x4 picture, each pixel its own number; the lower right 2x2 part drawn at 4x4.
        let img = Image { w: 4, h: 4, px: (0..16).collect() };
        let mut buf = vec![99u32; 4 * 4];
        let mut c = Canvas::new(&mut buf, 4, 4);
        c.blit_region(&img, (2, 2, 2, 2), 0, 0, 4, 4);
        assert_eq!(c.buf, [10, 10, 11, 11, 10, 10, 11, 11, 14, 14, 15, 15, 14, 14, 15, 15]);
        // The whole picture is a plain blit; a part sticking out of the picture is cut to it; nothing to draw is nothing.
        let mut whole = vec![0u32; 16];
        Canvas::new(&mut whole, 4, 4).blit_region(&img, (0, 0, 4, 4), 0, 0, 4, 4);
        assert_eq!(whole, img.px);
        let mut cut = vec![99u32; 16];
        Canvas::new(&mut cut, 4, 4).blit_region(&img, (3, 3, 4, 4), 0, 0, 4, 4);
        assert!(cut.iter().all(|p| *p == 15));
        let mut none = vec![99u32; 16];
        Canvas::new(&mut none, 4, 4).blit_region(&img, (4, 4, 2, 2), 0, 0, 4, 4);
        assert!(none.iter().all(|p| *p == 99));
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

    // ---- painted natively (n = 2) against the old way: the design canvas and the 2:1 filter ----------------------

    const BACK: u32 = 0x203040;

    /// `paint` the old way (a 2w x 2h design canvas reduced 2:1, `scaled_layout`'s filter) and the new (straight into
    /// a w x h screen at n = 2), both over `BACK`.
    pub(crate) fn old_and_new(w: u32, h: u32, paint: impl Fn(&mut Canvas)) -> (Vec<u32>, Vec<u32>) {
        let mut design = vec![BACK; (w * h * 4) as usize];
        paint(&mut Canvas::new(&mut design, w * 2, h * 2));
        let mut old = vec![0u32; (w * h) as usize];
        crate::scaled_layout::ScaledFrame::default().downscale(&design, (w * 2, h * 2), &mut old, (w, h));
        let mut new = vec![BACK; (w * h) as usize];
        let mut c = Canvas::screen(&mut new, (w, h), 2);
        assert_eq!((c.w, c.h, c.n()), (w as i32 * 2, h as i32 * 2, 2), "the pages lay out at the design size");
        paint(&mut c);
        (old, new)
    }

    fn same(what: &str, (old, new): (Vec<u32>, Vec<u32>)) {
        let differ: Vec<usize> = (0..old.len()).filter(|&i| old[i] != new[i]).collect();
        assert!(differ.is_empty(), "{what}: {} pixels differ, first at {}: old {:06x} new {:06x}", differ.len(), differ[0], old[differ[0]], new[differ[0]]);
    }

    #[test]
    fn rectangles_and_their_kin_at_even_places_are_the_old_picture_exactly() {
        same("rect", old_and_new(40, 30, |c| c.rect(4, 6, 30, 18, 0xffcc00)));
        same("rect over the edges", old_and_new(40, 30, |c| c.rect(-10, 50, 200, 40, 0x00ff00)));
        same("frame", old_and_new(40, 30, |c| c.frame(2, 2, 60, 40, 4, 0xffffff)));
        same("shade_rect", old_and_new(40, 30, |c| {
            c.rect(0, 0, 80, 30, 0x808080);
            c.shade_rect(10, 4, 40, 20, 0xffffff, 100);
        }));
        same("darken_rect", old_and_new(40, 30, |c| c.darken_rect(0, 0, 50, 50, 77)));
        // The soft edge of a scrolled list: 1 px rows, each mixed a little less (game_info, game_card, console_page,
        // game_box). Painted as one call it is the old picture at even and odd places alike; a `shade_rect` per row
        // mixed a screen row twice where two design rows land on it (checked below), and the rectangle at an odd
        // place mixes its edge rows by the share it covers.
        for top in [0, 1, 2, 3] {
            close(&format!("an edge faded row by row from {top}"), 2, old_and_new(40, 30, |c| {
                c.rect(0, 0, 80, 60, 0xe8e2d0);
                c.shade_rect(3, top + 31, 71, 3, 0x405060, 90);
                let amounts: Vec<u8> = (0..24).map(|k| (240 - k * 10) as u8).collect();
                c.shade_rows(5, top, 70, 0, &amounts);
            }));
        }
        let (old, new) = old_and_new(40, 30, |c| {
            c.rect(0, 0, 80, 60, 0xe8e2d0);
            for k in 0..24 {
                c.shade_rect(5, k, 70, 1, 0, (240 - k * 10) as u8);
            }
        });
        let far = old.iter().zip(&new).filter(|(a, b)| [0, 8, 16].iter().any(|s| ((*a >> s) & 255).abs_diff((*b >> s) & 255) > 32)).count();
        assert!(far > 0, "a shade_rect per 1 px row is not the old picture at n = 2 (that is why shade_rows exists)");
        same("fade", old_and_new(40, 30, |c| {
            c.rect(0, 0, 44, 60, 0xff0000);
            c.fade(140);
        }));
        same("clear", old_and_new(8, 8, |c| c.clear(0x123456)));
        same("8x8 text", old_and_new(64, 16, |c| {
            c.text(2, 2, 2, 0xffffff, "Hi 42");
        }));
    }

    #[test]
    fn round_shapes_are_the_old_picture_exactly_wherever_they_are() {
        // Discs, rounded rectangles and rings come out with the same soft edges as the filter gave them, at even and
        // at odd places (each screen pixel: how many of its four design pixels the shape covers).
        for (x, y) in [(20, 16), (21, 17), (20, 17)] {
            same("disc", old_and_new(40, 30, |c| c.disc(x, y, 13, 0xffffff)));
            same("disc r 1", old_and_new(40, 30, |c| c.disc(x, y, 1, 0xffffff)));
            same("round_rect", old_and_new(40, 30, |c| c.round_rect(x - 14, y - 10, 51, 33, 9, 0xff8800)));
            same("pill", old_and_new(40, 30, |c| c.round_rect(x - 14, y - 10, 40, 20, 10, 0xff8800)));
            same("ring", old_and_new(40, 30, |c| c.ring(x, y, 12, 3, 0x00ffcc)));
        }
        same("round_rect without corners", old_and_new(40, 30, |c| c.round_rect(4, 4, 30, 20, 0, 0x00ff00)));
        same("round_rect cut by the edge", old_and_new(40, 30, |c| c.round_rect(-9, 40, 60, 40, 12, 0x00ff00)));
    }

    #[test]
    fn pictures_are_the_old_picture_exactly() {
        let img = Image { w: 14, h: 10, px: (0..140).map(|i| (i * 0x051a2f + 0x103050) & 0xffffff).collect() };
        for (x, y) in [(4, 6), (5, 7), (-6, -4), (70, 50)] {
            same("blit 1:1", old_and_new(40, 30, |c| c.blit_scaled(&img, x, y, 14, 10)));
            same("blit enlarged", old_and_new(40, 30, |c| c.blit_scaled(&img, x, y, 37, 23)));
            same("blit shrunk", old_and_new(40, 30, |c| c.blit_scaled(&img, x, y, 9, 7)));
            same("blit_region", old_and_new(40, 30, |c| c.blit_region(&img, (3, 2, 8, 6), x, y, 30, 22)));
            same("crossfade", old_and_new(40, 30, |c| c.crossfade(Some((&img, x, y)), Some((&img, x + 3, y + 1)), 0x112233, 90)));
        }
        let sprite = crate::system_art::Sprite { w: 6, h: 6, px: (0..36).map(|i| if i % 5 == 0 { 0 } else { 0xff00_0000 | (i * 0x070707) }).collect() };
        for (x, y) in [(4, 6), (5, 7)] {
            same("sprite", old_and_new(20, 20, |c| c.blit_sprite(&sprite, x, y, 0)));
            same("sprite dimmed", old_and_new(20, 20, |c| c.blit_sprite_to(&sprite, x, y, 120, 0xffffff)));
        }
    }

    #[test]
    fn soft_masks_are_the_old_picture_within_rounding() {
        // A glyph's coverage averaged once per screen pixel, not each design pixel mixed and then averaged: at most a
        // step of rounding apart per channel.
        let alpha: Vec<u8> = (0..15 * 11).map(|i| ((i * 37) % 256) as u8).collect();
        for (x, y) in [(4, 6), (5, 7), (5, 6), (-3, -2)] {
            let (old, new) = old_and_new(20, 16, |c| c.mask(x, y, 15, 11, &alpha, 0xf0e0d0));
            for (i, (a, b)) in old.iter().zip(&new).enumerate() {
                for shift in [0, 8, 16] {
                    let (p, q) = ((a >> shift) & 0xff, (b >> shift) & 0xff);
                    assert!(p.abs_diff(q) <= 2, "at {x},{y} pixel {i}: old {a:06x} new {b:06x}");
                }
            }
        }
        // Fully covered or not at all: exactly.
        let solid = vec![255u8; 8 * 6];
        same("solid mask", old_and_new(20, 16, |c| c.mask(4, 2, 8, 6, &solid, 0xffffff)));
    }

    /// At most `steps` apart in every channel of every pixel.
    pub(crate) fn close(what: &str, steps: u32, (old, new): (Vec<u32>, Vec<u32>)) {
        for (i, (a, b)) in old.iter().zip(&new).enumerate() {
            for shift in [0, 8, 16] {
                let (p, q) = ((a >> shift) & 0xff, (b >> shift) & 0xff);
                assert!(p.abs_diff(q) <= steps, "{what}: pixel {i}: old {a:06x} new {b:06x}");
            }
        }
    }

    #[test]
    fn shapes_built_from_rows_and_squares_are_the_old_picture_exactly_wherever_they_are() {
        // An arrow of 1 px rows, a check mark of overlapping squares, columns that overlap: painted piece by piece
        // natively, every 1 px row keeps a screen pixel of its own and the shape grows a pixel at odd places.
        for (x, y) in [(10, 20), (11, 21), (10, 21), (11, 20)] {
            let arrow: Vec<_> = (-7..=7).map(|dy| (x, y + dy, (10 * (7 - i32::abs(dy)) / 7).max(1), 1)).collect();
            same("arrow", old_and_new(40, 30, |c| c.fill_rects(&arrow, 0xffffff)));
            let squares: Vec<_> = (0..12).map(|i| (x + i, y + i / 2, 3, 3)).collect();
            same("squares", old_and_new(40, 30, |c| c.fill_rects(&squares, 0x40ff80)));
            let cols: Vec<_> = (0..9).map(|i| (x + i, y - (9 - i), 2, (9 - i) * 2)).collect();
            same("columns", old_and_new(40, 30, |c| c.fill_rects(&cols, 0xff4040)));
            let mut piecewise = vec![BACK; 40 * 30];
            let mut c = Canvas::screen(&mut piecewise, (40, 30), 2);
            for &(x, y, w, h) in &arrow {
                c.rect(x, y, w, h, 0xffffff);
            }
            if y % 2 == 1 {
                assert_ne!(piecewise, old_and_new(40, 30, |c| c.fill_rects(&arrow, 0xffffff)).1, "row by row the arrow is not the old one");
            }
        }
        same("cut by the edges", old_and_new(20, 10, |c| c.fill_rects(&[(-5, -3, 12, 9), (30, 15, 20, 9), (3, 2, 4, 30)], 0xffffff)));
    }

    #[test]
    fn shapes_blended_pixel_by_pixel_are_the_old_picture_within_rounding() {
        let soft = |c: &mut Canvas, cx: i32, cy: i32| {
            c.blend_with((cx - 12, cy - 12, 25, 25), 0xf0e0d0, |put| {
                for y in cy - 12..=cy + 12 {
                    for x in cx - 12..=cx + 12 {
                        let d = (((x - cx).pow(2) + (y - cy).pow(2)) as f32).sqrt();
                        let a = ((10.5 - d).clamp(0.0, 1.0) * 200.0) as u8;
                        if a > 0 {
                            put(x, y, a);
                        }
                    }
                }
            });
        };
        for (x, y) in [(20, 16), (21, 17), (20, 17)] {
            close("soft disc", 2, old_and_new(40, 30, |c| soft(c, x, y)));
            let rows: Vec<_> = (0..14).map(|dy| (x - 10 + (dy % 3), y - 7 + dy, 20, 1)).collect();
            close("rows mixed at an alpha", 2, old_and_new(40, 30, |c| c.fill_rects_at(&rows, 0xffffff, 90)));
        }
        // A 1 px stroke at an odd column blended pixel by pixel: `blend` alone keeps only every fourth design pixel,
        // so the stroke was gone; as one shape it is there, half as strong, as the filter showed it (the coverage is
        // averaged before it is mixed: a step or so of rounding).
        let (old, new) = old_and_new(10, 10, |c| {
            c.blend_with((0, 0, 20, 20), 0xffffff, |put| {
                for y in 2..18 {
                    put(7, y, 255);
                }
            })
        });
        assert!((1..9).all(|y| new[y * 10 + 3] != BACK), "the stroke shows in every row");
        close("a 1 px stroke", 3, (old, new));
        // At n = 1 every put is a blend, also twice on the same pixel.
        let (mut a, mut b) = (vec![BACK; 64], vec![BACK; 64]);
        Canvas::new(&mut a, 8, 8).blend_with((0, 0, 8, 8), 0xff0000, |put| {
            put(3, 3, 100);
            put(3, 3, 100);
            put(9, 3, 255);
        });
        let mut c = Canvas::new(&mut b, 8, 8);
        c.blend(3, 3, 0xff0000, 100);
        c.blend(3, 3, 0xff0000, 100);
        assert_eq!(a, b);
    }

    #[test]
    fn a_one_pixel_line_stays_a_whole_line_wherever_it_is() {
        // The hairline rule (docs/dev/native-paint.md): every line 1 design pixel thick keeps a whole screen pixel at
        // n = 2, in its full colour, at even and at odd places: rows, columns, frames, shaded lines.
        for (x, y) in [(4, 4), (5, 5), (4, 5), (5, 4)] {
            let mut buf = vec![0u32; 20 * 16];
            let mut c = Canvas::screen(&mut buf, (20, 16), 2);
            c.frame(x, y, 21, 17, 1, 0xffffff);
            let at = |c: &Canvas, sx: i32, sy: i32| c.pixels()[(sy * 20 + sx) as usize];
            // A closed ring on the screen pixels the 1 px lines lie in (where the old picture showed them).
            let (l, r, t, b) = (x / 2, (x + 20) / 2, y / 2, (y + 16) / 2);
            for sy in 0..16 {
                for sx in 0..20 {
                    let edge = (sx == l || sx == r || sy == t || sy == b) && (l..=r).contains(&sx) && (t..=b).contains(&sy);
                    assert_eq!(at(&c, sx, sy), if edge { 0xffffff } else { 0 }, "frame at {x},{y}: screen pixel {sx},{sy}");
                }
            }
            // A shaded 1 px column is mixed by the share it covers, as the old picture showed it: it does not vanish.
            close(&format!("a shaded 1 px column at {x}"), 1, old_and_new(20, 16, |c| c.shade_rect(x + 2, 0, 1, 32, 0xffffff, 255)));
        }
    }

    #[test]
    fn an_edge_rounds_to_the_nearest_screen_pixel_and_a_hairline_stays() {
        let mut buf = vec![0u32; 10 * 4];
        let mut c = Canvas::screen(&mut buf, (10, 4), 2);
        // Design columns 3..9 (odd start): screen columns 2..5 (3 and 9 round up), the old way's half grey edges are
        // whole pixels now; a 1 px line at an odd row is one whole screen row, not half a grey one.
        c.rect(3, 0, 6, 2, 0xffffff);
        c.rect(0, 5, 20, 1, 0x00ff00);
        let row = |c: &Canvas, y: usize| c.pixels()[y * 10..(y + 1) * 10].to_vec();
        assert_eq!(row(&c, 0), [0, 0, 0xffffff, 0xffffff, 0xffffff, 0, 0, 0, 0, 0]);
        assert_eq!(row(&c, 2), [0x00ff00; 10]);
        assert_eq!(row(&c, 1), [0; 10]);
        assert_eq!(c.get(5, 0), 0xffffff, "the design pixel reads the screen pixel that shows it");
        assert_eq!(c.get(-1, 0), 0);
    }

    #[test]
    fn a_band_of_rows_rounds_its_edges_as_a_rectangle_there_would() {
        // The band 3..9 (odd top) cuts what is painted into it where `rect(.., 3, .., 6)` would end: the same rows.
        let (mut a, mut b) = (vec![0u32; 8 * 8], vec![0u32; 8 * 8]);
        let mut whole = Canvas::screen(&mut a, (8, 8), 2);
        let id = whole.id();
        {
            let mut band = whole.rows(3, 9);
            assert_eq!((band.w, band.h, band.n()), (16, 6, 2));
            assert_eq!(band.id(), id + 3 * 16 * 4, "the text checks see the band 3 design rows down");
            band.rect(-5, -5, 40, 40, 0xffffff);
            // In the band, design row 0 is the whole picture's row 3.
            band.rect(0, 3, 4, 2, 0xff0000);
        }
        let mut c = Canvas::screen(&mut b, (8, 8), 2);
        c.rect(0, 3, 16, 6, 0xffffff);
        c.rect(0, 6, 4, 2, 0xff0000);
        assert_eq!(a, b);
        // At n = 1 a band is the rows themselves, as it always was.
        let mut px = vec![0u32; 4 * 4];
        let mut c = Canvas::new(&mut px, 4, 4);
        let first = c.pixels().as_ptr() as usize;
        let band = c.rows(1, 3);
        assert_eq!((band.id(), band.pixels().as_ptr() as usize, band.pixels().len()), (first + 16, first + 16, 8));
    }

    #[test]
    fn code_on_the_design_pixels_paints_the_old_picture_and_leaves_the_rest_alone() {
        // `design` for code that is not painted natively yet: what it paints is the old way's picture; what it leaves
        // alone is the screen as it was, bit for bit.
        let img = Image { w: 7, h: 5, px: (0..35).map(|i| i * 0x030507).collect() };
        same("design", old_and_new(20, 16, |c| {
            c.rect(0, 0, 40, 32, 0x405060);
            c.disc(9, 9, 6, 0xffffff);
            let mut d = c.design();
            let mut dc = d.canvas();
            assert_eq!((dc.w, dc.h, dc.n()), (40, 32, 1));
            let i = (25 * dc.w + 31) as usize;
            dc.pixels_mut()[i] = 0xff0000;
            dc.blit_scaled(&img, 21, 17, 7, 5);
        }));
        same("design_area", old_and_new(20, 16, |c| {
            c.rect(0, 0, 40, 32, 0x405060);
            c.ring(20, 16, 10, 2, 0xffffff);
            let mut d = c.design_area(5, 7, 9, 9);
            let mut dc = d.canvas();
            dc.rect(5, 7, 9, 9, 0x00ff00);
        }));
        let mut px = vec![0x123456u32; 12 * 10];
        let before = px.clone();
        {
            let mut c = Canvas::screen(&mut px, (12, 10), 2);
            let passes = design_passes();
            let mut band = c.rows(3, 15);
            drop(band.design());
            assert_eq!(design_passes(), passes + 1, "counted for the benches");
        }
        assert_eq!(px, before, "a guard that paints nothing changes nothing");
        // At n = 1 it is the canvas itself: nothing copied, nothing counted.
        let mut px = vec![0u32; 16];
        let mut c = Canvas::new(&mut px, 4, 4);
        let at = c.pixels().as_ptr();
        let passes = design_passes();
        assert_eq!(c.design().canvas().pixels().as_ptr(), at);
        assert_eq!(design_passes(), passes);
    }

    #[test]
    fn a_half_mask_averages_each_screen_pixel_where_the_glyph_falls() {
        // 3x2 design pixels placed one in and one down: 2x2 screen pixels, each the mean of its four.
        let (w, h, m) = half_mask(&[255, 255, 255, 255, 255, 255], 3, 2, 1, 1);
        assert_eq!((w, h), (2, 2));
        assert_eq!(m, [64, 128, 64, 128]);
        assert_eq!(half_mask(&[255; 4], 2, 2, 0, 0), (1, 1, vec![255]));
    }


    #[test]
    fn a_design_block_copied_is_the_old_picture_exactly() {
        let px: Vec<u32> = (0..17 * 11).map(|i| (i * 0x0b1d2f + 0x405060) & 0xffffff).collect();
        for (x, y) in [(4, 6), (5, 7), (4, 7), (-3, -2), (30, 20)] {
            same("copy_design", old_and_new(20, 16, |c| c.copy_design(x, y, 17, &px, None)));
            let even: Vec<u32> = px[..16 * 10].to_vec();
            same("copy_design even", old_and_new(20, 16, |c| c.copy_design(x, y, 16, &even, None)));
            let mut holes = even.clone();
            for i in (0..holes.len()).step_by(7) {
                holes[i] = 0xff00_0000;
            }
            same("copy_design see-through", old_and_new(20, 16, |c| {
                c.rect(0, 0, 40, 32, 0x334455);
                c.copy_design(x, y, 16, &holes, Some(0xff00_0000));
            }));
        }
    }

}
