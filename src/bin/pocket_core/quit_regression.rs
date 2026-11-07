//! Host recovery only: the real exit watchdog ends a child stuck inside serialize, preserving Continue.
#[path = "../../test_clock.rs"]
mod test_clock;
use super::*;
use std::process::Command;

fn load(root: &Path) -> Core {
    Core::load(&root.join("stub_libretro.so"), &root.join("game.gb"), root, root, &[]).unwrap()
}

#[test]
#[ignore = "timing tier: scripts/test-timing.sh (serial)"]
fn timing_blocked_serialize_quit_exits_within_deadline_and_continue_restores_previous_moment() {
    // Run the actual terminal callback in a child: it uses _exit to avoid stuck core destructors.
    if let Some(root) = std::env::var_os("POCKETUI_TEST_QUIT_CHILD") {
        let root = PathBuf::from(root);
        let mut core = load(&root);
        assert_eq!(core.resume(&slots::auto_path(&root.join("game"))), pocket_ui::libretro::Resume::Resumed);
        core.run(0); // Unsaved progress must not overwrite the earlier valid Continue moment.
        spawn_exit_watchdog(Duration::from_millis(300), root.join("game"));
        CORE_CALLS.during(|| core.save_state());
        panic!("the fake core must block forever in serialize");
    }
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    std::fs::write(root.join("game.gb"), [0u8; 32]).unwrap();
    std::fs::write(root.join("stub.c"), include_str!("../../../tests/common/blocked_serialize.c")).unwrap();
    let output = Command::new("cc").args(["-shared", "-fPIC", "-o"]).arg(root.join("stub_libretro.so"))
        .arg(root.join("stub.c")).output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let previous = {
        let core = load(root);
        for _ in 0..30 { core.run(0); }
        core.save_state().unwrap()
    };
    let auto = slots::auto_path(&root.join("game"));
    pocket_ui::state_file::write(&auto, &previous).unwrap();
    let entered = root.join("serialize-entered");
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "psp_quit_regression::timing_blocked_serialize_quit_exits_within_deadline_and_continue_restores_previous_moment", "--nocapture", "--ignored"])
        .env("POCKETUI_TEST_QUIT_CHILD", root).env("POCKETUI_TEST_BLOCK_SERIALIZE", &entered)
        .spawn().unwrap();
    let deadline = Instant::now() + test_clock::timeout(Duration::from_secs(5));
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() { break status; }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("Quit must release the game process within 5 seconds so the shelf can return");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    assert!(status.success() && entered.exists(), "the watchdog must end a core stuck in serialize");
    let mut core = load(root);
    assert_eq!(core.resume(&auto), pocket_ui::libretro::Resume::ResumedAgain);
    assert_eq!(core.save_state().unwrap(), previous, "Continue restores the previous marker, not the unsaved frame");
}

#[test]
#[ignore = "timing tier: scripts/test-timing.sh (serial)"]
fn timing_stalled_player_returns_to_shelf_and_preserves_continue_in_every_phase() {
    if let Some(root) = std::env::var_os("POCKETUI_TEST_STALL_CHILD") {
        let root = PathBuf::from(root);
        unsafe { libc::signal(libc::SIGTERM, on_sigterm as extern "C" fn(libc::c_int) as libc::sighandler_t); }
        let ms = Duration::from_millis;
        spawn_progress_watchdog(pocket_ui::exit_guard::Limits { stall: ms(700), unanswered: ms(150), busy: ms(400), exiting: ms(700) });
        let phase = std::env::var("POCKETUI_TEST_OPERATION").unwrap();
        let mut core = load(&root);
        match phase.as_str() {
            "load" => panic!("load must block"),
            "run" => { PROGRESS.busy(false); core.run(0); }
            "size" => { core.save_state(); }
            "resume" => { core.resume(&slots::auto_path(&root.join("game"))); }
            "retry-run" => {
                let resumed = core.resume(&slots::auto_path(&root.join("game")));
                assert_eq!(resumed, Resume::ResumedAgain);
                remember_resumed_moment(&core, resumed);
                PROGRESS.busy(false);
                core.run(0);
            }
            "unload" => { PROGRESS.exiting(); drop(core) }
            _ => unreachable!(),
        }
        panic!("stub must block");
    }
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    std::fs::write(root.join("game.gb"), [0u8; 32]).unwrap();
    std::fs::write(root.join("stub.c"), include_str!("../../../tests/common/blocked_serialize.c")).unwrap();
    let output = Command::new("cc").args(["-shared", "-fPIC", "-o"]).arg(root.join("stub_libretro.so"))
        .arg(root.join("stub.c")).output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let previous = load(root).save_state().unwrap();
    let auto = slots::auto_path(&root.join("game"));
    // Exercise an ordinary Auto and the retry of a previous crash. A Continue that hangs the core counts like one that
    // crashes it: the Auto is tried once more, a retry that hangs too is left for Load only.
    for (phase, signal, taken) in [("load", false, false), ("run", false, false), ("run", true, false),
                                  ("size", false, false), ("resume", false, false), ("resume", false, true), ("retry-run", false, true), ("unload", false, false)] {
        let path = if taken { slots::taken_auto_path(&auto) } else { auto.clone() };
        pocket_ui::state_file::write(&path, &previous).unwrap();
        let entered = root.join("phase-entered");
        let _ = std::fs::remove_file(&entered);
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "psp_quit_regression::timing_stalled_player_returns_to_shelf_and_preserves_continue_in_every_phase", "--nocapture", "--ignored"])
            .env("POCKETUI_TEST_STALL_CHILD", root).env("POCKETUI_TEST_OPERATION", phase)
            .env("POCKETUI_TEST_BLOCK_PHASE", if phase == "retry-run" { "run" } else { phase }).env("POCKETUI_TEST_ENTERED", &entered)
            .spawn().unwrap();
        let started = Instant::now();
        let mut sent = false;
        let status = loop {
            if signal && !sent && entered.exists() {
                assert_eq!(unsafe { libc::kill(child.id() as libc::pid_t, libc::SIGTERM) }, 0);
                sent = true;
            }
            if let Some(status) = child.try_wait().unwrap() { break status; }
            if started.elapsed() > test_clock::timeout(Duration::from_secs(5)) {
                let _ = child.kill(); let _ = child.wait();
                panic!("{phase}: game must release its process so the shelf can return");
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        assert!(status.success() && entered.exists(), "{phase}: terminal guard ends the blocked player");
        if signal { assert!(sent); }
        let mut healthy = load(root);
        if phase == "resume" && taken {
            assert_eq!(pocket_ui::state_file::read(&slots::retry_auto_path(&auto)).unwrap(), previous, "Load still has it");
            assert_eq!(healthy.resume(&auto), Resume::NoState, "a moment that hung the core twice is not tried a third time");
        } else {
            // Resume the actual old marker in a healthy core after recovery, rather than merely checking file existence.
            assert!(matches!(healthy.resume(&auto), Resume::Resumed | Resume::ResumedAgain), "{phase}");
            assert_eq!(healthy.save_state().unwrap(), previous, "{phase}: previous moment unchanged");
        }
        for f in [auto.clone(), slots::taken_auto_path(&auto), slots::retry_auto_path(&auto)] { let _ = std::fs::remove_file(f); }
    }
}

#[test]
fn a_moment_that_crashes_the_core_while_loading_is_not_tried_at_every_start() {
    if let Some(root) = std::env::var_os("POCKETUI_TEST_CRASH_CHILD") {
        let root = PathBuf::from(root);
        load(&root).resume(&slots::auto_path(&root.join("game")));
        panic!("stub must abort in unserialize");
    }
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    std::fs::write(root.join("game.gb"), [0u8; 32]).unwrap();
    std::fs::write(root.join("stub.c"), include_str!("../../../tests/common/blocked_serialize.c")).unwrap();
    let output = Command::new("cc").args(["-shared", "-fPIC", "-o"]).arg(root.join("stub_libretro.so"))
        .arg(root.join("stub.c")).output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let previous = load(root).save_state().unwrap();
    let auto = slots::auto_path(&root.join("game"));
    pocket_ui::state_file::write(&auto, &previous).unwrap();
    // Two starts die inside retro_unserialize (a core bug the moment triggers), as a segfault would.
    for start in 0..2 {
        let status = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "psp_quit_regression::a_moment_that_crashes_the_core_while_loading_is_not_tried_at_every_start", "--nocapture"])
            .env("POCKETUI_TEST_CRASH_CHILD", root).env("POCKETUI_TEST_ABORT_PHASE", "resume")
            .output().unwrap().status;
        assert!(!status.success(), "start {start} must crash");
    }
    // The third start is fresh, so the game can be played again; Load still lists the moment.
    let mut core = load(root);
    assert_eq!(core.resume(&auto), Resume::NoState);
    assert_eq!(pocket_ui::state_file::read(&slots::retry_auto_path(&auto)).unwrap(), previous);
}
