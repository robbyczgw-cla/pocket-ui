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
        let status = Command::new("python3").args(["-I", &format!("{ROOT}/scripts/curator-fake-library.py"), dir.path().to_str().unwrap(), "dark"]).stdout(std::process::Stdio::null()).status().unwrap();
        assert!(status.success());
        let stub = (cfg!(target_os = "linux") && std::env::var_os("WAYLAND_DISPLAY").is_none_or(|w| w.is_empty())).then(|| {
            let child = Command::new("python3").args(["-I", &format!("{ROOT}/scripts/wayland-stub.py"), dir.path().join("wl").to_str().unwrap()]).spawn().unwrap();
            for _ in 0..200 {
                if dir.path().join("wl").exists() {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
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
        if !text_size.is_empty() {
            cmd.env("POCKETUI_TEXT_SIZE", text_size);
        }
        let out = cmd
            .args(["--snapshot", state, png.to_str().unwrap()])
            .env("POCKETUI_MOCK", "1")
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
    for word in ["Easy to read", "On", "Off", "Later", "Done"] {
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
        // Controls card is the first row, before the stick and sixteen button mappings.
        let screen = card.screen("controls:18", size);
        assert!(screen.contains("Hold to toggle"), "{size}: {screen}");
        assert!(screen.lines().any(|l| l == "Off"), "{size}: {screen}");
        assert!(screen.contains("Pick a button"), "the selected control explains itself: {size}: {screen}");
    }
}
