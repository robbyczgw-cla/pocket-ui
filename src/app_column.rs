//! Part of the pocket-ui binary: pages of rows and text on a wide screen (docs/DEVICES.md "Layouts").
//!
//! The pages are laid out for 1240 to 1280 pixels across (the Mini V2, the Mini V1). On the Retroid Pocket 5's and
//! Flip 2's 1920x1080 a list row would put its name and its value 1,880 px apart and a line of text would run to about
//! 200 characters, so on a screen wider than [`WIDE_FROM`] the pages of rows and text (Settings and the manual, the
//! Shelf look, the search, the first start's steps) are painted into a column of [`column_width`] in the middle,
//! with the page's background on both sides. The looks made for the whole width (Split, Cards, Minimal, Cabinet),
//! the game card, the console page, the pause menu over the game and everything else keep the whole screen.
//!
//! Android's handheld view (a phone or a handheld with a pad, `play_route::phone`) gives the Shelf look the whole width
//! too ([`shelf_fills_width`]): on the Odin's 16:9 and a 20:9 phone turned the column left a quarter of the screen black,
//! and the shelf showed fewer spines than the width holds (ANDROID-READINESS, 13.11.). Its pages of rows and text keep
//! the column.
//!
//! The pages of rows and text (Settings and the manual, the first start's steps) take more of a wide screen since
//! 15.11.2026 (a hand test on the Odin at 1920x1080: "Settings don't use the whole screen"): their column is
//! [`page_column_width`] (the whole width up to 16:9 with a slim margin, 1800 of 1920, 2160 of a 20:9 2400) and they are
//! painted with `type_scale::wide_page`'s factor (1.3 at 1080 lines): type, rows and lines grow together, so a
//! category's rows reach down the screen. The search and the Linux Shelf look keep the ¾ column of [`column_width`].
//!
//! How: the page paints into a buffer of the column's size as if that were the screen (so every page needs nothing
//! of its own), which is then copied into the frame; the touch areas it registered and the text checks' record
//! (`text::boxes`) move with it. The copy is one row-by-row `copy_from_slice` per frame, only on such a screen; at
//! 1240 and 1280 pixels nothing of this runs.

use pocket_ui::gfx::Canvas;
use pocket_ui::palette::pal;

use super::App;

/// Screens at least this wide get the column. The Mini V2 (1240) and the Mini V1 (1280) never do.
pub const WIDE_FROM: i32 = pocket_ui::type_scale::WIDE_FROM;
/// The column on such a screen: three quarters of it, at least 1440 px (1920 → 1440).
pub fn column_width(w: i32) -> i32 {
    if w < WIDE_FROM {
        w
    } else {
        (w * 3 / 4).max(1440).min(w)
    }
}

/// The column of a page of rows and text (`App::in_page_column`) on a screen `w` x `h`: the whole width on a screen
/// narrower than [`WIDE_FROM`]; on a wider one fifteen sixteenths of it (a slim margin, as the shelf's ends), at most
/// twice the height (a 20:9 2400x1080 keeps 2160, so a row's name and value stay within a glance).
pub fn page_column_width(w: i32, h: i32) -> i32 {
    if w < WIDE_FROM {
        w
    } else {
        (w * 15 / 16).min(2 * h).min(w)
    }
}

/// The Shelf look across the whole screen instead of in the column: Android's handheld view (the host read a phone,
/// `play_route::phone`). Linux keeps the column (docs/DEVICES.md "Layouts").
pub fn shelf_fills_width() -> bool {
    pocket_ui::play_route::phone().is_some()
}

impl App {
    /// The Shelf look's home page, as `App::paint` and the look step's previews paint it: in the column on a wide Linux
    /// screen, across the whole width in Android's handheld view ([`shelf_fills_width`]).
    pub(crate) fn paint_shelf_look(&mut self, c: &mut Canvas) {
        if shelf_fills_width() {
            self.paint_home(c, true);
        } else {
            self.in_column(c, |app, c| app.paint_home(c, true));
        }
    }

    /// Paints a page of rows and text with `paint`: across the whole canvas on a screen narrower than [`WIDE_FROM`],
    /// else into the column in the middle (see the module's notes).
    pub(crate) fn in_column(&mut self, c: &mut Canvas, paint: impl FnOnce(&mut App, &mut Canvas)) {
        let cw = column_width(c.w);
        self.in_column_of(c, cw, paint);
    }

    /// Paints a page of rows and text (Settings, the manual, the first start) with `paint`: on a wide screen into
    /// [`page_column_width`] and with `type_scale::wide_page`'s larger type and rows; elsewhere as before, across the
    /// whole canvas (see the module's notes).
    pub(crate) fn in_page_column(&mut self, c: &mut Canvas, paint: impl FnOnce(&mut App, &mut Canvas)) {
        let _wide = pocket_ui::type_scale::panel(pocket_ui::type_scale::wide_page(c.w, c.h));
        let cw = page_column_width(c.w, c.h);
        self.in_column_of(c, cw, paint);
    }

    fn in_column_of(&mut self, c: &mut Canvas, cw: i32, paint: impl FnOnce(&mut App, &mut Canvas)) {
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
        c.copy_design(x0, 0, cw as usize, &buf, None);
        self.hits.shift_x(first_hit, x0);
        pocket_ui::text::boxes::moved(&buf, cw, c, x0);
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

    #[test]
    fn a_wide_screen_s_pages_take_nearly_the_whole_width() {
        assert_eq!(page_column_width(1240, 1080), 1240, "the Mini V2: the whole width, as before");
        assert_eq!(page_column_width(1024, 768), 1024);
        assert_eq!(page_column_width(1920, 1080), 1800, "16:9: a slim margin");
        assert_eq!(page_column_width(2400, 1080), 2160, "20:9: at most twice the height");
    }
}
