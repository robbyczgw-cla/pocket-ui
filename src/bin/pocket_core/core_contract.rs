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

/// HOME (pause, and HOME again: resume), the screen going dark and on again, all while the start is still held (the
/// Cabinet TV's hand-over or an open case on Cozy, `startup::HOLD_PAUSE`, the first start of a console after its
/// controls card): the game is kept, nothing in it asks to end. 15.11.2026: the crash sweep saw such games end by
/// themselves; it was its parallel runs answering each other's Quit on one /tmp (`scripts/crash-sweep.py` `private_tmp`).
#[test]
fn core_contract_home_and_screen_off_while_the_start_is_held_keep_the_game() {
    save_safety::child("core_contract::core_contract_home_and_screen_off_while_the_start_is_held_keep_the_game", |root| {
        let mut app = save_safety::app(root);
        app.hold = true;
        app.hold_pause = true;
        for wire in ["pause", "pause", "resume", "sleep", "wake", "screen off", "pause", "wake", "resume"] {
            assert_eq!(command(&mut app, wire), None, "{wire}: the game goes on");
            assert!(!app.should_exit(), "{wire}: nothing asks the game to end");
        }
        assert_eq!(flags(&app), (false, false, false, false), "playing again, the hold is over");
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

/// A frame of the game as the core delivered it: `w` x `h`, one grey everywhere.
fn grey_frame(w: u32, h: u32, v: u8) -> Frame {
    Frame { width: w, height: h, pitch: w as usize * 4, format: pocket_ui::libretro::PIXEL_XRGB8888, data: vec![v; (w * h * 4) as usize] }
}

/// Both ways pocket-core draws a picture: GL (ROCKNIX, Android's handheld view) and the Brick's software picture.
const PATHS: [DrawPath; 2] = [DrawPath::Gl { ready: true }, DrawPath::Soft];

/// test060.1 on the Brick (20.11.2026): the console scene and its power light, then the game with no TV coming on. The
/// Brick's software picture returned from `draw` before the GL path's lines that mark the picture as shown, so its
/// switch-on (`switch_on_at`, `SoftScreen::set_crt`) never began: every Cozy start there went without the dot, the line,
/// the AV channel's blue and the snow. On either way of drawing the first picture begins it, its hook goes out, and on the
/// Brick's 1024x768 it covers the game's picture.
#[test]
fn core_contract_first_picture_begins_the_tv_switch_on_on_both_draw_paths() {
    save_safety::child("core_contract::core_contract_first_picture_begins_the_tv_switch_on_on_both_draw_paths", |root| {
        for path in PATHS {
            let mut app = save_safety::app(root);
            app.crt_on = true;
            app.crt_screen = pocket_ui::crt_on::Screen::Tv;
            assert_eq!(app.switch_on_at(), None, "{path:?}: no picture yet, nothing to switch on");
            app.last_frame = Some(grey_frame(256, 224, 0x80));
            app.woke_frame = true;
            app.picture_out(path);
            let (since, screen) = app.switch_on_at().unwrap_or_else(|| panic!("{path:?}: the first picture begins the TV's switch-on"));
            assert_eq!(screen, pocket_ui::crt_on::Screen::Tv);
            assert!(since < pocket_ui::crt_on::DURATION, "{path:?}: it begins at the first picture, not after it is over: {since:?}");
            assert!(app.crt_started, "{path:?}: its hook (and sound) went out");
            assert!(!app.woke_frame, "{path:?}: the wake mark went out with the picture");
            #[cfg(feature = "brick")]
            {
                let (w, h) = (1024usize, 768usize);
                let (x, y, pw, ph) = pocket_ui::soft_screen::placement((256, 224), (w as u32, h as u32));
                let mut screen_px = vec![0x808080u32; w * h];
                let quads = pocket_ui::lcd_on::switch_on(screen, since, (x as f32, y as f32, pw as f32, ph as f32));
                pocket_ui::soft_screen::fill_quads(&mut screen_px, w, h, &quads);
                let changed = screen_px.iter().filter(|&&p| p != 0x808080).count();
                assert!(changed * 2 > (pw * ph) as usize, "{path:?}: the TV's dark covers the picture at 1024x768: {changed} of {} pixels", pw * ph);
            }
        }
    });
}

/// A boot screen's start waits for its first light on either way of drawing (`boot_screen::Wait`): the TV switches on with
/// the logo, not on the black before it.
#[test]
fn core_contract_switch_on_waits_for_a_boot_screen_s_light_on_both_draw_paths() {
    save_safety::child("core_contract::core_contract_switch_on_waits_for_a_boot_screen_s_light_on_both_draw_paths", |root| {
        for path in PATHS {
            let mut app = save_safety::app(root);
            app.crt_on = true;
            app.boot_wait = Some(pocket_ui::boot_screen::Wait::default());
            app.last_frame = Some(grey_frame(320, 240, 0));
            app.picture_out(path);
            assert_eq!(app.switch_on_at(), None, "{path:?}: a black boot screen, the TV waits");
            // GL reads the light off its back buffer before the swap; the software picture off the frame in `picture_out`.
            app.last_frame = Some(grey_frame(320, 240, 0xc0));
            if let DrawPath::Gl { .. } = path {
                app.boot_wait.as_mut().unwrap().see(true, Instant::now());
            }
            app.picture_out(path);
            assert!(app.switch_on_at().is_some(), "{path:?}: the logo's light, the TV switches on");
            assert!(app.boot_wait.is_none(), "{path:?}");
        }
    });
}
