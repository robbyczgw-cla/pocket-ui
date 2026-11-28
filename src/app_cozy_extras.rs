//! Part of the pocket-ui binary: Settings → Look → Cozy extras (`pocket_ui::cozy_extras` has the switches). A list of
//! switches in the Settings style, one per extra you see (`Extra::PAGE_ROWS`), each On or Off, followed by the GBA
//! console-model choice and the names of the consoles sold under regional names (`region_pairs`: Mega Drive / Genesis,
//! SNES / Super Famicom, NES / Famicom, PC Engine / TurboGrafx-16); the selected one's line
//! under the list, with the looks it is seen in. Playful cases and old-TV switch-on came here from Sound & feel with the
//! tree of 2026-11-13; case sounds is a row of Screen & sound.
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

use super::{settings_row, window_start, App, SettingsPage, CANCEL, CONFIRM, NU_BTN_PAD, NU_PAD};

/// The name in the Settings list, to come back to.
pub(crate) const SETTINGS_NAME: &str = "Cozy extras";
pub(crate) const GBA_MODEL_NAME: &str = "GBA console";

fn rows() -> Vec<Extra> {
    Extra::shown()
}

/// The consoles sold under regional names, after the GBA console (`region_pairs`): Mega Drive / Genesis and the others.
fn name_rows() -> Vec<&'static pocket_ui::region_pairs::Pair> {
    pocket_ui::region_pairs::rows().collect()
}

/// The page's rows: the extras, the GBA console, the console names.
fn page_len() -> usize {
    rows().len() + 1 + name_rows().len()
}

impl App {
    pub(crate) fn open_cozy_extras(&mut self) -> Option<SettingsPage> {
        Some(SettingsPage::CozyExtras(0))
    }

    /// The first start's "How cozy should it be?" (`cozy_level`): Game start animation (the Lived-in shelf off) and the cozy
    /// extras as the level sets them, used at once and kept; each stays switchable on its own in Settings › Look.
    pub(crate) fn adopt_cozy(&mut self, level: pocket_ui::cozy_level::CozyLevel) {
        let c = level.apply(cozy_extras::get());
        self.animations = Some(c.animations);
        pocket_ui::motion::set_cozy(self.cozy());
        if self.lived_in != c.lived_in {
            self.lived_in = c.lived_in;
            self.forget_shelf_row();
        }
        cozy_extras::set(c.extras);
        for e in Extra::ALL {
            self.cozy_extras_changed(e);
        }
        self.save_settings();
        println!("first start: {} ({:?}, lived-in {})", level.name(), c.animations, c.lived_in);
    }

    /// Whether what extra `e` needs elsewhere is on.
    pub(crate) fn cozy_need_met(&self, n: Needs) -> bool {
        match n {
            Needs::Sounds => self.sound.on,
            Needs::CozyFull => self.animations_in_use() == Animations::Cozy && pocket_ui::motion::level() == pocket_ui::motion::Motion::Full,
            Needs::MotionFull => pocket_ui::motion::level() == pocket_ui::motion::Motion::Full,
        }
    }

    /// Extra `e` the other way, used and saved at once: from the page or Screen & sound's row.
    pub(crate) fn switch_extra(&mut self, e: Extra) {
        let mut x = cozy_extras::get();
        x.set(e, !x.get(e));
        cozy_extras::set(x);
        self.cozy_extras_changed(e);
        self.save_settings();
    }

    /// The line under extra `e` (`line` its own): what keeps it from showing, while another switch does and it is on,
    /// else the row's own line (Playful cases: that it follows Animations until switched).
    pub(crate) fn extra_row_line(&self, e: Extra, line: &'static str) -> &'static str {
        match e.needs().filter(|n| cozy_extras::on(e) && !self.cozy_need_met(*n)) {
            Some(n) => n.note(),
            None => cozy_extras::row_line(e, &cozy_extras::get(), line),
        }
    }

    /// A key on the page: A, Left or Right switch the selected extra, used and saved at once.
    pub(crate) fn cozy_extras_input(&mut self, sel: usize, b: Button) -> Option<SettingsPage> {
        let list = rows();
        let len = page_len();
        let sel = sel.min(len - 1);
        match b {
            Button::Up => return Some(SettingsPage::CozyExtras((sel + len - 1) % len)),
            Button::Down => return Some(SettingsPage::CozyExtras((sel + 1) % len)),
            b if b == CANCEL => return Some(SettingsPage::List(settings_row(SETTINGS_NAME))),
            _ => {}
        }
        if b == CONFIRM || matches!(b, Button::Left | Button::Right) {
            if let Some(e) = list.get(sel) {
                self.switch_extra(*e);
            } else if let Some(pair) = sel.checked_sub(list.len() + 1).and_then(|i| name_rows().get(i).copied()) {
                pocket_ui::region_pairs::step(pair, if b == Button::Left { -1 } else { 1 });
                self.console_names_changed();
                self.save_settings();
            } else {
                pocket_ui::system_art::set_gba_model(pocket_ui::system_art::gba_model().next());
                self.console_look_changed();
                self.save_settings();
            }
        }
        Some(SettingsPage::CozyExtras(sel))
    }

    /// A console's name changed (`region_pairs`): the shelves show it at once, the list is read again for its order, and the
    /// launch pictures follow (the SNES's US console).
    pub(crate) fn console_names_changed(&mut self) {
        pocket_ui::library::name_region_pairs(&mut self.systems);
        self.console_look_changed();
        self.library_rescan("console names");
    }

    /// What was made with the old switch and is kept is made again: the cases' pictures (stickers), the box's disc.
    fn cozy_extras_changed(&mut self, e: Extra) {
        // Progress from saves: the box's back is made again (without or with its line); the shelf's meta lines, the card and
        // "Where was I?" ask `App::case_progress` each time they are painted.
        if matches!(e, Extra::Stickers | Extra::PrintedDiscs | Extra::ShrinkWrap | Extra::ProgressFromSaves) {
            self.cases_look_changed();
        }
        // The TV playing the game switched off: a run on the glass ends now (on, it starts after the next rest).
        if e == Extra::TvPlaysGame && !cozy_extras::on(e) {
            self.cabinet.tv.showcase_stop();
        }
    }

    /// Where the page's switches stand: a change is what the menu sounds and haptics answer.
    pub(crate) fn cozy_extras_mark(&self) -> (cozy_extras::Extras, pocket_ui::system_art::GbaModel, std::collections::BTreeMap<String, String>) {
        (cozy_extras::get(), pocket_ui::system_art::gba_model(), pocket_ui::region_pairs::chosen())
    }

    pub(crate) fn paint_cozy_extras(&mut self, c: &mut Canvas, sel: usize) {
        c.clear(pal().void);
        let (pad, row, font) = (NU_PAD, pocket_ui::type_scale::row(66, 36), 36u32);
        self.sheet_header(c, SETTINGS_NAME);
        let (top, right) = (pocket_ui::kit::header_bottom(c.h) + 12, c.w - pad);
        let list = rows();
        let names = name_rows();
        let len = page_len();
        let sel = sel.min(len - 1);
        let x = cozy_extras::get();
        let pair_at = |i: usize| i.checked_sub(list.len() + 1).and_then(|i| names.get(i).copied());
        // The selected row's line and the looks it is seen in, under the list over the hints.
        let desc = match list.get(sel) {
            None if pair_at(sel).is_some() => pair_at(sel).map_or("", |p| p.blurb).to_string(),
            None => "Choose the GBA console: Game Boy Advance or Game Boy Advance SP. Changes its picture everywhere and its cartridge slot at launch.".to_string(),
            // Playful cases and old-TV switch-on say what keeps them from showing (Animations, Motion), as their rows did.
            Some(e) => match e.only() {
                Some(only) => format!("{}\n{only}", self.extra_row_line(*e, e.blurb())),
                None => self.extra_row_line(*e, e.blurb()).to_string(),
            },
        };
        let foot = pocket_ui::kit::footer(c, &mut self.text, &mut self.hits, &[("B", "Back"), ("A", "Change")], &desc, 4, pal().text);
        let visible = (((foot - 12 - top) / row).max(1) as usize).min(len);
        let first = window_start(sel, visible, len);
        for i in first..(first + visible).min(len) {
            let y = top + (i - first) as i32 * row;
            self.hits.row(Area::new(pad, y, right - pad, row), i);
            let (label, v) = match (list.get(i), pair_at(i)) {
                (Some(e), _) => (e.label(), pocket_ui::settings_values::on_off(x.get(*e))),
                (None, Some(p)) => (p.row, pocket_ui::region_pairs::name_of(p).label),
                (None, None) => (GBA_MODEL_NAME, pocket_ui::system_art::gba_model().label()),
            };
            let shown = if i == sel { format!("‹ {v} ›") } else { v.to_string() };
            // The name gives way to the value; both stay inside the row (German, Large text, 640x480).
            let vw = self.text.width(Face::Body, font, &shown);
            let name = self.text.fit(Face::Body, font, label, right - pad - 2 * NU_BTN_PAD - vw - 24);
            let base = pocket_ui::kit::row_label(c, &mut self.text, (pad, y, right - pad, row), font, &name, i == sel);
            pocket_ui::kit::row_value(c, &mut self.text, font, right - NU_BTN_PAD, base, &shown, i == sel);
        }
        pocket_ui::kit::scrollbar(c, top, visible as i32 * row, first, visible, len);
        self.hits.list(sel, len, 0.0, row as f32);
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
        let presses = parts.next().unwrap_or_default();
        // Optional fourth field: paint a hardware view before changing the model and return
        // to it afterwards. This exercises the player's warm case/icon caches in one App.
        let warm = parts.next().filter(|look| ["shelf", "cabinet", "split"].contains(look));
        if let Some(look) = warm {
            self.shelf_snapshot(&format!("gba:{look}"));
            let (w,h) = super::snapshot_dims();
            let mut buf = vec![0; (w*h) as usize];
            self.paint(&mut Canvas::new(&mut buf,w,h));
        }
        let mut page = SettingsPage::CozyExtras(row);
        for key in presses.chars() {
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
        if let Some(look) = warm {
            self.shelf_snapshot(&format!("gba:{look}"));
            self.settings_page = None;
        } else {
            self.settings_page = Some(page);
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The switches you see or that move, playful cases and old-TV switch-on among them (the tree of 2026-11-13), no
    /// links to rows elsewhere; case sounds stays beside Sounds.
    #[test]
    fn the_page_has_the_extras_you_see_and_nothing_else() {
        let r = rows();
        // The page's rows in their order, less what is taken off for now (the Window, 19.11.2026: `Extra::built`).
        assert_eq!(r, Extra::PAGE_ROWS.into_iter().filter(|e| *e != Extra::Window).collect::<Vec<_>>());
        assert!(r.contains(&Extra::Playful) && r.contains(&Extra::CrtOn) && !r.contains(&Extra::CaseSounds) && !r.contains(&Extra::Window));
        // Twelve switches since Sealed new games, Progress from saves and the TV playing the game (14.11.2026), fourteen
        // with the TV's switch-off and evening glow (19.11.2026; the Window and the power switch taken off again), sixteen
        // with Handheld switch-on and Power lights; the page scrolls.
        assert!(r.len() < 17);
        assert!(r.iter().all(|e| e.home() == pocket_ui::cozy_extras::Home::Page));
    }
}
