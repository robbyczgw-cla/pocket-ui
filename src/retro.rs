//! Settings → Look → Style → **Retro**: a skin over every page in the manner of a 90s/2000s handheld's menus
//! (docs/RETRO-STYLE.md). It is the fourth value of the Style row (`font_style::FontStyle::Retro`, settings.json
//! `"font": "retro"`); the looks (Shelf, Cabinet, Easy, Kids, …), the box art and Continue stay as they are. What it
//! changes, all from here:
//! - the colours: one of two palettes of its own (`Colours`: Cocoa, Pea), dark and light, instead of Settings → Look →
//!   Colours' (the Colours row steps through these two while Retro is on, settings.json `"retro_colours"`);
//! - the type: Pixelify Sans for titles *and* body text (`font_style`);
//! - the chrome: panels, sheets, pills and the hint bar as chunky pixel windows with notched corners and an inner line
//!   (`frame`), the selection as a flat bar with a pixel cursor (`bar`), key caps as notched blocks (`cap`);
//! - the Settings categories and the pause menu's rows get small colour icons made with PixelLab (`Icon`,
//!   `assets/ui-art/retro/`);
//! - the menu sounds: short synthesized blips (`ui_sound`).
//!
//! Painters ask `on()`: one atomic load. `POCKETUI_RETRO=cocoa|pea` turns it on for the snapshots (`from_env`).

use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::OnceLock;

use crate::gfx::{mix, Canvas};
use crate::palette::{pal, Scheme};
use crate::system_art::{decode, Sprite};

/// True while the Retro style is in use (Settings → Look → Style → Retro).
pub fn on() -> bool {
    crate::font_style::get() == crate::font_style::FontStyle::Retro
}

/// The Retro style's palettes. Cocoa is the default.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Colours {
    /// Warm brown and cream with a tangerine selection, like the shelf's walnut.
    #[default]
    Cocoa,
    /// Four greens of a monochrome LCD.
    Pea,
}

impl Colours {
    pub const ALL: [Colours; 2] = [Colours::Cocoa, Colours::Pea];

    /// The value on the Colours row while Retro is on.
    pub fn label(self) -> &'static str {
        match self {
            Colours::Cocoa => "Cocoa",
            Colours::Pea => "Pea",
        }
    }

    /// The line under the Colours row while Retro is on.
    pub fn blurb(self) -> &'static str {
        match self {
            Colours::Cocoa => "Cocoa: warm brown and cream, a tangerine selection.",
            Colours::Pea => "Pea: the four greens of an old handheld's screen.",
        }
    }

    /// The next (`forward`) or previous palette.
    pub fn step(self, forward: bool) -> Colours {
        let i = Colours::ALL.iter().position(|s| *s == self).unwrap_or(0);
        let n = Colours::ALL.len();
        Colours::ALL[if forward { (i + 1) % n } else { (i + n - 1) % n }]
    }

    fn from_u8(v: u8) -> Colours {
        Colours::ALL.get(v as usize).copied().unwrap_or_default()
    }

    pub fn scheme(self) -> &'static Scheme {
        match self {
            Colours::Cocoa => &COCOA,
            Colours::Pea => &PEA,
        }
    }
}

/// `POCKETUI_RETRO=cocoa|pea` (or `on`): the snapshots turn Retro on with that palette; `off` turns it off.
/// `Some(None)` is off, `None` no override.
pub fn from_env() -> Option<Option<Colours>> {
    let v = std::env::var("POCKETUI_RETRO").ok()?;
    match v.trim().to_ascii_lowercase().as_str() {
        "off" | "0" => Some(None),
        "1" | "on" | "cocoa" => Some(Some(Colours::Cocoa)),
        "pea" => Some(Some(Colours::Pea)),
        _ => None,
    }
}

#[cfg(not(test))]
static COLOURS: AtomicU8 = AtomicU8::new(0);
#[cfg(test)]
thread_local! {
    static COLOURS: AtomicU8 = const { AtomicU8::new(0) };
}

/// The palette Retro paints with while it is on (kept while it is off).
pub fn set_colours(c: Colours) {
    #[cfg(not(test))]
    COLOURS.store(c as u8, Ordering::Relaxed);
    #[cfg(test)]
    COLOURS.with(|v| v.store(c as u8, Ordering::Relaxed));
}

pub fn colours() -> Colours {
    #[cfg(not(test))]
    let v = COLOURS.load(Ordering::Relaxed);
    #[cfg(test)]
    let v = COLOURS.with(|v| v.load(Ordering::Relaxed));
    Colours::from_u8(v)
}

/// One pixel of the frames, in screen pixels: 4 on the Mini V2's 1240x1080 and on the small screens painted at
/// 1280x960 (2 real pixels on 640x480).
pub const U: i32 = 4;

/// A rectangle with its four corner pixels (`U`) cut off: the notched corner of pixel-art windows. Too small for a
/// notch (a thin bar, a dot under five frame pixels), it stays square.
pub fn notched(c: &mut Canvas, x: i32, y: i32, w: i32, h: i32, color: u32) {
    notched_at(c, x, y, w, h, color, U);
}

/// `notched` with frame pixels `u` screen pixels wide.
fn notched_at(c: &mut Canvas, x: i32, y: i32, w: i32, h: i32, color: u32, u: i32) {
    if w < 5 * u || h < 5 * u {
        c.rect(x, y, w, h, color);
        return;
    }
    c.rect(x + u, y, w - 2 * u, h, color);
    c.rect(x, y + u, u, h - 2 * u, color);
    c.rect(x + w - u, y + u, u, h - 2 * u, color);
}

/// The frame's line colour: the page's text, a little sunk into the panel.
pub fn line() -> u32 {
    mix(pal().chalk, pal().pill, 40)
}

/// A pixel window at (x, y), `w` x `h`, filled with `fill`: a one-pixel shadow down and right, a light outer line with
/// notched corners and, one pixel inside, a thin inner line. Faint scanlines run across its inside.
pub fn frame(c: &mut Canvas, x: i32, y: i32, w: i32, h: i32, fill: u32) {
    frame_at(c, x, y, w, h, fill, U);
}

/// `frame` with frame pixels `u` screen pixels wide: pocket-core paints its notes in the panel's own pixels, where a
/// 640x480 screen wants half of `U` to look as pocket-ui's frames do there (`overlay::paint_pill_on`).
pub fn frame_at(c: &mut Canvas, x: i32, y: i32, w: i32, h: i32, fill: u32, u: i32) {
    let shadow = mix(pal().void, 0x000000, 140);
    notched_at(c, x + u, y + u, w, h, shadow, u);
    notched_at(c, x, y, w, h, line(), u);
    notched_at(c, x + u, y + u, w - 2 * u, h - 2 * u, fill, u);
    // The inner line, half a frame pixel thick, two frame pixels in.
    let (t, i) = ((u / 2).max(1), 2 * u);
    let inner = mix(fill, line(), 90);
    c.frame(x + i, y + i, w - 2 * i, h - 2 * i, t, inner);
    // Scanlines: every second frame pixel a shade lighter (darker on paper).
    let scan = mix(fill, if pal().is_light() { 0x000000 } else { 0xffffff }, 6);
    let mut yy = y + i + t + u;
    while yy < y + h - i - t {
        c.rect(x + i + t, yy, w - 2 * (i + t), (u / 2).max(1), scan);
        yy += 2 * u;
    }
}

/// How far inside a `frame_at` with pixels `u` wide its inner line ends: text keeps its padding from there.
pub fn frame_inset(u: i32) -> i32 {
    2 * u + (u / 2).max(1)
}

/// For the text checks (`text::boxes`, the text-size test's Retro runs): the lines of a `frame` at (x, y), `w` x `h`,
/// are areas no text may run into, as a hint bar is ("‹frame›"). The pause menu's windows say so: names ran through
/// the rows' window and under the preview window while the record saw only the screen's edges (Retro clip, 06.11.2026).
pub fn frame_lines_are_no_text(c: &Canvas, x: i32, y: i32, w: i32, h: i32) {
    if !crate::text::boxes::on() {
        return;
    }
    let t = frame_inset(U);
    for r in [(x, y, w, t), (x, y + h - t, w, t), (x, y + t, t, h - 2 * t), (x + w - t, y + t, t, h - 2 * t)] {
        crate::text::boxes::area(c, r, "‹frame›");
    }
}

/// The selection: a flat bar in `pal().select` with notched corners, a lighter top and a darker bottom pixel row, and
/// a pixel cursor (a triangle pointing right) in `pal().ink` near its left end.
pub fn bar(c: &mut Canvas, x: i32, y: i32, w: i32, h: i32) {
    bar_in(c, x, y, w, h, pal().select, pal().ink);
}

/// `bar` in `color` with its cursor in `ink`.
pub fn bar_in(c: &mut Canvas, x: i32, y: i32, w: i32, h: i32, color: u32, ink: u32) {
    plain_bar(c, x, y, w, h, color);
    if h >= 7 * U && w >= 12 * U {
        cursor(c, x + 2 * U + 2, y + h / 2, ink);
    }
}

/// The bar without its cursor: for lists whose names start close to the bar's end (Split, Minimal, the sheets).
pub fn plain_bar(c: &mut Canvas, x: i32, y: i32, w: i32, h: i32, color: u32) {
    notched(c, x, y, w, h, color);
    if h < 3 * U {
        return;
    }
    c.rect(x + U, y, w - 2 * U, U, mix(color, 0xffffff, 70));
    c.rect(x + U, y + h - U, w - 2 * U, U, mix(color, 0x000000, 60));
}

/// A round button (Easy's and Kids' arrows, their A): smooth in Clean, a chunky circle of frame pixels in Retro.
pub fn disc(c: &mut Canvas, cx: i32, cy: i32, r: i32, color: u32) {
    if !on() {
        c.disc(cx, cy, r, color);
        return;
    }
    let steps = (r / U).max(1);
    for k in -steps..steps {
        // The row's middle, one frame pixel high, and the circle's half width there in whole frame pixels.
        let dy = (k * U + U / 2) as f32;
        let half = ((r * r) as f32 - dy * dy).max(0.0).sqrt();
        let half = ((half / U as f32).round() as i32) * U;
        c.rect(cx - half, cy + k * U, 2 * half, U, color);
    }
}

/// A pill of the interface (a toast, a chip, a progress bar, a tile): rounded with radius `r` in Clean, notched in Retro.
pub fn pill(c: &mut Canvas, x: i32, y: i32, w: i32, h: i32, r: i32, color: u32) {
    if on() {
        notched(c, x, y, w, h, color);
    } else {
        c.round_rect(x, y, w, h, r, color);
    }
}

/// A selected row or tile in `color`: rounded in Clean, `plain_bar` in Retro (no cursor: the callers' names start
/// close to its end).
pub fn select(c: &mut Canvas, x: i32, y: i32, w: i32, h: i32, r: i32, color: u32) {
    crate::nav_probe::focus(c, (x, y, w, h));
    if on() {
        plain_bar(c, x, y, w, h, color);
    } else {
        c.round_rect(x, y, w, h, r, color);
    }
}

/// A panel or card over the page or a game (quick settings, the switcher, the OSD, a note over the game): rounded in
/// Clean, a pixel window (`frame`) in Retro.
pub fn window(c: &mut Canvas, x: i32, y: i32, w: i32, h: i32, r: i32, fill: u32) {
    if on() {
        frame(c, x, y, w, h, fill);
    } else {
        c.round_rect(x, y, w, h, r, fill);
    }
}

/// The cursor: a triangle five frame pixels high, its left edge at `x`, its middle on `mid`.
pub fn cursor(c: &mut Canvas, x: i32, mid: i32, color: u32) {
    for k in 0..3 {
        let half = 2 - k;
        c.rect(x + k * U, mid - U / 2 - half * U, U, (2 * half + 1) * U, color);
    }
}

/// A key cap: a notched block in `bg`.
pub fn cap(c: &mut Canvas, x: i32, y: i32, w: i32, h: i32, bg: u32) {
    notched(c, x, y, w, h, bg);
    c.rect(x + U, y + h - U, w - 2 * U, U, mix(bg, 0x000000, 70));
}

/// How Retro paints digits (times, percentages, slot numbers, play time). Pixelify's own digits read badly even
/// crisp: "8" like "S", "5" like "S", "0" like "O" (Retro clip, 06.11.2026). Three ways to choose from
/// (`POCKETUI_RETRO_DIGITS=lcd|pixelify|plain` for the snapshots); `Lcd` until he does.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Digits {
    /// Pixelify's own digits, on its pixel grid.
    Pixelify,
    /// A 5x7 set in the manner of an LCD's character set (`LCD`): slashed 0, a flat-topped 5, an open 4, drawn on
    /// Pixelify's grid at its digits' widths, so nothing moves.
    Lcd,
    /// Barlow's digits inside the pixel text.
    Plain,
}

/// The digits in use (`Digits`): read once.
pub fn digits() -> Digits {
    static D: OnceLock<Digits> = OnceLock::new();
    *D.get_or_init(|| match std::env::var("POCKETUI_RETRO_DIGITS").as_deref() {
        Ok("pixelify") => Digits::Pixelify,
        Ok("plain") => Digits::Plain,
        _ => Digits::Lcd,
    })
}

/// `Digits::Lcd`: each digit's seven rows, five pixels across (bit 4 the left one); "1" uses the left three.
pub const LCD: [[u8; 7]; 10] = [
    [0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110],
    [0b01000, 0b11000, 0b01000, 0b01000, 0b01000, 0b01000, 0b11100],
    [0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b01000, 0b11111],
    [0b01110, 0b10001, 0b00001, 0b00110, 0b00001, 0b10001, 0b01110],
    [0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010],
    [0b11111, 0b10000, 0b11110, 0b00001, 0b00001, 0b10001, 0b01110],
    [0b00110, 0b01000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110],
    [0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000],
    [0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110],
    [0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00010, 0b01100],
];

/// Pixelify's capitals that read as other letters or digits whatever the digits are: "Z" as "2" ("2urück"), "B" as "8"
/// or "G" on a key cap, "C" as "O" ("SELEOT", "Oountry"), "S" as "8" (two pixels apart). Retro paints these from here,
/// in `LCD`'s manner, at Pixelify's widths.
pub fn lcd_letter(ch: char) -> Option<&'static [u8; 7]> {
    match ch {
        'S' => Some(&[0b01111, 0b10000, 0b10000, 0b01110, 0b00001, 0b00001, 0b11110]),
        'C' => Some(&[0b01110, 0b10001, 0b10000, 0b10000, 0b10000, 0b10001, 0b01110]),
        'B' => Some(&[0b11110, 0b10001, 0b10001, 0b11110, 0b10001, 0b10001, 0b11110]),
        'Z' => Some(&[0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b10000, 0b11111]),
        _ => None,
    }
}

/// The little pictures of the Retro style (PixelLab, `assets/ui-art/retro/`, 32 x 32).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    Look,
    Sound,
    Playing,
    Games,
    Connections,
    System,
    Help,
    Continue,
    Save,
    Load,
    Options,
    Quick,
    Quit,
}

const FILES: [(Icon, &[u8]); 13] = [
    (Icon::Look, include_bytes!("../assets/ui-art/retro/look.png")),
    (Icon::Sound, include_bytes!("../assets/ui-art/retro/sound.png")),
    (Icon::Playing, include_bytes!("../assets/ui-art/retro/playing.png")),
    (Icon::Games, include_bytes!("../assets/ui-art/retro/games.png")),
    (Icon::Connections, include_bytes!("../assets/ui-art/retro/connections.png")),
    (Icon::System, include_bytes!("../assets/ui-art/retro/system.png")),
    (Icon::Help, include_bytes!("../assets/ui-art/retro/help.png")),
    (Icon::Continue, include_bytes!("../assets/ui-art/retro/continue.png")),
    (Icon::Save, include_bytes!("../assets/ui-art/retro/save.png")),
    (Icon::Load, include_bytes!("../assets/ui-art/retro/load.png")),
    (Icon::Options, include_bytes!("../assets/ui-art/retro/options.png")),
    (Icon::Quick, include_bytes!("../assets/ui-art/retro/quick.png")),
    (Icon::Quit, include_bytes!("../assets/ui-art/retro/quit.png")),
];

fn sprite(icon: Icon) -> Option<&'static Sprite> {
    static DECODED: [OnceLock<Option<Sprite>>; FILES.len()] = [const { OnceLock::new() }; FILES.len()];
    let i = FILES.iter().position(|(a, _)| *a == icon)?;
    DECODED[i].get_or_init(|| decode(FILES[i].1)).as_ref()
}

/// An icon's colour as the style paints it: as drawn, or on Pea's LCD one of its four greens by its lightness.
fn shade(rgb: u32) -> u32 {
    if !on() || colours() != Colours::Pea {
        return rgb;
    }
    let (r, g, b) = ((rgb >> 16) & 0xff, (rgb >> 8) & 0xff, rgb & 0xff);
    match (r * 30 + g * 59 + b * 11) / 100 {
        0..=49 => 0x182010,
        50..=109 => 0x5d7034,
        110..=169 => 0x9cae5e,
        _ => 0xdfe8ac,
    }
}

/// The size of an icon on the screen at `k` (32 x `k`).
pub const ICON: i32 = 32;

/// `icon` at (x, y), each of its pixels `k` x `k`; half-clear pixels count as clear, so the art stays crisp.
pub fn icon(c: &mut Canvas, icon: Icon, x: i32, y: i32, k: i32) {
    let Some(s) = sprite(icon) else { return };
    for sy in 0..s.h as i32 {
        let mut sx = 0;
        while sx < s.w as i32 {
            let p = s.px[(sy * s.w as i32 + sx) as usize];
            if p >> 24 < 128 {
                sx += 1;
                continue;
            }
            let mut end = sx + 1;
            while end < s.w as i32 && s.px[(sy * s.w as i32 + end) as usize] == p {
                end += 1;
            }
            c.rect(x + sx * k, y + sy * k, (end - sx) * k, k, shade(p & 0x00ff_ffff));
            sx = end;
        }
    }
}

use crate::palette::Palette;

/// Cocoa: dark chocolate panels with cream lines, a tangerine selection; in light, cream paper with cocoa lines.
pub const COCOA: Scheme = Scheme {
    dark: Palette {
        void: 0x1b1411,
        spine: 0x2b211b,
        label: 0xf3e5c7,
        ink: 0x22160f,
        chalk: 0xf2e6cc,
        dust: 0xb9a588,
        keycap: 0x3a2d24,
        alert: 0xf06a50,
        on_alert: 0x22160f,
        label_dust: 0x473527,
        off: 0x6c5a4b,
        text: 0xfbf1de,
        soft: 0xeadcc1,
        quiet: 0xc7b398,
        pill: 0x2b211b,
        chip: 0xe9d7b3,
        band: 0x5b4535,
        select: 0xf08a3e,
        amber: 0xf2b04a,
        star: 0xffcf4d,
        split_bg: 0x231a15,
        split_dim: 0xcbb79c,
        on_wash: 0xffffff,
        t: 0,
    },
    light: Palette {
        void: 0xf6ecd6,
        spine: 0xe4d3b0,
        label: 0xfff8e8,
        ink: 0x1a0f08,
        chalk: 0x2e2118,
        dust: 0x6b5440,
        keycap: 0xdcc8a2,
        alert: 0xa8381f,
        on_alert: 0xfff8e8,
        label_dust: 0x271a11,
        off: 0xa89478,
        text: 0x2a1d14,
        soft: 0x3d2d21,
        quiet: 0x5e4a38,
        pill: 0xfaf0da,
        chip: 0x5c4532,
        band: 0xd8c39c,
        select: 0xcc6c31,
        amber: 0x8f5a0c,
        star: 0xa8700f,
        split_bg: 0xeadcbf,
        split_dim: 0x5e4a38,
        on_wash: 0x2a1d14,
        t: 255,
    },
    wood: [0, 0],
    stain: 0,
    holder: 0x8a7350,
    card: 0xe6dfcf,
    card_ink: 0x3a3026,
    lamp: 0xffd9a8,
};

/// Pea: the four greens of a monochrome LCD, darkest as the page.
pub const PEA: Scheme = Scheme {
    dark: Palette {
        void: 0x182010,
        spine: 0x2f3d1c,
        label: 0xcdd89a,
        ink: 0x182010,
        chalk: 0xcdd89a,
        dust: 0x9cae5e,
        keycap: 0x2f3d1c,
        alert: 0xe0a050,
        on_alert: 0x182010,
        label_dust: 0x333f1e,
        off: 0x5d7034,
        text: 0xdfe8ac,
        soft: 0xcdd89a,
        quiet: 0x9cae5e,
        pill: 0x222d15,
        chip: 0x9cae5e,
        band: 0x4a5a2a,
        select: 0x9cae5e,
        amber: 0xcdd89a,
        star: 0xdfe8ac,
        split_bg: 0x1d2614,
        split_dim: 0x9cae5e,
        on_wash: 0xdfe8ac,
        t: 0,
    },
    light: Palette {
        void: 0xe2eab4,
        spine: 0xb5c27e,
        label: 0xdde6ac,
        ink: 0x0c1008,
        chalk: 0x182010,
        dust: 0x3e4d24,
        keycap: 0xaebb76,
        alert: 0x6b2a10,
        on_alert: 0xdde6ac,
        label_dust: 0x0f140a,
        off: 0x7f8e52,
        text: 0x141b0c,
        soft: 0x24301a,
        quiet: 0x34421f,
        pill: 0xd6e0a2,
        chip: 0x2f3d1c,
        band: 0xa9b770,
        select: 0x768546,
        amber: 0x2f3d1c,
        star: 0x2f3d1c,
        split_bg: 0xbfcb8a,
        split_dim: 0x34421f,
        on_wash: 0x141b0c,
        t: 255,
    },
    wood: [0x3b4a22, 0x9cae5e],
    stain: 210,
    holder: 0x5d7034,
    card: 0xcdd89a,
    card_ink: 0x182010,
    lamp: 0xdfe8ac,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_icon_decodes() {
        for (icon, _) in FILES {
            let s = sprite(icon).unwrap_or_else(|| panic!("{icon:?}"));
            assert_eq!((s.w, s.h), (32, 32), "{icon:?}");
        }
    }

    #[test]
    fn retro_follows_the_style_row_and_keeps_its_palette() {
        for c in Colours::ALL {
            assert_eq!(Colours::from_u8(c as u8), c);
        }
        assert_eq!(Colours::default(), Colours::Cocoa);
        assert_eq!(Colours::Cocoa.step(true), Colours::Pea);
        assert_eq!(Colours::Cocoa.step(false), Colours::Pea);
        assert!(!on());
        crate::font_style::set(crate::font_style::FontStyle::Retro);
        assert!(on());
        crate::font_style::set(crate::font_style::FontStyle::Barlow);
        assert!(!on());
    }
}
