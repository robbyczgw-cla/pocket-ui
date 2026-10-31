//! The starter pack as the player sees it (docs/STARTER-PACK.md), rendered by the real binary in German (so that the
//! painter's record of translations says what was painted, `POCKETUI_I18N_MISSES`):
//! - Home on a card with no games shows the calm hint with "Get the starter pack", not an empty shelf;
//! - the page lists the games, says "offline" calmly, shows progress, and says what it did.
//!
//! Needs python3 for the made-up library (as tests/i18n_snapshots.rs) and, on Linux without a compositor,
//! scripts/wayland-stub.py.

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

/// Renders `state` in German with a copy of the made-up library's data folder; returns the German lines painted.
fn painted(tmp: &Path, stub: &Option<Stub>, state: &str) -> Vec<String> {
    let name = state.replace(':', "-");
    let data = tmp.join(format!("data-{name}"));
    copy_dir(&tmp.join("data"), &data);
    let (png, record) = (tmp.join(format!("{name}.png")), tmp.join(format!("{name}.tsv")));
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
    if let Some(s) = stub {
        cmd.env("WAYLAND_DISPLAY", &s.path);
    }
    let out = cmd
        .args(["--snapshot", state, png.to_str().unwrap()])
        .env("POCKETUI_MOCK", "1")
        .env("POCKETUI_LANG", "de")
        .env("POCKETUI_I18N_MISSES", &record)
        .env("POCKETUI_ES_SYSTEMS", tmp.join("es_systems.cfg"))
        .env("POCKETUI_DATA", &data)
        .output()
        .expect("run pocket-ui");
    assert!(out.status.success() && png.exists(), "{state} didn't render: {}", String::from_utf8_lossy(&out.stderr));
    std::fs::read_to_string(record).unwrap_or_default().lines().filter_map(|l| l.strip_prefix("O\t")).map(str::to_string).collect()
}

#[test]
fn an_empty_card_shows_the_starter_pack_and_the_page_speaks_plainly() {
    let _tmp_guard = tempfile::tempdir().unwrap();
    let tmp = _tmp_guard.path().to_path_buf();
    let status = Command::new("python3")
        .args(["-I", &format!("{ROOT}/scripts/curator-fake-library.py"), tmp.to_str().unwrap(), "dark"])
        .stdout(std::process::Stdio::null())
        .status()
        .expect("python3 for scripts/curator-fake-library.py");
    assert!(status.success());
    let stub = wayland(&tmp);
    let has = |lines: &[String], what: &str| lines.iter().any(|l| l.contains(what));

    // Home with no games: the hint, and what it needs, instead of a gap (or only the Curator tile).
    let home = painted(&tmp, &stub, "starter-home");
    assert!(has(&home, "Noch keine Spiele"), "{home:?}");
    assert!(has(&home, "Startpaket holen"), "the one thing to press: {home:?}");
    assert!(has(&home, "kostenlose Spiele"), "{home:?}");
    assert!(has(&home, "Einstellungen"), "where else games come from: {home:?}");
    // A download under way says so instead of offering it again.
    let busy = painted(&tmp, &stub, "starter-home:busy");
    assert!(has(&busy, "wird geholt"), "{busy:?}");
    assert!(!has(&busy, "Startpaket holen"), "{busy:?}");

    // The page: every game of the pack, with what it is; "all", and what to press.
    let page = painted(&tmp, &stub, "starter");
    assert!(has(&page, "Startpaket"), "{page:?}");
    assert!(has(&page, "Alle 9 Spiele holen"), "{page:?}");
    assert!(has(&page, "Alle holen"), "{page:?}");
    // A game: what it is, and whose it is.
    let game = painted(&tmp, &stub, "starter:2");
    assert!(has(&game, "Von "), "author and licence under the list: {game:?}");
    // Offline: said calmly, with what to do.
    let offline = painted(&tmp, &stub, "starter:3:offline");
    assert!(has(&offline, "nicht online") && has(&offline, "WLAN"), "{offline:?}");
    // A damaged download: nothing was added.
    let failed = painted(&tmp, &stub, "starter:0:failed");
    assert!(has(&failed, "Nichts wurde hinzugefügt"), "{failed:?}");
    // Games on the SD card say so.
    let there = painted(&tmp, &stub, "starter:0:there");
    assert!(has(&there, "Auf deiner SD-Karte"), "{there:?}");
    drop(stub);
}

/// The offline note under the starter-pack button wraps: as one line it ran off the right edge
/// (German, Large text, 1280x800; monkey run 2026-10-30 seed 8, TEXT E).
#[test]
fn the_offline_note_on_an_empty_home_wraps_inside_the_screen() {
    let tmp_guard = tempfile::tempdir().unwrap();
    let tmp = tmp_guard.path();
    let status = Command::new("python3")
        .args(["-I", &format!("{ROOT}/scripts/curator-fake-library.py"), tmp.to_str().unwrap(), "dark"])
        .stdout(std::process::Stdio::null())
        .status()
        .expect("python3 for scripts/curator-fake-library.py");
    assert!(status.success());
    let stub = wayland(tmp);
    let (png, boxes, text) = (tmp.join("home-offline.png"), tmp.join("home-offline.box"), tmp.join("home-offline.txt"));
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
    if let Some(s) = &stub {
        cmd.env("WAYLAND_DISPLAY", &s.path);
    }
    let out = cmd
        .args(["--snapshot", "starter-home:offline", png.to_str().unwrap()])
        .env("POCKETUI_MOCK", "1")
        .env("POCKETUI_LANG", "de")
        .env("POCKETUI_TEXT_SIZE", "large")
        .env("POCKETUI_SNAPSHOT_SIZE", "1280x800")
        .env("POCKETUI_VISIBLE_TEXT", &text)
        .env("POCKETUI_TEXT_BOXES", &boxes)
        .env("POCKETUI_ES_SYSTEMS", tmp.join("es_systems.cfg"))
        .env("POCKETUI_DATA", tmp.join("data"))
        .output()
        .expect("run pocket-ui");
    assert!(out.status.success() && png.exists(), "{}", String::from_utf8_lossy(&out.stderr));
    let problems = std::fs::read_to_string(&boxes).unwrap();
    assert!(problems.trim().is_empty(), "offline note clipped or overlapping: {problems}");
    let seen = std::fs::read_to_string(&text).unwrap();
    assert!(seen.contains("nicht online") && seen.contains("noch einmal"), "the whole note is painted: {seen}");
    drop(stub);
}

/// House rule "never a blank where a figure goes": a store device whose catalog never loaded still paints words and
/// art on the list and detail pages (they once painted only the two pills).
#[test]
fn the_store_pages_are_never_blank_without_a_catalog() {
    let _tmp_guard = tempfile::tempdir().unwrap();
    let tmp = _tmp_guard.path().to_path_buf();
    let status = Command::new("python3")
        .args(["-I", &format!("{ROOT}/scripts/curator-fake-library.py"), tmp.to_str().unwrap(), "dark"])
        .stdout(std::process::Stdio::null())
        .status()
        .expect("python3 for scripts/curator-fake-library.py");
    assert!(status.success());
    let stub = wayland(&tmp);
    for state in ["store", "store:list", "store:detail", "store:list:tools"] {
        let lines = painted(&tmp, &stub, state);
        assert!(!lines.is_empty(), "{state} painted no words: {lines:?}");
    }
}
