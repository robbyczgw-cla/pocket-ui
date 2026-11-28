//! The game's real manual on screen (Details → Manual, `app_manual.rs` loads and paints it; docs/SCREENSCRAPER.md
//! "Manuals"). One page at a time, whole on the screen; L1/R1 (and Left/Right while the page is whole) turn the page;
//! A zooms in the way the pause menu's Zoom does (whole → 2x → 3x → whole), the D-pad moves the part in view; B leaves
//! the zoom first (as on the game box's real back), then the manual.
//! On a touch screen a swipe to the left or right turns the page, and so does a tap on the right or left half ([`ManualView::touch`]).
//!
//! The pages are the manual's own pictures (`screenscraper::manual::kept`). They are decoded on the App's loading
//! thread ([`ManualView::wanted`]: this page first, then the next and the one before), never on the UI thread; a page
//! that is not decoded yet shows its number, and a page that is not a picture says so.

use crate::gfx::Canvas;
use crate::image::Image;
use crate::kit::{self, PAD, SMALL};
use crate::pad::Button;
use crate::screenscraper::manual::Kept;
use crate::text::{Face, Text};
use crate::touch::{Gesture, HitMap};
use crate::type_scale;
use std::sync::Arc;

/// How much larger than whole a page gets: A steps through them (1 is the whole page).
pub const LEVELS: [u32; 3] = [1, 2, 3];
/// The longest side a page is decoded at: a 3x part still has the scan's own pixels on the Brick's 1024x768, and a
/// larger scan does not hold more than ~8 MB in memory.
pub const MAX_EDGE: u32 = 2400;
/// Pages kept decoded: this one and the two beside it.
pub const KEEP: usize = 3;

/// What the page shows while it has no pages.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Wait {
    /// Fetched from ScreenScraper now.
    Loading,
    /// None after all, or none this time: the line says which.
    Failed(String),
}

/// What a button asks for.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    Nothing,
    Close,
    /// The page, the level or the part changed: paint again (and load what [`ManualView::wanted`] lists).
    Moved,
}

pub struct ManualView {
    pub title: String,
    pages: Vec<Kept>,
    pub wait: Option<Wait>,
    page: usize,
    level: usize,
    /// The part's place, per mille of the way it can move (500 the middle), as in `zoom`.
    x: i32,
    y: i32,
    /// Decoded pages: (index, picture).
    decoded: Vec<(usize, Arc<Image>)>,
    /// While it loads: the bytes that came so far (the size is not known ahead).
    pub loaded: u64,
    /// Pages whose picture did not decode: shown as "can't be shown", never asked for again.
    broken: Vec<usize>,
    /// This finger's swipe has turned its page already: one page per swipe, however far it goes.
    swiped: bool,
}

impl ManualView {
    /// Opened while the manual is fetched.
    pub fn loading(title: &str) -> ManualView {
        ManualView { title: title.to_string(), pages: Vec::new(), wait: Some(Wait::Loading), page: 0, level: 0, x: 500, y: 0, decoded: Vec::new(), loaded: 0, broken: Vec::new(), swiped: false }
    }

    /// Opened on a manual on the card.
    pub fn new(title: &str, pages: Vec<Kept>) -> ManualView {
        let mut v = ManualView::loading(title);
        v.set_pages(pages);
        v
    }

    pub fn set_pages(&mut self, pages: Vec<Kept>) {
        self.wait = if pages.is_empty() { Some(Wait::Failed("This manual has no pages".to_string())) } else { None };
        self.pages = pages;
        self.page = 0;
        self.decoded.clear();
    }

    pub fn failed(&mut self, why: &str) {
        self.wait = Some(Wait::Failed(why.to_string()));
    }

    pub fn page(&self) -> usize {
        self.page
    }

    pub fn count(&self) -> usize {
        self.pages.len()
    }

    pub fn level(&self) -> u32 {
        LEVELS[self.level]
    }

    /// The pages to decode, most wanted first, that are not decoded yet: (index, file).
    pub fn wanted(&self) -> Vec<(usize, std::path::PathBuf)> {
        let n = self.pages.len();
        // Nothing yet (loading, none, offline): nothing to decode.
        if n == 0 {
            return Vec::new();
        }
        let mut order = vec![self.page];
        if self.page + 1 < n {
            order.push(self.page + 1);
        }
        if self.page > 0 {
            order.push(self.page - 1);
        }
        order
            .into_iter()
            .filter(|i| !self.decoded.iter().any(|(d, _)| d == i) && !self.broken.contains(i))
            .filter_map(|i| match &self.pages[i] {
                Kept::Jpeg(p, ..) => Some((i, p.clone())),
                Kept::Other => None,
            })
            .collect()
    }

    /// A page decoded on the loading thread; the pages farthest from this one are let go.
    pub fn decoded(&mut self, index: usize, img: Arc<Image>) {
        self.decoded.retain(|(i, _)| *i != index);
        self.decoded.push((index, img));
        let page = self.page;
        self.decoded.sort_by_key(|(i, _)| i.abs_diff(page));
        self.decoded.truncate(KEEP);
    }

    /// A page's picture did not decode (missing, broken, too large).
    pub fn broken_page(&mut self, index: usize) {
        if !self.broken.contains(&index) {
            self.broken.push(index);
        }
    }

    fn shown(&self) -> Option<&Arc<Image>> {
        self.decoded.iter().find(|(i, _)| *i == self.page).map(|(_, img)| img)
    }

    fn turn(&mut self, forward: bool) -> Action {
        let to = if forward { (self.page + 1).min(self.pages.len().saturating_sub(1)) } else { self.page.saturating_sub(1) };
        if to == self.page {
            return Action::Nothing;
        }
        self.page = to;
        // A new page starts whole, or at its top when zoomed in (reading on from the top).
        self.x = 500;
        self.y = 0;
        Action::Moved
    }

    pub fn press(&mut self, b: Button, zoom: Button, back: Button) -> Action {
        // B in the zoom leaves the zoom first, as on the game box's real back; B on the whole page closes the manual.
        if b == back && self.level > 0 {
            self.level = 0;
            return Action::Moved;
        }
        if b == back {
            return Action::Close;
        }
        if self.pages.is_empty() {
            return Action::Nothing;
        }
        let zoomed = self.level > 0;
        // A press moves the part a quarter of its size, as on the pause menu's Zoom page.
        let step = if zoomed { 1000 / (4 * (self.level() as i32 - 1)) } else { 0 };
        let before = (self.page, self.level, self.x, self.y);
        match b {
            Button::L1 => return self.turn(false),
            Button::R1 => return self.turn(true),
            Button::Left if !zoomed => return self.turn(false),
            Button::Right if !zoomed => return self.turn(true),
            Button::Left => self.x = (self.x - step).max(0),
            Button::Right => self.x = (self.x + step).min(1000),
            Button::Up if zoomed => self.y = (self.y - step).max(0),
            Button::Down if zoomed => self.y = (self.y + step).min(1000),
            b if b == zoom && matches!(self.pages.get(self.page), Some(Kept::Jpeg(..))) => self.level = (self.level + 1) % LEVELS.len(),
            _ => return Action::Nothing,
        }
        if before == (self.page, self.level, self.x, self.y) { Action::Nothing } else { Action::Moved }
    }

    /// A finger on the page (Android and the other touch screens; the buttons stay as they are): a swipe to the left
    /// turns to the next page, to the right to the one before, once per swipe; a tap on the right half of the page turns
    /// on, on the left half back. `w`: the screen's width. The hints' own taps are the caller's (`app_touch`).
    pub fn touch(&mut self, gesture: Gesture, w: i32) -> Action {
        match gesture {
            Gesture::Tap { x, .. } => self.turn(x >= w as f32 / 2.0),
            Gesture::Scroll { steps, vertical: false } if steps != 0 && !self.swiped => {
                self.swiped = true;
                self.turn(steps < 0)
            }
            _ => Action::Nothing,
        }
    }

    /// A finger came down: its swipe may turn a page.
    pub fn finger_down(&mut self) {
        self.swiped = false;
    }

    /// How far a finger goes sideways before its swipe turns the page (`touch::Recognizer`'s pitch): a sixth of the screen.
    pub fn swipe_pitch(w: u32) -> (f32, f32) {
        (w as f32 / 6.0, 0.0)
    }

    /// The page's part in view, in the picture's pixels, and where it goes on the screen (`w` x `h`, `room_h` above
    /// the hints): ((x, y, w, h) of the picture, (x, y, w, h) on screen).
    pub fn layout(&self, img_w: u32, img_h: u32, w: i32, room_h: i32) -> ((u32, u32, u32, u32), (i32, i32, i32, i32)) {
        let (iw, ih) = (img_w.max(1) as f32, img_h.max(1) as f32);
        let fit = (w as f32 / iw).min(room_h as f32 / ih);
        let scale = fit * self.level() as f32;
        // The part of the picture the screen takes at this scale, never more than the picture.
        let (pw, ph) = (((w as f32 / scale).min(iw)) as u32, ((room_h as f32 / scale).min(ih)) as u32);
        let (pw, ph) = (pw.max(1), ph.max(1));
        let sx = ((img_w - pw) as u64 * self.x as u64 / 1000) as u32;
        let sy = ((img_h - ph) as u64 * self.y as u64 / 1000) as u32;
        let (dw, dh) = ((pw as f32 * scale).round() as i32, (ph as f32 * scale).round() as i32);
        ((sx, sy, pw, ph), ((w - dw) / 2, (room_h - dh) / 2, dw, dh))
    }

    pub fn hints(&self) -> Vec<(&'static str, &'static str)> {
        let mut h = Vec::new();
        if self.pages.len() > 1 {
            h.push(("L1/R1", "Page"));
        }
        if self.level > 0 {
            h.push(("D-pad", "Move"));
        }
        if matches!(self.pages.get(self.page), Some(Kept::Jpeg(..))) {
            h.push(("A", if self.level + 1 == LEVELS.len() { "Whole page" } else { "Zoom" }));
        }
        h.push(("B", "Back"));
        h
    }

    pub fn paint(&self, c: &mut Canvas, text: &mut Text, hits: &mut HitMap) {
        // Always dark, in either appearance: the scans are the light here (as a photo viewer).
        c.clear(0x101010);
        let room_h = kit::hint_bar_top(c.h);
        let (light, soft) = (0xf0f0f0, 0xa8a8a8);
        let centre = |c: &mut Canvas, text: &mut Text, size: u32, y: i32, color: u32, s: &str| {
            let w = text.width(Face::Body, size, s);
            text.draw(c, Face::Body, size, (c.w - w) / 2, y, color, s);
        };
        match (&self.wait, self.pages.get(self.page)) {
            (Some(wait), _) => {
                let line = match wait {
                    Wait::Loading => "Getting the manual from ScreenScraper…".to_string(),
                    Wait::Failed(why) => why.clone(),
                };
                centre(c, text, kit::ROW_FONT, room_h / 2 - 20, light, &self.title);
                centre(c, text, SMALL, room_h / 2 + 30, soft, &line);
                // Loading: how much came so far, and a bar whose bright part moves on with every piece that comes, so a
                // slow Wi-Fi never looks frozen (Neon, 26.11.2026). The size isn't known ahead: no percent.
                if matches!(wait, Wait::Loading) {
                    let (bw, bh) = (c.w / 3, 8);
                    let (bx, by) = ((c.w - bw) / 2, room_h / 2 + 70);
                    c.rect(bx, by, bw, bh, 0x303030);
                    let seg = bw / 4;
                    let at = ((self.loaded / 32_768) as i32 * 13) % (bw - seg).max(1);
                    c.rect(bx + at, by, seg, bh, soft);
                    if self.loaded > 0 {
                        centre(c, text, SMALL, by + 60, soft, &megabytes(self.loaded));
                    }
                }
            }
            (None, Some(Kept::Jpeg(..))) if !self.broken.contains(&self.page) => match self.shown() {
                Some(img) => {
                    let (part, (x, y, w, h)) = self.layout(img.w, img.h, c.w, room_h);
                    c.blit_region(img, part, x, y, w, h);
                }
                None => centre(c, text, SMALL, room_h / 2, soft, &format!("Page {}", self.page + 1)),
            },
            (None, _) => centre(c, text, SMALL, room_h / 2, soft, "This page can't be shown here"),
        }
        let hints_x = kit::pill_hints(c, text, hits, &self.hints());
        // Where you are: "3 / 32", and the zoom level while zoomed, on the hint bar's line at the left: never over the
        // page (the lead, 26.11.2026: a pill at the top left hid the start of a heading). Left out where the hints
        // take the whole line.
        if !self.pages.is_empty() {
            let mut label = format!("{} / {}", self.page + 1, self.pages.len());
            if self.level > 0 {
                label.push_str(&format!(" · {}x", self.level()));
            }
            let w = text.width(Face::Body, SMALL, &label);
            if PAD + w + 24 < hints_x {
                text.draw(c, Face::Body, SMALL, PAD, kit::hint_bar_mid(c.h) + type_scale::mid(SMALL), soft, &label);
            }
        }
    }
}

/// "1.4 MB", "0.3 MB": what came so far.
pub fn megabytes(bytes: u64) -> String {
    format!("{}.{} MB", bytes / 1_000_000, bytes % 1_000_000 / 100_000)
}

/// Decodes a kept page for the view, at most [`MAX_EDGE`] on its longest side (the JPEG's own 1/2, 1/4 steps).
/// Runs on the loading thread.
pub fn decode(path: &std::path::Path) -> Option<Image> {
    let bytes = crate::background_io::read(path).ok()?;
    let mut d = jpeg_decoder::Decoder::new(bytes.as_slice());
    d.read_info().ok()?;
    let info = d.info()?;
    // The header's size before anything is decoded (Sol 26.11.2026: jpeg-decoder sizes a progressive picture's
    // buffers from it before its own check).
    if !crate::image::size_ok(info.width as u32, info.height as u32) {
        return None;
    }
    let long = info.width.max(info.height) as u32;
    if long > MAX_EDGE {
        let f = long.div_ceil(MAX_EDGE);
        let _ = d.scale((info.width as u32 / f) as u16, (info.height as u32 / f) as u16);
    }
    let data = d.decode().ok()?;
    let info = d.info()?;
    if !crate::image::size_ok(info.width as u32, info.height as u32) {
        return None;
    }
    let px: Vec<u32> = match info.pixel_format {
        jpeg_decoder::PixelFormat::RGB24 => data.chunks_exact(3).map(|p| (p[0] as u32) << 16 | (p[1] as u32) << 8 | p[2] as u32).collect(),
        jpeg_decoder::PixelFormat::L8 => data.iter().map(|g| (*g as u32) * 0x010101).collect(),
        jpeg_decoder::PixelFormat::CMYK32 => data.chunks_exact(4).map(|p| {
            // Adobe's CMYK JPEGs are stored inverted.
            let (c, m, y, k) = (p[0] as u32, p[1] as u32, p[2] as u32, p[3] as u32);
            (c * k / 255) << 16 | (m * k / 255) << 8 | (y * k / 255)
        }).collect(),
        _ => return None,
    };
    Some(Image { w: info.width as u32, h: info.height as u32, px })
}

#[cfg(test)]
#[path = "manual_view_tests.rs"]
mod tests;
