//! Five systems across small and large panels: the Cabinet has more shelves than fit, and Down must reach them all.
use std::{fs, path::PathBuf, process::{Child, Command}, time::Duration};

struct Stub(Child);
impl Drop for Stub {
    fn drop(&mut self) { let _ = self.0.kill(); let _ = self.0.wait(); }
}

#[test]
fn five_systems_remain_reachable_below_the_visible_cabinet() {
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
        let child = Command::new("python3").args(["-I", concat!(env!("CARGO_MANIFEST_DIR"), "/scripts/wayland-stub.py")]).arg(&socket).spawn().unwrap();
        let stub = Stub(child);
        for _ in 0..200 { if socket.exists() { break; } std::thread::sleep(Duration::from_millis(10)); }
        assert!(socket.exists());
        Some(stub)
    } else { None };
    // Match a simple card: five systems, no recent games, favourites or pinned Curator shelves.
    fs::create_dir_all(root.join("data")).unwrap();
    fs::write(root.join("data/curator.json"), r#"{"lens":false,"stickers":false,"history":false}"#).unwrap();
    for (panel_w, panel_h, layout_w, layout_h) in [
        (640usize, 480usize, 1280usize, 960usize), (1024, 768, 1024, 768),
        (1240, 1080, 1240, 1080), (1920, 1080, 1920, 1080),
    ] {
        let sx = |x| x * panel_w / layout_w;
        let sy = |y| y * panel_h / layout_h;
        let size = format!("{panel_w}x{panel_h}");
        for appearance in ["dark", "light"] {
            let mut headers = std::collections::HashMap::new();
            fs::write(root.join("data/settings.json"), format!(r#"{{"appearance":"{appearance}","onboarded":true}}"#)).unwrap();
            for (keys, name, more_below) in [
                ("", "Game Boy Advance", true), ("d", "Game Boy Color", true),
                ("dd", "Nintendo Entertainment System", true), ("ddd", "PlayStation", true),
                ("dddd", "Super Nintendo", false), ("ddddd", "Super Nintendo", false),
                ("dddduuuu", "Game Boy Advance", true),
            ] {
                let record = root.join("painted.tsv");
                let png = root.join(format!("cabinet-{size}-{appearance}-{}.png", keys.replace(' ', "")));
                let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
                cmd.args(["--snapshot", &format!("cabinet:{keys}"), png.to_str().unwrap()])
                    .env("POCKETUI_MOCK", "1").env("POCKETUI_LANG", "de")
                    .env("POCKETUI_DEVICE", "miniv2").env("POCKETUI_SNAPSHOT_SIZE", &size)
                    .env("POCKETUI_ES_SYSTEMS", root.join("es_systems.cfg"))
                    .env("POCKETUI_DATA", root.join("data")).env("POCKETUI_I18N_MISSES", &record);
                if _stub.is_some() { cmd.env("WAYLAND_DISPLAY", &socket); }
                let _ = fs::remove_file(&record);
                let out = cmd.output().unwrap();
                assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
                let painted = fs::read_to_string(&record).unwrap();
                assert!(painted.contains(name), "{keys}: selected shelf {name} missing: {painted}");
                let decoder = png::Decoder::new(std::io::BufReader::new(fs::File::open(png).unwrap()));
                let mut reader = decoder.read_info().unwrap();
                assert_eq!((reader.info().width, reader.info().height), (panel_w as u32, panel_h as u32));
                let mut pixels = vec![0; reader.output_buffer_size()];
                let info = reader.next_frame(&mut pixels).unwrap();
                let channels = info.color_type.samples();
                let pixel = |x: usize, y: usize| &pixels[(y * panel_w + x) * channels..(y * panel_w + x) * channels + 3];
                // The down arrow on the metadata line reveals shelves below the view and vanishes at the last one.
                assert_eq!(pixel(sx(layout_w - 80), sy(186)) != pixel(sx(layout_w - 80), sy(168)), more_below, "{keys}: continuation arrow");
                assert!((sx(layout_w - 184)..sx(layout_w - 96)).any(|x| (sy(172)..sy(193)).any(|y| pixel(x, y) != pixel(x, sy(168)))), "position is painted");
                // The recorder also saw intermediate frames. Compare the final header on repeated visits to
                // prove Up returns to the first selection, and Down past the end leaves the last selection intact.
                let mut header = Vec::new();
                for y in sy(40)..sy(200) { for x in sx(72)..sx(layout_w - 204) { header.extend_from_slice(pixel(x, y)); } }
                if let Some(previous) = headers.insert(name, header.clone()) {
                    assert_eq!(previous, header, "{keys}: final selection must return to {name}");
                }
            }
        }
    }
}
