//! The start of a game as a picture and a schedule, for pocket-ui on Linux (`app_cabinet`) and the Android launcher
//! (`android/src/insert.rs`): the console comes up, the case opens, the medium leaves it and goes in, by its console's
//! real mechanism where there is a rig (`insert_rig`), the plain way otherwise. Each host makes the case's front and the
//! medium with its own caches and says where the console stands ([`Stage`]); [`Scene::paint`] draws one moment,
//! [`cues`] are the sounds with their waits and [`hold`] how long the game's window keeps back.

use std::rc::Rc;
use std::time::Duration;

use crate::cabinet::{self, FormFactor};
use crate::cabinet_art::{self as art, Look, Pic};
use crate::gfx::{mix, Canvas};
use crate::insert_rig as ir;
use crate::media::{Game, Medium};
use crate::system_art::Sprite;
use crate::ui_sound::{self, Sound};

/// The launch fade: from the shelf to black, and the game's window comes.
pub const FADE: Duration = Duration::from_millis(250);
/// A sound goes a little before its moment: the output takes about that long to play what it is given.
const SOUND_LEAD: Duration = Duration::from_millis(10);

/// A handheld's power-on (`Scene::power_on`) lets its display stay lit while the rest of the screen goes dark: the fade to
/// black runs between these moments after it would begin (`cabinet::fade_delay`), and the game's window comes when it
/// has, shorter than the fade it replaces (the game is on screen sooner, not later).
pub const WASH: (Duration, Duration) = (Duration::from_millis(90), Duration::from_millis(210));

/// How long after A the game's window may come up: Cozy holds it until the medium is in and the fade has run (a handheld's
/// power-on: its light has spread), the quick way doesn't hold it at all.
pub fn hold(cozy: bool, power_on: bool) -> Option<Duration> {
    cozy.then(|| cabinet::fade_delay(true) + if power_on { WASH.1 } else { FADE })
}

/// The light a handheld's start ends in, when it ends so (`Scene::power_on`): Cozy, a handheld (its game had no TV), and
/// both the motion level and the old-TV switch-on extra allow it, as for the tube (`crt_on`). `system`'s own picture says
/// the colour (`insert_rig::light_of`), a handheld without one a neutral light.
pub fn power_on_light(system: &str, cozy: bool, motion_full: bool, extra_on: bool) -> Option<u32> {
    let lcd = crate::eject::Screen::of(FormFactor::of(system)) == crate::eject::Screen::Lcd;
    (cozy && motion_full && extra_on && lcd).then(|| light_of(system))
}

/// The light of a handheld's display without a picture of its own: a muted grey.
const DISPLAY_LIGHT_DEFAULT: u32 = 0x9fa3a2;

/// The light of `system`'s display: its rig's, the plain picture's (`DISPLAYS`) or a neutral one.
pub fn light_of(system: &str) -> u32 {
    let game = Game::of(system, "x.rom");
    ir::light_of(system, game).or_else(|| plain_display(system).map(|d| d.lit)).unwrap_or(DISPLAY_LIGHT_DEFAULT)
}

/// When a game's window may come, for a host that starts it itself at that moment (Android's `PlayerActivity`): when the
/// hold has run or at the first skip, once. `insert_scene::hold`'s rules.
#[derive(Clone, Copy, Debug)]
pub struct HandOver {
    hold: Option<Duration>,
    released: bool,
    skipped: bool,
}

impl HandOver {
    pub fn new(cozy: bool, power_on: bool) -> HandOver {
        HandOver { hold: hold(cozy, power_on), released: false, skipped: false }
    }

    /// For `scene`'s start: its hold (`Scene::hold`), longer for a console that turns back to its front.
    pub fn of(scene: &Scene) -> HandOver {
        HandOver { hold: scene.hold(), released: false, skipped: false }
    }

    /// A tap or any button: the picture ends and the game comes now. True the first time.
    pub fn skip(&mut self) -> bool {
        let first = !self.skipped;
        self.skipped = true;
        self.hold = None;
        first
    }

    pub fn skipped(&self) -> bool {
        self.skipped
    }

    pub fn released(&self) -> bool {
        self.released
    }

    /// True once, `elapsed` after A when the game may come.
    pub fn take_release(&mut self, elapsed: Duration) -> bool {
        if self.released || self.hold.is_some_and(|hold| elapsed < hold) {
            return false;
        }
        self.released = true;
        true
    }
}

/// One sound of a start: `after` from A, `db` against the UI volume.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cue {
    pub sound: Sound,
    pub after: Duration,
    pub db: f32,
}

/// The sounds of a start, queued at A. Quick uses a short click. Cozy opens the case, seats the game and shuts the
/// mechanism when there is one. A front-loading NES slides the cartridge in before it presses it down.
pub fn cues(cozy: bool, medium: Medium, game: Game, form: FormFactor, mechanism: Option<ir::Kind>, playful: bool) -> Vec<Cue> {
    let in_at = cabinet::in_at(cozy).saturating_sub(SOUND_LEAD);
    let at = |sound: Sound, moment: Duration, db: f32| Cue { sound, after: moment.saturating_sub(ui_sound::hit_at(sound)), db };
    if !cozy {
        return vec![at(Sound::Insert, in_at, ui_sound::QUICK_DB)];
    }
    let opens = if playful { ir::case_opens_at() } else { cabinet::plain_case_opens_at(true) };
    let opening = at(ui_sound::opening(medium), opens.saturating_sub(SOUND_LEAD), 0.0);
    let insert = ui_sound::going_in(game, form);
    let shut_at = in_at + ir::SHUT_AFTER;
    match mechanism {
        Some(ir::Kind::Door) if game == Game::Nes => {
            let slide_at = in_at - Duration::from_millis(ir::PRESS_MS as u64);
            vec![opening, at(Sound::NesSlide, slide_at, 0.0), at(Sound::NesPress, in_at, 0.0), at(Sound::NesDoorShut, shut_at, 0.0)]
        }
        Some(kind) => {
            let down = if game.is_round() { Sound::DiscDrop } else { insert };
            let shut = match (game, kind) {
                (Game::Umd, _) => Sound::UmdShut,
                (_, ir::Kind::Tray) => Sound::TrayShut,
                _ => Sound::DiscShut,
            };
            vec![opening, at(down, in_at, 0.0), at(shut, shut_at, 0.0)]
        }
        None => vec![opening, at(insert, in_at, 0.0)],
    }
}

/// A handheld's power switch at its start (`power_on_light`): slid on as its power light comes on, 90 ms after the game
/// is in (`insert_rig::pose_of`'s `led`; a PSP's after it has turned back to its front). Only where the start ends in the
/// display's light, which the caller knows (`App::apply_power_on`).
pub fn switch_on_cue(system: &str, kind: crate::media::Game) -> Cue {
    let turns = ir::files(system, kind).is_some_and(|f| f.front.is_some());
    let lights = cabinet::IN_AT_COZY + if turns { ir::TURN_BACK } else { Duration::ZERO };
    let led = lights + Duration::from_millis(ir::LED_AFTER_MS);
    Cue { sound: Sound::SwitchOn, after: led.saturating_sub(SOUND_LEAD + ui_sound::hit_at(Sound::SwitchOn)), db: 0.0 }
}

/// Where the console stands on this screen. `k` is screen pixels per pixel of pocket-ui's handheld layout, on which the
/// animation's measures are made: 1 on Linux.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stage {
    /// The floor under the console.
    pub floor: i32,
    pub k: f32,
}

/// A disc's radius over the drive of a console `sw` pixels wide: a 12 cm disc a sixth of the console, the GameCube's
/// 8 cm one and a UMD smaller by their real size.
pub fn over_drive(game: Game, sw: f32, k: f32) -> f32 {
    (sw * 0.16).clamp(46.0 * k, 90.0 * k) * game.mm().1 / 120.0
}

/// Where the medium goes on a system's picture (`assets/systems`): `x`, `y` as parts of the picture, what stands on the
/// picture beside the console (a pad, a cable) left out by `body`, the console's share of its width. Measured on the
/// pictures, whose consoles are turned and rarely in their middle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SlotAt {
    pub x: f32,
    pub y: Option<f32>,
    pub body: f32,
}

/// `(system, x, y, body)`; y 0 keeps its form factor's own line (`paint_plain`).
const SLOTS: &[(&str, f32, f32, f32)] = &[
    ("nes", 0.56, 0.34, 0.72),
    ("snes", 0.66, 0.22, 0.60),
    ("n64", 0.73, 0.24, 0.62),
    ("megadrive", 0.70, 0.28, 0.60),
    ("gb", 0.58, 0.10, 1.0),
    ("gbc", 0.62, 0.08, 1.0),
    ("gba", 0.55, 0.10, 1.0),
    ("gba-sp", 0.19, 0.835, 1.0),
    ("nds", 0.57, 0.07, 1.0),
    ("3ds", 0.60, 0.07, 1.0),
    ("psp", 0.48, 0.40, 1.0),
    ("mastersystem", 0.62, 0.20, 0.92),
    ("pcengine", 0.74, 0.33, 0.52),
    ("supergrafx", 0.39, 0.17, 0.75),
    ("sega32x", 0.50, 0.22, 1.0),
    ("atari2600", 0.55, 0.23, 0.80),
    ("atari7800", 0.55, 0.29, 1.0),
    ("atarijaguar", 0.48, 0.20, 1.0),
    ("colecovision", 0.77, 0.30, 1.0),
    ("intellivision", 0.48, 0.29, 1.0),
    ("neogeo", 0.55, 0.19, 1.0),
    ("sg-1000", 0.61, 0.16, 0.90),
    ("virtualboy", 0.57, 0.05, 0.85),
    ("fds", 0.42, 0.30, 1.0),
    ("psx", 0.68, 0.29, 0.62),
    ("ps2", 0.55, 0.15, 0.50),
    ("gamecube", 0.70, 0.23, 0.50),
    ("neocd", 0.60, 0.33, 0.70),
    ("3do", 0.55, 0.40, 0.75),
    ("xbox", 0.37, 0.60, 1.0),
    ("wii", 0.15, 0.10, 0.50),
    ("ps3", 0.45, 0.72, 1.0),
    ("wiiu", 0.25, 0.58, 1.0),
    ("saturn", 0.45, 0.30, 1.0),
    // Computers (2026-11-02): where the game's door or port is on each picture.
    ("c64", 0.80, 0.30, 1.0),
    ("atari800", 0.48, 0.28, 1.0),
    ("atari5200", 0.50, 0.30, 1.0),
    ("amiga", 0.90, 0.42, 1.0),
    ("zxspectrum", 0.77, 0.25, 1.0),
    // The phone (J2ME): nothing goes in, its display lights up where `PLAIN_DISPLAYS` has it.
    ("j2me", 0.48, 0.42, 1.0),
    // The arcade cabinet: no medium goes in (`lights_up_only`), its screen lights up.
    ("arcade", 0.45, 0.38, 1.0),
    // The crate of games: where a system without a picture of its own puts its medium (`STAND_IN`).
    ("ports", 0.50, 0.40, 0.80),
];

/// What stands in for the console of a system without a picture: the crate of games, its medium goes into it.
pub const STAND_IN: &str = "ports";

/// The system whose console picture, rig and display a start shows: a handheld without a picture of its own shows the
/// closest handheld that has one, never the crate of games (a TV or a box would say the game runs somewhere else).
pub fn scene_system(system: &str) -> &str {
    match system {
        // Their games are the bigger console's (a GBA Video is a GBA cartridge, PSP Minis go into the PSP, DSiWare into the DS).
        "gbav" => "gba",
        "pspminis" => "psp",
        "ndsiware" => "nds",
        // Handhelds without a picture of their own: the closest we have. Landscape ones and the tiny Arduboy: the Game Gear; the
        // upright ones of the Game Boy's build: the Game Boy; the GP32 (a GBA-shaped handheld): the GBA; the Palm: the phone.
        "gp32" => "gba",
        "gameandwatch" | "arduboy" => "gamegear",
        "supervision" | "gamate" | "megaduck" => "gb",
        "palm" => "j2me",
        _ => system,
    }
}

/// Where the medium goes on `system`'s picture; a picture not listed has its console in its middle.
pub fn slot_of(system: &str) -> SlotAt {
    let id = if crate::system_art::get(system).is_some() { crate::system_art::picture_id(system) } else { STAND_IN };
    SLOTS.iter().find(|s| s.0 == id).map_or(SlotAt { x: 0.5, y: None, body: 1.0 }, |s| SlotAt { x: s.1, y: Some(s.2), body: s.3 })
}

/// Systems whose games come on a medium pocket-ui has no picture of (floppy disk, tape) or on none (an arcade board), or
/// go in where the console's picture doesn't show it: their start shows no medium going in, a false one would be wrong.
/// The case fades and the machine lights up. A PSP takes its UMD in a door at its back, label out: its rig shows that
/// (Cozy); the plain animation must not show the disc dropping on its screen.
pub fn lights_up_only(system: &str) -> bool {
    // The CD32 has the Amiga's picture but its games are discs.
    system != "amigacd32"
        && matches!(
            crate::system_art::canonical(system),
            "psp" | "arcade" | "c64" | "c128" | "c16" | "vic20" | "pet" | "amiga" | "amiga500" | "amiga1200" | "zxspectrum" | "zx81" | "msx" | "msx2" | "atarist"
                | "atari800" | "amstradcpc" | "x68000" | "x1" | "pc-8800" | "pc-9800" | "bbcmicro" | "apple2" | "dragon" | "trs-80"
                // A phone game comes over the air: nothing goes in, it loads onto the phone (`paint_plain_display`).
                | "j2me"
        )
}

/// A rig's pictures, ready to paint: the console's at screen size, what moves at its own (drawn through a map).
#[derive(Clone)]
pub struct Rigged {
    pub rig: ir::Rig,
    /// The console's front, at the art's own size, for a console that turns round to its back first (the PSP).
    pub front: Option<Pic>,
    pub base: Pic,
    pub over: Option<Pic>,
    pub top: Option<Pic>,
    pub under: Option<Pic>,
    /// Screen pixels per pixel of the rig's art.
    pub scale: f32,
    /// The handheld's display lit: its pixels taken to its light, nothing elsewhere, at the base's size (`Display`).
    pub lit: Option<Pic>,
    /// A clamshell's display lit on its lid's underside (`ir::Lid::display`), at the art's own size as `under` (drawn through
    /// the lid's map).
    pub lid_lit: Option<Pic>,
    /// The front's display lit (`ir::Rig::front_display`), at the art's size as `front`; `front` has its glass dark then.
    pub front_lit: Option<Pic>,
    /// A cartridge's edge, one column of its own plastic (`edge_of`), keyed by cartridge picture pointer so a cached rig can be shared across games.
    pub edge: std::cell::RefCell<Option<(usize, Rc<Pic>)>>,
    /// The see-through rows under the console's lowest visible one (`base` and `over` together): it stands on the floor
    /// by what is seen of it (18.11.: up to 30 of them under some consoles, which floated over the shelf's board).
    pub foot: i32,
    /// What is seen of the console (`seen_box`), made once when the rig is prepared and kept.
    pub seen: std::cell::OnceCell<(i32, i32, i32, i32)>,
}

/// Bounded thread-local cache of decoded and scaled rigs (`Rigged::cached_scaled`), so repeated launches and focus
/// warm-up (`warm_rig_scaled`) never decode PNGs or run `scaled_smooth` on Frame 0 (Brick 1024x768, blocks = 1.36).
const RIG_CACHE_KEEP: usize = 6;

type RigCacheKey = (String, Option<&'static str>, u32);

thread_local! {
    static RIG_CACHE: std::cell::RefCell<Vec<(RigCacheKey, Option<Rc<Rigged>>)>> = const { std::cell::RefCell::new(Vec::new()) };
    static RIG_LOADS: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

/// How many rigs this thread decoded and scaled (cache misses), for tests and diagnostics.
pub fn rig_loads() -> u32 {
    RIG_LOADS.with(std::cell::Cell::get)
}

/// Decodes and scales `system`'s rig at `blocks` ahead of time into the cache (`Rigged::cached_scaled`); true if the system has one.
pub fn warm_rig_scaled(system: &str, game: Game, blocks: f32) -> bool {
    Rigged::cached_scaled(system, game, blocks).is_some()
}

fn rig_cache_key(system: &str, game: Game, blocks: f32) -> RigCacheKey {
    let pic_id = crate::system_art::picture_id(system);
    let id = if game == Game::SnesUs {
        "snes-us"
    } else if system == "famicom" {
        "famicom"
    } else {
        pic_id
    };
    let colour = crate::system_art::colour(system).filter(|_| id == pic_id).map(|c| c.id);
    (id.to_string(), colour, blocks.to_bits())
}

/// The see-through rows at the bottom of a picture `w` wide (nothing seen at all: none).
pub fn clear_rows_below(px: &[u32], w: usize) -> i32 {
    let h = px.len() / w.max(1);
    (0..h).rev().take_while(|&y| px[y * w..(y + 1) * w].iter().all(|p| p >> 24 <= 0x40)).count().min(h.saturating_sub(1)) as i32
}

impl Rigged {
    /// `blocks`: screen pixels per pixel of the art (`ir::SCALE` on Linux).
    pub fn load(system: &str, game: Game, blocks: i32) -> Option<Rigged> {
        Self::load_scaled(system, game, blocks as f32)
    }

    /// `load` at any number of screen pixels per pixel of the art (not whole on a short screen).
    pub fn load_scaled(system: &str, game: Game, blocks: f32) -> Option<Rigged> {
        Self::of_files(ir::files(system, game)?, blocks)
    }

    /// Cached `load_scaled` (bypassed when `POCKETUI_RIG_ASSETS` overrides rig files from disk).
    pub fn cached_scaled(system: &str, game: Game, blocks: f32) -> Option<Rc<Rigged>> {
        if std::env::var_os("POCKETUI_RIG_ASSETS").is_some() {
            RIG_LOADS.with(|n| n.set(n.get() + 1));
            return Self::load_scaled(system, game, blocks).map(Rc::new);
        }
        let key = rig_cache_key(system, game, blocks);
        let hit = RIG_CACHE.with(|c| {
            let mut cache = c.borrow_mut();
            let pos = cache.iter().position(|(k, _)| *k == key)?;
            let entry = cache.remove(pos);
            let val = entry.1.clone();
            cache.push(entry);
            Some(val)
        });
        if let Some(val) = hit {
            return val;
        }
        RIG_LOADS.with(|n| n.set(n.get() + 1));
        let built = Self::load_scaled(system, game, blocks).map(Rc::new);
        RIG_CACHE.with(|c| {
            let mut cache = c.borrow_mut();
            if cache.len() >= RIG_CACHE_KEEP {
                cache.remove(0);
            }
            cache.push((key, built.clone()));
        });
        built
    }

    /// The rig of `f` at `blocks` screen pixels per pixel of its art.
    pub fn of_files(f: ir::Files, blocks: f32) -> Option<Rigged> {
        let pic = |data: &[u8]| {
            let s = crate::system_art::decode(data)?;
            Some(Pic::of(s.w as i32, s.h as i32, s.px))
        };
        let art = pic(&f.base)?;
        let lit = f.rig.display.map(|d| {
            let mask = ir::display_mask(&art.px, art.w as usize, art.h as usize, d.seeds_of(), d.fuzz);
            enlarged_to(Pic::of(art.w, art.h, ir::lit_pixels(&art.px, &mask, art.w as usize, d.lit)), blocks)
        });
        let mut front = f.front.as_ref().and_then(|d| pic(d));
        let front_lit = f.rig.front_display.zip(front.as_mut()).map(|(d, fp)| {
            let mask = ir::display_mask(&fp.px, fp.w as usize, fp.h as usize, d.seeds_of(), d.fuzz);
            // Off, the glass is dark (the picture's is light, as if on): its own pixels kept a little, under a dark tint.
            // With the glass's anti-aliased edge (the ring `lit_pixels` lights with it), else a light outline stays round it.
            let w = fp.w as usize;
            let grow = |m: &[bool]| -> Vec<bool> {
                (0..m.len()).map(|i| m[i] || (i % w > 0 && m[i - 1]) || (i % w + 1 < w && m[i + 1]) || (i >= w && m[i - w]) || m.get(i + w).copied().unwrap_or(false)).collect()
            };
            // Two pixels out: the photo's edge is soft over two.
            let near = grow(&grow(&mask));
            for (i, p) in fp.px.iter_mut().enumerate() {
                if near[i] && *p >> 24 == 255 {
                    *p = ir::glass_off(*p);
                }
            }
            Pic::of(fp.w, fp.h, ir::lit_pixels(&fp.px, &mask, fp.w as usize, d.lit))
        });
        let lid_lit = f.rig.clamshell.and_then(|l| l.display).zip(f.under.as_ref().and_then(|d| pic(d))).map(|(d, u)| {
            let mask = ir::display_mask(&u.px, u.w as usize, u.h as usize, d.seeds_of(), d.fuzz);
            Pic::of(u.w, u.h, ir::lit_pixels(&u.px, &mask, u.w as usize, d.lit))
        });
        let (mut base, over) = (enlarged_to(art, blocks), f.over.and_then(|d| pic(&d)).map(|p| enlarged_to(p, blocks)));
        // What `over` covers whole is never seen on the base: each frame only copies a console's pixels once.
        if let Some(o) = over.as_ref().filter(|o| (o.w, o.h) == (base.w, base.h)) {
            for (b, t) in base.px.iter_mut().zip(&o.px) {
                if t >> 24 == 255 {
                    *b = 0;
                }
            }
        }
        let foot = std::iter::once(&base).chain(over.as_ref()).map(|p| clear_rows_below(&p.px, p.w as usize)).min().unwrap_or(0);
        let seen = std::cell::OnceCell::new();
        let _ = seen.set(seen_box(&base, over.as_ref()));
        Some(Rigged { rig: f.rig, front, front_lit, base, over, top: f.top.and_then(|d| pic(&d)), under: f.under.and_then(|d| pic(&d)), scale: blocks, lit, lid_lit, edge: Default::default(), foot, seen })
    }
}

/// The console's picture at (`x`, `y`) painted again over the screen rows `rows`, solid pixels only: what the medium
/// sinks behind is the console's outline, whatever its tilt.
fn paint_over(c: &mut Canvas, sp: &crate::system_art::Sprite, (x, y): (i32, i32), rows: (i32, i32)) {
    let (r0, r1) = (rows.0.max(y).max(0), rows.1.min(y + sp.h as i32).min(c.h));
    if c.n() != 1 {
        // Painted natively (docs/dev/native-paint.md): the four design pixels of each screen pixel as below, averaged.
        return crate::cabinet_art::cells_over(c, x, x + sp.w as i32, r0, r1, |col, row, bg| {
            let p = sp.px[((row - y) as u32 * sp.w) as usize + (col - x) as usize];
            if p >> 24 == 255 { p & 0xffffff } else { bg }
        });
    }
    let c_w = c.w;
    for row in r0..r1 {
        let src = &sp.px[((row - y) as u32 * sp.w) as usize..][..sp.w as usize];
        for (i, &p) in src.iter().enumerate() {
            let col = x + i as i32;
            if p >> 24 == 255 && col >= 0 && col < c.w {
                c.pixels_mut()[(row * c_w + col) as usize] = p & 0xffffff;
            }
        }
    }
}

/// A display's lit pixels (`Rigged::lit`) laid over the console drawn at (`x`, `y`), `amount` of the way (0-1) from the
/// picture's own to its light. Only the glass is touched, so only the glass's rows are walked.
fn lit_over(c: &mut Canvas, lit: &Pic, x: i32, y: i32, amount: f32) {
    let t = (amount.clamp(0.0, 1.0) * 255.0).round() as u32;
    if t == 0 {
        return;
    }
    if c.n() != 1 {
        // Painted natively (docs/dev/native-paint.md): the four design pixels of each screen pixel as below, averaged.
        let (y0, y1) = (y.max(0), (y + lit.h).min(c.h));
        return crate::cabinet_art::cells_over(c, x, x + lit.w, y0, y1, |col, row, bg| {
            let p = lit.px[((row - y) * lit.w + col - x) as usize];
            if p >> 24 == 255 { mix(bg, p & 0xff_ffff, t) } else { bg }
        });
    }
    let c_w = c.w;
    for row in y.max(0)..(y + lit.h).min(c.h) {
        let src = &lit.px[((row - y) * lit.w) as usize..][..lit.w as usize];
        for (i, &p) in src.iter().enumerate() {
            let col = x + i as i32;
            if p >> 24 == 255 && col >= 0 && col < c.w {
                let d = &mut c.pixels_mut()[(row * c_w + col) as usize];
                *d = mix(*d, p & 0xff_ffff, t);
            }
        }
    }
}

/// Where a handheld's display is on its plain picture (`assets/systems`), for the handhelds without a rig: points on the
/// glass as parts of the picture, filled out as a rig's (`insert_rig::display_mask`), and the light it ends in.
#[derive(Clone, Copy, Debug)]
pub struct PlainDisplay {
    pub at: &'static [(f32, f32)],
    pub fuzz: u32,
    pub lit: u32,
    /// Its power light on the picture (parts of its width and height), where the picture shows one: lit in the Cabinet as the
    /// original's (`frame_led::Model::of_system`).
    pub led: Option<(f32, f32)>,
}

/// `(picture, display)`.
const DISPLAYS: &[(&str, PlainDisplay)] = &[
    ("gb", PlainDisplay { at: &[(0.576, 0.311)], fuzz: 24, lit: 0xabb080, led: Some((0.346, 0.218)) }),
    ("gbc", PlainDisplay { at: &[(0.546, 0.289)], fuzz: 24, lit: 0xa3b0b0, led: Some((0.307, 0.163)) }),
    ("gba", PlainDisplay { at: &[(0.506, 0.411)], fuzz: 24, lit: 0x919191, led: Some((0.848, 0.368)) }),
    ("gba-sp", PlainDisplay { at: &[(0.6625, 0.262)], fuzz: 24, lit: 0x9c9d9e, led: Some((0.628, 0.692)) }),
    ("gamegear", PlainDisplay { at: &[(0.5125, 0.331)], fuzz: 24, lit: 0x8c9298, led: Some((0.246, 0.173)) }),
    ("atarilynx", PlainDisplay { at: &[(0.544, 0.396)], fuzz: 15, lit: 0x808a94, led: None }),
    ("ngpc", PlainDisplay { at: &[(0.480, 0.430)], fuzz: 24, lit: 0x9ea8af, led: None }),
    ("wonderswan", PlainDisplay { at: &[(0.5, 0.427)], fuzz: 24, lit: 0x9aabb0, led: None }),
    ("pokemini", PlainDisplay { at: &[(0.574, 0.383)], fuzz: 24, lit: 0xb0b0b0, led: None }),
    ("nds", PlainDisplay { at: &[(0.591, 0.230), (0.467, 0.631)], fuzz: 24, lit: 0x878891, led: None }),
    ("3ds", PlainDisplay { at: &[(0.539, 0.255), (0.415, 0.634)], fuzz: 24, lit: 0xb0b0b0, led: None }),
    ("psp", PlainDisplay { at: &[(0.502, 0.376)], fuzz: 13, lit: 0x838b94, led: Some((0.796, 0.902)) }),
    // The phone (`assets/systems/j2me.png`): its one display, a soft green-grey LCD.
    ("j2me", PlainDisplay { at: &[(0.48, 0.42)], fuzz: 24, lit: 0xa0b09c, led: None }),
    ("psvita", PlainDisplay { at: &[(0.492, 0.420)], fuzz: 13, lit: 0x83868e, led: None }),
];

/// The display of a system's plain picture, if it has one listed.
pub fn plain_display(system: &str) -> Option<&'static PlainDisplay> {
    let id = crate::system_art::picture_id(system);
    DISPLAYS.iter().find(|d| d.0 == id).map(|d| &d.1)
}

/// `p` `n` times as large, each pixel a block.
fn enlarged(p: Pic, n: i32) -> Pic {
    let (w, h) = (p.w * n, p.h * n);
    let mut px = Vec::with_capacity((w * h) as usize);
    for y in 0..h {
        let row = &p.px[((y / n) * p.w) as usize..][..p.w as usize];
        for x in 0..w {
            px.push(row[(x / n) as usize]);
        }
    }
    Pic::of(w, h, px)
}

/// `p` at `blocks` screen pixels a pixel: whole blocks as `enlarged`; between two whole numbers (a short screen's console,
/// `Scene::load_rig_scaled`) made two blocks large and averaged down, so the console is as sharp as it is small.
fn enlarged_to(p: Pic, blocks: f32) -> Pic {
    if (blocks - blocks.round()).abs() < 0.01 {
        return enlarged(p, blocks.round() as i32);
    }
    let (w, h) = (((p.w as f32 * blocks).round() as i32).max(1), ((p.h as f32 * blocks).round() as i32).max(1));
    enlarged(p, 2).scaled_smooth(w, h)
}

/// A front-loader's cartridge inside the console, seen through its opening: in its shadow (of 256).
const INSIDE_SHADE: u32 = 48;

/// A cartridge's edge as it shows while it turns: one column, each row the plastic just inside its picture's right
/// outline, a little darker (an edge is out of the light). Made once from the cartridge's own picture, never drawn.
fn edge_of(p: &Pic) -> Pic {
    let px = (0..p.h)
        .map(|y| {
            let row = &p.px[(y * p.w) as usize..][..p.w as usize];
            match row.iter().rposition(|q| q >> 24 > 200) {
                Some(x) => {
                    let q = row[x.saturating_sub(2)];
                    (q & 0xff00_0000) | mix(q & 0xffffff, 0x000000, 70)
                }
                None => 0,
            }
        })
        .collect();
    Pic::of(1, p.h, px)
}

/// From how far on its way (`Pose::fly`) a medium for a slot at the console's back sinks behind the console's top.
pub const REAR_SINK_FROM: f32 = 0.7;

/// How far open a lid is (0 shut, 1 all the way) before it is drawn behind its console's rear wall instead of over it.
const LID_BEHIND_OPEN: f32 = 0.35;

/// How far a jewel or DVD case's cover swings open for its disc to be lifted out, degrees: short of edge-on, so it never
/// thins to a line (the first version squeezed it flat to 90 degrees).
const COVER_OPEN_DEG: f32 = 78.0;

/// The black tray inside a jewel case (PlayStation and Saturn games come in one, `assets/media/case/ps1.json`).
const CASE_TRAY: u32 = 0x15161a;

/// A rectangle of `rgb` over what is on `c`, `alpha` of 256.
fn fill_alpha(c: &mut Canvas, x: i32, y: i32, w: i32, h: i32, rgb: u32, alpha: u32) {
    let keep = 256 - alpha.min(256);
    if c.n() != 1 {
        // Painted natively (docs/dev/native-paint.md): a screen pixel the rectangle covers only partly by that part.
        let (x0, x1, y0, y1) = (x.max(0), (x + w).min(c.w), y.max(0), (y + h).min(c.h));
        return crate::cabinet_art::cells_over(c, x0, x1, y0, y1, |_, _, bg| mix(bg & 0xffffff, rgb, 255 - (keep * 255 / 256)));
    }
    let c_w = c.w;
    for yy in y.max(0)..(y + h).min(c.h) {
        for xx in x.max(0)..(x + w).min(c.w) {
            let d = &mut c.pixels_mut()[(yy * c_w + xx) as usize];
            *d = mix(*d & 0xffffff, rgb, 255 - (keep * 255 / 256));
        }
    }
}

/// How much of the case still shows, 1-0: it fades while the medium leaves it.
fn case_shown(st: cabinet::Insert) -> f32 {
    1.0 - ((st.carry - 0.3) / 0.4).clamp(0.0, 1.0)
}

/// The box (x, y, w, h, within the picture) of what is seen of a console's pictures: pixels more than a quarter opaque.
/// Single row-major pass (a column-outer scan misses cache lines on the Brick's Cortex-A53).
fn seen_box(base: &Pic, over: Option<&Pic>) -> (i32, i32, i32, i32) {
    let (mut x0, mut y0, mut x1, mut y1) = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
    for pic in std::iter::once(base).chain(over) {
        let (w, h) = (pic.w, pic.h);
        for y in 0..h {
            let row = &pic.px[(y * w) as usize..((y + 1) * w) as usize];
            if let Some(left) = row.iter().position(|&p| p >> 24 > 0x40) {
                let right = row.iter().rposition(|&p| p >> 24 > 0x40).unwrap();
                x0 = x0.min(left as i32);
                x1 = x1.max(right as i32 + 1);
                y0 = y0.min(y);
                y1 = y1.max(y + 1);
            }
        }
    }
    if x1 > x0 && y1 > y0 { (x0, y0, x1 - x0, y1 - y0) } else { (0, 0, base.w, base.h) }
}

/// Whether two boxes (x, y, w, h) overlap.
fn overlap(a: (i32, i32, i32, i32), b: (i32, i32, i32, i32)) -> bool {
    a.0 < b.0 + b.2 && b.0 < a.0 + a.2 && a.1 < b.1 + b.3 && b.1 < a.1 + a.3
}

/// How far on its way home (`case_back`) a case is in its row on the shelf, behind the stage the console stands on: from
/// here it is painted under the console (ritual 19.11.: a GBA's case went back into the row through the GBA).
pub const CASE_IN_ROW: f32 = 0.5;

/// Where a case goes back to once its game is out of it, as the host shows it at rest: its spine standing with its
/// middle at `at` (screen pixels), `scale` screen pixels per shelf pixel (`cabinet::Size`). The case shuts, turns its
/// spine to you in perspective (`cabinet_art::draw_turning`, the turn it made coming out, backwards) and goes there, into
/// the light of the dimmed shelf; on the way back from a game the same, backwards. Without one (Easy, Kids, Android) it
/// fades out as before.
#[derive(Clone)]
pub struct Home {
    pub spine: Rc<Pic>,
    pub at: (f32, f32),
    pub scale: f32,
}

/// Cozy, a case with a [`Home`], `elapsed` after A: (how far its cover has shut again, how far it is on its way back),
/// each 0 to 1. A cover shuts once the disc is clear of it (`insert_rig::pose`'s `fly` starts at 420 ms, a third of the
/// way by 600 ms); the case goes back while the game flies to its console, and is home before the game is in
/// (`cabinet::IN_AT_COZY`). On the way back from a game the same backwards: the case is out and open before its disc
/// is back beside it.
pub fn case_back(elapsed: Duration, opens: bool) -> (f32, f32) {
    let ms = elapsed.as_secs_f32() * 1000.0;
    let phase = |a: f32, b: f32| ((ms - a) / (b - a)).clamp(0.0, 1.0);
    let smooth = |p: f32| p * p * (3.0 - 2.0 * p);
    if opens {
        (smooth(phase(590.0, 780.0)), smooth(phase(760.0, 1100.0)))
    } else {
        (1.0, smooth(phase(440.0, 900.0)))
    }
}

/// What the page draws round a case's place (the selection's frame, its shadow on the board), canvas pixels. On the way
/// back from a game (`eject`) the place is empty, as on the handhelds, and these come with the case (`Launch::paint_scene`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Mark {
    Frame { x: i32, y: i32, w: i32, h: i32, t: i32, color: u32 },
    Shadow { x: i32, y: i32, w: i32, h: i32, amount: u8 },
}

impl Mark {
    /// The same on a canvas `sx`, `sy` times as big.
    pub fn scaled(self, (sx, sy): (f32, f32)) -> Mark {
        let by = |v: i32, k: f32| (v as f32 * k) as i32;
        match self {
            Mark::Frame { x, y, w, h, t, color } => Mark::Frame { x: by(x, sx), y: by(y, sy), w: by(w, sx), h: by(h, sy), t: by(t, sx.min(sy)).max(1), color },
            Mark::Shadow { x, y, w, h, amount } => Mark::Shadow { x: by(x, sx), y: by(y, sy), w: by(w, sx), h: by(h, sy), amount },
        }
    }

    /// Over what is on `c`, `shown` of 256.
    pub fn paint(self, c: &mut Canvas, shown: u32) {
        match self {
            Mark::Shadow { x, y, w, h, amount } => c.shade_rect(x, y, w, h, 0, (amount as u32 * shown / 256) as u8),
            Mark::Frame { x, y, w, h, t, color } => {
                // `shade_rect` mixes as `blend` does, pixel for pixel at `n` = 1, whole screen pixels at `n` = 2.
                for (rx, ry, rw, rh) in [(x, y, w, t), (x, y + h - t, w, t), (x, y + t, t, h - 2 * t), (x + w - t, y + t, t, h - 2 * t)] {
                    if rw > 0 && rh > 0 {
                        c.shade_rect(rx, ry, rw, rh, color, shown.min(255) as u8);
                    }
                }
            }
        }
    }
}

/// Where the phone's display is in its picture (`assets/systems/j2me.png`), as parts of its width and height: found once
/// from the picture with the display's own seed (`plain_display`).
fn phone_display() -> Option<(f32, f32, f32, f32)> {
    static AT: std::sync::OnceLock<Option<(f32, f32, f32, f32)>> = std::sync::OnceLock::new();
    *AT.get_or_init(|| {
        let sp = crate::system_art::get("j2me")?;
        let d = plain_display("j2me")?;
        let (w, h) = (sp.w as usize, sp.h as usize);
        let mask = ir::display_mask(&sp.px, w, h, d.at.iter().map(|&(x, y)| (x * w as f32, y * h as f32)), d.fuzz);
        let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0, 0);
        for (i, _) in mask.iter().enumerate().filter(|(_, on)| **on) {
            let (x, y) = (i % w, i / w);
            (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x + 1), y1.max(y + 1));
        }
        (x1 > x0 && y1 > y0).then(|| (x0 as f32 / w as f32, y0 as f32 / h as f32, x1 as f32 / w as f32, y1 as f32 / h as f32))
    })
}

/// How full the phone's loading bar is `elapsed` after A: from when a medium would leave its case until it would be in
/// (the shared timeline, `insert_rig::pose`), so the phone keeps the other consoles' beat: loaded at 1.2 s, lit at 1.4 s.
pub fn phone_loaded(elapsed: Duration, cozy: bool) -> f32 {
    let (a, b) = (Duration::from_millis(420), cabinet::in_at(cozy));
    let a = if cozy { a } else { a.mul_f32(b.as_secs_f32() / cabinet::IN_AT_COZY.as_secs_f32()) };
    ((elapsed.as_secs_f32() - a.as_secs_f32()) / (b - a).as_secs_f32()).clamp(0.0, 1.0)
}

/// A phone game "loading" onto the phone: a bar on its still dark display fills while the case fades, where another
/// console's medium would go in; then the display goes on as every handheld's (`paint_plain_display`). Painted where
/// `paint_plain` has the phone, risen or rising.
fn phone_loading(c: &mut Canvas, elapsed: Duration, sp: &Sprite, foot: i32, cozy: bool, Stage { floor, k }: Stage) {
    let Some((fx0, fy0, fx1, fy1)) = phone_display() else { return };
    let loaded = phone_loaded(elapsed, cozy);
    let st = cabinet::insert_at(elapsed, cozy);
    if elapsed < Duration::from_millis(300) || st.rise < 0.9 {
        return;
    }
    let (sw, sh) = (sp.w as i32, sp.h as i32);
    let px = |n: i32| (n as f32 * k) as i32;
    let sx = (c.w - sw) / 2;
    let sy_final = floor - sh + foot;
    let sy = sy_final + ((1.0 - st.rise) * (sh + px(160)).max(c.h - sy_final) as f32) as i32;
    let (dx0, dy0) = (sx + (fx0 * sw as f32) as i32, sy + (fy0 * sh as f32) as i32);
    let (dw, dh) = (((fx1 - fx0) * sw as f32) as i32, ((fy1 - fy0) * sh as f32) as i32);
    // An LCD's own dark: an outlined bar a little under the display's middle, filling from the left.
    let ink = 0x24301f;
    // At least 7 pixels high, so the frame, a gap and the fill all show on a small screen.
    let (bw, bh) = ((dw * 64 / 100).max(10), (dh / 5).max(7));
    let (bx, by) = (dx0 + (dw - bw) / 2, dy0 + dh * 3 / 5 - bh / 2);
    let t = (bh / 7).max(1);
    c.rect(bx, by, bw, t, ink);
    c.rect(bx, by + bh - t, bw, t, ink);
    c.rect(bx, by, t, bh, ink);
    c.rect(bx + bw - t, by, t, bh, ink);
    let inner = ((bw - 4 * t) as f32 * loaded) as i32;
    if inner > 0 {
        c.rect(bx + 2 * t, by + 2 * t, inner, bh - 4 * t, ink);
    }
}

/// The scene's own time the photos take to slide into their case (`Scene::tuck` without a hold): from the hold's moment
/// (`eject::photo_hold_scene`) to the case's shutting.
pub const TUCK: Duration = Duration::from_millis(260);

/// Where photo `i` of `n` stands at tuck `p` over a case at `(x, y, w, h)`: in the middle of its top, the newer a little
/// higher and to the right (a small pile), each with its lower part in the case until `p` 1 has them wholly in.
pub fn tucked_at((x, y, w, _): (i32, i32, i32, i32), (pw, ph): (i32, i32), i: usize, n: usize, p: f32) -> (i32, i32) {
    let off = i as f32 - (n as f32 - 1.0) / 2.0;
    let px = x + (w - pw) / 2 + (off * pw as f32 / 9.0) as i32;
    let out = ph as f32 * 0.55 * (1.0 - p) * (1.0 - 0.15 * (n - 1 - i) as f32);
    (px, y - out.round() as i32)
}

/// The case going into the console while the game starts.
pub struct Scene {
    pub system: String,
    pub ff: FormFactor,
    pub medium: Medium,
    pub look: Look,
    pub title: String,
    /// What comes out of the case (`media::Game::of`: the system's own cartridge).
    pub kind: Game,
    /// The case's front has the game's box art (not a title card): the label gets it.
    pub has_cover: bool,
    pub front: Rc<Pic>,
    pub from: (i32, i32, i32, i32),
    /// The cartridge or card that comes out of the case, made on the first frame of the animation (after the game's
    /// process is started, so nothing is added before it) from the cover on the case's front; None for a disc.
    pub game: Option<Rc<Pic>>,
    /// Reverse face, read once after launch starts; no image creation in the frame loop.
    pub game_back: Option<Rc<Pic>>,
    /// Cozy or Quick when it started: its length and when the cartridge is in.
    pub cozy: bool,
    /// Playful animations: the disc at its size over the drive, made once (each frame only scales it a little).
    pub at_drive: Option<Rc<Pic>>,
    /// Cozy + Playful, a console with its own mechanism (`insert_rig`): its pictures, decoded on the first frame and gone with the animation.
    pub rig: Option<Rc<Rigged>>,
    /// It has a rig: asked once, on the first frame.
    pub rig_asked: bool,
    /// Where the case goes back to (Cozy): None, it fades out.
    pub home: Option<Home>,
    /// A handheld's start: its power light and its display (lit in this colour, 0xRRGGBB, muted) come on once the game is
    /// in, then the screen goes dark round it and the game's picture comes up out of the dark (`crt_on`), without the tube.
    /// None: the fade only, as for a console on a TV (`power_on_light`).
    pub power_on: Option<u32>,
    /// A plain picture's display lit (made once from the picture on the stage, `plain_display`).
    pub plain_lit: Option<Rc<Pic>>,
    /// The see-through rows under the plain picture's lowest visible one (`clear_rows_below`), found once per scene:
    /// every frame looked for them over the whole picture (19.11.: a launch frame's p95 +25 %, tests/launch_frames.rs).
    pub plain_foot: Option<i32>,
    /// The way back from a game (`eject`): the photos its camera kept this session (`album`), as instant photos at the
    /// case's size, newest last. They stick out of the case's top and slide down into it with the game ([`Scene::tuck`]).
    pub tucked: Vec<Rc<Pic>>,
    /// How far they are in, when the host holds the way back for them (`eject::PhotoHold::tuck`); None: by the scene's own
    /// moment ([`Scene::tuck`]).
    pub tuck_p: Option<f32>,
}

impl Scene {
    /// `clear_rows_below` of the plain picture `sp`, once per scene.
    fn plain_foot(&mut self, sp: &Sprite) -> i32 {
        *self.plain_foot.get_or_insert_with(|| {
            #[cfg(test)]
            FOOT_SCANS.with(|n| n.set(n.get() + 1));
            clear_rows_below(&sp.px, sp.w as usize)
        })
    }

    /// How far the launch fade is, `elapsed` after A, 0-1: it waits for the medium to go in, and a rig's lid or tray shuts
    /// first, so its fade keeps its end and runs after the clunk.
    pub fn fade_at(&self, elapsed: Duration) -> f32 {
        let p = (elapsed.saturating_sub(cabinet::fade_delay(self.cozy) + self.turn_back()).as_secs_f32() / FADE.as_secs_f32()).min(1.0);
        self.rig_fade(p)
    }

    /// How much later this start's lights, fade and game window come: a console that turned round to its back for its game
    /// turns back to its front to light its display (`ir::TURN_BACK`, a power-on with the rig's scene only).
    pub fn turn_back(&self) -> Duration {
        let rig = self.cozy && crate::playful::on();
        if self.power_on.is_some() && rig && ir::turns_back(&self.system, self.kind) { ir::TURN_BACK } else { Duration::ZERO }
    }

    /// When the game's window may come (`hold`), with this scene's `turn_back`.
    pub fn hold(&self) -> Option<Duration> {
        hold(self.cozy, self.power_on.is_some()).map(|h| h + self.turn_back())
    }

    /// The launch animation's length (`cabinet::insert_len`), with this scene's `turn_back`.
    pub fn len(&self) -> Duration {
        cabinet::insert_len(self.cozy) + self.turn_back()
    }

    /// How much of the case still shows, `elapsed` after A: what it casts on the page goes with it.
    pub fn case_shown(&self, elapsed: Duration) -> f32 {
        match self.returning() {
            true => 1.0 - case_back(elapsed, self.opens()).1,
            false => case_shown(cabinet::insert_at(elapsed, self.cozy)),
        }
    }

    /// The rectangle `r` (x, y, w, h) of something that stands under the case at its place on the page (its shadow), `elapsed`
    /// after A: moved and shrunk with the case on its way to its [`Home`], where it goes there.
    pub fn with_case(&self, elapsed: Duration, r: (i32, i32, i32, i32)) -> (i32, i32, i32, i32) {
        let Some(home) = self.home.as_ref().filter(|_| self.returning()) else { return r };
        let back = case_back(elapsed, self.opens()).1;
        let lerp = |a: f32, b: f32| a + (b - a) * back;
        let (w, h) = (lerp(r.2 as f32, home.spine.w as f32), lerp(r.3 as f32, home.spine.h as f32));
        let (cx, cy) = (lerp(r.0 as f32 + r.2 as f32 / 2.0, home.at.0), lerp(r.1 as f32 + r.3 as f32 / 2.0, home.at.1));
        ((cx - w / 2.0) as i32, (cy - h / 2.0) as i32, w as i32, h as i32)
    }

    /// The case's box at `back` of its way home (`case_back`'s second part), as `with_case` moves it.
    fn case_rect_at(&self, back: f32) -> (i32, i32, i32, i32) {
        let r = (self.from.0, self.from.1, self.front.w, self.front.h);
        let Some(home) = self.home.as_ref() else { return r };
        let lerp = |a: f32, b: f32| a + (b - a) * back;
        let (w, h) = (lerp(r.2 as f32, home.spine.w as f32), lerp(r.3 as f32, home.spine.h as f32));
        let (cx, cy) = (lerp(r.0 as f32 + r.2 as f32 / 2.0, home.at.0), lerp(r.1 as f32 + r.3 as f32 / 2.0, home.at.1));
        ((cx - w / 2.0) as i32, (cy - h / 2.0) as i32, w as i32, h as i32)
    }

    /// Whether the case, `elapsed` after A, is painted under the console standing at `console` (its box, risen): from where
    /// its way home crosses behind the stage. It changes sides only where it is clear of the console (changing over it showed
    /// as a jump, ritual 19.11.): the last point of its way up to half way home where it does not overlap it, else half way.
    fn case_under(&self, elapsed: Duration, opens: bool, console: (i32, i32, i32, i32)) -> bool {
        if !self.returning() {
            return false;
        }
        let back = case_back(elapsed, opens).1;
        if back <= 0.0 {
            return false;
        }
        // Where it first comes over the console's box on its way home (by half way at the latest): it goes behind as it
        // comes over it out of the clear and stays behind, never changing sides while it is over it.
        const STEPS: i32 = 128;
        let half = (CASE_IN_ROW * STEPS as f32) as i32;
        let over_at = |b: f32| overlap(self.case_rect_at(b), console);
        let over = |i: i32| over_at(i as f32 / STEPS as f32);
        // The exact point it comes over, not the step after it: a 60 Hz frame between the two showed it over the console in
        // front, and behind it the frame after (NDS at 633 ms).
        let switch = (1..=half).find(|&i| over(i) && !over(i - 1)).map_or(CASE_IN_ROW, |i| {
            let (mut clear, mut on) = ((i - 1) as f32 / STEPS as f32, i as f32 / STEPS as f32);
            for _ in 0..24 {
                let mid = (clear + on) / 2.0;
                if over_at(mid) {
                    on = mid;
                } else {
                    clear = mid;
                }
            }
            on
        });
        back >= switch
    }

    /// The case on its way back to its place in the row, for the rests check (`scene_check::case_over_console`): from half
    /// way it is in the row, behind the stage the console stands on; `under` says whether it was painted under the console.
    fn record_case(&self, elapsed: Duration, opens: bool, under: bool) {
        if !crate::scene_check::recording() || !self.returning() {
            return;
        }
        let back = case_back(elapsed, opens).1;
        if back <= 0.0 {
            return;
        }
        let (x, y, w, h) = self.with_case(elapsed, (self.from.0, self.from.1, self.front.w, self.front.h));
        let what = match (back >= CASE_IN_ROW, under) {
            (true, true) => "the case back in its row, under the console",
            (true, false) => "the case back in its row",
            (false, true) => "the case on its way back to its row, under the console",
            (false, false) => "the case on its way back to its row",
        };
        crate::scene_check::put(what, x, y, w, h, crate::scene_check::Rests::Held("going back into its row"), true);
    }

    /// The case goes back to its [`Home`] instead of fading: Cozy, with a home.
    fn returning(&self) -> bool {
        // A console that turns round for its game (the PSP, `Rigged::front`) and a front-loader's door (the NES) keep
        // their own way with the case.
        self.cozy && self.home.is_some() && self.rig.as_ref().is_none_or(|r| r.front.is_none() && !matches!(r.rig.mech(), ir::Mech::Door(_)))
    }

    /// How much of a case's black tray shows `elapsed` after A, with its cover open `open` (0-1) and the old fade
    /// `fade`: as alpha of 256.
    fn tray_now(&self, elapsed: Duration, open: f32, fade: u32) -> u32 {
        if !self.returning() {
            return if open > 0.0 { fade } else { 0 };
        }
        let (shut, back) = case_back(elapsed, true);
        if open * (1.0 - shut) > 0.0 && back <= 0.0 {
            256
        } else {
            0
        }
    }

    /// Its case opens for the game (a disc's jewel or DVD case, going into a console with a lid or tray).
    fn opens(&self) -> bool {
        self.kind.is_round() && self.rig.as_ref().is_some_and(|r| r.rig.kind() != ir::Kind::Cart)
    }

    /// Draws the case `elapsed` after A: at `from` while its game leaves it (its cover `cover` open, 0-1, when it opens),
    /// then shutting and on its way to its [`Home`]; without a home, `fade` of 256 as before (`tray_now` says where what a
    /// cover uncovers shows). Returns false once there is nothing of it to draw.
    #[allow(clippy::too_many_arguments)]
    fn draw_case(&self, c: &mut Canvas, elapsed: Duration, cover: Option<f32>, fade: u32, dim: f32, clip: (i32, i32), under: bool) -> bool {
        let (front, from) = (&self.front, self.from);
        let (fx, fy, fw, fh) = (from.0 as f32, from.1 as f32, front.w as f32, front.h as f32);
        self.record_case(elapsed, cover.is_some(), under);
        let Some(home) = self.home.as_ref().filter(|_| self.returning()) else {
            if fade == 0 {
                return false;
            }
            match cover {
                Some(open) if open > 0.0 => {
                    art::draw_hinged(c, front, (fx, fy), (fw, fh), open, COVER_OPEN_DEG, fade, clip);
                }
                // Shut, the picture as the shelf draws it, pixel for pixel: nothing changes when the shelf takes over.
                _ => art::blit(c, front, from.0, from.1, fade, clip),
            }
            return true;
        };
        let (shut, back) = case_back(elapsed, cover.is_some());
        if back <= 0.0 {
            match cover {
                Some(open) if open * (1.0 - shut) > 0.0 => {
                    art::draw_hinged(c, front, (fx, fy), (fw, fh), open * (1.0 - shut), COVER_OPEN_DEG, 256, clip);
                }
                _ => art::blit(c, front, from.0, from.1, 256, clip),
            }
            return true;
        }
        // A lying case (Lived-in shelf: its home is the lying picture, `Home::spine` wider than high) lies in its stack on
        // the page under the scene already: its front fades where it is, it does not turn into a spine standing there.
        if home.spine.w > home.spine.h {
            let a = (256.0 * (1.0 - back)).round() as u32;
            if a > 0 {
                art::blit(c, front, from.0, from.1, a, clip);
            }
            return true;
        }
        // Its spine to you and home, into the shelf's dimmed light; it keeps standing there.
        let size = self.medium.size();
        let k0 = fw / size.front.max(1) as f32;
        let lerp = |a: f32, b: f32| a + (b - a) * back;
        let turn = art::Turn {
            cx: lerp(fx + fw / 2.0, home.at.0),
            cy: lerp(fy + fh / 2.0, home.at.1),
            scale: lerp(k0, home.scale),
            turned: 1.0 - back,
            shade: lerp(256.0, 256.0 * (1.0 - dim)).round() as u32,
        };
        art::draw_turning(c, &home.spine, front, size, turn, clip);
        true
    }

    /// The launch fade `p` (0-1, `fade_at`) over what is painted, for Linux and Android: to black, also for a handheld's
    /// power-on (12.11. a light spread over the whole screen was glaring in a dark room: only the console's display is
    /// bright, the rest goes dark as it did before).
    pub fn veil(&self, c: &mut Canvas, p: f32) {
        c.fade((p * 255.0) as u8);
    }

    /// The fade `p` as it shows with this scene's rig.
    pub fn rig_fade(&self, p: f32) -> f32 {
        if self.power_on.is_some() {
            // The fade to black is the shorter `WASH`, softly at both ends; `p` is the fade's 250 ms.
            let (a, b) = (WASH.0.as_secs_f32() / FADE.as_secs_f32(), WASH.1.as_secs_f32() / FADE.as_secs_f32());
            let q = ((p - a) / (b - a)).clamp(0.0, 1.0);
            return q * q * (3.0 - 2.0 * q);
        }
        if self.rig.is_some() {
            ((p - 0.55) / 0.45).clamp(0.0, 1.0)
        } else {
            p
        }
    }

    /// Asks for the console's own mechanism, once: Cozy + Playful, and the SP's bottom slot in every mode.
    pub fn load_rig(&mut self, blocks: i32) {
        self.load_rig_scaled(blocks as f32);
    }

    /// `load_rig` at any screen pixels per pixel of the art: a short screen's console is smaller in the proportion its cases are.
    pub fn load_rig_scaled(&mut self, blocks: f32) {
        if !self.rig_asked {
            if ir::label_away(&self.system) {
                self.game_back = crate::media::game_rear(self.kind, self.look.plastic).map(Rc::new);
            }
            // The SP also needs its bottom-slot geometry in Quick and without playful motion.
            let sp = crate::system_art::picture_id(&self.system) == "gba-sp";
            self.rig = ((self.cozy && crate::playful::on()) || sp).then(|| Rigged::cached_scaled(&self.system, self.kind, blocks)).flatten();
            self.rig_asked = true;
        }
    }

    /// One moment of it, `elapsed` after A, over what is on `c`. `sprite` is the console's picture (`fitted` to the stage)
    /// for the plain animation.
    pub fn paint(&mut self, c: &mut Canvas, elapsed: Duration, sprite: Option<&Sprite>, stage: Stage) {
        self.paint_with_dim(c, elapsed, sprite, stage, true);
    }

    /// As [`Scene::paint`], when the caller already dimmed the kept background page to `cabinet::insert_at(elapsed, cozy).dim`.
    pub fn paint_dimmed(&mut self, c: &mut Canvas, elapsed: Duration, sprite: Option<&Sprite>, stage: Stage) {
        self.paint_with_dim(c, elapsed, sprite, stage, false);
    }

    fn paint_with_dim(&mut self, c: &mut Canvas, elapsed: Duration, sprite: Option<&Sprite>, stage: Stage, apply_dim: bool) {
        if self.rig.is_some() {
            self.paint_rig(c, elapsed, stage, apply_dim);
        } else {
            self.paint_plain(c, elapsed, sprite, stage, apply_dim);
            self.paint_plain_display(c, elapsed, sprite, stage);
        }
        self.paint_tucked(c, elapsed);
    }

    /// How far the photos are in their case, `elapsed` into the scene (the way back runs it backwards): 0 sticking out of
    /// its top, 1 wholly in, by the moment the case shuts (`eject::Way::case_shut_at`). A host that holds the way back
    /// for them says it itself (`tuck_p`).
    pub fn tuck(&self, elapsed: Duration) -> f32 {
        if let Some(p) = self.tuck_p {
            return p.clamp(0.0, 1.0);
        }
        let shut = cabinet::INSERT_COZY.mul_f32(0.06);
        let from = shut + TUCK;
        if elapsed >= from {
            0.0
        } else {
            let k = (from - elapsed).as_secs_f32() / TUCK.as_secs_f32();
            let k = k.min(1.0);
            k * k * (3.0 - 2.0 * k)
        }
    }

    /// The photos over the case: only what is above its top edge is painted, the rest is in the case (behind its front).
    /// They come and go with the case (its fade), and stay in once it is on its way home.
    fn paint_tucked(&self, c: &mut Canvas, elapsed: Duration) {
        if self.tucked.is_empty() || (self.returning() && case_back(elapsed, self.opens()).1 > 0.0) {
            return;
        }
        let p = self.tuck(elapsed);
        let shown = (self.case_shown(elapsed) * 256.0) as u32;
        if p >= 1.0 || shown == 0 {
            return;
        }
        let case = (self.from.0, self.from.1, self.front.w, self.front.h);
        let n = self.tucked.len();
        for (i, pic) in self.tucked.iter().enumerate() {
            let (x, y) = tucked_at(case, (pic.w, pic.h), i, n, p);
            if crate::scene_check::recording() {
                crate::scene_check::put(format!("photo {i} into its case"), x, y, pic.w, pic.h, crate::scene_check::Rests::Held("in its case"), true);
            }
            art::blit(c, pic, x, y, shown.min(256), (0, case.1.max(0)));
        }
    }

    /// A handheld's display lit on its plain picture (`plain_display`), over the console once its game is in: where the
    /// rig lights its own (`power_on`).
    fn paint_plain_display(&mut self, c: &mut Canvas, elapsed: Duration, sprite: Option<&Sprite>, Stage { floor, k }: Stage) {
        if let (Some(sp), true) = (sprite, self.system == "j2me") {
            let foot = self.plain_foot(sp);
            phone_loading(c, elapsed, sp, foot, self.cozy, Stage { floor, k });
        }
        let (Some(_), Some(sp), Some(d)) = (self.power_on, sprite, plain_display(&self.system)) else { return };
        let lit = ir::pose(ir::Kind::Cart, elapsed).lit;
        if lit <= 0.0 {
            return;
        }
        let pic = self
            .plain_lit
            .get_or_insert_with(|| {
                let at = d.at.iter().map(|&(x, y)| (x * sp.w as f32, y * sp.h as f32));
                let mask = ir::display_mask(&sp.px, sp.w as usize, sp.h as usize, at, d.fuzz);
                // The rig's light when the system has one (so the wash and the display are one colour), else the table's.
                let light = ir::light_of(&self.system, self.kind).unwrap_or(d.lit);
                Rc::new(Pic::of(sp.w as i32, sp.h as i32, ir::lit_pixels(&sp.px, &mask, sp.w as usize, light)))
            })
            .clone();
        // It stands where `paint_plain` put it once risen.
        let foot = self.plain_foot(sp);
        lit_over(c, &pic, (c.w - sp.w as i32) / 2, floor - sp.h as i32 + foot, lit);
    }

    /// Every system without a rig, and every system in Quick: the console comes up from below and the medium goes in.
    fn paint_plain(&mut self, c: &mut Canvas, elapsed: Duration, sprite: Option<&Sprite>, Stage { floor, k }: Stage, apply_dim: bool) {
        let px = |n: i32| (n as f32 * k) as i32;
        let (system, ff, medium, from) = (self.system.clone(), self.ff, self.medium, self.from);
        let tint = self.look.color;
        let game = self.kind;
        let game_pic = self.game.clone();
        let st = cabinet::insert_at(elapsed, self.cozy);
        // Playful animations: each medium goes in its own way (`playful::Loader`), timed to its sound.
        let playful = crate::playful::on();
        let loader = crate::playful::Loader::of(&system, game);
        let keys = if playful {
            let t = (elapsed.as_secs_f32() / cabinet::insert_len(self.cozy).as_secs_f32()).min(1.0);
            let since_in = elapsed.as_secs_f32() - cabinet::in_at(self.cozy).as_secs_f32();
            crate::playful::keys(loader, t, since_in, if self.cozy { crate::motion::COZY_SLOWER } else { 1.0 })
        } else {
            crate::playful::Keys::default()
        };
        if apply_dim {
            art::dim(c, 0, c.h, 256 - (st.dim * 256.0) as u32);
        }
        crate::scene_check::new_layer();
        // The console, up from below the screen.
        let (sw, sh) = sprite.as_ref().map_or((px(360), px(120)), |s| (s.w as i32, s.h as i32));
        // It stands on the floor (18.11.: 10 pixels over it, it floated over the shelf's board).
        let base = floor + sprite.map_or(0, |sp| self.plain_foot(sp));
        let sx = (c.w - sw) / 2;
        let sy_final = base - sh;
        // From wholly under the screen's edge, also on a tall phone held upright (its floor is high up the screen).
        let sy = sy_final + ((1.0 - st.rise) * (sh + px(160)).max(c.h - sy_final) as f32) as i32;
        // It stands on something: a soft contact shadow under it, as under the case on the shelf (art::soft_shadow), a
        // console that floated over the floor looked pasted on (polish review 12.11.).
        art::soft_shadow(c, sx + sw / 12, sy + sh - px(5), sw - sw / 6, px(9), px(12), 0.6 * st.rise, (0, c.h));
        if crate::scene_check::recording() {
            // The stage's floor, and the console on it once it is up (on its way up from below the screen it is held).
            crate::scene_check::set_origin(0, 0);
            crate::scene_check::surface("the stage's floor", floor, 0, 0, c.w);
            let handheld = matches!(ff, cabinet::FormFactor::HandheldCart | cabinet::FormFactor::UmdCase | cabinet::FormFactor::CardBox);
            let rests = if handheld {
                crate::scene_check::Rests::Held("a handheld, in the hands")
            } else if st.rise >= 1.0 {
                crate::scene_check::Rests::Settled
            } else {
                crate::scene_check::Rests::Held("coming up from below the screen")
            };
            crate::scene_check::put("the console (plain)", sx, sy, sw, sh, rests, true);
        }
        // From half way home the case is in its row, behind the stage the console stands on: painted before the console,
        // so the console stands in front of it (it went back into the row through the console, ritual 19.11.).
        let case_under = self.case_under(elapsed, self.opens(), (sx, sy_final, sw, sh));
        if case_under {
            self.draw_case(c, elapsed, None, 256, st.dim, (0, c.h), true);
        }
        if let Some(sp) = &sprite {
            c.blit_sprite(sp, sx, sy, ((1.0 - st.rise) * 200.0) as u8);
        } else {
            c.round_rect(sx, sy, sw, sh, px(16), mix(0x2c2c30, 0x000000, ((1.0 - st.rise) * 200.0) as u32));
        }
        // The slot where the picture has it (`slot_of`); else a handheld's at its top edge, a console's on its top, a disc
        // drive's in its middle.
        let at = sprite.as_ref().map_or(SlotAt { x: 0.5, y: None, body: 1.0 }, |_| slot_of(&system));
        let slot_y = sy
            + at.y.map_or_else(
                || match ff {
                    FormFactor::HandheldCart | FormFactor::CardBox => sh / 14,
                    FormFactor::ConsoleCart => sh / 3,
                    _ => sh * 2 / 5,
                },
                |y| (y * sh as f32) as i32,
            );
        let slot_cx = sx + (at.x * sw as f32) as i32;
        // The console's own width: what the medium is sized by, the picture may hold its pad as well.
        let bw = sw as f32 * at.body;
        let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
        let (fx, fy, fw, fh) = (from.0 as f32, from.1 as f32, from.2 as f32, from.3 as f32);
        // The game slides out of its case (from behind it, to its right), the case fades, the game goes to the console
        // and in.
        let case_fade = 256 - ((1.0 - case_shown(st)) * 256.0) as u32;
        let out = (st.carry / 0.35).min(1.0);
        let fly = ((st.carry - 0.35) / 0.65).clamp(0.0, 1.0);
        // A handheld's slot is its top edge, which a tilted picture slopes: the medium sinks behind the console's own
        // outline there (the console is painted over its lower part again, below), not behind a straight line.
        let behind_edge = at.y.is_some_and(|y| y <= 0.12) && matches!(ff, FormFactor::HandheldCart | FormFactor::CardBox) && fly > 0.5;
        let band = sh / 6;
        let clip_bottom = if behind_edge { slot_y + band } else if fly > 0.5 { slot_y } else { c.h };
        // The slot's bar is the console's own surface a good deal darker, not black: on a PlayStation's lid it reads as a
        // seam, on a dark console as the opening.
        let bar = sprite
            .as_ref()
            .and_then(|sp| {
                let (x, y) = (((slot_cx - sx).max(0) as u32).min(sp.w - 1), ((slot_y - sy).max(0) as u32).min(sp.h - 1));
                let p = sp.px[(y * sp.w + x) as usize];
                (p >> 24 > 200).then(|| mix(p & 0xffffff, 0x0c0c0d, 170))
            })
            .unwrap_or(0x0c0c0d);
        use crate::playful::Loader;
        // The crate standing in for a console has no tray or lid: the disc just goes in.
        let own_picture = crate::system_art::get(&system).is_some();
        let draw_game: Box<dyn Fn(&mut Canvas)> = if lights_up_only(&system) {
            // No medium of its own to show: nothing flies, the case fades and the machine lights up.
            Box::new(|_: &mut Canvas| {})
        } else if game.is_round() && playful && own_picture && game_pic.is_some() && matches!(loader, Loader::Tray | Loader::Lid) {
            let r0 = game.size_beside(medium, from.3).1 as f32 / 2.0;
            let r1 = over_drive(game, bw, k);
            // The plastic around the drive, for its tray or lid; the console's top edge, for the lid.
            let body = sprite
                .as_ref()
                .and_then(|sp| {
                    let y = ((slot_y - sy - px(10)).max(0) as u32).min(sp.h.saturating_sub(1));
                    let p = sp.px[(y * sp.w + sp.w / 2) as usize];
                    (p >> 24 > 200).then_some(p & 0xffffff)
                })
                .unwrap_or_else(|| art::plastic(&system));
            // A top-loader's lid where its picture has it, in the colour the picture gives it.
            let (ax, ay, arx, ary) = crate::playful::lid_of(&system);
            let lid = ((sx as f32 + ax * sw as f32) as i32, (sy as f32 + ay * sh as f32) as i32, (arx * sw as f32) as i32, (ary * sh as f32) as i32);
            let lid_top = sprite
                .as_ref()
                .and_then(|sp| {
                    let (x, y) = (((lid.0 - sx).max(0) as u32).min(sp.w - 1), ((lid.1 + lid.3 * 3 / 4 - sy).max(0) as u32).min(sp.h - 1));
                    let p = sp.px[(y * sp.w + x) as usize];
                    (p >> 24 > 200).then_some(p & 0xffffff)
                })
                .unwrap_or(body);
            let start = (fx + fw / 2.0 + out * fw * 0.55, fy + fh / 2.0);
            self.playful_disc(c, (slot_cx, slot_y, body), (lid, lid_top), (r0, r1), (start, fly), (st, keys, loader), k)
        } else if game.is_round() {
            // As large as the real disc beside its case.
            let r0 = game.size_beside(medium, from.3).1 as f32 / 2.0;
            let r1 = over_drive(game, bw, k);
            let start = (fx + fw / 2.0 + out * fw * 0.55, fy + fh / 2.0);
            let end = (slot_cx as f32, slot_y as f32 - r1);
            let r = lerp(r0, r1, fly);
            let (x, y) = (lerp(start.0, end.0, fly), lerp(start.1, end.1, fly) + st.sink * 2.0 * r1);
            if st.rise > 0.4 {
                c.round_rect(slot_cx - r1 as i32 - px(10), slot_y - px(4), 2 * r1 as i32 + px(20), px(9), px(4), bar);
            }
            if let Some(pic) = game_pic {
                // The printed disc, scaled to its size on the way (a UMD keeps its shell's shape).
                let (h, w) = (2.0 * r, 2.0 * r * pic.w as f32 / pic.h.max(1) as f32);
                let shown = if (h as i32, w as i32) == (pic.h, pic.w) { pic } else { Rc::new(pic.scaled_smooth(w as i32, h as i32)) };
                Box::new(move |c: &mut Canvas| art::blit(c, &shown, (x - w / 2.0) as i32, (y - h / 2.0) as i32, 256, (0, clip_bottom)))
            } else {
                Box::new(move |c: &mut Canvas| {
                    if game == Game::Umd {
                        art::umd(c, x as i32, y as i32, r as i32, tint, clip_bottom);
                    } else {
                        art::disc(c, x as i32, y as i32, r as i32, tint, clip_bottom);
                    }
                })
            }
        } else {
            let Some(pic) = game_pic else { return };
            let (gw, gh) = (pic.w as f32, pic.h as f32);
            let (width, back) = ir::face_at(ir::label_away(&system), fly);
            let pic = if back { self.game_back.clone().unwrap_or(pic) } else { pic };
            // The cartridge shrinks onto the slot and sinks in, partly: it stays out of its slot as it does in the real
            // console. A game card goes in all the way.
            let end_w = (bw * if game == Game::Card { 0.2 } else { 0.36 }).clamp(if game == Game::Card { 60.0 * k } else { 90.0 * k }, 230.0 * k);
            let end_h = end_w * gh / gw.max(1.0);
            let depth = match (game, ff) {
                (Game::Card, _) => 1.0,
                (_, FormFactor::ConsoleCart) => 0.55,
                (_, FormFactor::HandheldCart) => 0.65,
                _ => 0.85,
            };
            let start = (fx + fw / 2.0 + out * (fw / 2.0 + gw * 0.45), fy + fh / 2.0);
            let end = (slot_cx as f32, slot_y as f32 - end_h / 2.0);
            let (w, h) = (lerp(gw, end_w, fly) * width, lerp(gh, end_h, fly));
            let (cx, cy) = (lerp(start.0, end.0, fly), lerp(start.1, end.1, fly) + st.sink * end_h * depth);
            if st.rise > 0.4 {
                c.round_rect(slot_cx - end_w as i32 / 2 - px(8), slot_y - px(4), end_w as i32 + px(16), px(9), px(4), bar);
            }
            if playful {
                // Pushed in, it seats: flatter and wider for a moment with the click, then back out a hair (a game card
                // clicks all the way in and springs back so its edge shows for a moment). Scaled as it is drawn.
                let (sq, lift) = (keys.squash, keys.lift * if loader == Loader::Card { 4.0 } else { 1.0 });
                let (w, h0) = (w * (1.0 + 0.08 * sq), h);
                let h = h0 * (1.0 - 0.14 * sq);
                // The part in the slot stays where it is: the top comes down as it squashes.
                let cy = cy + (h0 - h) / 2.0 - lift * end_h;
                Box::new(move |c: &mut Canvas| art::blit_scaled(c, &pic, (cx - w / 2.0) as i32, (cy - h / 2.0) as i32, w as i32, h as i32, 256, (0, clip_bottom)))
            } else {
                let shown = if fly > 0.0 { Rc::new(pic.scaled(w as i32, h as i32)) } else { pic };
                Box::new(move |c: &mut Canvas| art::blit(c, &shown, (cx - w / 2.0) as i32, (cy - h / 2.0) as i32, 256, (0, clip_bottom)))
            }
        };
        // Behind the case while it comes out, in front once it is on its way.
        if fly == 0.0 {
            draw_game(c);
        }
        if !case_under {
            self.draw_case(c, elapsed, None, case_fade, st.dim, (0, c.h), false);
        }
        if fly > 0.0 {
            draw_game(c);
            if let (true, Some(sp)) = (behind_edge, sprite) {
                paint_over(c, sp, (sx, sy), (slot_y - band, slot_y + band));
            }
        }
        // It is in: the slot lights up for a moment, in the label's colour lifted towards white.
        if st.glow > 0.0 {
            let light = mix(tint, crate::palette::scheme().lamp, 170);
            let bw = bw as i32;
            art::glow(c, slot_cx, slot_y, (bw / 3).max(px(90)), (sh / 5).max(px(40)), light, st.glow * 0.8);
            art::glow(c, slot_cx, slot_y, (bw / 8).max(px(36)), (sh / 14).max(px(14)), 0xffffff, st.glow * 0.7);
        }
    }

    /// A game going into its console's real mechanism (`insert_rig`, docs/DESIGN.md): the console comes up, the lid or
    /// tray opens, the game leaves its case and goes in behind the right parts, the lid shuts with the sound's clunk.
    fn paint_rig(&mut self, c: &mut Canvas, elapsed: Duration, Stage { floor, k }: Stage, apply_dim: bool) {
        use ir::Mech;
        let Some(pics) = self.rig.clone() else { return };
        let (front, from, kind, medium) = (self.front.clone(), self.from, self.kind, self.medium);
        let Some(pic) = self.game.clone() else { return };
        let rig = pics.rig;
        let scale = pics.scale;
        let st = cabinet::insert_at(elapsed, self.cozy);
        let mechanism_time = if self.cozy { elapsed } else { elapsed.mul_f32(cabinet::IN_AT_COZY.as_secs_f32() / cabinet::IN_AT.as_secs_f32()) };
        // A power-on of a console that turned round to its back: it turns back to its front and lights there (`ir::pose_of`).
        let back = self.power_on.is_some() && pics.front_lit.is_some();
        let pose = ir::pose_of(rig.kind(), mechanism_time, back);
        if apply_dim {
            art::dim(c, 0, c.h, 256 - (st.dim * 256.0) as u32);
        }
        crate::scene_check::new_layer();
        let (bw, bh) = (pics.base.w, pics.base.h);
        let px = |n: i32| (n as f32 * k) as i32;
        let ox = ((c.w - bw) / 2) as f32;
        // It stands on the floor (18.11.: 10 pixels over it, it floated over the shelf's board).
        // A cartridge going in from below (the Advance SP's) sticks out under it: that one is held in the hands, over the
        // floor, as it always was.
        let top = if rig.bottom_slot { floor - px(10) - bh } else { floor - bh + pics.foot };
        // From wholly under the screen's edge, also on a tall phone held upright (its floor is high up the screen).
        let oy = top as f32 + (1.0 - st.rise) * (bh + px(160)).max(c.h - top) as f32 + pose.dip * scale;
        // The same contact shadow as under a plain console (`paint_plain`).
        // Under its visible foot (not the see-through rows of its art under it, `Rigged::foot`).
        let feet = oy + (bh - if rig.bottom_slot { 0 } else { pics.foot }) as f32;
        art::soft_shadow(c, ox as i32 + bw / 12, feet as i32 - px(5), bw - bw / 6, px(9), px(12), 0.6 * st.rise, (0, c.h));
        let shade = 256 - ((1.0 - st.rise) * 200.0) as u32;
        if crate::scene_check::recording() {
            // The console: its body and the layer over its slot together. A handheld is in the player's hands; a console
            // stands on the floor once it is up, but for the moment it is pressed down as its game seats (`pose.dip`).
            crate::scene_check::set_origin(0, 0);
            crate::scene_check::surface("the stage's floor", floor, 0, 0, c.w);
            let handheld = matches!(self.ff, FormFactor::HandheldCart | FormFactor::UmdCase | FormFactor::CardBox);
            let rests = if handheld {
                crate::scene_check::Rests::Held("a handheld, in the hands")
            } else if st.rise < 1.0 {
                crate::scene_check::Rests::Held("coming up from below the screen")
            } else if pose.dip != 0.0 {
                crate::scene_check::Rests::Held("pressed down as its game seats")
            } else {
                crate::scene_check::Rests::Settled
            };
            let layers: Vec<&Pic> = std::iter::once(&pics.base).chain(pics.over.as_ref()).collect();
            crate::scene_check::put_pics("the console (its real mechanism)", &layers, ox.round() as i32, oy.round() as i32, rests, true);
        }
        let screen = |p: (f32, f32)| (ox + p.0 * scale, oy + p.1 * scale);
        let all = (0, c.h);

        // The medium on its way: face-on at its size beside the case, down to lying in its seat.
        let (fx, fy, fw, fh) = (from.0 as f32, from.1 as f32, from.2 as f32, from.3 as f32);
        let beside = kind.size_beside(medium, from.3);
        let near_h = beside.1 as f32;
        let near_w = if kind.is_round() { near_h * pic.w as f32 / pic.h.max(1) as f32 } else { beside.0 as f32 };
        // A disc lies on the tray of its jewel case, which opens (its cover swings away on the hinge at the left) and lets
        // it be lifted out; a cartridge or card slides out of its case.
        let opens = kind.is_round() && rig.kind() != ir::Kind::Cart;
        let start = if opens { (fx + fw / 2.0, fy + fh / 2.0) } else { (fx + fw / 2.0 + pose.out * (fw / 2.0 + near_w * 0.45), fy + fh / 2.0) };
        let m = rig.medium_at(start, screen, (near_w, near_h), &pose, scale);
        // What is seen of the console (its base and the layer over its slot), where it stands once up: the box the rests
        // check knows it by (`scene_check::put_pics`); the case changes sides of it only where it comes over it.
        let console_box = {
            let (x0, y0, w, h) = *pics.seen.get_or_init(|| seen_box(&pics.base, pics.over.as_ref()));
            (ox.round() as i32 + x0, top + y0, w, h)
        };
        // Its rows from `rows.0` to `rows.1` (parts of its height, top first), at `shade`.
        // Turning round on its way to a rear slot, it is a thin box, not a card: its face is off its middle by half its
        // thickness and its edge shows beside it (`MediumAt::turn`), so it never thins to a line.
        let edge = m.turn.map(|(angle, full)| {
            let half = full.0.hypot(full.1).max(0.5);
            let dir = (full.0 / half, full.1 / half);
            let d = 2.0 * half * ir::thickness(kind);
            let (sin, cos) = angle.sin_cos();
            let face = if m.back { d / 2.0 * sin } else { -d / 2.0 * sin };
            (dir, face, half * cos - d / 2.0 * sin, d * sin)
        });
        let edge_strip = edge.map(|_| {
            let key = Rc::as_ptr(&pic) as usize;
            let mut slot = pics.edge.borrow_mut();
            if let Some((k, p)) = slot.as_ref() {
                if *k == key {
                    return p.clone();
                }
            }
            let p = Rc::new(edge_of(&pic));
            *slot = Some((key, p.clone()));
            p
        });
        let draw_part = |c: &mut Canvas, shade: u32, rows: (f32, f32)| {
            let face_pic = if m.back { self.game_back.as_deref().unwrap_or(&pic) } else { &pic };
            let (sx, sy) = (face_pic.w as f32 / 2.0, face_pic.h as f32 / 2.0);
            let at = edge.map_or(m.at, |(dir, off, _, _)| (m.at.0 + dir.0 * off, m.at.1 + dir.1 * off));
            let o = (at.0 - m.u.0 - m.v.0, at.1 - m.u.1 - m.v.1);
            let h = face_pic.h as f32;
            art::blit_affine_rows(c, face_pic, o, (m.u.0 / sx, m.u.1 / sx), (m.v.0 / sy, m.v.1 / sy), shade, all, (rows.0 * h, rows.1 * h));
            if let (Some((dir, _, x0, w)), Some(strip)) = (edge, edge_strip.as_deref()) {
                if w >= 0.5 {
                    let o = (m.at.0 + dir.0 * x0 - m.v.0, m.at.1 + dir.1 * x0 - m.v.1);
                    let sy = strip.h as f32 / 2.0;
                    art::blit_affine_rows(c, strip, o, (dir.0 * w, dir.1 * w), (m.v.0 / sy, m.v.1 / sy), shade, all, (rows.0 * strip.h as f32, rows.1 * strip.h as f32));
                }
            }
        };
        let draw_medium = |c: &mut Canvas| draw_part(c, 256, (0.0, 1.0));

        // Behind the case while it comes out, in front once it is on its way.
        let case_fade = 256 - ((1.0 - case_shown(st)) * 256.0) as u32;
        let flying = pose.fly > 0.0;
        if let (Some(face_pic), true) = (pics.front.as_ref(), pose.face < 1.0) {
            // A console that takes its game at its back (the PSP) comes up showing its front and turns round: the front
            // narrows to its edge and the back widens from it, darker edge-on, as the game box turns on the card.
            let k = (pose.face * std::f32::consts::PI).cos();
            let turn_shade = shade * (150 + (106.0 * k.abs()) as u32) / 256;
            let (cx, bottom) = (ox + bw as f32 / 2.0, oy + bh as f32);
            if k > 0.0 {
                let (fw, fh) = (face_pic.w as f32 * scale, face_pic.h as f32 * scale);
                art::blit_affine(c, face_pic, (cx - fw * k / 2.0, bottom - fh), (scale * k, 0.0), (0.0, scale), turn_shade, all);
                // Turned back to its front with its game in: its display lights (`back`).
                if let (Some(lit), true) = (&pics.front_lit, back && pose.lit > 0.0) {
                    lit_affine(c, lit, (cx - fw * k / 2.0, bottom - fh), (scale * k, 0.0), (0.0, scale), pose.lit);
                }
            } else {
                let k = -k;
                art::blit_affine(c, &pics.base, (cx + (ox - cx) * k, oy), (k, 0.0), (0.0, 1.0), turn_shade, all);
                if let Mech::Lid(l) = rig.mech() {
                    let m = l.map(pose.open);
                    if let Some(p) = if m.under { pics.under.as_ref() } else { pics.top.as_ref() } {
                        let at = (cx + (ox + m.o.0 * scale - cx) * k, oy + m.o.1 * scale);
                        art::blit_affine(c, p, at, (m.u.0 * scale * k, m.u.1 * scale), (m.v.0 * scale * k, m.v.1 * scale), turn_shade, all);
                    }
                }
            }
            let opens = kind.is_round();
            if opens && pose.out > 0.0 && case_fade > 0 {
                fill_alpha(c, from.0, from.1, front.w, front.h, CASE_TRAY, case_fade);
            }
            if !flying {
                draw_medium(c);
            }
            if case_fade > 0 {
                if opens {
                    let cos = (pose.out * std::f32::consts::FRAC_PI_2).cos();
                    if cos * front.w as f32 >= 1.0 {
                        let shade = (case_fade as f32 * (0.55 + 0.45 * cos)) as u32;
                        art::blit_affine(c, &front, (fx, fy), (cos, 0.0), (0.0, 1.0), shade, all);
                    }
                } else {
                    art::blit(c, &front, from.0, from.1, case_fade, all);
                }
            }
            // Turning back to its front (`back`), its game is inside behind the shut door.
            if flying && !pose.shut_in {
                draw_medium(c);
            }
            return;
        }
        // From half way home the case is in its row, behind the stage the console stands on: under the console (as in
        // `paint_plain`).
        // Decided by where the console stands once up: while it rises into the case's way the case does not change sides.
        let case_under = self.case_under(elapsed, opens, console_box);
        if case_under {
            self.draw_case(c, elapsed, opens.then_some(pose.out), case_fade, st.dim, all, true);
        }
        // On its way from the case to over its slot (`fly` < 1) a medium is in front of the whole console, as a
        // front-loader's is: it goes behind the console's parts only from over its slot (19.11.: on the Mini's screen,
        // where the console is big beside the case, the SNES cartridge passed through the console, behind its top and
        // the layer over its slot). A cover it slides in behind (the PSP's, `Lid::medium_behind`) stays over it.
        let way_in_front = flying && pose.fly < 1.0 && !matches!(rig.mech(), Mech::Lid(l) if l.medium_behind);
        // A slot at the console's back (`rig.behind`): over its slot the medium is already a little behind the console's
        // top, so on the last stretch (`REAR_SINK_FROM`) its copy in front fades into the one behind: no jump.
        let sink = if rig.behind && way_in_front {
            let t = ((pose.fly - REAR_SINK_FROM) / (1.0 - REAR_SINK_FROM)).clamp(0.0, 1.0);
            t * t * (3.0 - 2.0 * t)
        } else {
            0.0
        };
        if rig.behind && flying && (!way_in_front || sink > 0.0) {
            draw_medium(c);
        }
        // A lid hinged behind the console's rear wall (`Lid::tuck`) stands up out of it: while it is open it is drawn first,
        // the console over it, so what of the lid has gone down behind the wall is hidden and it is plainly fixed there.
        let lid_behind = matches!(rig.mech(), Mech::Lid(l) if l.tuck > 0.0) && pose.open > LID_BEHIND_OPEN;
        if let (Mech::Lid(l), true) = (rig.mech(), lid_behind) {
            draw_lid(c, l, &pics, (ox, oy), pose.open, shade);
        }
        art::blit(c, &pics.base, ox.round() as i32, oy.round() as i32, shade, all);
        if let Mech::Door(door) = rig.mech() {
            // A front-loader (the NES): the door lifts, the cartridge slides in lying flat through the opening, is pressed
            // down and the door drops shut. What of it is inside is in the console's shadow and shows only through the
            // opening (`over` is the console around it); what is still outside is in front of everything.
            if !flying {
                draw_medium(c);
            }
            if case_fade > 0 {
                art::blit(c, &front, from.0, from.1, case_fade, all);
            }
            // Only once it is at the opening (`fly` 1): on its way there it is wholly outside and in front of the console, not
            // dark and behind its lid, where its path crosses the console's top (ritual review 14.11.: the NES cartridge
            // dived behind the console and came out in front again).
            let on_its_way = flying && pose.fly < 1.0;
            let inside = flying && !on_its_way && !pose.shut_in;
            if inside {
                draw_part(c, INSIDE_SHADE, (0.0, 1.0));
            }
            if let Some(over) = &pics.over {
                art::blit(c, over, ox.round() as i32, oy.round() as i32, shade, all);
            }
            draw_lid_as(c, door, &pics, (ox, oy), pose.open, shade, false);
            if inside {
                draw_part(c, 256, (0.0, rig.outside(&m, &pose, screen)));
            }
            if on_its_way {
                draw_medium(c);
            }
            if let (Some(led), true) = (rig.led, pose.led > 0.0) {
                let at = screen(led.at);
                art::glow(c, at.0 as i32, at.1 as i32, px(16), px(16), led.color, 0.9 * pose.led);
                art::glow(c, at.0 as i32, at.1 as i32, px(5), px(5), 0xffffff, 0.8 * pose.led);
            }
            return;
        }
        // The lid stands behind the disc coming down, and shuts over it; the tray is under it always.
        // A cover the medium slides in behind (the PSP's) is over it all the time.
        let lid_later = pose.landed >= 1.0 || matches!(rig.mech(), Mech::Lid(l) if l.medium_behind);
        match rig.mech() {
            Mech::Lid(l) if !lid_later && !lid_behind => draw_lid(c, l, &pics, (ox, oy), pose.open, shade),
            Mech::Tray(t) => {
                if let Some(tray) = &pics.top {
                    art::blit_affine(c, tray, screen(rig.tray_at(t, pose.open)), (scale, 0.0), (0.0, scale), shade, all);
                }
            }
            _ => {}
        }
        let tray = if opens { self.tray_now(elapsed, pose.out, case_fade) } else { 0 };
        if tray > 0 {
            // What the cover uncovers: the case's black tray.
            fill_alpha(c, from.0, from.1, front.w, front.h, CASE_TRAY, tray);
        }
        if !flying {
            draw_medium(c);
        }
        // A disc's cover swings open towards you about the hinge on its left edge, in perspective (its free edge comes
        // nearer and grows), darker as it turns from the light; never squeezed flat to a hairline. Then the case shuts
        // and goes home (`Home`).
        if !case_under {
            self.draw_case(c, elapsed, opens.then_some(pose.out), case_fade, st.dim, all, false);
        }
        // A disc coming down is in front of everything; once it is down the parts in front of it hide it.
        let coming_down = rig.kind() != ir::Kind::Cart && pose.landed < 1.0;
        if flying && !rig.behind && !way_in_front {
            // A shadow on the console under it: darker as it gets close.
            if coming_down && pose.fly >= 1.0 {
                let at = screen(rig.seat.at);
                let (rx, ry) = (rig.seat.u.0.hypot(rig.seat.u.1) * scale, rig.seat.v.0.hypot(rig.seat.v.1) * scale);
                art::glow(c, at.0 as i32, at.1 as i32, (rx * 1.08) as i32, (ry * 1.08) as i32, 0x000000, 0.45 * pose.drop);
            }
            if !coming_down && !pose.shut_in {
                draw_medium(c);
            }
        }
        if let Some(over) = &pics.over {
            art::blit(c, over, ox.round() as i32, oy.round() as i32, shade, all);
        }
        // The SP's foreground includes its controls as well as the slot rim. Its
        // closed screen must cover those controls, then reveal them as it opens.
        if let Some(lid) = rig.clamshell {
            let opening = if self.cozy && crate::playful::on() { ((mechanism_time.as_secs_f32() * 1000.0 - 160.0) / 500.0).clamp(0.0, 1.0) } else { 1.0 };
            draw_lid(c, lid, &pics, (ox, oy), crate::cabinet::ease_out(opening), shade);
            // Its display is in the lid: lit where the lid stands, through the lid's own map.
            if let (Some(lit), true, true) = (&pics.lid_lit, self.power_on.is_some(), pose.lit > 0.0) {
                lid_lit_over(c, lid, lit, &pics, (ox, oy), crate::cabinet::ease_out(opening), pose.lit);
            }
        }
        if flying && !rig.behind && coming_down && !way_in_front {
            draw_medium(c);
        }
        if let Mech::Lid(l) = rig.mech() {
            if lid_later && !lid_behind {
                draw_lid(c, l, &pics, (ox, oy), pose.open, shade);
            }
        }
        // On its way: in front of the console (`way_in_front`); a rear slot's fading into the one behind (`sink`).
        if way_in_front && sink <= 0.0 {
            draw_medium(c);
        } else if way_in_front && sink < 1.0 {
            let keep = (sink * 255.0) as u32;
            if let Some((sx0, sy0, sx1, sy1)) = rear_sink_bounds(&m, edge, c) {
                let stride = c.stride();
                let rw = sx1 - sx0;
                let mut under = Vec::with_capacity((sy1 - sy0) * rw);
                {
                    let px = c.pixels();
                    for row in sy0..sy1 {
                        under.extend_from_slice(&px[row * stride + sx0..row * stride + sx1]);
                    }
                }
                draw_medium(c);
                let px = c.pixels_mut();
                for (ry, row) in (sy0..sy1).enumerate() {
                    let dst = &mut px[row * stride + sx0..row * stride + sx1];
                    let src = &under[ry * rw..(ry + 1) * rw];
                    for (p, &u) in dst.iter_mut().zip(src) {
                        if *p != u {
                            *p = mix(*p, u, keep);
                        }
                    }
                }
            } else {
                draw_medium(c);
            }
        }
        // The console takes it: its power light comes on, a moment later its display (a handheld's start, `power_on`).
        if let (Some(lit), true, true) = (&pics.lit, self.power_on.is_some(), pose.lit > 0.0) {
            lit_over(c, lit, ox.round() as i32, oy.round() as i32, pose.lit);
        }
        if let (Some(led), true) = (rig.led, pose.led > 0.0) {
            let at = screen(led.at);
            art::glow(c, at.0 as i32, at.1 as i32, px(16), px(16), led.color, 0.9 * pose.led);
            art::glow(c, at.0 as i32, at.1 as i32, px(5), px(5), 0xffffff, 0.8 * pose.led);
        }
    }

    /// Playful animations, a disc going into a console with a tray or a lid (`playful::Loader`): the tray slides out
    /// under the drive's slot, the disc comes over it, drops in lying down and rides in as the tray shuts; or the lid
    /// on top flips up, the disc drops in behind the console's top edge and the lid shuts. Draws the tray or lid now
    /// (behind the disc) and gives back what draws the disc. `slot` is (x, y) of the drive's slot, the console's top
    /// edge and its plastic's colour; the disc is made at its size over the drive once (`at_drive`) and
    /// only scaled a little on the way.
    #[allow(clippy::type_complexity, clippy::too_many_arguments)]
    fn playful_disc(&mut self, c: &mut Canvas, (slot_x, slot_y, body): (i32, i32, u32), (lid, lid_top): ((i32, i32, i32, i32), u32), (r0, r1): (f32, f32), (start, fly): ((f32, f32), f32), (st, keys, loader): (cabinet::Insert, crate::playful::Keys, crate::playful::Loader), k: f32) -> Box<dyn Fn(&mut Canvas)> {
        let px = |n: i32| (n as f32 * k) as i32;
        let Some(pic) = self.game.clone() else { return Box::new(|_: &mut Canvas| {}) };
        let aspect = pic.w as f32 / pic.h.max(1) as f32;
        let big = match &self.at_drive {
            Some(p) => p.clone(),
            None => {
                let h = (2.0 * r1).round() as i32;
                let w = (h as f32 * aspect).round() as i32;
                let p = Rc::new(if (w, h) == (pic.w, pic.h) { (*pic).clone() } else { pic.scaled_smooth(w, h) });
                self.at_drive = Some(p.clone());
                p
            }
        };
        let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
        let r = lerp(r0, r1, fly);
        // Where it goes: over the tray's well or over the lid's opening, then down.
        let src = if fly >= 1.0 { big } else { pic };
        if loader == crate::playful::Loader::Tray {
            let tw = (r1 * 2.5) as i32;
            if st.rise > 0.4 {
                c.round_rect(slot_x - tw / 2 - px(6), slot_y - px(4), tw + px(12), px(9), px(4), 0x0c0c0d);
            }
            let (wy, depth) = art::tray(c, slot_x, slot_y, tw, keys.open, body);
            let well = slot_y as f32 + tw as f32 * 0.42 / 2.0;
            let over = (slot_x as f32, well - r1 * 1.15);
            let landed = keys.drop >= 1.0;
            if landed && depth == 0 {
                return Box::new(|_: &mut Canvas| {});
            }
            let x = lerp(start.0, over.0, fly);
            let y = if landed { wy as f32 } else { lerp(lerp(start.1, over.1, fly), well, keys.drop) };
            // Lying in the tray it is seen at a slant: 0.4 of its height.
            let (w, h) = (2.0 * r * aspect, 2.0 * r * lerp(1.0, 0.4, keys.drop));
            let clip = if landed { (slot_y, c.h) } else { (0, c.h) };
            return Box::new(move |c: &mut Canvas| art::blit_scaled(c, &src, (x - w / 2.0).round() as i32, (y - h / 2.0).round() as i32, w.round() as i32, h.round() as i32, 256, clip));
        }
        // A lid: open behind, the disc over the opening, down into it lying as the opening is drawn, the lid over it.
        let (lx, ly, lrx, lry) = lid;
        art::lid_behind(c, lid, keys.open, lid_top);
        if keys.drop >= 1.0 && keys.open <= 0.0 {
            return Box::new(|_: &mut Canvas| {});
        }
        let over = (lx as f32, ly as f32 - lry as f32 - r1 * 1.1);
        let x = lerp(lerp(start.0, over.0, fly), lx as f32, keys.drop);
        let y = lerp(lerp(start.1, over.1, fly), ly as f32, keys.drop);
        let w = lerp(2.0 * r * aspect, 2.0 * lrx as f32 * 0.92, keys.drop);
        let h = lerp(2.0 * r, 2.0 * lry as f32 * 0.92, keys.drop);
        let open = keys.open;
        Box::new(move |c: &mut Canvas| {
            art::blit_scaled(c, &src, (x - w / 2.0).round() as i32, (y - h / 2.0).round() as i32, w.round() as i32, h.round() as i32, 256, (0, c.h));
            art::lid_over(c, lid, open, lid_top);
        })
    }
}

fn draw_lid(c: &mut Canvas, l: ir::Lid, pics: &Rigged, o: (f32, f32), open: f32, shade: u32) {
    draw_lid_as(c, l, pics, o, open, shade, true);
}

/// Tight screen-pixel bounding box `(sx0, sy0, sx1, sy1)` covering `draw_medium` (both face and edge strip) so
/// `REAR_SINK_FROM` only saves and blends the medium's pixels instead of cloning the full framebuffer.
fn rear_sink_bounds(m: &ir::MediumAt, edge: Option<((f32, f32), f32, f32, f32)>, c: &Canvas) -> Option<(usize, usize, usize, usize)> {
    let stride = c.stride();
    let rows = c.pixels().len() / stride.max(1);
    if stride == 0 || rows == 0 {
        return None;
    }
    let at = edge.map_or(m.at, |(dir, off, _, _)| (m.at.0 + dir.0 * off, m.at.1 + dir.1 * off));
    let dx = m.u.0.abs() + m.v.0.abs();
    let dy = m.u.1.abs() + m.v.1.abs();
    let mut min_x = at.0 - dx;
    let mut max_x = at.0 + dx;
    let mut min_y = at.1 - dy;
    let mut max_y = at.1 + dy;
    if let Some((dir, _, x0, w)) = edge {
        if w >= 0.5 {
            let o = (m.at.0 + dir.0 * x0 - m.v.0, m.at.1 + dir.1 * x0 - m.v.1);
            let ex = dir.0 * w;
            let ey = dir.1 * w;
            let vx = 2.0 * m.v.0;
            let vy = 2.0 * m.v.1;
            for (px, py) in [(o.0, o.1), (o.0 + ex, o.1 + ey), (o.0 + vx, o.1 + vy), (o.0 + ex + vx, o.1 + ey + vy)] {
                min_x = min_x.min(px);
                max_x = max_x.max(px);
                min_y = min_y.min(py);
                max_y = max_y.max(py);
            }
        }
    }
    let pad = 4.0;
    let dx0 = (min_x - pad).floor() as i32;
    let dx1 = (max_x + pad).ceil() as i32;
    let dy0 = (min_y - pad).floor() as i32;
    let dy1 = (max_y + pad).ceil() as i32;
    let sx0 = (c.screen_x(dx0) - 1).clamp(0, stride as i32) as usize;
    let sx1 = (c.screen_x(dx1) + 2).clamp(0, stride as i32) as usize;
    let sy0 = (c.screen_y(dy0) - 1).clamp(0, rows as i32) as usize;
    let sy1 = (c.screen_y(dy1) + 2).clamp(0, rows as i32) as usize;
    (sx0 < sx1 && sy0 < sy1).then_some((sx0, sy0, sx1, sy1))
}

/// A clamshell's lit display (`Rigged::lid_lit`) over its lid drawn open `open` (`draw_lid`), `amount` (0-1) of the way to its
/// light; nothing while the lid shows its top side (the glass is inside).
fn lid_lit_over(c: &mut Canvas, l: ir::Lid, lit: &Pic, pics: &Rigged, o: (f32, f32), open: f32, amount: f32) {
    let m = l.map(open);
    if !m.under {
        return;
    }
    let s = pics.scale;
    lit_affine(c, lit, (o.0 + m.o.0 * s, o.1 + m.o.1 * s), (m.u.0 * s, m.u.1 * s), (m.v.0 * s, m.v.1 * s), amount);
}

/// Lit pixels at the art's size (`Rigged::lid_lit`, `front_lit`) laid through a map (`blit_affine`), `amount` (0-1) of the way.
fn lit_affine(c: &mut Canvas, lit: &Pic, o: (f32, f32), u: (f32, f32), v: (f32, f32), amount: f32) {
    let a = (amount.clamp(0.0, 1.0) * 255.0).round() as u32;
    if a == 0 {
        return;
    }
    let faded = Pic::of(lit.w, lit.h, lit.px.iter().map(|&p| if p >> 24 == 255 { a << 24 | (p & 0xff_ffff) } else { 0 }).collect());
    art::blit_affine(c, &faded, o, u, v, 256, (0, c.h));
}

/// [`draw_lid`]; `slab`: with its thickness (a front-loader's door keeps its own look).
fn draw_lid_as(c: &mut Canvas, l: ir::Lid, pics: &Rigged, o: (f32, f32), open: f32, shade: u32, slab: bool) {
    let m = l.map(open);
    let Some(p) = (if m.under { pics.under.as_ref() } else { pics.top.as_ref() }) else { return };
    let s = pics.scale;
    let at = (o.0 + m.o.0 * s, o.1 + m.o.1 * s);
    let (u, v) = ((m.u.0 * s, m.u.1 * s), (m.v.0 * s, m.v.1 * s));
    // Turned away from the light above it, a little darker.
    let shade = shade * (256 - (m.turned * 50.0) as u32) / 256;
    // A slab, not a sheet: its other face first, in shadow, a lid's thickness behind it along its normal; what of it
    // shows round the face is its edge, in its own plastic. Open it never thins to a line.
    let off = l.thickness(open);
    if slab && off.0.abs() + off.1.abs() > 0.25 {
        art::blit_affine(c, p, (at.0 + off.0 * s, at.1 + off.1 * s), u, v, shade * 140 / 256, (0, c.h));
    }
    art::blit_affine(c, p, at, u, v, shade, (0, c.h));
}

/// The handhelds' screen the start is made for (1240 x 1080): there the console has [`ir::SCALE`] screen pixels
/// per pixel of its art and stands on a floor this far over the screen's bottom (`app_cabinet::scene_bottom`).
pub const HANDHELD_SCREEN: (i32, i32) = (1240, 1080);
pub const HANDHELD_FLOOR_GAP: i32 = 88;

/// How a phone places a start: the console as large against the screen as on the handhelds (`HANDHELD_SCREEN`), in whole
/// screen pixels per pixel of the art, never below the handhelds' 2; and never so large that it, its medium hanging over
/// it included, leaves `w` x `h` (at most 4: a console, not a poster). The Odin 2 Mini's 1920 x 1080 gets the
/// handhelds' own 2 (11.11. it got 3, the console half as large again as on Linux and cut off at the bottom).
pub fn phone_blocks(rig: Option<&ir::Rig>, base: (i32, i32), (w, h): (i32, i32)) -> i32 {
    let (half, tall) = phone_extent(rig, base);
    let across = ((w - 32) as f32 / (2.0 * half)) as i32;
    let down = ((h - 32) as f32 / tall) as i32;
    let like = (ir::SCALE * (w as f32 / HANDHELD_SCREEN.0 as f32).min(h as f32 / HANDHELD_SCREEN.1 as f32)).round() as i32;
    like.min(across).min(down).clamp(2, 4)
}

/// Half the width and the height a start needs, in art pixels: the console, and the medium over its seat's hover.
fn phone_extent(rig: Option<&ir::Rig>, (bw, bh): (i32, i32)) -> (f32, f32) {
    let (bw, bh) = (bw as f32, bh as f32);
    let Some(rig) = rig else { return (bw / 2.0, bh) };
    let s = rig.seat;
    let (ru, rv) = (s.u.0.hypot(s.u.1), s.v.0.hypot(s.v.1));
    (((s.hover.0 - bw / 2.0).abs() + ru).max(bw / 2.0), bh - (s.hover.1 - rv).min(0.0))
}

/// The floor for a rig `blocks` screen pixels per art pixel, as on the handhelds: `HANDHELD_FLOOR_GAP` over the
/// screen's bottom (at the screen's scale `k`), under the page's case, so its medium comes down into the console from
/// above and never up out of it (11.11. the console stood in the screen's middle, over the case). Never so low that the
/// medium over the console leaves the screen's top.
pub fn phone_floor(rig: Option<&ir::Rig>, base: (i32, i32), blocks: i32, h: i32, k: f32) -> i32 {
    let (_, tall) = phone_extent(rig, base);
    let gap = (HANDHELD_FLOOR_GAP as f32 * k) as i32;
    // The console stands on the floor (`paint_rig`); the medium's top at least 16 down.
    let lowest_top = 16 + (tall * blocks as f32) as i32;
    // Never on the screen's very edge: a little room under it, as when it stood 10 pixels over the floor.
    (h - gap).max(lowest_top).min(h - (10.0 * k) as i32)
}

/// A phone's start for a rig (None: the plain animation's console, `base` its room): screen pixels per art pixel, and the
/// stage (`phone_blocks`, `phone_floor`).
pub fn phone_stage(rig: Option<&ir::Rig>, base: (i32, i32), (w, h): (i32, i32)) -> (i32, Stage) {
    let blocks = phone_blocks(rig, base, (w, h));
    let k = blocks as f32 / ir::SCALE;
    (blocks, Stage { floor: phone_floor(rig, base, blocks, h, k), k })
}

/// Where a rig `blocks` screen pixels per art pixel on `stage` holds its medium before it goes in (`Seat::hover`), on a
/// screen `w` wide: as `Scene::paint_rig` places it, at rest.
pub fn hover_at(rig: &ir::Rig, base: (i32, i32), blocks: i32, stage: Stage, w: i32) -> (f32, f32) {
    let (bw, bh) = (base.0 * blocks, base.1 * blocks);
    let top = if rig.bottom_slot { stage.floor - (10.0 * stage.k) as i32 - bh } else { stage.floor - bh };
    (((w - bw) / 2) as f32 + rig.seat.hover.0 * blocks as f32, top as f32 + rig.seat.hover.1 * blocks as f32)
}

#[cfg(test)]
thread_local! {
    /// How often a scene looked for its plain picture's foot (`Scene::plain_foot`).
    static FOOT_SCANS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
mod tests {
    /// Physics, nothing floats: each handheld's power light in the Cabinet sits on its body, the pixels round it solid, and
    /// clear of its display (`PlainDisplay::led`).
    #[test]
    fn each_handhelds_power_light_sits_on_its_body_not_beside_it_or_on_its_glass() {
        let mut lit = 0;
        for (id, d) in DISPLAYS {
            let Some((fx, fy)) = d.led else { continue };
            lit += 1;
            let s = crate::system_art::get(id).unwrap_or_else(|| panic!("{id}: its picture"));
            let (x, y) = ((fx * s.w as f32) as i32, (fy * s.h as f32) as i32);
            for (dx, dy) in [(0, 0), (-1, 0), (1, 0), (0, -1), (0, 1)] {
                let (px, py) = (x + dx, y + dy);
                assert!(px >= 0 && py >= 0 && px < s.w as i32 && py < s.h as i32, "{id}: inside the picture");
                assert_eq!(s.px[(py as u32 * s.w + px as u32) as usize] >> 24, 255, "{id}: solid at {px},{py}, not in the air");
            }
            let glass = crate::insert_rig::display_mask(&s.px, s.w as usize, s.h as usize, d.at.iter().map(|&(a, b)| (a * s.w as f32, b * s.h as f32)), d.fuzz);
            assert!(!glass[(y as u32 * s.w + x as u32) as usize], "{id}: the light is not on the display");
        }
        assert!(lit >= 6, "the Game Boy, Color, GBA, SP, Game Gear and PSP have their light");
    }

    use super::*;

    /// A plain console's foot (the see-through rows under it) is looked for once per launch, not in every frame: from
    /// bd1269e1a (18.11.) every frame scanned the whole picture for it, and a launch frame cost about 1.25 times as much
    /// (tests/launch_frames.rs went red in loaded gates, 19.11.). The handheld and the console, Cozy and Quick, with the
    /// handheld's lit display (`paint_plain_display`) and the phone's loading bar, which ask for it too.
    #[test]
    fn a_plain_console_looks_for_its_foot_once_per_launch() {
        for (system, cozy) in [("gbc", true), ("gbc", false), ("megadrive", true), ("j2me", true)] {
            let mut s = scene(system, cozy);
            s.power_on = Some(0x80ff80);
            let before = FOOT_SCANS.with(|n| n.get());
            let stage = Stage { floor: 1400, k: 1.0 };
            for ms in (0..2400).step_by(48) {
                frame(&mut s, ms, stage);
            }
            let scans = FOOT_SCANS.with(|n| n.get()) - before;
            assert!(scans <= 1, "{system} (cozy {cozy}): the foot was looked for {scans} times in 50 frames");
        }
    }

    /// 19.11. (Alex, pocket-ui mini's demo): the SNES cartridge went through the console on its way to its slot. On a
    /// small screen (the Miyoo's 750 x 560, a phone held across) the console stands big beside the case, and the medium
    /// was painted under the layer over a top slot (SNES, Mega Drive, Master System, PC Engine) or under the whole console
    /// (a slot at the back: Game Boy, GBA, Game Gear) for its whole way. Missed before: on the big screen the path never
    /// crosses the console. Now: the medium painted one colour, each frame painted with the console and without its
    /// pictures (body and the layer over its slot); on its way nothing of it is hidden, only a game for a slot at the back
    /// fades behind the console's top on the last stretch (`REAR_SINK_FROM`), smoothly (frames 10 ms apart).
    #[test]
    fn a_game_on_its_way_to_its_slot_never_goes_through_the_console() {
        crate::cozy_extras::set(crate::cozy_extras::Extras::default());
        crate::motion::set_cozy(true);
        const MARK: u32 = 0xffff_00ff;
        let (w, h) = (750usize, 560usize);
        // How much of the medium's colour is seen, in pixels: a pixel of it mixed with the console's counts by its share.
        let seen = |b: &[u32]| {
            b.iter()
                .map(|&p| {
                    let (r, g, b) = (((p >> 16) & 255) as i32, ((p >> 8) & 255) as i32, (p & 255) as i32);
                    if (r - b).abs() < 24 { (r.min(b) - g).max(0) } else { 0 }
                })
                .sum::<i32>() as usize
                / 255
        };
        let mut wrong = Vec::new();
        for system in ["snes", "megadrive", "mastersystem", "pcengine", "gb", "gbc", "gba", "gamegear", "nes", "psx"] {
            let mut s = scene(system, true);
            s.from = (40, 110, 160, 230);
            s.front = Rc::new(Pic::new(160, 230, 0xff40_6080));
            s.game = Some(Rc::new(Pic::new(90, 100, MARK)));
            let kind = s.kind;
            let Some(f) = ir::files(system, kind) else { continue };
            let base = {
                let dim = |at: usize| i32::from_be_bytes(f.base[at..at + 4].try_into().unwrap());
                (dim(16), dim(20))
            };
            let (whole, stage) = phone_stage(Some(&f.rig), base, (w as i32, h as i32));
            drop(f);
            s.load_rig(whole);
            let Some(rig) = s.rig.clone() else { continue };
            let clear = |p: &Pic| Pic::new(p.w, p.h, 0);
            let bare = Rc::new(Rigged {
                rig: rig.rig,
                front: rig.front.clone(),
                base: clear(&rig.base),
                over: rig.over.as_ref().map(clear),
                top: rig.top.clone(),
                under: rig.under.clone(),
                scale: rig.scale,
                lit: rig.lit.clone(),
                lid_lit: rig.lid_lit.clone(),
                front_lit: rig.front_lit.clone(),
                edge: Default::default(),
                foot: rig.foot,
                seen: Default::default(),
            });
            let paint = |s: &mut Scene, ms: u64| {
                let mut buf = vec![0u32; w * h];
                s.paint(&mut Canvas::new(&mut buf, w as u32, h as u32), Duration::from_millis(ms), None, stage);
                seen(&buf)
            };
            let (mut before, mut on_way, mut over_slot) = (0usize, 0, 0);
            for ms in (0..4000u64).step_by(10) {
                let pose = ir::pose_of(rig.rig.kind(), Duration::from_millis(ms), false);
                if pose.fly <= 0.0 {
                    continue;
                }
                if pose.fly >= 1.0 {
                    over_slot += 1;
                    if over_slot > 3 {
                        break;
                    }
                } else {
                    on_way += 1;
                }
                s.rig = Some(rig.clone());
                let with = paint(&mut s, ms);
                s.rig = Some(bare.clone());
                let without = paint(&mut s, ms);
                let hidden = without.saturating_sub(with);
                if pose.fly < REAR_SINK_FROM && hidden > 40 {
                    wrong.push(format!("{system} {ms} ms (fly {:.2}): {hidden} px of the game behind the console on its way", pose.fly));
                }
                if hidden > before + 600 {
                    wrong.push(format!("{system} {ms} ms (fly {:.2}): {} px more of the game behind the console at once", pose.fly, hidden - before));
                }
                before = hidden;
            }
            assert!(on_way > 5, "{system}: frames on its way");
        }
        assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    }

    fn scene(system: &str, cozy: bool) -> Scene {
        let hardware = if system == "gba-sp" { "gba" } else { system };
        let kind = Game::of(hardware, "x.rom");
        let medium = Medium::of(hardware);
        let look = Look { color: 0x8040c0, band: 0x8040c0, plastic: art::plastic(system), tape: false };
        let front = Rc::new(Pic::new(120, 180, 0xff40_6080));
        let game = Rc::new(Pic::new(90, 90, 0xffc0_c0c0));
        Scene { system: system.into(), ff: FormFactor::of(hardware), medium, look, title: "T".into(), kind, has_cover: false, front, from: (40, 40, 120, 180), game: Some(game), game_back: None, cozy, at_drive: None, rig: None, rig_asked: false, home: None, power_on: None, plain_lit: None, plain_foot: None, tucked: Vec::new(), tuck_p: None }
    }

    /// 0.6 Brick (1024x768, blocks = 2.0 * 1.7 / 2.5 = 1.36): repeated launches of the same system must reuse the
    /// decoded and scaled rig instead of decoding PNGs and running scaled_smooth again on Frame 0.
    #[test]
    fn brick_rig_is_cached_across_launches() {
        crate::cozy_extras::set(crate::cozy_extras::Extras::default());
        crate::motion::set_cozy(true);
        let blocks = ir::SCALE * (crate::media::SHORT_SHELF_SCALE / crate::media::SHELF_SCALE);
        for system in ["snes", "psx", "gba"] {
            let (mut a, mut b) = (scene(system, true), scene(system, true));
            a.load_rig_scaled(blocks);
            b.load_rig_scaled(blocks);
            let ra = a.rig.as_ref().unwrap_or_else(|| panic!("{system}: rig missing"));
            let rb = b.rig.as_ref().unwrap_or_else(|| panic!("{system}: rig missing"));
            assert!(Rc::ptr_eq(ra, rb), "{system}: rig at blocks={blocks} was decoded and scaled anew on launch 2");
        }
    }

    fn frame(s: &mut Scene, ms: u64, stage: Stage) -> Vec<u32> {
        let mut buf = vec![0x202020u32; 720 * 1600];
        let sprite = crate::system_art::get(&s.system).map(|p| p.fitted((560.0 * stage.k) as i32, (330.0 * stage.k) as i32));
        s.paint(&mut Canvas::new(&mut buf, 720, 1600), Duration::from_millis(ms), sprite.as_ref(), stage);
        buf
    }

    fn lit(px: &[u32]) -> usize {
        px.iter().filter(|p| (*p & 0xff) > 0x30 || (*p >> 8 & 0xff) > 0x30 || (*p >> 16 & 0xff) > 0x30).count()
    }

    const STAGE: Stage = Stage { floor: 1000, k: 1.0 };

    /// Nothing floats (Variant A, 19.11.: the session's photos go into the case on the way back): at every moment of the
    /// tuck, for one to three photos, each photo's lower part is in the case (its bottom below the case's top edge, its
    /// top above it while it sticks out), within the case's width; at 1 every one is wholly in.
    #[test]
    fn the_photos_slide_into_their_case_and_never_hang_over_it() {
        let case = (300, 400, 120, 180);
        let (pw, ph) = (100, 118);
        for n in 1..=3 {
            for i in 0..n {
                let mut last = i32::MIN;
                for k in 0..=40 {
                    let p = k as f32 / 40.0;
                    let (x, y) = tucked_at(case, (pw, ph), i, n, p);
                    assert!(y + ph > case.1 + ph / 3, "{n}/{i} at {p}: not held in the case (bottom {} over the top {})", y + ph, case.1);
                    assert!(y + ph <= case.1 + case.3, "{n}/{i} at {p}: lower than the case's bottom");
                    assert!(x >= case.0 - pw / 6 && x + pw <= case.0 + case.2 + pw / 6, "{n}/{i} at {p}: beside the case");
                    assert!(y >= last, "{n}/{i}: it only ever goes down into the case");
                    last = y;
                }
                assert!(tucked_at(case, (pw, ph), i, n, 1.0).1 >= case.1, "{n}/{i}: wholly in at the end");
            }
        }
    }

    /// The way back paints the session's photos over the case's top while they stick out, and nothing of them once they are
    /// in (by the case's shutting); without photos nothing changes.
    #[test]
    fn the_way_back_shows_the_photos_going_into_the_case() {
        let mut s = scene("gba", true);
        let photo = Rc::new(Pic::new(100, 118, 0xfff4_f0e8));
        let at = crate::eject::photo_hold_scene().as_millis() as u64;
        let whites = |px: &[u32]| px.iter().filter(|p| (**p & 0xff_ffff) == 0xf4f0e8).count();
        assert_eq!(whites(&frame(&mut s, at, STAGE)), 0, "no photos: nothing of them");
        s.tucked = vec![photo.clone(), photo];
        s.tuck_p = Some(0.0);
        let out = frame(&mut s, at, STAGE);
        assert!(whites(&out) > 500, "sticking out of the case: {}", whites(&out));
        // Only above the case's top: what is below it is in the case.
        let below: usize = (s.from.1 as usize..1600).map(|y| (0..720).filter(|&x| (out[y * 720 + x] & 0xff_ffff) == 0xf4f0e8).count()).sum();
        assert_eq!(below, 0, "a photo shows below the case's top edge");
        s.tuck_p = Some(0.5);
        let half = frame(&mut s, at, STAGE);
        assert!(whites(&half) < whites(&out) && whites(&half) > 0, "it slides down");
        s.tuck_p = Some(1.0);
        assert_eq!(whites(&frame(&mut s, at, STAGE)), 0, "wholly in");
        // Without a hold, by the scene's moment: in by the case's shutting.
        s.tuck_p = None;
        let shut = cabinet::INSERT_COZY.mul_f32(0.06);
        assert!(s.tuck(shut) >= 1.0 && s.tuck(shut + TUCK + Duration::from_millis(1)) == 0.0);
    }

    #[test]
    fn a_system_without_a_rig_shows_the_plain_animation_never_a_gap() {
        for system in ["psp", "3ds", "nes", "pcengine", "nosuchsystem"] {
            let mut s = scene(system, true);
            s.load_rig(2);
            assert!(s.rig.is_none(), "{system}");
            let (a, b) = (frame(&mut s, 700, STAGE), frame(&mut s, 1100, STAGE));
            assert!(lit(&a) > 3000, "{system}: a console stands there");
            assert!(a != b, "{system}: the game moves");
        }
    }

    #[test]
    fn a_rig_with_cozy_and_never_without() {
        crate::cozy_extras::set(crate::cozy_extras::Extras::default());
        crate::motion::set_cozy(true);
        for system in ["psx", "ps2", "gamecube", "snes", "gba", "n64", "megadrive", "dreamcast", "saturn", "xbox", "gb", "nds", "wii"] {
            let mut s = scene(system, true);
            s.load_rig(2);
            assert!(s.rig.is_some(), "{system}");
            assert!(lit(&frame(&mut s, 750, STAGE)) > 3000, "{system}: a console stands there");
            let mut quick = scene(system, false);
            quick.load_rig(2);
            assert!(quick.rig.is_none(), "{system}: Quick never loads a rig");
        }
    }

    #[test]
    fn rear_slot_handhelds_show_the_back_in_rendered_frames_while_top_loaders_keep_the_label() {
        crate::cozy_extras::set(crate::cozy_extras::Extras::default());
        crate::motion::set_cozy(true);
        // Contrasting faces identify what actually gets painted, independent of a game's cover.
        // A regression that keeps drawing the front despite the plan's `back` flag fails here.
        for (system, back) in [("gba", true), ("gba-sp", true), ("gb", true), ("gbc", true), ("nds", true), ("snes", false), ("n64", false), ("psx", false)] {
            for cozy in [true, false] {
                let mut s = scene(system, cozy);
                s.load_rig(2);
                s.game = Some(Rc::new(Pic::new(90, 90, 0xffff_0000)));
                s.game_back = Some(Rc::new(Pic::new(90, 90, 0xff00_00ff)));
                let ms = if cozy { 900 } else { 150 };
                let painted = frame(&mut s, ms, STAGE);
                let count = |color| painted.iter().filter(|p| **p == color).count();
                if back {
                    assert!(count(0x0000ff) > 100 && count(0xff0000) == 0, "{system} cozy={cozy}: reverse face before the slot");
                } else {
                    assert!(count(0xff0000) > 100 && count(0x0000ff) == 0, "{system} cozy={cozy}: printed face stays visible");
                }
            }
        }
    }

    #[test]
    fn the_sp_closed_lid_covers_the_controls_and_opening_reveals_them() {
        crate::cozy_extras::set(crate::cozy_extras::Extras::default());
        crate::motion::set_cozy(true);
        let mut s = scene("gba-sp", true);
        s.load_rig(2);
        let r = Rc::make_mut(s.rig.as_mut().expect("SP hardware"));
        let over = r.over.as_mut().expect("SP controls and slot rim");
        // Mark the D-pad on the actual foreground picture. It must be hidden by the
        // closed outer lid, then visible once the photographed screen swings up.
        for y in 150 * 2..158 * 2 {
            for x in 86 * 2..94 * 2 {
                over.px[(y * over.w + x) as usize] = 0xffff00ff;
            }
        }
        let marked = |pixels: &[u32]| pixels.iter().filter(|p| {
            let (red, green, blue) = (*p >> 16 & 255, *p >> 8 & 255, *p & 255);
            red > 64 && blue > 64 && green < 8
        }).count();
        let shut = frame(&mut s, 160, STAGE);
        let open = frame(&mut s, 900, STAGE);
        assert_eq!(marked(&shut), 0, "closed SP lid must cover the controls instead of being painted behind them");
        assert!(marked(&open) > 100, "opening the SP must reveal its controls");
    }

    #[test]
    fn the_sp_slot_is_empty_before_the_cartridge_arrives_and_filled_only_after_insertion() {
        crate::cozy_extras::set(crate::cozy_extras::Extras::default());
        crate::motion::set_cozy(true);
        let mut s = scene("gba-sp", true);
        s.load_rig(2);
        let r = s.rig.as_ref().expect("SP hardware");
        let (a, b) = r.rig.seat.slot.expect("bottom slot");
        let (ox, oy) = ((720 - r.base.w) / 2, STAGE.floor - 10 - r.base.h);
        // Sample the interior along the photographed slot mouth, away from its blue rim.
        let points: Vec<_> = (30..=70).step_by(5).map(|percent| {
            let t = percent as f32 / 100.0;
            let x = ox + ((a.0 + (b.0 - a.0) * t) * r.scale).round() as i32;
            let y = oy + ((a.1 + (b.1 - a.1) * t) * r.scale).round() as i32;
            (y * 720 + x) as usize
        }).collect();
        let before = frame(&mut s, 650, STAGE);
        let after = frame(&mut s, 1200, STAGE);
        let brightness = |p: u32| [p & 255, (p >> 8) & 255, (p >> 16) & 255].into_iter().max().unwrap();
        let empty = points.iter().filter(|i| brightness(before[**i]) <= 38).count();
        assert!(empty * 5 >= points.len() * 4, "SP slot already contains a flat cartridge before insertion: {empty}/{} interior samples are a dark empty opening", points.len());
        let filled = points.iter().filter(|i| brightness(after[**i]) > brightness(before[**i]) + 15).count();
        assert!(filled * 2 >= points.len(), "SP cartridge must visibly fill the previously empty slot after it seats: {filled}/{} samples changed", points.len());
    }

    #[test]
    fn motion_off_or_reduced_is_the_quick_way_with_no_hold_and_one_quiet_click() {
        use crate::motion::Motion::{Full, Off, Reduced};
        for motion in [Off, Reduced] {
            let cozy = cabinet::starts_cozy(true, motion);
            assert!(!cozy, "{motion:?}");
            assert_eq!(hold(cozy, false), None);
            let mut s = scene("psx", cozy);
            s.load_rig(2);
            assert!(s.rig.is_none());
            assert_eq!(cues(cozy, Medium::of("psx"), Game::of("psx", "game.rom"), FormFactor::of("psx"), None, true).iter().map(|c| c.sound).collect::<Vec<_>>(), [Sound::Insert]);
        }
        assert!(cabinet::starts_cozy(true, Full));
    }

    #[test]
    fn the_window_is_held_until_the_fade_has_run_and_the_fade_ends_black() {
        assert_eq!(hold(true, false), Some(Duration::from_millis(1600)));
        for system in ["psx", "psp"] {
            let mut s = scene(system, true);
            s.load_rig(2);
            assert_eq!(s.fade_at(cabinet::IN_AT_COZY), 0.0, "{system}");
            assert_eq!(s.fade_at(hold(true, false).unwrap()), 1.0, "{system}");
        }
    }

    #[test]
    fn a_skip_brings_the_game_at_once_and_only_once() {
        let at = |ms| Duration::from_millis(ms);
        let mut cozy = HandOver::new(true, false);
        assert!(!cozy.take_release(at(0)) && !cozy.take_release(at(1599)), "held while the animation plays");
        assert!(cozy.skip() && cozy.skipped());
        assert!(cozy.take_release(at(300)), "a skip ends the hold");
        assert!(!cozy.take_release(at(301)) && !cozy.skip(), "once");
        let mut played = HandOver::new(true, false);
        assert!(played.take_release(at(1600)) && played.released());
        // Quick, Motion below Full, a frozen game: nothing to wait for.
        assert!(HandOver::new(false, false).take_release(at(0)));
    }

    #[test]
    fn each_sound_hits_on_its_moment_of_the_picture() {
        let hit = |c: &Cue| c.after + ui_sound::hit_at(c.sound) + SOUND_LEAD;
        let disc = cues(true, Medium::of("psx"), Game::of("psx", "game.rom"), FormFactor::of("psx"), Some(ir::Kind::Lid), true);
        assert_eq!(disc.iter().map(|c| c.sound).collect::<Vec<_>>(), [Sound::JewelOpen, Sound::DiscDrop, Sound::DiscShut]);
        assert_eq!(hit(&disc[1]), cabinet::IN_AT_COZY);
        assert_eq!(hit(&disc[2]), cabinet::IN_AT_COZY + ir::SHUT_AFTER);
        let cart = cues(true, Medium::of("snes"), Game::of("snes", "game.rom"), FormFactor::of("snes"), None, true);
        assert_eq!(cart[0].sound, ui_sound::opening(Medium::of("snes")));
        assert_eq!((cart[1].sound, hit(&cart[1])), (Sound::CartIn, cabinet::IN_AT_COZY));
        // The plain animation's disc keeps the whole sound, on the same moment.
        let plain = cues(true, Medium::of("psp"), Game::of("psp", "game.rom"), FormFactor::of("psp"), None, true);
        assert_eq!(hit(&plain[1]), cabinet::IN_AT_COZY);
        let quick = cues(false, Medium::of("psx"), Game::of("psx", "game.rom"), FormFactor::of("psx"), None, true);
        assert_eq!((hit(&quick[0]), quick[0].db), (cabinet::IN_AT, ui_sound::QUICK_DB));
    }

    #[test]
    fn a_handhelds_switch_clicks_as_its_power_light_comes_on() {
        let hit = |c: &Cue| c.after + ui_sound::hit_at(c.sound) + SOUND_LEAD;
        for system in ["gb", "gba", "gamegear", "nds", "ngpc", "psp"] {
            let kind = Game::of(system, "x.rom");
            let cue = switch_on_cue(system, kind);
            assert_eq!(cue.sound, Sound::SwitchOn);
            assert!(ui_sound::is_case_sound(cue.sound), "Case sounds off silences it");
            let back = system == "psp";
            let rig = ir::files(system, kind).unwrap().rig.kind();
            // The power light starts coming up at the switch's click, not before, and it is still dark a moment before.
            let at = hit(&cue);
            assert!(ir::pose_of(rig, at, back).led < 0.02 && ir::pose_of(rig, at + Duration::from_millis(40), back).led > 0.0, "{system}: {at:?}");
        }
    }

    #[test]
    fn a_phone_keeps_every_rig_and_its_medium_on_screen_whatever_way_it_is_held() {
        // With the scenes of 11.11. (NES, GBC, PSP, Master System, Game Gear, PC Engine, 3DS), which Android shows too.
        for system in ["psx", "ps2", "gamecube", "snes", "snes-us", "gba", "gba-sp", "gb", "n64", "megadrive", "dreamcast", "saturn", "xbox", "nds", "wii", "nes", "gbc", "psp", "mastersystem", "gamegear", "pcengine", "3ds"] {
            let game = if system == "snes-us" { Game::SnesUs } else { Game::of(system, "x.rom") };
            let system = if system == "snes-us" { "snes" } else { system };
            let f = ir::files(system, game).expect(system);
            let b = crate::system_art::decode(&f.base).unwrap();
            let base = (b.w as i32, b.h as i32);
            // The X300's screen, held both ways.
            for (w, h) in [(1216, 2500), (2500, 1120)] {
                let n = phone_blocks(Some(&f.rig), base, (w, h));
                let k = n as f32 / ir::SCALE;
                assert!(n * base.0 <= w, "{system} {n}: wider than {w}");
                let floor = phone_floor(Some(&f.rig), base, n, h, k);
                let top = floor - base.1 * n;
                let bottom = top + base.1 * n;
                assert!(top >= 0 && bottom <= h, "{system} {n} on {w}x{h}: {top}..{bottom}");
                let s = f.rig.seat;
                let hover_top = top as f32 + (s.hover.1 - s.v.0.hypot(s.v.1)) * n as f32;
                assert!(hover_top >= 0.0, "{system} {n} on {w}x{h}: the medium hangs off the top ({hover_top})");
                if f.rig.bottom_slot {
                    let hover_bottom = top as f32 + (s.hover.1 + s.u.1.abs() + s.v.1.abs()) * n as f32;
                    assert!(hover_bottom <= h as f32, "{system}: bottom-entry cartridge must stay visible ({hover_bottom} > {h})");
                }
                let x0 = (w - base.0 * n) / 2;
                let reach = s.u.0.hypot(s.u.1) * n as f32;
                let (left, right) = (x0 as f32 + s.hover.0 * n as f32 - reach, x0 as f32 + s.hover.0 * n as f32 + reach);
                assert!(left >= 0.0 && right <= w as f32, "{system} {n} on {w}x{h}: the medium hangs off the side {left}..{right}");
            }
        }
    }

    #[test]
    fn the_plain_animation_aims_at_the_console_not_at_the_picture_s_middle() {
        // The console stands at the right of its pad on these pictures: its slot is too.
        for system in ["nes", "snes", "n64", "megadrive", "pcengine", "psx", "gamecube"] {
            let at = slot_of(system);
            let sprite = crate::system_art::get(system).expect(system);
            // Something of the console is under the slot, and the column above the slot is what shows the medium.
            let (x, y) = ((at.x * sprite.w as f32) as u32, (at.y.unwrap() * sprite.h as f32) as u32);
            assert!(sprite.px[(y * sprite.w + x) as usize] >> 24 == 255, "{system}: nothing there at {x},{y}");
            assert!(at.x > 0.55, "{system}: {at:?}");
        }
        // An alias finds its hardware; a system without a picture of its console keeps the middle.
        assert_eq!(slot_of("sfc"), slot_of("snes"));
        assert_eq!(slot_of("nosuchsystem"), slot_of(STAND_IN));
        assert!(crate::system_art::get(STAND_IN).is_some());
    }

    #[test]
    fn every_slot_is_on_its_console_and_the_computers_have_one() {
        // Every picture with a listed slot has something of its console under the slot point; the Amiga CD32 finds its
        // hardware by alias.
        for (system, ..) in SLOTS {
            let at = slot_of(system);
            let sprite = crate::system_art::get(system).unwrap_or_else(|| panic!("{system}: no picture"));
            let (x, y) = ((at.x * sprite.w as f32) as u32, (at.y.unwrap() * sprite.h as f32) as u32);
            assert!(sprite.px[(y.min(sprite.h - 1) * sprite.w + x.min(sprite.w - 1)) as usize] >> 24 == 255, "{system}: nothing there at {x},{y}");
        }
        for system in ["c64", "atari800", "atari5200", "amiga", "zxspectrum"] {
            assert!(SLOTS.iter().any(|s| s.0 == system), "{system}: no slot");
        }
        for (alias, own) in [("amigacd32", "amiga"), ("amiga500", "amiga"), ("amiga1200", "amiga")] {
            assert_eq!(slot_of(alias), slot_of(own), "{alias}");
        }
    }

    #[test]
    fn a_medium_sinks_behind_the_outline_of_a_tilted_console_not_a_straight_line() {
        // A console whose top edge slopes (left column starts at row 1, right at row 2): painting it over the rows a medium
        // was drawn in hides the medium below each column's edge, and leaves it where the picture is clear.
        let sp = Sprite { w: 2, h: 5, px: vec![0, 0, 0xff112233, 0, 0xff112233, 0xff112233, 0xff112233, 0xff112233, 0xff112233, 0xff112233] };
        let mut buf = vec![0x00ff00u32; 10];
        let mut c = Canvas::new(&mut buf, 2, 5);
        paint_over(&mut c, &sp, (0, 0), (0, 5));
        let col = |x: usize| (0..5).map(|y| buf[y * 2 + x] == 0x112233).collect::<Vec<_>>();
        assert_eq!(col(0), vec![false, true, true, true, true]);
        assert_eq!(col(1), vec![false, false, true, true, true]);
    }

    #[test]
    fn the_arcade_sets_show_the_cabinet_and_light_its_screen() {
        // Every arcade set has the cabinet's picture (assets/systems/arcade.png), not the crate, and its game has no medium
        // to show: the cabinet lights up where its screen is.
        for system in ["arcade", "mame", "fbneo", "cps2", "naomi"] {
            assert!(crate::system_art::get(system).is_some(), "{system}: the cabinet, not the plain card");
            assert_ne!(slot_of(system), slot_of(STAND_IN), "{system}: not the crate");
            assert!(lights_up_only(system), "{system}");
            let s = slot_of(system);
            assert!((0.3..0.6).contains(&s.x) && s.y.is_some_and(|y| (0.25..0.5).contains(&y)), "{system}: on the screen, {s:?}");
        }
    }

    #[test]
    fn computers_and_arcade_sets_show_no_false_disc() {
        for system in ["c64", "amiga", "zxspectrum", "atari800", "msx", "mame", "fbneo", "cps2", "naomi", "arcade"] {
            assert!(lights_up_only(system), "{system}");
        }
        // A UMD goes in the PSP's back door, label out: a disc dropped on its screen is wrong. Cozy shows its rig (the PSP from
        // behind, `insert_rig`); the plain animation (Quick) shows no medium.
        assert!(lights_up_only("psp"));
        // Real cartridges and discs keep their medium.
        for system in ["atari5200", "atari7800", "psx", "nes", "amigacd32", "neogeo", "dreamcast"] {
            assert!(!lights_up_only(system), "{system}");
        }
    }

    /// What of a frame differs from its plain ground `bg`: (left, top, right, bottom), or None.
    fn bbox(px: &[u32], w: i32, bg: u32) -> Option<(i32, i32, i32, i32)> {
        let mut b: Option<(i32, i32, i32, i32)> = None;
        for (i, p) in px.iter().enumerate() {
            if (p & 0xff_ffff) != (bg & 0xff_ffff) {
                let (x, y) = (i as i32 % w, i as i32 / w);
                b = Some(b.map_or((x, y, x, y), |b| (b.0.min(x), b.1.min(y), b.2.max(x), b.3.max(y))));
            }
        }
        b
    }

    /// 12.11. (Alex: "SNES Mist", "GBA shit", "Cart kommt von unten rein"): on the Odin 2 Mini the console of a start
    /// was half as large again as on the handhelds, stood in the screen's middle over the shelf's case and was cut off at
    /// the bottom; its medium came up out of it. A phone's start is now the handhelds' own (Linux, `app_cabinet`: 2 screen
    /// pixels per art pixel, `HANDHELD_FLOOR_GAP` over the bottom) at the screen's size: on a screen as high as the
    /// handhelds' the same console at the same height, frame for frame; every console of every system with a scene.
    /// Missed before: the phone test only asked that the rig stays on the screen, never how large or where.
    #[test]
    fn a_phone_start_is_the_handhelds_start_at_the_screen_s_size() {
        crate::cozy_extras::set(crate::cozy_extras::Extras::default());
        crate::motion::set_cozy(true);
        const BG: u32 = 0xff12_3456;
        let paint = |system: &str, (w, h): (i32, i32), linux: bool| {
            let mut s = scene(if system == "snes-us" { "snes" } else { system }, true);
            if system == "snes-us" {
                s.kind = Game::SnesUs;
            }
            s.from = (40, 40, 120, 180);
            let f = ir::files(&s.system, s.kind).expect(system);
            let b = crate::system_art::decode(&f.base).unwrap();
            let base = (b.w as i32, b.h as i32);
            let (blocks, stage) = if linux {
                (ir::SCALE as i32, Stage { floor: h - HANDHELD_FLOOR_GAP, k: 1.0 })
            } else {
                let n = phone_blocks(Some(&f.rig), base, (w, h));
                let k = n as f32 / ir::SCALE;
                (n, Stage { floor: phone_floor(Some(&f.rig), base, n, h, k), k })
            };
            s.load_rig(blocks);
            let mut buf = vec![BG; (w * h) as usize];
            // The medium is in and the case is gone: the console alone, as the fade begins.
            s.paint(&mut Canvas::new(&mut buf, w as u32, h as u32), Duration::from_millis(1500), None, stage);
            let ground = buf[0];
            (bbox(&buf, w, ground).expect(system), blocks)
        };
        for system in ["snes", "snes-us", "n64", "megadrive", "nes", "mastersystem", "pcengine", "gb", "gbc", "gba", "gba-sp", "gamegear", "nds", "3ds", "psp", "psx", "saturn", "dreamcast", "ps2", "gamecube", "wii", "xbox"] {
            let (linux, _) = paint(system, HANDHELD_SCREEN, true);
            let lh = linux.3 - linux.1;
            // The Odin 2 Mini, and the Odin with its bars shown; a phone held across.
            for (w, h) in [(1920, 1080), (1782, 951), (2340, 1080)] {
                let (a, n) = paint(system, (w, h), false);
                assert_eq!(n, 2, "{system} on {w}x{h}: the handhelds' 2 screen pixels per art pixel");
                assert_eq!(a.3 - a.1, lh, "{system} on {w}x{h}: as high as on the handhelds");
                assert_eq!(h - a.3, HANDHELD_SCREEN.1 - linux.3, "{system} on {w}x{h}: on the handhelds' floor");
                assert!(((a.0 + a.2 - w) - (linux.0 + linux.2 - HANDHELD_SCREEN.0)).abs() <= 2, "{system} on {w}x{h}: where it stands on the handhelds, from the middle");
            }
            // Larger screens scale it whole, and it stays low on the screen, whole, under the page's case.
            for (w, h) in [(2560, 1600), (1080, 2340), (720, 1600), (1216, 2500)] {
                let (a, n) = paint(system, (w, h), false);
                assert!(a.0 >= 0 && a.1 >= 0 && a.2 < w && a.3 < h - 8, "{system} {n} on {w}x{h}: whole on the screen {a:?}");
                assert!(a.3 >= h * 3 / 4, "{system} {n} on {w}x{h}: low on the screen, {a:?}");
                assert!(((a.3 - a.1) as f32) / (h as f32) <= (lh as f32 / HANDHELD_SCREEN.1 as f32) * 1.1 || h > w, "{system} {n} on {w}x{h}: no larger against the screen than on the handhelds");
            }
        }
    }

    /// Ritual 19.11.: a case going home and a game going into a slot at the back changed sides of the console (in front,
    /// behind) while they overlapped it, which shows as a jump through it. Every console with its own mechanism, every 60 Hz
    /// frame of the start: the case's and the game's side may change only where they are clear of the console.
    #[test]
    fn the_case_and_the_game_change_sides_of_the_console_only_clear_of_it() {
        crate::cozy_extras::set(crate::cozy_extras::Extras::default());
        let rigs: Vec<&str> = ir::rig_ids().collect();
        let next = std::sync::atomic::AtomicUsize::new(0);
        let wrong = std::sync::Mutex::new(Vec::new());
        let workers = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1).clamp(1, 4).min(rigs.len().max(1));
        std::thread::scope(|scope| {
            for _ in 0..workers {
                let (rigs, next, wrong) = (&rigs, &next, &wrong);
                scope.spawn(move || {
                    crate::motion::set_cozy(true);
                    let mut local = Vec::new();
                    loop {
                        let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let Some(&system) = rigs.get(i) else { break };
                        let mut s = scene(system, true);
                        // Its place in the row on the shelf, low behind where the console stands (as on a handheld's screen).
                        s.home = Some(Home { spine: Rc::new(Pic::new(20, 180, 0xffe0_2010)), at: (360.0, 930.0), scale: 1.0 });
                        s.load_rig(2);
                        if s.rig.is_none() {
                            continue;
                        }
                        crate::scene_check::record(true);
                        let mut last: Option<(Option<(bool, bool)>, Option<(bool, bool)>)> = None;
                        let mut hidden = false;
                        let mut ms = 0.0f32;
                        while ms < cabinet::INSERT_COZY.as_secs_f32() * 1000.0 {
                            crate::scene_check::take();
                            let _ = frame(&mut s, ms as u64, STAGE);
                            let (objs, _) = crate::scene_check::take();
                            let console = objs.iter().find(|o| o.what.starts_with("the console ("));
                            let side = |prefix: &str| objs.iter().find(|o| o.what.starts_with(prefix)).map(|o| (o.what.contains("under the console"), o));
                            let (case, game) = (side("the case "), None::<(bool, &crate::scene_check::Obj)>);
                            // A change of sides shows as a jump when it is over the console before and after it (coming over it out of
                            // the clear and going behind there is not one).
                            if let (Some(con), Some((before_case, before_game))) = (console, last) {
                                for (now, was, what) in [(case, before_case, "the case"), (game, before_game, "the game")] {
                                    if let (Some((under, o)), Some((was_under, was_over))) = (now, was) {
                                        let over = o.x < con.x + con.w && con.x < o.x + o.w && o.y < con.y + con.h && con.y < o.y + o.h;
                                        if under != was_under && over && was_over {
                                            local.push(format!("{system} {ms:.0} ms: {what} changes sides over the console ({} at {},{} {}x{}, the console at {},{} {}x{})", o.what, o.x, o.y, o.w, o.h, con.x, con.y, con.w, con.h));
                                        }
                                    }
                                }
                            }
                            // Behind the console and over it, then clear of it again before it is in: it flew through it.
                            if let (Some(con), Some((under, o))) = (console, game) {
                                let over = o.x < con.x + con.w && con.x < o.x + o.w && o.y < con.y + con.h && con.y < o.y + o.h;
                                if under && over {
                                    hidden = true;
                                } else if hidden && !over {
                                    local.push(format!("{system} {ms:.0} ms: the game went behind the console and comes out clear of it on its way"));
                                    hidden = false;
                                }
                            }
                            let over_con = |o: &crate::scene_check::Obj| {
                                console.is_some_and(|con| o.x < con.x + con.w && con.x < o.x + o.w && o.y < con.y + con.h && con.y < o.y + o.h)
                            };
                            last = Some((case.map(|c| (c.0, over_con(c.1))), game.map(|g| (g.0, over_con(g.1)))));
                            ms += 1000.0 / 60.0;
                        }
                        crate::scene_check::record(false);
                    }
                    wrong.lock().unwrap().extend(local);
                });
            }
        });
        let wrong = wrong.into_inner().unwrap();
        assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    }

    #[test]
    fn a_bigger_phone_only_scales_the_handhelds_picture() {
        // At the handhelds' 2 the blocks are the art's old doubling.
        let p = Pic::of(2, 1, vec![1, 2]);
        assert_eq!(enlarged(p, 2).px, vec![1, 1, 2, 2, 1, 1, 2, 2]);
    }

    /// Pixels where blue is the strongest by far (the test case's front, lit or shaded) and where red is (its spine).
    fn front_and_spine(px: &[u32]) -> (usize, usize, (usize, usize)) {
        let (mut f, mut sp, mut at) = (0, 0, (0, 0));
        for (i, p) in px.iter().enumerate() {
            let (r, g, b) = (p >> 16 & 0xff, p >> 8 & 0xff, p & 0xff);
            if b > 24 && b > 2 * r && b > 2 * g {
                f += 1;
            } else if r > 24 && r > 2 * g && r > 2 * b {
                sp += 1;
                at = (at.0 + i % 720, at.1 + i / 720);
            }
        }
        (f, sp, (at.0 / sp.max(1), at.1 / sp.max(1)))
    }

    /// 11.11.: once its game was out, a case simply faded away (and faded in on the way back), the only thing in the
    /// launch that was neither moved nor turned. Now, with somewhere to go (`Home`), it shuts, turns its spine to you in
    /// perspective and stands there, in the light of the dimmed shelf; never gone, never half see-through.
    #[test]
    fn a_case_goes_home_instead_of_fading() {
        crate::cozy_extras::set(crate::cozy_extras::Extras::default());
        crate::motion::set_cozy(true);
        for system in ["snes", "gba", "psx"] {
            let mut s = scene(system, true);
            // The front in its case's own shape, as the hosts make it.
            let size = s.medium.size();
            let fh = 120 * size.front_h / size.front;
            s.front = Rc::new(Pic::new(120, fh, 0xff10_20e0));
            s.from = (40, 40, 120, fh);
            // Its game see-through: only the case is counted, not what passes in front of it.
            s.game = Some(Rc::new(Pic::new(90, 90, 0x0000_0000)));
            s.load_rig(2);
            let home = (520.0, 300.0);
            s.home = Some(Home { spine: Rc::new(Pic::new(20, 180, 0xffe0_2010)), at: home, scale: 1.0 });
            let (full, _, _) = front_and_spine(&frame(&mut s, 60, STAGE));
            assert!(full > 8_000, "{system}: the front at first ({full})");
            let mut last = full as f32;
            for k in 4..=72 {
                let ms = k * 1000 / 60;
                let (f, sp, at) = front_and_spine(&frame(&mut s, ms, STAGE));
                let seen = (f + sp) as f32;
                assert!(seen > 1_000.0, "{system} {ms} ms: the case is gone ({f} front, {sp} spine)");
                // It turns and goes away smoothly: never more than a fifth of its first size in one 60 Hz frame.
                assert!((seen - last).abs() < full as f32 * 0.2, "{system} {ms} ms: the case jumps from {last} to {seen} pixels");
                last = seen;
                if ms >= 1_100 {
                    assert!(f < 200 && sp > 1_000, "{system} {ms} ms: its spine to you ({f} front, {sp} spine)");
                    assert!(at.0.abs_diff(home.0 as usize) <= 3 && at.1.abs_diff(home.1 as usize) <= 3, "{system} {ms} ms: home at {home:?}, is at {at:?}");
                }
            }
        }
        // Without a home (Easy, Kids, Android) it fades as before: nothing of it left, the same picture as with a
        // see-through case.
        let mut s = scene("snes", true);
        s.front = Rc::new(Pic::new(120, 180, 0xff10_20e0));
        let with = frame(&mut s, 1_100, STAGE);
        s.front = Rc::new(Pic::new(120, 180, 0x0000_0000));
        assert!(with == frame(&mut s, 1_100, STAGE), "no home: faded");
    }

    /// The way back runs it backwards: the case is out and open before its disc is back beside it, the disc in before it
    /// shuts, and it is home before the game is in.
    #[test]
    fn a_case_is_out_and_open_while_its_game_comes_back_and_home_before_the_game_is_in() {
        for opens in [true, false] {
            for ms in (0..=420).step_by(10) {
                assert_eq!(case_back(Duration::from_millis(ms), opens).1, 0.0, "{ms} ms: the case is where its game leaves it");
            }
            for ms in (0..=580).step_by(10) {
                assert!(!opens || case_back(Duration::from_millis(ms), opens).0 == 0.0, "{ms} ms: a disc's case is open while the disc is near");
            }
            assert_eq!(case_back(cabinet::IN_AT_COZY, opens).1, 1.0, "home before the game is in");
        }
    }

    /// The cover of a jewel or DVD case swung open by squeezing it flat, to a hairline at the end. Now in perspective:
    /// its free edge comes nearer and is taller than the hinge, and it never gets thinner than a sixth of the case.
    #[test]
    fn a_cover_opens_in_perspective_and_never_to_a_hairline() {
        let cover = Pic::new(120, 180, 0xff10_20e0);
        for open in [0.0f32, 0.25, 0.5, 0.75, 1.0] {
            let mut buf = vec![0u32; 400 * 400];
            let r = art::draw_hinged(&mut Canvas::new(&mut buf, 400, 400), &cover, (100.0, 100.0), (120.0, 180.0), open, COVER_OPEN_DEG, 256, (0, 400));
            assert_eq!(r.0, 100, "{open}: stays on its hinge");
            assert!(r.2 >= 20, "{open}: {} px wide, never a hairline", r.2);
            let col_h = |x: i32| (0..400).filter(|y| buf[(*y * 400 + x) as usize] != 0).count();
            let (hinge, free) = (col_h(r.0 + 1), col_h(r.0 + r.2 - 2));
            if open > 0.2 {
                assert!(free > hinge, "{open}: the free edge comes nearer, taller ({free}) than at the hinge ({hinge})");
            } else {
                assert!(free.abs_diff(hinge) <= 2, "{open}: flat when shut");
            }
        }
    }

    /// 11.11.: a cartridge turning round on its way to a rear slot (Game Boy, GBA, DS) was squeezed flat to a line half
    /// way, then its back popped in. Now it is a thin box: its edge (its own plastic, `edge_of`) shows beside its face,
    /// so at every 60 Hz frame of the turn a good part of it is there.
    #[test]
    fn a_cartridge_turning_round_shows_its_edge_never_a_line() {
        crate::cozy_extras::set(crate::cozy_extras::Extras::default());
        crate::motion::set_cozy(true);
        // Magenta, the face's and (darker) its edge's colour, found nowhere else in the picture.
        let magenta = |p: &u32| {
            let (r, g, b) = (p >> 16 & 0xff, p >> 8 & 0xff, p & 0xff);
            r > 40 && b > 40 && g < 24 && r.abs_diff(b) < 12
        };
        for system in ["gb", "gba", "nds"] {
            let mut s = scene(system, true);
            s.load_rig(2);
            assert!(s.rig.as_ref().is_some_and(|r| r.rig.label_away), "{system}: turns round to its rear slot");
            let pic = Rc::new(Pic::new(60, 70, 0xffe0_00e0));
            s.game = Some(pic.clone());
            s.game_back = Some(pic);
            // Its turn falls between 420 and 900 ms (`face_at` over the flight): every frame of that.
            let counts: Vec<usize> = (25..=54).map(|k| frame(&mut s, k * 1000 / 60, STAGE).iter().filter(|p| magenta(p)).count()).collect();
            // At its thinnest (edge on) against the most it showed before: squeezed flat it was 29-31 %, with its edge 44-51 %.
            let (at, thinnest) = counts.iter().copied().enumerate().min_by_key(|(_, n)| *n).unwrap();
            let before = counts[..at.max(1)].iter().copied().max().unwrap();
            assert!(before > 1_000 && thinnest as f32 >= 0.4 * before as f32, "{system}: thins to {thinnest} of {before} pixels: {counts:?}");
        }
    }

    /// Ritual review 14.11.: the NES cartridge flew to its console dark and behind the lid, and came out in front of it again.
    /// On its whole way to the door (420 to 900 ms) it is in front of the console, lit as ever: its colour is on screen in
    /// every frame of it, never gone to the inside's shadow.
    #[test]
    fn a_front_loaders_cartridge_flies_to_the_door_in_front_of_the_console_and_never_dark() {
        crate::cozy_extras::set(crate::cozy_extras::Extras::default());
        crate::motion::set_cozy(true);
        // Bright magenta, found nowhere else in the picture; in the inside's shadow (48 of 256) it is a dark purple.
        let magenta = |p: &u32| {
            let (r, g, b) = (p >> 16 & 0xff, p >> 8 & 0xff, p & 0xff);
            r > 150 && b > 150 && g < 60
        };
        for system in ["nes"] {
            let mut s = scene(system, true);
            s.load_rig(2);
            assert!(s.rig.as_ref().is_some_and(|r| r.rig.kind() == ir::Kind::Door), "{system}: a front-loader");
            s.game = Some(Rc::new(Pic::new(60, 70, 0xffe0_00e0)));
            let counts: Vec<usize> = (26..=53).map(|k| frame(&mut s, k * 1000 / 60, STAGE).iter().filter(|p| magenta(p)).count()).collect();
            let most = counts.iter().copied().max().unwrap();
            let least = counts.iter().copied().min().unwrap();
            assert!(most > 1_000 && least as f32 >= 0.4 * most as f32, "{system}: lit pixels of the cartridge on its way {least} of {most}: {counts:?}");
        }
    }

    /// Review 11.11.: the 3DS looked off the middle. Every console with its own scene stands in the middle of the
    /// screen as the player sees it, its game in it (Cozy, seated, before the fade): the middle of what is drawn over the
    /// page is within a fiftieth of the screen's width of the screen's middle, its weight within a thirtieth.
    #[test]
    fn every_console_stands_in_the_middle_of_its_scene() {
        crate::cozy_extras::set(crate::cozy_extras::Extras::default());
        crate::motion::set_cozy(true);
        let mut off = Vec::new();
        for system in ["psx", "ps2", "gamecube", "snes", "gba", "gba-sp", "gb", "gbc", "gamegear", "n64", "megadrive", "dreamcast", "saturn", "xbox", "3ds", "nds", "wii", "psp", "pcengine", "mastersystem", "nes"] {
            let mut s = scene(system, true);
            s.load_rig(2);
            assert!(s.rig.is_some(), "{system}: has its scene");
            let px = frame(&mut s, 1_300, STAGE);
            let page = px[0];
            let cols: Vec<usize> = (0..720).filter(|x| (0..1600).any(|y| px[y * 720 + x] != page && y > 200)).collect();
            let (l, r) = (cols[0] as f32, *cols.last().unwrap() as f32 + 1.0);
            let middle = (l + r) / 2.0 - 360.0;
            // And where its weight is: a console drawn at a slant may fill its box unevenly.
            let (mut sum, mut count) = (0.0f64, 0usize);
            for (i, p) in px.iter().enumerate() {
                if *p != page && i / 720 > 200 {
                    sum += (i % 720) as f64;
                    count += 1;
                }
            }
            let weight = (sum / count.max(1) as f64) as f32 - 360.0;
            if middle.abs() > 720.0 / 50.0 || weight.abs() > 720.0 / 30.0 {
                off.push(format!("{system}: box {middle:+.1} px ({l}..{r}), weight {weight:+.1} px"));
            }
        }
        assert!(off.is_empty(), "off the middle:\n{}", off.join("\n"));
    }

    /// 11.11.: a top-loader's lid was a flat picture turning, a sheet without an edge. Now a slab: open, its edge shows
    /// round the face it turns to you (more of it drawn than of the sheet alone, at every step of the opening); shut,
    /// nothing is added (its edge is in the console's picture).
    #[test]
    fn a_lid_has_its_thickness_open_and_none_shut() {
        for system in ["psx", "saturn", "dreamcast", "gamecube"] {
            let pics = Rigged::load(system, Game::Disc, 2).expect(system);
            let ir::Mech::Lid(lid) = pics.rig.mech() else { panic!("{system}: a lid") };
            let drawn = |open: f32, slab: bool| {
                let mut buf = vec![0u32; 800 * 800];
                draw_lid_as(&mut Canvas::new(&mut buf, 800, 800), lid, &pics, (200.0, 300.0), open, 256, slab);
                buf.iter().filter(|p| **p != 0).count()
            };
            assert_eq!(drawn(0.0, true), drawn(0.0, false), "{system}: shut, no edge added");
            for k in 4..=20 {
                let open = k as f32 / 20.0;
                let (slab, sheet) = (drawn(open, true), drawn(open, false));
                assert!(slab as f32 >= sheet as f32 * 1.04 + 20.0, "{system} at {open}: its edge shows ({slab} against {sheet} px)");
            }
        }
    }

    #[test]
    fn a_disc_case_opens_and_the_disc_lifts_off_its_tray_it_does_not_slide_out_sideways() {
        crate::cozy_extras::set(crate::cozy_extras::Extras::default());
        crate::motion::set_cozy(true);
        const DISC: u32 = 0xff00ff;
        let count = |px: &[u32], c: u32| px.iter().filter(|p| **p & 0xffffff == c).count();
        for system in ["psx", "saturn", "dreamcast", "gamecube"] {
            let mut s = scene(system, true);
            s.load_rig(2);
            s.game = Some(Rc::new(Pic::new(90, 90, 0xff00_0000 | DISC)));
            let (shut, open) = (frame(&mut s, 60, STAGE), frame(&mut s, 400, STAGE));
            let cover = count(&shut, 0x406080);
            assert!(cover > 8000, "{system}: the case's front stands on the shelf ({cover})");
            assert!(count(&open, 0x406080) < cover / 4, "{system}: the cover swings away on its hinge, the case opens");
            // The disc lies on the tray in the middle of the case until it is lifted, as wide as the case is.
            let (mut n, mut sx) = (0usize, 0usize);
            for (i, p) in open.iter().enumerate() {
                if *p & 0xffffff == DISC {
                    n += 1;
                    sx += i % 720;
                }
            }
            assert!(n > 2000, "{system}: the disc is seen once the case is open ({n})");
            let middle = 40 + 120 / 2;
            assert!(sx / n.max(1) < middle + 12 && sx / n.max(1) + 12 > middle, "{system}: the disc rests at the case's middle, its mean x is {}", sx / n.max(1));
        }
    }

    // ---- A handheld's power-on: power light, display, then the game's picture (instead of the tube's / the fade) ----

    /// Every system whose game had no TV: the whole class (`eject::Screen::Lcd`), rigs and plain pictures and systems
    /// without any picture of their own.
    const HANDHELDS: &[&str] = &[
        "gb", "gbh", "gbc", "gbch", "gba", "gbah", "gamegear", "ggh", "ngp", "ngpc", "wonderswan", "wonderswancolor", "atarilynx", "lynx", "virtualboy", "pokemini",
        "supervision", "gamate", "gameandwatch", "arduboy", "pico-8", "tic-80", "psp", "pspminis", "nds", "3ds", "n3ds", "psvita",
    ];
    /// Handhelds with a rig and a display in it (`rig.json`): the power light, then the display on the console's picture.
    const RIG_DISPLAYS: &[&str] = &["gb", "gbc", "gba", "gamegear", "nds", "3ds", "ngpc", "ngp", "atarilynx", "wonderswan"];

    #[test]
    fn a_handhelds_start_ends_in_its_display_light_a_consoles_on_a_tv_in_the_fade() {
        for system in HANDHELDS {
            assert!(power_on_light(system, true, true, true).is_some(), "{system}: its start ends in its display's light");
            for (cozy, full, extra) in [(false, true, true), (true, false, true), (true, true, false)] {
                assert!(power_on_light(system, cozy, full, extra).is_none(), "{system}: cozy {cozy} motion full {full} extra {extra}: the fade as before");
            }
        }
        for system in ["snes", "nes", "n64", "psx", "ps2", "gamecube", "wii", "xbox", "megadrive", "dreamcast", "saturn", "mastersystem", "pcengine", "atari2600", "arcade", "c64"] {
            assert!(power_on_light(system, true, true, true).is_none(), "{system}: a console on a TV keeps its fade and the tube");
        }
    }

    #[test]
    fn a_phone_game_loads_onto_the_phone_on_the_other_consoles_beat_and_its_display_goes_on() {
        // Nothing goes in: no disc or cartridge flies, the phone lights up as a handheld does.
        assert!(lights_up_only("j2me"));
        assert_eq!(crate::media::Medium::of("j2me"), crate::media::Medium::GenericDigital, "a download's case, not a CD case");
        assert!(power_on_light("j2me", true, true, true).is_some(), "the phone's display goes on, no TV");
        assert!(phone_display().is_some_and(|(x0, y0, x1, y1)| x1 - x0 > 0.3 && y1 - y0 > 0.1), "the display is found in the picture");
        // Loaded when another console's medium is in (1.2 s), lit by the shared 1.4 s.
        let ms = Duration::from_millis;
        assert_eq!(phone_loaded(ms(300), true), 0.0);
        assert!(phone_loaded(ms(800), true) > 0.3 && phone_loaded(ms(800), true) < 0.7);
        assert_eq!(phone_loaded(cabinet::IN_AT_COZY, true), 1.0);
        assert!(ir::pose(ir::Kind::Cart, ms(1400)).lit > 0.5, "the display is on at 1.4 s");
        assert_eq!(ir::pose(ir::Kind::Cart, cabinet::IN_AT_COZY).lit, 0.0, "and not before it has loaded");
        assert_eq!(phone_loaded(cabinet::in_at(false), false), 1.0, "Quick: loaded when its quicker medium would be in");
    }

    #[test]
    fn the_power_on_takes_no_longer_than_the_fade_it_replaces_and_the_picture_is_whole_sooner() {
        let (old, new) = (hold(true, false).unwrap(), hold(true, true).unwrap());
        assert!(new < old, "the window comes {new:?} after A, not {old:?}");
        // The tube's switch-on was 300 ms after the window, the display's light clears in LCD_LEN.
        assert!(new + crate::crt_on::LCD_LEN < old + crate::crt_on::DURATION, "whole picture: {:?} against {:?}", new + crate::crt_on::LCD_LEN, old + crate::crt_on::DURATION);
        assert_eq!(hold(false, true), None, "the quick way never holds");
    }

    #[test]
    fn the_steps_come_in_order_light_display_spreading_picture() {
        use crate::cabinet::{FADE_AT_COZY, IN_AT_COZY};
        let ms = |d: Duration| d.as_secs_f32() * 1000.0;
        let (led_on, led_full) = (IN_AT_COZY.as_secs_f32() * 1000.0 + 90.0, IN_AT_COZY.as_secs_f32() * 1000.0 + 240.0);
        let (lit_on, lit_full) = (IN_AT_COZY.as_secs_f32() * 1000.0 + ir::LIT_AFTER.0, IN_AT_COZY.as_secs_f32() * 1000.0 + ir::LIT_AFTER.1);
        let (wash_on, wash_full) = (ms(FADE_AT_COZY + WASH.0), ms(FADE_AT_COZY + WASH.1));
        assert!(led_on < lit_on, "the power light first, then the display");
        assert!(lit_on > IN_AT_COZY.as_secs_f32() * 1000.0, "nothing lights before the game is in");
        assert!(lit_full <= wash_full && lit_on < wash_on + 1.0, "the display is lit before the screen goes dark round it");
        assert!(led_full <= wash_full && wash_on < wash_full);
        // The scene itself: nothing before the game is in, the display coming up, then the end.
        for system in RIG_DISPLAYS {
            let rig_kind = ir::files(system, Game::of(system, "x.rom")).unwrap().rig.kind();
            for k in 0..=40u64 {
                let (a, b) = (ir::pose(rig_kind, Duration::from_millis(k * 50)), ir::pose(rig_kind, Duration::from_millis(k * 50 + 50)));
                assert!(b.lit + 1e-6 >= a.lit && b.led + 1e-6 >= a.led, "{system}: they only come on");
                if a.led < 1.0 {
                    assert!(a.lit <= a.led + 1e-6, "{system} {} ms: the display never before its light", k * 50);
                }
            }
        }
    }

    #[test]
    fn the_fade_of_a_power_on_is_the_light_spreading_and_ends_when_the_window_comes() {
        let mut s = scene("gba", true);
        s.power_on = Some(0xc0c8d0);
        let at = |ms: u64| s.fade_at(Duration::from_millis(ms));
        assert_eq!(at(0), 0.0);
        assert_eq!(at(cabinet::fade_delay(true).as_millis() as u64), 0.0, "it waits for the display");
        assert!(at(1500) > 0.0 && at(1500) < 1.0, "fading at 1500 ms: {}", at(1500));
        let end = hold(true, true).unwrap().as_millis() as u64;
        assert_eq!(at(end), 1.0, "all dark when the window comes");
        let mut last = 0.0;
        for ms in (0..=end + 100).step_by(10) {
            assert!(at(ms) + 1e-6 >= last, "{ms} ms: never goes back");
            last = at(ms);
        }
        // Painted: all black at the end, never a light over the screen.
        let mut buf = vec![0x202020u32; 100 * 100];
        s.veil(&mut Canvas::new(&mut buf, 100, 100), at(end));
        assert!(buf.iter().all(|p| *p == 0), "black at the end");
    }

    #[test]
    fn any_button_ends_a_power_on_at_once() {
        let mut hand = HandOver::new(true, true);
        assert!(!hand.take_release(Duration::from_millis(900)), "held while it plays");
        assert!(hand.skip(), "the first button");
        assert!(hand.take_release(Duration::from_millis(900)), "the game comes at once");
        assert!(!hand.skip(), "once");
        let mut late = HandOver::new(true, true);
        assert!(!late.take_release(hold(true, true).unwrap() - Duration::from_millis(1)));
        assert!(late.take_release(hold(true, true).unwrap()), "else when the hold has run");
    }

    /// The scene with its display light on or off, as the player would see it.
    fn lit_scene(system: &str, on: bool) -> Scene {
        let mut s = scene(system, true);
        s.power_on = on.then(|| light_of(system));
        s.load_rig(2);
        s
    }

    #[test]
    fn a_rig_handhelds_display_lights_only_after_the_game_is_in_and_only_the_glass() {
        crate::cozy_extras::set(crate::cozy_extras::Extras::default());
        crate::motion::set_cozy(true);
        for system in RIG_DISPLAYS {
            let (mut on, mut off) = (lit_scene(system, true), lit_scene(system, false));
            assert!(on.rig.as_ref().is_some_and(|r| r.lit.is_some()), "{system}: a rig with its display");
            for before in [200u64, 700, 1190] {
                assert!(frame(&mut on, before, STAGE) == frame(&mut off, before, STAGE), "{system} {before} ms: nothing lit before the game is in");
            }
            let (a, b) = (frame(&mut on, 1560, STAGE), frame(&mut off, 1560, STAGE));
            let changed: Vec<usize> = (0..a.len()).filter(|&i| a[i] != b[i]).collect();
            assert!(changed.len() > 400, "{system}: the glass is lit ({} px)", changed.len());
            // Only the glass: one block (or two for the DS pair) of the console, a small part of the screen.
            let rows = |f: &dyn Fn(usize) -> usize| changed.iter().map(|&i| f(i)).fold((usize::MAX, 0), |(lo, hi), v| (lo.min(v), hi.max(v)));
            let ((y0, y1), (x0, x1)) = (rows(&|i| i / 720), rows(&|i| i % 720));
            assert!((y1 - y0) * (x1 - x0) < 720 * 600, "{system}: the lit part is the display, {x0}-{x1} x {y0}-{y1}");
            // Brighter, not darker.
            let bright = |p: u32| (p >> 16 & 0xff) + (p >> 8 & 0xff) + (p & 0xff);
            assert!(changed.iter().filter(|&&i| bright(a[i]) >= bright(b[i])).count() * 10 >= changed.len() * 9, "{system}: lit means brighter");
            // The last step before the fade: the display (all of it) is at its light.
            let (full, none) = (frame(&mut on, 1500, STAGE), frame(&mut off, 1500, STAGE));
            let still = (0..full.len()).filter(|&i| full[i] != none[i]).count();
            assert!(still >= changed.len() * 9 / 10, "{system}: the display stays lit ({still} of {})", changed.len());
        }
    }

    #[test]
    fn every_handheld_start_paints_without_a_gap_with_or_without_a_display_of_its_own() {
        crate::cozy_extras::set(crate::cozy_extras::Extras::default());
        crate::motion::set_cozy(true);
        for system in HANDHELDS {
            let mut s = lit_scene(system, true);
            let mut last = None;
            for ms in [300u64, 700, 1100, 1300, 1450, 1560] {
                let f = frame(&mut s, ms, STAGE);
                assert!(lit(&f) > 3000, "{system} {ms} ms: a console (or its stand-in) stands there");
                if ms >= 1300 {
                    last = Some(f);
                }
            }
            assert!(last.is_some());
            assert!(light_of(system) != 0, "{system}: it ends in some light");
        }
    }

    /// Every handheld's rig in every colour it comes in: (shown as, rig id, its files).
    fn every_handheld_rig() -> Vec<(String, &'static str, ir::Files)> {
        let mut all = Vec::new();
        for id in ir::rig_ids() {
            let hardware = if id == "gba-sp" { "gba" } else { id };
            if crate::eject::Screen::of(FormFactor::of(hardware)) != crate::eject::Screen::Lcd {
                continue;
            }
            all.push((id.to_string(), id, ir::files_of(id, None).unwrap()));
            for c in crate::system_art::colours_of(id) {
                all.push((format!("{id} {}", c.id), id, ir::files_of(id, Some(c.rig())).unwrap()));
            }
        }
        all
    }

    /// The scene of a handheld's rig `f` at its power-on, its display's light on, or off: the same moves (a PSP turns back to its
    /// front either way), only its lit pixels left out.
    fn rig_scene(id: &str, f: ir::Files, on: bool) -> Scene {
        let mut s = scene(id, true);
        s.power_on = Some(light_of(id));
        let mut r = Rigged::of_files(f, 2.0).unwrap();
        if !on {
            let none = |p: &mut Option<Pic>| p.iter_mut().for_each(|p| p.px.iter_mut().for_each(|x| *x = 0));
            (none(&mut r.lit), none(&mut r.lid_lit), none(&mut r.front_lit));
        }
        s.rig = Some(Rc::new(r));
        s.rig_asked = true;
        s
    }

    /// 16.11. (Alex's hand test, pocket, 0.4.2-test050.4): with the GBA SP the display stayed dark after the cartridge went in;
    /// the power light came on, the game came up out of a dark screen. The SP's glass is on its lid's underside (`under.png`),
    /// which turns open, and its rig had no display at all. Missed before: the display tests listed the rigs by hand
    /// (`RIG_DISPLAYS`) and the SP was not on it, nor any colour. The PSP, on its back for its UMD, showed no glass at all; since
    /// 16.11. (Alex) it turns back to its front. Now every handheld rig in every colour: just before the game's window comes
    /// (`Scene::hold`), the console's glass is brighter than without its light, in one block of it.
    #[test]
    fn every_handheld_rig_in_every_colour_lights_its_display_before_the_game_comes() {
        crate::cozy_extras::set(crate::cozy_extras::Extras::default());
        crate::motion::set_cozy(true);
        let bright = |p: u32| (p >> 16 & 0xff) + (p >> 8 & 0xff) + (p & 0xff);
        let mut dark = Vec::new();
        let all = every_handheld_rig();
        for name in ["gba-sp", "psp", "gba milky", "gbc dandelion"] {
            let id = name.split(' ').next().unwrap();
            assert!(all.iter().any(|a| a.0 == name || (a.0 == id && !name.contains(' '))), "{name}: in the list");
        }
        for (name, id, _) in &all {
            let files = || every_handheld_rig().into_iter().find(|a| &a.0 == name).unwrap().2;
            let (mut on, mut off) = (rig_scene(id, files(), true), rig_scene(id, files(), false));
            let at = on.hold().unwrap().as_millis() as u64 - 60;
            let (a, b) = (frame(&mut on, at, STAGE), frame(&mut off, at, STAGE));
            let lit: Vec<usize> = (0..a.len()).filter(|&i| bright(a[i]) > bright(b[i]) + 30).collect();
            if lit.len() < 1500 {
                dark.push(format!("{name}: {} px lit", lit.len()));
                continue;
            }
            let span = |f: &dyn Fn(usize) -> usize| lit.iter().map(|&i| f(i)).fold((usize::MAX, 0), |(lo, hi), v| (lo.min(v), hi.max(v)));
            let ((y0, y1), (x0, x1)) = (span(&|i| i / 720), span(&|i| i % 720));
            if (y1 - y0) * (x1 - x0) >= 720 * 600 {
                dark.push(format!("{name}: lit {x0}-{x1} x {y0}-{y1}, more than the glass"));
            }
        }
        assert!(dark.is_empty(), "displays that stay dark at the power-on:\n{}", dark.join("\n"));
    }

    /// 16.11. (Alex: PSP = B): the PSP turns back to its front after its UMD door has shut, as smoothly as it turned to its
    /// back, its display lights there; its start is longer by that turn only (about half a second, never past a second), the
    /// others' as before. While it turns back its UMD is inside, never drawn over the door.
    #[test]
    fn the_psp_turns_back_to_its_front_smoothly_and_its_start_is_at_most_a_second_longer() {
        crate::cozy_extras::set(crate::cozy_extras::Extras::default());
        crate::motion::set_cozy(true);
        let psp = rig_scene("psp", ir::files_of("psp", None).unwrap(), true);
        let gba = rig_scene("gba", ir::files_of("gba", None).unwrap(), true);
        let longer = psp.hold().unwrap() - gba.hold().unwrap();
        assert!(longer >= Duration::from_millis(300) && longer <= Duration::from_secs(1), "{longer:?}");
        assert_eq!(gba.turn_back(), Duration::ZERO, "the others keep their start");
        let mut off = scene("psp", true);
        off.power_on = None;
        assert_eq!(off.turn_back(), Duration::ZERO, "without the power-on it stays on its back, as before");
        let kind = ir::Kind::Lid;
        let face = |ms: u64| ir::pose_of(kind, Duration::from_millis(ms), true).face;
        let shut = (cabinet::IN_AT_COZY + ir::SHUT_AFTER).as_millis() as u64;
        assert_eq!(face(shut), 1.0, "on its back until its door has shut");
        let lit_from = (0..3000).find(|&ms| ir::pose_of(kind, Duration::from_millis(ms), true).led > 0.0).unwrap();
        assert_eq!(face(lit_from), 0.0, "on its front before its light comes ({lit_from} ms)");
        // As smooth as the turn to the back: no step at 60 Hz larger than that turn's largest.
        let steps = |a: u64, b: u64, f: &dyn Fn(u64) -> f32| (a..b).step_by(16).map(|ms| (f(ms + 16) - f(ms)).abs()).fold(0.0f32, f32::max);
        let there = steps(0, 800, &|ms| ir::pose(kind, Duration::from_millis(ms)).face);
        let back = steps(shut - 100, lit_from + 100, &face);
        assert!(back <= there + 1e-3 && back > 0.0, "turn back {back} vs turn there {there}");
        // The UMD is not painted while it turns back: the frames with and without a game picture are the same.
        let mut with = rig_scene("psp", ir::files_of("psp", None).unwrap(), true);
        let mut without = rig_scene("psp", ir::files_of("psp", None).unwrap(), true);
        without.game = Some(Rc::new(Pic::new(90, 90, 0xffff_0000)));
        for ms in [shut + 40, shut + 140, shut + 240] {
            assert!(frame(&mut with, ms, STAGE) == frame(&mut without, ms, STAGE), "{ms} ms: the UMD shows while it turns back");
        }
    }

    /// Frames of every handheld rig's power-on for a look (`POCKETUI_HANDHELD_ON_DUMP=<dir>`): PPMs at 900, 1200, 1350, 1500 ms.
    #[test]
    fn dump_handheld_power_on_frames() {
        let Some(dir) = std::env::var_os("POCKETUI_HANDHELD_ON_DUMP") else { return };
        crate::cozy_extras::set(crate::cozy_extras::Extras::default());
        crate::motion::set_cozy(true);
        for (name, id, f) in every_handheld_rig() {
            let mut s = rig_scene(id, f, true);
            let end = s.hold().unwrap().as_millis() as u64 - 60;
            for ms in [900u64, 1200, 1350, 1500, 1600, 1700, 1800, 1900, end] {
                let px = frame(&mut s, ms, STAGE);
                let (x0, y0, w, h) = (0usize, 500usize, 720usize, 520usize);
                let mut out = format!("P6\n{w} {h}\n255\n").into_bytes();
                for y in y0..y0 + h {
                    for x in x0..x0 + w {
                        let p = px[y * 720 + x];
                        out.extend([(p >> 16) as u8, (p >> 8) as u8, p as u8]);
                    }
                }
                std::fs::write(std::path::Path::new(&dir).join(format!("{}-{ms}.ppm", name.replace(' ', "-"))), out).unwrap();
            }
        }
    }

    /// Every handheld of ROCKNIX's list (and the phone), by the ids a library has them under.
    const EVERY_HANDHELD: &[&str] = &[
        "arduboy", "atarilynx", "gameandwatch", "gamate", "gb", "gbh", "gbc", "gbch", "gba", "gbah", "gbav", "gamegear", "ggh", "gp32", "j2me", "megaduck", "nds", "ndsiware", "3ds", "n3ds",
        "ngp", "ngpc", "palm", "pico-8", "pokemini", "psp", "pspminis", "psvita", "supervision", "tic-80", "virtualboy", "wonderswan", "wonderswancolor",
    ];

    /// 15.11. (tester, RG353: "NGP/NGPC should look like GBA"; Ritual review 14.11. found, too, that GP32, Mega Duck, Palm, DSiWare and
    /// GBA Video started on a TV and Supervision, Game & Watch, Gamate, Arduboy and PSP Minis put their game into the crate): a
    /// handheld's game goes into a handheld on its own display, never a TV, a crate or an empty box.
    /// Missed before: the tests only listed the handhelds that already had a picture.
    #[test]
    fn every_handheld_starts_in_a_handheld_on_its_own_display_never_a_tv_or_the_crate() {
        for id in EVERY_HANDHELD {
            assert_eq!(crate::eject::Screen::of(FormFactor::of(id)), crate::eject::Screen::Lcd, "{id}: its own display, no TV");
            let shown = scene_system(id);
            assert!(crate::system_art::get(shown).is_some(), "{id}: shows {shown}, which has no picture");
            assert_ne!(slot_of(shown), slot_of(STAND_IN), "{id}: not the crate of games");
            assert!(power_on_light(shown, true, true, true).is_some(), "{id}: its display lights");
            // A handheld with a picture of its own shows it as it is (the Virtual Boy's goggles glow, PICO-8 and TIC-80 draw their own
            // display); one that stands in for another needs the glass of the picture it shows.
            let own = shown == *id;
            assert!(own || plain_display(shown).is_some() || ir::files(shown, Game::of(shown, "x.rom")).is_some_and(|f| f.rig.display.is_some()), "{id}: shows {shown}: a display to light");
        }
    }

    /// 15.11. (tester: "NGP/NGPC should look like GBA"): the cartridge goes into the console's slot, label to its back, as the GBA's
    /// does, and the display lights; the plain animation (a flat cartridge dropped on the screen) was all the Neo Geo Pockets had.
    #[test]
    fn a_neo_geo_pocket_takes_its_cartridge_like_the_gba_in_its_rig() {
        for system in ["ngp", "ngpc"] {
            let game = Game::of(system, "x.ngc");
            let f = ir::files(system, game).unwrap_or_else(|| panic!("{system}: a rig, not the plain animation"));
            let gba = ir::files("gba", Game::of("gba", "x.gba")).unwrap().rig;
            assert!(f.rig.behind && f.rig.label_away && f.rig.kind() == ir::Kind::Cart, "{system}: slot at the back, label turned away");
            assert!(f.rig.display.is_some() && f.rig.led.is_some(), "{system}: its power light and its display");
            assert_eq!((f.rig.behind, f.rig.label_away), (gba.behind, gba.label_away), "{system}: as the GBA");
        }
    }

    /// 15.11. (Lead: "the label must not be upside down"): a UMD flies from its case to the PSP's back and lies in the cover with
    /// its label upright, never turned. Painted with a red corner at its top left and a blue one at its bottom right: red stays the
    /// upper one all the way. Missed before: the tests checked its size and its place, never which way up it lay.
    #[test]
    fn a_umd_goes_into_the_psp_with_its_label_upright() {
        crate::cozy_extras::set(crate::cozy_extras::Extras::default());
        crate::motion::set_cozy(true);
        let mut s = scene("psp", true);
        let mut px = vec![0xffc0_c0c0u32; 90 * 90];
        for y in 0..24 {
            for x in 0..24 {
                px[y * 90 + x] = 0xffff_0000;
                px[(89 - y) * 90 + 89 - x] = 0xff00_00ff;
            }
        }
        s.game = Some(Rc::new(Pic::of(90, 90, px)));
        s.load_rig(2);
        assert!(s.rig.is_some(), "the PSP's rig");
        let mut seen = 0;
        for ms in [700u64, 800, 900, 1000, 1050] {
            let f = frame(&mut s, ms, STAGE);
            let centre = |hit: &dyn Fn(u32) -> bool| {
                let (mut n, mut x, mut y) = (0i64, 0i64, 0i64);
                for (i, p) in f.iter().enumerate().filter(|(_, p)| hit(**p)) {
                    let _ = p;
                    n += 1;
                    x += (i % 720) as i64;
                    y += (i / 720) as i64;
                }
                (n > 12).then(|| (x / n, y / n))
            };
            let red = centre(&|p| (p >> 16 & 0xff) > (p >> 8 & 0xff) + 70 && (p >> 16 & 0xff) > (p & 0xff) + 70);
            let blue = centre(&|p| (p & 0xff) > (p >> 16 & 0xff) + 70 && (p & 0xff) > (p >> 8 & 0xff) + 70);
            if let (Some(r), Some(b)) = (red, blue) {
                seen += 1;
                assert!(r.1 < b.1 && r.0 < b.0, "{ms} ms: red {r:?} blue {b:?}: the UMD's label is not upright");
            }
        }
        assert!(seen >= 2, "the UMD was seen on only {seen} of 5 frames");
    }

    #[test]
    fn a_plain_handheld_picture_lights_its_glass_too() {
        crate::cozy_extras::set(crate::cozy_extras::Extras::default());
        crate::motion::set_cozy(true);
        // No `load_rig`: the plain animation, whatever the global switches say (other tests set them in parallel).
        for system in ["gb", "gbc", "gba", "gamegear", "atarilynx", "ngpc", "wonderswan", "pokemini", "nds", "3ds", "psp", "psvita", "j2me"] {
            let (mut on, mut off) = (scene(system, true), scene(system, true));
            on.power_on = Some(light_of(system));
            assert!(on.rig.is_none(), "{system}: plain animation");
            let (a, b) = (frame(&mut on, 1500, STAGE), frame(&mut off, 1500, STAGE));
            let n = (0..a.len()).filter(|&i| a[i] != b[i]).count();
            assert!(n > 300, "{system}: its glass is lit ({n} px)");
            assert!(frame(&mut on, 900, STAGE) == frame(&mut off, 900, STAGE), "{system}: not before the game is in");
        }
    }

    #[test]
    fn a_phone_games_bar_fills_on_the_phones_display_while_the_case_fades() {
        let mut s = scene("j2me", true);
        let ink = |px: &[u32]| px.iter().filter(|p| **p & 0xffffff == 0x24301f).count();
        let (early, half, full) = (ink(&frame(&mut s, 200, STAGE)), ink(&frame(&mut s, 800, STAGE)), ink(&frame(&mut s, 1180, STAGE)));
        assert_eq!(early, 0, "no bar before the phone is up");
        assert!(half > 20 && full > half + 20, "the bar fills: {early} {half} {full}");
    }

    #[test]
    fn every_plain_displays_glass_is_found_and_its_light_is_what_the_picture_gives() {
        let mut wrong = Vec::new();
        for (id, d) in DISPLAYS {
            let sp = crate::system_art::get(id).unwrap_or_else(|| panic!("{id}: no picture"));
            let at = d.at.iter().map(|&(x, y)| (x * sp.w as f32, y * sp.h as f32));
            let mask = ir::display_mask(&sp.px, sp.w as usize, sp.h as usize, at, d.fuzz);
            let n = mask.iter().filter(|m| **m).count();
            let solid = sp.px.iter().filter(|p| **p >> 24 == 255).count();
            assert!(n > 300 && n * 2 < solid, "{id}: the glass is a part of the console ({n} of {solid})");
            let light = ir::display_light(&sp.px, &mask);
            if light != d.lit {
                wrong.push(format!("{id}: table lit {:#08x}, the picture gives {light:#08x}", d.lit));
            }
        }
        assert!(wrong.is_empty(), "{wrong:#?}");
    }

    /// Fable 5.2 (12.11.): the power-on spread an almost white light over the whole screen for about 100 ms, glaring in a dark
    /// room. Only the console's display may be bright; everything else of the start stays as dark as the fade it replaced.
    /// Missed before: the test only asked whether the glass lit, never how much of the screen was bright.
    #[test]
    fn a_handhelds_start_never_floods_the_screen_with_white_only_the_display_is_bright() {
        crate::cozy_extras::set(crate::cozy_extras::Extras::default());
        crate::motion::set_cozy(true);
        // Past the muted top of a lit display (`MUTED_MAX`, a little of the glass's own bright pixel kept): white.
        let near_white = |p: u32| (p >> 16 & 0xff) > 0xc4 && (p >> 8 & 0xff) > 0xc4 && (p & 0xff) > 0xc4;
        for system in HANDHELDS {
            let (mut on, mut off) = (lit_scene(system, true), lit_scene(system, false));
            for ms in (1200..=1600u64).step_by(20) {
                let elapsed = Duration::from_millis(ms);
                let (mut a, b) = (frame(&mut on, ms, STAGE), frame(&mut off, ms, STAGE));
                let count = |f: &[u32]| f.iter().filter(|p| near_white(**p)).count();
                // The display: what the lit scene paints differently from the dark one. Only it may be brighter than before.
                let glass = (0..a.len()).filter(|&i| a[i] != b[i]).count();
                let painted = count(&a);
                assert!(painted <= count(&b) + glass * 11 / 10 + 50, "{system} {ms} ms: {painted} near-white pixels against {} without the lit display, which is {glass} of {}", count(&b), a.len());
                // And the fade over it only darkens: no light spread over the screen (a console's own pale plastic stays).
                on.veil(&mut Canvas::new(&mut a, 720, 1600), on.fade_at(elapsed));
                assert!(count(&a) <= painted, "{system} {ms} ms: the fade made {} near-white pixels out of {painted}", count(&a));
            }
        }
    }

    /// When the case first visibly opens on the screen, ms after A: the cover's front has lost a share of its width (a
    /// jewel or UMD case), the case has left its place, or the game shows beside it (a cartridge or card sliding out).
    fn visible_open_ms(system: &str, playful: bool) -> Option<u64> {
        // Playful follows Cozy (this thread's), so Cozy off with the scene still Cozy is the plain animation.
        crate::cozy_extras::set(crate::cozy_extras::Extras::default());
        crate::motion::set_cozy(playful);
        let mut s = scene(system, true);
        s.load_rig(2);
        s.game = Some(Rc::new(Pic::new(90, 90, 0xffc0_c0c0)));
        // The case's front is the only bluish thing on the page (dimming keeps its hue); the game is the only light grey.
        let measure = |buf: &[u32]| {
            let (mut front, mut game) = (0usize, 0usize);
            for y in 0..600usize {
                for x in 0..720usize {
                    let p = buf[y * 720 + x];
                    let (r, g, b) = ((p >> 16 & 0xff) as i32, (p >> 8 & 0xff) as i32, (p & 0xff) as i32);
                    if x >= 40 && x < 160 && y >= 40 && y < 220 && b > r + 8 {
                        front += 1;
                    }
                    if (r - 0xc0).abs() < 12 && (g - 0xc0).abs() < 12 && (b - 0xc0).abs() < 12 {
                        game += 1;
                    }
                }
            }
            (front, game)
        };
        let first = measure(&frame(&mut s, 0, STAGE));
        let opened = |s: &mut Scene, ms: u64| {
            let now = measure(&frame(s, ms, STAGE));
            (now.0 as f32) < first.0 as f32 * 0.995 || now.1 > first.1 + 4
        };
        // Coarse 20 ms scan then 2 ms refinement: finds the exact same 2 ms opening timestamp with ~10x fewer renders.
        let hi = (0..1400u64).step_by(20).find(|&ms| opened(&mut s, ms))?;
        (hi.saturating_sub(18)..=hi).step_by(2).find(|&ms| opened(&mut s, ms))
    }

    /// The case's opening sounds when the case opens on the screen: its cover has begun to swing, or its cartridge or card to
    /// slide out. At A it came 0.17 to 0.27 s early (13.11., the PSP's was the one noticed: the console comes up and turns
    /// first). Measured on the painted frames, every 2 ms, against the moment the cue hits.
    #[test]
    fn the_case_sound_comes_when_the_case_visibly_opens_on_every_console() {
        let hit = |system: &str, mechanism: Option<ir::Kind>, playful: bool| {
            let c = &cues(true, Medium::of(system), Game::of(system, "game.rom"), FormFactor::of(system), mechanism, playful)[0];
            (c.after + ui_sound::hit_at(c.sound) + SOUND_LEAD).as_millis() as i64
        };
        let mut failures = Vec::new();
        for system in [
            "psx", "ps2", "saturn", "dreamcast", "gamecube", "psp", "nes", "snes", "n64", "gba", "gb", "gbc", "nds", "3ds", "megadrive", "gamegear",
            "mastersystem", "pcengine", "xbox", "wii",
        ] {
            let mechanism = crate::insert_rig::moving_kind(system, Game::of(system, "x.rom"));
            let seen = visible_open_ms(system, true).expect(system) as i64;
            if (hit(system, mechanism, true) - seen).abs() > 60 {
                failures.push(format!("{system}: sound at {} ms, the case opens at {seen} ms", hit(system, mechanism, true)));
            }
        }
        // Without Playful the plain animation's case (consoles with a cartridge or card, drawn the same way).
        for system in ["nes", "snes", "n64", "gba", "gb", "gbc", "nds", "3ds", "megadrive", "gamegear", "mastersystem", "pcengine"] {
            let seen = visible_open_ms(system, false).expect(system) as i64;
            if (hit(system, None, false) - seen).abs() > 60 {
                failures.push(format!("{system} (plain): sound at {} ms, the case opens at {seen} ms", hit(system, None, false)));
            }
        }
        assert!(failures.is_empty(), "the case sound is not on the case opening:\n{}", failures.join("\n"));
    }

    #[test]
    fn seen_box_row_major_matches_column_scan() {
        let mut base = Pic::new(40, 30, 0);
        let mut over = Pic::new(40, 30, 0);
        base.px[7 * 40 + 5] = 0xff11_2233;
        base.px[21 * 40 + 29] = 0xff44_5566;
        over.px[3 * 40 + 34] = 0xff77_8899;
        over.px[25 * 40 + 12] = 0xffaa_bbcc;
        assert_eq!(seen_box(&base, None), (5, 7, 25, 15));
        assert_eq!(seen_box(&base, Some(&over)), (5, 3, 30, 23));
        assert_eq!(seen_box(&Pic::new(10, 10, 0), None), (0, 0, 10, 10));
    }

    #[test]
    fn paint_dimmed_matches_paint_after_pre_dimming_canvas() {
        crate::cozy_extras::set(crate::cozy_extras::Extras::default());
        crate::motion::set_cozy(true);
        let bg = 0xff3a_4f68u32;
        for system in ["snes", "psx", "gba"] {
            let mut a = scene(system, true);
            let mut b = scene(system, true);
            let blocks = ir::SCALE * (1.7 / 2.5);
            a.load_rig_scaled(blocks);
            b.load_rig_scaled(blocks);
            a.game = Some(Rc::new(Pic::new(80, 90, 0xffaa_6633)));
            b.game = a.game.clone();
            let stage = Stage { floor: 660, k: 1.7 / 2.5 };
            for ms in [0u64, 150, 420, 700, 1050] {
                let elapsed = Duration::from_millis(ms);
                let mut buf_a = vec![bg; 1024 * 768];
                let mut buf_b = vec![bg; 1024 * 768];
                a.paint(&mut Canvas::new(&mut buf_a, 1024, 768), elapsed, None, stage);
                let keep = 256 - (cabinet::insert_at(elapsed, true).dim * 256.0) as u32;
                if keep < 256 {
                    for p in &mut buf_b {
                        *p = ((*p & 0xff00ff) * keep >> 8 & 0xff00ff) | ((*p & 0x00ff00) * keep >> 8 & 0x00ff00);
                    }
                }
                b.paint_dimmed(&mut Canvas::new(&mut buf_b, 1024, 768), elapsed, None, stage);
                assert_eq!(buf_a, buf_b, "{system} at {ms} ms: paint_dimmed differed from paint");
            }
        }
    }

    #[test]
    fn rear_sink_bounds_covers_every_pixel_drawn_by_handheld_sink() {
        crate::cozy_extras::set(crate::cozy_extras::Extras::default());
        crate::motion::set_cozy(true);
        let mut s = scene("gba", true);
        let blocks = ir::SCALE * (1.7 / 2.5);
        s.load_rig_scaled(blocks);
        s.game = Some(Rc::new(Pic::new(80, 90, 0xffcc_3344)));
        let stage = Stage { floor: 660, k: 1.7 / 2.5 };
        // Paint every frame across the rear-sink window at both n=1 (1024x768) and n=2 (640x480) and verify the tight
        // bounding box strictly covers all pixels drawn by the medium and stays inside canvas bounds.
        let pics = s.rig.as_ref().expect("gba rig").clone();
        let (bw, bh) = (pics.base.w, pics.base.h);
        let ox = ((1024 - bw) / 2) as f32;
        let top = stage.floor - bh + pics.foot;
        let beside = s.kind.size_beside(s.medium, s.from.3);
        let (near_w, near_h) = (beside.0 as f32, beside.1 as f32);
        let (fx, fy, fw, fh) = (s.from.0 as f32, s.from.1 as f32, s.from.2 as f32, s.from.3 as f32);
        let mut checked_sink = 0u32;
        for ms in (300..=1100u64).step_by(20) {
            let elapsed = Duration::from_millis(ms);
            let mut buf = vec![0xff20_2530u32; 1024 * 768];
            s.paint(&mut Canvas::new(&mut buf, 1024, 768), elapsed, None, stage);
            let mut small = vec![0xff20_2530u32; 640 * 480];
            s.paint(&mut Canvas::screen(&mut small, (640, 480), 2), elapsed, None, stage);
            // When in the rear-sink window, verify rear_sink_bounds encloses every pixel modified by blitting the medium.
            let st = cabinet::insert_at(elapsed, true);
            let pose = ir::pose_of(pics.rig.kind(), elapsed, false);
            if pose.fly > REAR_SINK_FROM && pose.fly < 1.0 {
                checked_sink += 1;
                let oy = top as f32 + (1.0 - st.rise) * (bh + (160.0 * stage.k) as i32).max(768 - top) as f32 + pose.dip * pics.scale;
                let screen = |p: (f32, f32)| (ox + p.0 * pics.scale, oy + p.1 * pics.scale);
                let start = (fx + fw / 2.0 + pose.out * (fw / 2.0 + near_w * 0.45), fy + fh / 2.0);
                let m = pics.rig.medium_at(start, screen, (near_w, near_h), &pose, pics.scale);
                let edge = m.turn.map(|(angle, full)| {
                    let half = full.0.hypot(full.1).max(0.5);
                    let dir = (full.0 / half, full.1 / half);
                    let d = 2.0 * half * ir::thickness(s.kind);
                    let (sin, cos) = angle.sin_cos();
                    let face = if m.back { d / 2.0 * sin } else { -d / 2.0 * sin };
                    (dir, face, half * cos - d / 2.0 * sin, d * sin)
                });
                let mut probe = vec![0xff10_1010u32; 1024 * 768];
                let mut c = Canvas::new(&mut probe, 1024, 768);
                let (sx0, sy0, sx1, sy1) = rear_sink_bounds(&m, edge, &c).expect("bounds");
                let pic = s.game.as_ref().unwrap();
                let at = edge.map_or(m.at, |(dir, off, _, _)| (m.at.0 + dir.0 * off, m.at.1 + dir.1 * off));
                let o = (at.0 - m.u.0 - m.v.0, at.1 - m.u.1 - m.v.1);
                art::blit_affine_rows(&mut c, pic, o, (m.u.0 / (pic.w as f32 / 2.0), m.u.1 / (pic.w as f32 / 2.0)), (m.v.0 / (pic.h as f32 / 2.0), m.v.1 / (pic.h as f32 / 2.0)), 256, (0, 768), (0.0, pic.h as f32));
                if let Some((dir, _, x0, w)) = edge.filter(|(_, _, _, w)| *w >= 0.5) {
                    let strip = edge_of(pic);
                    let eo = (m.at.0 + dir.0 * x0 - m.v.0, m.at.1 + dir.1 * x0 - m.v.1);
                    art::blit_affine_rows(&mut c, &strip, eo, (dir.0 * w, dir.1 * w), (m.v.0 / (strip.h as f32 / 2.0), m.v.1 / (strip.h as f32 / 2.0)), 256, (0, 768), (0.0, strip.h as f32));
                }
                for y in 0..768usize {
                    for x in 0..1024usize {
                        if probe[y * 1024 + x] != 0xff10_1010 {
                            assert!(x >= sx0 && x < sx1 && y >= sy0 && y < sy1, "ms={ms}: pixel ({x},{y}) outside rear_sink_bounds ({sx0}..{sx1}, {sy0}..{sy1})");
                        }
                    }
                }
            }
        }
        assert!(checked_sink >= 4, "expected rear-sink frames in 300..=1100 ms, saw {checked_sink}");
    }
}
