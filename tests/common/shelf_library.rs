// A made-up library for the Shelf look's tests (tests/lived_in_shelf.rs, tests/scene_rests.rs): 40 Super Nintendo games
// and a Favourites row of 27 games of seven systems, and pocket-ui run on it. `include!` after common/root.rs, with
// `use std::path::{Path, PathBuf}` and `use std::process::Command`.

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
    library_in(dir, lived, "shelf")
}

/// [`library`] with the look `theme` (shelf, cabinet, split, big).
#[allow(dead_code)]
fn library_in(dir: &Path, lived: Option<bool>, theme: &str) -> PathBuf {
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
    // Five games played lately (Home's recent row; open cases with `POCKETUI_SNAPSHOT_FROZEN=all`).
    let recents: Vec<serde_json::Value> = favs
        .iter()
        .step_by(5)
        .enumerate()
        .map(|(i, f)| {
            let mut r = f.clone();
            r["last_played"] = serde_json::json!(1_794_139_200 - 3600 * (2 + 20 * i as i64));
            r["play_secs"] = serde_json::json!(3600 * (2 + i as i64));
            r
        })
        .collect();
    std::fs::write(data.join("recents.json"), serde_json::Value::Array(recents).to_string()).unwrap();
    std::fs::write(data.join("favorites.json"), serde_json::Value::Array(favs).to_string()).unwrap();
    let mut settings = serde_json::json!({"onboarded": true, "theme": theme});
    if let Some(on) = lived {
        settings["lived_in"] = serde_json::json!(on);
    }
    std::fs::write(data.join("settings.json"), settings.to_string()).unwrap();
    dir.to_path_buf()
}

/// pocket-ui `--snapshot <state>` at `size` on the library made for `lived` (in a folder of its own): its stdout and the
/// picture (width, height, RGB).
fn run(tmp: &Path, stub: &Option<Stub>, lived: Option<bool>, size: &str, state: &str) -> (String, (i32, i32, Vec<u8>)) {
    run_in(tmp, stub, lived, "shelf", size, state)
}

/// [`run`] in the look `theme`.
#[allow(dead_code)]
fn run_in(tmp: &Path, stub: &Option<Stub>, lived: Option<bool>, theme: &str, size: &str, state: &str) -> (String, (i32, i32, Vec<u8>)) {
    let setting = match lived {
        None => "unset",
        Some(false) => "off",
        Some(true) => "on",
    };
    let name = format!("{theme}-{setting}-{size}-{}", state.replace(':', "_"));
    let lib = library_in(&tmp.join(format!("lib-{name}")), lived, theme);
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
