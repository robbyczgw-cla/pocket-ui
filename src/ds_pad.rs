//! DS actions shared by the Linux and Android players. Action ids are menu targets,
//! never RetroPad bits. The resulting right stick, R2 touch and R3 layout button
//! are the input contract of the shipped melonDS DS core.
use crate::controls::Mapping;

pub const STYLUS: u8 = 16;
pub const LAYOUT: u8 = 17;
pub const ACTIONS: [(u8, &str, &str); 2] = [
    (STYLUS, "ds_stylus", "Stylus mode"),
    (LAYOUT, "ds_layout", "Screen layout"),
];
const DPAD: u16 = 0xf0;

pub fn name(id: u8) -> Option<&'static str> {
    ACTIONS.iter().find(|a| a.0 == id).map(|a| a.2)
}

#[derive(Default, Clone, Copy, Debug)]
pub struct Pad {
    pub stylus: bool,
    previous: u16,
    blocked: u16,
}

pub struct Frame {
    pub raw: u16,
    pub buttons: u16,
    pub axes: [i16; 4],
    pub changed: Option<bool>,
}

impl Pad {
    /// A shortcut/pause must not turn a held menu button into a fresh DS action.
    pub fn suppress(&mut self, raw: u16) {
        self.previous = raw;
        self.blocked |= raw;
    }

    pub fn apply(&mut self, system: &str, mapping: &Mapping, raw: u16, axes: [i16; 4]) -> Frame {
        let mut f = Frame { raw, buttons: 0, axes, changed: None };
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
        // Let the core detect the press, just as for a physical R3. A display
        // tick without an emulated frame must not consume the layout button.
        if raw & !self.blocked & sources(LAYOUT) != 0 { f.buttons |= 1 << 15; }
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
        let f = frame(&mut p, &m, 1 << 13);
        assert_eq!(f.buttons, 1 << 15);
        assert_eq!(frame(&mut p, &m, 1 << 13).buttons, 1 << 15);
        assert_eq!(frame(&mut p, &m, 0).buttons, 0);
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
        assert_eq!(frame(&mut p, &m, 1 << 9).buttons, 1 << 15);
        p.suppress(1 << 2);
        assert_eq!(frame(&mut p, &m, 1 << 2).changed, None);
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
