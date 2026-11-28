//! The rows under ScreenScraper (Settings → Games → Box art & downloads: Real spines, Real labels, Real box backs), as
//! the player sees them (owner 26.11.2026): each row Off paints exactly what pocket-ui draws without ScreenScraper, even
//! with the picture already on the card; On shows it. ScreenScraper itself Off only sends nothing: what it brought stays. Real box backs is Off unless switched on: pocket-ui's own back, with the
//! player's progress, stays the usual.
//!
//! Needs python3 (and, on Linux without a compositor, scripts/wayland-stub.py), as tests/screenscraper_shelf.rs.

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

struct World {
    tmp: PathBuf,
    lib: PathBuf,
    stub: Option<Stub>,
    _dir: tempfile::TempDir,
}

/// A picture as ScreenScraper sends it, `w` x `h`, all one colour.
fn png_file(path: &Path, w: u32, h: u32, rgb: [u8; 3]) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let file = std::fs::File::create(path).unwrap();
    let mut enc = png::Encoder::new(file, w, h);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    let px: Vec<u8> = (0..w * h).flat_map(|_| rgb).collect();
    enc.write_header().unwrap().write_image_data(&px).unwrap();
}

const MAGENTA: [u8; 3] = [240, 16, 224];

/// Pixels that are (a little muted) magenta.
fn magenta(px: &[u8]) -> usize {
    px.chunks_exact(3).filter(|p| p[0] > 170 && p[1] < 80 && p[2] > 150).count()
}

impl World {
    fn new(name: &str) -> World {
        let dir = tempfile::Builder::new().prefix(&format!("pui-ss-rows-{name}-")).tempdir().unwrap();
        let tmp = dir.path().to_path_buf();
        let lib = tmp.join("lib");
        let ok = Command::new("python3").env("POCKETUI_MOCK_NOW", FIXTURE_NOW)
            .args(["-I", &format!("{ROOT}/scripts/curator-fake-library.py"), lib.to_str().unwrap(), "dark"])
            .stdout(std::process::Stdio::null())
            .status()
            .expect("python3");
        assert!(ok.success());
        let stub = wayland(&tmp);
        World { tmp, lib, stub, _dir: dir }
    }

    /// The first SNES game's file stem (the one a `shelf:snes:…:games:0` state shows).
    fn first_snes(&self) -> String {
        let first = std::fs::read_dir(self.lib.join("roms/snes")).unwrap().map(|e| e.unwrap().path()).min().unwrap();
        first.file_stem().unwrap().to_string_lossy().into_owned()
    }

    /// `state` painted with `settings` merged into the library's, `files` (face folder, size) of the first SNES game on the
    /// card, `env` extra.
    fn shot(&self, name: &str, state: &str, settings: serde_json::Value, files: &[(&str, (u32, u32))], env: &[(&str, &str)]) -> Vec<u8> {
        let data = self.tmp.join(format!("data-{name}"));
        let _ = std::fs::remove_dir_all(&data);
        assert!(Command::new("cp").args(["-r", self.lib.join("data").to_str().unwrap(), data.to_str().unwrap()]).status().unwrap().success());
        let file = data.join("settings.json");
        let mut json: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
        for (k, v) in settings.as_object().unwrap() {
            json[k] = v.clone();
        }
        std::fs::write(&file, json.to_string()).unwrap();
        let stem = self.first_snes();
        for (face, (w, h)) in files {
            // A spine on the shelf: a game that stands as a spine, not the selected one (out as its case in the Cabinet).
            let stem = if *face == "boxsides" { "Donkey Kong Country (USA) (Rev 2)".to_string() } else { stem.clone() };
            png_file(&data.join(face).join("snes").join(format!("{stem}.png")), *w, *h, MAGENTA);
        }
        let png = self.tmp.join(format!("{name}.png"));
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
        if let Some(s) = &self.stub {
            cmd.env("WAYLAND_DISPLAY", &s.1);
        }
        for (k, v) in env {
            cmd.env(k, v);
        }
        let out = cmd
            .args(["--snapshot", state, png.to_str().unwrap()])
            .env("POCKETUI_MOCK", "1").env("POCKETUI_MOCK_NOW", FIXTURE_NOW)
            .env("POCKETUI_LANG", "en")
            .env("POCKETUI_SNAPSHOT_SIZE", "1024x768")
            .env("POCKETUI_ES_SYSTEMS", self.lib.join("es_systems.cfg"))
            .env("POCKETUI_DATA", &data)
            .output()
            .expect("run pocket-ui");
        assert!(out.status.success() && png.exists(), "{state}: {}", String::from_utf8_lossy(&out.stderr));
        let mut reader = png::Decoder::new(std::fs::File::open(&png).unwrap()).read_info().unwrap();
        let mut buf = vec![0; reader.output_buffer_size()];
        let info = reader.next_frame(&mut buf).unwrap();
        buf.truncate(info.buffer_size());
        buf
    }
}

fn cozy(extra: serde_json::Value) -> serde_json::Value {
    let mut v = serde_json::json!({"onboarded": true, "theme": "shelf", "animations": "cozy", "extras": {"playful_animations": true}, "screenscraper": true});
    for (k, x) in extra.as_object().unwrap() {
        v[k] = x.clone();
    }
    v
}

#[test]
fn real_labels_off_starts_the_game_exactly_as_without_screenscraper() {
    let w = World::new("labels");
    let state = "shelf:snes:shelf:games:0";
    let at = [("POCKETUI_SNAPSHOT_LAUNCH_MS", "1000")];
    let label = [("supporttextures", (300, 120))];
    let none = w.shot("none", state, cozy(serde_json::json!({})), &[], &at);
    let off = w.shot("off", state, cozy(serde_json::json!({"screenscraper_no_labels": true})), &label, &at);
    let switch_off = w.shot("switched-off", state, cozy(serde_json::json!({"screenscraper": false})), &label, &at);
    let on = w.shot("on", state, cozy(serde_json::json!({})), &label, &at);
    assert!(off == none, "Real labels Off: the cartridge goes in with today's label, pixel for pixel");
    assert!(magenta(&switch_off) > 300, "ScreenScraper Off only sends nothing: the label already on the card stays");
    assert!(magenta(&on) > 300, "Real labels On (the usual): the real label goes in ({} px)", magenta(&on));
}

#[test]
fn real_box_backs_are_off_unless_switched_on_and_off_is_todays_back() {
    let w = World::new("backs");
    let name = w.first_snes().split_whitespace().next().unwrap().to_lowercase();
    let state = format!("box:snes:{name}:r");
    let at = [("POCKETUI_SNAPSHOT_FLIP_MS", "2000")];
    let back = [("boxbacks", (300, 400))];
    let none = w.shot("none", &state, serde_json::json!({"screenscraper": true}), &[], &at);
    let usual = w.shot("usual", &state, serde_json::json!({"screenscraper": true}), &back, &at);
    let on = w.shot("on", &state, serde_json::json!({"screenscraper": true, "screenscraper_backs": true}), &back, &at);
    assert!(usual == none, "Real box backs Off (the usual): pocket-ui's own back, pixel for pixel");
    assert_eq!(magenta(&none), 0);
    assert!(magenta(&on) > 20_000, "Real box backs On: the real back shows when the box is turned ({} px)", magenta(&on));
}

#[test]
fn real_spines_off_is_todays_shelf() {
    let w = World::new("spines");
    let spine = [("boxsides", (680, 97))];
    for (i, state) in ["shelf:snes:shelf:games:0", "shelf:snes:cabinet:games:0"].into_iter().enumerate() {
        let none = w.shot(&format!("none-{i}"), state, serde_json::json!({"screenscraper": true}), &[], &[]);
        let off = w.shot(&format!("off-{i}"), state, serde_json::json!({"screenscraper": true, "screenscraper_no_spines": true}), &spine, &[]);
        let on = w.shot(&format!("on-{i}"), state, serde_json::json!({"screenscraper": true}), &spine, &[]);
        assert!(off == none, "{state}: Real spines Off: the painted spines, pixel for pixel");
        assert!(none != on, "{state}: Real spines On (the usual): the real spine shows");
    }
}
