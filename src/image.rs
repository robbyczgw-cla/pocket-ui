//! Small 0RGB images: the paused game frame (from grim) and save-state previews (RetroArch PNGs).

use std::path::Path;
use std::process::Command;
use std::time::Duration;

#[derive(Debug)]
pub struct Image {
    pub w: u32,
    pub h: u32,
    pub px: Vec<u32>,
}

/// The largest picture read from a file: 8192 pixels a side, 16.7 million pixels (64 MB as 0RGB). Covers, previews,
/// screenshots and frames are far below. A file claiming more is treated as damaged *before* anything is allocated
/// for it (review N2: a 196-byte PNG claiming 100 x 50,000,000 pixels made pocket-ui ask for 20 GB and abort, again
/// at every start, from the shelf's covers or the menu's previews).
pub const MAX_EDGE: u32 = 8192;
pub const MAX_PIXELS: u64 = 4096 * 4096;

/// Whether a picture of `w` x `h` may be decoded (see [`MAX_EDGE`]).
pub fn size_ok(w: u32, h: u32) -> bool {
    w > 0 && h > 0 && w <= MAX_EDGE && h <= MAX_EDGE && w as u64 * h as u64 <= MAX_PIXELS
}

/// A PNG reader past the header, with palette, grey and 16-bit expanded to 8-bit channels; None for a broken file or
/// one larger than [`size_ok`] allows. Every PNG pocket-ui decodes goes through here.
pub fn png_reader<R: std::io::Read>(r: R) -> Option<png::Reader<R>> {
    let mut decoder = png::Decoder::new(r);
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let reader = decoder.read_info().ok()?;
    let (w, h) = (reader.info().width, reader.info().height);
    if !size_ok(w, h) {
        eprintln!("image: a PNG of {w} x {h} pixels is not read");
        return None;
    }
    Some(reader)
}

/// Screenshot of the whole output via `grim` (PPM on stdout). None when grim fails or doesn't answer in `timeout`.
pub fn capture_screen(timeout: Duration) -> Option<Image> {
    capture_via(Command::new("grim").args(["-t", "ppm", "-"]), timeout)
}

fn capture_via(cmd: &mut Command, timeout: Duration) -> Option<Image> {
    let out = crate::proc::run_with_timeout(cmd, timeout).ok()?;
    out.status.success().then(|| parse_ppm(&out.stdout)).flatten()
}

/// Binary PPM (P6, maxval 255) as written by grim.
pub fn parse_ppm(data: &[u8]) -> Option<Image> {
    let mut fields = Vec::new();
    let mut i = 0;
    while fields.len() < 4 {
        while data.get(i)?.is_ascii_whitespace() {
            i += 1;
        }
        if data[i] == b'#' {
            while *data.get(i)? != b'\n' {
                i += 1;
            }
            continue;
        }
        let start = i;
        while !data.get(i)?.is_ascii_whitespace() {
            i += 1;
        }
        fields.push(std::str::from_utf8(&data[start..i]).ok()?.to_string());
    }
    if fields[0] != "P6" || fields[3] != "255" {
        return None;
    }
    let (w, h): (u32, u32) = (fields[1].parse().ok()?, fields[2].parse().ok()?);
    if !size_ok(w, h) {
        return None;
    }
    let body = data.get(i + 1..i + 1 + w as usize * h as usize * 3)?;
    let px = body.chunks_exact(3).map(|p| (p[0] as u32) << 16 | (p[1] as u32) << 8 | p[2] as u32).collect();
    Some(Image { w, h, px })
}

/// Any PNG (palette, grey, 16-bit and alpha are converted); transparent pixels end up on black.
/// Width and height of a PNG from its header alone (the IHDR chunk in the first 24 bytes), without decoding it.
pub fn png_size(path: &Path) -> Option<(u32, u32)> {
    use std::io::Read;
    let mut head = [0u8; 24];
    std::fs::File::open(path).ok()?.read_exact(&mut head).ok()?;
    if &head[..8] != b"\x89PNG\r\n\x1a\n" || &head[12..16] != b"IHDR" {
        return None;
    }
    let (w, h) = (u32::from_be_bytes(head[16..20].try_into().ok()?), u32::from_be_bytes(head[20..24].try_into().ok()?));
    (w > 0 && h > 0 && size_ok(w, h)).then_some((w, h))
}

pub fn load_png(path: &Path) -> Option<Image> {
    let mut reader = png_reader(std::io::BufReader::new(std::fs::File::open(path).ok()?))?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).ok()?;
    let rgb = |r: u8, g: u8, b: u8, a: u8| {
        let s = |c: u8| (c as u32 * a as u32 / 255) & 0xff;
        s(r) << 16 | s(g) << 8 | s(b)
    };
    let data = &buf[..info.buffer_size()];
    let px: Vec<u32> = match info.color_type {
        png::ColorType::Rgb => data.chunks_exact(3).map(|p| rgb(p[0], p[1], p[2], 255)).collect(),
        png::ColorType::Rgba => data.chunks_exact(4).map(|p| rgb(p[0], p[1], p[2], p[3])).collect(),
        png::ColorType::Grayscale => data.iter().map(|g| rgb(*g, *g, *g, 255)).collect(),
        png::ColorType::GrayscaleAlpha => data.chunks_exact(2).map(|p| rgb(p[0], p[0], p[0], p[1])).collect(),
        png::ColorType::Indexed => return None,
    };
    Some(Image { w: info.width, h: info.height, px })
}

impl Image {
    /// Size that fits inside `max_w` x `max_h` while keeping the aspect ratio.
    pub fn fit(&self, max_w: i32, max_h: i32) -> (i32, i32) {
        if self.w == 0 || self.h == 0 {
            return (0, 0);
        }
        let scale = (max_w as f32 / self.w as f32).min(max_h as f32 / self.h as f32);
        ((self.w as f32 * scale) as i32, (self.h as f32 * scale) as i32)
    }

    /// Box-filtered downscale (each target pixel averages its source area); used for photos and box art,
    /// where nearest-neighbour would shimmer.
    pub fn resized(&self, w: i32, h: i32) -> Image {
        let (w, h) = (w.max(1) as u32, h.max(1) as u32);
        let mut px = Vec::with_capacity((w * h) as usize);
        for ty in 0..h {
            let (y0, y1) = (ty * self.h / h, ((ty + 1) * self.h / h).max(ty * self.h / h + 1));
            for tx in 0..w {
                let (x0, x1) = (tx * self.w / w, ((tx + 1) * self.w / w).max(tx * self.w / w + 1));
                let (mut r, mut g, mut b, mut n) = (0u32, 0u32, 0u32, 0u32);
                for sy in y0..y1.min(self.h) {
                    for sx in x0..x1.min(self.w) {
                        let p = self.px[(sy * self.w + sx) as usize];
                        r += p >> 16 & 0xff;
                        g += p >> 8 & 0xff;
                        b += p & 0xff;
                        n += 1;
                    }
                }
                let n = n.max(1);
                px.push((r / n) << 16 | (g / n) << 8 | b / n);
            }
        }
        Image { w, h, px }
    }
}

/// Baseline/progressive JPEG (PortMaster screenshots).
pub fn load_jpeg(path: &Path) -> Option<Image> {
    let mut decoder = jpeg_decoder::Decoder::new(std::io::BufReader::new(std::fs::File::open(path).ok()?));
    // The header first: nothing is allocated for a picture larger than `size_ok` allows (N2).
    decoder.read_info().ok()?;
    let head = decoder.info()?;
    if !size_ok(head.width as u32, head.height as u32) {
        eprintln!("image: a JPEG of {} x {} pixels is not read", head.width, head.height);
        return None;
    }
    let data = decoder.decode().ok()?;
    let info = decoder.info()?;
    let px: Vec<u32> = match info.pixel_format {
        jpeg_decoder::PixelFormat::RGB24 => data.chunks_exact(3).map(|p| (p[0] as u32) << 16 | (p[1] as u32) << 8 | p[2] as u32).collect(),
        jpeg_decoder::PixelFormat::L8 => data.iter().map(|g| (*g as u32) * 0x010101).collect(),
        _ => return None,
    };
    Some(Image { w: info.width as u32, h: info.height as u32, px })
}

/// PNG or JPEG by file extension.
pub fn load_image(path: &Path) -> Option<Image> {
    match path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).as_deref() {
        Some("png") => load_png(path),
        Some("jpg" | "jpeg") => load_jpeg(path),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;
    use std::time::Instant;

    fn sh(script: &str) -> Command {
        let mut c = Command::new("sh");
        c.args(["-c", script]);
        c
    }

    fn crc32(bytes: &[u8]) -> u32 {
        let mut c = !0u32;
        for b in bytes {
            c ^= *b as u32;
            for _ in 0..8 {
                c = if c & 1 != 0 { 0xedb8_8320 ^ (c >> 1) } else { c >> 1 };
            }
        }
        !c
    }

    /// A PNG whose header claims `w` x `h` RGB pixels, with one row of data (a few hundred bytes).
    fn png_claiming(w: u32, h: u32) -> Vec<u8> {
        let chunk = |out: &mut Vec<u8>, kind: &[u8], data: &[u8]| {
            out.extend_from_slice(&(data.len() as u32).to_be_bytes());
            let start = out.len();
            out.extend_from_slice(kind);
            out.extend_from_slice(data);
            let crc = crc32(&out[start..]);
            out.extend_from_slice(&crc.to_be_bytes());
        };
        let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
        let mut ihdr = Vec::new();
        ihdr.extend_from_slice(&w.to_be_bytes());
        ihdr.extend_from_slice(&h.to_be_bytes());
        ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
        chunk(&mut out, b"IHDR", &ihdr);
        let row = vec![0u8; 1 + w.min(64) as usize * 3];
        chunk(&mut out, b"IDAT", &miniz_oxide::deflate::compress_to_vec_zlib(&row, 6));
        chunk(&mut out, b"IEND", &[]);
        out
    }

    #[test]
    fn a_picture_claiming_absurd_dimensions_is_refused_before_anything_is_allocated() {
        // Review N2: 100 x 50,000,000 made the decoder buffer 15 GB (and pocket-ui abort) from a 200-byte file.
        let dir = TestDir::new("image-n2");
        let bomb = dir.join("cover.png");
        std::fs::write(&bomb, png_claiming(100, 50_000_000)).unwrap();
        assert!(std::fs::metadata(&bomb).unwrap().len() < 300);
        assert!(png_reader(std::io::Cursor::new(png_claiming(100, 10))).is_some(), "the crafted file itself is a valid PNG");
        assert!(png_reader(std::io::Cursor::new(png_claiming(100, 50_000_000))).is_none());
        assert!(load_png(&bomb).is_none());
        assert!(load_image(&bomb).is_none());
        assert!(crate::bezel::load_png_rgba(&bomb).is_none());
        assert!(png_reader(std::io::Cursor::new(png_claiming(9000, 10))).is_none(), "an edge over 8192");
        assert!(png_reader(std::io::Cursor::new(png_claiming(5000, 5000))).is_none(), "25 MP");
        // A real one of the largest kind still loads.
        let ok = dir.join("ok.png");
        let f = std::fs::File::create(&ok).unwrap();
        let mut enc = png::Encoder::new(std::io::BufWriter::new(f), 4096, 2);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header().unwrap().write_image_data(&vec![9u8; 4096 * 2 * 3]).unwrap();
        let img = load_png(&ok).expect("a sane PNG");
        assert_eq!((img.w, img.h, img.px.len()), (4096, 2, 8192));
        // A JPEG header claiming 65,000 x 65,000 (SOF0, one component; no data follows).
        let mut jpeg = vec![0xff, 0xd8, 0xff, 0xc0, 0, 11, 8];
        jpeg.extend_from_slice(&65000u16.to_be_bytes());
        jpeg.extend_from_slice(&65000u16.to_be_bytes());
        jpeg.extend_from_slice(&[1, 1, 0x11, 0]);
        jpeg.extend_from_slice(&[0xff, 0xd9]);
        let big = dir.join("shot.jpg");
        std::fs::write(&big, jpeg).unwrap();
        assert!(load_jpeg(&big).is_none());
        // A PPM claiming more than its bytes, or more than allowed: None (it used to overflow `w * h * 3`).
        assert!(parse_ppm(b"P6\n70000 70000\n255\n\0\0\0").is_none());
        assert!(parse_ppm(b"P6\n2 2\n255\n\0\0\0").is_none());
    }

    #[test]
    fn sizes_allowed() {
        assert!(size_ok(1, 1) && size_ok(1240, 1080) && size_ok(4096, 4096) && size_ok(8192, 2048));
        assert!(!size_ok(0, 10) && !size_ok(8193, 1) && !size_ok(1, 8193) && !size_ok(8192, 8192) && !size_ok(100, 50_000_000));
    }

    #[test]
    fn a_grim_that_hangs_gives_no_picture_instead_of_a_frozen_ui() {
        let t0 = Instant::now();
        assert!(capture_via(&mut sh("sleep 5"), Duration::from_millis(200)).is_none());
        assert!(t0.elapsed() < Duration::from_secs(2), "waited {:?}", t0.elapsed());
    }

    #[test]
    fn a_grim_that_answers_in_time_gives_the_picture() {
        let img = capture_via(&mut sh("printf 'P6\\n2 1\\n255\\n\\377\\0\\0\\0\\377\\0'"), Duration::from_secs(5)).expect("a picture");
        assert_eq!((img.w, img.h), (2, 1));
        assert_eq!(img.px, vec![0xff0000, 0x00ff00]);
    }

    #[test]
    fn a_failing_grim_gives_none() {
        assert!(capture_via(&mut sh("exit 1"), Duration::from_secs(5)).is_none());
        assert!(capture_via(&mut Command::new("pocket-ui-no-such-grim"), Duration::from_secs(5)).is_none());
    }
}
