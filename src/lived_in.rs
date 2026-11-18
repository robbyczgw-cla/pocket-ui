//! Settings → Look → **Lived-in shelf** (off unless switched on, `App::lived_in`): the Shelf look's rows of games as
//! someone's real shelf, calm, not messy. Now and then a case leans a few degrees on its neighbour (most often the last
//! one standing), the last two or three of a long row lie on a small stack with their spines to you, the spaces between
//! the cases differ a little and a few cases stand a little further back. Everything comes from the game (its ROM's
//! name) and its place in the row, so the shelf looks the same at every start and nothing moves while it scrolls.
//!
//! This is the plan and the pictures; the row's layout and painting is `app_cabinet` (`shelf_row`, `shelf_tile`). The
//! posed pictures are made once per row, ahead in idle moments after the row opens (`App::lived_warm_tick`), so a
//! frame costs what it cost before.

use crate::cabinet_art::Pic;

/// How a case stands on the row.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Pose {
    Up,
    /// Leaning by this many degrees: negative, its top rests on the neighbour on its left (it stands on its bottom-left
    /// corner, out a little from it); positive, on the one on its right. Two leaning left one after the other lean like
    /// dominoes, the second on the first (`app_cabinet::shelf_row` lays them out so).
    Lean(f32),
    /// Lying on a stack, this many cases above the board (0 at the bottom: a new stack starts there), pushed sideways by
    /// a few template pixels so the stack is not a ruler-straight pile.
    Flat { level: u8, nudge: i8 },
}

/// One case's plan: its pose, extra room before it (template pixels, a multiple of 4 so a spine still starts on the
/// 4-pixel grid; now and then a small gap), and whether it stands a little further back in the shelf (a touch darker)
/// or a little forward (its foot over the board's front edge).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Plan {
    pub pose: Pose,
    pub extra: i32,
    pub back: bool,
    pub forward: bool,
}

const UP: Plan = Plan { pose: Pose::Up, extra: 0, back: false, forward: false };

/// The steepest lean: the title still reads.
pub const MAX_LEAN: f32 = 9.0;

/// A stable hash of a game's name (FNV-1a): the same on every start and every machine.
pub fn seed(name: &str) -> u64 {
    name.bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| (h ^ b as u64).wrapping_mul(0x0000_0100_0000_01b3))
}

/// A few more bits from a seed (the seeds of neighbouring names share their high bits less than one would like).
fn bits(s: u64, salt: u64) -> u64 {
    let mut x = s ^ salt.wrapping_mul(0x9e37_79b9_7f4a_7c15);
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}

/// The plan for a row of games, one seed each (`seed` of the ROM's name) and whether it may move at all (a frozen game
/// keeps standing up: its game sticks out of the top). Short rows stay as they are.
pub fn plan(seeds: &[u64], fixed: &[bool]) -> Vec<Plan> {
    plan_sized(seeds, fixed, &[], true)
}

/// [`plan`] for a row of mixed cases, `sizes` their lengths (any unit; empty: all alike): a stack only where each case
/// lies on one at least as long as itself, the small ones on top (a GBA box on an SNES box, never the other way).
/// `stacks` false: no case lies (a row that fits the shelf standing must not grow past it, `app_cabinet::shelf_row`);
/// the rest of the plan is the same as with them where no stack was.
pub fn plan_sized(seeds: &[u64], fixed: &[bool], sizes: &[f32], stacks_ok: bool) -> Vec<Plan> {
    let n = seeds.len();
    let settles = |start: usize, size: usize| (start + 1..start + size).all(|i| sizes.get(i).zip(sizes.get(i - 1)).is_none_or(|(up, down)| up <= down));
    let mut out = vec![UP; n];
    if n < 6 {
        return out;
    }
    let free = |i: usize| !fixed.get(i).copied().unwrap_or(false);
    let mut flat = vec![false; n];
    // A stack at the row's end (2 to 4 cases) from 10 games on, and in most rows of 14 or more a second one in the
    // middle (2 or 3), where it stands in a gap of its own.
    let mut stacks: Vec<(usize, usize)> = Vec::new();
    if n >= 10 && stacks_ok {
        let want = 2 + bits(seeds[n - 1], 1) as usize % 3;
        let mut k = 0;
        while k < want && free(n - 1 - k) {
            k += 1;
        }
        while k >= 2 && !settles(n - k, k) {
            k -= 1;
        }
        if k >= 2 {
            stacks.push((n - k, k));
        }
    }
    let end_start = stacks.first().map_or(n, |s| s.0);
    if n >= 14 && stacks_ok && bits(seeds[0], 2) % 10 < 7 {
        let size = 2 + bits(seeds[0], 3) as usize % 2;
        let (lo, hi) = (n / 4, (n * 3 / 5).min(end_start.saturating_sub(size + 3)));
        if hi > lo {
            // From its place on along the row to the first spot where it settles (always the first in a row of one
            // system's games).
            let first = bits(seeds[0], 4) as usize % (hi - lo);
            if let Some(start) = (0..hi - lo).map(|k| lo + (first + k) % (hi - lo)).find(|&st| (st..st + size).all(free) && settles(st, size)) {
                stacks.push((start, size));
            }
        }
    }
    for &(start, size) in &stacks {
        for k in 0..size {
            let i = start + k;
            flat[i] = true;
            out[i].pose = Pose::Flat { level: k as u8, nudge: (bits(seeds[i], 5) % 13) as i8 - 6 };
        }
    }
    // Leaning: about every fourth to sixth case, 5 to 9 degrees; now and then the next one leans on it like a domino.
    // Never against a stack (it is lower than they are), never into one leaning the other way.
    let mut since = 2usize;
    for i in 1..n {
        if flat[i] || !free(i) {
            since = 0;
            continue;
        }
        since += 1;
        let s = seeds[i];
        let prev = out[i - 1].pose;
        if let Pose::Lean(d) = prev {
            let lone = i < 2 || !matches!(out[i - 2].pose, Pose::Lean(_));
            if d < 0.0 && lone && bits(s, 6) % 3 == 0 {
                out[i].pose = Pose::Lean((d - 1.0 + (bits(s, 7) % 3) as f32).clamp(-MAX_LEAN, -4.0));
                since = 0;
            }
            continue;
        }
        if flat[i - 1] || since < 4 || (since < 6 && bits(s, 8) % 3 != 0) {
            continue;
        }
        let deg = 5.0 + (bits(s, 9) % 5) as f32;
        let right = bits(s, 10) % 4 == 0 && i + 1 < end_start && !flat[i + 1] && free(i + 1);
        out[i].pose = Pose::Lean(if right { deg } else { -deg });
        since = 0;
    }
    // The last one standing before the stack at the end leans back on its neighbour, as a row's end does.
    if let Some(&(start, _)) = stacks.first().filter(|s| s.0 + s.1 == n) {
        let last = start - 1;
        let prev_right = matches!(out[last - 1].pose, Pose::Lean(d) if d > 0.0);
        let two_before = last >= 2 && matches!((out[last - 2].pose, out[last - 1].pose), (Pose::Lean(_), Pose::Lean(_)));
        if free(last) && out[last].pose == Pose::Up && !flat[last - 1] && !prev_right && !two_before {
            out[last].pose = Pose::Lean(-(5.0 + (bits(seeds[last], 11) % 4) as f32));
        }
    }
    for i in 1..n {
        if flat[i] {
            continue;
        }
        let domino = matches!((out[i - 1].pose, out[i].pose), (Pose::Lean(a), Pose::Lean(b)) if a < 0.0 && b < 0.0);
        let s = bits(seeds[i], 12);
        // Uneven spaces: a quarter get 4 template pixels more, some 8, one in twelve a small gap.
        // A case leaning left rests on its neighbour: no extra room before it, or it would lean on nothing.
        let leans_left = matches!(out[i].pose, Pose::Lean(d) if d < 0.0);
        if !domino && !leans_left {
            out[i].extra = match s % 12 {
                0..=2 => 4,
                3..=4 => 8,
                5 => 20,
                _ => 0,
            };
        }
        // A few stand further back, a few a little forward (not a leaning one: it rests on its neighbours).
        if out[i].pose == Pose::Up {
            out[i].back = (s >> 8) % 8 == 3;
            out[i].forward = (s >> 8) % 8 == 5 && !out[i - 1].forward;
        }
    }
    out
}

/// The room a case of `w` x `h` takes on the row in `pose` (its leaning outline's width), in the same pixels.
pub fn footprint(pose: Pose, w: f32, h: f32) -> f32 {
    match pose {
        Pose::Up => w,
        Pose::Lean(deg) => {
            let a = deg.abs().to_radians();
            w * a.cos() + h * a.sin()
        }
        Pose::Flat { .. } => h,
    }
}

/// How high a leaning case's outline stands over the board.
pub fn lean_height(deg: f32, w: f32, h: f32) -> f32 {
    let a = deg.abs().to_radians();
    h * a.cos() + w * a.sin()
}

/// `p` turned by `deg` degrees (positive: its top to the right), smoothly (bilinear, its edges soft), on a picture just
/// large enough for the turned outline, with the outline's middle in the picture's middle. Made once per spine.
pub fn turned(p: &Pic, deg: f32) -> Pic {
    let a = deg.to_radians();
    let (sin, cos) = (a.sin(), a.cos());
    let (w, h) = (p.w as f32, p.h as f32);
    let ow = (w * cos.abs() + h * sin.abs()).ceil() as i32 + 2;
    let oh = (h * cos.abs() + w * sin.abs()).ceil() as i32 + 2;
    let mut out = Pic::new(ow, oh, 0);
    let (ocx, ocy, icx, icy) = (ow as f32 / 2.0, oh as f32 / 2.0, w / 2.0, h / 2.0);
    // The picture once with its colours times their alpha, and a see-through rim one pixel wide all round, so every
    // sample below reads four neighbours without a test.
    let (pw, ph) = (p.w as usize + 2, p.h as usize + 2);
    let mut pre = vec![[0.0f32; 4]; pw * ph];
    for y in 0..p.h as usize {
        for x in 0..p.w as usize {
            let v = p.px[y * p.w as usize + x];
            let al = (v >> 24) as f32 / 255.0;
            pre[(y + 1) * pw + x + 1] = [al, ((v >> 16) & 255) as f32 * al, ((v >> 8) & 255) as f32 * al, (v & 255) as f32 * al];
        }
    }
    for oy in 0..oh {
        let dy = oy as f32 + 0.5 - ocy;
        for ox in 0..ow {
            // The screen point back into the picture (turning the other way round the middles), one pixel in for the rim.
            let dx = ox as f32 + 0.5 - ocx;
            let (sx, sy) = (dx * cos + dy * sin + icx + 0.5, -dx * sin + dy * cos + icy + 0.5);
            if sx < 0.0 || sy < 0.0 || sx >= (pw - 1) as f32 || sy >= (ph - 1) as f32 {
                continue;
            }
            let (x0, y0) = (sx as usize, sy as usize);
            let (fx, fy) = (sx - x0 as f32, sy - y0 as f32);
            let i = y0 * pw + x0;
            let (a, b, c, d) = (pre[i], pre[i + 1], pre[i + pw], pre[i + pw + 1]);
            let (wa, wb, wc, wd) = ((1.0 - fx) * (1.0 - fy), fx * (1.0 - fy), (1.0 - fx) * fy, fx * fy);
            let acc: [f32; 4] = std::array::from_fn(|k| a[k] * wa + b[k] * wb + c[k] * wc + d[k] * wd);
            if acc[0] > 0.004 {
                let al = acc[0].min(1.0);
                let inv = 1.0 / acc[0];
                let ch = |v: f32| ((v * inv + 0.5) as u32).min(255);
                out.px[(oy * ow + ox) as usize] = (((al * 255.0 + 0.5) as u32) << 24) | (ch(acc[1]) << 16) | (ch(acc[2]) << 8) | ch(acc[3]);
            }
        }
    }
    out
}

/// `p` lying on its side, its top to the left: a spine's title that runs down the standing case reads left to right.
pub fn lying(p: &Pic, title_runs_down: bool) -> Pic {
    let (w, h) = (p.w, p.h);
    let mut out = Pic::new(h, w, 0);
    for y in 0..h {
        for x in 0..w {
            let v = p.px[(y * w + x) as usize];
            // Turned a quarter: counter-clockwise when the title runs down (its top goes to the left), clockwise otherwise.
            let (ox, oy) = if title_runs_down { (y, w - 1 - x) } else { (h - 1 - y, x) };
            out.px[(oy * h + ox) as usize] = v;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(n: usize) -> Vec<u64> {
        (0..n).map(|i| seed(&format!("Game number {i} (USA)"))).collect()
    }

    #[test]
    fn the_same_games_always_stand_the_same_way() {
        let s = names(30);
        assert_eq!(plan(&s, &[]), plan(&s, &[]));
    }

    #[test]
    fn short_rows_stand_straight() {
        assert!(plan(&names(5), &[]).iter().all(|p| *p == UP));
    }

    #[test]
    fn a_long_row_ends_on_a_small_stack_and_has_one_in_the_middle_too() {
        let p = plan(&names(30), &[]);
        let starts: Vec<usize> = (0..30).filter(|&i| matches!(p[i].pose, Pose::Flat { level: 0, .. })).collect();
        assert!((1..=2).contains(&starts.len()), "{starts:?}");
        let flat = p.iter().filter(|p| matches!(p.pose, Pose::Flat { .. })).count();
        assert!(flat <= 7, "{flat} lying");
        assert!(matches!(p[29].pose, Pose::Flat { .. }));
        // Over many rows, most long ones get a stack in the middle as well.
        let two = (0..40).filter(|r| {
            let s: Vec<u64> = (0..30).map(|i| seed(&format!("Row {r} game {i}"))).collect();
            plan(&s, &[]).iter().filter(|p| matches!(p.pose, Pose::Flat { level: 0, .. })).count() == 2
        });
        assert!(two.count() >= 15);
    }

    #[test]
    fn leaning_is_common_but_readable_and_never_three_in_a_row() {
        for n in [12, 30, 120] {
            let p = plan(&names(n), &[]);
            let lean: Vec<f32> = p.iter().filter_map(|p| if let Pose::Lean(d) = p.pose { Some(d) } else { None }).collect();
            assert!(lean.len() >= n / 8 && lean.len() <= n / 3, "{} of {n} lean", lean.len());
            assert!(lean.iter().all(|d| d.abs() <= MAX_LEAN), "{lean:?}");
            assert!(p.windows(3).all(|w| !w.iter().all(|p| matches!(p.pose, Pose::Lean(_)))));
            // A right-leaning case never has a neighbour leaning into it.
            assert!(p.windows(2).all(|w| !matches!((w[0].pose, w[1].pose), (Pose::Lean(a), Pose::Lean(b)) if a > 0.0 && b < 0.0)));
            // A case leaning left has no extra room before it (it rests on its neighbour).
            assert!(p.iter().all(|p| !matches!(p.pose, Pose::Lean(d) if d < 0.0) || p.extra == 0));
            // Nothing leans on a stack.
            assert!(p.windows(2).all(|w| !matches!((w[0].pose, w[1].pose), (Pose::Flat { .. }, Pose::Lean(_)))));
        }
    }

    #[test]
    fn mixed_cases_stack_small_on_big() {
        // Many mixed rows (SNES/N64 boxes, PS1 jewel cases, GBA and DS boxes, Mega Drive and GameCube cases): in every
        // stack each case lies on one at least as long, and stacks still happen.
        let lens = [218.0f32, 218.0, 184.0, 142.0, 166.0, 260.0, 230.0];
        let mut stacks = 0;
        for r in 0..60 {
            let s: Vec<u64> = (0..24).map(|i| seed(&format!("Mixed row {r} game {i}"))).collect();
            let sizes: Vec<f32> = s.iter().map(|&x| lens[bits(x, 99) as usize % lens.len()]).collect();
            let p = plan_sized(&s, &[], &sizes, true);
            for i in 1..p.len() {
                if let (Pose::Flat { level: a, .. }, Pose::Flat { level: b, .. }) = (p[i - 1].pose, p[i].pose) {
                    if b == a + 1 {
                        assert!(sizes[i] <= sizes[i - 1], "row {r}: case {i} ({}) on a shorter one ({})", sizes[i], sizes[i - 1]);
                    }
                }
            }
            stacks += p.iter().filter(|p| matches!(p.pose, Pose::Flat { level: 0, .. })).count();
        }
        assert!(stacks >= 30, "{stacks} stacks in 60 mixed rows");
        // A row of one system's games plans as before.
        let s = names(30);
        assert_eq!(plan_sized(&s, &[], &[180.0; 30], true), plan(&s, &[]));
    }

    #[test]
    fn without_stacks_nothing_lies_and_the_cases_lean_as_before() {
        let s = names(30);
        let (with, without) = (plan(&s, &[]), plan_sized(&s, &[], &[], false));
        assert!(without.iter().all(|p| !matches!(p.pose, Pose::Flat { .. })));
        assert!(without.iter().any(|p| matches!(p.pose, Pose::Lean(_))), "leaning stays");
        assert_eq!(plan_sized(&s, &[], &[], false), without, "and stays the same at every start");
        // Before the first stack the plans are the same.
        let first = with.iter().position(|p| matches!(p.pose, Pose::Flat { .. })).unwrap();
        assert_eq!(with[..first.saturating_sub(1)], without[..first.saturating_sub(1)]);
    }

    #[test]
    fn a_frozen_game_keeps_standing() {
        let s = names(30);
        let fixed: Vec<bool> = vec![true; 30];
        assert!(plan(&s, &fixed).iter().all(|p| !matches!(p.pose, Pose::Lean(_) | Pose::Flat { .. })));
    }

    /// What turning one spine costs (a PS1 spine on a 1024x768 shelf is about 28 x 180): `--ignored --nocapture`.
    #[test]
    #[ignore]
    fn turning_a_spine_costs() {
        let p = Pic::new(28, 180, 0xff80_4020);
        let t0 = std::time::Instant::now();
        for _ in 0..200 {
            std::hint::black_box(turned(&p, -5.0));
        }
        println!("turned 28x180: {:.1} us each", t0.elapsed().as_secs_f64() * 1e6 / 200.0);
    }

    #[test]
    fn a_turned_spine_keeps_its_colour_and_fits_its_outline() {
        let p = Pic::new(20, 100, 0xff80_4020);
        let t = turned(&p, -5.0);
        assert!((t.w as f32 - footprint(Pose::Lean(-5.0), 20.0, 100.0)).abs() <= 3.0);
        assert_eq!(t.px[(t.h / 2 * t.w + t.w / 2) as usize], 0xff80_4020);
        let l = lying(&p, true);
        assert_eq!((l.w, l.h), (100, 20));
    }
}
