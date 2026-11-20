//! RetroAchievements' Hardcore in the host (`achievements::hardcore`), against a real stub core: no Load of any kind,
//! no Save, Continue starts from the power-on (the moment stays on the card), and switching Hardcore on mid-game starts
//! the game again. The stub's state is one byte (`marker`), so what a state brought back is plain to see.
use super::*;
use std::process::Command;

/// The stub of `save_safety`, whose reset button sets the marker to 0 (a restart is visible).
fn child(name: &str, test: impl FnOnce(&Path)) {
    if let Some(root) = std::env::var_os("POCKET_SAVE_SAFETY_CHILD") {
        test(Path::new(&root));
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let source = include_str!("../../../tests/common/blocked_serialize.c").replace("#include \"POCKET_SHARED_STUB\"", include_str!("../../../scripts/crash-sweep/stub-core.c")).replace("void retro_reset(void) {}", "void retro_reset(void) { marker = 0; }");
    assert!(source.contains("marker = 0; }"), "the stub's reset changed");
    std::fs::write(root.path().join("stub.c"), source).unwrap();
    assert!(Command::new("cc").args(["-shared", "-fPIC", "-o"]).arg(root.path().join("stub.so")).arg(root.path().join("stub.c")).arg("-lm").status().unwrap().success());
    std::fs::write(root.path().join("Game.gb"), b"game").unwrap();
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", &format!("hardcore_rules::{name}"), "--nocapture"])
        .env("POCKET_SAVE_SAFETY_CHILD", root.path())
        .env("POCKETUI_DATA", root.path())
        .output()
        .unwrap();
    assert!(output.status.success(), "{}\n{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
}

/// What the game holds now: the stub's one byte.
fn moment(app: &mut App) -> u8 {
    app.core.save_state().unwrap()[0]
}

fn command(app: &mut App, line: &str) {
    app.early = Some(line.into());
    app.handle_commands();
}

#[test]
fn hardcore_loads_no_state_and_continue_starts_fresh() {
    child("hardcore_loads_no_state_and_continue_starts_fresh", |root| {
        // Without Hardcore the moment Quit kept comes back: what the guard below has to stop.
        let mut soft = save_safety::app(root);
        pocket_ui::state_file::write(&slots::auto_path(&soft.states), &[0x66; 16]).unwrap();
        soft.resume();
        assert_eq!(moment(&mut soft), 0x66, "softcore continues where it quit");
        drop(soft);

        let mut app = save_safety::app(root);
        let auto = slots::auto_path(&app.states);
        pocket_ui::state_file::write(&auto, &[0x66; 16]).unwrap();
        pocket_ui::state_file::write(&slots::state_path(&app.states, 0), &[0x55; 16]).unwrap();
        app.hardcore = true;
        app.resume();
        let fresh = moment(&mut app);
        assert_ne!(fresh, 0x66, "Continue in Hardcore loads no state: the game starts from its power-on");
        assert!(slots::auto_candidates(&auto, false).iter().any(|p| p.exists()), "the moment stays for later");
        // No Load of any kind: a slot, Where I quit, Before load, Earlier.
        for line in ["load 1", "load auto", "load undo", "load earlier1 0", "load started1 0"] {
            command(&mut app, line);
            assert_eq!(moment(&mut app), fresh, "{line} loaded a state in Hardcore");
        }
        assert!(!slots::undo_path(&app.states).exists(), "nothing was loaded, so nothing to undo");
        // No Save either (a slot written in Hardcore could be loaded once it is off).
        command(&mut app, "save 2");
        assert!(!slots::state_path(&app.states, 1).exists(), "Save wrote a state in Hardcore");
        // Hardcore off mid-game: Load works again at once.
        command(&mut app, "hardcore off");
        assert!(!app.hardcore());
        command(&mut app, "load 1");
        assert_eq!(moment(&mut app), 0x55, "off: the slot loads");
    });
}

#[test]
fn hardcore_on_mid_game_starts_the_game_again() {
    child("hardcore_on_mid_game_starts_the_game_again", |root| {
        let mut app = save_safety::app(root);
        pocket_ui::state_file::write(&slots::state_path(&app.states, 0), &[0x55; 16]).unwrap();
        command(&mut app, "load 1");
        assert_eq!(moment(&mut app), 0x55, "softcore: a loaded moment");
        app.ex.ff_on = true;
        command(&mut app, "hardcore on");
        assert!(app.hardcore(), "on for the rest of the game");
        assert_eq!(moment(&mut app), 0, "the game started again from its power-on (the reset button)");
        assert!(app.ex.ff_on, "fast-forward goes on: RetroAchievements allows it in Hardcore, as RetroArch does");
        command(&mut app, "load 1");
        assert_eq!(moment(&mut app), 0, "and from now on nothing loads");
        // The fast-forward shortcut still switches it in Hardcore (only rewind, states and cheats are out).
        app.hotkey(Action::ToggleFastForward);
        assert!(!app.ex.ff_on);
        app.hotkey(Action::ToggleFastForward);
        assert!(app.ex.ff_on, "HOME + fast-forward works in Hardcore");
    });
}
