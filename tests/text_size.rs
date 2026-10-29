//! Settings → Look & feel → Text size (docs/DESIGN.md, Type; `src/type_scale.rs`) against the snapshot suite: every
//! state of `tests/i18n-states.txt` is rendered at Large/German, and `layout-risk-states.txt` at all four corners
//! (Medium/Large × English/German), with scripts/curator-fake-library.py's
//! made-up library, and the text painter's record of where each string went (`POCKETUI_TEXT_BOXES`, `text::boxes`)
//! must show
//! - no string whose ink runs into another string's (`O`) or into a hint bar,
//! - no two strings on one line closer than a space (`T`: they would read as one word),
//! - no string cut by the left or right edge of what it is painted into (`E`; a list's band cutting its rows at the
//!   top and bottom while they scroll is fine, and so is what a page marks as cut on purpose).
//!
//! A second test makes sure the size comes from settings.json and reaches the Settings row.
//!
//! Needs python3 for the made-up library and, on Linux without a compositor, scripts/wayland-stub.py (as
//! tests/i18n_snapshots.rs). About 700 snapshots, eight at a time.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};

const ROOT: &str = env!("CARGO_MANIFEST_DIR");

fn states() -> Vec<(bool, String)> {
    std::fs::read_to_string(Path::new(ROOT).join("tests/i18n-states.txt"))
        .expect("tests/i18n-states.txt")
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| match l.strip_prefix("safe ") {
            Some(s) => (true, s.to_string()),
            None => (false, l.to_string()),
        })
        .filter(|(_, state)| {
            std::env::var_os("POCKETUI_TEST_QUICK").is_none()
                || matches!(state.as_str(), "settings" | "settings:0:2" | "shelf:gb:split" | "sys:quick"
                    | "sys:power" | "tailscale" | "achievements" | "search:results:mario" | "notice:bios"
                    | "setup:wifi" | "menu:pocket-core" | "storage")
                || std::env::var("POCKETUI_TEST_STATES").is_ok_and(|extra| extra.lines().any(|s| s == state))
        })
        .collect()
}

/// scripts/wayland-stub.py, while it runs.
struct Stub {
    path: PathBuf,
    child: std::process::Child,
}

impl Drop for Stub {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn wayland(tmp: &Path) -> Option<Stub> {
    if !cfg!(target_os = "linux") || std::env::var_os("WAYLAND_DISPLAY").is_some_and(|w| !w.is_empty()) {
        return None;
    }
    let path = tmp.join("wayland-stub");
    let child = Command::new("python3").args(["-I", &format!("{ROOT}/scripts/wayland-stub.py"), path.to_str().unwrap()]).spawn().expect("python3 for scripts/wayland-stub.py");
    let stub = Stub { path, child };
    for _ in 0..200 {
        if stub.path.exists() {
            return Some(stub);
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    panic!("scripts/wayland-stub.py didn't start");
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap().flatten() {
        let target = to.join(e.file_name());
        if e.path().is_dir() {
            copy_dir(&e.path(), &target);
        } else {
            std::fs::copy(e.path(), target).unwrap();
        }
    }
}

fn fake_library(dir: &Path) {
    let status = Command::new("python3")
        .args(["-I", &format!("{ROOT}/scripts/curator-fake-library.py"), dir.to_str().unwrap(), "dark"])
        .stdout(std::process::Stdio::null())
        .status()
        .expect("python3 for scripts/curator-fake-library.py");
    assert!(status.success(), "curator-fake-library.py failed");
}

#[test]
fn in_medium_and_large_text_no_string_runs_into_another_or_off_its_room() {
    let _tmp_guard = tempfile::tempdir().unwrap();
    let tmp = _tmp_guard.path().to_path_buf();
    fake_library(&tmp);
    let bin = PathBuf::from(env!("CARGO_BIN_EXE_pocket-ui"));
    let stub = wayland(&tmp);
    let risk: std::collections::HashSet<_> = include_str!("layout-risk-states.txt")
        .lines().map(str::trim).filter(|s| !s.is_empty() && !s.starts_with('#')).collect();
    let all = states();
    if std::env::var_os("POCKETUI_TEST_QUICK").is_none() {
        for state in &risk {
            assert!(all.iter().any(|(_, s)| s == state), "unknown layout risk state: {state}");
        }
    }
    let mut jobs = Vec::new();
    for size in ["medium", "large"] {
        for lang in ["en", "de"] {
            for (i, (safe, state)) in states().into_iter().enumerate() {
                // Every state at the largest size/longest language; the risky layouts also at the other corners.
                if (size == "large" && lang == "de") || risk.contains(state.as_str())
                    || std::env::var_os("POCKETUI_TEST_QUICK").is_some()
                {
                    jobs.push((format!("{size}-{lang}-{i}"), size, lang, safe, state));
                }
            }
        }
    }
    let work = Arc::new(Mutex::new(jobs));
    let found = Arc::new(Mutex::new(Vec::new()));
    let threads: Vec<_> = (0..std::thread::available_parallelism().map_or(4, |n| n.get().min(8)))
        .map(|_| {
            let (work, found, tmp, bin, stub) = (work.clone(), found.clone(), tmp.clone(), bin.clone(), stub.as_ref().map(|s| s.path.clone()));
            std::thread::spawn(move || loop {
                let Some((name, size, lang, safe, state)) = work.lock().unwrap().pop() else { break };
                // Its own copy of the data folder: some states write into it, and they run side by side.
                let data = tmp.join(format!("data-{name}"));
                copy_dir(&tmp.join("data"), &data);
                let (png, boxes) = (tmp.join(format!("{name}.png")), tmp.join(format!("{name}.box")));
                let mut cmd = Command::new(&bin);
                if safe {
                    cmd.arg("--safe");
                }
                if let Some(path) = &stub {
                    cmd.env("WAYLAND_DISPLAY", path);
                }
                let out = cmd
                    .args(["--snapshot", &state, png.to_str().unwrap()])
                    .env("POCKETUI_MOCK", "1")
                    .env("POCKETUI_LANG", lang)
                    .env("POCKETUI_TEXT_SIZE", size)
                    .env("POCKETUI_TEXT_BOXES", &boxes)
                    .env("POCKETUI_ES_SYSTEMS", tmp.join("es_systems.cfg"))
                    .env("POCKETUI_DATA", &data)
                    .output()
                    .expect("run pocket-ui");
                let mut found = found.lock().unwrap();
                if !out.status.success() || !png.exists() {
                    found.push(format!("{state} ({size}, {lang}) didn't render: {}", String::from_utf8_lossy(&out.stderr)));
                    continue;
                }
                match std::fs::read_to_string(&boxes) {
                    Ok(text) => found.extend(text.lines().filter(|l| !l.is_empty()).map(|l| format!("{state} ({size}, {lang}): {}", l.replace('\t', " | ")))),
                    Err(_) => found.push(format!("{state} ({size}, {lang}): no record of the painted text (does its snapshot end in write_png?)")),
                }
                let _ = std::fs::remove_dir_all(&data);
            })
        })
        .collect();
    for t in threads {
        t.join().unwrap();
    }
    drop(stub);
    let mut found = found.lock().unwrap().clone();
    found.sort();
    assert!(
        found.is_empty(),
        "{} problems with Settings → Text size → Medium and Large (O: runs into another string or a hint bar, T: touches the string beside it, E: cut by the edge):\n{}",
        found.len(),
        found.join("\n")
    );
}

/// The choice in settings.json is what a start uses (no override here), and the Settings row says it.
#[test]
fn the_text_size_in_settings_json_is_used_and_shown_on_its_row() {
    let _tmp_guard = tempfile::tempdir().unwrap();
    let tmp = _tmp_guard.path().to_path_buf();
    let stub = wayland(&tmp);
    let shot = |settings: &str, name: &str| -> String {
        let data = tmp.join(name);
        std::fs::create_dir_all(&data).unwrap();
        std::fs::write(data.join("settings.json"), settings).unwrap();
        let record = tmp.join(format!("{name}.tsv"));
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
        // Look & feel's third row, Text size, selected: its value shows as "‹ Groß ›" or "‹ Normal ›".
        cmd.args(["--snapshot", "settings:0:2", tmp.join(format!("{name}.png")).to_str().unwrap()])
            .env("POCKETUI_MOCK", "1")
            .env("POCKETUI_LANG", "de")
            .env_remove("POCKETUI_TEXT_SIZE")
            .env("POCKETUI_I18N_MISSES", &record)
            .env("POCKETUI_ES_SYSTEMS", tmp.join("none.cfg"))
            .env("POCKETUI_DATA", &data);
        if let Some(s) = &stub {
            cmd.env("WAYLAND_DISPLAY", &s.path);
        }
        let out = cmd.output().expect("run pocket-ui");
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        std::fs::read_to_string(record).unwrap_or_default()
    };
    let large = shot(r#"{"onboarded": true, "text_size": "large"}"#, "large");
    assert!(large.contains("O\tSchriftgröße") && large.contains("Groß ›"), "Large chosen:\n{large}");
    let medium = shot(r#"{"onboarded": true, "text_size": "medium"}"#, "medium");
    assert!(medium.contains("O\tSchriftgröße") && medium.contains("Mittel ›") && !medium.contains("Groß"), "Medium chosen:\n{medium}");
    let normal = shot(r#"{"onboarded": true}"#, "normal");
    assert!(normal.contains("O\tSchriftgröße") && !normal.contains("Groß"), "nothing chosen: Normal\n{normal}");
    drop(stub);
}

/// Real game metadata, rather than short synthetic labels: the first frame must fit, including
/// the selected search result (Marquee) and the two-line game card header.
#[test]
fn long_game_titles_fit_at_rest_in_both_languages_and_large_text() {
    let tmp = tempfile::tempdir().unwrap();
    let stub = wayland(tmp.path());
    let output = std::env::var_os("POCKETUI_TEXT_FIT_OUTPUT").map(PathBuf::from);
    if let Some(dir) = &output {
        std::fs::create_dir_all(dir).unwrap();
    }
    for look in ["dark", "light"] {
        let lib = tmp.path().join(look);
        let status = Command::new("python3")
            .args(["-I", &format!("{ROOT}/scripts/curator-fake-library.py"), lib.to_str().unwrap(), look])
            .env("POCKETUI_TEXT_FIT", "1").stdout(std::process::Stdio::null()).status().unwrap();
        assert!(status.success());
        for size in ["normal", "large"] {
            for lang in ["en", "de"] {
                for dimensions in ["1240x1080", "1280x960"] {
                    for state in ["switcher", "search:results:pokemon", "search:results:super mario advance", "curator:card:gba:pokemon", "curator:card:gba:super mario", "curator:card:ps2:revenge"] {
                        let name = format!("{look}-{size}-{lang}-{dimensions}-{}", state.replace([':', ' '], "-"));
                        let png = output.as_deref().unwrap_or(tmp.path()).join(format!("{name}.png"));
                        let boxes = tmp.path().join("text.box");
                        let visible = tmp.path().join("visible.txt");
                        let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
                        cmd.args(["--snapshot", state, png.to_str().unwrap()])
                            .env("POCKETUI_MOCK", "1").env("POCKETUI_LANG", lang)
                            .env("POCKETUI_TEXT_SIZE", size).env("POCKETUI_SNAPSHOT_SIZE", dimensions)
                            .env("POCKETUI_TEXT_BOXES", &boxes).env("POCKETUI_VISIBLE_TEXT", &visible)
                            .env("POCKETUI_ES_SYSTEMS", lib.join("es_systems.cfg"))
                            .env("POCKETUI_DATA", lib.join("data"));
                        if let Some(s) = &stub { cmd.env("WAYLAND_DISPLAY", &s.path); }
                        let out = cmd.output().unwrap();
                        assert!(out.status.success() && png.exists(), "{name}: {}", String::from_utf8_lossy(&out.stderr));
                        let painted = std::fs::read_to_string(&visible).expect("visible title record");
                        let expected = if state.contains("super mario") { "Super Mario" } else if state.contains("revenge") { "Burnout Revenge" } else { "Pokemon" };
                        assert!(painted.contains(expected), "{name} did not paint {expected}: {painted}");
                        let problems = std::fs::read_to_string(&boxes).expect("painted text record");
                        assert!(problems.trim().is_empty(), "{name}: {problems}");
                    }
                }
            }
        }
    }
}
