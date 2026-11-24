//! Render the Brick at its native resolution and check what reaches the screen.
#![cfg(feature = "brick")]
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::Command;

include!("common/root.rs");
include!("common/shelf_library.rs");

fn visible_on_brick(case: &str, lang: &str, size: &str, standalone: bool) -> String {
    let tmp = tempfile::tempdir().expect("snapshot folder");
    let stub = wayland(tmp.path());
    let lib = library_in(&tmp.path().join("lib"), None, "shelf");
    let text = tmp.path().join("visible.txt");
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
    if let Some(stub) = &stub {
        cmd.env("WAYLAND_DISPLAY", &stub.1);
    }
    let out = cmd.args(["--snapshot", case]).arg(tmp.path().join("shot.png"))
        .env("POCKETUI_DEVICE", "brick")
        .env("POCKETUI_BRICK_STANDALONE", if standalone { "1" } else { "" })
        .env("POCKETUI_MOCK", "1")
        .env("POCKETUI_MOCK_NOW", FIXTURE_NOW)
        .env("POCKETUI_LANG", lang)
        .env("POCKETUI_TEXT_SIZE", size)
        .env("POCKETUI_SNAPSHOT_SIZE", "1024x768")
        .env("POCKETUI_ES_SYSTEMS", lib.join("es_systems.cfg"))
        .env("POCKETUI_DATA", lib.join("data"))
        .env("POCKETUI_VISIBLE_TEXT", &text)
        .output().expect("render the Brick");
    assert!(out.status.success(), "{case}: {}", String::from_utf8_lossy(&out.stderr));
    std::fs::read_to_string(text).expect("visible text")
}

#[test]
fn brick_report_keeps_the_complete_privacy_note() {
    use pocket_ui::i18n::{tr_in, Lang};
    for (lang, code, size) in [(Lang::En, "en", "normal"), (Lang::De, "de", "large")] {
        let visible = visible_on_brick("report:saved", code, size, true);
        let note = tr_in(lang, pocket_ui::report::LEFT_OUT);
        assert!(visible.replace('\n', " ").contains(&note), "{code}: {visible}");
    }
}

#[test]
fn brick_search_shows_three_results_and_the_full_intro_above_the_keys() {
    let results = visible_on_brick("search:typing:mario", "en", "normal", true);
    assert!(results.lines().filter(|line| line.contains("Mario")).count() >= 3, "{results}");
    let fresh = visible_on_brick("search:fresh", "en", "normal", true);
    assert!(fresh.replace('\n', " ").contains("Small typos are fine."), "{fresh}");
}

#[test]
fn brick_gallery_shows_five_screenshots_and_the_count() {
    let visible = visible_on_brick("gallery", "en", "normal", true);
    let times = visible.lines().filter(|line| line.len() == 5 && line.as_bytes()[2] == b':').count();
    assert!(times >= 6, "clock and at least five screenshot times: {visible}");
    assert!(visible.contains("10 screenshots"), "{visible}");
}

#[test]
fn brick_pixel_path_keeps_stage_explanations_and_the_full_legend() {
    let normal = visible_on_brick("pixel-path:gba", "en", "normal", true);
    for line in ["Nothing over the picture.", "Resampled, rate-controlled, no gaps.", "Enhanced", "Compromised"] {
        assert!(normal.lines().any(|shown| shown == line), "{line}: {normal}");
    }
    let large = visible_on_brick("pixel-path:gba", "de", "large", true);
    assert!(large.lines().any(|line| line == "Beeinträchtigt"), "{large}");
}
