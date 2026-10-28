//! Part of the pocket-ui binary: the App's side of `pocket_ui::clock_check`.
//!
//! A clock that is clearly wrong (a dead battery, no time server yet) is said once on the shelf, `FIRST_LOOK` after the
//! start: network time needs a moment after Wi-Fi comes up, and a clock that a server sets in the meantime is never
//! mentioned. Looking costs a read of the clock, no thread and no program. Where a notice would not be seen (a game, a
//! page, another notice) it waits and looks again; during a game there is no timer at all, the first turn of the loop
//! after it asks again. Settings → System → Time says it whenever it is open (`quick_paint`).

use std::time::{Duration, Instant};

use pocket_ui::clock_check;
use pocket_ui::notice::Notice;

use super::App;

/// How long after the start the clock is looked at: Wi-Fi and the time server have had their go.
pub(crate) const FIRST_LOOK: Duration = Duration::from_secs(45);
/// How long to wait before looking again when the shelf was covered.
const AGAIN: Duration = Duration::from_secs(30);

impl App {
    /// Called every turn of the loop; the time the next look is due, if a timer is needed for it.
    pub(crate) fn clock_tick(&mut self, now: Instant) -> Option<Instant> {
        let due = self.clock_look?;
        if self.running.is_some() {
            return None;
        }
        if now < due {
            return Some(due);
        }
        let unix = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64);
        if !clock_check::looks_wrong(unix, clock_check::built_unix()) {
            self.clock_look = None;
            return None;
        }
        if self.shelf_notice_free() {
            self.notice = Some(Notice::new(clock_check::SHELF_NOTICE));
            self.clock_look = None;
            self.request_redraw();
            return None;
        }
        // Covered by a page: again soon.
        self.clock_look = Some(now + AGAIN);
        Some(now + AGAIN)
    }
}
