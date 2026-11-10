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
        covers: vec![], pictures: vec![], cheats: String::new(),
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

#[test]
fn android_retro_style_and_its_palette_are_reachable_and_remembered() {
    let mut u = page();
    u.navigate(ui::Screen::Settings);
    tap(&mut u, "Style");
    tap(&mut u, "Style");
    tap(&mut u, "Style");
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
        let games = scan(vec![Document {uri: "content://qa/game".into(),id: relative.into(),authority: "qa".into(),relative: relative.into(),sheet: String::new(),size: 100}]);
        assert_eq!(games.len(), 1, "{relative} must reach the shelf");
        assert_eq!(games[0].system_id, system, "{relative}");
    }
}

#[test]
fn android_card_art_prefers_boxes_and_never_crosses_saf_grants_or_game_regions() {
    use pocket_ui::document_library::{Document, scan};
    let doc = |root: &str, path: &str, sheet: &str| Document {uri:format!("content://qa/tree/{root}/document/{path}"),authority:"qa".into(),id:format!("{root}/{path}"),relative:path.into(),sheet:sheet.into(),size:100};
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
fn android_long_selected_names_are_actually_painted_in_all_six_looks() {
    use pocket_ui::{document_library::{scan,Document}, settings::Theme, font_style::FontStyle};
    if std::env::var_os("ANDROID03_TITLE_CHILD").is_none() {
        let dir=tempfile::tempdir().unwrap();
        let result=std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact","android_02_tests::android_long_selected_names_are_actually_painted_in_all_six_looks","--nocapture"])
            .env("ANDROID03_TITLE_CHILD","1").env("POCKETUI_TITLE_BLOCKS",dir.path().join("titles"))
            .env("POCKETUI_TEXT_BOXES",dir.path().join("ink")).output().unwrap();
        assert!(result.status.success() && String::from_utf8_lossy(&result.stdout).contains("1 passed; 0 failed"),"{}\n{}",String::from_utf8_lossy(&result.stdout),String::from_utf8_lossy(&result.stderr));
        return;
    }
    let title="The Legend of Zelda - Ocarina of Time";
    for theme in Theme::PHONE { for (w,h) in [(720,1280),(1280,720)] { for scale in [1.0,2.0] {
        let mut u=page();u.prefs.theme=theme;u.prefs.font=FontStyle::Retro;u.dp=1.5;u.system_font_scale=scale;
        u.install(ui::library(scan(vec![Document {uri:"content://qa/long".into(),authority:"qa".into(),id:"long".into(),relative:format!("n64/{title}.z64"),sheet:String::new(),size:100}])));
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
            for theme in [Theme::Shelf, Theme::Minimal] {
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
                            u.install(ui::library(scan(vec![Document { uri: "content://qa/literal".into(), authority: "qa".into(), id: "literal".into(), relative: format!("gb/{title}.gb"), sheet: String::new(), size: 32768 }])));
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
