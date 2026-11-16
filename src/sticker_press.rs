//! Pressing a sticker onto a case, and peeling it off (Playful animations, docs/DESIGN.md): in the game box, when the
//! card's Stickers sheet closes, each sticker you put on comes down onto the case a little large and crooked, is pressed
//! in, springs once and sits; each one you took off lifts, floats up a little and fades. (Not while the sheet is open: its
//! panel is over the case, so the landing would be behind it.)
//!
//! The motion is a small piece of mechanics, not an easing curve:
//! - **Falling**: the sticker is let go above the case and accelerates evenly towards it ([`FALL`] seconds, from
//!   [`FROM`] times its size: a bigger sticker is a nearer one), so it meets the case with the speed that fall gives it.
//! - **Pressing**: the case's surface is a damped spring ([`OMEGA`], [`ZETA`]). The sticker enters it with that speed
//!   (position and speed both continue where the fall ended), is pressed in a few percent, comes back past its place
//!   by a hair once, and rests. By [`PRESS`] what is left is under a third of a percent, so cutting there is invisible.
//! - **Turn and shadow** follow the height: the extra tilt is gone when the sticker touches, and its shadow shrinks to
//!   the one-pixel edge the pictures of the case paint (`case_stickers::stick`) as it lands.
//!
//! It is a transform of a small picture made before ([`crate::case_stickers::placed`]): nothing is made per frame. It
//! follows the Animations' pace (Cozy 1.4x slower: [`PRESS`] stays under 450 ms there), runs only with
//! [`crate::playful::on`], and any button ends it (the sticker sits at once: the case's own picture shows it).

use std::time::Duration;

use crate::cabinet_art::{blit_pose, Pic};
use crate::gfx::Canvas;

/// A sticker put on: from letting go to the last wobble (animation time; Cozy's 1.4x makes it 448 ms).
pub const PRESS: Duration = Duration::from_millis(320);
/// A sticker taken off: from the first lift to gone.
pub const PEEL: Duration = Duration::from_millis(200);
/// How long the fall lasts, seconds.
pub const FALL: f32 = 0.12;
/// The size it is let go at, as a multiple of its size on the case.
pub const FROM: f32 = 1.35;
/// The extra tilt it is let go at, radians (about 8 degrees), which is gone as it touches.
pub const CROOKED: f32 = 0.14;
/// The case's surface as a spring: radians a second and damping ratio (under 1: it swings).
pub const OMEGA: f32 = 37.7;
pub const ZETA: f32 = 0.5;
/// How quickly it shows after being let go (it does not pop in), seconds.
const SHOW: f32 = 0.04;

/// How the sticker is drawn at one moment, against how it sits on the case.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    /// Its size against its size on the case (1 = sitting).
    pub scale: f32,
    /// A turn on top of its own tilt, radians, in the direction of that tilt's sign `side` (+1 or -1).
    pub turn: f32,
    /// How far above the case it is, 0 (on it) to 1 (let go): its shadow's offset and spread.
    pub lift: f32,
    /// 0 to 1.
    pub alpha: f32,
    /// How far it has floated up off its place, as a part of its own height (a sticker peeled off; 0 while it lands).
    pub rise: f32,
}

impl Pose {
    /// Sitting on the case: the picture the case itself shows.
    pub const SITTING: Pose = Pose { scale: 1.0, turn: 0.0, lift: 0.0, alpha: 1.0, rise: 0.0 };

    pub fn is_sitting(&self) -> bool {
        *self == Pose::SITTING
    }
}

/// The scale `secs` after the sticker was let go: the fall, then the spring. Continuous, with a continuous speed.
fn press_scale(secs: f32) -> f32 {
    let rise = FROM - 1.0;
    if secs < FALL {
        // Evenly accelerated from rest: all of `rise` covered at `FALL`.
        let u = secs.max(0.0) / FALL;
        return 1.0 + rise * (1.0 - u * u);
    }
    // The speed it meets the case with (negative: towards it), the spring takes it up from there.
    let v0 = -2.0 * rise / FALL;
    let t = secs - FALL;
    let wd = OMEGA * (1.0 - ZETA * ZETA).sqrt();
    1.0 + v0 / wd * (-ZETA * OMEGA * t).exp() * (wd * t).sin()
}

/// A sticker let go `secs` ago (animation time); `side` is the sign of its tilt (+1 or -1): it arrives more crooked in
/// the same direction. Sitting from [`PRESS`] on.
pub fn press(secs: f32, side: f32) -> Pose {
    if secs >= PRESS.as_secs_f32() {
        return Pose::SITTING;
    }
    let secs = secs.max(0.0);
    let scale = press_scale(secs);
    let u = (1.0 - secs / FALL).max(0.0);
    Pose {
        scale,
        turn: side * CROOKED * u * u,
        lift: ((scale - 1.0) / (FROM - 1.0)).clamp(0.0, 1.0),
        alpha: (secs / SHOW).min(1.0),
        rise: 0.0,
    }
}

/// A sticker taken off `secs` ago: it lifts away from the case (the lift quickens, it floats up a third of its height,
/// grows a little and turns the other way) and fades; gone (alpha 0) from [`PEEL`] on.
pub fn peel(secs: f32, side: f32) -> Pose {
    let u = (secs / PEEL.as_secs_f32()).clamp(0.0, 1.0);
    let quick = u * u;
    let fade = ((u - 0.4) / 0.6).clamp(0.0, 1.0);
    Pose { scale: 1.0 + 0.2 * quick, turn: -side * 0.12 * quick, lift: quick, alpha: 1.0 - fade * fade * (3.0 - 2.0 * fade), rise: 0.35 * quick }
}

/// One sticker on its way onto (or off) a case's front, as the game box paints it over the front.
pub struct Moving<'a> {
    /// The sticker turned to its place's tilt, as the case's own picture has it (`case_stickers::placed`).
    pub pic: &'a Pic,
    /// Where its top left is on the front.
    pub at: (i32, i32),
    pub pose: Pose,
}

impl Moving<'_> {
    /// Paints it over a front whose top left is at `front` on `c`: a shadow first, below and spread by the lift, then the
    /// sticker. `clip` keeps rows to (top, bottom).
    pub fn paint(&self, c: &mut Canvas, front: (i32, i32), clip: (i32, i32)) {
        let p = self.pose;
        if p.alpha <= 0.0 {
            return;
        }
        let (cx, cy) = ((front.0 + self.at.0) as f32 + self.pic.w as f32 / 2.0, (front.1 + self.at.1) as f32 + self.pic.h as f32 * (0.5 - p.rise));
        // The shadow: one pixel below at rest (the case's own picture paints that), a tenth of the sticker's height when let go.
        let drop = 1.0 + p.lift * self.pic.h as f32 * 0.1;
        let a = (p.alpha * 255.0).round();
        let shade = (90.0 + 40.0 * p.lift) * p.alpha;
        blit_pose(c, self.pic, cx, cy + drop, p.scale, p.turn, shade.round() as u32, Some(0x000000), clip);
        blit_pose(c, self.pic, cx, cy, p.scale, p.turn, a as u32, None, clip);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(ms: f32) -> Pose {
        press(ms / 1000.0, 1.0)
    }

    #[test]
    fn it_is_let_go_large_and_crooked_and_ends_sitting() {
        let p = at(0.0);
        assert!((p.scale - FROM).abs() < 1e-4 && (p.turn - CROOKED).abs() < 1e-4 && p.lift > 0.99, "{p:?}");
        assert_eq!(press(PRESS.as_secs_f32(), 1.0), Pose::SITTING);
        assert_eq!(press(5.0, -1.0), Pose::SITTING);
        assert!(Pose::SITTING.is_sitting());
        // The other tilt arrives the other way round.
        assert!((press(0.0, -1.0).turn + CROOKED).abs() < 1e-4);
    }

    #[test]
    fn it_falls_faster_and_faster_until_it_touches() {
        let step = |a: f32, b: f32| at(a).scale - at(b).scale;
        // Equal steps cover more distance each (an even acceleration), and the fall ends exactly on the case.
        let (d1, d2, d3) = (step(0.0, 30.0), step(30.0, 60.0), step(60.0, 90.0));
        assert!(d1 > 0.0 && d2 > d1 && d3 > d2, "{d1} {d2} {d3}");
        assert!((at(FALL * 1000.0).scale - 1.0).abs() < 1e-4);
        // Straight after the touch it does not bounce off at once: it is pressed in first.
        assert!(at(FALL * 1000.0 + 10.0).scale < 1.0);
    }

    #[test]
    fn the_touch_is_smooth_position_and_speed() {
        let h = 1e-4;
        let s = |t: f32| press_scale(t);
        let (before, after) = (FALL - h, FALL + h);
        assert!((s(before) - s(after)).abs() < 2e-3, "a jump in size at the touch");
        let (v_before, v_after) = ((s(FALL) - s(before)) / h, (s(after) - s(FALL)) / h);
        assert!((v_before - v_after).abs() / v_before.abs() < 0.02, "a kink in speed at the touch: {v_before} {v_after}");
    }

    #[test]
    fn it_is_pressed_in_a_few_percent_springs_back_once_and_rests() {
        let samples: Vec<f32> = (0..=320).map(|ms| at(ms as f32).scale).collect();
        let low = samples.iter().cloned().fold(f32::MAX, f32::min);
        assert!((0.90..=0.96).contains(&low), "pressed in to {low}");
        // One swing past its place and no more: count the crossings of 1.0 after the touch.
        let after: Vec<f32> = samples[(FALL * 1000.0) as usize + 1..].to_vec();
        let crossings = after.windows(2).filter(|w| (w[0] - 1.0) * (w[1] - 1.0) < 0.0).count();
        assert_eq!(crossings, 2, "down through its place, back up through it, then no more: {crossings}");
        let high = after.iter().cloned().fold(f32::MIN, f32::max);
        assert!((1.005..=1.04).contains(&high), "springs back past its place by {high}");
        // What is left when the animation is cut is under a third of a percent (under a pixel of a 100 px sticker).
        let last = samples[319];
        assert!((last - 1.0).abs() < 0.004, "left over at the end: {last}");
        // The wobble only dies down: each swing smaller than the one before.
        let mut peaks: Vec<f32> = Vec::new();
        for w in after.windows(3) {
            if (w[1] - w[0]) * (w[2] - w[1]) < 0.0 {
                peaks.push((w[1] - 1.0).abs());
            }
        }
        assert!(peaks.windows(2).all(|p| p[1] < p[0]), "{peaks:?}");
    }

    #[test]
    fn the_turn_and_the_shadow_are_gone_as_it_touches_and_it_shows_at_once() {
        let touch = at(FALL * 1000.0);
        assert!(touch.turn.abs() < 1e-4 && touch.lift < 1e-3, "{touch:?}");
        assert!(at(5.0).alpha < 0.5 && at(0.0).alpha == 0.0, "it does not pop in");
        assert!(at(SHOW * 1000.0).alpha >= 1.0 - 1e-6);
        for ms in 0..320 {
            let p = at(ms as f32);
            assert!((0.0..=1.0).contains(&p.lift) && (0.0..=1.0).contains(&p.alpha), "{ms} ms: {p:?}");
        }
        // After the touch it is flat on the case.
        assert!((((FALL * 1000.0) as i32 + 1)..320).all(|ms| at(ms as f32).turn == 0.0));
    }

    #[test]
    fn it_is_short_even_in_cozy() {
        let cozy = crate::motion::COZY_SLOWER;
        assert!(PRESS.as_secs_f32() * cozy <= 0.450 + 1e-6, "{:?} at {cozy}x", PRESS);
        assert!(PEEL < PRESS);
    }

    #[test]
    fn a_sticker_taken_off_lifts_and_fades_to_nothing() {
        let p = |ms: f32| peel(ms / 1000.0, 1.0);
        assert_eq!(p(0.0).scale, 1.0);
        assert_eq!(p(0.0).alpha, 1.0, "it starts exactly as it sat");
        assert_eq!(p(0.0).lift, 0.0);
        assert_eq!(p(PEEL.as_millis() as f32).alpha, 0.0);
        assert_eq!(p(1000.0).alpha, 0.0);
        let steps: Vec<Pose> = (0..=200).map(|ms| p(ms as f32)).collect();
        assert!(steps.windows(2).all(|w| w[1].alpha <= w[0].alpha + 1e-6 && w[1].lift >= w[0].lift - 1e-6 && w[1].scale >= w[0].scale - 1e-6));
        // It lifts slowly first and quickens (a corner coming up), and turns the other way from its tilt.
        assert!(p(50.0).lift - p(0.0).lift < p(150.0).lift - p(100.0).lift);
        assert!(p(100.0).turn < 0.0 && peel(0.1, -1.0).turn > 0.0);
        // It floats up, more and more, from its place; a sticker landing does not float.
        assert!(p(0.0).rise == 0.0 && steps.windows(2).all(|w| w[1].rise >= w[0].rise) && p(190.0).rise > 0.25);
        assert!((0..320).all(|ms| at(ms as f32).rise == 0.0));
    }

    fn sticker() -> Pic {
        let mut s = Pic::new(20, 14, 0);
        for y in 2..12 {
            for x in 2..18 {
                s.px[(y * 20 + x) as usize] = 0xffe0_4030;
            }
        }
        s
    }

    /// The canvas the box paints on: 0RGB pixels, the case's front under the sticker.
    fn sheet() -> Vec<u32> {
        vec![0x20_4060; 80 * 60]
    }

    #[test]
    fn sitting_it_paints_exactly_what_the_case_picture_has() {
        // The case's own picture (stuck on) against the base with the sticker moving in on top at rest.
        let s = sticker();
        let mut base = Pic::new(80, 60, 0xff20_4060);
        crate::case_stickers::stick_for_test(&mut base, &s, 30, 20);
        let mut buf = sheet();
        let mut c = Canvas::new(&mut buf, 80, 60);
        Moving { pic: &s, at: (30, 20), pose: Pose::SITTING }.paint(&mut c, (0, 0), (0, 60));
        let rgb: Vec<u32> = base.px.iter().map(|p| p & 0xff_ffff).collect();
        assert_eq!(buf, rgb);
    }

    #[test]
    fn let_go_it_is_larger_and_throws_a_shadow_and_a_gone_one_paints_nothing() {
        let s = sticker();
        let rest = {
            let mut buf = sheet();
            Moving { pic: &s, at: (30, 20), pose: Pose::SITTING }.paint(&mut Canvas::new(&mut buf, 80, 60), (0, 0), (0, 60));
            buf
        };
        let high = {
            let mut buf = sheet();
            Moving { pic: &s, at: (30, 20), pose: press(0.06, 1.0) }.paint(&mut Canvas::new(&mut buf, 80, 60), (0, 0), (0, 60));
            buf
        };
        let solid = |b: &[u32]| b.iter().filter(|p| **p == 0xe0_4030).count();
        assert!(solid(&high) > solid(&rest), "{} against {}", solid(&high), solid(&rest));
        assert!(high.iter().any(|p| *p != 0x20_4060 && *p != 0xe0_4030), "a shadow");
        let mut buf = sheet();
        Moving { pic: &s, at: (30, 20), pose: peel(1.0, 1.0) }.paint(&mut Canvas::new(&mut buf, 80, 60), (0, 0), (0, 60));
        assert_eq!(buf, sheet());
    }
}
