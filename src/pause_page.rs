//! The pause menu's pages as they are painted: the main page (the game's name, the rows at the left, the save's picture
//! at the right) and the Options family of pages (Options, Picture with its live preview, Emulator, Controls, Cheats,
//! Speed, rumble, fan, …: a heading, rows with their values, the explanation at the bottom left, the hints).
//!
//! One painter for both menus: Linux's (`main.rs` `paint_menu_page`, `paint_options`) and the one in Android's player
//! (`android/src/player.rs`, a game from the Handheld view). Each side gathers what a page shows from its own state
//! (Linux's from pocket-core over its FIFO, the player's from the core it runs itself) into the views here; every pixel
//! of the page comes from this file. Until 17.11.2026 Android painted a copy of its own (`pause_list`), and it drifted
//! in every part a player sees (the Retroid Pocket Mini V2 hand test: "the menus are still a bit different from Linux,
//! no preview on Picture").

use crate::gfx::Canvas;
use crate::image::Image;
use crate::kit::{self, BTN_PAD, FONT, PAD, PILL, SMALL};
use crate::palette::pal;
use crate::preview;
use crate::retro;
use crate::status_corner::Corner;
use crate::text::{Face, Text};
use crate::touch::{Area, HitMap};
use crate::type_scale;

/// Retro's pause menu: how far a row's name and value end before its window's right edge (the selection bar ends
/// three frame pixels in, the text three more), and the gap between the rows' window and the preview window.
pub const RETRO_ROW_END: i32 = 6 * retro::U;
pub const RETRO_GAP: i32 = 6 * retro::U;

/// The most lines an options page's explanation takes (the list scrolls above them).
const MOST_DESC_LINES: usize = 8;
/// The most lines an options page keeps for every row's explanation (`list_bottom`); a longer one takes its room only
/// while its row is selected.
const STABLE_DESC_LINES: usize = 5;

/// The hints of a row whose value Left/Right change.
pub const VALUE_HINTS: &[(&str, &str)] = &[("B", "Back"), ("< >", "Change")];

/// The width of Retro's rows' window on a canvas `w` wide when its rows need `need`: at least half the screen less
/// 80 (as before), at most what leaves the preview window beside it a third of the screen ("Guardar y salir" whole in
/// Large text on 1024x768); whole frame pixels.
pub fn retro_list_w(w: i32, need: i32) -> i32 {
    let (least, most) = (w / 2 - PAD - 80, w - PAD - (w / 3 + 32) - RETRO_GAP - PAD);
    let u = retro::U;
    (need.clamp(least, most.max(least)) + u - 1) / u * u
}

/// First index of a scrolled window of `visible` rows that keeps `sel` inside it.
pub fn window_start(sel: usize, visible: usize, len: usize) -> usize {
    if len <= visible {
        0
    } else {
        sel.saturating_sub(visible / 2).min(len - visible)
    }
}

/// The status corner (time, Wi-Fi while connected, battery) in a pill at the top right of a page; returns the pill's
/// left edge.
pub fn status_pill(c: &mut Canvas, text: &mut Text, corner: &Corner) -> i32 {
    let pill = crate::status_corner::Pill { right: c.w - PAD, top: PAD, h: PILL - 20, px: SMALL, pad: BTN_PAD };
    pill.paint(c, text, corner, pal().pill, pal().soft)
}

/// The line under the main page's title while the game's saves fail: a small dot in the alert colour, then the words,
/// wrapped below the title. Returns where the rows may start.
pub fn trouble_line(c: &mut Canvas, text: &mut Text, line: &str) -> i32 {
    let (x, max) = (PAD + BTN_PAD, c.w - 2 * (PAD + BTN_PAD) - 28);
    let lines = text.wrap_prose(Face::Body, SMALL, line, max);
    // At the size painted (Settings → Text size, `type_scale`).
    let px = type_scale::px(SMALL) as i32;
    let line_h = px * 4 / 3;
    // Close under the title (it has no pill since 0.2), so the rows keep nearly their normal pitch below.
    let first = PAD + PILL + px * 2 / 3;
    c.disc(x + 8, first - px * 3 / 10, 8, crate::palette::ALERT);
    for (i, l) in lines.iter().enumerate() {
        text.draw(c, Face::Body, SMALL, x + 28, first + i as i32 * line_h, 0xffffff, l);
    }
    first + (lines.len() as i32 - 1) * line_h + px / 3
}

/// A line beside the pause menu's rows, right-aligned at the middle of the screen; a long one (German, a long
/// emulator line) is wrapped into the right half instead of running into the rows.
pub fn menu_note(c: &mut Canvas, text: &mut Text, note: &str, room: i32) {
    let lines = text.wrap_prose(Face::Body, SMALL, note, room);
    let line_h = SMALL as i32 * 4 / 3;
    let top = c.h / 2 - (lines.len() as i32 - 1) * line_h / 2;
    for (i, line) in lines.iter().enumerate() {
        text.draw_right(c, Face::Body, SMALL, c.w - PAD - BTN_PAD, top + i as i32 * line_h, 0xcccccc, line);
    }
}

/// `msg` in the middle of the box (x, y, w, h).
fn centered(c: &mut Canvas, text: &mut Text, (x, y, w, h): (i32, i32, i32, i32), msg: &str) {
    let tw = text.width(Face::Body, FONT, msg);
    text.draw(c, Face::Body, FONT, x + (w - tw) / 2, y + h / 2 + type_scale::mid(FONT), pal().text, msg);
}

// ---- The main page ----

/// A row of the main page as it is painted.
#[derive(Clone, Debug, PartialEq)]
pub struct MainRow {
    /// Its words as shown ("Save & quit").
    pub label: String,
    /// Its picture in the Retro style.
    pub icon: retro::Icon,
    /// Grey when off (Load with nothing to load, Earlier with nothing yet).
    pub enabled: bool,
    /// Opens a page of its own: ends in the drawn angle, A says Open.
    pub opens: bool,
    /// Its value at rest ("3", "Auto", "1 of 3", "12 min ago").
    pub chip: Option<String>,
    /// The widest value it can show (Load: the longest of its stops), for Retro's window width; None: no value.
    pub widest: Option<String>,
    /// Selected and Left/Right step its value: "‹ 3 ›".
    pub arrows: bool,
}

/// The window at the right while Save, Load or Earlier is selected.
pub struct Window<'a> {
    pub picture: Option<&'a Image>,
    /// What the window says without a picture: "No picture", "None yet", "From the start", "Empty".
    pub empty: &'a str,
    /// The line under it.
    pub note: String,
    /// The slots stand under the picture (`MainOut::slots` says where).
    pub slots: bool,
    /// How much height the slots take for a window this wide (the slim row's 28, the medium its own).
    pub strip_h: &'a dyn Fn(i32) -> i32,
}

/// The main page.
pub struct MainPage<'a> {
    pub title: &'a str,
    /// The battery in this game at the right of the title ("About 3 h 20 min left in this game").
    pub battery: Option<&'a str>,
    /// Saves that don't reach the card: the line under the title.
    pub trouble: Option<&'a str>,
    pub rows: &'a [MainRow],
    pub sel: usize,
    /// The selected row shows the window at the right (Save, Load, Earlier, not while quitting).
    pub window: Option<Window<'a>>,
    /// Quitting: what the page says instead of a note ("Saving and quitting…").
    pub quitting: Option<&'a str>,
    /// The line beside the rows for a row without the window.
    pub note: Option<String>,
    pub hints: &'a [(&'a str, &'a str)],
}

/// Where the slots go under the window: the slim row centred on `row`, the medium (`strip`: x, y, width).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SlotsAt {
    pub row: (i32, i32),
    pub strip: (i32, i32, i32),
}

/// Where the main page put things: each row, the window's picture (for Save's picture to fly out of), the slots; and
/// what `finish_main` paints after the slots (the line under the window).
#[derive(Default)]
pub struct MainOut {
    pub rows: Vec<Area>,
    pub picture: Option<(i32, i32, i32, i32)>,
    pub slots: Option<SlotsAt>,
    /// The line under the window: its left edge, width and first baseline, and how wide it may wrap.
    note_at: Option<(i32, i32, i32, i32)>,
    list_w: i32,
}

/// The main page over a canvas the caller has darkened (the paused game behind it), up to the window's picture. Then
/// the caller paints the slots under it (`MainOut::slots`), as Linux always did, and `finish_main` the rest: the line
/// under the window, Quit's words, the line beside the rows, the hints (Quit's spinner is the caller's).
pub fn paint_main(c: &mut Canvas, text: &mut Text, v: &MainPage) -> MainOut {
    let mut out = MainOut::default();
    let title = v.title;
    let sel = v.sel;
    let beside = v.window.is_some();
    // The game's name across the top as the page's plain title, as on Options (`sheet_header`): no paper under it,
    // so only the selected row looks selected (taste pass 0.2: a paper band read as a second selection). The menu
    // is dark in both appearances.
    let (pad, pill) = (PAD, PILL);
    let title_w = c.w - 2 * pad;
    // Retro (`retro`): the title in a window across the top, the rows in a window at the left with an icon each.
    let retro = retro::on();

    // The battery in this game at the right ("About 3 h 20 min left in this game", `battery_games`); the title gets
    // the rest.
    let mut room = title_w - 2 * BTN_PAD;
    if let Some(line) = v.battery {
        if retro {
            // Retro's wide letters cut the game's name to "Donkey Kong Co…" beside the battery (Retro clip,
            // 06.11.2026): the name keeps what it needs up to three fifths, the battery line takes two lines beside it.
            let title_need = text.width(Face::Title, 50, title);
            let beside = (room - title_need - 40).clamp(room * 2 / 5, room * 3 / 5);
            let lines = text.wrap_prose_at_most(Face::Body, SMALL, line, beside, 2);
            let line_h = type_scale::line(SMALL, 4);
            let mid = pad + pill / 2 + type_scale::mid(SMALL) - (lines.len() as i32 - 1) * line_h / 2;
            let mut widest = 0;
            for (k, l) in lines.iter().enumerate() {
                widest = widest.max(text.width(Face::Body, SMALL, l));
                text.draw_right(c, Face::Body, SMALL, pad + title_w - BTN_PAD, mid + k as i32 * line_h, pal().dust, l);
            }
            room -= widest + 40;
        } else {
            let line = text.fit(Face::Body, SMALL, line, room * 3 / 5);
            room -= text.width(Face::Body, SMALL, &line) + 40;
            text.draw_right(c, Face::Body, SMALL, pad + title_w - BTN_PAD, pad + pill / 2 + type_scale::mid(SMALL), pal().dust, &line);
        }
    }
    // Title 50, as every page's. A game's name is its own: never translated.
    text.literal(|text| {
        let fitted = text.fit(Face::Title, 50, title, room);
        text.draw(c, Face::Title, 50, pad + BTN_PAD, pad + pill / 2 + type_scale::mid(50), pal().chalk, &fitted);
    });
    // Saves that don't reach the card: said under the title on every row (`app_save_trouble`); the rows start below.
    let below_title = match v.trouble {
        Some(line) => trouble_line(c, text, line),
        None => pad + pill,
    };

    // Items from 24 px under the title, on the sheet's grid as every Settings and Options list (no longer centred
    // on the screen with a gap of its own); the selected one sits in a paper pill as wide as its text. A long menu
    // (Players, Achievements and Pixel Path all there) closes up so it stays between the title and the hints.
    // Retro: its window's frame (four frame pixels above and below the rows) and a gap stay clear of the hints too;
    // six rows in Spanish Large ran into the hint bar on 1024x768.
    let frame_room = if retro { 6 * retro::U } else { 0 };
    let step = pill.min((c.h - pad - pill - below_title - 24 - frame_room) / v.rows.len().max(1) as i32);
    let top = below_title + 24;
    // Retro: the rows' window and the room its icons take before each name. The window is as wide as its longest
    // name with its value at rest needs (Load: the longest of its stops, so it doesn't jump while stepping), at
    // least half the screen less 80 and at most what leaves the preview window a third of the screen
    // (`retro_list_w`); "Sichern & Ende" in Large ran into its frame on 640x480, "Load Where I quit" under the preview.
    // The icon two frame pixels after the bar's cursor, the name four after the icon.
    let u = retro::U;
    let icon_k = (2 * step.min(80) / 80).max(1);
    let icon_x = pad + 10 * u;
    let text_x = if retro { icon_x + retro::ICON * icon_k + 4 * u } else { pad + BTN_PAD };
    let list_w = if retro {
        let mut need = 0;
        for row in v.rows {
            let rest = row.widest.clone();
            let chip_w = rest.map_or(0, |t| SMALL as i32 / 2 + text.width(Face::Body, FONT, &t));
            let angle_w = if row.opens { SMALL as i32 / 2 + text.width(Face::Body, FONT, ">") } else { 0 };
            need = need.max(text.width(Face::Body, FONT, &row.label) + chip_w + angle_w);
        }
        retro_list_w(c.w, text_x + need + RETRO_ROW_END - pad)
    } else {
        0
    };
    let list_top = top - 4 * retro::U;
    if retro {
        retro::frame(c, pad, list_top, list_w, v.rows.len() as i32 * step + 8 * u, pal().pill);
        retro::frame_lines_are_no_text(c, pad, list_top, list_w, v.rows.len() as i32 * step + 8 * u);
    }
    for (i, row) in v.rows.iter().enumerate() {
        let label = row.label.as_str();
        let y = top + i as i32 * step;
        let label_w = text.width(Face::Body, FONT, label);
        let baseline = y + step / 2 + type_scale::mid(FONT);
        out.rows.push(Area::new(pad, y, if retro { list_w } else { c.w / 2 - pad }, step));
        // Save and Load show their state: the slot ("3" at rest, "‹ 3 ›" when selected), or on Load also "Auto" and
        // "Undo"; Disc the disc chosen ("1 of 3", "‹ 2 of 3 ›").
        let chip = row.chip.as_ref().map(|name| {
            let arrows = i == sel && row.arrows;
            // Not under the preview window at the right (half the screen): "Antes de cargar" in Spanish ran
            // beneath it on 1024x768. The window's caption says it in full.
            let room = if retro {
                // Retro: inside the rows' window, whatever is selected (it ran under the preview, Retro clip).
                pad + list_w - RETRO_ROW_END - (text_x + label_w + SMALL as i32 / 2)
            } else if beside {
                c.w - pad - (c.w / 2 + 24) - 16 - (pad + BTN_PAD + label_w + SMALL as i32 / 2) - BTN_PAD
            } else {
                i32::MAX / 2
            };
            let full = if arrows { format!("‹ {name} ›") } else { name.clone() };
            // Whole when it fits; else the name alone, cut short (the arrows are in the hint bar's "Estado").
            if text.width(Face::Body, FONT, &full) <= room {
                full
            } else {
                text.fit(Face::Body, FONT, name, room.max(0))
            }
        });
        let chip_w = chip.as_ref().map_or(0, |t| SMALL as i32 / 2 + text.width(Face::Body, FONT, t));
        // A row that opens a page ends in the drawn angle on every row: Retro's at the window's right edge, as a
        // Settings list's; the plain look's after the name, inside the selected pill.
        let angle_w = if row.opens { SMALL as i32 / 2 + text.width(Face::Body, FONT, ">") } else { 0 };
        let fg = if i == sel { pal().ink } else if !row.enabled { if retro { pal().off } else { 0x7f7f7f } } else if retro { pal().text } else { 0xffffff };
        if retro {
            if i == sel {
                retro::bar(c, pad + 3 * u, y + 2, list_w - 6 * u, step - 4);
            }
            let icon_y = y + (step - retro::ICON * icon_k) / 2;
            retro::icon(c, row.icon, icon_x, icon_y, icon_k);
            let label = text.fit(Face::Body, FONT, label, pad + list_w - RETRO_ROW_END - text_x - angle_w);
            text.draw(c, Face::Body, FONT, text_x, baseline, fg, &label);
        } else if i == sel {
            crate::nav_probe::focus(c, (pad, y, label_w + chip_w + angle_w + 2 * BTN_PAD, step));
            c.round_rect(pad, y, label_w + chip_w + angle_w + 2 * BTN_PAD, step, step / 2, pal().select);
            text.draw(c, Face::Body, FONT, pad + BTN_PAD, baseline, pal().ink, label);
        } else {
            text.draw(c, Face::Body, FONT, pad + BTN_PAD, baseline, fg, label);
        }
        if row.opens {
            let right = if retro { pad + list_w - RETRO_ROW_END } else { text_x + label_w + angle_w };
            kit::row_value_in(c, text, FONT, right, baseline, ">", (fg, fg));
        }
        if let Some(chip) = chip {
            // Its arrows drawn, as every list's (`kit::row_value`).
            let (x, color) = (text_x + label_w + SMALL as i32 / 2, if i == sel { pal().label_dust } else if retro { pal().dust } else { 0x9a9a9a });
            let right = x + text.width(Face::Body, FONT, &chip);
            kit::row_value_in(c, text, FONT, right, baseline, &chip, (color, color));
        }
    }

    // Right: a dark window, half the screen, with the save's preview or a message.
    if let Some(win) = &v.window {
        // A little lower with larger text (Settings → Text size), so the note under it keeps clear of the hints.
        let (hw, hh) = (c.w / 2, c.h / 2 - 5 * type_scale::grow(SMALL));
        let r = if retro { 16 } else { 12 };
        // The strip borrows its height from the picture. Together they still need room for two note lines
        // before the hint pill (native 1024x768 with German/Large otherwise hides the save's age).
        let note_tail = 48 + 34 + type_scale::line(SMALL, 8) + type_scale::px(SMALL) as i32 / 3 + 12;
        let (top, bottom) = (pad + PILL + 16, c.h - pad - PILL);
        let hh = hh.min((bottom - top - 2 * r - note_tail).max(1));
        // Under the picture the slots (`app_save_media`): the slim row in the dots' 28 px, or with "Memory card in
        // the pause menu" the medium, for which the picture gives up its height but those 28 px; the note and
        // everything around stay where they were.
        // Retro: beside the rows' window, a gap of six frame pixels, as wide as that leaves.
        let hw = if retro { hw.min(c.w - pad - 2 * r - (pad + list_w + RETRO_GAP)) } else { hw };
        let strip_h = (win.strip_h)(hw + 2 * r);
        let ox = c.w - pad - hw - 2 * r;
        let centred = (c.h - hh - 2 * r) / 2;
        // Retro: its top on the rows' window's, so the two windows line up.
        let centred = if retro { list_top } else { centred };
        let oy = centred.min(bottom - hh - 2 * r - note_tail).max(top);
        let hh = hh + 28 - strip_h;
        if retro {
            retro::frame(c, ox, oy, hw + 2 * r, hh + 2 * r, pal().pill);
            retro::frame_lines_are_no_text(c, ox, oy, hw + 2 * r, hh + 2 * r);
        } else {
            c.round_rect(ox, oy, hw + 2 * r, hh + 2 * r, r * 2, 0x262626);
        }
        c.rect(ox + r, oy + r, hw, hh, 0x000000);
        match win.picture {
            Some(img) => {
                // The picture keeps its shape inside the window.
                let (iw, ih) = img.fit(hw, hh);
                out.picture = Some((ox + r + (hw - iw) / 2, oy + r + (hh - ih) / 2, iw, ih));
                c.blit_scaled(img, ox + r + (hw - iw) / 2, oy + r + (hh - ih) / 2, iw, ih);
            }
            None => centered(c, text, (ox + r, oy + r, hw, hh), win.empty),
        }
        let mut note_y = oy + hh + 2 * r + 48;
        if win.slots {
            out.slots = Some(SlotsAt { row: (ox + (hw + 2 * r) / 2, oy + hh + 2 * r + 26), strip: (ox, oy + hh + 2 * r + 14, hw + 2 * r) });
        }
        // The note where it was under the dots, with the slots or without (Earlier, RetroArch's one save).
        note_y += strip_h + 6;
        // Centred under the window, as wide as the screen leaves on both sides of its middle, on a second line
        // if it is wider (larger text).
        // Retro: within the window's width (it ran to 10 px from the screen's edge).
        let room = if retro { hw + 2 * r } else { 2 * (c.w - 8 - (ox + hw / 2 + r)) };
        out.note_at = Some((ox, hw + 2 * r, note_y, room));
    }
    out.list_w = list_w;
    out
}

/// The main page after the slots (`paint_main`): the line under the window, Quit's words, the line beside the rows, the
/// hints.
pub fn finish_main(c: &mut Canvas, text: &mut Text, hits: &mut HitMap, v: &MainPage, out: &MainOut) {
    let (pad, retro, list_w) = (PAD, retro::on(), out.list_w);
    if let (Some(win), Some((ox, ww, note_y, room))) = (&v.window, out.note_at) {
        for (k, line) in text.wrap_prose_at_most(Face::Body, SMALL, &win.note, room, 2).iter().enumerate() {
            let w = text.width(Face::Body, SMALL, line);
            text.draw(c, Face::Body, SMALL, ox + (ww - w) / 2, note_y + k as i32 * type_scale::line(SMALL, 8), 0xcccccc, line);
        }
    }
    if let Some(msg) = v.quitting {
        text.draw_right(c, Face::Body, FONT, c.w - pad - BTN_PAD, c.h / 2, 0xcccccc, msg);
        return;
    }
    if let Some(note) = &v.note {
        let room = if retro { c.w - PAD - BTN_PAD - (pad + list_w + RETRO_GAP) } else { c.w / 2 - PAD };
        menu_note(c, text, note, room);
    }
    kit::pill_hints(c, text, hits, v.hints);
}

/// The main page's hints: what Left/Right step on the selected row (`step`: "Slot", "State", "Disc"), else START's way to
/// the quick settings (`quick`); SELECT's to the recent games (`switcher`); B; A, which says Open on a row that opens a
/// page. Where the four don't fit whole, SELECT's goes first and START's stays: it is the way to the quick settings.
pub fn main_hints(step: Option<&'static str>, opens: bool, switcher: bool, quick: bool) -> Vec<(&'static str, &'static str)> {
    let mut hints = Vec::new();
    match step {
        Some(what) => hints.push(("< >", what)),
        None if quick => hints.push(("START", "Quick settings")),
        None => (),
    }
    if switcher {
        hints.push(("SELECT", "Recent games"));
    }
    hints.extend([("B", "Back"), ("A", if opens && step.is_none() { "Open" } else { "OK" })]);
    hints
}

// ---- The Options family of pages ----

/// A row of an options page: its name, its value (">" or "2 on >" for a row that opens a page, "" for one that does
/// something), its explanation.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct OptRow {
    pub name: String,
    pub value: String,
    pub desc: String,
    /// A step in, under the row it belongs to (Sharpness and Effect under the Look's Custom).
    pub indent: bool,
    /// Left/Right change it (a cheat's line that only explains doesn't).
    pub fixed: bool,
}

impl OptRow {
    pub fn new(name: impl Into<String>, value: impl Into<String>, desc: impl Into<String>) -> OptRow {
        OptRow { name: name.into(), value: value.into(), desc: desc.into(), ..OptRow::default() }
    }
}

/// The Picture page's preview: how many rows its list shows, where the images go, and what they show.
pub struct PicturePreview<'a> {
    pub rows: usize,
    pub layout: preview::Layout,
    pub live: &'a preview::Live,
}

/// An options page.
pub struct OptionsPage<'a> {
    /// "Options", "Options › Picture", "Emulator   Sound", a question.
    pub heading: Option<&'a str>,
    /// Information under the heading (the Achievements page's points).
    pub counts: Option<&'a str>,
    pub rows: &'a [OptRow],
    pub sel: usize,
    /// A confirmation or note in place of the selected row's explanation.
    pub footer: Option<&'a str>,
    /// The selected row only explains (a cheat's heading): B is its only hint.
    pub explanation: bool,
    /// The game's system stripe at the selected row's start.
    pub accent: u32,
    pub corner: &'a Corner,
    pub picture: Option<PicturePreview<'a>>,
}

/// Where an options page put its rows (None: scrolled out of view), and the selected row's explanation as painted,
/// line by line.
#[derive(Default)]
pub struct OptionsOut {
    pub rows: Vec<Option<Area>>,
    pub desc: Vec<String>,
}

/// The Picture page on a `w`×`h` screen: how many of its `max_rows` rows the list shows, and where the preview goes,
/// between the rows and the longest of the group's explanations (`descs`), so the images keep their size while the
/// selection moves.
pub fn picture_page(text: &mut Text, w: i32, h: i32, max_rows: usize, descs: &[&str]) -> (usize, preview::Layout) {
    // The explanation stands beside the hint pill, in the column left of it, its last line on the pill's
    // (`kit::footer`); the longest of the group sets the room.
    let pill_x = w - PAD - kit::pill_width(text, w, VALUE_HINTS);
    let most = if preview::small_screen(h) { preview::SMALL_SCREEN_LINES } else { 5 };
    let lines = descs.iter().map(|d| kit::footer_lines(text, d, w, pill_x, most).len()).max().unwrap_or(1);
    let foot = h - kit::footer_baseline(h) + SMALL as i32 - type_scale::line(SMALL, 10);
    preview::picture_page_over((w, h), PAD, foot, PAD + PILL + 12, max_rows, lines, SMALL)
}

/// Options screens in NextUI's layout: a list of rows with the value at the right; the selected row is one sheet of
/// paper with the game's system colour as a short stripe at its start. The selected entry's explanation sits at the
/// bottom left beside the hint pill (`kit::footer`), the status pill at the top right.
pub fn paint_options(c: &mut Canvas, text: &mut Text, hits: &mut HitMap, v: &OptionsPage) -> OptionsOut {
    let rows = v.rows;
    let sel = v.sel;
    let mut out = OptionsOut { rows: vec![None; rows.len()], desc: Vec::new() };
    let (pad, row, font) = (PAD, type_scale::row(66, 36), 36u32);
    // The heading ("Options   <game>") stops short of the status pill, which is wider with Wi-Fi and "charging".
    let pill_left = status_pill(c, text, v.corner);
    // The page's title in Title 50, as every page's (`sheet_header`); a question ("Couldn't save. Quit anyway?") is
    // the title, not a small line over it.
    if let Some(h) = v.heading {
        let h = text.fit(Face::Title, 50, h, pill_left - 24 - (pad + BTN_PAD));
        text.draw(c, Face::Title, 50, pad + BTN_PAD, pad + 58, pal().chalk, &h);
    }
    let mut top = pad + PILL + 12;
    let right = c.w - pad;
    // Counts are information, not part of the title: Retro's wider title face must not hide the points.
    if let Some(counts) = v.counts {
        let step = type_scale::line(SMALL, 8);
        for line in text.wrap_prose(Face::Body, SMALL, counts, right - (pad + BTN_PAD)) {
            text.draw(c, Face::Body, SMALL, pad + BTN_PAD, top + SMALL as i32, pal().dust, &line);
            top += step;
        }
        top += 12;
    }
    // Explanation of the selected row (or the last confirmation) beside the hints or over them, as on every page
    // of Settings (`kit::footer`); the hints say what B, A and Left/Right do here.
    let desc = v.footer.map(str::to_string).unwrap_or_else(|| rows.get(sel).map(|r| r.desc.clone()).unwrap_or_default());
    let hints = hints_for(rows.get(sel), v.explanation);
    let pill_x = c.w - PAD - kit::pill_width(text, c.w, hints);
    // Long lists (a core's options) scroll; the explanation below keeps the room the page's tallest explanation takes
    // (`list_bottom`), up to eight lines: the list shows fewer rows rather than cut one (a long emulator line with its
    // restart note in Large text, 17.11.2026), and no more room than that: five lines kept under every list left a
    // third of the Brick's screen empty under Options (hand test, 17.11.2026). The Picture page with Large text shows
    // fewer rows, so its preview keeps a useful size (`picture_page`).
    let (bottom, need) = if v.picture.is_some() {
        (c.h - (c.h - kit::footer_baseline(c.h) + 5 * type_scale::line(SMALL, 10)), 5)
    } else {
        list_bottom(text, c.w, c.h, rows, (&desc, hints))
    };
    // Three rows at least (the selected one and its neighbours): where an explanation needs more room than that leaves
    // (eight lines in Large text on the Brick), it is cut with "…" at the room there is, never painted over the rows.
    let least = top + 3 * row;
    let squeezed = bottom < least;
    let fit = ((bottom.max(least) - top) / row) as usize;
    let fit = v.picture.as_ref().map_or(fit, |p| p.rows.min(fit));
    let visible = fit.min(rows.len().max(1));
    let first = window_start(sel, visible, rows.len());
    for (k, r) in rows.iter().enumerate().skip(first).take(visible) {
        let (name, value) = (&r.name, &r.value);
        let i = k;
        let y = top + (k - first) as i32 * row;
        let base = y + row / 2 + type_scale::mid(font);
        // Sharpness and Effect stand a step in under the Look (Custom).
        let step_in = if r.indent { 36 } else { 0 };
        // The selected row's value that Left/Right change stands between its arrows (`kit::row_value`).
        let selected = i == sel;
        let shown = if selected && !r.fixed && !value.is_empty() && !value.ends_with('>') { format!("‹ {value} ›") } else { value.clone() };
        // The name gives way to a long value; both stay inside the row.
        let value_w = if shown.is_empty() { 0 } else { text.width(Face::Body, font, &shown) + 40 };
        let name = text.fit(Face::Body, font, name, right - pad - 2 * BTN_PAD - value_w - step_in);
        // One sheet of paper with the game's system stripe at its start, as a case's spine (`kit::selection_striped`).
        if selected {
            kit::selection_striped(c, pad, y, right - pad, row, v.accent);
        }
        text.draw(c, Face::Body, font, pad + BTN_PAD + step_in, base, if selected { pal().ink } else { pal().text }, &name);
        if !shown.is_empty() {
            kit::row_value(c, text, font, right - BTN_PAD, base, &shown, selected);
        }
        out.rows[i] = Some(Area::new(pad, y, right - pad, row));
    }
    if rows.len() > visible {
        // A thin bar at the right edge shows where the window sits in the list.
        let track = visible as i32 * row;
        kit::scrollbar(c, top, track, first, visible, rows.len());
    }

    if let Some(p) = &v.picture {
        p.live.paint(c, text, &p.layout, 0x9a978f);
    }

    // Five lines, more when the list leaves room above them: a long explanation (German, Spanish) ends in "…" only
    // when it must.
    let step = type_scale::line(SMALL, 10);
    let spare = (kit::footer_baseline(c.h) - (top + visible as i32 * row) - SMALL as i32 - 12) / step + 1;
    let desc_lines = if v.picture.is_some() {
        if preview::small_screen(c.h) { preview::SMALL_SCREEN_LINES } else { 5 }
    } else {
        let spare = spare.max(1) as usize;
        if squeezed { spare.min(MOST_DESC_LINES) } else { spare.max(5).max(need).min(MOST_DESC_LINES) }
    };
    out.desc = kit::footer_lines(text, &desc, c.w, pill_x, desc_lines);
    kit::footer(c, text, hits, hints, &desc, desc_lines, pal().text);
    out
}

/// The lowest an options page's rows may reach on a `w`×`h` screen, and how many lines that leaves the explanation shown
/// now (`shown`, with its hints): over the room the page's tallest explanation takes, up to five lines (the same for
/// every row, so the list doesn't move as the selection does), and over the one shown when it takes more (up to
/// `MOST_DESC_LINES`; the list shows fewer rows rather than cut it); each as `kit::footer` paints it beside its row's
/// hints, up to the top of its first line's capitals (`type_scale::mid` is half their height) and a 10 px gap. Over
/// the hint pill when every line stands beside it.
fn list_bottom(text: &mut Text, w: i32, h: i32, rows: &[OptRow], shown: (&str, &[(&str, &str)])) -> (i32, usize) {
    // The top of `d`'s capitals painted in at most `most` lines beside `hints`, and how many lines it takes.
    let mut top_of = |d: &str, hints: &[(&str, &str)], most: usize| {
        let pill_x = w - PAD - kit::pill_width(text, w, hints);
        let lines = kit::footer_lines(text, d, w, pill_x, most).len();
        let px = kit::footer_px(text, d, pill_x);
        (kit::footer_baseline(h) - (lines as i32 - 1) * type_scale::line(px, 10) - 2 * type_scale::mid(px), lines)
    };
    let (mut bottom, mut need) = (kit::content_bottom(h), 0);
    let all = rows.iter().map(|r| (r.desc.as_str(), hints_for(Some(r), false), STABLE_DESC_LINES)).chain(std::iter::once((shown.0, shown.1, MOST_DESC_LINES)));
    for (d, hints, most) in all.collect::<Vec<_>>() {
        if d.trim().is_empty() {
            continue;
        }
        let (first, lines) = top_of(d, hints, most);
        bottom = bottom.min(first - 10);
        need = need.max(lines);
    }
    (bottom, need)
}

/// What B, A and Left/Right do on an options page's selected row: a page's row opens, a value changes, any other row
/// is chosen; a row that only explains has Back alone.
pub fn hints_for(row: Option<&OptRow>, explanation: bool) -> &'static [(&'static str, &'static str)] {
    match row.map(|r| r.value.as_str()) {
        _ if explanation => &[("B", "Back")],
        Some(v) if v.ends_with('>') => &[("B", "Back"), ("A", "Open")],
        Some("") | None => &[("B", "Back"), ("A", "Choose")],
        Some(_) => VALUE_HINTS,
    }
}

// ---- Messages over the game ----

/// A message over the game (a note at the start, what a save did, why something didn't work) is a dark pill at the
/// bottom centre of the screen, `NOTE_MARGIN` from its edge, at `NOTE_ALPHA`: pocket-core paints it so on Linux
/// (`overlay`), Android's player with the same function.
pub const NOTE_FONT: u32 = 34;
pub const NOTE_PAD_X: i32 = 20;
pub const NOTE_PAD_Y: i32 = 12;
pub const NOTE_MARGIN: i32 = 24;
pub const NOTE_PILL: u32 = 0x1a1a1a;
pub const NOTE_ALPHA: f32 = 0.82;
/// The see-through colour of the pill's picture (`pixel_live::KEY`).
pub const NOTE_KEY: u32 = 0x010203;
/// The widest a message gets before it goes on to a second line (the save warning, a long German note).
pub const NOTE_WRAP_AT: i32 = 1000;
/// A panel narrower than this has the messages in `NOTE_SMALL_FONT`.
pub const NOTE_SMALL_VIEW: u32 = 800;
pub const NOTE_SMALL_FONT: u32 = 22;

/// A message in its dark pill for a screen `view_w` wide: one line, or a rounded box of several when it is wider than
/// `NOTE_WRAP_AT` (the save warning); a small panel gets a smaller type and lines that stay on it. Returns width,
/// height and pixels (`NOTE_KEY` is transparent).
pub fn note_pill(text: &mut Text, msg: &str, view_w: u32) -> (i32, i32, Vec<u32>) {
    let font = if view_w < NOTE_SMALL_VIEW { NOTE_SMALL_FONT } else { NOTE_FONT };
    // Retro: the text keeps its padding from the window's inner line, not from its outer edge (it sat on the line,
    // Retro clip 06.11.2026); frame pixels of half `U` on a small panel, as pocket-ui's look there.
    let u = if view_w < NOTE_SMALL_VIEW { retro::U / 2 } else { retro::U };
    let inset = if retro::on() { retro::frame_inset(u) } else { 0 };
    let (pad_x, pad_y) = (NOTE_PAD_X + inset, NOTE_PAD_Y + inset);
    let wrap_at = NOTE_WRAP_AT.min(view_w as i32 - 2 * NOTE_MARGIN - 2 * pad_x);
    let lines = if text.width(Face::Body, font, msg) > wrap_at { text.wrap_prose(Face::Body, font, msg, wrap_at) } else { vec![msg.to_string()] };
    let line_h = font as i32 * 13 / 10;
    let w = lines.iter().map(|l| text.width(Face::Body, font, l)).max().unwrap_or(0) + 2 * pad_x;
    let h = font as i32 + (lines.len() as i32 - 1) * line_h + 2 * pad_y;
    let mut buf = vec![NOTE_KEY; (w * h) as usize];
    let mut c = Canvas::new(&mut buf, w as u32, h as u32);
    let radius = if lines.len() > 1 { 26 } else { h / 2 };
    if retro::on() {
        retro::frame_at(&mut c, 0, 0, w, h, NOTE_PILL, u);
    } else {
        c.round_rect(0, 0, w, h, radius, NOTE_PILL);
    }
    for (i, l) in lines.iter().enumerate() {
        text.draw(&mut c, Face::Body, font, pad_x, pad_y + font as i32 * 4 / 5 + i as i32 * line_h, 0xffffff, l);
    }
    (w, h, buf)
}

// ---- Zoom ----

/// The Zoom page (`zoom`): the part of the paused screen `frame` in view, `zoom`'s level times as large as the screen
/// shows it, moving inside the game's own rectangle `game` (without the black borders or a frame); "2x" in a pill at the
/// top left, the hints at the bottom right. Linux paints it from pocket-core's shot of the screen, Android's player from
/// its own picture of the screen (`Graphics::render_preview`): the same page.
pub fn paint_zoom(c: &mut Canvas, text: &mut Text, hits: &mut HitMap, frame: &Image, zoom: &crate::zoom::Zoom, game: Option<crate::preview::GameRect>) {
    // The game's part, `level` times as large as the screen shows it; a game smaller than that is shown whole, centred.
    let part = zoom.part(frame.w, frame.h, game);
    let (dw, dh) = ((part.2 as u64 * c.w as u64 * zoom.level() as u64 / frame.w.max(1) as u64) as i32, (part.3 as u64 * c.h as u64 * zoom.level() as u64 / frame.h.max(1) as u64) as i32);
    c.clear(0x000000);
    c.blit_region(frame, part, (c.w - dw) / 2, (c.h - dh) / 2, dw, dh);
    let label = zoom.label();
    let w = text.width(Face::Body, SMALL, &label) + 2 * BTN_PAD;
    retro::pill(c, PAD, PAD, w, PILL, PILL / 2, crate::palette::DARK.select);
    text.draw(c, Face::Body, SMALL, PAD + BTN_PAD, PAD + (PILL - 20) / 2 + type_scale::mid(SMALL), crate::palette::INK, &label);
    kit::pill_hints(c, text, hits, ZOOM_HINTS);
}

/// The Zoom page's hints.
pub const ZOOM_HINTS: &[(&str, &str)] = &[("D-pad", "Move"), ("A", "2x / 3x"), ("B", "Back")];

#[cfg(test)]
mod tests {
    /// Both menus paint every page through this file, so no part of a page can drift again: Android's player's list menu
    /// (main page, the Options pages with the Picture preview, Zoom) calls the painters here and draws no text, pill or
    /// frame of its own; Linux's menu calls the same; the copy Android had (`pause_list`) paints nothing any more. Until
    /// 17.11.2026 Android painted its own copy and differed in the preview, sizes, headings, hints and rows.
    #[test]
    fn both_menus_paint_their_pages_with_this_painter() {
        let player = include_str!("../android/src/player.rs");
        let start = player.find("if let Some(list) = list {").expect("the player's list menu paint");
        let end = start + player[start..].find("} else if self.paused {").expect("its end");
        let block = &player[start..end];
        for call in ["pause_page::paint_main(", "pause_page::finish_main(", "pause_page::paint_options(", "pause_page::paint_zoom("] {
            assert!(block.contains(call), "the player's list menu doesn't call {call}");
        }
        for own in ["text.draw", "draw_right", "round_rect", "kit::", "retro::", "paint_badge", "pill("] {
            assert!(!block.contains(own), "the player's list menu paints {own} of its own");
        }
        // The Picture page's preview where Linux has it.
        assert!(player.contains("let picture = self.nav.page == Some(pause_options::Entry::Picture);"));
        assert!(player.contains("fn refresh_live("), "the player makes the preview");
        let linux = include_str!("main.rs");
        for call in ["pocket_ui::pause_page::paint_main(", "pocket_ui::pause_page::finish_main(", "pocket_ui::pause_page::paint_options(", "pocket_ui::pause_page::picture_page("] {
            assert!(linux.contains(call), "Linux's menu doesn't call {call}");
        }
        assert!(include_str!("app_zoom.rs").contains("pocket_ui::pause_page::paint_zoom("));
        assert!(!include_str!("pause_list.rs").contains("fn paint"), "pause_list paints again");
    }

    #[test]
    fn the_main_page_s_hints_are_linux_s() {
        use super::main_hints;
        assert_eq!(main_hints(None, true, true, true), [("START", "Quick settings"), ("SELECT", "Recent games"), ("B", "Back"), ("A", "Open")]);
        assert_eq!(main_hints(Some("Slot"), false, true, true), [("< >", "Slot"), ("SELECT", "Recent games"), ("B", "Back"), ("A", "OK")]);
        assert_eq!(main_hints(Some("Disc"), false, false, false), [("< >", "Disc"), ("B", "Back"), ("A", "OK")]);
    }
}
