//! Part of the pocket-ui binary: Settings → Look → Cozy extras (`pocket_ui::cozy_extras` has the switches). A list of
//! switches in the Settings style, one per extra you see (`Extra::PAGE_ROWS`), each On or Off; the selected one's line
//! under the list, with the looks it is seen in. The extras you hear or that move are rows of Sound & feel, the
//! slideshow is in Sleep & power (`Extra::home`); all of them switch through [`App::switch_extra`].
//!
//! A switch takes effect at once (an atomic, read where the extra is drawn or heard) and is saved at once with the
//! other settings. Nothing here reads a file or starts work.

use pocket_ui::cozy_extras::{self, Extra, Needs};
use pocket_ui::gfx::Canvas;
use pocket_ui::pad::Button;
use pocket_ui::palette::*;
use pocket_ui::settings::Animations;
use pocket_ui::text::Face;
use pocket_ui::touch::Area;

use super::{settings_row, window_start, App, SettingsPage, CANCEL, CONFIRM, NU_BTN_PAD, NU_PAD, NU_PILL};

/// The name in the Settings list, to come back to.
pub(crate) const SETTINGS_NAME: &str = "Cozy extras";

fn rows() -> Vec<Extra> {
    Extra::shown()
}

impl App {
    pub(crate) fn open_cozy_extras(&mut self) -> Option<SettingsPage> {
        Some(SettingsPage::CozyExtras(0))
    }

    /// Whether what extra `e` needs elsewhere is on.
    pub(crate) fn cozy_need_met(&self, n: Needs) -> bool {
        match n {
            Needs::Sounds => self.sound.on,
            Needs::CozyFull => self.animations_in_use() == Animations::Cozy && pocket_ui::motion::level() == pocket_ui::motion::Motion::Full,
            Needs::MotionFull => pocket_ui::motion::level() == pocket_ui::motion::Motion::Full,
        }
    }

    /// Extra `e` the other way, used and saved at once: from the page, a row of Sound & feel or of Sleep & power.
    pub(crate) fn switch_extra(&mut self, e: Extra) {
        let mut x = cozy_extras::get();
        x.set(e, !x.get(e));
        cozy_extras::set(x);
        self.cozy_extras_changed(e);
        self.save_settings();
    }

    /// The line under Settings row `line` for extra `e` (a row of Sound & feel): what keeps it from showing, while
    /// another switch does and it is on, else the row's own line.
    pub(crate) fn extra_row_line(&self, e: Extra, line: &'static str) -> &'static str {
        match e.needs().filter(|n| cozy_extras::on(e) && !self.cozy_need_met(*n)) {
            Some(n) => n.note(),
            None => line,
        }
    }

    /// A key on the page: A, Left or Right switch the selected extra, used and saved at once.
    pub(crate) fn cozy_extras_input(&mut self, sel: usize, b: Button) -> Option<SettingsPage> {
        let list = rows();
        let sel = sel.min(list.len() - 1);
        match b {
            Button::Up => return Some(SettingsPage::CozyExtras((sel + list.len() - 1) % list.len())),
            Button::Down => return Some(SettingsPage::CozyExtras((sel + 1) % list.len())),
            b if b == CANCEL => return Some(SettingsPage::List(settings_row(SETTINGS_NAME))),
            _ => {}
        }
        if b == CONFIRM || matches!(b, Button::Left | Button::Right) {
            self.switch_extra(list[sel]);
        }
        Some(SettingsPage::CozyExtras(sel))
    }

    /// What was made with the old switch and is kept is made again: the cases' pictures (stickers), the box's disc.
    fn cozy_extras_changed(&mut self, e: Extra) {
        if matches!(e, Extra::Stickers | Extra::PrintedDiscs) {
            self.cases_look_changed();
        }
    }

    /// Where the page's switches stand: a change is what the menu sounds and haptics answer.
    pub(crate) fn cozy_extras_mark(&self) -> cozy_extras::Extras {
        cozy_extras::get()
    }

    pub(crate) fn paint_cozy_extras(&mut self, c: &mut Canvas, sel: usize) {
        c.clear(pal().void);
        let (pad, row, font) = (NU_PAD, pocket_ui::type_scale::row(66, 36), 36u32);
        self.sheet_header(c, SETTINGS_NAME);
        let (top, right) = (pad + NU_PILL + 12, c.w - pad);
        let list = rows();
        let sel = sel.min(list.len() - 1);
        let x = cozy_extras::get();
        // The selected row's line and the looks it is seen in, under the list over the hints.
        let e = list[sel];
        let desc = match e.only() {
            Some(only) => format!("{}\n{only}", e.blurb()),
            None => e.blurb().to_string(),
        };
        let foot = pocket_ui::kit::footer(c, &mut self.text, &mut self.hits, &[("B", "Back"), ("A", "Switch")], &desc, 4, pal().text);
        let visible = (((foot - 12 - top) / row).max(1) as usize).min(list.len());
        let first = window_start(sel, visible, list.len());
        for (i, e) in list.iter().enumerate().skip(first).take(visible) {
            let y = top + (i - first) as i32 * row;
            self.hits.row(Area::new(pad, y, right - pad, row), i);
            let v = pocket_ui::settings_values::on_off(x.get(*e));
            let shown = if i == sel { format!("‹ {v} ›") } else { v.to_string() };
            // The name gives way to the value; both stay inside the row (German, Large text, 640x480).
            let vw = self.text.width(Face::Body, font, &shown);
            let name = self.text.fit(Face::Body, font, e.label(), right - pad - 2 * NU_BTN_PAD - vw - 24);
            let base = pocket_ui::kit::row_label(c, &mut self.text, (pad, y, right - pad, row), font, &name, i == sel);
            let ink = if i == sel { pal().ink } else { pal().text };
            self.text.draw_right(c, Face::Body, font, right - NU_BTN_PAD, base, ink, &shown);
        }
        pocket_ui::kit::scrollbar(c, top, visible as i32 * row, first, visible, list.len());
        self.hits.list(sel, list.len(), 0.0, row as f32);
    }

    /// `cozy[:<row>][:<keys off>][:<presses>]` for the screenshots and the journey test: the page at a row with the
    /// extras named by key (comma separated) switched away from their start, e.g. `cozy:2:cabinet_tv,case_sounds`
    /// (none named: as settings.json has them), then the presses `u`, `d`, `l`, `r`, `a`, `b` handled as on the
    /// device, which save what they switch (`cozy:0::dda`).
    pub(crate) fn cozy_extras_snapshot(&mut self, state: &str) -> bool {
        let Some(rest) = state.strip_prefix("cozy") else { return false };
        if !rest.is_empty() && !rest.starts_with(':') {
            return false;
        }
        let mut parts = rest.trim_start_matches(':').split(':');
        let row: usize = parts.next().and_then(|r| r.parse().ok()).unwrap_or(0);
        let off: Vec<&str> = parts.next().unwrap_or_default().split(',').filter(|k| !k.is_empty()).collect();
        if !off.is_empty() {
            let mut x = cozy_extras::Extras::default();
            for e in Extra::ALL.into_iter().filter(|e| off.contains(&e.key())) {
                x.set(e, !e.default_on());
            }
            cozy_extras::set(x);
        }
        let mut page = SettingsPage::CozyExtras(row);
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
                SettingsPage::CozyExtras(sel) => self.cozy_extras_input(sel, b).unwrap_or(page),
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

    /// Eight switches you see, no links to rows elsewhere (they are siblings of Animations and Sounds now).
    #[test]
    fn the_page_has_the_extras_you_see_and_nothing_else() {
        let r = rows();
        assert_eq!(r, Extra::PAGE_ROWS);
        assert!(r.len() <= 8);
        assert!(r.iter().all(|e| e.home() == pocket_ui::cozy_extras::Home::Page));
    }
}
