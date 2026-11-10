//! The big name of the selected game (Shelf, Cabinet, Cards, Split, Easy): whole, never cut at two lines while there is
//! room under it. A name with a subtitle ("The Legend of Zelda - Ocarina of Time") is a big main title and a smaller
//! line under it ("Ocarina of Time"), without the dash. Up to `max_lines` lines in all; only when they do not hold the
//! name, the type goes one step smaller (and again, down to little more than half its size), and only when it still does
//! not fit, the last line ends in "…".
//!
//! Layout is pure measuring (`layout`, `positions`); the callers keep the room for the most lines they allow free, so the
//! cover and what stands under the name do not move when the selection does.

use crate::text::{Face, Text};

/// One line of a name: its text, its size (design px, before Settings → Text size) and whether it is the subtitle.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    pub text: String,
    pub px: u32,
    pub sub: bool,
    /// This line belongs to a game/system name, including a shortened fragment.
    pub literal: bool,
}

/// The subtitle's type is this share of the main title's, in tenths.
const SUB_TENTHS: u32 = 7;
/// "One step smaller": five sixths. The subtitle follows its main title.
const STEP_NUM: u32 = 5;
const STEP_DEN: u32 = 6;
/// Nothing is made smaller than this (it still reads on the small screens) nor than this share of the size asked for.
const LEAST: u32 = 21;
const LEAST_SHARE: u32 = 55;

/// A name's main part and its subtitle at the first " - " or ": ", `None` when it has none (or one side is too short to
/// be a title of its own: "R-Type - 2" stays whole).
pub fn split(title: &str) -> Option<(&str, &str)> {
    let (cut, sep) = [" - ", ": "].iter().filter_map(|sep| title.find(sep).map(|i| (i, sep.len()))).min()?;
    let (main, sub) = (title[..cut].trim(), title[cut + sep..].trim());
    (main.len() >= 3 && sub.len() >= 2).then_some((main, sub))
}

/// The subtitle's size for a main title of `px`.
pub fn sub_px(px: u32) -> u32 {
    (px * SUB_TENTHS / 10).max(LEAST.min(px))
}

/// One step below `px`.
pub fn smaller(px: u32) -> u32 {
    (px * STEP_NUM / STEP_DEN).max(LEAST.min(px))
}

/// The distance from one line's baseline to the next in a block of type `px` (before Settings → Text size): the wider
/// faces' ink (Pixelify Sans) is taller than Barlow Condensed's, so their lines stand further apart. A subtitle line is
/// `step(text, sub_px(px))` from the one before it, halfway between the two sizes (`positions`).
pub fn step(text: &Text, px: u32) -> i32 {
    let k = if text.style() == crate::font_style::FontStyle::Barlow { 0.92 } else { 1.08 };
    (crate::type_scale::px(px) as f32 * k).round() as i32
}

/// The largest size from `px` down (a step at a time) at which `lines` lines of the block fit into `room` pixels.
pub fn px_for(text: &Text, px: u32, lines: usize, room: i32) -> u32 {
    let mut p = px;
    while p > LEAST && step(text, p) * lines as i32 > room {
        let next = smaller(p);
        if next >= p {
            break;
        }
        p = next;
    }
    p
}

/// `lines` are the whole `title`: its words, in order, without the dash or colon of its subtitle, none cut with "…".
pub fn is_whole(lines: &[Line], title: &str) -> bool {
    let words = |s: &str| s.replace(" - ", " ").replace(": ", " ").split_whitespace().map(str::to_string).collect::<Vec<_>>();
    lines.iter().flat_map(|l| words(&l.text)).collect::<Vec<_>>() == words(title)
}

/// `title` in at most `max_lines` lines of `width` pixels, starting at main type `px`. Names are laid out as they are
/// painted: a game's name is literal, and its lines carry that identity through measurement and painting.
pub fn layout(text: &mut Text, face: Face, px: u32, title: &str, width: i32, max_lines: usize) -> Vec<Line> {
    layout_kind(text, face, px, title, width, max_lines, crate::i18n::is_name(title))
}

/// A game/system title, even before library/metadata loading has learned its name.
pub fn name_layout(text: &mut Text, face: Face, px: u32, title: &str, width: i32, max_lines: usize) -> Vec<Line> {
    layout_kind(text, face, px, title, width, max_lines, true)
}

#[allow(clippy::too_many_arguments)]
fn layout_kind(text: &mut Text, face: Face, px: u32, title: &str, width: i32, max_lines: usize, name: bool) -> Vec<Line> {
    if name { text.literal(|text| fit(text, face, px, title, width, max_lines.max(1), true)) }
    else { fit(text, face, px, title, width, max_lines.max(1), false) }
}

/// The same title rule inside a fixed heading band. Details keeps the cover, system/year and facts
/// in place: shrink a step when the measured baselines would cross that band, before cutting words.
pub fn layout_in(text: &mut Text, face: Face, px: u32, title: &str, width: i32, height: i32) -> Vec<Line> {
    layout_in_kind(text, face, px, title, width, height, false)
}

/// A literal game/system heading fitted into a fixed band.
pub fn name_layout_in(text: &mut Text, face: Face, px: u32, title: &str, width: i32, height: i32) -> Vec<Line> {
    layout_in_kind(text, face, px, title, width, height, true)
}

#[allow(clippy::too_many_arguments)]
fn layout_in_kind(text: &mut Text, face: Face, px: u32, title: &str, width: i32, height: i32, name: bool) -> Vec<Line> {
    let mut size = px;
    loop {
        let lines = layout_kind(text, face, size, title, width, 3, name || crate::i18n::is_name(title));
        let first = lines.first().map_or(size, |l| l.px);
        let bottom = positions(text, face, &lines, 0, crate::type_scale::px(first) as i32, false).last().map_or(0, |p| p.1);
        let next = smaller(first);
        if bottom <= height || next >= first {
            return lines;
        }
        size = next;
    }
}

/// A name on one line of a list row (the search results): `room` pixels wide, starting at `px`. The same rule as the
/// spine (`media::spine_fit`), for a line: the whole name; without a leading "The"; the part that tells the games apart
/// (`media::telling_parts`: "Link's Awakening DX" of "The Legend of Zelda: Link's Awakening DX"); each by the type's
/// steps down from `px` (`smaller`, to `LEAST_SHARE` of it). Only when none fits, the end of the best part is cut with
/// "…". The first two sizes are tried for every part before a smaller one. Returns the text to paint and its size; a
/// game's name is painted as it is, not translated.
pub fn row(text: &mut Text, face: Face, px: u32, title: &str, room: i32) -> (String, u32) {
    text.literal(|text| {
    let least = (px * LEAST_SHARE / 100).max(LEAST.min(px));
    let mut sizes = vec![px];
    while let Some(&last) = sizes.last() {
        let next = smaller(last);
        if next >= last || next < least {
            break;
        }
        sizes.push(next);
    }
    let bare = title.strip_prefix("The ").map(str::trim).filter(|r| !r.is_empty());
    let parts = crate::media::telling_parts(title);
    let candidates: Vec<&str> = std::iter::once(title).chain(bare).chain(parts.iter().copied()).collect();
    // A step smaller before a part is dropped, but the part at the size asked for beats the whole name at 21 beside rows
    // of 37: the first two sizes for every candidate, then the smaller ones.
    let (near, far) = sizes.split_at(sizes.len().min(2));
    let found = [near, far].iter().find_map(|group| candidates.iter().find_map(|&t| group.iter().find(|&&s| text.width(face, s, t) <= room).map(|&s| (t.to_string(), s))));
    let (shown, size) = found.unwrap_or_else(|| {
        let (size, best) = (*sizes.last().unwrap_or(&px), parts.first().copied().unwrap_or(title));
        (text.fit(face, size, best, room), size)
    });
    (shown, size)
    })
}

/// `name`: a game's name, whose subtitle is cut off at its dash; any other title ("Today: Xenoblade") is only wrapped, as
/// its translation may be a pattern that the pieces would break.
fn fit(text: &mut Text, face: Face, px: u32, title: &str, width: i32, max_lines: usize, name: bool) -> Vec<Line> {
    let one = |t: &str, px: u32, sub: bool| Line { text: t.to_string(), px, sub, literal: name };
    if text.width(face, px, title) <= width {
        return vec![one(title, px, false)];
    }
    let parts = split(title).filter(|_| name);
    // Whole, at the size asked for, then a step smaller each time, down to the least size that still reads.
    let least = (px * LEAST_SHARE / 100).max(LEAST.min(px));
    let mut size = px;
    loop {
        let lines = match parts {
            Some((main, sub)) if text.words_fit(face, size, main, width) && text.words_fit(face, sub_px(size), sub, width) => {
                let mut lines: Vec<Line> = text.wrap(face, size, main, width).into_iter().map(|t| Line { text: t, px: size, sub: false, literal: name }).collect();
                lines.extend(text.wrap(face, sub_px(size), sub, width).into_iter().map(|t| Line { text: t, px: sub_px(size), sub: true, literal: name }));
                Some(lines)
            }
            None if text.words_fit(face, size, title, width) => Some(text.wrap(face, size, title, width).into_iter().map(|t| Line { text: t, px: size, sub: false, literal: name }).collect()),
            _ => None,
        };
        if let Some(lines) = lines.filter(|l| l.len() <= max_lines) {
            return lines;
        }
        let next = smaller(size);
        if next >= size || next < least {
            break;
        }
        size = next;
    }
    // Still too much: at the smaller size (smaller yet for a word wider than the line), the last line ends in "…". The
    // main title keeps its lines before the subtitle gets any.
    let size = text.title_px(face, size, title, width, LEAST);
    match parts {
        Some((main, sub)) => {
            let main_lines = text.wrap(face, size, main, width);
            if main_lines.len() >= max_lines {
                return text.wrap_at_most(face, size, main, width, max_lines).into_iter().map(|t| Line { text: t, px: size, sub: false, literal: name }).collect();
            }
            let mut lines: Vec<Line> = main_lines.into_iter().map(|t| Line { text: t, px: size, sub: false, literal: name }).collect();
            let room = max_lines - lines.len();
            lines.extend(text.wrap_at_most(face, sub_px(size), sub, width, room).into_iter().map(|t| Line { text: t, px: sub_px(size), sub: true, literal: name }));
            lines
        }
        None => text.wrap_at_most(face, size, title, width, max_lines).into_iter().map(|t| Line { text: t, px: size, sub: false, literal: name }).collect(),
    }
}

/// Where each of `lines` (from `layout`) goes: the left edge and baseline, the first baseline at `first`; `centre`
/// centres every line on `x` instead of starting it there.
pub fn positions(text: &mut Text, face: Face, lines: &[Line], x: i32, first: i32, centre: bool) -> Vec<(i32, i32)> {
    let mut y = first;
    let mut out = Vec::with_capacity(lines.len());
    for (k, l) in lines.iter().enumerate() {
        if k > 0 {
            // Between two sizes, halfway: the subtitle stands a little closer than a line of the main title would.
            y += (step(text, lines[k - 1].px) + step(text, l.px)) / 2;
        }
        let w = if l.literal { text.literal(|text| text.width(face, l.px, &l.text)) } else { text.width(face, l.px, &l.text) };
        out.push((if centre { x - w / 2 } else { x }, y));
    }
    out
}

/// Paints `lines` (`positions`). Returns the end of the last line (its right edge and its baseline), for a star after
/// the name.
#[allow(clippy::too_many_arguments)]
pub fn paint(text: &mut Text, c: &mut crate::gfx::Canvas, face: Face, lines: &[Line], x: i32, first: i32, colour: u32, sub_colour: u32, centre: bool) -> (i32, i32) {
    let mut end = (x, first);
    for (l, (lx, y)) in lines.iter().zip(positions(text, face, lines, x, first, centre)) {
        let w = paint_line(text, c, face, l, lx, y, if l.sub { sub_colour } else { colour });
        note(l, y);
        end = (lx + w, y);
    }
    end
}

/// Paint a measured line without sending name fragments back through i18n.
#[allow(clippy::too_many_arguments)]
pub fn paint_line(text: &mut Text, c: &mut crate::gfx::Canvas, face: Face, line: &Line, x: i32, y: i32, colour: u32) -> i32 {
    if line.literal { text.draw_name(c, face, line.px, x, y, colour, &line.text) }
    else { text.draw(c, face, line.px, x, y, colour, &line.text) }
}

/// A line painted (the tests read the lines of a picture, `text::boxes::note_title`); `paint` does it itself.
pub fn note(line: &Line, baseline: i32) {
    crate::text::boxes::note_title(line.px, line.sub, baseline, &line.text);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(text: &mut Text, title: &str, width: i32, max: usize) -> Vec<(String, u32)> {
        crate::i18n::learn_names([title]);
        layout(text, Face::Title, 67, title, width, max).into_iter().map(|l| (l.text, l.px)).collect()
    }

    #[test]
    fn a_subtitle_is_a_smaller_line_of_its_own() {
        let mut t = Text::builtin();
        let room = t.width(Face::Title, 67, "The Legend of Zelda") + 10;
        let got = lines(&mut t, "The Legend of Zelda - Ocarina of Time", room, 3);
        assert_eq!(got.iter().map(|l| l.0.as_str()).collect::<Vec<_>>(), ["The Legend of Zelda", "Ocarina of Time"]);
        assert!(got[1].1 < got[0].1, "{got:?}");
    }

    #[test]
    fn a_name_that_fits_one_line_stays_whole() {
        let mut t = Text::builtin();
        let got = lines(&mut t, "Castlevania - Aria of Sorrow", 4000, 3);
        assert_eq!(got, [("Castlevania - Aria of Sorrow".to_string(), 67)]);
    }

    #[test]
    fn three_lines_before_the_type_shrinks_and_only_then_an_ellipsis() {
        let mut t = Text::builtin();
        let title = "Donkey Kong Country 3 - Dixie Kong's Double Trouble!";
        let room = t.width(Face::Title, 67, "Donkey Kong Country") + 10;
        let got = lines(&mut t, title, room, 3);
        assert!(got.len() <= 3, "{got:?}");
        assert_eq!(got.last().map(|l| l.0.ends_with('…')), Some(false), "whole in three lines (or a step smaller): {got:?}");
        // No room at all for three lines: smaller first, then cut.
        let tight = t.width(Face::Title, 67, "Donkey") + 4;
        let cut = lines(&mut t, title, tight, 3);
        assert!(cut.len() <= 3 && cut.last().is_some_and(|l| l.0.ends_with('…')), "{cut:?}");
        assert!(cut[0].1 < 67, "the type went smaller before the cut: {cut:?}");
    }

    #[test]
    fn a_title_that_is_no_game_name_is_wrapped_not_split() {
        let mut t = Text::builtin();
        let title = "Today: Xenoblade Chronicles Definitive Edition";
        let room = t.width(Face::Title, 67, "Today: Xenoblade") + 10;
        let got = layout(&mut t, Face::Title, 67, title, room, 3);
        assert!(got.iter().all(|l| !l.sub), "{got:?}");
        assert!(is_whole(&got, title), "{got:?}");
    }

    #[test]
    fn a_result_row_keeps_what_tells_the_game_apart_before_it_cuts() {
        let mut t = Text::builtin();
        let (dx, plain) = ("The Legend of Zelda: Link's Awakening DX", "The Legend of Zelda: Link's Awakening");
        crate::i18n::learn_names([dx, plain]);
        // Room for the whole name: as it is, at the size asked for.
        assert_eq!(row(&mut t, Face::Title, 37, dx, 4000), (dx.to_string(), 37));
        // Too narrow even a step down: the subtitle, which ends in "DX", is not cut.
        let room = t.width(Face::Title, 37, "Link's Awakening DX") + 4;
        let (shown, px) = row(&mut t, Face::Title, 37, dx, room);
        assert_eq!((shown.as_str(), px), ("Link's Awakening DX", 37));
        let (other, _) = row(&mut t, Face::Title, 37, plain, room);
        assert_ne!(shown, other);
        // A numbered main part loses its first words, not its number.
        let dkc = "Donkey Kong Country 3 - Dixie Kong's Double Trouble!";
        crate::i18n::learn_names([dkc]);
        let room = t.width(Face::Title, 37, "Donkey Kong Country 3") + 4;
        let (shown, _) = row(&mut t, Face::Title, 37, dkc, room);
        assert_eq!(shown, "Donkey Kong Country 3");
        // Nothing fits: the end is cut, last of all.
        let room = t.width(Face::Title, 21, "Link's Awak");
        let (cut, _) = row(&mut t, Face::Title, 37, dx, room);
        assert!(cut.ends_with('…'), "{cut}");
    }

    #[test]
    fn split_wants_a_title_on_both_sides() {
        assert_eq!(split("Pokemon - Origin HeartGold"), Some(("Pokemon", "Origin HeartGold")));
        assert_eq!(split("Zelda: Link's Awakening"), Some(("Zelda", "Link's Awakening")));
        assert_eq!(split("R-Type"), None);
        assert_eq!(split("Tetris"), None);
    }
}
