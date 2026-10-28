//! Part of the pocket-ui binary: pages of rows and text on a wide screen (docs/DEVICES.md "Layouts").
//!
//! The pages are laid out for 1240 to 1280 pixels across (the Mini V2, the Mini V1). On the Retroid Pocket 5's and
//! Flip 2's 1920x1080 a list row would put its name and its value 1,880 px apart and a line of text would run to about
//! 200 characters, so on a screen wider than [`WIDE_FROM`] the pages of rows and text (Settings and the manual, the
//! Shelf look, the search, the first start's steps) are painted into a column of [`column_width`] in the middle,
//! with the page's background on both sides. The looks made for the whole width (Split, Cards, Minimal, Cabinet),
//! the game card, the console page, the pause menu over the game and everything else keep the whole screen.
//!
//! How: the page paints into a buffer of the column's size as if that were the screen (so every page needs nothing
//! of its own), which is then copied into the frame; the touch areas it registered and the text checks' record
//! (`text::boxes`) move with it. The copy is one row-by-row `copy_from_slice` per frame, only on such a screen; at
//! 1240 and 1280 pixels nothing of this runs.

use pocket_ui::gfx::Canvas;
use pocket_ui::palette::pal;

use super::App;

/// Screens at least this wide get the column. The Mini V2 (1240) and the Mini V1 (1280) never do.
pub const WIDE_FROM: i32 = 1600;
/// The column on such a screen: three quarters of it, at least 1440 px (1920 → 1440).
pub fn column_width(w: i32) -> i32 {
    if w < WIDE_FROM {
        w
    } else {
        (w * 3 / 4).max(1440).min(w)
    }
}

impl App {
    /// Paints a page of rows and text with `paint`: across the whole canvas on a screen narrower than [`WIDE_FROM`],
    /// else into the column in the middle (see the module's notes).
    pub(crate) fn in_column(&mut self, c: &mut Canvas, paint: impl FnOnce(&mut App, &mut Canvas)) {
        let cw = column_width(c.w);
        if cw >= c.w {
            paint(self, c);
            return;
        }
        let x0 = (c.w - cw) / 2;
        let mut buf = std::mem::take(&mut self.column_buf);
        buf.clear();
        buf.resize((cw * c.h) as usize, pal().void);
        let first_hit = self.hits.areas_len();
        {
            let mut col = Canvas::new(&mut buf, cw as u32, c.h as u32);
            paint(self, &mut col);
        }
        // The sides in the page's own background: its top left pixel (a page with a tint or a picture behind it, as
        // Cards or Cabinet would have, never comes here).
        let side = buf.first().copied().unwrap_or(pal().void);
        c.clear(side);
        let (w, cw_u) = (c.w as usize, cw as usize);
        for (y, row) in buf.chunks_exact(cw_u).enumerate() {
            let at = y * w + x0 as usize;
            c.buf[at..at + cw_u].copy_from_slice(row);
        }
        self.hits.shift_x(first_hit, x0);
        pocket_ui::text::boxes::moved(&buf, cw, c.buf, c.w, x0);
        self.column_buf = buf;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_wide_screen_gets_a_column() {
        assert_eq!(column_width(1240), 1240, "the Mini V2: the whole width, as before");
        assert_eq!(column_width(1280), 1280, "the Mini V1");
        assert_eq!(column_width(1920), 1440, "the Pocket 5 and Flip 2");
        assert_eq!(column_width(2560), 1920);
    }
}
