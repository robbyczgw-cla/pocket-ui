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
    let quebec = ["courriel", "magasin", "fin de semaine", "clavardage", "téléverser", "cellulaire", "fichier de journal"];
    let bad: Vec<String> = t
        .entries()
        .into_iter()
        .filter(|(_, fr)| {
            let lower = fr.to_lowercase();
            quebec.iter().any(|w| lower.contains(w))
        })
        .map(|(en, fr)| format!("{en:?} -> {fr:?}"))
        .collect();
    assert!(bad.is_empty(), "words of Quebec, not France:\n{}", bad.join("\n"));
}

/// French typography as the glossary has it: a no-break space (U+00A0, the one every font has; the narrow U+202F no font here has)
/// before `? ! : ;`, never a plain space, « » with one inside, the straight apostrophe, and no "vous". `Text::wrap` keeps such a mark
/// with its word (tests/text_size.rs finds a line cut before it).
#[test]
fn the_marks_follow_their_word_after_a_no_break_space() {
    let t = Lang::Fr.table().unwrap();
    let mut bad = Vec::new();
    for (en, fr) in t.entries() {
        let hole = |s: &str| s.replace(|c: char| c.is_alphanumeric() || c == '{' || c == '}', "");
        let _ = hole;
        for (at, ch) in fr.char_indices() {
            let before = fr[..at].chars().last();
            if matches!(ch, '?' | '!' | ';') && before == Some(' ') {
                bad.push(format!("{en:?} -> {fr:?}: a plain space before {ch:?}"));
            }
            if ch == ':' && before == Some(' ') {
                bad.push(format!("{en:?} -> {fr:?}: a plain space before ':'"));
            }
            if ch == '«' && !fr[at + ch.len_utf8()..].starts_with('\u{a0}') {
                bad.push(format!("{en:?} -> {fr:?}: « without its no-break space"));
            }
            if ch == '»' && before != Some('\u{a0}') {
                bad.push(format!("{en:?} -> {fr:?}: » without its no-break space"));
            }
            if matches!(ch, '’' | '\u{202f}') {
                bad.push(format!("{en:?} -> {fr:?}: {ch:?} (straight apostrophe and U+00A0 only)"));
            }
        }
        let lower = fr.to_lowercase();
        if lower.split(|c: char| !c.is_alphabetic()).any(|w| w == "vous" || w == "votre" || w == "vos") {
            bad.push(format!("{en:?} -> {fr:?}: vous, not tu"));
        }
    }
    assert!(bad.is_empty(), "{} lines break the French typography:\n{}", bad.len(), bad.join("\n"));
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
