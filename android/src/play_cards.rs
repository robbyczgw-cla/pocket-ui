//! Android-only presentation and input policies, also exercised by host tests.
use pocket_ui::{
    controls::{Mapping, Target},
    controls_card,
    gfx::Canvas,
    image::Image,
    palette::pal,
    text::{Face, Text},
    touch::Area,
    type_scale,
};

pub fn text_size(chosen: pocket_ui::text_size::TextSize, scale: f32) -> pocket_ui::text_size::TextSize {
    use pocket_ui::text_size::TextSize::*;
    if scale >= 1.25 || chosen == Large {
        Large
    } else if scale >= 1.12 || chosen == Medium {
        Medium
    } else {
        Normal
    }
}

/// Core descriptors belong to this game/core identity; another game's action names must not leak into its card.
pub fn remember_names(data: &std::path::Path, key: &str, names: &[Target]) -> std::io::Result<()> {
    pocket_ui::fsutil::write_json(&data.join("android-buttons").join(format!("{key}.json")), &names)
}
pub fn remembered_names(data: &std::path::Path, key: &str) -> Vec<Target> {
    pocket_ui::fsutil::read_json_or_backup(&data.join("android-buttons").join(format!("{key}.json"))).unwrap_or_default()
}

/// Status over a game needs its own contrast in both appearances, whatever that frame's colours are.
pub fn paint_badge(c: &mut Canvas, text: &mut Text, label: &str) {
    let label = text.fit(Face::Body, 28, label, c.w - 96);
    let (w, h) = (text.width(Face::Body, 28, &label) + 40, type_scale::row(64, 28));
    c.round_rect(28, 20, w, h, h / 2, pal().pill);
    text.draw(c, Face::Body, 28, 48, 20 + h / 2 + type_scale::mid(28), pal().text, &label);
}

/// Android's printed A/B/X/Y are south/east/west/north. Touch uses the console names.
pub fn pad_label(bit: usize) -> &'static str {
    [
        "A",
        "X",
        "Select",
        "Start",
        "D-pad up",
        "D-pad down",
        "D-pad left",
        "D-pad right",
        "B",
        "Y",
        "L1",
        "R1",
        "L2",
        "R2",
        "L3",
        "R3",
    ][bit]
}
pub fn touch_label(system: &str, bit: usize) -> &'static str {
    match (system, bit) {
        ("n64", 0) => "A",
        ("n64", 1) => "B",
        ("n64", 8) => "C<",
        ("n64", 9) => "C^",
        ("n64", 13) => "C>",
        ("n64", 14) => "Cv",
        ("n64", 12) => "Z",
        ("psp", 0) => "Cross",
        ("psp", 8) => "Circle",
        ("psp", 1) => "Square",
        ("psp", 9) => "Triangle",
        ("megadrive", 1) => "A",
        ("megadrive", 0) => "B",
        ("megadrive", 8) => "C",
        ("megadrive", 10) => "X",
        ("megadrive", 9) => "Y",
        ("megadrive", 11) => "Z",
        _ => [
            "B", "Y", "Select", "Start", "Up", "Down", "Left", "Right", "A", "X", "L", "R", "L2", "R2", "L3", "R3",
        ][bit],
    }
}
/// Only controls actually painted by `touch_controls`; N64 C arrows use the right stick, separately.
pub fn touch_bits(system: &str) -> Vec<u8> {
    let mut ids = match system {
        "megadrive" => vec![10, 9, 11, 1, 0, 8],
        "n64" => vec![1, 0, 10, 12, 11],
        "snes" | "psx" | "nds" | "psp" => vec![1, 9, 0, 8, 10, 11],
        "gba" => vec![0, 8, 10, 11],
        _ => vec![0, 8],
    };
    ids.extend([2, 3]);
    ids
}
/// Honest names from the core, else the shared console card's verified table.
pub fn targets(system: &str, names: &[Target]) -> Vec<Target> {
    let mapping = Mapping::defaults(system);
    let card = controls_card::build(&controls_card::Input {
        system,
        system_name: system,
        mapping: &mapping,
        core_names: names,
        shortcuts: None,
        layout: pocket_ui::button_layout_model::ButtonLayout::Xbox,
        deck: false,
    });
    if !card.known {
        return Vec::new();
    }
    card.slots
        .iter()
        .enumerate()
        .filter_map(|(id, slot)| {
            slot.label.as_ref().map(|label| Target {
                id: id as u8,
                label: label.clone(),
            })
        })
        .collect()
}
pub fn control_lines(system: &str, names: &[Target]) -> Vec<String> {
    let tr = pocket_ui::i18n::tr;
    let mut lines = vec![tr("Android gamepad / Touch controls: Console")];
    let targets = targets(system, names);
    if targets.is_empty() {
        lines.push(tr("The console’s button names will show here after its first game."));
    }
    for t in targets {
        if (4..8).contains(&t.id) {
            continue;
        }
        let touch = touch_bits(system).contains(&t.id);
        let input = if touch {
            format!("{} / {}", tr(pad_label(t.id as usize)), tr(touch_label(system, t.id as usize)))
        } else {
            tr(pad_label(t.id as usize))
        };
        lines.push(format!("{input}: {}", tr(&t.label)));
    }
    lines.push(tr("D-pad: D-pad"));
    lines.push(tr(if system == "n64" {
        "Touch D-pad: Analog stick"
    } else if pocket_ui::controls::stick_dpad_default(system) {
        "Left stick: D-pad"
    } else {
        "Left stick: Analog stick"
    }));
    if system == "nds" {
        lines.push(tr("Touch the lower screen to use the stylus"));
    }
    if system == "n64" {
        lines.push(tr("Touch C arrows: C buttons (right stick)"));
    }
    lines.push(tr("Back / Pause: Menu"));
    lines.push(tr("Swipe or use the D-pad to read more"));
    lines
}

/// A continuous crop of a frozen frame. Drag and pad movement are bounded by the picture, even at 1x.
#[derive(Clone, Copy, Debug)]
pub struct Zoom {
    pub scale: f32,
    pub x: f32,
    pub y: f32,
}
impl Zoom {
    pub fn new(system: &str) -> Self {
        Self {
            scale: 2.0,
            x: 0.5,
            y: if pocket_ui::zoom::starts_low(system) { 1.0 } else { 0.5 },
        }
    }
    pub fn magnify(&mut self, factor: f32) {
        if factor.is_finite() {
            self.scale = (self.scale * factor).clamp(1.0, 4.0);
        }
    }
    pub fn pan(&mut self, dx: f32, dy: f32) {
        self.x = (self.x + dx).clamp(0.0, 1.0);
        self.y = (self.y + dy).clamp(0.0, 1.0);
    }
    pub fn part(&self, w: u32, h: u32) -> (u32, u32, u32, u32) {
        let pw = ((w as f32 / self.scale) as u32).clamp(1, w.max(1));
        let ph = ((h as f32 / self.scale) as u32).clamp(1, h.max(1));
        (((w - pw) as f32 * self.x) as u32, ((h - ph) as f32 * self.y) as u32, pw, ph)
    }
    pub fn paint(&self, c: &mut Canvas, image: &Image, area: Area) {
        let (x, y, w, h) = self.part(image.w, image.h);
        // Copy only the native-resolution crop; never an enlarged screenshot.
        let px = (y..y + h)
            .flat_map(|row| image.px[(row * image.w + x) as usize..(row * image.w + x + w) as usize].iter().copied())
            .collect();
        let crop = Image { w, h, px };
        let k = (area.w as f32 / w as f32).min(area.h as f32 / h as f32);
        let (dw, dh) = ((w as f32 * k) as i32, (h as f32 * k) as i32);
        c.blit_scaled(&crop, area.x + (area.w - dw) / 2, area.y + (area.h - dh) / 2, dw, dh);
    }
}

/// Scrollable prose for the controls card, with one Back target kept clear of text in either orientation.
pub fn paint_controls(c: &mut Canvas, text: &mut Text, system: &str, names: &[Target], scroll: usize) -> (Area, usize) {
    paint_lines(c, text, "Controls", &control_lines(system, names), scroll)
}
pub fn paint_lines(c: &mut Canvas, text: &mut Text, title: &str, lines: &[String], scroll: usize) -> (Area, usize) {
    let margin = 36;
    let title = text.fit(Face::Title, 46, title, c.w - margin * 2);
    text.draw(c, Face::Title, 46, margin, 82, pal().text, &title);
    let bh = 106; // 48 dp at the shared mobile canvas's 2.2 logical pixels per dp.
    let back = Area::new(margin, c.h - bh - 28, c.w - margin * 2, bh);
    let line = type_scale::line(28, 22);
    let mut wrapped = Vec::new();
    for s in lines {
        for l in text.wrap(Face::Body, 28, &s, c.w - margin * 2) {
            wrapped.push(l);
        }
    }
    let visible = ((back.y - 140) / line).max(1) as usize;
    let max = wrapped.len().saturating_sub(visible);
    for (n, l) in wrapped.iter().skip(scroll.min(max)).take(visible).enumerate() {
        text.draw(c, Face::Body, 28, margin, 140 + n as i32 * line, pal().text, l);
    }
    if pocket_ui::retro::on() { pocket_ui::retro::frame(c, back.x, back.y, back.w, back.h, pal().pill); }
    else { c.round_rect(back.x, back.y, back.w, back.h, back.h / 2, pal().pill); }
    let label = text.translated("Back").to_string();
    text.draw(c, Face::Body, 36, back.x + 30, back.y + back.h / 2 + type_scale::mid(36), pal().text, &label);
    if max > 0 {
        let window = (back.y - 140).max(1);
        let thumb = (window * visible as i32 / wrapped.len().max(1) as i32).max(24);
        let y = 120 + (window - thumb) * scroll.min(max) as i32 / max as i32;
        c.round_rect(c.w - 14, y, 6, thumb, 3, pal().chip);
    }
    (back, max)
}

/// A short old PSP state is deliberately refused by the shared safety policy. Say what remains available.
pub fn recovery_note(core: &str, resume: pocket_ui::libretro::Resume) -> Option<&'static str> {
    (core == "ppsspp" && resume == pocket_ui::libretro::Resume::Failed)
        .then_some("This PSP moment could not be loaded safely. It is kept under Load; your in-game save is safe.")
}
