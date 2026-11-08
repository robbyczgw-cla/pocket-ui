//! A second card that holds a flashed system image (`card_health::image_card`): Home's "No games yet" and Add games say so,
//! in every language, inside the 640x480 screen.

use std::path::{Path, PathBuf};
use std::process::Command;

include!("common/root.rs");

struct Stub(std::process::Child);
impl Drop for Stub {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn fixture(tmp: &Path) -> (Option<Stub>, Option<PathBuf>) {
    assert!(Command::new("python3").env("POCKETUI_MOCK_NOW", FIXTURE_NOW)
        .args(["-I", &format!("{ROOT}/scripts/curator-fake-library.py"), tmp.to_str().unwrap(), "dark"])
        .stdout(std::process::Stdio::null()).status().unwrap().success());
    std::fs::write(tmp.join("data/sounds.json"), r#"{"on":false,"volume":0}"#).unwrap();
    if !cfg!(target_os = "linux") || std::env::var_os("WAYLAND_DISPLAY").is_some_and(|w| !w.is_empty()) {
        return (None, None);
    }
    let path = tmp.join("wayland-stub");
    let child = Command::new("python3").args(["-I", &format!("{ROOT}/scripts/wayland-stub.py"), path.to_str().unwrap()]).spawn().unwrap();
    let stub = Stub(child);
    let until = std::time::Instant::now() + test_clock::timeout(std::time::Duration::from_secs(8));
    while std::time::Instant::now() < until {
        if path.exists() { return (Some(stub), Some(path)); }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    panic!("Wayland stub did not start");
}

fn shot(tmp: &Path, display: Option<&Path>, state: &str, lang: &str) -> String {
    let (png, visible, boxes) = (tmp.join("frame.png"), tmp.join("visible.txt"), tmp.join("boxes.txt"));
    let _ = std::fs::remove_file(&visible);
    let _ = std::fs::remove_file(&boxes);
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
    cmd.args(["--snapshot", state, png.to_str().unwrap()])
        .env("POCKETUI_MOCK", "1").env("POCKETUI_MOCK_NOW", FIXTURE_NOW).env("POCKETUI_LANG", lang).env("POCKETUI_SNAPSHOT_IMAGE_CARD", "1")
        .env("POCKETUI_SNAPSHOT_WIFI", "0").env("POCKETUI_SNAPSHOT_SIZE", "640x480").env("POCKETUI_MOTION_SCALE", "1")
        .env("POCKETUI_SOUND_QUANTUM", "0")
        .env("POCKETUI_DATA", tmp.join("data")).env("POCKETUI_ES_SYSTEMS", tmp.join("es_systems.cfg"))
        .env("POCKETUI_VISIBLE_TEXT", &visible).env("POCKETUI_TEXT_BOXES", &boxes);
    if let Some(display) = display { cmd.env("WAYLAND_DISPLAY", display); }
    let out = cmd.output().unwrap();
    assert!(out.status.success(), "{state}: {}", String::from_utf8_lossy(&out.stderr));
    let problems = std::fs::read_to_string(boxes).unwrap_or_default();
    assert!(problems.trim().is_empty(), "{state}, {lang}: {problems}");
    std::fs::read_to_string(visible).unwrap().split_whitespace().collect::<Vec<_>>().join(" ")
}

#[test]
fn a_system_image_in_the_second_slot_is_said_plainly_and_fits() {
    let tmp = tempfile::tempdir().unwrap();
    let (_stub, display) = fixture(tmp.path());
    for (lang, words) in [("en", "holds a system image"), ("de", "System-Image"), ("es", "imagen del sistema")] {
        for state in ["starter-home", "setup:games"] {
            let seen = shot(tmp.path(), display.as_deref(), state, lang);
            assert!(seen.contains(words), "{state}/{lang} says the card holds an image: {seen}");
        }
    }
}
