//! The Shelf look at the ends of its list (`shelf_first` in src/main.rs, docs/DESIGN.md "Layout"): at rest the selection
//! is the third row, but the list stops at its end, so with the last row selected the list reaches down to the hints
//! instead of leaving most of the screen empty; with the first row selected it starts at the top; a short list shows
//! whole. Rendered at the Mini V2's 1240x1080, the Pocket 5's 1920x1080 and the Mini V1's 1280x960, dark and light,
//! with scripts/curator-fake-library.py's library, and measured in the pictures: the bands of rows down the list's
//! middle.
//!
//! Needs python3 (and, on Linux without a compositor, scripts/wayland-stub.py), as tests/text_size.rs.

use std::path::{Path, PathBuf};
use std::process::Command;

const ROOT: &str = env!("CARGO_MANIFEST_DIR");
/// Where the Shelf look's list starts (`paint_shelf`'s `top`) and how far above the bottom edge it ends at most.
const TOP: i32 = 112;
const BOTTOM_GAP: i32 = 120;

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
    for _ in 0..200 {
        if stub.1.exists() {
            return Some(stub);
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    panic!("scripts/wayland-stub.py didn't start");
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap().flatten() {
        let target = to.join(e.file_name());
        if e.path().is_dir() {
            copy_dir(&e.path(), &target);
        } else {
            std::fs::copy(e.path(), target).unwrap();
        }
    }
}

/// The picture: width, height, RGB.
fn shot(tmp: &Path, stub: &Option<Stub>, look: &str, size: &str, state: &str) -> (i32, i32, Vec<u8>) {
    let lib = tmp.join(format!("lib-{look}"));
    if !lib.exists() {
        let ok = Command::new("python3")
            .args(["-I", &format!("{ROOT}/scripts/curator-fake-library.py"), lib.to_str().unwrap(), look])
            .stdout(std::process::Stdio::null())
            .status()
            .expect("python3 for scripts/curator-fake-library.py");
        assert!(ok.success());
    }
    let name = format!("{look}-{size}-{}", state.replace(':', "_"));
    let data = tmp.join(format!("data-{name}"));
    copy_dir(&lib.join("data"), &data);
    // Settings → Look & feel → Look: Shelf, whatever the made-up library chose.
    let settings = data.join("settings.json");
    let mut json: serde_json::Value = std::fs::read_to_string(&settings).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_else(|| serde_json::json!({}));
    json["theme"] = serde_json::json!("shelf");
    json["onboarded"] = serde_json::json!(true);
    std::fs::write(&settings, json.to_string()).unwrap();
    let png = tmp.join(format!("{name}.png"));
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
    if let Some(s) = stub {
        cmd.env("WAYLAND_DISPLAY", &s.1);
    }
    let out = cmd
        .args(["--snapshot", state, png.to_str().unwrap()])
        .env("POCKETUI_MOCK", "1")
        .env("POCKETUI_LANG", "en")
        .env("POCKETUI_TEXT_SIZE", "normal")
        .env("POCKETUI_SNAPSHOT_SIZE", size)
        .env("POCKETUI_ES_SYSTEMS", lib.join("es_systems.cfg"))
        .env("POCKETUI_DATA", &data)
        .output()
        .expect("run pocket-ui");
    assert!(out.status.success() && png.exists(), "{state} at {size}: {}", String::from_utf8_lossy(&out.stderr));
    let decoder = png::Decoder::new(std::fs::File::open(&png).unwrap());
    let mut reader = decoder.read_info().unwrap();
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).unwrap();
    assert_eq!(info.color_type, png::ColorType::Rgb);
    (info.width as i32, info.height as i32, buf[..info.buffer_size()].to_vec())
}

/// The rows down the middle of the list: (top, bottom) of each band of pixels that differ from the page's background.
fn bands((w, h, px): &(i32, i32, Vec<u8>)) -> Vec<(i32, i32)> {
    let at = |x: i32, y: i32| {
        let i = ((y * w + x) * 3) as usize;
        (px[i], px[i + 1], px[i + 2])
    };
    let bg = at(2, h / 2);
    let mut out: Vec<(i32, i32)> = Vec::new();
    for y in TOP..h - BOTTOM_GAP {
        if at(w / 2, y) == bg {
            continue;
        }
        match out.last_mut() {
            Some(b) if b.1 == y - 1 => b.1 = y,
            _ => out.push((y, y)),
        }
    }
    out
}

#[test]
fn the_shelf_stops_at_the_ends_of_its_list_at_every_size() {
    let _tmp_guard = tempfile::tempdir().unwrap();
    let tmp = _tmp_guard.path().to_path_buf();
    let stub = wayland(&tmp);
    let mut wrong = Vec::new();
    for look in ["dark", "light"] {
        for size in ["1240x1080", "1920x1080", "1280x960"] {
            for state in ["home", "home:-2", "home:0", "curator:hub:shelf:6"] {
                let pic = shot(&tmp, &stub, look, size, state);
                let b = bands(&pic);
                let h = pic.1;
                let (first, last) = (b.first().map_or(i32::MAX, |r| r.0), b.last().map_or(0, |r| r.1));
                let what = format!("{state} at {size}, {look}: rows {b:?}");
                // Every case: the list starts at its top edge.
                if first > TOP + 4 {
                    wrong.push(format!("{what}: the list doesn't start at the top"));
                }
                match state {
                    // The long list: it reaches down to the hints, whichever end is selected; less than a row is left.
                    "home" | "home:-2" | "home:0" if last < h - BOTTOM_GAP - 90 => wrong.push(format!("{what}: empty below the list")),
                    // The short list (Curator's seven rows, the last selected): all of them, none scrolled away.
                    "curator:hub:shelf:6" if b.len() != 7 => wrong.push(format!("{what}: not the seven rows")),
                    _ => {}
                }
            }
        }
    }
    drop(stub);
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}
