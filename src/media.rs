//! The games' cases as they stand on a shelf: a pixel-art template per kind of case (`assets/media`, drawn by
//! `assets/media/tools/draw_media.py`), painted with the colour of the game's cover, its title and its cover. Shared by
//! the Shelf look (every entry a spine on one shelf, the selected one shown with its front, `app_cabinet.rs` "the Shelf
//! look") and the Cabinet look (cases standing on a cabinet's shelves). Every game stands as a case, never as a bare
//! cartridge: a cartridge system's games as their cardboard boxes (0.1.4). What is inside (`Game`: the cartridge, game
//! card, disc or UMD) comes out when the box is opened (`game_box`) and goes into the console at the start.
//!
//! A template pixel is `SHELF_SCALE` screen pixels at the Cabinet's size (`Medium::size`, `k` = 1); the Shelf look
//! picks its own scale for the screen. Scaling is by nearest pixel, so pixel art stays pixel art. Templates are
//! decoded once, on first use.

use crate::cabinet::{ink_on, luma, Size};
use crate::cabinet_art::{self as art, Look, Pic};
use crate::gfx::mix;
use crate::image::Image;
use crate::system_art::Sprite;
use crate::text::Text;

/// The kinds of case, each with a template of its own.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Medium {
    /// The cardboard boxes of the cartridge systems: the SNES's and N64's lie wide, the NES's stands.
    Snes,
    N64,
    Nes,
    /// Game Boy and Game Boy Color.
    Gb,
    Gba,
    /// pocket-ui's own case for every other cartridge system: one size, a band in the system's colour with its console
    /// and name (`front_band`), a generic cartridge inside.
    GenericCart,
    /// The Mega Drive's clamshell case.
    MegaDrive,
    /// The Master System's and Game Gear's case with the white grid.
    MasterSystem,
    /// The thin PlayStation jewel case.
    Ps1,
    /// The Saturn's, Dreamcast's and Mega-CD's light disc case.
    SegaDisc,
    /// pocket-ui's own case for every other disc system: DVD-sized, a generic disc inside.
    GenericDisc,
    /// The tall black DVD case.
    Ps2,
    GameCube,
    /// The white DVD case.
    Wii,
    /// A plain DVD case: other DVD systems, and Curator's boxes in Curator's colour.
    Keep,
    /// The PSP's slim UMD case.
    Umd,
    Ds,
    N3ds,
    /// The original Xbox's black DVD case with its green top band.
    Xbox,
    /// The PS Vita's small case with the blue top band.
    Vita,
    /// pocket-ui's own small case for what has no medium (Ports, PICO-8, TIC-80…): a memory card inside.
    GenericDigital,
}

/// Where things are on a template (template pixels): the spine's label and its band at the top (cases whose band
/// takes the cover's accent colour), the front's cover, and the spine's two ends that are not stretched.
struct Template {
    spine: (i32, i32),
    front: (i32, i32),
    spine_label: (i32, i32, i32, i32),
    front_label: (i32, i32, i32, i32),
    band: Option<(i32, i32, i32, i32)>,
    /// The front's band in the system's colour with its console and name (pocket-ui's own cases).
    front_band: Option<(i32, i32, i32, i32)>,
    /// The spine's two ends (template rows) that a stretched spine would keep: printed by `draw_media.py` with the
    /// rest, checked by the tests; nothing stretches a spine at the moment.
    #[allow(dead_code)]
    caps: (i32, i32),
    /// Drawn in neutral grey, coloured with the cover's colour (a cardboard box's printed card).
    tint: bool,
    /// Its pictures are this many times the template's size (a photo converted by PixelLab: 2), its rectangles in their
    /// pixels.
    res: i32,
}

impl Template {
    /// One of its rectangles on a picture made `f` screen pixels a template pixel.
    fn at(&self, r: (i32, i32, i32, i32), f: f32) -> (i32, i32, i32, i32) {
        scale_rect(r, f / self.res as f32)
    }
}

/// Screen pixels a template pixel on the Cabinet's shelf (`k` = 1 everywhere below): two and a half, so a shelf of
/// a few games still fills the cabinet and the titles on the spines are 30 px.
pub const SHELF_SCALE: f32 = 2.5;

/// The template's neutral plastic grey (`draw_media.py`'s GREY).
const GREY: u32 = 156;

// BEGIN CASE TABLE (assets/media/case/tools/table.py)
/// The cases, in the order of `Medium::ALL`: drawn by `assets/media/tools/draw_media.py` (res 1) or converted from photos
/// (`assets/media/case/SOURCES.md`, their pictures `res` times the size, their rectangles in their pixels).
#[rustfmt::skip]
const TEMPLATES: [Template; 21] = [
    // snes
    Template { spine: (24, 116), front: (116, 84), spine_label: (2, 19, 20, 91), front_label: (1, 1, 114, 82), band: Some((2, 5, 20, 12)), front_band: None, caps: (19, 6), tint: true, res: 1 },
    // n64
    Template { spine: (24, 116), front: (116, 82), spine_label: (2, 19, 20, 91), front_label: (1, 1, 114, 80), band: Some((2, 5, 20, 12)), front_band: None, caps: (19, 6), tint: true, res: 1 },
    // nes
    Template { spine: (24, 116), front: (84, 116), spine_label: (2, 19, 20, 91), front_label: (1, 1, 82, 114), band: Some((2, 5, 20, 12)), front_band: None, caps: (19, 6), tint: true, res: 1 },
    // gb
    Template { spine: (20, 81), front: (76, 81), spine_label: (2, 16, 16, 59), front_label: (1, 1, 74, 79), band: Some((2, 5, 16, 9)), front_band: None, caps: (16, 6), tint: true, res: 1 },
    // gba
    Template { spine: (20, 74), front: (74, 74), spine_label: (2, 15, 16, 53), front_label: (1, 1, 72, 72), band: Some((2, 5, 16, 8)), front_band: None, caps: (15, 6), tint: true, res: 1 },
    // generic-cart
    Template { spine: (22, 113), front: (82, 113), spine_label: (5, 38, 34, 183), front_label: (6, 38, 152, 182), band: Some((5, 5, 34, 30)), front_band: Some((6, 6, 152, 30)), caps: (1, 1), tint: false, res: 2 },
    // md
    Template { spine: (20, 120), front: (85, 120), spine_label: (2, 16, 16, 102), front_label: (3, 3, 79, 114), band: Some((2, 2, 16, 13)), front_band: None, caps: (16, 3), tint: false, res: 1 },
    // sms
    Template { spine: (20, 120), front: (85, 120), spine_label: (2, 15, 16, 103), front_label: (8, 8, 69, 104), band: None, front_band: None, caps: (15, 3), tint: false, res: 1 },
    // ps1
    Template { spine: (13, 79), front: (90, 79), spine_label: (4, 8, 18, 142), front_label: (19, 5, 156, 148), band: None, front_band: None, caps: (1, 1), tint: false, res: 2 },
    // segadisc
    Template { spine: (16, 80), front: (90, 80), spine_label: (5, 8, 22, 144), front_label: (19, 5, 156, 150), band: None, front_band: None, caps: (1, 1), tint: false, res: 2 },
    // generic-disc
    Template { spine: (19, 120), front: (85, 120), spine_label: (5, 38, 28, 197), front_label: (6, 40, 158, 194), band: Some((5, 5, 28, 30)), front_band: Some((6, 6, 158, 32)), caps: (1, 1), tint: false, res: 2 },
    // ps2
    Template { spine: (19, 120), front: (85, 120), spine_label: (5, 40, 28, 195), front_label: (6, 6, 158, 228), band: None, front_band: None, caps: (1, 1), tint: false, res: 2 },
    // gc
    Template { spine: (18, 116), front: (85, 116), spine_label: (5, 40, 26, 187), front_label: (6, 6, 158, 220), band: None, front_band: None, caps: (1, 1), tint: false, res: 2 },
    // wii
    Template { spine: (19, 120), front: (85, 120), spine_label: (5, 40, 28, 195), front_label: (6, 6, 158, 228), band: None, front_band: None, caps: (1, 1), tint: false, res: 2 },
    // keep
    Template { spine: (19, 120), front: (85, 120), spine_label: (2, 17, 15, 101), front_label: (3, 3, 79, 114), band: Some((2, 2, 15, 14)), front_band: None, caps: (17, 3), tint: false, res: 1 },
    // umd
    Template { spine: (15, 110), front: (66, 110), spine_label: (5, 36, 20, 179), front_label: (6, 6, 120, 208), band: None, front_band: None, caps: (1, 1), tint: false, res: 2 },
    // ds
    Template { spine: (16, 86), front: (79, 86), spine_label: (5, 32, 22, 135), front_label: (6, 6, 146, 160), band: None, front_band: None, caps: (1, 1), tint: false, res: 2 },
    // 3ds
    Template { spine: (15, 86), front: (79, 86), spine_label: (5, 32, 20, 135), front_label: (6, 6, 146, 160), band: None, front_band: None, caps: (1, 1), tint: false, res: 2 },
    // xbox
    Template { spine: (19, 120), front: (85, 120), spine_label: (5, 40, 28, 195), front_label: (6, 6, 158, 228), band: None, front_band: None, caps: (1, 1), tint: false, res: 2 },
    // vita
    Template { spine: (14, 86), front: (66, 86), spine_label: (1, 15, 12, 70), front_label: (3, 15, 60, 68), band: None, front_band: None, caps: (15, 2), tint: false, res: 1 },
    // generic-digital
    Template { spine: (14, 86), front: (66, 86), spine_label: (5, 32, 18, 135), front_label: (6, 32, 120, 134), band: Some((5, 5, 18, 24)), front_band: Some((6, 6, 120, 24)), caps: (1, 1), tint: false, res: 2 },
];
// END CASE TABLE

/// The templates' pictures: (spine, front) per medium, in the order of `Medium::ALL`.
const PNGS: [(&[u8], &[u8]); 21] = [
    (include_bytes!("../assets/media/snes-spine.png"), include_bytes!("../assets/media/snes-front.png")),
    (include_bytes!("../assets/media/n64-spine.png"), include_bytes!("../assets/media/n64-front.png")),
    (include_bytes!("../assets/media/nes-spine.png"), include_bytes!("../assets/media/nes-front.png")),
    (include_bytes!("../assets/media/gb-spine.png"), include_bytes!("../assets/media/gb-front.png")),
    (include_bytes!("../assets/media/gba-spine.png"), include_bytes!("../assets/media/gba-front.png")),
    (include_bytes!("../assets/media/generic-cart-spine.png"), include_bytes!("../assets/media/generic-cart-front.png")),
    (include_bytes!("../assets/media/md-spine.png"), include_bytes!("../assets/media/md-front.png")),
    (include_bytes!("../assets/media/sms-spine.png"), include_bytes!("../assets/media/sms-front.png")),
    (include_bytes!("../assets/media/ps1-spine.png"), include_bytes!("../assets/media/ps1-front.png")),
    (include_bytes!("../assets/media/segadisc-spine.png"), include_bytes!("../assets/media/segadisc-front.png")),
    (include_bytes!("../assets/media/generic-disc-spine.png"), include_bytes!("../assets/media/generic-disc-front.png")),
    (include_bytes!("../assets/media/ps2-spine.png"), include_bytes!("../assets/media/ps2-front.png")),
    (include_bytes!("../assets/media/gc-spine.png"), include_bytes!("../assets/media/gc-front.png")),
    (include_bytes!("../assets/media/wii-spine.png"), include_bytes!("../assets/media/wii-front.png")),
    (include_bytes!("../assets/media/keep-spine.png"), include_bytes!("../assets/media/keep-front.png")),
    (include_bytes!("../assets/media/umd-spine.png"), include_bytes!("../assets/media/umd-front.png")),
    (include_bytes!("../assets/media/ds-spine.png"), include_bytes!("../assets/media/ds-front.png")),
    (include_bytes!("../assets/media/3ds-spine.png"), include_bytes!("../assets/media/3ds-front.png")),
    (include_bytes!("../assets/media/xbox-spine.png"), include_bytes!("../assets/media/xbox-front.png")),
    (include_bytes!("../assets/media/vita-spine.png"), include_bytes!("../assets/media/vita-front.png")),
    (include_bytes!("../assets/media/generic-digital-spine.png"), include_bytes!("../assets/media/generic-digital-front.png")),
];

impl Medium {
    pub const ALL: [Medium; 21] = [
        Medium::Snes,
        Medium::N64,
        Medium::Nes,
        Medium::Gb,
        Medium::Gba,
        Medium::GenericCart,
        Medium::MegaDrive,
        Medium::MasterSystem,
        Medium::Ps1,
        Medium::SegaDisc,
        Medium::GenericDisc,
        Medium::Ps2,
        Medium::GameCube,
        Medium::Wii,
        Medium::Keep,
        Medium::Umd,
        Medium::Ds,
        Medium::N3ds,
        Medium::Xbox,
        Medium::Vita,
        Medium::GenericDigital,
    ];

    /// What a ROCKNIX system's games come in; a generic cartridge or disc case for the rest.
    pub fn of(system: &str) -> Medium {
        use crate::cabinet::FormFactor;
        match system {
            "snes" | "snesh" | "sfc" | "satellaview" | "sufami" => Medium::Snes,
            "n64" | "64dd" => Medium::N64,
            "nes" | "nesh" | "famicom" | "fds" => Medium::Nes,
            "gb" | "gbh" | "gbc" | "gbch" => Medium::Gb,
            "gba" | "gbah" => Medium::Gba,
            "megadrive" | "genesis" | "megadrive-japan" | "megadriveh" | "genh" | "sega32x" => Medium::MegaDrive,
            "mastersystem" | "gamegear" | "ggh" | "sg-1000" | "sg1000" => Medium::MasterSystem,
            "psx" | "ps1" | "playstation" => Medium::Ps1,
            "saturn" | "dreamcast" | "segacd" | "megacd" => Medium::SegaDisc,
            "ps2" => Medium::Ps2,
            "gamecube" | "gc" => Medium::GameCube,
            "wii" => Medium::Wii,
            "psp" => Medium::Umd,
            "nds" => Medium::Ds,
            "3ds" | "n3ds" => Medium::N3ds,
            "xbox" => Medium::Xbox,
            "psvita" => Medium::Vita,
            // Every other system: pocket-ui's own case, by what its games come on.
            _ if is_digital(system) => Medium::GenericDigital,
            _ => match FormFactor::of(system) {
                FormFactor::HandheldCart | FormFactor::ConsoleCart | FormFactor::CardBox => Medium::GenericCart,
                FormFactor::JewelCase | FormFactor::DvdCase | FormFactor::UmdCase => Medium::GenericDisc,
            },
        }
    }

    /// pocket-ui's own case (every system without a case of its own): its front has a band in the system's colour.
    pub fn is_generic(self) -> bool {
        matches!(self, Medium::GenericCart | Medium::GenericDisc | Medium::GenericDigital)
    }

    /// The real case's front, width x height in millimetres (docs/DESIGN.md "Media"); the templates are drawn from
    /// these at one scale (`MM_PX`).
    pub fn mm(self) -> (f32, f32) {
        match self {
            Medium::Snes => (184.0, 133.0),
            Medium::N64 => (184.0, 130.0),
            Medium::Nes => (133.0, 184.0),
            Medium::Gb => (120.0, 128.0),
            Medium::Gba => (118.0, 118.0),
            Medium::GenericCart => (130.0, 180.0),
            Medium::Ps1 => (142.0, 125.0),
            Medium::SegaDisc => (142.0, 127.0),
            Medium::GameCube => (135.0, 184.0),
            Medium::Umd => (104.0, 175.0),
            Medium::Ds | Medium::N3ds => (125.0, 137.0),
            Medium::Vita | Medium::GenericDigital => (105.0, 136.0),
            Medium::MegaDrive | Medium::MasterSystem | Medium::GenericDisc | Medium::Ps2 | Medium::Wii | Medium::Keep | Medium::Xbox => (135.0, 190.0),
        }
    }

    fn index(self) -> usize {
        Medium::ALL.iter().position(|m| *m == self).unwrap_or(0)
    }

    fn template(self) -> &'static Template {
        &TEMPLATES[self.index()]
    }

    /// Its size on the Cabinet's shelf, in screen pixels: the template at `SHELF_SCALE`.
    pub fn size(self) -> Size {
        let t = self.template();
        let s = |v: i32| (v as f32 * SHELF_SCALE).round() as i32;
        Size {
            spine: s(t.spine.0),
            front: s(t.front.0),
            h: s(t.spine.1),
            front_h: s(t.front.1),
        }
    }

    /// The template's own size, one pixel a template pixel: the Shelf look scales it to the screen.
    pub fn template_size(self) -> Size {
        let t = self.template();
        Size {
            spine: t.spine.0,
            front: t.front.0,
            h: t.spine.1,
            front_h: t.front.1,
        }
    }

    /// A cartridge system's cardboard box (no clear sleeve, the card in the cover's colour).
    pub fn is_cardboard(self) -> bool {
        self.template().tint
    }

    /// What is inside the case.
    pub fn game(self) -> Game {
        match self {
            Medium::Snes => Game::Snes,
            Medium::N64 => Game::N64,
            Medium::Nes => Game::Nes,
            Medium::Gb => Game::Gb,
            Medium::Gba => Game::Gba,
            Medium::GenericCart | Medium::MasterSystem => Game::Cart,
            Medium::MegaDrive => Game::MegaDrive,
            Medium::Ps1 | Medium::SegaDisc | Medium::GenericDisc | Medium::Ps2 | Medium::Wii | Medium::Keep | Medium::Xbox => Game::Disc,
            Medium::GameCube => Game::MiniDisc,
            Medium::Umd => Game::Umd,
            Medium::Ds | Medium::N3ds | Medium::Vita => Game::Card,
            Medium::GenericDigital => Game::Memory,
        }
    }

    /// The decoded templates, (spine, front).
    fn pics(self) -> &'static (Sprite, Sprite) {
        static DECODED: [std::sync::OnceLock<(Sprite, Sprite)>; 21] = [const { std::sync::OnceLock::new() }; 21];
        let i = self.index();
        DECODED[i].get_or_init(|| {
            let empty = || Sprite { w: 1, h: 1, px: vec![0] };
            let (s, f) = PNGS[i];
            (crate::system_art::decode(s).unwrap_or_else(empty), crate::system_art::decode(f).unwrap_or_else(empty))
        })
    }
}

/// The systems whose games come on no medium of their own (ports, fantasy consoles, engines, downloads): pocket-ui's
/// small case with a memory card inside.
pub fn is_digital(system: &str) -> bool {
    matches!(
        system,
        "ports" | "pico-8" | "pico8" | "tic-80" | "tic80" | "scummvm" | "openbor" | "easyrpg" | "solarus" | "lutro" | "love" | "pygame" | "wiiware" | "pspockets" | "steam" | "moonlight" | "streaming"
    )
}

/// Decodes every template and prop now (on a thread of the caller's: the Cabinet's reader), so no frame decodes one.
pub fn warm() {
    for m in Medium::ALL {
        m.pics();
    }
    for p in Prop::ALL {
        p.pic();
    }
    for g in Game::DRAWN {
        g.pic();
    }
    for d in DiscKind::ALL {
        d.template();
    }
}

/// A template pixel in the cover's colour (cardboard), the system's plastic (a cartridge) or as drawn (cases).
#[inline]
fn colour(p: u32, tint: Option<u32>) -> u32 {
    let Some(plastic) = tint else { return p & 0xffffff };
    // The grey's brightness, relative to the template's plastic grey, laid over the plastic.
    let l = luma(p & 0xffffff);
    let ch = |shift: u32| (((plastic >> shift & 0xff) * l / GREY).min(255)) << shift;
    ch(16) | ch(8) | ch(0)
}

/// Screen pixels of a template's span `from .. from + len` scaled by `f` (the pixels whose template pixel is in it).
fn span(from: i32, len: i32, f: f32) -> (i32, i32) {
    let a = (from as f32 * f).ceil() as i32;
    let b = ((from + len) as f32 * f).ceil() as i32;
    (a, (b - a).max(1))
}

fn scale_rect((x, y, w, h): (i32, i32, i32, i32), f: f32) -> (i32, i32, i32, i32) {
    let (x, w) = span(x, w, f);
    let (y, h) = span(y, h, f);
    (x, y, w, h)
}

/// A template scaled by `f`, nearest pixel (pixel art stays pixel art), coloured.
fn scaled(s: &Sprite, f: f32, tint: Option<u32>) -> Pic {
    let (w, h) = ((s.w as f32 * f).round() as i32, (s.h as f32 * f).round() as i32);
    let mut p = Pic::new(w, h, 0);
    // The template column of each screen column, worked out once (a front is half a million pixels at the Shelf look's
    // size).
    let cols: Vec<u32> = (0..w).map(|x| ((x as f32 / f) as u32).min(s.w - 1)).collect();
    for y in 0..h {
        let sy = ((y as f32 / f) as u32).min(s.h - 1);
        let src = &s.px[(sy * s.w) as usize..((sy + 1) * s.w) as usize];
        let dst = &mut p.px[(y * w) as usize..((y + 1) * w) as usize];
        for (d, &sx) in dst.iter_mut().zip(&cols) {
            let px = src[sx as usize];
            if px >> 24 != 0 {
                *d = 0xff00_0000 | colour(px, tint);
            }
        }
    }
    p
}

/// A picture scaled by `k`, nearest pixel (pixel art stays pixel art): the props at the Shelf look's scale.
pub fn rescaled(p: &Pic, k: f32) -> Pic {
    let (w, h) = ((p.w as f32 * k).round() as i32, (p.h as f32 * k).round() as i32);
    let mut out = Pic::new(w, h, 0);
    for y in 0..out.h {
        let sy = ((y as f32 / k) as i32).min(p.h - 1);
        for x in 0..out.w {
            out.px[(y * out.w + x) as usize] = p.px[(sy * p.w + ((x as f32 / k) as i32).min(p.w - 1)) as usize];
        }
    }
    out
}

/// A label's paper in `color`: a little lighter at the top, darker at the bottom, its edges catching the light.
fn paper(p: &mut Pic, (x, y, w, h): (i32, i32, i32, i32), color: u32) {
    for row in 0..h {
        let k = 1.05 - 0.13 * row as f32 / (h - 1).max(1) as f32;
        let c = scale(color, k);
        p.fill(x, y + row, w, 1, 0xff00_0000 | c);
    }
    // A printed label's edge: a thin darker line around it, its top catching the light.
    p.tint(x, y, w, 1, 0x000000, 70);
    p.tint(x, y + h - 1, w, 1, 0x000000, 80);
    p.tint(x, y, 1, h, 0x000000, 60);
    p.tint(x + w - 1, y, 1, h, 0x000000, 80);
    p.tint(x + 1, y + 1, w - 2, 1, 0xffffff, 50);
}

fn scale(c: u32, k: f32) -> u32 {
    let ch = |s: u32| (((c >> s & 0xff) as f32 * k).round().clamp(0.0, 255.0) as u32) << s;
    ch(16) | ch(8) | ch(0)
}

fn tint_of(m: Medium, look: Look) -> Option<u32> {
    m.is_cardboard().then_some(look.color)
}

/// The spine of a game's case standing on the shelf, `k` times its shelf size: the template, the label in the
/// cover's colour with the title read top to bottom, the band (on cases that have one) in the cover's accent.
pub fn spine(text: &mut Text, m: Medium, k: f32, look: Look, title: &str) -> Pic {
    // 24 px, or 21 on a narrow label (the Game Boy's, the thin jewel case's): the two sizes the shelf has always used,
    // so a new spine finds its letters drawn already. Bigger (a case out of the shelf) at the nearest size the app
    // paints anyway, never past the label.
    let (_, _, w, _) = m.template().at(m.template().spine_label, SHELF_SCALE * k);
    let want = (if (w as f32 / k) < 25.0 { 21.0 } else { 24.0 }) * k;
    spine_with(text, m, k, look, title, want)
}

/// The sizes a spine's title is written in: the ones the app paints anyway, so a spine made in a frame draws no
/// letters of its own (tests/shelf_moves.rs).
const SPINE_PX: [u32; 8] = [21, 24, 28, 30, 37, 44, 50, 67];

/// The title size for a spine label `w` pixels across: the largest of `SPINE_PX` up to `want` whose letters, capitals
/// to descenders, still fit across the label with a little paper on both sides; 21 at the least.
pub fn spine_px(w: i32, want: f32) -> u32 {
    SPINE_PX
        .into_iter()
        .rev()
        .find(|&s| s as f32 <= want + 0.5 && (s as f32 * 0.86).round() as i32 <= w - 6)
        .unwrap_or(SPINE_PX[0])
}

/// `spine` with the title at `want` pixels or the nearest size below it that fits the label (`spine_px`): the Shelf
/// look's spines, whose title follows Settings → Text size.
pub fn spine_with(text: &mut Text, m: Medium, k: f32, look: Look, title: &str, want: f32) -> Pic {
    let look = spine_print(m).map_or(look, |(paper, band)| Look { color: paper.unwrap_or(look.color), band, ..look });
    let t = m.template();
    let f = SHELF_SCALE * k;
    let (spine, _) = m.pics();
    let mut p = scaled(spine, f / t.res as f32, tint_of(m, look));
    card_edge(&mut p, m, f);
    if let Some(band) = t.band {
        let (x, y, w, h) = t.at(band, f);
        paper(&mut p, (x, y, w, h), look.band);
        if m == Medium::N64 {
            n64_badge(&mut p, (x, y, w, h));
        }
    }
    let (x, y, w, h) = t.at(t.spine_label, f);
    paper(&mut p, (x, y, w, h), look.color);
    let pad = (4.0 * k).round() as i32;
    let (lines, px) = spine_fit(text, title, w, h - 2 * pad, want);
    let ink = ink_on(look.color);
    match lines.as_slice() {
        [one] => art::spine_title(text, &mut p, one, px, x, w, y + pad, h - 2 * pad, ink),
        // Two lines across a thick spine: the first at the right (the top of the letters faces right), as printed.
        // The halves of a name are not words to translate.
        [first, second] => {
            let translating = text.translating();
            text.set_translate(false);
            art::spine_title(text, &mut p, first, px, x + w / 2, w - w / 2, y + pad, h - 2 * pad, ink);
            art::spine_title(text, &mut p, second, px, x, w / 2, y + pad, h - 2 * pad, ink);
            text.set_translate(translating);
        }
        _ => {}
    }
    p
}

/// The cut edge of a cardboard box: one template pixel all round in the same warm grey on every box, whatever its
/// print. A box in the cover's colour has its edge in that colour otherwise, so a dark one melted into the dark page and
/// looked smaller than a white one beside it (the owner, 2026-10-29: "same platform, different sizes?").
const CARD_EDGE: u32 = 0x8a8074;

fn card_edge(p: &mut Pic, m: Medium, f: f32) {
    if !m.is_cardboard() {
        return;
    }
    let e = (f.round() as i32).max(1);
    let (w, h) = (p.w, p.h);
    for (x, y, ew, eh) in [(0, 0, w, e), (0, h - e, w, e), (0, 0, e, h), (w - e, 0, e, h)] {
        p.fill(x, y, ew, eh, 0xff00_0000 | CARD_EDGE);
    }
}

/// The printed spine a system's cases share, whatever the game (paper, band; no paper = the cover's own colour): every
/// box of the system reads the same on the shelf, as real ones do. The US Super Nintendo's grey with its dark band, the
/// NES's black box, the Game Boy's and the Game Boy Advance's white with their band; the Nintendo 64's art in the
/// cover's colour with the red block at its top that carries the white logo square (US boxes; `n64_badge`); and the PS2
/// case's black insert spine.
fn spine_print(m: Medium) -> Option<(Option<u32>, u32)> {
    match m {
        Medium::Snes => Some((Some(0xc9c9cd), 0x3a3a40)),
        Medium::N64 => Some((None, 0xd0101a)),
        Medium::Nes => Some((Some(0x101012), 0xb3141c)),
        Medium::Gb => Some((Some(0xeeeeea), 0x2d2f6e)),
        Medium::Gba => Some((Some(0xf1f1ef), 0x463f96)),
        // The PS2's insert: a black spine whatever the cover, a neutral block at its top (no logo).
        Medium::Ps2 => Some((Some(0x111114), 0x2a2a2e)),
        _ => None,
    }
}

/// The white square in the red block of an N64 box's spine (where the logo is printed; plain here), `band` its block.
fn n64_badge(p: &mut Pic, (x, y, w, h): (i32, i32, i32, i32)) {
    let side = (h * 3 / 4).min(w - 2).max(2);
    let (sx, sy) = (x + (w - side) / 2, y + (h - side) / 2);
    p.fill(sx, sy, side, side, 0xfff4f4f0);
    // The logo's colours as two blocks (green and blue), no lettering.
    let q = (side / 3).max(1);
    p.fill(sx + q / 2, sy + side - q - q / 2, q, q, 0xff1f9a3a);
    p.fill(sx + side - q - q / 2, sy + side - q - q / 2, q, q, 0xff2742b0);
}

/// A cardboard box's front over its cover: the tuck flap's fold a little in from the top edge and the corners rubbed
/// light, `f` screen pixels a template pixel.
fn box_wear(p: &mut Pic, f: f32) {
    let e = (f.round() as i32).max(1);
    let fold = 2 * e + e / 2;
    p.tint(e, fold, p.w - 2 * e, (e / 2).max(1), 0xffffff, 70);
    p.tint(e, fold + (e / 2).max(1), p.w - 2 * e, (e / 2).max(1), 0x000000, 50);
    for (cx, cy) in [(0, 0), (p.w - 1, 0), (0, p.h - 1), (p.w - 1, p.h - 1)] {
        for i in 0..3 * e {
            for j in 0..3 * e - i {
                let x = if cx == 0 { i } else { cx - i };
                let y = if cy == 0 { j } else { cy - j };
                p.blend(x, y, 0xe8e2d8, 90);
            }
        }
    }
}

/// A title's main part and its subtitle (" - ", ": "), None when it has none.
fn main_title(title: &str) -> Option<(&str, &str)> {
    let (cut, sep) = [" - ", ": "].iter().filter_map(|sep| title.find(sep).map(|i| (i, sep.len()))).min()?;
    let (main, sub) = (title[..cut].trim(), title[cut + sep..].trim());
    (main.len() >= 3 && !sub.is_empty()).then_some((main, sub))
}

/// How a title goes on a spine label `thick` pixels across and `len` long, at most `want` pixels: its lines (one, or
/// two across a spine thick enough) and their size (one of `SPINE_PX`). Until it fits whole, in this order: a little
/// smaller (seven tenths of `want`); on two lines, a subtitle on a line of its own without its dash; smaller still, down
/// to 21 px (it still reads); the same again without the subtitle after " - " or ": " ("Prince of Persia"); and only
/// then cut short with "…" (trailer frames, 2026-10-29: "Advance…", "Donkey Kon…" on spines that had the room).
pub fn spine_fit(text: &mut Text, title: &str, thick: i32, len: i32, want: f32) -> (Vec<String>, u32) {
    // As thick as the case: the design's sizes whatever Settings → Text size says (`type_scale`).
    let _fixed = crate::type_scale::fixed();
    let face = crate::text::Face::Title;
    let across = |s: u32| (s as f32 * 0.86).round() as i32;
    // The sizes that fit across the label, largest first (the smallest of all on a label too narrow for any).
    let mut sizes: Vec<u32> = SPINE_PX.into_iter().rev().filter(|&s| s as f32 <= want + 0.5 && across(s) <= thick - 6).collect();
    if sizes.is_empty() {
        sizes.push(SPINE_PX[0]);
    }
    let near = (want * 0.7).max(SPINE_PX[0] as f32);
    let (big, small): (Vec<u32>, Vec<u32>) = sizes.iter().partition(|&&s| s as f32 >= near - 0.5);
    // Two lines with two pixels between them and one of paper at each edge.
    let two: Vec<u32> = sizes.iter().copied().filter(|&s| 2 * across(s) + 2 <= thick - 2).collect();
    let split = main_title(title);
    let mut tries = vec![title];
    tries.extend(split.map(|(main, _)| main));
    for (i, t) in tries.iter().enumerate() {
        let one = |text: &mut Text, list: &[u32]| list.iter().copied().find(|&s| text.width(face, s, t) <= len);
        if let Some(s) = one(text, &big) {
            return (vec![t.to_string()], s);
        }
        for &s in &two {
            // The whole title on two lines: a subtitle on a line of its own, without its dash.
            let lines = match split.filter(|_| i == 0) {
                Some((main, sub)) => vec![main.to_string(), sub.to_string()],
                None => text.wrap(face, s, t, len),
            };
            // Two lines of whole words: a word broken in pieces would not read.
            let whole = split.is_some() && i == 0 || lines.join(" ") == t.split_whitespace().collect::<Vec<_>>().join(" ");
            // The halves of a name are measured as they are (they are not words to translate, `spine_with`).
            let translating = text.translating();
            text.set_translate(false);
            let fits = lines.iter().all(|l| text.width(face, s, l) <= len);
            text.set_translate(translating);
            if lines.len() == 2 && whole && fits {
                return (lines, s);
            }
        }
        if let Some(s) = one(text, &small) {
            return (vec![t.to_string()], s);
        }
    }
    let s = *sizes.last().unwrap_or(&SPINE_PX[0]);
    let short = tries.last().copied().unwrap_or(title);
    (vec![text.fit(face, s, short, len)], s)
}

/// The size a case's front is made at for `k` times its shelf size.
pub fn front_size(m: Medium, k: f32) -> (i32, i32) {
    let t = m.template();
    let f = SHELF_SCALE * k;
    ((t.front.0 as f32 * f).round() as i32, (t.front.1 as f32 * f).round() as i32)
}

/// Where the cover sits on the front made at `k` times the shelf size: (x, y, width, height).
pub fn label_rect(m: Medium, k: f32) -> (i32, i32, i32, i32) {
    m.template().at(m.template().front_label, SHELF_SCALE * k)
}

/// Where a spine made at `k` times its shelf size has its title's label: (x, y, width, height). Above it is the
/// spine's top, where a case sticker goes (`case_stickers::on_spine`).
pub fn spine_label_rect(m: Medium, k: f32) -> (i32, i32, i32, i32) {
    m.template().at(m.template().spine_label, SHELF_SCALE * k)
}

/// The front of a case, `k` times its shelf size: the cover (`cover`, already `cabinet_art::cover_fit` to the
/// size of `label_rect`) in the case, or a plain label with the console (`art`) and the title. pocket-ui's own case
/// (`Medium::is_generic`) has a band at the top in the system's colour (`look.band`) with its console and `system`, its
/// name.
#[allow(clippy::too_many_arguments)]
pub fn front(text: &mut Text, m: Medium, k: f32, look: Look, title: &str, cover: Option<&Image>, console: Option<&Sprite>, system: &str) -> Pic {
    let (_, tpl) = m.pics();
    let t = m.template();
    let mut p = scaled(tpl, SHELF_SCALE * k / t.res as f32, tint_of(m, look));
    card_edge(&mut p, m, SHELF_SCALE * k);
    if let Some(band) = t.front_band {
        system_band(text, &mut p, t.at(band, SHELF_SCALE * k), look.band, console, system);
    }
    let (lx, ly, lw, lh) = label_rect(m, k);
    match cover {
        Some(img) => {
            let w = lw.min(img.w as i32).min(p.w - lx).max(0) as usize;
            for y in 0..lh.min(img.h as i32).min(p.h - ly) {
                let src = &img.px[(y as u32 * img.w) as usize..][..w];
                let dst = &mut p.px[((ly + y) * p.w + lx) as usize..][..w];
                for (d, &s) in dst.iter_mut().zip(src) {
                    *d = 0xff00_0000 | s;
                }
            }
        }
        None => art::plain_label(text, &mut p, (lx, ly, lw, lh), look, title, console),
    }
    if m.is_cardboard() {
        box_wear(&mut p, SHELF_SCALE * k);
    } else {
        // The clear sleeve over the cover.
        art::sheen(&mut p, 18);
    }
    p
}

/// The band of pocket-ui's own case: the system's colour, its console small at the left and its name beside it (as
/// large as the band takes, 21 px at the least, cut with "…" when long).
fn system_band(text: &mut Text, p: &mut Pic, (x, y, w, h): (i32, i32, i32, i32), color: u32, console: Option<&Sprite>, system: &str) {
    paper(p, (x, y, w, h), color);
    let pad = (h / 6).max(2);
    let mut tx = x + pad * 2;
    if let Some(s) = console {
        // Its console, fitted to the band's height (nearest pixel, as all pixel art here).
        let ch = h - 2 * pad;
        let cw = (s.w as i32 * ch / s.h.max(1) as i32).min(w / 3);
        if cw > 4 && ch > 4 {
            for yy in 0..ch {
                let sy = (yy as u32 * s.h / ch as u32).min(s.h - 1);
                for xx in 0..cw {
                    let sx = (xx as u32 * s.w / cw as u32).min(s.w - 1);
                    let px = s.px[(sy * s.w + sx) as usize];
                    if px >> 24 != 0 {
                        p.blend(tx + xx, y + pad + yy, px & 0xffffff, px >> 24);
                    }
                }
            }
            tx += cw + pad * 2;
        }
    }
    // Printed on the case: its size follows the case, not Settings → Text size, and is one the app paints anyway (a
    // front made in a frame draws no letters of its own, tests/shelf_moves.rs).
    let _fixed = crate::type_scale::fixed();
    let size = [24u32, 28, 30, 37].into_iter().rev().find(|&s| s as f32 <= h as f32 * 0.6).unwrap_or(24);
    let room = x + w - pad * 2 - tx;
    if room > 20 && !system.is_empty() {
        let name = text.fit(crate::text::Face::Title, size, system, room);
        let ink = ink_on(color);
        let bh = size as i32 * 3 / 2;
        let mut buf = vec![0u32; (room * bh) as usize];
        {
            let mut c = crate::gfx::Canvas::new(&mut buf, room as u32, bh as u32);
            text.draw(&mut c, crate::text::Face::Title, size, 0, size as i32, 0xffffff, &name);
        }
        // The capitals in the band's middle: they are about 0.7 of the size, under 0.3 of it of room in `buf`.
        let top = y + h / 2 - size as i32 * 13 / 20;
        for ty in 0..bh {
            for tx2 in 0..room {
                p.blend(tx + tx2, top + ty, ink, buf[(ty * room + tx2) as usize] & 0xff);
            }
        }
    }
}

/// The smallest a cartridge, card or disc is drawn beside its case (screen pixels).
pub const GAME_MIN: i32 = 48;

/// What is inside a case: it comes out when the case is opened (`game_box`) and goes into the console when the game
/// starts (`app_cabinet.rs`, the launch animation).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Game {
    /// The Super Famicom's and the PAL Super Nintendo's cartridge (rounded top).
    Snes,
    /// The American Super Nintendo's cartridge (the grips at its sides): for a game whose file says USA or Canada.
    SnesUs,
    N64,
    Nes,
    Gb,
    Gbc,
    Gba,
    /// Any other cartridge.
    Cart,
    MegaDrive,
    MasterSystem,
    GameGear,
    /// The PC Engine's HuCard.
    HuCard,
    NeoGeoPocket,
    WonderSwan,
    Lynx,
    /// The small game card of the DS and the Vita.
    Card,
    /// The 3DS's game card (the tab on its side).
    Card3ds,
    /// A memory card: what a game without a medium (a port, a fantasy console's cartridge) comes on.
    Memory,
    /// A CD or DVD, painted round (`cabinet_art::disc`).
    Disc,
    /// The GameCube's 8 cm disc.
    MiniDisc,
    /// The PSP's UMD, painted round (`cabinet_art::umd`).
    Umd,
}

/// A cartridge's or card's template: its size and where its label is (template pixels), and whether it is drawn in grey
/// to take the system's plastic (`tint`) or a photo's own colours (`assets/media/game/SOURCES.md`).
struct GameTemplate {
    size: (i32, i32),
    label: (i32, i32, i32, i32),
    tint: bool,
}

// BEGIN GAME TABLE (assets/media/game/tools/table.py)
/// The games' pictures, in the order of `Game::DRAWN`.
#[rustfmt::skip]
const GAME_TEMPLATES: [GameTemplate; 14] = [
    // snes
    GameTemplate { size: (150, 98), label: (10, 2, 130, 46), tint: false },
    // snes-us
    GameTemplate { size: (152, 100), label: (33, 4, 87, 30), tint: false },
    // n64
    GameTemplate { size: (152, 98), label: (40, 6, 72, 70), tint: false },
    // nes
    GameTemplate { size: (136, 152), label: (54, 2, 63, 101), tint: false },
    // gb
    GameTemplate { size: (132, 150), label: (17, 42, 98, 87), tint: false },
    // gbc
    GameTemplate { size: (132, 150), label: (18, 48, 97, 87), tint: false },
    // gba
    GameTemplate { size: (152, 92), label: (19, 20, 116, 61), tint: false },
    // cart
    GameTemplate { size: (108, 135), label: (8, 10, 90, 54), tint: true },
    // md
    GameTemplate { size: (151, 108), label: (27, 43, 99, 48), tint: false },
    // sms
    GameTemplate { size: (152, 96), label: (3, 0, 146, 22), tint: false },
    // gg
    GameTemplate { size: (132, 128), label: (11, 21, 111, 76), tint: false },
    // hucard
    GameTemplate { size: (82, 126), label: (3, 11, 76, 112), tint: false },
    // ds
    GameTemplate { size: (151, 142), label: (3, 13, 138, 98), tint: false },
    // memory
    GameTemplate { size: (24, 32), label: (3, 8, 18, 21), tint: true },
];

const GAME_PNGS: [&[u8]; 14] = [
    include_bytes!("../assets/media/game/snes.png"),
    include_bytes!("../assets/media/game/snes-us.png"),
    include_bytes!("../assets/media/game/n64.png"),
    include_bytes!("../assets/media/game/nes.png"),
    include_bytes!("../assets/media/game/gb.png"),
    include_bytes!("../assets/media/game/gbc.png"),
    include_bytes!("../assets/media/game/gba.png"),
    include_bytes!("../assets/media/game/cart.png"),
    include_bytes!("../assets/media/game/md.png"),
    include_bytes!("../assets/media/game/sms.png"),
    include_bytes!("../assets/media/game/gg.png"),
    include_bytes!("../assets/media/game/hucard.png"),
    include_bytes!("../assets/media/game/ds.png"),
    include_bytes!("../assets/media/game/memory.png"),
];

impl Game {
    /// The ones with a picture of their own (the rest stand in for each other, `drawn`, or are painted round).
    const DRAWN: [Game; 14] = [Game::Snes, Game::SnesUs, Game::N64, Game::Nes, Game::Gb, Game::Gbc, Game::Gba, Game::Cart, Game::MegaDrive, Game::MasterSystem, Game::GameGear, Game::HuCard, Game::Card, Game::Memory];
}
// END GAME TABLE

impl Game {
    /// The template it is painted from: its own, or while it has none the one of the nearest kind.
    fn drawn(self) -> Option<usize> {
        let stand_in = match self {
            Game::SnesUs => Game::Snes,
            Game::Gbc => Game::Gb,
            Game::Card3ds => Game::Card,
            Game::MasterSystem | Game::GameGear | Game::HuCard | Game::NeoGeoPocket | Game::WonderSwan | Game::Lynx => Game::Cart,
            g => g,
        };
        Game::DRAWN.iter().position(|g| *g == self).or_else(|| Game::DRAWN.iter().position(|g| *g == stand_in))
    }

    /// What a game of `system` comes on, its file `rom` (a SNES game from the USA on the American cartridge): the
    /// system's own cartridge where one is drawn, else what its case holds (`Medium::game`).
    pub fn of(system: &str, rom: &str) -> Game {
        match system {
            "snes" | "snesh" | "sfc" | "satellaview" | "sufami" if rom.contains("(USA") || rom.contains("(Canada") => Game::SnesUs,
            "gbc" | "gbch" => Game::Gbc,
            "3ds" | "n3ds" => Game::Card3ds,
            "mastersystem" | "sg-1000" | "sg1000" => Game::MasterSystem,
            "gamegear" | "ggh" => Game::GameGear,
            "pcengine" | "tg16" | "supergrafx" => Game::HuCard,
            "ngp" | "ngpc" => Game::NeoGeoPocket,
            "wonderswan" | "wonderswancolor" => Game::WonderSwan,
            "atarilynx" | "lynx" => Game::Lynx,
            _ => Medium::of(system).game(),
        }
    }

    /// A disc or a UMD: painted round, not from a template.
    pub fn is_round(self) -> bool {
        matches!(self, Game::Disc | Game::MiniDisc | Game::Umd)
    }

    /// The real thing, width x height in millimetres (a disc's diameter).
    pub fn mm(self) -> (f32, f32) {
        match self {
            Game::Snes => (130.0, 86.0),
            Game::SnesUs => (130.0, 84.0),
            Game::N64 => (116.0, 75.0),
            Game::Nes => (120.0, 133.0),
            Game::Gb | Game::Gbc => (57.0, 65.0),
            Game::Gba => (57.0, 35.0),
            Game::Cart => (80.0, 100.0),
            Game::MegaDrive => (112.0, 84.0),
            Game::MasterSystem => (108.0, 69.0),
            Game::GameGear => (68.0, 68.0),
            Game::HuCard => (54.0, 85.0),
            Game::NeoGeoPocket => (40.0, 46.0),
            Game::WonderSwan => (56.0, 46.0),
            Game::Lynx => (80.0, 56.0),
            Game::Card | Game::Card3ds => (35.0, 33.0),
            Game::Memory => (24.0, 32.0),
            Game::Disc => (120.0, 120.0),
            Game::MiniDisc => (80.0, 80.0),
            Game::Umd => (64.0, 65.0),
        }
    }

    /// Its size in pixels beside its case `m` drawn `case_h` pixels high: the case's own millimetres to the pixel, so a
    /// Game Boy cartridge is small beside its box and a SNES one big; never under `GAME_MIN` pixels high, so a DS card
    /// still shows its label on a small screen.
    pub fn size_beside(self, m: Medium, case_h: i32) -> (i32, i32) {
        let px_mm = case_h as f32 / m.mm().1;
        let (w, h) = self.mm();
        let k = (px_mm * h).max(GAME_MIN as f32) / h;
        ((w * k).round() as i32, (h * k).round() as i32)
    }

    fn pic(self) -> Option<&'static Sprite> {
        static DECODED: [std::sync::OnceLock<Sprite>; GAME_PNGS.len()] = [const { std::sync::OnceLock::new() }; GAME_PNGS.len()];
        let i = self.drawn()?;
        Some(DECODED[i].get_or_init(|| crate::system_art::decode(GAME_PNGS[i]).unwrap_or(Sprite { w: 1, h: 1, px: vec![0] })))
    }
}

/// The game itself, `h` pixels high: the cartridge or card in its system's plastic with the cover on its label
/// (`cover` is fitted to it here), or the title and the console on a plain label. None for a disc or a UMD, which are
/// painted round (`cabinet_art::disc`).
pub fn game_pic(text: &mut Text, g: Game, h: i32, look: Look, title: &str, cover: Option<&Image>, console: Option<&Sprite>) -> Option<Pic> {
    let i = g.drawn()?;
    let sprite = g.pic()?;
    let f = h as f32 / GAME_TEMPLATES[i].size.1 as f32;
    let mut p = scaled(sprite, f, GAME_TEMPLATES[i].tint.then_some(look.plastic));
    let (lx, ly, lw, lh) = scale_rect(GAME_TEMPLATES[i].label, f);
    let (lw, lh) = (lw.min(p.w - lx), lh.min(p.h - ly));
    // Only the label's own flat grey is printed on: its rounded corners (a GBA's, the top of an N64's) stay plastic.
    let flat = p.px[((ly + lh / 2) * p.w + lx + lw / 2) as usize];
    let shell: Vec<(usize, u32)> = (ly..ly + lh)
        .flat_map(|y| (lx..lx + lw).map(move |x| (y * p.w + x) as usize))
        .filter(|&i| p.px[i] != flat)
        .map(|i| (i, p.px[i]))
        .collect();
    match cover {
        Some(img) if img.w > 0 && img.h > 0 => {
            // A label shows the artwork, not the box's printed side strip (a SNES box's black band, a GBA box's
            // silver one, a US N64 box's red one) or the margin a case's front left round it.
            // A cartridge's label is printed edge to edge: the artwork fills it, cut at its sides or top and bottom.
            let art = label_part(&trim_system_band(&trim_bands(img), g), g, lw, lh);
            let fitted = art::cover_fill(&art, lw, lh);
            for y in 0..lh.min(fitted.h as i32) {
                for x in 0..lw.min(fitted.w as i32) {
                    p.px[((ly + y) * p.w + lx + x) as usize] = 0xff00_0000 | fitted.px[(y as u32 * fitted.w + x as u32) as usize];
                }
            }
        }
        // A label too small for a title (a game card at the shelf's size): the cover's colour only.
        _ if lh < 32 || lw < 40 => paper(&mut p, (lx, ly, lw, lh), look.color),
        _ => {
            let _ = console;
            label_title(text, &mut p, (lx, ly, lw, lh), look, title);
        }
    }
    for (i, c) in shell {
        p.px[i] = c;
    }
    Some(p)
}

/// The sizes a cartridge's label title is written in, smallest first.
const LABEL_PX: [u32; 9] = [14, 16, 18, 21, 24, 28, 30, 37, 44];

/// A cartridge's label without a cover: the cover's colour, the title in the middle, as large as fits (a quarter of the
/// label's height at most), on up to three lines of whole words; else without its subtitle; only then cut with "…".
fn label_title(text: &mut Text, p: &mut Pic, (x, y, w, h): (i32, i32, i32, i32), look: Look, title: &str) {
    for row in 0..h {
        let c = mix(look.color, 0x000000, 20 + (row as u32 * 90 / h.max(1) as u32));
        p.fill(x, y + row, w, 1, 0xff00_0000 | c);
    }
    write_title(text, p, (x, y, w, h), look, title, 4);
}

/// The title written into `(x, y, w, h)` of `p` as `label_title` does, over what is painted there, at most a
/// `per_h`th of the height in size.
fn write_title(text: &mut Text, p: &mut Pic, (x, y, w, h): (i32, i32, i32, i32), look: Look, title: &str, per_h: i32) {
    let _fixed = crate::type_scale::fixed();
    // A shortened title is still a game's name, even when only its full name was learned by i18n. Keep every
    // measurement and wrap untranslated too: otherwise the fallback can turn "Black: ..." into "Schwarz".
    let translating = text.translating();
    text.set_translate(false);
    let face = crate::text::Face::Title;
    let max_w = w - (w / 8).max(4);
    let step = |s: u32| s as i32 * 11 / 10;
    let mut tries = vec![title];
    tries.extend(main_title(title).map(|(main, _)| main));
    let sizes: Vec<u32> = LABEL_PX.into_iter().rev().filter(|&s| s as i32 * per_h <= h.max(56)).collect();
    let fits = |text: &mut Text, t: &str, s: u32| {
        let lines = text.wrap(face, s, t, max_w);
        let whole = lines.join(" ") == t.split_whitespace().collect::<Vec<_>>().join(" ");
        (whole && lines.len() <= 3 && lines.len() as i32 * step(s) <= h - 4).then_some(lines)
    };
    let found = tries.iter().find_map(|t| sizes.iter().find_map(|&s| fits(text, t, s).map(|l| (l, s))));
    let (lines, px) = found.unwrap_or_else(|| {
        let s = *sizes.last().unwrap_or(&LABEL_PX[0]);
        let rows = ((h - 4) / step(s)).clamp(1, 3) as usize;
        (text.wrap_at_most(face, s, tries.last().copied().unwrap_or(title), max_w, rows), s)
    });
    let ink = ink_on(mix(look.color, 0x000000, 60));
    let block = lines.len() as i32 * step(px);
    let mut base = y + (h - block) / 2 + px as i32;
    for line in &lines {
        let lw = text.width(face, px, line);
        let bh = px as i32 * 3 / 2;
        let mut buf = vec![0u32; (w * bh) as usize];
        {
            let mut c = crate::gfx::Canvas::new(&mut buf, w as u32, bh as u32);
            text.draw(&mut c, face, px, (w - lw) / 2, px as i32, 0xffffff, line);
        }
        for ty in 0..bh {
            for tx in 0..w {
                p.blend(x + tx, base - px as i32 + ty, ink, buf[(ty * w + tx) as usize] & 0xff);
            }
        }
        base += step(px);
    }
    text.set_translate(translating);
}

/// The printed side of a system's disc, or the PSP's UMD: a pixel-art template from a photo of the real one
/// (`assets/media/disc/SOURCES.md`), every logo painted out, its print one flat magenta (`PRINT`) that `disc_pic`
/// fills with the game's cover and title.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum DiscKind {
    /// The PlayStation's black disc.
    Ps1,
    Ps2,
    /// The 8 cm mini disc.
    GameCube,
    Wii,
    /// The GD-ROM with its ring.
    Dreamcast,
    Saturn,
    Xbox,
    /// A plain silver CD: every other disc system.
    Cd,
    /// The UMD's shell, its label on the front.
    Umd,
}

/// The colour of a disc template's print (`DiscKind`).
const PRINT: u32 = 0xff00ff;

const DISC_PNGS: [&[u8]; 9] = [
    include_bytes!("../assets/media/disc/ps1.png"),
    include_bytes!("../assets/media/disc/ps2.png"),
    include_bytes!("../assets/media/disc/gamecube.png"),
    include_bytes!("../assets/media/disc/wii.png"),
    include_bytes!("../assets/media/disc/dreamcast.png"),
    include_bytes!("../assets/media/disc/saturn.png"),
    include_bytes!("../assets/media/disc/xbox.png"),
    include_bytes!("../assets/media/disc/cd.png"),
    include_bytes!("../assets/media/disc/umd.png"),
];

/// A decoded disc template and where its print is, in template pixels: the rectangle round it, and for a disc its
/// inner and outer radius from the middle.
struct DiscTemplate {
    sprite: Sprite,
    print: (i32, i32, i32, i32),
    /// The print's middle and its inner and outer edge (a UMD's label has no hole: inner 0).
    centre: (f32, f32),
    radii: (f32, f32),
    /// A disc's centre hole and outer edge, from the middle of the picture.
    hole: f32,
    edge: f32,
    /// A disc's colours outside its print by distance from its middle, in steps of `PROFILE_STEP` template pixels:
    /// its hub rings and rim drawn as true circles at any size.
    profile: Vec<u32>,
}

/// The step of `DiscTemplate::profile`, in template pixels.
const PROFILE_STEP: f32 = 0.25;

impl DiscKind {
    pub const ALL: [DiscKind; 9] = [DiscKind::Ps1, DiscKind::Ps2, DiscKind::GameCube, DiscKind::Wii, DiscKind::Dreamcast, DiscKind::Saturn, DiscKind::Xbox, DiscKind::Cd, DiscKind::Umd];

    /// The disc a game of `system` comes on.
    pub fn of(system: &str) -> DiscKind {
        match system {
            "psx" | "ps1" | "playstation" => DiscKind::Ps1,
            "ps2" => DiscKind::Ps2,
            "gamecube" | "gc" => DiscKind::GameCube,
            "wii" => DiscKind::Wii,
            "dreamcast" => DiscKind::Dreamcast,
            "saturn" => DiscKind::Saturn,
            "xbox" => DiscKind::Xbox,
            "psp" => DiscKind::Umd,
            _ => DiscKind::Cd,
        }
    }

    /// Decoded once (`warm`, on the cabinet's reader thread), its print found.
    fn template(self) -> &'static DiscTemplate {
        static DECODED: [std::sync::OnceLock<DiscTemplate>; 9] = [const { std::sync::OnceLock::new() }; 9];
        let i = DiscKind::ALL.iter().position(|k| *k == self).unwrap_or(0);
        DECODED[i].get_or_init(|| {
            let sprite = crate::system_art::decode(DISC_PNGS[i]).unwrap_or(Sprite { w: 1, h: 1, px: vec![0] });
            let (w, h) = (sprite.w as i32, sprite.h as i32);
            let (cx, cy) = (w as f32 / 2.0, h as f32 / 2.0);
            let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0, 0);
            let (mut sx, mut sy, mut n) = (0f32, 0f32, 0f32);
            let (mut hole, mut edge) = (f32::MAX, 0f32);
            let dist = |x: i32, y: i32, (cx, cy): (f32, f32)| ((x as f32 + 0.5 - cx).powi(2) + (y as f32 + 0.5 - cy).powi(2)).sqrt();
            for y in 0..h {
                for x in 0..w {
                    let px = sprite.px[(y * w + x) as usize];
                    if px >> 24 == 0 {
                        continue;
                    }
                    let d = dist(x, y, (cx, cy));
                    (hole, edge) = (hole.min(d), edge.max(d));
                    if px & 0xffffff == PRINT {
                        (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x + 1), y1.max(y + 1));
                        (sx, sy, n) = (sx + x as f32 + 0.5, sy + y as f32 + 0.5, n + 1.0);
                    }
                }
            }
            // A round print's middle is the disc's; a label's (the UMD's) where its pixels are.
            let centre = if self.is_round() || n == 0.0 { (cx, cy) } else { (sx / n, sy / n) };
            let (mut r0, mut r1) = (f32::MAX, 0f32);
            for y in y0..y1 {
                for x in x0..x1 {
                    if sprite.px[(y * w + x) as usize] == 0xff00_0000 | PRINT {
                        let d = dist(x, y, centre);
                        (r0, r1) = (r0.min(d), r1.max(d));
                    }
                }
            }
            let print = if x1 > x0 { (x0, y0, x1 - x0, y1 - y0) } else { (0, 0, 0, 0) };
            // Edges half a pixel past the outermost pixel centres; a print reaching its middle has no inner edge.
            let r0 = if r0 < 1.5 { 0.0 } else { r0 - 0.5 };
            // The colours by distance: every solid pixel outside the print in the step its centre falls in; a step
            // no pixel falls in takes its nearest neighbour's.
            let steps = (edge / PROFILE_STEP) as usize + 2;
            let mut sum = vec![[0u32; 4]; steps];
            for y in 0..h {
                for x in 0..w {
                    let px = sprite.px[(y * w + x) as usize];
                    if px >> 24 != 0 && px & 0xffffff != PRINT {
                        let b = &mut sum[((dist(x, y, (cx, cy)) / PROFILE_STEP) as usize).min(steps - 1)];
                        (b[0], b[1], b[2], b[3]) = (b[0] + (px >> 16 & 0xff), b[1] + (px >> 8 & 0xff), b[2] + (px & 0xff), b[3] + 1);
                    }
                }
            }
            let filled: Vec<usize> = (0..steps).filter(|&i| sum[i][3] > 0).collect();
            let profile = (0..steps)
                .map(|i| {
                    let Some(&j) = filled.iter().min_by_key(|&&j| j.abs_diff(i)) else { return 0 };
                    let b = sum[j];
                    (b[0] / b[3]) << 16 | (b[1] / b[3]) << 8 | b[2] / b[3]
                })
                .collect();
            DiscTemplate { sprite, print, centre, radii: (r0.min(r1), r1 + 0.5), hole: (hole - 0.5).max(0.0), edge: edge + 0.5, profile }
        })
    }

    fn is_round(self) -> bool {
        self != DiscKind::Umd
    }

    /// The ring at the rim where it is the system's own: the PlayStation's black disc showing past its label, the
    /// PS2's blue, the Dreamcast's silver GD-ROM area, the Wii's white disc, the dark rims of the Saturn's and the
    /// Xbox's labels; the rest take the cover's colour.
    fn ring(self) -> Option<u32> {
        match self {
            DiscKind::Ps1 => Some(0x0e0e10),
            DiscKind::Ps2 => Some(0x26408c),
            DiscKind::Dreamcast => Some(0xc8ccd2),
            DiscKind::Wii => Some(0xf0f0ee),
            DiscKind::Saturn => Some(0x1c1c22),
            DiscKind::Xbox => Some(0x16201a),
            _ => None,
        }
    }

    /// Where the system's covers carry its printed strip with the console's name and logo (as `band_zones`), which a
    /// disc does not: the PlayStation's black strip at the left, the band at the top of a PS2, GameCube, Wii, Xbox
    /// and PSP cover, a Saturn's strip at the left or top, a Dreamcast's band at the top, a Sega CD's at the left.
    fn band_zones(self) -> &'static [(u8, f32, f32, Option<f32>)] {
        match self {
            DiscKind::Ps1 => &[(0, 0.06, 0.25, Some(0.13))],
            DiscKind::Ps2 | DiscKind::GameCube | DiscKind::Umd => &[(2, 0.05, 0.2, Some(0.11))],
            DiscKind::Wii | DiscKind::Xbox => &[(2, 0.05, 0.2, Some(0.12))],
            DiscKind::Saturn => &[(0, 0.05, 0.22, None), (2, 0.05, 0.2, None)],
            DiscKind::Dreamcast => &[(2, 0.05, 0.2, None)],
            // A Sega CD's strip at the left (cut where it shows, other CD systems' covers have none).
            DiscKind::Cd => &[(0, 0.05, 0.2, None)],
        }
    }
}

/// The game's disc (or UMD) `h` pixels high, printed as a real one is: the cover's art fills the print, cut round by
/// the disc's own shape, and a clean ring at its rim carries the title along its lower arc where the disc is large
/// enough to read it. Without a cover the print is the cover's colour with the title above the hole. Made once per
/// case and size (`App::box_game`, the launch's first frame), never per frame.
pub fn disc_pic(text: &mut Text, kind: DiscKind, h: i32, look: Look, title: &str, cover: Option<&Image>) -> Pic {
    // Made at the size it is shown: the template's colours sampled smoothly between its pixels, every edge (the rim,
    // the hole, the print, the ring) worked out as a circle and anti-aliased, the cover and the title at full
    // resolution. A pixel-art template scaled by whole pixels looked blocky beside the smooth case and cover.
    let t = kind.template();
    let s = &t.sprite;
    let f = h as f32 / s.h.max(1) as f32;
    let w = ((s.w as f32 * f).round() as i32).max(1);
    let mut p = Pic::new(w, h, 0);
    let (bx, by, bw, bh) = scale_rect(t.print, f);
    let (bx, by) = (bx.clamp(0, p.w), by.clamp(0, p.h));
    let (bw, bh) = (bw.min(p.w - bx).max(0), bh.min(p.h - by).max(0));
    let (cx, cy) = (s.w as f32 * f / 2.0, s.h as f32 * f / 2.0);
    let (pcx, pcy) = (t.centre.0 * f, t.centre.1 * f);
    let (r_in, r_out) = (t.radii.0 * f, t.radii.1 * f);
    let (r_hole, r_edge) = (t.hole * f, t.edge * f);
    // How much of a pixel `d` from a centre lies inside a circle of radius `r`.
    let inside = |d: f32, r: f32| (r - d + 0.5).clamp(0.0, 1.0);
    let cover = cover.filter(|c| c.w > 0 && c.h > 0);
    // The ring at the rim, in the cover's colour darkened: an eighth of the disc's radius, never more than a third of
    // the print (the GameCube's hub is large).
    let ring = kind.is_round().then(|| r_out - (r_out * 0.13).min((r_out - r_in) / 3.0)).filter(|r| r_out - r > 3.0);
    let ring_c = kind.ring().unwrap_or(mix(look.color, 0x000000, 110));
    let fitted = cover.map(|img| art::cover_fill(&trim_zones(&trim_bands(img), kind.band_zones()), bw.max(1), bh.max(1)));
    let (sw, sh) = (s.w as i32, s.h as i32);
    let k = |v: f32| (v.clamp(0.0, 1.0) * 255.0) as u32;
    for y in 0..h {
        for x in 0..w {
            let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
            // The template's colour here: the solid pixels round the spot that are not print, weighted by nearness.
            let (u, v) = (fx / f - 0.5, fy / f - 0.5);
            let (u0, v0) = (u.floor() as i32, v.floor() as i32);
            let (du, dv) = (u - u0 as f32, v - v0 as f32);
            let (mut acc, mut wsum, mut alpha) = ([0f32; 3], 0f32, 0f32);
            for (ox, oy, wt) in [(0, 0, (1.0 - du) * (1.0 - dv)), (1, 0, du * (1.0 - dv)), (0, 1, (1.0 - du) * dv), (1, 1, du * dv)] {
                let (sx, sy) = ((u0 + ox).clamp(0, sw - 1), (v0 + oy).clamp(0, sh - 1));
                let q = s.px[(sy * sw + sx) as usize];
                if q >> 24 == 0 {
                    continue;
                }
                alpha += wt;
                if q & 0xffffff != PRINT {
                    for (c, sh) in acc.iter_mut().zip([16, 8, 0]) {
                        *c += wt * (q >> sh & 0xff) as f32;
                    }
                    wsum += wt;
                }
            }
            let d = ((fx - cx).powi(2) + (fy - cy).powi(2)).sqrt();
            let a = if kind.is_round() { inside(d, r_edge) * (1.0 - inside(d, r_hole)) } else { alpha };
            if a <= 0.0 {
                continue;
            }
            // How much of it is print, and what is printed there.
            let dp = ((fx - pcx).powi(2) + (fy - pcy).powi(2)).sqrt();
            let pc = inside(dp, r_out) * if r_in > 0.0 { 1.0 - inside(dp, r_in) } else { 1.0 };
            let printed = (pc > 0.0).then(|| {
                let (ax, ay) = ((x - bx).clamp(0, bw.max(1) - 1), (y - by).clamp(0, bh.max(1) - 1));
                let mut c = match &fitted {
                    Some(f) => f.px[(ay.min(f.h as i32 - 1) as u32 * f.w + ax.min(f.w as i32 - 1) as u32) as usize] & 0xffffff,
                    // Without a cover: the cover's colour, darker down the disc.
                    None => mix(look.color, 0x000000, 20 + (ay as u32 * 90 / bh.max(1) as u32)),
                };
                if let Some(r) = ring {
                    c = mix(c, ring_c, k(dp - r + 0.5));
                    // A darker line where the ring begins; the GD-ROM's line between its two areas, halfway out.
                    c = mix(c, mix(ring_c, 0x000000, 90), k(1.0 - (dp - r - 0.5).abs()));
                    if kind == DiscKind::Dreamcast {
                        c = mix(c, 0x8a8e96, k(1.25 - (dp - (r + r_out) / 2.0).abs()));
                    }
                }
                c
            });
            let base = if kind.is_round() {
                // A disc: its colour at this distance (`DiscTemplate::profile`), between two steps.
                let at = (d / f / PROFILE_STEP - 0.5).max(0.0);
                let i = (at as usize).min(t.profile.len() - 1);
                let j = (i + 1).min(t.profile.len() - 1);
                mix(t.profile[i], t.profile[j], k(at - i as f32))
            } else if wsum > 0.0 {
                let ch = |i: usize| ((acc[i] / wsum).round() as u32).min(255);
                ch(0) << 16 | ch(1) << 8 | ch(2)
            } else {
                printed.unwrap_or(0)
            };
            let rgb = match printed {
                Some(c) => mix(base, c, k(pc)),
                None => base,
            };
            p.px[(y * w + x) as usize] = k(a) << 24 | rgb;
        }
    }
    match (cover, ring) {
        (Some(_), Some(r)) => ring_title(text, &mut p, (cx, cy), (r + 1.0, r_out), ink_on(ring_c), title),
        (Some(_), None) => {}
        // No cover: the title above the hole, where the print is wide enough for it; on a UMD's label.
        (None, Some(r)) => {
            let top = cy - r * 0.8;
            let half = (r * r - (cy - top).powi(2)).max(0.0).sqrt() * 0.95;
            let room = ((cx - half) as i32, top as i32, (2.0 * half) as i32, (cy - r_in - 3.0 - top) as i32);
            if room.3 >= 24 && room.2 >= 40 {
                write_title(text, &mut p, room, look, title, 2);
            }
        }
        (None, None) if !kind.is_round() => write_title(text, &mut p, (bx, by, bw, bh), look, title, 4),
        (None, None) => {}
    }
    p
}

/// The smallest a title is written along a disc's ring (screen pixels): smaller, the ring stays plain.
const RING_PX: u32 = 12;

/// `title` along the lower arc of the ring from radius `r0` to `r1` round `c`, upright, in the middle of it; as large
/// as the ring allows, without its subtitle when it is too long for the arc, only then cut with "…".
fn ring_title(text: &mut Text, p: &mut Pic, (cx, cy): (f32, f32), (r0, r1): (f32, f32), ink: u32, title: &str) {
    let px = ((r1 - r0) * 0.78) as u32;
    if px < RING_PX {
        return;
    }
    let _fixed = crate::type_scale::fixed();
    let translating = text.translating();
    text.set_translate(false);
    let face = crate::text::Face::Title;
    let mid = (r0 + r1) / 2.0;
    // Up to 70 degrees either side of the bottom.
    let max_w = (mid * 2.0 * 70f32.to_radians()) as i32;
    let line = std::iter::once(title)
        .chain(main_title(title).map(|(main, _)| main))
        .find(|t| text.width(face, px, t) <= max_w)
        .map(str::to_string)
        .unwrap_or_else(|| text.fit(face, px, main_title(title).map_or(title, |(m, _)| m), max_w));
    let tw = text.width(face, px, &line).max(1);
    let bh = px as i32 * 3 / 2;
    let mut buf = vec![0u32; (tw * bh) as usize];
    {
        let mut c = crate::gfx::Canvas::new(&mut buf, tw as u32, bh as u32);
        text.draw(&mut c, face, px, 0, px as i32, 0xffffff, &line);
    }
    text.set_translate(translating);
    // The text's middle (its capitals' middle) on the ring's middle; its top towards the disc's middle.
    let top = mid - px as f32 * 0.62;
    let alpha = |x: f32, y: f32| {
        let (x0, y0) = (x.floor() as i32, y.floor() as i32);
        let (fx, fy) = (x - x0 as f32, y - y0 as f32);
        let at = |x: i32, y: i32| if x < 0 || y < 0 || x >= tw || y >= bh { 0.0 } else { (buf[(y * tw + x) as usize] & 0xff) as f32 };
        at(x0, y0) * (1.0 - fx) * (1.0 - fy) + at(x0 + 1, y0) * fx * (1.0 - fy) + at(x0, y0 + 1) * (1.0 - fx) * fy + at(x0 + 1, y0 + 1) * fx * fy
    };
    let (y_from, y_to) = ((cy + r0 * 0.3) as i32, ((cy + r1).ceil() as i32).min(p.h));
    let (x_from, x_to) = (((cx - r1).floor() as i32).max(0), ((cx + r1).ceil() as i32).min(p.w));
    for y in y_from.max(0)..y_to {
        for x in x_from..x_to {
            let (dx, dy) = (x as f32 + 0.5 - cx, y as f32 + 0.5 - cy);
            let d = (dx * dx + dy * dy).sqrt();
            if d < r0 || d > r1 {
                continue;
            }
            // Along the arc from left to right; at the bottom the angle is 0.
            let s = (-dx).atan2(dy) * -mid + tw as f32 / 2.0;
            let a = alpha(s - 0.5, d - top - 0.5);
            if a >= 1.0 {
                p.blend(x, y, ink, a as u32);
            }
        }
    }
}

/// Where a system's box art has its printed side strip, as a part of the picture's width or height to look in: the
/// left edge of a Game Boy (Color) or GBA box ("GAME BOY ADVANCE" up its side), the right one of an American SNES box (the
/// black band), the left one or the top of a Genesis box (the red "GENESIS" strip, or the black band over the art).
/// (side, from, to, usual): side 0 left, 1 right, 2 top; `usual` is where to cut when no edge shows (a dark strip
/// running into dark art), for the boxes that always carry the strip.
fn band_zones(g: Game) -> &'static [(u8, f32, f32, Option<f32>)] {
    match g {
        Game::Gb | Game::Gbc => &[(0, 0.06, 0.30, Some(0.20))],
        // The strip is a fifth of the box's width; its GAME BOY ADVANCE lettering's straight edge (an eighth in) is not it.
        Game::Gba => &[(0, 0.16, 0.30, Some(0.22))],
        Game::SnesUs => &[(1, 0.06, 0.30, Some(0.20))],
        Game::Snes => &[(1, 0.06, 0.30, None)],
        // An American N64 box's red strip with the logo down its right side (a box without it keeps its art).
        Game::N64 => &[(1, 0.06, 0.30, None)],
        Game::MegaDrive => &[(0, 0.05, 0.25, None), (2, 0.05, 0.22, None)],
        _ => &[],
    }
}

/// The part of the box art a label `w` x `h` shows: a GBA cartridge's wide label the top of the art, where its title
/// is (as on the real labels), not a band through the middle; every other label the middle (`cabinet_art::cover_fill`).
fn label_part(img: &Image, g: Game, w: i32, h: i32) -> Image {
    let rows = (img.w as i64 * h.max(1) as i64 / w.max(1) as i64) as u32;
    if g != Game::Gba || rows == 0 || rows >= img.h {
        return img.clone();
    }
    Image { w: img.w, h: rows, px: img.px[..(img.w * rows) as usize].to_vec() }
}

/// `img` without its system's printed side strip (`band_zones`): cut at the straightest edge across the picture in the
/// strip's zone, when there is one (a European or Japanese box without the strip keeps its art whole).
pub fn trim_system_band(img: &Image, g: Game) -> Image {
    trim_zones(img, band_zones(g))
}

/// `img` cut at the straightest edge in each of `zones` (`band_zones`).
fn trim_zones(img: &Image, zones: &[(u8, f32, f32, Option<f32>)]) -> Image {
    let (w, h) = (img.w as i32, img.h as i32);
    if w < 24 || h < 24 {
        return img.clone();
    }
    let px = |x: i32, y: i32| img.px[(y * w + x) as usize];
    let d = |a: u32, b: u32| {
        let c = |s: u32| ((a >> s & 0xff) as i32 - (b >> s & 0xff) as i32).abs();
        c(16) + c(8) + c(0)
    };
    // How much of the picture's height (width) a straight edge at column (row) `i` runs along: the step between two
    // pixels apart (a strip's edge is often a pixel of blend).
    let col = |x: i32| (0..h).filter(|&y| d(px(x - 2, y), px(x, y)) > 70).count() as f32 / h as f32;
    let row = |y: i32| (0..w).filter(|&x| d(px(x, y - 2), px(x, y)) > 70).count() as f32 / w as f32;
    let (mut left, mut right, mut top) = (0, w, 0);
    for &(side, from, to, usual) in zones {
        let len = if side == 2 { h } else { w };
        let (a, b) = (((len as f32 * from) as i32).max(2), (len as f32 * to) as i32);
        let best = (a..b)
            .map(|i| if side == 1 { len - i } else { i })
            .map(|i| (i, if side == 2 { row(i) } else { col(i) }))
            .max_by(|p, q| p.1.total_cmp(&q.1));
        let at = match (best, usual) {
            (Some((i, score)), _) if score > 0.35 => Some(i),
            (_, Some(f)) => Some(if side == 1 { len - (len as f32 * f) as i32 } else { (len as f32 * f) as i32 }),
            _ => None,
        };
        match (side, at) {
            (0, Some(i)) => left = i,
            (1, Some(i)) => right = i,
            (_, Some(i)) => top = i,
            _ => {}
        }
    }
    if (left, right, top) == (0, w, 0) {
        return img.clone();
    }
    let (nw, nh) = (right - left, h - top);
    let mut out = Vec::with_capacity((nw * nh) as usize);
    for y in top..h {
        out.extend_from_slice(&img.px[(y * w + left) as usize..(y * w + right) as usize]);
    }
    Image { w: nw as u32, h: nh as u32, px: out }
}

/// `img` without the plain margins round its artwork: the even colour a case's front leaves round a cover of another
/// shape (`cabinet_art::cover_fit`), a scan's white border. A margin row or column is nearly one colour along its length.
pub fn trim_bands(img: &Image) -> Image {
    let (w, h) = (img.w as i32, img.h as i32);
    if w < 24 || h < 24 {
        return img.clone();
    }
    let px = |x: i32, y: i32| img.px[(y * w + x) as usize];
    let d = |a: u32, b: u32| {
        let c = |s: u32| ((a >> s & 0xff) as i32 - (b >> s & 0xff) as i32).abs();
        c(16) + c(8) + c(0)
    };
    // Nearly every pixel of the line within a small step of its first one.
    let even_col = |x: i32| (0..h).filter(|&y| d(px(x, y), px(x, 0)) <= 24).count() * 50 >= h as usize * 49;
    let even_row = |y: i32| (0..w).filter(|&x| d(px(x, y), px(0, y)) <= 24).count() * 50 >= w as usize * 49;
    let mut left = 0;
    while left < w / 3 && even_col(left) {
        left += 1;
    }
    let mut right = w;
    while right > w - w / 3 && even_col(right - 1) {
        right -= 1;
    }
    let mut top = 0;
    while top < h / 3 && even_row(top) {
        top += 1;
    }
    let mut bottom = h;
    while bottom > h - h / 3 && even_row(bottom - 1) {
        bottom -= 1;
    }
    let (nw, nh) = (right - left, bottom - top);
    if nw <= 0 || nh <= 0 || (nw == w && nh == h) {
        return img.clone();
    }
    let mut out = Vec::with_capacity((nw * nh) as usize);
    for y in top..bottom {
        out.extend_from_slice(&img.px[(y * w + left) as usize..(y * w + right) as usize]);
    }
    Image { w: nw as u32, h: nh as u32, px: out }
}

/// The cover printed on a front made at `k` times the shelf size, as a picture of its own (to put on the game inside).
pub fn cover_of(front: &Pic, m: Medium, k: f32) -> Image {
    let (x, y, w, h) = label_rect(m, k);
    let (w, h) = (w.min(front.w - x).max(1), h.min(front.h - y).max(1));
    let mut px = Vec::with_capacity((w * h) as usize);
    for row in y..y + h {
        px.extend(front.px[(row * front.w + x) as usize..][..w as usize].iter().map(|p| p & 0xffffff));
    }
    Image { w: w as u32, h: h as u32, px }
}

/// The two colours of a cover: the one it is mostly (`cabinet::spine_color`) and an accent, a clear colour of the
/// cover that is far from the first (the red of a logo on a blue cover); the first again when there is none.
pub fn cover_colors(px: &[u32]) -> Option<(u32, u32)> {
    let main = crate::cabinet::spine_color(px)?;
    let mut count = vec![0u32; 512];
    let mut sum = vec![[0u32; 3]; 512];
    let every = (px.len() / 40_000).max(1);
    let mut n = 0u32;
    for &p in px.iter().step_by(every) {
        let (r, g, b) = (p >> 16 & 0xff, p >> 8 & 0xff, p & 0xff);
        let k = ((r >> 5) << 6 | (g >> 5) << 3 | b >> 5) as usize;
        count[k] += 1;
        sum[k][0] += r;
        sum[k][1] += g;
        sum[k][2] += b;
        n += 1;
    }
    let dist = |a: u32, b: u32| {
        let d = |s: u32| (a >> s & 0xff) as i32 - (b >> s & 0xff) as i32;
        ((d(16).pow(2) + d(8).pow(2) + d(0).pow(2)) as f32).sqrt()
    };
    let accent = (0..512)
        // At least 1.5 % of the cover: a colour you see, not a stray pixel.
        .filter(|&k| count[k] * 66 >= n.max(1))
        .map(|k| {
            let c = count[k].max(1);
            let (r, g, b) = (sum[k][0] / c, sum[k][1] / c, sum[k][2] / c);
            let rgb = (r << 16) | (g << 8) | b;
            let (hi, lo) = (r.max(g).max(b), r.min(g).min(b));
            let sat = (hi - lo) as f32 / hi.max(1) as f32;
            let score = sat * dist(rgb, main).min(200.0) * (count[k] as f32).sqrt() * if (40..235).contains(&hi) { 1.0 } else { 0.3 };
            (rgb, score)
        })
        .filter(|(rgb, _)| dist(*rgb, main) > 70.0)
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .map_or(main, |(rgb, _)| rgb);
    Some((main, accent))
}

/// A lighter or darker shade of `c` that stands apart from `bg` (a band in the cover's own colour, say).
pub fn apart(c: u32, bg: u32) -> u32 {
    if luma(c).abs_diff(luma(bg)) > 40 {
        c
    } else if luma(bg) > 128 {
        mix(c, 0x000000, 110)
    } else {
        mix(c, 0xffffff, 90)
    }
}

/// The few things that stand on the Cabinet's shelves beside the games, at most two in view (`app_cabinet.rs`):
/// pixel art from PixelLab (`assets/media/props`, how each was made in its `pixellab-assets.json`).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Prop {
    Plant,
    Lamp,
    Cactus,
    /// Stands right after the last case of a shelf, holding the row up.
    Bookend,
}

impl Prop {
    pub const ALL: [Prop; 4] = [Prop::Plant, Prop::Lamp, Prop::Cactus, Prop::Bookend];

    /// The picture at the Cabinet's size (`SHELF_SCALE` screen pixels a pixel), decoded on first use.
    pub fn pic(self) -> &'static Pic {
        static DECODED: [std::sync::OnceLock<Pic>; 4] = [const { std::sync::OnceLock::new() }; 4];
        let i = Prop::ALL.iter().position(|p| *p == self).unwrap_or(0);
        DECODED[i].get_or_init(|| {
            let png: &[u8] = match self {
                Prop::Plant => include_bytes!("../assets/media/props/plant.png"),
                Prop::Lamp => include_bytes!("../assets/media/props/lamp.png"),
                Prop::Cactus => include_bytes!("../assets/media/props/cactus.png"),
                Prop::Bookend => include_bytes!("../assets/media/props/bookend.png"),
            };
            crate::system_art::decode(png).map_or_else(|| Pic::new(1, 1, 0), |s| scaled(&s, SHELF_SCALE, None))
        })
    }

    /// The prop that stands further along a shelf with room, after its bookend: a plant, a lamp or a cactus, picked by a
    /// number the shelf keeps (the same one every time), the tallest that fits `room_h` if the pick doesn't.
    pub fn beside(seed: u32, room_h: i32) -> Option<Prop> {
        let h = seed.wrapping_mul(2654435761) >> 7;
        let pick = [Prop::Plant, Prop::Lamp, Prop::Cactus, Prop::Plant][(h % 4) as usize];
        [pick, Prop::Plant, Prop::Cactus].into_iter().find(|p| p.pic().h <= room_h)
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_template_is_the_size_its_table_says_and_its_rectangles_are_inside() {
        for m in Medium::ALL {
            let t = m.template();
            let (s, f) = m.pics();
            assert_eq!((s.w as i32, s.h as i32), (t.spine.0 * t.res, t.spine.1 * t.res), "{m:?} spine");
            assert_eq!((f.w as i32, f.h as i32), (t.front.0 * t.res, t.front.1 * t.res), "{m:?} front");
            let x2 = |(w, h): (i32, i32)| (w * t.res, h * t.res);
            for (what, (x, y, w, h), (pw, ph)) in [("spine label", t.spine_label, x2(t.spine)), ("front label", t.front_label, x2(t.front))]
                .into_iter()
                .chain(t.band.map(|b| ("band", b, x2(t.spine))))
            {
                assert!(x >= 0 && y >= 0 && w > 0 && h > 0 && x + w <= pw && y + h <= ph, "{m:?} {what}");
            }
            assert!(t.caps.0 + t.caps.1 < t.spine.1, "{m:?} caps");
        }
    }

    #[test]
    fn systems_get_their_own_media_and_the_rest_a_generic_one() {
        for (system, want) in [
            ("snes", Medium::Snes),
            ("sfc", Medium::Snes),
            ("n64", Medium::N64),
            ("famicom", Medium::Nes),
            ("gbc", Medium::Gb),
            ("gba", Medium::Gba),
            ("genesis", Medium::MegaDrive),
            ("gamegear", Medium::MasterSystem),
            ("psx", Medium::Ps1),
            ("dreamcast", Medium::SegaDisc),
            ("saturn", Medium::SegaDisc),
            ("ps2", Medium::Ps2),
            ("gamecube", Medium::GameCube),
            ("wii", Medium::Wii),
            ("psp", Medium::Umd),
            ("nds", Medium::Ds),
            ("3ds", Medium::N3ds),
            ("atari2600", Medium::GenericCart),
            ("ngp", Medium::GenericCart),
            ("pcengine", Medium::GenericCart),
            ("3do", Medium::GenericDisc),
            ("wiiu", Medium::GenericDisc),
            ("ports", Medium::GenericDigital),
            ("pico-8", Medium::GenericDigital),
            ("xbox", Medium::Xbox),
            ("psvita", Medium::Vita),
            ("something-new", Medium::GenericDisc),
        ] {
            assert_eq!(Medium::of(system), want, "{system}");
        }
    }

    #[test]
    fn heights_keep_their_real_order() {
        let h = |m: Medium| m.size().h;
        // On the shelf: the GBA box, the Game Boy's, the NES box standing; the SNES and N64 boxes stand on their short
        // side as collectors keep them, as tall as the NES box (184 mm).
        assert!(h(Medium::Gba) < h(Medium::Gb) && h(Medium::Gb) < h(Medium::Nes));
        assert!(h(Medium::Snes) == h(Medium::Nes) && h(Medium::N64) == h(Medium::Nes));
        assert!(h(Medium::Ds) < h(Medium::Nes) && h(Medium::Nes) < h(Medium::Ps2));
        assert!(h(Medium::Ds) < h(Medium::Umd) && h(Medium::Umd) < h(Medium::Ps2));
        assert!(h(Medium::GameCube) < h(Medium::Ps2) && h(Medium::Ps1) < h(Medium::GameCube));
        let wide = |m: Medium| m.template_size().front > m.template_size().front_h;
        assert!(wide(Medium::Snes) && wide(Medium::N64) && !wide(Medium::Nes), "the SNES and N64 boxes lie wide");
        assert_eq!(Medium::Snes.template_size().h, Medium::Snes.template_size().front, "its spine as long as it is wide");
    }

    /// 0.1.4: every game stands as a case; a cartridge is what is inside a cartridge system's box.
    #[test]
    fn every_system_stands_as_a_case_with_its_game_inside() {
        for system in ["snes", "n64", "nes", "gb", "gbc", "gba", "atari2600", "ngp", "megadrive", "gamegear", "psx", "ps2", "psp", "nds", "3ds", "xbox", "psvita", "something-new"] {
            let m = Medium::of(system);
            let g = m.game();
            let cart_system = crate::cabinet::FormFactor::of(system).is_cart();
            assert_eq!(cart_system, !g.is_round() && g != Game::Card && g != Game::Memory, "{system}: {m:?} holds {g:?}");
            if matches!(m, Medium::Snes | Medium::N64 | Medium::Nes | Medium::Gb | Medium::Gba) {
                assert!(m.is_cardboard(), "{system}: a cardboard box");
            }
        }
        assert_eq!(Medium::of("nds").game(), Game::Card);
        assert_eq!(Medium::of("psp").game(), Game::Umd);
        assert_eq!(Medium::of("xbox").game(), Game::Disc);
        assert_eq!(Medium::of("megadrive").game(), Game::MegaDrive);
        assert_eq!(Medium::of("ports").game(), Game::Memory);
        assert_eq!(Medium::of("atari7800").game(), Game::Cart);
    }

    /// 2026-10-29: the popular systems have cases of their own, every other one pocket-ui's one generic case,
    /// in one size per kind of medium.
    #[test]
    fn popular_systems_have_their_own_case_and_the_rest_one_generic_one() {
        for system in ["gb", "gbc", "gba", "nes", "snes", "n64", "megadrive", "genesis", "psx", "ps2", "psp", "nds", "gamecube", "wii", "dreamcast", "xbox"] {
            assert!(!Medium::of(system).is_generic(), "{system}");
        }
        for system in ["atari2600", "atarilynx", "ngpc", "wonderswan", "pcengine", "neogeo", "3do", "pcenginecd", "wiiu", "ps3", "ports", "pico-8", "tic-80", "scummvm", "switch"] {
            assert!(Medium::of(system).is_generic(), "{system}");
        }
        let size = |s: &str| Medium::of(s).template_size();
        assert_eq!(size("atari2600"), size("ngpc"), "one size per medium");
        assert_eq!(size("3do"), size("wiiu"));
    }

    /// One scale for everything (0.63 template pixels a millimetre): the templates are the real cases' sizes, within a
    /// pixel or two of rounding, and what is inside keeps its real size to its case.
    #[test]
    fn cases_and_games_keep_their_real_sizes_to_each_other() {
        for m in Medium::ALL {
            let (w, h) = m.mm();
            let t = m.template_size();
            assert!((t.front_h as f32 - h * 0.63).abs() <= 2.0, "{m:?}: {} px high, {h} mm", t.front_h);
            assert!((t.front as f32 - w * 0.63).abs() <= 3.0, "{m:?}: {} px wide, {w} mm", t.front);
        }
        let beside = |s: &str| {
            let m = Medium::of(s);
            m.game().size_beside(m, 400)
        };
        assert!(beside("gb").1 < beside("snes").1 && beside("gba").1 < beside("gb").1, "a GBA cartridge is smaller than a GB one, a SNES one big");
        assert!(beside("n64").0 > beside("gb").0 * 3 / 2);
        assert!(beside("nds").1 < 400 / 3, "a DS card is tiny beside its case");
        assert!(Game::Card.size_beside(Medium::Ds, 60).1 >= GAME_MIN, "never under the minimum");
        for m in Medium::ALL {
            let (gw, gh) = m.game().size_beside(m, 300);
            assert!(gh <= 300 && gw <= m.template_size().front * 300 / m.template_size().front_h + 2, "{m:?}: its game fits in it");
        }
    }

    #[test]
    fn every_game_template_is_its_size_and_its_label_inside() {
        for (i, g) in Game::DRAWN.into_iter().enumerate() {
            let t = &GAME_TEMPLATES[i];
            let s = g.pic().unwrap();
            assert_eq!((s.w as i32, s.h as i32), t.size, "{g:?}");
            let (x, y, w, h) = t.label;
            assert!(x >= 0 && y >= 0 && w > 0 && h > 0 && x + w <= t.size.0 && y + h <= t.size.1, "{g:?} label");
        }
        assert!(Game::Disc.pic().is_none() && Game::Umd.pic().is_none());
    }

    /// The game inside shows its cover, or without one its name, never an empty label.
    #[test]
    fn the_game_inside_shows_its_cover_or_its_name() {
        let mut text = Text::new();
        let look = Look { color: 0x3a2a6a, band: 0x7a6cb0, plastic: 0x9c99a2 };
        let cover = Image { w: 40, h: 30, px: vec![0xd02020; 1200] };
        for g in Game::DRAWN {
            let h = if g == Game::Card { 120 } else { 200 };
            let with = game_pic(&mut text, g, h, look, "Chrono Trigger", Some(&cover), None).unwrap();
            assert_eq!(with.h, h, "{g:?}");
            let i = g.drawn().unwrap();
            let f = h as f32 / GAME_TEMPLATES[i].size.1 as f32;
            let (x, y, w, lh) = scale_rect(GAME_TEMPLATES[i].label, f);
            let (cx, cy) = (x + w / 2, y + lh / 2);
            assert_eq!(with.get(cx, cy) & 0xffffff, 0xd02020, "{g:?}: the cover in the middle of its label");
            let plain = game_pic(&mut text, g, h, look, "Chrono Trigger", None, None).unwrap();
            let colours: std::collections::HashSet<u32> = (y..y + lh).flat_map(|r| (x..x + w).map(move |c| (c, r))).map(|(c, r)| plain.get(c, r) & 0xffffff).collect();
            assert!(colours.len() > 3, "{g:?}: a plain label, not a hole");
        }
        assert!(game_pic(&mut text, Game::Disc, 200, look, "x", None, None).is_none());
    }

    /// Every disc template has its print in one flat colour, inside it, round about its middle for a disc; nothing
    /// is half transparent (pixel art).
    #[test]
    fn every_disc_template_has_a_print() {
        for k in DiscKind::ALL {
            let t = k.template();
            let (x, y, w, h) = t.print;
            assert!(w > t.sprite.w as i32 / 3 && h > t.sprite.h as i32 / 4, "{k:?}: print {:?}", t.print);
            assert!(x + w <= t.sprite.w as i32 && y + h <= t.sprite.h as i32, "{k:?}");
            assert!(t.sprite.px.iter().all(|p| matches!(p >> 24, 0 | 255)), "{k:?}: half transparent pixels");
            if k.is_round() {
                assert_eq!(t.sprite.w, t.sprite.h, "{k:?}: a disc is square");
                assert!(t.radii.0 > 4.0 && t.radii.1 <= t.sprite.w as f32 / 2.0 + 0.5, "{k:?}: {:?}", t.radii);
                // The hole is clear.
                let c = t.sprite.w / 2;
                assert_eq!(t.sprite.px[(c * t.sprite.w + c) as usize] >> 24, 0, "{k:?}: the hole");
            }
        }
    }

    /// The disc of each system: a GameCube's is the 8 cm one, the rest 12 cm, a UMD smaller still; Saturn, Dreamcast
    /// and Mega-CD keep their own (one case, three discs).
    #[test]
    fn each_system_has_its_disc() {
        assert_eq!(Game::of("gamecube", "F-Zero GX (USA).rvz"), Game::MiniDisc);
        assert_eq!(Game::of("ps2", "x.iso"), Game::Disc);
        let beside = |s: &str| Game::of(s, "x").size_beside(Medium::of(s), 300).1;
        assert!(beside("gamecube") < beside("ps2") && beside("psp") < beside("gamecube"), "UMD < 8 cm < 12 cm");
        assert_eq!((DiscKind::of("saturn"), DiscKind::of("dreamcast"), DiscKind::of("segacd")), (DiscKind::Saturn, DiscKind::Dreamcast, DiscKind::Cd));
        assert_eq!((DiscKind::of("psx"), DiscKind::of("psp"), DiscKind::of("3do")), (DiscKind::Ps1, DiscKind::Umd, DiscKind::Cd));
        for k in DiscKind::ALL {
            assert!(DiscKind::ALL.iter().filter(|o| o.template().sprite.px == k.template().sprite.px).count() == 1, "{k:?}: its own picture");
        }
    }

    /// A printed disc shows the cover in its print and the title on its ring when large enough; no cover, the title
    /// on the cover's colour; nothing of the template's print colour is left, and the hole stays clear.
    #[test]
    fn a_disc_is_printed_with_its_cover_or_its_title() {
        let mut text = Text::new();
        let look = Look { color: 0x3a2a6a, band: 0x7a6cb0, plastic: 0x141416 };
        // Red art with some texture (a flat colour would be trimmed as a margin, `trim_bands`).
        let (cw, ch) = (140u32, 200u32);
        let cover = Image { w: cw, h: ch, px: (0..cw * ch).map(|i| 0xd00000 | ((i % cw * 37 + i / cw * 11) % 64) << 8 | (i % cw * 5 % 48)).collect() };
        let is_red = |p: u32| p >> 24 == 255 && p >> 16 & 0xff == 0xd0 && p >> 8 & 0xff < 64 && p & 0xff < 48;
        for k in DiscKind::ALL {
            for h in [80, 160, 300] {
                let with = disc_pic(&mut text, k, h, look, "Castlevania: Symphony of the Night", Some(&cover));
                assert_eq!(with.h, h, "{k:?}");
                assert!(with.px.iter().all(|p| p >> 24 == 0 || p & 0xffffff != PRINT), "{k:?} {h}: print colour left");
                let red = with.px.iter().filter(|p| is_red(**p)).count();
                assert!(red > (h * h) as usize / 20, "{k:?} {h}: the cover's art shows ({red})");
                let plain = disc_pic(&mut text, k, h, look, "Castlevania: Symphony of the Night", None);
                assert!(plain.px.iter().all(|p| p >> 24 == 0 || p & 0xffffff != PRINT), "{k:?} {h}: print colour left");
                if k.is_round() {
                    assert_eq!(with.get(with.w / 2, with.h / 2) >> 24, 0, "{k:?}: the hole");
                }
                if h == 300 {
                    // The title: light pixels on the ring (with a cover) or above the hole (without).
                    let light = |p: &Pic, y0: i32, y1: i32| (y0..y1).flat_map(|y| (0..p.w).map(move |x| (x, y))).filter(|&(x, y)| luma(p.get(x, y) & 0xffffff) > 180 && p.get(x, y) >> 24 == 255).count();
                    if k.is_round() {
                        assert!(light(&with, h * 3 / 4, h) > 40, "{k:?}: the title on the ring");
                    }
                    assert!(light(&plain, 0, h) > 40, "{k:?}: the title without a cover");
                }
            }
        }
    }

    /// Every disc and the UMD, with a cover from `DISC_COVERS` (`<kind>.png`) and without, to look at
    /// (`cargo test --release disc_prints_to_look_at -- --ignored`, PNGs in `DISC_OUT`).
    #[test]
    #[ignore]
    fn disc_prints_to_look_at() {
        let (Some(covers), Some(out)) = (std::env::var_os("DISC_COVERS"), std::env::var_os("DISC_OUT")) else { return };
        let (covers, out) = (std::path::PathBuf::from(covers), std::path::PathBuf::from(out));
        let mut text = Text::new();
        for k in DiscKind::ALL {
            let name = format!("{k:?}").to_lowercase();
            let cover = crate::image::load_image(&covers.join(format!("{name}.png")));
            let look = cover.as_ref().and_then(|c| cover_colors(&c.px)).map_or(Look { color: 0x2a3a6a, band: 0x7a6cb0, plastic: 0x141416 }, |(c, a)| Look { color: c, band: a, plastic: 0x141416 });
            let title = std::fs::read_to_string(covers.join(format!("{name}.txt"))).unwrap_or_else(|_| "Game Title".into());
            for (tag, h) in [("big", 300), ("small", 126)] {
                for (with, c) in [("cover", cover.as_ref()), ("plain", None)] {
                    let p = disc_pic(&mut text, k, h, look, title.trim(), c);
                    let rgba: Vec<u8> = p.px.iter().flat_map(|q| [(q >> 16) as u8, (q >> 8) as u8, *q as u8, (q >> 24) as u8]).collect();
                    let f = std::fs::File::create(out.join(format!("{name}-{tag}-{with}.png"))).unwrap();
                    let mut e = png::Encoder::new(f, p.w as u32, p.h as u32);
                    e.set_color(png::ColorType::Rgba);
                    e.set_depth(png::BitDepth::Eight);
                    e.write_header().unwrap().write_image_data(&rgba).unwrap();
                }
            }
        }
    }

    /// A GBA cartridge's label keeps its rounded corners (plastic there, cover or not), and shows the top of the box art,
    /// where its title is; an N64 label keeps the rounded top of its recess.
    #[test]
    fn cartridge_labels_keep_their_shape_and_show_the_title_end() {
        let mut text = Text::new();
        let look = Look { color: 0x3a2a6a, band: 0x7a6cb0, plastic: 0x9c99a2 };
        // Square box art: a red top half (the title), a blue bottom half, both with some texture (a flat area at an
        // edge would be taken for a case's margin, `trim_bands`).
        let (w, h) = (100u32, 100u32);
        let px = (0..w * h).map(|i| (((i % w) * 7 + (i / w) * 3) % 32) | if i / w < h / 2 { 0xd02000 } else { 0x2020c0 }).collect();
        let cover = Image { w, h, px };
        for g in [Game::Gba, Game::N64] {
            let i = g.drawn().unwrap();
            let f = 300.0 / GAME_TEMPLATES[i].size.1 as f32;
            let (x, y, lw, lh) = scale_rect(GAME_TEMPLATES[i].label, f);
            for cover in [Some(&cover), None] {
                let pic = game_pic(&mut text, g, 300, look, "Golden Sun", cover, None).unwrap();
                // The label's top left corner is the shell's plastic: a neutral grey, not art or paper.
                let c = pic.get(x, y) & 0xffffff;
                let (r, gg, b) = (c >> 16 & 0xff, c >> 8 & 0xff, c & 0xff);
                assert!(r.abs_diff(gg) < 12 && gg.abs_diff(b) < 12, "{g:?}: the label's corner stays plastic ({c:06x})");
            }
            let pic = game_pic(&mut text, g, 300, look, "Golden Sun", Some(&cover), None).unwrap();
            let red = (y..y + lh).flat_map(|r| (x..x + lw).map(move |c| (c, r))).filter(|&(c, r)| pic.get(c, r) >> 16 & 0xff > 0xb0).count();
            let share = red as f32 / (lw * lh) as f32;
            if g == Game::Gba {
                assert!(share > 0.85, "GBA: the label is the top of the art ({share:.2} red)");
            } else {
                assert!((0.3..0.7).contains(&share), "N64: the label is the art's middle ({share:.2} red)");
            }
        }
    }

    /// An N64 box's spine is printed in its cover's colour with the red block at its top (US boxes), not black.
    #[test]
    fn an_n64_spine_is_its_cover_with_the_red_block() {
        let mut text = Text::new();
        let look = Look { color: 0x2a6ab0, band: 0x7a6cb0, plastic: 0x9c99a2 };
        let s = spine(&mut text, Medium::N64, 1.0, look, "Super Mario 64");
        let t = Medium::N64.template();
        let (bx, by, bw, bh) = t.at(t.band.unwrap(), SHELF_SCALE);
        let reds = (by..by + bh).flat_map(|r| (bx..bx + bw).map(move |c| (c, r))).filter(|&(c, r)| {
            let p = s.get(c, r);
            (p >> 16 & 0xff) > 150 && (p >> 8 & 0xff) < 60
        });
        assert!(reds.count() > (bw * bh / 4) as usize, "the red block");
        let (lx, ly, lw, lh) = t.at(t.spine_label, SHELF_SCALE);
        let blue = (ly..ly + lh).flat_map(|r| (lx..lx + lw).map(move |c| (c, r))).filter(|&(c, r)| {
            let p = s.get(c, r);
            (p & 0xff) > (p >> 16 & 0xff) + 40
        });
        assert!(blue.count() > (lw * lh / 2) as usize, "the spine in the cover's colour");
    }

    /// A SNES box's black side strip (and a case front's margin) is not put on a cartridge's label.
    #[test]
    fn a_box_side_strip_is_trimmed_off_the_label() {
        // 120 x 80 artwork in red with a grey margin of 4 round it (a case's front round a cover of another shape).
        let (w, h) = (128u32, 88u32);
        let mut px = vec![0x808080u32; (w * h) as usize];
        for y in 4..84 {
            for x in 4..124 {
                px[(y * w + x) as usize] = 0xc00000 | ((x * 37 + y * 11) % 200) << 8 | (x * 5 % 64);
            }
        }
        let t = trim_bands(&Image { w, h, px });
        assert_eq!((t.w, t.h), (120, 80));
        assert!(t.px.iter().all(|p| p >> 16 & 0xff > 0x80), "no grey margin left");
        // A picture without bands stays as it is.
        let plain = Image { w: 60, h: 40, px: (0..2400u32).map(|i| 0x204080 + (i * 37 % 97) * 0x10101).collect() };
        assert_eq!(trim_bands(&plain).w, 60);
    }

    /// A GBA box's silver strip up its left side is not put on the cartridge's label; a box without one keeps its art.
    #[test]
    fn a_systems_side_strip_is_cut_off_the_label() {
        let (w, h) = (100u32, 100u32);
        let art = |x: u32, y: u32| 0x3070c0 ^ ((x * 13 + y * 7) % 32) << 8;
        // The strip with its lettering: a dark column with a straight edge, an eighth in.
        let strip: Vec<u32> = (0..w * h)
            .map(|i| match i % w {
                9..=12 => 0x202020,
                x if x < 18 => 0xd8d8d8 - (i / w % 8),
                x => art(x, i / w),
            })
            .collect();
        let t = trim_system_band(&Image { w, h, px: strip }, Game::Gba);
        assert!((80..=83).contains(&t.w) && t.h == h, "{}x{}", t.w, t.h);
        assert!(t.px.iter().all(|p| p & 0xff > 0x80), "no silver left");
        let plain = Image { w, h, px: (0..w * h).map(|i| art(i % w, i / w)).collect() };
        // A GBA box always has its strip: where none shows, the usual part goes.
        assert_eq!(trim_system_band(&plain, Game::Gba).w, w - 22, "the usual strip");
        // A Super Famicom / PAL box has none unless it shows; an N64 box has none.
        assert_eq!(trim_system_band(&plain, Game::Snes).w, w, "no strip, nothing cut");
        assert_eq!(trim_system_band(&plain, Game::N64).w, w);
    }

    /// With real box art (`POCKETUI_TEST_COVERS=<dir>` holding gba/, gb/, snes/ and n64/ box art PNGs, e.g. the libretro
    /// thumbnails of a library; not in the repository): the strip is gone from every GBA, US SNES and US N64 label, and no label
    /// loses more than the strip's part of its picture.
    #[test]
    fn real_covers_lose_their_side_strip() {
        let Some(dir) = std::env::var_os("POCKETUI_TEST_COVERS") else {
            eprintln!("SKIPPED: POCKETUI_TEST_COVERS not set");
            return;
        };
        let dir = std::path::PathBuf::from(dir);
        let mut seen = 0;
        for (system, g, side) in [("gba", Game::Gba, 0), ("snes", Game::SnesUs, 1), ("gb", Game::Gb, 0), ("n64", Game::N64, 1)] {
            let Ok(files) = std::fs::read_dir(dir.join(system)) else { continue };
            for f in files.flatten() {
                let Some(img) = crate::image::load_image(&f.path()) else { continue };
                let tb = trim_bands(&img);
                let t = trim_system_band(&tb, g);
                let kept = t.w as f32 / img.w as f32;
                eprintln!("{system} {:?}: {}x{} bands {}x{} strip {}x{}", f.file_name(), img.w, img.h, tb.w, tb.h, t.w, t.h);
                // US boxes all carry the strip: a sixth to a third of the width goes, never more.
                assert!((0.68..=0.94).contains(&kept), "{system} {:?}: kept {kept:.2} of the width", f.file_name());
                // The cut side's edge column is not the strip's colour any more: it differs from the old edge column.
                let edge = |im: &Image, x: u32| (0..im.h).map(|y| im.px[(y * im.w + x) as usize]).collect::<Vec<_>>();
                let (old, new) = if side == 0 { (edge(&img, 0), edge(&t, 0)) } else { (edge(&img, img.w - 1), edge(&t, t.w - 1)) };
                assert_ne!(old, new, "{system} {:?}", f.file_name());
                seen += 1;
            }
        }
        assert!(seen > 5, "no covers read from {dir:?}");
    }

    /// A label without a cover writes the whole title, smaller or on more lines, never cut while it can fit.
    #[test]
    fn a_label_title_fits_its_label() {
        let mut text = Text::new();
        let look = Look { color: 0x3a2a6a, band: 0x7a6cb0, plastic: 0x9c99a2 };
        for (w, h) in [(120, 80), (90, 70), (200, 120)] {
            let mut p = Pic::new(w + 10, h + 10, 0xff000000);
            label_title(&mut text, &mut p, (5, 5, w, h), look, "N64 Game Title Long Name");
            let ink = ink_on(mix(look.color, 0x000000, 60));
            let inked = p.px.iter().filter(|q| **q & 0xffffff == ink).count();
            assert!(inked > 30, "{w}x{h}: the title is written");
            // Nothing outside the label.
            for y in 0..p.h {
                for x in 0..p.w {
                    if !(5..5 + w).contains(&x) || !(5..5 + h).contains(&y) {
                        assert_eq!(p.get(x, y), 0xff000000, "{w}x{h}: ink at {x},{y}");
                    }
                }
            }
        }
    }

    #[test]
    fn the_cover_is_taken_off_a_front_whole() {
        let mut text = Text::new();
        let look = Look { color: 0x3a2a6a, band: 0x7a6cb0, plastic: 0x9c99a2 };
        for m in [Medium::Snes, Medium::Ps2, Medium::Ds] {
            let (_, _, lw, lh) = label_rect(m, 1.0);
            let cover = art::cover_fit(&Image { w: 4, h: 3, px: vec![0x20c040; 12] }, lw, lh);
            let f = front(&mut text, m, 1.0, look, "x", Some(&cover), None, "");
            let got = cover_of(&f, m, 1.0);
            assert_eq!((got.w as i32, got.h as i32), (lw, lh), "{m:?}");
            assert!(got.px.iter().filter(|p| **p == 0x20c040 || (**p >> 8 & 0xff) > 0xb0).count() > got.px.len() / 2, "{m:?}");
        }
    }

    /// A game without box art still shows something you can read: its system's colour on the label, its name on the
    /// spine, and a front with a plain label and its name (never an empty case).
    #[test]
    fn without_a_cover_a_case_still_shows_its_name() {
        let mut text = Text::new();
        let look = Look { color: 0x3a2a6a, band: 0x7a6cb0, plastic: 0x9c99a2 };
        for m in Medium::ALL {
            let spine = spine(&mut text, m, 1.0, look, "Chrono Trigger");
            let (x, y, w, h) = m.template().at(m.template().spine_label, SHELF_SCALE);
            let ink = ink_on(spine_print(m).and_then(|(paper, _)| paper).unwrap_or(look.color));
            let inked = (y..y + h).flat_map(|r| (x..x + w).map(move |c| (c, r))).filter(|&(c, r)| spine.get(c, r) & 0xffffff == ink).count();
            assert!(inked > 20, "{m:?}: the title on the spine ({inked} pixels)");
            let front = front(&mut text, m, 1.0, look, "Chrono Trigger", None, None, "Atari 2600");
            let (fx, fy, fw, fh) = label_rect(m, 1.0);
            let colours: std::collections::HashSet<u32> = (fy..fy + fh).flat_map(|r| (fx..fx + fw).map(move |c| (c, r))).map(|(c, r)| front.get(c, r) & 0xffffff).collect();
            assert!(colours.len() > 8, "{m:?}: the plain front has a gradient and its name, not one flat colour");
        }
    }

    /// Every box of one system is exactly the same size, whatever its title and its cover's colours (a white one, a
    /// black one, a long title), and its outline is the same on all of them, so none looks bigger.
    #[test]
    fn every_box_of_one_system_has_the_same_size_and_edge() {
        let mut text = Text::new();
        let looks = [
            Look { color: 0xf4f4f0, band: 0xd02020, plastic: 0x9c99a2 },
            Look { color: 0x101014, band: 0x2050c0, plastic: 0x9c99a2 },
            Look { color: 0xc02030, band: 0xf0d020, plastic: 0x9c99a2 },
        ];
        let titles = ["F-Zero", "Castlevania - Aria of Sorrow", "Mario & Luigi - Superstar Saga", "Boktai - The Sun Is in Your Hand"];
        for m in Medium::ALL {
            for k in [1.0, 0.6, 1.2] {
                let mut sizes = std::collections::HashSet::new();
                let mut fronts = std::collections::HashSet::new();
                let mut edges = std::collections::HashSet::new();
                for look in looks {
                    for title in titles {
                        let p = spine(&mut text, m, k, look, title);
                        sizes.insert((p.w, p.h));
                        if m.is_cardboard() {
                            edges.insert((p.get(0, p.h / 2), p.get(p.w - 1, p.h / 2), p.get(p.w / 2, 0)));
                        }
                    }
                    let f = front(&mut text, m, k, look, "x", None, None, "");
                    fronts.insert((f.w, f.h));
                }
                assert_eq!(sizes.len(), 1, "{m:?} at {k}: spines {sizes:?}");
                assert_eq!(fronts.len(), 1, "{m:?} at {k}: fronts {fronts:?}");
                assert!(edges.len() <= 1, "{m:?} at {k}: edges {edges:?}");
            }
        }
    }

    /// Trailer frames, 2026-10-29: "Advance…", "Donkey Kon…", "Prince of Persia - The Sa…" on spines with room. A long
    /// title shrinks, goes on two lines across a thick box, loses its subtitle, and is cut with "…" only after that.
    #[test]
    fn long_titles_fit_their_spine_before_any_is_cut() {
        let mut text = Text::new();
        // A SNES box's spine label (it stands on its short side: 91 template pixels long) at two screen pixels a
        // template pixel.
        let (thick, len) = ((20.0 * 2.0) as i32, (91.0 * 2.0) as i32 - 8);
        for title in ["Donkey Kong Country", "Super Mario All-Stars", "Prince of Persia - The Sands of Time", "Street Fighter II - The World Warrior"] {
            let (lines, px) = spine_fit(&mut text, title, thick, len, 30.0);
            assert!(lines.iter().all(|l| !l.ends_with('…')), "{title}: {lines:?} at {px}");
            assert!(px >= 21, "{title}: readable ({px})");
            assert!(lines.iter().all(|l| !l.starts_with('-') && !l.ends_with('-')), "{title}: no dash at a line's end: {lines:?}");
        }
        // At three screen pixels a template pixel (the Mini V2), two lines across it: the subtitle on a line of its own.
        let (lines, _) = spine_fit(&mut text, "Prince of Persia - The Sands of Time", 60, 91 * 3 - 12, 30.0);
        assert_eq!(lines, ["Prince of Persia", "The Sands of Time"], "the subtitle on a line of its own");
        // A thin spine with no room for two lines: the subtitle goes before anything is cut.
        let (lines, _) = spine_fit(&mut text, "Astro Boy - Omega Factor", 30, 130, 30.0);
        assert_eq!(lines, ["Astro Boy"]);
        // A short title stays at the size asked for.
        assert_eq!(spine_fit(&mut text, "F-Zero", thick, len, 30.0), (vec!["F-Zero".to_string()], 30));
        // Only a title too long for anything is cut, at the least size.
        let (lines, px) = spine_fit(&mut text, "Supercalifragilistic Expialidocious Adventures", 30, 80, 30.0);
        assert!(lines[0].ends_with('…') && px == 21, "{lines:?} {px}");
    }

    #[test]
    fn every_shelf_with_room_gets_a_prop_beside_its_bookend() {
        assert!((0..100).all(|s| Prop::beside(s, 300).is_some_and(|p| p != Prop::Bookend)));
        assert!((0..100).all(|s| Prop::beside(s, 80).is_none_or(|p| p.pic().h <= 80)), "never taller than the room");
        let kinds: std::collections::HashSet<Prop> = (0..100).filter_map(|s| Prop::beside(s, 300)).collect();
        assert!(kinds.len() >= 3, "not the same plant on every shelf: {kinds:?}");
        assert_eq!(Prop::beside(7, 300), Prop::beside(7, 300));
    }

    #[test]
    fn props_are_small() {
        for p in Prop::ALL {
            let pic = p.pic();
            assert!(pic.w > 25 && pic.h > 50 && pic.h < 175 && pic.w < 113, "{p:?} {}x{}", pic.w, pic.h);
        }
    }

    #[test]
    fn the_accent_is_a_clear_colour_apart_from_the_main_one() {
        // A blue cover with a red logo: blue, then red.
        let mut px = vec![0x2050c0u32; 9000];
        px.extend(std::iter::repeat_n(0xd02020, 900));
        px.extend(std::iter::repeat_n(0xffffff, 100));
        let (main, accent) = cover_colors(&px).unwrap();
        assert!(main & 0xff > 0x80 && accent >> 16 & 0xff > 0x80, "{main:06x} {accent:06x}");
        // One colour only: the accent is the same.
        let (main, accent) = cover_colors(&[0x2050c0; 400]).unwrap();
        assert_eq!(main, accent);
        assert!(cover_colors(&[]).is_none());
    }
}
