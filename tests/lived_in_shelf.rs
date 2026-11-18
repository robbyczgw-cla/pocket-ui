//! Settings → Look → Lived-in shelf (0.5.2, `pocket_ui::lived_in`, `app_cabinet::shelf_row`): the Shelf look's rows of
//! games with some cases leaning and a few on a small stack. Off (the default) the shelf is painted as before, byte for
//! byte; on, the same games always stand the same way; a row that fits the shelf standing straight never has to scroll
//! for its stacks (a mixed Favourites row at 1280x720 did in the prototype); and the leaning and lying spines are made
//! ahead after the row opens (`App::lived_warm_tick`), so a walk along the row turns none of them in a frame (the
//! prototype's first frame on a tile of them: 5-6 ms on a desktop core, estimated 40-55 ms on the Brick).
//!
//! On a made-up library written here: 40 Super Nintendo games and a Favourites row of 27 games of seven systems (SNES and
//! N64 boxes, PS1 jewel cases, GBA and DS boxes, Mega Drive and GameCube cases). Needs python3 on Linux without a
//! compositor (scripts/wayland-stub.py), as tests/shelf_end.rs.

use std::path::{Path, PathBuf};
use std::process::Command;

include!("common/root.rs");

/// The page's side margin (`palette::MARGIN`).
const MARGIN: i32 = 72;

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

/// The mixed row: (system, full name, extension, games).
const MIXED: &[(&str, &str, &str, &[&str])] = &[
    ("n64", "Nintendo 64", ".z64", &["Super Mario 64 (USA)", "Legend of Zelda, The - Ocarina of Time (USA)", "Mario Kart 64 (USA)", "Banjo-Kazooie (USA)"]),
    ("gba", "Game Boy Advance", ".gba", &["Metroid Fusion (USA)", "Golden Sun (USA, Europe)", "Advance Wars (USA)", "Mario Kart - Super Circuit (USA)", "Wario Land 4 (USA, Europe)"]),
    ("psx", "PlayStation", ".chd", &["Final Fantasy VII (USA)", "Crash Bandicoot (USA)", "Metal Gear Solid (USA)", "Spyro the Dragon (USA)"]),
    ("megadrive", "Mega Drive", ".md", &["Sonic the Hedgehog 2 (World)", "Streets of Rage 2 (USA)", "Gunstar Heroes (USA)"]),
    ("gc", "GameCube", ".iso", &["Metroid Prime (USA)", "Legend of Zelda, The - The Wind Waker (USA)", "Super Smash Bros. Melee (USA)", "Pikmin (USA)"]),
    ("nds", "Nintendo DS", ".nds", &["Mario Kart DS (USA)", "New Super Mario Bros. (USA)", "Professor Layton and the Curious Village (USA)"]),
];

/// The SNES games that are favourites too (the rest of the mixed row).
const SNES_FAVOURITES: [&str; 4] = ["Super Metroid (Japan, USA) (En,Ja)", "Chrono Trigger (USA)", "Donkey Kong Country (USA) (Rev 2)", "EarthBound (USA)"];

/// The library: roms, es_systems.cfg and data/ (favorites.json; settings.json with the Shelf look and, when given,
/// `lived_in`).
fn library(dir: &Path, lived: Option<bool>) -> PathBuf {
    let (roms, data) = (dir.join("roms"), dir.join("data"));
    let mut cfg = vec!["<systemList>".to_string()];
    let mut favs = Vec::new();
    let mut system = |id: &str, full: &str, ext: &str, games: &[String]| {
        let d = roms.join(id);
        std::fs::create_dir_all(&d).unwrap();
        cfg.push(format!("  <system><name>{id}</name><fullname>{full}</fullname><path>{}</path><extension>{ext}</extension></system>", d.display()));
        for g in games {
            let path = d.join(format!("{g}{ext}"));
            std::fs::write(&path, b"\0").unwrap();
            if id != "snes" || SNES_FAVOURITES.contains(&g.as_str()) {
                favs.push(serde_json::json!({"system": id, "name": g, "path": path}));
            }
        }
    };
    let mut snes: Vec<String> = SNES_FAVOURITES.iter().map(|s| s.to_string()).collect();
    snes.extend((0..36).map(|i| format!("Made-up Quest {i:02} (USA)")));
    system("snes", "Super Nintendo", ".sfc", &snes);
    for (id, full, ext, games) in MIXED {
        system(id, full, ext, &games.iter().map(|g| g.to_string()).collect::<Vec<_>>());
    }
    cfg.push("</systemList>".into());
    std::fs::create_dir_all(&data).unwrap();
    std::fs::write(dir.join("es_systems.cfg"), cfg.join("\n") + "\n").unwrap();
    favs.sort_by_key(|f| f["name"].as_str().unwrap().to_lowercase());
    std::fs::write(data.join("favorites.json"), serde_json::Value::Array(favs).to_string()).unwrap();
    let mut settings = serde_json::json!({"onboarded": true, "theme": "shelf"});
    if let Some(on) = lived {
        settings["lived_in"] = serde_json::json!(on);
    }
    std::fs::write(data.join("settings.json"), settings.to_string()).unwrap();
    dir.to_path_buf()
}

/// pocket-ui `--snapshot <state>` at `size` on the library made for `lived` (in a folder of its own): its stdout and the
/// picture (width, height, RGB).
fn run(tmp: &Path, stub: &Option<Stub>, lived: Option<bool>, size: &str, state: &str) -> (String, (i32, i32, Vec<u8>)) {
    let setting = match lived {
        None => "unset",
        Some(false) => "off",
        Some(true) => "on",
    };
    let name = format!("{setting}-{size}-{}", state.replace(':', "_"));
    let lib = library(&tmp.join(format!("lib-{name}")), lived);
    let png = tmp.join(format!("{name}.png"));
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
    if let Some(s) = stub {
        cmd.env("WAYLAND_DISPLAY", &s.1);
    }
    let out = cmd
        .args(["--snapshot", state, png.to_str().unwrap()])
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
    assert!(out.status.success(), "{state} at {size}: {}", String::from_utf8_lossy(&out.stderr));
    if !png.exists() {
        return (stdout, (0, 0, Vec::new()));
    }
    // `LIVED_SHOTS=<dir>` keeps the pictures.
    if let Some(dir) = std::env::var_os("LIVED_SHOTS") {
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::copy(&png, Path::new(&dir).join(format!("{name}.png")));
    }
    let decoder = png::Decoder::new(std::fs::File::open(&png).unwrap());
    let mut reader = decoder.read_info().unwrap();
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).unwrap();
    assert_eq!(info.color_type, png::ColorType::Rgb);
    (stdout, (info.width as i32, info.height as i32, buf[..info.buffer_size()].to_vec()))
}

/// The shelf's left and right end and a line just above the board (as tests/shelf_end.rs measures it).
fn shelf_line(w: i32, h: i32) -> (i32, i32, i32) {
    let top = if h < 600 { 80 } else { 104 };
    let bottom = h - 128;
    let f = (((h - 112 - top).max(200) as f32 * 0.42 / 120.0 * 2.0).floor() / 2.0).clamp(1.0, 4.0);
    let floor = bottom - (8.0 * f).round() as i32;
    let col = if w >= 1600 { (w * 3 / 4).max(1440).min(w) } else { w };
    let x0 = (w - col) / 2;
    (x0 + MARGIN, x0 + col - MARGIN, floor - 3)
}

/// The first and last pixel along line `y` that differ from the page.
fn ends((w, _, px): &(i32, i32, Vec<u8>), y: i32) -> (i32, i32) {
    let at = |x: i32| {
        let i = ((y * w + x) * 3) as usize;
        (px[i], px[i + 1], px[i + 2])
    };
    let bg = at(2);
    let painted: Vec<i32> = (0..*w).filter(|&x| at(x) != bg).collect();
    (painted.first().copied().unwrap_or(i32::MAX), painted.last().copied().unwrap_or(0))
}

/// Off is the shelf as before: settings.json without the key and with `"lived_in": false` paint the same bytes (Home
/// and a system's row; Home stays tidy when it is on, too). On, a system's row is posed, and the same games stand the
/// same way at every start.
#[test]
fn off_is_the_shelf_as_before_and_on_stands_the_same_way_every_time() {
    let _guard = tempfile::tempdir().unwrap();
    let tmp = _guard.path().to_path_buf();
    let stub = wayland(&tmp);
    for size in ["640x480", "1280x720", "1024x768"] {
        for state in ["shelf:snes:shelf:games:12", "shelf:snes:shelf:games:39", "shelf:favorites:shelf:games:3"] {
            let (_, before) = run(&tmp, &stub, None, size, state);
            let (_, off) = run(&tmp, &stub, Some(false), size, state);
            assert!(before.2 == off.2, "{state} at {size}: Off must paint the shelf as before");
            let (_, on) = run(&tmp, &stub, Some(true), size, state);
            let (_, again) = run(&tmp, &stub, Some(true), size, state);
            assert!(on.2 == again.2, "{state} at {size}: the same games must stand the same way at every start");
            assert!(on.2 != off.2, "{state} at {size}: On must pose the row");
        }
        // Home stays tidy: the systems' row is not a row of games.
        let (_, off) = run(&tmp, &stub, Some(false), size, "shelf:snes:shelf");
        let (_, on) = run(&tmp, &stub, Some(true), size, "shelf:snes:shelf");
        assert!(on.2 == off.2, "Home at {size}: Lived-in shelf leaves Home as it is");
    }
    drop(stub);
}

/// A mixed Favourites row that fits the shelf standing straight (with the props after its end) at 1280x720 stays whole on
/// the shelf with Lived-in shelf on: its stacks went, not its place (the prototype's mixed row had to scroll there, its
/// first case cut at the left end). With a case in the middle and the last one selected, the shelf's left end shows the
/// same pixels (the row did not move), and the row ends before the shelf's right end.
#[test]
fn a_row_that_fits_standing_never_scrolls_for_its_stacks() {
    let _guard = tempfile::tempdir().unwrap();
    let tmp = _guard.path().to_path_buf();
    let stub = wayland(&tmp);
    let mut wrong = Vec::new();
    for lived in [Some(false), Some(true)] {
        let size = "1280x720";
        let (_, mid) = run(&tmp, &stub, lived, size, "shelf:favorites:shelf:games:13");
        let (_, last) = run(&tmp, &stub, lived, size, "shelf:favorites:shelf:games:26");
        let (left, right, y) = shelf_line(mid.0, mid.1);
        // The shelf's first 160 pixels, from 80 lines above the line down to it.
        let band = |(w, _, px): &(i32, i32, Vec<u8>)| -> Vec<u8> {
            (y - 80..=y).flat_map(|yy| px[((yy * w + left) * 3) as usize..((yy * w + left + 160) * 3) as usize].to_vec()).collect()
        };
        let end = ends(&last, y).1;
        let what = format!("{size}, lived_in {lived:?}: the row ends at {end} with the last case selected, the shelf at {right}");
        println!("{what}");
        if band(&mid) != band(&last) {
            wrong.push(format!("{what}: the row scrolled"));
        }
        if end > right - 16 {
            wrong.push(format!("{what}: no room left after the row"));
        }
    }
    drop(stub);
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// The value of `key=` in the line of `out` starting with `start`.
fn value(out: &str, start: &str, key: &str) -> u64 {
    let line = out.lines().find(|l| l.starts_with(start)).unwrap_or_else(|| panic!("no {start} line in:\n{out}"));
    let v = line.split_whitespace().find_map(|w| w.strip_prefix(&format!("{key}="))).unwrap_or_else(|| panic!("no {key} in {line}"));
    v.parse().unwrap()
}

/// Opening the row turns the leaning and lying spines in view; the rest are made ahead in the idle moments after
/// (`lived_warm_tick`, here all at once as `walk-warm` does), so a calm walk to the row's end turns none in a frame.
/// Without the warming the walk turns them as it reaches them (the stutter on the Brick). Off, nothing is turned.
#[test]
fn the_rows_turned_spines_are_made_ahead_not_while_walking() {
    let _guard = tempfile::tempdir().unwrap();
    let tmp = _guard.path().to_path_buf();
    let stub = wayland(&tmp);
    let (cold, _) = run(&tmp, &stub, Some(true), "640x480", "bench-playful:snes:walk");
    assert!(value(&cold, "lived walk", "posed_in_walk") > 0, "without the warming the walk turns spines:\n{cold}");
    let (warm, _) = run(&tmp, &stub, Some(true), "640x480", "bench-playful:snes:walk-warm");
    assert!(value(&warm, "lived warm", "posed") > 0, "{warm}");
    assert_eq!(value(&warm, "lived walk", "posed_in_walk"), 0, "made ahead, the walk turns none:\n{warm}");
    let (off, _) = run(&tmp, &stub, Some(false), "640x480", "bench-playful:snes:walk");
    assert_eq!(value(&off, "lived walk", "posed_in_walk"), 0, "{off}");
    drop(stub);
}
