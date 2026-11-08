//! A game going into its console's real mechanism (Cozy + Playful cases, docs/DESIGN.md "Starting a game"): a PS1's lid
//! opens, the disc comes down onto the spindle and the lid shuts; a tray slides out and takes the disc in; a cartridge
//! is pushed into its slot behind the slot's lip and clicks. Systems without a [`Rig`] keep the plain animation
//! (`insert_scene`), and so does every system in Quick.
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
    /// The hinge sits this far (art pixels, straight down the picture) behind the console's rear wall: what of the lid is
    /// behind the hinge goes down behind that wall as the lid opens (a real lid is hinged under its rear lip, so it stands
    /// up out of the console instead of floating above it). Needs the lid drawn behind the console while it is open.
    #[serde(default)]
    pub tuck: f32,
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
        let hinge = (add(self.hinge.0, (0.0, self.tuck)), add(self.hinge.1, (0.0, self.tuck)));
        let a = sub(hinge.1, hinge.0);
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
        let o = add(hinge.0, map(sub(self.at, hinge.0)));
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
    /// Front-view handheld: the label turns away before entering its rear slot.
    #[serde(default)]
    pub label_away: bool,
    /// A clamshell's cartridge travels in from the lower edge, connector first.
    #[serde(default)]
    pub bottom_slot: bool,
    /// SP: the screen opens before insertion and stays open; independent of a disc mechanism.
    #[serde(default)]
    pub clamshell: Option<Lid>,
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
    /// The visible face is the cartridge's back, with its label facing away.
    pub back: bool,
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
    pub fn seated_h(&self, scale: f32) -> i32 {
        (2.0 * self.seat.u.0.hypot(self.seat.u.1) * scale).ceil() as i32
    }

    /// The tray's picture's top-left corner at `open` (0 in, 1 all the way out).
    pub fn tray_at(&self, t: Tray, open: f32) -> V {
        sub(t.at, mul(t.out, 1.0 - open))
    }

    /// Where the medium is at `pose`: from `start` (screen, face-on, `near` wide and high) to the hover and in. `scale`:
    /// screen pixels per pixel of the art ([`SCALE`] on the handhelds, where the arc's 24 pixels were measured).
    pub fn medium_at(&self, start: V, screen: impl Fn(V) -> V, near: V, pose: &Pose, scale: f32) -> MediumAt {
        let s = self.seat;
        let (u1, v1) = (mul(s.u, scale), mul(s.v, scale));
        // Over its place it already lies as it will lie in its seat: a disc is carried flat, label up, and lowered onto
        // its spindle or tray (the consoles' manuals), never held upright over it. It turns from face-on beside its case
        // to lying on the way there.
        let (u_hover, v_hover) = (u1, v1);
        let (u0, v0) = ((near.0 / 2.0, 0.0), (0.0, near.1 / 2.0));
        let hover = screen(s.hover);
        let (mut at, mut u, mut v) = if pose.fly < 1.0 {
            // A shallow arc on the way.
            let mid = lerp(start, hover, pose.fly);
            let arc = (pose.fly * std::f32::consts::PI).sin() * 24.0 * (scale / SCALE);
            (add(mid, (0.0, -arc)), lerp(u0, u_hover, pose.fly), lerp(v0, v_hover, pose.fly))
        } else {
            let tilt = (pose.drop / 0.75).min(1.0);
            (lerp(hover, screen(s.at), pose.drop), lerp(u_hover, u1, tilt), lerp(v_hover, v1, tilt))
        };
        if self.bottom_slot && pose.fly < 1.0 {
            // Rotate both axes together to put the connector edge towards the SP.
            // Directly lerping opposite axes would collapse the cartridge through its centre.
            let angle = u1.1.atan2(u1.0);
            let rotate = |p: V, a: f32| { let (sn, cs) = a.sin_cos(); (p.0 * cs - p.1 * sn, p.0 * sn + p.1 * cs) };
            u = rotate(lerp(u0, rotate(u1, -angle), pose.fly), angle * pose.fly);
            v = rotate(lerp(v0, rotate(v1, -angle), pose.fly), angle * pose.fly);
        }
        // On a tray it rides with the tray once it is on it.
        if let (Mech::Tray(t), true) = (self.mech(), pose.landed >= 1.0) {
            at = add(at, mul(t.out, -(1.0 - pose.open) * scale));
        }
        // After it is in, a small bounce along its way in.
        if pose.settle != 0.0 {
            let dir = sub(screen(s.at), hover);
            let n = dir.0.hypot(dir.1).max(1.0);
            at = add(at, mul(dir, pose.settle * scale / n));
        }
        let squash = pose.squash;
        if squash != 0.0 {
            u = mul(u, 1.0 + 0.03 * squash);
            v = mul(v, 1.0 - 0.05 * squash);
        }
        let (width, back) = face_at(self.label_away, pose.fly);
        u = mul(u, width);
        MediumAt { at, u, v, back }
    }
}

/// The cartridge leaves its case label-first, then turns around its upright axis on the
/// flight to a rear slot. The back image has its own handedness; never mirror either face.
/// This also drives the plain launch (GBC and Quick), so they cannot retain the old label.
pub fn face_at(away: bool, fly: f32) -> (f32, bool) {
    if !away {
        return (1.0, false);
    }
    let turn = ((fly - 0.2) / 0.6).clamp(0.0, 1.0);
    ((turn * std::f32::consts::PI).cos().abs(), turn > 0.5)
}

/// These handhelds are viewed from their screen side, with a cartridge slot behind them.
pub fn label_away(system: &str) -> bool {
    matches!(system, "gb" | "gbh" | "gbc" | "gbch" | "gba" | "gbah" | "gba-sp" | "nds" | "ndsh")
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
    (&["gba-sp"], bytes!("gba-sp", "rig.json"), bytes!("gba-sp", "base.png"), Some(bytes!("gba-sp", "over.png")), Some(bytes!("gba-sp", "top.png")), Some(bytes!("gba-sp", "under.png"))),
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
        crate::system_art::picture_id(system)
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
            let off = if f.rig.bottom_slot {
                // The SP's slanted photo projects depth obliquely: follow the cartridge's
                // depth axis to the mouth, rather than assuming perpendicular screen axes.
                cross(sub(s.at, mid), s.v) / cross(axis, s.v) * axis.0.hypot(axis.1)
            } else {
                ((s.at.0 - mid.0) * axis.0 + (s.at.1 - mid.1) * axis.1) / axis.0.hypot(axis.1)
            };
            assert!(off.abs() <= 1.0, "{}: {off:.2} art pixels off the slot's middle", r.0[0]);
            // And it fills no more than its slot.
            assert!(s.u.0.hypot(s.u.1) * 2.0 <= axis.0.hypot(axis.1) + 2.0, "{}: wider than its slot", r.0[0]);
        }
    }

    #[test]
    fn every_insert_uses_the_real_slot_direction_and_label_side() {
        // Independently reviewed hardware expectations: screen coordinates of travel from hover
        // to seat, rear occlusion, and which face a player in front of the console sees.
        let expected = [
            ("psx", Kind::Lid, false, (0, 1), false),
            ("ps2", Kind::Tray, false, (1, -1), false),
            ("gamecube", Kind::Lid, false, (-1, 1), false),
            ("snes", Kind::Cart, false, (0, 1), false),
            ("snes-us", Kind::Cart, false, (0, 1), false),
            ("gba", Kind::Cart, true, (-1, 1), true),
            ("gba-sp", Kind::Cart, false, (1, -1), true),
            ("gb", Kind::Cart, true, (-1, 1), true),
            ("n64", Kind::Cart, false, (0, 1), false),
            ("megadrive", Kind::Cart, false, (0, 1), false),
            ("dreamcast", Kind::Lid, false, (0, 1), false),
            ("saturn", Kind::Lid, false, (0, 1), false),
            ("xbox", Kind::Tray, false, (1, -1), false),
            ("nds", Kind::Cart, true, (-1, 1), true),
            ("wii", Kind::Cart, false, (1, 1), false),
        ];
        assert_eq!(expected.len(), RIGS.len(), "review hardware expectations for new rigs");
        for (system, kind, behind, direction, back) in expected {
            let game = if system == "snes-us" { Game::SnesUs } else { Game::of(system, "test.rom") };
            let f = files(system, game).expect(system);
            let r = f.rig;
            assert_eq!(r.kind(), kind, "{system}: mechanism");
            assert_eq!(r.behind, behind, "{system}: slot at the back versus on the top/front");
            if system == "gba-sp" {
                let lid = r.clamshell.expect("SP screen must open before its cartridge enters");
                assert!(f.top.is_some() && f.under.is_some(), "SP needs both photographed faces of its screen panel");
                // Both panel PNGs share the lower shell's canvas. Through the whole opening,
                // the photographed hinge endpoints must remain attached to the lower shell.
                for opening in [0.0, 0.25, 0.5, 0.75, 1.0] {
                    let m = lid.map(opening);
                    for h in [lid.hinge.0, lid.hinge.1] {
                        let p = add(m.o, add(mul(m.u, h.0), mul(m.v, h.1)));
                        assert!(sub(p,h).0.hypot(sub(p,h).1) < 0.01, "SP screen detaches from its hinge at {opening}");
                    }
                }
            }
            if let Some((a, b)) = r.seat.slot {
                let base = crate::system_art::decode(&f.base).unwrap();
                let y = (a.1 + b.1) / 2.0 / base.h as f32;
                let range = match system {
                    "gb" => 0.0..0.12,
                    "gba" => 0.0..0.30,
                    "gba-sp" => 0.65..0.90, // lower front edge; label faces the underside
                    "nds" => 0.35..0.65, // rear of the lower half, beside the hinge
                    "wii" => 0.30..0.65, // vertical slot on the front face
                    _ => 0.10..0.55, // cartridge slot on the console's upper surface
                };
                assert!(range.contains(&y), "{system}: slot must stay on its physical side of the photographed console ({y})");
            }
            let mut previous = r.medium_at((0.0, 0.0), |p| p, (90.0, 90.0), &at(kind, 900), 1.0).at;
            for ms in (920..=1200).step_by(20) {
                let m = r.medium_at((0.0, 0.0), |p| p, (90.0, 90.0), &at(kind, ms), 1.0);
                let delta = sub(m.at, previous);
                for (d, sign) in [(delta.0, direction.0), (delta.1, direction.1)] {
                    assert!(if sign == 0 { d.abs() < 0.01 } else { d * sign as f32 > 0.0 }, "{system} at {ms}: travelling {delta:?}, expected {direction:?}");
                }
                assert_eq!(m.back, back, "{system} at {ms}: label must face away on a front-view rear-slot handheld");
                assert!(cross(m.u, m.v) > 0.0, "{system}: artwork must stay upright, without mirroring");
                previous = m.at;
            }
            if kind == Kind::Cart && system != "wii" {
                let travel = sub(r.seat.at, r.seat.hover);
                assert!(travel.0 * r.seat.v.0 + travel.1 * r.seat.v.1 > 0.0, "{system}: connector end must lead");
                let (a, b) = r.seat.slot.unwrap();
                let below = system == "gba-sp";
                assert_eq!(r.bottom_slot, below, "{system}: Nintendo's upper/bottom slot");
                assert!(cross(sub(b, a), travel) * if below { -1.0 } else { 1.0 } > 0.0, "{system}: enter from the physical slot side");
            }
        }
    }

    #[test]
    fn the_label_turns_only_after_leaving_the_case_and_is_away_before_insertion() {
        for (system, away) in [("gb", true), ("gbh", true), ("gbc", true), ("gbch", true), ("gba", true), ("gbah", true), ("gba-sp", true), ("nds", true), ("snes", false), ("n64", false), ("psx", false)] {
            assert_eq!(label_away(system), away, "{system}: plain animation has the same physical face");
            assert_eq!(face_at(away, 0.0), (1.0, false), "{system}: label beside the case");
            assert_eq!(face_at(away, 1.0), (1.0, away), "{system}: seated face");
        }
        let (edge, _) = face_at(true, 0.5);
        assert!(edge < 0.001, "the cartridge turns through its edge rather than changing pictures face-on");
        assert!(!label_away("gbasp"), "SP is a different bottom-slot console, never treated as the AGB-001");
    }

    /// How far a rig's medium sits from the console's own middle, art pixels across (in its own axes): the console's outline
    /// runs from this much to that in the rows its picture has them.
    fn off_body_centre(system: &str) -> f32 {
        let f = files(system, Game::Disc).expect(system);
        let b = crate::system_art::decode(&f.base).expect("base");
        let s = f.rig.seat;
        let det = cross(s.u, s.v);
        let across = |p: V| cross(p, s.v) / det * s.u.0.hypot(s.u.1);
        let (mut lo, mut hi) = (f32::MAX, f32::MIN);
        for y in 0..b.h {
            for x in 0..b.w {
                if b.px[(y * b.w + x) as usize] >> 24 == 255 {
                    let a = across((x as f32, y as f32));
                    lo = lo.min(a);
                    hi = hi.max(a);
                }
            }
        }
        across(s.at) - (lo + hi) / 2.0
    }

    #[test]
    fn a_handheld_s_cartridge_is_on_the_console_s_middle() {
        // The GBA's stood 17.5 art pixels (35 on the screen) to the left of its console.
        for system in ["gba", "gb"] {
            let off = off_body_centre(system);
            assert!(off.abs() <= 2.0, "{system}: {off:.1} art pixels off the console's middle");
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
            // A lid's underside is plastic: the photo's white backdrop showed as a pale blade when it stood open.
            if let Some(under) = f.under.as_ref().and_then(|d| crate::system_art::decode(d)) {
                let white = under.px.iter().filter(|p| **p >> 24 == 255 && (0..3).all(|k| (**p >> (8 * k) & 0xff) > 225)).count();
                assert!(white * 100 <= under.px.iter().filter(|p| **p >> 24 == 255).count(), "{}: {white} white pixels on its lid's underside", r.0[0]);
            }
            // It lies inside the picture (a tray's: all the way out).
            let s = f.rig.seat;
            assert!(s.at.0 > 0.0 && s.at.0 < base.w as f32 && s.at.1 > 0.0 && s.at.1 < base.h as f32 + 200.0, "{}", r.0[0]);
        }
    }

    #[test]
    fn a_shut_lid_is_exactly_where_its_picture_is() {
        let lid = Lid { at: (10.0, 20.0), hinge: ((40.0, 20.0), (90.0, 50.0)), front: (-30.0, 40.0), up: (0.0, -55.0), open_deg: 80.0, tuck: 0.0 };
        let m = lid.map(0.0);
        for (a, b) in [(m.o, (10.0, 20.0)), (m.u, (1.0, 0.0)), (m.v, (0.0, 1.0))] {
            assert!((a.0 - b.0).abs() < 0.001 && (a.1 - b.1).abs() < 0.001, "{m:?}");
        }
        assert!(!m.under);
        // Standing past upright its underside shows and it is above the hinge.
        let m = lid.map(1.0);
        assert!(m.turned > 0.9);
    }

    /// The lid consoles and how wide their disc is against the console as the picture shows it (a 12 cm disc; the GameCube's is 8 cm).
    const DISC_CONSOLES: [(&str, f32); 4] = [("psx", 0.46), ("saturn", 0.46), ("dreamcast", 0.62), ("gamecube", 0.365)];

    #[test]
    fn a_disc_is_carried_flat_label_up_and_lowered_onto_its_spindle_never_held_upright() {
        // The consoles' manuals: lay the disc label side up on the spindle. Over its place it lies as it will lie in its
        // seat (a flat ellipse in the picture), not standing face-on to the player.
        for (system, _) in DISC_CONSOLES.into_iter().chain([("ps2", 0.0), ("xbox", 0.0)]) {
            let r = files(system, Game::Disc).expect(system).rig;
            let kind = r.kind();
            let hover = r.medium_at((0.0, 0.0), |p| p, (90.0, 90.0), &at(kind, 900), 1.0);
            let seated = r.medium_at((0.0, 0.0), |p| p, (90.0, 90.0), &at(kind, 1200), 1.0);
            // Same shape over its place as in its seat (to the bounce's few percent): its short axis is the lying disc's.
            let (hv, sv) = (hover.v.0.hypot(hover.v.1), seated.v.0.hypot(seated.v.1));
            assert!((hv - sv).abs() < 0.08 * sv, "{system}: over its place the disc's depth is {hv}, lying it is {sv}");
            assert!(sv < 0.75 * seated.u.0.hypot(seated.u.1), "{system}: a lying disc is a flat ellipse");
            // And it comes down straight: no turning while it drops.
            let mid = r.medium_at((0.0, 0.0), |p| p, (90.0, 90.0), &at(kind, 1100), 1.0);
            let mv = mid.v.0.hypot(mid.v.1);
            assert!((mv - sv).abs() < 0.08 * sv, "{system}: tipping over while it drops ({mv} against {sv})");
        }
    }

    /// The width of the opaque part of a picture.
    fn opaque_width(png: &[u8]) -> f32 {
        let s = crate::system_art::decode(png).unwrap();
        let cols: Vec<u32> = (0..s.w).filter(|x| (0..s.h).any(|y| s.px[(y * s.w + x) as usize] >> 24 == 255)).collect();
        (cols.last().unwrap() - cols.first().unwrap() + 1) as f32
    }

    #[test]
    fn a_disc_is_as_large_beside_its_console_as_the_real_one() {
        // Real disc across over the console's width (a 12 cm disc, the GameCube's 8 cm one) as the pictures show them.
        for (system, real) in DISC_CONSOLES {
            let f = files(system, Game::Disc).expect(system);
            let across = 2.0 * f.rig.seat.u.0.hypot(f.rig.seat.u.1);
            let share = across / opaque_width(&f.base);
            assert!((share - real).abs() < 0.04, "{system}: its disc is {share:.2} of the console's width, the real one {real:.2}");
        }
    }

    /// The mean brightness (0-255) of a picture's opaque pixels.
    fn mean_luma(png: &[u8]) -> f32 {
        let s = crate::system_art::decode(png).unwrap();
        let l: Vec<f32> = s.px.iter().filter(|p| **p >> 24 == 255).map(|p| 0.299 * (p >> 16 & 255) as f32 + 0.587 * (p >> 8 & 255) as f32 + 0.114 * (p & 255) as f32).collect();
        l.iter().sum::<f32>() / l.len() as f32
    }

    #[test]
    fn a_lids_underside_is_its_own_plastic_never_far_lighter_than_its_console() {
        // The underside of a lid is the console's own moulding, in shadow: the Saturn's lid is black on both sides (it
        // stood open as a pale grey slab above a black console). The GameCube's carries its grey instruction sticker.
        for system in ["psx", "saturn", "dreamcast"] {
            let f = files(system, Game::Disc).expect(system);
            let (body, under) = (mean_luma(&f.base), mean_luma(f.under.as_deref().unwrap()));
            assert!(under <= body * 1.15, "{system}: underside {under:.0} against the console's {body:.0}");
        }
    }

    #[test]
    fn an_open_lid_stands_up_out_of_the_consoles_rear_wall_it_does_not_float_above_it() {
        // Real lids (a PlayStation's, a Saturn's, a Dreamcast's) are hinged under their rear lip: open, the lid comes up out of
        // the console and its foot is hidden behind the wall. Across the middle third of the lid's width, the lowest point of
        // the open lid must be at or below the console's rear edge in the picture, never above it (a floating second disc).
        for system in ["psx", "saturn", "dreamcast"] {
            let f = files(system, Game::Disc).expect(system);
            let Mech::Lid(lid) = f.rig.mech() else { panic!("{system}: a lid") };
            let (base, top) = (crate::system_art::decode(&f.base).unwrap(), crate::system_art::decode(f.top.as_deref().unwrap()).unwrap());
            let m = lid.map(1.0);
            let rim = |x: i32| (0..base.h as i32).find(|y| base.px[(*y as u32 * base.w + x as u32) as usize] >> 24 == 255);
            // The lowest mapped lid pixel in each column of the picture.
            let mut low = std::collections::BTreeMap::<i32, f32>::new();
            for y in 0..top.h {
                for x in 0..top.w {
                    if top.px[(y * top.w + x) as usize] >> 24 == 255 {
                        let (px, py) = (m.o.0 + m.u.0 * (x as f32 + 0.5) + m.v.0 * (y as f32 + 0.5), m.o.1 + m.u.1 * (x as f32 + 0.5) + m.v.1 * (y as f32 + 0.5));
                        let e = low.entry(px as i32).or_insert(f32::MIN);
                        *e = e.max(py);
                    }
                }
            }
            let (x0, x1) = (*low.keys().next().unwrap(), *low.keys().last().unwrap());
            let third = (x1 - x0) / 3;
            let mut compared = 0;
            for x in (x0 + third)..(x1 - third) {
                let (Some(&y), Some(r)) = (low.get(&x), rim(x)) else { continue };
                compared += 1;
                assert!(y >= r as f32 + 2.0, "{system}: at x {x} the open lid ends {y:.0} above the console's rear edge {r}");
            }
            assert!(compared > 10, "{system}: the lid and the console overlap in the picture ({compared} columns)");
        }
    }
}
