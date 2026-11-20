//! The crate of Shelf space (`shelf_space`), as a scene: the games not on the shelf stand in cardboard boxes on the floor
//! under it, one box (or more) per system, by name, their spines up out of the box. Where everything is, in screen
//! pixels, worked out here without painting (`app_crate` paints it), so the physics check (`scene_check`,
//! tests/scene_rests.rs) and the tests read the same numbers the picture is made of.
//!
//! A swap (`Swap`): the game from the crate rises out of its box, goes up over the shelf and down into the place of the one
//! going into the crate, which comes out of the row and goes down into its system's box the same way; the cases beside
//! each place slide apart or together while the two are in the air. Nothing floats: a case is on the box's floor, on the
//! shelf's board, or in the hand on its way.

use std::time::Duration;

/// The box picture (`assets/media/props/crate.png`): its rows over the front's top edge are its back and inside (painted
/// behind the cases), the rest its front (painted over them).
pub const ART_W: i32 = 90;
pub const ART_H: i32 = 60;
pub const ART_RIM: i32 = 14;
/// Its columns that stay as they are at each side when a box is made wider (the flaps and the corners).
pub const ART_SIDE: i32 = 22;
/// The packing tape down the middle of the front.
pub const ART_TAPE: (i32, i32) = (40, 50);
/// The front's columns.
pub const ART_FRONT: (i32, i32) = (7, 83);

/// How many cases a box holds at most: a system with more fills more boxes.
pub const PER_BOX: usize = 14;

/// A case as it stands: its width and height on the screen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Case {
    pub w: i32,
    pub h: i32,
}

/// A box on the floor: where its front is (x, w; row pixels) and how high its front wall is, the scale of its picture,
/// and which cases are in it.
#[derive(Clone, Debug, PartialEq)]
pub struct Crate {
    pub x: i32,
    pub w: i32,
    pub wall: i32,
    pub k: f32,
    pub cases: std::ops::Range<usize>,
    /// Which group (system) it holds.
    pub group: usize,
}

/// The boxes and the cases in them, along the row (x from the row's start).
#[derive(Clone, Debug, PartialEq)]
pub struct Laid {
    pub crates: Vec<Crate>,
    /// Each case's left edge.
    pub x: Vec<i32>,
    /// Each case's box.
    pub in_crate: Vec<usize>,
    pub len: i32,
}

impl Laid {
    /// How far case `i`'s foot is over the box's bottom: the box's floor.
    pub fn floor_lift(&self, i: usize) -> i32 {
        self.crates.get(self.in_crate[i]).map_or(0, |c| inner_floor(c.k))
    }
}

/// The box's floor over its bottom edge (its cardboard).
pub fn inner_floor(k: f32) -> i32 {
    (2.0 * k).round() as i32
}

/// The space between a box's side and its first case.
fn pad(f: f32) -> i32 {
    (6.0 * f).round() as i32
}

/// The space between two boxes.
pub fn gap(f: f32) -> i32 {
    (14.0 * f).round() as i32
}

/// Lays out `cases` in boxes: `groups` are the number of cases of each system, in order; a group with more than
/// [`PER_BOX`] fills several boxes. Each box's front wall hides about the lower half of its tallest case; it is as wide
/// as its cases and `room` more (the free space a real box has, so a case can go in and out).
pub fn lay(groups: &[usize], cases: &[Case], f: f32, room: i32) -> Laid {
    let (mut crates, mut x, mut in_crate) = (Vec::new(), Vec::new(), Vec::new());
    let mut at = 0;
    let mut i = 0;
    for (g, &n) in groups.iter().enumerate() {
        let mut left = n;
        while left > 0 {
            let take = left.min(PER_BOX);
            let range = i..i + take;
            let tallest = cases[range.clone()].iter().map(|c| c.h).max().unwrap_or(0);
            let wall = (tallest as f32 * 0.5).round().max(8.0) as i32;
            let k = wall as f32 / (ART_H - ART_RIM) as f32;
            let side = (ART_FRONT.0 as f32 * k).round() as i32;
            let mut cx = at + side + pad(f);
            for c in &cases[range.clone()] {
                x.push(cx);
                in_crate.push(crates.len());
                cx += c.w;
            }
            let front_w = cx - at - side + pad(f) + room;
            let w = front_w + 2 * side;
            crates.push(Crate { x: at, w, wall, k, cases: range, group: g });
            at += w + gap(f);
            i += take;
            left -= take;
        }
    }
    Laid { crates, x, in_crate, len: (at - gap(f)).max(0) }
}

/// The scroll of a row `len` long in a view `view` wide that keeps `x0..x1` in view with `ctx` beside it, starting from
/// `now`: as the Shelf look's row glides.
pub fn scroll_to(now: f32, x0: i32, x1: i32, len: i32, view: i32, ctx: i32) -> f32 {
    let mut want = now;
    if (x0 as f32) - want < ctx as f32 {
        want = (x0 - ctx) as f32;
    }
    if (x1 as f32) - want > (view - ctx) as f32 {
        want = (x1 - view + ctx) as f32;
    }
    want.clamp(0.0, (len - view).max(0) as f32).round()
}

/// How long a swap takes with Motion on.
pub const SWAP: Duration = Duration::from_millis(2000);
/// A case going straight up into the shelf's free place (no swap).
pub const PUT: Duration = Duration::from_millis(1100);

/// Where a swap is at `t` (0..1): the parts of the way, so the painter and the check agree.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Moment {
    /// 0..1: out of the box / out of the row.
    pub up: f32,
    /// 0..1: across, from over where it was to over where it goes.
    pub across: f32,
    /// 0..1: down into its place.
    pub down: f32,
    /// 0..1: the cases beside each place sliding apart or together, and the rows gliding.
    pub slide: f32,
}

fn smooth(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

pub fn moment(t: f32) -> Moment {
    Moment { up: smooth(t / 0.25), across: smooth((t - 0.25) / 0.45), down: smooth((t - 0.7) / 0.3), slide: smooth((t - 0.2) / 0.5) }
}

/// 0..1 between `a` and `b` of `t`, eased.
pub fn phase(t: f32, a: f32, b: f32) -> f32 {
    smooth((t - a) / (b - a))
}

/// Where everything of a swap is at `t` (0..1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Steps {
    /// The game going into the crate: up out of the row, over to its box, down into it.
    pub out: Moment,
    /// The game coming out: from half out of its box (where it waited while the player picked) up out of it, over to
    /// the shelf and onto the free place.
    pub inn: Moment,
    /// The crate's cases making room where the one going in goes (done before it comes down).
    pub crate_open: f32,
    /// The crate's cases closing the gap where the one coming out stood (after it is out).
    pub crate_close: f32,
    /// The shelf's cases sliding for the new one's width: after the old one is out, before the new one comes down.
    pub shelf_slide: f32,
    /// The crate row gliding to the box the one going in goes to, and back to the one coming out.
    pub scroll_go: f32,
    pub scroll_back: f32,
}

/// A swap at `t`: one out, then one in, one case moving at a time, so nothing passes through anything; onto a free place
/// (`swap` false) only the one coming out moves.
pub fn steps(t: f32, swap: bool) -> Steps {
    if !swap {
        let inn = Moment { up: phase(t, 0.0, 0.25), across: phase(t, 0.25, 0.7), down: phase(t, 0.7, 1.0), slide: 0.0 };
        return Steps { out: moment(1.0), inn, crate_open: 1.0, crate_close: phase(t, 0.3, 0.6), shelf_slide: phase(t, 0.2, 0.6), scroll_go: 0.0, scroll_back: 0.0 };
    }
    Steps {
        out: Moment { up: phase(t, 0.0, 0.12), across: phase(t, 0.12, 0.38), down: phase(t, 0.38, 0.5), slide: 0.0 },
        inn: Moment { up: 0.55 + 0.45 * phase(t, 0.6, 0.68), across: phase(t, 0.68, 0.88), down: phase(t, 0.88, 1.0), slide: 0.0 },
        crate_open: phase(t, 0.1, 0.36),
        crate_close: phase(t, 0.7, 0.9),
        shelf_slide: phase(t, 0.45, 0.85),
        scroll_go: phase(t, 0.08, 0.35),
        scroll_back: phase(t, 0.5, 0.62),
    }
}

/// The foot of a case on its way at `t`: from foot `from` (where it stood) up by `clear_from` (out of its box or row),
/// across to over `to` (clear by `clear_to`) in a small arc, and down onto `to`.
pub fn way(m: Moment, from: (f32, f32), clear_from: f32, to: (f32, f32), clear_to: f32, arc: f32) -> (f32, f32) {
    let (a, b) = ((from.0, from.1 - clear_from), (to.0, to.1 - clear_to));
    if m.down > 0.0 {
        return (to.0, b.1 + (to.1 - b.1) * m.down);
    }
    if m.across > 0.0 || m.up >= 1.0 {
        let u = m.across;
        let lift = arc * (std::f32::consts::PI * u).sin();
        return (a.0 + (b.0 - a.0) * u, a.1 + (b.1 - a.1) * u - lift);
    }
    (from.0, from.1 - clear_from * m.up)
}

/// The left edges of a row's cases between two layouts `before` and `after` (the same cases, the ones going
/// away and coming left out): each slides from its place before to its place after.
pub fn slide(before: &[(u64, i32)], after: &[(u64, i32)], s: f32) -> Vec<(u64, i32)> {
    let mut out = Vec::new();
    for &(id, x0) in before {
        if let Some(&(_, x1)) = after.iter().find(|(j, _)| *j == id) {
            out.push((id, (x0 as f32 + (x1 - x0) as f32 * s).round() as i32));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cases(n: usize) -> Vec<Case> {
        (0..n).map(|i| Case { w: 20 + (i as i32 % 3) * 4, h: 100 + (i as i32 % 4) * 10 }).collect()
    }

    #[test]
    fn every_case_stands_inside_its_box_and_no_two_boxes_touch() {
        let cs = cases(40);
        let l = lay(&[3, 30, 7], &cs, 1.0, 30);
        assert_eq!(l.crates.len(), 1 + 3 + 1, "30 cases fill three boxes");
        for (i, c) in cs.iter().enumerate() {
            let b = &l.crates[l.in_crate[i]];
            assert!(l.x[i] > b.x && l.x[i] + c.w < b.x + b.w, "case {i} sticks out of its box");
            assert!(b.wall * 2 >= c.h - 1, "its box's wall hides about half its tallest case");
        }
        for w in l.crates.windows(2) {
            assert!(w[0].x + w[0].w < w[1].x, "boxes stand apart");
        }
        for i in 1..cs.len() {
            assert!(l.x[i - 1] + cs[i - 1].w <= l.x[i], "cases {} and {i} side by side, not in each other", i - 1);
        }
    }

    #[test]
    fn an_empty_crate_has_no_boxes() {
        let l = lay(&[], &[], 1.0, 30);
        assert!(l.crates.is_empty() && l.len == 0);
    }

    #[test]
    fn a_case_on_its_way_starts_where_it_stood_and_ends_where_it_goes() {
        let (from, to) = ((10.0, 300.0), (200.0, 150.0));
        assert_eq!(way(moment(0.0), from, 60.0, to, 20.0, 30.0), from);
        assert_eq!(way(moment(1.0), from, 60.0, to, 20.0, 30.0), to);
        // Out of its box first, straight up; down into its place last, straight down.
        let up = way(moment(0.2), from, 60.0, to, 20.0, 30.0);
        assert!(up.0 == from.0 && up.1 < from.1);
        let down = way(moment(0.9), from, 60.0, to, 20.0, 30.0);
        assert!(down.0 == to.0 && down.1 < to.1);
    }

    #[test]
    fn the_cases_beside_a_place_slide_without_passing_each_other() {
        let before = [(1, 0), (2, 30), (3, 60)];
        let after = [(1, 0), (3, 30)];
        for s in [0.0, 0.3, 1.0] {
            let at = slide(&before, &after, s);
            assert_eq!(at.len(), 2);
            assert!(at[0].1 + 30 <= at[1].1 || s > 0.0);
        }
        assert_eq!(slide(&before, &after, 1.0), vec![(1, 0), (3, 30)]);
    }
}
