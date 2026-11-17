//! The pause menu of the handhelds (Linux, `main.rs` `paint_menu`) for Android's player when the game came from the
//! Handheld view: the game's name at the top, the rows under each other at the left (Continue, Save, Load, Earlier,
//! Options, Save & quit), the selected one on the paper pill with its value ("‹ 2 ›", "‹ 12 min ago ›"), a dark window
//! at the right with the save's picture and a line under it, and the hint bar at the bottom right. The same sizes
//! (`kit`), colours (`pal`) and Retro frames as Linux's, so a player who knows one knows the other (the Odin hand test,
//! 15.11.2026: "the pause menu is completely different on Android").
//!
//! Its second page is the same list style for Options: a title, full-width rows with their values at the right, the
//! sections as quiet headings, a line about the selected row at the bottom left.
//!
//! Painted by the player (`android/src/player.rs`); it lives here so the host's tests can render it beside Linux's.

use crate::gfx::Canvas;
use crate::image::Image;
use crate::kit::{self, BTN_PAD, FONT, PAD, PILL, SMALL};
use crate::pad::Button;
use crate::palette::pal;
use crate::save_media;
use crate::text::{Face, Text};
use crate::touch::Area;
use crate::type_scale;

/// What a row of the main page does. The player matches its actions on these, never on the words.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Item {
    Continue,
    Save,
    Load,
    Earlier,
    Options,
    Achievements,
    Quit,
}

impl Item {
    /// Whether A opens a page (Options, Achievements): the row ends in the drawn angle on every row and A's hint says
    /// Open, as Linux's (`main.rs` `MainItem::opens_page`, 0.5.1).
    pub fn opens_page(self) -> bool {
        matches!(self, Item::Options | Item::Achievements)
    }
}

/// A row of the main page: its words, its value (the slot, Earlier's time) and whether Left/Right step that value.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub item: Item,
    pub label: String,
    pub chip: Option<String>,
    pub steps: bool,
    /// Grey: Load on an empty slot, Earlier with nothing yet, Hardcore.
    pub enabled: bool,
}

/// The slots under the picture, as Linux's row of small blocks (`save_media::paint_row`).
pub struct Slots<'a> {
    pub kind: save_media::Kind,
    pub filled: &'a [bool],
    pub ring: Option<usize>,
    pub icon: save_media::Icon<'a>,
}

/// The window at the right while Save, Load or Earlier is selected.
pub struct Preview<'a> {
    pub picture: Option<&'a Image>,
    /// What the window says without a picture: "Empty", "No picture", "None yet".
    pub empty: &'a str,
    /// The line under it ("Slot 2 · saved 5 min ago", "Go back to 12 min ago …").
    pub note: String,
    pub slots: Option<Slots<'a>>,
}

/// The main page.
pub struct Main<'a> {
    pub title: &'a str,
    /// What the last action did ("Saved to slot 2") or why it couldn't: under the title, as Linux's save-trouble line.
    pub status: &'a str,
    pub rows: &'a [Row],
    pub sel: usize,
    pub preview: Option<Preview<'a>>,
    /// The line beside the rows for a row without a picture (Save & quit: "Next time, A continues from this moment.").
    pub note: Option<String>,
    pub hints: &'a [(&'a str, &'a str)],
}

/// Where things were painted: each row (for a finger) and the hint keys.
#[derive(Default)]
pub struct Out {
    pub rows: Vec<Area>,
    pub hints: Vec<(Area, Button)>,
}

/// Retro's frame pixel, the gap after the rows' window and the room after a row's words (Linux's `RETRO_*`).
const U: i32 = crate::retro::U;
const RETRO_ROW_END: i32 = 6 * U;
const RETRO_GAP: i32 = 6 * U;

fn retro_icon(item: Item) -> crate::retro::Icon {
    use crate::retro::Icon;
    match item {
        Item::Continue => Icon::Continue,
        Item::Save => Icon::Save,
        Item::Load | Item::Earlier => Icon::Load,
        Item::Options => Icon::Options,
        Item::Achievements => Icon::Playing,
        Item::Quit => Icon::Quit,
    }
}

/// Linux's `retro_list_w`: at least half the screen less 80, at most what leaves the preview a third of it.
fn retro_list_w(w: i32, need: i32) -> i32 {
    let (least, most) = (w / 2 - PAD - 80, w - PAD - (w / 3 + 32) - RETRO_GAP - PAD);
    (need.clamp(least, most.max(least)) + U - 1) / U * U
}

/// The title row: the game's name (never translated) in Title 50 at the top left, as every page's.
fn paint_title(c: &mut Canvas, text: &mut Text, title: &str, literal: bool) {
    let room = c.w - 2 * PAD - 2 * BTN_PAD;
    let base = PAD + PILL / 2 + type_scale::mid(50);
    if literal {
        text.literal(|text| {
            let fitted = text.fit(Face::Title, 50, title, room);
            text.draw(c, Face::Title, 50, PAD + BTN_PAD, base, pal().chalk, &fitted);
        });
    } else {
        let fitted = text.fit(Face::Title, 50, title, room);
        text.draw(c, Face::Title, 50, PAD + BTN_PAD, base, pal().chalk, &fitted);
    }
}

/// The main page over a canvas the caller has darkened (the paused game behind it, as Linux's).
pub fn paint_main(c: &mut Canvas, text: &mut Text, v: &Main) -> Out {
    let mut out = Out::default();
    let retro = crate::retro::on();
    paint_title(c, text, v.title, true);
    // The last action's line under the title; the rows start below it.
    let mut below_title = PAD + PILL;
    if !v.status.is_empty() {
        let lines = text.wrap_prose_at_most(Face::Body, SMALL, v.status, c.w / 2 - PAD, 2);
        for (k, line) in lines.iter().enumerate() {
            text.draw(c, Face::Body, SMALL, PAD + BTN_PAD, below_title + type_scale::px(SMALL) as i32 + k as i32 * type_scale::line(SMALL, 8), pal().dust, line);
        }
        below_title += lines.len() as i32 * type_scale::line(SMALL, 8) + 8;
    }
    let frame_room = if retro { 6 * U } else { 0 };
    let n = v.rows.len().max(1) as i32;
    let step = PILL.min((c.h - PAD - PILL - below_title - 24 - frame_room) / n);
    let top = below_title + 24;
    let icon_k = (2 * step.min(80) / 80).max(1);
    let icon_x = PAD + 10 * U;
    let text_x = if retro { icon_x + crate::retro::ICON * icon_k + 4 * U } else { PAD + BTN_PAD };
    let list_w = if retro {
        let need = v.rows.iter().map(|r| {
            let chip_w = r.chip.as_ref().map_or(0, |t| SMALL as i32 / 2 + text.width(Face::Body, FONT, t));
            let angle_w = if r.item.opens_page() { SMALL as i32 / 2 + text.width(Face::Body, FONT, ">") } else { 0 };
            text.width(Face::Body, FONT, &r.label) + chip_w + angle_w
        }).max().unwrap_or(0);
        retro_list_w(c.w, text_x + need + RETRO_ROW_END - PAD)
    } else {
        0
    };
    let list_top = top - 4 * U;
    if retro {
        crate::retro::frame(c, PAD, list_top, list_w, n * step + 8 * U, pal().pill);
        crate::retro::frame_lines_are_no_text(c, PAD, list_top, list_w, n * step + 8 * U);
    }
    let selected = v.rows.get(v.sel).map(|r| r.item);
    let beside = v.preview.is_some();
    for (i, row) in v.rows.iter().enumerate() {
        let y = top + i as i32 * step;
        let on = i == v.sel;
        let label_w = text.width(Face::Body, FONT, &row.label);
        let baseline = y + step / 2 + type_scale::mid(FONT);
        // The value: "‹ 2 ›" while selected and it steps, else as it is; cut short before the preview window.
        let chip = row.chip.as_ref().map(|name| {
            let room = if retro {
                PAD + list_w - RETRO_ROW_END - (text_x + label_w + SMALL as i32 / 2)
            } else if beside {
                c.w - PAD - (c.w / 2 + 24) - 16 - (PAD + BTN_PAD + label_w + SMALL as i32 / 2) - BTN_PAD
            } else {
                i32::MAX / 2
            };
            let full = if on && row.steps { format!("‹ {name} ›") } else { name.clone() };
            if text.width(Face::Body, FONT, &full) <= room { full } else { text.fit(Face::Body, FONT, name, room.max(0)) }
        });
        let chip_w = chip.as_ref().map_or(0, |t| SMALL as i32 / 2 + text.width(Face::Body, FONT, t));
        // A row that opens a page ends in the drawn angle on every row: Retro's at the window's right edge, the plain
        // look's after the name, inside the selected pill (Linux's `paint_menu_page`).
        let angle_w = if row.item.opens_page() { SMALL as i32 / 2 + text.width(Face::Body, FONT, ">") } else { 0 };
        let fg = if on { pal().ink } else if !row.enabled { if retro { pal().off } else { 0x7f7f7f } } else if retro { pal().text } else { 0xffffff };
        if retro {
            if on {
                crate::retro::bar(c, PAD + 3 * U, y + 2, list_w - 6 * U, step - 4);
            }
            let icon_y = y + (step - crate::retro::ICON * icon_k) / 2;
            crate::retro::icon(c, retro_icon(row.item), icon_x, icon_y, icon_k);
            let label = text.fit(Face::Body, FONT, &row.label, PAD + list_w - RETRO_ROW_END - text_x - angle_w);
            text.draw(c, Face::Body, FONT, text_x, baseline, fg, &label);
            out.rows.push(Area::new(PAD, y, list_w, step));
        } else {
            let w = label_w + chip_w + angle_w + 2 * BTN_PAD;
            if on {
                crate::nav_probe::focus(c, (PAD, y, w, step));
                c.round_rect(PAD, y, w, step, step / 2, pal().select);
            }
            text.draw(c, Face::Body, FONT, PAD + BTN_PAD, baseline, fg, &row.label);
            out.rows.push(Area::new(PAD, y, w.max(c.w / 3), step));
        }
        if row.item.opens_page() {
            let right = if retro { PAD + list_w - RETRO_ROW_END } else { text_x + label_w + angle_w };
            kit::row_value_in(c, text, FONT, right, baseline, ">", (fg, fg));
        }
        if let Some(chip) = chip {
            let x = text_x + label_w + SMALL as i32 / 2;
            let color = if on { pal().label_dust } else if retro { pal().dust } else { 0x9a9a9a };
            let right = x + text.width(Face::Body, FONT, &chip);
            kit::row_value_in(c, text, FONT, right, baseline, &chip, (color, color));
        }
    }
    if let Some(p) = &v.preview {
        paint_preview(c, text, p, retro, list_w, list_top, selected);
    } else if let Some(note) = &v.note {
        // Beside the rows, right-aligned at the middle of the screen, as Linux's `menu_note`.
        let room = if retro { c.w - PAD - BTN_PAD - (PAD + list_w + RETRO_GAP) } else { c.w / 2 - PAD };
        let lines = text.wrap_prose(Face::Body, SMALL, note, room);
        let line_h = type_scale::line(SMALL, 8);
        let y0 = c.h / 2 - (lines.len() as i32 - 1) * line_h / 2;
        for (k, line) in lines.iter().enumerate() {
            text.draw_right(c, Face::Body, SMALL, c.w - PAD - BTN_PAD, y0 + k as i32 * line_h, 0xcccccc, line);
        }
    }
    out.hints = kit::page_hints(c, text, v.hints);
    out
}

/// Linux's preview window: half the screen at the right, the picture keeping its shape inside, the slots and the line
/// under it.
fn paint_preview(c: &mut Canvas, text: &mut Text, p: &Preview, retro: bool, list_w: i32, list_top: i32, _item: Option<Item>) {
    let (hw, hh) = (c.w / 2, c.h / 2 - 5 * type_scale::grow(SMALL));
    let r = if retro { 16 } else { 12 };
    let note_tail = 48 + 34 + type_scale::line(SMALL, 8) + type_scale::px(SMALL) as i32 / 3 + 12;
    let (top, bottom) = (PAD + PILL + 16, c.h - PAD - PILL);
    let hh = hh.min((bottom - top - 2 * r - note_tail).max(1));
    let hw = if retro { hw.min(c.w - PAD - 2 * r - (PAD + list_w + RETRO_GAP)) } else { hw };
    let ox = c.w - PAD - hw - 2 * r;
    let centred = if retro { list_top } else { (c.h - hh - 2 * r) / 2 };
    let oy = centred.min(bottom - hh - 2 * r - note_tail).max(top);
    if retro {
        crate::retro::frame(c, ox, oy, hw + 2 * r, hh + 2 * r, pal().pill);
        crate::retro::frame_lines_are_no_text(c, ox, oy, hw + 2 * r, hh + 2 * r);
    } else {
        c.round_rect(ox, oy, hw + 2 * r, hh + 2 * r, r * 2, 0x262626);
    }
    c.rect(ox + r, oy + r, hw, hh, 0x000000);
    match p.picture {
        Some(img) => {
            let (iw, ih) = img.fit(hw, hh);
            c.blit_scaled(img, ox + r + (hw - iw) / 2, oy + r + (hh - ih) / 2, iw, ih);
        }
        None => {
            let tw = text.width(Face::Body, FONT, p.empty);
            text.draw(c, Face::Body, FONT, ox + r + (hw - tw) / 2, oy + r + hh / 2 + type_scale::mid(FONT), pal().text, p.empty);
        }
    }
    let mut note_y = oy + hh + 2 * r + 48;
    if let Some(s) = &p.slots {
        if s.filled.len() > 1 {
            save_media::paint_row(c, s.kind, ox + (hw + 2 * r) / 2, oy + hh + 2 * r + 26, ROW_BLOCK, ROW_GAP, s.filled, s.ring, s.icon);
        }
    }
    note_y += 28 + 6;
    let room = if retro { hw + 2 * r } else { 2 * (c.w - 8 - (ox + hw / 2 + r)) };
    for (k, line) in text.wrap_prose_at_most(Face::Body, SMALL, &p.note, room, 2).iter().enumerate() {
        let w = text.width(Face::Body, SMALL, line);
        text.draw(c, Face::Body, SMALL, ox + (hw + 2 * r - w) / 2, note_y + k as i32 * type_scale::line(SMALL, 8), 0xcccccc, line);
    }
}

/// Linux's slot blocks under the picture (`app_save_media`'s `ROW_BLOCK`, `ROW_GAP`).
const ROW_BLOCK: i32 = 30;
const ROW_GAP: i32 = 12;

/// A row of a list page: a section's heading, or a row with its value at the right.
#[derive(Clone, Debug, PartialEq)]
pub enum ListRow {
    Heading(String),
    /// Its words, its value ("‹ On ›", "This game", "" for a row that does something), whether it opens a page (">").
    Row { label: String, value: String, opens: bool, enabled: bool },
}

/// A list page (Options, the Advanced group): `rows` from the top, `sel` the index in `rows` of the selected one (never
/// a heading), `desc` the line at the bottom left.
pub struct List<'a> {
    pub title: &'a str,
    pub rows: &'a [ListRow],
    pub sel: usize,
    pub desc: &'a str,
    pub hints: &'a [(&'a str, &'a str)],
}

/// Where each list row was painted (None: scrolled out or a heading) and the hint keys.
#[derive(Default)]
pub struct ListOut {
    pub rows: Vec<Option<Area>>,
    pub hints: Vec<(Area, Button)>,
    /// The selected row's explanation as painted, line by line, and the top of its first line.
    pub desc: Vec<String>,
    pub desc_top: i32,
}

/// The lines the list keeps free for the selected row's explanation over the hint pill, as Linux's Options page
/// (`main.rs` `paint_options`: "room for up to five lines"); a longer one takes more and the list scrolls above it.
pub const DESC_ROOM: usize = 5;

/// A list page in Linux's Options style: full-width rows, the selected one on the paper, scrolled so it is in view.
pub fn paint_list(c: &mut Canvas, text: &mut Text, v: &List) -> ListOut {
    let mut out = ListOut { rows: vec![None; v.rows.len()], ..Default::default() };
    paint_title(c, text, v.title, false);
    let row_h = kit::row();
    let head_h = type_scale::line(SMALL, 20);
    let top = PAD + PILL;
    // The explanation is never cut ("Takes effect the next time the game starts." went under "…" on the Mini V2,
    // 17.11.): it is wrapped as Linux's footer wraps it (`kit::footer_lines`: its two lowest lines beside the hint pill,
    // the rest across the page) with no limit, and the list ends above the room it needs, at least DESC_ROOM lines.
    // Where the painted hint pill starts (`page_hints`, the same concise hints).
    let pill_x = kit::page_hints_left(text, c.w, c.h, v.hints);
    let px = kit::footer_px(text, v.desc, pill_x);
    let step = type_scale::line(px, 10);
    let desc = if v.desc.trim().is_empty() { Vec::new() } else { kit::footer_lines(text, v.desc, c.w, pill_x, usize::MAX) };
    let last = kit::footer_baseline(c.h);
    let desc_top = last - (desc.len() as i32 - 1).max(0) * step - type_scale::px(px) as i32;
    let room_top = last - (desc.len().max(DESC_ROOM) as i32 - 1) * step - type_scale::px(px) as i32;
    // The selected row always shows: on a screen too short for the list and the room, the room gives way first.
    let bottom = kit::content_bottom(c.h).min(room_top - 12).max(top + row_h);
    out.desc_top = desc_top;
    let height = |r: &ListRow| if matches!(r, ListRow::Heading(_)) { head_h } else { row_h };
    // The first row shown: as few scrolled away as keep the selected row (and its heading) in view.
    let mut first = 0;
    loop {
        let used: i32 = v.rows[first..=v.sel.min(v.rows.len().saturating_sub(1))].iter().map(height).sum();
        if used <= bottom - top || first >= v.sel {
            break;
        }
        first += 1;
    }
    let (x, w) = (PAD, c.w - 2 * PAD);
    let mut y = top;
    for (i, row) in v.rows.iter().enumerate().skip(first) {
        let h = height(row);
        if y + h > bottom {
            break;
        }
        match row {
            ListRow::Heading(name) => {
                text.draw(c, Face::Body, SMALL, x + BTN_PAD, y + h - 12, pal().dust, name);
            }
            ListRow::Row { label, value, opens, enabled } => {
                let on = i == v.sel;
                let base = y + h / 2 + type_scale::mid(kit::ROW_FONT);
                crate::nav_probe::row(c, (x, y, w, h));
                if on {
                    kit::selection(c, x, y, w, h);
                }
                let fg = if on { pal().ink } else if *enabled { pal().text } else { pal().off };
                let shown = if *opens { format!("{value} >") } else if on && !value.is_empty() { format!("‹ {value} ›") } else { value.clone() };
                let value_w = if shown.is_empty() { 0 } else { text.width(Face::Body, kit::ROW_FONT, &shown) + 24 };
                let label = text.fit(Face::Body, kit::ROW_FONT, label, w - 2 * BTN_PAD - value_w);
                text.draw(c, Face::Body, kit::ROW_FONT, x + BTN_PAD, base, fg, &label);
                if !shown.is_empty() {
                    let colours = if on { (pal().ink, pal().ink) } else { (pal().text, pal().dust) };
                    kit::row_value_in(c, text, kit::ROW_FONT, x + w - BTN_PAD, base, &shown, colours);
                }
                out.rows[i] = Some(Area::new(x, y, w, h));
            }
        }
        y += h;
    }
    for (k, line) in desc.iter().enumerate() {
        text.draw(c, Face::Body, px, PAD + BTN_PAD, last - (desc.len() - 1 - k) as i32 * step, pal().dust, line);
    }
    out.desc = desc;
    out.hints = kit::page_hints(c, text, v.hints);
    out
}

/// Earlier's value beside the row: how long ago, said in full so the row explains itself ("12 min ago", "just now"),
/// never a bare "12 min" (the Odin hand test: "Earlier" wasn't understood, 15.11.).
pub fn earlier_chip(then: u64, now: u64) -> String {
    crate::play_time::ago_from(then, now)
}

/// The line under Earlier's picture: what A does, and how the moments come about.
pub fn earlier_note(then: Option<u64>, now: u64) -> String {
    match then {
        Some(t) => format!("Back to the moment from {} · one is kept every 5 min of play", crate::play_time::ago_from(t, now)),
        None => "Nothing earlier yet · a moment is kept every 5 min of play".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(sel_steps: bool) -> Vec<Row> {
        let r = |item, label: &str, chip: Option<&str>| Row { item, label: label.into(), chip: chip.map(String::from), steps: sel_steps, enabled: true };
        vec![
            r(Item::Continue, "Continue", None),
            r(Item::Save, "Save", Some("1")),
            r(Item::Load, "Load", Some("1")),
            r(Item::Earlier, "Earlier", Some("12 min ago")),
            r(Item::Options, "Options", None),
            r(Item::Quit, "Save & quit", None),
        ]
    }

    /// The main page lays its rows out under each other at the left, as Linux's, and every row can be touched.
    #[test]
    fn the_rows_stand_under_each_other_at_the_left() {
        let (w, h) = (1920, 1080);
        let mut px = vec![0u32; (w * h) as usize];
        let mut c = Canvas::new(&mut px, w as u32, h as u32);
        let mut text = Text::builtin();
        let rows = rows(true);
        let out = paint_main(&mut c, &mut text, &Main { title: "Advance Wars", status: "", rows: &rows, sel: 1, preview: None, note: None, hints: &[("B", "Back"), ("A", "OK")] });
        assert_eq!(out.rows.len(), 6);
        for pair in out.rows.windows(2) {
            assert_eq!(pair[0].x, pair[1].x, "one column");
            assert!(pair[1].y >= pair[0].y + pair[0].h, "under each other");
        }
        assert!(out.rows.iter().all(|a| a.x + a.w <= w / 2 + 40), "the rows leave the right half to the picture");
        assert!(out.rows.last().unwrap().y + out.rows.last().unwrap().h <= kit::hint_bar_top(h), "above the hint bar");
    }

    /// The Odin hand test (15.11.): Android's pause menu showed no picture of a save. With Save selected the slot's picture
    /// stands in the window at the right, its shape kept, as Linux's; an empty slot says "Empty" there instead.
    #[test]
    fn the_save_s_picture_stands_beside_the_rows() {
        let (w, h) = (1920, 1080);
        let picture = Image { w: 240, h: 160, px: vec![0x00ff00; 240 * 160] };
        let rows = rows(true);
        let filled = [true, false, false, false, false, false, false, false];
        let green = |pic: Option<&Image>| {
            let mut px = vec![0u32; (w * h) as usize];
            let mut c = Canvas::new(&mut px, w as u32, h as u32);
            let mut text = Text::builtin();
            let preview = Preview { picture: pic, empty: "Empty", note: "Slot 1 · saved 5 min ago".into(), slots: Some(Slots { kind: save_media::Kind::of("gba"), filled: &filled, ring: Some(0), icon: save_media::Icon::None }) };
            paint_main(&mut c, &mut text, &Main { title: "Advance Wars", status: "", rows: &rows, sel: 1, preview: Some(preview), note: None, hints: &[("B", "Back"), ("A", "OK")] });
            let right: Vec<u32> = px.chunks(w as usize).flat_map(|row| row[w as usize / 2..].to_vec()).collect();
            let left: Vec<u32> = px.chunks(w as usize).flat_map(|row| row[..w as usize / 3].to_vec()).collect();
            (right.iter().filter(|p| **p & 0xffffff == 0x00ff00).count(), left.iter().filter(|p| **p & 0xffffff == 0x00ff00).count())
        };
        let (right, left) = green(Some(&picture));
        // At least the picture's own size (the window is half the screen wide, 3:2 kept).
        assert!(right > 240 * 160 * 9, "the picture in the window: {right} px");
        assert_eq!(left, 0, "nothing of it under the rows");
        assert_eq!(green(None).0, 0, "an empty slot shows no picture");
    }

    #[test]
    fn earlier_says_what_it_does_and_when() {
        assert_eq!(earlier_chip(1000, 1000 + 12 * 60), "12 min ago");
        assert_eq!(earlier_note(Some(1000), 1000 + 12 * 60), "Back to the moment from 12 min ago · one is kept every 5 min of play");
        assert!(earlier_note(None, 5).starts_with("Nothing earlier yet"));
    }
}
