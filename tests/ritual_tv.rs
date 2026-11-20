//! The TV's Cozy extras (Settings → Look → Cozy extras, docs/user/settings.md "Cozy extras"), as a player meets them on
//! the made-up library (scripts/curator-fake-library.py):
//! - Old-TV switch-off and Evening TV glow are reached with the D-pad from the page's first row in
//!   every language, shown whole at 640x480 in Large text and on the big screen, and a press on them is saved;
//! - Evening TV glow: after dark the cabinet is warmer round the TV; switched off it is the picture without it, exactly.
//!
//! The pictures of the switch-on and the switch-off are unit tests (`crt_on`, `eject`).
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
    /// Last, so the stub is gone before the folder is.
    _dir: tempfile::TempDir,
}

struct Shot {
    visible: Vec<String>,
    faults: Vec<String>,
    data: PathBuf,
    png: PathBuf,
}

impl Ctx {
    /// `state` in the look `look` with `settings` (the part of settings.json after the look), the room's clock at `time`.
    fn shot(&self, look: &str, state: &str, lang: &str, size: &str, settings: &str, time: &str) -> Shot {
        self.n.set(self.n.get() + 1);
        let n = self.n.get();
        let (data, tmpdir) = (self.tmp.join(format!("data-{n}")), self.tmp.join(format!("tmp-{n}")));
        std::fs::create_dir_all(&data).unwrap();
        std::fs::create_dir_all(&tmpdir).unwrap();
        std::fs::write(data.join("settings.json"), format!(r#"{{"onboarded": true, "theme": "{look}"{settings}}}"#)).unwrap();
        let (visible, boxes, png) = (self.tmp.join(format!("{n}.txt")), self.tmp.join(format!("{n}.tsv")), self.tmp.join(format!("{n}.png")));
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
        cmd.args(["--snapshot", state, png.to_str().unwrap()])
            .env("POCKETUI_MOCK", "1")
            .env("POCKETUI_MOCK_NOW", FIXTURE_NOW)
            .env("POCKETUI_LANG", lang)
            .env("POCKETUI_SNAPSHOT_SIZE", size)
            .env("POCKETUI_SNAPSHOT_FROZEN", "5")
            .env("POCKETUI_ROOM_TIME", time)
            .env("POCKETUI_VISIBLE_TEXT", &visible)
            .env("POCKETUI_TEXT_BOXES", &boxes)
            .env("POCKETUI_DATA", &data)
            .env("POCKETUI_ES_SYSTEMS", self.tmp.join("lib/es_systems.cfg"))
            .env("TMPDIR", &tmpdir);
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
            png,
        }
    }
}

fn context(name: &str) -> Ctx {
    let dir = tempfile::Builder::new().prefix(&format!("pocketui-ritual-tv-{name}-")).tempdir().unwrap();
    let tmp = dir.path().to_path_buf();
    let ok = Command::new("python3")
        .env("POCKETUI_MOCK_NOW", FIXTURE_NOW)
        .args(["-I", &format!("{ROOT}/scripts/curator-fake-library.py"), tmp.join("lib").to_str().unwrap(), "dark"])
        .stdout(std::process::Stdio::null())
        .status()
        .expect("python3");
    assert!(ok.success());
    Ctx { stub: wayland(&tmp), tmp, n: Default::default(), _dir: dir }
}

/// `english` in `lang` (assets/i18n), as the page shows it.
fn tr(lang: &str, english: &str) -> String {
    if lang == "en" {
        return english.to_string();
    }
    let tsv = std::fs::read_to_string(format!("{ROOT}/assets/i18n/{lang}.tsv")).unwrap();
    tsv.lines()
        .find_map(|l| l.strip_prefix(english).and_then(|r| r.strip_prefix('\t')))
        .unwrap_or_else(|| panic!("{lang}: no line for {english:?}"))
        .to_string()
}

fn pixels(png: &Path) -> (u32, u32, Vec<u8>) {
    let mut reader = png::Decoder::new(std::fs::File::open(png).unwrap()).read_info().unwrap();
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).unwrap();
    assert_eq!(info.color_type, png::ColorType::Rgb);
    (info.width, info.height, buf)
}

/// The page's new rows: where they are (rows from the first) and their keys in settings.json.
const ROWS: [(&str, usize, &str); 2] = [("Old-TV switch-off", 7, "tv_switch_off"), ("Evening TV glow", 11, "evening_tv_glow")];

/// Down from the first row to each new one in all six languages: shown whole (nothing cut, nothing run together) at
/// 640x480 in Large text and at 1240x1080, its line under the list in the language, and A switches it off and is saved.
#[test]
fn the_tv_rows_are_reached_with_the_dpad_in_every_language_and_saved() {
    let c = context("nav");
    let mut failed = Vec::new();
    for lang in ["en", "de", "es", "pt", "fr", "it"] {
        for (label, row, key) in ROWS {
            let want = tr(lang, label);
            let down = "d".repeat(row);
            for (size, text) in [("640x480", r#", "text_size": "large""#), ("1240x1080", "")] {
                let s = c.shot("cabinet", &format!("cozy:0::{down}"), lang, size, text, "12:00");
                if !s.visible.iter().any(|v| *v == want) {
                    failed.push(format!("{lang} {size}{text}: {label} ({want:?}) not shown whole after {row} × Down: {:?}", s.visible));
                }
                if s.visible.iter().any(|v| v.ends_with('…')) {
                    failed.push(format!("{lang} {size}{text} {label}: a line is cut: {:?}", s.visible));
                }
                failed.extend(s.faults.iter().map(|f| format!("{lang} {size}{text} {label}: {f}")));
            }
            let s = c.shot("cabinet", &format!("cozy:0::{down}a"), lang, "640x480", "", "12:00");
            let saved: serde_json::Value = serde_json::from_slice(&std::fs::read(s.data.join("settings.json")).unwrap()).unwrap();
            if saved["extras"] != serde_json::json!({ key: false }) {
                failed.push(format!("{lang} {label}: A should switch only {key} off: {}", saved["extras"]));
            }
        }
    }
    assert!(failed.is_empty(), "{}", failed.join("\n"));
}

/// After dark the TV's warm light falls round it; switched off, the picture is the one without it, pixel for pixel
/// (the room's own light by time of day and the window are off in all three, so only the glow can differ).
#[test]
fn the_evening_glow_warms_the_cabinet_after_dark_and_off_is_the_old_picture() {
    let c = context("glow");
    // The room's own light by time of day and the window's sky are off in all three: only the glow may differ.
    let no_time = r#", "extras": {"time_of_day_light": false, "window": false}"#;
    let glow_off = r#", "extras": {"time_of_day_light": false, "window": false, "evening_tv_glow": false}"#;
    let state = "shelf:snes:cabinet:games:0";
    let day = pixels(&c.shot("cabinet", state, "en", "1240x1080", no_time, "12:00").png);
    let night = pixels(&c.shot("cabinet", state, "en", "1240x1080", no_time, "23:30").png);
    let off = pixels(&c.shot("cabinet", state, "en", "1240x1080", glow_off, "23:30").png);
    assert_eq!(off, day, "switched off, the night's cabinet is the old picture");
    // Warmer (more red than blue gained) on the board and shelves round the TV, at the right, under it.
    let (w, h, _) = day;
    let warm = |p: &(u32, u32, Vec<u8>)| {
        let (mut r, mut b) = (0i64, 0i64);
        for y in h * 2 / 5..h * 3 / 5 {
            for x in w / 2..w {
                let i = ((y * w + x) * 3) as usize;
                r += p.2[i] as i64;
                b += p.2[i + 2] as i64;
            }
        }
        r - b
    };
    assert!(warm(&night) > warm(&day), "warmer round the TV after dark: {} against {}", warm(&night), warm(&day));
    let changed = night.2.iter().zip(&day.2).filter(|(a, b)| a != b).count();
    assert!(changed > 1000, "the glow is seen: {changed} bytes");
}

/// The room dims in the evening the same with a handheld game's handheld in the TV's place as with the TV: the room's
/// light is part of the cached wood and tiles, and making it depend on what stands there rebuilt them with every move
/// between a handheld and a console game (RG35XX Plus, 19.11.2026: a move in the evening 77 ms instead of 14). The moves
/// of `bench-shelf:cabinet` (handheld and console games next to each other) at night rebuild the room at most once.
#[test]
fn moving_between_handheld_and_console_games_in_the_evening_keeps_the_room() {
    let c = context("glow-moves");
    let data = c.tmp.join("data-moves");
    std::fs::create_dir_all(&data).unwrap();
    std::fs::write(data.join("settings.json"), r#"{"onboarded": true, "theme": "cabinet", "extras": {"window": false}}"#).unwrap();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
    cmd.args(["--snapshot", "bench-shelf:cabinet", c.tmp.join("moves.png").to_str().unwrap()])
        .env("POCKETUI_MOCK", "1").env("POCKETUI_MOCK_NOW", FIXTURE_NOW).env("POCKETUI_ROOM_TIME", "23:30")
        .env("POCKETUI_SNAPSHOT_SIZE", "640x480").env("POCKETUI_DATA", &data)
        .env("POCKETUI_ES_SYSTEMS", c.tmp.join("lib/es_systems.cfg"));
    if let Some(s) = &c.stub {
        cmd.env("WAYLAND_DISPLAY", &s.1);
    }
    let out = cmd.output().expect("run pocket-ui");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let line = stdout.lines().find(|l| l.starts_with("bench-shelf ")).unwrap_or_else(|| panic!("no bench line: {stdout}"));
    let field = |k: &str| line.split_whitespace().find_map(|w| w.strip_prefix(k)).and_then(|v| v.parse::<usize>().ok()).unwrap();
    assert!(field("moves=") >= 10, "{line}");
    assert!(field("room_rebuilds=") <= 1, "the evening room is made once, not with every move: {line}");
}
