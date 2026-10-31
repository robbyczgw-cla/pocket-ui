//! Settings in seven categories (2026-10-30) as the player meets them: every choice is reached
//! by plain rows (START, a category, its row), nothing only on a button. Each category's list is rendered with each of
//! its rows selected, in English and German, at 1280x720 and at 640x480 with Large text (the H700 handhelds), and the
//! row's name must be painted whole on its row (`POCKETUI_VISIBLE_TEXT`) with no text cut by an edge or running into
//! other text (`POCKETUI_TEXT_BOXES`). The Buttons page shows which button confirms, and Systems on the shelf without
//! games says so instead of staying empty. The categories are bare one-line rows, all seven on one page also at 640x480
//! with Large text, the selected one's line whole at the bottom; Cozy extras and Sleep & power show each row whole too. On the made-up library (scripts/curator-fake-library.py). Needs python3 (scripts/wayland-stub.py on Linux without a compositor).

use std::path::{Path, PathBuf};
use std::process::Command;

const ROOT: &str = env!("CARGO_MANIFEST_DIR");

/// The tree as the player sees it without a store catalog: category, then its rows.
const TREE: [(&str, &[&str]); 7] = [
    ("Look", &["Look", "Appearance", "Colours", "Font", "Text size", "Night light", "Systems on the shelf", "Cozy extras"]),
    ("Sound & feel", &["Animations", "Playful cases", "Old-TV switch-on", "Motion", "Sounds", "Case sounds", "Haptics", "LEDs"]),
    ("Playing", &["On start", "Remember where I quit", "Buttons", "Over the game", "Screen frames", "Performance & fan", "Battery", "RetroAchievements"]),
    ("Games", &["Add games", "Starter pack", "Pictures & extras", "From EmulationStation", "Screenshots", "Curator", "PC streaming"]),
    ("Connections", &["Wi-Fi", "Bluetooth", "Tailscale", "Sync", "Remote access (SSH)", "Web companion"]),
    ("System", &["Language", "Time", "Sleep & power", "Updates", "Storage", "Backup & reset"]),
    ("Help & tools", &["Tour", "Manual", "Tools", "Report a problem", "About"]),
];

struct Stub(std::process::Child, PathBuf);

impl Drop for Stub {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn wayland(tmp: &Path) -> Option<Stub> {
    if !cfg!(target_os = "linux") || std::env::var_os("WAYLAND_DISPLAY").is_some_and(|w| !w.is_empty()) {
        return None;
    }
    let path = tmp.join("wl");
    let child = Command::new("python3").args(["-I", &format!("{ROOT}/scripts/wayland-stub.py"), path.to_str().unwrap()]).spawn().expect("python3");
    let stub = Stub(child, path);
    for _ in 0..200 {
        if stub.1.exists() {
            return Some(stub);
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    panic!("scripts/wayland-stub.py didn't start");
}

/// The German line of `en` in assets/i18n/de.tsv (the Look category's key ends with a zero-width mark).
fn german(en: &str) -> String {
    let table = std::fs::read_to_string(format!("{ROOT}/assets/i18n/de.tsv")).unwrap();
    table
        .lines()
        .filter_map(|l| l.split_once('\t'))
        .find(|(k, _)| *k == en)
        .map(|(_, v)| v.to_string())
        .unwrap_or_else(|| en.to_string())
}

/// The rows of Settings → Look → Cozy extras and System → Sleep & power, in their order.
const COZY: [&str; 8] = ["TV on the cabinet", "Stickers on cases", "Printed discs", "Memory card in the pause menu", "Light by time of day", "Decorations", "Game connections", "Rediscover"];
const SLEEP: [&str; 6] = ["Screen off after", "Slideshow before screen off", "Sleep after", "Switch off after sleeping", "Short press", "Hold"];

/// The line naming what is in each category, as at the bottom of the categories (the Look key ends with a zero-width mark).
const LINES: [&str; 7] = [
    "Look, appearance, colours, font, text size, night light, systems, cozy extras.",
    "Animations, playful cases, old-TV switch-on, motion, sounds, case sounds, haptics, LEDs.",
    "On start, quit, buttons, over the game, frames, performance, battery, achievements.",
    // With the Store, as the German table has it; a test run has no catalog, so its last part is left out (below).
    "Games, starter pack, pictures, EmulationStation, screenshots, Curator, PC, Store.",
    "Wi-Fi, Bluetooth, Tailscale, Sync, remote access, web companion.",
    "Language, time, sleep & power, updates, storage, backup & reset.",
    "Tour, manual, tools, report a problem, about.",
];

/// What one snapshot painted: the visible strings and the text-box faults (`E` cut, `O` overlapping).
struct Shot {
    visible: Vec<String>,
    faults: Vec<String>,
}

fn shot(tmp: &Path, stub: &Option<Stub>, state: &str, lang: &str, size: &str, text: &str, games: bool) -> Shot {
    let name = format!("{}-{lang}-{size}-{text}", state.replace(':', "_"));
    let data = tmp.join(&name);
    std::fs::create_dir_all(&data).unwrap();
    std::fs::write(data.join("settings.json"), r#"{"onboarded": true}"#).unwrap();
    let (visible, boxes) = (tmp.join(format!("{name}.txt")), tmp.join(format!("{name}.tsv")));
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
    cmd.args(["--snapshot", state, tmp.join(format!("{name}.png")).to_str().unwrap()])
        .env("POCKETUI_MOCK", "1")
        .env("POCKETUI_LANG", lang)
        .env("POCKETUI_TEXT_SIZE", text)
        .env("POCKETUI_SNAPSHOT_SIZE", size)
        .env("POCKETUI_VISIBLE_TEXT", &visible)
        .env("POCKETUI_TEXT_BOXES", &boxes)
        .env("POCKETUI_DATA", &data);
    // The made-up library (scripts/curator-fake-library.py), so the rows' values are as long as a real one's; or none.
    cmd.env("POCKETUI_ES_SYSTEMS", if games { tmp.join("lib/es_systems.cfg") } else { tmp.join("none.cfg") });
    if let Some(s) = stub {
        cmd.env("WAYLAND_DISPLAY", &s.1);
    }
    let out = cmd.output().expect("run pocket-ui");
    assert!(out.status.success(), "{state}: {}", String::from_utf8_lossy(&out.stderr));
    let read = |p: &Path| std::fs::read_to_string(p).unwrap_or_default();
    Shot {
        visible: read(&visible).lines().map(str::to_string).collect(),
        // The hint bar giving way on a narrow screen is by design (`kit::pill_hints`); text cut or run together is not.
        faults: read(&boxes).lines().filter(|l| l.starts_with('E') || l.starts_with('O')).map(str::to_string).collect(),
    }
}

#[test]
fn every_choice_is_a_row_and_shows_whole() {
    let tmp = std::env::temp_dir().join(format!("pocketui-settings-ia-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    std::fs::write(tmp.join("none.cfg"), "<systemList></systemList>").unwrap();
    let ok = Command::new("python3")
        .args(["-I", &format!("{ROOT}/scripts/curator-fake-library.py"), tmp.join("lib").to_str().unwrap(), "dark"])
        .stdout(std::process::Stdio::null())
        .status()
        .expect("python3 for scripts/curator-fake-library.py");
    assert!(ok.success());
    let stub = wayland(&tmp);
    let mut failed = Vec::new();
    for (lang, size, text) in [("en", "1280x720", "normal"), ("de", "1280x720", "normal"), ("de", "640x480", "large")] {
        let tr = |s: &str| if lang == "de" { german(if s == "Look" { "Look\u{200b}" } else { s }) } else { s.to_string() };
        // The categories, all seven on one page (bare rows, NextUI-style) on every screen and text size, the selected
        // one's line whole at the bottom (wrapped, never cut with "…").
        for g in 0..TREE.len() {
            let page = shot(&tmp, &stub, &format!("settings:{g}"), lang, size, text, true);
            for (cat, _) in TREE {
                if !page.visible.iter().any(|v| v.trim_end_matches('\u{200b}') == tr(cat)) {
                    failed.push(format!("{lang} {size} {text}: the categories don't show {cat:?}: {:?}", page.visible));
                }
            }
            let mut line = tr(LINES[g]);
            if LINES[g].ends_with(", Store.") {
                line = format!("{}.", line.rsplit_once(", ").unwrap().0);
            }
            // Wrapped at spaces: the painted lines one after another are the whole line.
            if !page.visible.join(" ").contains(&line) || page.visible.iter().any(|v| v.ends_with('…')) {
                failed.push(format!("{lang} {size} {text} category {g}: its line {line:?} isn't shown whole: {:?}", page.visible));
            }
            failed.extend(page.faults.iter().map(|f| format!("{lang} {size} {text} categories:{g}: {f}")));
        }
        // Cozy extras and Sleep & power: each row whole when selected (8 rows at Large text on 640x480 scroll, as every
        // list of 8 does), its line never cut.
        for (state, rows) in [("cozy", &COZY[..]), ("sleep", &SLEEP[..])] {
            for (r, row) in rows.iter().enumerate() {
                let s = shot(&tmp, &stub, &format!("{state}:{r}"), lang, size, text, true);
                if !s.visible.iter().any(|v| *v == tr(row)) {
                    failed.push(format!("{lang} {size} {text} {state}:{r}: row {row:?} not shown whole: {:?}", s.visible));
                }
                if s.visible.iter().any(|v| v.ends_with('…')) {
                    failed.push(format!("{lang} {size} {text} {state}:{r} ({row}): a line is cut: {:?}", s.visible));
                }
                failed.extend(s.faults.iter().map(|f| format!("{lang} {size} {text} {state}:{r}: {f}")));
            }
        }
        for (g, (cat, rows)) in TREE.iter().enumerate() {
            for (r, row) in rows.iter().enumerate() {
                let state = format!("settings:{g}:{r}");
                let s = shot(&tmp, &stub, &state, lang, size, text, true);
                let (cat_name, row_name) = (if *cat == "Look" { german("Look\u{200b}") } else { tr(cat) }, if lang == "de" { german(row) } else { row.to_string() });
                let cat_name = if lang == "de" { cat_name } else { cat.to_string() };
                if !s.visible.iter().any(|v| v.trim_end_matches('\u{200b}') == cat_name) {
                    failed.push(format!("{lang} {size} {text} {state}: title {cat_name:?} not shown"));
                }
                if !s.visible.iter().any(|v| *v == row_name) {
                    failed.push(format!("{lang} {size} {text} {state}: row {row_name:?} not shown whole: {:?}", s.visible));
                }
                // The row's line at the bottom is never cut short.
                if s.visible.iter().any(|v| v.ends_with('…') && !v.starts_with('…')) {
                    failed.push(format!("{lang} {size} {text} {state}: a line is cut: {:?}", s.visible));
                }
                failed.extend(s.faults.iter().map(|f| format!("{lang} {size} {text} {state}: {f}")));
            }
        }
        // Playing → Buttons: both rows, and which button confirms, in words and on the picture.
        for state in ["buttons:0", "buttons:1:r"] {
            let s = shot(&tmp, &stub, state, lang, size, text, true);
            for want in ["Button layout", "Button symbols", "Confirm", "Back"] {
                if !s.visible.iter().any(|v| *v == tr(want)) {
                    failed.push(format!("{lang} {size} {text} {state}: {want:?} not shown: {:?}", s.visible));
                }
            }
            failed.extend(s.faults.iter().map(|f| format!("{lang} {size} {text} {state}: {f}")));
        }
        // Systems on the shelf without any games: a line says so, the page is never empty.
        let s = shot(&tmp, &stub, "setup:look-systems", lang, size, text, false);
        let empty = tr("No systems with games yet. Once games are on the SD card, their systems show up here.");
        let first_word = empty.split_whitespace().next().unwrap().to_string();
        if !s.visible.iter().any(|v| v.starts_with(&first_word)) {
            failed.push(format!("{lang} {size} {text} setup:look-systems without games: no line saying so: {:?}", s.visible));
        }
    }
    let _ = std::fs::remove_dir_all(&tmp);
    assert!(failed.is_empty(), "{} problem(s):\n{}", failed.len(), failed.join("\n"));
}

/// What the player typed is painted as it is, also in German: a search for "unknown" shows "unknown" in its field
/// (it showed "unbekannt", the German table's word, while the line under it said "unknown"), and a network called
/// "Unknown" keeps its name on the Wi-Fi page. The keyboard's own field was already painted untranslated.
#[test]
fn typed_text_is_painted_as_typed() {
    let tmp = std::env::temp_dir().join(format!("pocketui-typed-text-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    std::fs::write(tmp.join("none.cfg"), "<systemList></systemList>").unwrap();
    let ok = Command::new("python3")
        .args(["-I", &format!("{ROOT}/scripts/curator-fake-library.py"), tmp.join("lib").to_str().unwrap(), "dark"])
        .stdout(std::process::Stdio::null())
        .status()
        .expect("python3 for scripts/curator-fake-library.py");
    assert!(ok.success());
    let stub = wayland(&tmp);
    assert_eq!(german("unknown"), "unbekannt", "the word is in the German table, so the test means something");
    let s = shot(&tmp, &stub, "search:results:unknown", "de", "1240x1080", "normal", true);
    assert!(s.visible.iter().any(|v| v == "unknown"), "the search field shows what was typed: {:?}", s.visible);
    assert!(!s.visible.iter().any(|v| v.contains("unbekannt")), "nothing typed is translated: {:?}", s.visible);
    let s = shot(&tmp, &stub, "setup:wifi-named=Unknown", "de", "1240x1080", "normal", true);
    assert!(s.visible.iter().any(|v| v == "Unknown"), "a network keeps its name: {:?}", s.visible);
    assert!(!s.visible.iter().any(|v| v == "Unbekannt"), "{:?}", s.visible);
    let _ = std::fs::remove_dir_all(&tmp);
}
