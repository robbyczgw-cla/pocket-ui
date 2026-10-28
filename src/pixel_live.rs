//! Pixel Path over the running game (docs/CURATOR.md "Pixel Path over the game"): one calm line that pocket-core's
//! HUD draws beside the picture, "● mGBA · 5× sharp · 60.00 fps · ≈34 ms · 48 kHz", the dot in the colour of the
//! overall grade. The moment something slips (pictures late, gaps in the sound, run-ahead switching itself off) the
//! dot turns orange and what happened is added in orange; once the last five seconds are clean again it calms down.
//!
//! - pocket-core builds the same [`Report`] the pause menu's page reads, but only while the strip is on and only
//!   twice a second, from the FPS counter's half-second (`overlay::Hud::frame`); [`Live::update`] turns it into a
//!   [`Strip`]. Nothing here runs per frame: the HUD draws the strip's texture, which is painted again only when the
//!   strip changed.
//! - [`place`] puts it where the picture isn't: the band above the picture, else the one below, else a column in a
//!   side band (the Nintendo DS); a picture that fills the screen gets it small and see-through in the top left
//!   corner. [`paint`] rasterises it in the HUD's style (dark pill, light text), the same in dark and light
//!   appearance, like every message over a game.
//! - Shown while **Over the game** is on **Pixel Path** (`over_game`: Settings → Playing, Options → Extras per console
//!   or game, HOME + Y for the running game). Since 2026-10-25 it doesn't wait for Settings → Curator → Pixel Path in
//!   the pause menu, which is the page's switch alone.

use crate::gfx::Canvas;
use crate::pixel_path::{self, Grade, Report};
use crate::text::{Face, Text};
use crate::video::Sharpness;

/// Updates a "run-ahead off" stays in the strip: five seconds at two a second, as long as late pictures and
/// underruns stay in the live window (`pixel_path::WINDOW_SECS`).
pub const HOLD_TICKS: u32 = 10;

/// Transparent in a painted strip (the HUD's key colour, `overlay::upload`).
pub const KEY: u32 = 0x010203;
const PILL: u32 = 0x1a1a1a;
const TEXT: u32 = 0xd9d6ce;
const DIM: u32 = 0x8a877f;
/// The panel edge and the picture are kept this far away.
const GAP: i32 = 8;
/// The fallback over a picture that fills the screen: this far from the corner.
const CORNER: i32 = 16;

/// One item of the strip: "5× sharp", or "3 late" in orange.
#[derive(Clone, Debug, PartialEq)]
pub struct Part {
    pub text: String,
    pub alert: bool,
}

/// What the strip shows and where the picture is (for [`place`]).
#[derive(Clone, Debug, PartialEq)]
pub struct Strip {
    /// The overall grade, as the page's heading shows it.
    pub grade: Grade,
    /// Something slipped in the last seconds: the dot is orange.
    pub alert: bool,
    pub parts: Vec<Part>,
    /// The picture on the panel (x, y, w, h) and the panel (w, h), from the report.
    pub picture: (i32, i32, i32, i32),
    pub view: (i32, i32),
}

impl Strip {
    /// The dot's colour: the grade's, orange while something slipped.
    pub fn dot(&self) -> u32 {
        if self.alert {
            Grade::Compromised.colour()
        } else {
            self.grade.colour()
        }
    }
}

/// pocket-core's side: what the strip remembers between updates.
#[derive(Default)]
pub struct Live {
    /// Cheats switched on (the Core stage), as pocket-core applied them.
    pub cheats: usize,
    /// Run-ahead had switched itself off at the last update.
    ra_off_seen: bool,
    /// Updates left with "run-ahead off" in the strip.
    ra_alert: u32,
}

impl Live {
    /// While the strip is hidden: keeps up with run-ahead, so showing it later doesn't announce an old switch-off.
    pub fn hidden(&mut self, ra_off: bool) {
        self.ra_off_seen = ra_off;
        self.ra_alert = 0;
    }

    /// The strip for the newest report (two a second).
    pub fn update(&mut self, r: &Report) -> Strip {
        let off = !r.run_ahead.off.is_empty();
        if off && !self.ra_off_seen {
            self.ra_alert = HOLD_TICKS;
        }
        self.ra_off_seen = off;
        let ra_alert = self.ra_alert > 0;
        self.ra_alert = self.ra_alert.saturating_sub(1);
        let known = pixel_path::Known {
            system_name: String::new(),
            cheats: self.cheats,
        };
        strip(r, pixel_path::path(r, &known).overall, ra_alert)
    }
}

/// The strip of report `r` with overall grade `grade`; `ra_alert`: run-ahead has just switched itself off.
pub fn strip(r: &Report, grade: Grade, ra_alert: bool) -> Strip {
    let calm = |t: String| Part { text: t, alert: false };
    let alert = |t: String| Part { text: t, alert: true };
    let mut parts = vec![calm(if r.core.is_empty() { "pocket-core".into() } else { r.core.clone() }), calm(scale_part(r))];
    let l = &r.live;
    // A 50 Hz game on the 60 Hz panel: the game's own rate is what tells whether it keeps up.
    let fps = if r.display.time_driven { l.emulated_fps } else { l.fps };
    parts.push(calm(format!("{fps:.2} fps")));
    let period_ms = 1000.0 / if r.display.refresh_hz > 0.0 { r.display.refresh_hz } else { 60.0 } as f32;
    let lat = pixel_path::Latency::estimate(period_ms, l.core_ms, r.run_ahead.frames);
    parts.push(calm(if r.run_ahead.frames > 0 {
        format!("≈{:.0} ms, {} ahead", lat.ms().max(0.0), r.run_ahead.frames)
    } else {
        format!("≈{:.0} ms", lat.ms())
    }));
    parts.push(calm(match &r.audio {
        Some(a) => format!("{} kHz", (a.out_rate as f32 / 1000.0).round()),
        None => "no sound".into(),
    }));
    if l.late > 0 {
        parts.push(alert(format!("{} late", l.late)));
    }
    if l.underruns > 0 {
        parts.push(alert(if l.underruns == 1 {
            "1 underrun".into()
        } else {
            format!("{} underruns", l.underruns)
        }));
    }
    if ra_alert {
        parts.push(alert("run-ahead off".into()));
    }
    let p = &r.picture;
    Strip {
        grade,
        alert: parts.iter().any(|p| p.alert),
        parts,
        picture: (p.x as i32, p.y as i32, p.w as i32, p.h as i32),
        view: (p.view_w.max(1) as i32, p.view_h.max(1) as i32),
    }
}

/// "5× sharp", "4× crisp" (SNES: whole lines), "4.47× soft", "6.75× soft, stretched"; a 3D core drawing above the console's lines: "4× res, soft".
fn scale_part(r: &Report) -> String {
    let filter = match Sharpness::parse(&r.picture.sharpness) {
        Sharpness::Sharp => "sharp",
        Sharpness::Crisp => "crisp",
        Sharpness::Soft => "soft",
    };
    if r.source.hw {
        if let Some(native) = pixel_path::native_lines(&r.system) {
            let lines = if r.source.frame_h > 0 { r.source.frame_h } else { r.source.base_h };
            let factor = lines as f32 / native as f32;
            if factor >= 1.5 {
                return format!("{} res, {filter}", pixel_path::scale_text(factor));
            }
        }
    }
    if r.picture.w == 0 {
        return filter.to_string();
    }
    let f = pixel_path::fit(r);
    let stretched = if f.stretch.abs() > 0.01 { ", stretched" } else { "" };
    format!("{} {filter}{stretched}", pixel_path::scale_text(f.sy))
}

/// How the strip is laid out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Style {
    /// One line in a band above or below the picture.
    Row,
    /// One item a line, in a band beside the picture.
    Column,
    /// One small line over a picture that fills the screen.
    Small,
}

impl Style {
    /// The HUD's other lines are 34 px; the strip stays quieter, but not below what reads at arm's length.
    fn font(self) -> u32 {
        match self {
            Style::Small => 24,
            _ => 28,
        }
    }

    fn pad(self) -> (i32, i32) {
        match self {
            Style::Small => (12, 6),
            _ => (16, 9),
        }
    }

    /// How opaque the HUD draws it: lighter over the picture.
    pub fn alpha(self) -> f32 {
        match self {
            Style::Small => 0.6,
            _ => 0.82,
        }
    }
}

const SEP: &str = "  ·  ";

/// The size `s` takes in `style`.
pub fn measure(text: &mut Text, s: &Strip, style: Style) -> (i32, i32) {
    let font = style.font();
    let (px, py) = style.pad();
    let dot = dot_room(font);
    match style {
        Style::Column => {
            let w = s.parts.iter().map(|p| text.width(Face::Body, font, &p.text)).max().unwrap_or(0);
            (2 * px + dot + w, 2 * py + s.parts.len() as i32 * line_h(font) - (line_h(font) - font as i32))
        }
        _ => {
            let sep = text.width(Face::Body, font, SEP);
            let w: i32 = s.parts.iter().map(|p| text.width(Face::Body, font, &p.text)).sum::<i32>() + sep * (s.parts.len() as i32 - 1).max(0);
            (2 * px + dot + w, 2 * py + font as i32)
        }
    }
}

fn line_h(font: u32) -> i32 {
    crate::type_scale::line(font, 10)
}

/// The dot and the space after it.
fn dot_room(font: u32) -> i32 {
    2 * dot_r(font) + 10
}

fn dot_r(font: u32) -> i32 {
    font as i32 / 3
}

/// Rasterises `s` in `style`: width, height and pixels (`KEY` is transparent).
pub fn paint(text: &mut Text, s: &Strip, style: Style) -> (i32, i32, Vec<u32>) {
    let (w, h) = measure(text, s, style);
    let (w, h) = (w.max(1), h.max(1));
    let font = style.font();
    let (px, py) = style.pad();
    let mut buf = vec![KEY; (w * h) as usize];
    let mut c = Canvas::new(&mut buf, w as u32, h as u32);
    let r = if style == Style::Column { 16 } else { h / 2 };
    c.round_rect(0, 0, w, h, r, PILL);
    let colour = |p: &Part| if p.alert { Grade::Compromised.colour() } else { TEXT };
    let dot_r = dot_r(font);
    let baseline = |line: i32| py + line * line_h(font) + font as i32 * 4 / 5;
    // The dot sits on the first line, centred on the lower-case letters.
    c.disc(px + dot_r, baseline(0) - crate::type_scale::mid(font), dot_r, s.dot());
    let x0 = px + dot_room(font);
    match style {
        Style::Column => {
            for (i, p) in s.parts.iter().enumerate() {
                text.draw(&mut c, Face::Body, font, x0, baseline(i as i32), colour(p), &p.text);
            }
        }
        _ => {
            let mut x = x0;
            for (i, p) in s.parts.iter().enumerate() {
                if i > 0 {
                    x += text.draw(&mut c, Face::Body, font, x, baseline(0), DIM, SEP);
                }
                x += text.draw(&mut c, Face::Body, font, x, baseline(0), colour(p), &p.text);
            }
        }
    }
    (w, h, buf)
}

/// Where a strip goes on the panel: its top left corner (x, y) and its style.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Place {
    pub x: i32,
    pub y: i32,
    pub style: Style,
}

/// Never over the picture where there is room beside it: centred in the band above the picture, lined up with the
/// picture's left edge (else the band below); else a column in the left (or right) band, at the top; else small in
/// the top left corner. `sizes(style)` is the strip's size in a style (`measure`).
pub fn place(s: &Strip, mut sizes: impl FnMut(Style) -> (i32, i32)) -> Place {
    let (vw, vh) = s.view;
    let (px, py, pw, ph) = if s.picture.2 > 0 && s.picture.3 > 0 { s.picture } else { (0, 0, vw, vh) };
    let (top, bottom) = (py, vh - (py + ph));
    let (left, right) = (px, vw - (px + pw));
    let (rw, rh) = sizes(Style::Row);
    if rw + 2 * GAP <= vw {
        // In line with the picture's left edge when the line fits from there, else from the panel's edge.
        let x = if px.max(GAP) + rw + GAP <= vw { px.max(GAP) } else { GAP };
        for (band, y0) in [(top, 0), (bottom, py + ph)] {
            if band >= rh + 2 * GAP {
                return Place {
                    x,
                    y: y0 + (band - rh) / 2,
                    style: Style::Row,
                };
            }
        }
    }
    let (cw, ch) = sizes(Style::Column);
    if ch + 2 * GAP <= vh {
        for (band, x0) in [(left, 0), (right, px + pw)] {
            if band >= cw + 2 * GAP {
                return Place {
                    x: x0 + (band - cw) / 2,
                    y: 3 * GAP,
                    style: Style::Column,
                };
            }
        }
    }
    Place {
        x: CORNER,
        y: CORNER,
        style: Style::Small,
    }
}

/// Made-up strips for snapshots and tests: the page's demo reports (`pixel_path::demo`: gba, snes, n64, runahead,
/// compromised, pal) and two placements of their own: `gb` (7×, no band wide enough: the corner) and `nds` (two
/// screens filling the height: a column beside them). `compromised` also has run-ahead switching off just now.
pub fn demo(case: &str) -> Option<Strip> {
    let (r, known) = match case {
        "gb" => {
            let (mut r, k) = pixel_path::demo("gba")?;
            r.core = "Gambatte".into();
            r.system = "gb".into();
            r.source.base_w = 160;
            r.source.base_h = 144;
            r.source.frame_w = 160;
            r.source.frame_h = 144;
            r.source.aspect = 160.0 / 144.0;
            r.picture.x = 60;
            r.picture.y = 36;
            r.picture.w = 1120;
            r.picture.h = 1008;
            (r, k)
        }
        "nds" => {
            let (mut r, k) = pixel_path::demo("gba")?;
            r.core = "melonDS DS".into();
            r.system = "nds".into();
            r.source.base_w = 256;
            r.source.base_h = 384;
            r.source.frame_w = 256;
            r.source.frame_h = 384;
            r.source.aspect = 256.0 / 384.0;
            r.picture.x = 260;
            r.picture.y = 0;
            r.picture.w = 720;
            r.picture.h = 1080;
            r.picture.scaling = "aspect".into();
            r.picture.sharpness = "crisp".into();
            (r, k)
        }
        other => pixel_path::demo(other)?,
    };
    let grade = pixel_path::path(&r, &known).overall;
    Some(strip(&r, grade, case == "compromised"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(s: &Strip) -> Vec<String> {
        s.parts.iter().map(|p| p.text.clone()).collect()
    }

    #[test]
    fn the_strip_says_core_scale_pace_latency_and_sound() {
        let s = demo("gba").unwrap();
        assert_eq!(texts(&s), ["mGBA", "5× sharp", "60.00 fps", "≈34 ms", "48 kHz"]);
        assert!(!s.alert);
        assert_eq!(s.grade, Grade::Faithful, "resampled sound keeps a pocket-core game at Faithful");
        assert_eq!(s.dot(), Grade::Faithful.colour());
        assert_eq!(texts(&demo("snes").unwrap())[1], "4× crisp", "whole lines, uneven columns");
        let n64 = demo("n64").unwrap();
        assert_eq!(texts(&n64)[1], "4× res, soft", "960 lines of the N64's 240");
        assert_eq!(n64.grade, Grade::Enhanced);
        assert_eq!(texts(&demo("runahead").unwrap())[3], "≈19 ms, 1 ahead");
        // A 50 Hz game: its own rate, not the panel's.
        let pal = pixel_path::demo("pal").unwrap().0;
        assert_eq!(texts(&strip(&pal, Grade::Compromised, false))[2], format!("{:.2} fps", pal.live.emulated_fps));
    }

    #[test]
    fn late_pictures_underruns_and_run_ahead_going_off_turn_it_orange() {
        let s = demo("compromised").unwrap();
        assert!(s.alert);
        assert_eq!(s.dot(), Grade::Compromised.colour());
        let alerts: Vec<String> = s.parts.iter().filter(|p| p.alert).map(|p| p.text.clone()).collect();
        assert_eq!(alerts, ["13 late", "3 underruns", "run-ahead off"]);
        let (mut r, _) = pixel_path::demo("gba").unwrap();
        r.live.underruns = 1;
        let mut live = Live::default();
        let one = live.update(&r);
        assert!(one.alert && one.parts.last().unwrap().text == "1 underrun");
        r.live.underruns = 0;
        assert!(!live.update(&r).alert, "calm again once the window is clean");
    }

    #[test]
    fn run_ahead_switching_off_shows_for_five_seconds_then_calms_down() {
        let (mut r, _) = pixel_path::demo("runahead").unwrap();
        let mut live = Live::default();
        assert!(!live.update(&r).alert);
        r.run_ahead.frames = 0;
        r.run_ahead.off = "Run-ahead is off: too slow for this game".into();
        let shown: Vec<bool> = (0..HOLD_TICKS + 3).map(|_| live.update(&r).alert).collect();
        assert_eq!(shown.iter().filter(|a| **a).count(), HOLD_TICKS as usize, "{shown:?}");
        assert!(shown[0] && !shown[HOLD_TICKS as usize]);
        // Switched off while the strip was hidden: no announcement when it comes up later.
        let mut later = Live::default();
        later.hidden(true);
        assert!(!later.update(&r).alert);
        // Tried again and off again: announced again.
        r.run_ahead.off.clear();
        live.update(&r);
        r.run_ahead.off = "Run-ahead is off".into();
        assert!(live.update(&r).alert);
    }

    fn placed(case: &str) -> Place {
        let s = demo(case).unwrap();
        let mut text = Text::builtin();
        place(&s, |st| measure(&mut text, &s, st))
    }

    /// The strip's rectangle never meets the picture, unless it is the small corner fallback.
    fn clear_of_picture(case: &str) -> bool {
        let s = demo(case).unwrap();
        let mut text = Text::builtin();
        let at = place(&s, |st| measure(&mut text, &s, st));
        let (w, h) = measure(&mut text, &s, at.style);
        let (px, py, pw, ph) = s.picture;
        at.x + w <= px || at.x >= px + pw || at.y + h <= py || at.y >= py + ph
    }

    #[test]
    fn it_goes_where_the_picture_isnt() {
        // GBA 5×: 140 px above the picture, in line with its left edge.
        let gba = placed("gba");
        assert_eq!((gba.style, gba.x), (Style::Row, 20));
        assert!(gba.y > 8 && gba.y + 46 < 140, "{gba:?}");
        for case in ["gba", "snes", "n64", "runahead", "nds"] {
            assert!(clear_of_picture(case), "{case}: {:?}", placed(case));
        }
        // N64 at Aspect: 75 px bands, enough for the line.
        assert_eq!(placed("n64").style, Style::Row);
        // Two screens filling the height: a column in the side band.
        let nds = placed("nds");
        assert_eq!(nds.style, Style::Column);
        assert!(nds.x >= 8 && nds.x < 260);
        // Game Boy 7× (36 px bands, 60 px beside) and a stretched full screen: small in the corner.
        assert_eq!(
            placed("gb"),
            Place {
                x: 16,
                y: 16,
                style: Style::Small
            }
        );
        assert_eq!(placed("compromised").style, Style::Small);
        // Nothing known about the picture yet: the corner too.
        let mut s = demo("gba").unwrap();
        s.picture = (0, 0, 0, 0);
        let mut text = Text::builtin();
        assert_eq!(place(&s, |st| measure(&mut text, &s, st)).style, Style::Small);
    }

    #[test]
    fn painting_keeps_the_corners_clear_and_uses_the_grade_and_alert_colours() {
        let mut text = Text::builtin();
        for case in ["gba", "compromised", "nds"] {
            let s = demo(case).unwrap();
            for style in [Style::Row, Style::Column, Style::Small] {
                let (w, h, buf) = paint(&mut text, &s, style);
                assert_eq!((w, h), measure(&mut text, &s, style));
                assert_eq!(buf.len(), (w * h) as usize);
                assert_eq!(buf[0], KEY, "{case} {style:?}: rounded corner is see-through");
                assert!(buf.contains(&s.dot()), "{case} {style:?}: the dot");
            }
        }
        let (w, _, _) = paint(&mut text, &demo("gba").unwrap(), Style::Row);
        // 28 px type since fix/contrast (24 before): about 640 px for the five items, half the panel.
        assert!(w < 700, "small: {w} px");
    }
}
