//! Box art read ahead of the selection (src/app_cover_prefetch.rs), checked through the binary
//! (`pocket-ui --snapshot prefetch-check:<look>:gbc`, which prints what it saw), with the covers decoded on their threads
//! as on a device:
//!
//! * after a move and a rest without frames, the next games' covers are already decoded, so the next move's first
//!   frame shows its cover instead of a plain stand-in front (Shelf, Cabinet, Easy) or no picture (Split);
//! * reading far-away windows never pushes the cover in view out of its cache (the Cabinet's cuts were cleared all at
//!   once at 48, Split's pictures went oldest first).
//!
//! Each check runs once more with `POCKETUI_COVER_PREFETCH=0`: without reading ahead the first frame has no cover, so
//! the check sees a difference that comes from the read-ahead and not from a slow machine.
//!
//! Needs python3 on Linux without a compositor (scripts/wayland-stub.py, as tests/shelf_moves.rs).

use std::path::{Path, PathBuf};
use std::process::Command;

include!("common/root.rs");

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

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap().flatten() {
        let target = to.join(e.file_name());
        if e.file_name() == "covers" && e.path().is_dir() {
            // Read-only fixture covers: symlink instead of copying 120 PNG files on every run.
            std::os::unix::fs::symlink(e.path(), &target).unwrap();
        } else if e.path().is_dir() {
            copy_dir(&e.path(), &target);
        } else {
            std::fs::copy(e.path(), target).unwrap();
        }
    }
}

/// A library of 120 GBC games with box art, made once per test.
fn library() -> (tempfile::TempDir, Option<Stub>) {
    let tmp = tempfile::tempdir().unwrap();
    assert!(Command::new("python3")
        .args(["-I", &format!("{ROOT}/scripts/perf-library.py"), tmp.path().to_str().unwrap(), "120", "400", "560"])
        .stdout(std::process::Stdio::null())
        .status()
        .unwrap()
        .success());
    let wayland = wayland_for_snapshots(tmp.path());
    (tmp, wayland)
}

/// `prefetch-check`'s two lines for a look, with reading ahead on or off.
fn check(tmp: &Path, wayland: &Option<Stub>, look: &str, on: bool) -> (String, String) {
    let data = tmp.join(format!("data-{look}-{on}"));
    let _ = std::fs::remove_dir_all(&data);
    copy_dir(&tmp.join("data"), &data);
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
    cmd.args(["--snapshot", &format!("prefetch-check:{look}:gbc"), tmp.join(format!("check-{look}-{on}.png")).to_str().unwrap()])
        .env("POCKETUI_MOCK", "1")
        .env("POCKETUI_MOCK_NOW", FIXTURE_NOW)
        .env("POCKETUI_SEALED", "0")
        .env("POCKETUI_SNAPSHOT_SIZE", "640x480")
        .env("POCKETUI_LANG", "en")
        .env("POCKETUI_COVER_PREFETCH", if on { "1" } else { "0" })
        .env("POCKETUI_ES_SYSTEMS", tmp.join("es_systems.cfg"))
        .env("POCKETUI_DATA", &data);
    if let Some(w) = wayland {
        cmd.env("WAYLAND_DISPLAY", &w.path);
    }
    let out = cmd.output().expect("run pocket-ui");
    assert!(out.status.success(), "{look}: {}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let line = |start: &str| stdout.lines().find(|l| l.starts_with(start)).unwrap_or_else(|| panic!("no {start} line in:\n{stdout}")).to_string();
    let _ = std::fs::remove_dir_all(&data);
    (line("prefetch-ahead "), line("prefetch-keep "))
}

/// `search-start-check`'s line for a look: a game started from the search, `warm` on or off.
fn search_start(tmp: &Path, wayland: &Option<Stub>, look: &str, warm: bool) -> String {
    let data = tmp.join(format!("data-start-{look}-{warm}"));
    let _ = std::fs::remove_dir_all(&data);
    copy_dir(&tmp.join("data"), &data);
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
    cmd.args(["--snapshot", &format!("search-start-check:{look}:gbc"), tmp.join("check.png").to_str().unwrap()])
        .env("POCKETUI_MOCK", "1")
        .env("POCKETUI_MOCK_NOW", FIXTURE_NOW)
        .env("POCKETUI_SEALED", "0")
        .env("POCKETUI_SNAPSHOT_SIZE", "640x480")
        .env("POCKETUI_LANG", "en")
        .env("POCKETUI_LAUNCH_COVER", if warm { "1" } else { "0" })
        .env("POCKETUI_ES_SYSTEMS", tmp.join("es_systems.cfg"))
        .env("POCKETUI_DATA", &data);
    if let Some(w) = wayland {
        cmd.env("WAYLAND_DISPLAY", &w.path);
    }
    let out = cmd.output().expect("run pocket-ui");
    assert!(out.status.success(), "{look}: {}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let _ = std::fs::remove_dir_all(&data);
    stdout.lines().find(|l| l.starts_with("search-start ")).unwrap_or_else(|| panic!("no search-start line in:\n{stdout}")).to_string()
}

#[test]
fn a_game_started_from_the_search_opens_its_scene_on_the_real_cover() {
    let (tmp, wayland) = library();
    // Easy ("big") shows the games of a system as a plain list (`paint_minimal`): no case front to start on, and Split has no insert scene.
    for look in ["shelf", "cabinet"] {
        let line = search_start(tmp.path(), &wayland, look, true);
        println!("{look}: {line}");
        assert!(line.contains("selected=true"), "{look}: the shelf is not on the started game: {line}");
        assert!(line.contains("asked_cover=true"), "{look}: the search had no cover to share: {line}");
        assert!(line.contains("first_frame_cover=true"), "{look}: the first frame of the start showed a stand-in front: {line}");
        // Red without it: the cover is only asked for on the thread, as before.
        let off = search_start(tmp.path(), &wayland, look, false);
        println!("{look} without: {off}");
        assert!(off.contains("first_frame_cover=false"), "{look}: the check can't tell the fix from none: {off}");
    }
}

const LOOKS: [&str; 4] = ["shelf", "cabinet", "big", "split"];

#[test]
fn the_next_games_covers_are_decoded_before_they_come_into_view() {
    // Each `prefetch-check` run already reports both `prefetch-ahead` and `prefetch-keep`; check both contracts here
    // across all four looks in parallel instead of rebuilding the 120-cover fixture and re-running the same four
    // subprocesses in a second test.
    let (tmp, wayland) = library();
    let root = tmp.path();
    let wref = &wayland;
    std::thread::scope(|s| {
        for look in LOOKS {
            s.spawn(move || {
                let (ahead, keep) = check(root, wref, look, true);
                println!("{look}: {ahead}\n{look}: {keep}");
                assert!(ahead.contains(" ready=4/4 "), "{look}: the next four covers were not read ahead: {ahead}");
                assert!(ahead.ends_with("first_frame=true"), "{look}: the next move's first frame had no cover: {ahead}");
                assert!(keep.contains("selected=true"), "{look}: the cover in view was pushed out: {keep}");
                let cached: usize = keep.rsplit("cached=").next().unwrap().parse().unwrap();
                // The caches' own bounds (Cabinet's cuts: 48 + the window being read; Split's pictures: 16).
                assert!(cached <= 48 + 8, "{look}: the cache grew past its bound: {keep}");
                // Red without reading ahead: the same move's first frame waits for its cover.
                let (off, _) = check(root, wref, look, false);
                println!("{look} without reading ahead: {off}");
                assert!(off.ends_with("first_frame=false"), "{look}: the check can't tell reading ahead from not: {off}");
            });
        }
    });
}
