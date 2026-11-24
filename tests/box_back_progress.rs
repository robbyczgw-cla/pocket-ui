//! The box's back with a line from the game's save (`case_knows`, `game_box::paint_back_body`): turned over, the line
//! ("6 badges · Charizard Lv. 45 · 31:12 h") stands right under the title, above the pictures and the description, seen
//! without scrolling, and only there (not again under "Your game"). Down still reaches the description and the player's
//! own rows. A game without a line keeps its back as before. Every panel size, Large text, Retro and Barlow, dark and
//! light. 14.11.: like a sticker with your save on the back. Needs python3 for the made-up library and, on
//! Linux without a compositor, scripts/wayland-stub.py.

#[path = "common/executable.rs"]
mod fixture_executable;

use std::path::{Path, PathBuf};
use std::process::Command;

include!("common/root.rs");

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

fn wayland(tmp: &Path) -> Option<Stub> {
    if !cfg!(target_os = "linux") || std::env::var_os("WAYLAND_DISPLAY").is_some_and(|w| !w.is_empty()) {
        return None;
    }
    let path = tmp.join("wayland-stub");
    let child = Command::new("python3").args(["-I", &format!("{ROOT}/scripts/wayland-stub.py"), path.to_str().unwrap()]).spawn().expect("python3");
    let mut stub = Stub { path, child };
    let until = std::time::Instant::now() + test_clock::timeout(std::time::Duration::from_secs(120));
    while std::time::Instant::now() < until {
        if stub.path.exists() {
            return Some(stub);
        }
        assert!(stub.child.try_wait().unwrap().is_none(), "scripts/wayland-stub.py exited at start-up");
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    panic!("scripts/wayland-stub.py didn't start");
}

fn library(dir: &Path, appearance: &str) {
    let status = Command::new("python3")
        .env("POCKETUI_MOCK_NOW", FIXTURE_NOW)
        .args(["-I", &format!("{ROOT}/scripts/curator-fake-library.py"), dir.to_str().unwrap(), appearance])
        .stdout(std::process::Stdio::null())
        .status()
        .expect("python3 for scripts/curator-fake-library.py");
    assert!(status.success());
    // No network: the description is the same on every run (as tests/info_page.rs). Publish once per fixture.
    let bin = dir.join("offline-bin");
    std::fs::create_dir_all(&bin).unwrap();
    fixture_executable::publish(&bin.join("curl"), "#!/bin/sh\nexit 7\n").unwrap();
}

/// One painted string: its top, its bottom, the string.
type Placed = (i32, i32, String);

struct Look<'a> {
    size: &'a str,
    text_size: &'a str,
    retro: &'a str,
}

fn copy_data(src: &Path, dst: &Path) {
    std::fs::create_dir_all(dst).unwrap();
    for entry in std::fs::read_dir(src).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.is_file() {
            std::fs::copy(&path, dst.join(entry.file_name())).unwrap();
        }
    }
}

/// Renders `state`; returns every string painted with its place, and the text painter's problems.
fn shot(lib: &Path, data: &Path, stub: Option<&Stub>, state: &str, look: &Look, progress: &str) -> (Vec<Placed>, String) {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let id = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let png = lib.join(format!("shot-{id}.png"));
    let (boxes, nav) = (lib.join(format!("text-{id}.box")), lib.join(format!("shot-{id}.nav")));
    let path = format!("{}:{}", lib.join("offline-bin").display(), std::env::var("PATH").unwrap_or_default());
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
    cmd.args(["--snapshot", state, png.to_str().unwrap()])
        .env("PATH", path)
        .env("POCKETUI_MOCK", "1")
        .env("POCKETUI_MOCK_NOW", FIXTURE_NOW)
        .env("POCKETUI_LANG", "en")
        .env("POCKETUI_TEXT_SIZE", look.text_size)
        .env("POCKETUI_SNAPSHOT_SIZE", look.size)
        .env("POCKETUI_RETRO", look.retro)
        .env("POCKETUI_SNAPSHOT_PROGRESS", progress)
        .env("POCKETUI_TEXT_BOXES", &boxes)
        .env("POCKETUI_NAV_PROBE", &nav)
        .env("POCKETUI_ES_SYSTEMS", lib.join("es_systems.cfg"))
        .env("POCKETUI_DATA", data);
    if let Some(s) = stub {
        cmd.env("WAYLAND_DISPLAY", &s.path);
    }
    let out = cmd.output().unwrap();
    assert!(out.status.success() && png.exists(), "{state} at {}: {}", look.size, String::from_utf8_lossy(&out.stderr));
    let placed = std::fs::read_to_string(&nav)
        .unwrap_or_default()
        .lines()
        .filter_map(|l| {
            let p: Vec<&str> = l.splitn(5, '\t').collect();
            (p.len() == 5 && !p[4].starts_with('‹')).then(|| Some((p[1].parse().ok()?, p[3].parse().ok()?, p[4].to_string())))?
        })
        .collect();
    let problems = std::fs::read_to_string(&boxes).unwrap_or_default();
    let _ = std::fs::remove_file(&png);
    let _ = std::fs::remove_file(&boxes);
    let _ = std::fs::remove_file(&nav);
    (placed, problems)
}

fn top_of(placed: &[Placed], what: &str) -> Option<i32> {
    placed.iter().filter(|(_, _, s)| s.contains(what)).map(|(t, _, _)| *t).min()
}

fn looks() -> Vec<Look<'static>> {
    let mut v = Vec::new();
    for retro in ["cocoa", "off"] {
        for size in ["640x480", "720x720", "1024x768", "1280x720", "1920x1080"] {
            v.push(Look { size, text_size: "normal", retro });
        }
        v.push(Look { size: "640x480", text_size: "large", retro });
    }
    v
}

#[test]
fn the_saves_line_stands_under_the_title_once_and_the_rest_stays_reachable() {
    let all_looks = looks();
    for appearance in ["dark", "light"] {
        let tmp = tempfile::tempdir().unwrap();
        library(tmp.path(), appearance);
        let stub = wayland(tmp.path());
        let mut jobs = Vec::new();
        for look in all_looks.iter().filter(|l| appearance == "dark" || l.size == "640x480" || l.size == "1920x1080") {
            for game in [("gba:emerald", "pokemon", "badges"), ("snes:link to the past", "zelda", "pendants")] {
                jobs.push((look, game));
            }
        }
        let next = std::sync::atomic::AtomicUsize::new(0);
        let workers = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1).clamp(1, 4).min(jobs.len().max(1));
        std::thread::scope(|scope| {
            for wid in 0..workers {
                let (tmp, stub, jobs, next) = (tmp.path(), stub.as_ref(), &jobs, &next);
                scope.spawn(move || {
                    let data = tmp.join(format!("data-w{wid}"));
                    copy_data(&tmp.join("data"), &data);
                    loop {
                        let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let Some(&(look, (game, progress, says))) = jobs.get(i) else { break };
                        let name = format!("{appearance} {} {} {}", look.size, look.text_size, look.retro);
                        let (back, problems) = shot(tmp, &data, stub, &format!("box:{game}:r"), look, progress);
                        let seen = back.iter().map(|p| p.2.as_str()).collect::<Vec<_>>().join("\n");
                        assert!(problems.trim().is_empty(), "{name} {game}: {problems}");
                        // Turned over, without a press more: the line is there, under the band's title and above everything
                        // else on the back (the pictures' captions or the hint for them, the description, the facts).
                        let line = top_of(&back, says).unwrap_or_else(|| panic!("{name} {game}: the save's line isn't on the turned back\n{seen}"));
                        let below = ["In the game", "Title screen", "description", "About the game", "Your game", "Status"];
                        for b in below {
                            if let Some(t) = top_of(&back, b) {
                                assert!(line < t, "{name} {game}: \"{b}\" ({t}) stands above the save's line ({line})\n{seen}");
                            }
                        }
                        let year_or_title = top_of(&back, "1991").or(top_of(&back, "2004")).or(top_of(&back, "Emerald")).or(top_of(&back, "Zelda"));
                        assert!(year_or_title.is_some_and(|t| t < line), "{name} {game}: the line isn't under the title\n{seen}");
                        // Down to the end: the description and the player's rows come, and the line is never painted twice
                        // (the small backs, where most has to scroll, and one 4:3 back; once per game is enough for the rest).
                        if appearance != "dark" || !(look.size == "640x480" || look.size == "1024x768") {
                            continue;
                        }
                        let mut all = seen.clone();
                        for n in 1..=24 {
                            let (page, problems) = shot(tmp, &data, stub, &format!("box:{game}:r{}", "d".repeat(n)), look, progress);
                            assert!(problems.trim().is_empty(), "{name} {game} Down × {n}: {problems}");
                            let shown: Vec<_> = page.iter().filter(|p| p.2.contains(says)).collect();
                            assert!(shown.len() <= 1, "{name} {game} Down × {n}: the line twice {shown:?}");
                            all += "\n";
                            all += &page.iter().map(|p| p.2.as_str()).collect::<Vec<_>>().join("\n");
                            if all.contains("Rating") && all.to_lowercase().contains("description") {
                                break;
                            }
                        }
                        assert!(all.to_lowercase().contains("description"), "{name} {game}: Down never reached the description\n{all}");
                        for row in ["Status", "Played", "Rating"] {
                            assert!(all.contains(row), "{name} {game}: Down never reached \"{row}\"\n{all}");
                        }
                    }
                });
            }
        });
    }
}

#[test]
fn a_game_without_a_line_keeps_its_back() {
    let tmp = tempfile::tempdir().unwrap();
    library(tmp.path(), "dark");
    let stub = wayland(tmp.path());
    let all_looks = looks();
    let next = std::sync::atomic::AtomicUsize::new(0);
    let workers = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1).clamp(1, 4).min(all_looks.len().max(1));
    std::thread::scope(|scope| {
        for wid in 0..workers {
            let (tmp, stub, all_looks, next) = (tmp.path(), stub.as_ref(), &all_looks, &next);
            scope.spawn(move || {
                let data = tmp.join(format!("data-w{wid}"));
                copy_data(&tmp.join("data"), &data);
                loop {
                    let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    let Some(look) = all_looks.get(i) else { break };
                    let name = format!("{} {} {}", look.size, look.text_size, look.retro);
                    // Metroid Fusion: no line from its save (the made-up one is a Pokémon game's), its description from the list.
                    let (back, problems) = shot(tmp, &data, stub, "box:gba:fusion:r", look, "pokemon");
                    let seen = back.iter().map(|p| p.2.as_str()).collect::<Vec<_>>().join("\n");
                    assert!(problems.trim().is_empty(), "{name}: {problems}");
                    assert!(!seen.contains("badges"), "{name}: a line on a game that has none\n{seen}");
                    // The description before the player's rows, as before (both where both are in view).
                    if let (Some(about), Some(your)) = (top_of(&back, "Samus"), top_of(&back, "Your game")) {
                        assert!(about < your, "{name}: the back's order changed\n{seen}");
                    }
                    assert!(top_of(&back, "Fusion").is_some(), "{name}: no title\n{seen}");
                }
            });
        }
    });
}
