//! Kids Mode as a grown-up and a child meet it: the real program paints each state, and the words on the screen say what
//! is open and what is closed (docs/KIDS-MODE.md). The buttons and the emulators' shortcuts need a device: `src/kids.rs`,
//! `standalone`, `fn_keys` test their logic; docs/KIDS-MODE.md says what was seen where.

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

struct Card {
    dir: tempfile::TempDir,
    _stub: Option<Stub>,
}

impl Card {
    fn new() -> Card {
        // A short path: the stub's socket has to fit.
        let dir = tempfile::Builder::new().prefix("pui-kids-").tempdir_in("/tmp").unwrap();
        let status = Command::new("python3").args(["-I", &format!("{ROOT}/scripts/curator-fake-library.py"), dir.path().to_str().unwrap(), "dark"]).stdout(std::process::Stdio::null()).status().unwrap();
        assert!(status.success());
        let stub = (cfg!(target_os = "linux") && std::env::var_os("WAYLAND_DISPLAY").is_none_or(|w| w.is_empty())).then(|| {
            let child = Command::new("python3").args(["-I", &format!("{ROOT}/scripts/wayland-stub.py"), dir.path().join("wl").to_str().unwrap()]).spawn().unwrap();
            for _ in 0..200 {
                if dir.path().join("wl").exists() {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            Stub(child)
        });
        Card { dir, _stub: stub }
    }

    /// What the state's picture says, line by line. `safe`: the program started in safe mode.
    fn screen(&self, state: &str, safe: bool) -> String {
        let tmp = self.dir.path();
        let (png, words) = (tmp.join("shot.png"), tmp.join("words.tsv"));
        let _ = std::fs::remove_file(&words);
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
        if safe {
            cmd.arg("--safe");
        }
        if tmp.join("wl").exists() {
            cmd.env("WAYLAND_DISPLAY", tmp.join("wl"));
        }
        let out = cmd
            .args(["--snapshot", state, png.to_str().unwrap()])
            .env("POCKETUI_MOCK", "1")
            .env("POCKETUI_LANG", "en")
            .env("POCKETUI_VISIBLE_TEXT", &words)
            .env("POCKETUI_DATA", tmp.join("data"))
            .env("POCKETUI_ES_SYSTEMS", tmp.join("es_systems.cfg"))
            .output()
            .unwrap();
        assert!(out.status.success() && png.exists(), "{state}: {}", String::from_utf8_lossy(&out.stderr));
        std::fs::read_to_string(words).unwrap_or_default()
    }

    fn data(&self) -> &Path {
        self.dir.path()
    }
}

#[test]
fn every_screen_says_what_it_is_about() {
    let card = Card::new();
    for (state, words) in [
        ("kids-warning", "5 minutes left"),
        ("kids-warning:1", "1 minute left"),
        ("kids-timeup", "all for today"),
        ("kids-stuck", "Please ask a grown-up"),
        ("kids-empty", "No games here yet"),
        ("kids-flow:games-empty", "Add games"),
        ("kids-flow:games-left", "7 games aren't offered"),
        ("kids-flow:games-first", "The next steps show how to add them"),
        ("kids-adult:0:behind", "The clock is behind"),
    ] {
        let screen = card.screen(state, false);
        assert!(screen.contains(words), "{state} says {words:?}: {screen}");
    }
    let adult = card.screen("kids-adult", false);
    for row in ["30 more minutes today", "Start today's time again", "Turn off Kids mode"] {
        assert!(adult.contains(row), "{row}: {adult}");
    }
}

/// "30 more minutes today" and "Start today's time again" only mean something with a daily limit.
#[test]
fn the_grown_up_page_leaves_out_the_time_rows_under_no_limit() {
    let card = Card::new();
    let file = card.data().join("data/kids.json");
    let mut kids: serde_json::Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
    kids["minutes"] = serde_json::json!(45);
    std::fs::write(&file, kids.to_string()).unwrap();
    let limited = card.screen("kids-adult", false);
    assert!(limited.contains("30 more minutes today") && limited.contains("Start today's time again"), "{limited}");
    kids["minutes"] = serde_json::Value::Null;
    std::fs::write(&file, kids.to_string()).unwrap();
    let open = card.screen("kids-adult", false);
    assert!(!open.contains("30 more minutes today") && !open.contains("Start today's time again"), "{open}");
    assert!(open.contains("Games") && open.contains("Time per day") && open.contains("Turn off Kids mode"), "{open}");
}

#[test]
fn a_game_that_left_the_card_is_gone_from_the_shelf_and_an_empty_shelf_says_so() {
    let card = Card::new();
    let file = card.data().join("data/kids.json");
    let mut kids: serde_json::Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
    kids["games"] = serde_json::json!(["/nowhere/gone.gba"]);
    std::fs::write(&file, kids.to_string()).unwrap();
    let screen = card.screen("kids-shelf", false);
    assert!(screen.contains("No games here yet") && screen.contains("Ask a grown-up"), "{screen}");
}

#[test]
fn a_child_has_no_way_out_of_the_mode_on_the_power_menu_or_the_rescue_page() {
    let card = Card::new();
    let power = card.screen("kids-power", false);
    assert!(power.contains("Power off") && power.contains("Restart"), "{power}");
    for word in ["Restart pocket-ui", "boot menu", "Android"] {
        assert!(!power.contains(word), "{word}: {power}");
    }
    let door = card.screen("rescue:kids", true);
    assert!(door.contains("Please ask a grown-up"), "{door}");
    for word in ["Safe mode", "Reset", "EmulationStation", "crash"] {
        assert!(!door.contains(word), "{word}: {door}");
    }
    assert!(card.screen("rescue", true).contains("Safe mode"), "and the page itself for everyone else, and behind the PIN");
}

#[test]
fn the_steam_deck_has_no_kids_mode_and_the_handhelds_do() {
    let card = Card::new();
    assert!(card.screen("setup:play", false).contains("For a child"));
    assert!(!card.screen("setup:play:device=steamdeck", false).contains("For a child"));
}

/// The picker beside the list shows the cover of the game under the cursor with covers read on the cabinet's thread, as
/// a device reads them (the other snapshots read at once): it was an empty box, nothing took the finished cover.
#[test]
fn the_picker_shows_the_cover_of_the_game_under_the_cursor_read_on_the_thread() {
    let card = Card::new();
    let covers = card.data().join("data/covers");
    let script = card.data().join("mkpng.py");
    std::fs::write(
        &script,
        r#"import sys, zlib, struct
def ch(t, d): return struct.pack('>I', len(d)) + t + d + struct.pack('>I', zlib.crc32(t + d) & 0xffffffff)
raw = b''.join(b'\0' + b'\x30\x90\xd0' * 32 for _ in range(48))
open(sys.argv[1], 'wb').write(b'\x89PNG\r\n\x1a\n' + ch(b'IHDR', struct.pack('>IIBBBBB', 32, 48, 8, 2, 0, 0, 0)) + ch(b'IDAT', zlib.compress(raw)) + ch(b'IEND', b''))
"#,
    )
    .unwrap();
    let roms = card.data().join("roms");
    for system in std::fs::read_dir(&roms).unwrap().flatten() {
        let dir = covers.join(system.file_name());
        std::fs::create_dir_all(&dir).unwrap();
        for rom in std::fs::read_dir(system.path()).unwrap().flatten() {
            let stem = Path::new(&rom.file_name()).file_stem().unwrap().to_owned();
            let status = Command::new("python3").args(["-I", script.to_str().unwrap(), dir.join(stem).with_extension("png").to_str().unwrap()]).status().unwrap();
            assert!(status.success());
        }
    }
    let tmp = card.data();
    let out = Command::new(env!("CARGO_BIN_EXE_pocket-ui"))
        .args(["--snapshot", "kids-flow:games-thread", tmp.join("shot.png").to_str().unwrap()])
        .env("POCKETUI_MOCK", "1")
        .env("POCKETUI_LANG", "en")
        .env("POCKETUI_DATA", tmp.join("data"))
        .env("POCKETUI_ES_SYSTEMS", tmp.join("es_systems.cfg"))
        .envs(tmp.join("wl").exists().then(|| ("WAYLAND_DISPLAY", tmp.join("wl"))))
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert!(stdout.contains("kids-cover ready=true"), "the cover never reached the picker: {stdout}");
}

#[test]
fn the_kids_shelf_gets_its_covers_read_on_the_thread_and_then_stops_repainting() {
    let card = Card::new();
    let covers = card.data().join("data/covers");
    let script = card.data().join("mkpng.py");
    std::fs::write(
        &script,
        r#"import sys, zlib, struct
def ch(t, d): return struct.pack('>I', len(d)) + t + d + struct.pack('>I', zlib.crc32(t + d) & 0xffffffff)
raw = b''.join(b'\0' + b'\x30\x90\xd0' * 32 for _ in range(48))
open(sys.argv[1], 'wb').write(b'\x89PNG\r\n\x1a\n' + ch(b'IHDR', struct.pack('>IIBBBBB', 32, 48, 8, 2, 0, 0, 0)) + ch(b'IDAT', zlib.compress(raw)) + ch(b'IEND', b''))
"#,
    )
    .unwrap();
    let roms = card.data().join("roms");
    for system in std::fs::read_dir(&roms).unwrap().flatten() {
        let dir = covers.join(system.file_name());
        std::fs::create_dir_all(&dir).unwrap();
        for rom in std::fs::read_dir(system.path()).unwrap().flatten() {
            let stem = Path::new(&rom.file_name()).file_stem().unwrap().to_owned();
            let status = Command::new("python3").args(["-I", script.to_str().unwrap(), dir.join(stem).with_extension("png").to_str().unwrap()]).status().unwrap();
            assert!(status.success());
        }
    }
    let tmp = card.data();
    let out = Command::new(env!("CARGO_BIN_EXE_pocket-ui"))
        .args(["--snapshot", "kids-shelf-thread", tmp.join("shot.png").to_str().unwrap()])
        .env("POCKETUI_MOCK", "1")
        .env("POCKETUI_LANG", "en")
        .env("POCKETUI_DATA", tmp.join("data"))
        .env("POCKETUI_ES_SYSTEMS", tmp.join("es_systems.cfg"))
        .envs(tmp.join("wl").exists().then(|| ("WAYLAND_DISPLAY", tmp.join("wl"))))
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert!(stdout.contains("kids-cover ready=true") && stdout.contains("animating=false"), "the cover never reached the shelf, or it keeps repainting: {stdout}");
}
