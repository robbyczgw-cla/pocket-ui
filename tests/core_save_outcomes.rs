//! Player-visible save failures and blocked Load in the existing notice/menu style.
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
fn painted(tmp: &Path, stub: &Option<Stub>, lang: &str, state: &str, size: &str) -> Vec<String> {
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
        .env("POCKETUI_DEVICE", "miniv2")
        .env("POCKETUI_LANG", lang)
        .env("POCKETUI_TEXT_SIZE", "large")
        .env("POCKETUI_SNAPSHOT_SIZE", size)
        .env("POCKETUI_VISIBLE_TEXT", &seen)
        .env("POCKETUI_ES_SYSTEMS", tmp.join("none.cfg"))
        .env("POCKETUI_DATA", &data)
        .output()
        .expect("run pocket-ui");
    assert!(out.status.success(), "{state}: {}", String::from_utf8_lossy(&out.stderr));
    std::fs::read_to_string(&seen).unwrap_or_default().lines().map(|l| l.trim().to_string()).collect()
}


#[test]
fn failed_save_is_visible_and_a_blocked_load_explains_and_returns_without_loading() {
    let tmp = tempfile::tempdir().unwrap();
    let stub = wayland(tmp.path());
    for size in ["640x480", "1024x768"] {
        for (lang, load_head, load_tail, save_head, save_tail, play) in [
            ("en", "Couldn't load:", "unchanged.", "Couldn't keep this moment.", "kept.", "Continue"),
            ("de", "Laden nicht möglich:", "unverändert.", "Dieser Spielmoment", "erhalten.", "Fortsetzen"),
            ("es", "No se pudo cargar:", "sigue igual.", "No se pudo guardar", "conservan.", "Continuar"),
        ] {
            let visible = painted(tmp.path(), &stub, lang, "load-blocked", size).join(" ");
            assert!(visible.contains(load_head) && visible.contains(load_tail), "{lang}/{size}: load failure must be readable: {visible}");
            let saved = painted(tmp.path(), &stub, lang, "notice:save-failed", size).join(" ");
            assert!(saved.contains(save_head) && saved.contains(save_tail), "{lang}/{size}: the previous saves' safety must be visible: {saved}");
            for key in ["a", "b"] {
                let back = painted(tmp.path(), &stub, lang, &format!("load-blocked:{key}"), size).join(" ");
                assert!(!back.contains(load_head) && back.contains(play), "{lang}/{size}/{key}: back to Continue, with no load: {back}");
            }
        }
    }
}
