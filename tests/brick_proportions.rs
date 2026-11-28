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
fn marks(lib: &Path, stub: &Option<Stub>, screen: (&str, &str), state: &str, look: &str) -> HashMap<String, Mark> {
    marks_with(lib, stub, screen, state, look, None)
}

/// `marks` with a picture of the paused game behind the pause menu (`--snapshot <state> <png> <frame>`).
fn marks_with(lib: &Path, stub: &Option<Stub>, (size, device): (&str, &str), state: &str, look: &str, frame: Option<&Path>) -> HashMap<String, Mark> {
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
        .args(frame.map(|f| f.to_str().unwrap()))
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
    same_share_frame(lib, stub, state, name, width, pro_less, None)
}

/// `same_share` for the pause menu over a picture of the game (`marks_with`), height and width, the Smart Pro within five.
fn same_share_with(lib: &Path, stub: &Option<Stub>, state: &str, name: &str, frame: Option<&Path>) -> Vec<String> {
    same_share_frame(lib, stub, state, name, true, 0.05, frame)
}

fn same_share_frame(lib: &Path, stub: &Option<Stub>, state: &str, name: &str, width: bool, pro_less: f32, frame: Option<&Path>) -> Vec<String> {
    let mut bad = Vec::new();
    for look in ["barlow", "retro"] {
        let plus = *mark(&marks_with(lib, stub, PLUS, state, look, frame), name, &format!("{state} {look} Plus"));
        for (screen, less) in [(BRICK, 0.05), (SMART_PRO, pro_less)] {
            let m = marks_with(lib, stub, screen, state, look, frame);
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
    let tmp = tempfile::Builder::new().prefix("pui-brick-").tempdir().unwrap();
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

/// What `--snapshot <state>` at `screen` paints, string by string (`POCKETUI_VISIBLE_TEXT`; a wrapped line is one string
/// per line).
fn visible(lib: &Path, stub: &Option<Stub>, (size, device): (&str, &str), state: &str, look: &str) -> Vec<String> {
    let png = lib.join(format!("vis-{size}-{}-{look}.png", state.replace(':', "_")));
    let txt = png.with_extension("txt");
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
        .env("POCKETUI_VISIBLE_TEXT", &txt)
        .env("POCKETUI_MOCK", "1")
        .env("POCKETUI_MOCK_NOW", FIXTURE_NOW)
        .env("POCKETUI_LANG", "en")
        .env("POCKETUI_TEXT_SIZE", "normal")
        .env("POCKETUI_SNAPSHOT_SIZE", size)
        .env("POCKETUI_ES_SYSTEMS", lib.join("es_systems.cfg"))
        .env("POCKETUI_DATA", lib.join("data"))
        .output()
        .expect("run pocket-ui");
    assert!(out.status.success() && png.exists(), "{state} at {size}: {}", String::from_utf8_lossy(&out.stderr));
    let _ = std::fs::remove_file(&png);
    let text = std::fs::read_to_string(&txt).unwrap_or_default();
    let _ = std::fs::remove_file(&txt);
    text.lines().map(str::to_string).collect()
}

/// Neon polish 21.11.2026: the Brick's first start, in English at Normal text, cut the Shelf's line to "The one you pick
/// shows its…" and showed none of the three lines under the rows (a note on two lines beside its pills took the one line
/// kept for them; the Brick's Normal is a few pixels larger, `type_scale::brick_base`). The unit tests paint without the
/// Brick's sizes, the share tests measure pictures, not words. On the Plus, the Brick and the Smart Pro, in Barlow and
/// Retro: the look's line whole and where the look changes later; what MENU (HOME) does in a game on the last page.
#[test]
fn the_first_start_says_all_of_it_on_the_brick() {
    let (_tmp, stub, lib) = setup();
    let mut bad = Vec::new();
    for screen in [PLUS, BRICK, SMART_PRO] {
        for look in ["barlow", "retro"] {
            let seen = visible(&lib, &stub, screen, "setup:first-look", look);
            let what = format!("{} {look}", screen.0);
            let blurb: Vec<&String> = seen.iter().filter(|l| l.starts_with("Games lined up on a shelf")).collect();
            if blurb.len() != 1 || blurb[0].contains('…') {
                bad.push(format!("{what}: the Shelf's line {blurb:?}"));
            }
            let all = seen.join(" ");
            if !all.contains("Change the look any time in Settings › Look.") {
                bad.push(format!("{what}: missing where the look changes later (painted: {seen:?})"));
            }
            // What MENU (HOME) does in a game: on the last page, once (26.11.: the look said it too).
            let done = visible(&lib, &stub, screen, "setup:first-done", look).join(" ");
            if !done.contains("opens the menu: save, load, quit.") || all.contains("opens the menu: save, load, quit.") {
                bad.push(format!("{what}: HOME's line not once on the last page (last page: {done}; look: {all})"));
            }
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

/// Neon polish 21.11.2026: Pocket Sync on the Brick said "Keep saves the same on your other Bricks", "Pair a Brick",
/// "Which Bricks: Bricks with pocket-ui OS…", "A paired Brick is listed by its model", though it pairs any handheld with
/// pocket-ui; "Pocket" there is any of them. A line wrapped or cut short is painted piece by piece, and each piece went
/// through the Brick's words again: the headings were right and the paragraphs under them still said "Bricks". On the
/// Brick and the Smart Pro, in Barlow and Retro, every Pocket Sync page and its row in Settings: no "Brick" but the
/// TrimUI Brick's name, no "Smart Pro" at all.
#[test]
fn pocket_sync_names_no_brick_for_the_other_handhelds() {
    let (_tmp, stub, lib) = setup();
    let mut bad = Vec::new();
    let states = ["pockets:on", "pockets:off", "pockets:none", "pockets:what", "pockets:pair", "pockets:nobody", "pockets:code", "settings:0:5"];
    for (screen, word) in [(BRICK, "Brick"), (SMART_PRO, "Smart Pro")] {
        for look in ["barlow", "retro"] {
            let mut sync_row = false;
            for state in states {
                let seen = visible(&lib, &stub, screen, state, look);
                sync_row |= state.starts_with("settings") && seen.join(" ").contains("other Pockets, without a computer.");
                for line in &seen {
                    let mut rest = line.as_str();
                    while let Some(at) = rest.find(word) {
                        if !rest[..at].ends_with("TrimUI ") {
                            bad.push(format!("{} {look} {state}: {line}", screen.0));
                            break;
                        }
                        rest = &rest[at + word.len()..];
                    }
                }
            }
            if !sync_row {
                bad.push(format!("{} {look}: settings:0:5 shows no Pocket Sync line (the row moved?)", screen.0));
            }
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

/// A made-up picture of a game (240x160, the GBA's), for the pause menu's background (`marks_with`).
fn game_frame(dir: &Path) -> PathBuf {
    let path = dir.join("frame.png");
    let (w, h) = (240u32, 160u32);
    let mut px = Vec::with_capacity((w * h * 3) as usize);
    for y in 0..h {
        for x in 0..w {
            px.extend([(x * 255 / w) as u8, (y * 255 / h) as u8, 160]);
        }
    }
    let mut enc = png::Encoder::new(std::fs::File::create(&path).unwrap(), w, h);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().unwrap().write_image_data(&px).unwrap();
    path
}

/// Neon polish 21.11.2026, the lead's choice A: on Continue, Options and Save & quit the pause menu's right two thirds
/// stood empty (dark); they now show the paused game in Save's window. With a picture of the game behind the menu, the
/// window (`pause-preview`) is there on each of the three rows, where Save's is (the same rectangle), on the Plus, the
/// Brick and the Smart Pro in Barlow and Retro, and on the Brick within five points of the Plus's share.
#[test]
fn the_pause_menu_shows_the_paused_game_on_continue_options_and_quit() {
    let (tmp, stub, lib) = setup();
    let frame = game_frame(tmp.path());
    let mut bad = Vec::new();
    for look in ["barlow", "retro"] {
        for screen in [PLUS, BRICK, SMART_PRO] {
            let at = |state: &str| marks_with(&lib, &stub, screen, state, look, Some(&frame)).get("pause-preview").map(|m| m.r);
            let save = at("menu:pocket-core:1");
            // Continue (row 0), Options (3), Save & quit (4) of pocket-core's menu (Continue, Save, Load, Options, Save & quit).
            for (state, row) in [("menu:pocket-core", "Continue"), ("menu:pocket-core:3", "Options"), ("menu:pocket-core:4", "Save & quit")] {
                let what = format!("{look} {} {row}", screen.0);
                match at(state) {
                    None => bad.push(format!("{what}: no window with the game")),
                    Some(r) if Some(r) != save => bad.push(format!("{what}: window at {r:?}, Save's at {save:?}")),
                    Some(_) => {}
                }
            }
        }
    }
    bad.extend(same_share_with(&lib, &stub, "menu:pocket-core", "pause-preview", Some(&frame)));
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}
