//! Settings' row of category symbols stays put (the hand test of 0.5.0 on the RG28XX, 16.11.): on "Look" the
//! selected tab named itself beside its symbol, on "Controls & sound" the name didn't fit there and became a title over
//! the row, so the row and every setting under it slid down while L1/R1 paged through the categories. Now a screen,
//! language, style and text size have one layout for every category (`App::paint_settings_header`).
//!
//! The real program paints each category's list and, separately, the same with the focus on the symbols; the navigation
//! record (`POCKETUI_NAV_PROBE`, every string and mark with its place) says where L1's key and the first row are. They
//! must be the same on every category. The same record checks that a row's value stands on its name's baseline.

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

const SCREENS: [&str; 6] = ["640x480", "720x720", "1024x768", "1280x720", "1240x1080", "1920x1080"];
const LANGS: [&str; 6] = ["en", "de", "fr", "es", "it", "pt"];
/// Barlow, and Retro (its pixel font).
const STYLES: [(&str, &str); 2] = [("off", "barlow"), ("on", "retro")];
const SIZES: [&str; 2] = ["normal", "large"];
/// How many categories `SETTINGS_GROUPS` in main.rs has (read from its type, so a new category is checked too); a
/// category a device has no row in falls back to the first (Android's Connections).
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

/// Where L1's key is and where the focus mark is (the selected first row in a list, the selected symbol on the tabs):
/// their tops.
fn header_and_focus(placed: &[Placed]) -> (i32, i32) {
    let l1 = placed.iter().find(|(_, s)| s.starts_with("‹nav:hint›L1\t")).map(|(r, _)| r.1).unwrap_or(-1);
    let focus = placed.iter().filter(|(_, s)| s == "‹nav:focus›").map(|(r, _)| r.1).min().unwrap_or(-1);
    (l1, focus)
}

/// Rows whose value stands lower or higher than its name: the value's ink ends on another line than the name's (only
/// strings without descenders, whose ink ends on the baseline).
fn misaligned(placed: &[Placed], w: i32) -> Vec<String> {
    // Words whose ink ends on the baseline: letters and figures without a descender (Barlow's Q has a tail), no
    // drawn angles or arrows (they sit on the middle of the line, not on it).
    let plain = |s: &str| s.chars().any(char::is_alphanumeric)
        && s.chars().all(|c| (c.is_alphanumeric() || " %·'-".contains(c)) && !"gjpqyQçÇąęŞş".contains(c));
    // The rows only: from the selected first row down to the hints' pill.
    let top = placed.iter().filter(|(_, s)| s == "‹nav:focus›").map(|(r, _)| r.1).min().unwrap_or(0);
    let bottom = placed.iter().filter(|(_, s)| s.starts_with("‹hints")).map(|(r, _)| r.1).min().unwrap_or(i32::MAX);
    let words: Vec<&Placed> = placed.iter().filter(|(r, s)| !s.starts_with('‹') && r.3 > r.1 && !s.trim().is_empty() && r.1 >= top && r.3 <= bottom).collect();
    let mut bad = Vec::new();
    for (v, vs) in words.iter().filter(|(r, _)| r.0 > w / 2).map(|p| (&p.0, &p.1)) {
        // The row's name: on the left, overlapping the value's height the most.
        let label = words.iter().filter(|(r, _)| r.2 < w / 2).map(|p| (&p.0, &p.1))
            .max_by_key(|(r, _)| v.3.min(r.3) - v.1.max(r.1));
        let Some((l, ls)) = label else { continue };
        if v.3.min(l.3) - v.1.max(l.1) <= 0 || !plain(vs) || !plain(ls) {
            continue;
        }
        if (v.3 - l.3).abs() > 1 {
            bad.push(format!("\"{ls}\" ends at {} but its value \"{vs}\" at {}", l.3, v.3));
        }
    }
    bad
}

struct Card {
    dir: tempfile::TempDir,
    _stub: Option<Stub>,
}

impl Card {
    fn new() -> Card {
        let dir = tempfile::Builder::new().prefix("pui-tabbar-").tempdir_in("/tmp").unwrap();
        let ok = Command::new("python3").env("POCKETUI_MOCK_NOW", FIXTURE_NOW).args(["-I", &format!("{ROOT}/scripts/curator-fake-library.py"), dir.path().to_str().unwrap(), "dark"]).stdout(std::process::Stdio::null()).status().unwrap();
        assert!(ok.success());
        // The status pill's battery, as the RG28XX had it; and a folder with none.
        std::fs::create_dir_all(dir.path().join("battery")).unwrap();
        std::fs::create_dir_all(dir.path().join("no-battery")).unwrap();
        std::fs::write(dir.path().join("battery/capacity"), "24\n").unwrap();
        std::fs::write(dir.path().join("battery/status"), "Charging\n").unwrap();
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

/// Every category of one screen, language, style and size, with and without the battery in the status pill ("24%,
/// charging" made the RG28XX's line too narrow for "Controls & sound"): L1 and the first row at one place, the tabs' L1
/// and focus at one place, and every value on its name's baseline. `screen` None: Android's handheld view (Odin 2 Mini).
fn check(card: &Card, screen: Option<&str>, lang: &str, (retro, font): (&str, &str), size: &str) -> Vec<String> {
    let mut failures = Vec::new();
    let battery = card.dir.path().join("battery");
    let nobattery = card.dir.path().join("no-battery");
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
    let (mut lists, mut tabs): (Vec<(String, (i32, i32))>, Vec<(String, (i32, i32))>) = (Vec::new(), Vec::new());
    for g in 0..groups() {
        let mut first_name: Option<(String, i32)> = None;
        // Without the battery once: the narrower pill must not change the layout either.
        let runs: &[(&str, &std::path::PathBuf)] = if g == 0 { &[("settings", &nobattery), ("settings", &battery), ("settings-tabs", &battery)] } else { &[("settings", &battery), ("settings-tabs", &battery)] };
        for &(page, bat) in runs {
            let mut env = env.clone();
            env.push(("POCKETUI_BATTERY", bat.to_str().unwrap()));
            let state = format!("{page}:{g}");
            let placed = paint(dir.path(), wl.as_deref(), &format!("{state}{suffix}"), &env);
            let mut at = header_and_focus(&placed);
            // With the focus on the tabs no row is selected: the first row's name says where the rows are.
            if page == "settings-tabs" {
                at.1 = first_name.as_ref().and_then(|(n, _)| placed.iter().find(|(_, s)| s == n)).map_or(-1, |(r, _)| r.1);
            }
            let state = format!("{state} {}", if bat == &battery { "charging" } else { "no battery" });
            if at.0 < 0 || at.1 < 0 {
                failures.push(format!("{what} {state}: no L1 key or no focus in the record"));
                continue;
            }
            if page == "settings" {
                lists.push((state.clone(), at));
                // The first row's name and its top, here in the list (the band's first name under the focus' top).
                let name = placed.iter().filter(|(r, s)| !s.starts_with('‹') && r.1 >= at.1 && r.0 < 200).min_by_key(|(r, _)| r.1);
                first_name = name.map(|(r, s)| (s.clone(), r.1));
                let w = placed.iter().map(|(r, _)| r.2).max().unwrap_or(0);
                for bad in misaligned(&placed, w) {
                    failures.push(format!("{what} {state}: {bad}"));
                }
            } else {
                // The same name at the same height as in the list (the same category: the same glyphs).
                let y = first_name.as_ref().map_or(-1, |n| n.1);
                if at.1 != y {
                    failures.push(format!("{what} {state}: the first row's name at {} with the focus on the tabs, at {y} in the list", at.1));
                }
                tabs.push((state, (at.0, 0)));
            }
        }
    }
    for seen in [&lists, &tabs] {
        if let Some((first, at)) = seen.first() {
            for (state, other) in &seen[1..] {
                if other != at {
                    failures.push(format!("{what}: L1 and the focus at {other:?} on {state}, at {at:?} on {first}"));
                }
            }
        }
    }
    if let (Some((_, list)), Some((_, tab))) = (lists.first(), tabs.first()) {
        if list.0 != tab.0 {
            failures.push(format!("{what}: L1 at {} in the list, at {} with the focus on the tabs", list.0, tab.0));
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
fn the_category_row_and_the_first_row_stay_put_on_every_category() {
    let card = Card::new();
    let mut combos = Vec::new();
    for screen in SCREENS {
        for lang in LANGS {
            for style in STYLES {
                for size in SIZES {
                    combos.push((Some(screen), lang, style, size));
                }
            }
        }
    }
    check_all(&card, combos);
}

#[test]
fn the_category_row_stays_put_in_androids_handheld_view() {
    let card = Card::new();
    let mut combos = Vec::new();
    for lang in LANGS {
        for style in STYLES {
            for size in SIZES {
                combos.push((None, lang, style, size));
            }
        }
    }
    check_all(&card, combos);
}
