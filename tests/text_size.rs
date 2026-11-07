//! Settings → Look → Text size (docs/DESIGN.md, Type; `src/type_scale.rs`) against the snapshot suite: every
//! state of `tests/i18n-states.txt` is rendered at Large/German, and `layout-risk-states.txt` at all four corners
//! (Medium/Large × English/German), with scripts/curator-fake-library.py's
//! made-up library, and the text painter's record of where each string went (`POCKETUI_TEXT_BOXES`, `text::boxes`)
//! must show
//! - no string whose ink runs into another string's (`O`) or into a hint bar,
//! - no two strings on one line closer than a space (`T`: they would read as one word),
//! - no string cut by the top or bottom edge of the picture itself (`V`: a heading half off the screen),
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

include!("common/root.rs");

fn states() -> Vec<(bool, String)> {
    std::fs::read_to_string(Path::new(&ROOT).join("tests/i18n-states.txt"))
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
                || matches!(state.as_str(), "settings" | "settings:0:4" | "shelf:gb:split" | "sys:quick"
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
    test_clock::wait_for(|| stub.path.exists(), std::time::Duration::from_secs(2), "Wayland stub listening");
    Some(stub)
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

/// Renders every job `(name, size, lang, safe, state)` (at `picture`, "WxH", when given) and returns the lines of the
/// text record, `state (size, lang): line`, of those whose letter (the first character: O, T, V, E) is in `only` (all of
/// them when empty), plus the `X` lines of the i18n record (a string drawn past the right edge) when `only` has an X,
/// plus a line for each state that didn't render.
fn render_all(tmp: &Path, bin: &Path, stub: Option<&Stub>, jobs: Vec<(String, &'static str, &'static str, bool, String)>, picture: Option<&str>, only: &str) -> Vec<String> {
    render_all_in(tmp, bin, stub, jobs, picture, only, &[])
}

/// `render_all` with more of the environment set for every snapshot (`POCKETUI_RETRO=cocoa`: Settings → Look → Style →
/// Retro in that palette).
fn render_all_in(tmp: &Path, bin: &Path, stub: Option<&Stub>, jobs: Vec<(String, &'static str, &'static str, bool, String)>, picture: Option<&str>, only: &str, env: &[(&str, &str)]) -> Vec<String> {
    let (tmp, bin, stub) = (tmp.to_path_buf(), bin.to_path_buf(), stub.map(|s| s.path.clone()));
    let env: Vec<(String, String)> = env.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
    let picture = picture.map(str::to_string);
    let only = only.to_string();
    let work = Arc::new(Mutex::new(jobs));
    let found = Arc::new(Mutex::new(Vec::new()));
    let threads: Vec<_> = (0..std::thread::available_parallelism().map_or(4, |n| n.get().min(8)))
        .map(|_| {
            let (work, found, tmp, bin, stub, picture, only, env) = (work.clone(), found.clone(), tmp.clone(), bin.clone(), stub.clone(), picture.clone(), only.clone(), env.clone());
            std::thread::spawn(move || loop {
                let Some((name, size, lang, safe, state)) = work.lock().unwrap().pop() else { break };
                // Its own copy of the data folder: some states write into it, and they run side by side.
                let data = tmp.join(format!("data-{name}"));
                copy_dir(&tmp.join("data"), &data);
                let (png, boxes, misses) = (tmp.join(format!("{name}.png")), tmp.join(format!("{name}.box")), tmp.join(format!("{name}.tsv")));
                let mut cmd = Command::new(&bin);
                if safe {
                    cmd.arg("--safe");
                }
                if let Some(path) = &stub {
                    cmd.env("WAYLAND_DISPLAY", path);
                }
                if let Some(p) = &picture {
                    cmd.env("POCKETUI_SNAPSHOT_SIZE", p);
                }
                cmd.envs(env.iter().map(|(k, v)| (k, v)));
                let out = cmd
                    .args(["--snapshot", &state, png.to_str().unwrap()])
                    .env("POCKETUI_MOCK", "1")
                    .env("POCKETUI_LANG", lang)
                    .env("POCKETUI_TEXT_SIZE", size)
                    .env("POCKETUI_TEXT_BOXES", &boxes)
                    .env("POCKETUI_I18N_MISSES", &misses)
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
                    Ok(text) => found.extend(text.lines().filter(|l| !l.is_empty() && (only.is_empty() || l.chars().next().is_some_and(|c| only.contains(c)))).map(|l| format!("{state} ({size}, {lang}): {}", l.replace('\t', " | ")))),
                    Err(_) => found.push(format!("{state} ({size}, {lang}): no record of the painted text (does its snapshot end in write_png?)")),
                }
                if only.contains('X') {
                    if let Ok(text) = std::fs::read_to_string(&misses) {
                        found.extend(text.lines().filter(|l| l.starts_with("X\t")).map(|l| format!("{state} ({size}, {lang}): {}", l.replace('\t', " | "))));
                    }
                }
                let _ = std::fs::remove_dir_all(&data);
            })
        })
        .collect();
    for t in threads {
        t.join().unwrap();
    }
    let found = found.lock().unwrap().clone();
    found
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
    let mut found = render_all(&tmp, &bin, stub.as_ref(), jobs, None, "");
    drop(stub);
    found.sort();
    assert!(
        found.is_empty(),
        "{} problems with Settings → Text size → Medium and Large (O: runs into another string or a hint bar, T: touches the string beside it, E: cut by the side edge, V: cut by the top or bottom edge):\n{}",
        found.len(),
        found.join("\n")
    );
}

/// Settings → Look → Style → Retro paints every page in Pixelify Sans, wider than Barlow: every state in Large text and
/// German, at the Mini V2's size and at 640x480 (the H700 handhelds), with nothing that runs into another string, a hint
/// bar or an edge. Cocoa and Pea share their sizes, so one palette stands for both.
#[test]
fn retro_in_large_german_text_no_string_runs_into_another_or_off_its_room() {
    let _tmp_guard = tempfile::tempdir().unwrap();
    let tmp = _tmp_guard.path().to_path_buf();
    fake_library(&tmp);
    let bin = PathBuf::from(env!("CARGO_BIN_EXE_pocket-ui"));
    let stub = wayland(&tmp);
    let mut found = Vec::new();
    for picture in [None, Some("640x480")] {
        let jobs = states().into_iter().enumerate().map(|(i, (safe, state))| (format!("retro-{}-{i}", picture.unwrap_or("native")), "large", "de", safe, state)).collect();
        found.extend(render_all_in(&tmp, &bin, stub.as_ref(), jobs, picture, "", &[("POCKETUI_RETRO", "cocoa")]).into_iter().map(|f| format!("{}: {f}", picture.unwrap_or("1240x1080"))));
    }
    drop(stub);
    found.sort();
    assert!(found.is_empty(), "{} problems with Retro in Large German text (O overlap, T touch, E side edge, V top/bottom edge):\n{}", found.len(), found.join("\n"));
}

/// "How do you like to play?" in Retro, Large text and Spanish on a 1024x768 screen (the Brick, 2026-11-07): the heading
/// "¿Cómo te gusta jugar?" and the line under it touched. Every language, normal and Large, at 1024x768 and 640x480.
#[test]
fn retro_play_question_keeps_its_heading_off_the_line_under_it() {
    let _tmp_guard = tempfile::tempdir().unwrap();
    let tmp = _tmp_guard.path().to_path_buf();
    fake_library(&tmp);
    let bin = PathBuf::from(env!("CARGO_BIN_EXE_pocket-ui"));
    let stub = wayland(&tmp);
    let mut found = Vec::new();
    for picture in ["1024x768", "640x480"] {
        let mut jobs = Vec::new();
        for (i, size) in ["normal", "large"].into_iter().enumerate() {
            for lang in ["en", "de", "es"] {
                jobs.push((format!("retro-play-{picture}-{lang}-{i}"), size, lang, false, "setup:play".to_string()));
            }
        }
        found.extend(render_all_in(&tmp, &bin, stub.as_ref(), jobs, Some(picture), "", &[("POCKETUI_RETRO", "cocoa")]).into_iter().map(|f| format!("{picture}: {f}")));
    }
    drop(stub);
    found.sort();
    assert!(found.is_empty(), "{} problems on the play question in Retro (O overlap, T touch, E side edge, V top/bottom edge):\n{}", found.len(), found.join("\n"));
}

/// The pause menu, its Save/Load slots, Options, quick settings and pocket-core's notes over a game in Retro, as
/// the Retro clip showed them (06.11.2026): "Save & quit" in German ran through the rows' window on 640x480,
/// Load's "Where I quit" ran under the preview window, the game's name was cut short by the battery line. Every one in
/// Large text, German and Spanish, at 1240x1080, 640x480 and 1024x768, behind a long real name (Donkey Kong Country 3's):
/// nothing may run into another string, a hint bar, a window's frame (the pause menu records its windows' lines,
/// `retro::frame_lines_are_no_text`) or an edge.
#[test]
fn retro_pause_menu_in_large_german_and_spanish_text_stays_inside_its_windows() {
    let _tmp_guard = tempfile::tempdir().unwrap();
    let tmp = _tmp_guard.path().to_path_buf();
    fake_library(&tmp);
    let bin = PathBuf::from(env!("CARGO_BIN_EXE_pocket-ui"));
    let stub = wayland(&tmp);
    let states = [
        "menu:pocket-core", "menu:pocket-core:2", "menu:pocket-core:3", "menu:pocket-core:5", "menu:pocket-core:0:battery=all",
        "menu:pocket-core:0:disc=1/3", "menu:pocket-core:5:save=full", "menu:retroarch", "menu:ps2", "menu:scummvm:3",
        "slot-save", "slot-load", "slot-auto", "slot-undo", "slot-earlier", "slot-load:snes:card", "slot-save:psx",
        "options", "game-quick", "game-twin-note", "game-save-warning", "achievements-menu", "players-menu:full",
    ];
    let game = [("POCKETUI_RETRO", "cocoa"), ("POCKETUI_SNAPSHOT_GAME", "snes|Donkey Kong Country 3: Dixie Kong's Double Trouble!|dkc3.sfc")];
    let mut found = Vec::new();
    for picture in [None, Some("640x480"), Some("1024x768")] {
        let mut jobs = Vec::new();
        for lang in ["de", "es"] {
            for (i, state) in states.iter().enumerate() {
                jobs.push((format!("retro-pause-{}-{lang}-{i}", picture.unwrap_or("native")), "large", lang, false, state.to_string()));
            }
        }
        found.extend(render_all_in(&tmp, &bin, stub.as_ref(), jobs, picture, "", &game).into_iter().map(|f| format!("{}: {f}", picture.unwrap_or("1240x1080"))));
    }
    // And what was cut short there shows whole now: the longest names and Load's "Where I quit", and the game's name
    // beside the battery line.
    let whole = [
        ("640x480", "de", "menu:pocket-core", "Donkey Kong Country 3: Dixie Kong's Double Trouble!", "Sichern & Ende"),
        ("1024x768", "es", "slot-earlier", "Donkey Kong Country 3: Dixie Kong's Double Trouble!", "Guardar y salir"),
        ("640x480", "de", "slot-auto", "Donkey Kong Country 3: Dixie Kong's Double Trouble!", "Wo ich aufhörte"),
        ("640x480", "es", "slot-auto", "Donkey Kong Country 3: Dixie Kong's Double Trouble!", "Donde lo dejé"),
        ("1240x1080", "en", "slot-auto", "Donkey Kong Country 3: Dixie Kong's Double Trouble!", "Where I quit"),
        ("1240x1080", "en", "menu:pocket-core:0:battery=all", "Donkey Kong Country 3", "Donkey Kong Country 3"),
    ];
    for (i, (picture, lang, state, game, want)) in whole.into_iter().enumerate() {
        let (data, seen) = (tmp.join(format!("data-whole-{i}")), tmp.join(format!("whole-{i}.txt")));
        copy_dir(&tmp.join("data"), &data);
        let mut cmd = Command::new(&bin);
        if let Some(stub) = &stub {
            cmd.env("WAYLAND_DISPLAY", &stub.path);
        }
        if picture != "1240x1080" {
            cmd.env("POCKETUI_SNAPSHOT_SIZE", picture);
        }
        let out = cmd
            .args(["--snapshot", state, tmp.join(format!("whole-{i}.png")).to_str().unwrap()])
            .envs([("POCKETUI_MOCK", "1"), ("POCKETUI_LANG", lang), ("POCKETUI_TEXT_SIZE", "large"), ("POCKETUI_RETRO", "cocoa")])
            .env("POCKETUI_SNAPSHOT_GAME", format!("snes|{game}|game.sfc"))
            .env("POCKETUI_VISIBLE_TEXT", &seen)
            .env("POCKETUI_ES_SYSTEMS", tmp.join("es_systems.cfg"))
            .env("POCKETUI_DATA", &data)
            .output()
            .expect("run pocket-ui");
        let seen = std::fs::read_to_string(&seen).unwrap_or_default();
        if !out.status.success() || !seen.lines().any(|l| l.trim() == want) {
            found.push(format!("{state} ({picture}, {lang}): \"{want}\" not whole on the picture, which shows:\n{seen}"));
        }
    }
    drop(stub);
    found.sort();
    assert!(found.is_empty(), "{} problems in Retro's pause menu (O overlap or a frame's line, T touch, E side edge, V top/bottom edge, or a name cut short):\n{}", found.len(), found.join("\n"));
}

/// Help & tools → Test buttons in Retro, as the Mini V2 showed it (06.11.2026): before the first press the second hint,
/// "…what it does in menus and in games.", ran past the last-press panel's frame and off the screen ("s." was gone), in
/// Medium English; Barlow's narrower letters just fitted. Every case of the page in Large text, English, German and
/// Spanish at 1240x1080, 640x480 and 1024x768, plus the Mini V2's own Medium English: nothing may run into another string,
/// the hint bar, the panel's frame (`retro::frame_lines_are_no_text`) or an edge, and the hint's last words show whole.
#[test]
fn retro_test_buttons_page_keeps_its_hints_inside_the_panel() {
    let _tmp_guard = tempfile::tempdir().unwrap();
    let tmp = _tmp_guard.path().to_path_buf();
    fake_library(&tmp);
    let bin = PathBuf::from(env!("CARGO_BIN_EXE_pocket-ui"));
    let stub = wayland(&tmp);
    let states = ["test-buttons:idle", "test-buttons", "test-buttons:bt", "test-buttons:two", "test-buttons:remap", "test-buttons:hold", "test-buttons:unknown", "test-buttons:empty"];
    let retro = [("POCKETUI_RETRO", "cocoa")];
    let mut found = Vec::new();
    for picture in [None, Some("640x480"), Some("1024x768")] {
        let mut jobs = Vec::new();
        for lang in ["en", "de", "es"] {
            for (i, state) in states.iter().enumerate() {
                jobs.push((format!("retro-tb-{}-{lang}-{i}", picture.unwrap_or("native")), "large", lang, false, state.to_string()));
            }
        }
        if picture.is_none() {
            jobs.push(("retro-tb-native-en-medium".to_string(), "medium", "en", false, "test-buttons:idle".to_string()));
        }
        found.extend(render_all_in(&tmp, &bin, stub.as_ref(), jobs, picture, "", &retro).into_iter().map(|f| format!("{}: {f}", picture.unwrap_or("1240x1080"))));
    }
    // The hint whole: its last line ends with the sentence's last word.
    for (i, (picture, lang, size, end)) in [("1240x1080", "en", "medium", "in games."), ("640x480", "de", "large", "bewirkt."), ("1024x768", "es", "large", "juegos.")].into_iter().enumerate() {
        let (data, seen) = (tmp.join(format!("data-tb-whole-{i}")), tmp.join(format!("tb-whole-{i}.txt")));
        copy_dir(&tmp.join("data"), &data);
        let mut cmd = Command::new(&bin);
        if let Some(stub) = &stub {
            cmd.env("WAYLAND_DISPLAY", &stub.path);
        }
        if picture != "1240x1080" {
            cmd.env("POCKETUI_SNAPSHOT_SIZE", picture);
        }
        let out = cmd
            .args(["--snapshot", "test-buttons:idle", tmp.join(format!("tb-whole-{i}.png")).to_str().unwrap()])
            .envs([("POCKETUI_MOCK", "1"), ("POCKETUI_LANG", lang), ("POCKETUI_TEXT_SIZE", size), ("POCKETUI_RETRO", "cocoa")])
            .env("POCKETUI_VISIBLE_TEXT", &seen)
            .env("POCKETUI_ES_SYSTEMS", tmp.join("es_systems.cfg"))
            .env("POCKETUI_DATA", &data)
            .output()
            .expect("run pocket-ui");
        let seen = std::fs::read_to_string(&seen).unwrap_or_default();
        if !out.status.success() || !seen.lines().any(|l| l.trim().ends_with(end)) {
            found.push(format!("test-buttons:idle ({picture}, {lang}, {size}): no line ends with \"{end}\"; the picture shows:\n{seen}"));
        }
    }
    drop(stub);
    found.sort();
    assert!(found.is_empty(), "{} problems on Retro's Test buttons page (O overlap or the panel's frame, T touch, E side edge, V top/bottom edge, or a hint cut short):\n{}", found.len(), found.join("\n"));
}

/// 1024x768 is the shortest screen with a big type: in Large text no heading, line or choice may be
/// cut by the picture's top or bottom edge (`V`: Leave's four choices, the status picker's heading, a keyboard's note
/// were off the picture). The other findings of that size are the size sweep's (scripts/size-sweep.sh).
#[test]
fn at_1024x768_in_large_text_nothing_is_cut_by_the_top_or_bottom_of_the_picture() {
    let tmp = tempfile::tempdir().unwrap();
    fake_library(tmp.path());
    let bin = PathBuf::from(env!("CARGO_BIN_EXE_pocket-ui"));
    let stub = wayland(tmp.path());
    let jobs: Vec<_> = states().into_iter().enumerate().map(|(i, (safe, state))| (format!("brick-{i}"), "large", "en", safe, state)).collect();
    let mut found = render_all(tmp.path(), &bin, stub.as_ref(), jobs, Some("1024x768"), "V");
    drop(stub);
    found.sort();
    assert!(found.is_empty(), "{} strings cut by the top or bottom of a 1024x768 picture in Large text:\n{}", found.len(), found.join("\n"));
}

/// The Brick's default: 1024x768 in Normal text. Every state of tests/i18n-states.txt (English) must show no string that
/// runs into another or into a hint bar (`O`), is cut by the top or bottom of the picture (`V`), is cut by the side edge of
/// what it is painted into (`E`) or is drawn past the right edge of the picture (`X`). A cut with "…" (`C`) is the
/// size sweep's to read (scripts/size-sweep.sh): spines and file names are cut on purpose.
#[test]
fn at_1024x768_in_normal_text_nothing_overlaps_or_runs_off_the_picture() {
    let tmp = tempfile::tempdir().unwrap();
    fake_library(tmp.path());
    let bin = PathBuf::from(env!("CARGO_BIN_EXE_pocket-ui"));
    let stub = wayland(tmp.path());
    let jobs: Vec<_> = states().into_iter().enumerate().map(|(i, (safe, state))| (format!("brick-normal-{i}"), "normal", "en", safe, state)).collect();
    let mut found = render_all(tmp.path(), &bin, stub.as_ref(), jobs, Some("1024x768"), "OVEX");
    drop(stub);
    found.sort();
    assert!(found.is_empty(), "{} problems at 1024x768 in Normal text (O: runs into another string or a hint bar, V: cut by the top or bottom edge, E: cut by the side edge, X: past the right edge):\n{}", found.len(), found.join("\n"));
}

/// Native 1024x768 outputs do not use the H700's larger logical canvas. A two-line save note must still clear the keys.
#[test]
fn save_preview_notes_stay_above_the_hints_on_native_and_small_outputs() {
    let tmp = tempfile::tempdir().unwrap();
    fake_library(tmp.path());
    let stub = wayland(tmp.path());
    // Quiet fake data only; no backend or quantum changes are needed for a layout regression.
    std::fs::write(tmp.path().join("data/sounds.json"), r#"{"on":false,"volume":0}"#).unwrap();
    for dimensions in ["1024x768", "640x480"] {
        for size in ["normal", "large"] {
            for lang in ["en", "de"] {
                for state in ["slot-save:psx", "slot-load:snes", "slot-auto", "slot-undo", "slot-restart"] {
                    let boxes = tmp.path().join("preview.box");
                    let png = tmp.path().join("preview.png");
                    let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
                    cmd.args(["--snapshot", state, png.to_str().unwrap()])
                        .env("POCKETUI_MOCK", "1").env("POCKETUI_SOUND_QUANTUM", "0")
                        .env("POCKETUI_ES_SYSTEMS", tmp.path().join("es_systems.cfg"))
                        .env("POCKETUI_DATA", tmp.path().join("data"))
                        .env("POCKETUI_SNAPSHOT_SIZE", dimensions).env("POCKETUI_TEXT_SIZE", size)
                        .env("POCKETUI_LANG", lang).env("POCKETUI_TEXT_BOXES", &boxes);
                    if let Some(s) = &stub { cmd.env("WAYLAND_DISPLAY", &s.path); }
                    let out = cmd.output().unwrap();
                    assert!(out.status.success(), "{state} {dimensions} {size} {lang}: {}", String::from_utf8_lossy(&out.stderr));
                    let problems = std::fs::read_to_string(&boxes).unwrap();
                    assert!(problems.trim().is_empty(), "{state} {dimensions} {size} {lang}: {problems}");
                }
            }
        }
    }
}

/// Native 1024x768 is shorter than the pages' design (960 and more): the sweep of 2026-11-02 found these pages with a
/// string lying on the hints or on the text above it. In English, German and Spanish none of them may do so any more.
#[test]
fn pages_found_overlapping_at_1024x768_keep_their_lines_apart() {
    let tmp = tempfile::tempdir().unwrap();
    fake_library(tmp.path());
    let stub = wayland(tmp.path());
    std::fs::write(tmp.path().join("data/sounds.json"), r#"{"on":false,"volume":0}"#).unwrap();
    let states = [
        "setup:extras", "setup:extras:da", "setup:play", "setup:card=lens", "tailscale:connected", "web:off", "web:waiting",
        "achievements:offline", "sys:time-wrong-wifi", "search:none:qqqq", "curator:history", "curator:history:year",
        "battery:3:samples", "port-perf:own", "port-perf:busy", "pc-streaming:pin", "slot-undo",
    ];
    for lang in ["en", "de", "es"] {
        for state in states {
            let boxes = tmp.path().join("pages.box");
            let png = tmp.path().join("pages.png");
            let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
            cmd.args(["--snapshot", state, png.to_str().unwrap()])
                .env("POCKETUI_MOCK", "1").env("POCKETUI_SOUND_QUANTUM", "0")
                .env("POCKETUI_ES_SYSTEMS", tmp.path().join("es_systems.cfg"))
                .env("POCKETUI_DATA", tmp.path().join("data"))
                .env("POCKETUI_SNAPSHOT_SIZE", "1024x768").env("POCKETUI_TEXT_SIZE", "normal")
                .env("POCKETUI_LANG", lang).env("POCKETUI_TEXT_BOXES", &boxes);
            if let Some(s) = &stub { cmd.env("WAYLAND_DISPLAY", &s.path); }
            let out = cmd.output().unwrap();
            assert!(out.status.success(), "{state} {lang}: {}", String::from_utf8_lossy(&out.stderr));
            let overlaps: Vec<String> = std::fs::read_to_string(&boxes).unwrap().lines().filter(|l| l.starts_with("O\t")).map(str::to_string).collect();
            assert!(overlaps.is_empty(), "{state} {lang}: {overlaps:?}");
        }
    }
}

/// Hiding a label keeps its key and the space between keys: the final START must remain inside the output.
#[test]
fn shelf_hint_keys_stay_on_screen_when_the_last_labels_are_hidden() {
    let tmp = tempfile::tempdir().unwrap();
    fake_library(tmp.path());
    let stub = wayland(tmp.path());
    std::fs::write(tmp.path().join("data/sounds.json"), r#"{"on":false,"volume":0}"#).unwrap();
    for dimensions in ["1024x768", "640x480"] {
        for size in ["normal", "large"] {
            for lang in ["en", "de"] {
                for appearance in ["dark", "light"] {
                    let settings_file = tmp.path().join("data/settings.json");
                    let mut settings: serde_json::Value = serde_json::from_slice(&std::fs::read(&settings_file).unwrap()).unwrap();
                    settings["appearance"] = appearance.into();
                    std::fs::write(settings_file, serde_json::to_vec(&settings).unwrap()).unwrap();
                    let boxes = tmp.path().join("hints.box");
                    let png = tmp.path().join("hints.png");
                    let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
                    cmd.args(["--snapshot", "shelf:snes:shelf:games:0", png.to_str().unwrap()])
                        .env("POCKETUI_MOCK", "1").env("POCKETUI_SOUND_QUANTUM", "0")
                        .env("POCKETUI_ES_SYSTEMS", tmp.path().join("es_systems.cfg"))
                        .env("POCKETUI_DATA", tmp.path().join("data"))
                        .env("POCKETUI_SNAPSHOT_SIZE", dimensions).env("POCKETUI_TEXT_SIZE", size)
                        .env("POCKETUI_LANG", lang).env("POCKETUI_TEXT_BOXES", &boxes);
                    if let Some(s) = &stub { cmd.env("WAYLAND_DISPLAY", &s.path); }
                    let out = cmd.output().unwrap();
                    assert!(out.status.success(), "{dimensions} {size} {lang} {appearance}: {}", String::from_utf8_lossy(&out.stderr));
                    let problems = std::fs::read_to_string(&boxes).unwrap();
                    assert!(problems.trim().is_empty(), "{dimensions} {size} {lang} {appearance}: {problems}");
                }
            }
        }
    }
}

/// Five built-in stickers plus the player's own: both ends of the scrolling sheet clear the hint bar.
#[test]
fn sticker_sheets_fit_short_outputs_and_scroll_to_the_last_choice() {
    let tmp = tempfile::tempdir().unwrap();
    fake_library(tmp.path());
    let stub = wayland(tmp.path());
    std::fs::write(tmp.path().join("data/sounds.json"), r#"{"on":false,"volume":0}"#).unwrap();
    for dimensions in ["1280x720", "1024x768", "640x480"] {
        for size in ["normal", "large"] {
            for lang in ["en", "de"] {
                for keys in ["", "dddddddd"] {
                    let state = format!("curator:stickers:snes:chrono:{keys}");
                    let boxes = tmp.path().join("stickers.box");
                    let png = tmp.path().join("stickers.png");
                    let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
                    cmd.args(["--snapshot", &state, png.to_str().unwrap()])
                        .env("POCKETUI_MOCK", "1").env("POCKETUI_SOUND_QUANTUM", "0")
                        .env("POCKETUI_ES_SYSTEMS", tmp.path().join("es_systems.cfg"))
                        .env("POCKETUI_DATA", tmp.path().join("data"))
                        .env("POCKETUI_SNAPSHOT_SIZE", dimensions).env("POCKETUI_TEXT_SIZE", size)
                        .env("POCKETUI_LANG", lang).env("POCKETUI_TEXT_BOXES", &boxes);
                    if let Some(s) = &stub { cmd.env("WAYLAND_DISPLAY", &s.path); }
                    let out = cmd.output().unwrap();
                    assert!(out.status.success(), "{state} {dimensions} {size} {lang}: {}", String::from_utf8_lossy(&out.stderr));
                    let problems = std::fs::read_to_string(&boxes).unwrap();
                    assert!(problems.trim().is_empty(), "{state} {dimensions} {size} {lang}: {problems}");
                }
            }
        }
    }
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
        // Look's seventh row, Text size, selected: its value shows as "‹ Groß ›" or "‹ Normal ›".
        cmd.args(["--snapshot", "settings:0:6", tmp.join(format!("{name}.png")).to_str().unwrap()])
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
    assert!(medium.contains("O\tSchriftgröße") && medium.contains("Mittel ›") && !medium.contains("Groß ›"), "Medium chosen:\n{medium}");
    let normal = shot(r#"{"onboarded": true}"#, "normal");
    assert!(normal.contains("O\tSchriftgröße") && !normal.contains("Groß ›"), "nothing chosen: Normal\n{normal}");
    drop(stub);
}

/// New-user test 2026-10-30: in German with Large text only three of the Settings categories fit and nothing said
/// there were more. Since the categories are bare one-line rows (NextUI's idea) all seven fit at every text size: no
/// scroll bar at Large or Normal (`kit::scrollbar` would draw one the moment they did not).
#[test]
fn settings_categories_all_fit_without_a_scroll_bar() {
    let _tmp_guard = tempfile::tempdir().unwrap();
    let tmp = _tmp_guard.path().to_path_buf();
    let stub = wayland(&tmp);
    let edge_ink = |size: &str| -> usize {
        let data = tmp.join(format!("data-{size}"));
        std::fs::create_dir_all(&data).unwrap();
        std::fs::write(data.join("settings.json"), format!(r#"{{"onboarded": true, "text_size": "{size}"}}"#)).unwrap();
        let png = tmp.join(format!("groups-{size}.png"));
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
        cmd.args(["--snapshot", "settings", png.to_str().unwrap()])
            .env("POCKETUI_MOCK", "1")
            .env("POCKETUI_LANG", "de")
            .env_remove("POCKETUI_TEXT_SIZE")
            .env("POCKETUI_ES_SYSTEMS", tmp.join("none.cfg"))
            .env("POCKETUI_DATA", &data);
        if let Some(s) = &stub {
            cmd.env("WAYLAND_DISPLAY", &s.path);
        }
        let out = cmd.output().expect("run pocket-ui");
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        let img = pocket_ui::image::load_png(&png).expect("snapshot written");
        // The 4 px track starts 12 px from the edge, below the status pill and above the hint bar.
        let (x0, rows) = (img.w as usize - 12, 120..(img.h as usize / 2));
        rows.map(|y| (x0..x0 + 4).filter(|&x| img.px[y * img.w as usize + x] & 0xffffff != 0).count()).sum()
    };
    assert_eq!(edge_ink("large"), 0, "Large text in German: all seven fit, no bar");
    assert_eq!(edge_ink("normal"), 0, "Normal text: all seven fit, no bar");
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

/// A real Brick, 2026-11-06: on the first start's "Your games" the hint bar lay on top of the third row ("Box art"), at
/// 1024x768 with 28 games found. At the sizes of every handheld (1024x768 and 640x480 are painted from 768 and 960 lines) and
/// in Normal and Large text (Easy to read), in English, German and Spanish, with and without games and with the selection
/// on Box art (its line over the rows), every row is on the picture, whole, and no text runs into the hint bar.
#[test]
fn your_games_rows_stay_above_the_hint_bar_on_the_small_screens() {
    let tmp = tempfile::tempdir().unwrap();
    fake_library(tmp.path());
    let stub = wayland(tmp.path());
    std::fs::write(tmp.path().join("data/sounds.json"), r#"{"on":false,"volume":0}"#).unwrap();
    // A card with no games at all: the page says how to add some.
    let empty = tmp.path().join("empty-systems.cfg");
    std::fs::write(&empty, "<systemList>\n</systemList>\n").unwrap();
    let rows = [
        ("en", ["Continue", "Add more…", "Box art"]),
        ("de", ["Fortsetzen", "Mehr hinzufügen …", "Cover"]),
        ("es", ["Continuar", "Añadir más…", "Portada"]),
    ];
    let empty_rows = [
        ("en", ["Add games…", "I'll add games later", "Box art"]),
        ("de", ["Spiele hinzufügen …", "Spiele füge ich später hinzu", "Cover"]),
        ("es", ["Añadir juegos…", "Añadiré juegos más tarde", "Portada"]),
    ];
    let mut found = Vec::new();
    for dimensions in ["1024x768", "640x480"] {
        for size in ["normal", "large"] {
            for (library, table) in [(true, &rows), (false, &empty_rows)] {
                for (lang, names) in table {
                    // The first row selected, then Box art (Down twice): the line over the rows comes and goes.
                    for keys in ["", "dd"] {
                        let seen = tmp.path().join("games.txt");
                        let boxes = tmp.path().join("games.box");
                        let png = tmp.path().join("games.png");
                        let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
                        cmd.args(["--snapshot", &format!("setup:first-games:{keys}"), png.to_str().unwrap()])
                            .env("POCKETUI_MOCK", "1").env("POCKETUI_SOUND_QUANTUM", "0")
                            .env("POCKETUI_ES_SYSTEMS", if library { tmp.path().join("es_systems.cfg") } else { empty.clone() })
                            .env("POCKETUI_DATA", tmp.path().join("data"))
                            .env("POCKETUI_SNAPSHOT_SIZE", dimensions).env("POCKETUI_TEXT_SIZE", size)
                            .env("POCKETUI_LANG", lang).env("POCKETUI_TEXT_BOXES", &boxes).env("POCKETUI_VISIBLE_TEXT", &seen);
                        if let Some(s) = &stub { cmd.env("WAYLAND_DISPLAY", &s.path); }
                        let out = cmd.output().unwrap();
                        let which = format!("{dimensions} {size} {lang} {} keys \"{keys}\"", if library { "with games" } else { "without games" });
                        assert!(out.status.success(), "{which}: {}", String::from_utf8_lossy(&out.stderr));
                        let visible = std::fs::read_to_string(&seen).unwrap_or_default();
                        for name in names {
                            if !visible.lines().any(|l| l.trim() == *name) {
                                found.push(format!("{which}: the row \"{name}\" is not on the picture, which shows:\n{visible}"));
                            }
                        }
                        let problems = std::fs::read_to_string(&boxes).unwrap_or_default();
                        found.extend(problems.lines().filter(|l| l.starts_with("O\t") || l.starts_with("V\t")).map(|l| format!("{which}: {}", l.replace('\t', " | "))));
                    }
                }
            }
        }
    }
    drop(stub);
    assert!(found.is_empty(), "{} problems on \"Your games\" (a row missing, text under the hint bar or cut by the picture's edge):\n{}", found.len(), found.join("\n"));
}
