//! "How cozy should it be?" (19.11.2026), the first start's page after the look (`src/cozy_level.rs`): painted by the real
//! program on the screens pocket-ui OS runs on (640x480 H700, 1024x768 Brick, 1280x720 Smart Pro, 1920x1080, the Mini V2's
//! 1240x1080) in every language at Normal and Large text, each level selected: no word cut short, no text off its room or
//! into other text, and the page names its three levels. `FIRST_START_SHOTS=<dir>` keeps the pictures.
//!
//! Needs python3 for the made-up library and, on Linux without a compositor, for scripts/wayland-stub.py.
use std::path::PathBuf;
use std::process::Command;
include!("common/root.rs");
struct Stub(std::process::Child);
impl Drop for Stub {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn the_cozy_page_fits_every_screen_language_and_text_size() {
    let tmp_guard = tempfile::Builder::new().prefix("pui-cozy-").tempdir_in("/tmp").unwrap();
    let tmp = tmp_guard.path().to_path_buf();
    let status = Command::new("python3").env("POCKETUI_MOCK_NOW", FIXTURE_NOW)
        .args(["-I", &format!("{ROOT}/scripts/curator-fake-library.py"), tmp.to_str().unwrap(), "dark"])
        .stdout(std::process::Stdio::null()).status().expect("python3 for scripts/curator-fake-library.py");
    assert!(status.success());
    let socket = tmp.join("wl");
    let _stub = (cfg!(target_os = "linux") && std::env::var_os("WAYLAND_DISPLAY").is_none_or(|w| w.is_empty())).then(|| {
        let child = Command::new("python3").args(["-I", &format!("{ROOT}/scripts/wayland-stub.py"), socket.to_str().unwrap()]).spawn().expect("python3");
        test_clock::wait_for(|| socket.exists(), std::time::Duration::from_secs(2), "Wayland stub listening");
        Stub(child)
    });
    let bin: PathBuf = std::env::var_os("POCKETUI_NAV_BINARY").map(PathBuf::from).unwrap_or_else(|| env!("CARGO_BIN_EXE_pocket-ui").into());
    let shots = std::env::var_os("FIRST_START_SHOTS").map(PathBuf::from);
    let quick = std::env::var_os("POCKETUI_TEST_QUICK").is_some();
    let sizes: &[&str] = if quick { &["640x480", "1920x1080"] } else { &["640x480", "1024x768", "1280x720", "1920x1080", "1240x1080"] };
    let mut failed = Vec::new();
    let mut i = 0;
    for size in sizes {
        for lang in ["en", "de", "es", "pt", "fr", "it"] {
            for large in [false, true] {
                // Each level selected: the page opens on Cozy, Up is Plain, Down Everything.
                for (keys, level) in [("", "Cozy"), ("u", "Plain"), ("d", "Everything")] {
                    i += 1;
                    let data = tmp.join(format!("data-{i}"));
                    std::fs::create_dir_all(&data).unwrap();
                    for e in std::fs::read_dir(tmp.join("data")).unwrap().flatten() {
                        if e.path().is_file() {
                            std::fs::copy(e.path(), data.join(e.file_name())).unwrap();
                        }
                    }
                    std::fs::write(data.join("settings.json"), r#"{"theme": "shelf", "onboarded": false, "motion": "off"}"#).unwrap();
                    let (png, boxes, seen) = (tmp.join(format!("{i}.png")), tmp.join(format!("{i}.boxes")), tmp.join(format!("{i}.txt")));
                    let state = if keys.is_empty() { "setup:first-cozy".to_string() } else { format!("setup:first-cozy:{keys}") };
                    let mut cmd = Command::new(&bin);
                    if socket.exists() {
                        cmd.env("WAYLAND_DISPLAY", &socket).env_remove("SWAYSOCK");
                    }
                    let out = cmd.args(["--snapshot", &state, png.to_str().unwrap()])
                        .env("POCKETUI_SNAPSHOT_SIZE", size).env("POCKETUI_SNAPSHOT_FROZEN", "1")
                        .env("POCKETUI_MOCK", "1").env("POCKETUI_MOCK_NOW", FIXTURE_NOW).env("POCKETUI_LANG", lang)
                        .env("POCKETUI_TEXT_SIZE", if large { "large" } else { "normal" })
                        .env("POCKETUI_ES_SYSTEMS", tmp.join("es_systems.cfg")).env("POCKETUI_DATA", &data)
                        .env("POCKETUI_TEXT_BOXES", &boxes).env("POCKETUI_VISIBLE_TEXT", &seen)
                        .output().expect("run pocket-ui");
                    let name = format!("{size} {lang} {} {level}", if large { "large" } else { "normal" });
                    if !out.status.success() {
                        failed.push(format!("{name}: did not render: {}", String::from_utf8_lossy(&out.stderr)));
                        continue;
                    }
                    if let Some(dir) = &shots {
                        std::fs::create_dir_all(dir).unwrap();
                        std::fs::copy(&png, dir.join(format!("cozy-{}.png", name.replace(' ', "-")))).unwrap();
                    }
                    let bad: Vec<String> = std::fs::read_to_string(&boxes).unwrap_or_default().lines()
                        .filter(|l| l.starts_with(['E', 'O', 'V', 'W', 'T'])).map(str::to_string).collect();
                    if !bad.is_empty() {
                        failed.push(format!("{name}: {bad:?}"));
                    }
                    let said = std::fs::read_to_string(&seen).unwrap_or_default();
                    // Nothing cut short with "…" (the French title was, at 1024x768, 19.11.).
                    if let Some(cut) = said.lines().find(|l| l.contains('…')) {
                        failed.push(format!("{name}: cut short: {cut:?}"));
                    }
                    if lang == "en" {
                        for w in ["How cozy should it be?", "Plain", "Cozy", "Everything"] {
                            if !said.lines().any(|l| l.contains(w)) {
                                failed.push(format!("{name}: {w:?} not on the page: {said}"));
                            }
                        }
                    }
                }
            }
        }
    }
    assert!(failed.is_empty(), "{}", failed.join("\n"));
}
