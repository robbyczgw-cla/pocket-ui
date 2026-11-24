//! Brick (1024x768) insertion animation startup and frame budget checks (0.6.1):
//! - Before this fix, every Cabinet launch repainted the entire cabinet on all 96 frames (even after the small
//!   console beside the TV finished fading at 120 ms), and Frame 0 decoded+scaled the console rig and cartridge/disc
//!   from scratch without reusing focused/cached assets.
//! Needs python3 on Linux without a compositor (scripts/wayland-stub.py), as tests/ritual_launch.rs.

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

fn setup_library(tmp: &Path, theme: &str) -> (PathBuf, PathBuf) {
    let lib = tmp.join("lib");
    let ok = Command::new("python3")
        .env("POCKETUI_MOCK_NOW", FIXTURE_NOW)
        .args(["-I", &format!("{ROOT}/scripts/curator-fake-library.py"), lib.to_str().unwrap(), "dark"])
        .stdout(std::process::Stdio::null())
        .status()
        .expect("python3 for scripts/curator-fake-library.py");
    assert!(ok.success());
    let data = lib.join("data");
    let settings = data.join("settings.json");
    let mut json: serde_json::Value = std::fs::read_to_string(&settings).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_else(|| serde_json::json!({}));
    json["theme"] = serde_json::json!(theme);
    json["onboarded"] = serde_json::json!(true);
    json["motion"] = serde_json::json!("full");
    json["animations"] = serde_json::json!("cozy");
    std::fs::write(&settings, json.to_string()).unwrap();
    (lib, data)
}

fn parse_u32(stdout: &str, field: &str) -> u32 {
    let line = stdout.lines().find(|l| l.starts_with("launch-series paint ")).unwrap_or_else(|| panic!("no launch-series line in:\n{stdout}"));
    let key = format!(" {field}=");
    let at = line.find(&key).unwrap_or_else(|| panic!("no {field} in: {line}")) + key.len();
    line[at..].split_whitespace().next().unwrap().parse().unwrap_or_else(|_| panic!("bad {field} in: {line}"))
}

#[test]
fn cabinet_launch_at_1024x768_does_not_repaint_the_cabinet_on_every_frame() {
    let dir = tempfile::Builder::new().prefix("pui-ins-").tempdir().unwrap();
    let tmp = dir.path();
    let stub = wayland(tmp);
    let (lib, data) = setup_library(tmp, "cabinet");

    for system in ["snes", "psx", "gba"] {
        let series = tmp.join(format!("series-{system}"));
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
        if let Some(s) = &stub {
            cmd.env("WAYLAND_DISPLAY", &s.1);
        }
        let out = cmd
            .args(["--snapshot", &format!("shelf:{system}:cabinet:games:0"), tmp.join("x.png").to_str().unwrap()])
            .env("POCKETUI_MOCK", "1")
            .env("POCKETUI_MOCK_NOW", FIXTURE_NOW)
            .env("POCKETUI_DEVICE", "brick")
            .env("POCKETUI_LANG", "en")
            .env("POCKETUI_ROOM_TIME", "12:00")
            .env("POCKETUI_SNAPSHOT_SIZE", "1024x768")
            .env("POCKETUI_ES_SYSTEMS", lib.join("es_systems.cfg"))
            .env("POCKETUI_DATA", &data)
            .env("POCKETUI_SEALED", "0")
            .env("POCKETUI_FRAME_LOG", "1")
            .env("POCKETUI_LAUNCH_SERIES_NO_PNG", "1")
            .env("POCKETUI_SNAPSHOT_LAUNCH_SERIES", &series)
            .output()
            .expect("run pocket-ui");
        assert!(out.status.success(), "{system}: {}", String::from_utf8_lossy(&out.stderr));
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(stdout.contains("launch-series paint frames="), "{system}: {stdout}");
        assert!(
            !stdout.contains("cabinet: 60 frames painted"),
            "{system}: Cabinet repainted 60+ full frames during a single launch animation instead of reusing the static background:\n{stdout}"
        );
        // Only the ~120 ms beside-TV fade (8-9 frames at 60 Hz) may repaint the Cabinet page; the rest reuses LaunchShelf.
        let pages = parse_u32(&stdout, "pages");
        assert!(pages <= 10, "{system}: expected <= 10 painted Cabinet pages during launch, got {pages}:\n{stdout}");
    }
}

#[test]
fn focused_game_assets_are_prewarmed_before_a_at_1024x768() {
    let dir = tempfile::Builder::new().prefix("pui-warm-").tempdir().unwrap();
    let tmp = dir.path();
    let stub = wayland(tmp);

    for theme in ["cabinet", "shelf"] {
        let sub = tmp.join(theme);
        std::fs::create_dir_all(&sub).unwrap();
        let (lib, data) = setup_library(&sub, theme);
        for system in ["snes", "psx", "gba"] {
            let series = sub.join(format!("series-{system}"));
            let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
            if let Some(s) = &stub {
                cmd.env("WAYLAND_DISPLAY", &s.1);
            }
            let out = cmd
                .args(["--snapshot", &format!("shelf:{system}:{theme}:games:0"), sub.join("x.png").to_str().unwrap()])
                .env("POCKETUI_MOCK", "1")
                .env("POCKETUI_MOCK_NOW", FIXTURE_NOW)
                .env("POCKETUI_DEVICE", "brick")
                .env("POCKETUI_LANG", "en")
                .env("POCKETUI_ROOM_TIME", "12:00")
                .env("POCKETUI_SNAPSHOT_SIZE", "1024x768")
                .env("POCKETUI_ES_SYSTEMS", lib.join("es_systems.cfg"))
                .env("POCKETUI_DATA", &data)
                .env("POCKETUI_SEALED", "0")
                .env("POCKETUI_SNAPSHOT_WARM_FOCUS", "1")
                .env("POCKETUI_LAUNCH_SERIES_NO_PNG", "1")
                .env("POCKETUI_SNAPSHOT_LAUNCH_SERIES", &series)
                .output()
                .expect("run pocket-ui");
            assert!(out.status.success(), "{theme}/{system}: {}", String::from_utf8_lossy(&out.stderr));
            let stdout = String::from_utf8_lossy(&out.stdout);
            let rig_loads = parse_u32(&stdout, "rig_loads");
            let game_builds = parse_u32(&stdout, "game_builds");
            let pages = parse_u32(&stdout, "pages");
            assert_eq!(rig_loads, 0, "{theme}/{system}: rig was still decoded on Frame 0 after focus warm-up:\n{stdout}");
            assert_eq!(game_builds, 0, "{theme}/{system}: cartridge/disc was still built on Frame 0 after focus warm-up:\n{stdout}");
            let max_pages = if theme == "shelf" { 1 } else { 10 };
            assert!(pages <= max_pages, "{theme}/{system}: expected <= {max_pages} pages, got {pages}:\n{stdout}");
        }
    }
}
