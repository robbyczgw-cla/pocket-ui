//! What a frame offers the D-pad, for the navigation explorer (`tests/nav_explorer.rs`, docs/DEVELOPING.md "The navigation
//! explorer"): where an operable row is drawn, where the focus mark is, and which hints the bar shows. The shared painters say
//! so (`kit::selection`, `kit::row_label`, `kit::shelf_row`, `features::row_back`, the first start's pill rows, both hint bars),
//! so a page that paints with them is covered without code of its own; a page that draws a row or its focus by hand calls
//! `row` or `focus` there.
//!
//! The marks go into `text::boxes`'s record of the frame, so what covers a page (a sheet, a dialog, the quick settings) takes
//! its marks away with its text. Off unless `POCKETUI_NAV_PROBE` names a file: then `text::boxes::finish` writes every string
//! and mark of the frame there, one a line, `x0 y0 x1 y1 string` (tabs), and the test driver adds where the App is
//! (`app_nav_probe`). With the variable unset nothing is recorded and nothing is written (one load of a flag per call).

use crate::gfx::Canvas;

/// The variable naming the file of the record.
pub const ENV: &str = "POCKETUI_NAV_PROBE";
/// The focus mark: the selection's paper, a selected spine row.
pub const FOCUS: &str = "‹nav:focus›";
/// A row or control a player can operate (move to, change, open).
pub const ROW: &str = "‹nav:row›";
/// A hint the bar shows: this, then the key, a tab and its word.
pub const HINT: &str = "‹nav:hint›";

/// The record is on (`POCKETUI_NAV_PROBE` is set).
pub fn on() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os(ENV).is_some_and(|p| !p.is_empty()))
}

/// The focus mark is painted at `(x, y, w, h)` of `c`.
pub fn focus(c: &Canvas, rect: (i32, i32, i32, i32)) {
    if on() {
        crate::text::boxes::area(c, rect, FOCUS);
    }
}

/// A row a player can operate is painted at `(x, y, w, h)` of `c`; the text painted inside it is its name and value.
pub fn row(c: &Canvas, rect: (i32, i32, i32, i32)) {
    if on() {
        crate::text::boxes::area(c, rect, ROW);
    }
}

/// The hint bar shows `key` with `label` at `(x, y, w, h)` of `c`.
pub fn hint(c: &Canvas, rect: (i32, i32, i32, i32), key: &str, label: &str) {
    if on() {
        crate::text::boxes::area(c, rect, &format!("{HINT}{key}\t{label}"));
    }
}

/// A mark of this record, not a string the player reads.
pub fn is_mark(s: &str) -> bool {
    s.starts_with("‹nav:")
}

/// The frame's strings and marks, each with its place in the picture (`text::boxes::finish`), into the record's file.
pub fn write(placed: &[((i32, i32, i32, i32), bool, &str)]) {
    if !on() {
        return;
    }
    let Some(path) = std::env::var_os(ENV) else { return };
    let mut out = String::new();
    for ((x0, y0, x1, y1), _, s) in placed {
        // A string's own tabs and line breaks would break the line apart.
        let s = s.replace(['\n', '\r'], " ");
        out += &format!("{x0}\t{y0}\t{x1}\t{y1}\t{s}\n");
    }
    let _ = std::fs::write(path, out);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marks_are_told_apart_from_what_a_player_reads() {
        assert!(is_mark(FOCUS) && is_mark(ROW) && is_mark(&format!("{HINT}A\tOpen")));
        assert!(!is_mark("Settings") && !is_mark("‹hints›") && !is_mark("‹ On ›"));
    }
}
