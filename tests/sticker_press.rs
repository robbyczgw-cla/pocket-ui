//! A sticker pressed onto the case in the game box (`sticker_press`, docs/DESIGN.md "Playful animations"), as the
//! player meets it on the made-up library (scripts/curator-fake-library.py): from the card's Stickers sheet, the sticker
//! is large and crooked when it is let go, is pressed in, springs and sits as the case's own picture has it; one taken
//! off lifts away; a button, Quick, Motion Off and Playful cases off all leave the case with its sticker at once.
//!
//! Frames are the box `POCKETUI_SNAPSHOT_STICKER_MS` into the press (animation time). Needs python3 (and, on Linux
//! without a compositor, scripts/wayland-stub.py), as tests/insert_rigs.rs.

use std::path::{Path, PathBuf};
use std::process::Command;

include!("common/root.rs");

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
    let path = tmp.join("wayland-stub");
    let child = Command::new("python3").args(["-I", &format!("{ROOT}/scripts/wayland-stub.py"), path.to_str().unwrap()]).spawn().expect("python3");
    let stub = Stub(child, path);
    test_clock::wait_for(|| stub.1.exists(), std::time::Duration::from_secs(2), "Wayland stub listening");
    Some(stub)
}

struct World {
    tmp: PathBuf,
    lib: PathBuf,
    stub: Option<Stub>,
    /// Last, so the stub is gone before the folder is.
    _dir: tempfile::TempDir,
}

/// Cozy with the playful animations, the stickers on the cases shown.
const COZY: &str = r#"{"onboarded": true, "theme": "shelf", "animations": "cozy", "extras": {"playful_animations": true, "case_stickers": true}}"#;
const QUICK: &str = r#"{"onboarded": true, "theme": "shelf", "animations": "quick", "extras": {"case_stickers": true}}"#;
const MOTION_OFF: &str = r#"{"onboarded": true, "theme": "shelf", "animations": "cozy", "motion": "off", "extras": {"playful_animations": true, "case_stickers": true}}"#;
const PLAYFUL_OFF: &str = r#"{"onboarded": true, "theme": "shelf", "animations": "cozy", "extras": {"playful_animations": false, "case_stickers": true}}"#;

impl World {
    fn new(name: &str) -> World {
        // Removed with the World, also when the test panics (docs/dev/tests.md "A test that needs a folder").
        let dir = tempfile::Builder::new().prefix(&format!("pui-sticker-{name}-")).tempdir().unwrap();
        let tmp = dir.path().to_path_buf();
        let lib = tmp.join("lib");
        let done = Command::new("python3").env("POCKETUI_MOCK_NOW", FIXTURE_NOW).args(["-I", &format!("{ROOT}/scripts/curator-fake-library.py"), lib.to_str().unwrap(), "dark"]).output().expect("python3");
        assert!(done.status.success(), "{}", String::from_utf8_lossy(&done.stderr));
        let stub = wayland(&tmp);
        World { tmp, lib, stub, _dir: dir }
    }

    fn run(&self, data: &Path, game: &str, keys: &str, size: &str, ms: Option<u32>, png: &Path) {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
        if let Some(s) = &self.stub {
            cmd.env("WAYLAND_DISPLAY", &s.1);
        }
        if let Some(ms) = ms {
            cmd.env("POCKETUI_SNAPSHOT_STICKER_MS", ms.to_string());
        }
        let out = cmd
            .args(["--snapshot", &format!("info:shelf:stickers:{game}:{keys}"), png.to_str().unwrap()])
            .env("POCKETUI_MOCK", "1")
            .env("POCKETUI_MOCK_NOW", FIXTURE_NOW)
            .env("POCKETUI_LANG", "en")
            .env("POCKETUI_SNAPSHOT_SIZE", size)
            .env("POCKETUI_ES_SYSTEMS", self.lib.join("es_systems.cfg"))
            .env("POCKETUI_DATA", data)
            .output()
            .expect("run pocket-ui");
        assert!(out.status.success(), "{game} {keys} {ms:?}: {}", String::from_utf8_lossy(&out.stderr));
    }

    /// The game box with the Stickers sheet opened and `keys` pressed on it (`a` presses A on the selected row, `d` moves
    /// down, `b` closes the sheet), `ms` into the press that closing it starts when given, otherwise long after it. Every
    /// frame has the library as it was made (a toggled sticker is saved): `prime` keys were pressed in a run before it.
    /// (width, height, RGB).
    fn frame(&self, settings: &str, game: &str, prime: &str, keys: &str, size: &str, ms: Option<u32>) -> (usize, usize, Vec<u8>) {
        let n = std::fs::read_dir(&self.tmp).unwrap().count();
        let data = self.tmp.join(format!("data-{n}"));
        copy(&self.lib.join("data"), &data);
        std::fs::write(data.join("settings.json"), settings).unwrap();
        let png = self.tmp.join(format!("frame-{n}.png"));
        if !prime.is_empty() {
            self.run(&data, game, prime, size, None, &png);
        }
        self.run(&data, game, keys, size, ms, &png);
        let mut reader = png::Decoder::new(std::fs::File::open(&png).unwrap()).read_info().unwrap();
        let mut buf = vec![0; reader.output_buffer_size()];
        let info = reader.next_frame(&mut buf).unwrap();
        (info.width as usize, info.height as usize, buf)
    }
}

impl World {
    /// `info:shelf:walk:<game>:<keys>`: the card opens as the box, then `keys` go where the pad's would (`S` SELECT), a
    /// frame after each; `ms` into the press when given. A fresh copy of the library each time. (RGB, stdout).
    fn walk(&self, settings: &str, game: &str, keys: &str, size: &str, ms: Option<u32>) -> (Vec<u8>, String) {
        let n = std::fs::read_dir(&self.tmp).unwrap().count();
        let data = self.tmp.join(format!("data-{n}"));
        copy(&self.lib.join("data"), &data);
        std::fs::write(data.join("settings.json"), settings).unwrap();
        let png = self.tmp.join(format!("walk-{n}.png"));
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
        if let Some(s) = &self.stub {
            cmd.env("WAYLAND_DISPLAY", &s.1);
        }
        if let Some(ms) = ms {
            cmd.env("POCKETUI_SNAPSHOT_STICKER_MS", ms.to_string());
        }
        let out = cmd
            .args(["--snapshot", &format!("info:shelf:walk:{game}:{keys}"), png.to_str().unwrap()])
            .env("POCKETUI_MOCK", "1")
            .env("POCKETUI_MOCK_NOW", FIXTURE_NOW)
            .env("POCKETUI_LANG", "en")
            .env("POCKETUI_SNAPSHOT_SIZE", size)
            .env("POCKETUI_ES_SYSTEMS", self.lib.join("es_systems.cfg"))
            .env("POCKETUI_DATA", &data)
            .output()
            .expect("run pocket-ui");
        assert!(out.status.success(), "{game} {keys} {ms:?}: {}", String::from_utf8_lossy(&out.stderr));
        let mut reader = png::Decoder::new(std::fs::File::open(&png).unwrap()).read_info().unwrap();
        let mut buf = vec![0; reader.output_buffer_size()];
        reader.next_frame(&mut buf).unwrap();
        (buf, String::from_utf8_lossy(&out.stdout).into_owned())
    }
}

fn copy(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap() {
        let e = e.unwrap();
        let target = to.join(e.file_name());
        if e.file_type().unwrap().is_dir() {
            copy(&e.path(), &target);
        } else {
            std::fs::copy(e.path(), target).unwrap();
        }
    }
}

fn differs(a: &[u8], b: &[u8]) -> usize {
    a.iter().zip(b).filter(|(x, y)| x != y).count()
}

/// How far apart two pictures are: the sum of the colour differences (a faint sticker is a small distance).
fn distance(a: &[u8], b: &[u8]) -> u64 {
    a.iter().zip(b).map(|(x, y)| u64::from(x.abs_diff(*y))).sum()
}

const GAME: &str = "snes:chrono";

/// Completed put on (the sheet's first row) and the sheet closed: in Cozy the sticker is let go over the case that does
/// not have it yet, large and crooked, is pressed in and springs, and the case has it as always once the press is over.
#[test]
fn a_sticker_put_on_lands_and_then_the_case_has_it_as_always() {
    let w = World::new("land");
    for size in ["640x480", "1280x960"] {
        let at = |ms| w.frame(COZY, GAME, "", "ab", size, ms);
        let after = at(None);
        // Let go (alpha 0 on the first frame): the case as it was, without the sticker.
        let first = at(Some(0));
        assert!(differs(&first.2, &after.2) > 300, "{size}: the sticker is on the case at the end, not at the start");
        let (early, mid, late, end) = (at(Some(60)), at(Some(125)), at(Some(250)), at(Some(318)));
        assert!(differs(&early.2, &first.2) > 200, "{size}: nothing to see 60 ms into the press");
        assert!(differs(&early.2, &after.2) > 100, "{size}: 60 ms in it is not where it will sit");
        // The spring dies away: each later moment is nearer to where it sits.
        let d = |f: &(usize, usize, Vec<u8>)| differs(&f.2, &after.2);
        assert!(d(&late) < d(&mid) && d(&end) <= d(&late), "{size}: not settling: {} {} {}", d(&mid), d(&late), d(&end));
        assert!(d(&end) * 20 < d(&first), "{size}: 318 ms in it is nearly where it sits: {} against {}", d(&end), d(&first));
    }
}

/// Taking Completed off (it was on before the box opened): it lifts away over the case without it and is gone.
#[test]
fn a_sticker_taken_off_lifts_away() {
    let w = World::new("lift");
    let size = "1280x960";
    let at = |ms| w.frame(COZY, GAME, "ab", "ab", size, ms);
    let gone = at(None);
    let start = at(Some(0));
    let half = at(Some(120));
    let late = at(Some(190));
    assert!(differs(&start.2, &gone.2) > 300, "the sticker is still on the case as it starts to lift");
    assert!(distance(&half.2, &gone.2) > 2000 && differs(&half.2, &start.2) > 100, "half way it has moved and is still on its way off");
    assert!(distance(&late.2, &gone.2) * 4 < distance(&half.2, &gone.2), "it is nearly gone at 190 ms: {} against {}", distance(&late.2, &gone.2), distance(&half.2, &gone.2));
}

/// Quick, Motion Off and Playful cases off, and any button, leave the case with its sticker at once: the same picture as
/// long after.
#[test]
fn without_playful_cozy_full_motion_or_after_a_button_the_sticker_just_sits() {
    let w = World::new("still");
    let size = "1280x960";
    for (name, settings) in [("quick", QUICK), ("motion off", MOTION_OFF), ("playful off", PLAYFUL_OFF)] {
        let end = w.frame(settings, GAME, "", "ab", size, None);
        let early = w.frame(settings, GAME, "", "ab", size, Some(40));
        assert_eq!(differs(&early.2, &end.2), 0, "{name}: the sticker is on the case at once");
    }
    // A button after the sheet closed (Right turns the box over, a button): the press is over.
    let end = w.frame(COZY, GAME, "", "abr", size, None);
    let after_button = w.frame(COZY, GAME, "", "abr", size, Some(40));
    assert_eq!(differs(&after_button.2, &end.2), 0, "any button ends the press");
}

/// A sheet closed with nothing changed: no press, the same case.
#[test]
fn closing_the_sheet_without_a_change_shows_the_case_as_it_is() {
    let w = World::new("same");
    let size = "1280x960";
    let end = w.frame(COZY, GAME, "", "b", size, None);
    let at_40 = w.frame(COZY, GAME, "", "b", size, Some(40));
    assert_eq!(differs(&at_40.2, &end.2), 0);
}

/// The way a player puts a sticker on in the Shelf look, by the keys alone: the card opens as the box, SELECT turns to its
/// page, Down to the Stickers row, A opens the sheet, A puts Completed on, B closes it. The card goes back to the box and
/// the sticker lands on the case there, at the H700's 640x480, the Brick's 1024x768 and 1280x720. Brick night check
/// 14.11.2026: the sheet is only on the page, and closing it there showed no press at all (it played only in the box).
#[test]
fn a_sticker_put_on_from_the_box_lands_on_the_case_back_in_the_box() {
    let w = World::new("walk");
    for size in ["640x480", "1024x768", "1280x720"] {
        // Down to the Stickers row, as many presses as the page needs.
        let downs = (1..=12).find(|n| w.walk(COZY, GAME, &format!("S{}a", "d".repeat(*n)), size, None).1.contains("page=Stickers"));
        let downs = downs.unwrap_or_else(|| panic!("{size}: the Stickers row is not reached from the box"));
        let keys = format!("S{}aab", "d".repeat(downs));
        let (first, out) = w.walk(COZY, GAME, &keys, size, Some(0));
        assert!(out.contains("walk box=true page=none"), "{size}: the sheet closed, back in the box: {out}");
        let (early, _) = w.walk(COZY, GAME, &keys, size, Some(60));
        let (after, _) = w.walk(COZY, GAME, &keys, size, None);
        assert!(differs(&first, &after) > 300, "{size}: the sticker is on the case at the end, not as the press begins");
        assert!(differs(&early, &first) > 200, "{size}: nothing moves 60 ms into the press");
        assert!(differs(&early, &after) > 100, "{size}: 60 ms in, it is not yet where it sits");
        // Nothing changed on the sheet: the page stays, as before.
        let (_, out) = w.walk(COZY, GAME, &format!("S{}ab", "d".repeat(downs)), size, None);
        assert!(out.contains("walk box=false page=none"), "{size}: no change, the page stays: {out}");
    }
}
