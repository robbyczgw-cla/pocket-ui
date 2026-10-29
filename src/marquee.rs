//! Long selected titles rest with an ellipsis, then make one pass and return to their start.
//! Only one text layer is painted; no repeated copy can collide with the original.
//! Unselected rows keep the ellipsis (`Text::fit`); only the one row drawn through `Marquee` moves.

use std::time::{Duration, Instant};

use crate::gfx::Canvas;
use crate::text::{Face, Text};

/// How long the start of the title rests before (and between) passes.
const REST: Duration = Duration::from_millis(1500);

pub struct Marquee {
    key: String,
    since: Instant,
    layout: (u32, i32, i32),
    /// Something is scrolling in the frame being painted; the app keeps redrawing while it is set.
    active: bool,
    was_active: bool,
}

impl Default for Marquee {
    fn default() -> Self {
        Self {
            key: String::new(),
            since: Instant::now(),
            layout: (0, 0, 0),
            active: false,
            was_active: false,
        }
    }
}

/// Scroll speed in pixels per second.
fn speed(px: u32) -> f32 {
    crate::type_scale::px(px) as f32 * 1.7
}

/// Offset for a single title: rest, travel to the end, rest, return, rest at its start.
/// `travel` is the overflow beyond the available row width, not the title's full width.
pub fn offset(t: Duration, travel: f32, px: u32) -> f32 {
    if travel <= 0.0 {
        return 0.0;
    }
    let pass = travel / speed(px);
    let rest = REST.as_secs_f32();
    let phase = t.as_secs_f32() % (2.0 * rest + 2.0 * pass);
    if phase < rest {
        0.0
    } else if phase < rest + pass {
        (phase - rest) * speed(px)
    } else if phase < 2.0 * rest + pass {
        travel
    } else {
        (2.0 * rest + 2.0 * pass - phase) * speed(px)
    }
}

impl Marquee {
    /// Call once at the start of each frame.
    pub fn begin_frame(&mut self) {
        if !self.active {
            self.key.clear();
        }
        self.was_active = std::mem::take(&mut self.active);
    }

    /// True while the last frame drew a scrolling title (asks for the next frame).
    pub fn animating(&self) -> bool {
        self.active || self.was_active
    }

    /// The next title drawn starts from rest, even if it is the one that was scrolling (a new screen).
    pub fn restart(&mut self) {
        self.key.clear();
        self.active = false;
        self.was_active = false;
    }

    /// Draws the selected row's title in `max_w` pixels from `x`: as is when it fits, else scrolling.
    #[allow(clippy::too_many_arguments)]
    pub fn draw(&mut self, text: &mut Text, c: &mut Canvas, face: Face, px: u32, x: i32, baseline: i32, max_w: i32, color: u32, s: &str) {
        let width = text.width(face, px, s);
        let layout = (crate::type_scale::px(px), max_w, width);
        if self.key != s || self.layout != layout {
            self.layout = layout;
            self.key = s.to_string();
            self.since = Instant::now();
        }
        if max_w <= 0 {
            return;
        }
        if width <= max_w {
            text.draw(c, face, px, x, baseline, color, s);
            return;
        }
        self.active = true;
        let off = offset(self.since.elapsed(), (width - max_w) as f32, px);
        if off <= 0.5 {
            let fitted = text.fit(face, px, s, max_w);
            text.draw(c, face, px, x, baseline, color, &fitted);
        } else {
            let feather = (crate::type_scale::px(px) as i32 * 3 / 4).max(8);
            text.draw_clipped(c, face, px, x as f32 - off, baseline, color, s, (x, x + max_w), (feather, 0));
        }
    }

    /// Like `draw`, centred in `max_w` when the title fits.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_centered(&mut self, text: &mut Text, c: &mut Canvas, face: Face, px: u32, x: i32, baseline: i32, max_w: i32, color: u32, s: &str) {
        let width = text.width(face, px, s);
        let x = if width <= max_w { x + (max_w - width) / 2 } else { x };
        self.draw(text, c, face, px, x, baseline, max_w, color, s);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rests_then_scrolls_one_pass_then_rests_again() {
        let ms = Duration::from_millis;
        assert_eq!(offset(ms(0), 500.0, 40), 0.0);
        assert_eq!(offset(REST - ms(1), 500.0, 40), 0.0, "the start rests first");
        let a = offset(REST + ms(500), 500.0, 40);
        assert!((a - 34.0).abs() < 0.5, "68 px a second at 40 px: {a}");
        let pass = Duration::from_secs_f32(500.0 / speed(40));
        assert!((offset(REST + pass, 500.0, 40) - 500.0).abs() < 0.5);
        assert_eq!(offset(2 * REST + 2 * pass + ms(5), 500.0, 40), 0.0);
        assert_eq!(offset(ms(1500), 0.0, 40), 0.0);
    }

    #[test]
    fn start_and_selection_reset_paint_exactly_one_ellipsized_title() {
        let mut text = Text::new();
        let title = "Pokemon (Emerald Rogue v2.2.0 EX)";
        let mut marquee = Marquee::default();
        let render = |m: &mut Marquee, text: &mut Text, s: &str| {
            let mut pixels = vec![0; 400 * 100];
            m.begin_frame();
            m.draw(text, &mut Canvas::new(&mut pixels, 400, 100), Face::Body, 28, 10, 60, 180, 0xffffff, s);
            pixels
        };
        let fitted = text.fit(Face::Body, 28, title, 180);
        assert!(fitted.starts_with("Pokemon") && fitted.ends_with('…'));
        let mut expected = vec![0; 400 * 100];
        text.draw(&mut Canvas::new(&mut expected, 400, 100), Face::Body, 28, 10, 60, 0xffffff, &fitted);
        assert_eq!(render(&mut marquee, &mut text, title), expected);
        marquee.since = Instant::now() - Duration::from_secs(3);
        assert_ne!(render(&mut marquee, &mut text, title), expected);
        render(&mut marquee, &mut text, "Short");
        assert_eq!(render(&mut marquee, &mut text, title), expected, "a fitting row between long rows resets the timer");
        marquee.since = Instant::now() - Duration::from_secs(3);
        marquee.restart();
        assert_eq!(render(&mut marquee, &mut text, title), expected, "even the same name starts cleanly on reselection");
    }

    #[test]
    fn fitting_titles_do_not_animate() {
        let mut text = Text::new();
        let mut buf = vec![0u32; 400 * 100];
        let mut c = Canvas::new(&mut buf, 400, 100);
        let mut m = Marquee::default();
        m.begin_frame();
        m.draw(&mut text, &mut c, Face::Body, 28, 10, 60, 380, 0xffffff, "uCity");
        assert!(!m.active);
        m.draw(
            &mut text,
            &mut c,
            Face::Body,
            28,
            10,
            60,
            120,
            0xffffff,
            "The Legend of Zelda: Link's Awakening DX",
        );
        assert!(m.active && m.animating());
        assert!(buf.chunks(400).all(|row| row[130..].iter().all(|p| *p == 0)), "nothing drawn right of the row");
        assert!(
            buf.iter().any(|p| *p != 0),
            "the fitted title is visible"
        );
    }
}
