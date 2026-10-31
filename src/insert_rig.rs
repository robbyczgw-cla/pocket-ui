//! A game going into its console's real mechanism (Cozy + Playful cases, docs/DESIGN.md "Starting a game"): a PS1's lid
//! opens, the disc comes down onto the spindle and the lid shuts; a tray slides out and takes the disc in; a cartridge
//! is pushed into its slot behind the slot's lip and clicks. Systems without a [`Rig`] keep the plain animation
//! (`app_cabinet::paint_insert`), and so does every system in Quick.
//!
//! A rig is pictures made from public-domain photos with PixelLab (`assets/media/rig/SOURCES.md`, never drawn in code),
//! plus where things are on them. [`pose`] is the one timeline, a pure function of the time since A: the medium is in
//! at `cabinet::IN_AT_COZY` with its sound, a disc rests for a moment and the lid or tray shuts [`SHUT_AFTER`] later with
//! the clunk (`Sound::DiscDrop`, `Sound::DiscShut`), and nothing runs past `cabinet::INSERT_COZY`.
//!
//! Pictures and `rig.json` are read once on the animation's first frame and go with it; each frame only maps them.
//! `POCKETUI_RIG_ASSETS=<dir>` reads `<dir>/<system>/` from disk instead of the compiled-in files, for working on a
//! rig without a build.

use std::borrow::Cow;
use std::path::PathBuf;
use std::time::Duration;

use serde::Deserialize;

use crate::cabinet::{ease_in_out, ease_out, IN_AT_COZY};
use crate::media::Game;

/// Screen pixels per pixel of the rigs' art.
pub const SCALE: f32 = 2.0;
/// A disc is down this long before its lid or tray has shut: the pause, then the close.
pub const SHUT_AFTER: Duration = Duration::from_millis(280);
/// The disc lies still on its spindle this long before the lid or tray starts to close.
const REST_MS: f32 = 110.0;

type V = (f32, f32);

fn add(a: V, b: V) -> V {
    (a.0 + b.0, a.1 + b.1)
}
fn sub(a: V, b: V) -> V {
    (a.0 - b.0, a.1 - b.1)
}
fn mul(a: V, k: f32) -> V {
    (a.0 * k, a.1 * k)
}
fn lerp(a: V, b: V, t: f32) -> V {
    add(a, mul(sub(b, a), t))
}
fn cross(a: V, b: V) -> f32 {
    a.0 * b.1 - a.1 * b.0
}

/// A top-loader's lid, turning about its hinge in 3D. All in the art's pixels.
#[derive(Clone, Copy, Debug, Deserialize)]
pub struct Lid {
    /// Where the pictures' top-left corner is on the base: `top.png`, its top side as it lies shut, cut out of the shut
    /// console's picture; `under.png`, its underside, warped to lie where `top` lies (so the same map shows it).
    pub at: V,
    /// The hinge: two points of its line on the base.
    pub hinge: (V, V),
    /// The lid's front as the picture shows it: from the hinge towards the edge that lifts.
    pub front: V,
    /// The same real length straight up, as the picture shows it.
    pub up: V,
    /// How far it opens, degrees past shut.
    pub open_deg: f32,
}

/// Where a lid's picture lands: its pixel (s, t) goes to `o + s·u + t·v` (art pixels).
#[derive(Clone, Copy, Debug)]
pub struct LidMap {
    pub o: V,
    pub u: V,
    pub v: V,
    /// Past standing up: its underside shows.
    pub under: bool,
    /// 0 lying, 1 standing: for its shade.
    pub turned: f32,
}

impl Lid {
    /// The map at `open` (0 shut, 1 `open_deg`).
    pub fn map(self, open: f32) -> LidMap {
        let th = open.max(-0.02) * self.open_deg.to_radians();
        let (sin, cos) = th.sin_cos();
        let a = sub(self.hinge.1, self.hinge.0);
        let f = self.front;
        let f_now = add(mul(f, cos), mul(self.up, sin));
        // Picture direction d = α·a + β·f goes to α·a + β·f_now.
        let det = cross(a, f);
        let map = |d: V| {
            let alpha = cross(d, f) / det;
            let beta = cross(a, d) / det;
            add(mul(a, alpha), mul(f_now, beta))
        };
        let (u, v) = (map((1.0, 0.0)), map((0.0, 1.0)));
        let o = add(self.hinge.0, map(sub(self.at, self.hinge.0)));
        LidMap { o, u, v, under: cross(a, f_now) * det < 0.0, turned: sin.abs() }
    }
}

/// A tray: slides out of the console along a line.
#[derive(Clone, Copy, Debug, Deserialize)]
pub struct Tray {
    /// Where `tray.png`'s top-left corner is on the base, all the way out.
    pub at: V,
    /// From shut to all the way out. Shut, the base's `over` must hide it whole.
    pub out: V,
}

#[derive(Clone, Copy, Debug)]
pub enum Mech {
    Lid(Lid),
    Tray(Tray),
    /// The medium goes in by itself: a cartridge, a card, a slot-loader.
    Fixed,
}

/// Where the medium ends up. All in the art's pixels.
#[derive(Clone, Copy, Debug, Deserialize)]
pub struct Seat {
    /// Its middle when it is in. On a tray: all the way out, and it rides in with the tray.
    pub at: V,
    /// From its middle to the middle of its right edge and of its bottom edge, as the picture shows them lying there.
    pub u: V,
    pub v: V,
    /// Where it hangs, before it goes in.
    pub hover: V,
    /// Already lying as in its seat when it gets to the hover (a disc coming to a tray from the front).
    #[serde(default)]
    pub flat: bool,
    /// The cartridge's or card's slot, two ends on the base: its middle is where the medium's middle goes (tests).
    #[serde(default)]
    pub slot: Option<(V, V)>,
}

#[derive(Clone, Copy, Debug, Deserialize)]
pub struct Led {
    pub at: V,
    pub color: u32,
}

/// How a rig's medium moves.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    /// Comes down onto its seat, a lid shuts over it.
    Lid,
    /// Comes down onto its tray, the tray takes it in.
    Tray,
    /// Pushed in along its own length, clicks.
    Cart,
}

/// `rig.json`: where things are on the pictures.
#[derive(Clone, Copy, Debug, Deserialize)]
pub struct Rig {
    #[serde(flatten)]
    pub mech: MechJson,
    pub seat: Seat,
    /// Goes in behind the whole console (a handheld's slot at its back).
    #[serde(default)]
    pub behind: bool,
    #[serde(default)]
    pub led: Option<Led>,
}

/// `lid` or `tray` in `rig.json`; neither: it goes in by itself.
#[derive(Clone, Copy, Debug, Default, Deserialize)]
pub struct MechJson {
    #[serde(default)]
    pub lid: Option<Lid>,
    #[serde(default)]
    pub tray: Option<Tray>,
}

/// A rig's files: `base` the console without what moves; `over` what is in front of the medium and the tray, the same
/// size as the base and the rest clear; `top` and `under` a lid's two sides or `top` a tray.
pub struct Files {
    pub rig: Rig,
    pub base: Cow<'static, [u8]>,
    pub over: Option<Cow<'static, [u8]>>,
    pub top: Option<Cow<'static, [u8]>>,
    pub under: Option<Cow<'static, [u8]>>,
}

/// The medium now, in screen pixels.
#[derive(Clone, Copy, Debug)]
pub struct MediumAt {
    pub at: V,
    pub u: V,
    pub v: V,
}

impl Rig {
    pub fn kind(&self) -> Kind {
        match self.mech() {
            Mech::Fixed => Kind::Cart,
            Mech::Lid(_) => Kind::Lid,
            Mech::Tray(_) => Kind::Tray,
        }
    }

    pub fn mech(&self) -> Mech {
        match (self.mech.lid, self.mech.tray) {
            (Some(l), _) => Mech::Lid(l),
            (_, Some(t)) => Mech::Tray(t),
            _ => Mech::Fixed,
        }
    }

    /// The medium's screen height lying in its seat as a size to print it at (the larger the two sizes wins).
    pub fn seated_h(&self) -> i32 {
        (2.0 * self.seat.u.0.hypot(self.seat.u.1) * SCALE).ceil() as i32
    }

    /// The tray's picture's top-left corner at `open` (0 in, 1 all the way out).
    pub fn tray_at(&self, t: Tray, open: f32) -> V {
        sub(t.at, mul(t.out, 1.0 - open))
    }

    /// Where the medium is at `pose`: from `start` (screen, face-on, `near` wide and high) to the hover and in.
    pub fn medium_at(&self, start: V, screen: impl Fn(V) -> V, near: V, pose: &Pose) -> MediumAt {
        let s = self.seat;
        let (u1, v1) = (mul(s.u, SCALE), mul(s.v, SCALE));
        // Standing face-on over its place at its final size: a disc as wide as it will be lying.
        let r = u1.0.hypot(u1.1);
        let (u_hover, v_hover) = if self.kind() != Kind::Cart && !s.flat { ((r, 0.0), (0.0, r)) } else { (u1, v1) };
        let (u0, v0) = ((near.0 / 2.0, 0.0), (0.0, near.1 / 2.0));
        let hover = screen(s.hover);
        let (mut at, mut u, mut v) = if pose.fly < 1.0 {
            // A shallow arc on the way.
            let mid = lerp(start, hover, pose.fly);
            let arc = (pose.fly * std::f32::consts::PI).sin() * 24.0;
            (add(mid, (0.0, -arc)), lerp(u0, u_hover, pose.fly), lerp(v0, v_hover, pose.fly))
        } else {
            let tilt = (pose.drop / 0.75).min(1.0);
            (lerp(hover, screen(s.at), pose.drop), lerp(u_hover, u1, tilt), lerp(v_hover, v1, tilt))
        };
        // On a tray it rides with the tray once it is on it.
        if let (Mech::Tray(t), true) = (self.mech(), pose.landed >= 1.0) {
            at = add(at, mul(t.out, -(1.0 - pose.open) * SCALE));
        }
        // After it is in, a small bounce along its way in.
        if pose.settle != 0.0 {
            let dir = sub(screen(s.at), hover);
            let n = dir.0.hypot(dir.1).max(1.0);
            at = add(at, mul(dir, pose.settle * SCALE / n));
        }
        let squash = pose.squash;
        if squash != 0.0 {
            u = mul(u, 1.0 + 0.03 * squash);
            v = mul(v, 1.0 - 0.05 * squash);
        }
        MediumAt { at, u, v }
    }
}

/// Everything that moves, at one moment.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Pose {
    /// The medium coming out of its case: 0 behind it, 1 beside it.
    pub out: f32,
    /// From beside the case to over its place.
    pub fly: f32,
    /// From over its place to in.
    pub drop: f32,
    /// It has touched down.
    pub landed: f32,
    /// A small bounce after that, art pixels along its way in (positive deeper).
    pub settle: f32,
    /// Squashed a little with the click (a cartridge).
    pub squash: f32,
    /// The lid or tray: 0 shut, 1 open.
    pub open: f32,
    /// The console giving under the click, art pixels down.
    pub dip: f32,
    /// The power light: 0 off, 1 on.
    pub led: f32,
    /// A disc's lid has shut over it: nothing of it to draw.
    pub shut_in: bool,
}

fn phase(ms: f32, a: f32, b: f32) -> f32 {
    ((ms - a) / (b - a)).clamp(0.0, 1.0)
}

/// A damped swing starting at 0: `hz` swings a second, dying by e every `decay` ms.
fn swing(ms: f32, hz: f32, decay: f32) -> f32 {
    if ms <= 0.0 {
        return 0.0;
    }
    (-ms / decay).exp() * (std::f32::consts::TAU * hz * ms / 1000.0).sin()
}

/// The shared timeline: where everything is `t` after A. In at `IN_AT_COZY`; a lid or tray shut [`SHUT_AFTER`] later.
pub fn pose(kind: Kind, t: Duration) -> Pose {
    let ms = t.as_secs_f32() * 1000.0;
    let in_at = IN_AT_COZY.as_secs_f32() * 1000.0;
    let out = ease_in_out(phase(ms, 120.0, 420.0));
    let fly = ease_in_out(phase(ms, 420.0, 900.0));
    let after = ms - in_at;
    let led = phase(ms, in_at + 90.0, in_at + 240.0);
    let landed = if ms >= in_at { 1.0 } else { 0.0 };
    if kind == Kind::Cart {
        // Pushed: slowly at first, then home.
        let drop = ease_in_out(phase(ms, 900.0, in_at));
        // Past the seat with the click, a hair back out.
        let settle = 2.0 * swing(after, 5.0, 100.0);
        let squash = 0.9 * (-after.max(0.0) / 85.0).exp() * landed;
        let dip = 2.0 * swing(after, 6.0, 110.0).abs();
        return Pose { out, fly, drop, landed, settle, squash, open: 0.0, dip, led, shut_in: false };
    }
    // Falling: slow off the hover, fast at the end; it rocks on the spindle, then lies still.
    let drop = phase(ms, 900.0, in_at).powi(2);
    let settle = 1.5 * swing(after, 7.0, 45.0).abs();
    let shut = in_at + SHUT_AFTER.as_secs_f32() * 1000.0;
    let opening = ease_out(phase(ms, 360.0, 720.0));
    // Over a little as it stops, as a lid on a hinge does.
    let open_over = 0.04 * swing(ms - 720.0, 2.2, 100.0);
    let begins = in_at + REST_MS;
    let (closing, rebound, dip) = match kind {
        // A lid picks up speed and lands with the clunk, a small rebound.
        Kind::Lid => (phase(ms, begins, shut).powi(2), 0.022 * swing(ms - shut, 6.0, 65.0).abs(), swing(ms - shut, 8.0, 55.0).abs()),
        // A tray is driven: it starts and stops softly.
        _ => (ease_in_out(phase(ms, begins, shut)), 0.0, 0.5 * swing(ms - shut, 8.0, 55.0).abs()),
    };
    let open = opening * (1.0 - closing) + open_over * (1.0 - closing) + rebound;
    Pose { out, fly, drop, landed, settle: -settle, squash: 0.0, open, dip, led, shut_in: ms >= shut }
}

/// The compiled-in rigs by system: its names, `rig.json`, base, over, top, under.
type Row = (&'static [&'static str], &'static [u8], &'static [u8], Option<&'static [u8]>, Option<&'static [u8]>, Option<&'static [u8]>);

macro_rules! bytes {
    ($id:literal, $file:literal) => {
        include_bytes!(concat!("../assets/media/rig/", $id, "/", $file)).as_slice()
    };
}

const RIGS: &[Row] = &[
    (&["psx", "ps1", "playstation"], bytes!("psx", "rig.json"), bytes!("psx", "base.png"), Some(bytes!("psx", "over.png")), Some(bytes!("psx", "top.png")), Some(bytes!("psx", "under.png"))),
    (&["ps2", "playstation2"], bytes!("ps2", "rig.json"), bytes!("ps2", "base.png"), Some(bytes!("ps2", "over.png")), Some(bytes!("ps2", "tray.png")), None),
    (&["gamecube", "gc"], bytes!("gamecube", "rig.json"), bytes!("gamecube", "base.png"), None, Some(bytes!("gamecube", "top.png")), Some(bytes!("gamecube", "under.png"))),
    (&["snes", "sfc", "snesh", "satellaview", "sufami"], bytes!("snes", "rig.json"), bytes!("snes", "base.png"), Some(bytes!("snes", "over.png")), None, None),
    (&["snes-us"], bytes!("snes-us", "rig.json"), bytes!("snes-us", "base.png"), Some(bytes!("snes-us", "over.png")), None, None),
    (&["gba"], bytes!("gba", "rig.json"), bytes!("gba", "base.png"), None, None, None),
    (&["gb"], bytes!("gb", "rig.json"), bytes!("gb", "base.png"), None, None, None),
    (&["n64"], bytes!("n64", "rig.json"), bytes!("n64", "base.png"), Some(bytes!("n64", "over.png")), None, None),
    (&["megadrive", "genesis", "megadrive-japan"], bytes!("megadrive", "rig.json"), bytes!("megadrive", "base.png"), Some(bytes!("megadrive", "over.png")), None, None),
    (&["dreamcast"], bytes!("dreamcast", "rig.json"), bytes!("dreamcast", "base.png"), Some(bytes!("dreamcast", "over.png")), Some(bytes!("dreamcast", "top.png")), Some(bytes!("dreamcast", "under.png"))),
    (&["saturn"], bytes!("saturn", "rig.json"), bytes!("saturn", "base.png"), Some(bytes!("saturn", "over.png")), Some(bytes!("saturn", "top.png")), Some(bytes!("saturn", "under.png"))),
    (&["xbox"], bytes!("xbox", "rig.json"), bytes!("xbox", "base.png"), Some(bytes!("xbox", "over.png")), Some(bytes!("xbox", "tray.png")), None),
    (&["nds"], bytes!("nds", "rig.json"), bytes!("nds", "base.png"), None, None, None),
    (&["wii"], bytes!("wii", "rig.json"), bytes!("wii", "base.png"), Some(bytes!("wii", "over.png")), None, None),
];

/// A rig from `POCKETUI_RIG_ASSETS` (module doc).
fn from_disk(system: &str) -> Option<Files> {
    let dir = PathBuf::from(std::env::var_os("POCKETUI_RIG_ASSETS")?).join(system);
    let read = |f: &str| std::fs::read(dir.join(f)).ok().map(Cow::Owned);
    let json = read("rig.json")?;
    Some(Files { rig: serde_json::from_slice(&json).ok()?, base: read("base.png")?, over: read("over.png"), top: read("top.png").or_else(|| read("tray.png")), under: read("under.png") })
}

/// Which rig a game goes into: a US cartridge fits the US console only.
fn rig_id(system: &str, game: Game) -> &str {
    if game == Game::SnesUs {
        "snes-us"
    } else {
        system
    }
}

/// A game's rig has a lid or tray to shut after the disc (the launch splits its sound for that).
pub fn takes_disc(system: &str, game: Game) -> bool {
    files(system, game).is_some_and(|f| f.rig.kind() != Kind::Cart)
}

/// A game's rig, if its console has one.
pub fn files(system: &str, game: Game) -> Option<Files> {
    let id = rig_id(system, game);
    if let Some(f) = from_disk(id) {
        return Some(f);
    }
    let r = RIGS.iter().find(|r| r.0.contains(&id))?;
    Some(Files { rig: serde_json::from_slice(r.1).ok()?, base: Cow::Borrowed(r.2), over: r.3.map(Cow::Borrowed), top: r.4.map(Cow::Borrowed), under: r.5.map(Cow::Borrowed) })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cabinet::INSERT_COZY;

    fn at(kind: Kind, ms: u64) -> Pose {
        pose(kind, Duration::from_millis(ms))
    }

    const KINDS: [Kind; 3] = [Kind::Lid, Kind::Tray, Kind::Cart];

    #[test]
    fn the_medium_is_in_with_its_sound_rests_and_a_lid_shuts_with_the_clunk() {
        let in_ms = IN_AT_COZY.as_millis() as u64;
        for kind in KINDS {
            assert!(at(kind, in_ms - 40).landed == 0.0 && at(kind, in_ms).landed == 1.0, "{kind:?}");
            assert_eq!(at(kind, in_ms).drop, 1.0, "{kind:?}");
        }
        let shut = in_ms + SHUT_AFTER.as_millis() as u64;
        for kind in [Kind::Lid, Kind::Tray] {
            assert!(at(kind, in_ms - 300).open > 0.95, "{kind:?}: open before the disc is down");
            assert!(at(kind, in_ms + 60).open > 0.95, "{kind:?}: still open while the disc rests");
            assert!(at(kind, shut).open.abs() < 0.01, "{kind:?}: {:?}", at(kind, shut));
        }
        // The launch fade runs after the clunk, and the game's window comes when the fade is done (`launch_fade_at`).
        let fade_starts = crate::cabinet::FADE_AT_COZY.as_millis() as u64 + 250 * 55 / 100;
        assert!(fade_starts >= shut, "{fade_starts} {shut}");
    }

    #[test]
    fn everything_is_still_by_the_end_of_the_animation() {
        let end = INSERT_COZY.as_millis() as u64;
        for kind in KINDS {
            let p = at(kind, end);
            assert!(p.open.abs() < 0.002 && p.settle.abs() < 0.05 && p.dip.abs() < 0.05 && p.squash.abs() < 0.01, "{kind:?}: {p:?}");
            assert_eq!(p.led, 1.0);
        }
    }

    #[test]
    fn the_medium_moves_only_one_way_until_it_is_in() {
        for kind in KINDS {
            let mut last = Pose::default();
            for ms in (0..=IN_AT_COZY.as_millis() as u64).step_by(8) {
                let p = at(kind, ms);
                assert!(p.out >= last.out && p.fly >= last.fly && p.drop >= last.drop, "{kind:?} at {ms}");
                last = p;
            }
        }
    }

    #[test]
    fn a_cartridge_sits_in_the_middle_of_its_slot() {
        for r in RIGS {
            let f = files(r.0[0], Game::Disc).expect(r.0[0]);
            if f.rig.kind() != Kind::Cart {
                continue;
            }
            let s = f.rig.seat;
            let (a, b) = s.slot.unwrap_or_else(|| panic!("{}: its slot is measured in rig.json", r.0[0]));
            let mid = lerp(a, b, 0.5);
            // Along the slot, from its middle to the medium's: under a pixel of the art.
            let axis = sub(b, a);
            let off = ((s.at.0 - mid.0) * axis.0 + (s.at.1 - mid.1) * axis.1) / axis.0.hypot(axis.1);
            assert!(off.abs() <= 1.0, "{}: {off:.2} art pixels off the slot's middle", r.0[0]);
            // And it fills no more than its slot.
            assert!(s.u.0.hypot(s.u.1) * 2.0 <= axis.0.hypot(axis.1) + 2.0, "{}: wider than its slot", r.0[0]);
        }
    }

    /// All of a rig's pictures in memory at once, at the size they are held.
    fn bytes_held(f: &Files) -> usize {
        let px = |d: &Option<Cow<'static, [u8]>>, k: usize| d.as_ref().and_then(|d| crate::system_art::decode(d)).map_or(0, |s| (s.w * s.h) as usize * 4 * k);
        px(&Some(f.base.clone()), 4) + px(&f.over, 4) + px(&f.top, 1) + px(&f.under, 1)
    }

    #[test]
    fn every_rig_reads_and_stays_small_and_real() {
        for r in RIGS {
            let f = files(r.0[0], Game::Disc).expect(r.0[0]);
            let base = crate::system_art::decode(&f.base).expect("base");
            for over in f.over.iter() {
                let o = crate::system_art::decode(over).expect("over");
                assert_eq!((o.w, o.h), (base.w, base.h), "{}: over has the base's size", r.0[0]);
            }
            assert!(bytes_held(&f) <= 6 << 20, "{}: {} bytes", r.0[0], bytes_held(&f));
            match f.rig.mech() {
                Mech::Lid(_) => assert!(f.top.is_some() && f.under.is_some(), "{}", r.0[0]),
                Mech::Tray(_) => assert!(f.top.is_some(), "{}", r.0[0]),
                Mech::Fixed => {}
            }
            // It lies inside the picture (a tray's: all the way out).
            let s = f.rig.seat;
            assert!(s.at.0 > 0.0 && s.at.0 < base.w as f32 && s.at.1 > 0.0 && s.at.1 < base.h as f32 + 200.0, "{}", r.0[0]);
        }
    }

    #[test]
    fn a_shut_lid_is_exactly_where_its_picture_is() {
        let lid = Lid { at: (10.0, 20.0), hinge: ((40.0, 20.0), (90.0, 50.0)), front: (-30.0, 40.0), up: (0.0, -55.0), open_deg: 80.0 };
        let m = lid.map(0.0);
        for (a, b) in [(m.o, (10.0, 20.0)), (m.u, (1.0, 0.0)), (m.v, (0.0, 1.0))] {
            assert!((a.0 - b.0).abs() < 0.001 && (a.1 - b.1).abs() < 0.001, "{m:?}");
        }
        assert!(!m.under);
        // Standing past upright its underside shows and it is above the hinge.
        let m = lid.map(1.0);
        assert!(m.turned > 0.9);
    }
}
