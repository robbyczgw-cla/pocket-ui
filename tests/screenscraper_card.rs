//! ScreenScraper's description and facts on the game's card and box (docs/SCREENSCRAPER.md "Facts and text"), as the
//! player sees them.
//!
//! * **Nothing kept for the game: exactly today's card**, ScreenScraper On or Off: the box's back and the Split look's
//!   info page paint the same pixels as without ScreenScraper.
//! * **Kept facts show, On and Off alike** (decided 26.11.2026: Off sends nothing, what came stays, as the spines): the
//!   back and the info page change (the description in the player's language, the facts the card lacked, the source
//!   line), in English and in German, and Off paints the very same pixels as On.
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

/// The made-up game's facts (testdata/screenscraper/jeuInfos-facts.json) kept for Chrono Trigger, as a run keeps them.
fn keep_facts(data: &Path) {
    let body = std::fs::read(format!("{ROOT}/testdata/screenscraper/jeuInfos-facts.json")).unwrap();
    let facts = pocket_ui::screenscraper_facts::parse(&body);
    assert!(!facts.is_empty());
    pocket_ui::screenscraper_facts::save(data, "snes", Path::new("Chrono Trigger (USA).sfc"), &facts);
    assert!(pocket_ui::screenscraper_facts::load(data, "snes", Path::new("Chrono Trigger (USA).sfc")).is_some());
}

/// The picture of `state` in `lang` with ScreenScraper `on` or off, and the game's facts kept or not.
#[allow(clippy::too_many_arguments)]
fn shot(tmp: &Path, lib: &Path, stub: &Option<Stub>, name: &str, state: &str, lang: &str, on: bool, facts: bool) -> Vec<u8> {
    let data = tmp.join(format!("data-{name}"));
    let status = Command::new("cp").args(["-r", lib.join("data").to_str().unwrap(), data.to_str().unwrap()]).status().unwrap();
    assert!(status.success());
    let settings = data.join("settings.json");
    let mut json: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&settings).unwrap()).unwrap();
    json["screenscraper"] = serde_json::json!(on);
    std::fs::write(&settings, json.to_string()).unwrap();
    if facts {
        keep_facts(&data);
    }
    let png = tmp.join(format!("{name}.png"));
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
    if let Some(s) = stub {
        cmd.env("WAYLAND_DISPLAY", &s.1);
    }
    let out = cmd
        .args(["--snapshot", state, png.to_str().unwrap()])
        .env("POCKETUI_MOCK", "1").env("POCKETUI_MOCK_NOW", FIXTURE_NOW)
        .env("POCKETUI_LANG", lang)
        .env("POCKETUI_SNAPSHOT_SIZE", "1240x1080")
        .env("POCKETUI_ES_SYSTEMS", lib.join("es_systems.cfg"))
        .env("POCKETUI_DATA", &data)
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

const STATES: [&str; 3] = ["box:snes:chrono:r", "box:snes:chrono:rddd", "info:split:snes:chrono"];

#[test]
fn without_facts_the_card_and_its_back_are_exactly_todays() {
    let _tmp_guard = tempfile::tempdir().unwrap();
    let tmp = _tmp_guard.path().to_path_buf();
    let stub = wayland(&tmp);
    let lib = library(&tmp);
    for (i, state) in STATES.into_iter().enumerate() {
        let today = shot(&tmp, &lib, &stub, &format!("today-{i}"), state, "en", false, false);
        let on_without = shot(&tmp, &lib, &stub, &format!("onwithout-{i}"), state, "en", true, false);
        assert!(today == on_without, "{state}: On with nothing kept for the game paints today's card, pixel for pixel");
    }
}

#[test]
fn kept_description_and_facts_show_on_and_off_in_english_and_german() {
    let _tmp_guard = tempfile::tempdir().unwrap();
    let tmp = _tmp_guard.path().to_path_buf();
    let stub = wayland(&tmp);
    let lib = library(&tmp);
    for lang in ["en", "de"] {
        for (i, state) in STATES.into_iter().enumerate() {
            let without = shot(&tmp, &lib, &stub, &format!("without-{lang}-{i}"), state, lang, true, false);
            let with = shot(&tmp, &lib, &stub, &format!("with-{lang}-{i}"), state, lang, true, true);
            assert_eq!(without.len(), with.len());
            let changed = without.chunks(3).zip(with.chunks(3)).filter(|(a, b)| a != b).count();
            assert!(changed > 2000, "{lang} {state}: the facts and the text show ({changed} pixels changed)");
            let off = shot(&tmp, &lib, &stub, &format!("off-{lang}-{i}"), state, lang, false, true);
            assert!(off == with, "{lang} {state}: switched Off, what was kept stays, exactly as On");
        }
    }
}
