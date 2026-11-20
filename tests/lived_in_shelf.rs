//! Settings → Look → Lived-in shelf (0.5.2, `pocket_ui::lived_in`, `app_cabinet::shelf_row`): the Shelf look's rows of
//! games with some cases leaning and a few on a small stack. Off (the default) the shelf is painted as before, byte for
//! byte; on, the same games always stand the same way; a row that fits the shelf standing straight never has to scroll
//! for its stacks (a mixed Favourites row at 1280x720 did in the prototype); and the leaning and lying spines are made
//! ahead after the row opens (`App::lived_warm_tick`), so a walk along the row turns none of them in a frame (the
//! prototype's first frame on a tile of them: 5-6 ms on a desktop core, estimated 40-55 ms on the Brick).
//!
//! On a made-up library written here: 40 Super Nintendo games and a Favourites row of 27 games of seven systems (SNES and
//! N64 boxes, PS1 jewel cases, GBA and DS boxes, Mega Drive and GameCube cases). Needs python3 on Linux without a
//! compositor (scripts/wayland-stub.py), as tests/shelf_end.rs.

use std::path::{Path, PathBuf};
use std::process::Command;

include!("common/root.rs");

/// The page's side margin (`palette::MARGIN`).
const MARGIN: i32 = 72;

include!("common/shelf_library.rs");

/// The shelf's left and right end and a line just above the board (as tests/shelf_end.rs measures it).
fn shelf_line(w: i32, h: i32) -> (i32, i32, i32) {
    let top = if h < 600 { 80 } else { 104 };
    let bottom = pocket_ui::kit::content_bottom(h);
    let f = (((h - 112 - top).max(200) as f32 * 0.42 / 120.0 * 2.0).floor() / 2.0).clamp(1.0, 4.0);
    let floor = bottom - (8.0 * f).round() as i32;
    let col = if w >= 1600 { (w * 3 / 4).max(1440).min(w) } else { w };
    let x0 = (w - col) / 2;
    (x0 + MARGIN, x0 + col - MARGIN, floor - 3)
}

/// The first and last pixel along line `y` that differ from the page.
fn ends((w, _, px): &(i32, i32, Vec<u8>), y: i32) -> (i32, i32) {
    let at = |x: i32| {
        let i = ((y * w + x) * 3) as usize;
        (px[i], px[i + 1], px[i + 2])
    };
    let bg = at(2);
    let painted: Vec<i32> = (0..*w).filter(|&x| at(x) != bg).collect();
    (painted.first().copied().unwrap_or(i32::MAX), painted.last().copied().unwrap_or(0))
}

/// Off is the shelf as before: settings.json without the key and with `"lived_in": false` paint the same bytes (Home
/// and a system's row; Home stays tidy when it is on, too). On, a system's row is posed, and the same games stand the
/// same way at every start.
#[test]
fn off_is_the_shelf_as_before_and_on_stands_the_same_way_every_time() {
    let _guard = tempfile::tempdir().unwrap();
    let tmp = _guard.path().to_path_buf();
    let stub = wayland(&tmp);
    for size in ["640x480", "1280x720", "1024x768"] {
        for state in ["shelf:snes:shelf:games:12", "shelf:snes:shelf:games:39", "shelf:favorites:shelf:games:3"] {
            let (_, before) = run(&tmp, &stub, None, size, state);
            let (_, off) = run(&tmp, &stub, Some(false), size, state);
            assert!(before.2 == off.2, "{state} at {size}: Off must paint the shelf as before");
            let (_, on) = run(&tmp, &stub, Some(true), size, state);
            let (_, again) = run(&tmp, &stub, Some(true), size, state);
            assert!(on.2 == again.2, "{state} at {size}: the same games must stand the same way at every start");
            assert!(on.2 != off.2, "{state} at {size}: On must pose the row");
        }
        // Home stays tidy: the systems' row is not a row of games.
        let (_, off) = run(&tmp, &stub, Some(false), size, "shelf:snes:shelf");
        let (_, on) = run(&tmp, &stub, Some(true), size, "shelf:snes:shelf");
        assert!(on.2 == off.2, "Home at {size}: Lived-in shelf leaves Home as it is");
    }
    drop(stub);
}

/// A mixed Favourites row that fits the shelf standing straight (with the props after its end) at 1280x720 stays whole on
/// the shelf with Lived-in shelf on: its stacks went, not its place (the prototype's mixed row had to scroll there, its
/// first case cut at the left end). With a case in the middle and the last one selected, the shelf's left end shows the
/// same pixels (the row did not move), and the row ends before the shelf's right end.
#[test]
fn a_row_that_fits_standing_never_scrolls_for_its_stacks() {
    let _guard = tempfile::tempdir().unwrap();
    let tmp = _guard.path().to_path_buf();
    let stub = wayland(&tmp);
    let mut wrong = Vec::new();
    for lived in [Some(false), Some(true)] {
        let size = "1280x720";
        let (_, mid) = run(&tmp, &stub, lived, size, "shelf:favorites:shelf:games:13");
        let (_, last) = run(&tmp, &stub, lived, size, "shelf:favorites:shelf:games:26");
        let (left, right, y) = shelf_line(mid.0, mid.1);
        // The shelf's first 160 pixels, from 80 lines above the line down to it.
        let band = |(w, _, px): &(i32, i32, Vec<u8>)| -> Vec<u8> {
            (y - 80..=y).flat_map(|yy| px[((yy * w + left) * 3) as usize..((yy * w + left + 160) * 3) as usize].to_vec()).collect()
        };
        let end = ends(&last, y).1;
        let what = format!("{size}, lived_in {lived:?}: the row ends at {end} with the last case selected, the shelf at {right}");
        println!("{what}");
        if band(&mid) != band(&last) {
            wrong.push(format!("{what}: the row scrolled"));
        }
        if end > right - 16 {
            wrong.push(format!("{what}: no room left after the row"));
        }
    }
    drop(stub);
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// The value of `key=` in the line of `out` starting with `start`.
fn value(out: &str, start: &str, key: &str) -> u64 {
    let line = out.lines().find(|l| l.starts_with(start)).unwrap_or_else(|| panic!("no {start} line in:\n{out}"));
    let v = line.split_whitespace().find_map(|w| w.strip_prefix(&format!("{key}="))).unwrap_or_else(|| panic!("no {key} in {line}"));
    v.parse().unwrap()
}

/// Opening the row turns the leaning and lying spines in view; the rest are made ahead in the idle moments after
/// (`lived_warm_tick`, here all at once as `walk-warm` does), so a calm walk to the row's end turns none in a frame.
/// Without the warming the walk turns them as it reaches them (the stutter on the Brick). Off, nothing is turned.
#[test]
fn the_rows_turned_spines_are_made_ahead_not_while_walking() {
    let _guard = tempfile::tempdir().unwrap();
    let tmp = _guard.path().to_path_buf();
    let stub = wayland(&tmp);
    let (cold, _) = run(&tmp, &stub, Some(true), "640x480", "bench-playful:snes:walk");
    assert!(value(&cold, "lived walk", "posed_in_walk") > 0, "without the warming the walk turns spines:\n{cold}");
    let (warm, _) = run(&tmp, &stub, Some(true), "640x480", "bench-playful:snes:walk-warm");
    assert!(value(&warm, "lived warm", "posed") > 0, "{warm}");
    assert_eq!(value(&warm, "lived walk", "posed_in_walk"), 0, "made ahead, the walk turns none:\n{warm}");
    let (off, _) = run(&tmp, &stub, Some(false), "640x480", "bench-playful:snes:walk");
    assert_eq!(value(&off, "lived walk", "posed_in_walk"), 0, "{off}");
    drop(stub);
}
