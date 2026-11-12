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
#[path = "common/snapshot.rs"]
mod snapshot;
use snapshot::SnapshotCommand;

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
    let status = Command::new("python3").env("POCKETUI_MOCK_NOW", FIXTURE_NOW)
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
    render_all_in(tmp, bin, stub, jobs, picture, only, &[], false)
}

/// `render_all` with more of the environment set for every snapshot (`POCKETUI_RETRO=cocoa`: Settings → Look → Style →
/// Retro in that palette).
fn render_all_in(tmp: &Path, bin: &Path, stub: Option<&Stub>, jobs: Vec<(String, &'static str, &'static str, bool, String)>, picture: Option<&str>, only: &str, env: &[(&str, &str)], reuse: bool) -> Vec<String> {
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
                cmd
                    .args(["--snapshot", &state, png.to_str().unwrap()])
                    .env("POCKETUI_MOCK", "1").env("POCKETUI_MOCK_NOW", FIXTURE_NOW)
                    .env("POCKETUI_LANG", lang)
                    .env("POCKETUI_TEXT_SIZE", size)
                    .env("POCKETUI_TEXT_BOXES", &boxes)
                    .env("POCKETUI_I18N_MISSES", &misses)
                    .env("POCKETUI_ES_SYSTEMS", tmp.join("es_systems.cfg"))
                    .env("POCKETUI_DATA", &data);
                let out = if reuse { cmd.snapshot_output() } else { cmd.output() }.expect("run pocket-ui");
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
    // In both of Retro's letters (0.3.2): Departure Mono, the default, and Pixelify Sans.
    for letters in RETRO_LETTERS {
        for picture in [None, Some("640x480")] {
            let jobs = states().into_iter().enumerate().map(|(i, (safe, state))| (format!("retro-{letters}-{}-{i}", picture.unwrap_or("native")), "large", "de", safe, state)).collect();
            let env = [("POCKETUI_RETRO", "cocoa"), ("POCKETUI_RETRO_LETTERS", letters)];
            found.extend(render_all_in(&tmp, &bin, stub.as_ref(), jobs, picture, "", &env, false).into_iter().map(|f| format!("{letters} {}: {f}", picture.unwrap_or("1240x1080"))));
        }
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
        found.extend(render_all_in(&tmp, &bin, stub.as_ref(), jobs, Some(picture), "", &[("POCKETUI_RETRO", "cocoa")], false).into_iter().map(|f| format!("{picture}: {f}")));
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
    let mut found = Vec::new();
    // In both of Retro's letters (0.3.2).
    for letters in RETRO_LETTERS {
        let game = [("POCKETUI_RETRO", "cocoa"), ("POCKETUI_RETRO_LETTERS", letters), ("POCKETUI_SNAPSHOT_GAME", "snes|Donkey Kong Country 3: Dixie Kong's Double Trouble!|dkc3.sfc")];
        for picture in [None, Some("640x480"), Some("1024x768")] {
            let mut jobs = Vec::new();
            for lang in ["de", "es"] {
                for (i, state) in states.iter().enumerate() {
                    jobs.push((format!("retro-pause-{letters}-{}-{lang}-{i}", picture.unwrap_or("native")), "large", lang, false, state.to_string()));
                }
            }
            found.extend(render_all_in(&tmp, &bin, stub.as_ref(), jobs, picture, "", &game, false).into_iter().map(|f| format!("{letters} {}: {f}", picture.unwrap_or("1240x1080"))));
        }
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
    for (i, (letters, (picture, lang, state, game, want))) in RETRO_LETTERS.into_iter().flat_map(|l| whole.into_iter().map(move |w| (l, w))).enumerate() {
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
            .env("POCKETUI_MOCK_NOW", FIXTURE_NOW).envs([("POCKETUI_MOCK", "1"), ("POCKETUI_LANG", lang), ("POCKETUI_TEXT_SIZE", "large"), ("POCKETUI_RETRO", "cocoa"), ("POCKETUI_RETRO_LETTERS", letters)])
            .env("POCKETUI_SNAPSHOT_GAME", format!("snes|{game}|game.sfc"))
            .env("POCKETUI_VISIBLE_TEXT", &seen)
            .env("POCKETUI_ES_SYSTEMS", tmp.join("es_systems.cfg"))
            .env("POCKETUI_DATA", &data)
            .output()
            .expect("run pocket-ui");
        let seen = std::fs::read_to_string(&seen).unwrap_or_default();
        if !out.status.success() || !seen.lines().any(|l| l.trim() == want) {
            found.push(format!("{letters} {state} ({picture}, {lang}): \"{want}\" not whole on the picture, which shows:\n{seen}"));
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
        found.extend(render_all_in(&tmp, &bin, stub.as_ref(), jobs, picture, "", &retro, false).into_iter().map(|f| format!("{}: {f}", picture.unwrap_or("1240x1080"))));
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
            .env("POCKETUI_MOCK_NOW", FIXTURE_NOW).envs([("POCKETUI_MOCK", "1"), ("POCKETUI_LANG", lang), ("POCKETUI_TEXT_SIZE", size), ("POCKETUI_RETRO", "cocoa")])
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
                        .env("POCKETUI_MOCK", "1").env("POCKETUI_MOCK_NOW", FIXTURE_NOW).env("POCKETUI_SOUND_QUANTUM", "0")
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
                .env("POCKETUI_MOCK", "1").env("POCKETUI_MOCK_NOW", FIXTURE_NOW).env("POCKETUI_SOUND_QUANTUM", "0")
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
                        .env("POCKETUI_MOCK", "1").env("POCKETUI_MOCK_NOW", FIXTURE_NOW).env("POCKETUI_SOUND_QUANTUM", "0")
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
                        .env("POCKETUI_MOCK", "1").env("POCKETUI_MOCK_NOW", FIXTURE_NOW).env("POCKETUI_SOUND_QUANTUM", "0")
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
            .env("POCKETUI_MOCK", "1").env("POCKETUI_MOCK_NOW", FIXTURE_NOW)
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
            .env("POCKETUI_MOCK", "1").env("POCKETUI_MOCK_NOW", FIXTURE_NOW)
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
        let status = Command::new("python3").env("POCKETUI_MOCK_NOW", FIXTURE_NOW)
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
                            .env("POCKETUI_MOCK", "1").env("POCKETUI_MOCK_NOW", FIXTURE_NOW).env("POCKETUI_LANG", lang)
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
/// in Normal and Large text (Big text), in English, German and Spanish, with and without games and with the selection
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
                            .env("POCKETUI_MOCK", "1").env("POCKETUI_MOCK_NOW", FIXTURE_NOW).env("POCKETUI_SOUND_QUANTUM", "0")
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

/// The looks every text-fit check below goes through: Barlow with Retro off and on, and Retro in the pixel font it ships
/// with (the widest one: about a fifth wider than Barlow).
// These two independent bug guards paint the same first-start cells. Share only
// their pictures; both keep all their assertions. Other matrices proved unique
// or assert write-back behavior, so they execute the renderer directly.
const FIRST_START_STATES: &[&str] = &["setup:first", "setup:first:device=rp5", "setup:first:device=mini",
    "setup:first:device=nova", "setup:first:device=rp6", "setup:first:device=odin2mini"];

/// Retro's letters (`POCKETUI_RETRO_LETTERS`, 0.3.2): Departure Mono, the default, and Pixelify Sans. The other Retro
/// runs paint the default; `POCKETUI_RETRO_LETTERS=pixelify cargo test --test text_size` runs the whole file in Pixelify.
const RETRO_LETTERS: [&str; 2] = ["departure", "pixelify"];
const BARLOW_LOOKS: [(&str, &str); 2] = [("off", "barlow"), ("cocoa", "barlow")];
const ALL_LOOKS: [(&str, &str); 3] = [("off", "barlow"), ("cocoa", "barlow"), ("cocoa", "retro")];

/// Reuse the text painter's edge/overlap checks for the 0.3.1 sweep's small fixes, at every requested corner.
fn check_settings_setup_textfit(states: &[&str]) {
    check_settings_setup_textfit_faults(states, "OVETX");
}

fn check_settings_setup_textfit_faults(states: &[&str], faults: &str) {
    check_textfit_in(states, faults, &BARLOW_LOOKS);
}

fn check_textfit_in(states: &[&str], faults: &str, looks: &[(&str, &str)]) {
    check_textfit_jobs(states, faults, looks, false);
}

/// `check_textfit_in`, with the states as safe mode (`--safe`) when `safe`.
fn check_textfit_jobs(states: &[&str], faults: &str, looks: &[(&str, &str)], safe: bool) {
    let tmp = tempfile::tempdir().unwrap();
    fake_library(tmp.path());
    let bin = std::env::var_os("POCKETUI_NAV_BINARY").map_or_else(|| PathBuf::from(env!("CARGO_BIN_EXE_pocket-ui")), PathBuf::from);
    let stub = wayland(tmp.path());
    let mut found = Vec::new();
    for (style, font) in looks {
        for picture in ["640x480", "1024x768", "1240x1080", "1280x720"] {
            let mut jobs = Vec::new();
            for size in ["normal", "large"] {
                for lang in ["en", "de", "es"] {
                    for (i, state) in states.iter().enumerate() {
                        jobs.push((format!("textfit-031-{style}-{font}-{picture}-{size}-{lang}-{i}"), size, lang, safe, state.to_string()));
                    }
                }
            }
            found.extend(render_all_in(tmp.path(), &bin, stub.as_ref(), jobs, Some(picture), faults,
                &[("POCKETUI_RETRO", style), ("POCKETUI_FONT", font)], states == FIRST_START_STATES)
                .into_iter().map(|f| format!("{style}/{font} {picture}: {f}")));
            // A heading made into an ellipsis is still a lost label, even though it no longer crosses the edge.
            for record in std::fs::read_dir(tmp.path()).unwrap().flatten().filter(|e| e.path().extension().is_some_and(|e| e == "tsv") && e.file_name().to_string_lossy().starts_with(&format!("textfit-031-{style}-{font}-{picture}-"))) {
                let text = std::fs::read_to_string(record.path()).unwrap();
                for heading in ["Welcome to your Pocket", "Copy your game files", "Other ways online",
                    "Any game, by its name", "Type part of a game's name.", "Other games in Burnout, in your library"] {
                    if text.lines().any(|l| l == format!("C\t{heading}")) {
                        found.push(format!("{}: heading cut short: {heading}", record.file_name().to_string_lossy()));
                    }
                }
            }
        }
    }
    found.sort();
    assert!(found.is_empty(), "Text clipped on a settings/setup page:\n{}", found.join("\n"));
}

/// Retro/Large/Spanish cut the missing-lights sentence at the screen's right edge.
#[test]
fn textfit_031_missing_lights_note_stays_inside_the_page() {
    check_settings_setup_textfit(&["leds:off:0", "leds:colour:1", "leds:colour-game:2", "leds:battery:0"]);
}

/// The Spanish and German translations of "Copy your game files" ran off their page at Large.
#[test]
fn textfit_031_port_files_heading_stays_inside_the_page() {
    check_settings_setup_textfit(&["port-files:on:wargus.zip", "port-files:off:wargus.zip:6", "setup:ports-shared:rx"]);
}

/// The captive portal's "Other ways online" heading lost its end in Retro/Large/Spanish.
#[test]
fn textfit_031_other_ways_heading_stays_inside_the_page() {
    check_settings_setup_textfit(&["setup:wifi-portal-sheet:da", "setup:wifi-portal-sheet-nomac:da"]);
}

/// The Players page's Spanish instruction lost its last words at the right edge in Retro/Large.
#[test]
fn textfit_031_players_instruction_stays_inside_the_page() {
    check_settings_setup_textfit(&["players:two", "players:promoted", "players:gap", "players:four", "players:five"]);
}


#[test]
fn textfit_031_welcome_heading_stays_inside_the_page() {
    check_settings_setup_textfit_faults(FIRST_START_STATES, "EX");
}

#[test]
fn textfit_031_empty_search_hints_stay_inside_the_page() {
    check_settings_setup_textfit(&["search:fresh", "search:empty"]);
}

#[test]
fn textfit_031_n64_controls_note_stays_inside_the_page() {
    check_settings_setup_textfit(&["controls-card:n64"]);
}

#[test]
fn textfit_031_console_launch_price_label_stays_inside_the_page() {
    check_settings_setup_textfit(&["console:psx:6"]);
}

/// A known system name must show whole, or use a known short system name; dots in the name lose its identity.
fn check_system_names(states: &[(&str, &str, &str)], themes: &[&str]) {
    let tmp = tempfile::tempdir().unwrap();
    fake_library(tmp.path());
    // Keep NES visible but unselected at every height: selected rows can marquee, and the Split
    // panel names the selected system too. A two-system fixture isolates the list label itself.
    if states.iter().any(|(state, _, _)| *state == "home") {
        let cfg = tmp.path().join("es_systems.cfg");
        let source = std::fs::read_to_string(&cfg).unwrap();
        let systems: String = source.split("<system>")
            .filter(|s| s.starts_with("<name>nes</name>") || s.starts_with("<name>psx</name>"))
            .map(|s| format!("<system>{}</system>", s.split("</system>").next().unwrap())).collect();
        std::fs::write(cfg, format!("<systemList>{systems}</systemList>")).unwrap();
    }
    let stub = wayland(tmp.path());
    let bin = std::env::var_os("POCKETUI_NAV_BINARY").map_or_else(|| PathBuf::from(env!("CARGO_BIN_EXE_pocket-ui")), PathBuf::from);
    for theme in themes {
        std::fs::write(tmp.path().join("data/settings.json"), format!(r#"{{"onboarded":true,"theme":"{theme}"}}"#)).unwrap();
        for dimensions in ["1280x720", "1024x768", "640x480", "1240x1080"] {
            for style in ["off", "cocoa"] {
                for size in ["normal", "large"] {
                    for lang in ["en", "de", "es"] {
                        for (state, full, short) in states {
                            let visible = tmp.path().join("name.txt");
                            let misses = tmp.path().join("name.tsv");
                            let _ = std::fs::remove_file(&misses);
                            let mut cmd = Command::new(&bin);
                            cmd.args(["--snapshot", state, tmp.path().join("name.png").to_str().unwrap()])
                                .env("POCKETUI_MOCK", "1").env("POCKETUI_SNAPSHOT_SIZE", dimensions)
                                .env("POCKETUI_RETRO", style).env("POCKETUI_FONT", "barlow")
                                .env("POCKETUI_TEXT_SIZE", size).env("POCKETUI_LANG", lang)
                                .env("POCKETUI_VISIBLE_TEXT", &visible).env("POCKETUI_I18N_MISSES", &misses)
                                .env("POCKETUI_ES_SYSTEMS", tmp.path().join("es_systems.cfg"))
                                .env("POCKETUI_DATA", tmp.path().join("data"));
                            if let Some(s) = &stub { cmd.env("WAYLAND_DISPLAY", &s.path); }
                            let out = cmd.output().unwrap();
                            assert!(out.status.success(), "{state}: {}", String::from_utf8_lossy(&out.stderr));
                            let seen = std::fs::read_to_string(&visible).unwrap();
                            assert!(seen.lines().any(|l| l == *full || l == *short),
                                "{state} {theme} {dimensions} {style} {size} {lang}: neither whole name nor short name:\n{seen}");
                            let cuts = std::fs::read_to_string(&misses).unwrap_or_default();
                            assert!(!cuts.lines().any(|l| l == format!("C\t{full}")),
                                "{state} {theme} {dimensions} {style} {size} {lang}: system name cut short");
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn textfit_031_system_rows_keep_known_names_whole() {
    check_system_names(&[("home", "Nintendo Entertainment System", "NES")], &["split"]);
}

#[test]
fn textfit_031_console_system_heading_keeps_a_whole_name() {
    check_system_names(&[("console:snes", "Super Nintendo Entertainment System", "SNES"),
        ("console:nes", "Nintendo Entertainment System", "NES")], &["split"]);
}

/// The full info page's series heading ended in dots in Retro/Large/Spanish.
#[test]
fn textfit_031_other_games_in_series_heading_stays_whole() {
    check_settings_setup_textfit(&["curator:card:ps2:revenge:dddddddddd"]);
}

/// 0.3.1 (rest): Settings › Tailscale at Large text on a 1024x768 screen drew the help under the selected row's line and the
/// facts of a connection into each other. Every state of the page, in every look.
#[test]
fn textfit_rest_031_tailscale_help_stays_above_the_footer() {
    check_textfit_in(&["tailscale", "tailscale:off", "tailscale:loggedout", "tailscale:login", "tailscale:connected", "tailscale:busy",
        "tailscale:key", "tailscale:name", "tailscale:error", "tailscale:offline", "tailscale:offline-loggedout", "tailscale:offline-note"],
        "OVETX", &ALL_LOOKS);
}

/// 0.3.1 (rest): on a handheld nobody has played on, the first start's "not tested yet" paragraph and the third row pushed
/// the rows under the hint bar at 1024x768.
#[test]
fn textfit_rest_031_first_start_with_the_untested_hint_keeps_its_rows_above_the_hints() {
    check_textfit_in(FIRST_START_STATES, "OVETX", &ALL_LOOKS);
}

/// 0.3.1 (rest): safe mode's last row, "Power off", ran off the bottom of the picture in German at Large text; and every row
/// can be the selected one on the page, whole, above the hints.
#[test]
fn textfit_rest_031_safe_mode_rows_are_whole_and_reachable() {
    check_textfit_jobs(&["rescue", "rescue:1", "rescue:2", "rescue:3", "rescue:4", "rescue:3:es", "rescue:2:reset"], "OVETX", &ALL_LOOKS, true);
}

/// 0.3.1 (rest): whichever row of safe mode is selected (Down from the top, as the D-pad does), its whole band is on the
/// page, above the hints: the list shows the window round the selection when the five rows don't all fit.
#[test]
fn textfit_rest_031_every_safe_mode_row_can_be_selected_in_full_view() {
    let tmp = tempfile::tempdir().unwrap();
    fake_library(tmp.path());
    let bin = std::env::var_os("POCKETUI_NAV_BINARY").map_or_else(|| PathBuf::from(env!("CARGO_BIN_EXE_pocket-ui")), PathBuf::from);
    let stub = wayland(tmp.path());
    let mut found = Vec::new();
    for (style, font) in ALL_LOOKS {
        for picture in ["640x480", "1024x768", "1280x720"] {
            for size in ["normal", "large"] {
                for lang in ["en", "de", "es"] {
                    for row in 0..5 {
                        let state = format!("rescue:{row}");
                        let probe = tmp.path().join("rescue.probe");
                        let _ = std::fs::remove_file(&probe);
                        let mut cmd = Command::new(&bin);
                        cmd.args(["--safe", "--snapshot", &state, tmp.path().join("rescue.png").to_str().unwrap()])
                            .env("POCKETUI_MOCK", "1").env("POCKETUI_SNAPSHOT_SIZE", picture).env("POCKETUI_LANG", lang)
                            .env("POCKETUI_TEXT_SIZE", size).env("POCKETUI_RETRO", style).env("POCKETUI_FONT", font)
                            .env("POCKETUI_NAV_PROBE", &probe).env("POCKETUI_DATA", tmp.path().join("data"))
                            .env("POCKETUI_ES_SYSTEMS", tmp.path().join("es_systems.cfg"));
                        if let Some(s) = &stub { cmd.env("WAYLAND_DISPLAY", &s.path); }
                        let out = cmd.output().unwrap();
                        let at = format!("{style}/{font} {picture} {size} {lang} row {row}");
                        assert!(out.status.success(), "{at}: {}", String::from_utf8_lossy(&out.stderr));
                        let record = std::fs::read_to_string(&probe).unwrap_or_default();
                        // "x0 y0 x1 y1 <tab> text": the marks the painters left (`nav_probe`).
                        let rects = |mark: &str| -> Vec<Vec<i32>> {
                            record.lines().filter_map(|l| {
                                let v: Vec<&str> = l.splitn(5, '\t').collect();
                                (v.len() == 5 && v[4].starts_with(mark)).then(|| v[..4].iter().map(|n| n.parse().unwrap_or(0)).collect())
                            }).collect()
                        };
                        let (focus, hints) = (rects("‹nav:focus›"), rects("‹nav:hint›"));
                        match (focus.first(), hints.iter().map(|h| h[1]).min()) {
                            (Some(f), Some(hint_top)) if f[1] >= 0 && f[3] <= hint_top => {}
                            (f, h) => found.push(format!("{at}: selected row {f:?} is not whole above the hints at {h:?}")),
                        }
                    }
                }
            }
        }
    }
    drop(stub);
    assert!(found.is_empty(), "{} safe-mode rows out of view:\n{}", found.len(), found.join("\n"));
}

/// The Brick's larger Normal/Medium minimum must keep the short facts, controls and explanatory lines whole.
/// Reuse the snapshot fixture, visible-text record and overlap record used by the other text-fit regressions.
#[cfg(feature = "brick")]
fn check_brick_031_visible(states: &[&str], expected: &[(&str, &[&str])]) {
    check_brick_031_visible_in(states, expected, true);
}

#[cfg(feature = "brick")]
fn check_brick_031_visible_in(states: &[&str], expected: &[(&str, &[&str])], standalone: bool) {
    check_brick_031_visible_fixture(states, expected, standalone, |_| {});
}

#[cfg(feature = "brick")]
fn check_brick_031_visible_fixture(states: &[&str], expected: &[(&str, &[&str])], standalone: bool, prepare: impl FnOnce(&Path)) {
    let tmp = tempfile::tempdir().unwrap();
    fake_library(tmp.path());
    prepare(tmp.path());
    let stub = wayland(tmp.path());
    let bin = std::env::var_os("POCKETUI_NAV_BINARY").map_or_else(|| PathBuf::from(env!("CARGO_BIN_EXE_pocket-ui")), PathBuf::from);
    let mut found = Vec::new();
    for style in ["barlow", "retro"] {
        for size in ["normal", "medium", "large"] {
            for (lang, words) in expected {
                for state in states {
                    let seen = tmp.path().join("brick-visible.txt");
                    let boxes = tmp.path().join("brick.box");
                    let mut cmd = Command::new(&bin);
                    cmd.args(["--snapshot", state, tmp.path().join("brick.png").to_str().unwrap()])
                        .env("POCKETUI_MOCK", "1").env("POCKETUI_DEVICE", "brick").env("POCKETUI_BRICK_STANDALONE", if standalone { "1" } else { "" })
                        .env("POCKETUI_SOUND_QUANTUM", "0").env("POCKETUI_LANG", lang)
                        .env("POCKETUI_TEXT_SIZE", size).env("POCKETUI_SNAPSHOT_SIZE", "1024x768")
                        .env("POCKETUI_FONT", style).env("POCKETUI_RETRO", if style == "retro" { "cocoa" } else { "off" })
                        .env("POCKETUI_VISIBLE_TEXT", &seen).env("POCKETUI_TEXT_BOXES", &boxes)
                        .env("POCKETUI_ES_SYSTEMS", tmp.path().join("es_systems.cfg"))
                        .env("POCKETUI_DATA", tmp.path().join("data"));
                    if let Some(s) = &stub { cmd.env("WAYLAND_DISPLAY", &s.path); }
                    let out = cmd.output().unwrap();
                    assert!(out.status.success(), "{state}: {}", String::from_utf8_lossy(&out.stderr));
                    let visible = std::fs::read_to_string(&seen).unwrap();
                    for word in *words {
                        if !visible.lines().any(|line| word.split('|').any(|variant| line == variant.replace("Pocket", "Brick"))) {
                            found.push(format!("{state} {style}/{size}/{lang}: missing whole {word:?}:\n{visible}"));
                        }
                    }
                    let problems = std::fs::read_to_string(&boxes).unwrap();
                    found.extend(problems.lines().filter(|l| l.starts_with("O\t") || l.starts_with("E\t") || l.starts_with("T\t") || l.starts_with("V\t"))
                        .map(|l| format!("{state} {style}/{size}/{lang}: {l}")));
                }
            }
        }
    }
    assert!(found.is_empty(), "{}", found.join("\n"));
}

#[cfg(feature = "brick")]
#[test]
fn textfit_brick_031_system_facts_keep_the_ended_line() {
    check_brick_031_visible(&["home", "shelf:psx:split"],
        &[("en", &["Ended"]), ("de", &["Eingestellt"]), ("es", &["Fin"])]);
}

#[cfg(feature = "brick")]
#[test]
fn textfit_brick_031_home_keeps_the_recent_games_hint() {
    check_brick_031_visible(&["home"],
        &[("en", &["Recent"]), ("de", &["Zuletzt"]), ("es", &["Recientes"])]);
}

#[cfg(feature = "brick")]
#[test]
fn textfit_brick_031_info_keeps_rating_and_save_labels_whole() {
    check_brick_031_visible(&["info:split:snes:chrono"],
        &[("en", &["None. Press Y|None · Y", "Save slots", "Not yet"]), ("de", &["Keine. Drück Y|Keine · Y", "Spielstände", "Noch nicht"]),
          ("es", &["Ninguna. Pulsa Y|Ninguna · Y", "Ranuras", "Aún no"])]);
}

#[cfg(feature = "brick")]
#[test]
fn textfit_brick_031_credits_keep_the_source_line_whole() {
    check_brick_031_visible(&["console:gb:credits:8"],
        &[("en", &["Facts: Wikidata (CC0), Wikipedia · summaries written for pocket-ui"]),
          ("de", &["Fakten: Wikidata (CC0), Wikipedia · Kurztexte für pocket-ui geschrieben"]),
          ("es", &["Datos: Wikidata (CC0), Wikipedia · resúmenes escritos para pocket-ui"])]);
}

#[cfg(feature = "brick")]
#[test]
fn textfit_brick_031_password_note_stays_whole() {
    check_brick_031_visible(&["achievements:password"],
        &[("en", &["Sent once to sign in, never kept. The Pocket keeps only the sign-in key."]),
          ("de", &["Passwort einmal zur Anmeldung; gespeichert wird nur der Schlüssel."]),
          ("es", &["Se envía una vez al entrar; solo se guarda la clave, nunca la contraseña."])]);
}

#[cfg(feature = "brick")]
#[test]
fn textfit_brick_031_pixel_path_grades_stay_clear_of_the_selected_explanation() {
    check_brick_031_visible(&["pixel-path:gba", "pixel-path:snes:2", "pixel-path:patched", "pixel-path:compromised",
        "pixel-path:pal", "pixel-path:bluetooth"], &[("en", &[]), ("de", &[]), ("es", &[])]);
}

#[cfg(feature = "brick")]
#[test]
fn textfit_brick_031_extras_without_wifi_keeps_the_explanation_whole() {
    // The NextUI profile has no network page; the standalone card offers Wi-Fi setup instead.
    check_brick_031_visible_in(&["setup:extras", "setup:extras:da"],
        &[("en", &["This device has no Wi-Fi, so the downloads aren't available."]),
          ("de", &["Kein WLAN: Downloads sind nicht verfügbar."]),
          ("es", &["Sin Wi-Fi, las descargas no están disponibles."])], false);
    check_brick_031_visible(&["setup:extras"],
        &[("en", &["Connect to Wi-Fi first to use these."]),
          ("de", &["Verbinde dich zuerst mit dem WLAN, um sie zu nutzen."]),
          ("es", &["Conéctate primero a Wi-Fi para usarlos."])]);
}

#[cfg(feature = "brick")]
#[test]
fn textfit_brick_031_performance_readings_keep_a_gap_after_their_labels() {
    check_brick_031_visible(&["perf", "perf:3", "perf:1:veryquiet", "perf:2:veryquiet"],
        &[("en", &[]), ("de", &[]), ("es", &[])]);
}

#[cfg(feature = "brick")]
#[test]
fn textfit_brick_031_info_keeps_the_year_beside_a_favourite() {
    check_brick_031_visible(&["info:split:snes:chrono"],
        &[("en", &["Super Nintendo · 1995"]), ("de", &["Super Nintendo · 1995"]), ("es", &["Super Nintendo · 1995"])]);
    check_brick_031_visible(&["info:split:nes:final"],
        &[("en", &["Nintendo Entertainment System · 1990|NES · 1990"]), ("de", &["Nintendo Entertainment System · 1990|NES · 1990"]), ("es", &["Nintendo Entertainment System · 1990|NES · 1990"])]);
    check_brick_031_visible_fixture(&["info:split:nes:final"],
        &[("en", &["Nintendo Entertainment System · 1990|NES · 1990", "Favourite"]),
          ("de", &["Nintendo Entertainment System · 1990|NES · 1990", "Favorit"]),
          ("es", &["Nintendo Entertainment System · 1990|NES · 1990", "Favorito"])], true, |tmp| {
            let file = tmp.join("data/favorites.json");
            let mut favorites: serde_json::Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
            let path = std::fs::read_dir(tmp.join("roms/nes")).unwrap().flatten().map(|e| e.path()).find(|p| p.extension().is_some_and(|e| e == "nes")).unwrap();
            favorites.as_array_mut().unwrap().push(serde_json::json!({"system": "nes", "name": "Final Fantasy III", "path": path}));
            std::fs::write(file, serde_json::to_vec(&favorites).unwrap()).unwrap();
        });
}

/// Every Extras row's explanation is painted whole, never cut with "…" (09.11.: on the Brick's 1024x768 Box art's two
/// paragraphs ended in "no account i…" beside the hint pill, because the footer allowed four lines). Each row in turn,
/// online (`extras-portmaster-installed` starts on the PortMaster row) and offline, in Barlow and Retro, normal and Large.
/// Returns the problems found; `rows` is how many rows the device lists.
fn extras_explanations_whole(device: &str, standalone: bool, sizes: &[&str], rows: usize) -> Vec<String> {
    const BOX_ART: &str = "The front of each game's box, from a free collection online. Pictures come from a free collection online: no account is needed, and only a game's name is sent. Off downloads nothing by itself.";
    let tmp = tempfile::tempdir().unwrap();
    fake_library(tmp.path());
    let stub = wayland(tmp.path());
    let bin = std::env::var_os("POCKETUI_NAV_BINARY").map_or_else(|| PathBuf::from(env!("CARGO_BIN_EXE_pocket-ui")), PathBuf::from);
    let mut found = Vec::new();
    for (start, online) in [("extras-portmaster-installed", true), ("extras-portmaster-installed-offline", false)] {
        for row in 0..rows {
            // From the PortMaster row (2): up to Box art and Pictures, down to the rest.
            let keys = if row < 2 { "u".repeat(2 - row) } else { "d".repeat(row - 2) };
            let state = format!("setup:{start}:{keys}");
            // Barlow, and Retro in both of its letters: Departure Mono (the default since 0.3.2) and Pixelify Sans.
            for (retro, letters) in [("off", "departure"), ("cocoa", "departure"), ("cocoa", "pixelify")] {
                for &picture in sizes {
                    let (snapshot, text_size) = picture.split_once('/').unwrap();
                    for lang in ["en", "de", "es"] {
                        let seen = tmp.path().join("extras-visible.txt");
                        let mut cmd = Command::new(&bin);
                        cmd.args(["--snapshot", &state, tmp.path().join("extras.png").to_str().unwrap()])
                            .env("POCKETUI_MOCK", "1").env("POCKETUI_MOCK_NOW", FIXTURE_NOW).env("POCKETUI_DEVICE", device)
                            .env("POCKETUI_BRICK_STANDALONE", if standalone { "1" } else { "" })
                            .env("POCKETUI_SOUND_QUANTUM", "0").env("POCKETUI_LANG", lang)
                            .env("POCKETUI_TEXT_SIZE", text_size).env("POCKETUI_SNAPSHOT_SIZE", snapshot)
                            .env("POCKETUI_FONT", if retro == "off" { "barlow" } else { "retro" }).env("POCKETUI_RETRO", retro)
                            .env("POCKETUI_RETRO_LETTERS", letters)
                            .env("POCKETUI_VISIBLE_TEXT", &seen)
                            .env("POCKETUI_ES_SYSTEMS", tmp.path().join("es_systems.cfg"))
                            .env("POCKETUI_DATA", tmp.path().join("data"));
                        if let Some(s) = &stub { cmd.env("WAYLAND_DISPLAY", &s.path); }
                        let out = cmd.output().unwrap();
                        let at = format!("{state} {retro}/{letters}/{picture}/{lang}");
                        if !out.status.success() { found.push(format!("{at}: {}", String::from_utf8_lossy(&out.stderr))); continue; }
                        let visible = std::fs::read_to_string(&seen).unwrap();
                        if visible.contains('…') {
                            found.push(format!("{at}: an explanation ends in \"…\":\n{visible}"));
                        }
                        let words = visible.split_whitespace().collect::<Vec<_>>().join(" ");
                        if lang == "en" && row == 0 && online && !words.contains(BOX_ART) {
                            found.push(format!("{at}: Box art's explanation is not whole:\n{visible}"));
                        }
                    }
                }
            }
        }
    }
    found
}

#[test]
fn textfit_extras_explanations_stay_whole_on_rocknix_640x480() {
    let found = extras_explanations_whole("rg35xx-plus", false, &["640x480/normal", "640x480/large"], 4);
    assert!(found.is_empty(), "{}", found.join("\n"));
}

#[cfg(feature = "brick")]
#[test]
fn textfit_extras_explanations_stay_whole_on_the_trimui_card() {
    let found = extras_explanations_whole("brick", true, &["1024x768/normal", "1024x768/large", "640x480/normal", "640x480/large"], 5);
    assert!(found.is_empty(), "{}", found.join("\n"));
}
