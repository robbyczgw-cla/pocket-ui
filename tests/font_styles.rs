//! Settings → Look & feel → Look → Theme → Font (`src/font_style.rs`) against the snapshot suite: the layout-risk states
//! (`layout-risk-states.txt`: narrow columns, long metadata, hint bars, keyboards, bottom rows) are rendered at 640x480,
//! Large text and German, the tightest corner, with each of the alternative fonts (Pixel: titles and labels, Rounded:
//! everything), and the text painter's record of where each string went (`POCKETUI_TEXT_BOXES`) must show no string
//! running into another (`O`) or touching its neighbour (`T`) and none cut by an edge (`E`), exactly as
//! tests/text_size.rs demands for Barlow; nor may a name be painted with a word broken in pieces (`W`, the first piece
//! of a word `Text::wrap` had to break: "PlayStatio" over "n Portable" on a card), which the wider fonts made likely
//! where a painter only counted lines: they make the name smaller instead (`Text::title_px`, `words_fit`). Further tests check that settings.json's `"font"` reaches a start, that a
//! store theme's own fonts still win, and that a running `Text` follows a change of the style (one process, so one test
//! owns the process-wide setting). Needs python3 (made-up library, wayland stub). About 120 snapshots.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};

const ROOT: &str = env!("CARGO_MANIFEST_DIR");

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
    for _ in 0..200 {
        if stub.path.exists() {
            return Some(stub);
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    panic!("scripts/wayland-stub.py didn't start");
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
    let status = Command::new("python3")
        .args(["-I", &format!("{ROOT}/scripts/curator-fake-library.py"), dir.to_str().unwrap(), "dark"])
        .stdout(std::process::Stdio::null())
        .status()
        .expect("python3 for scripts/curator-fake-library.py");
    assert!(status.success(), "curator-fake-library.py failed");
}

fn risk_states() -> Vec<(bool, String)> {
    let all = std::fs::read_to_string(Path::new(ROOT).join("tests/i18n-states.txt")).expect("tests/i18n-states.txt");
    let safe: std::collections::HashSet<&str> = all.lines().map(str::trim).filter_map(|l| l.strip_prefix("safe ")).collect();
    include_str!("layout-risk-states.txt")
        .lines().map(str::trim).filter(|s| !s.is_empty() && !s.starts_with('#'))
        .chain(BROKEN_WORDS_SEEN)
        .map(|s| (safe.contains(s), s.to_string()))
        .collect()
}

/// States where a wide font broke a word before its painter made the name smaller (2026-10-30): the Cards look's names,
/// the Shelf look's case fronts ("Favoriten"), the Cabinet's fronts, the Year in review's headline.
const BROKEN_WORDS_SEEN: [&str; 4] = ["shelf:ps3:cards", "curator:home:shelf:favorites", "curator:cabinet:ddrrra", "curator:history-hub:cutoff"];

fn check_font(font: &str) {
    let _tmp_guard = tempfile::tempdir().unwrap();
    let tmp = _tmp_guard.path().to_path_buf();
    fake_library(&tmp);
    let bin = PathBuf::from(env!("CARGO_BIN_EXE_pocket-ui"));
    let stub = wayland(&tmp);
    let jobs: Vec<_> = risk_states().into_iter().enumerate().collect();
    let work = Arc::new(Mutex::new(jobs));
    let found = Arc::new(Mutex::new(Vec::new()));
    let threads: Vec<_> = (0..std::thread::available_parallelism().map_or(4, |n| n.get().min(8)))
        .map(|_| {
            let (work, found, tmp, bin, stub) = (work.clone(), found.clone(), tmp.clone(), bin.clone(), stub.as_ref().map(|s| s.path.clone()));
            let font = font.to_string();
            std::thread::spawn(move || loop {
                let Some((i, (safe, state))) = work.lock().unwrap().pop() else { break };
                let name = format!("{font}-{i}");
                let data = tmp.join(format!("data-{name}"));
                copy_dir(&tmp.join("data"), &data);
                let (png, boxes, words) = (tmp.join(format!("{name}.png")), tmp.join(format!("{name}.box")), tmp.join(format!("{name}.words")));
                let mut cmd = Command::new(&bin);
                if safe {
                    cmd.arg("--safe");
                }
                if let Some(path) = &stub {
                    cmd.env("WAYLAND_DISPLAY", path);
                }
                let out = cmd
                    .args(["--snapshot", &state, png.to_str().unwrap()])
                    .env("POCKETUI_MOCK", "1")
                    .env("POCKETUI_LANG", "de")
                    .env("POCKETUI_TEXT_SIZE", "large")
                    .env("POCKETUI_SNAPSHOT_SIZE", "640x480")
                    .env("POCKETUI_FONT", &font)
                    .env("POCKETUI_TEXT_BOXES", &boxes)
                    .env("POCKETUI_I18N_MISSES", &words)
                    .env("POCKETUI_ES_SYSTEMS", tmp.join("es_systems.cfg"))
                    .env("POCKETUI_DATA", &data)
                    .output()
                    .expect("run pocket-ui");
                let mut found = found.lock().unwrap();
                if !out.status.success() || !png.exists() {
                    found.push(format!("{state} didn't render: {}", String::from_utf8_lossy(&out.stderr)));
                    continue;
                }
                match std::fs::read_to_string(&boxes) {
                    Ok(text) => found.extend(text.lines().filter(|l| !l.is_empty()).map(|l| format!("{state}: {}", l.replace('\t', " | ")))),
                    Err(_) => found.push(format!("{state}: no record of the painted text")),
                }
                let broken = std::fs::read_to_string(&words).unwrap_or_default();
                found.extend(broken.lines().filter_map(|l| l.strip_prefix("W\t")).map(|w| format!("{state}: W | a word broken after \"{w}\"")));
                let _ = std::fs::remove_dir_all(&data);
            })
        })
        .collect();
    for t in threads {
        t.join().unwrap();
    }
    drop(stub);
    let mut found = found.lock().unwrap().clone();
    found.sort();
    assert!(
        found.is_empty(),
        "{} problems with the {font} font at 640x480, Large, German (O: runs into another string or a hint bar, T: touches the string beside it, E: cut by the edge, W: a word broken in pieces):\n{}",
        found.len(),
        found.join("\n")
    );
}

#[test]
fn the_pixel_font_clips_nothing_at_640x480_large_german() {
    check_font("pixel");
}

#[test]
fn the_rounded_font_clips_nothing_at_640x480_large_german() {
    check_font("rounded");
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
            std::fs::copy(Path::new(ROOT).join("assets/fonts").join(from), data.join("fonts").join(to)).unwrap();
        }
    }
    let png = tmp.join(format!("{name}.png"));
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
    cmd.args(["--snapshot", "home", png.to_str().unwrap()])
        .env("POCKETUI_MOCK", "1").env("POCKETUI_LANG", "en").env("POCKETUI_TEXT_SIZE", "normal").env("POCKETUI_SNAPSHOT_SIZE", "1280x720")
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
    let pixel = home_png(&tmp, &stub, "d", r#"{"font": "pixel"}"#, None, None);
    let rounded = home_png(&tmp, &stub, "e", r#"{"font": "rounded"}"#, None, None);
    assert!(pixel != barlow && rounded != barlow && pixel != rounded, "the three styles look different");
    assert_eq!(pixel, home_png(&tmp, &stub, "f", "{}", Some("pixel"), None), "POCKETUI_FONT is the same as the setting");
    assert_eq!(barlow, home_png(&tmp, &stub, "g", r#"{"font": "pixel"}"#, Some("barlow"), None), "and wins over it");
    // A theme's title face stays whatever the style: Pixel changes only the title face, so with a theme's title.ttf
    // (and Barlow's body, which is Pixel's body too) Pixel and Barlow come out the same.
    let theme = [("title.ttf", "Nunito-ExtraBold.ttf")];
    assert_eq!(
        home_png(&tmp, &stub, "h", r#"{"font": "pixel"}"#, None, Some(&theme)),
        home_png(&tmp, &stub, "i", r#"{"font": "barlow"}"#, None, Some(&theme)),
        "a theme's title face wins over Pixel"
    );
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
    let mut pixel = Text::new().pinned_to(FontStyle::Pixel);
    let barlow_w = text.width(Face::Title, 50, "Game Boy Advance");
    assert_eq!(text.style(), FontStyle::Barlow);
    for style in [FontStyle::Pixel, FontStyle::Rounded] {
        font_style::set(style);
        let w = text.width(Face::Title, 50, "Game Boy Advance");
        assert_eq!(text.style(), style, "followed");
        assert_ne!(w, barlow_w, "{style:?}: another face, another width");
        assert_eq!(safe.width(Face::Title, 50, "Game Boy Advance"), barlow_w, "builtin() stays Barlow");
        assert_eq!(safe.style(), FontStyle::Barlow);
        assert_eq!(pixel.style(), FontStyle::Pixel, "pinned");
        // The capitals stand as high as Barlow's whatever the face: baselines and row heights keep working.
        for face in [Face::Title, Face::Body] {
            let (top, _) = cap_height(&mut text, face, 50);
            let (top_barlow, _) = cap_height(&mut safe, face, 50);
            assert!((top - top_barlow).abs() <= 1, "{style:?} face {}: capitals top at {top}, Barlow's at {top_barlow}", face as u8);
        }
    }
    assert_eq!(pixel.width(Face::Body, 28, "Game Boy Advance"), safe.width(Face::Body, 28, "Game Boy Advance"), "Pixel keeps Barlow's body");
    font_style::set(FontStyle::Barlow);
    assert_eq!(text.width(Face::Title, 50, "Game Boy Advance"), barlow_w, "and back");
}
