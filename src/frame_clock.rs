//! The clock the animations are painted by: `Instant::now()`, unless a snapshot froze it. A frame series
//! (`POCKETUI_SNAPSHOT_LAUNCH_SERIES`, `…_EJECT_SERIES`) and a single frame of an animation (`POCKETUI_SNAPSHOT_EJECT_MS`,
//! `…_LAUNCH_MS`, `…_FLIP_MS`) freeze it and step it by exactly one 60 Hz frame, so every frame shows the moment it
//! stands for however long painting and writing the last one took. Under load the real clock moved on between setting
//! an animation's start and painting it, and the frame-sequence tests (`tests/eject.rs`, `tests/box_turn.rs`) saw steps
//! that were not in the animation (11.11.: two flakes in one gate).
//!
//! Only animation moments read it; what measures real time (frame costs, benches, timeouts) keeps `Instant::now()`.

use std::cell::Cell;
use std::time::{Duration, Instant};

thread_local! {
    /// Per thread: the UI thread paints; a test's thread freezing it leaves the others alone.
    static AT: Cell<Option<Instant>> = const { Cell::new(None) };
}

/// Now, as the animations see it.
pub fn now() -> Instant {
    AT.with(|at| at.get()).unwrap_or_else(Instant::now)
}

/// From here on `now` is `at` (on this thread), until it is stepped or thawed.
pub fn freeze(at: Instant) {
    AT.with(|held| held.set(Some(at)));
}

/// Freezes it where it is now (the real time when it was not frozen) and returns that.
pub fn hold() -> Instant {
    let at = now();
    freeze(at);
    at
}

/// How long after a snapshot's setup its clock stands ([`hold_settled`]): longer than anything the setup starts (a
/// case turning out, the shelf's wiggle, the TV's crossfade), so those are over however long the setup took.
pub const SETTLE: Duration = Duration::from_secs(3);

/// Freezes it [`SETTLE`] after the real now and returns that: a snapshot's own animation starts there, everything its
/// setup started has come to rest.
pub fn hold_settled() -> Instant {
    let at = Instant::now() + SETTLE;
    freeze(at);
    at
}

/// The frozen clock moves on by `by`.
pub fn step(by: Duration) {
    freeze(now() + by);
}

/// Back to the real clock.
pub fn thaw() {
    AT.with(|held| held.set(None));
}

/// Exactly `n` frames of 60 Hz.
pub fn frames(n: u32) -> Duration {
    Duration::from_nanos(n as u64 * 16_666_667)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frozen_it_stands_still_and_steps_by_whole_frames() {
        let at = hold();
        std::thread::sleep(Duration::from_millis(5));
        assert_eq!(now(), at, "frozen: the real clock moving on changes nothing");
        step(frames(3));
        assert_eq!(now(), at + Duration::from_nanos(50_000_001));
        thaw();
        assert!(now() >= at, "thawed: the real clock again");
        let later = hold_settled();
        assert!(later >= Instant::now() + SETTLE - Duration::from_millis(50) && now() == later);
        thaw();
    }
}
