//! The three remaining 0.5 candidate text losses: whole words, whole hints, no overlaps.
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
            .args(["-I", &format!("{ROOT}/scripts/curator-fake-library.py"), dir.path().to_str().unwrap(), "dark"]).output().unwrap();
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        // A newly opened game, the exact long duration wording seen on the Brick. No save-derived case line.
        let recent = dir.path().join("data/recents.json");
        let mut rows: serde_json::Value = serde_json::from_slice(&fs::read(&recent).unwrap()).unwrap();
        rows[0]["system"] = "gba".into();
        rows[0]["name"] = "Advance Wars".into();
        rows[0]["path"] = dir.path().join("roms/gba/Advance Wars (USA) (Rev 1).gba").to_str().unwrap().into();
        rows[0]["last_played"] = (FIXTURE_NOW.parse::<u64>().unwrap() - 53 * 60).into();
        rows[0]["play_secs"] = 20.into();
        fs::write(recent, serde_json::to_vec(&rows).unwrap()).unwrap();
        fs::write(dir.path().join("data/open-case-told"), b"").unwrap();
        let socket = dir.path().join("wl");
        let stub = Command::new("python3").args(["-I", &format!("{ROOT}/scripts/wayland-stub.py"), socket.to_str().unwrap()]).spawn().unwrap();
        test_clock::wait_for(|| socket.exists(), std::time::Duration::from_secs(2), "Wayland stub");
        Self { dir, stub }
    }

    fn shot(&self, state: &str, device: &str, resolution: &str, lang: &str, font: &str, size: &str) -> (String, String, String) {
        self.shot_env(state, device, resolution, lang, font, size, &[])
    }

    #[allow(clippy::too_many_arguments)]
    fn shot_env(&self, state: &str, device: &str, resolution: &str, lang: &str, font: &str, size: &str, env: &[(&str, &str)]) -> (String, String, String) {
        let root = self.dir.path();
        let settings: serde_json::Value = serde_json::from_slice(&fs::read(root.join("data/settings.json")).unwrap()).unwrap();
        let look = settings["theme"].as_str().unwrap();
        let name = format!("{}-{look}-{device}-{resolution}-{lang}-{font}-{size}", state.replace(':', "-"));
        let png = root.join(format!("{name}.png"));
        let seen = root.join(format!("{name}.txt"));
        let boxes = root.join(format!("{name}.box"));
        let nav = root.join(format!("{name}.nav"));
        let bin = std::env::var_os("POCKETUI_NAV_BINARY").map(PathBuf::from).unwrap_or_else(|| env!("CARGO_BIN_EXE_pocket-ui").into());
        let out = Command::new(bin).args(["--snapshot", state]).arg(&png)
            .env("WAYLAND_DISPLAY", root.join("wl")).env_remove("SWAYSOCK")
            .env("POCKETUI_MOCK", "1").env("POCKETUI_MOCK_NOW", FIXTURE_NOW)
            .env("POCKETUI_DEVICE", device).env("POCKETUI_BRICK_STANDALONE", "1")
            .env("POCKETUI_ES_SYSTEMS", root.join("es_systems.cfg")).env("POCKETUI_DATA", root.join("data"))
            .env("POCKETUI_SNAPSHOT_FROZEN", "1").env("POCKETUI_SNAPSHOT_SIZE", resolution)
            .env("POCKETUI_LANG", lang).env("POCKETUI_FONT", font).env("POCKETUI_TEXT_SIZE", size)
            .env("POCKETUI_RETRO", if font == "retro" { "cocoa" } else { "off" })
            .env("POCKETUI_VISIBLE_TEXT", &seen).env("POCKETUI_TEXT_BOXES", &boxes).env("POCKETUI_NAV_PROBE", &nav)
            .envs(env.iter().copied())
            .output().unwrap();
        assert!(out.status.success(), "{name}: {}", String::from_utf8_lossy(&out.stderr));
        if let Some(proof) = std::env::var_os("POCKETUI_TEXT_FIT_PROOF") {
            let proof = PathBuf::from(proof);
            fs::create_dir_all(&proof).unwrap();
            for path in [&png, &seen, &boxes, &nav] { fs::copy(path, proof.join(path.file_name().unwrap())).unwrap(); }
        }
        (fs::read_to_string(seen).unwrap(), fs::read_to_string(boxes).unwrap(), fs::read_to_string(nav).unwrap())
    }

    fn look(&self, look: &str) {
        let path = self.dir.path().join("data/settings.json");
        let mut settings: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        settings["theme"] = look.into();
        fs::write(path, serde_json::to_vec(&settings).unwrap()).unwrap();
    }
}

fn matrix(mut check: impl FnMut(&Scene, &str, &str, &str) -> Vec<String>) {
    let scene = Scene::new();
    let mut bad = Vec::new();
    for lang in ["en", "de", "es", "pt", "fr", "it"] {
        for font in ["barlow", "retro"] {
            for size in ["normal", "large"] {
                bad.extend(check(&scene, lang, font, size).into_iter().map(|s| format!("{lang}/{font}/{size}: {s}")));
            }
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

fn faults(boxes: &str) -> Vec<String> {
    boxes.lines().filter(|s| s.starts_with(['O', 'E', 'T', 'V'])).map(str::to_string).collect()
}

#[test]
fn portal_sheet_keeps_whole_text_above_its_buttons_at_1280x720() {
    matrix(|scene, lang, font, size| {
        let (shown, boxes, _) = scene.shot("setup:wifi-portal-sheet", "smartpro", "1280x720", lang, font, size);
        let mut bad = faults(&boxes);
        let whole = shown.split_whitespace().collect::<Vec<_>>().join(" ");
        for source in ["This network wants a sign-in", "Hotel, train and café Wi-Fi often need a browser login or terms check before going online.",
            "pocket-ui checks again every 15 seconds and says when you're online.", "Sign in…", "Other ways", "Hotspot, front desk"] {
            let expected = i18n::tr_in(Lang::from_locale(lang), source);
            if !whole.contains(&expected.split_whitespace().collect::<Vec<_>>().join(" ")) { bad.push(format!("missing {expected:?}: {shown}")); }
        }
        bad
    });
}

#[cfg(feature = "brick")]
#[test]
fn brick_home_keeps_system_age_and_short_play_time_at_1024x768() {
    matrix(|scene, lang, font, size| {
        scene.look("cabinet");
        let (shown, boxes, _) = scene.shot("home:1", "brick", "1024x768", lang, font, size);
        let mut bad = faults(&boxes);
        let expected = i18n::tr_in(Lang::from_locale(lang), "Game Boy Advance, 53 min ago, under a minute played");
        let normalize = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ");
        if !normalize(&shown).contains(&normalize(&expected)) { bad.push(format!("missing whole {expected:?}: {shown}")); }
        bad
    });
}

#[cfg(not(feature = "brick"))]
#[test]
fn pmini_home_keeps_details_beside_quick_settings_settings_and_resume() {
    matrix(|scene, lang, font, size| {
        let mut bad = Vec::new();
        for (look, state) in [("shelf", "home:3"), ("cabinet", "home:1")] {
            scene.look(look);
            let (shown, boxes, nav) = scene.shot(state, "", "1240x1080", lang, font, size);
            bad.extend(faults(&boxes));
            for (key, label) in [("HOME", "Quick settings"), ("START", "Settings"), ("X", "Details"), ("A", "Continue")] {
                let label = i18n::tr_in(Lang::from_locale(lang), label);
                if !nav.contains(&format!("{}{}\t", pocket_ui::nav_probe::HINT, key)) || !shown.lines().any(|s| s == label) { bad.push(format!("{look}: missing {key} {label}: {shown}")); }
            }
        }
        bad
    });
}

/// Every UI language (the release check needs them all), both letters, both text sizes.
fn every_language(mut check: impl FnMut(&Scene, &str, &str, &str) -> Vec<String>) {
    let scene = Scene::new();
    let mut bad = Vec::new();
    for lang in ["en", "de", "es", "pt", "fr", "it", "ko", "zh", "vi", "ja"] {
        for font in ["barlow", "retro"] {
            for size in ["normal", "large"] {
                bad.extend(check(&scene, lang, font, size).into_iter().map(|s| format!("{lang}/{font}/{size}: {s}")));
            }
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

fn squeeze(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// 0.8 check video on the Brick (27.11.2026): Load's value at its last stop read "From the…" in the pause menu. The
/// value is whole on 1024x768, selected (with its arrows) and at rest, in every language, letters and size.
#[test]
fn pause_load_from_the_start_is_whole_at_1024x768() {
    every_language(|scene, lang, font, size| {
        let mut bad = Vec::new();
        for state in ["slot-restart:snes", "slot-restart"] {
            let (shown, boxes, _) = scene.shot(state, "brick", "1024x768", lang, font, size);
            bad.extend(faults(&boxes).into_iter().map(|f| format!("{state}: {f}")));
            // The window says it too: the row's value is the line after Load's name, and it is never cut short ("…").
            let (want, load) = (i18n::tr_in(Lang::from_locale(lang), "From the start"), i18n::tr_in(Lang::from_locale(lang), "Load"));
            let lines: Vec<&str> = shown.lines().map(str::trim).collect();
            // Its arrows, where they fit, are drawn as marks of their own around it.
            let value = lines.iter().position(|l| *l == load).and_then(|i| lines[i + 1..].iter().find(|l| **l != "‹")).copied().unwrap_or("");
            // Whole on Load's row (the video's case: English at the usual size, both letters); where a long translation
            // or Large text leaves the row no room beside the preview, a size smaller first, and the window says it
            // whole (as for every long value, `pause_page`).
            let whole = squeeze(value) == squeeze(&want);
            if lang == "en" && size == "normal" && !whole { bad.push(format!("{state}: Load's value {value:?}, not {want:?}")); }
            if !lines.iter().any(|l| squeeze(l) == squeeze(&want)) { bad.push(format!("{state}: {want:?} whole nowhere: {shown}")); }
        }
        bad
    });
}

/// 0.8 check video on the Brick: on the ScreenScraper row its long text pushed Real box backs and Real manuals off the
/// page, and the credit broke as "(CC" / "BY-NC-SA 4.0)" beside the hint bar. On 1024x768, every language, letters and
/// size, guest, running, signed in and at the day's limit: the four rows under ScreenScraper are on the page (at Large as
/// many as fit, the rest a scroll away) and the licence stays one piece.
#[test]
fn screenscraper_row_keeps_its_rows_and_licence_whole_at_1024x768() {
    every_language(|scene, lang, font, size| {
        let l = Lang::from_locale(lang);
        let mut bad = Vec::new();
        for demo in ["on", "running", "signed", "quota"] {
            let state = format!("setup:extras-screenscraper={demo}");
            let (shown, boxes, _) = scene.shot_env(&state, "brick", "1024x768", lang, font, size, &[("POCKETUI_SCREENSCRAPER_ROW", "1")]);
            bad.extend(faults(&boxes).into_iter().map(|f| format!("{demo}: {f}")));
            // At the usual size all five rows; at Large the list scrolls (`extras_page`): ScreenScraper and the rows
            // right under it as far as they fit, never fewer than two.
            let rows: &[&str] = if size == "normal" { &["ScreenScraper", "Real spines", "Real labels", "Real box backs", "Real manuals"] } else { &["ScreenScraper", "Real spines", "Real labels"] };
            for row in rows {
                let row = i18n::tr_in(l, row);
                if !shown.lines().any(|s| s.trim() == row) { bad.push(format!("{demo}: row {row:?} not on the page: {shown}")); }
            }
            // The credit in the interface's language, its licence's name never split over two lines.
            let credit = squeeze(&i18n::tr_in(l, "Data and media: ScreenScraper.fr (CC BY-NC-SA 4.0)"));
            let all = squeeze(&shown.replace('\n', " ")).replace("（ ", "（").replace(" ）", "）");
            if !all.contains(&credit) && !all.replace(" ", "").contains(&credit.replace(" ", "")) { bad.push(format!("{demo}: the credit {credit:?} missing: {shown}")); }
            if !shown.lines().any(|s| squeeze(s).contains("CC BY-NC-SA 4.0")) { bad.push(format!("{demo}: \"CC BY-NC-SA 4.0\" broken apart: {shown}")); }
        }
        bad
    });
}
