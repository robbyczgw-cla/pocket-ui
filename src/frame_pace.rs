//! Animation deadlines include the time spent painting the previous frame.
//!
//! Waiting a whole refresh after painting made a 14 ms frame take 31 ms. Late
//! paints can ask for the next frame immediately; fast paints still wait for
//! their refresh budget. Static pages do not ask this clock for redraws.

use std::time::{Duration, Instant};

pub fn after_paint(started: Instant, finished: Instant, refresh: Duration) -> Instant {
    (started + refresh).max(finished)
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
    fn a_fast_paint_still_caps_animation_work_at_one_frame_per_refresh() {
        assert_eq!(frames_in_one_second(Duration::ZERO), 60);
    }
}
