//! Animation deadlines include the time spent painting the previous frame.
//!
//! Waiting a whole refresh after painting made a 14 ms frame take 31 ms. Late
//! paints can ask for the next frame immediately; fast paints still wait for
//! their refresh budget. Static pages do not ask this clock for redraws.

use std::time::{Duration, Instant};

pub fn after_paint(started: Instant, finished: Instant, refresh: Duration) -> Instant {
    (started + refresh).max(finished)
}

/// Which frames the event loop asks for: one per refresh while something moves, and one more after the last frame
/// that was painted while it moved, so the screen always ends on the animation's end. Asking only while it moved left
/// a slow screen on its last in-between frame: on the RK3566 the box turn's (280 ms) last frame was painted half way,
/// and by the time it was out the turn was over, so nothing painted the turned box (15.11.2026, dArkOS test-6: the
/// box "stuck" half turned until the next button). A fast screen ended a few degrees short, which no one saw.
#[derive(Default)]
pub struct Settle {
    shown_moving: bool,
}

impl Settle {
    /// A frame went to the screen; `moving`: something moved when it was painted (the loop's `animating`).
    pub fn painted(&mut self, moving: bool) {
        self.shown_moving = moving;
    }

    /// Whether the loop asks for a frame now that `moving` says whether something still moves.
    pub fn wants(&self, moving: bool) -> bool {
        moving || self.shown_moving
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const REFRESH: Duration = Duration::from_micros(16_667);

    fn frames_in_one_second(paint: Duration) -> usize {
        let start = Instant::now();
        let mut at = start;
        let mut frames = 0;
        while at.duration_since(start) < Duration::from_secs(1) {
            at = after_paint(at, at + paint, REFRESH);
            frames += 1;
        }
        frames
    }

    #[test]
    fn a_fourteen_ms_paint_keeps_sixty_animation_frames_in_a_second() {
        assert_eq!(frames_in_one_second(Duration::from_millis(14)), 60);
    }

    #[test]
    fn a_slow_paint_has_no_extra_refresh_wait() {
        assert_eq!(frames_in_one_second(Duration::from_millis(25)), 40);
        let start = Instant::now();
        assert_eq!(after_paint(start, start + Duration::from_millis(25), REFRESH), start + Duration::from_millis(25));
    }

    #[test]
    fn after_the_last_frame_that_moved_comes_exactly_one_more() {
        let mut s = Settle::default();
        assert!(!s.wants(false), "at rest: no frames");
        assert!(s.wants(true));
        s.painted(true);
        assert!(s.wants(false), "the animation ended while its last frame was painted: one more frame");
        s.painted(false);
        assert!(!s.wants(false), "and then none");
    }

    #[test]
    fn a_fast_paint_still_caps_animation_work_at_one_frame_per_refresh() {
        assert_eq!(frames_in_one_second(Duration::ZERO), 60);
    }
}
