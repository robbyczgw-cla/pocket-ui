//! Nothing floats (18.11.2026: "wir sind skeuomorph, da kann nichts schweben im Regal"): every object a scene
//! paints on a shelf, a board or a floor rests on one of them, on another object, or is held by a hand or a slot, and
//! no two objects are inside each other. The painters say where they put each object, as a rectangle with its
//! baseline at the bottom, and where the surfaces are (`put`, `surface`), only while a check records (`record`): a
//! frame costs nothing otherwise. `check` reads the model, not pixels, so a whole row with every selection and every
//! moment of its animations is checked in milliseconds (tests/scene_rests.rs).
//!
//! The deliberate exceptions are named where they are recorded (`Rests::Raised`, `Rests::Held`): the selected standing
//! case lifted out of the Shelf look's row, a case on its way between a shelf and the showcase, a case in the hand
//! that puts its game into the console.

use std::cell::RefCell;

/// How an object is meant to rest.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Rests {
    /// On a surface or on another object: checked.
    Settled,
    /// Lifted on purpose (the selection): its name says why. Not checked for resting nor for touching others.
    Raised(&'static str),
    /// Held by a hand or a slot: its name says by what. Not checked.
    Held(&'static str),
}

/// One object as painted: what it is (for the message), its box (x, y, w, h; y + h its baseline) and whether its box is
/// its outline (`solid`: a leaning case's box is larger than the case, so only its baseline counts).
#[derive(Clone, Debug, PartialEq)]
pub struct Obj {
    pub what: String,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub rests: Rests,
    pub solid: bool,
}

/// A surface things stand on: a board's or a floor's top at `y`, from `x0` to `x1`, `depth` lines deep (a board seen a
/// little from above: a case standing a little forward stands lower on it).
#[derive(Clone, Debug, PartialEq)]
pub struct Surface {
    pub what: &'static str,
    pub y: i32,
    pub depth: i32,
    pub x0: i32,
    pub x1: i32,
}

#[derive(Default)]
struct Rec {
    on: bool,
    origin: (i32, i32),
    objs: Vec<Obj>,
    surfaces: Vec<Surface>,
}

thread_local! {
    static REC: RefCell<Rec> = RefCell::new(Rec::default());
}

/// Starts (or stops) recording on this thread, with an empty model.
pub fn record(on: bool) {
    REC.with(|r| *r.borrow_mut() = Rec { on, ..Default::default() });
}

/// Whether a check records: the painters ask before they work anything out.
pub fn recording() -> bool {
    REC.with(|r| r.borrow().on)
}

/// Where (0, 0) of the picture being painted is on the screen (a tile of the Shelf look's row), for `put`.
pub fn set_origin(x: i32, y: i32) {
    REC.with(|r| r.borrow_mut().origin = (x, y));
}

/// An object painted at `(x, y)` of the current picture (see `set_origin`). The same object twice (a case cut by two
/// tiles) counts once.
pub fn put(what: impl Into<String>, x: i32, y: i32, w: i32, h: i32, rests: Rests, solid: bool) {
    REC.with(|r| {
        let mut r = r.borrow_mut();
        if !r.on || w <= 0 || h <= 0 {
            return;
        }
        let (ox, oy) = r.origin;
        let obj = Obj { what: what.into(), x: x + ox, y: y + oy, w, h, rests, solid };
        if !r.objs.contains(&obj) {
            r.objs.push(obj);
        }
    });
}

/// An object painted as picture `pic` at `(x, y)`: its box is the picture's visible pixels (a sprite's see-through
/// rows under its foot would hide a gap).
pub fn put_pic(what: impl Into<String>, pic: &crate::cabinet_art::Pic, x: i32, y: i32, rests: Rests, solid: bool) {
    put_pic_in(what, pic, x, y, rests, solid, (i32::MIN, i32::MAX));
}

/// [`put_pic`] painted only between lines `clip` (a shelf behind the showcase): what is cut away is not there; an
/// object whose foot is cut away is left out (nothing of it can be seen to float).
pub fn put_pic_in(what: impl Into<String>, pic: &crate::cabinet_art::Pic, x: i32, y: i32, rests: Rests, solid: bool, clip: (i32, i32)) {
    if !recording() {
        return;
    }
    let seen = |i: usize| pic.px[i] >> 24 > 0x40;
    let (w, h) = (pic.w as usize, pic.h as usize);
    let rows: Vec<usize> = (0..h).filter(|&yy| (0..w).any(|xx| seen(yy * w + xx))).collect();
    let cols: Vec<usize> = (0..w).filter(|&xx| (0..h).any(|yy| seen(yy * w + xx))).collect();
    let (Some(&y0), Some(&y1), Some(&x0), Some(&x1)) = (rows.first(), rows.last(), cols.first(), cols.last()) else { return };
    let (top, bottom) = (y + y0 as i32, y + y1 as i32 + 1);
    let (oy, _) = REC.with(|r| (r.borrow().origin.1, ()));
    if bottom + oy > clip.1 || bottom + oy <= clip.0 {
        return;
    }
    let top = top.max(clip.0 - oy);
    put(what, x + x0 as i32, top, (x1 - x0 + 1) as i32, bottom - top, rests, solid);
}

/// Takes back what was recorded as `what` (a case painted again elsewhere in the same frame: lifted out of its tile).
pub fn remove(what: &str) {
    REC.with(|r| r.borrow_mut().objs.retain(|o| o.what != what));
}

/// A surface on the screen (not moved by `set_origin`).
pub fn surface(what: &'static str, y: i32, depth: i32, x0: i32, x1: i32) {
    REC.with(|r| {
        let mut r = r.borrow_mut();
        if r.on {
            let s = Surface { what, y, depth, x0, x1 };
            if !r.surfaces.contains(&s) {
                r.surfaces.push(s);
            }
        }
    });
}

/// The model recorded since `record` or the last `take`, emptied; recording goes on.
pub fn take() -> (Vec<Obj>, Vec<Surface>) {
    REC.with(|r| {
        let mut r = r.borrow_mut();
        (std::mem::take(&mut r.objs), std::mem::take(&mut r.surfaces))
    })
}

/// How far (pixels) a baseline may be off what it rests on, and two boxes may reach into each other (rounding, a
/// shadow's pixel).
pub const TOLERANCE: i32 = 2;

fn overlap(a0: i32, a1: i32, b0: i32, b1: i32) -> i32 {
    (a1.min(b1) - a0.max(b0)).max(0)
}

/// What is wrong in a model: every settled object that rests on nothing (its baseline on no surface and on no other
/// object under at least half of it), and every two solid objects inside each other. Empty: all is well.
pub fn check(objs: &[Obj], surfaces: &[Surface]) -> Vec<String> {
    let t = TOLERANCE;
    let mut wrong = Vec::new();
    let settled = |o: &Obj| o.rests == Rests::Settled;
    for (k, o) in objs.iter().enumerate().filter(|(_, o)| settled(o)) {
        let base = o.y + o.h;
        let half = (o.w + 1) / 2;
        let on_surface = surfaces.iter().any(|s| base >= s.y - t && base <= s.y + s.depth + t && overlap(o.x, o.x + o.w, s.x0, s.x1) >= half);
        let on_object = objs
            .iter()
            .enumerate()
            .any(|(j, u)| j != k && settled(u) && u.solid && (base - u.y).abs() <= t && overlap(o.x, o.x + o.w, u.x, u.x + u.w) >= half);
        if !on_surface && !on_object {
            let near = surfaces.iter().map(|s| format!("{} at y {}", s.what, s.y)).collect::<Vec<_>>().join(", ");
            wrong.push(format!("{} floats: its foot at y {base} (x {}..{}) rests on nothing ({near})", o.what, o.x, o.x + o.w));
        }
    }
    for (k, a) in objs.iter().enumerate().filter(|(_, o)| settled(o) && o.solid) {
        for b in objs[k + 1..].iter().filter(|o| settled(o) && o.solid) {
            let (dx, dy) = (overlap(a.x, a.x + a.w, b.x, b.x + b.w), overlap(a.y, a.y + a.h, b.y, b.y + b.h));
            if dx > t && dy > t {
                wrong.push(format!("{} and {} are inside each other ({dx} x {dy} px)", a.what, b.what));
            }
        }
    }
    wrong
}

#[cfg(test)]
mod tests {
    use super::*;

    fn obj(what: &str, x: i32, y: i32, w: i32, h: i32) -> Obj {
        Obj { what: what.into(), x, y, w, h, rests: Rests::Settled, solid: true }
    }

    #[test]
    fn a_case_on_the_board_or_on_another_rests_and_one_in_the_air_floats() {
        let board = [Surface { what: "board", y: 100, depth: 4, x0: 0, x1: 500 }];
        let stack = [obj("bottom", 10, 80, 100, 20), obj("top", 30, 60, 60, 20)];
        assert!(check(&stack, &board).is_empty(), "{:?}", check(&stack, &board));
        let lifted = [obj("bottom", 10, 80, 100, 20), obj("top", 30, 40, 60, 20)];
        assert_eq!(check(&lifted, &board).len(), 1, "a case over the stack floats");
        // A case standing a little forward stands lower, on the board's depth.
        assert!(check(&[obj("forward", 10, 3, 20, 100)], &board).is_empty());
        assert!(!check(&[obj("sunk", 10, 10, 20, 100)], &board).is_empty(), "under the board's front edge");
        // Held on purpose: not checked.
        let held = Obj { rests: Rests::Held("hand"), ..obj("in the hand", 0, 0, 10, 10) };
        assert!(check(&[held], &board).is_empty());
        // Resting on only a corner of the one under it is not resting.
        assert!(!check(&[obj("bottom", 10, 80, 100, 20), obj("off", 95, 60, 60, 20)], &board).is_empty());
    }

    #[test]
    fn two_cases_in_one_place_are_inside_each_other_but_touching_is_fine() {
        let board = [Surface { what: "board", y: 100, depth: 4, x0: 0, x1: 500 }];
        assert!(check(&[obj("a", 10, 0, 30, 100), obj("b", 40, 0, 30, 100)], &board).is_empty());
        assert_eq!(check(&[obj("a", 10, 0, 30, 100), obj("b", 30, 0, 30, 100)], &board).len(), 1);
        // A leaning case's box is no outline: only its foot counts.
        let lean = Obj { solid: false, ..obj("leaning", 30, 0, 30, 100) };
        assert!(check(&[obj("a", 10, 0, 30, 100), lean], &board).is_empty());
    }

    #[test]
    fn nothing_is_recorded_unless_a_check_records() {
        record(false);
        put("x", 0, 0, 10, 10, Rests::Settled, true);
        assert!(take().0.is_empty());
        record(true);
        set_origin(5, 7);
        put("x", 0, 0, 10, 10, Rests::Settled, true);
        put("x", 0, 0, 10, 10, Rests::Settled, true);
        let (objs, _) = take();
        assert_eq!((objs.len(), objs[0].x, objs[0].y), (1, 5, 7));
        record(false);
    }
}
