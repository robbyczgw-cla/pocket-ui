//! Real Android controller/painter code on a host: navigation and retained moments, not source-string checks.
#![allow(dead_code)]
fn data_dir() -> std::path::PathBuf {
    thread_local! {static DATA: tempfile::TempDir = tempfile::tempdir().unwrap();}
    DATA.with(|d| d.path().to_path_buf())
}
#[path = "../android/src/play_cards.rs"]
mod play_cards;
#[path = "../android/src/ui.rs"]
mod ui;
use pocket_ui::{controls::Target, gfx::Canvas, text::Text, text_size::TextSize, type_scale};
fn bundled_core(system: &str) -> Option<&'static str> {
    static CORES: std::sync::OnceLock<serde_json::Value> = std::sync::OnceLock::new();
    CORES.get_or_init(|| serde_json::from_str(include_str!("../android/cores.lock.json")).unwrap())["cores"]
        .as_array()?
        .iter()
        .find(|c| c["systems"].as_array().is_some_and(|s| s.iter().any(|v| v.as_str() == Some(system))))?["id"]
        .as_str()
}
fn page() -> ui::Ui {
    ui::Ui::new(ui::Preferences {
        onboarding_done: true,
        language: Some(pocket_ui::i18n::Lang::En),
        ..Default::default()
    })
}
fn paint(u: &mut ui::Ui, w: u32, h: u32) -> Vec<u32> {
    let mut p = vec![0; (w * h) as usize];
    u.paint(&mut Canvas::new(&mut p, w, h), &mut Text::builtin(), None);
    p
}
fn tap(u: &mut ui::Ui, label: &str) {
    u.drag(0.0, 10000.0);
    for _ in 0..12 {
        paint(u, 960, 1600);
        if let Some(a) = u
            .regions
            .iter()
            .find(|r| r.label == label || r.label.starts_with(&format!("{label} ·")))
            .map(|r| r.area)
        {
            u.tap((a.x + a.w / 2) as f32, (a.y + a.h / 2) as f32);
            return;
        }
        u.drag(0.0, -350.0);
    }
    panic!("missing {label}: {:?}", u.regions.iter().map(|r| r.label.clone()).collect::<Vec<_>>());
}
#[test]
fn android_back_leaves_nested_pages_before_returning_to_games() {
    let mut u = page();
    u.navigate(ui::Screen::Settings);
    tap(&mut u, "Cozy extras");
    assert!(u.screen == ui::Screen::Extras);
    u.back();
    assert!(u.screen == ui::Screen::Settings);
    u.back();
    assert!(u.screen == ui::Screen::Library);
    u.back();
    assert!(u.screen == ui::Screen::Library);
}
#[test]
fn accessibility_preferences_survive_reload_and_back() {
    let mut u = page();
    u.navigate(ui::Screen::Settings);
    tap(&mut u, "Haptics");
    tap(&mut u, "Remind me where I was");
    tap(&mut u, "Text size");
    u.back();
    let saved = serde_json::to_string(&u.prefs).unwrap();
    let mut reloaded = ui::Ui::new(serde_json::from_str(&saved).unwrap());
    reloaded.navigate(ui::Screen::Settings);
    paint(&mut reloaded, 960, 1600);
    assert!(reloaded.prefs.haptics);
    assert_eq!(reloaded.prefs.remind_where, Some(false));
    assert_eq!(reloaded.prefs.text_size, TextSize::Medium);
    assert!(!page().prefs.haptics, "a fresh install never vibrates by surprise");
}
#[test]
fn android_and_in_app_font_preferences_take_the_larger_size_and_reflow_controls() {
    type_scale::set(TextSize::Normal);
    type_scale::set_system_scale(2.0);
    assert_eq!(type_scale::px(28), 56);
    assert!(type_scale::line(28, 22) > type_scale::px(28) as i32);
    type_scale::set(TextSize::Large);
    type_scale::set_system_scale(1.0);
    assert_eq!(type_scale::px(28), 35);
    let mut text = Text::builtin();
    let mut p = vec![0; 960 * 720];
    let (back, max) = play_cards::paint_controls(&mut Canvas::new(&mut p, 960, 720), &mut text, "gba", &[], 0);
    assert!(max > 0, "all control names remain available through scrolling in landscape");
    assert!(back.y > 100 && back.y + back.h <= 720, "Back remains on screen");
    type_scale::set_system_scale(1.0);
    type_scale::set(TextSize::Normal);
    assert_eq!(play_cards::text_size(TextSize::Large, 1.0), TextSize::Large);
}
#[test]
fn controls_describe_android_buttons_and_only_the_touch_buttons_on_screen() {
    pocket_ui::i18n::set(pocket_ui::i18n::Lang::En);
    let names = vec![
        Target { id: 0, label: "Jump".into() },
        Target { id: 8, label: "Run".into() },
        Target {
            id: 9,
            label: "Turbo Run".into(),
        },
    ];
    let lines = play_cards::control_lines("gba", &names);
    let data = tempfile::tempdir().unwrap();
    play_cards::remember_names(data.path(), "game-with-core", &names).unwrap();
    assert_eq!(
        play_cards::control_lines("gba", &play_cards::remembered_names(data.path(), "game-with-core")),
        lines,
        "the launcher's card keeps the actual core's names after the player exits"
    );
    assert!(
        play_cards::remembered_names(data.path(), "another-game").is_empty(),
        "another game's action names are never guessed"
    );
    assert!(lines.iter().any(|s| s == "A / B: Jump"));
    assert!(lines.iter().any(|s| s == "B / A: Run"));
    assert!(
        lines.iter().any(|s| s == "Y: Turbo Run"),
        "no invented on-screen X button on the two-button GBA pad"
    );
    assert!(play_cards::control_lines("n64", &[]).iter().any(|s| s.contains("C buttons (right stick)")));
}
#[test]
fn pinch_and_drag_reach_dialogue_and_stay_inside_the_frozen_frame() {
    let mut z = play_cards::Zoom::new("gba");
    assert_eq!(z.part(240, 160), (60, 80, 120, 80), "opens where the handheld dialogue is");
    z.magnify(1.5);
    assert_eq!(z.part(240, 160).2, 80);
    z.pan(-100.0, -100.0);
    assert_eq!(z.part(240, 160), (0, 0, 80, 53));
    z.pan(100.0, 100.0);
    let (x, y, w, h) = z.part(240, 160);
    assert_eq!((x + w, y + h), (240, 160));
    z.magnify(0.001);
    assert_eq!(z.part(240, 160), (0, 0, 240, 160));
}
#[test]
fn a_toggled_button_releases_on_pause_and_state_load() {
    let mut held = pocket_ui::controls::HoldToggle::default();
    assert_eq!(held.apply(1, Some(0)), 1);
    assert_eq!(held.apply(0, Some(0)), 1, "release keeps running");
    assert_eq!(held.apply(1, Some(0)), 0, "next tap lets go");
    held.apply(0, Some(0));
    held.apply(1, Some(0));
    held = Default::default();
    assert_eq!(held.apply(0, Some(0)), 0, "returning from a pause/load never keeps a button latched");
}
#[test]
fn old_short_psp_moment_gets_a_clear_explanation_and_remains_loadable() {
    use pocket_ui::{frozen_phone, libretro::Resume, slots};
    let d = tempfile::tempdir().unwrap();
    let base = d.path().join("Game");
    let old_retry = base.with_extension("pstate.auto.taken.retry");
    let old = b"old short PSP moment";
    std::fs::write(old_retry, old).unwrap();
    let notice = frozen_phone::after_resume(&base, Resume::Failed, None);
    assert!(notice.iter().any(|s| s.contains("slot 1")));
    assert_eq!(std::fs::read(slots::state_path(&base, 0)).unwrap(), old);
    assert!(play_cards::recovery_note("ppsspp", Resume::Failed).unwrap().contains("in-game save is safe"));
    assert!(play_cards::recovery_note("ppsspp", Resume::Resumed).is_none());
}
#[test]
fn a_return_after_a_long_break_is_quiet_and_optional() {
    use pocket_ui::where_was_i::{card, Facts, AWAY};
    let f = Facts {
        on: true,
        last_played: Some(100),
        play_secs: 3600,
        easy: true,
        note: "",
        ..Default::default()
    };
    assert!(card(&f, 101).is_none());
    assert!(card(&f, 101 + AWAY).unwrap().played.is_none());
    assert!(card(&Facts { on: false, ..f }, 101 + AWAY).is_none());
}

#[test]
fn the_game_card_controls_page_returns_to_the_same_game() {
    use pocket_ui::{document_library::Game, pad::Button};
    let mut u = page();
    let game = Game {
        name: "Garden".into(),
        key: "test:garden".into(),
        uri: "content://test/garden".into(),
        relative: "gb/Garden.gb".into(),
        system: "Game Boy",
        system_id: "gb",
        profile: "retroarch:mgba",
        color: 0,
        size: 32768,
        missing: vec![],
    };
    play_cards::remember_names(&data_dir(), &ui::game_key(&game, false), &[Target { id: 0, label: "Jump".into() }]).unwrap();
    u.install(ui::library(vec![game]));
    u.press(Button::North);
    assert!(u.screen == ui::Screen::Box);
    tap(&mut u, "Controls");
    paint(&mut u, 960, 1600);
    assert!(u.regions.iter().any(|r| r.label.contains("Android gamepad")));
    assert!(
        u.regions.iter().any(|r| r.label.contains("Jump")),
        "game-card Controls keeps the running core's names after returning to the launcher"
    );
    u.back();
    assert!(u.screen == ui::Screen::Box);
    assert_eq!(u.game().unwrap().name, "Garden");
    u.press(Button::R2);
    paint(&mut u, 1600, 960);
    assert!(u.regions.iter().any(|r| r.label.contains("Android gamepad")));
    u.back();
    assert!(u.screen == ui::Screen::Box);
}
