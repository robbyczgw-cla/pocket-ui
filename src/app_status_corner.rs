//! Part of the pocket-ui binary: the status corner's App side (`pocket_ui::status_corner` has the logic).
//! - Painting: `paint_top_line` puts a page's place at the left and the corner (time, Wi-Fi while connected,
//!   battery) at the right of the top line, the place cut short before it meets the corner; `status_corner` is the
//!   corner for the pages that paint their own top line (game card, console page, History). Either marks the frame
//!   as one with the corner (`corner_minute`). Settings, its pages and the pause menu's pages show the corner in a
//!   pill (`App::status_pill`), the quick settings in theirs (`quick_paint`); those frames count too.
//! - The minute: a frame with the corner asks for one wake-up just after the next minute begins
//!   (`status_corner_tick` from `about_to_wait`); nothing in between, and none while asleep, in a game or on a page
//!   without the corner.
//! - Wi-Fi and the battery come from the corner's thread (`status_corner::start`), which reads them every 3 s only
//!   while the corner is on screen and wakes the UI (`UserEvent::StatusCorner`) when they change: no file is read
//!   on this thread.
//! - `--snapshot`: `POCKETUI_SNAPSHOT_WIFI=1` paints the corner as if Wi-Fi were connected, `0` as if not.

use std::time::Instant;

use pocket_ui::gfx::Canvas;
use pocket_ui::palette::MARGIN;
use pocket_ui::status_corner::{self, Corner};
use pocket_ui::text::Face;

use crate::{App, UserEvent};

/// The top line's baseline on Home and the pages over it.
pub(crate) const TOP_LINE: i32 = 72;

impl App {
    /// The corner now (no file read: memory and the clock), and this frame counts as one with the corner.
    pub(crate) fn status_corner(&mut self) -> Corner {
        if self.corner_minute.is_none() {
            self.corner_minute = Some(Instant::now() + status_corner::next_minute_in());
        }
        Corner::now()
    }

    /// The top line: `place` at the left (cut short before the corner), the corner at the right in `colour`.
    /// Returns the corner's left edge, for a note to stand left of it.
    pub(crate) fn paint_top_line(&mut self, c: &mut Canvas, place: Option<&str>, colour: u32) -> i32 {
        let corner = self.status_corner();
        let left = corner.paint(c, &mut self.text, c.w - MARGIN, TOP_LINE, colour);
        if let Some(place) = place {
            let room = status_corner::room_left_of(left, c.w - MARGIN) - MARGIN;
            let place = self.text.fit(Face::Body, status_corner::PX, place, room);
            self.text.draw(c, Face::Body, status_corner::PX, MARGIN, TOP_LINE, colour, &place);
        }
        left
    }

    /// From `about_to_wait`: the wake-up for the next minute while the corner is on screen and the device awake
    /// outside a game; tells the corner's thread whether to read at all.
    pub(crate) fn status_corner_tick(&mut self, now: Instant) -> Option<Instant> {
        let shown = self.status_corner_shown();
        status_corner::set_shown(shown);
        let at = self.corner_minute.filter(|_| shown)?;
        if now >= at {
            // The frame this asks for paints the new minute and sets the next wake-up (`paint` clears this one).
            self.request_redraw();
            return None;
        }
        Some(at)
    }

    /// Whether the corner is on screen: a change its thread read (`UserEvent::StatusCorner`) is worth a frame. In a
    /// game only while the pause menu or the quick settings are up over it (their pill is the corner too).
    pub(crate) fn status_corner_shown(&self) -> bool {
        let over_game = self.menu.is_some() || self.sys.is_open();
        self.corner_minute.is_some() && (self.running.is_none() || over_game) && self.sleep.state() == pocket_ui::sleep::State::Awake
    }

    /// Starts the corner's thread (after `--snapshot` has had its chance: snapshots read once, on this thread).
    pub(crate) fn status_corner_start(&self) {
        let proxy = self.proxy.clone();
        status_corner::start(move || {
            let _ = proxy.send_event(UserEvent::StatusCorner);
        });
    }
}

/// `--snapshot`: POCKETUI_SNAPSHOT_WIFI=1 shows the Wi-Fi symbol, 0 leaves it out, whatever this computer has.
pub(crate) fn snapshot_knobs() {
    match std::env::var("POCKETUI_SNAPSHOT_WIFI").as_deref() {
        Ok("1") => status_corner::force_wifi(true),
        Ok("0") => status_corner::force_wifi(false),
        _ => {}
    }
}
