//! The 0.8.0 "New in this version" cards (Real box pictures from ScreenScraper, Shelves by series) keep every word whole
//! in every language, both type styles and both text sizes, at 640x480, on the Brick's 1024x768 and a 1920x1080 screen:
//! the title, the line, the way to the setting and, on ScreenScraper's card, what it sends (`features::NOTES`), its four
//! kinds and its example's game and credit. A player who reads
//! half of the privacy line has not been told what is sent.
use std::{fs, path::PathBuf, process::Command};
use pocket_ui::i18n::{self, Lang};
include!("common/root.rs");

struct Scene {
    dir: tempfile::TempDir,
    stub: std::process::Child,
}
impl Drop for Scene {
    fn drop(&mut self) { let _ = self.stub.kill(); let _ = self.stub.wait(); }
}
impl Scene {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let out = Command::new("python3").env("POCKETUI_MOCK_NOW", FIXTURE_NOW)
            .args(["-I", &format!("{ROOT}/scripts/curator-fake-library.py"), dir.path().to_str().unwrap(),
                // Proof pictures in the other appearance: `POCKETUI_TEXT_FIT_APPEARANCE=light`.
                &std::env::var("POCKETUI_TEXT_FIT_APPEARANCE").unwrap_or_else(|_| "dark".into())]).output().unwrap();
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        let socket = dir.path().join("wl");
        let stub = Command::new("python3").args(["-I", &format!("{ROOT}/scripts/wayland-stub.py"), socket.to_str().unwrap()]).spawn().unwrap();
        test_clock::wait_for(|| socket.exists(), std::time::Duration::from_secs(2), "Wayland stub");
        Self { dir, stub }
    }

    /// The text seen and the text boxes' faults of one snapshot.
    fn shot(&self, state: &str, resolution: &str, lang: &str, font: &str, size: &str) -> (String, String) {
        let root = self.dir.path();
        let name = format!("{}-{resolution}-{lang}-{font}-{size}", state.replace([':', ','], "-"));
        let png = root.join(format!("{name}.png"));
        let seen = root.join(format!("{name}.txt"));
        let boxes = root.join(format!("{name}.box"));
        let bin = std::env::var_os("POCKETUI_NAV_BINARY").map(PathBuf::from).unwrap_or_else(|| env!("CARGO_BIN_EXE_pocket-ui").into());
        let out = Command::new(bin).args(["--snapshot", state]).arg(&png)
            .env("WAYLAND_DISPLAY", root.join("wl")).env_remove("SWAYSOCK")
            .env("POCKETUI_MOCK", "1").env("POCKETUI_MOCK_NOW", FIXTURE_NOW).env("POCKETUI_SCREENSCRAPER_ROW", "1")
            .env("POCKETUI_ES_SYSTEMS", root.join("es_systems.cfg")).env("POCKETUI_DATA", root.join("data"))
            .env("POCKETUI_SNAPSHOT_FROZEN", "1").env("POCKETUI_SNAPSHOT_SIZE", resolution)
            .env("POCKETUI_LANG", lang).env("POCKETUI_FONT", font).env("POCKETUI_TEXT_SIZE", size)
            .env("POCKETUI_RETRO", if font == "retro" { "cocoa" } else { "off" })
            .env("POCKETUI_VISIBLE_TEXT", &seen).env("POCKETUI_TEXT_BOXES", &boxes)
            .output().unwrap();
        assert!(out.status.success(), "{name}: {}", String::from_utf8_lossy(&out.stderr));
        if let Some(proof) = std::env::var_os("POCKETUI_TEXT_FIT_PROOF") {
            let proof = PathBuf::from(proof);
            fs::create_dir_all(&proof).unwrap();
            for path in [&png, &seen, &boxes] { fs::copy(path, proof.join(path.file_name().unwrap())).unwrap(); }
        }
        (fs::read_to_string(seen).unwrap(), fs::read_to_string(boxes).unwrap())
    }
}

/// Without any white space: wrapped CJK lines join without a space, Latin ones with one.
fn squeezed(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}

#[test]
fn the_0_8_cards_keep_title_line_place_and_note_whole_in_every_language() {
    let scene = Scene::new();
    // The first start's Real box pictures page is the same painter (Your games › its row › A; the fixture's fourth row).
    let cards: [(&str, &str); 3] = [("tour-new:screenscraper,shelf_by_series", "screenscraper"), ("tour-new:shelf_by_series", "shelf_by_series"),
        ("setup:first-games:ddda", "screenscraper")];
    let mut bad = Vec::new();
    for lang in ["en", "de", "es", "pt", "fr", "it", "ko", "zh", "ja", "vi"] {
        let l = Lang::from_locale(lang);
        for (state, id) in cards {
            let f = pocket_ui::features::FEATURES.iter().find(|f| f.id == id).unwrap();
            let mut want: Vec<String> = vec![f.title.into(), f.line.into()];
            want.extend(pocket_ui::features::note(f).map(String::from));
            if id == "screenscraper" {
                // The four kinds to tick, the example's game and its art's credit (owner 27.11.2026). The way to the
                // setting may give way on a short screen: the rows are the setting's own rows.
                want.extend(pocket_ui::screenscraper::PICTURE_NAMES.iter().map(|n| n.to_string()));
                want.push(format!("Example: {}", pocket_ui::features::SAMPLE_GAME));
                want.push(pocket_ui::features::SAMPLE_CREDIT.into());
                want.push(pocket_ui::features::NOTHING_BEFORE_START.into());
            } else {
                want.push(f.place.into());
            }
            for resolution in ["640x480", "1024x768", "1920x1080"] {
                for font in ["barlow", "retro"] {
                    for size in ["normal", "large"] {
                        let (shown, boxes) = scene.shot(state, resolution, lang, font, size);
                        let at = format!("{lang}/{resolution}/{font}/{size} {state}");
                        if state.starts_with("setup:") && !shown.contains(&i18n::tr_in(l, "Real spines")) {
                            bad.push(format!("{at}: the first start's Real box pictures page did not open"));
                        }
                        bad.extend(boxes.lines().filter(|s| s.starts_with(['O', 'E', 'T', 'V'])).map(|s| format!("{at}: {s}")));
                        let all = squeezed(&shown);
                        for source in &want {
                            let expected = if source == pocket_ui::features::SAMPLE_CREDIT { source.clone() } else { i18n::tr_in(l, source) };
                            if !all.contains(&squeezed(&expected)) {
                                bad.push(format!("{at}: missing whole {expected:?}"));
                            }
                        }
                    }
                }
            }
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}
