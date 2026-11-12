//! Cozy's way back from a game (`src/eject.rs`, `app_cabinet::begin_eject`) as the player meets it, on the made-up
//! library (scripts/curator-fake-library.py) with the cabinet on the game just played: the last picture switches off
//! (a SNES game's to a bright line, a GBA game's display goes dark), the shelf is live at once, and a button at any
//! moment ends the way back and moves the shelf in the same press. Quick, and Motion below Full, show none of it.
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

/// The made-up library with `rom` of `system` as the game just played (first on Continue, its moment waiting).
fn library(tmp: &Path, system: &str, rom: &str, settings: serde_json::Value) -> (PathBuf, PathBuf) {
    let lib = tmp.join(format!("lib-{system}"));
    let ok = Command::new("python3")
        .env("POCKETUI_MOCK_NOW", FIXTURE_NOW)
        .args(["-I", &format!("{ROOT}/scripts/curator-fake-library.py"), lib.to_str().unwrap(), "dark"])
        .stdout(std::process::Stdio::null())
        .status()
        .expect("python3 for scripts/curator-fake-library.py");
    assert!(ok.success());
    let data = lib.join("data");
    let path = lib.join("roms").join(system).join(rom);
    if !path.exists() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"fixture ROM").unwrap();
        let cfg = lib.join("es_systems.cfg");
        let xml = std::fs::read_to_string(&cfg).unwrap();
        if !xml.contains(&format!("<name>{system}</name>")) {
            let row = format!("<system><name>{system}</name><fullname>{system}</fullname><path>{}</path><extension>.chd</extension><command>true</command></system>", path.parent().unwrap().display());
            std::fs::write(cfg, xml.replace("</systemList>", &format!("{row}</systemList>"))).unwrap();
        }
    }
    let recents = data.join("recents.json");
    let mut list: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&recents).unwrap()).unwrap();
    let name = rom.split(" (").next().unwrap();
    let now: u64 = FIXTURE_NOW.parse().unwrap();
    list.as_array_mut().unwrap().insert(0, serde_json::json!({"system": system, "name": name, "path": path, "last_played": now - 60, "play_secs": 3600}));
    std::fs::write(&recents, list.to_string()).unwrap();
    let mut json: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(data.join("settings.json")).unwrap()).unwrap();
    json["theme"] = serde_json::json!("cabinet");
    json["onboarded"] = serde_json::json!(true);
    for (k, v) in settings.as_object().unwrap() {
        json[k] = v.clone();
    }
    std::fs::write(data.join("settings.json"), json.to_string()).unwrap();
    // The one-time line about open cases was read long ago (`first_game::tell_open_case_once`).
    std::fs::write(data.join("open-case-told"), b"").unwrap();
    std::fs::write(data.join("kids.json"), serde_json::json!({"games": [path], "pin": "1234", "theme": "blue"}).to_string()).unwrap();
    // The game's last picture: a bright square in the middle of blue, the game's own size.
    let frame = tmp.join(format!("{system}-frame.png"));
    let (w, h) = if system == "gba" { (240, 160) } else { (256, 224) };
    let mut e = png::Encoder::new(std::fs::File::create(&frame).unwrap(), w, h);
    e.set_color(png::ColorType::Rgb);
    e.set_depth(png::BitDepth::Eight);
    let px: Vec<u8> = (0..w * h).flat_map(|i| if (i % w).abs_diff(w / 2) < 20 && (i / w).abs_diff(h / 2) < 20 { [250, 250, 250] } else { [40, 90, 200] }).collect();
    e.write_header().unwrap().write_image_data(&px).unwrap();
    (lib, frame)
}

/// The cabinet on the game just played, with `env`: its stdout, and the picture when one was written.
fn run(tmp: &Path, stub: &Option<Stub>, lib: &Path, frame: &Path, env: &[(&str, &str)]) -> (String, Option<(u32, u32, Vec<u8>)>) {
    let png = tmp.join(format!("shot-{}.png", env.iter().map(|(k, v)| format!("{k}{v}")).collect::<String>().replace(['/', ' '], "_")));
    let _ = std::fs::remove_file(&png);
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
    if let Some(s) = stub {
        cmd.env("WAYLAND_DISPLAY", &s.1);
    }
    let scene = env.iter().find(|(k, _)| *k == "EJECT_TEST_SCENE").map_or("cabinet:", |(_, v)| *v);
    cmd.args(["--snapshot", scene, png.to_str().unwrap()])
        .env("POCKETUI_MOCK", "1")
        .env("POCKETUI_MOCK_NOW", FIXTURE_NOW)
        .env("POCKETUI_LANG", "en")
        .env("POCKETUI_ROOM_TIME", "07:00")
        .env("POCKETUI_SNAPSHOT_SIZE", "1240x1080")
        .env("POCKETUI_SNAPSHOT_FROZEN", "1")
        .env("POCKETUI_ES_SYSTEMS", lib.join("es_systems.cfg"))
        .env("POCKETUI_DATA", lib.join("data"))
        .env("POCKETUI_SNAPSHOT_EJECT_FRAME", frame);
    for (k, v) in env {
        cmd.env(k, v);
    }
    let out = cmd.output().expect("run pocket-ui");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(out.status.success(), "{env:?}: {}\n{stdout}", String::from_utf8_lossy(&out.stderr));
    let pic = png.exists().then(|| {
        let mut reader = png::Decoder::new(std::fs::File::open(&png).unwrap()).read_info().unwrap();
        let mut buf = vec![0; reader.output_buffer_size()];
        let info = reader.next_frame(&mut buf).unwrap();
        (info.width, info.height, buf)
    });
    (stdout, pic)
}

fn value<'a>(line: &'a str, key: &str) -> &'a str {
    line.split_whitespace().find_map(|w| w.strip_prefix(key).and_then(|w| w.strip_prefix('='))).unwrap_or_else(|| panic!("{key} in {line}"))
}

/// Brightness of one row of the picture, 0-255 on average.
fn row_light((w, _, px): &(u32, u32, Vec<u8>), y: u32) -> u32 {
    let row = &px[(y * w * 3) as usize..((y + 1) * w * 3) as usize];
    row.iter().map(|b| *b as u32).sum::<u32>() / row.len() as u32
}

#[test]
fn quit_is_not_slower_the_shelf_takes_a_button_at_any_moment_of_the_way_back() {
    let tmp = tempfile::tempdir().unwrap();
    let stub = wayland(tmp.path());
    let (lib, frame) = library(tmp.path(), "snes", "Chrono Trigger (USA).sfc", serde_json::json!({}));
    // Best of three: a busy build computer doesn't fail it.
    let mut best = u64::MAX;
    let mut line = String::new();
    for _ in 0..3 {
        let (out, _) = run(tmp.path(), &stub, &lib, &frame, &[("POCKETUI_SNAPSHOT_EJECT_BENCH", "1")]);
        line = out.lines().find(|l| l.starts_with("bench-eject")).unwrap_or_else(|| panic!("no bench line:\n{out}")).to_string();
        assert!(!line.contains("started=false"), "Cozy, Motion Full, the cabinet on the game: the way back plays\n{line}");
        best = best.min(value(&line, "begin_us").parse().unwrap());
    }
    // CPU time of the UI thread (`eject_bench`), not the wall clock: a busy gate computer doesn't add to it.
    // Starting it is a few lines of bookkeeping on the shelf that is already there (the pause menu's picture is taken
    // over, not copied): far less than a frame, nothing added to Quit.
    assert!(best < 1_000, "the way back costs {best} µs to start\n{line}");
    assert_eq!(value(&line, "screen"), "Tv");
    assert!(value(&line, "len_ms").parse::<u64>().unwrap() < 2_100, "{line}");
    // At every moment (the picture switching off, black lifting, the case on its way back): the press ends it, the
    // cursor moves at once, nothing is left playing.
    let presses: Vec<&str> = value(&line, "presses").split(',').collect();
    assert_eq!(presses.len(), 5, "{line}");
    for p in presses {
        let f: Vec<&str> = p.split(':').collect();
        let (ms, playing, ended, moved, handled_us) = (f[0], f[1], f[2], f[3], f[4].parse::<u64>().unwrap());
        assert_eq!(playing, "true", "{ms} ms: it was playing when the button came\n{line}");
        assert_eq!(ended, "true", "{ms} ms: the button ends the way back\n{line}");
        assert_eq!(moved, "true", "{ms} ms: and moves the shelf in the same press (operable from the first frame)\n{line}");
        assert!(handled_us < 16_667, "{ms} ms: the press took {handled_us} µs\n{line}");
    }
}

#[test]
fn a_tv_picture_pulls_together_to_a_line_and_a_handheld_display_goes_dark() {
    let tmp = tempfile::tempdir().unwrap();
    let stub = wayland(tmp.path());
    // SNES: at 165 ms the picture is a bright line through the middle, black above and below.
    let (lib, frame) = library(tmp.path(), "snes", "Chrono Trigger (USA).sfc", serde_json::json!({}));
    let (out, pic) = run(tmp.path(), &stub, &lib, &frame, &[("POCKETUI_SNAPSHOT_EJECT_MS", "165")]);
    assert!(out.contains("eject-snapshot ms=165 playing=true"), "{out}");
    let pic = pic.expect("a picture");
    let mid = pic.1 / 2;
    assert!(row_light(&pic, mid) > 150, "the line in the middle: {}", row_light(&pic, mid));
    assert!(row_light(&pic, mid - 60) < 5 && row_light(&pic, 100) < 5 && row_light(&pic, pic.1 - 100) < 5, "black round it");
    // GBA: no TV. At 120 ms its display is darker than the game was, the same picture (no line).
    let (lib, frame) = library(tmp.path(), "gba", "Golden Sun (USA, Europe).gba", serde_json::json!({}));
    let (out, pic) = run(tmp.path(), &stub, &lib, &frame, &[("POCKETUI_SNAPSHOT_EJECT_MS", "120")]);
    assert!(out.contains("playing=true"), "{out}");
    let pic = pic.expect("a picture");
    let (top, mid) = (row_light(&pic, pic.1 / 2 - 100), row_light(&pic, pic.1 / 2));
    assert!(top > 5 && top < 110, "the blue gone darker, not black yet: {top}");
    assert!(mid > top, "the bright square still there, darker too: {mid}");
    // Later the shelf is back with the case on its way: no black screen left.
    let (_, pic) = run(tmp.path(), &stub, &lib, &frame, &[("POCKETUI_SNAPSHOT_EJECT_MS", "900")]);
    let pic = pic.expect("a picture");
    assert!((0..pic.1).step_by(40).any(|y| row_light(&pic, y) > 25), "the shelf shows");
}

#[test]
fn quick_and_motion_below_full_go_straight_back_to_the_shelf() {
    let tmp = tempfile::tempdir().unwrap();
    let stub = wayland(tmp.path());
    for settings in [serde_json::json!({"animations": "quick"}), serde_json::json!({"motion": "reduced"}), serde_json::json!({"motion": "off"})] {
        let (lib, frame) = library(tmp.path(), "snes", "Chrono Trigger (USA).sfc", settings.clone());
        let (out, _) = run(tmp.path(), &stub, &lib, &frame, &[("POCKETUI_SNAPSHOT_EJECT_MS", "200")]);
        assert!(out.contains("eject: not shown (not Cozy with Motion Full)") && out.contains("playing=false"), "{settings}: {out}");
    }
}

#[test]
fn every_look_that_inserts_also_returns_the_case_and_accepts_the_same_press() {
    let tmp = tempfile::tempdir().unwrap();
    let stub = wayland(tmp.path());
    let (lib, frame) = library(tmp.path(), "snes", "Chrono Trigger (USA).sfc", serde_json::json!({}));
    for scene in ["big-shelf", "kids-shelf", "shelf:snes:shelf:games:0"] {
        let (out, _) = run(tmp.path(), &stub, &lib, &frame, &[("EJECT_TEST_SCENE", scene), ("POCKETUI_SNAPSHOT_EJECT_BENCH", "1")]);
        let line = out.lines().find(|l| l.starts_with("bench-eject")).unwrap_or_else(|| panic!("{scene}: {out}"));
        assert!(!line.contains("started=false"), "{scene}: a look with insertion also has ejection: {out}");
        for press in value(line, "presses").split(',') {
            let fields: Vec<_> = press.split(':').collect();
            assert_eq!(fields[2], "true", "{scene}: the press cancels at every stage: {line}");
        }
    }
}

#[test]
fn the_case_sound_waits_until_the_medium_is_back_inside() {
    use pocket_ui::eject::{Moment, Screen, Way};
    use pocket_ui::{cabinet, insert_rig};
    for lid in [false, true] {
        let way = Way { screen: Screen::Tv, off: true, lid };
        let Moment::Shelf { scene, .. } = way.moment(way.case_shut_at()) else { panic!("the case still returns") };
        assert!(cabinet::insert_at(scene, true).carry < 0.01, "the cartridge is inside its case when it clicks shut: {scene:?}");
        assert!(insert_rig::pose(insert_rig::Kind::Lid, scene).out < 0.01, "a jewel case has closed over the disc when it clicks: {scene:?}");
    }
}

#[test]
fn cartridges_discs_and_missing_art_return_on_each_handheld_family() {
    let tmp = tempfile::tempdir().unwrap();
    let stub = wayland(tmp.path());
    let mut profiles = vec![
        ("miniv2", "1240x1080"), ("mini", "1280x960"), ("odin2mini", "1920x1080"),
        ("rg35xx-plus", "640x480"), ("rgdsplus", "1024x768"), ("rp-classic", "1240x1080"),
    ];
    if cfg!(feature = "brick") { profiles.extend([("brick", "1024x768"), ("smartpro", "1280x720")]); }
    for (system, rom) in [
        ("snes", "Chrono Trigger (USA).sfc"), ("n64", "Super Mario 64 (USA).z64"),
        ("gb", "Tetris (World) (Rev 1).gb"), ("gba", "Golden Sun (USA, Europe).gba"),
        ("psx", "Final Fantasy VII (USA).chd"), ("saturn", "Fixture Disc.chd"),
        ("dreamcast", "Fixture Disc.chd"), ("unknown-console", "Missing Art.chd"),
    ] {
        let (lib, frame) = library(tmp.path(), system, rom, serde_json::json!({}));
        for &(device, size) in &profiles {
            for look in ["cabinet", "shelf", "big", "kids"] {
                let scene = match look {
                    "big" => format!("big-shelf:{rom}"),
                    "kids" => "kids-shelf".to_string(),
                    _ => format!("shelf:{system}:{look}:games:0"),
                };
                let (out, pic) = run(tmp.path(), &stub, &lib, &frame, &[
                    ("EJECT_TEST_SCENE", &scene), ("POCKETUI_DEVICE", device),
                    ("POCKETUI_SNAPSHOT_SIZE", size), ("POCKETUI_SNAPSHOT_EJECT_MS", "900"),
                ]);
                assert!(out.contains("playing=true inserting=true"), "{device} {system} {look}: {out}");
                let pic = pic.expect("the returning case and its console are painted");
                assert!((pic.1 / 3..pic.1 * 9 / 10).step_by(20).any(|y| row_light(&pic, y) > 20), "{device} {system} {look}: no empty gap when art is absent");
            }
        }
    }
}

/// Mean difference of two pictures of the same size, 0-255 per colour channel.
fn picture_diff(a: &[u8], b: &[u8]) -> f64 {
    a.iter().zip(b).map(|(x, y)| x.abs_diff(*y) as u64).sum::<u64>() as f64 / a.len() as f64
}

fn read_png(path: &Path) -> Vec<u8> {
    let mut reader = png::Decoder::new(std::fs::File::open(path).unwrap()).read_info().unwrap();
    let mut buf = vec![0; reader.output_buffer_size()];
    reader.next_frame(&mut buf).unwrap();
    buf
}

/// The last frame of the way back and the first of the still shelf are the same picture, and nothing jumps after it:
/// on 11.11. the case lay front-up at the end of the way back, then snapped to its edge and turned in again (the shelf's
/// move animation started anew), and its game popped out at once. Every look with ejection, every kind of medium and
/// screen, every 60 Hz frame from the end of the game to 0.7 s after it (`POCKETUI_SNAPSHOT_EJECT_SERIES`).
#[test]
fn the_way_back_hands_over_to_the_still_shelf_without_a_jump() {
    let tmp = tempfile::tempdir().unwrap();
    let stub = wayland(tmp.path());
    // A step at the hand-over measured on the device was 7.7 to 14 (mean per channel); the game sliding a little out
    // of its case afterwards is well under 1 a frame.
    const STILL: f64 = 0.5;
    const AFTER: f64 = 1.5;
    let mut failures = Vec::new();
    for (system, rom) in [
        ("snes", "Chrono Trigger (USA).sfc"), ("n64", "Super Mario 64 (USA).z64"),
        ("gb", "Tetris (World) (Rev 1).gb"), ("gba", "Golden Sun (USA, Europe).gba"),
        ("psx", "Final Fantasy VII (USA).chd"), ("saturn", "Fixture Disc.chd"),
        ("dreamcast", "Fixture Disc.chd"), ("unknown-console", "Missing Art.chd"),
    ] {
        let (lib, frame) = library(tmp.path(), system, rom, serde_json::json!({}));
        for look in ["cabinet", "shelf", "big", "kids"] {
            let scene = match look {
                "big" => format!("big-shelf:{rom}"),
                "kids" => "kids-shelf".to_string(),
                _ => format!("shelf:{system}:{look}:games:0"),
            };
            let dir = tmp.path().join(format!("series-{system}-{look}"));
            let (out, _) = run(tmp.path(), &stub, &lib, &frame, &[
                ("EJECT_TEST_SCENE", &scene), ("POCKETUI_SNAPSHOT_SIZE", "640x480"),
                ("POCKETUI_SNAPSHOT_EJECT_SERIES", dir.to_str().unwrap()),
            ]);
            let line = out.lines().find(|l| l.starts_with("eject-series")).unwrap_or_else(|| panic!("{system} {look}: {out}"));
            let (frames, done): (usize, i64) = (value(line, "frames").parse().unwrap(), value(line, "done").parse().unwrap());
            assert!(done > 0 && frames as i64 - done >= 30, "{system} {look}: the series ends after the way back: {line}");
            let done = done as usize;
            let pics: Vec<Vec<u8>> = (done - 1..frames).map(|k| read_png(&dir.join(format!("{k:04}.png")))).collect();
            let steps: Vec<f64> = pics.windows(2).map(|p| picture_diff(&p[0], &p[1])).collect();
            let worst = steps.iter().cloned().fold(0.0, f64::max);
            if steps[0] > STILL || worst > AFTER {
                failures.push(format!("{system} {look}: hand-over step {:.2}, worst step after it {worst:.2} (frame {})", steps[0],
                    done + steps.iter().position(|s| *s == worst).unwrap()));
            }
        }
    }
    assert!(failures.is_empty(), "the case jumps when the way back ends:\n{}", failures.join("\n"));
}
