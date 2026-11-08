//! The player's daily recommendation survives a restart and follows the injected calendar.
use std::{process::{Child, Command}, time::Duration};

include!("common/root.rs");

struct Stub(Child);
impl Drop for Stub {
    fn drop(&mut self) { let _ = self.0.kill(); let _ = self.0.wait(); }
}

#[test]
fn the_daily_pick_survives_restarts_and_changes_with_the_fixture_day() {
    let tmp = tempfile::Builder::new().prefix("pui-day-").tempdir().unwrap();
    assert!(Command::new("python3").env("POCKETUI_MOCK_NOW", FIXTURE_NOW)
        .args(["-I", &format!("{ROOT}/scripts/curator-fake-library.py"), tmp.path().to_str().unwrap(), "dark"])
        .stdout(std::process::Stdio::null()).status().unwrap().success());
    let socket = tmp.path().join("wl");
    let _stub = if cfg!(target_os = "linux") && std::env::var_os("WAYLAND_DISPLAY").is_none_or(|s| s.is_empty()) {
        let child = Command::new("python3").args(["-I", &format!("{ROOT}/scripts/wayland-stub.py")]).arg(&socket).spawn().unwrap();
        let stub = Stub(child);
        test_clock::wait_for(|| socket.exists(), Duration::from_secs(2), "Wayland ready");
        Some(stub)
    } else { None };
    // An archived UI can replay the same journey to prove the clock wiring was missing.
    let binary = std::env::var_os("POCKETUI_CURATOR_BINARY").unwrap_or_else(|| env!("CARGO_BIN_EXE_pocket-ui").into());
    let pick = |now: u64| {
        let words = tmp.path().join("words.txt");
        let _ = std::fs::remove_file(&words);
        let mut cmd = Command::new(&binary);
        if _stub.is_some() { cmd.env("WAYLAND_DISPLAY", &socket); }
        let out = cmd.args(["--snapshot", "curator:home"]).arg(tmp.path().join("home.png"))
            .env("POCKETUI_MOCK", "1").env("POCKETUI_MOCK_NOW", now.to_string())
            .env("POCKETUI_LANG", "en").env("POCKETUI_FONT", "barlow")
            .env("POCKETUI_TEXT_SIZE", "normal").env("POCKETUI_SNAPSHOT_SIZE", "1280x720")
            .env("POCKETUI_DATA", tmp.path().join("data"))
            .env("POCKETUI_ES_SYSTEMS", tmp.path().join("es_systems.cfg"))
            .env("POCKETUI_VISIBLE_TEXT", &words).output().unwrap();
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        let words = std::fs::read_to_string(words).unwrap();
        let line = words.lines().find(|line| line.starts_with("Today: ")).expect(&words);
        // The game must change; an aging reason beside the same game is not a new pick.
        line.split(" · ").next().unwrap().to_string()
    };
    let now: u64 = FIXTURE_NOW.parse().unwrap();
    let first = pick(now);
    assert_eq!(first, pick(now), "restarting on the same day keeps the recommendation");
    assert!((1..=6).any(|day| pick(now + day * 86400) != first),
        "the rendered recommendation must follow the injected days, not the build host's date: {first}");
}
