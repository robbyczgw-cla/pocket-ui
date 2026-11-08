//! The names in the search results, whole or telling the game apart (`title_block::row`, docs/DESIGN.md "A game's name").
//! Brick-beta photo, 2026-11-08: "The Legend of Zelda: Link's Awakening DX" and "…: Ocarina of Time 3D" stood cut with "…"
//! beside the system plaque, and two Link's Awakening rows could end in the same "The Legend of Zelda: Link's Aw…".
//! The real long names of tests/long_titles.rs and the three Zelda games of the made-up library, searched in Barlow and
//! Retro (Pixelify), in Normal and Large text, in English, German and Spanish, at 640x480, 1024x768, 1240x1080 and
//! 1280x720 (and, with `--features brick`, as the Brick draws it):
//! - no result's name ends in "…" while the whole name, or the part of it that tells the game apart, has room;
//! - each result shows the end of its name that tells it apart ("DX", "3D") and the two Link's Awakening rows differ;
//! - nothing the picture paints runs into the system plaque, another string or an edge.
//!
//! Needs python3 (and, on Linux without a compositor, scripts/wayland-stub.py), as tests/long_titles.rs.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};

include!("common/root.rs");

/// (system folder, file, the name as the shelf shows it).
const GAMES: [(&str, &str, &str); 8] = [
    ("n64", "Legend of Zelda, The - Ocarina of Time (USA).z64", "The Legend of Zelda - Ocarina of Time"),
    ("3ds", "Legend of Zelda, The - Ocarina of Time 3D (USA) (En,Fr,Es).3ds", "The Legend of Zelda - Ocarina of Time 3D"),
    ("gb", "Legend of Zelda, The - Link's Awakening (USA, Europe) (Rev 2).gb", "The Legend of Zelda - Link's Awakening"),
    ("gbc", "Legend of Zelda, The - Link's Awakening DX (USA, Europe) (Rev 2) (SGB Enhanced) (GB Compatible).gbc", "The Legend of Zelda - Link's Awakening DX"),
    ("snes", "Donkey Kong Country 3 - Dixie Kong's Double Trouble! (USA).sfc", "Donkey Kong Country 3 - Dixie Kong's Double Trouble!"),
    ("gba", "Super Mario Advance 4 - Super Mario Bros. 3 (USA).gba", "Super Mario Advance 4 - Super Mario Bros. 3"),
    ("gba", "Castlevania - Aria of Sorrow (USA).gba", "Castlevania - Aria of Sorrow"),
    ("nds", "Pokemon - Origin HeartGold (USA).nds", "Pokemon - Origin HeartGold"),
];

/// What a player types to find each of them, and the games that query must show.
const QUERIES: [(&str, &[usize]); 6] = [
    ("ocarina of time", &[0, 1]),
    ("link's awakening", &[2, 3]),
    ("dixie kong", &[4]),
    ("advance 4", &[5]),
    ("aria of sorrow", &[6]),
    ("origin heartgold", &[7]),
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
    let path = tmp.join("wayland-stub");
    let child = Command::new("python3").args(["-I", &format!("{ROOT}/scripts/wayland-stub.py"), path.to_str().unwrap()]).spawn().expect("python3");
    let stub = Stub(child, path);
    test_clock::wait_for(|| stub.1.exists(), std::time::Duration::from_secs(2), "Wayland stub listening");
    Some(stub)
}

/// The made-up library with only these games on its shelves, one folder for each system.
fn library(tmp: &Path) -> PathBuf {
    let lib = tmp.join("lib");
    let ok = Command::new("python3")
        .env("POCKETUI_MOCK_NOW", FIXTURE_NOW)
        .args(["-I", &format!("{ROOT}/scripts/curator-fake-library.py"), lib.to_str().unwrap(), "dark"])
        .stdout(std::process::Stdio::null())
        .status()
        .expect("python3 for scripts/curator-fake-library.py");
    assert!(ok.success());
    for entry in std::fs::read_dir(lib.join("roms")).unwrap() {
        let dir = entry.unwrap().path();
        if dir.is_dir() {
            for f in std::fs::read_dir(&dir).unwrap() {
                let f = f.unwrap().path();
                if f.is_file() {
                    std::fs::remove_file(f).unwrap();
                }
            }
        }
    }
    for (system, file, _) in GAMES {
        std::fs::write(lib.join("roms").join(system).join(file), b"made up").unwrap();
    }
    lib
}

#[derive(Clone)]
struct Job {
    state: String,
    size: &'static str,
    retro: bool,
    text: &'static str,
    lang: &'static str,
}

struct Shot {
    /// The strings the page painted.
    names: Vec<String>,
    faults: Vec<String>,
}

fn shoot(tmp: &Path, lib: &Path, stub: &Option<PathBuf>, id: usize, j: &Job) -> Result<Shot, String> {
    let name = format!("{id}");
    let data = tmp.join(format!("data-{name}"));
    assert!(Command::new("cp").args(["-r", lib.join("data").to_str().unwrap(), data.to_str().unwrap()]).status().unwrap().success());
    let settings = data.join("settings.json");
    let mut json: serde_json::Value = std::fs::read_to_string(&settings).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_else(|| serde_json::json!({}));
    json["onboarded"] = serde_json::json!(true);
    std::fs::write(&settings, json.to_string()).unwrap();
    let (png, boxes, probe) = (tmp.join(format!("{name}.png")), tmp.join(format!("{name}.box")), tmp.join(format!("{name}.probe")));
    let bin = std::env::var_os("POCKETUI_NAV_BINARY").map_or_else(|| PathBuf::from(env!("CARGO_BIN_EXE_pocket-ui")), PathBuf::from);
    let mut cmd = Command::new(bin);
    if cfg!(feature = "brick") {
        cmd.env("POCKETUI_DEVICE", "brick").env("POCKETUI_BRICK_STANDALONE", "1");
    }
    if let Some(s) = stub {
        cmd.env("WAYLAND_DISPLAY", s);
    }
    if j.retro {
        cmd.env("POCKETUI_RETRO", "cocoa");
    }
    let out = cmd
        .args(["--snapshot", &j.state, png.to_str().unwrap()])
        .env("POCKETUI_MOCK", "1")
        .env("POCKETUI_MOCK_NOW", FIXTURE_NOW)
        .env("POCKETUI_LANG", j.lang)
        .env("POCKETUI_TEXT_SIZE", j.text)
        .env("POCKETUI_SNAPSHOT_SIZE", j.size)
        .env("POCKETUI_FONT", if j.retro { "pixel" } else { "barlow" })
        .env("POCKETUI_TEXT_BOXES", &boxes)
        .env("POCKETUI_NAV_PROBE", &probe)
        .env("POCKETUI_ES_SYSTEMS", lib.join("es_systems.cfg"))
        .env("POCKETUI_DATA", &data)
        .output()
        .expect("run pocket-ui");
    if !out.status.success() || !png.exists() {
        return Err(format!("{} didn't render: {}", j.state, String::from_utf8_lossy(&out.stderr)));
    }
    // Every string the page painted (x, y, right, bottom, text; `nav_probe`).
    let names = std::fs::read_to_string(&probe)
        .unwrap_or_default()
        .lines()
        .filter_map(|l| Some(l.splitn(5, '\t').nth(4)?.to_string()))
        .collect();
    let faults = std::fs::read_to_string(&boxes).unwrap_or_default().lines().filter(|l| !l.is_empty()).map(str::to_string).collect();
    let _ = std::fs::remove_dir_all(&data);
    let _ = std::fs::remove_file(&png);
    Ok(Shot { names, faults })
}

/// The words of a name without its dash or colon.
fn words(s: &str) -> Vec<String> {
    s.replace(" - ", " ").replace(": ", " ").split_whitespace().map(str::to_string).collect()
}

/// `shown` is a run of the name's words that ends where the name does ("Link's Awakening DX" of "The Legend of Zelda -
/// Link's Awakening DX", "Kong Country 3" would not be: a numbered main part is allowed too, as the spine has it).
fn tells_apart(shown: &str, title: &str) -> bool {
    let (s, t) = (words(shown), words(title));
    !s.is_empty() && !shown.ends_with('…') && t.windows(s.len()).any(|w| w == s.as_slice()) && (t.ends_with(&s) || s.iter().any(|w| w.chars().all(|c| c.is_ascii_digit())))
}

#[test]
fn search_results_show_long_names_whole_or_telling_the_game_apart() {
    let tmp_guard = tempfile::tempdir().unwrap();
    let tmp = tmp_guard.path().to_path_buf();
    let stub = wayland(&tmp);
    let lib = library(&tmp);
    let quick = std::env::var_os("POCKETUI_TEST_QUICK").is_some();
    let mut jobs = Vec::new();
    for size in if quick { vec!["1024x768"] } else { vec!["640x480", "1024x768", "1240x1080", "1280x720"] } {
        for retro in [false, true] {
            for text in if quick { vec!["normal"] } else { vec!["normal", "large"] } {
                for lang in if quick { vec!["en"] } else { vec!["en", "de", "es"] } {
                    for (q, _) in QUERIES {
                        jobs.push(Job { state: format!("search:results:{q}"), size, retro, text, lang });
                    }
                    // While typing: the keyboard under the list, smaller rows.
                    jobs.push(Job { state: "search:typing:link's awakening".into(), size, retro, text, lang });
                }
            }
        }
    }
    let n = jobs.len();
    let stub_path = stub.as_ref().map(|s| s.1.clone());
    let work = Arc::new(Mutex::new(jobs.into_iter().enumerate().collect::<Vec<_>>()));
    let done: Arc<Mutex<Vec<(Job, Result<Shot, String>)>>> = Arc::new(Mutex::new(Vec::new()));
    let threads: Vec<_> = (0..std::thread::available_parallelism().map_or(4, |n| n.get().min(6)))
        .map(|_| {
            let (work, done, tmp, lib, stub) = (work.clone(), done.clone(), tmp.clone(), lib.clone(), stub_path.clone());
            std::thread::spawn(move || loop {
                let Some((id, job)) = work.lock().unwrap().pop() else { break };
                let shot = shoot(&tmp, &lib, &stub, id, &job);
                done.lock().unwrap().push((job, shot));
            })
        })
        .collect();
    for t in threads {
        t.join().unwrap();
    }
    let done = done.lock().unwrap();
    assert_eq!(done.len(), n);
    let mut wrong = Vec::new();
    for (j, shot) in done.iter() {
        let label = format!("{} at {}, {}, {} text, {}", j.state, j.size, if j.retro { "Retro" } else { "Barlow" }, j.text, j.lang);
        let shot = match shot {
            Ok(s) => s,
            Err(e) => {
                wrong.push(format!("{label}: {e}"));
                continue;
            }
        };
        let shown: Vec<&str> = shot.names.iter().map(String::as_str).collect();
        if let Some(cut) = shown.iter().find(|s| s.ends_with('…')) {
            wrong.push(format!("{label}: a name is cut: {cut:?} (all: {shown:?})"));
        }
        if j.state.starts_with("search:results:") {
            let query = j.state.trim_start_matches("search:results:");
            let (_, games) = QUERIES.iter().find(|q| q.0 == query).unwrap();
            for &g in games.iter() {
                let title = GAMES[g].2;
                if !shown.iter().any(|s| s == &title || s == &title.trim_start_matches("The ") || tells_apart(s, title)) {
                    wrong.push(format!("{label}: no row shows {title:?} whole or telling it apart: {shown:?}"));
                }
            }
        }
        if j.state.starts_with("search:results:link's awakening") {
            let dx = shown.iter().any(|s| s.contains("DX"));
            let plain = shown.iter().any(|s| s.contains("Awakening") && !s.contains("DX"));
            if !dx || !plain {
                wrong.push(format!("{label}: the two Link's Awakening rows must differ (DX {dx}, plain {plain}): {shown:?}"));
            }
        }
        for f in &shot.faults {
            if matches!(f.chars().next(), Some('O' | 'T' | 'V' | 'E')) {
                wrong.push(format!("{label}: {}", f.replace('\t', " | ")));
            }
        }
    }
    wrong.sort();
    assert!(wrong.is_empty(), "{} problems in {n} renders:\n{}", wrong.len(), wrong.join("\n"));
}
