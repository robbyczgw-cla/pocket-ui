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
