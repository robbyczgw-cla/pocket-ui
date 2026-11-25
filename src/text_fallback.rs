//! The second fonts of the text painter (`text.rs`): the Korean fonts pocket-ui ships (`korean`, for hangul on every
//! handheld), and the system's Noto Sans CJK, for the Japanese, Chinese and Korean
//! letters Barlow has no picture for ("ファイナルファンタジーVI", "スーパーマリオワールド"). Not ours and not shipped:
//! ROCKNIX installs it (`packages/fonts/noto-sans-cjk`, one 16 MB OpenType file with 65 535 glyphs). The virtual device has
//! it too (its root is the ROCKNIX image's), Android has its own (`OTHER_SYSTEM_CJK`). Without one the painter writes
//! kana and hangul in Latin letters and leaves the other letters out (`latinise`), never "?" or an empty box.
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

/// Where else a system keeps Noto Sans CJK (or a font of the same letters), in the order they are tried: Android's own
/// (`/system/fonts`, a collection whose first face is the Japanese one: every kana, kanji and hangul), Debian/Ubuntu,
/// Arch and Fedora. The Brick and the other systems without such a font fall back to Latin letters (`latinise`).
pub const OTHER_SYSTEM_CJK: [&str; 8] = [
    "/system/fonts/NotoSansCJK-Regular.ttc",
    "/system/fonts/NotoSansCJKjp-Regular.otf",
    "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
    "/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc",
    "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
    "/usr/share/fonts/google-noto-cjk/NotoSansCJK-Regular.ttc",
    "/usr/share/fonts/OTF/NotoSansCJK-Regular.ttc",
    "/usr/share/fonts/opentype/noto/NotoSansCJKsc-Regular.otf",
];

/// The font file to fall back to: `POCKETUI_CJK_FONT` if set (empty or `off`: none, as without the font), else
/// ROCKNIX's, else the first of `OTHER_SYSTEM_CJK` that is there. The snapshots and tests point it at the file of an
/// unpacked ROCKNIX image.
pub fn path() -> Option<PathBuf> {
    pick(std::env::var_os("POCKETUI_CJK_FONT"), &|p| p.is_file())
}

/// `path` with the environment's value and the question "is this a file" given.
fn pick(env: Option<std::ffi::OsString>, exists: &dyn Fn(&Path) -> bool) -> Option<PathBuf> {
    match env {
        Some(p) if p.is_empty() || p == "off" => None,
        Some(p) => Some(PathBuf::from(p)),
        // Nothing found: ROCKNIX's path, so that the one line in the log says where the font was expected.
        None => [SYSTEM_CJK].iter().chain(OTHER_SYSTEM_CJK.iter()).map(Path::new).find(|p| exists(p)).or(Some(Path::new(SYSTEM_CJK))).map(Path::to_path_buf),
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
    let font = load(path).map_err(|e| eprintln!("text: no fallback font for CJK ({}: {e}), those letters are written in Latin where that can be done", path.display())).ok().map(Arc::new);
    opened.push((path.to_path_buf(), font.clone()));
    font
}

/// Galmuri11 (the Retro style's) and Noto Sans KR Regular (Barlow's), cut to the 2,350 hangul syllables of KS X 1001, the
/// compatibility jamo and a few marks (`scripts/korean-fonts.py`, 474 and 265 KB): the Korean letters every handheld
/// paints, with a system CJK font or without (the Brick, the Android app). Read like the system's font (`raster`, from
/// the binary's own pages), never through fontdue.
const KOREAN_PIXEL: &[u8] = include_bytes!("../assets/fonts/Galmuri11-KSX1001.ttf");
const KOREAN_SANS: &[u8] = include_bytes!("../assets/fonts/NotoSansKR-Regular-KSX1001.otf");
/// Galmuri11's pixel in its units: 1200 to the em, a syllable 11 pixels high on the baseline.
pub const KOREAN_PIXEL_UNITS: f32 = 100.0;
/// How many of Galmuri11's pixels a syllable stands high.
pub const KOREAN_PIXEL_ROWS: i32 = 11;

/// Whether `ch` is Korean (a syllable, a jamo of any of the blocks): the letters the shipped Korean fonts paint before the
/// system's CJK font is asked (`Text::glyph`), so that hangul looks the same on every handheld and in every style.
/// Later Japanese and Chinese fonts of our own go the same way, by block.
pub fn is_korean(ch: char) -> bool {
    matches!(ch as u32, 0xAC00..=0xD7A3 | 0x1100..=0x11FF | 0x3130..=0x318F | 0xA960..=0xA97F | 0xD7B0..=0xD7FF)
}

/// The shipped Korean font for a pixel face (`pixel`: Galmuri11) or any other (Noto Sans KR), parsed once per process.
pub fn korean(pixel: bool) -> &'static Fallback {
    static PIXEL: std::sync::OnceLock<Fallback> = std::sync::OnceLock::new();
    static SANS: std::sync::OnceLock<Fallback> = std::sync::OnceLock::new();
    let (slot, bytes) = if pixel { (&PIXEL, KOREAN_PIXEL) } else { (&SANS, KOREAN_SANS) };
    slot.get_or_init(|| parse(bytes).expect("embedded Korean font"))
}

/// Whether the shipped Korean fonts paint `ch` (both have the same letters).
pub fn korean_has(ch: char) -> bool {
    is_korean(ch) && korean(false).has(ch)
}

/// The Chinese fonts pocket-ui ships for the Retro style's pixel faces (Fusion Pixel 12px, Simplified Chinese forms) and
/// any other (Noto Sans SC Regular), cut to the 6,763 hanzi of GB 2312, the 6,355 kanji of JIS X 0208, the kana, CJK
/// punctuation and the full-width forms (`scripts/cjk-fonts.py`, 1.5 and 2.3 MB). Too big to be compiled into each of
/// the three programs like the Korean ones: files next to the programs (`shipped_dir`), mapped like the system's font.
/// They paint the Chinese interface and every ideograph and kana of a game's name, on every handheld alike: the same
/// Noto Sans letters ROCKNIX's own font has, and pixel letters in the Retro style.
pub const CJK_PIXEL_FILE: &str = "FusionPixel12-zh_hans-CJK.otf";
pub const CJK_SANS_FILE: &str = "NotoSansSC-Regular-CJK.otf";
/// Fusion Pixel's ideographs stand one of its pixels below the baseline (Galmuri's on it): how many.
pub const CJK_PIXEL_DROP: i32 = 1;

/// The folder of pocket-ui's own font files (`CJK_PIXEL_FILE`, `CJK_SANS_FILE`): `POCKETUI_FONTS_DIR` if set (empty or
/// `off`: none, as on a card without them), else `fonts/` next to the program (a release's folder) or one folder up (the
/// Brick's `app/`, its programs in `app/bin`), else on Android `fonts/` in the app's own files (copied there from the APK), else the source tree's `assets/fonts`
/// (builds on the computer: tests, snapshots, the virtual device).
pub fn shipped_dir() -> Option<PathBuf> {
    static DIR: std::sync::OnceLock<Option<PathBuf>> = std::sync::OnceLock::new();
    DIR.get_or_init(|| {
        let bin = std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf));
        let exe = bin.as_ref().map(|d| d.join("fonts"));
        // The Brick's programs are in app/bin, its files in app/.
        let app = bin.as_ref().and_then(|d| d.parent()).map(|d| d.join("fonts"));
        #[cfg(target_os = "android")]
        let android = Some(crate::platform::android::data_dir().join("fonts"));
        #[cfg(not(target_os = "android"))]
        let android = None;
        // Cargo's folder when it runs this (a test, `cargo run`, nextest; a test binary reused from another copy's build
        // folder would point into a folder that is gone), else, in a debug build only, where it was built: a release
        // program carries no build path (scripts/check-binary-paths.sh).
        #[cfg(debug_assertions)]
        let built = Some(PathBuf::from(env!("CARGO_MANIFEST_DIR")));
        #[cfg(not(debug_assertions))]
        let built: Option<PathBuf> = None;
        let source = std::env::var_os("CARGO_MANIFEST_DIR").map(PathBuf::from).or(built).map(|d| d.join("assets/fonts"));
        pick_dir(std::env::var_os("POCKETUI_FONTS_DIR"), [exe, app, android, source], &|d| d.join(CJK_SANS_FILE).is_file())
    })
    .clone()
}

/// `shipped_dir` with the environment's value, the places in order and the question "are the fonts there" given.
fn pick_dir(env: Option<std::ffi::OsString>, places: [Option<PathBuf>; 4], has_fonts: &dyn Fn(&Path) -> bool) -> Option<PathBuf> {
    match env {
        Some(p) if p.is_empty() || p == "off" => None,
        Some(p) => Some(PathBuf::from(p)),
        None => places.into_iter().flatten().find(|d| has_fonts(d)),
    }
}

/// Whether `ch` is one of the letters the shipped Chinese fonts are asked for before the system's (`Text::glyph`): an
/// ideograph (the unified block and extension A, the compatibility ones), a kana, CJK punctuation or a full-width form.
pub fn is_cjk(ch: char) -> bool {
    matches!(ch as u32, 0x4E00..=0x9FFF | 0x3400..=0x4DBF | 0xF900..=0xFAFF | 0x3000..=0x30FF | 0x31F0..=0x31FF | 0xFF01..=0xFF60 | 0xFFE0..=0xFFE6)
}

/// The shipped Chinese font for a pixel face (`pixel`: Fusion Pixel) or any other (Noto Sans SC), opened once per process;
/// `None` when its file is not on the card.
pub fn cjk(pixel: bool) -> Option<Arc<Fallback>> {
    let dir = shipped_dir()?;
    open(&dir.join(if pixel { CJK_PIXEL_FILE } else { CJK_SANS_FILE }))
}

/// Whether the shipped Chinese fonts paint `ch` (Noto Sans SC has every letter of the pixel font and 36 rare hanzi more:
/// those come from it in the Retro style too).
pub fn cjk_has(ch: char) -> bool {
    is_cjk(ch) && cjk(false).is_some_and(|f| f.has(ch))
}

/// Whether the shipped Chinese fonts are there: Chinese is offered only then (`i18n::Lang::offered`), never as lines
/// with nothing to paint them.
pub fn cjk_available() -> bool {
    cjk(false).is_some() && cjk(true).is_some()
}

fn parse(data: &'static [u8]) -> Result<Fallback, String> {
    let face = Face::parse(data, 0).map_err(|e| e.to_string())?;
    Ok(Fallback { unit: 1.0 / face.units_per_em() as f32, face })
}

fn load(path: &Path) -> Result<Fallback, String> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    // SAFETY: the file is a read-only system font (squashfs on the device) or one of pocket-ui's own (`shipped_dir`),
    // which an update never rewrites in place (a new release's folder, or the Brick's file renamed whole); nothing writes
    // to it while it is mapped.
    // The mapping lives as long as the process (once per file, `open`), so the parsed face may borrow it forever.
    let map = unsafe { memmap2::Mmap::map(&file) }.map_err(|e| e.to_string())?;
    let data: &'static [u8] = Box::leak(Box::new(map));
    parse(data)
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

    /// `ch` of a pixel font (Galmuri11, `KOREAN_PIXEL_UNITS` units to its pixel) with each of its pixels `n` canvas
    /// pixels and its bottom `rise` canvas pixels over the baseline: whole pixels, in or out, no grey edge (as
    /// `text::raster` paints Departure Mono and Pixelify).
    pub fn raster_pixels(&self, ch: char, n: u32, rise: i32) -> Option<(Metrics, Vec<u8>)> {
        let id = self.face.glyph_index(ch).filter(|g| g.0 != 0)?;
        let k = n as f32 / KOREAN_PIXEL_UNITS;
        let advance = self.face.glyph_hor_advance(id).unwrap_or(0) as f32 * k;
        let mut path = Flatten { k, rise: rise as f32, tol: 0.2, lines: Vec::new(), start: (0.0, 0.0), at: (0.0, 0.0) };
        if self.face.outline_glyph(id, &mut path).is_none() || path.lines.is_empty() {
            return Some((Metrics { advance_width: advance, ..Metrics::default() }, Vec::new()));
        }
        // Galmuri's outlines run along its pixels: on whole canvas pixels, without the float's 99.000001 that made a
        // syllable of 99 rows a bitmap of 100.
        let snap = |(x, y): (f32, f32)| (x.round(), y.round());
        let lines: Vec<Line> = path.lines.iter().map(|&(a, b)| (snap(a), snap(b))).collect();
        let (metrics, mut bitmap) = coverage(&lines, advance.round(), 0.0);
        for v in &mut bitmap {
            *v = if *v >= 128 { 255 } else { 0 };
        }
        Some((metrics, bitmap))
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

    #[test]
    fn the_shipped_fonts_are_found_next_to_the_program_then_one_up_then_on_android_then_in_the_source_tree() {
        let (exe, app, source) = (PathBuf::from("/storage/pocket-ui/current/fonts"), PathBuf::from("/mnt/SDCARD/pocket-ui/app/fonts"), PathBuf::from("/src/assets/fonts"));
        let android = PathBuf::from("/data/x/files/fonts");
        let places = || [Some(exe.clone()), Some(app.clone()), Some(android.clone()), Some(source.clone())];
        assert_eq!(pick_dir(None, places(), &|_| true), Some(exe.clone()));
        assert_eq!(pick_dir(None, places(), &|d| d != exe), Some(app.clone()), "the Brick's app/");
        assert_eq!(pick_dir(None, places(), &|d| d == android || d == source), Some(android.clone()));
        assert_eq!(pick_dir(None, places(), &|d| d == source), Some(source.clone()));
        assert_eq!(pick_dir(None, places(), &|_| false), None, "a card without them: none");
        assert_eq!(pick_dir(Some("off".into()), places(), &|_| true), None);
        assert_eq!(pick_dir(Some("".into()), places(), &|_| true), None);
        assert_eq!(pick_dir(Some("/x".into()), places(), &|_| false), Some(PathBuf::from("/x")));
    }

    #[test]
    fn the_shipped_chinese_fonts_paint_the_interface_and_japanese_names_and_leave_hangul_to_the_korean_ones() {
        assert!(cjk_available(), "assets/fonts has them");
        for ch in "设置继续游戏存档。，：（）「」ファイナルファンタジーぁ国國".chars() {
            assert!(cjk_has(ch), "{ch}");
        }
        for ch in "가힣Aé€".chars() {
            assert!(!cjk_has(ch), "{ch}");
        }
        let pixel = cjk(true).unwrap();
        // GB 2312 and JIS X 0208 whole in the sans; the pixel font lacks a few rare ones, which come from the sans.
        assert!("设置继续游戏存档国ア".chars().all(|c| pixel.has(c)));
    }

    /// Every letter of the Chinese tables is one both shipped Chinese fonts paint (GB 2312's hanzi and the marks; the
    /// pixel font lacks 36 rare hanzi) or one the faces have: a translator's hanzi outside them would come from another
    /// font or not at all. Trivially true while `zh.tsv` is empty.
    #[test]
    fn every_letter_of_the_chinese_tables_is_in_both_shipped_fonts() {
        let (pixel, sans) = (cjk(true).unwrap(), cjk(false).unwrap());
        let mut missing = std::collections::BTreeSet::new();
        for src in [include_str!("../assets/i18n/zh.tsv"), include_str!("../assets/i18n/brick-zh.tsv")] {
            for line in src.lines().filter(|l| !l.starts_with('#')) {
                let Some((_, zh)) = line.split_once('\t') else { continue };
                for ch in zh.chars().filter(|c| is_cjk(*c) && *c != '\u{3000}') {
                    if !pixel.has(ch) || !sans.has(ch) {
                        missing.insert(ch);
                    }
                }
            }
        }
        assert!(missing.is_empty(), "letters the shipped Chinese fonts lack (choose another word): {missing:?}");
    }

    #[test]
    fn the_font_is_found_where_the_system_keeps_it() {
        let rocknix = Path::new(SYSTEM_CJK);
        let android = Path::new("/system/fonts/NotoSansCJK-Regular.ttc");
        let debian = Path::new("/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc");
        let only = |p: &'static Path| move |q: &Path| q == p;
        // ROCKNIX first, then Android's, then the desktop's; nothing found: ROCKNIX's path (the log says where it was expected).
        assert_eq!(pick(None, &|q| q == rocknix || q == android), Some(rocknix.to_path_buf()));
        assert_eq!(pick(None, &only(android)), Some(android.to_path_buf()));
        assert_eq!(pick(None, &only(debian)), Some(debian.to_path_buf()));
        assert_eq!(pick(None, &|_| false), Some(rocknix.to_path_buf()));
        // The environment decides before any of that.
        assert_eq!(pick(Some("off".into()), &|_| true), None);
        assert_eq!(pick(Some("".into()), &|_| true), None);
        assert_eq!(pick(Some("/x/font.otf".into()), &|_| false), Some(PathBuf::from("/x/font.otf")));
    }
}
