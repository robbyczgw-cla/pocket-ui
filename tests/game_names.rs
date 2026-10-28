//! VDEV-ROUND-2026-10-25 #22: a game's name is never translated. The painter looks every string up in the German
//! table by its words, so a game called "Black" came out as "Schwarz" and "Time to Play" as "Uhrzeit bis Spielen".
//! The places that make names tell the interface (`i18n::learn_names`); these tests run the real table.
//!
//! Own test binary: what is learned is process-wide, and each test uses names of its own.

use pocket_ui::i18n::{self, Cache, Lang};

fn german() {
    i18n::set(Lang::De);
    i18n::german();
}

#[test]
fn a_learned_name_is_painted_as_it_is_and_the_interface_word_still_is_translated() {
    german();
    let mut c = Cache::default();
    // The case that was wrong: the table has these words (whole, or as a pattern).
    assert_eq!(&*c.get("Black"), "Schwarz", "the interface's word, before any game of that name is known");
    let before = c.get("Time to Play").to_string();
    assert_ne!(before, "Time to Play", "this one fits a pattern of the table, or the test proves nothing");

    i18n::learn_names(["Black", "Time to Play"]);
    assert_eq!(&*c.get("Black"), "Black", "a game called Black");
    assert_eq!(&*c.get("Time to Play"), "Time to Play", "a game called Time to Play");
    // Other words are untouched.
    assert_eq!(&*c.get("Back"), "Zurück");
    assert_eq!(&*c.get("Settings"), "Einstellungen");
    // A part of a line of parts is a name too, the rest of the line is translated.
    let line = c.get("Black · Game Boy").to_string();
    assert!(line.starts_with("Black · "), "{line}");
    // The same set again does nothing (and a fresh painter agrees).
    i18n::learn_names(["Black"]);
    assert_eq!(&*Cache::default().get("Black"), "Black");
}

#[test]
fn the_games_of_a_library_read_are_names() {
    german();
    let _dir_guard = tempfile::tempdir().unwrap();
    let dir = _dir_guard.path().to_path_buf();
    std::fs::create_dir_all(dir.join("gb")).unwrap();
    // "Dark" and "Light" are the Look page's words.
    for f in ["Dark.gb", "Light.gb"] {
        std::fs::write(dir.join("gb").join(f), b"x").unwrap();
    }
    let cfg = dir.join("es_systems.cfg");
    std::fs::write(&cfg, format!("<systemList><system><name>gb</name><fullname>Game Boy</fullname><path>{}</path><extension>.gb</extension></system></systemList>", dir.join("gb").display())).unwrap();
    let mut c = Cache::default();
    assert_eq!(&*c.get("Dark"), "Dunkel");
    assert_eq!(&*c.get("Light"), "Hell");

    let systems = pocket_ui::library::load(&cfg).unwrap();
    assert_eq!(systems[0].games.len(), 2);
    assert_eq!(&*c.get("Dark"), "Dark", "the game, once the library is read");
    assert_eq!(&*c.get("Light"), "Light");
}

#[test]
fn a_name_made_for_painting_from_a_file_name_is_learned_but_one_made_for_matching_is_not() {
    german();
    let mut c = Cache::default();
    assert_eq!(&*c.get("Options"), "Optionen");
    assert_eq!(pocket_ui::library::display_name("Options (World)"), "Options");
    assert_eq!(&*c.get("Options"), "Optionen", "display_name compares names (the picture catalogue's thousands): it teaches nothing");
    assert_eq!(pocket_ui::library::shelf_name("Options (World)"), "Options");
    assert_eq!(&*c.get("Options"), "Options", "shelf_name makes a name to paint");
}

/// A title with sentence-like punctuation fills a {s} like any other (it is a name, not a line of text): "…, after 6
/// months" was left in English for "WarioWare, Inc. - Mega Microgame$!" (the span rule of `kept_spans_too_much`).
#[test]
fn a_title_with_sentence_punctuation_fills_a_placeholder() {
    german();
    let titles = ["WarioWare, Inc. - Mega Microgame$!", "Dr. Mario", "Mr. Driller", "Super Mario Bros. 3"];
    i18n::learn_names(titles);
    let mut c = Cache::default();
    for t in titles {
        let line = c.get(&format!("{t}, after 6 months")).to_string();
        assert!(line.starts_with(&format!("{t}, nach ")), "{line}");
    }
    // What the rule is for stays: text that is no known name is not one value.
    let line = c.get("The panel is on. Rest of it, after 6 months").to_string();
    assert!(!line.starts_with("The panel is on. Rest of it, nach "), "{line}");
}
