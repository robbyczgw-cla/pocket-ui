//! The old-TV power-on at the end of Cozy's launch animation: when the game's window comes up (`startup::HOLD_UNTIL`
//! has run out), its first 300 ms start dark, a bright line grows out of the middle of the screen, and the picture opens
//! out of it, vertically, squeezed into what is lit as a tube's picture is while its sweep comes up (the reverse of
//! `eject::tv_off`), with a glow at the two edges that settles. A handheld's game (Game Boy, GBA, DS, PSP: no TV) gets a
//! display switching on instead: dark, then its light coming up, no line ([`Screen::Lcd`]). The game itself runs
//! meanwhile (the window is no longer held back, nothing waits for the effect); a button press ends it at once.
//!
//! Only in Cozy with Motion on Full: pocket-ui sets [`ENV`] for pocket-core then (`1` a TV, [`LCD`] a handheld's
//! display). `plan` is the whole effect as rectangles for a moment in time, `picture` where the squeezed picture goes;
//! pocket-core draws the picture there and the rectangles as a handful of solid quads (`Overlay::draw_solid`), so the
//! cost is a few fill-rate passes over a part of the screen and no shader of its own.
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
/// `1`: the effect without its sound (Settings → Sound & feel → Case sounds off, `cozy_extras`).
pub const SILENT_ENV: &str = "POCKET_LAUNCH_CRT_SILENT";

/// The name the CRT sound hooks in on: [`start`] writes it as a trace event (`POCKETUI_TRACE`) and as the line
/// `crt-on` on stdout of pocket-core the moment the effect begins. The sound agent plays its sample at that point.
pub const HOOK: &str = "crt-on";

/// How long the effect lasts.
pub const DURATION: Duration = Duration::from_millis(300);

/// Up to this moment the screen is dark with only the line growing out of the middle (fraction of [`DURATION`]).
const LINE_END: f32 = 0.2;
/// The picture is fully open at this moment; the glow settles until the end.
const OPEN_END: f32 = 0.85;
/// The line is this many pixels thick at 720 px of screen height (scaled with the screen).
const LINE_PX: f32 = 3.0;
/// A handheld's display: dark this long (fraction of [`DURATION`]), then its light comes up until [`LIT_AT`].
const LCD_DARK: f32 = 0.04;
const LIT_AT: f32 = 0.85;

/// What the game is shown on, for its switch-on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    /// An old TV: a line, the picture opening out of it.
    Tv,
    /// A handheld's own display: its light comes up.
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

/// The effect starts now: the hook for the sound.
pub fn start() {
    crate::mark!(HOOK, "start");
    println!("{HOOK}");
}

/// A solid rectangle over the game: x, y, w, h in pixels from the top left, `white` or black, opacity 0..=1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Quad {
    pub rect: (f32, f32, f32, f32),
    pub white: bool,
    pub alpha: f32,
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
    let t = since.as_secs_f32() / DURATION.as_secs_f32();
    if t >= 1.0 || view_w < 1.0 || view_h < 1.0 {
        return Vec::new();
    }
    let at = |(x, y, w, h): (f32, f32, f32, f32)| (ax + x, ay + y, w, h);
    if screen == Screen::Lcd {
        // Dark a moment, then the light comes up softly over the whole display: no line, no glow.
        let dark = 1.0 - smooth((t - LCD_DARK) / (LIT_AT - LCD_DARK));
        return if dark > 0.0 { vec![Quad { rect: at((0.0, 0.0, view_w, view_h)), white: false, alpha: dark }] } else { Vec::new() };
    }
    let mid = view_h / 2.0;
    let half = half_lit(t, view_h);
    let mut quads = Vec::with_capacity(9);
    // Dark above and below the lit part.
    for (y, h) in [(0.0, mid - half), (mid + half, mid - half)] {
        if h > 0.0 {
            quads.push(Quad { rect: at((0.0, y, view_w, h)), white: false, alpha: 1.0 });
        }
    }
    if t < LINE_END {
        // The line grows out of the middle to both sides, fast at first; dark beside it.
        let grown = 1.0 - (1.0 - t / LINE_END).powi(2);
        let lw = (view_w * (0.04 + 0.96 * grown)).min(view_w);
        let side = (view_w - lw) / 2.0;
        quads.push(Quad { rect: at((0.0, mid - half, side, 2.0 * half)), white: false, alpha: 1.0 });
        quads.push(Quad { rect: at((side + lw, mid - half, side, 2.0 * half)), white: false, alpha: 1.0 });
        quads.push(Quad { rect: at((side, mid - half, lw, 2.0 * half)), white: true, alpha: 1.0 });
    } else {
        // The lit part: white at first (the line), the squeezed picture showing through more and more as it opens.
        let open = ((t - LINE_END) / (OPEN_END - LINE_END)).clamp(0.0, 1.0);
        let flash = (1.0 - open).powi(2) * 0.9;
        if flash > 0.0 {
            quads.push(Quad { rect: at((0.0, mid - half, view_w, 2.0 * half)), white: true, alpha: flash });
        }
    }
    // Glow: three soft bands inside the picture at each edge of the lit part, brightest at the line, fading out and
    // settling over the last stretch.
    let settle = 1.0 - smooth((t - OPEN_END) / (1.0 - OPEN_END));
    let band = (view_h * 0.035).max(6.0);
    if t >= LINE_END {
        for i in 0..3 {
            let a = 0.34 * settle / (1.0 + i as f32 * 1.6);
            let reach = band * (i as f32 + 1.0);
            let inner = reach.min(half);
            if inner <= 0.0 || a <= 0.0 {
                continue;
            }
            quads.push(Quad { rect: at((0.0, mid - half, view_w, inner)), white: true, alpha: a });
            quads.push(Quad { rect: at((0.0, mid + half - inner, view_w, inner)), white: true, alpha: a });
        }
    }
    quads
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    /// The plan composited over a test picture (every pixel 0x808080) on a w x h screen, as grey levels per row.
    pub(crate) fn rows(since: Duration, w: f32, h: f32) -> Vec<f32> {
        let mut y = vec![0.5f32; h as usize];
        for q in plan(since, (0.0, 0.0, w, h)) {
            let v = if q.white { 1.0 } else { 0.0 };
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
            let v = if q.white { 1.0 } else { 0.0 };
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
        (0..=19)
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
                let still = s.iter().take(17).skip(1).position(|f| !f.2);
                assert!(still.is_none(), "{screen:?} {w}x{h}: frame {} is the same as the one before: {s:.3?}", still.unwrap_or(0) + 1);
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
        assert_eq!(picture(Screen::Tv, ms(260), area), area, "whole once open");
        assert_eq!(picture(Screen::Lcd, ms(20), area), area, "a handheld's display is never squeezed");
    }

    #[test]
    fn a_handhelds_display_comes_up_without_a_line() {
        for t in (0..300).step_by(10) {
            assert!(plan_for(Screen::Lcd, ms(t), (0.0, 0.0, 640.0, 480.0)).iter().all(|q| !q.white), "{t} ms: no white line or glow");
        }
        let dark = |t| plan_for(Screen::Lcd, ms(t), (0.0, 0.0, 640.0, 480.0)).first().map_or(0.0, |q| q.alpha);
        assert!(dark(10) > 0.99, "dark at first");
        assert!(dark(150) > 0.2 && dark(150) < 0.8, "coming up halfway: {}", dark(150));
        assert!(dark(260) == 0.0, "lit before the end");
        assert!(plan_for(Screen::Lcd, DURATION, (0.0, 0.0, 640.0, 480.0)).is_empty());
    }

    #[test]
    fn the_line_grows_out_of_the_middle_instead_of_standing() {
        let lit_width = |t| {
            plan(ms(t), (0.0, 0.0, 1280.0, 720.0)).iter().filter(|q| q.white && q.alpha >= 1.0).map(|q| q.rect.2).fold(0.0, f32::max)
        };
        let (a, b, c) = (lit_width(0), lit_width(20), lit_width(45));
        assert!(a > 0.0 && a < b && b < c && c <= 1280.0, "{a} {b} {c}");
        let q = plan(ms(20), (0.0, 0.0, 1280.0, 720.0)).into_iter().find(|q| q.white).unwrap();
        assert!((q.rect.0 + q.rect.2 / 2.0 - 640.0).abs() < 0.5, "from the middle");
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
        assert!(r.iter().filter(|v| **v > 0.5).count() <= 6, "and thin");
    }

    #[test]
    fn opens_to_the_picture_and_ends_with_nothing_drawn() {
        let mid = rows(ms(150), 1280.0, 720.0);
        let lit = mid.iter().filter(|v| **v > 0.3).count();
        assert!(lit > 30 && lit < 700, "partly open halfway ({lit} rows)");
        assert!(mid[0] < 0.01, "the edge is still dark");
        let late = rows(ms(285), 1280.0, 720.0);
        assert!(late.iter().all(|v| (v - 0.5).abs() < 0.08), "the picture, the glow almost gone: {:?}", &late[..3]);
        assert!(plan(DURATION, (0.0, 0.0, 1280.0, 720.0)).is_empty());
        assert!(plan(ms(900), (0.0, 0.0, 1280.0, 720.0)).is_empty());
    }

    #[test]
    fn the_opening_only_grows_and_stays_inside_the_screen() {
        let mut last = 0;
        for t in (0..300).step_by(10) {
            let r = rows(ms(t), 640.0, 480.0);
            let lit = r.iter().filter(|v| **v > 0.05).count();
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
        for t in (0..300).step_by(10) {
            for q in plan(ms(t), area) {
                let (x, y, w, h) = q.rect;
                assert!(x >= area.0 && x + w <= area.0 + area.2 + 0.01 && y >= area.1 - 0.01 && y + h <= area.1 + area.3 + 0.01, "{t} ms: {q:?}");
            }
        }
    }

    #[test]
    fn cheap_a_handful_of_quads() {
        for t in (0..300).step_by(5) {
            assert!(plan(ms(t), (0.0, 0.0, 1280.0, 720.0)).len() <= 10);
        }
    }

    #[test]
    fn the_hook_name_is_stable_for_the_sound() {
        assert_eq!(HOOK, "crt-on");
        assert_eq!(ENV, "POCKET_LAUNCH_CRT");
    }
}
