//! The interface's shared parts (docs/DESIGN.md, "Design language"): the hint bar, a list row in
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
/// The hint bar at the bottom: its height, its keys' height and its gap to the screen's bottom edge at Normal text (the
/// owner's choice, 18.11.2026: "flachere Tipps"). It was a pill as tall as the status pill (`PILL`, 90) with 60 px keys,
/// `PAD` (30) over the edge, which took much of a small screen and floated under the shelf. With larger text the bar and
/// its keys grow with the letters (`hint_h`, `hint_key`).
pub const HINT_H: i32 = 62;
pub const HINT_KEY: i32 = 44;
pub const HINT_GAP: i32 = 10;
/// The pause menu's items and title.
pub const FONT: u32 = 46;
/// Headings in a pill's row, explanations, the hint pill's labels.
pub const SMALL: u32 = 30;

/// A screen of 720 to 899 lines (the Brick's 1024x768 and the Smart Pro's 1280x720, painted natively): the pages are made for
/// 960 and more, so what stands between a list and the footer is tighter there. 640x480 and the other small panels are painted
/// at 1280x960 and are not "short".
pub fn short_screen(h: i32) -> bool {
    (720..900).contains(&h)
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

/// The top edge of the hint bar on a screen `h` px high: what a page paints above it stays above this.
pub fn hint_bar_top(h: i32) -> i32 {
    h - HINT_GAP - hint_h()
}

/// The hint bar's height: `HINT_H`, taller by as much as its letters grow (Medium, Large, the Brick's larger Normal).
pub fn hint_h() -> i32 {
    let _one = type_scale::panel(1.0);
    HINT_H + type_scale::grow(SMALL)
}

/// A hint key's height: `HINT_KEY`, taller by as much as its letters grow.
pub fn hint_key() -> i32 {
    let _one = type_scale::panel(1.0);
    HINT_KEY + type_scale::grow(SMALL)
}

/// The middle line of the hint bar on a screen `h` px high: a line of text beside it stands on it.
pub fn hint_bar_mid(h: i32) -> i32 {
    hint_bar_top(h) + hint_h() / 2
}

/// The lowest a page's content may reach: a little over the hint bar's top edge.
pub fn content_bottom(h: i32) -> i32 {
    hint_bar_top(h) - 8
}

/// The hint keys of a page of the shelf family (Home and its looks, the game card, console pages, search, Curator, the
/// first start): the same pill as every other page's (`pill_hints`), at the bottom right. Never a key without its word;
/// at most four hints, and when they don't fit whole hints go, the least needed first. Returns where each key can be tapped.
pub fn page_hints(c: &mut Canvas, text: &mut Text, hints: &[(&str, &str)]) -> Vec<(Area, Button)> {
    let hints = concise(c.h, hints);
    paint_bar(c, text, &hints).1
}

/// Where `page_hints`' pill starts on a canvas `w` by `h`: nothing of it is left of this (a fade between two bars mixes
/// only the part from the leftmost of them, `App::paint_hint_row`).
pub fn page_hints_left(text: &mut Text, w: i32, h: i32, hints: &[(&str, &str)]) -> i32 {
    w - PAD - pill_layout(text, w, &concise(h, hints)).0
}

/// The page's hints as the bar shows them: on the Brick's short screen "Recent games" is "Recent".
fn concise<'a>(h: i32, hints: &[(&'a str, &'a str)]) -> Vec<(&'a str, &'a str)> {
    #[cfg(feature = "brick")]
    return hints.iter().map(|&(key, label)| (key, if crate::device::brick() && short_screen(h) && label == "Recent games" { "Recent" } else { label })).collect();
    #[cfg(not(feature = "brick"))]
    {
        let _ = h;
        hints.to_vec()
    }
}

/// The one hint bar of every page (docs/DESIGN.md "Hint bar"): the keys in a pill at the bottom right, a chip per key,
/// then its label, A last, next to the face buttons. At most four hints; when they don't fit, whole hints go, the least
/// needed first (A, B, HOME and START stay longest; `type_scale::hints_that_fit`). Registers each key with `hits`;
/// returns where the pill starts (a description beside it ends 24 px short).
pub fn pill_hints(c: &mut Canvas, text: &mut Text, hits: &mut HitMap, hints: &[(&str, &str)]) -> i32 {
    let (x0, areas) = paint_bar(c, text, hints);
    for (area, b) in areas {
        hits.button(area, b);
    }
    x0
}

fn paint_bar(c: &mut Canvas, text: &mut Text, hints: &[(&str, &str)]) -> (i32, Vec<(Area, Button)>) {
    // One bar on every page: a wide page's factor (`type_scale::wide_page`, Settings at 1920x1080) doesn't grow it or its
    // letters, so it stands the same over the shelf and over Settings (it was 71 there and 62 on the shelf, 18.11.2026).
    let _one = type_scale::panel(1.0);
    let (w, px, gap, shown) = pill_layout(text, c.w, hints);
    let (x0, y0) = (c.w - PAD - w, hint_bar_top(c.h));
    if crate::retro::on() {
        crate::retro::frame(c, x0, y0, w, hint_h(), pal().pill);
    } else {
        c.round_rect(x0, y0, w, hint_h(), hint_h() / 2, pal().pill);
    }
    // For the snapshot checks of Settings → Text size: nothing else may run under the pill.
    crate::text::boxes::area(c, (x0, y0, w, hint_h()), "‹hints›");
    let mut areas = Vec::new();
    let mut x = x0 + gap;
    for ((key, label), (kw, lw)) in shown {
        let bw = (kw + 16).max(hint_key());
        let start = x;
        let base = y0 + hint_h() / 2 + type_scale::mid(px);
        key_cap(c, text, (x, y0 + (hint_h() - hint_key()) / 2, bw, hint_key()), key, px, base, (pal().chip, pal().pill));
        x += bw + gap;
        text.draw(c, Face::Body, px, x, base, pal().soft, label);
        x += lw + gap * 2;
        crate::nav_probe::hint(c, (start, y0, x - start - gap, hint_h()), key, label);
        if let Some(b) = hint_button(key) {
            areas.push((Area::new(start - gap, y0 - 8, x - start - gap, hint_h() + 16), b));
        }
    }
    (x0, areas)
}

/// A hint key: its round cap (`x`, `y`, `w`, `h`) in `bg` and `key` centred on it in Body `px` in `fg`, its baseline at
/// `base`, as the Button symbols in use show it (`button_prompts`): A, B, X and Y on Xbox's colours or as the
/// PlayStation shapes, every other key as it is. Every hint key goes through here (both hint bars, the manual's chips).
pub fn key_cap(c: &mut Canvas, text: &mut Text, (x, y, w, h): (i32, i32, i32, i32), key: &str, px: u32, base: i32, (bg, fg): (u32, u32)) {
    let (bg, fg) = button_prompts::colours(key).unwrap_or_else(|| cap_colours(bg, fg));
    if crate::retro::on() {
        crate::retro::cap(c, x, y, w, h, bg);
    } else {
        c.round_rect(x, y, w, h, h / 2, bg);
    }
    let shown = button_prompts::shown(key);
    let tw = text.width(Face::Body, px, shown);
    text.draw(c, Face::Body, px, x + (w - tw) / 2, base, fg, shown);
}

/// The least contrast between a key's letters and its cap (WCAG AA for text).
pub const CAP_CONTRAST: f64 = 4.5;

/// The colours a key cap is painted in for the cap `bg` and letters `fg` a page asks for (`pal().chip`, `pal().pill`):
/// always light letters on a darker cap, at least `CAP_CONTRAST` apart. In the dark appearance the pages ask for a light
/// cap with dark letters (Retro Cocoa's cream, Warm's grey); on the Brick's bright panel the thin dark strokes of a
/// pixel face vanished in the cap's glow: L1, R1 and "< >" read as blank white keys, MENU and START as pale grey ones
/// (photos of the hand test, 17.11.2026), while the white letters on the red B and green A stayed. Here such a cap turns
/// round: the page's text colour (or the cap's own, if lighter) on the lightest mix of the two that still holds
/// `CAP_CONTRAST`. A cap already darker than its letters (the light appearance) stays as it is.
pub fn cap_colours(bg: u32, fg: u32) -> (u32, u32) {
    use crate::palette::{contrast, luminance};
    if luminance(bg) <= luminance(fg) {
        return (bg, fg);
    }
    let text = pal().text;
    let light = if luminance(text) > luminance(bg) { text } else { bg };
    for k in (0..=255u32).rev().step_by(5) {
        let cap = crate::gfx::mix(fg, bg, k);
        if luminance(cap) < luminance(light) && contrast(light, cap) >= CAP_CONTRAST {
            return (cap, light);
        }
    }
    (fg, light)
}

/// How wide the hint pill of `hints` is on a screen `screen_w` wide (`pill_hints`).
pub fn pill_width(text: &mut Text, screen_w: i32, hints: &[(&str, &str)]) -> i32 {
    pill_layout(text, screen_w, hints).0
}

/// The pill's width and the hints it shows with each key's and label's width (`pill_hints`).
#[allow(clippy::type_complexity)]
fn pill_layout<'a>(text: &mut Text, screen_w: i32, hints: &[(&'a str, &'a str)]) -> (i32, u32, i32, Vec<((&'a str, &'a str), (i32, i32))>) {
    // Measured as `paint_bar` paints it: without a wide page's factor.
    let _one = type_scale::panel(1.0);
    let hints = top_hints(hints, MAX_HINTS);
    // A fitting bar keeps its exact old spacing and type. Before losing a whole hint (French/Italian Details on the
    // Mini), close the gaps, then try slightly smaller letters. Only the last layout may drop a hint by priority.
    let layouts = [(SMALL, 15), (SMALL, 10), (SMALL, 6), (27, 6), (24, 6), (22, 6)];
    for (index, &(px, gap)) in layouts.iter().enumerate() {
        let widths: Vec<(i32, i32)> = hints.iter().map(|(k, l)| (text.width(Face::Body, px, button_prompts::shown(k)).max(hint_key() - 16), text.width(Face::Body, px, l))).collect();
        let whole: Vec<i32> = widths.iter().map(|(kw, lw)| (kw + 16).max(hint_key()) + gap + lw + gap * 2).collect();
        if whole.iter().sum::<i32>() + gap > screen_w - 2 * PAD && index + 1 < layouts.len() { continue; }
        let mut shown: Vec<_> = hints_shown(&hints, &whole, screen_w - 2 * PAD - gap).into_iter().map(|(i, h)| (h, widths[i])).collect();
        // A stands last, next to the face buttons, whichever order the page lists them in.
        shown.sort_by_key(|((key, _), _)| *key == "A");
        let inner: i32 = shown.iter().map(|(_, (kw, lw))| (kw + 16).max(hint_key()) + gap + lw + gap * 2).sum();
        return (inner + gap, px, gap, shown);
    }
    unreachable!("last layout always taken")
}

/// The foot of a page of the Settings family: the hint pill at the bottom right (`pill_hints`) and the explanation of
/// the selected row at the left, never under the pill: its last line on the pill's line, the two lowest lines in the
/// column beside the pill and the ones above across the page (`footer_lines`), at most `lines` lines (the last shown
/// ending with "…"), in `color` (`pal().text`, or `pal().alert` for something that went wrong). A line break in
/// `desc` starts a new line. Returns the y of the explanation's top, so a list can stop above it.
#[allow(clippy::too_many_arguments)]
pub fn footer(c: &mut Canvas, text: &mut Text, hits: &mut HitMap, hints: &[(&str, &str)], desc: &str, lines: usize, color: u32) -> i32 {
    let pill_x = if hints.is_empty() { c.w - PAD } else { pill_hints(c, text, hits, hints) };
    footer_text(c, text, desc, pill_x, lines, color)
}

/// `footer`'s explanation alone, beside a hint pill already painted that starts at `pill_x` (Settings' categories paint
/// their hints first). Returns the y of the explanation's top.
pub fn footer_text(c: &mut Canvas, text: &mut Text, desc: &str, pill_x: i32, lines: usize, color: u32) -> i32 {
    let pill_top = hint_bar_top(c.h);
    let wrapped = footer_lines(text, desc, c.w, pill_x, lines);
    let px = footer_px(text, desc, pill_x);
    let (left, step, last) = (PAD + BTN_PAD, type_scale::line(px, 10), footer_baseline(c.h));
    // The lines are in the interface's language already (`footer_lines`): painted as they are, never looked up again.
    let translating = text.translating();
    text.set_translate(false);
    for (n, line) in wrapped.iter().enumerate() {
        text.draw(c, Face::Body, px, left, last - (wrapped.len() - 1 - n) as i32 * step, color, line);
    }
    text.set_translate(translating);
    (last - wrapped.len() as i32 * step).min(pill_top)
}

/// The top of the capitals of `desc`'s first line as `footer` paints it in at most `lines` lines on a `w`×`h` screen
/// beside a hint pill starting at `pill_x` (`type_scale::mid` is half their height), and how many lines it takes: where
/// a list over it may end (a gap above this).
pub fn footer_top(text: &mut Text, desc: &str, w: i32, h: i32, pill_x: i32, lines: usize) -> (i32, usize) {
    let n = footer_lines(text, desc, w, pill_x, lines).len();
    let px = footer_px(text, desc, pill_x);
    (footer_baseline(h) - (n as i32 - 1).max(0) * type_scale::line(px, 10) - 2 * type_scale::mid(px), n)
}

/// The words of a line as it may break: at plain spaces only. A no-break space (French "Complet\u{a0}:") holds its two
/// sides together; `split_whitespace` broke there and joined them again with a plain space, so ":" could start a line.
/// A lone "&" holds the words on both sides of it ("Lights & buttons", a page's name): it never starts or ends a line
/// (18.11.2026: "& buttons." stood alone under "Settings › Controls › Lights" beside the flatter hint bar).
fn words_of(line: &str) -> Vec<&str> {
    // Each word's byte range in `line`.
    let mut spans: Vec<(usize, usize)> = Vec::new();
    let mut at = 0;
    for w in line.split(' ') {
        if !w.is_empty() {
            spans.push((at, at + w.len()));
        }
        at += w.len() + 1;
    }
    let mut out: Vec<(usize, usize)> = Vec::new();
    let mut i = 0;
    while i < spans.len() {
        let (mut a, mut b) = spans[i];
        // This word, and an "&" after it with the word after that.
        while i + 2 < spans.len() && &line[spans[i + 1].0..spans[i + 1].1] == "&" {
            b = spans[i + 2].1;
            i += 2;
        }
        if &line[a..b] == "&" {
            // A leading "&" (or one at the end) joins what follows (or what precedes).
            if let Some(next) = spans.get(i + 1) {
                b = next.1;
                i += 1;
            } else if let Some(prev) = out.pop() {
                a = prev.0;
            }
        }
        out.push((a, b));
        i += 1;
    }
    out.into_iter().map(|(a, b)| &line[a..b]).collect()
}

/// The lines `footer` paints `desc` in on a screen `screen_w` wide with the hint pill starting at `pill_x`: wrapped
/// from the bottom up, the two lowest (beside the pill) to the column left of it, the rest to the page's width, each
/// paragraph (a line of `desc`) on lines of its own. At most `lines`: a longer one is wrapped to the column alone,
/// the last line shown ending with "…".
pub fn footer_lines(text: &mut Text, desc: &str, screen_w: i32, pill_x: i32, lines: usize) -> Vec<String> {
    let px = footer_px(text, desc, pill_x);
    let left = PAD + BTN_PAD;
    let (column, wide) = (pill_x - 24 - left, screen_w - 2 * left);
    let paragraphs: Vec<&str> = desc.trim().lines().filter(|l| !l.trim().is_empty()).collect();
    let mut budget = 2usize;
    let mut parts: Vec<Vec<String>> = Vec::new();
    for p in paragraphs.iter().rev() {
        if budget == 0 {
            parts.push(text.wrap_prose(Face::Body, px, p, wide));
            continue;
        }
        let narrow = text.wrap_prose(Face::Body, px, p, column);
        let k = budget.min(narrow.len());
        budget -= k;
        // A word wider than the column is never broken beside the pill: it goes above, across the page ("despertarlo." in
        // Retro's even-width letters, Large Spanish at 1024x768).
        let too_wide = |w: &str, text: &mut Text| text.width_plain(Face::Body, px, w) > column;
        // The words as painted: the paragraph in the interface's language (`wrap_prose` translates it as well).
        let shown = text.translated(p).to_string();
        let unbroken = !words_of(&shown).into_iter().any(|w| too_wide(w, text));
        // A line of the plain wrap that starts or ends at a lone "&" takes the way below, which keeps it with its words.
        let at_ampersand = narrow.iter().any(|l| l.starts_with("& ") || l.ends_with(" &") || l == "&");
        if k == narrow.len() && unbroken && !at_ampersand {
            parts.push(narrow);
            continue;
        }
        // The lines beside the pill are filled from the paragraph's end, the rest stands above across the page: a
        // lone word stays off the pill's line ("a TV."). The lines are the interface's language already, so they are
        // measured and wrapped as they are: never looked up again (a piece like "Shelf, Easy," would run the language
        // table's patterns, which Settings' first frame must not, tests/settings_pages.rs).
        let translating = text.translating();
        text.set_translate(false);
        let mut words: Vec<&str> = if unbroken { narrow.iter().flat_map(|l| words_of(l)).collect() } else { words_of(&shown) };
        let mut beside: Vec<String> = Vec::new();
        for _ in 0..k {
            let mut line: Vec<&str> = Vec::new();
            while let Some(w) = words.last() {
                let candidate = std::iter::once(*w).chain(line.iter().copied()).collect::<Vec<_>>().join(" ");
                if (!line.is_empty() || too_wide(w, text)) && text.width(Face::Body, px, &candidate) > column {
                    break;
                }
                line.insert(0, *w);
                words.pop();
            }
            beside.insert(0, line.join(" "));
        }
        // A short line above gets words back from the first line beside the pill until the two are about as long
        // ("LCD grid imitates a handheld screen," over "scanlines and CRT a TV.", not "LCD grid" alone on top).
        let mut part = if words.is_empty() { Vec::new() } else { text.wrap_prose(Face::Body, px, &words.join(" "), wide) };
        if let ([above], Some(first)) = (part.as_mut_slice(), beside.first_mut()) {
            while text.width(Face::Body, px, above) < text.width(Face::Body, px, first) {
                let Some((word, rest)) = first.split_once(' ') else { break };
                let longer = format!("{above} {word}");
                if text.width(Face::Body, px, &longer) > wide {
                    break;
                }
                *above = longer;
                *first = rest.to_string();
            }
        }
        text.set_translate(translating);
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
        out.extend(text.wrap_prose_at_most(Face::Body, px, p, column, room));
    }
    out
}

/// The size `footer` paints `desc` at beside a hint pill starting at `pill_x`: `SMALL`, a step smaller when a word of
/// it (in the interface's language) is wider than the column beside the pill, which would break it in two
/// ("despertarl" over "o." in Retro's even-width letters, Large Spanish at 1024x768).
pub fn footer_px(text: &mut Text, desc: &str, pill_x: i32) -> u32 {
    let column = pill_x - 24 - (PAD + BTN_PAD);
    let shown = text.translated(desc).to_string();
    if words_of(&shown).into_iter().any(|w| text.width_plain(Face::Body, SMALL, w) > column) {
        crate::title_block::smaller(SMALL)
    } else {
        SMALL
    }
}

/// Where `footer` puts its last line when the explanation stands beside the pill: the baseline on the pill's line.
pub fn footer_baseline(screen_h: i32) -> i32 {
    hint_bar_mid(screen_h) + type_scale::mid(SMALL)
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
    angle(c, (tip, mid), h, right, color, if right { ">" } else { "<" }, (h / 12).max(1));
}

/// `chevron`, recorded for the snapshot checks as `name`: the angle is a mark like the letter it replaces ("›" of a
/// switch, ">" of a page), and nothing may run into it.
fn angle(c: &mut Canvas, (tip, mid): (i32, i32), h: i32, right: bool, color: u32, name: &str, r: i32) {
    let half = (h / 2 - r).max(1);
    let reach = half * 2 / 3;
    // A disc per step down each stroke, all of them one shape: each disc as its rows (as `Canvas::disc` paints them),
    // painted once (gfx::Canvas::fill_rects), so the discs' soft edges don't pile up where they overlap.
    let mut rows = Vec::new();
    for k in 0..=half {
        let dx = reach * k / half;
        let x = if right { tip - r - dx } else { tip + r + dx };
        for y in [mid - k, mid + k] {
            for dy in -r..=r {
                let w = ((r * r - dy * dy) as f32).sqrt() as i32;
                rows.push((x - w, y + dy, w * 2 + 1, 1));
            }
        }
    }
    c.fill_rects(&rows, color);
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
/// selected row all of it is ink on the paper; elsewhere the words and a page's angle are `pal().text` (a switch's
/// angles are only on the selected row), so every row says whether it opens a page before the cursor gets there. Returns the width it took.
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
        // A little larger than the letters' ‹ › (0.5.1): the selected row's value reads as one to step with Left/Right.
        let h = px * 11 / 20;
        angle(c, (x + 2, mid), h, false, mark, "‹", (h / 10).max(1));
        text.draw(c, Face::Body, font, x + lw, base, fg, words);
        angle(c, (right - 2, mid), h, true, mark, "›", (h / 10).max(1));
        return lw + ww + rw;
    }
    if opens {
        let aw = text.width(Face::Body, font, " >");
        let tail = if words.is_empty() { aw - text.width(Face::Body, font, " ") } else { aw };
        text.draw(c, Face::Body, font, right - tail - ww, base, fg, words);
        // A page's angle (0.5.1, a hand walk: which rows open a page was not clear enough): as tall as the letters, a
        // little bolder than the switch's, in the words' colour on every row, not the quiet one.
        let h = px * 7 / 10;
        angle(c, (right - 2, mid), h, true, fg, ">", (h / 9).max(1));
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
    crate::nav_probe::list(c, (0, top, c.w, track));
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
    // One shape of rows: painted natively, its rows don't each keep a whole screen pixel (gfx::Canvas::fill_rects).
    let rows: Vec<_> = (-s..=s).map(|dy| (x, mid + dy, (w * (s - dy.abs()) / s).max(1), 1)).collect();
    c.fill_rects(&rows, color);
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
    fn arrows_and_angles_painted_natively_are_the_old_picture_not_a_pixel_thicker() {
        // The link arrow (1 px rows) and a row's angle (a disc per step) at even and odd places on a 640x480 panel
        // painted natively: the same picture as the design canvas through the 2:1 filter (they grew a pixel at the
        // bottom and the angle's overlapping discs piled up their soft edges when each piece rounded by itself).
        use crate::gfx::tests::{old_and_new, close};
        for (x, mid) in [(20, 30), (21, 31), (20, 31), (21, 30)] {
            for w in [10, 13, 22] {
                let pair = old_and_new(60, 40, |c| arrow(c, x, mid, w, 0xffffff));
                assert_eq!(pair.0, pair.1, "arrow {w} wide at {x},{mid}");
            }
            for (h, right) in [(15, true), (24, false), (33, true)] {
                close(&format!("angle {h} at {x},{mid}"), 0, old_and_new(60, 40, |c| angle(c, (x + 30, mid), h, right, 0xf0e0d0, ">", (h / 12).max(1))));
            }
        }
    }

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
    fn only_the_native_720_to_899_line_screens_are_short() {
        // 1024x768 and 1280x720 are painted natively; the small panels (640x480 is painted at 1280x960) and
        // everything from 900 lines up keep the design's spacing.
        for h in [720, 768, 800, 899] {
            assert!(short_screen(h), "{h}");
        }
        for h in [480, 719, 900, 960, 1080, 1200] {
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

    /// The TrimUI Brick's hand test (17.11.2026, Retro Cocoa, dark): L1, R1 and "< >" were blank white keys, MENU and START pale ones,
    /// dark pixel letters lost in the light cap's glow. Every key cap the pages ask for (`pal().chip`, `pal().pill`, as
    /// both hint bars, Settings' category row and the manual do), in every set of colours, Retro's too, dark and light,
    /// Barlow and Retro letters, is painted with letters lighter than the cap and at least `CAP_CONTRAST` from it.
    #[test]
    fn every_key_cap_has_light_letters_that_stand_out_from_it() {
        use crate::font_style::{for_this_thread, FontStyle, RetroLetters};
        use crate::palette::{contrast, luminance};
        let mut text = Text::new();
        text.set_translate(false);
        let mut faults = Vec::new();
        let (w, h) = (200, 80);
        let (x, y, cw, ch) = (20, 12, 150, 56);
        for style in [FontStyle::Barlow, FontStyle::Retro] {
            for letters in [RetroLetters::default(), RetroLetters::Pixelify] {
                if style == FontStyle::Barlow && letters != RetroLetters::default() {
                    continue;
                }
                let _style = for_this_thread(style, letters);
                let sets: Vec<(String, Box<dyn Fn()>)> = if style == FontStyle::Retro {
                    crate::retro::Colours::ALL.iter().map(|&k| (format!("{k:?}"), Box::new(move || crate::retro::set_colours(k)) as Box<dyn Fn()>)).collect()
                } else {
                    crate::palette::Colours::ALL.iter().map(|&k| (format!("{k:?}"), Box::new(move || crate::palette::set_colours(k)) as Box<dyn Fn()>)).collect()
                };
                for (name, set) in sets {
                    set();
                    for t in [0u8, 255] {
                        for key in ["L1", "R1", "< >", "MENU", "START", "SELECT", "HOME", "A", "B"] {
                            crate::palette::scoped(t, || {
                                let mut buf = vec![0u32; (w * h) as usize];
                                let mut c = Canvas::new(&mut buf, w as u32, h as u32);
                                let base = y + ch / 2 + type_scale::mid(SMALL);
                                key_cap(&mut c, &mut text, (x, y, cw, ch), key, SMALL, base, (pal().chip, pal().pill));
                                // The cap is the middle band's commonest colour; its letters the pixel furthest from it.
                                let band: Vec<u32> = (y + ch / 4..y + ch * 3 / 4).flat_map(|py| (x + 8..x + cw - 8).map(move |px| (px, py))).map(|(px, py)| c.get(px, py)).collect();
                                let mut counts = std::collections::HashMap::new();
                                for p in &band {
                                    *counts.entry(*p).or_insert(0) += 1;
                                }
                                let cap = *counts.iter().max_by_key(|(_, n)| **n).unwrap().0;
                                let ink = *band.iter().max_by(|a, b| contrast(**a, cap).total_cmp(&contrast(**b, cap))).unwrap();
                                let what = format!("{style:?}/{letters:?} {name} {} \"{key}\": cap {cap:06x}, letters {ink:06x}, {:.2}:1", if t == 0 { "dark" } else { "light" }, contrast(ink, cap));
                                if luminance(ink) <= luminance(cap) {
                                    faults.push(format!("{what}: dark letters on a light cap"));
                                } else if contrast(ink, cap) < CAP_CONTRAST {
                                    faults.push(format!("{what}: too little contrast"));
                                }
                            });
                        }
                    }
                }
            }
        }
        crate::palette::set_colours(crate::palette::Colours::Warm);
        crate::retro::set_colours(crate::retro::Colours::Cocoa);
        assert!(faults.is_empty(), "{} key caps don't read:\n{}", faults.len(), faults.join("\n"));
    }

    /// "&" binds the words beside it: never at a line's start or end (a page's name, "Lights & buttons").
    #[test]
    fn a_footer_never_breaks_at_an_ampersand() {
        assert_eq!(words_of("in Lights & buttons."), vec!["in", "Lights & buttons."]);
        assert_eq!(words_of("A & B & C d"), vec!["A & B & C", "d"]);
        assert_eq!(words_of("& x"), vec!["& x"]);
        let mut text = Text::new();
        text.set_translate(false);
        let desc = "F1/F2 are shortcuts, not game buttons. Set them in Settings › Controls › Lights & buttons.";
        for column in (200..1100).step_by(9) {
            let out = footer_lines(&mut text, desc, column + 600, column + 24 + PAD + BTN_PAD, 8);
            assert!(!out.iter().any(|l| l.starts_with('&') || l.ends_with('&')), "column {column}: {out:?}");
        }
    }

    /// French typography holds "Complet\u{a0}:" together with a no-break space (Settings' Motion line, 18.11.2026):
    /// `footer_lines` split it like a plain space and joined it with one, so the line read with a plain space and ":"
    /// could start a line. At every column, the lines put back together are the line itself, and none starts with
    /// French punctuation.
    #[test]
    fn a_footer_keeps_no_break_spaces() {
        let mut text = Text::new();
        text.set_translate(false);
        let desc = "Complet\u{a0}: les sélections glissent. Réduit\u{a0}: fondus seulement. Désactivé\u{a0}: rien ne bouge.";
        for column in (200..1100).step_by(9) {
            let out = footer_lines(&mut text, desc, column + 600, column + 24 + PAD + BTN_PAD, 8);
            assert_eq!(out.join(" "), desc, "column {column}: {out:?}");
            assert!(!out.iter().any(|l| l.starts_with([':', ';', '?', '!'])), "column {column}: {out:?}");
        }
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

    /// Sleep & power in Retro's letters, Large Spanish at 1024x768 (the merge gate, 09.11.2026): "despertarlo." was wider
    /// than the column beside the hints and came out broken in two. A word too wide for the column stays whole, above.
    #[test]
    fn a_footer_word_wider_than_the_column_beside_the_pill_stays_whole() {
        let mut text = Text::new();
        text.set_translate(false);
        let desc = "Guarda el juego y suspende el Pocket. Pulsa de nuevo el botón de encendido para despertarlo.";
        let wide = text.width(Face::Body, SMALL, "despertarlo.");
        for column in [wide - 40, wide - 1] {
            let out = footer_lines(&mut text, desc, 1024, column + 24 + PAD + BTN_PAD, 4);
            let words: Vec<&str> = out.iter().flat_map(|l| l.split_whitespace()).collect();
            assert_eq!(words, desc.split_whitespace().collect::<Vec<_>>(), "column {column}: every word whole, in order: {out:?}");
            // Painted a step smaller (`footer_px`), the lines beside the pill fit its column.
            let px = footer_px(&mut text, desc, column + 24 + PAD + BTN_PAD);
            assert!(px < SMALL, "column {column}: the explanation a step smaller");
            for (i, line) in out.iter().enumerate().rev().take(2) {
                assert!(text.width(Face::Body, px, line) <= column, "column {column}: line {i} beside the pill is too wide: {out:?}");
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
