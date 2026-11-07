//! Native-size outcomes for escaped bugs. No device or real game core required.
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

fn render(root: &Path, state: &str, size: &str, name: &str) -> (String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_pocket-ui"))
        .args(["--snapshot", state]).arg(root.join(format!("{name}.png")))
        .env("WAYLAND_DISPLAY", root.join("wayland"))
        .env("POCKETUI_MOCK", "1").env("POCKETUI_LANG", "en")
        .env("POCKETUI_DEVICE", "miniv2").env("POCKETUI_TEXT_SIZE", "normal")
        .env("POCKETUI_SNAPSHOT_SIZE", size).env("POCKETUI_DATA", root.join("data"))
        .env("POCKETUI_ES_SYSTEMS", root.join("es_systems.cfg"))
        .env("POCKETUI_VISIBLE_TEXT", root.join("visible.txt"))
        .env("POCKETUI_TEXT_BOXES", root.join("boxes.tsv"))
        .output().unwrap();
    assert!(out.status.success(), "{state}: {}", String::from_utf8_lossy(&out.stderr));
    (String::from_utf8(out.stdout).unwrap(), fs::read_to_string(root.join("visible.txt")).unwrap())
}

#[test]
fn pause_at_640_after_provisional_800_paints_and_accepts_navigation_without_raise_timeout() {
    let tmp = tempfile::tempdir().unwrap();
    let _compositor = compositor(tmp.path());
    let (_, visible) = render(tmp.path(), "menu:pocket-core:0:startup-resize", "640x480", "pause");
    assert!(visible.contains("Continue"), "the frame admitted without waiting must contain the usable menu: {visible}");
    assert!(visible.to_lowercase().contains("quit"), "the quit action must also be visible: {visible}");
}

#[test]
fn every_cabinet_shelf_has_a_visible_case_title_showcase_and_hints_at_native_1024() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().to_owned();
    let evidence = std::env::var_os("POCKETUI_ESCAPED_EVIDENCE").map(std::path::PathBuf::from);
    fs::create_dir_all(root.join("data")).unwrap();
    fs::write(root.join("data/curator.json"), r#"{"lens":false,"stickers":false,"history":false}"#).unwrap();
    let mut xml = String::from("<systemList>");
    let systems = [("gba", "Game Boy Advance", "gba"), ("gbc", "Game Boy Color", "gbc"),
        ("nes", "Nintendo Entertainment System", "nes"), ("psx", "PlayStation", "bin"),
        ("snes", "Super Nintendo", "sfc")];
    for (id, full, ext) in systems {
        let dir = root.join(id);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(format!("Fixture {id}.{ext}")), "x").unwrap();
        xml += &format!("<system><name>{id}</name><fullname>{full}</fullname><path>{}</path><extension>.{ext}</extension></system>", dir.display());
    }
    xml += "</systemList>";
    fs::write(root.join("es_systems.cfg"), xml).unwrap();
    let _compositor = compositor(&root);
    for appearance in ["dark", "light"] {
        fs::write(root.join("data/settings.json"), format!(r#"{{"appearance":"{appearance}","onboarded":true,"motion":"off"}}"#)).unwrap();
        for (index, (id, full, _)) in systems.iter().enumerate() {
            let (stdout, visible) = render(&root, &format!("cabinet:{}", "d".repeat(index)), "1024x768", &format!("{appearance}-{id}"));
            if let Some(dir) = &evidence {
                fs::create_dir_all(dir).unwrap();
                fs::copy(root.join(format!("{appearance}-{id}.png")), dir.join(format!("{appearance}-{id}.png"))).unwrap();
            }
            assert!(visible.contains(full) && visible.contains(&format!("Fixture {id}")), "selected shelf and title: {visible}");
            assert!(visible.contains("Play") && visible.contains("Details"), "usable hints: {visible}");
            let line = stdout.lines().find(|l| l.starts_with("cabinet-outcome ")).expect("painted geometry");
            let field = |name: &str| line.split_whitespace().find_map(|p| p.strip_prefix(&format!("{name}="))).unwrap();
            let nums = |name| field(name).split(',').map(|v| v.parse::<i32>().unwrap()).collect::<Vec<_>>();
            let case = nums("case");
            let shelf = nums("shelf");
            let hints = nums("hints")[0];
            assert!(case[0] >= 0 && case[0] + case[2] <= 1024 && case[1] >= 196 && case[1] + case[3] <= shelf[0], "case clear of title and shelf: {line}");
            assert!(case[3] >= 120 && nums("showcase")[0] >= 150 && field("tv") == "true", "a usable showcase, not a tiny chip: {line}");
            assert!(shelf[0] > case[1] && shelf[1] <= hints, "selected case compartment above hints: {line}");
            let faults = fs::read_to_string(root.join("boxes.tsv")).unwrap();
            assert!(faults.is_empty(), "{appearance}/{id}: visible text overlaps or is clipped:\n{faults}");
            let decoder = png::Decoder::new(std::io::BufReader::new(fs::File::open(root.join(format!("{appearance}-{id}.png"))).unwrap()));
            let reader = decoder.read_info().unwrap();
            assert_eq!((reader.info().width, reader.info().height), (1024, 768), "native panel pixels");
        }
    }
}

#[path = "../src/test_clock.rs"]
mod test_clock;
