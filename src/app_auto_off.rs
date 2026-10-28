//! Part of the pocket-ui binary: the start after a switch-off after sleeping (`pocket_ui::auto_off`) says why the
//! Pocket was off, once and calmly: "Switched off after 3 h asleep to save the battery."
//!
//! - The resume note sent the start into the game (`on_start` rule 1, whatever On start says), and it is a pocket-core
//!   game: over the game, as pocket-core's `note` (a line at the bottom for a few seconds). pocket-core makes its
//!   command FIFO a moment after it starts, so a thread of its own tries until it can write (at most `TRY_FOR`).
//! - Another emulator: on the shelf when the game ends (pocket-ui's window is under the game until then).
//! - No game (switched off on the shelf, or the game's file is gone): on the shelf right away.

use std::time::{Duration, Instant};

use pocket_ui::notice::Notice;

use super::App;

/// How long the thread keeps trying to reach a starting pocket-core.
const TRY_FOR: Duration = Duration::from_secs(30);
const TRY_EVERY: Duration = Duration::from_millis(200);

impl App {
    /// At start, after the game of the resume note was started (or not).
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    pub(crate) fn say_switched_off(&mut self) {
        let Some(text) = self.sleep.take_switched_off() else { return };
        println!("start: {text}");
        match &self.running {
            Some(run) if run.hosted() => {
                let line = format!("note {text}");
                let _ = std::thread::Builder::new().name("switched-off-note".into()).spawn(move || {
                    let end = Instant::now() + TRY_FOR;
                    while !pocket_ui::control::pocket_core(&line) && Instant::now() < end {
                        std::thread::sleep(TRY_EVERY);
                    }
                });
            }
            Some(_) => self.after_game_note = Some(text),
            None if self.notice.is_none() => self.notice = Some(Notice::new(text)),
            // A notice of its own is up (a crash before): this one beside it, smaller.
            None => self.toast = Some((text, Instant::now())),
        }
    }

    /// A game ended: the switch-off's line, if it is still to be said and nothing else is.
    pub(crate) fn switched_off_after_game(&mut self) {
        if let Some(text) = self.after_game_note.take() {
            if self.notice.is_none() {
                self.notice = Some(Notice::new(text));
            }
        }
    }
}
