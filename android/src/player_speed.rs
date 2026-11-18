//! Fast-forward and rewind in Android's player, run as pocket-core runs them (`pocket_ui::play_speed`): the pad's HOME
//! (Android's BUTTON_MODE, 110) is Linux's HOME. A tap opens the pause menu when it is let go, HOME + R2 turns fast-forward
//! on and off, HOME + L2 held plays backwards (`hotkeys`, the same code as Linux's); while HOME is down the game gets no
//! buttons. Linux's other HOME shortcuts (save, load, slot, screenshot, Over the game, Zoom) stay Linux's: Android owns
//! its Home layer (ROADMAP „Android-Player wie Karte“), and the player has those in its pause menu.
use pocket_ui::hotkeys::{home_tap_step, Action, Enabled, Hotkeys};
use pocket_ui::pad::{Button, PadEvent};
use pocket_ui::play_speed::{FastForward, RewindHold, REWIND_BYTES, REWIND_FRAMES};
use pocket_ui::rewind::Rewind;

/// Android's key code of the pad's HOME (KEYCODE_BUTTON_MODE): the Mini V2's HOME and a controller's guide button.
pub const HOME_KEY: i32 = 110;

/// The shortcuts the player has: Linux's fast-forward and rewind.
pub fn enabled() -> Enabled {
    Enabled { fast_forward: true, rewind: true, ..Enabled::none() }
}

/// The button a key code is for the shortcuts (`hotkeys` needs L2/R2; any other key only ends a HOME tap).
pub fn button_of(key: i32) -> Option<Button> {
    Some(match key {
        HOME_KEY => Button::Mode,
        102 => Button::L1,
        103 => Button::R1,
        104 => Button::L2,
        105 => Button::R2,
        96 => Button::South,
        97 => Button::East,
        99 => Button::West,
        100 => Button::North,
        19 => Button::Up,
        20 => Button::Down,
        21 => Button::Left,
        22 => Button::Right,
        108 => Button::Start,
        109 => Button::Select,
        _ => return None,
    })
}

pub struct Speed {
    hotkeys: Hotkeys,
    /// HOME is down and nothing else was pressed yet: letting go opens the pause menu (`home_tap_step`).
    tap: bool,
    /// Fast-forward is on (HOME + R2).
    pub ff_on: bool,
    pub hold: RewindHold,
    pub history: Rewind,
    /// The buffer `retro_serialize` writes into each refresh while Rewind is on.
    pub scratch: Vec<u8>,
    /// What the top right corner says now (`play_speed::badge`): the overlay is painted again when it changes.
    pub badge: String,
    /// Samples of one ordinary frame: the silence queued while fast-forwarding or rewinding.
    pub nominal_samples: usize,
}

impl Default for Speed {
    fn default() -> Self {
        Speed { hotkeys: Hotkeys::new(enabled()), tap: false, ff_on: false, hold: RewindHold::default(), history: Rewind::new(REWIND_BYTES, REWIND_FRAMES), scratch: Vec::new(), badge: String::new(), nominal_samples: 0 }
    }
}

impl Speed {
    /// A key of the pad while the game runs. Returns true when HOME was tapped (the pause menu opens now).
    pub fn key(&mut self, key: i32, down: bool) -> bool {
        let Some(button) = button_of(key) else {
            if down {
                self.tap = false;
            }
            return false;
        };
        self.button(button, down)
    }

    /// L2/R2 reported as axes (`pad::android::Trigger`), as they change.
    pub fn trigger(&mut self, right: bool, down: bool) {
        self.button(if right { Button::R2 } else { Button::L2 }, down);
    }

    fn button(&mut self, button: Button, down: bool) -> bool {
        let (tap, open) = home_tap_step(self.tap, button, down);
        self.tap = tap;
        self.hotkeys.feed(&PadEvent::Button { source: 0, button, pressed: down });
        for action in self.hotkeys.take() {
            if action == Action::ToggleFastForward {
                self.ff_on = !self.ff_on;
            }
        }
        open
    }

    /// HOME is down: the game's buttons are held back.
    pub fn home_held(&self) -> bool {
        self.hotkeys.home_held()
    }

    /// HOME + L2 is held.
    pub fn rewind_held(&self) -> bool {
        self.hotkeys.rewind_held()
    }

    /// The pause menu opened, or the game went to the background: what was held is let go (a HOME still down when the
    /// menu closes is no shortcut).
    pub fn release_all(&mut self) {
        for b in [Button::Mode, Button::L2, Button::R2] {
            self.hotkeys.feed(&PadEvent::Button { source: 0, button: b, pressed: false });
        }
        self.hotkeys.take();
        self.tap = false;
    }

    /// Rewind was switched off, or a state was loaded: the history is no longer the game's past.
    pub fn forget(&mut self) {
        self.history.clear();
    }

    /// Fast-forward while it is on, at the chosen speed.
    pub fn fast_forward(&self, speed: FastForward) -> Option<FastForward> {
        self.ff_on.then_some(speed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn home_tap_opens_on_release_and_a_shortcut_does_not() {
        let mut s = Speed::default();
        assert!(!s.key(HOME_KEY, true));
        assert!(s.home_held());
        assert!(s.key(HOME_KEY, false), "a tap opens the menu when HOME is let go");
        s.key(HOME_KEY, true);
        s.key(105, true);
        s.key(105, false);
        assert!(s.ff_on, "HOME + R2 turns fast-forward on");
        assert!(!s.key(HOME_KEY, false), "no menu after a shortcut");
        s.key(HOME_KEY, true);
        s.trigger(false, true);
        assert!(s.rewind_held(), "HOME + L2 as an axis rewinds");
        s.trigger(false, false);
        assert!(!s.rewind_held());
        assert!(!s.key(HOME_KEY, false));
        s.key(105, true);
        assert!(s.ff_on, "R2 alone is the game's");
    }

    #[test]
    fn only_linux_s_speed_shortcuts() {
        let e = enabled();
        assert!(e.fast_forward && e.rewind);
        assert!(!e.save && !e.load && !e.slot && !e.screenshot && !e.over_game && !e.zoom);
    }
}
