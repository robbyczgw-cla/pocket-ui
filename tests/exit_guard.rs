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
    let taken = stuck_rx.recv_timeout(test_clock::timeout(Duration::from_secs(5))).expect("the watcher fires for a core that hangs");
    assert!(taken >= Duration::from_millis(300) && taken < test_clock::timeout(Duration::from_secs(3)), "{taken:?}");
    assert!(done.try_recv().is_err(), "the hanging call has not returned");
}

#[test]
#[ignore = "timing tier: scripts/test-timing.sh (serial)"]
fn timing_a_core_that_answers_is_left_alone() {
    let calls = CoreCalls::new();
    let (stuck_tx, stuck_rx) = channel();
    calls.watch(Duration::from_millis(300), Duration::from_millis(50), move |taken| {
        let _ = stuck_tx.send(taken);
    });
    let done = save_state_on_thread("fine", false, calls.clone());
    let state = done.recv_timeout(test_clock::timeout(Duration::from_secs(5))).expect("the stub answers");
    assert_eq!(state, Some(vec![7u8; 16]));
    // Idle time between the calls is not a call.
    assert!(stuck_rx.recv_timeout(Duration::from_millis(600)).is_err(), "no false alarm");
    calls.stop();
}

#[test]
#[ignore = "timing tier: scripts/test-timing.sh (serial)"]
fn timing_a_timed_out_call_cannot_start_writing_while_fallback_runs() {
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
    entered_rx.recv_timeout(test_clock::timeout(Duration::from_secs(5))).unwrap();
    calls.watch(Duration::ZERO, Duration::from_millis(1), move |_| {
        fallback_tx.send(()).unwrap();
        finish_rx.recv().unwrap();
    });
    fallback_rx.recv_timeout(test_clock::timeout(Duration::from_secs(5))).unwrap();
    release_tx.send(()).unwrap();
    assert!(written_rx.recv_timeout(Duration::from_millis(100)).is_err());
    finish_tx.send(()).unwrap();
    written_rx.recv_timeout(test_clock::timeout(Duration::from_secs(5))).unwrap();
    save.join().unwrap();
}

static NO_TERM: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[test]
#[ignore = "timing tier: scripts/test-timing.sh (serial)"]
fn timing_a_pause_during_a_shortcut_save_waits_until_the_slot_is_durable() {
    use pocket_ui::exit_guard::Progress;
    let progress = Progress::new();
    progress.busy(false); // Already playing, unlike a FIFO command's busy section.
    let (stuck_tx, stuck_rx) = channel();
    progress.watch(limits(5000, 50), Duration::from_millis(5), &NO_TERM, move || { let _ = stuck_tx.send(()); });
    let dir = tempfile::tempdir().unwrap();
    let slot = dir.path().join("Game.pstate");
    let requested = progress.clone();
    let request = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(20));
        requested.request(); // The player opens the menu while HOME + Save is still writing.
    });
    progress.work(|| {
        std::thread::sleep(Duration::from_millis(250)); // Slow core/card, below the busy limit.
        pocket_ui::state_file::write(&slot, b"the current game moment").unwrap();
    });
    // The next frame/loop still has time to pick up the Pause after the slow work returns.
    std::thread::sleep(Duration::from_millis(20));
    progress.answered();
    progress.stop();
    request.join().unwrap();
    assert!(stuck_rx.try_recv().is_err(), "a shortcut save must not get a terminal stall verdict before it finishes");
    assert_eq!(pocket_ui::state_file::read(&slot).unwrap(), b"the current game moment");

    let next = Progress::new();
    next.busy(false);
    next.work(|| next.work(|| {}));
    let (tx, rx) = channel();
    next.watch(limits(5000, 50), Duration::from_millis(5), &NO_TERM, move || { let _ = tx.send(()); });
    next.request();
    rx.recv_timeout(test_clock::timeout(Duration::from_millis(300))).expect("nested save work must restore frame-time recovery afterwards");
}

fn limits(stall: u64, unanswered: u64) -> pocket_ui::exit_guard::Limits {
    let ms = |n| test_clock::timeout(Duration::from_millis(n));
    pocket_ui::exit_guard::Limits { stall: ms(stall), unanswered: ms(unanswered), busy: ms(600), exiting: ms(600) }
}

#[test]
#[ignore = "timing tier: scripts/test-timing.sh (serial)"]
fn timing_an_unhandled_pause_or_quit_is_bounded_even_if_frames_and_more_requests_arrive() {
    let progress = pocket_ui::exit_guard::Progress::new();
    let (tx, rx) = channel();
    progress.watch(limits(5000, 100), Duration::from_millis(5), &NO_TERM, move || { tx.send(()).unwrap(); });
    progress.busy(false);
    progress.request();
    let keep_alive = progress.clone();
    let worker = std::thread::spawn(move || {
        for _ in 0..30 {
            keep_alive.request(); // repeated menu/quit inputs must not extend the first request's deadline
            keep_alive.checkpoint(); // a frame is not an acknowledgement of the queued command
            std::thread::sleep(Duration::from_millis(5));
        }
    });
    rx.recv_timeout(test_clock::timeout(Duration::from_secs(2))).expect("an unhandled command ends the stalled player");
    worker.join().unwrap();
}

#[test]
#[ignore = "timing tier: scripts/test-timing.sh (serial)"]
fn timing_a_responsive_game_can_stay_paused_without_a_false_recovery() {
    let progress = pocket_ui::exit_guard::Progress::new();
    let (tx, rx) = channel();
    progress.watch(limits(200, 100), Duration::from_millis(5), &NO_TERM, move || { tx.send(()).unwrap(); });
    progress.request();
    progress.answered();
    for _ in 0..30 {
        progress.checkpoint(); // paused event loop still handles commands
        std::thread::sleep(Duration::from_millis(10));
    }
    progress.stop();
    assert!(rx.try_recv().is_err());
}

/// Quit right after "Save to slot" on a 50 MB state: the save takes seconds and the Quit must wait for it, not end the
/// game halfway and lose both the slot and the exit save.
#[test]
#[ignore = "timing tier: scripts/test-timing.sh (serial)"]
fn timing_a_quit_during_a_slow_state_save_waits_for_it() {
    let progress = pocket_ui::exit_guard::Progress::new();
    let (tx, rx) = channel();
    progress.watch(limits(5000, 50), Duration::from_millis(5), &NO_TERM, move || { tx.send(()).unwrap(); });
    progress.busy(true);
    progress.request();
    assert!(rx.recv_timeout(Duration::from_millis(300)).is_err(), "a slow save is no hang");
    progress.busy(false);
    progress.answered();
    progress.stop();
    // Still bounded: a save that never returns ends the player at `Limits::busy`.
    let stuck = pocket_ui::exit_guard::Progress::new();
    let (tx, rx) = channel();
    stuck.watch(limits(5000, 50), Duration::from_millis(5), &NO_TERM, move || { tx.send(()).unwrap(); });
    stuck.request();
    rx.recv_timeout(test_clock::timeout(Duration::from_secs(2))).expect("a save that never returns is a hang");
}

/// A second Quit (or pocket-ui's SIGTERM) while the exit save is still writing must not cut it short.
#[test]
#[ignore = "timing tier: scripts/test-timing.sh (serial)"]
fn timing_another_quit_during_the_exit_save_does_not_end_it_halfway() {
    static TERM: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    let progress = pocket_ui::exit_guard::Progress::new();
    let (tx, rx) = channel();
    progress.watch(limits(5000, 50), Duration::from_millis(5), &TERM, move || { tx.send(()).unwrap(); });
    progress.exiting();
    progress.request();
    TERM.store(true, std::sync::atomic::Ordering::SeqCst);
    assert!(rx.recv_timeout(Duration::from_millis(400)).is_err(), "the exit save goes on");
    rx.recv_timeout(test_clock::timeout(Duration::from_secs(2))).expect("an exit that never ends is still bounded");
}

/// The Odin 2 Mini's screen off (05.11.2026): pocket-ui switched the output off, pocket-core's swap waited for a frame
/// the compositor never asked for, and the stall guard ended the game after 120 s with the moment lost. Waiting for the
/// display is no stall: here 130 "seconds" (the limits scaled down, the screen off for 13 times the stall limit) with a
/// Pause arriving meanwhile, and the game is still there afterwards; the guard is armed again once the frame went out.
#[test]
#[ignore = "timing tier: scripts/test-timing.sh (serial)"]
fn timing_a_game_waiting_for_a_switched_off_screen_for_130_s_is_not_ended() {
    use pocket_ui::exit_guard::{Limits, Progress};
    static TERMINATE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    let ms = |n| test_clock::timeout(Duration::from_millis(n));
    let progress = Progress::new();
    progress.busy(false);
    let (stuck_tx, stuck_rx) = channel();
    progress.watch(Limits { stall: ms(100), unanswered: ms(50), busy: ms(80), exiting: ms(100) }, ms(5), &TERMINATE, move || {
        let _ = stuck_tx.send(());
    });
    progress.checkpoint();
    let waiting = progress.clone();
    let swap = std::thread::spawn(move || {
        waiting.display(|| {
            std::thread::sleep(ms(300));
            // pocket-ui's `sleep` (or a Pause) while the frame waits: no answer is possible until the screen is on.
            waiting.request();
            std::thread::sleep(ms(1000));
        })
    });
    swap.join().unwrap();
    assert!(stuck_rx.try_recv().is_err(), "a game waiting for its screen must not be ended");
    // The frame went out: the Pause is answered within its patience, and the turns go on.
    progress.answered();
    for _ in 0..10 {
        std::thread::sleep(ms(20));
        progress.checkpoint();
    }
    assert!(stuck_rx.try_recv().is_err(), "normal play after the wait");
    // Still guarded: a core that really stops now is ended at the stall limit.
    stuck_rx.recv_timeout(test_clock::timeout(Duration::from_secs(5))).expect("a real stall after the wait is still noticed");
}

#[path = "../src/test_clock.rs"]
mod test_clock;
