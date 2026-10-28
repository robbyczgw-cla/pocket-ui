//! Part of the pocket-ui binary: the pause menu's side of game saves that don't reach the card
//! (`pocket_ui::save_trouble`).
//!
//! When the menu opens over a pocket-core game, the note pocket-core leaves while its saves fail is read (one small
//! read on tmpfs, as the Disc row's). While there is one, the main page says so under the title, and Quit asks once
//! before it goes: "Couldn't save. Quit anyway?", with what is at stake. Keep playing (the first row, and A on it)
//! goes back to the game, which goes on trying; B comes back to the menu; Quit anyway quits as Quit always does, and
//! that quit still tries the save once more. Nothing waits for the card.

use pocket_ui::gfx::Canvas;
use pocket_ui::image::Image;
use pocket_ui::pad::Button;
use pocket_ui::palette::ALERT;
use pocket_ui::save_trouble::{self, Trouble};
use pocket_ui::text::Face;

use super::{App, MainItem, Page, QuickMenu, CANCEL, CONFIRM, NU_BTN_PAD, NU_PAD, NU_PILL, NU_SMALL};

/// The question's rows, Keep playing first.
const ROWS: [&str; 2] = [save_trouble::KEEP_PLAYING, save_trouble::QUIT_ANYWAY];

/// Whether Quit asks first: the game's saves don't reach the card and the player hasn't answered yet.
pub(crate) fn asks_before_quit(menu: &QuickMenu) -> bool {
    menu.save_trouble.is_some()
}

impl App {
    /// The note pocket-core left for the game the menu opens over (pocket-core games only).
    pub(crate) fn save_trouble_at_open(hosted: bool) -> Option<Trouble> {
        if !hosted {
            return None;
        }
        save_trouble::read_from(std::path::Path::new(save_trouble::FILE))
    }

    /// Input on the question before Quit.
    pub(crate) fn quit_question_input(&mut self, b: Button) {
        let Some(menu) = self.menu.as_mut() else { return };
        let Page::QuitUnsaved { sel } = menu.page else { return };
        match b {
            Button::Up | Button::Down => menu.page = Page::QuitUnsaved { sel: 1 - sel },
            b if b == CANCEL => {
                menu.page = Page::Main;
                menu.sel = menu.items().iter().position(|i| *i == MainItem::Quit).unwrap_or(0);
            }
            b if b == CONFIRM && sel == 0 => self.resume_game(None),
            b if b == CONFIRM => {
                // Answered: Quit goes on as it always does (it tries the save once more on the way out).
                menu.save_trouble = None;
                menu.page = Page::Main;
                menu.sel = menu.items().iter().position(|i| *i == MainItem::Quit).unwrap_or(0);
                self.menu_input(CONFIRM);
            }
            _ => {}
        }
    }

    /// The question's rows with their explanation, for `paint_options` (the heading is the question).
    pub(crate) fn quit_question_rows(trouble: Trouble) -> Vec<(String, String, String)> {
        let keep = "Back to the game. It keeps trying to save; the line goes away once a save gets through.";
        ROWS.iter().zip([keep, trouble.risk()]).map(|(name, desc)| (name.to_string(), String::new(), desc.to_string())).collect()
    }

    /// The line under the main page's title while the game's saves fail: a small dot in the alert colour, then the
    /// words, wrapped below the pill. Returns where the rows may start.
    pub(crate) fn paint_save_trouble_line(&mut self, c: &mut Canvas, trouble: Trouble) -> i32 {
        let (x, max) = (NU_PAD + NU_BTN_PAD, c.w - 2 * (NU_PAD + NU_BTN_PAD) - 28);
        let lines = self.text.wrap(Face::Body, NU_SMALL, trouble.line(), max);
        // At the size painted (Settings → Text size, `type_scale`).
        let px = pocket_ui::type_scale::px(NU_SMALL) as i32;
        let line_h = px * 4 / 3;
        let first = NU_PAD + NU_PILL + 26 + px;
        c.disc(x + 8, first - px * 3 / 10, 8, ALERT);
        for (i, l) in lines.iter().enumerate() {
            self.text.draw(c, Face::Body, NU_SMALL, x + 28, first + i as i32 * line_h, 0xffffff, l);
        }
        first + (lines.len() as i32 - 1) * line_h + px
    }

    /// Snapshots: "…:save=<full|unwritable>" gives the pause menu the line (the menu: `menu:pocket-core:5:save=full`
    /// has Quit selected); "quit-unsaved[:<row>][:full|:unwritable]" is the question with that row selected.
    pub(crate) fn save_trouble_snapshot(&mut self, state: &str) {
        let kind = |s: &str| if s.contains("unwritable") { Trouble::Unwritable } else { Trouble::Full };
        let Some(menu) = self.menu.as_mut() else { return };
        if let Some(spec) = state.split(':').find_map(|p| p.strip_prefix("save=")) {
            menu.save_trouble = Some(kind(spec));
        }
        if let Some(rest) = state.strip_prefix("quit-unsaved") {
            menu.save_trouble = Some(kind(rest));
            let sel = rest.split(':').find_map(|p| p.parse::<usize>().ok()).unwrap_or(0).min(1);
            menu.page = Page::QuitUnsaved { sel };
        }
    }
}

/// Snapshot "game-save-warning[:full|:unwritable]": what pocket-core shows over the game while saves fail, its line in
/// the pill `overlay::paint_pill` paints (the same function pocket-core draws with), at the bottom of the game's picture
/// (`background`, else a grey stand-in). Returns the pixels, or None for another state.
pub(crate) fn game_warning_snapshot(state: &str, background: Option<Image>, w: u32, h: u32, text: &mut pocket_ui::text::Text) -> Option<Vec<u32>> {
    let rest = state.strip_prefix("game-save-warning")?;
    // pocket-core draws it, at the design's sizes whatever Settings → Text size says.
    let _fixed = pocket_ui::type_scale::fixed();
    let trouble = if rest.contains("unwritable") { Trouble::Unwritable } else { Trouble::Full };
    let mut buf = vec![0x202024u32; (w * h) as usize];
    let mut c = Canvas::new(&mut buf, w, h);
    if let Some(img) = background {
        let (iw, ih) = img.fit(w as i32, h as i32);
        c.blit_scaled(&img, (w as i32 - iw) / 2, (h as i32 - ih) / 2, iw, ih);
    }
    let (pw, ph, pill) = pocket_ui::overlay::paint_pill(text, trouble.line());
    // As the overlay draws it: bottom centre, 24 px up, 82 % opaque, the key colour transparent.
    let (x0, y0) = ((w as i32 - pw) / 2, h as i32 - 24 - ph);
    for y in 0..ph {
        for x in 0..pw {
            let p = pill[(y * pw + x) as usize];
            if p == pocket_ui::pixel_live::KEY {
                continue;
            }
            let at = ((y0 + y) as u32 * w + (x0 + x) as u32) as usize;
            buf[at] = blend(buf[at], p, 0.82);
        }
    }
    Some(buf)
}

fn blend(under: u32, over: u32, a: f32) -> u32 {
    let ch = |v: u32, s: u32| (((v >> s) & 0xff) as f32 * (1.0 - a) + ((over >> s) & 0xff) as f32 * a).round() as u32;
    (ch(under, 16) << 16) | (ch(under, 8) << 8) | ch(under, 0)
}
