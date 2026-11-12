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

/// How long after A the game's window may come up: Cozy holds it until the medium is in and the fade has run, the quick
/// way doesn't hold it at all.
pub fn hold(cozy: bool) -> Option<Duration> {
    cozy.then(|| cabinet::fade_delay(true) + FADE)
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
    pub fn new(cozy: bool) -> HandOver {
        HandOver { hold: hold(cozy), released: false, skipped: false }
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

/// The sounds of a start, all queued at A with their waits: the sound thread plays them on time. Quick: a quiet click as
/// it goes in (`cabinet::IN_AT`). Cozy: the case opens, and the game goes into the console with its own sound, seated as
/// it is in (`IN_AT_COZY`). A rig's disc (`disc_rig`): down on its spindle, a pause, and the lid or tray shuts with the
/// rest of the sound; a front-loader's cartridge clicks with its own sound and the door shuts with that clunk.
pub fn cues(cozy: bool, medium: Medium, disc_rig: bool) -> Vec<Cue> {
    let in_at = cabinet::in_at(cozy).saturating_sub(SOUND_LEAD);
    let at = |sound: Sound, moment: Duration, db: f32| Cue { sound, after: moment.saturating_sub(ui_sound::hit_at(sound)), db };
    if !cozy {
        return vec![at(Sound::Insert, in_at, ui_sound::QUICK_DB)];
    }
    let opening = Cue { sound: ui_sound::opening(medium), after: Duration::ZERO, db: 0.0 };
    if disc_rig {
        // A front-loader's cartridge clicks as itself; its door drops shut with the lid's clunk.
        let down = if medium.game().is_round() { Sound::DiscDrop } else { ui_sound::going_in(medium) };
        vec![opening, at(down, in_at, 0.0), at(Sound::DiscShut, in_at + ir::SHUT_AFTER, 0.0)]
    } else {
        vec![opening, at(ui_sound::going_in(medium), in_at, 0.0)]
    }
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
    // The arcade cabinet: no medium goes in (`lights_up_only`), its screen lights up.
    ("arcade", 0.45, 0.38, 1.0),
    // The crate of games: where a system without a picture of its own puts its medium (`STAND_IN`).
    ("ports", 0.50, 0.40, 0.80),
];

/// What stands in for the console of a system without a picture: the crate of games, its medium goes into it.
pub const STAND_IN: &str = "ports";

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
        )
}

/// A rig's pictures, ready to paint: the console's at screen size, what moves at its own (drawn through a map).
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
    /// A cartridge's edge, one column of its own plastic (`edge_of`), made the first time it turns and kept.
    pub edge: std::cell::OnceCell<Pic>,
}

impl Rigged {
    /// `blocks`: screen pixels per pixel of the art (`ir::SCALE` on Linux).
    pub fn load(system: &str, game: Game, blocks: i32) -> Option<Rigged> {
        let f = ir::files(system, game)?;
        let pic = |data: &[u8]| {
            let s = crate::system_art::decode(data)?;
            Some(Pic { w: s.w as i32, h: s.h as i32, px: s.px })
        };
        let (mut base, over) = (enlarged(pic(&f.base)?, blocks), f.over.and_then(|d| pic(&d)).map(|p| enlarged(p, blocks)));
        // What `over` covers whole is never seen on the base: each frame only copies a console's pixels once.
        if let Some(o) = over.as_ref().filter(|o| (o.w, o.h) == (base.w, base.h)) {
            for (b, t) in base.px.iter_mut().zip(&o.px) {
                if t >> 24 == 255 {
                    *b = 0;
                }
            }
        }
        Some(Rigged { rig: f.rig, front: f.front.and_then(|d| pic(&d)), base, over, top: f.top.and_then(|d| pic(&d)), under: f.under.and_then(|d| pic(&d)), scale: blocks as f32, edge: Default::default() })
    }
}

/// The console's picture at (`x`, `y`) painted again over the screen rows `rows`, solid pixels only: what the medium
/// sinks behind is the console's outline, whatever its tilt.
fn paint_over(c: &mut Canvas, sp: &crate::system_art::Sprite, (x, y): (i32, i32), rows: (i32, i32)) {
    for row in rows.0.max(y).max(0)..rows.1.min(y + sp.h as i32).min(c.h) {
        let src = &sp.px[((row - y) as u32 * sp.w) as usize..][..sp.w as usize];
        for (i, &p) in src.iter().enumerate() {
            let col = x + i as i32;
            if p >> 24 == 255 && col >= 0 && col < c.w {
                c.buf[(row * c.w + col) as usize] = p & 0xffffff;
            }
        }
    }
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
    Pic { w, h, px }
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
    Pic { w: 1, h: p.h, px }
}

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
    for yy in y.max(0)..(y + h).min(c.h) {
        for xx in x.max(0)..(x + w).min(c.w) {
            let d = &mut c.buf[(yy * c.w + xx) as usize];
            *d = mix(*d & 0xffffff, rgb, 255 - (keep * 255 / 256));
        }
    }
}

/// How much of the case still shows, 1-0: it fades while the medium leaves it.
fn case_shown(st: cabinet::Insert) -> f32 {
    1.0 - ((st.carry - 0.3) / 0.4).clamp(0.0, 1.0)
}

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
}

impl Scene {
    /// How far the launch fade is, `elapsed` after A, 0-1: it waits for the medium to go in, and a rig's lid or tray shuts
    /// first, so its fade keeps its end and runs after the clunk.
    pub fn fade_at(&self, elapsed: Duration) -> f32 {
        let p = (elapsed.saturating_sub(cabinet::fade_delay(self.cozy)).as_secs_f32() / FADE.as_secs_f32()).min(1.0);
        self.rig_fade(p)
    }

    /// How much of the case still shows, `elapsed` after A: what it casts on the page goes with it.
    pub fn case_shown(&self, elapsed: Duration) -> f32 {
        match self.returning() {
            true => 1.0 - case_back(elapsed, self.opens()).1,
            false => case_shown(cabinet::insert_at(elapsed, self.cozy)),
        }
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
    fn draw_case(&self, c: &mut Canvas, elapsed: Duration, cover: Option<f32>, fade: u32, dim: f32, clip: (i32, i32)) -> bool {
        let (front, from) = (&self.front, self.from);
        let (fx, fy, fw, fh) = (from.0 as f32, from.1 as f32, front.w as f32, front.h as f32);
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

    /// The fade `p` as it shows with this scene's rig.
    pub fn rig_fade(&self, p: f32) -> f32 {
        if self.rig.is_some() {
            ((p - 0.55) / 0.45).clamp(0.0, 1.0)
        } else {
            p
        }
    }

    /// Asks for the console's own mechanism, once: Cozy + Playful, and the SP's bottom slot in every mode.
    pub fn load_rig(&mut self, blocks: i32) {
        if !self.rig_asked {
            if ir::label_away(&self.system) {
                self.game_back = crate::media::game_back(self.kind).map(|p| Rc::new(p.clone()));
            }
            // The SP also needs its bottom-slot geometry in Quick and without playful motion.
            let sp = crate::system_art::picture_id(&self.system) == "gba-sp";
            self.rig = ((self.cozy && crate::playful::on()) || sp).then(|| Rigged::load(&self.system, self.kind, blocks)).flatten().map(Rc::new);
            self.rig_asked = true;
        }
    }

    /// One moment of it, `elapsed` after A, over what is on `c`. `sprite` is the console's picture (`fitted` to the stage)
    /// for the plain animation.
    pub fn paint(&mut self, c: &mut Canvas, elapsed: Duration, sprite: Option<&Sprite>, stage: Stage) {
        if self.rig.is_some() {
            return self.paint_rig(c, elapsed, stage);
        }
        self.paint_plain(c, elapsed, sprite, stage);
    }

    /// Every system without a rig, and every system in Quick: the console comes up from below and the medium goes in.
    fn paint_plain(&mut self, c: &mut Canvas, elapsed: Duration, sprite: Option<&Sprite>, Stage { floor, k }: Stage) {
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
        art::dim(c, 0, c.h, 256 - (st.dim * 256.0) as u32);
        // The console, up from below the screen.
        let (sw, sh) = sprite.as_ref().map_or((px(360), px(120)), |s| (s.w as i32, s.h as i32));
        let base = floor - px(10);
        let sx = (c.w - sw) / 2;
        let sy_final = base - sh;
        // From wholly under the screen's edge, also on a tall phone held upright (its floor is high up the screen).
        let sy = sy_final + ((1.0 - st.rise) * (sh + px(160)).max(c.h - sy_final) as f32) as i32;
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
        self.draw_case(c, elapsed, None, case_fade, st.dim, (0, c.h));
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
    fn paint_rig(&mut self, c: &mut Canvas, elapsed: Duration, Stage { floor, k }: Stage) {
        use ir::Mech;
        let Some(pics) = self.rig.clone() else { return };
        let (front, from, kind, medium) = (self.front.clone(), self.from, self.kind, self.medium);
        let Some(pic) = self.game.clone() else { return };
        let rig = pics.rig;
        let scale = pics.scale;
        let st = cabinet::insert_at(elapsed, self.cozy);
        let mechanism_time = if self.cozy { elapsed } else { elapsed.mul_f32(cabinet::IN_AT_COZY.as_secs_f32() / cabinet::IN_AT.as_secs_f32()) };
        let pose = ir::pose(rig.kind(), mechanism_time);
        art::dim(c, 0, c.h, 256 - (st.dim * 256.0) as u32);
        let (bw, bh) = (pics.base.w, pics.base.h);
        let px = |n: i32| (n as f32 * k) as i32;
        let ox = ((c.w - bw) / 2) as f32;
        let top = floor - px(10) - bh;
        // From wholly under the screen's edge, also on a tall phone held upright (its floor is high up the screen).
        let oy = top as f32 + (1.0 - st.rise) * (bh + px(160)).max(c.h - top) as f32 + pose.dip * scale;
        let shade = 256 - ((1.0 - st.rise) * 200.0) as u32;
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
        if edge.is_some() {
            pics.edge.get_or_init(|| edge_of(&pic));
        }
        let draw_part = |c: &mut Canvas, shade: u32, rows: (f32, f32)| {
            let face_pic = if m.back { self.game_back.as_deref().unwrap_or(&pic) } else { &pic };
            let (sx, sy) = (face_pic.w as f32 / 2.0, face_pic.h as f32 / 2.0);
            let at = edge.map_or(m.at, |(dir, off, _, _)| (m.at.0 + dir.0 * off, m.at.1 + dir.1 * off));
            let o = (at.0 - m.u.0 - m.v.0, at.1 - m.u.1 - m.v.1);
            let h = face_pic.h as f32;
            art::blit_affine_rows(c, face_pic, o, (m.u.0 / sx, m.u.1 / sx), (m.v.0 / sy, m.v.1 / sy), shade, all, (rows.0 * h, rows.1 * h));
            if let (Some((dir, _, x0, w)), Some(strip)) = (edge, pics.edge.get()) {
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
            if flying {
                draw_medium(c);
            }
            return;
        }
        if rig.behind && flying {
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
            let inside = flying && !pose.shut_in;
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
        self.draw_case(c, elapsed, opens.then_some(pose.out), case_fade, st.dim, all);
        // A disc coming down is in front of everything; once it is down the parts in front of it hide it.
        let coming_down = rig.kind() != ir::Kind::Cart && pose.landed < 1.0;
        if flying && !rig.behind {
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
        }
        if flying && !rig.behind && coming_down {
            draw_medium(c);
        }
        if let Mech::Lid(l) = rig.mech() {
            if lid_later && !lid_behind {
                draw_lid(c, l, &pics, (ox, oy), pose.open, shade);
            }
        }
        // The console takes it: its power light comes on.
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
    // The floor is 10 handheld pixels under the console's bottom (`paint_rig`); the medium's top at least 16 down.
    let lowest_top = 16 + (tall * blocks as f32) as i32 + (10.0 * k) as i32;
    (h - gap).max(lowest_top).min(h)
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
    let top = stage.floor - (10.0 * stage.k) as i32 - bh;
    (((w - bw) / 2) as f32 + rig.seat.hover.0 * blocks as f32, top as f32 + rig.seat.hover.1 * blocks as f32)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scene(system: &str, cozy: bool) -> Scene {
        let hardware = if system == "gba-sp" { "gba" } else { system };
        let kind = Game::of(hardware, "x.rom");
        let medium = Medium::of(hardware);
        let look = Look { color: 0x8040c0, band: 0x8040c0, plastic: art::plastic(system) };
        let front = Rc::new(Pic::new(120, 180, 0xff40_6080));
        let game = Rc::new(Pic::new(90, 90, 0xffc0_c0c0));
        Scene { system: system.into(), ff: FormFactor::of(hardware), medium, look, title: "T".into(), kind, has_cover: false, front, from: (40, 40, 120, 180), game: Some(game), game_back: None, cozy, at_drive: None, rig: None, rig_asked: false, home: None }
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
        let r = Rc::get_mut(s.rig.as_mut().expect("SP hardware")).unwrap();
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
    fn motion_off_or_a_frozen_game_is_the_quick_way_with_no_hold_and_one_quiet_click() {
        use crate::motion::Motion::{Full, Off, Reduced};
        for (motion, frozen) in [(Off, false), (Reduced, false), (Full, true)] {
            let cozy = cabinet::starts_cozy(true, motion, frozen);
            assert!(!cozy, "{motion:?} {frozen}");
            assert_eq!(hold(cozy), None);
            let mut s = scene("psx", cozy);
            s.load_rig(2);
            assert!(s.rig.is_none());
            assert_eq!(cues(cozy, Medium::of("psx"), false).iter().map(|c| c.sound).collect::<Vec<_>>(), [Sound::Insert]);
        }
        assert!(cabinet::starts_cozy(true, Full, false));
    }

    #[test]
    fn the_window_is_held_until_the_fade_has_run_and_the_fade_ends_black() {
        assert_eq!(hold(true), Some(Duration::from_millis(1600)));
        for system in ["psx", "psp"] {
            let mut s = scene(system, true);
            s.load_rig(2);
            assert_eq!(s.fade_at(cabinet::IN_AT_COZY), 0.0, "{system}");
            assert_eq!(s.fade_at(hold(true).unwrap()), 1.0, "{system}");
        }
    }

    #[test]
    fn a_skip_brings_the_game_at_once_and_only_once() {
        let at = |ms| Duration::from_millis(ms);
        let mut cozy = HandOver::new(true);
        assert!(!cozy.take_release(at(0)) && !cozy.take_release(at(1599)), "held while the animation plays");
        assert!(cozy.skip() && cozy.skipped());
        assert!(cozy.take_release(at(300)), "a skip ends the hold");
        assert!(!cozy.take_release(at(301)) && !cozy.skip(), "once");
        let mut played = HandOver::new(true);
        assert!(played.take_release(at(1600)) && played.released());
        // Quick, Motion below Full, a frozen game: nothing to wait for.
        assert!(HandOver::new(false).take_release(at(0)));
    }

    #[test]
    fn each_sound_hits_on_its_moment_of_the_picture() {
        let hit = |c: &Cue| c.after + ui_sound::hit_at(c.sound) + SOUND_LEAD;
        let disc = cues(true, Medium::of("psx"), true);
        assert_eq!(disc.iter().map(|c| c.sound).collect::<Vec<_>>(), [Sound::CaseOpen, Sound::DiscDrop, Sound::DiscShut]);
        assert_eq!(hit(&disc[1]), cabinet::IN_AT_COZY);
        assert_eq!(hit(&disc[2]), cabinet::IN_AT_COZY + ir::SHUT_AFTER);
        let cart = cues(true, Medium::of("snes"), false);
        assert_eq!(cart[0].sound, ui_sound::opening(Medium::of("snes")));
        assert_eq!((cart[1].sound, hit(&cart[1])), (Sound::CartIn, cabinet::IN_AT_COZY));
        // The plain animation's disc keeps the whole sound, on the same moment.
        let plain = cues(true, Medium::of("psp"), false);
        assert_eq!(hit(&plain[1]), cabinet::IN_AT_COZY);
        let quick = cues(false, Medium::of("psx"), false);
        assert_eq!((hit(&quick[0]), quick[0].db), (cabinet::IN_AT, ui_sound::QUICK_DB));
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
                let top = floor - (10.0 * k) as i32 - base.1 * n;
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

    #[test]
    fn a_bigger_phone_only_scales_the_handhelds_picture() {
        // At the handhelds' 2 the blocks are the art's old doubling.
        let p = Pic { w: 2, h: 1, px: vec![1, 2] };
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
}
