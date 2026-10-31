//! Part of the pocket-ui binary: the App's side of `pocket_ui::night_light`. Settings → Look → Night light, the
//! thread that fades the display warmer in the evening, and what its steps do on the UI thread (one event each, no
//! work per frame). The choices, the schedule, the fade and the thread are in `src/night_light.rs`; how the white
//! point becomes ramps or a tint is `display.rs` (`Look::night`), applied by `Sys::set_night`.

use pocket_ui::gfx::Canvas;
use pocket_ui::night_light::{self, clock_label, strength_label, Mode, Prefs, STRENGTHS};
use pocket_ui::pad::Button;
use pocket_ui::palette::*;
use pocket_ui::display::Backend;
use pocket_ui::text::Face;
use pocket_ui::touch::Area;

use super::{App, SettingsPage, UserEvent, CANCEL, CONFIRM, NU_BTN_PAD, NU_PAD, NU_PILL, NU_SMALL};

/// The name in the Settings list, to come back to.
pub(crate) const SETTINGS_NAME: &str = "Night light";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Row {
    Mode,
    From,
    Until,
    Strength,
}

impl Row {
    fn label(self) -> &'static str {
        match self {
            Row::Mode => "Night light",
            Row::From => "From",
            Row::Until => "Until",
            Row::Strength => "Strength",
        }
    }
}

/// From and until only mean something on a schedule.
fn rows(p: &Prefs) -> &'static [Row] {
    match p.mode {
        Mode::Scheduled => &[Row::Mode, Row::From, Row::Until, Row::Strength],
        Mode::Off | Mode::On => &[Row::Mode, Row::Strength],
    }
}

fn value(p: &Prefs, row: Row) -> String {
    match row {
        Row::Mode => p.mode.label().to_string(),
        Row::From => clock_label(p.from),
        Row::Until => clock_label(p.until),
        Row::Strength => strength_label(p.strength).to_string(),
    }
}

/// What the selected row is for.
fn describe(p: &Prefs, row: Row) -> &'static str {
    match (row, p.mode) {
        (Row::Mode, Mode::Off) => "Off: the screen keeps its own colours.",
        (Row::Mode, Mode::On) => "On: warmer, less blue colours all the time, in the menus and in games.",
        (Row::Mode, Mode::Scheduled) => "Scheduled: warmer, less blue colours every evening, from one time to another.",
        (Row::From, _) => "When the screen starts to turn warmer. A, or left and right, moves it by half an hour.",
        (Row::Until, _) => "When the colours are back to normal. A, or left and right, moves it by half an hour.",
        (Row::Strength, _) => "How warm it gets. A change takes a second or two, even in the middle of a game.",
    }
}

/// The minute of the day on the device's clock (the page's "On now").
fn minute_now() -> u32 {
    let unix = night_light::clock_unix();
    pocket_ui::appearance::local_minute(unix, pocket_ui::appearance::utc_offset_now(unix))
}

impl App {
    /// Starts the thread, once the window exists and the first frames are no longer the point (not for snapshots).
    /// Nothing fades at the start: `Sys` took the level in its first look.
    pub(crate) fn night_light_start(&mut self) {
        let proxy = std::sync::Mutex::new(self.proxy.clone());
        night_light::start(self.night, self.sys.look().night, move |level| {
            let _ = proxy.lock().unwrap().send_event(UserEvent::Night(level));
        });
        pocket_ui::night_light::set_instant(pocket_ui::motion::level() == pocket_ui::motion::Motion::Off);
    }

    /// A step of the fade from the thread: the compositor's ramps or the tint, through `Sys`.
    pub(crate) fn night_light_event(&mut self, level: u16) {
        let game = self.sys_ctx().game;
        if self.sys.set_night(level, game) {
            self.request_redraw();
        }
    }

    /// Settings → Look → Motion changed: Off makes a switch one step.
    pub(crate) fn night_light_motion_changed(&self) {
        night_light::set_instant(pocket_ui::motion::level() == pocket_ui::motion::Motion::Off);
    }

    /// Waking from sleep: the clock moved on while nothing ran, so the thread looks at it again (its timer fires on
    /// resume as well, this covers a light sleep, where the screen was off and the process went on). A comparison
    /// per pass through `about_to_wait`.
    pub(crate) fn night_light_tick(&mut self) {
        let asleep = self.sleep.state() != pocket_ui::sleep::State::Awake;
        if std::mem::replace(&mut self.night_asleep, asleep) && !asleep {
            night_light::poke();
        }
    }

    pub(crate) fn open_night_light(&mut self) -> Option<SettingsPage> {
        Some(SettingsPage::NightLight(0))
    }

    /// A key on the page. The choice is saved at once and handed to the thread, which fades to it.
    pub(crate) fn night_light_input(&mut self, sel: usize, b: Button) -> Option<SettingsPage> {
        let list = rows(&self.night);
        let sel = sel.min(list.len() - 1);
        let step = match b {
            Button::Up => return Some(SettingsPage::NightLight((sel + list.len() - 1) % list.len())),
            Button::Down => return Some(SettingsPage::NightLight((sel + 1) % list.len())),
            Button::Left => -1,
            Button::Right => 1,
            b if b == CONFIRM => 1,
            b if b == CANCEL => return Some(SettingsPage::List(super::settings_row(SETTINGS_NAME))),
            _ => return Some(SettingsPage::NightLight(sel)),
        };
        let before = self.night;
        match list[sel] {
            Row::Mode => self.night.step_mode(step),
            Row::From => self.night.step_from(step),
            Row::Until => self.night.step_until(step),
            // A goes round (a button has no "back"); left and right stop at the ends.
            Row::Strength if b == CONFIRM && self.night.strength == STRENGTHS => self.night.strength = 1,
            Row::Strength => self.night.step_strength(step),
        }
        if self.night != before {
            if !self.night.save(&self.data_dir) {
                pocket_ui::ui_sound::refuse();
            }
            night_light::set_prefs(self.night);
        }
        // The rows change with the mode: the cursor stays on one that is still there.
        let sel = sel.min(rows(&self.night).len() - 1);
        Some(SettingsPage::NightLight(sel))
    }

    /// Where the page's choices stand: a change is what the menu sounds and haptics answer.
    pub(crate) fn night_light_row(&self) -> Prefs {
        self.night
    }

    /// Rows in the style of the Performance & fan page: name, and at the right the value with arrows on the
    /// selected row; under them what the row does, and what the night light is doing now.
    pub(crate) fn paint_night_light(&mut self, c: &mut Canvas, sel: usize) {
        c.clear(pal().void);
        let (pad, row, font) = (NU_PAD, pocket_ui::type_scale::row(66, 36), 36u32);
        self.sheet_header(c, SETTINGS_NAME);
        let (top, right) = (pad + NU_PILL + 12, c.w - pad);
        let prefs = self.night;
        let list = rows(&prefs);
        let sel = sel.min(list.len() - 1);
        for (i, r) in list.iter().enumerate() {
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
            let v = value(&prefs, *r);
            let shown = if i == sel { format!("‹ {v} ›") } else { v };
            let vx = right - NU_BTN_PAD;
            self.text.draw_right(c, Face::Body, font, vx, base, pal().text, &shown);
            let vw = self.text.width(Face::Body, font, &shown);
            // The arrows of the selected row are buttons of their own for a finger.
            if i == sel {
                let aw = self.text.width(Face::Body, font, "‹ ") + 24;
                self.hits.button(Area::new(vx - vw - 24, y, aw, row), Button::Left);
                self.hits.button(Area::new(vx - aw + 24, y, aw, row), Button::Right);
            }
            // Left of the strength: five pips, as many lit as the strength.
            if *r == Row::Strength {
                let (pw, ph, gap) = (14, 28, 8);
                let mut x = vx - vw - 20 - (STRENGTHS as i32 * (pw + gap) - gap);
                for n in 0..STRENGTHS {
                    let lit = n < prefs.strength;
                    c.round_rect(x, base - ph + 4, pw, ph, 4, if lit { pal().text } else { pal().pick(0x4a4a4a, 0xcfc8ba) });
                    x += pw + gap;
                }
            }
        }
        self.hits.list(sel, list.len(), 0.0, row as f32);

        let (left, width) = (pad + NU_BTN_PAD, c.w - 2 * (pad + NU_BTN_PAD));
        let mut y = top + list.len() as i32 * row + 44;
        let mut lines = vec![(describe(&prefs, list[sel]).to_string(), pal().text), (prefs.status(minute_now()), pal().dust)];
        // Only the Soft look (a compositor without gamma tables, the Mini V2's probably) leaves some emulators out.
        if self.sys.backend() == Backend::Soft {
            lines.push(("Here it warms the menus and the games pocket-core runs, not RetroArch or the other emulators.".to_string(), pal().dust));
        }
        for (text, color) in lines {
            for line in self.text.wrap(Face::Body, NU_SMALL, &text, width) {
                self.text.draw(c, Face::Body, NU_SMALL, left, y, color, &line);
                y += pocket_ui::type_scale::line(NU_SMALL, 12);
            }
            y += 14;
        }
        self.nu_hints(c, &[("B", "Back"), ("A", "Next"), ("< >", "Change")]);
    }

    /// `night[:<off|on|scheduled>][:<row>][:<strength>]` for the screenshots, with made-up choices.
    pub(crate) fn night_light_snapshot(&mut self, state: &str) -> bool {
        let Some(rest) = state.strip_prefix("night") else { return false };
        if !rest.is_empty() && !rest.starts_with(':') {
            return false;
        }
        let mut parts = rest.trim_start_matches(':').split(':');
        let mode = match parts.next().unwrap_or_default() {
            "on" => Mode::On,
            "scheduled" => Mode::Scheduled,
            _ => Mode::Off,
        };
        let row: usize = parts.next().and_then(|r| r.parse().ok()).unwrap_or(0);
        let strength: u8 = parts.next().and_then(|r| r.parse().ok()).unwrap_or(3);
        self.night = Prefs { mode, strength: strength.clamp(1, STRENGTHS), ..Prefs::default() };
        self.settings_page = Some(SettingsPage::NightLight(row));
        true
    }
}
