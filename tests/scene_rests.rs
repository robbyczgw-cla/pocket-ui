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
    let what = match (theme, row.split_once('+')) {
        (_, Some((_, how))) => how,
        ("cabinet", _) => "rests-cabinet",
        _ => "rests",
    };
    let row = row.split('+').next().unwrap();
    let (out, _) = run_in(tmp, stub, Some(lived), theme, size, &format!("bench-playful:{row}:{what}"));
    let seen = out.lines().find_map(|l| l.strip_prefix("rests seen: ")).unwrap_or_default();
    println!("{theme} {row} at {size}, lived_in {lived}: {seen}");
    let need: &[&str] = if what == "rests-launch" {
        &[]
    } else if theme == "cabinet" {
        &["shelf # case #", "shelf # case #, out", "the TV", "surface: a shelf of the cabinet", "surface: the cabinet's top board"]
    } else {
        &["case #", "surface: the shelf's board"]
    };
    for n in need {
        assert!(seen.split(" | ").any(|k| k == *n), "{theme} {row} at {size}: \"{n}\" was never recorded; seen: {seen}");
    }
    if what == "rests-launch" {
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
/// Not in the gate yet (18.11.): a console stands `px(10)` over the stage's floor by design (`paint_plain`, `paint_rig`:
/// a contact shadow under it), which this check reports as floating; and the real mechanisms (`paint_rig`) are painted
/// in several layers that are not recorded yet. Both wait for the lead's decision.
#[test]
#[ignore = "open: consoles stand px(10) over the stage floor; the rigs' layers are not recorded yet"]
fn nothing_floats_while_a_game_goes_into_its_console() {
    let _guard = tempfile::tempdir().unwrap();
    let tmp = _guard.path().to_path_buf();
    let stub = wayland(&tmp);
    let mut wrong = Vec::new();
    for size in ["640x480", "1280x720"] {
        let (frames, faults) = rests(&tmp, &stub, false, "shelf", size, "favorites+rests-launch");
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
    for (k, ms) in [(0, 1500), (1, 1500), (2, 1500)] {
        std::env::set_var("POCKETUI_SNAPSHOT_LAUNCH_MS", ms.to_string());
        let _ = run_in(&tmp, &stub, Some(false), "shelf", "640x480", &format!("shelf:favorites:shelf:games:{k}"));
    }
    drop(stub);
}
