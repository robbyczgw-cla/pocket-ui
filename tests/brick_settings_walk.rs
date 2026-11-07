//! The TrimUI Brick's own settings as a player meets them (settings walk 06.11.2026, before the public release): rendered with
//! `pocket-ui --snapshot` as the Brick's own card runs it (`POCKETUI_DEVICE=brick`, `POCKETUI_BRICK_STANDALONE=1`, 1024x768) on
//! scripts/curator-fake-library.py's made-up library, in English, German and Spanish. Each test is a spot the walk found confusing:
//! a page that named the Pocket, offered ROCKNIX's menu or Android, called a tested handheld untested, said "System: unknown", ran
//! text into the foot or cut a line short, or drew sticks the Brick hasn't.
//!
//! Needs python3 for the made-up library and, on Linux without a compositor, scripts/wayland-stub.py (as
//! tests/controls_card_snapshots.rs).
#![cfg(feature = "brick")]

use std::path::PathBuf;
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

/// The made-up library, a Wayland stand-in and a place for the pictures.
struct Card {
    _dir: tempfile::TempDir,
    tmp: PathBuf,
    stub: Option<Stub>,
}

impl Card {
    fn new() -> Card {
        let dir = tempfile::tempdir().unwrap();
        let tmp = dir.path().to_path_buf();
        let out = Command::new("python3").args(["-I", &format!("{ROOT}/scripts/curator-fake-library.py"), tmp.join("lib").to_str().unwrap(), "dark"]).output().unwrap();
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        let stub = (cfg!(target_os = "linux") && !std::env::var_os("WAYLAND_DISPLAY").is_some_and(|w| !w.is_empty())).then(|| {
            let path = tmp.join("wl");
            let child = Command::new("python3").args(["-I", &format!("{ROOT}/scripts/wayland-stub.py"), path.to_str().unwrap()]).spawn().expect("python3 for scripts/wayland-stub.py");
            let stub = Stub { path, child };
            test_clock::wait_for(|| stub.path.exists(), std::time::Duration::from_secs(2), "Wayland stub listening");
            stub
        });
        Card { _dir: dir, tmp, stub }
    }

    /// What the player reads on `state` in `lang` at `size` (Normal or Large text), and the painter's findings (text running into text
    /// or the hints, cut by an edge).
    fn shot(&self, state: &str, lang: &str, size: &str) -> (String, String) {
        let (visible, boxes) = (self.tmp.join("visible.txt"), self.tmp.join("boxes.txt"));
        let _ = std::fs::remove_file(&visible);
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
        if let Some(s) = &self.stub {
            cmd.env("WAYLAND_DISPLAY", &s.path);
        }
        let out = cmd
            .args(["--snapshot", state, self.tmp.join("shot.png").to_str().unwrap()])
            .env("POCKETUI_MOCK", "1")
            .env("POCKETUI_DEVICE", "brick")
            .env("POCKETUI_BRICK_STANDALONE", "1")
            .env("POCKETUI_ES_SYSTEMS", self.tmp.join("lib/es_systems.cfg"))
            .env("POCKETUI_DATA", self.tmp.join("lib/data"))
            .env("POCKETUI_LANG", lang)
            .env("POCKETUI_TEXT_SIZE", size)
            .env("POCKETUI_TEXT_BOXES", &boxes)
            .env("POCKETUI_VISIBLE_TEXT", &visible)
            .env_remove("SWAYSOCK")
            .output()
            .expect("run pocket-ui");
        assert!(out.status.success(), "{state}: {}", String::from_utf8_lossy(&out.stderr));
        (std::fs::read_to_string(&visible).unwrap(), std::fs::read_to_string(&boxes).unwrap_or_default())
    }
}

const LANGS: [&str; 3] = ["en", "de", "es"];

/// The handheld is a Brick: the Tailscale, Sync, remote access and power pages said "the Pocket", in every language.
#[test]
fn the_bricks_pages_call_it_the_brick() {
    let card = Card::new();
    for lang in LANGS {
        for state in ["tailscale", "sync:off", "sys:power-noboot", "settings:4:4"] {
            let (shown, _) = card.shot(state, lang, "normal");
            // The German and Spanish line under Remote access names no device at all.
            assert!(shown.contains("Brick") || (state == "settings:4:4" && lang != "en"), "{lang} {state}: {shown}");
            assert!(!shown.contains("Pocket"), "{lang} {state}: {shown}");
        }
    }
}

/// SELECT + START on the Brick's own card: there is no NextUI on it, and pocket-ui only starts again. The dialog says so.
#[test]
fn leaving_on_the_bricks_own_card_says_it_restarts_pocket_ui() {
    let card = Card::new();
    for (lang, title) in [("en", "Restart pocket-ui?"), ("de", "pocket-ui neu starten?"), ("es", "¿Reiniciar pocket-ui?")] {
        let (shown, _) = card.shot("leave", lang, "normal");
        assert!(shown.lines().any(|l| l == title), "{lang}: {shown}");
        assert!(!shown.contains("NextUI"), "{lang}: {shown}");
    }
}

/// Nothing of ROCKNIX's or Android's on the Brick: About offered "Open EmulationStation…", the power menu how to start Android.
#[test]
fn the_brick_offers_no_emulationstation_and_no_android() {
    let card = Card::new();
    for lang in LANGS {
        let (about, _) = card.shot("about", lang, "normal");
        assert!(!about.contains("EmulationStation"), "{lang}: {about}");
        let (power, _) = card.shot("sys:power-noboot", lang, "normal");
        assert!(!power.contains("Android"), "{lang}: {power}");
    }
}

/// The first start called the Brick untested ("pocket-ui hasn't been tested on a TrimUI Brick yet"), and Updates showed a System line
/// "unknown" for TrimUI's firmware, which nothing on the card updates.
#[test]
fn the_first_start_and_updates_say_nothing_untrue_about_the_brick() {
    let card = Card::new();
    let (first, _) = card.shot("setup:first", "en", "normal");
    assert!(first.contains("Get started"), "{first}");
    assert!(!first.contains("tested"), "{first}");
    for lang in LANGS {
        let (updates, _) = card.shot("updates:note", lang, "normal");
        assert!(updates.contains("pocket-ui"), "{lang}: {updates}");
        assert!(!updates.lines().any(|l| ["System", "Sistema"].contains(&l)), "{lang}: {updates}");
    }
}

/// Sleep & power on the Brick: its note took two lines and the selected row's explanation was cut to one line with "…"; at Large
/// text the explanation ran under the hints.
#[test]
fn sleep_and_power_explains_each_row_in_full_on_the_brick() {
    let card = Card::new();
    for lang in LANGS {
        for state in ["sleep", "sleep:1", "sleep:3"] {
            let (shown, _) = card.shot(state, lang, "normal");
            assert!(!shown.lines().any(|l| l.ends_with('…') || l.ends_with("...")), "{lang} {state}: {shown}");
            let (_, boxes) = card.shot(state, lang, "large");
            assert!(boxes.lines().all(|l| !l.starts_with('O')), "{lang} {state} large: {boxes}");
        }
    }
}

/// Sync with no Wi-Fi: "Connect to Wi-Fi first" ran into the page's two-line foot at 1024x768, touched the hints under a one-line
/// foot (the Library data row), and in German ran off the screen.
#[test]
fn syncs_wifi_line_stays_clear_of_the_foot() {
    let card = Card::new();
    for lang in LANGS {
        for size in ["normal", "large"] {
            for state in ["sync:nowifi", "sync:nowifi:5"] {
                let (shown, boxes) = card.shot(state, lang, size);
                assert!(shown.contains("Wi-Fi") || shown.contains("WLAN"), "{lang} {size} {state}: {shown}");
                assert!(boxes.lines().all(|l| !l.starts_with('O') && !l.starts_with('E')), "{lang} {size} {state}: {boxes}");
            }
        }
    }
}

/// Test buttons: the Brick's own pad was drawn with two sticks and their readouts; it has none.
#[test]
fn test_buttons_draws_no_sticks_on_the_bricks_own_pad() {
    let card = Card::new();
    let (shown, _) = card.shot("test-buttons", "en", "normal");
    assert!(shown.contains("SELECT") && shown.contains("Player 1"), "{shown}");
    let readout = |l: &str| l.split_once(" · ").is_some_and(|(x, y)| x.trim_start_matches('-').parse::<i32>().is_ok() && y.trim_start_matches('-').parse::<i32>().is_ok());
    assert!(!shown.lines().any(readout), "{shown}");
}

/// The defaults chosen for the walk's open questions (06.11.): About doesn't call the Brick experimental; Performance has no fan rows
/// (the Brick has none); Sync offers no PS2, GameCube & Wii or 3DS folders; Backup names the card's folder as a computer sees it and
/// says nothing about flashing; the sleep line says what happens, without "yet"; Test buttons draws F1 and F2.
#[test]
fn the_bricks_pages_offer_only_what_the_brick_has() {
    let card = Card::new();
    for lang in LANGS {
        let (about, _) = card.shot("about", lang, "normal");
        assert!(about.contains("TrimUI Brick") && !about.contains("experiment"), "{lang}: {about}");
        let (perf, _) = card.shot("perf", lang, "normal");
        assert!(!["fan", "Lüfter", "ventilador", "Fan"].iter().any(|w| perf.contains(w)), "{lang}: {perf}");
        let (sync, _) = card.shot("sync:off:6", lang, "normal");
        assert!(!["PS2", "GameCube", "3DS"].iter().any(|w| sync.contains(w)), "{lang}: {sync}");
        let (backup, _) = card.shot("backup", lang, "normal");
        assert!(backup.contains("Roms/backup/pocket-ui-saves"), "{lang}: {backup}");
        assert!(!["Flashing", "Aufspielen", "grabarla", "PS2"].iter().any(|w| backup.contains(w)), "{lang}: {backup}");
        let (sleep, _) = card.shot("sleep", lang, "normal");
        assert!(!sleep.contains(" yet") && !sleep.contains("Noch kein") && !sleep.contains("Aún sin"), "{lang}: {sleep}");
    }
    let (pad, _) = card.shot("test-buttons", "en", "normal");
    assert!(pad.lines().any(|l| l == "F1") && pad.lines().any(|l| l == "F2"), "{pad}");
}

/// The manual on the Brick starts with the Brick (its own card, Roms/…, the power key, updates) and leaves out ROCKNIX's pages.
#[test]
fn the_manual_on_the_brick_starts_with_the_brick() {
    let card = Card::new();
    let (first, _) = card.shot("manual", "en", "normal");
    assert!(first.contains("Brick") && !first.contains("ROCKNIX"), "{first}");
}

/// Add games names each system's folder as a computer sees it on the Brick's card ("Roms/gba"), not "roms/gba": the card's es_systems
/// paths are under /storage/roms, which is the card's Roms folder.
#[test]
fn add_games_names_the_cards_roms_folders() {
    let card = Card::new();
    let cfg = card.tmp.join("lib/es_systems.cfg");
    let text = std::fs::read_to_string(&cfg).unwrap();
    let mut out = String::new();
    for part in text.split("<path>") {
        match part.split_once("</path>") {
            Some((path, rest)) if !out.is_empty() => {
                let system = path.rsplit('/').next().unwrap_or_default();
                out += &format!("<path>/storage/roms/{system}</path>{rest}");
            }
            _ => out += part,
        }
    }
    std::fs::write(&cfg, out).unwrap();
    let (shown, _) = card.shot("setup:games", "en", "normal");
    assert!(shown.contains("Roms/gba") && !shown.contains("roms/gba"), "{shown}");
}

