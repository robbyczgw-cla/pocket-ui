//! Saved games and translated shelf art, through the real snapshot painter (no device).
use std::path::{Path, PathBuf};
use std::process::Command;
include!("common/root.rs");

struct Fixture {
    stub: Option<std::process::Child>,
    dir: tempfile::TempDir,
    n: std::cell::Cell<usize>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if let Some(child) = &mut self.stub { let _ = child.kill(); let _ = child.wait(); }
    }
}
impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("roms")).unwrap();
        std::fs::create_dir_all(root.join("states/snes")).unwrap();
        std::fs::write(root.join("roms/Super Mario World.sfc"), b"fixture game").unwrap();
        std::fs::write(root.join("es_systems.cfg"), format!("<systemList><system><name>snes</name><fullname>Super Nintendo</fullname><path>{}/roms</path><extension>.sfc</extension><command>true</command></system></systemList>", root.display())).unwrap();
        let stub = if cfg!(target_os = "linux") && std::env::var_os("WAYLAND_DISPLAY").is_none() {
            let child = Command::new("python3").args(["-I", &format!("{ROOT}/scripts/wayland-stub.py")]).arg(root.join("wl")).spawn().unwrap();
            test_clock::wait_for(|| root.join("wl").exists(), std::time::Duration::from_secs(2), "Wayland stub");
            Some(child)
        } else { None };
        Self { stub, dir, n: Default::default() }
    }
    fn root(&self) -> &Path { self.dir.path() }
    fn shot(&self, label: &str, state: &str, look: &str, light: &str, lang: &str, sealed: bool, progress: bool) -> (Vec<u8>, String, String) {
        let n = self.n.get(); self.n.set(n + 1);
        let data = self.root().join(format!("data-{n}"));
        std::fs::create_dir_all(&data).unwrap();
        let motion = if label.starts_with("launch-") { "full" } else { "off" };
        std::fs::write(data.join("settings.json"), format!(r#"{{"onboarded":true,"seen_version":"9.9.9","theme":"{look}","appearance":"{light}","language":"{lang}","motion":"{motion}","animations":"cozy","art":"off"}}"#)).unwrap();
        let rom = self.root().join("roms/Super Mario World.sfc");
        std::fs::write(data.join("favorites.json"), serde_json::to_vec(&vec![pocket_ui::favorites::Favorite { system: "snes".into(), name: "Super Mario World".into(), path: rom }]).unwrap()).unwrap();
        let png = self.root().join(format!("{n}.png"));
        let visible = self.root().join(format!("{n}.txt"));
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
        cmd.args(["--snapshot", state]).arg(&png)
            .env("POCKETUI_MOCK", "1").env("POCKETUI_MOCK_NOW", FIXTURE_NOW)
            .env("POCKETUI_DATA", &data).env("POCKETUI_ES_SYSTEMS", self.root().join("es_systems.cfg"))
            .env("POCKETUI_STATES", self.root().join("states"))
            .env("POCKETUI_SNAPSHOT_SIZE", "1024x768").env("POCKETUI_VISIBLE_TEXT", &visible)
            .env("POCKETUI_SEALED", if sealed { "1" } else { "0" })
            .env_remove("POCKETUI_LANG").env_remove("POCKETUI_SNAPSHOT_PROGRESS");
        if self.stub.is_some() { cmd.env("WAYLAND_DISPLAY", self.root().join("wl")); }
        if label.starts_with("launch-") { cmd.env("POCKETUI_SNAPSHOT_LAUNCH_MS", "120"); }
        if progress { cmd.env("POCKETUI_SNAPSHOT_PROGRESS", "mario"); }
        let out = cmd.output().unwrap();
        assert!(out.status.success(), "{state}: {}\n{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
        assert!(png.exists(), "{state}: snapshot must exist");
        if let Some(dir) = std::env::var_os("POCKETUI_TEST_EVIDENCE") {
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::copy(&png, PathBuf::from(dir).join(format!("{label}.png"))).unwrap();
        }
        (std::fs::read(png).unwrap(), std::fs::read_to_string(visible).unwrap_or_default(), String::from_utf8_lossy(&out.stdout).into_owned())
    }
}

#[test]
fn a_save_or_state_without_recents_never_has_film_or_a_not_played_label() {
    let f = Fixture::new();
    for (kind, path) in [("srm", "roms/Super Mario World.srm"), ("state", "states/snes/Super Mario World.state3")] {
        std::fs::write(f.root().join(path), b"save copied from another frontend").unwrap();
        for look in ["shelf", "cabinet"] {
            let state = format!("shelf:snes:{look}:games:0");
            let on = f.shot(&format!("saved-{kind}-{look}"), &state, look, "dark", "en", true, false);
            let off = f.shot("comparison", &state, look, "dark", "en", false, false);
            assert!(on.0 == off.0, "{kind}/{look}: a saved game must have the bare case");
            assert!(!on.1.contains("Not played yet"), "{}", on.1);
        }
        let back = f.shot(&format!("saved-{kind}-back"), "box:snes:mario:rdddddddd", "shelf", "dark", "en", true, false);
        assert!(!back.1.contains("Not played yet"), "{}", back.1);
        assert!(back.1.contains("Not here yet"), "no invented play time: {}", back.1);
        std::fs::remove_file(f.root().join(path)).unwrap();
    }
    let fresh = f.shot("fresh-sealed", "shelf:snes:shelf:games:0", "shelf", "dark", "en", true, false);
    let bare = f.shot("fresh-bare", "shelf:snes:shelf:games:0", "shelf", "dark", "en", false, false);
    assert!(fresh.0 != bare.0, "a truly fresh game still wears its film");
    assert!(fresh.1.contains("Not played yet"), "{}", fresh.1);
}

#[test]
fn progress_without_recents_never_sits_beside_not_played_or_film() {
    let f = Fixture::new();
    for light in ["dark", "light"] {
        let back = f.shot(&format!("progress-back-{light}"), "box:snes:mario:r", "shelf", light, "en", true, true);
        assert!(back.1.contains("43 of 96"), "{}", back.1);
        assert!(!back.1.contains("Not played yet"), "{}", back.1);
        let bare = f.shot("comparison", "box:snes:mario:r", "shelf", light, "en", false, true);
        assert!(back.0 == bare.0, "progress alone keeps the whole case unsealed");
    }
}

#[test]
fn shelf_spines_and_cabinet_faces_after_a_live_language_change_match_a_cold_start() {
    let f = Fixture::new();
    for look in ["shelf", "cabinet"] {
        for light in ["dark", "light"] {
            let state = format!("shelf::{look}:menu");
            let english = f.shot("english", &state, look, light, "en", false, false);
            for lang in ["de", "fr"] {
                let cold = f.shot("comparison", &state, look, light, lang, false, false);
                let switched = f.shot(&format!("language-{look}-{light}-{lang}"), &format!("{state}:language={lang}"), look, light, "en", false, false);
                assert!(english.0 != cold.0, "the fixture must exercise translated words");
                assert!(switched.0 == cold.0, "{look}/{light}/{lang}: all cached spines and fronts must change at once");
            }
        }
    }
}

#[test]
fn only_a_truly_fresh_game_tears_its_film_on_start() {
    let f = Fixture::new();
    let state = "shelf:snes:shelf:games:0";
    let fresh = f.shot("launch-fresh", state, "shelf", "dark", "en", true, false);
    assert!(fresh.2.contains("launch-wrap tearing=true"), "{}", fresh.2);
    for (kind, file) in [("srm", "roms/Super Mario World.srm"), ("state", "states/snes/Super Mario World.state3")] {
        std::fs::write(f.root().join(file), b"save").unwrap();
        let saved = f.shot(&format!("launch-{kind}"), state, "shelf", "dark", "en", true, false);
        assert!(saved.2.contains("launch-wrap tearing=false"), "{kind}: {}", saved.2);
        std::fs::remove_file(f.root().join(file)).unwrap();
    }
}
