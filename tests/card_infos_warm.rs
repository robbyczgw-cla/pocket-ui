//! The first card's game facts looked up ahead (`App::infos_warm_tick`, src/app_card.rs), checked through the binary
//! (`pocket-ui --snapshot infos-warm-check:gb`): the first card of a process asks the database for every game of the
//! library ("More by", "Other games in": `related_games`); on the RG35XX Plus that was 205 of the first box's 240 ms with
//! 1200 games (PERF-FEEL.md, 19.11.2026). After the idle look-ahead the card asks for none.
//!
//! Needs python3 on Linux without a compositor (scripts/wayland-stub.py, as tests/cover_prefetch.rs).

use std::path::PathBuf;
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

fn wayland_for_snapshots(tmp: &std::path::Path) -> Option<Stub> {
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
fn after_the_idle_look_ahead_the_first_card_looks_up_no_other_game() {
    let tmp = tempfile::tempdir().unwrap();
    let roms = tmp.path().join("roms/gb");
    std::fs::create_dir_all(&roms).unwrap();
    // A game the database knows (its card has "More by"), and many it doesn't (each one a close-match search).
    std::fs::write(roms.join("Super Mario Land 2 - 6 Golden Coins (USA, Europe).gb"), b"").unwrap();
    for i in 0..60 {
        std::fs::write(roms.join(format!("zz Homebrew {i:03} (World).gb")), b"").unwrap();
    }
    let data = tmp.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    std::fs::write(data.join("settings.json"), r#"{"theme":"shelf","onboarded":true}"#).unwrap();
    std::fs::write(
        tmp.path().join("es_systems.cfg"),
        format!("<systemList><system><name>gb</name><fullname>Game Boy</fullname><path>{}</path><extension>.gb</extension><command>true</command></system></systemList>", roms.display()),
    )
    .unwrap();
    let wayland = wayland_for_snapshots(tmp.path());
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
    cmd.args(["--snapshot", "infos-warm-check:gb", tmp.path().join("check.png").to_str().unwrap()])
        .env("POCKETUI_MOCK", "1")
        .env("POCKETUI_SNAPSHOT_SIZE", "640x480")
        .env("POCKETUI_LANG", "en")
        .env("POCKETUI_ES_SYSTEMS", tmp.path().join("es_systems.cfg"))
        .env("POCKETUI_DATA", &data);
    if let Some(w) = &wayland {
        cmd.env("WAYLAND_DISPLAY", &w.path);
    }
    let out = cmd.output().expect("run pocket-ui");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8_lossy(&out.stdout);
    let line = stdout.lines().find(|l| l.starts_with("infos-warm ")).unwrap_or_else(|| panic!("no infos-warm line in:\n{stdout}"));
    println!("{line}");
    let field = |k: &str| -> usize { line.split_whitespace().find_map(|w| w.strip_prefix(k)).unwrap_or_else(|| panic!("{k} in {line}")).parse().unwrap() };
    let (games, cold, warmed, renamed) = (field("games="), field("cold="), field("warmed="), field("renamed="));
    assert_eq!(games, 61, "{line}");
    // Red before the look-ahead: the first card asked for the whole library.
    assert!(cold >= games, "the check sees no cold lookups, it can't tell warm from cold: {line}");
    assert_eq!(warmed, 0, "after the look-ahead the first card still looked games up: {line}");
    // 19.11. on the RG35XX Plus: the start's checksum pass named a game after the look-ahead had finished, the facts
    // were cleared and the first card looked the whole library up again (180-200 ms).
    assert_eq!(renamed, 0, "after new names the look-ahead didn't start again: {line}");
}
