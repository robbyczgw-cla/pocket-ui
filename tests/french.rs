//! French as a player meets it, with the real table. Its own test binary: the interface language is process-wide.

use pocket_ui::i18n::{self, Cache, Lang};
use pocket_ui::keyboard::{Key, Keyboard};
use pocket_ui::search;

#[test]
fn a_painter_goes_from_one_table_to_the_other_without_old_words_and_the_keyboard_follows() {
    // This thread's language only: the other tests in this program keep theirs.
    let _language = i18n::for_this_thread(Lang::En);
    let mut cache = Cache::default();
    i18n::set(Lang::Pt);
    // A pattern's result stays in the painter's cache: from a table to the next it must not stay on the screen.
    assert_eq!(&*cache.get("12 games"), "12 jogos");
    i18n::set(Lang::Fr);
    assert_eq!(&*cache.get("12 games"), "12 jeux");
    assert_eq!(&*cache.get("Settings"), "Paramètres");
    i18n::set(Lang::En);
    assert_eq!(&*cache.get("12 games"), "12 games");
    i18n::set(Lang::Fr);
    // Its own name, whatever the language; in a line of a game's languages it is translated.
    assert_eq!(&*cache.get("Français"), "Français");
    assert_eq!(&*cache.get("Português (Brasil)"), "Português (Brasil)");
    assert_eq!(&*cache.get("Deutsch"), "Deutsch");
    assert_eq!(&*cache.get("English"), "English");
    assert_eq!(&*cache.get("English, French"), "Anglais, Français");

    // The search's keyboard is AZERTY-free (the same QWERTY layout as English: one on-screen layout for every language) and finds
    // a name with é, è, ç or œ from the plain letter.
    let row: String = Keyboard::for_search().rows()[1].iter().filter_map(|k| if let Key::Char(c) = k { Some(*c) } else { None }).collect();
    assert_eq!(row, "qwertyuiop");
    assert_eq!(search::fold("Pokémon Émeraude"), search::fold("pokemon emeraude"));
    assert_eq!(search::fold("Garçon"), search::fold("garcon"));
}

#[test]
fn dates_and_times_are_french() {
    let t = Lang::Fr.table().unwrap();
    assert_eq!(t.translate("14 March 2025").as_deref(), Some("14 mars 2025"));
    assert_eq!(t.translate("14 March").as_deref(), Some("14 mars"));
    assert_eq!(t.translate("March 2025").as_deref(), Some("mars 2025"));
    assert_eq!(t.translate("Mon").as_deref(), Some("lun."));
    assert_eq!(t.translate("Dec").as_deref(), Some("déc"));
}

/// The words the glossary decided on are the ones every page says (docs/i18n-glossary.md): a block translated apart must not
/// drift to Quebec's words ("courriel", "magasiner", "fin de semaine"), to "vous", or to another word for a page the code looks for.
#[test]
fn the_words_are_french_of_france_in_tu_and_the_page_names_are_the_ones_the_code_looks_for() {
    let t = Lang::Fr.table().unwrap();
    for (english, french) in [
        ("Performance & fan", "Performances et ventilateur"),
        ("Continue", "Reprendre"),
        ("Quit", "Quitter"),
        ("Language", "Langue"),
        ("Settings", "Paramètres"),
    ] {
        assert_eq!(t.translate(english).as_deref(), Some(french), "{english}");
    }
    // Words of Quebec are a warning of scripts/fix-typography.py now (19.11.2026), not a merge blocker.
}

/// French typography (no-break space before `? ! : ;`, « », the straight apostrophe, "vous") is not a merge blocker any more
/// (19.11.2026: an apostrophe no person sees must not stop anything): scripts/fix-typography.py lists it every night and
/// fixes it with --write. This test only proves the fixer does what its rules say.
#[test]
fn the_typography_fixer_passes_its_own_cases() {
    let out = std::process::Command::new("python3")
        .arg("-I")
        .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/scripts/fix-typography.py"))
        .arg("--self-test")
        .output()
        .unwrap();
    assert!(out.status.success(), "{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
}

/// A French mark at the end of a wrapped line would stand alone on the next one: `Text::wrap` never breaks at U+00A0.
#[test]
fn a_line_is_never_broken_between_a_word_and_its_mark() {
    use pocket_ui::text::{Face, Text};
    let _language = i18n::for_this_thread(Lang::En);
    i18n::set(Lang::En);
    let mut text = Text::new();
    let s = "Tu es prêt\u{a0}? Choisis un jeu\u{a0}: «\u{a0}Mario\u{a0}»";
    for max in (150..500).step_by(7) {
        for line in text.wrap(Face::Body, 28, s, max) {
            assert!(!line.starts_with(['?', '!', ':', ';', '»']), "{max}: {line:?}");
            assert!(!line.ends_with('«'), "{max}: {line:?}");
        }
    }
    assert!(text.words_fit(Face::Body, 28, s, 100_000));
}
