//! Spanish as a player meets it, with the real table. Its own test binary: the interface language is process-wide.

use pocket_ui::i18n::{self, Cache, Lang};
use pocket_ui::keyboard::{Key, Keyboard};
use pocket_ui::search;

#[test]
fn a_painter_goes_from_one_table_to_the_other_without_old_words_and_the_keyboard_follows() {
    let mut cache = Cache::default();
    i18n::set(Lang::De);
    // A pattern's result stays in the painter's cache: from a table to the next it must not stay on the screen.
    assert_eq!(&*cache.get("12 games"), "12 Spiele");
    assert_eq!(&*cache.get("Settings"), "Einstellungen");
    i18n::set(Lang::Es);
    assert_eq!(&*cache.get("12 games"), "12 juegos");
    assert_eq!(&*cache.get("Settings"), "Ajustes");
    i18n::set(Lang::En);
    assert_eq!(&*cache.get("12 games"), "12 games");
    i18n::set(Lang::Es);
    // Its own name, whatever the language; in a line of a game's languages it is translated.
    assert_eq!(&*cache.get("Español"), "Español");
    assert_eq!(&*cache.get("Deutsch"), "Deutsch");
    assert_eq!(&*cache.get("English"), "English");
    assert_eq!(&*cache.get("English, French"), "Inglés, Francés");

    // The search's keyboard is QWERTY and finds a name with ñ from a plain n.
    let row: String = Keyboard::for_search().rows()[1].iter().filter_map(|k| if let Key::Char(c) = k { Some(*c) } else { None }).collect();
    assert_eq!(row, "qwertyuiop");
    assert_eq!(search::fold("Español"), search::fold("espanol"));
}

#[test]
fn dates_and_times_are_spanish() {
    let t = Lang::Es.table().unwrap();
    assert_eq!(t.translate("14 March 2025").as_deref(), Some("14 de marzo de 2025"));
    assert_eq!(t.translate("14 March").as_deref(), Some("14 de marzo"));
    assert_eq!(t.translate("March 2025").as_deref(), Some("marzo de 2025"));
    assert_eq!(t.translate("14 Mar 2025").as_deref(), Some("14 mar 2025"));
}
