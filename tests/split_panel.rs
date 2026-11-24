//! Split's picture panel on every screen: the console over a system's game count, a game's cover big, in the RG35XX
//! Plus's share of the screen (640x480, laid out at 1280x960). Found on the Brick's photos (0.6): on 1024x768 the panel
//! gave up all 312 lines it lacked to 1080, a 108-line strip with a 60-pixel cover and no console at all; the Smart Pro's
//! 1280x720 had a 60-line strip. No test looked at the pictures, only at the words.
//!
//! The `shelf:` snapshots print where the panel and the console stand (`split-panel`, `split-console`, in the canvas'
//! pixels); the cover is found in the picture by its colour. Needs python3 on Linux without a compositor
//! (scripts/wayland-stub.py), as tests/lived_in_shelf.rs.

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::Command;

include!("common/root.rs");
include!("common/shelf_library.rs");

/// The screens Split is shown on, with the device each is (the Brick build takes its type from it). The Plus first: the
/// share the others are measured against.
const SCREENS: [(&str, &str); 6] =
    [("640x480", "rg35xx-plus"), ("1024x768", "brick"), ("1280x720", "smartpro"), ("1280x960", ""), ("1240x1080", "miniv2"), ("1920x1080", "")];

/// The cover's colour: nothing else on the page has it.
const COVER: [u8; 3] = [255, 0, 255];

struct Seen {
    /// (x, y, w, h) of the panel and of the console (w 0: none), and the canvas they are measured on.
    panel: [i32; 4],
    console: [i32; 4],
    canvas: (i32, i32),
}

fn field(line: &str, key: &str) -> i32 {
    line.split_whitespace().find_map(|p| p.strip_prefix(&format!("{key}="))).and_then(|v| v.parse().ok()).unwrap_or_else(|| panic!("{key} in {line}"))
}

fn seen(stdout: &str) -> Seen {
    let panel = stdout.lines().find(|l| l.starts_with("split-panel ")).unwrap_or_else(|| panic!("no split-panel line: {stdout}"));
    let console = stdout.lines().find(|l| l.starts_with("split-console ")).unwrap();
    let canvas = panel.split_whitespace().find_map(|p| p.strip_prefix("canvas=")).unwrap();
    let (cw, ch) = canvas.split_once('x').unwrap();
    let four = |l: &str| [field(l, "x"), field(l, "y"), field(l, "w"), field(l, "h")];
    Seen { panel: four(panel), console: four(console), canvas: (cw.parse().unwrap(), ch.parse().unwrap()) }
}

/// The library of tests/common/shelf_library.rs in Split, with a cover of [`COVER`] for every PlayStation game.
fn split_library(dir: &Path) -> PathBuf {
    let lib = library_in(dir, None, "split");
    let covers = lib.join("data/covers/psx");
    std::fs::create_dir_all(&covers).unwrap();
    let mut png = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut png, 400, 400);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header().unwrap().write_image_data(&COVER.repeat(400 * 400)).unwrap();
    }
    for entry in std::fs::read_dir(lib.join("roms/psx")).unwrap() {
        let stem = entry.unwrap().path().file_stem().unwrap().to_string_lossy().into_owned();
        std::fs::write(covers.join(format!("{stem}.png")), &png).unwrap();
    }
    lib
}

/// pocket-ui `--snapshot <state>` at `size` as `device`: its stdout and the picture (width, height, RGB).
fn shot(lib: &Path, stub: &Option<Stub>, size: &str, device: &str, state: &str) -> (String, (i32, i32, Vec<u8>)) {
    let png = lib.join(format!("{size}-{}.png", state.replace(':', "_")));
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
    if let Some(s) = stub {
        cmd.env("WAYLAND_DISPLAY", &s.1);
    }
    if !device.is_empty() {
        cmd.env("POCKETUI_DEVICE", device);
    }
    let out = cmd
        .args(["--snapshot", state, png.to_str().unwrap()])
        .env("POCKETUI_MOCK", "1")
        .env("POCKETUI_MOCK_NOW", FIXTURE_NOW)
        .env("POCKETUI_LANG", "en")
        .env("POCKETUI_TEXT_SIZE", "normal")
        // A file there: the games route to pocket-core, as on a device.
        .env("POCKETUI_HOST_BIN", "/bin/sh")
        .env("POCKETUI_SNAPSHOT_SIZE", size)
        .env("POCKETUI_ES_SYSTEMS", lib.join("es_systems.cfg"))
        .env("POCKETUI_DATA", lib.join("data"))
        .output()
        .expect("run pocket-ui");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(out.status.success() && png.exists(), "{state} at {size}: {}", String::from_utf8_lossy(&out.stderr));
    // `SPLIT_SHOTS=<dir>` keeps the pictures.
    if let Some(dir) = std::env::var_os("SPLIT_SHOTS") {
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::copy(&png, Path::new(&dir).join(png.file_name().unwrap()));
    }
    let decoder = png::Decoder::new(std::fs::File::open(&png).unwrap());
    let mut reader = decoder.read_info().unwrap();
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).unwrap();
    assert_eq!(info.color_type, png::ColorType::Rgb);
    (stdout, (info.width as i32, info.height as i32, buf[..info.buffer_size()].to_vec()))
}

/// The box around the cover's pixels (x0, y0, x1, y1), in the picture's pixels.
fn cover_box((w, h, rgb): &(i32, i32, Vec<u8>)) -> Option<(i32, i32, i32, i32)> {
    let mut b: Option<(i32, i32, i32, i32)> = None;
    for y in 0..*h {
        for x in 0..*w {
            let i = ((y * w + x) * 3) as usize;
            let p = &rgb[i..i + 3];
            if p[0] > 230 && p[1] < 30 && p[2] > 230 {
                b = Some(b.map_or((x, y, x + 1, y + 1), |(x0, y0, x1, y1)| (x0.min(x), y0.min(y), x1.max(x + 1), y1.max(y + 1))));
            }
        }
    }
    b
}

#[test]
fn the_panel_shows_the_console_and_a_big_cover_in_the_plus_share_of_every_screen() {
    let tmp = tempfile::Builder::new().prefix("pui-split-").tempdir().unwrap();
    let stub = wayland(tmp.path());
    let lib = split_library(&tmp.path().join("lib"));
    // Per screen: the panel's and the console's share of the canvas' height, the cover's of the picture's.
    let mut plus: Option<(f32, f32, f32)> = None;
    let mut bad = Vec::new();
    for (size, device) in SCREENS {
        let (out, _) = shot(&lib, &stub, size, device, "shelf:snes:split");
        let s = seen(&out);
        let ch = s.canvas.1 as f32;
        let (panel, console) = (s.panel[3] as f32 / ch, s.console[3] as f32 / ch);
        if s.console[2] == 0 {
            bad.push(format!("{size}: no console over the game count"));
        }
        // The console stays in the panel.
        if s.console[2] > 0 && (s.console[1] < s.panel[1] || s.console[1] + s.console[3] > s.panel[1] + s.panel[3]) {
            bad.push(format!("{size}: the console {:?} leaves the panel {:?}", s.console, s.panel));
        }
        let (game_out, picture) = shot(&lib, &stub, size, device, "shelf:psx:split:games:0");
        let g = seen(&game_out);
        let Some((x0, y0, x1, y1)) = cover_box(&picture) else {
            bad.push(format!("{size}: no cover"));
            continue;
        };
        let cover = (y1 - y0) as f32 / picture.1 as f32;
        // The cover stands inside the panel (canvas to picture: the H700 panels' half).
        let k = picture.1 as f32 / g.canvas.1 as f32;
        let p = g.panel.map(|v| (v as f32 * k).round() as i32);
        if x0 < p[0] || y0 < p[1] || x1 > p[0] + p[2] || y1 > p[1] + p[3] {
            bad.push(format!("{size}: the cover {:?} leaves the panel {p:?}", (x0, y0, x1, y1)));
        }
        let (pp, pc, pv) = *plus.get_or_insert((panel, console, cover));
        println!("{size}: panel {:.3} of the height (Plus {pp:.3}), console {console:.3} ({pc:.3}), cover {cover:.3} ({pv:.3})", panel);
        // Short screens keep the Plus's share; taller ones have more.
        if panel < pp * 0.95 || (s.canvas.1 < 960 && panel > pp * 1.05) {
            bad.push(format!("{size}: the panel is {panel:.3} of the height, the Plus's {pp:.3}"));
        }
        if console < pc * 0.7 {
            bad.push(format!("{size}: the console is {console:.3} of the height, the Plus's {pc:.3}"));
        }
        if cover < pv * 0.9 || cover < 0.2 {
            bad.push(format!("{size}: the cover is {cover:.3} of the height, the Plus's {pv:.3}"));
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}
