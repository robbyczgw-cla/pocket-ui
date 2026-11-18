//! The first page of EVERY Settings category runs no string through the language table's patterns, in every language: the
//! deterministic half of the guard on Settings' first frame (tests/settings_open.rs keeps the timing, which is a time
//! budget and runs alone, nightly and before a release). This one is fast and exact, so it runs in every merge gate.
//!
//! 0.5.0 guarded only the page START opens (Connections). The other categories' values ("Shelf >", "Nintendo",
//! "Auto · Quiet", "‹ Home ›", "Deutsch", "New York") ran the patterns on their first sight and were found by the
//! export's full run. Each category's first page is painted from the shelf's frame, with the focus in the list and on
//! the tab bar (`pocket-ui --snapshot bench-settings-pages`, `App::settings_pages_patterns`).
//!
//! Needs python3 on Linux without a compositor (scripts/wayland-stub.py, as tests/settings_open.rs).

use std::path::{Path, PathBuf};
use std::process::Command;

include!("common/root.rs");

/// scripts/wayland-stub.py, while it runs.
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

/// A snapshot opens no window, but winit's event loop wants a Wayland compositor to say hello first.
fn wayland_for_snapshots(tmp: &Path) -> Option<Stub> {
    if !cfg!(target_os = "linux") || std::env::var_os("WAYLAND_DISPLAY").is_some_and(|w| !w.is_empty()) {
        return None;
    }
    let path = tmp.join("wayland-stub");
    let child = Command::new("python3")
        .args(["-I", &format!("{ROOT}/scripts/wayland-stub.py"), path.to_str().unwrap()])
        .spawn()
        .expect("python3 for scripts/wayland-stub.py");
    let stub = Stub { path, child };
    test_clock::wait_for(|| stub.path.exists(), std::time::Duration::from_secs(2), "Wayland stub listening");
    Some(stub)
}

#[test]
fn every_settings_category_first_page_runs_no_pattern() {
    let tmp = tempfile::tempdir().unwrap();
    let wayland = wayland_for_snapshots(tmp.path());
    for lang in ["en", "de", "es", "pt", "fr", "it"] {
        let data = tmp.path().join(format!("pages-{lang}"));
        std::fs::create_dir_all(&data).unwrap();
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
        cmd.args(["--snapshot", "bench-settings-pages", tmp.path().join("pages.png").to_str().unwrap()])
            .env("POCKETUI_MOCK", "1")
            .env("POCKETUI_LANG", lang)
            // The corner as on a device with Wi-Fi: the symbol is part of the frame.
            .env("POCKETUI_SNAPSHOT_WIFI", "1")
            .env("POCKETUI_ES_SYSTEMS", tmp.path().join("none.cfg"))
            .env("POCKETUI_DATA", &data);
        if let Some(w) = &wayland {
            cmd.env("WAYLAND_DISPLAY", &w.path);
        }
        let out = cmd.output().expect("run pocket-ui");
        assert!(out.status.success(), "{lang}: {}", String::from_utf8_lossy(&out.stderr));
        let stdout = String::from_utf8_lossy(&out.stdout);
        let line = stdout.lines().find(|l| l.starts_with("bench-settings-pages ")).unwrap_or_else(|| panic!("{lang}: no line in:\n{stdout}"));
        let field = |name: &str| -> usize {
            let key = format!("{name}=");
            let at = line.find(&key).unwrap_or_else(|| panic!("no {name} in {line}")) + key.len();
            line[at..].split_whitespace().next().unwrap().parse().unwrap_or_else(|_| panic!("{name} in {line}"))
        };
        // Eight categories with two pages each: the test must not pass by painting nothing.
        assert_eq!(field("pages"), 16, "{lang}: every category's two first pages were painted ({line})");
        assert_eq!(field("patterns"), 0, "{lang}: a first page ran the table's patterns ({line})");
    }
    drop(wayland);
}
