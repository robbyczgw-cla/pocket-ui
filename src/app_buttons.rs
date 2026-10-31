//! Part of the pocket-ui binary: Settings → Playing → **Buttons**. The two settings about the face buttons on one page
//! (2026-10-30: they were in two places): **Button layout**, which button confirms in the menus
//! (`pocket_ui::button_layout`), and **Button symbols**, how the hints draw them (`pocket_ui::button_prompts`). Under
//! the rows a small picture of the four buttons where they sit on the device, each with the symbol it shows in the
//! hints, the one that confirms and the one that goes back named beside it.
//!
//! A change is used at once (the pads' role and the symbols are process wide) and saved at once. settings.json keeps
//! its two keys (`button_layout`, `prompts`). Nothing here reads a file or starts work.

use pocket_ui::button_layout::ButtonLayout;
use pocket_ui::button_prompts;
use pocket_ui::gfx::Canvas;
use pocket_ui::pad::Button;
use pocket_ui::palette::*;
use pocket_ui::text::Face;
use pocket_ui::touch::Area;

use super::{settings_row, App, SettingsPage, CANCEL, CONFIRM, NU_BTN_PAD, NU_PAD, NU_PILL};

/// The name in the Settings list, to come back to.
pub(crate) const SETTINGS_NAME: &str = "Buttons";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Row {
    Layout,
    Symbols,
}

const ROWS: [Row; 2] = [Row::Layout, Row::Symbols];

impl Row {
    fn label(self) -> &'static str {
        match self {
            Row::Layout => "Button layout",
            Row::Symbols => "Button symbols",
        }
    }
}

/// The line under the rows: what the selected one decides, then where the two buttons are now.
fn desc(row: Row, layout: ButtonLayout) -> String {
    let what = match row {
        Row::Layout => "Which button confirms in the menus. Games keep their own buttons.",
        Row::Symbols => "How the buttons look in the hints: letters, Xbox colours or PlayStation shapes.",
    };
    let now = match layout {
        ButtonLayout::Nintendo => "The right button confirms, the bottom one goes back.",
        ButtonLayout::Xbox => "The bottom button confirms, the right one goes back.",
    };
    format!("{what}\n{now}")
}

impl App {
    pub(crate) fn open_buttons(&mut self, sel: usize) -> Option<SettingsPage> {
        Some(SettingsPage::Buttons(sel.min(ROWS.len() - 1)))
    }

    /// A key on the page: Up/Down move, A, Left or Right change the selected row; used and saved at once.
    pub(crate) fn buttons_input(&mut self, sel: usize, b: Button) -> Option<SettingsPage> {
        let sel = sel.min(ROWS.len() - 1);
        match b {
            Button::Up | Button::Down => return Some(SettingsPage::Buttons((sel + 1) % ROWS.len())),
            b if b == CANCEL => return Some(SettingsPage::List(settings_row(SETTINGS_NAME))),
            _ => {}
        }
        if b == CONFIRM || matches!(b, Button::Left | Button::Right) {
            match ROWS[sel] {
                Row::Layout => self.switch_button_layout(),
                Row::Symbols => {
                    button_prompts::set(button_prompts::get().step(b != Button::Left));
                    self.save_settings();
                }
            }
        }
        Some(SettingsPage::Buttons(sel))
    }

    /// The Playing row's value: the layout, and the symbols when they are another style ("Nintendo · Xbox").
    pub(crate) fn buttons_list_value(&self) -> String {
        let (layout, symbols) = (self.button_layout.label(), button_prompts::get().label());
        if layout == symbols {
            layout.to_string()
        } else {
            format!("{layout} · {symbols}")
        }
    }

    pub(crate) fn paint_buttons(&mut self, c: &mut Canvas, sel: usize) {
        c.clear(pal().void);
        let (pad, row, font) = (NU_PAD, pocket_ui::type_scale::row(66, 36), 36u32);
        self.sheet_header(c, SETTINGS_NAME);
        let (top, right) = (pad + NU_PILL + 12, c.w - pad);
        let sel = sel.min(ROWS.len() - 1);
        let foot = pocket_ui::kit::footer(c, &mut self.text, &mut self.hits, &[("B", "Back"), ("A", "Switch")], &desc(ROWS[sel], self.button_layout), 4, pal().text);
        for (i, r) in ROWS.iter().enumerate() {
            let y = top + i as i32 * row;
            self.hits.row(Area::new(pad, y, right - pad, row), i);
            let v = match r {
                Row::Layout => self.button_layout.label(),
                Row::Symbols => button_prompts::get().label(),
            };
            let shown = if i == sel { format!("‹ {v} ›") } else { v.to_string() };
            let vw = self.text.width(Face::Body, font, &shown);
            let name = self.text.fit(Face::Body, font, r.label(), right - pad - 2 * NU_BTN_PAD - vw - 24);
            let base = pocket_ui::kit::row_label(c, &mut self.text, (pad, y, right - pad, row), font, &name, i == sel);
            let ink = if i == sel { pal().ink } else { pal().text };
            self.text.draw_right(c, Face::Body, font, right - NU_BTN_PAD, base, ink, &shown);
        }
        self.hits.list(sel, ROWS.len(), 0.0, row as f32);
        self.paint_button_diamond(c, top + ROWS.len() as i32 * row + 16, foot - 16);
    }

    /// The four face buttons as they sit on the device, between `top` and `bottom`: each shows the symbol of the role
    /// the layout gives it, the confirming and the going-back one named at their right. Smaller when the room is
    /// small (Large text on 640x480); left out only when not even a small one fits.
    fn paint_button_diamond(&mut self, c: &mut Canvas, top: i32, bottom: i32) {
        let room = bottom - top;
        // Three caps and two gaps high.
        let d = ((room - 16) / 3).min(64);
        if d < 28 {
            return;
        }
        let gap = d / 6;
        let (cx, cy) = (c.w / 2 - d, top + room / 2);
        let step = d / 2 + gap + d / 2;
        let px = (d as u32 * 30 / 64).max(16);
        let layout = self.button_layout;
        // The device's cluster, faint behind the caps.
        let r = step + d / 2 + gap;
        c.round_rect(cx - r, cy - r, 2 * r, 2 * r, r, pal().spine);
        for (physical, dx, dy) in [(Button::North, 0, -step), (Button::East, step, 0), (Button::South, 0, step), (Button::West, -step, 0)] {
            let role = layout.role(physical);
            let key = match role {
                Button::East => "A",
                Button::South => "B",
                Button::North => "X",
                _ => "Y",
            };
            let (x, y) = (cx + dx - d / 2, cy + dy - d / 2);
            let base = y + d / 2 + pocket_ui::type_scale::mid(px);
            pocket_ui::kit::key_cap(c, &mut self.text, (x, y, d, d), key, px, base, (pal().chip, pal().pill));
            let label = match role {
                b if b == CONFIRM => "Confirm",
                b if b == CANCEL => "Back",
                _ => continue,
            };
            // Beside the cap, right of the cluster: the bottom one's label under the right one's.
            let lx = cx + r + 24;
            let ly = y + d / 2 + pocket_ui::type_scale::mid(28);
            c.rect(x + d + 6, y + d / 2 - 1, lx - 10 - (x + d + 6), 2, pal().keycap);
            self.text.draw(c, Face::Body, 28, lx, ly, pal().chalk, label);
        }
    }

    /// `buttons[:<row>][:<presses>]` for the screenshots and the journey test: the page at a row, then the presses
    /// `u`, `d`, `l`, `r`, `a`, `b` handled as on the device (they save what they switch).
    pub(crate) fn buttons_snapshot(&mut self, state: &str) -> bool {
        let Some(rest) = state.strip_prefix("buttons") else { return false };
        if !rest.is_empty() && !rest.starts_with(':') {
            return false;
        }
        let mut parts = rest.trim_start_matches(':').split(':');
        let row: usize = parts.next().and_then(|r| r.parse().ok()).unwrap_or(0);
        let mut page = SettingsPage::Buttons(row.min(ROWS.len() - 1));
        for key in parts.next().unwrap_or_default().chars() {
            let b = match key {
                'u' => Button::Up,
                'd' => Button::Down,
                'l' => Button::Left,
                'r' => Button::Right,
                'a' => CONFIRM,
                'b' => CANCEL,
                _ => continue,
            };
            page = match page {
                SettingsPage::Buttons(sel) => self.buttons_input(sel, b).unwrap_or(page),
                other => other,
            };
        }
        self.settings_page = Some(page);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_line_says_which_button_confirms_in_each_layout() {
        assert!(desc(Row::Layout, ButtonLayout::Nintendo).contains("The right button confirms"));
        assert!(desc(Row::Symbols, ButtonLayout::Xbox).contains("The bottom button confirms"));
        // The picture names the roles where the layout puts them.
        assert_eq!(ButtonLayout::Nintendo.role(Button::East), CONFIRM);
        assert_eq!(ButtonLayout::Xbox.role(Button::South), CONFIRM);
    }
}
