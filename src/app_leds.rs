//! Part of the pocket-ui binary: the App's side of `pocket_ui::leds`. Settings > LEDs (the lights around the sticks),
//! and the game's own colour while a game runs. The choice, the scripts and the first start are in `src/leds.rs`.

use pocket_ui::gfx::Canvas;
use pocket_ui::leds::{self, Config, Hue, Job, Leds, Mode, LEVELS};
use pocket_ui::pad::Button;
use pocket_ui::palette::*;
use pocket_ui::text::Face;
use pocket_ui::touch::Area;

use super::{stripe, App, SettingsPage, CANCEL, CONFIRM, NU_BTN_PAD, NU_PAD, NU_PILL, NU_SMALL};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Row {
    Mode,
    Colour,
    Brightness,
}

impl Row {
    fn label(self) -> &'static str {
        match self {
            Row::Mode => "Lights",
            Row::Colour => "Colour",
            Row::Brightness => "Brightness",
        }
    }
}

/// Colour and brightness only mean something for one colour; the battery colours are ROCKNIX's own and always full.
fn rows(c: &Config) -> &'static [Row] {
    match c.mode {
        Mode::Colour => &[Row::Mode, Row::Colour, Row::Brightness],
        Mode::Off | Mode::Battery => &[Row::Mode],
    }
}

impl App {
    pub(crate) fn open_leds(&mut self) -> Option<SettingsPage> {
        Some(SettingsPage::Leds(0))
    }

    /// A key on the page. The choice is saved at once and handed to ROCKNIX's scripts on a thread of its own.
    pub(crate) fn leds_input(&mut self, sel: usize, b: Button) -> Option<SettingsPage> {
        let rows = rows(&self.leds);
        let sel = sel.min(rows.len() - 1);
        let step = match b {
            Button::Up => return Some(SettingsPage::Leds((sel + rows.len() - 1) % rows.len())),
            Button::Down => return Some(SettingsPage::Leds((sel + 1) % rows.len())),
            Button::Left => -1,
            Button::Right => 1,
            b if b == CONFIRM => 1,
            b if b == CANCEL => return Some(SettingsPage::List(super::settings_row("LEDs"))),
            _ => return Some(SettingsPage::Leds(sel)),
        };
        let before = self.leds;
        match rows[sel] {
            Row::Mode => self.leds.step_mode(step),
            Row::Colour => self.leds.step_hue(step),
            // A goes round (a button has no "back"); left and right stop at the ends.
            Row::Brightness if b == CONFIRM && self.leds.level == LEVELS.len() - 1 => self.leds.level = 0,
            Row::Brightness => self.leds.step_level(step),
        }
        if self.leds != before {
            self.leds.save(&self.data_dir);
            leds::apply_in_background(Job::Choice(self.leds));
        }
        Some(SettingsPage::Leds(sel))
    }

    /// A game starts: with the game's colour chosen, the sticks take the colour of its system.
    pub(crate) fn leds_game_started(&self, system: &str) {
        if self.leds.mode == Mode::Colour && self.leds.hue == Hue::Game {
            leds::apply_in_background(Job::Direct(self.leds.level, leds::unpack(stripe(system))));
        }
    }

    /// The game ended: back to the menu colour.
    pub(crate) fn leds_game_ended(&self) {
        if self.leds.mode == Mode::Colour && self.leds.hue == Hue::Game {
            leds::apply_in_background(Job::Direct(self.leds.level, Hue::Game.rgb()));
        }
    }

    /// Rows in the style of the Sync page: name, and at the right the value (with arrows on the selected row).
    pub(crate) fn paint_leds(&mut self, c: &mut Canvas, sel: usize) {
        c.clear(pal().void);
        let (pad, row, font) = (NU_PAD, pocket_ui::type_scale::row(66, 36), 36u32);
        self.status_pill(c);
        self.text.draw(c, Face::Title, 50, pad + NU_BTN_PAD, pad + 58, pal().chalk, "LEDs");
        let (top, right) = (pad + NU_PILL + 12, c.w - pad);
        let cfg = self.leds;
        let rows = rows(&cfg);
        let sel = sel.min(rows.len() - 1);
        for (i, r) in rows.iter().enumerate() {
            let y = top + i as i32 * row;
            self.hits.row(Area::new(pad, y, right - pad, row), i);
            let base = y + row / 2 + pocket_ui::type_scale::mid(font);
            let name = r.label();
            if i == sel {
                let w = self.text.width(Face::Body, font, name) + 2 * NU_BTN_PAD;
                c.round_rect(pad, y + 4, right - pad, row - 8, (row - 8) / 2, pal().band);
                c.round_rect(pad, y + 4, w, row - 8, (row - 8) / 2, pal().select);
                self.text.draw(c, Face::Body, font, pad + NU_BTN_PAD, base, pal().ink, name);
            } else {
                self.text.draw(c, Face::Body, font, pad + NU_BTN_PAD, base, pal().text, name);
            }
            let value = match r {
                Row::Mode => cfg.mode.label().to_string(),
                Row::Colour => cfg.hue.label().to_string(),
                Row::Brightness => cfg.brightness_label(),
            };
            let value = if i == sel { format!("‹ {value} ›") } else { value };
            let vx = right - NU_BTN_PAD;
            self.text.draw_right(c, Face::Body, font, vx, base, pal().text, &value);
            let vw = self.text.width(Face::Body, font, &value);
            // Left of the value: a dot in the colour, or four pips for the brightness.
            match r {
                Row::Colour => {
                    let d = 28;
                    let (cr, cg, cb) = cfg.hue.rgb();
                    c.round_rect(vx - vw - 20 - d, base - d + 4, d, d, d / 2, (cr as u32) << 16 | (cg as u32) << 8 | cb as u32);
                }
                Row::Brightness => {
                    let (pw, ph, gap) = (14, 28, 8);
                    let mut x = vx - vw - 20 - (LEVELS.len() as i32 * (pw + gap) - gap);
                    for n in 0..LEVELS.len() {
                        let lit = n <= cfg.level;
                        c.round_rect(x, base - ph + 4, pw, ph, 4, if lit { pal().text } else { pal().pick(0x4a4a4a, 0xcfc8ba) });
                        x += pw + gap;
                    }
                }
                Row::Mode => {}
            }
        }
        self.hits.list(sel, rows.len(), 0.0, row as f32);

        // What the selected row does, and a word when this machine has no such lights.
        let desc = match rows[sel] {
            Row::Mode => cfg.mode.blurb(),
            Row::Colour => cfg.hue.blurb(),
            Row::Brightness => "How bright both sticks glow.",
        };
        let (left, mut y) = (pad + NU_BTN_PAD, top + rows.len() as i32 * row + 44);
        for line in self.text.wrap(Face::Body, NU_SMALL, desc, c.w - 4 * pad) {
            self.text.draw(c, Face::Body, NU_SMALL, left, y, pal().text, &line);
            y += pocket_ui::type_scale::line(NU_SMALL, 12);
        }
        if !Leds::system().present() {
            self.text.draw(
                c,
                Face::Body,
                NU_SMALL,
                left,
                y + 14,
                pal().dust,
                "No lights around the sticks found on this machine.",
            );
        }
        self.nu_hints(c, &[("B", "Back"), ("A", "Next"), ("< >", "Change")]);
    }

    /// `leds:<mode>:<row>` for the screenshots: `off`, `colour` (blue), `colour-game`, `battery`; the row to select.
    pub(crate) fn leds_snapshot(&mut self, state: &str) -> bool {
        let Some(rest) = state.strip_prefix("leds") else { return false };
        let mut parts = rest.trim_start_matches(':').split(':');
        let mode = parts.next().unwrap_or_default();
        let row: usize = parts.next().and_then(|r| r.parse().ok()).unwrap_or(0);
        self.leds = match mode {
            "colour" => Config {
                mode: Mode::Colour,
                ..Config::default()
            },
            "game" => Config {
                mode: Mode::Colour,
                hue: Hue::Game,
                level: 2,
            },
            "rose" => Config {
                mode: Mode::Colour,
                hue: Hue::Rose,
                level: 3,
            },
            "battery" => Config {
                mode: Mode::Battery,
                ..Config::default()
            },
            _ => Config::default(),
        };
        self.settings_page = Some(SettingsPage::Leds(row));
        true
    }
}
