//! Part of the pocket-ui binary: Settings > "Lights & buttons" on the Brick (docs/TRIMUI-BRICK-HW.md). One page with the lights
//! (off, colour, battery and charging), the F1 and F2 keys and the mute slider; "Each light" is one level down and sets the colour,
//! the effect and the brightness of the grips, the middle and the two F keys. Sensible defaults: dark lights, F1 quick save, F2
//! quick load, the slider mutes.

use pocket_ui::brick_hw::{Glow, LIGHT_NAMES};
use pocket_ui::gfx::Canvas;
use pocket_ui::leds::{self, Job, Mode, LEVELS};
use pocket_ui::pad::Button;
use pocket_ui::palette::*;
use pocket_ui::text::Face;
use pocket_ui::touch::Area;

use super::{App, SettingsPage, CANCEL, CONFIRM, NU_BTN_PAD, NU_PAD, NU_PILL, NU_SMALL};

/// The page's title, and the name of its row in Settings > Sound & feel on the Brick (`app_settings_groups::row_name`): row and page
/// carry the same words (Alex, 06.11.: the row said "LEDs" and opened "Lights & buttons").
pub(crate) const TITLE: &str = "Lights & buttons";

/// The short value of that row: "Lights off" while they are dark (the first start), else what the lights do (Battery, or the colour).
pub(crate) fn list_value(cfg: &leds::Config) -> &'static str {
    if pocket_ui::brick_profile::board().is_some_and(|b| b.fkeys() && !b.leds()) { return "F1 / F2"; }
    match cfg.mode {
        Mode::Off => "Lights off",
        _ => cfg.summary(),
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Row {
    Mode,
    Colour,
    Brightness,
    Each,
    F1,
    F2,
    Slider,
    // one level down
    Light,
    LColour,
    LHue,
    LSat,
    LGlow,
    LSpeed,
    LBrightness,
}

impl Row {
    fn label(self) -> &'static str {
        match self {
            Row::Mode => "Lights",
            Row::Colour => "Colour",
            Row::Brightness => "Brightness",
            Row::Each => "Each light",
            Row::F1 => "F1 button",
            Row::F2 => "F2 button",
            Row::Slider => "Mute slider",
            Row::Light => "Light",
            Row::LColour => "Colour",
            Row::LHue => "Hue",
            Row::LSat => "Saturation",
            Row::LGlow => "Effect",
            Row::LSpeed => "Speed",
            Row::LBrightness => "Brightness",
        }
    }
}

/// Colour, brightness and the lights one by one only mean something for the colour mode. One level down, a light's hue and saturation
/// show only with its own colour, its speed only with an effect that moves, and its colour rows not while it shows the battery.
pub(crate) fn rows(mode: Mode, detail: bool, light: &pocket_ui::brick_hw::LightPref) -> Vec<Row> {
    if pocket_ui::brick_profile::board().is_some_and(|b| b.fkeys() && !b.leds()) { return vec![Row::F1, Row::F2]; }
    if detail {
        let mut r = vec![Row::Light];
        if !matches!(light.glow, Glow::Battery | Glow::Rainbow) {
            r.push(Row::LColour);
            if light.use_own {
                r.extend([Row::LHue, Row::LSat]);
            }
        }
        r.push(Row::LGlow);
        if light.glow.animated() {
            r.push(Row::LSpeed);
        }
        r.push(Row::LBrightness);
        return r;
    }
    let mut r = vec![Row::Mode];
    if mode == Mode::Colour {
        r.extend([Row::Colour, Row::Brightness, Row::Each]);
    }
    r.extend([Row::F1, Row::F2, Row::Slider]);
    r
}

impl App {
    pub(crate) fn brick_lights_input(&mut self, sel: usize, b: Button) -> Option<SettingsPage> {
        let detail = self.brick.detail && self.leds.mode == Mode::Colour;
        let rows = rows(self.leds.mode, detail, &self.brick.lights.zones[self.brick.light]);
        let sel = sel.min(rows.len() - 1);
        let step = match b {
            Button::Up => return Some(SettingsPage::Leds((sel + rows.len() - 1) % rows.len())),
            Button::Down => return Some(SettingsPage::Leds((sel + 1) % rows.len())),
            Button::Left => -1,
            Button::Right => 1,
            b if b == CONFIRM => 1,
            b if b == CANCEL && detail => {
                self.brick.detail = false;
                return Some(SettingsPage::Leds(rows_of_top_each()));
            }
            b if b == CANCEL => return Some(SettingsPage::List(super::app_settings_groups::leds_row())),
            _ => return Some(SettingsPage::Leds(sel)),
        };
        let (before, lights_before) = (self.leds, self.brick.lights);
        let n = self.brick.light;
        match rows[sel] {
            Row::Mode => self.leds.step_mode(step),
            Row::Colour => self.leds.step_hue(step),
            Row::Brightness if b == CONFIRM && self.leds.level == LEVELS.len() - 1 => self.leds.level = 0,
            Row::Brightness => self.leds.step_level(step),
            Row::Each => {
                self.brick.detail = true;
                return Some(SettingsPage::Leds(0));
            }
            Row::F1 | Row::F2 => {
                self.brick_key_step(if rows[sel] == Row::F1 { 1 } else { 2 }, step);
            }
            Row::Slider => {
                self.brick.slider = self.brick.slider.other();
                self.brick.slider.save(&self.data_dir);
            }
            Row::Light => self.brick.light = (n as i32 + step).rem_euclid(LIGHT_NAMES.len() as i32) as usize,
            Row::LColour => self.brick.lights.zones[n].step_colour(step),
            Row::LHue => self.brick.lights.zones[n].step_own_hue(step),
            Row::LSat => self.brick.lights.zones[n].step_own_sat(step),
            Row::LGlow => self.brick.lights.zones[n].step_glow(step),
            Row::LSpeed => self.brick.lights.zones[n].step_speed(step),
            Row::LBrightness => self.brick.lights.zones[n].step_level(step),
        }
        if self.brick.lights != lights_before {
            self.brick.lights.save(&self.data_dir);
            pocket_ui::brick_hw::set_lights(self.brick.lights);
        }
        if self.leds != before || self.brick.lights != lights_before {
            self.leds.save(&self.data_dir);
            leds::apply_in_background(Job::Choice(self.leds));
        }
        Some(SettingsPage::Leds(sel))
    }

    fn brick_lights_value(&self, r: Row) -> String {
        let cfg = self.leds;
        let z = self.brick.lights.zones[self.brick.light];
        match r {
            Row::Mode => match cfg.mode {
                Mode::Off => "Off".into(),
                Mode::Colour => "Colour".into(),
                Mode::Battery => "Battery & charging".into(),
            },
            Row::Colour => cfg.hue.label().into(),
            Row::Brightness => cfg.brightness_label(),
            Row::Each => if self.brick.lights == Default::default() { "Like the rest".into() } else { "Customised".into() },
            Row::F1 => self.brick.keys.f1.label().into(),
            Row::F2 => self.brick.keys.f2.label().into(),
            Row::Slider => self.brick.slider.label().into(),
            Row::Light => LIGHT_NAMES[self.brick.light].into(),
            Row::LColour if z.use_own => "Own colour".into(),
            Row::LColour => z.hue.map_or("Like the rest", |h| h.label()).into(),
            Row::LHue => format!("{}°", z.own.hue),
            Row::LSat => format!("{}%", z.own.sat),
            Row::LSpeed => z.speed.label().into(),
            Row::LGlow => z.glow.label().into(),
            Row::LBrightness => z.level.map_or("Like the rest".to_string(), |l| format!("{}%", LEVELS[(l as usize).min(LEVELS.len() - 1)])),
        }
    }

    fn brick_lights_desc(&self, r: Row) -> &'static str {
        match r {
            Row::Mode => self.leds.mode.blurb(),
            Row::Colour => self.leds.hue.blurb(),
            Row::Brightness => "How bright the lights glow.",
            Row::Each => "Colour, effect and brightness of the grips, the middle and the two F keys one by one. A: open.",
            Row::F1 | Row::F2 => "Shortcuts for all games; not game buttons. Choose their action here.",
            Row::Slider => "The slider on the side mutes the sound, or does nothing.",
            Row::Light => "Which light to set.",
            Row::LColour => "Its own colour, or the one above. Own colour: pick the hue and the saturation below.",
            Row::LHue => "Round the colour wheel: red, yellow, green, blue, violet and back.",
            Row::LSat => "From pale to full colour.",
            Row::LGlow => "Steady, breathing, fading, pulsing, a colour cycle, the battery and charger, or off; \"Like the rest\" follows the colour above.",
            Row::LSpeed => "How fast it breathes, fades, pulses or cycles.",
            Row::LBrightness => "Its own brightness, or the one above.",
        }
    }

    pub(crate) fn paint_brick_lights(&mut self, c: &mut Canvas, sel: usize) {
        c.clear(pal().void);
        let (pad, row, font) = (NU_PAD, pocket_ui::type_scale::row(66, 36), 36u32);
        let detail = self.brick.detail && self.leds.mode == Mode::Colour;
        self.sheet_header(c, if detail { "Each light" } else { TITLE });
        let (top, right) = (pad + NU_PILL + 12, c.w - pad);
        let rows = rows(self.leds.mode, detail, &self.brick.lights.zones[self.brick.light]);
        let sel = sel.min(rows.len() - 1);
        for (i, r) in rows.iter().enumerate() {
            let y = top + i as i32 * row;
            self.hits.row(Area::new(pad, y, right - pad, row), i);
            let base = y + row / 2 + pocket_ui::type_scale::mid(font);
            let name = r.label();
            if i == sel {
                pocket_ui::kit::selection(c, pad, y, right - pad, row);
                self.text.draw(c, Face::Body, font, pad + NU_BTN_PAD, base, pal().ink, name);
            } else {
                self.text.draw(c, Face::Body, font, pad + NU_BTN_PAD, base, pal().text, name);
            }
            let value = self.brick_lights_value(*r);
            let value = if i == sel && *r != Row::Each { format!("‹ {value} ›") } else { value };
            let vx = right - NU_BTN_PAD;
            pocket_ui::kit::row_value(c, &mut self.text, font, vx, base, &value, i == sel);
            let vw = self.text.width(Face::Body, font, &value);
            let dot = match r {
                Row::Colour => Some(self.leds.hue.rgb()),
                Row::LColour | Row::LHue | Row::LSat if self.brick.lights.zones[self.brick.light].use_own => Some(self.brick.lights.zones[self.brick.light].own.rgb()),
                Row::LColour => self.brick.lights.zones[self.brick.light].hue.map(|h| h.rgb()),
                _ => None,
            };
            if let Some((cr, cg, cb)) = dot {
                let d = 28;
                c.round_rect(vx - vw - 20 - d, base - d + 4, d, d, d / 2, (cr as u32) << 16 | (cg as u32) << 8 | cb as u32);
            }
        }
        self.hits.list(sel, rows.len(), 0.0, row as f32);

        let desc = self.brick_lights_desc(rows[sel]);
        let (left, mut y) = (pad + NU_BTN_PAD, top + rows.len() as i32 * row + 44);
        for line in self.text.wrap_prose(Face::Body, NU_SMALL, desc, c.w - 4 * pad) {
            self.text.draw(c, Face::Body, NU_SMALL, left, y, pal().text, &line);
            y += pocket_ui::type_scale::line(NU_SMALL, 12);
        }
        let _ = Glow::Same;
        self.nu_hints(c, &[("B", "Back"), ("A", if rows[sel] == Row::Each { "Open" } else { "Next" }), ("< >", "Change")]);
    }
}

/// The row "Each light" has on the page it was opened from (colour mode: Lights, Colour, Brightness, Each light).
fn rows_of_top_each() -> usize {
    3
}
