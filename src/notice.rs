//! A message on the shelf after a game could not start. pocket-core leaves its reason in a file
//! (a missing BIOS, a ROM set the core rejects); pocket-ui shows it for a few seconds when the game
//! exits, instead of just dropping back to the shelf without a word.

use std::time::{Duration, Instant};

use crate::gfx::Canvas;
use crate::palette::*;
use crate::text::{Face, Text};

pub const ERROR_FILE: &str = "/tmp/pocket-core.error";
const SHOWN: Duration = Duration::from_secs(8);

pub struct Notice {
    text: String,
    since: Instant,
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
        Notice { text: text.into(), since: Instant::now() }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn active(&self) -> bool {
        self.since.elapsed() < SHOWN
    }

    /// A card at the top of the screen, as wide as the text needs up to the screen width less a margin.
    pub fn paint(&self, c: &mut Canvas, text: &mut Text) {
        if !self.active() {
            return;
        }
        // For the snapshot checks of Settings → Text size: a card over the top of the page while it is up.
        crate::text::boxes::covered();
        let max = c.w as i32 - 2 * MARGIN;
        let lines = text.wrap(Face::Body, 30, &self.text, max - 64);
        let w = lines.iter().map(|l| text.width(Face::Body, 30, l)).max().unwrap_or(0) + 64;
        // Taller lines with larger text (Settings → Text size).
        let (step, grow) = (crate::type_scale::line(30, 12), crate::type_scale::grow(30));
        let h = 52 + grow + lines.len() as i32 * step - 8;
        let (x, y) = (c.w as i32 / 2 - w / 2, 36);
        c.round_rect(x, y, w, h, 26, KEYCAP);
        c.rect(x + 16, y + 20, 6, h - 40, ALERT);
        for (i, l) in lines.iter().enumerate() {
            text.draw(c, Face::Body, 30, x + 40, y + 46 + grow * 7 / 10 + i as i32 * step, CHALK, l);
        }
    }
}
