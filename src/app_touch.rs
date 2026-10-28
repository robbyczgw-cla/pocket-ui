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
        } else if self.setup.is_some() {
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
        if let Some(why) = self.touch_blocked() {
            return self.touch_ignored(why);
        }
        self.ignore_log.accepted();
        let Some(window) = &self.window else { return };
        let size = window.inner_size();
        let (x, y) = touch::to_px((ev.x, ev.y), size.width, size.height);
        let gesture = self.touch_rec.feed(ev.phase, x, y, std::time::Instant::now(), self.hits.pitch());
        // The search: keys, result rows and its list's swipe (`app_search`); its hints go the usual way below.
        if self.find.is_open() && self.search_touch(el, gesture) {
            return;
        }
        // The cabinet: a tap pulls out the case under it (and plays the one already out), a swipe moves along
        // the shelf sideways and between the shelves up and down.
        if self.theme == pocket_ui::settings::Theme::Cabinet {
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
        if let Some(Gesture::Tap { y, .. }) = gesture.filter(|_| self.card.is_some() && self.curator.page.is_none() && !self.gallery_over_card()) {
            match self.curator_card_tap(y) {
                Some(true) => return self.tap_button(el, CONFIRM),
                Some(false) => return self.request_redraw(),
                None => {}
            }
        }
        match gesture {
            Some(Gesture::Tap { x, y }) => match self.hits.tap(x, y) {
                TapAction::Nothing => {}
                TapAction::Activate => self.tap_button(el, CONFIRM),
                TapAction::Press(b) => self.tap_button(el, b),
                // A tab two steps away (History's Year from Week): both steps in this tap.
                TapAction::Presses(b, n) => (0..n).for_each(|_| self.tap_button(el, b)),
                TapAction::Steps(n) => self.steps(el, n),
            },
            // The list follows the finger: a swipe up shows what is further down, so the selection goes down.
            Some(Gesture::Scroll { steps, .. }) => self.steps(el, self.hits.clamp_steps(-steps)),
            None => {}
        }
    }

    /// A tap or a swipe on the quick settings or the power menu: their rows, the arrows of a row that changes with
    /// Left/Right, their hints. The panel is pocket-ui's window, so a paused game under it never sees the finger.
    fn sys_touch(&mut self, el: &ActiveEventLoop, ev: TouchEvent) {
        if !self.sleep.touched() || self.sys.hits.is_empty() {
            self.touch_rec.reset();
            return;
        }
        let Some(window) = &self.window else { return };
        let size = window.inner_size();
        let (x, y) = touch::to_px((ev.x, ev.y), size.width, size.height);
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
