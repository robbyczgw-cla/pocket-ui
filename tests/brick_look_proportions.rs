//! The looks on the Brick's 1024x768 in the RG35XX Plus's proportions (640x480, laid out at 1280x960; both 4:3). The
//! Brick paints its screen natively, so every size made for 960-1080 lines met 768: where a page gave up the lines it
//! lacked to 1080 (Split's panel, the switcher's card) the Brick lost 312 of them from one picture, and where it rounded
//! to whole steps it rounded the other way (the shelf's spines, the Cabinet's TV). Found on photos of the Brick
//! (0.6). Each page here prints its parts in the canvas' pixels (`split-panel`, `switcher-card`, `cabinet-outcome`,
//! `card-body`); the Brick's share of its screen is held against the Plus's.
//! Needs python3 on Linux without a compositor (scripts/wayland-stub.py), as tests/big_shelf.rs.

use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

include!("common/root.rs");

struct Stub(std::process::Child);

impl Drop for Stub {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// The Plus (the reference), the Brick, and the Smart Pro (16:9, also painted natively under 960 lines).
const PLUS: (&str, &str) = ("640x480", "rg35xx-plus");
const BRICK: (&str, &str) = ("1024x768", "brick");
const SMART_PRO: (&str, &str) = ("1280x720", "smartpro");

struct Card {
    dir: tempfile::TempDir,
    _stub: Option<Stub>,
}

impl Card {
    fn new() -> Card {
        let dir = tempfile::Builder::new().prefix("pui-prop-").tempdir().unwrap();
        let script = format!("{ROOT}/scripts/curator-fake-library.py");
        let status = Command::new("python3").env("POCKETUI_MOCK_NOW", FIXTURE_NOW).args(["-I", &script, dir.path().to_str().unwrap(), "dark"]).stdout(std::process::Stdio::null()).status().unwrap();
        assert!(status.success());
        let stub = (cfg!(target_os = "linux") && std::env::var_os("WAYLAND_DISPLAY").is_none_or(|w| w.is_empty())).then(|| {
            let child = Command::new("python3").args(["-I", &format!("{ROOT}/scripts/wayland-stub.py"), dir.path().join("wl").to_str().unwrap()]).spawn().unwrap();
            test_clock::wait_for(|| dir.path().join("wl").exists(), std::time::Duration::from_secs(2), "Wayland stub listening");
            Stub(child)
        });
        Card { dir, _stub: stub }
    }

    /// The fields of the line `what` prints in `state` on `screen`, and its canvas (w, h).
    fn line(&self, state: &str, screen: (&str, &str), what: &str) -> (HashMap<String, String>, (f32, f32)) {
        self.line_with(state, screen, what, &[])
    }

    /// `line` with more of the environment (`envs`).
    fn line_with(&self, state: &str, (size, device): (&str, &str), what: &str, envs: &[(&str, &str)]) -> (HashMap<String, String>, (f32, f32)) {
        let tmp = self.dir.path();
        let png = tmp.join(format!("{size}-{}.png", state.replace(':', "_")));
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
        if tmp.join("wl").exists() {
            cmd.env("WAYLAND_DISPLAY", tmp.join("wl"));
        }
        let out = cmd
            .args(["--snapshot", state, png.to_str().unwrap()])
            .env("POCKETUI_MOCK", "1")
            .env("POCKETUI_MOCK_NOW", FIXTURE_NOW)
            .env("POCKETUI_LANG", "en")
            .env("POCKETUI_TEXT_SIZE", "normal")
            .env("POCKETUI_DEVICE", device)
            .env("POCKETUI_HOST_BIN", "/bin/sh")
            .env("POCKETUI_SNAPSHOT_SIZE", size)
            .env("POCKETUI_DATA", tmp.join("data"))
            .env("POCKETUI_ES_SYSTEMS", tmp.join("es_systems.cfg"))
            .envs(envs.iter().copied())
            .output()
            .unwrap();
        assert!(out.status.success(), "{state} at {size}: {}", String::from_utf8_lossy(&out.stderr));
        // `PROPORTION_SHOTS=<dir>` keeps the pictures.
        if let Some(dir) = std::env::var_os("PROPORTION_SHOTS") {
            let _ = std::fs::create_dir_all(&dir);
            let _ = std::fs::copy(&png, Path::new(&dir).join(png.file_name().unwrap()));
        }
        let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
        let line = stdout.lines().find(|l| l.starts_with(&format!("{what} "))).unwrap_or_else(|| panic!("{state} at {size}: no {what} line: {stdout}"));
        let fields: HashMap<String, String> = line.split_whitespace().filter_map(|p| p.split_once('=')).map(|(k, v)| (k.to_string(), v.to_string())).collect();
        let canvas = fields.get("canvas").cloned().unwrap_or_else(|| {
            // The Cabinet's line has no canvas: the H700 panels are laid out at twice their size.
            let (w, h) = size.split_once('x').unwrap();
            let k = if size == "640x480" { 2 } else { 1 };
            format!("{}x{}", w.parse::<i32>().unwrap() * k, h.parse::<i32>().unwrap() * k)
        });
        let (cw, ch) = canvas.split_once('x').unwrap();
        (fields, (cw.parse().unwrap(), ch.parse().unwrap()))
    }
}

fn num(f: &HashMap<String, String>, k: &str) -> f32 {
    f.get(k).unwrap_or_else(|| panic!("{k} in {f:?}")).parse().unwrap_or_else(|_| panic!("{k} in {f:?}"))
}

fn rect(f: &HashMap<String, String>, k: &str) -> Option<[f32; 4]> {
    let v = f.get(k)?;
    let p: Vec<f32> = v.split(',').filter_map(|n| n.parse().ok()).collect();
    (p.len() == 4).then(|| [p[0], p[1], p[2], p[3]])
}

/// Shares within `points` (0.05: five percentage points) of the Plus's.
fn near(bad: &mut Vec<String>, what: &str, size: &str, got: f32, plus: f32, points: f32) {
    println!("{what} at {size}: {got:.3} (Plus {plus:.3})");
    if (got - plus).abs() > points {
        bad.push(format!("{what} at {size}: {got:.3} of the screen, the Plus's {plus:.3}"));
    }
}

#[test]
fn the_brick_shows_the_looks_in_the_plus_proportions() {
    let card = Card::new();
    let mut bad = Vec::new();

    // Split: the panel's height and where it starts (the list's width), the console over the count.
    let (p, (pw, ph)) = card.line("shelf:snes:split", PLUS, "split-panel");
    for screen in [BRICK, SMART_PRO] {
        let (f, (w, h)) = card.line("shelf:snes:split", screen, "split-panel");
        near(&mut bad, "Split's panel, height", screen.0, num(&f, "h") / h, num(&p, "h") / ph, 0.05);
        if screen == BRICK {
            near(&mut bad, "Split's panel, its left edge", screen.0, num(&f, "x") / w, num(&p, "x") / pw, 0.05);
        }
        let (c, _) = card.line("shelf:snes:split", screen, "split-console");
        if num(&c, "w") <= 0.0 {
            bad.push(format!("Split at {}: no console over the game count", screen.0));
        }
    }

    // The game switcher: the selected card's height.
    let (p, (_, ph)) = card.line("switcher", PLUS, "switcher-card");
    let (f, (_, h)) = card.line("switcher", BRICK, "switcher-card");
    near(&mut bad, "the switcher's card", BRICK.0, num(&f, "h") / h, num(&p, "h") / ph, 0.05);
    // Its dots stay over the hints (title, line and dots under the card keep their sizes).
    for screen in [BRICK, SMART_PRO] {
        let (f, (_, h)) = card.line("switcher", screen, "switcher-card");
        let _ = h;
        let hints = num(&f, "hints");
        // The card's area has 18 of touch margin round it; the title stands 108 under the card, the dots' bottom 104
        // under the title.
        if num(&f, "y") + num(&f, "h") - 18.0 + 108.0 + 104.0 > hints {
            bad.push(format!("the switcher at {}: the dots under the card {f:?} reach the hints (~{hints})", screen.0));
        }
    }
    // The letter of an L2/R2 jump on the Shelf look: its box in the Plus's share of the height, reaching over the spines'
    // top by about as much (audit 0.6 #17: 260 lines, a third of the Brick's, lifted over the case's "Not played yet").
    let (p, (_, ph)) = card.line("letters:gba:2", PLUS, "letter-flash");
    let (f, (_, h)) = card.line("letters:gba:2", BRICK, "letter-flash");
    near(&mut bad, "the jump's letter, its size", BRICK.0, num(&f, "size") / h, num(&p, "size") / ph, 0.05);
    let over = |f: &HashMap<String, String>, h: f32| (num(f, "spines") - num(f, "y")) / h;
    near(&mut bad, "the jump's letter, over the spines", BRICK.0, over(&f, h), over(&p, ph), 0.05);

    // The selected card's bezel (as far as its touch margin, 18) stays under the heading pill "Recent games 1 / 3",
    // whose type does not shrink with the screen (audit 0.6 #8: 960's share of the card's top put the Brick's bezel 6
    // lines into the pill), on every screen.
    for screen in [PLUS, BRICK, SMART_PRO, ("720x720", "rg35xx-plus"), ("1280x960", "rg35xx-plus")] {
        let (f, _) = card.line("switcher", screen, "switcher-card");
        if num(&f, "y") < num(&f, "pill") + 8.0 {
            bad.push(format!("the switcher at {}: the selected card's bezel {f:?} under the heading pill's bottom", screen.0));
        }
    }

    // The Cabinet's TV and the console beside it: there, near the Plus's share of the height, together as much of the
    // screen as on the Plus, and no empty band over them (photos of the Brick, 0.6: rounded down to whole steps the TV was
    // 0.185 of the height against 0.222, TV and console 70 % of the Plus's area, 0.08 more empty room over the TV).
    let (p, (pw, ph)) = card.line("cabinet:", PLUS, "cabinet-outcome");
    let (f, (w, h)) = card.line("cabinet:", BRICK, "cabinet-outcome");
    match (rect(&p, "place"), rect(&f, "place"), rect(&p, "console_place"), rect(&f, "console_place")) {
        (Some(pt), Some(bt), Some(pc), Some(bc)) => {
            near(&mut bad, "the Cabinet's TV", BRICK.0, bt[3] / h, pt[3] / ph, 0.02);
            near(&mut bad, "the Cabinet's console", BRICK.0, bc[3] / h, pc[3] / ph, 0.02);
            let area = |t: [f32; 4], c: [f32; 4], w: f32, h: f32| (t[2] * t[3] + c[2] * c[3]) / (w * h);
            let (plus, brick) = (area(pt, pc, pw, ph), area(bt, bc, w, h));
            println!("the Cabinet's TV and console at {}: {brick:.4} of the screen (Plus {plus:.4})", BRICK.0);
            if !(0.85..=1.25).contains(&(brick / plus)) {
                bad.push(format!("the Cabinet's TV and console at {}: {brick:.4} of the screen, the Plus's {plus:.4}", BRICK.0));
            }
            // The empty band over the TV: where its top stands, as a share of the height.
            near(&mut bad, "the top of the Cabinet's TV", BRICK.0, bt[1] / h, pt[1] / ph, 0.05);
        }
        (_, None, _, _) => bad.push("the Cabinet at 1024x768: no TV".into()),
        (None, _, _, _) => bad.push("the Cabinet at 640x480: no TV".into()),
        _ => bad.push(format!("the Cabinet: no console beside the TV ({p:?} / {f:?})")),
    }

    // The game card's body reaches down to the hints, as on the Plus (the label over it keeps its size).
    for screen in [BRICK, SMART_PRO] {
        let (f, _) = card.line("curator:card:snes:chrono", screen, "card-body");
        let gap = num(&f, "hints") - num(&f, "bottom");
        println!("the card's body at {}: ends {gap} over the hints", screen.0);
        if gap > 16.0 {
            bad.push(format!("the game card at {}: its body ends {gap} lines over the hints", screen.0));
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

/// Brick review 20.11.2026: in the Cabinet a shelf of DVD cases (Curator's, the Continue shelf with a PlayStation 2 game) was
/// taller than the room under the showcase on the Brick, so it faded out under the hints with no board and no name plate
/// ("Curator" missing; the Plus shows every plate); and the console of the way into a game stood 8 lines over the hint bar
/// (the Plus: 18 of 960), so a HuCard going into a PC Engine's front was cut by the bar. The selected shelf keeps its plate
/// on both, and the stage keeps the Plus's share of room over the hints.
#[test]
fn the_cabinet_keeps_each_shelfs_plate_and_its_stage_over_the_hints() {
    let card = Card::new();
    let mut bad = Vec::new();
    for keys in ["", "d", "dd", "ddd", "dddd"] {
        for screen in [PLUS, BRICK] {
            let (f, _) = card.line(&format!("cabinet:{keys}"), screen, "cabinet-outcome");
            if f.get("plate").map(String::as_str) != Some("true") {
                bad.push(format!("cabinet:{keys} at {}: the selected shelf has no name plate: {f:?}", screen.0));
            }
        }
    }
    let launch = [("POCKETUI_SNAPSHOT_LAUNCH_MS", "900"), ("POCKETUI_SHARES", "1")];
    let stage = |screen| {
        let (f, (_, h)) = card.line_with("shelf:snes:cabinet:games:2", screen, "share insert-stage", &launch);
        num(&f, "h") / h
    };
    let plus = stage(PLUS);
    let brick = stage(BRICK);
    println!("the stage's room over the hints: Brick {brick:.4}, Plus {plus:.4}");
    if brick < plus - 0.002 {
        bad.push(format!("the way into the console at {}: the stage stands {brick:.4} of the screen over the hints, the Plus's {plus:.4}", BRICK.0));
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}
