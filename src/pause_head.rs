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

/// The height the pause rows need at the least, under the head: seven targets for a portrait phone, four for a landscape one.
pub fn rows_need(portrait: bool, card: bool) -> i32 {
    if portrait {
        7 * TARGET_MIN + 6 * GAP + if card { 38 + 110 } else { 38 } + 16
    } else {
        4 * TARGET_MIN + 3 * GAP
    }
}
