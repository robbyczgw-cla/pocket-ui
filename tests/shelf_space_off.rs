//! Settings → Games → Shelf space is Off unless a player switches it on (19.11.2026: optional):
//! after the update a shelf looks exactly as before. Off, the made-up library's Home and a system's row are the same
//! picture whether shelf_space.json is missing, says Off, or still holds the games of an earlier time it was on; Home has no
//! Crate row; and only Settings' question switches it on (no first-start step writes the file).
//!
//! On the made-up library of tests/common/shelf_library.rs. Needs python3 on Linux without a compositor
//! (scripts/wayland-stub.py).

use std::path::{Path, PathBuf};
use std::process::Command;

include!("common/root.rs");
include!("common/shelf_library.rs");

/// pocket-ui `--snapshot <state>` on the made-up library with `space` as shelf_space.json (None: no file): the picture's
/// pixels and the words painted.
fn shot(tmp: &Path, stub: &Option<Stub>, name: &str, space: Option<&str>, state: &str) -> (Vec<u8>, String) {
    let lib = library_in(&tmp.join(format!("lib-{name}")), None, "shelf");
    if let Some(space) = space {
        std::fs::write(lib.join("data/shelf_space.json"), space).unwrap();
    }
    let png = tmp.join(format!("{name}.png"));
    let words = tmp.join(format!("{name}.txt"));
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
    if let Some(s) = stub {
        cmd.env("WAYLAND_DISPLAY", &s.1);
    }
    let out = cmd
        .args(["--snapshot", state, png.to_str().unwrap()])
        .env("POCKETUI_MOCK", "1")
        .env("POCKETUI_MOCK_NOW", FIXTURE_NOW)
        .env("POCKETUI_LANG", "en")
        .env("POCKETUI_TEXT_SIZE", "normal")
        .env("POCKETUI_SNAPSHOT_SIZE", "640x480")
        .env("POCKETUI_VISIBLE_TEXT", &words)
        .env("POCKETUI_ES_SYSTEMS", lib.join("es_systems.cfg"))
        .env("POCKETUI_DATA", lib.join("data"))
        .output()
        .expect("run pocket-ui");
    assert!(out.status.success(), "{state}: {}", String::from_utf8_lossy(&out.stderr));
    // Nothing of the snapshot run switched it on.
    let after = std::fs::read_to_string(lib.join("data/shelf_space.json")).unwrap_or_default();
    assert!(!after.contains(r#""limit":1"#) && !after.contains(r#""limit":2"#) && !after.contains(r#""limit":5"#), "{name}: switched on by itself: {after}");
    (std::fs::read(&png).unwrap(), std::fs::read_to_string(&words).unwrap_or_default())
}

#[test]
fn off_by_default_the_shelf_looks_as_before_and_home_has_no_crate() {
    let _guard = tempfile::tempdir().unwrap();
    let tmp = _guard.path().to_path_buf();
    let stub = wayland(&tmp);
    let shelf = |n: usize| (0..n).map(|i| format!("\"/made-up/{i}.sfc\"")).collect::<Vec<_>>().join(",");
    let left_over = format!(r#"{{"limit":null,"shelf":[{}]}}"#, shelf(3));
    for state in ["shelf::shelf", "shelf:snes:shelf:games:0", "shelf:snes:shelf:games:12"] {
        let tag = state.replace(':', "_");
        let (none, words) = shot(&tmp, &stub, &format!("none{tag}"), None, state);
        let (off, _) = shot(&tmp, &stub, &format!("off{tag}"), Some(r#"{"limit":null,"shelf":[]}"#), state);
        let (old, _) = shot(&tmp, &stub, &format!("old{tag}"), Some(&left_over), state);
        assert!(none == off && off == old, "{state}: Off must look the same with no file, Off, and an old shelf left in it");
        assert!(!words.lines().any(|l| l == "Crate" || l.contains("in the crate")), "{state}: no crate while Off: {words}");
    }
    drop(stub);
}

/// The first start's steps never write the file (it is Settings' question alone, `app_crate::shelf_space_step`).
#[test]
fn no_first_start_step_switches_shelf_space_on() {
    let src = std::fs::read_to_string(Path::new(&ROOT).join("src/setup.rs")).unwrap();
    assert!(!src.contains("shelf_space"), "src/setup.rs must not touch Shelf space");
    let callers: Vec<PathBuf> = std::fs::read_dir(Path::new(&ROOT).join("src"))
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "rs"))
        .filter(|p| std::fs::read_to_string(p).unwrap_or_default().contains(".switch_on("))
        .collect();
    let names: Vec<String> = callers.iter().map(|p| p.file_name().unwrap().to_string_lossy().into_owned()).collect();
    assert!(names.iter().all(|n| n == "app_crate.rs" || n == "shelf_space.rs"), "Shelf space switched on outside Settings: {names:?}");
}
