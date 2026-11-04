//! Quit with a core that never hands over its state (PPSSPP's emulation thread and the main thread waiting for each other):
//! the stub core hangs in `retro_serialize_size`, as PPSSPP does, and the watcher of the exit save has to notice.

use std::sync::mpsc::channel;
use std::time::Duration;

use pocket_ui::exit_guard::CoreCalls;
use pocket_ui::libretro::Core;

const STUB: &str = r#"
#include <stdbool.h>
#include <stddef.h>
#include <string.h>
#include <unistd.h>
struct info { const char *name, *version, *exts; bool need_fullpath, block_extract; };
struct av { unsigned bw, bh, mw, mh; float aspect; double fps, rate; };
struct game { const char *path; const void *data; size_t size; const char *meta; };
void retro_set_environment(void *f) {}
void retro_set_video_refresh(void *f) {}
void retro_set_audio_sample(void *f) {}
void retro_set_audio_sample_batch(void *f) {}
void retro_set_input_poll(void *f) {}
void retro_set_input_state(void *f) {}
void retro_set_controller_port_device(unsigned port, unsigned device) {}
unsigned retro_api_version(void) { return 1; }
void retro_init(void) {}
void retro_deinit(void) {}
void retro_get_system_info(struct info *i) { memset(i, 0, sizeof *i); i->name = "stub"; i->exts = "gb"; }
void retro_get_system_av_info(struct av *a) { memset(a, 0, sizeof *a); a->bw = a->mw = 160; a->bh = a->mh = 144; a->fps = 60; a->rate = 48000; }
bool retro_load_game(const struct game *g) { return true; }
void retro_unload_game(void) {}
void retro_reset(void) {}
void retro_run(void) {}
void *retro_get_memory_data(unsigned id) { return NULL; }
size_t retro_get_memory_size(unsigned id) { return 0; }
size_t retro_serialize_size(void) { if (HANG) for (;;) usleep(1000); return 16; }
bool retro_serialize(void *d, size_t s) { memset(d, 7, s); return true; }
bool retro_unserialize(const void *d, size_t s) { return true; }
"#;

/// Builds the stub, loads it on a thread of its own (the host state is per thread) and takes one state through `calls`.
/// The thread of a hanging stub never ends; the test process does.
fn save_state_on_thread(name: &str, hang: bool, calls: std::sync::Arc<CoreCalls>) -> std::sync::mpsc::Receiver<Option<Vec<u8>>> {
    let dir = tempfile::Builder::new().prefix(&format!("pocketui-exit-guard-{name}-")).tempdir().unwrap().keep();
    std::fs::write(dir.join("stub.c"), STUB).unwrap();
    std::fs::write(dir.join("game.gb"), [0u8; 32]).unwrap();
    let core = dir.join("stub_libretro.so");
    let out = std::process::Command::new("cc")
        .args(["-shared", "-fPIC", &format!("-DHANG={}", hang as u8), "-o"])
        .arg(&core)
        .arg(dir.join("stub.c"))
        .output()
        .expect("the exit guard test builds a stub core and needs a C compiler (cc)");
    assert!(out.status.success(), "cc: {}", String::from_utf8_lossy(&out.stderr));
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let core = Core::load(&core, &dir.join("game.gb"), &dir, &dir, &[]).expect("the stub loads");
        // dlopen keeps the library mapped; the stub does not access these files again.
        let _ = std::fs::remove_dir_all(&dir);
        let state = calls.during(|| core.save_state());
        let _ = tx.send(state);
        let _ = std::fs::remove_dir_all(&dir);
    });
    rx
}

#[test]
fn a_core_hanging_in_serialize_size_is_noticed_within_the_limit() {
    let calls = CoreCalls::new();
    let (stuck_tx, stuck_rx) = channel();
    calls.watch(Duration::from_millis(300), Duration::from_millis(50), move |taken| {
        let _ = stuck_tx.send(taken);
    });
    let done = save_state_on_thread("hang", true, calls.clone());
    let taken = stuck_rx.recv_timeout(Duration::from_secs(5)).expect("the watcher fires for a core that hangs");
    assert!(taken >= Duration::from_millis(300) && taken < Duration::from_secs(3), "{taken:?}");
    assert!(done.try_recv().is_err(), "the hanging call has not returned");
}

#[test]
fn a_core_that_answers_is_left_alone() {
    let calls = CoreCalls::new();
    let (stuck_tx, stuck_rx) = channel();
    calls.watch(Duration::from_millis(300), Duration::from_millis(50), move |taken| {
        let _ = stuck_tx.send(taken);
    });
    let done = save_state_on_thread("fine", false, calls.clone());
    let state = done.recv_timeout(Duration::from_secs(5)).expect("the stub answers");
    assert_eq!(state, Some(vec![7u8; 16]));
    // Idle time between the calls is not a call.
    assert!(stuck_rx.recv_timeout(Duration::from_millis(600)).is_err(), "no false alarm");
    calls.stop();
}

#[test]
fn a_timed_out_call_cannot_start_writing_while_fallback_runs() {
    let calls = CoreCalls::new();
    let (entered_tx, entered_rx) = channel();
    let (release_tx, release_rx) = channel();
    let (fallback_tx, fallback_rx) = channel();
    let (finish_tx, finish_rx) = channel();
    let (written_tx, written_rx) = channel();
    let active = calls.clone();
    let save = std::thread::spawn(move || {
        active.during(|| {
            entered_tx.send(()).unwrap();
            release_rx.recv().unwrap();
        });
        written_tx.send(()).unwrap();
    });
    entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    calls.watch(Duration::ZERO, Duration::from_millis(1), move |_| {
        fallback_tx.send(()).unwrap();
        finish_rx.recv().unwrap();
    });
    fallback_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    release_tx.send(()).unwrap();
    assert!(written_rx.recv_timeout(Duration::from_millis(100)).is_err());
    finish_tx.send(()).unwrap();
    written_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    save.join().unwrap();
}
