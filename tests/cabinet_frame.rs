//! The Cabinet's frame stands still (the hand test 16.11.2026 on the Mini V2: it jumped around):
//! walking from shelf to shelf, the top board and the TV's place on it are the same on every shelf, at every look size;
//! only the shelves move. A handheld system's game shows on its handheld in the TV's place, the others on the TV.
use std::{fs, path::Path, process::{Child, Command}, time::Duration};

struct Compositor(Child);
impl Drop for Compositor {
    fn drop(&mut self) { let _ = self.0.kill(); let _ = self.0.wait(); }
}

fn compositor(root: &Path) -> Compositor {
    let socket = root.join("wayland");
    let child = Command::new("python3").args(["-I", concat!(env!("CARGO_MANIFEST_DIR"), "/scripts/wayland-stub.py")])
        .arg(&socket).spawn().unwrap();
    let stub = Compositor(child);
    test_clock::wait_for(|| socket.exists(), Duration::from_secs(2), "Wayland stub listening");
    stub
}

/// The painted geometry after `keys` (`cabinet_snapshot_outcome`): the line's fields by name.
fn outcome(root: &Path, keys: &str, size: &str) -> std::collections::HashMap<String, String> {
    let out = Command::new(env!("CARGO_BIN_EXE_pocket-ui"))
        .args(["--snapshot", &format!("cabinet:{keys}")]).arg(root.join("shot.png"))
        .env("WAYLAND_DISPLAY", root.join("wayland"))
        .env("POCKETUI_MOCK", "1").env("POCKETUI_LANG", "en")
        .env("POCKETUI_DEVICE", "miniv2").env("POCKETUI_TEXT_SIZE", "normal")
        .env("POCKETUI_SNAPSHOT_SIZE", size).env("POCKETUI_DATA", root.join("data"))
        .env("POCKETUI_ES_SYSTEMS", root.join("es_systems.cfg"))
        .output().unwrap();
    assert!(out.status.success(), "{size} {keys}: {}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8(out.stdout).unwrap();
    let line = stdout.lines().find(|l| l.starts_with("cabinet-outcome ")).unwrap_or_else(|| panic!("{size} {keys}: no geometry: {stdout}"));
    line.split_whitespace().filter_map(|p| p.split_once('=')).map(|(k, v)| (k.to_string(), v.to_string())).collect()
}

#[test]
fn the_top_board_and_the_tvs_place_are_the_same_on_every_shelf_and_handhelds_show_their_handheld() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().to_owned();
    fs::create_dir_all(root.join("data")).unwrap();
    fs::write(root.join("data/curator.json"), r#"{"lens":false,"stickers":false,"history":false}"#).unwrap();
    fs::write(root.join("data/settings.json"), r#"{"appearance":"dark","onboarded":true}"#).unwrap();
    // Short and tall shelves, carts, discs, boxes, the PSP's UMD and the DS's card box: (system, name, extension, handheld).
    let systems = [
        ("gb", "Game Boy", "gb", true), ("gba", "Game Boy Advance", "gba", true), ("nds", "Nintendo DS", "nds", true),
        ("nes", "Nintendo Entertainment System", "nes", false), ("ps2", "PlayStation 2", "iso", false),
        ("psp", "PlayStation Portable", "cso", true), ("psx", "PlayStation", "bin", false), ("snes", "Super Nintendo", "sfc", false),
        ("gamegear", "Game Gear", "gg", true),
    ];
    let mut xml = String::from("<systemList>");
    for (id, full, ext, _) in systems {
        let dir = root.join(id);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(format!("Fixture {id}.{ext}")), "x").unwrap();
        xml += &format!("<system><name>{id}</name><fullname>{full}</fullname><path>{}</path><extension>.{ext}</extension></system>", dir.display());
    }
    fs::write(root.join("es_systems.cfg"), xml + "</systemList>").unwrap();
    let _compositor = compositor(&root);
    // The looks' sizes: the H700's 640 x 480 (laid out at twice its size), the Brick, the Mini V2, the Smart Pro, a TV.
    for size in ["640x480", "1024x768", "1240x1080", "1280x720", "1920x1080"] {
        let mut first: Option<(String, String)> = None;
        // The systems stand in the library's order (by name); each shelf's own system says which preview it gets.
        for k in 0..systems.len() {
            let f = outcome(&root, &"d".repeat(k), size);
            let frame = (f["board"].clone(), f["place"].clone());
            assert_ne!(frame.1, "none", "{size}, shelf {k}: the TV (or handheld) is there");
            match &first {
                None => first = Some(frame),
                Some(first) => assert_eq!(&frame, first, "{size}, shelf {k}: the top board and the TV's place moved"),
            }
        }
    }
    // Which preview: a handheld for the handheld systems, the TV for the others (the Mini V2's size).
    let mut handhelds = Vec::new();
    for k in 0..systems.len() {
        let f = outcome(&root, &"d".repeat(k), "1240x1080");
        if f["handheld"] == "true" {
            handhelds.push(k);
        }
    }
    let want = systems.iter().filter(|s| s.3).count();
    assert_eq!(handhelds.len(), want, "one handheld per handheld system: shelves {handhelds:?}");
}

#[path = "../src/test_clock.rs"]
mod test_clock;
