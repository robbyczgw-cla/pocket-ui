// Frozen pre-optimisation CPU renderer: colour/sampling oracle and before/after benchmark.
#![allow(dead_code)]
use pocket_ui::display::Look;
use pocket_ui::libretro::{Frame, PIXEL_RGB565, PIXEL_XRGB8888};
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
                for x in 0..pw {
                    dst[at + x] = pixel(frame.format, row, x * fw / pw);
                }
            }
        }
        last = Some(sy);
    }
}

pub fn render(frame: &Frame, dst: &mut [u32], w: usize, h: usize, look: Look) {
    blit(frame, dst, w, h);
    if look.soft && (look.warmth != 0 || look.night != 0) {
        let t = look.tables();
        for p in dst {
            *p = (t[0][(*p >> 16 & 255) as usize] as u32) << 16 | (t[1][(*p >> 8 & 255) as usize] as u32) << 8 | t[2][(*p & 255) as usize] as u32;
        }
    }
}

pub fn frame(format: u32, w: usize, h: usize) -> Frame {
    let bytes = if format == PIXEL_XRGB8888 { 4 } else { 2 };
    let pitch = w * bytes + 12; // Padding must never enter the picture.
    let mut data = vec![0xa5; pitch * h];
    let mut seed = 0x12345678u32;
    for y in 0..h {
        for x in 0..w {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            let p = if x == 0 {
                0
            } else if x == w - 1 {
                u32::MAX
            } else {
                seed
            };
            data[y * pitch + x * bytes..y * pitch + (x + 1) * bytes].copy_from_slice(&p.to_le_bytes()[..bytes]);
        }
    }
    Frame {
        width: w as u32,
        height: h as u32,
        pitch,
        format,
        data,
    }
}
