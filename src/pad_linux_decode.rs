//! evdev → positional controls. SDL's Linux button/axis indices are capability ordered, never evdev codes.
use super::{Axis, Button, PadEvent};
use evdev::{AbsoluteAxisCode as Abs, Device, EventSummary, InputEvent, KeyCode};
use std::collections::BTreeMap;

const DB: &str = include_str!("../assets/controllers/gamecontrollerdb.txt");
pub const PRESS: f32 = 0.55;
pub const RELEASE: f32 = 0.40;
/// A stick or trigger value older than this when it is read waited in the kernel's queue: nothing read the device
/// for a while (pocket-ui stopped by a test harness and let go on again, a long stall). Readers normally take an
/// event within milliseconds.
pub const STALE: std::time::Duration = std::time::Duration::from_secs(1);

#[derive(Clone, Copy)]
enum Target { Button(Button), Axis(Axis) }

/// Created on the reader thread, once per event node; event processing does no file IO.
pub struct Decoder {
    keys: BTreeMap<u16, Button>,
    axes: BTreeMap<u16, (Axis, i32, i32)>,
    hats: BTreeMap<u16, Vec<(Button, i32)>>,
    held: BTreeMap<(u16, u16, Button), bool>,
    name: String,
    /// The newest value of each stick and trigger that came out of the kernel's queue late (`event_read_at`).
    backlog: BTreeMap<u16, i32>,
    /// On an RK3566 board ROCKNIX's InputPlumber sends Guide (BTN_MODE) with every SELECT (`anbernic_gen1` and `powkiddy`
    /// maps, "Guide Button (Select)", listed after "Select Button"): a BTN_MODE press that comes within `GUIDE_ECHO` of a
    /// SELECT press is that copy, and it and its release are dropped. Otherwise SELECT would be HOME held, and a game would
    /// never get SELECT (`hotkeys`). HOME there is the F key or the right stick's click (`pad::linux::rk3566_home_key`).
    guide_echo: bool,
    select_down_at: Option<std::time::SystemTime>,
    guide_dropped: bool,
}

/// How close behind a SELECT press a Guide press counts as InputPlumber's copy of it (both come from one source event).
pub const GUIDE_ECHO: std::time::Duration = std::time::Duration::from_millis(60);

/// One read's events with BTN_SELECT ahead of BTN_MODE (the order InputPlumber's map sends them in), so the echo filter
/// sees SELECT first also when both arrive in one report. Nothing else moves.
pub fn select_before_guide(batch: &mut [InputEvent]) {
    let rank = |e: &InputEvent| match e.destructure() {
        EventSummary::Key(_, KeyCode::BTN_SELECT, _) => 0,
        EventSummary::Key(_, KeyCode::BTN_MODE, _) => 1,
        _ => 2,
    };
    let (mut s, mut m) = (None, None);
    for (i, e) in batch.iter().enumerate() {
        match rank(e) { 0 if s.is_none() => s = Some(i), 1 if m.is_none() => m = Some(i), _ => {} }
    }
    if let (Some(s), Some(m)) = (s, m) {
        if m < s { batch[m..=s].rotate_right(1); }
    }
}

impl Decoder {
    pub fn new(dev: &Device) -> Self {
        let id = dev.input_id();
        let keys = dev.supported_keys().map(|k| k.iter().map(|k| k.0).collect()).unwrap_or_default();
        let ranges = dev.get_absinfo().map(|a| a.map(|(c, a)| (c.0, a.minimum(), a.maximum())).collect()).unwrap_or_default();
        Self::with_caps(id.bus_type().0, id.vendor(), id.product(), id.version(), dev.name().unwrap_or(""), keys, ranges)
    }

    pub fn with_caps(bus: u16, vendor: u16, product: u16, version: u16, name: &str, keys: Vec<u16>, ranges: Vec<(u16, i32, i32)>) -> Self {
        #[cfg(feature = "brick")]
        let legacy_fkeys = crate::brick_profile::legacy_input_fkeys();
        #[cfg(not(feature = "brick"))]
        let legacy_fkeys = false;
        Self::with_caps_profile((bus, vendor, product, version), name, keys, ranges, legacy_fkeys)
    }

    /// Fixture readers supply the verified physical profile: the Brick advertises the Pro's unused Xbox capabilities too.
    /// Live readers use `with_caps`, which verifies the firmware identity instead of trusting the caller.
    #[doc(hidden)]
    pub fn with_caps_profile(id: (u16, u16, u16, u16), name: &str, mut keys: Vec<u16>, mut ranges: Vec<(u16, i32, i32)>, legacy_fkeys: bool) -> Self {
        let (bus, vendor, product, version) = id;
        #[cfg(not(feature = "brick"))]
        let _ = legacy_fkeys;
        // Same two loops as SDL_sysjoystick.c (BTN_JOYSTICK first, then smaller key codes).
        keys.sort_by_key(|k| (*k < 0x120, *k));
        ranges.sort_by_key(|a| a.0);
        let guide_echo = crate::device::current().model.is_some_and(|m| m.is_rk3566());
        let mut out = Self { keys: BTreeMap::new(), axes: BTreeMap::new(), hats: BTreeMap::new(), held: BTreeMap::new(), name: name.into(), backlog: BTreeMap::new(),
            guide_echo, select_down_at: None, guide_dropped: false };
        // The built-in Brick pad borrows the Xbox 360 USB ID. Its measured layout
        // (including F1/F2 and analog triggers) wins over generic Xbox/SDL guesses.
        let brick = name.trim() == "TRIMUI Player1";
        let has = |code: Abs| ranges.iter().any(|a| a.0 == code.0);
        // These raw MCU pads are normally hidden by InputPlumber. Their native
        // fallback keeps direct face positions and Retroid's HAT2 triggers.
        let native_mcu = vendor == 0x2020 && product == 0x3001 && matches!(name, "Retroid Pocket Gamepad" | "AYN Odin2 Gamepad");
        let retroid_triggers = native_mcu && has(Abs::ABS_HAT2X) && has(Abs::ABS_HAT2Y);
        let known_layout = brick || native_mcu || super::linux::is_h700_pad(name);
        let z_stick = !known_layout && !has(Abs::ABS_RX) && !has(Abs::ABS_RY) && has(Abs::ABS_Z) && has(Abs::ABS_RZ);
        for &key in &keys {
            #[cfg(feature = "brick")]
            let mapped = super::linux::button_on_with_legacy_fkeys(name, KeyCode(key), legacy_fkeys);
            #[cfg(not(feature = "brick"))]
            let mapped = super::linux::button_on(name, KeyCode(key));
            if let Some(mut b) = mapped {
                // xpad follows Xbox X/Y codes (BTN_NORTH/BTN_WEST), rather than Sony's positional convention.
                if vendor == 0x045e && !known_layout { b = match b { Button::North => Button::West, Button::West => Button::North, b => b }; }
                // 8BitDo's generic HID descriptor has A before B; these are Nintendo letters.
                if vendor == 0x2dc8 && !known_layout && keys.contains(&KeyCode::BTN_C.0) { b = match b { Button::South => Button::East, Button::East => Button::South, b => b }; }
                out.keys.insert(key, b);
            }
        }
        for &(code, lo, hi) in &ranges {
            let axis = match Abs(code) {
                Abs::ABS_X => Axis::LeftX, Abs::ABS_Y => Axis::LeftY,
                Abs::ABS_RX => Axis::RightX, Abs::ABS_RY => Axis::RightY,
                Abs::ABS_Z | Abs::ABS_RZ if retroid_triggers => continue, // unused MCU axes, not triggers
                Abs::ABS_HAT2X if retroid_triggers => Axis::L2,
                Abs::ABS_HAT2Y if retroid_triggers => Axis::R2,
                Abs::ABS_Z if z_stick => Axis::RightX, Abs::ABS_RZ if z_stick => Axis::RightY,
                Abs::ABS_Z | Abs::ABS_BRAKE => Axis::L2,
                Abs::ABS_RZ | Abs::ABS_GAS => Axis::R2,
                Abs::ABS_HAT0X => { out.hats.insert(code, vec![(Button::Left, -1), (Button::Right, 1)]); continue; }
                Abs::ABS_HAT0Y => { out.hats.insert(code, vec![(Button::Up, -1), (Button::Down, 1)]); continue; }
                _ => continue,
            };
            out.axes.insert(code, (axis, lo, hi));
        }
        let le = |n: u16| format!("{:02x}{:02x}0000", n & 255, n >> 8);
        let guid = format!("{}{}{}{}", le(bus), le(vendor), le(product), le(version));
        // A version mismatch must not silently apply a different firmware's layout.
        if let Some(row) = DB.lines().find(|s| !known_layout && s.starts_with(&format!("{guid},"))) {
            let analog: Vec<_> = ranges.iter().filter(|a| !(16..=23).contains(&a.0) || a.2 - a.1 > 2).copied().collect();
            let mut mapped_keys = BTreeMap::new();
            let mut mapped_axes = BTreeMap::new();
            let mut mapped_hats: BTreeMap<u16, Vec<(Button, i32)>> = BTreeMap::new();
            for field in row.split(',').skip(2) {
                let Some((logical, raw)) = field.split_once(':') else { continue };
                let Some(target) = target(logical) else { continue };
                match (raw.as_bytes().first(), target) {
                    (Some(b'b'), Target::Button(b)) => {
                        if let Some(key) = raw[1..].parse::<usize>().ok().and_then(|i| keys.get(i)) { mapped_keys.insert(*key, b); }
                    }
                    (Some(b'b'), Target::Axis(a)) => {
                        if let Some(key) = raw[1..].parse::<usize>().ok().and_then(|i| keys.get(i)) { mapped_keys.insert(*key, trigger_button(a)); }
                    }
                    (Some(b'a'), Target::Axis(a)) => {
                        if let Some(&(code, lo, hi)) = raw[1..].parse::<usize>().ok().and_then(|i| analog.get(i)) { mapped_axes.insert(code, (a, lo, hi)); }
                    }
                    (Some(b'h'), Target::Button(b)) => {
                        if let Some((hat, bit)) = raw[1..].split_once('.') {
                            if let (Ok(hat), Ok(bit)) = (hat.parse::<u16>(), bit.parse::<u16>()) {
                                let (offset, sign) = match bit { 1 => (1, -1), 2 => (0, 1), 4 => (1, 1), 8 => (0, -1), _ => continue };
                                mapped_hats.entry(16 + hat * 2 + offset).or_default().push((b, sign));
                            }
                        }
                    }
                    _ => {}
                }
            }
            // System keys are independent of a pad's SDL map. Unmapped gamepad buttons stay raw.
            // Older SN30 records omit `guide` although the kernel exposes BTN_MODE. Keep
            // recovery/menu keys; an explicit SDL binding for that code still wins below.
            out.keys.retain(|k, b| *k < 0x120 || matches!(b, Button::Mode | Button::Back)
                // Some real pads (including InputPlumber's DS5 target) advertise
                // both trigger keys and axes. Keep the digital aliases of mapped
                // triggers; `button` holds them together until BOTH are released.
                || matches!(b, Button::L2) && mapped_axes.values().any(|(a, _, _)| *a == Axis::L2)
                || matches!(b, Button::R2) && mapped_axes.values().any(|(a, _, _)| *a == Axis::R2));
            out.keys.extend(mapped_keys);
            out.axes = mapped_axes;
            out.hats = mapped_hats;
        }
        out
    }

    fn button(&mut self, source: usize, kind: u16, code: u16, b: Button, down: bool, out: &mut Vec<PadEvent>) {
        let was = self.held.iter().any(|((_, _, button), down)| *button == b && *down);
        self.held.insert((kind, code, b), down);
        let now = self.held.iter().any(|((_, _, button), down)| *button == b && *down);
        if was != now { out.push(PadEvent::Button { source, button: b, pressed: now }); }
    }

    /// `event`, for an event read at `now` (the live reader). Stick and trigger values that waited in the kernel's
    /// queue (`STALE`) are history: only the newest of each axis goes on, at `flush`, once the queue is read empty.
    /// So a stick pushed and sprung back, or one bad report, while nothing read the device is nothing (it woke Alex's
    /// dark Mini V2 on 2026-11-14 the moment a stopped pocket-ui went on); a stick still held then is still held.
    /// Buttons and the d-pad go on as they came: each press is one.
    pub fn event_read_at(&mut self, source: usize, event: InputEvent, now: std::time::SystemTime) -> Vec<PadEvent> {
        if let EventSummary::AbsoluteAxis(_, code, value) = event.destructure() {
            if self.axes.contains_key(&code.0) {
                if now.duration_since(event.timestamp()).is_ok_and(|age| age > STALE) {
                    self.backlog.insert(code.0, value);
                    return Vec::new();
                }
                // A value read in time is newer than what waited.
                self.backlog.remove(&code.0);
            }
        }
        self.event(source, event)
    }

    /// Whether late stick or trigger values wait for `flush`.
    pub fn has_backlog(&self) -> bool {
        !self.backlog.is_empty()
    }

    /// The newest late value of each stick and trigger (`event_read_at`), now that the queue is read empty.
    pub fn flush(&mut self, source: usize) -> Vec<PadEvent> {
        let backlog = std::mem::take(&mut self.backlog);
        backlog.into_iter().flat_map(|(code, value)| self.event(source, InputEvent::new(evdev::EventType::ABSOLUTE.0, code, value))).collect()
    }

    /// The RK3566 Guide-echo filter on or off (`guide_echo`), for tests; live readers take it from the device.
    #[doc(hidden)]
    pub fn with_guide_echo(mut self, on: bool) -> Self {
        self.guide_echo = on;
        self
    }

    /// `guide_echo`: whether this key event is InputPlumber's Guide copy of SELECT (or its release), to be dropped.
    fn guide_copy(&mut self, code: KeyCode, value: i32, at: std::time::SystemTime) -> bool {
        if !self.guide_echo { return false; }
        match code {
            KeyCode::BTN_SELECT => { self.select_down_at = (value != 0).then_some(at); false }
            KeyCode::BTN_MODE if value != 0 => {
                let copy = self.select_down_at.is_some_and(|s| at.duration_since(s).map_or(true, |d| d <= GUIDE_ECHO));
                self.guide_dropped = copy;
                copy
            }
            KeyCode::BTN_MODE => std::mem::take(&mut self.guide_dropped),
            _ => false,
        }
    }

    pub fn event(&mut self, source: usize, event: InputEvent) -> Vec<PadEvent> {
        let mut out = Vec::new();
        match event.destructure() {
            EventSummary::Key(_, code, value) if value != 2 && self.guide_copy(code, value, event.timestamp()) => {}
            EventSummary::Key(_, code, value) if value != 2 => {
                if let Some(&b) = self.keys.get(&code.0) {
                    self.button(source, 1, code.0, b, value != 0, &mut out);
                } else { out.push(PadEvent::Raw { source, text: format!("{}: {code:?} {value}", self.name) }); }
            }
            EventSummary::AbsoluteAxis(_, code, value) => {
                if let Some(&(axis, lo, hi)) = self.axes.get(&code.0) {
                    let value = if hi > lo { ((value as f64 - lo as f64) / (hi as f64 - lo as f64)).clamp(0.0, 1.0) as f32 } else { 0.0 };
                    let trigger = matches!(axis, Axis::L2 | Axis::R2);
                    out.push(PadEvent::Axis { source, axis, value: if trigger { value } else { value * 2.0 - 1.0 } });
                    if trigger {
                        let b = trigger_button(axis);
                        let was = self.held.get(&(3, code.0, b)).copied().unwrap_or(false);
                        self.button(source, 3, code.0, b, value >= if was { RELEASE } else { PRESS }, &mut out);
                    }
                } else if let Some(bindings) = self.hats.get(&code.0).cloned() {
                    for (b, sign) in bindings { self.button(source, 3, code.0, b, value.signum() == sign, &mut out); }
                }
            }
            EventSummary::Switch(..) => { if let Some(e) = super::linux::switch(source, &event) { out.push(e); } }
            _ => {}
        }
        out
    }
}

fn trigger_button(a: Axis) -> Button { if a == Axis::L2 { Button::L2 } else { Button::R2 } }

fn target(name: &str) -> Option<Target> {
    Some(match name {
        "a" => Target::Button(Button::South), "b" => Target::Button(Button::East),
        "x" => Target::Button(Button::West), "y" => Target::Button(Button::North),
        "back" => Target::Button(Button::Select), "start" => Target::Button(Button::Start), "guide" => Target::Button(Button::Mode),
        "leftshoulder" => Target::Button(Button::L1), "rightshoulder" => Target::Button(Button::R1),
        "leftstick" => Target::Button(Button::L3), "rightstick" => Target::Button(Button::R3),
        "dpup" => Target::Button(Button::Up), "dpdown" => Target::Button(Button::Down),
        "dpleft" => Target::Button(Button::Left), "dpright" => Target::Button(Button::Right),
        "leftx" => Target::Axis(Axis::LeftX), "lefty" => Target::Axis(Axis::LeftY),
        "rightx" => Target::Axis(Axis::RightX), "righty" => Target::Axis(Axis::RightY),
        "lefttrigger" => Target::Axis(Axis::L2), "righttrigger" => Target::Axis(Axis::R2),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pad::{Controller, Held, Mux};
    use crate::players::Inputs;
    use evdev::{uinput::VirtualDevice, AbsInfo, AttributeSet, BusType, InputId, UinputAbsSetup};
    use std::sync::mpsc;
    use std::time::Duration;

    struct Case { name: &'static str, vendor: u16, product: u16, version: u16, keys: Vec<u16>, axes: Vec<(u16, i32, i32)>, l2: (u16, u16), r2: (u16, u16) }
    fn cases() -> Vec<Case> {
        let standard = || vec![304,305,307,308,310,311,312,313,314,315,316,317,318,544,545,546,547];
        let generic = || (304..=318).collect();
        let switch = || vec![304,305,307,308,309,310,311,312,313,314,315,316,317,318];
        let switch_axes = || vec![(0,-32767,32767),(1,-32767,32767),(3,-32767,32767),(4,-32767,32767),(16,-1,1),(17,-1,1)];
        let xy = || vec![(0,-32768,32767),(1,-32768,32767),(2,0,255),(3,-32768,32767),(4,-32768,32767),(5,0,255),(16,-1,1),(17,-1,1)];
        let direct = || vec![(0,0,255),(1,0,255),(2,0,255),(5,0,255),(16,-1,1),(17,-1,1)];
        let brake = || vec![(0,0,65535),(1,0,65535),(2,0,65535),(5,0,65535),(9,0,1023),(10,0,1023),(16,-1,1),(17,-1,1)];
        vec![
            Case { name:"SN30 Pro X-input", vendor:0x045e,product:0x028e,version:0x0114,keys:standard(),axes:xy(),l2:(3,2),r2:(3,5) },
            Case { name:"SN30 Pro D-input", vendor:0x2dc8,product:0x6101,version:0x0100,keys:generic(),axes:direct(),l2:(1,312),r2:(1,313) },
            Case { name:"SN30 Pro Switch", vendor:0x057e,product:0x2009,version:0x8001,keys:switch(),axes:switch_axes(),l2:(1,312),r2:(1,313) },
            Case { name:"SN30 Pro macOS", vendor:0x054c,product:0x05c4,version:0x8100,keys:standard(),axes:xy(),l2:(3,2),r2:(3,5) },
            Case { name:"Pro 2 X-input", vendor:0x045e,product:0x02e0,version:0x0903,keys:standard(),axes:xy(),l2:(3,2),r2:(3,5) },
            Case { name:"Pro 2 D-input", vendor:0x2dc8,product:0x6006,version:0x0100,keys:generic(),axes:brake(),l2:(3,10),r2:(3,9) },
            Case { name:"Pro 2 Switch", vendor:0x057e,product:0x2009,version:0x8001,keys:switch(),axes:switch_axes(),l2:(1,312),r2:(1,313) },
            Case { name:"Pro 2 macOS", vendor:0x054c,product:0x05c4,version:0x8100,keys:standard(),axes:xy(),l2:(3,2),r2:(3,5) },
            Case { name:"Xbox Series BLE", vendor:0x045e,product:0x0b13,version:0x0509,keys:generic(),axes:brake(),l2:(3,10),r2:(3,9) },
            Case { name:"DualSense hid-playstation", vendor:0x054c,product:0x0ce6,version:0x8100,keys:standard().into_iter().filter(|k| ![544,545,546,547].contains(k)).collect(),axes:xy(),l2:(3,2),r2:(3,5) },
            Case { name:"Switch Pro hid-nintendo", vendor:0x057e,product:0x2009,version:0x8001,keys:switch(),axes:switch_axes(),l2:(1,312),r2:(1,313) },
        ]
    }
    fn decoder(c: &Case) -> Decoder { Decoder::with_caps(5,c.vendor,c.product,c.version,c.name,c.keys.clone(),c.axes.clone()) }
    fn value(c: &Case, control: (u16,u16), down: bool) -> i32 {
        if control.0 == 1 { return i32::from(down); }
        let a = c.axes.iter().find(|a| a.0 == control.1).unwrap(); if down { a.2 } else { a.1 }
    }

    #[test]
    #[cfg(feature = "brick")]
    fn the_real_brick_pad_saves_and_loads_with_f_keys_despite_its_xbox_id() {
        use crate::hotkeys::{Action, Enabled, Hotkeys};
        // The verified plain Brick borrows the Xbox 360 ID and advertises dummy stick axes too.
        // Physical F keys still use thumb codes; identity, not these capabilities, establishes that.
        let mut decoder = Decoder::with_caps_profile((3, 0x045e, 0x028e, 0x0114), "TRIMUI Player1",
            vec![59,60,304,305,307,308,310,311,314,315,316,317,318],
            vec![(0,-32767,32767),(1,-32767,32767),(2,0,255),(3,-32767,32767),(4,-32767,32767),(5,0,255),(16,-1,1),(17,-1,1)], true);
        let input = Inputs::new(vec![], None);
        input.feed(PadEvent::Connected { source: 3, controller: Controller::builtin() }, |_| {});
        input.arrange();
        let mut hotkeys = Hotkeys::new(Enabled::default());
        let dir = crate::testutil::TestDir::new("brick-real-fkeys");
        std::fs::write(dir.join("brick-keys.json"), r#"{"f1":"save","f2":"load"}"#).unwrap();
        let keys = crate::brick_hw::FKeys::load(&dir);
        hotkeys.set_f_keys([crate::hotkeys::action_of(keys.f1), crate::hotkeys::action_of(keys.f2)]);
        for (code, action) in [(317, Action::Save), (318, Action::Load)] {
            for down in [1, 2, 0] {
                for e in decoder.event(3, InputEvent::new(1, code, down)) {
                    input.feed(e, |merged| hotkeys.feed(merged));
                }
            }
            assert_eq!(hotkeys.take(), vec![action], "one physical F-key press must complete its configured action");
        }
        for (kind, code, button, value) in [(1,308,Button::North,1),(1,307,Button::West,1),
            (3,2,Button::L2,255),(3,5,Button::R2,255)] {
            assert!(decoder.event(3, InputEvent::new(kind,code,value)).iter().any(|e|
                matches!(e, PadEvent::Button { button: b, pressed: true, .. } if *b == button)), "{button:?} must retain its real Brick position");
        }
    }
    #[test]
    #[cfg(feature = "brick")]
    fn brickpro_stick_click_never_quick_saves_or_loads_even_with_legacy_board_detection() {
        use crate::hotkeys::{Action, Enabled, Hotkeys};
        // This test also runs with POCKETUI_DEVICE=brick: without verified plain-Brick identity the guess must fail safe.
        let mut decoder = Decoder::with_caps(3, 0x045e, 0x028e, 0x0114, "TRIMUI Player1",
            vec![59,60,304,305,307,308,310,311,314,315,316,317,318],
            vec![(0,-32768,32767),(1,-32768,32767),(2,0,255),(3,-32768,32767),(4,-32768,32767),(5,0,255),(16,-1,1),(17,-1,1)]);
        let input = Inputs::new(vec![], None);
        input.feed(PadEvent::Connected { source: 3, controller: Controller::builtin() }, |_| {});
        input.arrange();
        let mut hotkeys = Hotkeys::new(Enabled::default());
        hotkeys.set_f_keys([Some(Action::Save), Some(Action::Load)]);
        for (code, want) in [(317,Button::L3),(318,Button::R3)] {
            let mut seen = false;
            for down in [1,2,0] {
                for e in decoder.event(3, InputEvent::new(1, code, down)) {
                    seen |= matches!(e, PadEvent::Button { button, pressed: true, .. } if button == want);
                    input.feed(e, |merged| hotkeys.feed(merged));
                }
            }
            let actions = hotkeys.take();
            assert!(actions.is_empty(), "stick click {code} issued {actions:?}");
            assert!(seen, "stick click {code} must reach the game as {want:?}");
        }
        for (code, want) in [(59,Action::Save),(60,Action::Load)] {
            for down in [1,0] { for e in decoder.event(3, InputEvent::new(1,code,down)) { input.feed(e, |e| hotkeys.feed(e)); } }
            assert_eq!(hotkeys.take(), vec![want], "the separate F key must still work");
        }
        for (index, (code, want)) in [(0,Axis::LeftX),(1,Axis::LeftY),(3,Axis::RightX),(4,Axis::RightY)].into_iter().enumerate() {
            let events = decoder.event(3, InputEvent::new(3,code,-32768));
            assert!(events.iter().any(|e|
                matches!(e, PadEvent::Axis { axis, value, .. } if *axis == want && *value == -1.0)), "{want:?} stays analog");
            for event in events { input.feed(event, |_| {}); }
            assert_eq!(input.sticks(0).analog(true)[index], -32767, "{want:?} reaches the core's analog inputs");
            for event in decoder.event(3, InputEvent::new(3,code,0)) { input.feed(event, |_| {}); }
            assert_eq!(input.sticks(0).analog(true)[index], 0, "{want:?} centers");
        }
    }

    #[test]
    fn every_mode_has_both_triggers_in_the_menu_and_in_a_game() {
        for c in cases() {
            let mut d = decoder(&c); let mut mux = Mux::default(); let input = Inputs::new(vec![],None);
            let controller = Controller { key:"bt:test-a".into(),name:c.name.into(),gamepad:true,node:String::new() };
            let connected = PadEvent::Connected { source:7,controller };
            mux.feed(connected.clone()); input.feed(connected, |_|{}); input.arrange();
            for (control,b,retro) in [(c.l2,Button::L2,12),(c.r2,Button::R2,13)] {
                for down in [true,false] {
                    for ev in d.event(7,InputEvent::new(control.0,control.1,value(&c,control,down))) { mux.feed(ev.clone()); input.feed(ev, |_|{}); }
                    assert_eq!(mux.is_pressed(b),down,"{} {b:?} menu",c.name);
                    assert_eq!(input.buttons(0) & (1<<retro) != 0,down,"{} {b:?} game",c.name);
                }
            }
        }
    }

    #[test]
    fn face_buttons_and_dpad_keep_their_positions_in_each_mode() {
        for c in cases() {
            let mut d = decoder(&c);
            let face = if c.vendor == 0x2dc8 { [305,304,307,308] } else if c.vendor == 0x045e { [304,305,308,307] } else { [304,305,307,308] };
            for (key, button) in face.into_iter().zip([Button::South,Button::East,Button::North,Button::West]) {
                assert!(d.event(3,InputEvent::new(1,key,1)).iter().any(|e|matches!(e,PadEvent::Button{button:b,pressed:true,..} if *b==button)), "{} {button:?}",c.name);
                d.event(3,InputEvent::new(1,key,0));
            }
            assert!(d.event(3,InputEvent::new(3,16,-1)).iter().any(|e|matches!(e,PadEvent::Button{button:Button::Left,pressed:true,..})), "{} hat",c.name);
            assert!(d.event(3,InputEvent::new(3,16,1)).iter().any(|e|matches!(e,PadEvent::Button{button:Button::Left,pressed:false,..})), "{} hat releases before opposite direction",c.name);
        }
    }

    #[test]
    fn trigger_jitter_and_duplicate_axis_button_reports_do_not_repeat_or_release_early() {
        let c = &cases()[0]; let mut d = decoder(c); let mut edges = Vec::new();
        for value in [0,140,130,141,120,103,102,0] {
            edges.extend(d.event(1,InputEvent::new(3,2,value)).into_iter().filter_map(|e| if let PadEvent::Button {button:Button::L2,pressed,..}=e {Some(pressed)} else {None}));
        }
        assert_eq!(edges,[true,false]);
        let mut d = Decoder::with_caps(5,0,0,1,"Dual reports",c.keys.clone(),c.axes.clone());
        let mut held = Held::default();
        for e in d.event(1,InputEvent::new(3,2,255)) { held.observe(&e); }
        assert!(d.event(1,InputEvent::new(1,312,1)).is_empty());
        assert!(d.event(1,InputEvent::new(1,312,0)).is_empty(),"the axis still holds L2");
        assert!(held.release(1).iter().any(|e| matches!(e,PadEvent::Button{button:Button::L2,pressed:false,..})),"disconnect releases L2");
    }

    #[test]
    fn dinput_right_stick_does_not_press_triggers_and_brake_is_left_gas_is_right() {
        let c = &cases()[5]; let mut d = decoder(c);
        let e = d.event(1,InputEvent::new(3,2,65535));
        assert!(matches!(e.as_slice(),[PadEvent::Axis{axis:Axis::RightX,..}]));
        assert!(d.event(1,InputEvent::new(3,10,1023)).iter().any(|e| matches!(e,PadEvent::Button{button:Button::L2,pressed:true,..})));
        assert!(d.event(1,InputEvent::new(3,9,1023)).iter().any(|e| matches!(e,PadEvent::Button{button:Button::R2,pressed:true,..})));
    }

    #[test]
    fn an_sn30_record_without_guide_keeps_the_kernel_home_button() {
        let c = &cases()[1];
        let mut d = Decoder::with_caps(5,0x2dc8,0x2862,0x0100,c.name,c.keys.clone(),c.axes.clone());
        let events = d.event(4,InputEvent::new(1,KeyCode::BTN_MODE.0,1));
        assert!(events.iter().any(|e| matches!(e,PadEvent::Button{button:Button::Mode,pressed:true,..})), "HOME still opens the pause menu: {events:?}");
    }

    struct TestNode { path: std::path::PathBuf, created: bool }
    impl TestNode {
        fn new(dev: &mut VirtualDevice) -> Self {
            let path = dev.enumerate_dev_nodes_blocking().unwrap().next().unwrap().unwrap();
            let created = !path.exists();
            if created {
                use std::os::unix::ffi::OsStrExt;
                let sys = dev.get_syspath().unwrap().join(path.file_name().unwrap()).join("dev");
                let ids = std::fs::read_to_string(sys).unwrap();
                let (major, minor) = ids.trim().split_once(':').unwrap();
                let name = std::ffi::CString::new(path.as_os_str().as_bytes()).unwrap();
                assert_eq!(unsafe { libc::mknod(name.as_ptr(), libc::S_IFCHR | 0o600, libc::makedev(major.parse().unwrap(), minor.parse().unwrap())) }, 0);
            }
            Self { path, created }
        }
    }
    impl Drop for TestNode { fn drop(&mut self) { if self.created { let _ = std::fs::remove_file(&self.path); } } }

    #[test]
    #[ignore = "requires /dev/uinput and permission to read its event nodes; run explicitly as root"]
    fn uinput_modes_two_pads_and_hotplug_use_the_real_reader() {
        for c in cases() {
            let mut keys = AttributeSet::<KeyCode>::new(); for k in &c.keys { keys.insert(KeyCode(*k)); }
            let mut builder = VirtualDevice::builder().unwrap().name(c.name).input_id(InputId::new(BusType::BUS_BLUETOOTH,c.vendor,c.product,c.version)).with_keys(&keys).unwrap();
            for &(code,lo,hi) in &c.axes { builder = builder.with_absolute_axis(&UinputAbsSetup::new(Abs(code),AbsInfo::new(if lo<0{0}else{lo},lo,hi,0,0,0))).unwrap(); }
            let mut virtual_pad = builder.build().unwrap();
            let node = TestNode::new(&mut virtual_pad);
            let dev = Device::open(&node.path).unwrap(); assert!(super::super::linux::is_wanted(&dev));
            let (tx,rx) = mpsc::channel();
            let reader = std::thread::spawn(move || super::super::linux::pump(17,dev,|e|tx.send(e).is_ok()));
            virtual_pad.emit(&[InputEvent::new(c.l2.0,c.l2.1,value(&c,c.l2,true)),InputEvent::new(c.r2.0,c.r2.1,value(&c,c.r2,true))]).unwrap();
            let mut seen = 0;
            while seen != 3 {
                match rx.recv_timeout(Duration::from_secs(3)).unwrap_or_else(|_|panic!("{} missing trigger {seen}",c.name)) {
                    PadEvent::Button{button:Button::L2,pressed:true,..} => seen|=1,
                    PadEvent::Button{button:Button::R2,pressed:true,..} => seen|=2,
                    _=>{}
                }
            }
            // A second independent controller is live while the first disconnects holding both triggers.
            let mut second = super::super::bridge::device(3).unwrap();
            let node2 = TestNode::new(&mut second);
            let mut dev2 = Device::open(&node2.path).unwrap();
            second.emit(&super::super::bridge::events(Button::South.bit(),[0.0;6])).unwrap();
            drop(virtual_pad);
            let mut released = 0;
            loop {
                match rx.recv_timeout(Duration::from_secs(3)).unwrap() {
                    PadEvent::Button{button:Button::L2,pressed:false,..}=>released|=1,
                    PadEvent::Button{button:Button::R2,pressed:false,..}=>released|=2,
                    PadEvent::Disconnected{source:17}=>break,
                    _=>{}
                }
            }
            assert_eq!(released,3,"{} disconnect",c.name);
            reader.join().unwrap();
            assert!(dev2.fetch_events().unwrap().any(|e|e.event_type().0==1 && e.code()==KeyCode::BTN_SOUTH.0 && e.value()==1));
        }
    }
}

#[cfg(test)]
mod noise_tests {
    use super::*;
    use crate::pad::{counts_as_input, Controller, PadState};
    use std::time::{Duration, SystemTime};

    const ABS: u16 = 3;
    const KEY: u16 = 1;

    /// The Mini V2's built-in pad as InputPlumber shows it (`/proc/bus/input/devices`, 2026-11-14): a virtual DualSense,
    /// sticks and triggers 0-255, the d-pad a hat.
    fn mini_v2() -> Decoder {
        let keys = vec![304, 305, 307, 308, 310, 311, 312, 313, 314, 315, 316, 317, 318];
        let axes = vec![(0, 0, 255), (1, 0, 255), (2, 0, 255), (3, 0, 255), (4, 0, 255), (5, 0, 255), (16, -1, 1), (17, -1, 1)];
        Decoder::with_caps(3, 0x054c, 0x0ce6, 0x8111, "Sony Interactive Entertainment DualSense Wireless Controller", keys, axes)
    }

    /// Reads `events` the way the live reader does when they all come out of the queue `late` after they happened,
    /// then the queue is empty.
    fn read(dec: &mut Decoder, events: &[(u16, u16, i32)], late: Duration) -> Vec<PadEvent> {
        let at = SystemTime::now() + late;
        let mut out: Vec<PadEvent> = events.iter().flat_map(|&(k, c, v)| dec.event_read_at(5, InputEvent::new_now(k, c, v), at)).collect();
        if dec.has_backlog() {
            out.extend(dec.flush(5));
        }
        out
    }

    fn any_input(events: &[PadEvent]) -> bool {
        events.iter().any(counts_as_input)
    }

    /// 2026-11-14 ~22:17: Alex's Mini V2 lay dark ("light" sleep) while a test harness held his pocket-ui stopped.
    /// The left stick went out and back meanwhile; when pocket-ui went on, it read the queue and "sleep: awake (a
    /// stick)" turned the screen on. What waited in the queue is history: a stick that is back at rest by now is
    /// nothing, never a wake.
    #[test]
    fn a_stick_pushed_and_back_while_nothing_read_is_nothing_when_read_late() {
        let mut dec = mini_v2();
        let stream = [(ABS, 0, 142), (ABS, 0, 240), (ABS, 0, 255), (ABS, 1, 30), (ABS, 0, 180), (ABS, 1, 128), (ABS, 0, 142)];
        let out = read(&mut dec, &stream, Duration::from_secs(240));
        assert!(!any_input(&out), "late stick history woke the screen: {out:?}");
        assert!(!dec.has_backlog());
    }

    /// One bad report while nothing read (a value far out and straight back) is the same: nothing.
    #[test]
    fn one_wild_report_in_the_queue_is_nothing_when_read_late() {
        let mut dec = mini_v2();
        let out = read(&mut dec, &[(ABS, 3, 0), (ABS, 3, 132)], Duration::from_secs(5));
        assert!(!any_input(&out), "{out:?}");
    }

    /// A stick still held when the queue is read is still held, and a trigger still pulled still pulled: the newest
    /// value of each goes on, once.
    #[test]
    fn a_stick_still_held_after_a_late_read_is_still_held() {
        let mut dec = mini_v2();
        let out = read(&mut dec, &[(ABS, 0, 200), (ABS, 0, 255), (ABS, 5, 255)], Duration::from_secs(30));
        let axes: Vec<_> = out.iter().filter_map(|e| match *e { PadEvent::Axis { axis, value, .. } => Some((axis, value)), _ => None }).collect();
        assert_eq!(axes, vec![(Axis::LeftX, 1.0), (Axis::R2, 1.0)], "{out:?}");
        assert!(out.iter().any(|e| matches!(e, PadEvent::Button { source: 5, button: Button::R2, pressed: true })));
    }

    /// Buttons and the d-pad are not history: each press that waited in the queue goes on, down and up.
    #[test]
    fn late_buttons_and_dpad_presses_all_go_on() {
        let mut dec = mini_v2();
        let out = read(&mut dec, &[(KEY, 304, 1), (KEY, 304, 0), (ABS, 17, 1), (ABS, 17, 0)], Duration::from_secs(30));
        let presses: Vec<_> = out.iter().filter_map(|e| match *e { PadEvent::Button { source: 5, button, pressed } => Some((button, pressed)), _ => None }).collect();
        assert_eq!(presses, vec![(Button::South, true), (Button::South, false), (Button::Down, true), (Button::Down, false)], "{out:?}");
        assert_eq!(presses.len(), out.len(), "{out:?}");
    }

    /// Read in time, every stick value goes on as it came (menus scroll with it, games get it).
    #[test]
    fn stick_values_read_in_time_go_on_as_they_came() {
        let mut dec = mini_v2();
        let out = read(&mut dec, &[(ABS, 0, 200), (ABS, 0, 255), (ABS, 0, 128)], Duration::ZERO);
        assert_eq!(out.len(), 3, "{out:?}");
        assert!(any_input(&out), "a real push counts");
    }

    /// A value read in time after late ones is newer: the late ones of that axis are dropped, not sent after it.
    #[test]
    fn a_value_read_in_time_replaces_what_waited() {
        let mut dec = mini_v2();
        let at = SystemTime::now();
        assert!(dec.event_read_at(5, InputEvent::new_now(ABS, 0, 255), at + Duration::from_secs(9)).is_empty());
        let fresh = dec.event_read_at(5, InputEvent::new_now(ABS, 0, 128), at);
        assert_eq!(fresh.len(), 1);
        assert!(!dec.has_backlog(), "the late 255 must not come after the newer 128");
    }

    /// A lying pad never counts as somebody using it: ten minutes of 100 Hz jitter around the Mini V2's resting
    /// offsets (left X rests at 142 = +0.11, the others near 128) and of a trigger at rest, read in time, through the
    /// menu's pad: nothing counts as input (sleep's timers, the dark screen, the UI's quiet spells), not even the
    /// menus' smaller dead zone's edge.
    #[test]
    fn ten_minutes_of_stick_and_trigger_jitter_never_count_as_input() {
        let mut dec = mini_v2();
        let mut menu = PadState::new(vec![]);
        menu.merge(PadEvent::Connected { source: 5, controller: Controller::builtin() });
        let mut seed = 0x2545_f491_u32;
        let mut noise = |spread: i32| {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            (seed % (2 * spread as u32 + 1)) as i32 - spread
        };
        let now = SystemTime::now();
        for _ in 0..60_000 {
            for (code, rest, spread) in [(0u16, 142, 8), (1, 128, 6), (3, 132, 6), (4, 126, 6), (2, 0, 3), (5, 0, 3)] {
                let value = (rest + noise(spread)).clamp(0, 255);
                for e in dec.event_read_at(5, InputEvent::new_now(ABS, code, value), now) {
                    assert!(!counts_as_input(&e), "jitter counted as input: {e:?}");
                    for merged in menu.merge(e) {
                        assert!(!counts_as_input(&merged), "merged jitter counted as input: {merged:?}");
                    }
                }
            }
        }
    }
}

/// ROCKNIX's InputPlumber on the RK3566 boards (20261101, `anbernic_gen1` and `powkiddy` maps) sends Guide with every SELECT.
#[cfg(test)]
mod rk3566_guide_echo_tests {
    use super::*;
    use std::time::{Duration, UNIX_EPOCH};

    fn at(ms: u64, code: KeyCode, value: i32) -> InputEvent {
        let e = InputEvent::new(evdev::EventType::KEY.0, code.0, value);
        // InputEvent::new has no time of its own: give it one, as the kernel does.
        let t = UNIX_EPOCH + Duration::from_millis(1_000_000 + ms);
        let d = t.duration_since(UNIX_EPOCH).unwrap();
        InputEvent::from(libc::input_event {
            time: libc::timeval { tv_sec: d.as_secs() as _, tv_usec: d.subsec_micros() as _ },
            type_: e.event_type().0, code: e.code(), value: e.value(),
        })
    }

    fn pad(echo: bool) -> Decoder {
        // InputPlumber's DS5 target: face buttons, SELECT/START/MODE, the stick clicks.
        Decoder::with_caps_profile((3, 0x054c, 0x0ce6, 0x8111), "Sony Interactive Entertainment DualSense Wireless Controller",
            vec![304, 305, 307, 308, 310, 311, 312, 313, 314, 315, 316, 317, 318], vec![], false).with_guide_echo(echo)
    }

    fn buttons(events: &[PadEvent]) -> Vec<(Button, bool)> {
        events.iter().filter_map(|e| match e { PadEvent::Button { button, pressed, .. } => Some((*button, *pressed)), _ => None }).collect()
    }

    #[test]
    fn the_guide_copy_of_select_is_dropped_and_select_reaches_the_game() {
        let mut d = pad(true);
        let mut got = Vec::new();
        for e in [at(0, KeyCode::BTN_SELECT, 1), at(1, KeyCode::BTN_MODE, 1), at(120, KeyCode::BTN_SELECT, 0), at(121, KeyCode::BTN_MODE, 0)] {
            got.extend(d.event(0, e));
        }
        assert_eq!(buttons(&got), [(Button::Select, true), (Button::Select, false)]);
        // A real HOME on its own still comes through (a Bluetooth pad on the same handheld), and so does SELECT + HOME later.
        let got: Vec<_> = [at(500, KeyCode::BTN_MODE, 1), at(560, KeyCode::BTN_MODE, 0)].into_iter().flat_map(|e| d.event(0, e)).collect();
        assert_eq!(buttons(&got), [(Button::Mode, true), (Button::Mode, false)]);
        let got: Vec<_> = [at(1000, KeyCode::BTN_SELECT, 1), at(1300, KeyCode::BTN_MODE, 1), at(1400, KeyCode::BTN_MODE, 0)].into_iter().flat_map(|e| d.event(0, e)).collect();
        assert_eq!(buttons(&got), [(Button::Select, true), (Button::Mode, true), (Button::Mode, false)]);
    }

    #[test]
    fn both_in_one_report_are_put_select_first() {
        let mut batch = vec![at(0, KeyCode::BTN_MODE, 1), at(0, KeyCode::BTN_SOUTH, 1), at(0, KeyCode::BTN_SELECT, 1)];
        select_before_guide(&mut batch);
        assert_eq!(batch.iter().map(|e| e.code()).collect::<Vec<_>>(), [KeyCode::BTN_SELECT.0, KeyCode::BTN_MODE.0, KeyCode::BTN_SOUTH.0]);
        let mut d = pad(true);
        let got: Vec<_> = batch.into_iter().flat_map(|e| d.event(0, e)).collect();
        assert_eq!(buttons(&got), [(Button::Select, true), (Button::South, true)]);
    }

    #[test]
    fn other_devices_keep_select_and_home_together() {
        let mut d = pad(false);
        let got: Vec<_> = [at(0, KeyCode::BTN_SELECT, 1), at(1, KeyCode::BTN_MODE, 1)].into_iter().flat_map(|e| d.event(0, e)).collect();
        assert_eq!(buttons(&got), [(Button::Select, true), (Button::Mode, true)]);
    }
}
