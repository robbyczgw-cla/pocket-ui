//! Part of the pocket-ui binary: what the touch screen does on the shelf, the game lists, Settings and the
//! game switcher. The logic (mapping, taps and swipes, what a tap on a row means) is `pocket_ui::touch`;
//! this only decides when it applies and turns it into the button presses the menus already understand,
//! so touch can't do anything a button can't.
//!
//! A game owns the screen while it runs: pocket-core reads the touch screen itself then (the Nintendo DS's
//! stylus), and nothing here acts. Touching still counts as activity for the idle timer, and never wakes
//! the screen.

use pocket_ui::pad::{Button, PadEvent};
use pocket_ui::touch::{self, Gesture, TapAction, TouchEvent};
use winit::application::ApplicationHandler;
use winit::event_loop::ActiveEventLoop;

use super::app_where::Place;
use super::{App, UserEvent, CONFIRM};

impl App {
    /// Why the menus on screen are not ours to touch, if they are not: a game, a launch fading out, the pause menu or
    /// quick settings over them, or the last frame left nothing to touch.
    fn touch_blocked(&self) -> Option<touch::Ignored> {
        if self.running.is_some() {
            Some(touch::Ignored::Game)
        } else if self.launch_at.is_some() {
            Some(touch::Ignored::Launching)
        } else if self.menu.is_some() {
            Some(touch::Ignored::PauseMenu)
        } else if self.setup.as_ref().is_some_and(|s| !s.takes_touch()) {
            // The first start's other steps are buttons only; Add games takes touch (`Setup::register_touch`).
            Some(touch::Ignored::Setup)
        } else if self.sys.is_open() {
            Some(touch::Ignored::SystemPanel)
        } else if self.hits.is_empty() {
            Some(touch::Ignored::NothingToTouch)
        } else {
            None
        }
    }

    /// A touch that did nothing on purpose: one log line for the first of a spell (`touch::IgnoreLog`), so that a report
    /// of "touch didn't work" shows whether the touch screen was read and why the touch was dropped.
    fn touch_ignored(&mut self, why: touch::Ignored) {
        self.touch_rec.reset();
        if let Some(line) = self.ignore_log.ignored(why) {
            println!("{line}");
        }
    }

    pub(crate) fn touch_event(&mut self, el: &ActiveEventLoop, ev: TouchEvent) {
        // The picture is on a TV and the panel is dark: a touch there would tap blindly (`app_display_out`).
        if self.on_tv() {
            return self.touch_ignored(touch::Ignored::TvOutput);
        }
        // Quick settings and the power menu (also over a paused game) have their own map (`Sys::hits`).
        if self.sys.quick_open() || self.sys.power_menu_open() {
            self.ignore_log.accepted();
            self.sys_touch(el, ev);
            return;
        }
        // Activity for the idle timer, also while a game is played by touch; asleep, a touch does nothing.
        if !self.sleep.touched() {
            return self.touch_ignored(touch::Ignored::Asleep);
        }
        // Kids Mode: a tap on the shelf or the pause menu; its pages and a game keep their own (`app_kids`).
        if self.kids_live() {
            return self.kids_touch(ev);
        }
        // The Big look's Continue, Stop or More.
        if self.big_pause() {
            return self.big_pause_touch(ev);
        }
        if let Some(why) = self.touch_blocked() {
            return self.touch_ignored(why);
        }
        self.ignore_log.accepted();
        let (w, h) = self.screen_size();
        let (x, y) = touch::to_px((ev.x, ev.y), w, h);
        let gesture = self.touch_rec.feed(ev.phase, x, y, std::time::Instant::now(), self.hits.pitch());
        // The page in front takes the finger (`app_where`, the page `paint` painted): never one under it.
        let place = self.where_now().place;
        // This overlay owns its hit map; search, a cabinet or the game card underneath must never take its tap.
        if place == Place::ControlsCard {
            if let Some(Gesture::Tap { x, y }) = gesture {
                if let TapAction::Press(b) = self.hits.tap(x, y) {
                    self.tap_button(el, b);
                }
            }
            return;
        }
        // Add games: a tap on a way selects it, a tap on the selected one does it; its hints are buttons. (The manual opened
        // over it with SELECT is a page of its own: its links and its swipe, not the ways behind it.)
        if matches!(place, Place::Setup { .. }) {
            if let Some(Gesture::Tap { x, y }) = gesture {
                match self.hits.tap(x, y) {
                    TapAction::Press(b) => self.tap_button(el, b),
                    TapAction::Activate if self.setup.as_ref().is_some_and(|s| s.ways_focused()) => self.tap_button(el, CONFIRM),
                    action @ (TapAction::Activate | TapAction::Steps(_)) => {
                        let n = if let TapAction::Steps(n) = action { n } else { 0 };
                        if let Some(s) = self.setup.as_mut() {
                            s.ways_steps(n);
                        }
                        self.request_redraw();
                    }
                    _ => {}
                }
            }
            return;
        }
        // The search: keys, result rows and its list's swipe (`app_search`); its hints go the usual way below.
        if place == Place::Search && self.search_touch(el, gesture) {
            return;
        }
        // The cabinet: a tap pulls out the case under it (and plays the one already out), a swipe moves along
        // the shelf sideways and between the shelves up and down.
        if matches!(place, Place::Shelf(_)) && self.cabinet_in_front() {
            match gesture {
                Some(Gesture::Tap { x, y }) => match self.cabinet_tap(x, y) {
                    Some(true) => return self.tap_button(el, CONFIRM),
                    Some(false) => return self.request_redraw(),
                    None => {}
                },
                Some(Gesture::Scroll { steps, vertical }) => {
                    let button = match (vertical, steps < 0) {
                        (true, true) => Button::Down,
                        (true, false) => Button::Up,
                        (false, true) => Button::Right,
                        (false, false) => Button::Left,
                    };
                    for _ in 0..steps.unsigned_abs().min(60) {
                        self.tap_button(el, button);
                    }
                    return;
                }
                None => {}
            }
        }
        // The card's Curator rows: a tap selects one, a tap on the selected one opens it (`app_curator`).
        if let Some(Gesture::Tap { y, .. }) = gesture.filter(|_| matches!(place, Place::Card(_))) {
            match self.curator_card_tap(y) {
                Some(true) => return self.tap_button(el, CONFIRM),
                Some(false) => return self.request_redraw(),
                None => {}
            }
        }
        // The manual: a tap on a link to another page opens it.
        if let Some(Gesture::Tap { x, y }) = gesture.filter(|_| matches!(place, Place::Settings { page: crate::SettingsPage::Manual, .. })) {
            if self.manual.tap_link(x, y) {
                return self.request_redraw();
            }
        }
        match gesture {
            Some(Gesture::Tap { x, y }) => match self.hits.tap(x, y) {
                TapAction::Nothing if self.big_live() => self.tap_button(el, super::app_big_shelf::tap_button(x, w)),
                TapAction::Nothing => {}
                TapAction::Activate => self.tap_button(el, CONFIRM),
                TapAction::Press(b) => self.tap_button(el, b),
                // A tab two steps away (History's Year from Week): both steps in this tap.
                TapAction::Presses(b, n) => (0..n).for_each(|_| self.tap_button(el, b)),
                TapAction::Steps(n) => self.steps(el, n),
            },
            // On the Big shelf the case follows the finger sideways.
            Some(Gesture::Scroll { steps, vertical: false }) if self.big_live() => {
                let button = if steps < 0 { Button::Right } else { Button::Left };
                (0..steps.unsigned_abs().min(60)).for_each(|_| self.tap_button(el, button));
            }
            // The list follows the finger: a swipe up shows what is further down, so the selection goes down.
            Some(Gesture::Scroll { steps, .. }) => self.steps(el, self.hits.clamp_steps(-steps)),
            None => {}
        }
    }

    /// The window's pointer moved (winit pixels of the window's inside). Only a drag counts, as a finger would.
    pub(crate) fn mouse_moved(&mut self, el: &ActiveEventLoop, pos: (f64, f64), size: (u32, u32)) {
        self.cursor = Some(pos);
        if self.mouse_down {
            if let Some(ev) = self.mouse_touch(touch::Phase::Move, size) {
                self.touch_event(el, ev);
            }
        }
    }

    /// The pointer's left button: a click is a tap where it is, a drag the swipe it makes — the same rules as a
    /// finger (`touch_event`), so a trackpad can do nothing a touch couldn't.
    pub(crate) fn mouse_button(&mut self, el: &ActiveEventLoop, pressed: bool, size: (u32, u32)) {
        self.mouse_down = pressed;
        let phase = if pressed { touch::Phase::Down } else { touch::Phase::Up };
        if let Some(ev) = self.mouse_touch(phase, size) {
            self.touch_event(el, ev);
        }
    }

    fn mouse_touch(&self, phase: touch::Phase, size: (u32, u32)) -> Option<TouchEvent> {
        let (x, y) = self.cursor?;
        if size.0 == 0 || size.1 == 0 {
            return None;
        }
        Some(TouchEvent { phase, x: (x / size.0 as f64).clamp(0.0, 0.999) as f32, y: (y / size.1 as f64).clamp(0.0, 0.999) as f32 })
    }

    /// The wheel (a circling trackpad, the left pad of the shipped layout): the selection follows it, as far as
    /// this page's rows reach; a game, the pause menu and the other blocked screens keep theirs (`touch_blocked`).
    pub(crate) fn mouse_wheel(&mut self, el: &ActiveEventLoop, lines: f32) {
        let n = touch::wheel_steps(lines, &mut self.wheel_carry);
        if n == 0 || self.touch_blocked().is_some() || self.sys.quick_open() || self.sys.power_menu_open() {
            return;
        }
        if !self.sleep.touched() {
            return;
        }
        let n = self.hits.clamp_steps(n);
        if n != 0 {
            self.steps(el, n);
        }
    }

    /// A tap or a swipe on the quick settings or the power menu: their rows, the arrows of a row that changes with
    /// Left/Right, their hints. The panel is pocket-ui's window, so a paused game under it never sees the finger.
    fn sys_touch(&mut self, el: &ActiveEventLoop, ev: TouchEvent) {
        if !self.sleep.touched() || self.sys.hits.is_empty() {
            self.touch_rec.reset();
            return;
        }
        let (w, h) = self.screen_size();
        let (x, y) = touch::to_px((ev.x, ev.y), w, h);
        match self.touch_rec.feed(ev.phase, x, y, std::time::Instant::now(), self.sys.hits.pitch()) {
            Some(Gesture::Tap { x, y }) => match self.sys.hits.tap(x, y) {
                TapAction::Nothing => {}
                TapAction::Activate => self.tap_button(el, CONFIRM),
                TapAction::Press(b) => self.tap_button(el, b),
                TapAction::Presses(b, n) => (0..n).for_each(|_| self.tap_button(el, b)),
                TapAction::Steps(n) => self.steps(el, n),
            },
            Some(Gesture::Scroll { steps, .. }) => self.steps(el, self.sys.hits.clamp_steps(-steps)),
            None => {}
        }
    }

    /// A button pressed and let go, as the command FIFO does it: it goes the way a real press goes.
    pub(crate) fn tap_button(&mut self, el: &ActiveEventLoop, button: Button) {
        for pressed in [true, false] {
            self.user_event(el, UserEvent::Pad(PadEvent::Button { source: usize::MAX, button, pressed }));
        }
    }

    /// The selection moves by `n` rows (negative: up).
    fn steps(&mut self, el: &ActiveEventLoop, n: i32) {
        let button = if n < 0 { Button::Up } else { Button::Down };
        for _ in 0..n.unsigned_abs().min(60) {
            self.tap_button(el, button);
        }
    }
}
