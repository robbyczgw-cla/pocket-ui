//! The Steam Deck at its own size (`docs/STEAMDECK.md`): the 0.2 screens rendered with
//! `pocket-ui --snapshot …:device=steamdeck` at 1280x800 on the made-up library, dark and light. What a player
//! would notice is asserted: every page paints something where its content is, no label is cut or run into
//! another (`POCKETUI_TEXT_BOXES` stays empty), the picture is exactly the Deck's 1280x800, and the controls
//! card prints the Deck's own letters (A at the bottom) with its back buttons L4/L5/R4/R5 on it.
//!
//! Needs python3 for the made-up library and, on Linux without a compositor, scripts/wayland-stub.py (as
//! tests/controls_card_snapshots.rs).

use std::path::{Path, PathBuf};
use std::process::Command;

include!("common/root.rs");

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
    let path = tmp.join("wayland-stub");
    let child = Command::new("python3").args(["-I", &format!("{ROOT}/scripts/wayland-stub.py"), path.to_str().unwrap()]).spawn().expect("python3 for scripts/wayland-stub.py");
    let stub = Stub { path, child };
    for _ in 0..200 {
        if stub.path.exists() {
            return Some(stub);
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    panic!("scripts/wayland-stub.py didn't start");
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap().flatten() {
        let target = to.join(e.file_name());
        if e.path().is_dir() {
            copy_dir(&e.path(), &target);
        } else {
            std::fs::copy(e.path(), target).unwrap();
        }
    }
}

/// A screen as (width, height, 0RGB pixels).
fn read_png(path: &Path) -> (usize, usize, Vec<u32>) {
    let mut reader = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(path).expect("snapshot written"))).read_info().unwrap();
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).unwrap();
    let ch = info.color_type.samples();
    let px = buf[..info.buffer_size()].chunks_exact(ch).map(|p| (p[0] as u32) << 16 | (p[1] as u32) << 8 | p[2] as u32).collect();
    (info.width as usize, info.height as usize, px)
}

/// How many pixels of the box (x, y, w, h) differ from the page colour at the screen's top left.
fn ink(shot: &(usize, usize, Vec<u32>), (x, y, w, h): (usize, usize, usize, usize)) -> usize {
    let bg = shot.2[0];
    (y..(y + h).min(shot.1)).flat_map(|row| (x..(x + w).min(shot.0)).map(move |col| (row, col))).filter(|&(row, col)| shot.2[row * shot.0 + col] != bg).count()
}

struct Shot {
    px: (usize, usize, Vec<u32>),
    shown: String,
}

fn library(tmp: &Path, look: &str) {
    let status = Command::new("python3")
        .env("POCKETUI_MOCK_NOW", FIXTURE_NOW)
        .args(["-I", &format!("{ROOT}/scripts/curator-fake-library.py"), tmp.join(look).to_str().unwrap(), look])
        .stdout(std::process::Stdio::null())
        .status()
        .expect("python3 for scripts/curator-fake-library.py");
    assert!(status.success(), "curator-fake-library.py {look} failed");
}

fn render(bin: &Path, tmp: &Path, look: &str, name: &str, state: &str, stub: Option<&Stub>) -> Shot {
    render_in(bin, tmp, look, name, state, stub, "en", "off", "normal")
}

#[allow(clippy::too_many_arguments)]
fn render_in(bin: &Path, tmp: &Path, look: &str, name: &str, state: &str, stub: Option<&Stub>, lang: &str, style: &str, size: &str) -> Shot {
    let data = tmp.join(format!("data-{look}-{name}"));
    copy_dir(&tmp.join(look).join("data"), &data);
    let png = tmp.join(format!("{look}-{name}.png"));
    let boxes = tmp.join(format!("{look}-{name}.boxes"));
    let transcript = tmp.join(format!("{look}-{name}.txt"));
    let mut cmd = Command::new(bin);
    if let Some(s) = stub {
        cmd.env("WAYLAND_DISPLAY", &s.path);
    }
    // "cocoa+pixelify": Retro in that palette and in those letters (`POCKETUI_RETRO_LETTERS`).
    let (style, letters) = style.split_once('+').unwrap_or((style, "departure"));
    let out = cmd
        .env("POCKETUI_RETRO_LETTERS", letters)
        .args(["--snapshot", state, png.to_str().unwrap()])
        .env("POCKETUI_MOCK", "1")
        .env("POCKETUI_ES_SYSTEMS", tmp.join(look).join("es_systems.cfg"))
        .env("POCKETUI_DATA", &data)
        .env("POCKETUI_SNAPSHOT_SIZE", "1280x800")
        .env("POCKETUI_TEXT_BOXES", &boxes)
        .env("POCKETUI_LANG", lang)
        .env("POCKETUI_RETRO", style)
        .env("POCKETUI_TEXT_SIZE", size)
        .env("POCKETUI_MOCK_NOW", FIXTURE_NOW)
        .env("POCKETUI_VISIBLE_TEXT", &transcript)
        .env_remove("POCKETUI_SCREENSHOTS")
        .output()
        .expect("run pocket-ui");
    assert!(out.status.success(), "{state}: {}", String::from_utf8_lossy(&out.stderr));
    let problems = std::fs::read_to_string(&boxes).unwrap();
    assert!(problems.trim().is_empty(), "{state} ({look}) at 1280x800: labels must be readable and separate: {problems}");
    let shot = read_png(&png);
    assert_eq!((shot.0, shot.1), (1280, 800), "{state}: the Deck's own pixels");
    Shot { px: shot, shown: std::fs::read_to_string(&transcript).unwrap() }
}

#[test]
fn the_decks_screens_at_1280x800() {
    let guard = tempfile::tempdir().unwrap();
    let tmp = guard.path().to_path_buf();
    library(&tmp, "dark");
    library(&tmp, "light");
    let bin = PathBuf::from(env!("CARGO_BIN_EXE_pocket-ui"));
    let stub = wayland(&tmp);
    let (w, h) = (1280usize, 800usize);
    let body = (w / 20, h / 6, w * 9 / 10, h * 2 / 3);

    // Home (the shelf's console page) and the Cabinet with its case and TV, in both looks.
    for look in ["dark", "light"] {
        let home = render(&bin, &tmp, look, "home", "shelf:snes:device=steamdeck", stub.as_ref());
        assert!(ink(&home.px, body) > (w * h) / 100, "{look}: Home is painted");
        assert!(home.shown.contains("Super Nintendo"), "{look}: the console is named: {}", home.shown);
        let cabinet = render(&bin, &tmp, look, "cabinet", "cabinet:snes:device=steamdeck", stub.as_ref());
        assert!(ink(&cabinet.px, (0, h / 2, w, h / 2)) > (w * h) / 50, "{look}: the cabinet's shelf is painted");
    }

    // The pause menu over a pocket-core game: its rows are there.
    let pause = render(&bin, &tmp, "dark", "pause", "menu:pocket-core:device=steamdeck", stub.as_ref());
    for row in ["Continue", "Save & quit"] {
        assert!(pause.shown.lines().any(|l| l == row), "the pause menu's {row}: {}", pause.shown);
    }

    // "Where was I?" over the moment it continues.
    let wherewasi = render(&bin, &tmp, "dark", "where", "where-was-i:device=steamdeck", stub.as_ref());
    assert!(wherewasi.shown.contains("Last played 3 weeks ago"), "{}", wherewasi.shown);

    // The controls card with the Deck's printing and its back buttons, for two consoles.
    for (name, state, words) in [
        ("controls-snes", "controls-card:snes:device=steamdeck", ["L4", "L5", "R4", "R5", "HOME"].as_slice()),
        ("controls-psx", "controls-card:psx:device=steamdeck", ["Cross", "Circle", "Square", "Triangle", "L5"].as_slice()),
    ] {
        let card = render(&bin, &tmp, "dark", name, state, stub.as_ref());
        for word in words {
            assert!(card.shown.lines().any(|l| l == *word), "{state}: {word} is on the card: {}", card.shown);
        }
        assert!(ink(&card.px, body) > (w * h) / 400, "{state}: the buttons are drawn");
    }

    // Easy to read, the Tour's last card.
    let easy = render(&bin, &tmp, "dark", "tour-easy", "tour:easy_to_read:device=steamdeck", stub.as_ref());
    assert!(easy.shown.contains("Easy to read"), "{}", easy.shown);

    // Zoom over a game's frozen picture.
    let zoom = render(&bin, &tmp, "dark", "zoom", "zoom:device=steamdeck", stub.as_ref());
    assert!(ink(&zoom.px, body) > (w * h) / 50, "the zoomed picture fills the screen");
    assert!(zoom.shown.contains("Back"), "its hints are there: {}", zoom.shown);
}

#[test]
fn the_decks_neighbouring_console_cards_keep_their_whole_names_in_large_retro() {
    let guard = tempfile::tempdir().unwrap();
    let tmp = guard.path();
    let bin = PathBuf::from(env!("CARGO_BIN_EXE_pocket-ui"));
    let stub = wayland(tmp);
    for look in ["dark", "light"] {
        library(tmp, look);
        for style in ["off", "cocoa"] {
            for size in ["normal", "large"] {
                let shot = render_in(&bin, tmp, look, &format!("names-{style}-{size}"), "shelf:gba:cards:device=steamdeck", stub.as_ref(), "de", style, size);
                assert!(!shot.shown.lines().any(|l| l == "Spiel" || l.contains("Boy …") || l.contains("Boy ...")), "console names must not be translated a word at a time or cut: {}", shot.shown);
                assert!(shot.shown.lines().any(|l| l == "Color" || l.ends_with("Boy Color")), "the neighbouring Game Boy Color must keep Color: {}", shot.shown);
                let games = render_in(&bin, tmp, look, &format!("game-names-{style}-{size}"), "info:cards:ps2:revenge:dda:device=steamdeck", stub.as_ref(), "de", style, size);
                assert!(!games.shown.contains('…'), "printed game names on Cards must stay whole too: {}", games.shown);
            }
        }
    }
}

#[test]
fn the_decks_achievement_counts_stay_complete_in_retro() {
    let guard = tempfile::tempdir().unwrap();
    let tmp = guard.path();
    let bin = PathBuf::from(env!("CARGO_BIN_EXE_pocket-ui"));
    let stub = wayland(tmp);
    for look in ["dark", "light"] {
        library(tmp, look);
        for size in ["normal", "large"] {
            let shot = render_in(&bin, tmp, look, &format!("achievements-{size}"), "achievements-list:0:device=steamdeck", stub.as_ref(), "de", "cocoa", size);
            assert!(shot.shown.contains("98 Punkten"), "both achievement and point totals must remain visible: {}", shot.shown);
        }
    }
}

#[test]
fn the_decks_printed_game_names_never_become_weekdays() {
    let guard = tempfile::tempdir().unwrap();
    let tmp = guard.path();
    let bin = PathBuf::from(env!("CARGO_BIN_EXE_pocket-ui"));
    let stub = wayland(tmp);
    for look in ["dark", "light"] {
        library(tmp, look);
        // Pixelify's letters break the name after "Golden", as the Deck showed it: "Sun" alone on the second line.
        let shot = render_in(&bin, tmp, look, "golden-name", "info:minimal:gba:golden:device=steamdeck", stub.as_ref(), "de", "cocoa+pixelify", "large");
        assert!(shot.shown.lines().any(|line| line == "Sun"), "the second line of Golden Sun must remain the name: {}", shot.shown);
        assert!(!shot.shown.lines().any(|line| line == "So"), "the game's name must not become the Sunday abbreviation: {}", shot.shown);
        // Departure Mono's, Retro's default since 0.3.2, keep it on one line or break it the same way: the name whole either way.
        let shot = render_in(&bin, tmp, look, "golden-name-departure", "info:minimal:gba:golden:device=steamdeck", stub.as_ref(), "de", "cocoa", "large");
        // The title is the first thing under the clock (the series further down names it too).
        let lines: Vec<&str> = shot.shown.lines().collect();
        let title = lines.iter().position(|l| l.starts_with("Golden")).expect("the title");
        assert!(title <= 1 && (lines[title] == "Golden Sun" || lines[title..].starts_with(&["Golden", "Sun"])), "the title Golden Sun must show whole: {}", shot.shown);
        assert!(!lines.contains(&"So"), "the game's name must not become the Sunday abbreviation: {}", shot.shown);
    }
}

#[test]
fn the_decks_spanish_system_facts_keep_labels_beside_their_values() {
    let guard = tempfile::tempdir().unwrap();
    let tmp = guard.path();
    let bin = PathBuf::from(env!("CARGO_BIN_EXE_pocket-ui"));
    let stub = wayland(tmp);
    for look in ["dark", "light"] {
        library(tmp, look);
        for size in ["normal", "large"] {
            let shot = render_in(&bin, tmp, look, &format!("spanish-facts-{size}"), "shelf:gba:device=steamdeck", stub.as_ref(), "es", "cocoa", size);
            assert!(shot.shown.contains("Lanzamiento") && shot.shown.contains("Funciona con"), "complete translated labels: {}", shot.shown);
        }
    }
}
