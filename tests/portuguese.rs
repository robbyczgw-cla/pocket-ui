//! Brazilian Portuguese as a player meets it, with the real table. Its own test binary: the interface language is process-wide.

use pocket_ui::i18n::{self, Cache, Lang};
use pocket_ui::keyboard::{Key, Keyboard};
use pocket_ui::search;

#[test]
fn a_painter_goes_from_one_table_to_the_other_without_old_words_and_the_keyboard_follows() {
    // This thread's language only: the other tests in this program keep theirs.
    let _language = i18n::for_this_thread(Lang::En);
    let mut cache = Cache::default();
    i18n::set(Lang::Es);
    // A pattern's result stays in the painter's cache: from a table to the next it must not stay on the screen.
    assert_eq!(&*cache.get("12 games"), "12 juegos");
    assert_eq!(&*cache.get("Settings"), "Ajustes");
    i18n::set(Lang::Pt);
    assert_eq!(&*cache.get("12 games"), "12 jogos");
    assert_eq!(&*cache.get("Settings"), "Configurações");
    i18n::set(Lang::En);
    assert_eq!(&*cache.get("12 games"), "12 games");
    i18n::set(Lang::Pt);
    // Its own name, whatever the language; in a line of a game's languages it is translated.
    assert_eq!(&*cache.get("Português (Brasil)"), "Português (Brasil)");
    assert_eq!(&*cache.get("Español"), "Español");
    assert_eq!(&*cache.get("Deutsch"), "Deutsch");
    assert_eq!(&*cache.get("English"), "English");
    assert_eq!(&*cache.get("English, French"), "Inglês, Francês");

    // The search's keyboard is QWERTY and finds a name with ã, ç or é from the plain letter.
    let row: String = Keyboard::for_search().rows()[1].iter().filter_map(|k| if let Key::Char(c) = k { Some(*c) } else { None }).collect();
    assert_eq!(row, "qwertyuiop");
    assert_eq!(search::fold("Português"), search::fold("portugues"));
    assert_eq!(search::fold("Coração"), search::fold("coracao"));
}

#[test]
fn dates_and_times_are_portuguese() {
    let t = Lang::Pt.table().unwrap();
    assert_eq!(t.translate("14 March 2025").as_deref(), Some("14 de março de 2025"));
    assert_eq!(t.translate("14 March").as_deref(), Some("14 de março"));
    assert_eq!(t.translate("March 2025").as_deref(), Some("março de 2025"));
    assert_eq!(t.translate("14 Mar 2025").as_deref(), Some("14 mar 2025"));
}

/// The Brazilian words the glossary decided on are the ones every page says (docs/i18n-glossary.md): a block translated apart
/// must not drift to Spain's words ("ecrã", "ficheiro", "telemóvel"), and the button and page names stay as the code needs them.
#[test]
fn the_words_are_brazilian_and_the_page_names_are_the_ones_the_code_looks_for() {
    let t = Lang::Pt.table().unwrap();
    for (english, brazilian) in [
        ("Performance & fan", "Desempenho e ventoinha"),
        ("Continue", "Continuar"),
        ("Quit", "Sair"),
        ("Language", "Idioma"),
    ] {
        assert_eq!(t.translate(english).as_deref(), Some(brazilian), "{english}");
    }
    let spain = ["ecrã", "ficheiro", "telemóvel", "descarreg", "utilizador", "partilh", "aceder", "consola"];
    let bad: Vec<String> = t
        .entries()
        .into_iter()
        // A word that starts with a stem of Portugal's: "partilhar" is Portugal's, "compartilhar" the Brazilian one.
        .filter(|(_, pt)| {
            let lower = pt.to_lowercase();
            lower.split(|c: char| !c.is_alphabetic()).any(|word| spain.iter().any(|w| word.starts_with(w)))
        })
        .map(|(en, pt)| format!("{en:?} -> {pt:?}"))
        .collect();
    assert!(bad.is_empty(), "words of Portugal, not Brazil:\n{}", bad.join("\n"));
}
