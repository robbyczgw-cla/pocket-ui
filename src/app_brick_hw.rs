//! Part of the pocket-ui binary: the App's side of `pocket_ui::brick_hw`, the TrimUI Brick's own motor, lights and F keys
//! (docs/TRIMUI-BRICK-HW.md). Settings > Haptics (strength), > LEDs (`app_leds` does the page; this keeps the Battery look current),
//! > F1 button and > F2 button, and what the keys do on the shelf and over a game.

use std::time::{Duration, Instant};

use pocket_ui::brick_hw::{self, FKey, FKeys, Look};
use pocket_ui::leds::{self, Job, Mode, LEVELS};
use pocket_ui::pad::Button;

use super::App;

/// How often the Battery lights look at the battery again (it changes by the minute; a charger plugged in shows within this).
const BATTERY_EVERY: Duration = Duration::from_secs(30);

/// What the App remembers of the Brick's hardware.
pub(crate) struct State {
    pub(crate) keys: FKeys,
    pub(crate) slider: brick_hw::Slider,
    /// Each light's choices, which light the page shows, and whether the page is the "Each light" level.
    pub(crate) lights: brick_hw::Lights,
    pub(crate) light: usize,
    pub(crate) detail: bool,
    /// The look the Battery lights were last given, and when they look again.
    led_last: Option<Look>,
    led_next: Option<Instant>,
    /// The screen was dark at the last look: the lights went out (all but Battery & charging) and come back when it is on. None until
    /// the first look, which puts the choice on once: Off really off at start (the firmware's boot breathing stayed on otherwise).
    led_dark: Option<bool>,
}

impl State {
    /// A temporary output choice; the saved light preferences stay intact across screen off and wake.
    fn screen_choice(&mut self, saved: leds::Config, dark: bool) -> Option<leds::Config> {
        if saved.mode == Mode::Battery || Some(dark) == self.led_dark { return None; }
        self.led_dark = Some(dark);
        Some(if dark && !(saved.mode == Mode::Colour && self.lights.shows_battery()) {
            leds::Config { mode: Mode::Off, ..saved }
        } else { saved })
    }

    pub(crate) fn load(data_dir: &std::path::Path) -> State {
        let lights = brick_hw::Lights::load(data_dir);
        brick_hw::set_lights(lights);
        State { keys: FKeys::load(data_dir), slider: brick_hw::Slider::load(data_dir), lights, light: 0, detail: false, led_last: None, led_next: None, led_dark: None }
    }
}

/// The strength steps of Settings > Haptics, in the order the row goes round them.
const STRENGTH: [&str; 4] = ["Off", "Low", "Medium", "High"];

impl App {
    /// Settings > Haptics on the Brick: Off, Low, Medium, High. A tick shows the new strength.
    pub(crate) fn brick_haptics_step(&mut self, delta: i32) {
        let now = pocket_ui::rumble::menu_step() as i32;
        let next = (now + delta.signum()).rem_euclid(STRENGTH.len() as i32) as u8;
        pocket_ui::rumble::set_menu_step(&self.data_dir, next);
    }

    pub(crate) fn brick_haptics_value(&self, selected: bool) -> String {
        let v = STRENGTH[pocket_ui::rumble::menu_step() as usize % STRENGTH.len()];
        if selected { format!("‹ {v} ›") } else { v.to_string() }
    }

    /// Settings > F1 button / F2 button: the next (or previous) choice, saved at once. pocket-core reads it when the next game starts.
    pub(crate) fn brick_key_step(&mut self, n: u8, delta: i32) {
        let keys = &mut self.brick.keys;
        if n == 1 {
            keys.f1 = keys.f1.next(delta);
        } else {
            keys.f2 = keys.f2.next(delta);
        }
        keys.save(&self.data_dir);
    }

    /// F1 or F2 went down. Quick save, quick load and the screenshot are pocket-core's (`hotkeys`); Recent games is ours: the
    /// switcher, over the game (as HOME + SELECT opens it) or on the shelf.
    pub(crate) fn brick_fkey(&mut self, b: Button, in_game: bool) {
        let n = if b == Button::F1 { 1 } else { 2 };
        let key = self.brick.keys.of(n);
        if self.switcher.is_some() || !matches!(key, FKey::Recent | FKey::Pause) {
            return;
        }
        if key == FKey::Pause {
            if in_game && self.menu.is_none() {
                self.open_menu();
            }
            return;
        }
        if in_game {
            if self.menu.is_none() {
                self.open_menu();
                if self.menu.is_some() {
                    self.open_switcher(true, false);
                }
            }
        } else if self.settings_page.is_none() {
            self.open_switcher(false, false);
        }
    }

    /// The Battery lights follow the charger and the level: looked at again every half minute, written only when the look changes.
    /// The time of the next look, for the loop's wake-up; None when the lights are not on Battery.
    pub(crate) fn brick_leds_tick(&mut self, now: Instant) -> Option<Instant> {
        if !pocket_ui::brick_profile::board().is_some_and(|b| b.leds()) {
            return None;
        }
        let dark = self.sleep.state() != pocket_ui::sleep::State::Awake;
        let colour = self.leds.mode == Mode::Colour;
        let lights = self.brick.lights;
        // A dark screen (Light, Deep, powering off) puts the lights out, but for what shows the battery (the whole page on Battery &
        // charging, or the lights set to it under Each light); awake again, the choice comes back.
        if let Some(choice) = self.brick.screen_choice(self.leds, dark) {
            if dark && colour && lights.shows_battery() {
                println!("lights: the screen is dark, only the battery lights stay on");
                brick_hw::set_lights(lights.battery_only());
            } else if !dark {
                println!("lights: the screen is on, the choice is back");
                brick_hw::set_lights(lights);
            } else {
                println!("lights: the screen is dark, all out (effects, scale and the raw picture)");
            }
            leds::apply_in_background(Job::Choice(choice));
        }
        // The colour cycle turns while a light is on it and the screen is on.
        brick_hw::set_rainbow(colour && !dark && lights.cycling(), std::path::Path::new("/"));
        let st = &mut self.brick;
        let battery_lights = colour && lights.shows_battery();
        if self.leds.mode != Mode::Battery && !battery_lights {
            st.led_last = None;
            st.led_next = None;
            return None;
        }
        if let Some(t) = st.led_next.filter(|t| now < *t) {
            return Some(t);
        }
        let sample = pocket_ui::battery::read(&pocket_ui::battery::dir(), 0);
        let look = brick_hw::battery_look(sample.as_ref(), LEVELS[self.leds.level.min(LEVELS.len() - 1)]);
        brick_hw::set_battery_now(Some(look));
        if st.led_last != Some(look) {
            st.led_last = Some(look);
            leds::apply_in_background(Job::Choice(self.leds));
        }
        let next = now + BATTERY_EVERY;
        st.led_next = Some(next);
        Some(next)
    }
}

#[cfg(test)]
mod standby_tests {
    use super::*;

    #[test]
    fn standby_keeps_default_lights_dark_and_restores_the_players_choice() {
        for mode in Mode::ALL {
            for battery_zone in [false, true] {
                let dir = tempfile::tempdir().unwrap();
                let mut state = State::load(dir.path());
                if battery_zone { state.lights.zones[0].glow = brick_hw::Glow::Battery; }
                let saved = leds::Config { mode, ..leds::Config::default() };
                let before = state.lights;
                let dark = state.screen_choice(saved, true);
                if mode == Mode::Battery {
                    assert_eq!(dark, None, "the chosen battery indication stays active");
                } else {
                    let dark = dark.expect("screen-off updates the light output");
                    let expected = if mode == Mode::Colour && battery_zone { mode } else { Mode::Off };
                    assert_eq!(dark.mode, expected, "default and colour lights go dark; chosen battery zones stay active");
                    assert!(state.screen_choice(saved, true).is_none(), "no repeated output while dark");
                    assert_eq!(state.screen_choice(saved, false), Some(saved), "wake restores the full saved choice");
                }
                assert_eq!(state.lights, before, "no per-light choice is lost");
            }
        }
    }
}
