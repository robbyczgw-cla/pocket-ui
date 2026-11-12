//! A on a game must paint its insertion immediately, even when preparing the launch is slow.
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
fn a_game_starts_animating_before_slow_preparation_finishes() {
    let dir = tempfile::tempdir().unwrap();
    let tmp = dir.path();
    let wayland = wayland_for_snapshots(tmp);
    assert!(Command::new("python3").args(["-I", &format!("{ROOT}/scripts/curator-fake-library.py"), tmp.to_str().unwrap(), "dark"])
        .stdout(std::process::Stdio::null()).status().unwrap().success());
    // Already explained: A means Play, without the intentional first-start controls card.
    std::fs::write(tmp.join("data/controls-card-seen"), "gba\n").unwrap();
    // A large library exercises the actual chosen-game path, not a helper that merely starts a timer.
    for n in 0..3000 {
        std::fs::write(tmp.join(format!("roms/gba/Fixture {n:04}.gba")), b"game").unwrap();
    }
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
    cmd.args(["--snapshot", "bench-launch-response", tmp.join("response.png").to_str().unwrap()])
        .env("POCKETUI_TRACE", tmp.join("trace"))
        .env("POCKETUI_MOCK", "1").env("POCKETUI_TEST_PREPARE_MS", "300").env("POCKETUI_TEST_PREPARE_HOLD", "1")
        .env("POCKETUI_ES_SYSTEMS", tmp.join("es_systems.cfg"))
        .env("POCKETUI_STATES", tmp.join("states"))
        .env("POCKETUI_DATA", tmp.join("data"));
    if let Some(w) = &wayland { cmd.env("WAYLAND_DISPLAY", &w.path); }
    let out = cmd.output().unwrap();
    assert!(out.status.success(), "{}\n{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8_lossy(&out.stdout);
    let line = stdout.lines().find(|l| l.starts_with("bench-launch-response ")).expect(&stdout);
    let field = |k: &str| line.split(&format!("{k}=")).nth(1).unwrap().split_whitespace().next().unwrap().to_string();
    let first_cpu: f64 = field("first_cpu_ms").parse().unwrap();
    println!("{line}");
    let trace = std::fs::read_to_string(tmp.join("trace")).unwrap();
    let mut last = None;
    for line in trace.lines().filter(|l| l.contains(" step launch")) {
        let now: u64 = line.split_whitespace().next().unwrap().parse().unwrap();
        if let Some(before) = last { println!("prepare +{:.3} ms: {}", (now - before) as f64 / 1000.0, line); }
        last = Some(now);
    }
    // The order, not a stopwatch (the wall clock made this a flake on a busy gate, 11.11.): the first frame with the
    // insertion is painted while the preparation still waits for it, and A with that frame cost the UI thread little.
    assert_eq!(field("prepared_before_frame"), "false", "A waited for the slow preparation before painting: {line}");
    assert!(first_cpu <= 50.0, "A and its first frame took {first_cpu:.1} ms of the UI thread's time: {line}");
    let result = stdout.lines().find(|l| l.starts_with("bench-launch-result ")).expect(&stdout);
    assert!(result.contains("correct=true") && result.contains("notice=false"), "the exact selected game must eventually start: {result}");
    let frames: usize = result.split("frames=").nth(1).unwrap().split_whitespace().next().unwrap().parse().unwrap();
    assert!(frames >= 5, "the animation must keep painting during slow preparation: {result}");
    assert!(line.contains("inserting=true") && line.contains("changed=true"), "the first frame must actually show the animation: {line}");
}
