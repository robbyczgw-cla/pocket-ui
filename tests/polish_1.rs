//! The polish review of 12.11.2026 (fable52/POLISH-REVIEW.md), what a player sees on the shelf and in the way into a game:
//!
//! - **A quiet sign, not a count.** While covers, pictures and descriptions load in the background, the shelf shows a small
//!   dot beside the clock, never "Covers 41 of 184 · Descriptions 16 of 198".
//! - **One light.** On the way into the console everything but the console dims alike in every look: title, hint bar,
//!   shelf. The console comes up from below the hint bar, never over it, and stands on a soft contact shadow.
//! - **One kind of name on the spines.** "GBA, N64" beside "Game Boy Color, Sega Master System, PlayStation 3" and a squeezed
//!   "Sega Dreamcast (experimental)" are the short names everywhere (`system_names`).
//!
//! The real program paints each state at 640x480, 1240x1080 and 1920x1080 and the test reads the picture and the words.
//! `POCKETUI_TEST_BIN` runs another pocket-ui than the one just built (the "before" proof of a fix).
//!
//! Needs python3 (and, on Linux without a compositor, scripts/wayland-stub.py), as tests/spine_names.rs.

use std::path::{Path, PathBuf};
use std::process::Command;

include!("common/root.rs");

const SIZES: [&str; 3] = ["640x480", "1240x1080", "1920x1080"];

struct Stub(std::process::Child, PathBuf);

impl Drop for Stub {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

struct Card {
    dir: tempfile::TempDir,
    stub: Option<Stub>,
    n: std::cell::Cell<u32>,
}

impl Card {
    fn new() -> Card {
        // A short path: the stub's socket has to fit.
        let dir = tempfile::Builder::new().prefix("pui-p1-").tempdir_in("/tmp").unwrap();
        let ok = Command::new("python3")
            .env("POCKETUI_MOCK_NOW", FIXTURE_NOW)
            .args(["-I", &format!("{ROOT}/scripts/curator-fake-library.py"), dir.path().to_str().unwrap(), "dark"])
            .stdout(std::process::Stdio::null())
            .status()
            .expect("python3 for scripts/curator-fake-library.py");
        assert!(ok.success());
        // The player's settings: the look is chosen by each state, the first-start setup is done.
        let settings = dir.path().join("data/settings.json");
        let mut json: serde_json::Value = std::fs::read_to_string(&settings).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_else(|| serde_json::json!({}));
        json["onboarded"] = serde_json::json!(true);
        std::fs::write(&settings, json.to_string()).unwrap();
        let stub = (cfg!(target_os = "linux") && std::env::var_os("WAYLAND_DISPLAY").is_none_or(|w| w.is_empty())).then(|| {
            let path = dir.path().join("wl");
            let child = Command::new("python3").args(["-I", &format!("{ROOT}/scripts/wayland-stub.py"), path.to_str().unwrap()]).spawn().unwrap();
            let stub = Stub(child, path);
            test_clock::wait_for(|| stub.1.exists(), std::time::Duration::from_secs(2), "Wayland stub listening");
            stub
        });
        Card { dir, stub, n: Default::default() }
    }

    /// Systems a Pocket may have that the made-up library lacks, with the names ROCKNIX gives them, one game each: added to
    /// the library's list of systems.
    fn with_more_systems(self) -> Card {
        let cfg = self.dir.path().join("es_systems.cfg");
        let text = std::fs::read_to_string(&cfg).unwrap();
        let mut extra = String::new();
        for (id, full, ext, game) in [
            ("dreamcast", "Sega Dreamcast (experimental)", ".chd", "Crazy Taxi (USA).chd"),
            ("mastersystem", "Sega Master System", ".sms", "Alex Kidd in Miracle World (USA).sms"),
            ("saturn", "Sega Saturn (experimental)", ".chd", "Nights into Dreams (USA).chd"),
            ("segacd", "Sega CD", ".chd", "Sonic CD (USA).chd"),
            ("atari2600", "Atari 2600", ".a26", "Pitfall (USA).a26"),
            ("pcengine", "NEC PC Engine / TurboGrafx-16", ".pce", "Bonk's Adventure (USA).pce"),
            ("wonderswancolor", "Bandai WonderSwan Color", ".wsc", "Mr. Driller (Japan).wsc"),
        ] {
            let roms = self.dir.path().join("roms").join(id);
            std::fs::create_dir_all(&roms).unwrap();
            std::fs::write(roms.join(game), b"made up").unwrap();
            extra += &format!("<system><name>{id}</name><fullname>{full}</fullname><path>{}</path><extension>{ext}</extension><command>x</command></system>\n", roms.display());
        }
        let at = text.rfind("</systemList>").expect("a systemList");
        std::fs::write(&cfg, format!("{}{extra}{}", &text[..at], &text[at..])).unwrap();
        self
    }

    /// One snapshot: its picture, and the words and spines it showed. `env` is added to the program's environment.
    fn shoot(&self, state: &str, size: &str, env: &[(&str, &str)]) -> Shot {
        let tmp = self.dir.path();
        let id = self.n.get();
        self.n.set(id + 1);
        let (png, words, spines) = (tmp.join(format!("shot{id}.png")), tmp.join(format!("words{id}.tsv")), tmp.join(format!("spines{id}.tsv")));
        let bin = std::env::var_os("POCKETUI_TEST_BIN").map_or_else(|| PathBuf::from(env!("CARGO_BIN_EXE_pocket-ui")), PathBuf::from);
        let mut cmd = Command::new(bin);
        if let Some(s) = &self.stub {
            cmd.env("WAYLAND_DISPLAY", &s.1);
        }
        for (k, v) in env {
            cmd.env(k, v);
        }
        let out = cmd
            .args(["--snapshot", state, png.to_str().unwrap()])
            .env("POCKETUI_MOCK", "1")
            .env("POCKETUI_MOCK_NOW", FIXTURE_NOW)
            .env("POCKETUI_LANG", "en")
            .env("POCKETUI_SNAPSHOT_SIZE", size)
            .env("POCKETUI_VISIBLE_TEXT", &words)
            .env("POCKETUI_SPINE_LINES", &spines)
            .env("POCKETUI_ES_SYSTEMS", tmp.join("es_systems.cfg"))
            .env("POCKETUI_DATA", tmp.join("data"))
            .output()
            .expect("run pocket-ui");
        assert!(out.status.success() && png.exists(), "{state} {size}: {}", String::from_utf8_lossy(&out.stderr));
        let image = pocket_ui::image::load_image(&png).expect("the snapshot's picture");
        let text = |p: &Path| std::fs::read_to_string(p).unwrap_or_default();
        let shot = Shot { w: image.w as i32, h: image.h as i32, px: image.px, words: text(&words), spines: text(&spines) };
        for p in [&png, &words, &spines] {
            let _ = std::fs::remove_file(p);
        }
        shot
    }
}

struct Shot {
    w: i32,
    h: i32,
    px: Vec<u32>,
    /// The words the picture says, one per line.
    words: String,
    /// The title of every spine painted, one per line (the first column of `POCKETUI_SPINE_LINES`).
    spines: String,
}

impl Shot {
    fn at(&self, x: i32, y: i32) -> [i32; 3] {
        let p = self.px[(y * self.w + x) as usize];
        [((p >> 16) & 255) as i32, ((p >> 8) & 255) as i32, (p & 255) as i32]
    }

    /// What this picture keeps of the light of `base`'s pixels in the area (x0, y0, x1, y1): the sum of brightness here over
    /// the sum there. With `lit_only` only the pixels that were not nearly black count (a black page has nothing to dim);
    /// None when there is nothing to measure.
    fn kept(&self, base: &Shot, (x0, y0, x1, y1): (i32, i32, i32, i32), lit_only: bool) -> Option<f32> {
        let (mut now, mut then) = (0i64, 0i64);
        for y in y0..y1.min(self.h) {
            for x in x0..x1.min(self.w) {
                let b = base.at(x, y);
                let sum: i32 = b.iter().sum();
                if !lit_only || sum > 3 * 40 {
                    now += self.at(x, y).iter().sum::<i32>() as i64;
                    then += sum as i64;
                }
            }
        }
        (then > 200).then(|| now as f32 / then as f32)
    }

    /// Pixels that differ from `base`'s (by more than noise) in the area.
    fn changed(&self, base: &Shot, (x0, y0, x1, y1): (i32, i32, i32, i32)) -> usize {
        let mut n = 0;
        for y in y0..y1.min(self.h) {
            for x in x0..x1.min(self.w) {
                let (a, b) = (self.at(x, y), base.at(x, y));
                if (0..3).any(|k| (a[k] - b[k]).abs() > 12) {
                    n += 1;
                }
            }
        }
        n
    }
}

/// Screen pixels per layout pixel: a panel of 720 lines or fewer is laid out on a 960-line canvas and scaled down
/// (`scaled_layout`), the bigger ones keep their own pixels.
fn k(h: i32) -> f32 {
    if h <= 720 { h as f32 / 960.0 } else { 1.0 }
}

/// The first row of the bar of hints on a panel `h` lines high: 120 layout pixels over the bottom edge (`kit::hint_bar_top`).
fn bar_top(h: i32) -> i32 {
    (h as f32 - 120.0 * k(h)).ceil() as i32 + 1
}

fn dims(size: &str) -> (i32, i32) {
    let (w, h) = size.split_once('x').unwrap();
    (w.parse().unwrap(), h.parse().unwrap())
}

#[test]
fn a_download_in_the_background_shows_a_dot_and_no_count() {
    let card = Card::new();
    for look in ["shelf", "cabinet", "split"] {
        for size in SIZES {
            let (w, h) = dims(size);
            let rest = card.shoot(&format!("shelf:snes:{look}"), size, &[]);
            let busy = card.shoot(&format!("shelf:snes:{look}:bgrun"), size, &[]);
            for word in ["Covers", "Descriptions", "Pictures"] {
                assert!(!busy.words.contains(word), "{look} {size}: the shelf says \"{word}\": {}", busy.words);
            }
            assert!(!busy.words.lines().any(|l| l.contains(" of ") && l.chars().any(|c| c.is_ascii_digit()) && !l.contains(" / ")), "{look} {size}: a count on the shelf: {}", busy.words);
            // Something is there, small and in the top right corner beside the clock: a sign, not a line of words.
            let corner = (w / 2, 0, w, 130.min(h / 4));
            let sign = busy.changed(&rest, corner);
            assert!(sign > 10, "{look} {size}: the shelf shows nothing while it loads");
            assert!(sign < 400, "{look} {size}: {sign} pixels changed, a sign is small");
            assert_eq!(busy.changed(&rest, (0, 130.min(h / 4), w, h)), 0, "{look} {size}: only the corner shows it");
        }
    }
}

/// The areas of a shelf the way into the console dims: the header (screen title, clock, the game's title), the bar of hints
/// and the wall beside the middle where the console stands. The header leaves out the place left of the title where the
/// Shelf look shows the selected case: the case goes into the console (it moves, it is not dimmed), and a light front
/// (a home-made one's masking tape, `tape_label`) leaving it read as a header gone dark.
fn areas(w: i32, h: i32) -> [(&'static str, (i32, i32, i32, i32)); 5] {
    [
        ("header", (w * 7 / 20, 0, w, h / 5)),
        ("screen title", (0, 0, w * 7 / 20, h / 12)),
        ("hint bar", (0, bar_top(h), w, h)),
        ("shelf, left", (0, h * 11 / 20, w / 4, h * 17 / 20)),
        ("shelf, right", (w * 3 / 4, h * 11 / 20, w, h * 17 / 20)),
    ]
}

#[test]
fn the_way_into_the_console_dims_title_hints_and_shelf_alike_in_every_look() {
    let card = Card::new();
    for look in ["cabinet", "shelf"] {
        for size in SIZES {
            let (w, h) = dims(size);
            let state = format!("shelf:snes:{look}:games:2");
            let rest = card.shoot(&state, size, &[]);
            let scene = card.shoot(&state, size, &[("POCKETUI_SNAPSHOT_LAUNCH_MS", "900")]);
            let kept: Vec<(&str, f32)> = areas(w, h).iter().filter_map(|(name, a)| scene.kept(&rest, *a, true).map(|k| (*name, k))).collect();
            assert!(kept.iter().any(|(n, _)| *n == "header") && kept.iter().any(|(n, _)| *n == "hint bar"), "{look} {size}: {kept:?}");
            let (lo, hi) = kept.iter().fold((9.0f32, 0.0f32), |(lo, hi), (_, k)| (lo.min(*k), hi.max(*k)));
            // Everything dims, nothing goes black (the Shelf look's own title fades a little further than the rest).
            assert!(hi < 0.65 && lo > 0.1, "{look} {size}: something stays lit while the console comes in: {kept:?}");
            assert!(hi - lo < 0.25, "{look} {size}: the light is not one light: {kept:?}");
        }
    }
}

#[test]
fn the_console_never_passes_over_the_hint_bar() {
    let card = Card::new();
    for look in ["cabinet", "shelf"] {
        for size in SIZES {
            let (w, h) = dims(size);
            let state = format!("shelf:snes:{look}:games:2");
            let rest = card.shoot(&state, size, &[]);
            // The console rises from below the screen's edge: the first moments are the ones that cross the bar.
            for ms in ["120", "250", "400", "600", "900"] {
                let scene = card.shoot(&state, size, &[("POCKETUI_SNAPSHOT_LAUNCH_MS", ms)]);
                for y in bar_top(h)..h {
                    for x in 0..w {
                        let (a, b) = (scene.at(x, y), rest.at(x, y));
                        assert!((0..3).all(|k| a[k] <= b[k] + 2), "{look} {size} {ms} ms: something lit over the hint bar at {x},{y}: {a:?} on {b:?}");
                    }
                }
            }
        }
    }
}

#[test]
fn the_console_stands_on_a_soft_shadow() {
    // In the Cabinet, where it stands on a shelf board; on the Shelf look's black floor a shadow cannot show.
    let card = Card::new();
    for size in SIZES {
        let (w, h) = dims(size);
        let state = "shelf:snes:cabinet:games:2";
        let rest = card.shoot(state, size, &[]);
        let scene = card.shoot(state, size, &[("POCKETUI_SNAPSHOT_LAUNCH_MS", "900")]);
        let shine = scene.kept(&rest, (0, 0, w, h / 5), true).expect("a lit header");
        // Just under the console's foot, in the middle: darker than the room's own dimming says.
        let layout_h = if h <= 720 { 960 } else { h };
        let foot = ((layout_h - 138) as f32 * k(h)).round() as i32;
        let under = (w * 2 / 5, foot + (2.0 * k(h)).ceil() as i32, w * 3 / 5, foot + (9.0 * k(h)).ceil() as i32);
        let kept = scene.kept(&rest, under, false).expect("a floor under the console");
        assert!(kept < shine * 0.8, "{size}: no shadow under the console: the floor keeps {kept:.2} where the room keeps {shine:.2}");
    }
}

#[test]
fn every_spine_of_the_systems_shelf_has_one_kind_of_name() {
    let card = Card::new().with_more_systems();
    for size in SIZES {
        // The shelf scrolls: its first, middle and last systems are on screen in turn.
        let mut titles: Vec<String> = Vec::new();
        for at in ["atari2600", "snes", "xbox"] {
            let shelf = card.shoot(&format!("shelf:{at}:shelf"), size, &[]);
            // The systems' own spines are the "Keep" cases (a recent game's is its console's case).
            titles.extend(shelf.spines.lines().filter_map(|l| l.split('\t').collect::<Vec<_>>().get(3).filter(|c| **c == "Keep").and(l.split('\t').next())).filter(|t| !t.is_empty()).map(str::to_string));
        }
        for t in &titles {
            assert!(!t.contains('(') && !t.contains("experimental"), "{size}: a note on a spine: {t}");
            assert!(t.chars().count() <= 14, "{size}: a long name on a spine: {t}");
            for long in ["Nintendo ", "Sony ", "NEC ", "Bandai ", "PlayStation ", "Game Boy ", "Sega Master", "Sega Saturn", "Sega Dream"] {
                assert!(!t.starts_with(long), "{size}: the long name {t} beside the short ones: {titles:?}");
            }
        }
        for want in ["GBC", "GBA", "PS3", "3DS", "Master System", "Dreamcast", "Saturn", "Sega CD", "SNES"] {
            assert!(titles.iter().any(|t| t == want), "{size}: no spine {want}: {titles:?}");
        }
    }
}

#[test]
fn a_battery_with_nothing_measured_says_only_its_percent() {
    // The pause menu's line is made of `battery_games::Left`: until a rate is measured it is the charge alone.
    use pocket_ui::battery_games::Left;
    assert_eq!(Left::Measuring(83).line("GBA"), "83%");
    assert!(!Left::Measuring(5).line("GBA").contains("enough play"));
}
