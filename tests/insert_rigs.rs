//! A game going into its console's real mechanism (`insert_rig`, docs/DESIGN.md "Starting a game"), as the player meets
//! it on the made-up library (scripts/curator-fake-library.py): Cozy shows the rig, a game without a cover still has a
//! medium to go in, a system without a rig keeps the plain animation, and Motion Off and Quick never show a rig.
//!
//! Needs python3 (and, on Linux without a compositor, scripts/wayland-stub.py), as tests/disc_prints.rs.

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
    let path = tmp.join("wayland-stub");
    let child = Command::new("python3").args(["-I", &format!("{ROOT}/scripts/wayland-stub.py"), path.to_str().unwrap()]).spawn().expect("python3");
    let stub = Stub(child, path);
    test_clock::wait_for(|| stub.1.exists(), std::time::Duration::from_secs(2), "Wayland stub listening");
    Some(stub)
}

struct World {
    tmp: PathBuf,
    lib: PathBuf,
    stub: Option<Stub>,
    /// Last, so the stub is gone before the folder is.
    _dir: tempfile::TempDir,
}

impl World {
    fn new(name: &str) -> World {
        // Removed with the World, also when the test panics (docs/dev/tests.md "A test that needs a folder").
        let dir = tempfile::Builder::new().prefix(&format!("pui-rigs-{name}-")).tempdir().unwrap();
        let tmp = dir.path().to_path_buf();
        let lib = tmp.join("lib");
        let done = Command::new("python3").env("POCKETUI_MOCK_NOW", FIXTURE_NOW).args(["-I", &format!("{ROOT}/scripts/curator-fake-library.py"), lib.to_str().unwrap(), "dark"]).output().expect("python3");
        assert!(done.status.success(), "{}", String::from_utf8_lossy(&done.stderr));
        let stub = wayland(&tmp);
        World { tmp, lib, stub, _dir: dir }
    }

    /// The launch `ms` after A, on the first game of `system`: RGB rows.
    fn frame(&self, system: &str, settings: &str, ms: u32) -> Vec<u8> {
        self.frame_with(system, settings, ms, 0, 0)
    }

    /// [`frame`] of row `row`, with the `frozen` newest games standing open (`POCKETUI_SNAPSHOT_FROZEN`).
    fn frame_with(&self, system: &str, settings: &str, ms: u32, row: u32, frozen: u32) -> Vec<u8> {
        self.frame_sized(system, settings, ms, row, frozen, "1280x960")
    }

    /// [`frame_with`] on a screen of `size`.
    fn frame_sized(&self, system: &str, settings: &str, ms: u32, row: u32, frozen: u32, size: &str) -> Vec<u8> {
        let data = self.lib.join("data");
        std::fs::write(data.join("settings.json"), settings).unwrap();
        let png = self.tmp.join(format!("{system}-{ms}-{size}.png"));
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
        if let Some(s) = &self.stub {
            cmd.env("WAYLAND_DISPLAY", &s.1);
        }
        let out = cmd
            .args(["--snapshot", &format!("shelf:{system}:shelf:games:{row}"), png.to_str().unwrap()])
            .env("POCKETUI_MOCK", "1").env("POCKETUI_MOCK_NOW", FIXTURE_NOW)
            .env("POCKETUI_LANG", "en")
            .env("POCKETUI_SNAPSHOT_SIZE", size)
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
    for system in ["psx", "gamecube", "ps2", "snes", "gba", "n64", "nes", "gbc", "psp", "mastersystem", "gamegear", "pcengine", "3ds"] {
        let (open, down, shut) = (w.frame(system, COZY, 750), w.frame(system, COZY, 1240), w.frame(system, COZY, 1400));
        assert!(lit(&open) > 3000, "{system}: a console stands there");
        assert!(differs(&open, &down) > 2000, "{system}: the game moved");
        assert!(differs(&down, &shut) > 200, "{system}: something still moves after it is in");
    }
    // No rig: the plain animation is there all the same, never a gap.
    let plain = w.frame("wiiu", COZY, 750);
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
fn an_open_case_on_cozy_plays_its_rig_as_every_game_does_and_quick_stays_quick() {
    let w = World::new("frozen");
    let quick = r#"{"onboarded": true, "theme": "shelf", "animations": "quick"}"#;
    // The newest recent games stand open (two PS2 games on the made-up library). Each row of the PS2's list, all open and
    // none open: an open case plays Cozy's rig like the others (since 14.11.; until then it went the quick way).
    for row in 0..5 {
        let (cozy, open, q) = (w.frame_with("ps2", COZY, 750, row, 0), w.frame_with("ps2", COZY, 750, row, 99), w.frame_with("ps2", quick, 750, row, 0));
        assert!(differs(&cozy, &q) > 2000, "row {row}: Cozy's rig is not Quick's picture");
        assert!(differs(&open, &cozy) < differs(&open, &q), "row {row}: an open case on Cozy is Cozy's scene, not Quick's");
        assert!(lit(&open) > 3000, "row {row}");
        assert!(differs(&open, &w.frame_with("ps2", COZY, 1240, row, 99)) > 2000, "row {row}: the rig moves");
    }
}

#[test]
fn in_light_nothing_of_the_case_stays_where_it_stood() {
    let w = World::new("light-shadow");
    let light = r#"{"onboarded": true, "theme": "shelf", "appearance": "light", "animations": "cozy", "extras": {"playful_animations": true}}"#;
    let (px, width) = (w.frame("gba", light, 1000), 1280usize);
    let at = |x: usize, y: usize| { let i = (y * width + x) * 3; [px[i] as i32, px[i + 1] as i32, px[i + 2] as i32] };
    // The middle of where the case stood (it has faded by now) against empty page in the same row.
    let (place, page) = (at(150, 230), at(1180, 230));
    assert!(place.iter().zip(&page).all(|(a, b)| (a - b).abs() <= 6), "a block stays where the case stood: {place:?}, the page {page:?}");
}

/// What the display's light changes: the pixels of `on` that differ from `off` by more than a rounding, their bounding box
/// (x0, y0, x1, y1), and how many of them are brighter in `on`.
fn lit_glass(on: &[u8], off: &[u8], w: usize) -> (usize, (usize, usize, usize, usize), usize) {
    let (mut n, mut brighter) = (0, 0);
    let mut bb = (usize::MAX, usize::MAX, 0, 0);
    for (i, (a, b)) in on.chunks(3).zip(off.chunks(3)).enumerate() {
        if a.iter().zip(b).any(|(x, y)| x.abs_diff(*y) > 12) {
            n += 1;
            let (x, y) = (i % w, i / w);
            bb = (bb.0.min(x), bb.1.min(y), bb.2.max(x), bb.3.max(y));
            if a.iter().map(|v| u32::from(*v)).sum::<u32>() > b.iter().map(|v| u32::from(*v)).sum::<u32>() {
                brighter += 1;
            }
        }
    }
    (n, bb, brighter)
}

/// A handheld's start (Cozy, Motion Full, Old-TV switch-on on) lights its display once the game is in: 1450 ms after A
/// (the display has come up, the fade round it has barely begun) the glass is brighter than with the switch-on off, a
/// block of the console and nothing else, at the H700's 640x480 (painted natively), the Brick's 1024x768 and 1280x720.
/// Brick night check 14.11.2026: only the power light came on there (tests/brick_settings_walk.rs has the Brick's build).
#[test]
fn a_handhelds_display_lights_up_at_every_screen_size() {
    let w = World::new("power-on");
    let off = r#"{"onboarded": true, "theme": "shelf", "animations": "cozy", "extras": {"playful_animations": true, "tv_switch_on": false}}"#;
    for size in ["640x480", "1024x768", "1280x720"] {
        let (sw, sh) = size.split_once('x').map(|(a, b)| (a.parse::<usize>().unwrap(), b.parse::<usize>().unwrap())).unwrap();
        for system in ["gba", "gb", "nds"] {
            let (on, dark) = (w.frame_sized(system, COZY, 1450, 0, 0, size), w.frame_sized(system, off, 1450, 0, 0, size));
            let (n, (x0, y0, x1, y1), brighter) = lit_glass(&on, &dark, sw);
            assert!(n * 300 > sw * sh, "{system} {size}: its display lights up ({n} px changed)");
            assert!(brighter * 10 >= n * 9, "{system} {size}: lit means brighter ({brighter} of {n})");
            assert!((x1 - x0) * (y1 - y0) * 4 < sw * sh, "{system} {size}: only the glass, {x0}-{x1} x {y0}-{y1}");
        }
        // A console on a TV has no display of its own to light: the switch-on is the tube's, in pocket-core.
        let (on, dark) = (w.frame_sized("snes", COZY, 1450, 0, 0, size), w.frame_sized("snes", off, 1450, 0, 0, size));
        assert_eq!(lit_glass(&on, &dark, sw).0, 0, "snes {size}: nothing lights on the console");
    }
}

/// A label as ScreenScraper gives it (`support-texture`, a flat scan): pure magenta, `w` x `h`.
fn write_label(path: &Path, w: u32, h: u32) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, w, h);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        let data: Vec<u8> = (0..w * h).flat_map(|_| [0xf0, 0x10, 0xe0]).collect();
        enc.write_header().unwrap().write_image_data(&data).unwrap();
    }
    std::fs::write(path, out).unwrap();
}

/// Pixels of the launch picture that are the label's magenta (printed: a little muted, still clearly magenta).
fn magenta(px: &[u8]) -> usize {
    px.chunks_exact(3).filter(|p| p[0] > 170 && p[1] < 70 && p[2] > 150).count()
}

#[test]
fn a_real_label_goes_into_the_console_and_without_one_the_start_is_as_before() {
    // 26.11.: offline, with ScreenScraper off or before its run, everything looks exactly as it did; with the
    // game's label on the card, the cartridge carries it into the console.
    let w = World::new("labels");
    let data = w.lib.join("data");
    for (system, ms) in [("snes", 1000), ("megadrive", 1000), ("nes", 1000), ("n64", 1000), ("gba", 350), ("nds", 350)] {
        // The Mega Drive and the DS keep the cover's art on their label (owner 26.11.2026: their real ones look plainer).
        let real = !matches!(system, "megadrive" | "nds");
        let before = w.frame(system, COZY, ms);
        // Another game's label and a ScreenScraper folder: nothing of it on this one.
        let first = std::fs::read_dir(w.lib.join("roms").join(system)).unwrap().map(|e| e.unwrap().path()).min().unwrap();
        let other = data.join("supporttextures").join(system).join("Not On This Card (USA).png");
        write_label(&other, 300, 150);
        assert_eq!(w.frame(system, COZY, ms), before, "{system}: a ScreenScraper folder without this game's label changes nothing");
        assert_eq!(magenta(&before), 0, "{system}: no magenta before");
        let own = data.join("supporttextures").join(system).join(format!("{}.png", first.file_stem().unwrap().to_string_lossy()));
        write_label(&own, 300, 150);
        let with = w.frame(system, COZY, ms);
        if real {
            assert!(magenta(&with) > 300, "{system}: the game's own label is on the cartridge going in ({} px)", magenta(&with));
        } else {
            assert_eq!(with, before, "{system}: the cover's art on its label, its real label on the card or not");
        }
        std::fs::remove_file(&own).unwrap();
        assert_eq!(w.frame(system, COZY, ms), before, "{system}: its label gone, the start is as before");
        std::fs::remove_dir_all(data.join("supporttextures")).unwrap();
    }
}
