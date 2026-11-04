//! Actual pocket-core refreshes under a private X server. Run with --features x11 --test launch_hold -- --ignored.
//! No hardware, shared command FIFO, sound server or player data is needed.
#![cfg(all(target_os = "linux", feature = "x11"))]

use std::io::{BufRead, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

struct Process(Child);
impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

const CORE: &str = r#"
#include <stdbool.h>
#include <stddef.h>
#include <stdlib.h>
#include <stdio.h>
#include <string.h>
struct info { const char *name, *version, *exts; bool fullpath, extract; };
struct av { unsigned bw, bh, mw, mh; float aspect; double fps, rate; };
static unsigned pos;
static void (*video)(const void *, unsigned, unsigned, size_t);
void retro_set_environment(void *f) {}
void retro_set_video_refresh(void *f) { video = f; }
void retro_set_audio_sample(void *f) {}
void retro_set_audio_sample_batch(void *f) {}
void retro_set_input_poll(void *f) {}
void retro_set_input_state(void *f) {}
void retro_set_controller_port_device(unsigned p, unsigned d) {}
void retro_init(void) { pos = 0; }
void retro_deinit(void) {}
void retro_get_system_info(struct info *i) { memset(i, 0, sizeof *i); i->name = "Hold test"; i->version = "1"; i->exts = "gb"; i->fullpath = true; }
void retro_get_system_av_info(struct av *a) { memset(a, 0, sizeof *a); a->bw = a->mw = 16; a->bh = a->mh = 16; a->fps = 60; a->rate = 48000; }
bool retro_load_game(const void *g) { return true; }
void retro_unload_game(void) {}
void retro_reset(void) { pos = 0; }
void *retro_get_memory_data(unsigned id) { return NULL; }
size_t retro_get_memory_size(unsigned id) { return 0; }
size_t retro_serialize_size(void) { return sizeof pos; }
bool retro_serialize(void *d, size_t n) { if (n < sizeof pos) return false; memcpy(d, &pos, sizeof pos); return true; }
bool retro_unserialize(const void *d, size_t n) { if (n != sizeof pos) return false; memcpy(&pos, d, sizeof pos); return true; }
void retro_run(void) {
    pos++;
    const char *path = getenv("HOLD_TEST_RUNS");
    if (path) { FILE *f = fopen(path, "ab"); if (f) { fwrite(&pos, sizeof pos, 1, f); fclose(f); } }
    static unsigned short frame[256];
    if (video) video(frame, 16, 16, 32);
}
"#;

fn wait_for(mut done: impl FnMut() -> bool, log: &Path) {
    let until = Instant::now() + Duration::from_secs(3);
    while Instant::now() < until {
        if done() { return; }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("host did not answer: {}", std::fs::read_to_string(log).unwrap_or_default());
}

#[test]
#[ignore = "host check: requires Xvfb and Mesa; run with --features x11"]
fn resumed_game_stays_at_its_moment_and_every_exit_from_the_hold_works() {
    let dir = tempfile::tempdir().unwrap();
    let mut x = Process(Command::new("Xvfb").args(["-displayfd", "1", "-screen", "0", "640x480x24", "-nolisten", "tcp"])
        .stdout(Stdio::piped()).stderr(Stdio::null()).spawn().expect("Xvfb"));
    let mut display = String::new();
    std::io::BufReader::new(x.0.stdout.take().unwrap()).read_line(&mut display).unwrap();
    let display = format!(":{}", display.trim());
    let source = dir.path().join("core.c");
    let so = dir.path().join("hold_libretro.so");
    std::fs::write(&source, CORE).unwrap();
    assert!(Command::new("cc").args(["-shared", "-fPIC", "-o"]).arg(&so).arg(source).status().unwrap().success());

    for scenario in ["quit", "skip", "press", "sleep", "wake-only", "dark", "pause", "deadline", "failed", "missing", "normal", "picture-pending"] {
        let root = dir.path().join(scenario);
        let states = root.join("states/gb");
        std::fs::create_dir_all(&states).unwrap();
        let rom = root.join("Game.gb");
        std::fs::write(&rom, b"game").unwrap();
        let auto = states.join("Game.pstate.auto");
        {
            let core = pocket_ui::libretro::Core::load(&so, &rom, &root, &root, &[]).unwrap();
            for _ in 0..7 { core.run(0); }
            if scenario != "missing" {
                std::fs::write(&auto, if scenario == "failed" { b"bad state".to_vec() } else { core.save_state().unwrap() }).unwrap();
            }
        }
        let log = root.join("core.log");
        let trace = root.join("runs");
        let skip = root.join("skip");
        let ready = root.join("ready");
        if scenario != "picture-pending" { std::fs::write(&ready, b"").unwrap(); }
        let mut game = Process(Command::new(env!("CARGO_BIN_EXE_pocket-core"))
            .args([so.as_os_str(), rom.as_os_str(), "--system".as_ref(), "gb".as_ref()])
            .env("WINIT_UNIX_BACKEND", "x11").env("DISPLAY", &display).env_remove("WAYLAND_DISPLAY")
            .env("POCKETUI_DATA", &root).env("POCKETUI_STATES", root.join("states"))
            .env("TMPDIR", &root).env("POCKET_CORE_TEST", "1").env("HOLD_TEST_RUNS", &trace)
            .env("POCKET_LAUNCH_HOLD_SKIP", &skip)
            .env("POCKET_LAUNCH_HOLD_READY", &ready)
            .env("POCKET_LAUNCH_HOLD_STILL", if scenario == "normal" { "0" } else { "1" })
            .env("POCKET_LAUNCH_HOLD_UNTIL", (pocket_ui::startup::clock_ms() + if scenario == "deadline" { 1500.0 } else { 5900.0 }).to_string())
            .stdout(std::fs::File::create(&log).unwrap()).stderr(Stdio::null()).spawn().unwrap());
        let fifo = root.join("pocket-core.cmd");
        wait_for(|| std::fs::read_to_string(&log).unwrap_or_default().contains("display "), &log);
        let send = |cmd: &str| {
            use std::os::unix::fs::OpenOptionsExt;
            let mut f = std::fs::OpenOptions::new().write(true).custom_flags(libc::O_NONBLOCK).open(&fifo).unwrap();
            writeln!(f, "{cmd}").unwrap();
        };
        if !["failed", "missing", "normal", "picture-pending"].contains(&scenario) {
            std::thread::sleep(Duration::from_millis(150));
            assert!(!trace.exists(), "{scenario}: moment advanced while held");
        }
        match scenario {
            "skip" => std::fs::write(&skip, b"").unwrap(),
            "press" => send("press B 150"),
            "sleep" => send("sleep\nwake"),
            "wake-only" => send("wake"),
            "dark" => send("screen off"),
            "pause" => send("pause\nresume"),
            _ => {}
        }
        if scenario != "quit" {
            wait_for(|| std::fs::metadata(&trace).is_ok_and(|m| m.len() >= 4), &log);
            let bytes = std::fs::read(&trace).unwrap();
            assert_eq!(u32::from_ne_bytes(bytes[..4].try_into().unwrap()), if ["failed", "missing"].contains(&scenario) { 1 } else { 8 }, "{scenario}: first played frame");
        }
        if scenario == "normal" {
            wait_for(|| std::fs::read_to_string(&log).unwrap_or_default().contains("first frame"), &log);
        }
        send("quit");
        wait_for(|| game.0.try_wait().unwrap().is_some(), &log);
        assert!(game.0.wait().unwrap().success(), "{scenario}: {}", std::fs::read_to_string(&log).unwrap());
        if scenario == "quit" {
            let mut core = pocket_ui::libretro::Core::load(&so, &rom, &root, &root, &[]).unwrap();
            assert_eq!(core.resume(&auto), pocket_ui::libretro::Resume::Resumed);
            let position = core.save_state().unwrap();
            // Compare with a fresh core advanced to the original moment: Quit preserved precisely that point.
            drop(core);
            let core = pocket_ui::libretro::Core::load(&so, &rom, &root, &root, &[]).unwrap();
            for _ in 0..7 { core.run(0); }
            assert_eq!(position, core.save_state().unwrap(), "quit during the hold preserves the moment");
        }
    }
}
