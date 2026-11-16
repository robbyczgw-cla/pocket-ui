//! Settings rows as a player with Large text on a 640x480 handheld meets them, in German and Spanish: the value on the
//! right of a row is in the player's language, whole.
//!
//! Escaped on 14.11.2026: Connections → PC streaming said "No PC paire…" in German. The value was cut to fit in English
//! and the piece painted, and a piece of a sentence is not in the translation table. Neither the translation check of
//! `tests/i18n_snapshots.rs` (default size, where the value fits and is translated) nor `tests/text_size.rs` (which
//! accepts a "…" cut) could see it. So here every `settings*` state of `tests/i18n-states.txt` is rendered in Large text
//! at 640x480, and
//! - no string with a translation in the table may be painted in English, whole or as a piece cut with "…"
//!   (`english_left`), and
//! - the PC streaming row is short enough to show whole.
//!
//! Needs python3 for scripts/wayland-stub.py on Linux without a compositor (as tests/game_check.rs).

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
    let path = tmp.join("wl");
    let child = Command::new("python3").args(["-I", &format!("{ROOT}/scripts/wayland-stub.py"), path.to_str().unwrap()]).spawn().expect("python3 for scripts/wayland-stub.py");
    let stub = Stub { path, child };
    test_clock::wait_for(|| stub.path.exists(), std::time::Duration::from_secs(5), "Wayland stub listening");
    Some(stub)
}

fn states() -> Vec<String> {
    std::fs::read_to_string(Path::new(&ROOT).join("tests/i18n-states.txt"))
        .expect("tests/i18n-states.txt")
        .lines()
        .map(str::trim)
        .filter(|l| l.starts_with("settings"))
        .map(str::to_string)
        .collect()
}

/// The strings painted in `state`, one per line, in `lang` at 640x480 in Large text.
/// `retro`: the Retro look (Cocoa, Departure Mono), whose letters are wider: the look most players pick.
fn painted(tmp: &Path, stub: &Option<Stub>, state: &str, lang: &str, retro: bool) -> Vec<String> {
    let data = tmp.join(format!("data-{lang}-{retro}-{}", state.replace(':', "_")));
    std::fs::create_dir_all(&data).unwrap();
    std::fs::write(data.join("settings.json"), r#"{"onboarded": true}"#).unwrap();
    let visible = data.join("visible.txt");
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
    cmd.args(["--snapshot", state, data.join("out.png").to_str().unwrap()])
        .env("POCKETUI_MOCK", "1")
        .env("POCKETUI_LANG", lang)
        .env("POCKETUI_TEXT_SIZE", "large")
        .env("POCKETUI_SNAPSHOT_SIZE", "640x480")
        .env("POCKETUI_VISIBLE_TEXT", &visible)
        .env("POCKETUI_DATA", &data);
    if retro {
        cmd.env("POCKETUI_RETRO", "cocoa");
    }
    if let Some(s) = stub {
        cmd.env("XDG_RUNTIME_DIR", tmp).env("WAYLAND_DISPLAY", &s.path);
    }
    let out = cmd.output().expect("pocket-ui runs");
    assert!(out.status.success(), "{state}: status={}\nstderr: {}", out.status, String::from_utf8_lossy(&out.stderr));
    std::fs::read_to_string(visible).unwrap_or_default().lines().map(|l| l.trim().to_string()).collect()
}

/// The English keys of `assets/i18n/<lang>.tsv` that are translated into something else (whole lines, no patterns).
fn translated_keys(lang: &str) -> Vec<String> {
    std::fs::read_to_string(Path::new(&ROOT).join(format!("assets/i18n/{lang}.tsv")))
        .expect("translation table")
        .lines()
        .filter_map(|l| l.split_once('\t'))
        .filter(|(en, tr)| en != tr && !en.contains('{') && !en.starts_with('!'))
        .map(|(en, _)| en.to_string())
        .collect()
}

/// Strings of `shown` that are English with a translation: a key of the table, or the start of one cut with "…"
/// (at least five letters, so "Aus…" is not "Auto-save"). `keys` is `translated_keys`.
fn english_left(shown: &[String], keys: &[String]) -> Vec<String> {
    let mut left = Vec::new();
    for s in shown {
        let core = s.trim_end_matches('…').trim();
        let cut = core.len() < s.trim().len();
        let hit = keys.iter().any(|k| k == core || (cut && core.chars().count() >= 5 && k.starts_with(core)));
        if hit && !left.contains(s) {
            left.push(s.clone());
        }
    }
    left
}

#[test]
fn the_detector_finds_an_english_value_and_a_piece_of_one() {
    let keys = ["No PC paired".to_string(), "Off".to_string(), "Not connected".to_string()];
    let shown = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    assert_eq!(english_left(&shown(&["Verbindungen", "No PC paire…", "Off", "Kein PC"]), &keys), ["No PC paire…", "Off"]);
    assert!(english_left(&shown(&["Kein PC", "Aus", "Nicht verbunden", "Not…"]), &keys).is_empty(), "German, and too short to be a piece");
}

#[test]
fn german_spanish_portuguese_french_and_italian_settings_in_large_text_show_no_english_where_a_translation_exists() {
    let dir = tempfile::Builder::new().prefix("svt-").tempdir().unwrap();
    let tmp = dir.path().to_path_buf();
    let stub = wayland(&tmp);
    let mut problems = Vec::new();
    for (lang, retro) in [("de", false), ("de", true), ("es", false), ("es", true), ("pt", false), ("pt", true), ("fr", false), ("fr", true), ("it", false), ("it", true)] {
        let keys = translated_keys(lang);
        let states = states();
        assert!(states.len() > 30, "the settings states are listed: {states:?}");
        std::thread::scope(|scope| {
            let found: Vec<_> = states
                .chunks(states.len().div_ceil(6))
                .map(|chunk| {
                    let (tmp, stub, keys) = (&tmp, &stub, &keys);
                    scope.spawn(move || {
                        chunk.iter().flat_map(|state| english_left(&painted(tmp, stub, state, lang, retro), keys).into_iter().map(move |s| format!("{lang} retro={retro} {state}: {s:?}"))).collect::<Vec<_>>()
                    })
                })
                .collect();
            for f in found {
                problems.extend(f.join().unwrap());
            }
        });
    }
    assert!(problems.is_empty(), "English painted where the table has a translation (640x480, Large text):\n{}", problems.join("\n"));
}

#[test]
fn the_pc_streaming_value_shows_whole_in_every_translated_language() {
    let dir = tempfile::Builder::new().prefix("svp-").tempdir().unwrap();
    let tmp = dir.path().to_path_buf();
    let stub = wayland(&tmp);
    for (lang, retro, row, value) in [("de", false, "PC-Streaming", "Kein PC"), ("de", true, "PC-Streaming", "Kein PC"), ("es", false, "Streaming desde PC", "Ningún PC"), ("es", true, "Streaming desde PC", "Ningún PC"), ("pt", false, "Streaming do PC", "Nenhum PC"), ("pt", true, "Streaming do PC", "Nenhum PC"), ("fr", false, "Streaming PC", "Aucun PC"), ("fr", true, "Streaming PC", "Aucun PC"), ("it", false, "Streaming dal PC", "Nessun PC"), ("it", true, "Streaming dal PC", "Nessun PC")] {
        let shown = painted(&tmp, &stub, "settings:0", lang, retro);
        let all = shown.join(" | ");
        assert!(shown.iter().any(|s| s.contains(row)), "{lang} retro={retro}: the row is there: {all}");
        assert!(shown.iter().any(|s| s.trim_end_matches(" >").trim() == value), "{lang} retro={retro}: the value {value:?} whole: {all}");
        assert!(!all.contains("No PC"), "{lang} retro={retro}: {all}");
    }
}
