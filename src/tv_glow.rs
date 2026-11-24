//! The evening TV glow (Settings → Look → Cozy extras → Evening TV glow): after dark by the Pocket's clock, while the
//! TV on the cabinet is on, the room is a little darker and the TV's warm light falls on what is round it, the top
//! board, the shelves and their cases, strongest near the TV and gone a couple of TV widths away. Gentle on purpose:
//! at most [`MAX`] of the way towards [`WARM`] on the pixels nearest the TV, so covers and spines stay readable, and the
//! title and hints are painted after it, untouched.
//!
//! Cheap by construction: the light is a handful of vertical strips, each mixed towards [`WARM`] row by row
//! (`Canvas::shade_rows`), so every pixel under it is touched once and no picture is made; [`Glow`] is kept until the
//! TV moves, the screen changes or the darkness steps (it is quantised, so the clock moves it a few times an evening).
//! Its pixels are worked out once for the screen (`Canvas::shade_plan`), so a frame mixes each screen row in one run
//! rather than 24 short ones: strip by strip it was 3.6 ms of a move on the RG35XX Plus.
//! The room's dimming is the room's light ([`dimmed`]): the wood is tinted once for it, as for the time of day.

use crate::gfx::{Canvas, ShadePlan};
use crate::room_light::Light;

/// The TV's light: a warm, slightly orange white, as a tube's picture looks from across a dark room.
pub const WARM: u32 = 0xffb068;
/// How far towards [`WARM`] the pixels nearest the TV go at full darkness (0-255).
pub const MAX: u8 = 40;
/// The room's light at full darkness while the TV is on: this much of it (the cases get half, `Light::case_shade`).
pub const DIM: f32 = 0.9;
/// How many strips across the glow: the steps between two are a fraction of [`MAX`], too small to see.
const STRIPS: i32 = 24;
/// The glow's reach as multiples of the TV's width: sideways, upwards and downwards (the light falls on the shelves).
const REACH: (f32, f32, f32) = (2.4, 0.9, 2.2);

/// How dark it is outside at `minute` after midnight (0 day to 1 night): with the room's lamp (`room_light`), so the
/// evening comes in over the same hour and a half, and quantised to eighths so a frame never sees it creep.
pub fn darkness(minute: f32) -> f32 {
    let lamp = crate::room_light::at(minute, false).lamp as f32 / 255.0;
    let d = ((lamp - 0.3) / 0.6).clamp(0.0, 1.0);
    (d * d * (3.0 - 2.0 * d) * 8.0).round() / 8.0
}

/// The room's light `l` with the TV on at darkness `dark`: a little dimmer, its tint kept.
pub fn dimmed(l: Light, dark: f32) -> Light {
    let k = 1.0 - (1.0 - DIM) * dark.clamp(0.0, 1.0);
    let g = |v: u16| (v as f32 * k).round() as u16;
    Light { r: g(l.r), g: g(l.g), b: g(l.b), lamp: l.lamp }
}

/// The glow for one place of the TV: strips of rows with how far each row goes towards [`WARM`].
#[derive(Clone, Debug, PartialEq)]
pub struct Glow {
    key: (i32, i32, i32, i32, i32, i32, u8),
    strips: Vec<(i32, i32, i32, Vec<u8>)>,
    /// Its pixels on the screen it was last painted on (`Canvas::shade_plan`).
    plan: Option<ShadePlan>,
}

impl Glow {
    /// The glow of a TV whose picture is centred at `(cx, cy)` and `tv_w` wide, on a `w` x `h` screen, at `strength`
    /// (0-1: the darkness, halved on the light page). Rows above `top` are left alone (the room above the cabinet).
    pub fn new((cx, cy): (i32, i32), tv_w: i32, (w, h): (i32, i32), top: i32, strength: f32) -> Glow {
        let peak = (MAX as f32 * strength.clamp(0.0, 1.0)).round() as u8;
        let key = (cx, cy, tv_w, w, h, top, peak);
        let mut strips = Vec::new();
        if peak == 0 || tv_w <= 0 {
            return Glow { key, strips, plan: None };
        }
        let tw = tv_w as f32;
        let (rx, up, down) = (tw * REACH.0, tw * REACH.1, tw * REACH.2);
        let (x0, x1) = (((cx as f32 - rx) as i32).max(0) & !1, ((cx as f32 + rx) as i32).min(w));
        let (y0, y1) = (((cy as f32 - up) as i32).max(top).max(0), ((cy as f32 + down) as i32).min(h));
        if x0 >= x1 || y0 >= y1 {
            return Glow { key, strips, plan: None };
        }
        // Even widths, so a strip's edges fall between screen pixels at 2 design pixels a screen pixel too.
        let sw = (((x1 - x0) + STRIPS - 1) / STRIPS + 1) & !1;
        let mut x = x0;
        while x < x1 {
            let ww = sw.min(x1 - x);
            let dx = ((x + ww / 2) as f32 - cx as f32) / rx;
            let rows: Vec<u8> = (y0..y1)
                .map(|y| {
                    let dy = (y as f32 - cy as f32) / if y < cy { up } else { down };
                    let d = (dx * dx + dy * dy).sqrt();
                    let k = (1.0 - d).max(0.0);
                    (peak as f32 * k * k).round() as u8
                })
                .collect();
            if rows.iter().any(|a| *a > 0) {
                strips.push((x, y0, ww, rows));
            }
            x += ww;
        }
        Glow { key, strips, plan: None }
    }

    /// Whether it was made for this place, screen and strength.
    pub fn is_for(&self, (cx, cy): (i32, i32), tv_w: i32, (w, h): (i32, i32), top: i32, strength: f32) -> bool {
        self.key == (cx, cy, tv_w, w, h, top, (MAX as f32 * strength.clamp(0.0, 1.0)).round() as u8)
    }

    /// Paints it over what is there: the strips' `shade_rows`, pixel for pixel, as worked out once for this canvas.
    pub fn paint(&mut self, c: &mut Canvas) {
        if self.plan.as_ref().is_some_and(|p| c.shade_planned(WARM, p)) {
            return;
        }
        let y = self.strips.first().map_or(0, |s| s.1);
        let strips: Vec<(i32, i32, &[u8])> = self.strips.iter().map(|(x, _, w, rows)| (*x, *w, rows.as_slice())).collect();
        self.plan = c.shade_plan(y, &strips);
        if !self.plan.as_ref().is_some_and(|p| c.shade_planned(WARM, p)) {
            self.paint_strips(c);
        }
    }

    /// The strips one by one (`shade_rows`): what `paint` paints.
    pub fn paint_strips(&self, c: &mut Canvas) {
        for (x, y, w, rows) in &self.strips {
            c.shade_rows(*x, *y, *w, WARM, rows);
        }
    }

    /// How far towards [`WARM`] the pixel at `(x, y)` goes (0 outside the glow).
    pub fn at(&self, x: i32, y: i32) -> u8 {
        self.strips
            .iter()
            .find(|(sx, sy, sw, rows)| x >= *sx && x < sx + sw && y >= *sy && y < sy + rows.len() as i32)
            .map_or(0, |(_, sy, _, rows)| rows[(y - sy) as usize])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The glow is mixed over most of the Cabinet's screen in every frame after dark, so it must cost well under painting
    /// the screen's wood once (`cabinet_art::blit`, the walls behind the shelves). Strip by strip (24 short runs per row,
    /// `paint_strips`) it was 0.73x the wood on the Air (0.36x worked out at once) and 3.6 ms of a move in the evening on
    /// the RG35XX Plus (0.6.0:
    /// the Cabinet's p50 13.0 ms, 9.4 ms without the glow). At 640x480 painted natively (2 design pixels a screen pixel),
    /// with the TV about where it stands there. Only in an optimised build (`--profile gate`): unoptimised timings say
    /// nothing.
    #[test]
    fn the_glow_costs_well_under_the_wood_behind_it() {
        if cfg!(debug_assertions) {
            return;
        }
        use crate::cabinet_art::{blit, Pic};
        let (pw, ph) = (640u32, 480u32);
        let (w, h) = (pw as i32 * 2, ph as i32 * 2);
        let wood = Pic::of(w, h, (0..w * h).map(|i| 0xff00_0000 | ((i as u32).wrapping_mul(2_654_435_761) >> 8 & 0xff_ffff)).collect());
        let mut glow = Glow::new((1060, 260), 320, (w, h), 0, 1.0);
        let mut buf = vec![0x30_2418u32; (pw * ph) as usize];
        let mut best = |paint: &mut dyn FnMut(&mut Canvas)| {
            (0..15)
                .map(|_| {
                    let mut c = Canvas::screen(&mut buf, (pw, ph), 2);
                    let t = std::time::Instant::now();
                    paint(&mut c);
                    t.elapsed().as_secs_f64() * 1e6
                })
                .fold(f64::MAX, f64::min)
        };
        let wood_us = best(&mut |c| blit(c, &wood, 0, 0, 230, (0, h)));
        let strips_us = best(&mut |c| glow.paint_strips(c));
        let glow_us = best(&mut |c| glow.paint(c));
        let covered = glow.strips.iter().map(|(_, _, sw, rows)| *sw as usize * rows.iter().filter(|a| **a > 0).count()).sum::<usize>();
        println!(
            "glow {glow_us:.0} us ({:.2}x the wood), strip by strip {strips_us:.0} us ({:.2}x), over {:.0} % of the screen, wood {wood_us:.0} us",
            glow_us / wood_us,
            strips_us / wood_us,
            100.0 * covered as f64 / (w * h) as f64
        );
        assert!(covered * 2 > (w * h) as usize, "the test's glow covers most of the screen, as on the Plus");
        assert!(glow_us <= wood_us * GLOW_PER_WOOD, "the glow {glow_us:.0} us, the wood {wood_us:.0} us: {:.2}x", glow_us / wood_us);
    }
    const GLOW_PER_WOOD: f64 = 0.55;

    /// `paint` is `paint_strips` pixel for pixel: at 1 and 2 design pixels a screen pixel, on a band of the screen, in
    /// the frame that works its pixels out and the ones after it, and again once the canvas changes shape.
    #[test]
    fn painted_at_once_as_strip_by_strip() {
        let (pw, ph) = (160u32, 120u32);
        let start: Vec<u32> = (0..pw * ph).map(|i| i.wrapping_mul(2_654_435_761) >> 8).collect();
        for n in [1u32, 2] {
            let (w, h) = ((pw * n) as i32, (ph * n) as i32);
            let mut glow = Glow::new((w * 3 / 4 + 1, h / 4 + 1), w / 4 + 3, (w, h), 7, 1.0);
            assert!(glow.strips.len() > 10);
            let (mut want, mut got) = (start.clone(), start.clone());
            glow.paint_strips(&mut Canvas::screen(&mut want, (pw, ph), n));
            for frame in 0..3 {
                got.copy_from_slice(&start);
                glow.paint(&mut Canvas::screen(&mut got, (pw, ph), n));
                assert!(got == want, "n = {n}, frame {frame}");
            }
            // A band from an odd design row: another shape, worked out again.
            let (mut want, mut got) = (start.clone(), start.clone());
            glow.paint_strips(&mut Canvas::screen(&mut want, (pw, ph), n).rows(13, h - 9));
            glow.paint(&mut Canvas::screen(&mut got, (pw, ph), n).rows(13, h - 9));
            assert!(got == want, "n = {n}, a band");
            assert_ne!(want, start, "the glow mixed something");
        }
    }

    #[test]
    fn dark_only_in_the_evening_and_at_night() {
        assert_eq!(darkness(12.0 * 60.0), 0.0, "noon");
        assert_eq!(darkness(9.0 * 60.0), 0.0, "morning");
        assert!(darkness(20.0 * 60.0) >= 0.5, "evening: {}", darkness(20.0 * 60.0));
        assert_eq!(darkness(23.5 * 60.0), 1.0, "night");
        assert_eq!(darkness(2.0 * 60.0), 1.0, "night");
        // It comes in over the evening in small steps, never a jump of more than a quarter.
        let mut last = darkness(15.0 * 60.0);
        for m in (15 * 60..24 * 60).step_by(5) {
            let d = darkness(m as f32);
            assert!((d - last).abs() <= 0.25, "{m}: {last} -> {d}");
            last = d;
        }
    }

    #[test]
    fn the_room_dims_a_little_and_keeps_its_tint() {
        let evening = Light { r: 266, g: 248, b: 225, lamp: 200 };
        let d = dimmed(evening, 1.0);
        assert!(d.r < evening.r && d.r as f32 >= evening.r as f32 * DIM - 1.0);
        assert!(d.r > d.g && d.g > d.b, "still warm: {d:?}");
        assert_eq!(dimmed(evening, 0.0), evening, "daytime: unchanged");
    }

    /// Strongest by the TV, gone a couple of TV widths away, never more than [`MAX`], and nothing above the cabinet.
    #[test]
    fn warm_near_the_tv_and_fading_with_distance() {
        let g = Glow::new((500, 120), 100, (640, 480), 40, 1.0);
        let near = g.at(500, 124);
        let mid = g.at(400, 260);
        let far = g.at(120, 400);
        assert!(near >= MAX - 6 && near <= MAX, "{near}");
        assert!(mid > 0 && mid < near, "{mid}");
        assert_eq!(far, 0, "far away: none");
        assert_eq!(g.at(500, 30), 0, "nothing above the cabinet");
        assert!(g.strips.len() <= STRIPS as usize + 1, "a handful of strips");
        // Half as strong on the light page, nothing at noon.
        assert!(Glow::new((500, 120), 100, (640, 480), 40, 0.5).at(500, 124) <= near / 2 + 1);
        assert!(Glow::new((500, 120), 100, (640, 480), 40, 0.0).strips.is_empty());
    }

    /// A pixel next to the TV changes by at most [`MAX`] of 255 towards warm: text and covers keep their contrast.
    #[test]
    fn paint_is_gentle() {
        let (w, h) = (640, 480);
        let mut buf = vec![0x202020u32; (w * h) as usize];
        let mut g = Glow::new((500, 120), 100, (w, h), 0, 1.0);
        g.paint(&mut Canvas::new(&mut buf, w as u32, h as u32));
        let p = buf[(124 * w + 500) as usize];
        let (r, b) = ((p >> 16 & 0xff) as i32, (p & 0xff) as i32);
        assert!(r > 0x20 && r <= 0x20 + 0xdf * MAX as i32 / 255 + 1, "warmer: {p:06x}");
        assert!(r > b, "warm, not white: {p:06x}");
        assert_eq!(buf[(400 * w + 20) as usize], 0x202020, "far away untouched");
        assert!(g.is_for((500, 120), 100, (w, h), 0, 1.0) && !g.is_for((501, 120), 100, (w, h), 0, 1.0));
    }
}
