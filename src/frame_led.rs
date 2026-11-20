//! The power light of the handheld frames (Brick, Pocket slim, Clamshell, Wide, Gloss, Swan and the lens of the Game Gear
//! and Lynx), live, as the original's light does it ([`Model`]): the Game Boy's red light dims when the battery runs low,
//! the GBA's and the SP's green one turns red, the PSP's blinks orange, the Game Gear's red one flashes. Low is the level
//! of the "Battery low" warning ([`LOW`], `low_battery::WARN_AT`). A save (a slot, the auto state) blinks the light twice,
//! briefly and bright, as a memory light does ([`save_blink_level`]).
//!
//! Settings → Look → Cozy extras → Power lights ([`live`]): off, the light stays as the frame was drawn, steadily on.
//! Without a frame (or in full screen) there is no light at all.
//!
//! The frame itself is drawn once per size and cached on the GPU (`bezel::Layer`). Only the light's
//! small square (`Led::rect`, about 60 px across) is painted again, from a copy of the frame under it
//! without the light, and uploaded with one `glTexSubImage2D` when what it shows changes: never while
//! it is steady, twice a second while it blinks, five times in a save's blink. The battery
//! is read every 10 s (one sysfs file).

use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::bezel::{noise, smoothstep, Style};
use crate::video::Rect;

/// Where a frame's light sits (output pixels) and how large it is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spot {
    pub x: f32,
    pub y: f32,
    pub r: f32,
}

/// The light of a drawn frame: its spot, the square it may paint (inside the image, never on the
/// picture) and that square of the frame without the light (premultiplied RGBA).
#[derive(Clone, Debug, PartialEq)]
pub struct Led {
    pub spot: Spot,
    pub rect: Rect,
    pub base: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lamp {
    Green,
    Red,
    Orange,
}

/// How a frame's light behaves: as the light of the handheld the frame is shaped after.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Model {
    /// Game Boy, Pocket and Color (Brick, Pocket slim): a red light that dims when the batteries run low.
    RedDims,
    /// Game Boy Advance and SP (Wide, Clamshell, and the Swan): green, red when low.
    #[default]
    GreenToRed,
    /// PSP (Gloss): green, blinking orange when low.
    GreenBlinksOrange,
    /// Game Gear, Lynx and the other handhelds of the lens frame (Handheld): red, flashing when low.
    RedFlashes,
}

impl Model {
    /// The light of a frame `style`.
    pub fn of(style: Style) -> Model {
        match style {
            Style::Brick(_) | Style::PocketSlim(_) => Model::RedDims,
            Style::Gloss => Model::GreenBlinksOrange,
            Style::Handheld => Model::RedFlashes,
            _ => Model::GreenToRed,
        }
    }

    /// The light of the handheld of `system`'s picture (`insert_scene::scene_system`'s id: `gb`, `gba-sp`, …): the same
    /// lights as the frames', for the handheld standing in the Cabinet.
    pub fn of_system(system: &str) -> Model {
        match system {
            "gb" | "gbh" | "gbc" | "gbch" | "pokemini" => Model::RedDims,
            "psp" | "psvita" => Model::GreenBlinksOrange,
            "gamegear" | "atarilynx" | "lynx" | "ngp" | "ngpc" => Model::RedFlashes,
            _ => Model::GreenToRed,
        }
    }

    /// Its light with plenty of battery (or none to read).
    pub fn steady(self) -> Look {
        let lamp = match self {
            Model::RedDims | Model::RedFlashes => Lamp::Red,
            Model::GreenToRed | Model::GreenBlinksOrange => Lamp::Green,
        };
        Look { lamp, lit: true, dim: false, flash: 0 }
    }
}

/// What the light shows. `flash` is a save's blink, 0 to 3 ([`save_blink_level`] in steps), 0 otherwise.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Look {
    pub lamp: Lamp,
    pub lit: bool,
    /// Lit, but faintly: a Game Boy's light on weak batteries.
    pub dim: bool,
    pub flash: u8,
}

impl Default for Look {
    /// Steady green: plenty of battery, or no battery to read (the desktop, the virtual device).
    fn default() -> Self {
        Model::GreenToRed.steady()
    }
}

/// Low at or below this many percent: the "Battery low" warning's level (`low_battery::WARN_AT`, held equal by a test).
pub const LOW: u8 = 5;
/// One blink while low: lit for the first half.
pub const BLINK_MS: u64 = 1000;
/// A save's blink: twice, short and bright, then out (`save_blink_level`).
pub const SAVE_BLINK: Duration = Duration::from_millis(360);
/// A save's blink in [`Look::flash`]'s steps, for the frame's light.
pub const FLASH_MS: u64 = 360;
/// How often the battery is read.
pub const READ_EVERY: Duration = Duration::from_secs(10);

/// The environment pocket-ui sets for pocket-core when Power lights is off (`cozy_extras::Extra::PowerLights`): `0`.
pub const ENV: &str = "POCKET_POWER_LIGHTS";

/// Whether the lights follow the battery and blink on a save (Power lights on): pocket-ui and Android's player by the
/// switch itself (`cozy_extras`), pocket-core by [`ENV`], read once.
pub fn live() -> bool {
    static ENV_ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENV_ON.get_or_init(|| std::env::var_os(ENV).is_none_or(|v| v != "0")) && crate::cozy_extras::on(crate::cozy_extras::Extra::PowerLights)
}

/// How bright a save's memory light is `since` the save (0 to 1): on, a short break, on again, then out; 0 after
/// [`SAVE_BLINK`]. The frames' light shows it.
pub fn save_blink_level(since: Duration) -> f32 {
    let ms = since.as_millis() as u64;
    match ms {
        0..90 => 1.0,
        90..150 => 0.12,
        150..270 => 1.0,
        270..360 => 1.0 - (ms - 270) as f32 / 90.0,
        _ => 0.0,
    }
}

/// What the light of `model` shows at `capacity` percent (None: no battery), `ms` after the clock started,
/// `since_flash` ms after the last save (None: none yet).
pub fn look_at(model: Model, capacity: Option<u8>, ms: u64, since_flash: Option<u64>) -> Look {
    let low = capacity.is_some_and(|c| c <= LOW);
    let blink = ms % BLINK_MS < BLINK_MS / 2;
    let mut look = model.steady();
    if low {
        match model {
            Model::RedDims => look.dim = true,
            Model::GreenToRed => look.lamp = Lamp::Red,
            Model::GreenBlinksOrange => (look.lamp, look.lit) = (Lamp::Orange, blink),
            Model::RedFlashes => look.lit = blink,
        }
    }
    look.flash = match since_flash {
        Some(t) if t < FLASH_MS => (save_blink_level(Duration::from_millis(t)) * 3.0).round() as u8,
        _ => 0,
    };
    look
}

/// The light's state over time: reads the battery now and then and remembers the last save.
pub struct Clock {
    dir: PathBuf,
    model: Model,
    start: Instant,
    capacity: Option<u8>,
    read_at: Option<Instant>,
    flash_at: Option<Instant>,
    /// A save asked for a flash that hasn't been shown yet.
    flash_pending: bool,
}

impl Clock {
    /// Reads the battery in `dir` (`battery::dir()` on the device), for a light of `model`.
    pub fn new(dir: PathBuf, model: Model) -> Self {
        Clock {
            dir,
            model,
            start: Instant::now(),
            capacity: None,
            read_at: None,
            flash_at: None,
            flash_pending: false,
        }
    }

    /// The frame's light is now one of `model`.
    pub fn set_model(&mut self, model: Model) {
        self.model = model;
    }

    /// A state was saved: flash, from the next time the light is drawn. A save from the pause menu
    /// happens while the game (and its frame) isn't drawn, so the flash shows as the game comes back.
    pub fn flash(&mut self) {
        self.flash_pending = true;
    }

    /// What the light shows at `now` on a picture that is painted only when something changes (the Cabinet): no blink,
    /// so a low light that blinks on the frame shows as low steadily (the PSP's orange, the Game Gear's dimmed red).
    pub fn still(&mut self, now: Instant) -> Look {
        let mut look = Look { flash: 0, ..self.look(now) };
        let low = self.capacity.is_some_and(|c| c <= LOW);
        look.lit = true;
        look.dim |= low && self.model == Model::RedFlashes;
        look
    }

    /// What the light shows at `now`; reads the battery when the last reading is 10 s old.
    pub fn look(&mut self, now: Instant) -> Look {
        if self.read_at.is_none_or(|t| now.duration_since(t) >= READ_EVERY) {
            self.capacity = read_capacity(&self.dir);
            self.read_at = Some(now);
        }
        if std::mem::take(&mut self.flash_pending) {
            self.flash_at = Some(now);
        }
        let ms = now.duration_since(self.start).as_millis() as u64;
        look_at(self.model, self.capacity, ms, self.flash_at.map(|t| now.duration_since(t).as_millis() as u64))
    }
}

/// The battery's level in `dir` (its `capacity` file), None without one. A battery on the charger reads as it is: the
/// originals' lights showed the batteries, not the plug.
pub fn read_capacity(dir: &std::path::Path) -> Option<u8> {
    std::fs::read_to_string(dir.join("capacity")).ok().and_then(|s| s.trim().parse::<i64>().ok()).map(|c| c.clamp(0, 100) as u8)
}

/// How far the halo of a flash reaches, in light radii: the square the light may paint.
const REACH: f32 = 6.0;

/// The square around `spot` the light may paint, inside a `w`x`h` image.
pub fn rect_for(spot: Spot, w: u32, h: u32) -> Rect {
    let half = (spot.r * REACH).ceil();
    let x0 = (spot.x - half).floor().clamp(0.0, w as f32) as u32;
    let y0 = (spot.y - half).floor().clamp(0.0, h as f32) as u32;
    let x1 = (spot.x + half).ceil().clamp(0.0, w as f32) as u32;
    let y1 = (spot.y + half).ceil().clamp(0.0, h as f32) as u32;
    Rect {
        x: x0,
        y: y0,
        w: x1 - x0,
        h: y1 - y0,
    }
}

/// Copies the light's square out of a frame image (before the light is painted).
pub fn capture(spot: Spot, rgba: &[u8], w: u32, h: u32) -> Led {
    let rect = rect_for(spot, w, h);
    let mut base = Vec::with_capacity((rect.w * rect.h * 4) as usize);
    for y in rect.y..rect.y + rect.h {
        let i = ((y * w + rect.x) * 4) as usize;
        base.extend_from_slice(&rgba[i..i + (rect.w * 4) as usize]);
    }
    Led { spot, rect, base }
}

/// The light's square showing `look`: premultiplied RGBA, `led.rect` large. Pixels inside `hole` (the
/// picture) and pixels the frame doesn't cover stay as they are.
pub fn paint(led: &Led, look: Look, hole: Rect) -> Vec<u8> {
    let mut out = led.base.clone();
    let Spot { x: lx, y: ly, r } = led.spot;
    let full = match look.lamp {
        Lamp::Green => [0.32, 0.93, 0.42],
        Lamp::Red => [0.97, 0.22, 0.16],
        Lamp::Orange => [1.0, 0.58, 0.14],
    };
    // Weak batteries: the light glows faintly, its halo nearly gone (a flash still shows bright).
    let colour = if look.dim && look.flash == 0 { scale(full, 0.32) } else { full };
    let f = look.flash as f32 / 3.0;
    // Lit: the dome in its colour with a hot centre and a soft halo in the plastic around it. A
    // flash brightens it towards white and widens the halo. Unlit: a dark dome.
    let core = mix(colour, [1.0; 3], 0.18 + 0.55 * f);
    let (halo_k, halo_r) = (if look.dim && look.flash == 0 { 0.05 } else { 0.20 + 0.45 * f }, r * (2.1 + 1.6 * f));
    let on = look.lit || look.flash > 0;
    for py in 0..led.rect.h {
        for px in 0..led.rect.w {
            let (x, y) = (led.rect.x + px, led.rect.y + py);
            if x >= hole.x && x < hole.x + hole.w && y >= hole.y && y < hole.y + hole.h {
                continue;
            }
            let i = ((py * led.rect.w + px) * 4) as usize;
            let a = led.base[i + 3] as f32 / 255.0;
            if a <= 0.0 {
                continue;
            }
            let mut c: [f32; 3] = std::array::from_fn(|k| led.base[i + k] as f32 / 255.0 / a);
            let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
            let d = ((fx - lx).powi(2) + (fy - ly).powi(2)).sqrt();
            // The hole in the plastic the light sits in: a fine dark ring.
            let ring = (-((d - r - 0.7) / 0.9).powi(2)).exp();
            c = scale(c, 1.0 - 0.45 * ring);
            let dome = 1.0 - smoothstep(r - 0.8, r + 0.6, d);
            let lens = if on {
                let hot = 1.0 - smoothstep(0.0, r * 0.9, d);
                mix(scale(colour, 0.78), core, hot)
            } else {
                // Off: the lens is dark, a hint of its colour, lit from above.
                add(scale(colour, 0.07), [0.035; 3])
            };
            c = mix(c, lens, dome);
            if on {
                // Faded out before the square's edge, so the square never shows.
                let halo = halo_k * (-(d / halo_r).powi(2)).exp() * (1.0 - smoothstep(r * 3.8, r * (REACH - 0.6), d));
                c = add(c, scale(colour, halo * (1.0 - dome)));
            }
            // The dome's own highlight, up and to the left.
            let hl = 1.0 - smoothstep(0.0, r * 0.5, ((fx - lx + r * 0.32).powi(2) + (fy - ly + r * 0.32).powi(2)).sqrt());
            c = add(c, [(if on { 0.22 } else { 0.10 }) * hl * dome; 3]);
            let dither = noise(x, y) / 255.0;
            for k in 0..3 {
                out[i + k] = (((c[k].clamp(0.0, 1.0) + dither).clamp(0.0, 1.0)) * a * 255.0 + 0.5) as u8;
            }
        }
    }
    out
}

/// A small light at (`cx`, `cy`), `r` pixels across, on a picture already painted (the handheld standing in the Cabinet):
/// a dome in its colour with a hot centre and a soft glow around it; unlit, nothing (the picture's own dark light shows).
pub fn paint_dot(c: &mut crate::gfx::Canvas, cx: f32, cy: f32, r: f32, look: Look) {
    let on = look.lit || look.flash > 0;
    if !on || r <= 0.0 {
        return;
    }
    let full = match look.lamp {
        Lamp::Green => [0.32, 0.93, 0.42],
        Lamp::Red => [0.97, 0.22, 0.16],
        Lamp::Orange => [1.0, 0.58, 0.14],
    };
    let f = look.flash as f32 / 3.0;
    let (gain, reach) = if look.dim && look.flash == 0 { (0.35, 1.6) } else { (1.0, 2.6 + 1.4 * f) };
    let core = mix(full, [1.0; 3], 0.25 + 0.5 * f);
    let rgb = |c: [f32; 3]| c.iter().fold(0u32, |v, &k| v << 8 | (k.clamp(0.0, 1.0) * 255.0 + 0.5) as u32);
    let half = (r * reach).ceil() as i32;
    let (x0, y0) = (cx.floor() as i32, cy.floor() as i32);
    for y in y0 - half..=y0 + half {
        for x in x0 - half..=x0 + half {
            let d = ((x as f32 + 0.5 - cx).powi(2) + (y as f32 + 0.5 - cy).powi(2)).sqrt();
            let dome = 1.0 - smoothstep(r * 0.55, r * 0.55 + 0.9, d);
            let glow = 0.45 * (-(d / (r * reach * 0.5)).powi(2)).exp() * (1.0 - dome);
            if dome > 0.01 {
                let hot = 1.0 - smoothstep(0.0, r * 0.5, d);
                c.blend(x, y, rgb(scale(mix(scale(full, 0.8), core, hot), gain)), (dome * 255.0) as u8);
            } else if glow * gain > 0.01 {
                c.blend(x, y, rgb(full), (glow * gain * 255.0) as u8);
            }
        }
    }
}

/// Puts a painted square back into a whole frame image.
pub fn blit(rgba: &mut [u8], w: u32, rect: Rect, patch: &[u8]) {
    for row in 0..rect.h {
        let o = (((rect.y + row) * w + rect.x) * 4) as usize;
        let s = (row * rect.w * 4) as usize;
        rgba[o..o + (rect.w * 4) as usize].copy_from_slice(&patch[s..s + (rect.w * 4) as usize]);
    }
}

fn mix(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

fn scale(a: [f32; 3], s: f32) -> [f32; 3] {
    [a[0] * s, a[1] * s, a[2] * s]
}

fn add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;

    const ALL: [Model; 4] = [Model::RedDims, Model::GreenToRed, Model::GreenBlinksOrange, Model::RedFlashes];

    fn look(lamp: Lamp, lit: bool, dim: bool) -> Look {
        Look { lamp, lit, dim, flash: 0 }
    }

    /// Low is the "Battery low" warning's level, the same number everywhere.
    #[cfg(not(target_os = "android"))]
    #[test]
    fn low_is_the_battery_low_warnings_level() {
        assert_eq!(LOW, crate::low_battery::WARN_AT);
    }

    /// Each light as the original's: above the warning level steady in its colour; at it the Game Boy's dims, the GBA's
    /// and the SP's turns red, the PSP's blinks orange, the Game Gear's flashes red. No battery: steady.
    #[test]
    fn each_light_goes_low_as_the_originals_did_at_the_warning_level() {
        for model in ALL {
            assert_eq!(look_at(model, None, 1500, None), model.steady(), "{model:?}: no battery, steady");
            assert_eq!(look_at(model, Some(100), 1500, None), model.steady(), "{model:?}: full");
            assert_eq!(look_at(model, Some(LOW + 1), 1700, None), model.steady(), "{model:?}: just above the warning");
        }
        assert_eq!(Model::RedDims.steady(), look(Lamp::Red, true, false));
        assert_eq!(Model::RedFlashes.steady(), look(Lamp::Red, true, false));
        assert_eq!(Model::GreenToRed.steady(), look(Lamp::Green, true, false));
        assert_eq!(Model::GreenBlinksOrange.steady(), look(Lamp::Green, true, false));
        for ms in [0, 400, 600, 1700] {
            assert_eq!(look_at(Model::RedDims, Some(LOW), ms, None), look(Lamp::Red, true, true), "Game Boy: dim, never off");
            assert_eq!(look_at(Model::GreenToRed, Some(LOW), ms, None), look(Lamp::Red, true, false), "GBA, SP: red, steady");
        }
        // Blinking: lit for the first half of every second.
        for (ms, lit) in [(0, true), (499, true), (500, false), (999, false), (1000, true), (7600, false)] {
            assert_eq!(look_at(Model::GreenBlinksOrange, Some(LOW), ms, None), look(Lamp::Orange, lit, false), "PSP {ms} ms");
            assert_eq!(look_at(Model::RedFlashes, Some(2), ms, None), look(Lamp::Red, lit, false), "Game Gear {ms} ms");
        }
    }

    /// The battery's `capacity` file missing, empty, cut short, flipped bits or not UTF-8: no battery (a steady light), never
    /// a panic; out-of-range numbers are held to 0-100.
    #[test]
    fn a_broken_capacity_file_reads_as_no_battery() {
        let d = TestDir::new("led-faults");
        assert_eq!(read_capacity(&d.join("absent")), None, "missing");
        for (bytes, want) in [(&b""[..], None), (b"4", Some(4)), (b"4\n\n", Some(4)), (b"\xff\xfe5", None), (b"5\x00", None), (b"-7", Some(0)), (b"250", Some(100)), (b"abc", None)] {
            std::fs::write(d.join("capacity"), bytes).unwrap();
            assert_eq!(read_capacity(&d), want, "{bytes:?}");
        }
        std::fs::write(d.join("capacity"), b"\xff").unwrap();
        let mut clock = Clock::new(d.to_path_buf(), Model::RedDims);
        assert_eq!(clock.look(Instant::now()), Model::RedDims.steady(), "garbage: as no battery");
    }

    /// The Cabinet's handhelds have the same lights as their frames; a dot is painted only where it is lit.
    #[test]
    fn the_cabinets_handhelds_have_their_frames_lights_and_paint_a_small_lit_dot() {
        assert_eq!(Model::of_system("gb"), Model::RedDims);
        assert_eq!(Model::of_system("gba-sp"), Model::GreenToRed);
        assert_eq!(Model::of_system("psp"), Model::GreenBlinksOrange);
        assert_eq!(Model::of_system("gamegear"), Model::RedFlashes);
        let (w, h) = (40u32, 30u32);
        let mut buf = vec![0xff40_4040u32; (w * h) as usize];
        paint_dot(&mut crate::gfx::Canvas::new(&mut buf, w, h), 20.0, 15.0, 3.0, Model::GreenToRed.steady());
        let at = |buf: &[u32], x: u32, y: u32| buf[(y * w + x) as usize];
        let c = at(&buf, 20, 15);
        assert!((c >> 8 & 255) > 200 && (c >> 8 & 255) > (c >> 16 & 255) + 40, "green and lit: {c:08x}");
        assert_eq!(at(&buf, 0, 0), 0xff40_4040, "far from it nothing changes");
        let mut dark = vec![0xff40_4040u32; (w * h) as usize];
        paint_dot(&mut crate::gfx::Canvas::new(&mut dark, w, h), 20.0, 15.0, 3.0, Look { lit: false, ..Model::RedFlashes.steady() });
        assert!(dark.iter().all(|&p| p == 0xff40_4040), "an unlit light leaves the picture's own");
        let mut dim = vec![0xff40_4040u32; (w * h) as usize];
        paint_dot(&mut crate::gfx::Canvas::new(&mut dim, w, h), 20.0, 15.0, 3.0, Look { dim: true, ..Model::RedDims.steady() });
        let mut full = vec![0xff40_4040u32; (w * h) as usize];
        paint_dot(&mut crate::gfx::Canvas::new(&mut full, w, h), 20.0, 15.0, 3.0, Model::RedDims.steady());
        assert!((at(&dim, 20, 15) >> 16 & 255) < (at(&full, 20, 15) >> 16 & 255), "a dim light is darker");
    }

    /// The frames' shapes take the light of the handheld they're after.
    #[test]
    fn each_frame_has_the_light_of_its_handheld() {
        use crate::bezel::{BrickColour, ClamFinish, SlimFinish, SwanFinish};
        assert_eq!(Model::of(Style::Brick(BrickColour::Grey)), Model::RedDims);
        assert_eq!(Model::of(Style::PocketSlim(SlimFinish::Clear)), Model::RedDims);
        assert_eq!(Model::of(Style::Wide), Model::GreenToRed);
        assert_eq!(Model::of(Style::Clamshell(ClamFinish::ALL[0])), Model::GreenToRed);
        assert_eq!(Model::of(Style::Gloss), Model::GreenBlinksOrange);
        assert_eq!(Model::of(Style::Handheld), Model::RedFlashes);
        assert_eq!(Model::of(Style::Swan(SwanFinish::Pastel)), Model::GreenToRed);
    }

    /// A save blinks the light twice, short and bright, and is over in `SAVE_BLINK`, also over a low light's dark half.
    #[test]
    fn a_save_blinks_twice_and_is_over_quickly() {
        let steps: Vec<u8> = [0, 89, 90, 149, 150, 269, 300, 359, 360, 5000].iter().map(|&t| look_at(Model::GreenToRed, Some(50), 0, Some(t)).flash).collect();
        assert_eq!(steps, [3, 3, 0, 0, 3, 3, 2, 0, 0, 0]);
        let off = look_at(Model::RedFlashes, Some(3), 700, Some(10));
        assert!(!off.lit && off.flash == 3, "the blink shows over the low light's dark half");
        assert_eq!(save_blink_level(Duration::ZERO), 1.0);
        assert!(save_blink_level(Duration::from_millis(120)) < 0.2, "a break between the two");
        assert_eq!(save_blink_level(SAVE_BLINK), 0.0);
        // Smooth at its end: no step of more than a third between two 60 Hz frames once it fades.
        let fade: Vec<f32> = (16..=22).map(|k| save_blink_level(Duration::from_millis(k * 1000 / 60))).collect();
        assert!(fade.windows(2).all(|w| (w[0] - w[1]).abs() <= 0.34), "{fade:?}");
    }

    #[test]
    fn the_clock_reads_the_battery_every_ten_seconds_and_flashes_on_save() {
        let dir = TestDir::new("led");
        std::fs::write(dir.join("capacity"), "64\n").unwrap();
        let mut clock = Clock::new(dir.to_path_buf(), Model::GreenToRed);
        let t0 = clock.start;
        assert_eq!(clock.look(t0).lamp, Lamp::Green);
        std::fs::write(dir.join("capacity"), "4\n").unwrap();
        assert_eq!(clock.look(t0 + Duration::from_secs(5)).lamp, Lamp::Green, "not read again yet");
        assert_eq!(clock.look(t0 + Duration::from_secs(11)).lamp, Lamp::Red);
        clock.set_model(Model::RedDims);
        assert!(clock.look(t0 + Duration::from_secs(12)).dim, "the same battery on a Game Boy's light");
        clock.flash();
        // The flash starts when the light is next drawn (here 3 s after the save was asked for).
        assert_eq!(clock.look(t0 + Duration::from_millis(15_000)).flash, 3);
        assert_eq!(clock.look(t0 + Duration::from_millis(15_100)).flash, 0);
        assert_eq!(clock.look(t0 + Duration::from_millis(15_200)).flash, 3);
        assert_eq!(clock.look(t0 + Duration::from_millis(15_500)).flash, 0);
        // A fake battery right at the "Battery low" level: 6 % is still steady green, 5 % is red.
        let fake = TestDir::new("led-threshold");
        let mut gba = Clock::new(fake.to_path_buf(), Model::GreenToRed);
        let t1 = gba.start;
        std::fs::write(fake.join("capacity"), format!("{}\n", LOW + 1)).unwrap();
        assert_eq!(gba.look(t1).lamp, Lamp::Green);
        std::fs::write(fake.join("capacity"), format!("{LOW}\n")).unwrap();
        assert_eq!(gba.look(t1 + READ_EVERY).lamp, Lamp::Red, "at the warning's level");
        // The Cabinet's still light: low shows without a blink.
        let mut gg = Clock::new(fake.to_path_buf(), Model::RedFlashes);
        let still = gg.still(gg.start + Duration::from_millis(700));
        assert!(still.lit && still.dim && still.lamp == Lamp::Red, "a low Game Gear glows dim red: {still:?}");
        let mut psp = Clock::new(fake.to_path_buf(), Model::GreenBlinksOrange);
        assert_eq!(psp.still(psp.start + Duration::from_millis(700)), Look { lamp: Lamp::Orange, lit: true, dim: false, flash: 0 });
        // No battery at all: steady.
        let mut none = Clock::new(PathBuf::from("/nonexistent/battery"), Model::RedDims);
        assert_eq!(none.look(Instant::now()), Model::RedDims.steady());
    }

    #[test]
    fn the_light_paints_only_its_square_and_never_the_picture() {
        let (w, h) = (200u32, 120u32);
        let hole = Rect { x: 60, y: 10, w: 120, h: 100 };
        // An opaque grey frame everywhere but the picture.
        let mut rgba = vec![0u8; (w * h * 4) as usize];
        for y in 0..h {
            for x in 0..w {
                if !(x >= hole.x && x < hole.x + hole.w && y >= hole.y && y < hole.y + hole.h) {
                    let i = ((y * w + x) * 4) as usize;
                    rgba[i..i + 4].copy_from_slice(&[60, 60, 60, 255]);
                }
            }
        }
        // A light close to the picture: its square reaches into it.
        let led = capture(Spot { x: 50.0, y: 40.0, r: 4.0 }, &rgba, w, h);
        assert!(led.rect.x + led.rect.w > hole.x, "the square overlaps the picture");
        for look in [
            Look::default(),
            Look { lamp: Lamp::Red, lit: false, dim: false, flash: 0 },
            Look { lamp: Lamp::Red, lit: true, dim: false, flash: 3 },
            Look { lamp: Lamp::Red, lit: true, dim: true, flash: 0 },
            Look { lamp: Lamp::Orange, lit: true, dim: false, flash: 0 },
        ] {
            let patch = paint(&led, look, hole);
            let mut out = rgba.clone();
            blit(&mut out, w, led.rect, &patch);
            for y in 0..h {
                for x in 0..w {
                    let i = ((y * w + x) * 4) as usize;
                    let inside = x >= led.rect.x && x < led.rect.x + led.rect.w && y >= led.rect.y && y < led.rect.y + led.rect.h;
                    let in_hole = x >= hole.x && x < hole.x + hole.w && y >= hole.y && y < hole.y + hole.h;
                    if !inside || in_hole {
                        assert_eq!(&out[i..i + 4], &rgba[i..i + 4], "{look:?} touched {x},{y}");
                    }
                }
            }
            let centre = (((40 * w) + 50) * 4) as usize;
            let px = &out[centre..centre + 3];
            match (look.lamp, look.lit, look.dim) {
                (Lamp::Green, true, _) => assert!(px[1] > 180 && px[1] > px[0] + 40, "{px:?}"),
                (Lamp::Red, false, _) => assert!(px.iter().all(|&v| v < 60), "off is dark: {px:?}"),
                (Lamp::Red, true, false) => assert!(px[0] > 230, "{px:?}"),
                (Lamp::Red, true, true) => assert!(px[0] > 70 && px[0] < 200 && px[0] > px[1] + 30, "dim, still red: {px:?}"),
                (Lamp::Orange, true, _) => assert!(px[0] > 230 && px[1] > 120 && px[1] < px[0] && px[2] < px[1], "orange: {px:?}"),
                _ => {}
            }
        }
        // A flash lights up more of the plastic around the light than the steady light.
        let lit_sum = |look| {
            paint(&led, look, hole)
                .chunks_exact(4)
                .map(|p| p[0] as u32 + p[1] as u32 + p[2] as u32)
                .sum::<u32>()
        };
        assert!(lit_sum(Look { flash: 3, ..Look::default() }) > lit_sum(Look::default()));
        let red = Model::RedDims.steady();
        assert!(lit_sum(Look { dim: true, ..red }) < lit_sum(red), "a dim light lights less of the plastic");
    }
}
