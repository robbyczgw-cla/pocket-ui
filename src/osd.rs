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
fn layout(text: &mut Text, d: &Data, w: i32) -> (String, i32, i32) {
    let gap = 24;
    let value_col = part(VALUE_COL, w).max(text.width(Face::Body, FONT, &d.value) + gap);
    let room = (w - 2 * part(PAD, w) - value_col - part(MIN_BAR, w)).max(part(LABEL_COL, w));
    let label = text.fit(Face::Body, FONT, &d.label, room - gap);
    let label_col = part(LABEL_COL, w).max(text.width(Face::Body, FONT, &label) + gap);
    (label, label_col, value_col)
}

/// A length of the design's pill (`W` wide) in a pill `w` wide: the same share of it.
fn part(px: i32, w: i32) -> i32 {
    if w == W { px } else { (px * w + W / 2) / W }
}

/// The pill's box on a canvas `cw` x `ch` (left, top, width, height), at the top centre. The design is made for a
/// canvas 1280 and more wide (the RG35XX Plus's 640x480 is laid out at 1280x960) or of 900 lines and more (the Mini V2's
/// 1240x1080): there it is `W` x `H` at `TOP`, 53 %
/// of the Plus's width, in the header's band between a sheet's title and the clock. A narrower canvas painted natively
/// (the Brick's 1024x768) takes the same share of its width and of the header's height (`kit::chrome`); its letters
/// keep their size (as every page's do there), so the band has no room between the title and the clock's pill (with
/// the battery, Large text, German): the pill comes down under the band (`kit::header_bottom`, the top line's band on
/// Home), never over the clock or a title.
pub fn frame(cw: i32, ch: i32) -> (i32, i32, i32, i32) {
    if cw >= 1280 || !crate::kit::short_screen(ch) {
        return ((cw - W) / 2, TOP, W, H);
    }
    let w = (W * cw + 640) / 1280;
    ((cw - w) / 2, crate::kit::header_bottom(ch), w, crate::kit::chrome(H, ch))
}

/// Draws the pill with its top-left corner at (x, y). `bg` is the pill colour.
pub fn paint(c: &mut Canvas, text: &mut Text, x: i32, y: i32, d: &Data, bg: u32) {
    paint_in(c, text, (x, y, W, H), d, bg);
}

/// `paint` in a pill of any size (`frame`): the pads, columns and bar are the design's shares of it.
pub fn paint_in(c: &mut Canvas, text: &mut Text, (x, y, w, h): (i32, i32, i32, i32), d: &Data, bg: u32) {
    crate::retro::window(c, x, y, w, h, h / 2, bg);
    let base = y + h / 2 + crate::type_scale::mid(FONT);
    let (label, label_col, value_col) = layout(text, d, w);
    let pad = part(PAD, w);
    let bar_h = if h == H { BAR_H } else { (BAR_H * h + H / 2) / H };
    text.draw(c, Face::Body, FONT, x + pad, base, CHALK, &label);
    text.draw_right(c, Face::Body, FONT, x + w - pad, base, CHALK, &d.value);
    let (bx, bw) = (x + pad + label_col, w - 2 * pad - label_col - value_col);
    let by = y + (h - bar_h) / 2;
    crate::retro::pill(c, bx, by, bw, bar_h, bar_h / 2, TRACK);
    if d.centered {
        let mid = bx + bw / 2;
        let end = bx + (bw as f32 * d.frac) as i32;
        let (l, r) = (mid.min(end), mid.max(end));
        if r - l >= 2 {
            c.rect(l, by, r - l, bar_h, LABEL);
        }
        c.rect(mid - 1, by - 5, 3, bar_h + 10, CHALK);
    } else {
        let fill = ((bw as f32 * d.frac) as i32).max(if d.frac > 0.0 { bar_h } else { 0 });
        if fill > 0 {
            crate::retro::pill(c, bx, by, fill, bar_h, bar_h / 2, LABEL);
        }
    }
}

/// Draws a level bar of the quick settings rows (same look, any size).
pub fn paint_bar(c: &mut Canvas, x: i32, y: i32, w: i32, frac: f32, centered: bool, fill: u32, track: u32) {
    let by = y - BAR_H / 2;
    crate::retro::pill(c, x, by, w, BAR_H, BAR_H / 2, track);
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
            crate::retro::pill(c, x, by, f, BAR_H, BAR_H / 2, fill);
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
        let (x, y, w, h) = frame(c.w, c.h);
        // A moment over the page, as a notice card or a jump's letter (`text::boxes::covered_since`): what lies under it
        // is hidden on purpose and comes back as it goes (on the Brick the "Continue" line under the header's band).
        // Its place stays checked: never on the clock or a title (`frame`, its test).
        crate::text::boxes::covered_since(usize::MAX, c, (x, y, w, h));
        crate::motion::paint_toast(c, self.appeared, Instant::now(), Some(life), (x, y, w, h), 20.0, |c, dy| {
            paint_in(c, text, (x, y + dy, w, h), &self.data, SPINE)
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_text_roundtrips_and_rejects_junk() {
        let d = Data::new("Colour", "Warmer 3", 0.65, true);
        assert_eq!(Data::from_wire(&d.to_wire()), Some(Data::new("Colour", "Warmer 3", 0.65, true)));
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
        for (label, value) in [("Colour", "Warmer 4"), ("Farbe", "Wärmer 4"), ("Brightness", "60%"), ("A very long made-up setting name", "Cooler 10")] {
            let d = Data::new(label, value, 0.5, true);
            let (shown, label_col, value_col) = layout(&mut text, &d, W);
            assert!(text.width(Face::Body, FONT, &shown) < label_col, "{label}: the bar would start under the label");
            assert!(W - 2 * PAD - label_col - value_col >= MIN_BAR, "{label}: the bar keeps a usable length");
            if label == "Colour" {
                assert_eq!(shown, label, "a name that fits is never cut");
            }
        }
    }

    /// Whether a pill at `r` stays clear of the header's title and its clock (a sheet's: the quick settings' title at
    /// Title 50 and the status pill, the widest of the pages over Home) on a canvas `cw` x `ch`, in every text size.
    fn clear_of_header(text: &mut Text, (x, y, w, h): (i32, i32, i32, i32), cw: i32, ch: i32) -> Result<(), String> {
        use crate::kit::{BTN_PAD, PAD, SMALL};
        let mut out = Ok(());
        for size in [crate::type_scale::TextSize::Normal, crate::type_scale::TextSize::Large] {
            crate::type_scale::set(size);
            let (top, band) = (crate::kit::header_top(ch), crate::kit::header_bottom(ch));
            let overlaps = |l: i32, r: i32| x < r && l < x + w && y < band && top < y + h;
            let title_right = PAD + BTN_PAD + ["Quick settings", "Configurações rápidas"].iter().map(|t| text.width(Face::Title, 50, t)).max().unwrap();
            let corner = crate::status_corner::Corner { clock: Some("23:59".into()), wifi: true, battery: Some("100%, charging".into()) };
            let pill = crate::status_corner::Pill { right: cw - PAD, top, h: crate::kit::header_pill(ch), px: SMALL, pad: BTN_PAD };
            let clock_left = pill.layout(&corner, |s| text.width(Face::Body, SMALL, s)).unwrap().1;
            if overlaps(PAD, title_right) {
                out = Err(format!("{cw}x{ch} {size:?}: over the title ({x}..{} vs ..{title_right})", x + w));
            } else if overlaps(clock_left, cw - PAD) {
                out = Err(format!("{cw}x{ch} {size:?}: over the clock ({x}..{} vs {clock_left}..)", x + w));
            }
        }
        crate::type_scale::set(crate::type_scale::TextSize::Normal);
        out
    }

    /// Neon's Brick audit (20.11.2026, #3 and anim #6): the pill was 680 px on the Brick's 1024 too (66 % of its width
    /// against the Plus's 53 %) and sat on the clock ("65%01:00") and over the quick settings' title.
    #[test]
    fn on_the_brick_the_pill_keeps_the_plus_share_and_stays_off_the_clock_and_title() {
        let mut text = Text::new();
        let plus = frame(1280, 960);
        let brick = frame(1024, 768);
        let share = |w: i32, cw: i32| w as f32 * 100.0 / cw as f32;
        assert!((share(brick.2, 1024) - share(plus.2, 1280)).abs() <= 5.0, "width {} of 1024 vs {} of 1280", brick.2, plus.2);
        assert!((share(brick.3, 768) - share(plus.3, 960)).abs() <= 5.0, "height {} of 768 vs {} of 960", brick.3, plus.3);
        assert_eq!(brick.0 * 2 + brick.2, 1024, "centred");
        clear_of_header(&mut text, brick, 1024, 768).unwrap();
        assert!(clear_of_header(&mut text, ((1024 - W) / 2, TOP, W, H), 1024, 768).is_err(), "the audit's pill: the check sees it");
        // The label, the value and a bar still fit in the narrower pill.
        for (label, value) in [("Volume", "Muted"), ("Brightness", "100%"), ("Helligkeit", "Extra dunkel 3"), ("Colour", "Warmer 4")] {
            let (_, label_col, value_col) = layout(&mut text, &Data::new(label, value, 0.5, false), brick.2);
            assert!(brick.2 - 2 * part(PAD, brick.2) - label_col - value_col >= part(MIN_BAR, brick.2), "{label} {value}");
        }
        // Everything painted stays in its box, under the header's band.
        let mut buf = vec![0u32; 1024 * 768];
        let mut c = Canvas::new(&mut buf, 1024, 768);
        paint_in(&mut c, &mut text, brick, &Data::percent("Volume", 65), SPINE);
        for (i, p) in buf.iter().enumerate() {
            let (px, py) = ((i % 1024) as i32, (i / 1024) as i32);
            if *p != 0 {
                assert!(px >= brick.0 && px < brick.0 + brick.2 && py >= brick.1 && py < brick.1 + brick.3, "pixel {px},{py}");
            }
        }
        // The other screens keep the design's pill where it was: the Plus (1280x960), 720x720 (1440x1440), the Mini V2,
        // the Smart Pro's 1280x720, 1920x1080.
        for (cw, ch) in [(1280, 960), (1440, 1440), (1240, 1080), (1280, 720), (1920, 1080)] {
            assert_eq!(frame(cw, ch), ((cw - W) / 2, TOP, W, H), "{cw}x{ch}");
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
