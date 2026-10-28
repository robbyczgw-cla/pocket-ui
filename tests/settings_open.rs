//! The first frame of Settings (START on the shelf), guarded (docs/LATENCY.md, "Settings' first frame"): the binary
//! paints the categories page the way `draw` does after START (`pocket-ui --snapshot bench-settings`, which prints
//! its numbers) and this test checks them.
//!
//! What is checked is what costs on the Mini V2 and does not depend on the computer: the page draws no glyph of its own
//! (the thread of `Text::prewarm` drew them) and runs no string through the German table's patterns (what
//! `Text::fit` returns is German already). Time is checked against itself: the first frame is not much slower than
//! the same frame again (the minimum of a few runs), and nothing is slow in absolute terms.
//!
//! Needs python3 on Linux without a compositor (scripts/wayland-stub.py, as tests/i18n_snapshots.rs).

use std::path::{Path, PathBuf};
use std::process::Command;

const ROOT: &str = env!("CARGO_MANIFEST_DIR");

/// scripts/wayland-stub.py, while it runs.
struct Stub {
    path: PathBuf,
    child: std::process::Child,
}

impl Drop for Stub {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// A snapshot opens no window, but winit's event loop wants a Wayland compositor to say hello first.
fn wayland_for_snapshots(tmp: &Path) -> Option<Stub> {
    if !cfg!(target_os = "linux") || std::env::var_os("WAYLAND_DISPLAY").is_some_and(|w| !w.is_empty()) {
        return None;
    }
    let path = tmp.join("wayland-stub");
    let child = Command::new("python3")
        .args(["-I", &format!("{ROOT}/scripts/wayland-stub.py"), path.to_str().unwrap()])
        .spawn()
        .expect("python3 for scripts/wayland-stub.py");
    let stub = Stub { path, child };
    for _ in 0..200 {
        if stub.path.exists() {
            return Some(stub);
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    panic!("scripts/wayland-stub.py didn't start");
}

#[derive(Debug)]
struct Bench {
    first_us: f64,
    first_glyphs: usize,
    first_patterns: usize,
    steady_us: f64,
    transition_us: f64,
}

fn run(tmp: &Path, wayland: &Option<Stub>, lang: &str) -> Bench {
    let data = tmp.join(format!("data-{lang}"));
    std::fs::create_dir_all(&data).unwrap();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
    cmd.args(["--snapshot", "bench-settings", tmp.join("bench.png").to_str().unwrap()])
        .env("POCKETUI_MOCK", "1")
        .env("POCKETUI_LANG", lang)
        // The corner as on a device with Wi-Fi: the symbol is part of the frame.
        .env("POCKETUI_SNAPSHOT_WIFI", "1")
        .env("POCKETUI_ES_SYSTEMS", tmp.join("none.cfg"))
        .env("POCKETUI_DATA", &data);
    if let Some(w) = wayland {
        cmd.env("WAYLAND_DISPLAY", &w.path);
    }
    let out = cmd.output().expect("run pocket-ui");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8_lossy(&out.stdout);
    let line = stdout.lines().find(|l| l.starts_with("bench-settings ")).unwrap_or_else(|| panic!("no bench line in:\n{stdout}"));
    let field = |name: &str| -> f64 {
        let key = format!("{name}=");
        let at = line.find(&key).unwrap_or_else(|| panic!("no {name} in {line}")) + key.len();
        line[at..].split_whitespace().next().unwrap().parse().unwrap_or_else(|_| panic!("{name} in {line}"))
    };
    Bench {
        first_us: field("first_us"),
        first_glyphs: field("first_glyphs") as usize,
        first_patterns: field("first_patterns") as usize,
        steady_us: field("steady_us"),
        transition_us: field("transition_us"),
    }
}

#[test]
fn settings_first_frame_draws_no_glyph_runs_no_pattern_and_is_not_much_slower_than_the_next() {
    let _tmp_guard = tempfile::tempdir().unwrap();
    let tmp = _tmp_guard.path().to_path_buf();
    let wayland = wayland_for_snapshots(&tmp);
    for lang in ["en", "de"] {
        let runs: Vec<Bench> = (0..5).map(|_| run(&tmp, &wayland, lang)).collect();
        for r in &runs {
            assert_eq!(r.first_glyphs, 0, "{lang}: the categories page found every letter in the cache ({r:?})");
            assert_eq!(r.first_patterns, 0, "{lang}: no string of the page ran the German table's patterns ({r:?})");
        }
        let min = |f: fn(&Bench) -> f64| runs.iter().map(f).fold(f64::MAX, f64::min);
        let (first, steady, transition) = (min(|b| b.first_us), min(|b| b.steady_us), min(|b| b.transition_us));
        println!("{lang}: first frame {first:.0} us, the same again {steady:.0} us, of it the page transition {transition:.0} us");
        // About 1.6 ms against 1.3 ms on the build computer; before the letters were drawn ahead of time and the
        // German lines were not run through the patterns, 2.4 and 3.2 ms (English, German) against 2.0 and 1.3.
        assert!(first <= steady * 1.6 + 300.0, "{lang}: the first frame ({first:.0} us) costs much more than the next ({steady:.0} us)");
        // Nowhere near slow: a frame is 16 ms. (The Mini V2 is three to four times slower than the build computer.)
        assert!(first < 8000.0 && steady < 8000.0, "{lang}: {first:.0} / {steady:.0} us");
    }
    drop(wayland);
}
