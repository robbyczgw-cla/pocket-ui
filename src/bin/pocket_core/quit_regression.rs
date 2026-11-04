//! Host recovery only: the real exit watchdog ends a child stuck inside serialize, preserving Continue.
use super::*;
use std::process::Command;

fn load(root: &Path) -> Core {
    Core::load(&root.join("stub_libretro.so"), &root.join("game.gb"), root, root, &[]).unwrap()
}

#[test]
fn blocked_serialize_quit_exits_within_deadline_and_continue_restores_previous_moment() {
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
    std::fs::write(&auto, &previous).unwrap();
    let entered = root.join("serialize-entered");
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "psp_quit_regression::blocked_serialize_quit_exits_within_deadline_and_continue_restores_previous_moment", "--nocapture"])
        .env("POCKETUI_TEST_QUIT_CHILD", root).env("POCKETUI_TEST_BLOCK_SERIALIZE", &entered)
        .spawn().unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
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
