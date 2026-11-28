//! The one rule for messages on the shelf (docs/DESIGN.md, "Messages"): a short confirmation is a toast
//! (`App::toast`, `toast_len`: by its length, 2 s at the least), and what went wrong, or needs a decision, is a notice
//! (`App::warn`): a card at the top that stays until A, B or a tap closes it, so a player who looked away still reads
//! the reason. The notice is drawn over every page of the shelf, not only over Home.

use pocket_ui::gfx::Canvas;
use pocket_ui::notice::Notice;
use pocket_ui::pad::PadEvent;
use pocket_ui::touch::{Phase, TouchEvent};

use super::{App, CANCEL, CONFIRM};

impl App {
    /// Says what went wrong: a notice that stays until it is closed. One that is up is not pushed away unread: the
    /// new words wait and come up when it is closed.
    pub(crate) fn warn(&mut self, text: impl Into<String>) {
        let text = text.into();
        match &self.notice {
            Some(up) if up.text() == text => {}
            Some(_) if !self.notice_later.contains(&text) => self.notice_later.push(text),
            Some(_) => {}
            None => self.notice = Some(Notice::new(text)),
        }
        self.request_redraw();
    }

    /// A page other than Home was painted this frame: the notice goes over it too (Home paints its own, under the
    /// toast).
    pub(crate) fn paint_notice_over_page(&mut self, c: &mut Canvas) {
        if self.notice_painted || self.running.is_some() || self.sleep.overlay().is_some() {
            return;
        }
        self.paint_shelf_notice(c);
    }

    /// The file-access sheet takes the whole screen. A pending notice waits for the shelf.
    pub(crate) fn paint_shelf_notice(&mut self, c: &mut Canvas) -> i32 {
        let bottom = self.notice.as_ref().filter(|_| !self.android_files_ask).map_or(0, |n| n.paint(c, &mut self.text));
        self.notice_painted = bottom > 0;
        bottom
    }

    fn notice_can_close(&self) -> bool {
        !self.android_files_ask && self.notice_painted && self.notice.is_some() && self.running.is_none()
    }

    /// A or B closes a notice that is on the screen, and nothing else happens with that press. Any other button
    /// works as usual and leaves the notice up.
    pub(crate) fn close_notice_pad(&mut self, pe: &PadEvent) -> bool {
        let PadEvent::Button { button, pressed: true, .. } = pe else { return false };
        if !self.notice_can_close() || !(*button == CONFIRM || *button == CANCEL) {
            return false;
        }
        // Android's installer at work (`app_update_hint`): B stops it, A leaves the card up (it is what is going on).
        if self.notice.as_ref().is_some_and(Notice::cancels_update) {
            if *button == CANCEL {
                self.cancel_android_install();
            }
            return true;
        }
        // The "newer pocket-ui is ready" card: A goes to the Updates page, B only closes (`app_update_hint`).
        let to_updates = *button == CONFIRM && self.notice.as_ref().is_some_and(Notice::opens_updates);
        let from_card = to_updates && self.notice.as_ref().is_some_and(Notice::installs_from_card);
        self.drop_notice();
        if from_card {
            self.install_card_update_from_notice();
        } else if to_updates {
            self.open_updates_from_notice();
        }
        true
    }

    /// A tap closes it, and the finger does nothing else.
    pub(crate) fn close_notice_touch(&mut self, t: &TouchEvent) -> bool {
        if t.phase != Phase::Down || !self.notice_can_close() {
            return false;
        }
        self.touch_rec.reset();
        self.drop_notice();
        true
    }

    fn drop_notice(&mut self) {
        // The "is ready" card closed by the player is the one that counts as said (`app_update_hint`).
        if self.notice.as_ref().is_some_and(|n| n.opens_updates() && !n.installs_from_card()) {
            self.update_hint_closed();
        }
        self.notice = (!self.notice_later.is_empty()).then(|| Notice::new(self.notice_later.remove(0)));
        self.notice_painted = false;
        self.request_redraw();
    }
}
