//! The game's info page (`src/game_info.rs`): X on a game in the Minimal, Cards and Split looks opens one screen with
//! everything about the game; Shelf and Cabinet still open the box (`game_box`). Each of the three looks is rendered at
//! a 720p-class screen and at the H700's 640x480 (laid out at 1280x960), in English at Normal and in German at Large,
//! and the text painter's record (`POCKETUI_TEXT_BOXES`, as tests/text_size.rs) must show nothing cut off or
//! overlapping. Needs python3 for the made-up library and, on Linux without a compositor, scripts/wayland-stub.py.

use std::path::{Path, PathBuf};
use std::process::Command;

const ROOT: &str = env!("CARGO_MANIFEST_DIR");

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
    let until = std::time::Instant::now() + std::time::Duration::from_secs(120);
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
        .env("POCKETUI_MOCK", "1")
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
    let out = cmd.output().unwrap();
    assert!(out.status.success() && png.exists(), "{state} at {size}: {}", String::from_utf8_lossy(&out.stderr));
    (std::fs::read_to_string(&visible).unwrap_or_default(), std::fs::read_to_string(&boxes).unwrap_or_default())
}

#[test]
fn minimal_cards_and_split_open_the_info_page_and_nothing_is_cut_off() {
    let tmp = tempfile::tempdir().unwrap();
    let status = Command::new("python3")
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
                    let (details, played, turn) = if lang == "de" { ("Details", "Gespielt", "Umdrehen") } else { ("Details", "Played", "Turn over") };
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
        assert!(seen.contains("Turn over") && !seen.contains("Details"), "{look}: not the box\n{seen}");
    }
}

/// New-user test 2026-10-30: "Entwickler: Nintendo Research & De…" was cut short in German at Large. A long developer
/// takes a second line instead, and nothing of it is lost.
#[test]
fn a_long_developer_takes_a_second_line_instead_of_ending_in_dots() {
    let tmp = tempfile::tempdir().unwrap();
    let status = Command::new("python3")
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
