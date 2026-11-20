//! A handheld without a Wi-Fi chip (the RG28XX, the RG35XX 2024, the R36S: `device::Device::has_wifi`), page by page
//! (fable52 gap #6, 15.11.2026): no page tells the player to check, connect or turn on Wi-Fi, or to do something over
//! Wi-Fi, where there is none. A page that needs a network says what to do instead (a computer, the card) or that this
//! device has no Wi-Fi. Commit 43f91d2b2 fixed Updates alone ("Couldn't reach the update server. Check Wi-Fi." on the
//! R36S); every other page still said it.
//!
//! The real program paints each state of `tests/i18n-states.txt` (the snapshot suite's list of every page) as on an
//! RG28XX in English, and the words it painted are read back (`POCKETUI_VISIBLE_TEXT`). The merge gate runs the pages
//! that speak of Wi-Fi, downloads or being online (`GATED`); the whole list is a release check. The D-pad walk of the
//! same rule is tests/nav_explorer.rs on the R36S and the RG28XX (`no_wifi_words`).
//!
//! Needs python3 for the made-up library and, on Linux without a compositor, for scripts/wayland-stub.py.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};

include!("common/root.rs");

/// The pages the merge gate paints: each one that names Wi-Fi, a download or being online somewhere.
const GATED: &[&str] = &[
    "settings:0",
    "settings:7:3",
    "setup:games",
    "tailscale",
    "tailscale:offline",
    "tailscale:offline-note",
    "web",
    "web:off",
    "web:waiting",
    "pockets:on",
    "pockets:nobody",
    "pockets:what",
    "achievements",
    "achievements:offline",
    "starter-home:wifi-off",
    "starter:3:offline",
    "setup:extras",
    "setup:extras:dd",
    "ports-empty:a",
    "setup:ports-shared:rx",
    "setup:ports-get-offline",
    "tour:quick",
    "tour:add_games",
    "sys:time",
    "sys:time:2",
    "sys:time-wrong",
    "sys:time-unsynced",
    "curator:card:gba:aria:lookup=offline",
    "cheats-offline",
    "report:saved",
    "setup:remote",
    "sync:off",
    // RomM (Settings › Games › RomM): a server out of reach says what this handheld lacks, never "Check Wi-Fi".
    "romm:error",
];

/// States a handheld without Wi-Fi never shows: the snapshot makes up a radio and networks for them (the Wi-Fi page's
/// and the first start's Wi-Fi states, the quick settings' Wi-Fi tile), or a page only a Wi-Fi handheld offers (Send from
/// your phone, `phone_upload_page::offered`). Updates is left to its own test (`update::no_wifi_for`, update-from-card).
const NOT_ON_SUCH_A_HANDHELD: &[&str] = &["setup:wifi", "setup:first-wifi", "sys:quick-wifi-", "setup:phone=", "updates:"];

struct Stub(std::process::Child);

impl Drop for Stub {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// What a line of text on a handheld without Wi-Fi may say about Wi-Fi: that there is none.
fn tells_to_use_wifi(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut rest = text;
    let mut before = String::new();
    while let Some(i) = rest.find("Wi-Fi") {
        let head = format!("{before}{}", &rest[..i]);
        let lower = head.to_lowercase();
        let after = rest[i + "Wi-Fi".len()..].trim_start();
        // "This device has no Wi-Fi", "Without Wi-Fi", the Connections row "Wi-Fi  None on this device".
        // A report's privacy line ("Left out: Wi-Fi names and passwords") asks nothing of the player.
        let absent = ["no ", "without "].iter().any(|w| lower.ends_with(w)) || after.starts_with("None on this device") || after.starts_with("names and passwords");
        if !absent {
            let start = head.char_indices().rev().nth(40).map_or(0, |(j, _)| j);
            let tail_end = rest[i..].char_indices().nth(40).map_or(rest.len() - i, |(j, _)| j);
            found.push(format!("…{}{}…", &head[start..], &rest[i..i + tail_end]));
        }
        before = format!("{head}Wi-Fi");
        rest = &rest[i + "Wi-Fi".len()..];
    }
    found
}

fn states(all: bool) -> Vec<String> {
    let list: Vec<String> = std::fs::read_to_string(Path::new(&ROOT).join("tests/i18n-states.txt"))
        .expect("tests/i18n-states.txt")
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#') && !l.starts_with("safe ") && !l.contains(":device="))
        .filter(|l| !NOT_ON_SUCH_A_HANDHELD.iter().any(|p| l.starts_with(p)))
        .map(String::from)
        .collect();
    if all {
        return list;
    }
    GATED.iter().map(|s| s.to_string()).collect()
}

/// Paints `states` as on `device` in English, eight at a time: per state, the words painted (joined in one line) or why
/// it didn't paint.
fn paint(device: &str, states: &[String]) -> Vec<(String, Result<String, String>)> {
    let dir = tempfile::tempdir().unwrap();
    let tmp = dir.path().to_path_buf();
    let ok = Command::new("python3").env("POCKETUI_MOCK_NOW", FIXTURE_NOW)
        .args(["-I", &format!("{ROOT}/scripts/curator-fake-library.py"), tmp.to_str().unwrap(), "dark"])
        .stdout(std::process::Stdio::null())
        .status()
        .expect("python3 for scripts/curator-fake-library.py");
    assert!(ok.success());
    let _stub = (cfg!(target_os = "linux") && std::env::var_os("WAYLAND_DISPLAY").is_none_or(|w| w.is_empty())).then(|| {
        let child = Command::new("python3").args(["-I", &format!("{ROOT}/scripts/wayland-stub.py"), tmp.join("wl").to_str().unwrap()]).spawn().unwrap();
        test_clock::wait_for(|| tmp.join("wl").exists(), std::time::Duration::from_secs(2), "Wayland stub listening");
        Stub(child)
    });
    let work: Arc<Mutex<Vec<(usize, String)>>> = Arc::new(Mutex::new(states.iter().cloned().enumerate().collect()));
    let out: Arc<Mutex<Vec<(usize, String, Result<String, String>)>>> = Arc::default();
    let threads: Vec<_> = (0..std::thread::available_parallelism().map_or(4, |n| n.get().min(8)))
        .map(|_| {
            let (work, out, tmp, device) = (work.clone(), out.clone(), tmp.clone(), device.to_string());
            std::thread::spawn(move || loop {
                let Some((i, state)) = work.lock().unwrap().pop() else { break };
                let data = tmp.join(format!("data-{i}"));
                copy_dir(&tmp.join("data"), &data);
                let (png, words): (PathBuf, PathBuf) = (tmp.join(format!("{i}.png")), tmp.join(format!("{i}.txt")));
                let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
                if tmp.join("wl").exists() {
                    cmd.env("WAYLAND_DISPLAY", tmp.join("wl"));
                }
                let run = cmd
                    .args(["--snapshot", &format!("{state}:device={device}"), png.to_str().unwrap()])
                    .env("POCKETUI_MOCK", "1").env("POCKETUI_MOCK_NOW", FIXTURE_NOW)
                    .env("POCKETUI_LANG", "en")
                    .env("POCKETUI_VISIBLE_TEXT", &words)
                    .env("POCKETUI_DATA", &data)
                    .env("POCKETUI_ES_SYSTEMS", tmp.join("es_systems.cfg"))
                    .output()
                    .expect("run pocket-ui");
                let result = if run.status.success() && png.exists() {
                    Ok(std::fs::read_to_string(&words).unwrap_or_default().lines().map(str::trim).collect::<Vec<_>>().join(" "))
                } else {
                    Err(String::from_utf8_lossy(&run.stderr).lines().last().unwrap_or_default().to_string())
                };
                out.lock().unwrap().push((i, state, result));
            })
        })
        .collect();
    for t in threads {
        t.join().unwrap();
    }
    let mut out = std::mem::take(&mut *out.lock().unwrap());
    out.sort_by_key(|(i, _, _)| *i);
    out.into_iter().map(|(_, s, r)| (s, r)).collect()
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

fn no_page_tells_to_use_wifi(device: &str, all: bool) {
    let mut bad = Vec::new();
    for (state, painted) in paint(device, &states(all)) {
        match painted {
            Ok(text) => bad.extend(tells_to_use_wifi(&text).into_iter().map(|w| format!("{state}: {w}"))),
            // A state the snapshot list names that doesn't paint is the snapshot suite's to report, not this test's.
            Err(why) if all => eprintln!("{state}: not painted ({why})"),
            Err(why) => bad.push(format!("{state}: not painted ({why})")),
        }
    }
    assert!(bad.is_empty(), "{} places on the {device} (no Wi-Fi chip) tell the player to use Wi-Fi:\n{}", bad.len(), bad.join("\n"));
}

#[test]
fn the_word_check_lets_absence_through_and_catches_advice() {
    assert!(tells_to_use_wifi("This device has no Wi-Fi. Copy games from a computer.").is_empty());
    assert!(tells_to_use_wifi("No Wi-Fi here, so the clock doesn't set itself.").is_empty());
    assert_eq!(tells_to_use_wifi("Couldn't reach the store. Check Wi-Fi.").len(), 1);
    assert_eq!(tells_to_use_wifi("Copy them over Wi-Fi from a computer or phone").len(), 1);
}

/// The pages that speak of Wi-Fi, downloads or being online, on an RG28XX.
#[test]
fn a_handheld_without_wifi_is_never_told_to_use_it() {
    no_page_tells_to_use_wifi("rg28xx", false);
}

/// What those pages say instead: the way without Wi-Fi, or that this handheld has none.
#[test]
fn a_handheld_without_wifi_is_told_the_way_without_it() {
    let states: Vec<String> = ["setup:extras", "achievements:offline", "tailscale:offline", "web:off", "starter-home:wifi-off", "sys:time-wrong", "tour:quick"]
        .iter().map(|s| s.to_string()).collect();
    let painted: std::collections::HashMap<String, String> = paint("rg28xx", &states).into_iter().map(|(s, r)| (s, r.unwrap_or_default())).collect();
    for (state, says) in [
        ("setup:extras", "From a computer"),
        ("setup:extras", "roms/gba/covers/Game.png"),
        ("achievements:offline", "RetroAchievements needs the internet"),
        ("tailscale:offline", "this handheld has no Wi-Fi"),
        ("web:off", "this handheld has no Wi-Fi"),
        ("starter-home:wifi-off", "Get the starter pack"),
        ("sys:time-wrong", "Set it under Date and time."),
        ("tour:quick", "Brightness, volume, Bluetooth"),
    ] {
        assert!(painted[state].contains(says), "{state} should say “{says}”: {}", painted[state]);
    }
    // The page says it needs the internet; it no longer promises unlocks sent later "without Wi-Fi" (15.11.).
    assert!(!painted["achievements:offline"].contains("Without Wi-Fi"), "{}", painted["achievements:offline"]);
}

/// Every page of the snapshot list, on an RG28XX and on an R36S.
#[test]
#[ignore = "release check: every snapshot state on the handhelds without Wi-Fi"]
fn every_page_on_a_handheld_without_wifi() {
    no_page_tells_to_use_wifi("rg28xx", true);
    no_page_tells_to_use_wifi("r36s", true);
}
