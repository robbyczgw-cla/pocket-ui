//! The launch's frames, guarded (docs/LATENCY.md, "Launch frames on the H700"): `pocket-ui --snapshot
//! bench-playful:gbc:launch` paints every frame of the Shelf look's launch five times, as `draw` paints it on a 640x480
//! panel (natively, `ScaledFrame::render`), and prints their thread CPU time beside a frame of the shelf standing still
//! measured in the same run; this test checks them.
//!
//! On the RG35XX Plus (H700) a launch frame cost 13.0 ms (p50) and 16.1 ms (p95) before 14.11.2026, 19 of 630 frames
//! over a 60 Hz frame: the shelf under the scene was painted anew every frame although nothing on it moves, and the
//! whole screen was darkened a slow way. Now 9.8 / 11.5 ms. Red before: `pages` was 630 (every frame). What is checked
//! does not depend on the computer: the shelf is painted once per launch (`app_cabinet::LaunchShelf`), and a launch
//! frame stays within a few frames of the shelf itself (5.4 to 5.7 at p95 on the H700 and the build computer; a
//! launch painted at design size again, as the old bench did, is about 13).
//!
//! Needs python3 on Linux without a compositor (scripts/wayland-stub.py, as tests/shelf_moves.rs).

use std::path::{Path, PathBuf};
use std::process::Command;

include!("common/root.rs");

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
    test_clock::wait_for(|| stub.path.exists(), std::time::Duration::from_secs(2), "Wayland stub listening");
    Some(stub)
}

#[test]
fn a_launch_paints_the_shelf_once_and_its_frames_cost_a_few_shelf_frames() {
    let _tmp_guard = tempfile::tempdir().unwrap();
    let tmp = _tmp_guard.path().to_path_buf();
    // 40 Game Boy Color games with box art, as the H700 measurement's library (scripts/perf-library.py, there 300).
    let lib = tmp.join("lib");
    let status = Command::new("python3")
        .args(["-I", &format!("{ROOT}/scripts/perf-library.py"), lib.to_str().unwrap(), "40", "400", "560"])
        .stdout(std::process::Stdio::null())
        .status()
        .expect("python3 for scripts/perf-library.py");
    assert!(status.success(), "perf-library.py failed");
    let wayland = wayland_for_snapshots(&tmp);
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
    cmd.args(["--snapshot", "bench-playful:gbc:launch", tmp.join("x.png").to_str().unwrap()])
        .env("POCKETUI_MOCK", "1")
        .env("POCKETUI_SNAPSHOT_SIZE", "640x480")
        .env("POCKETUI_SEALED", "0")
        .env("POCKETUI_ES_SYSTEMS", lib.join("es_systems.cfg"))
        .env("POCKETUI_DATA", lib.join("data"))
        .env_remove("POCKETUI_PAINT");
    if let Some(w) = &wayland {
        cmd.env("WAYLAND_DISPLAY", &w.path);
    }
    let out = cmd.output().expect("run pocket-ui");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8_lossy(&out.stdout);
    let line = stdout.lines().find(|l| l.starts_with("bench-playful ")).unwrap_or_else(|| panic!("no bench line in:\n{stdout}"));
    let field = |name: &str| -> f64 {
        let key = format!(" {name}=");
        let at = line.find(&key).unwrap_or_else(|| panic!("no {name} in {line}")) + key.len();
        line[at..].split_whitespace().next().unwrap().parse().unwrap_or_else(|_| panic!("{name} in {line}"))
    };
    println!("{line}");
    let (frames, p95, shelf, pages) = (field("frames"), field("p95_us"), field("shelf_p50_us"), field("pages"));
    assert!(frames >= 500.0, "five whole launches: {line}");
    // Once per launch (its first frame), not once per frame.
    assert!(pages <= 5.0, "the shelf under the launch is painted anew every frame: {line}");
    // A net for gross regressions only (the scene's own pictures cost what they cost): 5.4 to 5.7 now, 7.2 to 7.7 before.
    assert!(p95 <= 8.0 * shelf.max(50.0), "a launch frame costs {:.1} frames of the shelf (p95): {line}", p95 / shelf.max(1.0));
}
