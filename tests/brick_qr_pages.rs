//! The pairing pages with a QR code and PC streaming's address keyboard on the Brick's 1024x768 (Neon's Brick audit,
//! 20.11.2026, #4 #5 #6 #16). The Brick paints natively with the text sizes of 960 lines, so a QR code of fixed pixels
//! left the words beside it a third of the width: Tailscale cut three of its four steps with "…" in Normal English,
//! the web pairing its instructions and how long the code holds; RomM's code shrank to a quarter of the height in Large
//! German, the space under it empty; and the line under PC streaming's keyboard stood at y=800, off the picture.
//!
//! (RomM's Large German follows in 0.6.1.)
//!
//! Each page at the Plus (the reference) and the Brick, Normal English and Large German: no string of it cut with "…",
//! every step on the picture, nothing running into the hint bar (`POCKETUI_TEXT_BOXES` `O`/`V`), and the QR code at the
//! Plus's share of the height within five points. Needs python3 on Linux without a compositor (scripts/wayland-stub.py).

#![allow(dead_code)]

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

include!("common/root.rs");
include!("common/shelf_library.rs");

const PLUS: (&str, &str) = ("640x480", "rg35xx-plus");
const BRICK: (&str, &str) = ("1024x768", "brick");

/// What one snapshot showed: the marks (x, y, w, h and the canvas's height), the strings painted, the text-box faults.
struct Shot {
    marks: HashMap<String, ([i32; 4], i32)>,
    visible: Vec<String>,
    faults: Vec<String>,
}

fn shot(lib: &Path, stub: &Option<Stub>, (size, device): (&str, &str), state: &str, lang: &str, text: &str) -> Shot {
    let png = lib.join(format!("qr-{size}-{}-{lang}-{text}.png", state.replace(':', "_")));
    let (visible, boxes) = (lib.join(format!("{}-{size}-{lang}-{text}.vis", state.replace(':', "_"))), lib.join(format!("{}-{size}-{lang}-{text}.box", state.replace(':', "_"))));
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
    if let Some(s) = stub {
        cmd.env("WAYLAND_DISPLAY", &s.1);
    }
    let out = cmd
        .args(["--snapshot", state, png.to_str().unwrap()])
        .env("POCKETUI_DEVICE", device)
        .env("POCKETUI_SHARES", "1")
        .env("POCKETUI_MOCK", "1")
        .env("POCKETUI_MOCK_NOW", FIXTURE_NOW)
        .env("POCKETUI_LANG", lang)
        .env("POCKETUI_TEXT_SIZE", text)
        .env("POCKETUI_SNAPSHOT_SIZE", size)
        .env("POCKETUI_VISIBLE_TEXT", &visible)
        .env("POCKETUI_TEXT_BOXES", &boxes)
        .env("POCKETUI_ES_SYSTEMS", lib.join("es_systems.cfg"))
        .env("POCKETUI_DATA", lib.join("data"))
        .output()
        .expect("run pocket-ui");
    assert!(out.status.success() && png.exists(), "{state} at {size}: {}", String::from_utf8_lossy(&out.stderr));
    // `BRICK_SHOTS=<dir>` keeps the pictures.
    if let Some(dir) = std::env::var_os("BRICK_SHOTS") {
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::copy(&png, Path::new(&dir).join(png.file_name().unwrap()));
    }
    let _ = std::fs::remove_file(&png);
    let marks = String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| l.strip_prefix("share "))
        .map(|l| {
            let mut it = l.split_whitespace();
            let name = it.next().unwrap().to_string();
            let f: HashMap<&str, &str> = it.filter_map(|p| p.split_once('=')).collect();
            let n = |k: &str| f[k].parse::<i32>().unwrap();
            let ch = f["canvas"].split_once('x').unwrap().1.parse().unwrap();
            (name, ([n("x"), n("y"), n("w"), n("h")], ch))
        })
        .collect();
    let read = |p: &Path| std::fs::read_to_string(p).unwrap_or_default().lines().map(str::to_string).collect::<Vec<_>>();
    let faults = read(&boxes).into_iter().filter(|l| l.starts_with("O\t") || l.starts_with("V\t")).collect();
    Shot { marks, visible: read(&visible), faults }
}

fn setup() -> (tempfile::TempDir, Option<Stub>, PathBuf) {
    let tmp = tempfile::Builder::new().prefix("pui-qr-").tempdir().unwrap();
    let stub = wayland(tmp.path());
    let lib = library_in(&tmp.path().join("lib"), None, "shelf");
    (tmp, stub, lib)
}

const CORNERS: [(&str, &str); 2] = [("en", "normal"), ("de", "large")];

/// Tailscale's sign-in and the web pairing: every line whole and on the picture beside a QR code that
/// keeps the Plus's share of the height.
#[test]
fn the_qr_pages_keep_every_step_whole_and_the_plus_share_on_the_brick() {
    let (_tmp, stub, lib) = setup();
    let mut bad = Vec::new();
    for (state, qr, must) in [
        ("tailscale:login", "tailscale-qr", &["Sign in", "approve"][..]),
        ("web:pair", "web-qr", &["Five wrong", "Valid for"][..]),
    ] {
        for (lang, text) in CORNERS {
            let plus = shot(&lib, &stub, PLUS, state, lang, text);
            let brick = shot(&lib, &stub, BRICK, state, lang, text);
            let what = format!("{state} ({lang}, {text})");
            for (name, s) in [("Plus", &plus), ("Brick", &brick)] {
                for l in s.visible.iter().filter(|l| l.ends_with('…')) {
                    bad.push(format!("{what} {name}: cut \"{l}\""));
                }
                bad.extend(s.faults.iter().map(|f| format!("{what} {name}: {}", f.replace('\t', " | "))));
            }
            if lang == "en" {
                for m in must {
                    if !brick.visible.iter().any(|l| l.contains(m)) {
                        bad.push(format!("{what} Brick: no line with \"{m}\" on the picture"));
                    }
                }
            }
            let share = |s: &Shot| s.marks.get(qr).map(|(r, h)| r[3] as f32 / *h as f32);
            match (share(&plus), share(&brick)) {
                (Some(p), Some(b)) => {
                    println!("{what}: {qr} Brick {b:.3}, Plus {p:.3}");
                    if b < p - 0.05 {
                        bad.push(format!("{what}: {qr} takes {b:.3} of the Brick's height, {p:.3} of the Plus's"));
                    }
                }
                (p, b) => bad.push(format!("{what}: {qr} not marked (Plus {p:?}, Brick {b:?})")),
            }
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

/// PC streaming's address: the line under the keyboard (what an empty address does, or why pairing failed) stands on the
/// Brick's picture, above the hints; it was painted at y=800 of 768.
#[test]
fn the_pc_address_line_stands_on_the_brick_picture() {
    let (_tmp, stub, lib) = setup();
    let mut bad = Vec::new();
    for (lang, text) in CORNERS {
        for screen in [PLUS, BRICK] {
            let s = shot(&lib, &stub, screen, "pc-streaming:address", lang, text);
            let what = format!("pc-streaming:address ({lang}, {text}) {}", screen.0);
            if lang == "en" && !s.visible.iter().any(|l| l.starts_with("Leave it empty")) {
                bad.push(format!("{what}: the line under the keys is not on the picture"));
            }
            bad.extend(s.faults.iter().map(|f| format!("{what}: {}", f.replace('\t', " | "))));
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}
