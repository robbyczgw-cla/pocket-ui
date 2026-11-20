//! Handhelds without a fan of ours (review 19.11. #2): the H700, Rockchip and R36S handhelds and the RG DS Plus had
//! "Performance & fan" with three dead "No fan" rows, and the pause menu's "Speed, rumble, fan" with no fan inside, its
//! line "Settings you rarely need.". Read from what the screen paints (`POCKETUI_VISIBLE_TEXT`) in English and German:
//! without a fan no row, page, group or note names one, and the group's line says what is in it; the Mini V2 keeps its fan.
//!
//! Needs python3 for scripts/wayland-stub.py on Linux without a compositor (as tests/settings_open.rs).

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
    let child = Command::new("python3").args(["-I", &format!("{ROOT}/scripts/wayland-stub.py"), path.to_str().unwrap()]).spawn().expect("python3 for scripts/wayland-stub.py");
    let stub = Stub { path, child };
    test_clock::wait_for(|| stub.path.exists(), std::time::Duration::from_secs(2), "Wayland stub listening");
    Some(stub)
}

/// The strings painted on `state` (one per element), for this device, language and Large text at 1024x768.
fn painted(tmp: &Path, stub: &Option<Stub>, device: &str, lang: &str, state: &str) -> Vec<String> {
    let data = tmp.join("data");
    std::fs::create_dir_all(&data).unwrap();
    let seen = tmp.join("seen.txt");
    let _ = std::fs::remove_file(&seen);
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
    if let Some(s) = stub {
        cmd.env("WAYLAND_DISPLAY", &s.path);
    }
    let out = cmd
        .args(["--snapshot", state, tmp.join("shot.png").to_str().unwrap()])
        .env("POCKETUI_MOCK", "1")
        .env("POCKETUI_DEVICE", device)
        .env("POCKETUI_LANG", lang)
        .env("POCKETUI_TEXT_SIZE", "large")
        .env("POCKETUI_SNAPSHOT_SIZE", "1024x768")
        .env("POCKETUI_VISIBLE_TEXT", &seen)
        .env("POCKETUI_ES_SYSTEMS", tmp.join("none.cfg"))
        .env("POCKETUI_DATA", &data)
        .output()
        .expect("run pocket-ui");
    assert!(out.status.success(), "{state}: {}", String::from_utf8_lossy(&out.stderr));
    std::fs::read_to_string(&seen).unwrap_or_default().lines().map(|l| l.trim().to_string()).collect()
}

const FANLESS: [&str; 4] = ["rg35xx-plus", "rg353v", "r36s", "rgdsplus"];
/// The fan's word in a language's table, as a word ("fan", not "fans"/"fantasy").
fn names_a_fan(lang: &str, t: &str) -> bool {
    let word = if lang == "de" { "Lüfter" } else { "fan" };
    t.split(|c: char| !c.is_alphanumeric()).any(|w| w.eq_ignore_ascii_case(word) || (lang == "de" && w.ends_with("Lüfter")))
}

#[cfg(not(feature = "brick"))]
#[test]
fn no_fan_no_fan_rows_and_the_group_says_what_is_in_it() {
    let tmp = tempfile::tempdir().unwrap();
    let stub = wayland(tmp.path());
    for (lang, page, group, line) in [
        ("en", "Performance", "Speed & rumble", "Fast-forward, rewind, earlier states, the frame rate, rumble and performance."),
        ("de", "Leistung", "Tempo & Vibration", "Vorspulen, Zurückspulen, frühere Spielstände, Bildrate, Vibration und Leistung."),
    ] {
        for device in FANLESS {
            let perf = painted(tmp.path(), &stub, device, lang, "perf");
            assert!(perf.iter().any(|t| t == page), "{device} {lang}: the page is {page:?}: {perf:?}");
            assert!(!perf.iter().any(|t| names_a_fan(lang, t) || t.contains("No fan") || t.contains("Kein Lüfter")), "{device} {lang}: a fan on the page: {perf:?}");
            let list = painted(tmp.path(), &stub, device, lang, "settings:4:7");
            assert!(list.iter().any(|t| t == page), "{device} {lang}: Playing names the row {page:?}: {list:?}");
            assert!(!list.iter().any(|t| names_a_fan(lang, t)), "{device} {lang}: {list:?}");
            let extras = painted(tmp.path(), &stub, device, lang, "extras:0");
            assert!(extras.iter().any(|t| t.ends_with(group)), "{device} {lang}: the group is {group:?}: {extras:?}");
            assert!(!extras.iter().any(|t| names_a_fan(lang, t)), "{device} {lang}: {extras:?}");
            let options: Vec<String> = (0..9).flat_map(|sel| painted(tmp.path(), &stub, device, lang, &format!("options:{sel}"))).collect();
            assert!(options.iter().any(|t| t == group), "{device} {lang}: Options lists {group:?}: {options:?}");
            assert!(options.join(" ").contains(line), "{device} {lang}: its line says what is in it: {options:?}");
            assert!(!options.iter().any(|t| names_a_fan(lang, t)), "{device} {lang}: {options:?}");
        }
    }
    // A handheld with a fan keeps the fan's page and group, its line naming the fan.
    let perf = painted(tmp.path(), &stub, "miniv2", "en", "perf");
    assert!(perf.iter().any(|t| t == "Performance & fan") && perf.iter().any(|t| t == "Smooth fan"), "{perf:?}");
    let options: Vec<String> = (0..9).flat_map(|sel| painted(tmp.path(), &stub, "miniv2", "en", &format!("options:{sel}"))).collect();
    assert!(options.iter().any(|t| t == "Speed, rumble, fan"), "{options:?}");
    assert!(options.join(" ").contains("rumble, performance and the fan."), "{options:?}");
}
