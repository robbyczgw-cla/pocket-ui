//! Controls of a hosted game: which Mini V2 button presses which button of the console, and whether
//! the left stick acts as a D-pad.
//!
//! The Mini V2's buttons are numbered like libretro's RetroPad (the Nintendo layout: A is the right
//! face button), so "physical button i" and "console button i" are the same number until the user
//! remaps. The console's own names for its buttons come from the core (`SET_INPUT_DESCRIPTORS`:
//! "A", "B", "Select", "D-Pad Up", …); pocket-core hands them to the pause menu in the snapshot file.
//!
//! Settings keys in the per-console / per-game files: `ctl.<button>` = `<button>` or `none`,
//! `ctl.stick_dpad` = `on` or `off`, `ctl.dpad_stick` = `on` or `off` (the D-pad moves the left stick), and `ctl.hold_toggle` = a console button or `none` (see `HoldToggle`).

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicI16, Ordering};

use serde::{Deserialize, Serialize};

use crate::libretro::joypad;
use crate::pad::{Axis, Button};

/// Prefix of control keys inside the per-console / per-game settings files.
pub const KEY_PREFIX: &str = "ctl.";
pub const STICK_KEY: &str = "stick_dpad";
/// D-pad as left stick: on a handheld without sticks the D-pad moves the left stick of an N64, PSP or Dreamcast game.
pub const DPAD_STICK_KEY: &str = "dpad_stick";
/// Hold to toggle (Controls page): the console button a press switches on and a second press off.
pub const HOLD_KEY: &str = "hold_toggle";

/// Key names by RetroPad id (0..15); the same names are the values a button can be mapped to.
pub const KEYS: [&str; 16] = ["b", "y", "select", "start", "up", "down", "left", "right", "a", "x", "l", "r", "l2", "r2", "l3", "r3"];

/// What each button is called on the Mini V2 (RetroPad id → name).
const LABELS: [&str; 16] = ["B", "Y", "Select", "Start", "D-pad up", "D-pad down", "D-pad left", "D-pad right", "A", "X", "L1", "R1", "L2", "R2", "L3", "R3"];

/// The order buttons are listed in the menu and offered as choices: face buttons, shoulders, system buttons, D-pad.
pub const MENU_ORDER: [u8; 16] = [8, 0, 9, 1, 10, 11, 12, 13, 14, 15, 2, 3, 4, 5, 6, 7];

/// Systems whose games need an analog stick. Everything else has no use for it, so the stick is a D-pad there.
/// PS1 is in the list because pocket-core plugs a DualShock into it (`hostsys`): the sticks go to the core.
/// ScummVM moves its cursor with the left stick; DOSBox Pure maps it to the game's joystick, mouse or keys.
/// Phone games (FreeJ2ME-Plus on RetroArch) take the left stick as the number keys 2 4 6 8, the D-pad being the arrows.
const ANALOG_SYSTEMS: &[&str] = &["psx", "n64", "dreamcast", "psp", "gamecube", "wii", "3ds", "ps2", "naomi", "atomiswave", "scummvm", "pc", "j2me"];

pub fn stick_dpad_default(system: &str) -> bool {
    !ANALOG_SYSTEMS.contains(&system)
}

/// Systems whose games can't be played without a stick (Mario 64 walks only with it). PS1 games play on the D-pad,
/// so it is not here.
const STICK_ONLY_SYSTEMS: &[&str] = &["n64", "psp", "dreamcast"];

/// D-pad as left stick: on for N64, PSP and Dreamcast games on a handheld that has no sticks (Brick, RG35XX Plus, …).
pub fn dpad_stick_default(system: &str, has_sticks: bool) -> bool {
    !has_sticks && STICK_ONLY_SYSTEMS.contains(&system)
}

/// The Controls page offers D-pad as left stick for every system whose games use a stick.
pub fn offers_dpad_stick(system: &str) -> bool {
    ANALOG_SYSTEMS.contains(&system)
}

/// D-pad as left stick: a held D-pad direction is let go as a button and pushes the left stick all the way instead
/// (diagonals at the stick's round edge). A real left stick that is moved wins. `buttons` are the pad's own (bit i =
/// RetroPad id i), `sticks` the core's (left x, left y, right x, right y), y down.
pub fn dpad_to_stick(buttons: u16, sticks: [i16; 4]) -> (u16, [i16; 4]) {
    let held = |id: u8| buttons & (1 << id) != 0;
    let x = held(joypad::RIGHT as u8) as i32 - held(joypad::LEFT as u8) as i32;
    let y = held(joypad::DOWN as u8) as i32 - held(joypad::UP as u8) as i32;
    let buttons = buttons & !(0b1111 << 4);
    if (x, y) == (0, 0) || sticks[0] != 0 || sticks[1] != 0 {
        return (buttons, sticks);
    }
    let full = if x != 0 && y != 0 { 23170 } else { 32767 };
    (buttons, [(x * full) as i16, (y * full) as i16, sticks[2], sticks[3]])
}

pub fn physical_label(id: u8) -> &'static str {
    LABELS.get(id as usize).copied().unwrap_or("?")
}

/// The name for a console button when the core gave none.
pub fn standard_name(id: u8) -> &'static str {
    match id {
        0..=3 | 8..=9 => LABELS[id as usize],
        4 => "D-pad up",
        5 => "D-pad down",
        6 => "D-pad left",
        7 => "D-pad right",
        10 => "L",
        11 => "R",
        _ => LABELS.get(id as usize).copied().unwrap_or("?"),
    }
}

pub fn key_id(key: &str) -> Option<u8> {
    KEYS.iter().position(|k| *k == key).map(|i| i as u8)
}

/// One button of the console as its core names it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Target {
    pub id: u8,
    pub label: String,
}

/// Part of the snapshot file: the console's buttons and the layout in use.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct ControlsFile {
    /// Buttons the core describes for player 1, in menu order. Empty: the core named none and the standard set applies.
    #[serde(default)]
    pub targets: Vec<Target>,
    /// Mini V2 button key → console button key or `none`.
    #[serde(default)]
    pub mapping: BTreeMap<String, String>,
    #[serde(default)]
    pub stick_dpad: bool,
    /// D-pad as left stick (`dpad_to_stick`).
    #[serde(default)]
    pub dpad_stick: bool,
    /// The console button (RetroPad id) that Hold to toggle works on; None: off.
    #[serde(default)]
    pub hold_toggle: Option<u8>,
}

/// What `retro_input_descriptor` says about one button (player 1, RetroPad only).
#[derive(Clone, Debug, PartialEq)]
pub struct Described {
    pub id: u8,
    pub label: String,
}

/// Console buttons the menu offers, in menu order. A core without descriptors gets the standard set.
pub fn targets_from(described: &[Described]) -> Vec<Target> {
    MENU_ORDER
        .iter()
        .filter_map(|&id| match described.iter().find(|d| d.id == id) {
            Some(d) => Some(Target { id, label: d.label.clone() }),
            None if described.is_empty() => Some(Target { id, label: standard_name(id).to_string() }),
            None => None,
        })
        .collect()
}

/// `targets_from`, but a core that describes nothing gets its console's own pad where `hostsys` knows it
/// (the Saturn's A B C X Y Z L R Start and the D-pad) instead of the standard set.
pub fn targets_for(system: &str, described: &[Described]) -> Vec<Target> {
    let pad = crate::platform::console_buttons(system);
    if !described.is_empty() || pad.is_empty() {
        let mut targets = targets_from(described);
        if system == "nds" { targets.extend(crate::ds_pad::ACTIONS.iter().map(|a| Target { id: a.0, label: a.2.into() })); }
        return targets;
    }
    MENU_ORDER
        .iter()
        .filter_map(|&id| match pad.iter().find(|(i, _)| *i == id) {
            Some((_, name)) => Some(Target { id, label: name.to_string() }),
            None if (4..=7).contains(&id) => Some(Target { id, label: standard_name(id).to_string() }),
            None => None,
        })
        .collect()
}

/// Hold to toggle only accepts real console buttons, not navigation or frontend actions.
pub fn can_hold(id: u8) -> bool { id < 16 && !(2..8).contains(&id) }

/// What a game gets from the handheld's pad in one frame, Options › Controls applied (Android's player; pocket-core does
/// the same with its own input): the pad's held buttons and its presses shorter than a frame (`pulses`), the stick's
/// D-pad presses, all through `Mapping::translate`; the touch pad's buttons as they are (they are the console's own);
/// D-pad as left stick moving `sticks`. Returns the buttons and the sticks to give the core when they changed.
pub fn pad_input(mapping: &Mapping, held: u16, pulses: u16, stick: u16, touch: u16, sticks: [i16; 4]) -> (u16, Option<[i16; 4]>) {
    let buttons = mapping.translate(held | pulses | stick) | touch;
    if mapping.dpad_stick {
        let (b, s) = dpad_to_stick(buttons, sticks);
        return (b, Some(s));
    }
    (buttons, None)
}

/// The pause menu's Controls page (Options › Controls): the sticks' rows, what each of the handheld's buttons presses,
/// Hold to toggle, Reset controls. Linux's menu (`emu_menu`, which then tells pocket-core) and Android's player (which
/// applies it itself) show and step the same page.
#[derive(Clone, Debug, PartialEq)]
pub struct ControlsPage {
    pub mapping: Mapping,
    /// The console's buttons the core describes (`targets_for`); empty: the standard set.
    pub targets: Vec<Target>,
    pub system: String,
}

/// What a step on the Controls page changed: entries (key without `KEY_PREFIX`, value, back to the default), or every
/// entry back to its default (Reset controls).
#[derive(Clone, Debug, PartialEq)]
pub enum ControlsChange {
    Set(Vec<(&'static str, String, bool)>),
    Reset,
}

impl ControlsPage {
    /// The console buttons a handheld button can be given: nothing, or any button the core describes.
    fn choices(&self) -> Vec<Option<u8>> {
        let mut c = vec![None];
        c.extend(self.targets.iter().map(|t| Some(t.id)));
        if self.system == "nds" {
            if self.targets.is_empty() { c.extend(targets_from(&[]).iter().map(|t| Some(t.id))); }
            for (id, _, _) in crate::ds_pad::ACTIONS { if !c.contains(&Some(id)) { c.push(Some(id)); } }
        }
        c
    }

    /// A console button's name: the core's, or the standard one when the core named none.
    pub fn target_name(&self, id: Option<u8>) -> String {
        if self.system == "nds" { if let Some(name) = id.and_then(crate::ds_pad::name) { return name.into(); } }
        match id {
            Some(id) => match self.targets.iter().find(|t| t.id == id) {
                Some(t) => t.label.clone(),
                // Not a button this console has, so pressing it does nothing.
                None if !self.targets.is_empty() => "None".into(),
                None => standard_name(id).into(),
            },
            None => "None".into(),
        }
    }

    /// The buttons Hold to toggle can hold: not the D-pad, not Select and Start.
    fn hold_choices(&self) -> Vec<Option<u8>> {
        let mut c = vec![None];
        if self.targets.is_empty() {
            c.extend(MENU_ORDER.iter().copied().filter(|id| can_hold(*id) && *id < 14).map(Some));
        } else {
            c.extend(self.targets.iter().map(|t| t.id).filter(|id| can_hold(*id)).map(Some));
        }
        c
    }

    /// Rows before the buttons: "Left stick as D-pad", and "D-pad as left stick" for a system whose games use a stick.
    pub fn stick_rows(&self) -> usize {
        1 + offers_dpad_stick(&self.system) as usize
    }

    /// How many rows the page has.
    pub fn count(&self) -> usize {
        MENU_ORDER.len() + 2 + self.stick_rows()
    }

    /// Row 0 is the stick, then D-pad as left stick (`stick_rows`), the handheld's buttons in `MENU_ORDER`, then "Hold to
    /// toggle" and "Reset controls": (name, value, explanation).
    pub fn rows(&self) -> Vec<(String, String, String)> {
        let mut rows = vec![(
            "Left stick as D-pad".to_string(),
            if self.mapping.stick_dpad { "On" } else { "Off" }.to_string(),
            "The left stick works as the D-pad.\nTurn it off for games that use an analogue stick.".to_string(),
        )];
        if self.stick_rows() == 2 {
            rows.push((
                "D-pad as left stick".into(),
                if self.mapping.dpad_stick { "On" } else { "Off" }.into(),
                "The D-pad moves the left stick, for games that need one.\nOn by itself on a handheld without sticks.".into(),
            ));
        }
        for &id in &MENU_ORDER {
            let target = self.mapping.target[id as usize];
            let name = self.target_name(target);
            let phys = physical_label(id);
            let desc = match target {
                Some(crate::ds_pad::STYLUS) => "Switch the D-pad between the game and the DS stylus. In stylus mode, the button mapped to A touches and holds.".into(),
                Some(crate::ds_pad::LAYOUT) => "Tap to swap the two Nintendo DS screens. Hold to show the next layout.".into(),
                _ if name == "None" => format!("{phys} does nothing in this game."),
                _ => format!("{phys} presses {name}."),
            };
            let extra = match id {
                14 => "\nL3 means pressing the left stick in.",
                15 => "\nR3 means pressing the right stick in.",
                _ => "",
            };
            let mut desc = format!("{desc}{extra}");
            desc.push('\n');
            desc.push_str("An occupied action swaps with this button's previous action.");
            rows.push((phys.into(), name, desc));
        }
        rows.push((
            "Hold to toggle".into(),
            self.mapping.hold_toggle.map_or("Off".into(), |id| self.target_name(Some(id))),
            "Pick a button: tap once to hold, tap again to let go.\nHandy for run or charge buttons.".into(),
        ));
        rows.push(("Reset controls".into(), String::new(), if self.system == "nds" { "Restore the DS buttons and actions." } else { "Every button does what it says, as it did before any change." }.into()));
        rows
    }

    /// Left/Right (`delta`) or A on row `sel`: the stick rows switch, a button gets the next console button (an occupied
    /// one swaps), Hold to toggle the next button it can hold, Reset controls puts everything back.
    pub fn step(&mut self, sel: usize, delta: i32) -> ControlsChange {
        let defaults = Mapping::defaults(&self.system);
        if sel == 0 {
            self.mapping.stick_dpad = !self.mapping.stick_dpad;
            return ControlsChange::Set(vec![(STICK_KEY, on_off(self.mapping.stick_dpad).into(), self.mapping.stick_dpad == defaults.stick_dpad)]);
        }
        let sel = sel + 2 - self.stick_rows();
        if sel == 1 {
            self.mapping.dpad_stick = !self.mapping.dpad_stick;
            ControlsChange::Set(vec![(DPAD_STICK_KEY, on_off(self.mapping.dpad_stick).into(), self.mapping.dpad_stick == defaults.dpad_stick)])
        } else if sel <= MENU_ORDER.len() + 1 {
            let id = MENU_ORDER[sel - 2];
            let choices = self.choices();
            let current = self.mapping.target[id as usize];
            let now = choices.iter().position(|c| *c == current).unwrap_or(0) as i32;
            let next = choices[(now + delta).rem_euclid(choices.len() as i32) as usize];
            let changed = self.mapping.assign(id, next);
            ControlsChange::Set(changed.into_iter().map(|c| (KEYS[c as usize], self.mapping.value_of(c).to_string(), self.mapping.target[c as usize] == defaults.target[c as usize])).collect())
        } else if sel == MENU_ORDER.len() + 2 {
            let choices = self.hold_choices();
            let now = choices.iter().position(|c| *c == self.mapping.hold_toggle).unwrap_or(0) as i32;
            self.mapping.hold_toggle = choices[(now + delta).rem_euclid(choices.len() as i32) as usize];
            ControlsChange::Set(vec![(HOLD_KEY, self.mapping.hold_value().into(), self.mapping.hold_toggle == defaults.hold_toggle)])
        } else {
            self.mapping = defaults;
            ControlsChange::Reset
        }
    }
}

fn on_off(on: bool) -> &'static str {
    if on {
        "on"
    } else {
        "off"
    }
}

/// Which console button each Mini V2 button presses.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mapping {
    pub target: [Option<u8>; 16],
    pub stick_dpad: bool,
    /// D-pad as left stick (`dpad_to_stick`).
    pub dpad_stick: bool,
    /// Hold to toggle: the console button that stays down after a press and lets go at the next one (`HoldToggle`).
    pub hold_toggle: Option<u8>,
}

impl Mapping {
    pub fn defaults(system: &str) -> Mapping {
        // The Android edition has no device table (`device_model` only): its handhelds bring their own pads, and the
        // D-pad stays a D-pad until the player turns it on.
        #[cfg(all(target_os = "android", not(feature = "handheld")))]
        let has_sticks = true;
        #[cfg(not(all(target_os = "android", not(feature = "handheld"))))]
        let has_sticks = crate::device::current().has_sticks();
        Mapping::defaults_on(system, has_sticks)
    }

    /// `defaults` on a handheld with or without sticks.
    pub fn defaults_on(system: &str, has_sticks: bool) -> Mapping {
        let mut m = Mapping { target: std::array::from_fn(|i| Some(i as u8)), stick_dpad: stick_dpad_default(system), dpad_stick: dpad_stick_default(system, has_sticks), hold_toggle: None };
        if system == "nds" && !has_sticks { m.target[12] = Some(crate::ds_pad::STYLUS); m.target[13] = Some(crate::ds_pad::LAYOUT); }
        m
    }

    /// The default layout with the `ctl.*` entries of a settings map applied.
    pub fn from_values(system: &str, values: &BTreeMap<String, String>) -> Mapping {
        let mut m = Mapping::defaults(system);
        for (k, v) in values {
            if let Some(k) = k.strip_prefix(KEY_PREFIX) {
                m.set(k, v);
            }
        }
        m
    }

    /// Applies one entry (key without prefix). False for anything unknown.
    pub fn set(&mut self, key: &str, value: &str) -> bool {
        if key == STICK_KEY {
            self.stick_dpad = matches!(value, "on" | "true" | "1");
            return matches!(value, "on" | "off" | "true" | "false" | "1" | "0");
        }
        if key == DPAD_STICK_KEY {
            self.dpad_stick = matches!(value, "on" | "true" | "1");
            return matches!(value, "on" | "off" | "true" | "false" | "1" | "0");
        }
        if key == HOLD_KEY {
            // Only the buttons a game holds down: not the D-pad, Select or Start.
            self.hold_toggle = match value {
                "none" => None,
                v => match key_id(v).filter(|id| !(2..=7).contains(id)) {
                    Some(id) => Some(id),
                    None => return false,
                },
            };
            return true;
        }
        let Some(id) = key_id(key) else { return false };
        match value {
            "none" => self.target[id as usize] = None,
            v => match key_id(v).or_else(|| crate::ds_pad::ACTIONS.iter().find(|a| a.1 == v).map(|a| a.0)) {
                Some(t) => self.target[id as usize] = Some(t),
                None => return false,
            },
        }
        true
    }

    pub fn value_of(&self, id: u8) -> &'static str {
        self.target[id as usize].map_or("none", |t| KEYS.get(t as usize).copied().or_else(|| crate::ds_pad::ACTIONS.iter().find(|a| a.0 == t).map(|a| a.1)).unwrap_or("none"))
    }

    /// Interactive assignment: move the previous action to the occupied button. Legacy duplicate
    /// mappings remain readable; additional occupants are cleared so the new choice is unambiguous.
    pub fn assign(&mut self, id: u8, next: Option<u8>) -> Vec<u8> {
        let old = self.target[id as usize];
        if old == next { return Vec::new(); }
        let mut changed = vec![id];
        let mut replacement = old;
        if let Some(next) = next {
            for other in 0..16u8 {
                if other != id && self.target[other as usize] == Some(next) {
                    self.target[other as usize] = replacement;
                    replacement = None;
                    changed.push(other);
                }
            }
        }
        self.target[id as usize] = next;
        changed
    }

    /// Mini V2 buttons (bit i = button i) → console buttons.
    pub fn translate(&self, pressed: u16) -> u16 {
        let mut out = 0;
        for (i, target) in self.target.iter().enumerate() {
            if let (true, Some(t)) = (pressed & (1 << i) != 0, target) {
                if *t < 16 { out |= 1 << t; }
            }
        }
        out
    }

    pub fn to_file(&self, targets: Vec<Target>) -> ControlsFile {
        ControlsFile {
            targets,
            mapping: (0..16u8).map(|i| (KEYS[i as usize].to_string(), self.value_of(i).to_string())).collect(),
            stick_dpad: self.stick_dpad,
            dpad_stick: self.dpad_stick,
            hold_toggle: self.hold_toggle,
        }
    }

    pub fn from_file(f: &ControlsFile) -> Mapping {
        let mut m = Mapping::defaults("");
        for (k, v) in &f.mapping {
            m.set(k, v);
        }
        m.stick_dpad = f.stick_dpad;
        m.dpad_stick = f.dpad_stick;
        m.hold_toggle = f.hold_toggle.filter(|id| (*id as usize) < KEYS.len() && !(2..=7).contains(id));
        m
    }

    /// `hold_toggle` as a settings value: a console button's key or `none`.
    pub fn hold_value(&self) -> &'static str {
        self.hold_toggle.and_then(|id| KEYS.get(id as usize).copied()).unwrap_or("none")
    }
}

/// Hold to toggle (Controls page, for people who can't keep a button down, or don't want to): the console button
/// chosen there is pressed by a press and let go by the next, instead of being held. One per player. A press while the
/// game isn't running (menu, sleep) is not seen: pocket-core calls `clear` then, so nothing stays pressed.
#[derive(Default, Clone, Copy, Debug)]
pub struct HoldToggle {
    /// Pressed on the pad before (only the chosen button).
    down: u16,
    /// Held down for the game now.
    on: u16,
}

impl HoldToggle {
    /// `pressed` are the console buttons the player holds, `chosen` the button Hold to toggle works on. Returns what
    /// the game gets: everything else as it is, the chosen button as the toggle has it.
    pub fn apply(&mut self, pressed: u16, chosen: Option<u8>) -> u16 {
        let mask = chosen.filter(|id| *id < 16).map_or(0, |id| 1u16 << id);
        // A change of button (or off) lets the old one go.
        self.on &= mask;
        let now = pressed & mask;
        self.on ^= now & !self.down;
        self.down = now;
        (pressed & !mask) | self.on
    }

    /// HOME shortcuts observe physical edges without changing the latch. Releasing HOME before the shortcut
    /// button must not turn that same press into a new game press.
    pub fn shortcut(&mut self, pressed: u16, chosen: Option<u8>) {
        let mask = chosen.filter(|id| *id < 16).map_or(0, |id| 1u16 << id);
        self.on &= mask;
        self.down = pressed & mask;
    }

    /// The button held down for the game now, if any (the badge over the game says so).
    pub fn held(&self) -> Option<u8> {
        (self.on != 0).then(|| self.on.trailing_zeros() as u8)
    }

    /// Lets go of what is held down for the game (a pause, a sleep, a dark screen, a loaded state).
    pub fn clear(&mut self) {
        self.on = 0;
    }
}

/// The RetroPad button a pad button is, Nintendo layout: libretro A is the right face button, B the bottom one.
/// (Moved here from pocket-core for `players::Inputs`.)
pub fn retropad_bit(b: Button) -> Option<u16> {
    Some(match b {
        Button::East => joypad::A,
        Button::South => joypad::B,
        Button::North => joypad::X,
        Button::West => joypad::Y,
        Button::Up => joypad::UP,
        Button::Down => joypad::DOWN,
        Button::Left => joypad::LEFT,
        Button::Right => joypad::RIGHT,
        Button::Select => joypad::SELECT,
        Button::Start => joypad::START,
        Button::L1 => joypad::L,
        Button::R1 => joypad::R,
        Button::L2 => joypad::L2,
        Button::R2 => joypad::R2,
        Button::L3 => joypad::L3,
        Button::R3 => joypad::R3,
        _ => return None,
    })
}

/// A mask of pad buttons (`Button::bit`) as RetroPad bits; HOME, BACK, power and volume are not the game's.
pub fn retropad(buttons: u32) -> u16 {
    Button::ALL.iter().filter(|b| buttons & b.bit() != 0).filter_map(|b| retropad_bit(*b)).fold(0, |m, bit| m | 1 << bit)
}

/// Left stick → D-pad presses, with a little hysteresis so a stick resting near the edge doesn't flutter.
#[derive(Default)]
pub struct StickPad {
    up: bool,
    down: bool,
    left: bool,
    right: bool,
}

impl StickPad {
    const PRESS: f32 = 0.55;
    const RELEASE: f32 = 0.40;

    fn edge(was: bool, v: f32) -> bool {
        if was {
            v > Self::RELEASE
        } else {
            v > Self::PRESS
        }
    }

    /// Stick position (x right, y down, -1..1) → D-pad bits in Mini V2 numbering.
    pub fn update(&mut self, x: f32, y: f32) -> u16 {
        self.up = Self::edge(self.up, -y);
        self.down = Self::edge(self.down, y);
        self.left = Self::edge(self.left, -x);
        self.right = Self::edge(self.right, x);
        (self.up as u16) << 4 | (self.down as u16) << 5 | (self.left as u16) << 6 | (self.right as u16) << 7
    }
}

/// The D-pad of a Pocket held turned (Options → Picture → Screen orientation, `turns` quarter turns as the picture's,
/// `video::Orientation::turns`): each direction pressed becomes the one it points to for the player, so up is up in
/// the turned picture. A Pocket turned right (1) points its D-pad's up to the player's right; turned left (3), to
/// their left. Other buttons stay.
pub fn turn_buttons(pressed: u16, turns: u32) -> u16 {
    use joypad::{DOWN, LEFT, RIGHT, UP};
    // Where up, right, down and left point after one turn to the right: each to the next.
    const ROUND: [u16; 4] = [UP, RIGHT, DOWN, LEFT];
    let turns = (turns & 3) as usize;
    if turns == 0 {
        return pressed;
    }
    let dpad = ROUND.iter().fold(0u16, |m, b| m | 1 << b);
    let mut out = pressed & !dpad;
    for (i, b) in ROUND.iter().enumerate() {
        if pressed & (1 << b) != 0 {
            out |= 1 << ROUND[(i + turns) % 4];
        }
    }
    out
}

/// A stick of a Pocket held turned (`turn_buttons`): (x, y), y down, as the player pushes it.
pub fn turn_stick(x: i16, y: i16, turns: u32) -> (i16, i16) {
    match turns & 3 {
        1 => (y.saturating_neg(), x),
        2 => (x.saturating_neg(), y.saturating_neg()),
        3 => (y, x.saturating_neg()),
        _ => (x, y),
    }
}

/// Both sticks of `analog` (`Sticks::analog`'s order) turned with `turn_stick`.
pub fn turn_sticks(a: [i16; 4], turns: u32) -> [i16; 4] {
    let (lx, ly) = turn_stick(a[0], a[1], turns);
    let (rx, ry) = turn_stick(a[2], a[3], turns);
    [lx, ly, rx, ry]
}

/// Stick positions as the pad threads report them (-1..1 scaled to ±32767).
#[derive(Default)]
pub struct Sticks {
    lx: AtomicI16,
    ly: AtomicI16,
    rx: AtomicI16,
    ry: AtomicI16,
}

impl Sticks {
    pub fn set(&self, axis: Axis, value: f32) {
        let v = (value.clamp(-1.0, 1.0) * 32767.0) as i16;
        match axis {
            Axis::LeftX => self.lx.store(v, Ordering::Relaxed),
            Axis::LeftY => self.ly.store(v, Ordering::Relaxed),
            Axis::RightX => self.rx.store(v, Ordering::Relaxed),
            Axis::RightY => self.ry.store(v, Ordering::Relaxed),
            Axis::L2 | Axis::R2 => {}
        }
    }

    /// Left stick as -1..1.
    pub fn left(&self) -> (f32, f32) {
        (self.lx.load(Ordering::Relaxed) as f32 / 32767.0, self.ly.load(Ordering::Relaxed) as f32 / 32767.0)
    }

    /// (left x, left y, right x, right y) for a core's analog inputs, dead zone removed. The left
    /// stick reads as centred while it serves as a D-pad.
    pub fn analog(&self, left_too: bool) -> [i16; 4] {
        let dz = crate::pad::STICK_DEADZONE;
        let conv = |v: i16| -> i16 {
            let f = v as f32 / 32767.0;
            if f.abs() < dz {
                0
            } else {
                (f.signum() * (f.abs() - dz) / (1.0 - dz) * 32767.0) as i16
            }
        };
        let (lx, ly) = (self.lx.load(Ordering::Relaxed), self.ly.load(Ordering::Relaxed));
        [if left_too { conv(lx) } else { 0 }, if left_too { conv(ly) } else { 0 }, conv(self.rx.load(Ordering::Relaxed)), conv(self.ry.load(Ordering::Relaxed))]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assigning_a_taken_game_action_swaps_and_saves_both_buttons() {
        let mut m = Mapping::defaults("snes");
        assert_eq!(m.assign(12, Some(8)), [12, 8]);
        assert_eq!(m.translate(1 << 12), 1 << 8, "L2 presses A");
        assert_eq!(m.translate(1 << 8), 1 << 12, "A keeps the previous L2 action");
        assert_eq!(Mapping::from_file(&m.to_file(vec![])), m);
        m.assign(12, None);
        assert_eq!(m.translate(1 << 12), 0);
        assert_eq!(m.translate(1 << 8), 1 << 12, "unassigning one button does not clear another");
    }

    #[test]
    fn hold_to_toggle_presses_on_one_press_and_lets_go_on_the_next() {
        use joypad::{B, UP};
        let (b, up) = (1u16 << B, 1u16 << UP);
        let mut h = HoldToggle::default();
        // Off: nothing changes.
        assert_eq!(h.apply(b | up, None), b | up);
        assert_eq!(h.apply(0, None), 0);
        // On B: a press holds it, also after the finger is up; the other buttons are as they are.
        assert_eq!(h.apply(b, Some(B as u8)), b);
        assert_eq!(h.apply(0, Some(B as u8)), b, "held for the game");
        assert_eq!(h.apply(up, Some(B as u8)), b | up);
        assert_eq!(h.apply(up | b, Some(B as u8)), up, "the second press lets go");
        assert_eq!(h.apply(up, Some(B as u8)), up, "and the finger staying down does not press it again");
        assert_eq!(h.apply(0, Some(B as u8)), 0);
        // Pressed again, then cleared (a pause): not held on afterwards.
        assert_eq!(h.apply(b, Some(B as u8)), b);
        h.clear();
        assert_eq!(h.apply(0, Some(B as u8)), 0);
        // The finger down through the clear is no new press; one after it is.
        assert_eq!(h.apply(b, Some(B as u8)), b);
        h.clear();
        assert_eq!(h.apply(b, Some(B as u8)), 0, "still the same press");
        assert_eq!(h.apply(0, Some(B as u8)), 0);
        assert_eq!(h.apply(b, Some(B as u8)), b);
        // Switching the option off, or to another button, lets the old one go.
        assert_eq!(h.held(), Some(B as u8), "held: the badge says which");
        assert_eq!(h.apply(0, Some(B as u8)), b);
        assert_eq!(h.apply(0, None), 0);
        assert_eq!(h.held(), None, "let go: the badge goes");
    }

    #[test]
    fn home_shortcuts_never_latch_and_their_release_does_not_cancel_a_latch() {
        let b = 1 << joypad::B;
        let chosen = Some(joypad::B as u8);
        let mut h = HoldToggle::default();
        h.shortcut(b, chosen);
        assert_eq!(h.apply(b, chosen), 0, "HOME released first: still the shortcut press");
        assert_eq!(h.apply(0, chosen), 0);
        assert_eq!(h.apply(b, chosen), b, "a new game press latches");
        h.apply(0, chosen);
        h.shortcut(b, chosen);
        h.shortcut(0, chosen);
        assert_eq!(h.apply(0, chosen), b, "a shortcut cannot release a running button either");
        h.clear();
        assert_eq!(h.apply(0, chosen), 0, "a load, restart or pause releases it");
    }

    #[test]
    fn old_and_invalid_control_snapshots_cannot_enable_an_invalid_latch() {
        let mut f: ControlsFile = serde_json::from_str(r#"{"targets":[],"mapping":{}}"#).unwrap();
        assert_eq!(Mapping::from_file(&f).hold_toggle, None, "old snapshots default to Off");
        for id in [2, 7, 16, 255] {
            f.hold_toggle = Some(id);
            assert_eq!(Mapping::from_file(&f).hold_value(), "none");
        }
    }

    #[test]
    fn hold_to_toggle_is_a_setting_of_a_game_button() {
        let mut m = Mapping::defaults("snes");
        assert_eq!((m.hold_toggle, m.hold_value()), (None, "none"));
        assert!(m.set(HOLD_KEY, "b"));
        assert_eq!((m.hold_toggle, m.hold_value()), (Some(joypad::B as u8), "b"));
        assert!(!m.set(HOLD_KEY, "up"), "the D-pad, Select and Start are not held down");
        assert!(!m.set(HOLD_KEY, "start") && !m.set(HOLD_KEY, "nonsense"));
        assert_eq!(m.hold_toggle, Some(joypad::B as u8), "a refused value changes nothing");
        assert!(m.set(HOLD_KEY, "none"));
        assert_eq!(m.hold_toggle, None);
        let values: BTreeMap<String, String> = [("ctl.hold_toggle".to_string(), "r".to_string())].into();
        let from_values = Mapping::from_values("gba", &values);
        assert_eq!(from_values.hold_toggle, Some(joypad::R as u8));
        assert_eq!(Mapping::from_file(&from_values.to_file(Vec::new())), from_values, "the snapshot file carries it");
    }

    #[test]
    fn without_sticks_the_d_pad_walks_in_n64_psp_and_dreamcast_games() {
        // Fable 5.2 gaps #8: Brick, RG35XX Plus/SP, RG28XX have no stick; Mario 64 only walks with one.
        for system in ["n64", "psp", "dreamcast"] {
            assert!(Mapping::defaults_on(system, false).dpad_stick, "{system} without sticks");
            assert!(!Mapping::defaults_on(system, true).dpad_stick, "{system}: a handheld with sticks keeps its D-pad");
        }
        for system in ["psx", "snes", "gba", "scummvm"] {
            assert!(!Mapping::defaults_on(system, false).dpad_stick, "{system} plays on the D-pad");
        }
        assert!(offers_dpad_stick("n64") && offers_dpad_stick("psx") && !offers_dpad_stick("gb"));
        let mut m = Mapping::defaults_on("n64", false);
        assert!(m.set(DPAD_STICK_KEY, "off") && !m.dpad_stick);
        assert!(m.set(DPAD_STICK_KEY, "on") && m.dpad_stick);
        assert_eq!(Mapping::from_file(&m.to_file(Vec::new())).dpad_stick, true, "the snapshot file carries it");
        let values: BTreeMap<String, String> = [("ctl.dpad_stick".to_string(), "on".to_string())].into();
        assert!(Mapping::from_values("psx", &values).dpad_stick);
    }

    #[test]
    fn the_d_pad_pushes_the_left_stick_all_the_way_and_is_no_button_then() {
        use joypad::{A, DOWN, LEFT, RIGHT, UP};
        let bits = |l: &[u16]| l.iter().fold(0u16, |m, b| m | 1 << b);
        assert_eq!(dpad_to_stick(bits(&[RIGHT, A]), [0; 4]), (bits(&[A]), [32767, 0, 0, 0]));
        assert_eq!(dpad_to_stick(bits(&[UP]), [0, 0, 5, 6]), (0, [0, -32767, 5, 6]), "up is negative y; the right stick stays");
        assert_eq!(dpad_to_stick(bits(&[DOWN, LEFT]), [0; 4]), (0, [-23170, 23170, 0, 0]), "a diagonal stays on the stick's round edge");
        assert_eq!(dpad_to_stick(bits(&[LEFT, RIGHT]), [0; 4]), (0, [0; 4]), "opposite directions cancel");
        assert_eq!(dpad_to_stick(bits(&[A]), [0; 4]), (bits(&[A]), [0; 4]));
        assert_eq!(dpad_to_stick(bits(&[UP]), [100, 0, 0, 0]), (0, [100, 0, 0, 0]), "a stick that is moved wins");
    }

    #[test]
    fn a_turned_pocket_turns_its_d_pad_to_where_the_player_points() {
        use joypad::{A, B, DOWN, LEFT, RIGHT, UP};
        let bits = |l: &[u16]| l.iter().fold(0u16, |m, b| m | 1 << b);
        // Turned right (1): the D-pad is on top, its up points to the player's right.
        for (pressed, seen) in [(UP, RIGHT), (RIGHT, DOWN), (DOWN, LEFT), (LEFT, UP)] {
            assert_eq!(turn_buttons(bits(&[pressed]), 1), bits(&[seen]), "right: {pressed}");
            // Turned left (3) the other way round.
            assert_eq!(turn_buttons(bits(&[seen]), 3), bits(&[pressed]), "left: {seen}");
        }
        // Diagonals turn whole, the other buttons stay, Normal changes nothing.
        assert_eq!(turn_buttons(bits(&[UP, LEFT, A]), 1), bits(&[RIGHT, UP, A]));
        assert_eq!(turn_buttons(bits(&[UP, B]), 2), bits(&[DOWN, B]));
        assert_eq!(turn_buttons(0xffff, 1), 0xffff);
        for b in 0..16 {
            assert_eq!(turn_buttons(1 << b, 0), 1 << b);
            assert_eq!(turn_buttons(turn_buttons(1 << b, 1), 3), 1 << b, "turning back undoes it");
        }
    }

    #[test]
    fn a_turned_pocket_turns_its_sticks_like_its_d_pad() {
        // Up on the stick (y negative) is right in the game when the Pocket is turned right, left when turned left.
        assert_eq!(turn_stick(0, -32767, 1), (32767, 0));
        assert_eq!(turn_stick(0, -32767, 3), (-32767, 0));
        assert_eq!(turn_stick(32767, 0, 1), (0, 32767), "right on the stick is down");
        assert_eq!(turn_stick(1000, -2000, 0), (1000, -2000));
        assert_eq!(turn_stick(i16::MIN, i16::MIN, 2), (i16::MAX, i16::MAX), "no overflow at the end of the range");
        assert_eq!(turn_sticks([0, -100, 50, 0], 1), [100, 0, 0, 50]);
        // The same turn as the D-pad's: a stick pushed like a pressed direction lands on the same new direction.
        let dir = |x: i16, y: i16| match (x.signum(), y.signum()) {
            (0, -1) => joypad::UP,
            (0, 1) => joypad::DOWN,
            (-1, 0) => joypad::LEFT,
            _ => joypad::RIGHT,
        };
        for t in [1, 3] {
            for (x, y) in [(0, -1), (1, 0), (0, 1), (-1, 0)] {
                let (tx, ty) = turn_stick(x, y, t);
                assert_eq!(1 << dir(tx, ty), turn_buttons(1 << dir(x, y), t), "turn {t}: {x},{y}");
            }
        }
    }

    #[test]
    fn default_layout_is_the_identity() {
        let m = Mapping::defaults("gbc");
        assert!((0..16).all(|i| m.target[i] == Some(i as u8)));
        assert_eq!(m.translate(0b1010_0000_0001), 0b1010_0000_0001);
        assert!(m.stick_dpad, "no analog on a Game Boy");
        assert!(!Mapping::defaults("n64").stick_dpad && !Mapping::defaults("dreamcast").stick_dpad);
        assert!(!Mapping::defaults("psx").stick_dpad && Mapping::defaults("snes").stick_dpad);
    }

    #[test]
    fn mouse_systems_get_the_left_stick() {
        // ScummVM moves its cursor with the left stick, DOSBox Pure maps it to the game's joystick or keys.
        assert!(!Mapping::defaults("scummvm").stick_dpad);
        assert!(!Mapping::defaults("pc").stick_dpad);
        // PICO-8 and TIC-80 have a D-pad only.
        assert!(Mapping::defaults("pico-8").stick_dpad && Mapping::defaults("tic-80").stick_dpad);
    }

    #[test]
    fn remapping_moves_none_and_merges() {
        let mut m = Mapping::defaults("snes");
        assert!(m.set("x", "a"));
        assert!(m.set("y", "none"));
        assert!(!m.set("x", "banana") && !m.set("banana", "a"));
        // Mini V2 X (9) now presses A (8); Y (1) nothing; A (8) still A.
        assert_eq!(m.translate(1 << 9), 1 << 8);
        assert_eq!(m.translate(1 << 1), 0);
        assert_eq!(m.translate((1 << 9) | (1 << 8)), 1 << 8, "two buttons onto one");
        assert_eq!((m.value_of(9), m.value_of(1), m.value_of(8)), ("a", "none", "a"));
    }

    #[test]
    fn swapping_works() {
        let mut m = Mapping::defaults("nes");
        m.set("a", "b");
        m.set("b", "a");
        assert_eq!(m.translate(1 << 8), 1 << 0);
        assert_eq!(m.translate(1 << 0), 1 << 8);
    }

    #[test]
    fn settings_map_applies_and_stick_follows_the_system() {
        let mut v = BTreeMap::new();
        v.insert("ctl.x".to_string(), "b".to_string());
        v.insert("ctl.stick_dpad".to_string(), "off".to_string());
        v.insert("opt.something".to_string(), "x".to_string());
        v.insert("ctl.garbage".to_string(), "x".to_string());
        let m = Mapping::from_values("gbc", &v);
        assert_eq!(m.value_of(9), "b");
        assert!(!m.stick_dpad);
        assert!(Mapping::from_values("gbc", &BTreeMap::new()).stick_dpad);
        assert!(Mapping::from_values("n64", &BTreeMap::new()) == Mapping::defaults("n64"));
    }

    #[test]
    fn file_roundtrip_keeps_the_layout() {
        let mut m = Mapping::defaults("gba");
        m.set("l", "none");
        m.set("x", "a");
        m.stick_dpad = false;
        let f = m.to_file(vec![Target { id: 8, label: "A".into() }]);
        assert_eq!(f.mapping["l"], "none");
        assert_eq!(Mapping::from_file(&f), m);
    }

    #[test]
    fn targets_follow_the_cores_descriptions_in_menu_order() {
        let described = vec![
            Described { id: 4, label: "D-Pad Up".into() },
            Described { id: 0, label: "B".into() },
            Described { id: 8, label: "A".into() },
            Described { id: 3, label: "Start".into() },
        ];
        let t = targets_from(&described);
        assert_eq!(t.iter().map(|t| t.id).collect::<Vec<_>>(), [8, 0, 3, 4], "A, B, then Start, then the D-pad");
        assert_eq!(t[3].label, "D-Pad Up");
        let std = targets_from(&[]);
        assert_eq!(std.len(), 16);
        assert_eq!(std[4].label, "L");
    }

    #[test]
    fn saturn_targets_without_descriptions_are_the_saturn_pad() {
        // A Saturn core that names no buttons still shows the Saturn's: A B C X Y Z L R Start and the D-pad.
        let t = targets_for("saturn", &[]);
        let label = |id: u8| t.iter().find(|t| t.id == id).map(|t| t.label.as_str());
        assert_eq!([label(0), label(8), label(10)], [Some("A"), Some("B"), Some("C")]);
        assert_eq!([label(1), label(9), label(11)], [Some("X"), Some("Y"), Some("Z")]);
        assert_eq!([label(12), label(13), label(3)], [Some("L"), Some("R"), Some("Start")]);
        assert_eq!(label(4), Some("D-pad up"));
        assert_eq!([label(2), label(14), label(15)], [None, None, None], "no Select, L3 or R3 on a Saturn pad");
        assert_eq!(t.len(), 13);
        // The core's own names win; other systems keep the standard set.
        let described = vec![Described { id: 0, label: "A".into() }];
        assert_eq!(targets_for("saturn", &described), targets_from(&described));
        assert_eq!(targets_for("snes", &[]), targets_from(&[]));
    }

    #[test]
    fn pad_buttons_become_retropad_bits_in_the_nintendo_layout() {
        assert_eq!(retropad(Button::East.bit() | Button::South.bit()), (1 << joypad::A) | (1 << joypad::B));
        assert_eq!(retropad(Button::Mode.bit() | Button::VolUp.bit() | Button::Back.bit()), 0, "not the game's");
        assert_eq!(retropad(Button::L1.bit()), 1 << joypad::L);
    }

    #[test]
    fn stick_presses_with_hysteresis() {
        let mut s = StickPad::default();
        assert_eq!(s.update(0.12, 0.12), 0, "the resting offset of the Mini V2's stick");
        assert_eq!(s.update(0.6, 0.0), 1 << 7);
        assert_eq!(s.update(0.45, 0.0), 1 << 7, "stays pressed above the release point");
        assert_eq!(s.update(0.35, 0.0), 0);
        assert_eq!(s.update(0.5, 0.0), 0, "and needs the full press point to start again");
        assert_eq!(s.update(-0.7, -0.9), (1 << 4) | (1 << 6), "up is negative y");
        assert_eq!(s.update(0.0, 0.9), 1 << 5);
    }

    #[test]
    fn analog_drops_the_dead_zone_and_can_leave_the_left_stick_out() {
        let s = Sticks::default();
        s.set(Axis::LeftX, 1.0);
        s.set(Axis::RightY, -0.5);
        s.set(Axis::RightX, 0.1);
        let a = s.analog(true);
        assert_eq!(a[0], 32767);
        assert_eq!(a[2], 0, "inside the dead zone");
        assert!((a[3] as f32 / 32767.0 + 0.41).abs() < 0.02, "{}", a[3]);
        assert_eq!(s.analog(false)[0], 0);
    }
}

#[cfg(test)]
mod controls_page_tests {
    use super::*;

    /// The Controls page both menus show: a button steps through the console's buttons and an occupied one swaps, Hold
    /// to toggle skips the D-pad, Select and Start, Reset puts the console's own layout back, each change named with its key
    /// and whether it is back to the default.
    #[test]
    fn the_controls_page_steps_swaps_and_resets_as_linux_s() {
        let mut page = ControlsPage { mapping: Mapping::defaults("snes"), targets: targets_from(&[]), system: "snes".into() };
        let rows = page.rows();
        assert_eq!(rows.len(), page.count());
        assert_eq!(rows[0].0, "Left stick as D-pad");
        assert_eq!(rows.last().unwrap().0, "Reset controls");
        // The first button row (MENU_ORDER[0], the A button) to the next console button: it swaps with that button's owner.
        let first = page.stick_rows();
        match page.step(first, 1) {
            ControlsChange::Set(changes) => {
                assert_eq!(changes.len(), 2, "{changes:?}");
                assert!(changes.iter().all(|(_, _, default)| !default));
            }
            ControlsChange::Reset => panic!("a button row is no reset"),
        }
        let hold = page.count() - 2;
        page.step(hold, 1);
        assert!(page.mapping.hold_toggle.is_some_and(|id| !(2..=7).contains(&id)));
        assert_eq!(page.step(page.count() - 1, 1), ControlsChange::Reset);
        assert_eq!(page.mapping, Mapping::defaults("snes"));
    }

    /// The choices reach the game: with A and B swapped the pad's A gives the core B and its B gives A, a press shorter
    /// than a frame too (it went in untranslated, 17.11.2026: the core saw A and B together); the touch pad's A stays A;
    /// D-pad as left stick turns the D-pad's presses into the stick; Reset gives the console's own buttons back.
    #[test]
    fn the_choices_reach_the_game() {
        let (a, b) = (1u16 << 8, 1u16 << 0);
        let mut page = ControlsPage { mapping: Mapping::defaults("snes"), targets: targets_from(&[]), system: "snes".into() };
        let row_of = |id: u8, page: &ControlsPage| page.stick_rows() + MENU_ORDER.iter().position(|i| *i == id).unwrap();
        // A (MENU_ORDER's first) one step on: B, which swaps.
        page.step(row_of(8, &page), 1);
        assert_eq!(pad_input(&page.mapping, a, 0, 0, 0, [0; 4]).0, b, "the pad's A presses B");
        assert_eq!(pad_input(&page.mapping, b, 0, 0, 0, [0; 4]).0, a, "and its B presses A");
        assert_eq!(pad_input(&page.mapping, 0, a, 0, 0, [0; 4]).0, b, "a short press too");
        assert_eq!(pad_input(&page.mapping, 0, 0, 0, a, [0; 4]).0, a, "the touch pad's A is the console's A");
        page.step(page.count() - 1, 1);
        assert_eq!(pad_input(&page.mapping, a, 0, 0, 0, [0; 4]).0, a, "Reset: A is A again");
        // D-pad as left stick (a console whose games use a stick).
        let mut n64 = ControlsPage { mapping: Mapping::defaults("n64"), targets: targets_from(&[]), system: "n64".into() };
        assert_eq!(n64.stick_rows(), 2);
        if !n64.mapping.dpad_stick {
            n64.step(1, 1);
        }
        let right = 1u16 << 7;
        let (buttons, sticks) = pad_input(&n64.mapping, right, 0, 0, 0, [0; 4]);
        assert_eq!(buttons & right, 0, "the D-pad's Right moves the stick instead");
        assert!(sticks.is_some_and(|s| s[0] > 0), "{sticks:?}");
    }
}
