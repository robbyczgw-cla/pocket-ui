//! Settings → Help & tools → Test buttons (`app_test_buttons.rs` paints it): what every connected controller is doing
//! right now. For a player ("is this button broken?") and for support ("what does the pad really send?").
//!
//! It reads what the App gets from the pad threads, one controller at a time (`App::merge_pad` hands every event over
//! before the controllers are merged into one pad, as the button capture of Controller setup gets it). By then the
//! reader has applied the player's own layout of that controller (`pad::bindings`) and the Button layout setting
//! (`button_layout`); the tester takes both off again to find the physical button that was pressed, and then shows what
//! that button does: in pocket-ui's menus (both layers) and in a game (the controller's layout only, a game keeps the
//! console's own letters, see `button_layout`). Nothing here reads a file, starts a thread or touches the pads.
//!
//! Leaving is a long hold, so that every button (B and START too) can be tested: HOME for two seconds, or B and START
//! together on one controller.

use std::time::{Duration, Instant};

use crate::button_layout::ButtonLayout;
use crate::controls;
use crate::pad::bindings::Layout;
use crate::pad::{Axis, Button, Controller, PadEvent, STICK_DEADZONE};
use crate::players;

/// How long HOME (or B + START) is held to leave.
pub const HOLD: Duration = Duration::from_secs(2);
/// An analog trigger counts as pressed from here and as let go below `TRIGGER_RELEASE` (what the pad reader does,
/// `pad::decode`: a test keeps the two in step).
pub const TRIGGER_PRESS: f32 = 0.55;
pub const TRIGGER_RELEASE: f32 = 0.40;

/// How a controller is connected, from its identity key (`pad::controller_key`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    /// The handheld's own buttons.
    BuiltIn,
    Bluetooth,
    Usb,
}

impl Kind {
    pub fn of(c: &Controller) -> Kind {
        if c.key.starts_with("bt:") {
            Kind::Bluetooth
        } else if c.key.starts_with("usb:") {
            Kind::Usb
        } else {
            Kind::BuiltIn
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Kind::BuiltIn => "Built-in",
            Kind::Bluetooth => "Bluetooth",
            Kind::Usb => "USB",
        }
    }
}

/// The two layers between a physical button and the event the App gets.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mapping {
    /// The player's layout of this controller (`controllers.json`).
    pub layout: Layout,
    /// Settings → Playing → Buttons → Button layout.
    pub menu: ButtonLayout,
}

impl Mapping {
    /// The physical button behind `seen`, a button as the App's input gets it (both layers undone).
    pub fn physical(&self, seen: Button) -> Button {
        self.layout.physical(self.menu.role(seen))
    }

    /// What a physical button plays as in a game: pocket-core reads the controller's layout and nothing else.
    pub fn game(&self, physical: Button) -> Button {
        self.layout.role(physical)
    }

    /// What it is in pocket-ui's menus (A, B, X, Y are roles, see `button_layout`).
    pub fn menus(&self, physical: Button) -> Button {
        self.menu.role(self.layout.role(physical))
    }
}

fn at(b: Button) -> usize {
    b.bit().trailing_zeros() as usize
}

/// One analog trigger.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Trigger {
    /// 0 to 1 (a pad with a digital trigger gives 0 or 1).
    pub value: f32,
    /// The pad reports the travel (an axis); a digital trigger only knows pressed or not.
    pub analog: bool,
    pub pressed: bool,
}

/// One controller as the tester sees it.
#[derive(Clone, Debug)]
pub struct PadView {
    pub controller: Controller,
    /// Physical buttons down (`Button::bit`).
    held: u32,
    /// The buttons as the App got them (roles), for the way out.
    seen: u32,
    /// Which physical button each role in `seen` came from, kept from the press, so the release finds it even when the
    /// layout changed in between.
    came_from: [Option<Button>; Button::ALL.len()],
    left: (f32, f32),
    right: (f32, f32),
    trigger: [Trigger; 2],
    /// The last thing the pad sent that pocket-ui doesn't know (`PadEvent::Raw`).
    pub unknown: Option<String>,
}

impl PadView {
    fn new(controller: Controller) -> PadView {
        let rest = Trigger { value: 0.0, analog: false, pressed: false };
        PadView { controller, held: 0, seen: 0, came_from: [None; Button::ALL.len()], left: (0.0, 0.0), right: (0.0, 0.0), trigger: [rest; 2], unknown: None }
    }

    /// A physical button is down (a trigger also by its travel).
    pub fn is_down(&self, b: Button) -> bool {
        self.held & b.bit() != 0
            || match b {
                Button::L2 => self.trigger[0].pressed,
                Button::R2 => self.trigger[1].pressed,
                _ => false,
            }
    }

    /// Anything of this controller is down or pushed out of the dead zone.
    pub fn busy(&self) -> bool {
        self.held != 0 || self.moving()
    }

    /// A stick is out of its dead zone or a trigger is past its point.
    fn moving(&self) -> bool {
        self.trigger.iter().any(|t| t.pressed) || outside(self.left) || outside(self.right)
    }

    pub fn trigger(&self, left: bool) -> Trigger {
        self.trigger[if left { 0 } else { 1 }]
    }

    /// The stick as it reported, -1 to 1 on both axes (y up is negative, as the pad has it).
    pub fn stick(&self, left: bool) -> (f32, f32) {
        if left {
            self.left
        } else {
            self.right
        }
    }

    /// The stick is pushed out of the dead zone pocket-ui's menus use (`pad::STICK_DEADZONE`).
    pub fn pushed(&self, left: bool) -> bool {
        outside(self.stick(left))
    }

    fn leaving(&self) -> bool {
        let chord = CHORD_B.bit() | Button::Start.bit();
        self.seen & Button::Mode.bit() != 0 || self.seen & chord == chord
    }
}

/// B, the way back, as the App's input has it.
const CHORD_B: Button = Button::South;

fn outside((x, y): (f32, f32)) -> bool {
    x.abs() >= STICK_DEADZONE || y.abs() >= STICK_DEADZONE
}

/// The last button pressed, on any controller.
#[derive(Clone, Debug, PartialEq)]
pub struct Last {
    pub controller: Controller,
    pub physical: Button,
    /// What it is in pocket-ui's menus.
    pub menus: Button,
    /// What a game gets: the player's layout of this controller.
    pub game: Button,
}

impl Last {
    /// The player's own layout moved this button (a Button layout of Xbox does not count: that is a setting for all).
    pub fn remapped(&self) -> bool {
        self.game != self.physical
    }
}

/// How pocket-ui's menus call the button that is `role` there: A, B, X and Y for the four face buttons, else its own name.
pub fn menu_name(role: Button) -> &'static str {
    match role {
        Button::East => "A",
        Button::South => "B",
        Button::North => "X",
        Button::West => "Y",
        other => other.label(),
    }
}

/// How a game's console calls the button a physical one plays as (RetroPad, as pocket-core hosts it); `None` for the
/// buttons that are never a game's (HOME, BACK).
pub fn console_name(game: Button) -> Option<&'static str> {
    controls::retropad_bit(game).map(|id| controls::physical_label(id as u8))
}

/// A trigger's travel as a whole percent.
pub fn percent(v: f32) -> u32 {
    (v.clamp(0.0, 1.0) * 100.0).round() as u32
}

/// The tester's state: the controllers, which one is shown, the last press and the way out.
#[derive(Default)]
pub struct ButtonTest {
    pads: Vec<PadView>,
    selected: Option<String>,
    last: Option<Last>,
    hold_since: Option<Instant>,
}

impl ButtonTest {
    pub fn new(controllers: &[Controller], now: Instant) -> ButtonTest {
        let mut t = ButtonTest::default();
        t.sync(controllers, now);
        t
    }

    /// The controllers connected now (`PadState::controllers`): the ones that can play, as the Players page has them.
    /// What a controller that stays has done so far stays; a new one starts at rest; one that went is dropped.
    pub fn sync(&mut self, controllers: &[Controller], now: Instant) {
        let mut next: Vec<PadView> = Vec::new();
        for c in controllers.iter().filter(|c| players::seatable(c)) {
            match self.pads.iter().position(|p| p.controller.key == c.key) {
                Some(i) => {
                    let mut p = self.pads.swap_remove(i);
                    p.controller = c.clone();
                    next.push(p);
                }
                None => next.push(PadView::new(c.clone())),
            }
        }
        self.pads = next;
        if self.selected.as_ref().is_none_or(|k| !self.pads.iter().any(|p| &p.controller.key == k)) {
            self.selected = self.pads.first().map(|p| p.controller.key.clone());
        }
        self.refresh_hold(now);
    }

    pub fn pads(&self) -> &[PadView] {
        &self.pads
    }

    /// The controller whose buttons are drawn.
    pub fn selected(&self) -> Option<&PadView> {
        let key = self.selected.as_ref()?;
        self.pads.iter().find(|p| &p.controller.key == key)
    }

    pub fn last(&self) -> Option<&Last> {
        self.last.as_ref()
    }

    /// One event of one device of `controller`; `map` is that controller's layers now.
    pub fn feed(&mut self, controller: &Controller, ev: &PadEvent, map: Mapping, now: Instant) {
        let Some(pad) = self.pads.iter_mut().find(|p| p.controller.key == controller.key) else { return };
        match *ev {
            PadEvent::Button { button, pressed, .. } => {
                let i = at(button);
                if pressed {
                    let physical = map.physical(button);
                    pad.came_from[i] = Some(physical);
                    pad.held |= physical.bit();
                    pad.seen |= button.bit();
                    pad.unknown = None;
                    let last = Last { controller: pad.controller.clone(), physical, menus: map.menus(physical), game: map.game(physical) };
                    self.selected = Some(pad.controller.key.clone());
                    self.last = Some(last);
                } else {
                    let physical = pad.came_from[i].take().unwrap_or_else(|| map.physical(button));
                    pad.held &= !physical.bit();
                    pad.seen &= !button.bit();
                }
            }
            PadEvent::Axis { axis, value, .. } => {
                // A stick pushed out of its dead zone, or a trigger pulled past its point, picks this controller too
                // (once, as it happens: a stick that drifts does not keep taking the drawing).
                let was = pad.moving();
                match axis {
                    Axis::LeftX => pad.left.0 = value,
                    Axis::LeftY => pad.left.1 = value,
                    Axis::RightX => pad.right.0 = value,
                    Axis::RightY => pad.right.1 = value,
                    Axis::L2 | Axis::R2 => {
                        let t = &mut pad.trigger[usize::from(axis == Axis::R2)];
                        t.analog = true;
                        t.value = value.clamp(0.0, 1.0);
                        t.pressed = t.value >= if t.pressed { TRIGGER_RELEASE } else { TRIGGER_PRESS };
                    }
                }
                if !was && pad.moving() {
                    self.selected = Some(pad.controller.key.clone());
                }
            }
            PadEvent::Raw { ref text, .. } => pad.unknown = Some(text.clone()),
            PadEvent::Connected { .. } | PadEvent::Disconnected { .. } | PadEvent::Lid { .. } => return,
        }
        self.refresh_hold(now);
    }

    fn refresh_hold(&mut self, now: Instant) {
        let want = self.pads.iter().any(PadView::leaving);
        self.hold_since = match (want, self.hold_since) {
            (true, since) => since.or(Some(now)),
            (false, _) => None,
        };
    }

    /// How far the hold to leave has come, 0 to 1; `None` while nothing is held.
    pub fn hold(&self, now: Instant) -> Option<f32> {
        let since = self.hold_since?;
        Some((now.saturating_duration_since(since).as_secs_f32() / HOLD.as_secs_f32()).min(1.0))
    }

    pub fn leave_due(&self, now: Instant) -> bool {
        self.hold(now).is_some_and(|h| h >= 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pad::bindings::BUTTONS;

    fn pad(key: &str, name: &str) -> Controller {
        Controller { key: key.into(), name: name.into(), gamepad: true, node: String::new() }
    }

    fn plain() -> Mapping {
        Mapping { layout: Layout::default(), menu: ButtonLayout::Nintendo }
    }

    fn press(t: &mut ButtonTest, c: &Controller, button: Button, pressed: bool, map: Mapping, now: Instant) {
        t.feed(c, &PadEvent::Button { source: 1, button, pressed }, map, now);
    }

    #[test]
    fn the_trigger_marks_are_what_the_pad_reader_uses() {
        assert_eq!((TRIGGER_PRESS, TRIGGER_RELEASE), (crate::pad::decode::PRESS, crate::pad::decode::RELEASE));
    }

    #[test]
    fn a_button_lights_while_it_is_down_and_goes_out_on_release() {
        let c = pad("builtin", "Pocket");
        let now = Instant::now();
        let mut t = ButtonTest::new(&[c.clone()], now);
        press(&mut t, &c, Button::East, true, plain(), now);
        assert!(t.selected().unwrap().is_down(Button::East));
        assert!(!t.selected().unwrap().is_down(Button::South));
        press(&mut t, &c, Button::East, false, plain(), now);
        assert!(!t.selected().unwrap().is_down(Button::East));
        assert!(!t.selected().unwrap().busy());
    }

    #[test]
    fn every_physical_button_is_found_through_both_layers() {
        // Xbox menus and a player's swap of two buttons: whatever is pressed, the drawing lights that very button.
        let mut layout = Layout::default();
        layout.assign(Button::L2, Button::South);
        layout.assign(Button::Select, Button::East);
        for menu in ButtonLayout::ALL {
            let map = Mapping { layout, menu };
            let c = pad("bt:a", "Pad");
            let now = Instant::now();
            for physical in BUTTONS {
                let mut t = ButtonTest::new(&[c.clone()], now);
                // The event the App gets: both layers applied, as the pad threads do.
                let seen = menu.role(layout.role(physical));
                press(&mut t, &c, seen, true, map, now);
                assert!(t.selected().unwrap().is_down(physical), "{physical:?} lights ({menu:?})");
                let lit: Vec<_> = BUTTONS.iter().filter(|b| t.selected().unwrap().is_down(**b)).collect();
                assert_eq!(lit, vec![&physical], "only {physical:?} ({menu:?})");
                press(&mut t, &c, seen, false, map, now);
                assert!(!t.selected().unwrap().busy(), "{physical:?} let go ({menu:?})");
            }
        }
    }

    #[test]
    fn a_press_says_what_it_does_in_menus_and_in_games_after_the_players_remaps() {
        let c = pad("bt:a", "Pad");
        let now = Instant::now();
        let mut t = ButtonTest::new(&[c.clone()], now);
        // Nothing changed: the bottom button is B in the menus and the console's B.
        press(&mut t, &c, Button::South, true, plain(), now);
        let last = t.last().unwrap().clone();
        assert_eq!((menu_name(last.menus), console_name(last.game)), ("B", Some("B")));
        assert!(!last.remapped());
        // Xbox menus: the same button is A in the menus; a game keeps the console's letters.
        let xbox = Mapping { layout: Layout::default(), menu: ButtonLayout::Xbox };
        press(&mut t, &c, Button::South, false, plain(), now);
        press(&mut t, &c, xbox.menus(Button::South), true, xbox, now);
        let last = t.last().unwrap().clone();
        assert_eq!((last.physical, menu_name(last.menus), console_name(last.game)), (Button::South, "A", Some("B")));
        assert!(!last.remapped(), "the Button layout is a setting for every controller, not the player's remap");
        // The player made the right button act as the bottom one: pressing it is the console's B, B in the menus.
        let mut layout = Layout::default();
        layout.assign(Button::East, Button::South);
        let remapped = Mapping { layout, menu: ButtonLayout::Nintendo };
        press(&mut t, &c, xbox.menus(Button::South), false, xbox, now);
        press(&mut t, &c, remapped.menus(Button::East), true, remapped, now);
        let last = t.last().unwrap().clone();
        assert_eq!((last.physical, menu_name(last.menus), console_name(last.game)), (Button::East, "B", Some("B")));
        assert!(last.remapped());
        assert_eq!(last.game, Button::South);
    }

    #[test]
    fn a_release_after_the_layout_changed_lets_go_of_the_button_that_was_pressed() {
        let c = pad("bt:a", "Pad");
        let now = Instant::now();
        let mut t = ButtonTest::new(&[c.clone()], now);
        press(&mut t, &c, Button::L2, true, plain(), now);
        let mut layout = Layout::default();
        layout.assign(Button::L2, Button::South);
        // The release arrives as the role it was pressed as (L2), whatever the layout says now.
        press(&mut t, &c, Button::L2, false, Mapping { layout, menu: ButtonLayout::Nintendo }, now);
        assert!(!t.selected().unwrap().busy(), "no light stuck");
    }

    #[test]
    fn a_trigger_shows_its_travel_and_counts_as_pressed_from_55_to_below_40() {
        let c = pad("bt:pro2", "8BitDo Pro 2");
        let now = Instant::now();
        let mut t = ButtonTest::new(&[c.clone()], now);
        let pull = |t: &mut ButtonTest, v: f32| t.feed(&c, &PadEvent::Axis { source: 1, axis: Axis::R2, value: v }, plain(), now);
        pull(&mut t, 0.3);
        let r2 = t.selected().unwrap().trigger(false);
        assert!(r2.analog && !r2.pressed && percent(r2.value) == 30);
        pull(&mut t, 0.5);
        assert!(!t.selected().unwrap().trigger(false).pressed, "half way is not a press");
        pull(&mut t, 0.6);
        assert!(t.selected().unwrap().trigger(false).pressed && t.selected().unwrap().is_down(Button::R2));
        pull(&mut t, 0.45);
        assert!(t.selected().unwrap().trigger(false).pressed, "held until below 40 %");
        pull(&mut t, 0.35);
        assert!(!t.selected().unwrap().trigger(false).pressed);
        assert!(!t.selected().unwrap().trigger(true).analog, "the left trigger sent nothing");
    }

    #[test]
    fn sticks_show_where_they_are_and_whether_the_dead_zone_swallows_them() {
        let c = pad("builtin", "Pocket");
        let now = Instant::now();
        let mut t = ButtonTest::new(&[c.clone()], now);
        let push = |t: &mut ButtonTest, axis, value| t.feed(&c, &PadEvent::Axis { source: 1, axis, value }, plain(), now);
        // The Mini V2's left stick rests around +0.12.
        push(&mut t, Axis::LeftX, 0.12);
        assert_eq!(t.selected().unwrap().stick(true), (0.12, 0.0));
        assert!(!t.selected().unwrap().pushed(true) && !t.selected().unwrap().busy());
        push(&mut t, Axis::LeftX, 0.6);
        push(&mut t, Axis::LeftY, -0.4);
        assert!(t.selected().unwrap().pushed(true) && t.selected().unwrap().busy());
        assert!(!t.selected().unwrap().pushed(false), "the right stick did not move");
    }

    #[test]
    fn two_controllers_are_kept_apart_and_the_last_press_names_its_controller() {
        let (a, b) = (pad("builtin", "Pocket"), pad("bt:aa", "8BitDo SN30 Pro"));
        let now = Instant::now();
        let mut t = ButtonTest::new(&[a.clone(), b.clone()], now);
        assert_eq!(t.pads().len(), 2);
        assert_eq!(t.selected().unwrap().controller.key, "builtin");
        press(&mut t, &a, Button::North, true, plain(), now);
        press(&mut t, &b, Button::L1, true, plain(), now);
        assert_eq!(t.last().unwrap().controller.key, "bt:aa", "the last press came from the second controller");
        assert_eq!(t.selected().unwrap().controller.key, "bt:aa", "and that is the one drawn");
        assert!(t.selected().unwrap().is_down(Button::L1) && !t.selected().unwrap().is_down(Button::North));
        let first = t.pads().iter().find(|p| p.controller.key == "builtin").unwrap();
        assert!(first.is_down(Button::North) && !first.is_down(Button::L1), "each keeps its own");
        // A stick pushed on the first one takes the drawing back, once; its drift after that does not.
        t.feed(&a, &PadEvent::Axis { source: 1, axis: Axis::LeftX, value: 0.5 }, plain(), now);
        assert_eq!(t.selected().unwrap().controller.key, "builtin");
        press(&mut t, &b, Button::L1, false, plain(), now);
        press(&mut t, &b, Button::L1, true, plain(), now);
        assert_eq!(t.selected().unwrap().controller.key, "bt:aa");
        t.feed(&a, &PadEvent::Axis { source: 1, axis: Axis::LeftX, value: 0.6 }, plain(), now);
        assert_eq!(t.selected().unwrap().controller.key, "bt:aa", "a stick that was out of its dead zone already does not pick again");
        assert_eq!(t.last().unwrap().controller.key, "bt:aa", "and a stick is no press");
        // The second one goes away: its row and its lights go, the first stays as it was.
        t.sync(&[a.clone()], now);
        assert_eq!(t.pads().len(), 1);
        assert!(t.selected().unwrap().is_down(Button::North));
    }

    #[test]
    fn a_pad_that_is_not_a_player_is_not_listed_and_a_new_one_starts_at_rest() {
        let keys = Controller { key: "usb:headset".into(), name: "Headset".into(), gamepad: false, node: String::new() };
        let c = pad("bt:a", "Pad");
        let now = Instant::now();
        let mut t = ButtonTest::new(&[keys.clone()], now);
        assert!(t.pads().is_empty() && t.selected().is_none(), "nothing to show yet");
        t.feed(&keys, &PadEvent::Button { source: 1, button: Button::VolUp, pressed: true }, plain(), now);
        assert!(t.last().is_none(), "a headset's volume key is not a button of a pad");
        t.sync(&[keys, c.clone()], now);
        assert_eq!(t.pads().len(), 1);
        assert!(!t.selected().unwrap().busy());
    }

    #[test]
    fn home_held_for_two_seconds_leaves_and_a_short_press_does_not() {
        let c = pad("builtin", "Pocket");
        let t0 = Instant::now();
        let mut t = ButtonTest::new(&[c.clone()], t0);
        assert_eq!(t.hold(t0), None);
        press(&mut t, &c, Button::Mode, true, plain(), t0);
        assert!(t.selected().unwrap().is_down(Button::Mode), "HOME lights like any button");
        assert!(t.hold(t0 + Duration::from_secs(1)).is_some_and(|h| (h - 0.5).abs() < 0.01));
        assert!(!t.leave_due(t0 + Duration::from_millis(1900)));
        assert!(t.leave_due(t0 + Duration::from_millis(2000)));
        press(&mut t, &c, Button::Mode, false, plain(), t0 + Duration::from_secs(1));
        assert_eq!(t.hold(t0 + Duration::from_secs(3)), None, "let go in time: stay");
        assert!(!t.leave_due(t0 + Duration::from_secs(3)));
    }

    #[test]
    fn b_and_start_together_leave_but_either_alone_is_just_a_test() {
        let c = pad("builtin", "Pocket");
        let t0 = Instant::now();
        let mut t = ButtonTest::new(&[c.clone()], t0);
        press(&mut t, &c, Button::South, true, plain(), t0);
        assert_eq!(t.hold(t0 + Duration::from_secs(5)), None, "B alone");
        press(&mut t, &c, Button::Start, true, plain(), t0 + Duration::from_millis(300));
        assert!(!t.leave_due(t0 + Duration::from_millis(2200)), "the two seconds count from when both are down");
        assert!(t.leave_due(t0 + Duration::from_millis(2300)));
        press(&mut t, &c, Button::Start, false, plain(), t0 + Duration::from_millis(2300));
        assert_eq!(t.hold(t0 + Duration::from_secs(9)), None);
    }

    #[test]
    fn b_on_one_controller_and_start_on_another_do_not_leave() {
        let (a, b) = (pad("builtin", "Pocket"), pad("bt:a", "Pad"));
        let t0 = Instant::now();
        let mut t = ButtonTest::new(&[a.clone(), b.clone()], t0);
        press(&mut t, &a, Button::South, true, plain(), t0);
        press(&mut t, &b, Button::Start, true, plain(), t0);
        assert_eq!(t.hold(t0 + Duration::from_secs(5)), None, "two players pressing two buttons is not the way out");
    }

    #[test]
    fn a_controller_that_goes_away_while_home_is_down_cancels_the_hold() {
        let (a, b) = (pad("builtin", "Pocket"), pad("bt:a", "Pad"));
        let t0 = Instant::now();
        let mut t = ButtonTest::new(&[a.clone(), b.clone()], t0);
        press(&mut t, &b, Button::Mode, true, plain(), t0);
        assert!(t.hold(t0).is_some());
        t.sync(&[a], t0 + Duration::from_secs(1));
        assert_eq!(t.hold(t0 + Duration::from_secs(5)), None);
    }

    #[test]
    fn an_input_pocket_ui_does_not_know_is_kept_for_support() {
        let c = pad("bt:a", "Pad");
        let now = Instant::now();
        let mut t = ButtonTest::new(&[c.clone()], now);
        t.feed(&c, &PadEvent::Raw { source: 1, text: "Pad: BTN_C 1".into() }, plain(), now);
        assert_eq!(t.selected().unwrap().unknown.as_deref(), Some("Pad: BTN_C 1"));
        press(&mut t, &c, Button::South, true, plain(), now);
        assert_eq!(t.selected().unwrap().unknown, None, "the next press clears the note");
    }

    #[test]
    fn the_connection_is_read_from_the_identity() {
        assert_eq!(Kind::of(&pad("builtin", "Pocket")), Kind::BuiltIn);
        assert_eq!(Kind::of(&pad("bt:aa:bb", "Pad")), Kind::Bluetooth);
        assert_eq!(Kind::of(&pad("usb:045e:028e:usb-1", "Pad")), Kind::Usb);
    }

    /// The real reader's path: a pad whose triggers are axes (a Pro 2's BRAKE/GAS) and two pads at once, through the
    /// decoder, the layout and the tester.
    #[test]
    fn a_trigger_as_axis_pad_and_a_second_pad_through_the_real_decoder() {
        use crate::pad::decode::Decoder;
        use evdev::InputEvent;
        let (a, b) = (pad("builtin", "Pocket"), pad("bt:pro2", "8BitDo Pro 2"));
        let now = Instant::now();
        let mut t = ButtonTest::new(&[a.clone(), b.clone()], now);
        // Pro 2, D-input: BRAKE (code 10) is the left trigger, GAS (9) the right one, X/Y the left stick, Z/RZ the right.
        let mut pro2 = Decoder::with_caps(5, 0x2dc8, 0x6006, 0x100, "8BitDo Pro 2", (304..=318).collect(), vec![(0, 0, 255), (1, 0, 255), (2, 0, 255), (5, 0, 255), (9, 0, 1023), (10, 0, 1023), (16, -1, 1), (17, -1, 1)]);
        let mut pocket = Decoder::with_caps(3, 0x045e, 0x028e, 0x114, "Microsoft X-Box 360 pad", vec![304, 305, 307, 308], vec![(0, -32768, 32767), (1, -32768, 32767)]);
        let feed = |t: &mut ButtonTest, c: &Controller, events: Vec<PadEvent>| {
            for e in events {
                t.feed(c, &e, plain(), now);
            }
        };
        // Pull the right trigger of the Pro 2 half way, then through the threshold, then let go.
        feed(&mut t, &b, pro2.event(2, InputEvent::new(3, 9, 512)));
        assert!(t.selected().is_none_or(|p| !p.is_down(Button::R2)), "half way: no press yet");
        let p2 = t.pads().iter().find(|p| p.controller.key == "bt:pro2").unwrap();
        assert!(p2.trigger(false).analog && percent(p2.trigger(false).value) == 50 && !p2.trigger(false).pressed);
        feed(&mut t, &b, pro2.event(2, InputEvent::new(3, 9, 800)));
        assert_eq!(t.selected().unwrap().controller.key, "bt:pro2", "the button press of the trigger picks its pad");
        assert!(t.selected().unwrap().is_down(Button::R2) && t.selected().unwrap().trigger(false).pressed);
        assert_eq!(t.last().unwrap().physical, Button::R2);
        // The other pad presses a face button meanwhile: the Pro 2 keeps its trigger down.
        feed(&mut t, &a, pocket.event(3, InputEvent::new(1, 304, 1)));
        assert_eq!(t.last().unwrap().controller.key, "builtin");
        let p2 = t.pads().iter().find(|p| p.controller.key == "bt:pro2").unwrap();
        assert!(p2.is_down(Button::R2) && !p2.is_down(Button::South));
        feed(&mut t, &b, pro2.event(2, InputEvent::new(3, 9, 0)));
        let p2 = t.pads().iter().find(|p| p.controller.key == "bt:pro2").unwrap();
        assert!(!p2.is_down(Button::R2) && p2.trigger(false).value == 0.0);
    }
}
