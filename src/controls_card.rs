//! The controls card: one picture per console of which button of the handheld is which button of the console, and
//! what HOME does on top (docs/user/controls-card.md).
//!
//! Nothing on it is made up. The Pocket side is `controls::Mapping`, the player's own remaps (`ctl.*` in the console's
//! and the game's settings) included; the console side is the name the core gives its buttons (the snapshot
//! pocket-core writes, `controls::Target`), or, before a game of that console ever ran, the table below: the names
//! read from the cores' own `retro_input_descriptor` lists (Snes9x, mGBA, Gambatte, Nestopia, PCSX ReARMed,
//! Mupen64Plus-Next, in the sources the release ships). A console with neither (Mega Drive before its first game)
//! says so instead of guessing.
//!
//! Model and painter live here so the shelf (the game card's Controls row, the first start of a console) and the
//! pause menu (Options > Controls) draw the same card; the colours differ (`Colors`).

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
    /// The console's own names are known (the core's or the table's). False: the card says they show after the first
    /// game and leaves the buttons out.
    pub known: bool,
    /// By Mini V2 button id (`controls::KEYS`): what each presses on the console.
    pub slots: [Slot; 16],
    /// The D-pad as a whole, and the sticks.
    pub dpad: Slot,
    pub left_stick: Slot,
    pub right_stick: Slot,
    /// (buttons, what it does): HOME and the shortcuts of pocket-core's own host that are switched on.
    pub shortcuts: Vec<(&'static str, &'static str)>,
    /// A line under the picture for the odd one out of a console (the N64's C buttons).
    pub note: Option<&'static str>,
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
}

/// What the card is built from.
pub struct Input<'a> {
    pub system: &'a str,
    pub system_name: &'a str,
    /// The player's layout for this game (`Mapping::from_values` of the game's settings).
    pub mapping: &'a Mapping,
    /// The core's own button names, empty before a game of this console ran.
    pub core_names: &'a [Target],
    /// The shortcuts that are on, for a game on pocket-core's own host; None for any other emulator (HOME opens the
    /// menu there, the rest is the emulator's).
    pub shortcuts: Option<Shortcuts>,
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
}

// ids: 0 B, 1 Y, 2 Select, 3 Start, 8 A, 9 X, 10 L1, 11 R1, 12 L2, 13 R2, 14 L3, 15 R3 (`controls::KEYS`).
const PADS: &[Pad] = &[
    // Gambatte, Nestopia: A and B, Turbo on X and Y.
    Pad {
        systems: &["gb", "gbc", "nes", "famicom"],
        cores: &["gambatte", "nestopia"],
        buttons: &[(8, "A"), (0, "B"), (9, "Turbo A"), (1, "Turbo B"), (2, "Select"), (3, "Start")],
        left_stick: None,
        right_stick: None,
        note: None,
    },
    // mGBA.
    Pad {
        systems: &["gba"],
        cores: &["mgba"],
        buttons: &[(8, "A"), (0, "B"), (10, "L"), (11, "R"), (9, "Turbo A"), (1, "Turbo B"), (13, "Turbo R"), (12, "Turbo L"), (2, "Select"), (3, "Start"), (15, "Brighten Solar Sensor"), (14, "Darken Solar Sensor")],
        left_stick: None,
        right_stick: None,
        note: None,
    },
    // Snes9x.
    Pad {
        systems: &["snes", "sfc"],
        cores: &["snes9x"],
        buttons: &[(8, "A"), (0, "B"), (9, "X"), (1, "Y"), (10, "L"), (11, "R"), (2, "Select"), (3, "Start")],
        left_stick: None,
        right_stick: None,
        note: None,
    },
    // PCSX ReARMed: a DualShock.
    Pad {
        systems: &["psx"],
        cores: &["pcsx_rearmed"],
        buttons: &[(0, "Cross"), (8, "Circle"), (1, "Square"), (9, "Triangle"), (10, "L1"), (11, "R1"), (12, "L2"), (13, "R2"), (14, "L3"), (15, "R3"), (2, "Select"), (3, "Start")],
        left_stick: Some("Left stick"),
        right_stick: Some("Right stick"),
        note: None,
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
    PADS.iter().find(|p| p.systems.contains(&system) || core.is_some_and(|c| p.cores.contains(&c)))
}

/// Whether the card can name the console's buttons before a game of it ran.
pub fn has_names(system: &str) -> bool {
    pad(system).is_some() || !crate::platform::console_buttons(system).is_empty()
}

/// The name of console button `id`, or None when the console has no such button.
fn console_name(system: &str, core_names: &[Target], id: u8) -> Option<String> {
    if (4..=7).contains(&id) {
        return Some(controls::standard_name(id).to_string());
    }
    if !core_names.is_empty() {
        return core_names.iter().find(|t| t.id == id).map(|t| t.label.clone());
    }
    if let Some(p) = pad(system) {
        return p.buttons.iter().find(|(i, _)| *i == id).map(|(_, n)| n.to_string());
    }
    controls::targets_for(system, &[]).into_iter().find(|t| t.id == id).map(|t| t.label)
}

pub fn build(i: &Input) -> Card {
    let defaults = Mapping::defaults(i.system);
    let known = !i.core_names.is_empty() || has_names(i.system);
    let table = pad(i.system);
    let slots: [Slot; 16] = std::array::from_fn(|id| {
        let target = i.mapping.target[id];
        Slot {
            label: target.and_then(|t| console_name(i.system, i.core_names, t)),
            changed: target != defaults.target[id],
        }
    });
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
        (None, true) => Slot::named("Right stick"),
        (None, false) => Slot::default(),
    };
    let mut shortcuts = vec![("HOME", "Menu")];
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
    Card { system_name: i.system_name.to_string(), known, slots, dpad, left_stick, right_stick, shortcuts, note }
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

/// Paints the card into `area` = (x, y, w, h). Returns nothing; what does not fit is made smaller or cut with "…".
pub fn paint(c: &mut Canvas, text: &mut Text, card: &Card, area: (i32, i32, i32, i32), col: Colors) {
    let (x0, y0, w, h) = area;
    if !card.known {
        // No names yet: say so, and still give the shortcuts below.
        let msg = "This console's button names show here after your first game on it.";
        let lines = text.wrap(Face::Body, LABEL, msg, w - 40);
        let step = crate::type_scale::line(LABEL, 8);
        let mut y = y0 + step;
        for l in &lines {
            text.draw(c, Face::Body, LABEL, x0 + 20, y, col.dim, l);
            y += step;
        }
        paint_shortcuts(c, text, card, (x0, y + 10, w, y0 + h - y - 10), col);
        return;
    }
    // The shortcuts take the lower part: a line each, at most four; the sketch gets the rest.
    let step = crate::type_scale::line(SMALL, 8);
    let sc_h = (shortcut_lines(text, card, w) + card.note.is_some() as i32 + card.has_changes() as i32) * step + 16;
    let dh = h - sc_h;
    paint_pad(c, text, card, (x0, y0, w, dh), col);
    paint_shortcuts(c, text, card, (x0, y0 + dh, w, sc_h), col);
}

/// A button outline: a round one, or a pill when `w` is given.
fn outline_disc(c: &mut Canvas, cx: i32, cy: i32, r: i32, col: Colors, on: bool) {
    // What the player changed has a heavier outline in the accent colour.
    let t = if on { 4 } else { 2 };
    c.disc(cx, cy, r, if on { col.accent } else { col.line });
    c.disc(cx, cy, r - t, col.bg);
}

fn outline_pill(c: &mut Canvas, x: i32, y: i32, w: i32, h: i32, col: Colors, on: bool) {
    let t = if on { 4 } else { 2 };
    c.round_rect(x, y, w, h, h / 2, if on { col.accent } else { col.line });
    c.round_rect(x + t, y + t, w - 2 * t, h - 2 * t, (h - 2 * t) / 2, col.bg);
}

fn centered(c: &mut Canvas, text: &mut Text, size: u32, cx: i32, base: i32, color: u32, s: &str) {
    let w = text.width(Face::Body, size, s);
    text.draw(c, Face::Body, size, cx - w / 2, base, color, s);
}

/// What a slot says: its name, or a dash when the button does nothing, in the colour that tells which it is.
fn slot_text(slot: &Slot, col: Colors) -> (String, u32) {
    match &slot.label {
        Some(l) => (l.clone(), if slot.changed { col.accent } else { col.text }),
        None => ("—".to_string(), col.dim),
    }
}

fn paint_pad(c: &mut Canvas, text: &mut Text, card: &Card, area: (i32, i32, i32, i32), col: Colors) {
    let (x0, y0, w, h) = area;
    let small = SMALL;
    let size = LABEL;
    let mid_s = crate::type_scale::mid(small);
    let mid_l = crate::type_scale::mid(size);
    let pill_h = crate::type_scale::px(small) as i32 + 20;
    // The picture's scale follows the room: a round button's radius, the face buttons' distance from their middle.
    let r = ((h - pill_h - 34) * 10 / 78).clamp(12, 58);
    let d = r * 19 / 10;
    let slot = |id: usize| &card.slots[id];

    // --- shoulders: L2 L1 at the left, R1 R2 at the right; each a pill as wide as what it says, the Pocket's name and
    // the console's right beside it (a wide pill with the name far off at its end read as an empty form field).
    let gap = 10;
    // At most a quarter of the row each, so "Turbo L" stays whole with Large text at 1024x768.
    let pill_w = ((w - 3 * gap) / 4).max(110);
    let lw = [shoulder_w(text, "L2", slot(12), pill_w), shoulder_w(text, "L1", slot(10), pill_w)];
    let rw = [shoulder_w(text, "R1", slot(11), pill_w), shoulder_w(text, "R2", slot(13), pill_w)];
    let shoulders = [
        (12usize, "L2", x0, lw[0]),
        (10, "L1", x0 + lw[0] + gap, lw[1]),
        (11, "R1", x0 + w - rw[1] - gap - rw[0], rw[0]),
        (13, "R2", x0 + w - rw[1], rw[1]),
    ];
    for (id, name, px, pw) in shoulders {
        shoulder_pill(c, text, px, y0, pw, pill_h, name, slot(id), col);
    }

    // --- the middle block, centred in what is left: D-pad and left stick at the left, the face buttons and the
    // right stick at the right, SELECT and START between the sticks.
    let used = (r + d) * 2 + 22 + 2 * (r + 2);
    let room = h - pill_h - 14;
    let cy = y0 + pill_h + 14 + (room - used).max(0) / 2 + d + r;
    let sy = cy + d + r + 22 + r + 2;
    let lx = x0 + w * 26 / 100;
    let rx = x0 + w * 74 / 100;
    let side = (w * 26 / 100 - r - 14).max(60);

    // D-pad: a plus of two bars; its label at the left.
    let arm = r;
    let t = r * 2 / 3 + 2;
    for inner in [false, true] {
        let inset = if inner { 2 } else { 0 };
        let colr = if inner { col.bg } else if card.dpad.changed { col.accent } else { col.line };
        c.round_rect(lx - arm + inset, cy - t / 2 + inset, 2 * arm - 2 * inset, t - 2 * inset, 3, colr);
        c.round_rect(lx - t / 2 + inset, cy - arm + inset, t - 2 * inset, 2 * arm - 2 * inset, 3, colr);
    }
    // The D-pad's label and the left stick's below it share one size (the stick often says the same word, "D-pad").
    let (stick_txt, stick_colour) = slot_text(&card.left_stick, col);
    let press = card.left_stick.label.as_ref().and(card.slots[14].label.as_deref());
    let (txt, colour) = slot_text(&card.dpad, col);
    let dpad_size = if text.width(Face::Body, size, &txt) <= side { size } else { small };
    let label_size = dpad_size.min(stick_size(text, &stick_txt, press, side));
    let txt = text.fit(Face::Body, label_size, &txt, side);
    text.draw_right(c, Face::Body, label_size, lx - arm - 14, cy + crate::type_scale::mid(label_size), colour, &txt);

    // The left stick under it; pressing it in is a button of its own on some consoles (L3).
    outline_disc(c, lx, sy, r + 2, col, card.left_stick.changed);
    centered(c, text, small, lx, sy + mid_s, col.dim, "L");
    stick_label(c, text, (lx - r - 16, sy + mid_l, true), &stick_txt, press, stick_colour, col.dim, label_size, side);

    // Face buttons: Pocket X on top, Y at the left, A at the right, B at the bottom. Labels at the right of each
    // circle, except Y's, which stands at its left.
    let faces = [(9usize, "X", rx, cy - d), (1, "Y", rx - d, cy), (8, "A", rx + d, cy), (0, "B", rx, cy + d)];
    for (id, name, fx, fy) in faces {
        let s = slot(id);
        outline_disc(c, fx, fy, r, col, s.changed);
        centered(c, text, small, fx, fy + mid_s, col.dim, name);
        let (label, colour) = slot_text(s, col);
        if id == 1 {
            let label = text.fit(Face::Body, size, &label, (fx - r - 12 - (lx + arm + 12)).max(60));
            text.draw_right(c, Face::Body, size, fx - r - 12, fy + mid_l, colour, &label);
        } else {
            let label = text.fit(Face::Body, size, &label, (x0 + w - (fx + r + 12)).max(40));
            text.draw(c, Face::Body, size, fx + r + 12, fy + mid_l, colour, &label);
        }
    }
    // The right stick under them.
    outline_disc(c, rx, sy, r + 2, col, false);
    centered(c, text, small, rx, sy + mid_s, col.dim, "R");
    let (txt, colour) = slot_text(&card.right_stick, col);
    let room = x0 + w - (rx + r + 16);
    let press = card.right_stick.label.as_ref().and(card.slots[15].label.as_deref());
    let size = stick_size(text, &txt, press, room);
    stick_label(c, text, (rx + r + 16, sy + mid_l, false), &txt, press, colour, col.dim, size, room);

    // --- SELECT and START side by side between the sticks.
    let pw = (w * 17 / 100).max(100);
    let mid = x0 + w / 2;
    pill_label(c, text, mid - pw - 5, sy - pill_h / 2, pw, pill_h, "Select", slot(2), col);
    pill_label(c, text, mid + 5, sy - pill_h / 2, pw, pill_h, "Start", slot(3), col);
}

/// The size of a stick's label (`stick_label`): keep the stick's name and click readable on native 1024x768, including
/// Large text. Use the shoulder label size when the larger face-button type would hide the click or abbreviate the
/// control's name.
/// A click that fits at no size is left out, and the name alone decides.
fn stick_size(text: &mut Text, name: &str, press: Option<&str>, room: i32) -> u32 {
    let mut fits = |press: Option<&str>| {
        [LABEL, SMALL, 20].into_iter().find(|&size| {
            let width = text.width(Face::Body, size, name) + press.map_or(0, |p| text.width(Face::Body, size, " · ") + text.width(Face::Body, size, p));
            width <= room
        })
    };
    fits(press).or_else(|| fits(None)).unwrap_or(SMALL)
}

/// A stick's name and, after a dot, what pressing it in is ("Left stick · L3") when both fit; each part is translated on
/// its own. `at` = (x, baseline, right-aligned: x is where the text ends).
#[allow(clippy::too_many_arguments)]
fn stick_label(c: &mut Canvas, text: &mut Text, at: (i32, i32, bool), name: &str, press: Option<&str>, colour: u32, dim: u32, size: u32, room: i32) {
    let (x, base, right) = at;
    let base = base - crate::type_scale::mid(LABEL) + crate::type_scale::mid(size);
    let name = text.fit(Face::Body, size, name, room);
    let nw = text.width(Face::Body, size, &name);
    let press = press.filter(|p| name != "—" && {
        let (dot, pw) = (text.width(Face::Body, size, " · "), text.width(Face::Body, size, p));
        nw + dot + pw <= room
    });
    let total = nw + press.map_or(0, |p| text.width(Face::Body, size, " · ") + text.width(Face::Body, size, p));
    let mut pen = if right { x - total } else { x };
    pen += text.draw(c, Face::Body, size, pen, base, colour, &name);
    if let Some(p) = press {
        pen += text.draw(c, Face::Body, size, pen, base, dim, " · ");
        text.draw(c, Face::Body, size, pen, base, dim, p);
    }
}

/// A button as a pill: the Pocket's name small at its left, the console's at its right; one name in the middle when they
/// are the same.
#[allow(clippy::too_many_arguments)]
fn pill_label(c: &mut Canvas, text: &mut Text, x: i32, y: i32, w: i32, h: i32, device: &str, slot: &Slot, col: Colors) {
    let small = SMALL;
    outline_pill(c, x, y, w, h, col, slot.changed);
    let b = y + h / 2 + crate::type_scale::mid(small);
    let (label, colour) = slot_text(slot, col);
    let label = text.fit(Face::Body, small, &label, w - 24);
    if label == device {
        centered(c, text, small, x + w / 2, b, colour, &label);
        return;
    }
    let dw = text.width(Face::Body, small, device);
    text.draw(c, Face::Body, small, x + 12, b, col.dim, device);
    let label = text.fit(Face::Body, small, &label, w - 36 - dw);
    text.draw_right(c, Face::Body, small, x + w - 12, b, colour, &label);
}

/// A shoulder's pill width: the Pocket's name, a gap, the console's (or the dash), at most `max`.
fn shoulder_w(text: &mut Text, device: &str, slot: &Slot, max: i32) -> i32 {
    let label = slot.label.clone().unwrap_or_else(|| "—".to_string());
    let label_w = text.width(Face::Body, SMALL, &label).min(max - 60 - text.width(Face::Body, SMALL, device));
    (24 + text.width(Face::Body, SMALL, device) + 12 + label_w + 24).min(max)
}

/// A shoulder button: the Pocket's name small and dim, the console's right after it; an unused one says "—" in dim.
#[allow(clippy::too_many_arguments)]
fn shoulder_pill(c: &mut Canvas, text: &mut Text, x: i32, y: i32, w: i32, h: i32, device: &str, slot: &Slot, col: Colors) {
    let small = SMALL;
    outline_pill(c, x, y, w, h, col, slot.changed);
    let b = y + h / 2 + crate::type_scale::mid(small);
    let (label, colour) = slot_text(slot, col);
    let pen = x + 24 + text.draw(c, Face::Body, small, x + 24, b, col.dim, device) + 12;
    let label = text.fit(Face::Body, small, &label, x + w - 24 - pen);
    text.draw(c, Face::Body, small, pen, b, colour, &label);
}

fn paint_shortcuts(c: &mut Canvas, text: &mut Text, card: &Card, area: (i32, i32, i32, i32), col: Colors) {
    let (x0, y0, w, _h) = area;
    let step = crate::type_scale::line(SMALL, 8);
    // A rule over them.
    c.rect(x0, y0 + 2, w, 2, col.line);
    let mut y = y0 + 12 + step - 6;
    if let Some(note) = card.note.filter(|_| card.known) {
        text.draw(c, Face::Body, SMALL, x0 + 4, y, col.dim, note);
        y += step;
    }
    if card.has_changes() && card.known {
        text.draw(c, Face::Body, SMALL, x0 + 4, y, col.accent, "In colour: what you changed in Options › Controls.");
        y += step;
    }
    for (keys, what, line) in shortcut_layout(text, card, w) {
        let (kw, x) = (text.width(Face::Body, SMALL, keys), x0 + 4 + line.0);
        text.draw(c, Face::Body, SMALL, x, y + line.1 * step, col.dim, keys);
        text.draw(c, Face::Body, SMALL, x + kw + 10, y + line.1 * step, col.text, what);
    }
}

/// Where each shortcut goes: (keys, what, (x offset, line)). They stand side by side while they fit, else on the next line.
fn shortcut_layout(text: &mut Text, card: &Card, w: i32) -> Vec<(&'static str, &'static str, (i32, i32))> {
    let (mut x, mut line, mut out) = (0, 0, Vec::new());
    for (keys, what) in &card.shortcuts {
        let (kw, ww) = (text.width(Face::Body, SMALL, keys), text.width(Face::Body, SMALL, what));
        if x > 0 && x + kw + ww + 40 > w - 8 {
            line += 1;
            x = 0;
        }
        out.push((*keys, *what, (x, line)));
        x += kw + ww + 10 + 36;
    }
    out
}

fn shortcut_lines(text: &mut Text, card: &Card, w: i32) -> i32 {
    shortcut_layout(text, card, w).last().map_or(0, |l| l.2 .1 + 1)
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
        build(&Input { system, system_name: system, mapping: &mapping, core_names: core, shortcuts })
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
    fn the_cores_own_names_win_over_the_table() {
        let core = vec![Target { id: 8, label: "Fire".into() }, Target { id: 0, label: "Jump".into() }];
        let c = card_for("snes", &[], &core, None);
        assert_eq!([label(&c, "a"), label(&c, "b"), label(&c, "x")], [Some("Fire".into()), Some("Jump".into()), None]);
    }

    #[test]
    fn a_console_nobody_knows_says_so() {
        let c = card_for("megadrive", &[], &[], None);
        assert!(!c.known, "no table and no snapshot: no guessing");
        // After its first game the core's names are there.
        let core = vec![Target { id: 8, label: "C".into() }];
        assert!(card_for("megadrive", &[], &core, None).known);
        // The Saturn has its own pad in hostsys.
        assert!(card_for("saturn", &[], &[], None).known);
    }

    #[test]
    fn shortcuts_follow_the_switches_and_the_host() {
        let c = card_for("snes", &[], &[], Some(Shortcuts { save: true, load: true, zoom: false }));
        assert_eq!(c.shortcuts, vec![("HOME", "Menu"), ("HOME + R1", "Save"), ("HOME + L1", "Load")]);
        let off = Shortcuts { save: false, load: true, zoom: false };
        assert_eq!(card_for("snes", &[], &[], Some(off)).shortcuts, vec![("HOME", "Menu"), ("HOME + L1", "Load")]);
        assert_eq!(card_for("snes", &[], &[], None).shortcuts, vec![("HOME", "Menu")], "another emulator: only the menu is ours");
    }

    #[test]
    fn zoom_is_listed_only_once_its_shortcut_is_switched_on() {
        let values = |zoom: Option<&str>| -> std::collections::BTreeMap<String, String> {
            zoom.map(|z| ("hk_zoom".to_string(), z.to_string())).into_iter().collect()
        };
        assert!(!Shortcuts::from_values(&values(None)).zoom, "off by default, unlike Save and Load");
        assert!(!Shortcuts::from_values(&values(Some("off"))).zoom);
        let on = Shortcuts::from_values(&values(Some("on")));
        assert_eq!(card_for("snes", &[], &[], Some(on)).shortcuts, vec![("HOME", "Menu"), ("HOME + R1", "Save"), ("HOME + L1", "Load"), ("HOME + X", "Zoom")]);
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
        assert_eq!(label(&card, "r2").as_deref(), Some("R Shoulder"));
        assert_eq!(label(&card, "a").as_deref(), Some("C-Down"));
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
}
