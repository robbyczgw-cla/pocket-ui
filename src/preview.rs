//! Live picture preview for the pause menu. pocket-ui sends `preview <w> <h> [<seq>]` to pocket-core;
//! pocket-core renders its last frame with the current (unsaved) Picture settings offscreen at the
//! screen's real size and writes two images to [`FILE`]:
//!
//! 1. an exact `w`×`h` crop from the centre of the screen at 1:1, so scanlines, the LCD grid and the
//!    CRT mask look exactly as in game;
//! 2. the whole screen, box-filtered down to height `h`, to show scaling and the frame.
//!
//! File layout (little endian): `b"PCPV0001"`, `seq: u32`, `count: u32`, then per image `w: u32`,
//! `h: u32` and `w*h` pixels as 0RGB `u32` (the canvas format of pocket-ui, so no conversion there).
//! pocket-core writes a temporary file and renames it, so a reader never sees half an image.

use std::path::Path;
use std::time::{Duration, Instant};

use crate::gfx::Canvas;
use crate::image::Image;
use crate::text::{Face, Text};

pub const FILE: &str = "/tmp/pocket-core-preview.bin";
const MAGIC: &[u8; 8] = b"PCPV0001";

/// One image of a preview, 0RGB pixels, rows top to bottom.
#[derive(Clone, PartialEq, Debug)]
pub struct Pixels {
    pub w: u32,
    pub h: u32,
    pub px: Vec<u32>,
}

pub fn encode(seq: u32, images: &[&Pixels]) -> Vec<u8> {
    let size = 16 + images.iter().map(|i| 8 + i.px.len() * 4).sum::<usize>();
    let mut out = Vec::with_capacity(size);
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&seq.to_le_bytes());
    out.extend_from_slice(&(images.len() as u32).to_le_bytes());
    for img in images {
        out.extend_from_slice(&img.w.to_le_bytes());
        out.extend_from_slice(&img.h.to_le_bytes());
        for p in &img.px {
            out.extend_from_slice(&p.to_le_bytes());
        }
    }
    out
}

/// Sequence number of an encoded preview, from its first 12 bytes.
pub fn peek_seq(data: &[u8]) -> Option<u32> {
    (data.len() >= 12 && &data[..8] == MAGIC).then(|| u32::from_le_bytes(data[8..12].try_into().unwrap()))
}

pub fn decode(data: &[u8]) -> Option<(u32, Vec<Pixels>)> {
    let seq = peek_seq(data)?;
    let word = |at: usize| data.get(at..at + 4).map(|b| u32::from_le_bytes(b.try_into().unwrap()));
    let count = word(12)?;
    let mut at = 16;
    let mut images = Vec::new();
    for _ in 0..count.min(8) {
        let (w, h) = (word(at)?, word(at + 4)?);
        at += 8;
        let n = (w as usize).checked_mul(h as usize)?;
        let body = data.get(at..at + n * 4)?;
        images.push(Pixels { w, h, px: body.chunks_exact(4).map(|b| u32::from_le_bytes(b.try_into().unwrap())).collect() });
        at += n * 4;
    }
    Some((seq, images))
}

/// Writes atomically (a new temporary file, then rename; `fsutil::replace_file`).
pub fn write(path: &Path, seq: u32, images: &[&Pixels]) -> std::io::Result<()> {
    crate::fsutil::replace_file(path, &encode(seq, images))
}

/// Builds both preview images from a full-screen RGBA readback (`rows` top to bottom). The crop is
/// clamped to the screen; the overview keeps the screen's shape at height `h`.
pub fn from_screen(rgba: &[u8], sw: u32, sh: u32, w: u32, h: u32) -> (Pixels, Pixels) {
    let at = |x: u32, y: u32| {
        let i = ((y * sw + x) * 4) as usize;
        (rgba[i] as u32) << 16 | (rgba[i + 1] as u32) << 8 | rgba[i + 2] as u32
    };
    let (cw, ch) = (w.clamp(1, sw), h.clamp(1, sh));
    let (x0, y0) = ((sw - cw) / 2, (sh - ch) / 2);
    let crop = Pixels { w: cw, h: ch, px: (0..ch).flat_map(|y| (0..cw).map(move |x| (x, y))).map(|(x, y)| at(x0 + x, y0 + y)).collect() };

    // Box filter: every overview pixel averages the screen pixels it covers.
    let th = h.clamp(1, sh);
    let tw = ((sw as u64 * th as u64 + sh as u64 / 2) / sh as u64).max(1) as u32;
    let mut px = Vec::with_capacity((tw * th) as usize);
    for ty in 0..th {
        let (ya, yb) = (ty * sh / th, ((ty + 1) * sh / th).max(ty * sh / th + 1).min(sh));
        for tx in 0..tw {
            let (xa, xb) = (tx * sw / tw, ((tx + 1) * sw / tw).max(tx * sw / tw + 1).min(sw));
            let (mut r, mut g, mut b) = (0u32, 0u32, 0u32);
            for y in ya..yb {
                let row = ((y * sw) * 4) as usize;
                for x in xa..xb {
                    let i = row + x as usize * 4;
                    r += rgba[i] as u32;
                    g += rgba[i + 1] as u32;
                    b += rgba[i + 2] as u32;
                }
            }
            let n = ((yb - ya) * (xb - xa)).max(1);
            px.push((r / n) << 16 | (g / n) << 8 | b / n);
        }
    }
    (crop, Pixels { w: tw, h: th, px })
}

/// The pause menu's background picture, `shot <seq>` (`App::write_shot` in pocket-core): the last frame as the screen
/// shows it, at the screen's full size, as one image in the format above. With it pocket-ui raises the menu at once
/// instead of first taking a screenshot of the game with grim and waiting for it (docs/ARCHITECTURE.md, the pause
/// menu); the picture comes in behind the menu a moment later.
pub const SHOT_FILE: &str = "/tmp/pocket-core-shot.bin";

/// pocket-core: the whole screen from an RGBA readback (rows top to bottom) as one image.
pub fn shot_from_screen(rgba: &[u8], w: u32, h: u32) -> Pixels {
    let px = rgba.chunks_exact(4).take((w * h) as usize).map(|p| (p[0] as u32) << 16 | (p[1] as u32) << 8 | p[2] as u32).collect();
    Pixels { w, h, px }
}

/// pocket-ui: a number for the next `shot` that no file left by an earlier menu (or an earlier pocket-ui) carries.
pub fn next_shot_seq() -> u32 {
    static SEQ: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let base = (crate::trace::now_us() / 1000) as u32 & 0x7fff_0000;
    base | (SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed) & 0xffff)
}

/// pocket-ui, on a helper thread: waits up to `timeout` for shot `seq` in `file` (the header is polled every
/// 2 ms, as for the live preview) and reads it. None when it doesn't come.
pub fn wait_for_shot(file: &Path, seq: u32, timeout: Duration) -> Option<Pixels> {
    use std::io::Read;
    let deadline = Instant::now() + timeout;
    loop {
        let mut head = [0u8; 12];
        if std::fs::File::open(file).and_then(|mut f| f.read_exact(&mut head)).is_ok() && peek_seq(&head) == Some(seq) {
            let (s, mut images) = decode(&std::fs::read(file).ok()?)?;
            return images.pop().filter(|_| s == seq);
        }
        if Instant::now() > deadline {
            return None;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// pocket-ui's side: the preview shown next to the Picture settings, and the request in flight.
#[derive(Default)]
pub struct Live {
    /// Number of the newest request; older answers are dropped.
    pub seq: u32,
    sent: Option<Instant>,
    /// (whole screen, actual size) of the newest answer.
    images: Option<(Image, Image)>,
    failed: bool,
    /// The newest answer arrived but hasn't been on screen yet: (seq, sent, arrived).
    unshown: Option<(u32, Instant, Instant)>,
}

/// Where the two images go: (x, y, w, h) of the whole-screen overview and of the 1:1 detail.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Layout {
    pub whole: (i32, i32, i32, i32),
    pub detail: (i32, i32, i32, i32),
}

const CAPTION: u32 = 30;
const CAPTION_H: i32 = 44;
const INSET: i32 = 8;
const GAP: i32 = 24;

/// The smallest share of the screen's height the images get with larger text (Medium or Large) (`rows_above`): 30 %, 324 px on the
/// Mini V2, where scanlines and the LCD grid still read at arm's length.
pub const LARGE_MIN_SHARE: (i32, i32) = (3, 10);

/// How many of the page's `max_rows` rows the list shows above the images with Large text: as many as leave the
/// images (between `top_for(n)`, the band's top under `n` rows, and `bottom`) at least `min_h` tall, and never fewer
/// than three (the selected row and its neighbours; the list scrolls). Normal text shows them all.
pub fn rows_above(max_rows: usize, top_for: impl Fn(usize) -> i32, bottom: i32, min_h: i32) -> usize {
    let floor = max_rows.min(3);
    (floor..=max_rows).rev().find(|&n| image_height(top_for(n), bottom) >= min_h).unwrap_or(floor)
}

/// The pause menu's Picture page on a `screen` of (w, h): how many of its `max_rows` rows the list shows, from
/// `list_top` in rows of `type_scale::row(66, 36)`, and where the images go, between the rows and the explanation at
/// the bottom (the longest of the group, `desc_lines` lines of `desc_font` px). Normal text shows every row, so the
/// images stay put when a sub-choice appears; Medium and Large show as many as leave the images `LARGE_MIN_SHARE` of the screen
/// (`rows_above`) and keeps at most the five lines the explanation shows free for it.
pub fn picture_page(screen: (i32, i32), pad: i32, list_top: i32, max_rows: usize, desc_lines: usize, desc_font: u32) -> (usize, Layout) {
    let (w, h) = screen;
    let large = crate::type_scale::scaled();
    let lines = if large { desc_lines.min(5) } else { desc_lines };
    let footer_top = h - pad - lines as i32 * crate::type_scale::line(desc_font, 10) + desc_font as i32 / 4;
    let bottom = footer_top - 20;
    let row = crate::type_scale::row(66, 36);
    let top_for = |n: usize| list_top + n as i32 * row + 18;
    let rows = if large { rows_above(max_rows, top_for, bottom, h * LARGE_MIN_SHARE.0 / LARGE_MIN_SHARE.1) } else { max_rows };
    (rows, layout(w, pad, top_for(rows), bottom, (w as u32, h as u32)))
}

/// Height of the images in the band from `top` to `bottom` (`layout`).
pub fn image_height(top: i32, bottom: i32) -> i32 {
    (bottom - top - CAPTION_H - 2 * INSET).max(16)
}

/// Splits the band from `top` to `bottom` (between the settings and their explanation) into the
/// overview, in the screen's shape, and the detail, which takes the rest of the width.
pub fn layout(canvas_w: i32, pad: i32, top: i32, bottom: i32, screen: (u32, u32)) -> Layout {
    let ih = image_height(top, bottom);
    let ww = (ih as i64 * screen.0 as i64 / screen.1.max(1) as i64) as i32;
    let y = top + CAPTION_H + INSET;
    let whole = (pad + INSET, y, ww, ih);
    let dx = pad + ww + 2 * INSET + GAP + INSET;
    Layout { whole, detail: (dx, y, (canvas_w - pad - INSET - dx).max(16), ih) }
}

impl Live {
    /// Asks pocket-core for a fresh preview at the detail's size. `deliver` is called from a
    /// helper thread with the answer (None after a timeout).
    pub fn request(&mut self, send: impl FnOnce(&str) -> bool, layout: &Layout, deliver: impl FnOnce(u32, Option<Vec<Pixels>>) + Send + 'static) {
        if self.seq == 0 {
            // A file from an earlier menu could carry the same number.
            let _ = std::fs::remove_file(FILE);
        }
        self.seq += 1;
        let seq = self.seq;
        let (_, _, w, h) = layout.detail;
        if !send(&format!("preview {w} {h} {seq}")) {
            self.failed = true;
            return;
        }
        self.sent = Some(Instant::now());
        std::thread::spawn(move || {
            use std::io::Read;
            let deadline = Instant::now() + Duration::from_secs(2);
            loop {
                // Only the header first: cheap enough to poll every 2 ms.
                let mut head = [0u8; 12];
                let ready = std::fs::File::open(FILE).and_then(|mut f| f.read_exact(&mut head)).is_ok() && peek_seq(&head) == Some(seq);
                if ready {
                    if let Some((s, images)) = std::fs::read(FILE).ok().and_then(|d| decode(&d)) {
                        if s == seq {
                            return deliver(seq, Some(images));
                        }
                    }
                }
                if Instant::now() > deadline {
                    return deliver(seq, None);
                }
                std::thread::sleep(Duration::from_millis(2));
            }
        });
    }

    /// An answer from the helper thread; true if it is the newest one and changes the screen.
    pub fn receive(&mut self, seq: u32, images: Option<Vec<Pixels>>) -> bool {
        if seq != self.seq {
            return false;
        }
        let to_image = |p: Pixels| Image { w: p.w, h: p.h, px: p.px };
        match images.map(|v| v.into_iter().map(to_image).collect::<Vec<_>>()) {
            Some(mut v) if v.len() >= 2 => {
                let whole = v.pop().unwrap();
                let detail = v.pop().unwrap();
                self.images = Some((whole, detail));
                self.failed = false;
                if let Some(sent) = self.sent {
                    self.unshown = Some((seq, sent, Instant::now()));
                }
            }
            _ => self.failed = self.images.is_none(),
        }
        true
    }

    /// Sets the images directly (desktop snapshots).
    pub fn set(&mut self, whole: Image, detail: Image) {
        self.images = Some((whole, detail));
    }

    /// After the screen was presented: the timing of a preview that just became visible, as
    /// (seq, ms from request to file read, ms from request to on screen), once.
    pub fn shown(&mut self) -> Option<(u32, f32, f32)> {
        let (seq, sent, arrived) = self.unshown.take()?;
        let ms = |d: Duration| d.as_secs_f32() * 1000.0;
        Some((seq, ms(arrived - sent), ms(sent.elapsed())))
    }

    /// Captions, two dark panels and the images in them.
    pub fn paint(&self, c: &mut Canvas, text: &mut Text, l: &Layout, caption_color: u32) {
        for ((x, y, w, h), caption) in [(l.whole, "Whole screen"), (l.detail, "Actual size")] {
            // As wide as its panel at most (a narrow whole screen, larger text): cut with "…" past that.
            let caption = text.fit(Face::Body, CAPTION, caption, w + 2 * INSET - 6);
            text.draw(c, Face::Body, CAPTION, x - INSET + 6, y - INSET - 14, caption_color, &caption);
            c.round_rect(x - INSET, y - INSET, w + 2 * INSET, h + 2 * INSET, 18, 0x1a1a1a);
            c.rect(x, y, w, h, 0x000000);
        }
        match &self.images {
            Some((whole, detail)) => {
                for (img, (x, y, w, h)) in [(whole, l.whole), (detail, l.detail)] {
                    // The detail must stay 1:1: centre it if it came back smaller, crop if larger.
                    let (bw, bh) = ((img.w as i32).min(w), (img.h as i32).min(h));
                    let (ox, oy) = (x + (w - bw) / 2, y + (h - bh) / 2);
                    let (sx, sy) = ((img.w as i32 - bw) / 2, (img.h as i32 - bh) / 2);
                    for row in 0..bh {
                        let src = ((sy + row) as u32 * img.w + sx as u32) as usize;
                        let dst = ((oy + row) * c.w + ox) as usize;
                        if oy + row >= 0 && oy + row < c.h && ox >= 0 && ox + bw <= c.w {
                            c.buf[dst..dst + bw as usize].copy_from_slice(&img.px[src..src + bw as usize]);
                        }
                    }
                }
            }
            None => {
                let msg = if self.failed { "No preview" } else { "" };
                let (x, y, w, h) = l.detail;
                let tw = text.width(Face::Body, CAPTION, msg);
                text.draw(c, Face::Body, CAPTION, x + (w - tw) / 2, y + h / 2 + 10, caption_color, msg);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn large_text_keeps_the_images_at_least_their_share_with_three_rows_or_more() {
        use crate::type_scale::{set, TextSize};
        // The pause menu's Picture page as main.rs lays it out (pad 30, the list from y 132, explanations of 30 px), on
        // the Mini V2, with every number of rows a group can have and explanations up to eight lines long.
        let min = 1080 * LARGE_MIN_SHARE.0 / LARGE_MIN_SHARE.1;
        set(TextSize::Large);
        for max in 1..=9 {
            for lines in 1..=8 {
                let (n, l) = picture_page((1240, 1080), 30, 132, max, lines, 30);
                assert!(n <= max && n >= max.min(3), "{max} rows, {lines} lines: {n}");
                assert!(l.whole.3 >= min && l.detail.3 == l.whole.3, "{max} rows, {lines} lines: {n} rows leave {:?}", l);
                if n < max {
                    let more = |k: usize| image_height(132 + k as i32 * 76 + 18, 1080 - 30 - lines.min(5) as i32 * 48 + 7 - 20);
                    assert!(more(n + 1) < min, "as many rows as fit: {max} {lines} {n}");
                }
            }
        }
        // The GBA's seven rows: three shown, the images 359 px high instead of a strip.
        assert_eq!(picture_page((1240, 1080), 30, 132, 7, 6, 30).0, 3);
        set(TextSize::Normal);
        // Normal: every row, whatever the images keep (the layout of before).
        let (n, l) = picture_page((1240, 1080), 30, 132, 7, 4, 30);
        assert_eq!((n, l.whole.1), (7, 132 + 7 * 66 + 18 + 44 + 8));
        // A band too small for even three rows keeps three.
        assert_eq!(rows_above(7, |n| 900 + n as i32 * 76, 1000, 324), 3);
    }

    #[test]
    fn the_menu_shot_is_the_whole_screen_and_only_the_one_asked_for_is_taken() {
        let (w, h) = (4u32, 3u32);
        let rgba: Vec<u8> = (0..w * h).flat_map(|i| [i as u8, 0x10, 0x20, 0xff]).collect();
        let shot = shot_from_screen(&rgba, w, h);
        assert_eq!((shot.w, shot.h, shot.px[5]), (4, 3, 0x05_10_20));
        let dir = crate::testutil::TestDir::new("shot");
        let file = dir.join("shot.bin");
        let (a, b) = (next_shot_seq(), next_shot_seq());
        assert_ne!(a, b);
        write(&file, a, &[&shot]).unwrap();
        let t0 = Instant::now();
        assert!(wait_for_shot(&file, b, Duration::from_millis(30)).is_none(), "an older shot is not this menu's");
        assert!(t0.elapsed() < Duration::from_millis(500));
        let got = wait_for_shot(&file, a, Duration::from_millis(30)).expect("the shot");
        assert_eq!((got.w, got.h, got.px), (shot.w, shot.h, shot.px));
        let _ = std::fs::remove_file(&file);
    }

    #[test]
    fn layout_fills_the_band_and_keeps_the_screen_shape() {
        let l = layout(1240, 30, 416, 877, (1240, 1080));
        let (wx, _, ww, wh) = l.whole;
        let (dx, dy, dw, dh) = l.detail;
        assert_eq!(wh, dh);
        assert!((ww as f32 / wh as f32 - 1240.0 / 1080.0).abs() < 0.01);
        assert_eq!(wx, 38);
        assert_eq!(dx + dw + INSET, 1240 - 30, "detail ends at the right margin");
        assert!(dy + dh + INSET <= 877);
    }

    #[test]
    fn only_the_newest_answer_counts() {
        let mut live = Live { seq: 3, sent: Some(Instant::now()), ..Default::default() };
        let px = |w: u32, h: u32| Pixels { w, h, px: vec![0; (w * h) as usize] };
        assert!(!live.receive(2, Some(vec![px(2, 2), px(3, 2)])));
        assert!(live.images.is_none());
        assert!(live.receive(3, Some(vec![px(2, 2), px(3, 2)])));
        let (whole, detail) = live.images.as_ref().unwrap();
        assert_eq!((whole.w, detail.w), (3, 2));
        assert!(live.shown().is_some() && live.shown().is_none(), "timing is reported once");
        // A timeout keeps the last good preview.
        live.seq = 4;
        assert!(live.receive(4, None));
        assert!(live.images.is_some() && !live.failed);
    }

    #[test]
    fn round_trip_and_truncated_files() {
        let a = Pixels { w: 2, h: 1, px: vec![0x112233, 0xffffff] };
        let b = Pixels { w: 1, h: 2, px: vec![0, 0x0a0b0c] };
        let data = encode(7, &[&a, &b]);
        assert_eq!(peek_seq(&data), Some(7));
        assert_eq!(decode(&data), Some((7, vec![a, b])));
        assert_eq!(decode(&data[..data.len() - 1]), None, "a short file is rejected, not misread");
        assert_eq!(decode(b"nonsense-and-more"), None);
    }

    #[test]
    fn crop_is_centred_and_overview_keeps_the_shape() {
        // 8x4 screen whose red channel is x and green channel is y.
        let (sw, sh) = (8u32, 4u32);
        let rgba: Vec<u8> = (0..sh).flat_map(|y| (0..sw).flat_map(move |x| [x as u8, y as u8, 0, 255])).collect();
        let (crop, over) = from_screen(&rgba, sw, sh, 4, 2);
        assert_eq!((crop.w, crop.h), (4, 2));
        assert_eq!(crop.px[0], 2 << 16 | 1 << 8, "starts at (2, 1)");
        assert_eq!((over.w, over.h), (4, 2));
        // Each overview pixel averages a 2x2 block: (0+1)/2 = 0, (0+1)/2 = 0.
        assert_eq!(over.px[0], 0);
        assert_eq!(over.px[3], 6 << 16, "x 6..8 averages to 6 (integer)");
        // A crop larger than the screen is clamped.
        let (crop, _) = from_screen(&rgba, sw, sh, 100, 100);
        assert_eq!((crop.w, crop.h), (8, 4));
    }
}
