//! The game search's keyboard is QWERTZ in German (y and z swapped), QWERTY otherwise. Its own test binary: the
//! interface language is process-wide.

use pocket_ui::i18n::{self, Lang};
use pocket_ui::keyboard::{Key, Keyboard};

fn letters(kb: &Keyboard, row: usize) -> String {
    kb.rows()[row].iter().filter_map(|k| if let Key::Char(c) = k { Some(*c) } else { None }).collect()
}

#[test]
fn the_german_search_keyboard_has_z_in_the_top_row_and_y_at_the_bottom_left() {
    i18n::set(Lang::En);
    let kb = Keyboard::for_search();
    assert_eq!((letters(&kb, 1), letters(&kb, 3)), ("qwertyuiop".into(), "zxcvbnm".into()));
    i18n::set(Lang::De);
    // Same shape, so the grid and the touch targets stay as they are.
    assert_eq!((letters(&kb, 1), letters(&kb, 3)), ("qwertzuiop".into(), "yxcvbnm".into()));
    assert_eq!(kb.rows().iter().map(Vec::len).collect::<Vec<_>>(), [10, 10, 9, 7, 3]);
}
