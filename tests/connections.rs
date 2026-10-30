//! Connections and Rediscover (docs/CURATOR.md): on the info page (Minimal, Cards, Split) and the box's back (Shelf,
//! Cabinet) a game's developer, publisher, series and genre lead to the player's other games with the same one, and
//! Curator's list has Rediscover. Rendered with the real binary on the made-up library of
//! scripts/curator-fake-library.py, in English at Normal and German at Large, at a 720p-class screen and the H700's
//! 640x480: nothing may be cut off or overlap (`POCKETUI_TEXT_BOXES`, as tests/text_size.rs), a fact that leads nowhere
//! is never a way into an empty list, and B comes back to the card. Needs python3 and, on Linux without a compositor,
//! scripts/wayland-stub.py.

use std::path::{Path, PathBuf};
use std::process::Command;

const ROOT: &str = env!("CARGO_MANIFEST_DIR");

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

/// Renders `state`; returns the visible text and the text painter's problems.
fn shot(tmp: &Path, stub: Option<&Stub>, state: &str, size: &str, lang: &str, text_size: &str) -> (String, String) {
    let png = tmp.join("shot.png");
    let (boxes, visible) = (tmp.join("text.box"), tmp.join("visible.txt"));
    let _ = std::fs::remove_file(&boxes);
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
    cmd.args(["--snapshot", state, png.to_str().unwrap()])
        .env("POCKETUI_MOCK", "1")
        .env("POCKETUI_LANG", lang)
        .env("POCKETUI_TEXT_SIZE", text_size)
        .env("POCKETUI_SNAPSHOT_SIZE", size)
        .env("POCKETUI_TEXT_BOXES", &boxes)
        .env("POCKETUI_VISIBLE_TEXT", &visible)
        .env("POCKETUI_ES_SYSTEMS", tmp.join("es_systems.cfg"))
        .env("POCKETUI_DATA", tmp.join("data"));
    if let Some(s) = stub {
        cmd.env("WAYLAND_DISPLAY", &s.path);
    }
    let out = cmd.output().unwrap();
    assert!(out.status.success() && png.exists(), "{state} at {size}: {}", String::from_utf8_lossy(&out.stderr));
    (std::fs::read_to_string(&visible).unwrap_or_default(), std::fs::read_to_string(&boxes).unwrap_or_default())
}

fn fake_library(tmp: &Path, rediscover: bool) {
    let mut cmd = Command::new("python3");
    cmd.args(["-I", &format!("{ROOT}/scripts/curator-fake-library.py"), tmp.to_str().unwrap(), "dark"]).stdout(std::process::Stdio::null());
    if rediscover {
        cmd.env("POCKETUI_REDISCOVER", "1");
    }
    assert!(cmd.status().expect("python3 for scripts/curator-fake-library.py").success());
}

/// Where `what` stands in the visible text, for the order of a list.
fn at(seen: &str, what: &str) -> usize {
    seen.find(what).unwrap_or_else(|| panic!("{what} not on screen:\n{seen}"))
}

#[test]
fn a_cards_developer_and_series_open_the_players_own_games_and_b_comes_back() {
    let tmp = tempfile::tempdir().unwrap();
    fake_library(tmp.path(), false);
    let stub = wayland(tmp.path());
    // Burnout Revenge: Developer, Publisher, Genre, Series lead somewhere. Down picks the developer, A opens its list.
    let (seen, problems) = shot(tmp.path(), stub.as_ref(), "info:minimal:ps2:revenge:da", "1280x720", "en", "normal");
    assert!(seen.contains("More from Criterion Games"), "the developer's list\n{seen}");
    assert!(at(&seen, "Burnout 2 - Point of Impact") < at(&seen, "Burnout 3 - Takedown"), "in release order\n{seen}");
    assert!(!seen.contains("Burnout Revenge"), "the game itself isn't more of its developer's\n{seen}");
    assert!(!seen.contains("Burnout Dominator"), "another developer's Burnout\n{seen}");
    assert_eq!(seen.matches("Burnout 2 - Point of Impact").count(), 1, "one game on two systems shows once\n{seen}");
    assert!(problems.trim().is_empty(), "{problems}");
    // The series: every game in it, this one too, oldest first.
    let (seen, _) = shot(tmp.path(), stub.as_ref(), "info:minimal:ps2:revenge:dddda", "1280x720", "en", "normal");
    assert!(seen.contains("Burnout series, in release order"), "{seen}");
    assert!(at(&seen, "Burnout 3 - Takedown") < at(&seen, "Burnout Revenge") && at(&seen, "Burnout Revenge") < at(&seen, "Burnout Dominator"), "{seen}");
    // B on the list: the card again, as it was left.
    let (seen, _) = shot(tmp.path(), stub.as_ref(), "info:minimal:ps2:revenge:dab", "1280x720", "en", "normal");
    assert!(seen.contains("Burnout Revenge") && seen.contains("Details") && !seen.contains("More from"), "back on the card\n{seen}");
    // The box's back in the Shelf look: turned over, Down picks the developer.
    let (seen, _) = shot(tmp.path(), stub.as_ref(), "info:shelf:ps2:revenge:rda", "1280x720", "en", "normal");
    assert!(seen.contains("More from Criterion Games"), "from the box's back\n{seen}");
}

#[test]
fn every_look_and_size_shows_the_picked_fact_and_its_list_without_anything_cut_off() {
    let tmp = tempfile::tempdir().unwrap();
    fake_library(tmp.path(), false);
    let stub = wayland(tmp.path());
    for size in ["1280x720", "640x480"] {
        for (lang, text_size, title) in [("en", "normal", "Burnout series, in release order"), ("de", "large", "Reihe „Burnout“, nach Erscheinen")] {
            for (look, open) in [("minimal", "dddd"), ("cards", "dddd"), ("split", "dddd"), ("shelf", "rdddd"), ("cabinet", "rdddd")] {
                for keys in [open.to_string(), format!("{open}a")] {
                    let state = format!("info:{look}:ps2:revenge:{keys}");
                    let (seen, problems) = shot(tmp.path(), stub.as_ref(), &state, size, lang, text_size);
                    let name = format!("{state} {size} {lang} {text_size}");
                    assert!(problems.trim().is_empty(), "{name}: {problems}");
                    if keys.ends_with('a') {
                        assert!(seen.contains(title), "{name}: not the series' list\n{seen}");
                    } else {
                        let open = if lang == "de" { "Öffnen" } else { "Open" };
                        assert!(seen.contains(open) && seen.contains("Burnout"), "{name}: A doesn't open the picked fact\n{seen}");
                    }
                }
            }
        }
    }
}

#[test]
fn a_game_without_facts_has_nothing_to_pick_and_never_an_empty_list() {
    let tmp = tempfile::tempdir().unwrap();
    fake_library(tmp.path(), false);
    // A homebrew game nobody has data for.
    std::fs::write(tmp.path().join("roms/gba/Pocket Homebrew Demo.gba"), "x").unwrap();
    let stub = wayland(tmp.path());
    for look in ["minimal", "shelf"] {
        let keys = if look == "shelf" { "rd" } else { "d" };
        let (seen, problems) = shot(tmp.path(), stub.as_ref(), &format!("info:{look}:gba:pocket homebrew:{keys}"), "640x480", "en", "normal");
        assert!(seen.contains("Not in the offline game list"), "{look}: the facts' place says why it's empty\n{seen}");
        assert!(seen.contains("Play") && !seen.contains("Open"), "{look}: Down picked nothing, A still plays\n{seen}");
        assert!(problems.trim().is_empty(), "{look}: {problems}");
    }
}

#[test]
fn rediscover_shows_a_year_ago_today_first_and_only_when_there_is_something() {
    let tmp = tempfile::tempdir().unwrap();
    fake_library(tmp.path(), false);
    let stub = wayland(tmp.path());
    // Nothing played a year ago: no row in Curator's list.
    let (seen, _) = shot(tmp.path(), stub.as_ref(), "curator:hub:minimal:0", "1280x720", "en", "normal");
    assert!(seen.contains("Lens") && !seen.contains("Rediscover"), "{seen}");
    // EmulationStation's "last played": Metroid Fusion a year ago today, Golden Sun two years ago, Advance Wars 400 days.
    let tmp = tempfile::tempdir().unwrap();
    fake_library(tmp.path(), true);
    let (seen, _) = shot(tmp.path(), stub.as_ref(), "curator:hub:minimal:0", "1280x720", "en", "normal");
    assert!(seen.contains("Rediscover"), "{seen}");
    for (look, size, lang, text_size) in [("minimal", "1280x720", "en", "normal"), ("shelf", "640x480", "de", "large"), ("cabinet", "640x480", "de", "large"), ("split", "640x480", "en", "large")] {
        let (seen, problems) = shot(tmp.path(), stub.as_ref(), &format!("curator:rediscover:{look}"), size, lang, text_size);
        let name = format!("{look} {size} {lang} {text_size}");
        assert!(seen.contains("Metroid Fusion"), "{name}\n{seen}");
        assert!(problems.trim().is_empty(), "{name}: {problems}");
        if look == "minimal" {
            assert!(at(&seen, "Metroid Fusion") < at(&seen, "Advance Wars") && seen.contains("Golden Sun"), "{name}: a year ago today first\n{seen}");
        }
        if look == "shelf" {
            assert!(seen.contains("heute vor einem Jahr gespielt"), "{name}: why it is there\n{seen}");
        }
    }
}

/// A rescan completes while the fact's list is open (and the original card no longer blocks a library swap).
#[test]
fn a_connection_follows_a_rescan_and_back_resolves_the_origin_by_path() {
    let tmp = tempfile::tempdir().unwrap();
    fake_library(tmp.path(), false);
    let stub = wayland(tmp.path());
    let cfg = std::fs::read_to_string(tmp.path().join("es_systems.cfg")).unwrap();
    // Same number of systems, different order, and one new game before the card's old game index.
    // The zip is invisible to the initial configuration and becomes visible in the rescan.
    std::fs::write(tmp.path().join("roms/ps2/AAA New Game.zip"), "x").unwrap();
    let reordered = cfg
        .replace("<fullname>PlayStation 2</fullname>", "<fullname>ZZZ PlayStation 2</fullname>")
        .replace("<extension>.iso .chd</extension>", "<extension>.iso .chd .zip</extension>");
    std::fs::write(tmp.path().join("rescan.cfg"), reordered).unwrap();
    let (seen, _) = shot(tmp.path(), stub.as_ref(), "curator:connection-rescan:ps2:revenge", "640x480", "en", "normal");
    assert!(
        seen.contains("Burnout Revenge") && seen.contains("Details"),
        "same card after both indices move\n{seen}"
    );
    // Drop the entire originating system. The fixture also asserts Home and no stale card after B.
    let gone = cfg.lines().filter(|line| !line.contains("<name>ps2</name>")).collect::<Vec<_>>().join("\n");
    std::fs::write(tmp.path().join("rescan.cfg"), gone).unwrap();
    let (seen, _) = shot(tmp.path(), stub.as_ref(), "curator:connection-rescan:ps2:revenge", "640x480", "en", "normal");
    assert!(!seen.contains("Details"), "no card for a removed origin\n{seen}");
}
