//! The game's own label from ScreenScraper ([docs/SCREENSCRAPER.md](../docs/SCREENSCRAPER.md), `Face::SupportTexture`
//! and `Face::Support`) on the cartridge, card or disc pocket-ui draws (`media::game_pic_labelled`,
//! `media::disc_pic_labelled`): so in the game box, beside the Cabinet's case and on its way into the console
//! (`insert_rig`, which maps the whole picture onto the slot in perspective). A game without one keeps the cover's art on
//! its label, as before.
//!
//! A label is cut out of its picture (the label alone, the label's place on a picture of the whole cartridge, or a
//! disc's round face) and kept in its own full colours, `media::LABEL_DETAIL` times finer than the template, so it is
//! scaled down to the screen sharp and never stretched (owner 26.11.2026: a muted, posterised label looked paler than the
//! cover's art it replaced). Made on a thread of its own ([`Labels`]), never in a frame; the frame asks, gets `None`
//! until it is there, and the cover stands in meanwhile.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use crate::image::Image;
use crate::media::{DiscKind, Game};
use crate::screenscraper::Face;

/// A picture with its alpha, 0xAARRGGBB (`image::Image` drops the alpha onto black: a disc's or a cartridge's
/// picture is cut out by it here).
pub struct Rgba {
    pub w: u32,
    pub h: u32,
    pub px: Vec<u32>,
}

/// A PNG with its alpha; None for a broken file or one larger than `image::size_ok` allows.
pub fn load_rgba(path: &Path) -> Option<Rgba> {
    let file = std::io::BufReader::new(crate::background_io::open(path).ok()?);
    let mut reader = crate::image::png_reader(file)?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).ok()?;
    let data = &buf[..info.buffer_size()];
    let argb = |r: u8, g: u8, b: u8, a: u8| (a as u32) << 24 | (r as u32) << 16 | (g as u32) << 8 | b as u32;
    let px: Vec<u32> = match info.color_type {
        png::ColorType::Rgb => data.chunks_exact(3).map(|p| argb(p[0], p[1], p[2], 255)).collect(),
        png::ColorType::Rgba => data.chunks_exact(4).map(|p| argb(p[0], p[1], p[2], p[3])).collect(),
        png::ColorType::Grayscale => data.iter().map(|g| argb(*g, *g, *g, 255)).collect(),
        png::ColorType::GrayscaleAlpha => data.chunks_exact(2).map(|p| argb(p[0], p[0], p[0], p[1])).collect(),
        png::ColorType::Indexed => return None,
    };
    (px.len() == (info.width * info.height) as usize).then_some(Rgba { w: info.width, h: info.height, px })
}

/// What a label goes on: a cartridge's or card's label (its template's size and label, template pixels), or a round
/// disc's print (its diameter in template pixels).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Target {
    Cart { size: (i32, i32), label: (i32, i32, i32, i32) },
    Disc { diameter: i32 },
}

impl Target {
    /// The label of `system`'s medium for the game file `rom`: None for one that takes no real label (a memory card, the
    /// UMD's paper label, a system pocket-ui draws no medium for).
    pub fn of(system: &str, rom: &str) -> Option<Target> {
        let g = Game::of(system, rom);
        if g.is_round() {
            return crate::media::disc_diameter(DiscKind::of(system)).map(|diameter| Target::Disc { diameter });
        }
        // The memory card has no label to find. The Mega Drive's and the DS's real labels look plainer than the cover's art
        // on their label (owner 26.11.2026, docs/DESIGN.md: a Genesis label small in the wide recess, the DS card's
        // white label with its small logo): they keep the cover's art.
        if matches!(g, Game::Memory | Game::MegaDrive | Game::Card | Game::Card3ds) {
            return None;
        }
        crate::media::label_cell(g).map(|(size, label)| Target::Cart { size, label })
    }

    /// The pictures that can give it, best first: a cartridge's flat label scan, else the picture of the whole cartridge
    /// (its label cut out where the template has it); a disc's picture (round, cut out), else its flat scan.
    pub fn faces(self) -> [Face; 2] {
        match self {
            Target::Cart { .. } => [Face::SupportTexture, Face::Support],
            Target::Disc { .. } => [Face::Support, Face::SupportTexture],
        }
    }
}

/// The part of `img` that is the thing itself: its opaque pixels. ScreenScraper's pictures come cut out (a cartridge's,
/// a disc's photo on transparency) or cropped to the label already (a label scan, its own black, white or grey edge part
/// of the label, never trimmed). (x, y, w, h); None for a picture with nothing in it.
pub fn content_box(img: &Rgba) -> Option<(u32, u32, u32, u32)> {
    let (w, h) = (img.w, img.h);
    if w == 0 || h == 0 {
        return None;
    }
    let solid = |x: u32, y: u32| img.px[(y * w + x) as usize] >> 24 >= 128;
    // A row or column counts when a few of its pixels do (a stray pixel of a cut-out's soft edge does not).
    let need_row = (w / 50).max(1);
    let need_col = (h / 50).max(1);
    let rows: Vec<bool> = (0..h).map(|y| (0..w).filter(|&x| solid(x, y)).count() as u32 >= need_row).collect();
    let cols: Vec<bool> = (0..w).map(|x| (0..h).filter(|&y| solid(x, y)).count() as u32 >= need_col).collect();
    let (y0, y1) = (rows.iter().position(|r| *r)?, rows.iter().rposition(|r| *r)?);
    let (x0, x1) = (cols.iter().position(|c| *c)?, cols.iter().rposition(|c| *c)?);
    let b = (x0 as u32, y0 as u32, (x1 - x0 + 1) as u32, (y1 - y0 + 1) as u32);
    // Nothing worth a label: a few pixels of a broken picture.
    (b.2 >= 8 && b.3 >= 8).then_some(b)
}

/// The region `(x, y, w, h)` of `img` (source pixels, it may reach past the picture: transparent there) as an opaque 0RGB
/// picture `ow` x `oh`, each pixel the mean of its part of the region, a cut-out's edge laid on `under`. Never larger than
/// asked: a huge or oddly shaped file costs no more memory than its result (review 26.11., P1: an 8192 x 512 disc picture
/// squared at full size took half a gigabyte). Each pixel reads at most `SAMPLES` x `SAMPLES` points of its part.
fn sample(img: &Rgba, (x, y, w, h): (f64, f64, f64, f64), ow: u32, oh: u32, under: u32) -> Image {
    const SAMPLES: u32 = 6;
    let (ow, oh) = (ow.max(1), oh.max(1));
    let (fx, fy) = (w / ow as f64, h / oh as f64);
    let (nx, ny) = ((fx.ceil() as u32).clamp(1, SAMPLES), (fy.ceil() as u32).clamp(1, SAMPLES));
    let mut px = Vec::with_capacity((ow * oh) as usize);
    for oy in 0..oh {
        for ox in 0..ow {
            let (mut acc, mut n) = ([0u32; 3], 0u32);
            for j in 0..ny {
                let sy = (y + (oy as f64 + (j as f64 + 0.5) / ny as f64) * fy).floor();
                for i in 0..nx {
                    let sx = (x + (ox as f64 + (i as f64 + 0.5) / nx as f64) * fx).floor();
                    let p = if sx >= 0.0 && sy >= 0.0 && (sx as u32) < img.w && (sy as u32) < img.h { img.px[(sy as u32 * img.w + sx as u32) as usize] } else { 0 };
                    let c = crate::gfx::mix(under, p & 0xffffff, p >> 24);
                    for (a, s) in acc.iter_mut().zip([16, 8, 0]) {
                        *a += c >> s & 0xff;
                    }
                    n += 1;
                }
            }
            px.push((acc[0] / n) << 16 | (acc[1] / n) << 8 | acc[2] / n);
        }
    }
    Image { w: ow, h: oh, px }
}

/// The longest side a label is cut at before it is fitted to its place: four times the largest template label, so its
/// fitting still averages, and a few hundred kilobytes at most.
const WORK_EDGE: u32 = 600;

/// The middle of `img` cut to `aspect` (width over height), as a label is printed edge to edge.
fn cut_to(img: &Image, aspect: f32) -> Image {
    let (w, h) = (img.w as f32, img.h as f32);
    let (cw, ch) = if w / h > aspect { ((h * aspect).round().max(1.0), h) } else { (w, (w / aspect).round().max(1.0)) };
    let (x0, y0) = (((w - cw) / 2.0) as u32, ((h - ch) / 2.0) as u32);
    let (cw, ch) = (cw as u32, ch as u32);
    let mut px = Vec::with_capacity((cw * ch) as usize);
    for y in y0..y0 + ch {
        px.extend_from_slice(&img.px[(y * img.w + x0) as usize..(y * img.w + x0 + cw) as usize]);
    }
    Image { w: cw, h: ch, px }
}

/// How far a label's shape may differ from its place on the template (width over height, either way) and still be
/// printed edge to edge, its middle cut to the place: a SNES label scan is a little wider than the template's. Further off
/// (a Genesis label on the Mega Drive's wide recess), it is printed whole in the middle and its own edge colour fills
/// the rest, as on a label with more paper; never stretched.
pub const CUT_UP_TO: f32 = 1.25;

/// `img` made into a label `w` x `h` (`CUT_UP_TO`).
fn fit_label(img: &Image, (w, h): (i32, i32), edge: u32) -> Image {
    let want = w as f32 / h as f32;
    let have = img.w as f32 / img.h.max(1) as f32;
    if (have / want).ln().abs() <= CUT_UP_TO.ln() {
        return cut_to(img, want).resized(w, h);
    }
    let (fw, fh) = img.fit(w, h);
    let inner = img.resized(fw.max(1), fh.max(1));
    let mut out = Image { w: w as u32, h: h as u32, px: vec![edge; (w * h) as usize] };
    let (ox, oy) = ((w - inner.w as i32) / 2, (h - inner.h as i32) / 2);
    for y in 0..inner.h as i32 {
        let at = ((oy + y) * w + ox) as usize;
        out.px[at..at + inner.w as usize].copy_from_slice(&inner.px[(y as u32 * inner.w) as usize..((y as u32 + 1) * inner.w) as usize]);
    }
    out
}

/// The colour of a label's own edge: the mean of the opaque pixels along the rim of `bx` (what fills a label's place
/// round a label of another shape, and its cut-off corners).
fn edge_colour(img: &Rgba, (x, y, w, h): (u32, u32, u32, u32)) -> u32 {
    // A little way in: a scan's very edge is often a lighter cut line (the Genesis label's).
    let (ix, iy) = ((w / 30).clamp(1, w / 2), (h / 30).clamp(1, h / 2));
    let (l, r, t, b) = (x + ix, x + w - 1 - ix, y + iy, y + h - 1 - iy);
    let ring = (l..=r).flat_map(|xx| [(xx, t), (xx, b)]).chain((t..=b).flat_map(|yy| [(l, yy), (r, yy)]));
    let (mut sum, mut n) = ([0u64; 3], 0u64);
    for (xx, yy) in ring {
        let p = img.px[(yy * img.w + xx) as usize];
        if p >> 24 >= 200 {
            for (i, s) in [16, 8, 0].into_iter().enumerate() {
                sum[i] += (p >> s & 0xff) as u64;
            }
            n += 1;
        }
    }
    if n == 0 {
        return 0x808080;
    }
    ((sum[0] / n) << 16 | (sum[1] / n) << 8 | sum[2] / n) as u32
}

/// A label ready to print, from the picture `img` of the face `face`, in its own colours: for a cartridge the label alone
/// at `LABEL_DETAIL` times the template's label size, for a disc its round face as a square of `LABEL_DETAIL` times the
/// template's diameter. None when the picture has nothing in it.
pub fn prepare(img: &Rgba, face: Face, target: Target) -> Option<Image> {
    let bx = content_box(img)?;
    let made = match target {
        Target::Cart { size: (cw, ch), label: (lx, ly, lw, lh) } => {
            let (lw, lh) = (lw.max(1), lh.max(1));
            // Made `LABEL_DETAIL` times finer than the template, so it is scaled down to the screen, sharp.
            let out = (lw * crate::media::LABEL_DETAIL, lh * crate::media::LABEL_DETAIL);
            let edge = edge_colour(img, bx);
            // A picture of the whole cartridge (ScreenScraper's support-2D): its label where the template has it. A flat
            // label scan (support-texture) is the label already.
            let (bx0, by0, bw, bh) = (bx.0 as f64, bx.1 as f64, bx.2 as f64, bx.3 as f64);
            let region = if face == Face::Support {
                let (fx, fy) = (bw / cw.max(1) as f64, bh / ch.max(1) as f64);
                (bx0 + lx as f64 * fx, by0 + ly as f64 * fy, (lw as f64 * fx).max(1.0), (lh as f64 * fy).max(1.0))
            } else {
                (bx0, by0, bw, bh)
            };
            let k = (WORK_EDGE as f64 / region.2.max(region.3)).min(1.0);
            let whole = sample(img, region, (region.2 * k).round().max(1.0) as u32, (region.3 * k).round().max(1.0) as u32, edge);
            fit_label(&whole, out, edge)
        }
        Target::Disc { diameter } => {
            // The disc's square round its middle: a disc photographed whole is round in its box; a scan cut off at a
            // side still keeps its middle there. Sampled straight at the template's diameter.
            let side = bx.2.max(bx.3) as f64;
            let (cx, cy) = (bx.0 as f64 + bx.2 as f64 / 2.0, bx.1 as f64 + bx.3 as f64 / 2.0);
            let d = (diameter.max(1) * crate::media::LABEL_DETAIL) as u32;
            sample(img, (cx - side / 2.0, cy - side / 2.0, side, side), d, d, 0x101010)
        }
    };
    Some(made)
}

/// The label file of a game, the best of `target`'s faces that is on the card.
pub fn file(data_dir: &Path, system: &str, rom: &Path, target: Target) -> Option<(PathBuf, Face)> {
    target.faces().into_iter().find_map(|face| crate::screenscraper::existing(data_dir, face, system, rom).map(|p| (p, face)))
}

/// A game without a label file is looked for again after this long (a ScreenScraper run may have brought it).
pub const MISSING_AGAIN: Duration = Duration::from_secs(30);

enum State {
    Missing(Instant),
    Loading,
    /// The label, and when it was last asked for (the least recent goes first when there are [`KEEP`]).
    Ready(Arc<Image>, Instant),
}

/// The most labels kept made (review 26.11., P2: every label rested on stayed): about 20 MB at most, a long walk
/// along a shelf makes the oldest again when it comes back.
pub const KEEP: usize = 48;

type Made = Arc<Mutex<HashMap<(String, PathBuf, Target), State>>>;

/// Called on the labels' thread when a label is made, so a frame shows it without waiting for a button.
pub type Wake = Arc<dyn Fn() + Send + Sync>;

/// Puts `state` in, the least recently asked for label going when there are more than [`KEEP`].
fn keep(made: &mut HashMap<(String, PathBuf, Target), State>, key: (String, PathBuf, Target), state: State) {
    made.insert(key, state);
    let ready = made.values().filter(|s| matches!(s, State::Ready(..))).count();
    if ready > KEEP {
        let oldest = made.iter().filter_map(|(k, s)| match s { State::Ready(_, at) => Some((k.clone(), *at)), _ => None }).min_by_key(|(_, at)| *at);
        if let Some((k, _)) = oldest {
            made.remove(&k);
        }
    }
}

/// A label to make: for which target, of which game (system, ROM).
type Job = (Target, String, PathBuf);

/// The labels made so far, by game and target, and the thread that makes them (started on the first label asked for).
pub struct Labels {
    data_dir: PathBuf,
    made: Made,
    tx: Option<mpsc::Sender<Job>>,
    wake: Option<Wake>,
    /// Made at once in the asking thread (snapshots and tests: a picture is all there in its first frame).
    pub sync: bool,
}

impl Labels {
    /// `wake` runs on the labels' thread after each label is made (the App asks for a frame).
    pub fn new(data_dir: &Path, wake: Option<Wake>) -> Labels {
        Labels { data_dir: data_dir.to_path_buf(), made: Arc::default(), tx: None, wake, sync: false }
    }

    /// The label of `system`'s game `rom` for `target`, when it is made; asks the thread for it the first time (and
    /// a game without one again after [`MISSING_AGAIN`]). Never reads or decodes in the caller's thread unless `sync`.
    pub fn get(&mut self, system: &str, rom: &Path, target: Target) -> Option<Arc<Image>> {
        let key = (system.to_string(), rom.to_path_buf(), target);
        {
            let mut made = self.made.lock().unwrap();
            match made.get_mut(&key) {
                Some(State::Ready(img, at)) => {
                    *at = Instant::now();
                    return Some(img.clone());
                }
                Some(State::Loading) => return None,
                Some(State::Missing(at)) if at.elapsed() < MISSING_AGAIN => return None,
                _ => {}
            }
        }
        if self.sync {
            let img = file(&self.data_dir, system, rom, target).and_then(|(p, face)| make(&p, face, target)).map(Arc::new);
            keep(&mut self.made.lock().unwrap(), key, img.clone().map_or(State::Missing(Instant::now()), |i| State::Ready(i, Instant::now())));
            return img;
        }
        self.made.lock().unwrap().insert(key.clone(), State::Loading);
        let tx = self.tx.get_or_insert_with(|| start(self.data_dir.clone(), self.made.clone(), self.wake.clone()));
        let _ = tx.send((target, key.0, key.1));
        None
    }

    /// Every label made again when next asked for: a ScreenScraper run brought new pictures (a label scan where only the
    /// cartridge's photo was, a label where there was none), `App::covers_changed`.
    pub fn forget(&mut self) {
        self.made.lock().unwrap().clear();
    }
}

/// A label from its file: read, cut and printed (`prepare`).
pub fn make(path: &Path, face: Face, target: Target) -> Option<Image> {
    prepare(&load_rgba(path)?, face, target)
}

fn start(data_dir: PathBuf, made: Made, wake: Option<Wake>) -> mpsc::Sender<Job> {
    let (tx, rx) = mpsc::channel::<Job>();
    let _ = std::thread::Builder::new().name("real-labels".into()).spawn(move || {
        for (target, system, rom) in rx {
            // Paused while a game runs (`background_io`): the label is made when the shelf is back.
            let img = crate::background_io::job(|| {
                file(&data_dir, &system, &rom, target).and_then(|(p, face)| make(&p, face, target))
            });
            let made_one = matches!(img, Some(Some(_)));
            let state = match img {
                Some(Some(img)) => State::Ready(Arc::new(img), Instant::now()),
                Some(None) => State::Missing(Instant::now()),
                // Cancelled: asked again on the next frame that wants it.
                None => State::Missing(Instant::now() - MISSING_AGAIN),
            };
            keep(&mut made.lock().unwrap(), (system, rom, target), state);
            if made_one {
                if let Some(wake) = &wake {
                    wake();
                }
            }
        }
    });
    tx
}

#[cfg(test)]
#[path = "real_label_tests.rs"]
mod tests;
