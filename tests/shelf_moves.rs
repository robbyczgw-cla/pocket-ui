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
    sealed: usize,
    line: String,
}

fn run(tmp: &Path, wayland: &Option<Stub>, look: &str, lang: &str) -> Bench {
    run_case(tmp, wayland, look, lang, "1024x768", None)
}

fn run_case(tmp: &Path, wayland: &Option<Stub>, look: &str, lang: &str, size: &str, sealed: Option<&str>) -> Bench {
    // Its own copy of the data folder: the start writes into it.
    let data = tmp.join(format!("data-{look}-{lang}"));
    let _ = std::fs::remove_dir_all(&data);
    copy_dir(&tmp.join("data"), &data);
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
    cmd.args(["--snapshot", &format!("bench-shelf:{look}"), tmp.join("bench.png").to_str().unwrap()])
        .env("POCKETUI_MOCK", "1").env("POCKETUI_MOCK_NOW", FIXTURE_NOW)
        .env_remove("POCKETUI_SEALED")
        .env("POCKETUI_SNAPSHOT_SIZE", size)
        .env("POCKETUI_LANG", lang)
        .env("POCKETUI_ES_SYSTEMS", tmp.join("es_systems.cfg"))
        .env("POCKETUI_DATA", &data);
    if let Some(sealed) = sealed { cmd.env("POCKETUI_SEALED", sealed); }
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
        sealed: field("sealed") as usize,
        line,
    }
}

/// What one run of `bench-shelf` must show; the reason when it doesn't.
fn check(look: &str, lang: &str, b: &Bench) -> Result<(), String> {
    if !(b.moves + 1 >= b.items && b.items > 8) {
        return Err(format!("{look} {lang}: over the whole list ({b:?})"));
    }
    // A letter drawn now and then (the clock's next minute) is fine; one in every frame is not. Cabinet’s moving
    // cases are built at intermediate sizes; its separate film guard compares the same animation moment.
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
    let status = Command::new("python3").env("POCKETUI_MOCK_NOW", FIXTURE_NOW)
        .args(["-I", &format!("{ROOT}/scripts/curator-fake-library.py"), tmp.to_str().unwrap(), "dark"])
        .stdout(std::process::Stdio::null())
        .status()
        .expect("python3 for scripts/curator-fake-library.py");
    assert!(status.success(), "curator-fake-library.py failed");
    let wayland = wayland_for_snapshots(&tmp);
    // German is where a line can be run through the patterns; English once, for Split, the look of a new Pocket.
    let runs = [("split", "en"), ("split", "de"), ("shelf", "de"), ("cabinet", "de")];
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

/// A new library must scroll as quickly in its default film as an opened collection. More games than the front
/// caches retain: a tiny or already-played library hides the cost of making the next sealed cover.
#[test]
fn new_games_in_film_scroll_as_quickly_as_opened_games() {
    let tmp = tempfile::tempdir().unwrap();
    assert!(Command::new("python3").args(["-I", &format!("{ROOT}/scripts/perf-library.py"), tmp.path().to_str().unwrap(), "80", "400", "560"])
        .stdout(std::process::Stdio::null()).status().unwrap().success());
    let wayland = wayland_for_snapshots(tmp.path());
    for (look, size) in [("cabinet:gbc", "640x480"), ("split:gbc", "640x480"), ("big:gbc", "640x480"), ("big:gbc:settled", "640x480"), ("cabinet:gbc", "720x720")] {
        let mut failures = Vec::new();
        for _ in 0..3 {
            let opened = run_case(tmp.path(), &wayland, look, "en", size, Some("0"));
            let sealed = run_case(tmp.path(), &wayland, look, "en", size, None);
            assert_eq!(opened.sealed, 0);
            assert_eq!(sealed.sealed, 80, "the bench must actually exercise its default film");
            let theme = if look.starts_with("big") { "look=Big" } else if look.starts_with("split") { "look=Split" } else { "look=Cabinet" };
            assert!(sealed.line.contains(theme), "{}", sealed.line);
            assert!(sealed.line.contains(&format!("changes={}", sealed.moves)), "every measured press must select another game: {}", sealed.line);
            assert!(sealed.line.contains(if look.ends_with(":settled") { "moment_ms=100.0" } else { "moment_ms=16.7" }), "{}", sealed.line);
            println!("opened: {}\nsealed by default: {}", opened.line, sealed.line);
            // Thread CPU, relative to the same frame and machine. Allow clock/cache noise, but not rebuilding the
            // expensive glare at every newly selected case (the original regression was 3–5 times slower). The
            // remaining pixel blend must stay cheap too: faster bare case templates exposed its cost on x86-64.
            if sealed.p50_us <= opened.p50_us * 1.20 + 100.0 && sealed.p95_us <= opened.p95_us * 1.20 + 150.0 { failures.clear(); break; }
            failures.push(format!("{look} {size}: film {:.0}/{:.0} vs opened {:.0}/{:.0} us (p50/p95)", sealed.p50_us, sealed.p95_us, opened.p50_us, opened.p95_us));
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }
}
