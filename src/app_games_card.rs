//! What the shelf does when a games card comes or goes.

use std::time::Instant;

use pocket_ui::games_card::Change;

use super::{App, Notice};

/// Over the shelf, once, after a games card came out; over a game it waits until the game ended.
pub(crate) const CARD_OUT: &str = "Your games card came out. Put it back to play: your games and saves are on it. A game that was running couldn't save after that moment.";
const CARD_IN: &str = "Games card found. While it's in, only its games show; your other games come back when you take it out.";


impl App {








    pub(crate) fn games_card_changed(&mut self, change: Change) {
        println!("games card: {change:?}");
        match change {
            // Not over a game: its files stay where they are, the folders are read when it has ended.
            Change::Came | Change::Left if self.running.is_some() => self.library_ui_rescan_after_game(),
            Change::Came => {
                self.card_out_note = false;
                self.toast = Some((CARD_IN.to_string(), Instant::now()));
                self.library_rescan("a games card came");
            }
            Change::Left => {
                self.library_rescan("back on the first card");
            }
            Change::Gone => self.card_out_note = true,
        }
        self.games_card_say();
    }

    /// The word about a card that came out, as soon as nothing is in its way (after each event).
    pub(crate) fn games_card_say(&mut self) {
        if self.card_out_note && self.running.is_none() && self.notice.is_none() {
            self.card_out_note = false;
            self.notice = Some(Notice::new(CARD_OUT.to_string()));
        }
    }
}
