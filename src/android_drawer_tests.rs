//! The drawer under the shelf on a host: the real Android controller and painters (`android/src/apps_page.rs`).
//! What a player notices: the apps listed and opened, a marked game on its own Android shelf and started from there,
//! the way in and back with the D-pad and by touch, words that say what is going on, and nothing cut on a Retroid.
use super::{page, paint, retroid_games, ui};
use pocket_ui::pad::Button;
use pocket_ui::settings::Theme;
use ui::apps_page::{self, AndroidApp};

// Test fixtures: app names, never painted by the app's own words (i18n-extract skips `cfg(test)` items).
#[cfg(test)]
fn phone_apps() -> Vec<AndroidApp> {
    let json = r#"[
        {"package":"com.android.vending","label":"Play Store","game":false,"icon":""},
        {"package":"com.android.settings","label":"Settings","game":false,"icon":""},
        {"package":"com.example.valley","label":"Stardew Valley","game":true,"icon":""},
        {"package":"org.example.chess","label":"chess","game":false,"icon":""},
        {"package":"com.android.vending","label":"Play Store","game":false,"icon":""}
    ]"#;
    apps_page::parse(json, |_| None)
}
#[cfg(test)]
fn many_apps(n: usize) -> Vec<AndroidApp> {
    let mut apps = phone_apps();
    apps.extend((0..n).map(|i| AndroidApp { package: format!("org.example.app{i:02}"), label: format!("Example app with a long name {i:02}"), game: i % 3 == 0, icon: None }));
    apps
}

fn labels(u: &ui::Ui) -> Vec<String> {
    u.regions.iter().map(|r| r.label.clone()).collect()
}
fn app_cards(u: &ui::Ui) -> Vec<(usize, String, pocket_ui::touch::Area)> {
    u.regions.iter().filter_map(|r| match r.action { ui::Action::App(i) => Some((i, r.label.clone(), r.area)), _ => None }).collect()
}
fn shelf_titles(u: &ui::Ui) -> Vec<String> {
    u.rows.as_ref().map(|s| (0..s.rows.len()).map(|r| s.title(r).to_string()).collect()).unwrap_or_default()
}
fn tap_label(u: &mut ui::Ui, label: &str, (w, h): (u32, u32)) -> ui::Command {
    paint(u, w, h);
    let a = u.regions.iter().find(|r| r.label == label).map(|r| r.area).unwrap_or_else(|| panic!("no {label}: {:?}", labels(u)));
    u.tap((a.x + a.w / 2) as f32, (a.y + a.h / 2) as f32)
}
fn select_app(u: &mut ui::Ui, name: &str) {
    for _ in 0..64 {
        if u.apps.list.as_ref().unwrap()[u.apps.selected].label == name {
            return;
        }
        u.press(Button::Right);
    }
    panic!("{name} is never reached with Right");
}

#[test]
fn the_drawer_lists_each_app_once_by_name_and_androids_game_category_only_suggests() {
    let apps = phone_apps();
    assert_eq!(apps.iter().map(|a| a.label.as_str()).collect::<Vec<_>>(), ["chess", "Play Store", "Settings", "Stardew Valley"], "by name as a home screen, each once");
    let mut u = page();
    u.prefs.theme = Theme::Shelf;
    u.install(retroid_games());
    u.apps_got(apps);
    assert!(!shelf_titles(&u).iter().any(|t| t == "Android"), "Android calling an app a game puts nothing on a shelf");
    u.press(Button::R2);
    assert!(u.screen == ui::Screen::Apps);
    paint(&mut u, 1920, 1080);
    let cards = app_cards(&u);
    assert_eq!(cards.len(), 4, "every app has its card: {cards:?}");
    assert!(cards.iter().any(|(_, l, _)| l == "Stardew Valley · Looks like a game"), "{cards:?}");
    assert!(cards.iter().any(|(_, l, _)| l == "Play Store"), "{cards:?}");
}

#[test]
fn a_marked_app_stands_on_the_android_shelf_starts_from_there_and_survives_a_restart() {
    let mut u = page();
    u.prefs.theme = Theme::Shelf;
    u.install(retroid_games());
    u.apps_got(phone_apps());
    u.selected = 3;
    let before = u.game().unwrap().key.clone();
    u.press(Button::R2);
    select_app(&mut u, "Stardew Valley");
    assert!(matches!(u.press(Button::West), ui::Command::Persist), "Y marks it and the mark is kept");
    paint(&mut u, 1240, 1080);
    assert!(labels(&u).iter().any(|l| l == "Stardew Valley · On the shelf"), "{:?}", labels(&u));
    u.press(Button::South);
    assert!(u.screen == ui::Screen::Library);
    assert_eq!(u.game().unwrap().key, before, "back on the Games page, on the game it was on");
    assert!(shelf_titles(&u).iter().any(|t| t == "Android"), "its own shelf beside the consoles: {:?}", shelf_titles(&u));
    let pos = u.visible.iter().position(|&i| u.library.as_ref().unwrap().games[i].name == "Stardew Valley").expect("on the shelf");
    u.selected = pos;
    paint(&mut u, 1240, 1080);
    assert!(labels(&u).iter().any(|l| l == "Stardew Valley"), "its case is on the page: {:?}", labels(&u));
    assert!(matches!(u.press(Button::East), ui::Command::Launch));
    assert_eq!(apps_page::package_of(u.game().unwrap()), Some("com.example.valley"), "A starts the app");
    // Back from the app (a restart of the launcher too): the mark is in launcher.json, the shelf is there before Android
    // lists its apps again.
    u.prefs.selected_key = u.game().unwrap().key.clone(); // as the host's persist writes it
    let saved = serde_json::to_string(&u.prefs).unwrap();
    let mut again = ui::Ui::new(serde_json::from_str(&saved).unwrap());
    again.install(retroid_games());
    assert!(again.game().is_some_and(|g| g.name == "Stardew Valley"), "the selection comes back to it");
    // Uninstalled meanwhile: Android's list has it no more, its shelf goes, the mark stays for a new install.
    again.apps_got(phone_apps().into_iter().filter(|a| a.package != "com.example.valley").collect());
    assert!(!again.library.as_ref().unwrap().games.iter().any(|g| g.name == "Stardew Valley"));
    assert!(again.prefs.android_games.contains_key("com.example.valley"));
    // Off the shelf again: Y once more.
    u.press(Button::R2);
    assert!(u.apps.list.as_ref().unwrap()[u.apps.selected].label == "Stardew Valley", "the drawer opens where it was left");
    u.press(Button::West);
    u.press(Button::South);
    assert!(!shelf_titles(&u).iter().any(|t| t == "Android"));
    assert!(!u.library.as_ref().unwrap().games.iter().any(|g| apps_page::package_of(g).is_some()));
}

#[test]
fn the_drawer_is_reached_by_r2_and_by_touch_walks_every_app_and_back_returns_to_the_same_game() {
    for (w, h, dp) in [(1240, 1080, 2.5), (1920, 1080, 3.0), (1280, 960, 2.0), (1080, 1920, 2.6)] {
        let mut u = page();
        u.dp = dp;
        u.touch_mode = false;
        u.install(retroid_games());
        u.apps_got(many_apps(30));
        u.selected = 2;
        let before = u.game().unwrap().key.clone();
        paint(&mut u, w, h);
        u.press(Button::R2);
        // Every card is reached with the D-pad alone and is on the page when selected.
        let n = u.apps.list.as_ref().unwrap().len();
        let mut seen = std::collections::BTreeSet::new();
        for _ in 0..n {
            paint(&mut u, w, h);
            let sel = u.apps.selected;
            assert!(app_cards(&u).iter().any(|(i, _, a)| *i == sel && a.h > 0 && a.y >= 0 && a.y + a.h <= h as i32), "{w}x{h}: app {sel} is on the page when selected");
            seen.insert(sel);
            u.press(Button::Right);
        }
        assert_eq!(seen.len(), n, "{w}x{h}: Right walks every app");
        for _ in 0..n {
            u.press(Button::Up);
        }
        u.press(Button::Left);
        assert_eq!(u.apps.selected, 0, "{w}x{h}: Up and Left come back to the first");
        u.press(Button::Down);
        paint(&mut u, w, h);
        assert!(u.apps.selected > 0 || app_cards(&u).iter().all(|(i, _, _)| *i == 0), "Down goes to the row below");
        u.press(Button::South);
        assert!(u.screen == ui::Screen::Library && u.game().unwrap().key == before, "{w}x{h}: B goes back to the same game");
        // By touch: the Apps button, a card opens its app, a held finger marks it, Android's Back goes back.
        u.touch_mode = true;
        tap_label(&mut u, "Android apps", (w, h));
        assert!(u.screen == ui::Screen::Apps, "{w}x{h}: the Apps button opens the drawer");
        paint(&mut u, w, h);
        let (i, _, a) = app_cards(&u).into_iter().find(|(_, l, _)| l == "Play Store").expect("Play Store's card");
        let (x, y) = ((a.x + a.w / 2) as f32, (a.y + a.h / 2) as f32);
        assert!(matches!(u.tap(x, y), ui::Command::OpenApp(p) if p == "com.android.vending"), "a tap opens the app");
        assert_eq!(u.apps.selected, i);
        assert!(u.screen == ui::Screen::Apps, "{w}x{h}: back from the app, the drawer is where it was");
        assert!(matches!(u.long_press(x, y), ui::Command::Persist) && u.prefs.android_games.contains_key("com.android.vending"), "holding marks it");
        u.back();
        assert!(u.screen == ui::Screen::Library && u.game().unwrap().key == before, "{w}x{h}: Back goes back to the same game");
    }
}

/// Before Android answers, with no apps, with nothing marked, and with no folder chosen: the drawer and the Games page
/// say what is going on and how to go on, never an empty gap.
#[test]
fn the_drawer_and_the_games_page_say_what_is_going_on_when_there_is_nothing() {
    let mut u = page();
    paint(&mut u, 1240, 1080);
    assert!(labels(&u).iter().any(|l| l == "Android apps"), "no folder chosen yet: the drawer is there all the same: {:?}", labels(&u));
    u.press(Button::R2);
    paint(&mut u, 1240, 1080);
    assert!(labels(&u).iter().any(|l| l == "Reading your apps..."), "{:?}", labels(&u));
    u.apps_got(Vec::new());
    paint(&mut u, 1240, 1080);
    assert!(labels(&u).iter().any(|l| l == "No apps found") && labels(&u).iter().any(|l| l.starts_with("Android lists no other apps")), "{:?}", labels(&u));
    u.apps_got(phone_apps());
    paint(&mut u, 1240, 1080);
    assert!(labels(&u).iter().any(|l| l.starts_with("No Android games on the shelf yet. Hold a game")), "touch: say how to mark: {:?}", labels(&u));
    u.touch_mode = false;
    paint(&mut u, 1240, 1080);
    assert!(labels(&u).iter().any(|l| l.starts_with("No Android games on the shelf yet. Press Y")), "controller: say which key: {:?}", labels(&u));
    // Only Android games, no folder of games: they stand on the Games page.
    select_app(&mut u, "Stardew Valley");
    u.press(Button::West);
    u.press(Button::South);
    paint(&mut u, 1240, 1080);
    assert!(u.game().is_some_and(|g| g.name == "Stardew Valley"), "the Android game is on the Games page");
    assert!(labels(&u).iter().any(|l| l == "Stardew Valley"), "{:?}", labels(&u));
}

#[test]
fn app_icons_are_read_whole_and_muted_and_a_broken_file_is_no_icon() {
    let dir = tempfile::tempdir().unwrap();
    let mut bytes = Vec::new();
    bytes.extend(2u32.to_be_bytes());
    bytes.extend(1u32.to_be_bytes());
    bytes.extend(0xffff_0000u32.to_be_bytes());
    bytes.extend(0x0000_0000u32.to_be_bytes());
    let good = dir.path().join("good.argb");
    std::fs::write(&good, &bytes).unwrap();
    let icon = apps_page::read_icon(good.to_str().unwrap()).expect("an icon");
    let (r, g) = ((icon.px[0] >> 16) & 0xff, (icon.px[0] >> 8) & 0xff);
    assert!(r < 0xff && g > 0 && r - g < 0x80, "a red icon is quietened towards grey: {:08x}", icon.px[0]);
    assert_eq!(icon.px[1] >> 24, 0, "what is see-through stays so");
    let short = dir.path().join("short.argb");
    std::fs::write(&short, &bytes[..12]).unwrap();
    assert!(apps_page::read_icon(short.to_str().unwrap()).is_none());
    assert!(apps_page::read_icon(dir.path().join("none").to_str().unwrap()).is_none());
}

/// The drawer, its words and the Games page's Apps button on the Retroid screens (Mini V2 1240x1080, RP5/Flip 2/Pocket 6
/// 1920x1080, a 4:3 1280x960) at two densities and two font sizes, in English, German and Spanish, by touch and by
/// controller: no word of ours cut short with "…", nothing running off its room or into other words.
#[test]
fn the_drawer_fits_retroid_screens_whole_in_english_german_and_spanish() {
    use pocket_ui::i18n::{self, Lang};
    if std::env::var_os("ANDROID_DRAWER_FIT_CHILD").is_none() {
        let ink = tempfile::tempdir().unwrap();
        let result = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "android_02_tests::drawer::the_drawer_fits_retroid_screens_whole_in_english_german_and_spanish", "--nocapture"])
            .env("ANDROID_DRAWER_FIT_CHILD", "1").env("POCKETUI_TEXT_BOXES", ink.path().join("ink"))
            .env("POCKETUI_VISIBLE_TEXT", ink.path().join("visible")).output().unwrap();
        assert!(result.status.success() && String::from_utf8_lossy(&result.stdout).contains("1 passed; 0 failed"), "{}\n{}", String::from_utf8_lossy(&result.stdout), String::from_utf8_lossy(&result.stderr));
        return;
    }
    let apps: Vec<AndroidApp> = phone_apps().into_iter().map(|mut a| {
        a.label = a.label.replace("chess", "Chess");
        a
    }).collect();
    let mut faults = Vec::new();
    for lang in [Lang::En, Lang::De, Lang::Es] {
        i18n::set(lang);
        for (w, h, dp) in [(1240, 1080, 2.0), (1240, 1080, 2.5), (1920, 1080, 2.5), (1920, 1080, 3.0), (1280, 960, 2.0)] {
            for scale in [1.0f32, 1.3] {
                for page_name in ["drawer", "drawer-marked", "drawer-reading", "drawer-none", "shelf", "easy", "split", "nofolder"] {
                    for controller in [false, true] {
                        let mut u = page();
                        u.prefs.choose_language(lang);
                        u.dp = dp;
                        u.system_font_scale = scale;
                        u.touch_mode = !controller;
                        u.controller_connected = controller;
                        u.prefs.theme = match page_name { "easy" => Theme::Big, "split" => Theme::Split, _ => Theme::Shelf };
                        if page_name != "nofolder" {
                            u.install(retroid_games());
                        }
                        match page_name {
                            "drawer-reading" => {}
                            "drawer-none" => u.apps_got(Vec::new()),
                            _ => u.apps_got(apps.clone()),
                        }
                        if page_name.starts_with("drawer") {
                            u.press(Button::R2);
                            u.touch_mode = !controller;
                        }
                        if page_name == "drawer-marked" {
                            u.apps.selected = 3;
                            u.press(Button::West);
                            u.apps.selected = 1;
                            u.touch_mode = !controller;
                        }
                        paint(&mut u, w, h);
                        pocket_ui::text::boxes::reset();
                        let frame = paint(&mut u, w, h);
                        pocket_ui::text::boxes::finish(&frame, w as i32, h as i32);
                        let ink = std::fs::read_to_string(std::env::var_os("POCKETUI_TEXT_BOXES").unwrap()).unwrap_or_default();
                        let visible = std::fs::read_to_string(std::env::var_os("POCKETUI_VISIBLE_TEXT").unwrap()).unwrap_or_default();
                        let context = format!("{lang:?} {w}x{h} dp={dp} font={scale} {page_name} controller={controller}");
                        // "Reading your apps..." ends in dots of its own.
                        let reading = pocket_ui::i18n::tr("Reading your apps...");
                        let cut = visible.lines().filter(|l| !reading.contains(l.trim())).filter(|l| l.contains('…') || l.ends_with("..."));
                        // The shelf pages: only the top row is the drawer's (Apps beside Just Play).
                        let ours = |l: &&str| !matches!(page_name, "shelf" | "easy" | "split") || [i18n::tr("Apps"), i18n::tr("Just Play")].iter().any(|w| w.starts_with(l.trim_end_matches('…')));
                        let cut = cut.filter(ours);
                        let ink = ink.lines().filter(|l| l.starts_with(['E', 'O', 'V']));
                        faults.extend(cut.chain(ink).map(|l| (context.clone(), l.to_string())));
                        faults.extend(u.cut.iter().map(|l| (context.clone(), format!("cut: {l}"))));
                    }
                }
            }
        }
    }
    i18n::set(Lang::En);
    assert!(faults.is_empty(), "words cut short (…) or running off or into other words on a Retroid screen: {faults:#?}");
}
