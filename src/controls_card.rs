//! The controls card: one picture per console of where the console's buttons are on the handheld, and what HOME does
//! on top (docs/user/controls-card.md). A player should get it in three seconds.
//!
//! The picture is the handheld's own buttons in their places, and in each the console's name for it: on a SNES the
//! right button says A, on a PlayStation the bottom one has the cross. Buttons the console does not have stay a faint
//! outline; extras of the emulator (turbo, the Boktai sun sensor) show only when the game needs them or the player
//! put them on a button.
//!
//! Nothing on it is made up and nothing is a raw emulator string. The Pocket side is `controls::Mapping`, the player's
//! own remaps (`ctl.*`) included. The console side is the table below (the names of the cores' own
//! `retro_input_descriptor` lists, written the way the console's box writes them), or, for a console without a
//! table, the core's names as far as they are a console button's plain name (`plain_name`). A console with neither
//! gets the short card: D-pad and buttons play, HOME is the menu.
//!
//! Model and painter live here so the shelf (the game card's Controls row, the first start of a console), the pause
//! menu (Options > Controls) and the Android launcher and player draw the same card; the colours differ (`Colors`).

use crate::controls::{self, Mapping, Target};
use crate::gfx::Canvas;
use crate::text::{Face, Text};

/// One button of the handheld on the card.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Slot {
    /// What it is on the console; None: it does nothing in this game.
    pub label: Option<String>,
    /// The player changed it (a remap, the stick switched), so the card marks it.
    pub changed: bool,
}

impl Slot {
    fn named(label: impl Into<String>) -> Slot {
        Slot { label: Some(label.into()), changed: false }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Card {
    pub system_name: String,
    /// The console's own names are known (the table's, or plain names from the core). False: the short card.
    pub known: bool,
    /// By Mini V2 button id (`controls::KEYS`): what each presses on the console. Extras (turbo, sensors) the game does
    /// not need are None here, as the console has no such button.
    pub slots: [Slot; 16],
    /// The D-pad as a whole, and the sticks.
    pub dpad: Slot,
    pub left_stick: Slot,
    pub right_stick: Slot,
    /// The key that opens the menu ("HOME", "Back" on Android), and the shortcuts of pocket-core's own host that are
    /// switched on: (buttons, what it does).
    pub menu_key: &'static str,
    pub shortcuts: Vec<(&'static str, &'static str)>,
    /// Extras this game needs, on the handheld's button: ("L3", "Less sunlight").
    pub extras: Vec<(&'static str, &'static str)>,
    /// A line under the picture for the odd one out of a console (the N64's C buttons).
    pub note: Option<&'static str>,
    /// The letters printed on this device's face buttons, in paint order: top, left, right, bottom. None where the
    /// print is not known (Android: every maker prints its own); the card then goes by the places alone.
    pub faces: Option<[&'static str; 4]>,
    /// Android without a gamepad: the touch buttons carry the same names, the card says so.
    pub touch: bool,
}

/// The printed face letters (top, left, right, bottom) of a device with this printed layout. The layout of the
/// menus a player picked changes no printing, so the builders pass the device's own (`device::menu_layout`).
pub fn face_letters(layout: crate::button_layout_model::ButtonLayout) -> [&'static str; 4] {
    match layout {
        crate::button_layout_model::ButtonLayout::Nintendo => ["X", "Y", "A", "B"],
        crate::button_layout_model::ButtonLayout::Xbox => ["Y", "X", "B", "A"],
    }
}

/// The HOME shortcuts that are switched on for the game (Options > Shortcuts).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shortcuts {
    pub save: bool,
    pub load: bool,
    /// HOME + X, Zoom: off unless Options > Shortcuts turned it on.
    pub zoom: bool,
}

impl Shortcuts {
    /// From a game's settings (`options::GameSettings::effective`).
    pub fn from_values(values: &std::collections::BTreeMap<String, String>) -> Shortcuts {
        let on = |key: &str| values.get(key).map_or(true, |v| v != "off");
        Shortcuts { save: on("hk_save"), load: on("hk_load"), zoom: values.get("hk_zoom").is_some_and(|v| v == "on") }
    }
}

impl Card {
    /// The player changed something the card shows (a remap, the stick).
    pub fn has_changes(&self) -> bool {
        self.slots.iter().any(|s| s.changed) || self.dpad.changed || self.left_stick.changed
    }

    /// The card as sentences, for a screen reader: one per button that does something.
    pub fn lines(&self) -> Vec<String> {
        let tr = crate::i18n::tr;
        let mut out = vec![self.system_name.clone()];
        if !self.known {
            out.push(tr("Play with the D-pad and the buttons."));
        } else {
            let places = [(9usize, "Top button"), (1, "Left button"), (8, "Right button"), (0, "Bottom button")];
            for (id, place) in places.into_iter().chain(SHOULDERS.iter().copied()).chain([(2, "Select"), (3, "Start")]) {
                if let Some(l) = &self.slots[id].label {
                    // Console names stay as the console prints them (assets/i18n/keep.txt).
                    out.push(format!("{}: {}", tr(place), l));
                }
            }
            for (stick, s) in [("Left stick", &self.left_stick), ("Right stick", &self.right_stick)] {
                if let Some(l) = s.label.as_ref().filter(|_| stick_drawn(s, stick == "Left stick")) {
                    out.push(format!("{}: {}", tr(stick), tr(l)));
                }
            }
        }
        out.extend(self.extras.iter().map(|(k, w)| format!("{}: {}", tr(k), tr(w))));
        out.extend(self.note.map(tr));
        out.push(format!("{}: {}", tr(self.menu_key), tr("Pause menu")));
        out.extend(self.shortcuts.iter().map(|(k, w)| format!("{}: {}", tr(k), tr(w))));
        if self.touch {
            out.push(tr("The buttons on the screen have the same names."));
        }
        out
    }
}

/// The shoulder buttons in paint order, with their names on the handheld.
const SHOULDERS: &[(usize, &str)] = &[(12, "L2"), (10, "L1"), (11, "R1"), (13, "R2")];

/// What the card is built from.
pub struct Input<'a> {
    pub system: &'a str,
    pub system_name: &'a str,
    /// The game's name: a few games need an extra (Boktai's sun sensor).
    pub game: &'a str,
    /// The player's layout for this game (`Mapping::from_values` of the game's settings).
    pub mapping: &'a Mapping,
    /// The core's own button names, empty before a game of this console ran.
    pub core_names: &'a [Target],
    /// The shortcuts that are on, for a game on pocket-core's own host; None for any other emulator (HOME opens the
    /// menu there, the rest is the emulator's).
    pub shortcuts: Option<Shortcuts>,
    /// The device's printed button layout (`face_letters`); None where it is not known (Android).
    pub layout: Option<crate::button_layout_model::ButtonLayout>,
    /// The key that opens the pause menu: "HOME" on the handhelds, "Back" on Android, "Pause" on a touch screen.
    pub menu_key: &'static str,
    /// Android without a gamepad.
    pub touch: bool,
}

/// One line per console that has had its card at a first start.
const SEEN_FILE: &str = "controls-card-seen";

fn seen_console(system: &str) -> &str {
    match system {
        "sfc" | "snesh" => "snes",
        "famicom" | "nesh" => "nes",
        "gbh" => "gb",
        "gbch" => "gbc",
        "gbah" | "gbav" => "gba",
        _ => system,
    }
}

/// Whether the first-start card is due for `system`, written down as shown: once for each console, never to a child.
/// The mark is written here, so a game that dies right after still counts as told.
pub fn first_start_due(data_dir: &std::path::Path, system: &str, kids: bool, resuming: bool) -> bool {
    if kids {
        return false;
    }
    let system = seen_console(system);
    let file = data_dir.join(SEEN_FILE);
    let seen = match std::fs::read_to_string(&file) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => {
            eprintln!("controls card: could not read previous consoles: {e}");
            return false;
        }
    };
    if seen.lines().any(|l| seen_console(l) == system) {
        return false;
    }
    // Persist before showing: a failed launch or a power cut must never erase earlier consoles.
    // If storage cannot remember it, skip the interruption instead of showing it on every start.
    if let Err(e) = crate::fsutil::write_atomic(&file, format!("{seen}{system}\n").as_bytes()) {
        eprintln!("controls card: could not remember {system}: {e}");
        return false;
    }
    // An existing game continues immediately, also on the first start after an update adds this card.
    !resuming
}

/// A console whose buttons the table knows.
struct Pad {
    /// The ROCKNIX folder names, and (on Linux) every folder pocket-core plays with one of `cores`: `snesh` and `sfc` are
    /// the SNES too.
    systems: &'static [&'static str],
    cores: &'static [&'static str],
    /// RetroPad id → the console's name for it. Ids not listed do nothing on that console (the D-pad is not listed).
    buttons: &'static [(u8, &'static str)],
    /// What the Pocket's left stick is when it is not a D-pad, and the right one.
    left_stick: Option<&'static str>,
    right_stick: Option<&'static str>,
    note: Option<&'static str>,
    /// Buttons the emulator adds that are no console button (turbo, sensors, mode switches): off the card unless the
    /// game needs them (`special`) or the player put one on a button of their own.
    extras: &'static [u8],
    /// An extra a few games need: (RetroPad id, what it does in player words), and those games' name parts.
    special: (&'static [(u8, &'static str)], &'static [&'static str]),
}

// ids: 0 B, 1 Y, 2 Select, 3 Start, 8 A, 9 X, 10 L1, 11 R1, 12 L2, 13 R2, 14 L3, 15 R3 (`controls::KEYS`).
const PADS: &[Pad] = &[
    // Pinned YabaSanshiro and Flycast descriptors (see android/cores.lock.json). Also known before the first Android start.
    Pad {systems: &["saturn"], cores: &["yabasanshiro"],
        buttons: &[(0,"A"),(8,"B"),(10,"C"),(1,"X"),(9,"Y"),(11,"Z"),(12,"L"),(13,"R"),(3,"Start")],
        left_stick: None, right_stick: None, note: None, extras: &[], special: (&[], &[])},
    Pad {systems: &["dreamcast"], cores: &["flycast"],
        buttons: &[(0,"A"),(8,"B"),(1,"X"),(9,"Y"),(12,"L"),(13,"R"),(3,"Start")],
        left_stick: Some("Analog stick"), right_stick: None, note: None, extras: &[], special: (&[], &[])},
    // Gambatte, Nestopia: A and B, Turbo on X and Y.
    Pad {
        systems: &["gb", "gbc", "nes", "famicom"],
        cores: &["gambatte", "nestopia"],
        buttons: &[(8, "A"), (0, "B"), (9, "Turbo A"), (1, "Turbo B"), (2, "Select"), (3, "Start")],
        left_stick: None,
        right_stick: None,
        note: None,
        extras: &[9, 1],
        special: (&[], &[]),
    },
    // mGBA.
    Pad {
        systems: &["gba"],
        cores: &["mgba"],
        buttons: &[(8, "A"), (0, "B"), (10, "L"), (11, "R"), (9, "Turbo A"), (1, "Turbo B"), (13, "Turbo R"), (12, "Turbo L"), (2, "Select"), (3, "Start"), (15, "Brighten Solar Sensor"), (14, "Darken Solar Sensor")],
        left_stick: None,
        right_stick: None,
        note: None,
        extras: &[9, 1, 13, 12, 15, 14],
        // The Boktai games read a light sensor on the cartridge; mGBA puts the sun on L3 and R3.
        special: (&[(14, "Less sunlight"), (15, "More sunlight")], &["boktai", "bokura no taiyou"]),
    },
    // Snes9x.
    Pad {
        systems: &["snes", "sfc"],
        cores: &["snes9x"],
        buttons: &[(8, "A"), (0, "B"), (9, "X"), (1, "Y"), (10, "L"), (11, "R"), (2, "Select"), (3, "Start")],
        left_stick: None,
        right_stick: None,
        note: None,
        extras: &[],
        special: (&[], &[]),
    },
    // PCSX ReARMed: a DualShock.
    Pad {
        systems: &["psx"],
        cores: &["pcsx_rearmed"],
        buttons: &[(0, "Cross"), (8, "Circle"), (1, "Square"), (9, "Triangle"), (10, "L1"), (11, "R1"), (12, "L2"), (13, "R2"), (14, "L3"), (15, "R3"), (2, "Select"), (3, "Start")],
        left_stick: Some("Left stick"),
        right_stick: Some("Right stick"),
        note: None,
        extras: &[],
        special: (&[], &[]),
    },
    // PPSSPP: the PSP's four shapes in the PlayStation's places, L and R, and its one analog stick.
    Pad {
        systems: &["psp"],
        cores: &["ppsspp"],
        buttons: &[(0, "Cross"), (8, "Circle"), (1, "Square"), (9, "Triangle"), (10, "L"), (11, "R"), (2, "Select"), (3, "Start")],
        left_stick: Some("Analog stick"),
        right_stick: None,
        note: None,
        extras: &[],
        special: (&[], &[]),
    },
    // Genesis Plus GX (handhelds) and clownmdemu (Android) name them alike: A B C on the left, bottom and right face
    // buttons, Y on top, X and Z on L1 and R1, Mode on Select (a six-button pad's).
    Pad {
        systems: &["megadrive", "megadrive-japan", "genesis", "genh", "megadriveh"],
        cores: &["genesis_plus_gx", "clownmdemu"],
        buttons: &[(1, "A"), (0, "B"), (8, "C"), (10, "X"), (9, "Y"), (11, "Z"), (2, "Mode"), (3, "Start")],
        left_stick: None,
        right_stick: None,
        note: None,
        extras: &[],
        special: (&[], &[]),
    },
    // Gearsystem: 1 and 2 on the bottom and right face buttons, Select resets the console. A Master System's pad has
    // no Start: the core's Start is the console's Pause button.
    Pad {
        systems: &["mastersystem", "sg-1000"],
        cores: &["gearsystem"],
        buttons: &[(0, "1"), (8, "2"), (2, "Reset"), (3, "Pause")],
        left_stick: None,
        right_stick: None,
        note: None,
        extras: &[],
        special: (&[], &[]),
    },
    Pad {
        systems: &["gamegear", "ggh"],
        cores: &[],
        buttons: &[(0, "1"), (8, "2"), (2, "Reset"), (3, "Start")],
        left_stick: None,
        right_stick: None,
        note: None,
        extras: &[],
        special: (&[], &[]),
    },
    // Mupen64Plus-Next (Independent C-button Controls off, its default): A and B on the bottom and left face button,
    // the C buttons on the right stick, or on the face buttons while R2 is held.
    Pad {
        systems: &["n64"],
        cores: &["mupen64plus_next"],
        buttons: &[(0, "A"), (1, "B"), (12, "Z"), (10, "L"), (11, "R"), (3, "Start"), (13, "C mode"), (8, "C (R2)"), (9, "C (R2)")],
        left_stick: Some("Control stick"),
        right_stick: Some("C buttons"),
        note: Some("Hold R2 and the face buttons are C buttons."),
        // R2's C mode is in the note; the face buttons' C role is the note's too.
        extras: &[13, 8, 9],
        special: (&[], &[]),
    },
];

/// The libretro core pocket-core plays `system` with (Linux; the table goes by folder names elsewhere).
fn core_of(system: &str) -> Option<&'static str> {
    #[cfg(target_os = "linux")]
    {
        crate::hostsys::find(system).map(|h| h.core)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = system;
        None
    }
}

fn pad(system: &str) -> Option<&'static Pad> {
    // The Famicom Disk System has Nestopia's disk buttons on L1 and R1, which the NES table doesn't know.
    let core = core_of(system).filter(|_| system != "fds");
    // The folder's own table first: the Game Gear and the Master System share a core, not a pad.
    PADS.iter().find(|p| p.systems.contains(&system)).or_else(|| PADS.iter().find(|p| core.is_some_and(|c| p.cores.contains(&c))))
}

/// Whether the card can name the console's buttons before a game of it ran.
pub fn has_names(system: &str) -> bool {
    pad(system).is_some() || !crate::platform::console_buttons(system).is_empty()
}

/// Every name a console button may have on the card, as boxes and manuals write them. A core's name outside this list
/// ("Darken Solar Sensor", "(FDS) Eject Disk", "Turbo A") is never shown: the card leaves the button out instead.
/// Left: a core's spelling; right: the card's.
const PLAIN: &[(&str, &str)] = &[
    ("A", "A"), ("B", "B"), ("C", "C"), ("X", "X"), ("Y", "Y"), ("Z", "Z"), ("L", "L"), ("R", "R"),
    ("L1", "L1"), ("R1", "R1"), ("L2", "L2"), ("R2", "R2"), ("L3", "L3"), ("R3", "R3"),
    ("1", "1"), ("2", "2"), ("I", "I"), ("II", "II"), ("Button 1", "1"), ("Button 2", "2"), ("Button I", "I"), ("Button II", "II"),
    ("Start", "Start"), ("Select", "Select"), ("Mode", "Mode"), ("Run", "Run"), ("Pause", "Pause"), ("Reset", "Reset"), ("Option", "Option"),
    ("Option 1", "Option 1"), ("Option 2", "Option 2"), ("Fire", "Fire"),
    ("Cross", "Cross"), ("Circle", "Circle"), ("Square", "Square"), ("Triangle", "Triangle"),
    ("L Shoulder", "L"), ("R Shoulder", "R"), ("Z Trigger", "Z"), ("A Button", "A"), ("B Button", "B"),
    ("C-Up", "C up"), ("C-Down", "C down"), ("C-Left", "C left"), ("C-Right", "C right"),
];

/// A core's button name as the card may show it, or None when it is no plain console button name.
pub fn plain_name(core: &str) -> Option<&'static str> {
    let core = core.trim();
    PLAIN.iter().find(|(raw, _)| raw.eq_ignore_ascii_case(core)).map(|(_, plain)| *plain)
}

/// The name of console button `id`, or None when the console has no such button. The table first, then the core's
/// plain names; a raw core string never.
fn console_name(system: &str, core_names: &[Target], id: u8) -> Option<String> {
    if (4..=7).contains(&id) {
        return Some(controls::standard_name(id).to_string());
    }
    let core = core_names.iter().find(|t| t.id == id).map(|t| t.label.as_str());
    if let Some(p) = pad(system) {
        // A core option can move buttons (the N64's independent C buttons): its plain names win then.
        if let Some(plain) = core.and_then(plain_name).filter(|_| core_moved(p, core_names)) {
            return Some(plain.to_string());
        }
        return p.buttons.iter().find(|(i, _)| *i == id).map(|(_, n)| n.to_string());
    }
    if !core_names.is_empty() {
        return core.and_then(plain_name).map(str::to_string);
    }
    controls::targets_for(system, &[]).into_iter().find(|t| t.id == id).and_then(|t| plain_name(&t.label).map(str::to_string))
}

/// Every console button of `system` with a name, for the remap menus: the core's own list when it ran, else the
/// table's, extras included. Empty when neither is known.
pub fn console_targets(system: &str, core_names: &[Target]) -> Vec<Target> {
    if !core_names.is_empty() {
        return core_names.iter().filter(|t| !(4..=7).contains(&t.id)).cloned().collect();
    }
    if let Some(p) = pad(system) {
        return p.buttons.iter().map(|(id, n)| Target { id: *id, label: n.to_string() }).collect();
    }
    controls::targets_for(system, &[]).into_iter().filter(|t| !(4..=7).contains(&t.id)).collect()
}

/// The core reports a layout the table does not describe: one of the table's plain names sits on another button.
fn core_moved(p: &Pad, core_names: &[Target]) -> bool {
    core_names.iter().any(|t| {
        // Only a name of this console's own buttons, on another button than the table's: "R" on the N64's R2. A plain
        // word the console does not have ("Fire" from a SNES core) moves nothing.
        let table = p.buttons.iter().find(|(i, _)| *i == t.id).and_then(|(_, n)| plain_name(n));
        let ours = |name: &str| p.buttons.iter().any(|(_, n)| plain_name(n) == Some(name));
        plain_name(&t.label).is_some_and(|core| ours(core) && table != Some(core))
    })
}

/// The handheld's name of its button `id`: the printed letter of a face button where it is known, else its place.
fn device_name(id: usize, faces: Option<[&'static str; 4]>) -> &'static str {
    let f = faces.unwrap_or(["Top button", "Left button", "Right button", "Bottom button"]);
    match id {
        9 => f[0],
        1 => f[1],
        8 => f[2],
        0 => f[3],
        2 => "Select",
        3 => "Start",
        10 => "L1",
        11 => "R1",
        12 => "L2",
        13 => "R2",
        14 => "L3",
        15 => "R3",
        _ => "D-pad",
    }
}

pub fn build(i: &Input) -> Card {
    let defaults = Mapping::defaults(i.system);
    let table = pad(i.system);
    let game = i.game.to_lowercase();
    let special: &[(u8, &str)] = table.filter(|p| p.special.1.iter().any(|g| game.contains(g))).map_or(&[], |p| p.special.0);
    // A core option that moved the buttons (the N64's independent C buttons) makes its plain names the card's.
    let moved = table.is_some_and(|p| core_moved(p, i.core_names));
    let extra = |t: u8| !moved && table.is_some_and(|p| p.extras.contains(&t)) && !special.iter().any(|(s, _)| *s == t);
    let slots: [Slot; 16] = std::array::from_fn(|id| {
        let target = i.mapping.target[id];
        let changed = target != defaults.target[id];
        // An extra stays on the card where the player put it themselves.
        let label = target.filter(|t| changed || !extra(*t)).and_then(|t| console_name(i.system, i.core_names, t));
        Slot { label, changed }
    });
    let faces_named = [0usize, 1, 8, 9].iter().any(|id| slots[*id].label.is_some());
    let known = faces_named && (table.is_some() || !i.core_names.is_empty() || has_names(i.system));
    let faces = i.layout.map(face_letters);
    let mut extras = Vec::new();
    for (sid, what) in special {
        if let Some(id) = (0..16).find(|id| i.mapping.target[*id] == Some(*sid)) {
            extras.push((device_name(id, faces), *what));
        }
    }
    let dpad_same = (4..8).all(|id| i.mapping.target[id] == defaults.target[id]);
    let dpad = Slot { label: Some(if dpad_same { "D-pad" } else { "D-pad (changed)" }.to_string()), changed: !dpad_same };
    let analog = !controls::stick_dpad_default(i.system);
    let left_stick = if i.mapping.stick_dpad {
        Slot { label: Some("D-pad".into()), changed: i.mapping.stick_dpad != defaults.stick_dpad }
    } else if analog {
        Slot { label: Some(table.and_then(|p| p.left_stick).unwrap_or("Analog stick").to_string()), changed: i.mapping.stick_dpad != defaults.stick_dpad }
    } else {
        Slot { label: None, changed: true }
    };
    let right_stick = match (table.and_then(|p| p.right_stick), analog) {
        (Some(name), _) => Slot::named(name),
        (None, true) if table.is_none() => Slot::named("Right stick"),
        _ => Slot::default(),
    };
    let mut shortcuts = Vec::new();
    if let Some(on) = i.shortcuts {
        if on.save {
            shortcuts.push(("HOME + R1", "Save"));
        }
        if on.load {
            shortcuts.push(("HOME + L1", "Load"));
        }
        if on.zoom {
            shortcuts.push(("HOME + X", "Zoom"));
        }
    }
    // Independent C-button Controls changes R2 into the R shoulder: do not claim it still selects C mode.
    let note = table.and_then(|p| p.note).filter(|_| i.core_names.is_empty() || i.core_names.iter().any(|t| t.id == 13 && t.label == "C Buttons Mode"));
    Card {
        system_name: i.system_name.to_string(),
        known,
        slots,
        dpad,
        left_stick,
        right_stick,
        menu_key: i.menu_key,
        shortcuts,
        extras,
        note,
        faces,
        touch: i.touch,
    }
}

/// The Brick's two extra keys under the shortcuts, with what they do now (Settings > Lights & buttons); one that does
/// nothing is left out.
#[cfg(feature = "brick")]
pub fn with_brick_keys(mut card: Card, keys: crate::brick_hw::FKeys) -> Card {
    for (name, key) in [("F1", keys.f1), ("F2", keys.f2)] {
        if key != crate::brick_hw::FKey::Nothing {
            card.shortcuts.push((name, key.label()));
        }
    }
    card
}

/// The card's colours: the shelf takes the palette in use, the pause menu its dark one.
#[derive(Clone, Copy)]
pub struct Colors {
    pub bg: u32,
    pub line: u32,
    pub text: u32,
    pub dim: u32,
    pub accent: u32,
}

/// Text sizes of the design (`type_scale` grows them with Text size).
const LABEL: u32 = 30;
const SMALL: u32 = 24;

/// What stands under a stick: the right one's job when it is not just a stick (the N64's C buttons), anything the
/// player changed. "Analog stick" under a drawn stick says nothing.
fn stick_caption(s: &Slot, left: bool) -> Option<&str> {
    let l = s.label.as_deref()?;
    (s.changed || (!left && l != "Right stick" && l != "Analog stick")).then_some(l)
}

/// A left stick worth drawing: an analog one, or one the player switched.
fn stick_drawn(s: &Slot, left: bool) -> bool {
    s.label.is_some() && (!left || s.label.as_deref() != Some("D-pad") || s.changed)
}

/// The picture's room inside `area` (what `paint` leaves above the lines under it), for the layout tests.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn picture_room(text: &mut Text, card: &Card, area: (i32, i32, i32, i32)) -> (i32, i32, i32, i32) {
    let lines = bottom_lines(text, card, area.2);
    (area.0, area.1, area.2, (area.3 - bottom_height(card, &lines)).max(60))
}

/// Paints the card into `area` = (x, y, w, h). What does not fit is made smaller or cut with "…".
pub fn paint(c: &mut Canvas, text: &mut Text, card: &Card, area: (i32, i32, i32, i32), col: Colors) {
    let (x0, y0, w, h) = area;
    let lines = bottom_lines(text, card, w);
    let bottom = bottom_height(card, &lines);
    let top_h = (h - bottom).max(60);
    if card.known {
        paint_pad(c, text, card, (x0, y0, w, top_h), col);
    } else {
        paint_short(c, text, (x0, y0, w, top_h), col);
    }
    paint_bottom(c, text, card, &lines, (x0, y0 + top_h, w, bottom), col);
}

/// A button outline: what the player changed has a heavier one in the accent colour; one the console lacks is faint.
fn outline_disc(c: &mut Canvas, cx: i32, cy: i32, r: i32, col: Colors, on: bool) {
    let t = if on { 4 } else { 2 };
    c.disc(cx, cy, r, if on { col.accent } else { col.line });
    c.disc(cx, cy, r - t, col.bg);
}

fn ghost_disc(c: &mut Canvas, cx: i32, cy: i32, r: i32, col: Colors) {
    c.disc(cx, cy, r, crate::gfx::mix(col.bg, col.line, 110));
    c.disc(cx, cy, r - 2, col.bg);
}

fn outline_pill(c: &mut Canvas, x: i32, y: i32, w: i32, h: i32, col: Colors, on: bool) {
    let t = if on { 4 } else { 2 };
    crate::retro::pill(c, x, y, w, h, h / 2, if on { col.accent } else { col.line });
    crate::retro::pill(c, x + t, y + t, w - 2 * t, h - 2 * t, (h - 2 * t) / 2, col.bg);
}

fn centered(c: &mut Canvas, text: &mut Text, face: Face, size: u32, cx: i32, base: i32, color: u32, s: &str) {
    let w = text.width(face, size, s);
    text.draw(c, face, size, cx - w / 2, base, color, s);
}

/// The PlayStation's four shapes, drawn: no font has them all, and they are what the player looks for.
fn shape(label: &str) -> Option<u8> {
    match label {
        "Cross" => Some(0),
        "Circle" => Some(1),
        "Square" => Some(2),
        "Triangle" => Some(3),
        _ => None,
    }
}

fn paint_shape(c: &mut Canvas, kind: u8, cx: i32, cy: i32, s: i32, t: i32, color: u32) {
    let line = |c: &mut Canvas, (ax, ay): (i32, i32), (bx, by): (i32, i32)| {
        let n = (ax - bx).abs().max((ay - by).abs()).max(1);
        for k in 0..=n {
            c.disc(ax + (bx - ax) * k / n, ay + (by - ay) * k / n, t / 2, color);
        }
    };
    match kind {
        0 => {
            line(c, (cx - s, cy - s), (cx + s, cy + s));
            line(c, (cx - s, cy + s), (cx + s, cy - s));
        }
        1 => c.ring(cx, cy, s + t / 2, t, color),
        2 => c.frame(cx - s, cy - s, 2 * s, 2 * s, t, color),
        _ => {
            let (top, l, r) = ((cx, cy - s - s / 6), (cx - s - s / 6, cy + s * 3 / 4), (cx + s + s / 6, cy + s * 3 / 4));
            line(c, top, l);
            line(c, l, r);
            line(c, r, top);
        }
    }
}

/// The biggest size of `s` in `face` that fits `room` wide and about `tall` high.
fn size_for(text: &mut Text, face: Face, s: &str, room: i32, tall: i32, max: u32) -> u32 {
    let mut size = max;
    while size > 14 && (text.width(face, size, s) > room || crate::type_scale::px(size) as i32 > tall) {
        size -= 2;
    }
    size
}

/// One face button: the console's name (or shape) big inside, and, where it differs, the printed letter of the
/// handheld's button small on its rim.
#[allow(clippy::too_many_arguments)]
fn face_button(c: &mut Canvas, text: &mut Text, (fx, fy, r): (i32, i32, i32), slot: &Slot, printed: Option<&str>, col: Colors) {
    let Some(label) = slot.label.as_deref() else {
        if slot.changed {
            outline_disc(c, fx, fy, r, col, true);
            centered(c, text, Face::Body, LABEL, fx, fy + crate::type_scale::mid(LABEL), col.dim, "—");
        } else {
            ghost_disc(c, fx, fy, r, col);
        }
        return;
    };
    outline_disc(c, fx, fy, r, col, slot.changed);
    let ink = if slot.changed { col.accent } else { col.text };
    let wants_badge = printed.is_some_and(|p| plain_name(label) != Some(p));
    let br = (r * 2 / 5).max(11);
    // The printed letter's badge sits on the rim at the lower right; it only comes when the name inside and the letter
    // both keep clear of each other at a readable size (a long remapped name with Large text goes without it).
    let badge_size = printed.map(|p| size_for(text, Face::Body, p, 2 * br - 6, br + 2, SMALL));
    let mut badge = wants_badge && badge_size.is_some_and(|s| text.width(Face::Body, s, printed.unwrap_or("")) <= 2 * br - 6 && crate::type_scale::px(s) as i32 <= br + 2);
    if let Some(kind) = shape(label) {
        paint_shape(c, kind, fx, fy, r * 4 / 10, (r / 9).max(3), ink);
    } else {
        // As the console prints it: never translated ("Run" on a PC Engine is no verb).
        text.literal(|text| {
            let (room, tall, shift) = (r, r * 3 / 5, r / 6);
            let small = size_for(text, Face::Title, label, room, tall, (r as u32).max(20));
            let fits = text.width(Face::Title, small, label) <= room && crate::type_scale::px(small) as i32 <= tall;
            badge = badge && fits;
            let (size, shift) = if badge { (small, shift) } else { (size_for(text, Face::Title, label, 2 * r - 14, r, (r as u32).max(20)), 0) };
            centered(c, text, Face::Title, size, fx - shift, fy - shift + crate::type_scale::mid(size), ink, label);
        });
    }
    // The handheld's own letter, where the console calls the button something else.
    if let (true, Some(p), Some(size)) = (badge, printed, badge_size) {
        // On the rim, mostly outside: the name inside keeps its room.
        let (bx, by) = (fx + r * 17 / 20, fy + r * 17 / 20);
        c.disc(bx, by, br, col.line);
        c.disc(bx, by, br - 2, col.bg);
        centered(c, text, Face::Body, size, bx, by + crate::type_scale::mid(size), col.dim, p);
    }
}

/// A pill for a button that is not round: the handheld's name small and dim, the console's after it; one name when
/// they are the same. Returns its width.
fn pill_w(text: &mut Text, device: &str, label: &str) -> i32 {
    let lw = text.literal(|text| text.width(Face::Body, LABEL, label));
    if plain_name(label) == Some(device) || label == device {
        return lw + 44;
    }
    text.width(Face::Body, SMALL, device) + 12 + lw + 48
}

#[allow(clippy::too_many_arguments)]
fn pill(c: &mut Canvas, text: &mut Text, x: i32, y: i32, w: i32, h: i32, device: &str, slot: &Slot, col: Colors) {
    let label = slot.label.as_deref().unwrap_or("—");
    outline_pill(c, x, y, w, h, col, slot.changed);
    let ink = if slot.changed { col.accent } else { col.text };
    let b = y + h / 2 + crate::type_scale::mid(LABEL);
    if plain_name(label) == Some(device) || label == device {
        text.literal(|text| {
            let s = text.fit(Face::Body, LABEL, label, w - 24);
            centered(c, text, Face::Body, LABEL, x + w / 2, b, ink, &s);
        });
        return;
    }
    let pen = x + 22 + text.draw(c, Face::Body, SMALL, x + 22, y + h / 2 + crate::type_scale::mid(SMALL), col.dim, device) + 12;
    text.literal(|text| {
        let s = text.fit(Face::Body, LABEL, label, x + w - 22 - pen);
        text.draw(c, Face::Body, LABEL, pen, b, ink, &s);
    });
}

/// Where the picture goes in the room it has: the face buttons' radius `r`, their distance `d` from the middle, the
/// middle's height `cy`, and how much of the room the picture fills (`share`, tests: it grows with the screen).
pub(crate) struct Geometry {
    pub sticks: bool,
    pub r: i32,
    pub d: i32,
    pub cy: i32,
    /// The picture's height (shoulders to the lowest button) over the room's.
    #[cfg_attr(not(test), allow(dead_code))]
    pub share: f32,
}

pub(crate) fn geometry(card: &Card, area: (i32, i32, i32, i32)) -> Geometry {
    let (_, y0, w, h) = area;
    let pill_h = crate::type_scale::px(LABEL) as i32 + 22;
    let shoulders = SHOULDERS.iter().any(|(id, _)| card.slots[*id].label.is_some() || card.slots[*id].changed);
    let top = if shoulders { pill_h + 18 } else { 0 };
    let sticks = (stick_drawn(&card.left_stick, true) || stick_drawn(&card.right_stick, false)) && {
        #[cfg(feature = "brick")]
        {
            crate::device::current().has_sticks()
        }
        #[cfg(not(feature = "brick"))]
        {
            true
        }
    };
    // A line under the sticks for what they do, when one says something.
    let caption = if sticks && (stick_caption(&card.left_stick, true).is_some() || stick_caption(&card.right_stick, false).is_some()) {
        crate::type_scale::px(SMALL) as i32 + 14
    } else {
        0
    };
    let room = h - top - 10 - caption;
    // The diamond is 5.8 radii high, a row of sticks adds 2.6; across, the face buttons take 5.8 radii of a half.
    // The picture grows with the screen: no cap but the room (a 1920 x 1080 card is as full as a 1240 x 1080 one).
    let r = ((room * 10) / if sticks { 84 } else { 58 }).min(w * 10 / 2 / 64).max(14);
    let d = r * 19 / 10;
    let block = 2 * d + 2 * r + if sticks { r * 26 / 10 } else { 0 };
    let cy = y0 + top + (room - block).max(0) / 2 + d + r;
    Geometry { sticks, r, d, cy, share: (top + block + caption) as f32 / h.max(1) as f32 }
}

fn paint_pad(c: &mut Canvas, text: &mut Text, card: &Card, area: (i32, i32, i32, i32), col: Colors) {
    let (x0, y0, w, _) = area;
    let pill_h = crate::type_scale::px(LABEL) as i32 + 22;
    let slot = |id: usize| &card.slots[id];
    // --- shoulders: only the ones the console has, L2 L1 at the left, R1 R2 at the right.
    let shoulders: Vec<(usize, &str)> = SHOULDERS.iter().copied().filter(|(id, _)| slot(*id).label.is_some() || slot(*id).changed).collect();
    let gap = 12;
    let max_w = (w - 3 * gap) / 4;
    let (mut lx_pen, mut rx_pen) = (x0, x0 + w);
    for (id, name) in shoulders.iter().filter(|(id, _)| *id == 12 || *id == 10) {
        let pw = pill_w(text, name, slot(*id).label.as_deref().unwrap_or("—")).min(max_w);
        pill(c, text, lx_pen, y0, pw, pill_h, name, slot(*id), col);
        lx_pen += pw + gap;
    }
    for (id, name) in shoulders.iter().rev().filter(|(id, _)| *id == 13 || *id == 11) {
        let pw = pill_w(text, name, slot(*id).label.as_deref().unwrap_or("—")).min(max_w);
        rx_pen -= pw;
        pill(c, text, rx_pen, y0, pw, pill_h, name, slot(*id), col);
        rx_pen -= gap;
    }
    // --- the middle: the D-pad (and an analog left stick under it) at the left, the face buttons at the right, the
    // right stick under them, SELECT and START between.
    let Geometry { sticks, r, d, cy, .. } = geometry(card, area);
    let lx = x0 + w * 25 / 100;
    let rx = x0 + w * 75 / 100;
    // D-pad: a plus of two bars.
    let arm = r * 12 / 10;
    let t = r * 3 / 4 + 2;
    for inner in [false, true] {
        let inset = if inner { 2 } else { 0 };
        let colr = if inner { col.bg } else if card.dpad.changed { col.accent } else { col.line };
        c.round_rect(lx - arm + inset, cy - t / 2 + inset, 2 * arm - 2 * inset, t - 2 * inset, 3, colr);
        c.round_rect(lx - t / 2 + inset, cy - arm + inset, t - 2 * inset, 2 * arm - 2 * inset, 3, colr);
    }
    if card.dpad.changed {
        centered(c, text, Face::Body, SMALL, lx, cy + arm + 10 + crate::type_scale::px(SMALL) as i32, col.accent, "D-pad (changed)");
    }
    let faces = [(9usize, rx, cy - d, 0usize), (1, rx - d, cy, 1), (8, rx + d, cy, 2), (0, rx, cy + d, 3)];
    for (id, fx, fy, place) in faces {
        face_button(c, text, (fx, fy, r), slot(id), card.faces.map(|f| f[place]), col);
    }
    let sy = cy + d + r + r * 13 / 10;
    if sticks {
        for (on, x, s, mark) in [(stick_drawn(&card.left_stick, true), lx, &card.left_stick, "L"), (stick_drawn(&card.right_stick, false), rx, &card.right_stick, "R")] {
            if !on {
                continue;
            }
            outline_disc(c, x, sy, r, col, s.changed);
            c.disc(x, sy, r / 2, col.line);
            c.disc(x, sy, r / 2 - 2, col.bg);
            centered(c, text, Face::Body, SMALL, x, sy + crate::type_scale::mid(SMALL), col.dim, mark);
            if let Some(label) = stick_caption(s, x == lx) {
                let ink = if s.changed { col.accent } else { col.text };
                let fit = text.fit(Face::Body, SMALL, label, w / 2 - 16);
                centered(c, text, Face::Body, SMALL, x, sy + r + 10 + crate::type_scale::px(SMALL) as i32, ink, &fit);
            }
        }
    }
    // SELECT and START: in the middle, level with the lowest face button (or the sticks).
    let mids: Vec<(usize, &str)> = [(2usize, "Select"), (3, "Start")].into_iter().filter(|(id, _)| slot(*id).label.is_some() || slot(*id).changed).collect();
    let py = if sticks { sy } else { cy + d } - pill_h / 2;
    // Their room: between the sticks, or from the left edge (under the D-pad) to the lowest face button.
    let (from, to) = if sticks { (lx + r + 16, rx - r - 16) } else { (x0, rx - r - 16) };
    let label = |id: usize| slot(id).label.as_deref().unwrap_or("—");
    let total = |text: &mut Text, compact: bool| -> i32 {
        mids.iter().map(|(id, n)| pill_w(text, if compact { label(*id) } else { n }, label(*id))).sum::<i32>() + gap * (mids.len() as i32 - 1).max(0)
    };
    // "Select Reset" and "Start Pause" side by side are too wide on a small screen: then the console's names alone.
    let compact = total(text, false) > to - from;
    let all = total(text, compact);
    let mut pen = (x0 + w / 2 - all / 2).min(to - all).max(from);
    for (id, n) in &mids {
        let device = if compact { label(*id) } else { n };
        let pw = pill_w(text, device, label(*id)).min(to - from);
        pill(c, text, pen, py, pw, pill_h, device, slot(*id), col);
        pen += pw + gap;
    }
}

/// The short card of a console whose buttons are not known: a faint pad and one sentence.
fn paint_short(c: &mut Canvas, text: &mut Text, area: (i32, i32, i32, i32), col: Colors) {
    let (x0, y0, w, h) = area;
    let step = crate::type_scale::line(LABEL, 10);
    let r = ((h - 2 * step) * 10 / 72).clamp(10, 40);
    let d = r * 19 / 10;
    let cy = y0 + (h - 2 * step) / 2;
    let (lx, rx) = (x0 + w / 2 - 4 * r, x0 + w / 2 + 4 * r);
    let arm = r * 12 / 10;
    let t = r * 3 / 4 + 2;
    c.round_rect(lx - arm, cy - t / 2, 2 * arm, t, 3, col.line);
    c.round_rect(lx - t / 2, cy - arm, t, 2 * arm, 3, col.line);
    for (fx, fy) in [(rx, cy - d), (rx - d, cy), (rx + d, cy), (rx, cy + d)] {
        outline_disc(c, fx, fy, r, col, false);
    }
    let msg = "Play with the D-pad and the buttons.";
    let lines = text.wrap(Face::Body, LABEL, msg, w - 40);
    let mut y = cy + d + r + step;
    for l in &lines {
        centered(c, text, Face::Body, LABEL, x0 + w / 2, y, col.text, l);
        y += step;
    }
}

/// The lines under the picture, as (keys, what, colour role) rows: the menu key first and big, then the rest.
fn bottom_lines(text: &mut Text, card: &Card, w: i32) -> Vec<Vec<(&'static str, String)>> {
    let mut rows: Vec<Vec<(&'static str, String)>> = Vec::new();
    let mut small: Vec<(&'static str, String)> = card.extras.iter().map(|(k, v)| (*k, v.to_string())).collect();
    small.extend(card.shortcuts.iter().map(|(k, v)| (*k, v.to_string())));
    // Side by side while they fit.
    let mut row: Vec<(&'static str, String)> = Vec::new();
    let mut x = 0;
    for (k, v) in small {
        let pw = text.width(Face::Body, SMALL, k) + 10 + text.width(Face::Body, SMALL, &v) + 36;
        if x > 0 && x + pw > w {
            rows.push(std::mem::take(&mut row));
            x = 0;
        }
        x += pw;
        row.push((k, v));
    }
    if !row.is_empty() {
        rows.push(row);
    }
    rows
}

fn sentences(card: &Card) -> Vec<(&'static str, bool)> {
    let mut out = Vec::new();
    if let Some(n) = card.note.filter(|_| card.known) {
        out.push((n, false));
    }
    if card.touch {
        out.push(("The buttons on the screen have the same names.", false));
    }
    if card.has_changes() && card.known {
        out.push(("In colour: what you changed in Options › Controls.", true));
    }
    out
}

fn bottom_height(card: &Card, rows: &[Vec<(&'static str, String)>]) -> i32 {
    let (big, small) = (crate::type_scale::line(LABEL, 14), crate::type_scale::line(SMALL, 8));
    // The sentences, a rule, the menu line, the rows of shortcuts.
    sentences(card).len() as i32 * small + 18 + big + rows.len() as i32 * small + 6
}

fn paint_bottom(c: &mut Canvas, text: &mut Text, card: &Card, rows: &[Vec<(&'static str, String)>], area: (i32, i32, i32, i32), col: Colors) {
    let (x0, y0, w, _) = area;
    let small = crate::type_scale::line(SMALL, 8);
    let mut y = y0 + 4;
    for (s, accent) in sentences(card) {
        let s = text.fit(Face::Body, SMALL, s, w - 8);
        text.draw(c, Face::Body, SMALL, x0 + 4, y + small - 8, if accent { col.accent } else { col.dim }, &s);
        y += small;
    }
    c.rect(x0, y + 6, w, 2, col.line);
    y += 18;
    // The menu key in a keycap, then what it does: the one thing nobody can guess.
    let key_w = text.width(Face::Body, SMALL, card.menu_key) + 28;
    let kh = crate::type_scale::px(SMALL) as i32 + 14;
    let big = crate::type_scale::line(LABEL, 14);
    let base = y + big - 12;
    crate::retro::pill(c, x0, base - crate::type_scale::mid(LABEL) - kh / 2 - 2, key_w, kh, kh / 2, col.line);
    crate::retro::pill(c, x0 + 2, base - crate::type_scale::mid(LABEL) - kh / 2, key_w - 4, kh - 4, (kh - 4) / 2, col.bg);
    centered(c, text, Face::Body, SMALL, x0 + key_w / 2, base - crate::type_scale::mid(LABEL) + crate::type_scale::mid(SMALL) - 2, col.text, card.menu_key);
    let what = text.fit(Face::Body, LABEL, "Pause menu", w - key_w - 20);
    text.draw(c, Face::Body, LABEL, x0 + key_w + 16, base, col.text, &what);
    y += big;
    for row in rows {
        let mut x = x0;
        for (k, v) in row {
            x += text.draw(c, Face::Body, SMALL, x, y + small - 8, col.dim, k) + 10;
            x += text.draw(c, Face::Body, SMALL, x, y + small - 8, col.text, v) + 36;
        }
        y += small;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn card_for(system: &str, remaps: &[(&str, &str)], core: &[Target], shortcuts: Option<Shortcuts>) -> Card {
        let mut values = BTreeMap::new();
        for (k, v) in remaps {
            values.insert(format!("ctl.{k}"), v.to_string());
        }
        let mapping = Mapping::from_values(system, &values);
        build(&Input { system, system_name: system, game: "", mapping: &mapping, core_names: core, shortcuts, layout: Some(crate::button_layout_model::ButtonLayout::Nintendo), menu_key: "HOME", touch: false })
    }

    fn label(c: &Card, key: &str) -> Option<String> {
        c.slots[controls::key_id(key).unwrap() as usize].label.clone()
    }

    #[test]
    fn snes_buttons_are_the_consoles_own_letters_on_the_same_places() {
        let c = card_for("snes", &[], &[], None);
        assert!(c.known);
        // The Pocket's right button is the SNES A, the bottom one B, the top X, the left Y (Nintendo layout).
        assert_eq!([label(&c, "a"), label(&c, "b"), label(&c, "x"), label(&c, "y")], [Some("A".into()), Some("B".into()), Some("X".into()), Some("Y".into())]);
        assert_eq!([label(&c, "l"), label(&c, "r")], [Some("L".into()), Some("R".into())]);
        // The SNES has no L2, R2 or stick press.
        assert_eq!([label(&c, "l2"), label(&c, "r2"), label(&c, "l3")], [None, None, None]);
        assert_eq!(c.left_stick.label.as_deref(), Some("D-pad"), "no analog stick on a SNES");
        assert_eq!(c.right_stick.label, None);
    }


    #[test]
    fn gba_has_its_two_shoulders_and_a_b() {
        let c = card_for("gba", &[], &[], None);
        assert_eq!([label(&c, "a"), label(&c, "b"), label(&c, "l"), label(&c, "r")], [Some("A".into()), Some("B".into()), Some("L".into()), Some("R".into())]);
        assert_eq!([label(&c, "select"), label(&c, "start")], [Some("Select".into()), Some("Start".into())]);
    }

    #[test]
    fn playstation_face_buttons_are_the_four_shapes() {
        let c = card_for("psx", &[], &[], None);
        // PCSX ReARMed: Cross is the RetroPad B (bottom), Circle A (right), Square Y (left), Triangle X (top).
        assert_eq!([label(&c, "b"), label(&c, "a"), label(&c, "y"), label(&c, "x")], [Some("Cross".into()), Some("Circle".into()), Some("Square".into()), Some("Triangle".into())]);
        assert_eq!([label(&c, "l"), label(&c, "r"), label(&c, "l2"), label(&c, "r2")], [Some("L1".into()), Some("R1".into()), Some("L2".into()), Some("R2".into())]);
        assert_eq!(c.left_stick.label.as_deref(), Some("Left stick"), "a DualShock: the sticks go to the game");
        assert_eq!(c.right_stick.label.as_deref(), Some("Right stick"));
    }

    #[test]
    fn n64_has_its_c_buttons_on_the_right_stick() {
        let c = card_for("n64", &[], &[], None);
        assert_eq!([label(&c, "b"), label(&c, "y"), label(&c, "l2")], [Some("A".into()), Some("B".into()), Some("Z".into())]);
        assert_eq!(c.right_stick.label.as_deref(), Some("C buttons"));
        assert_eq!(c.left_stick.label.as_deref(), Some("Control stick"));
        assert!(c.note.is_some());
    }

    #[test]
    fn the_players_remaps_are_on_the_card_and_marked() {
        // SNES: the Pocket's X presses the SNES A, Y presses nothing.
        let c = card_for("snes", &[("x", "a"), ("y", "none")], &[], None);
        assert_eq!(label(&c, "x").as_deref(), Some("A"));
        assert!(c.slots[9].changed);
        assert_eq!(label(&c, "y"), None);
        assert!(c.slots[1].changed);
        assert!(!c.slots[8].changed, "A stays A");
        assert!(!c.dpad.changed);
        // A swapped D-pad is said.
        let c = card_for("snes", &[("up", "down")], &[], None);
        assert_eq!(c.dpad.label.as_deref(), Some("D-pad (changed)"));
    }

    #[test]
    fn the_stick_follows_the_players_choice() {
        let c = card_for("psx", &[("stick_dpad", "on")], &[], None);
        assert_eq!(c.left_stick.label.as_deref(), Some("D-pad"));
        assert!(c.left_stick.changed);
        let c = card_for("snes", &[("stick_dpad", "off")], &[], None);
        assert_eq!(c.left_stick.label, None, "a console without an analog stick: it does nothing");
    }

    #[test]
    fn a_cores_actions_and_extras_never_reach_the_card() {
        // What the cores really report (mGBA, Nestopia with the Disk System, Mupen64Plus-Next standard): the card shows
        // console names only, whatever the core calls its buttons.
        let reported: &[(&str, &[(u8, &str)])] = &[
            ("gba", &[(8, "A"), (0, "B"), (9, "Turbo A"), (1, "Turbo B"), (10, "L"), (11, "R"), (12, "Turbo L"), (13, "Turbo R"), (14, "Darken Solar Sensor"), (15, "Brighten Solar Sensor"), (2, "Select"), (3, "Start"), (4, "Up")]),
            ("fds", &[(8, "A"), (0, "B"), (10, "(FDS) Disk Side Change"), (11, "(FDS) Eject Disk"), (2, "Select"), (3, "Start")]),
            ("n64", &[(0, "A Button (C3)"), (1, "B Button (C2)"), (8, "(C1)"), (9, "(C4)"), (13, "C Buttons Mode"), (12, "Z Trigger"), (11, "R Shoulder"), (10, "L Shoulder"), (3, "Start")]),
            ("snes", &[(8, "Fire"), (0, "Jump")]),
            ("atari7800", &[(0, "Fire 1"), (8, "Fire 2"), (2, "Pause"), (3, "Reset")]),
        ];
        let allowed: Vec<&str> = PLAIN.iter().map(|(_, p)| *p).chain(PADS.iter().flat_map(|p| p.buttons.iter().map(|(_, n)| *n))).collect();
        for (system, names) in reported {
            let core: Vec<Target> = names.iter().map(|(id, l)| Target { id: *id, label: l.to_string() }).collect();
            let c = card_for(system, &[], &core, None);
            for (id, slot) in c.slots.iter().enumerate().filter(|(id, _)| !(4..8).contains(id)) {
                if let Some(l) = &slot.label {
                    assert!(allowed.contains(&l.as_str()) && !l.contains("Turbo") && !l.contains("Solar") && !l.contains("C mode"), "{system} button {id}: {l}");
                }
            }
            for line in c.lines() {
                for (_, raw) in names.iter().filter(|(_, r)| plain_name(r).is_none()) {
                    assert!(!line.contains(raw), "{system}: \"{raw}\" in \"{line}\"");
                }
            }
        }
        // A table's console keeps its names when the core calls them actions; a moved core layout uses its plain names.
        let c = card_for("snes", &[], &[Target { id: 8, label: "Fire".into() }, Target { id: 0, label: "Jump".into() }], None);
        assert_eq!([label(&c, "a"), label(&c, "b"), label(&c, "x")], [Some("A".into()), Some("B".into()), Some("X".into())]);
        // The GBA's turbo and sun stay off the card, unless a game needs the sun or the player put turbo on a button.
        let c = card_for("gba", &[], &[], None);
        assert_eq!([label(&c, "x"), label(&c, "y"), label(&c, "l3"), label(&c, "r3")], [None, None, None, None]);
        assert!(c.extras.is_empty());
        let c = card_for("gba", &[("x", "x")], &[], None);
        assert_eq!(label(&c, "x"), None, "the default is no change");
        let c = card_for("gba", &[("l2", "x")], &[], None);
        assert_eq!(label(&c, "l2").as_deref(), Some("Turbo A"), "the player's own choice is shown");
    }

    #[test]
    fn boktai_shows_its_sun_on_the_buttons_that_press_it() {
        let mapping = Mapping::from_values("gba", &BTreeMap::new());
        let input = |game| Input { system: "gba", system_name: "Game Boy Advance", game, mapping: &mapping, core_names: &[], shortcuts: None, layout: None, menu_key: "Back", touch: false };
        let c = build(&input("Boktai 2 - Solar Boy Django (USA)"));
        assert_eq!(c.extras, vec![("L3", "Less sunlight"), ("R3", "More sunlight")]);
        assert!(build(&input("Golden Sun (USA)")).extras.is_empty(), "the sun only for the games with the sensor");
    }

    #[test]
    fn sega_consoles_have_their_own_names_before_the_first_game() {
        let c = card_for("megadrive", &[], &[], None);
        assert!(c.known);
        assert_eq!([label(&c, "y"), label(&c, "b"), label(&c, "a"), label(&c, "x")], [Some("A".into()), Some("B".into()), Some("C".into()), Some("Y".into())]);
        assert_eq!([label(&c, "l"), label(&c, "r"), label(&c, "select")], [Some("X".into()), Some("Z".into()), Some("Mode".into())]);
        let sms = card_for("mastersystem", &[], &[], None);
        assert_eq!([label(&sms, "b"), label(&sms, "a"), label(&sms, "start"), label(&sms, "select")], [Some("1".into()), Some("2".into()), Some("Pause".into()), Some("Reset".into())]);
        let gg = card_for("gamegear", &[], &[], None);
        assert_eq!([label(&gg, "b"), label(&gg, "a"), label(&gg, "start")], [Some("1".into()), Some("2".into()), Some("Start".into())]);
        assert_eq!([label(&gg, "x"), label(&gg, "y"), label(&gg, "l")], [None, None, None], "two buttons, nothing else");
    }

    #[test]
    fn the_picture_grows_with_the_screen() {
        // The 1920 x 1080 SNES card sat small at the top with empty room under it: whatever the room, the picture fills
        // most of its height or, on a very wide room, of its half's width.
        for system in ["snes", "gb", "gba", "psx", "psp", "n64", "megadrive"] {
            let c = card_for(system, &[], &[], None);
            for (w, h) in [(560, 300), (1100, 600), (1600, 700), (1650, 760), (1300, 980), (2400, 1300), (900, 1300)] {
                let g = geometry(&c, (0, 0, w, h));
                let across = (2 * g.d + 2 * g.r) as f32 / (w as f32 / 2.0);
                assert!(g.share >= 0.8 || across >= 0.85, "{system} {w}x{h}: picture {:.2} of the height, {across:.2} of the half", g.share);
                assert!(g.share <= 1.0, "{system} {w}x{h}: taller than its room");
            }
        }
    }

    #[test]
    fn a_console_nobody_knows_gets_the_short_card() {
        let c = card_for("atari7800", &[], &[], None);
        assert!(!c.known, "no table and no snapshot: no guessing");
        // After its first game the core's plain names are there; its other words never are.
        let core = vec![Target { id: 8, label: "B".into() }];
        assert!(card_for("atari7800", &[], &core, None).known);
        let core = vec![Target { id: 8, label: "Fire 2".into() }];
        assert!(!card_for("atari7800", &[], &core, None).known, "no plain name: still the short card");
        // The Saturn has its own pad in hostsys.
        assert!(card_for("saturn", &[], &[], None).known);
    }

    #[test]
    fn shortcuts_follow_the_switches_and_the_host() {
        let c = card_for("snes", &[], &[], Some(Shortcuts { save: true, load: true, zoom: false }));
        assert_eq!(c.shortcuts, vec![("HOME + R1", "Save"), ("HOME + L1", "Load")]);
        let off = Shortcuts { save: false, load: true, zoom: false };
        assert_eq!(card_for("snes", &[], &[], Some(off)).shortcuts, vec![("HOME + L1", "Load")]);
        assert_eq!(card_for("snes", &[], &[], None).shortcuts, vec![], "another emulator: only the menu (Card::menu_key) is ours");
    }

    #[test]
    fn zoom_is_listed_only_once_its_shortcut_is_switched_on() {
        let values = |zoom: Option<&str>| -> std::collections::BTreeMap<String, String> {
            zoom.map(|z| ("hk_zoom".to_string(), z.to_string())).into_iter().collect()
        };
        assert!(!Shortcuts::from_values(&values(None)).zoom, "off by default, unlike Save and Load");
        assert!(!Shortcuts::from_values(&values(Some("off"))).zoom);
        let on = Shortcuts::from_values(&values(Some("on")));
        assert_eq!(card_for("snes", &[], &[], Some(on)).shortcuts, vec![("HOME + R1", "Save"), ("HOME + L1", "Load"), ("HOME + X", "Zoom")]);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn folders_of_one_console_share_its_buttons() {
        // ROCKNIX's hack and translation folders play with the console's own core.
        for (alias, system) in [("snesh", "snes"), ("sfc", "snes"), ("gbah", "gba"), ("gbch", "gbc"), ("nesh", "nes"), ("famicom", "nes")] {
            assert_eq!(card_for(alias, &[], &[], None).slots, card_for(system, &[], &[], None).slots, "{alias}");
            assert!(has_names(alias), "{alias}");
        }
        // The Disk System has buttons the NES table lacks: it waits for the core's own names.
        assert!(!has_names("fds"));
    }

    #[test]
    fn the_first_start_card_comes_once_for_each_console_and_never_to_a_child() {
        let dir = crate::testutil::TestDir::new("controls-card-seen");
        assert!(first_start_due(&dir, "snes", false, false), "the first SNES game");
        assert!(!first_start_due(&dir, "snes", false, false), "never again for the SNES");
        assert!(first_start_due(&dir, "gba", false, false), "the GBA has its own first time");
        assert!(!first_start_due(&dir, "gba", false, false));
        assert!(!first_start_due(&dir, "psx", true, false), "not to a child");
        assert!(first_start_due(&dir, "psx", false, false), "and a child's start used no mark: the PlayStation is still new");
        // A prefix of a console's name is not that console.
        assert!(first_start_due(&dir, "ps", false, false));
    }

    #[test]
    fn a_full_card_never_loses_earlier_consoles_or_repeatedly_interrupts_launch() {
        let dir = crate::testutil::TestDir::new("controls-card-full");
        assert!(first_start_due(&dir, "snes", false, false));
        crate::fsutil::fail_next_write();
        assert!(!first_start_due(&dir, "gba", false, false), "an unremembered card must not delay a launch");
        assert!(!first_start_due(&dir, "snes", false, false), "a failed write must preserve the earlier mark");
        assert!(first_start_due(&dir, "gba", false, false), "a later healthy write can remember the new console");
    }

    #[test]
    fn resuming_an_existing_game_skips_and_consumes_the_first_start_card() {
        let dir = crate::testutil::TestDir::new("controls-card-resume");
        assert!(!first_start_due(&dir, "snes", false, true), "Continue after an update must start immediately");
        assert!(!first_start_due(&dir, "snes", false, false), "another game of this console must not interrupt later");
    }

    #[test]
    fn hack_and_translation_folders_do_not_repeat_a_consoles_first_card() {
        let dir = crate::testutil::TestDir::new("controls-card-alias");
        assert!(first_start_due(&dir, "snesh", false, false));
        assert!(!first_start_due(&dir, "snes", false, false));
        assert!(!first_start_due(&dir, "sfc", false, false));
        assert!(first_start_due(&dir, "gb", false, false));
        assert!(first_start_due(&dir, "gbc", false, false), "Game Boy Color is a separate console");
        assert!(!first_start_due(&dir, "gbch", false, false));
    }

    #[test]
    fn independent_n64_controls_do_not_claim_r2_selects_c_mode() {
        let core = vec![Target { id: 13, label: "R Shoulder".into() }, Target { id: 8, label: "C-Down".into() }];
        let card = card_for("n64", &[], &core, None);
        assert_eq!(label(&card, "r2").as_deref(), Some("R"));
        assert_eq!(label(&card, "a").as_deref(), Some("C down"));
        assert!(card.note.is_none(), "R2 is a shoulder now, not a mode switch");
    }

    #[test]
    fn every_table_name_is_a_button_the_menu_can_offer() {
        for p in PADS {
            let mut ids: Vec<u8> = p.buttons.iter().map(|(i, _)| *i).collect();
            ids.sort();
            ids.dedup();
            assert_eq!(ids.len(), p.buttons.len(), "{:?}: one name per button", p.systems);
            assert!(ids.iter().all(|i| controls::MENU_ORDER.contains(i) && !(4..=7).contains(i)), "{:?}", p.systems);
        }
    }

    #[cfg(feature = "brick")]
    #[test]
    fn the_brick_card_says_what_f1_and_f2_do_and_leaves_out_one_that_does_nothing() {
        use crate::brick_hw::{FKey, FKeys};
        let c = with_brick_keys(card_for("snes", &[], &[], None), FKeys { f1: FKey::Save, f2: FKey::Load });
        assert_eq!(c.shortcuts, vec![("F1", "Quick save"), ("F2", "Quick load")]);
        let c = with_brick_keys(card_for("snes", &[], &[], None), FKeys { f1: FKey::Nothing, f2: FKey::Screenshot });
        assert_eq!(c.shortcuts, vec![("F2", "Screenshot")]);
    }
}
