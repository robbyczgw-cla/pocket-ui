//! The start scene on a short screen and the case's way home, as a player sees them (ritual review 14.11.2026): on a
//! 1024x768 panel (the Brick, the Smart Pro) the console is as small beside the case as it is on the other screens, so it
//! does not stand over the title and the case; and in the Shelf look the case that went into the console does not stay up
//! in the air over the shelf as a bare spine, it goes back to its place in the row. Runs the real start (60 Hz series
//! of `--snapshot shelf:<system>:shelf:games:<row>`) on the made-up library (scripts/curator-fake-library.py). Needs
//! python3 (and, on Linux without a compositor, scripts/wayland-stub.py), as tests/sealed_games.rs.

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
    let path = tmp.join("wl");
    let child = Command::new("python3").args(["-I", &format!("{ROOT}/scripts/wayland-stub.py"), path.to_str().unwrap()]).spawn().expect("python3");
    let stub = Stub(child, path);
    test_clock::wait_for(|| stub.1.exists(), std::time::Duration::from_secs(2), "Wayland stub listening");
    Some(stub)
}

fn library(tmp: &Path) -> PathBuf {
    let lib = tmp.join("lib");
    let ok = Command::new("python3").env("POCKETUI_MOCK_NOW", FIXTURE_NOW)
        .args(["-I", &format!("{ROOT}/scripts/curator-fake-library.py"), lib.to_str().unwrap(), "dark"])
        .stdout(std::process::Stdio::null())
        .status()
        .expect("python3 for scripts/curator-fake-library.py");
    assert!(ok.success());
    lib
}

struct Pic {
    w: i32,
    h: i32,
    px: Vec<u8>,
}

impl Pic {
    fn read(png: &Path) -> Pic {
        let mut reader = png::Decoder::new(std::fs::File::open(png).unwrap()).read_info().unwrap();
        let mut buf = vec![0; reader.output_buffer_size()];
        let info = reader.next_frame(&mut buf).unwrap();
        assert_eq!(info.color_type, png::ColorType::Rgb);
        Pic { w: info.width as i32, h: info.height as i32, px: buf }
    }

    fn at(&self, x: i32, y: i32) -> [u8; 3] {
        let i = ((y * self.w + x) * 3) as usize;
        [self.px[i], self.px[i + 1], self.px[i + 2]]
    }

    /// The width, as a share of the picture's, of the light grey and white a console is made of (no colour in it: the
    /// dimmed page, the covers and labels are darker or coloured); columns with only a few such pixels are noise.
    fn console_share(&self) -> f32 {
        let light = |p: [u8; 3]| p.iter().all(|v| *v >= 150) && p.iter().max().unwrap() - p.iter().min().unwrap() < 40;
        let cols: Vec<i32> = (0..self.w).filter(|x| (0..self.h).filter(|y| light(self.at(*x, *y))).count() >= 6).collect();
        match (cols.first(), cols.last()) {
            (Some(a), Some(b)) => (b - a + 1) as f32 / self.w as f32,
            _ => 0.0,
        }
    }

    /// How many pixels in the part `x0..x1`, `y0..y1` (shares of the picture) are anything but the page's black.
    fn lit_in(&self, (x0, x1): (f32, f32), (y0, y1): (f32, f32)) -> usize {
        let (xs, ys) = ((x0 * self.w as f32) as i32..(x1 * self.w as f32) as i32, (y0 * self.h as f32) as i32..(y1 * self.h as f32) as i32);
        ys.flat_map(|y| xs.clone().map(move |x| (x, y))).filter(|(x, y)| self.at(*x, *y).iter().any(|v| *v > 25)).count()
    }
}

/// The frame `from_end` frames before the end of the start of the game in row `row` of `system`'s shelf (Shelf look,
/// Cozy), at `size`: the medium is in, the case at home, the fade not yet begun.
fn start_frame(system: &str, row: u32, size: &str, from_end: u32) -> Pic {
    let tmp = tempfile::tempdir().unwrap();
    let stub = wayland(tmp.path());
    let lib = library(tmp.path());
    let data = lib.join("data");
    let settings = data.join("settings.json");
    let mut json: serde_json::Value = std::fs::read_to_string(&settings).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_else(|| serde_json::json!({}));
    json["theme"] = serde_json::json!("shelf");
    json["onboarded"] = serde_json::json!(true);
    json["motion"] = serde_json::json!("full");
    json["animations"] = serde_json::json!("cozy");
    std::fs::write(&settings, json.to_string()).unwrap();
    let series = tmp.path().join("series");
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
    if let Some(s) = &stub {
        cmd.env("WAYLAND_DISPLAY", &s.1);
    }
    let state = format!("shelf:{system}:shelf:games:{row}");
    let out = cmd
        .args(["--snapshot", &state, tmp.path().join("x.png").to_str().unwrap()])
        .env("POCKETUI_MOCK", "1").env("POCKETUI_MOCK_NOW", FIXTURE_NOW)
        .env("POCKETUI_LANG", "en")
        .env("POCKETUI_ROOM_TIME", "12:00")
        .env("POCKETUI_SNAPSHOT_SIZE", size)
        .env("POCKETUI_ES_SYSTEMS", lib.join("es_systems.cfg"))
        .env("POCKETUI_DATA", &data)
        .env("POCKETUI_SEALED", "0")
        .env("POCKETUI_SNAPSHOT_LAUNCH_SERIES", &series)
        .output()
        .expect("run pocket-ui");
    assert!(out.status.success(), "{state}: {}", String::from_utf8_lossy(&out.stderr));
    let out = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(out.contains("launch-series paint frames="), "{out}");
    let frames: u32 = out.split("frames=").nth(1).and_then(|r| r.split_whitespace().next()).and_then(|n| n.parse().ok()).expect("the series' frame count");
    let frame = frames - from_end;
    let pic = Pic::read(&series.join(format!("{frame:04}.png")));
    if let Some(dir) = std::env::var_os("RITUAL_SHOTS") {
        let dir = PathBuf::from(dir);
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::copy(series.join(format!("{frame:04}.png")), dir.join(format!("{system}-{size}-f{frame}.png")));
    }
    pic
}

/// The game is in, the shelf behind it dimmed (frame 88 of 96).
const SEATED: u32 = 8;

#[test]
fn on_a_short_screen_the_console_is_as_small_beside_the_case_as_everywhere_else() {
    // At 560 pixels it was 55 % of a 1024x768 screen's width (47 % at 1240x1080) and stood over the title and the case.
    for (system, row) in [("snes", 1), ("psx", 0)] {
        let short = start_frame(system, row, "1024x768", SEATED).console_share();
        assert!((0.25..=0.46).contains(&short), "{system}: the console is {short:.2} of a 1024x768 screen's width");
    }
}

#[test]
fn the_case_that_went_into_the_console_goes_back_to_its_place_in_the_row_not_up_into_the_air() {
    // Over the shelf's first spines, under the title and the case's old place, there was a bare grey spine standing in the air.
    for size in ["1024x768", "1240x1080"] {
        let pic = start_frame("snes", 1, size, SEATED);
        let lit = pic.lit_in((0.10, 0.30), (0.15, 0.40));
        assert!(lit < 40, "{size}: {lit} pixels of something hang over the shelf where the case's spine was left");
    }
}
