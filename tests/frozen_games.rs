//! Frozen games on the shelf as a player sees them: the recent games of the made-up library (scripts/curator-fake-library.py)
//! stand open with POCKETUI_SNAPSHOT_FROZEN, in every look, dark and light, English and German: the word or the Resume hint
//! is there, with Frozen games off none of it is, and a shelf where nothing is frozen shows what it always showed. What may
//! be frozen at all is `frozen::tests`. Needs python3 (scripts/wayland-stub.py on Linux without a compositor).

use std::path::{Path, PathBuf};
use std::process::Command;

const ROOT: &str = env!("CARGO_MANIFEST_DIR");

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

struct Ctx {
    tmp: PathBuf,
    stub: Option<Stub>,
    n: std::cell::Cell<u32>,
}

impl Ctx {
    /// The strings a snapshot painted and its text faults. `frozen`: how many recent games stand open.
    fn shot(&self, look: &str, state: &str, frozen: usize, lang: &str, extras: &str) -> (Vec<String>, Vec<String>) {
        self.n.set(self.n.get() + 1);
        let data = self.tmp.join(format!("data-{}", self.n.get()));
        std::fs::create_dir_all(&data).unwrap();
        std::fs::copy(self.tmp.join("lib/data/recents.json"), data.join("recents.json")).unwrap();
        std::fs::write(data.join("settings.json"), format!(r#"{{"onboarded": true, "theme": "{look}", "extras": {extras}}}"#)).unwrap();
        let (visible, boxes) = (self.tmp.join(format!("{}.txt", self.n.get())), self.tmp.join(format!("{}.tsv", self.n.get())));
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
        cmd.args(["--snapshot", state, self.tmp.join(format!("{}.png", self.n.get())).to_str().unwrap()])
            .env("POCKETUI_MOCK", "1")
            .env("POCKETUI_LANG", lang)
            .env("POCKETUI_VISIBLE_TEXT", &visible)
            .env("POCKETUI_TEXT_BOXES", &boxes)
            .env("POCKETUI_DATA", &data)
            .env("POCKETUI_SNAPSHOT_FROZEN", frozen.to_string())
            .env("POCKETUI_ES_SYSTEMS", self.tmp.join("lib/es_systems.cfg"));
        if let Some(s) = &self.stub {
            cmd.env("WAYLAND_DISPLAY", &s.1);
        }
        let out = cmd.output().expect("run pocket-ui");
        assert!(out.status.success(), "{state}: {}", String::from_utf8_lossy(&out.stderr));
        let read = |p: &Path| std::fs::read_to_string(p).unwrap_or_default();
        (read(&visible).lines().map(str::to_string).collect(), read(&boxes).lines().filter(|l| l.starts_with('E') || l.starts_with('O')).map(str::to_string).collect())
    }
}

fn has(v: &[String], s: &str) -> bool {
    v.iter().any(|l| l.contains(s))
}

#[test]
fn frozen_games_show_in_every_look_and_not_when_switched_off_or_when_there_are_none() {
    let tmp = std::env::temp_dir().join(format!("pocketui-frozen-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    let ok = Command::new("python3")
        .args(["-I", &format!("{ROOT}/scripts/curator-fake-library.py"), tmp.join("lib").to_str().unwrap(), "dark"])
        .stdout(std::process::Stdio::null())
        .status()
        .expect("python3");
    assert!(ok.success());
    let c = Ctx { stub: wayland(&tmp), tmp: tmp.clone(), n: Default::default() };
    // The row of the first recent game, after Curator and Search on top of Home in every look but the Cabinet.
    for look in ["shelf", "minimal", "split", "cards", "cabinet"] {
        let state = if look == "cabinet" { "home:0" } else { "home:2" };
        for (lang, resume, word) in [("en", "Resume", "Frozen"), ("de", "Fortsetzen", "Eingefroren")] {
            let (v, faults) = c.shot(look, state, 5, lang, "{}");
            assert!(has(&v, resume), "{look} {lang}: the A hint says it goes on: {v:?}");
            // The first recent game is "Continue" on the Shelf, where its open case says it.
            if look != "shelf" {
                assert!(has(&v, word), "{look} {lang}: the word says it's frozen: {v:?}");
            }
            assert!(faults.is_empty(), "{look} {lang}: {faults:?}");

            let (off, _) = c.shot(look, state, 5, lang, r#"{"frozen_games": false}"#);
            assert!(!has(&off, word), "{look} {lang}: switched off, no word: {off:?}");
            let (none, _) = c.shot(look, state, 0, lang, "{}");
            assert!(!has(&none, word), "{look} {lang}: nothing frozen, no word: {none:?}");
            assert!(has(&none, if lang == "en" { "Play" } else { "Spielen" }), "{look} {lang}: a normal case plays: {none:?}");
        }
    }
    // A later game of the row, which isn't "Continue": the Shelf says the word over its name.
    let (v, faults) = c.shot("shelf", "home:3", 5, "en", "{}");
    assert!(has(&v, "Frozen"), "{v:?}");
    assert!(faults.is_empty(), "{faults:?}");
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn the_switch_says_what_remember_where_i_quit_does_to_it() {
    let tmp = std::env::temp_dir().join(format!("pocketui-frozen-sw-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    let stub = wayland(&tmp);
    let row = 3; // System's rows: Language, Time, Sleep & power, Frozen games.
    for (forget, line) in [(false, "stand a little open"), (true, "freezes nothing new")] {
        let data = tmp.join(format!("data-{forget}"));
        std::fs::create_dir_all(&data).unwrap();
        std::fs::write(data.join("settings.json"), format!(r#"{{"onboarded": true, "forget_where_quit": {forget}}}"#)).unwrap();
        let visible = tmp.join(format!("{forget}.txt"));
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
        cmd.args(["--snapshot", &format!("settings:5:{row}"), tmp.join(format!("{forget}.png")).to_str().unwrap()])
            .env("POCKETUI_MOCK", "1")
            .env("POCKETUI_LANG", "en")
            .env("POCKETUI_VISIBLE_TEXT", &visible)
            .env("POCKETUI_DATA", &data)
            .env("POCKETUI_ES_SYSTEMS", tmp.join("none.cfg"));
        std::fs::write(tmp.join("none.cfg"), "<systemList></systemList>").unwrap();
        if let Some(s) = &stub {
            cmd.env("WAYLAND_DISPLAY", &s.1);
        }
        let out = cmd.output().expect("run pocket-ui");
        assert!(out.status.success());
        let v = std::fs::read_to_string(&visible).unwrap();
        assert!(v.contains("Frozen games"), "{v}");
        assert!(v.contains(line), "forget={forget}: {v}");
    }
    let _ = std::fs::remove_dir_all(&tmp);
}
