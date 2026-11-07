//! Five systems across the real panel sizes: a player can walk to every shelf, open it, and get back to the first,
//! and always sees which shelf is selected. (No decoration coordinates: those move with the design.)
use std::{fs, path::PathBuf, process::{Child, Command}, time::Duration};

struct Stub(Child);
impl Drop for Stub {
    fn drop(&mut self) { let _ = self.0.kill(); let _ = self.0.wait(); }
}

/// Share of pixels that differ. Clock-like corners may flicker between two paints; a moved selection changes far more.
fn differing(a: &[u8], b: &[u8]) -> f64 {
    assert_eq!(a.len(), b.len());
    a.iter().zip(b).filter(|(x, y)| x != y).count() as f64 / a.len() as f64
}
const SAME: f64 = 0.005;

#[test]
fn every_shelf_can_be_reached_opened_and_left_at_every_panel_size() {
    let tmp = tempfile::tempdir().unwrap();
    // An optional persistent directory keeps the same fixture and screenshots for visual review.
    let root = std::env::var_os("POCKETUI_CABINET_EVIDENCE").map(PathBuf::from).unwrap_or_else(|| tmp.path().into());
    fs::create_dir_all(&root).unwrap();
    let mut xml = String::from("<systemList>");
    for (id, name, ext) in [
        ("gba", "Game Boy Advance", "gba"), ("gbc", "Game Boy Color", "gbc"),
        ("nes", "Nintendo Entertainment System", "nes"), ("psx", "PlayStation", "bin"),
        ("snes", "Super Nintendo", "sfc"),
    ] {
        let dir = root.join(id);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(format!("Fixture {id}.{ext}")), "").unwrap();
        xml.push_str(&format!("<system><name>{id}</name><fullname>{name}</fullname><path>{}</path><extension>.{ext} .cue .m3u</extension></system>", dir.display()));
    }
    xml.push_str("</systemList>");
    fs::write(root.join("es_systems.cfg"), xml).unwrap();
    let socket = tmp.path().join("wayland");
    let _stub = if cfg!(target_os = "linux") {
        let child = Command::new("python3").args(["-I", &format!("{}/scripts/wayland-stub.py", std::env::var("CARGO_MANIFEST_DIR").unwrap())]).arg(&socket).spawn().unwrap();
        let stub = Stub(child);
        test_clock::wait_for(|| socket.exists(), Duration::from_secs(2), "Wayland stub listening");
        Some(stub)
    } else { None };
    // Match a simple card: five systems, no recent games, favourites or pinned Curator shelves.
    fs::create_dir_all(root.join("data")).unwrap();
    fs::write(root.join("data/curator.json"), r#"{"lens":false,"stickers":false,"history":false}"#).unwrap();
    let systems = [("gba", "Game Boy Advance"), ("gbc", "Game Boy Color"), ("nes", "Nintendo Entertainment System"), ("psx", "PlayStation"), ("snes", "Super Nintendo")];
    for (panel_w, panel_h) in [(640usize, 480usize), (1024, 768), (1240, 1080), (1920, 1080)] {
        let size = format!("{panel_w}x{panel_h}");
        for appearance in ["dark", "light"] {
            fs::write(root.join("data/settings.json"), format!(r#"{{"appearance":"{appearance}","onboarded":true}}"#)).unwrap();
            // Paints the cabinet after the keys and returns what was painted (strings) and the picture's pixels.
            let run = |keys: &str| -> (String, Vec<u8>) {
                let record = root.join("painted.tsv");
                let png = root.join(format!("cabinet-{size}-{appearance}-{keys}.png"));
                let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
                cmd.args(["--snapshot", &format!("cabinet:{keys}"), png.to_str().unwrap()])
                    .env("POCKETUI_MOCK", "1").env("POCKETUI_LANG", "de")
                    .env("POCKETUI_DEVICE", "miniv2").env("POCKETUI_SNAPSHOT_SIZE", &size)
                    .env("POCKETUI_ES_SYSTEMS", root.join("es_systems.cfg"))
                    .env("POCKETUI_DATA", root.join("data")).env("POCKETUI_I18N_MISSES", &record);
                if _stub.is_some() { cmd.env("WAYLAND_DISPLAY", &socket); }
                let _ = fs::remove_file(&record);
                let out = cmd.output().unwrap();
                assert!(out.status.success(), "{size} {keys}: {}", String::from_utf8_lossy(&out.stderr));
                let painted = fs::read_to_string(&record).unwrap();
                let decoder = png::Decoder::new(std::io::BufReader::new(fs::File::open(png).unwrap()));
                let mut reader = decoder.read_info().unwrap();
                assert_eq!((reader.info().width, reader.info().height), (panel_w as u32, panel_h as u32));
                let mut pixels = vec![0; reader.output_buffer_size()];
                reader.next_frame(&mut pixels).unwrap();
                (painted, pixels)
            };
            let (_, first) = run("");
            let mut previous = first.clone();
            for (k, (id, name)) in systems.iter().enumerate() {
                // Down k times: the shelf is named on the screen, and the picture changed with the move.
                let keys = "d".repeat(k);
                let (painted, shown) = run(&keys);
                assert!(painted.contains(name), "{size} {appearance}: shelf {name} is not shown after {k} Down presses: {painted}");
                if k > 0 {
                    assert_ne!(shown, previous, "{size} {appearance}: Down {k} changed nothing on the screen");
                }
                previous = shown;
                // A on that shelf opens it: its game starts (the fixture is empty, so the player is told so by name).
                let (opened, _) = run(&format!("{keys}a"));
                assert!(opened.contains(&format!("Fixture {id} kann nicht starten")), "{size} {appearance}: A on {name} did not open it: {opened}");
            }
            // Down past the last shelf stays on it.
            let (_, past_end) = run("ddddddd");
            let (_, last) = run("dddd");
            // Up all the way is the first shelf again, looking the same however far the player had gone (the cabinet
            // starts on its Continue entry, so "the first shelf" is what one Down and one Up show).
            let (_, one_down) = run("d");
            let (_, first_shelf) = run("du");
            let (_, back) = run("ddddduuuuu");
            assert!(differing(&past_end, &last) < SAME, "{size} {appearance}: Down past the end moved the selection");
            assert!(differing(&back, &first_shelf) < SAME, "{size} {appearance}: Up does not return to the first shelf");
            assert!(differing(&one_down, &first_shelf) > 10.0 * SAME, "{size} {appearance}: moving one shelf must look different from staying");
        }
    }
}

#[path = "../src/test_clock.rs"]
mod test_clock;
