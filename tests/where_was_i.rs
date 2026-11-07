//! "Where was I?" as a player meets it (`--snapshot where-was-i[:…]`, `app_where_was_i`): a moment continued after
//! three weeks shows when it was played last, the time played and the game card's note over the launch picture, in
//! every look, dark and light, English, German and Spanish, at 640x480 and 1240x1080; without a note there is no note
//! line; a button takes the card away at once and ends the game's still hold (`startup::skip_hold`); switched off in
//! Settings there is no card. Within two weeks there is none either, and how long the game stands still is
//! `startup::tests` (`where_was_i::tests`). No text is cut or runs
//! into other text (`POCKETUI_TEXT_BOXES`). On the made-up library (scripts/curator-fake-library.py). Needs python3 (scripts/wayland-stub.py on Linux without a compositor).

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
    let path = tmp.join("wl");
    let child = Command::new("python3").args(["-I", &format!("{ROOT}/scripts/wayland-stub.py"), path.to_str().unwrap()]).spawn().expect("python3");
    let stub = Stub(child, path);
    test_clock::wait_for(|| stub.1.exists(), std::time::Duration::from_secs(2), "Wayland stub listening");
    Some(stub)
}

struct Ctx {
    tmp: PathBuf,
    stub: Option<Stub>,
    n: std::cell::Cell<u32>,
}

struct Shot {
    visible: Vec<String>,
    faults: Vec<String>,
    /// The launch's skip file was written: the game's window waits for nothing.
    skipped: bool,
    png: PathBuf,
}

impl Ctx {
    fn shot(&self, look: &str, appearance: &str, state: &str, lang: &str, size: &str, more: &str) -> Shot {
        self.n.set(self.n.get() + 1);
        let n = self.n.get();
        let (data, tmpdir) = (self.tmp.join(format!("data-{n}")), self.tmp.join(format!("tmp-{n}")));
        std::fs::create_dir_all(&data).unwrap();
        std::fs::create_dir_all(&tmpdir).unwrap();
        std::fs::copy(self.tmp.join("lib/data/recents.json"), data.join("recents.json")).unwrap();
        std::fs::write(data.join("settings.json"), format!(r#"{{"onboarded": true, "theme": "{look}", "appearance": "{appearance}"{more}}}"#)).unwrap();
        let (visible, boxes) = (self.tmp.join(format!("{n}.txt")), self.tmp.join(format!("{n}.tsv")));
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
        cmd.args(["--snapshot", state, self.tmp.join(format!("{n}.png")).to_str().unwrap()])
            .env("POCKETUI_MOCK", "1")
            .env("POCKETUI_LANG", lang)
            .env("POCKETUI_SNAPSHOT_SIZE", size)
            .env("POCKETUI_VISIBLE_TEXT", &visible)
            .env("POCKETUI_TEXT_BOXES", &boxes)
            .env("POCKETUI_DATA", &data)
            .env("POCKETUI_ES_SYSTEMS", self.tmp.join("lib/es_systems.cfg"))
            .env("TMPDIR", &tmpdir);
        if let Some(s) = &self.stub {
            cmd.env("WAYLAND_DISPLAY", &s.1);
        }
        let out = cmd.output().expect("run pocket-ui");
        assert!(out.status.success(), "{state}: {}", String::from_utf8_lossy(&out.stderr));
        let read = |p: &Path| std::fs::read_to_string(p).unwrap_or_default();
        Shot {
            visible: read(&visible).lines().map(str::to_string).collect(),
            faults: read(&boxes).lines().filter(|l| l.starts_with('E') || l.starts_with('O')).map(str::to_string).collect(),
            skipped: tmpdir.join("pocket-launch-skip").exists(),
            png: self.tmp.join(format!("{n}.png")),
        }
    }
}

fn has(v: &[String], s: &str) -> bool {
    v.iter().any(|l| l.contains(s))
}

#[test]
fn the_card_after_a_long_break_its_note_a_button_and_the_switch() {
    let tmp = std::env::temp_dir().join(format!("pocketui-where-was-i-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    // The made-up library behind the fading shelf, as a player's start leaves it.
    let ok = Command::new("python3")
        .args(["-I", &format!("{ROOT}/scripts/curator-fake-library.py"), tmp.join("lib").to_str().unwrap(), "dark"])
        .stdout(std::process::Stdio::null())
        .status()
        .expect("python3");
    assert!(ok.success());
    let c = Ctx { stub: wayland(&tmp), tmp: tmp.clone(), n: Default::default() };

    for (lang, ago, played) in [("en", "Last played 3 weeks ago", "12 h 05 min played"), ("de", "Zuletzt vor 3 Wochen gespielt", "12 h 05 min gespielt"), ("es", "Jugado por última vez hace 3 semanas", "12 h 05 min de juego")] {
        for size in ["640x480", "1024x768", "1240x1080"] {
            for appearance in ["dark", "light"] {
                let s = c.shot("shelf", appearance, "where-was-i", lang, size, "");
                assert!(has(&s.visible, ago) && has(&s.visible, played), "{lang} {size} {appearance}: {:?}", s.visible);
                // The note as the player wrote it, never translated.
                assert!(has(&s.visible, "Fire temple, key still missing."), "{lang} {size} {appearance}: the note: {:?}", s.visible);
                assert!(s.faults.is_empty(), "{lang} {size} {appearance}: {:?}", s.faults);
                assert!(!s.skipped, "{lang} {size}: no button, the reminder stays");
                // A long note: three lines at most, ending in "…", inside the card.
                let s = c.shot("shelf", appearance, "where-was-i:long", lang, size, "");
                assert!(has(&s.visible, "Fire temple"), "{lang} {size}: {:?}", s.visible);
                assert!(s.faults.is_empty(), "{lang} {size} {appearance} long: {:?}", s.faults);
            }
            // Without a note: no note line (and no empty quotes).
            let s = c.shot("shelf", "dark", "where-was-i:plain", lang, size, "");
            assert!(has(&s.visible, ago), "{lang} {size}: {:?}", s.visible);
            assert!(!has(&s.visible, "“") && !has(&s.visible, "Fire temple"), "{lang} {size}: no note, no note line: {:?}", s.visible);
            // A big emulator's spinner under it: both readable, the card clear of the spinner's line. (That line over the
            // faded page's hint bar is the launch's own, from before the card.)
            let s = c.shot("shelf", "dark", "where-was-i:wait", lang, size, "");
            let card_faults: Vec<_> = s.faults.iter().filter(|f| f.contains(ago) || f.contains(played) || f.contains("Fire temple")).collect();
            assert!(has(&s.visible, ago) && card_faults.is_empty(), "{lang} {size} wait: {:?} {:?}", s.visible, card_faults);
        }
    }

    // Every look shows it the same way; Easy plainly, without the time played.
    for look in ["minimal", "split", "cards", "cabinet"] {
        let s = c.shot(look, "dark", "where-was-i", "en", "1240x1080", "");
        assert!(has(&s.visible, "Last played 3 weeks ago") && s.faults.is_empty(), "{look}: {:?} {:?}", s.visible, s.faults);
    }
    for size in ["640x480", "1240x1080"] {
        let s = c.shot("big", "light", "where-was-i:easy", "en", size, "");
        assert!(has(&s.visible, "Last played 3 weeks ago") && !has(&s.visible, "12 h"), "Easy {size}: {:?}", s.visible);
        assert!(s.faults.is_empty(), "Easy {size}: {:?}", s.faults);
    }

    // Any button: the card is gone at once, and the game plays at once.
    let s = c.shot("shelf", "dark", "where-was-i:press", "en", "1240x1080", "");
    assert!(!has(&s.visible, "Last played") && !has(&s.visible, "Fire temple"), "after a button: {:?}", s.visible);
    assert!(s.skipped, "a button ends the still hold (startup::skip_hold)");

    for state in ["where-was-i:early-press", "where-was-i:missing"] {
        let s = c.shot("shelf", "dark", state, "en", "1240x1080", "");
        assert!(!has(&s.visible, "Last played") && !has(&s.visible, "Fire temple"), "{state}: {:?}", s.visible);
        assert!(s.skipped, "{state}: an early button or failed picture releases the hold");
    }

    // Settings → Playing → Remind me where I was off: no card.
    let s = c.shot("shelf", "dark", "where-was-i", "en", "1240x1080", r#", "no_where_was_i": true"#);
    assert!(!has(&s.visible, "Last played"), "switched off: {:?}", s.visible);

    let _ = std::fs::remove_dir_all(&tmp);
}

/// A slow game load: a PS1 start stood 160 s on a still screen. While pocket-core is still loading after a few seconds the
/// launch says so, readable at the Brick's 1024x768 in every language, beside the card.
#[test]
fn a_slow_start_on_the_brick_says_it_is_loading() {
    let tmp = std::env::temp_dir().join(format!("pocketui-brick-loading-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    let ok = Command::new("python3")
        .args(["-I", &format!("{ROOT}/scripts/curator-fake-library.py"), tmp.join("lib").to_str().unwrap(), "dark"])
        .stdout(std::process::Stdio::null())
        .status()
        .expect("python3");
    assert!(ok.success());
    let c = Ctx { stub: wayland(&tmp), tmp: tmp.clone(), n: Default::default() };
    for (lang, word) in [("en", "Loading the game…"), ("de", "Spiel wird geladen…"), ("es", "Cargando el juego…")] {
        for appearance in ["dark", "light"] {
            let s = c.shot("shelf", appearance, "where-was-i:loading", lang, "1024x768", "");
            // The note stays clear of the reminder and readable over a game, even in the light appearance.
            let faults: Vec<_> = s.faults.iter().filter(|f| f.contains(word) && !f.contains("‹hints›")).collect();
            assert!(has(&s.visible, word) && faults.is_empty(), "{lang} {appearance}: {:?} {:?}", s.visible, faults);
            let mut reader = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(&s.png).unwrap())).read_info().unwrap();
            let mut pixels = vec![0; reader.output_buffer_size()];
            let info = reader.next_frame(&mut pixels).unwrap();
            let (w, h, channels) = (info.width as usize, info.height as usize, info.color_type.samples());
            let (mut light, mut dark) = (0, 0);
            // The line the player reads: enough bright letter ink against a dark backing. Before the fix,
            // Light painted dark letters into the game's black border; recording the words alone missed it.
            for y in h - 100..h - 42 {
                for x in w / 4..w * 3 / 4 {
                    let rgb = &pixels[(y * w + x) * channels..][..3];
                    light += usize::from(rgb.iter().all(|v| *v >= 192));
                    dark += usize::from(rgb.iter().all(|v| *v <= 48));
                }
            }
            assert!(light > 150 && dark > 150, "{lang} {appearance}: the loading text needs contrast (light {light}, dark {dark})");
        }
    }
    let _ = std::fs::remove_dir_all(&tmp);
}
