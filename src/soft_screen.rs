//! pocket-core without a GPU: the core's picture, copied by the CPU into a wl_shm window (`pocket-fb` shows it on the
//! framebuffer). The Brick's first test and the fallback if its GL blob won't share the screen (docs/TRIMUI-BRICK.md).
//! No shaders, screen frames or overlay: square pixels, whole-number scale. Only behind the `brick` feature.
//!
//! The Soft colour look (temperature and the night light, `display.rs`) is folded into the copy: where the GL path
//! draws a multiply layer over the game (`overlay.rs`), this path passes each converted pixel through the look's
//! three 256-entry tables. A neutral look keeps the tables off, so an untinted game pays nothing for them.

use std::num::NonZeroU32;
use std::rc::Rc;

use winit::event_loop::ActiveEventLoop;
use winit::window::{Fullscreen, Window};

use crate::display::Look;
use crate::libretro::{Frame, PIXEL_RGB565, PIXEL_XRGB8888};

pub struct SoftScreen {
    window: Rc<Window>,
    surface: softbuffer::Surface<Rc<Window>, Rc<Window>>,
    size: (u32, u32),
    /// The picture goes out at the core's own size, for the bridge's shaders (`brick_picture`), not scaled here.
    native: bool,
    scratch: Vec<u8>,
    /// The look the tables below were made from (rebuilt only when it changes; a fade step is a change).
    look: Look,
    /// The look's tables while it tints (`Look::tables`); None for a neutral or compositor-applied look.
    tables: Option<[[u8; 256]; 3]>,
}

impl SoftScreen {
    pub fn new(el: &ActiveEventLoop) -> Result<SoftScreen, String> {
        let attrs = Window::default_attributes().with_title("pocket-core").with_fullscreen(Some(Fullscreen::Borderless(None)));
        let window = Rc::new(el.create_window(attrs).map_err(|e| format!("window: {e}"))?);
        let context = softbuffer::Context::new(window.clone()).map_err(|e| format!("context: {e}"))?;
        let surface = softbuffer::Surface::new(&context, window.clone()).map_err(|e| format!("surface: {e}"))?;
        Ok(SoftScreen { window, surface, size: (0, 0), native: false, scratch: Vec::new(), look: Look::default(), tables: None })
    }

    /// On: the next pictures go to the bridge at their own size (it draws them with shaders and frames). Off: scaled here.
    pub fn set_native(&mut self, on: bool) {
        self.native = on;
    }

    /// The colour look the copy goes through (the HUD's, which pocket-ui sends and the night light fades): the
    /// tables are rebuilt when it changes, and a look that does not tint in software turns them off.
    pub fn set_look(&mut self, look: &Look) {
        if *look == self.look && (self.tables.is_some() == tinted(look)) {
            return;
        }
        self.look = *look;
        self.tables = tinted(look).then(|| look.tables());
    }

    /// Draws the picture and hands it over. The window's size, for the previews.
    pub fn show(&mut self, frame: &Frame) -> (u32, u32) {
        if self.native {
            return self.show_native(frame);
        }
        let size = self.window.inner_size();
        let (Some(w), Some(h)) = (NonZeroU32::new(size.width), NonZeroU32::new(size.height)) else { return self.size };
        if self.size != (size.width, size.height) {
            if self.surface.resize(w, h).is_err() {
                return self.size;
            }
            self.size = (size.width, size.height);
        }
        if let Ok(mut buffer) = self.surface.buffer_mut() {
            match &self.tables {
                Some(t) => blit_tinted(frame, &mut buffer, size.width as usize, size.height as usize, t),
                None => blit(frame, &mut buffer, size.width as usize, size.height as usize),
            }
            let _ = buffer.present();
        }
        self.size
    }
}

/// Whether this look tints in software: the look says so (`soft`: no compositor does it) and it changes a colour.
pub fn tinted(look: &Look) -> bool {
    look.soft && (look.warmth != 0 || look.night != 0)
}

/// `blit` through the look's tables (`Look::tables`); the black around the picture stays black (a multiply keeps 0).
pub fn blit_tinted(frame: &Frame, dst: &mut [u32], dw: usize, dh: usize, t: &[[u8; 256]; 3]) {
    blit_map(frame, dst, dw, dh, |p| tint(p, t));
}

/// One 0RGB pixel through the look's tables.
fn tint(p: u32, t: &[[u8; 256]; 3]) -> u32 {
    (t[0][(p >> 16 & 0xff) as usize] as u32) << 16 | (t[1][(p >> 8 & 0xff) as usize] as u32) << 8 | t[2][(p & 0xff) as usize] as u32
}

impl SoftScreen {
    /// The picture as it is (XRGB8888, its own size) in a buffer of that size; the window's size is returned, as `show` does.
    fn show_native(&mut self, frame: &Frame) -> (u32, u32) {
        let window = self.window.inner_size();
        let (w, h) = crate::brick_picture::to_xrgb(frame, &mut self.scratch);
        if let (Some(nw), Some(nh)) = (NonZeroU32::new(w), NonZeroU32::new(h)) {
            if self.size != (w, h) && self.surface.resize(nw, nh).is_ok() {
                self.size = (w, h);
            }
            if self.size == (w, h) {
                if let Ok(mut buffer) = self.surface.buffer_mut() {
                    // The night light and the colour temperature go through the tables here too: the bridge's shaders draw the
                    // picture as it comes.
                    for (d, s) in buffer.iter_mut().zip(self.scratch.chunks_exact(4)) {
                        let p = u32::from_le_bytes([s[0], s[1], s[2], 0]);
                        *d = match &self.tables { Some(t) => tint(p, t), None => p };
                    }
                    let _ = buffer.present();
                }
            }
        }
        (window.width, window.height)
    }
}

/// Where the picture goes: the largest whole-number scale, centred; a picture larger than the screen is fitted instead.
/// (x, y, width, height) on the screen.
pub fn placement(src: (u32, u32), dst: (u32, u32)) -> (u32, u32, u32, u32) {
    let (sw, sh) = (src.0.max(1), src.1.max(1));
    let scale = (dst.0 / sw).min(dst.1 / sh);
    let (w, h) = if scale >= 1 {
        (sw * scale, sh * scale)
    } else if u64::from(dst.0) * u64::from(sh) < u64::from(dst.1) * u64::from(sw) {
        (dst.0, (u64::from(dst.0) * u64::from(sh) / u64::from(sw)) as u32)
    } else {
        ((u64::from(dst.1) * u64::from(sw) / u64::from(sh)) as u32, dst.1)
    };
    ((dst.0 - w) / 2, (dst.1 - h) / 2, w, h)
}

fn pixel(format: u32, row: &[u8], x: usize) -> u32 {
    let c = |v: u32, bits: u32| (v << (8 - bits)) | (v >> (2 * bits - 8));
    match format {
        PIXEL_XRGB8888 => u32::from_le_bytes([row[x * 4], row[x * 4 + 1], row[x * 4 + 2], 0]),
        PIXEL_RGB565 => {
            let v = u32::from(u16::from_le_bytes([row[x * 2], row[x * 2 + 1]]));
            (c(v >> 11, 5) << 16) | (c((v >> 5) & 0x3f, 6) << 8) | c(v & 0x1f, 5)
        }
        _ => {
            let v = u32::from(u16::from_le_bytes([row[x * 2], row[x * 2 + 1]]));
            (c((v >> 10) & 0x1f, 5) << 16) | (c((v >> 5) & 0x1f, 5) << 8) | c(v & 0x1f, 5)
        }
    }
}

/// The frame into `dst` (0RGB, `dw` by `dh`), black around it. A row that repeats is copied, not converted again.
pub fn blit(frame: &Frame, dst: &mut [u32], dw: usize, dh: usize) {
    blit_map(frame, dst, dw, dh, |p| p);
}

// Monomorphised for neutral/tinted copies: no look test in the pixel loop. Convert and tint a source pixel
// once before repeating it horizontally; repeated rows copy the finished colours. In particular a GBA
// picture at 4x needs 240*160 table lookups, rather than another pass over all 1024*768 display pixels.
fn blit_map(frame: &Frame, dst: &mut [u32], dw: usize, dh: usize, map: impl Fn(u32) -> u32) {
    let (px, py, pw, ph) = placement((frame.width, frame.height), (dw as u32, dh as u32));
    let (px, py, pw, ph) = (px as usize, py as usize, pw as usize, ph as usize);
    dst.fill(0);
    let (fw, fh) = (frame.width as usize, frame.height as usize);
    if fw == 0 || fh == 0 || frame.pitch == 0 {
        return;
    }
    let mut last = None;
    for y in 0..ph {
        let sy = y * fh / ph;
        let at = (py + y) * dw + px;
        match last {
            Some(prev) if prev == sy => {
                let from = at - dw;
                dst.copy_within(from..from + pw, at);
            }
            _ => {
                let Some(row) = frame.data.get(sy * frame.pitch..) else { return };
                if pw >= fw && pw % fw == 0 {
                    let scale = pw / fw;
                    for x in 0..fw {
                        let p = map(pixel(frame.format, row, x));
                        dst[at + x * scale..at + (x + 1) * scale].fill(p);
                    }
                } else {
                    // Fitted/downscaled pictures keep the original nearest-neighbour sampling and rounding.
                    for x in 0..pw {
                        dst[at + x] = map(pixel(frame.format, row, x * fw / pw));
                    }
                }
            }
        }
        last = Some(sy);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::libretro::PIXEL_0RGB1555;

    #[test]
    fn a_gba_picture_fills_the_brick_four_times_over() {
        assert_eq!(placement((240, 160), (1024, 768)), (32, 64, 960, 640));
    }

    #[test]
    fn game_boy_and_snes_take_the_largest_whole_scale() {
        assert_eq!(placement((160, 144), (1024, 768)), (112, 24, 800, 720));
        assert_eq!(placement((256, 224), (1024, 768)), (128, 48, 768, 672));
    }

    #[test]
    fn a_picture_larger_than_the_screen_is_fitted() {
        assert_eq!(placement((2048, 768), (1024, 768)), (0, 192, 1024, 384));
    }

    fn two_pixels(format: u32, a: u16, b: u16) -> Frame {
        let mut data = a.to_le_bytes().to_vec();
        data.extend_from_slice(&b.to_le_bytes());
        Frame { width: 2, height: 1, pitch: 4, format, data }
    }

    #[test]
    fn rgb565_reaches_full_white_and_keeps_pure_red() {
        let f = two_pixels(PIXEL_RGB565, 0xffff, 0xf800);
        let mut d = vec![0u32; 4 * 2];
        blit(&f, &mut d, 4, 2);
        // Twice the size: 4 by 2, whole screen.
        assert_eq!(d, vec![0x00ff_ffff, 0x00ff_ffff, 0x00ff_0000, 0x00ff_0000, 0x00ff_ffff, 0x00ff_ffff, 0x00ff_0000, 0x00ff_0000]);
    }

    #[test]
    fn the_older_16_bit_format_reads_right_too() {
        let f = two_pixels(PIXEL_0RGB1555, 0x7fff, 0x001f);
        let mut d = vec![0u32; 2];
        blit(&f, &mut d, 2, 1);
        assert_eq!(d, vec![0x00ff_ffff, 0x0000_00ff]);
    }

    #[test]
    fn the_night_light_tints_the_copy_and_a_neutral_look_does_not() {
        let neutral = Look { soft: true, ..Look::default() };
        assert!(!tinted(&neutral));
        assert!(!tinted(&Look { night: 3800, ..Look::default() }), "a compositor look is not ours to apply");
        let night = Look { soft: true, night: 3800, ..Look::default() };
        assert!(tinted(&night) && tinted(&Look { soft: true, warmth: 4, ..Look::default() }));
        let f = Frame { width: 1, height: 1, pitch: 4, format: PIXEL_XRGB8888, data: vec![255, 255, 255, 0] };
        let (mut plain, mut warm) = (vec![0u32; 3], vec![0u32; 3]);
        blit(&f, &mut plain, 3, 1);
        blit_tinted(&f, &mut warm, 3, 1, &night.tables());
        assert_eq!(plain[1], 0x00ff_ffff);
        let (r, g, b) = (warm[1] >> 16 & 0xff, warm[1] >> 8 & 0xff, warm[1] & 0xff);
        assert!(r == 255 && g < r && b < g, "white goes warm: {r} {g} {b}");
        assert_eq!((warm[0], warm[2]), (0, 0), "the black around stays black");
    }

    #[test]
    fn around_the_picture_stays_black() {
        let f = Frame { width: 1, height: 1, pitch: 4, format: PIXEL_XRGB8888, data: vec![1, 2, 3, 0] };
        let mut d = vec![7u32; 3];
        blit(&f, &mut d, 3, 1);
        assert_eq!(d, vec![0, 0x0003_0201, 0]);
        let mut wide = vec![7u32; 2 * 3];
        blit(&f, &mut wide, 2, 3);
        assert_eq!(wide, vec![0x0003_0201, 0x0003_0201, 0x0003_0201, 0x0003_0201, 0, 0]);
    }

    #[test]
    fn a_frame_shorter_than_it_says_does_not_panic_at_the_row_lookup() {
        let f = Frame { width: 0, height: 0, pitch: 0, format: PIXEL_XRGB8888, data: Vec::new() };
        let mut d = vec![7u32; 4];
        blit(&f, &mut d, 2, 2);
        assert_eq!(d, vec![0, 0, 0, 0]);
    }
}
