//! CPU layout canvas for small panels. Both live frames and snapshots use this path.
//! Large outputs keep their native canvas; output size wins over the profile (HDMI).

use crate::gfx::Canvas;

/// The supported layouts start at 1280×960, with the panel's aspect ratio preserved.
/// Outputs above 720 lines and panels of 1024×720 or more (the Smart Pro's 1280×720) keep their native canvas. A panel
/// whose double already holds 1280×960 and stays within 1440×1440 gets exactly that double (every H700 panel: 640×480, 720×480 and the CubeXX's 720×720): the cheap 2:1
/// box reduction, and a pixel font lands on whole screen pixels (`step` 2). The CubeXX's 1280×1280 with a 16:9 area
/// filter cost about three times the frame of a 640×480 panel (docs/H700-SCALED-LAYOUT.md "CubeXX").
pub fn logical_size(panel: (u32, u32)) -> (u32, u32) {
    // A panel of 1024x720 and more is painted natively, as the Brick's 1024x768: the TrimUI Smart Pro's 1280x720 is the
    // full 1280 width with 48 lines less (`kit::short_screen`). Laid out at 1707x960 and reduced by 3:4 it took 108-132 ms
    // a frame on an A53, and a pixel font fell between screen pixels.
    if panel.0 >= 1024 && panel.1 >= 720 {
        return panel;
    }
    logical_size_up_to(panel, 720)
}

fn logical_size_up_to((w, h): (u32, u32), max_h: u32) -> (u32, u32) {
    if w < 64 || !(64..=max_h).contains(&h) || u64::from(w) > u64::from(h) * 4 || u64::from(h) > u64::from(w) * 2 {
        return (w, h);
    }
    if (1280..=1440).contains(&(w * 2)) && (960..=1440).contains(&(h * 2)) {
        return (w * 2, h * 2);
    }
    if u64::from(w) * 960 >= u64::from(h) * 1280 {
        (((u64::from(w) * 960).div_ceil(u64::from(h))) as u32, 960)
    } else {
        (1280, ((u64::from(h) * 1280).div_ceil(u64::from(w))) as u32)
    }
}

/// Covered source pixels with Q14 weights summing to 16384 per destination pixel.
/// At 1280→720 the fractions are multiples of 1/16, represented exactly. Other ratios
/// round cumulative boundaries, preserving constant colours without per-pixel division.
fn coverage(src: u32, dst: u32) -> Vec<Vec<(usize, u64)>> {
    (0..dst)
        .map(|d| {
            let start = u64::from(d) * u64::from(src);
            let end = start + u64::from(src);
            let unit = u64::from(dst);
            (start / unit..end.div_ceil(unit))
                .map(|s| {
                    let scale = |v: u64| ((v - start) * (1 << 14) + u64::from(src) / 2) / u64::from(src);
                    (s as usize, scale(end.min((s + 1) * unit)) - scale(start.max(s * unit)))
                })
                .collect()
        })
        .collect()
}

/// Reuses the logical buffer and area weights across redraws; never schedules a redraw itself.
#[derive(Default)]
pub struct ScaledFrame {
    sizes: Option<((u32, u32), (u32, u32))>,
    pixels: Vec<u32>,
    xs: Vec<Vec<(usize, u64)>>,
    ys: Vec<Vec<(usize, u64)>>,
    packed_area: bool,
    horizontal: Vec<u64>,
    row_sums: Vec<u64>,
    /// Keep the whole logical frame after painting, also where it is the panel's own size (the showcase's partial
    /// frames, `render_partial`).
    pub retain: bool,
    /// The logical size of the frame kept in `pixels`, whole and valid (None: nothing to paint over).
    kept: Option<(u32, u32)>,
}

/// How many canvas pixels make one pixel of the screen: 2 where a 640x480 panel shows a 1280x960 canvas, 1 on a native
/// one and where the ratio isn't whole (1280 to 720). A pixel font sized in whole screen pixels per font pixel
/// (`text::raster`, Departure Mono) asks it: an odd number of canvas pixels per font pixel came out as half pixels.
#[cfg(not(test))]
static STEP: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);
#[cfg(test)]
thread_local! {
    static STEP: std::sync::atomic::AtomicU32 = const { std::sync::atomic::AtomicU32::new(1) };
}

/// The canvas `logical` is shown on `panel` from now on (`step`): the live frame and `--snapshot` say so before painting.
pub fn set_step(logical: (u32, u32), panel: (u32, u32)) {
    let step = if panel.0 > 0 && logical.0 % panel.0 == 0 && logical.0 / panel.0 == logical.1 / panel.1.max(1) { logical.0 / panel.0 } else { 1 };
    #[cfg(not(test))]
    STEP.store(step.max(1), std::sync::atomic::Ordering::Relaxed);
    #[cfg(test)]
    STEP.with(|s| s.store(step.max(1), std::sync::atomic::Ordering::Relaxed));
}

pub fn step() -> u32 {
    #[cfg(not(test))]
    let v = STEP.load(std::sync::atomic::Ordering::Relaxed);
    #[cfg(test)]
    let v = STEP.with(|s| s.load(std::sync::atomic::Ordering::Relaxed));
    v
}

/// How a small panel's frame is painted (`POCKETUI_PAINT`, docs/dev/native-paint.md): `native` (the default) paints
/// straight into the panel's buffer at a whole divisor (`gfx::Canvas::screen`); `scaled` (the fallback, and always
/// safe mode) paints the design canvas at its size and reduces it. Read once; the tests set it per thread (`set_paint`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Paint {
    Scaled,
    Native,
}

impl Paint {
    /// `POCKETUI_PAINT`'s value: `scaled` (or `old`) paints the old way; anything else, or nothing, natively.
    pub fn parse(v: Option<&str>) -> Paint {
        match v.map(str::trim) {
            Some("scaled" | "old") => Paint::Scaled,
            _ => Paint::Native,
        }
    }
}

#[cfg(not(test))]
static PAINT: std::sync::OnceLock<std::sync::atomic::AtomicU8> = std::sync::OnceLock::new();
#[cfg(test)]
thread_local! {
    static PAINT: std::sync::atomic::AtomicU8 = const { std::sync::atomic::AtomicU8::new(1) };
}

/// The way frames are painted (`Paint`).
pub fn paint() -> Paint {
    #[cfg(not(test))]
    let v = PAINT
        .get_or_init(|| std::sync::atomic::AtomicU8::new((Paint::parse(std::env::var("POCKETUI_PAINT").ok().as_deref()) == Paint::Native) as u8))
        .load(std::sync::atomic::Ordering::Relaxed);
    #[cfg(test)]
    let v = PAINT.with(|p| p.load(std::sync::atomic::Ordering::Relaxed));
    if v == 1 { Paint::Native } else { Paint::Scaled }
}

/// Paint from now on as `p` (the A/B bench and the tests; a running pocket-ui reads `POCKETUI_PAINT` once).
pub fn set_paint(p: Paint) {
    let v = (p == Paint::Native) as u8;
    #[cfg(not(test))]
    PAINT.get_or_init(|| std::sync::atomic::AtomicU8::new(v)).store(v, std::sync::atomic::Ordering::Relaxed);
    #[cfg(test)]
    PAINT.with(|s| s.store(v, std::sync::atomic::Ordering::Relaxed));
}

/// Design pixels per screen pixel when `panel` is painted natively: 2 where the design canvas is exactly twice the
/// panel (640x480, 720x480, 720x720: 1280x960, 1440x960, 1440x1440) and `paint` says `native`; `None` otherwise (the
/// old way, or a panel painted at its own size). Fractional ratios (1280x720's 1707x960) stay scaled.
pub fn native_divisor(panel: (u32, u32)) -> Option<u32> {
    native_divisor_for(paint(), panel)
}

pub fn native_divisor_for(p: Paint, panel: (u32, u32)) -> Option<u32> {
    (p == Paint::Native && panel.0 > 0 && logical_size(panel) == (panel.0 * 2, panel.1 * 2)).then_some(2)
}

impl ScaledFrame {
    /// One frame for `panel` into `output` (`panel`'s size): `paint` gets the design canvas, at its own size and
    /// reduced afterwards (`Paint::Scaled`) or straight on the screen (`Paint::Native`, `native_divisor`).
    pub fn render(&mut self, output: &mut [u32], panel: (u32, u32), paint: &mut dyn FnMut(&mut Canvas)) {
        self.render_as(self::paint(), output, panel, paint);
    }

    /// `render` painted as `how` whatever `POCKETUI_PAINT` says (safe mode keeps the old way).
    pub fn render_as(&mut self, how: Paint, output: &mut [u32], panel: (u32, u32), paint: &mut dyn FnMut(&mut Canvas)) {
        let logical = logical_size(panel);
        set_step(logical, panel);
        if logical == panel {
            if self.retain {
                self.pixels.resize(output.len(), 0);
                paint(&mut Canvas::new(&mut self.pixels, panel.0, panel.1));
                output.copy_from_slice(&self.pixels);
                self.kept = Some(panel);
            } else {
                paint(&mut Canvas::new(output, panel.0, panel.1));
                self.kept = None;
            }
            return;
        }
        if let Some(n) = native_divisor_for(how, panel) {
            // Painted natively there is no whole design frame to paint over later (`render_partial`): a full frame.
            paint(&mut Canvas::screen(output, panel, n));
            self.kept = None;
            return;
        }
        self.prepare(logical, panel);
        paint(&mut Canvas::new(&mut self.pixels, logical.0, logical.1));
        self.reduce(output);
        self.kept = Some(logical);
    }

    /// Paints over the last whole frame (only what changed, e.g. the Cabinet's TV) and shows it. False, painting
    /// nothing, when there is no whole frame of this size to paint over.
    pub fn render_partial(&mut self, output: &mut [u32], panel: (u32, u32), paint: &mut dyn FnMut(&mut [u32], u32, u32)) -> bool {
        let logical = logical_size(panel);
        if self.kept != Some(logical) || self.pixels.len() != logical.0 as usize * logical.1 as usize {
            return false;
        }
        if logical != panel && self.sizes != Some((logical, panel)) {
            return false;
        }
        set_step(logical, panel);
        paint(&mut self.pixels, logical.0, logical.1);
        if logical == panel {
            output.copy_from_slice(&self.pixels);
        } else {
            self.reduce(output);
        }
        true
    }

    /// Whether `render_partial` has a whole frame for this panel.
    pub fn can_partial(&self, panel: (u32, u32)) -> bool {
        let logical = logical_size(panel);
        self.kept == Some(logical) && (logical == panel || self.sizes == Some((logical, panel)))
    }

    fn prepare(&mut self, logical: (u32, u32), panel: (u32, u32)) {
        if self.sizes == Some((logical, panel)) {
            return;
        }
        self.sizes = Some((logical, panel));
        self.pixels.resize(logical.0 as usize * logical.1 as usize, 0);
        self.xs = coverage(logical.0, panel.0);
        self.ys = coverage(logical.1, panel.1);
        self.packed_area = logical != (panel.0 * 2, panel.1 * 2) && self.xs.iter().chain(&self.ys).flatten().all(|(_, weight)| weight % 256 == 0);
        if self.packed_area {
            self.horizontal.resize(panel.0 as usize * logical.1 as usize, 0);
            self.row_sums.resize(panel.0 as usize, 0);
            for (_, weight) in self.xs.iter_mut().chain(&mut self.ys).flatten() {
                *weight >>= 8;
            }
        }
    }

    /// Reduce an already painted frame (snapshots and the isolated scaling benchmark).
    pub fn downscale(&mut self, input: &[u32], logical: (u32, u32), output: &mut [u32], panel: (u32, u32)) {
        self.prepare(logical, panel);
        self.pixels.copy_from_slice(input);
        self.reduce(output);
    }

    fn reduce(&mut self, output: &mut [u32]) {
        let ((sw, sh), (dw, dh)) = self.sizes.expect("prepared frame");
        assert_eq!(output.len(), dw as usize * dh as usize);
        if sw == dw * 2 && sh == dh * 2 {
            // Four taps, packed R/B sums: no floating point, divisions, weights or intermediate image.
            for (rows, output_row) in self.pixels.chunks_exact(sw as usize * 2).zip(output.chunks_exact_mut(dw as usize)) {
                let (top, bottom) = rows.split_at(sw as usize);
                for ((a, b), pixel) in top.as_chunks::<2>().0.iter().zip(bottom.as_chunks::<2>().0).zip(output_row) {
                    let rb = (a[0] & 0xff00ff) + (a[1] & 0xff00ff) + (b[0] & 0xff00ff) + (b[1] & 0xff00ff);
                    let g = (a[0] & 0x00ff00) + (a[1] & 0x00ff00) + (b[0] & 0x00ff00) + (b[1] & 0x00ff00);
                    *pixel = (((rb + 0x020002) >> 2) & 0xff00ff) | (((g + 0x000200) >> 2) & 0x00ff00);
                }
            }
        } else if self.packed_area {
            // Exact Q6 axis weights (including 1280→720), in two passes without intermediate
            // rounding. Three channels fit in one u64, each with 20 bits (255 × 4096).
            for (source_row, filtered_row) in self.pixels.chunks_exact(sw as usize).zip(self.horizontal.chunks_exact_mut(dw as usize)) {
                for (sum, weights) in filtered_row.iter_mut().zip(&self.xs) {
                    *sum = 0;
                    for &(sx, wx) in weights {
                        let p = u64::from(source_row[sx]);
                        let packed = (p & 255) | ((p & 0xff00) << 12) | ((p & 0xff0000) << 24);
                        *sum += packed * wx;
                    }
                }
            }
            for (y, row) in output.chunks_exact_mut(dw as usize).enumerate() {
                self.row_sums.fill(0);
                for &(sy, wy) in &self.ys[y] {
                    let source_row = &self.horizontal[sy * dw as usize..(sy + 1) * dw as usize];
                    for (sum, source) in self.row_sums.iter_mut().zip(source_row) {
                        *sum += source * wy;
                    }
                }
                for (pixel, sum) in row.iter_mut().zip(&self.row_sums) {
                    let sum = sum + ((1 << 11) | (1 << 31) | (1 << 51));
                    *pixel = (((sum >> 12) & 255) | (((sum >> 32) & 255) << 8) | (((sum >> 52) & 255) << 16)) as u32;
                }
            }
        } else {
            // Q14 × Q14: fixed denominator, so each channel needs one shift rather than a division.
            const TOTAL: u64 = 1 << 28;
            for (y, row) in output.chunks_exact_mut(dw as usize).enumerate() {
                for (x, pixel) in row.iter_mut().enumerate() {
                    let (mut r, mut g, mut b) = (0, 0, 0);
                    for &(sy, wy) in &self.ys[y] {
                        for &(sx, wx) in &self.xs[x] {
                            let p = self.pixels[sy * sw as usize + sx];
                            let weight = wx * wy;
                            r += u64::from((p >> 16) & 255) * weight;
                            g += u64::from((p >> 8) & 255) * weight;
                            b += u64::from(p & 255) * weight;
                        }
                    }
                    *pixel = ((((r + TOTAL / 2) >> 28) as u32) << 16) | ((((g + TOTAL / 2) >> 28) as u32) << 8) | (((b + TOTAL / 2) >> 28) as u32);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supported_panels_and_native_outputs() {
        for (panel, logical) in [
            ((640, 480), (1280, 960)),
            ((720, 480), (1440, 960)),
            ((720, 720), (1440, 1440)),
            ((1240, 1080), (1240, 1080)),
            ((1920, 1080), (1920, 1080)),
            ((1280, 720), (1280, 720)),
            ((1024, 720), (1024, 720)),
            ((960, 544), (1695, 960)),
            ((0, 0), (0, 0)),
        ] {
            assert_eq!(logical_size(panel), logical);
        }
        assert_eq!(logical_size_up_to((1280, 800), 800), (1536, 960));
        assert_eq!(logical_size_up_to((1280, 800), 720), (1280, 800));
        assert_eq!(logical_size((1024, 768)), (1024, 768));
    }

    #[test]
    fn normalized_touch_reaches_the_same_logical_hit_rect_after_scaling() {
        use crate::{
            pad::Button,
            touch::{HitMap, TapAction},
        };
        for panel in [(640, 480), (720, 480), (720, 720)] {
            let (w, h) = logical_size(panel);
            let mut hits = HitMap::default();
            hits.button(crate::touch::Area::new(w as i32 / 2 - 20, h as i32 / 2 - 20, 40, 40), Button::South);
            let (x, y) = crate::touch::to_px((0.5, 0.5), w, h);
            assert_eq!(hits.tap(x, y), TapAction::Press(Button::South));
        }
    }

    #[test]
    fn box_filter_averages_channels_without_overflow_or_bleeding() {
        let mut frame = ScaledFrame::default();
        let mut out = [0; 2];
        frame.downscale(
            &[0xff0000, 0x00ff00, 0xffffff, 0xffffff, 0x0000ff, 0xffffff, 0xffffff, 0xffffff],
            (4, 2),
            &mut out,
            (2, 1),
        );
        assert_eq!(out, [0x808080, 0xffffff]);
    }

    #[test]
    fn fractional_area_filter_covers_every_edge_and_preserves_constants() {
        let mut frame = ScaledFrame::default();
        let mut out = [0; 4];
        frame.downscale(&[0, 0, 0xffffff, 0, 0, 0xffffff, 0xffffff, 0xffffff, 0xffffff], (3, 3), &mut out, (2, 2));
        assert_eq!(out, [0, 0xaaaaaa, 0xaaaaaa, 0xe3e3e3]);
        frame.downscale(&[0x123456; 9], (3, 3), &mut out, (2, 2));
        assert_eq!(out, [0x123456; 4]);
    }

    #[test]
    fn square_panel_filter_matches_exact_integer_area_reference() {
        // 32→18 has the same fractional weights as the real 1280→720 panel.
        let source: Vec<u32> = (0..32 * 32).map(|i| ((i * 7919) & 0xffffff) as u32).collect();
        let mut output = vec![0; 18 * 18];
        ScaledFrame::default().downscale(&source, (32, 32), &mut output, (18, 18));
        for y in 0usize..18 {
            for x in 0usize..18 {
                let mut expected = 0;
                for shift in [0, 8, 16] {
                    let mut sum = 0;
                    for sy in 0..32 {
                        for sx in 0..32 {
                            let wx = ((x + 1) * 32).min((sx + 1) * 18).saturating_sub((x * 32).max(sx * 18));
                            let wy = ((y + 1) * 32).min((sy + 1) * 18).saturating_sub((y * 32).max(sy * 18));
                            sum += ((source[sy * 32 + sx] >> shift) & 255) as usize * wx * wy;
                        }
                    }
                    expected |= (((sum + 512) / 1024) as u32) << shift;
                }
                assert_eq!(output[y * 18 + x], expected);
            }
        }
    }

    #[test]
    fn the_scaled_fallback_paints_big_panels_directly_and_reuses_its_small_canvas() {
        let mut frame = ScaledFrame::default();
        let mut output = vec![0; 640 * 480];
        frame.render_as(Paint::Scaled, &mut output, (640, 480), &mut |c| {
            assert_eq!((c.w, c.h, c.n()), (1280, 960, 1));
            c.clear(0xabcdef);
        });
        assert!(output.iter().all(|p| *p == 0xabcdef));
        let pointer = frame.pixels.as_ptr();
        frame.render_as(Paint::Scaled, &mut output, (640, 480), &mut |c| c.clear(0));
        assert_eq!(pointer, frame.pixels.as_ptr());
        let mut native = vec![0; 1240 * 1080];
        let native_pointer = native.as_ptr();
        frame.render_as(Paint::Scaled, &mut native, (1240, 1080), &mut |c| assert_eq!(c.pixels().as_ptr(), native_pointer));
    }

    #[test]
    fn paint_native_lays_out_the_same_design_canvas_straight_on_the_panel() {
        assert_eq!(Paint::parse(None), Paint::Native, "painting natively is the default");
        assert_eq!(Paint::parse(Some("scaled")), Paint::Scaled, "the old way stays as a fallback");
        assert_eq!(Paint::parse(Some(" old ")), Paint::Scaled);
        assert_eq!(Paint::parse(Some("typo")), Paint::Native);
        assert_eq!(Paint::parse(Some("native")), Paint::Native);
        assert_eq!(paint(), Paint::Native, "tests paint as players get it unless they say otherwise");
        for (panel, n) in [((640, 480), Some(2)), ((720, 480), Some(2)), ((720, 720), Some(2)), ((1280, 720), None), ((1024, 768), None), ((1240, 1080), None)] {
            assert_eq!(native_divisor_for(Paint::Native, panel), n, "{panel:?}");
            assert_eq!(native_divisor_for(Paint::Scaled, panel), None);
        }
        set_paint(Paint::Native);
        let mut frame = ScaledFrame::default();
        let mut output = vec![0; 640 * 480];
        let out = output.as_ptr();
        frame.render(&mut output, (640, 480), &mut |c| {
            // The pages see the same 1280x960 as before; the pixels are the panel's own.
            assert_eq!((c.w, c.h, c.n(), c.stride()), (1280, 960, 2, 640));
            assert_eq!(c.pixels().as_ptr(), out);
            assert_eq!(step(), 2, "a pixel font still knows two design pixels are one of the screen's");
            c.rect(0, 0, 640, 960, 0xffffff);
        });
        assert!(frame.pixels.is_empty(), "no design-size buffer");
        assert_eq!((output[0], output[319], output[320]), (0xffffff, 0xffffff, 0));
        // No whole design frame is kept to paint the Showcase's TV over (`render_partial`): the caller paints a whole one.
        frame.retain = true;
        frame.render(&mut output, (640, 480), &mut |c| c.clear(0));
        assert!(!frame.can_partial((640, 480)));
        assert!(!frame.render_partial(&mut output, (640, 480), &mut |_, _, _| panic!("nothing to paint over")));
    }
}

/// Android uses density-independent units for readable type, preserving the usable viewport's aspect.
/// This leaves landscape room for a detail panel and portrait room for additional rows, without stretching.
pub fn mobile_size((w, h): (u32, u32), density: f32) -> (u32, u32) {
    if w == 0 || h == 0 { return (w.max(1), h.max(1)); }
    let density = if density.is_finite() && density > 0.0 { density } else { 1.0 };
    let short = ((w.min(h) as f32 / density * 2.2).round() as u32).clamp(800, 1280);
    if w < h { (short, (u64::from(h) * u64::from(short) / u64::from(w)) as u32) }
    else { ((u64::from(w) * u64::from(short) / u64::from(h)) as u32, short) }
}

#[cfg(test)]
mod mobile_tests {
    #[test]
    fn phone_rotation_preserves_aspect_and_density_controls_readability() {
        let portrait = super::mobile_size((1216, 2640), 3.0);
        let landscape = super::mobile_size((2640, 1216), 3.0);
        assert_eq!(portrait, (landscape.1, landscape.0));
        assert!((800..=1000).contains(&portrait.0));
        assert!((portrait.1 as f64 / portrait.0 as f64 - 2640.0 / 1216.0).abs() < 0.002);
        assert!(super::mobile_size((1216,2640), 4.0).0 < portrait.0);
    }
}
