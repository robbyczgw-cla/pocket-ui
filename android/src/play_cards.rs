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

/// These bundled cores have no release qualification on Android yet. Nintendo DS left the list on 12.11.2026
/// (three games, 180 s each, zero underruns, Continue with sound: ANDROID-BIG-SYSTEMS.md).
pub fn experimental(system: &str) -> bool {
    matches!(system, "n64" | "psp" | "saturn" | "dreamcast")
}
/// A system's name on a case: "Experimental" after it while our own player starts it (`Ui::experimental`; the app a
/// game plays with in Settings → Plays with is not what that word is about).
pub fn system_label(experimental: bool, name: &str) -> String {
    if experimental {
        format!("{} · {}", name, pocket_ui::i18n::tr("Experimental"))
    } else {
        name.into()
    }
}

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
        ("saturn"|"dreamcast",0) => "A",
        ("saturn"|"dreamcast",8) => "B",
        ("saturn"|"dreamcast",1) => "X",
        ("saturn"|"dreamcast",9) => "Y",
        ("saturn"|"dreamcast",12) => "L",
        ("saturn"|"dreamcast",13) => "R",
        ("saturn",10) => "C",
        ("saturn",11) => "Z",
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
/// The actual painted face buttons: label, RetroPad bit, horizontal/vertical spacing, radius multiplier.
/// The controls card derives its touch list from this same table.
pub fn touch_faces(system: &str) -> &'static [(&'static str, u16, f32, f32, f32)] {
    match system {
        "megadrive" => &[("X",10,-1.75,-0.7,0.82),("Y",9,0.0,-0.95,0.82),("Z",11,1.75,-1.2,0.82),("A",1,-1.75,1.0,0.9),("B",0,0.0,0.75,0.9),("C",8,1.75,0.5,0.9)],
        "saturn" => &[("X",1,-1.75,-0.7,0.82),("Y",9,0.0,-0.95,0.82),("Z",11,1.75,-1.2,0.82),("A",0,-1.75,1.0,0.9),("B",8,0.0,0.75,0.9),("C",10,1.75,0.5,0.9)],
        "dreamcast" => &[("X",1,-1.45,0.0,1.0),("Y",9,0.0,-1.45,1.0),("A",0,0.0,1.45,1.0),("B",8,1.45,0.0,1.0)],
        "n64" => &[("B",1,-1.2,0.9,1.0),("A",0,0.35,1.45,1.0),("◀",8,0.0,-0.9,0.62),("▲",9,1.0,-1.85,0.62),("▶",13,2.0,-0.9,0.62),("▼",14,1.0,0.05,0.62)],
        "snes"|"psx"|"nds"|"psp" => &[("Y",1,-1.45,0.0,1.0),("X",9,0.0,-1.45,1.0),("B",0,0.0,1.45,1.0),("A",8,1.45,0.0,1.0)],
        _ => &[("B",0,-0.95,0.55,1.0),("A",8,0.95,-0.55,1.0)],
    }
}
pub fn touch_shoulders(system: &str) -> &'static [(&'static str,u16,bool)] {
    match system {
        "saturn"|"dreamcast" => &[("L",12,false),("R",13,true)],
        "gba"|"snes"|"psx"|"nds"|"psp" => &[("L",10,false),("R",11,true)],
        "n64" => &[("L",10,false),("Z",12,false),("R",11,true)],
        _ => &[],
    }
}
/// Touch arrows supply an analog stick for consoles whose games need one. Other buttons keep their core bits.
pub fn touch_analog(system: &str, touch: u16) -> Option<([i16; 4], u16)> {
    if !matches!(system,"n64"|"dreamcast") { return None; }
    let axis = |negative: u32, positive: u32| {
        if touch & (1u16 << negative) != 0 { -32767 }
        else if touch & (1u16 << positive) != 0 { 32767 } else { 0 }
    };
    if system == "dreamcast" { return Some(([axis(6,7),axis(4,5),0,0],0)); }
    Some(([axis(6,7),axis(4,5),axis(8,13),axis(9,14)], (1<<4)|(1<<5)|(1<<6)|(1<<7)|(1<<8)|(1<<9)|(1<<13)|(1<<14)))
}
/// Only controls actually painted by `touch_controls`; N64 C arrows use the right stick, separately.
pub fn touch_bits(system: &str) -> Vec<u8> {
    let mut ids: Vec<_> = touch_faces(system).iter().map(|b| b.1 as u8)
        .filter(|id| system != "n64" || ![8,9,13,14].contains(id)).collect();
    ids.extend(touch_shoulders(system).iter().map(|b| b.1 as u8));
    if !matches!(system,"n64"|"saturn"|"dreamcast") { ids.push(2); }
    ids.push(3);
    ids
}
/// What a button can be set to in Options > Controls: the core's names, else the console table's (extras such as
/// turbo included; the card leaves those out unless they are used).
pub fn targets(system: &str, names: &[Target]) -> Vec<Target> {
    controls_card::console_targets(system, names)
}
/// The card of `system` as Android shows it: the console's names in the places of the gamepad's buttons (Android
/// does not know what a maker printed on them), Back as the menu key, and a line for the touch buttons without a pad.
pub fn card(system: &str, game: &str, names: &[Target], touch: bool) -> controls_card::Card {
    let mapping = Mapping::defaults(system);
    let name = pocket_ui::document_library::system(system).map_or(system, |s| s.1);
    controls_card::build(&controls_card::Input {
        system,
        system_name: name,
        game,
        mapping: &mapping,
        core_names: names,
        shortcuts: None,
        layout: None,
        menu_key: if touch { "Pause" } else { "Back" },
        touch,
    })
}
/// The card as sentences, for TalkBack.
pub fn control_lines(system: &str, game: &str, names: &[Target], touch: bool) -> Vec<String> {
    let mut lines = card(system, game, names, touch).lines();
    if experimental(system) {
        lines.insert(1, pocket_ui::i18n::tr("Experimental: compatibility, sound and saves are still being tested."));
    }
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

/// The controls card: the console's name, Back beside it as a button like the sheets' (48 dp, as wide as its word),
/// the shared picture (`controls_card::paint`) in all the room under them. It fits one screen in every Retroid
/// format, so nothing scrolls (the second value stays 0 for the callers' scroll state).
pub fn paint_controls(c: &mut Canvas, text: &mut Text, system: &str, game: &str, names: &[Target], touch: bool, _scroll: usize) -> (Area, usize) {
    let margin = 36;
    let card = card(system, game, names, touch);
    let bh = 106; // 48 dp at the shared mobile canvas's 2.2 logical pixels per dp.
    let label = text.translated("Back").to_string();
    let bw = text.width(Face::Title, 36, &label) + 96;
    let back = Area::new(c.w - margin - bw, 24, bw, bh);
    // A long console name ("Nintendo Entertainment System") gets smaller before it is cut.
    let room = back.x - margin - 24;
    let mut size = 46;
    while size > 30 && text.width(Face::Title, size, &card.system_name) > room {
        size -= 2;
    }
    let title = text.fit(Face::Title, size, &card.system_name, room);
    text.draw(c, Face::Title, size, margin, 82, pal().text, &title);
    let sub = text.fit(Face::Body, 28, "Controls", back.x - margin - 24);
    text.draw(c, Face::Body, 28, margin, 82 + type_scale::line(28, 18), pal().dust, &sub);
    if pocket_ui::retro::on() { pocket_ui::retro::frame(c, back.x, back.y, back.w, back.h, pal().pill); }
    else { c.round_rect(back.x, back.y, back.w, back.h, back.h / 2, pal().pill); }
    text.draw(c, Face::Title, 36, back.x + 48, back.y + back.h / 2 + type_scale::mid(36), pal().text, &label);
    let room = controls_area(c);
    let colors = controls_card::Colors { bg: pal().void, line: pal().dust, text: pal().text, dim: pal().dust, accent: pal().amber };
    controls_card::paint(c, text, &card, (room.x, room.y, room.w, room.h), colors);
    (back, 0)
}
/// The card's room under the heading (`paint_controls`), for the screen reader's label and the layout tests.
pub fn controls_area(c: &Canvas) -> Area {
    let top = (82 + type_scale::line(28, 18) + 36).max(24 + 106 + 24);
    Area::new(36, top, c.w - 72, c.h - 28 - top)
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
