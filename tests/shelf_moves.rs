//! A move on Home, guarded (docs/LATENCY.md, "A move on Home"): the binary moves the selection down to the end of
//! Home's list and up to its top in every look and paints the first frame after each move the way `draw` does
//! (`pocket-ui --snapshot bench-shelf:<look>`, which prints its numbers); this test checks them.
//!
//! The speed guard's `nav` is this frame on the Mini V2, and on the device it is mostly the CPU's clock and the
//! compositor (the same frame measured 1.4 ms of CPU there and 3 to 14 ms from the press to the frame, 2026-10-26).
//! What is checked is what pocket-ui itself adds and does not depend on the computer: no frame draws letters of its
//! own or runs strings through the German table's patterns again and again (the rows of Curator, Favorites and the
//! pinned shelves put lines together from the day's pick and the games' names), and no frame is slow in absolute
//! terms. The library is the made-up one of the Curator snapshots: recent games, Favorites, a pinned smart shelf and
//! sticker, so every kind of row and panel is passed.
//!
//! Needs python3 on Linux without a compositor (scripts/wayland-stub.py, as tests/settings_open.rs).

use std::path::{Path, PathBuf};
use std::process::Command;

const ROOT: &str = env!("CARGO_MANIFEST_DIR");

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

#[derive(Debug)]
struct Bench {
    items: usize,
    moves: usize,
    p50_us: f64,
    p95_us: f64,
    glyphs: usize,
    patterns: usize,
    line: String,
}

fn run(tmp: &Path, wayland: &Option<Stub>, look: &str, lang: &str) -> Bench {
    // Its own copy of the data folder: the start writes into it.
    let data = tmp.join(format!("data-{look}-{lang}"));
    let _ = std::fs::remove_dir_all(&data);
    copy_dir(&tmp.join("data"), &data);
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
    cmd.args(["--snapshot", &format!("bench-shelf:{look}"), tmp.join("bench.png").to_str().unwrap()])
        .env("POCKETUI_MOCK", "1")
        .env("POCKETUI_LANG", lang)
        .env("POCKETUI_ES_SYSTEMS", tmp.join("es_systems.cfg"))
        .env("POCKETUI_DATA", &data);
    if let Some(w) = wayland {
        cmd.env("WAYLAND_DISPLAY", &w.path);
    }
    let out = cmd.output().expect("run pocket-ui");
    assert!(out.status.success(), "{look} {lang}: {}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8_lossy(&out.stdout);
    let line = stdout.lines().find(|l| l.starts_with("bench-shelf ")).unwrap_or_else(|| panic!("no bench line in:\n{stdout}")).to_string();
    let field = |name: &str| -> f64 {
        let key = format!(" {name}=");
        let at = line.find(&key).unwrap_or_else(|| panic!("no {name} in {line}")) + key.len();
        line[at..].split_whitespace().next().unwrap().parse().unwrap_or_else(|_| panic!("{name} in {line}"))
    };
    let _ = std::fs::remove_dir_all(&data);
    Bench {
        items: field("items") as usize,
        moves: field("moves") as usize,
        p50_us: field("p50_us"),
        p95_us: field("p95_us"),
        glyphs: field("glyphs") as usize,
        patterns: field("patterns") as usize,
        line,
    }
}

/// What one run of `bench-shelf` must show; the reason when it doesn't.
fn check(look: &str, lang: &str, b: &Bench) -> Result<(), String> {
    if !(b.moves + 1 >= b.items && b.items > 8) {
        return Err(format!("{look} {lang}: over the whole list ({b:?})"));
    }
    // A letter drawn now and then (the clock's next minute) is fine; one in every frame is not. Not the cabinet:
    // its cases glide and grow, and a busy computer catches them at sizes whose letters are drawn on the spot
    // (9df5564 the same; 0 when the computer is idle).
    if look != "cabinet" {
        if b.glyphs * 10 >= b.moves {
            return Err(format!("{look} {lang}: frames drew letters of their own ({b:?})"));
        }
        if b.patterns * 10 >= b.moves {
            return Err(format!("{look} {lang}: frames ran strings through the German patterns ({b:?})"));
        }
    }
    // About 1-2 ms on the build computer and 1.4 ms on the Mini V2 (cards, 2026-10-26); a frame is 16 ms.
    if !(b.p50_us < 6000.0 && b.p95_us < 8000.0) {
        return Err(format!("{look} {lang}: a move costs {:.0} us (p50), {:.0} us (p95)", b.p50_us, b.p95_us));
    }
    Ok(())
}

#[test]
fn a_move_on_home_draws_no_letters_runs_no_patterns_and_is_quick_in_every_look() {
    let _tmp_guard = tempfile::tempdir().unwrap();
    let tmp = _tmp_guard.path().to_path_buf();
    let status = Command::new("python3")
        .args(["-I", &format!("{ROOT}/scripts/curator-fake-library.py"), tmp.to_str().unwrap(), "dark"])
        .stdout(std::process::Stdio::null())
        .status()
        .expect("python3 for scripts/curator-fake-library.py");
    assert!(status.success(), "curator-fake-library.py failed");
    let wayland = wayland_for_snapshots(&tmp);
    // German is where a line can be run through the patterns; English once, for Split, the look of a new Pocket.
    let runs = [("split", "en"), ("split", "de"), ("shelf", "de"), ("minimal", "de"), ("cards", "de"), ("cabinet", "de")];
    for (look, lang) in runs {
        // The numbers are already the frame's thread CPU time (`App::bench_shelf`), and a busy build computer still
        // inflates them (shared caches and hyper-threads, the clock; cabinet de at load 13-17 measured p50 4.6-6.2 ms,
        // p95 6.9-9.5 ms against 6 / 8 ms, and shelf de once 6-8 patterns in 42 moves). Noise only ever adds, so one
        // clean run of three counts: a real regression is in every run, a noisy neighbour in some.
        let mut failures = Vec::new();
        for attempt in 1..=3 {
            let b = run(&tmp, &wayland, look, lang);
            println!("{look} {lang} (run {attempt}): {}", b.line);
            match check(look, lang, &b) {
                Ok(()) => {
                    failures.clear();
                    break;
                }
                Err(why) => failures.push(why),
            }
        }
        assert!(failures.is_empty(), "failed in all 3 runs:\n{}", failures.join("\n"));
    }
    drop(wayland);
}
