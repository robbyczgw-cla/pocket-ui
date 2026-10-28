//! In-game shortcuts of pocket-core: HOME held plus another button.
//!
//! While HOME is down the game gets no buttons, so a shortcut never presses anything in the game.
//! A HOME tap with nothing else pressed is not ours: pocket-ui sees the same events and opens the
//! pause menu on release (it treats every other button as "this was a shortcut").
//!
//! The Mini V2's L2/R2 are analog triggers; they count as pressed from half travel. The d-pad
//! arrives as buttons (hat events are turned into Left/Right/Up/Down by `pad`).

use crate::pad::{Axis, Button, PadEvent};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    Save,
    Load,
    SlotPrev,
    SlotNext,
    ToggleFastForward,
    /// HOME + Y: Over the game goes on to its next level (`over_game`).
    NextOverGame,
    /// HOME + A: a screenshot of the game's picture (`screenshot`).
    Screenshot,
}

/// Which shortcuts are switched on (Options > Shortcuts).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Enabled {
    pub save: bool,
    pub load: bool,
    pub slot: bool,
    pub fast_forward: bool,
    pub rewind: bool,
    /// HOME + Y, Over the game (`over_game::SHORTCUT_KEY`).
    pub over_game: bool,
    pub screenshot: bool,
}

impl Default for Enabled {
    fn default() -> Self {
        Enabled { save: true, load: true, slot: true, fast_forward: true, rewind: true, over_game: true, screenshot: true }
    }
}

/// Analog travel (0 to 1) from which a trigger counts as pressed.
const TRIGGER: f32 = 0.5;

#[derive(Default)]
pub struct Hotkeys {
    pub enabled: Enabled,
    home: bool,
    l1: bool,
    r1: bool,
    left: bool,
    right: bool,
    l2: bool,
    r2: bool,
    y: bool,
    /// The right face button, "A" on the Mini V2 (`Button::East`).
    a: bool,
    pending: Vec<Action>,
}

impl Hotkeys {
    pub fn new(enabled: Enabled) -> Self {
        Hotkeys { enabled, ..Default::default() }
    }

    /// Feeds one input event; completed shortcuts are queued for `take`.
    pub fn feed(&mut self, e: &PadEvent) {
        match *e {
            PadEvent::Button { button, pressed, .. } => self.button(button, pressed),
            PadEvent::Axis { axis: Axis::L2, value, .. } => self.button(Button::L2, value > TRIGGER),
            PadEvent::Axis { axis: Axis::R2, value, .. } => self.button(Button::R2, value > TRIGGER),
            _ => {}
        }
    }

    fn button(&mut self, button: Button, pressed: bool) {
        let slot = match button {
            Button::Mode => {
                self.home = pressed;
                return;
            }
            Button::L1 => &mut self.l1,
            Button::R1 => &mut self.r1,
            Button::Left => &mut self.left,
            Button::Right => &mut self.right,
            Button::L2 => &mut self.l2,
            Button::R2 => &mut self.r2,
            Button::West => &mut self.y,
            Button::East => &mut self.a,
            _ => return,
        };
        let was = std::mem::replace(slot, pressed);
        // A shortcut fires when its button goes down while HOME is already held.
        if !pressed || was || !self.home {
            return;
        }
        let e = self.enabled;
        let action = match button {
            Button::R1 if e.save => Action::Save,
            Button::L1 if e.load => Action::Load,
            Button::Left if e.slot => Action::SlotPrev,
            Button::Right if e.slot => Action::SlotNext,
            Button::R2 if e.fast_forward => Action::ToggleFastForward,
            Button::West if e.over_game => Action::NextOverGame,
            Button::East if e.screenshot => Action::Screenshot,
            _ => return,
        };
        self.pending.push(action);
    }

    /// HOME is down: the game's buttons are held back.
    pub fn home_held(&self) -> bool {
        self.home
    }

    /// HOME + L2 is held and the shortcut is on.
    pub fn rewind_held(&self) -> bool {
        self.home && self.l2 && self.enabled.rewind
    }

    /// The shortcuts completed since the last call.
    pub fn take(&mut self) -> Vec<Action> {
        std::mem::take(&mut self.pending)
    }
}

/// What a button event does to "HOME is down and nothing else has been pressed yet" (a tap), in pocket-ui while a game
/// runs: returns the new state and whether the pause menu opens now. HOME going down starts a tap, HOME coming up
/// ends it (the menu opens if it was still a tap), and any other button going down while HOME is held makes it a
/// shortcut instead (HOME + A is the screenshot, HOME + R1 saves ...): no menu on release.
pub fn home_tap_step(tap: bool, button: Button, pressed: bool) -> (bool, bool) {
    match (button, pressed) {
        (Button::Mode, true) => (true, false),
        (Button::Mode, false) => (false, tap),
        (_, true) => (false, false),
        _ => (tap, false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(h: &mut Hotkeys, b: Button, pressed: bool) {
        h.feed(&PadEvent::Button { source: 0, button: b, pressed });
    }

    fn tap(h: &mut Hotkeys, b: Button) {
        press(h, b, true);
        press(h, b, false);
    }

    #[test]
    fn home_plus_button_fires_once_per_press() {
        let mut h = Hotkeys::default();
        press(&mut h, Button::Mode, true);
        tap(&mut h, Button::R1);
        tap(&mut h, Button::L1);
        tap(&mut h, Button::Left);
        tap(&mut h, Button::Right);
        tap(&mut h, Button::R2);
        tap(&mut h, Button::West);
        tap(&mut h, Button::East);
        assert_eq!(h.take(), [Action::Save, Action::Load, Action::SlotPrev, Action::SlotNext, Action::ToggleFastForward, Action::NextOverGame, Action::Screenshot]);
        assert!(h.take().is_empty());
    }

    #[test]
    fn a_button_alone_or_pressed_before_home_does_nothing() {
        let mut h = Hotkeys::default();
        tap(&mut h, Button::R1);
        press(&mut h, Button::L1, true); // already down when HOME arrives
        press(&mut h, Button::Mode, true);
        press(&mut h, Button::L1, false);
        assert!(h.take().is_empty());
        press(&mut h, Button::Mode, false);
        tap(&mut h, Button::R1);
        assert!(h.take().is_empty(), "HOME released");
    }

    #[test]
    fn home_blocks_the_game_only_while_held() {
        let mut h = Hotkeys::default();
        assert!(!h.home_held());
        press(&mut h, Button::Mode, true);
        assert!(h.home_held());
        press(&mut h, Button::Mode, false);
        assert!(!h.home_held());
    }

    #[test]
    fn analog_triggers_count_from_half_travel_and_do_not_repeat() {
        let mut h = Hotkeys::default();
        press(&mut h, Button::Mode, true);
        for v in [0.2, 0.6, 0.8, 1.0, 0.9] {
            h.feed(&PadEvent::Axis { source: 0, axis: Axis::R2, value: v });
        }
        assert_eq!(h.take(), [Action::ToggleFastForward], "one press, however many axis reports");
        for v in [0.3, 0.0, 0.7] {
            h.feed(&PadEvent::Axis { source: 0, axis: Axis::R2, value: v });
        }
        assert_eq!(h.take(), [Action::ToggleFastForward], "released and pressed again");
    }

    #[test]
    fn rewind_is_held_not_toggled() {
        let mut h = Hotkeys::default();
        press(&mut h, Button::Mode, true);
        assert!(!h.rewind_held());
        h.feed(&PadEvent::Axis { source: 0, axis: Axis::L2, value: 0.9 });
        assert!(h.rewind_held());
        h.feed(&PadEvent::Axis { source: 0, axis: Axis::L2, value: 0.0 });
        assert!(!h.rewind_held());
        // The button form of L2 (a virtual pad) works too, and letting go of HOME stops it.
        press(&mut h, Button::L2, true);
        assert!(h.rewind_held());
        press(&mut h, Button::Mode, false);
        assert!(!h.rewind_held());
        assert!(h.take().is_empty());
    }

    #[test]
    fn switched_off_shortcuts_stay_quiet() {
        let mut h = Hotkeys::new(Enabled { save: false, rewind: false, ..Default::default() });
        press(&mut h, Button::Mode, true);
        tap(&mut h, Button::R1);
        tap(&mut h, Button::L1);
        press(&mut h, Button::L2, true);
        assert_eq!(h.take(), [Action::Load]);
        assert!(!h.rewind_held());
        h.enabled.rewind = true;
        assert!(h.rewind_held());
    }

    #[test]
    fn other_buttons_are_ignored() {
        let mut h = Hotkeys::default();
        press(&mut h, Button::Mode, true);
        tap(&mut h, Button::South);
        tap(&mut h, Button::Start);
        tap(&mut h, Button::Up);
        assert!(h.take().is_empty());
    }

    #[test]
    fn home_plus_a_is_the_screenshot_and_only_with_home_held_first() {
        let mut h = Hotkeys::default();
        tap(&mut h, Button::East);
        assert!(h.take().is_empty(), "A alone is the game's");
        press(&mut h, Button::East, true); // A already down when HOME arrives
        press(&mut h, Button::Mode, true);
        press(&mut h, Button::East, false);
        assert!(h.take().is_empty());
        tap(&mut h, Button::East);
        assert_eq!(h.take(), [Action::Screenshot]);
        assert!(h.home_held(), "the game gets no A while HOME is down");
        // Held down, it fires once, not on every report.
        press(&mut h, Button::East, true);
        press(&mut h, Button::East, true);
        press(&mut h, Button::East, true);
        assert_eq!(h.take(), [Action::Screenshot]);
    }

    #[test]
    fn the_screenshot_can_be_switched_off_like_the_other_shortcuts() {
        let mut h = Hotkeys::new(Enabled { screenshot: false, ..Enabled::default() });
        press(&mut h, Button::Mode, true);
        tap(&mut h, Button::East);
        assert!(h.take().is_empty());
        tap(&mut h, Button::R1);
        assert_eq!(h.take(), [Action::Save], "the others still work");
    }

    /// Plays events through `home_tap_step` the way pocket-ui's `in_game_input` does; returns how often the pause menu opened.
    fn menus_opened(events: &[(Button, bool)]) -> usize {
        let mut tap = false;
        let mut opened = 0;
        for &(b, pressed) in events {
            let (t, open) = home_tap_step(tap, b, pressed);
            tap = t;
            opened += open as usize;
        }
        opened
    }

    #[test]
    fn a_home_tap_opens_the_pause_menu_but_home_plus_a_does_not() {
        use Button::{East, Mode};
        assert_eq!(menus_opened(&[(Mode, true), (Mode, false)]), 1, "a tap");
        // The screenshot: HOME down, A down and up, HOME up.
        assert_eq!(menus_opened(&[(Mode, true), (East, true), (East, false), (Mode, false)]), 0);
        // HOME released first, A still down.
        assert_eq!(menus_opened(&[(Mode, true), (East, true), (Mode, false), (East, false)]), 0);
        // Two screenshots in one HOME hold.
        assert_eq!(menus_opened(&[(Mode, true), (East, true), (East, false), (East, true), (East, false), (Mode, false)]), 0);
        // The next tap works again.
        assert_eq!(menus_opened(&[(Mode, true), (East, true), (East, false), (Mode, false), (Mode, true), (Mode, false)]), 1);
        // A alone never opens it.
        assert_eq!(menus_opened(&[(East, true), (East, false)]), 0);
    }
}
