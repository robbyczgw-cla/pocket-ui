//! The game card's rows survive a refresh (VDEV-ROUND-2026-10-25): a new Curator index (after a rating or a sticker)
//! and the sync conflicts' scan make an open card's rows again, and once did so from the Curator rows alone, so
//! Screenshots and "Hide or delete" went off the card until it was opened again. Rendered with
//! `pocket-ui --snapshot curator:card:…` on scripts/curator-fake-library.py's made-up library: the card scrolled to its
//! end (Down onto its last row, "Hide or delete") looks the same with and without a refresh (`f`, `c`) before.
//!
//! Needs python3 for the made-up library and, on Linux without a compositor, scripts/wayland-stub.py (as
//! tests/i18n_snapshots.rs).

use std::path::{Path, PathBuf};
use std::process::Command;

const ROOT: &str = env!("CARGO_MANIFEST_DIR");

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
    for _ in 0..200 {
        if stub.path.exists() {
            return Some(stub);
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    panic!("scripts/wayland-stub.py didn't start");
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

/// The card's scrolling body and hint bar (under the label, so the clock can't make a difference), as 0RGB pixels.
fn body(path: &Path) -> Vec<u32> {
    let mut reader = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(path).expect("snapshot written"))).read_info().unwrap();
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).unwrap();
    let ch = info.color_type.samples();
    let w = info.width as usize;
    buf[..info.buffer_size()].chunks_exact(ch).enumerate().filter(|(i, _)| i / w >= 430).map(|(_, p)| (p[0] as u32) << 16 | (p[1] as u32) << 8 | p[2] as u32).collect()
}

fn render(bin: &Path, tmp: &Path, state: &str, stub: Option<&Stub>) -> Vec<u32> {
    let data = tmp.join(format!("data-{}", state.replace(':', "-")));
    copy_dir(&tmp.join("lib").join("data"), &data);
    let png = tmp.join(format!("{}.png", state.replace(':', "-")));
    let mut cmd = Command::new(bin);
    if let Some(s) = stub {
        cmd.env("WAYLAND_DISPLAY", &s.path);
    }
    let out = cmd
        .args(["--snapshot", state, png.to_str().unwrap()])
        .env("POCKETUI_MOCK", "1")
        .env("POCKETUI_ES_SYSTEMS", tmp.join("lib").join("es_systems.cfg"))
        .env("POCKETUI_DATA", &data)
        .env_remove("POCKETUI_SCREENSHOTS")
        .output()
        .expect("run pocket-ui");
    assert!(out.status.success(), "{state}: {}", String::from_utf8_lossy(&out.stderr));
    body(&png)
}

#[test]
fn a_refresh_keeps_every_row_of_the_card() {
    let _tmp_guard = tempfile::tempdir().unwrap();
    let tmp = _tmp_guard.path().to_path_buf();
    let status = Command::new("python3")
        .args(["-I", &format!("{ROOT}/scripts/curator-fake-library.py"), tmp.join("lib").to_str().unwrap(), "dark"])
        .stdout(std::process::Stdio::null())
        .status()
        .expect("python3 for scripts/curator-fake-library.py");
    assert!(status.success(), "curator-fake-library.py failed");
    let bin = PathBuf::from(env!("CARGO_BIN_EXE_pocket-ui"));
    let stub = wayland(&tmp);
    let downs = "d".repeat(60);
    let top = render(&bin, &tmp, "curator:card:snes:chrono", stub.as_ref());
    let end = render(&bin, &tmp, &format!("curator:card:snes:chrono:{downs}"), stub.as_ref());
    assert_ne!(top, end, "Down scrolls the card to its end");
    // `f`: a new Curator index (after a rating, a sticker), `c`: the sync conflicts' scan.
    for refresh in ["f", "c", "fc"] {
        let after = render(&bin, &tmp, &format!("curator:card:snes:chrono:{refresh}{downs}"), stub.as_ref());
        assert!(after == end, "{refresh}: the card's end after a refresh is the same, Hide or delete included");
    }
}
