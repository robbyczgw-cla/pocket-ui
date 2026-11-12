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
    let (back, max) = play_cards::paint_controls(&mut Canvas::new(&mut p, 960, 720), &mut text, "gba", "", &[], false, 0);
    assert_eq!(max, 0, "the card fits one screen with large text: nothing to scroll to");
    assert!(back.y >= 0 && back.y + back.h <= 720 && back.x + back.w <= 960, "Back remains on screen");
    assert!(back.w < 960 / 2, "Back is a button beside the title, not a bar across the screen");
    type_scale::set_system_scale(1.0);
    type_scale::set(TextSize::Normal);
    assert_eq!(play_cards::text_size(TextSize::Large, 1.0), TextSize::Large);
}
#[cfg(test)] // fixtures: cores' raw names and game names, never painted (i18n-extract skips it)
#[test]
fn the_android_card_names_console_buttons_by_place_and_never_shows_raw_core_strings() {
    pocket_ui::i18n::set(pocket_ui::i18n::Lang::En);
    // What mGBA and friends report: actions and extras no box prints.
    let names = vec![
        Target { id: 0, label: "Jump".into() },
        Target { id: 8, label: "Run".into() },
        Target { id: 9, label: "Turbo Run".into() },
        Target { id: 14, label: "Darken Solar Sensor".into() },
    ];
    let lines = play_cards::control_lines("gba", "Garden", &names, false);
    let data = tempfile::tempdir().unwrap();
    play_cards::remember_names(data.path(), "game-with-core", &names).unwrap();
    assert_eq!(
        play_cards::control_lines("gba", "Garden", &play_cards::remembered_names(data.path(), "game-with-core"), false),
        lines,
        "the launcher's card keeps the actual core's names after the player exits"
    );
    assert!(play_cards::remembered_names(data.path(), "another-game").is_empty(), "another game's action names are never guessed");
    assert_eq!(lines[0], "Game Boy Advance");
    for want in ["Right button: A", "Bottom button: B", "L1: L", "R1: R", "Select: Select", "Start: Start", "Back: Pause menu"] {
        assert!(lines.iter().any(|s| s == want), "{want}: {lines:?}");
    }
    for raw in ["Jump", "Run", "Turbo", "Solar", "Android gamepad", "D-pad: D-pad"] {
        assert!(!lines.iter().any(|s| s.contains(raw)), "{raw} on the card: {lines:?}");
    }
    // Boktai needs its sun: the sensor shows for it, on the buttons that press it.
    let boktai = play_cards::control_lines("gba", "Boktai - The Sun Is in Your Hand (USA)", &names, false);
    assert!(boktai.iter().any(|s| s == "L3: Less sunlight") && boktai.iter().any(|s| s == "R3: More sunlight"), "{boktai:?}");
    let n64 = play_cards::control_lines("n64", "", &[], false);
    assert!(n64.iter().any(|s| s == "Right stick: C buttons") && n64.iter().any(|s| s.contains("Hold R2")), "{n64:?}");
    // Touch only: the on-screen buttons carry the names; Pause is the menu.
    let touch = play_cards::control_lines("snes", "", &[], true);
    assert!(touch.iter().any(|s| s == "Pause: Pause menu") && touch.iter().any(|s| s.contains("on the screen")), "{touch:?}");
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
        size: 32768, document_size: 32768,
        missing: vec![],
        covers: vec![], pictures: vec![], cheats: String::new(), parts: vec![],
    };
    // The core reported plain names in other places than the table's (a core option): the card follows the core.
    play_cards::remember_names(&data_dir(), &ui::game_key(&game, false), &[Target { id: 0, label: "A".into() }, Target { id: 8, label: "B".into() }]).unwrap();
    u.install(ui::library(vec![game]));
    u.press(Button::North);
    assert!(u.screen == ui::Screen::Box);
    tap(&mut u, "Controls");
    paint(&mut u, 960, 1600);
    assert!(
        u.regions.iter().any(|r| r.label.contains("Bottom button: A")),
        "game-card Controls keeps the running core's names after returning to the launcher"
    );
    u.back();
    assert!(u.screen == ui::Screen::Box);
    assert_eq!(u.game().unwrap().name, "Garden");
    u.press(Button::R2);
    paint(&mut u, 1600, 960);
    assert!(u.regions.iter().any(|r| r.label.contains("Bottom button: A")));
    u.back();
    assert!(u.screen == ui::Screen::Box);
}

/// The game card's hint says "R2 Controls". On an Odin 2 Mini R2 did nothing: the launcher turned Android's keys into
/// buttons with its own short list, which had no L2/R2/L3/R3 (the test above presses `Button::R2` itself and never saw it).
#[test]
fn android_r2_on_the_game_card_opens_its_controls_as_the_hint_says() {
    use pocket_ui::{document_library::Game, pad::android};
    let mut u = page();
    let game = Game {
        name: "Garden".into(), key: "test:garden-r2".into(), uri: "content://test/garden".into(), relative: "gb/Garden.gb".into(),
        system: "Game Boy", system_id: "gb", profile: "retroarch:mgba", color: 0, size: 32768, document_size: 32768, missing: vec![],
        covers: vec![], pictures: vec![], cheats: String::new(), parts: vec![],
    };
    u.install(ui::library(vec![game]));
    const BUTTON_R2: i32 = 105;
    u.press(android::button(99).expect("Android X"));
    assert!(u.screen == ui::Screen::Box, "X turns the game over");
    u.press(android::button(BUTTON_R2).expect("Android R2 reaches the launcher"));
    paint(&mut u, 1600, 960);
    assert!(u.regions.iter().any(|r| r.label.contains("Pause menu")), "R2 opened the Controls card");
    u.press(android::button(97).expect("Android B"));
    assert!(u.screen == ui::Screen::Box, "B closes the card, the game card stays");
}

/// The whole class: every button a launcher or player hint can name comes from some key of an Android pad, and every
/// libretro joypad button (L3/R3 included) has its Android key.
#[test]
fn every_pad_button_a_hint_names_arrives_from_an_android_key() {
    use pocket_ui::pad::{android, Button};
    let reached: std::collections::HashSet<Button> = (0..300).filter_map(android::button).collect();
    for b in [Button::Up, Button::Down, Button::Left, Button::Right, Button::South, Button::East, Button::North, Button::West,
              Button::L1, Button::R1, Button::L2, Button::R2, Button::L3, Button::R3, Button::Select, Button::Start] {
        assert!(reached.contains(&b), "{b:?} has no Android key");
    }
    let bits: std::collections::HashSet<u16> = (0..300).filter_map(android::retro_bit).collect();
    assert_eq!(bits, (0..16).collect(), "every libretro joypad button");
}

/// A player's first minutes without games: no folder yet, a folder without games (the wrong one), a search with no hit
/// (also while the keyboard is up). Each page says what is going on and, where it helps, offers the folder again.
#[test]
fn android_pages_without_games_always_say_what_is_going_on() {
    use pocket_ui::document_library::Game;
    let labels = |u: &mut ui::Ui, w, h| { paint(u, w, h); u.regions.iter().map(|r| r.label.clone()).collect::<Vec<_>>() };
    for (w, h) in [(960, 1600), (1600, 960), (1056, 1056)] {
        let mut u = page();
        let none = labels(&mut u, w, h);
        assert!(none.iter().any(|l| l == "No games yet") && none.iter().any(|l| l == "Choose your ROM folder"), "{w}x{h} no folder: {none:?}");
        u.install(ui::library(vec![]));
        let empty = labels(&mut u, w, h);
        assert!(empty.iter().any(|l| l == "No games found") && empty.iter().any(|l| l == "Choose your ROM folder"), "{w}x{h} wrong folder: {empty:?}");
        assert!(empty.join(" ").contains("Choose the folder that contains your system folders"), "an empty scan must explain which folder to choose: {empty:?}");
        assert!(u.cut.is_empty(), "the folder advice stays readable: {:?}", u.cut);
        let game = Game {
            name: "Garden".into(), key: "test:garden-empty".into(), uri: "content://test/garden".into(), relative: "gb/Garden.gb".into(),
            system: "Game Boy", system_id: "gb", profile: "retroarch:mgba", color: 0, size: 32768, document_size: 32768, missing: vec![],
            covers: vec![], pictures: vec![], cheats: String::new(), parts: vec![],
        };
        u.install(ui::library(vec![game]));
        u.navigate(ui::Screen::Search);
        u.query = "zzzz".into();
        u.filter();
        // Search may open its keyboard by itself (portrait): the hint stands above it, else as the page.
        let typing = labels(&mut u, w, h);
        assert!(typing.iter().any(|l| l.starts_with("No games found")), "{w}x{h} no hit (keyboard {}): {typing:?}", u.keyboard.is_some());
        u.keyboard = None;
        let typed = labels(&mut u, w, h);
        assert!(typed.iter().any(|l| l == "No games found"), "{w}x{h} no hit, keyboard closed: {typed:?}");
    }
}

#[test]
fn android_retro_style_and_its_palette_are_reachable_and_remembered() {
    let mut u = page();
    u.navigate(ui::Screen::Settings);
    let mut seen = std::collections::HashSet::new();
    while u.prefs.font != pocket_ui::font_style::FontStyle::Retro {
        assert!(seen.insert(u.prefs.font), "the Style choices must reach Retro before repeating");
        tap(&mut u, "Style");
    }
    assert_eq!(u.prefs.font, pocket_ui::font_style::FontStyle::Retro);
    paint(&mut u, 960, 1600);
    tap(&mut u, "Colours");
    let saved = serde_json::to_string(&u.prefs).unwrap();
    let mut u = ui::Ui::new(serde_json::from_str(&saved).unwrap());
    u.navigate(ui::Screen::Settings);
    paint(&mut u, 960, 1600);
    assert_eq!(pocket_ui::retro::colours(), pocket_ui::retro::Colours::Pea);
    assert!(u.regions.iter().any(|r| r.label == "Colours · Pea"));
}

#[test]
fn android_other_card_folders_classify_ambiguous_discs_and_archives() {
    use pocket_ui::document_library::{Document, scan};
    let docs = [
        ("Game Boy Advance (GBA)/Game.zip", "gba"),
        ("Nintendo Game Boy Advance/Game.zip", "gba"),
        ("Sega Genesis/Game.bin", "megadrive"),
        ("Sony PlayStation/Game.chd", "psx"),
    ];
    for (relative, system) in docs {
        let games = scan(vec![Document { root_name: String::new(),uri: "content://qa/game".into(),id: relative.into(),authority: "qa".into(),relative: relative.into(),sheet: String::new(),size: 100}]);
        assert_eq!(games.len(), 1, "{relative} must reach the shelf");
        assert_eq!(games[0].system_id, system, "{relative}");
    }
}

#[test]
fn android_card_art_prefers_boxes_and_never_crosses_saf_grants_or_game_regions() {
    use pocket_ui::document_library::{Document, scan};
    let doc = |root: &str, path: &str, sheet: &str| Document { root_name: String::new(),uri:format!("content://qa/tree/{root}/document/{path}"),authority:"qa".into(),id:format!("{root}/{path}"),relative:path.into(),sheet:sheet.into(),size:100};
    let games = scan(vec![
        doc("a","Game Boy Advance (GBA)/Game (USA).zip",""),
        doc("a","Game Boy Advance (GBA)/images/Game (USA)-image.png",""),
        doc("a","Game Boy Advance (GBA)/Imgs/Game (USA).jpg",""),
        doc("a","Game Boy Advance (GBA)/Game (Europe).cht","cheat0_desc = \"Wrong region\"\ncheat0_code = \"123\"\n"),
        doc("b","Game Boy Advance (GBA)/covers/Game (USA).png",""),
        doc("a","Game Boy Advance (GBA)/images/Game (USA)-marquee.png",""),
    ]);
    assert_eq!(games.len(),1);
    assert_eq!(games[0].covers.len(),2);
    assert!(games[0].covers[0].contains("/Imgs/"),"the box wins over the screenshot");
    assert_eq!(games[0].pictures.len(),1);
    assert!(games[0].cheats.is_empty(),"a different region's cheats never apply");
    let games=scan(vec![doc("a","gb/Game.gb",""),doc("a","gb/Game.cht","cheat0_desc = \"Lives\"\ncheat0_code = \"010301C0\"\n"),doc("a","gb/gamelist.xml","<gameList><game><path>./Game.gb</path><image>./unknown.png</image><cover>./covers/Game.png</cover></game></gameList>"),doc("a","gb/covers/Game.png","")]);
    assert!(games[0].cheats.contains("Lives"));
    assert_eq!(games[0].covers.len(),1,"XML and folder references to the same image are deduplicated");
}

#[test]
fn android_cheat_switches_are_per_identity_and_failed_writes_leave_the_core_unchanged() {
    use pocket_ui::cheats::CheatMenu;
    let dir=tempfile::tempdir().unwrap();
    let file=dir.path().join("game-core.json");
    let text="cheat0_desc = \"Lives\"\ncheat0_code = \"010301C0\"\ncheat1_desc = \"Unsupported\"\ncheat1_address = \"12\"\ncheat1_value = \"3\"\ncheat1_cheat_type = \"1\"\n";
    let mut menu=CheatMenu::from_text(file.clone(),text).unwrap();
    assert!(menu.codes().is_empty(),"cheats start off");
    menu.try_press(0).unwrap();
    assert_eq!(menu.codes(),["010301C0"]);
    assert_eq!(menu.unsupported(),1);
    let mut menu=CheatMenu::from_text(file.clone(),text).unwrap();
    assert_eq!(menu.codes(),["010301C0"],"relaunch remembers the choice");
    assert!(CheatMenu::from_text(dir.path().join("other-core.json"),text).unwrap().codes().is_empty());
    // Replace the containing folder with a regular file; this fails even when tests run as root.
    std::fs::remove_file(file).unwrap();
    let blocked=dir.path().join("blocked");std::fs::write(&blocked,b"not a directory").unwrap();
    let mut failed=CheatMenu::from_text(blocked.join("game.json"),text).unwrap();
    assert!(failed.try_press(0).is_err());
    assert!(failed.codes().is_empty());
    menu.try_press(1).unwrap();assert!(menu.codes().is_empty(),"All off resets the codes");
}

#[path = "../android/src/moments.rs"]
mod moments;

#[test]
fn android_long_selected_names_are_actually_painted_in_every_look() {
    use pocket_ui::{document_library::{scan,Document}, settings::Theme, font_style::FontStyle};
    if std::env::var_os("ANDROID03_TITLE_CHILD").is_none() {
        let dir=tempfile::tempdir().unwrap();
        let result=std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact","android_02_tests::android_long_selected_names_are_actually_painted_in_every_look","--nocapture"])
            .env("ANDROID03_TITLE_CHILD","1").env("POCKETUI_TITLE_BLOCKS",dir.path().join("titles"))
            .env("POCKETUI_TEXT_BOXES",dir.path().join("ink")).output().unwrap();
        assert!(result.status.success() && String::from_utf8_lossy(&result.stdout).contains("1 passed; 0 failed"),"{}\n{}",String::from_utf8_lossy(&result.stdout),String::from_utf8_lossy(&result.stderr));
        return;
    }
    let title="The Legend of Zelda - Ocarina of Time";
    for theme in Theme::PHONE { for (w,h) in [(720,1280),(1280,720)] { for scale in [1.0,2.0] {
        let mut u=page();u.prefs.theme=theme;u.prefs.font=FontStyle::Retro;u.dp=1.5;u.system_font_scale=scale;
        u.install(ui::library(scan(vec![Document { root_name: String::new(),uri:"content://qa/long".into(),authority:"qa".into(),id:"long".into(),relative:format!("n64/{title}.z64"),sheet:String::new(),size:100}])));
        let mut px=vec![0;w*h];let mut text=Text::builtin().following_font();
        pocket_ui::text::boxes::reset();
        u.paint(&mut Canvas::new(&mut px,w as u32,h as u32),&mut text,None);
        pocket_ui::text::boxes::finish(&px,w as i32,h as i32);
        let record=std::fs::read_to_string(std::env::var_os("POCKETUI_TITLE_BLOCKS").unwrap()).unwrap();
        let lines=record.lines().filter_map(|l| l.splitn(4,'\t').nth(3)).collect::<Vec<_>>().join(" ");
        assert_eq!(lines.replace(" - "," "),title.replace(" - "," "),"{theme:?} {w}x{h} scale={scale}: the title must be painted whole");
        if w>h && theme!=Theme::Big {
            if let Some(just)=u.regions.iter().find(|r| r.label=="Just Play") {
                let name=u.regions.iter().rev().find(|r|r.label==title && matches!(r.action,ui::Action::Press(pocket_ui::pad::Button::North))).unwrap();
                assert!(name.area.y>=just.area.y+just.area.h,"the selected name must leave Just Play readable: {theme:?}, scale={scale}");
            }
        }
    }}}
}

#[test]
fn android_phone_rows_and_title_bands_keep_names_literal_in_de_and_es() {
    use pocket_ui::{document_library::{scan, Document}, font_style::FontStyle, i18n::{self, Lang}, settings::Theme};
    if std::env::var_os("ANDROID03_LITERAL_CHILD").is_none() {
        let ink = tempfile::tempdir().unwrap();
        let result = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "android_02_tests::android_phone_rows_and_title_bands_keep_names_literal_in_de_and_es", "--nocapture"])
            .env("ANDROID03_LITERAL_CHILD", "1").env("POCKETUI_TEXT_BOXES", ink.path().join("ink")).output().unwrap();
        assert!(result.status.success() && String::from_utf8_lossy(&result.stdout).contains("1 passed; 0 failed"), "{}\n{}", String::from_utf8_lossy(&result.stdout), String::from_utf8_lossy(&result.stderr));
        return;
    }
    fn pixels(frame: &[u32], w: usize, h: usize, area: (i32, i32, i32, i32)) -> Vec<u32> {
        let (x, y, aw, ah) = area;
        let mut out = Vec::new();
        for row in y.max(0)..(y + ah).min(h as i32) {
            out.extend_from_slice(&frame[row as usize * w + x.max(0) as usize..row as usize * w + (x + aw).min(w as i32) as usize]);
        }
        out
    }
    // Compare actual phone name regions with their untranslated reference; surrounding UI stays in DE/ES.
    // A fresh child avoids names learned by other tests hiding a metadata/fragment translation.
    for lang in [Lang::De, Lang::Es] {
        for title in ["Game Boy Color", "Golden Sun", "Sun", "Settings"] {
            for theme in [Theme::Shelf, Theme::Split] {
                for (w, h) in [(360, 800), (800, 360), (720, 1280), (1280, 720)] {
                    for boxed in [false, true] {
                        if boxed && (theme != Theme::Shelf || w < 720 || h < 720) { continue; }
                        for scale in [1.0, 2.0] {
                            let mut u = page();
                            u.prefs.choose_language(lang);
                            u.prefs.font = FontStyle::Retro;
                            u.prefs.theme = theme;
                            u.dp = 1.5;
                            u.system_font_scale = scale;
                            u.install(ui::library(scan(vec![Document { root_name: String::new(), uri: "content://qa/literal".into(), authority: "qa".into(), id: "literal".into(), relative: format!("gb/{title}.gb"), sheet: String::new(), size: 32768 }])));
                            if boxed { u.press(pocket_ui::pad::Button::North); assert!(u.screen == ui::Screen::Box); }
                            let mut actual = vec![0; w * h];
                            let mut literal = actual.clone();
                            let mut text = Text::builtin().following_font();
                            pocket_ui::text::boxes::reset();
                            u.paint(&mut Canvas::new(&mut actual, w as u32, h as u32), &mut text, None);
                            if boxed {
                                pocket_ui::text::boxes::finish(&actual, w as i32, h as i32);
                                let ink = std::fs::read_to_string(std::env::var_os("POCKETUI_TEXT_BOXES").unwrap()).unwrap();
                                let clashes: Vec<_> = ink.lines().filter(|line| {
                                    let parts: Vec<_> = line.split('\t').collect();
                                    parts.len() == 3 && parts[0] == "O" && title.contains(parts[1]) && title.contains(parts[2])
                                }).collect();
                                assert!(clashes.is_empty(), "Box name lines must remain separately readable: {lang:?} {w}x{h} scale={scale}: {title}: {clashes:?}");
                            }
                            let region = if boxed {
                                u.regions.iter().find(|r| r.label == title && matches!(r.action, ui::Action::Label))
                            } else if theme == Theme::Shelf {
                                u.regions.iter().rev().find(|r| r.label == title && matches!(r.action, ui::Action::Press(pocket_ui::pad::Button::North)))
                            } else {
                                u.regions.iter().find(|r| r.label == title && matches!(r.action, ui::Action::Row(0)))
                            }.unwrap_or_else(|| panic!("The phone must expose the literal game name: {lang:?} {theme:?}: {title}"));
                            let a = region.area;
                            let area = (a.x, a.y, a.w, a.h);
                            let mut reference = Text::builtin().following_font();
                            reference.set_translate(false);
                            reference.literal(|reference| u.paint(&mut Canvas::new(&mut literal, w as u32, h as u32), reference, None));
                            assert!(pixels(&actual, w, h, area) == pixels(&literal, w, h, area), "translated phone name: {lang:?} {theme:?} {w}x{h} scale={scale}: {title}");
                            assert!(text.translating() && !reference.translating(), "Name painting must restore its caller's language mode");
                        }
                    }
                }
            }
        }
        assert_ne!(&*i18n::Cache::default().get("Settings"), "Settings", "The interface itself must still translate");
    }
}

#[test]
fn android_pause_header_shows_the_whole_name_and_status_and_leaves_the_rows_their_room() {
    use pocket_ui::{font_style::FontStyle, i18n::{self, Lang}, pause_head, text::Face, title_block};
    if std::env::var_os("ANDROID03_HEAD_CHILD").is_none() {
        let result = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "android_02_tests::android_pause_header_shows_the_whole_name_and_status_and_leaves_the_rows_their_room", "--nocapture"])
            .env("ANDROID03_HEAD_CHILD", "1").output().unwrap();
        assert!(result.status.success() && String::from_utf8_lossy(&result.stdout).contains("1 passed; 0 failed"), "{}\n{}", String::from_utf8_lossy(&result.stdout), String::from_utf8_lossy(&result.stderr));
        return;
    }
    let names = ["Sun", "Settings", "The Legend of Zelda - Ocarina of Time", "Pokemon Mystery Dungeon: Explorers of Sky - Special Edition Deluxe"];
    let mut fails: Vec<String> = Vec::new();
    let statuses = ["", "Saved to slot 2", "Could not save: the save folder is read-only. Choose another folder in Games."];
    for lang in [Lang::En, Lang::De, Lang::Es] {
        i18n::set(lang);
        i18n::learn_names(names);
        for retro in [false, true] {
            // Phone logical pixels: about 2.2 per dp; 360x800 dp portrait and landscape.
            for (w, h) in [(792, 1760), (1760, 792)] {
                for scale in [1.0, 2.0] {
                    pocket_ui::type_scale::set_system_scale(scale);
                    pocket_ui::type_scale::set(play_cards::text_size(TextSize::Normal, scale));
                    for name in names {
                        for status in statuses {
                            let mut text = Text::builtin().pinned_to(if retro { FontStyle::Retro } else { FontStyle::Barlow });
                            let (width, height) = (w - 2 * pause_head::EDGE, h - 36);
                            let need = pause_head::rows_need(w < h, false);
                            let head = pause_head::layout(&mut text, name, status, width, height, need);
                            let what = format!("{lang:?} retro={retro} {w}x{h} scale={scale} {name:?} / {status:?}");
                            let lines: Vec<_> = head.name.iter().map(|(l, _)| l.clone()).collect();
                            if !title_block::is_whole(&lines, name) { fails.push(format!("name not whole: {what}: {lines:?}")); }
                            if !lines.iter().all(|l| l.literal) { fails.push(format!("name translated: {what}")); }
                            if lines.iter().any(|l| l.text.ends_with('…')) { fails.push(format!("name cut: {what}")); }
                            for (l, _) in &head.name {
                                if text.literal(|t| t.width(Face::Title, l.px, &l.text)) > width { fails.push(format!("name line wide: {what}")); }
                            }
                            let shown: Vec<&str> = head.status.iter().map(|(s, _)| s.as_str()).collect();
                            if !status.is_empty() {
                                if shown.is_empty() || shown.iter().any(|s| s.ends_with('…')) { fails.push(format!("status not whole: {what}: {shown:?}")); }
                            }
                            for (s, _) in &head.status {
                                if text.width(Face::Body, head.body, s) > width { fails.push(format!("status line wide: {what}")); }
                            }
                            // Lines stand apart, and the rows keep at least their own height.
                            let ys: Vec<i32> = head.name.iter().map(|p| p.1).chain(head.status.iter().map(|p| p.1)).collect();
                            if !ys.windows(2).all(|p| p[1] > p[0]) { fails.push(format!("lines overlap: {what}: {ys:?}")); }
                            if !ys.iter().all(|&y| y > 0) { fails.push(format!("line above the sheet: {what}")); }
                            if head.bottom + need > height { fails.push(format!("head takes the rows' room ({} + {need} > {height}): {what}", head.bottom)); }
                        }
                    }
                }
            }
        }
        // The name is painted as it is in DE/ES, the status is the sheet's own text.
        for name in ["Sun", "Settings"] {
            let (w, h) = (792, 400);
            let mut actual = vec![0; w * h];
            let mut reference = actual.clone();
            let mut text = Text::builtin();
            let head = pause_head::layout(&mut text, name, "", w as i32 - 80, h as i32, 0);
            pause_head::paint(&mut Canvas::new(&mut actual, w as u32, h as u32), &mut text, &head, 40, pocket_ui::palette::pal().chalk, pocket_ui::palette::pal().dust);
            let mut literal = Text::builtin();
            literal.set_translate(false);
            let head = pause_head::layout(&mut literal, name, "", w as i32 - 80, h as i32, 0);
            pause_head::paint(&mut Canvas::new(&mut reference, w as u32, h as u32), &mut literal, &head, 40, pocket_ui::palette::pal().chalk, pocket_ui::palette::pal().dust);
            assert!(actual == reference, "the pause header translated {name} in {lang:?}");
            assert!(text.translating(), "the header must restore translation for its actions");
        }
    }
    pocket_ui::type_scale::set_system_scale(1.0);
    pocket_ui::type_scale::set(TextSize::Normal);
    assert!(fails.is_empty(), "{} problems:\n{}", fails.len(), fails.join("\n"));
}

// ---- Settings → Save backup (docs/ANDROID-MVP.md, "Saves out and back"). What goes where is `save_transfer`'s tests;
// these are the page: reached with the D-pad alone, B a step back, every state in words, nothing cut in three languages.

/// Moves the controller's selection down the sheet to the row named `label`.
fn dpad_to(u: &mut ui::Ui, label: &str) {
    u.touch_mode = false;
    for _ in 0..3 {
        u.press(pocket_ui::pad::Button::Up);
    }
    for _ in 0..40 {
        paint(u, 960, 1600);
        if u.regions.iter().any(|r| matches!(r.action, ui::Action::Row(_)) && u.selected_region(r) && (r.label == label || r.label.starts_with(&format!("{label} ·")))) {
            return;
        }
        u.press(pocket_ui::pad::Button::Down);
    }
    panic!("the D-pad never reaches {label}: {:?}", u.regions.iter().map(|r| r.label.clone()).collect::<Vec<_>>());
}
/// D-pad to the row, then A.
fn choose(u: &mut ui::Ui, label: &str) -> ui::Command {
    dpad_to(u, label);
    u.press(pocket_ui::pad::Button::East)
}
fn sheet_rows(u: &mut ui::Ui) -> Vec<String> {
    paint(u, 960, 1600);
    u.regions.iter().filter(|r| matches!(r.action, ui::Action::Row(_))).map(|r| r.label.clone()).collect()
}
/// What the page says in words (not its rows or tabs).
fn page_words(u: &mut ui::Ui) -> String {
    paint(u, 960, 1600);
    u.regions.iter().filter(|r| matches!(r.action, ui::Action::Label)).map(|r| r.label.clone()).collect::<Vec<_>>().join(" | ")
}

#[test]
fn android_save_backup_is_reached_with_the_d_pad_alone_and_b_goes_back_one_step() {
    use pocket_ui::pad::Button;
    let mut u = page();
    u.navigate(ui::Screen::Settings);
    assert!(matches!(choose(&mut u, "Save backup"), ui::Command::None));
    assert!(u.screen == ui::Screen::Saves);
    assert_eq!(sheet_rows(&mut u), ["Export saves · Nothing yet", "Import saves", "Saved copies", "Back"]);
    for row in ["Export saves", "Import saves", "Saved copies", "Back"] {
        dpad_to(&mut u, row);
    }
    // B: one step back, onto the row it came from, not to the top of Settings or out to the games.
    u.press(Button::South);
    assert!(u.screen == ui::Screen::Settings);
    paint(&mut u, 960, 1600);
    let on = u.regions.iter().find(|r| matches!(r.action, ui::Action::Row(_)) && u.selected_region(r)).map(|r| r.label.clone());
    assert_eq!(on.as_deref(), Some("Save backup"));
    // The Back row and Android's own Back do the same.
    u.press(Button::East);
    assert!(u.screen == ui::Screen::Saves);
    assert!(matches!(choose(&mut u, "Back"), ui::Command::None));
    assert!(u.screen == ui::Screen::Settings);
    u.press(Button::East);
    u.back();
    assert!(u.screen == ui::Screen::Settings);
    // A finger gets there too, and the Settings tab stays the lit one.
    tap(&mut u, "Save backup");
    assert!(u.screen == ui::Screen::Saves);
    paint(&mut u, 960, 1600);
    assert!(u.regions.iter().any(|r| matches!(r.action, ui::Action::Tab(ui::Screen::Settings))));
}

#[test]
fn android_save_backup_says_in_words_where_it_stands_at_every_step() {
    use pocket_ui::{pad::Button, save_transfer};
    let mut u = page();
    u.navigate(ui::Screen::Settings);
    choose(&mut u, "Save backup");
    assert!(page_words(&mut u).contains("Android deletes them when the app is removed"), "{}", page_words(&mut u));
    // Nothing was played yet: Export says so on the page, and Android's picker is not opened for an empty file.
    assert!(matches!(choose(&mut u, "Export saves"), ui::Command::None));
    assert!(page_words(&mut u).contains("No saves yet"), "{}", page_words(&mut u));
    // A game was played.
    let saves = data_dir().join("saves/00000000000000aa");
    std::fs::create_dir_all(&saves).unwrap();
    std::fs::write(saves.join("Zelda.srm"), b"hearts").unwrap();
    u.back();
    choose(&mut u, "Save backup");
    assert_eq!(sheet_rows(&mut u)[0], "Export saves · 1 game");
    assert!(matches!(choose(&mut u, "Export saves"), ui::Command::SavesExport));
    assert!(u.saves_busy() && page_words(&mut u).contains("Writing the backup"));
    // While it is on its way a second A starts nothing, and the page can still be left.
    assert!(matches!(choose(&mut u, "Import saves"), ui::Command::None));
    assert!(matches!(choose(&mut u, "Export saves"), ui::Command::None));
    u.saves_said(&save_transfer::exported_sentence(1));
    assert!(page_words(&mut u).contains("Backup written: the saves of 1 game."));
    // Import: a backup whose games partly have saves here asks, in rows, and the answer is the row's.
    assert!(matches!(choose(&mut u, "Import saves"), ui::Command::SavesImport));
    assert!(page_words(&mut u).contains("Reading the backup"));
    u.saves_ask(3, 1);
    assert_eq!(sheet_rows(&mut u), ["Keep this phone's saves", "Use the backup's saves", "Cancel"]);
    let words = page_words(&mut u);
    assert!(words.contains("Games in this backup: 3") && words.contains("already with saves on this phone: 1"), "{words}");
    assert!(matches!(choose(&mut u, "Use the backup's saves"), ui::Command::SavesAnswer(true)));
    u.saves_ask(3, 1);
    assert!(matches!(choose(&mut u, "Keep this phone's saves"), ui::Command::SavesAnswer(false)));
    // B on the question is "never mind": nothing is copied, and the page says so instead of going quiet.
    u.saves_ask(3, 1);
    assert!(matches!(u.press(Button::South), ui::Command::SavesCancel));
    assert!(u.screen == ui::Screen::Saves && page_words(&mut u).contains("Nothing was changed."));
    u.saves_ask(3, 1);
    assert!(matches!(choose(&mut u, "Cancel"), ui::Command::SavesCancel));
    // What came back, and what a wrong file is, stay on the page until it is left.
    u.saves_said("2 games have their saves back.\n1 game kept the saves from this phone.");
    let words = page_words(&mut u);
    assert!(words.contains("2 games have their saves back.") && words.contains("1 game kept the saves from this phone."), "{words}");
    u.saves_said(save_transfer::NOT_A_BACKUP);
    assert!(page_words(&mut u).contains("not a pocket-ui save backup"));
    assert_eq!(sheet_rows(&mut u), ["Export saves · 1 game", "Import saves", "Saved copies", "Back"]);
}

#[test]
fn android_saved_copies_row_says_what_is_wrong_and_leads_to_the_folder_choice() {
    use ui::saves_page::Copies;
    let mut u = page();
    u.navigate(ui::Screen::Settings);
    choose(&mut u, "Save backup");
    // Before the launcher has looked: no claim either way.
    assert_eq!(sheet_rows(&mut u)[2], "Saved copies");
    // No permission to write (every install before this one, or a folder the player only allowed reading): said in
    // words and in the row, and A leads to choosing the folder, where Android asks for the permission.
    u.saves_copies(Copies::Off, pocket_ui::save_copy::NO_ACCESS);
    assert_eq!(sheet_rows(&mut u)[2], "Saved copies · Off");
    let words = page_words(&mut u);
    assert!(words.contains("may not write") && words.contains("allow changes"), "{words}");
    assert!(matches!(choose(&mut u, "Saved copies"), ui::Command::Folder));
    // The card is out or the folder was removed.
    u.saves_copies(Copies::Gone, pocket_ui::save_copy::GONE);
    assert_eq!(sheet_rows(&mut u)[2], "Saved copies · Folder missing");
    assert!(page_words(&mut u).contains("can't be reached"));
    assert!(matches!(choose(&mut u, "Saved copies"), ui::Command::Folder));
    // It works: A copies now and says it is on its way; the answer comes with what was done.
    u.saves_copies(Copies::On, "");
    assert_eq!(sheet_rows(&mut u)[2], "Saved copies · On");
    assert!(page_words(&mut u).contains("pocket-ui saves"), "{}", page_words(&mut u));
    assert!(matches!(choose(&mut u, "Saved copies"), ui::Command::SavesCopyNow));
    assert_eq!(sheet_rows(&mut u)[2], "Saved copies · Copying");
    assert!(page_words(&mut u).contains("Copying your saves."));
    assert!(matches!(choose(&mut u, "Saved copies"), ui::Command::None), "a second A starts nothing while it is on its way");
    u.saves_copies(Copies::On, "Copied the saves of 2 games to your folder.\n1 game's saves differ from the copy in your folder. Nothing was overwritten.");
    let words = page_words(&mut u);
    assert!(words.contains("Copied the saves of 2 games") && words.contains("Nothing was overwritten"), "{words}");
    // B and the Back row still leave the page.
    assert!(matches!(choose(&mut u, "Back"), ui::Command::None));
    assert!(u.screen == ui::Screen::Settings);
}

#[test]
fn android_save_backup_shows_every_word_whole_in_english_german_and_spanish() {
    use pocket_ui::{i18n::Lang, save_transfer};
    use ui::saves_page::{self, Step};
    // The language is one for the whole process: in a child, so no other test reads German words.
    if std::env::var_os("ANDROID_SAVES_WORDS_CHILD").is_none() {
        let result = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "android_02_tests::android_save_backup_shows_every_word_whole_in_english_german_and_spanish", "--nocapture"])
            .env("ANDROID_SAVES_WORDS_CHILD", "1").output().unwrap();
        assert!(result.status.success() && String::from_utf8_lossy(&result.stdout).contains("1 passed; 0 failed"), "{}\n{}", String::from_utf8_lossy(&result.stdout), String::from_utf8_lossy(&result.stderr));
        return;
    }
    let said = [
        save_transfer::NOT_A_BACKUP.to_string(), save_transfer::NEWER_BACKUP.into(), save_transfer::EMPTY_BACKUP.into(), save_transfer::NOTHING_YET.into(),
        saves_page::STOPPED.into(), saves_page::FAILED_WRITE.into(), saves_page::FAILED_READ.into(), saves_page::NO_ROOM.into(),
        save_transfer::exported_sentence(1), save_transfer::exported_sentence(128),
        "1 game has its saves back.\n1 game kept the saves from this phone.".into(), "128 games have their saves back.\n37 games kept the saves from this phone.".into(),
    ];
    let mut steps = vec![Step::Rest, Step::Busy(true), Step::Busy(false), Step::Ask { games: 128, here: 37 }];
    steps.extend(said.into_iter().map(Step::Said));
    // The copies' words, in every state, with what a run can add to them.
    let copy_states: Vec<(saves_page::Copies, String)> = vec![
        (saves_page::Copies::On, String::new()),
        (saves_page::Copies::On, "128 games have their saves back from your folder.\nCopied the saves of 37 games to your folder.".into()),
        (saves_page::Copies::On, "37 games' saves differ from the copies in your folder. Nothing was overwritten.".into()),
        (saves_page::Copies::Off, pocket_ui::save_copy::NO_ACCESS.into()),
        (saves_page::Copies::Gone, pocket_ui::save_copy::GONE.into()),
        (saves_page::Copies::On, pocket_ui::save_copy::FAILED.into()),
    ];
    // A 360 dp wide phone upright and on its side, and a handheld's 16:9 screen; Android's font size at 100 % and 130 %.
    for lang in [Lang::En, Lang::De, Lang::Es] {
        for (w, h, dp) in [(720, 1600, 2.0), (1600, 720, 2.0), (1920, 1080, 2.3)] {
            for scale in [1.0, 1.3] {
                let mut u = page();
                u.prefs.choose_language(lang);
                u.dp = dp;
                u.system_font_scale = scale;
                u.touch_mode = false;
                u.navigate(ui::Screen::Settings);
                choose(&mut u, &ui::tr("Save backup"));
                assert!(u.screen == ui::Screen::Saves);
                u.saves.games = 128;
                for (copies, words) in &copy_states {
                    u.saves.step = Step::Rest;
                    u.saves_copies(*copies, words);
                    let mut px = vec![0; w * h];
                    u.paint(&mut Canvas::new(&mut px, w as u32, h as u32), &mut Text::builtin(), None);
                    assert!(u.cut.is_empty(), "{lang:?} {w}x{h} font x{scale}: copies {copies:?} are shown cut: {:?}", u.cut);
                    // The row is reached by the D-pad (rows scroll when a big type leaves little room) and is whole.
                    u.touch_mode = false;
                    for _ in 0..4 {
                        u.press(pocket_ui::pad::Button::Up);
                    }
                    for _ in 0..2 {
                        u.press(pocket_ui::pad::Button::Down);
                    }
                    let mut px = vec![0; w * h];
                    u.paint(&mut Canvas::new(&mut px, w as u32, h as u32), &mut Text::builtin(), None);
                    assert!(
                        u.regions.iter().any(|r| matches!(r.action, ui::Action::Row(_)) && u.selected_region(r) && r.label.starts_with(&ui::tr("Saved copies"))),
                        "{lang:?} {w}x{h} font x{scale}: the Saved copies row is not where the D-pad lands: {:?}",
                        u.regions.iter().map(|r| r.label.clone()).collect::<Vec<_>>()
                    );
                    assert!(u.cut.is_empty(), "{lang:?} {w}x{h} font x{scale}: the row is shown cut: {:?}", u.cut);
                }
                u.saves_copies(saves_page::Copies::Unknown, "");
                for step in &steps {
                    u.saves.step = step.clone();
                    let mut px = vec![0; w * h];
                    u.paint(&mut Canvas::new(&mut px, w as u32, h as u32), &mut Text::builtin(), None);
                    assert!(u.cut.is_empty(), "{lang:?} {w}x{h} font x{scale}: {step:?} is shown cut: {:?}", u.cut);
                    let words = u.regions.iter().filter(|r| matches!(r.action, ui::Action::Label)).map(|r| r.label.clone()).collect::<Vec<_>>().join(" ");
                    if lang != Lang::En {
                        // The folder's own name stays as it is on the card.
                        let words = words.replace("pocket-ui saves", "");
                        for english in ["saves", "backup", "changed"] {
                            assert!(!words.contains(english), "{lang:?}: {step:?} is said in English: {words}");
                        }
                    }
                }
                // The row in Settings that leads here.
                u.navigate(ui::Screen::Settings);
                dpad_to(&mut u, &ui::tr("Save backup"));
                let mut px = vec![0; w * h];
                u.paint(&mut Canvas::new(&mut px, w as u32, h as u32), &mut Text::builtin(), None);
                assert!(!u.cut.contains(&ui::tr("Save backup")), "{lang:?} {w}x{h} font x{scale}: the Settings row is cut: {:?}", u.cut);
            }
        }
    }
}

#[test]
fn unverified_android_systems_are_labelled_before_play_and_continue() {
    use pocket_ui::{document_library::{scan, Document}, settings::Theme};
    for (system, extension, experimental) in [("n64", "z64", true), ("psp", "iso", true), ("nds", "nds", false), ("saturn", "chd", true), ("dreamcast", "chd", true), ("gba", "gba", false)] {
        for theme in Theme::PHONE {
            for (w, h) in [(960, 540), (540, 960)] {
                for resume in [false, true] {
                    let mut u = page();
                    u.prefs.theme = theme;
                    u.install(ui::library(scan(vec![Document { root_name: String::new(),
                        uri: "content://qa/game".into(), authority: "qa".into(), id: "game".into(),
                        relative: format!("{system}/Test.{extension}"), sheet: String::new(), size: 8_388_608,
                    }])));
                    let key = u.game().unwrap().key.clone();
                    if resume { u.frozen_got(vec![key]); }
                    paint(&mut u, w, h);
                    let words = u.regions.iter().map(|r| r.label.as_str()).collect::<Vec<_>>().join(" ");
                    assert_eq!(words.contains("Experimental"), experimental, "{system} {theme:?} {w}x{h} resume={resume}: {words}");
                    u.press(pocket_ui::pad::Button::North);
                    paint(&mut u, w, h);
                    let play = u.regions.iter().find(|r| r.label.starts_with("Play")).unwrap();
                    assert_eq!(play.label.contains("Experimental"), experimental, "game card: {system} {theme:?} {w}x{h}");
                    assert_eq!(play_cards::control_lines(system, "", &[], false).iter().any(|s| s.starts_with("Experimental:")), experimental);
                }
            }
        }
    }
}

/// Android started with six looks and keeps four (Shelf, Easy, Split, Cabinet). A launcher.json from a build before
/// that still opens in a look like the one it had (Minimal and Cards were flat lists and fronts: Split), and nothing
/// else in it is lost, also for a look no build knows.
#[test]
fn a_retired_phone_look_opens_as_split_and_keeps_the_rest() {
    use pocket_ui::settings::Theme;
    assert_eq!(Theme::PHONE, [Theme::Shelf, Theme::Big, Theme::Split, Theme::Cabinet]);
    for (old, want) in [("minimal", Theme::Split), ("cards", Theme::Split), ("cabinet", Theme::Cabinet), ("carousel", Theme::Shelf)] {
        let p: ui::Preferences = serde_json::from_str(&format!(r#"{{"theme":"{old}","light":true,"onboarding_done":true}}"#)).unwrap();
        assert_eq!((p.theme, p.light, p.onboarding_done), (want, true, true), "{old}");
    }
    let saved = serde_json::to_string(&serde_json::from_str::<ui::Preferences>(r#"{"theme":"cards"}"#).unwrap()).unwrap();
    assert!(saved.contains(r#""theme":"split""#), "{saved}");
}

/// Android's tab bar has four tabs: Games, Favourites, Search, Settings. Curator comes back as a fifth with
/// Settings → Curator tab, reached with the D-pad alone; L1/R1 walk the tabs that are there, never a hidden one.
#[test]
fn curator_is_a_tab_only_after_settings_switches_it_on() {
    use pocket_ui::pad::Button;
    let tabs = |u: &mut ui::Ui| {
        paint(u, 960, 1600);
        u.regions.iter().filter_map(|r| if let ui::Action::Tab(s) = r.action { Some(s) } else { None }).collect::<Vec<_>>()
    };
    // L1 from Games: the last tab shown. (Search opens its keyboard, which has L1/R1 for itself.)
    let left_of_games = |u: &mut ui::Ui| {
        u.navigate(ui::Screen::Library);
        u.press(Button::L1);
        u.screen
    };
    let mut u = page();
    assert!(tabs(&mut u) == [ui::Screen::Library, ui::Screen::Favorites, ui::Screen::Search, ui::Screen::Settings]);
    assert!(left_of_games(&mut u) == ui::Screen::Search, "L1/R1 never land on a tab that isn't there");
    u.navigate(ui::Screen::Settings);
    assert!(matches!(choose(&mut u, "Curator tab"), ui::Command::None | ui::Command::Persist));
    assert!(u.prefs.curator_tab);
    assert!(tabs(&mut u).contains(&ui::Screen::Curator));
    assert!(left_of_games(&mut u) == ui::Screen::Curator);
    u.press(Button::R1);
    assert!(u.screen == ui::Screen::Library, "R1 from Curator wraps to Games");
    // Kept with the rest of the preferences, and off for a launcher.json from before the switch.
    let saved = serde_json::to_string(&u.prefs).unwrap();
    assert!(serde_json::from_str::<ui::Preferences>(&saved).unwrap().curator_tab);
    assert!(!serde_json::from_str::<ui::Preferences>(r#"{"theme":"shelf"}"#).unwrap().curator_tab);
}

#[test]
fn android_more_systems_needs_selection_and_consent_and_remembers_it() {
    use pocket_ui::pad::Button;
    let mut u = page();
    u.core_channel_open = true;
    u.prefs.onboarding_done = false;
    u.prefs.onboarding_step = 5;
    let offer: pocket_ui::android_cores::CorePackage = serde_json::from_value(serde_json::json!({
        "id":"android-core-ppsspp","name":"PSP","category":"android-cores","version":"1","description":"","size":25000000,
        "url":"psp.tar.gz","sha256":"a".repeat(64),"kind":"tar","dest":"/ignored","core":"ppsspp","systems":["psp"],
        "library_sha256":"b".repeat(64),"corresponding_source":"source.tar.gz","source_sha256":"c".repeat(64)
    })).unwrap();
    u.core_offers = vec![offer];
    paint(&mut u, 1600, 960);
    assert!(u.regions.iter().any(|r| r.label == "More systems?"));
    assert!(u.regions.iter().any(|r| r.label == "PSP · 25.0 MB"));
    assert!(u.prefs.selected_cores.is_empty());
    assert!(!u.prefs.core_download_consent);
    u.press(Button::East); // Select PSP; this alone starts no transfer.
    assert!(u.prefs.selected_cores.contains("ppsspp"));
    assert!(!u.prefs.core_download_consent);
    let saved = serde_json::to_string(&u.prefs).unwrap();
    let mut restored = ui::Ui::new(serde_json::from_str(&saved).unwrap());
    restored.core_offers = u.core_offers.clone();
    restored.core_channel_open = true;
    restored.press(Button::Down);
    restored.press(Button::East); // Continue gives consent and advances to controls.
    assert_eq!(restored.prefs.onboarding_step, 6);
    assert!(restored.prefs.core_download_consent);
    restored.prefs.onboarding_done = true;
    restored.prefs.selected_cores.clear();
    restored.missing_core("ppsspp");
    assert!(matches!(choose(&mut restored,"PSP"),ui::Command::CoreDownload(core) if core=="ppsspp"));
    restored.core_busy = Some("ppsspp".into());
    assert!(matches!(restored.press(Button::South),ui::Command::CoreCancel));
}

#[test]
fn android_system_download_choices_keep_sizes_and_navigation_reachable() {
    if std::env::var_os("ANDROID_CORE_LAYOUT_CHILD").is_none() {
        let ink = tempfile::tempdir().unwrap();
        let result = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "android_02_tests::android_system_download_choices_keep_sizes_and_navigation_reachable", "--nocapture"])
            .env("ANDROID_CORE_LAYOUT_CHILD", "1").env("POCKETUI_TEXT_BOXES", ink.path().join("ink"))
            .env("POCKETUI_VISIBLE_TEXT", ink.path().join("visible")).output().unwrap();
        assert!(result.status.success(), "{}\n{}", String::from_utf8_lossy(&result.stdout), String::from_utf8_lossy(&result.stderr));
        return;
    }
    let choices = [("ppsspp", "PSP", 18_600_000), ("melondsds", "DS", 1_900_000),
        ("mupen64plus_next", "N64", 1_620_000), ("flycast", "Dreamcast", 4_800_000)];
    for lang in [pocket_ui::i18n::Lang::En, pocket_ui::i18n::Lang::De] {
        for light in [false, true] { for (w,h) in [(720,1600), (1600,720)] { for scale in [1.0,2.0] {
            let mut u = page();
            u.core_channel_open = true;
            u.prefs.choose_language(lang); u.prefs.light = light; u.prefs.onboarding_done = false; u.prefs.onboarding_step = 5;
            u.dp = 2.0; u.system_font_scale = scale;
            u.core_offers = choices.iter().map(|(core,name,size)| serde_json::from_value(serde_json::json!({
                "id":format!("android-core-{core}"),"name":name,"category":"android-cores","version":"1","description":"","size":size,
                "url":"core.tar.gz","sha256":"a".repeat(64),"kind":"tar","dest":"/ignored","core":core,"systems":[core],
                "library_sha256":"b".repeat(64),"corresponding_source":"source.tar.gz","source_sha256":"c".repeat(64)
            })).unwrap()).collect();
            pocket_ui::text::boxes::reset();
            let frame = paint(&mut u,w,h);
            pocket_ui::text::boxes::finish(&frame,w as i32,h as i32);
            let ink = std::fs::read_to_string(std::env::var_os("POCKETUI_TEXT_BOXES").unwrap()).unwrap();
            let context = format!("{lang:?} light={light} {w}x{h} scale={scale}");
            assert!(u.cut.is_empty(), "system names must stay whole: {context}: {:?}", u.cut);
            assert!(ink.trim().is_empty(), "download names, sizes and instructions overlap or clip: {context}: {ink}");
            if let Some(dir) = std::env::var_os("ANDROID_CORE_SHOTS") {
                let dir = std::path::PathBuf::from(dir); std::fs::create_dir_all(&dir).unwrap();
                let rgb: Vec<u8> = frame.iter().flat_map(|p| [(p>>16) as u8,(p>>8) as u8,*p as u8]).collect();
                std::fs::write(dir.join(format!("{lang:?}-{light}-{w}x{h}-{scale}.png")), pocket_ui::screenshot::encode_rgb(w,h,&rgb).unwrap()).unwrap();
            }
            let nav: Vec<_> = u.regions.iter().filter(|r| ["Continue","Back","Skip"].iter().any(|s| r.label == ui::tr(s))).map(|r| r.area).collect();
            for (_,name,size) in choices {
                let label = format!("{name} · {}",ui::core_size(size));
                let area = u.regions.iter().find(|r| r.label == label).unwrap_or_else(||panic!("{label} missing: {context}")).area;
                assert!(area.x >= 0 && area.y >= 0 && area.x+area.w <= w as i32 && area.y+area.h <= h as i32, "{label} outside screen: {context}");
                assert!(area.h >= 96, "{label} needs a 48 dp touch target: {context}");
                assert!(nav.iter().all(|b| area.x+area.w<=b.x || b.x+b.w<=area.x || area.y+area.h<=b.y || b.y+b.h<=area.y), "{label} hides navigation: {context}");
                u.tap((area.x+area.w/2) as f32,(area.y+area.h/2) as f32);
            }
            assert_eq!(u.prefs.selected_cores.len(),4,"every download can be selected: {context}");
            assert!(!u.prefs.core_download_consent,"selection alone downloads nothing");
            u.prefs.onboarding_done = true;
            u.missing_core("melondsds");
            u.core_note = ui::tr("The store list's signature doesn't match, so it wasn't used. Nothing was changed.");
            pocket_ui::text::boxes::reset();
            let frame = paint(&mut u,w,h);
            pocket_ui::text::boxes::finish(&frame,w as i32,h as i32);
            let ink = std::fs::read_to_string(std::env::var_os("POCKETUI_TEXT_BOXES").unwrap()).unwrap();
            assert!(u.regions.iter().any(|r| r.label == u.core_note && matches!(r.action,ui::Action::Label)),
                "the whole download notice must be read out: {context}");
            assert!(ink.trim().is_empty(), "download notice must wrap without clipping the title or choices: {context}: {ink}");
            if let Some(dir) = std::env::var_os("ANDROID_CORE_SHOTS") {
                let rgb: Vec<u8> = frame.iter().flat_map(|p| [(p>>16) as u8,(p>>8) as u8,*p as u8]).collect();
                std::fs::write(std::path::PathBuf::from(dir).join(format!("extras-{lang:?}-{light}-{w}x{h}-{scale}.png")), pocket_ui::screenshot::encode_rgb(w,h,&rgb).unwrap()).unwrap();
            }
            for (core,_,size) in choices {
                u.missing_core(core);
                pocket_ui::text::boxes::reset();
                let frame = paint(&mut u,w,h);
                pocket_ui::text::boxes::finish(&frame,w as i32,h as i32);
                let visible = std::fs::read_to_string(std::env::var_os("POCKETUI_VISIBLE_TEXT").unwrap()).unwrap();
                assert!(visible.contains(&ui::core_size(size)), "the download size must be visible before choosing {core}: {context}: {visible}");
            }

        }}}
    }
}

#[test]
fn android_updates_open_a_real_page_and_back_returns_to_its_settings_row() {
    let mut u = page();
    u.navigate(ui::Screen::Settings);
    tap(&mut u, "Updates");
    paint(&mut u, 960, 1600);
    assert!(u.regions.iter().any(|r| r.label == "Check again"), "Updates needs the same check action as Linux, not an APK toast");
    u.back();
    assert!(u.screen == ui::Screen::Settings);
    paint(&mut u, 960, 1600);
    assert!(u.regions.iter().any(|r| r.label.starts_with("Updates")));
}

fn android_update_offer() -> pocket_ui::update_channel::Manifest {
    use pocket_ui::update_channel::{Manifest, Release};
    Manifest { format: 1, app: Some(Release { version: "0.4.0-android.2".into(), url: "app.apk".into(), size: 100,
        sha256: "0".repeat(64), mirrors: vec![], preferred_sources: vec![], notes: pocket_ui::update_page::CONFIRM.into() }), ..Default::default() }
}
#[test]
fn android_updates_require_confirmation_and_b_cancels_the_download_in_place() {
    use pocket_ui::pad::Button;
    let mut u = page();
    u.updates.version = "0.4.0-android.1".into();
    u.updates_checked(Ok(android_update_offer()));
    u.navigate(ui::Screen::Updates);
    assert!(matches!(u.press(Button::East), ui::Command::None));
    paint(&mut u, 1600, 960);
    assert!(u.regions.iter().any(|r| r.label == "Install now"));
    assert!(matches!(u.press(Button::South), ui::Command::None), "B dismisses confirmation without starting anything");
    assert!(!u.updates.confirming);
    u.press(Button::East);
    assert!(matches!(u.press(Button::East), ui::Command::UpdateInstall));
    assert!(matches!(u.press(Button::South), ui::Command::UpdateCancel));
    assert!(u.screen == ui::Screen::Updates, "B stops the update while its progress remains on screen");
}
#[test]
fn android_updates_show_download_progress_and_failure_reasons_in_both_appearances() {
    use pocket_ui::i18n::{self, Lang};
    // Language is process-wide; keep this matrix from changing other tests' words.
    if std::env::var_os("ANDROID_UPDATES_WORDS_CHILD").is_none() {
        let result = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "android_02_tests::android_updates_show_download_progress_and_failure_reasons_in_both_appearances", "--nocapture"])
            .env("ANDROID_UPDATES_WORDS_CHILD", "1").output().unwrap();
        assert!(result.status.success() && String::from_utf8_lossy(&result.stdout).contains("1 passed; 0 failed"), "{}\n{}", String::from_utf8_lossy(&result.stdout), String::from_utf8_lossy(&result.stderr));
        return;
    }
    for lang in [Lang::En, Lang::De, Lang::Es] {
        i18n::set(lang);
        for light in [false, true] {
            let mut u = page();
            u.prefs.light = light;
            u.updates.version = "0.4.0-android.1".into();
            u.updates_checked(Ok(android_update_offer()));
            u.navigate(ui::Screen::Updates);
            u.updates.phase = "downloading".into(); u.updates.bytes = 51; u.updates.total = 100; u.updates.cancellable = true;
            for (w,h) in [(1600,960), (960,1600)] {
                paint(&mut u,w,h);
                assert!(u.regions.iter().any(|r| r.label.contains("51")), "progress is readable");
                assert!(u.regions.iter().any(|r| r.label == i18n::tr("Cancel")));
                assert!(u.cut.is_empty(), "progress and actions fit: {:?}", u.cut);
            }
            u.updates.phase.clear(); u.updates.confirming = true;
            for (w,h) in [(1600,960), (960,1600)] {
                paint(&mut u,w,h);
                assert!(u.regions.iter().any(|r| r.label == i18n::tr(ui::updates_page::CONFIRM)), "before installing, say that the app closes and must be opened again");
                assert!(u.cut.is_empty(), "the complete installation explanation fits: {:?}", u.cut);
            }
            u.updates.confirming = false;
            u.updates.phase = "failed".into(); u.updates.cancellable = false;
            for why in ["Connect to Wi-Fi to download updates. Nothing was installed.", "The download is damaged or was changed. Nothing was installed.", "This APK has a different signing key. Nothing was installed."] {
                u.updates.message = why.into();
                paint(&mut u,1600,960);
                assert!(u.regions.iter().any(|r| r.label == i18n::tr(why)), "the complete reason is accessible");
                assert!(u.cut.is_empty(), "failure reason fits: {:?}", u.cut);
            }
        }
    }
    i18n::set(Lang::En);
}

#[test]
fn android_update_lookup_keeps_play_available_while_installation_requires_attention() {
    use pocket_ui::document_library::Game;
    // Exercise the real touch/controller routes: checking a channel must not replace Play with Updates.
    for checking in [false, true] {
        for phase in ["", "done", "failed", "cancelled", "connecting", "downloading", "checking", "permission", "installing"] {
            for action in ["Play", "Just Play", "controller"] {
                let mut u = page();
                u.install(ui::library(vec![Game {
                    name: "Garden".into(), key: "test:update-play".into(), uri: "content://test/garden".into(),
                    relative: "gb/Garden.gb".into(), system: "Game Boy", system_id: "gb", profile: "retroarch:mgba",
                    color: 0, size: 32768, document_size: 32768, missing: vec![], covers: vec![], pictures: vec![], cheats: String::new(), parts: vec![],
                }]));
                u.updates.checking = checking;
                u.updates.phase = phase.into();
                paint(&mut u, 1600, 960);
                let command = if action == "controller" {
                    u.press(pocket_ui::pad::Button::East)
                } else {
                    let area = u.regions.iter().find(|r| r.label == action).unwrap_or_else(|| panic!("missing {action}")).area;
                    u.tap((area.x + area.w / 2) as f32, (area.y + area.h / 2) as f32)
                };
                if matches!(phase, "connecting" | "downloading" | "checking" | "permission" | "installing") {
                    assert!(matches!(command, ui::Command::Notice("Finish or cancel the update before starting a game.")), "{action} must explain the active installation ({phase})");
                    assert!(u.screen == ui::Screen::Updates);
                } else {
                    assert!(matches!(command, ui::Command::Launch), "{action} must start the game while only the channel is being checked ({phase})");
                    assert!(u.screen == ui::Screen::Library, "a channel lookup must not steal the library");
                }
            }
        }
    }
}

/// A new Retroid player's first minutes: every first-start step and both pages without games, on the Retroid screens
/// (Mini V2 1240x1080, RP5/Flip 2/Pocket 6 1920x1080, a 4:3 1280x960) at two Android densities and two font sizes, in
/// English, German and Spanish: no word cut short with "…", nothing running off its room or into other words.
#[test]
fn android_first_start_and_empty_pages_fit_retroid_screens_whole() {
    use pocket_ui::i18n::{self, Lang};
    if std::env::var_os("ANDROID_RETROID_FIT_CHILD").is_none() {
        let ink = tempfile::tempdir().unwrap();
        let result = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "android_02_tests::android_first_start_and_empty_pages_fit_retroid_screens_whole", "--nocapture"])
            .env("ANDROID_RETROID_FIT_CHILD", "1").env("POCKETUI_TEXT_BOXES", ink.path().join("ink"))
            .env("POCKETUI_VISIBLE_TEXT", ink.path().join("visible")).output().unwrap();
        assert!(result.status.success() && String::from_utf8_lossy(&result.stdout).contains("1 passed; 0 failed"), "{}\n{}", String::from_utf8_lossy(&result.stdout), String::from_utf8_lossy(&result.stderr));
        return;
    }
    let mut faults = Vec::new();
    for lang in [Lang::En, Lang::De, Lang::Es] {
        i18n::set(lang);
        for (w, h, dp) in [(1240, 1080, 2.0), (1240, 1080, 2.5), (1920, 1080, 2.5), (1920, 1080, 3.0), (1280, 960, 2.0)] {
            for scale in [1.0f32, 1.3] {
                for page_name in ["s1", "s2", "s3", "s4", "s5", "s7", "s8", "nofolder", "emptyfolder"] {
                    let mut u = page();
                    u.prefs.choose_language(lang);
                    u.dp = dp; u.system_font_scale = scale; u.controller_connected = true;
                    match page_name {
                        "nofolder" => {}
                        "emptyfolder" => u.install(ui::library(vec![])),
                        step => { u.prefs.onboarding_done = false; u.prefs.onboarding_step = step[1..].parse::<usize>().unwrap() - 1; }
                    }
                    paint(&mut u, w, h);
                    pocket_ui::text::boxes::reset();
                    let frame = paint(&mut u, w, h);
                    pocket_ui::text::boxes::finish(&frame, w as i32, h as i32);
                    let ink = std::fs::read_to_string(std::env::var_os("POCKETUI_TEXT_BOXES").unwrap()).unwrap_or_default();
                    let visible = std::fs::read_to_string(std::env::var_os("POCKETUI_VISIBLE_TEXT").unwrap()).unwrap_or_default();
                    let context = format!("{lang:?} {w}x{h} dp={dp} font={scale} {page_name}");
                    let cut = visible.lines().filter(|l| l.contains('…') || l.ends_with("..."));
                    let ink = ink.lines().filter(|l| l.starts_with(['E', 'O', 'V']));
                    faults.extend(cut.chain(ink).map(|l| (context.clone(), l.to_string())));
                }
            }
        }
    }
    i18n::set(Lang::En);
    assert!(faults.is_empty(), "words cut short (…) or running off or into other words on a Retroid screen: {faults:#?}");
}

// Test fixtures: game names, never painted by the app (i18n-extract skips `cfg(test)` items).
#[cfg(test)]
fn retroid_games() -> ui::Library {
    use pocket_ui::document_library::{scan, Document};
    let names = ["gb/Libbet.gb", "gb/Pocket.gb", "gb/Tobu Tobu Girl.gb", "gba/Voxelburg.gba",
        "nes/Thwaite.nes", "snes/Pipes.sfc", "n64/N64Hello.n64"];
    ui::library(scan(names.iter().enumerate().map(|(i, r)| Document { root_name: String::new(), uri: format!("content://qa/{i}"),
        authority: "qa".into(), id: i.to_string(), relative: r.to_string(), sheet: String::new(), size: 100_000 }).collect()))
}

/// Search on a Retroid: before typing it says what to do (not "No games found"), and when the keyboard leaves no room for
/// the list (Mini V2 landscape) the matches still show above it.
#[test]
fn android_search_says_what_to_do_before_typing_and_shows_matches_above_the_keyboard() {
    for (w, h, dp) in [(1240, 1080, 2.0), (1240, 1080, 2.5), (1920, 1080, 2.3), (1280, 960, 2.0), (1080, 1920, 2.3)] {
        let mut u = page();
        u.dp = dp;
        u.install(retroid_games());
        u.navigate(ui::Screen::Search);
        assert!(u.keyboard.is_some());
        paint(&mut u, w, h);
        let labels: Vec<String> = u.regions.iter().map(|r| r.label.clone()).collect();
        assert!(!labels.iter().any(|l| l.contains("No games found")), "{w}x{h} dp={dp}: nothing typed yet is not 'no games': {labels:?}");
        assert!(labels.iter().any(|l| l.contains("Type a few letters")), "{w}x{h} dp={dp}: say what to do: {labels:?}");
        u.query = "tobo".into();
        u.filter();
        paint(&mut u, w, h);
        let labels: Vec<String> = u.regions.iter().map(|r| r.label.clone()).collect();
        assert!(labels.iter().any(|l| l.contains("Tobu Tobu Girl")), "{w}x{h} dp={dp}: the match must be visible while typing: {labels:?}");
    }
}

/// The selected game stands on the landscape shelf as its open case on every Retroid screen, wherever it is in its shelf
/// (Mini V2 1240x1080 left it out: its neighbours took a third of the shelf and the case no longer fitted).
#[cfg(test)]
#[test]
fn android_the_selected_case_is_on_the_shelf_on_every_retroid_screen() {
    use pocket_ui::settings::Theme;
    for theme in [Theme::Shelf, Theme::Cabinet] {
        for (w, h, dp) in [(1240, 1080, 2.0), (1240, 1080, 2.5), (1920, 1080, 2.3), (1920, 1080, 3.0), (1280, 960, 2.0)] {
            for name in ["Libbet", "Pocket", "Tobu Tobu Girl"] {
                let mut u = page();
                u.prefs.theme = theme;
                u.dp = dp;
                u.install(retroid_games());
                u.selected = u.visible.iter().position(|&i| u.library.as_ref().unwrap().games[i].name == name).unwrap();
                u.follow_selection();
                paint(&mut u, w, h);
                let selected = u.selected;
                let case = u.regions.iter().find(|r| matches!(r.action, ui::Action::Row(p) if p == selected));
                assert!(case.is_some_and(|r| r.label == name && r.area.x >= 0 && r.area.x + r.area.w <= w as i32),
                    "{theme:?} {w}x{h} dp={dp}: {name} must stand on the shelf: {:?}", u.regions.iter().map(|r| (r.label.clone(), r.area.x, r.area.w)).collect::<Vec<_>>());
            }
        }
    }
}

/// Game check after choosing a folder: "Not now", B, Android Back and Cancel end where the player came from (the empty
/// Games page, a Settings row, Favorites), never on a page they did not choose. Found on the Odin: Games (empty) -> folder ->
/// "Game check" offer -> "Not now" landed in Settings.
#[test]
fn android_game_check_leaves_to_where_it_was_reached_from() {
    use pocket_ui::pad::Button;
    // How the offer is left: the "Not now" button as painted, B, Android's Back, and the same on a running check.
    let leaves: [(&str, fn(&mut ui::Ui)); 4] = [
        ("Not now", |u| tap(u, "Not now")),
        ("B", |u| { u.press(Button::South); }),
        ("Back", |u| { u.back(); }),
        ("cancel_while_running", |u| { u.press(Button::East); u.press(Button::South); }),
    ];
    for (name, from) in [("Games", ui::Screen::Library), ("Settings", ui::Screen::Settings), ("Favorites", ui::Screen::Favorites)] {
        for (how, leave) in leaves {
            // An empty Games page (no folder yet) is the case that went wrong; with games on the shelf it is the same.
            for with_games in [false, true] {
                let mut u = page();
                if with_games {
                    u.install(retroid_games());
                }
                u.navigate(from);
                u.offer_check(7);
                assert!(u.screen == ui::Screen::GameCheck, "{name}: the offer is on screen");
                leave(&mut u);
                assert!(u.screen == from, "{name} (games {with_games}): {how} must lead back where the folder was chosen, not to {}", [("Games", ui::Screen::Library), ("Settings", ui::Screen::Settings), ("Favorites", ui::Screen::Favorites), ("the Game check", ui::Screen::GameCheck)].iter().find(|(_, s)| *s == u.screen).map_or("another page", |(n, _)| *n));
            }
        }
    }
    // Settings -> Game check -> B stays in Settings, and a finished report climbs its levels before it leaves.
    let mut u = page();
    u.navigate(ui::Screen::Settings);
    tap(&mut u, "Game check");
    assert!(u.screen == ui::Screen::GameCheck);
    u.press(Button::South);
    assert!(u.screen == ui::Screen::Settings);
    // Offer from the empty Games page, check it, read the result, leave: the Games page again.
    let mut u = page();
    u.offer_check(3);
    u.press(Button::East);
    u.check_done(pocket_ui::document_check::Outcome { report: pocket_ui::game_check::demo(true), unreadable: Vec::new(), no_folder: false, documents: Vec::new() });
    u.press(Button::South);
    assert!(u.screen == ui::Screen::Library, "the report's B ends on the Games page, not in Settings");
}

/// Choosing an empty or wrong folder from Settings ended in silence (the old games stayed, nothing said why nothing
/// came): the player is told, in their language, which folder to choose.
#[test]
fn android_a_chosen_folder_without_new_games_says_which_folder_to_choose() {
    use pocket_ui::i18n::{self, Lang};
    // Language is process-wide; keep it from changing other tests' words.
    if std::env::var_os("ANDROID_FOLDER_NOTE_CHILD").is_none() {
        let result = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "android_02_tests::android_a_chosen_folder_without_new_games_says_which_folder_to_choose", "--nocapture"])
            .env("ANDROID_FOLDER_NOTE_CHILD", "1").output().unwrap();
        assert!(result.status.success() && String::from_utf8_lossy(&result.stdout).contains("1 passed; 0 failed"), "{}\n{}", String::from_utf8_lossy(&result.stdout), String::from_utf8_lossy(&result.stderr));
        return;
    }
    assert!(ui::Ui::folder_without_new_games(3).is_none(), "new games: the Game check offer speaks instead");
    for lang in [Lang::En, Lang::De, Lang::Es] {
        i18n::set(lang);
        let note = ui::Ui::folder_without_new_games(0).expect("an empty folder must not end in silence");
        assert!(!note.contains("No new games") || lang == Lang::En, "{lang:?}: translated: {note}");
        // Android 12+ shows two lines of a toast (about 35 characters each on a Mini V2): longer advice was cut.
        assert!(note.chars().count() <= 70, "{lang:?}: the whole toast must be readable: {note}");
    }
    i18n::set(Lang::En);
}

/// The controls card in every format it is shown in, Retroid (Android) and the handhelds, in English, German and
/// Spanish, with normal and large text, Barlow and Retro: no word cut short or running into another, no raw core
/// string on screen, and the picture fills its room (a 1920 x 1080 card sat small at the top before).
#[cfg(test)] // fixtures: cores' raw names and game names, never painted (i18n-extract skips it)
#[test]
fn controls_card_fits_every_format_language_and_text_size() {
    use pocket_ui::{controls_card, font_style::FontStyle, i18n::{self, Lang}, text_size::TextSize};
    if std::env::var_os("CONTROLS_CARD_FIT_CHILD").is_none() {
        let ink = tempfile::tempdir().unwrap();
        let result = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "android_02_tests::controls_card_fits_every_format_language_and_text_size", "--nocapture"])
            .env("CONTROLS_CARD_FIT_CHILD", "1").env("POCKETUI_TEXT_BOXES", ink.path().join("ink"))
            .env("POCKETUI_VISIBLE_TEXT", ink.path().join("visible")).output().unwrap();
        assert!(result.status.success() && String::from_utf8_lossy(&result.stdout).contains("1 passed; 0 failed"), "{}\n{}", String::from_utf8_lossy(&result.stdout), String::from_utf8_lossy(&result.stderr));
        return;
    }
    // Raw strings cores report for these consoles; none may be painted.
    let core = |system: &str| -> Vec<Target> {
        let raw: &[(u8, &str)] = match system {
            "gba" => &[(8, "A"), (0, "B"), (9, "Turbo A"), (1, "Turbo B"), (10, "L"), (11, "R"), (14, "Darken Solar Sensor"), (15, "Brighten Solar Sensor"), (2, "Select"), (3, "Start")],
            "n64" => &[(0, "A Button (C3)"), (1, "B Button (C2)"), (8, "(C1)"), (9, "(C4)"), (13, "C Buttons Mode"), (12, "Z Trigger"), (11, "R Shoulder"), (10, "L Shoulder"), (3, "Start")],
            "atari7800" => &[(0, "Fire 1"), (8, "Fire 2")],
            _ => &[],
        };
        raw.iter().map(|(id, l)| Target { id: *id, label: l.to_string() }).collect()
    };
    let raw_words = ["Turbo", "Solar", "(C", "Fire 1", "Fire 2", "Android gamepad", "C Buttons Mode"];
    let cards: &[(&str, &str, bool)] = &[("gb", "Tobu Tobu Girl", false), ("gba", "Boktai - The Sun Is in Your Hand", false), ("gba", "Voxelburg", true),
        ("snes", "Pipes", false), ("nes", "Thwaite", false), ("psx", "Crash Bandicoot 2", false), ("psp", "Bomberman", true), ("n64", "N64Hello", false),
        ("megadrive", "Old Towers", false), ("mastersystem", "Test", false), ("gamegear", "Test", true), ("saturn", "Twelve Snakes", false),
        ("dreamcast", "Sonic", false), ("atari7800", "Test", false)];
    let mut faults = Vec::new();
    let check = |frame: &[u32], w: u32, h: u32, context: String, faults: &mut Vec<(String, String)>| {
        pocket_ui::text::boxes::finish(frame, w as i32, h as i32);
        let ink = std::fs::read_to_string(std::env::var_os("POCKETUI_TEXT_BOXES").unwrap()).unwrap_or_default();
        let visible = std::fs::read_to_string(std::env::var_os("POCKETUI_VISIBLE_TEXT").unwrap()).unwrap_or_default();
        let cut = visible.lines().filter(|l| l.contains('…') || l.ends_with("..."));
        let raw = visible.lines().filter(|l| raw_words.iter().any(|r| l.contains(r)));
        let ink = ink.lines().filter(|l| l.starts_with(['E', 'O', 'V']));
        faults.extend(cut.chain(raw).chain(ink).map(|l| (context.clone(), l.to_string())));
    };
    for style in [FontStyle::Barlow, FontStyle::Retro] {
        pocket_ui::font_style::set(style);
        for lang in [Lang::En, Lang::De, Lang::Es] {
            i18n::set(lang);
            for size in [TextSize::Normal, TextSize::Large] {
                type_scale::set(size);
                for &(system, game, touch) in cards {
                    let names = core(system);
                    // Android: the Retroid formats at their densities, on the shared mobile canvas, with Android's own
                    // font size too.
                    for (pw, ph, dp) in [(1240, 1080, 2.0), (1240, 1080, 2.5), (1920, 1080, 2.5), (1920, 1080, 3.0), (1280, 960, 2.0)] {
                        for scale in [1.0f32, 1.3] {
                            type_scale::set_system_scale(scale);
                            let (w, h) = pocket_ui::scaled_layout::mobile_size((pw, ph), dp);
                            let mut p = vec![0; (w * h) as usize];
                            let mut text = Text::builtin().following_font();
                            pocket_ui::text::boxes::reset();
                            let mut c = Canvas::new(&mut p, w, h);
                            play_cards::paint_controls(&mut c, &mut text, system, game, &names, touch, 0);
                            let room = play_cards::controls_area(&c);
                            let card = play_cards::card(system, game, &names, touch);
                            let pic = controls_card::picture_room(&mut text, &card, (room.x, room.y, room.w, room.h));
                            let g = controls_card::geometry(&card, pic);
                            if card.known && g.share < 0.8 && ((2 * g.d + 2 * g.r) as f32) < 0.85 * pic.2 as f32 / 2.0 {
                                faults.push((format!("{system} {pw}x{ph} dp={dp}"), format!("picture fills {:.2} of its room", g.share)));
                            }
                            check(&p, w, h, format!("{style:?} {lang:?} {size:?} font={scale} android {pw}x{ph} dp={dp} {system} {game}"), &mut faults);
                        }
                    }
                    type_scale::set_system_scale(1.0);
                    if touch {
                        continue;
                    }
                    // The handhelds: 640 x 480 and 1024 x 768 paint at a logical 1280 x 960, the Mini V2 at its own size.
                    for panel in [(640u32, 480u32), (1024, 768), (1240, 1080)] {
                        let (w, h) = pocket_ui::scaled_layout::logical_size(panel);
                        let mut p = vec![0; (w * h) as usize];
                        let mut text = Text::builtin().following_font();
                        pocket_ui::text::boxes::reset();
                        let mut c = Canvas::new(&mut p, w, h);
                        let mapping = pocket_ui::controls::Mapping::from_values(system, &[("ctl.x".to_string(), "a".to_string())].into_iter().collect());
                        let card = controls_card::build(&controls_card::Input { system, system_name: system, game, mapping: &mapping, core_names: &names,
                            shortcuts: Some(controls_card::Shortcuts { save: true, load: true, zoom: true }),
                            layout: Some(pocket_ui::button_layout_model::ButtonLayout::Xbox), menu_key: "HOME", touch: false });
                        let colors = controls_card::Colors { bg: 0, line: 0x6b6963, text: 0xffffff, dim: 0x9a978f, accent: pocket_ui::palette::DARK.amber };
                        // The room `App::paint_card_screen` gives the card.
                        use pocket_ui::kit::{BTN_PAD, PAD, PILL};
                        let (x, top, bottom) = (PAD + BTN_PAD, PAD + PILL + 46, h as i32 - PAD - PILL - 24);
                        controls_card::paint(&mut c, &mut text, &card, (x, top, w as i32 - 2 * x, bottom - top), colors);
                        check(&p, w, h, format!("{style:?} {lang:?} {size:?} handheld {}x{} {system}", panel.0, panel.1), &mut faults);
                    }
                }
            }
        }
    }
    pocket_ui::font_style::set(FontStyle::Barlow);
    type_scale::set(TextSize::Normal);
    i18n::set(Lang::En);
    faults.dedup();
    assert!(faults.is_empty(), "the controls card cut a word, ran text into other text, showed a raw core string or sat small: {faults:#?}");
}

/// `CARD_RENDER_DIR=<dir> cargo test --lib card_renders -- --ignored`: the controls card of a few consoles as
/// PPM pictures, on Android (Retroid formats) and in a handheld's frame, for looking at.
#[cfg(test)] // fixtures: cores' raw names and game names, never painted (i18n-extract skips it)
#[test]
#[ignore]
fn card_renders() {
    use pocket_ui::controls_card;
    let Some(dir) = std::env::var_os("CARD_RENDER_DIR").map(std::path::PathBuf::from) else { return };
    let save = |name: &str, p: &[u32], w: u32, h: u32| {
        let mut out = format!("P6\n{w} {h}\n255\n").into_bytes();
        for px in p {
            out.extend([(px >> 16) as u8, (px >> 8) as u8, *px as u8]);
        }
        std::fs::write(dir.join(format!("{name}.ppm")), out).unwrap();
    };
    let games = [("gb", "Tobu Tobu Girl"), ("gba", "Boktai - The Sun Is in Your Hand"), ("snes", "Pipes"), ("psx", "Crash Bandicoot 2"), ("psp", "Bomberman"), ("n64", "N64Hello"), ("megadrive", "Old Towers"), ("mastersystem", "Test"), ("atari7800", "Test")];
    let langs: &[(pocket_ui::i18n::Lang, &str)] = &[(pocket_ui::i18n::Lang::En, "en"), (pocket_ui::i18n::Lang::De, "de"), (pocket_ui::i18n::Lang::Es, "es")];
    if std::env::var_os("CARD_RENDER_RETRO").is_some() {
        pocket_ui::font_style::set(pocket_ui::font_style::FontStyle::Retro);
    }
    for (lang, l) in langs {
        pocket_ui::i18n::set(*lang);
        for (system, game) in games {
            for (pw, ph, dp, touch) in [(1240u32, 1080u32, 2.0f32, false), (1920, 1080, 2.5, false), (1280, 960, 2.0, true)] {
                // The shared mobile canvas the launcher paints at (`scaled_layout::mobile_size`).
                let (w, h) = pocket_ui::scaled_layout::mobile_size((pw, ph), dp);
                let mut p = vec![0; (w * h) as usize];
                let mut c = Canvas::new(&mut p, w, h);
                c.clear(pocket_ui::palette::pal().void);
                play_cards::paint_controls(&mut c, &mut Text::builtin().following_font(), system, game, &[], touch, 0);
                save(&format!("android-{system}-{pw}x{ph}-{l}"), &p, w, h);
            }
            for panel in [(640u32, 480u32), (1024, 768), (1240, 1080)] {
                // The handhelds paint at a logical size and scale it to the panel (`scaled_layout`).
                let (w, h) = pocket_ui::scaled_layout::logical_size(panel);
                let mut p = vec![0; (w * h) as usize];
                let mut c = Canvas::new(&mut p, w, h);
                let pal = pocket_ui::palette::pal();
                let colors = controls_card::Colors { bg: pal.void, line: pal.dust, text: pal.chalk, dim: pal.dust, accent: pal.amber };
                c.clear(colors.bg);
                let mut text = Text::builtin().following_font();
                let mapping = pocket_ui::controls::Mapping::defaults(system);
                let name = pocket_ui::document_library::system(system).map_or(system, |s| s.1);
                let card = controls_card::build(&controls_card::Input { system, system_name: name, game, mapping: &mapping, core_names: &[], shortcuts: Some(controls_card::Shortcuts { save: true, load: true, zoom: false }), layout: Some(pocket_ui::button_layout_model::ButtonLayout::Nintendo), menu_key: "HOME", touch: false });
                // The shelf's frame (`App::paint_card_screen`): heading, sub line, card, hint.
                use pocket_ui::kit::{BTN_PAD, PAD, PILL};
                let x = PAD + BTN_PAD;
                let heading = text.fit(pocket_ui::text::Face::Title, 50, game, w as i32 - 2 * x - 330);
                text.draw(&mut c, pocket_ui::text::Face::Title, 50, x, PAD + 58, colors.text, &heading);
                let sub = format!("{} · {}", pocket_ui::i18n::tr("Controls"), name);
                text.draw(&mut c, pocket_ui::text::Face::Body, 28, x, PAD + 104, colors.dim, &sub);
                let top = PAD + PILL + 46;
                let bottom = h as i32 - PAD - PILL - 24;
                controls_card::paint(&mut c, &mut text, &card, (x, top, w as i32 - 2 * x, bottom - top), colors);
                save(&format!("linux-{system}-{}x{}-{l}", panel.0, panel.1), &p, w, h);
            }
        }
    }
    pocket_ui::i18n::set(pocket_ui::i18n::Lang::En);
}

// ---- Settings → Report a problem -----------------------------------------------------------------------------------

/// A Retroid tester reaches "Report a problem" with the D-pad alone, B goes back one step onto its row, and a finger gets there too.
#[test]
fn android_report_a_problem_is_reached_with_the_d_pad_alone_and_b_goes_back_one_step() {
    use pocket_ui::pad::Button;
    let mut u = page();
    u.navigate(ui::Screen::Settings);
    assert!(matches!(choose(&mut u, "Report a problem"), ui::Command::None));
    assert!(u.screen == ui::Screen::Report);
    assert_eq!(sheet_rows(&mut u), ["Share the report", "Copy the report", "Back"]);
    for row in ["Share the report", "Copy the report", "Back"] {
        dpad_to(&mut u, row);
    }
    // B: onto the Settings row it came from, not the top of Settings or out to the games.
    u.press(Button::South);
    assert!(u.screen == ui::Screen::Settings);
    paint(&mut u, 960, 1600);
    let on = u.regions.iter().find(|r| matches!(r.action, ui::Action::Row(_)) && u.selected_region(r)).map(|r| r.label.clone());
    assert_eq!(on.as_deref(), Some("Report a problem"));
    // The Back row and Android's own Back do the same; START goes to the games.
    u.press(Button::East);
    assert!(matches!(choose(&mut u, "Back"), ui::Command::None));
    assert!(u.screen == ui::Screen::Settings);
    u.press(Button::East);
    u.back();
    assert!(u.screen == ui::Screen::Settings);
    u.press(Button::East);
    u.press(Button::Start);
    assert!(u.screen == ui::Screen::Library);
    // A finger gets there too, and the Settings tab stays the lit one.
    u.navigate(ui::Screen::Settings);
    tap(&mut u, "Report a problem");
    assert!(u.screen == ui::Screen::Report);
    paint(&mut u, 960, 1600);
    assert!(u.regions.iter().any(|r| matches!(r.action, ui::Action::Tab(ui::Screen::Settings))));
    tap(&mut u, "Copy the report");
    assert!(u.report_busy());
}

/// The page says where it stands at every step, with whatever happened (or nothing at all): never a silent press.
#[test]
fn android_report_a_problem_says_in_words_where_it_stands_at_every_step() {
    use pocket_ui::report_phone as report;
    let mut u = page();
    u.navigate(ui::Screen::Settings);
    choose(&mut u, "Report a problem");
    let words = page_words(&mut u);
    assert!(words.contains("the app sends nothing") && words.contains("Left out:"), "{words}");
    assert!(matches!(choose(&mut u, "Share the report"), ui::Command::ReportShare));
    assert!(u.report_busy() && page_words(&mut u).contains("Writing the report"), "{}", page_words(&mut u));
    // While it is on its way a second A starts nothing, and the page can still be left.
    assert!(matches!(choose(&mut u, "Copy the report"), ui::Command::None));
    assert!(matches!(choose(&mut u, "Share the report"), ui::Command::None));
    u.report_said(report::SHARED);
    assert!(!u.report_busy() && page_words(&mut u).contains("The share sheet is open"), "{}", page_words(&mut u));
    assert!(matches!(choose(&mut u, "Copy the report"), ui::Command::ReportCopy));
    u.report_said(report::COPIED);
    assert!(page_words(&mut u).contains("Copied. Paste it into your report"), "{}", page_words(&mut u));
    // Android with no share target, and a clipboard that refuses: the answer names the other row.
    u.report_said(report::NO_SHARE);
    assert!(page_words(&mut u).contains("Use Copy the report instead"));
    u.report_said(report::NO_COPY);
    assert!(page_words(&mut u).contains("Use Share the report instead"));
    u.report_said(report::FAILED);
    assert!(page_words(&mut u).contains("could not be written"));
    // The rows are the same after every answer, and leaving the page starts the next visit fresh.
    assert_eq!(sheet_rows(&mut u), ["Share the report", "Copy the report", "Back"]);
    u.back();
    assert!(u.screen == ui::Screen::Settings);
    choose(&mut u, "Report a problem");
    assert!(page_words(&mut u).contains("Left out:"), "{}", page_words(&mut u));
}

// Test fixtures: English words that must not show in German or Spanish, and the fault's text (never painted by the app).
#[cfg(test)]
fn report_english_left(words: &str) -> Vec<String> {
    ["report", "Report", "Share", "Copy", "Back", "sends nothing", "clipboard"].iter().filter(|w| words.contains(**w)).map(|w| format!("said in English ({w}): {words}")).collect()
}
#[cfg(test)]
fn report_settings_row_cut() -> String {
    "the Settings row is cut".into()
}

/// The page on the Retroid screens (Mini V2 1240x1080, RP5/Flip 2/Pocket 6 1920x1080, 4:3 1280x960) at two densities and
/// two text sizes in English, German and Spanish: every step whole, nothing cut with "…", nothing running off or into
/// other words, nothing left in English in German and Spanish, and Settings' own row whole.
#[test]
fn android_report_a_problem_fits_retroid_screens_whole_in_english_german_and_spanish() {
    use pocket_ui::{i18n::{self, Lang}, report_phone as report};
    use ui::report_page::Step;
    if std::env::var_os("ANDROID_REPORT_FIT_CHILD").is_none() {
        let ink = tempfile::tempdir().unwrap();
        let result = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "android_02_tests::android_report_a_problem_fits_retroid_screens_whole_in_english_german_and_spanish", "--nocapture"])
            .env("ANDROID_REPORT_FIT_CHILD", "1").env("POCKETUI_TEXT_BOXES", ink.path().join("ink")).env("POCKETUI_VISIBLE_TEXT", ink.path().join("visible")).output().unwrap();
        assert!(result.status.success() && String::from_utf8_lossy(&result.stdout).contains("1 passed; 0 failed"), "{}\n{}", String::from_utf8_lossy(&result.stdout), String::from_utf8_lossy(&result.stderr));
        return;
    }
    let steps: Vec<Step> = [Step::Rest, Step::Busy].into_iter()
        .chain([report::SHARED, report::COPIED, report::NO_SHARE, report::NO_COPY, report::FAILED].map(|s| Step::Said(s.to_string()))).collect();
    let mut faults = Vec::new();
    for lang in [Lang::En, Lang::De, Lang::Es] {
        i18n::set(lang);
        for (w, h, dp) in [(1240, 1080, 2.0), (1240, 1080, 2.5), (1920, 1080, 2.5), (1920, 1080, 3.0), (1280, 960, 2.0)] {
            for scale in [1.0f32, 1.3] {
                for step in &steps {
                    let mut u = page();
                    u.prefs.choose_language(lang);
                    u.dp = dp; u.system_font_scale = scale; u.controller_connected = true; u.touch_mode = false;
                    u.navigate(ui::Screen::Settings);
                    choose(&mut u, &ui::tr("Report a problem"));
                    assert!(u.screen == ui::Screen::Report, "{lang:?} {w}x{h}: the page did not open");
                    u.report.step = step.clone();
                    paint(&mut u, w, h);
                    pocket_ui::text::boxes::reset();
                    let frame = paint(&mut u, w, h);
                    pocket_ui::text::boxes::finish(&frame, w as i32, h as i32);
                    let ink = std::fs::read_to_string(std::env::var_os("POCKETUI_TEXT_BOXES").unwrap()).unwrap_or_default();
                    let visible = std::fs::read_to_string(std::env::var_os("POCKETUI_VISIBLE_TEXT").unwrap()).unwrap_or_default();
                    let context = format!("{lang:?} {w}x{h} dp={dp} font={scale} {step:?}");
                    let writing = ui::tr(report::WRITING);
                    let cut = visible.lines().filter(|l| (l.contains('…') || l.ends_with("...")) && l.trim() != writing);
                    let ink = ink.lines().filter(|l| l.starts_with(['E', 'O', 'V']));
                    faults.extend(cut.chain(ink).map(|l| (context.clone(), l.to_string())));
                    if !u.cut.is_empty() {
                        faults.push((context.clone(), format!("shown cut: {:?}", u.cut)));
                    }
                    let words = u.regions.iter().filter(|r| matches!(r.action, ui::Action::Label | ui::Action::Row(_))).map(|r| r.label.clone()).collect::<Vec<_>>().join(" ");
                    if lang != Lang::En {
                        faults.extend(report_english_left(&words).into_iter().map(|f| (context.clone(), f)));
                    }
                    // Settings' own row is whole, with the D-pad on it.
                    u.back();
                    dpad_to(&mut u, &ui::tr("Report a problem"));
                    paint(&mut u, w, h);
                    if u.cut.contains(&ui::tr("Report a problem")) {
                        faults.push((context.clone(), report_settings_row_cut()));
                    }
                }
            }
        }
    }
    i18n::set(Lang::En);
    assert!(faults.is_empty(), "Report a problem is cut short, runs off its room or is left in English on a Retroid screen: {faults:#?}");
}

fn offers(names: &[(&str, &str, u64)]) -> Vec<pocket_ui::android_cores::CorePackage> {
    names.iter().map(|(core, name, size)| serde_json::from_value(serde_json::json!({
        "id":format!("android-core-{core}"),"name":name,"category":"android-cores","version":"1","description":"","size":size,
        "url":"core.tar.gz","sha256":"a".repeat(64),"kind":"tar","dest":"/ignored","core":core,"systems":[core],
        "library_sha256":"b".repeat(64),"corresponding_source":"source.tar.gz","source_sha256":"c".repeat(64)
    })).unwrap()).collect()
}
const SYSTEMS: [(&str, &str, u64); 4] = [("ppsspp", "PSP", 18_600_000), ("melondsds", "DS", 1_900_000), ("mupen64plus_next", "N64", 1_620_000), ("flycast", "Dreamcast", 4_800_000)];

/// The release preview ships before the download channel is public: PSP, DS, N64 and Dreamcast ended in "Download now" and a
/// red error. Until `android/core-channel.url` holds an address, nothing is offered and nothing fails.
#[test]
fn without_a_public_download_channel_the_extra_systems_read_coming_soon_and_nothing_can_fail() {
    use pocket_ui::pad::Button;
    // The language is process-wide: the other languages are checked through `tr_in`, not by switching it under parallel tests.
    for text in ["Coming soon", ui::COMING_SOON_STEP, ui::COMING_SOON_NOTE] {
        for lang in [pocket_ui::i18n::Lang::De, pocket_ui::i18n::Lang::Es] {
            assert_ne!(pocket_ui::i18n::tr_in(lang, text), text, "{lang:?}: {text}");
        }
    }
    for lang in [pocket_ui::i18n::Lang::En] {
        let mut u = page();
        assert!(!u.core_channel_open, "a fresh Ui has no channel: honest by default");
        u.prefs.onboarding_done = false;
        u.prefs.onboarding_step = 5;
        u.core_offers = offers(&SYSTEMS);
        paint(&mut u, 720, 1600);
        let soon = ui::tr("Coming soon");
        for (_, name, size) in SYSTEMS {
            assert!(u.regions.iter().any(|r| r.label == format!("{name} · {soon}")), "{lang:?} {name} lists as coming soon: {:?}", u.regions.iter().map(|r| &r.label).collect::<Vec<_>>());
            assert!(!u.regions.iter().any(|r| r.label.contains(&ui::core_size(size))), "{lang:?}: no download size is shown");
        }
        let said = page_words_of(&u);
        assert!(said.contains(&ui::tr(ui::COMING_SOON_STEP)), "{lang:?}: the step says why: {said}");
        assert!(u.cut.is_empty(), "{lang:?}: {:?}", u.cut);
        // A press on a system chooses nothing and gives no consent; Continue moves on without a download.
        u.press(Button::East);
        assert!(u.prefs.selected_cores.is_empty());
        let row = u.regions.iter().find(|r| r.label.starts_with("PSP")).unwrap().area;
        u.tap((row.x + row.w / 2) as f32, (row.y + row.h / 2) as f32);
        assert!(u.prefs.selected_cores.is_empty(), "{lang:?}: a tap chooses nothing either");
        let next = u.regions.iter().find(|r| r.label == ui::tr("Continue")).unwrap().area;
        u.tap((next.x + next.w / 2) as f32, (next.y + next.h / 2) as f32);
        assert_eq!(u.prefs.onboarding_step, 6);
        assert!(!u.prefs.core_download_consent, "{lang:?}: Continue is no consent to a download that cannot happen");
    }
    // Settings → Cozy extras lists them the same way; a press says so and starts nothing.
    let mut u = page();
    u.core_offers = offers(&SYSTEMS);
    u.navigate(ui::Screen::Extras);
    for (_, name, _) in SYSTEMS {
        dpad_to(&mut u, name);
        let rows = sheet_rows(&mut u);
        assert!(rows.iter().any(|r| r == &format!("{name} · Coming soon")), "{name}: {rows:?}");
        assert!(!rows.iter().any(|r| r.contains("Download now") || r.contains("MB")), "{rows:?}");
    }
    assert!(matches!(choose(&mut u, "PSP"), ui::Command::None));
    assert_eq!(u.core_note, ui::COMING_SOON_NOTE);
    // A game that needs one of them (PSP) lands on that page, with the same words, and never on a failing download.
    u.missing_core("ppsspp");
    assert_eq!(u.core_note, ui::COMING_SOON_NOTE);
    assert!(!page_words(&mut u).contains("Download now"));
    // With a public channel the very same Ui offers the download again.
    u.core_channel_open = true;
    u.missing_core("ppsspp");
    assert!(u.core_note.contains("Download now"));
    assert!(sheet_rows(&mut u).iter().any(|r| r.contains("Download now")));
}

fn page_words_of(u: &ui::Ui) -> String {
    u.regions.iter().filter(|r| matches!(r.action, ui::Action::Label)).map(|r| r.label.clone()).collect::<Vec<_>>().join(" | ")
}

// ---- Settings → Help -------------------------------------------------------------------------------------------------

/// A player reaches Help with the D-pad alone, every topic opens and says its answer, B goes back one step onto Help's row, and
/// a finger gets there too. Nothing on the page is a silent row: opening a topic always shows words.
#[test]
fn android_help_is_reached_with_the_d_pad_alone_every_topic_answers_and_b_goes_back_one_step() {
    use pocket_ui::pad::Button;
    let mut u = page();
    u.navigate(ui::Screen::Settings);
    assert!(matches!(choose(&mut u, "Help"), ui::Command::None));
    assert!(u.screen == ui::Screen::Help);
    let titles: Vec<String> = ui::help_page::TOPICS.iter().map(|(t, _)| t.to_string()).chain(["Back".to_string()]).collect();
    assert_eq!(sheet_rows(&mut u), titles);
    assert!(page_words(&mut u).contains("Choose a topic"), "the page says what it is before a topic is chosen: {}", page_words(&mut u));
    for (title, answer) in ui::help_page::TOPICS {
        dpad_to(&mut u, title);
        u.press(Button::East);
        let words = page_words(&mut u);
        assert!(words.contains(title) && words.contains(answer), "{title}: {words}");
        assert!(u.screen == ui::Screen::Help, "{title}: a topic stays on the page");
    }
    // B: onto the Help row it came from, not the top of Settings or out to the games.
    u.press(Button::South);
    assert!(u.screen == ui::Screen::Settings);
    paint(&mut u, 960, 1600);
    let on = u.regions.iter().find(|r| matches!(r.action, ui::Action::Row(_)) && u.selected_region(r)).map(|r| r.label.clone());
    assert_eq!(on.as_deref(), Some("Help"));
    // Back, Android's own Back and START: Settings, Settings, the games. Leaving starts the next visit fresh.
    u.press(Button::East);
    assert!(matches!(choose(&mut u, "Back"), ui::Command::None));
    assert!(u.screen == ui::Screen::Settings);
    u.press(Button::East);
    u.back();
    assert!(u.screen == ui::Screen::Settings);
    u.press(Button::East);
    u.press(Button::Start);
    assert!(u.screen == ui::Screen::Library);
    u.navigate(ui::Screen::Settings);
    tap(&mut u, "Help");
    assert!(u.screen == ui::Screen::Help);
    assert!(page_words(&mut u).contains("Choose a topic"));
    paint(&mut u, 960, 1600);
    assert!(u.regions.iter().any(|r| matches!(r.action, ui::Action::Tab(ui::Screen::Settings))), "the Settings tab stays the lit one");
    tap(&mut u, "Pause menu");
    assert!(page_words(&mut u).contains("Menu button"), "{}", page_words(&mut u));
}

/// Every text of the page has a German and a Spanish line (never English left over), and the answers name the rows and
/// buttons the way the app does: the Settings rows they point at exist and the other emulator apps' own menus are named.
#[test]
fn android_help_is_in_german_and_spanish_and_points_at_rows_that_exist() {
    use pocket_ui::i18n::{tr_in, Lang};
    let mut texts: Vec<&str> = vec![ui::help_page::ABOUT, "Help"];
    for (title, answer) in ui::help_page::TOPICS {
        texts.push(title);
        texts.push(answer);
    }
    for text in &texts {
        for lang in [Lang::De, Lang::Es] {
            assert_ne!(tr_in(lang, text), *text, "{lang:?}: {text}");
        }
    }
    // The Settings rows the answers send you to.
    let mut u = page();
    u.navigate(ui::Screen::Settings);
    let rows = sheet_rows(&mut u);
    let _ = rows;
    let settings: Vec<String> = {
        let mut seen = Vec::new();
        for _ in 0..40 {
            paint(&mut u, 960, 1600);
            for r in u.regions.iter().filter(|r| matches!(r.action, ui::Action::Row(_))) {
                if !seen.contains(&r.label) {
                    seen.push(r.label.clone());
                }
            }
            u.press(pocket_ui::pad::Button::Down);
        }
        seen
    };
    for named in ["Choose your ROM folder", "Cozy extras", "Plays with", "Save backup", "Report a problem", "Game check"] {
        assert!(settings.iter().any(|r| r == named || r.starts_with(&format!("{named} ·"))), "Help names the row {named} of Android's Settings, which is not there: {settings:?}");
    }
    let all: String = ui::help_page::TOPICS.iter().map(|(_, a)| *a).collect::<Vec<_>>().join(" ");
    for named in ["Choose your ROM folder", "Cozy extras", "Download now", "Plays with", "Save backup", "Report a problem",
                  "ARMSX2", "Dolphin", "Azahar", "hakuX", "Close Game", "Exit Emulation", "Save & quit"] {
        assert!(all.contains(named), "an answer should name {named}");
    }
}

/// The page on the Retroid screens (Mini V2 1240x1080, RP5/Flip 2/Pocket 6 1920x1080, 4:3 1280x960) and the Odin 2 family
/// (1920x1080 at 2.625 and 2.0 dp) at two text sizes in English, German and Spanish: the list and every answer whole, nothing cut
/// with "…", nothing running off or into other words, nothing left in English in German and Spanish, and Settings' own Help
/// row whole.
#[test]
fn android_help_fits_retroid_and_odin_screens_whole_in_english_german_and_spanish() {
    use pocket_ui::i18n::{self, Lang};
    if std::env::var_os("ANDROID_HELP_FIT_CHILD").is_none() {
        let ink = tempfile::tempdir().unwrap();
        let result = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "android_02_tests::android_help_fits_retroid_and_odin_screens_whole_in_english_german_and_spanish", "--nocapture"])
            .env("ANDROID_HELP_FIT_CHILD", "1").env("POCKETUI_TEXT_BOXES", ink.path().join("ink")).env("POCKETUI_VISIBLE_TEXT", ink.path().join("visible")).output().unwrap();
        assert!(result.status.success() && String::from_utf8_lossy(&result.stdout).contains("1 passed; 0 failed"), "{}\n{}", String::from_utf8_lossy(&result.stdout), String::from_utf8_lossy(&result.stderr));
        return;
    }
    let mut faults = Vec::new();
    for lang in [Lang::En, Lang::De, Lang::Es] {
        i18n::set(lang);
        for (w, h, dp) in [(1240, 1080, 2.0), (1240, 1080, 2.5), (1920, 1080, 2.5), (1920, 1080, 3.0), (1280, 960, 2.0), (1920, 1080, 2.625), (1920, 1080, 2.0)] {
            for scale in [1.0f32, 1.3] {
                for open in std::iter::once(None).chain((0..ui::help_page::TOPICS.len()).map(Some)) {
                    let mut u = page();
                    u.prefs.choose_language(lang);
                    u.dp = dp; u.system_font_scale = scale; u.controller_connected = true; u.touch_mode = false;
                    u.navigate(ui::Screen::Settings);
                    choose(&mut u, &ui::tr("Help"));
                    assert!(u.screen == ui::Screen::Help, "{lang:?} {w}x{h}: the page did not open");
                    u.help.open = open;
                    paint(&mut u, w, h);
                    pocket_ui::text::boxes::reset();
                    let frame = paint(&mut u, w, h);
                    pocket_ui::text::boxes::finish(&frame, w as i32, h as i32);
                    let ink = std::fs::read_to_string(std::env::var_os("POCKETUI_TEXT_BOXES").unwrap()).unwrap_or_default();
                    let visible = std::fs::read_to_string(std::env::var_os("POCKETUI_VISIBLE_TEXT").unwrap()).unwrap_or_default();
                    let context = format!("{lang:?} {w}x{h} dp={dp} font={scale} topic={open:?}");
                    let cut = visible.lines().filter(|l| l.contains('…') || l.ends_with("..."));
                    let ink = ink.lines().filter(|l| l.starts_with(['E', 'O', 'V']));
                    faults.extend(cut.chain(ink).map(|l| (context.clone(), l.to_string())));
                    if !u.cut.is_empty() {
                        faults.push((context.clone(), format!("shown cut: {:?}", u.cut)));
                    }
                    let words = u.regions.iter().filter(|r| matches!(r.action, ui::Action::Label | ui::Action::Row(_))).map(|r| r.label.clone()).collect::<Vec<_>>().join(" ");
                    if lang != Lang::En {
                        for (title, answer) in ui::help_page::TOPICS {
                            if words.contains(title) || words.contains(answer) {
                                faults.push((context.clone(), format!("said in English: {title}")));
                            }
                        }
                    }
                    // Settings' own row is whole, with the D-pad on it.
                    u.back();
                    dpad_to(&mut u, &ui::tr("Help"));
                    paint(&mut u, w, h);
                    if u.cut.contains(&ui::tr("Help")) {
                        faults.push((context.clone(), "the Settings row is cut".into()));
                    }
                }
            }
        }
    }
    i18n::set(Lang::En);
    assert!(faults.is_empty(), "{} faults:\n{}", faults.len(), faults.iter().take(40).map(|(c, f)| format!("{c}: {f}")).collect::<Vec<_>>().join("\n"));
}

fn licence_list() -> &'static str {
    "mgba\tgb,gbc,gba\tMPL-2.0\tgithub.com/libretro/mgba\t7a12d6d4b9acb14c0ae62c9166b6a2f3d08007f6\tapk\t\n\
     ppsspp\tpsp\tGPL-2.0-or-later\tgithub.com/hrydgard/ppsspp\tfa50bb1976065c4f8b1b47af227d367fe9771555\tdownload\tppsspp-no-xbrz.patch,ppsspp-pause-drain.patch\n"
}

#[test]
fn settings_licences_lists_every_core_with_licence_source_and_patches_and_goes_back_with_b() {
    use pocket_ui::pad::Button;
    let mut u = page();
    u.licences.load(licence_list());
    u.navigate(ui::Screen::Settings);
    dpad_to(&mut u, "Licences"); // a Settings row opens the page, reachable with the D-pad
    assert!(matches!(choose(&mut u, "Licences"), ui::Command::None));
    assert!(u.screen == ui::Screen::Licences);
    assert_eq!(sheet_rows(&mut u), ["pocket-ui · GPL-2.0-or-later", "mgba · MPL-2.0", "ppsspp · GPL-2.0-or-later", "Back"]);
    assert!(page_words(&mut u).contains("free software"), "the page says what it is before anything is chosen");
    // The optional download is listed like the one in the APK, with its upstream commit and both patches.
    dpad_to(&mut u, "ppsspp");
    u.press(Button::East);
    let words = page_words(&mut u);
    for needle in ["ppsspp · psp", "GPL-2.0-or-later", "github.com/hrydgard/ppsspp", "fa50bb197606", "ppsspp-no-xbrz.patch, ppsspp-pause-drain.patch", "Optional download"] {
        assert!(words.contains(needle), "{needle}: {words}");
    }
    dpad_to(&mut u, "mgba");
    u.press(Button::East);
    let words = page_words(&mut u);
    assert!(words.contains("Patches: None") && words.contains("In the app") && words.contains("MPL-2.0"), "{words}");
    // Translated labels (checked without switching the process-wide language under parallel tests).
    for text in [ui::licences_page::ABOUT, ui::licences_page::OFFER, ui::licences_page::UNREADABLE, "In the app", "Optional download"] {
        for lang in [pocket_ui::i18n::Lang::De, pocket_ui::i18n::Lang::Es] {
            assert_ne!(pocket_ui::i18n::tr_in(lang, text), text, "{lang:?}: {text}");
        }
    }
    u.press(Button::South);
    assert!(u.screen == ui::Screen::Settings, "B goes back one step");
    // Every row of the page is reachable with the D-pad alone.
    u.navigate(ui::Screen::Licences);
    for row in ["pocket-ui", "mgba", "ppsspp", "Back"] { dpad_to(&mut u, row); }
}

#[test]
fn settings_licences_without_its_list_says_so_instead_of_showing_an_empty_sheet() {
    let mut u = page();
    u.licences.load("");
    u.navigate(ui::Screen::Licences);
    let words = page_words(&mut u);
    assert!(words.contains("could not be read"), "{words}");
    assert_eq!(sheet_rows(&mut u), ["pocket-ui · GPL-2.0-or-later", "Back"], "pocket-ui's own licence is always there");
    // The real build's list parses: every line of a CORES.tsv has seven columns, the last may be empty.
    let mut u = page();
    u.licences.load(licence_list());
    assert!(u.licences.read && u.licences.cores.len() == 2 && u.licences.cores[1].patches.len() == 2 && u.licences.cores[1].download);
}

/// The status corner in the header (the same time and battery as on the handhelds): in every Retroid format, at the
/// phone's font sizes and in three languages it never touches the title, the note or the shelf buttons, and where it
/// has no room it is gone, not cut.
#[test]
fn the_status_corner_fits_every_screen_and_gives_way_instead_of_cutting() {
    use pocket_ui::{document_library::{scan, Document}, i18n::Lang, phone_corner::Reading, settings::Theme};
    let games: Vec<Document> = (0..6)
        .map(|i| Document { root_name: String::new(), uri: format!("content://qa/g{i}"), authority: "qa".into(), id: format!("g{i}"),
            relative: ["gba/", &i.to_string(), ".gba"].concat(), sheet: String::new(), size: 4_194_304 })
        .collect();
    let screens = [ui::Screen::Library, ui::Screen::Favorites, ui::Screen::Search, ui::Screen::Settings, ui::Screen::Curator, ui::Screen::Quick];
    // The formats of the Retroid family and a phone, upright and on its side; dp as a handheld at that size has it.
    let formats = [(1240u32, 1080u32, 2.0f32), (1920, 1080, 2.3), (1280, 960, 2.0), (720, 1600, 2.0), (1600, 720, 2.0)];
    let readings = ["0,23,59,5,1", "1,9,5,-1,0"];
    let mut shown = 0;
    let mut dropped = 0;
    for lang in [Lang::En, Lang::De] {
        for theme in Theme::PHONE {
            for &(w, h, dp) in &formats {
                for scale in [1.0f32, 2.0] {
                    for line in readings {
                        for (which, screen) in screens.into_iter().enumerate() {
                            let mut u = page();
                            u.prefs.choose_language(lang);
                            u.prefs.theme = theme;
                            u.install(ui::library(scan(games.clone())));
                            u.dp = dp;
                            u.system_font_scale = scale;
                            u.touch_mode = false;
                            u.corner = Reading::parse(line);
                            u.navigate(screen);
                            let mut with = vec![0; (w * h) as usize];
                            u.paint(&mut Canvas::new(&mut with, w, h), &mut Text::builtin(), None);
                            let who = format!("{lang:?} {theme:?} {w}x{h} dp{dp} font x{scale} {line} screen #{which}");
                            let rect = |name: &str| u.header_rects.iter().find(|r| r.0 == name).map(|r| (r.1, r.2));
                            let title = rect("title").expect("a title");
                            let (margin, header_h) = rect("margin").unwrap();
                            let Some((cl, cr)) = rect("corner") else {
                                dropped += 1;
                                continue;
                            };
                            shown += 1;
                            assert!(cl >= margin && cr <= w as i32 - margin, "{who}: the corner leaves the page: {cl}..{cr}");
                            // Left to right: title, note, corner; none over another.
                            assert!(title.1 <= cl, "{who}: the title runs under the corner: {title:?} {cl}");
                            if let Some((nl, nr)) = rect("note") {
                                assert!(nr <= cl && title.1 <= nl, "{who}: the note meets the corner or the title: {title:?} {nl}..{nr} {cl}");
                            }
                            // The shelf buttons of the landscape shelf stay clear of it.
                            for r in u.regions.iter().filter(|r| r.label == ui::tr("Shelf above") || r.label == ui::tr("Shelf below")) {
                                assert!(r.area.x + r.area.w <= cl, "{who}: the shelf button {:?} meets the corner {cl}", r.label);
                            }
                            // It was painted: more pixels of the header's row differ from the page than without it.
                            let mut hidden = u.corner;
                            hidden.clock = None;
                            hidden.battery = None;
                            u.corner = hidden;
                            let mut without = vec![0; (w * h) as usize];
                            u.paint(&mut Canvas::new(&mut without, w, h), &mut Text::builtin(), None);
                            // The corner never costs the title a letter: it is the same with and without it.
                            assert_eq!(u.header_rects.iter().find(|r| r.0 == "title").map(|r| (r.1, r.2)), Some(title), "{who}: the corner cut the title");
                            let changed = (0..header_h as usize)
                                .flat_map(|y| (cl.max(0) as usize..cr as usize).map(move |x| y * w as usize + x))
                                .filter(|&i| with[i] != without[i])
                                .count();
                            assert!(changed > 20, "{who}: the corner's room is empty ({changed} pixels)");
                            // Only the header's row changed (the note beside the corner moves over, the title may be shorter; at twice the font size its glyphs reach half a row lower): the page below is the same.
                            let stray = (0..with.len()).filter(|&i| {
                                let (x, y) = ((i % w as usize) as i32, (i / w as usize) as i32);
                                with[i] != without[i] && !(y < header_h * 3 / 2 && x >= margin)
                            }).count();
                            assert_eq!(stray, 0, "{who}: the corner changed pixels below the header");
                        }
                    }
                }
            }
        }
    }
    assert!(shown > 200, "the corner shows in most cases ({shown} shown, {dropped} given way)");
    assert!(dropped > 0, "at the largest font on the narrow phone there is no room, and the corner is gone then");
}

/// Missing data and Android's own status bar: the header is exactly what it was, no gap, no half corner.
#[test]
fn the_status_corner_leaves_no_trace_without_facts_or_beside_androids_status_bar() {
    use pocket_ui::phone_corner::Reading;
    for (w, h) in [(1240u32, 1080u32), (720, 1600)] {
        let render = |reading: Reading, system_bar: bool| {
            let mut u = page();
            u.dp = 2.0;
            u.corner = reading;
            u.system_bar = system_bar;
            u.navigate(ui::Screen::Settings);
            paint(&mut u, w, h)
        };
        let nothing = render(Reading::default(), false);
        assert_eq!(render(Reading::parse("1,13,28,87,1"), true), nothing, "{w}x{h}: Android's status bar shows: no second clock");
        assert_eq!(render(Reading::parse("garbage"), false), nothing, "{w}x{h}: unreadable facts show nothing");
        assert_eq!(render(Reading::parse("1,25,99,500,1"), false), nothing, "{w}x{h}: impossible figures show nothing");
        assert_ne!(render(Reading::parse("1,13,28,87,1"), false), nothing, "{w}x{h}: and with facts it shows");
        // No battery (a device without one): the time alone is still there.
        assert_ne!(render(Reading::parse("1,13,28,-1,0"), false), nothing, "{w}x{h}: no battery, the time still shows");
    }
}

// ---- Settings → Plays with: the emulator app each system starts in ----

// Test fixtures: game names and the apps a phone might have, never painted by the app.
#[cfg(test)]
fn plays_games() -> ui::Library {
    use pocket_ui::document_library::{scan, Document};
    let names = ["n64/Test.z64", "psp/Test.iso", "gba/Test.gba", "nds/Test.nds", "ps2/Test.iso"];
    ui::library(scan(names.iter().enumerate().map(|(i, r)| Document { root_name: String::new(), uri: format!("content://qa/{i}"),
        authority: "qa".into(), id: i.to_string(), relative: r.to_string(), sheet: String::new(), size: 100_000 }).collect()))
}
#[cfg(test)]
fn phone_apps(which: &[&str]) -> String {
    let all = serde_json::json!({
        "psp": [{"p": "org.ppsspp.ppsspp", "l": "PPSSPP"}, {"p": "org.ppsspp.ppssppgold", "l": "PPSSPP Gold (supporter edition)"}],
        "retroarch:mupen64plus_next": [{"p": "com.retroarch.aarch64", "l": "RetroArch"}],
        "retroarch:mgba": [{"p": "com.retroarch.aarch64", "l": "RetroArch"}],
        "retroarch:melonds": [{"p": "com.retroarch.aarch64", "l": "RetroArch"}],
        // The PS2 profile has no player of ours: it is never offered, whatever is installed.
        "ps2": [{"p": "com.armsx2", "l": "ARMSX2"}],
    });
    let mut map = serde_json::Map::new();
    for key in which { map.insert(key.to_string(), all[*key].clone()); }
    serde_json::Value::Object(map).to_string()
}
#[cfg(test)]
fn plays_page(apps: &[&str]) -> ui::Ui {
    let mut u = page();
    u.install(plays_games());
    u.set_installed_apps(&phone_apps(apps));
    u.navigate(ui::Screen::Settings);
    u
}

#[cfg(test)]
#[test]
fn plays_with_offers_only_what_is_installed_and_is_reached_with_the_d_pad() {
    use pocket_ui::pad::Button;
    // A normal phone: no emulator app at all. The row is there, the page says why there is nothing to choose.
    let mut u = plays_page(&[]);
    assert!(matches!(choose(&mut u, "Plays with"), ui::Command::None));
    assert!(u.screen == ui::Screen::PlaysWith);
    assert_eq!(sheet_rows(&mut u), ["Back"]);
    assert!(page_words(&mut u).contains("Nothing to choose yet"), "{}", page_words(&mut u));
    u.press(Button::South);
    assert!(u.screen == ui::Screen::Settings);
    paint(&mut u, 960, 1600);
    let on = u.regions.iter().find(|r| matches!(r.action, ui::Action::Row(_)) && u.selected_region(r)).map(|r| r.label.clone());
    assert_eq!(on.as_deref(), Some("Plays with"), "B goes back onto the row it came from");
    // Only PPSSPP is there: PSP is offered; the systems without an app, and PS2 (no player of ours), are not.
    let mut u = plays_page(&["psp", "ps2"]);
    choose(&mut u, "Plays with");
    assert_eq!(sheet_rows(&mut u), ["PlayStation Portable · pocket-ui", "Back"]);
    assert!(page_words(&mut u).contains("Continue and the pause menu are then that app's own"), "{}", page_words(&mut u));
    assert!(page_words(&mut u).contains("Experimental in pocket-ui's player: PlayStation Portable"), "our player's word stays visible on the page: {}", page_words(&mut u));
    // RetroArch as well: its systems join, each row reachable with the D-pad, the Back row last.
    let mut u = plays_page(&["psp", "retroarch:mupen64plus_next", "retroarch:mgba", "retroarch:melonds"]);
    choose(&mut u, "Plays with");
    assert_eq!(sheet_rows(&mut u), ["Game Boy Advance · pocket-ui", "Nintendo 64 · pocket-ui", "Nintendo DS · pocket-ui", "PlayStation Portable · pocket-ui", "Back"]);
    for row in ["Game Boy Advance", "Nintendo 64", "Nintendo DS", "PlayStation Portable", "Back"] {
        dpad_to(&mut u, row);
    }
    // The Back row and Android's Back both leave onto the Settings row.
    u.press(Button::East);
    assert!(u.screen == ui::Screen::Settings);
    // The phone's apps changed while the page was open (installed from the store and back): the rows follow.
    choose(&mut u, "Plays with");
    u.set_installed_apps(&phone_apps(&[]));
    assert_eq!(sheet_rows(&mut u), ["Back"]);
}

#[cfg(test)]
#[test]
fn the_choice_is_saved_and_a_game_then_plays_with_that_app() {
    use pocket_ui::pad::Button;
    let mut u = plays_page(&["psp", "retroarch:mgba"]);
    choose(&mut u, "Plays with");
    let psp = |u: &ui::Ui| u.library.as_ref().unwrap().games.iter().find(|g| g.system_id == "psp").unwrap().clone();
    let gba = |u: &ui::Ui| u.library.as_ref().unwrap().games.iter().find(|g| g.system_id == "gba").unwrap().clone();
    assert!(u.plays_in_app(&psp(&u)).is_none(), "nothing chosen: our own player, as before");
    // A: the first app. The preference is kept (Persist), and the game is now another app's.
    dpad_to(&mut u, "PlayStation Portable");
    assert!(matches!(u.press(Button::East), ui::Command::Persist));
    assert_eq!(u.prefs.plays_with.get("psp").map(String::as_str), Some("org.ppsspp.ppsspp"));
    assert_eq!(u.plays_in_app(&psp(&u)).map(|a| a.label.as_str()), Some("PPSSPP"));
    assert!(u.plays_in_app(&gba(&u)).is_none(), "another system is not touched");
    assert!(sheet_rows(&mut u).contains(&"PlayStation Portable · PPSSPP".to_string()), "{:?}", sheet_rows(&mut u));
    // It survives a restart (launcher.json), and a file with something odd in the field still opens.
    let kept = serde_json::to_string(&u.prefs).unwrap();
    let back: ui::Preferences = serde_json::from_str(&kept).unwrap();
    assert_eq!(back.plays_with.get("psp").map(String::as_str), Some("org.ppsspp.ppsspp"));
    let odd: ui::Preferences = serde_json::from_str(r#"{"plays_with": 5, "light": true, "onboarding_done": true}"#).unwrap();
    assert!(odd.plays_with.is_empty() && odd.light, "a wrong value is the start value, the rest of the file stays");
    // A again: the second app, then round to our own player; Continue is ours again then.
    u.press(Button::East);
    assert_eq!(u.prefs.plays_with.get("psp").map(String::as_str), Some("org.ppsspp.ppssppgold"));
    u.press(Button::East);
    assert!(u.prefs.plays_with.get("psp").is_none() && u.plays_in_app(&psp(&u)).is_none());
    // The app is gone from the phone while chosen: the game plays with our own player, and the row says so.
    u.press(Button::East);
    u.set_installed_apps(&phone_apps(&[]));
    assert!(u.plays_in_app(&psp(&u)).is_none());
    // ... and it is the choice again when the app comes back.
    u.set_installed_apps(&phone_apps(&["psp"]));
    assert_eq!(u.plays_in_app(&psp(&u)).map(|a| a.label.as_str()), Some("PPSSPP"));
}

#[cfg(test)]
#[test]
fn experimental_belongs_to_our_player_and_goes_when_another_app_starts_the_game() {
    use pocket_ui::{pad::Button, settings::Theme};
    for theme in Theme::PHONE {
        for (w, h) in [(960, 540), (540, 960)] {
            // Only the PSP game on the shelf: other systems that are "Experimental" would be on the page too.
            let mut u = page();
            u.install(ui::library(pocket_ui::document_library::scan(vec![pocket_ui::document_library::Document { root_name: String::new(),
                uri: "content://qa/psp".into(), authority: "qa".into(), id: "psp".into(), relative: "psp/Test.iso".into(), sheet: String::new(), size: 100_000 }])));
            u.set_installed_apps(&phone_apps(&["psp"]));
            u.prefs.theme = theme;
            u.navigate(ui::Screen::Library);
            let words = |u: &mut ui::Ui| {
                paint(u, w, h);
                u.regions.iter().map(|r| r.label.as_str()).collect::<Vec<_>>().join(" ")
            };
            let psp = u.library.as_ref().unwrap().games.iter().position(|g| g.system_id == "psp").unwrap();
            u.selected = u.visible.iter().position(|i| *i == psp).unwrap();
            assert!(words(&mut u).contains("Experimental"), "{theme:?} {w}x{h}: our player says it");
            u.prefs.plays_with.insert("psp".into(), "org.ppsspp.ppsspp".into());
            u.cover_changed();
            assert!(!words(&mut u).contains("Experimental"), "{theme:?} {w}x{h}: PPSSPP starts it, the word is about our player");
            // The case's Controls page describes our player's buttons: not offered for another app's game.
            u.press(Button::North);
            assert!(!words(&mut u).contains("Controls"), "{theme:?} {w}x{h}: controls card for an external game");
            u.prefs.plays_with.remove("psp");
            u.cover_changed();
            assert!(words(&mut u).contains("Experimental") && words(&mut u).contains("Controls"), "{theme:?} {w}x{h}: back on our player");
        }
    }
}

/// The page on a phone and on the Retroid screens (Mini V2 1240x1080, RP5/Flip 2/Pocket 6 1920x1080, a 4:3 1280x960), at
/// two densities and two font sizes, English, German and Spanish: nothing cut short with "…", nothing running off its room.
#[cfg(test)]
#[test]
fn plays_with_fits_phone_and_retroid_screens_in_english_german_and_spanish() {
    use pocket_ui::i18n::{self, Lang};
    if std::env::var_os("ANDROID_PLAYS_FIT_CHILD").is_none() {
        let ink = tempfile::tempdir().unwrap();
        let result = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "android_02_tests::plays_with_fits_phone_and_retroid_screens_in_english_german_and_spanish", "--nocapture"])
            .env("ANDROID_PLAYS_FIT_CHILD", "1").env("POCKETUI_TEXT_BOXES", ink.path().join("ink"))
            .env("POCKETUI_VISIBLE_TEXT", ink.path().join("visible")).output().unwrap();
        assert!(result.status.success() && String::from_utf8_lossy(&result.stdout).contains("1 passed; 0 failed"), "{}\n{}", String::from_utf8_lossy(&result.stdout), String::from_utf8_lossy(&result.stderr));
        return;
    }
    let mut faults = Vec::new();
    for lang in [Lang::En, Lang::De, Lang::Es] {
        i18n::set(lang);
        for (w, h, dp) in [(720, 1600, 2.0), (1600, 720, 2.0), (1240, 1080, 2.0), (1240, 1080, 2.5), (1920, 1080, 2.5), (1920, 1080, 3.0), (1280, 960, 2.0)] {
            for scale in [1.0f32, 1.3] {
                // Nothing to choose, one system, every system with the longest app name chosen.
                for (state, apps) in [("none", &[][..]), ("one", &["psp"][..]), ("all", &["psp", "retroarch:mupen64plus_next", "retroarch:mgba", "retroarch:melonds"][..])] {
                    let mut u = plays_page(apps);
                    u.prefs.choose_language(lang);
                    u.dp = dp; u.system_font_scale = scale; u.controller_connected = true;
                    u.prefs.plays_with.insert("psp".into(), "org.ppsspp.ppssppgold".into());
                    u.navigate(ui::Screen::PlaysWith);
                    paint(&mut u, w, h);
                    pocket_ui::text::boxes::reset();
                    let frame = paint(&mut u, w, h);
                    pocket_ui::text::boxes::finish(&frame, w as i32, h as i32);
                    let ink = std::fs::read_to_string(std::env::var_os("POCKETUI_TEXT_BOXES").unwrap()).unwrap_or_default();
                    let visible = std::fs::read_to_string(std::env::var_os("POCKETUI_VISIBLE_TEXT").unwrap()).unwrap_or_default();
                    let context = format!("{lang:?} {w}x{h} dp={dp} font={scale} {state}");
                    let cut = visible.lines().filter(|l| l.contains('…') || l.ends_with("..."));
                    let ink = ink.lines().filter(|l| l.starts_with(['E', 'O', 'V']));
                    faults.extend(cut.chain(ink).map(|l| (context.clone(), l.to_string())));
                    assert!(u.cut.is_empty(), "{context}: words shown cut: {:?}", u.cut);
                    // The words are in the language, and the Settings row that leads here is whole too.
                    let words = u.regions.iter().filter(|r| matches!(r.action, ui::Action::Label)).map(|r| r.label.clone()).collect::<Vec<_>>().join(" ");
                    if lang != Lang::En {
                        for english in ["Another app", "Nothing to choose", "emulator app", "pause menu", "Experimental in"] {
                            assert!(!words.contains(english), "{context}: said in English: {words}");
                        }
                    }
                    u.navigate(ui::Screen::Settings);
                    dpad_to(&mut u, &ui::tr("Plays with"));
                    paint(&mut u, w, h);
                    assert!(!u.cut.contains(&ui::tr("Plays with")), "{context}: the Settings row is cut");
                }
            }
        }
    }
    i18n::set(Lang::En);
    assert!(faults.is_empty(), "words cut short (…) or running off or into other words: {faults:#?}");
}

#[cfg(test)]
#[test]
fn the_start_hands_the_chosen_app_to_android_with_the_same_number_of_words_on_both_sides() {
    // A launchGame call that Java does not have is a NoSuchMethodError at the first Play: the JNI signature is built from the
    // number of strings, so the Rust call and the Java method must agree.
    let java = include_str!("../android/java/xyz/pocketui/app/PocketActivity.java");
    let rust = include_str!("../android/src/lib.rs");
    let java_args = |name: &str| {
        let at = java.find(&format!("public String {name}(")).unwrap_or_else(|| panic!("PocketActivity has no {name}"));
        let list = java[at..].split(')').next().unwrap().split('(').nth(1).unwrap();
        list.split(',').filter(|a| a.trim().starts_with("String ")).count()
    };
    let rust_args = |call: &str| {
        let at = rust.find(call).unwrap_or_else(|| panic!("lib.rs never calls {call}"));
        rust[at..].split("&[").nth(1).unwrap().split("])").next().unwrap().split(',').count()
    };
    assert_eq!(rust_args("\"launchGame\""), java_args("launchGame"));
    assert_eq!(rust_args("bridge(\"installedApps\""), java_args("installedApps"));
    assert!(rust.contains("plays_in_app(&game)"), "the start must pass the chosen app");
}

// ---- RetroAchievements (Settings → RetroAchievements, the golden sticker) -------------------------------------------

fn ra_account() -> pocket_ui::achievements::account::Account {
    pocket_ui::achievements::account::Account { username: "Ravi".into(), token: "Tk".into(), display_name: String::new() }
}

#[test]
fn android_retroachievements_is_reached_with_the_d_pad_and_b_goes_back_one_step() {
    use pocket_ui::pad::Button;
    let mut u = page();
    u.navigate(ui::Screen::Settings);
    assert!(matches!(choose(&mut u, "RetroAchievements"), ui::Command::None));
    assert!(u.screen == ui::Screen::Achievements);
    assert_eq!(sheet_rows(&mut u), ["Sign in", "Back"]);
    for row in ["Sign in", "Back"] {
        dpad_to(&mut u, row);
    }
    u.press(Button::South);
    assert!(u.screen == ui::Screen::Settings);
    paint(&mut u, 960, 1600);
    let on = u.regions.iter().find(|r| matches!(r.action, ui::Action::Row(_)) && u.selected_region(r)).map(|r| r.label.clone());
    assert_eq!(on.as_deref(), Some("RetroAchievements"), "B lands on the row it came from");
    u.press(Button::East);
    assert!(matches!(choose(&mut u, "Back"), ui::Command::None));
    assert!(u.screen == ui::Screen::Settings);
    u.press(Button::East);
    u.back();
    assert!(u.screen == ui::Screen::Settings);
    u.press(Button::East);
    u.press(Button::Start);
    assert!(u.screen == ui::Screen::Library);
    // A finger gets there too, the Settings tab stays the lit one, and Sign in opens the keyboard.
    u.navigate(ui::Screen::Settings);
    tap(&mut u, "RetroAchievements");
    assert!(u.screen == ui::Screen::Achievements);
    paint(&mut u, 960, 1600);
    assert!(u.regions.iter().any(|r| matches!(r.action, ui::Action::Tab(ui::Screen::Settings))));
    tap(&mut u, "Sign in");
    assert!(u.keyboard.is_some(), "the on-screen keyboard is up");
}

#[test]
fn android_retroachievements_signs_in_once_and_never_shows_the_password_or_keeps_it() {
    use pocket_ui::pad::Button;
    let mut u = page();
    u.navigate(ui::Screen::Settings);
    choose(&mut u, "RetroAchievements");
    let words = page_words(&mut u);
    assert!(words.contains("Not signed in") && words.contains("Earn achievements") && words.contains("never kept"), "{words}");
    // Sign in: the user name on the keyboard (A types, START is done), then the password.
    assert!(matches!(choose(&mut u, "Sign in"), ui::Command::None));
    assert!(u.keyboard.is_some() && page_words(&mut u).contains("User name"), "{}", page_words(&mut u));
    u.press(Button::East);
    u.press(Button::Start);
    let words = page_words(&mut u);
    assert!(words.contains("Password for q"), "{words}");
    for _ in 0..3 {
        u.press(Button::East);
    }
    // What a screen reader hears of the password field is how many characters, never which.
    let words = page_words(&mut u);
    assert!(words.contains("Password for q: 3") && !words.contains("qqq") && !words.contains('•'), "{words}");
    match u.press(Button::Start) {
        ui::Command::RaLogin(name, password) => assert_eq!((name.as_str(), password.as_str()), ("q", "qqq")),
        _ => panic!("START on the password asks the host to sign in"),
    }
    assert!(u.keyboard.is_none());
    assert!(page_words(&mut u).contains("Signing in"), "{}", page_words(&mut u));
    // The server said no: the page says why in the server's words and the next try has the name filled in.
    u.achievements_login_done(Err("Invalid user/password combination. Please try again.".into()));
    let words = page_words(&mut u);
    assert!(words.contains("Invalid user/password combination") && !words.contains("Signed in as"), "{words}");
    assert_eq!(sheet_rows(&mut u), ["Sign in", "Back"]);
    choose(&mut u, "Sign in");
    assert_eq!(u.keyboard.as_ref().map(|k| k.text.as_str()), Some("q"), "the name typed last is offered again");
    u.press(Button::South);
    u.press(Button::South);
    assert!(u.keyboard.is_none() && u.screen == ui::Screen::Achievements, "B steps back through the keyboards, then stops");
    // It worked: signed in as the site spells the name; Log out asks first, the second A does it.
    // (The host saves the key before it tells the page; opening the page again reads it from there.)
    ra_account().save(&data_dir()).unwrap();
    u.achievements_login_done(Ok(ra_account()));
    assert!(page_words(&mut u).contains("Signed in as Ravi"), "{}", page_words(&mut u));
    assert_eq!(sheet_rows(&mut u), ["Log out · Ravi", "Back"]);
    // Settings' own row names the account (kept in memory: the painter reads no file).
    u.back();
    assert!(sheet_rows(&mut u).iter().any(|r| r == "RetroAchievements · Ravi"), "{:?}", sheet_rows(&mut u));
    choose(&mut u, "RetroAchievements");
    assert!(matches!(choose(&mut u, "Log out"), ui::Command::None));
    assert!(page_words(&mut u).contains("Press A again to log out"), "{}", page_words(&mut u));
    // Any other key keeps the account; A twice logs out.
    u.press(pocket_ui::pad::Button::Down);
    u.press(pocket_ui::pad::Button::Up);
    assert!(!page_words(&mut u).contains("Press A again"), "a key that isn't A takes the question back");
    assert!(matches!(u.press(pocket_ui::pad::Button::East), ui::Command::None));
    assert!(matches!(u.press(pocket_ui::pad::Button::East), ui::Command::RaLogout));
    assert!(pocket_ui::achievements::account::Account::remove(&data_dir()));
    u.achievements_logged_out();
    assert_eq!(sheet_rows(&mut u), ["Sign in", "Back"]);
    assert!(page_words(&mut u).contains("Not signed in"));
    u.back();
    assert!(sheet_rows(&mut u).iter().any(|r| r == "RetroAchievements"), "{:?}", sheet_rows(&mut u));
}

/// Every step of the page on the Retroid screens, at two densities and two text sizes, in English, German and Spanish:
/// nothing cut with "…", nothing running off or into other words, nothing left in English in German and Spanish.
#[test]
fn android_retroachievements_fits_retroid_screens_whole_in_english_german_and_spanish() {
    use pocket_ui::{i18n::{self, Lang}, pad::Button};
    if std::env::var_os("ANDROID_RA_FIT_CHILD").is_none() {
        let ink = tempfile::tempdir().unwrap();
        let result = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "android_02_tests::android_retroachievements_fits_retroid_screens_whole_in_english_german_and_spanish", "--nocapture"])
            .env("ANDROID_RA_FIT_CHILD", "1").env("POCKETUI_TEXT_BOXES", ink.path().join("ink")).env("POCKETUI_VISIBLE_TEXT", ink.path().join("visible")).output().unwrap();
        assert!(result.status.success() && String::from_utf8_lossy(&result.stdout).contains("1 passed; 0 failed"), "{}\n{}", String::from_utf8_lossy(&result.stdout), String::from_utf8_lossy(&result.stderr));
        return;
    }
    let steps = ["signed out", "signing in", "failed", "signed in", "asks to log out", "user name", "password"];
    let mut faults = Vec::new();
    for lang in [Lang::En, Lang::De, Lang::Es] {
        i18n::set(lang);
        for (w, h, dp) in [(1240, 1080, 2.0), (1240, 1080, 2.5), (1920, 1080, 2.5), (1920, 1080, 3.0), (1280, 960, 2.0)] {
            for scale in [1.0f32, 1.3] {
                for step in steps {
                    let mut u = page();
                    u.prefs.choose_language(lang);
                    u.dp = dp; u.system_font_scale = scale; u.controller_connected = true; u.touch_mode = false;
                    u.navigate(ui::Screen::Settings);
                    choose(&mut u, &ui::tr("RetroAchievements"));
                    assert!(u.screen == ui::Screen::Achievements, "{lang:?} {w}x{h}: the page did not open");
                    let long = "Invalid user/password combination. Please try again.".to_string();
                    match step {
                        "signing in" | "password" => {
                            choose(&mut u, &ui::tr("Sign in"));
                            for c in 0..12 { u.press(Button::East); if c == 3 { u.press(Button::Down); } }
                            u.press(Button::Start);
                            if step == "password" { for _ in 0..20 { u.press(Button::East); } } else { u.press(Button::East); u.press(Button::Start); }
                        }
                        "failed" => u.achievements_login_done(Err(long)),
                        "signed in" => u.achievements_login_done(Ok(ra_account())),
                        "asks to log out" => { u.achievements_login_done(Ok(ra_account())); choose(&mut u, &ui::tr("Log out")); }
                        "user name" => { choose(&mut u, &ui::tr("Sign in")); for _ in 0..30 { u.press(Button::East); } }
                        _ => {}
                    }
                    paint(&mut u, w, h);
                    pocket_ui::text::boxes::reset();
                    let frame = paint(&mut u, w, h);
                    pocket_ui::text::boxes::finish(&frame, w as i32, h as i32);
                    let ink = std::fs::read_to_string(std::env::var_os("POCKETUI_TEXT_BOXES").unwrap()).unwrap_or_default();
                    let visible = std::fs::read_to_string(std::env::var_os("POCKETUI_VISIBLE_TEXT").unwrap()).unwrap_or_default();
                    let context = format!("{lang:?} {w}x{h} dp={dp} font={scale} {step}");
                    let cut = visible.lines().filter(|l| l.contains('…') || l.ends_with("..."));
                    let ink = ink.lines().filter(|l| l.starts_with(['E', 'O', 'V']));
                    faults.extend(cut.chain(ink).map(|l| (context.clone(), l.to_string())));
                    if !u.cut.is_empty() {
                        faults.push((context.clone(), format!("shown cut: {:?}", u.cut)));
                    }
                    let words = u.regions.iter().filter(|r| matches!(r.action, ui::Action::Label | ui::Action::Row(_))).map(|r| r.label.clone()).collect::<Vec<_>>().join(" ");
                    if lang != Lang::En {
                        for english in ["Not signed in", "Sign in", "Log out", "Signing in", "Earn achievements", "Your password is sent once", "Press A again", "User name", "Password for", "Sent once"] {
                            if words.contains(english) && !(english == "Sign in" && words.contains("Sign in ·")) {
                                faults.push((context.clone(), format!("said in English ({english}): {words}")));
                            }
                        }
                    }
                }
            }
        }
    }
    i18n::set(Lang::En);
    assert!(faults.is_empty(), "RetroAchievements is cut short, runs off its room or is left in English on a Retroid screen: {faults:#?}");
}

/// A game whose achievements are all unlocked gets the golden "100 %" sticker, and it counts for the trophy on the shelf
/// as a completed game does. The mark the player leaves is used once: a sticker taken off stays off.
#[test]
fn a_mastered_game_gets_the_golden_sticker_and_it_counts_for_the_trophy() {
    let mut u = page();
    u.library = Some(retroid_games());
    u.prefs.extras.decorations = true;
    let game = u.library.as_ref().unwrap().games.iter().find(|g| g.name.contains("Libbet")).unwrap().clone();
    let (_, before) = u.room_sync();
    assert!(!before.as_ref().unwrap().trophy && !before.as_ref().unwrap().hundred);
    assert!(!u.master_game("0000000000000000"), "no game of the shelf has that identity");
    assert!(!u.prefs.stickers_of(&game.key).hundred);
    // The player's identity of a Game Boy game depends on the Game Boy core chosen: either finds it.
    assert!(u.master_game(&ui::game_key(&game, true)));
    assert!(u.prefs.stickers_of(&game.key).hundred, "the golden sticker is on its case");
    assert!(u.prefs.stickers_shown(&game.key).hundred || !u.prefs.extras.case_stickers);
    let (_, after) = u.room_sync();
    let after = after.unwrap();
    assert!(after.trophy && after.hundred, "the trophy and the framed 100 % stand on the shelf: {after:?}");
    // The other stickers of the case stay as they were.
    u.prefs.stickers.get_mut(&game.key).unwrap().completed = true;
    assert!(u.master_game(&ui::game_key(&game, false)));
    assert!(u.prefs.stickers_of(&game.key).completed && u.prefs.stickers_of(&game.key).hundred);
}

#[path = "android_drawer_tests.rs"]
mod drawer;
#[path = "android_phones_tests.rs"]
mod phones;
