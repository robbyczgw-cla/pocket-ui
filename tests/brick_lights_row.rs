//! Settings > Sound & feel: the row that opens the lights page carries the page's name (device report, 06.11.: the row said
//! "LEDs" and opened "Lights & buttons"). Read from what the screen paints (`POCKETUI_VISIBLE_TEXT`), at the Brick's 1024x768 in
//! Large text, in English, German and Spanish:
//! - the Brick's row is called as its page is, its value is the short "Lights off" (the lights are dark at the first start),
//!   and B on the page comes back to the list with that row selected;
//! - the handhelds keep "LEDs" (the lights around the sticks), and their page is not the Brick's.
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

/// The last row of Sound & feel (category 1): the lights row, on the Brick and on a handheld with sticks.
const LIST: &str = "settings:1:99";

#[cfg(feature = "brick")]
#[test]
fn the_bricks_lights_row_is_named_like_its_page_and_says_lights_off() {
    let tmp = tempfile::tempdir().unwrap();
    let stub = wayland(tmp.path());
    // (language, the page's title, the short value) as the player reads them.
    for (lang, name, value) in [("en", "Lights & buttons", "Lights off"), ("de", "Lichter & Tasten", "Lichter aus"), ("es", "Luces y botones", "Luces apagadas")] {
        let list = painted(tmp.path(), &stub, "brick", lang, LIST);
        let page = painted(tmp.path(), &stub, "brick", lang, "leds:off:0");
        assert!(page.iter().any(|t| t == name), "{lang}: the page is titled {name:?}: {page:?}");
        assert!(list.iter().any(|t| t == name), "{lang}: the row is called {name:?} like its page: {list:?}");
        assert!(list.iter().any(|t| t.trim_end_matches(['>', '›', ' ']) == value), "{lang}: the row's value is {value:?}: {list:?}");
        assert!(!list.iter().any(|t| t.contains("LED")), "{lang}: no sticks' LEDs on the Brick: {list:?}");
        // B on the page: the list again, the lights row there (it is painted with the same name; the page's own rows are gone).
        let back = painted(tmp.path(), &stub, "brick", lang, "leds:off:0:b");
        assert!(back.iter().any(|t| t == name) && back.iter().any(|t| t.trim_end_matches(['>', '›', ' ']) == value), "{lang}: B comes back to the list: {back:?}");
        assert!(!page.iter().any(|t| t == value), "{lang}: the page itself shows no list value: {page:?}");
    }
}

#[cfg(not(feature = "brick"))]
#[test]
fn the_handhelds_keep_their_leds_row() {
    let tmp = tempfile::tempdir().unwrap();
    let stub = wayland(tmp.path());
    for lang in ["en", "de", "es"] {
        let list = painted(tmp.path(), &stub, "miniv2", lang, LIST);
        assert!(list.iter().any(|t| t == "LEDs"), "{lang}: the sticks' row is called LEDs: {list:?}");
        assert!(!list.iter().any(|t| ["Lights & buttons", "Lichter & Tasten", "Luces y botones", "Lights off"].contains(&t.as_str())), "{lang}: the Brick's name is not on a handheld: {list:?}");
    }
}
