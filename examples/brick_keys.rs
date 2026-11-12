//! Read the real pad's identity/capabilities and replay F-key edges through the production decoder.
//! No event is written to the input device, no game is started and no setting is changed.
#[cfg(feature = "brick")]
fn main() {
    use evdev::{Device, InputEvent};
    use pocket_ui::pad::{decode::Decoder, Button, PadEvent};
    use pocket_ui::hotkeys::{Action, Enabled, Hotkeys};
    let args: Vec<String> = std::env::args().collect();
    let path = args.get(1).cloned().unwrap_or_else(|| "/dev/input/event3".into());
    let device = Device::open(path).expect("read the built-in pad");
    println!("board={:?} pad={:?} id={:?}", pocket_ui::brick_profile::board(), device.name(), device.input_id());
    assert_eq!(pocket_ui::brick_profile::board(), Some(pocket_ui::brick_profile::Board::Brick));
    let mut decoder = Decoder::new(&device);
    for (code, button) in [(317, Button::F1), (318, Button::F2), (59, Button::F1), (60, Button::F2)] {
        for (action, index) in [(Action::Save, 0), (Action::Load, 1), (Action::Screenshot, 2)] {
            let mut keys = Hotkeys::new(Enabled::default());
            keys.set_f_keys([Some(action), Some(action)]);
            let mut observed = Vec::new();
            for value in [1, 2, 0] {
                for event in decoder.event(0, InputEvent::new(1, code, value)) {
                    if let PadEvent::Button { button: b, pressed: true, .. } = event { observed.push(b); }
                    keys.feed(&event);
                }
            }
            assert_eq!(observed, vec![button], "repeat must not trigger a second press");
            assert_eq!(keys.take(), vec![action], "one configured action per press");
            println!("code={code} button={button:?} action={action:?} choice={index} count=1");
        }
    }
    if let Some(dir) = args.get(2) {
        let saved = pocket_ui::brick_hw::FKeys::load(std::path::Path::new(dir));
        let actions = [pocket_ui::hotkeys::action_of(saved.f1), pocket_ui::hotkeys::action_of(saved.f2)];
        println!("configured f1={:?} f2={:?}", saved.f1, saved.f2);
        for (code, wanted) in [(317, actions[0]), (318, actions[1])] {
            let mut keys = Hotkeys::new(Enabled::default());
            keys.set_f_keys(actions);
            for value in [1, 2, 0] {
                for event in decoder.event(0, InputEvent::new(1, code, value)) { keys.feed(&event); }
            }
            let got = keys.take();
            assert_eq!(got, wanted.into_iter().collect::<Vec<_>>());
            println!("configured code={code} actions={got:?}");
        }
    }

}

#[cfg(not(feature = "brick"))]
fn main() { eprintln!("build with --features brick"); }
