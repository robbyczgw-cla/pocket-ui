//! Part of the pocket-ui binary: Settings → System → Battery's "Reset battery reading", on devices with the RK817
//! power chip only (`pocket_ui::fuel_gauge::offered`: R36S, R36XX and the other RK3326/RK3566 handhelds).
//!
//! X on the Battery page asks first, on a sheet over the page: what it is for (a level that is wrong, 0 % on a full
//! battery), that the Pocket switches off, and what to do after it. A resets and switches off through the power
//! menu's own Power off (a game under Settings is saved first); B keeps everything. The next start says once what to
//! do now: charge it full once (`fuel_gauge::take_note`). A chip that doesn't take the reset is said on the sheet,
//! and the Pocket stays on.

use pocket_ui::gfx::Canvas;
use pocket_ui::pad::Button;
use pocket_ui::palette::*;
use pocket_ui::text::Face;
use pocket_ui::type_scale;

use super::{App, SettingsPage, CANCEL, CONFIRM, NU_PAD};

/// The hint on the Battery page.
pub(crate) const HINT: &str = "Reset reading";
pub(crate) const TITLE: &str = "Reset the battery reading?";
/// What it is for, and what happens (two paragraphs, each a line of the tables).
pub(crate) const WHAT: &str = "Only for a battery level that is wrong, like 0% while the battery is full.";
pub(crate) const THEN: &str = "Your Pocket forgets the saved level and switches off. Switch it on again and it measures the level anew. Then charge it until it is full once.";
pub(crate) const FAILED_TITLE: &str = "The battery reading was not reset";
pub(crate) const FAILED: &str = "The battery chip did not take it. Everything stays as it was, and your Pocket stays on.";

/// The question's state on the Battery page.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) enum Ask {
    #[default]
    Closed,
    Asking,
    Failed,
}

impl Ask {
    pub(crate) fn is_open(self) -> bool {
        self != Ask::Closed
    }
}

impl App {
    /// The key is this page's: X where the chip is, and every key while the sheet is up.
    pub(crate) fn battery_reset_takes(&self, b: Button) -> bool {
        self.battery_reset.is_open() || (b == Button::North && pocket_ui::fuel_gauge::offered())
    }

    /// X opens the question; on it A resets and switches off, B (or A on the failure) closes it.
    pub(crate) fn battery_reset_input(&mut self, b: Button, range: usize) -> SettingsPage {
        match (self.battery_reset, b) {
            (Ask::Closed, Button::North) => self.battery_reset = Ask::Asking,
            (Ask::Closed, _) => {}
            (Ask::Asking, b) if b == CONFIRM => {
                // A snapshot or the test driver can run on a real R36S beside its pocket-ui: never the chip, never off.
                let result = if super::snapshot_run() || super::driven() {
                    Err(std::io::Error::other("a snapshot or test run never writes the chip"))
                } else {
                    pocket_ui::fuel_gauge::reset_device(&self.data_dir)
                };
                match result {
                    Ok(_) => {
                        self.battery_reset = Ask::Closed;
                        // The power menu's Power off: a game under Settings is saved and quit first.
                        self.sleep.power_off();
                    }
                    Err(e) => {
                        eprintln!("battery reading: reset failed: {e}");
                        self.battery_reset = Ask::Failed;
                    }
                }
            }
            (_, b) if b == CANCEL || b == CONFIRM => self.battery_reset = Ask::Closed,
            _ => {}
        }
        SettingsPage::Battery(range)
    }

    /// The sheet over the Battery page, and its hints.
    pub(crate) fn paint_battery_reset(&mut self, c: &mut Canvas) {
        let failed = self.battery_reset == Ask::Failed;
        c.shade_rect(0, 0, c.w, c.h, pal().void, 225);
        pocket_ui::text::boxes::covered();
        let w = type_scale::wide(860, 30).min(c.w - 2 * NU_PAD - 96);
        let small = 30u32;
        let step = type_scale::line(small, 10);
        let paragraphs: Vec<Vec<String>> = if failed { vec![FAILED] } else { vec![WHAT, THEN] }
            .into_iter()
            .map(|p| self.text.wrap_prose(Face::Body, small, p, w))
            .collect();
        let title_h = type_scale::px(50) as i32;
        let body_h: i32 = paragraphs.iter().map(|p| p.len() as i32 * step).sum::<i32>() + (paragraphs.len() as i32 - 1) * 16;
        let h = title_h + 36 + body_h;
        let top = pocket_ui::kit::header_bottom(c.h);
        let bottom = pocket_ui::kit::hint_bar_top(c.h) - 24;
        let (x, y) = ((c.w - w) / 2, (top + (bottom - top - h) / 2).max(top + 48));
        pocket_ui::kit::sheet(c, x - 48, y - 48, w + 96, h + 96);
        let head = self.text.fit(Face::Title, 50, if failed { FAILED_TITLE } else { TITLE }, w);
        self.text.draw(c, Face::Title, 50, x, y + title_h, if failed { pal().alert } else { pal().chalk }, &head);
        let mut by = y + title_h + 36;
        for (i, lines) in paragraphs.iter().enumerate() {
            for l in lines {
                by += step;
                self.text.draw(c, Face::Body, small, x, by - 10, if i == 0 && !failed { pal().quiet } else { pal().text }, l);
            }
            by += 16;
        }
        if failed {
            self.nu_hints(c, &[("B", "Back")]);
        } else {
            self.nu_hints(c, &[("A", "Reset and switch off"), ("B", "Keep")]);
        }
    }

    /// At start: the battery reading was reset before it, so what to do now, once.
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    pub(crate) fn say_battery_reset(&mut self) {
        let Some(text) = pocket_ui::fuel_gauge::take_note(&self.data_dir) else { return };
        println!("start: {text}");
        if self.running.is_some() && self.after_game_note.is_none() {
            self.after_game_note = Some(text.to_string());
        } else {
            self.warn(text);
        }
    }

    /// `--snapshot battery[:<range>]:reset` (the page with X, as on an R36S), `:reset-ask` (the question) and
    /// `:reset-failed` (a chip that did not take it); `:keys=<presses>` then presses keys through the page's own input
    /// (`x` X, `a` A, `b` B: a snapshot never writes the chip, so A shows the failure and nothing switches off).
    pub(crate) fn battery_reset_snapshot(&mut self, state: &str) {
        if !state.starts_with("battery") {
            return;
        }
        let parts: Vec<&str> = state.split(':').collect();
        if parts.iter().any(|p| p.starts_with("reset")) {
            pocket_ui::fuel_gauge::pretend_present();
        }
        if parts.contains(&"reset-ask") {
            self.battery_reset = Ask::Asking;
        } else if parts.contains(&"reset-failed") {
            self.battery_reset = Ask::Failed;
        }
        for key in parts.iter().filter_map(|p| p.strip_prefix("keys=")).flat_map(str::chars) {
            let b = match key {
                'x' => Button::North,
                'a' => CONFIRM,
                'b' => CANCEL,
                _ => continue,
            };
            if self.battery_reset_takes(b) {
                self.battery_reset_input(b, 0);
            }
        }
    }
}
