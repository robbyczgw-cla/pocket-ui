//! Case stickers and the Cabinet's TV as the player meets them (docs/DESIGN.md "Case stickers", "The TV on the
//! cabinet"), on the made-up library (scripts/curator-fake-library.py):
//!
//! - a journey: the game card's Stickers sheet (opened from the page in Split), A on Completed and on Game of the year (Right first: another year),
//!   lands in `library.json`, and the game's box and the Cabinet's case show them; B takes nothing off;
//! - the TV on the Cabinet shows the game's screenshot (a made-up red one), the console's picture for a game with no
//!   picture at all, fits 640x480, 1280x720 and the Mini V2, in dark and light; the Shelf look has none.
//!
//! Needs python3 (and, on Linux without a compositor, scripts/wayland-stub.py), as tests/spine_titles.rs.

use std::path::{Path, PathBuf};
use std::process::Command;

const ROOT: &str = env!("CARGO_MANIFEST_DIR");
/// The first SNES game on its shelf (`games:0`).
const CHRONO: &str = "Chrono Trigger (USA)";

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

/// A flat PNG of one colour, `w` x `h`.
fn write_png(path: &Path, w: u32, h: u32, rgb: [u8; 3]) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut e = png::Encoder::new(std::fs::File::create(path).unwrap(), w, h);
    e.set_color(png::ColorType::Rgb);
    e.set_depth(png::BitDepth::Eight);
    let data: Vec<u8> = (0..w * h).flat_map(|_| rgb).collect();
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

    /// Pixels in the rectangle that `pred` holds for.
    fn count(&self, (x0, y0, x1, y1): (i32, i32, i32, i32), pred: impl Fn((u8, u8, u8)) -> bool) -> usize {
        let mut n = 0;
        for y in y0.max(0)..y1.min(self.h) {
            for x in x0.max(0)..x1.min(self.w) {
                n += usize::from(pred(self.at(x, y)));
            }
        }
        n
    }
}

/// Renders `state` with a copy of `data` (or `data` itself with `keep`, so what it saves stays) and returns the picture.
#[allow(clippy::too_many_arguments)]
fn shot(tmp: &Path, lib: &Path, data: &Path, stub: &Option<Stub>, state: &str, look: &str, appearance: &str, size: &str, lang: &str) -> Shot {
    let name = format!("{look}-{appearance}-{size}-{lang}-{}", state.replace([':', ' '], "_"));
    let settings = data.join("settings.json");
    let mut json: serde_json::Value = std::fs::read_to_string(&settings).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_else(|| serde_json::json!({}));
    json["theme"] = serde_json::json!(look);
    json["appearance"] = serde_json::json!(appearance);
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
        .env("POCKETUI_LANG", lang)
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

fn case_of(data: &Path, file: &str) -> serde_json::Value {
    let lib: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(data.join("library.json")).unwrap()).unwrap();
    let games = lib["games"].as_object().unwrap();
    games.iter().find(|(k, _)| k.ends_with(file)).map(|(_, g)| g["case"].clone()).unwrap_or(serde_json::Value::Null)
}

#[test]
fn stickers_go_on_from_the_card_and_show_on_the_box_and_the_case() {
    let _tmp_guard = tempfile::tempdir().unwrap();
    let tmp = _tmp_guard.path().to_path_buf();
    let stub = wayland(&tmp);
    let lib = library(&tmp);
    // A cover, so the stickers sit on box art as they will for most games.
    write_png(&lib.join("data/covers/snes").join(format!("{CHRONO}.png")), 256, 180, [40, 70, 160]);
    let bare = copy(&lib.join("data"), &tmp.join("bare"));
    let data = copy(&lib.join("data"), &tmp.join("stuck"));
    // B alone puts nothing on.
    shot(&tmp, &lib, &data, &stub, "curator:stickers:snes:chrono:b", "split", "dark", "1240x1080", "en");
    assert!(case_of(&data, &format!("{CHRONO}.sfc")).is_null(), "B took nothing off and put nothing on");
    // A on Completed; down to Game of the year, a year later, A.
    shot(&tmp, &lib, &data, &stub, "curator:stickers:snes:chrono:addddra", "split", "dark", "1240x1080", "en");
    let case = case_of(&data, &format!("{CHRONO}.sfc"));
    assert_eq!(case["completed"], serde_json::json!(true), "{case}");
    let year = case["goty"].as_u64().expect("Game of the year is on");
    assert!((1971..=2100).contains(&year), "{year}");
    // The box front and the Cabinet's case out of the shelf show them: compared with the same game without, in
    // English and German, dark and light, on the Mini V2 and on 640x480.
    for (size, lang, appearance) in [("1240x1080", "en", "dark"), ("1240x1080", "de", "light"), ("640x480", "en", "dark")] {
        for (look, state) in [("shelf", "box:snes:chrono"), ("cabinet", "shelf:snes:cabinet:games:0"), ("shelf", "shelf:snes:shelf:games:0")] {
            let a = shot(&tmp, &lib, &bare, &stub, state, look, appearance, size, lang);
            let b = shot(&tmp, &lib, &data, &stub, state, look, appearance, size, lang);
            assert_eq!((a.w, a.h), (b.w, b.h));
            let changed = a.px.chunks(3).zip(b.px.chunks(3)).filter(|(p, q)| p != q).count();
            assert!(changed > 400, "{state} ({look}, {size}, {lang}, {appearance}): the stickers changed only {changed} pixels");
        }
    }
}

#[test]
fn the_cabinet_tv_shows_the_game_and_never_stands_empty() {
    let _tmp_guard = tempfile::tempdir().unwrap();
    let tmp = _tmp_guard.path().to_path_buf();
    let stub = wayland(&tmp);
    let lib = library(&tmp);
    let with = copy(&lib.join("data"), &tmp.join("with-snaps"));
    // A red screenshot for every SNES game: whichever is selected, the TV shows red.
    for e in std::fs::read_dir(lib.join("roms/snes")).unwrap().flatten() {
        let stem = e.path().file_stem().unwrap().to_string_lossy().into_owned();
        write_png(&with.join("snaps/snes").join(format!("{stem}.png")), 160, 120, [220, 30, 30]);
    }
    let red = |(r, g, b): (u8, u8, u8)| r > 140 && g < 90 && b < 90;
    for (size, appearance) in [("1240x1080", "dark"), ("1280x720", "light"), ("640x480", "dark")] {
        let s = shot(&tmp, &lib, &with, &stub, "shelf:snes:cabinet:games:0", "cabinet", appearance, size, "en");
        // The TV stands at the right, above the shelves (the snapshot is the layout's canvas).
        let tv = (s.w * 3 / 4, s.h / 6, s.w, s.h / 2);
        let n = s.count(tv, red);
        assert!(n > 1500, "{size} {appearance}: the TV shows {n} red pixels");
        // It stands on the board: its picture is in the showcase, not down among the shelves.
        let low = s.count((s.w * 3 / 4, s.h * 2 / 3, s.w, s.h), red);
        assert!(low < n / 20, "{size}: {low} red pixels down among the shelves");
        // The Shelf look has no TV.
        let shelf = shot(&tmp, &lib, &with, &stub, "shelf:snes:shelf:games:0", "shelf", appearance, size, "en");
        assert_eq!(shelf.count(tv, red), 0, "{size}: a TV in the Shelf look");
    }
    // No screenshot, no picture of the player's, no cover: the TV still stands there (its glass shows the console,
    // `cabinet_tv::console_screen`, checked in its unit tests), its warm beige body in view.
    let none = copy(&lib.join("data"), &tmp.join("none"));
    let s = shot(&tmp, &lib, &none, &stub, "shelf:snes:cabinet:games:0", "cabinet", "dark", "1240x1080", "en");
    let beige = s.count((s.w * 3 / 4, s.h / 6, s.w, s.h / 2), |(r, g, b)| r > 200 && g > 150 && b > 100 && r > b + 40);
    assert!(beige > 2000, "the TV without pictures: {beige} pixels of its body");
}
