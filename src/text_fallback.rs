//! The second font of the text painter (`text.rs`): the system's Noto Sans CJK, for the Japanese, Chinese and Korean
//! letters Barlow has no picture for ("ファイナルファンタジーVI", "スーパーマリオワールド"). Not ours and not shipped:
//! ROCKNIX installs it (`packages/fonts/noto-sans-cjk`, one 16 MB OpenType file with 65 535 glyphs). The virtual device has
//! it too (its root is the ROCKNIX image's). Without it (a desktop computer) the painter does what it did before: "?" for a letter, nothing for a mark.
//!
//! fontdue, which paints Barlow, turns every glyph of a font into an outline when it loads it: for this font seconds
//! and well over a hundred MB. So this one is read differently: the file is mapped into memory (the kernel reads
//! only the pages a glyph touches, and can drop them again), `ttf-parser` finds one glyph's outline when it is asked
//! for, and a small rasteriser here turns it into the same coverage bitmap fontdue gives (signed area accumulated per
//! pixel, the method of font-rs). A glyph is drawn once per size and kept in the painter's cache like Barlow's.
//!
//! Opened at most once per process and file (`open`), the first time a letter needs it: on the thread that draws a
//! library's names ahead of time (`Text::prewarm_names`), else on the spot. Never per frame.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use fontdue::{Metrics, OutlineBounds};
use ttf_parser::{Face, OutlineBuilder};

/// Where ROCKNIX puts the font (`noto-sans-cjk/package.mk`, ROCKNIX 20261001).
pub const SYSTEM_CJK: &str = "/usr/share/fonts/truetype/noto-cjk/NotoSansCJKsc-Regular.otf";

/// The font file to fall back to: `POCKETUI_CJK_FONT` if set (empty or `off`: none, as without the font), else
/// ROCKNIX's. The snapshots and tests point it at the file of an unpacked ROCKNIX image.
pub fn path() -> Option<PathBuf> {
    match std::env::var_os("POCKETUI_CJK_FONT") {
        Some(p) if p.is_empty() || p == "off" => None,
        Some(p) => Some(PathBuf::from(p)),
        None => Some(PathBuf::from(SYSTEM_CJK)),
    }
}

/// A fallback font, mapped and parsed once.
pub struct Fallback {
    face: Face<'static>,
    /// Font units per pixel at 1 px: 1 / units per em.
    unit: f32,
}

/// The fonts opened so far in this process, by file; `None` for one that is missing or no font.
static OPENED: Mutex<Vec<(PathBuf, Option<Arc<Fallback>>)>> = Mutex::new(Vec::new());

/// The font in `path`, mapped and parsed the first time it is asked for in this process; a second caller (the
/// prewarm thread and a frame at once) waits for the first and gets the same. `None` when it is missing or broken,
/// remembered too: a missing font costs one `open` per process.
pub fn open(path: &Path) -> Option<Arc<Fallback>> {
    let mut opened = OPENED.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((_, f)) = opened.iter().find(|(p, _)| p == path) {
        return f.clone();
    }
    let font = load(path).map_err(|e| eprintln!("text: no fallback font for CJK ({}: {e}), \"?\" stands in for those letters", path.display())).ok().map(Arc::new);
    opened.push((path.to_path_buf(), font.clone()));
    font
}

fn load(path: &Path) -> Result<Fallback, String> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    // SAFETY: the file is a read-only system font (squashfs on the device); nothing writes to it while it is mapped.
    // The mapping lives as long as the process (once per file, `open`), so the parsed face may borrow it forever.
    let map = unsafe { memmap2::Mmap::map(&file) }.map_err(|e| e.to_string())?;
    let data: &'static [u8] = Box::leak(Box::new(map));
    let face = Face::parse(data, 0).map_err(|e| e.to_string())?;
    Ok(Fallback { unit: 1.0 / face.units_per_em() as f32, face })
}

/// How a fallback glyph is fitted to the face it stands in for (`Fit::to`): its size, its height on the line, and
/// how much bolder.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fit {
    /// The fallback's em in ems of the main face.
    pub scale: f32,
    /// Moved up by this many ems.
    pub rise: f32,
    /// Made bolder by this many ems (the outline painted again up to that far right and up, `copies`).
    pub bold: f32,
}

impl Fit {
    pub const PLAIN: Fit = Fit { scale: 1.0, rise: 0.0, bold: 0.0 };

    /// The fit for a main face whose capitals ("H") stand `cap` ems high from `cap_bottom` ems up: an ideograph ("国")
    /// as high as a capital and a sixth, its middle on the capitals' middle, so that "ファイナルファンタジーVI" reads
    /// as one line; `bold` ems bolder (the main face's weight, `Text`).
    pub fn to(fallback: &Fallback, cap: f32, cap_bottom: f32, bold: f32) -> Fit {
        let Some((lo, hi)) = fallback.bounds('国') else { return Fit { bold, ..Fit::PLAIN } };
        if hi <= lo || cap <= 0.0 {
            return Fit { bold, ..Fit::PLAIN };
        }
        let scale = (cap * 7.0 / 6.0 / (hi - lo)).clamp(0.5, 1.5);
        let rise = (cap_bottom + cap / 2.0) - (lo + hi) / 2.0 * scale;
        Fit { scale, rise, bold }
    }
}

impl Fallback {
    pub fn has(&self, ch: char) -> bool {
        self.face.glyph_index(ch).is_some_and(|g| g.0 != 0)
    }

    /// Bottom and top of `ch`'s outline in ems.
    fn bounds(&self, ch: char) -> Option<(f32, f32)> {
        let r = self.face.glyph_bounding_box(self.face.glyph_index(ch)?)?;
        Some((r.y_min as f32 * self.unit, r.y_max as f32 * self.unit))
    }

    /// `ch` at `px` pixels, fitted by `fit`, as fontdue would give it (metrics and a row-major coverage bitmap); `None`
    /// when the font has no glyph for it.
    pub fn raster(&self, ch: char, px: u32, fit: Fit) -> Option<(Metrics, Vec<u8>)> {
        let id = self.face.glyph_index(ch).filter(|g| g.0 != 0)?;
        let em = px as f32;
        let k = em * fit.scale * self.unit;
        let bold = (fit.bold * em).max(0.0);
        let advance = self.face.glyph_hor_advance(id).unwrap_or(0) as f32 * k + bold;
        // Half the extra weight goes below the outline, half above (`copies` paints up to `bold` up): the letter stays
        // centred on the capitals.
        let mut path = Flatten { k, rise: fit.rise * em - bold / 2.0, tol: 0.2, lines: Vec::new(), start: (0.0, 0.0), at: (0.0, 0.0) };
        if self.face.outline_glyph(id, &mut path).is_none() || path.lines.is_empty() {
            // A space of the font (an ideographic space): its advance, no ink.
            return Some((Metrics { advance_width: advance, ..Metrics::default() }, Vec::new()));
        }
        Some(coverage(&path.lines, advance, bold))
    }
}

/// A line of an outline in pixels, y upwards from the baseline.
type Line = ((f32, f32), (f32, f32));

/// The outline as lines in pixels: curves cut into pieces less than `tol` px off the curve.
struct Flatten {
    k: f32,
    rise: f32,
    tol: f32,
    lines: Vec<Line>,
    start: (f32, f32),
    at: (f32, f32),
}

impl Flatten {
    fn pt(&self, x: f32, y: f32) -> (f32, f32) {
        (x * self.k, y * self.k + self.rise)
    }

    fn to(&mut self, p: (f32, f32)) {
        if p != self.at {
            self.lines.push((self.at, p));
        }
        self.at = p;
    }

    /// Pieces for a curve bending `dev` px away from its chord (the second difference of its points).
    fn pieces(&self, dev: f32) -> usize {
        ((dev / self.tol).sqrt().ceil() as usize).clamp(1, 32)
    }
}

impl OutlineBuilder for Flatten {
    fn move_to(&mut self, x: f32, y: f32) {
        self.close();
        self.start = self.pt(x, y);
        self.at = self.start;
    }

    fn line_to(&mut self, x: f32, y: f32) {
        let p = self.pt(x, y);
        self.to(p);
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let (p0, p1, p2) = (self.at, self.pt(x1, y1), self.pt(x, y));
        let dev = ((p0.0 - 2.0 * p1.0 + p2.0).hypot(p0.1 - 2.0 * p1.1 + p2.1)) / 4.0;
        let n = self.pieces(dev);
        for i in 1..=n {
            let t = i as f32 / n as f32;
            let u = 1.0 - t;
            self.to((u * u * p0.0 + 2.0 * u * t * p1.0 + t * t * p2.0, u * u * p0.1 + 2.0 * u * t * p1.1 + t * t * p2.1));
        }
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let (p0, p1, p2, p3) = (self.at, self.pt(x1, y1), self.pt(x2, y2), self.pt(x, y));
        let d1 = (p0.0 - 2.0 * p1.0 + p2.0).hypot(p0.1 - 2.0 * p1.1 + p2.1);
        let d2 = (p1.0 - 2.0 * p2.0 + p3.0).hypot(p1.1 - 2.0 * p2.1 + p3.1);
        let n = self.pieces(d1.max(d2) * 0.75);
        for i in 1..=n {
            let t = i as f32 / n as f32;
            let u = 1.0 - t;
            let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
            self.to((a * p0.0 + b * p1.0 + c * p2.0 + d * p3.0, a * p0.1 + b * p1.1 + c * p2.1 + d * p3.1));
        }
    }

    fn close(&mut self) {
        let start = self.start;
        self.to(start);
    }
}

/// Where the outline is painted to make it `bold` px bolder, right and up from where it is: none for 0; up to 1.5 px
/// the four corners of a square (the strokes of a 37 px name grow by a pixel); beyond, points on a circle, so that a
/// big letter (the jump's, 200 px) keeps round ends and clean diagonals instead of the square's steps.
fn copies(bold: f32) -> Vec<(f32, f32)> {
    if bold <= 0.0 {
        return vec![(0.0, 0.0)];
    }
    if bold <= 1.5 {
        return vec![(0.0, 0.0), (bold, 0.0), (0.0, bold), (bold, bold)];
    }
    let (r, n) = (bold / 2.0, if bold <= 4.0 { 8 } else { 12 });
    (0..n).map(|i| (i as f32 / n as f32) * std::f32::consts::TAU).map(|a| (r + r * a.cos(), r + r * a.sin())).collect()
}

/// The coverage of `lines` (nonzero winding), painted `bold` px bolder (`copies`), as a bitmap with fontdue's metrics.
fn coverage(lines: &[Line], advance: f32, bold: f32) -> (Metrics, Vec<u8>) {
    let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for &((ax, ay), (bx, by)) in lines {
        x0 = x0.min(ax).min(bx);
        x1 = x1.max(ax).max(bx);
        y0 = y0.min(ay).min(by);
        y1 = y1.max(ay).max(by);
    }
    let (xmin, ymin) = (x0.floor() as i32, y0.floor() as i32);
    let (xmax, ymax) = ((x1 + bold).ceil() as i32, (y1 + bold).ceil() as i32);
    let (w, h) = ((xmax - xmin).max(1) as usize, (ymax - ymin).max(1) as usize);
    let mut acc = Accumulator::new(w, h);
    for (dx, dy) in copies(bold) {
        let (ox, oy) = (dx - xmin as f32, ymax as f32 - dy);
        for &((ax, ay), (bx, by)) in lines {
            // Into the bitmap: x from its left edge, y downwards from its top row.
            acc.line((ax + ox, oy - ay), (bx + ox, oy - by));
        }
    }
    let metrics = Metrics {
        xmin,
        ymin,
        width: w,
        height: h,
        advance_width: advance,
        advance_height: 0.0,
        bounds: OutlineBounds { xmin: x0, ymin: y0, width: x1 + bold - x0, height: y1 + bold - y0 },
    };
    (metrics, acc.finish())
}

/// Signed area per pixel: each line adds, to the pixels it crosses, how much of them lies to its right times its
/// direction; summed along a row that is the winding coverage of every pixel.
struct Accumulator {
    w: usize,
    h: usize,
    /// One spare cell per row: a line on the right edge spills into it.
    cells: Vec<f32>,
}

impl Accumulator {
    fn new(w: usize, h: usize) -> Self {
        Self { w, h, cells: vec![0.0; (w + 1) * h] }
    }

    fn add(&mut self, row: usize, x: i32, v: f32) {
        let x = x.clamp(0, self.w as i32) as usize;
        self.cells[row * (self.w + 1) + x] += v;
    }

    fn line(&mut self, a: (f32, f32), b: (f32, f32)) {
        if (a.1 - b.1).abs() < 1e-6 {
            return;
        }
        let (dir, top, bottom) = if a.1 < b.1 { (1.0, a, b) } else { (-1.0, b, a) };
        let dxdy = (bottom.0 - top.0) / (bottom.1 - top.1);
        let first = top.1.max(0.0).floor() as usize;
        let last = (bottom.1.ceil().max(0.0) as usize).min(self.h);
        for row in first..last {
            // The piece of the line within this row.
            let (ya, yb) = ((row as f32).max(top.1), ((row + 1) as f32).min(bottom.1));
            if yb <= ya {
                continue;
            }
            let (xa, xb) = (top.0 + (ya - top.1) * dxdy, top.0 + (yb - top.1) * dxdy);
            let d = (yb - ya) * dir;
            let (l, r) = if xa < xb { (xa, xb) } else { (xb, xa) };
            let li = l.floor();
            let ri = r.ceil();
            if ri - li <= 1.0 {
                // Within one pixel: the part right of the piece's middle stays in it, the rest goes to the next.
                let mid = (l + r) / 2.0 - li;
                self.add(row, li as i32, d * (1.0 - mid));
                self.add(row, li as i32 + 1, d * mid);
                continue;
            }
            // Across several pixels: the area right of the piece in each, in steps of 1 / slope.
            let s = 1.0 / (r - l);
            let lf = l - li;
            let a0 = 0.5 * s * (1.0 - lf) * (1.0 - lf);
            let rf = r - ri + 1.0;
            let am = 0.5 * s * rf * rf;
            let (li, ri) = (li as i32, ri as i32);
            self.add(row, li, d * a0);
            if ri == li + 2 {
                self.add(row, li + 1, d * (1.0 - a0 - am));
            } else {
                let a1 = s * (1.5 - lf);
                self.add(row, li + 1, d * (a1 - a0));
                for x in li + 2..ri - 1 {
                    self.add(row, x, d * s);
                }
                let a2 = a1 + (ri - li - 3) as f32 * s;
                self.add(row, ri - 1, d * (1.0 - a2 - am));
            }
            self.add(row, ri, d * am);
        }
    }

    fn finish(self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.w * self.h);
        for row in self.cells.chunks_exact(self.w + 1) {
            let mut sum = 0.0f32;
            for &c in &row[..self.w] {
                sum += c;
                out.push((sum.abs().min(1.0) * 255.0 + 0.5) as u8);
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_square_comes_out_covered_inside_and_half_covered_on_a_half_pixel_edge() {
        // 0..4 x 0..2.5 px, counter-clockwise as fonts draw outer contours (y up).
        let sq = [((0.0, 0.0), (4.0, 0.0)), ((4.0, 0.0), (4.0, 2.5)), ((4.0, 2.5), (0.0, 2.5)), ((0.0, 2.5), (0.0, 0.0))];
        let (m, bmp) = coverage(&sq, 5.0, 0.0);
        assert_eq!((m.xmin, m.ymin, m.width, m.height), (0, 0, 4, 3));
        assert_eq!(&bmp[..4], &[128, 128, 128, 128], "the top row is half inside");
        assert!(bmp[4..].iter().all(|&a| a == 255), "{bmp:?}");
        // Clockwise: the same (nonzero winding), and a hole cut by an inner contour the other way round is empty.
        let cw: Vec<Line> = sq.iter().map(|&(a, b)| (b, a)).collect();
        assert_eq!(coverage(&cw, 5.0, 0.0).1, bmp);
    }

    #[test]
    fn a_slanted_edge_shades_its_pixels_by_the_area_inside() {
        // A right triangle 0..4 x 0..4 (x + y <= 4, y up): the pixels on the diagonal are half covered, those left of it full.
        let tri = [((0.0, 0.0), (4.0, 0.0)), ((4.0, 0.0), (0.0, 4.0)), ((0.0, 4.0), (0.0, 0.0))];
        let (m, bmp) = coverage(&tri, 4.0, 0.0);
        assert_eq!((m.width, m.height), (4, 4));
        let total: u32 = bmp.iter().map(|&a| a as u32).sum();
        assert!((total as f32 / 255.0 - 8.0).abs() < 0.05, "the area is 8 px: {total}");
        for row in 0..4 {
            assert_eq!(bmp[row * 4 + row], 128, "on the diagonal: {bmp:?}");
        }
    }

    #[test]
    fn bolder_paints_wider_and_taller_and_advances_further() {
        let sq = [((0.0, 0.0), (4.0, 0.0)), ((4.0, 0.0), (4.0, 4.0)), ((4.0, 4.0), (0.0, 4.0)), ((0.0, 4.0), (0.0, 0.0))];
        let (m, bmp) = coverage(&sq, 5.0, 1.0);
        assert_eq!((m.width, m.height), (5, 5));
        assert!(bmp.iter().all(|&a| a == 255));
    }

    #[test]
    fn a_missing_or_broken_font_file_is_none_and_remembered() {
        assert!(open(Path::new("/nonexistent/NotoSansCJK.otf")).is_none());
        let dir = crate::testutil::TestDir::new("not-a-font");
        let not_a_font = dir.join("NotAFont.otf");
        std::fs::write(&not_a_font, b"not a font at all").unwrap();
        assert!(open(&not_a_font).is_none());
        let _ = std::fs::remove_file(&not_a_font);
        assert!(open(&not_a_font).is_none(), "remembered: not opened again");
    }
}
