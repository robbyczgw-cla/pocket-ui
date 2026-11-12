//! The big name of the selected game, whole (`title_block`, docs/DESIGN.md "A game's name"). A Brick photo, 2026-11-07
//! (Retro, Shelf look, 1024x768): "The Legend of Zelda - Ocarina of Time" stood cut after two lines with free room under it.
//! The real long names of five N64, SNES, GBA and DS games, in every look that has a big name (Shelf, Cabinet, Cards,
//! Split, Easy), at 640x480, 1024x768 and 1240x1080, in Barlow and Retro (Pixelify), in Normal and Large text, in English,
//! German and Spanish:
//! - the name's lines, put together, are the whole name (its subtitle on a line of its own, smaller, without the dash), none
//!   ends in "…";
//! - nothing the picture paints runs into another string, a hint bar or an edge;
//! - what stands under the name (the meta line) is where it is for every other game of that look: the name's height is
//!   reserved, so the shelf does not jump while a player scrolls it (Easy: for names of up to two lines).
//!
//! Needs python3 (and, on Linux without a compositor, scripts/wayland-stub.py), as tests/spine_titles.rs.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};

include!("common/root.rs");

/// (system folder, file, the name as the shelf shows it, its row in the system's list, its row in Easy's). The first five
/// are the long names; each of the others shares a system with one of them and is short: the meta line has to stand still
/// from one game of a system to the next.
const GAMES: [(&str, &str, &str, usize, usize); 8] = [
    ("n64", "Legend of Zelda, The - Ocarina of Time (USA).z64", "The Legend of Zelda - Ocarina of Time", 1, 3),
    ("snes", "Donkey Kong Country 3 - Dixie Kong's Double Trouble! (USA).sfc", "Donkey Kong Country 3 - Dixie Kong's Double Trouble!", 0, 6),
    ("gba", "Super Mario Advance 4 - Super Mario Bros. 3 (USA).gba", "Super Mario Advance 4 - Super Mario Bros. 3", 1, 1),
    ("gba", "Castlevania - Aria of Sorrow (USA).gba", "Castlevania - Aria of Sorrow", 0, 0),
    ("nds", "Pokemon - Origin HeartGold (USA).nds", "Pokemon - Origin HeartGold", 1, 5),
    ("n64", "Mario Kart 64 (USA).z64", "Mario Kart 64", 0, 2),
    ("snes", "EarthBound (USA).sfc", "EarthBound", 1, 7),
    ("nds", "Mario Kart DS (USA).nds", "Mario Kart DS", 0, 4),
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

/// The made-up library with only these five games on its shelves, one folder for each system.
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
    for (system, file, _, _, _) in GAMES {
        std::fs::write(lib.join("roms").join(system).join(file), b"made up").unwrap();
    }
    lib
}

#[derive(Clone)]
struct Job {
    look: &'static str,
    size: &'static str,
    retro: bool,
    text: &'static str,
    lang: &'static str,
    game: usize,
    info: bool,
    /// Home with this row selected and the Zelda game the first of the recent ones (the "Continue" row), open when `frozen`.
    home: Option<usize>,
    frozen: bool,
}

struct Shot {
    /// (size, main or sub, baseline, text) of each line of the name.
    name: Vec<(u32, bool, i32, String)>,
    /// The meta line's top, where the page has one by this text.
    meta_y: Option<i32>,
    faults: Vec<String>,
    probe: String,
}

fn shoot(tmp: &Path, lib: &Path, stub: &Option<PathBuf>, id: usize, j: &Job) -> Result<Shot, String> {
    let name = format!("{id}-{}-{}-{}-{}-{}-{}", j.look, j.size, j.retro, j.text, j.lang, j.game);
    let data = tmp.join(format!("data-{name}"));
    let status = Command::new("cp").args(["-r", lib.join("data").to_str().unwrap(), data.to_str().unwrap()]).status().unwrap();
    assert!(status.success());
    let settings = data.join("settings.json");
    let mut json: serde_json::Value = std::fs::read_to_string(&settings).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_else(|| serde_json::json!({}));
    json["theme"] = serde_json::json!(if j.look == "easy" { "big" } else { j.look });
    if j.info { json["details_page"] = serde_json::json!(true); }
    json["onboarded"] = serde_json::json!(true);
    std::fs::write(&settings, json.to_string()).unwrap();
    let (g_system, _, _, row, easy_row) = GAMES[j.game];
    if j.home.is_some() {
        // The long N64 game was the last played: the Continue row.
        let recents = serde_json::json!([{ "system": "n64", "name": GAMES[0].2, "path": lib.join("roms/n64").join(GAMES[0].1),
            "last_played": FIXTURE_NOW.parse::<u64>().unwrap() - 16 * 3600, "play_secs": 40 }]);
        std::fs::write(data.join("recents.json"), recents.to_string()).unwrap();
    }
    let state = if let Some(row) = j.home { format!("home:{row}") } else if j.info { format!("curator:card:{g_system}:{}", GAMES[j.game].2.to_lowercase()) } else if j.look == "easy" { format!("big-shelf:{easy_row}") } else { format!("shelf:{g_system}:{}:games:{row}", j.look) };
    let (png, boxes, blocks, probe) = (tmp.join(format!("{name}.png")), tmp.join(format!("{name}.box")), tmp.join(format!("{name}.title")), tmp.join(format!("{name}.probe")));
    let bin = std::env::var_os("POCKETUI_NAV_BINARY").map_or_else(|| PathBuf::from(env!("CARGO_BIN_EXE_pocket-ui")), PathBuf::from);
    let mut cmd = Command::new(bin);
    if j.info && cfg!(feature = "brick") {
        cmd.env("POCKETUI_DEVICE", "brick").env("POCKETUI_BRICK_STANDALONE", "1");
    }
    if let Some(s) = stub {
        cmd.env("WAYLAND_DISPLAY", s);
    }
    if j.retro {
        cmd.env("POCKETUI_RETRO", "cocoa");
    }
    if j.frozen {
        cmd.env("POCKETUI_SNAPSHOT_FROZEN", "1");
    }
    let out = cmd
        .args(["--snapshot", &state, png.to_str().unwrap()])
        .env("POCKETUI_MOCK", "1").env("POCKETUI_MOCK_NOW", FIXTURE_NOW)
        .env("POCKETUI_LANG", j.lang)
        .env("POCKETUI_TEXT_SIZE", j.text)
        .env("POCKETUI_SNAPSHOT_SIZE", j.size)
        .env("POCKETUI_FONT", if j.retro { "retro" } else { "barlow" })
        .env("POCKETUI_TEXT_BOXES", &boxes)
        .env("POCKETUI_TITLE_BLOCKS", &blocks)
        .env("POCKETUI_NAV_PROBE", &probe)
        .env("POCKETUI_ES_SYSTEMS", lib.join("es_systems.cfg"))
        .env("POCKETUI_DATA", &data)
        .output()
        .expect("run pocket-ui");
    if !out.status.success() || !png.exists() {
        return Err(format!("{state} didn't render: {}", String::from_utf8_lossy(&out.stderr)));
    }
    let name_lines = std::fs::read_to_string(&blocks).unwrap_or_default();
    let name = name_lines
        .lines()
        .filter_map(|l| {
            let mut p = l.splitn(4, '\t');
            Some((p.next()?.parse().ok()?, p.next()? == "sub", p.next()?.parse().ok()?, p.next()?.to_string()))
        })
        .collect();
    // The meta line: "Not played yet" (and what the German and Spanish tables make of it) stands under the name on every look
    // but Easy, whose lines under the name come from what is known of the game.
    let probe = std::fs::read_to_string(&probe).unwrap_or_default();
    let meta_y = probe.lines().find_map(|l| {
        let p: Vec<&str> = l.splitn(5, '\t').collect();
        (p.len() == 5 && matches!(p[4], "Not played yet" | "Noch nicht gespielt" | "Aún sin jugar" | "Todavía sin jugar" | "Sin jugar aún")).then(|| p[1].parse().ok()).flatten()
    });
    let faults = std::fs::read_to_string(&boxes).unwrap_or_default().lines().filter(|l| !l.is_empty()).map(str::to_string).collect();
    let _ = std::fs::remove_dir_all(&data);
    let _ = std::fs::remove_file(&png);
    Ok(Shot { name, meta_y, faults, probe })
}

/// The words of a name without its dash or colon, as the lines of its block have them.
fn words(s: &str) -> Vec<String> {
    s.replace(" - ", " ").replace(": ", " ").split_whitespace().map(str::to_string).collect()
}

#[test]
fn long_game_names_are_whole_beside_their_cover_in_every_look() {
    let _tmp_guard = tempfile::tempdir().unwrap();
    let tmp = _tmp_guard.path().to_path_buf();
    let stub = wayland(&tmp);
    let lib = library(&tmp);
    let quick = std::env::var_os("POCKETUI_TEST_QUICK").is_some();
    let mut jobs = Vec::new();
    for look in ["shelf", "cabinet", "split", "easy"] {
        for size in if quick { vec!["1024x768"] } else { vec!["640x480", "1024x768", "1240x1080"] } {
            for retro in [false, true] {
                for text in if quick { vec!["normal"] } else { vec!["normal", "large"] } {
                    for lang in if quick { vec!["en"] } else { vec!["en", "de", "es"] } {
                        for game in 0..GAMES.len() {
                            jobs.push(Job { look, size, retro, text, lang, game, info: false, home: None, frozen: false });
                        }
                    }
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
        let label = format!("{} at {}, {}, {} text, {}, {}", j.look, j.size, if j.retro { "Retro" } else { "Barlow" }, j.text, j.lang, GAMES[j.game].2);
        let shot = match shot {
            Ok(s) => s,
            Err(e) => {
                wrong.push(format!("{label}: {e}"));
                continue;
            }
        };
        let title = GAMES[j.game].2;
        let shown: Vec<String> = shot.name.iter().flat_map(|l| words(&l.3)).collect();
        if shown != words(title) {
            wrong.push(format!("{label}: the name reads {:?}", shot.name.iter().map(|l| l.3.as_str()).collect::<Vec<_>>()));
        }
        // Every look but Easy puts the name on its own lines; no other fault of the page counts here but strings that run into
        // one another or off the picture.
        for f in &shot.faults {
            if matches!(f.chars().next(), Some('O' | 'T' | 'V' | 'E')) {
                wrong.push(format!("{label}: {}", f.replace('\t', " | ")));
            }
        }
    }
    // The meta line stands still: one place for every game of a system (its cover is the same size, so the name's lines
    // never move what is under them), for a look, a size, a font, a text size and a language.
    for look in ["shelf", "cabinet", "split"] {
        for size in if quick { vec!["1024x768"] } else { vec!["640x480", "1024x768", "1240x1080"] } {
            for retro in [false, true] {
                for text in if quick { vec!["normal"] } else { vec!["normal", "large"] } {
                    for system in ["n64", "snes", "gba", "nds"] {
                        let mut ys: Vec<(i32, &str)> = done
                            .iter()
                            .filter(|(j, _)| j.look == look && j.size == size && j.retro == retro && j.text == text && j.lang == "en" && GAMES[j.game].0 == system)
                            .filter_map(|(j, s)| Some((s.as_ref().ok()?.meta_y?, GAMES[j.game].2)))
                            .collect();
                        ys.sort();
                        let games = GAMES.iter().filter(|g| g.0 == system).count();
                        if ys.len() < games {
                            wrong.push(format!("{look} at {size}, {system}: no meta line found for {} of the games", games - ys.len()));
                        } else if ys.first().map(|y| y.0) != ys.last().map(|y| y.0) {
                            wrong.push(format!("{look} at {size}, {}, {text} text, {system}: the meta line moves between games: {ys:?}", if retro { "Retro" } else { "Barlow" }));
                        }
                    }
                }
            }
        }
    }
    wrong.sort();
    assert!(wrong.is_empty(), "{} problems:\n{}", wrong.len(), wrong.join("\n"));
}


/// Both Details headers use the same title rule as the shelf. DKC3 previously lost its subtitle at
/// Retro/Large, even though a smaller main title and subtitle fit in the existing heading band.
#[test]
fn info_titles_031_are_whole_above_the_fixed_facts() {
    let tmp = tempfile::tempdir().unwrap();
    let stub = wayland(tmp.path());
    let lib = library(tmp.path());
    let mut jobs = Vec::new();
    // Two actual painters: compact info (Split), and the full Details page (Shelf with the box off).
    // The other looks route to these same painters; check their routes once in the worst case below.
    for look in ["split", "shelf"] {
        for size in ["640x480", "1024x768", "1240x1080", "1280x720"] {
            for retro in [false, true] {
                for text in if cfg!(feature = "brick") { vec!["normal", "medium", "large"] } else { vec!["normal", "large"] } {
                    for lang in ["en", "de", "es"] {
                        for game in 0..GAMES.len() {
                            jobs.push(Job { look, size, retro, text, lang, game, info: true, home: None, frozen: false });
                        }
                    }
                }
            }
        }
    }
    for look in ["easy", "cabinet"] {
        for game in 0..GAMES.len() {
            jobs.push(Job { look, size: "1024x768", retro: true, text: "large", lang: "es", game, info: true, home: None, frozen: false });
        }
    }
    let n = jobs.len();
    let work = Arc::new(Mutex::new(jobs.into_iter().enumerate().collect::<Vec<_>>()));
    let wrong = Arc::new(Mutex::new(Vec::new()));
    let threads: Vec<_> = (0..std::thread::available_parallelism().map_or(4, |n| n.get().min(6)))
        .map(|_| {
            let (work, wrong, tmp, lib, stub) = (work.clone(), wrong.clone(), tmp.path().to_path_buf(), lib.clone(), stub.as_ref().map(|s| s.1.clone()));
            std::thread::spawn(move || loop {
                let Some((id, job)) = work.lock().unwrap().pop() else { break };
                let label = format!("{} {} {} {} {} {}", job.look, job.size, job.retro, job.text, job.lang, GAMES[job.game].2);
                let shot = match shoot(&tmp, &lib, &stub, id, &job) {
                    Ok(s) => s,
                    Err(e) => { wrong.lock().unwrap().push(format!("{label}: {e}")); continue; }
                };
                let rows: Vec<(i32, i32, i32, i32, &str)> = shot.probe.lines().filter_map(|line| {
                    let p: Vec<_> = line.splitn(5, '\t').collect();
                    if p.len() != 5 { return None; }
                    Some((p[0].parse().ok()?, p[1].parse().ok()?, p[2].parse().ok()?, p[3].parse().ok()?, p[4]))
                }).collect();
                let system = match GAMES[job.game].0 { "snes" => "Super Nintendo", "n64" => "Nintendo 64", "gba" => "Game Boy Advance", "nds" => "Nintendo DS", _ => unreachable!() };
                let Some(meta) = rows.iter().filter(|r| r.4.split(" · ").next() == Some(system)).max_by_key(|r| r.0) else {
                    wrong.lock().unwrap().push(format!("{label}: no fixed system/year line: {}", shot.probe)); continue;
                };
                let mut header: Vec<_> = rows.iter().filter(|r| (r.0 - meta.0).abs() <= 12 && r.3 <= meta.1 && !r.4.starts_with('‹')).collect();
                header.sort_by_key(|r| r.1);
                let shown = words(&header.iter().map(|r| r.4).collect::<Vec<_>>().join(" "));
                let title = GAMES[job.game].2;
                if shown != words(title) {
                    wrong.lock().unwrap().push(format!("{label}: visible heading {:?}, expected {title}", header.iter().map(|r| r.4).collect::<Vec<_>>()));
                }
                if header.windows(2).any(|p| p[0].3 > p[1].1 + 2) || header.last().is_none_or(|r| r.3 + 4 > meta.1) {
                    wrong.lock().unwrap().push(format!("{label}: heading overlaps another line or its fixed system/year line"));
                }
                let blocks = shot.name.iter().flat_map(|l| words(&l.3)).collect::<Vec<_>>();
                if blocks != words(title) || shot.name.len() > 3 || (shot.name.len() > 1 && title.contains(" - ") && !shot.name.iter().any(|l| l.1 && l.0 < shot.name[0].0)) {
                    wrong.lock().unwrap().push(format!("{label}: title rule not met: {:?}", shot.name));
                }
            })
        }).collect();
    for thread in threads { thread.join().unwrap(); }
    let mut wrong = wrong.lock().unwrap();
    wrong.sort();
    assert!(wrong.is_empty(), "{} problems in {n} info renders:\n{}", wrong.len(), wrong.join("\n"));
}

/// The Continue row on Home (a long name, a game left open) in every look: the meta line and the word "Continue" of an open
/// game stand clear of the name's lines and of each other (Brick photo 2026-11-08: Cabinet, Retro, 1024x768, the meta line ran
/// into the word). Row 0 and 1 are tried: which of them holds the recent game depends on the look (Just Play stands first in
/// some). Open or not, with the recent game or not (then the row holds another game and the name is its own).
#[test]
fn continue_row_031_meta_line_and_open_word_stay_clear_of_the_name() {
    let _tmp_guard = tempfile::tempdir().unwrap();
    let tmp = _tmp_guard.path().to_path_buf();
    let stub = wayland(&tmp);
    let lib = library(&tmp);
    let quick = std::env::var_os("POCKETUI_TEST_QUICK").is_some();
    let mut jobs = Vec::new();
    for look in ["cabinet", "shelf", "split", "easy"] {
        for size in if quick { vec!["1024x768"] } else { vec!["640x480", "1024x768", "1240x1080", "1280x720", "1920x1080"] } {
            for retro in [false, true] {
                for text in if quick { vec!["normal"] } else if cfg!(feature = "brick") { vec!["normal", "medium", "large"] } else { vec!["normal", "large"] } {
                    for lang in if quick { vec!["en"] } else { vec!["en", "de", "es"] } {
                        for home in [0, 1] {
                            for frozen in [true, false] {
                                jobs.push(Job { look, size, retro, text, lang, game: 0, info: false, home: Some(home), frozen });
                            }
                        }
                    }
                }
            }
        }
    }
    let n = jobs.len();
    let stub_path = stub.as_ref().map(|s| s.1.clone());
    let work = Arc::new(Mutex::new(jobs.into_iter().enumerate().collect::<Vec<_>>()));
    let wrong = Arc::new(Mutex::new(Vec::new()));
    let seen = Arc::new(Mutex::new(std::collections::BTreeSet::new()));
    let threads: Vec<_> = (0..std::thread::available_parallelism().map_or(4, |n| n.get().min(6)))
        .map(|_| {
            let (work, wrong, seen, tmp, lib, stub) = (work.clone(), wrong.clone(), seen.clone(), tmp.clone(), lib.clone(), stub_path.clone());
            std::thread::spawn(move || loop {
                let Some((id, j)) = work.lock().unwrap().pop() else { break };
                let label = format!("{} at {}, {}, {} text, {}, row {}{}", j.look, j.size, if j.retro { "Retro" } else { "Barlow" }, j.text, j.lang, j.home.unwrap(), if j.frozen { ", open" } else { "" });
                match shoot(&tmp, &lib, &stub, id, &j) {
                    Err(e) => wrong.lock().unwrap().push(format!("{label}: {e}")),
                    Ok(shot) => {
                        // Only a row that shows the long recent game counts here (the other holds Just Play, a page of its own).
                        if !(shot.name.iter().any(|l| l.3.contains("Zelda") || l.3.contains("Ocarina")) || shot.probe.contains("Ocarina")) {
                            continue;
                        }
                        seen.lock().unwrap().insert((j.look, j.size, j.retro, j.text, j.lang, j.frozen));
                        for f in shot.faults.iter().filter(|f| matches!(f.chars().next(), Some('O' | 'T' | 'V' | 'E'))) {
                            wrong.lock().unwrap().push(format!("{label}: {}", f.replace('\t', " | ")));
                        }
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
    // Every look shows the long recent game on one of the rows (else the test would look at the wrong page).
    for look in ["cabinet", "split"] {
        if !seen.iter().any(|s| s.0 == look) {
            wrong.push(format!("{look}: the recent game's name was on neither row"));
        }
    }
    wrong.sort();
    assert!(wrong.is_empty(), "{} problems in {n} renders:\n{}", wrong.len(), wrong.join("\n"));
}

/// The "Just Play" entry on Home in every look but Easy (which has none), at every size, text size, font and language (Brick photo 2026-11-08, Cards,
/// Retro, Large text, 1024x768: the card's name "Just" ran into its middle line "One pr…"). Rows 0 to 3 are tried: which of
/// them holds Just Play depends on the look and on what else Home offers. Whatever shows Just Play (the card, its
/// neighbours' texts, its row) has no string over another, none closer than a space, none cut by an edge, and its short line
/// "One press" ("Ein Druck", "Un toque") is not cut with "…".
#[test]
fn just_play_031_texts_stay_clear_in_every_look() {
    let _tmp_guard = tempfile::tempdir().unwrap();
    let tmp = _tmp_guard.path().to_path_buf();
    let stub = wayland(&tmp);
    let lib = library(&tmp);
    let quick = std::env::var_os("POCKETUI_TEST_QUICK").is_some();
    let mut jobs = Vec::new();
    for look in ["cabinet", "shelf", "split"] {
        for size in if quick { vec!["1024x768"] } else { vec!["640x480", "1024x768", "1240x1080", "1280x720", "1920x1080"] } {
            for retro in [false, true] {
                for text in if quick { vec!["large"] } else if cfg!(feature = "brick") { vec!["normal", "medium", "large"] } else { vec!["normal", "large"] } {
                    for lang in if quick { vec!["en"] } else { vec!["en", "de", "es"] } {
                        for home in 0..4 {
                            jobs.push(Job { look, size, retro, text, lang, game: 0, info: false, home: Some(home), frozen: false });
                        }
                    }
                }
            }
        }
    }
    let n = jobs.len();
    let stub_path = stub.as_ref().map(|s| s.1.clone());
    let work = Arc::new(Mutex::new(jobs.into_iter().enumerate().collect::<Vec<_>>()));
    let wrong = Arc::new(Mutex::new(Vec::new()));
    let seen = Arc::new(Mutex::new(std::collections::BTreeSet::new()));
    let threads: Vec<_> = (0..std::thread::available_parallelism().map_or(4, |n| n.get().min(6)))
        .map(|_| {
            let (work, wrong, seen, tmp, lib, stub) = (work.clone(), wrong.clone(), seen.clone(), tmp.clone(), lib.clone(), stub_path.clone());
            std::thread::spawn(move || loop {
                let Some((id, j)) = work.lock().unwrap().pop() else { break };
                let label = format!("{} at {}, {}, {} text, {}, row {}", j.look, j.size, if j.retro { "Retro" } else { "Barlow" }, j.text, j.lang, j.home.unwrap());
                match shoot(&tmp, &lib, &stub, id, &j) {
                    Err(e) => wrong.lock().unwrap().push(format!("{label}: {e}")),
                    Ok(shot) => {
                        // The name and the short line as painted: whole or, on a narrow card, one word to a line.
                        const NAMES: [&str; 9] = ["Just Play", "Einfach spielen", "Jugar ya", "Just", "Play", "Einfach", "spielen", "Jugar", "ya"];
                        const SIDES: [&str; 3] = ["One press", "Ein Druck", "Un toque"];
                        let strings: Vec<&str> = shot.probe.lines().filter_map(|l| l.splitn(5, '\t').nth(4)).collect();
                        if !strings.iter().any(|s| NAMES.contains(s) || SIDES.iter().any(|d| d.starts_with(s.trim_end_matches('…')) && s.chars().count() >= 4)) {
                            continue;
                        }
                        seen.lock().unwrap().insert((j.look, j.size, j.retro, j.text, j.lang));
                        for f in shot.faults.iter().filter(|f| matches!(f.chars().next(), Some('O' | 'T' | 'V' | 'E'))) {
                            // Faults of another row's text (the Continue game beside it) are the Continue test's business.
                            if f.split('\t').skip(1).any(|s| NAMES.contains(&s) || SIDES.iter().any(|d| d.starts_with(s.trim_end_matches('…')) && s.chars().count() >= 4)) {
                                wrong.lock().unwrap().push(format!("{label}: {}", f.replace('\t', " | ")));
                            }
                        }
                        // The short line cut short: "One pr…" (a prefix of the line, ending in an ellipsis).
                        for s in &strings {
                            if let Some(head) = s.strip_suffix('…') {
                                if SIDES.iter().any(|side| side.starts_with(head) && head.chars().count() >= 3) {
                                    wrong.lock().unwrap().push(format!("{label}: the short line is cut: {s:?}"));
                                }
                            }
                        }
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
    // Every look shows Just Play on one of the four rows (else the test would look at the wrong page).
    for look in ["cabinet", "shelf", "split"] {
        if !seen.iter().any(|s| s.0 == look) {
            wrong.push(format!("{look}: Just Play was on none of the rows"));
        }
    }
    wrong.sort();
    assert!(wrong.is_empty(), "{} problems in {n} renders:\n{}", wrong.len(), wrong.join("\n"));
}
