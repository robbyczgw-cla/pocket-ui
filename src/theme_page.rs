//! Settings → Look & feel → Look → **Theme** (R2 on the Look page): Colours, Font and Button symbols, one row per
//! choice in the shelf family's style, the selected one's value switched with Left/Right (or A), and a live preview
//! under them: the shelf in the look in use, painted with the choices as they stand, and a sample of the hint keys.
//!
//! Each choice is process wide and takes effect with the next frame (an atomic per choice, read where the part is
//! painted); `Setup` keeps nothing of it and the App saves it with the other settings when the Look page closes.
//! Nothing here reads a file or starts work.

use crate::button_prompts;
use crate::font_style;
use crate::gfx::Canvas;
use crate::image::Image;
use crate::kit;
use crate::pad::Button;
use crate::palette::{self, pal, MARGIN};
use crate::text::{Face, Text};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Row {
    /// Warm, Indigo Cube, Grey Brick, Green X or Blue Station (`palette::Colours`).
    Colours,
    /// Barlow, Pixel or Rounded (`font_style`).
    Font,
    /// Nintendo, Xbox or PlayStation (`button_prompts`).
    Prompts,
}

pub const ROWS: [Row; 3] = [Row::Colours, Row::Font, Row::Prompts];

impl Row {
    pub fn label(self) -> &'static str {
        match self {
            Row::Colours => "Colours",
            Row::Font => "Font",
            Row::Prompts => "Button symbols",
        }
    }

    /// The value in use, as the row shows it.
    pub fn value(self) -> &'static str {
        match self {
            Row::Colours => palette::colours().label(),
            Row::Font => font_style::get().label(),
            Row::Prompts => button_prompts::get().label(),
        }
    }

    /// The line about the value in use.
    pub fn blurb(self) -> &'static str {
        match self {
            Row::Colours => palette::colours().blurb(),
            Row::Font => font_style::get().blurb(),
            Row::Prompts => button_prompts::get().blurb(),
        }
    }

    /// The next value (or the one before it), used at once.
    fn step(self, forward: bool) {
        match self {
            Row::Colours => palette::set_colours(palette::colours().step(forward)),
            Row::Font => font_style::set(if forward { font_style::get().next() } else { font_style::get().prev() }),
            Row::Prompts => button_prompts::set(button_prompts::get().step(forward)),
        }
    }
}

/// Everything the page's choices stand at, for the setup's repaint mark and for painting the shelf's previews again.
pub fn mark() -> u32 {
    button_prompts::get() as u32 | (palette::colours() as u32) << 4 | (font_style::get() as u32) << 8
}

/// A press on the page. `true`: back to the Look page (B, START).
pub fn press(sel: &mut usize, b: Button) -> bool {
    let n = ROWS.len();
    *sel = (*sel).min(n - 1);
    match b {
        Button::Up => *sel = (*sel + n - 1) % n,
        Button::Down => *sel = (*sel + 1) % n,
        Button::Left => ROWS[*sel].step(false),
        Button::Right | Button::East => ROWS[*sel].step(true),
        Button::South | Button::Start => return true,
        _ => {}
    }
    false
}

/// The page's hints.
pub fn hints() -> Vec<(&'static str, &'static str)> {
    vec![("Left/Right", "Change"), ("A", "Next"), ("B", "Back")]
}

/// The page: its title, the rows, the selected one's line, and the preview (`preview`: the shelf in the look in use,
/// `None` while it is painted) with the sample keys, as large as the room under the rows allows.
pub fn paint(c: &mut Canvas, text: &mut Text, sel: usize, preview: Option<&Image>) {
    c.clear(pal().void);
    text.draw(c, Face::Title, 67, MARGIN, 140, pal().chalk, "Theme");
    let sub = text.fit(Face::Body, 28, "How every screen looks: colours, type and the buttons in the hints.", c.w - 2 * MARGIN);
    text.draw(c, Face::Body, 28, MARGIN, 192, pal().dust, &sub);
    let (top, row) = (232, crate::type_scale::row(76, 37));
    let width = c.w - 2 * MARGIN;
    let sel = sel.min(ROWS.len() - 1);
    for (i, r) in ROWS.iter().enumerate() {
        let y = top + i as i32 * row;
        let selected = i == sel;
        kit::shelf_row(c, MARGIN, y, width, row - 8, selected);
        let fg = if selected { pal().ink } else { pal().chalk };
        let value = if selected { format!("‹ {} ›", r.value()) } else { r.value().to_string() };
        let vw = text.width(Face::Body, 28, &value);
        let name = text.fit(Face::Title, 37, r.label(), width - 56 - vw - 40);
        text.draw(c, Face::Title, 37, MARGIN + 28, y + (row - 8) / 2 + crate::type_scale::base(13, 37), fg, &name);
        let dim = if selected { pal().label_dust } else { pal().dust };
        text.draw_right(c, Face::Body, 28, MARGIN + width - 28, y + (row - 8) / 2 + crate::type_scale::base(10, 28), dim, &value);
    }
    let mut y = top + ROWS.len() as i32 * row + 34 + crate::type_scale::grow(28);
    for line in text.wrap_at_most(Face::Body, 28, ROWS[sel].blurb(), width, 2) {
        text.draw(c, Face::Body, 28, MARGIN, y, pal().dust, &line);
        y += crate::type_scale::line(28, 10);
    }
    // The preview: the shelf, and the sample keys under it, between the line and the hint bar.
    let keys_h = crate::type_scale::row(60, 30);
    let (room_top, room_bottom) = (y + 4, c.h - 120);
    let ph = (room_bottom - room_top - keys_h - 24).min(crate::setup::look_preview((c.w, c.h)).1);
    if let Some(img) = preview.filter(|_| ph >= 120) {
        let pw = (ph * img.w as i32 / img.h.max(1) as i32).min(width - 16);
        let ph = pw * img.h as i32 / img.w.max(1) as i32;
        let x = (c.w - pw) / 2;
        c.round_rect(x - 8, room_top - 8 + 8, pw + 16, ph + 16, 16, pal().keycap);
        c.blit_scaled(img, x, room_top + 8, pw, ph);
        sample_keys(c, text, room_top + 8 + ph + 24, keys_h);
    } else {
        sample_keys(c, text, room_top + 8, keys_h);
    }
    crate::setup::hints(c, text, &hints());
}

/// The four face keys in a pill as the Settings pages show them, centred, `h` high from `top`.
fn sample_keys(c: &mut Canvas, text: &mut Text, top: i32, h: i32) {
    const SAMPLE: [(&str, &str); 4] = [("A", "Open"), ("B", "Back"), ("X", "Options"), ("Y", "Star")];
    let px = kit::SMALL;
    let (gap, cap) = (12, h - 16);
    let widths: Vec<(i32, i32)> = SAMPLE.iter().map(|(k, l)| ((text.width(Face::Body, px, button_prompts::shown(k)) + 16).max(cap), text.width(Face::Body, px, l))).collect();
    let w: i32 = widths.iter().map(|(kw, lw)| kw + gap + lw + 2 * gap).sum::<i32>() + gap;
    if w > c.w - 2 * MARGIN {
        return;
    }
    let x0 = (c.w - w) / 2;
    c.round_rect(x0, top, w, h, h / 2, pal().pill);
    let base = top + h / 2 + crate::type_scale::mid(px);
    let mut x = x0 + gap;
    for ((k, l), (kw, lw)) in SAMPLE.iter().zip(widths) {
        kit::key_cap(c, text, (x, top + 8, kw, cap), k, px, base, (pal().chip, pal().pill));
        x += kw + gap;
        text.draw(c, Face::Body, px, x, base, pal().soft, l);
        x += lw + 2 * gap;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::button_prompts::PromptStyle;

    #[test]
    fn left_and_right_switch_the_selected_choice_at_once_and_b_goes_back() {
        let h = std::thread::spawn(|| {
            button_prompts::set(PromptStyle::Nintendo);
            let mut sel = 0;
            let before = mark();
            assert!(!press(&mut sel, Button::Right));
            assert_eq!(palette::colours(), palette::Colours::IndigoCube, "the first row: Colours");
            assert!(!press(&mut sel, Button::Down) && !press(&mut sel, Button::Left));
            assert_eq!(font_style::get(), font_style::FontStyle::Rounded, "the second: Font");
            assert!(!press(&mut sel, Button::Down) && !press(&mut sel, Button::Right));
            assert_eq!(button_prompts::get(), PromptStyle::Xbox);
            assert_ne!(mark(), before, "the previews are painted again");
            assert!(!press(&mut sel, Button::Left) && !press(&mut sel, Button::Left));
            assert_eq!(button_prompts::get(), PromptStyle::PlayStation);
            assert!(!press(&mut sel, Button::Down));
            assert_eq!(sel, 0, "round to the first row");
            assert!(press(&mut sel, Button::South));
            button_prompts::set(PromptStyle::Nintendo);
        });
        h.join().unwrap();
    }
}
