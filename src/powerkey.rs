//! What one press of the power button is: a short press or a long one (held for `LONG_PRESS`). Pure state machine
//! (no clock of its own, no side effects), so the rules are unit-tested; `sleep.rs` feeds it the key events and does
//! what Settings → System → Sleep & power says (`sleep_settings`: a short press sleeps or turns the screen off, a long
//! one opens the power menu or sleeps).
//!
//! The decision for a short press can only be made when the key goes up (before that it might still
//! become a long one), so it acts at the release, typically 80 to 150 ms after the press.
//! A long press is told while the key is still held (`poll`, from the event loop's timer: the power menu opens then),
//! and its release once more (`LongEnd`: a long press that sleeps sleeps then, so the key is up before the suspend);
//! a long press never also counts as a short one.

use std::time::{Duration, Instant};

/// How long the power button must be held to open the power menu.
pub const LONG_PRESS: Duration = Duration::from_millis(800);

/// What the caller does about a power key event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    /// Not ours (logind or the menu handles it): let the event go on.
    Pass,
    /// Consumed; nothing to do yet.
    Swallow,
    /// A short press ended.
    Short,
    /// A long press, while the key is held (or at its release, when the timer didn't catch it before).
    Long,
    /// The key went up after `Long` was told while it was held.
    LongEnd,
}

/// What the caller knows when a press arrives.
#[derive(Clone, Copy, Debug, Default)]
pub struct Gate {
    /// pocket-ui handles the key (the logind inhibitor is held); otherwise logind suspends by itself.
    pub ours: bool,
    /// The press is the one that just woke the device (or its bounce).
    pub guarded: bool,
    /// The power menu is open: a press belongs to the menu (it closes it).
    pub menu_open: bool,
}

#[derive(Debug, Default)]
pub struct PowerKey {
    down: Option<Instant>,
    fired: bool,
}

impl PowerKey {
    /// The key went down. A second report of the same press (two input devices) keeps the first one's time.
    pub fn press(&mut self, now: Instant, gate: Gate) -> Step {
        if gate.menu_open || !gate.ours {
            return Step::Pass;
        }
        if gate.guarded {
            return Step::Swallow;
        }
        if self.down.is_none() {
            self.down = Some(now);
            self.fired = false;
        }
        Step::Swallow
    }

    /// The key went up: `Short` for a short press, `Long` for a long one the timer hadn't caught yet,
    /// `LongEnd` for one `poll` told already, `Pass` for a release whose press was not ours.
    pub fn release(&mut self, now: Instant) -> Step {
        let Some(since) = self.down.take() else { return Step::Pass };
        if std::mem::take(&mut self.fired) {
            Step::LongEnd
        } else if now.duration_since(since) >= LONG_PRESS {
            Step::Long
        } else {
            Step::Short
        }
    }

    /// Call from the timer: `Long` once, as soon as the key has been held for `LONG_PRESS`.
    pub fn poll(&mut self, now: Instant) -> Step {
        match self.down {
            Some(since) if !self.fired && now.duration_since(since) >= LONG_PRESS => {
                self.fired = true;
                Step::Long
            }
            _ => Step::Pass,
        }
    }

    /// When `poll` will have something to say, for the event loop's timer.
    pub fn deadline(&self) -> Option<Instant> {
        self.down.filter(|_| !self.fired).map(|since| since + LONG_PRESS)
    }

    pub fn is_down(&self) -> bool {
        self.down.is_some()
    }

    /// Forget the key (the device went to sleep or woke up meanwhile).
    pub fn cancel(&mut self) {
        self.down = None;
        self.fired = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OURS: Gate = Gate {
        ours: true,
        guarded: false,
        menu_open: false,
    };

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn a_short_press_is_told_when_the_key_goes_up() {
        let (mut k, t0) = (PowerKey::default(), Instant::now());
        assert_eq!(k.press(t0, OURS), Step::Swallow);
        assert_eq!(k.poll(t0 + ms(100)), Step::Pass, "nothing yet while it may still become a long press");
        assert_eq!(k.release(t0 + ms(120)), Step::Short);
        assert!(!k.is_down());
        // The next press starts from scratch.
        assert_eq!(k.press(t0 + ms(2000), OURS), Step::Swallow);
        assert_eq!(k.release(t0 + ms(2100)), Step::Short);
    }

    #[test]
    fn a_long_press_is_told_while_held_and_its_release_once_more() {
        let (mut k, t0) = (PowerKey::default(), Instant::now());
        k.press(t0, OURS);
        assert_eq!(k.poll(t0 + LONG_PRESS - ms(1)), Step::Pass);
        assert_eq!(k.poll(t0 + LONG_PRESS), Step::Long);
        assert_eq!(k.poll(t0 + LONG_PRESS + ms(500)), Step::Pass, "only once");
        assert_eq!(k.release(t0 + LONG_PRESS + ms(900)), Step::LongEnd, "never a short press after a long one");
        assert!(!k.is_down());
    }

    #[test]
    fn a_release_after_the_threshold_that_the_timer_missed_is_still_a_long_press() {
        let (mut k, t0) = (PowerKey::default(), Instant::now());
        k.press(t0, OURS);
        assert_eq!(k.release(t0 + LONG_PRESS + ms(50)), Step::Long);
        assert_eq!(k.release(t0 + LONG_PRESS + ms(60)), Step::Pass, "a second release has no press");
    }

    #[test]
    fn the_deadline_is_the_threshold_while_the_key_is_down_and_unfired() {
        let (mut k, t0) = (PowerKey::default(), Instant::now());
        assert_eq!(k.deadline(), None);
        k.press(t0, OURS);
        assert_eq!(k.deadline(), Some(t0 + LONG_PRESS));
        k.poll(t0 + LONG_PRESS);
        assert_eq!(k.deadline(), None, "fired");
        k.release(t0 + ms(2000));
        k.press(t0 + ms(3000), OURS);
        k.release(t0 + ms(3050));
        assert_eq!(k.deadline(), None, "released");
    }

    #[test]
    fn a_second_device_reporting_the_same_press_does_not_restart_the_clock() {
        let (mut k, t0) = (PowerKey::default(), Instant::now());
        k.press(t0, OURS);
        assert_eq!(k.press(t0 + ms(500), OURS), Step::Swallow);
        assert_eq!(k.poll(t0 + LONG_PRESS), Step::Long);
    }

    #[test]
    fn a_release_without_a_press_is_not_ours() {
        let (mut k, t0) = (PowerKey::default(), Instant::now());
        assert_eq!(k.release(t0), Step::Pass, "the release of the press that woke the device");
    }

    #[test]
    fn presses_that_are_not_ours_are_not_tracked() {
        let (mut k, t0) = (PowerKey::default(), Instant::now());
        // logind handles the key (no inhibitor): the event goes on, nothing sleeps or opens.
        assert_eq!(k.press(t0, Gate { ours: false, ..OURS }), Step::Pass);
        assert_eq!(k.poll(t0 + LONG_PRESS), Step::Pass);
        assert_eq!(k.release(t0 + ms(900)), Step::Pass);
        // The press that woke the device: swallowed, and its release neither sleeps nor opens the menu.
        assert_eq!(k.press(t0 + ms(1000), Gate { guarded: true, ..OURS }), Step::Swallow);
        assert_eq!(k.poll(t0 + ms(3000)), Step::Pass);
        assert_eq!(k.release(t0 + ms(3100)), Step::Pass);
    }

    #[test]
    fn with_the_menu_open_a_press_goes_to_the_menu_and_never_sleeps() {
        let (mut k, t0) = (PowerKey::default(), Instant::now());
        let open = Gate { menu_open: true, ..OURS };
        assert_eq!(k.press(t0, open), Step::Pass, "the menu closes on it");
        assert_eq!(k.poll(t0 + LONG_PRESS), Step::Pass);
        assert_eq!(k.release(t0 + ms(100)), Step::Pass, "and the release is not a short press");
    }

    #[test]
    fn cancel_forgets_a_key_that_was_down() {
        let (mut k, t0) = (PowerKey::default(), Instant::now());
        k.press(t0, OURS);
        k.cancel();
        assert_eq!(k.poll(t0 + LONG_PRESS), Step::Pass);
        assert_eq!(k.release(t0 + ms(2000)), Step::Pass);
    }
}
