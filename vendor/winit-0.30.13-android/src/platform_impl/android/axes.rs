//! pocket-ui patch: what a gamepad's axes say, kept per device. The D-pad (hat) and the left stick become arrow-key edges, the
//! analog L2/R2 become button edges; each pad has its own state, so a second pad's resting stick (noise around zero) never
//! lets go of a direction the first pad holds (review of 12.11.2026, #9). No Android types: pocket-ui's tests include this file
//! as it is (`src/pad_axes_tests.rs`).

use std::collections::HashMap;

/// A direction of one axis as -1, 0 or 1: the hat when it is past a half, else the stick.
pub fn direction(hat: f32, stick: f32) -> i8 {
    let v = if hat.abs() > 0.5 { hat } else { stick };
    if v > 0.5 {
        1
    } else if v < -0.5 {
        -1
    } else {
        0
    }
}

/// An analog trigger as pressed: down past 0.75, up again under 0.5 (`pocket_ui::pad::android::Trigger`).
pub fn latch(down: bool, v: f32) -> bool {
    if down {
        v >= 0.5
    } else {
        v > 0.75
    }
}

/// What one motion event changed on its device: the state before and after.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct Step {
    pub joystick: ((i8, i8), (i8, i8)),
    pub triggers: ((bool, bool), (bool, bool)),
}

/// Every pad's last state, by Android's device id.
#[derive(Default, Debug)]
pub struct Axes {
    joystick: HashMap<i32, (i8, i8)>,
    triggers: HashMap<i32, (bool, bool)>,
}

impl Axes {
    /// A motion event of `device`: hat (x, y), left stick (x, y), left and right trigger.
    pub fn step(&mut self, device: i32, hat: (f32, f32), stick: (f32, f32), triggers: (f32, f32)) -> Step {
        let before_joystick = self.joystick.get(&device).copied().unwrap_or_default();
        let before_triggers = self.triggers.get(&device).copied().unwrap_or_default();
        let joystick = (direction(hat.0, stick.0), direction(hat.1, stick.1));
        let pressed = (latch(before_triggers.0, triggers.0), latch(before_triggers.1, triggers.1));
        self.joystick.insert(device, joystick);
        self.triggers.insert(device, pressed);
        Step { joystick: (before_joystick, joystick), triggers: (before_triggers, pressed) }
    }
}
