//! Small screens must not crash (docs/DEVICES.md "Layouts"): every state of `tests/i18n-states.txt` is rendered at
//! 640x480, the screen of the RG28XX and the RG35XX SP, and the test fails when one of them does not come out.
//!
//! Small panels now use a supported logical canvas and area reduction. This retains the regression coverage for
//! the 62 states that aborted before and verifies the PNG is still the physical panel size.
//!
//! It uses the same machinery as `scripts/size-sweep.sh`: the made-up library of `scripts/curator-fake-library.py`,
//! `POCKETUI_SNAPSHOT_SIZE=640x480` and one snapshot per state. Needs python3 (also for `scripts/wayland-stub.py` on
//! Linux without a compositor). About 620 snapshots, eight at a time.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};

const ROOT: &str = env!("CARGO_MANIFEST_DIR");
const SIZE: &str = "640x480";

fn states() -> Vec<(bool, String)> {
    std::fs::read_to_string(Path::new(ROOT).join("tests/i18n-states.txt"))
        .expect("tests/i18n-states.txt")
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| match l.strip_prefix("safe ") {
            Some(s) => (true, s.to_string()),
            None => (false, l.to_string()),
        })
        .collect()
}

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
    for _ in 0..200 {
        if stub.path.exists() {
            return Some(stub);
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    panic!("scripts/wayland-stub.py didn't start");
}

fn fake_library(dir: &Path) {
    let status = Command::new("python3")
        .args(["-I", &format!("{ROOT}/scripts/curator-fake-library.py"), dir.to_str().unwrap(), "dark"])
        .stdout(std::process::Stdio::null())
        .status()
        .expect("python3 for scripts/curator-fake-library.py");
    assert!(status.success(), "curator-fake-library.py failed");
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

#[test]
fn every_state_renders_at_640x480_without_panicking() {
    let _tmp_guard = tempfile::tempdir().unwrap();
    let tmp = _tmp_guard.path().to_path_buf();
    fake_library(&tmp);
    let bin = PathBuf::from(env!("CARGO_BIN_EXE_pocket-ui"));
    let wayland = wayland_for_snapshots(&tmp);
    let work: Arc<Mutex<Vec<(usize, bool, String)>>> = Arc::new(Mutex::new(states().into_iter().enumerate().map(|(i, (safe, s))| (i, safe, s)).collect()));
    let failed = Arc::new(Mutex::new(Vec::new()));
    let threads: Vec<_> = (0..std::thread::available_parallelism().map_or(4, |n| n.get().min(8)))
        .map(|_| {
            let (work, failed, tmp, bin, wayland) = (work.clone(), failed.clone(), tmp.clone(), bin.clone(), wayland.as_ref().map(|w| w.path.clone()));
            std::thread::spawn(move || loop {
                let Some((i, safe, state)) = work.lock().unwrap().pop() else { break };
                // Its own copy of the data folder: some states write into it (save slots), and they run side by side.
                let data = tmp.join(format!("data-{i}"));
                copy_dir(&tmp.join("data"), &data);
                let png = tmp.join(format!("{i}.png"));
                let mut cmd = Command::new(&bin);
                if safe {
                    cmd.arg("--safe");
                }
                if let Some(path) = &wayland {
                    cmd.env("WAYLAND_DISPLAY", path);
                }
                let out = cmd
                    .args(["--snapshot", &state, png.to_str().unwrap()])
                    .env("POCKETUI_MOCK", "1")
                    .env("POCKETUI_LANG", "en")
                    .env("POCKETUI_TEXT_SIZE", "normal")
                    .env("POCKETUI_SNAPSHOT_SIZE", SIZE)
                    .env("POCKETUI_ES_SYSTEMS", tmp.join("es_systems.cfg"))
                    .env("POCKETUI_DATA", &data)
                    .output()
                    .expect("run pocket-ui");
                let dimensions = std::fs::File::open(&png).ok().and_then(|f| png::Decoder::new(f).read_info().ok())
                    .map(|r| (r.info().width, r.info().height));
                if !out.status.success() || dimensions != Some((640, 480)) {
                    failed.lock().unwrap().push(format!("{state}: {}", String::from_utf8_lossy(&out.stderr)));
                }
            })
        })
        .collect();
    for t in threads {
        t.join().unwrap();
    }
    drop(wayland);
    let failed = failed.lock().unwrap();
    assert!(failed.is_empty(), "{} snapshot(s) at {SIZE} did not render:\n{}", failed.len(), failed.join("\n"));
}
