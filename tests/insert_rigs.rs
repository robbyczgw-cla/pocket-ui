//! A game going into its console's real mechanism (`insert_rig`, docs/DESIGN.md "Starting a game"), as the player meets
//! it on the made-up library (scripts/curator-fake-library.py): Cozy shows the rig, a game without a cover still has a
//! medium to go in, a system without a rig keeps the plain animation, and Motion Off and Quick never show a rig.
//!
//! Needs python3 (and, on Linux without a compositor, scripts/wayland-stub.py), as tests/disc_prints.rs.

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
    let path = tmp.join("wayland-stub");
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

struct World {
    tmp: PathBuf,
    lib: PathBuf,
    stub: Option<Stub>,
}

impl World {
    fn new(name: &str) -> World {
        let tmp = std::env::temp_dir().join(format!("pui-rigs-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let lib = tmp.join("lib");
        let done = Command::new("python3").args(["-I", &format!("{ROOT}/scripts/curator-fake-library.py"), lib.to_str().unwrap(), "dark"]).output().expect("python3");
        assert!(done.status.success(), "{}", String::from_utf8_lossy(&done.stderr));
        let stub = wayland(&tmp);
        World { tmp, lib, stub }
    }

    /// The launch `ms` after A, on the first game of `system`: RGB rows.
    fn frame(&self, system: &str, settings: &str, ms: u32) -> Vec<u8> {
        self.frame_with(system, settings, ms, 0, 0)
    }

    /// [`frame`] of row `row`, with the `frozen` newest games standing open (`POCKETUI_SNAPSHOT_FROZEN`).
    fn frame_with(&self, system: &str, settings: &str, ms: u32, row: u32, frozen: u32) -> Vec<u8> {
        let data = self.lib.join("data");
        std::fs::write(data.join("settings.json"), settings).unwrap();
        let png = self.tmp.join(format!("{system}-{ms}.png"));
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
        if let Some(s) = &self.stub {
            cmd.env("WAYLAND_DISPLAY", &s.1);
        }
        let out = cmd
            .args(["--snapshot", &format!("shelf:{system}:shelf:games:{row}"), png.to_str().unwrap()])
            .env("POCKETUI_MOCK", "1")
            .env("POCKETUI_LANG", "en")
            .env("POCKETUI_SNAPSHOT_SIZE", "1280x960")
            .env("POCKETUI_SNAPSHOT_LAUNCH_MS", ms.to_string())
            .env("POCKETUI_SNAPSHOT_FROZEN", frozen.to_string())
            .env("POCKETUI_ES_SYSTEMS", self.lib.join("es_systems.cfg"))
            .env("POCKETUI_DATA", &data)
            .output()
            .expect("run pocket-ui");
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        let mut reader = png::Decoder::new(std::fs::File::open(&png).unwrap()).read_info().unwrap();
        let mut buf = vec![0; reader.output_buffer_size()];
        reader.next_frame(&mut buf).unwrap();
        buf
    }
}

const COZY: &str = r#"{"onboarded": true, "theme": "shelf", "animations": "cozy", "extras": {"playful_animations": true}}"#;

fn differs(a: &[u8], b: &[u8]) -> usize {
    a.iter().zip(b).filter(|(x, y)| x != y).count()
}

/// Lit pixels' bytes in the middle of the screen, where the console stands.
fn lit(px: &[u8]) -> usize {
    let (w, h) = (1280usize, 960usize);
    let (x0, x1, y0, y1) = (w / 4, w * 3 / 4, h / 2, h - 120);
    (y0..y1).map(|y| px[(y * w + x0) * 3..(y * w + x1) * 3].iter().filter(|b| **b > 60).count()).sum()
}

#[test]
fn cozy_shows_the_mechanism_at_every_step_and_the_plain_animation_elsewhere() {
    let w = World::new("cozy");
    for system in ["psx", "gamecube", "ps2", "snes", "gba", "n64"] {
        let (open, down, shut) = (w.frame(system, COZY, 750), w.frame(system, COZY, 1240), w.frame(system, COZY, 1400));
        assert!(lit(&open) > 3000, "{system}: a console stands there");
        assert!(differs(&open, &down) > 2000, "{system}: the game moved");
        assert!(differs(&down, &shut) > 200, "{system}: something still moves after it is in");
    }
    // No rig: the plain animation is there all the same, never a gap.
    let plain = w.frame("3ds", COZY, 750);
    assert!(lit(&plain) > 3000, "a system without a rig shows its console");
}

#[test]
fn quick_and_motion_off_never_show_a_rig() {
    let w = World::new("quick");
    let quick = r#"{"onboarded": true, "theme": "shelf", "animations": "quick"}"#;
    let off = r#"{"onboarded": true, "theme": "shelf", "animations": "cozy", "motion": "off"}"#;
    // The rig's console stands alone and large; the plain picture is the set with its controller.
    let rig = w.frame("psx", COZY, 750);
    let (q, a) = (w.frame("psx", quick, 180), w.frame("psx", off, 750));
    assert!(differs(&rig, &q) > 2000, "Quick isn't the rig");
    assert!(differs(&rig, &a) > 2000, "Motion Off isn't the rig");
    // Reduced and Off start straight away: long past Quick's end, their picture has faded as Quick's has.
    let reduced = r#"{"onboarded": true, "theme": "shelf", "animations": "cozy", "motion": "reduced"}"#;
    for (name, settings) in [("off", off), ("reduced", reduced)] {
        assert!(lit(&w.frame("psx", settings, 750)) < 200, "Motion {name}: Cozy's long fade isn't waited for");
    }
}

#[test]
fn a_frozen_game_goes_straight_back_and_the_others_keep_their_rig() {
    let w = World::new("frozen");
    let quick = r#"{"onboarded": true, "theme": "shelf", "animations": "quick"}"#;
    // The newest recent games stand open (two PS2 games on the made-up library). Each row of the PS2's list:
    let mut frozen_rows = 0;
    for row in 0..5 {
        let (cozy, open, q) = (w.frame_with("ps2", COZY, 750, row, 0), w.frame_with("ps2", COZY, 750, row, 99), w.frame_with("ps2", quick, 750, row, 0));
        let (to_cozy, to_quick) = (differs(&open, &cozy), differs(&open, &q));
        if to_quick < to_cozy {
            // A frozen game: no rig, the quick way, whatever Animations says.
            frozen_rows += 1;
            assert!(differs(&cozy, &q) > 2000, "row {row}: the rig is there when it isn't frozen");
        } else {
            // A game beside the half-open cases: its rig as ever, moving.
            assert!(lit(&open) > 3000, "row {row}");
            assert!(differs(&open, &w.frame_with("ps2", COZY, 1240, row, 99)) > 2000, "row {row}: the rig moves");
        }
    }
    assert!(frozen_rows >= 1, "the made-up library has frozen games in the PS2's list");
}
