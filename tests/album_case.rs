//! The photo album in a game's case (`album`, `album_paint`), as a player reaches it: Down on the box's front opens the
//! case, Left and Right reach every photo (each a row the D-pad reaches, one with the focus), A looks closer, B comes back
//! and B again shuts the case; the hint bar names only keys that do something there; every word is in the interface's
//! language (all six); an empty case says why in a sentence that fits its slip. Needs python3 for the made-up library
//! and, on Linux without a compositor, scripts/wayland-stub.py.

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
    let child = Command::new("python3").args(["-I", &format!("{ROOT}/scripts/wayland-stub.py"), path.to_str().unwrap()]).spawn().expect("python3");
    let mut stub = Stub { path, child };
    let until = std::time::Instant::now() + test_clock::timeout(std::time::Duration::from_secs(120));
    while std::time::Instant::now() < until {
        if stub.path.exists() {
            return Some(stub);
        }
        assert!(stub.child.try_wait().unwrap().is_none(), "scripts/wayland-stub.py exited at start-up");
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    panic!("scripts/wayland-stub.py didn't start");
}

fn library(dir: &Path) {
    let status = Command::new("python3")
        .env("POCKETUI_MOCK_NOW", FIXTURE_NOW)
        .args(["-I", &format!("{ROOT}/scripts/curator-fake-library.py"), dir.to_str().unwrap(), "dark"])
        .stdout(std::process::Stdio::null())
        .status()
        .expect("python3 for scripts/curator-fake-library.py");
    assert!(status.success());
}

/// Settings → Screen & sound → Photos of big moments switched on (off on a new device), as the player's settings.json keeps it.
fn photos_on(dir: &Path, on: bool) {
    let file = dir.join("data/settings.json");
    let mut v: serde_json::Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
    if on {
        v["moment_photos"] = serde_json::Value::Bool(true);
    } else if let Some(m) = v.as_object_mut() {
        m.remove("moment_photos");
    }
    std::fs::write(&file, serde_json::to_vec(&v).unwrap()).unwrap();
}

/// Three photos in Golden Sun's case, as the camera keeps them (JPEG, the album's index).
fn photos(lib: &Path) {
    let dir = lib.join("data/album/gba/Golden Sun (USA, Europe)");
    for (i, note) in ["Isaac reached Lv. 10", "Isaac reached Lv. 20", "Isaac reached Lv. 30"].iter().enumerate() {
        let rgb: Vec<u8> = (0..240 * 160).flat_map(|p| [(p % 240) as u8, (p / 240 * (i + 1)) as u8, 120]).collect();
        let (jpeg, size) = pocket_ui::album::encode(240, 160, &rgb).unwrap();
        pocket_ui::album::add(&dir, 1_794_000_000 + i as i64 * 86_400, &[note.to_string(), "9 h 41 min".into()], size, &jpeg).unwrap();
    }
}

/// What a frame offers: its strings with their boxes (`x0 y0 x1 y1 string`), the text painter's problems, the strings
/// painted without a translation.
struct Shot {
    placed: Vec<(i32, i32, i32, i32, String)>,
    problems: String,
    misses: Vec<String>,
}

impl Shot {
    fn marks(&self, mark: &str) -> Vec<(i32, i32, i32, i32)> {
        self.placed.iter().filter(|p| p.4.starts_with(mark)).map(|p| (p.0, p.1, p.2, p.3)).collect()
    }
    fn hints(&self) -> Vec<String> {
        self.placed.iter().filter_map(|p| p.4.strip_prefix("‹nav:hint›").and_then(|h| h.split('\t').next()).map(str::to_string)).collect()
    }
    fn has(&self, what: &str) -> bool {
        self.placed.iter().any(|p| p.4.contains(what))
    }
}

fn shot(lib: &Path, stub: Option<&Stub>, state: &str, lang: &str, size: &str) -> Shot {
    let png = lib.join("shot.png");
    let (boxes, nav, misses) = (lib.join("text.box"), lib.join("shot.nav"), lib.join("misses.tsv"));
    for f in [&boxes, &nav, &misses] {
        let _ = std::fs::remove_file(f);
    }
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
    cmd.args(["--snapshot", state, png.to_str().unwrap()])
        .env("POCKETUI_MOCK", "1")
        .env("POCKETUI_MOCK_NOW", FIXTURE_NOW)
        .env("POCKETUI_LANG", lang)
        .env("POCKETUI_SNAPSHOT_SIZE", size)
        .env("POCKETUI_TEXT_BOXES", &boxes)
        .env("POCKETUI_NAV_PROBE", &nav)
        .env("POCKETUI_I18N_MISSES", &misses)
        .env("POCKETUI_ES_SYSTEMS", lib.join("es_systems.cfg"))
        .env("POCKETUI_DATA", lib.join("data"));
    if let Some(s) = stub {
        cmd.env("WAYLAND_DISPLAY", &s.path);
    }
    let out = cmd.output().unwrap();
    assert!(out.status.success() && png.exists(), "{state} {lang} {size}: {}", String::from_utf8_lossy(&out.stderr));
    let placed = std::fs::read_to_string(&nav)
        .unwrap_or_default()
        .lines()
        .filter_map(|l| {
            let p: Vec<&str> = l.splitn(5, '\t').collect();
            (p.len() == 5).then(|| Some((p[0].parse().ok()?, p[1].parse().ok()?, p[2].parse().ok()?, p[3].parse().ok()?, p[4].to_string())))?
        })
        .collect();
    let misses = std::fs::read_to_string(&misses).unwrap_or_default().lines().filter_map(|l| l.strip_prefix("M\t")).map(str::to_string).collect();
    Shot { placed, problems: std::fs::read_to_string(&boxes).unwrap_or_default(), misses }
}

const LANGS: [&str; 6] = ["en", "de", "es", "pt", "fr", "it"];

/// The case's own words: none of them may be painted untranslated (the photos' notes are translated by their parts).
fn album_words(misses: &[String]) -> Vec<String> {
    let ours = ["Photos", "Browse", "Look closer", "Close", "Delete", "Keep", "Back", "No photos yet", "Badge", "reached Lv.", "of 3", "Nov", "h ", "min"];
    misses.iter().filter(|m| ours.iter().any(|o| m.contains(o))).cloned().collect()
}

#[test]
fn the_case_opens_its_photos_are_reached_and_every_word_is_translated() {
    let tmp = tempfile::tempdir().unwrap();
    library(tmp.path());
    photos(tmp.path());
    photos_on(tmp.path(), true);
    let stub = wayland(tmp.path());
    for size in ["640x480", "1240x1080"] {
        for lang in LANGS {
            let name = format!("{lang} {size}");
            // The box's front: Down is named while the case holds photos.
            let front = shot(tmp.path(), stub.as_ref(), "box:gba:Golden Sun", lang, size);
            assert!(front.hints().iter().any(|k| k == "DOWN"), "{name}: the front's bar doesn't name Down: {:?}", front.hints());
            // Down: the case lies open, every photo a row the D-pad reaches, the newest on top with the focus.
            let case = shot(tmp.path(), stub.as_ref(), "box:gba:Golden Sun:d", lang, size);
            let (rows, focus) = (case.marks("‹nav:row›"), case.marks("‹nav:focus›"));
            assert_eq!(rows.len(), 3, "{name}: three photos, three rows");
            assert_eq!(focus.len(), 1, "{name}: one photo has the focus");
            let mut hints = case.hints();
            hints.sort();
            assert_eq!(hints, ["A", "B", "LEFT RIGHT", "X"], "{name}: the case's bar names its keys");
            assert!(case.problems.trim().is_empty(), "{name}: {}", case.problems);
            assert!(album_words(&case.misses).is_empty(), "{name}: painted untranslated: {:?}", album_words(&case.misses));
            // Left: the photo before it comes on top (the one on top always lies in the middle): the fan moves.
            let left = shot(tmp.path(), stub.as_ref(), "box:gba:Golden Sun:dl", lang, size);
            assert_eq!(left.marks("‹nav:focus›").len(), 1, "{name}: after Left");
            assert_ne!(left.marks("‹nav:row›"), rows, "{name}: Left didn't bring another photo on top");
            // A: the photo up close; its bar has no A (A does nothing more there).
            let close = shot(tmp.path(), stub.as_ref(), "box:gba:Golden Sun:da", lang, size);
            let mut h = close.hints();
            h.sort();
            assert_eq!(h, ["B", "LEFT RIGHT", "X"], "{name}: up close");
            assert!(album_words(&close.misses).is_empty(), "{name}: up close, untranslated: {:?}", album_words(&close.misses));
            // B, B: back to the case, then the case shuts: the box's front again.
            let shut = shot(tmp.path(), stub.as_ref(), "box:gba:Golden Sun:dabb", lang, size);
            assert!(shut.hints().iter().any(|k| k == "DOWN") && shut.marks("‹nav:row›").is_empty(), "{name}: B B didn't come back to the box's front: {:?}", shut.hints());
        }
    }
}

#[test]
fn an_empty_case_says_why_in_a_sentence_that_fits() {
    let tmp = tempfile::tempdir().unwrap();
    library(tmp.path());
    photos_on(tmp.path(), true);
    let stub = wayland(tmp.path());
    for size in ["640x480", "1240x1080"] {
        for lang in LANGS {
            let name = format!("{lang} {size}");
            let case = shot(tmp.path(), stub.as_ref(), "box:psx:Castlevania - Symphony of the Night:d", lang, size);
            assert!(case.marks("‹nav:row›").is_empty() && case.marks("‹nav:focus›").is_empty(), "{name}: no photo, nothing to reach");
            assert_eq!(case.hints(), ["B"], "{name}: only B does something in an empty case");
            assert!(case.problems.trim().is_empty(), "{name}: the sentence doesn't fit: {}", case.problems);
            assert!(album_words(&case.misses).is_empty(), "{name}: untranslated: {:?}", album_words(&case.misses));
            if lang == "en" {
                assert!(case.has("No photos yet."), "{name}: the empty case has no sentence");
            }
        }
    }
}

/// Off (a new device, 19.11.: optional, never in the way): even with photos from before in the album, the box names
/// no Photos and Down opens no case.
#[test]
fn off_the_box_has_no_photos_at_all() {
    let tmp = tempfile::tempdir().unwrap();
    library(tmp.path());
    photos(tmp.path());
    photos_on(tmp.path(), false);
    let stub = wayland(tmp.path());
    let front = shot(tmp.path(), stub.as_ref(), "box:gba:Golden Sun", "en", "640x480");
    assert!(!front.hints().iter().any(|k| k == "DOWN"), "off: the bar names Down: {:?}", front.hints());
    // The box's own bar as before (since 19.11. START gives way there to SELECT and turning over, `Card::start_yields`).
    assert!(front.hints().iter().any(|k| k == "LEFT RIGHT"), "off: the box's own bar as before: {:?}", front.hints());
    let down = shot(tmp.path(), stub.as_ref(), "box:gba:Golden Sun:d", "en", "640x480");
    assert!(down.marks("‹nav:row›").is_empty() && !down.has("of 3") && !down.has("No photos yet"), "off: Down opened a case");
}
