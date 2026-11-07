//! The controls card (`src/controls_card.rs`, `src/app_controls_card.rs`) as the player sees it, rendered with
//! `pocket-ui --snapshot controls-card…` on scripts/curator-fake-library.py's made-up library: a drawing with the
//! console's buttons for the consoles the table knows, in every one of them something where the buttons go, and for a console
//! with no names yet a sentence instead of an empty page. The player's own changes show (a remapped button is drawn in
//! another colour). The game card's Controls row and Options > Controls open it.
//!
//! Needs python3 for the made-up library and, on Linux without a compositor, scripts/wayland-stub.py (as
//! tests/gallery_snapshots.rs).

use std::path::{Path, PathBuf};
use std::process::Command;

include!("common/root.rs");

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

/// A screen as (width, height, 0RGB pixels).
fn read_png(path: &Path) -> (usize, usize, Vec<u32>) {
    let mut reader = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(path).expect("snapshot written"))).read_info().unwrap();
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).unwrap();
    let ch = info.color_type.samples();
    let px = buf[..info.buffer_size()].chunks_exact(ch).map(|p| (p[0] as u32) << 16 | (p[1] as u32) << 8 | p[2] as u32).collect();
    (info.width as usize, info.height as usize, px)
}

/// How many pixels of the box (x, y, w, h) differ from the page colour at the screen's top left.
fn ink(shot: &(usize, usize, Vec<u32>), (x, y, w, h): (usize, usize, usize, usize)) -> usize {
    let bg = shot.2[0];
    (y..(y + h).min(shot.1)).flat_map(|row| (x..(x + w).min(shot.0)).map(move |col| (row, col))).filter(|&(row, col)| shot.2[row * shot.0 + col] != bg).count()
}

fn render(bin: &Path, tmp: &Path, state: &str, size: &str, text_size: &str, stub: Option<&Stub>) -> (usize, usize, Vec<u32>) {
    render_on(bin, tmp, state, size, text_size, stub, None)
}

fn render_on(bin: &Path, tmp: &Path, state: &str, size: &str, text_size: &str, stub: Option<&Stub>, device: Option<(&str, &str)>) -> (usize, usize, Vec<u32>) {
    let data = tmp.join(format!("data-{}-{size}", state.replace(':', "-")));
    copy_dir(&tmp.join("dark").join("data"), &data);
    let png = tmp.join(format!("{}-{size}.png", state.replace(':', "-")));
    let boxes = tmp.join("layout.boxes");
    let transcript = tmp.join("visible.txt");
    let mut cmd = Command::new(bin);
    if let Some(s) = stub {
        cmd.env("WAYLAND_DISPLAY", &s.path);
    }
    if let Some((device, _)) = device { cmd.env("POCKETUI_DEVICE", device); }
    let out = cmd
        .args(["--snapshot", state, png.to_str().unwrap()])
        .env("POCKETUI_MOCK", "1")
        .env("POCKETUI_ES_SYSTEMS", tmp.join("dark").join("es_systems.cfg"))
        .env("POCKETUI_DATA", &data)
        .env("POCKETUI_SNAPSHOT_SIZE", size)
        .env("POCKETUI_TEXT_SIZE", text_size)
        .env("POCKETUI_TEXT_BOXES", &boxes)
        .env("POCKETUI_LANG", device.map_or("en", |(_, lang)| lang))
        .env("POCKETUI_VISIBLE_TEXT", &transcript)
        .env_remove("POCKETUI_SCREENSHOTS")
        .output()
        .expect("run pocket-ui");
    assert!(out.status.success(), "{state}: {}", String::from_utf8_lossy(&out.stderr));
    if !state.starts_with("controls-card-row:") {
        let problems = std::fs::read_to_string(&boxes).unwrap();
        assert!(problems.trim().is_empty(), "{state} {size} {text_size}: labels must be readable and separate: {problems}");
    }
    let shown = std::fs::read_to_string(&transcript).unwrap();
    if state == "controls-card:n64" {
        assert!(shown.lines().any(|s| s == "Control stick"), "{size} {text_size}: show the control's full name: {shown}");
    }
    if state == "controls-card:psx" {
        assert!(shown.lines().any(|s| s == "R3"), "{size} {text_size}: the right stick click must remain visible: {shown}");
    }
    read_png(&png)
}

#[test]
fn every_console_shows_a_card_and_a_console_without_names_says_so() {
    let guard = tempfile::tempdir().unwrap();
    let tmp = guard.path().to_path_buf();
    let status = Command::new("python3")
        .args(["-I", &format!("{ROOT}/scripts/curator-fake-library.py"), tmp.join("dark").to_str().unwrap(), "dark"])
        .stdout(std::process::Stdio::null())
        .status()
        .expect("python3 for scripts/curator-fake-library.py");
    assert!(status.success(), "curator-fake-library.py failed");
    let bin = PathBuf::from(env!("CARGO_BIN_EXE_pocket-ui"));
    let stub = wayland(&tmp);
    for (size, text_size) in [("1240x1080", "normal"), ("640x480", "normal"), ("640x480", "large"), ("1024x768", "normal"), ("1024x768", "large")] {
        let (w, h) = size.split_once('x').map(|(w, h)| (w.parse::<usize>().unwrap(), h.parse::<usize>().unwrap())).unwrap();
        // The drawing's middle third: the face buttons and their names, the D-pad and the sticks.
        let body = (w / 20, h * 3 / 10, w * 9 / 10, h * 4 / 10);
        let mut shots = Vec::new();
        for system in ["snes", "gba", "psx", "n64"] {
            let shot = render(&bin, &tmp, &format!("controls-card:{system}"), size, text_size, stub.as_ref());
            assert!(ink(&shot, body) > (w * h) / 400, "{system} at {size}: the buttons are drawn");
            // The HOME keys under it.
            assert!(ink(&shot, (w / 20, h * 3 / 4 - h / 10, w * 9 / 10, h / 8)) > 200, "{system} at {size}: HOME's keys are there");
            shots.push(shot);
        }
        assert!(shots.windows(2).all(|p| p[0].2 != p[1].2), "{size}: each console has its own card");
        // The player's remap shows.
        let plain = &shots[0];
        let remapped = render(&bin, &tmp, "controls-card:snes:remap", size, text_size, stub.as_ref());
        assert_ne!(plain.2, remapped.2, "{size}: a remapped button is drawn differently");
        // A console that has told no names yet: a sentence where the buttons would be, not an empty page.
        let unknown = render(&bin, &tmp, "controls-card:megadrive", size, text_size, stub.as_ref());
        assert!(ink(&unknown, (w / 20, h / 6, w * 9 / 10, h / 8)) > 300, "{size}: the page says why there are no buttons");
        // The first start's card says what A does, as the hint bar.
        let first = render(&bin, &tmp, "controls-card:gba:first", size, text_size, stub.as_ref());
        assert!(ink(&first, (w * 6 / 10, h * 8 / 10, w * 4 / 10, h / 5)) > 300, "{size}: the hint bar of the first start's card");
        // The game card's Controls row, and Options > Controls in the pause menu.
        let row = render(&bin, &tmp, "controls-card-row:snes:chrono", size, text_size, stub.as_ref());
        assert!(ink(&row, (0, h / 2, w, h / 2)) > (w * h) / 100, "{size}: the game card with its Controls row");
        let pause = render(&bin, &tmp, "controls-card-pause", size, text_size, stub.as_ref());
        assert!(ink(&pause, body) > (w * h) / 400, "{size}: Options > Controls draws the card of the game that runs");
    }
}

#[cfg(feature = "brick")]
#[test]
fn printed_menu_key_is_used_on_brick_cards_and_hints_in_both_appearances() {
    let tmp = tempfile::tempdir().unwrap();
    let out = Command::new("python3").args(["-I", &format!("{ROOT}/scripts/curator-fake-library.py"), tmp.path().join("dark").to_str().unwrap(), "dark"]).output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let stub = wayland(tmp.path());
    let bin = Path::new(env!("CARGO_BIN_EXE_pocket-ui"));
    for device in ["brick", "miniv2"] {
        for appearance in ["dark", "light"] {
            let settings = tmp.path().join("dark/data/settings.json");
            let mut saved: serde_json::Value = serde_json::from_slice(&std::fs::read(&settings).unwrap()).unwrap();
            saved["appearance"] = appearance.into();
            std::fs::write(&settings, serde_json::to_vec(&saved).unwrap()).unwrap();
            for (lang, state) in ["en", "de", "es"].into_iter().flat_map(|lang| ["controls-card:snes", "home"].map(|state| (lang, state))) {
                render_on(bin, tmp.path(), state, "1024x768", "normal", stub.as_ref(), Some((device, lang)));
                let visible = std::fs::read_to_string(tmp.path().join("visible.txt")).unwrap();
                let (wanted, absent) = if device == "brick" { ("MENU", "HOME") } else { ("HOME", "MENU") };
                assert!(visible.contains(wanted), "{device} {appearance} {state}: {visible}");
                assert!(!visible.contains(absent), "{device} {appearance} {state}: {visible}");
            }
        }
    }
}
