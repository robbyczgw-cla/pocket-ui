//! RetroAchievements standing in for "the case knows where you are" (`achievements::case_line`, `app_achievements`,
//! `App::case_or_achievements`), as a player meets it on the made-up library (scripts/curator-fake-library.py) with a
//! made-up tally (`POCKETUI_SNAPSHOT_RA=34/60`: signed in, every game 34 of 60) and a made-up line from a save
//! (`POCKETUI_SNAPSHOT_PROGRESS`, one game's):
//! - the save's line comes first; a game whose save says nothing shows "34 of 60 achievements · 56%" on the box's back,
//!   beside Continue and on "Where was I?", and the box's "Unlocked" row doesn't say it again;
//! - signed out, without an unlock, or with Progress from saves off: no line, never "0 of 60", and the pages stay whole;
//! - it fits, translated, in every language at every size and in Large text.
//!
//! Needs python3 (and, on Linux without a compositor, scripts/wayland-stub.py), as tests/progress_from_saves.rs.

use std::path::{Path, PathBuf};
use std::process::Command;

include!("common/root.rs");

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
    test_clock::wait_for(|| stub.1.exists(), std::time::Duration::from_secs(2), "Wayland stub listening");
    Some(stub)
}

struct Ctx {
    tmp: PathBuf,
    stub: Option<Stub>,
    n: std::cell::Cell<u32>,
    /// Last, so the stub is gone before the folder is; removes the folder with the Ctx, also on a panic.
    _dir: tempfile::TempDir,
}

struct Shot {
    visible: Vec<String>,
    faults: Vec<String>,
    #[allow(dead_code)]
    data: PathBuf,
}

impl Ctx {
    /// `state` with `settings` (the part of settings.json after the look), `progress` the made-up line's case ("" for none).
    fn shot(&self, look: &str, state: &str, lang: &str, size: &str, settings: &str, progress: &str) -> Shot {
        self.shot_ra(look, state, (lang, "normal"), size, settings, progress, "34/60")
    }

    /// `ra` the made-up tally (`POCKETUI_SNAPSHOT_RA`, "unlocked/total"; "" for signed out).
    #[allow(clippy::too_many_arguments)]
    fn shot_ra(&self, look: &str, state: &str, (lang, text_size): (&str, &str), size: &str, settings: &str, progress: &str, ra: &str) -> Shot {
        self.n.set(self.n.get() + 1);
        let n = self.n.get();
        let (data, tmpdir) = (self.tmp.join(format!("data-{n}")), self.tmp.join(format!("tmp-{n}")));
        std::fs::create_dir_all(&data).unwrap();
        std::fs::create_dir_all(&tmpdir).unwrap();
        std::fs::copy(self.tmp.join("lib/data/recents.json"), data.join("recents.json")).unwrap();
        std::fs::write(data.join("settings.json"), format!(r#"{{"onboarded": true, "theme": "{look}"{settings}}}"#)).unwrap();
        let (visible, boxes) = (self.tmp.join(format!("{n}.txt")), self.tmp.join(format!("{n}.tsv")));
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
        cmd.args(["--snapshot", state, self.tmp.join(format!("{n}.png")).to_str().unwrap()])
            .env("POCKETUI_MOCK", "1").env("POCKETUI_MOCK_NOW", FIXTURE_NOW)
            .env("POCKETUI_LANG", lang)
            .env("POCKETUI_TEXT_SIZE", text_size)
            .env("POCKETUI_SNAPSHOT_SIZE", size)
            .env("POCKETUI_SNAPSHOT_FROZEN", "5")
            .env("POCKETUI_VISIBLE_TEXT", &visible)
            .env("POCKETUI_TEXT_BOXES", &boxes)
            .env("POCKETUI_DATA", &data)
            .env("POCKETUI_ES_SYSTEMS", self.tmp.join("lib/es_systems.cfg"))
            .env("TMPDIR", &tmpdir);
        if !ra.is_empty() {
            cmd.env("POCKETUI_SNAPSHOT_RA", ra);
        }
        if !progress.is_empty() {
            cmd.env("POCKETUI_SNAPSHOT_PROGRESS", progress);
        }
        if let Some(s) = &self.stub {
            cmd.env("WAYLAND_DISPLAY", &s.1);
        }
        let out = cmd.output().expect("run pocket-ui");
        assert!(out.status.success(), "{state}: {}", String::from_utf8_lossy(&out.stderr));
        let read = |p: &Path| std::fs::read_to_string(p).unwrap_or_default();
        Shot {
            visible: read(&visible).lines().map(str::to_string).collect(),
            faults: read(&boxes).lines().filter(|l| l.starts_with('E') || l.starts_with('O')).map(str::to_string).collect(),
            data,
        }
    }
}

fn has(v: &[String], s: &str) -> bool {
    v.iter().any(|l| l.contains(s))
}

/// What "Where was I?" says under its "Last played …" (the shelf behind the card has lines of its own).
fn on_the_card(v: &[String]) -> String {
    v.iter().skip_while(|l| !l.starts_with("Last played") && !l.starts_with("Zuletzt")).nth(1).cloned().unwrap_or_default()
}

fn context(name: &str) -> Ctx {
    let dir = tempfile::Builder::new().prefix(&format!("pocketui-ra-line-{name}-")).tempdir().unwrap();
    let tmp = dir.path().to_path_buf();
    let ok = Command::new("python3").env("POCKETUI_MOCK_NOW", FIXTURE_NOW)
        .args(["-I", &format!("{ROOT}/scripts/curator-fake-library.py"), tmp.join("lib").to_str().unwrap(), "dark"])
        .stdout(std::process::Stdio::null())
        .status()
        .expect("python3");
    assert!(ok.success());
    Ctx { stub: wayland(&tmp), tmp, n: Default::default(), _dir: dir }
}

const REMIND: &str = r#", "where_was_i": true"#;
const OFF: &str = r#", "extras": {"progress_from_saves": false}"#;
/// The made-up tally in English (`case_line::english`), its first part as a narrow screen may break it.
const RA: &str = "34 of 60 achievements";
const SAVE: &str = "Crono Lv. 24";

#[test]
fn the_save_comes_first_and_the_tally_stands_in_where_the_save_says_nothing() {
    let c = context("first");
    for look in ["shelf", "cabinet"] {
        // Chrono Trigger's save says where you are: that line, no tally in its place.
        let save = c.shot(look, "box:snes:chrono:r", "en", "1240x1080", "", "chrono");
        assert!(has(&save.visible, SAVE), "{look}: {:?}", save.visible);
        assert!(!has(&save.visible, RA), "{look}: the tally took the save's place: {:?}", save.visible);
        // Metroid Fusion's save says nothing here (the made-up line is Chrono Trigger's): the tally, once.
        let ra = c.shot(look, "box:gba:fusion:r", "en", "1240x1080", "", "chrono");
        assert!(has(&ra.visible, RA) && has(&ra.visible, "56%"), "{look}: no tally on the box's back: {:?}", ra.visible);
        assert_eq!(ra.visible.iter().filter(|l| l.contains("34 of 60")).count(), 1, "{look}: said twice: {:?}", ra.visible);
        assert!(!has(&ra.visible, "Unlocked"), "{look}: the Unlocked row repeats the line: {:?}", ra.visible);
        assert!(has(&ra.visible, "Fusion"), "{look}: {:?}", ra.visible);
        assert!(ra.faults.is_empty(), "{look}: {:?}", ra.faults);
        // Beside Continue: Chrono Trigger (DS) stands open on Home; with Pokémon's made-up line its save says nothing.
        let state = if look == "cabinet" { "home:3" } else { "home:5" };
        let home = c.shot(look, state, "en", "1240x1080", "", "pokemon");
        assert!(has(&home.visible, RA), "{look}: no tally beside Continue: {:?}", home.visible);
        let home = c.shot(look, state, "en", "1240x1080", "", "chrono");
        assert!(has(&home.visible, SAVE) && !has(&home.visible, RA), "{look}: the save's line first beside Continue: {:?}", home.visible);
        // "Where was I?" without a line from a save.
        let card = c.shot(look, "where-was-i", "en", "1240x1080", REMIND, "");
        assert!(on_the_card(&card.visible).contains(RA), "{look}: {:?}", card.visible);
        let card = c.shot(look, "where-was-i", "en", "1240x1080", REMIND, "chrono");
        assert!(on_the_card(&card.visible).contains(SAVE), "{look}: {:?}", card.visible);
    }
}

#[test]
fn no_line_signed_out_without_an_unlock_or_with_progress_off() {
    let c = context("none");
    for (why, settings, ra) in [("signed out", "", ""), ("nothing unlocked", "", "0/60"), ("Progress from saves off", OFF, "34/60")] {
        let back = c.shot_ra("shelf", "box:gba:fusion:r", ("en", "normal"), "1240x1080", settings, "chrono", ra);
        assert!(!has(&back.visible, "achievements ·") && !has(&back.visible, RA) && !has(&back.visible, "0 of 60 achievements"), "{why}: {:?}", back.visible);
        assert!(has(&back.visible, "Fusion") && has(&back.visible, "Your game"), "{why}: the back is still whole: {:?}", back.visible);
        assert!(back.faults.is_empty(), "{why}: {:?}", back.faults);
        let home = c.shot_ra("shelf", "home:5", ("en", "normal"), "1240x1080", settings, "pokemon", ra);
        assert!(!has(&home.visible, "achievements ·") && has(&home.visible, "played"), "{why}: Continue keeps its played-line: {:?}", home.visible);
        let card = c.shot_ra("shelf", "where-was-i", ("en", "normal"), "1240x1080", &format!("{REMIND}{settings}"), "", ra);
        assert!(has(&card.visible, "Last played") && !on_the_card(&card.visible).contains("achievements"), "{why}: {:?}", card.visible);
    }
    // A game without a set has no line either: the made-up tally is every game's, so the model says it (`case_line`).
}

#[test]
fn the_line_fits_translated_in_every_language_and_size() {
    let c = context("fit");
    let says = [("en", "achievements"), ("de", "Erfolgen"), ("fr", "succès"), ("es", "logros"), ("pt", "conquistas"), ("it", "obiettivi")];
    for (lang, word) in says {
        for (size, text_size) in [("640x480", "normal"), ("640x480", "large"), ("720x720", "normal"), ("1024x768", "normal"), ("1240x1080", "normal"), ("1920x1080", "large")] {
            let name = format!("{lang} {size} {text_size}");
            for state in ["box:gba:fusion:r", "where-was-i"] {
                let s = c.shot_ra("shelf", state, (lang, text_size), size, REMIND, "chrono", "1234/2000");
                assert!(has(&s.visible, word) && has(&s.visible, "1234") && has(&s.visible, "2000"), "{name} {state}: {:?}", s.visible);
                assert!(s.faults.is_empty(), "{name} {state}: cut or overflowing: {:?}", s.faults);
            }
        }
        let home = c.shot_ra("shelf", "home:5", (lang, "normal"), "640x480", "", "pokemon", "1234/2000");
        assert!(has(&home.visible, "1234"), "{lang} Continue: {:?}", home.visible);
        assert!(home.faults.is_empty(), "{lang} Continue: {:?}", home.faults);
    }
}
