//! Views with missing or still-loading data (polish 2026-11-04, docs/DESIGN.md "One rule for an empty or loading view"):
//! the real program paints each state and the words on the screen say something understandable, never a blank where
//! data goes and never a placeholder that looks broken. A row whose data is not there yet shows no words at all.
//!
//! Needs python3 for the made-up library and, on Linux without a compositor, scripts/wayland-stub.py (as
//! tests/kids_mode.rs).

use std::path::Path;
use std::process::Command;

include!("common/root.rs");

struct Stub(std::process::Child);

impl Drop for Stub {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

struct Card {
    dir: tempfile::TempDir,
    _stub: Option<Stub>,
}

impl Card {
    fn new() -> Card {
        // A short path: the stub's socket has to fit.
        let dir = tempfile::Builder::new().prefix("pui-empty-").tempdir_in("/tmp").unwrap();
        let status = Command::new("python3").env("POCKETUI_MOCK_NOW", FIXTURE_NOW).args(["-I", &format!("{ROOT}/scripts/curator-fake-library.py"), dir.path().to_str().unwrap(), "dark"]).stdout(std::process::Stdio::null()).status().unwrap();
        assert!(status.success());
        let stub = (cfg!(target_os = "linux") && std::env::var_os("WAYLAND_DISPLAY").is_none_or(|w| w.is_empty())).then(|| {
            let child = Command::new("python3").args(["-I", &format!("{ROOT}/scripts/wayland-stub.py"), dir.path().join("wl").to_str().unwrap()]).spawn().unwrap();
            test_clock::wait_for(|| dir.path().join("wl").exists(), std::time::Duration::from_secs(2), "Wayland stub listening");
            Stub(child)
        });
        Card { dir, _stub: stub }
    }

    /// The words the state's picture says, one per line, at `size` (WxH, "" for the default).
    fn screen(&self, state: &str, size: &str) -> String {
        self.screen_in(state, size, "en", "").0
    }

    /// The same in a language (`en`, `de`, `es`) and a text size (`large`, `medium`, "" for the setting), with where the
    /// painter saw text run into other text or a hint bar (empty when nowhere).
    fn screen_in(&self, state: &str, size: &str, lang: &str, text_size: &str) -> (String, String) {
        self.screen_styled(state, size, lang, text_size, "")
    }

    fn screen_styled(&self, state: &str, size: &str, lang: &str, text_size: &str, font: &str) -> (String, String) {
        let tmp: &Path = self.dir.path();
        let (png, words, boxes) = (tmp.join("shot.png"), tmp.join("words.tsv"), tmp.join("boxes.txt"));
        let _ = std::fs::remove_file(&words);
        let _ = std::fs::remove_file(&boxes);
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_pocket-ui"));
        if tmp.join("wl").exists() {
            cmd.env("WAYLAND_DISPLAY", tmp.join("wl"));
        }
        if !size.is_empty() {
            cmd.env("POCKETUI_SNAPSHOT_SIZE", size);
        }
        if !font.is_empty() {
            cmd.env("POCKETUI_FONT", font);
        }
        if !text_size.is_empty() {
            cmd.env("POCKETUI_TEXT_SIZE", text_size);
        }
        let out = cmd
            .args(["--snapshot", state, png.to_str().unwrap()])
            .env("POCKETUI_MOCK", "1").env("POCKETUI_MOCK_NOW", FIXTURE_NOW)
            .env("POCKETUI_LANG", lang)
            .env("POCKETUI_VISIBLE_TEXT", &words)
            .env("POCKETUI_TEXT_BOXES", &boxes)
            .env("POCKETUI_DATA", tmp.join("data"))
            .env("POCKETUI_ES_SYSTEMS", tmp.join("es_systems.cfg"))
            .output()
            .unwrap();
        assert!(out.status.success() && png.exists(), "{state}: {}", String::from_utf8_lossy(&out.stderr));
        (std::fs::read_to_string(words).unwrap_or_default(), std::fs::read_to_string(boxes).unwrap_or_default())
    }
}

#[test]
fn curator_before_its_index_shows_no_words_on_home_and_a_sentence_on_a_page() {
    let card = Card::new();
    // Home's Curator row: its title, no "Getting ready…" and no empty boxes where covers go.
    let home = card.screen("curator:ready", "");
    assert!(home.contains("Curator") && !home.contains("Getting ready"), "{home}");
    // A page that was opened anyway says what it is doing, in a sentence.
    let lens = card.screen("curator:ready:lens", "");
    assert!(lens.contains("Getting ready") && lens.contains("Curator is reading your library"), "{lens}");
}

#[test]
fn a_game_without_box_art_is_a_title_card_and_a_narrow_tile_names_its_console_not_a_cut_word() {
    let card = Card::new();
    // Home's cover is the day's pick, and the pick changes with the date: read which game it is instead of assuming one.
    let wide = card.screen("curator:home", "");
    let title = wide.lines().find_map(|l| l.strip_prefix("Today: ")).and_then(|l| l.split(" · ").next()).expect(&wide);
    let first_word = title.split_whitespace().next().unwrap();
    let lines: Vec<&str> = wide.lines().collect();
    let at = lines.iter().position(|l| *l == first_word).unwrap_or_else(|| panic!("the title card says {title}: {wide}"));
    let console = lines[at.saturating_sub(1)];
    // The date can pick Wii U, Game Boy or Mega Drive too: a real console name need not be all-caps.
    // Check the selected game's actual console, rather than accepting any short uppercase word.
    let systems = pocket_ui::library::load_shown(&card.dir.path().join("es_systems.cfg"), &card.dir.path().join("data")).unwrap();
    let right_console = systems.iter().filter(|s| s.games.iter().any(|g| g.name == title)).any(|s| {
        let common = match s.name.as_str() {
            "gb" => "Game Boy", "gbc" => "Game Boy Color", "nds" => "DS", "psx" => "PS1", "megadrive" => "Mega Drive", _ => "",
        };
        console == s.fullname || console == s.name.to_uppercase() || (!common.is_empty() && console == common)
    });
    assert!(at > 0 && right_console, "the correct console over {title}: {wide}");
    let narrow = card.screen("curator:home", "1024x768");
    assert!(narrow.lines().any(|l| l == console), "the narrow tile names {console}: {narrow}");
    let first_letters: Vec<String> = (1..=3.min(first_word.chars().count().saturating_sub(1))).map(|n| format!("{}…", first_word.chars().take(n).collect::<String>())).collect();
    assert!(!narrow.lines().any(|l| first_letters.iter().any(|c| l.contains(c.as_str()))), "never a word cut to its first letters: {narrow}");
}

#[test]
fn the_pause_menus_empty_previews_use_one_word_each() {
    let card = Card::new();
    for state in ["slot-save", "slot-load", "slot-one", "slot-undo"] {
        let screen = card.screen(state, "");
        assert!(!screen.contains("Empty slot") && !screen.contains("No preview"), "{state}: {screen}");
    }
    assert!(card.screen("slot-save", "").contains("No picture"));
    assert!(card.screen("slot-earlier", "").contains("Earlier"), "an Earlier state with no picture says the same");
}

#[test]
fn the_tour_explains_six_basics_then_offers_easier_reading() {
    let card = Card::new();
    for (id, words) in [
        ("play", "Play, and HOME is the menu"),
        ("continue", "Continue where you stopped"),
        ("saving", "Save and Load are in the pause menu"),
        ("quick", "Brightness, volume, Wi-Fi"),
        ("add_games", "Games go in each system's folder"),
        ("settings", "Everything on these cards"),
    ] {
        let screen = card.screen(&format!("tour:{id}"), "");
        assert!(screen.contains("The basics in 7 cards") && screen.contains(words), "{id}: {screen}");
        for off in ["On", "Off", "Later", "Now"] {
            assert!(!screen.lines().any(|l| l == off), "{id}: no On/Off/Later rows on a basics card ({off}): {screen}");
        }
    }
    assert!(card.screen("tour:settings", "").lines().any(|l| l == "Next"));
    let reading = card.screen("tour:easy_to_read", "");
    for word in ["Big text", "On", "Off", "Later", "Done"] {
        assert!(reading.lines().any(|l| l == word), "{reading}");
    }
    card.screen("tour:easy_to_read:ua", "");
    let settings = || pocket_ui::settings::Settings::load(&card.dir.path().join("data"));
    assert_eq!(settings().text_size, pocket_ui::settings::TextSize::Large);
    assert_eq!(settings().motion, pocket_ui::settings::Motion::Reduced);
    // A fresh process sees the saved On choice and can turn both settings back off.
    card.screen("tour:easy_to_read:da", "");
    assert_eq!(settings().text_size, pocket_ui::settings::TextSize::Normal);
    assert_eq!(settings().motion, pocket_ui::settings::Motion::Full);
}

#[test]
fn the_reading_cards_path_to_the_settings_is_whole_and_clear_of_the_hint_bar_in_every_language_and_size() {
    let card = Card::new();
    for (lang, path) in [
        ("en", "Settings › Look › Text size, Settings › Sound & feel › Motion"),
        ("de", "Einstellungen › Aussehen › Schriftgröße, Einstellungen › Klang & Gefühl › Bewegung"),
        ("es", "Ajustes › Aspecto › Tamaño de texto, Ajustes › Sonido y tacto › Movimiento"),
    ] {
        for size in ["640x480", "1024x768", "1240x1080"] {
            for text in ["normal", "medium", "large"] {
                let (words, boxes) = card.screen_in("tour:easy_to_read", size, lang, text);
                let one_line = words.split_whitespace().collect::<Vec<_>>().join(" ");
                assert!(one_line.contains(path), "{lang} {size} {text}: the path is cut or broken: {words}");
                assert!(boxes.trim().is_empty(), "{lang} {size} {text}: text runs into other text or the hint bar: {boxes}");
            }
        }
    }
}

#[test]
fn the_hold_to_toggle_row_is_visible_with_its_default_on_every_panel() {
    let card = Card::new();
    let data = card.dir.path().join("data");
    let file = pocket_ui::core_options::CoreOptionsFile {
        core: "Test".into(), rom: "/storage/roms/gbc/uCity.gbc".into(),
        categories: Vec::new(), options: Vec::new(),
        controls: pocket_ui::controls::Mapping::defaults("gbc").to_file(Vec::new()),
    };
    assert!(file.write(&pocket_ui::core_options::file_path(&data, "gbc")));
    for size in ["640x480", "1024x768", "1240x1080", "1920x1080"] {
        // Controls card and controller setup precede the stick and sixteen button mappings.
        let screen = card.screen("controls:19", size);
        assert!(screen.contains("Hold to toggle"), "{size}: {screen}");
        assert!(screen.lines().any(|l| l == "Off"), "{size}: {screen}");
        assert!(screen.contains("Pick a button"), "the selected control explains itself: {size}: {screen}");
    }
}

#[test]
fn the_button_tester_without_a_controller_says_what_to_do_in_every_language_and_size() {
    let card = Card::new();
    for (lang, heading, leave) in [
        ("en", "No controller found", "Hold to leave"),
        ("de", "Kein Controller gefunden", "Halten zum Verlassen"),
        ("es", "No se encontró ningún mando", "Mantén para salir"),
    ] {
        for size in ["640x480", "1024x768", "1240x1080"] {
            let (words, boxes) = card.screen_in("test-buttons:empty", size, lang, "");
            assert!(words.contains(heading) && words.contains(leave), "{lang} {size}: a sentence and the way out, not a blank: {words}");
            assert!(boxes.trim().is_empty(), "{lang} {size}: text runs into other text or the hint bar: {boxes}");
        }
    }
}

#[test]
fn the_button_tester_lists_every_pad_and_explains_the_last_press_without_cut_words() {
    let card = Card::new();
    // (state, words that must be on the screen)
    let cases: [(&str, &[&str]); 5] = [
        ("test-buttons", &["Pocket", "Player 1 · Built-in", "In menus: L1", "In games: the console's L1", "30%", "R2"]),
        ("test-buttons:bt", &["8BitDo Pro 2", "Player 2 · Bluetooth", "80%", "In menus: B", "In games: the console's B", "A trigger counts as pressed from 55%"]),
        ("test-buttons:two", &["Pocket", "8BitDo SN30 Pro", "Player 1 · Built-in", "Player 2 · Bluetooth", "Player 2 · 8BitDo SN30 Pro"]),
        ("test-buttons:remap", &["Remapped to: Bottom button", "Bottom button"]),
        ("test-buttons:hold", &["Keep holding to leave"]),
    ];
    for (state, expected) in cases {
        for size in ["640x480", "1024x768", "1240x1080"] {
            let (words, boxes) = card.screen_in(state, size, "en", "");
            for want in expected {
                assert!(words.contains(want), "{state} {size}: \"{want}\" is on the screen: {words}");
            }
            assert!(boxes.trim().is_empty(), "{state} {size}: text runs into other text or the hint bar: {boxes}");
        }
    }
    // Press something nobody knows: the controller's own name for it is kept for a bug report.
    let (words, _) = card.screen_in("test-buttons:unknown", "1240x1080", "en", "");
    assert!(words.contains("Not recognised: 8BitDo SN30 Pro: BTN_C 1"), "{words}");
}

#[test]
fn the_button_tester_reads_in_german_and_spanish_with_large_text_without_running_into_anything() {
    let card = Card::new();
    for lang in ["de", "es"] {
        for state in ["test-buttons:two", "test-buttons:remap", "test-buttons:hold"] {
            for size in ["640x480", "1240x1080"] {
                for text in ["normal", "large"] {
                    let (words, boxes) = card.screen_in(state, size, lang, text);
                    assert!(!words.is_empty(), "{lang} {state} {size} {text}");
                    assert!(boxes.trim().is_empty(), "{lang} {state} {size} {text}: text runs into other text or the hint bar: {boxes}\n{words}");
                }
            }
        }
    }
}

/// The same snapshot harness, with a real small card rather than the curator's large demo collection.
fn sparse_card(files: &[&str]) -> Card {
    let card = Card::new();
    let tmp = card.dir.path();
    let roms = tmp.join("small-roms/gba");
    std::fs::create_dir_all(&roms).unwrap();
    for name in files { std::fs::write(roms.join(name), b"x").unwrap(); }
    std::fs::write(tmp.join("es_systems.cfg"), format!(
        "<systemList><system><name>gba</name><fullname>Game Boy Advance</fullname><path>{}</path><extension>.gba .zip .7z</extension><command>true</command></system></systemList>", roms.display()
    )).unwrap();
    // No previous play history, ratings, art or saved curator preferences.
    std::fs::remove_dir_all(tmp.join("data")).unwrap();
    std::fs::create_dir_all(tmp.join("data")).unwrap();
    std::fs::write(tmp.join("data/settings.json"), r#"{"onboarded":true}"#).unwrap();
    std::fs::write(tmp.join("data/sounds.json"), r#"{"on":false,"volume":0}"#).unwrap();
    card
}

#[test]
fn an_empty_card_explains_how_to_add_games_in_every_look_and_both_styles() {
    let card = sparse_card(&[]);
    for font in ["barlow", "retro"] {
        for look in ["shelf", "cabinet", "split", "easy"] {
            let state = if look == "easy" { "big-shelf".into() } else { format!("shelf::{look}") };
            let (words, _) = card.screen_styled(&state, "640x480", "en", "normal", font);
            assert!(words.contains("No games yet") && words.contains("Add games"), "{look}/{font}: a cause and a way forward: {words}");
        }
    }
}

#[test]
fn unknown_games_keep_their_title_and_explain_missing_facts_and_description_in_both_styles() {
    let card = sparse_card(&["Pocket Homebrew Demo (En,Fr) [!].gba"]);
    for font in ["barlow", "retro"] {
        for look in ["shelf", "cabinet", "split"] {
            let turn = if matches!(look, "shelf" | "cabinet") { ":r" } else { "" };
            let (words, _) = card.screen_styled(&format!("info:{look}:gba:pocket homebrew{turn}"), "640x480", "en", "normal", font);
            let words = words.split_whitespace().collect::<Vec<_>>().join(" ");
            for expected in ["Pocket Homebrew Demo", "Not in the offline game list", "No description yet."] {
                assert!(words.contains(expected), "{look}/{font}: {expected}: {words}");
            }
        }
    }
}

#[test]
fn missing_connections_and_history_always_explain_the_next_step_in_both_styles() {
    let card = sparse_card(&["Pocket Homebrew Demo.gba"]);
    for font in ["barlow", "retro"] {
        for (state, expected) in [
            ("switcher", &["Nothing played yet", "The games you play"][..]),
            ("setup:wifi-empty", &["No networks", "Scan again"][..]),
            ("setup:first-wifi-empty", &["No networks", "Continue offline", "Skip the rest"][..]),
            ("curator:history-empty", &["Nothing played", "Games you play"][..]),
            ("setup:wifi-off", &["Wi-Fi is off", "Turn on"][..]),
            ("updates:unreachable", &["Couldn't reach the update server", "Check Wi-Fi", "Check again"][..]),
            ("test-buttons:empty", &["No controller found", "Hold to leave"][..]),
            ("updates:timeout", &["server stopped responding", "Check again"][..]),
            ("slot-save", &["No picture"][..]),
        ] {
            let (words, _) = card.screen_styled(state, "640x480", "en", "normal", font);
            let words = words.split_whitespace().collect::<Vec<_>>().join(" ");
            for want in expected { assert!(words.contains(want), "{state}/{font}: {want}: {words}"); }
        }
    }
}

#[test]
fn easy_details_explain_when_a_game_has_no_description() {
    let card = sparse_card(&["Pocket Homebrew Demo.gba"]);
    for font in ["barlow", "retro"] {
        let (words, _) = card.screen_styled("big-details", "640x480", "en", "normal", font);
        let words = words.split_whitespace().collect::<Vec<_>>().join(" ");
        assert!(words.contains("Pocket Homebrew Demo") && words.contains("No description yet."), "Easy/{font}: missing information needs words too: {words}");
    }
}

#[test]
fn one_system_keeps_roms_with_language_tags_typos_and_archive_extensions_visible() {
    let card = sparse_card(&["Pocket Homebrew Demo (En,Fr) [!].gba", "Poket Dmeo [!].zip", "Odd Demo (En,Fr).7z"]);
    for font in ["barlow", "retro"] {
        let (words, _) = card.screen_styled("shelf:gba:split:games", "640x480", "en", "normal", font);
        for title in ["Pocket Homebrew Demo", "Poket Dmeo", "Odd Demo"] {
            assert!(words.contains(title), "{font}: the file remains a recognisable game: {words}");
        }
    }
}

#[test]
fn a_system_without_known_art_or_facts_still_names_itself_and_counts_its_games() {
    let card = sparse_card(&["Pocket Demo.gba"]);
    let cfg = card.dir.path().join("es_systems.cfg");
    let xml = std::fs::read_to_string(&cfg).unwrap().replace("<name>gba</name>", "<name>mystery</name>").replace("Game Boy Advance", "Mystery console");
    std::fs::write(cfg, xml).unwrap();
    for font in ["barlow", "retro"] {
        for look in ["shelf", "cabinet", "split"] {
            let (words, _) = card.screen_styled(&format!("shelf:mystery:{look}"), "640x480", "en", "normal", font);
            let content = if look == "cabinet" { "Pocket Demo" } else { "1 game" };
            assert!(words.contains("Mystery console") && words.contains(content), "{look}/{font}: unknown systems still have a name and games: {words}");
        }
    }
}

#[test]
fn a_series_with_one_owned_game_is_a_fact_and_never_an_empty_link() {
    let card = sparse_card(&["Metroid Fusion (USA).gba"]);
    for font in ["barlow", "retro"] {
        let (words, _) = card.screen_styled("info:split:gba:metroid fusion:d", "640x480", "en", "normal", font);
        assert!(words.contains("Series") && words.contains("Metroid"), "{font}: the series fact stays: {words}");
        assert!(words.lines().any(|w| w == "Play") && !words.lines().any(|w| w == "Open"), "{font}: a single game cannot open an empty connection: {words}");
    }
}

#[test]
fn an_empty_bluetooth_list_says_what_is_missing_and_how_to_find_a_device() {
    let card = sparse_card(&[]);
    for font in ["barlow", "retro"] {
        for (lang, heading, pairing, search) in [
            ("en", "No devices found", "pairing mode", "Search for devices"),
            ("de", "Keine Geräte gefunden", "Kopplungsmodus", "Nach Geräten suchen"),
            ("es", "No se encontraron dispositivos", "vinculación", "Buscar dispositivos"),
        ] {
            let (words, boxes) = card.screen_styled("bluetooth:empty", "640x480", lang, "large", font);
            let words = words.split_whitespace().collect::<Vec<_>>().join(" ");
            for expected in [heading, pairing, search] { assert!(words.contains(expected), "{lang}/{font}: {expected}: {words}"); }
            assert!(!words.contains("Xbox Wireless Controller"), "a truly empty list has no demo controller: {words}");
            assert!(boxes.trim().is_empty(), "{lang}/{font}: the explanation remains readable: {boxes}");
        }
    }
}

/// 0.3.1, update notice: Settings → System → Updates says what it offers, asks before it installs, names the switch and
/// the charger, whatever the language, text size, style and screen (nothing cut, nothing running into the hint bar).
#[test]
fn the_update_notice_views_are_whole_in_every_language_size_and_style() {
    let card = Card::new();
    for lang in ["en", "de", "es"] {
        for (size, text_size) in [("640x480", "large"), ("1024x768", "normal"), ("1280x720", "large")] {
            for font in ["barlow", "retro"] {
                for (state, must) in [("updates:ready", &["Updates"][..]), ("updates:confirm", &["Updates"][..]), ("updates:confirm-low", &["Updates"][..]), ("updates:off", &["Updates"][..])] {
                    let (words, boxes) = card.screen_styled(state, size, lang, text_size, font);
                    let words = words.split_whitespace().collect::<Vec<_>>().join(" ");
                    assert!(boxes.trim().is_empty(), "{state}/{lang}/{size}/{text_size}/{font}: text runs into other text or the hints: {boxes}");
                    for want in must {
                        assert!(lang != "en" || words.contains(want), "{state}: {want}: {words}");
                    }
                }
            }
        }
    }
    // The words themselves, English: the page offers the version with its notes, the question, the charger and the switch.
    let say = |state: &str| card.screen_styled(state, "1024x768", "en", "normal", "barlow").0.split_whitespace().collect::<Vec<_>>().join(" ");
    let ready = say("updates:ready");
    for want in ["Install pocket-ui 99.0.0", "Check again", "Check for updates automatically", "On", "Update notice:"] {
        assert!(ready.contains(want), "{want}: {ready}");
    }
    let confirm = say("updates:confirm");
    assert!(confirm.contains("Install now") && confirm.contains("Later") && !confirm.contains("Check again") && !confirm.contains("battery"), "{confirm}");
    let low = say("updates:confirm-low");
    assert!(low.contains("Install now") && low.contains("Later") && low.contains("battery is at 14%") && low.contains("Plug in the charger"), "{low}");
    let charging = say("updates:confirm-charging");
    assert!(charging.contains("Install now") && !charging.contains("Plug in the charger"), "on the charger it just goes ahead: {charging}");
    let off = say("updates:off");
    assert!(off.contains("Check for updates automatically") && off.contains("Off"), "{off}");
    // German and Spanish: the new lines are translated, not English left behind.
    let de = card.screen_styled("updates:confirm-low", "1024x768", "de", "normal", "barlow").0.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(de.contains("Jetzt installieren") && de.contains("Später") && de.contains("Der Akku ist bei 14 %"), "{de}");
    let es = card.screen_styled("updates:ready", "1024x768", "es", "normal", "barlow").0.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(es.contains("Buscar actualizaciones automáticamente"), "{es}");
}

/// The dot: with a newer pocket-ui known, Settings → System and its Updates row carry a small amber dot; without, none.
#[test]
fn a_newer_pocket_ui_puts_a_dot_on_system_and_on_updates_and_nowhere_else() {
    let card = Card::new();
    let amber = |state: &str| -> usize {
        card.screen_styled(state, "1024x768", "en", "normal", "barlow");
        let mut reader = png::Decoder::new(std::fs::File::open(card.dir.path().join("shot.png")).unwrap()).read_info().unwrap();
        let mut pixels = vec![0; reader.output_buffer_size()];
        let info = reader.next_frame(&mut pixels).unwrap();
        let bpp = info.color_type.samples();
        let want = pocket_ui::palette::DARK.amber;
        pixels[..info.buffer_size()].chunks(bpp).filter(|p| u32::from(p[0]) << 16 | u32::from(p[1]) << 8 | u32::from(p[2]) == want & 0xffffff).count()
    };
    for (plain, with_dot) in [("settings:5", "settings:5:dot"), ("settings:5:5", "settings:5:5:dot")] {
        let (none, some) = (amber(plain), amber(with_dot));
        assert!(some > none + 20, "{with_dot}: the dot is painted ({some} amber pixels, {none} without)");
    }
    assert_eq!(amber("settings:0:0:dot"), amber("settings:0:0"), "a list without the Updates row shows no dot");
}

/// A main artifact has newer code than the published release but an older Cargo version: do not offer a downgrade.
#[test]
fn development_build_explains_why_it_does_not_offer_a_release_as_newer() {
    let card = Card::new();
    for lang in ["en", "de", "es"] {
        for size in ["640x480", "1024x768"] {
            let (words, boxes) = card.screen_in("updates:development", size, lang, "normal");
            let words = words.split_whitespace().collect::<Vec<_>>().join(" ");
            assert!(boxes.trim().is_empty(), "{lang}/{size}: {boxes}");
            let explanation = match lang { "de" => "Entwicklungsversion", "es" => "Versión de desarrollo", _ => "Development build" };
            assert!(words.contains(explanation), "{lang}/{size}: {words}");
            assert!(!words.contains("0.3.2") && !words.contains("Everything is up to date"), "no false newer-version offer or claim: {words}");
        }
    }
    if !pocket_ui::update::release_build() {
        let words = card.screen("about", "1024x768");
        assert!(words.contains("-dev (") && words.contains(option_env!("POCKETUI_COMMIT").unwrap_or("unknown")), "About identifies the dev build and commit: {words}");
    }
}
