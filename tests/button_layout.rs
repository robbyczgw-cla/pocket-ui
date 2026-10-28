//! Settings → Playing → Button layout, from a physical button to what a screen does (docs/DEVELOPING.md): a pad's
//! button goes through `Remap`, then `PadState::merge` as in `user_event`, then the screen's own `press`/`input`.
//! The screens here are the ones that take roles the way all of them do (`Button::East` confirms).

use pocket_ui::button_layout::{ButtonLayout, Remap};
use pocket_ui::keyboard::{Keyboard, Outcome};
use pocket_ui::pad::{Button, PadEvent, PadState};
use pocket_ui::switcher::{Action, Entry, Switcher};
use pocket_ui::touch::hint_button;

/// What the menus get when `button` is pressed (and let go) on a pad, in `layout`.
fn through(layout: ButtonLayout, remap: &mut Remap, pad: &mut PadState, button: Button) -> Button {
    let mut seen = Vec::new();
    for pressed in [true, false] {
        for e in pad.merge(remap.event_with(layout, PadEvent::Button { source: 0, button, pressed })) {
            if let PadEvent::Button { button, pressed: true, .. } = e {
                seen.push(button);
            }
        }
    }
    assert_eq!(seen.len(), 1, "one press reaches the screens: {seen:?}");
    seen[0]
}

fn switcher() -> Switcher {
    let entry = |name: &str| Entry::new("snes", name, std::path::Path::new(&format!("/nonexistent/{name}.sfc")), name.into(), String::new(), 0, false, None);
    Switcher::new(vec![entry("one"), entry("two")], false)
}

/// The four face buttons by where they sit: right, bottom, top, left.
const RIGHT: Button = Button::East;
const BOTTOM: Button = Button::South;
const TOP: Button = Button::North;
const LEFT: Button = Button::West;

#[test]
fn nintendo_is_what_the_pocket_always_did() {
    let (mut remap, mut pad) = (Remap::default(), PadState::new(vec![]));
    for b in Button::ALL {
        assert_eq!(through(ButtonLayout::Nintendo, &mut remap, &mut pad, b), b, "{b:?}");
    }
    let mut kb = Keyboard::default();
    assert_eq!(kb.press(through(ButtonLayout::Nintendo, &mut remap, &mut pad, RIGHT)), Outcome::Editing);
    assert_eq!(kb.text.len(), 1, "the right button types the key");
    assert_eq!(kb.press(through(ButtonLayout::Nintendo, &mut remap, &mut pad, BOTTOM)), Outcome::Editing);
    assert!(kb.text.is_empty(), "the bottom button deletes");
}

#[test]
fn xbox_confirms_at_the_bottom_in_the_keyboard() {
    let (mut remap, mut pad) = (Remap::default(), PadState::new(vec![]));
    let mut kb = Keyboard::default();
    let x = ButtonLayout::Xbox;
    kb.press(through(x, &mut remap, &mut pad, BOTTOM));
    assert_eq!(kb.text.len(), 1, "A (bottom) types the key");
    // The right button is B: it deletes, and on an empty text it cancels.
    kb.press(through(x, &mut remap, &mut pad, RIGHT));
    assert!(kb.text.is_empty());
    assert_eq!(kb.press(through(x, &mut remap, &mut pad, RIGHT)), Outcome::Cancelled);
    // X is on the left (Shift), Y on top (symbols).
    kb.press(through(x, &mut remap, &mut pad, LEFT));
    assert!(kb.shift, "X (left) is Shift");
    kb.press(through(x, &mut remap, &mut pad, TOP));
    assert!(kb.symbols, "Y (top) is the symbols");
}

#[test]
fn xbox_plays_with_the_bottom_button_in_the_switcher() {
    let (mut remap, mut pad) = (Remap::default(), PadState::new(vec![]));
    let x = ButtonLayout::Xbox;
    let mut s = switcher();
    assert_eq!(s.input(through(x, &mut remap, &mut pad, BOTTOM)), Action::Play(0), "A resumes");
    assert_eq!(s.input(through(x, &mut remap, &mut pad, RIGHT)), Action::Close, "B closes");
    assert_eq!(s.input(through(x, &mut remap, &mut pad, TOP)), Action::Favorite(0), "Y (top) stars");
    assert_eq!(s.input(through(x, &mut remap, &mut pad, LEFT)), Action::Remove(0), "X (left) removes");
    // The same buttons in the start layout do the opposite.
    let n = ButtonLayout::Nintendo;
    assert_eq!(s.input(through(n, &mut remap, &mut pad, RIGHT)), Action::Play(0));
    assert_eq!(s.input(through(n, &mut remap, &mut pad, BOTTOM)), Action::Close);
}

#[test]
fn the_hints_say_the_layouts_letters_and_the_pills_press_those_roles() {
    // The hint bar is drawn from roles ("A", "B"), the same in both layouts; a tap on a pill sends the role itself,
    // past `Remap`, so it does what its letter says in either layout.
    let s = switcher();
    let hints = s.hints();
    for (key, role) in [("A", Button::East), ("B", Button::South), ("X", Button::North), ("Y", Button::West)] {
        assert_eq!(hint_button(key), Some(role));
    }
    assert!(hints.iter().any(|h| h.0 == "A") && hints.iter().any(|h| h.0 == "B"));
    // The physical buttons that play those roles are the ones the settings page and the manual describe.
    assert_eq!(ButtonLayout::Xbox.role(BOTTOM), hint_button("A").unwrap());
    assert_eq!(ButtonLayout::Nintendo.role(RIGHT), hint_button("A").unwrap());
}

#[test]
fn switching_the_layout_with_a_press_leaves_nothing_held() {
    // Settings: the bottom button (A in Xbox) switches back to Nintendo; the release arrives in Nintendo.
    let (mut remap, mut pad) = (Remap::default(), PadState::new(vec![]));
    let down = pad.merge(remap.event_with(ButtonLayout::Xbox, PadEvent::Button { source: 0, button: BOTTOM, pressed: true }));
    assert_eq!(down.len(), 1);
    let up = pad.merge(remap.event_with(ButtonLayout::Nintendo, PadEvent::Button { source: 0, button: BOTTOM, pressed: false }));
    assert_eq!(up.len(), 1, "the release reaches the pad state");
    for b in Button::ALL {
        assert!(!pad.is_pressed(b), "{b:?} is let go");
    }
}

#[test]
fn only_the_face_buttons_move() {
    let (mut remap, mut pad) = (Remap::default(), PadState::new(vec![]));
    for b in Button::ALL {
        let want = match b {
            Button::South => Button::East,
            Button::East => Button::South,
            Button::North => Button::West,
            Button::West => Button::North,
            other => other,
        };
        assert_eq!(through(ButtonLayout::Xbox, &mut remap, &mut pad, b), want, "{b:?}");
    }
}
