//! Empty-file explanations through the real application renderer, with actual throwaway zero-byte ROMs.
//! The transcript records visible glyphs only; layout diagnostics also check the German/Large H700 canvas.

use std::path::{Path, PathBuf};
use std::process::Command;

const ROOT: &str = env!("CARGO_MANIFEST_DIR");

struct Stub(std::process::Child);
impl Drop for Stub {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn fixture(tmp: &Path) -> (Option<Stub>, Option<PathBuf>) {
    assert!(Command::new("python3")
        .args(["-I", &format!("{ROOT}/scripts/curator-fake-library.py"), tmp.to_str().unwrap(), "dark"])
        .stdout(std::process::Stdio::null()).status().unwrap().success());
    std::fs::write(tmp.join("data/sounds.json"), r#"{"on":false,"volume":0}"#).unwrap();
    if !cfg!(target_os = "linux") || std::env::var_os("WAYLAND_DISPLAY").is_some_and(|w| !w.is_empty()) {
        return (None, None);
    }
    let path = tmp.join("wayland-stub");
    let child = Command::new("python3").args(["-I", &format!("{ROOT}/scripts/wayland-stub.py"), path.to_str().unwrap()]).spawn().unwrap();
    let stub = Stub(child);
    let until = std::time::Instant::now() + std::time::Duration::from_secs(8);
    while std::time::Instant::now() < until {
        if path.exists() { return (Some(stub), Some(path)); }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    panic!("Wayland stub did not start");
}

fn rom(tmp: &Path, system: &str, name: &str) -> PathBuf {
    std::fs::read_dir(tmp.join("roms").join(system)).unwrap().flatten().map(|e| e.path())
        .find(|p| p.file_name().unwrap().to_string_lossy().to_lowercase().contains(name)).unwrap()
}

fn shot(tmp: &Path, display: Option<&Path>, state: &str, lang: &str, text_size: &str) -> String {
    let (png, visible, boxes) = (tmp.join("frame.png"), tmp.join("visible.txt"), tmp.join("boxes.txt"));
    let _ = std::fs::remove_file(&visible);
    let _ = std::fs::remove_file(&boxes);
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
    cmd.args(["--snapshot", state, png.to_str().unwrap()])
        .env("POCKETUI_MOCK", "1").env("POCKETUI_LANG", lang).env("POCKETUI_TEXT_SIZE", text_size)
        .env("POCKETUI_SNAPSHOT_SIZE", "640x480").env("POCKETUI_MOTION_SCALE", "1")
        .env("POCKETUI_SOUND_QUANTUM", "0")
        .env("POCKETUI_DATA", tmp.join("data")).env("POCKETUI_ES_SYSTEMS", tmp.join("es_systems.cfg"))
        .env("POCKETUI_VISIBLE_TEXT", &visible).env("POCKETUI_TEXT_BOXES", &boxes);
    if let Some(display) = display { cmd.env("WAYLAND_DISPLAY", display); }
    let out = cmd.output().unwrap();
    assert!(out.status.success(), "{state}: {}", String::from_utf8_lossy(&out.stderr));
    assert!(png.is_file(), "a real snapshot was rendered");
    let problems = std::fs::read_to_string(boxes).unwrap_or_default();
    assert!(problems.trim().is_empty(), "{state}, {lang}/{text_size}: {problems}");
    std::fs::read_to_string(visible).unwrap().split_whitespace().collect::<Vec<_>>().join(" ")
}

#[test]
fn the_full_card_can_scroll_to_the_empty_file_cause_and_remedy_after_a_long_german_large_title() {
    let tmp = tempfile::tempdir().unwrap();
    let (_stub, display) = fixture(tmp.path());
    let game = rom(tmp.path(), "snes", "link to the past");
    std::fs::write(&game, b"").unwrap();
    let mut seen = String::new();
    for downs in [0, 2, 4, 6] {
        seen.push_str(&shot(tmp.path(), display.as_deref(), &format!("curator:card:snes:past:{downs}"), "de", "large"));
        seen.push(' ');
    }
    assert!(seen.contains("Link to the Past"), "the long title is the one being rendered: {seen}");
    assert!(seen.contains("Datei ist leer") && seen.contains("0 Byte"), "the cause survives beyond the title's first three lines: {seen}");
    assert!(seen.contains("Kopiere das Spiel noch einmal auf die Karte."), "the remedy is reachable too: {seen}");
    std::fs::write(&game, b"x").unwrap();
    let healthy = shot(tmp.path(), display.as_deref(), "curator:card:snes:past", "de", "large");
    assert!(!healthy.contains("Problem") && !healthy.contains("Datei ist leer"), "healthy games have no error: {healthy}");
}

#[test]
fn all_five_default_card_looks_show_the_problem_and_keep_their_connection_facts_working() {
    let tmp = tempfile::tempdir().unwrap();
    let (_stub, display) = fixture(tmp.path());
    std::fs::write(rom(tmp.path(), "snes", "link to the past"), b"").unwrap();
    std::fs::write(rom(tmp.path(), "ps2", "revenge"), b"").unwrap();
    for look in ["minimal", "cards", "split", "shelf", "cabinet"] {
        let boxed = matches!(look, "shelf" | "cabinet");
        for (lang, size, cause, remedy) in [
            ("en", "normal", "file is empty (0 bytes)", "Copy the game to the card again."),
            ("de", "large", "Datei ist leer (0 Byte)", "Kopiere das Spiel noch einmal auf die Karte."),
        ] {
            // The box's unopened front must explain the problem too; turning over keeps the explanation.
            for keys in if boxed { &["", ":r"][..] } else { &[""][..] } {
                let state = format!("info:{look}:snes:past{keys}");
                let seen = shot(tmp.path(), display.as_deref(), &state, lang, size);
                assert!(seen.contains("Problem") && seen.contains(cause) && seen.contains(remedy), "{state} {lang}/{size}: {seen}");
            }
        }
        // Problem is outside CardData::facts: Down/A still opens the same Developer connection as on a healthy card.
        let keys = if boxed { "rda" } else { "da" };
        let state = format!("info:{look}:ps2:revenge:{keys}");
        let seen = shot(tmp.path(), display.as_deref(), &state, "en", "normal");
        // Shelf/Cabinet bake the other titles into spine pictures; their transcript only names the selected game.
        assert!(seen.contains("More from Criterion Games"), "the Developer fact retains its identity in {look}: {seen}");
    }
}
