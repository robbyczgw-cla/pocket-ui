//! The game's info page (`src/game_info.rs`): X on a game in the Minimal, Cards and Split looks opens one screen with
//! everything about the game; Shelf and Cabinet still open the box (`game_box`). Each of the three looks is rendered at
//! a 720p-class screen and at the H700's 640x480 (laid out at 1280x960), in English at Normal and in German at Large,
//! and the text painter's record (`POCKETUI_TEXT_BOXES`, as tests/text_size.rs) must show nothing cut off or
//! overlapping. Needs python3 for the made-up library and, on Linux without a compositor, scripts/wayland-stub.py.

use std::path::{Path, PathBuf};
use std::process::Command;

include!("common/root.rs");

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
    let child = Command::new("python3")
        .args(["-I", &format!("{ROOT}/scripts/wayland-stub.py"), path.to_str().unwrap()])
        .spawn()
        .expect("python3 for scripts/wayland-stub.py");
    let mut stub = Stub { path, child };
    // The stub publishes its socket only once it listens. Python's start-up can take many seconds on a loaded
    // machine (the gate runs the whole suite in parallel), so the limit is only there to end a stub that is dead.
    let until = std::time::Instant::now() + test_clock::timeout(std::time::Duration::from_secs(120));
    while std::time::Instant::now() < until {
        if stub.path.exists() {
            return Some(stub);
        }
        assert!(stub.child.try_wait().unwrap().is_none(), "scripts/wayland-stub.py exited at start-up");
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    panic!("scripts/wayland-stub.py didn't start");
}

/// Renders `state`; returns the visible text and the text painter's problems.
fn shot(tmp: &Path, stub: Option<&Stub>, state: &str, size: &str, lang: &str, text_size: &str) -> (String, String) {
    shot_in(tmp, stub, state, size, lang, text_size, &[])
}

fn shot_in(tmp: &Path, stub: Option<&Stub>, state: &str, size: &str, lang: &str, text_size: &str, env: &[(&str, &str)]) -> (String, String) {
    let png = tmp.join("shot.png");
    let (boxes, visible) = (tmp.join("text.box"), tmp.join("visible.txt"));
    let _ = std::fs::remove_file(&boxes);
    // The card asks Wikipedia for a description through curl from a worker thread. With a real curl the page showed the
    // placeholder, the "offline" note or the real article depending on the network and on whether the answer came
    // before the render (or an earlier shot's cache); a curl that can't connect makes it the same every time.
    let bin = tmp.join("offline-bin");
    std::fs::create_dir_all(&bin).unwrap();
    let curl = bin.join("curl");
    std::fs::write(&curl, "#!/bin/sh\nexit 7\n").unwrap();
    std::fs::set_permissions(&curl, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    let path = format!("{}:{}", bin.display(), std::env::var("PATH").unwrap_or_default());
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
    cmd.args(["--snapshot", state, png.to_str().unwrap()])
        .env("PATH", path)
        .env("POCKETUI_MOCK", "1").env("POCKETUI_MOCK_NOW", FIXTURE_NOW)
        .env("POCKETUI_LANG", lang)
        .env("POCKETUI_TEXT_SIZE", text_size)
        .env("POCKETUI_SNAPSHOT_SIZE", size)
        .env("POCKETUI_TEXT_BOXES", &boxes)
        .env("POCKETUI_VISIBLE_TEXT", &visible)
        .env("POCKETUI_ES_SYSTEMS", tmp.join("es_systems.cfg"))
        .env("POCKETUI_DATA", tmp.join("data"));
    if let Some(s) = stub {
        cmd.env("WAYLAND_DISPLAY", &s.path);
    }
    cmd.envs(env.iter().copied());
    let out = cmd.output().unwrap();
    assert!(out.status.success() && png.exists(), "{state} at {size}: {}", String::from_utf8_lossy(&out.stderr));
    (std::fs::read_to_string(&visible).unwrap_or_default(), std::fs::read_to_string(&boxes).unwrap_or_default())
}

#[test]
fn minimal_cards_and_split_open_the_info_page_and_nothing_is_cut_off() {
    let tmp = tempfile::tempdir().unwrap();
    let status = Command::new("python3").env("POCKETUI_MOCK_NOW", FIXTURE_NOW)
        .args(["-I", &format!("{ROOT}/scripts/curator-fake-library.py"), tmp.path().to_str().unwrap(), "dark"])
        .stdout(std::process::Stdio::null())
        .status()
        .expect("python3 for scripts/curator-fake-library.py");
    assert!(status.success());
    let stub = wayland(tmp.path());
    for look in ["minimal", "cards", "split"] {
        for size in ["1280x720", "640x480"] {
            for (lang, text_size) in [("en", "normal"), ("de", "large")] {
                // A game with a description from the gamelist, one with none, a Favourite.
                for (game, title) in [("gba:advance wars", "Advance Wars"), ("ps2:revenge", "Burnout Revenge"), ("snes:chrono", "Chrono Trigger")] {
                    let state = format!("info:{look}:{game}");
                    let (seen, problems) = shot(tmp.path(), stub.as_ref(), &state, size, lang, text_size);
                    let name = format!("{state} {size} {lang} {text_size}");
                    assert!(seen.contains(title), "{name}: no title\n{seen}");
                    // The info page names Y Status; the four-hint bar leaves SELECT Details out (`kit::top_hints`).
                    let (details, played, turn) = if lang == "de" { ("Status", "Gespielt", "Umdrehen") } else { ("Status", "Played", "Turn over") };
                    assert!(seen.contains(details) && seen.contains(played) && !seen.contains(turn), "{name}: not the info page\n{seen}");
                    // Something visible where a description would be, never a gap.
                    let about = ["Command an army", "description", "Beschreibung"];
                    assert!(about.iter().any(|a| seen.contains(a)), "{name}: nothing in the description's place\n{seen}");
                    assert!(problems.trim().is_empty(), "{name}: {problems}");
                }
            }
        }
    }
    // The collector's looks keep the box.
    for look in ["shelf", "cabinet"] {
        let (seen, _) = shot(tmp.path(), stub.as_ref(), &format!("info:{look}:gba:advance wars"), "640x480", "en", "normal");
        assert!(seen.contains("Turn over") && !seen.contains("Status"), "{look}: not the box\n{seen}");
    }
}

/// New-user test 2026-10-30: "Entwickler: Nintendo Research & De…" was cut short in German at Large. A long developer
/// takes a second line instead, and nothing of it is lost.
#[test]
fn a_long_developer_takes_a_second_line_instead_of_ending_in_dots() {
    let tmp = tempfile::tempdir().unwrap();
    let status = Command::new("python3").env("POCKETUI_MOCK_NOW", FIXTURE_NOW)
        .args(["-I", &format!("{ROOT}/scripts/curator-fake-library.py"), tmp.path().to_str().unwrap(), "dark"])
        .stdout(std::process::Stdio::null())
        .status()
        .expect("python3 for scripts/curator-fake-library.py");
    assert!(status.success());
    let stub = wayland(tmp.path());
    for (lang, text_size) in [("de", "large"), ("en", "normal")] {
        let (seen, problems) = shot(tmp.path(), stub.as_ref(), "info:minimal:gb:link", "640x480", lang, text_size);
        assert!(seen.contains("Nintendo Entertainment") && seen.contains("Analysis & Development"), "{lang}: the whole developer is there\n{seen}");
        assert!(problems.trim().is_empty(), "{lang}: {problems}");
    }
}

/// 0.3.1: Retro/Large facts filled the right column on 1024x768 and silently hid the description.
/// Down must reach it without moving the fixed game heading; every fact label must be readable along the way.
#[test]
fn textfit_031_description_is_reachable_below_the_facts() {
    let tmp = tempfile::tempdir().unwrap();
    let status = Command::new("python3")
        .args(["-I", &format!("{ROOT}/scripts/curator-fake-library.py"), tmp.path().to_str().unwrap(), "dark"])
        .stdout(std::process::Stdio::null()).status().unwrap();
    assert!(status.success());
    let stub = wayland(tmp.path());
    for dimensions in ["1024x768", "640x480"] {
        let mut all = String::new();
        let mut reached = false;
        let nav = tmp.path().join("description.nav");
        for n in 0..=20 {
            let state = format!("info:cards:ps2:revenge:{}", "d".repeat(n));
            let (seen, problems) = shot_in(tmp.path(), stub.as_ref(), &state, dimensions, "es", "large",
                &[("POCKETUI_RETRO", "cocoa"), ("POCKETUI_NAV_PROBE", nav.to_str().unwrap())]);
            let words = seen.split_whitespace().collect::<Vec<_>>().join(" ");
            assert!(words.contains("Burnout Revenge") && words.contains("2005"), "fixed title/year lost at {dimensions}, Down × {n}: {seen}");
            assert!(problems.trim().is_empty(), "{dimensions}, Down × {n}: {problems}");
            // The visible-text transcript can include a line with only a few pixels left at a scroll edge.
            // Compare its painted ink height with the fixed metadata in the same body face: it must be readable whole.
            let record = std::fs::read_to_string(&nav).unwrap();
            let lines: Vec<_> = record.lines().filter_map(|line| {
                let parts: Vec<_> = line.splitn(5, '\t').collect();
                if parts.len() != 5 { return None; }
                Some((parts[1].parse::<i32>().ok()?, parts[3].parse::<i32>().ok()?, parts[4]))
            }).collect();
            let metadata_height = lines.iter().find(|(_, _, text)| text.contains("2005"))
                .map(|(top, bottom, _)| bottom - top).expect("fixed metadata");
            reached |= lines.iter().any(|(top, bottom, text)| text.contains("descripción") &&
                (bottom - top) * 4 >= metadata_height * 3);
            all += &format!(" {words}");
        }
        assert!(reached, "{dimensions}: Down never reached a description or its missing-description note:\n{all}");
        for label in ["Desarrollador", "Distribuidor", "Género", "Serie", "Jugadores"] {
            assert!(all.contains(label), "{dimensions}: whole fact label {label} missing");
        }
        let rating = include_str!("../assets/i18n/es.tsv").lines().find_map(|l| l.strip_prefix("Rating\t")).unwrap();
        assert!(all.split_whitespace().any(|w| w == rating), "{dimensions}: whole rating label {rating} missing");
    }
}
