//! A list that scrolls uses the screen down to its explanation (the TrimUI Brick's hand test, 17.11.2026): the pause menu's Options showed
//! five rows over a third of an empty screen, Settings → Look five rows over the room of two lines kept for a line that
//! stood beside the hint bar. The Options page kept five lines for every explanation, Settings two for any that did not
//! fit beside the hints.
//!
//! The real program paints the pages of the pause menu and every category of Settings, on every screen, in every language,
//! Barlow and Retro, Normal and Large text, with the selection on each row in turn. Where the list scrolls (`kit::scrollbar`
//! marks its window, `nav_probe::LIST`), nothing under the window (the explanation, the hint bar) reaches into it, and on
//! the row whose explanation is the tallest the space between the window and that explanation's first line is less than a
//! row (and `MARGIN`): the room kept under a list is never more than its tallest explanation takes.

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

const SCREENS: [&str; 7] = ["640x480", "720x720", "960x544", "1024x768", "1240x1080", "1280x720", "1920x1080"];
/// Every state of a page in every language is a lot of pictures: the longest languages everywhere, all six on the Brick.
const LONG_LANGS: [&str; 3] = ["en", "de", "fr"];
const LANGS: [&str; 6] = ["en", "de", "fr", "es", "it", "pt"];
/// Barlow, and Retro (its pixel font).
const STYLES: [(&str, &str); 2] = [("off", "barlow"), ("on", "retro")];
const SIZES: [&str; 2] = ["normal", "large"];

/// How many categories `SETTINGS_GROUPS` in main.rs has (as tests/settings_tabbar_stable.rs reads it).
fn groups() -> usize {
    let main = std::fs::read_to_string(format!("{ROOT}/src/main.rs")).unwrap();
    let decl = main.split("const SETTINGS_GROUPS: [(&str, &[SettingsRow]); ").nth(1).expect("SETTINGS_GROUPS in main.rs");
    decl.split(']').next().unwrap().trim().parse().expect("SETTINGS_GROUPS' length")
}

/// One painted string or mark: its box and the string.
type Placed = ((i32, i32, i32, i32), String);

fn paint(dir: &Path, wl: Option<&Path>, state: &str, env: &[(&str, &str)]) -> Vec<Placed> {
    let (png, probe) = (dir.join("shot.png"), dir.join("probe.tsv"));
    let _ = std::fs::remove_file(&probe);
    let bin = std::env::var_os("POCKETUI_NAV_BINARY").unwrap_or_else(|| env!("CARGO_BIN_EXE_pocket-ui").into());
    let mut cmd = Command::new(bin);
    cmd.args(["--snapshot", state, png.to_str().unwrap()])
        .env("POCKETUI_MOCK", "1").env("POCKETUI_SOUND_QUANTUM", "0")
        .env("POCKETUI_DATA", dir.join("data")).env("POCKETUI_ES_SYSTEMS", dir.join("es_systems.cfg"))
        .env("POCKETUI_NAV_PROBE", &probe);
    for (k, v) in env {
        cmd.env(k, v);
    }
    if let Some(wl) = wl {
        cmd.env("WAYLAND_DISPLAY", wl);
    }
    let out = cmd.output().unwrap();
    assert!(out.status.success(), "{state} {env:?}: {}", String::from_utf8_lossy(&out.stderr));
    std::fs::read_to_string(&probe).unwrap_or_default().lines().filter_map(|l| {
        let mut p = l.splitn(5, '\t');
        let mut n = || p.next()?.parse::<i32>().ok();
        let r = (n()?, n()?, n()?, n()?);
        Some((r, p.next().unwrap_or("").to_string()))
    }).collect()
}

/// The gap a page leaves under its rows may be this much more than a row: the space kept clear above the explanation.
const MARGIN: i32 = 24;

/// Under a list that scrolls: the window of its rows (`‹nav:list›`), a row's height (the focus mark's), and the first thing
/// under the window (the explanation's first line or the hint bar: strings and the bar that don't lie inside the
/// window): the window's bottom, the row, that thing's top and name. `Err` when something under it reaches into the rows.
fn under_list(placed: &[Placed]) -> Option<Result<(i32, i32, i32, String), String>> {
    let window = placed.iter().find(|(_, s)| s == "‹nav:list›").map(|(r, _)| *r)?;
    let row = placed.iter().filter(|(_, s)| s == "‹nav:focus›").map(|(r, _)| r.3 - r.1).max()?;
    let (top, bottom) = (window.1, window.3);
    let under = placed
        .iter()
        .filter(|(r, s)| {
            let text = !s.starts_with('‹') && !s.trim().is_empty() && r.3 > r.1;
            (s.starts_with("‹hints") || text) && r.3 > bottom && r.1 >= top
        })
        .min_by_key(|(r, _)| r.1)?;
    if under.0 .1 < bottom {
        return Some(Err(format!("\"{}\" at {}..{} reaches into the rows, which end at {bottom}", under.1, under.0 .1, under.0 .3)));
    }
    Some(Ok((bottom, row, under.0 .1, under.1.clone())))
}

struct Card {
    dir: tempfile::TempDir,
    _stub: Option<Stub>,
}

impl Card {
    fn new() -> Card {
        let dir = tempfile::Builder::new().prefix("pui-listspace-").tempdir_in("/tmp").unwrap();
        let ok = Command::new("python3").env("POCKETUI_MOCK_NOW", FIXTURE_NOW).args(["-I", &format!("{ROOT}/scripts/curator-fake-library.py"), dir.path().to_str().unwrap(), "dark"]).stdout(std::process::Stdio::null()).status().unwrap();
        assert!(ok.success());
        let stub = (cfg!(target_os = "linux") && std::env::var_os("WAYLAND_DISPLAY").is_none_or(|w| w.is_empty())).then(|| {
            let child = Command::new("python3").args(["-I", &format!("{ROOT}/scripts/wayland-stub.py"), dir.path().join("wl").to_str().unwrap()]).spawn().unwrap();
            test_clock::wait_for(|| dir.path().join("wl").exists(), std::time::Duration::from_secs(4), "Wayland stub listening");
            Stub(child)
        });
        Card { dir, _stub: stub }
    }

    fn wl(&self) -> Option<std::path::PathBuf> {
        let wl = self.dir.path().join("wl");
        wl.exists().then_some(wl)
    }
}

/// The pages of one screen, language, style and text size. `screen` None: Android's handheld view.
fn check(card: &Card, screen: Option<&str>, lang: &str, (retro, font): (&str, &str), size: &str) -> Vec<String> {
    let mut env = vec![("POCKETUI_LANG", lang), ("POCKETUI_RETRO", retro), ("POCKETUI_FONT", font), ("POCKETUI_TEXT_SIZE", size), ("POCKETUI_MOCK_NOW", FIXTURE_NOW)];
    let suffix = match screen {
        Some(s) => {
            env.push(("POCKETUI_SNAPSHOT_SIZE", s));
            ""
        }
        None => {
            env.push(("POCKETUI_ANDROID_PROFILE", "1"));
            ":device=odin2mini"
        }
    };
    let what = format!("{} {lang} {font} {size}", screen.unwrap_or("android"));
    let wl = card.wl();
    let dir = tempfile::Builder::new().prefix("run-").tempdir_in(card.dir.path()).unwrap();
    for name in ["data", "es_systems.cfg"] {
        std::os::unix::fs::symlink(card.dir.path().join(name), dir.path().join(name)).unwrap();
    }
    // Each page with the selection on every row in turn (the room under the rows is the tallest explanation's, kept for
    // every row so the list doesn't move): the pause menu's Options, Speed, rumble, fan (Earlier states' three lines), a
    // core's options, every category of Settings. A state past the last row shows the last row again.
    let mut pages: Vec<(String, Vec<String>)> = vec![
        ("options".into(), (0..10).map(|r| format!("options:{r}")).collect()),
        ("extras".into(), (0..10).map(|r| format!("extras:{r}")).collect()),
        ("emulator".into(), (0..12).map(|r| format!("emulator:{r}")).collect()),
    ];
    for g in 0..groups() {
        pages.push((format!("settings:{g}"), (0..14).map(|r| format!("settings:{g}:{r}")).collect()));
    }
    let mut failures = Vec::new();
    for (page, states) in pages {
        let mut tightest: Option<(i32, i32, String)> = None;
        let mut before: Option<Vec<Placed>> = None;
        for state in states {
            let placed = paint(dir.path(), wl.as_deref(), &format!("{state}{suffix}"), &env);
            // Past the last row (the same picture again, or no selection): the page is done.
            if before.as_ref() == Some(&placed) || !placed.iter().any(|(_, s)| s == "‹nav:focus›") {
                break;
            }
            match under_list(&placed) {
                Some(Err(bad)) => failures.push(format!("{what} {state}: {bad}")),
                Some(Ok((bottom, row, next, name))) => {
                    let gap = next - bottom - row;
                    if tightest.as_ref().is_none_or(|t| gap < t.0) {
                        tightest = Some((gap, row, format!("{state}: {} px between the last row (ends at {bottom}) and \"{name}\" at {next}, a row is {row}", next - bottom)));
                    }
                }
                None => {}
            }
            before = Some(placed);
        }
        if let Some((gap, _, how)) = tightest {
            if gap >= MARGIN {
                failures.push(format!("{what} {page}: room for another row under the list on every row; at its closest {how}"));
            }
        }
    }
    failures
}

/// `check` for every combination, four at a time.
fn check_all(card: &Card, combos: Vec<(Option<&'static str>, &'static str, (&'static str, &'static str), &'static str)>) {
    let next = std::sync::atomic::AtomicUsize::new(0);
    let failures = std::sync::Mutex::new(Vec::new());
    std::thread::scope(|s| {
        for _ in 0..4 {
            s.spawn(|| loop {
                let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                let Some(&(screen, lang, style, size)) = combos.get(i) else { break };
                let found = check(card, screen, lang, style, size);
                failures.lock().unwrap().extend(found);
            });
        }
    });
    let failures = failures.into_inner().unwrap();
    assert!(failures.is_empty(), "{} faults:\n{}", failures.len(), failures.join("\n"));
}

#[test]
fn a_list_that_scrolls_reaches_down_to_its_explanation_on_every_screen() {
    let card = Card::new();
    let mut combos = Vec::new();
    for screen in SCREENS {
        for lang in if screen == "1024x768" { &LANGS[..] } else { &LONG_LANGS[..] } {
            for style in STYLES {
                for size in SIZES {
                    combos.push((Some(screen), *lang, style, size));
                }
            }
        }
    }
    check_all(&card, combos);
}

#[test]
fn a_list_that_scrolls_reaches_down_to_its_explanation_in_androids_handheld_view() {
    let card = Card::new();
    let mut combos = Vec::new();
    for lang in LONG_LANGS {
        for style in STYLES {
            for size in SIZES {
                combos.push((None, lang, style, size));
            }
        }
    }
    check_all(&card, combos);
}
