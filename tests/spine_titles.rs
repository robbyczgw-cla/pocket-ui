//! Long titles on the spines (`media::spine_fit`, docs/DESIGN.md "Media"): a trailer's frames of 2026-10-29 showed
//! "Advance…", "Tekken…", "Donkey Kon…" and "Prince of Persia - The Sa…" on spines that had the room. The Shelf and the
//! Cabinet look with long official titles of SNES, GBA and PS2 games, rendered at 640x480 (the H700 handhelds) and
//! 1280x720, in Normal and Large text: no title, nor its main part, is cut short with "…" (`Text::fit` notes every cut
//! in `POCKETUI_I18N_MISSES`).
//!
//! Needs python3 (and, on Linux without a compositor, scripts/wayland-stub.py), as tests/text_size.rs.

use std::path::{Path, PathBuf};
use std::process::Command;

const ROOT: &str = env!("CARGO_MANIFEST_DIR");

/// (system folder, file, title as the shelf shows it)
const GAMES: [(&str, &str, &str); 14] = [
    ("snes", "Donkey Kong Country 2 - Diddy's Kong Quest (USA).sfc", "Donkey Kong Country 2 - Diddy's Kong Quest"),
    ("snes", "Super Mario World 2 - Yoshi's Island (USA).sfc", "Super Mario World 2 - Yoshi's Island"),
    ("snes", "Teenage Mutant Ninja Turtles IV - Turtles in Time (USA).sfc", "Teenage Mutant Ninja Turtles IV - Turtles in Time"),
    ("snes", "Super Castlevania IV (USA).sfc", "Super Castlevania IV"),
    ("gba", "Advance Wars 2 - Black Hole Rising (USA).gba", "Advance Wars 2 - Black Hole Rising"),
    ("gba", "Mario & Luigi - Superstar Saga (USA).gba", "Mario & Luigi - Superstar Saga"),
    ("gba", "Castlevania - Harmony of Dissonance (USA).gba", "Castlevania - Harmony of Dissonance"),
    ("gba", "Golden Sun - The Lost Age (USA).gba", "Golden Sun - The Lost Age"),
    ("gba", "Final Fantasy Tactics Advance (USA).gba", "Final Fantasy Tactics Advance"),
    ("gba", "Legend of Zelda, The - The Minish Cap (USA).gba", "The Legend of Zelda - The Minish Cap"),
    ("ps2", "Prince of Persia - The Sands of Time (USA).iso", "Prince of Persia - The Sands of Time"),
    ("ps2", "Tekken Tag Tournament (USA).iso", "Tekken Tag Tournament"),
    ("ps2", "Grand Theft Auto - San Andreas (USA).iso", "Grand Theft Auto - San Andreas"),
    ("ps2", "Metal Gear Solid 2 - Sons of Liberty (USA).iso", "Metal Gear Solid 2 - Sons of Liberty"),
];

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

/// The made-up library with the long titles added to its SNES, GBA and PS2 folders.
fn library(tmp: &Path) -> PathBuf {
    let lib = tmp.join("lib");
    let ok = Command::new("python3")
        .args(["-I", &format!("{ROOT}/scripts/curator-fake-library.py"), lib.to_str().unwrap(), "dark"])
        .stdout(std::process::Stdio::null())
        .status()
        .expect("python3 for scripts/curator-fake-library.py");
    assert!(ok.success());
    for (system, file, _) in GAMES {
        std::fs::write(lib.join("roms").join(system).join(file), b"made up").unwrap();
    }
    lib
}

/// The titles cut short while `state` was painted at `size`.
fn cuts(tmp: &Path, lib: &Path, stub: &Option<Stub>, look: &str, size: &str, text: &str, state: &str) -> Vec<String> {
    let name = format!("{look}-{size}-{text}-{}", state.replace(':', "_"));
    let data = tmp.join(format!("data-{name}"));
    let _ = std::fs::remove_dir_all(&data);
    let status = Command::new("cp").args(["-r", lib.join("data").to_str().unwrap(), data.to_str().unwrap()]).status().unwrap();
    assert!(status.success());
    let settings = data.join("settings.json");
    let mut json: serde_json::Value = std::fs::read_to_string(&settings).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_else(|| serde_json::json!({}));
    json["theme"] = serde_json::json!(look);
    json["onboarded"] = serde_json::json!(true);
    std::fs::write(&settings, json.to_string()).unwrap();
    let misses = tmp.join(format!("{name}.misses"));
    let png = tmp.join(format!("{name}.png"));
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
    if let Some(s) = stub {
        cmd.env("WAYLAND_DISPLAY", &s.1);
    }
    let out = cmd
        .args(["--snapshot", state, png.to_str().unwrap()])
        .env("POCKETUI_MOCK", "1")
        .env("POCKETUI_LANG", "en")
        .env("POCKETUI_TEXT_SIZE", text)
        .env("POCKETUI_SNAPSHOT_SIZE", size)
        .env("POCKETUI_I18N_MISSES", &misses)
        .env("POCKETUI_ES_SYSTEMS", lib.join("es_systems.cfg"))
        .env("POCKETUI_DATA", &data)
        .output()
        .expect("run pocket-ui");
    assert!(out.status.success() && png.exists(), "{state} at {size}: {}", String::from_utf8_lossy(&out.stderr));
    std::fs::read_to_string(&misses).unwrap_or_default().lines().filter_map(|l| l.strip_prefix("C\t")).map(str::to_string).collect()
}

#[test]
fn long_titles_fit_their_spines_on_small_and_wide_screens() {
    let _tmp_guard = tempfile::tempdir().unwrap();
    let tmp = _tmp_guard.path().to_path_buf();
    let stub = wayland(&tmp);
    let lib = library(&tmp);
    let mut wrong = Vec::new();
    for size in ["640x480", "1280x720"] {
        for text in ["normal", "large"] {
            for state in ["shelf:snes:shelf:games:0", "shelf:gba:shelf:games:0", "shelf:ps2:shelf:games:0", "shelf:snes:cabinet:games:0", "shelf:gba:cabinet:games:0"] {
                for cut in cuts(&tmp, &lib, &stub, "shelf", size, text, state) {
                    // A title, or its part before the subtitle, cut short on a spine.
                    if GAMES.iter().any(|(_, _, title)| *title == cut || title.split(" - ").next() == Some(cut.as_str())) {
                        wrong.push(format!("{state} at {size}, {text} text: \"{cut}\" cut short"));
                    }
                }
            }
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// The shelf on a screen `w` x `h` as `app_cabinet::shelf_layout` lays it out (as tests/shelf_end.rs): the line to look
/// along, above the lifted spine's foot.
fn shelf_line(h: i32) -> i32 {
    let top = if h < 600 { 80 } else { 104 };
    let bottom = h - 112;
    let f = (((bottom - top).max(200) as f32 * 0.42 / 120.0 * 2.0).floor() / 2.0).clamp(1.0, 4.0);
    let floor = bottom - (8.0 * f).round() as i32;
    floor - (10.0 * f).round() as i32 - 12
}

/// The widths of what stands on the shelf along that line, left to right (runs of pixels that differ from the page). A
/// snapshot is the layout's canvas: 640x480 is laid out at 1280x960 (`scaled_layout`) and that is what it measures.
fn widths(png: &Path) -> Vec<i32> {
    let decoder = png::Decoder::new(std::fs::File::open(png).unwrap());
    let mut reader = decoder.read_info().unwrap();
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).unwrap();
    let w = info.width as i32;
    let y = shelf_line(info.height as i32);
    let at = |x: i32| {
        let i = ((y * w + x) * 3) as usize;
        (buf[i], buf[i + 1], buf[i + 2])
    };
    let bg = at(2);
    let mut runs: Vec<(i32, i32)> = Vec::new();
    for x in 0..w {
        if at(x) == bg {
            continue;
        }
        match runs.last_mut() {
            Some(r) if r.1 == x - 1 => r.1 = x,
            _ => runs.push((x, x)),
        }
    }
    runs.iter().map(|r| r.1 - r.0 + 1).collect()
}

/// The owner, 2026-10-29: "same platform, different sizes?" Every box of one system stands on the shelf exactly as wide
/// as the others, white covers, dark covers, long titles and the selected one alike (it is lifted, not scaled).
#[test]
fn every_box_of_one_system_stands_as_wide_as_the_others() {
    let _tmp_guard = tempfile::tempdir().unwrap();
    let tmp = _tmp_guard.path().to_path_buf();
    let stub = wayland(&tmp);
    let lib = library(&tmp);
    let mut wrong = Vec::new();
    for size in ["1240x1080", "640x480", "1280x720"] {
        for state in ["shelf:snes:shelf:games:0", "shelf:gba:shelf:games:2", "shelf:ps2:shelf:games:1"] {
            cuts(&tmp, &lib, &stub, "shelf", size, "normal", state);
            let png = tmp.join(format!("shelf-{size}-normal-{}.png", state.replace(':', "_")));
            let runs = widths(&png);
            // The spines come first, all one width; after them only the bookend and the props (at most three runs): a
            // spine a pixel off would end the run of equal widths among the spines, with more of them after it.
            let first = runs.first().copied().unwrap_or(0);
            let n = runs.iter().take_while(|w| **w == first).count();
            if n < 4 || runs.len() - n > 3 {
                wrong.push(format!("{state} at {size}: {runs:?}"));
            }
        }
    }
    assert!(wrong.is_empty(), "spines of one system of different widths:\n{}", wrong.join("\n"));
}
