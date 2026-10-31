//! Settings → Games → Game check as a player meets it (feat/game-doctor): a card with the usual mistakes made here (a game
//! in `roms/snes` with the wrong extension, a BIOS in the wrong folder, German umlauts, a folder without a system), and a
//! card with nothing wrong, rendered with `pocket-ui --snapshot game-check:real`. What matters is what is on the screen, in
//! English and German, at 1280x720 and at 640x480 with Large text. Needs python3 for scripts/wayland-stub.py on Linux
//! without a compositor (as tests/card_rows.rs).

use std::path::{Path, PathBuf};
use std::process::Command;

const ROOT: &str = env!("CARGO_MANIFEST_DIR");

struct Stub {
    path: PathBuf,
    child: std::process::Child,
}

impl Drop for Stub {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn wayland(tmp: &Path) -> Option<Stub> {
    if !cfg!(target_os = "linux") || std::env::var_os("WAYLAND_DISPLAY").is_some_and(|w| !w.is_empty()) {
        return None;
    }
    let path = tmp.join("wl");
    let child = Command::new("python3").args(["-I", &format!("{ROOT}/scripts/wayland-stub.py"), path.to_str().unwrap()]).spawn().expect("python3 for scripts/wayland-stub.py");
    let stub = Stub { path, child };
    for _ in 0..200 {
        if stub.path.exists() {
            return Some(stub);
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    panic!("wayland stub did not start");
}

fn put(root: &Path, rel: &str, bytes: &[u8]) {
    let p = root.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, bytes).unwrap();
}

/// A card: its es_systems.cfg with the folders under `root`.
fn card(root: &Path) -> PathBuf {
    let sys = |id: &str, name: &str, exts: &str| format!("<system><name>{id}</name><fullname>{name}</fullname><path>{}/roms/{id}</path><extension>{exts}</extension></system>", root.display());
    let cfg = root.join("es_systems.cfg");
    std::fs::create_dir_all(root.join("roms/bios")).unwrap();
    std::fs::write(
        &cfg,
        format!(
            "<systemList>{}{}{}{}{}</systemList>",
            sys("gba", "Game Boy Advance", ".gba .zip"),
            sys("snes", "Super Nintendo", ".sfc .smc .zip"),
            sys("psx", "PlayStation", ".cue .chd .m3u"),
            sys("segacd", "Sega CD", ".cue .chd"),
            sys("megadrive", "Mega Drive", ".md .gen .zip"),
        ),
    )
    .unwrap();
    cfg
}

/// What is on the screen: the strings painted, and the painter's findings (text cut, or running into other text).
fn screen(tmp: &Path, stub: &Option<Stub>, cfg: &Path, state: &str, lang: &str, size: &str, text: &str) -> (String, Vec<String>) {
    let data = tmp.join(format!("data-{lang}-{size}-{}", state.replace(':', "_")));
    std::fs::create_dir_all(&data).unwrap();
    std::fs::write(data.join("settings.json"), r#"{"onboarded": true}"#).unwrap();
    let (visible, boxes) = (data.join("visible.txt"), data.join("boxes.tsv"));
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
    cmd.args(["--snapshot", state, data.join("out.png").to_str().unwrap()])
        .env("POCKETUI_MOCK", "1")
        .env("POCKETUI_LANG", lang)
        .env("POCKETUI_TEXT_SIZE", text)
        .env("POCKETUI_SNAPSHOT_SIZE", size)
        .env("POCKETUI_VISIBLE_TEXT", &visible)
        .env("POCKETUI_TEXT_BOXES", &boxes)
        .env("POCKETUI_DATA", &data)
        .env("POCKETUI_ES_SYSTEMS", cfg);
    if let Some(s) = stub {
        cmd.env("XDG_RUNTIME_DIR", tmp).env("WAYLAND_DISPLAY", &s.path);
    }
    let out = cmd.output().expect("pocket-ui runs");
    assert!(out.status.success(), "{state}: {}", String::from_utf8_lossy(&out.stderr));
    let shown = std::fs::read_to_string(visible).unwrap_or_default();
    let faults = std::fs::read_to_string(boxes).unwrap_or_default().lines().filter(|l| l.starts_with(['O', 'T', 'E'])).map(str::to_string).collect();
    (shown.lines().collect::<Vec<_>>().join(" "), faults)
}

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("gc{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn a_clean_card_says_all_good_and_counts_its_games() {
    let t = tmp("clean");
    let stub = wayland(&t);
    let cfg = card(&t);
    put(&t, "roms/gba/Metroid Fusion (Europe).gba", b"game");
    put(&t, "roms/snes/Chrono Trigger (USA).sfc", b"game");
    for (lang, size, text, good, ready) in [("en", "1280x720", "normal", "All good", "2 games ready"), ("de", "640x480", "large", "Alles gut", "2 Spiele startklar")] {
        let (shown, faults) = screen(&t, &stub, &cfg, "game-check:real", lang, size, text);
        assert!(shown.contains(good) && shown.contains(ready), "{lang}: {shown}");
        assert_eq!(faults, Vec::<String>::new(), "{lang} {size}");
    }
    let _ = std::fs::remove_dir_all(&t);
}

#[test]
fn nothing_on_the_card_says_so() {
    let t = tmp("none");
    let stub = wayland(&t);
    let cfg = card(&t);
    let (shown, _) = screen(&t, &stub, &cfg, "game-check:real", "de", "640x480", "large");
    assert!(shown.contains("Noch keine Spiele") || shown.contains("No games yet"), "never an empty page: {shown}");
    let _ = std::fs::remove_dir_all(&t);
}

#[test]
fn the_usual_mistakes_are_named_with_what_to_do() {
    let t = tmp("mistakes");
    let stub = wayland(&t);
    let cfg = card(&t);
    put(&t, "roms/gba/Pokémon Blau.gba", b"");
    put(&t, "roms/snes/Super Mario World.sfc", b"game");
    put(&t, "roms/snes/Chrono Trigger.gba", b"game");
    put(&t, "roms/segacd/Sonic CD.cue", b"FILE \"Sonic CD.bin\" BINARY\n");
    put(&t, "roms/segacd/Sonic CD.bin", b"x");
    put(&t, "roms/segacd/bios_CD_E.bin", &[1; 16]);
    put(&t, "roms/psx/Müller.cue", "FILE \"Müller.bin\" BINARY\n".as_bytes());
    put(&t, "roms/Sega Genesis/Sonic.md", b"x");

    let (en, faults) = screen(&t, &stub, &cfg, "game-check:real", "en", "1280x720", "normal");
    for row in ["BIOS files", "Discs missing a file", "Empty files", "Wrong folder or file type"] {
        assert!(en.contains(row), "{row}: {en}");
    }
    assert!(en.contains("of 3 games ready") || en.contains("of 4 games ready"), "{en}");
    assert_eq!(faults, Vec::<String>::new());
    let (de, faults) = screen(&t, &stub, &cfg, "game-check:real", "de", "640x480", "large");
    assert!(de.contains("Leere Dateien") && de.contains("Falscher Ordner"), "{de}");
    assert_eq!(faults, Vec::<String>::new());
    let _ = std::fs::remove_dir_all(&t);
}
