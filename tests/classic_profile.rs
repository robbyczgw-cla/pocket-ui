//! The Classic's first-start descriptions stay whole and its fan choices belong to ROCKNIX.
use std::{path::PathBuf, process::{Command, Child}};
include!("common/root.rs");
struct Stub(Child);
impl Drop for Stub { fn drop(&mut self) { let _ = self.0.kill(); let _ = self.0.wait(); } }
#[test]
fn classic_welcome_is_readable_and_fan_rows_are_read_only() {
    let tmp = tempfile::tempdir().unwrap();
    let socket = tmp.path().join("wayland");
    let _stub = Stub(Command::new("python3").args(["-I", &format!("{ROOT}/scripts/wayland-stub.py"), socket.to_str().unwrap()]).spawn().unwrap());
    test_clock::wait_for(|| socket.exists(), std::time::Duration::from_secs(2), "Wayland stub");
    let bin = std::env::var_os("POCKETUI_NAV_BINARY").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(env!("CARGO_BIN_EXE_pocket-ui")));
    for lang in ["en", "de", "es"] {
        for size in ["normal", "medium", "large"] {
            for state in ["setup:first", "perf"] {
                let misses = tmp.path().join("misses");
                let visible = tmp.path().join("visible");
                let out = Command::new(&bin).args(["--snapshot", state, tmp.path().join("screen.png").to_str().unwrap()])
                    .env("WAYLAND_DISPLAY", &socket).env("POCKETUI_MOCK", "1").env("POCKETUI_DEVICE", "rp-classic")
                    .env("POCKETUI_SNAPSHOT_SIZE", "1240x1080").env("POCKETUI_LANG", lang).env("POCKETUI_TEXT_SIZE", size)
                    .env("POCKETUI_DATA", tmp.path().join(format!("data-{lang}-{size}-{state}")))
                    .env("POCKETUI_I18N_MISSES", &misses).env("POCKETUI_VISIBLE_TEXT", &visible).output().unwrap();
                assert!(out.status.success(), "{lang}/{size}/{state}: {}", String::from_utf8_lossy(&out.stderr));
                let cuts = std::fs::read_to_string(&misses).unwrap_or_default();
                assert!(!cuts.lines().any(|l| l.starts_with("C\t") || l.starts_with("X\t")), "{lang}/{size}/{state}: {cuts}");
                if state == "perf" {
                    let text = std::fs::read_to_string(&visible).unwrap();
                    assert_eq!(text.lines().filter(|l| *l == "ROCKNIX").count(), 3, "{lang}/{size}: fan rows must say ROCKNIX: {text}");
                }
            }
        }
    }
}
