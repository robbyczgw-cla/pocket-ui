//! A separate process: the German interface and learned library names are process-wide.

use pocket_ui::cabinet_art::Look;
use pocket_ui::i18n::{self, Lang};
use pocket_ui::media::{self, Game};
use pocket_ui::text::Text;

#[test]
fn a_cartridge_labels_shortened_title_keeps_its_original_language() {
    i18n::set(Lang::De);
    i18n::german();
    let title = format!("Black: {}", "A very long subtitle that cannot fit on a cartridge label ".repeat(12));
    i18n::learn_names([&title]);
    assert!(!i18n::is_name("Black"), "only the complete library name is learned");
    let look = Look { color: 0x3a2a6a, band: 0x7a6cb0, plastic: 0x9c99a2 };
    let mut translated = Text::new();
    let actual = media::game_pic(&mut translated, Game::Gb, 200, look, &title, None, None).unwrap();
    assert!(translated.translating(), "the painter keeps its caller's translation setting");
    let mut original = Text::new();
    original.set_translate(false);
    let expected = media::game_pic(&mut original, Game::Gb, 200, look, &title, None, None).unwrap();
    assert!(!original.translating(), "an untranslated caller stays untranslated");
    assert_eq!((actual.w, actual.h), (expected.w, expected.h));
    assert!(actual.px == expected.px, "shortening Black must not turn its label into Schwarz");
}
