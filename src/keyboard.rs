//! On-screen keyboard for the gamepad: a key grid moved with the D-pad.
//! A types, B deletes (or cancels when empty), Y shift, X symbols, START done.
//!
//! `Keyboard::for_search` is the shorter layout of the game search (`search_page`): letters and digits only,
//! no Shift and no symbols (the search ignores case and punctuation), so X and Y stay free for the results.

use crate::pad::Button;

const LETTERS: [&str; 4] = ["1234567890", "qwertyuiop", "asdfghjkl-", "zxcvbnm_.@"];
const SYMBOLS: [&str; 4] = ["!@#$%^&*()", "-_=+[]{}|\\", ";:'\",.<>/?", "`~"];
/// The game search's layout: what a game's name can be found by.
const SEARCH: [&str; 4] = ["1234567890", "qwertyuiop", "asdfghjkl", "zxcvbnm"];
/// The same with the German thumb's y and z (the search folds accents, so umlaut keys are not needed).
const SEARCH_DE: [&str; 4] = ["1234567890", "qwertzuiop", "asdfghjkl", "yxcvbnm"];
pub const SEARCH_SPECIALS: [Special; 3] = [Special::Space, Special::Delete, Special::Done];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Special {
    Shift,
    Symbols,
    Space,
    Delete,
    Done,
}

pub const SPECIALS: [Special; 5] = [Special::Shift, Special::Symbols, Special::Space, Special::Delete, Special::Done];

#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    Editing,
    Done(String),
    Cancelled,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Key {
    Char(char),
    Special(Special),
}

pub struct Keyboard {
    pub text: String,
    pub row: usize,
    pub col: usize,
    pub shift: bool,
    pub symbols: bool,
    /// The game search's layout (`for_search`).
    pub search: bool,
}

impl Default for Keyboard {
    fn default() -> Self {
        Self { text: String::new(), row: 1, col: 0, shift: false, symbols: false, search: false }
    }
}

impl Keyboard {
    /// The game search's keyboard: letters and digits, Space, Delete and Done; it starts on "q".
    pub fn for_search() -> Self {
        Self { search: true, ..Default::default() }
    }

    /// Rows of keys as currently shown; the last row holds the special keys.
    pub fn rows(&self) -> Vec<Vec<Key>> {
        if self.search {
            let layout = if crate::i18n::current() == crate::i18n::Lang::De { SEARCH_DE } else { SEARCH };
            let mut rows: Vec<Vec<Key>> = layout.iter().map(|r| r.chars().map(Key::Char).collect()).collect();
            rows.push(SEARCH_SPECIALS.iter().map(|s| Key::Special(*s)).collect());
            return rows;
        }
        let layer = if self.symbols { SYMBOLS } else { LETTERS };
        let mut rows: Vec<Vec<Key>> = layer
            .iter()
            .map(|r| {
                r.chars()
                    .map(|c| Key::Char(if self.shift && !self.symbols { c.to_ascii_uppercase() } else { c }))
                    .collect()
            })
            .collect();
        rows.push(SPECIALS.iter().map(|s| Key::Special(*s)).collect());
        rows
    }

    pub fn press(&mut self, b: Button) -> Outcome {
        let rows = self.rows();
        match b {
            Button::Up => {
                self.row = (self.row + rows.len() - 1) % rows.len();
                self.clamp(&rows);
            }
            Button::Down => {
                self.row = (self.row + 1) % rows.len();
                self.clamp(&rows);
            }
            Button::Left => self.col = (self.col + rows[self.row].len() - 1) % rows[self.row].len(),
            Button::Right => self.col = (self.col + 1) % rows[self.row].len(),
            Button::East => return self.activate(rows[self.row][self.col]),
            Button::South => {
                if self.text.pop().is_none() {
                    return Outcome::Cancelled;
                }
            }
            // The search layout has no Shift or symbols: X and Y belong to its results.
            Button::North | Button::West if self.search => {}
            Button::North => self.shift = !self.shift,
            Button::West => {
                self.symbols = !self.symbols;
                self.clamp(&self.rows());
            }
            Button::Start => return Outcome::Done(self.text.clone()),
            _ => {}
        }
        Outcome::Editing
    }

    fn activate(&mut self, key: Key) -> Outcome {
        match key {
            Key::Char(c) => {
                self.text.push(c);
                // Shift is one-shot, as on a phone keyboard.
                self.shift = false;
            }
            Key::Special(Special::Shift) => self.shift = !self.shift,
            Key::Special(Special::Symbols) => {
                self.symbols = !self.symbols;
                self.clamp(&self.rows());
            }
            Key::Special(Special::Space) => self.text.push(' '),
            Key::Special(Special::Delete) => {
                self.text.pop();
            }
            Key::Special(Special::Done) => return Outcome::Done(self.text.clone()),
        }
        Outcome::Editing
    }

    fn clamp(&mut self, rows: &[Vec<Key>]) {
        self.col = self.col.min(rows[self.row].len() - 1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn types_shifts_deletes_and_finishes() {
        let mut kb = Keyboard::default();
        // Row 1 starts at 'q'; shift, type, move right, type.
        kb.press(Button::North);
        kb.press(Button::East);
        kb.press(Button::Right);
        kb.press(Button::East);
        assert_eq!(kb.text, "Qw", "shift only applies to one letter");
        kb.press(Button::South);
        assert_eq!(kb.text, "Q");
        assert_eq!(kb.press(Button::Start), Outcome::Done("Q".into()));
    }

    #[test]
    fn delete_on_empty_cancels_and_rows_wrap() {
        let mut kb = Keyboard::default();
        assert_eq!(kb.press(Button::South), Outcome::Cancelled);
        kb.row = 0;
        kb.col = 9;
        kb.press(Button::Up);
        assert_eq!(kb.row, 4);
        assert_eq!(kb.col, 4, "clamped to the special row");
    }

    #[test]
    fn the_search_layout_has_letters_digits_and_three_specials_and_leaves_x_and_y_alone() {
        let mut kb = Keyboard::for_search();
        let rows = kb.rows();
        assert_eq!(rows.len(), 5);
        assert_eq!(rows[4], SEARCH_SPECIALS.iter().map(|s| Key::Special(*s)).collect::<Vec<_>>());
        assert_eq!((kb.row, kb.col), (1, 0), "starts on q");
        kb.press(Button::North);
        kb.press(Button::West);
        assert!(!kb.shift && !kb.symbols);
        kb.press(Button::East);
        assert_eq!(kb.text, "q", "never upper case");
        // Row 1 (10 keys) to row 2 (9 keys) from the last column: clamped.
        kb.col = 9;
        kb.press(Button::Down);
        assert_eq!((kb.row, kb.col), (2, 8));
        kb.row = 4;
        kb.col = 2;
        assert_eq!(kb.press(Button::East), Outcome::Done("q".into()), "the last special is Done");
    }
}
