//! Controls of a hosted game: which Mini V2 button presses which button of the console, and whether
//! the left stick acts as a D-pad.
//!
//! The Mini V2's buttons are numbered like libretro's RetroPad (the Nintendo layout: A is the right
//! face button), so "physical button i" and "console button i" are the same number until the user
//! remaps. The console's own names for its buttons come from the core (`SET_INPUT_DESCRIPTORS`:
//! "A", "B", "Select", "D-Pad Up", …); pocket-core hands them to the pause menu in the snapshot file.
//!
//! Settings keys in the per-console / per-game files: `ctl.<button>` = `<button>` or `none`, and
//! `ctl.stick_dpad` = `on` or `off`.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicI16, Ordering};

use serde::{Deserialize, Serialize};

use crate::libretro::joypad;
use crate::pad::{Axis, Button};

/// Prefix of control keys inside the per-console / per-game settings files.
pub const KEY_PREFIX: &str = "ctl.";
pub const STICK_KEY: &str = "stick_dpad";

/// Key names by RetroPad id (0..15); the same names are the values a button can be mapped to.
pub const KEYS: [&str; 16] = ["b", "y", "select", "start", "up", "down", "left", "right", "a", "x", "l", "r", "l2", "r2", "l3", "r3"];

/// What each button is called on the Mini V2 (RetroPad id → name).
const LABELS: [&str; 16] = ["B", "Y", "Select", "Start", "D-pad up", "D-pad down", "D-pad left", "D-pad right", "A", "X", "L1", "R1", "L2", "R2", "L3", "R3"];

/// The order buttons are listed in the menu and offered as choices: face buttons, shoulders, system buttons, D-pad.
pub const MENU_ORDER: [u8; 16] = [8, 0, 9, 1, 10, 11, 12, 13, 14, 15, 2, 3, 4, 5, 6, 7];

/// Systems whose games need an analog stick. Everything else has no use for it, so the stick is a D-pad there.
/// PS1 is in the list because pocket-core plugs a DualShock into it (`hostsys`): the sticks go to the core.
/// ScummVM moves its cursor with the left stick; DOSBox Pure maps it to the game's joystick, mouse or keys.
const ANALOG_SYSTEMS: &[&str] = &["psx", "n64", "dreamcast", "psp", "gamecube", "wii", "3ds", "ps2", "naomi", "atomiswave", "scummvm", "pc"];

pub fn stick_dpad_default(system: &str) -> bool {
    !ANALOG_SYSTEMS.contains(&system)
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
        return targets_from(described);
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

/// Which console button each Mini V2 button presses.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mapping {
    pub target: [Option<u8>; 16],
    pub stick_dpad: bool,
}

impl Mapping {
    pub fn defaults(system: &str) -> Mapping {
        Mapping { target: std::array::from_fn(|i| Some(i as u8)), stick_dpad: stick_dpad_default(system) }
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
        let Some(id) = key_id(key) else { return false };
        match value {
            "none" => self.target[id as usize] = None,
            v => match key_id(v) {
                Some(t) => self.target[id as usize] = Some(t),
                None => return false,
            },
        }
        true
    }

    pub fn value_of(&self, id: u8) -> &'static str {
        self.target[id as usize].map_or("none", |t| KEYS[t as usize])
    }

    /// Mini V2 buttons (bit i = button i) → console buttons.
    pub fn translate(&self, pressed: u16) -> u16 {
        let mut out = 0;
        for (i, target) in self.target.iter().enumerate() {
            if let (true, Some(t)) = (pressed & (1 << i) != 0, target) {
                out |= 1 << t;
            }
        }
        out
    }

    pub fn to_file(&self, targets: Vec<Target>) -> ControlsFile {
        ControlsFile {
            targets,
            mapping: (0..16u8).map(|i| (KEYS[i as usize].to_string(), self.value_of(i).to_string())).collect(),
            stick_dpad: self.stick_dpad,
        }
    }

    pub fn from_file(f: &ControlsFile) -> Mapping {
        let mut m = Mapping::defaults("");
        for (k, v) in &f.mapping {
            m.set(k, v);
        }
        m.stick_dpad = f.stick_dpad;
        m
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
