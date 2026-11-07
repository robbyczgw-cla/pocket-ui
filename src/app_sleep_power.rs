//! Part of the pocket-ui binary: Settings → System → Sleep & power (`pocket_ui::sleep_settings`). Screen off after,
//! the slideshow before it, Sleep after, Switch off after sleeping, and the power button's short press and hold, on
//! one page; a change is kept in `sleep.json` and the sleep module counts with it at once (`Sleep::set_config`), the
//! slideshow is a cozy extra kept in settings.json (`App::switch_extra`). A Steam Deck has the slideshow alone. Up/Down choose a row,
//! Left/Right change it (stopping at the ends), A goes on to the next choice; the selected row is explained below.

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
            Action::Slideshow => self.switch_extra(pocket_ui::cozy_extras::Extra::IdleSlideshow),
        }
        Some(SettingsPage::SleepPower)
    }

    /// The Settings row's value: the screen-off time ("2 min", "Never"); the slideshow's On or Off where it is the
    /// page's only row (a Steam Deck).
    pub(crate) fn sleep_power_value(&self) -> String {
        self.sleep.config.value(rows()[0])
    }

    /// `--snapshot sleep[:<row>][:screen][:never][:keys=<presses>]`: the page with that row selected; `screen` with the
    /// short press on Screen off and the hold on Sleep, `never` with both times on Never, `old` with a file of before the
    /// page (3 min); then the presses `u`, `d`, `l`, `r`, `a` handled as on the device (the slideshow's switch is saved).
    pub(crate) fn sleep_power_snapshot(&mut self, state: &str) -> bool {
        let Some(rest) = state.strip_prefix("sleep") else { return false };
        if !(rest.is_empty() || rest.starts_with(':')) {
            return false;
        }
        let parts: Vec<&str> = rest.split(':').filter(|p| !p.is_empty()).collect();
        let mut config = pocket_ui::sleep_settings::Config::default();
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

    #[cfg(not(feature = "brick"))]
    pub(crate) fn paint_sleep_power(&mut self, c: &mut Canvas) {
        let page = self.sleep_page;
        let config = self.sleep.config;
        c.clear(pal().void);
        let (pad, row, font) = (NU_PAD, pocket_ui::type_scale::row(66, 36), 36u32);
        self.sheet_header(c, SETTINGS_NAME);
        let (top, right) = (pad + NU_PILL + 12, c.w - pad);
        let heading_h = pocket_ui::type_scale::line(NU_SMALL, 30);
        let mut y = top;
        for (i, r) in rows().iter().enumerate() {
            // The power button's rows under a heading of their own, a little apart from the times.
            if r.starts_group() {
                y += 14;
                self.text.draw(c, Face::Body, NU_SMALL, pad + NU_BTN_PAD, y + heading_h - 14, pal().dust, BUTTON_HEADING);
                y += heading_h;
            }
            self.hits.row(Area::new(pad, y, right - pad, row), i);
            let base = y + row / 2 + pocket_ui::type_scale::mid(font);
            let name = r.label();
            let selected = i == page.sel;
            if selected {
                pocket_ui::kit::selection(c, pad, y, right - pad, row);
                self.text.draw(c, Face::Body, font, pad + NU_BTN_PAD, base, pal().ink, name);
            } else {
                self.text.draw(c, Face::Body, font, pad + NU_BTN_PAD, base, pal().text, name);
            }
            let value = config.value(*r);
            let adjustable = !(pocket_ui::device::current().screen_off_only() && *r == Row::ShortPress);
            let shown = if selected && adjustable { format!("‹ {value} ›") } else { value };
            let vx = right - NU_BTN_PAD;
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
        // Touch: the rows are not evenly spaced (the heading), so each has its own area above; the list only says
        // which one is selected.
        self.hits.list(page.sel, rows().len(), 0.0, row as f32);

        // What the selected row does with its value now.
        let (left, width) = (pad + NU_BTN_PAD, c.w - 2 * (pad + NU_BTN_PAD));
        // Close under the rows: with the slideshow's row a German line needs its third line at Large text on 640x480.
        let mut ly = y + 24 + pocket_ui::type_scale::px(NU_SMALL) as i32 / 2;
        let step = pocket_ui::type_scale::line(NU_SMALL, 12);
        let hints_top = c.h - pad - NU_PILL - 8;
        if pocket_ui::device::current().screen_off_only() {
            for line in self.text.wrap_prose(Face::Body, NU_SMALL, pocket_ui::sleep_settings::NO_SLEEP, width) {
                self.text.draw(c, Face::Body, NU_SMALL, left, ly, pal().dust, &line);
                ly += step;
            }
            ly += 12;
        }
        let room = ((hints_top - ly) / step + 1).max(1) as usize;
        for line in self.text.wrap_prose_at_most(Face::Body, NU_SMALL, config.explain(page.row()), width, room) {
            self.text.draw(c, Face::Body, NU_SMALL, left, ly, pal().text, &line);
            ly += step;
        }
        // Short press on a handheld that only turns its screen off is fixed (no arrows above): no key offered for it (the
        // navigation explorer found "A Next" doing nothing there, Brick 2026-11-07).
        let fixed = pocket_ui::device::current().screen_off_only() && page.row() == Row::ShortPress;
        let hints: &[(&str, &str)] = if fixed { &[("B", "Back")] } else { &[("B", "Back"), ("A", "Next"), ("< >", "Change")] };
        self.nu_hints(c, hints);
    }

    #[cfg(feature = "brick")]
    pub(crate) fn paint_sleep_power(&mut self, c: &mut Canvas) {
        let page = self.sleep_page;
        let config = self.sleep.config;
        c.clear(pal().void);
        let (pad, row, font) = (NU_PAD, pocket_ui::type_scale::row(66, 36), 36u32);
        self.sheet_header(c, SETTINGS_NAME);
        let (top, right) = (pad + NU_PILL + 12, c.w - pad);
        let heading_h = pocket_ui::type_scale::line(NU_SMALL, 30);
        let mut y = top;
        for (i, r) in rows().iter().enumerate() {
            // The power button's rows under a heading of their own, a little apart from the times.
            if r.starts_group() {
                y += 14;
                self.text.draw(c, Face::Body, NU_SMALL, pad + NU_BTN_PAD, y + heading_h - 14, pal().dust, BUTTON_HEADING);
                y += heading_h;
            }
            self.hits.row(Area::new(pad, y, right - pad, row), i);
            let base = y + row / 2 + pocket_ui::type_scale::mid(font);
            let selected = i == page.sel;
            let value = config.value(*r);
            let adjustable = !(pocket_ui::device::current().screen_off_only() && *r == Row::ShortPress);
            let shown = if selected && adjustable { format!("‹ {value} ›") } else { value };
            let vx = right - NU_BTN_PAD;
            // The name leaves room for the value: at Large text "Presentación antes de apagar la pantalla" ran into "Desactivado" (Brick).
            let room = vx - self.text.width(Face::Body, font, &shown) - 12 - (pad + NU_BTN_PAD);
            let name = self.text.fit(Face::Body, font, r.label(), room);
            if selected {
                pocket_ui::kit::selection(c, pad, y, right - pad, row);
                self.text.draw(c, Face::Body, font, pad + NU_BTN_PAD, base, pal().ink, &name);
            } else {
                self.text.draw(c, Face::Body, font, pad + NU_BTN_PAD, base, pal().text, &name);
            }
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
        // Touch: the rows are not evenly spaced (the heading), so each has its own area above; the list only says
        // which one is selected.
        self.hits.list(page.sel, rows().len(), 0.0, row as f32);

        // What the selected row does with its value now.
        let (left, width) = (pad + NU_BTN_PAD, c.w - 2 * (pad + NU_BTN_PAD));
        // Close under the rows: with the slideshow's row a German line needs its third line at Large text on 640x480.
        let mut ly = y + 24 + pocket_ui::type_scale::px(NU_SMALL) as i32 / 2;
        let step = pocket_ui::type_scale::line(NU_SMALL, 12);
        // The last baseline: its letters' feet stay above the hints (at Large text a line ran under them on the Brick's 768 lines).
        let last = c.h - pad - NU_PILL - 8 - pocket_ui::type_scale::px(NU_SMALL) as i32 / 4;
        let lines_from = |y: i32| if y > last { 0 } else { ((last - y) / step + 1) as usize };
        if pocket_ui::device::current().screen_off_only() {
            // The note goes when it would leave the selected row's explanation less than two lines (or all of it).
            let note = self.text.wrap_prose(Face::Body, NU_SMALL, pocket_ui::sleep_settings::no_sleep(), width);
            let explain = self.text.wrap_prose(Face::Body, NU_SMALL, config.explain(page.row()), width).len();
            if lines_from(ly + note.len() as i32 * step + 12) >= explain.min(2) {
                for line in note {
                    self.text.draw(c, Face::Body, NU_SMALL, left, ly, pal().dust, &line);
                    ly += step;
                }
                ly += 12;
            }
        }
        let room = lines_from(ly);
        for line in self.text.wrap_prose_at_most(Face::Body, NU_SMALL, config.explain(page.row()), width, room).into_iter().take(room) {
            self.text.draw(c, Face::Body, NU_SMALL, left, ly, pal().text, &line);
            ly += step;
        }
        // As in the other painter: Short press is fixed where the screen only turns off, so no key is offered for it.
        let fixed = pocket_ui::device::current().screen_off_only() && page.row() == Row::ShortPress;
        let hints: &[(&str, &str)] = if fixed { &[("B", "Back")] } else { &[("B", "Back"), ("A", "Next"), ("< >", "Change")] };
        self.nu_hints(c, hints);
    }
}
