//! The screenshot gallery (`src/gallery.rs`, `src/app_gallery.rs`) as the player sees it, rendered with
//! `pocket-ui --snapshot gallery:…` on scripts/curator-fake-library.py's made-up library, in the dark and the light
//! appearance: a page whose data is missing says something in words, never a blank where the list or the picture goes.
//! A made-up folder of screenshots is written by the snapshot itself (`App::gallery_snapshot`).
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

/// A screen as (width, height, 0RGB pixels).
fn read_png(path: &Path) -> (usize, usize, Vec<u32>) {
    let mut reader = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(path).expect("snapshot written"))).read_info().unwrap();
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).unwrap();
    let ch = info.color_type.samples();
    let px = buf[..info.buffer_size()].chunks_exact(ch).map(|p| (p[0] as u32) << 16 | (p[1] as u32) << 8 | p[2] as u32).collect();
    (info.width as usize, info.height as usize, px)
}

/// How many pixels of the box (x, y, w, h) differ from the page colour at the screen's top left.
fn ink(shot: &(usize, usize, Vec<u32>), (x, y, w, h): (usize, usize, usize, usize)) -> usize {
    let bg = shot.2[0];
    (y..(y + h).min(shot.1)).flat_map(|row| (x..(x + w).min(shot.0)).map(move |col| (row, col))).filter(|&(row, col)| shot.2[row * shot.0 + col] != bg).count()
}

fn render(bin: &Path, tmp: &Path, library: &str, state: &str, stub: Option<&Stub>) -> (usize, usize, Vec<u32>) {
    render_noting(bin, tmp, library, state, stub, None)
}

/// [`render`], with the text painter's notes (`POCKETUI_I18N_MISSES`: `C` lines are strings cut short with "…") in
/// `notes`.
fn render_noting(bin: &Path, tmp: &Path, library: &str, state: &str, stub: Option<&Stub>, notes: Option<&Path>) -> (usize, usize, Vec<u32>) {
    let data = tmp.join(format!("data-{library}-{}", state.replace(':', "-")));
    copy_dir(&tmp.join(library).join("data"), &data);
    let png = tmp.join(format!("{library}-{}.png", state.replace(':', "-")));
    let mut cmd = Command::new(bin);
    if let Some(s) = stub {
        cmd.env("WAYLAND_DISPLAY", &s.path);
    }
    if let Some(n) = notes {
        cmd.env("POCKETUI_I18N_MISSES", n);
    }
    let out = cmd
        .args(["--snapshot", state, png.to_str().unwrap()])
        .env("POCKETUI_MOCK", "1")
        .env("POCKETUI_ES_SYSTEMS", tmp.join(library).join("es_systems.cfg"))
        .env("POCKETUI_DATA", &data)
        .env_remove("POCKETUI_SCREENSHOTS")
        .output()
        .expect("run pocket-ui");
    assert!(out.status.success(), "{state}: {}", String::from_utf8_lossy(&out.stderr));
    read_png(&png)
}

#[test]
fn the_gallery_says_something_in_every_state_in_both_appearances() {
    let _tmp_guard = tempfile::tempdir().unwrap();
    let tmp = _tmp_guard.path().to_path_buf();
    for appearance in ["dark", "light"] {
        let status = Command::new("python3")
            .args(["-I", &format!("{ROOT}/scripts/curator-fake-library.py"), tmp.join(appearance).to_str().unwrap(), appearance])
            .stdout(std::process::Stdio::null())
            .status()
            .expect("python3 for scripts/curator-fake-library.py");
        assert!(status.success(), "curator-fake-library.py failed");
    }
    std::fs::create_dir_all(&tmp).unwrap();
    let bin = PathBuf::from(env!("CARGO_BIN_EXE_pocket-ui"));
    let stub = wayland(&tmp);
    for library in ["dark", "light"] {
        // No screenshots at all: the page names what is missing and how to take one (the Title line and a sentence).
        let empty = render(&bin, &tmp, library, "gallery:empty", stub.as_ref());
        assert!(ink(&empty, (60, 150, 1100, 80)) > 800, "{library}: the heading of an empty page is there");
        assert!(ink(&empty, (60, 230, 1100, 90)) > 800, "{library}: and the way to take one");
        // The folder is not read yet: a word, not an empty list.
        let loading = render(&bin, &tmp, library, "gallery:loading", stub.as_ref());
        assert!(ink(&loading, (60, 150, 1100, 80)) > 400, "{library}: 'Looking for screenshots…'");
        // A list: the rows are there (pictures and names).
        let list = render(&bin, &tmp, library, "gallery", stub.as_ref());
        assert!(ink(&list, (60, 150, 1120, 700)) > 150_000, "{library}: the list");
        // A damaged file full screen: the words, not a black screen.
        let broken = render(&bin, &tmp, library, "gallery:broken-full", stub.as_ref());
        assert!(ink(&broken, (60, 300, 1120, 400)) > 1_500, "{library}: 'This screenshot can't be opened'");
        // A good one: the picture fills most of the middle.
        let full = render(&bin, &tmp, library, "gallery:full", stub.as_ref());
        assert!(ink(&full, (60, 200, 1120, 700)) > 200_000, "{library}: the picture");
        // A damaged file in the list: the row is there and its small picture says '?', the others show theirs.
        let some_broken = render(&bin, &tmp, library, "gallery:broken", stub.as_ref());
        assert!(ink(&some_broken, (60, 150, 1120, 700)) > 150_000, "{library}: the list with damaged files");
        // What Y came to is read whole, in the list and over the picture, though four hints leave little room beside
        // them (VDEV-ROUND-2026-10-25: "This is no…").
        for state in ["gallery:shelf", "gallery:full-shelf"] {
            let notes = tmp.join(format!("notes-{library}-{}.tsv", state.replace(':', "-")));
            let shot = render_noting(&bin, &tmp, library, state, stub.as_ref(), Some(&notes));
            let cut = std::fs::read_to_string(&notes).unwrap_or_default();
            assert!(!cut.lines().any(|l| l.starts_with("C\tThis is now")), "{library} {state}: the message is cut:\n{cut}");
            assert!(ink(&shot, (60, 870, 1120, 60)) > 1_000, "{library} {state}: the message above the hint bar");
        }
    }
}
