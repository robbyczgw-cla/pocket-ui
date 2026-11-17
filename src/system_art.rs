//! Pixel-art pictures of the consoles for the system cards on the shelf (assets/systems, sources and
//! licences in assets/systems/SOURCES.md). They are compiled into the binary, so every install has them.

/// A picture with binary transparency: 0xAARRGGBB, alpha 0 or 255.
pub struct Sprite {
    pub w: u32,
    pub h: u32,
    pub px: Vec<u32>,
}

/// The player's GBA hardware picture, kept independently of the ROM system and emulator.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GbaModel {
    #[default]
    Advance,
    Sp,
}

impl GbaModel {
    pub fn label(self) -> &'static str {
        match self { Self::Advance => "Game Boy Advance", Self::Sp => "Game Boy Advance SP" }
    }
    pub fn next(self) -> Self {
        match self { Self::Advance => Self::Sp, Self::Sp => Self::Advance }
    }
}

static GBA_MODEL: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn gba_model() -> GbaModel {
    if GBA_MODEL.load(std::sync::atomic::Ordering::Relaxed) { GbaModel::Sp } else { GbaModel::Advance }
}

pub fn set_gba_model(model: GbaModel) {
    GBA_MODEL.store(model == GbaModel::Sp, std::sync::atomic::Ordering::Relaxed);
}

/// One other colour of a console (assets/systems/colours): its saved id, its name as the console page shows it, the
/// shelf and console-page picture, and the insert scene's pictures (the rig is the original's: same pose, same slot).
pub struct Colour {
    pub id: &'static str,
    pub name: &'static str,
    art: &'static [u8],
    rig_base: &'static [u8],
    /// The layers of the console's own plastic besides the base (the N64's front, the GameCube's lid and its underside); a
    /// layer the colour does not list stays the original's.
    rig_over: Option<&'static [u8]>,
    rig_top: Option<&'static [u8]>,
    rig_under: Option<&'static [u8]>,
    decoded: std::sync::OnceLock<Option<Sprite>>,
}

/// The insert scene's pictures of a colour besides the base; `None` is the original's own.
#[derive(Clone, Copy, Default)]
pub struct RigLayers {
    pub over: Option<&'static [u8]>,
    pub top: Option<&'static [u8]>,
    pub under: Option<&'static [u8]>,
}

macro_rules! colour {
    ($id:literal, $name:literal, $system:literal $(, $layer:ident)*) => {
        Colour {
            id: $id,
            name: $name,
            art: include_bytes!(concat!("../assets/systems/colours/", $system, "-", $id, ".png")),
            rig_base: include_bytes!(concat!("../assets/media/rig/", $system, "-", $id, "/base.png")),
            rig_over: colour!(@layer over, $system, $id $(, $layer)*),
            rig_top: colour!(@layer top, $system, $id $(, $layer)*),
            rig_under: colour!(@layer under, $system, $id $(, $layer)*),
            decoded: std::sync::OnceLock::new(),
        }
    };
    (@layer over, $system:literal, $id:literal, over $(, $rest:ident)*) => { Some(include_bytes!(concat!("../assets/media/rig/", $system, "-", $id, "/over.png")).as_slice()) };
    (@layer top, $system:literal, $id:literal, top $(, $rest:ident)*) => { Some(include_bytes!(concat!("../assets/media/rig/", $system, "-", $id, "/top.png")).as_slice()) };
    (@layer under, $system:literal, $id:literal, under $(, $rest:ident)*) => { Some(include_bytes!(concat!("../assets/media/rig/", $system, "-", $id, "/under.png")).as_slice()) };
    (@layer $which:ident, $system:literal, $id:literal, $skip:ident $(, $rest:ident)*) => { colour!(@layer $which, $system, $id $(, $rest)*) };
    (@layer $which:ident, $system:literal, $id:literal) => { None };
}

/// The consoles that come in more than one colour, by picture id (the original is always the first stop and has no entry).
/// All from photos of the real consoles; the Advance only (the SP picture has its own, `gba-sp`).
static GBA_COLOURS: [Colour; 2] = [colour!("milky", "Milky Blue", "gba"), colour!("pink", "Pink", "gba")];
static GBC_COLOURS: [Colour; 1] = [colour!("dandelion", "Dandelion", "gbc")];
static N64_COLOURS: [Colour; 2] = [colour!("iceblue", "Ice Blue", "n64", over), colour!("jungle", "Jungle Green", "n64", over)];
static GAMECUBE_COLOURS: [Colour; 1] = [colour!("jetblack", "Jet Black", "gamecube", top, under)];
static COLOURS: &[(&str, &[Colour])] = &[("gba", &GBA_COLOURS), ("gbc", &GBC_COLOURS), ("n64", &N64_COLOURS), ("gamecube", &GAMECUBE_COLOURS)];

/// The player's colour per console (picture id → colour id), process-wide like the GBA model; absent means the original.
static CHOSEN: std::sync::Mutex<Vec<(String, String)>> = std::sync::Mutex::new(Vec::new());

/// The other colours a system's picture comes in (empty: only the original).
pub fn colours_of(system: &str) -> &'static [Colour] {
    let id = picture_id(system);
    COLOURS.iter().find(|(s, _)| *s == id).map_or(&[], |(_, c)| c)
}

/// The colour chosen for a system; None is the original (also when the saved one no longer exists).
pub fn colour(system: &str) -> Option<&'static Colour> {
    let id = picture_id(system);
    let chosen = CHOSEN.lock().ok()?.iter().find(|(s, _)| s == id)?.1.clone();
    colours_of(system).iter().find(|c| c.id == chosen)
}

/// The chosen colour's name; "Original" for the console as it was first.
pub fn colour_name(system: &str) -> &'static str {
    colour(system).map_or("Original", |c| c.name)
}

/// One stop along Original, then each colour, round and round (`step` is -1 or 1). False for a system with one colour only.
pub fn step_colour(system: &str, step: i32) -> bool {
    let list = colours_of(system);
    if list.is_empty() {
        return false;
    }
    let now = colour(system).and_then(|c| list.iter().position(|x| x.id == c.id)).map_or(0, |i| i + 1);
    let next = stop_after(list.len(), now, step);
    set_colour(system, if next == 0 { None } else { Some(list[next - 1].id) });
    true
}

/// The stop after `now` (0 is the original, 1.. the colours) among `colours` colours, `step` of -1 or 1, round and round.
fn stop_after(colours: usize, now: usize, step: i32) -> usize {
    (now as i32 + step).rem_euclid(colours as i32 + 1) as usize
}

fn set_colour(system: &str, id: Option<&str>) {
    let key = picture_id(system).to_string();
    let Ok(mut chosen) = CHOSEN.lock() else { return };
    chosen.retain(|(s, _)| *s != key);
    if let Some(id) = id {
        chosen.push((key, id.to_string()));
    }
}

/// The saved choices (picture id → colour id), only the ones that name a colour that exists, in a fixed order.
pub fn colours_chosen() -> std::collections::BTreeMap<String, String> {
    let chosen = CHOSEN.lock().map(|c| c.clone()).unwrap_or_default();
    chosen.into_iter().filter(|(s, id)| COLOURS.iter().any(|(sys, list)| sys == s && list.iter().any(|c| c.id == *id))).collect()
}

/// Takes the saved choices at the start (an unknown system or colour is dropped, the console stays as it was).
pub fn set_colours_chosen(saved: &std::collections::BTreeMap<String, String>) {
    if let Ok(mut chosen) = CHOSEN.lock() {
        *chosen = saved.iter().map(|(s, id)| (s.clone(), id.clone())).collect();
    }
}

/// Shared by every console picture and the launch rig; `canonical` keeps the actual ROM system.
pub fn picture_id(system: &str) -> &str {
    if canonical(system) == "gba" && gba_model() == GbaModel::Sp { "gba-sp" } else { canonical(system) }
}

/// The systems with a picture of their own (assets/systems/<id>.png), by ROCKNIX system name.
const ART: &[(&str, &[u8])] = &[
    ("3do", include_bytes!("../assets/systems/3do.png")),
    // Arcade sets (MAME, FinalBurn Neo, CPS, NAOMI…): one upright cabinet with its marquee and control panel blank.
    ("arcade", include_bytes!("../assets/systems/arcade.png")),
    ("3ds", include_bytes!("../assets/systems/3ds.png")),
    ("amiga", include_bytes!("../assets/systems/amiga.png")),
    ("atari2600", include_bytes!("../assets/systems/atari2600.png")),
    ("atari5200", include_bytes!("../assets/systems/atari5200.png")),
    ("atari7800", include_bytes!("../assets/systems/atari7800.png")),
    ("atarijaguar", include_bytes!("../assets/systems/atarijaguar.png")),
    ("atarilynx", include_bytes!("../assets/systems/atarilynx.png")),
    ("atari800", include_bytes!("../assets/systems/atari800.png")),
    ("c64", include_bytes!("../assets/systems/c64.png")),
    ("colecovision", include_bytes!("../assets/systems/colecovision.png")),
    ("dreamcast", include_bytes!("../assets/systems/dreamcast.png")),
    ("fds", include_bytes!("../assets/systems/fds.png")),
    ("gamecube", include_bytes!("../assets/systems/gamecube.png")),
    ("gamegear", include_bytes!("../assets/systems/gamegear.png")),
    ("gb", include_bytes!("../assets/systems/gb.png")),
    ("gba", include_bytes!("../assets/systems/gba.png")),
    ("gba-sp", include_bytes!("../assets/systems/gba-sp.png")),
    ("gbc", include_bytes!("../assets/systems/gbc.png")),
    ("intellivision", include_bytes!("../assets/systems/intellivision.png")),
    ("mastersystem", include_bytes!("../assets/systems/mastersystem.png")),
    ("megadrive", include_bytes!("../assets/systems/megadrive.png")),
    ("n64", include_bytes!("../assets/systems/n64.png")),
    ("nds", include_bytes!("../assets/systems/nds.png")),
    ("neocd", include_bytes!("../assets/systems/neocd.png")),
    ("neogeo", include_bytes!("../assets/systems/neogeo.png")),
    ("nes", include_bytes!("../assets/systems/nes.png")),
    ("ngpc", include_bytes!("../assets/systems/ngpc.png")),
    ("pcengine", include_bytes!("../assets/systems/pcengine.png")),
    ("pcenginecd", include_bytes!("../assets/systems/pcenginecd.png")),
    ("pokemini", include_bytes!("../assets/systems/pokemini.png")),
    // Phone games (J2ME): a candy-bar phone of the 2000s, no brand on it.
    ("j2me", include_bytes!("../assets/systems/j2me.png")),
    ("pico-8", include_bytes!("../assets/systems/pico-8.png")),
    ("pc", include_bytes!("../assets/systems/pc.png")),
    ("ports", include_bytes!("../assets/systems/ports.png")),
    ("ps2", include_bytes!("../assets/systems/ps2.png")),
    ("ps3", include_bytes!("../assets/systems/ps3.png")),
    ("psp", include_bytes!("../assets/systems/psp.png")),
    ("psvita", include_bytes!("../assets/systems/psvita.png")),
    ("psx", include_bytes!("../assets/systems/psx.png")),
    ("saturn", include_bytes!("../assets/systems/saturn.png")),
    ("scummvm", include_bytes!("../assets/systems/scummvm.png")),
    ("sega32x", include_bytes!("../assets/systems/sega32x.png")),
    ("segacd", include_bytes!("../assets/systems/segacd.png")),
    ("sg-1000", include_bytes!("../assets/systems/sg-1000.png")),
    ("snes", include_bytes!("../assets/systems/snes.png")),
    ("supergrafx", include_bytes!("../assets/systems/supergrafx.png")),
    // Settings > Games > Systems on the shelf: the row of "PC streaming" (an app, not a system with games).
    ("streaming", include_bytes!("../assets/systems/streaming.png")),
    ("tic-80", include_bytes!("../assets/systems/tic-80.png")),
    ("vectrex", include_bytes!("../assets/systems/vectrex.png")),
    ("virtualboy", include_bytes!("../assets/systems/virtualboy.png")),
    ("wii", include_bytes!("../assets/systems/wii.png")),
    ("wiiu", include_bytes!("../assets/systems/wiiu.png")),
    ("wonderswan", include_bytes!("../assets/systems/wonderswan.png")),
    ("xbox", include_bytes!("../assets/systems/xbox.png")),
    ("zxspectrum", include_bytes!("../assets/systems/zxspectrum.png")),
];

/// Other ROCKNIX names for the same hardware.
const ALIASES: &[(&str, &str)] = &[
    ("famicom", "nes"),
    ("nesh", "nes"),
    ("sfc", "snes"),
    ("snesh", "snes"),
    ("gbh", "gb"),
    ("gbch", "gbc"),
    ("gbah", "gba"),
    ("genesis", "megadrive"),
    ("megadrive-japan", "megadrive"),
    ("megadriveh", "megadrive"),
    ("genh", "megadrive"),
    ("ggh", "gamegear"),
    ("megacd", "segacd"),
    ("tg16", "pcengine"),
    ("tg16cd", "pcenginecd"),
    ("ngp", "ngpc"),
    ("wonderswancolor", "wonderswan"),
    ("pico8", "pico-8"),
    ("wiiware", "wii"),
    ("n3ds", "3ds"),
    // Arcade boards and sets share one id and its cabinet.
    ("mame", "arcade"),
    ("fbn", "arcade"),
    ("fbneo", "arcade"),
    ("cps1", "arcade"),
    ("cps2", "arcade"),
    ("cps3", "arcade"),
    ("naomi", "arcade"),
    ("atomiswave", "arcade"),
    ("st-v", "arcade"),
    ("triforce", "arcade"),
    ("segamodel3", "arcade"),
    ("daphne", "arcade"),
    ("amigacd32", "amiga"),
    ("amiga500", "amiga"),
    ("amiga1200", "amiga"),
];

fn slot(system: &str) -> Option<usize> {
    let id = picture_id(system);
    ART.iter().position(|(name, _)| *name == id)
}

/// A system's own name for its picture: an alias's hardware (`sfc` is `snes`), else the name it came with.
pub fn canonical(system: &str) -> &str {
    ALIASES.iter().find(|(alias, _)| *alias == system).map_or(system, |(_, id)| id)
}

/// The embedded picture for a system id, aliases included; None means the card stays plain.
pub fn png(system: &str) -> Option<&'static [u8]> {
    slot(system).map(|i| ART[i].1)
}

/// The decoded picture for a system, decoded on first use and kept.
pub fn get(system: &str) -> Option<&'static Sprite> {
    static DECODED: [std::sync::OnceLock<Option<Sprite>>; ART.len()] = [const { std::sync::OnceLock::new() }; ART.len()];
    let i = slot(system)?;
    // The chosen colour's picture, decoded once.
    if let Some(c) = colour(system) {
        return c.decoded.get_or_init(|| decode(c.art)).as_ref();
    }
    DECODED[i].get_or_init(|| decode(ART[i].1)).as_ref()
}

/// The insert scene's base picture of a system's chosen colour, if it has one other than the original, and the layers of
/// its own plastic that go with it.
pub fn colour_rig(system: &str) -> Option<(&'static [u8], RigLayers)> {
    colour(system).map(Colour::rig)
}

impl Colour {
    /// Its insert scene's base picture and the layers of its own plastic.
    pub fn rig(&self) -> (&'static [u8], RigLayers) {
        (self.rig_base, RigLayers { over: self.rig_over, top: self.rig_top, under: self.rig_under })
    }
}

/// PNG (palette with tRNS, RGB or RGBA) to a sprite; partial alpha counts as solid from half on.
pub fn decode(data: &[u8]) -> Option<Sprite> {
    let mut reader = crate::image::png_reader(std::io::Cursor::new(data))?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).ok()?;
    let data = &buf[..info.buffer_size()];
    let argb = |r: u8, g: u8, b: u8, a: u8| {
        if a < 128 {
            0
        } else {
            0xff00_0000 | (r as u32) << 16 | (g as u32) << 8 | b as u32
        }
    };
    let px: Vec<u32> = match info.color_type {
        png::ColorType::Rgba => data.chunks_exact(4).map(|p| argb(p[0], p[1], p[2], p[3])).collect(),
        png::ColorType::Rgb => data.chunks_exact(3).map(|p| argb(p[0], p[1], p[2], 255)).collect(),
        png::ColorType::GrayscaleAlpha => data.chunks_exact(2).map(|p| argb(p[0], p[0], p[0], p[1])).collect(),
        png::ColorType::Grayscale => data.iter().map(|g| argb(*g, *g, *g, 255)).collect(),
        png::ColorType::Indexed => return None,
    };
    Some(Sprite {
        w: info.width,
        h: info.height,
        px,
    })
}

impl Sprite {
    /// The largest copy that fits `max_w` x `max_h`: whole-number upscales so pixel art stays crisp (a
    /// nearest-neighbour fractional one when the whole number would leave it below three quarters of the
    /// box), area-averaged downscales (with transparency taken into account) when the box is smaller.
    /// `fitted`, the last few kept: a plain case front fits the same console for every game of a system (the Cabinet
    /// made it anew for each case it pulled out, H700 14.11.). Found by its pixels, so any picture may ask.
    pub fn fitted_kept(&self, max_w: i32, max_h: i32) -> Sprite {
        type Kept = Vec<((u32, u32, i32, i32), Vec<u32>, Sprite)>;
        thread_local! {
            static KEPT: std::cell::RefCell<Kept> = const { std::cell::RefCell::new(Vec::new()) };
        }
        let key = (self.w, self.h, max_w, max_h);
        let copy = |s: &Sprite| Sprite { w: s.w, h: s.h, px: s.px.clone() };
        if let Some(f) = KEPT.with(|k| k.borrow().iter().find(|(k, px, _)| *k == key && *px == self.px).map(|(_, _, f)| copy(f))) {
            return f;
        }
        let f = self.fitted(max_w, max_h);
        KEPT.with(|k| {
            let mut k = k.borrow_mut();
            if k.len() >= 4 {
                k.remove(0);
            }
            k.push((key, self.px.clone(), copy(&f)));
        });
        f
    }

    pub fn fitted(&self, max_w: i32, max_h: i32) -> Sprite {
        if self.w == 0 || self.h == 0 || max_w <= 0 || max_h <= 0 {
            return Sprite { w: 0, h: 0, px: Vec::new() };
        }
        let scale = (max_w as f32 / self.w as f32).min(max_h as f32 / self.h as f32);
        if scale >= 1.0 {
            let k = if scale.floor() / scale >= 0.75 { scale.floor() } else { scale };
            let (w, h) = ((self.w as f32 * k) as u32, (self.h as f32 * k) as u32);
            let (sw, sh) = (self.w, self.h);
            let px = (0..h)
                .flat_map(|y| (0..w).map(move |x| ((x * sw / w).min(sw - 1), (y * sh / h).min(sh - 1))))
                .map(|(x, y)| self.px[(y * self.w + x) as usize])
                .collect();
            return Sprite { w, h, px };
        }
        let (w, h) = (((self.w as f32 * scale) as u32).max(1), ((self.h as f32 * scale) as u32).max(1));
        let mut px = Vec::with_capacity((w * h) as usize);
        for ty in 0..h {
            let (y0, y1) = (ty * self.h / h, ((ty + 1) * self.h / h).max(ty * self.h / h + 1));
            for tx in 0..w {
                let (x0, x1) = (tx * self.w / w, ((tx + 1) * self.w / w).max(tx * self.w / w + 1));
                let (mut r, mut g, mut b, mut solid, mut n) = (0u32, 0u32, 0u32, 0u32, 0u32);
                for sy in y0..y1.min(self.h) {
                    for sx in x0..x1.min(self.w) {
                        let p = self.px[(sy * self.w + sx) as usize];
                        n += 1;
                        if p >> 24 != 0 {
                            r += p >> 16 & 0xff;
                            g += p >> 8 & 0xff;
                            b += p & 0xff;
                            solid += 1;
                        }
                    }
                }
                // Solid where at least half the area is; the colour only from the solid part, so no dark fringe.
                px.push(if solid * 2 >= n.max(1) {
                    0xff00_0000 | (r / solid) << 16 | (g / solid) << 8 | b / solid
                } else {
                    0
                });
            }
        }
        Sprite { w, h, px }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_kept_fit_is_the_fit_itself_also_after_another_picture_took_its_place() {
        let pic = |seed: u32| Sprite { w: 40, h: 30, px: (0..1200).map(|i: u32| (i.wrapping_mul(2654435761) ^ seed) | 0xff00_0000).collect() };
        let (a, b) = (pic(1), pic(2));
        for (s, w, h) in [(&a, 25, 25), (&a, 25, 25), (&b, 25, 25), (&a, 80, 70), (&b, 13, 40), (&a, 25, 25)] {
            let (kept, fresh) = (s.fitted_kept(w, h), s.fitted(w, h));
            assert_eq!((kept.w, kept.h, kept.px), (fresh.w, fresh.h, fresh.px), "{w}x{h}");
        }
        // Same size, same pixel count, other pixels: never the other's fit.
        let c = pic(3);
        assert_eq!(c.fitted_kept(25, 25).px, c.fitted(25, 25).px);
    }
    use crate::gfx::Canvas;

    /// Every system the shelf shows a card for (src/main.rs `stripe`, src/hostsys.rs, pocket-core's own).
    const SHELF: &[&str] = &[
        "gb",
        "gbc",
        "gba",
        "nes",
        "snes",
        "megadrive",
        "mastersystem",
        "gamegear",
        "sg-1000",
        "pcengine",
        "segacd",
        "sega32x",
        "pcenginecd",
        "supergrafx",
        "neogeo",
        "neocd",
        "atarilynx",
        "atari7800",
        "ngpc",
        "virtualboy",
        "atarijaguar",
        "3do",
        "fds",
        "colecovision",
        "intellivision",
        "vectrex",
        "wonderswan",
        "xbox",
        "atari2600",
        "pokemini",
        "psx",
        "ps2",
        "n64",
        "psp",
        "psvita",
        "dreamcast",
        "nds",
        "gamecube",
        "wii",
        "wiiu",
        "ps3",
        "3ds",
        "pico-8",
        "ports",
        "pc",
        "scummvm",
        "tic-80",
        "streaming",
        "amiga",
        "atari5200",
        "atari800",
        "c64",
        "zxspectrum",
        "arcade",
    ];

    #[test]
    fn every_arcade_set_shows_the_upright_cabinet_not_the_plain_card() {
        // Before 2026-11-11 MAME, FinalBurn Neo, CPS and NAOMI had no picture: a plain card and the crate. Now each shows
        // the same cabinet: taller than wide, its screen a dark block in the upper half, its body light plastic.
        for s in ["arcade", "mame", "fbn", "fbneo", "cps1", "cps2", "cps3", "naomi", "atomiswave", "daphne"] {
            let sprite = get(s).unwrap_or_else(|| panic!("{s}: plain card"));
            assert!(std::ptr::eq(sprite, get("arcade").unwrap()), "{s}: the arcade cabinet");
            assert!(sprite.h as f32 > sprite.w as f32 * 1.3, "{s}: an upright cabinet, {}x{}", sprite.w, sprite.h);
        }
        let s = get("arcade").unwrap();
        let share = |y0: u32, y1: u32, f: &dyn Fn(f32) -> bool| {
            let row = |y: u32| (0..s.w).map(move |x| s.px[(y * s.w + x) as usize]).filter(|p| p >> 24 != 0);
            let (hit, all) = (y0..y1).flat_map(row).fold((0, 0), |(h, a), p| (h + f(luma(p)) as u32, a + 1));
            hit as f32 / all.max(1) as f32
        };
        let screen = share(s.h * 20 / 100, s.h * 55 / 100, &|l| l < 60.0);
        let base = share(s.h * 75 / 100, s.h, &|l| l > 120.0);
        assert!(screen > 0.5, "the screen is the dark middle of the upper half: {screen}");
        assert!(base > 0.7, "the cabinet's base is light plastic: {base}");
    }

    #[test]
    fn every_shelf_system_has_an_embedded_picture() {
        let missing: Vec<&str> = SHELF.iter().copied().filter(|s| png(s).is_none()).collect();
        assert!(missing.is_empty(), "no picture for {missing:?}");
    }

    #[test]
    fn every_picture_decodes_transparent_and_about_256_wide() {
        for s in SHELF {
            let sprite = get(s).unwrap_or_else(|| panic!("{s}: does not decode"));
            assert!((150..=256).contains(&sprite.w.max(sprite.h)), "{s}: {}x{}", sprite.w, sprite.h);
            assert_eq!(sprite.px.len(), (sprite.w * sprite.h) as usize, "{s}");
            let clear = sprite.px.iter().filter(|p| *p >> 24 == 0).count();
            let solid = sprite.px.iter().filter(|p| *p >> 24 == 0xff).count();
            assert_eq!(clear + solid, sprite.px.len(), "{s}: only fully clear or fully solid pixels");
            assert!(
                clear > 0 && solid > sprite.px.len() / 3,
                "{s}: background cut out, console kept ({clear} clear, {solid} solid)"
            );
        }
    }

    /// Luminance 0..255 of an opaque 0xAARRGGBB pixel.
    fn luma(p: u32) -> f32 {
        0.2126 * ((p >> 16) & 0xff) as f32 + 0.7152 * ((p >> 8) & 0xff) as f32 + 0.0722 * (p & 0xff) as f32
    }

    #[test]
    fn a_picture_that_is_dark_all_over_has_a_light_rim_so_it_shows_on_the_dark_spine() {
        // The spine is #1F1F22 (luminance 31); the tiles of Add games and the switcher are as dark or black. A console that
        // is black or navy all over (Mega Drive, PS2) vanished there. Its outline must stand out; lighter pictures are as drawn.
        let mut dark = Vec::new();
        for s in SHELF {
            let sprite = get(s).unwrap();
            let solid = |x: i64, y: i64| x >= 0 && y >= 0 && x < sprite.w as i64 && y < sprite.h as i64 && sprite.px[(y as usize) * sprite.w as usize + x as usize] >> 24 != 0;
            let (mut near_spine, mut total, mut edge, mut edge_lit) = (0, 0, 0, 0);
            for y in 0..sprite.h as i64 {
                for x in 0..sprite.w as i64 {
                    if !solid(x, y) {
                        continue;
                    }
                    let l = luma(sprite.px[(y as usize) * sprite.w as usize + x as usize]);
                    total += 1;
                    near_spine += (l < 53.0) as u32;
                    if !(solid(x - 1, y) && solid(x + 1, y) && solid(x, y - 1) && solid(x, y + 1)) {
                        edge += 1;
                        edge_lit += (l >= 80.0) as u32;
                    }
                }
            }
            if near_spine as f32 / total as f32 >= 0.85 {
                dark.push(*s);
                assert!(edge_lit as f32 / edge as f32 >= 0.6, "{s}: dark all over and only {edge_lit} of {edge} outline pixels show on the spine");
            }
        }
        // The rule is meant for these (the list changes when a picture is made lighter or a darker one comes).
        for s in ["megadrive", "ps2"] {
            assert!(dark.contains(&s), "{s} counts as dark all over: {dark:?}");
        }
    }

    #[test]
    fn rocknix_alias_names_share_the_picture() {
        for (alias, id) in [
            ("genesis", "megadrive"),
            ("megadrive-japan", "megadrive"),
            ("famicom", "nes"),
            ("sfc", "snes"),
            ("tg16", "pcengine"),
            ("wonderswancolor", "wonderswan"),
            ("pico8", "pico-8"),
            ("wiiware", "wii"),
            ("n3ds", "3ds"),
            ("megacd", "segacd"),
            ("tg16cd", "pcenginecd"),
            ("ngp", "ngpc"),
            ("mame", "arcade"),
            ("fbneo", "arcade"),
            ("cps2", "arcade"),
            ("naomi", "arcade"),
        ] {
            assert_eq!(png(alias).map(|b| b.as_ptr()), png(id).map(|b| b.as_ptr()), "{alias} -> {id}");
            assert!(png(alias).is_some(), "{alias}");
        }
    }

    #[test]
    fn unknown_systems_and_the_special_cards_stay_plain() {
        for s in ["favorites", "settings", "no-such-system", ""] {
            assert!(png(s).is_none() && get(s).is_none(), "{s}");
        }
    }

    #[test]
    fn fitting_scales_up_by_whole_pixels_and_down_to_the_box() {
        let s = Sprite {
            w: 100,
            h: 50,
            px: vec![0xff00_ff00; 5000],
        };
        let up = s.fitted(310, 400);
        assert_eq!((up.w, up.h), (300, 150), "3x, the largest whole factor");
        let odd = s.fitted(175, 400);
        assert_eq!((odd.w, odd.h), (175, 87), "1x would fill only 57% of the box: 1.75x instead");
        let down = s.fitted(80, 80);
        assert!(down.w <= 80 && down.h <= 80 && down.w >= 78, "{}x{}", down.w, down.h);
        assert!(down.px.iter().all(|p| *p == 0xff00_ff00));
    }

    #[test]
    fn transparent_pixels_leave_the_card_showing() {
        let s = Sprite {
            w: 2,
            h: 1,
            px: vec![0x0000_0000, 0xffff_0000],
        };
        let mut buf = vec![0x0012_3456u32; 4];
        let mut c = Canvas::new(&mut buf, 2, 2);
        c.blit_sprite(&s, 0, 0, 0);
        assert_eq!(buf[..2], [0x0012_3456, 0x00ff_0000]);
    }

    #[test]
    fn the_stops_go_round_from_the_original_through_every_colour_both_ways() {
        // The Advance: original, Milky Blue, Pink.
        assert_eq!([0, 1, 2].map(|now| stop_after(2, now, 1)), [1, 2, 0]);
        assert_eq!([0, 1, 2].map(|now| stop_after(2, now, -1)), [2, 0, 1]);
        // The Game Boy Color: original and Dandelion, either key goes to the other.
        assert_eq!([stop_after(1, 0, 1), stop_after(1, 1, 1)], [1, 0]);
        assert_eq!([stop_after(1, 0, -1), stop_after(1, 1, -1)], [1, 0]);
    }

    #[test]
    fn every_colour_is_a_picture_the_size_of_its_console_with_the_rig_of_the_original() {
        for (system, list) in COLOURS {
            let original = decode(ART.iter().find(|(id, _)| id == system).expect(system).1).unwrap();
            assert!(!list.is_empty());
            for c in list.iter() {
                let art = decode(c.art).unwrap_or_else(|| panic!("{system}/{}: the picture", c.id));
                // The same console in the same pose, so it fits where the original does (within a few pixels).
                assert!(art.w.abs_diff(original.w) <= 12 && art.h.abs_diff(original.h) <= 12, "{system}/{}: {}x{} against {}x{}", c.id, art.w, art.h, original.w, original.h);
                assert!(art.px.iter().filter(|p| **p >> 24 == 255).count() > (art.w * art.h) as usize / 3, "{system}/{}: a console, not a speck", c.id);
                // The insert scene's base lies on the original's rig: exactly its canvas.
                let base = decode(c.rig_base).unwrap_or_else(|| panic!("{system}/{}: the rig base", c.id));
                let rig_original = decode(rig_layer(system, "base")).unwrap();
                assert_eq!((base.w, base.h), (rig_original.w, rig_original.h), "{system}/{}: the original rig's canvas", c.id);
                // Each other layer of its own plastic is the original's size too, and the colour gives the layers the console
                // has: a colour without its lid would be the original's lid on another colour's base.
                for (layer, own) in [("over", c.rig_over), ("top", c.rig_top), ("under", c.rig_under)] {
                    let Some(original_layer) = rig_layer_if(system, layer) else {
                        assert!(own.is_none(), "{system}/{}: the original has no {layer}", c.id);
                        continue;
                    };
                    let original_layer = decode(original_layer).unwrap();
                    let own = decode(own.unwrap_or_else(|| panic!("{system}/{}: needs its own {layer} (the original has one)", c.id))).unwrap_or_else(|| panic!("{system}/{}: {layer}", c.id));
                    assert_eq!((own.w, own.h), (original_layer.w, original_layer.h), "{system}/{}: {layer}'s canvas", c.id);
                }
                assert!(!c.name.is_empty() && c.id.chars().all(|ch| ch.is_ascii_lowercase()), "{}", c.id);
            }
            let mut ids: Vec<_> = list.iter().map(|c| c.id).collect();
            ids.dedup();
            assert_eq!(ids.len(), list.len(), "{system}: ids are unique");
        }
    }

    fn rig_layer(system: &str, layer: &str) -> &'static [u8] {
        rig_layer_if(system, layer).unwrap_or_else(|| panic!("{system}: add its rig {layer} here"))
    }

    /// A layer of the original's rig for a console with colours; None where the console has no such layer.
    fn rig_layer_if(system: &str, layer: &str) -> Option<&'static [u8]> {
        Some(match (system, layer) {
            ("gba", "base") => include_bytes!("../assets/media/rig/gba/base.png").as_slice(),
            ("gbc", "base") => include_bytes!("../assets/media/rig/gbc/base.png"),
            ("n64", "base") => include_bytes!("../assets/media/rig/n64/base.png"),
            ("n64", "over") => include_bytes!("../assets/media/rig/n64/over.png"),
            ("gamecube", "base") => include_bytes!("../assets/media/rig/gamecube/base.png"),
            ("gamecube", "top") => include_bytes!("../assets/media/rig/gamecube/top.png"),
            ("gamecube", "under") => include_bytes!("../assets/media/rig/gamecube/under.png"),
            ("gba" | "gbc" | "n64" | "gamecube", _) => return None,
            (other, _) => panic!("{other}: add its rig layers here"),
        })
    }

    #[test]
    fn only_the_consoles_with_a_colour_list_offer_one_and_a_saved_unknown_colour_is_the_original() {
        assert_eq!(colours_of("gba").len(), 2);
        assert_eq!(colours_of("gbah").len(), 2, "an alias is the same console");
        assert_eq!(colours_of("gbc").len(), 1);
        assert_eq!((colours_of("n64").len(), colours_of("gamecube").len()), (2, 1));
        for none in ["snes", "nes", "arcade", "unknown-system"] {
            assert!(colours_of(none).is_empty(), "{none}");
            assert!(!step_colour(none, 1), "{none}: nothing to step");
        }
        let saved = std::collections::BTreeMap::from([("gba".to_string(), "neon".to_string()), ("zzz".to_string(), "pink".to_string())]);
        set_colours_chosen(&saved);
        assert!(colours_chosen().is_empty(), "only a colour that exists is kept");
        assert!(colour("gba").is_none() && colour_name("gba") == "Original");
        set_colours_chosen(&Default::default());
    }
}
