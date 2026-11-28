//! ScreenScraper on the shelf (docs/SCREENSCRAPER.md "The spines on the shelf"), as the player sees it.
//!
//! * **Off, offline, or nothing downloaded yet: exactly today's shelf** (the owner, 26.11.2026: no message, no note, no
//!   wait, nothing new on the shelf). The Shelf and the Cabinet look with ScreenScraper switched On, in a run without
//!   any network (`POCKETUI_MOCK`), paint the very same pixels as with it Off: painted spines, no line by the clock.
//! * **A real spine, where there is one; the painted one beside it, never a gap.** With one game's spine photo in
//!   `boxsides/`, that game's spine changes and the picture is otherwise the same shelf. Switched Off it stays (the owner:
//!   Off means no network; what is downloaded stays visible).
//!
//! Needs python3 (and, on Linux without a compositor, scripts/wayland-stub.py), as tests/spine_titles.rs.

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

/// A spine photo as ScreenScraper sends the SNES's: lying, 680x97, here in bright stripes.
fn spine_png(path: &Path) {
    let (w, h) = (680u32, 97u32);
    let mut px = Vec::with_capacity((w * h * 3) as usize);
    for _y in 0..h {
        for x in 0..w {
            px.extend_from_slice(if (x / 40) % 2 == 0 { &[230, 40, 40] } else { &[40, 40, 230] });
        }
    }
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let file = std::fs::File::create(path).unwrap();
    let mut enc = png::Encoder::new(file, w, h);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().unwrap().write_image_data(&px).unwrap();
}

/// The picture of `state` with the library's data folder and `settings` merged into its settings.json; `photo`: one
/// SNES spine photo in `boxsides/`.
fn shot(tmp: &Path, lib: &Path, stub: &Option<Stub>, name: &str, state: &str, screenscraper: bool, photo: bool) -> Vec<u8> {
    shot_noted(tmp, lib, stub, name, state, screenscraper, photo, "")
}

/// `shot` with the shelf's line of a run (`POCKETUI_SCREENSCRAPER_NOTE`: `spines:34:120`, `tomorrow`; empty: none).
#[allow(clippy::too_many_arguments)]
fn shot_noted(tmp: &Path, lib: &Path, stub: &Option<Stub>, name: &str, state: &str, screenscraper: bool, photo: bool, note: &str) -> Vec<u8> {
    let data = tmp.join(format!("data-{name}"));
    let status = Command::new("cp").args(["-r", lib.join("data").to_str().unwrap(), data.to_str().unwrap()]).status().unwrap();
    assert!(status.success());
    let settings = data.join("settings.json");
    let mut json: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&settings).unwrap()).unwrap();
    json["screenscraper"] = serde_json::json!(screenscraper);
    std::fs::write(&settings, json.to_string()).unwrap();
    if photo {
        spine_png(&data.join("boxsides/snes/Donkey Kong Country (USA) (Rev 2).png"));
    }
    let png = tmp.join(format!("{name}.png"));
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
    if let Some(s) = stub {
        cmd.env("WAYLAND_DISPLAY", &s.1);
    }
    let out = cmd
        .args(["--snapshot", state, png.to_str().unwrap()])
        .env("POCKETUI_MOCK", "1").env("POCKETUI_MOCK_NOW", FIXTURE_NOW)
        .env("POCKETUI_LANG", "en")
        .env("POCKETUI_SNAPSHOT_SIZE", "640x480")
        .env("POCKETUI_ES_SYSTEMS", lib.join("es_systems.cfg"))
        .env("POCKETUI_DATA", &data)
        .env("POCKETUI_SCREENSCRAPER_NOTE", note)
        .output()
        .expect("run pocket-ui");
    assert!(out.status.success() && png.exists(), "{state}: {}", String::from_utf8_lossy(&out.stderr));
    let decoder = png::Decoder::new(std::fs::File::open(&png).unwrap());
    let mut reader = decoder.read_info().unwrap();
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).unwrap();
    buf.truncate(info.buffer_size());
    buf
}

#[test]
fn off_offline_or_with_nothing_downloaded_the_shelf_is_exactly_todays() {
    let _tmp_guard = tempfile::tempdir().unwrap();
    let tmp = _tmp_guard.path().to_path_buf();
    let stub = wayland(&tmp);
    let lib = library(&tmp);
    for (i, state) in ["shelf:snes:shelf:games:0", "shelf:snes:cabinet:games:0", "shelf:gba:shelf:games:2"].into_iter().enumerate() {
        let off = shot(&tmp, &lib, &stub, &format!("off-{i}"), state, false, false);
        let on = shot(&tmp, &lib, &stub, &format!("on-{i}"), state, true, false);
        assert!(off == on, "{state}: ScreenScraper On without network or pictures must paint today's shelf, pixel for pixel");
    }
}

#[test]
fn a_real_spine_shows_where_there_is_one_and_the_painted_ones_stay_beside_it() {
    let _tmp_guard = tempfile::tempdir().unwrap();
    let tmp = _tmp_guard.path().to_path_buf();
    let stub = wayland(&tmp);
    let lib = library(&tmp);
    for (i, state) in ["shelf:snes:shelf:games:0", "shelf:snes:cabinet:games:0"].into_iter().enumerate() {
        let painted = shot(&tmp, &lib, &stub, &format!("painted-{i}"), state, true, false);
        let real = shot(&tmp, &lib, &stub, &format!("real-{i}"), state, true, true);
        // Product decision (the owner, 26.11.2026): Off sends nothing, and spines already on the card stay on the shelf.
        let off = shot(&tmp, &lib, &stub, &format!("real-off-{i}"), state, false, true);
        assert!(off == real, "{state}: switched Off, the downloaded spine stays");
        assert_eq!(painted.len(), real.len());
        let changed = painted.chunks(3).zip(real.chunks(3)).filter(|(a, b)| a != b).count();
        let total = painted.len() / 3;
        // One spine of a dozen: some of the picture, far from all of it.
        assert!(changed > 200, "{state}: the game with a spine photo shows it ({changed} pixels changed)");
        assert!(changed * 10 < total, "{state}: only that spine changes ({changed} of {total} pixels)");
        // The photo's stripes are on the shelf (in the room's light): its red and its blue both show.
        let changed_px: Vec<&[u8]> = painted.chunks(3).zip(real.chunks(3)).filter(|(a, b)| a != b).map(|(_, b)| b).collect();
        assert!(changed_px.iter().any(|p| p[0] as i32 > p[2] as i32 + 80), "{state}: the real spine's red");
        assert!(changed_px.iter().any(|p| p[2] as i32 > p[0] as i32 + 80), "{state}: the real spine's blue");
    }
}

/// The owner, 26.11.2026: while spines come in, the shelf says so in a small line by the clock; when the day's share is used,
/// that the rest comes tomorrow. Visible words where the quiet dot is otherwise, in the corner only; switched off, no line.
#[test]
fn the_shelf_tells_spines_coming_in_and_more_tomorrow_only_while_it_is_on() {
    let _tmp_guard = tempfile::tempdir().unwrap();
    let tmp = _tmp_guard.path().to_path_buf();
    let stub = wayland(&tmp);
    let lib = library(&tmp);
    let state = "shelf:snes:shelf:games:0";
    let plain = shot(&tmp, &lib, &stub, "plain", state, true, false);
    let width = 640 * 3;
    for note in ["spines:34:120", "tomorrow"] {
        let noted = shot_noted(&tmp, &lib, &stub, &format!("noted-{}", &note[..5]), state, true, false, note);
        let rows: Vec<usize> = (0..480).filter(|y| plain[y * width..(y + 1) * width] != noted[y * width..(y + 1) * width]).collect();
        assert!(!rows.is_empty(), "{note}: a line shows on the shelf");
        assert!(rows.iter().all(|&y| y < 110), "{note}: only in the top corner, by the clock (rows {rows:?})");
        let off = shot_noted(&tmp, &lib, &stub, &format!("off-{}", &note[..5]), state, false, false, note);
        assert!(off == shot(&tmp, &lib, &stub, &format!("offplain-{}", &note[..5]), state, false, false), "{note}: Off shows no line at all");
    }
}
