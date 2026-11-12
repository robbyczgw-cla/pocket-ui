//! Settings → Look → Style (`src/font_style.rs`): settings.json's `"font"` reaches a start (and the styles that went with
//! 0.4, Rounded and Pixel, start as Barlow), a store theme's own fonts are used, and a running `Text` follows a change
//! of the style (one process, so one test owns the process-wide setting). Retro's text fit is tests/text_size.rs's
//! (`POCKETUI_RETRO`). Until 0.4 this suite also rendered the layout-risk states in the Pixel style, about 120
//! snapshots. Needs python3 (made-up library, wayland stub).

use std::path::{Path, PathBuf};
use std::process::Command;

include!("common/root.rs");

/// scripts/wayland-stub.py, while it runs.
struct Stub {
    path: PathBuf,
    child: std::process::Child,
}

impl Drop for Stub {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn wayland(tmp: &Path) -> Option<Stub> {
    if !cfg!(target_os = "linux") || std::env::var_os("WAYLAND_DISPLAY").is_some_and(|w| !w.is_empty()) {
        return None;
    }
    let path = tmp.join("wayland-stub");
    let child = Command::new("python3").args(["-I", &format!("{ROOT}/scripts/wayland-stub.py"), path.to_str().unwrap()]).spawn().expect("python3 for scripts/wayland-stub.py");
    let stub = Stub { path, child };
    test_clock::wait_for(|| stub.path.exists(), std::time::Duration::from_secs(2), "Wayland stub listening");
    Some(stub)
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap().flatten() {
        let target = to.join(e.file_name());
        if e.path().is_dir() {
            copy_dir(&e.path(), &target);
        } else {
            std::fs::copy(e.path(), target).unwrap();
        }
    }
}

fn fake_library(dir: &Path) {
    let status = Command::new("python3").env("POCKETUI_MOCK_NOW", FIXTURE_NOW)
        .args(["-I", &format!("{ROOT}/scripts/curator-fake-library.py"), dir.to_str().unwrap(), "dark"])
        .stdout(std::process::Stdio::null())
        .status()
        .expect("python3 for scripts/curator-fake-library.py");
    assert!(status.success(), "curator-fake-library.py failed");
}

/// The PNG of the Home snapshot with `settings` (settings.json) and, when given, `POCKETUI_FONT` and a theme's faces.
fn home_png(tmp: &Path, stub: &Option<Stub>, name: &str, settings: &str, font: Option<&str>, theme: Option<&[(&str, &str)]>) -> Vec<u8> {
    let data = tmp.join(name);
    copy_dir(&tmp.join("data"), &data);
    let mut json: serde_json::Value = serde_json::from_slice(&std::fs::read(data.join("settings.json")).unwrap_or_else(|_| b"{}".to_vec())).unwrap();
    for (k, v) in serde_json::from_str::<serde_json::Value>(settings).unwrap().as_object().unwrap() {
        json[k] = v.clone();
    }
    // Nothing that moves with the clock: the playful spines' lean and the light by the time of day (Cozy extras).
    json["extras"]["playful_animations"] = serde_json::json!(false);
    json["extras"]["time_of_day_light"] = serde_json::json!(false);
    std::fs::write(data.join("settings.json"), serde_json::to_vec(&json).unwrap()).unwrap();
    if let Some(files) = theme {
        std::fs::create_dir_all(data.join("fonts")).unwrap();
        for (to, from) in files {
            std::fs::copy(Path::new(&ROOT).join("assets/fonts").join(from), data.join("fonts").join(to)).unwrap();
        }
    }
    let png = tmp.join(format!("{name}.png"));
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
    cmd.args(["--snapshot", "home", png.to_str().unwrap()])
        .env("POCKETUI_MOCK", "1").env("POCKETUI_MOCK_NOW", FIXTURE_NOW).env("POCKETUI_LANG", "en").env("POCKETUI_TEXT_SIZE", "normal").env("POCKETUI_SNAPSHOT_SIZE", "1280x720")
        .env("POCKETUI_ES_SYSTEMS", tmp.join("es_systems.cfg")).env("POCKETUI_DATA", &data)
        .env_remove("POCKETUI_FONT");
    if let Some(f) = font {
        cmd.env("POCKETUI_FONT", f);
    }
    if let Some(w) = stub {
        cmd.env("WAYLAND_DISPLAY", &w.path);
    }
    let out = cmd.output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    // Without the status corner (the clock moves on while the tests run) and the console's picture, which is animated.
    let mut reader = png::Decoder::new(std::fs::File::open(png).unwrap()).read_info().unwrap();
    let mut pixels = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut pixels).unwrap();
    let (w, h) = (info.width as usize, info.height as usize);
    let bytes = info.line_size / w;
    for y in 0..h {
        for x in 0..w {
            if y < 120 || (x < 360 && y < 400) {
                pixels[y * info.line_size + x * bytes..][..bytes].fill(0);
            }
        }
    }
    pixels
}

/// The choice in settings.json is what a start uses, Barlow without one (or with a wrong one), and a store theme's own
/// faces win over the style.
#[test]
fn the_font_in_settings_json_is_used_and_a_themes_own_fonts_win() {
    let _tmp_guard = tempfile::tempdir().unwrap();
    let tmp = _tmp_guard.path().to_path_buf();
    fake_library(&tmp);
    let stub = wayland(&tmp);
    let barlow = home_png(&tmp, &stub, "a", "{}", None, None);
    assert_eq!(barlow, home_png(&tmp, &stub, "b", r#"{"font": "barlow"}"#, None, None), "Barlow is the default");
    assert_eq!(barlow, home_png(&tmp, &stub, "c", r#"{"font": "comic sans"}"#, None, None), "a wrong value is Barlow");
    let retro = home_png(&tmp, &stub, "d", r#"{"font": "retro"}"#, None, None);
    let rounded = home_png(&tmp, &stub, "e", r#"{"font": "rounded"}"#, None, None);
    let pixel = home_png(&tmp, &stub, "j", r#"{"font": "pixel"}"#, None, None);
    assert_ne!(retro, barlow, "Retro paints its own");
    assert_eq!(rounded, barlow, "the retired Rounded style migrates to Barlow");
    assert_eq!(pixel, barlow, "the retired Pixel style migrates to Barlow, not to Retro");
    assert_eq!(retro, home_png(&tmp, &stub, "f", "{}", Some("retro"), None), "POCKETUI_FONT is the same as the setting");
    assert_eq!(barlow, home_png(&tmp, &stub, "g", r#"{"font": "retro"}"#, Some("barlow"), None), "and wins over it");
    // A theme's title face wins over the built-in style's.
    let theme = [("title.ttf", "DepartureMono-Regular.otf")];
    assert_ne!(home_png(&tmp, &stub, "h", r#"{"font": "barlow"}"#, None, Some(&theme)), barlow, "a theme's title face is used");
}

/// A `Text` takes up a change of the style by itself: its next glyph is the new face's, the old glyphs are gone, a
/// pinned one and `builtin()` (safe mode) stay as they are. The only test here that sets the process-wide style.
#[test]
fn a_text_follows_the_style_and_a_pinned_one_does_not() {
    use pocket_ui::font_style::{self, FontStyle};
    use pocket_ui::text::{Face, Text};
    let cap_height = |text: &mut Text, face: Face, px: u32| -> (i32, i32) {
        // Where 'H' is painted: its top row and its width on a canvas, by the baseline at 100.
        let mut buf = vec![0u32; 200 * 120];
        let mut c = pocket_ui::gfx::Canvas::new(&mut buf, 200, 120);
        let w = text.draw(&mut c, face, px, 10, 100, 0xffffff, "H");
        let top = (0..120).find(|y| buf[*y as usize * 200..(*y as usize + 1) * 200].iter().any(|p| *p != 0)).unwrap();
        (top, w)
    };
    font_style::set(FontStyle::Barlow);
    let mut text = Text::new();
    let mut safe = Text::builtin();
    let mut pinned = Text::new().pinned_to(FontStyle::Retro);
    let barlow_w = text.width(Face::Title, 50, "Game Boy Advance");
    assert_eq!(text.style(), FontStyle::Barlow);
    for style in [FontStyle::Retro] {
        font_style::set(style);
        let w = text.width(Face::Title, 50, "Game Boy Advance");
        assert_eq!(text.style(), style, "followed");
        assert_ne!(w, barlow_w, "{style:?}: another face, another width");
        assert_eq!(safe.width(Face::Title, 50, "Game Boy Advance"), barlow_w, "builtin() stays Barlow");
        assert_eq!(safe.style(), FontStyle::Barlow);
        assert_eq!(pinned.style(), FontStyle::Retro, "pinned");
        // Retro's capitals stand a little lower than Barlow's on the same baseline (0.9 of the title's, 0.85 of the body's,
        // docs/DESIGN.md "Font styles"), never taller: rows and baselines laid out for Barlow still hold them.
        for face in [Face::Title, Face::Body] {
            let (top, _) = cap_height(&mut text, face, 50);
            let (top_barlow, _) = cap_height(&mut safe, face, 50);
            let (h, h_barlow) = (100 - top, 100 - top_barlow);
            assert!(h <= h_barlow && h * 5 >= h_barlow * 4, "{style:?} face {}: capitals {h} px high, Barlow's {h_barlow}", face as u8);
        }
    }
    font_style::set(FontStyle::Barlow);
    assert_eq!(text.width(Face::Title, 50, "Game Boy Advance"), barlow_w, "and back");
}
