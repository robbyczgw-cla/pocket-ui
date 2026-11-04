//! Part of the pocket-ui binary: the row Settings → Connections → USB file transfer (`pocket_ui::usb_transfer`) and what the
//! shelf does when a games card comes or goes (`pocket_ui::games_card`).

use std::time::Instant;

use pocket_ui::games_card::Change;
use pocket_ui::usb_transfer::{self, Status};

use super::{App, Notice, UserEvent};

/// Over the shelf, once, after a games card came out; over a game it waits until the game ended.
pub(crate) const CARD_OUT: &str = "Your games card came out. Put it back to play: your games and saves are on it. A game that was running couldn't save after that moment.";
const CARD_IN: &str = "Games card found. While it's in, only its games show; your other games come back when you take it out.";

const USB_ON: &str = "USB file transfer is on. Connect a computer: your games are under Roms.";
const USB_FAILED: &str = "USB file transfer didn't start. Take out what's in the USB port and try again.";

impl App {
    /// A on the row: on or off, on a thread (about a second).
    pub(crate) fn usb_transfer_switch(&mut self) {
        if self.usb_busy {
            return;
        }
        self.usb_busy = true;
        let on = pocket_ui::settings_values::get().usb.unwrap_or(Status::Off) == Status::Off;
        let proxy = self.proxy.clone();
        std::thread::spawn(move || {
            let status = usb_transfer::set(on);
            let _ = proxy.send_event(UserEvent::UsbTransfer { wanted_on: on, status });
        });
    }

    /// The row's value now.
    pub(crate) fn usb_transfer_value(&self) -> String {
        if self.usb_busy {
            return "Switching…".to_string();
        }
        pocket_ui::settings_values::get().usb.map(|s| s.label().to_string()).unwrap_or_default()
    }

    pub(crate) fn usb_transfer_switched(&mut self, wanted_on: bool, status: Status) {
        self.usb_busy = false;
        pocket_ui::settings_values::set_usb(status);
        match (wanted_on, status) {
            (true, Status::Off) => self.warn(USB_FAILED),
            (true, _) => self.toast = Some((USB_ON.to_string(), Instant::now())),
            // Whatever the computer copied is on the shelf now.
            (false, _) => {
                self.library_rescan("after USB file transfer");
            }
        }
    }

    /// Leaving Settings with the transfer on: the computer may have copied games while the list was open.
    pub(crate) fn usb_transfer_left_settings(&mut self) {
        if pocket_ui::settings_values::get().usb.is_some_and(|s| s != Status::Off) {
            self.library_rescan("leaving Settings with USB file transfer on");
        }
    }

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
