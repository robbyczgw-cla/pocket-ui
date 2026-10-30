//! The interface's shared parts (docs/DESIGN.md, "Design language"): the hint bar in its two places, a list row in
//! the Settings style, the scroll bar, a panel, a sheet and an empty page's picture with its words. Pages paint with
//! these instead of copies of their own, so a selected row, a hint key or an empty page looks and measures the same
//! wherever it is. Everything takes its colours from `pal()` (an overlay over a game paints them in
//! `palette::scoped(0, …)`) and its sizes from `type_scale`, so Text size and the appearance reach every page alike.

use crate::gfx::Canvas;
use crate::pad::Button;
use crate::palette::{pal, MARGIN, VOID};
use crate::text::{Face, Text};
use crate::touch::{hint_button, Area, HitMap};
use crate::type_scale;
use crate::ui_art::{self, Art};

/// The Settings family's grid: NextUI's menu metrics (padding 10, pill 30, button 20 at its scale 1) times three.
/// The edge of the screen to a pill or a band.
pub const PAD: i32 = 30;
/// A pill's height (the status pill, the hint pill, the pause menu's title).
pub const PILL: i32 = 90;
/// A key chip in the hint pill.
pub const BUTTON: i32 = 60;
/// From a pill's or a band's end to its text.
pub const BTN_PAD: i32 = 36;
/// The pause menu's items and title.
pub const FONT: u32 = 46;
/// Headings in a pill's row, explanations, the hint pill's labels.
pub const SMALL: u32 = 30;
/// A list row: 66 px around Body 36 (taller with larger text: `row`).
pub const ROW: i32 = 66;
pub const ROW_FONT: u32 = 36;
/// The corners of a panel and of the safe-mode box.
pub const PANEL_R: i32 = 24;
/// The inside of a panel: its text starts this far from its left edge, as a row's does from the band's end.
pub const PANEL_PAD: i32 = BTN_PAD;

/// A list row's height at the text size in use.
pub fn row() -> i32 {
    type_scale::row(ROW, ROW_FONT)
}

/// The hint keys at the bottom left of a page of the shelf family (Home and its looks, the game card, console
/// pages, search, Curator, the first start), as loose keys on the page: a round key cap with the key's name, then its
/// label in Dust. Labels that don't fit go, last first, and their keys stay (`type_scale::hint_labels_that_fit`).
/// Returns where each key can be tapped.
pub fn page_hints(c: &mut Canvas, text: &mut Text, hints: &[(&str, &str)]) -> Vec<(Area, Button)> {
    let (mut x, base) = (MARGIN, c.h - 52);
    let mut areas = Vec::new();
    // The keys are 36 px around Body 21, taller with larger text (`type_scale`).
    let key_h = type_scale::row(36, 21);
    let key_top = base - 9 - key_h / 2 - type_scale::grow(21) / 3;
    // One letter in a round key, a word in a key as wide as it.
    let widths: Vec<(i32, i32)> = hints.iter().map(|(k, l)| (text.width(Face::Body, 21, k), text.width(Face::Body, 21, l))).collect();
    // The 36 px between hints remains even when a label goes. Count it with every key, not with labels that
    // may be hidden; otherwise each hidden label leaves an unbudgeted gap and can push START off the edge.
    let shown = type_scale::hint_labels_that_fit(&widths.iter().map(|(kw, lw)| ((kw + 24).max(key_h) + 10 + 36, *lw)).collect::<Vec<_>>(), c.w - MARGIN - 24 + 36);
    for (k, (key, label)) in hints.iter().enumerate() {
        let kw = widths[k].0;
        let key_w = (kw + 24).max(key_h);
        let start = x;
        c.round_rect(x, key_top, key_w, key_h, key_h / 2, pal().keycap);
        text.draw(c, Face::Body, 21, x + (key_w - kw) / 2, base - 1, pal().chalk, key);
        x += key_w + 10;
        if k < shown {
            x += text.draw(c, Face::Body, 21, x, base, pal().dust, label);
        }
        x += 36;
        if let Some(b) = hint_button(key) {
            areas.push((Area::new(start - 8, base - 46, x - start - 20, 64), b));
        }
    }
    // For the snapshot checks of Settings → Text size: nothing else may run into the hint bar.
    crate::text::boxes::area(c, (MARGIN - 4, key_top, x - 36 - MARGIN + 8, key_h), "‹hints›");
    areas
}

/// The hint keys of the Settings family (Settings and its pages, quick settings, the pause menu, dialogs, safe mode)
/// in a pill at the bottom right: a chip per key, then its label. Labels that don't fit go, last first, and their keys
/// stay. Registers each key with `hits`; returns where the pill starts (a description beside it ends 24 px short).
pub fn pill_hints(c: &mut Canvas, text: &mut Text, hits: &mut HitMap, hints: &[(&str, &str)]) -> i32 {
    let gap = (PILL - BUTTON) / 2;
    let (w, widths) = pill_layout(text, c.w, hints);
    let (x0, y0) = (c.w - PAD - w, c.h - PAD - PILL);
    c.round_rect(x0, y0, w, PILL, PILL / 2, pal().pill);
    // For the snapshot checks of Settings → Text size: nothing else may run under the pill.
    crate::text::boxes::area(c, (x0, y0, w, PILL), "‹hints›");
    let mut x = x0 + gap;
    for ((key, label), (kw, lw)) in hints.iter().zip(widths) {
        let bw = (kw + 16).max(BUTTON);
        let start = x;
        c.round_rect(x, y0 + gap, bw, BUTTON, BUTTON / 2, pal().chip);
        let tw = text.width(Face::Body, SMALL, key);
        let base = y0 + PILL / 2 + type_scale::mid(SMALL);
        text.draw(c, Face::Body, SMALL, x + (bw - tw) / 2, base, pal().pill, key);
        x += bw + gap;
        if lw >= 0 {
            text.draw(c, Face::Body, SMALL, x, base, pal().soft, label);
        }
        x += lw + gap * 2;
        if let Some(b) = hint_button(key) {
            hits.button(Area::new(start - gap, y0 - 8, x - start - gap, PILL + 16), b);
        }
    }
    x0
}

/// How wide the hint pill of `hints` is on a screen `screen_w` wide (`pill_hints`).
pub fn pill_width(text: &mut Text, screen_w: i32, hints: &[(&str, &str)]) -> i32 {
    pill_layout(text, screen_w, hints).0
}

/// The pill's width and each key's (key, label) widths; a label that doesn't fit is -gap wide (left out).
fn pill_layout(text: &mut Text, screen_w: i32, hints: &[(&str, &str)]) -> (i32, Vec<(i32, i32)>) {
    let gap = (PILL - BUTTON) / 2;
    let mut widths: Vec<(i32, i32)> = hints.iter().map(|(k, l)| (text.width(Face::Body, SMALL, k).max(BUTTON - 16), text.width(Face::Body, SMALL, l))).collect();
    // Larger text (`type_scale`) or a long translation: labels that don't fit on the line go, last first, and their
    // keys stay.
    let shown = type_scale::hint_labels_that_fit(&widths.iter().map(|(kw, lw)| ((kw + 16).max(BUTTON) + gap, lw + gap * 2)).collect::<Vec<_>>(), screen_w - 2 * PAD - gap);
    for w in widths.iter_mut().skip(shown) {
        w.1 = -gap;
    }
    let inner: i32 = widths.iter().map(|(kw, lw)| (kw + 16).max(BUTTON) + gap + lw + gap * 2).sum();
    (inner + gap, widths)
}

/// The foot of a page of the Settings family: the hint pill at the bottom right (`pill_hints`) and the explanation of
/// the selected row at the left, never under the pill: its last line on the pill's line, the two lowest lines in the
/// column beside the pill and the ones above across the page (`footer_lines`), at most `lines` lines (the last shown
/// ending with "…"), in `color` (`pal().text`, or `pal().alert` for something that went wrong). A line break in
/// `desc` starts a new line. Returns the y of the explanation's top, so a list can stop above it.
#[allow(clippy::too_many_arguments)]
pub fn footer(c: &mut Canvas, text: &mut Text, hits: &mut HitMap, hints: &[(&str, &str)], desc: &str, lines: usize, color: u32) -> i32 {
    let pill_x = if hints.is_empty() { c.w - PAD } else { pill_hints(c, text, hits, hints) };
    let pill_top = c.h - PAD - PILL;
    let wrapped = footer_lines(text, desc, c.w, pill_x, lines);
    let (left, step, last) = (PAD + BTN_PAD, type_scale::line(SMALL, 10), footer_baseline(c.h));
    for (n, line) in wrapped.iter().enumerate() {
        text.draw(c, Face::Body, SMALL, left, last - (wrapped.len() - 1 - n) as i32 * step, color, line);
    }
    (last - wrapped.len() as i32 * step).min(pill_top)
}

/// The lines `footer` paints `desc` in on a screen `screen_w` wide with the hint pill starting at `pill_x`: wrapped
/// from the bottom up, the two lowest (beside the pill) to the column left of it, the rest to the page's width, each
/// paragraph (a line of `desc`) on lines of its own. At most `lines`: a longer one is wrapped to the column alone,
/// the last line shown ending with "…".
pub fn footer_lines(text: &mut Text, desc: &str, screen_w: i32, pill_x: i32, lines: usize) -> Vec<String> {
    let left = PAD + BTN_PAD;
    let (column, wide) = (pill_x - 24 - left, screen_w - 2 * left);
    let paragraphs: Vec<&str> = desc.trim().lines().filter(|l| !l.trim().is_empty()).collect();
    let mut budget = 2usize;
    let mut parts: Vec<Vec<String>> = Vec::new();
    for p in paragraphs.iter().rev() {
        if budget == 0 {
            parts.push(text.wrap(Face::Body, SMALL, p, wide));
            continue;
        }
        let narrow = text.wrap(Face::Body, SMALL, p, column);
        let k = budget.min(narrow.len());
        budget -= k;
        if k == narrow.len() {
            parts.push(narrow);
            continue;
        }
        // The lines beside the pill are filled from the paragraph's end, the rest stands above across the page: a
        // lone word stays off the pill's line ("a TV."). The lines are the interface's language already, so they are
        // measured as they are.
        let mut words: Vec<&str> = narrow.iter().flat_map(|l| l.split_whitespace()).collect();
        let mut beside: Vec<String> = Vec::new();
        for _ in 0..k {
            let mut line: Vec<&str> = Vec::new();
            while let Some(w) = words.last() {
                let candidate = std::iter::once(*w).chain(line.iter().copied()).collect::<Vec<_>>().join(" ");
                if !line.is_empty() && text.width(Face::Body, SMALL, &candidate) > column {
                    break;
                }
                line.insert(0, *w);
                words.pop();
            }
            beside.insert(0, line.join(" "));
        }
        // A short line above gets words back from the first line beside the pill until the two are about as long
        // ("LCD grid imitates a handheld screen," over "scanlines and CRT a TV.", not "LCD grid" alone on top).
        let mut part = if words.is_empty() { Vec::new() } else { text.wrap(Face::Body, SMALL, &words.join(" "), wide) };
        if let ([above], Some(first)) = (part.as_mut_slice(), beside.first_mut()) {
            while text.width(Face::Body, SMALL, above) < text.width(Face::Body, SMALL, first) {
                let Some((word, rest)) = first.split_once(' ') else { break };
                let longer = format!("{above} {word}");
                if text.width(Face::Body, SMALL, &longer) > wide {
                    break;
                }
                *above = longer;
                *first = rest.to_string();
            }
        }
        part.extend(beside);
        parts.push(part);
    }
    let out: Vec<String> = parts.into_iter().rev().flatten().collect();
    if out.len() <= lines {
        return out;
    }
    let mut out: Vec<String> = Vec::new();
    for p in paragraphs {
        let room = lines.saturating_sub(out.len());
        if room == 0 {
            break;
        }
        out.extend(text.wrap_at_most(Face::Body, SMALL, p, column, room));
    }
    out
}

/// Where `footer` puts its last line when the explanation stands beside the pill: the baseline on the pill's line.
pub fn footer_baseline(screen_h: i32) -> i32 {
    screen_h - PAD - PILL / 2 + type_scale::mid(SMALL)
}

/// The selection of a Settings-style list at (x, y) in a row `w` x `h`: the band across, and the name's paper pill
/// `name_w` wide at its left end (the pulled-out label's paper, as everywhere a row is picked). The caller paints the
/// name in `pal().ink` over the pill.
pub fn selection(c: &mut Canvas, x: i32, y: i32, w: i32, h: i32, name_w: i32) {
    selection_in(c, x, y, w, h, name_w, pal().band);
}

/// `selection` with the band in another colour: the pause menu's pages use the game's system stripe.
pub fn selection_in(c: &mut Canvas, x: i32, y: i32, w: i32, h: i32, name_w: i32, band: u32) {
    let r = (h - 8) / 2;
    c.round_rect(x, y + 4, w, h - 8, r, band);
    c.round_rect(x, y + 4, name_w.min(w), h - 8, r, pal().select);
}

/// A list row in the Settings style: the name `BTN_PAD` in, on the band and its pill when selected. Returns its text
/// baseline.
pub fn row_label(c: &mut Canvas, text: &mut Text, (x, y, w, h): (i32, i32, i32, i32), font: u32, label: &str, selected: bool) -> i32 {
    let base = y + h / 2 + type_scale::mid(font);
    if selected {
        selection(c, x, y, w, h, text.width(Face::Body, font, label) + 2 * BTN_PAD);
        text.draw(c, Face::Body, font, x + BTN_PAD, base, pal().ink, label);
    } else {
        text.draw(c, Face::Body, font, x + BTN_PAD, base, pal().text, label);
    }
    base
}

/// A row of a shelf-family list (the first start's lists, Settings' pages there, the search's results): a spine-coloured
/// bar, the selected one the pulled-out label's paper, lifted off a light page by the label's shadow (the selection in
/// Light was paper on paper, the weakest in the app: Muse's review A11). The caller paints ink on the selected one.
pub fn shelf_row(c: &mut Canvas, x: i32, y: i32, w: i32, h: i32, selected: bool) {
    c.rect(x, y, w, h, if selected { pal().label } else { pal().spine });
    if selected {
        crate::game_card::lift_shadow(c, x, y + h, w, pal().t);
    }
}

/// The bar at the right edge of a list longer than its window: a 4 px track `track` high from `top`, its thumb where
/// the `visible` rows from `first` sit among `len`.
pub fn scrollbar(c: &mut Canvas, top: i32, track: i32, first: usize, visible: usize, len: usize) {
    if len <= visible || len == 0 {
        return;
    }
    let (bar_h, bar_y) = (track * visible as i32 / len as i32, track * first as i32 / len as i32);
    c.rect(c.w - 12, top, 4, track, pal().pill);
    c.rect(c.w - 12, top + bar_y, 4, bar_h.max(12), pal().chip);
}

/// A panel: a live readout or a state that is not a choice (Performance & fan's "Now", safe mode's "What happened"),
/// a rounded box in the pill colour. Its text starts `PANEL_PAD` in.
pub fn panel(c: &mut Canvas, x: i32, y: i32, w: i32, h: i32) {
    c.round_rect(x, y, w, h, PANEL_R, pal().pill);
}

/// A sheet over a dimmed page (a dialog, a picker): the page colour with a hairline round it, and on paper the
/// label's soft shadow under it so it lifts off the page.
pub fn sheet(c: &mut Canvas, x: i32, y: i32, w: i32, h: i32) {
    // For the snapshot checks of Settings → Text size: the page under the sheet is dimmed, not read.
    crate::text::boxes::covered();
    c.rect(x, y, w, h, pal().void);
    if pal().t > 0 {
        c.frame(x, y, w, h, 1, pal().pick(VOID, 0xd6d0c4));
        crate::game_card::lift_shadow(c, x, y + h, w, pal().t);
    } else {
        c.frame(x, y, w, h, 1, pal().pill);
    }
}

/// A fact's value that leads to a list of games (Connections: the game's developer, series, genre on the info page and
/// the box's back): the value, then a small drawn arrow in `quiet`; the selected one, both on a pill of `pill` with
/// `on_pill`, the way a shelf list's selection is paper with ink. `base` is the text's baseline; the value is cut with
/// "…" to stay inside `w` with its arrow and the pill's ends. Returns the width it took from `x`.
pub fn link_value(c: &mut Canvas, text: &mut Text, (x, base, w): (i32, i32, i32), value: &str, selected: bool, (fg, quiet, pill, on_pill): (u32, u32, u32, u32)) -> i32 {
    const FONT: u32 = 28;
    let (gap, end) = (12, 14);
    let a = type_scale::wide(10, FONT);
    let v = text.fit(Face::Body, FONT, value, w - gap - a - end);
    let vw = text.width(Face::Body, FONT, &v);
    let mid = base - type_scale::mid(FONT);
    if selected {
        let h = type_scale::row(44, FONT) - 6;
        c.round_rect(x - end, mid - h / 2, vw + gap + a + 2 * end, h, h / 2, pill);
    }
    text.draw(c, Face::Body, FONT, x, base, if selected { on_pill } else { fg }, &v);
    arrow(c, x + vw + gap, mid, a, if selected { on_pill } else { quiet });
    vw + gap + a
}

/// A small arrow pointing right, `w` wide, its point on `mid`.
fn arrow(c: &mut Canvas, x: i32, mid: i32, w: i32, color: u32) {
    let s = (w * 7 / 10).max(3);
    for dy in -s..=s {
        c.rect(x, mid + dy, (w * (s - dy.abs()) / s).max(1), 1, color);
    }
}

/// An empty page: its picture (`art`, an `ui_art` vignette) above the words, the heading in Title 37 and what to do in
/// Body 30 Quiet, left aligned at `x` and at most `w` wide, the heading's baseline at `base` (moved down as far as the
/// picture needs; the picture no higher than `top`). At most three lines of explanation, the last ending with "…"
/// above `bottom`. Returns the y under the words. Where the room can't take the picture, it is left out, never the
/// words. (The search keeps its own: its picture stands right of the words, the keyboard may take the room below.)
#[allow(clippy::too_many_arguments)]
pub fn empty_state(c: &mut Canvas, text: &mut Text, art: Option<Art>, (x, w): (i32, i32), top: i32, base: i32, bottom: i32, heading: &str, line: &str) -> i32 {
    let room = |c: &Canvas, art: Art| ui_art::size(art, ui_art::empty_scale(c.h)).1 + 28 + 3 * type_scale::line(SMALL, 12) < bottom - top - 60;
    let dy = match art {
        Some(art) if room(c, art) => ui_art::above_words(c, art, x, false, top, base, 37),
        _ => 0,
    };
    let base = base + dy;
    let heading = text.fit(Face::Title, 37, heading, w);
    text.draw(c, Face::Title, 37, x, base, pal().text, &heading);
    let step = type_scale::line(SMALL, 12);
    let mut y = base + 50 + type_scale::grow(SMALL);
    let lines = text.wrap_at_most(Face::Body, SMALL, line, w, 3);
    for l in lines {
        if y > bottom {
            break;
        }
        text.draw(c, Face::Body, SMALL, x, y, pal().quiet, &l);
        y += step;
    }
    y
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_scroll_bar_is_only_there_when_the_list_is_longer_than_its_window() {
        let mut buf = vec![0u32; 200 * 100];
        let mut c = Canvas::new(&mut buf, 200, 100);
        scrollbar(&mut c, 0, 100, 0, 5, 5);
        assert!(buf.iter().all(|p| *p == 0));
        let mut c = Canvas::new(&mut buf, 200, 100);
        scrollbar(&mut c, 0, 100, 5, 5, 10);
        assert_eq!(buf[20 * 200 + 188], pal().pill, "the track");
        assert_eq!(buf[70 * 200 + 188], pal().chip, "the thumb, in the lower half for the second half of the list");
    }

    #[test]
    fn a_selected_row_is_paper_with_ink_on_a_band() {
        let mut buf = vec![0u32; 600 * 80];
        let mut c = Canvas::new(&mut buf, 600, 80);
        selection(&mut c, 0, 0, 600, 66, 200);
        assert_eq!(buf[33 * 600 + 100], pal().select);
        assert_eq!(buf[33 * 600 + 400], pal().band);
        assert_eq!(pal().select, pal().label, "the selected name is on the label's paper");
    }
}
