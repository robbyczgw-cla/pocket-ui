//! Settings → Look → Cozy extras as the player meets them (docs/user/settings.md "Cozy extras"), on the made-up library
//! (scripts/curator-fake-library.py): a journey on the page (A on TV on the cabinet; Down to Stickers on cases, A)
//! lands in settings.json, the Cabinet then stands without its TV, and A again brings it back; a settings.json from
//! before the page keeps every extra as it starts. Needs python3 (and, on Linux without
//! a compositor, scripts/wayland-stub.py), as tests/stickers_tv.rs.

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

fn library(tmp: &Path) -> PathBuf {
    let lib = tmp.join("lib");
    let ok = Command::new("python3").env("POCKETUI_MOCK_NOW", FIXTURE_NOW)
        .args(["-I", &format!("{ROOT}/scripts/curator-fake-library.py"), lib.to_str().unwrap(), "dark"])
        .stdout(std::process::Stdio::null())
        .status()
        .expect("python3 for scripts/curator-fake-library.py");
    assert!(ok.success());
    lib
}

fn write_png(path: &Path, w: u32, h: u32, rgb: [u8; 3]) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut e = png::Encoder::new(std::fs::File::create(path).unwrap(), w, h);
    e.set_color(png::ColorType::Rgb);
    e.set_depth(png::BitDepth::Eight);
    let data: Vec<u8> = (0..w * h).flat_map(|_| rgb).collect();
    e.write_header().unwrap().write_image_data(&data).unwrap();
}

/// Renders `state` with `data` (what it saves stays there); the picture's size and RGB pixels.
fn shot(tmp: &Path, lib: &Path, data: &Path, stub: &Option<Stub>, state: &str, lang: &str) -> (i32, i32, Vec<u8>) {
    shot_in(tmp, lib, data, stub, state, lang, "cabinet")
}

/// `shot` in the look `theme`.
fn shot_in(tmp: &Path, lib: &Path, data: &Path, stub: &Option<Stub>, state: &str, lang: &str, theme: &str) -> (i32, i32, Vec<u8>) {
    let settings = data.join("settings.json");
    let mut json: serde_json::Value = std::fs::read_to_string(&settings).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_else(|| serde_json::json!({}));
    json["theme"] = serde_json::json!(theme);
    json["onboarded"] = serde_json::json!(true);
    std::fs::write(&settings, json.to_string()).unwrap();
    let png = tmp.join(format!("{lang}-{}.png", state.replace([':', ' ', ','], "_")));
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
    let rendered_state = if let Some(s) = state.strip_suffix(":insert") {
        cmd.env("POCKETUI_SNAPSHOT_LAUNCH_MS", "900");
        s
    } else { state };
    if let Some(s) = stub {
        cmd.env("WAYLAND_DISPLAY", &s.1);
    }
    let out = cmd
        .args(["--snapshot", rendered_state, png.to_str().unwrap()])
        .env("POCKETUI_MOCK", "1").env("POCKETUI_MOCK_NOW", FIXTURE_NOW)
        .env("POCKETUI_LANG", lang)
        .env("POCKETUI_ROOM_TIME", "07:00")
        .env("POCKETUI_SNAPSHOT_SIZE", "1240x1080")
        .env("POCKETUI_ES_SYSTEMS", lib.join("es_systems.cfg"))
        .env("POCKETUI_DATA", data)
        .output()
        .expect("run pocket-ui");
    assert!(out.status.success() && png.exists(), "{state}: {}", String::from_utf8_lossy(&out.stderr));
    let mut reader = png::Decoder::new(std::fs::File::open(&png).unwrap()).read_info().unwrap();
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).unwrap();
    assert_eq!(info.color_type, png::ColorType::Rgb);
    (info.width as i32, info.height as i32, buf)
}

fn extras(data: &Path) -> serde_json::Value {
    let s: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(data.join("settings.json")).unwrap()).unwrap();
    s["extras"].clone()
}

#[test]
fn the_dpad_reaches_the_gba_choice_and_every_console_view_uses_the_saved_sp() {
    let tmp = tempfile::tempdir().unwrap();
    let stub = wayland(tmp.path());
    let lib = library(tmp.path());
    let data = lib.join("data");
    let views = ["shelf:gba:shelf", "shelf:gba:cabinet:games:0:insert", "shelf:gba:split", "console:gba"];
    // Compare the pictures that contain hardware, below the live status corner.
    let pixels = |v: (i32, i32, Vec<u8>)| v.2[(v.0 * 110 * 3) as usize..].to_vec();
    let before: Vec<_> = views.iter().map(|s| pixels(shot_in(tmp.path(), &lib, &data, &stub, s, "en", "shelf"))).collect();
    // Up wraps from the first extra to the model choice; A selects SP, immediately saved.
    shot_in(tmp.path(), &lib, &data, &stub, "cozy:0::ua", "en", "shelf");
    let saved: serde_json::Value = serde_json::from_slice(&std::fs::read(data.join("settings.json")).unwrap()).unwrap();
    assert_eq!(saved["gba_model"], "sp", "reachable with the D-pad, under Cozy extras");
    let mut sp_views = Vec::new();
    for (state, original) in views.iter().zip(&before) {
        let sp = pixels(shot_in(tmp.path(), &lib, &data, &stub, state, "en", "shelf"));
        let changed = sp.iter().zip(original).filter(|(a,b)| a != b).count();
        let minimum = if state.ends_with(":insert") { 15_000 } else { 300 };
        assert!(changed > minimum, "{state}: selected SP must replace the classic hardware picture, including after restart ({changed} bytes changed)");
        sp_views.push(sp);
    }
    // Right returns to the default; the same views return to their original hardware picture.
    shot_in(tmp.path(), &lib, &data, &stub, "cozy:8::r", "en", "shelf");
    for ((state, original), sp) in views.iter().zip(before).zip(sp_views) {
        let restored = pixels(shot_in(tmp.path(), &lib, &data, &stub, state, "en", "shelf"));
        if state.ends_with(":insert") {
            // This frame is captured while the cartridge moves. Startup scheduling can
            // advance it a few pixels: it must look like classic hardware again and be
            // clearly unlike the SP, rather than match a moving frame pixel for pixel.
            let difference = |a: &[u8], b: &[u8]| a.iter().zip(b).filter(|(a,b)| a != b).count();
            let (classic, other) = (difference(&restored, &original), difference(&restored, &sp));
            assert!(classic * 10 < other, "{state}: switching back restores classic hardware ({classic} bytes from classic, {other} from SP)");
        } else {
            assert!(restored == original, "{state}: switching back restores the default");
        }
    }
    // A cold cabinet's first paint is at the start of its pull-out animation; a warm one
    // has already advanced. Settle both before comparing hardware pictures pixel for pixel.
    let settings = data.join("settings.json");
    let mut saved: serde_json::Value = serde_json::from_slice(&std::fs::read(&settings).unwrap()).unwrap();
    saved["motion"] = serde_json::json!("off");
    std::fs::write(settings, saved.to_string()).unwrap();
    for look in ["shelf", "cabinet", "split"] {
        // Paint classic first, then choose SP through the actual handler and paint again in
        // the same process. A stale cached case front must not survive that choice.
        let warm = pixels(shot_in(tmp.path(), &lib, &data, &stub, &format!("cozy:0::ua:{look}"), "en", "shelf"));
        let cold = pixels(shot_in(tmp.path(), &lib, &data, &stub, &format!("shelf:gba:{look}"), "en", "shelf"));
        assert!(warm == cold, "{look}: choosing SP immediately replaces already-painted hardware, without a restart");
        shot_in(tmp.path(), &lib, &data, &stub, "cozy:8::r", "en", "shelf");
    }
}

#[test]
fn switching_an_extra_on_the_page_is_saved_and_takes_the_tv_off_the_cabinet() {
    let _guard = tempfile::tempdir().unwrap();
    let tmp = _guard.path().to_path_buf();
    let stub = wayland(&tmp);
    let lib = library(&tmp);
    let data = lib.join("data");
    // A red screenshot for every SNES game: the TV shows red whichever is selected.
    for e in std::fs::read_dir(lib.join("roms/snes")).unwrap().flatten() {
        let stem = e.path().file_stem().unwrap().to_string_lossy().into_owned();
        write_png(&data.join("snaps/snes").join(format!("{stem}.png")), 160, 120, [220, 30, 30]);
    }
    let red_on_tv = |(w, h, px): &(i32, i32, Vec<u8>)| {
        let mut n = 0;
        for y in h / 6..h / 2 {
            for x in w * 3 / 4..*w {
                let i = ((y * w + x) * 3) as usize;
                if px[i] > 140 && px[i + 1] < 90 && px[i + 2] < 90 {
                    n += 1;
                }
            }
        }
        n
    };
    // As it starts: nothing written, the TV on the cabinet.
    shot(&tmp, &lib, &data, &stub, "cozy:0", "en");
    assert!(extras(&data).is_null(), "the start writes no extras: {}", extras(&data));
    assert!(red_on_tv(&shot(&tmp, &lib, &data, &stub, "shelf:snes:cabinet:games:0", "en")) > 1500, "the TV as it starts");
    // A on TV on the cabinet, the first row; Down to Stickers on cases, A (in German: the keys are the same).
    shot(&tmp, &lib, &data, &stub, "cozy:0::ada", "de");
    assert_eq!(extras(&data), serde_json::json!({"cabinet_tv": false, "case_stickers": false}));
    assert_eq!(red_on_tv(&shot(&tmp, &lib, &data, &stub, "shelf:snes:cabinet:games:0", "en")), 0, "no TV with it off");
    // Up to the TV again, Right switches it back; B leaves the page and changes nothing.
    shot(&tmp, &lib, &data, &stub, "cozy:1::urb", "en");
    assert_eq!(extras(&data), serde_json::json!({"case_stickers": false}));
    assert!(red_on_tv(&shot(&tmp, &lib, &data, &stub, "shelf:snes:cabinet:games:0", "en")) > 1500, "the TV is back");
}
