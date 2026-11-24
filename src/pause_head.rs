//! The head of Android's touch pause sheet: the game's name at the top and the status line under it ("Saved to slot 2",
//! an error). Painted by the player (`android/src/player.rs`, `touch_pause`); it lives here so that the host's tests can
//! render it.

use crate::gfx::Canvas;
use crate::text::{Face, Text};
use crate::title_block::{self, Line};
use crate::touch::Area;
use crate::type_scale;

/// The sheet's logical pixels (about 2.2 per dp): the side margin, the gap between rows and the least height of a target
/// (48 dp).
pub const EDGE: i32 = 40;
pub const GAP: i32 = 20;
pub const TARGET_MIN: i32 = 106;
/// The rows start at `TOP` at the least.
const TOP: i32 = 170;

/// What the head painted: its lines, where the status line is, and where the rows may start.
pub struct Head {
    pub name: Vec<(Line, i32)>,
    pub status: Vec<(String, i32)>,
    /// The status line's design size (28, a step smaller when the rows need the room).
    pub body: u32,
    pub bottom: i32,
}

/// The head laid out for a sheet `width` wide in `height` pixels whose rows need `rows_need` of them: the whole name in
/// up to three lines (a subtitle smaller under its title, never translated), then the status line in up to three. The type
/// goes a step smaller, name first, only while head and rows would not fit; the rows keep their height.
pub fn layout(text: &mut Text, name: &str, status: &str, width: i32, height: i32, rows_need: i32) -> Head {
    let room = height - rows_need;
    let (mut px, mut body) = (46, 28);
    loop {
        let lines = text.literal(|text| title_block::name_layout(text, Face::Title, px, name, width, 3));
        let first = 46 + type_scale::px(lines.first().map_or(px, |l| l.px)) as i32;
        let ys = title_block::positions(text, Face::Title, &lines, 0, first, false);
        let mut at = ys.last().map_or(first, |p| p.1);
        let mut shown = Vec::new();
        if !status.is_empty() {
            at += type_scale::px(body) as i32 + 20;
            for line in text.wrap_at_most(Face::Body, body, status, width, 3) {
                shown.push((line, at));
                at += type_scale::px(body) as i32 + 8;
            }
            at -= type_scale::px(body) as i32 + 8;
        }
        let bottom = TOP.max(at + 30);
        let next = (title_block::smaller(px), title_block::smaller(body));
        if bottom <= room || (next.0 >= px && next.1 >= body) {
            return Head { name: lines.into_iter().zip(ys.iter().map(|p| p.1)).collect(), status: shown, body, bottom };
        }
        // The name first; the status follows once the name is as small as it goes.
        if next.0 < px { px = next.0 } else { body = next.1 }
    }
}

/// Paints `head` from the left edge `x`.
pub fn paint(c: &mut Canvas, text: &mut Text, head: &Head, x: i32, chalk: u32, dust: u32) {
    for (line, y) in &head.name {
        crate::title_block::paint_line(text, c, Face::Title, line, x, *y, chalk);
        crate::title_block::note(line, *y);
    }
    for (line, y) in &head.status {
        text.draw(c, Face::Body, head.body, x, *y, dust, line);
    }
}

/// Whether the sheet takes its portrait layout (seven rows under each other). Only on a page taller than wide where
/// those rows fit under the head, the status line and a controller's hint line; a near-square screen (720x720 with
/// Android's buttons at the side: 684x720) takes the two columns, which need four rows. Before, `w < h` alone picked
/// portrait there and Save & quit fell off the bottom.
pub fn portrait(width: i32, height: i32, card: bool) -> bool {
    width < height && height - 36 - 56 - TOP >= rows_need(true, card)
}

/// All of the pause sheet's rows fit under its head on a `width` x `height` sheet, in two columns of at least 300 when
/// it is held across.
pub fn rows_fit(width: i32, height: i32, card: bool) -> bool {
    let portrait = portrait(width, height, card);
    rows_need(portrait, card) <= height - 36 - 56 - TOP && (portrait || (width - 4 * EDGE) / 2 >= 300)
}
/// The height the pause rows need at the least, under the head: seven targets for a portrait phone, four for a landscape one.
pub fn rows_need(portrait: bool, card: bool) -> i32 {
    if portrait {
        7 * TARGET_MIN + 6 * GAP + if card { 38 + 110 } else { 38 } + 16
    } else {
        4 * TARGET_MIN + 3 * GAP
    }
}

/// A pause button's words, whole in a step or two smaller type before anything is cut with "…": German "Speichern" and
/// "Steuerung" did not fit a half button of a Retroid Pocket Mini V2 (1240x1080) at Android's larger display size.
pub fn button_words(text: &mut Text, face: Face, px: u32, label: &str, width: i32) -> (u32, String) {
    text.fit_sizes(face, &[px, px * 7 / 8, px * 3 / 4], label, width)
}

/// The pause sheet's buttons in the order they are read: Continue, Save | Load, Earlier | Options, Zoom, Screenshot,
/// Save & quit, then Achievements. The rows' numbers are the code's (`rows()`), and Screenshot is row 5 but sits below
/// Zoom (row 7); walking the numbers made Down jump up there and look like presses that did nothing. Both layouts (one
/// column, or two with these buttons in reading order) read the same way. Row 6 (Controls, the console's card) is no
/// longer on the sheet: the controls card went (the 0.5 hand test, 15.11.2026).
const READING_ORDER: [usize; 9] = [0, 1, 2, 3, 4, 7, 5, 8, 9];

/// The row the D-pad reaches from `row` with Down (`down`) or Up on a sheet of `rows` rows: the next button in reading
/// order, and none past the ends. Left and Right stay the slot picker's.
pub fn pad_step(row: usize, rows: usize, down: bool) -> usize {
    let order: Vec<usize> = READING_ORDER.iter().copied().filter(|r| *r < rows).collect();
    let at = order.iter().position(|r| *r == row).unwrap_or(0);
    order[if down { (at + 1).min(order.len() - 1) } else { at.saturating_sub(1) }]
}

/// The slot picker's place in `places`: not a row of `rows()` (A on it does nothing), but the D-pad stops on it, and
/// Left/Right there change the slot.
pub const PICKER: usize = 100;
/// The extra height of the slot picker (taller with the whole memory card) and of the primary button (Continue).
pub const fn picker_extra(card: bool) -> i32 {
    if card { 38 + 110 } else { 38 }
}
pub const PRIMARY_EXTRA: i32 = 16;

/// Where the pause sheet's buttons sit on a `width` x `height` sheet whose rows go from `top` (under the head) to
/// `bottom` (over the hint line): `(row, area)` for rows 0..=8 but 6 (9 when `achievements`), and `(PICKER, area)`.
/// The painter (`android/src/player.rs`, `touch_pause`) paints exactly these; the D-pad walks them (`grid_step`).
pub fn places(width: i32, height: i32, top: i32, bottom: i32, card: bool, achievements: bool) -> Vec<(usize, Area)> {
    let (w, inner) = (width, width - 2 * EDGE);
    let (pick_extra, primary_extra) = (picker_extra(card), PRIMARY_EXTRA);
    let more = achievements as i32;
    let mut out = Vec::new();
    if portrait(width, height, card) {
        let avail = bottom - top;
        let n = 7 + more;
        let bh = ((avail - (n - 1) * GAP - pick_extra - primary_extra) / n).clamp(TARGET_MIN, 124);
        let total = n * bh + (n - 1) * GAP + pick_extra + primary_extra;
        let mut y = (bottom - total).max(top);
        out.push((0, Area::new(EDGE, y, inner, bh + primary_extra)));
        y += bh + primary_extra + GAP;
        out.push((PICKER, Area::new(EDGE, y, inner, bh + pick_extra)));
        y += bh + pick_extra + GAP;
        let half = (inner - GAP) / 2;
        for pair in [[1, 2], [3, 4]] {
            for (col, row) in pair.into_iter().enumerate() {
                out.push((row, Area::new(EDGE + col as i32 * (half + GAP), y, half, bh)));
            }
            y += bh + GAP;
        }
        // Zoom and the two longest actions keep a full row, including at 200% font scale in DE/ES.
        for row in [7, 5, 8].into_iter().chain(achievements.then_some(9)) {
            out.push((row, Area::new(EDGE, y, inner, bh)));
            y += bh + GAP;
        }
    } else {
        // Two columns that always fit the height: Continue, the slot and Save | Load at the left; Earlier | Options,
        // Zoom, Screenshot and Quit at the right. The buttons shrink to fit, never below 48 dp, before
        // anything scrolls.
        let avail = bottom - top;
        let col_w = (inner - 2 * EDGE) / 2;
        let rows = 4 + more;
        let bh = ((avail - (rows - 1) * GAP) / rows).clamp(TARGET_MIN, 124);
        let right_h = rows * bh + (rows - 1) * GAP;
        let y0 = top + ((avail - right_h) / 2).max(0);
        let x2 = w - EDGE - col_w;
        let half = landscape_half(w);
        // Left column: the same height as the right one, the spare room shared between its gaps.
        let spare = (right_h - 3 * bh - 2 * GAP) / 2;
        let (cont, pick) = (bh + primary_extra.min(spare).max(0), bh + pick_extra.min(spare).max(0));
        // The whole memory card: the picker takes two rows' height, Continue and Save | Load keep a plain one.
        let (cont, pick) = if card { (bh, right_h - 2 * bh - 2 * GAP) } else { (cont, pick) };
        let gap = ((right_h - cont - pick - bh) / 2).max(GAP / 2);
        out.push((0, Area::new(EDGE, y0, col_w, cont)));
        let y = y0 + cont + gap;
        out.push((PICKER, Area::new(EDGE, y, col_w, pick)));
        let y = y + pick + gap;
        let left_half = (col_w - GAP) / 2;
        out.push((1, Area::new(EDGE, y, left_half, bh)));
        out.push((2, Area::new(EDGE + left_half + GAP, y, left_half, bh)));
        for (col, row) in [3, 4].into_iter().enumerate() {
            out.push((row, Area::new(x2 + col as i32 * (half + GAP), y0, half, bh)));
        }
        for (n, row) in [7, 5, 8].into_iter().chain(achievements.then_some(9)).enumerate() {
            out.push((row, Area::new(x2, y0 + (n as i32 + 1) * (bh + GAP), col_w, bh)));
        }
    }
    out
}

/// A D-pad direction on the pause sheet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dir {
    Up,
    Down,
    Left,
    Right,
}

/// The button the D-pad reaches from `from` in `dir` on the sheet `places` (13.11.: "simply natural", the
/// pad moves as the screen looks, half buttons side by side included). `anchor` is the column a run of Up/Down keeps
/// (the x where it started; None: the middle of `from`), so Down from Zoom to the wide Screenshot and Up again lands on
/// Zoom, not on Controls. Returns the new button and the anchor to keep; at an edge the focus stays.
pub fn grid_step(places: &[(usize, Area)], from: usize, dir: Dir, anchor: Option<i32>) -> (usize, i32) {
    let Some(&(_, f)) = places.iter().find(|(r, _)| *r == from) else {
        return (from, anchor.unwrap_or(0));
    };
    let mid = |a: Area| (a.x + a.w / 2, a.y + a.h / 2);
    let (fx, fy) = mid(f);
    let anchor = anchor.unwrap_or(fx);
    // Ahead: past `from`'s far edge (a few pixels of overlap allowed), its middle further in that direction.
    let ahead = |a: Area| match dir {
        Dir::Down => a.y >= f.y + f.h - 4 && mid(a).1 > fy,
        Dir::Up => a.y + a.h <= f.y + 4 && mid(a).1 < fy,
        Dir::Right => a.x >= f.x + f.w - 4 && mid(a).0 > fx,
        Dir::Left => a.x + a.w <= f.x + 4 && mid(a).0 < fx,
    };
    let distance = |lo: i32, hi: i32, at: i32| if at < lo { lo - at } else if at >= hi { at - hi + 1 } else { 0 };
    let best = places.iter().filter(|(r, a)| *r != from && ahead(*a)).min_by_key(|(r, a)| {
        let (m, (lo, hi)) = (mid(*a), match dir { Dir::Up | Dir::Down => (a.x, a.x + a.w), _ => (a.y, a.y + a.h) });
        let (gap, beside, off) = match dir {
            // Up/Down: a button under `from` (sharing some of its width) first, then the nearest row, then the one
            // nearest to the anchor's column.
            Dir::Down => (a.y - (f.y + f.h), lo < f.x + f.w && f.x < hi, (m.0 - anchor).abs()),
            Dir::Up => (f.y - (a.y + a.h), lo < f.x + f.w && f.x < hi, (m.0 - anchor).abs()),
            // Left/Right: a button beside `from` (sharing some of its height) first, then the nearest one.
            Dir::Right => (a.x - (f.x + f.w), lo < f.y + f.h && f.y < hi, (m.1 - fy).abs()),
            Dir::Left => (f.x - (a.x + a.w), lo < f.y + f.h && f.y < hi, (m.1 - fy).abs()),
        };
        // A few pixels are no difference (an odd width puts the middle 1 px nearer the right half): then reading order.
        (!beside, gap.max(0), off / 16, *r)
    });
    match best {
        None => (from, anchor),
        Some(&(row, a)) => {
            // Up/Down keep their column while they pass a wider button; a side step or a narrower one starts a new one.
            let keep = matches!(dir, Dir::Up | Dir::Down) && a.w > f.w;
            (row, if keep { anchor } else { mid(a).0 })
        }
    }
}

/// The width of a half button (Save | Load, Earlier | Options) in the landscape sheet `width` wide.
pub fn landscape_half(width: i32) -> i32 {
    let col_w = (width - 4 * EDGE) / 2;
    (col_w - GAP) / 2
}

#[cfg(test)]
mod tests {
    use super::*;
    /// Where each row sits on the one-column sheet (band from the top, place in the band), and the order of the
    /// two-column one (left column, then right) is the same reading.
    fn place(row: usize) -> (usize, usize) {
        match row {
            0 => (0, 0),
            1 | 2 => (1, row - 1),
            3 | 4 => (2, row - 3),
            7 => (3, 0),
            5 => (4, 0),
            8 => (5, 0),
            _ => (6, 0),
        }
    }
    #[test]
    fn down_on_the_pause_sheet_never_moves_up_and_up_never_moves_down() {
        for rows in [9, 10] {
            let buttons = rows - 1;
            let mut row = 0;
            let mut visited = vec![row];
            for _ in 0..rows + 3 {
                let next = pad_step(row, rows, true);
                assert!(place(next) >= place(row), "Down from row {row} landed on row {next}, higher up");
                row = next;
                visited.push(row);
            }
            assert_eq!(row, rows - 1, "the walk ends on the last button");
            visited.dedup();
            assert_eq!(visited.len(), buttons, "Down reaches every button once: {visited:?}");
            for _ in 0..rows + 3 {
                let next = pad_step(row, rows, false);
                assert!(place(next) <= place(row), "Up from row {row} landed on row {next}, further down");
                row = next;
            }
            assert_eq!(row, 0);
        }
    }
    #[test]
    fn the_pause_sheets_steps_from_the_places_that_used_to_jump() {
        // Options -> Zoom (used: Screenshot, below it), Zoom -> Screenshot, Screenshot -> Save & quit, and back.
        assert_eq!(pad_step(4, 9, true), 7);
        assert_eq!(pad_step(7, 9, true), 5);
        assert_eq!(pad_step(5, 9, true), 8);
        assert_eq!(pad_step(8, 9, false), 5);
        assert_eq!(pad_step(5, 9, false), 7);
        assert_eq!(pad_step(7, 9, false), 4);
        assert_eq!(pad_step(0, 9, false), 0, "Up on Continue stays");
        assert_eq!(pad_step(8, 9, true), 8, "Down on the last button stays");
        assert_eq!(pad_step(8, 10, true), 9, "Achievements follows Save & quit");
    }
    /// Every half button's words stay whole on the Retroid screens (Mini V2 1240x1080, RP5 1920x1080, 4:3 1280x960) at
    /// Android's usual and larger display sizes, in English, German and Spanish.
    #[test]
    fn the_pause_buttons_say_their_whole_word_on_retroid_screens() {
        use crate::i18n::{self, Lang};
        // The language is this thread's own in the unit tests (`i18n::for_this_thread`): no other test's words change.
        let mut text = Text::builtin();
        let mut cut = Vec::new();
        for lang in Lang::ALL {
            i18n::set(lang);
            for (w, h, density) in [(1240, 1080, 2.0), (1240, 1080, 2.5), (1920, 1080, 2.5), (1920, 1080, 3.0), (1280, 960, 2.0)] {
                // Android's buttons at the side in landscape on a small screen (Mini V2: about 64 dp), or at the bottom.
                for side in [0, (64.0 * density) as u32] {
                    let (lw, lh) = crate::scaled_layout::mobile_size((w - side, h), density);
                    assert!(!portrait(lw as i32, lh as i32, false));
                    let room = landscape_half(lw as i32) - 24;
                    for label in ["Save", "Load", "Earlier", "Options"] {
                        let (_, shown) = button_words(&mut text, Face::Body, 38, label, room);
                        if shown.contains('…') {
                            cut.push(format!("{lang:?} {w}x{h} density {density}, side bar {side} px: {shown}"));
                        }
                    }
                }
            }
        }
        i18n::set(Lang::En);
        assert!(cut.is_empty(), "pause buttons cut short: {cut:?}");
    }
    /// The pause sheet fits on the screens of common Android handhelds, with Android's buttons at the side or the bottom:
    /// all of its rows under the head, never below the screen.
    #[test]
    fn the_pause_rows_fit_on_every_common_handheld_screen() {
        // (name, width, height px, density): 16:9, 4:3, 1:1, 3:2 handhelds and a tall phone in both orientations.
        let screens = [("16:9", 1920, 1080, 2.3), ("4:3 1240", 1240, 1080, 2.25), ("4:3 1280", 1280, 960, 2.0), ("1:1", 720, 720, 1.5),
            ("3:2", 1620, 1080, 2.25), ("phone", 1216, 2640, 3.0), ("phone landscape", 2640, 1216, 3.0)];
        for (name, w, h, density) in screens {
            for bar in [0, 36, 48] {
                for (vw, vh) in [(w - bar, h), (w, h - bar)] {
                    let (lw, lh) = crate::scaled_layout::mobile_size((vw as u32, vh as u32), density);
                    let (lw, lh) = (lw as i32, lh as i32);
                    for card in [false, true] {
                        let portrait = portrait(lw, lh, card);
                        let room = lh - 36 - 56 - TOP;
                        assert!(rows_need(portrait, card) <= room, "{name} {vw}x{vh} ({lw}x{lh}, card {card}): rows need {} of {room}", rows_need(portrait, card));
                        if !portrait {
                            assert!((lw - 4 * EDGE) / 2 >= 300, "{name}: two columns of at least 300 ({lw})");
                        }
                    }
                }
            }
        }
        assert!(portrait(880, 1910, false), "a portrait phone keeps its one column");
    }

    /// The sheet as a player sees it on a `w` x `h` screen of `density`, with the controller's hint line.
    fn sheet(w: u32, h: u32, density: f32, card: bool, achievements: bool) -> Vec<(usize, Area)> {
        let (lw, lh) = crate::scaled_layout::mobile_size((w, h), density);
        let (lw, lh) = (lw as i32, lh as i32);
        let mut text = Text::builtin();
        let bottom = lh - 36 - 56;
        let head = layout(&mut text, "Advance Wars", "Saved to slot 2", lw - 2 * EDGE, bottom, rows_need(portrait(lw, lh, card), card));
        places(lw, lh, head.bottom, bottom, card, achievements)
    }
    const NAMES: [(&str, usize); 10] = [("Continue", 0), ("Save", 1), ("Load", 2), ("Earlier", 3), ("Options", 4),
        ("Screenshot", 5), ("Zoom", 7), ("Save & quit", 8), ("Achievements", 9), ("Slot", PICKER)];
    fn row(name: &str) -> usize {
        NAMES.iter().find(|(n, _)| *n == name).unwrap().1
    }
    fn name(row: usize) -> &'static str {
        NAMES.iter().find(|(_, r)| *r == row).unwrap().0
    }
    /// Walks `table` (from, direction, where it lands) on `places` and lists every step that lands elsewhere.
    fn walk(places: &[(usize, Area)], table: &[(&str, Dir, &str)]) -> Vec<String> {
        table.iter().filter_map(|&(from, dir, to)| {
            let (got, _) = grid_step(places, row(from), dir, None);
            (got != row(to)).then(|| format!("{from} {dir:?} -> {} (wanted {to})", name(got)))
        }).collect()
    }
    use Dir::{Down, Left, Right, Up};
    /// 13.11.: the D-pad moves as the sheet looks, half buttons side by side included. The landscape sheet of a
    /// 16:9 handheld (Odin 2 Mini, RP5): Continue, the slot and Save | Load at the left; Earlier | Options,
    /// Controls | Zoom, Screenshot and Save & quit at the right.
    #[test]
    fn the_pause_pad_moves_as_the_landscape_sheet_looks() {
        let table = [
            ("Continue", Down, "Slot"), ("Continue", Right, "Earlier"), ("Continue", Up, "Continue"), ("Continue", Left, "Continue"),
            ("Slot", Up, "Continue"), ("Slot", Down, "Save"),
            ("Save", Right, "Load"), ("Load", Left, "Save"), ("Save", Up, "Slot"), ("Load", Up, "Slot"), ("Save", Down, "Save"),
            ("Load", Right, "Save & quit"),
            ("Earlier", Right, "Options"), ("Options", Left, "Earlier"), ("Earlier", Left, "Continue"),
            ("Earlier", Down, "Zoom"), ("Options", Down, "Zoom"), ("Options", Right, "Options"), ("Earlier", Up, "Earlier"),
            ("Zoom", Down, "Screenshot"), ("Screenshot", Down, "Save & quit"),
            ("Save & quit", Up, "Screenshot"), ("Save & quit", Left, "Load"), ("Save & quit", Down, "Save & quit"),
        ];
        for (screen, w, h, density) in [("Odin 2 Mini", 1920, 1080, 2.75), ("RP5", 1920, 1080, 2.5), ("Mini V2", 1240, 1080, 2.0), ("4:3", 1280, 960, 2.0)] {
            let wrong = walk(&sheet(w, h, density, false, false), &table);
            assert!(wrong.is_empty(), "{screen}: {wrong:#?}");
        }
    }
    /// The one-column sheet of a phone held upright: Continue, the slot, then Save | Load, Earlier | Options and
    /// Controls | Zoom side by side, Screenshot and Save & quit.
    #[test]
    fn the_pause_pad_moves_as_the_portrait_sheet_looks() {
        let table = [
            ("Continue", Down, "Slot"), ("Continue", Right, "Continue"), ("Slot", Down, "Save"), ("Slot", Up, "Continue"),
            ("Save", Right, "Load"), ("Load", Left, "Save"), ("Save", Down, "Earlier"), ("Load", Down, "Options"),
            ("Save", Up, "Slot"), ("Load", Up, "Slot"), ("Options", Up, "Load"), ("Earlier", Up, "Save"),
            ("Earlier", Right, "Options"), ("Options", Left, "Earlier"), ("Earlier", Down, "Zoom"), ("Options", Down, "Zoom"),
            ("Zoom", Down, "Screenshot"),
            ("Screenshot", Down, "Save & quit"), ("Save & quit", Up, "Screenshot"), ("Save & quit", Down, "Save & quit"),
        ];
        let wrong = walk(&sheet(1080, 2400, 2.75, false, false), &table);
        assert!(wrong.is_empty(), "{wrong:#?}");
        let with_ra = sheet(1080, 2400, 2.75, false, true);
        assert_eq!(grid_step(&with_ra, 8, Down, None).0, 9, "Achievements under Save & quit");
    }
    /// Down past a wide button and Up again comes back to the column it started in: Options -> Zoom -> Options,
    /// Load -> Slot -> Load.
    #[test]
    fn up_and_down_come_back_to_the_column_they_left() {
        for places in [sheet(1080, 2400, 2.75, false, false), sheet(1920, 1080, 2.75, false, false)] {
            for (start, there) in [("Options", Down), ("Earlier", Down), ("Load", Up), ("Save", Up)] {
                let back = if there == Down { Up } else { Down };
                let (mid, anchor) = grid_step(&places, row(start), there, None);
                let (end, _) = grid_step(&places, mid, back, Some(anchor));
                assert_eq!(name(end), start, "{start} {there:?} -> {} -> {back:?}", name(mid));
            }
        }
    }
    /// The nav explorer for the pause sheet on Android's screens (16:9, 4:3, 1:1, 3:2 handhelds, a phone both ways),
    /// with and without the whole memory card and Achievements: from Continue the D-pad reaches every button and the
    /// slot, no step goes against its direction, and Left/Right between two half buttons go back and forth.
    #[test]
    fn the_pad_reaches_every_pause_button_on_every_android_screen() {
        let screens = [("16:9", 1920, 1080, 2.75), ("4:3 1240", 1240, 1080, 2.0), ("4:3 1280", 1280, 960, 2.0), ("1:1", 720, 720, 1.5),
            ("3:2", 1620, 1080, 2.25), ("phone", 1080, 2400, 2.75), ("phone landscape", 2400, 1080, 2.75)];
        let mut faults = Vec::new();
        for (screen, w, h, density) in screens {
            for card in [false, true] {
                for achievements in [false, true] {
                    let places = sheet(w, h, density, card, achievements);
                    let at = |r: usize| places.iter().find(|(p, _)| *p == r).unwrap().1;
                    let mut seen = vec![0];
                    let mut todo = vec![0];
                    while let Some(from) = todo.pop() {
                        for dir in [Up, Down, Left, Right] {
                            let (to, _) = grid_step(&places, from, dir, None);
                            if to == from {
                                continue;
                            }
                            let (f, t) = (at(from), at(to));
                            let (fx, fy, tx, ty) = (f.x + f.w / 2, f.y + f.h / 2, t.x + t.w / 2, t.y + t.h / 2);
                            let right_way = match dir { Up => ty < fy, Down => ty > fy, Left => tx < fx, Right => tx > fx };
                            if !right_way {
                                faults.push(format!("{screen} card {card}: {} {dir:?} -> {} goes the wrong way", name(from), name(to)));
                            }
                            if !seen.contains(&to) {
                                seen.push(to);
                                todo.push(to);
                            }
                        }
                    }
                    for (r, _) in &places {
                        if !seen.contains(r) {
                            faults.push(format!("{screen} card {card} achievements {achievements}: {} unreachable", name(*r)));
                        }
                    }
                    for (a, b) in [(1, 2), (3, 4)] {
                        if grid_step(&places, a, Right, None).0 != b || grid_step(&places, b, Left, None).0 != a {
                            faults.push(format!("{screen} card {card}: {} <-> {} not side by side", name(a), name(b)));
                        }
                    }
                }
            }
        }
        assert!(faults.is_empty(), "{faults:#?}");
    }
}
