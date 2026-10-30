//! Part of the pocket-ui binary: Settings → Look & feel → Cozy extras (`pocket_ui::cozy_extras` has the switches).
//! A list of switches in the Settings style, one per extra, each On or Off; the selected one's line under the list,
//! with the looks it is seen in or, when another switch keeps it from showing (Sounds, Animations, Motion), which.
//! The last two rows lead to Animations and Sounds in Look & feel instead of repeating them.
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

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Row {
    Extra(Extra),
    /// Look & feel → Animations: Cozy or Quick.
    Animations,
    /// Look & feel → Sounds: the menu sounds.
    Sounds,
}

fn rows() -> Vec<Row> {
    Extra::shown().into_iter().map(Row::Extra).chain([Row::Animations, Row::Sounds]).collect()
}

impl App {
    pub(crate) fn open_cozy_extras(&mut self) -> Option<SettingsPage> {
        Some(SettingsPage::CozyExtras(0))
    }

    /// Whether what extra `e` needs elsewhere is on.
    fn cozy_need_met(&self, n: Needs) -> bool {
        match n {
            Needs::Sounds => self.sound.on,
            Needs::CozyFull => self.animations_in_use() == Animations::Cozy && pocket_ui::motion::level() == pocket_ui::motion::Motion::Full,
            Needs::MotionFull => pocket_ui::motion::level() == pocket_ui::motion::Motion::Full,
        }
    }

    /// A key on the page: A, Left or Right switch the selected extra; on the last two rows A goes to that row of
    /// Look & feel. The switch is used and saved at once.
    pub(crate) fn cozy_extras_input(&mut self, sel: usize, b: Button) -> Option<SettingsPage> {
        let list = rows();
        let sel = sel.min(list.len() - 1);
        match b {
            Button::Up => return Some(SettingsPage::CozyExtras((sel + list.len() - 1) % list.len())),
            Button::Down => return Some(SettingsPage::CozyExtras((sel + 1) % list.len())),
            b if b == CANCEL => return Some(SettingsPage::List(settings_row(SETTINGS_NAME))),
            _ => {}
        }
        match list[sel] {
            Row::Extra(e) if b == CONFIRM || matches!(b, Button::Left | Button::Right) => {
                let mut x = cozy_extras::get();
                x.set(e, !x.get(e));
                cozy_extras::set(x);
                self.cozy_extras_changed(e);
                self.save_settings();
            }
            Row::Animations if b == CONFIRM => return Some(SettingsPage::List(settings_row("Animations"))),
            Row::Sounds if b == CONFIRM => return Some(SettingsPage::List(settings_row("Sounds"))),
            _ => {}
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
        let value = |app: &App, r: Row| match r {
            Row::Extra(e) => pocket_ui::settings_values::on_off(x.get(e)).to_string(),
            Row::Animations => pocket_ui::settings_values::page(app.animations_in_use().name()),
            Row::Sounds => pocket_ui::settings_values::page(&app.sound.label()),
        };
        // The selected row's line, its looks and what keeps it from showing, under the list over the hints.
        let desc = match list[sel] {
            Row::Extra(e) => {
                // What keeps it from showing comes before the looks it is seen in: the footer has four lines, and
                // German needs three for some blurbs.
                let mut d = e.blurb().to_string();
                match (e.needs().filter(|n| x.get(e) && !self.cozy_need_met(*n)), e.only()) {
                    (Some(n), _) => d = format!("{d}\n{}", n.note()),
                    (None, Some(only)) => d = format!("{d}\n{only}"),
                    (None, None) => {}
                }
                d
            }
            Row::Animations => "Cozy: watch your game slide into the console. Quick: straight into the game.".to_string(),
            Row::Sounds => "Soft clicks in menus, not in games. Case sounds need them on.".to_string(),
        };
        let hints: &[(&str, &str)] = match list[sel] {
            Row::Extra(_) => &[("B", "Back"), ("A", "Switch")],
            _ => &[("B", "Back"), ("A", "Open")],
        };
        let foot = pocket_ui::kit::footer(c, &mut self.text, &mut self.hits, hints, &desc, 4, pal().text);
        let visible = (((foot - 12 - top) / row).max(1) as usize).min(list.len());
        let first = window_start(sel, visible, list.len());
        for (i, r) in list.iter().enumerate().skip(first).take(visible) {
            let y = top + (i - first) as i32 * row;
            self.hits.row(Area::new(pad, y, right - pad, row), i);
            let v = value(self, *r);
            let shown = if i == sel && matches!(r, Row::Extra(_)) { format!("‹ {v} ›") } else { v };
            // The name gives way to the value; both stay inside the row (German, Large text, 640x480).
            let vw = self.text.width(Face::Body, font, &shown);
            let name = match r {
                Row::Extra(e) => e.label(),
                Row::Animations => "Animations",
                Row::Sounds => "Sounds",
            };
            let name = self.text.fit(Face::Body, font, name, right - pad - 2 * NU_BTN_PAD - vw - 24);
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

    #[test]
    fn the_page_has_every_built_extra_then_the_two_links() {
        let r = rows();
        assert_eq!(r.len(), Extra::shown().len() + 2);
        assert_eq!(r[r.len() - 2..], [Row::Animations, Row::Sounds]);
        assert!(Extra::shown().iter().all(|e| r.contains(&Row::Extra(*e))));
    }
}
