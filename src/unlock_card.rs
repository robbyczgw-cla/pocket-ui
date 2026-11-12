//! The achievement unlock card, painted the same on Linux (pocket-core's HUD, `overlay`) and in the Android player: the
//! badge on the left (a drawn amber ring when it has none), "Achievement unlocked · 5 points" in grey over the title in
//! white, dark like every message over a game.

use crate::gfx::Canvas;
use crate::image::Image;
use crate::text::{Face, Text};

/// The badge's size, the padding around the card's content, the text column's widest line.
pub const BADGE: i32 = 72;
pub const PAD: i32 = 18;
pub const TEXT_MAX: i32 = 760;
/// The card's own colour.
pub const FILL: u32 = 0x1a1a1a;
const SMALL: u32 = 24;
const TITLE: u32 = 34;

/// The small line over the title.
pub fn head(points: u32) -> String {
    match points {
        0 => "Achievement unlocked".to_string(),
        1 => "Achievement unlocked · 1 point".to_string(),
        p => format!("Achievement unlocked · {p} points"),
    }
}

/// The widest line of text a card may have on a screen `view_w` wide: the card (badge, paddings, text) then stays
/// 40 pixels clear of both edges, and never grows past `TEXT_MAX`.
pub fn text_max(view_w: i32) -> i32 {
    (view_w - 80 - (PAD + BADGE + 20 + PAD + 8)).clamp(120, TEXT_MAX)
}

/// The title as it fits the card (cut with an ellipsis when it is longer than the text column).
pub fn fitted_title(text: &mut Text, title: &str, max: i32) -> String {
    text.fit(Face::Title, TITLE, title, max.min(TEXT_MAX))
}

/// The small line as it fits (a long language on a narrow screen is cut with an ellipsis, never run off the card).
pub fn fitted_head(text: &mut Text, points: u32, max: i32) -> String {
    text.fit(Face::Body, SMALL, &crate::i18n::tr(&head(points)), max.min(TEXT_MAX))
}

/// The card's size for this text, the widest line at most `max` wide.
pub fn size(text: &mut Text, title: &str, points: u32, max: i32) -> (i32, i32) {
    let title = fitted_title(text, title, max);
    let small = fitted_head(text, points, max);
    let text_w = text.width(Face::Body, SMALL, &small).max(text.width(Face::Title, TITLE, &title));
    (PAD + BADGE + 20 + text_w + PAD + 8, BADGE + 2 * PAD)
}

/// Paints the card with its top left at (`x`, `y`) on `c`; returns its size.
pub fn paint(c: &mut Canvas, text: &mut Text, (x, y): (i32, i32), title: &str, points: u32, badge: Option<&Image>, max: i32) -> (i32, i32) {
    let title = fitted_title(text, title, max);
    let (w, h) = size(text, &title, points, max);
    crate::retro::window(c, x, y, w, h, 26, FILL);
    let (bx, by) = (x + PAD, y + PAD);
    match badge {
        Some(img) => c.blit_scaled(img, bx, by, BADGE, BADGE),
        None => {
            crate::retro::pill(c, bx, by, BADGE, BADGE, 14, 0x2a2a2e);
            c.ring(bx + BADGE / 2, by + BADGE / 2, 20, 4, 0xd39a2c);
            c.disc(bx + BADGE / 2, by + BADGE / 2, 7, 0xd39a2c);
        }
    }
    let tx = bx + BADGE + 20;
    let small = fitted_head(text, points, max);
    text.draw(c, Face::Body, SMALL, tx, y + PAD + 26, 0xaaa79f, &small);
    text.draw(c, Face::Title, TITLE, tx, y + PAD + 26 + 46, 0xffffff, &title);
    (w, h)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_card_stays_whole_on_every_width_a_phone_or_handheld_paints() {
        let mut text = Text::builtin();
        let title = "A very long achievement title that goes on and on and on and would never fit on one line of the card";
        for w in [480, 640, 720, 854, 960, 1080, 1280, 1920] {
            let (cw, ch) = size(&mut text, title, 25, text_max(w));
            assert!(cw <= w - 80 && cw > BADGE + 2 * PAD, "{w}: card {cw}x{ch}");
            assert_eq!(ch, BADGE + 2 * PAD);
            // Painted into a canvas of that width it leaves no pixel outside it (Canvas clips; the size is what counts).
            let mut px = vec![0u32; (w * 200) as usize];
            let mut c = Canvas::new(&mut px, w as u32, 200);
            let (pw, _) = paint(&mut c, &mut text, ((w - cw) / 2, 14), title, 25, None, text_max(w));
            assert_eq!(pw, cw, "{w}");
        }
    }

    #[test]
    fn the_head_says_the_points() {
        assert_eq!(head(0), "Achievement unlocked");
        assert_eq!(head(1), "Achievement unlocked · 1 point");
        assert_eq!(head(25), "Achievement unlocked · 25 points");
    }
}
