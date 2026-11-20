//! The Brick's pages keep the RG35XX Plus's proportions (20.11.2026, photos of the Brick: "the UI and the proportions of our
//! most important platform"). Both are 4:3, but the Plus's 640x480 is laid out at 1280x960 and the Brick's 1024x768 is
//! painted natively, with the chrome made for 960 and 1080 lines: pictures that gave up the missing lines shrank or went,
//! and no test saw it, because a picture that shrinks never overlaps anything.
//!
//! Each page marks its main picture (`pocket_ui::shares`, printed with `POCKETUI_SHARES`); its share of the canvas on the
//! Brick must be the Plus's within five points, never nothing, and the Smart Pro's 1280x720 (natively painted, 16:9)
//! keeps a useful share too. tests/brick_look_proportions.rs has the shelf's looks (Split, the switcher, the Cabinet).
//! Needs python3 on Linux without a compositor (scripts/wayland-stub.py), as tests/split_panel.rs.

#![allow(dead_code)]

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

include!("common/root.rs");
include!("common/shelf_library.rs");

/// The Plus (the reference), the Brick, the Smart Pro.
const PLUS: (&str, &str) = ("640x480", "rg35xx-plus");
const BRICK: (&str, &str) = ("1024x768", "brick");
const SMART_PRO: (&str, &str) = ("1280x720", "smartpro");

/// A mark's rectangle (x, y, w, h) and the canvas it was painted on (w, h).
#[derive(Clone, Copy, Debug)]
struct Mark {
    r: [i32; 4],
    canvas: (i32, i32),
}

impl Mark {
    fn h_share(&self) -> f32 {
        self.r[3] as f32 / self.canvas.1 as f32
    }
    fn w_share(&self) -> f32 {
        self.r[2] as f32 / self.canvas.0 as f32
    }
    fn bottom(&self) -> i32 {
        self.r[1] + self.r[3]
    }
}

/// `--snapshot <state>` at `screen` with `POCKETUI_SHARES`: the marks by name.
fn marks(lib: &Path, stub: &Option<Stub>, (size, device): (&str, &str), state: &str, look: &str) -> HashMap<String, Mark> {
    let png = lib.join(format!("{size}-{}-{look}.png", state.replace(':', "_")));
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
    if let Some(s) = stub {
        cmd.env("WAYLAND_DISPLAY", &s.1);
    }
    if look == "retro" {
        cmd.env("POCKETUI_RETRO", "cocoa");
    }
    let out = cmd
        .args(["--snapshot", state, png.to_str().unwrap()])
        .env("POCKETUI_DEVICE", device)
        .env("POCKETUI_SHARES", "1")
        .env("POCKETUI_MOCK", "1")
        .env("POCKETUI_MOCK_NOW", FIXTURE_NOW)
        .env("POCKETUI_LANG", "en")
        .env("POCKETUI_TEXT_SIZE", "normal")
        .env("POCKETUI_SNAPSHOT_SIZE", size)
        .env("POCKETUI_ES_SYSTEMS", lib.join("es_systems.cfg"))
        .env("POCKETUI_DATA", lib.join("data"))
        .output()
        .expect("run pocket-ui");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(out.status.success() && png.exists(), "{state} at {size}: {}", String::from_utf8_lossy(&out.stderr));
    // `BRICK_SHOTS=<dir>` keeps the pictures.
    if let Some(dir) = std::env::var_os("BRICK_SHOTS") {
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::copy(&png, Path::new(&dir).join(png.file_name().unwrap()));
    }
    let _ = std::fs::remove_file(&png);
    stdout
        .lines()
        .filter_map(|l| l.strip_prefix("share "))
        .map(|l| {
            let mut it = l.split_whitespace();
            let name = it.next().unwrap().to_string();
            let f: HashMap<&str, &str> = it.filter_map(|p| p.split_once('=')).collect();
            let n = |k: &str| f[k].parse::<i32>().unwrap();
            let (cw, ch) = f["canvas"].split_once('x').unwrap();
            (name, Mark { r: [n("x"), n("y"), n("w"), n("h")], canvas: (cw.parse().unwrap(), ch.parse().unwrap()) })
        })
        .collect()
}

fn mark<'a>(m: &'a HashMap<String, Mark>, name: &str, what: &str) -> &'a Mark {
    m.get(name).unwrap_or_else(|| panic!("{what}: no {name} painted (marks: {:?})", m.keys().collect::<Vec<_>>()))
}

/// The page's `name` at the Plus, the Brick and the Smart Pro in Barlow and Retro: painted on each, its height (and,
/// with `width`, its width) within five points of the Plus's share on the Brick and no more than `pro_less` under it on
/// the Smart Pro. Returns the problems.
fn same_share(lib: &Path, stub: &Option<Stub>, state: &str, name: &str, width: bool, pro_less: f32) -> Vec<String> {
    let mut bad = Vec::new();
    for look in ["barlow", "retro"] {
        let plus = *mark(&marks(lib, stub, PLUS, state, look), name, &format!("{state} {look} Plus"));
        for (screen, less) in [(BRICK, 0.05), (SMART_PRO, pro_less)] {
            let m = marks(lib, stub, screen, state, look);
            let what = format!("{state} {look} {}", screen.0);
            let Some(s) = m.get(name) else {
                bad.push(format!("{what}: {name} not painted (the Plus paints it)"));
                continue;
            };
            println!("{what}: {name} h {:.3} w {:.3} (Plus h {:.3} w {:.3})", s.h_share(), s.w_share(), plus.h_share(), plus.w_share());
            if s.r[2] <= 0 || s.r[3] <= 0 {
                bad.push(format!("{what}: {name} is {}x{}", s.r[2], s.r[3]));
            }
            if s.h_share() < plus.h_share() - less {
                bad.push(format!("{what}: {name} is {:.3} of the height, the Plus's {:.3}", s.h_share(), plus.h_share()));
            }
            if width && s.w_share() < plus.w_share() - less {
                bad.push(format!("{what}: {name} is {:.3} of the width, the Plus's {:.3}", s.w_share(), plus.w_share()));
            }
        }
    }
    bad
}

fn setup() -> (tempfile::TempDir, Option<Stub>, PathBuf) {
    let tmp = tempfile::Builder::new().prefix("pui-brick-").tempdir_in("/tmp").unwrap();
    let stub = wayland(tmp.path());
    let lib = library_in(&tmp.path().join("lib"), None, "shelf");
    (tmp, stub, lib)
}

/// Curator → History: the chart was 54 px on the Brick (0.070 of the height, the Plus 0.121) and its weekday labels ran
/// into "Most played" and "Systems" under it. The chart keeps the Plus's share, the labels stay clear of the words under
/// them, and the five rows still end over the hints.
#[test]
fn history_chart_keeps_the_plus_share_and_its_labels_clear() {
    let (_tmp, stub, lib) = setup();
    let mut bad = same_share(&lib, &stub, "curator:history", "history-chart", false, 0.05);
    for look in ["barlow", "retro"] {
        for screen in [PLUS, BRICK, SMART_PRO] {
            let m = marks(&lib, &stub, screen, "curator:history", look);
            let what = format!("curator:history {look} {}", screen.0);
            let (days, below, row) = (mark(&m, "history-days", &what), mark(&m, "history-below", &what), mark(&m, "history-row", &what));
            let hints = mark(&m, "hints", &what);
            if below.r[1] < days.bottom() + 4 {
                bad.push(format!("{what}: the weekdays end at {}, \"Most played\" starts at {}", days.bottom(), below.r[1]));
            }
            // The last row ends over the hint bar, with a breath between.
            if row.bottom() > hints.r[1] - 12 {
                bad.push(format!("{what}: the fifth row ends at {}, the hints start at {}", row.bottom(), hints.r[1]));
            }
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

/// The pictures that were right on the Brick stay so: the pause menu's preview of a save, RomM's pairing QR code (one of
/// its letters' wraps beside it shrinks it, down to 200 px) and Pocket Sync's six code boxes.
#[test]
fn the_save_preview_the_romm_code_and_the_pocket_code_keep_the_plus_share() {
    let (_tmp, stub, lib) = setup();
    let mut bad = same_share(&lib, &stub, "slot-load", "pause-preview", true, 0.05);
    bad.extend(same_share(&lib, &stub, "romm:code", "romm-qr", false, 0.05));
    bad.extend(same_share(&lib, &stub, "pockets:code", "pocket-code", false, 0.05));
    // Beside the RomM code its page's address stays whole (it is typed from there): the code grew with the header's
    // lines on the Brick (0.6.1) and cut it to "…/pair/de…".
    for look in ["barlow", "retro"] {
        for screen in [PLUS, BRICK, SMART_PRO] {
            if !marks(&lib, &stub, screen, "romm:code", look).contains_key("romm-page") {
                bad.push(format!("romm:code {look} {}: the page's address is cut", screen.0));
            }
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

/// The frame round the pages (0.6.1): the header (the Settings family's band with the status pill, Home's and its
/// looks' top line, the pause menu's title) and the hint bar under them. The Brick's 1024x768 is painted natively, and
/// with the frame's pixels made for 960 lines it took 25 % of the height (120 + 74 lines) against the Plus's 20 %; the
/// pictures between lost it. On the Brick each part takes the Plus's share of the height within three points; the
/// Smart Pro's 1280x720 (16:9, its own layouts) keeps the design's pixels, as on the Plus's 1280x960 canvas.
#[test]
fn the_header_and_the_hint_bar_take_the_plus_share_on_the_brick() {
    let (_tmp, stub, lib) = setup();
    let mut bad = Vec::new();
    for state in ["settings", "home", "menu:pocket-core", "big-shelf", "curator:home", "shelf:psx:split"] {
        for look in ["barlow", "retro"] {
            let (plus, brick, pro) = (marks(&lib, &stub, PLUS, state, look), marks(&lib, &stub, BRICK, state, look), marks(&lib, &stub, SMART_PRO, state, look));
            let mut frame = [0.0f32; 2];
            for name in ["header", "hints"] {
                let what = format!("{state} {look}: {name}");
                let (p, b, s) = (mark(&plus, name, &what), mark(&brick, name, &what), mark(&pro, name, &what));
                println!("{what}: Brick {:.3} ({} lines), Plus {:.3}, Smart Pro {} lines", b.h_share(), b.r[3], p.h_share(), s.r[3]);
                if (b.h_share() - p.h_share()).abs() > 0.03 {
                    bad.push(format!("{what} takes {:.3} of the Brick's height, {:.3} of the Plus's", b.h_share(), p.h_share()));
                }
                if s.r[3] != p.r[3] {
                    bad.push(format!("{what} is {} lines on the Smart Pro, {} on the Plus's canvas (the design's)", s.r[3], p.r[3]));
                }
                frame[0] += b.h_share();
                frame[1] += p.h_share();
            }
            if frame[0] > frame[1] + 0.03 {
                bad.push(format!("{state} {look}: header and hints take {:.3} of the Brick's height, {:.3} of the Plus's", frame[0], frame[1]));
            }
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}
