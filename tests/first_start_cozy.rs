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
    let tmp_guard = tempfile::Builder::new().prefix("pui-cozy-").tempdir().unwrap();
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
                    let probe = tmp.join(format!("{i}.probe"));
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
                        .env("POCKETUI_NAV_PROBE", &probe).envs(device(size))
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
                    // What the level brings is there to its end: the lowest line on the page ends its sentence. Lines that
                    // ran under the hint bar were left out, the German one broken off at "alter" (Large, Brick, 20.11.).
                    let strings = probe_strings(&probe);
                    // The hint bar's keys under the rows (L2 Aa at the top right is one too).
                    let rows_end = strings.iter().filter(|(_, s)| s.starts_with("‹nav:focus›")).map(|(r, _)| r[3]).max().unwrap_or(0);
                    let hints = strings.iter().filter(|(r, s)| s.starts_with("‹nav:hint›") && r[1] > rows_end).map(|(r, _)| r[1]).min().unwrap_or(i32::MAX);
                    match strings.iter().filter(|(r, s)| !s.starts_with('‹') && r[3] <= hints).max_by_key(|(r, _)| r[1]) {
                        Some((_, last)) if last.trim_end().ends_with(['.', ')']) => {}
                        last => failed.push(format!("{name}: what the level brings ends {last:?}, not its sentence")),
                    }
                    rows_hold_their_text(&strings, &name, &mut failed);
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

/// The device a snapshot size stands for: the Brick's 1024x768 with its own type (`type_scale::brick_base` in a
/// `--features brick` build), the others as they are.
fn device(size: &str) -> Vec<(&'static str, &'static str)> {
    if size == "1024x768" { vec![("POCKETUI_DEVICE", "brick")] } else { Vec::new() }
}

/// The strings and marks of a frame (`POCKETUI_NAV_PROBE`: "x0 y0 x1 y1 <tab> text", ink rectangles).
fn probe_strings(probe: &std::path::Path) -> Vec<([i32; 4], String)> {
    std::fs::read_to_string(probe).unwrap_or_default().lines().filter_map(|l| {
        let v: Vec<&str> = l.splitn(5, '\t').collect();
        let n = |i: usize| v.get(i).and_then(|n| n.parse().ok());
        Some(([n(0)?, n(1)?, n(2)?, n(3)?], v.get(4)?.to_string()))
    }).collect()
}

/// The selected row (its focus mark) holds the text painted in it: no string that starts in it reaches out of it (German
/// names in Large stood 6 px above the Brick's 86-px rows of "How do you like to play?", review 20.11.).
fn rows_hold_their_text(strings: &[([i32; 4], String)], name: &str, failed: &mut Vec<String>) {
    let Some((f, _)) = strings.iter().find(|(_, s)| s.starts_with("‹nav:focus›")) else {
        failed.push(format!("{name}: no selected row marked"));
        return;
    };
    for (r, s) in strings.iter().filter(|(_, s)| !s.starts_with('‹')) {
        let (mid_x, mid_y) = ((r[0] + r[2]) / 2, (r[1] + r[3]) / 2);
        let inside = mid_x >= f[0] && mid_x < f[2] && mid_y >= f[1] && mid_y < f[3];
        if inside && (r[1] < f[1] || r[3] > f[3]) {
            failed.push(format!("{name}: {s:?} at {r:?} reaches out of its row {f:?}"));
        }
    }
}

/// "How do you like to play?" (`features::paint_profiles`), the first start's page before the look: each row selected in
/// turn, on the screens and in the languages and sizes of the cozy page; the name and the line under it stay in their row,
/// no text into other text or cut by an edge.
#[test]
fn the_play_page_keeps_each_name_in_its_row() {
    let tmp_guard = tempfile::Builder::new().prefix("pui-play-").tempdir().unwrap();
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
    let sizes: &[&str] = if quick { &["1024x768"] } else { &["640x480", "1024x768", "1280x720", "1920x1080"] };
    std::fs::write(tmp.join("data").join("settings.json"), r#"{"theme": "shelf", "onboarded": false, "motion": "off"}"#).unwrap();
    let mut failed = Vec::new();
    for size in sizes {
        for lang in ["en", "de", "fr"] {
            for large in [false, true] {
                for row in 0..5 {
                    let keys = "d".repeat(row);
                    let state = if keys.is_empty() { "setup:play".to_string() } else { format!("setup:play:{keys}") };
                    let (png, boxes, probe) = (tmp.join("play.png"), tmp.join("play.boxes"), tmp.join("play.probe"));
                    let mut cmd = Command::new(&bin);
                    if socket.exists() {
                        cmd.env("WAYLAND_DISPLAY", &socket).env_remove("SWAYSOCK");
                    }
                    let out = cmd.args(["--snapshot", &state, png.to_str().unwrap()])
                        .env("POCKETUI_SNAPSHOT_SIZE", size).env("POCKETUI_SNAPSHOT_FROZEN", "1")
                        .env("POCKETUI_MOCK", "1").env("POCKETUI_MOCK_NOW", FIXTURE_NOW).env("POCKETUI_LANG", lang)
                        .env("POCKETUI_TEXT_SIZE", if large { "large" } else { "normal" }).envs(device(size))
                        .env("POCKETUI_ES_SYSTEMS", tmp.join("es_systems.cfg")).env("POCKETUI_DATA", tmp.join("data"))
                        .env("POCKETUI_TEXT_BOXES", &boxes).env("POCKETUI_NAV_PROBE", &probe)
                        .output().expect("run pocket-ui");
                    let name = format!("{size} {lang} {} row {row}", if large { "large" } else { "normal" });
                    if !out.status.success() {
                        failed.push(format!("{name}: did not render: {}", String::from_utf8_lossy(&out.stderr)));
                        continue;
                    }
                    if let Some(dir) = &shots {
                        std::fs::create_dir_all(dir).unwrap();
                        std::fs::copy(&png, dir.join(format!("play-{}.png", name.replace(' ', "-")))).unwrap();
                    }
                    let bad: Vec<String> = std::fs::read_to_string(&boxes).unwrap_or_default().lines()
                        .filter(|l| l.starts_with(['E', 'O', 'V', 'W', 'T'])).map(str::to_string).collect();
                    if !bad.is_empty() {
                        failed.push(format!("{name}: {bad:?}"));
                    }
                    rows_hold_their_text(&probe_strings(&probe), &name, &mut failed);
                }
            }
        }
    }
    assert!(failed.is_empty(), "{}", failed.join("\n"));
}
