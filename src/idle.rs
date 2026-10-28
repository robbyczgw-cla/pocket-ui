//! When pocket-ui may stay asleep. While a game runs, pocket-ui's window sits behind the game's and nothing it
//! would draw can be seen, unless one of its own screens (the pause menu, the game switcher) is up. It should then
//! use no CPU: wake for HOME, for the game's end and for timers measured in seconds.
//!
//! The pad is read by pocket-ui all the time (HOME, the power key and the volume keys belong to it, and any input
//! keeps the device awake), so every stick movement and every button of the game reaches it: a hundred events a
//! second with the sticks in use. Each of them used to end in a redraw of the whole shelf behind the game, 11 % of a
//! CPU in the virtual device with the left stick wiggled (`scripts/idle-cpu.sh`), about 10 % on the device behind
//! Dolphin. Now:
//!
//! * a pad event the game's input handling deals with draws nothing unless a pocket-ui screen is up ([`game_input_needs_redraw`]);
//! * the pad reader passes a stick's movements on at most twice a second while a game runs ([`StickThrottle`]): the
//!   only thing the UI does with them then is to note that someone is playing (the idle timer counts minutes);
//! * and the rules that keep the loop asleep are where they are used: `App::animating` (main.rs) only asks for frames
//!   while something moves on a screen that is shown, `about_to_wait` waits for the next deadline,
//!   `quick::Sys::next_wake` and `sleep::Sleep::tick` give theirs, and `audio::Sink` closes the stream a menu sound
//!   opened (an open stream keeps a PipeWire thread running).

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::Instant;

use crate::pad::{Axis, PadEvent};

/// Whether a pad event that the game's input handling has just dealt with can have changed what the window
/// shows. `screen_before` and `screen_after`: one of pocket-ui's own screens (the pause menu, the switcher) was
/// up before the event, or is up after it. A button that opens one, closes one or moves in one changes the
/// picture; everything else is the game's own input and changes nothing.
pub fn game_input_needs_redraw(screen_before: bool, screen_after: bool) -> bool {
    screen_before || screen_after
}

/// How often a stick's movement is passed on to the UI thread while a game runs and no pocket-ui screen is up.
pub const STICK_PASS_EVERY_MS: u64 = 500;

/// Keeps the pad reader from waking the UI thread a hundred times a second for a stick the game is using.
/// Buttons, the triggers (HOME + L2/R2 is a shortcut) and everything while a screen of ours is up pass untouched.
/// The UI thread says when a game runs behind a closed menu (`set_quiet`); the reader thread asks (`pass`).
pub struct StickThrottle {
    quiet: AtomicBool,
    last_ms: AtomicU64,
}

impl Default for StickThrottle {
    fn default() -> Self {
        Self::new()
    }
}

impl StickThrottle {
    pub const fn new() -> Self {
        StickThrottle {
            quiet: AtomicBool::new(false),
            last_ms: AtomicU64::new(0),
        }
    }

    /// A game runs and nothing of ours is on screen: sticks are thinned out.
    pub fn set_quiet(&self, quiet: bool) {
        self.quiet.store(quiet, Ordering::Relaxed);
    }

    /// Whether `ev` goes on to the UI thread, `now_ms` milliseconds after any fixed start (not 0).
    pub fn pass_at(&self, ev: &PadEvent, now_ms: u64) -> bool {
        if !self.quiet.load(Ordering::Relaxed) || !matches!(ev, PadEvent::Axis { axis: Axis::LeftX | Axis::LeftY | Axis::RightX | Axis::RightY, .. }) {
            return true;
        }
        let last = self.last_ms.load(Ordering::Relaxed);
        if last != 0 && now_ms.saturating_sub(last) < STICK_PASS_EVERY_MS {
            return false;
        }
        self.last_ms.store(now_ms, Ordering::Relaxed);
        true
    }

    /// [`pass_at`](Self::pass_at) with the clock.
    pub fn pass(&self, ev: &PadEvent) -> bool {
        static START: OnceLock<Instant> = OnceLock::new();
        // Offset by a second so that 0 means "none yet".
        self.pass_at(ev, START.get_or_init(Instant::now).elapsed().as_millis() as u64 + 1000)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pad::Button;

    fn stick(axis: Axis, value: f32) -> PadEvent {
        PadEvent::Axis { source: 0, axis, value }
    }

    fn button(button: Button) -> PadEvent {
        PadEvent::Button { source: 0, button, pressed: true }
    }

    #[test]
    fn the_games_own_input_draws_nothing_behind_the_game() {
        // sticks, triggers and buttons while no pocket-ui screen is up: a hundred a second, no frame for any
        assert!(!game_input_needs_redraw(false, false));
    }

    #[test]
    fn opening_moving_in_and_closing_the_pause_menu_or_the_switcher_do_draw() {
        assert!(game_input_needs_redraw(false, true), "HOME opened it");
        assert!(game_input_needs_redraw(true, true), "a button moved in it");
        assert!(game_input_needs_redraw(true, false), "it closed: the last frame of the window is the game's turn");
    }

    #[test]
    fn sticks_are_thinned_out_while_a_game_runs_behind_a_closed_menu() {
        let t = StickThrottle::new();
        t.set_quiet(true);
        let mut passed = 0;
        // a hundred events a second for ten seconds
        for i in 0..1000u64 {
            let axis = if i % 2 == 0 { Axis::LeftX } else { Axis::RightY };
            passed += t.pass_at(&stick(axis, 0.7), 1000 + i * 10) as u32;
        }
        assert_eq!(passed, 20, "twice a second: one per {STICK_PASS_EVERY_MS} ms");
    }

    #[test]
    fn the_first_movement_gets_through_at_once_so_that_it_wakes_a_screen_that_went_dark() {
        let t = StickThrottle::new();
        t.set_quiet(true);
        assert!(t.pass_at(&stick(Axis::LeftY, -1.0), 1000));
        assert!(!t.pass_at(&stick(Axis::LeftY, -0.9), 1001));
        assert!(!t.pass_at(&stick(Axis::LeftY, -0.8), 1000 + STICK_PASS_EVERY_MS - 1));
        assert!(t.pass_at(&stick(Axis::LeftY, -0.8), 1000 + STICK_PASS_EVERY_MS));
    }

    #[test]
    fn buttons_and_triggers_always_pass() {
        let t = StickThrottle::new();
        t.set_quiet(true);
        assert!(t.pass_at(&stick(Axis::LeftX, 0.5), 1000));
        for ms in 1001..1100 {
            assert!(t.pass_at(&button(Button::Mode), ms), "HOME");
            assert!(t.pass_at(&button(Button::South), ms));
            assert!(t.pass_at(&stick(Axis::L2, 1.0), ms), "HOME + L2 is a shortcut, not a tap");
            assert!(t.pass_at(&stick(Axis::R2, 1.0), ms));
        }
    }

    #[test]
    fn nothing_is_held_back_when_a_screen_of_ours_is_up_or_no_game_runs() {
        let t = StickThrottle::new();
        for ms in 1000..1100 {
            assert!(t.pass_at(&stick(Axis::LeftX, 0.5), ms), "not quiet: menus and the manual use the sticks");
        }
        t.set_quiet(true);
        assert!(t.pass_at(&stick(Axis::LeftX, 0.5), 2000));
        t.set_quiet(false);
        assert!(t.pass_at(&stick(Axis::LeftX, 0.5), 2001), "the pause menu opened");
    }

    #[test]
    fn the_clock_version_passes_the_first_and_thins_the_rest() {
        let t = StickThrottle::new();
        t.set_quiet(true);
        let e = stick(Axis::LeftX, 0.5);
        assert!(t.pass(&e));
        assert!(!t.pass(&e), "a moment later");
    }
}
