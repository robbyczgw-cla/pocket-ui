//! Settings → Look → Cozy extras → Progress from saves (`cozy_extras::Extra::ProgressFromSaves`, `app_case_knows`) as a
//! player meets it, on the made-up library (scripts/curator-fake-library.py) with a made-up line from the game's save
//! (`POCKETUI_SNAPSHOT_PROGRESS=chrono`: "Crono Lv. 24 …" for every Chrono Trigger):
//! - as it starts the line is on the box's back, beside Continue and on "Where was I?";
//! - switched off in settings.json there is no line in any of the three, and nothing else of those pages goes missing;
//! - the row is on the page in every language, fits at 640x480 in Large text, and a press on it is saved and kept;
//! - nothing about the reading thread is asked once it is off (`app_case_knows::tests`, `App::case_progress`).
//!
//! Needs python3 (and, on Linux without a compositor, scripts/wayland-stub.py), as tests/cozy_extras.rs.

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
    data: PathBuf,
}

impl Ctx {
    /// `state` with `settings` (the part of settings.json after the look), `progress` the made-up line's case ("" for none).
    fn shot(&self, look: &str, state: &str, lang: &str, size: &str, settings: &str, progress: &str) -> Shot {
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
            .env("POCKETUI_SNAPSHOT_SIZE", size)
            .env("POCKETUI_SNAPSHOT_FROZEN", "5")
            .env("POCKETUI_VISIBLE_TEXT", &visible)
            .env("POCKETUI_TEXT_BOXES", &boxes)
            .env("POCKETUI_DATA", &data)
            .env("POCKETUI_ES_SYSTEMS", self.tmp.join("lib/es_systems.cfg"))
            .env("TMPDIR", &tmpdir);
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

fn context(name: &str) -> Ctx {
    let dir = tempfile::Builder::new().prefix(&format!("pocketui-progress-saves-{name}-")).tempdir().unwrap();
    let tmp = dir.path().to_path_buf();
    let ok = Command::new("python3").env("POCKETUI_MOCK_NOW", FIXTURE_NOW)
        .args(["-I", &format!("{ROOT}/scripts/curator-fake-library.py"), tmp.join("lib").to_str().unwrap(), "dark"])
        .stdout(std::process::Stdio::null())
        .status()
        .expect("python3");
    assert!(ok.success());
    Ctx { stub: wayland(&tmp), tmp, n: Default::default(), _dir: dir }
}

/// Where was I? is Off until switched on (0.5); the shots of its card switch it on.
const REMIND: &str = r#", "where_was_i": true"#;
const OFF: &str = r#", "extras": {"progress_from_saves": false}"#;
/// What the made-up Chrono Trigger line says in English (`case_knows_read::line`).
const LINE: &str = "Crono Lv. 24";

/// The line is on the box's back, beside Continue and on "Where was I?" as it starts, and in none of them when off; the
/// rest of each page stays.
#[test]
fn off_there_is_no_line_on_the_box_beside_continue_or_on_where_was_i() {
    let c = context("three");
    for (look, size) in [("shelf", "640x480"), ("cabinet", "1240x1080"), ("split", "1240x1080")] {
        // The box's back (where the whole back fits: at 640x480 it scrolls and the line may be below the first screen).
        let on = c.shot(look, "box:snes:chrono:r", "en", "1240x1080", "", "chrono");
        assert!(has(&on.visible, LINE), "{look}: the box's back says where you are by default: {:?}", on.visible);
        let off = c.shot(look, "box:snes:chrono:r", "en", "1240x1080", OFF, "chrono");
        assert!(!has(&off.visible, LINE) && !has(&off.visible, "Lv. 24"), "{look}: off, the box's back says nothing: {:?}", off.visible);
        assert!(has(&off.visible, "Developer") && has(&off.visible, "Square"), "{look}: the box's back is still there, not a gap: {:?}", off.visible);
        assert!(off.faults.is_empty(), "{look}: {:?}", off.faults);
        // "Where was I?".
        let on = c.shot(look, "where-was-i", "en", size, REMIND, "chrono");
        assert!(has(&on.visible, LINE), "{look} {size}: Where was I? has the line by default: {:?}", on.visible);
        let off = c.shot(look, "where-was-i", "en", size, &format!("{REMIND}{OFF}"), "chrono");
        assert!(!has(&off.visible, LINE), "{look} {size}: off, Where was I? has no line: {:?}", off.visible);
        assert!(has(&off.visible, "Last played"), "{look} {size}: its card is still there: {:?}", off.visible);
        assert!(off.faults.is_empty(), "{look} {size}: {:?}", off.faults);
    }
    // Beside Continue: the third recent game of the made-up library is Chrono Trigger (DS), standing open.
    for look in ["shelf", "cabinet", "split"] {
        let state = if look == "cabinet" { "home:3" } else { "home:5" };
        let on = c.shot(look, state, "en", "1240x1080", "", "chrono");
        assert!(has(&on.visible, LINE), "{look}: the line stands beside Continue by default: {:?}", on.visible);
        let off = c.shot(look, state, "en", "1240x1080", OFF, "chrono");
        assert!(!has(&off.visible, LINE), "{look}: off, Continue's line is the played-line again: {:?}", off.visible);
        assert!(has(&off.visible, "played"), "{look}: and says what it said before the line: {:?}", off.visible);
        assert!(off.faults.is_empty(), "{look}: {:?}", off.faults);
    }
}

/// On the page in every language, at 640x480 in Large text with nothing cut, saved with a press and kept at the next start.
#[test]
fn the_row_is_on_the_page_in_every_language_fits_large_text_and_is_kept() {
    let c = context("page");
    for (lang, label) in [("en", "Progress from saves"), ("de", "Fortschritt aus Spielständen"), ("es", "Progreso de tus partidas"), ("pt", "Progresso dos saves"), ("fr", "Progrès des saves"), ("it", "Progressi dai save")] {
        for (size, text) in [("640x480", r#", "text_size": "large""#), ("640x480", ""), ("1240x1080", "")] {
            // Down to the row (it is the fourteenth of the page since the TV and the handheld rows, 19.11.2026), selected.
            let s = c.shot("shelf", "cozy:13", lang, size, text, "");
            assert!(s.visible.iter().any(|l| l.starts_with(&label[..label.len().min(14)])), "{lang} {size} {text}: the row {label:?}: {:?}", s.visible);
            assert!(s.faults.is_empty(), "{lang} {size} {text}: {:?}", s.faults);
        }
    }
    // The row's line says what it does, in German too.
    let s = c.shot("shelf", "cozy:13", "de", "640x480", r#", "text_size": "large""#, "");
    assert!(has(&s.visible, "spoilerfrei"), "the German line: {:?}", s.visible);
    // A on it is saved at once; the next start has it off, and the box's back says nothing.
    let s = c.shot("shelf", "cozy:13::a", "en", "640x480", "", "");
    let saved: serde_json::Value = serde_json::from_slice(&std::fs::read(s.data.join("settings.json")).unwrap()).unwrap();
    assert_eq!(saved["extras"], serde_json::json!({"progress_from_saves": false}), "saved like the other extras");
    // A again switches it on and the start writes nothing for it.
    let s = c.shot("shelf", "cozy:13::aa", "en", "640x480", "", "");
    let saved: serde_json::Value = serde_json::from_slice(&std::fs::read(s.data.join("settings.json")).unwrap()).unwrap();
    assert!(saved["extras"].is_null(), "back to the start writes nothing: {saved}");
    // The setting a start reads: off in settings.json, the box's back has no line; the default has it.
    let off = c.shot("shelf", "box:snes:chrono:r", "en", "1240x1080", OFF, "chrono");
    assert!(!has(&off.visible, LINE));
    let on = c.shot("shelf", "box:snes:chrono:r", "en", "1240x1080", "", "chrono");
    assert!(has(&on.visible, LINE));
}
