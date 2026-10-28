//! The pause menu's "Emulator" and "Controls" pages: state and input handling. Painting stays in
//! main.rs, which turns the `Row`s into the same list the other option pages use.
//!
//! Both pages work from the snapshot pocket-core writes for the running game
//! (`core_options::CoreOptionsFile`): the core's options with their values and the console's own
//! button names. Changes go to pocket-core at once (`opt …` / `ctl …` over the command FIFO) and into
//! the game settings map as `opt.<key>` / `ctl.<key>`, so "Save changes" stores them for the console
//! or the game exactly like the picture settings.

use std::collections::HashMap;
use std::path::Path;

use crate::control;
use crate::controls::{self, Mapping, MENU_ORDER};
use crate::core_options::{self, CoreOptionsFile, KEY_PREFIX};
use crate::options::GameSettings;

/// One line of a page: name on the left, value on the right, explanation at the bottom.
pub struct Row {
    pub name: String,
    pub value: String,
    pub desc: String,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Entry {
    /// Index into the file's categories.
    Category(usize),
    /// Index into the file's options.
    Option(usize),
}

pub struct EmuMenu {
    file: CoreOptionsFile,
    values: HashMap<String, String>,
    mapping: Mapping,
    system: String,
}

/// Footer note for options the core only reads when it starts.
const RESTART_NOTE: &str = "Takes effect the next time the game starts. Save it, then start the game again.";

fn needs_restart(label: &str) -> bool {
    let l = label.to_ascii_lowercase();
    l.contains("(restart") || l.contains("(reload core") || l.contains("(requires restart") || l.contains("restart required")
}

impl EmuMenu {
    /// Reads pocket-core's snapshot for this very game. None when there is none (yet).
    ///
    /// What the running game really uses is in the snapshot, not in the saved files: a change made in
    /// an earlier visit to the menu and never saved is still in effect. Those values go into `settings`
    /// too, so that Save changes stores what is on screen.
    pub fn load(data_dir: &Path, system: &str, rom: &Path, settings: &mut GameSettings) -> Option<EmuMenu> {
        let file = CoreOptionsFile::read(&core_options::file_path(data_dir, system), rom)?;
        let mut values = HashMap::new();
        for o in &file.options {
            values.insert(o.key.clone(), o.current.clone());
            let key = format!("{KEY_PREFIX}{}", o.key);
            if o.current != o.default {
                settings.values.insert(key, o.current.clone());
            } else {
                settings.values.remove(&key);
            }
        }
        let mapping = Mapping::from_file(&file.controls);
        let defaults = Mapping::defaults(system);
        for id in 0..16u8 {
            let key = format!("{}{}", controls::KEY_PREFIX, controls::KEYS[id as usize]);
            if mapping.value_of(id) != defaults.value_of(id) {
                settings.values.insert(key, mapping.value_of(id).to_string());
            } else {
                settings.values.remove(&key);
            }
        }
        let stick = format!("{}{}", controls::KEY_PREFIX, controls::STICK_KEY);
        if mapping.stick_dpad != defaults.stick_dpad {
            settings.values.insert(stick, on_off(mapping.stick_dpad).into());
        } else {
            settings.values.remove(&stick);
        }
        Some(EmuMenu { file, values, mapping, system: system.to_string() })
    }

    /// Whether the core has anything to show on the Emulator page.
    pub fn has_options(&self) -> bool {
        !self.file.options.is_empty()
    }

    /// Lines of the Emulator page: at the top level the categories, then the options that have none;
    /// inside a category its options.
    pub fn entries(&self, cat: Option<usize>) -> Vec<Entry> {
        let in_cat = |o: &core_options::FileOption| match cat {
            Some(c) => self.file.categories.get(c).is_some_and(|c| o.category.as_deref() == Some(c.key.as_str())),
            None => o.category.is_none() || !self.file.categories.iter().any(|c| o.category.as_deref() == Some(c.key.as_str())),
        };
        let mut out: Vec<Entry> = if cat.is_none() { (0..self.file.categories.len()).map(Entry::Category).collect() } else { Vec::new() };
        out.extend(self.file.options.iter().enumerate().filter(|(_, o)| in_cat(o)).map(|(i, _)| Entry::Option(i)));
        out
    }

    pub fn rows(&self, cat: Option<usize>) -> Vec<Row> {
        self.entries(cat)
            .into_iter()
            .map(|e| match e {
                Entry::Category(i) => {
                    let c = &self.file.categories[i];
                    Row { name: c.label.clone(), value: ">".into(), desc: c.info.clone() }
                }
                Entry::Option(i) => {
                    let o = &self.file.options[i];
                    let current = self.values.get(&o.key).map_or(o.default.as_str(), String::as_str);
                    let mut desc = o.info.clone();
                    if needs_restart(&o.label) {
                        desc = format!("{desc}\n{RESTART_NOTE}").trim().to_string();
                    }
                    if o.values.len() > 12 {
                        desc = format!("{desc}\nL1 and R1 skip five choices at a time.").trim().to_string();
                    }
                    Row { name: o.label.clone(), value: o.label_of(current).to_string(), desc }
                }
            })
            .collect()
    }

    pub fn category_label(&self, cat: usize) -> Option<String> {
        self.file.categories.get(cat).map(|c| c.label.clone())
    }

    /// A/Right (`delta` 1) or Left (-1) on a line. Returns the category to open when a category was chosen.
    pub fn step(&mut self, cat: Option<usize>, sel: usize, delta: i32, settings: &mut GameSettings) -> Option<usize> {
        match self.entries(cat).get(sel).copied()? {
            Entry::Category(c) => (delta == 1).then_some(c),
            Entry::Option(i) => {
                let o = &self.file.options[i];
                let n = o.values.len() as i32;
                let now = self.values.get(&o.key).and_then(|v| o.values.iter().position(|x| x.value == *v)).unwrap_or(0) as i32;
                let next = o.values[(now + delta).rem_euclid(n) as usize].value.clone();
                let key = o.key.clone();
                let is_default = next == o.default;
                self.values.insert(key.clone(), next.clone());
                let setting = format!("{KEY_PREFIX}{key}");
                if is_default {
                    settings.values.remove(&setting);
                } else {
                    settings.values.insert(setting, next.clone());
                }
                control::pocket_core(&format!("opt {key} {next}"));
                None
            }
        }
    }

    // ---- Controls ----

    /// The console buttons a Mini V2 button can be given: nothing, or any button the core describes.
    fn choices(&self) -> Vec<Option<u8>> {
        let mut c = vec![None];
        c.extend(self.file.controls.targets.iter().map(|t| Some(t.id)));
        c
    }

    fn target_name(&self, id: Option<u8>) -> String {
        match id {
            Some(id) => match self.file.controls.targets.iter().find(|t| t.id == id) {
                Some(t) => t.label.clone(),
                // Not a button this console has, so pressing it does nothing.
                None if !self.file.controls.targets.is_empty() => "None".into(),
                None => controls::standard_name(id).into(),
            },
            None => "None".into(),
        }
    }

    /// Row 0 is the stick, then the Mini V2's buttons in `MENU_ORDER`, then "Reset controls".
    pub fn control_rows(&self) -> Vec<Row> {
        let mut rows = vec![Row {
            name: "Left stick as D-pad".into(),
            value: if self.mapping.stick_dpad { "On" } else { "Off" }.into(),
            desc: "The left stick works as the D-pad.\nTurn it off for games that use an analogue stick.".into(),
        }];
        for &id in &MENU_ORDER {
            let target = self.mapping.target[id as usize];
            let name = self.target_name(target);
            let phys = controls::physical_label(id);
            let desc = if name == "None" { format!("{phys} does nothing in this game.") } else { format!("{phys} presses {name}.") };
            let extra = match id {
                14 => "\nL3 means pressing the left stick in.",
                15 => "\nR3 means pressing the right stick in.",
                _ => "",
            };
            rows.push(Row { name: phys.into(), value: name, desc: format!("{desc}{extra}") });
        }
        rows.push(Row { name: "Reset controls".into(), value: String::new(), desc: "Every button does what it says, as it did before any change.".into() });
        rows
    }

    pub fn control_count(&self) -> usize {
        MENU_ORDER.len() + 2
    }

    pub fn control_step(&mut self, sel: usize, delta: i32, settings: &mut GameSettings) {
        let defaults = Mapping::defaults(&self.system);
        let prefix = controls::KEY_PREFIX;
        if sel == 0 {
            self.mapping.stick_dpad = !self.mapping.stick_dpad;
            let (key, value) = (controls::STICK_KEY, on_off(self.mapping.stick_dpad));
            if self.mapping.stick_dpad == defaults.stick_dpad {
                settings.values.remove(&format!("{prefix}{key}"));
            } else {
                settings.values.insert(format!("{prefix}{key}"), value.into());
            }
            control::pocket_core(&format!("ctl {key} {value}"));
        } else if sel <= MENU_ORDER.len() {
            let id = MENU_ORDER[sel - 1];
            let choices = self.choices();
            let current = self.mapping.target[id as usize];
            let now = choices.iter().position(|c| *c == current).unwrap_or(0) as i32;
            let next = choices[(now + delta).rem_euclid(choices.len() as i32) as usize];
            self.mapping.target[id as usize] = next;
            let key = controls::KEYS[id as usize];
            let value = self.mapping.value_of(id);
            if self.mapping.target[id as usize] == defaults.target[id as usize] {
                settings.values.remove(&format!("{prefix}{key}"));
            } else {
                settings.values.insert(format!("{prefix}{key}"), value.to_string());
            }
            control::pocket_core(&format!("ctl {key} {value}"));
        } else {
            self.mapping = defaults;
            settings.values.retain(|k, _| !k.starts_with(prefix));
            for id in 0..16usize {
                control::pocket_core(&format!("ctl {} {}", controls::KEYS[id], self.mapping.value_of(id as u8)));
            }
            control::pocket_core(&format!("ctl {} {}", controls::STICK_KEY, on_off(self.mapping.stick_dpad)));
        }
    }

    /// Restore defaults was chosen: every option and button is back to its default.
    pub fn restored(&mut self) {
        for o in &self.file.options {
            self.values.insert(o.key.clone(), o.default.clone());
        }
        self.mapping = Mapping::defaults(&self.system);
    }
}

fn on_off(on: bool) -> &'static str {
    if on {
        "on"
    } else {
        "off"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::controls::Target;
    use crate::core_options::{FileCategory, FileOption, FileValue};

    fn opt(key: &str, cat: Option<&str>, values: &[&str], default: &str, current: &str) -> FileOption {
        FileOption {
            key: key.into(),
            label: key.to_uppercase(),
            info: format!("about {key}"),
            category: cat.map(String::from),
            values: values.iter().map(|v| FileValue { value: v.to_string(), label: v.to_uppercase() }).collect(),
            default: default.into(),
            current: current.into(),
        }
    }

    fn menu_with(file: CoreOptionsFile, saved: &[(&str, &str)]) -> (EmuMenu, GameSettings) {
        let dir = crate::testutil::TestDir::new("emumenu");
        let rom = Path::new("/roms/gbc/g.gbc");
        let mut s = GameSettings::load(&dir, "gbc", rom);
        for (k, v) in saved {
            s.values.insert(k.to_string(), v.to_string());
        }
        assert!(file.write(&core_options::file_path(&dir, "gbc")));
        let m = EmuMenu::load(&dir, "gbc", rom, &mut s).expect("snapshot is readable");
        (m, s)
    }

    fn sample() -> CoreOptionsFile {
        CoreOptionsFile {
            core: "Test".into(),
            rom: "/roms/gbc/g.gbc".into(),
            categories: vec![FileCategory { key: "video".into(), label: "Video".into(), info: "Picture.".into() }],
            options: vec![
                opt("pal", Some("video"), &["grey", "green", "blue"], "grey", "grey"),
                opt("loose", None, &["off", "on"], "off", "on"),
                opt("boot", None, &["a", "b"], "a", "a"),
            ],
            controls: controls::ControlsFile::default(),
        }
    }

    #[test]
    fn categories_first_then_loose_options_and_category_pages() {
        let (m, _) = menu_with(sample(), &[]);
        assert_eq!(m.entries(None), [Entry::Category(0), Entry::Option(1), Entry::Option(2)]);
        assert_eq!(m.entries(Some(0)), [Entry::Option(0)]);
        let rows = m.rows(None);
        assert_eq!((rows[0].name.as_str(), rows[0].value.as_str(), rows[0].desc.as_str()), ("Video", ">", "Picture."));
        assert_eq!((rows[1].name.as_str(), rows[1].value.as_str()), ("LOOSE", "ON"));
    }

    #[test]
    fn unsaved_changes_from_an_earlier_visit_end_up_in_the_settings() {
        // `loose` runs at "on" (default "off") although nothing was saved; `pal` was saved as green but is back at grey.
        let (_, s) = menu_with(sample(), &[("opt.pal", "green"), ("opt.stale", "x")]);
        assert_eq!(s.values.get("opt.loose").map(String::as_str), Some("on"));
        assert_eq!(s.values.get("opt.pal"), None, "back at the default: nothing to store");
        assert_eq!(s.values.get("opt.stale").map(String::as_str), Some("x"), "unknown keys are left alone");
    }

    #[test]
    fn stepping_cycles_values_and_keeps_only_non_defaults() {
        let (mut m, mut s) = menu_with(sample(), &[]);
        assert_eq!(m.step(Some(0), 0, 1, &mut s), None);
        assert_eq!(s.values["opt.pal"], "green");
        m.step(Some(0), 0, 1, &mut s);
        m.step(Some(0), 0, 1, &mut s);
        assert_eq!(m.rows(Some(0))[0].value, "GREY", "wrapped around to the default");
        assert!(!s.values.contains_key("opt.pal"));
        m.step(Some(0), 0, -1, &mut s);
        assert_eq!(s.values["opt.pal"], "blue", "left wraps the other way");
    }

    #[test]
    fn a_category_opens_with_right_or_ok_only() {
        let (mut m, mut s) = menu_with(sample(), &[]);
        assert_eq!(m.step(None, 0, -1, &mut s), None);
        assert_eq!(m.step(None, 0, 1, &mut s), Some(0));
    }

    #[test]
    fn restart_options_get_a_note() {
        let mut f = sample();
        f.options[2].label = "Use BIOS (Restart)".into();
        let (m, _) = menu_with(f, &[]);
        assert!(m.rows(None)[2].desc.contains("next time the game starts"));
        assert!(!m.rows(None)[1].desc.contains("next time"));
    }

    fn with_controls() -> CoreOptionsFile {
        let mut f = sample();
        f.controls.targets = vec![
            Target { id: 8, label: "A".into() },
            Target { id: 0, label: "B".into() },
            Target { id: 3, label: "Start".into() },
            Target { id: 2, label: "Select".into() },
        ];
        f.controls.mapping = Mapping::defaults("gbc").to_file(vec![]).mapping;
        f.controls.stick_dpad = true;
        f
    }

    #[test]
    fn controls_show_the_consoles_names_and_none_for_buttons_it_lacks() {
        let (m, _) = menu_with(with_controls(), &[]);
        let rows = m.control_rows();
        assert_eq!(rows.len(), m.control_count());
        assert_eq!((rows[0].name.as_str(), rows[0].value.as_str()), ("Left stick as D-pad", "On"));
        assert_eq!((rows[1].name.as_str(), rows[1].value.as_str()), ("A", "A"));
        assert_eq!((rows[3].name.as_str(), rows[3].value.as_str()), ("X", "None"), "a Game Boy has no X");
        assert_eq!(rows[3].desc, "X does nothing in this game.");
        assert!(rows[1].desc.starts_with("A presses A"));
    }

    #[test]
    fn remapping_a_button_cycles_through_the_described_buttons_and_stores_only_changes() {
        let (mut m, mut s) = menu_with(with_controls(), &[]);
        // Row 3 is X: None → (one right) A.
        m.control_step(3, 1, &mut s);
        assert_eq!(m.control_rows()[3].value, "A");
        assert_eq!(s.values["ctl.x"], "a");
        // Left from A goes back to None. The default is the identity (X presses X), so None is stored.
        m.control_step(3, -1, &mut s);
        assert_eq!(m.control_rows()[3].value, "None");
        assert_eq!(s.values["ctl.x"], "none", "None differs from the default identity, so it is stored");
        // Row 1 is A: right → B.
        m.control_step(1, 1, &mut s);
        assert_eq!(m.control_rows()[1].value, "B");
        assert_eq!(s.values["ctl.a"], "b");
        // Reset removes every ctl.* entry.
        let last = m.control_count() - 1;
        m.control_step(last, 1, &mut s);
        assert!(s.values.keys().all(|k| !k.starts_with("ctl.")));
        assert_eq!(m.control_rows()[1].value, "A");
    }

    #[test]
    fn the_stick_toggle_is_stored_only_when_it_differs_from_the_system_default() {
        let (mut m, mut s) = menu_with(with_controls(), &[]);
        m.control_step(0, 1, &mut s);
        assert_eq!(s.values["ctl.stick_dpad"], "off");
        assert_eq!(m.control_rows()[0].value, "Off");
        m.control_step(0, 1, &mut s);
        assert!(!s.values.contains_key("ctl.stick_dpad"));
    }

    #[test]
    fn a_core_without_descriptors_offers_the_standard_buttons() {
        let mut f = with_controls();
        f.controls.targets.clear();
        let (m, _) = menu_with(f, &[]);
        assert_eq!(m.control_rows()[3].value, "X");
    }

    #[test]
    fn restore_defaults_resets_the_screen_state() {
        let (mut m, mut s) = menu_with(with_controls(), &[]);
        m.step(None, 1, 1, &mut s);
        m.control_step(1, 1, &mut s);
        s.restore_defaults();
        m.restored();
        assert_eq!(m.rows(None)[1].value, "OFF");
        assert_eq!(m.control_rows()[1].value, "A");
    }
}
