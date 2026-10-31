//! The Shelf look at the ends of its shelf (`app_cabinet.rs` "the Shelf look", docs/DESIGN.md "Layout"): with the first
//! entry selected the row starts at the shelf's left end, with the last one selected it reaches the right end (or
//! stands whole from the left, where it fits), so the end of the row never leaves the shelf half empty; a short list
//! shows whole; nothing is painted past the shelf's ends. Rendered at the Mini V2's 1240x1080, the Pocket 5's
//! 1920x1080 (in its column) and the Mini V1's 1280x960, dark and light, with scripts/curator-fake-library.py's
//! library, and measured in the pictures: the spines along a line just above the board.
//!
//! Needs python3 (and, on Linux without a compositor, scripts/wayland-stub.py), as tests/text_size.rs.

use std::path::{Path, PathBuf};
use std::process::Command;

const ROOT: &str = env!("CARGO_MANIFEST_DIR");
/// The page's side margin (`palette::MARGIN`).
const MARGIN: i32 = 72;

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
    // Settings → Look → Look: Shelf, whatever the made-up library chose.
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

/// The shelf on a screen `w` x `h` as `app_cabinet::shelf_layout` lays it out: its left and right end (in the column
/// of a wide screen, `app_column::column_width`) and the line to look along, above the lifted spine's foot.
fn shelf_line(w: i32, h: i32) -> (i32, i32, i32) {
    let top = if h < 600 { 80 } else { 104 };
    let bottom = h - 112;
    let f = (((bottom - top).max(200) as f32 * 0.42 / 120.0 * 2.0).floor() / 2.0).clamp(1.0, 4.0);
    let floor = bottom - (8.0 * f).round() as i32;
    let lift = (10.0 * f).round() as i32;
    let col = if w >= 1600 { (w * 3 / 4).max(1440).min(w) } else { w };
    let x0 = (w - col) / 2;
    (x0 + MARGIN, x0 + col - MARGIN, floor - lift - 12)
}

/// What stands on the shelf along that line: (left, right) of each run of pixels that differ from the page.
fn runs((w, _, px): &(i32, i32, Vec<u8>), y: i32) -> Vec<(i32, i32)> {
    let at = |x: i32| {
        let i = ((y * w + x) * 3) as usize;
        (px[i], px[i + 1], px[i + 2])
    };
    let bg = at(2);
    let mut out: Vec<(i32, i32)> = Vec::new();
    for x in 0..*w {
        if at(x) == bg {
            continue;
        }
        match out.last_mut() {
            Some(r) if r.1 == x - 1 => r.1 = x,
            _ => out.push((x, x)),
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
                let (left, right, y) = shelf_line(pic.0, pic.1);
                let r = runs(&pic, y);
                let what = format!("{state} at {size}, {look}: along y {y} from {left} to {right}: {r:?}");
                let (first, last) = (r.first().map_or(i32::MAX, |r| r.0), r.last().map_or(0, |r| r.1));
                if first < left || last > right {
                    wrong.push(format!("{what}: painted past the shelf's ends"));
                }
                let from_left = first <= left + 4;
                let to_right = last >= right - 4;
                match state {
                    // The first entry selected: the row starts at the shelf's left end.
                    "home:0" if !from_left => wrong.push(format!("{what}: doesn't start at the left end")),
                    // The last ones: the row reaches the right end, or stands whole from the left.
                    "home" | "home:-2" if !(to_right || from_left) => wrong.push(format!("{what}: the shelf's end left empty")),
                    // The short list (Curator's seven entries, the last selected): all of them, from the left (counted on
                    // the dark page; on the light one the cases' shadows fill the gaps between them).
                    "curator:hub:shelf:6" if !from_left || (look == "dark" && r.len() < 7) => {
                        wrong.push(format!("{what}: not the seven entries from the left"))
                    }
                    _ => {}
                }
            }
        }
    }
    drop(stub);
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}
