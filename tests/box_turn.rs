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
fn frame(tmp: &Path, lib: &Path, stub: &Option<Stub>, state: &str, size: &str, ms: u32) -> (i32, i32, Vec<u8>) {
    let png = tmp.join(format!("turn-{}-{size}-{ms}.png", state.replace([':', ' '], "_")));
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
    if let Some(s) = stub {
        cmd.env("WAYLAND_DISPLAY", &s.1);
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
    for state in ["box:psx:castlevania:r", "box:snes:chrono:r"] {
        for size in ["640x480", "1240x1080"] {
            // Every 60 Hz frame from the start of the turn to past its end (Cozy makes it slower). Squeezed flat it was 24
            // px wide at 640 half way; turning in perspective with its edge it stays above 50.
            let frames: Vec<_> = (0..=26).map(|k| frame(tmp.path(), &lib, &stub, state, size, k * 1000 / 60)).collect();
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
}
