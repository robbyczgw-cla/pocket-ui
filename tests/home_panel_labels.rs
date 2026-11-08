//! The same artless Curator tile for every known system, with one game so the day cannot change the pick.
use std::process::Command;
include!("common/root.rs");

struct Stub(std::process::Child);
impl Drop for Stub {
    fn drop(&mut self) { let _ = self.0.kill(); let _ = self.0.wait(); }
}

#[test]
fn every_console_keeps_its_identity_on_a_narrow_curator_title_card() {
    let tmp = tempfile::Builder::new().prefix("pui-tile-").tempdir_in("/tmp").unwrap();
    let display = tmp.path().join("wl");
    let _stub = (cfg!(target_os = "linux") && std::env::var_os("WAYLAND_DISPLAY").is_none_or(|s| s.is_empty())).then(|| {
        let child = Command::new("python3").args(["-I", &format!("{ROOT}/scripts/wayland-stub.py"), display.to_str().unwrap()]).spawn().unwrap();
        let stub = Stub(child);
        test_clock::wait_for(|| display.exists(), std::time::Duration::from_secs(8), "Wayland stub listening");
        stub
    });
    let mut failures = Vec::new();
    for console in pocket_ui::consoles::all() {
        let dir = tmp.path().join(&console.id);
        let roms = dir.join("roms");
        let data = dir.join("data");
        std::fs::create_dir_all(&roms).unwrap();
        std::fs::create_dir_all(&data).unwrap();
        std::fs::write(roms.join("Tetris.rom"), []).unwrap();
        std::fs::write(data.join("settings.json"), r#"{"onboarded":true,"theme":"shelf","appearance":"dark"}"#).unwrap();
        let cfg = dir.join("es_systems.cfg");
        std::fs::write(&cfg, format!("<systemList><system><name>{}</name><fullname>{}</fullname><path>{}</path><extension>.rom</extension></system></systemList>", console.id, console.name, roms.display())).unwrap();
        let short = match console.id.as_str() {
            "gb" => "GB", "gba" => "GBA", "gbc" => "GBC", "nes" => "NES", "snes" => "SNES", "n64" => "N64", "nds" => "DS",
            "psx" => "PS1", "psp" => "PSP", "3do" => "3DO", "atari2600" => "2600", "atari5200" => "5200", "atari7800" => "7800",
            "atari800" => "800", "atarijaguar" => "Jaguar", "atarilynx" => "Lynx", "colecovision" => "CV", "dreamcast" => "DC",
            "gamecube" => "GC", "gamegear" => "GG", "intellivision" => "INTV", "mastersystem" => "SMS", "megadrive" => "MD",
            "neocd" => "Neo CD", "neogeo" => "Neo Geo", "pcengine" => "PCE", "pokemini" => "POKEMINI", "psvita" => "PS Vita",
            "saturn" => "Saturn", "segacd" => "Sega CD", "supergrafx" => "SGX", "virtualboy" => "VB", "wonderswan" => "WS", "zxspectrum" => "ZX",
            _ => "",
        };
        for style in ["off", "cocoa"] {
            for size in ["normal", "large"] {
                let visible = dir.join("words.txt");
                let boxes = dir.join("words.box");
                let _ = std::fs::remove_file(&boxes);
                let bin = std::env::var_os("POCKETUI_NAV_BINARY").unwrap_or_else(|| env!("CARGO_BIN_EXE_pocket-ui").into());
                let mut cmd = Command::new(bin);
                cmd.args(["--snapshot", "curator:home", dir.join("tile.png").to_str().unwrap()])
                    .env("POCKETUI_MOCK", "1").env("POCKETUI_SOUND_QUANTUM", "0").env("POCKETUI_LANG", "en")
                    .env("POCKETUI_SNAPSHOT_SIZE", "1024x768").env("POCKETUI_TEXT_SIZE", size).env("POCKETUI_RETRO", style).env("POCKETUI_FONT", "barlow")
                    .env("POCKETUI_DATA", &data).env("POCKETUI_ES_SYSTEMS", &cfg)
                    .env("POCKETUI_VISIBLE_TEXT", &visible).env("POCKETUI_TEXT_BOXES", &boxes);
                if display.exists() { cmd.env("WAYLAND_DISPLAY", &display); }
                let output = cmd.output().unwrap();
                assert!(output.status.success(), "{}: {}", console.id, String::from_utf8_lossy(&output.stderr));
                let seen = std::fs::read_to_string(&visible).unwrap();
                let allowed = |line: &str| line == console.name || line == console.id.to_uppercase() ||
                    (!short.is_empty() && line == short) || console.aka.iter().any(|a| a == line);
                if !seen.lines().any(allowed) {
                    failures.push(format!("{} {style} {size}: no whole console name or known short name:\n{seen}", console.id));
                }
                assert!(seen.lines().any(|l| l.starts_with("Today: Tetris")), "{}: the one game's artless tile is shown: {seen}", console.id);
                let faults = std::fs::read_to_string(boxes).unwrap_or_default();
                assert!(faults.trim().is_empty(), "{} {style} {size}: {faults}", console.id);
            }
        }
    }
    assert!(failures.is_empty(), "{} truncated console labels:\n{}", failures.len(), failures.join("\n"));
}
