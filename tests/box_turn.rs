//! The game box turning over (Left/Right on the box, `game_box::BoxView::flip`) as the player sees it, every 60 Hz
//! frame of the turn (`POCKETUI_SNAPSHOT_FLIP_MS`), on the made-up library (scripts/curator-fake-library.py): the box
//! turns in perspective with its edge coming round, so it never thins to a line and never jumps. 11.11.: it was
//! squeezed flat, to a hairline half way.
//!
//! Needs python3 (and, on Linux without a compositor, scripts/wayland-stub.py), as tests/disc_prints.rs.

use std::path::{Path, PathBuf};
use std::process::Command;

include!("common/root.rs");

struct Stub(std::process::Child, PathBuf);

impl Drop for Stub {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn wayland(tmp: &Path) -> Option<Stub> {
    if !cfg!(target_os = "linux") || std::env::var_os("WAYLAND_DISPLAY").is_some_and(|w| !w.is_empty()) {
        return None;
    }
    let path = tmp.join("wayland-stub");
    let child = Command::new("python3").args(["-I", &format!("{ROOT}/scripts/wayland-stub.py"), path.to_str().unwrap()]).spawn().expect("python3");
    let stub = Stub(child, path);
    test_clock::wait_for(|| stub.1.exists(), std::time::Duration::from_secs(2), "Wayland stub listening");
    Some(stub)
}

/// One frame of the turn `ms` after it began: (width, height, RGB).
fn frame(tmp: &Path, lib: &Path, stub: Option<&Path>, state: &str, size: &str, ms: u32) -> (i32, i32, Vec<u8>) {
    let png = tmp.join(format!("turn-{}-{size}-{ms}.png", state.replace([':', ' '], "_")));
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
    if let Some(s) = stub {
        cmd.env("WAYLAND_DISPLAY", s);
    }
    let out = cmd
        .args(["--snapshot", state, png.to_str().unwrap()])
        .env("POCKETUI_MOCK", "1")
        .env("POCKETUI_MOCK_NOW", FIXTURE_NOW)
        .env("POCKETUI_LANG", "en")
        .env("POCKETUI_SNAPSHOT_SIZE", size)
        .env("POCKETUI_ES_SYSTEMS", lib.join("es_systems.cfg"))
        .env("POCKETUI_DATA", lib.join("data"))
        .env("POCKETUI_SNAPSHOT_FLIP_MS", ms.to_string())
        .output()
        .expect("run pocket-ui");
    assert!(out.status.success(), "{state} {ms}: {}", String::from_utf8_lossy(&out.stderr));
    let mut reader = png::Decoder::new(std::fs::File::open(&png).unwrap()).read_info().unwrap();
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).unwrap();
    (info.width as i32, info.height as i32, buf)
}

/// How many columns of the middle row show something other than the page behind the box.
fn box_columns((w, h, px): &(i32, i32, Vec<u8>), page: [u8; 3]) -> i32 {
    let y = h / 2;
    (0..*w).filter(|x| {
        let i = ((y * w + x) * 3) as usize;
        px[i..i + 3].iter().zip(page).any(|(a, b)| a.abs_diff(b) > 12)
    })
    .count() as i32
}

#[test]
fn the_box_turns_over_in_perspective_never_to_a_line() {
    let tmp = tempfile::tempdir().unwrap();
    let stub = wayland(tmp.path());
    let lib = tmp.path().join("lib");
    let ok = Command::new("python3")
        .env("POCKETUI_MOCK_NOW", FIXTURE_NOW)
        .args(["-I", &format!("{ROOT}/scripts/curator-fake-library.py"), lib.to_str().unwrap(), "dark"])
        .stdout(std::process::Stdio::null())
        .status()
        .expect("python3 for scripts/curator-fake-library.py");
    assert!(ok.success());
    let stub_path = stub.as_ref().map(|s| s.1.clone());
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get().min(6));
    let mut jobs = Vec::new();
    for state in ["box:psx:castlevania:r", "box:snes:chrono:r"] {
        for size in ["640x480", "1240x1080"] {
            for k in 0..=26u32 {
                jobs.push((state, size, k));
            }
        }
    }
    let next = std::sync::atomic::AtomicUsize::new(0);
    let results: std::sync::Mutex<Vec<Option<((&str, &str, u32), (i32, i32, Vec<u8>))>>> = std::sync::Mutex::new(vec![None; jobs.len()]);
    std::thread::scope(|s| {
        for _ in 0..threads {
            s.spawn(|| loop {
                let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                let Some(&(state, size, k)) = jobs.get(i) else { break };
                let f = frame(tmp.path(), &lib, stub_path.as_deref(), state, size, k * 1000 / 60);
                results.lock().unwrap()[i] = Some(((state, size, k), f));
            });
        }
    });
    let results = results.into_inner().unwrap();
    for chunk in results.chunks_exact(27) {
        let frames: Vec<(i32, i32, Vec<u8>)> = chunk.iter().map(|r| r.as_ref().unwrap().1.clone()).collect();
        let (state, size, _) = chunk[0].as_ref().unwrap().0;
        let (w, _, _) = frames[0];
        let page = {
            let (fw, fh, px) = &frames[0];
            let i = (((fh / 2) * fw + 2) * 3) as usize;
            [px[i], px[i + 1], px[i + 2]]
        };
        let widths: Vec<i32> = frames.iter().map(|f| box_columns(f, page)).collect();
        let narrowest = *widths.iter().min().unwrap();
        assert!(narrowest >= w / 16, "{state} {size}: the box thins to {narrowest} px of {w}: {widths:?}");
        for (k, pair) in widths.windows(2).enumerate() {
            assert!((pair[1] - pair[0]).abs() <= w / 7, "{state} {size}: frame {} jumps from {} to {} px: {widths:?}", k + 1, pair[0], pair[1]);
        }
    }
}

/// Brick review 20.11.2026 (NEON-BRICK-ANIM #2): turning a box over, its perspective grew it past the room it stands in,
/// over the title line (black rows of the face's picture wiped letters of "PlayStation · Castlevania") and into the hint
/// bar, on the Brick for every box and on the Plus for the Game Boy Advance's and the PSP's. The box marks the room and the
/// turning face's extent (`shares`: `box-room`, `box-turn`), the page its strings (`nav_probe`): through the whole turn
/// the face stays in the room, under the title line's letters and over the hints.
#[test]
fn the_turning_box_stays_between_the_title_line_and_the_hints() {
    let tmp = tempfile::tempdir().unwrap();
    let stub = wayland(tmp.path());
    let lib = tmp.path().join("lib");
    let ok = Command::new("python3")
        .env("POCKETUI_MOCK_NOW", FIXTURE_NOW)
        .args(["-I", &format!("{ROOT}/scripts/curator-fake-library.py"), lib.to_str().unwrap(), "dark"])
        .stdout(std::process::Stdio::null())
        .status()
        .expect("python3 for scripts/curator-fake-library.py");
    assert!(ok.success());
    let mut jobs = Vec::new();
    for (size, device) in [("640x480", "rg35xx-plus"), ("1024x768", "brick")] {
        for state in ["box:psx:castlevania:r", "box:n64:mario:r", "box:gba:golden:r", "box:psp:burnout:r"] {
            // The turn takes 280 ms (`game_box::FLIP`): frames from its start to past its middle and on.
            for k in [2, 4, 6, 7, 8, 9, 10, 12, 14] {
                jobs.push((size, device, state, k));
            }
        }
    }
    let stub_path = stub.as_ref().map(|s| s.1.clone());
    let next = std::sync::atomic::AtomicUsize::new(0);
    let bad = std::sync::Mutex::new(Vec::new());
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get().min(6));
    std::thread::scope(|s| {
        for _ in 0..threads {
            s.spawn(|| loop {
                let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                let Some(&(size, device, state, k)) = jobs.get(i) else { break };
                let ms = k * 1000 / 60;
                let (png, probe) = (tmp.path().join(format!("t-{i}.png")), tmp.path().join(format!("t-{i}.probe")));
                let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
                if let Some(sp) = &stub_path {
                    cmd.env("WAYLAND_DISPLAY", sp);
                }
                let out = cmd
                    .args(["--snapshot", state, png.to_str().unwrap()])
                    .env("POCKETUI_MOCK", "1")
                    .env("POCKETUI_MOCK_NOW", FIXTURE_NOW)
                    .env("POCKETUI_LANG", "en")
                    .env("POCKETUI_DEVICE", device)
                    .env("POCKETUI_SNAPSHOT_SIZE", size)
                    .env("POCKETUI_SHARES", "1")
                    .env("POCKETUI_NAV_PROBE", &probe)
                    .env("POCKETUI_ES_SYSTEMS", lib.join("es_systems.cfg"))
                    .env("POCKETUI_DATA", lib.join("data"))
                    .env("POCKETUI_SNAPSHOT_FLIP_MS", ms.to_string())
                    .output()
                    .expect("run pocket-ui");
                let at = format!("{state} {size} {ms} ms");
                assert!(out.status.success(), "{at}: {}", String::from_utf8_lossy(&out.stderr));
                let stdout = String::from_utf8_lossy(&out.stdout);
                // "share <name> x=… y=… w=… h=… canvas=…": (y, bottom).
                let share = |name: &str| {
                    stdout.lines().filter_map(|l| l.strip_prefix("share ")).find(|l| l.split_whitespace().next() == Some(name)).map(|l| {
                        let v = |k: &str| l.split_whitespace().find_map(|p| p.strip_prefix(k)).and_then(|n| n.parse::<i32>().ok()).unwrap();
                        (v("y="), v("y=") + v("h="))
                    })
                };
                let (Some(room), Some(turn)) = (share("box-room"), share("box-turn")) else {
                    bad.lock().unwrap().push(format!("{at}: no turn painted (marks: {stdout})"));
                    continue;
                };
                // A pixel's rounding either way (the room's middle is a whole line).
                if turn.0 < room.0 - 1 || turn.1 > room.1 + 1 {
                    bad.lock().unwrap().push(format!("{at}: the turning face {turn:?} leaves its room {room:?}"));
                }
                // The strings over the room (the title line, the status corner) end over the face; the hints start under it.
                let record = std::fs::read_to_string(&probe).unwrap_or_default();
                let rects: Vec<(Vec<i32>, &str)> = record.lines().filter_map(|l| {
                    let v: Vec<&str> = l.splitn(5, '\t').collect();
                    (v.len() == 5).then(|| (v[..4].iter().map(|n| n.parse().unwrap_or(0)).collect(), v[4]))
                }).collect();
                let title = rects.iter().filter(|(r, s)| r[3] <= room.0 + 4 && !s.starts_with('‹')).map(|(r, _)| r[3]).max();
                let hints = rects.iter().filter(|(_, s)| s.starts_with("‹nav:hint›")).map(|(r, _)| r[1]).min();
                if title.is_none_or(|t| turn.0 < t) {
                    bad.lock().unwrap().push(format!("{at}: the turning face from {} reaches into the title line ending at {title:?}", turn.0));
                }
                if hints.is_none_or(|h| turn.1 > h) {
                    bad.lock().unwrap().push(format!("{at}: the turning face down to {} reaches into the hints from {hints:?}", turn.1));
                }
            });
        }
    });
    let bad = bad.into_inner().unwrap();
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}
