//! Italian as a player meets it, with the real table. Its own test binary: the interface language is process-wide.

use pocket_ui::i18n::{self, Cache, Lang};
use pocket_ui::keyboard::{Key, Keyboard};
use pocket_ui::search;

#[test]
fn a_painter_goes_from_the_italian_table_to_the_others_without_old_words_and_the_keyboard_follows() {
    // This thread's language only: the other tests in this program keep theirs.
    let _language = i18n::for_this_thread(Lang::En);
    let mut cache = Cache::default();
    i18n::set(Lang::It);
    // A pattern's result stays in the painter's cache: from a table to the next it must not stay on the screen.
    assert_eq!(&*cache.get("12 games"), "12 giochi");
    assert_eq!(&*cache.get("1 game"), "1 gioco");
    assert_eq!(&*cache.get("Settings"), "Impostazioni");
    i18n::set(Lang::Pt);
    assert_eq!(&*cache.get("12 games"), "12 jogos");
    i18n::set(Lang::En);
    assert_eq!(&*cache.get("12 games"), "12 games");
    i18n::set(Lang::It);
    // Its own name, whatever the language; in a line of a game's languages it is translated.
    assert_eq!(&*cache.get("Italiano"), "Italiano");
    assert_eq!(&*cache.get("Português (Brasil)"), "Português (Brasil)");
    assert_eq!(&*cache.get("Deutsch"), "Deutsch");
    assert_eq!(&*cache.get("English"), "English");
    assert_eq!(&*cache.get("English, French"), "Inglese, Francese");

    // The search's keyboard is QWERTY and finds a name with à è é ì ò ù from the plain letter.
    let row: String = Keyboard::for_search().rows()[1].iter().filter_map(|k| if let Key::Char(c) = k { Some(*c) } else { None }).collect();
    assert_eq!(row, "qwertyuiop");
    assert_eq!(search::fold("Città"), search::fold("citta"));
    assert_eq!(search::fold("Perché"), search::fold("perche"));
    assert_eq!(search::fold("Più"), search::fold("piu"));
}

#[test]
fn dates_and_times_are_italian() {
    let t = Lang::It.table().unwrap();
    assert_eq!(t.translate("14 March 2025").as_deref(), Some("14 marzo 2025"));
    assert_eq!(t.translate("14 March").as_deref(), Some("14 marzo"));
    assert_eq!(t.translate("March 2025").as_deref(), Some("Marzo 2025"));
    assert_eq!(t.translate("14 Mar 2025").as_deref(), Some("14 mar 2025"));
}

/// The words the glossary decided on are the ones every page says (docs/i18n-glossary.md): a block translated apart must not
/// drift, the page names the code looks for stay, and the friend's "tu" never turns into the formal "Lei".
#[test]
fn the_words_are_the_glossarys_and_the_tone_is_tu() {
    let t = Lang::It.table().unwrap();
    for (english, italian) in [
        ("Settings", "Impostazioni"),
        ("Performance & fan", "Prestazioni e ventola"),
        ("Continue", "Continua"),
        ("Quit", "Esci"),
        ("Language", "Lingua"),
        ("Achievements", "Obiettivi"),
        ("Controls", "Controlli"),
        ("Screen & sound", "Schermo e audio"),
    ] {
        assert_eq!(t.translate(english).as_deref(), Some(italian), "{english}");
    }
    // Formal imperatives, a typed apostrophe for an accent and a no-break space are warnings of scripts/fix-typography.py now
    // (19.11.2026), not a merge blocker; --write fixes the accents and spaces.
}
