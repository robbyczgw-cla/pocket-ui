//! What the live tests of 06.11.2026 (RG35XX Plus, Retroid Pocket Mini) found, painted by the real program: "Runs on" has a way back
//! to the default, and Controller setup says which pad to press, lets the Pocket's own buttons out of another pad's
//! capture and gives up by itself (the asserts of `controllers:<row>:capture-escape` run inside the program).
//!
//! Needs python3 for the made-up library and, on Linux without a compositor, scripts/wayland-stub.py (as
//! tests/empty_views.rs).

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
        let dir = tempfile::Builder::new().prefix("pui-live-").tempdir_in("/tmp").unwrap();
        let status = Command::new("python3").args(["-I", &format!("{ROOT}/scripts/curator-fake-library.py"), dir.path().to_str().unwrap(), "dark"]).stdout(std::process::Stdio::null()).status().unwrap();
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
            .env("POCKETUI_MOCK", "1")
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

/// Retroid Pocket Mini, 06.11.: "Runs on" had no entry for the default; going back meant knowing which core it was.
#[test]
fn runs_on_names_the_default() {
    let card = Card::new();
    // A game without a choice of its own opens on the default, named as such, in every language.
    let game = card.screen("emu:gba:Advance Wars (USA) (Rev 1).gba", "en");
    assert!(game.contains("Runs on ‹ Default (pocket-core · "), "{game}");
    let german = card.screen("emu:gba:Advance Wars (USA) (Rev 1).gba", "de");
    assert!(german.contains("‹ Standard (pocket-core · "), "{german}");
    // A game with its own choice shows that, and Default is the first entry to go back to (`emu_choice_page` tests).
    let chosen = card.screen("emu:gba:Advance Wars (USA) (Rev 1).gba:retroarch/vbam", "en");
    assert!(chosen.contains("‹ RetroArch · VBA-M ›") && !chosen.contains("Default ("), "{chosen}");
}

/// RG35XX Plus, 06.11.: during a Bluetooth pad's capture the menus didn't answer the Pocket's own buttons.
#[test]
fn controller_setup_says_which_pad_to_press_and_always_lets_go() {
    let card = Card::new();
    let waiting = card.screen("controllers:7:capture", "en");
    assert!(waiting.contains("Press a button on 8BitDo SN30 Pro"), "{waiting}");
    assert!(waiting.contains("HOME on any pad cancels"), "{waiting}");
    // The Pocket's B ends the capture, and a capture nobody answers ends by itself without a change (asserted in
    // the program); the page says so.
    let after = card.screen("controllers:7:capture-escape", "en");
    assert!(!after.contains("HOME on any pad cancels"), "{after}");
    assert!(after.contains("No button pressed. Nothing changed."), "{after}");
}
