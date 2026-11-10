//! The browser demo's softbuffer (docs/WEB-DEMO.md): `Surface::buffer_mut` hands the App a 0RGB buffer, `present`
//! turns it into RGBA for the page's `putImageData` and keeps it until the page takes it (`web::take_frame`).

use std::marker::PhantomData;
use std::num::NonZeroU32;
use std::ops::{Deref, DerefMut};
use std::sync::Mutex;

#[derive(Debug)]
pub struct SoftBufferError(pub &'static str);
impl std::fmt::Display for SoftBufferError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for SoftBufferError {}

pub struct Context<D>(PhantomData<D>);
impl<D> Context<D> {
    pub fn new(_display: D) -> Result<Self, SoftBufferError> {
        Ok(Context(PhantomData))
    }
}

pub struct Surface<D, W> {
    _window: W,
    _display: PhantomData<D>,
    size: (u32, u32),
    pixels: Vec<u32>,
}

impl<D, W> Surface<D, W> {
    pub fn new(_context: &Context<D>, window: W) -> Result<Self, SoftBufferError> {
        Ok(Surface { _window: window, _display: PhantomData, size: (0, 0), pixels: Vec::new() })
    }
    pub fn resize(&mut self, w: NonZeroU32, h: NonZeroU32) -> Result<(), SoftBufferError> {
        self.size = (w.get(), h.get());
        self.pixels.resize((w.get() * h.get()) as usize, 0);
        Ok(())
    }
    pub fn buffer_mut(&mut self) -> Result<Buffer<'_, D, W>, SoftBufferError> {
        if self.pixels.is_empty() {
            return Err(SoftBufferError("surface not resized"));
        }
        Ok(Buffer { surface: self })
    }
}

pub struct Buffer<'a, D, W> {
    surface: &'a mut Surface<D, W>,
}
impl<D, W> Buffer<'_, D, W> {
    pub fn present(self) -> Result<(), SoftBufferError> {
        web::publish(&self.surface.pixels, self.surface.size);
        Ok(())
    }
    pub fn age(&self) -> u8 {
        0
    }
}
impl<D, W> Deref for Buffer<'_, D, W> {
    type Target = [u32];
    fn deref(&self) -> &[u32] {
        &self.surface.pixels
    }
}
impl<D, W> DerefMut for Buffer<'_, D, W> {
    fn deref_mut(&mut self) -> &mut [u32] {
        &mut self.surface.pixels
    }
}

pub mod web {
    use super::*;

    pub(crate) struct Frame {
        pub rgba: Vec<u8>,
        pub size: (u32, u32),
        pub seq: u64,
    }

    static FRAME: Mutex<Frame> = Mutex::new(Frame { rgba: Vec::new(), size: (0, 0), seq: 0 });

    pub(crate) fn publish(pixels: &[u32], size: (u32, u32)) {
        let mut f = FRAME.lock().unwrap_or_else(|p| p.into_inner());
        f.rgba.resize(pixels.len() * 4, 0);
        for (out, &p) in f.rgba.chunks_exact_mut(4).zip(pixels) {
            out.copy_from_slice(&[(p >> 16) as u8, (p >> 8) as u8, p as u8, 255]);
        }
        f.size = size;
        f.seq += 1;
    }

    /// The newest frame when it is newer than `seen`: copied into `out` (resized to fit), with its size and number.
    pub fn take_frame(seen: u64, out: &mut Vec<u8>) -> Option<(u32, u32, u64)> {
        let f = FRAME.lock().unwrap_or_else(|p| p.into_inner());
        if f.seq == seen || f.rgba.is_empty() {
            return None;
        }
        out.clear();
        out.extend_from_slice(&f.rgba);
        Some((f.size.0, f.size.1, f.seq))
    }
}
