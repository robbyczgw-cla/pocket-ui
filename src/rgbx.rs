//! The canvas's pixels (0RGB in a `u32`: blue in the low byte) as the bytes an Android window wants (R, G, B, X).
//! softbuffer's Android backend does this byte by byte through `MaybeUninit<u8>` and on a zeroed buffer made anew for
//! every frame (its own comment: "pretty slow"); here it is one byte swap per pixel, which the compiler turns into a
//! vector instruction, written straight into the locked buffer (`native_surface`).

use std::mem::MaybeUninit;

/// 0x00RRGGBB as the little-endian word whose bytes are R, G, B, 0xff: 0xffBBGGRR.
#[inline(always)]
pub fn to_rgbx(p: u32) -> u32 {
    (p.swap_bytes() >> 8) | 0xff00_0000
}

/// One row; `dst` is as long as `src`.
#[inline]
pub fn row_to_rgbx(src: &[u32], dst: &mut [MaybeUninit<u32>]) {
    for (s, d) in src.iter().zip(dst) {
        d.write(to_rgbx(*s));
    }
}

/// The whole canvas (`src`, rows of `width` pixels) into a buffer whose rows are `stride` pixels apart. Rows or columns
/// the buffer doesn't have are left out; the stride's padding is not touched.
pub fn canvas_to_rgbx(src: &[u32], width: usize, dst: &mut [MaybeUninit<u32>], stride: usize) {
    if width == 0 || stride == 0 {
        return;
    }
    let columns = width.min(stride);
    for (s, d) in src.chunks_exact(width).zip(dst.chunks_mut(stride)) {
        let n = columns.min(d.len());
        row_to_rgbx(&s[..n], &mut d[..n]);
    }
}

/// The canvas of a surface, kept from frame to frame: made once per size, never cleared (the page paints all of it, as on
/// every other target). softbuffer's Android backend made a new zeroed one every frame.
#[derive(Default)]
pub struct Kept {
    pixels: Vec<u32>,
    size: (u32, u32),
}

impl Kept {
    /// The canvas for `size`; the same memory as last time while the size stays.
    pub fn for_size(&mut self, size: (u32, u32)) -> &mut [u32] {
        if self.size != size {
            self.size = size;
            self.pixels.clear();
            self.pixels.resize(size.0 as usize * size.1 as usize, 0);
        }
        &mut self.pixels
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bytes(words: &[MaybeUninit<u32>]) -> Vec<u8> {
        words.iter().flat_map(|w| unsafe { w.assume_init() }.to_le_bytes()).collect()
    }

    #[test]
    fn the_canvas_is_the_same_memory_from_frame_to_frame_and_new_only_for_a_new_size() {
        let mut kept = Kept::default();
        let first = kept.for_size((1920, 1080));
        first[7] = 0xabcdef;
        let at = first.as_ptr();
        assert_eq!(kept.for_size((1920, 1080)).as_ptr(), at, "no new buffer per frame");
        assert_eq!(kept.for_size((1920, 1080))[7], 0xabcdef, "and not cleared: the page paints all of it");
        let smaller = kept.for_size((1240, 1080));
        assert_eq!(smaller.len(), 1240 * 1080);
        assert!(smaller.iter().all(|p| *p == 0));
    }

    /// The thread's own CPU time (not the wall clock: a busy computer doesn't fail it).
    fn thread_cpu() -> std::time::Duration {
        let mut t = libc::timespec { tv_sec: 0, tv_nsec: 0 };
        // SAFETY: a valid timespec to fill in.
        unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &mut t) };
        std::time::Duration::new(t.tv_sec as u64, t.tv_nsec as u32)
    }

    #[test]
    fn a_1080p_frame_goes_into_the_window_in_a_few_ms_of_cpu() {
        // The handheld view's frame at 1920x1080 had 12 ms in all on the Odin, softbuffer's copy about half of it; this pass
        // is a few ms on the slowest build computer (about 1 ms here). The bound is generous: it catches a per-pixel
        // call or a second pass, not noise.
        let src: Vec<u32> = (0..1920 * 1080u32).map(|i| i.wrapping_mul(2654435761) & 0xffffff).collect();
        let mut dst = vec![MaybeUninit::<u32>::uninit(); 1920 * 1088];
        canvas_to_rgbx(&src, 1920, &mut dst, 1920);
        let at = thread_cpu();
        for _ in 0..10 {
            canvas_to_rgbx(&src, 1920, &mut dst, 1920);
        }
        let per_frame = (thread_cpu() - at) / 10;
        assert!(per_frame < std::time::Duration::from_millis(10), "{per_frame:?} per 1080p frame");
        assert_eq!(unsafe { dst[12345].assume_init() }, to_rgbx(src[12345]));
    }

    #[test]
    fn a_pixel_comes_out_as_the_bytes_red_green_blue_and_opaque() {
        let mut out = [MaybeUninit::uninit(); 2];
        row_to_rgbx(&[0x112233, 0xff0080], &mut out);
        assert_eq!(bytes(&out), [0x11, 0x22, 0x33, 0xff, 0xff, 0x00, 0x80, 0xff]);
    }

    #[test]
    fn it_is_the_swizzle_softbuffer_does_pixel_by_pixel() {
        // softbuffer's Android present(): `let [b, g, r, a] = pixel.to_le_bytes(); out = [r, g, b, a]`.
        for p in [0u32, 0xffffff, 0x123456, 0x00ff00, 0x0000ff, 0xff0000, 0x7f8081] {
            let [b, g, r, a] = p.to_le_bytes();
            assert_eq!(to_rgbx(p).to_le_bytes(), [r, g, b, 0xff], "{p:06x} (its own alpha byte {a} is ignored: the window has none)");
        }
    }

    #[test]
    fn rows_land_at_the_stride_and_the_padding_stays_untouched() {
        let src = [0x010203, 0x040506, 0x070809, 0x0a0b0c];
        let mut dst = [MaybeUninit::new(0xdead_beef); 6];
        canvas_to_rgbx(&src, 2, &mut dst, 3);
        let words: Vec<u32> = dst.iter().map(|w| unsafe { w.assume_init() }).collect();
        assert_eq!(words[2], 0xdead_beef);
        assert_eq!(words[5], 0xdead_beef);
        assert_eq!(&words[..2], [to_rgbx(0x010203), to_rgbx(0x040506)]);
        assert_eq!(&words[3..5], [to_rgbx(0x070809), to_rgbx(0x0a0b0c)]);
    }

    #[test]
    fn a_buffer_smaller_than_the_canvas_is_filled_as_far_as_it_goes() {
        let src = [1u32; 4 * 3];
        let mut dst = [MaybeUninit::new(0u32); 2 * 2];
        canvas_to_rgbx(&src, 4, &mut dst, 2);
        assert!(dst.iter().all(|w| unsafe { w.assume_init() } == to_rgbx(1)));
        // And one with a short last row.
        let mut dst = [MaybeUninit::new(0u32); 3];
        canvas_to_rgbx(&src, 4, &mut dst, 2);
        assert_eq!(unsafe { dst[2].assume_init() }, to_rgbx(1));
    }
}
