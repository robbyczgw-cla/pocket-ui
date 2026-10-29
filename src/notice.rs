//! A message on the shelf after a game could not start. pocket-core leaves its reason in a file
//! (a missing BIOS, a ROM set the core rejects); pocket-ui shows it for a few seconds when the game
//! exits, instead of just dropping back to the shelf without a word.

use std::time::{Duration, Instant};

use crate::gfx::Canvas;
use crate::palette::*;
use crate::text::{Face, Text};

pub const ERROR_FILE: &str = "/tmp/pocket-core.error";
const SHOWN: Duration = Duration::from_secs(8);
/// The room the status corner (time, Wi-Fi, battery) takes at the top line's right end, and the place its left end.
const CORNER: i32 = 300;

/// Under a game's error: where the report is (SELECT + START held opens the question that has "Report a problem").
pub const REPORT_HINT: &str = "To report it, hold SELECT + START and choose Report a problem.";

pub struct Notice {
    text: String,
    since: Instant,
    /// A game that didn't start: the card says how to report it.
    report: bool,
}

impl Notice {
    /// Takes the reason pocket-core left behind, if any (and removes it, so it shows once).
    pub fn take() -> Option<Notice> {
        let text = std::fs::read_to_string(ERROR_FILE).ok()?;
        let _ = std::fs::remove_file(ERROR_FILE);
        let text = text.trim();
        (!text.is_empty()).then(|| Notice {
            text: text.to_string(),
            since: Instant::now(),
            report: true,
        })
    }

    /// After a pocket-core game: its reason for not starting, else that its last save didn't reach the card
    /// (`save_trouble`, the note pocket-core leaves while saves fail). Each shows once.
    pub fn take_after_game() -> Option<Notice> {
        let trouble = crate::save_trouble::take();
        Notice::take().or_else(|| trouble.map(|t| Notice::new(t.after_quit())))
    }

    /// A message of pocket-ui's own.
    pub fn new(text: impl Into<String>) -> Notice {
        Notice { text: text.into(), since: Instant::now(), report: false }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn active(&self) -> bool {
        self.since.elapsed() < SHOWN
    }

    /// A card at the top of the screen, the OSD's surface and corners (`osd`), as wide as the text needs. It stays
    /// clear of the top line's ends (the place, the status corner) or, when it needs more room, spans the screen and
    /// covers them whole: never half a clock peeking out beside it.
    pub fn paint(&self, c: &mut Canvas, text: &mut Text) {
        if !self.active() {
            return;
        }
        // For the snapshot checks of Settings → Text size: a card over the top of the page while it is up.
        crate::text::boxes::covered();
        let clear = c.w - 2 * (MARGIN + CORNER);
        let report = self.report;
        let mut wrapped = |max: i32| {
            let mut lines = text.wrap(Face::Body, 30, &self.text, max - 64);
            let own = lines.len();
            if report {
                lines.extend(text.wrap(Face::Body, 30, REPORT_HINT, max - 64));
            }
            (lines, own)
        };
        let (mut lines, mut own) = wrapped(clear);
        if lines.len() > 1 {
            (lines, own) = wrapped(c.w - 2 * crate::kit::PAD);
        }
        let mut w = lines.iter().map(|l| text.width(Face::Body, 30, l)).max().unwrap_or(0) + 64;
        if w > clear {
            w = c.w - 2 * crate::kit::PAD;
        }
        // Taller lines with larger text (Settings → Text size).
        let (step, grow) = (crate::type_scale::line(30, 12), crate::type_scale::grow(30));
        let h = 52 + grow + lines.len() as i32 * step - 8;
        let (x, y) = (c.w / 2 - w / 2, crate::osd::TOP);
        c.round_rect(x, y, w, h, crate::osd::H / 2, SPINE);
        c.rect(x + 16, y + 20, 6, h - 40, ALERT);
        for (i, l) in lines.iter().enumerate() {
            let colour = if i < own { CHALK } else { DUST };
            text.draw(c, Face::Body, 30, x + 40, y + 46 + grow * 7 / 10 + i as i32 * step, colour, l);
        }
    }
}
