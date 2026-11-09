//! A game's name on two lines stays its name in every language. The language is process-wide, hence one test in a file
//! of its own. New-player walk 2026-11-02: "Sonic Adventure" read "Sonic Aventura" on its case in Spanish.
use pocket_ui::cabinet_art::{plain_label, Look, Pic};
use pocket_ui::gfx::Canvas;
use pocket_ui::i18n::{self, Lang};
use pocket_ui::text::{Face, Text};

#[test]
fn a_name_on_two_lines_is_not_translated_word_by_word() {
    let paint = |lang| {
        i18n::set(lang);
        let mut t = Text::builtin();
        let mut p = Pic::new(120, 160, 0);
        plain_label(&mut t, &mut p, (0, 0, 120, 160), Look { color: 0x804020, band: 0, plastic: 0 }, "Sonic Adventure", None);
        p.px
    };
    let ui = |lang| {
        i18n::set(lang);
        let mut t = Text::builtin();
        let mut pixels = vec![0; 120 * 160];
        let mut canvas = Canvas::new(&mut pixels, 120, 160);
        for (row, label) in ["Search", "Settings"].iter().enumerate() {
            t.draw(&mut canvas, Face::Body, 28, 8, 48 + row as i32 * 42, 0xffffff, label);
        }
        pixels
    };
    assert!(ui(Lang::Es) != ui(Lang::En), "genuine two-line UI copy still translates");
    i18n::set(Lang::Es);
    assert_eq!(i18n::tr("Adventure"), "Aventura", "the ordinary UI word translates");
    assert!(paint(Lang::Es) == paint(Lang::En), "a cover keeps even an unlearned name literal");
    i18n::learn_names(["Sonic Adventure"]);
    let spanish = paint(Lang::Es);
    assert_eq!(spanish, paint(Lang::En), "on a case with no cover");

    i18n::set(Lang::Es);
    let mut t = Text::builtin();
    let room = t.width(Face::Title, 37, "Mmmmmmmm");
    let lines = t.wrap(Face::Title, 37, "Sonic Adventure", room);
    assert_eq!(lines, ["Sonic", "Adventure"]);
    assert_eq!(&*t.translated(&lines[1]), "Adventure", "the name's line");
    assert_eq!(&*t.translated("Adventure"), "Aventura", "the word elsewhere, as before");
}
