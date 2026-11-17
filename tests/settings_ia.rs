//! Settings in categories (2026-10-30; the tree of 2026-11-13, SETTINGS-IA-DECISION; eight since Settings v2, 2026-11-16) as the player meets them: every choice is reached
//! by plain rows (START, a category, its row), nothing only on a button. Each category's list is rendered with each of
//! its rows selected, in English and German, at 1280x720 and at 640x480 with Large text (the H700 handhelds), and the
//! row's name must be painted whole on its row (`POCKETUI_VISIBLE_TEXT`) with no text cut by an edge or running into
//! other text (`POCKETUI_TEXT_BOXES`). The Buttons page shows which button confirms, and Systems on the shelf without
//! games says so instead of staying empty. START shows the first category's rows at once, the categories' symbols with
//! L1/R1 at the top (`the_categories_header_fits_at_large_text_in_german`); Cozy extras and Sleep & power show each row whole too. On the made-up library (scripts/curator-fake-library.py). Needs python3 (scripts/wayland-stub.py on Linux without a compositor).

use std::path::{Path, PathBuf};
use std::process::Command;

include!("common/root.rs");

/// The tree as the player sees it without a store catalog: category, then its rows. Every row is one of a category; no
/// row opens a list of more rows (For tinkerers is gone: Sync and PC streaming are rows of Connections).
const TREE: [(&str, &[&str]); 8] = [
    ("Connections", &["Wi-Fi", "Bluetooth", "Tailscale", "Remote access (SSH)", "Sync", "Web companion", "PC streaming"]),
    ("Look", &["Layout", "Appearance", "Colours", "Style", "Text size", "Motion", "Game start animation", "Cozy extras"]),
    ("Screen & sound", &["Brightness", "Volume", "Screen colour", "Night light", "Menu sounds", "Case sounds"]),
    ("Controls", &["Buttons", "Controller setup", "Test buttons", "Haptics", "LEDs"]),
    ("Playing", &["On start", "Continue where I quit", "Console boot screen", "Remind me where I was", "Screen frames", "Over the game", "Zoom in the pause menu", "Performance & fan", "RetroAchievements"]),
    ("Games", &["Add games", "Box art & downloads", "Game check", "Systems on the shelf", "Curator", "Screenshots", "From EmulationStation"]),
    ("System", &["Updates", "Language", "Sleep & power", "Battery", "Time", "Storage", "Backup & reset", "Kids mode"]),
    ("Help & tools", &["Manual", "Tour", "How you like to play", "First-start setup", "Tools", "Report a problem", "About"]),
];

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

/// The German line of `en` in assets/i18n/de.tsv (the Look category's key ends with a zero-width mark).
fn german(en: &str) -> String {
    let table = std::fs::read_to_string(format!("{ROOT}/assets/i18n/de.tsv")).unwrap();
    table
        .lines()
        .filter_map(|l| l.split_once('\t'))
        .find(|(k, _)| *k == en)
        .map(|(_, v)| v.to_string())
        .unwrap_or_else(|| en.to_string())
}

/// The Spanish line of `en` in assets/i18n/es.tsv (the Look category's key ends with a zero-width mark).
fn spanish(en: &str) -> String {
    let table = std::fs::read_to_string(format!("{ROOT}/assets/i18n/es.tsv")).unwrap();
    let en = if en == "Look" { "Look\u{200b}" } else { en };
    table.lines().filter_map(|l| l.split_once('\t')).find(|(k, _)| *k == en).map(|(_, v)| v.to_string()).unwrap_or_else(|| en.to_string())
}

/// The rows of Settings → Look → Cozy extras and System → Sleep & power, in their order.
const COZY: [&str; 13] = ["TV on the cabinet", "TV plays the game", "Stickers on cases", "Printed discs", "Playful cases", "Sealed new games", "Old-TV switch-on", "Light by time of day", "Decorations", "Progress from saves", "Game connections", "Rediscover", "GBA console"];
const SLEEP: [&str; 5] = ["Screen off after", "Sleep after", "Switch off after sleeping", "Short press", "Hold"];

/// The symbols, the name and L1/R1 at the top of category `g`'s list (`settings:<g>`, its first row): the name painted whole
/// (in the header line or, where the line doesn't fit, as the title over a slim line of symbols), or in the header line its
/// short name, the part before " & " (`tab_label`), never cut; L1 and R1 shown, no text cut by an edge or running into
/// other text.
#[allow(clippy::too_many_arguments)]
fn header(tmp: &Path, stub: &Option<Stub>, g: usize, lang: &str, size: &str, text: &str, tr: &dyn Fn(&str) -> String, failed: &mut Vec<String>) {
    let (cat, rows) = TREE[g];
    let page = shot(tmp, stub, &format!("settings:{g}"), lang, size, text, true);
    let name = if cat == "Look" && lang == "de" { german("Look\u{200b}") } else { tr(cat) };
    let short = cat.split(" & ").next().filter(|s| *s != cat).map(tr);
    if !page.visible.iter().any(|v| v.trim_end_matches('\u{200b}') == name || Some(v.as_str()) == short.as_deref()) {
        failed.push(format!("{lang} {size} {text} settings:{g}: the name {name:?} isn't shown whole: {:?}", page.visible));
    }
    for key in ["L1", "R1"] {
        if !page.visible.iter().any(|v| v == key) {
            failed.push(format!("{lang} {size} {text} settings:{g}: no {key} at the top: {:?}", page.visible));
        }
    }
    let first = tr(rows[0]);
    if !page.visible.iter().any(|v| *v == first) {
        failed.push(format!("{lang} {size} {text} settings:{g}: its first row {first:?} isn't shown: {:?}", page.visible));
    }
    failed.extend(page.faults.iter().map(|f| format!("{lang} {size} {text} settings:{g}: {f}")));
}

/// What one snapshot painted: the visible strings and the text-box faults (`E` cut, `O` overlapping).
struct Shot {
    visible: Vec<String>,
    faults: Vec<String>,
}

fn shot(tmp: &Path, stub: &Option<Stub>, state: &str, lang: &str, size: &str, text: &str, games: bool) -> Shot {
    shot_with(tmp, stub, state, lang, size, text, games, r#"{"onboarded": true}"#, "")
}

/// `shot` with `settings` as settings.json, its pictures named with `tag` besides.
#[allow(clippy::too_many_arguments)]
fn shot_with(tmp: &Path, stub: &Option<Stub>, state: &str, lang: &str, size: &str, text: &str, games: bool, settings: &str, tag: &str) -> Shot {
    let name = format!("{}-{lang}-{size}-{text}{tag}", state.replace(':', "_"));
    let data = tmp.join(&name);
    std::fs::create_dir_all(&data).unwrap();
    std::fs::write(data.join("settings.json"), settings).unwrap();
    let (visible, boxes) = (tmp.join(format!("{name}.txt")), tmp.join(format!("{name}.tsv")));
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
    cmd.args(["--snapshot", state, tmp.join(format!("{name}.png")).to_str().unwrap()])
        .env("POCKETUI_MOCK", "1").env("POCKETUI_MOCK_NOW", FIXTURE_NOW)
        .env("POCKETUI_LANG", lang)
        .env("POCKETUI_TEXT_SIZE", text)
        .env("POCKETUI_SNAPSHOT_SIZE", size)
        .env("POCKETUI_VISIBLE_TEXT", &visible)
        .env("POCKETUI_TEXT_BOXES", &boxes)
        .env("POCKETUI_DATA", &data);
    // The made-up library (scripts/curator-fake-library.py), so the rows' values are as long as a real one's; or none.
    cmd.env("POCKETUI_ES_SYSTEMS", if games { tmp.join("lib/es_systems.cfg") } else { tmp.join("none.cfg") });
    if let Some(s) = stub {
        cmd.env("WAYLAND_DISPLAY", &s.1);
    }
    let out = cmd.output().expect("run pocket-ui");
    assert!(out.status.success(), "{state}: {}", String::from_utf8_lossy(&out.stderr));
    let read = |p: &Path| std::fs::read_to_string(p).unwrap_or_default();
    Shot {
        visible: read(&visible).lines().map(str::to_string).collect(),
        // The hint bar giving way on a narrow screen is by design (`kit::pill_hints`); text cut or run together is not.
        faults: read(&boxes).lines().filter(|l| l.starts_with('E') || l.starts_with('O')).map(str::to_string).collect(),
    }
}

#[test]
fn every_choice_is_a_row_and_shows_whole() {
    let dir = tempfile::Builder::new().prefix("pocketui-settings-ia-").tempdir().unwrap();
    let tmp = dir.path().to_path_buf();
    std::fs::write(tmp.join("none.cfg"), "<systemList></systemList>").unwrap();
    let ok = Command::new("python3").env("POCKETUI_MOCK_NOW", FIXTURE_NOW)
        .args(["-I", &format!("{ROOT}/scripts/curator-fake-library.py"), tmp.join("lib").to_str().unwrap(), "dark"])
        .stdout(std::process::Stdio::null())
        .status()
        .expect("python3 for scripts/curator-fake-library.py");
    assert!(ok.success());
    let stub = wayland(&tmp);
    let mut failed = Vec::new();
    for (lang, size, text) in [("en", "1280x720", "normal"), ("de", "1280x720", "normal"), ("de", "640x480", "large")] {
        let tr = |s: &str| if lang == "de" { german(if s == "Look" { "Look\u{200b}" } else { s }) } else { s.to_string() };
        // START shows the first category's rows at once (SETTINGS-IA package 2); every category's list has the symbols at
        // the top with its name whole and L1 and R1 drawn as keys, nothing cut or run together.
        for g in 0..TREE.len() {
            header(&tmp, &stub, g, lang, size, text, &tr, &mut failed);
        }
        // Cozy extras and Sleep & power: each row whole when selected (9 rows at Large text on 640x480 scroll, as every
        // list of 9 does), its line never cut.
        for (state, rows) in [("cozy", &COZY[..]), ("sleep", &SLEEP[..])] {
            for (r, row) in rows.iter().enumerate() {
                let s = shot(&tmp, &stub, &format!("{state}:{r}"), lang, size, text, true);
                if !s.visible.iter().any(|v| *v == tr(row)) {
                    failed.push(format!("{lang} {size} {text} {state}:{r}: row {row:?} not shown whole: {:?}", s.visible));
                }
                if s.visible.iter().any(|v| v.ends_with('…')) {
                    failed.push(format!("{lang} {size} {text} {state}:{r} ({row}): a line is cut: {:?}", s.visible));
                }
                failed.extend(s.faults.iter().map(|f| format!("{lang} {size} {text} {state}:{r}: {f}")));
            }
        }
        let sp = shot(&tmp, &stub, &format!("cozy:{}::a", COZY.len() - 1), lang, size, text, true);
        if !sp.visible.iter().any(|line| line.contains("Game Boy Advance SP")) {
            failed.push(format!("{lang} {size} {text}: the full selected SP model name must be readable"));
        }
        failed.extend(sp.faults.iter().map(|f| format!("{lang} {size} {text} GBA SP choice: {f}")));
        for (g, (cat, rows)) in TREE.iter().copied().enumerate() {
            for (r, row) in rows.iter().enumerate() {
                let state = format!("settings:{g}:{r}");
                let s = shot(&tmp, &stub, &state, lang, size, text, true);
                let (cat_name, row_name) = (if cat == "Look" { german("Look\u{200b}") } else { tr(cat) }, if lang == "de" { german(row) } else { row.to_string() });
                let cat_name = if lang == "de" { cat_name } else { cat.to_string() };
                // Or its short name beside its symbol (the part before " & ", `tab_label`) where the whole one doesn't fit.
                let short = cat.split(" & ").next().filter(|s| *s != cat).map(|s| if lang == "de" { german(s) } else { s.to_string() });
                if !s.visible.iter().any(|v| v.trim_end_matches('\u{200b}') == cat_name || Some(v.as_str()) == short.as_deref()) {
                    failed.push(format!("{lang} {size} {text} {state}: title {cat_name:?} not shown"));
                }
                if !s.visible.iter().any(|v| *v == row_name) {
                    failed.push(format!("{lang} {size} {text} {state}: row {row_name:?} not shown whole: {:?}", s.visible));
                }
                // The row's line at the bottom is never cut short.
                if s.visible.iter().any(|v| v.ends_with('…') && !v.starts_with('…')) {
                    failed.push(format!("{lang} {size} {text} {state}: a line is cut: {:?}", s.visible));
                }
                failed.extend(s.faults.iter().map(|f| format!("{lang} {size} {text} {state}: {f}")));
            }
        }
        // Controls → Buttons: both rows, and which button confirms, in words and on the picture.
        for state in ["buttons:0", "buttons:1:r"] {
            let s = shot(&tmp, &stub, state, lang, size, text, true);
            for want in ["Button layout", "Button symbols", "Confirm", "Back"] {
                if !s.visible.iter().any(|v| *v == tr(want)) {
                    failed.push(format!("{lang} {size} {text} {state}: {want:?} not shown: {:?}", s.visible));
                }
            }
            failed.extend(s.faults.iter().map(|f| format!("{lang} {size} {text} {state}: {f}")));
        }
        // Systems on the shelf without any games: a line says so, the page is never empty.
        let s = shot(&tmp, &stub, "setup:look-systems", lang, size, text, false);
        let empty = tr("No systems with games yet. Once games are on the SD card, their systems show up here.");
        let first_word = empty.split_whitespace().next().unwrap().to_string();
        if !s.visible.iter().any(|v| v.starts_with(&first_word)) {
            failed.push(format!("{lang} {size} {text} setup:look-systems without games: no line saying so: {:?}", s.visible));
        }
    }
    assert!(failed.is_empty(), "{} problem(s):\n{}", failed.len(), failed.join("\n"));
}

/// The symbols at the top fit, with the longest German names at Large text, on the small screen and at the Brick's
/// 1024x768: in one line where there is room, else the name over a slim line of symbols (SETTINGS-IA package 2).
#[test]
fn the_categories_header_fits_at_large_text_in_german() {
    let dir = tempfile::Builder::new().prefix("pocketui-settings-header-").tempdir().unwrap();
    let tmp = dir.path().to_path_buf();
    std::fs::write(tmp.join("none.cfg"), "<systemList></systemList>").unwrap();
    let ok = Command::new("python3").env("POCKETUI_MOCK_NOW", FIXTURE_NOW)
        .args(["-I", &format!("{ROOT}/scripts/curator-fake-library.py"), tmp.join("lib").to_str().unwrap(), "dark"])
        .stdout(std::process::Stdio::null())
        .status()
        .expect("python3 for scripts/curator-fake-library.py");
    assert!(ok.success());
    let stub = wayland(&tmp);
    let mut failed = Vec::new();
    for (lang, size, text) in [("de", "640x480", "large"), ("de", "1024x768", "large"), ("es", "640x480", "large"), ("en", "1280x720", "normal")] {
        let tr = |s: &str| match lang {
            "de" => german(s),
            "es" => spanish(s),
            _ => s.to_string(),
        };
        for g in 0..TREE.len() {
            header(&tmp, &stub, g, lang, size, text, &tr, &mut failed);
        }
    }
    assert!(failed.is_empty(), "{} problem(s):\n{}", failed.len(), failed.join("\n"));
}

/// Settings › Look › Style in Retro (0.3.2): its value, "Retro" in Departure Mono or "Retro Pixelify", shows whole when
/// selected and the Style row with it, in Large German on the small screen and the Brick's, in both of Retro's letters.
#[test]
fn the_style_rows_retro_values_show_whole_in_both_letters() {
    let dir = tempfile::Builder::new().prefix("pocketui-settings-ia-retro-").tempdir().unwrap();
    let tmp = dir.path().to_path_buf();
    std::fs::write(tmp.join("none.cfg"), "<systemList></systemList>").unwrap();
    let stub = wayland(&tmp);
    let look = TREE.iter().position(|c| c.0 == "Look").unwrap();
    let style = TREE[look].1.iter().position(|r| *r == "Style").unwrap();
    let mut failed = Vec::new();
    for (letters, value) in [("departure", "Retro"), ("pixelify", "Retro Pixelify")] {
        let settings = format!(r#"{{"onboarded": true, "font": "retro", "retro_letters": "{letters}"}}"#);
        for (lang, size, text) in [("de", "640x480", "large"), ("de", "1024x768", "large"), ("es", "640x480", "large"), ("en", "1280x960", "normal")] {
            let s = shot_with(&tmp, &stub, &format!("settings:{look}:{style}"), lang, size, text, false, &settings, &format!("-{letters}"));
            let row = if lang == "en" { "Style".to_string() } else if lang == "de" { german("Style") } else { "Estilo".to_string() };
            // The value whole, its arrows beside it (Retro paints them apart, as pixel arrows).
            if !s.visible.iter().any(|v| *v == value || *v == format!("‹ {value} ›")) || !s.visible.iter().any(|v| v.contains('‹')) {
                failed.push(format!("{letters} {lang} {size} {text}: \"{value}\" not whole beside its arrows: {:?}", s.visible));
            }
            if !s.visible.iter().any(|v| *v == row) {
                failed.push(format!("{letters} {lang} {size} {text}: row {row:?} not whole: {:?}", s.visible));
            }
            if s.visible.iter().any(|v| v.ends_with('…') && !v.starts_with('…')) {
                failed.push(format!("{letters} {lang} {size} {text}: a line is cut: {:?}", s.visible));
            }
            failed.extend(s.faults.iter().map(|f| format!("{letters} {lang} {size} {text}: {f}")));
        }
    }
    assert!(failed.is_empty(), "{} problem(s):\n{}", failed.len(), failed.join("\n"));
}

/// What the player typed is painted as it is, also in German: a search for "unknown" shows "unknown" in its field
/// (it showed "unbekannt", the German table's word, while the line under it said "unknown"), and a network called
/// "Unknown" keeps its name on the Wi-Fi page. The keyboard's own field was already painted untranslated.
#[test]
fn typed_text_is_painted_as_typed() {
    let dir = tempfile::Builder::new().prefix("pocketui-typed-text-").tempdir().unwrap();
    let tmp = dir.path().to_path_buf();
    std::fs::write(tmp.join("none.cfg"), "<systemList></systemList>").unwrap();
    let ok = Command::new("python3").env("POCKETUI_MOCK_NOW", FIXTURE_NOW)
        .args(["-I", &format!("{ROOT}/scripts/curator-fake-library.py"), tmp.join("lib").to_str().unwrap(), "dark"])
        .stdout(std::process::Stdio::null())
        .status()
        .expect("python3 for scripts/curator-fake-library.py");
    assert!(ok.success());
    let stub = wayland(&tmp);
    assert_eq!(german("unknown"), "unbekannt", "the word is in the German table, so the test means something");
    let s = shot(&tmp, &stub, "search:results:unknown", "de", "1240x1080", "normal", true);
    assert!(s.visible.iter().any(|v| v == "unknown"), "the search field shows what was typed: {:?}", s.visible);
    assert!(!s.visible.iter().any(|v| v.contains("unbekannt")), "nothing typed is translated: {:?}", s.visible);
    let s = shot(&tmp, &stub, "setup:wifi-named=Unknown", "de", "1240x1080", "normal", true);
    assert!(s.visible.iter().any(|v| v == "Unknown"), "a network keeps its name: {:?}", s.visible);
    assert!(!s.visible.iter().any(|v| v == "Unbekannt"), "{:?}", s.visible);
}

/// Every row that opens a page says so when it is selected: Kids mode's row showed nothing at its right end (the size
/// sweep, 2026-11-02), while Updates, Storage and the others show ">".
#[test]
fn the_selected_kids_mode_row_shows_that_it_opens_a_page() {
    let dir = tempfile::Builder::new().prefix("pocketui-settings-kids-").tempdir().unwrap();
    let tmp = dir.path().to_path_buf();
    std::fs::write(tmp.join("none.cfg"), "<systemList></systemList>").unwrap();
    let stub = wayland(&tmp);
    for lang in ["en", "de", "es", "pt", "fr", "it"] {
        let kids = TREE[6].1.iter().position(|r| *r == "Kids mode").unwrap();
        let s = shot(&tmp, &stub, &format!("settings:6:{kids}"), lang, "1280x720", "normal", false);
        assert!(s.visible.iter().any(|v| v.trim() == ">"), "{lang}: no \">\" on the selected Kids mode row: {:?}", s.visible);
    }
}

/// Walk 2026-11-02: Continue where I quit said "Quit keeps the moment for the next start." also while it was Off.
#[test]
fn the_line_under_continue_where_i_quit_says_what_off_does() {
    let dir = tempfile::Builder::new().prefix("pocketui-settings-remember-").tempdir().unwrap();
    let tmp = dir.path().to_path_buf();
    std::fs::write(tmp.join("none.cfg"), "<systemList></systemList>").unwrap();
    let stub = wayland(&tmp);
    for (forget, want, not) in [(false, "Quit keeps the moment for the next start.", "Quit keeps no moment"), (true, "Quit keeps no moment: the next start is fresh", "Quit keeps the moment for the next start.")] {
        let data = tmp.join(format!("data-{forget}"));
        std::fs::create_dir_all(&data).unwrap();
        std::fs::write(data.join("settings.json"), format!(r#"{{"onboarded": true, "forget_where_quit": {forget}}}"#)).unwrap();
        let visible = tmp.join(format!("{forget}.txt"));
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
        cmd.args(["--snapshot", "settings:4:1", tmp.join(format!("{forget}.png")).to_str().unwrap()])
            .env("POCKETUI_MOCK", "1").env("POCKETUI_MOCK_NOW", FIXTURE_NOW)
            .env("POCKETUI_LANG", "en")
            .env("POCKETUI_VISIBLE_TEXT", &visible)
            .env("POCKETUI_DATA", &data)
            .env("POCKETUI_ES_SYSTEMS", tmp.join("none.cfg"));
        if let Some(s) = &stub {
            cmd.env("WAYLAND_DISPLAY", &s.1);
        }
        assert!(cmd.output().expect("run pocket-ui").status.success());
        let v = std::fs::read_to_string(&visible).unwrap();
        let flat = v.replace('\n', " ");
        assert!(flat.contains("Continue where I quit"), "the Playing page: {v}");
        assert!(flat.contains(want) && !flat.contains(not), "forget={forget}: {v}");
    }
}
