//! The on-screen display for brightness, volume and the display look: a dark pill at the top centre
//! with the name, a level bar and the value. The same picture is drawn by pocket-ui over the shelf
//! and, as a texture, by pocket-core's overlay over a running game (sent as `osd <wire text>`).

use std::time::{Duration, Instant};

use crate::gfx::Canvas;
use crate::palette::*;
use crate::text::{Face, Text};

pub const W: i32 = 680;
pub const H: i32 = 76;
pub const TOP: i32 = 28;
pub const SHOW_FOR: Duration = Duration::from_millis(1400);

const FONT: u32 = 28;
const PAD: i32 = 36;
const LABEL_COL: i32 = 230;
const VALUE_COL: i32 = 118;
const MIN_BAR: i32 = 120;
const BAR_H: i32 = 14;
const TRACK: u32 = 0x3a3a40;

/// What one OSD shows.
#[derive(Clone, Debug, PartialEq)]
pub struct Data {
    pub label: String,
    pub value: String,
    /// 0..=1; for a centred bar 0.5 is the middle.
    pub frac: f32,
    /// The bar grows out of its middle (colour temperature, contrast).
    pub centered: bool,
}

impl Data {
    pub fn new(label: impl Into<String>, value: impl Into<String>, frac: f32, centered: bool) -> Self {
        Data {
            label: label.into(),
            value: value.into(),
            frac: frac.clamp(0.0, 1.0),
            centered,
        }
    }

    pub fn percent(label: &str, pct: u8) -> Self {
        Data::new(
            label,
            if pct == 0 && label == "Volume" {
                "Muted".to_string()
            } else {
                format!("{pct}%")
            },
            pct as f32 / 100.0,
            false,
        )
    }

    /// Text for pocket-core's command FIFO: `Name|value|0.60|0`.
    pub fn to_wire(&self) -> String {
        format!("{}|{}|{:.3}|{}", self.label, self.value, self.frac, self.centered as u8)
    }

    pub fn from_wire(s: &str) -> Option<Data> {
        let mut p = s.split('|');
        let (label, value) = (p.next()?, p.next()?);
        let frac: f32 = p.next()?.parse().ok()?;
        let centered = p.next()? == "1";
        Some(Data::new(label, value, frac, centered))
    }
}

/// The label to draw and the widths of the label and value columns. The columns are the usual fixed ones, wider
/// when the text needs it (longer name, Large text size, German), so the bar never starts under its own label;
/// the bar keeps `MIN_BAR` and the label is cut ("…") only when nothing else is left to give.
fn layout(text: &mut Text, d: &Data) -> (String, i32, i32) {
    let gap = 24;
    let value_col = VALUE_COL.max(text.width(Face::Body, FONT, &d.value) + gap);
    let room = (W - 2 * PAD - value_col - MIN_BAR).max(LABEL_COL);
    let label = text.fit(Face::Body, FONT, &d.label, room - gap);
    let label_col = LABEL_COL.max(text.width(Face::Body, FONT, &label) + gap);
    (label, label_col, value_col)
}

/// Draws the pill with its top-left corner at (x, y). `bg` is the pill colour.
pub fn paint(c: &mut Canvas, text: &mut Text, x: i32, y: i32, d: &Data, bg: u32) {
    c.round_rect(x, y, W, H, H / 2, bg);
    let base = y + H / 2 + crate::type_scale::mid(FONT);
    let (label, label_col, value_col) = layout(text, d);
    text.draw(c, Face::Body, FONT, x + PAD, base, CHALK, &label);
    text.draw_right(c, Face::Body, FONT, x + W - PAD, base, CHALK, &d.value);
    let (bx, bw) = (x + PAD + label_col, W - 2 * PAD - label_col - value_col);
    let by = y + (H - BAR_H) / 2;
    c.round_rect(bx, by, bw, BAR_H, BAR_H / 2, TRACK);
    if d.centered {
        let mid = bx + bw / 2;
        let end = bx + (bw as f32 * d.frac) as i32;
        let (l, r) = (mid.min(end), mid.max(end));
        if r - l >= 2 {
            c.rect(l, by, r - l, BAR_H, LABEL);
        }
        c.rect(mid - 1, by - 5, 3, BAR_H + 10, CHALK);
    } else {
        let fill = ((bw as f32 * d.frac) as i32).max(if d.frac > 0.0 { BAR_H } else { 0 });
        if fill > 0 {
            c.round_rect(bx, by, fill, BAR_H, BAR_H / 2, LABEL);
        }
    }
}

/// Draws a level bar of the quick settings rows (same look, any size).
pub fn paint_bar(c: &mut Canvas, x: i32, y: i32, w: i32, frac: f32, centered: bool, fill: u32, track: u32) {
    let by = y - BAR_H / 2;
    c.round_rect(x, by, w, BAR_H, BAR_H / 2, track);
    if centered {
        let mid = x + w / 2;
        let end = x + (w as f32 * frac.clamp(0.0, 1.0)) as i32;
        let (l, r) = (mid.min(end), mid.max(end));
        if r - l >= 2 {
            c.rect(l, by, r - l, BAR_H, fill);
        }
        c.rect(mid - 1, by - 5, 3, BAR_H + 10, fill);
    } else {
        let f = ((w as f32 * frac.clamp(0.0, 1.0)) as i32).max(if frac > 0.0 { BAR_H } else { 0 });
        if f > 0 {
            c.round_rect(x, by, f, BAR_H, BAR_H / 2, fill);
        }
    }
}

/// An OSD on the shelf: what it shows and since when (it stays SHOW_FOR after the last change), and when it came
/// up (it slides in once, not on every step of the level).
pub struct Shown {
    pub data: Data,
    pub since: Instant,
    pub appeared: Instant,
}

impl Shown {
    pub fn new(data: Data) -> Self {
        let now = Instant::now();
        Shown { data, since: now, appeared: now }
    }

    /// `data` in place of what `before` showed: keeps its place on screen when it is still up.
    pub fn update(before: Option<&Shown>, data: Data) -> Self {
        let mut s = Shown::new(data);
        if let Some(b) = before.filter(|b| b.active()) {
            s.appeared = b.appeared;
        }
        s
    }

    pub fn active(&self) -> bool {
        self.since.elapsed() < SHOW_FOR
    }

    pub fn expires(&self) -> Instant {
        self.since + SHOW_FOR
    }

    /// Comes down into place and fades in as it appears, fades out as it goes (`motion::paint_toast`).
    pub fn paint(&self, c: &mut Canvas, text: &mut Text) {
        let life = self.since.saturating_duration_since(self.appeared) + SHOW_FOR;
        crate::motion::paint_toast(c, self.appeared, Instant::now(), Some(life), ((c.w - W) / 2, TOP, W, H), 20.0, |c, dy| {
            paint(c, text, (c.w - W) / 2, TOP + dy, &self.data, SPINE)
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_text_roundtrips_and_rejects_junk() {
        let d = Data::new("Colour temperature", "Warm 3", 0.65, true);
        assert_eq!(Data::from_wire(&d.to_wire()), Some(Data::new("Colour temperature", "Warm 3", 0.65, true)));
        assert_eq!(Data::from_wire("Volume|45%|0.450|0").unwrap().value, "45%");
        assert!(Data::from_wire("nonsense").is_none());
        assert!(Data::from_wire("a|b|notanumber|0").is_none());
    }

    #[test]
    fn volume_zero_reads_muted_and_levels_are_clamped() {
        assert_eq!(Data::percent("Volume", 0).value, "Muted");
        assert_eq!(Data::percent("Brightness", 0).value, "0%");
        assert_eq!(Data::new("x", "y", 3.0, false).frac, 1.0);
        assert_eq!(Data::new("x", "y", -1.0, false).frac, 0.0);
    }

    #[test]
    fn the_bar_starts_after_its_label_whatever_the_name() {
        let mut text = Text::new();
        for (label, value) in [("Colour temperature", "Warm 4"), ("Farbtemperatur", "Warm 4"), ("Brightness", "60%"), ("A very long made-up setting name", "Cooler 10")] {
            let d = Data::new(label, value, 0.5, true);
            let (shown, label_col, value_col) = layout(&mut text, &d);
            assert!(text.width(Face::Body, FONT, &shown) < label_col, "{label}: the bar would start under the label");
            assert!(W - 2 * PAD - label_col - value_col >= MIN_BAR, "{label}: the bar keeps a usable length");
            if label == "Colour temperature" {
                assert_eq!(shown, label, "a name that fits is never cut");
            }
        }
    }

    #[test]
    fn the_pill_draws_inside_its_box_and_the_bar_follows_the_level() {
        let mut text = Text::new();
        let mut render = |frac: f32, centered: bool| -> Vec<u32> {
            let mut buf = vec![0u32; (W * H) as usize];
            let mut c = Canvas::new(&mut buf, W as u32, H as u32);
            paint(&mut c, &mut text, 0, 0, &Data::new("Brightness", "60%", frac, centered), SPINE);
            buf
        };
        let lit = |buf: &[u32]| buf.iter().filter(|p| **p == LABEL).count();
        let (low, high) = (lit(&render(0.2, false)), lit(&render(0.8, false)));
        assert!(high > low * 2, "{low} {high}");
        // Corners stay empty: the pill is round.
        assert_eq!(render(0.5, false)[0], 0);
        // A centred bar at 0.5 has only the tick; away from the middle it fills one side.
        assert!(lit(&render(0.5, true)) < 10 * (BAR_H as usize + 10));
        assert!(lit(&render(0.9, true)) > lit(&render(0.55, true)));
    }
}
