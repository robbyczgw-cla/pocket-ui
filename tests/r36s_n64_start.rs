//! A startup stall must leave evidence even when the core never produces a frame.
#![cfg(all(target_os = "linux", not(feature = "brick")))]

use std::io::Write;
use std::process::{Child, Command, Stdio};
use std::time::Duration;
use pocket_ui::report::{self, Sources, Stamp};

#[path = "../src/test_clock.rs"]
mod test_clock;

struct Player(Child);

impl Drop for Player {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn loading_sample(log: &str) -> bool {
    log.lines().any(|line| line.starts_with("n64 start: waiting phase=loading core and game ")
        && line.contains("main=") && line.contains("read_bytes=") && line.contains("mem_available_kib="))
}

#[test]
fn report_log_has_live_n64_start_phases_even_when_the_core_never_draws() {
    report_log_has_live_phases(&[]);
}

/// The watch thread first runs after the core's dlopen began (a busy handheld; nightly 27.11.2026 under the full suite):
/// it still samples the stall, the start waits for it to be ready.
#[test]
fn a_late_n64_start_watch_still_samples_a_core_stuck_in_its_load() {
    report_log_has_live_phases(&[("POCKET_CORE_TEST_LATE_WATCH_MS", "300")]);
}

fn report_log_has_live_phases(extra: &[(&str, &str)]) {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path();
    let stub = p.join("stub.c");
    let core = p.join("mupen64plus_next_libretro.so");
    std::fs::write(&stub, r#"
#include <errno.h>
#include <unistd.h>
/* Stay in dlopen until the test has collected the report. */
__attribute__((constructor)) static void stall(void) {
    char command;
    ssize_t n;
    do {
        n = read(STDIN_FILENO, &command, 1);
    } while (n < 0 && errno == EINTR);
    _exit(n == 1 && command == 'q' ? 38 : 37);
}
"#).unwrap();
    assert!(Command::new("cc").args(["-shared", "-fPIC"]).arg(&stub).arg("-o").arg(&core).status().unwrap().success());
    let rom = p.join("test.z64");
    std::fs::write(&rom, [0x80, 0x37, 0x12, 0x40]).unwrap();
    // Let pocket-core write the live log as the launcher does, then collect the report while dlopen is stuck.
    let logs = p.join("logs");
    std::fs::create_dir(&logs).unwrap();
    let log_path = logs.join("pocket-ui.log");
    let log = std::fs::File::create(&log_path).unwrap();
    let mut player = Player(Command::new(env!("CARGO_BIN_EXE_pocket-core"))
        .arg(&core).arg(&rom).args(["--system", "n64"])
        .env("POCKETUI_MOCK", "1").env("POCKETUI_DEVICE", "r36s").env("POCKET_CORE_TEST", "1")
        .env("POCKETUI_DATA", p).env("TMPDIR", p).envs(extra.iter().copied())
        .stdin(Stdio::piped()).stdout(log.try_clone().unwrap()).stderr(log)
        .spawn().unwrap());
    // The hang guard names what pocket-core wrote: which phase it was in when no loading sample came (nightly 27.11.:
    // two misses under the full suite's load said nothing about where the start stood).
    let end = std::time::Instant::now() + test_clock::timeout(Duration::from_secs(30));
    loop {
        let log = std::fs::read_to_string(&log_path).unwrap();
        if let Some(status) = player.0.try_wait().unwrap() {
            panic!("pocket-core exited before the report ({status}):\n{log}");
        }
        if loading_sample(&log) { break; }
        assert!(std::time::Instant::now() < end, "no progress waiting for a live N64 waiting sample in the loading phase:\n{log}");
        std::thread::sleep(Duration::from_millis(5));
    }
    let source = Sources {
        root: p.join("root"), storage: p.join("storage"), battery: p.join("battery"),
        data_dir: p.to_path_buf(), live_log: p.join("live.log"), pocket_core: p.join("no-player"),
        out_dir: p.join("reports"), card_dir: Some(p.join("TF2/reports")),
    };
    let entries = report::entries(&source, &[], 0, &Stamp::at(0));
    let (_, ui_log) = entries.iter().find(|(name, _)| name == "logs/pocket-ui.log").unwrap();
    let log = String::from_utf8_lossy(ui_log);
    assert!(log.contains("n64 start: loading core and game"), "{log}");
    assert!(loading_sample(&log), "{log}");
    assert!(!log.contains("n64 start: first game picture"), "{log}");
    assert!(player.0.try_wait().unwrap().is_none(), "the report was collected while the core was still loading");

    player.0.stdin.take().unwrap().write_all(b"q").unwrap();
    let mut status = None;
    test_clock::wait_for(|| {
        status = player.0.try_wait().unwrap();
        status.is_some()
    }, Duration::from_secs(10), "the stalled test core to exit");
    assert_eq!(status.unwrap().code(), Some(38), "the stub reached its deliberate exit");
}
