//! A huge card's shelves (a player's R36S card with 179,305 files), checked through
//! the binary: `pocket-ui --snapshot bench-bigcard:<look>` walks from shelf to shelf and along the biggest one, frame by
//! frame as the device paints, and prints what it cost. Here a GBC library copied to systems of 100 and 22,000 games
//! (`BIGCARD_SIZES`), the size of the R36S owner's ZX Spectrum folder.
//!
//! * Switching to a system's shelf copies no game's title, system and ROM for every game of it (`case_info`): a row of
//!   22,000 games was laid out with three string copies per game, three times over (the build computer: 8.7 ms against 2.7 ms for
//!   100 games; the R36S's A35 five to ten times that).
//! * The covers' look-ups ask the card nothing for a name its picture folder does not hold (`pocket_ui::dir_index`): on
//!   exFAT each missing name walks the whole folder, and with ES-DE's folders every game on screen was looked for ten
//!   times.
//!
//! Neither showed in the other benches: their libraries have at most a few hundred games, all with covers.
//!
//! Needs python3 on Linux without a compositor (scripts/wayland-stub.py, as tests/shelf_moves.rs).

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

/// `bench-bigcard`'s numbers for one look.
#[derive(Debug)]
struct Run {
    reached: usize,
    lookups_switching: usize,
    lookups_moving: usize,
    infos_max: usize,
    cpu_max_ms: f64,
    lines: String,
}

fn run(tmp: &Path, wayland: &Option<Stub>, look: &str) -> Run {
    let data = tmp.join(format!("data-{look}"));
    let _ = std::fs::remove_dir_all(&data);
    copy_dir(&tmp.join("data"), &data);
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
    cmd.args(["--snapshot", &format!("bench-bigcard:{look}"), tmp.join("bench.png").to_str().unwrap()])
        .env("POCKETUI_MOCK", "1")
        .env("POCKETUI_SNAPSHOT_SIZE", "640x480")
        .env("POCKETUI_LANG", "en")
        .env("POCKETUI_ES_SYSTEMS", tmp.join("es_systems.cfg"))
        .env("POCKETUI_DATA", &data)
        .env("POCKETUI_ES_HOME", tmp.join("es-home"))
        .env("BIGCARD_SIZES", "100,22000,100,22000")
        .env("BIGCARD_ROUNDS", "2")
        .env("BIGCARD_MOVES", "60");
    if let Some(w) = wayland {
        cmd.env("WAYLAND_DISPLAY", &w.path);
    }
    let out = cmd.output().expect("run pocket-ui");
    assert!(out.status.success(), "{look}: {}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8_lossy(&out.stdout);
    let lines: Vec<&str> = stdout.lines().filter(|l| l.starts_with("bench-bigcard ")).collect();
    let field = |name: &str| -> f64 {
        let key = format!(" {name}=");
        let line = lines.iter().find(|l| l.contains(&key)).unwrap_or_else(|| panic!("no {name} in:\n{stdout}"));
        let at = line.find(&key).unwrap() + key.len();
        line[at..].split_whitespace().next().unwrap().parse().unwrap_or_else(|_| panic!("{name} in {line}"))
    };
    let _ = std::fs::remove_dir_all(&data);
    Run {
        reached: field("reached") as usize,
        lookups_switching: field("lookups_switching") as usize,
        lookups_moving: field("lookups_moving") as usize,
        infos_max: field("infos_max") as usize,
        cpu_max_ms: field("cpu_max_ms"),
        lines: lines.join("\n"),
    }
}

#[test]
fn a_shelf_of_22000_games_switches_without_copying_each_game_or_asking_the_card_for_missing_covers() {
    let tmp = tempfile::tempdir().unwrap();
    // 40 GBC games with box art; the bench copies them to the big systems under names that have none.
    assert!(Command::new("python3")
        .args(["-I", &format!("{ROOT}/scripts/perf-library.py"), tmp.path().to_str().unwrap(), "40", "80", "112"])
        .stdout(std::process::Stdio::null())
        .status()
        .unwrap()
        .success());
    // ES-DE's picture folder on the card, with the box of each of the 40: the copies have none there, and each of them is
    // looked for under five folders and two extensions (`gamelist::Index::cover_candidates`).
    let media = tmp.path().join("es-home/ES-DE/downloaded_media/gbc/covers");
    std::fs::create_dir_all(&media).unwrap();
    for rom in std::fs::read_dir(tmp.path().join("roms/gbc")).unwrap().flatten() {
        let stem = rom.path().file_stem().unwrap().to_string_lossy().into_owned();
        std::fs::write(media.join(format!("{stem}.png")), b"\x89PNG\r\n\x1a\n").unwrap();
    }
    let wayland = wayland_for_snapshots(tmp.path());
    let mut wrong = Vec::new();
    for look in ["cabinet", "shelf"] {
        let r = run(tmp.path(), &wayland, look);
        println!("{look}:\n{}", r.lines);
        let mut check = |ok: bool, why: String| {
            if !ok {
                wrong.push(format!("{look}: {why}"));
            }
        };
        check(r.reached == 8, format!("{} of 8 shelves opened (both rounds)", r.reached));
        // A frame copies the entries it shows (a few dozen), never the 22,000 of the row (main: 66,000 and more).
        check(r.infos_max < 2_000, format!("a frame made {} copies of games' title, system and ROM", r.infos_max));
        // Along a row of games without covers on the card: no look-up there at all once the folders are listed (main:
        // ten per game coming into view). The switches: the folders' times, a few per shelf.
        check(r.lookups_moving <= 4, format!("{} look-ups moving along games without covers", r.lookups_moving));
        check(r.lookups_switching <= 60, format!("{} look-ups switching 8 times", r.lookups_switching));
        // Thread CPU on the build computer, noise only adds: a generous bound against the walk getting slow as a whole.
        check(r.cpu_max_ms < 80.0, format!("a frame took {:.1} ms of CPU", r.cpu_max_ms));
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    drop(wayland);
}
