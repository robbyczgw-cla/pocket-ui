//! The game's real manual from ScreenScraper (docs/SCREENSCRAPER.md "Manuals"), as the player sees it.
//!
//! * **No manual: exactly today's Details page.** ScreenScraper On, a game it knows but lists no manual for, in a run
//!   without network: the very pixels of ScreenScraper Off. Real manuals Off: the same, also for a game that has one.
//! * **A manual: its row, and its pages.** A game ScreenScraper lists a manual for gets the row "Game manual"; opened,
//!   the page fills the screen above the hints, R1 turns to the next one.
//! * **Never an empty screen**: not fetched yet and offline, the page says it needs Wi-Fi.
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

const GAME: &str = "Super Mario World (USA)";

#[derive(Clone, Copy, PartialEq)]
enum Index {
    /// No ScreenScraper index at all (never looked up).
    None,
    /// Known, with pictures but no manual.
    NoManual,
    /// Known, with a manual.
    Manual,
}

/// Two scanned pages: red, then blue, 3:4.
fn pages(dir: &Path) {
    std::fs::create_dir_all(dir).unwrap();
    let mut list = String::new();
    for (i, rgb) in [[220u8, 30, 30], [30, 30, 220]].into_iter().enumerate() {
        let (w, h) = (300u16, 400u16);
        let px: Vec<u8> = (0..w as usize * h as usize).flat_map(|_| rgb).collect();
        let mut jpg = Vec::new();
        jpeg_encoder::Encoder::new(&mut jpg, 85).encode(&px, w, h, jpeg_encoder::ColorType::Rgb).unwrap();
        std::fs::write(dir.join(format!("{:03}.jpg", i + 1)), jpg).unwrap();
        list.push_str(&format!("jpg {w} {h}\n"));
    }
    std::fs::write(dir.join("pages.txt"), list).unwrap();
}

/// The picture of `state` (RGB) at `size`.
#[allow(clippy::too_many_arguments)]
fn shot(tmp: &Path, lib: &Path, stub: &Option<Stub>, name: &str, state: &str, size: &str, screenscraper: bool, manuals: bool, index: Index, kept: bool) -> Vec<u8> {
    let data = tmp.join(format!("data-{name}"));
    let status = Command::new("cp").args(["-r", lib.join("data").to_str().unwrap(), data.to_str().unwrap()]).status().unwrap();
    assert!(status.success());
    let settings = data.join("settings.json");
    let mut json: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&settings).unwrap()).unwrap();
    json["screenscraper"] = serde_json::json!(screenscraper);
    if !manuals {
        json["screenscraper_no_manuals"] = serde_json::json!(true);
    }
    std::fs::write(&settings, json.to_string()).unwrap();
    let medias = match index {
        Index::None => None,
        Index::NoManual => Some("box-2D(us).png,box-2D-side(us).png"),
        Index::Manual => Some("box-2D(us).png,manuel(us).pdf,manuel(de).pdf"),
    };
    if let Some(medias) = medias {
        std::fs::create_dir_all(data.join("screenscraper")).unwrap();
        std::fs::write(data.join("screenscraper/snes.tsv"), format!("{GAME}\t00000000\t1\t2144 us {medias}\t-\t-\n")).unwrap();
    }
    if kept {
        pages(&data.join("manuals/snes").join(GAME).join("us"));
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
        .env("POCKETUI_SNAPSHOT_SIZE", size)
        .env("POCKETUI_ES_SYSTEMS", lib.join("es_systems.cfg"))
        .env("POCKETUI_DATA", &data)
        // The ScreenScraper row as in a build with pocket-ui's developer id (`screenscraper::shown`).
        .env("POCKETUI_SCREENSCRAPER_ROW", "1")
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

/// The Details page (`game_box` off) at 640x480 with Down pressed four times: the rows under the facts in view, Game
/// manual selected where there is one.
const CARD: &str = "curator:card:snes:Super Mario World:4";

#[test]
fn without_a_manual_the_details_page_is_exactly_todays() {
    let _tmp_guard = tempfile::tempdir().unwrap();
    let tmp = _tmp_guard.path().to_path_buf();
    let stub = wayland(&tmp);
    let lib = library(&tmp);
    let today = shot(&tmp, &lib, &stub, "off", CARD, "640x480", false, true, Index::None, false);
    for (name, scraper_on, manuals, index) in [
        ("on-unknown", true, true, Index::None),
        ("on-nomanual", true, true, Index::NoManual),
        ("manuals-off", true, false, Index::Manual),
        ("screenscraper-off", false, true, Index::Manual),
    ] {
        assert!(shot(&tmp, &lib, &stub, name, CARD, "640x480", scraper_on, manuals, index, false) == today, "{name}: today's Details page, pixel for pixel");
    }
}

#[test]
fn a_listed_manual_adds_its_row_and_opens_on_its_pages() {
    let _tmp_guard = tempfile::tempdir().unwrap();
    let tmp = _tmp_guard.path().to_path_buf();
    let stub = wayland(&tmp);
    let lib = library(&tmp);
    let without = shot(&tmp, &lib, &stub, "card-without", CARD, "640x480", true, true, Index::NoManual, false);
    let with = shot(&tmp, &lib, &stub, "card-with", CARD, "640x480", true, true, Index::Manual, false);
    assert!(without != with, "the row Game manual is on the page");
    for (size, w, h) in [("640x480", 640usize, 480usize), ("1024x768", 1024, 768), ("1240x1080", 1240, 1080)] {
        let page1 = shot(&tmp, &lib, &stub, &format!("p1-{size}"), "manual:snes:Super Mario World", size, true, true, Index::Manual, true);
        let page2 = shot(&tmp, &lib, &stub, &format!("p2-{size}"), "manual:snes:Super Mario World:r", size, true, true, Index::Manual, true);
        let red = page1.chunks(3).filter(|p| p[0] > 180 && p[2] < 80).count();
        let blue = page2.chunks(3).filter(|p| p[2] > 180 && p[0] < 80).count();
        // The 3:4 page as high as the room above the hints: well over a third of the screen.
        assert!(red * 3 > w * h, "{size}: page 1 fills the screen's height ({red} px)");
        assert!(blue * 3 > w * h, "{size}: R1 turns to page 2 ({blue} px)");
        assert!(page2.chunks(3).filter(|p| p[0] > 180 && p[2] < 80).count() < 100, "{size}: page 1 has gone");
    }
}

#[test]
fn a_manual_not_on_the_card_says_it_needs_wifi_never_an_empty_screen() {
    let _tmp_guard = tempfile::tempdir().unwrap();
    let tmp = _tmp_guard.path().to_path_buf();
    let stub = wayland(&tmp);
    let lib = library(&tmp);
    let px = shot(&tmp, &lib, &stub, "wifi", "manual:snes:Super Mario World", "640x480", true, true, Index::Manual, false);
    // The words in the middle of the screen: bright pixels between a quarter and three quarters of its height.
    let w = 640 * 3;
    let bright = px[120 * w..360 * w].chunks(3).filter(|p| p[0] > 150 && p[1] > 150 && p[2] > 150).count();
    assert!(bright > 300, "the title and the Wi-Fi line are on the screen ({bright} px)");
}
