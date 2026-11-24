//! Save failures against a real stub core, without a compositor, audio or player files.
use super::*;
use std::process::Command;

pub(super) fn app(root: &Path) -> App {
    let core = Core::load(&root.join("stub.so"), &root.join("Game.gb"), root, root, &[]).unwrap();
    let input = Arc::new(Inputs::new(Vec::new(), None));
    let touch = Arc::new(pocket_ui::touch::Shared::default());
    let (_, commands) = std::sync::mpsc::channel();
    let ex = Extras::new(&Default::default(), Power::with_root(root, root.join("restore.json")));
    App {
        status: pocket_ui::core_status::Publisher::new(root.join("pocket-core.status")),
        core,
        window: None,
        gfx: None, #[cfg(feature = "brick")] soft: None, #[cfg(feature = "brick")] brick_gles: None, #[cfg(feature = "brick")] brick_picture: BrickPicture::default(),
        audio: None,
        audio_opening: None,
        shown_at: None,
        quiet_picture: false,
        input,
        touch,
        mapping: Mapping::defaults("gb"),
        stick_pad: Default::default(),
        exit_since: None,
        frames: 0,
        stats: Stats { emulated: 0, started: Instant::now(), last_swap: None, intervals: Vec::new(), run_ms: Vec::new(), draw_ms: Vec::new(), strip_us: Vec::new() },
        bench: None,
        period: Duration::from_micros(16_667),
        pacer: pocket_ui::core_pacing::Pacer::new(60.0, 60.0),
        pacing: Default::default(),
        pacing_at: Instant::now(),
        last_frame: None,
        album: None,
        album_ring: std::collections::VecDeque::new(),
        album_moment: false,
        states: root.join("Game"),
        commands,
        video: VideoSettings::default(),
        slot: 0,
        video_src: ("gb".into(), root.join("Game.gb")),
        paused: false,
        sleeping: false,
        dark: false,
        hold: false,
        hold_still: false,
        hold_pause: false,
        fade_in: Default::default(),
        hold_down: None,
        crt_on: false,
        crt_started: false,
        crt_screen: pocket_ui::crt_on::Screen::Tv,
        boot_wait: None,
        woke_frame: false,
        held_mask: [0; players::MAX],
        ds_pad: Default::default(),
        toggled: Default::default(),
        view: (0, 0),
        early: None,
        ex,
        hud: Hud::default(),
        nominal_samples: 1600,
        audio_peak: 0,
        rotate_override: None,
        inject_release: Vec::new(),
        resume_in: None,
        ra_options: Vec::new(),
        cheevos: None,
        hardcore: false,
        pause_wait: false,
        chime: Default::default(),
        shots: None,
        chime_gain: 0.0,
        pixel_window: Default::default(),
        pixel_live: Default::default(),
        discs: None,
        timeline: Default::default(),
        paused_at: None,
    }
}

pub(super) fn child(name: &str, test: impl FnOnce(&Path)) {
    if let Some(root) = std::env::var_os("POCKET_SAVE_SAFETY_CHILD") {
        test(Path::new(&root));
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let source = include_str!("../../../tests/common/blocked_serialize.c").replace("#include \"POCKET_SHARED_STUB\"", include_str!("../../../scripts/crash-sweep/stub-core.c")).replace(
        "memset(d, marker, s); return true;", "if (getenv(\"POCKET_SAVE_REFUSE\")) return false; memset(d, marker, s); return true;");
    let source = if name.contains("double_refusal") {
        source.replace("#define NO_SRAM", "")
            .replace("static uint8_t sram[SRAM_SIZE];", "static uint8_t sram[SRAM_SIZE], rtc[8];")
            .replace("id == 0 ? sram : NULL", "id == 0 ? sram : id == 1 ? rtc : NULL")
            .replace("id == 0 ? sizeof sram : 0", "id == 0 ? sizeof sram : id == 1 ? sizeof rtc : 0")
            .replace("marker = *(const unsigned char *)d; return true;",
                     "marker = *(const unsigned char *)d; if (getenv(\"POCKET_TEST_REFUSE_LOAD\")) { sram[0] = rtc[0] = 99; return false; } sram[0] = rtc[0] = marker; return true;")
    } else {
        source
    };
    std::fs::write(root.path().join("stub.c"), source).unwrap();
    assert!(Command::new("cc").args(["-shared", "-fPIC", "-o"]).arg(root.path().join("stub.so"))
        .arg(root.path().join("stub.c")).arg("-lm").status().unwrap().success());
    std::fs::write(root.path().join("Game.gb"), b"game").unwrap();
    let qualified = if name.contains("::") { name.to_string() } else { format!("save_safety::{name}") };
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", &qualified, "--nocapture"])
        .env("POCKET_SAVE_SAFETY_CHILD", root.path()).env("POCKETUI_DATA", root.path()).output().unwrap();
    assert!(output.status.success(), "{}\n{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
}

#[test]
fn save_safety_failed_exit_never_announces_saved() {
    child("save_safety_failed_exit_never_announces_saved", |root| {
        let mut app = app(root);
        let auto = slots::auto_path(&app.states);
        std::fs::create_dir(&auto).unwrap(); // Atomic replacement fails, even when tests run as root.
        let legacy = pocket_ui::state_file::legacy_path(&auto).unwrap();
        let previous = app.core.save_state().unwrap();
        std::fs::write(&legacy, &previous).unwrap();
        app.save_for_handoff();
        assert!(!pocket_ui::fast_switch::requested(), "a failed exit save cannot announce saved");
        assert_eq!(std::fs::read(&legacy).unwrap(), previous, "failed quit keeps the earlier Auto");
        let notice = std::fs::read_to_string(root.join("pocket-core.error")).unwrap();
        assert!(notice.contains("Couldn't save latest progress"), "the shelf must explain the failed save: {notice}");
        std::fs::remove_dir(&auto).unwrap();
        app.save_for_handoff();
        assert!(pocket_ui::fast_switch::requested(), "a later successful exit still permits fast switching");
        assert!(!root.join("pocket-core.error").exists(), "successful retry clears its old failure");
    });
}

#[test]
fn save_safety_failed_auto_never_enters_earlier() {
    child("save_safety_failed_auto_never_enters_earlier", |root| {
        let mut app = app(root);
        let auto = slots::auto_path(&app.states);
        let previous = app.core.save_state().unwrap();
        pocket_ui::state_file::write(&auto, &previous).unwrap();
        let now = Instant::now();
        for n in 0..=2001 { app.timeline.play(now + Duration::from_millis(n * 150)); }
        assert!(app.timeline.due());
        std::env::set_var("POCKET_SAVE_REFUSE", "1");
        app.autosave();
        app.timeline.wait();
        assert_eq!(pocket_ui::state_file::read(&auto).unwrap(), previous);
        assert_eq!(app.timeline.written().written, 0, "an older Auto must not masquerade as the new Earlier moment");
    });
}

#[test]
fn save_safety_core_input_gate_drops_shortcuts_and_a_last_dark_touch() {
    child("save_safety_core_input_gate_drops_shortcuts_and_a_last_dark_touch", |root| {
        use pocket_ui::pad::{Button, PadEvent};
        let mut app = app(root);
        for (enter, leave) in [("pause", "resume"), ("sleep", "wake"), ("screen off", "wake")] {
            app.early = Some(enter.into());
            app.handle_commands();
            {
                let mut hotkeys = app.ex.hotkeys.lock().unwrap();
                for (button, pressed) in [(Button::Mode, true), (Button::R1, true), (Button::R1, false), (Button::Mode, false)] {
                    hotkeys.feed(&PadEvent::Button { source: 0, button, pressed });
                }
            }
            if enter == "screen off" {
                // Arrives just before wake, after the last dark frame had already polled input.
                app.touch.feed(&pocket_ui::touch::TouchEvent { phase: pocket_ui::touch::Phase::Down, x: 0.2, y: 0.3 });
            }
            app.early = Some(leave.into());
            app.handle_commands();
            assert!(app.ex.hotkeys.lock().unwrap().take().is_empty(), "{enter}: no hidden save on resume");
            assert_eq!(app.touch.frame_for_game(false), None, "the final dark touch belongs to no game");
        }
    });
}

#[test]
fn save_safety_double_refusal_preserves_battery_and_moments_until_recovery() {
    child("save_safety_double_refusal_preserves_battery_and_moments_until_recovery", |root| {
        for recover in [true, false] {
            double_refusal(root, recover);
        }
    });
}

#[cfg(test)]
fn double_refusal(root: &Path, recover: bool) {
    let battery = root.join("Game.srm");
    let rtc = root.join("Game.rtc");
    let original = vec![7; 8192];
    std::fs::write(&battery, &original).unwrap();
    std::fs::write(&rtc, [7; 8]).unwrap();
    let mut app = app(root);
    let auto = slots::auto_path(&app.states);
    let slot = slots::state_path(&app.states, 0);
    let good = vec![4; 16];
    for path in [&auto, &slot] {
        pocket_ui::state_file::write(path, &good).unwrap();
    }

    std::env::set_var("POCKET_TEST_REFUSE_LOAD", "1");
    app.load_slot(0);
    app.core.write_sram_in_background();
    let _ = app.core.write_sram();
    assert_eq!(std::fs::read(&battery).unwrap(), original, "flushes must preserve the battery save");
    assert_eq!(std::fs::read(&rtc).unwrap(), [7; 8], "flushes must preserve the cartridge clock");
    assert!(app.paused);
    for command in ["resume", "restart", "save", "autosave"] {
        app.early = Some(command.into());
        app.handle_commands();
    }
    assert!(app.paused);
    assert_eq!(pocket_ui::state_file::read(&auto).unwrap(), good);
    assert_eq!(pocket_ui::state_file::read(&slot).unwrap(), good);

    let undo = slots::undo_path(&app.states);
    let before = pocket_ui::state_file::read(&undo).unwrap();
    app.load_slot(0);
    app.load_state_file(&root.join("missing.state"), "missing");
    assert!(app.paused);
    assert!(!app.save_auto().complete());
    assert_eq!(pocket_ui::state_file::read(&undo).unwrap(), before);

    if !recover {
        assert!(!app.save_exit().complete());
        drop(app);
        assert_eq!(std::fs::read(&battery).unwrap(), original, "Quit and Drop must preserve the battery save");
        assert_eq!(std::fs::read(&rtc).unwrap(), [7; 8]);
        assert_eq!(pocket_ui::state_file::read(&auto).unwrap(), good);
        return;
    }
    std::env::remove_var("POCKET_TEST_REFUSE_LOAD");
    app.load_slot(0);
    app.early = Some("resume".into());
    app.handle_commands();
    assert!(!app.paused);
    drop(app);
    let mut recovered = original;
    recovered[0] = 4;
    assert_eq!(std::fs::read(&battery).unwrap(), recovered);
    assert_eq!(std::fs::read(&rtc).unwrap(), [4, 7, 7, 7, 7, 7, 7, 7]);
}
