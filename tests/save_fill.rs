//! Save in the pause menu made visible (`src/save_fill.rs`, `app_save_media::begin_save_fill`), as the player meets it: A on
//! Save keeps the menu up while the slot's block fills, the game goes on when it has run or at the first button, and
//! Quick, Motion below Full and a menu without slots go straight back as before. The clock stands (`frame_clock`), so
//! the moments are facts of the order, not of how busy the computer is. Needs python3 on Linux without a compositor
//! (scripts/wayland-stub.py), as tests/eject.rs.

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

/// `pocket-ui --snapshot <state>` on an empty data folder with `settings`; what it printed.
fn run(tmp: &Path, stub: &Option<Stub>, state: &str, settings: serde_json::Value) -> String {
    let data = tmp.join(format!("data-{}", settings.to_string().replace(['"', '{', '}', ':', ','], "")));
    std::fs::create_dir_all(&data).unwrap();
    std::fs::write(data.join("settings.json"), settings.to_string()).unwrap();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
    if let Some(s) = stub {
        cmd.env("WAYLAND_DISPLAY", &s.1);
    }
    let out = cmd
        .args(["--snapshot", state, tmp.join("shot.png").to_str().unwrap()])
        .env("POCKETUI_MOCK", "1")
        .env("POCKETUI_MOCK_NOW", FIXTURE_NOW)
        .env("POCKETUI_LANG", "en")
        .env("POCKETUI_DATA", &data)
        .env("POCKETUI_ES_SYSTEMS", tmp.join("none.cfg"))
        .output()
        .expect("run pocket-ui");
    assert!(out.status.success(), "{state}: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn lines(out: &str) -> Vec<&str> {
    out.lines().filter(|l| l.starts_with("save-run ")).collect()
}

#[test]
fn the_menu_stays_up_while_the_block_fills_and_the_game_goes_on_when_it_has_run() {
    let tmp = tempfile::tempdir().unwrap();
    let stub = wayland(tmp.path());
    let out = run(tmp.path(), &stub, "slot-save:psx:fill-run=run", serde_json::json!({"onboarded": true}));
    assert_eq!(
        lines(&out),
        [
            "save-run before saving=false menu=true",
            "save-run pressed saving=true menu=true",
            "save-run at-300ms saving=true menu=true",
            "save-run end saving=false menu=false",
            "save-run skip-without-a-fill=false",
        ],
        "A shows the slot filling, still in the menu at 300 ms, and the game is back after the animation's length: {out}"
    );
}

#[test]
fn any_button_ends_it_at_once_and_the_game_comes_back() {
    let tmp = tempfile::tempdir().unwrap();
    let stub = wayland(tmp.path());
    let out = run(tmp.path(), &stub, "slot-save:psx:fill-run=skip", serde_json::json!({"onboarded": true}));
    assert_eq!(
        lines(&out),
        [
            "save-run before saving=false menu=true",
            "save-run pressed saving=true menu=true",
            "save-run at-300ms saving=true menu=true",
            "save-run skip-used-the-press=true",
            "save-run end saving=false menu=false",
            "save-run skip-without-a-fill=false",
        ],
        "a button at 300 ms ends the picture, is used up by it, and the game is back at once: {out}"
    );
}

#[test]
fn quick_and_motion_below_full_save_and_go_back_at_once_as_before() {
    let tmp = tempfile::tempdir().unwrap();
    let stub = wayland(tmp.path());
    for settings in [serde_json::json!({"onboarded": true, "animations": "quick"}), serde_json::json!({"onboarded": true, "motion": "reduced"}), serde_json::json!({"onboarded": true, "motion": "off"})] {
        let out = run(tmp.path(), &stub, "slot-save:psx:fill-run=run", settings.clone());
        let l = lines(&out);
        assert_eq!(l[1], "save-run pressed saving=false menu=false", "{settings}: the press saves and the game is back, nothing plays: {out}");
    }
}
