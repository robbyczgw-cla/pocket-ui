//! The axes of Android's pads, per device (review of 12.11.2026, #9): the file the patched winit uses
//! (`vendor/winit-0.30.13-android/.../axes.rs`) is included as it is, and two simulated pads are driven through it. Where a second
//! physical pad can't be had, this is the same input the device gets from a second virtual pad (`vpad`).

#[path = "../vendor/winit-0.30.13-android/src/platform_impl/android/axes.rs"]
mod axes;

use axes::Axes;

const PAD: i32 = 7;
const SECOND: i32 = 12;

fn idle() -> ((f32, f32), (f32, f32), (f32, f32)) {
    ((0.0, 0.0), (0.0, 0.0), (0.0, 0.0))
}

/// The built-in pad holds Left on its hat; a second pad's resting stick jitters around zero and a hair either way. The first pad's
/// Left stays held: with one shared state the second pad's event let it go (a key release in the middle of a held direction).
#[test]
fn a_second_pad_at_rest_never_lets_go_of_the_first_pad_s_direction() {
    let mut axes = Axes::default();
    let held = axes.step(PAD, (-1.0, 0.0), (0.0, 0.0), (0.0, 0.0));
    assert_eq!(held.joystick, ((0, 0), (-1, 0)), "Left goes down");
    for noise in [0.0f32, 0.05, -0.08, 0.2, -0.3, 0.0] {
        let step = axes.step(SECOND, (0.0, 0.0), (noise, -noise), (0.0, 0.0));
        assert_eq!(step.joystick, ((0, 0), (0, 0)), "the second pad at rest ({noise}) changes nothing");
    }
    // The first pad's next event still knows Left is held: no new press, and its release is a release.
    let again = axes.step(PAD, (-1.0, 0.0), (0.0, 0.0), (0.0, 0.0));
    assert_eq!(again.joystick, ((-1, 0), (-1, 0)));
    let up = axes.step(PAD, (0.0, 0.0), (0.0, 0.0), (0.0, 0.0));
    assert_eq!(up.joystick, ((-1, 0), (0, 0)), "Left is let go by the pad that held it");
}

/// The same for the analog triggers: L2 held on one pad, the other pad's idle triggers never release it.
#[test]
fn a_second_pad_s_idle_triggers_never_release_the_first_pad_s_l2() {
    let mut axes = Axes::default();
    let down = axes.step(PAD, (0.0, 0.0), (0.0, 0.0), (0.9, 0.0));
    assert_eq!(down.triggers, ((false, false), (true, false)), "L2 goes down");
    let (hat, stick, _) = idle();
    let other = axes.step(SECOND, hat, stick, (0.0, 0.0));
    assert_eq!(other.triggers, ((false, false), (false, false)));
    // Held between half and three quarters: still down, the dead band of the latch.
    let held = axes.step(PAD, (0.0, 0.0), (0.0, 0.0), (0.6, 0.0));
    assert_eq!(held.triggers, ((true, false), (true, false)));
    let up = axes.step(PAD, (0.0, 0.0), (0.0, 0.0), (0.2, 0.0));
    assert_eq!(up.triggers, ((true, false), (false, false)));
}

/// Two pads each press their own directions: each is its own press and release, as two people playing.
#[test]
fn each_pad_has_its_own_edges() {
    let mut axes = Axes::default();
    assert_eq!(axes.step(PAD, (1.0, 0.0), (0.0, 0.0), (0.0, 0.0)).joystick, ((0, 0), (1, 0)));
    assert_eq!(axes.step(SECOND, (0.0, 0.0), (0.0, 1.0), (0.0, 0.0)).joystick, ((0, 0), (0, 1)));
    assert_eq!(axes.step(PAD, (0.0, 0.0), (0.0, 0.0), (0.0, 0.0)).joystick, ((1, 0), (0, 0)));
    assert_eq!(axes.step(SECOND, (0.0, 0.0), (0.0, 0.0), (0.0, 0.0)).joystick, ((0, 1), (0, 0)));
}

/// The hat wins over the stick past a half, and the stick counts where the hat is idle (a pad has one or the other).
#[test]
fn the_hat_beats_the_stick_and_the_stick_stands_in_for_it() {
    assert_eq!(axes::direction(1.0, -1.0), 1);
    assert_eq!(axes::direction(0.0, -0.9), -1);
    assert_eq!(axes::direction(0.3, 0.4), 0);
}
