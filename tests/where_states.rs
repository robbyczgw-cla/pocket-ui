//! Where the UI says it is, against what it painted (`src/app_where.rs`, docs/DEVICE-TESTS.md "Where the UI is"): each
//! snapshot state is rendered with `POCKETUI_WHERE_RECORD`, which writes the painter `App::paint` ran and the name
//! `App::where_now` gives; the name must be a page of that painter (`painter_of`): a page the status file and
//! `@where` name wrongly is one an agent presses blind on, and one the navigation explorer never walks (it enters a page
//! only when `@where` changes; Date and time, the Tour and Settings → Controllers over the pause menu kept their opener's
//! name until 2026-11-08).
//!
//! The gate runs the pages that were named wrongly and one state of each painter; `sh scripts/release-check.sh` runs
//! every state of `tests/i18n-states.txt`. Needs python3 (the made-up library, and scripts/wayland-stub.py on Linux
//! without a compositor).

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};

include!("common/root.rs");

/// What a state's record says: the painter that ran, and where the App says it is.
#[derive(Debug, Clone)]
struct Record {
    painted: String,
    at: String,
}

/// The painter of the page `at` names (its part before " / "), read from the name alone: the test doesn't take the
/// binary's word for it (`Place::painter` is the binary's own mapping, for the explorer's `@painter`).
fn painter_of(at: &str) -> &'static str {
    let page = at.split(" / ").next().unwrap_or(at);
    let named = |p: &str| page == p || page.starts_with(&format!("{p}:")) || page.starts_with(&format!("{p} ("));
    if page.starts_with("settings:Manual") {
        "manual"
    } else if named("settings") {
        "settings"
    } else if named("setup") {
        "setup"
    } else if named("kids") {
        "kids"
    } else if page == "pause:Big" {
        "big pause"
    } else if named("pause") {
        "pause"
    } else if named("card") || page == "box" {
        "card"
    } else if named("curator") {
        "curator"
    } else if named("recent games") {
        "switcher"
    } else if named("shelf") || page == "game" {
        "home"
    } else {
        match page {
            "powering off" => "powering off",
            "app" => "app starting",
            "tour" => "tour",
            "port files" => "port files",
            "controls card" => "controls card",
            "runs on" => "runs on",
            "this game" => "this game",
            "port performance" => "port perf",
            "screenshots" => "gallery",
            "save versions" => "conflicts",
            "console" => "console",
            "search" => "search",
            _ => "?",
        }
    }
}

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

/// winit wants a Wayland compositor to say hello before a snapshot: scripts/wayland-stub.py stands in where there is none.
fn wayland(tmp: &Path) -> Option<Stub> {
    if !cfg!(target_os = "linux") || std::env::var_os("WAYLAND_DISPLAY").is_some_and(|w| !w.is_empty()) {
        return None;
    }
    let path = tmp.join("wayland-stub");
    let child = Command::new("python3")
        .args(["-I", &format!("{ROOT}/scripts/wayland-stub.py"), path.to_str().unwrap()])
        .spawn()
        .expect("python3 for scripts/wayland-stub.py");
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

/// Renders `states` (side by side, each with its own copy of the made-up library's data) and returns each one's record;
/// None for a state that painted nothing through `App::paint` (a snapshot that calls one painter itself).
fn records(states: &[String]) -> Vec<(String, Option<Record>)> {
    let tmp_guard = tempfile::tempdir().unwrap();
    let tmp = tmp_guard.path().to_path_buf();
    let status = Command::new("python3")
        .env("POCKETUI_MOCK_NOW", FIXTURE_NOW)
        .args(["-I", &format!("{ROOT}/scripts/curator-fake-library.py"), tmp.to_str().unwrap(), "dark"])
        .stdout(std::process::Stdio::null())
        .status()
        .expect("python3 for scripts/curator-fake-library.py");
    assert!(status.success(), "curator-fake-library.py failed");
    let bin = PathBuf::from(env!("CARGO_BIN_EXE_pocket-ui"));
    let stub = wayland(&tmp);
    let work: Arc<Mutex<Vec<(usize, String)>>> = Arc::new(Mutex::new(states.iter().cloned().enumerate().collect()));
    let out: Arc<Mutex<Vec<(usize, String, Option<Record>)>>> = Arc::default();
    let threads: Vec<_> = (0..std::thread::available_parallelism().map_or(4, |n| n.get().min(8)))
        .map(|_| {
            let (work, out, tmp, bin, display) = (work.clone(), out.clone(), tmp.clone(), bin.clone(), stub.as_ref().map(|s| s.path.clone()));
            std::thread::spawn(move || loop {
                let Some((i, state)) = work.lock().unwrap().pop() else { break };
                let data = tmp.join(format!("data-{i}"));
                copy_dir(&tmp.join("data"), &data);
                let record = tmp.join(format!("{i}.where"));
                let (safe, s) = match state.strip_prefix("safe ") {
                    Some(s) => (true, s.to_string()),
                    None => (false, state.clone()),
                };
                let mut cmd = Command::new(&bin);
                if safe {
                    cmd.arg("--safe");
                }
                if let Some(path) = &display {
                    cmd.env("WAYLAND_DISPLAY", path);
                }
                let run = cmd
                    .args(["--snapshot", &s, tmp.join(format!("{i}.png")).to_str().unwrap()])
                    .env("POCKETUI_MOCK", "1")
                    .env("POCKETUI_MOCK_NOW", FIXTURE_NOW)
                    .env("POCKETUI_ES_SYSTEMS", tmp.join("es_systems.cfg"))
                    .env("POCKETUI_DATA", &data)
                    .env("POCKETUI_WHERE_RECORD", &record)
                    .output()
                    .expect("run pocket-ui");
                assert!(run.status.success(), "{state}: {}\n{}", run.status, String::from_utf8_lossy(&run.stderr));
                let text = std::fs::read_to_string(&record).unwrap_or_default();
                let (painted, at) = text.trim_end_matches('\n').split_once('\t').map(|(p, a)| (p.to_string(), a.to_string())).unwrap_or_default();
                let r = (!painted.is_empty()).then_some(Record { painted, at });
                out.lock().unwrap().push((i, state, r));
            })
        })
        .collect();
    for t in threads {
        t.join().unwrap();
    }
    drop(stub);
    let mut all = std::mem::take(&mut *out.lock().unwrap());
    all.sort_by_key(|(i, ..)| *i);
    all.into_iter().map(|(_, s, r)| (s, r)).collect()
}

/// Every state whose page the painter and `@where` disagree on.
fn mismatches(records: &[(String, Option<Record>)]) -> Vec<String> {
    records
        .iter()
        .filter_map(|(state, r)| r.as_ref().filter(|r| r.painted != painter_of(&r.at)).map(|r| format!("{state}: painted by {}, but named \"{}\"", r.painted, r.at)))
        .collect()
}

fn at(records: &[(String, Option<Record>)], state: &str) -> String {
    records.iter().find(|(s, _)| s == state).and_then(|(_, r)| r.as_ref()).map(|r| r.at.clone()).unwrap_or_else(|| panic!("{state}: no record"))
}

/// The pages `@where` named after the page under them, and one state of each painter.
const GATE_STATES: &[&str] = &[
    "settings", "settings:0:4", "sys:time", "sys:quick", "sys:power", "tour", "search:results:mario", "curator:lens",
    "card-manage:ps2:revenge:delete", "kids-pin", "kids-shelf", "this-game:gba", "port-perf", "conflicts:card",
    "controls-card:psx", "setup:wifi", "menu:pocket-core", "notice:bios", "main",
];

#[test]
fn the_page_where_says_is_the_page_that_was_painted() {
    let states: Vec<String> = GATE_STATES.iter().map(|s| s.to_string()).collect();
    let records = records(&states);
    let wrong = mismatches(&records);
    assert!(wrong.is_empty(), "{} states named after another page:\n{}", wrong.len(), wrong.join("\n"));
    // What used to be named after what was under it (an agent pressed blind on these).
    assert_eq!(at(&records, "tour"), "tour");
    assert_eq!(at(&records, "search:results:mario"), "search");
    assert!(at(&records, "sys:time").ends_with(" / time"), "{}", at(&records, "sys:time"));
    assert_eq!(at(&records, "card-manage:ps2:revenge:delete"), "card:Delete");
    assert_eq!(at(&records, "kids-pin"), "kids:pin");
    assert!(at(&records, "curator:lens").starts_with("curator:"), "{}", at(&records, "curator:lens"));
}

#[test]
#[ignore = "release check: sh scripts/release-check.sh"]
fn every_snapshot_state_names_the_page_it_painted() {
    let states: Vec<String> = std::fs::read_to_string(Path::new(&ROOT).join("tests/i18n-states.txt"))
        .expect("tests/i18n-states.txt")
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(str::to_string)
        .collect();
    let records = records(&states);
    let painted = records.iter().filter(|(_, r)| r.is_some()).count();
    assert!(painted * 2 > records.len(), "only {painted} of {} states painted through App::paint", records.len());
    let wrong = mismatches(&records);
    assert!(wrong.is_empty(), "{} of {painted} states named after another page:\n{}", wrong.len(), wrong.join("\n"));
}
