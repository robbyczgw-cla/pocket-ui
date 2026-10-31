//! Playful animations (Settings → Sound & feel → **Playful cases**, docs/DESIGN.md "Playful
//! animations"): the Shelf and Cabinet looks' cases a little springy, in our 2D pixel style.
//!
//! - **Pulled out**, a case springs: it goes a touch past its place and settles (`back_out`).
//! - **Selected**, it rocks once on its bottom edge, minimally (`wiggle`, a shear of its rows).
//! - **Going in**, each medium goes its own way (`Loader`): a cartridge is pushed into its slot, seats with a squash
//!   and springs back a hair; a game card clicks in; a disc tray slides out, the disc drops in and the tray closes; a
//!   lid flips up and shuts over the disc; a slot-loader or the PSP's UMD door takes it in. The case sounds keep their
//!   moment: the medium is in at `cabinet::IN_AT` (0.6 of the launch animation), where `ui_sound::going_in` hits.
//! - **In the open box** a disc turns slowly (`spin`), for a while after the last button, then comes to rest.
//! - **Scrolling fast**, the Shelf look's spines lean against the move and settle when it stops (`Lean`).
//!
//! Every one of them is a transform of a picture made before (an offset, a shear, a scale, a turn of the disc): no
//! picture is made, read or decoded per frame. They follow the Animations' pace (`motion::time_scale`: Cozy is 1.4x
//! slower) and none runs unless Motion is Full ([`on`]).

use std::time::{Duration, Instant};

use crate::media::Game;
use crate::motion::{self, Kind};

/// Whether the playful animations run: the extra (on with Cozy, off with Quick until the player chooses) and Motion on
/// Full (Reduced and Off: nothing moves).
pub fn on() -> bool {
    motion::level() == motion::Motion::Full && crate::cozy_extras::on(crate::cozy_extras::Extra::Playful)
}

/// How long the selected case rocks after it is out.
pub const WIGGLE: Duration = Duration::from_millis(420);
/// How long a cartridge or card squashes and springs back after it is in.
pub const SEAT: Duration = Duration::from_millis(260);
/// How long the box's disc turns after the box opened or the last button.
pub const SPIN_FOR: Duration = Duration::from_secs(12);
/// One turn of the box's disc, slow.
pub const SPIN_TURN: Duration = Duration::from_secs(9);

/// An ease-out that goes about 5 % past the end and comes back: a case springing to its place.
pub fn back_out(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0) - 1.0;
    const S: f32 = 1.1;
    1.0 + (S + 1.0) * t * t * t + S * t * t
}

/// A damped wobble starting at 0: `hz` swings a second, dying by e every `decay` seconds.
pub fn wobble(secs: f32, hz: f32, decay: f32) -> f32 {
    if secs <= 0.0 {
        return 0.0;
    }
    (-secs / decay).exp() * (std::f32::consts::TAU * hz * secs).sin()
}

/// A damped bounce starting at 1 (squashed): out, a hair the other way, and still.
pub fn bounce(secs: f32, hz: f32, decay: f32) -> f32 {
    if secs < 0.0 {
        return 0.0;
    }
    (-secs / decay).exp() * (std::f32::consts::TAU * hz * secs).cos()
}

/// Seconds of animation time from `at` to `now`: real time divided by the time scale (Cozy, `POCKETUI_MOTION_SCALE`).
pub fn secs(at: Instant, now: Instant) -> f32 {
    now.saturating_duration_since(at).as_secs_f32() / motion::time_scale()
}

/// `d` at the current pace (zero when Motion keeps things still).
pub fn span(d: Duration) -> Duration {
    motion::span(Kind::Move, d)
}

/// How far the selected case rocks, `secs` (animation time) after it came out: a shear of its top edge, as a part of
/// its height (about 2 % at most), back to 0 within `WIGGLE`.
pub fn wiggle(secs: f32) -> f32 {
    if !(0.0..WIGGLE.as_secs_f32()).contains(&secs) {
        return 0.0;
    }
    0.02 * wobble(secs, 4.2, 0.11)
}

/// A cartridge or card seated `secs` ago: (how much it is squashed, 1 = 10 % flatter; how far it is lifted back out,
/// as a part of its height). Both 0 from `SEAT` on.
pub fn seat(secs: f32) -> (f32, f32) {
    if !(0.0..SEAT.as_secs_f32()).contains(&secs) {
        return (0.0, 0.0);
    }
    let b = bounce(secs, 6.5, 0.07);
    (b, 0.025 * (-b).max(0.0))
}

/// The box's disc's turn in radians, `secs` (animation time) after the box opened or the last button: a slow, steady
/// turn that slows to a stop over its last two seconds.
pub fn spin(secs: f32) -> f32 {
    let (run, turn) = (SPIN_FOR.as_secs_f32(), SPIN_TURN.as_secs_f32());
    let coast = 2.0f32;
    let w = std::f32::consts::TAU / turn;
    let secs = secs.max(0.0);
    let steady = run - coast;
    if secs <= steady {
        w * secs
    } else {
        // Slowing evenly to 0 over `coast`.
        let t = (secs - steady).min(coast);
        w * steady + w * (t - t * t / (2.0 * coast))
    }
}

// ---- going into the console --------------------------------------------------------------------------------------

/// How a console takes its game in.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Loader {
    /// A cartridge pushed down into its slot.
    Cart,
    /// A game card (DS, 3DS, Vita) clicked in all the way.
    Card,
    /// A disc tray slides out of the drive, the disc drops in, the tray closes (PS2, Xbox, 3DO, Wii U's tray-less
    /// cousins aside: every disc console not named below).
    Tray,
    /// A lid on top flips up, the disc drops in, the lid closes (GameCube, PlayStation, Saturn, Dreamcast, Sega CD,
    /// PC Engine CD, Neo Geo CD).
    Lid,
    /// A slot-loading drive or the PSP's UMD door: it slides in (Wii, Wii U, PS3, PSP).
    Slide,
}

impl Loader {
    pub fn of(system: &str, game: Game) -> Loader {
        match game {
            Game::Card | Game::Card3ds => Loader::Card,
            Game::Umd => Loader::Slide,
            Game::Disc | Game::MiniDisc => match system {
                "gamecube" | "gc" | "psx" | "ps1" | "playstation" | "saturn" | "dreamcast" | "segacd" | "megacd" | "pcenginecd" | "tg-cd"
                | "neogeocd" | "pcfx" => Loader::Lid,
                "wii" | "wiiu" | "ps3" => Loader::Slide,
                _ => Loader::Tray,
            },
            _ => Loader::Cart,
        }
    }
}

/// Where a top-loader's lid is on its console's picture (`assets/systems/<id>.png`, drawn as it is): its middle and
/// its two radii as parts of the picture's width and height, measured on the pictures (2026-10-30). Systems without a
/// picture of their own get a lid on the middle of the plain box drawn for them.
pub fn lid_of(system: &str) -> (f32, f32, f32, f32) {
    match system {
        "gamecube" | "gc" => (0.70, 0.23, 0.10, 0.076),
        "psx" | "ps1" | "playstation" => (0.64, 0.37, 0.18, 0.21),
        "dreamcast" => (0.49, 0.31, 0.32, 0.24),
        "segacd" | "megacd" => (0.59, 0.27, 0.245, 0.17),
        "pcenginecd" | "tg-cd" => (0.66, 0.13, 0.19, 0.095),
        _ => (0.5, 0.3, 0.28, 0.16),
    }
}

/// The medium's own moves `t` through the launch animation (0-1 of `cabinet::insert_len`; in at 0.6).
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Keys {
    /// The tray out of the drive or the lid up: 0 shut, 1 open.
    pub open: f32,
    /// A disc dropping into the open tray or drive: 0 over it, 1 in.
    pub drop: f32,
    /// After it is in: squashed (positive: 14 % flatter, 8 % wider at 1) or stretched (negative), about -0.5 to 1.
    pub squash: f32,
    /// After it is in: lifted back out, as a part of its height.
    pub lift: f32,
}

/// When the disc-in sound's tray shuts with its clunk, after the disc is in (`assets/ui-sounds/disc-in.wav`: in at
/// 34 ms, the clunk at 160 ms): a tray or lid is shut then.
pub const SHUT_AFTER: f32 = 0.126;

/// Where the moves are, `t` of the way through the launch animation, `since_in` seconds (real time, negative before)
/// after the medium is in, the animations `slower` times their designed pace (Cozy): the seat's spring keeps its shape,
/// a tray or lid shuts with the sound's clunk.
pub fn keys(loader: Loader, t: f32, since_in: f32, slower: f32) -> Keys {
    let phase = |a: f32, b: f32| ((t - a) / (b - a)).clamp(0.0, 1.0);
    let (squash, lift) = seat(since_in / slower.max(0.1));
    match loader {
        Loader::Cart | Loader::Card | Loader::Slide => Keys { open: 0.0, drop: 0.0, squash, lift },
        Loader::Tray | Loader::Lid => {
            // Open while the console comes up and the disc flies over; shut once it is in.
            let opening = crate::cabinet::ease_out(phase(0.24, 0.40));
            let closing = crate::cabinet::ease_in_out(((since_in - 0.012) / (SHUT_AFTER - 0.012)).clamp(0.0, 1.0));
            Keys {
                open: opening * (1.0 - closing),
                drop: phase(0.47, 0.6).powi(2),
                squash: squash * 0.5,
                lift: 0.0,
            }
        }
    }
}

// ---- the shelf's spines lean -------------------------------------------------------------------------------------

/// The Shelf look's spines leaning while the row scrolls fast: a spring (3 swings a second, half damped) pulled
/// towards a lean that follows the row's speed, so a single step barely tips them and a held button or a jump does;
/// when the row stops they come back with a small swing.
///
/// Worked out from the scroll glide's own segments (`cabinet::Tween::segment`) in fixed steps of animation time: the
/// same glide and the same moment give the same lean, in a frame and in a snapshot.
#[derive(Clone, Copy, Debug, Default)]
pub struct Lean {
    /// The glide's segment last seen and the spring (lean, speed) at its start.
    seg: Option<Seg>,
    start: (f32, f32),
    /// The spring as painted last.
    last: (f32, f32),
}

/// A glide from `from` to `to`, started `at`, `dur` long (as designed: the time scale divides it here).
pub type Seg = (f32, f32, Instant, Duration);

/// Px per second of row speed (per pixel of the layout's scale) at which the lean is full.
const FULL_AT: f32 = 380.0;
const STEP: f32 = 1.0 / 240.0;
/// The spring is still after this long without a new segment.
const STILL: f32 = 1.6;

impl Lean {
    /// The lean now, -1 to 1 (top to the left / right, times the most the caller allows), for the glide's current
    /// segment; `f` is the layout's scale (screen pixels per template pixel).
    pub fn at(&mut self, seg: Seg, f: f32, now: Instant) -> f32 {
        if self.seg != Some(seg) {
            self.start = match self.seg {
                Some(old) if seg.2 >= old.2 => run(old, self.start, f, seg.2),
                _ => (0.0, 0.0),
            };
            self.seg = Some(seg);
        }
        self.last = run(seg, self.start, f, now);
        self.last.0
    }

    /// Still leaning or swinging back, as painted last (the glide itself keeps frames coming while it moves).
    pub fn moving(&self, now: Instant) -> bool {
        self.seg.is_some_and(|s| secs(s.2, now) < STILL) && (self.last.0.abs() > 0.01 || self.last.1.abs() > 0.05)
    }

    /// Back upright at once (Motion off, the extra off, another list).
    pub fn reset(&mut self) {
        *self = Lean::default();
    }
}

/// The spring from `state` at the segment's start to `until`.
fn run(seg: Seg, state: (f32, f32), f: f32, until: Instant) -> (f32, f32) {
    let (from, to, at, dur) = seg;
    let total = secs(at, until).min(STILL);
    let d = dur.as_secs_f32().max(0.001);
    let w = std::f32::consts::TAU * 3.0;
    let zeta = 0.45;
    let (mut x, mut v) = state;
    let mut t = 0.0;
    while t < total {
        let p = t / d;
        // The glide's speed (an ease-out cubic), in template pixels per second.
        let speed = if p < 1.0 { (to - from) * 3.0 * (1.0 - p) * (1.0 - p) / d / f.max(0.1) } else { 0.0 };
        let want = (speed / FULL_AT).clamp(-1.0, 1.0);
        let a = w * w * (want - x) - 2.0 * zeta * w * v;
        v += a * STEP;
        x += v * STEP;
        t += STEP;
    }
    (x.clamp(-1.5, 1.5), v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn back_out_overshoots_a_little_and_lands() {
        assert!(back_out(0.0).abs() < 1e-6);
        assert!((back_out(1.0) - 1.0).abs() < 1e-6);
        let peak = (0..=100).map(|i| back_out(i as f32 / 100.0)).fold(0.0f32, f32::max);
        assert!(peak > 1.02 && peak < 1.08, "{peak}");
    }

    #[test]
    fn the_wiggle_and_the_seat_end_still() {
        assert_eq!(wiggle(0.0), 0.0);
        assert!(wiggle(0.05).abs() > 0.005 && wiggle(0.05).abs() <= 0.02);
        assert_eq!(wiggle(WIGGLE.as_secs_f32()), 0.0);
        assert!(wiggle(WIGGLE.as_secs_f32() - 0.01).abs() < 0.001, "no jump at the end");
        assert_eq!(seat(0.0).0, 1.0);
        assert_eq!(seat(SEAT.as_secs_f32()), (0.0, 0.0));
        assert!(seat(SEAT.as_secs_f32() - 0.01).0.abs() < 0.05);
        assert!((0..26).any(|i| seat(i as f32 / 100.0).1 > 0.0), "it springs back out a hair");
    }

    #[test]
    fn the_disc_turns_slowly_and_comes_to_rest() {
        assert_eq!(spin(0.0), 0.0);
        let end = spin(SPIN_FOR.as_secs_f32());
        assert_eq!(spin(SPIN_FOR.as_secs_f32() + 5.0), end, "still after it stopped");
        assert!(spin(1.0) < 1.0, "slow: under a radian a second");
        assert!((spin(SPIN_FOR.as_secs_f32() - 0.01) - end).abs() < 0.001);
    }

    #[test]
    fn every_medium_has_its_loader() {
        assert_eq!(Loader::of("snes", Game::Snes), Loader::Cart);
        assert_eq!(Loader::of("nds", Game::Card), Loader::Card);
        assert_eq!(Loader::of("ps2", Game::Disc), Loader::Tray);
        assert_eq!(Loader::of("xbox", Game::Disc), Loader::Tray);
        assert_eq!(Loader::of("gamecube", Game::MiniDisc), Loader::Lid);
        assert_eq!(Loader::of("psx", Game::Disc), Loader::Lid);
        assert_eq!(Loader::of("wii", Game::Disc), Loader::Slide);
        assert_eq!(Loader::of("psp", Game::Umd), Loader::Slide);
    }

    #[test]
    fn a_tray_is_open_while_the_disc_drops_and_shut_at_the_end() {
        assert_eq!(keys(Loader::Tray, 0.0, -1.0, 1.4).open, 0.0);
        let k = keys(Loader::Tray, 0.5, -0.1, 1.4);
        assert!(k.open > 0.99 && k.drop > 0.0 && k.drop < 1.0);
        let k = keys(Loader::Tray, 0.6, 0.0, 1.4);
        assert!(k.drop == 1.0 && k.open > 0.99, "in at IN_AT, with its sound, the tray still out");
        assert!(keys(Loader::Lid, 0.7, SHUT_AFTER, 1.4).open < 1e-6, "shut with the clunk");
        assert_eq!(keys(Loader::Cart, 0.3, -0.3, 1.0), Keys::default());
        assert_eq!(keys(Loader::Cart, 0.6, 0.0, 1.0).squash, 1.0, "squashed as it seats");
    }

    #[test]
    fn a_single_step_barely_leans_a_held_scroll_does_and_both_settle() {
        let t0 = Instant::now();
        let ms = |n: u64| t0 + Duration::from_millis(n);
        let mut one = Lean::default();
        let seg = (0.0, 50.0, t0, Duration::from_millis(220));
        let peak_one = (0..60).map(|i| one.at(seg, 2.5, ms(i * 10)).abs()).fold(0.0f32, f32::max);
        // Held: a new step every 70 ms.
        let mut held = Lean::default();
        let mut peak_held = 0.0f32;
        let mut from = 0.0f32;
        for k in 0..20u64 {
            let at = ms(k * 70);
            let to = 50.0 * (k + 1) as f32;
            let seg = (from, to, at, Duration::from_millis(220));
            for i in 0..7 {
                let v = held.at(seg, 2.5, at + Duration::from_millis(i * 10));
                peak_held = peak_held.max(v.abs());
            }
            from = held_value(seg, at + Duration::from_millis(70));
        }
        assert!(peak_held > 0.6, "{peak_held}");
        assert!(peak_one < 0.6 * peak_held, "a single step leans much less: {peak_one} / {peak_held}");
        assert!(one.at(seg, 2.5, ms(3000)).abs() < 0.01, "upright again");
    }

    fn held_value((from, to, at, dur): Seg, now: Instant) -> f32 {
        let p = (now.saturating_duration_since(at).as_secs_f32() / dur.as_secs_f32()).min(1.0);
        from + (to - from) * crate::cabinet::ease_out(p)
    }
}
