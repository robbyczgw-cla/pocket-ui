//! Saving you can see (Animations: Cozy, Motion Full): Save in the pause menu writes the state at once, then a copy of the
//! saved picture flies from the big preview, which stays as it is, and shrinks into its slot, the slot lights up as it lands,
//! a quiet click of the card going in sounds at that moment, and the game goes on. Nothing flickers: no frame has an empty
//! or dimmed preview (Fable's review, point 7). Nothing is drawn
//! that isn't there already: the picture is the one that was saved, the slot is the one in the row. The animation is only
//! a picture: the save command is sent at the press (`app_save_media::begin_save_fill`), nothing here waits for it to be
//! written, and any button or touch ends it and the game comes back right then (`App::skip_save_fill`).
//!
//! [`moment`] says what shows `since` the press, [`rect_at`] where the flying picture is, [`paint_landed`] draws the slot
//! with its picture, [`cue`] is the sound with its wait: the click is queued at the press and is due when the picture
//! lands, so a skip drops it (`ui_sound::forget_waiting`) and a key never plays it early.

use std::time::{Duration, Instant};

use crate::gfx::{mix, Canvas};
use crate::image::Image;
use crate::insert_scene::Cue;
use crate::ui_sound::{self, Sound};

/// The picture flies from the preview into its slot.
pub const FLY: Duration = Duration::from_millis(360);
/// The slot lights up as it lands, the last part of the picture.
pub const GLOW: Duration = Duration::from_millis(180);
/// The whole animation: the game comes back after this long at the latest.
pub const LEN: Duration = Duration::from_millis(540);

/// A little before its moment, the output takes about this long to play what it is given (`insert_scene`'s lead).
const SOUND_LEAD: Duration = Duration::from_millis(10);

/// What shows at one moment, each 0 to 1: how far the copy has flown (0 at the preview, 1 in its slot), how bright the
/// slot's glow is, and how far the slot is lifted.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Moment {
    pub fly: f32,
    pub glow: f32,
    pub pop: f32,
}

/// What shows `since` the press; None once it is over.
pub fn moment(since: Duration) -> Option<Moment> {
    if since >= LEN {
        return None;
    }
    if since < FLY {
        // Slow to leave, quick across, slow to settle: easy to follow.
        let x = since.as_secs_f32() / FLY.as_secs_f32();
        let fly = x * x * (3.0 - 2.0 * x);
        return Some(Moment { fly, glow: 0.0, pop: 0.0 });
    }
    let x = (since - FLY).as_secs_f32() / GLOW.as_secs_f32();
    Some(Moment { fly: 1.0, glow: (1.0 - x).powi(2), pop: 4.0 * x * (1.0 - x) })
}

/// The card going in, quietly, due when the picture lands. The case sounds' switch applies (`ui_sound::is_case_sound`).
pub fn cue() -> Cue {
    Cue { sound: Sound::CardIn, after: FLY.saturating_sub(ui_sound::hit_at(Sound::CardIn) + SOUND_LEAD), db: ui_sound::QUICK_DB }
}

/// A Save being shown: which slot (0-based) and when it was pressed. Held in the pause menu while it plays.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Saving {
    pub slot: usize,
    pub at: Instant,
}

impl Saving {
    pub fn new(slot: usize, at: Instant) -> Saving {
        Saving { slot, at }
    }

    /// What shows at `now`; None when it is over.
    pub fn moment(&self, now: Instant) -> Option<Moment> {
        moment(now.saturating_duration_since(self.at))
    }
}

/// Where the slot's block stands and how big, for the slim row (`save_media::paint_row`'s measures): (x, y, size) of
/// slot `i` of `n` in a row centred on (cx, cy), blocks `b` apart by `gap`.
pub fn row_block(cx: i32, cy: i32, b: i32, gap: i32, n: usize, i: usize) -> (i32, i32, i32) {
    let n = n as i32;
    let x0 = cx - (n * b + (n - 1) * gap) / 2;
    (x0 + i as i32 * (b + gap), cy - b / 2, b)
}

/// The picture's place in the slot's block: as large as the block's inside leaves it, in its shape, in the middle.
pub fn landing(picture: &Image, (x, y, b): (i32, i32, i32)) -> (i32, i32, i32, i32) {
    let line = (b / 12).max(2);
    let inner = b - 2 * line;
    let (w, h) = picture.fit(inner, inner);
    (x + line + (inner - w) / 2, y + line + (inner - h) / 2, w, h)
}

/// Where the flying picture is at `fly` (0 to 1): from its place in the preview (`from`: x, y, w, h) to its place in the slot
/// (`to`), straight, its size changing with it.
pub fn rect_at(from: (i32, i32, i32, i32), to: (i32, i32, i32, i32), fly: f32) -> (i32, i32, i32, i32) {
    let l = |a: i32, b: i32| a + ((b - a) as f32 * fly).round() as i32;
    (l(from.0, to.0), l(from.1, to.1), l(from.2, to.2).max(1), l(from.3, to.3).max(1))
}

/// How many pixels bigger the slot is while it lights up: nothing when the picture lands and when it is over, 0.6 of its size
/// at the middle.
pub fn grow(b: i32, pop: f32) -> i32 {
    (b as f32 * 0.6 * pop).round().max(0.0) as i32
}

/// What a `Moment` asks of the screen over the preview and the row. `preview`: the preview's picture place (x, y, w, h);
/// `block`: the slot's block (x, y, size) in the row. The preview itself is left as the menu painted it; this paints a copy of
/// the picture flying from it while it flies, and the slot with its picture and its glow after.
pub fn paint(c: &mut Canvas, picture: &Image, preview: (i32, i32, i32, i32), block: (i32, i32, i32), m: Moment) {
    if m.fly < 1.0 {
        let (x, y, w, h) = rect_at(preview, landing(picture, block), m.fly);
        c.blit_scaled(picture, x, y, w, h);
        return;
    }
    paint_landed(c, block, picture, m);
}

/// The slot with the picture in it, lifted by `m.pop` and framed by the glow.
pub fn paint_landed(c: &mut Canvas, (x, y, b): (i32, i32, i32), picture: &Image, m: Moment) {
    let g = grow(b, m.pop);
    let (x, y, b) = (x - g / 2, y - g / 2, b + g);
    let line = (b / 12).max(2);
    let inner = b - 2 * line;
    c.rect(x + line, y + line, inner, inner, 0x0c0c0c);
    let (w, h) = picture.fit(inner, inner);
    c.blit_scaled(picture, x + line + (inner - w) / 2, y + line + (inner - h) / 2, w, h);
    if m.glow > 0.0 {
        let t = (m.glow * 255.0) as u32;
        let r = (b / 7).max(3);
        c.frame(x - r - 2, y - r - 2, b + 2 * r + 4, b + 2 * r + 4, 3, mix(0x000000, 0xffffff, t));
        c.frame(x - 1, y - 1, b + 2, b + 2, 2, mix(0x000000, 0xfff1c8, t));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    fn picture() -> Image {
        // Top half red, bottom half blue.
        let (w, h) = (20u32, 20u32);
        let px = (0..w * h).map(|i| if i / w < h / 2 { 0xff0000 } else { 0x0000ff }).collect();
        Image { w, h, px }
    }

    #[test]
    fn it_is_short_and_ends() {
        assert!(LEN <= ms(600), "Alex: at most 600 ms");
        assert_eq!(LEN, FLY + GLOW);
        assert!(moment(LEN - ms(1)).is_some());
        assert!(moment(LEN).is_none() && moment(ms(5_000)).is_none());
    }

    #[test]
    fn the_copy_flies_in_order_lands_and_only_then_glows() {
        assert_eq!(moment(ms(0)).unwrap(), Moment { fly: 0.0, glow: 0.0, pop: 0.0 }, "it starts where it was: in the preview");
        let mut last = -1.0;
        for t in (0..FLY.as_millis() as u64).step_by(10) {
            let m = moment(ms(t)).unwrap();
            assert!(m.fly >= last, "never flying back, at {t} ms");
            assert_eq!((m.glow, m.pop), (0.0, 0.0), "the slot stays dark while it flies ({t} ms)");
            last = m.fly;
        }
        let landed = moment(FLY).unwrap();
        assert_eq!((landed.fly, landed.glow, landed.pop), (1.0, 1.0, 0.0), "landed: glowing brightest, not yet lifted");
        let mut glow = 2.0;
        for t in (FLY.as_millis() as u64..LEN.as_millis() as u64).step_by(10) {
            let m = moment(ms(t)).unwrap();
            assert_eq!(m.fly, 1.0);
            assert!(m.glow <= glow, "the glow only fades ({t} ms)");
            glow = m.glow;
        }
        let end = moment(LEN - ms(1)).unwrap();
        assert!(end.glow < 0.01 && end.pop < 0.05, "at the end: glow out, slot at its size: {end:?}");
    }

    #[test]
    fn the_click_is_due_when_the_picture_lands_and_never_before() {
        let cue = cue();
        assert_eq!(cue.sound, Sound::CardIn);
        assert!(cue.db < 0.0, "quiet");
        let hit = cue.after + ui_sound::hit_at(cue.sound);
        assert!(hit <= FLY && FLY - hit <= ms(20), "{hit:?} against {FLY:?}");
        assert!(cue.after > Duration::ZERO, "not at the press");
        assert!(ui_sound::is_case_sound(cue.sound), "the Case sounds switch silences it");
    }

    #[test]
    fn saving_follows_the_clock_it_is_given() {
        let t0 = Instant::now();
        let s = Saving::new(2, t0);
        assert_eq!(s.moment(t0).unwrap().fly, 0.0);
        assert_eq!(s.moment(t0 + ms(100)), moment(ms(100)));
        assert!(s.moment(t0 + LEN).is_none());
        assert_eq!(s.moment(t0.checked_sub(ms(50)).unwrap()).unwrap().fly, 0.0);
    }

    #[test]
    fn the_row_block_is_where_the_row_paints_it() {
        let (x, y, b) = row_block(400, 100, 30, 12, 8, 0);
        assert_eq!((x, y, b), (400 - 162, 85, 30));
        assert_eq!(row_block(400, 100, 30, 12, 8, 7).0, 400 - 162 + 7 * 42);
    }

    #[test]
    fn the_flying_picture_goes_from_the_preview_to_the_slot() {
        let from = (100, 40, 600, 450);
        let img = picture();
        let to = landing(&img, (700, 520, 30));
        assert!(to.2 <= 30 && to.3 <= 30 && to.0 >= 700 && to.1 >= 520, "inside the block: {to:?}");
        assert_eq!(rect_at(from, to, 0.0), from);
        assert_eq!(rect_at(from, to, 1.0), to);
        let mut last = from.2;
        for i in 0..=20 {
            let r = rect_at(from, to, i as f32 / 20.0);
            assert!(r.2 <= last, "only shrinking: {r:?}");
            last = r.2;
        }
        let mid = rect_at(from, to, 0.5);
        assert!(mid.2 > 100 && mid.2 < 600, "big enough half way to follow with the eye: {mid:?}");
    }

    #[test]
    fn the_slot_lifts_as_it_lights_up_and_is_its_own_size_before_and_after() {
        assert_eq!((grow(30, 0.0), grow(30, 1.0)), (0, 18));
        assert_eq!(moment(FLY).unwrap().pop, 0.0, "landing is at its size");
        let lit = |pop: f32| {
            let img = picture();
            let mut px = vec![0x111111u32; 100 * 100];
            let mut c = Canvas::new(&mut px, 100, 100);
            paint_landed(&mut c, (35, 35, 30), &img, Moment { fly: 1.0, glow: 0.0, pop });
            px.iter().filter(|p| **p != 0x111111).count()
        };
        assert!(lit(1.0) > lit(0.0) * 2, "{} {}", lit(0.0), lit(1.0));
    }

    /// Fable's review (point 7): the preview went black between 300 and 360 ms and came back dimmed, which reads as a flicker. The
    /// picture stays where it is and a copy flies: no frame of the whole sequence has a black or darkened preview.
    #[test]
    fn the_preview_never_goes_black_or_dim_while_a_copy_flies() {
        let img = picture();
        let preview = (10, 10, 40, 40);
        let base_colour = 0x336699u32;
        let mut base = vec![0x00ff00u32; 100 * 100];
        Canvas::new(&mut base, 100, 100).rect(10, 10, 40, 40, base_colour);
        let mut t = 0;
        let mut flew = false;
        while let Some(m) = moment(ms(t)) {
            let mut px = base.clone();
            paint(&mut Canvas::new(&mut px, 100, 100), &img, preview, (60, 70, 20), m);
            for y in preview.1..preview.1 + preview.3 {
                for x in preview.0..preview.0 + preview.2 {
                    let p = px[(y * 100 + x) as usize];
                    assert!(p == base_colour || p == 0xff0000 || p == 0x0000ff, "{t} ms: the preview at ({x}, {y}) is {p:06x}: the picture stays, only the copy covers it");
                }
            }
            // The copy really is in flight and away from the preview at some moment.
            flew |= px[(70 * 100 + 62) as usize] != 0x00ff00 || px[(72 * 100 + 65) as usize] != 0x00ff00;
            t += 16;
        }
        assert!(flew, "the copy reaches its slot");
    }

    #[test]
    fn at_the_press_the_copy_sits_exactly_on_the_preview_and_once_landed_the_slot_holds_it() {
        let img = picture();
        let preview = (10, 10, 40, 40);
        let base = vec![0x00ff00u32; 100 * 100];
        let shot = |m: Moment| {
            let mut px = base.clone();
            paint(&mut Canvas::new(&mut px, 100, 100), &img, preview, (60, 70, 20), m);
            px
        };
        let at = |px: &[u32], x: i32, y: i32| px[(y * 100 + x) as usize];
        assert_eq!(at(&shot(moment(ms(0)).unwrap()), 12, 12), 0xff0000, "the copy covers the preview's place at the press: nothing jumps");
        let landed = shot(moment(FLY + ms(10)).unwrap());
        let (lx, ly, lw, lh) = landing(&img, (60, 70, 20));
        assert_ne!(at(&landed, lx + lw / 2, ly + lh / 4), 0x00ff00, "the slot holds the picture");
    }
}
