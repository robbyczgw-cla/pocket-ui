//! A RetroArch game's HOME shortcuts as the player sees them, painted by the real program (`ra_hotkeys`,
//! `shortcut_list`): its pause menu's Options (Runs on) has a Shortcuts row, and the list says what HOME and another
//! button do in RetroArch, in English and German. Red before 0.6.1: Runs on had no such row and named two of them.
//!
//! Needs python3 for the made-up library and, on Linux without a compositor, scripts/wayland-stub.py (as
//! tests/live_fixes.rs).

use std::path::Path;
use std::process::Command;

include!("common/root.rs");

struct Stub(std::process::Child);

impl Drop for Stub {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

struct Card {
    dir: tempfile::TempDir,
    _stub: Option<Stub>,
}

impl Card {
    fn new() -> Card {
        // A short path: the stub's socket has to fit.
        let dir = tempfile::Builder::new().prefix("pui-live-").tempdir().unwrap();
        let status = Command::new("python3").env("POCKETUI_MOCK_NOW", FIXTURE_NOW).args(["-I", &format!("{ROOT}/scripts/curator-fake-library.py"), dir.path().to_str().unwrap(), "dark"]).stdout(std::process::Stdio::null()).status().unwrap();
        assert!(status.success());
        let stub = (cfg!(target_os = "linux") && std::env::var_os("WAYLAND_DISPLAY").is_none_or(|w| w.is_empty())).then(|| {
            let child = Command::new("python3").args(["-I", &format!("{ROOT}/scripts/wayland-stub.py"), dir.path().join("wl").to_str().unwrap()]).spawn().unwrap();
            test_clock::wait_for(|| dir.path().join("wl").exists(), std::time::Duration::from_secs(2), "Wayland stub listening");
            Stub(child)
        });
        Card { dir, _stub: stub }
    }

    /// The words the state's picture says, one per line, in `lang`.
    fn screen(&self, state: &str, lang: &str) -> String {
        let tmp: &Path = self.dir.path();
        let (png, words) = (tmp.join("shot.png"), tmp.join("words.tsv"));
        let _ = std::fs::remove_file(&words);
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
        if tmp.join("wl").exists() {
            cmd.env("WAYLAND_DISPLAY", tmp.join("wl"));
        }
        let out = cmd
            .args(["--snapshot", state, png.to_str().unwrap()])
            .env("POCKETUI_MOCK", "1").env("POCKETUI_MOCK_NOW", FIXTURE_NOW)
            .env("POCKETUI_LANG", lang)
            .env("POCKETUI_VISIBLE_TEXT", &words)
            .env("POCKETUI_DATA", tmp.join("data"))
            .env("POCKETUI_ES_SYSTEMS", tmp.join("es_systems.cfg"))
            .output()
            .unwrap();
        assert!(out.status.success() && png.exists(), "{state}: {}", String::from_utf8_lossy(&out.stderr));
        // One line, so a sentence the painter wrapped is still found whole.
        std::fs::read_to_string(words).unwrap_or_default().split_whitespace().collect::<Vec<_>>().join(" ")
    }
}

/// The pause menu of a RetroArch game → Options: the Runs on page, with a row that lists the shortcuts.
#[test]
fn a_retroarch_games_options_lead_to_its_shortcuts() {
    let card = Card::new();
    let page = card.screen("runs-on", "en");
    assert!(page.contains("Shortcuts"), "{page}");
    assert!(page.contains("HOME + R1 save, HOME + L1 load"), "the page's line names pocket-core's keys: {page}");
}

/// The list: pocket-core's keys as RetroArch has them, the two of pocket-ui's own, nothing RetroArch games lack.
#[test]
fn the_list_says_what_home_does_in_retroarch_in_every_language() {
    let card = Card::new();
    let en = card.screen("shortcut-list", "en");
    for words in ["Pause menu", "HOME + SELECT", "Save state", "HOME + R1", "Load state", "HOME + L1", "HOME + R2", "Hold HOME + L2", "Screenshot", "HOME + A", "HOME + START"] {
        assert!(en.contains(words), "{words}: {en}");
    }
    assert!(!en.contains("Left") && !en.contains("Zoom"), "no slots and no Zoom in RetroArch games: {en}");
    let de = card.screen("shortcut-list", "de");
    for words in ["Spielstand speichern", "HOME + R1", "HOME + L2 halten", "Menü von RetroArch"] {
        assert!(de.contains(words), "{words}: {de}");
    }
}
