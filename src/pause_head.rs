//! The head of Android's touch pause sheet: the game's name at the top and the status line under it ("Saved to slot 2",
//! an error). Painted by the player (`android/src/player.rs`, `touch_pause`); it lives here so that the host's tests can
//! render it.

use crate::gfx::Canvas;
use crate::text::{Face, Text};
use crate::title_block::{self, Line};
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

/// The width of a half button (Save | Load, Earlier | Options, Controls | Zoom) in the landscape sheet `width` wide.
pub fn landscape_half(width: i32) -> i32 {
    let col_w = (width - 4 * EDGE) / 2;
    (col_w - GAP) / 2
}

#[cfg(test)]
mod tests {
    use super::*;
    /// Every half button's words stay whole on the Retroid screens (Mini V2 1240x1080, RP5 1920x1080, 4:3 1280x960) at
    /// Android's usual and larger display sizes, in English, German and Spanish.
    #[test]
    fn the_pause_buttons_say_their_whole_word_on_retroid_screens() {
        use crate::i18n::{self, Lang};
        // Language is process-wide; keep it from changing other tests' words.
        if std::env::var_os("PAUSE_WORDS_CHILD").is_none() {
            let result = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "pause_head::tests::the_pause_buttons_say_their_whole_word_on_retroid_screens", "--nocapture"])
                .env("PAUSE_WORDS_CHILD", "1").output().unwrap();
            assert!(result.status.success() && String::from_utf8_lossy(&result.stdout).contains("1 passed; 0 failed"), "{}\n{}", String::from_utf8_lossy(&result.stdout), String::from_utf8_lossy(&result.stderr));
            return;
        }
        let mut text = Text::builtin();
        let mut cut = Vec::new();
        for lang in [Lang::En, Lang::De, Lang::Es] {
            i18n::set(lang);
            for (w, h, density) in [(1240, 1080, 2.0), (1240, 1080, 2.5), (1920, 1080, 2.5), (1920, 1080, 3.0), (1280, 960, 2.0)] {
                // Android's buttons at the side in landscape on a small screen (Mini V2: about 64 dp), or at the bottom.
                for side in [0, (64.0 * density) as u32] {
                    let (lw, lh) = crate::scaled_layout::mobile_size((w - side, h), density);
                    assert!(!portrait(lw as i32, lh as i32, false));
                    let room = landscape_half(lw as i32) - 24;
                    for label in ["Save", "Load", "Earlier", "Options", "Controls", "Zoom"] {
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
}
