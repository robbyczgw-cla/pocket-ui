//! Printed discs as the player meets them (docs/DESIGN.md "Media", "Printed discs"), on the made-up library
//! (scripts/curator-fake-library.py):
//!
//! - X on a PlayStation game in the Shelf look opens its case and its disc comes out printed with the cover's art,
//!   on the Mini V2 and on 640x480, dark and light;
//! - a game without a cover gets a plain print with its title, never an empty disc;
//! - the Cozy launch carries the printed disc to the console.
//!
//! Needs python3 (and, on Linux without a compositor, scripts/wayland-stub.py), as tests/stickers_tv.rs.

use std::path::{Path, PathBuf};
use std::process::Command;

const ROOT: &str = env!("CARGO_MANIFEST_DIR");
const CASTLEVANIA: &str = "Castlevania - Symphony of the Night (USA)";

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

fn library(tmp: &Path) -> PathBuf {
    let lib = tmp.join("lib");
    let ok = Command::new("python3")
        .args(["-I", &format!("{ROOT}/scripts/curator-fake-library.py"), lib.to_str().unwrap(), "dark"])
        .stdout(std::process::Stdio::null())
        .status()
        .expect("python3 for scripts/curator-fake-library.py");
    assert!(ok.success());
    lib
}

/// A red cover with some texture (a flat one would be trimmed as a margin), `w` x `h`.
fn write_cover(path: &Path, w: u32, h: u32) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut e = png::Encoder::new(std::fs::File::create(path).unwrap(), w, h);
    e.set_color(png::ColorType::Rgb);
    e.set_depth(png::BitDepth::Eight);
    let data: Vec<u8> = (0..w * h).flat_map(|i| [210, ((i % w * 37 + i / w * 11) % 50) as u8, (i % w * 5 % 40) as u8]).collect();
    e.write_header().unwrap().write_image_data(&data).unwrap();
}

struct Shot {
    w: i32,
    h: i32,
    px: Vec<u8>,
}

impl Shot {
    fn at(&self, x: i32, y: i32) -> (u8, u8, u8) {
        let i = ((y * self.w + x) * 3) as usize;
        (self.px[i], self.px[i + 1], self.px[i + 2])
    }

    fn count(&self, (x0, y0, x1, y1): (i32, i32, i32, i32), pred: impl Fn((u8, u8, u8)) -> bool) -> usize {
        let mut n = 0;
        for y in y0.max(0)..y1.min(self.h) {
            for x in x0.max(0)..x1.min(self.w) {
                n += usize::from(pred(self.at(x, y)));
            }
        }
        n
    }

    fn colours(&self, (x0, y0, x1, y1): (i32, i32, i32, i32)) -> usize {
        let mut set = std::collections::HashSet::new();
        for y in y0.max(0)..y1.min(self.h) {
            for x in x0.max(0)..x1.min(self.w) {
                set.insert(self.at(x, y));
            }
        }
        set.len()
    }
}

#[allow(clippy::too_many_arguments)]
fn shot(tmp: &Path, lib: &Path, data: &Path, stub: &Option<Stub>, state: &str, appearance: &str, size: &str, launch_ms: Option<u32>) -> Shot {
    let name = format!("{appearance}-{size}-{}-{launch_ms:?}", state.replace([':', ' '], "_"));
    let settings = data.join("settings.json");
    let mut json: serde_json::Value = std::fs::read_to_string(&settings).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_else(|| serde_json::json!({}));
    json["theme"] = serde_json::json!("shelf");
    json["appearance"] = serde_json::json!(appearance);
    json["onboarded"] = serde_json::json!(true);
    std::fs::write(&settings, json.to_string()).unwrap();
    let png = tmp.join(format!("{name}.png"));
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
    if let Some(s) = stub {
        cmd.env("WAYLAND_DISPLAY", &s.1);
    }
    if let Some(ms) = launch_ms {
        cmd.env("POCKETUI_SNAPSHOT_LAUNCH_MS", ms.to_string());
    }
    let out = cmd
        .args(["--snapshot", state, png.to_str().unwrap()])
        .env("POCKETUI_MOCK", "1")
        .env("POCKETUI_LANG", "en")
        .env("POCKETUI_SNAPSHOT_SIZE", size)
        .env("POCKETUI_ES_SYSTEMS", lib.join("es_systems.cfg"))
        .env("POCKETUI_DATA", data)
        .output()
        .expect("run pocket-ui");
    assert!(out.status.success() && png.exists(), "{state} at {size}: {}", String::from_utf8_lossy(&out.stderr));
    let mut reader = png::Decoder::new(std::fs::File::open(&png).unwrap()).read_info().unwrap();
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).unwrap();
    assert_eq!(info.color_type, png::ColorType::Rgb);
    Shot { w: info.width as i32, h: info.height as i32, px: buf }
}

fn copy(from: &Path, to: &Path) -> PathBuf {
    let _ = std::fs::remove_dir_all(to);
    assert!(Command::new("cp").args(["-r", from.to_str().unwrap(), to.to_str().unwrap()]).status().unwrap().success());
    to.to_path_buf()
}

/// The cover's red (the made-up cover above).
fn red((r, g, b): (u8, u8, u8)) -> bool {
    r > 180 && g < 70 && b < 60
}

#[test]
fn the_disc_out_of_its_case_is_printed_with_the_cover_or_the_title() {
    let _tmp_guard = tempfile::tempdir().unwrap();
    let tmp = _tmp_guard.path().to_path_buf();
    let stub = wayland(&tmp);
    let lib = library(&tmp);
    let bare = copy(&lib.join("data"), &tmp.join("bare"));
    let covered = copy(&lib.join("data"), &tmp.join("covered"));
    write_cover(&covered.join("covers/psx").join(format!("{CASTLEVANIA}.png")), 240, 240);
    for (size, appearance) in [("1240x1080", "dark"), ("640x480", "dark"), ("640x480", "light"), ("1280x720", "light")] {
        // The box: the case left of the middle, its disc out at its right.
        let with = shot(&tmp, &lib, &covered, &stub, "box:psx:castlevania", appearance, size, None);
        let right = (with.w * 55 / 100, with.h / 8, with.w, with.h * 7 / 8);
        let n = with.count(right, red);
        assert!(n > (with.w * with.h / 60) as usize, "{size} {appearance}: the disc shows the cover's art ({n} red pixels)");
        // The PlayStation's black disc shows round its print.
        let black = with.count(right, |(r, g, b)| r < 24 && g < 24 && b < 28);
        if appearance == "light" {
            assert!(black > (with.w / 4) as usize, "{size}: the black rim ({black})");
        }
        // No cover: a plain print with the title, not an empty disc.
        let plain = shot(&tmp, &lib, &bare, &stub, "box:psx:castlevania", appearance, size, None);
        assert_eq!(plain.count(right, red), 0, "{size}: no cover, no art");
        let c = plain.colours(right);
        assert!(c > 60, "{size} {appearance}: the plain disc shows its print and title ({c} colours)");
    }
    // Cozy launch, mid-flight: the printed disc over the console.
    let s = shot(&tmp, &lib, &covered, &stub, "shelf:psx:shelf:games:0", "dark", "1280x720", Some(450));
    let n = s.count((s.w / 4, s.h / 3, s.w * 3 / 4, s.h * 5 / 6), red);
    assert!(n > 2000, "the launch carries the printed disc ({n} red pixels)");
}
