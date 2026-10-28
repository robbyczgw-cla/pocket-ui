//! Titles wider than their row scroll sideways while selected, as on NextUI: a rest at the start,
//! one smooth pass until the copy that follows has taken the start's place, a rest, and again.
//! Unselected rows keep the ellipsis (`Text::fit`); only the one row drawn through `Marquee` moves.

use std::time::{Duration, Instant};

use crate::gfx::Canvas;
use crate::text::{Face, Text};

/// How long the start of the title rests before (and between) passes.
const REST: Duration = Duration::from_millis(1400);

pub struct Marquee {
    key: String,
    since: Instant,
    /// Something is scrolling in the frame being painted; the app keeps redrawing while it is set.
    active: bool,
    was_active: bool,
}

impl Default for Marquee {
    fn default() -> Self {
        Self {
            key: String::new(),
            since: Instant::now(),
            active: false,
            was_active: false,
        }
    }
}

/// Distance between the end of the title and the start of its copy, for a font of `px` pixels.
fn gap(px: u32) -> f32 {
    px as f32 * 2.2
}

/// Scroll speed in pixels per second: about a character and a half per second at any size.
fn speed(px: u32) -> f32 {
    px as f32 * 1.7
}

/// How far a title of `width` pixels has scrolled after `t` since it was selected.
pub fn offset(t: Duration, width: f32, px: u32) -> f32 {
    let travel = width + gap(px);
    let pass = travel / speed(px);
    let cycle = REST.as_secs_f32() + pass;
    let in_cycle = t.as_secs_f32() % cycle;
    ((in_cycle - REST.as_secs_f32()).max(0.0) * speed(px)).min(travel)
}

impl Marquee {
    /// Call once at the start of each frame.
    pub fn begin_frame(&mut self) {
        self.was_active = std::mem::take(&mut self.active);
    }

    /// True while the last frame drew a scrolling title (asks for the next frame).
    pub fn animating(&self) -> bool {
        self.active || self.was_active
    }

    /// The next title drawn starts from rest, even if it is the one that was scrolling (a new screen).
    pub fn restart(&mut self) {
        self.key.clear();
    }

    /// Draws the selected row's title in `max_w` pixels from `x`: as is when it fits, else scrolling.
    #[allow(clippy::too_many_arguments)]
    pub fn draw(&mut self, text: &mut Text, c: &mut Canvas, face: Face, px: u32, x: i32, baseline: i32, max_w: i32, color: u32, s: &str) {
        let width = text.width(face, px, s);
        if width <= max_w {
            text.draw(c, face, px, x, baseline, color, s);
            return;
        }
        if self.key != s {
            self.key = s.to_string();
            self.since = Instant::now();
        }
        self.active = true;
        let off = offset(self.since.elapsed(), width as f32, px);
        let feather = (px as i32 * 3 / 4).max(8);
        let clip = (x, x + max_w);
        // The left edge only fades once the title has moved, so its first letter stands still and whole.
        let fade = (if off > 0.5 { feather } else { 0 }, feather);
        text.draw_clipped(c, face, px, x as f32 - off, baseline, color, s, clip, fade);
        let next = x as f32 - off + width as f32 + gap(px);
        if next < (x + max_w) as f32 {
            text.draw_clipped(c, face, px, next, baseline, color, s, clip, fade);
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
        // One pass covers the title and the gap, so the copy ends where the title began.
        let travel = 500.0 + gap(40);
        let pass = Duration::from_secs_f32(travel / speed(40));
        assert!(offset(REST + pass - ms(5), 500.0, 40) > travel - 1.0);
        assert!(offset(REST + pass + ms(5), 500.0, 40) < 0.5, "next cycle starts at rest");
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
        drop(c);
        assert!(buf.chunks(400).all(|row| row[130..].iter().all(|p| *p == 0)), "nothing drawn right of the row");
        assert!(
            buf.chunks(400).any(|row| row[100..130].iter().any(|p| *p != 0)),
            "the title reaches the right edge"
        );
    }
}
