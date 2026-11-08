//! The interface's shared parts (docs/DESIGN.md, "Design language"): the hint bar in its two places, a list row in
//! the Settings style, the scroll bar, a panel, a sheet and an empty page's picture with its words. Pages paint with
//! these instead of copies of their own, so a selected row, a hint key or an empty page looks and measures the same
//! wherever it is. Everything takes its colours from `pal()` (an overlay over a game paints them in
//! `palette::scoped(0, …)`) and its sizes from `type_scale`, so Text size and the appearance reach every page alike.

use crate::button_prompts;
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

/// A screen of 721 to 899 lines (1024x768, painted natively): the pages are made for 960 and more, so what stands
/// between a list and the footer is tighter there. 640x480 and the other small panels are painted at 1280x960 and are not "short".
pub fn short_screen(h: i32) -> bool {
    (721..900).contains(&h)
}
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

/// At most this many hints in a bar, and in Easy (the looks for a first-time player) this many: a bar is read at a glance.
pub const MAX_HINTS: usize = 4;
pub const EASY_HINTS: usize = 3;

/// The priority of each hint (0 is dropped last): A, B, HOME and START first, which every page needs, then the rest in
/// the order the page lists them.
fn hint_ranks(hints: &[(&str, &str)]) -> Vec<usize> {
    hints.iter().enumerate().map(|(i, (k, _))| ["A", "B", "HOME", "START"].iter().position(|f| f == k).unwrap_or(4 + i)).collect()
}

/// `hints` cut to at most `max`: whole hints, the lowest priority first, the rest in their order (Easy has three).
pub fn top_hints<'a>(hints: &[(&'a str, &'a str)], max: usize) -> Vec<(&'a str, &'a str)> {
    let keep = type_scale::hints_that_fit(&vec![0; hints.len()], &hint_ranks(hints), 0, max);
    hints.iter().zip(keep).filter(|(_, k)| *k).map(|(h, _)| *h).collect()
}

/// The hints of `hints` that stay (whole, never a key without its word): at most `MAX_HINTS`, and only as many as
/// fit in `room` pixels, given each one's width.
fn hints_shown<'a>(hints: &[(&'a str, &'a str)], widths: &[i32], room: i32) -> Vec<(usize, (&'a str, &'a str))> {
    let keep = type_scale::hints_that_fit(widths, &hint_ranks(hints), room, MAX_HINTS);
    hints.iter().copied().enumerate().zip(keep).filter(|(_, k)| *k).map(|(h, _)| h).collect()
}

/// The top edge of the keys of `page_hints` on a screen `h` px high: what a page paints above them stays above this.
pub fn hint_bar_top(h: i32) -> i32 {
    h - 52 - 9 - type_scale::row(36, 21) / 2 - type_scale::grow(21) / 3
}

/// The hint keys at the bottom left of a page of the shelf family (Home and its looks, the game card, console
/// pages, search, Curator, the first start), as loose keys on the page: a round key cap with the key's name, then its
/// label in Dust. Never a key without its word: at most four hints, and when they don't fit whole hints go, the least
/// needed first (A, B, HOME and START stay longest; `type_scale::hints_that_fit`). Returns where each key can be tapped.
pub fn page_hints(c: &mut Canvas, text: &mut Text, hints: &[(&str, &str)]) -> Vec<(Area, Button)> {
    #[cfg(feature = "brick")]
    let concise: Vec<_> = hints.iter().map(|&(key, label)|
        (key, if crate::device::brick() && short_screen(c.h) && label == "Recent games" { "Recent" } else { label })).collect();
    #[cfg(feature = "brick")]
    let hints = concise.as_slice();
    let (mut x, base) = (MARGIN, c.h - 52);
    let mut areas = Vec::new();
    // Between two hints: a little less on 1024x768, where the larger Brick type would otherwise push the fourth hint out.
    let gap = if short_screen(c.h) { 24 } else { 36 };
    // The keys are 36 px around Body 21, taller with larger text (`type_scale`).
    let key_h = type_scale::row(36, 21);
    let key_top = hint_bar_top(c.h);
    // One letter in a round key, a word in a key as wide as it.
    let widths: Vec<(i32, i32)> = hints.iter().map(|(k, l)| (text.width(Face::Body, 21, button_prompts::shown(k)), text.width(Face::Body, 21, l))).collect();
    let whole: Vec<i32> = widths.iter().map(|(kw, lw)| (kw + 24).max(key_h) + 10 + lw + gap).collect();
    let shown = hints_shown(hints, &whole, c.w - MARGIN - 24 + gap);
    for (k, (key, label)) in shown {
        let key_w = (widths[k].0 + 24).max(key_h);
        let start = x;
        key_cap(c, text, (x, key_top, key_w, key_h), key, 21, base - 1, (pal().keycap, pal().chalk));
        x += key_w + 10;
        x += text.draw(c, Face::Body, 21, x, base, pal().dust, label);
        x += gap;
        crate::nav_probe::hint(c, (start, key_top, x - start - gap, key_h), key, label);
        if let Some(b) = hint_button(key) {
            areas.push((Area::new(start - 8, base - 46, x - start - gap + 16, 64), b));
        }
    }
    // For the snapshot checks of Settings → Text size: nothing else may run into the hint bar.
    crate::text::boxes::area(c, (MARGIN - 4, key_top, x - gap - MARGIN + 8, key_h), "‹hints›");
    areas
}

/// The hint keys of the Settings family (Settings and its pages, quick settings, the pause menu, dialogs, safe mode)
/// in a pill at the bottom right: a chip per key, then its label. At most four hints; when they don't fit, whole hints
/// go, the least needed first (`page_hints`). Registers each key with `hits`; returns where the pill starts (a
/// description beside it ends 24 px short).
pub fn pill_hints(c: &mut Canvas, text: &mut Text, hits: &mut HitMap, hints: &[(&str, &str)]) -> i32 {
    let gap = (PILL - BUTTON) / 2;
    let (w, shown) = pill_layout(text, c.w, hints);
    let (x0, y0) = (c.w - PAD - w, c.h - PAD - PILL);
    if crate::retro::on() {
        crate::retro::frame(c, x0, y0, w, PILL, pal().pill);
    } else {
        c.round_rect(x0, y0, w, PILL, PILL / 2, pal().pill);
    }
    // For the snapshot checks of Settings → Text size: nothing else may run under the pill.
    crate::text::boxes::area(c, (x0, y0, w, PILL), "‹hints›");
    let mut x = x0 + gap;
    for ((key, label), (kw, lw)) in shown {
        let bw = (kw + 16).max(BUTTON);
        let start = x;
        let base = y0 + PILL / 2 + type_scale::mid(SMALL);
        key_cap(c, text, (x, y0 + gap, bw, BUTTON), key, SMALL, base, (pal().chip, pal().pill));
        x += bw + gap;
        text.draw(c, Face::Body, SMALL, x, base, pal().soft, label);
        x += lw + gap * 2;
        crate::nav_probe::hint(c, (start, y0, x - start - gap, PILL), key, label);
        if let Some(b) = hint_button(key) {
            hits.button(Area::new(start - gap, y0 - 8, x - start - gap, PILL + 16), b);
        }
    }
    x0
}

/// A hint key: its round cap (`x`, `y`, `w`, `h`) in `bg` and `key` centred on it in Body `px` in `fg`, its baseline at
/// `base`, as the Button symbols in use show it (`button_prompts`): A, B, X and Y on Xbox's colours or as the
/// PlayStation shapes, every other key as it is. Every hint key goes through here (both hint bars, the manual's chips).
pub fn key_cap(c: &mut Canvas, text: &mut Text, (x, y, w, h): (i32, i32, i32, i32), key: &str, px: u32, base: i32, (bg, fg): (u32, u32)) {
    let (bg, fg) = button_prompts::colours(key).unwrap_or((bg, fg));
    if crate::retro::on() {
        crate::retro::cap(c, x, y, w, h, bg);
    } else {
        c.round_rect(x, y, w, h, h / 2, bg);
    }
    let shown = button_prompts::shown(key);
    let tw = text.width(Face::Body, px, shown);
    text.draw(c, Face::Body, px, x + (w - tw) / 2, base, fg, shown);
}

/// How wide the hint pill of `hints` is on a screen `screen_w` wide (`pill_hints`).
pub fn pill_width(text: &mut Text, screen_w: i32, hints: &[(&str, &str)]) -> i32 {
    pill_layout(text, screen_w, hints).0
}

/// The pill's width and the hints it shows with each key's and label's width (`pill_hints`).
#[allow(clippy::type_complexity)]
fn pill_layout<'a>(text: &mut Text, screen_w: i32, hints: &[(&'a str, &'a str)]) -> (i32, Vec<((&'a str, &'a str), (i32, i32))>) {
    let gap = (PILL - BUTTON) / 2;
    let widths: Vec<(i32, i32)> = hints.iter().map(|(k, l)| (text.width(Face::Body, SMALL, button_prompts::shown(k)).max(BUTTON - 16), text.width(Face::Body, SMALL, l))).collect();
    // Larger text (`type_scale`) or a long translation: whole hints that don't fit on the line go.
    let whole: Vec<i32> = widths.iter().map(|(kw, lw)| (kw + 16).max(BUTTON) + gap + lw + gap * 2).collect();
    let shown: Vec<((&str, &str), (i32, i32))> = hints_shown(hints, &whole, screen_w - 2 * PAD - gap).into_iter().map(|(i, h)| (h, widths[i])).collect();
    let inner: i32 = shown.iter().map(|(_, (kw, lw))| (kw + 16).max(BUTTON) + gap + lw + gap * 2).sum();
    (inner + gap, shown)
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
            parts.push(text.wrap_prose(Face::Body, SMALL, p, wide));
            continue;
        }
        let narrow = text.wrap_prose(Face::Body, SMALL, p, column);
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
        let mut part = if words.is_empty() { Vec::new() } else { text.wrap_prose(Face::Body, SMALL, &words.join(" "), wide) };
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
        out.extend(text.wrap_prose_at_most(Face::Body, SMALL, p, column, room));
    }
    out
}

/// Where `footer` puts its last line when the explanation stands beside the pill: the baseline on the pill's line.
pub fn footer_baseline(screen_h: i32) -> i32 {
    screen_h - PAD - PILL / 2 + type_scale::mid(SMALL)
}

/// The selection of a list row at (x, y), `w` x `h` (docs/DESIGN.md "Lists"): one sheet of the pulled-out label's
/// paper across the whole row, 4 px in from its top and bottom, lifted off a light page by the label's shadow. It was
/// a paper pill round the name running into a grey band, which looked half loaded (the visual review 2026-11-04, 2).
/// Everything on it is `pal().ink`, quieter parts `pal().label_dust`.
pub fn selection(c: &mut Canvas, x: i32, y: i32, w: i32, h: i32) {
    crate::nav_probe::focus(c, (x, y, w, h));
    let (y, h) = (y + 4, h - 8);
    if crate::retro::on() {
        crate::retro::bar(c, x, y, w, h);
        return;
    }
    c.round_rect(x, y, w, h, h / 2, pal().select);
    crate::game_card_model::lift_shadow(c, x + h / 2, y + h, w - h, pal().t);
}

/// `selection` with the game's system stripe as a short bar inside its left end, like a case's spine: the pause
/// menu's pages.
pub fn selection_striped(c: &mut Canvas, x: i32, y: i32, w: i32, h: i32, stripe: u32) {
    selection(c, x, y, w, h);
    let bar = (h - 8) / 2;
    c.round_rect(x + 12, y + h / 2 - bar / 2, 6, bar, 3, stripe);
}

/// A list row in the Settings style: the name `BTN_PAD` in, on the selection's paper when selected. Returns its text
/// baseline.
pub fn row_label(c: &mut Canvas, text: &mut Text, (x, y, w, h): (i32, i32, i32, i32), font: u32, label: &str, selected: bool) -> i32 {
    let base = y + h / 2 + type_scale::mid(font);
    crate::nav_probe::row(c, (x, y, w, h));
    if selected {
        selection(c, x, y, w, h);
    }
    text.draw(c, Face::Body, font, x + BTN_PAD, base, if selected { pal().ink } else { pal().text }, label);
    base
}

/// A drawn angle in the icons' manner (one colour, round ends): pointing right (`>`) or left (`<`), `h` tall, its
/// point at `tip` on the line `mid`. Rows and values use it instead of the letters ">" and "‹ ›".
pub fn chevron(c: &mut Canvas, tip: i32, mid: i32, h: i32, right: bool, color: u32) {
    angle(c, tip, mid, h, right, color, if right { ">" } else { "<" });
}

/// `chevron`, recorded for the snapshot checks as `name`: the angle is a mark like the letter it replaces ("›" of a
/// switch, ">" of a page), and nothing may run into it.
fn angle(c: &mut Canvas, tip: i32, mid: i32, h: i32, right: bool, color: u32, name: &str) {
    let r = (h / 12).max(1);
    let half = (h / 2 - r).max(1);
    let reach = half * 2 / 3;
    for k in 0..=half {
        let dx = reach * k / half;
        let x = if right { tip - r - dx } else { tip + r + dx };
        c.disc(x, mid - k, r, color);
        c.disc(x, mid + k, r, color);
    }
    let w = reach + 2 * r + 1;
    crate::text::boxes::area(c, (if right { tip - w } else { tip }, mid - h / 2, w, h), name);
}

/// How a row's value reads (`row_value`): its words, and its marks.
fn value_parts(value: &str) -> (&str, bool, bool) {
    if let Some(v) = value.strip_prefix("‹ ").and_then(|v| v.strip_suffix(" ›")) {
        return (v, true, false);
    }
    match value.strip_suffix('>') {
        Some(v) => (v.trim_end(), false, true),
        None => (value, false, false),
    }
}

/// A row's value at the right of a list, its right edge at `right`, on the baseline `base`: "Home net >" (a row that
/// opens a page) with a drawn angle after the words, "‹ On ›" (the selected row's switch, Left/Right change it)
/// between two. The angles stand where the letters were, so a finger's areas measured on the string still fit. On the
/// selected row all of it is ink on the paper; elsewhere the words are `pal().text` and the angle `pal().dust`, so
/// every row says whether it opens a page before the cursor gets there. Returns the width it took.
pub fn row_value(c: &mut Canvas, text: &mut Text, font: u32, right: i32, base: i32, value: &str, selected: bool) -> i32 {
    let colours = if selected { (pal().ink, pal().ink) } else { (pal().text, pal().dust) };
    row_value_in(c, text, font, right, base, value, colours)
}

/// `row_value` with the words in `fg` and the angles in `mark`: a list whose paper glides to the selection blends
/// them as it passes (`app_motion::under`).
pub fn row_value_in(c: &mut Canvas, text: &mut Text, font: u32, right: i32, base: i32, value: &str, (fg, mark): (u32, u32)) -> i32 {
    let (words, switch, opens) = value_parts(value);
    let mid = base - type_scale::mid(font);
    let px = type_scale::px(font) as i32;
    let ww = text.width(Face::Body, font, words);
    if switch {
        let (lw, rw) = (text.width(Face::Body, font, "‹ "), text.width(Face::Body, font, " ›"));
        let x = right - rw - ww - lw;
        let h = px * 9 / 20;
        angle(c, x + 2, mid, h, false, mark, "‹");
        text.draw(c, Face::Body, font, x + lw, base, fg, words);
        angle(c, right - 2, mid, h, true, mark, "›");
        return lw + ww + rw;
    }
    if opens {
        let aw = text.width(Face::Body, font, " >");
        let tail = if words.is_empty() { aw - text.width(Face::Body, font, " ") } else { aw };
        text.draw(c, Face::Body, font, right - tail - ww, base, fg, words);
        chevron(c, right - 2, mid, px * 11 / 20, true, mark);
        return ww + tail;
    }
    text.draw_right(c, Face::Body, font, right, base, fg, words);
    ww
}

/// A row of a shelf-family list (the first start's lists, Settings' pages there, the search's results): a spine-coloured
/// bar, the selected one the pulled-out label's paper, lifted off a light page by the label's shadow (the selection in
/// Light was paper on paper, the weakest in the app: Muse's review A11). The caller paints ink on the selected one.
pub fn shelf_row(c: &mut Canvas, x: i32, y: i32, w: i32, h: i32, selected: bool) {
    crate::nav_probe::row(c, (x, y, w, h));
    if selected {
        crate::nav_probe::focus(c, (x, y, w, h));
    }
    if crate::retro::on() {
        if selected {
            crate::retro::bar(c, x, y, w, h);
        } else {
            crate::retro::notched(c, x, y, w, h, pal().spine);
        }
        return;
    }
    c.rect(x, y, w, h, if selected { pal().label } else { pal().spine });
    if selected {
        crate::game_card_model::lift_shadow(c, x, y + h, w, pal().t);
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
    if crate::retro::on() {
        crate::retro::frame(c, x, y, w, h, pal().pill);
        return;
    }
    c.round_rect(x, y, w, h, PANEL_R, pal().pill);
}

/// A sheet over a dimmed page (a dialog, a picker): the page colour with a hairline round it, and on paper the
/// label's soft shadow under it so it lifts off the page.
pub fn sheet(c: &mut Canvas, x: i32, y: i32, w: i32, h: i32) {
    // For the snapshot checks of Settings → Text size: the page under the sheet is dimmed, not read.
    crate::text::boxes::covered();
    if crate::retro::on() {
        crate::retro::frame(c, x, y, w, h, pal().void);
        return;
    }
    c.rect(x, y, w, h, pal().void);
    if pal().t > 0 {
        c.frame(x, y, w, h, 1, pal().pick(VOID, 0xd6d0c4));
        crate::game_card_model::lift_shadow(c, x, y + h, w, pal().t);
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
    let lines = text.wrap_prose_at_most(Face::Body, SMALL, line, w, 3);
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
    fn a_hint_bar_shows_at_most_four_whole_hints_and_the_needed_ones_stay() {
        let shelf = [("A", "Play"), ("HOME", "Quick settings"), ("X", "Details"), ("Y", "Star"), ("SELECT", "Recent games"), ("START", "Settings")];
        let four = top_hints(&shelf, MAX_HINTS);
        assert_eq!(four, [("A", "Play"), ("HOME", "Quick settings"), ("X", "Details"), ("START", "Settings")], "Star and Recent games go, whole");
        let easy = top_hints(&shelf, EASY_HINTS);
        assert_eq!(easy, [("A", "Play"), ("HOME", "Quick settings"), ("START", "Settings")], "Easy has three");
        // The pause menu: START (the way to the quick settings) outlasts SELECT.
        let pause = [("START", "Quick settings"), ("SELECT", "Recent games"), ("B", "Back"), ("A", "OK")];
        assert_eq!(top_hints(&pause, 3), [("START", "Quick settings"), ("B", "Back"), ("A", "OK")]);
        let few = [("B", "Back"), ("A", "OK")];
        assert_eq!(top_hints(&few, MAX_HINTS), few, "nothing goes that fits");
    }

    #[test]
    fn only_the_native_768_line_screens_are_short() {
        // 1024x768 and its neighbours are painted natively; the small panels (640x480 is painted at 1280x960) and
        // everything from 900 lines up keep the design's spacing.
        for h in [768, 800, 720 + 1, 899] {
            assert!(short_screen(h), "{h}");
        }
        for h in [480, 720, 900, 960, 1080, 1200] {
            assert!(!short_screen(h), "{h}");
        }
    }

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
    fn a_selected_row_is_one_sheet_of_paper() {
        let mut buf = vec![0u32; 600 * 80];
        let mut c = Canvas::new(&mut buf, 600, 80);
        selection(&mut c, 0, 0, 600, 66);
        assert_eq!(buf[33 * 600 + 100], pal().select);
        assert_eq!(buf[33 * 600 + 400], pal().select, "no band: the paper runs to the row's end");
        assert_eq!(pal().select, pal().label, "the selected row is on the label's paper");
    }

    #[test]
    fn a_value_names_its_marks() {
        assert_eq!(value_parts("Home net >"), ("Home net", false, true));
        assert_eq!(value_parts(">"), ("", false, true));
        assert_eq!(value_parts("‹ On ›"), ("On", true, false));
        assert_eq!(value_parts("Off"), ("Off", false, false));
    }

    #[test]
    fn an_angle_is_drawn_inside_its_box() {
        let mut buf = vec![0u32; 40 * 40];
        let mut c = Canvas::new(&mut buf, 40, 40);
        chevron(&mut c, 30, 20, 18, true, 1);
        let lit: Vec<(usize, usize)> = (0..40 * 40).filter(|i| buf[*i] == 1).map(|i| (i % 40, i / 40)).collect();
        assert!(!lit.is_empty());
        assert!(lit.iter().all(|(x, y)| (18..=30).contains(x) && (11..=29).contains(y)), "{lit:?}");
        assert!(lit.iter().any(|(x, y)| *x >= 28 && *y == 20), "the point at the right, on the middle line");
    }

    /// The size sweep (1240x1080, German) found "ist." alone on the pill's line under a two-line explanation.
    #[test]
    fn a_footer_paragraph_does_not_end_on_one_word() {
        let mut text = Text::new();
        text.set_translate(false);
        // Whatever the column, no paragraph's last line is a lone word (`Text::wrap`).
        let desc = "Entfällt, solange die Farbkorrektur des Emulators an ist.";
        for column in (300..1100).step_by(7) {
            let out = footer_lines(&mut text, desc, column + 300, column + 24 + PAD + BTN_PAD, 5);
            if out.len() >= 2 {
                assert!(out.last().unwrap().contains(' '), "column {column}: {out:?}");
            }
        }
    }
}

/// A QR code of `data`, `size` pixels square including a white margin of `quiet` modules (4 is the standard's; the
/// pairing codes use 2), dark modules in black, centred in `size` across, at `y`. `even`: each module an even number of
/// pixels, so that a 640×480 panel (the logical canvas halved) keeps them sharp.
pub fn qr(c: &mut Canvas, x: i32, y: i32, size: i32, data: &str, quiet: i32, even: bool) {
    use qrcode::{Color, EcLevel, QrCode};
    let Ok(code) = QrCode::with_error_correction_level(data, EcLevel::M) else {
        return;
    };
    let modules = code.width() as i32;
    let mut cell = (size / (modules + 2 * quiet)).max(1);
    if even && cell >= 4 {
        cell &= !1;
    }
    let total = cell * (modules + 2 * quiet);
    let (x, y) = (x + (size - total) / 2, y);
    c.round_rect(x, y, total, total, 12, 0xffffff);
    for (i, color) in code.to_colors().into_iter().enumerate() {
        if color == Color::Dark {
            let (mx, my) = (i as i32 % modules, i as i32 / modules);
            c.rect(x + (quiet + mx) * cell, y + (quiet + my) * cell, cell, cell, 0x000000);
        }
    }
}
