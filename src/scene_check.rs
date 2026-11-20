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
//!
//! What hangs on a wall (`Rests::Hung`: the window, `room_window`) hangs inside a wall the painter recorded (`wall`),
//! whole, and nothing standing is in front of it unless that wall says things pass in front of it (the Cabinet's wall
//! over the top board, where the cases come out into the showcase); two things hung never overlap.

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
    /// Hung on a wall: inside a recorded wall (`wall`), whole; clear of what stands and of other hung things.
    Hung,
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
    /// Which layer of the picture it is in (`new_layer`): a scene in front of a dimmed page is a world of its own.
    pub layer: u32,
}

/// A surface things stand on: a board's or a floor's top at `y`, from `x0` to `x1`, `depth` lines deep (a board seen a
/// little from above: a case standing a little forward stands lower on it).
#[derive(Clone, Debug, PartialEq)]
pub struct Surface {
    pub layer: u32,
    pub what: &'static str,
    pub y: i32,
    pub depth: i32,
    pub x0: i32,
    pub x1: i32,
    /// A wall, not a surface to stand on: its bottom (it reaches from `y` down to here) and what may stand in front of
    /// what hangs on it (objects whose name has these words: the cases out in the Cabinet's showcase).
    pub wall: Option<(i32, Option<&'static str>)>,
}

#[derive(Default)]
struct Rec {
    on: bool,
    layer: u32,
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

/// From here on the frame paints a scene in front of what is under it (the way into the console over the dimmed
/// shelf): its objects rest on and touch only each other and its own surfaces.
pub fn new_layer() {
    REC.with(|r| r.borrow_mut().layer += 1);
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
        let obj = Obj { what: what.into(), x: x + ox, y: y + oy, w, h, rests, solid, layer: r.layer };
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
    let oy = REC.with(|r| r.borrow().origin.1);
    if bottom + oy > clip.1 || bottom + oy <= clip.0 {
        return;
    }
    let top = top.max(clip.0.saturating_sub(oy));
    put(what, x + x0 as i32, top, (x1 - x0 + 1) as i32, bottom - top, rests, solid);
}

/// An object painted as several pictures over each other at `(x, y)` (a console's body and the layer over its slot):
/// its box is their visible pixels together.
pub fn put_pics(what: impl Into<String>, pics: &[&crate::cabinet_art::Pic], x: i32, y: i32, rests: Rests, solid: bool) {
    if !recording() {
        return;
    }
    let mut b: Option<(i32, i32, i32, i32)> = None;
    for pic in pics {
        let (w, h) = (pic.w as usize, pic.h as usize);
        let seen = |i: usize| pic.px[i] >> 24 > 0x40;
        let rows: Vec<usize> = (0..h).filter(|&yy| (0..w).any(|xx| seen(yy * w + xx))).collect();
        let cols: Vec<usize> = (0..w).filter(|&xx| (0..h).any(|yy| seen(yy * w + xx))).collect();
        if let (Some(&y0), Some(&y1), Some(&x0), Some(&x1)) = (rows.first(), rows.last(), cols.first(), cols.last()) {
            let r = (x0 as i32, y0 as i32, x1 as i32 + 1, y1 as i32 + 1);
            b = Some(b.map_or(r, |o| (o.0.min(r.0), o.1.min(r.1), o.2.max(r.2), o.3.max(r.3))));
        }
    }
    if let Some((x0, y0, x1, y1)) = b {
        put(what, x + x0, y + y0, x1 - x0, y1 - y0, rests, solid);
    }
}

/// Takes back what was recorded as `what` (a case painted again elsewhere in the same frame: lifted out of its tile).
pub fn remove(what: &str) {
    REC.with(|r| r.borrow_mut().objs.retain(|o| o.what != what));
}

/// A surface on the screen (not moved by `set_origin`).
pub fn surface(what: &'static str, y: i32, depth: i32, x0: i32, x1: i32) {
    add_surface(Surface { layer: 0, what, y, depth, x0, x1, wall: None }, false);
}

/// A surface in the current picture (moved by `set_origin`, as `put`): a window's sill in a tile of the Shelf look.
pub fn surface_here(what: &'static str, y: i32, depth: i32, x0: i32, x1: i32) {
    add_surface(Surface { layer: 0, what, y, depth, x0, x1, wall: None }, true);
}

/// A wall things may hang on (`Rests::Hung`), from `(x0, y0)` to `(x1, y1)` of the current picture (moved by
/// `set_origin`); `in_front`: what may stand in front of what hangs on it, by words in its name (", out": the cases out
/// in the Cabinet's showcase), nothing else.
pub fn wall(what: &'static str, x0: i32, y0: i32, x1: i32, y1: i32, in_front: Option<&'static str>) {
    add_surface(Surface { layer: 0, what, y: y0, depth: 0, x0, x1, wall: Some((y1, in_front)) }, true);
}

fn add_surface(mut s: Surface, here: bool) {
    REC.with(|r| {
        let mut r = r.borrow_mut();
        if r.on {
            s.layer = r.layer;
            if here {
                let (ox, oy) = r.origin;
                s.x0 += ox;
                s.x1 += ox;
                s.y += oy;
                s.wall = s.wall.map(|(y1, f)| (y1 + oy, f));
            }
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
        r.layer = 0;
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
/// A case back in its row on the shelf, which stands behind the stage, painted over the console in front of it (ritual
/// 19.11.: Alex saw a GBA's case go back into its row through the GBA, the SNES's through the SNES). Overlapping
/// boxes: the case must be under the console there.
pub fn case_over_console(objs: &[Obj]) -> Option<String> {
    let case = objs.iter().find(|o| o.what == "the case back in its row")?;
    let console = objs.iter().find(|o| {
        o.what.starts_with("the console (") && o.x < case.x + case.w && case.x < o.x + o.w && o.y < case.y + case.h && case.y < o.y + o.h
    })?;
    Some(format!("the case back in its row is painted over {}, which stands in front of it", console.what))
}

pub fn check(objs: &[Obj], surfaces: &[Surface]) -> Vec<String> {
    let t = TOLERANCE;
    let mut wrong = Vec::new();
    let settled = |o: &Obj| o.rests == Rests::Settled;
    for (k, o) in objs.iter().enumerate().filter(|(_, o)| settled(o)) {
        let base = o.y + o.h;
        let half = (o.w + 1) / 2;
        let on_surface = surfaces.iter().any(|s| s.wall.is_none() && s.layer == o.layer && base >= s.y - t && base <= s.y + s.depth + t && overlap(o.x, o.x + o.w, s.x0, s.x1) >= half);
        let on_object = objs
            .iter()
            .enumerate()
            .any(|(j, u)| j != k && u.layer == o.layer && settled(u) && u.solid && (base - u.y).abs() <= t && overlap(o.x, o.x + o.w, u.x, u.x + u.w) >= half);
        if !on_surface && !on_object {
            let near = surfaces.iter().map(|s| format!("{} at y {}", s.what, s.y)).collect::<Vec<_>>().join(", ");
            wrong.push(format!("{} floats: its foot at y {base} (x {}..{}) rests on nothing ({near})", o.what, o.x, o.x + o.w));
        }
    }
    // What hangs: inside a wall, whole; clear of what stands (unless things pass in front of that wall) and of each other.
    let hung = |o: &Obj| o.rests == Rests::Hung;
    for (k, o) in objs.iter().enumerate().filter(|(_, o)| hung(o)) {
        let inside = |s: &&Surface| {
            s.wall.is_some_and(|(y1, _)| s.layer == o.layer && o.x >= s.x0 - t && o.x + o.w <= s.x1 + t && o.y >= s.y - t && o.y + o.h <= y1 + t)
        };
        let Some(wall) = surfaces.iter().find(inside) else {
            let walls = surfaces.iter().filter(|s| s.wall.is_some()).map(|s| format!("{} {}..{} x {}..{}", s.what, s.x0, s.x1, s.y, s.wall.unwrap().0)).collect::<Vec<_>>().join(", ");
            wrong.push(format!("{} hangs on no wall: {}..{} x {}..{} ({walls})", o.what, o.x, o.x + o.w, o.y, o.y + o.h));
            continue;
        };
        let in_front = wall.wall.and_then(|(_, f)| f);
        // Each pair of hung things once (the later one), what stands always.
        for (_, u) in objs.iter().enumerate().filter(|&(j, u)| j != k && u.layer == o.layer && if hung(u) { j > k } else { settled(u) && u.solid && !in_front.is_some_and(|w| u.what.contains(w)) }) {
            let (dx, dy) = (overlap(o.x, o.x + o.w, u.x, u.x + u.w), overlap(o.y, o.y + o.h, u.y, u.y + u.h));
            if dx > t && dy > t {
                wrong.push(format!("{} hangs behind {} ({dx} x {dy} px)", o.what, u.what));
            }
        }
    }
    for (k, a) in objs.iter().enumerate().filter(|(_, o)| settled(o) && o.solid) {
        for b in objs[k + 1..].iter().filter(|o| settled(o) && o.solid && o.layer == a.layer) {
            let (dx, dy) = (overlap(a.x, a.x + a.w, b.x, b.x + b.w), overlap(a.y, a.y + a.h, b.y, b.y + b.h));
            if dx > t && dy > t {
                wrong.push(format!("{} and {} are inside each other ({dx} x {dy} px)", a.what, b.what));
            }
        }
    }
    wrong.extend(case_over_console(objs));
    wrong
}


/// The same console twice in one frame (ritual inventory 19.11.2026: every Cabinet start showed the console small beside
/// the TV and big in front, the handheld in the TV's place and in front): the scene's console, once any of it is above the
/// stage's floor (before that it is still under the screen's edge), with the Cabinet's.
pub fn console_twice(objs: &[Obj], surfaces: &[Surface]) -> Option<String> {
    let floor = surfaces.iter().find(|s| s.what == "the stage's floor").map_or(i32::MAX, |s| s.y);
    let scene = objs.iter().find(|o| o.what.starts_with("the console (") && o.y < floor)?;
    let cabinet = objs.iter().find(|o| o.what == "the console beside the TV" || o.what == "the handheld in the TV's place")?;
    Some(format!("the console stands twice: {} and {}", scene.what, cabinet.what))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn obj(what: &str, x: i32, y: i32, w: i32, h: i32) -> Obj {
        Obj { what: what.into(), x, y, w, h, rests: Rests::Settled, solid: true, layer: 0 }
    }

    #[test]
    fn pictures_in_shifted_tiles_keep_their_visible_bounds() {
        let pic = crate::cabinet_art::Pic::new(4, 10, 0xff00_0000);
        record(true);
        for origin in [200, -200, 0] {
            set_origin(10, origin);
            put_pic("whole", &pic, 3, 5, Rests::Settled, true);
            put_pic_in("top clipped", &pic, 3, 5, Rests::Settled, true, (origin + 9, origin + 15));
            put_pic_in("foot clipped", &pic, 3, 5, Rests::Settled, true, (origin, origin + 14));
            assert_eq!(take().0, [
                obj("whole", 13, origin + 5, 4, 10),
                obj("top clipped", 13, origin + 9, 4, 6),
            ]);
        }
        record(false);
    }

    #[test]
    fn a_case_on_the_board_or_on_another_rests_and_one_in_the_air_floats() {
        let board = [Surface { layer: 0, what: "board", y: 100, depth: 4, x0: 0, x1: 500, wall: None }];
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
        let board = [Surface { layer: 0, what: "board", y: 100, depth: 4, x0: 0, x1: 500, wall: None }];
        assert!(check(&[obj("a", 10, 0, 30, 100), obj("b", 40, 0, 30, 100)], &board).is_empty());
        assert_eq!(check(&[obj("a", 10, 0, 30, 100), obj("b", 30, 0, 30, 100)], &board).len(), 1);
        // A leaning case's box is no outline: only its foot counts.
        let lean = Obj { solid: false, ..obj("leaning", 30, 0, 30, 100) };
        assert!(check(&[obj("a", 10, 0, 30, 100), lean], &board).is_empty());
    }

    #[test]
    fn what_hangs_hangs_whole_on_a_wall_and_clear_of_what_stands() {
        let wall = |x0, y0, x1, y1, f: bool| Surface { layer: 0, what: "wall", y: y0, depth: 0, x0, x1, wall: Some((y1, f.then_some(", out"))) };
        let board = Surface { layer: 0, what: "board", y: 200, depth: 4, x0: 0, x1: 500, wall: None };
        let win = Obj { rests: Rests::Hung, ..obj("window", 100, 50, 60, 60) };
        assert!(check(std::slice::from_ref(&win), &[wall(80, 20, 300, 190, false), board.clone()]).is_empty());
        // Cut by the wall's end, or on no wall at all.
        assert_eq!(check(std::slice::from_ref(&win), &[wall(120, 20, 300, 190, false)]).len(), 1);
        assert_eq!(check(std::slice::from_ref(&win), std::slice::from_ref(&board)).len(), 1, "a board is no wall");
        // A case standing in front of it: wrong beside the row, fine where cases pass in front (the showcase).
        let case = obj("case, out", 140, 100, 30, 100);
        assert_eq!(check(&[win.clone(), case.clone()], &[wall(80, 20, 300, 190, false), board.clone()]).len(), 1);
        assert!(check(&[win.clone(), case], &[wall(80, 20, 300, 190, true), board.clone()]).is_empty());
        // Only those named: a TV in front of it is wrong on that wall too.
        assert_eq!(check(&[win.clone(), obj("the TV", 140, 100, 30, 100)], &[wall(80, 20, 300, 190, true), board.clone()]).len(), 1);
        // Two things hung into each other.
        let other = Obj { rests: Rests::Hung, ..obj("poster", 130, 60, 40, 40) };
        assert_eq!(check(&[win.clone(), other], &[wall(80, 20, 300, 190, false)]).len(), 1);
        // Standing on a wall is standing on nothing.
        assert_eq!(check(&[obj("pot", 100, 150, 20, 40)], &[wall(80, 20, 300, 190, false)]).len(), 1);
        // On its sill: resting.
        let sill = Surface { layer: 0, what: "sill", y: 105, depth: 0, x0: 100, x1: 160, wall: None };
        // The piece on the sill stands in front of the glass: recorded not solid, so the window isn't behind it.
        let pumpkin = Obj { solid: false, ..obj("pumpkin", 140, 90, 15, 15) };
        assert!(check(&[win, pumpkin], &[wall(80, 20, 300, 190, false), sill]).is_empty());
    }

    #[test]
    fn walls_and_sills_follow_the_picture_they_are_in() {
        record(true);
        set_origin(100, 40);
        wall("w", 0, 0, 50, 60, None);
        surface_here("sill", 30, 0, 5, 45);
        surface("board", 70, 4, 0, 500);
        let (_, surfaces) = take();
        assert_eq!((surfaces[0].x0, surfaces[0].y, surfaces[0].wall), (100, 40, Some((100, None))));
        assert_eq!((surfaces[1].x0, surfaces[1].y), (105, 70));
        assert_eq!((surfaces[2].x0, surfaces[2].y), (0, 70), "a screen surface stays where it is");
        record(false);
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
