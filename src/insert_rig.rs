//! A game going into its console's real mechanism (Cozy + Playful cases, docs/DESIGN.md "Starting a game"): a PS1's lid
//! opens, the disc comes down onto the spindle and the lid shuts; a tray slides out and takes the disc in; a cartridge
//! is pushed into its slot behind the slot's lip and clicks; a front-loader's door (the NES) lifts, the cartridge slides in
//! lying flat, is pressed down and the door drops shut. Systems without a [`Rig`] keep the plain animation
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
/// A console with a front picture turns round to its back between these moments (ms after A): as long as the game box's
/// turn on the card (280 ms), eased at both ends, while the console comes up and before its cover opens.
pub const TURN_MS: (f32, f32) = (180.0, 460.0);
/// A front-loader's cartridge, slid in, is pressed down over this long, ending with the click at `IN_AT_COZY`.
const PRESS_MS: f32 = 110.0;

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

/// A lid's thickness as a part of its depth (front to hinge): a PlayStation's lid is about 7 of 110 mm.
pub const LID_THICK: f32 = 0.065;

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
    /// The medium goes in behind it: the PSP-1000's UMD slides down into its cover, which stands open a little, and the
    /// cover shuts with it. Drawn over the medium all the time.
    #[serde(default)]
    pub medium_behind: bool,
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

    /// Where the lid's other face is at `open`, from the face the map shows (art pixels): the lid is a slab
    /// [`LID_THICK`] of its depth thick, its other face along its normal (`up` turned with it). Grows in over the first
    /// moment of the opening: shut, its edge is part of the console's picture.
    pub fn thickness(self, open: f32) -> V {
        let th = open.max(0.0) * self.open_deg.to_radians();
        let (sin, cos) = th.sin_cos();
        let normal = sub(mul(self.up, cos), mul(self.front, sin));
        let grow = (open / 0.15).clamp(0.0, 1.0);
        mul(normal, -LID_THICK * grow * grow * (3.0 - 2.0 * grow))
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
    /// A front-loader's door (the NES): it lifts about its hinge at the back, the cartridge slides in under it lying flat
    /// through the opening (`Rig::mouth`), is pressed down (`Seat::press`) and the door drops shut over it.
    Door(Lid),
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
    /// A front-loader: once slid in, the cartridge is pressed this far (art pixels, its last part of the way to `at`).
    #[serde(default)]
    pub press: V,
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
    /// Slides in flat under a lifted door, is pressed down and the door shuts.
    Door,
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
    /// A front-loader's opening: where the console's front cuts the plane the cartridge lies in, two ends on the base. What
    /// of the cartridge is beyond it is inside the console, seen only through the opening.
    #[serde(default)]
    pub mouth: Option<(V, V)>,
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
    #[serde(default)]
    pub door: Option<Lid>,
}

/// A rig's files: `base` the console without what moves; `over` what is in front of the medium and the tray, the same
/// size as the base and the rest clear; `top` and `under` a lid's two sides or `top` a tray.
pub struct Files {
    pub rig: Rig,
    /// A handheld that takes its game at its back (the PSP): its front, which turns round to the back first ([`Pose::face`]).
    pub front: Option<Cow<'static, [u8]>>,
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
    /// While it turns round on its way to a rear slot (`turn_angle`): how far (0 to π) and its half width before the
    /// turn narrowed `u`. The face is then off its middle by half its thickness and its edge shows beside it.
    pub turn: Option<(f32, V)>,
}

impl Rig {
    pub fn kind(&self) -> Kind {
        match self.mech() {
            Mech::Fixed => Kind::Cart,
            Mech::Lid(_) => Kind::Lid,
            Mech::Tray(_) => Kind::Tray,
            Mech::Door(_) => Kind::Door,
        }
    }

    pub fn mech(&self) -> Mech {
        match (self.mech.lid, self.mech.tray, self.mech.door) {
            (Some(l), _, _) => Mech::Lid(l),
            (_, Some(t), _) => Mech::Tray(t),
            (_, _, Some(d)) => Mech::Door(d),
            _ => Mech::Fixed,
        }
    }

    /// A front-loader's cartridge at `m` (at `pose`): how much of its picture, from its top row (the end
    /// nearest the player, its connector goes first), is still outside the console, 0 to 1. `screen` maps the art to the
    /// screen as for `m`. Slid all the way in it is inside, pressed down or not: the press takes the plane it lies in
    /// down with it, below the opening's lip.
    pub fn outside(&self, m: &MediumAt, pose: &Pose, screen: impl Fn(V) -> V) -> f32 {
        let Some((a, b)) = self.mouth else { return 1.0 };
        if pose.drop >= 1.0 {
            return 0.0;
        }
        let (q, d) = (screen(a), sub(screen(b), screen(a)));
        let along = cross(m.v, d);
        if along.abs() < 1e-3 {
            return 1.0;
        }
        // m.at + k·m.v is on the mouth's line at k; the picture's rows run from k = -1 (top) to 1 (bottom).
        let k = cross(sub(q, m.at), d) / along;
        ((k + 1.0) / 2.0).clamp(0.0, 1.0)
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
        } else if self.kind() == Kind::Door {
            // Slid in lying flat, then pressed down.
            let slid = sub(s.at, s.press);
            (add(lerp(hover, screen(slid), pose.drop), mul(s.press, pose.press * scale)), u1, v1)
        } else {
            let tilt = (pose.drop / 0.75).min(1.0);
            (lerp(hover, screen(s.at), pose.drop), lerp(u_hover, u1, tilt), lerp(v_hover, v1, tilt))
        };
        // A cartridge that goes in turned around (the NES's: label up, its connector away from the player) turns on the
        // way, as the SP's does, both axes together: lerping each would squeeze it through nothing.
        if (self.bottom_slot || self.kind() == Kind::Door) && pose.fly < 1.0 {
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
        // After it is in, a small bounce along its way in (a front-loader's: along the press).
        if pose.settle != 0.0 {
            let dir = if self.kind() == Kind::Door { s.press } else { sub(screen(s.at), hover) };
            let n = dir.0.hypot(dir.1).max(1.0);
            at = add(at, mul(dir, pose.settle * scale / n));
        }
        let squash = pose.squash;
        if squash != 0.0 {
            u = mul(u, 1.0 + 0.03 * squash);
            v = mul(v, 1.0 - 0.05 * squash);
        }
        let (width, back) = face_at(self.label_away, pose.fly);
        let turn = turn_angle(self.label_away, pose.fly).map(|a| (a, u));
        u = mul(u, width);
        MediumAt { at, u, v, back, turn }
    }
}

/// The cartridge leaves its case label-first, then turns around its upright axis on the
/// flight to a rear slot. The back image has its own handedness; never mirror either face.
/// This also drives the plain launch (GBC and Quick), so they cannot retain the old label.
pub fn face_at(away: bool, fly: f32) -> (f32, bool) {
    if !away {
        return (1.0, false);
    }
    let turn = turn_of(fly);
    ((turn * std::f32::consts::PI).cos().abs(), turn > 0.5)
}

/// How far a cartridge has turned round (0 to 1) at `fly`: over most of its flight, so no 60 Hz frame turns it by much
/// more than a tenth of the way (the first version turned it in the middle 60 %, by up to 45 degrees a frame).
fn turn_of(fly: f32) -> f32 {
    ((fly - 0.08) / 0.84).clamp(0.0, 1.0)
}

/// While a cartridge turns round on its way to a rear slot (`face_at`), how far it has turned: 0 to π.
pub fn turn_angle(away: bool, fly: f32) -> Option<f32> {
    let turn = turn_of(fly);
    (away && turn > 0.0 && turn < 1.0).then(|| turn * std::f32::consts::PI)
}

/// A cartridge's or card's thickness as a part of its width: what of its edge shows while it turns (a Game Boy cartridge
/// is about 8 of 57 mm, a GBA's 8 of 57, a DS card 3.8 of 35, a Game Gear cartridge 10 of 68).
pub fn thickness(game: Game) -> f32 {
    match game {
        Game::Card => 0.11,
        _ => 0.14,
    }
}

/// These handhelds are viewed from their screen side, with a cartridge slot behind them.
pub fn label_away(system: &str) -> bool {
    matches!(system, "gb" | "gbh" | "gbc" | "gbch" | "gba" | "gbah" | "gba-sp" | "nds" | "ndsh" | "gamegear" | "ggh" | "3ds" | "n3ds")
}

/// Everything that moves, at one moment.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Pose {
    /// The medium coming out of its case: 0 behind it, 1 beside it.
    pub out: f32,
    /// From beside the case to over its place.
    pub fly: f32,
    /// From over its place to in (a front-loader: slid in).
    pub drop: f32,
    /// A front-loader's cartridge pressed down, 0 to 1.
    pub press: f32,
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
    /// A console with a front picture (`Files::front`): 0 its front, 1 turned round to its back, where the game goes in.
    pub face: f32,
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
        return Pose { out, fly, drop, landed, settle, squash, open: 0.0, dip, led, shut_in: false, press: 0.0, face: 1.0 };
    }
    // Falling: slow off the hover, fast at the end; it rocks on the spindle, then lies still. A front-loader's cartridge
    // slides in, then is pressed down with the click.
    let (drop, press) = if kind == Kind::Door {
        (ease_in_out(phase(ms, 900.0, in_at - PRESS_MS)), ease_in_out(phase(ms, in_at - PRESS_MS, in_at)))
    } else {
        (phase(ms, 900.0, in_at).powi(2), 0.0)
    };
    let settle = if kind == Kind::Door { 0.5 * swing(after, 6.0, 60.0) } else { -1.5 * swing(after, 7.0, 45.0).abs() };
    let shut = in_at + SHUT_AFTER.as_secs_f32() * 1000.0;
    let opening = ease_out(phase(ms, 360.0, 720.0));
    // Over a little as it stops, as a lid on a hinge does.
    let open_over = 0.04 * swing(ms - 720.0, 2.2, 100.0);
    let begins = in_at + REST_MS;
    let (closing, rebound, dip) = match kind {
        // A lid picks up speed and lands with the clunk, a small rebound; so does a front-loader's door.
        Kind::Lid | Kind::Door => (phase(ms, begins, shut).powi(2), 0.022 * swing(ms - shut, 6.0, 65.0).abs(), swing(ms - shut, 8.0, 55.0).abs()),
        // A tray is driven: it starts and stops softly.
        _ => (ease_in_out(phase(ms, begins, shut)), 0.0, 0.5 * swing(ms - shut, 8.0, 55.0).abs()),
    };
    let open = opening * (1.0 - closing) + open_over * (1.0 - closing) + rebound;
    let face = ease_in_out(phase(ms, TURN_MS.0, TURN_MS.1));
    Pose { out, fly, drop, press, landed, settle, squash: 0.0, open, dip, led, shut_in: ms >= shut, face }
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
    (&["gbc"], bytes!("gbc", "rig.json"), bytes!("gbc", "base.png"), None, None, None),
    (&["gamegear"], bytes!("gamegear", "rig.json"), bytes!("gamegear", "base.png"), None, None, None),
    (&["n64"], bytes!("n64", "rig.json"), bytes!("n64", "base.png"), Some(bytes!("n64", "over.png")), None, None),
    (&["megadrive", "genesis", "megadrive-japan"], bytes!("megadrive", "rig.json"), bytes!("megadrive", "base.png"), Some(bytes!("megadrive", "over.png")), None, None),
    (&["dreamcast"], bytes!("dreamcast", "rig.json"), bytes!("dreamcast", "base.png"), Some(bytes!("dreamcast", "over.png")), Some(bytes!("dreamcast", "top.png")), Some(bytes!("dreamcast", "under.png"))),
    (&["saturn"], bytes!("saturn", "rig.json"), bytes!("saturn", "base.png"), Some(bytes!("saturn", "over.png")), Some(bytes!("saturn", "top.png")), Some(bytes!("saturn", "under.png"))),
    (&["xbox"], bytes!("xbox", "rig.json"), bytes!("xbox", "base.png"), Some(bytes!("xbox", "over.png")), Some(bytes!("xbox", "tray.png")), None),
    (&["3ds"], bytes!("3ds", "rig.json"), bytes!("3ds", "base.png"), None, None, None),
    (&["nds"], bytes!("nds", "rig.json"), bytes!("nds", "base.png"), None, None, None),
    (&["wii"], bytes!("wii", "rig.json"), bytes!("wii", "base.png"), Some(bytes!("wii", "over.png")), None, None),
    (&["psp"], bytes!("psp", "rig.json"), bytes!("psp", "base.png"), None, Some(bytes!("psp", "top.png")), Some(bytes!("psp", "under.png"))),
    (&["pcengine"], bytes!("pcengine", "rig.json"), bytes!("pcengine", "base.png"), Some(bytes!("pcengine", "over.png")), None, None),
    (&["mastersystem"], bytes!("mastersystem", "rig.json"), bytes!("mastersystem", "base.png"), Some(bytes!("mastersystem", "over.png")), None, None),
    (&["nes"], bytes!("nes", "rig.json"), bytes!("nes", "base.png"), Some(bytes!("nes", "over.png")), Some(bytes!("nes", "top.png")), Some(bytes!("nes", "under.png"))),
];

/// The consoles that turn round from their front to their back first ([`Files::front`]).
const FRONTS: &[(&str, &[u8])] = &[("psp", bytes!("psp", "front.png"))];

/// A rig from `POCKETUI_RIG_ASSETS` (module doc).
fn from_disk(system: &str) -> Option<Files> {
    let dir = PathBuf::from(std::env::var_os("POCKETUI_RIG_ASSETS")?).join(system);
    let read = |f: &str| std::fs::read(dir.join(f)).ok().map(Cow::Owned);
    let json = read("rig.json")?;
    Some(Files { rig: serde_json::from_slice(&json).ok()?, front: read("front.png"), base: read("base.png")?, over: read("over.png"), top: read("top.png").or_else(|| read("tray.png")), under: read("under.png") })
}

/// Which rig a game goes into: a US cartridge fits the US console only; a Famicom is not an NES (a top-loader, another
/// cartridge), so its games keep the plain animation until it has a rig of its own.
fn rig_id(system: &str, game: Game) -> &str {
    if game == Game::SnesUs {
        "snes-us"
    } else if system == "famicom" {
        "famicom"
    } else {
        crate::system_art::picture_id(system)
    }
}

/// A game's rig has a lid, tray or door to shut after the medium is in (the launch splits its sound for that, and the way
/// back starts with it shut).
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
    let front = FRONTS.iter().find(|f| f.0 == r.0[0]).map(|f| Cow::Borrowed(f.1));
    Some(Files { rig: serde_json::from_slice(r.1).ok()?, front, base: Cow::Borrowed(r.2), over: r.3.map(Cow::Borrowed), top: r.4.map(Cow::Borrowed), under: r.5.map(Cow::Borrowed) })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cabinet::INSERT_COZY;

    fn at(kind: Kind, ms: u64) -> Pose {
        pose(kind, Duration::from_millis(ms))
    }

    const KINDS: [Kind; 4] = [Kind::Lid, Kind::Tray, Kind::Cart, Kind::Door];

    #[test]
    fn the_medium_is_in_with_its_sound_rests_and_a_lid_shuts_with_the_clunk() {
        let in_ms = IN_AT_COZY.as_millis() as u64;
        for kind in KINDS {
            assert!(at(kind, in_ms - 40).landed == 0.0 && at(kind, in_ms).landed == 1.0, "{kind:?}");
            assert_eq!(at(kind, in_ms).drop, 1.0, "{kind:?}");
        }
        let shut = in_ms + SHUT_AFTER.as_millis() as u64;
        for kind in [Kind::Lid, Kind::Tray, Kind::Door] {
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
            ("gbc", Kind::Cart, true, (-1, 1), true),
            ("gamegear", Kind::Cart, true, (-1, 1), true),
            // Turned round to its back: the UMD slides down into its open cover, label out (PSP-1000 manual).
            ("psp", Kind::Lid, false, (0, 1), false),
            ("mastersystem", Kind::Cart, false, (0, 1), false),
            // The HuCard slides in from the front, lying on the slot's ramp, up and right on the picture.
            ("pcengine", Kind::Cart, false, (1, -1), false),
            ("n64", Kind::Cart, false, (0, 1), false),
            ("megadrive", Kind::Cart, false, (0, 1), false),
            ("dreamcast", Kind::Lid, false, (0, 1), false),
            ("saturn", Kind::Lid, false, (0, 1), false),
            ("xbox", Kind::Tray, false, (1, -1), false),
            ("nds", Kind::Cart, true, (-1, 1), true),
            ("3ds", Kind::Cart, true, (-1, 1), true),
            ("wii", Kind::Cart, false, (1, 1), false),
            // Slid in towards the back (up and right on the picture), then pressed down: checked below.
            ("nes", Kind::Door, false, (1, -1), false),
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
                    "gb" | "gbc" | "gamegear" => 0.0..0.12,
                    "gba" => 0.0..0.30,
                    "gba-sp" => 0.65..0.90, // lower front edge; label faces the underside
                    "nds" | "3ds" => 0.35..0.65, // rear of the lower half, beside the hinge
                    "wii" => 0.30..0.65, // vertical slot on the front face
                    _ => 0.10..0.55, // cartridge slot on the console's upper surface
                };
                assert!(range.contains(&y), "{system}: slot must stay on its physical side of the photographed console ({y})");
            }
            let mut previous = r.medium_at((0.0, 0.0), |p| p, (90.0, 90.0), &at(kind, 900), 1.0).at;
            // A front-loader's cartridge slides in until it is pressed down (that part is checked in its own test).
            let last = if kind == Kind::Door { IN_AT_COZY.as_millis() as u64 - PRESS_MS as u64 } else { 1200 };
            for ms in (920..=last).step_by(20) {
                let m = r.medium_at((0.0, 0.0), |p| p, (90.0, 90.0), &at(kind, ms), 1.0);
                let delta = sub(m.at, previous);
                for (d, sign) in [(delta.0, direction.0), (delta.1, direction.1)] {
                    assert!(if sign == 0 { d.abs() < 0.01 } else { d * sign as f32 > 0.0 }, "{system} at {ms}: travelling {delta:?}, expected {direction:?}");
                }
                assert_eq!(m.back, back, "{system} at {ms}: label must face away on a front-view rear-slot handheld");
                assert!(cross(m.u, m.v) > 0.0, "{system}: artwork must stay upright, without mirroring");
                previous = m.at;
            }
            if kind == Kind::Door {
                let travel = sub(sub(r.seat.at, r.seat.press), r.seat.hover);
                assert!(travel.0 * r.seat.v.0 + travel.1 * r.seat.v.1 > 0.0, "{system}: connector end must lead");
            }
            if system == "pcengine" {
                // A HuCard's contacts are at its picture's top: they lead, and it comes from the front (below the slot
                // on the picture), label up.
                let travel = sub(r.seat.at, r.seat.hover);
                assert!(travel.0 * r.seat.v.0 + travel.1 * r.seat.v.1 < 0.0, "{system}: contacts first");
                let (a, b) = r.seat.slot.unwrap();
                assert!(cross(sub(b, a), travel) < 0.0, "{system}: from the front");
            } else if kind == Kind::Cart && system != "wii" {
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
        for (system, away) in [("gb", true), ("gbh", true), ("gbc", true), ("gbch", true), ("gba", true), ("gbah", true), ("gba-sp", true), ("nds", true), ("gamegear", true), ("3ds", true), ("snes", false), ("n64", false), ("psx", false)] {
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
        for system in ["gba", "gb", "gbc"] {
            let off = off_body_centre(system);
            assert!(off.abs() <= 2.0, "{system}: {off:.1} art pixels off the console's middle");
        }
        // The Game Gear's photo is turned and much nearer at its right: across its whole height its outline leans, so its
        // middle is measured where the slot is, over the top half of its outline.
        let f = files("gamegear", Game::GameGear).expect("gamegear");
        let b = crate::system_art::decode(&f.base).unwrap();
        let cols: Vec<u32> = (0..b.w).filter(|x| (0..b.h / 2).any(|y| b.px[(y * b.w + x) as usize] >> 24 == 255)).collect();
        let middle = (cols[0] + cols[cols.len() - 1] + 1) as f32 / 2.0;
        assert!((f.rig.seat.at.0 - middle).abs() <= 3.0, "gamegear: {} against its middle {middle}", f.rig.seat.at.0);
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
                Mech::Lid(_) | Mech::Door(_) => assert!(f.top.is_some() && f.under.is_some(), "{}", r.0[0]),
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

    /// The scenes made from photos on 2026-11-11 (fable52/IDEEN-ANIMATION.md, place 1): each has its own console and
    /// mechanism, never the plain animation's fallback.
    const PHOTO_SCENES_1111: [(&str, &str, Kind); 7] = [
        ("nes", "Super Mario Bros. 3 (USA).nes", Kind::Door),
        ("gbc", "Pokemon - Crystal Version (USA, Europe).gbc", Kind::Cart),
        ("psp", "Lumines (USA).iso", Kind::Lid),
        ("mastersystem", "Alex Kidd in Miracle World (USA, Europe).sms", Kind::Cart),
        ("gamegear", "Sonic The Hedgehog (World).gg", Kind::Cart),
        ("pcengine", "PC Genjin (Japan).pce", Kind::Cart),
        ("3ds", "Legend of Zelda, The - Ocarina of Time 3D (USA) (En,Fr,Es).3ds", Kind::Cart),
    ];

    #[test]
    fn every_new_scene_has_its_own_console_and_mechanism_not_the_plain_fallback() {
        for (system, rom, kind) in PHOTO_SCENES_1111 {
            let game = Game::of(system, rom);
            let f = files(system, game).unwrap_or_else(|| panic!("{system}: no rig, the plain animation would show"));
            assert_eq!(f.rig.kind(), kind, "{system}");
            let base = crate::system_art::decode(&f.base).expect(system);
            assert!(base.w >= 120 && base.h >= 80 && base.w <= 320 && base.h <= 300, "{system}: {}x{} art pixels (a PSP is wide and flat)", base.w, base.h);
            let opaque = base.px.iter().filter(|p| **p >> 24 == 255).count();
            assert!(opaque * 3 > (base.w * base.h) as usize, "{system}: a console, not an empty picture");
        }
    }

    #[test]
    fn the_psp_turns_round_calmly_to_its_back_and_its_umd_slides_into_the_open_cover_label_out() {
        let f = files("psp", Game::Umd).expect("psp: a rig, not the plain animation");
        let r = f.rig;
        let base = crate::system_art::decode(&f.base).unwrap();
        let front = crate::system_art::decode(f.front.as_deref().expect("its front, to turn from")).expect("front");
        // Seen straight from behind and from the front, as large as the GBA's and DS's consoles (290 art pixels across).
        assert!(base.w >= 270 && front.w >= 270 && (base.w as i32 - front.w as i32).abs() <= 4, "{}x{} / {}x{}", base.w, base.h, front.w, front.h);
        // It starts on its front and has turned round, calmly (at least the game box's 280 ms), before its UMD flies over.
        assert_eq!(at(Kind::Lid, 0).face, 0.0);
        assert!(TURN_MS.1 - TURN_MS.0 >= 280.0, "a calm turn");
        let mut last = 0.0;
        for ms in (0..=600).step_by(10) {
            let p = at(Kind::Lid, ms);
            assert!(p.face >= last, "turning one way only at {ms}");
            last = p.face;
        }
        assert_eq!(at(Kind::Lid, TURN_MS.1 as u64).face, 1.0);
        assert!(at(Kind::Lid, 900).fly >= 1.0 && at(Kind::Lid, 900).face == 1.0);
        // Eased at both ends: its first and last tenth turn less than a fortieth each.
        let t = |frac: f32| at(Kind::Lid, (TURN_MS.0 + frac * (TURN_MS.1 - TURN_MS.0)) as u64).face;
        assert!(t(0.1) < 0.04 && t(0.9) > 0.96, "{} {}", t(0.1), t(0.9));
        // The cover: open a little (the UMD slides in behind it), shut with the clunk; it stays attached to its hinge.
        let Mech::Lid(lid) = r.mech() else { panic!("psp: a cover") };
        assert!(lid.medium_behind && (20.0..=45.0).contains(&lid.open_deg), "{lid:?}");
        // The UMD comes from above and slides straight down, label to the player, as wide against the PSP as the real one.
        assert!(!r.label_away && r.seat.hover.1 < 0.0 && r.seat.hover.0 == r.seat.at.0);
        let share = 2.0 * r.seat.u.0.hypot(r.seat.u.1) / opaque_width(&f.base);
        assert!((share - 64.0 / 170.0).abs() < 0.03, "the UMD is {share:.2} of the PSP's width");
        // Shut, the cover hides the UMD: the UMD lies within the cover's picture.
        let top = crate::system_art::decode(f.top.as_deref().unwrap()).unwrap();
        let (x0, y0) = lid.at;
        assert!(r.seat.at.0 - r.seat.u.0 >= x0 - 1.0 && r.seat.at.0 + r.seat.u.0 <= x0 + top.w as f32 + 1.0, "the UMD fits under its cover across");
        assert!(r.seat.at.1 - r.seat.v.1 >= y0 - 1.0 && r.seat.at.1 + r.seat.v.1 <= y0 + top.h as f32 + 1.0, "and down");
    }

    /// The front-loaders: the consoles whose cartridge slides in under a door.
    const FRONT_LOADERS: [&str; 1] = ["nes"];

    #[test]
    fn a_front_loader_s_cartridge_slides_in_under_its_open_door_is_pressed_down_and_the_door_shuts_over_it() {
        // The NES's manual: lift the door, push the Game Pak in label up all the way, press it down, close the door.
        let in_ms = IN_AT_COZY.as_millis() as u64;
        let slid_ms = in_ms - PRESS_MS as u64;
        for system in FRONT_LOADERS {
            let f = files(system, Game::of(system, "test.nes")).unwrap_or_else(|| panic!("{system}: a rig of its own, not the plain animation"));
            let r = f.rig;
            assert_eq!(r.kind(), Kind::Door, "{system}");
            assert!(f.top.is_some() && f.under.is_some() && f.over.is_some(), "{system}: the door's two sides and the console around the opening");
            assert!(r.mouth.is_some(), "{system}: where the front cuts the cartridge");
            let m = |ms: u64| r.medium_at((0.0, 0.0), |p| p, (90.0, 90.0), &at(Kind::Door, ms), 1.0);
            let outside = |ms: u64| r.outside(&m(ms), &at(Kind::Door, ms), |p| p);
            // Hanging in front of the opening it is all outside; slid in it is all inside, and so it stays.
            assert!(outside(900) > 0.99, "{system}: in front of its opening before it goes in ({})", outside(900));
            assert!((0.2..0.8).contains(&outside((900 + slid_ms) / 2)), "{system}: half in on the way ({})", outside((900 + slid_ms) / 2));
            // Pressed down and bouncing after the click, it never comes out again (it showed as a red strip at the lip).
            for ms in (slid_ms..=in_ms + 300).step_by(10) {
                assert!(outside(ms) < 0.01, "{system}: all of it inside at {ms} ({})", outside(ms));
            }
            // The door is up all the while it goes in, and shut over it with the clunk.
            for ms in (900..=in_ms).step_by(20) {
                assert!(at(Kind::Door, ms).open > 0.95, "{system}: the door is up at {ms}");
            }
            assert!(at(Kind::Door, in_ms + SHUT_AFTER.as_millis() as u64).open.abs() < 0.01, "{system}: the door shut");
            // Pressed: straight down on the picture, the click at the moment it is down.
            assert!(r.seat.press.1 > 2.0 && r.seat.press.0.abs() < 0.5, "{system}: pressed down {:?}", r.seat.press);
            let (a, b) = (m(slid_ms).at, m(in_ms).at);
            assert!((b.0 - a.0).abs() < 0.1 && (b.1 - a.1 - r.seat.press.1).abs() < 0.1, "{system}: pressed from {a:?} to {b:?}");
            // Label up, its connector away from the player: the picture turned round, never mirrored.
            let seated = m(in_ms);
            assert!(seated.u.0 < 0.0 && seated.v.1 < 0.0 && cross(seated.u, seated.v) > 0.0, "{system}: label up, connector first");
            for ms in (420..=900).step_by(20) {
                let mm = m(ms);
                assert!(cross(mm.u, mm.v) > 0.0 && mm.u.0.hypot(mm.u.1) > 10.0, "{system} at {ms}: turned round on the way, never through nothing");
            }
            // The way back opens the door first: it starts with it shut.
            assert!(takes_disc(system, Game::of(system, "test.nes")), "{system}");
        }
        // A Famicom is another console with another cartridge: not the NES's rig.
        assert!(files("famicom", Game::Nes).is_none(), "the Famicom keeps the plain animation");
    }

    #[test]
    fn a_front_loader_s_cartridge_is_as_large_against_its_opening_as_the_real_one() {
        // The NES Game Pak is 120 mm across; its door is 154 mm wide (measured on the photo against the console's 256 mm).
        // Both lie along the console's front, so the picture shortens them alike.
        for (system, cart_mm, opening_mm) in [("nes", 120.0, 154.0)] {
            let r = files(system, Game::Nes).expect(system).rig;
            let (a, b) = r.mouth.expect(system);
            let share = 2.0 * r.seat.u.0.hypot(r.seat.u.1) / sub(b, a).0.hypot(sub(b, a).1);
            let real = cart_mm / opening_mm;
            assert!((share - real).abs() < 0.05, "{system}: the cartridge is {share:.2} of its opening, the real one {real:.2}");
        }
    }

    #[test]
    fn a_shut_lid_is_exactly_where_its_picture_is() {
        let lid = Lid { at: (10.0, 20.0), hinge: ((40.0, 20.0), (90.0, 50.0)), front: (-30.0, 40.0), up: (0.0, -55.0), open_deg: 80.0, tuck: 0.0, medium_behind: false };
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
