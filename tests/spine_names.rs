//! The names on the spines of the Shelf and the Cabinet (`media::spine_fit`, docs/DESIGN.md "Media"). A Brick photo,
//! 2026-11-07 (Nintendo 64, Retro, 1024x768): "Conker's B…", "Diddy Kong…", "Mario Kar…" (three spines!), "Super Sma…",
//! "The Legend…" (twice): a player could not tell the spines apart. The real long names of Nintendo 64, SNES, GBA and DS
//! games on their shelves, at 640x480, 1024x768, 1240x1080 and 1280x720, in Barlow and in Retro (Pixelify), in Normal and
//! Large text: every spine shows its name whole or the part of it that tells it apart ("Ocarina of Time"), none ends in
//! "…", none is smaller than 16 px, and no two spines of one shelf read the same (`POCKETUI_SPINE_LINES`,
//! `media::note_spine`).
//!
//! `POCKETUI_TEST_BIN` runs another pocket-ui than the one just built (the "before" proof of a fix). With `--features brick`
//! the Brick's own start (`POCKETUI_BRICK_STANDALONE=1`) is checked at 1024x768 as well.
//!
//! Needs python3 (and, on Linux without a compositor, scripts/wayland-stub.py), as tests/spine_titles.rs.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};

include!("common/root.rs");

/// (system folder, the files on its shelf); the names are the ones the shelf shows ("The Legend of Zelda - Ocarina of Time").
const SHELVES: [(&str, &[&str]); 4] = [
    (
        "n64",
        &[
            "Banjo-Kazooie (USA).z64",
            "Conker's Bad Fur Day (USA).z64",
            "Diddy Kong Racing (USA).z64",
            "GoldenEye 007 (USA).z64",
            "Legend of Zelda, The - Majora's Mask (USA).z64",
            "Legend of Zelda, The - Ocarina of Time (USA).z64",
            "Mario Kart 64 (USA).z64",
            "Star Fox 64 (USA).z64",
            "Super Mario 64 (USA).z64",
            "Super Smash Bros. (USA).z64",
        ],
    ),
    (
        "snes",
        &[
            "Donkey Kong Country 3 - Dixie Kong's Double Trouble! (USA).sfc",
            "Donkey Kong Country 2 - Diddy's Kong Quest (USA).sfc",
            "Legend of Zelda, The - A Link to the Past (USA).sfc",
            "Super Mario All-Stars (USA).sfc",
            "Super Mario World 2 - Yoshi's Island (USA).sfc",
            "Super Metroid (USA).sfc",
            "Teenage Mutant Ninja Turtles IV - Turtles in Time (USA).sfc",
        ],
    ),
    (
        "gba",
        &[
            "Mario Kart - Super Circuit (USA).gba",
            "Mario & Luigi - Superstar Saga (USA).gba",
            "Super Mario Advance 4 - Super Mario Bros. 3 (USA).gba",
            "Legend of Zelda, The - The Minish Cap (USA).gba",
            "Metroid - Zero Mission (USA).gba",
            "Golden Sun - The Lost Age (USA).gba",
            "Castlevania - Harmony of Dissonance (USA).gba",
            "Castlevania - Aria of Sorrow (USA).gba",
        ],
    ),
    ("nds", &["Mario Kart DS (USA).nds", "Pokemon - Origin HeartGold (USA).nds", "Chrono Trigger (USA) (En,Fr).nds"]),
];

/// The name the shelf shows for a ROM's file: "Legend of Zelda, The - Ocarina of Time (USA).z64" is "The Legend of Zelda -
/// Ocarina of Time".
fn shown_name(file: &str) -> String {
    let stem = file.rsplit_once('.').map_or(file, |(s, _)| s);
    let stem = stem.split(" (").next().unwrap_or(stem);
    match stem.split_once(" - ") {
        Some((main, sub)) => format!("{} - {sub}", the_first(main)),
        None => the_first(stem),
    }
}

fn the_first(name: &str) -> String {
    match name.strip_suffix(", The") {
        Some(rest) => format!("The {rest}"),
        None => name.to_string(),
    }
}

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

/// The made-up library with only these games on its shelves.
fn library(tmp: &Path) -> PathBuf {
    let lib = tmp.join("lib");
    let ok = Command::new("python3")
        .env("POCKETUI_MOCK_NOW", FIXTURE_NOW)
        .args(["-I", &format!("{ROOT}/scripts/curator-fake-library.py"), lib.to_str().unwrap(), "dark"])
        .stdout(std::process::Stdio::null())
        .status()
        .expect("python3 for scripts/curator-fake-library.py");
    assert!(ok.success());
    for (system, files) in SHELVES {
        let dir = lib.join("roms").join(system);
        std::fs::create_dir_all(&dir).unwrap();
        for f in std::fs::read_dir(&dir).unwrap() {
            let f = f.unwrap().path();
            if f.is_file() {
                std::fs::remove_file(f).unwrap();
            }
        }
        for f in files {
            std::fs::write(dir.join(f), b"made up").unwrap();
        }
    }
    lib
}

#[derive(Clone)]
struct Job {
    system: &'static str,
    look: &'static str,
    size: &'static str,
    retro: bool,
    text: &'static str,
    brick: bool,
}

/// The spines one snapshot made: (title, size, lines, case).
type Spines = Vec<(String, u32, String, String)>;

fn shoot(tmp: &Path, lib: &Path, stub: &Option<PathBuf>, id: usize, j: &Job) -> Result<Spines, String> {
    let name = format!("{id}-{}-{}-{}-{}-{}-{}", j.system, j.look, j.size, j.retro, j.text, j.brick);
    let data = tmp.join(format!("data-{name}"));
    let status = Command::new("cp").args(["-r", lib.join("data").to_str().unwrap(), data.to_str().unwrap()]).status().unwrap();
    assert!(status.success());
    let settings = data.join("settings.json");
    let mut json: serde_json::Value = std::fs::read_to_string(&settings).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_else(|| serde_json::json!({}));
    json["theme"] = serde_json::json!("shelf");
    json["onboarded"] = serde_json::json!(true);
    std::fs::write(&settings, json.to_string()).unwrap();
    let (png, lines) = (tmp.join(format!("{name}.png")), tmp.join(format!("{name}.spines")));
    let bin = std::env::var_os("POCKETUI_TEST_BIN").map_or_else(|| PathBuf::from(env!("CARGO_BIN_EXE_pocket-ui")), PathBuf::from);
    let mut cmd = Command::new(bin);
    if let Some(s) = stub {
        cmd.env("WAYLAND_DISPLAY", s);
    }
    if j.retro {
        cmd.env("POCKETUI_RETRO", "cocoa");
    }
    if j.brick {
        cmd.env("POCKETUI_BRICK_STANDALONE", "1");
    }
    let state = format!("shelf:{}:{}:games:0", j.system, j.look);
    let out = cmd
        .args(["--snapshot", &state, png.to_str().unwrap()])
        .env("POCKETUI_MOCK", "1")
        .env("POCKETUI_MOCK_NOW", FIXTURE_NOW)
        .env("POCKETUI_LANG", "en")
        .env("POCKETUI_TEXT_SIZE", j.text)
        .env("POCKETUI_SNAPSHOT_SIZE", j.size)
        .env("POCKETUI_FONT", if j.retro { "retro" } else { "barlow" })
        .env("POCKETUI_SPINE_LINES", &lines)
        .env("POCKETUI_ES_SYSTEMS", lib.join("es_systems.cfg"))
        .env("POCKETUI_DATA", &data)
        .output()
        .expect("run pocket-ui");
    if !out.status.success() || !png.exists() {
        return Err(format!("{state} didn't render: {}", String::from_utf8_lossy(&out.stderr)));
    }
    let spines = std::fs::read_to_string(&lines)
        .unwrap_or_default()
        .lines()
        .filter_map(|l| {
            let mut p = l.splitn(4, '\t');
            Some((p.next()?.to_string(), p.next()?.parse().ok()?, p.next()?.to_string(), p.next().unwrap_or_default().to_string()))
        })
        .collect();
    let _ = std::fs::remove_dir_all(&data);
    let _ = std::fs::remove_file(&png);
    Ok(spines)
}

fn run(jobs: Vec<Job>) -> Vec<String> {
    let _tmp_guard = tempfile::tempdir().unwrap();
    let tmp = _tmp_guard.path().to_path_buf();
    let stub = wayland(&tmp);
    let lib = library(&tmp);
    let n = jobs.len();
    let stub_path = stub.as_ref().map(|s| s.1.clone());
    let work = Arc::new(Mutex::new(jobs.into_iter().enumerate().collect::<Vec<_>>()));
    let done: Arc<Mutex<Vec<(Job, Result<Spines, String>)>>> = Arc::new(Mutex::new(Vec::new()));
    let threads: Vec<_> = (0..std::thread::available_parallelism().map_or(4, |n| n.get().min(4)))
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
        let at = format!("{} {} {}{} {} text{}", j.system, j.look, j.size, if j.retro { " Retro" } else { "" }, j.text, if j.brick { " Brick" } else { "" });
        let spines = match shot {
            Ok(s) => s,
            Err(e) => {
                wrong.push(format!("{at}: {e}"));
                continue;
            }
        };
        // The games of this system's shelf (the Cabinet shows other systems' shelves too: those are not judged).
        let mine: Vec<String> = SHELVES.iter().filter(|(s, _)| *s == j.system).flat_map(|(_, files)| files.iter().map(|f| shown_name(f))).collect();
        let mut shown: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
        for (title, px, lines, _case) in spines {
            if !mine.contains(title) {
                continue;
            }
            // The Game Boy Advance's and the DS's labels at 1024x768 are 90 pixels long: a long name has no room whole
            // there, and a cut one is allowed to be the only way, if it tells itself apart (below).
            let thin = (j.system == "gba" || j.system == "nds") && j.size == "1024x768";
            if lines.split('|').any(|l| l.ends_with('…')) && !thin {
                wrong.push(format!("{at}: \"{title}\" cut short: \"{lines}\" at {px} px"));
            }
            if *px < 16 {
                wrong.push(format!("{at}: \"{title}\" at {px} px"));
            }
            shown.entry(title).or_default().push(lines);
        }
        let titles: std::collections::BTreeSet<&&str> = shown.keys().collect();
        if titles.len() < mine.len() {
            wrong.push(format!("{at}: only {} of {} spines made: {titles:?}", titles.len(), mine.len()));
        }
        // No two spines of one shelf read the same (a name that is the same on a bigger spine of the selected game is that
        // game's).
        let all: Vec<(&&str, &Vec<&str>)> = shown.iter().collect();
        for (i, (ta, la)) in all.iter().enumerate() {
            for (tb, lb) in &all[i + 1..] {
                if let Some(same) = la.iter().find(|l| lb.contains(*l)) {
                    wrong.push(format!("{at}: \"{ta}\" and \"{tb}\" both read \"{same}\""));
                }
            }
        }
    }
    wrong.sort();
    wrong.dedup();
    wrong
}

#[test]
fn every_spine_is_whole_or_tells_itself_apart() {
    let quick = std::env::var_os("POCKETUI_TEST_QUICK").is_some();
    let mut jobs = Vec::new();
    for (system, _) in SHELVES {
        for look in ["shelf", "cabinet"] {
            for size in if quick { vec!["1024x768"] } else { vec!["640x480", "1024x768", "1240x1080", "1280x720"] } {
                for retro in [false, true] {
                    for text in if quick { vec!["normal"] } else { vec!["normal", "large"] } {
                        jobs.push(Job { system, look, size, retro, text, brick: false });
                    }
                }
            }
        }
    }
    let wrong = run(jobs);
    assert!(wrong.is_empty(), "{} faults:\n{}", wrong.len(), wrong.join("\n"));
}

/// The Brick's own start (a card without NextUI, `brick_profile`): its shelf at 1024x768, the screen the photo came from.
#[cfg(feature = "brick")]
#[test]
fn every_spine_on_the_brick_is_whole_or_tells_itself_apart() {
    let mut jobs = Vec::new();
    for (system, _) in SHELVES {
        for look in ["shelf", "cabinet"] {
            for retro in [false, true] {
                for text in ["normal", "large"] {
                    jobs.push(Job { system, look, size: "1024x768", retro, text, brick: true });
                }
            }
        }
    }
    let wrong = run(jobs);
    assert!(wrong.is_empty(), "{} faults:\n{}", wrong.len(), wrong.join("\n"));
}
