//! Nothing floats (`pocket_ui::scene_check`, 18.11.2026: "wir sind skeuomorph, da kann nichts schweben im
//! Regal"): every object a look paints on a shelf rests on the board, the floor or another object, or is held by a hand
//! or a slot, and no two are inside each other. The painters record their objects as rectangles with baselines while
//! the check runs (`--snapshot bench-playful:<row>:rests`): every selection of a row, each at eight moments of the
//! selection's animation, checked in the model, not in pixels. The deliberate exceptions are named where they are
//! recorded (the selected standing case lifted out of the row).
//!
//! Escaped bug (0.5.1, a hand test on the Brick): with Lived-in shelf on, a lying case selected rose like a standing one and
//! hung over its stack ("sm64 auf chrono trigger wtf?"); the stack under it was painted a case lower. No test looked
//! at where things rest: the snapshots compared pictures with themselves.
//!
//! On the made-up library of tests/common/shelf_library.rs. Needs python3 on Linux without a compositor
//! (scripts/wayland-stub.py).

use std::path::{Path, PathBuf};
use std::process::Command;

include!("common/root.rs");
include!("common/shelf_library.rs");

/// The faults of one row: `rests frames=N faults=M` and its fault lines; and what was seen (`rests seen:`).
fn rests(tmp: &Path, stub: &Option<Stub>, lived: bool, theme: &str, size: &str, row: &str) -> (u64, Vec<String>) {
    // Its recent games stand open, their games a little out of the case (`frozen_snapshot`): open cases on every shelf.
    std::env::set_var("POCKETUI_SNAPSHOT_FROZEN", "all");
    let how = row.split_once('+').map(|(_, h)| if theme == "cabinet" { format!("{h}-cabinet") } else { h.to_string() });
    let what = match (theme, how.as_deref()) {
        (_, Some(how)) => how,
        ("cabinet", _) => "rests-cabinet",
        _ => "rests",
    };
    let row = row.split('+').next().unwrap();
    let (out, _) = run_in(tmp, stub, Some(lived), theme, size, &format!("bench-playful:{row}:{what}"));
    let seen = out.lines().find_map(|l| l.strip_prefix("rests seen: ")).unwrap_or_default();
    println!("{theme} {row} at {size}, lived_in {lived}: {seen}");
    let need: &[&str] = if what.starts_with("rests-launch") || what.starts_with("rests-eject") {
        &[]
    } else if theme == "cabinet" {
        &["shelf # case #", "shelf # case #, out", "the TV", "surface: a shelf of the cabinet", "surface: the cabinet's top board"]
    } else {
        &["case #", "surface: the shelf's board"]
    };
    for n in need {
        assert!(seen.split(" | ").any(|k| k == *n), "{theme} {row} at {size}: \"{n}\" was never recorded; seen: {seen}");
    }
    // In the Shelf look a console comes up for every game; the Cabinet's way in and back goes by its TV.
    if what == "rests-launch" || what == "rests-eject" {
        assert!(seen.contains("the console ("), "{row} at {size}: no console was recorded going in; seen: {seen}");
    }
    let line = out.lines().find(|l| l.starts_with("rests frames=")).unwrap_or_else(|| panic!("no rests line for {row} at {size}:\n{out}"));
    let frames = line.split_whitespace().find_map(|w| w.strip_prefix("frames=")).unwrap().parse().unwrap();
    let faults = out.lines().filter_map(|l| l.strip_prefix("rests fault: ")).map(|f| format!("{theme} {row} at {size}, lived_in {lived}: {f}")).collect();
    (frames, faults)
}

/// The Shelf look: a system's row and the mixed Favourites row, Lived-in shelf on and off, on the handhelds' sizes:
/// standing, leaning, lying and stacked cases, the cases standing a little forward, the bookend and decorations after
/// a short row, at every selection and through its animation.
#[test]
fn nothing_on_the_shelf_looks_row_floats_or_stands_inside_another() {
    let _guard = tempfile::tempdir().unwrap();
    let tmp = _guard.path().to_path_buf();
    let stub = wayland(&tmp);
    let mut wrong = Vec::new();
    for size in ["640x480", "1024x768", "1280x720", "720x720"] {
        for row in ["snes", "favorites", "n64"] {
            for lived in [true, false] {
                let (frames, faults) = rests(&tmp, &stub, lived, "shelf", size, row);
                assert!(frames > 0, "{row} at {size}: no frame checked");
                wrong.extend(faults);
            }
        }
    }
    drop(stub);
    assert!(wrong.is_empty(), "{} faults:\n{}", wrong.len(), wrong.iter().take(60).cloned().collect::<Vec<_>>().join("\n"));
}

/// The Cabinet look: every case of every shelf selected, each at eight moments of its way out into the showcase: the
/// cases on the shelves, the props beside them, the TV (or the handheld in its place) and the console beside it on the
/// top board, and the case out in the showcase once it is there.
#[test]
fn nothing_in_the_cabinet_floats_or_stands_inside_another() {
    let _guard = tempfile::tempdir().unwrap();
    let tmp = _guard.path().to_path_buf();
    let stub = wayland(&tmp);
    let mut wrong = Vec::new();
    for size in ["640x480", "1024x768", "1280x720"] {
        let (frames, faults) = rests(&tmp, &stub, false, "cabinet", size, "snes");
        assert!(frames > 0, "cabinet at {size}: no frame checked");
        wrong.extend(faults);
    }
    drop(stub);
    assert!(wrong.is_empty(), "{} faults:\n{}", wrong.len(), wrong.iter().take(60).cloned().collect::<Vec<_>>().join("\n"));
}

/// A game going into its console, from the Shelf look: every game of the mixed row started, at ten moments of the way in
/// each: the console on the stage's floor once it is up (coming up from below the screen it is held), the case where it
/// stood.
///
/// 18.11.: the consoles stood `px(10)` over the stage's floor ("its contact shadow under it"), which looked as if they
/// floated over the shelf's board; they stand on it now.
#[test]
fn nothing_floats_while_a_game_goes_into_its_console() {
    let _guard = tempfile::tempdir().unwrap();
    let tmp = _guard.path().to_path_buf();
    let stub = wayland(&tmp);
    let mut wrong = Vec::new();
    // The Cabinet at 1240x1080 too: there the console stands beside the TV, and must not stand there and in the scene.
    for (theme, size) in [("shelf", "640x480"), ("shelf", "1280x720"), ("cabinet", "1024x768"), ("cabinet", "1240x1080")] {
        let (frames, faults) = rests(&tmp, &stub, false, theme, size, "favorites+rests-launch");
        assert!(frames > 0);
        wrong.extend(faults);
    }
    drop(stub);
    assert!(wrong.is_empty(), "{} faults:\n{}", wrong.len(), wrong.iter().take(60).cloned().collect::<Vec<_>>().join("\n"));
}

/// Pictures for a look by hand: `SCENE_SHOTS=<dir> cargo test --test scene_rests -- --ignored launch_pictures`.
#[test]
#[ignore = "pictures by hand"]
fn launch_pictures() {
    let _guard = tempfile::tempdir().unwrap();
    let tmp = _guard.path().to_path_buf();
    let stub = wayland(&tmp);
    let dir = std::env::var("SCENE_SHOTS").expect("SCENE_SHOTS");
    std::env::set_var("LIVED_SHOTS", &dir);
    let look = std::env::var("SCENE_LOOK").unwrap_or_else(|_| "shelf".into());
    for (k, ms) in [(0, 1500), (1, 1500), (2, 1500)] {
        std::env::set_var("POCKETUI_SNAPSHOT_LAUNCH_MS", ms.to_string());
        let _ = run_in(&tmp, &stub, Some(false), &look, "640x480", &format!("shelf:snes:{look}:games:{k}"));
    }
    drop(stub);
}

/// The way back from a game: every game of the mixed row, its game out of the console and back into its case on the
/// shelf, every 50 ms of it (the same painter as the way in, played back, `paint_eject`).
#[test]
fn nothing_floats_on_the_way_back_from_a_game() {
    let _guard = tempfile::tempdir().unwrap();
    let tmp = _guard.path().to_path_buf();
    let stub = wayland(&tmp);
    let mut wrong = Vec::new();
    for (theme, size) in [("shelf", "640x480"), ("shelf", "1280x720"), ("cabinet", "1024x768"), ("cabinet", "1240x1080")] {
        let (frames, faults) = rests(&tmp, &stub, false, theme, size, "favorites+rests-eject");
        assert!(frames > 0);
        wrong.extend(faults);
    }
    drop(stub);
    assert!(wrong.is_empty(), "{} faults:\n{}", wrong.len(), wrong.iter().take(60).cloned().collect::<Vec<_>>().join("\n"));
}

/// The window (Cozy extras → Window) is taken off for 0.6 (`Extra::built`): with its switch on as it starts, every sky and
/// every season, the shelf beside the N64 row and the Cabinet still have nothing floating, and no window, no piece on a
/// sill and no sill is ever recorded.
#[test]
fn the_window_is_taken_off_and_nothing_floats() {
    let _guard = tempfile::tempdir().unwrap();
    let tmp = _guard.path().to_path_buf();
    let stub = wayland(&tmp);
    let mut wrong = Vec::new();
    for (time, month) in [("07:00", "4"), ("12:00", "7"), ("19:00", "10"), ("23:30", "1")] {
        std::env::set_var("POCKETUI_ROOM_TIME", time);
        std::env::set_var("POCKETUI_ROOM_MONTH", month);
        for (theme, size) in [("shelf", "640x480"), ("shelf", "1024x768"), ("shelf", "1280x720"), ("shelf", "720x720"), ("cabinet", "640x480"), ("cabinet", "1280x720")] {
            let row = if theme == "cabinet" { "snes" } else { "n64" };
            let (frames, faults) = rests(&tmp, &stub, true, theme, size, row);
            assert!(frames > 0);
            wrong.extend(faults);
        }
    }
    // Not there at all: in the Cabinet at 1280 x 720 and beside the N64 row at 1280 x 720 (the outcome of the run above).
    for theme in ["shelf", "cabinet"] {
        let row = if theme == "cabinet" { "snes" } else { "n64" };
        let what = if theme == "cabinet" { "rests-cabinet" } else { "rests" };
        let (out, _) = run_in(&tmp, &stub, Some(true), theme, "1280x720", &format!("bench-playful:{row}:{what}"));
        let seen = out.lines().find_map(|l| l.strip_prefix("rests seen: ")).unwrap_or_default().to_string();
        for n in ["the window", "the piece on the window's sill", "surface: the window's sill"] {
            assert!(!seen.split(" | ").any(|k| k == n), "{theme}: \"{n}\" is still recorded; seen: {seen}");
        }
    }
    std::env::remove_var("POCKETUI_ROOM_TIME");
    std::env::remove_var("POCKETUI_ROOM_MONTH");
    drop(stub);
    assert!(wrong.is_empty(), "{} faults:\n{}", wrong.len(), wrong.iter().take(60).cloned().collect::<Vec<_>>().join("\n"));
}

/// The photo album in a game's case (`pocket_ui::album`, seen from above): every photo lies on the open case, none hangs
/// over its edge into the air, at every selection and at eight moments of every slide, at the sizes the case is painted
/// at on 640x480, 1240x1080 and 1920x1080, for 1 to 12 photos of every shape (square Game Boy, wide Game Boy Advance,
/// a 4:3 console). The photo held up close (A) is the one deliberate exception: it is in the hand.
#[test]
fn every_photo_in_the_album_lies_on_its_case() {
    use pocket_ui::album::{self, Photo};
    let shapes = [(160u32, 144u32), (240, 160), (320, 240)];
    let mut checked = 0;
    for (w, h) in [(640, 480), (1240, 1080), (1920, 1080)] {
        // The room under the card's top line, above the hint bar, as the box gives it (`game_box::room`).
        let room = (24, h / 9, w - 48, h - h / 9 - 90);
        let plan = pocket_ui::album_paint::plan(room, 66);
        let (_, _, cw, ch) = plan.case;
        let aspect = cw as f32 / ch as f32;
        for size in shapes {
            for n in 1..=12usize {
                let photos: Vec<Photo> = (0..n).map(|i| Photo { file: PathBuf::from(format!("{i}.jpg")), at: 1_795_000_000 + i as i64 * 7919, note: vec![], size }).collect();
                for sel in 0..n {
                    for step in 0..8 {
                        // Sliding in from the photo before it (Right) and from the one after it (Left).
                        for from in [sel as f32 - 1.0 + step as f32 / 8.0, sel as f32 + 1.0 - step as f32 / 8.0] {
                            for p in album::layout_at(&photos, from, aspect, plan.photo_h) {
                                for (x, y) in album::corners(&p, &photos[p.index], aspect, plan.photo_h) {
                                    assert!((0.0..=1.0).contains(&x) && (0.0..=1.0).contains(&y), "{w}x{h}, {n} photos {size:?}, at {from}: photo {} hangs over the case at ({x:.3}, {y:.3})", p.index);
                                }
                                checked += 1;
                            }
                        }
                    }
                }
            }
        }
    }
    println!("album: {checked} photo placements on their case");
}

/// Settings → Games → Shelf space's crate (`app_crate`, 10 places on the made-up library: the rest of its 63 games in the
/// boxes): every case in its box at every selection and through its way up out of it, the shelf's games picked to make
/// room, and swaps with the first, middle and last game of the shelf and onto a free place, twenty moments each, from three
/// places of the crate. A case rests on its box's floor or on the shelf's board, a box on the floor; only the case in
/// the hand on its way, and the selected or picked one lifted, are exempt.
///
/// Found by it while the crate was built (18.11.): the case going into a box landed before its neighbours had made room
/// (inside the next one), and out-of-view boxes stood on no floor.
#[test]
fn nothing_in_the_crate_floats_or_stands_inside_another_nor_while_two_games_swap() {
    let _guard = tempfile::tempdir().unwrap();
    let tmp = _guard.path().to_path_buf();
    let stub = wayland(&tmp);
    let mut wrong = Vec::new();
    for size in ["640x480", "1280x720", "720x720"] {
        let (out, _) = run_in(&tmp, &stub, Some(false), "shelf", size, "crate:20:rests");
        let seen = out.lines().find_map(|l| l.strip_prefix("rests seen: ")).unwrap_or_default();
        for n in ["box #", "case #", "shelf case #", "the game coming out", "the game going in", "surface: a box's floor", "surface: the floor", "surface: the shelf's board"] {
            assert!(seen.split(" | ").any(|k| k == n), "crate at {size}: \"{n}\" was never recorded; seen: {seen}");
        }
        let line = out.lines().find(|l| l.starts_with("rests frames=")).unwrap_or_else(|| panic!("no rests line at {size}:\n{out}"));
        let frames: u64 = line.split_whitespace().find_map(|w| w.strip_prefix("frames=")).unwrap().parse().unwrap();
        assert!(frames > 300, "crate at {size}: only {frames} frames checked");
        wrong.extend(out.lines().filter_map(|l| l.strip_prefix("rests fault: ")).map(|f| format!("crate at {size}: {f}")));
    }
    drop(stub);
    assert!(wrong.is_empty(), "{} faults:\n{}", wrong.len(), wrong.iter().take(60).cloned().collect::<Vec<_>>().join("\n"));
}
