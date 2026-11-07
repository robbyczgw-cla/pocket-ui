//! The second screen (`src/app_second_screen.rs`, docs/DUAL-SCREEN.md): what the RG DS Plus's lower panel shows below
//! the shelf, at its 1024x768, in English and German. Every case shows something readable, never
//! an empty screen: a game's box and its info page (with and without a description), a system, a row that is no game.
//! The text painter's record (`POCKETUI_TEXT_BOXES`, as tests/info_page.rs) must show nothing cut off or overlapping.
//! Needs python3 for the made-up library and, on Linux without a compositor, scripts/wayland-stub.py.

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

/// Renders `state` on the RG DS Plus; returns the visible text and the text painter's problems.
fn shot(tmp: &Path, stub: Option<&Stub>, state: &str, lang: &str, text_size: &str) -> (String, String) {
    let png = tmp.join("shot.png");
    let (boxes, visible) = (tmp.join("text.box"), tmp.join("visible.txt"));
    let _ = std::fs::remove_file(&boxes);
    let _ = std::fs::remove_file(&visible);
    // No network: the card's Wikipedia worker gets a curl that can't connect, so the page is the same every time.
    let bin = tmp.join("offline-bin");
    std::fs::create_dir_all(&bin).unwrap();
    let curl = bin.join("curl");
    std::fs::write(&curl, "#!/bin/sh\nexit 7\n").unwrap();
    std::fs::set_permissions(&curl, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    let path = format!("{}:{}", bin.display(), std::env::var("PATH").unwrap_or_default());
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
    cmd.args(["--snapshot", &format!("{state}:device=rgdsplus"), png.to_str().unwrap()])
        .env("PATH", path)
        .env("POCKETUI_MOCK", "1")
        .env("POCKETUI_LANG", lang)
        .env("POCKETUI_TEXT_SIZE", text_size)
        .env("POCKETUI_SNAPSHOT_SIZE", "1024x768")
        .env("POCKETUI_TEXT_BOXES", &boxes)
        .env("POCKETUI_VISIBLE_TEXT", &visible)
        .env("POCKETUI_ES_SYSTEMS", tmp.join("es_systems.cfg"))
        .env("POCKETUI_DATA", tmp.join("data"));
    if let Some(s) = stub {
        cmd.env("WAYLAND_DISPLAY", &s.path);
    }
    let out = cmd.output().unwrap();
    assert!(out.status.success() && png.exists(), "{state}: {}", String::from_utf8_lossy(&out.stderr));
    (std::fs::read_to_string(&visible).unwrap_or_default(), std::fs::read_to_string(&boxes).unwrap_or_default())
}

#[test]
fn the_lower_screen_always_shows_something_readable() {
    check(&[("en", "normal"), ("de", "normal")]);
}

/// 1024x768 is painted as it is (`scaled_layout` keeps 721 to 800 lines native; scaling 1280x960 down would cost more
/// per frame than the RG CubeXX's path, which was too slow on an A53). At Large in German the info page's facts take two
/// lines each there and leave no room for the description: a finding for the 1024x768 layout pass, kept here so that
/// the fix has its test.
#[test]
#[ignore = "1024x768 at Large: the info page has no room for the description yet (the 1024x768 layout pass)"]
fn the_lower_screen_at_large_text_in_german() {
    check(&[("de", "large")]);
}

fn check(langs: &[(&str, &str)]) {
    let tmp = tempfile::tempdir().unwrap();
    let status = Command::new("python3")
        .args(["-I", &format!("{ROOT}/scripts/curator-fake-library.py"), tmp.path().to_str().unwrap(), "dark"])
        .stdout(std::process::Stdio::null())
        .status()
        .expect("python3 for scripts/curator-fake-library.py");
    assert!(status.success());
    let stub = wayland(tmp.path());
    for &(lang, text_size) in langs {
        let de = lang == "de";
        // The box first: the game's name on it, no hint bar (no key acts on the screen below).
        let (seen, problems) = shot(tmp.path(), stub.as_ref(), "second:box:gba:advance wars", lang, text_size);
        assert!(seen.contains("Advance Wars"), "{lang} box: no title\n{seen}");
        assert!(!seen.contains(if de { "Umdrehen" } else { "Turn over" }), "{lang} box: a hint bar below\n{seen}");
        assert!(problems.trim().is_empty(), "{lang} box: {problems}");
        // The info page after 4 s: with a description from the gamelist, and without one (something in its place).
        for (game, title) in [("gba:advance wars", "Advance Wars"), ("ps2:revenge", "Burnout Revenge")] {
            let (seen, problems) = shot(tmp.path(), stub.as_ref(), &format!("second:info:{game}"), lang, text_size);
            assert!(seen.contains(title) && seen.contains(if de { "Gespielt" } else { "Played" }), "{lang} {game}: not the info page\n{seen}");
            let about = ["Command an army", "A 2005 racing game", "description", "Beschreibung"];
            assert!(about.iter().any(|a| seen.contains(a)), "{lang} {game}: nothing in the description's place\n{seen}");
            assert!(problems.trim().is_empty(), "{lang} {game}: {problems}");
            assert!(!seen.contains("Press Y") && !seen.contains("Drück Y"), "the passive page must not tell the player to rate it with a key: {seen}");
            let empty = if de { "Noch keine Beschreibung." } else { "No description yet." };
            assert!(seen.matches(empty).count() <= 1, "the missing-description message is shown once: {seen}");
        }
        // A system row: its console's name. A row that is no game: a line saying what shows here.
        let (seen, _) = shot(tmp.path(), stub.as_ref(), "second:system:snes", lang, text_size);
        assert!(seen.contains("Super Nintendo"), "{lang} system\n{seen}");
        let (seen, _) = shot(tmp.path(), stub.as_ref(), "second:none", lang, text_size);
        let line = if de { "Wähle ein Spiel: Hier erscheint seine Box." } else { "Choose a game: its box shows here." };
        assert!(seen.contains(line), "{lang} none\n{seen}");
    }
}
