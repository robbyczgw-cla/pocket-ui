//! Spanish as a player meets it, with the real table. Its own test binary: the interface language is process-wide.

use pocket_ui::i18n::{self, Cache, Lang};
use pocket_ui::keyboard::{Key, Keyboard};
use pocket_ui::search;

#[test]
fn a_painter_goes_from_one_table_to_the_other_without_old_words_and_the_keyboard_follows() {
    // This thread's language only: the other tests in this program keep theirs.
    let _language = i18n::for_this_thread(Lang::En);
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

/// Tests run side by side in one program: a language of one thread's own (`i18n::for_this_thread`) stays on that thread, the
/// program's language is untouched, and dropping the guard brings back what was there, also when the test fails.
#[test]
fn a_threads_own_language_stays_on_that_thread_and_comes_back() {
    let program = || std::thread::spawn(i18n::current).join().unwrap();
    let start = i18n::current();
    {
        let _spanish = i18n::for_this_thread(Lang::Es);
        assert_eq!(i18n::tr("Settings"), "Ajustes");
        i18n::set(Lang::De);
        assert_eq!(i18n::tr("Settings"), "Einstellungen", "set changes this thread's own language");
        assert_eq!(program(), start, "another thread keeps the program's language");
    }
    assert_eq!(i18n::current(), start);
    let failed = std::panic::catch_unwind(|| {
        let _german = i18n::for_this_thread(Lang::De);
        panic!("a failing test");
    });
    assert!(failed.is_err());
    assert_eq!(i18n::current(), start, "a failing test gives the language back");
}
