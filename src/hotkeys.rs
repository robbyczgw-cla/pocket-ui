//! In-game shortcuts of pocket-core: HOME held plus another button.
//!
//! While HOME is down the game gets no buttons, so a shortcut never presses anything in the game.
//! A HOME tap with nothing else pressed is not ours: pocket-ui sees the same events and opens the
//! pause menu on release (it treats every other button as "this was a shortcut").
//!
//! The Mini V2's L2/R2 are analog triggers; they count as pressed from half travel. The d-pad
//! arrives as buttons (hat events are turned into Left/Right/Up/Down by `pad`).

use crate::pad::{Button, PadEvent};
#[cfg(test)]
use crate::pad::Axis;

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
    /// HOME + X, Zoom (`hk_zoom`): off unless switched on. pocket-ui acts on it (the pause menu opens on Zoom, `app_zoom`);
    /// it is here so that the settings are one set and Kids Mode's `none` covers it.
    pub zoom: bool,
}

impl Default for Enabled {
    fn default() -> Self {
        Enabled { save: true, load: true, slot: true, fast_forward: true, rewind: true, over_game: true, screenshot: true, zoom: false }
    }
}

impl Enabled {
    /// Kids Mode: nothing but the HOME tap, which is pocket-ui's.
    pub fn none() -> Self {
        Enabled { save: false, load: false, slot: false, fast_forward: false, rewind: false, over_game: false, screenshot: false, zoom: false }
    }
}

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
    /// The Brick's F1 and F2 (`brick_hw`): held, and what each does (set from the player's choice; none elsewhere).
    #[cfg(feature = "brick")]
    f_down: [bool; 2],
    #[cfg(feature = "brick")]
    f_action: [Option<Action>; 2],
    pending: Vec<Action>,
    suppressed: bool,
}

impl Hotkeys {
    pub fn new(enabled: Enabled) -> Self {
        Hotkeys { enabled, ..Default::default() }
    }

    /// Commands gathered while the game is paused, asleep or dark are discarded.
    pub fn suppress(&mut self, on: bool) {
        self.suppressed = on;
        if on { self.pending.clear(); }
    }

    /// Feeds one input event; completed shortcuts are queued for `take`.
    pub fn feed(&mut self, e: &PadEvent) {
        match *e {
            PadEvent::Button { button, pressed, .. } => self.button(button, pressed),
            // pad has already thresholded (with hysteresis) and remapped triggers as buttons.
            // Reading the raw axis here too would fire the old shortcut after a device remap.
            _ => {}
        }
    }

    /// What F1 and F2 do (`brick_hw::FKeys`); an action the Shortcuts page has switched off stays off.
    #[cfg(feature = "brick")]
    pub fn set_f_keys(&mut self, actions: [Option<Action>; 2]) {
        self.f_action = actions;
    }

    #[cfg(feature = "brick")]
    fn f_key(&mut self, i: usize, pressed: bool) {
        let was = std::mem::replace(&mut self.f_down[i], pressed);
        if !pressed || was || self.suppressed {
            return;
        }
        let e = self.enabled;
        match self.f_action[i] {
            Some(a @ Action::Save) if e.save => self.pending.push(a),
            Some(a @ Action::Load) if e.load => self.pending.push(a),
            Some(a @ Action::Screenshot) if e.screenshot => self.pending.push(a),
            _ => {}
        }
    }

    fn button(&mut self, button: Button, pressed: bool) {
        #[cfg(feature = "brick")]
        if matches!(button, Button::F1 | Button::F2) {
            return self.f_key((button == Button::F2) as usize, pressed);
        }
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
        if !pressed || was || !self.home || self.suppressed {
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
        !self.suppressed && self.home && self.l2 && self.enabled.rewind
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

/// The action of pocket-core a Brick F key choice stands for; `Recent games` is pocket-ui's, `Nothing` is nothing.
#[cfg(feature = "brick")]
pub fn action_of(k: crate::brick_hw::FKey) -> Option<Action> {
    use crate::brick_hw::FKey;
    match k {
        FKey::Save => Some(Action::Save),
        FKey::Load => Some(Action::Load),
        FKey::Screenshot => Some(Action::Screenshot),
        FKey::Recent | FKey::Pause | FKey::Nothing => None,
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
    fn save_safety_shortcuts_during_menu_sleep_and_dark_never_replay() {
        for blocked in ["menu", "sleep", "dark"] {
            let mut h = Hotkeys::new(Enabled::default());
            press(&mut h, Button::Mode, true);
            tap(&mut h, Button::R1); // queued just before the pause was taken
            h.suppress(true);
            tap(&mut h, Button::L1);
            tap(&mut h, Button::R2);
            h.suppress(false);
            assert!(h.take().is_empty(), "{blocked}: no deferred save, load or speed action");
            tap(&mut h, Button::R1);
            assert_eq!(h.take(), vec![Action::Save], "a fresh gameplay press still works");
        }
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
    fn analog_reports_do_not_bypass_a_controllers_remapped_trigger() {
        let mut h = Hotkeys::default();
        press(&mut h, Button::Mode, true);
        for v in [0.2, 0.6, 0.8, 0.45, 0.56] {
            h.feed(&PadEvent::Axis { source: 0, axis: Axis::R2, value: v });
        }
        assert!(h.take().is_empty());
        press(&mut h, Button::R2, true);
        press(&mut h, Button::R2, true);
        assert_eq!(h.take(), [Action::ToggleFastForward]);
    }

    #[test]
    fn rewind_is_held_not_toggled() {
        let mut h = Hotkeys::default();
        press(&mut h, Button::Mode, true);
        assert!(!h.rewind_held());
        press(&mut h, Button::L2, true);
        assert!(h.rewind_held());
        press(&mut h, Button::L2, false);
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

    #[test]
    #[cfg(feature = "brick")]
    fn brick_fkeys_after_pause_sleep_and_dark_require_a_fresh_press() {
        for blocked in ["pause", "sleep", "dark"] {
            let mut h = Hotkeys::new(Enabled::default());
            h.set_f_keys([Some(Action::Save), Some(Action::Load)]);
            tap(&mut h, Button::F1);
            h.suppress(true);
            press(&mut h, Button::F1, true);
            press(&mut h, Button::F2, true);
            h.suppress(false);
            assert!(h.take().is_empty(), "{blocked}: no queued or suspended save/load replays");
            press(&mut h, Button::F1, true);
            press(&mut h, Button::F2, true);
            assert!(h.take().is_empty(), "{blocked}: held keys do not act on resume");
            press(&mut h, Button::F1, false);
            press(&mut h, Button::F2, false);
            tap(&mut h, Button::F1);
            tap(&mut h, Button::F2);
            assert_eq!(h.take(), [Action::Save, Action::Load], "{blocked}: new gameplay presses still work");
        }
    }

    #[test]
    #[cfg(feature = "brick")]
    fn the_brick_f_keys_do_what_they_were_given_without_home_and_on_the_press_only() {
        let mut h = Hotkeys::new(Enabled::default());
        let press = |h: &mut Hotkeys, b: Button, down: bool| h.feed(&PadEvent::Button { source: 0, button: b, pressed: down });
        press(&mut h, Button::F1, true);
        assert!(h.take().is_empty(), "nothing is set yet: a plain Brick does nothing");
        h.set_f_keys([Some(Action::Save), Some(Action::Load)]);
        press(&mut h, Button::F1, false);
        press(&mut h, Button::F1, true);
        press(&mut h, Button::F1, true);
        press(&mut h, Button::F2, true);
        assert_eq!(h.take(), [Action::Save, Action::Load], "one action per press, no HOME needed");
        // Held: no repeat.
        press(&mut h, Button::F2, true);
        assert!(h.take().is_empty());
        // The Shortcuts page (and Kids Mode) still switch them off.
        let mut kids = Hotkeys::new(Enabled::none());
        kids.set_f_keys([Some(Action::Save), Some(Action::Screenshot)]);
        press(&mut kids, Button::F1, true);
        press(&mut kids, Button::F2, true);
        assert!(kids.take().is_empty());
        // F keys do not disturb HOME shortcuts.
        let mut both = Hotkeys::new(Enabled::default());
        both.set_f_keys([Some(Action::Screenshot), None]);
        press(&mut both, Button::Mode, true);
        press(&mut both, Button::R1, true);
        press(&mut both, Button::F1, true);
        assert_eq!(both.take(), [Action::Save, Action::Screenshot]);
    }
}
