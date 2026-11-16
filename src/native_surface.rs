//! The handheld view's window on Android, without softbuffer's Android backend (docs/HANDHELD-PAINT.md).
//!
//! softbuffer's `buffer_mut()` locks the window, allocates a new zeroed `Vec` of the whole frame (8 MB at 1920x1080,
//! all of it page-faulted in again every frame) and its `present()` swaps the colours byte by byte through
//! `MaybeUninit<u8>`. Here the canvas lives as long as the surface and is painted *before* the window's buffer is
//! locked, so waiting for the compositor to hand a buffer back overlaps the painting instead of coming first; what is
//! left after the lock is one vectorised pass into the buffer (`rgbx`) and the post.

use crate::frame::{FrameError, Target};
use crate::rgbx;
use ndk::hardware_buffer_format::HardwareBufferFormat;
use ndk::native_window::NativeWindow;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::mem::MaybeUninit;
use std::num::NonZeroU32;
use std::time::{Duration, Instant};

pub struct NativeSurface {
    window: NativeWindow,
    /// The canvas, kept from frame to frame (painted over completely every time, as on any other target).
    canvas: rgbx::Kept,
    size: (u32, u32),
    /// The size the window's buffers were last asked for: asked again only when it changes.
    geometry: Option<(u32, u32)>,
    waited: Duration,
}

impl NativeSurface {
    pub fn new(window: &impl HasWindowHandle) -> Result<Self, FrameError> {
        let handle = window.window_handle().map_err(|e| FrameError(format!("window handle: {e}")))?;
        let RawWindowHandle::AndroidNdk(a) = handle.as_raw() else { return Err("not an Android window".into()) };
        // SAFETY: the handle is valid while `window` lives; this takes a reference of its own, dropped with the surface.
        let window = unsafe { NativeWindow::clone_from_ptr(a.a_native_window.cast()) };
        Ok(Self { window, canvas: rgbx::Kept::default(), size: (0, 0), geometry: None, waited: Duration::ZERO })
    }
}

impl Target for NativeSurface {
    fn resize(&mut self, w: NonZeroU32, h: NonZeroU32) -> Result<(), FrameError> {
        let size = (w.get(), h.get());
        if self.geometry != Some(size) {
            // 32 bits per pixel (the default is often 16), as softbuffer asks.
            self.window
                .set_buffers_geometry(size.0 as i32, size.1 as i32, Some(HardwareBufferFormat::R8G8B8X8_UNORM))
                .map_err(|e| FrameError(format!("set_buffers_geometry: {e}")))?;
            self.geometry = Some(size);
        }
        self.size = size;
        Ok(())
    }

    fn paint_present(&mut self, _width: u32, paint: &mut dyn FnMut(&mut [u32]) -> crate::damage::Rows) -> Result<(), FrameError> {
        // Android takes whole frames (no damage).
        paint(self.canvas.for_size(self.size));
        let asked = Instant::now();
        let mut buffer = self.window.lock(None).map_err(|e| FrameError(format!("lock: {e}")))?;
        self.waited = asked.elapsed();
        if !matches!(buffer.format(), HardwareBufferFormat::R8G8B8A8_UNORM | HardwareBufferFormat::R8G8B8X8_UNORM) {
            return Err(FrameError(format!("unexpected buffer format {:?}", buffer.format())));
        }
        // A buffer made for another size (the window was just resized): this frame is lost, the next one is right.
        if (buffer.width(), buffer.height()) != (self.size.0 as usize, self.size.1 as usize) {
            return Err(FrameError(format!("buffer is {}x{}, the canvas {}x{}", buffer.width(), buffer.height(), self.size.0, self.size.1)));
        }
        let (stride, width) = (buffer.stride(), self.size.0 as usize);
        let Some(bytes) = buffer.bytes() else { return Err("buffer without a pixel size".into()) };
        // SAFETY: any bytes are valid `MaybeUninit<u32>`; only the aligned middle is used.
        let (head, words, _) = unsafe { bytes.align_to_mut::<MaybeUninit<u32>>() };
        if !head.is_empty() {
            return Err("misaligned buffer".into());
        }
        rgbx::canvas_to_rgbx(self.canvas.for_size(self.size), width, words, stride);
        // Dropping the guard unlocks and posts.
        Ok(())
    }

    fn waited(&self) -> Duration {
        self.waited
    }
}
