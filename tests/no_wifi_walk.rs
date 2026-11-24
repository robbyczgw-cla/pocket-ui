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
    // PortMaster games from the card (23.11.2026: the row said "No Wi-Fi" and led nowhere).
    "setup:ports-card-empty-games",
    "setup:ports-card-found",
    "tour:quick",
    "tour:add_games",
    "sys:time",
    "sys:time:2",
    "sys:time-wrong",
    "sys:time-unsynced",
    "curator:card:gba:aria:lookup=offline",
    "cheats-offline",
    "report:saved",
    "report:saved-card",
    "setup:remote",
    "sync:off",
    // RomM (Settings › Games › RomM): a server out of reach says what this handheld lacks, never "Check Wi-Fi".
    "romm:error",
    // UI text pass 1 (21.11.2026) gave both a Wi-Fi line the release walk found, the gate not: the starter pack's
    // download note and the backup's "copy it to a PC" line.
    "starter",
    "backup:saves-counted",
];

/// States for a simulated radio or a feature that needs one. On a board without an adapter, the Wi-Fi page has
/// a dedicated test below. Updates is left to its own test (`update::no_wifi_for`, update-from-card).
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
    paint_with_adapter(device, states, None)
}

fn paint_with_adapter(device: &str, states: &[String], adapter: Option<&str>) -> Vec<(String, Result<String, String>)> {
    let dir = tempfile::tempdir().unwrap();
    let tmp = dir.path().to_path_buf();
    let net = tmp.join("net");
    std::fs::create_dir(&net).unwrap();
    if let Some(iface) = adapter {
        std::fs::create_dir_all(net.join(iface).join("wireless")).unwrap();
    }
    let mut path = std::env::var_os("PATH").unwrap_or_default();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let bin = tmp.join("bin");
        std::fs::create_dir(&bin).unwrap();
        for name in ["nmcli", "ip", "iw", "rfkill", "wifictl", "wpa_cli"] {
            let file = bin.join(name);
            std::fs::write(&file, "#!/bin/sh\nexit 1\n").unwrap();
            std::fs::set_permissions(file, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        path = std::env::join_paths(std::iter::once(bin).chain(std::env::split_paths(&path))).unwrap();
    }
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
            let (work, out, tmp, device, path) = (work.clone(), out.clone(), tmp.clone(), device.to_string(), path.clone());
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
                    .env("PATH", &path)
                    .env("POCKETUI_SNAPSHOT_WIFI", "0")
                    .env("POCKETUI_NET_CLASS", tmp.join("net"))
                    .env_remove("POCKETUI_WIFI_IFACE")
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
    assert!(tells_to_use_wifi("No Wi-Fi here, so set the clock manually.").is_empty());
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
        ("tour:quick", "Brightness, volume, performance"),
    ] {
        assert!(painted[state].contains(says), "{state} should say “{says}”: {}", painted[state]);
    }
    // The page says it needs the internet; it no longer promises unlocks sent later "without Wi-Fi" (15.11.).
    assert!(!painted["achievements:offline"].contains("Without Wi-Fi"), "{}", painted["achievements:offline"]);
    assert!(!painted["achievements:offline"].contains("Offline unlocks"), "{}", painted["achievements:offline"]);
}

/// Every page of the snapshot list, on an RG28XX and on an R36S.
#[test]
#[ignore = "release check: every snapshot state on the handhelds without Wi-Fi"]
fn every_page_on_a_handheld_without_wifi() {
    no_page_tells_to_use_wifi("rg28xx", true);
    no_page_tells_to_use_wifi("r36s", true);
}

#[test]
fn a_board_without_builtin_wifi_can_use_an_adapter_at_startup() {
    let states = ["settings:0", "setup:wifi", "setup:extras", "pockets:what"].map(String::from);
    for (device, iface) in [("rg28xx", "wlan0"), ("rg35xx-2024", "wlan0"), ("r36s", "wlx001122334455"), ("rk2023", "wlan1")] {
        let painted: std::collections::HashMap<_, _> = paint_with_adapter(device, &states, Some(iface))
            .into_iter().map(|(state, result)| (state, result.unwrap())).collect();
        for (state, expected) in [
            ("setup:extras", "Connect to Wi-Fi first"),
            ("pockets:what", "same Wi-Fi"),
            ("settings:0", "Not connected"),
            ("setup:wifi", "Other network"),
        ] {
            assert!(painted[state].contains(expected), "{device} {state} should show {expected:?}: {}", painted[state]);
            assert!(!painted[state].contains("has no Wi-Fi") && !painted[state].contains("None on this device"), "{device} {state}: {}", painted[state]);
        }
    }
}

#[test]
fn a_board_without_an_adapter_explains_what_to_do() {
    for device in ["rg28xx", "r36s"] {
        let painted = paint(device, &["setup:wifi".into()]);
        let text = painted[0].1.as_ref().unwrap();
        assert!(text.contains("Plug in a Wi-Fi adapter and restart"), "{device}: {text}");
        assert!(!text.contains("Other network"), "{device}: {text}");
    }
}

/// R36S hand test 20.11.2026 #13/#15e: the R36S has neither Wi-Fi nor Bluetooth, yet Settings › Connections and the quick
/// settings showed a Bluetooth row (every ROCKNIX card has `rocknix-bluetooth`; no chip was asked). No row names it there
/// now, on both handhelds without a radio chip (`system_ctl::no_bluetooth`).
#[test]
fn a_handheld_without_a_radio_has_no_bluetooth_row() {
    // A USB Wi-Fi stick (`a_board_without_builtin_wifi_can_use_an_adapter_at_startup`) brings Wi-Fi, not Bluetooth.
    for (state, painted) in paint_with_adapter("r36s", &["settings:0".to_string(), "sys:quick".to_string()], Some("wlan0")) {
        let text = painted.unwrap_or_else(|e| panic!("{state} with a Wi-Fi stick: not painted ({e})"));
        assert!(!text.contains("Bluetooth"), "{state} on the r36s with a Wi-Fi stick names Bluetooth: {text}");
    }
    let states: Vec<String> = ["settings:0", "sys:quick", "sys:power", "sys:power-noboot"].iter().map(|s| s.to_string()).collect();
    for device in ["r36s", "rg28xx"] {
        for (state, painted) in paint(device, &states) {
            let text = painted.unwrap_or_else(|e| panic!("{state} on the {device}: not painted ({e})"));
            assert!(!text.contains("Bluetooth"), "{state} on the {device} names Bluetooth: {text}");
            assert!(text.contains("Brightness") || state != "sys:quick", "{state} on the {device}: {text}");
            // #15a: no Android beside ROCKNIX on these boards: no Restart to boot menu, no "To start Android" note.
            assert!(!text.contains("boot menu") && !text.contains("Android"), "{state} on the {device} offers Android: {text}");
            assert!(text.contains("Restart pocket-ui") || !state.starts_with("sys:power"), "{state} on the {device}: {text}");
            // #15b, text that looked doubled in the menu: each line is painted once (a photo of its fade-in, not two draws).
            if state == "sys:power" {
                assert_eq!(text.matches("Restarts the whole Pocket.").count(), 1, "{state} on the {device}: {text}");
            }
        }
    }
}

/// R36S hand test 20.11.2026 #9: the first start said pocket-ui "hasn't been tested on a Game Console R36S yet". It is a
/// beta now, and the welcome says so.
#[test]
fn the_r36s_first_start_calls_it_a_beta() {
    let painted = paint("r36s", &["setup:first".to_string()]);
    let text = painted[0].1.clone().expect("setup:first painted");
    assert!(text.contains("is a beta") && !text.contains("hasn't been tested"), "{text}");
}

/// R36S 640x480 render 21.11.2026: Quick settings showed a "Fan … in menus" row on the R36S, which has no fan. Settings
/// follow `device::no_fan` since 0.6 ("No fan, no fan rows"); the panel took its Fan row from a sysfs look alone, and said
/// "a fan" until that look was done. Every handheld without a fan of ours: no Fan row in the panel, on the shelf and over a game.
#[test]
fn a_handheld_without_a_fan_has_no_fan_row_in_quick_settings() {
    let states: Vec<String> = ["sys:quick", "game-quick"].iter().map(|s| s.to_string()).collect();
    for device in ["r36s", "rg28xx", "rg35xx-plus", "rg35xx-2024"] {
        for (state, painted) in paint(device, &states) {
            let text = painted.unwrap_or_else(|e| panic!("{state} on the {device}: not painted ({e})"));
            assert!(text.contains("Brightness"), "{state} on the {device}: {text}");
            assert!(!text.contains("Fan") && !text.contains("in menus"), "{state} on the {device} has a Fan row: {text}");
        }
    }
}
