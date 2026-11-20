//! The meta line under a game's name in the Shelf and Cabinet looks ("Super Nintendo, last played 3 days ago, 6 h in total")
//! is always whole: never cut with "…", never left off. A Brick photo (1024x768, Retro, Cocoa, 18.11.2026) showed the
//! Favourites' line as "Super Nintendo, last played 3 days ago, 6 h …", and the Android handheld view "Last played just now,
//! under a …": since the shown box grew 1.4 times (21b457727) the words beside it have less width.
//!
//! Every state of the line (never played, just now, hours, days, years ago, with a status, with the play time, the
//! RetroAchievements tally, the progress read from a save, Rediscover's "played a year ago today"), on the Favourites row, a
//! system's shelf, Home's Continue and Rediscover, in Shelf and Cabinet, on every panel, in all six languages, Normal and Large
//! text, Barlow and Retro, and in the Android handheld view (1240x1080, `POCKETUI_ANDROID_PROFILE`). The lines painted
//! (`POCKETUI_META_LINES`), put together, are the whole line in the interface's language, word for word.
//! `POCKETUI_TEST_QUICK` (the merge gate's sample): 1024x768 and the Android view, German and French, both fonts and looks.
//! `META_LINE_SHOTS=<dir>` keeps the pictures of the faults there (`META_LINE_SHOTS_ALL=1`: every picture;
//! `META_LINE_ONLY=<a>|<b>`: only the pictures whose label has one of these).
//!
//! Needs python3 (and, on Linux without a compositor, scripts/wayland-stub.py), as tests/long_titles.rs.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};

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

const HOUR: u64 = 3600;
const DAY: u64 = 24 * HOUR;

/// The Favourites row, each game in one state: (system, file, name, last played this long ago and played this long in all,
/// its status).
const FAVOURITES: [(&str, &str, &str, Option<(u64, u64)>, Option<&str>); 10] = [
    ("snes", "Super Mario Kart (USA).sfc", "Super Mario Kart", None, None),
    ("snes", "Secret of Mana (USA).sfc", "Secret of Mana", Some((0, 20)), None),
    ("snes", "Chrono Trigger (USA).sfc", "Chrono Trigger", Some((5 * HOUR, 6 * HOUR)), None),
    ("nes", "Final Fantasy III (Japan) [T-En by Neill Corlett v1.1].nes", "Final Fantasy III", Some((3 * DAY, 6 * HOUR)), None),
    ("psp", "Burnout Legends (USA).iso", "Burnout Legends", Some((800 * DAY, 125 * HOUR)), None),
    ("ps2", "Gran Turismo 4 (USA).iso", "Gran Turismo 4", Some((2 * HOUR, 30 * HOUR)), Some("playing")),
    ("snes", "Super Metroid (Japan, USA) (En,Ja).sfc", "Super Metroid", Some((9 * DAY, 8 * HOUR)), Some("completed")),
    ("nds", "Chrono Trigger (USA) (En,Fr).nds", "Chrono Trigger", Some((40 * DAY, 9 * HOUR)), Some("paused")),
    ("gba", "Pokemon - Emerald Version (USA, Europe).gba", "Pokemon - Emerald Version", Some((DAY + HOUR, 2 * HOUR)), Some("dropped")),
    ("psp", "Lumines (USA).iso", "Lumines", Some((0, 20)), Some("playing")),
];

/// The made-up library: the fixture's (with Rediscover's year-ago games), the Super Nintendo shelf only the four games of the
/// Favourites row, the recent games and statuses as `FAVOURITES` says.
fn library(tmp: &Path) -> PathBuf {
    let lib = tmp.join("lib");
    let ok = Command::new("python3")
        .env("POCKETUI_MOCK_NOW", FIXTURE_NOW)
        .env("POCKETUI_REDISCOVER", "1")
        .args(["-I", &format!("{ROOT}/scripts/curator-fake-library.py"), lib.to_str().unwrap(), "dark"])
        .stdout(std::process::Stdio::null())
        .status()
        .expect("python3 for scripts/curator-fake-library.py");
    assert!(ok.success());
    let snes = lib.join("roms/snes");
    for f in std::fs::read_dir(&snes).unwrap() {
        let f = f.unwrap().path();
        let keep = FAVOURITES.iter().any(|g| g.0 == "snes" && f.file_name().is_some_and(|n| n == g.1));
        if f.is_file() && !keep {
            std::fs::remove_file(f).unwrap();
        }
    }
    let now: u64 = FIXTURE_NOW.parse().unwrap();
    let path = |system: &str, file: &str| lib.join("roms").join(system).join(file);
    let favourites: Vec<_> = FAVOURITES.iter().map(|g| serde_json::json!({"system": g.0, "name": g.2, "path": path(g.0, g.1)})).collect();
    let mut recents: Vec<_> = FAVOURITES
        .iter()
        .filter_map(|g| g.3.map(|(ago, secs)| (now - ago, serde_json::json!({"system": g.0, "name": g.2, "path": path(g.0, g.1), "last_played": now - ago, "play_secs": secs}))))
        .collect();
    recents.sort_by_key(|r| std::cmp::Reverse(r.0));
    let recents: Vec<_> = recents.into_iter().map(|r| r.1).collect();
    let data = lib.join("data");
    let library_file = data.join("library.json");
    let mut library: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&library_file).unwrap()).unwrap();
    for g in FAVOURITES {
        let key = path(g.0, g.1).to_string_lossy().to_string();
        match g.4 {
            Some(status) => library["games"][&key] = serde_json::json!({"status": status, "status_set": true, "sessions": 3, "secs": g.3.map_or(0, |p| p.1)}),
            None => {
                if let Some(games) = library["games"].as_object_mut() {
                    games.remove(&key);
                }
            }
        }
    }
    std::fs::write(&library_file, library.to_string()).unwrap();
    std::fs::write(data.join("favorites.json"), serde_json::Value::from(favourites).to_string()).unwrap();
    std::fs::write(data.join("recents.json"), serde_json::Value::from(recents).to_string()).unwrap();
    lib
}

#[derive(Clone)]
struct Job {
    state: String,
    look: &'static str,
    size: &'static str,
    android: bool,
    retro: bool,
    text: &'static str,
    lang: &'static str,
    /// Extra variables: the RetroAchievements tally or a save's progress for Home's Continue.
    env: Vec<(&'static str, &'static str)>,
}

impl Job {
    fn label(&self) -> String {
        let extra: Vec<String> = self.env.iter().map(|(k, v)| format!("{k}={v}")).collect();
        format!(
            "{}{} at {}, {}, {} text, {}, {}{}",
            if self.android { "Android handheld view, " } else { "" },
            self.look,
            self.size,
            if self.retro { "Retro" } else { "Barlow" },
            self.text,
            self.lang,
            self.state,
            if extra.is_empty() { String::new() } else { format!(" ({})", extra.join(" ")) }
        )
    }
}

/// The states of a look: every row of the Favourites, of the Super Nintendo and the PlayStation Portable shelves (a
/// system's list says no system), Home's first rows with Continue (also with its tally and its save's progress) and
/// Rediscover.
fn states(look: &str) -> Vec<(String, Vec<(&'static str, &'static str)>)> {
    let mut v: Vec<(String, Vec<(&'static str, &'static str)>)> = Vec::new();
    for row in 0..FAVOURITES.len() {
        v.push((format!("shelf:favorites:{look}:games:{row}"), vec![]));
    }
    for (system, n) in [("snes", 4), ("psp", 4)] {
        for row in 0..n {
            v.push((format!("shelf:{system}:{look}:games:{row}"), vec![]));
        }
    }
    // Which of Home's first rows holds Continue depends on the look (Curator, Search and Just Play may come first).
    for row in 0..6 {
        v.push((format!("home:{row}"), vec![]));
        v.push((format!("home:{row}"), vec![("POCKETUI_SNAPSHOT_RA", "34/120")]));
        v.push((format!("home:{row}"), vec![("POCKETUI_SNAPSHOT_PROGRESS", "pokemon")]));
    }
    v.push((format!("curator:rediscover:{look}"), vec![]));
    v
}

struct Shot {
    /// (the whole line, the lines painted) of each meta line the picture has.
    metas: Vec<(String, Vec<String>)>,
    faults: Vec<String>,
}

fn shoot(tmp: &Path, lib: &Path, stub: &Option<PathBuf>, id: usize, j: &Job, keep: Option<&Path>) -> Result<Shot, String> {
    let data = tmp.join(format!("data-{id}"));
    let status = Command::new("cp").args(["-r", lib.join("data").to_str().unwrap(), data.to_str().unwrap()]).status().unwrap();
    assert!(status.success());
    let settings = data.join("settings.json");
    let mut json: serde_json::Value = std::fs::read_to_string(&settings).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_else(|| serde_json::json!({}));
    json["theme"] = serde_json::json!(j.look);
    json["onboarded"] = serde_json::json!(true);
    json["motion"] = serde_json::json!("off");
    std::fs::write(&settings, json.to_string()).unwrap();
    let (png, boxes, metas) = (tmp.join(format!("{id}.png")), tmp.join(format!("{id}.box")), tmp.join(format!("{id}.meta")));
    let bin = std::env::var_os("POCKETUI_NAV_BINARY").map_or_else(|| PathBuf::from(env!("CARGO_BIN_EXE_pocket-ui")), PathBuf::from);
    let mut cmd = Command::new(bin);
    if let Some(s) = stub {
        cmd.env("WAYLAND_DISPLAY", s).env_remove("SWAYSOCK");
    }
    if j.retro {
        cmd.env("POCKETUI_RETRO", "cocoa");
    }
    if j.android {
        cmd.env("POCKETUI_ANDROID_PROFILE", "1");
    }
    for (k, v) in &j.env {
        cmd.env(k, v);
    }
    let out = cmd
        .args(["--snapshot", &j.state, png.to_str().unwrap()])
        .env("POCKETUI_MOCK", "1")
        .env("POCKETUI_MOCK_NOW", FIXTURE_NOW)
        .env("POCKETUI_LANG", j.lang)
        .env("POCKETUI_TEXT_SIZE", j.text)
        .env("POCKETUI_SNAPSHOT_SIZE", j.size)
        .env("POCKETUI_FONT", if j.retro { "retro" } else { "barlow" })
        .env("POCKETUI_TEXT_BOXES", &boxes)
        .env("POCKETUI_META_LINES", &metas)
        .env("POCKETUI_ES_SYSTEMS", lib.join("es_systems.cfg"))
        .env("POCKETUI_DATA", &data)
        .output()
        .expect("run pocket-ui");
    let _ = std::fs::remove_dir_all(&data);
    if !out.status.success() || !png.exists() {
        return Err(format!("didn't render: {}", String::from_utf8_lossy(&out.stderr)));
    }
    let metas = std::fs::read_to_string(&metas)
        .unwrap_or_default()
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| {
            let mut p = l.split('\t').map(str::to_string);
            (p.next().unwrap_or_default(), p.collect())
        })
        .collect();
    let faults = std::fs::read_to_string(&boxes).unwrap_or_default().lines().filter(|l| !l.is_empty()).map(str::to_string).collect();
    if let Some(dir) = keep {
        let name = j.label().replace([' ', '/', ':', '=', ','], "_").replace("__", "_");
        let _ = std::fs::copy(&png, dir.join(format!("{name}.png")));
    }
    let _ = std::fs::remove_file(&png);
    Ok(Shot { metas, faults })
}

/// What is wrong with a meta line: cut with "…", left off, or its lines not the whole line word for word.
fn meta_faults(whole: &str, lines: &[String]) -> Option<String> {
    if whole.trim().is_empty() {
        return None;
    }
    if lines.is_empty() {
        return Some(format!("left off: {whole:?}"));
    }
    if lines.iter().any(|l| l.trim_end().ends_with('…')) {
        return Some(format!("cut: {lines:?} (whole: {whole:?})"));
    }
    // A line broken at its " · " leaves the dot out, on two lines and on three (review 19.11. #15: "Super Nintendo ·").
    if lines.iter().any(|l| l.trim_end().ends_with('·') || l.trim_start().starts_with('·')) {
        return Some(format!("a dot at a line's end: {lines:?} (whole: {whole:?})"));
    }
    let words = |s: &str| s.split_whitespace().filter(|w| *w != "·").map(str::to_string).collect::<Vec<_>>();
    if words(&lines.join(" ")) != words(whole) {
        return Some(format!("not the whole line: {lines:?} (whole: {whole:?})"));
    }
    None
}

/// A status or sticker word in English, as a word of its own, in `line`.
fn english_word(line: &str) -> Option<&'static str> {
    let low = line.to_lowercase();
    ["playing now", "paused", "completed", "dropped", "favourite", "game of the year"].into_iter().find(|w| {
        low.match_indices(w).any(|(at, _)| {
            low[..at].chars().next_back().is_none_or(|c| !c.is_alphanumeric()) && low[at + w.len()..].chars().next().is_none_or(|c| !c.is_alphanumeric())
        })
    })
}

fn jobs(quick: bool) -> Vec<Job> {
    let mut jobs = Vec::new();
    let sizes: &[&str] = if quick { &["1024x768"] } else { &["640x480", "720x720", "1024x768", "1240x1080", "1280x720", "1920x1080"] };
    let langs: &[&str] = if quick { &["de", "fr"] } else { &["en", "de", "es", "pt", "fr", "it"] };
    let texts: &[&str] = if quick { &["large"] } else { &["normal", "large"] };
    for look in ["shelf", "cabinet"] {
        for (android, size) in sizes.iter().map(|s| (false, *s)).chain(std::iter::once((true, "1240x1080"))) {
            for retro in [false, true] {
                for &text in texts {
                    for &lang in langs {
                        for (state, env) in states(look) {
                            jobs.push(Job { state, look, size, android, retro, text, lang, env });
                        }
                    }
                }
            }
        }
    }
    jobs
}

#[test]
fn the_meta_line_under_a_games_name_is_whole_in_every_state_look_and_language() {
    let _tmp_guard = tempfile::tempdir().unwrap();
    let tmp = _tmp_guard.path().to_path_buf();
    let stub = wayland(&tmp);
    let lib = library(&tmp);
    let quick = std::env::var_os("POCKETUI_TEST_QUICK").is_some();
    let keep = std::env::var_os("META_LINE_SHOTS").map(PathBuf::from);
    if let Some(dir) = &keep {
        std::fs::create_dir_all(dir).unwrap();
    }
    let keep_all = std::env::var_os("META_LINE_SHOTS_ALL").is_some();
    // META_LINE_ONLY=<a>|<b>: only the pictures whose label has one of these (for renders by hand).
    let only = std::env::var("META_LINE_ONLY").ok();
    let jobs: Vec<Job> = jobs(quick).into_iter().filter(|j| only.as_deref().is_none_or(|o| o.split('|').any(|p| j.label().contains(p)))).collect();
    let n = jobs.len();
    let stub_path = stub.as_ref().map(|s| s.1.clone());
    let work = Arc::new(Mutex::new(jobs.into_iter().enumerate().collect::<Vec<_>>()));
    let wrong = Arc::new(Mutex::new(Vec::new()));
    let seen = Arc::new(Mutex::new(std::collections::BTreeSet::new()));
    let threads: Vec<_> = (0..std::thread::available_parallelism().map_or(4, |n| n.get().min(6)))
        .map(|_| {
            let (work, wrong, seen, tmp, lib, stub, keep) = (work.clone(), wrong.clone(), seen.clone(), tmp.clone(), lib.clone(), stub_path.clone(), keep.clone());
            std::thread::spawn(move || loop {
                let Some((id, j)) = work.lock().unwrap().pop() else { break };
                let label = j.label();
                let first = shoot(&tmp, &lib, &stub, id, &j, keep.as_deref().filter(|_| keep_all));
                let shot = match first {
                    Ok(s) => s,
                    Err(e) => {
                        wrong.lock().unwrap().push(format!("{label}: {e}"));
                        continue;
                    }
                };
                let mut bad = Vec::new();
                for (whole, lines) in &shot.metas {
                    seen.lock().unwrap().insert(whole.clone());
                    if let Some(f) = meta_faults(whole, lines) {
                        bad.push(f);
                    }
                    // No status or sticker word stays English ("Nintendo 64, completed, zuletzt gespielt …", 18.11.2026).
                    if j.lang != "en" {
                        if let Some(w) = english_word(whole) {
                            bad.push(format!("{w:?} in English: {whole:?}"));
                        }
                    }
                    // Its lines run off no edge (strings running into one another are long_titles' and text_size's).
                    for f in shot.faults.iter().filter(|f| matches!(f.chars().next(), Some('V' | 'E'))) {
                        if lines.iter().any(|l| !l.is_empty() && f.contains(l.as_str())) {
                            bad.push(f.replace('\t', " | "));
                        }
                    }
                }
                // A game of a shelf or of Rediscover says something under its name: never nothing at all.
                if shot.metas.is_empty() && !j.state.starts_with("home:") {
                    bad.push("no meta line painted".into());
                }
                if !bad.is_empty() {
                    if let Some(dir) = keep.as_deref().filter(|_| !keep_all) {
                        let _ = shoot(&tmp, &lib, &stub, id, &j, Some(dir));
                    }
                    let mut w = wrong.lock().unwrap();
                    for b in bad {
                        w.push(format!("{label}: {b}"));
                    }
                }
            })
        })
        .collect();
    for t in threads {
        t.join().unwrap();
    }
    let seen = seen.lock().unwrap();
    let mut wrong = wrong.lock().unwrap();
    // The test looked at what it means to: each state's line was on one of the pictures (in English where the matrix has it).
    let en = !quick;
    for (what, part) in [
        ("never played", if en { "Not played yet" } else { "Noch nicht gespielt" }),
        ("just now", if en { "just now" } else { "gerade eben" }),
        ("a year ago today", if en { "a year ago today" } else { "heute vor einem Jahr" }),
        ("the tally", "34"),
    ] {
        if !seen.iter().any(|s| s.contains(part)) {
            wrong.push(format!("no picture showed the line for {what} ({part:?}): the fixture is not what the test thinks"));
        }
    }
    wrong.sort();
    wrong.dedup();
    assert!(wrong.is_empty(), "{} meta lines not whole in {n} pictures:\n{}", wrong.len(), wrong.join("\n"));
}
