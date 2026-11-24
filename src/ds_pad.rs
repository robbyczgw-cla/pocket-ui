//! DS actions shared by the Linux and Android players. Action ids are menu targets,
//! never RetroPad bits. The resulting right stick, R2 touch and R3 layout button
//! are the input contract of the shipped melonDS DS core.
//! The layout button: a short press swaps the two screens (`swap_options`), held it
//! presses R3 and the core shows its next layout.
use std::time::{Duration, Instant};
use crate::controls::Mapping;

pub const STYLUS: u8 = 16;
pub const LAYOUT: u8 = 17;
pub const ACTIONS: [(u8, &str, &str); 2] = [
    (STYLUS, "ds_stylus", "Stylus mode"),
    (LAYOUT, "ds_layout", "Screen layout"),
];
const DPAD: u16 = 0xf0;
/// Held this long, the layout button is the old "next layout"; let go earlier, it swaps the screens.
pub const LAYOUT_HOLD: Duration = Duration::from_millis(400);
/// melonDS DS reads up to this many `melonds_screen_layoutN` options.
const MELONDS_LAYOUTS: usize = 8;

pub fn name(id: u8) -> Option<&'static str> {
    ACTIONS.iter().find(|a| a.0 == id).map(|a| a.2)
}

#[derive(Default, Clone, Copy, Debug)]
pub struct Pad {
    pub stylus: bool,
    previous: u16,
    blocked: u16,
    /// When the layout button went down (a fresh press, not one held over from a menu).
    layout_down: Option<Instant>,
}

pub struct Frame {
    pub raw: u16,
    pub buttons: u16,
    pub axes: [i16; 4],
    pub changed: Option<bool>,
    /// The layout button was tapped: swap the screens (`swap_options`).
    pub swap: bool,
}

impl Frame {
    /// The buttons as they are, no DS action (other systems, a held HOME, the pause menu).
    pub fn plain(raw: u16, axes: [i16; 4]) -> Frame {
        Frame { raw, buttons: 0, axes, changed: None, swap: false }
    }
}

/// The same layout with the two screens the other way round; `None` where there is no such twin.
fn swapped(layout: &str) -> Option<&'static str> {
    Some(match layout {
        "top-bottom" => "bottom-top",
        "bottom-top" => "top-bottom",
        "left-right" => "right-left",
        "right-left" => "left-right",
        "top" => "bottom",
        "bottom" => "top",
        "hybrid-top" => "hybrid-bottom",
        "hybrid-bottom" => "hybrid-top",
        _ => return None,
    })
}

/// The core options that swap the two DS screens, for the core's current `option` values. DSperate keeps its
/// current layout in `dsperate_screen_layout` (it writes it there when R3 cycles); melonDS DS keeps only an index
/// into `melonds_screen_layout1..N`, so every entry is turned round and the index then shows the swapped twin.
/// Cycling with a held button afterwards goes through the swapped arrangements.
pub fn swap_options(option: impl Fn(&str) -> Option<String>) -> Vec<(String, &'static str)> {
    let keys = std::iter::once("dsperate_screen_layout".to_string())
        .chain((1..=MELONDS_LAYOUTS).map(|i| format!("melonds_screen_layout{i}")));
    keys.filter_map(|k| { let to = swapped(&option(&k)?)?; Some((k, to)) }).collect()
}

impl Pad {
    /// A shortcut/pause must not turn a held menu button into a fresh DS action.
    pub fn suppress(&mut self, raw: u16) {
        self.previous = raw;
        self.blocked |= raw;
        // A layout press that went into a menu neither swaps nor cycles when it is let go there.
        self.layout_down = None;
    }

    pub fn apply(&mut self, system: &str, mapping: &Mapping, raw: u16, axes: [i16; 4]) -> Frame {
        self.apply_at(system, mapping, raw, axes, Instant::now())
    }

    pub fn apply_at(&mut self, system: &str, mapping: &Mapping, raw: u16, axes: [i16; 4], now: Instant) -> Frame {
        let mut f = Frame::plain(raw, axes);
        if system != "nds" { return f; }
        let sources = |target| mapping.target.iter().enumerate().fold(0u16, |mask, (i, t)|
            mask | if *t == Some(target) { 1 << i } else { 0 });
        self.blocked &= raw;
        let edge = raw & !self.previous & !self.blocked;
        self.previous = raw;
        let tap = sources(8);
        if edge & sources(STYLUS) != 0 {
            self.stylus = !self.stylus;
            f.changed = Some(self.stylus);
            // Require release in both directions: neither moving nor touching the
            // stylus, nor walking in the game, starts with an already held button.
            self.blocked |= raw & (DPAD | tap);
        }
        // Tapped: the player swaps the screens. Held: R3 from LAYOUT_HOLD until it is
        // let go, so the core detects the press, just as for a physical R3; a display
        // tick without an emulated frame must not consume it.
        let layout = raw & !self.blocked & sources(LAYOUT) != 0;
        match self.layout_down {
            None if layout => self.layout_down = Some(now),
            Some(down) if layout => if now.duration_since(down) >= LAYOUT_HOLD { f.buttons |= 1 << 15; },
            Some(down) => {
                f.swap = now.duration_since(down) < LAYOUT_HOLD;
                self.layout_down = None;
            }
            None => {}
        }
        f.raw &= !self.blocked;
        if self.stylus {
            let held = f.raw;
            f.raw &= !(DPAD | tap);
            let x = ((held >> 7) & 1) as i32 - ((held >> 6) & 1) as i32;
            let y = ((held >> 5) & 1) as i32 - ((held >> 4) & 1) as i32;
            if x != 0 || y != 0 {
                let size = if x != 0 && y != 0 { 23170 } else { 32767 };
                f.axes[2] = (x * size) as i16;
                f.axes[3] = (y * size) as i16;
            }
            if held & tap != 0 { f.buttons |= 1 << 13; }
        }
        f
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn frame(p: &mut Pad, m: &Mapping, raw: u16) -> Frame { p.apply("nds", m, raw, [0; 4]) }

    #[test]
    fn ds_actions_are_edges_and_stylus_never_walks_or_presses_a() {
        let m = Mapping::defaults_on("nds", false);
        let mut p = Pad::default();
        assert_eq!(frame(&mut p, &m, 1 << 12).changed, Some(true));
        assert_eq!(frame(&mut p, &m, 1 << 12).changed, None);
        frame(&mut p, &m, 0);
        let f = frame(&mut p, &m, (1 << 7) | (1 << 8));
        assert_eq!(m.translate(f.raw) | f.buttons, 1 << 13);
        assert_eq!(f.axes, [0, 0, 32767, 0]);
        let t = Instant::now();
        assert_eq!(p.apply_at("nds", &m, 1 << 13, [0; 4], t).buttons, 0);
        let f = p.apply_at("nds", &m, 1 << 13, [0; 4], t + LAYOUT_HOLD);
        assert_eq!(f.buttons, 1 << 15);
        assert_eq!(p.apply_at("nds", &m, 1 << 13, [0; 4], t + 2 * LAYOUT_HOLD).buttons, 1 << 15);
        let f = p.apply_at("nds", &m, 0, [0; 4], t + 2 * LAYOUT_HOLD);
        assert_eq!((f.buttons, f.swap), (0, false));
        frame(&mut p, &m, 0);
        assert_eq!(frame(&mut p, &m, 1 << 12).changed, Some(false));
        let f = frame(&mut p, &m, 1 << 8);
        assert_eq!(m.translate(f.raw), 1 << 8);
    }

    #[test]
    fn switching_requires_release_and_remapped_actions_and_a_work() {
        let mut m = Mapping::defaults_on("nds", false);
        m.assign(2, Some(STYLUS));
        m.assign(9, Some(LAYOUT));
        m.assign(0, Some(8));
        let mut p = Pad::default();
        let f = frame(&mut p, &m, (1 << 2) | (1 << 7) | 1);
        assert_eq!(f.changed, Some(true));
        assert_eq!((f.raw & ((1 << 7) | 1), f.buttons, f.axes), (0, 0, [0; 4]));
        frame(&mut p, &m, 0);
        assert_eq!(frame(&mut p, &m, 1).buttons, 1 << 13);
        let t = Instant::now();
        p.apply_at("nds", &m, 1 << 9, [0; 4], t);
        assert_eq!(p.apply_at("nds", &m, 1 << 9, [0; 4], t + LAYOUT_HOLD).buttons, 1 << 15);
        assert!(!p.apply_at("nds", &m, 0, [0; 4], t + LAYOUT_HOLD).swap);
        p.suppress(1 << 2);
        assert_eq!(frame(&mut p, &m, 1 << 2).changed, None);
    }

    #[test]
    fn a_tap_on_layout_swaps_the_screens_and_a_hold_shows_the_next_layout() {
        let m = Mapping::defaults_on("nds", false);
        let r2 = 1 << 13;
        let mut p = Pad::default();
        let t = Instant::now();
        let ms = |n: u64| t + Duration::from_millis(n);
        // A tap: no R3 for the core, the player swaps once, on the release.
        for (raw, at) in [(r2, 0), (r2, 150)] {
            let f = p.apply_at("nds", &m, raw, [0; 4], ms(at));
            assert_eq!((f.buttons, f.swap), (0, false));
        }
        let f = p.apply_at("nds", &m, 0, [0; 4], ms(200));
        assert_eq!((f.buttons, f.swap), (0, true));
        assert!(!p.apply_at("nds", &m, 0, [0; 4], ms(210)).swap, "one tap swaps once");
        // Android's press too short for a frame (one tick held) is a tap too.
        p.apply_at("nds", &m, r2, [0; 4], ms(300));
        assert!(p.apply_at("nds", &m, 0, [0; 4], ms(316)).swap);
        // Held: R3 from LAYOUT_HOLD until let go (the core's own edge cycles once), no swap.
        p.apply_at("nds", &m, r2, [0; 4], ms(1000));
        assert_eq!(p.apply_at("nds", &m, r2, [0; 4], ms(1399)).buttons, 0);
        assert_eq!(p.apply_at("nds", &m, r2, [0; 4], ms(1400)).buttons, 1 << 15);
        let f = p.apply_at("nds", &m, 0, [0; 4], ms(1600));
        assert_eq!((f.buttons, f.swap), (0, false));
        // A press that went into the pause menu does nothing when it is let go after it.
        p.apply_at("nds", &m, r2, [0; 4], ms(2000));
        p.suppress(r2);
        assert!(!p.apply_at("nds", &m, 0, [0; 4], ms(2100)).swap);
        // Remapped: the action swaps, R2 no longer does.
        let mut m2 = m;
        m2.assign(13, None);
        m2.assign(9, Some(LAYOUT));
        p.apply_at("nds", &m2, r2, [0; 4], ms(3000));
        assert!(!p.apply_at("nds", &m2, 0, [0; 4], ms(3050)).swap);
        p.apply_at("nds", &m2, 1 << 9, [0; 4], ms(3100));
        assert!(p.apply_at("nds", &m2, 0, [0; 4], ms(3150)).swap);
        // Other systems never swap.
        p.apply_at("snes", &m, r2, [0; 4], ms(4000));
        assert!(!p.apply_at("snes", &m, 0, [0; 4], ms(4050)).swap);
    }

    #[test]
    fn swapping_turns_round_the_current_layout_of_both_cores() {
        use std::collections::HashMap;
        let opts = |pairs: &[(&str, &str)]| -> HashMap<String, String> { pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect() };
        let swap = |o: &HashMap<String, String>| swap_options(|k| o.get(k).cloned());
        // DSperate: its one current layout, both ways, and the single screens.
        for (from, to) in [("top-bottom", "bottom-top"), ("bottom-top", "top-bottom"), ("left-right", "right-left"),
                           ("right-left", "left-right"), ("top", "bottom"), ("bottom", "top")] {
            assert_eq!(swap(&opts(&[("dsperate_screen_layout", from)])), vec![("dsperate_screen_layout".to_string(), to)]);
        }
        // melonDS DS (pocket-ui's four layouts): every entry, so the core's index shows the twin.
        let melon = opts(&[("melonds_number_of_screen_layouts", "4"), ("melonds_screen_layout1", "top-bottom"),
                           ("melonds_screen_layout2", "left-right"), ("melonds_screen_layout3", "top"), ("melonds_screen_layout4", "bottom")]);
        let got = swap(&melon);
        assert_eq!(got, vec![("melonds_screen_layout1".to_string(), "bottom-top"), ("melonds_screen_layout2".to_string(), "right-left"),
                             ("melonds_screen_layout3".to_string(), "bottom"), ("melonds_screen_layout4".to_string(), "top")]);
        // Swapped twice is the start again; touch still finds the bottom screen of the swapped layout.
        let mut once = melon.clone();
        for (k, v) in &got { once.insert(k.clone(), v.to_string()); }
        let mut twice = once.clone();
        for (k, v) in swap(&once) { twice.insert(k, v.to_string()); }
        assert_eq!(twice, melon);
        let layouts: Vec<&str> = (1..=4).map(|i| once[&format!("melonds_screen_layout{i}")].as_str()).collect();
        assert!(matches!(crate::touch::nds_bottom(256, 384, &layouts), crate::touch::Bottom::Screen(p) if p.y == 0.0));
        // A layout without a twin (rotated) and a core without these options: nothing changes.
        assert!(swap(&opts(&[("melonds_screen_layout1", "rotate-left")])).is_empty());
        assert!(swap(&opts(&[("snes9x_overclock", "on")])).is_empty());
    }

    #[test]
    fn other_systems_and_real_analog_sticks_are_unchanged() {
        let m = Mapping::defaults_on("snes", false);
        let mut p = Pad::default();
        let f = p.apply("snes", &m, 0xffff, [1, 2, 3, 4]);
        assert_eq!((f.raw, f.buttons, f.axes, f.changed), (0xffff, 0, [1, 2, 3, 4], None));
        let m = Mapping::defaults_on("nds", true);
        assert_eq!(frame(&mut p, &m, 1 << 12).changed, None, "a pad with sticks keeps its old L2");
    }
    #[test]
    fn ds_actions_are_reachable_in_controls_and_round_trip_through_settings() {
        use crate::controls::{ControlsPage, ControlsChange, MENU_ORDER, KEY_PREFIX};
        let mut page = ControlsPage { system: "nds".into(), mapping: Mapping::defaults_on("nds", false), targets: Vec::new() };
        assert!(page.rows().iter().any(|r| r.0 == "L2" && r.1 == "Stylus mode"));
        assert!(page.rows().iter().any(|r| r.0 == "R2" && r.1 == "Screen layout"));
        let select = page.stick_rows() + MENU_ORDER.iter().position(|i| *i == 2).unwrap();
        let mut values = std::collections::BTreeMap::new();
        for _ in 0..20 {
            if let ControlsChange::Set(entries) = page.step(select, 1) {
                for (key, value, _) in entries { values.insert(format!("{KEY_PREFIX}{key}"), value); }
            }
            if page.mapping.target[2] == Some(STYLUS) { break; }
        }
        assert_eq!(page.mapping.target[2], Some(STYLUS));
        // Both the pause menu and Android persist these same ctl.* values.
        let mut loaded = Mapping::defaults_on("nds", false);
        for (k, v) in values { loaded.set(k.strip_prefix(KEY_PREFIX).unwrap(), &v); }
        assert_eq!(loaded, page.mapping);
        assert_eq!(Mapping::from_file(&loaded.to_file(Vec::new())), loaded);
        // The same hold predicate serves Android's older options list: actions
        // can be assigned to a button, but never latched as a console button.
        assert!(crate::controls::targets_for("nds", &[]).iter().filter(|t| crate::controls::can_hold(t.id)).all(|t| t.id < STYLUS));
        loaded.hold_toggle = Some(STYLUS);
        assert_eq!(loaded.hold_value(), "none");

    }

}
