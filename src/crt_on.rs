//! The old-TV power-on at the end of Cozy's launch animation: when the game's window comes up (`startup::HOLD_UNTIL`
//! has run out), its first 300 ms start dark, a bright line grows out of the middle of the screen, and the picture opens
//! out of it, vertically, squeezed into what is lit as a tube's picture is while its sweep comes up (the reverse of
//! `eject::tv_off`), with a glow at the two edges that settles. A handheld's game (Game Boy, GBA, DS, PSP: no TV) gets a
//! display's start instead: its console's power light and display came on in the launch scene (`insert_scene`), the screen
//! went dark with the fade, and the picture comes up out of the dark, no line ([`Screen::Lcd`]). The game itself runs
//! meanwhile (the window is no longer held back, nothing waits for the effect); a button press ends it at once.
//!
//! Only in Cozy with Motion on Full: pocket-ui sets [`ENV`] for pocket-core then (`1` a TV, [`LCD`] a handheld's
//! display). `plan` is the whole effect as rectangles for a moment in time, `picture` where the squeezed picture goes;
//! pocket-core draws the picture there and the rectangles as a handful of solid quads (`Overlay::draw_solid`), so the
//! cost is a few fill-rate passes over a part of the screen and no shader of its own.
//!
//!
//! Smooth at 60 Hz: the line grows instead of standing still, and the opening starts and ends softly, so no frame
//! opens more than about a tenth of the picture (`tests::no_frame_jumps_at_60_hz`; the first version opened a quarter of
//! it in one frame after four still ones).

use std::time::Duration;

/// Set by pocket-ui for a pocket-core game whose launch is Cozy with Motion Full; absent in Quick, Reduced and Off.
/// `1`: an old TV; [`LCD`]: a handheld's own display.
pub const ENV: &str = "POCKET_LAUNCH_CRT";
/// [`ENV`]'s value for a handheld's game: its display switches on, no tube.
pub const LCD: &str = "lcd";
/// `1`: the effect without its sound (Settings → Screen & sound → Case sounds off, `cozy_extras`).
pub const SILENT_ENV: &str = "POCKET_LAUNCH_CRT_SILENT";

/// The name the CRT sound hooks in on: [`start`] writes it as a trace event (`POCKETUI_TRACE`) and as the line
/// `crt-on` on stdout of pocket-core the moment the effect begins. The sound agent plays its sample at that point.
pub const HOOK: &str = "crt-on";

/// How long the effect lasts on a TV.
pub const DURATION: Duration = Duration::from_millis(460);

/// The tube's moments, as fractions of [`DURATION`]: a bright dot in the dark until [`DOT_END`], it stretches to a line
/// until [`LINE_END`], the line opens to the AV channel's blue until [`OPEN_END`], the blue (with a little snow at its
/// start) until [`SIGNAL_AT`], then the game's picture comes through it.
/// In ms: the dot 0-35, the line 35-90, opening 90-270 (no faster: no frame opens more than a tenth of the picture),
/// snow on the blue 270-320, the blue alone to 330, the picture coming through it 330-460.
const DOT_END: f32 = 35.0 / 460.0;
const LINE_END: f32 = 90.0 / 460.0;
const OPEN_END: f32 = 270.0 / 460.0;
const SIGNAL_AT: f32 = 330.0 / 460.0;
/// The snow on the blue lasts this long after it opened (fraction of [`DURATION`]).
const SNOW_LEN: f32 = 50.0 / 460.0;
/// The line is this many pixels thick at 720 px of screen height (scaled with the screen).
const LINE_PX: f32 = 3.0;
/// An old TV's empty AV channel: the blue it shows before the console's signal comes in.
pub const AV_BLUE: u32 = 0x1c34c8;
/// A handheld's picture comes up out of the dark this long: dark for [`LCD_HOLD`] of it, then clearing. Shorter than the
/// tube's [`DURATION`]: the launch scene already did the switching on (its power light and display).
pub const LCD_LEN: Duration = Duration::from_millis(200);
const LCD_HOLD: f32 = 0.1;
/// What the game is shown on, for its switch-on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    /// An old TV: a line, the picture opening out of it.
    Tv,
    /// A handheld's own display: its picture comes up out of the dark.
    Lcd,
}

impl Screen {
    /// [`ENV`]'s value for it.
    pub fn env_value(self) -> &'static str {
        match self {
            Screen::Tv => "1",
            Screen::Lcd => LCD,
        }
    }
}

/// Whether pocket-ui asked for the effect.
pub fn wanted() -> bool {
    std::env::var_os(ENV).is_some_and(|v| v == "1" || v == LCD)
}

/// What pocket-ui asked for: a handheld's display or an old TV.
pub fn screen() -> Screen {
    if std::env::var_os(ENV).is_some_and(|v| v == LCD) {
        Screen::Lcd
    } else {
        Screen::Tv
    }
}

/// Whether pocket-ui asked for it without its sound.
pub fn silent() -> bool {
    std::env::var_os(SILENT_ENV).is_some_and(|v| v == "1")
}

/// How long the effect lasts on `screen`.
pub fn len(screen: Screen) -> Duration {
    match screen {
        Screen::Tv => DURATION,
        Screen::Lcd => LCD_LEN,
    }
}

/// The effect starts now: the hook for the sound.
pub fn start() {
    crate::mark!(HOOK, "start");
    println!("{HOOK}");
}

/// The switch-on's sound at `rate` (mono, full UI volume): the tube's relay click and degauss (`Sound::CrtOn`) from the
/// start, and the empty channel's hiss (`Sound::TvStatic`) from the moment the blue opens, fading as the console's signal
/// comes through it (its picture, [`SIGNAL_AT`]) and gone when the picture is whole. One buffer, so the game's sound
/// mixes it in as one chime (pocket-core).
pub fn sound(rate: u32) -> Vec<i16> {
    use crate::ui_sound::{synth_at, Sound};
    let mut out = synth_at(Sound::CrtOn, rate);
    let hiss = synth_at(Sound::TvStatic, rate);
    let at = |f: f32| (f * DURATION.as_secs_f32() * rate as f32) as usize;
    let (from, fade, end) = (at(OPEN_END), at(SIGNAL_AT), at(1.0));
    if out.len() < end {
        out.resize(end, 0);
    }
    for (k, h) in hiss.iter().enumerate().take(end.saturating_sub(from)) {
        let i = from + k;
        let g = if i < fade { 1.0 } else { 1.0 - (i - fade) as f32 / (end - fade).max(1) as f32 };
        out[i] = (out[i] as i32 + (*h as f32 * g) as i32).clamp(i16::MIN as i32, i16::MAX as i32) as i16;
    }
    out
}

/// A solid rectangle over the game: x, y, w, h in pixels from the top left, `white` or black (or `color`, 0xRRGGBB,
/// when set: the AV channel's blue), opacity 0..=1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Quad {
    pub rect: (f32, f32, f32, f32),
    pub white: bool,
    pub alpha: f32,
    pub color: Option<u32>,
}

impl Quad {
    /// White or black.
    pub const fn new(rect: (f32, f32, f32, f32), white: bool, alpha: f32) -> Quad {
        Quad { rect, white, alpha, color: None }
    }

    /// In the colour `rgb` (0xRRGGBB).
    pub const fn colored(rect: (f32, f32, f32, f32), rgb: u32, alpha: f32) -> Quad {
        Quad { rect, white: false, alpha, color: Some(rgb) }
    }

    /// Its colour as 0xRRGGBB.
    pub fn rgb(&self) -> u32 {
        self.color.unwrap_or(if self.white { 0xffffff } else { 0 })
    }

    /// Its colour as red, green, blue 0..=1.
    pub fn rgb_f32(&self) -> [f32; 3] {
        let c = self.rgb();
        [(c >> 16 & 0xff) as f32 / 255.0, (c >> 8 & 0xff) as f32 / 255.0, (c & 0xff) as f32 / 255.0]
    }
}

/// Soft at both ends, steepest (1.5) in the middle.
fn smooth(p: f32) -> f32 {
    let p = p.clamp(0.0, 1.0);
    p * p * (3.0 - 2.0 * p)
}

/// The line's thickness on a picture `view_h` high.
fn line_px(view_h: f32) -> f32 {
    (LINE_PX * view_h / 720.0).max(2.0)
}

/// Half the height of the lit part at `t` (fraction of [`DURATION`]) on a picture `view_h` high: the line, then the
/// picture opening, soft at its start and at its end.
fn half_lit(t: f32, view_h: f32) -> f32 {
    let line = line_px(view_h);
    if t < LINE_END {
        return line / 2.0;
    }
    let p = (t - LINE_END) / (OPEN_END - LINE_END);
    line / 2.0 + (view_h / 2.0 - line / 2.0) * smooth(p)
}

/// Where the game's picture is drawn `since` its first frame, over its `area` (x, y, w, h from the top left): squeezed
/// into the lit part while the tube's picture opens, the whole area before and after (and always on a handheld).
pub fn picture(screen: Screen, since: Duration, area: (f32, f32, f32, f32)) -> (f32, f32, f32, f32) {
    let (ax, ay, w, h) = area;
    let t = since.as_secs_f32() / DURATION.as_secs_f32();
    if screen == Screen::Lcd || t >= OPEN_END || h < 1.0 {
        return area;
    }
    let half = half_lit(t, h);
    (ax, ay + h / 2.0 - half, w, 2.0 * half)
}

/// The rectangles to draw over the game's picture `area` (x, y, w, h in pixels from the top left; the frame around
/// it is not part of the effect) `since` its first frame, for an old TV; empty once the effect is over.
pub fn plan(since: Duration, area: (f32, f32, f32, f32)) -> Vec<Quad> {
    plan_for(Screen::Tv, since, area)
}

/// [`plan`] for what the game is shown on.
pub fn plan_for(screen: Screen, since: Duration, area: (f32, f32, f32, f32)) -> Vec<Quad> {
    let (ax, ay, view_w, view_h) = area;
    let t = since.as_secs_f32() / len(screen).as_secs_f32();
    if t >= 1.0 || view_w < 1.0 || view_h < 1.0 {
        return Vec::new();
    }
    let at = |(x, y, w, h): (f32, f32, f32, f32)| (ax + x, ay + y, w, h);
    if screen == Screen::Lcd {
        // Dark a moment (the scene's fade left it so), then the picture comes up softly over the whole picture: no line, no glow.
        let dark = 1.0 - smooth((t - LCD_HOLD) / (1.0 - LCD_HOLD));
        return if dark > 0.0 { vec![Quad::new(at((0.0, 0.0, view_w, view_h)), false, dark)] } else { Vec::new() };
    }
    let mid = view_h / 2.0;
    let line = line_px(view_h);
    let mut quads = Vec::with_capacity(12);
    let black = |r| Quad::new(at(r), false, 1.0);
    if t < LINE_END {
        // A bright dot in the dark, a little bigger than the line and with a halo, that stretches into a line from the
        // middle to both sides, fast at first, getting as thin as the line as it goes.
        let dot = line * 3.0;
        let (lw, lh, glow, halo) = if t < DOT_END {
            let p = smooth(t / DOT_END);
            (dot * (0.6 + 0.4 * p), dot * (0.6 + 0.4 * p), line * (2.0 + 3.0 * p), 0.3)
        } else {
            let grown = 1.0 - (1.0 - (t - DOT_END) / (LINE_END - DOT_END)).powi(2);
            ((dot + (view_w - dot) * grown).min(view_w), dot + (line - dot) * grown, line * (5.0 - 2.0 * grown), 0.3 - 0.18 * grown)
        };
        let (lx, ly) = (view_w / 2.0 - lw / 2.0, mid - lh / 2.0);
        quads.push(black((0.0, 0.0, view_w, ly)));
        quads.push(black((0.0, ly + lh, view_w, view_h - ly - lh)));
        quads.push(black((0.0, ly, lx, lh)));
        quads.push(black((lx + lw, ly, view_w - lx - lw, lh)));
        quads.push(Quad::new(at((lx - glow, ly - glow, lw + 2.0 * glow, lh + 2.0 * glow).clip((0.0, 0.0, view_w, view_h))), true, halo));
        quads.push(Quad::new(at((lx, ly, lw, lh)), true, 1.0));
        return quads;
    }
    let half = half_lit(t, view_h);
    // Dark above and below the lit part.
    for (y, h) in [(0.0, mid - half), (mid + half, mid - half)] {
        if h > 0.0 {
            quads.push(black((0.0, y, view_w, h)));
        }
    }
    let lit = (0.0, mid - half, view_w, 2.0 * half);
    // The empty AV channel's blue, until the console's signal comes through it.
    let blue = 1.0 - smooth((t - SIGNAL_AT) / (1.0 - SIGNAL_AT));
    if blue > 0.0 {
        quads.push(Quad::colored(at(lit), AV_BLUE, blue));
    }
    if t < OPEN_END {
        // White at first (the line), the blue showing through more and more as it opens.
        let open = (t - LINE_END) / (OPEN_END - LINE_END);
        let flash = (1.0 - open).powi(2) * 0.9;
        if flash > 0.0 {
            quads.push(Quad::new(at(lit), true, flash));
        }
    } else if t < OPEN_END + SNOW_LEN {
        // A moment of snow on the blue: a few thin streaks where the tuner finds no picture yet, a new place every
        // frame, fading as the channel settles.
        let p = (t - OPEN_END) / SNOW_LEN;
        let frame = (t * DURATION.as_secs_f32() * 60.0) as u32;
        for i in 0..3u32 {
            let r = hash(frame * 7 + i);
            let h = line * (1.0 + (r >> 16 & 3) as f32);
            let y = (r & 0xffff) as f32 / 65535.0 * (view_h - h).max(0.0);
            quads.push(Quad::new(at((0.0, y, view_w, h)), true, 0.3 * (1.0 - p)));
        }
    }
    // A brighter band rolls down once over the blue and the picture coming in: the sweep finding its sync.
    if t >= OPEN_END {
        let p = (t - OPEN_END) / (1.0 - OPEN_END);
        let bh = view_h * 0.14;
        let y = -bh + (view_h + bh) * p;
        let (y0, y1) = (y.max(0.0), (y + bh).min(view_h));
        let a = 0.08 * (1.0 - smooth((p - 0.6) / 0.4));
        if y1 > y0 && a > 0.0 {
            quads.push(Quad::new(at((0.0, y0, view_w, y1 - y0)), true, a));
        }
    }
    // Glow: three soft bands inside the picture at each edge of the lit part, brightest at the line, settling once
    // it is open.
    let settle = 1.0 - smooth((t - OPEN_END) / (SIGNAL_AT - OPEN_END));
    let band = (view_h * 0.035).max(6.0);
    for i in 0..3 {
        let a = 0.34 * settle / (1.0 + i as f32 * 1.6);
        let inner = (band * (i as f32 + 1.0)).min(half);
        if inner <= 0.0 || a <= 0.0 {
            continue;
        }
        quads.push(Quad::new(at((0.0, mid - half, view_w, inner)), true, a));
        quads.push(Quad::new(at((0.0, mid + half - inner, view_w, inner)), true, a));
    }
    quads
}

/// A cheap integer hash: the snow's places.
fn hash(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb_352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846c_a68b);
    x ^ x >> 16
}

trait Clip {
    fn clip(self, area: (f32, f32, f32, f32)) -> Self;
}

impl Clip for (f32, f32, f32, f32) {
    /// The part of the rectangle inside `area`.
    fn clip(self, (ax, ay, aw, ah): (f32, f32, f32, f32)) -> Self {
        let (x0, y0) = (self.0.max(ax), self.1.max(ay));
        let (x1, y1) = ((self.0 + self.2).min(ax + aw), (self.1 + self.3).min(ay + ah));
        (x0, y0, (x1 - x0).max(0.0), (y1 - y0).max(0.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    /// A quad's brightness 0..=1.
    fn luma(q: &Quad) -> f32 {
        let [r, g, b] = q.rgb_f32();
        0.3 * r + 0.59 * g + 0.11 * b
    }

    /// The plan composited over a test picture (every pixel 0x808080) on a w x h screen, as grey levels per row.
    pub(crate) fn rows(since: Duration, w: f32, h: f32) -> Vec<f32> {
        let mut y = vec![0.5f32; h as usize];
        for q in plan(since, (0.0, 0.0, w, h)) {
            let v = luma(&q);
            for (row, px) in y.iter_mut().enumerate() {
                let r = row as f32 + 0.5;
                if r >= q.rect.1 && r < q.rect.1 + q.rect.3 {
                    *px = *px * (1.0 - q.alpha) + v * q.alpha;
                }
            }
        }
        y
    }

    /// One whole frame `since` the first on a w x h screen, grey levels 0..=1, as pocket-core composites it: the game's
    /// picture (rows of 0.25 and 0.75 stripes, 8 rows each, so a squeeze shows) drawn into `picture`, the quads over it.
    fn frame(screen: Screen, since: Duration, w: usize, h: usize) -> Vec<f32> {
        let area = (0.0, 0.0, w as f32, h as f32);
        let (_, py, _, ph) = picture(screen, since, area);
        let mut px = vec![0.0f32; w * h];
        for y in 0..h {
            let r = y as f32 + 0.5;
            if r < py || r >= py + ph || ph < 1.0 {
                continue;
            }
            let src = ((r - py) / ph * h as f32) as usize;
            let v = if (src / 8) % 2 == 0 { 0.25 } else { 0.75 };
            px[y * w..(y + 1) * w].iter_mut().for_each(|p| *p = v);
        }
        for q in plan_for(screen, since, area) {
            let v = luma(&q);
            let (x0, y0) = (q.rect.0.round().max(0.0) as usize, q.rect.1.round().max(0.0) as usize);
            let (x1, y1) = (((q.rect.0 + q.rect.2).round() as usize).min(w), ((q.rect.1 + q.rect.3).round() as usize).min(h));
            for y in y0..y1 {
                for p in &mut px[y * w + x0..y * w + x1.max(x0)] {
                    *p = *p * (1.0 - q.alpha) + v * q.alpha;
                }
            }
        }
        px
    }

    /// Per 60 Hz frame over the effect and a frame after it: (mean brightness, how much of the picture is lit or how
    /// dark the display still is, whether anything changed from the frame before).
    fn series(screen: Screen, w: usize, h: usize) -> Vec<(f32, f32, bool)> {
        let tick = |k: u32| Duration::from_nanos(k as u64 * 16_666_667);
        let area = (0.0, 0.0, w as f32, h as f32);
        let mut last: Option<Vec<f32>> = None;
        (0..=29)
            .map(|k| {
                let f = frame(screen, tick(k), w, h);
                let light = f.iter().sum::<f32>() / f.len() as f32;
                let open = match screen {
                    Screen::Tv => picture(screen, tick(k), area).3 / h as f32,
                    Screen::Lcd => 1.0 - plan_for(screen, tick(k), area).first().map_or(0.0, |q| q.alpha),
                };
                let moved = last.as_ref().is_none_or(|l| l != &f);
                last = Some(f);
                (light, open, moved)
            })
            .collect()
    }

    /// 11.11.: the line stood still for four frames, then a quarter of the picture opened in one frame (a step of 52 of
    /// 255 per channel; the picture switching off on the way back is 29 at most). Now every 60 Hz frame moves and none
    /// jumps: the picture opens (or a display's light comes up) by at most 14 % a frame, its brightness by at most 0.09,
    /// on every screen shape the handhelds have.
    #[test]
    fn no_frame_jumps_at_60_hz() {
        for (w, h) in [(640, 480), (1240, 1080), (1280, 720), (1024, 768)] {
            for screen in [Screen::Tv, Screen::Lcd] {
                let s = series(screen, w, h);
                for (k, p) in s.windows(2).enumerate() {
                    let (dl, dopen) = ((p[1].0 - p[0].0).abs(), (p[1].1 - p[0].1).abs());
                    assert!(dopen <= 0.14 && dl <= 0.09, "{screen:?} {w}x{h}: frame {} jumps: opens {dopen:.3}, light {dl:.3}\n{s:.3?}", k + 1);
                }
                // Until it is over, every frame changes something: nothing stands still.
                // The display's light covers the picture for its first 20 ms (two frames) and is over after 12 frames.
                let (live, first) = if screen == Screen::Tv { (27, 1) } else { (11, 3) };
                let still = s.iter().take(live).skip(first).position(|f| !f.2);
                assert!(still.is_none(), "{screen:?} {w}x{h}: frame {} is the same as the one before: {s:.3?}", still.unwrap_or(0) + first);
            }
        }
    }

    #[test]
    fn the_picture_opens_squeezed_out_of_the_line_and_is_whole_after() {
        let area = (10.0, 20.0, 1280.0, 720.0);
        let mut last = 0.0;
        for k in 0..=18 {
            let (x, y, w, h) = picture(Screen::Tv, ms(k * 16), area);
            assert!(x == 10.0 && w == 1280.0, "only its height changes");
            assert!((y + h / 2.0 - 380.0).abs() < 0.5, "{k}: about the middle");
            assert!(h + 0.01 >= last, "{k}: never closes again");
            last = h;
        }
        assert!(picture(Screen::Tv, ms(20), area).3 < 8.0, "a line at first");
        assert_eq!(picture(Screen::Tv, ms(280), area), area, "whole once open");
        assert_eq!(picture(Screen::Lcd, ms(20), area), area, "a handheld's display is never squeezed");
    }

    #[test]
    fn a_handhelds_picture_comes_up_out_of_the_dark_without_a_line_and_never_flashes_light() {
        let area = (0.0, 0.0, 640.0, 480.0);
        for t in (0..200).step_by(10) {
            let quads = plan_for(Screen::Lcd, ms(t), area);
            assert_eq!(quads.len(), 1, "{t} ms: one cover, no line, no glow, no light bands");
            assert!(!quads[0].white && quads[0].rect == area, "{t} ms: black over the picture, {:?}", quads[0]);
        }
        let dark = |t| plan_for(Screen::Lcd, ms(t), area).first().map_or(0.0, |q| q.alpha);
        assert!(dark(0) > 0.99 && dark(10) > 0.99, "dark at first (the scene's fade left it so)");
        assert!(dark(100) > 0.2 && dark(100) < 0.8, "clearing halfway: {}", dark(100));
        assert!(dark(190) < 0.05, "clear before the end");
        assert!(plan_for(Screen::Lcd, LCD_LEN, area).is_empty());
        assert!(LCD_LEN < DURATION, "shorter than the tube: the scene did the switching on");
    }

    #[test]
    fn the_line_grows_out_of_the_middle_instead_of_standing() {
        let lit_width = |t| {
            plan(ms(t), (0.0, 0.0, 1280.0, 720.0)).iter().filter(|q| q.white && q.alpha >= 1.0).map(|q| q.rect.2).fold(0.0, f32::max)
        };
        // A dot first (growing a little, its halo with it), then the line grows out of it.
        let (dot, a, b, c) = (lit_width(0), lit_width(45), lit_width(60), lit_width(80));
        assert!(dot > 0.0 && dot <= lit_width(20) && lit_width(20) < a && a < b && b < c && c <= 1280.0, "{dot} {a} {b} {c}");
        let halo = |t| plan(ms(t), (0.0, 0.0, 1280.0, 720.0)).iter().filter(|q| q.white && q.alpha < 1.0).map(|q| q.rect.2).fold(0.0, f32::max);
        assert!(halo(0) < halo(20), "the dot's halo grows: {} {}", halo(0), halo(20));
        let q = plan(ms(50), (0.0, 0.0, 1280.0, 720.0)).into_iter().find(|q| q.white).unwrap();
        assert!((q.rect.0 + q.rect.2 / 2.0 - 640.0).abs() < 0.5, "from the middle");
    }

    /// 19.11. (Ritual TV): a dot, a line, the empty AV channel's blue with a moment of snow, then the console's signal:
    /// the game's picture comes through the blue.
    #[test]
    fn a_dot_a_line_the_av_blue_then_the_signal() {
        let area = (0.0, 0.0, 1280.0, 720.0);
        let lit = |t| plan(ms(t), area).into_iter().filter(|q| q.white && q.alpha >= 1.0).map(|q| q.rect).next();
        let dot = lit(10).unwrap();
        assert!(dot.2 <= dot.3 * 1.5, "a dot first: {dot:?}");
        let line = lit(85).unwrap();
        assert!(line.2 > 400.0 && line.3 < 6.0, "then a thin line: {line:?}");
        let blue = |t| plan(ms(t), area).iter().find(|q| q.color == Some(AV_BLUE)).map_or(0.0, |q| q.alpha * q.rect.3 / 720.0);
        assert!(blue(300) > 0.99, "the whole picture blue once open: {}", blue(300));
        let streaks = |t| plan(ms(t), area).iter().filter(|q| q.white && q.rect.3 < 20.0 && q.rect.2 == 1280.0).count();
        assert!(streaks(280) >= 3, "snow on the blue at first");
        assert_eq!(streaks(325), 0, "then the blue alone");
        assert!(blue(400) > 0.1 && blue(400) < 0.9, "the signal coming through: {}", blue(400));
        assert!(blue(458) < 0.01 && plan(DURATION, area).is_empty());
    }

    /// The relay click and degauss from the start, the hiss only while the channel is empty: it begins as the blue opens
    /// and is gone once the picture is whole.
    #[test]
    fn the_hiss_belongs_to_the_empty_channel() {
        use crate::ui_sound::{synth_at, Sound};
        let rate = 48_000;
        let (with, without) = (sound(rate), synth_at(Sound::CrtOn, rate));
        let at = |ms: u64| (ms as usize) * rate as usize / 1000;
        assert_eq!(&with[..at(260)], &without[..at(260)], "the click and degauss alone before the blue");
        assert!(with[at(280)..at(320)].iter().zip(&without[at(280)..at(320)]).any(|(a, b)| a != b), "the hiss on the blue");
        let after = with.get(at(470)..).unwrap_or(&[]);
        let plain = without.get(at(470)..).unwrap_or(&[]);
        assert!(after.iter().zip(plain).all(|(a, b)| a == b), "no hiss once the picture is whole");
    }

    #[test]
    fn the_env_says_tv_or_handheld() {
        assert_eq!(Screen::Tv.env_value(), "1");
        assert_eq!(Screen::Lcd.env_value(), LCD);
    }

    #[test]
    fn starts_dark_with_one_bright_line_in_the_middle() {
        let r = rows(ms(10), 1280.0, 720.0);
        assert!(r[0] < 0.01 && r[719] < 0.01, "dark above and below");
        assert!(r[360] > 0.95 && r[359] > 0.95, "the line is bright");
        assert!(r.iter().filter(|v| **v > 0.5).count() <= 10, "a small dot");
    }

    #[test]
    fn opens_to_the_picture_and_ends_with_nothing_drawn() {
        let mid = rows(ms(150), 1280.0, 720.0);
        let lit = mid.iter().filter(|v| **v > 0.3).count();
        assert!(lit > 30 && lit < 700, "partly open halfway ({lit} rows)");
        assert!(mid[0] < 0.01, "the edge is still dark");
        let late = rows(ms(455), 1280.0, 720.0);
        assert!(late.iter().all(|v| (v - 0.5).abs() < 0.08), "the picture, the blue and the glow almost gone: {:?}", &late[..3]);
        assert!(plan(DURATION, (0.0, 0.0, 1280.0, 720.0)).is_empty());
        assert!(plan(ms(900), (0.0, 0.0, 1280.0, 720.0)).is_empty());
    }

    #[test]
    fn the_opening_only_grows_and_stays_inside_the_screen() {
        let mut last = 0;
        // From the line on (the dot before it is a little thicker than the line it stretches into).
        for t in (90..480).step_by(10) {
            let r = rows(ms(t), 640.0, 480.0);
            // Lit: the line, the opening blue, the picture; not the faint halo round the dot.
            let lit = r.iter().filter(|v| **v > 0.15).count();
            assert!(lit + 1 >= last, "never closes again ({t} ms: {lit} < {last})");
            last = lit;
            for q in plan(ms(t), (0.0, 0.0, 640.0, 480.0)) {
                assert!(q.rect.1 >= -0.01 && q.rect.1 + q.rect.3 <= 480.01 && q.alpha <= 1.0, "{q:?}");
            }
        }
    }

    #[test]
    fn stays_inside_the_picture_not_the_frame_around_it() {
        let area = (160.0, 20.0, 960.0, 680.0);
        for t in (0..480).step_by(5) {
            for q in plan(ms(t), area) {
                let (x, y, w, h) = q.rect;
                assert!(x >= area.0 && x + w <= area.0 + area.2 + 0.01 && y >= area.1 - 0.01 && y + h <= area.1 + area.3 + 0.01, "{t} ms: {q:?}");
            }
        }
    }

    #[test]
    fn cheap_a_handful_of_quads() {
        for t in (0..480).step_by(5) {
            assert!(plan(ms(t), (0.0, 0.0, 1280.0, 720.0)).len() <= 12);
        }
    }

    #[test]
    fn the_hook_name_is_stable_for_the_sound() {
        assert_eq!(HOOK, "crt-on");
        assert_eq!(ENV, "POCKET_LAUNCH_CRT");
    }
}
