//! Settings → System → Sleep & power: screen-off, sleep and switch-off times, plus the power button.
//! Left/Right change the selected choice, A cycles it, and a change is saved and applied at once.

use pocket_ui::gfx::Canvas;
use pocket_ui::pad::Button;
use pocket_ui::palette::*;
use pocket_ui::sleep_settings::{rows, Action, Hold, Page, Row, ShortPress};
use pocket_ui::text::Face;
use pocket_ui::touch::Area;

use super::{App, SettingsPage, CANCEL, CONFIRM, NU_BTN_PAD, NU_PAD, NU_PILL, NU_SMALL};

/// The name in the Settings list, to come back to.
pub(crate) const SETTINGS_NAME: &str = "Sleep & power";

/// The heading over the power button's two rows (`Row::starts_group`).
pub(crate) const BUTTON_HEADING: &str = "Power button";

impl App {
    pub(crate) fn open_sleep_power(&mut self) -> Option<SettingsPage> {
        self.sleep_page = Page::new(self.sleep.config);
        Some(SettingsPage::SleepPower)
    }

    pub(crate) fn sleep_power_input(&mut self, b: Button) -> Option<SettingsPage> {
        // The values as they are now (the switch-off note's Off at the start may have changed them meanwhile).
        self.sleep_page.config = self.sleep.config;
        match self.sleep_page.press(b, CONFIRM, CANCEL) {
            Action::Nothing => {}
            Action::Close => return Some(SettingsPage::List(super::settings_row(SETTINGS_NAME))),
            Action::Save(config) => self.sleep.set_config(config),
        }
        Some(SettingsPage::SleepPower)
    }

    pub(crate) fn sleep_power_value(&self) -> String {
        self.sleep.config.value(rows()[0])
    }

    /// `--snapshot sleep[:<row>][:screen][:never][:keys=<presses>]`: the page with that row selected; `screen` with the
    /// short press on Screen off and the hold on Sleep, `never` with both times on Never, or `old` with a legacy file.
    pub(crate) fn sleep_power_snapshot(&mut self, state: &str) -> bool {
        let Some(rest) = state.strip_prefix("sleep") else { return false };
        if !(rest.is_empty() || rest.starts_with(':')) {
            return false;
        }
        let parts: Vec<&str> = rest.split(':').filter(|p| !p.is_empty()).collect();
        let mut config = pocket_ui::sleep_settings::Config::default();
        #[cfg(feature = "brick")]
        if pocket_ui::sleep_settings::brick_choice() {
            config.short_press = pocket_ui::sleep_settings::BRICK_POWER_DEFAULT;
        }
        if parts.contains(&"screen") {
            config.short_press = ShortPress::ScreenOff;
            config.hold = Hold::Sleep;
        }
        if parts.contains(&"never") {
            config.screen_off = None;
            config.sleep_after = None;
        }
        if parts.contains(&"old") {
            config.screen_off = Some(180);
        }
        let mut config = config.for_device(pocket_ui::device::current().screen_off_only());
        if pocket_ui::device::current().screen_off_only() {
            config.off_after = None;
        }
        let mut page = Page::new(config);
        page.sel = parts.first().and_then(|r| r.parse().ok()).unwrap_or(0).min(rows().len() - 1);
        // In memory only: a snapshot never writes sleep.json.
        self.sleep.config = config;
        self.sleep_page = page;
        self.settings_page = Some(SettingsPage::SleepPower);
        for key in parts.iter().filter_map(|p| p.strip_prefix("keys=")).flat_map(str::chars) {
            let b = match key {
                'u' => Button::Up,
                'd' => Button::Down,
                'l' => Button::Left,
                'r' => Button::Right,
                'a' => CONFIRM,
                _ => continue,
            };
            self.settings_page = self.sleep_power_input(b);
        }
        true
    }

    pub(crate) fn paint_sleep_power(&mut self, c: &mut Canvas) {
        let page = self.sleep_page;
        let config = self.sleep.config;
        c.clear(pal().void);
        let (pad, row, font) = (NU_PAD, pocket_ui::type_scale::row(66, 36), 36u32);
        self.sheet_header(c, SETTINGS_NAME);
        let (top, right) = (pad + NU_PILL + 12, c.w - pad);
        let heading_h = pocket_ui::type_scale::line(NU_SMALL, 30);
        // The same footer and scrolling window as Controllers and Cozy extras: help gives way before rows do.
        let fixed = !pocket_ui::sleep_settings::Config::adjustable(page.row()) && page.row() == Row::ShortPress;
        let hints: &[(&str, &str)] = if fixed { &[("B", "Back")] } else { &[("B", "Back"), ("A", "Next"), ("< >", "Change")] };
        let mut desc = config.explain(page.row()).to_string();
        // The Brick's own card says what its power button does in that row's own explanation instead.
        #[cfg(feature = "brick")]
        let note_here = !pocket_ui::sleep_settings::brick_choice();
        #[cfg(not(feature = "brick"))]
        let note_here = true;
        if pocket_ui::device::current().screen_off_only() && note_here {
            #[cfg(feature = "brick")]
            let note = pocket_ui::sleep_settings::no_sleep();
            #[cfg(not(feature = "brick"))]
            let note = pocket_ui::sleep_settings::NO_SLEEP;
            desc = format!("{note}\n{desc}");
        }
        // The Brick's larger type needs room for both the screen-off note and the selected row's explanation.
        // The existing scrolling window below reserves the whole footer before choosing the visible rows.
        #[cfg(feature = "brick")]
        let lines = if pocket_ui::device::brick() && pocket_ui::kit::short_screen(c.h) { 8 } else { 4 };
        #[cfg(not(feature = "brick"))]
        let lines = 4;
        let foot = pocket_ui::kit::footer(c, &mut self.text, &mut self.hits, hints, &desc, lines, pal().text);
        // Reserve the group heading as well as complete rows, even when the scroll window begins inside that group.
        let visible = (((foot - 12 - top - heading_h - 14) / row).max(1) as usize).min(rows().len());
        let first = super::window_start(page.sel, visible, rows().len());
        let mut y = top;
        for (i, r) in rows().iter().enumerate().skip(first).take(visible) {
            // The power button's rows under a heading of their own, a little apart from the times.
            if r.starts_group() || (i == first && *r == Row::Hold) {
                y += 14;
                self.text.draw(c, Face::Body, NU_SMALL, pad + NU_BTN_PAD, y + heading_h - 14, pal().dust, BUTTON_HEADING);
                y += heading_h;
            }
            self.hits.row(Area::new(pad, y, right - pad, row), i);
            let selected = i == page.sel;
            let value = config.value(*r);
            let adjustable = *r != Row::ShortPress || pocket_ui::sleep_settings::Config::adjustable(*r);
            let shown = if selected && adjustable { format!("‹ {value} ›") } else { value };
            let vx = right - NU_BTN_PAD;
            // The name leaves room for the value: at Large text "Presentación antes de apagar la pantalla" ran into "Desactivado" (Brick).
            let room = vx - self.text.width(Face::Body, font, &shown) - 12 - (pad + NU_BTN_PAD);
            let (name_px, name) = self.text.fit_sizes(Face::Body, &[font, 30], r.label(), room);
            let base = pocket_ui::kit::row_label(c, &mut self.text,
                (pad, y, right - pad, row), name_px, &name, selected);
            pocket_ui::kit::row_value(c, &mut self.text, font, vx, base, &shown, selected);
            // The arrows of the selected row are buttons of their own for a finger.
            if selected && adjustable {
                let vw = self.text.width(Face::Body, font, &shown);
                let aw = self.text.width(Face::Body, font, "‹ ") + 24;
                self.hits.button(Area::new(vx - vw - 24, y, aw, row), Button::Left);
                self.hits.button(Area::new(vx - aw + 24, y, aw, row), Button::Right);
            }
            y += row;
        }
        pocket_ui::kit::scrollbar(c, top, y - top, first, visible, rows().len());
        // The heading makes row spacing uneven; explicit areas above keep taps on the row they name.
        self.hits.list(page.sel, rows().len(), 0.0, row as f32);
    }
}
