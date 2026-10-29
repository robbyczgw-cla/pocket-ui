//! The pictures on the feature cards (`features`): each feature's own screen, or the part that shows it, painted at
//! the screen's size (1240 x 1080) with the shelf's parts and tokens and made-up data, then scaled down by the cards.
//! The real data models are used where they exist (Pixel Path's report and grades, the screen frames, the game card,
//! Curator's facets, the pages' row names), so a preview can't show something the feature doesn't do.
//! Nothing here reads a file or the network; a preview costs a few milliseconds and is painted only when its card is
//! shown. Games are called "Sample game", as on the look step (`look_sample`).
//!
//! What sits over a game (the pause menu and its pages) stays dark in both appearances, as it does on the device.

use crate::bezel;
use crate::game_card::{self, Card, CardData, CuratorRow, RowKind, Summary};
use crate::gfx::{mix, Canvas};
use crate::image::Image;
use crate::library_meta::Status;
use crate::palette::{self, *};
use crate::pixel_path::{self, Grade};
use crate::setup::hints;
use crate::text::{Face, Text};

const W: i32 = 1240;
const H: i32 = 1080;

fn stripe(system: &str) -> u32 {
    bezel::system_colour(system)
}

/// The top line of a page: where it is, left; the battery, right (as on Curator's pages).
fn page_top(c: &mut Canvas, text: &mut Text, place: &str) {
    text.draw(c, Face::Body, 28, MARGIN, 72, pal().dust, place);
    text.draw_right(c, Face::Body, 28, c.w - MARGIN, 72, pal().dust, "82%");
}

fn heading(c: &mut Canvas, text: &mut Text, title: &str, sub: &str) {
    let title = text.fit(Face::Title, 67, title, c.w - 2 * MARGIN);
    text.draw(c, Face::Title, 67, MARGIN, 160, pal().chalk, &title);
    text.draw(c, Face::Body, 28, MARGIN, 208, pal().dust, sub);
}

/// A made-up game picture, `w` x `h` console pixels: sky, sun, hills, a row of blocks, a small figure. Pixel art in
/// plain shapes, no game's.
pub fn sample_scene(w: u32, h: u32) -> Image {
    let mut px = vec![0u32; (w * h) as usize];
    let mut c = Canvas::new(&mut px, w, h);
    let (w, h) = (w as i32, h as i32);
    // Sky in four bands, lighter towards the horizon.
    let sky = [0x2b3f8c, 0x3d5bb0, 0x5a7fd0, 0x86a8e6];
    for (i, col) in sky.iter().enumerate() {
        c.rect(0, i as i32 * h / 8, w, h / 8 + 1, *col);
    }
    c.rect(0, h / 2, w, h / 2, sky[3]);
    c.disc(w * 3 / 4, h / 5, h / 12, 0xf4d27a);
    // Two rows of hills.
    for (k, (col, base, amp)) in [(0x4e8a5a, h * 62 / 100, h / 9), (0x3b6e46, h * 72 / 100, h / 12)].iter().enumerate() {
        for x in 0..w {
            let t = (x as f32 / w as f32 * std::f32::consts::TAU * (1.5 + k as f32)).sin();
            let top = base - (t.abs() * *amp as f32) as i32;
            c.rect(x, top, 1, h - top, *col);
        }
    }
    // The ground: blocks with a lighter top edge.
    let g = h * 82 / 100;
    let s = (h / 14).max(4);
    let mut x = 0;
    while x < w {
        c.rect(x, g, s, h - g, 0x9a6b3f);
        c.rect(x, g, s, 2, 0xc8935a);
        // (Canvas::rect wants x inside the canvas.)
        if x + s - 1 < w {
            c.rect(x + s - 1, g, 1, h - g, 0x7a522e);
        }
        x += s;
    }
    // Floating blocks and a small figure.
    for i in 0..3 {
        let bx = w / 3 + i * s;
        c.rect(bx, g - 4 * s, s - 1, s - 1, 0xd8a040);
        c.rect(bx + s / 3, g - 4 * s + s / 3, s / 3, s / 3, 0x8a5a20);
    }
    let (fx, fy) = (w / 5, g - s * 2);
    c.rect(fx, fy, s, s, 0xe8e2d0);
    c.rect(fx + s / 4, fy + s / 4, s / 5, s / 5, 0x202020);
    c.rect(fx - s / 4, fy + s, s + s / 2, s, 0xc0402e);
    Image { w: w as u32, h: h as u32, px }
}

/// The scene at a whole-number scale in the middle of the screen, as Integer scaling shows it; (x, y, w, h).
fn scene_on_screen(c: &mut Canvas, (sw, sh): (u32, u32)) -> (i32, i32, i32, i32) {
    let scale = (W / sw as i32).min(H / sh as i32).max(1);
    let (gw, gh) = (sw as i32 * scale, sh as i32 * scale);
    let (gx, gy) = ((W - gw) / 2, (H - gh) / 2);
    blit_nearest(c, &sample_scene(sw, sh), gx, gy, scale);
    (gx, gy, gw, gh)
}

fn blit_nearest(c: &mut Canvas, img: &Image, x: i32, y: i32, scale: i32) {
    for sy in 0..img.h as i32 {
        for sx in 0..img.w as i32 {
            c.rect(x + sx * scale, y + sy * scale, scale, scale, img.px[(sy * img.w as i32 + sx) as usize]);
        }
    }
}

/// A list in the style of Settings (and the pause menu): a title, rows of name and value, the selected one on the
/// band with its pill; `values` right-aligned.
fn list_page(c: &mut Canvas, text: &mut Text, title: &str, rows: &[(&str, String)], sel: usize, top: i32) {
    let (pad, row, font) = (30, 66, 36u32);
    // The status pill, top right.
    let status = "12:34   82%";
    let sw = text.width(Face::Body, 30, status) + 48;
    c.round_rect(c.w - pad - sw, pad, sw, 90, 45, pal().pill);
    text.draw(c, Face::Body, 30, c.w - pad - sw + 24, pad + 56, pal().soft, status);
    text.draw(c, Face::Title, 50, pad + 36, pad + 58, pal().chalk, title);
    let right = c.w - pad;
    for (i, (name, value)) in rows.iter().enumerate() {
        let y = top + i as i32 * row;
        let base = y + row / 2 + font as i32 * 7 / 20;
        let selected = i == sel;
        if selected {
            c.round_rect(pad, y + 4, right - pad, row - 8, (row - 8) / 2, pal().band);
            let pill = text.width(Face::Body, font, name) + 72;
            c.round_rect(pad, y + 4, pill, row - 8, (row - 8) / 2, pal().select);
        }
        text.draw(c, Face::Body, font, pad + 36, base, if selected { pal().ink } else { pal().text }, name);
        text.draw_right(c, Face::Body, font, right - 36, base, pal().text, value);
    }
}

/// The paper label of a game on the shelf: title, meta line, the system's stripe.
fn label(c: &mut Canvas, text: &mut Text, y: i32, title: &str, meta: &str, system: &str) {
    let w = c.w - 2 * MARGIN;
    c.rect(MARGIN, y, w, 200, pal().label);
    game_card::lift_shadow(c, MARGIN, y + 200, w, pal().t);
    c.rect(MARGIN, y, STRIPE, 200, stripe(system));
    text.draw(c, Face::Title, 67, MARGIN + 40, y + 100, pal().ink, title);
    text.draw(c, Face::Body, 28, MARGIN + 40, y + 152, pal().label_dust, meta);
}

/// A spine on the shelf: name left, a short note right.
fn spine(c: &mut Canvas, text: &mut Text, y: i32, name: &str, note: &str, system: &str) {
    let w = c.w - 2 * MARGIN;
    c.rect(MARGIN, y, w, 72, pal().spine);
    c.rect(MARGIN, y, STRIPE, 72, stripe(system));
    text.draw(c, Face::Title, 37, MARGIN + 40, y + 49, pal().chalk, name);
    text.draw_right(c, Face::Body, 28, MARGIN + w - 28, y + 47, pal().dust, note);
}

// ---- Curator --------------------------------------------------------------------------------------------------

pub fn lens(c: &mut Canvas, text: &mut Text) {
    use crate::curator::Facet;
    c.clear(pal().void);
    page_top(c, text, "Curator");
    text.draw(c, Face::Title, 67, MARGIN, 160, pal().chalk, "Lens");
    text.draw_right(c, Face::Title, 50, c.w - MARGIN, 160, pal().chalk, "12 games");
    text.draw(c, Face::Body, 28, MARGIN, 208, pal().dust, "Game Boy Advance, Short sessions");
    let (top, row, gap) = (246, 64, 8);
    let (lx, lw) = (MARGIN, 360);
    let rx = lx + lw + 28;
    let rw = c.w - MARGIN - rx;
    let facets = [
        Facet::System,
        Facet::SubGenre,
        Facet::Players,
        Facet::Session,
        Facet::Pace,
        Facet::Mood,
        Facet::Era,
        Facet::Stickers,
        Facet::Favourites,
        Facet::Status,
    ];
    for (k, f) in facets.iter().enumerate() {
        let y = top + k as i32 * (row + gap);
        let picks = [1, 0, 0, 1].get(k).copied().unwrap_or(0);
        let (fg, dim) = crate::features::row_back(c, (lx, y, lw, row), if picks > 0 { pal().chalk } else { pal().dust }, false);
        if k == 0 {
            // The facet whose values are open: a brighter spine.
            c.rect(lx + STRIPE, y, lw - STRIPE, row, mix(pal().spine, pal().label, 40));
            c.rect(lx, y, STRIPE, row, pal().chalk);
        }
        text.draw(c, Face::Title, 37, lx + STRIPE + 24, y + row / 2 + 13, fg, f.label());
        if picks > 0 {
            text.draw_right(c, Face::Body, 28, lx + lw - 24, y + row / 2 + 10, dim, &picks.to_string());
        }
    }
    let values = [
        ("gba", "Game Boy Advance", 12, true),
        ("snes", "Super Nintendo", 9, false),
        ("megadrive", "Mega Drive", 7, false),
        ("gb", "Game Boy", 4, false),
        ("nes", "NES", 3, false),
        ("psx", "PlayStation", 2, false),
    ];
    for (k, (sys, name, n, picked)) in values.iter().enumerate() {
        let y = top + k as i32 * (row + gap);
        let (fg, dim) = crate::features::row_back(c, (rx, y, rw, row), stripe(sys), k == 1);
        let mark = if *picked { Status::Completed } else { Status::NotStarted };
        game_card::paint_mark(c, rx + STRIPE + 30, y + row / 2, mark, if *picked { fg } else { dim });
        text.draw(c, Face::Title, 37, rx + STRIPE + 60, y + row / 2 + 13, fg, name);
        text.draw_right(c, Face::Body, 28, rx + rw - 24, y + row / 2 + 10, dim, &n.to_string());
    }
    hints(c, text, &[("A", "Pick"), ("START", "Show"), ("X", "Save"), ("Y", "Clear"), ("B", "Back")]);
}

/// Curator's Echoes page for a sample game; `elsewhere`: with the names of games not on this device under it.
fn echoes_page(c: &mut Canvas, text: &mut Text, elsewhere: bool) {
    c.clear(pal().void);
    page_top(c, text, "Echoes");
    heading(c, text, "More like Sample game", "4 games you own, the closest first");
    let rows = [
        ("Sample game 2", "gba", "GBA · arcade racer, fast"),
        ("Sample game 3", "snes", "SNES · same series · same era"),
        ("Sample game 4", "megadrive", "Mega Drive · like Sample game, which you finished"),
        ("Sample game 5", "gbc", "GBC · short sessions, calm"),
    ];
    let (top, row, gap) = (246, 124, 10);
    let w = c.w - 2 * MARGIN;
    let shown = if elsewhere { 3 } else { 4 };
    let mut y = top;
    for (k, (name, sys, meta)) in rows.iter().take(shown).enumerate() {
        let selected = k == 0;
        let (fg, dim) = crate::features::row_back(c, (MARGIN, y, w, row), stripe(sys), selected);
        let tx = MARGIN + STRIPE + 24;
        let bg = if selected { pal().label } else { pal().spine };
        c.round_rect(tx + 12, y + 14, 80, row - 28, 6, mix(stripe(sys), bg, 170));
        text.draw(c, Face::Title, 37, tx + 128, y + 54, fg, name);
        text.draw(c, Face::Body, 28, tx + 128, y + 98, dim, meta);
        y += row + gap;
    }
    if elsewhere {
        y += 20;
        text.draw(c, Face::Body, 21, MARGIN, y + 10, pal().dust, "Not on this device");
        text.draw(c, Face::Body, 28, MARGIN, y + 50, pal().dust, "Sample game 7 · Sample game 8 · Sample game 9");
    }
    hints(c, text, &[("A", "Play"), ("X", "Details"), ("B", "Back")]);
}

pub fn echoes(c: &mut Canvas, text: &mut Text) {
    echoes_page(c, text, false);
}

pub fn echoes_elsewhere(c: &mut Canvas, text: &mut Text) {
    echoes_page(c, text, true);
}

/// Curator → History, a month of made-up play over "Sample game" and friends (`play_history::demo`), painted by the
/// page itself (`history_page`).
pub fn play_history(c: &mut Canvas, text: &mut Text) {
    use crate::play_history::{demo, demo_games, summarise, Period, Plain, Span};
    let today = crate::play_history::day_of(2026, 10, 23);
    let log = demo(&demo_games(), today, 3600);
    let mut page = crate::history_page::HistoryPage::new(Period::of(Span::Month, today), today);
    page.set(summarise(&log, page.period, today, &Plain));
    let names = |id: &str| (id.to_uppercase(), id.to_uppercase());
    crate::history_page::paint_history(c, text, &page, &names, Some(&crate::status_corner::Corner::battery_only("82%")));
    hints(c, text, &page.hints());
}

pub fn versions(c: &mut Canvas, text: &mut Text) {
    c.clear(pal().void);
    page_top(c, text, "Versions");
    heading(c, text, "Sample game", "2 versions on this Pocket. The preferred one starts from mixed lists.");
    let rows = [
        (
            "Super Nintendo",
            "snes",
            true,
            ["1994 · Europe", "Runs on pocket-ui's own host", "Saves: 3 slots, played 6 h"],
        ),
        (
            "Game Boy Advance",
            "gba",
            false,
            ["2002 · Europe", "Runs on pocket-ui's own host", "Saves: none yet"],
        ),
    ];
    let (top, row, gap) = (246, 196, 12);
    let w = c.w - 2 * MARGIN;
    for (k, (system, sys, preferred, lines)) in rows.iter().enumerate() {
        let y = top + k as i32 * (row + gap);
        let (fg, dim) = crate::features::row_back(c, (MARGIN, y, w, row), stripe(sys), k == 0);
        let tx = MARGIN + STRIPE + 28;
        if *preferred {
            let side = text.width(Face::Body, 28, "Preferred") + 32;
            text.draw_right(c, Face::Body, 28, MARGIN + w - 28, y + 52, fg, "Preferred");
            game_card::paint_mark(c, MARGIN + w - 28 - side + 8, y + 42, Status::Completed, fg);
        }
        text.draw(c, Face::Title, 37, tx, y + 54, fg, system);
        for (i, line) in lines.iter().enumerate() {
            text.draw(c, Face::Body, 28, tx, y + 98 + i as i32 * 40, if i == 0 { fg } else { dim }, line);
        }
    }
    hints(c, text, &[("A", "Play"), ("Y", "Preferred"), ("B", "Back")]);
}

/// The game card of a sample game with Curator's rows, `focus` on one of them.
fn card(c: &mut Canvas, text: &mut Text, focus: usize) {
    let data = CardData {
        title: "Sample game".into(),
        system: "Game Boy Advance".into(),
        stripe: stripe("gba"),
        matched: true,
        listed_as: None,
        year: Some(2002),
        players: Some("1–2 players".into()),
        facts: vec![("Genre", "Racing".into()), ("Series", "Sample series".into())],
        scores: None,
        sentence: Some("A made-up game, to show what the card holds.".into()),
        summary: Summary::None,
        // Two days ago, six hours in all.
        played: Some((crate::recents::now_secs().saturating_sub(2 * 86_400), 6 * 3600)),
        sessions: 14,
        status: Status::Playing,
        suggestion: None,
        rating: 4,
        note: String::new(),
        more_by: None,
        series: None,
        achievements: None,
        battery: None,
        port: None,
        port_update: false,
        surprise: None,
        look_up: true,
        from_gamelist: false,
        patch: None,
        favorite: false,
        rows: vec![
            CuratorRow {
                kind: RowKind::Echoes,
                title: "Echoes".into(),
                line: "Sample game 2 · Sample game 3 · Sample game 4".into(),
            },
            CuratorRow {
                kind: RowKind::Versions,
                title: "Versions".into(),
                line: "Super Nintendo · Game Boy Advance".into(),
            },
            CuratorRow {
                kind: RowKind::Stickers,
                title: "Stickers".into(),
                line: "Couch co-op · Short sessions".into(),
            },
        ],
    };
    let mut card = Card::new(data);
    card.focus = Some(focus);
    card.paint(c, text, None, None);
}

pub fn stickers(c: &mut Canvas, text: &mut Text) {
    card(c, text, 2);
}

// ---- Pixel Path, frames, starting ----------------------------------------------------------------------------

/// The game picture dimmed behind a menu, as the pause menu shows it.
fn paused_game(c: &mut Canvas) {
    c.clear(VOID);
    scene_on_screen(c, (240, 160));
    c.shade_rect(0, 0, W, H, VOID, 225);
}

pub fn pixel_path(c: &mut Canvas, text: &mut Text) {
    palette::scoped(0, || {
        paused_game(c);
        let Some((report, known)) = pixel_path::demo("gba") else { return };
        let path = pixel_path::path(&report, &known);
        let pad = 30;
        text.draw(c, Face::Body, 30, pad + 36, pad + 56, 0xcccccc, "Pixel Path");
        let w = text.width(Face::Body, 30, "Pixel Path");
        c.disc(pad + 36 + w + 38, pad + 46, 8, path.overall.colour());
        text.draw(c, Face::Body, 30, pad + 36 + w + 56, pad + 56, path.overall.colour(), path.overall.name());
        let (top, row) = (pad + 104, 104);
        let right = c.w - pad;
        let (dot_x, name_x, value_x) = (pad + 34, pad + 74, pad + 236);
        for (i, s) in path.stages.iter().enumerate() {
            let y0 = top + i as i32 * row;
            let cy = y0 + 34;
            let colour = s.grade.colour();
            if i == 2 {
                c.round_rect(pad, y0 + 2, right - pad, row - 6, 24, 0x222225);
            }
            if i + 1 < path.stages.len() {
                c.rect(dot_x - 1, cy + 14, 3, row - 28, mix(colour, 0x000000, 150));
            }
            c.disc(dot_x, cy, 10, colour);
            if i == 2 {
                c.ring(dot_x, cy, 17, 2, colour);
            }
            let base = cy + 11;
            text.draw(c, Face::Body, 28, name_x, base, 0xaaa79f, s.kind.name());
            let gw = text.width(Face::Body, 24, s.grade.name());
            if s.grade != Grade::Unknown {
                text.draw_right(c, Face::Body, 24, right - 36, base - 2, colour, s.grade.name());
            }
            let value = text.fit(Face::Body, 32, &s.value, right - 36 - gw - 30 - value_x);
            text.draw(c, Face::Body, 32, value_x, base, 0xffffff, &value);
            if !s.line.is_empty() {
                let line = text.fit(Face::Body, 24, &s.line, right - 36 - value_x);
                text.draw(c, Face::Body, 24, value_x, base + 38, 0x9a978f, &line);
            }
        }
        let base = c.h - pad - 45 + 8;
        let mut lx = pad + 36;
        for g in Grade::SCALE {
            c.disc(lx + 7, base - 8, 7, g.colour());
            lx += 22 + text.draw(c, Face::Body, 24, lx + 22, base, 0xaaa79f, g.name()) + 30;
        }
    });
}

pub fn frames(c: &mut Canvas, _text: &mut Text) {
    // A home console at Integer scaling, as Auto frames it on Every console: the monitor surround. Drawn at half the
    // screen's size (the frames are drawn in code at any size) and doubled, which the card's scaling hides.
    let (vw, vh) = (W as u32 / 2, H as u32 / 2);
    let (sw, sh) = (256u32, 224u32);
    let scale = 2u32;
    let game = crate::video::Rect {
        x: (vw - sw * scale) / 2,
        y: (vh - sh * scale) / 2,
        w: sw * scale,
        h: sh * scale,
    };
    let pic = sample_scene(sw, sh);
    let frame = bezel::generate(bezel::style_for("snes"), vw, vh, game, scale as f32);
    let rgba = bezel::compose(vw, vh, game, frame.as_ref(), sw, sh, &pic.px);
    let half = Image {
        w: vw,
        h: vh,
        px: rgba.chunks_exact(4).map(|p| (p[0] as u32) << 16 | (p[1] as u32) << 8 | p[2] as u32).collect(),
    };
    c.blit_scaled(&half, 0, 0, W, H);
}

pub fn last_game(c: &mut Canvas, text: &mut Text) {
    // The shelf on the left fades into the game's last picture on the right, as the start does.
    c.clear(pal().void);
    text.draw(c, Face::Body, 28, MARGIN, 72, pal().dust, "Home");
    label(c, text, 150, "Sample game", "Game Boy Advance · Continue", "gba");
    spine(c, text, 386, "Game Boy Advance", "12", "gba");
    spine(c, text, 470, "Super Nintendo", "9", "snes");
    spine(c, text, 554, "Mega Drive", "7", "megadrive");
    let mut game = vec![0u32; (W * H) as usize];
    let mut g = Canvas::new(&mut game, W as u32, H as u32);
    g.clear(VOID);
    scene_on_screen(&mut g, (240, 160));
    let (from, to) = (W * 30 / 100, W * 75 / 100);
    for (i, p) in c.buf.iter_mut().enumerate() {
        let x = i as i32 % W;
        if x >= from {
            let t = (((x - from) * 255) / (to - from)).clamp(0, 255) as u32;
            *p = mix(*p, game[i], t);
        }
    }
}

// ---- the menus' feel -------------------------------------------------------------------------------------------

/// Settings → Look & feel, where these switches are (`SETTINGS_GROUPS` in main.rs).
const SETTINGS_ROWS: [&str; 5] = ["Look", "Screen frames", "Sounds", "Haptics", "LEDs"];
const SETTINGS_TITLE: &str = "Look & feel";

fn settings_rows(values: &[(&str, &str)]) -> Vec<(&'static str, String)> {
    SETTINGS_ROWS
        .iter()
        .map(|n| (*n, values.iter().find(|(k, _)| k == n).map_or(">".to_string(), |(_, v)| v.to_string())))
        .collect()
}

pub fn motion(c: &mut Canvas, text: &mut Text) {
    // The band has just glided from Sounds down to LEDs: where it was a moment ago, fainter the longer ago.
    c.clear(pal().void);
    let (pad, row, top) = (30, 66, 132);
    let right = c.w - pad;
    for (k, amount) in [(2.0f32, 40u32), (2.5, 70), (3.0, 105), (3.5, 145)] {
        let y = top + (k * row as f32) as i32;
        c.round_rect(pad, y + 4, right - pad, row - 8, (row - 8) / 2, mix(pal().void, pal().band, amount));
    }
    let rows = settings_rows(&[("Screen frames", "Off"), ("Sounds", "30%"), ("Haptics", "Off"), ("LEDs", "Off >")]);
    list_page(c, text, SETTINGS_TITLE, &rows, 4, top);
    hints(c, text, &[("B", "Back"), ("A", "Open")]);
}

pub fn sounds(c: &mut Canvas, text: &mut Text) {
    c.clear(pal().void);
    let rows = settings_rows(&[("Screen frames", "Off"), ("Sounds", "‹ 30% ›"), ("Haptics", "Off"), ("LEDs", "Off >")]);
    list_page(c, text, SETTINGS_TITLE, &rows, 2, 132);
    // A soft click: a short tone that dies away.
    let (x0, y0, w, amp) = (MARGIN + 40, 830, c.w - 2 * MARGIN - 80, 70.0f32);
    for x in 0..w {
        let t = x as f32 / w as f32;
        let v = (t * 90.0).sin() * (-t * 6.0).exp() * amp;
        let h = (v.abs() as i32).max(1);
        c.rect(x0 + x, y0 - h, 1, 2 * h, pal().dust);
    }
    hints(c, text, &[("B", "Back"), ("A", "Switch"), ("Left/Right", "How loud")]);
}

pub fn haptics(c: &mut Canvas, text: &mut Text) {
    c.clear(pal().void);
    let rows = settings_rows(&[("Screen frames", "Off"), ("Sounds", "30%"), ("Haptics", "On"), ("LEDs", "Off >")]);
    list_page(c, text, SETTINGS_TITLE, &rows, 3, 132);
    hints(c, text, &[("B", "Back"), ("A", "Switch")]);
}

pub fn leds(c: &mut Canvas, text: &mut Text) {
    c.clear(pal().void);
    // The two sticks seen from above, their rings lit blue.
    let (r, g, b) = crate::leds::Hue::Blue.rgb();
    let glow = (r as u32) << 16 | (g as u32) << 8 | b as u32;
    for cx in [W / 2 - 260, W / 2 + 260] {
        let cy = 300;
        for k in (0..6).rev() {
            c.disc(cx, cy, 150 + k * 10, mix(pal().void, glow, 40 + (5 - k as u32) * 20));
        }
        c.ring(cx, cy, 150, 14, glow);
        c.disc(cx, cy, 130, mix(pal().void, 0x2a2a2e, 220));
        c.disc(cx, cy, 96, mix(pal().void, 0x3a3a40, 230));
    }
    let rows = [("Lights", "‹ Colour ›"), ("Colour", "Blue"), ("Brightness", "50%")];
    let (pad, row) = (30, 66);
    for (i, (name, value)) in rows.iter().enumerate() {
        let y = 560 + i as i32 * row;
        let base = y + row / 2 + 13;
        if i == 0 {
            c.round_rect(pad, y + 4, c.w - 2 * pad, row - 8, (row - 8) / 2, pal().band);
            let pill = text.width(Face::Body, 36, name) + 72;
            c.round_rect(pad, y + 4, pill, row - 8, (row - 8) / 2, pal().select);
        }
        text.draw(c, Face::Body, 36, pad + 36, base, if i == 0 { pal().ink } else { pal().text }, name);
        text.draw_right(c, Face::Body, 36, c.w - pad - 36, base, pal().text, value);
    }
    hints(c, text, &[("B", "Back"), ("Left/Right", "Change")]);
}

// ---- finding and playing -----------------------------------------------------------------------------------------

pub fn search(c: &mut Canvas, text: &mut Text) {
    // A system's list after R2: the letter of the jump over the spines, Search at the top of Home's lists.
    c.clear(pal().void);
    text.draw(c, Face::Body, 28, MARGIN, 72, pal().dust, "Game Boy Advance");
    let w = c.w - 2 * MARGIN;
    c.rect(MARGIN, 110, w, 72, pal().spine);
    text.draw(c, Face::Title, 37, MARGIN + 40, 159, pal().chalk, "Search");
    text.draw_right(c, Face::Body, 28, MARGIN + w - 28, 157, pal().dust, "All games");
    label(c, text, 200, "Sample game 6", "Game Boy Advance · Not played yet", "gba");
    for (k, name) in ["Sample game 7", "Sample game 8", "Sample game 9", "Sample game 10", "Sample game 11"]
        .iter()
        .enumerate()
    {
        spine(c, text, 420 + k as i32 * 84, name, "", "gba");
    }
    // The letter: the inverse of the page, as the selected row of the Minimal look.
    let (s, r) = (260, 44);
    let (x, y) = ((W - s) / 2, (H - s) / 2);
    c.round_rect(x, y, s, s, r, mix(pal().void, pal().text, 240));
    let lw = text.width(Face::Title, 200, "S");
    text.draw(c, Face::Title, 200, x + (s - lw) / 2, y + s / 2 + 70, pal().void, "S");
    hints(c, text, &[("A", "Play"), ("L2/R2", "Letter"), ("X", "Details"), ("B", "Back")]);
}

const PAUSE_ROWS: [&str; 6] = ["Continue", "Save", "Load", "Options", "Quick settings", "Quit"];

/// Settings › Remember where I quit: the pause menu's one Quit, and what the switch makes of it.
pub fn remember_quit(c: &mut Canvas, text: &mut Text) {
    palette::scoped(0, || {
        paused_game(c);
        let rows: Vec<(&str, String)> = PAUSE_ROWS.iter().map(|r| (*r, String::new())).collect();
        list_page(c, text, "Sample game", &rows, 5, 160);
        let mut y = c.h - 104;
        for line in ["On: the next start continues from this moment.", "Off: only the game's own saves are kept."] {
            text.draw(c, Face::Body, 30, 66, y, 0xcccccc, line);
            y += 44;
        }
    });
}

pub fn run_ahead(c: &mut Canvas, text: &mut Text) {
    palette::scoped(0, || {
        paused_game(c);
        // The rows of Options → Extras (`options::GROUPS`), with their own choice labels.
        let rows = vec![
            ("Fast-forward speed", "2x".to_string()),
            ("Rewind", "On".to_string()),
            ("Over the game", "Off".to_string()),
            ("Rumble", "Medium".to_string()),
            ("Performance", "Auto".to_string()),
            ("Fan", "Auto".to_string()),
            ("Run-ahead", "‹ 1 frame ›".to_string()),
        ];
        list_page(c, text, "Extras", &rows, 6, 160);
        let mut y = 160 + rows.len() as i32 * 66 + 60;
        for line in [
            "The game responds to your buttons 1 or 2 frames sooner.",
            "It switches itself off where a game can't keep up.",
        ] {
            text.draw(c, Face::Body, 30, 66, y, 0xcccccc, line);
            y += 44;
        }
    });
}

pub fn runs_on(c: &mut Canvas, text: &mut Text) {
    // A console played on RetroArch: the emulator, RetroArch's own picture settings, Save changes.
    c.clear(pal().void);
    let rows = vec![
        ("Runs on", "‹ RetroArch · mGBA ›".to_string()),
        ("Aspect ratio", "Core provided".to_string()),
        ("Integer scaling", "Off".to_string()),
        ("Smoothing", "Off".to_string()),
        ("Save changes", ">".to_string()),
    ];
    list_page(c, text, "Game Boy Advance", &rows, 0, 132);
    let mut y = 132 + rows.len() as i32 * 66 + 60;
    for line in ["Which emulator plays this console's games.", "Save states don't carry over to another one."] {
        text.draw(c, Face::Body, 30, 66, y, pal().text, line);
        y += 44;
    }
    hints(c, text, &[("B", "Back"), ("Left/Right", "Change")]);
}

// ---- accounts and the system ---------------------------------------------------------------------------------

pub fn achievements(c: &mut Canvas, text: &mut Text) {
    c.clear(pal().void);
    // One row, as the page has it: Log out with the account at the right; the status under it.
    let rows = vec![("Log out", "sample_player".to_string())];
    list_page(c, text, "RetroAchievements", &rows, 0, 132);
    text.draw(c, Face::Body, 30, 66, 132 + 66 + 60, pal().text, "Signed in as sample_player");
    // An unlock as it shows over a game: a dark pill with the badge, the title and the points.
    let (w, h) = (760, 132);
    let (x, y) = ((W - w) / 2, 700);
    c.round_rect(x, y, w, h, 28, KEYCAP);
    c.round_rect(x + 22, y + 22, 88, 88, 14, 0xd39a2c);
    c.disc(x + 66, y + 66, 26, 0xf2c14e);
    text.draw(c, Face::Body, 24, x + 134, y + 50, DUST, "Achievement unlocked · 10 points");
    text.draw(c, Face::Title, 37, x + 134, y + 98, CHALK, "First lap");
    hints(c, text, &[("B", "Back"), ("A", "Log out")]);
}

pub fn sync(c: &mut Canvas, text: &mut Text) {
    use crate::sync_page::ROWS;
    c.clear(pal().void);
    let rows: Vec<(&str, String)> = ROWS
        .iter()
        .take(6)
        .enumerate()
        .map(|(i, r)| (r.label(), if i < 4 { "On".to_string() } else { "Off".to_string() }))
        .collect();
    list_page(c, text, "Sync", &rows, 2, 132);
    text.draw(c, Face::Body, 30, 66, 132 + 6 * 66 + 60, pal().text, "Up to date with 2 devices.");
    hints(c, text, &[("B", "Back"), ("A", "Switch")]);
}

pub fn tailscale(c: &mut Canvas, text: &mut Text) {
    c.clear(pal().void);
    let rows = vec![
        ("Tailscale", "On".to_string()),
        ("Log out", "sample.ts.net".to_string()),
        ("Sign in with a key", String::new()),
        ("Name", "pocket".to_string()),
    ];
    list_page(c, text, "Tailscale", &rows, 0, 132);
    let mut y = 132 + 4 * 66 + 60;
    for line in ["Connected as sample.ts.net", "This Pocket: 100.64.0.7, pocket"] {
        text.draw(c, Face::Body, 30, 66, y, pal().text, line);
        y += 44;
    }
    hints(c, text, &[("B", "Back"), ("A", "Switch")]);
}

pub fn tools(c: &mut Canvas, text: &mut Text) {
    c.clear(pal().void);
    let tools = crate::tools::demo();
    let rows: Vec<(&str, String)> = tools.iter().take(8).map(|t| (t.name.as_str(), t.group.label().to_string())).collect();
    list_page(c, text, "Tools", &rows, 1, 132);
    if let Some(t) = tools.get(1) {
        let line = text.fit(Face::Body, 30, &t.about, c.w - 132);
        text.draw(c, Face::Body, 30, 66, 132 + rows.len() as i32 * 66 + 60, pal().text, &line);
    }
    hints(c, text, &[("B", "Back"), ("A", "Start")]);
}

pub fn perf_fan(c: &mut Canvas, text: &mut Text) {
    use crate::perf_fan_page::{PerfFanPage, ROWS};
    c.clear(pal().void);
    let page = PerfFanPage::new(crate::perf_fan::Prefs::default());
    let rows: Vec<(&str, String)> = ROWS.iter().map(|r| (page.label(*r), page.value(*r).to_string())).collect();
    list_page(c, text, "Performance & fan", &rows, 0, 132);
    let mut y = 132 + ROWS.len() as i32 * 66 + 70;
    text.draw(c, Face::Title, 37, 66, y, pal().chalk, "Now");
    y += 54;
    for (k, v) in [
        ("Hottest", "51 °C"),
        ("Fan", "17%, smooth, Auto"),
        ("Processor", "1.2 / 1.8 / 2.4 GHz"),
        ("Battery", "3.1 W"),
    ] {
        text.draw(c, Face::Body, 30, 66, y, pal().quiet, k);
        text.draw(c, Face::Body, 30, 330, y, pal().text, v);
        y += 44;
    }
    hints(c, text, &[("B", "Back"), ("Left/Right", "Change")]);
}

/// The shelf after a game, with Up next over its foot: the paper label, the box, the name and the reason.
pub fn up_next(c: &mut Canvas, text: &mut Text) {
    c.clear(pal().void);
    text.draw(c, Face::Body, 28, MARGIN, 72, pal().dust, "Home");
    label(c, text, 110, "Sample game", "Game Boy Advance · played 1 min ago", "gba");
    spine(c, text, 346, "Game Boy Advance", "12", "gba");
    spine(c, text, 430, "Super Nintendo", "9", "snes");
    spine(c, text, 514, "Mega Drive", "7", "megadrive");
    spine(c, text, 598, "PlayStation", "2", "psx");
    let (x, w, h) = (MARGIN, c.w - 2 * MARGIN, 132);
    let y = c.h - 112 - h;
    for k in 0..40 {
        c.shade_rect(0, y - 40 + k, c.w, 1, pal().void, (k * 255 / 40) as u8);
    }
    c.rect(0, y, c.w, c.h - y, pal().void);
    c.rect(x, y, w, h, pal().label);
    game_card::lift_shadow(c, x, y + h, w, pal().t.max(120));
    c.rect(x, y, STRIPE, h, stripe("psp"));
    let tx = x + STRIPE + 24;
    c.round_rect(tx + 12, y + 16, 72, h - 32, 6, mix(stripe("psp"), pal().label, 170));
    let tx = tx + 120;
    text.draw(c, Face::Body, 21, tx, y + 36, pal().label_dust, "Up next");
    text.draw(c, Face::Title, 37, tx, y + 80, pal().ink, "Sample game 2");
    text.draw(c, Face::Body, 28, tx, y + 116, pal().label_dust, "PSP · Same series");
    hints(c, text, &[("A", "Play it"), ("B", "Not now")]);
}

/// A game at Integer scaling with Pixel Path's line in the band above the picture, placed and painted by
/// `pixel_live` itself from its demo report.
pub fn pixel_path_live(c: &mut Canvas, text: &mut Text) {
    palette::scoped(0, || {
        c.clear(VOID);
        let Some(strip) = crate::pixel_live::demo("gba") else { return };
        let (px, py, pw, ph) = strip.picture;
        let scene = sample_scene(240, 160);
        c.blit_scaled(&scene, px, py, pw, ph);
        let at = crate::pixel_live::place(&strip, |st| crate::pixel_live::measure(text, &strip, st));
        let (w, h, buf) = crate::pixel_live::paint(text, &strip, at.style);
        let alpha = (at.style.alpha() * 255.0) as u32;
        for row in 0..h {
            for col in 0..w {
                let (x, y) = (at.x + col, at.y + row);
                let p = buf[(row * w + col) as usize];
                if p == crate::pixel_live::KEY || x < 0 || y < 0 || x >= c.w || y >= c.h {
                    continue;
                }
                let i = (y * c.w + x) as usize;
                c.buf[i] = mix(c.buf[i], p, alpha);
            }
        }
    });
}

/// The pause menu's Players page over the paused game: who has which controller.
pub fn players(c: &mut Canvas, text: &mut Text) {
    palette::scoped(0, || {
        paused_game(c);
        let rows = [
            ("Player 1", "Pocket".to_string()),
            ("Player 2", "DualSense".to_string()),
            ("Player 3", "8BitDo Pro 2".to_string()),
            ("Player 4", "No controller".to_string()),
        ];
        list_page(c, text, "Players", &rows, 1, 160);
        text.draw(
            c,
            Face::Body,
            30,
            66,
            160 + 4 * 66 + 60,
            0xcccccc,
            "A on a controller makes it the selected player.",
        );
        hints(c, text, &[("A", "This controller"), ("B", "Back")]);
    });
}

/// Credits & links of the Game Boy, from the real credits file, painted by the page itself (`credits_page`).
pub fn console_credits(c: &mut Canvas, text: &mut Text) {
    use crate::credits_page::{CreditsData, CreditsPage};
    let Some(credits) = crate::console_credits::for_system("gb") else {
        c.clear(pal().void);
        return;
    };
    let rest: Vec<_> = credits.family.iter().map(|r| (r.relation, r.name.clone())).collect();
    let data = CreditsData {
        credits,
        console_name: crate::consoles::for_system("gb").map_or("Game Boy".to_string(), |k| k.name.clone()),
        stripe: stripe("gb"),
        same_chip: Vec::new(),
        family_links: Vec::new(),
        family_text: CreditsData::group_text(&rest),
        games: Vec::new(),
        games_more: 0,
        games_ready: true,
    };
    let bg = pal().void;
    CreditsPage::new(data).paint(c, text, None, bg);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sample_scene_is_not_one_colour() {
        let img = sample_scene(240, 160);
        let mut colours: Vec<u32> = img.px.clone();
        colours.sort();
        colours.dedup();
        assert!(colours.len() > 8);
    }
}
