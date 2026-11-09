//! Characterisation of the shipped FIFO before replacing its string dispatch.
use super::*;

fn command(app: &mut App, line: &str) -> Option<Quit> {
    app.early = Some(line.to_string());
    app.handle_commands()
}

fn flags(app: &App) -> (bool, bool, bool, bool) {
    (app.paused, app.sleeping, app.dark, app.hold)
}

#[test]
fn core_contract_wake_keeps_the_menu_paused_and_screen_off_keeps_playing() {
    save_safety::child("core_contract::core_contract_wake_keeps_the_menu_paused_and_screen_off_keeps_playing", |root| {
        let mut app = save_safety::app(root);
        app.hold = true;
        app.crt_on = true;
        for (wire, expected) in [
            ("pause", (true, false, false, false)),
            ("sleep", (true, true, false, false)),
            ("wake", (true, false, false, false)),
            ("resume", (false, false, false, false)),
            ("screen off", (false, false, true, false)),
            ("pause", (true, false, true, false)),
            ("wake", (true, false, false, false)),
            ("sleep", (true, true, false, false)),
            ("resume", (false, true, false, false)),
            ("wake", (false, false, false, false)),
        ] {
            assert_eq!(command(&mut app, wire), None);
            assert_eq!(flags(&app), expected, "legacy {wire}");
            assert!(!app.crt_on, "pause/sleep/wake dismiss the startup effect");
        }
    });
}

#[test]
fn core_contract_slot_defaults_extra_words_and_exact_named_loads_stay_compatible() {
    save_safety::child("core_contract::core_contract_slot_defaults_extra_words_and_exact_named_loads_stay_compatible", |root| {
        let mut app = save_safety::app(root);
        app.slot = 2;
        for wire in ["save", "save 0", "save 9", "save invalid"] {
            command(&mut app, wire);
            assert!(slots::state_path(&app.states, 2).is_file(), "{wire}: current slot fallback");
        }
        app.core.run(0);
        command(&mut app, "save 2 extra words");
        let second = pocket_ui::state_file::read(&slots::state_path(&app.states, 1)).unwrap();
        app.core.run(0);
        command(&mut app, "load 2 extra words");
        assert_eq!(app.core.save_state().unwrap(), second);
        let current = pocket_ui::state_file::read(&slots::state_path(&app.states, 2)).unwrap();
        app.core.run(0);
        pocket_ui::state_file::write(&slots::auto_path(&app.states), &app.core.save_state().unwrap()).unwrap();
        command(&mut app, "load auto extra");
        assert_eq!(app.core.save_state().unwrap(), current, "named Auto is exact; extra words use current slot");
        for (wire, slot) in [("slot 0", 2), ("slot 2 extra", 1), ("slot 9", 1), ("slot 8", 7)] {
            command(&mut app, wire);
            assert_eq!(app.slot, slot, "{wire}");
        }
    });
}

#[test]
fn core_contract_look_defaults_and_set_arity_preserve_the_picture() {
    save_safety::child("core_contract::core_contract_look_defaults_and_set_arity_preserve_the_picture", |root| {
        let mut app = save_safety::app(root);
        command(&mut app, " look 12 invalid 6500 extra ");
        assert_eq!((app.hud.look.warmth, app.hud.look.contrast, app.hud.look.night), (10, 0, 6500));
        command(&mut app, "look\t999 -2 wrong");
        assert_eq!((app.hud.look.warmth, app.hud.look.contrast, app.hud.look.night), (0, -2, 0));
        let before = app.video.frame;
        command(&mut app, "set frame ambient extra");
        assert_eq!(app.video.frame, before, "set needs exactly key and value");
        command(&mut app, "set frame ambient");
        assert_eq!(app.video.frame, pocket_ui::video::FrameMode::Ambient);
    });
}

#[test]
fn core_contract_only_exact_quit_words_exit_and_nothing_after_quit_runs() {
    save_safety::child("core_contract::core_contract_only_exact_quit_words_exit_and_nothing_after_quit_runs", |root| {
        let mut app = save_safety::app(root);
        for wire in ["quit later", "quit nosave extra", "quit handoff extra", "quitnosave", "unknown"] {
            assert_eq!(command(&mut app, wire), None, "{wire}");
        }
        for (wire, expected) in [(" quit ", Quit::Save), ("quit\tnosave", Quit::SkipResume), ("quit handoff", Quit::ForSwitch)] {
            assert_eq!(command(&mut app, wire), Some(expected), "{wire}");
        }
        let (tx, rx) = std::sync::mpsc::channel();
        app.commands = rx;
        for wire in ["slot 2", "quit nosave", "slot 3"] { tx.send(wire.to_string()).unwrap(); }
        assert_eq!(app.handle_commands(), Some(Quit::SkipResume));
        assert_eq!(app.slot, 1, "commands after the terminal quit in that batch never execute");
    });
}

#[test]
fn core_contract_status_reports_real_state_without_private_game_data_and_only_on_change() {
    save_safety::child("core_contract::core_contract_status_reports_real_state_without_private_game_data_and_only_on_change", |root| {
        let mut app = save_safety::app(root);
        app.video_src.1 = root.join("private-game-name.gb");
        let path = root.join("pocket-core.status");
        let read = || std::fs::read_to_string(&path).expect("an agent must be able to read the core state");
        command(&mut app, "sleep");
        let asleep = read();
        assert!(asleep.starts_with("v=1\trev=1\tpid="));
        assert!(asleep.contains("\tplay=asleep\t"));
        assert!(!asleep.contains("private-game-name") && !asleep.contains(root.to_str().unwrap()));
        command(&mut app, "pause");
        assert!(read().contains("\tplay=asleep\t") && read().contains("\tmenu=paused\t"));
        command(&mut app, "wake");
        let paused = read();
        assert!(paused.contains("\tplay=menu-paused\t"));
        command(&mut app, "pause");
        assert_eq!(read(), paused, "repeated pause does not rewrite the status");
        command(&mut app, "resume");
        assert!(read().contains("\tplay=playing\t"));
        command(&mut app, "screen off");
        assert!(read().contains("\tplay=dark-playing\t"));
        command(&mut app, "quit nosave");
        let exiting = read();
        assert!(exiting.contains("\tplay=exiting\t") && exiting.contains("\tquit=skip-resume\t"));
    });
}

#[test]
fn core_contract_status_failure_never_blocks_commands_or_quitting() {
    save_safety::child("core_contract::core_contract_status_failure_never_blocks_commands_or_quitting", |root| {
        let mut app = save_safety::app(root);
        let blocked = root.join("blocked");
        std::fs::write(&blocked, b"not a directory").unwrap();
        app.status = pocket_ui::core_status::Publisher::new(blocked.join("status"));
        assert_eq!(command(&mut app, "sleep"), None);
        assert!(app.sleeping);
        assert_eq!(command(&mut app, "wake"), None);
        assert!(!app.sleeping);
        command(&mut app, "slot 4");
        assert_eq!(app.slot, 3);
        assert_eq!(command(&mut app, "quit handoff"), Some(Quit::ForSwitch));
        assert!(!blocked.join("status").is_file(), "the command path really encountered a status write failure");
    });
}
