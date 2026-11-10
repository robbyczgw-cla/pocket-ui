//! Settings-style painter for the existing Extras page. Rows stay visible offline and scroll at Large text.
use crate::{emu_menu::Row, gfx::Canvas, kit, palette::*, text::{Face, Text}, touch::HitMap};

pub fn hints(action: Option<&str>) -> Vec<(&str, &str)> {
    let mut hints = Vec::new();
    if action == Some("Change") { hints.push(("Left/Right", "Change")); }
    if let Some(action) = action { hints.push(("A", action)); }
    hints.push(("B", "Back"));
    hints
}

pub struct Footer<'a> { pub action: Option<&'a str>, pub alert: bool, pub remove: bool }

pub fn paint(c: &mut Canvas, text: &mut Text, hits: &mut HitMap, rows: &[Row], sel: usize, subtitle: Option<&str>, footer: Footer<'_>) {
    hits.clear();
    c.clear(pal().void);
    let left = kit::PAD + kit::BTN_PAD;
    text.draw(c, Face::Title, 50, left, kit::PAD + 58, pal().chalk, "Extras");
    text.draw_right(c, Face::Body, 21, c.w - left, kit::PAD + 56, pal().dust, "Settings › Games");
    let selected = rows.get(sel);
    let mut hints = hints(footer.action);
    if footer.remove { hints.insert(hints.len() - 1, ("X", "Remove")); }
    let mut top = kit::PAD + 90;
    if let Some(subtitle) = subtitle {
        // A page-wide reason must stay whole when Pictures adds wide controls to the footer.
        let lines = text.wrap_prose(Face::Body, 21, subtitle, c.w - 2 * left);
        let step = crate::type_scale::line(21, 8);
        for (i, line) in lines.iter().enumerate() {
            text.draw(c, Face::Body, 21, left, kit::PAD + 102 + i as i32 * step, pal().dust, line);
        }
        top = kit::PAD + 124 + lines.len().saturating_sub(1) as i32 * step;
    }
    let row_h = crate::type_scale::row(62, 32);
    // The explanation stays whole (Box art's two paragraphs ended in "no account i…" beside the pill on the Brick's
    // 1024x768 with four lines): as many lines as leave two rows above it, never fewer than four; the rows scroll.
    let room = (kit::footer_baseline(c.h) - 12 - top - 2 * row_h) / crate::type_scale::line(kit::SMALL, 10);
    let bottom = kit::footer(c, text, hits, &hints, selected.map_or("", |r| r.desc.as_str()), (room.max(0) as usize).max(4),
        if footer.alert { pal().alert } else { pal().text });
    let visible = ((bottom - 12 - top) / row_h).max(1) as usize;
    let first = sel.saturating_add(1).saturating_sub(visible);
    let width = c.w - 2 * kit::PAD;
    for (i, row) in rows.iter().enumerate().skip(first).take(visible) {
        let y = top + (i - first) as i32 * row_h;
        // Reserve the full status before fitting the name. Short values remain at the normal Settings size.
        let (value_px, value) = text.fit_sizes(Face::Body, &[28, 25, 22], &row.value, width * 3 / 5);
        let value_w = text.width(Face::Body, value_px, &value);
        let (name_px, name) = text.fit_sizes(Face::Body, &[32, 28, 25], &row.name, width - 2 * kit::BTN_PAD - value_w - 24);
        let base = kit::row_label(c, text, (kit::PAD, y, width, row_h - 4), name_px, &name, i == sel);
        kit::row_value(c, text, value_px, c.w - left, base, &value, i == sel);
        hits.row(crate::touch::Area::new(kit::PAD, y, width, row_h - 4), i);
    }
    hits.list(sel, rows.len(), 0.0, 0.0);
    kit::scrollbar(c, top, visible as i32 * row_h, first, visible, rows.len());
}
