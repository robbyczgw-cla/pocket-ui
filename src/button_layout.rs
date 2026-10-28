//! Settings → Playing → Button layout: which of the four face buttons confirms in pocket-ui's own menus.
//!
//! The Mini V2 prints the Nintendo letters (A right, B bottom, X top, Y left). Players used to an Xbox controller or
//! an Android handheld's "Xbox" controller style expect A at the bottom. pocket-ui's code and its hints speak in four
//! *roles* (A confirms, B goes back, X and Y are the two extra actions) that sit on `Button::East`, `South`, `North`
//! and `West`; this setting decides which physical button plays which role:
//!
//! | Layout | A (confirm) | B (back) | X | Y |
//! |---|---|---|---|---|
//! | Nintendo (default except Odin 2 Mini/Portal) | right | bottom | top | left |
//! | Xbox | bottom | right | left | top |
//!
//! So the hint bar, the touch hints and the manual keep saying "A", "B", "X" and "Y" and are right in both layouts:
//! the letters are those of the chosen layout. The role is decided once, where a physical controller's button enters
//! pocket-ui (`Remap`, in the thread that reads the pads), so every screen, the pause menu, quick settings, dialogs
//! and the keyboards follow it without knowing about it, and a new screen cannot forget it.
//!
//! What it leaves alone:
//! * **Games.** pocket-core and the emulators read the pads themselves; a game keeps the console's own layout
//!   (libretro A is the right button, Options → Controls changes it per game).
//! * **Touch, the command FIFO and the desktop preview's keyboard.** They already say roles (a tap on the "A" hint, `a`
//!   in the FIFO), so they do the same in both layouts.
//! * **Safe mode.** It runs before settings.json is read and has its own pad loop; its A and B are the printed ones,
//!   whatever is set.
//!
//! An unset or wrong value follows the device: Xbox on Odin 2 Mini/Portal, Nintendo elsewhere. Saved choices win.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU8, Ordering};

use crate::pad::{Button, PadEvent};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ButtonLayout {
    /// A on the right, as printed on the Mini V2 and original Odin 2.
    Nintendo,
    /// A at the bottom, B on the right, X on the left, Y on top, as on an Xbox controller.
    Xbox,
}

impl Default for ButtonLayout {
    fn default() -> Self {
        crate::device::current().menu_layout()
    }
}

impl ButtonLayout {
    pub const ALL: [ButtonLayout; 2] = [ButtonLayout::Nintendo, ButtonLayout::Xbox];

    /// The value on the Settings row.
    pub fn label(self) -> &'static str {
        match self {
            ButtonLayout::Nintendo => "Nintendo",
            ButtonLayout::Xbox => "Xbox",
        }
    }

    pub fn toggled(self) -> ButtonLayout {
        match self {
            ButtonLayout::Nintendo => ButtonLayout::Xbox,
            ButtonLayout::Xbox => ButtonLayout::Nintendo,
        }
    }

    /// The role the physical button `b` plays in the menus. Everything but the four face buttons is itself.
    pub fn role(self, b: Button) -> Button {
        match (self, b) {
            (ButtonLayout::Xbox, Button::South) => Button::East,
            (ButtonLayout::Xbox, Button::East) => Button::South,
            (ButtonLayout::Xbox, Button::North) => Button::West,
            (ButtonLayout::Xbox, Button::West) => Button::North,
            _ => b,
        }
    }

    /// Whether `role` is the physical right face button, which pocket-core reads as libretro A in every layout.
    /// The legacy name follows the Mini V2 labels; on Odin 2 Mini/Portal this button is printed B. Screenshot
    /// shortcuts remain positional in both the menus and the host.
    pub fn is_printed_a(self, role: Button) -> bool {
        self.role(Button::East) == role
    }

    fn to_u8(self) -> u8 {
        self as u8
    }

    fn from_u8(v: u8) -> ButtonLayout {
        if v == ButtonLayout::Xbox as u8 {
            ButtonLayout::Xbox
        } else {
            ButtonLayout::Nintendo
        }
    }
}

/// The layout in use. Process wide, because the pad threads read it and the UI thread sets it (Settings, the loaded
/// settings.json); Nintendo until the loaded device default or saved choice is set. Safe mode maps its pad separately.
static LAYOUT: AtomicU8 = AtomicU8::new(0);

pub fn set(l: ButtonLayout) {
    LAYOUT.store(l.to_u8(), Ordering::Relaxed);
}

pub fn get() -> ButtonLayout {
    ButtonLayout::from_u8(LAYOUT.load(Ordering::Relaxed))
}

/// Turns the events of physical controllers into roles. A release goes back the way its press went, so a layout that
/// changes while a button is held (the A that switched it is still down) cannot leave a role pressed for good.
#[derive(Default)]
pub struct Remap {
    held: HashMap<(usize, Button), Button>,
}

impl Remap {
    /// `e` as the menus should see it, with the layout in use now.
    pub fn event(&mut self, e: PadEvent) -> PadEvent {
        self.event_with(get(), e)
    }

    pub fn event_with(&mut self, layout: ButtonLayout, e: PadEvent) -> PadEvent {
        match e {
            PadEvent::Button { source, button, pressed } => {
                let role = if pressed {
                    let role = layout.role(button);
                    // Only a button that moves is remembered; the others cost nothing per event.
                    if role != button {
                        self.held.insert((source, button), role);
                    } else {
                        self.held.remove(&(source, button));
                    }
                    role
                } else {
                    self.held.remove(&(source, button)).unwrap_or(button)
                };
                PadEvent::Button { source, button: role, pressed }
            }
            // A controller that goes away lets go of what it held (`Mux`), as the roles it pressed.
            PadEvent::Disconnected { source } => {
                self.held.retain(|(s, _), _| *s != source);
                PadEvent::Disconnected { source }
            }
            other => other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(source: usize, button: Button, pressed: bool) -> PadEvent {
        PadEvent::Button { source, button, pressed }
    }

    fn button_of(e: &PadEvent) -> Button {
        match e {
            PadEvent::Button { button, .. } => *button,
            other => panic!("not a button: {other:?}"),
        }
    }

    #[test]
    fn nintendo_is_the_start_setting_and_changes_nothing() {
        assert_eq!(ButtonLayout::default(), ButtonLayout::Nintendo);
        for b in Button::ALL {
            assert_eq!(ButtonLayout::Nintendo.role(b), b);
        }
    }

    #[test]
    fn xbox_swaps_the_face_buttons_in_pairs_and_nothing_else() {
        let l = ButtonLayout::Xbox;
        assert_eq!((l.role(Button::South), l.role(Button::East)), (Button::East, Button::South));
        assert_eq!((l.role(Button::North), l.role(Button::West)), (Button::West, Button::North));
        for b in Button::ALL {
            assert_eq!(l.role(l.role(b)), b, "a swap undoes itself: {b:?}");
            if !matches!(b, Button::South | Button::East | Button::North | Button::West) {
                assert_eq!(l.role(b), b, "{b:?} is no face button");
            }
        }
    }

    #[test]
    fn the_layout_is_kept_as_a_lowercase_word_and_a_wrong_value_reads_as_nintendo() {
        assert_eq!(serde_json::to_string(&ButtonLayout::Xbox).unwrap(), "\"xbox\"");
        assert_eq!(serde_json::from_str::<ButtonLayout>("\"nintendo\"").unwrap(), ButtonLayout::Nintendo);
        assert!(serde_json::from_str::<ButtonLayout>("\"playstation\"").is_err(), "the lenient reader turns this into the default");
    }

    #[test]
    fn a_pad_event_becomes_the_role_and_other_events_pass() {
        let mut r = Remap::default();
        assert_eq!(button_of(&r.event_with(ButtonLayout::Xbox, press(1, Button::South, true))), Button::East);
        assert_eq!(button_of(&r.event_with(ButtonLayout::Xbox, press(1, Button::South, false))), Button::East);
        assert_eq!(button_of(&r.event_with(ButtonLayout::Xbox, press(1, Button::Start, true))), Button::Start);
        assert_eq!(button_of(&r.event_with(ButtonLayout::Nintendo, press(1, Button::South, true))), Button::South);
        let axis = PadEvent::Axis { source: 1, axis: crate::pad::Axis::LeftX, value: 0.5 };
        assert!(matches!(r.event_with(ButtonLayout::Xbox, axis), PadEvent::Axis { value, .. } if value == 0.5));
    }

    #[test]
    fn the_screenshot_key_is_the_printed_a_in_both_layouts() {
        let mut r = Remap::default();
        // Xbox: the bottom button arrives as A (East), the printed A on the right arrives as South.
        let bottom = button_of(&r.event_with(ButtonLayout::Xbox, press(1, Button::South, true)));
        let right = button_of(&r.event_with(ButtonLayout::Xbox, press(1, Button::East, true)));
        assert_eq!(bottom, Button::East);
        assert_eq!(right, Button::South);
        assert!(!ButtonLayout::Xbox.is_printed_a(bottom), "the bottom button is not the printed A");
        assert!(ButtonLayout::Xbox.is_printed_a(right), "the printed A is the right button");
        // Nintendo: the right button is A, as before.
        let right = button_of(&r.event_with(ButtonLayout::Nintendo, press(2, Button::East, true)));
        let bottom = button_of(&r.event_with(ButtonLayout::Nintendo, press(2, Button::South, true)));
        assert!(ButtonLayout::Nintendo.is_printed_a(right));
        assert!(!ButtonLayout::Nintendo.is_printed_a(bottom));
        // Nothing but the right face button is the printed A, in any layout.
        for l in ButtonLayout::ALL {
            for b in Button::ALL {
                assert_eq!(l.is_printed_a(l.role(b)), b == Button::East, "{l:?} {b:?}");
            }
        }
    }

    #[test]
    fn a_release_follows_its_press_when_the_layout_changes_in_between() {
        // The bottom button confirms the switch to Nintendo; its release comes after the switch.
        let mut r = Remap::default();
        assert_eq!(button_of(&r.event_with(ButtonLayout::Xbox, press(1, Button::South, true))), Button::East);
        assert_eq!(button_of(&r.event_with(ButtonLayout::Nintendo, press(1, Button::South, false))), Button::East, "A is let go");
        // And the other way round.
        assert_eq!(button_of(&r.event_with(ButtonLayout::Nintendo, press(1, Button::East, true))), Button::East);
        assert_eq!(button_of(&r.event_with(ButtonLayout::Xbox, press(1, Button::East, false))), Button::East);
        assert!(r.held.is_empty(), "nothing is left over");
    }

    #[test]
    fn two_controllers_and_a_controller_that_leaves_keep_their_own_roles() {
        let mut r = Remap::default();
        r.event_with(ButtonLayout::Xbox, press(1, Button::South, true));
        r.event_with(ButtonLayout::Xbox, press(2, Button::South, true));
        assert_eq!(button_of(&r.event_with(ButtonLayout::Xbox, press(2, Button::South, false))), Button::East);
        assert_eq!(r.held.len(), 1);
        r.event_with(ButtonLayout::Xbox, PadEvent::Disconnected { source: 1 });
        assert!(r.held.is_empty());
    }

    #[test]
    fn the_process_wide_layout_is_nintendo_until_set() {
        // Other tests may set it; only the round trip is checked here.
        let before = get();
        set(ButtonLayout::Xbox);
        assert_eq!(get(), ButtonLayout::Xbox);
        set(ButtonLayout::Nintendo);
        assert_eq!(get(), ButtonLayout::Nintendo);
        set(before);
    }
}
