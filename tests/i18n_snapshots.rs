//! The German interface against the snapshot suite (docs/DEVELOPING.md "Translations"): every state of
//! `tests/i18n-states.txt` is rendered in German with scripts/curator-fake-library.py's made-up library, and every
//! string painted without a translation fails the test unless it is
//! - no words (numbers, sizes, times, symbols),
//! - a piece of a translation (a line of a wrapped one, one cut short with "…"),
//! - data that stays English on purpose: the console fact sheets and credits (`assets/data/*.json`), or
//! - in `assets/i18n/keep.txt` (names: games, systems, emulators, brands, people; `re:` lines are patterns).
//!
//! Needs python3 for the made-up library (as scripts/curator-shots.sh) and, on Linux without a compositor, for
//! scripts/wayland-stub.py. About 250 snapshots, eight at a time.

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};

const ROOT: &str = env!("CARGO_MANIFEST_DIR");

fn states() -> Vec<(bool, String)> {
    std::fs::read_to_string(Path::new(ROOT).join("tests/i18n-states.txt"))
        .expect("tests/i18n-states.txt")
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| match l.strip_prefix("safe ") {
            Some(s) => (true, s.to_string()),
            None => (false, l.to_string()),
        })
        .filter(|(_, state)| {
            std::env::var_os("POCKETUI_TEST_QUICK").is_none()
                || matches!(state.as_str(), "settings" | "settings:0:2" | "shelf:gb:split" | "sys:quick"
                    | "sys:power" | "tailscale" | "achievements" | "search:results:mario" | "notice:bios"
                    | "setup:wifi" | "menu:pocket-core" | "storage")
                || std::env::var("POCKETUI_TEST_STATES").is_ok_and(|extra| extra.lines().any(|s| s == state))
        })
        .collect()
}

/// keep.txt: exact names and `re:` patterns (a small subset: `^`, `$`, `.*`, literal text; see `keep_match`).
fn keep() -> (HashSet<String>, Vec<String>) {
    let text = std::fs::read_to_string(Path::new(ROOT).join("assets/i18n/keep.txt")).expect("assets/i18n/keep.txt");
    let mut exact = HashSet::new();
    let mut patterns = Vec::new();
    for line in text.lines() {
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        match line.strip_prefix("re:") {
            Some(p) => patterns.push(p.to_string()),
            None => {
                exact.insert(line.trim().to_string());
            }
        }
    }
    (exact, patterns)
}

/// keep.txt's `re:` lines, a small pattern language (scripts/i18n-extract.py reads it the same way): `^` and `$`
/// anchor, `.*` is any text, `\d` one or more digits, `\` before any other character takes it literally (`\.`,
/// `\+`, `\\`); every other character is itself.
fn keep_match(pattern: &str, s: &str) -> bool {
    fn at(p: &str, s: &str) -> bool {
        if p.is_empty() {
            return true;
        }
        if p == "$" {
            return s.is_empty();
        }
        if let Some(rest) = p.strip_prefix(".*") {
            return (0..=s.len()).filter(|i| s.is_char_boundary(*i)).any(|i| at(rest, &s[i..]));
        }
        if let Some(rest) = p.strip_prefix("\\d") {
            let n = s.bytes().take_while(u8::is_ascii_digit).count();
            return n > 0 && at(rest, &s[n..]);
        }
        let p = p.strip_prefix('\\').filter(|r| !r.is_empty()).unwrap_or(p);
        let ch = p.chars().next().unwrap();
        s.starts_with(ch) && at(&p[ch.len_utf8()..], &s[ch.len_utf8()..])
    }
    match pattern.strip_prefix('^') {
        Some(p) => at(p, s),
        None => (0..=s.len()).filter(|i| s.is_char_boundary(*i)).any(|i| at(pattern, &s[i..])),
    }
}

/// Every string value in the data files that stay English (console fact sheets, credits), but for the short roles
/// of the credits ("Manufacturer", "Sound"): those are labels of the page and translated.
fn english_data() -> Vec<String> {
    fn walk(v: &serde_json::Value, out: &mut Vec<String>) {
        match v {
            serde_json::Value::String(s) => out.push(s.clone()),
            serde_json::Value::Array(a) => a.iter().for_each(|v| walk(v, out)),
            serde_json::Value::Object(o) => o.iter().for_each(|(k, v)| {
                out.push(k.clone());
                if !(k == "role" && v.as_str().is_some_and(|r| r.split_whitespace().count() <= 2)) {
                    walk(v, out)
                }
            }),
            _ => {}
        }
    }
    let mut out = Vec::new();
    for f in ["assets/data/consoles.json", "assets/data/console-credits.json"] {
        let text = std::fs::read_to_string(Path::new(ROOT).join(f)).expect(f);
        walk(&serde_json::from_str(&text).expect(f), &mut out);
    }
    out
}

/// Whether `s` has words to translate: two letters in a row, not counting units and abbreviations that read the same
/// in German ("60.00 Hz", "2 h 00 min", "UTC-04:00").
fn words(s: &str) -> bool {
    const UNITS: [&str; 22] = ["Hz", "kHz", "MHz", "GHz", "ms", "fps", "FPS", "KB", "MB", "GB", "TB", "rpm", "min", "px", "mA", "mAh", "UTC", "CPU", "GPU", "RAM", "VRAM", "dB"];
    s.split(|c: char| !c.is_alphanumeric())
        .filter(|w| !UNITS.contains(w))
        .any(|w| w.chars().collect::<Vec<_>>().windows(2).any(|p| p[0].is_alphabetic() && p[1].is_alphabetic()))
}

/// Names the made-up library and the computer's time zones bring: ROM file names without their extension, the zones
/// as the time pages show them, and the comments of zone.tab.
fn names_from_data(tmp: &Path) -> Vec<String> {
    let mut out = Vec::new();
    if let Ok(systems) = std::fs::read_dir(tmp.join("roms")) {
        for sys in systems.flatten() {
            for f in std::fs::read_dir(sys.path()).into_iter().flatten().flatten() {
                let name = f.file_name().to_string_lossy().into_owned();
                let stem = name.rsplit_once('.').map_or(name.clone(), |(stem, _)| stem.to_string());
                // And as the shelf shows it (`library::display_name`: "Legend of Zelda, The - …" is "The Legend of
                // Zelda - …"), and without the tags: "Phantasy Star IV (USA)" is "Phantasy Star IV".
                out.push(pocket_ui::library::display_name(&stem));
                out.push(stem.split(" (").next().unwrap_or(&stem).split(" [").next().unwrap_or(&stem).trim().to_string());
                out.push(stem);
            }
        }
    }
    // What the player's EmulationStation gamelists say (`gamelist.rs`): names, descriptions, developers and genres are
    // their own text, in whatever language they scraped it, and shown as it is.
    if let Ok(systems) = std::fs::read_dir(tmp.join("roms")) {
        for sys in systems.flatten() {
            let Ok(bytes) = std::fs::read(sys.path().join("gamelist.xml")) else { continue };
            for e in pocket_ui::gamelist::parse(&pocket_ui::gamelist::decode(&bytes), &sys.path()) {
                out.extend([e.name, e.desc, e.developer, e.publisher, e.genre, e.players]);
            }
        }
    }
    // The systems, by the names es_systems.cfg gives them.
    let cfg = std::fs::read_to_string(tmp.join("es_systems.cfg")).unwrap_or_default();
    for tag in ["name", "fullname"] {
        for part in cfg.split(&format!("<{tag}>")).skip(1) {
            if let Some((v, _)) = part.split_once(&format!("</{tag}>")) {
                out.push(v.trim().to_string());
            }
        }
    }
    // The time zone pages list the computer's zones the way the device lists its own (`timezone::list`, `city_of`).
    for zone in pocket_ui::timezone::list(Path::new("/usr/share/zoneinfo"), Path::new("/usr/config/system/configs/tz")) {
        out.push(pocket_ui::timezone::city_of(&zone));
        out.push(pocket_ui::timezone::region_of(&zone).to_string());
        out.push(zone);
    }
    for tab in ["/usr/share/zoneinfo/zone.tab", "/usr/share/zoneinfo/zone1970.tab"] {
        for line in std::fs::read_to_string(tab).unwrap_or_default().lines().filter(|l| !l.starts_with('#')) {
            if let Some(comment) = line.split('\t').nth(3) {
                out.push(comment.to_string());
            }
        }
    }
    out
}

/// scripts/wayland-stub.py, while it runs.
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

/// A snapshot opens no window, but winit's event loop wants a Wayland compositor to say hello first: on Linux without
/// one (a build machine, a terminal over SSH), scripts/wayland-stub.py stands in. macOS needs none.
fn wayland_for_snapshots(tmp: &Path) -> Option<Stub> {
    if !cfg!(target_os = "linux") || std::env::var_os("WAYLAND_DISPLAY").is_some_and(|w| !w.is_empty()) {
        return None;
    }
    let path = tmp.join("wayland-stub");
    let child = Command::new("python3")
        .args(["-I", &format!("{ROOT}/scripts/wayland-stub.py"), path.to_str().unwrap()])
        .spawn()
        .expect("python3 for scripts/wayland-stub.py");
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

fn fake_library(dir: &Path) {
    let status = Command::new("python3")
        .args(["-I", &format!("{ROOT}/scripts/curator-fake-library.py"), dir.to_str().unwrap(), "dark"])
        .stdout(std::process::Stdio::null())
        .status()
        .expect("python3 for scripts/curator-fake-library.py");
    assert!(status.success(), "curator-fake-library.py failed");
}

#[test]
fn every_string_in_the_german_snapshots_is_translated_or_a_name() {
    let _tmp_guard = tempfile::tempdir().unwrap();
    let tmp = _tmp_guard.path().to_path_buf();
    fake_library(&tmp);
    let bin = PathBuf::from(env!("CARGO_BIN_EXE_pocket-ui"));
    let wayland = wayland_for_snapshots(&tmp);
    let work: Arc<Mutex<Vec<(usize, bool, String)>>> = Arc::new(Mutex::new(states().into_iter().enumerate().map(|(i, (safe, s))| (i, safe, s)).collect()));
    let failed = Arc::new(Mutex::new(Vec::new()));
    let threads: Vec<_> = (0..std::thread::available_parallelism().map_or(4, |n| n.get().min(8)))
        .map(|_| {
            let (work, failed, tmp, bin, wayland) = (work.clone(), failed.clone(), tmp.clone(), bin.clone(), wayland.as_ref().map(|w| w.path.clone()));
            std::thread::spawn(move || loop {
                let Some((i, safe, state)) = work.lock().unwrap().pop() else { break };
                // Its own copy of the data folder: some states write into it (save slots), and they run side by side.
                let data = tmp.join(format!("data-{i}"));
                copy_dir(&tmp.join("data"), &data);
                let mut cmd = Command::new(&bin);
                if safe {
                    cmd.arg("--safe");
                }
                if let Some(path) = &wayland {
                    cmd.env("WAYLAND_DISPLAY", path);
                }
                let out = cmd
                    .args(["--snapshot", &state, tmp.join(format!("{i}.png")).to_str().unwrap()])
                    .env("POCKETUI_MOCK", "1")
                    .env("POCKETUI_LANG", "de")
                    .env("POCKETUI_I18N_MISSES", tmp.join(format!("{i}.tsv")))
                    .env("POCKETUI_ES_SYSTEMS", tmp.join("es_systems.cfg"))
                    .env("POCKETUI_DATA", &data)
                    .output()
                    .expect("run pocket-ui");
                if !out.status.success() || !tmp.join(format!("{i}.png")).exists() {
                    failed.lock().unwrap().push(format!("{state}: {}", String::from_utf8_lossy(&out.stderr)));
                }
            })
        })
        .collect();
    for t in threads {
        t.join().unwrap();
    }
    drop(wayland);
    let failed = failed.lock().unwrap();
    assert!(failed.is_empty(), "snapshots that didn't render:\n{}", failed.join("\n"));

    let (keep_exact, keep_patterns) = keep();
    let mut data = english_data();
    data.extend(names_from_data(&tmp));
    // Names (of games, systems, places, and what keep.txt keeps), longest first, for `bare` below.
    let mut names: Vec<String> = data.iter().chain(keep_exact.iter()).filter(|n| n.chars().count() >= 2 && n.chars().any(|c| c.is_uppercase() || c.is_ascii_digit() || c == '-')).cloned().collect();
    names.sort_by_key(|n| std::cmp::Reverse(n.len()));
    names.dedup();
    let mut missing: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (i, (_, state)) in states().iter().enumerate() {
        let text = std::fs::read_to_string(tmp.join(format!("{i}.tsv"))).unwrap_or_default();
        let made: Vec<&str> = text.lines().filter_map(|l| l.strip_prefix("O\t")).collect();
        let shown: Vec<&str> = text.lines().filter_map(|l| l.strip_prefix("M\t")).map(str::trim).collect();
        // `M`: painted as it is; `P`: a part a pattern left in English, inside a German line (never excused by it).
        for (part, m) in text.lines().filter_map(|l| l.strip_prefix("M\t").map(|m| (false, m)).or_else(|| l.strip_prefix("P\t").map(|m| (true, m)))) {
            let core = m.trim().trim_end_matches('…').trim();
            // A piece of a name or a note: its start (cut short with "…", or measured letter by letter), its last
            // line, or a line in the middle long enough not to be a word of the interface that happens to be in a
            // name or a note ("Display", "Memory").
            // Names taken out, is anything left that reads like words ("GBA 74 h · SNES 60 h": no)?
            let bare = || {
                let mut b = format!(" {core} ");
                for n in &names {
                    b = b.replace(n.as_str(), " ");
                }
                b
            };
            let piece_of = |whole: &str| {
                whole == core || whole.starts_with(core) || (core.chars().count() >= 6 && whole.ends_with(core)) || (core.chars().count() >= 12 && whole.contains(core))
            };
            if !words(core)
                || (!part && made.iter().any(|t| t.contains(core)))
                // Measured letter by letter on the way to a longer one, which is checked itself.
                || shown.iter().any(|w| w.len() > core.len() && w.starts_with(core))
                || keep_exact.contains(m.trim())
                || keep_exact.iter().any(|k| piece_of(k))
                || keep_patterns.iter().any(|p| keep_match(p, m.trim()))
                || data.iter().any(|d| piece_of(d))
                || !words(&bare())
            {
                continue;
            }
            missing.entry(m.trim().to_string()).or_default().push(state.clone());
        }
    }
    let report: Vec<String> = missing.iter().map(|(s, states)| format!("{s:?}  ({})", states.join(", "))).collect();
    assert!(
        missing.is_empty(),
        "{} strings are shown in German snapshots without a translation: add them to assets/i18n/de.tsv (or, for a name, to assets/i18n/keep.txt):\n{}",
        missing.len(),
        report.join("\n")
    );
}

#[test]
fn keep_patterns() {
    assert!(keep_match("^/", "/storage/roms"));
    assert!(!keep_match("^/", "a/b"));
    assert!(keep_match("^\\d h$", "12 h"));
    assert!(keep_match("^v\\d.*$", "v0.1.0"));
    assert!(keep_match("pocket-ui", "1 of 19   pocket-ui"));
    assert!(keep_match("\\.lan$", "office-pc.lan"));
    assert!(!keep_match("\\.lan$", "office-pclan"));
    assert!(keep_match("^HOME \\+ ", "HOME + L1"));
    assert!(keep_match("^\\\\\\\\", "\\\\host\\share"));
    assert!(keep_match(".* · \\d$", "PS2 · 2004"));
}

/// The other half: every string in src/ that reads like words for a person (scripts/i18n-extract.py) has a German
/// line, or is a name (keep.txt), or is never painted on its own (skip.txt). This catches strings of pages no
/// snapshot state shows yet; the test above catches what the script can't see (strings put together at run time).
#[test]
fn every_string_in_the_source_has_a_german_line_or_is_listed() {
    let out = Command::new("python3")
        .args(["-I", &format!("{ROOT}/scripts/i18n-extract.py"), "--check"])
        .output()
        .expect("python3 for scripts/i18n-extract.py");
    assert!(
        out.status.success(),
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

/// What a player gets without touching Settings → Language: the system language EmulationStation was set to, which
/// the launcher passes on as `LANG` (deploy/run-pocket-ui.sh); a choice in settings.json wins over it.
#[test]
fn the_system_language_decides_until_one_is_chosen() {
    let _tmp_guard = tempfile::tempdir().unwrap();
    let tmp = _tmp_guard.path().to_path_buf();
    let wayland = wayland_for_snapshots(&tmp);
    let shot = |lang: &str, settings: Option<&str>, name: &str| -> String {
        let data = tmp.join(name);
        std::fs::create_dir_all(&data).unwrap();
        if let Some(s) = settings {
            std::fs::write(data.join("settings.json"), s).unwrap();
        }
        let misses = tmp.join(format!("{name}.tsv"));
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
        cmd.args(["--snapshot", "setup:first", tmp.join(format!("{name}.png")).to_str().unwrap()])
            .env("POCKETUI_MOCK", "1")
            .env_remove("POCKETUI_LANG")
            .env("LANG", lang)
            .env("POCKETUI_I18N_MISSES", &misses)
            .env("POCKETUI_ES_SYSTEMS", tmp.join("none.cfg"))
            .env("POCKETUI_DATA", &data);
        if let Some(w) = &wayland {
            cmd.env("WAYLAND_DISPLAY", &w.path);
        }
        let out = cmd.output().expect("run pocket-ui");
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        std::fs::read_to_string(misses).unwrap_or_default()
    };
    let german = shot("de_DE.UTF-8", None, "de");
    assert!(german.contains("O\tWillkommen auf deinem Pocket"), "ROCKNIX set to German, nothing chosen:\n{german}");
    let english = shot("en_US.UTF-8", None, "en");
    assert!(!english.contains("O\t"), "ROCKNIX set to English:\n{english}");
    let chosen = shot("de_DE.UTF-8", Some(r#"{"language": "en"}"#), "chosen");
    assert!(!chosen.contains("O\t"), "English chosen in Settings, ROCKNIX German:\n{chosen}");
    drop(wayland);
}
