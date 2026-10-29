//! Frames around the game picture ("Screen frame" in the pause menu). A frame is an image with a
//! *hole*: the rectangle where the game goes. It is drawn behind the picture whenever the picture
//! doesn't fill the screen (Native and Aspect scaling), scaled so that the hole lands exactly on the
//! game rectangle, so it grows and shrinks with the integer scale.
//!
//! Two sources:
//! - **Built-in** frames are drawn here in code at the exact output size, so they are pixel-sharp at
//!   every scale: a recessed screen lens for handhelds, a quiet monitor surround for home consoles.
//!   Original designs, no logos or lettering. They never run off the screen: each side is as wide as
//!   the space beside the picture allows, keeping a strip of true black at the panel edge.
//! - **Frame packs** in `<data>/frames/<pack>/` (`pack.json` plus PNGs, see docs/FRAMES.md), for the
//!   store. Auto uses the first installed pack (by folder name) that has a frame for the console,
//!   otherwise the built-in one.

use std::path::{Path, PathBuf};

use glow::HasContext;

use crate::video::Rect;

/// An RGBA image (premultiplied alpha, rows top to bottom) and where the game goes in it.
#[derive(Clone)]
pub struct Bezel {
    pub w: u32,
    pub h: u32,
    pub rgba: Vec<u8>,
    pub hole: Rect,
    /// The power light, painted live over the cached frame (`frame_led`).
    pub led: Option<crate::frame_led::Led>,
    /// A strip inside the hole that is not game picture and that the frame draws over, after the
    /// picture: the gap between the Nintendo DS's two screens, where the Twin frame's hinge goes.
    pub inlay: Option<Rect>,
}

impl Bezel {
    pub fn new(w: u32, h: u32, rgba: Vec<u8>, hole: Rect) -> Self {
        Bezel { w, h, rgba, hole, led: None, inlay: None }
    }

    /// Puts a power light at `spot`: keeps the frame under it for live updates and paints it
    /// steady green (what the light shows with a full battery).
    pub fn with_led(mut self, spot: crate::frame_led::Spot) -> Self {
        let led = crate::frame_led::capture(spot, &self.rgba, self.w, self.h);
        let patch = crate::frame_led::paint(&led, crate::frame_led::Look::default(), self.hole);
        crate::frame_led::blit(&mut self.rgba, self.w, led.rect, &patch);
        self.led = Some(led);
        self
    }

    /// Where the whole image goes, in viewport pixels, so that its hole covers `game` exactly. Each
    /// axis scales on its own, so the hole always matches; it may reach past the screen.
    pub fn placement(&self, game: Rect) -> (f32, f32, f32, f32) {
        let sx = game.w as f32 / self.hole.w.max(1) as f32;
        let sy = game.h as f32 / self.hole.h.max(1) as f32;
        (game.x as f32 - self.hole.x as f32 * sx, game.y as f32 - self.hole.y as f32 * sy, self.w as f32 * sx, self.h as f32 * sy)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Style {
    /// A dark screen lens with the LCD set into it, and a small power light.
    Handheld,
    /// A monitor surround: a thin black gap, then a dark bezel lit faintly by the picture.
    Monitor,
    /// The handheld lens inside a wider body of moulded plastic in a colourway.
    Shell(Colourway),
    /// A curved-screen TV: dark tube glass in a rounded opening, an inner bezel and a cabinet.
    Crt(Cabinet),
    /// An arcade cabinet's glossy black bezel with soft coloured light strips at the sides.
    Arcade,
    /// A thin rounded outline glowing softly in a colour (0xRRGGBB, the console's stripe colour).
    Glow(u32),
    /// A brick-shaped handheld: darker screen glass with one large rounded corner bottom right, two
    /// thin coloured lines above the picture, a power light left of it.
    Brick(BrickColour),
    /// A slimmer handheld in brushed metal (or clear plastic), with tighter glass.
    PocketSlim(SlimFinish),
    /// An opened clamshell's lid, with the hinge along the bottom.
    Clamshell(ClamFinish),
    /// A landscape body with rounded wings, shoulder bumps and a speaker grille.
    Wide,
    /// Two screens stacked, with a hinge between them (the Nintendo DS picture stacked).
    Twin,
    /// A piano-black wide body with oval ends and a diagonal sheen.
    Gloss,
    /// A minimal body in pastel or see-through plastic.
    Swan(SwanFinish),
    /// A brushed metal faceplate around a small screen window.
    Watch(WatchFinish),
    /// A red and black visor around the picture.
    Goggles,
    /// A grey professional monitor: flat-ish tube, crisp glass and a row of controls below.
    Studio,
    /// A small portable TV with a telescopic antenna and a tuning dial.
    PortableTv,
    /// A faux-wood panel with a ribbed black band on top.
    Woodgrain,
}

/// A finish of one of the frames: parsed from its setting's value, unknown values are the first.
macro_rules! finish {
    ($(#[$doc:meta])* $name:ident { $($variant:ident = $value:literal),+ $(,)? }) => {
        $(#[$doc])*
        #[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, Default)]
        pub enum $name {
            #[default]
            $($variant),+
        }

        impl $name {
            pub const ALL: &'static [$name] = &[$($name::$variant),+];

            pub fn parse(s: &str) -> Self {
                Self::ALL.iter().copied().find(|f| f.value() == s).unwrap_or_default()
            }

            /// The setting's value for this finish.
            pub fn value(self) -> &'static str {
                match self {
                    $($name::$variant => $value),+
                }
            }
        }
    };
}

finish!(
    /// Colourways of the Brick frame.
    BrickColour { Grey = "grey", Clear = "clear", Red = "red", Yellow = "yellow", Green = "green", Black = "black" }
);
finish!(
    /// Finishes of the Pocket slim frame.
    SlimFinish { Silver = "silver", Clear = "clear" }
);
finish!(
    /// Finishes of the Clamshell frame.
    ClamFinish { Platinum = "platinum", Cobalt = "cobalt", FlameRed = "flame", Graphite = "graphite" }
);
finish!(
    /// Finishes of the Swan frame.
    SwanFinish { Pastel = "pastel", Skeleton = "skeleton" }
);
finish!(
    /// Finishes of the Watch frame.
    WatchFinish { Gold = "gold", Silver = "silver" }
);

/// Colourways of the Shell frame. Generic plastics, named by colour only.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, Default)]
pub enum Colourway {
    #[default]
    Grey,
    Indigo,
    ClearPurple,
    AtomicPurple,
    Black,
    White,
    GlacierBlue,
}

impl Colourway {
    pub fn parse(s: &str) -> Self {
        match s {
            "indigo" => Self::Indigo,
            "clear-purple" => Self::ClearPurple,
            "atomic-purple" => Self::AtomicPurple,
            "black" => Self::Black,
            "white" => Self::White,
            "glacier" => Self::GlacierBlue,
            _ => Self::Grey,
        }
    }
}

/// The CRT frame's cabinet.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, Default)]
pub enum Cabinet {
    #[default]
    Plastic,
    Wood,
}

impl Cabinet {
    pub fn parse(s: &str) -> Self {
        if s == "wood" {
            Self::Wood
        } else {
            Self::Plastic
        }
    }
}

/// The colour of a console's edge stripe on the shelf (0xRRGGBB); unknown systems get a muted
/// colour derived from the name. The Glow frame lights up in it.
pub fn system_colour(system: &str) -> u32 {
    match system {
        "nes" | "famicom" => 0xc8102e,
        "gb" => 0x8a9a3b,
        "gbc" => 0x2e8c9a,
        "gba" => 0x5b4aa6,
        "snes" | "sfc" => 0x7a6cb0,
        "megadrive" | "genesis" => 0xd4af37,
        "psx" => 0x9aa3ad,
        "n64" => 0x2e7d32,
        "psp" => 0x6b6f76,
        "nds" => 0xb0b6bd,
        "dreamcast" => 0xe2711d,
        "gamecube" => 0x6a5acd,
        "wii" | "wiiware" => 0x5aa9d6,
        "3ds" | "n3ds" => 0x6b7f8c,
        "ps2" => 0x2b46a8,
        "xbox" => 0x107c10,
        "saturn" => 0x5b5f7a,
        "settings" => 0x8a8f96,
        "favorites" => 0xd39a2c,
        "streaming" => 0x3f8f7a,
        _ => {
            let h = system.bytes().fold(5381u32, |h, b| h.wrapping_mul(33) ^ b as u32);
            let base = [0x8c6d5a, 0x5a7d8c, 0x7d8c5a, 0x8c5a7a, 0x5a8c7a, 0x7a5a8c];
            base[(h as usize) % base.len()]
        }
    }
}

/// How wide each style may get, as a share of the picture height.
fn reach(style: Style) -> f32 {
    match style {
        Style::Handheld => 0.085,
        Style::Monitor => 0.07,
        Style::Shell(_) => 0.26,
        Style::Crt(_) => 0.16,
        Style::Arcade => 0.13,
        Style::Glow(_) => 0.06,
        Style::Brick(_) => 0.26,
        Style::PocketSlim(_) => 0.2,
        Style::Clamshell(_) => 0.24,
        Style::Wide => 0.3,
        Style::Twin => 0.25,
        Style::Gloss => 0.27,
        Style::Swan(_) => 0.2,
        Style::Watch(_) => 0.24,
        Style::Goggles => 0.26,
        Style::Studio => 0.16,
        Style::PortableTv => 0.24,
        Style::Woodgrain => 0.2,
    }
}

/// The frame's width on each side of `game`, [left, right, top, bottom] in output pixels: as wide
/// as the style wants, but never to the panel edge, where a strip of black stays.
pub fn bands(style: Style, view_w: u32, view_h: u32, game: Rect) -> [f32; 4] {
    let want = game.h as f32 * reach(style);
    let side = |m: f32| (m - (m * 0.15).clamp(4.0, 16.0)).min(want).max(0.0);
    let (gx1, gy1) = ((game.x + game.w) as f32, (game.y + game.h) as f32);
    [side(game.x as f32), side(view_w as f32 - gx1), side(game.y as f32), side(view_h as f32 - gy1)]
}

const HANDHELDS: &[&str] = &["gamegear", "ngp", "ngpc", "atarilynx", "lynx", "pokemini"];

/// Auto's built-in frame for a console (an installed pack's frame still comes first): the handheld
/// it ran on, in spirit; the lens for other handhelds; the monitor surround for everything else.
pub fn style_for(system: &str) -> Style {
    match system {
        "gb" | "gbh" => Style::Brick(BrickColour::Grey),
        "gbc" | "gbch" => Style::PocketSlim(SlimFinish::Clear),
        "gba" | "gbah" | "gbav" => Style::Wide,
        // Falls back to the lens when the screens aren't stacked (`frames_more::twin`).
        "nds" => Style::Twin,
        "psp" => Style::Gloss,
        "wonderswan" => Style::Swan(SwanFinish::Pastel),
        "wonderswancolor" => Style::Swan(SwanFinish::Skeleton),
        "gameandwatch" => Style::Watch(WatchFinish::Gold),
        "virtualboy" => Style::Goggles,
        "atari2600" | "atari7800" => Style::Woodgrain,
        _ if HANDHELDS.contains(&system) => Style::Handheld,
        _ => Style::Monitor,
    }
}

pub(crate) fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Signed distance to a rounded rectangle (negative inside).
pub(crate) fn sd_round_rect(x: f32, y: f32, (x0, y0, x1, y1): (f32, f32, f32, f32), r: f32) -> f32 {
    let (cx, cy, hw, hh) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0, (x1 - x0) / 2.0, (y1 - y0) / 2.0);
    let qx = (x - cx).abs() - (hw - r);
    let qy = (y - cy).abs() - (hh - r);
    let (ox, oy) = (qx.max(0.0), qy.max(0.0));
    (ox * ox + oy * oy).sqrt() + qx.max(qy).min(0.0) - r
}

/// Cheap per-pixel noise in -0.5..0.5, used as dither: dark gradients would band visibly on OLED.
pub(crate) fn noise(x: u32, y: u32) -> f32 {
    let mut h = x.wrapping_mul(0x8da6_b343) ^ y.wrapping_mul(0xd816_3841) ^ 0x9e37_79b9;
    h ^= h >> 15;
    h = h.wrapping_mul(0x2c1b_3c6d);
    h ^= h >> 12;
    (h & 0xffff) as f32 / 65535.0 - 0.5
}

/// Draws a built-in frame at the viewport's size around `game`. `px` is output pixels per game
/// pixel (the integer scale at Native). None when there is no room for a frame.
pub fn generate(style: Style, view_w: u32, view_h: u32, game: Rect, px: f32) -> Option<Bezel> {
    let b = bands(style, view_w, view_h, game);
    if b.iter().cloned().fold(0.0, f32::max) < 6.0 {
        return None;
    }
    match style {
        Style::Handheld | Style::Monitor => classic(style, view_w, view_h, game, px, b),
        Style::Shell(c) => crate::frames::shell(c, view_w, view_h, game, px, b),
        Style::Crt(c) => Some(crate::frames::crt(c, view_w, view_h, game, px, b)),
        Style::Arcade => Some(crate::frames::arcade(view_w, view_h, game, px, b)),
        Style::Glow(rgb) => Some(crate::frames::glow(rgb, view_w, view_h, game, px, b)),
        Style::Brick(c) => Some(crate::frames_more::brick(c, view_w, view_h, game, px, b)),
        Style::PocketSlim(f) => Some(crate::frames_more::pocket(f, view_w, view_h, game, px, b)),
        Style::Clamshell(f) => Some(crate::frames_more::clamshell(f, view_w, view_h, game, px, b)),
        Style::Wide => Some(crate::frames_more::wide(view_w, view_h, game, px, b)),
        Style::Twin => crate::frames_more::twin(view_w, view_h, game, px, b),
        Style::Gloss => Some(crate::frames_more::gloss(view_w, view_h, game, px, b)),
        Style::Swan(f) => Some(crate::frames_more::swan(f, view_w, view_h, game, px, b)),
        Style::Watch(f) => Some(crate::frames_more::watch(f, view_w, view_h, game, px, b)),
        Style::Goggles => Some(crate::frames_more::goggles(view_w, view_h, game, px, b)),
        Style::Studio => Some(crate::frames_tv::studio(view_w, view_h, game, px, b)),
        Style::PortableTv => Some(crate::frames_tv::portable(view_w, view_h, game, px, b)),
        Style::Woodgrain => Some(crate::frames_tv::woodgrain(view_w, view_h, game, px, b)),
    }
}

/// The two original styles, with each side `[left, right, top, bottom]` wide. The Shell frame uses
/// the handheld lens of this with narrower bands, inside its body.
pub(crate) fn classic(style: Style, view_w: u32, view_h: u32, game: Rect, px: f32, [bl, br, bt, bb]: [f32; 4]) -> Option<Bezel> {
    let (gx0, gy0, gx1, gy1) = (game.x as f32, game.y as f32, (game.x + game.w) as f32, (game.y + game.h) as f32);
    if bl.max(br).max(bt).max(bb) < 2.0 {
        return None;
    }
    let plate = (gx0 - bl, gy0 - bt, gx1 + br, gy1 + bb);
    // Corner radius from the picture size; small enough that every corner of the hole stays inside.
    let mut radius = game.h as f32 * if style == Style::Handheld { 0.03 } else { 0.045 };
    for (a, b) in [(bl, bt), (br, bt), (bl, bb), (br, bb)] {
        if a < radius && b < radius {
            radius = a.max(b);
        }
    }
    let px = px.max(1.0);

    // Palette, sRGB 0..1. Handheld: warm charcoal plastic. Monitor: neutral graphite.
    let (top, bottom) = match style {
        Style::Handheld => ([0.112, 0.110, 0.122], [0.062, 0.061, 0.068]),
        _ => ([0.085, 0.085, 0.088], [0.047, 0.047, 0.049]),
    };
    // Power light: on the wider of the left and top bands, if one has room.
    let led = (style == Style::Handheld)
        .then(|| {
            if bl >= 34.0 {
                Some((gx0 - bl / 2.0, gy0 + game.h as f32 * 0.24))
            } else if bt >= 34.0 {
                Some((gx0 + game.w as f32 * 0.045, gy0 - bt / 2.0))
            } else {
                None
            }
        })
        .flatten();
    let led_r = (px * 0.75).clamp(3.5, 5.5);

    let recess = (px * 0.7).clamp(2.0, 6.0);
    let gap = (px * 0.5).clamp(2.0, 4.0);
    // Shaded on all cores (`frames::Canvas::paint`, the same arithmetic as when this was one loop here, pixel for
    // pixel): the Monitor frame of a home console covers most of the screen.
    let mut canvas = crate::frames::Canvas::new(view_w, view_h);
    canvas.paint(plate, game, |fx, fy| {
        let d = sd_round_rect(fx, fy, plate, radius);
        let cover = (0.5 - d).clamp(0.0, 1.0);
        if cover <= 0.0 {
            return None;
        }
        // Plate: vertical gradient over its height.
        let t = ((fy - plate.1) / (plate.3 - plate.1)).clamp(0.0, 1.0);
        let mut c = [0f32; 3];
        for i in 0..3 {
            c[i] = top[i] + (bottom[i] - top[i]) * t;
        }
        let mut add = 0.0;
        // Bevel along the outer edge, lit from above: the top edge catches light, the bottom one
        // falls into shadow.
        if d > -4.0 {
            let ny = (sd_round_rect(fx, fy + 1.0, plate, radius) - sd_round_rect(fx, fy - 1.0, plate, radius)) / 2.0;
            let edge = smoothstep(-4.0, -1.0, d) * (1.0 - smoothstep(-0.5, 0.5, d));
            add += edge * (0.07 * (-ny).max(0.0) + 0.018 - 0.03 * ny.max(0.0));
        }
        // Distance to the picture, and which side of it this pixel is on.
        let dx = (gx0 - fx).max(fx - gx1).max(0.0);
        let dy = (gy0 - fy).max(fy - gy1).max(0.0);
        let dh = (dx * dx + dy * dy).sqrt();
        let below = if fy > gy1 { 1.0 } else if fy < gy0 { 0.0 } else { 0.5 };
        match style {
            Style::Handheld => {
                // A faint diagonal sheen across the upper left of the lens.
                let u = (fx - plate.0) / (plate.2 - plate.0) + 0.55 * (fy - plate.1) / (plate.3 - plate.1);
                add += 0.016 * smoothstep(0.05, 0.3, u) * (1.0 - smoothstep(0.3, 0.62, u));
                // The LCD sits in a well: dark walls, the lower one lit, and a fine lip around it.
                if dh < recess {
                    let wall = 0.010 + 0.034 * below;
                    c = [wall, wall, wall * 1.08];
                    add = 0.0;
                } else if dh < recess + 1.5 {
                    add += 0.035 * below - 0.012 * (1.0 - below) + 0.01;
                }
            }
            _ => {
                if dh < gap {
                    c = [0.0, 0.0, 0.0];
                    add = 0.0;
                } else {
                    // Light from the picture falling on the bezel.
                    add += 0.03 * (-(dh - gap) / (px * 5.0)).exp();
                }
            }
        }
        if let Some((lx, ly)) = led {
            let ld = ((fx - lx).powi(2) + (fy - ly).powi(2)).sqrt();
            // A small lit light with a soft halo in the plastic around it.
            let core = 1.0 - smoothstep(led_r - 0.8, led_r + 0.6, ld);
            let halo = 0.22 * (-(ld / (led_r * 2.2)).powi(2)).exp();
            let lit = [0.86, 0.20, 0.16];
            for i in 0..3 {
                c[i] = c[i] * (1.0 - core) + lit[i] * core + lit[i] * halo * (1.0 - core);
            }
            // The light's own highlight, slightly off-centre.
            let hl = 1.0 - smoothstep(0.0, led_r * 0.55, ((fx - lx + led_r * 0.3).powi(2) + (fy - ly + led_r * 0.3).powi(2)).sqrt());
            add += 0.25 * hl * core;
        }
        Some(([c[0] + add, c[1] + add, c[2] + add], cover))
    });
    Some(canvas.done(game))
}

/// One frame of a pack: the systems it is for, its image and the hole in image pixels.
#[derive(serde::Deserialize, serde::Serialize, Clone, Debug, PartialEq)]
pub struct PackFrame {
    pub systems: Vec<String>,
    pub image: String,
    pub hole: [u32; 4],
}

/// `pack.json`. Name, author and licence are shown by the store; the frames are what we draw.
#[derive(serde::Deserialize, serde::Serialize, Clone, Debug, PartialEq)]
pub struct Pack {
    pub name: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub license: String,
    pub frames: Vec<PackFrame>,
}

/// The pack frame for `system` from the first pack (by folder name) in `frames_dir` that has one:
/// (image path, hole).
pub fn find_pack_frame(frames_dir: &Path, system: &str) -> Option<(PathBuf, Rect)> {
    let mut packs: Vec<PathBuf> = std::fs::read_dir(frames_dir).ok()?.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.is_dir()).collect();
    packs.sort();
    packs.into_iter().find_map(|dir| {
        let pack: Pack = serde_json::from_slice(&std::fs::read(dir.join("pack.json")).ok()?).ok()?;
        let f = pack.frames.into_iter().find(|f| f.systems.iter().any(|s| s == system))?;
        let [x, y, w, h] = f.hole;
        // Only plain file names: a pack can't point outside its own folder.
        (w > 0 && h > 0 && !f.image.contains('/') && !f.image.contains("..")).then(|| (dir.join(&f.image), Rect { x, y, w, h }))
    })
}

/// A PNG as premultiplied RGBA (grey, RGB, palette and 16-bit are converted).
pub fn load_png_rgba(path: &Path) -> Option<(u32, u32, Vec<u8>)> {
    // Bounded like every PNG (`image::png_reader`, review N2).
    let mut reader = crate::image::png_reader(std::io::BufReader::new(std::fs::File::open(path).ok()?))?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).ok()?;
    let data = &buf[..info.buffer_size()];
    let pm = |r: u8, g: u8, b: u8, a: u8| {
        let s = |c: u8| ((c as u32 * a as u32 + 127) / 255) as u8;
        [s(r), s(g), s(b), a]
    };
    let rgba: Vec<u8> = match info.color_type {
        png::ColorType::Rgba => data.chunks_exact(4).flat_map(|p| pm(p[0], p[1], p[2], p[3])).collect(),
        png::ColorType::Rgb => data.chunks_exact(3).flat_map(|p| [p[0], p[1], p[2], 255]).collect(),
        png::ColorType::GrayscaleAlpha => data.chunks_exact(2).flat_map(|p| pm(p[0], p[0], p[0], p[1])).collect(),
        png::ColorType::Grayscale => data.iter().flat_map(|g| [*g, *g, *g, 255]).collect(),
        png::ColorType::Indexed => return None,
    };
    Some((info.width, info.height, rgba))
}

/// Writes `bezel` as a one-frame pack (`pack.json` and `<system>.png`): a template for frame
/// designers and a test pack for the loader. The PNG is straight (not premultiplied) alpha.
pub fn write_pack(dir: &Path, name: &str, system: &str, bezel: &Bezel) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let file = format!("{system}.png");
    let straight: Vec<u8> = bezel
        .rgba
        .chunks_exact(4)
        .flat_map(|p| {
            let a = p[3] as u32;
            let s = |c: u8| if a == 0 { 0 } else { ((c as u32 * 255 + a / 2) / a).min(255) as u8 };
            [s(p[0]), s(p[1]), s(p[2]), p[3]]
        })
        .collect();
    let mut enc = png::Encoder::new(std::io::BufWriter::new(std::fs::File::create(dir.join(&file))?), bezel.w, bezel.h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().and_then(|mut w| w.write_image_data(&straight)).map_err(std::io::Error::other)?;
    let h = bezel.hole;
    let pack = Pack {
        name: name.to_string(),
        author: "pocket-ui".into(),
        license: "MIT".into(),
        frames: vec![PackFrame { systems: vec![system.to_string()], image: file, hole: [h.x, h.y, h.w, h.h] }],
    };
    std::fs::write(dir.join("pack.json"), serde_json::to_vec_pretty(&pack).map_err(std::io::Error::other)?)
}

/// CPU stand-in for the GPU path, for previews on the desktop: black screen, the frame placed on
/// `game`, the picture scaled nearest-neighbour into it (no sharpness or effect). RGBA, opaque.
pub fn compose(view_w: u32, view_h: u32, game: Rect, bezel: Option<&Bezel>, pic_w: u32, pic_h: u32, pic: &[u32]) -> Vec<u8> {
    let mut out = vec![0u8; (view_w * view_h * 4) as usize];
    for p in out.chunks_exact_mut(4) {
        p[3] = 255;
    }
    if let Some(b) = bezel {
        let (px, py, pw, ph) = b.placement(game);
        for y in 0..view_h {
            let sy = ((y as f32 + 0.5 - py) / ph * b.h as f32).floor();
            if sy < 0.0 || sy >= b.h as f32 {
                continue;
            }
            for x in 0..view_w {
                let sx = ((x as f32 + 0.5 - px) / pw * b.w as f32).floor();
                if sx < 0.0 || sx >= b.w as f32 {
                    continue;
                }
                let s = ((sy as u32 * b.w + sx as u32) * 4) as usize;
                let o = ((y * view_w + x) * 4) as usize;
                out[o..o + 3].copy_from_slice(&b.rgba[s..s + 3]);
            }
        }
    }
    if pic_w > 0 && pic_h > 0 {
        for y in game.y..(game.y + game.h).min(view_h) {
            let sy = ((y - game.y) as u64 * pic_h as u64 / game.h as u64) as u32;
            for x in game.x..(game.x + game.w).min(view_w) {
                let sx = ((x - game.x) as u64 * pic_w as u64 / game.w as u64) as u32;
                let p = pic[(sy * pic_w + sx) as usize];
                let o = ((y * view_w + x) * 4) as usize;
                out[o..o + 3].copy_from_slice(&[(p >> 16) as u8, (p >> 8) as u8, p as u8]);
            }
        }
    }
    // The inlay goes over the picture (built-in frames only, drawn 1:1).
    if let Some((b, r)) = bezel.and_then(|b| b.inlay.map(|r| (b, r))).filter(|(b, _)| (b.w, b.h) == (view_w, view_h)) {
        for y in r.y..(r.y + r.h).min(view_h) {
            let o = ((y * view_w + r.x) * 4) as usize;
            let n = (r.w.min(view_w - r.x) * 4) as usize;
            for (d, s) in out[o..o + n].chunks_exact_mut(4).zip(b.rgba[o..o + n].chunks_exact(4)) {
                d[..3].copy_from_slice(&s[..3]);
            }
        }
    }
    out
}

const VS: &str = include_str!("shaders/video.vert");
pub(crate) const FS: &str = include_str!("shaders/bezel.frag");

/// Compiles and links a vertex and a fragment shader (GLSL ES 3.00, see `video::glsl_for`).
pub(crate) fn compile(gl: &glow::Context, vs: &str, fs: &str) -> Result<glow::Program, String> {
    unsafe {
        let program = gl.create_program()?;
        let mut shaders = Vec::new();
        for (kind, src) in [(glow::VERTEX_SHADER, vs), (glow::FRAGMENT_SHADER, fs)] {
            let sh = gl.create_shader(kind)?;
            gl.shader_source(sh, &crate::video::glsl_for(gl, src));
            gl.compile_shader(sh);
            if !gl.get_shader_compile_status(sh) {
                let log = gl.get_shader_info_log(sh);
                gl.delete_shader(sh);
                gl.delete_program(program);
                return Err(log);
            }
            gl.attach_shader(program, sh);
            shaders.push(sh);
        }
        gl.link_program(program);
        let ok = gl.get_program_link_status(program);
        for sh in shaders {
            gl.detach_shader(program, sh);
            gl.delete_shader(sh);
        }
        if !ok {
            let log = gl.get_program_info_log(program);
            gl.delete_program(program);
            return Err(log);
        }
        Ok(program)
    }
}

/// The four bands around `game` (top-left origin), as screen rectangles x, y, w, h; empty ones left out.
pub fn bands_around(view_w: u32, view_h: u32, game: Rect) -> Vec<(u32, u32, u32, u32)> {
    let (x1, y1) = ((game.x + game.w).min(view_w), (game.y + game.h).min(view_h));
    [(0, 0, view_w, game.y), (0, y1, view_w, view_h - y1), (0, game.y, game.x, y1 - game.y), (x1, game.y, view_w - x1, y1 - game.y)]
        .into_iter()
        .filter(|r| r.2 > 0 && r.3 > 0)
        .collect()
}

/// Draws the bound program's quad on the bands around `game` only (scissored): whatever is drawn
/// behind the picture costs nothing where the picture covers it.
pub(crate) fn draw_around(gl: &glow::Context, view_w: u32, view_h: u32, game: Rect) {
    unsafe {
        gl.enable(glow::SCISSOR_TEST);
        for (x, y, w, h) in bands_around(view_w, view_h, game) {
            gl.scissor(x as i32, (view_h - y - h) as i32, w as i32, h as i32);
            gl.draw_arrays(glow::TRIANGLE_STRIP, 0, 4);
        }
        gl.disable(glow::SCISSOR_TEST);
    }
}

/// What the texture holds.
#[derive(Clone, PartialEq, Debug)]
enum Source {
    Builtin { style: Style, view: (u32, u32), game: Rect, px_milli: u32 },
    Pack(PathBuf),
}

/// A frame being prepared on a thread (`Layer::set_async`): what it is for, where it arrives, when it was asked for.
type Job = (Source, std::sync::mpsc::Receiver<Option<std::sync::Arc<Bezel>>>, std::time::Instant);

/// The frame on the GPU: one program, one texture, rebuilt only when the picture's geometry, the
/// console or the chosen frame changes. A frame is read from the RAM cache (`frame_cache`) when it was
/// drawn before, else drawn and kept there.
pub struct Layer {
    system: String,
    frames_dir: PathBuf,
    program: Option<(glow::Program, Option<glow::UniformLocation>, Option<glow::UniformLocation>)>,
    texture: Option<glow::Texture>,
    loaded: Option<Source>,
    /// Size and hole of what is in the texture; None when the source has nothing to draw.
    bezel: Option<(u32, u32, Rect)>,
    /// The pack frame for this console, looked up once.
    pack: Option<Option<(PathBuf, Rect)>>,
    /// The frame's power light, what it shows now, and the clock that decides (made on first use).
    led: Option<(crate::frame_led::Led, Option<crate::frame_led::Look>)>,
    clock: Option<crate::frame_led::Clock>,
    /// Drawn after the picture (`Bezel::inlay`).
    inlay: Option<Rect>,
    /// Prepare a new frame on a thread and draw none until it is there (`set_async`).
    asynchronous: bool,
    job: Option<Job>,
    /// The cache's folder (`frame_cache::dir`), looked up once; None inside: no cache.
    cache: Option<Option<PathBuf>>,
}

impl Layer {
    pub fn new() -> Self {
        Layer {
            system: String::new(),
            frames_dir: PathBuf::new(),
            program: None,
            texture: None,
            loaded: None,
            bezel: None,
            pack: None,
            led: None,
            clock: None,
            inlay: None,
            asynchronous: false,
            job: None,
            cache: None,
        }
    }

    /// On: a frame that isn't ready is prepared (read from the cache or drawn) on a thread, and `draw` draws no frame
    /// until it is there, a frame or two later, instead of holding the picture up for it. pocket-core turns this on
    /// for a game's first moments, when the frame is still under the veil it fades in from (`Overlay::draw_veil`, which
    /// starts once the frame is there), and off again after: a frame asked for later (the pause menu's preview, a new
    /// picture size) is waited for, as before. Returns the previous setting.
    pub fn set_async(&mut self, on: bool) -> bool {
        std::mem::replace(&mut self.asynchronous, on)
    }

    /// The console being played and where frame packs are installed.
    pub fn set_source(&mut self, system: &str, frames_dir: &Path) {
        self.system = system.to_string();
        self.frames_dir = frames_dir.to_path_buf();
        self.pack = None;
        self.loaded = None;
    }

    /// The console being played.
    pub fn system(&self) -> &str {
        &self.system
    }

    /// Draws the frame for a picture at `game` (viewport pixels). `px` is output pixels per game
    /// pixel. `fixed` is a built-in style chosen in the menu; None is Auto: the installed pack's
    /// frame for the console, else its own built-in style. False while the frame is being prepared
    /// on a thread (`set_async`): nothing was drawn, the bands stay black.
    pub fn draw(&mut self, gl: &glow::Context, view_w: u32, view_h: u32, game: Rect, px: f32, fixed: Option<Style>) -> bool {
        if self.program.is_none() {
            match compile(gl, VS, FS) {
                Ok(p) => unsafe {
                    let (rect, tex) = (gl.get_uniform_location(p, "rect"), gl.get_uniform_location(p, "tex"));
                    self.program = Some((p, rect, tex));
                },
                Err(log) => {
                    eprintln!("bezel: shader failed: {log}");
                    return true;
                }
            }
        }
        if self.pack.is_none() {
            self.pack = Some(find_pack_frame(&self.frames_dir, &self.system));
        }
        let builtin = |style| Source::Builtin { style, view: (view_w, view_h), game, px_milli: (px * 1000.0) as u32 };
        let source = match (fixed, self.pack.as_ref().and_then(|p| p.as_ref())) {
            (Some(style), _) => builtin(style),
            (None, Some((path, _))) => Source::Pack(path.clone()),
            (None, None) => builtin(style_for(&self.system)),
        };
        if self.loaded.as_ref() != Some(&source) {
            let recipe = match &source {
                Source::Pack(path) => {
                    let hole = self.pack.as_ref().and_then(|p| p.as_ref()).map(|(_, h)| *h).unwrap();
                    crate::frame_cache::Recipe::Pack { path: path.clone(), hole }
                }
                Source::Builtin { style, .. } => crate::frame_cache::Recipe::Builtin { style: *style, view: (view_w, view_h), game, px },
            };
            let Some(image) = self.prepare(&source, recipe) else { return false };
            let t0 = std::time::Instant::now();
            self.upload(gl, image.as_deref(), matches!(source, Source::Pack(_)));
            self.led = image.as_ref().and_then(|b| b.led.clone()).map(|l| (l, None));
            self.inlay = image.as_ref().and_then(|b| b.inlay);
            self.bezel = image.map(|b| (b.w, b.h, b.hole));
            self.loaded = Some(source);
            eprintln!("bezel: uploaded in {:.1} ms", t0.elapsed().as_secs_f32() * 1000.0);
        }
        self.update_light(gl);
        if self.bind(gl, view_w, view_h, game) {
            // The picture covers its own rectangle, so only the bands around it are drawn.
            draw_around(gl, view_w, view_h, game);
        }
        true
    }

    /// The frame for `source`: from the cache or drawn, here or on a thread (`set_async`). None while a thread is
    /// still on it; Some(None) when there is nothing to draw (no room, an unreadable pack).
    fn prepare(&mut self, source: &Source, recipe: crate::frame_cache::Recipe) -> Option<Option<std::sync::Arc<Bezel>>> {
        use std::sync::{mpsc, Arc};
        if let Some((for_source, rx, asked)) = self.job.take() {
            if for_source == *source {
                let got = if self.asynchronous {
                    match rx.try_recv() {
                        Ok(b) => b,
                        Err(mpsc::TryRecvError::Empty) => {
                            self.job = Some((for_source, rx, asked));
                            return None;
                        }
                        Err(mpsc::TryRecvError::Disconnected) => None,
                    }
                } else {
                    rx.recv().ok().flatten()
                };
                eprintln!("bezel: ready {:.1} ms after it was asked for", asked.elapsed().as_secs_f32() * 1000.0);
                return Some(got);
            }
            // For an earlier picture (sway's resize to the full screen came in): not wanted any more; it still lands in
            // the cache.
        }
        let cache = self.cache.get_or_insert_with(crate::frame_cache::dir).clone();
        if self.asynchronous {
            let (tx, rx) = mpsc::channel();
            let recipe2 = recipe.clone();
            let cache2 = cache.clone();
            let spawned = std::thread::Builder::new().name("frame".into()).spawn(move || {
                // Behind the game's own threads while it starts (the drawing threads inherit this).
                #[cfg(target_os = "linux")]
                unsafe {
                    libc::setpriority(libc::PRIO_PROCESS, libc::syscall(libc::SYS_gettid) as libc::id_t, 10);
                }
                let (b, keep) = crate::frame_cache::fetch(&recipe2, cache2.as_deref());
                let b = b.map(Arc::new);
                let _ = tx.send(b.clone());
                if let (Some(b), Some(key), Some(dir)) = (b, keep, cache2) {
                    crate::frame_cache::store(&dir, &key, &b);
                }
            });
            if spawned.is_ok() {
                self.job = Some((source.clone(), rx, std::time::Instant::now()));
                return None;
            }
        }
        let (b, keep) = crate::frame_cache::fetch(&recipe, cache.as_deref());
        let b = b.map(Arc::new);
        // Writing it down is no reason to wait.
        if let (Some(b), Some(key), Some(dir)) = (b.clone(), keep, cache) {
            let _ = std::thread::Builder::new().name("frame-keep".into()).spawn(move || crate::frame_cache::store(&dir, &key, &b));
        }
        Some(b)
    }

    /// Binds the program and texture with the frame placed on `game`; false when there is no frame.
    fn bind(&self, gl: &glow::Context, view_w: u32, view_h: u32, game: Rect) -> bool {
        let (Some((w, h, hole)), Some(texture), Some((program, rect, tex))) = (self.bezel, self.texture, self.program.as_ref()) else { return false };
        let (x, y, pw, ph) = Bezel::new(w, h, Vec::new(), hole).placement(game);
        let (vw, vh) = (view_w as f32, view_h as f32);
        let ndc = |p: f32, total: f32| p / total * 2.0 - 1.0;
        unsafe {
            gl.use_program(Some(*program));
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, Some(texture));
            gl.uniform_4_f32(rect.as_ref(), ndc(x, vw), -ndc(y + ph, vh), ndc(x + pw, vw), -ndc(y, vh));
            gl.uniform_1_i32(tex.as_ref(), 0);
        }
        true
    }

    /// Draws the frame's inlay over the picture (after it), if it has one: a few rows per frame.
    pub fn draw_inlay(&self, gl: &glow::Context, view_w: u32, view_h: u32, game: Rect) {
        let Some(r) = self.inlay else { return };
        // Only where the frame was drawn for this very picture (a built-in frame, 1:1).
        if self.bezel.map(|b| b.2) != Some(game) || !self.bind(gl, view_w, view_h, game) {
            return;
        }
        unsafe {
            gl.enable(glow::SCISSOR_TEST);
            gl.scissor(r.x as i32, (view_h - r.y - r.h) as i32, r.w as i32, r.h as i32);
            gl.draw_arrays(glow::TRIANGLE_STRIP, 0, 4);
            gl.disable(glow::SCISSOR_TEST);
        }
    }

    /// A state was saved: the power light flashes (frames without one ignore it).
    pub fn flash_light(&mut self) {
        self.clock.get_or_insert_with(|| crate::frame_led::Clock::new(crate::battery::dir())).flash();
    }

    /// Repaints the power light's square when what it shows changed (battery level, blink, flash):
    /// one small `glTexSubImage2D`, the rest of the frame stays as it is.
    fn update_light(&mut self, gl: &glow::Context) {
        let (Some((led, shown)), Some(texture), Some((_, _, hole))) = (self.led.as_mut(), self.texture, self.bezel) else { return };
        let look = self.clock.get_or_insert_with(|| crate::frame_led::Clock::new(crate::battery::dir())).look(std::time::Instant::now());
        if *shown == Some(look) {
            return;
        }
        let patch = crate::frame_led::paint(led, look, hole);
        let r = led.rect;
        unsafe {
            gl.bind_texture(glow::TEXTURE_2D, Some(texture));
            gl.pixel_store_i32(glow::UNPACK_ROW_LENGTH, 0);
            gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 4);
            gl.tex_sub_image_2d(glow::TEXTURE_2D, 0, r.x as i32, r.y as i32, r.w as i32, r.h as i32, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelUnpackData::Slice(&patch));
        }
        *shown = Some(look);
    }

    fn upload(&mut self, gl: &glow::Context, image: Option<&Bezel>, smooth: bool) {
        let Some(b) = image else { return };
        unsafe {
            if self.texture.is_none() {
                self.texture = gl.create_texture().ok();
            }
            gl.bind_texture(glow::TEXTURE_2D, self.texture);
            // The game frame upload leaves its own row length set.
            gl.pixel_store_i32(glow::UNPACK_ROW_LENGTH, 0);
            gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 4);
            gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::RGBA8 as i32, b.w as i32, b.h as i32, 0, glow::RGBA, glow::UNSIGNED_BYTE, Some(&b.rgba));
            // Built-in frames are drawn 1:1; packs are scaled.
            let filter = if smooth { glow::LINEAR } else { glow::NEAREST } as i32;
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, filter);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, filter);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
        }
    }
}

impl Default for Layer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;

    const GBA_5X: Rect = Rect { x: 20, y: 140, w: 1200, h: 800 };
    const GB_7X: Rect = Rect { x: 60, y: 36, w: 1120, h: 1008 };
    const SNES_4X: Rect = Rect { x: 22, y: 92, w: 1195, h: 896 };

    fn alpha(b: &Bezel, x: u32, y: u32) -> u8 {
        b.rgba[((y * b.w + x) * 4 + 3) as usize]
    }

    #[test]
    fn built_in_frames_surround_the_picture_and_leave_the_edge_black() {
        for (style, game, px) in [(Style::Handheld, GBA_5X, 5.0), (Style::Handheld, GB_7X, 7.0), (Style::Monitor, SNES_4X, 4.0)] {
            let b = generate(style, 1240, 1080, game, px).expect("room for a frame");
            assert_eq!((b.w, b.h, b.hole), (1240, 1080, game));
            // The pixel right above and right left of the picture belong to the frame.
            assert!(alpha(&b, game.x + game.w / 2, game.y - 1) == 255, "{style:?} above");
            assert!(alpha(&b, game.x - 1, game.y + game.h / 2) == 255, "{style:?} left");
            // The panel edge stays black on every side.
            for (x, y) in [(0, 540), (1239, 540), (620, 0), (620, 1079)] {
                assert_eq!(alpha(&b, x, y), 0, "{style:?} edge {x},{y}");
            }
            // Nothing is drawn where the picture goes.
            assert_eq!(alpha(&b, game.x + 5, game.y + 5), 0);
        }
    }

    const GBA_4X: Rect = Rect { x: 140, y: 220, w: 960, h: 640 };
    const NES_3X: Rect = Rect { x: 140, y: 180, w: 960, h: 720 };
    const SNES_3X: Rect = Rect { x: 172, y: 204, w: 896, h: 672 };

    fn all_styles() -> Vec<Style> {
        use Colourway::*;
        let mut v = vec![Style::Handheld, Style::Monitor, Style::Arcade, Style::Glow(0x5b4aa6), Style::Crt(Cabinet::Plastic), Style::Crt(Cabinet::Wood)];
        v.extend([Grey, Indigo, ClearPurple, AtomicPurple, Black, White, GlacierBlue].map(Style::Shell));
        v.extend(new_styles());
        v
    }

    /// The frames of `frames_more.rs` and `frames_tv.rs`, every finish.
    fn new_styles() -> Vec<Style> {
        let mut v: Vec<Style> = BrickColour::ALL.iter().map(|&c| Style::Brick(c)).collect();
        v.extend(SlimFinish::ALL.iter().map(|&f| Style::PocketSlim(f)));
        v.extend(ClamFinish::ALL.iter().map(|&f| Style::Clamshell(f)));
        v.extend(SwanFinish::ALL.iter().map(|&f| Style::Swan(f)));
        v.extend(WatchFinish::ALL.iter().map(|&f| Style::Watch(f)));
        v.extend([Style::Wide, Style::Twin, Style::Gloss, Style::Goggles, Style::Studio, Style::PortableTv, Style::Woodgrain]);
        v
    }

    fn in_rect(r: Rect, x: u32, y: u32) -> bool {
        x >= r.x && x < r.x + r.w && y >= r.y && y < r.y + r.h
    }

    /// Frames at the experimental siblings' sizes (docs/DEVICES.md): none where the picture fills the Mini V1's
    /// 1280x960 (320x240 at 4x), and around a GBA at 6x or a Game Boy at 7x on the Pocket 5's 1920x1080 they keep off
    /// the picture.
    #[test]
    fn frames_at_the_pocket_5s_and_the_mini_v1s_sizes() {
        let full = Rect { x: 0, y: 0, w: 1280, h: 960 };
        for style in new_styles() {
            assert!(generate(style, 1280, 960, full, 4.0).is_none(), "{style:?}: no room around a full screen");
        }
        let gba = Rect { x: 240, y: 60, w: 1440, h: 960 };
        let gb = Rect { x: 400, y: 36, w: 1120, h: 1008 };
        for style in new_styles() {
            for (game, px) in [(gba, 6.0), (gb, 7.0)] {
                let Some(b) = generate(style, 1920, 1080, game, px) else { continue };
                assert_eq!((b.w, b.h, b.hole), (1920, 1080, game), "{style:?}");
                for (x, y) in [(game.x, game.y), (game.x + game.w - 1, game.y + game.h - 1), (game.x + game.w / 2, game.y + game.h / 2)] {
                    assert_eq!(alpha(&b, x, y), 0, "{style:?} {game:?} drew on the picture at {x},{y}");
                }
            }
        }
    }

    #[test]
    fn new_frames_never_draw_on_the_picture_and_keep_the_panel_edge_black() {
        // Native and one step smaller for GB, GBA, SNES; a PSP at Aspect (no room at the sides) and
        // at 2x; a WonderSwan, a Virtual Boy, a tall Atari picture and a small one.
        let cases = [
            (GB_7X, 7.0),
            (Rect { x: 140, y: 108, w: 960, h: 864 }, 6.0),
            (GBA_5X, 5.0),
            (GBA_4X, 4.0),
            (SNES_4X, 4.0),
            (SNES_3X, 3.0),
            (NES_3X, 3.0),
            (Rect { x: 0, y: 188, w: 1240, h: 703 }, 703.0 / 272.0),
            (Rect { x: 140, y: 268, w: 960, h: 544 }, 2.0),
            (Rect { x: 60, y: 180, w: 1120, h: 720 }, 5.0),
            (Rect { x: 44, y: 204, w: 1152, h: 672 }, 3.0),
            (Rect { x: 220, y: 15, w: 800, h: 1050 }, 5.0),
            (Rect { x: 300, y: 320, w: 640, h: 440 }, 2.0),
        ];
        for style in new_styles() {
            for (game, px) in cases {
                let b = generate(style, 1240, 1080, game, px).unwrap_or_else(|| panic!("{style:?} has room at {game:?}"));
                assert_eq!((b.w, b.h, b.hole), (1240, 1080, game), "{style:?}: the hole is the picture");
                assert!(b.inlay.is_none(), "{style:?} {game:?}: only stacked DS screens have an inlay");
                for y in game.y..game.y + game.h {
                    for x in game.x..game.x + game.w {
                        assert_eq!(alpha(&b, x, y), 0, "{style:?} {game:?} drew on the picture at {x},{y}");
                    }
                }
                // The panel's edge stays black beside the picture's middle wherever the picture leaves room.
                let (cx, cy) = (game.x + game.w / 2, game.y + game.h / 2);
                for (x, y, room) in [(0, cy, game.x), (1239, cy, 1240 - game.x - game.w), (cx, 0, game.y), (cx, 1079, 1080 - game.y - game.h)] {
                    if room > 0 {
                        assert_eq!(alpha(&b, x, y), 0, "{style:?} {game:?} edge {x},{y}");
                    }
                }
                for (x, y) in [(0, 0), (1239, 0), (0, 1079), (1239, 1079)] {
                    assert_eq!(alpha(&b, x, y), 0, "{style:?} {game:?} corner {x},{y}");
                }
                // Reuse this render for the band and power-light contracts instead of regenerating the same
                // style/geometry in two further tests.
                if [GBA_5X, GB_7X, SNES_4X, GBA_4X, NES_3X, SNES_3X].contains(&game) {
                    assert_bands_drawn(style, game, &b);
                }
                if [Rect { x: 140, y: 108, w: 960, h: 864 }, GBA_4X, Rect { x: 140, y: 268, w: 960, h: 544 }].contains(&game) {
                    let lit = matches!(style, Style::Brick(_) | Style::PocketSlim(_) | Style::Clamshell(_) | Style::Wide | Style::Gloss | Style::Swan(_));
                    assert_eq!(b.led.is_some(), lit, "{style:?} {game:?}");
                }
                if game == GB_7X && matches!(style, Style::Brick(BrickColour::Grey) | Style::PocketSlim(SlimFinish::Clear)) {
                    assert!(b.led.is_some(), "{style:?}");
                }
                if game == GB_7X && style == Style::Brick(BrickColour::Grey) {
                    let led = b.led.as_ref().unwrap();
                    let i = (((led.spot.y as u32) * b.w + led.spot.x as u32) * 4) as usize;
                    assert!(b.rgba[i + 1] > 150 && b.rgba[i + 1] > b.rgba[i] + 40, "green: {:?}", &b.rgba[i..i + 4]);
                    let j = (((led.spot.y as u32 - led.rect.y) * led.rect.w + (led.spot.x as u32 - led.rect.x)) * 4) as usize;
                    assert!(led.base[j + 1] < 150, "the frame under the light is not green: {:?}", &led.base[j..j + 4]);
                }
                if let Some(led) = &b.led {
                    // The light is in the frame, clear of the picture, and its square inside the image.
                    let (lx, ly) = (led.spot.x as u32, led.spot.y as u32);
                    assert!(!in_rect(game, lx, ly) && alpha(&b, lx, ly) == 255, "{style:?} {game:?} light at {lx},{ly}");
                    assert!(led.rect.x + led.rect.w <= 1240 && led.rect.y + led.rect.h <= 1080);
                }
            }
        }
    }

    #[test]
    fn twin_puts_its_hinge_exactly_in_the_gap_between_stacked_screens() {
        // 256x384 at Aspect: 720x1080, no gap. The hinge is beside the picture only.
        let px = 1080.0 / 384.0;
        let game = Rect { x: 260, y: 0, w: 720, h: 1080 };
        let b = generate(Style::Twin, 1240, 1080, game, px).unwrap();
        assert!(b.inlay.is_none());
        assert!(alpha(&b, game.x - 2, 540) == 255 && alpha(&b, game.x + game.w + 2, 540) == 255, "the hinge beside the seam");
        assert!((game.y..game.y + game.h).all(|y| (game.x..game.x + game.w).all(|x| alpha(&b, x, y) == 0)));
        // A 64-line gap (256x448): the inlay is the gap's rows, the picture outside them untouched.
        let px = 1080.0 / 448.0;
        let w = (256.0 * px) as u32;
        let game = Rect { x: (1240 - w) / 2, y: 0, w, h: 1080 };
        let b = generate(Style::Twin, 1240, 1080, game, px).unwrap();
        let inlay = b.inlay.expect("an inlay in the gap");
        let (top, bottom) = (192.0 * px, (192.0 + 64.0) * px);
        assert_eq!((inlay.x, inlay.w), (game.x, game.w));
        assert!(inlay.y as f32 >= top && (inlay.y + inlay.h) as f32 <= bottom && inlay.h as f32 >= bottom - top - 2.0, "{inlay:?}");
        for y in game.y..game.y + game.h {
            for x in (game.x..game.x + game.w).step_by(7) {
                let a = alpha(&b, x, y);
                if in_rect(inlay, x, y) {
                    assert_eq!(a, 255, "the inlay is opaque at {x},{y}");
                } else {
                    assert_eq!(a, 0, "drew on a screen at {x},{y}");
                }
            }
        }
        // Side by side or one screen: the plain lens.
        let side = Rect { x: 0, y: 307, w: 1240, h: 465 };
        let b = generate(Style::Twin, 1240, 1080, side, 465.0 / 192.0).unwrap();
        assert!(b.inlay.is_none() && b.led.is_none());
        assert_eq!(alpha(&b, 620, 306), 255);
        // The CPU preview puts the inlay over the picture too.
        let game = Rect { x: (1240 - w) / 2, y: 0, w, h: 1080 };
        let b = generate(Style::Twin, 1240, 1080, game, px).unwrap();
        let pic = vec![0x00ff_ffffu32; 256 * 448];
        let out = compose(1240, 1080, game, Some(&b), 256, 448, &pic);
        let at = |x: u32, y: u32| out[((y * 1240 + x) * 4) as usize];
        let inlay = b.inlay.unwrap();
        assert_ne!(at(620, inlay.y + inlay.h / 2), 255, "the hinge over the gap");
        assert_eq!(at(620, 100), 255, "the screen");
    }

    fn near(a: [f32; 4], b: [f32; 4]) -> bool {
        a.iter().zip(b).all(|(a, b)| (a - b).abs() < 0.01)
    }

    #[test]
    fn border_sizes_per_style_and_scale() {
        // The documented table: Handheld at GBA 5x and GB 7x.
        assert!(near(bands(Style::Handheld, 1240, 1080, GBA_5X), [16.0, 16.0, 68.0, 68.0]));
        assert!(near(bands(Style::Handheld, 1240, 1080, GB_7X), [51.0, 51.0, 30.6, 30.6]));
        // One step smaller (GBA 4x) gives every side room: the lens reaches its full width.
        assert!(near(bands(Style::Handheld, 1240, 1080, GBA_4X), [54.4; 4]));
        // A shell's body is wider: as far as the panel allows at the sides, 26 % of the height above and below.
        assert!(near(bands(Style::Shell(Colourway::Grey), 1240, 1080, GBA_4X), [124.0, 124.0, 166.4, 166.4]));
        assert!(near(bands(Style::Crt(Cabinet::Wood), 1240, 1080, SNES_3X), [107.52, 107.52, 107.52, 107.52]));
        assert!(near(bands(Style::Arcade, 1240, 1080, NES_3X), [93.6; 4]));
        // Glow stays close to the picture.
        assert!(near(bands(Style::Glow(0xffffff), 1240, 1080, GBA_4X), [38.4; 4]));
    }

    fn assert_bands_drawn(style: Style, game: Rect, b: &Bezel) {
        // Each side has something drawn within its band, close to the picture.
        let [l, r, t, bt] = bands(style, 1240, 1080, game);
        let (cx, cy) = (game.x + game.w / 2, game.y + game.h / 2);
        let lit = |pts: Vec<(u32, u32)>| pts.into_iter().any(|(x, y)| alpha(b, x, y) > 0);
        assert!(lit((1..=l as u32).map(|d| (game.x - d, cy)).collect()), "{style:?} {game:?} left");
        assert!(lit((0..r as u32).map(|d| (game.x + game.w + d, cy)).collect()), "{style:?} {game:?} right");
        assert!(lit((1..=t as u32).map(|d| (cx, game.y - d)).collect()), "{style:?} {game:?} top");
        assert!(lit((0..bt as u32).map(|d| (cx, game.y + game.h + d)).collect()), "{style:?} {game:?} bottom");
    }

    #[test]
    fn classic_styles_surround_the_picture_at_every_scale() {
        for style in all_styles().into_iter().filter(|s| !new_styles().contains(s)) {
            for (game, px) in [(GBA_5X, 5.0), (GB_7X, 7.0), (SNES_4X, 4.0), (GBA_4X, 4.0), (NES_3X, 3.0), (SNES_3X, 3.0)] {
                let b = generate(style, 1240, 1080, game, px).unwrap_or_else(|| panic!("{style:?} has room at {game:?}"));
                assert_eq!((b.w, b.h, b.hole), (1240, 1080, game), "the hole is the picture");
                // Nothing is drawn where the picture goes, and the panel edge stays black.
                assert_eq!(alpha(&b, game.x + 5, game.y + 5), 0);
                assert_eq!(alpha(&b, game.x + game.w / 2, game.y + game.h - 1), 0);
                for (x, y) in [(0, 540), (1239, 540), (620, 0), (620, 1079), (0, 0), (1239, 1079)] {
                    assert_eq!(alpha(&b, x, y), 0, "{style:?} {game:?} edge {x},{y}");
                }
                assert_bands_drawn(style, game, &b);
            }
        }
    }

    #[test]
    fn colourways_differ_and_translucent_ones_show_their_insides() {
        // Middle of the band below the picture at GBA 4x: plastic, away from the lens.
        let at = |c: Colourway| {
            let b = generate(Style::Shell(c), 1240, 1080, GBA_4X, 4.0).unwrap();
            let i = (((GBA_4X.y + GBA_4X.h + 120) * b.w + 620) * 4) as usize;
            [b.rgba[i], b.rgba[i + 1], b.rgba[i + 2]]
        };
        use Colourway::*;
        let all = [Grey, Indigo, ClearPurple, AtomicPurple, Black, White, GlacierBlue].map(at);
        for i in 0..all.len() {
            for j in i + 1..all.len() {
                assert_ne!(all[i], all[j], "{i} {j}");
            }
        }
        let luma = |p: [u8; 3]| p[0] as u32 + p[1] as u32 + p[2] as u32;
        assert!(luma(all[5]) > luma(all[0]) && luma(all[0]) > luma(all[4]), "white > grey > black");
        assert!(all[1][2] > all[1][0] && all[3][2] > all[3][1], "indigo and purple lean blue/violet");
    }

    #[test]
    fn glow_takes_the_console_colour_and_stays_dark_at_a_distance() {
        let b = generate(Style::Glow(0xc8102e), 1240, 1080, GBA_4X, 4.0).unwrap();
        // The brightest pixel left of the picture is red.
        let row = GBA_4X.y + GBA_4X.h / 2;
        let best = (GBA_4X.x - 40..GBA_4X.x).map(|x| ((row * b.w + x) * 4) as usize).max_by_key(|&i| b.rgba[i]).unwrap();
        assert!(b.rgba[best] > 120 && b.rgba[best] > 2 * b.rgba[best + 1], "{:?}", &b.rgba[best..best + 4]);
        // Far from the picture it is black.
        assert_eq!(alpha(&b, GBA_4X.x - 60, row), 0);
    }

    #[test]
    fn frames_are_drawn_on_the_four_bands_around_the_picture_only() {
        for game in [GBA_5X, GB_7X, GBA_4X, NES_3X, Rect { x: 0, y: 191, w: 1240, h: 698 }] {
            let bands = bands_around(1240, 1080, game);
            let area: u32 = bands.iter().map(|r| r.2 * r.3).sum();
            assert_eq!(area, 1240 * 1080 - game.w * game.h, "{game:?}");
            for (x, y, w, h) in &bands {
                let overlaps = *x < game.x + game.w && game.x < x + w && *y < game.y + game.h && game.y < y + h;
                assert!(!overlaps, "{game:?}: band {x},{y} {w}x{h} covers the picture");
            }
        }
        // A picture that fills the screen leaves nothing to draw.
        assert!(bands_around(1240, 1080, Rect { x: 0, y: 0, w: 1240, h: 1080 }).is_empty());
    }

    #[test]
    fn the_shelf_stripe_colours_are_the_glow_colours() {
        assert_eq!(system_colour("gba"), 0x5b4aa6);
        assert_eq!(system_colour("nes"), 0xc8102e);
        assert_eq!(system_colour("famicom"), system_colour("nes"));
        assert_eq!(system_colour("unknownsys"), system_colour("unknownsys"));
    }

    #[test]
    fn no_frame_when_the_picture_fills_the_screen() {
        assert!(generate(Style::Handheld, 1240, 1080, Rect { x: 0, y: 0, w: 1240, h: 1080 }, 7.0).is_none());
        assert!(generate(Style::Monitor, 1240, 1080, Rect { x: 2, y: 3, w: 1236, h: 1074 }, 4.0).is_none());
    }

    #[test]
    fn placement_maps_the_hole_onto_the_picture() {
        let b = Bezel::new(300, 200, Vec::new(), Rect { x: 30, y: 20, w: 240, h: 160 });
        let (x, y, w, h) = b.placement(GBA_5X);
        assert_eq!((x, y, w, h), (20.0 - 150.0, 140.0 - 100.0, 1500.0, 1000.0));
    }

    #[test]
    fn styles_by_console() {
        for (system, style) in [
            ("gb", Style::Brick(BrickColour::Grey)),
            ("gbc", Style::PocketSlim(SlimFinish::Clear)),
            ("gba", Style::Wide),
            ("nds", Style::Twin),
            ("psp", Style::Gloss),
            ("wonderswan", Style::Swan(SwanFinish::Pastel)),
            ("wonderswancolor", Style::Swan(SwanFinish::Skeleton)),
            ("gameandwatch", Style::Watch(WatchFinish::Gold)),
            ("virtualboy", Style::Goggles),
            ("atari2600", Style::Woodgrain),
            ("atari7800", Style::Woodgrain),
            // Other handhelds keep the lens, home consoles the monitor surround.
            ("gamegear", Style::Handheld),
            ("ngpc", Style::Handheld),
            ("atarilynx", Style::Handheld),
            ("pokemini", Style::Handheld),
            ("snes", Style::Monitor),
            ("nes", Style::Monitor),
            ("megadrive", Style::Monitor),
            ("psx", Style::Monitor),
        ] {
            assert_eq!(style_for(system), style, "{system}");
        }
    }

    /// Every style at the Native and one-smaller geometries of GB, GBA and SNES, with a fingerprint of
    /// everything a frame holds (pixels, hole, light, inlay) and how long it took. Not run by default:
    /// `cargo test --release --lib frame_fingerprints -- --ignored --nocapture` before and after a
    /// change to the frame code proves the pictures stayed the same pixel for pixel (FRAMES.md).
    #[test]
    #[ignore]
    fn frame_fingerprints() {
        let cases = [(GB_7X, 7.0), (GBA_5X, 5.0), (SNES_4X, 4.0), (GBA_4X, 4.0), (SNES_3X, 3.0), (Rect { x: 260, y: 0, w: 720, h: 1080 }, 1080.0 / 384.0)];
        let mut total = 0.0;
        for style in all_styles() {
            let mut line = format!("{style:?}");
            for (game, px) in cases {
                let t0 = std::time::Instant::now();
                let b = generate(style, 1240, 1080, game, px);
                let ms = t0.elapsed().as_secs_f32() * 1000.0;
                total += ms;
                let fp = b.as_ref().map_or(0, |b| {
                    let mut h = 0xcbf2_9ce4_8422_2325u64;
                    let meta = format!("{} {} {:?} {:?} {:?}", b.w, b.h, b.hole, b.led, b.inlay);
                    for byte in b.rgba.iter().chain(meta.as_bytes()) {
                        h = (h ^ *byte as u64).wrapping_mul(0x100_0000_01b3);
                    }
                    h
                });
                line.push_str(&format!(" {fp:016x}/{ms:.1}"));
            }
            println!("{line}");
        }
        println!("total {total:.0} ms");
    }

    #[test]
    fn finishes_parse_their_values_and_default_to_the_first() {
        for f in BrickColour::ALL {
            assert_eq!(BrickColour::parse(f.value()), *f);
        }
        assert_eq!((BrickColour::parse(""), SlimFinish::parse("x"), ClamFinish::parse("flame"), SwanFinish::parse("skeleton"), WatchFinish::parse("silver")), (BrickColour::Grey, SlimFinish::Silver, ClamFinish::FlameRed, SwanFinish::Skeleton, WatchFinish::Silver));
    }

    #[test]
    fn packs_round_trip_and_are_found_by_console() {
        let dir = TestDir::new("frames");
        let b = generate(Style::Handheld, 1240, 1080, GB_7X, 7.0).unwrap();
        write_pack(&dir.join("b-test"), "Test", "gbc", &b).unwrap();
        // An earlier pack (by name) without a gbc frame does not hide it.
        std::fs::create_dir_all(dir.join("a-other")).unwrap();
        std::fs::write(dir.join("a-other/pack.json"), r#"{"name":"Other","frames":[{"systems":["nes"],"image":"nes.png","hole":[1,2,3,4]}]}"#).unwrap();
        let (path, hole) = find_pack_frame(&dir, "gbc").expect("found");
        assert_eq!((path, hole), (dir.join("b-test/gbc.png"), GB_7X));
        assert_eq!(find_pack_frame(&dir, "nes").map(|f| f.1), Some(Rect { x: 1, y: 2, w: 3, h: 4 }));
        assert!(find_pack_frame(&dir, "snes").is_none());
        // Premultiplied → straight → premultiplied comes back within rounding.
        let (w, h, rgba) = load_png_rgba(&dir.join("b-test/gbc.png")).unwrap();
        assert_eq!((w, h), (1240, 1080));
        let max_err = rgba.iter().zip(&b.rgba).map(|(a, b)| (*a as i32 - *b as i32).abs()).max().unwrap();
        assert!(max_err <= 1, "{max_err}");
        // Image names can't leave the pack's folder.
        std::fs::write(dir.join("a-other/pack.json"), r#"{"name":"Bad","frames":[{"systems":["snes"],"image":"../x.png","hole":[0,0,1,1]}]}"#).unwrap();
        assert!(find_pack_frame(&dir, "snes").is_none());
    }
}
