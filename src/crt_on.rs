//! The old-TV power-on at the end of Cozy's launch animation: when the game's window comes up (`startup::HOLD_UNTIL`
//! has run out), its first 300 ms start dark, a bright line shows in the middle of the screen, and the picture opens
//! out of it, vertically, with a glow at the two edges that settles. The game itself runs meanwhile (the window is no
//! longer held back, nothing waits for the effect); a button press ends it at once.
//!
//! Only in Cozy with Motion on Full: pocket-ui sets [`ENV`] for pocket-core then. `plan` is the whole effect as
//! rectangles for a moment in time; pocket-core draws them as a handful of solid quads (`Overlay::draw_solid`), so
//! the cost is a few fill-rate passes over a part of the screen and no shader of its own.

use std::time::Duration;

/// Set by pocket-ui for a pocket-core game whose launch is Cozy with Motion Full; absent in Quick, Reduced and Off.
pub const ENV: &str = "POCKET_LAUNCH_CRT";

/// The name the CRT sound hooks in on: [`start`] writes it as a trace event (`POCKETUI_TRACE`) and as the line
/// `crt-on` on stdout of pocket-core the moment the effect begins. The sound agent plays its sample at that point.
pub const HOOK: &str = "crt-on";

/// How long the effect lasts.
pub const DURATION: Duration = Duration::from_millis(300);

/// Up to this moment the screen is dark with only the line growing out of the middle (fraction of [`DURATION`]).
const LINE_END: f32 = 0.22;
/// The picture is fully open at this moment; the glow settles until the end.
const OPEN_END: f32 = 0.78;
/// The line is this many pixels thick at 720 px of screen height (scaled with the screen).
const LINE_PX: f32 = 3.0;

/// Whether pocket-ui asked for the effect.
pub fn wanted() -> bool {
    std::env::var_os(ENV).is_some_and(|v| v == "1")
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

/// The rectangles to draw over the game's picture `area` (x, y, w, h in pixels from the top left; the frame around
/// it is not part of the effect) `since` its first frame; empty once the effect is over.
pub fn plan(since: Duration, area: (f32, f32, f32, f32)) -> Vec<Quad> {
    let (ax, ay, view_w, view_h) = area;
    let t = since.as_secs_f32() / DURATION.as_secs_f32();
    if t >= 1.0 || view_w < 1.0 || view_h < 1.0 {
        return Vec::new();
    }
    let mid = view_h / 2.0;
    let at = |(x, y, w, h): (f32, f32, f32, f32)| (ax + x, ay + y, w, h);
    let line = (LINE_PX * view_h / 720.0).max(2.0);
    // Half the height of the lit part: the line while it flashes, then the picture opening, eased out.
    let half = if t < LINE_END {
        line / 2.0
    } else {
        let p = ((t - LINE_END) / (OPEN_END - LINE_END)).clamp(0.0, 1.0);
        let eased = 1.0 - (1.0 - p).powi(3);
        line / 2.0 + (mid - line / 2.0) * eased
    };
    let mut quads = Vec::with_capacity(8);
    // Dark above and below the lit part.
    for (y, h) in [(0.0, mid - half), (mid + half, mid - half)] {
        if h > 0.0 {
            quads.push(Quad { rect: at((0.0, y, view_w, h)), white: false, alpha: 1.0 });
        }
    }
    // The lit part: white at first (the line), the picture showing through more and more as it opens. A bloom that
    // lets go after the opening.
    let open = ((t - LINE_END) / (OPEN_END - LINE_END)).clamp(0.0, 1.0);
    let flash = if t < LINE_END { 1.0 } else { (1.0 - open).powi(2) * 0.9 };
    if flash > 0.0 {
        quads.push(Quad { rect: at((0.0, mid - half, view_w, 2.0 * half)), white: true, alpha: flash });
    }
    // Glow: three soft bands inside the picture at each edge of the lit part, brightest at the line, fading out and
    // settling over the last stretch.
    let settle = if t < OPEN_END { 1.0 } else { 1.0 - (t - OPEN_END) / (1.0 - OPEN_END) };
    let band = (view_h * 0.035).max(6.0);
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
