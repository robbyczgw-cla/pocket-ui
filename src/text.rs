//! Anti-aliased text with the embedded Barlow fonts (or a store theme's, see `fonts_dir`),
//! rasterised once per glyph and size. Every size a painter asks for goes through Settings → Text size
//! (`crate::type_scale::px`) first, so the cache holds each glyph at the size it is painted at, Normal and Large side
//! by side: switching the size draws the new size's glyphs once, nothing is thrown away or drawn again.
//!
//! A letter Barlow lacks (a Japanese name, "ファイナルファンタジーVI") comes from the system's Noto Sans CJK
//! (`text_fallback`), fitted to Barlow's capitals and weight, and is kept apart in a cache of its own of at most
//! `FALLBACK_BYTES`. Without that font it is painted as "?" (a letter) or left out (a mark), as before.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{mpsc, Arc};

use fontdue::{Font, FontSettings, Metrics};

use crate::gfx::Canvas;
use crate::font_style::{self, FontStyle, RetroLetters};
use crate::text_fallback::{Fallback, Fit};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Face {
    /// Barlow Condensed SemiBold: titles on spines and labels.
    Title,
    /// Barlow Medium: meta text and hints.
    Body,
    /// Barlow Condensed SemiBold (or a store theme's title face) whatever Theme → Font says: a spine's last try in a
    /// wider font before its title is cut (`media::spine_fit`).
    Narrow,
}

impl Face {
    /// Which of the two fitted faces' slots (`Text::scale`, the fallback's fits) a face goes by: Narrow is a title.
    fn slot(self) -> usize {
        match self {
            Face::Body => 1,
            Face::Title | Face::Narrow => 0,
        }
    }
}

type Glyph = (Metrics, Vec<u8>);
type Key = (Face, u32, char);

pub struct Text {
    title: Arc<Font>,
    body: Arc<Font>,
    /// How much larger each face is painted than the size asked for (`font_style`: an alternative face fitted to
    /// Barlow's capitals); 1.0 for Barlow and a theme's own faces.
    scale: [f32; 2],
    /// A pixel face's pixel in ems for each (`font_style::Alt::grid`), 0 for the others: `raster` paints it crisp.
    grid: [f32; 2],
    /// Whether each face's digits and look-alike capitals come from Retro's LCD set (`font_style::Alt::lcd`).
    lcd: [bool; 2],
    /// Whether each face's letters stand a font pixel closer and its pixels are whole screen pixels
    /// (`font_style::Alt::tight`).
    tight: [bool; 2],
    /// Canvas pixels per screen pixel the glyphs were drawn for (`scaled_layout::step`), followed as the style is.
    step: u32,
    /// The faces of Barlow, or of the store theme that replaced them (`new`): what a style that leaves a face alone
    /// (Pixel's body) or the Barlow style itself uses, and `themed`: which of the two came from the store, which no
    /// style replaces.
    base: [Arc<Font>; 2],
    themed: [bool; 2],
    /// The style `title` and `body` are in now, and the one this Text is held to whatever the setting says
    /// (`pinned`: safe mode, the tests), else `font_style::get()` is followed (`sync`).
    style: FontStyle,
    /// Retro's letters the faces are in (`font_style::retro_letters`), followed as the style is.
    letters: RetroLetters,
    pinned: Option<FontStyle>,
    /// `prewarm` has been asked for: asked again when the style changes.
    prewarmed: bool,
    cache: HashMap<Key, Glyph>,
    /// Which glyphs of `cache` came from the fallback font (`text_fallback`): a library of Japanese names can ask for
    /// thousands, so they are dropped once they pass `FALLBACK_BYTES` (and drawn again as they are needed).
    more: Vec<Key>,
    /// The bytes they take.
    more_bytes: usize,
    /// The fallback font's file (`text_fallback::path`), opened the first time a letter needs it.
    fallback_path: Option<PathBuf>,
    /// Not tried yet; tried: the font and its fit to the title face and the body face, or `None` without one.
    fallback: Option<Option<(Arc<Fallback>, [Fit; 2])>>,
    /// The glyphs a thread draws ahead of time (`prewarm`), taken in at the first glyph the cache lacks.
    warm: Option<mpsc::Receiver<Vec<(Key, Glyph)>>>,
    /// The same for the library's names that need the fallback font (`prewarm_names`).
    warm_names: Option<mpsc::Receiver<Vec<(Key, Glyph)>>>,
    /// How many glyphs were drawn on the spot, in a frame (`drawn_on_the_spot`).
    drawn: usize,
    /// The interface's language (`crate::i18n`): every public method below paints, measures, fits and wraps the
    /// translation of the string it is given, so the callers keep their English strings.
    tr: crate::i18n::Cache,
    /// Off while the manual's English pages are painted (`set_translate`).
    raw: bool,
    /// A literal game/system name also bypasses device-specific wording substitutions.
    naming: bool,
    /// For the snapshot checks only (`i18n::recording`): the first pieces of words `wrap` broke inside, so that one that
    /// is painted is noted (`i18n::note_broken`); a wrap only measured to try a size is not.
    broken: std::collections::HashSet<String>,
}

impl Default for Text {
    fn default() -> Self {
        Self::new()
    }
}

/// What `Text::prewarm` draws: the sizes of the body type (the status corner 28 and 30, a row 36, a heading 46, the
/// pause menu 21) and of the titles (50), for ASCII and `PREWARM_MORE`.
const PREWARM: [(Face, &[u32]); 2] = [(Face::Body, &[21, 28, 30, 36, 46]), (Face::Title, &[50])];
/// The letters of German and the marks of Settings' values besides ASCII.
const PREWARM_MORE: &str = "ÄÖÜäöüßáéíóúñÁÉÍÓÚÑ¿¡“”…‹›·–×°→\u{200b}";
/// The sizes games' names are painted at (the rows of a list and of search 37, the card's title 50 and 67, a cover's
/// caption 44, the hints and meta lines 28 and 30), at which `prewarm_names` draws their CJK letters ahead.
const PREWARM_NAMES: [(Face, &[u32]); 2] = [(Face::Title, &[37, 44, 50, 67]), (Face::Body, &[28, 30])];
/// At most this many different letters of the names are drawn ahead (the most frequent first).
const PREWARM_NAMES_MAX: usize = 1200;
/// At most this many bytes of fallback glyphs are kept (`Text::more`): about 3000 Japanese letters at 37 to 67 px.
pub const FALLBACK_BYTES: usize = 8 << 20;
/// How much bolder the fallback (Noto Sans CJK Regular) is painted next to each face, in ems: Barlow Condensed
/// SemiBold's titles, Barlow Medium's body text.
const FALLBACK_BOLD: [f32; 2] = [0.04, 0.018];

/// Type from the store (a theme package): `title.ttf` and `body.ttf` in here replace the embedded
/// faces. `POCKETUI_DATA` moves it with the rest of pocket-ui's data.
pub fn fonts_dir() -> std::path::PathBuf {
    std::path::PathBuf::from(std::env::var("POCKETUI_DATA").unwrap_or_else(|_| "/storage/.config/pocket-ui".into())).join("fonts")
}

/// A glyph as `Text` paints it. A character the font has no picture for would come out as an empty box ("Ünïcödé ▯
/// Spiel", first-run test 2026-10-24, finding #9): a letter or digit (a Japanese title) is painted as "?" so that the
/// name still shows something, anything else (a tick, a star, an emoji, a zero-width mark) is left out. Decided once
/// per character and size, when the glyph enters the cache: nothing per frame.
#[allow(clippy::too_many_arguments)]
fn raster(font: &Font, scale: f32, grid: f32, lcd: bool, tight: Option<u32>, plain: &Font, ch: char, px: u32) -> Glyph {
    // Retro's digits in Pixelify's letters (`retro::Digits`): Barlow's as they are, or the LCD set on the pixel face's
    // grid; its "B" and "Z" from that set whatever the digits are (`retro::lcd_letter`). Departure Mono's are its own.
    if grid > 0.0 && lcd && ch.is_ascii_digit() {
        match crate::retro::digits() {
            crate::retro::Digits::Plain => return raster(plain, 1.0, 0.0, false, None, plain, ch, px),
            crate::retro::Digits::Lcd => {
                let rows = &crate::retro::LCD[(ch as u8 - b'0') as usize];
                return lcd_glyph(font, on_grid(px as f32 * scale, grid), grid, ch, rows, if ch == '1' { 3 } else { 5 });
            }
            crate::retro::Digits::Pixelify => {}
        }
    }
    if let Some(rows) = crate::retro::lcd_letter(ch).filter(|_| grid > 0.0 && lcd) {
        return lcd_glyph(font, on_grid(px as f32 * scale, grid), grid, ch, rows, 5);
    }
    // `scale` fits an alternative face to Barlow's capitals (1.0 for Barlow: the size as it was).
    let px = if scale == 1.0 { px as f32 } else { px as f32 * scale };
    let px = match tight {
        Some(step) => on_screen_grid(px, grid, step),
        None => on_grid(px, grid),
    };
    let ch = if font.lookup_glyph_index(ch) != 0 || ch == '?' {
        ch
    } else if ch.is_alphanumeric() {
        '?'
    } else {
        return (Metrics::default(), Vec::new());
    };
    let (mut metrics, mut bitmap) = font.rasterize(ch, px);
    // A pixel face: every screen pixel in or out, as the font's own pixels are (`on_grid`), and each letter a whole
    // number of font pixels on from the last, so the gaps between letters are even ("ha t" in "hat" otherwise).
    if grid > 0.0 {
        for v in &mut bitmap {
            *v = if *v >= 128 { 255 } else { 0 };
        }
        metrics.advance_width = whole_pixels(metrics.advance_width, px, grid);
        // One font pixel closer where the ink ends a whole font pixel before the next letter (`Alt::tight`).
        let n = (px * grid).round().max(1.0);
        if tight.is_some() && !font_style::departure_wide() && (metrics.xmin + metrics.width as i32) as f32 <= metrics.advance_width - n {
            metrics.advance_width -= n;
        }
    }
    (metrics, bitmap)
}

/// `ch` drawn from `rows` (seven rows, `cols` pixels across from bit 4; `retro::LCD`) for a pixel face painted at `px`
/// (on its grid of `grid` ems): as wide, as far in and as far apart as the face's own `ch`, its seven rows the face's
/// capitals' height.
fn lcd_glyph(font: &Font, px: f32, grid: f32, ch: char, rows: &[u8; 7], cols: usize) -> Glyph {
    let own = font.metrics(ch, px);
    let n = (px * grid).round().max(1.0) as usize;
    let (w, h) = (cols * n, 7 * n);
    let mut bitmap = vec![0u8; w * h];
    for (r, bits) in rows.iter().enumerate() {
        for col in 0..cols {
            if bits & (0b10000 >> col) != 0 {
                for y in r * n..(r + 1) * n {
                    bitmap[y * w + col * n..y * w + (col + 1) * n].fill(255);
                }
            }
        }
    }
    let xmin = own.bounds.xmin.round() as i32;
    let metrics = Metrics { xmin, ymin: 0, width: w, height: h, advance_width: whole_pixels(own.advance_width, px, grid), ..own };
    (metrics, bitmap)
}

/// `advance` of a pixel face painted at `px` (its pixel `grid` ems) in whole font pixels.
fn whole_pixels(advance: f32, px: f32, grid: f32) -> f32 {
    let n = (px * grid).round().max(1.0);
    (advance / n).round() * n
}

/// The size `px` of a pixel face whose pixel is `grid` ems (`font_style::Alt::grid`; 0: any other face, `px` as it
/// is) rounded to whole screen pixels per font pixel, at least two: Pixelify between two such sizes paints strokes of
/// two and three pixels with grey edges.
fn on_grid(px: f32, grid: f32) -> f32 {
    if grid <= 0.0 {
        return px;
    }
    (px * grid).round().max(2.0) / grid
}

/// `on_grid` for a face whose pixels must be whole pixels of the screen (`font_style::Alt::tight`): with `step` canvas
/// pixels to one of the screen's (`scaled_layout::step`), the canvas pixels per font pixel are a multiple of `step`, at
/// least two, and halfway between two the smaller (Departure Mono's three canvas pixels came out as one and a half on a
/// 640x480 screen: uneven strokes, grey edges).
fn on_screen_grid(px: f32, grid: f32, step: u32) -> f32 {
    let step = step.max(1) as f32;
    let screen = (px * grid / step - 0.5).ceil().max(1.0);
    (screen * step).max(2.0) / grid
}

/// `ch` from the fallback font in `path`, fitted to `face`; opens the font the first time (`text_fallback::open`, the
/// state in `fallback`). `None` when there is none or it lacks `ch` too: then the face's "?" or nothing (`raster`).
#[allow(clippy::too_many_arguments)]
fn fallback_glyph(fallback: &mut Option<Option<(Arc<Fallback>, [Fit; 2])>>, path: Option<&std::path::Path>, title: &Font, body: &Font, scale: [f32; 2], face: Face, px: u32, ch: char) -> Option<Glyph> {
    let (font, fits) = fallback
        .get_or_insert_with(|| {
            let font = path.and_then(crate::text_fallback::open)?;
            let fits = fits(&font, title, body, scale);
            Some((font, fits))
        })
        .as_ref()?;
    font.raster(ch, px, fits[face.slot()])
}

/// The memory a fallback glyph takes in the cache (`Text::more_bytes`).
fn glyph_bytes(glyph: &Glyph) -> usize {
    glyph.1.len() + std::mem::size_of::<(Key, Glyph)>()
}

/// The fit of the fallback font to the title face and the body face (`Fit::to`): its ideographs as high as their
/// capitals and a little more, centred on them, as bold as they are.
fn fits(fallback: &Fallback, title: &Font, body: &Font, scale: [f32; 2]) -> [Fit; 2] {
    // The capitals as painted: an alternative face is scaled to Barlow's (`font_style`).
    let fit = |font: &Font, scale: f32, bold: f32| {
        let b = font.metrics('H', 1000.0).bounds;
        Fit::to(fallback, b.height / 1000.0 * scale, b.ymin / 1000.0 * scale, bold)
    };
    [fit(title, scale[0], FALLBACK_BOLD[0]), fit(body, scale[1], FALLBACK_BOLD[1])]
}

/// The letters of `names` that a face lacks (a Japanese name's), the most frequent first, at most `max`.
fn letters_for_fallback<'a>(title: &Font, body: &Font, names: impl Iterator<Item = &'a str>, max: usize) -> Vec<char> {
    let mut count: HashMap<char, u32> = HashMap::new();
    let mut known: HashSet<char> = HashSet::new();
    for ch in names.flat_map(str::chars) {
        if ch.is_ascii() || ch.is_whitespace() || known.contains(&ch) {
            continue;
        }
        if let Some(n) = count.get_mut(&ch) {
            *n += 1;
        } else if title.lookup_glyph_index(ch) == 0 || body.lookup_glyph_index(ch) == 0 {
            count.insert(ch, 1);
        } else {
            known.insert(ch);
        }
    }
    let mut letters: Vec<(char, u32)> = count.into_iter().collect();
    letters.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    letters.into_iter().take(max).map(|(ch, _)| ch).collect()
}

impl Text {
    fn with_faces(title: Font, body: Font, themed: [bool; 2], fallback_path: Option<PathBuf>) -> Self {
        let (title, body) = (Arc::new(title), Arc::new(body));
        Self {
            base: [title.clone(), body.clone()],
            title,
            body,
            scale: [1.0; 2],
            grid: [0.0; 2],
            lcd: [false; 2],
            tight: [false; 2],
            step: 1,
            themed,
            style: FontStyle::Barlow,
            letters: RetroLetters::Departure,
            pinned: None,
            prewarmed: false,
            cache: HashMap::new(),
            more: Vec::new(),
            more_bytes: 0,
            fallback_path,
            fallback: None,
            warm: None,
            warm_names: None,
            drawn: 0,
            tr: Default::default(),
            raw: false,
            naming: false,
            broken: Default::default(),
        }
    }

    /// The embedded Barlow faces only, never a theme's nor the system's CJK font (safe mode, `rescue`).
    pub fn builtin() -> Self {
        let load = |bytes: &[u8]| Font::from_bytes(bytes, FontSettings::default()).expect("embedded font");
        Self::with_faces(load(font_style::BARLOW_TITLE), load(font_style::BARLOW_BODY), [false; 2], None).pinned_to(FontStyle::Barlow)
    }

    /// Follows Settings → Look → Style again (`builtin` holds Barlow): the Android launcher and player, which have the
    /// embedded faces only but a Font row of their own.
    pub fn following_font(mut self) -> Self {
        self.pinned = None;
        self.sync();
        self
    }

    /// Held to `style` whatever Settings → Look → Style says: the tests, and safe mode (`builtin`).
    pub fn pinned_to(mut self, style: FontStyle) -> Self {
        self.pinned = Some(style);
        self.sync();
        self
    }

    /// The style this Text paints in now.
    pub fn style(&self) -> FontStyle {
        self.style
    }

    /// Follows the style in use (`font_style::get`; Settings → Look → Style): one atomic read, and only when it
    /// changed the faces are swapped and everything drawn with the old ones dropped. The faces of a style are parsed
    /// the first time it is used (`FontStyle::fitted`), not here per frame. A store theme's faces stay.
    fn sync(&mut self) {
        let want = self.pinned.unwrap_or_else(font_style::get);
        // Retro's letters matter only in Retro: another style keeps its faces when they change.
        let letters = if want == FontStyle::Retro { font_style::retro_letters() } else { self.letters };
        // A face sized in screen pixels (Departure Mono) is drawn anew when the canvas is shown at another scale.
        let step = if self.tight.iter().any(|t| *t) || want == FontStyle::Retro { crate::scaled_layout::step() } else { self.step };
        if want == self.style && letters == self.letters && step == self.step {
            return;
        }
        self.style = want;
        self.letters = letters;
        self.step = step;
        let alt = want.fitted();
        let pick = |i: usize, alt: Option<&font_style::Alt>| match alt {
            Some(a) if !self.themed[i] => (a.font.clone(), a.scale, a.grid, a.lcd, a.tight),
            _ => (self.base[i].clone(), 1.0, 0.0, false, false),
        };
        let (title, body) = (pick(0, alt.and_then(|a| a.title.as_ref())), pick(1, alt.and_then(|a| a.body.as_ref())));
        (self.title, self.scale[0], self.grid[0], self.lcd[0], self.tight[0]) = title;
        (self.body, self.scale[1], self.grid[1], self.lcd[1], self.tight[1]) = body;
        // Glyphs, the fallback's fit to the capitals and what the threads were drawing ahead are of the old faces.
        self.cache.clear();
        self.more.clear();
        self.more_bytes = 0;
        self.fallback = None;
        self.warm = None;
        self.warm_names = None;
        if self.prewarmed {
            self.prewarm();
        }
    }

    /// The embedded Barlow faces, each replaced by an installed one from `fonts_dir()` if it loads; the system's CJK
    /// font behind them (`text_fallback::path`, opened when a letter first needs it).
    pub fn new() -> Self {
        let load = |bytes: &[u8]| Font::from_bytes(bytes, FontSettings::default()).expect("embedded font");
        let installed = |name: &str| {
            let bytes = std::fs::read(fonts_dir().join(name)).ok()?;
            Font::from_bytes(bytes, FontSettings::default()).map_err(|e| eprintln!("fonts/{name}: {e}, using the built-in one")).ok()
        };
        let (title, body) = (installed("title.ttf"), installed("body.ttf"));
        let themed = [title.is_some(), body.is_some()];
        let mut text = Self::with_faces(
            title.unwrap_or_else(|| load(font_style::BARLOW_TITLE)),
            body.unwrap_or_else(|| load(font_style::BARLOW_BODY)),
            themed,
            crate::text_fallback::path(),
        );
        text.sync();
        text
    }

    /// The same faces with `path` as the fallback font instead (`None`: none): for the tests.
    pub fn with_fallback(mut self, path: Option<PathBuf>) -> Self {
        self.fallback_path = path;
        self.fallback = None;
        self.cache.clear();
        self.more.clear();
        self.more_bytes = 0;
        self
    }

    fn glyph(&mut self, face: Face, px: u32, ch: char) -> &Glyph {
        self.sync();
        let key = (face, px, ch);
        // The fallback glyphs passed their share (with the one drawn last): they go, and are drawn again as needed.
        if self.more_bytes > FALLBACK_BYTES {
            self.drop_fallback_glyphs();
        }
        // A glyph the cache lacks: first see whether the threads of `prewarm` and `prewarm_names` have drawn it already.
        if (self.warm.is_some() || self.warm_names.is_some()) && !self.cache.contains_key(&key) {
            self.take_warm();
        }
        // One lookup for a glyph in the cache, as before the fallback font; the fields apart for the one that isn't.
        let Text { cache, title, body, scale, grid, lcd, tight, step, base, fallback_path, fallback, more, more_bytes, drawn, .. } = self;
        cache.entry(key).or_insert_with(|| {
            *drawn += 1;
            let (font, k, g, l, n) = match face {
                Face::Title => (&**title, scale[0], grid[0], lcd[0], tight[0].then_some(*step)),
                Face::Body => (&**body, scale[1], grid[1], lcd[1], tight[1].then_some(*step)),
                Face::Narrow => (&*base[0], 1.0, 0.0, false, None),
            };
            // The PlayStation-style shapes of the hints (`button_prompts`): drawn, at the face's cap height.
            if let Some(glyph) = crate::button_prompts::raster(ch, px, font.metrics('H', px as f32 * k).height as f32) {
                return glyph;
            }
            // A letter the face lacks: the fallback font's, if it has one.
            if font.lookup_glyph_index(ch) == 0 && ch != '?' {
                if let Some(glyph) = fallback_glyph(fallback, fallback_path.as_deref(), title, body, *scale, face, px, ch) {
                    *more_bytes += glyph_bytes(&glyph);
                    more.push(key);
                    return glyph;
                }
            }
            raster(font, k, g, l, n, &base[face.slot()], ch, px)
        })
    }

    fn face(&self, face: Face) -> &Font {
        match face {
            Face::Title => &self.title,
            Face::Body => &self.body,
            Face::Narrow => &self.base[0],
        }
    }

    /// Drops the fallback glyphs from the cache.
    fn drop_fallback_glyphs(&mut self) {
        for key in self.more.drain(..) {
            self.cache.remove(&key);
        }
        self.more_bytes = 0;
    }

    /// How many bytes the fallback glyphs take (`FALLBACK_BYTES` and at most one glyph more), and how many there are.
    pub fn fallback_cache(&self) -> (usize, usize) {
        (self.more_bytes, self.more.len())
    }

    /// Takes in what the `prewarm` and `prewarm_names` threads have drawn, if they are done; the cache keeps what it
    /// has.
    fn take_warm(&mut self) {
        if let Some(rx) = &self.warm {
            match rx.try_recv() {
                Ok(glyphs) => {
                    self.cache.reserve(glyphs.len());
                    for (key, glyph) in glyphs {
                        self.cache.entry(key).or_insert(glyph);
                    }
                    self.warm = None;
                }
                Err(mpsc::TryRecvError::Empty) => {}
                Err(mpsc::TryRecvError::Disconnected) => self.warm = None,
            }
        }
        if let Some(rx) = &self.warm_names {
            match rx.try_recv() {
                Ok(glyphs) => {
                    self.take_names(glyphs);
                    self.warm_names = None;
                }
                Err(mpsc::TryRecvError::Empty) => {}
                Err(mpsc::TryRecvError::Disconnected) => self.warm_names = None,
            }
        }
    }

    /// The fallback glyphs `prewarm_names` drew, as far as `FALLBACK_BYTES` leaves room.
    fn take_names(&mut self, glyphs: Vec<(Key, Glyph)>) {
        for (key, glyph) in glyphs {
            if self.cache.contains_key(&key) {
                continue;
            }
            let bytes = glyph_bytes(&glyph);
            if self.more_bytes + bytes > FALLBACK_BYTES {
                break;
            }
            self.more_bytes += bytes;
            self.more.push(key);
            self.cache.insert(key, glyph);
        }
    }

    /// How many strings have run the German table's patterns (their first sight, `i18n::Cache`).
    pub fn pattern_runs(&self) -> usize {
        self.tr.patterns_run()
    }

    /// Waits (at most 5 s) for the thread of `prewarm` and takes in what it drew: for the benchmark and the tests,
    /// which want the state of a pocket-ui that has been running for a moment.
    pub fn finish_prewarm(&mut self) {
        if let Some(rx) = &self.warm {
            if let Ok(glyphs) = rx.recv_timeout(std::time::Duration::from_secs(5)) {
                for (key, glyph) in glyphs {
                    self.cache.entry(key).or_insert(glyph);
                }
            }
            self.warm = None;
        }
        if let Some(rx) = self.warm_names.take() {
            if let Ok(glyphs) = rx.recv_timeout(std::time::Duration::from_secs(5)) {
                self.take_names(glyphs);
            }
        }
    }

    /// Draws the letters of `names` (the library's games) that need the fallback font on a thread of its own, at the
    /// sizes names are painted at (`PREWARM_NAMES`), so that a shelf of Japanese titles finds them in the cache. The
    /// font is opened there, not in a frame. Only names that are not plain ASCII are looked at (copied here, the rest
    /// on the thread); a library without such names starts no thread and never opens the font. Called once the first
    /// frame is on screen and when the library changes; a thread still at work is left to finish.
    pub fn prewarm_names<'a>(&mut self, names: impl IntoIterator<Item = &'a str>) {
        if self.warm_names.is_some() {
            // Done by now? Its glyphs are taken in; else it is left to finish.
            self.take_warm();
            if self.warm_names.is_some() {
                return;
            }
        }
        let Some(path) = self.fallback_path.clone() else { return };
        let names: Vec<String> = names.into_iter().filter(|n| !n.is_ascii()).map(str::to_string).collect();
        if names.is_empty() {
            return;
        }
        self.sync();
        let (title, body, scale) = (self.title.clone(), self.body.clone(), self.scale);
        // At the sizes they are painted at (`type_scale` is per thread: read here).
        let sizes: Vec<(Face, u32)> = PREWARM_NAMES.iter().flat_map(|(face, sizes)| sizes.iter().map(|px| (*face, crate::type_scale::px(*px)))).collect();
        let (tx, rx) = mpsc::channel();
        let spawned = std::thread::Builder::new().name("glyph-names".into()).spawn(move || {
            let letters = letters_for_fallback(&title, &body, names.iter().map(String::as_str), PREWARM_NAMES_MAX);
            if letters.is_empty() {
                return;
            }
            let Some(font) = crate::text_fallback::open(&path) else { return };
            let fits = fits(&font, &title, &body, scale);
            let mut out = Vec::with_capacity(letters.len() * sizes.len());
            for &(face, px) in &sizes {
                let main = if face == Face::Title { &title } else { &body };
                for &ch in letters.iter().filter(|&&ch| main.lookup_glyph_index(ch) == 0) {
                    if let Some(glyph) = font.raster(ch, px, fits[face as usize]) {
                        out.push(((face, px, ch), glyph));
                    }
                }
            }
            let _ = tx.send(out);
        });
        if spawned.is_ok() {
            self.warm_names = Some(rx);
        }
    }

    /// Draws the glyphs the menus use at their sizes on a thread of its own (`PREWARM`; called once the first frame
    /// is on screen), so that a page seen for the first time (Settings: 70 new glyphs, 3-4 ms on the Mini V2) finds
    /// its letters in the cache instead of drawing them in its frame. The glyphs come out as `glyph` would draw
    /// them; one needed before the thread is done is drawn on the spot as ever.
    pub fn prewarm(&mut self) {
        self.sync();
        self.prewarmed = true;
        if self.warm.is_some() {
            return;
        }
        // The names of the buttons are never translated: their first sight needn't try the patterns.
        for key in ["A", "B", "X", "Y", "L1", "R1", "L2", "R2", "START", "SELECT"] {
            self.tr.is_translated(key);
        }
        let (tx, rx) = mpsc::channel();
        let (title, body, scale, grid, lcd, tight, step, base) = (self.title.clone(), self.body.clone(), self.scale, self.grid, self.lcd, self.tight, self.step, self.base.clone());
        // At the sizes they are painted at (Settings → Text size, `type_scale`); a change of size draws them anew.
        let sizes: Vec<(Face, u32)> = PREWARM.iter().flat_map(|(face, sizes)| sizes.iter().map(|px| (*face, crate::type_scale::px(*px)))).collect();
        let spawned = std::thread::Builder::new().name("glyph-prewarm".into()).spawn(move || {
            let mut out: Vec<(Key, Glyph)> = Vec::with_capacity(sizes.len() * 120);
            for (face, px) in sizes {
                let font = if face == Face::Title { &title } else { &body };
                for ch in (' '..='~').chain(PREWARM_MORE.chars()) {
                    out.push(((face, px, ch), raster(font, scale[face as usize], grid[face as usize], lcd[face as usize], tight[face as usize].then_some(step), &base[face.slot()], ch, px)));
                }
            }
            let _ = tx.send(out);
        });
        if spawned.is_ok() {
            self.warm = Some(rx);
        }
    }

    /// How many glyphs were drawn when a frame asked for them (not ahead of time by `prewarm`): the benchmark of
    /// Settings' first frame counts the ones that frame had to draw.
    pub fn drawn_on_the_spot(&self) -> usize {
        self.drawn
    }

    /// Whether the face itself (Barlow or a theme's) has a picture for `ch`, not counting the fallback font.
    pub fn has_glyph(&self, face: Face, ch: char) -> bool {
        self.face(face).lookup_glyph_index(ch) != 0
    }

    /// `false`: paint strings as they are, untranslated (the manual's pages, which stay English for now); `true` again
    /// afterwards.
    pub fn set_translate(&mut self, on: bool) {
        self.raw = !on;
    }

    /// `line`, a piece of a game's name laid out on its own (`title_block`), is painted as it is in every language, as the
    /// lines of `wrap` are for a name (`i18n::Cache::name_line`).
    pub fn keep_as_name(&mut self, line: &str) {
        self.tr.name_line(line);
    }

    /// Measure and paint a game/system name literally, restoring the caller's language mode.
    /// Keep the scope around both operations: measuring must not consume a one-use name exemption.
    pub fn literal<R>(&mut self, f: impl FnOnce(&mut Self) -> R) -> R {
        let translating = self.translating();
        let naming = self.naming;
        self.naming = true;
        self.set_translate(false);
        let result = f(self);
        self.set_translate(translating);
        self.naming = naming;
        result
    }

    /// Fit a data name for a compact row (metadata choices, related games, management sheets).
    pub fn fit_name(&mut self, face: Face, px: u32, name: &str, width: i32) -> String {
        self.literal(|text| text.fit(face, px, name, width))
    }

    /// Wrap a data name without interpreting words inside it as UI copy.
    pub fn wrap_name(&mut self, face: Face, px: u32, name: &str, width: i32) -> Vec<String> {
        self.literal(|text| text.wrap(face, px, name, width))
    }

    /// A name already fitted/wrapped by its component; never translate the resulting fragments.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_name(&mut self, c: &mut crate::gfx::Canvas, face: Face, px: u32, x: i32, y: i32, colour: u32, name: &str) -> i32 {
        self.literal(|text| text.draw(c, face, px, x, y, colour, name))
    }

    /// The printed name on a carousel card, fitted before it is painted.
    pub fn card_name(&mut self, title: &str, width: i32) -> (u32, Vec<String>) {
        let lines = crate::title_block::name_layout_in(self, Face::Title, 37, title, width, 80);
        // Printed cards use one size for every line. A subtitle may have been
        // fitted smaller than the name; keeping the first size would undo that fit.
        let px = lines.iter().map(|line| line.px).min().unwrap_or(37);
        (px, lines.into_iter().map(|line| line.text).collect())
    }

    /// Whether strings are translated now (`set_translate`).
    pub fn translating(&self) -> bool {
        !self.raw
    }

    /// `s` in the interface's language (`crate::i18n`); a borrowed string or a shared one, never a new allocation
    /// for a string painted in the frame before.
    pub fn translated<'a>(&mut self, s: &'a str) -> crate::i18n::Tr<'a> {
        if self.naming { return crate::i18n::Tr::Same(s); }
        let translated = if self.raw { crate::i18n::Tr::Same(s) } else { self.tr.get(s) };
        if crate::i18n::is_name(s) { translated } else { crate::i18n::device_text(translated) }
    }

    pub fn width(&mut self, face: Face, px: u32, s: &str) -> i32 {
        let px = crate::type_scale::px(px);
        let s = self.translated(s);
        self.width_raw(face, px, &s)
    }

    /// The width of `s` exactly as given: not translated, and not counted as a string the page shows (the translation
    /// checks list what is shown). For measuring a word of a longer string that is drawn whole.
    pub fn width_plain(&mut self, face: Face, px: u32, s: &str) -> i32 {
        let px = crate::type_scale::px(px);
        self.width_raw(face, px, s)
    }

    fn width_raw(&mut self, face: Face, px: u32, s: &str) -> i32 {
        s.chars().map(|ch| self.glyph(face, px, ch).0.advance_width).sum::<f32>().round() as i32
    }

    /// Draws `s` with its baseline at `baseline`; returns the advance width.
    #[allow(clippy::too_many_arguments)]
    pub fn draw(&mut self, c: &mut Canvas, face: Face, px: u32, x: i32, baseline: i32, color: u32, s: &str) -> i32 {
        let px = crate::type_scale::px(px);
        let tr = self.translated(s);
        let w = self.draw_raw(c, face, px, x, baseline, color, &tr);
        // For the German snapshot checks (`i18n::note_overflow`; nothing is written unless they run).
        // A string that starts past the edge is a card waiting off the picture, not a cut one.
        if x < c.w && x + w > c.w {
            crate::i18n::note_overflow(s);
        }
        w
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_raw(&mut self, c: &mut Canvas, face: Face, px: u32, x: i32, baseline: i32, color: u32, s: &str) -> i32 {
        if !self.broken.is_empty() && self.broken.contains(s) {
            crate::i18n::note_broken(s);
        }
        // A face of whole screen pixels (Departure Mono) starts on the screen's grid too: on a 640x480 panel's half-size
        // canvas a letter at an odd x or baseline came out with every font pixel half grey (`font_style::Alt::tight`).
        self.sync();
        let whole = match face {
            Face::Title => self.tight[0],
            Face::Body => self.tight[1],
            Face::Narrow => false,
        };
        let step = if whole { self.step.max(1) as i32 } else { 1 };
        let (x, baseline) = (x - x.rem_euclid(step), baseline - baseline.rem_euclid(step));
        let mut pen = x as f32;
        let mut ink = boxes::Ink::new(boxes::on());
        for ch in s.chars() {
            let (m, bitmap) = self.glyph(face, px, ch);
            let gx = pen.round() as i32 + m.xmin;
            let gy = baseline - m.height as i32 - m.ymin;
            ink.add(gx, gy, m.width as i32, m.height as i32);
            if m.width > 0 && gx >= 0 && gy >= 0 && gx + m.width as i32 <= c.w && gy + m.height as i32 <= c.h {
                // Inside the picture, the usual case: rows of the picture, no bounds test per pixel, and a fully
                // covered pixel is the colour itself (`mix` at 255 gives exactly that).
                let (w, plain) = (c.w as usize, color & 0xff_ffff);
                for (row, cover) in bitmap.chunks_exact(m.width).enumerate() {
                    let at = (gy as usize + row) * w + gx as usize;
                    for (px, &a) in c.buf[at..at + m.width].iter_mut().zip(cover) {
                        match a {
                            0 => {}
                            255 => *px = plain,
                            a => *px = crate::gfx::mix(*px, color, a as u32),
                        }
                    }
                }
            } else {
                for row in 0..m.height {
                    for col in 0..m.width {
                        let a = bitmap[row * m.width + col];
                        if a > 0 {
                            c.blend(gx + col as i32, gy + row as i32, color, a);
                        }
                    }
                }
            }
            pen += m.advance_width;
        }
        ink.record(c, s);
        boxes::note_px(px, s);
        (pen - x as f32).round() as i32
    }

    /// Like `draw`, but only between `clip.0` and `clip.1`; within `fade.0` / `fade.1` pixels of the
    /// left / right edge the text fades out (0 = a hard edge). For scrolling titles (`marquee`).
    #[allow(clippy::too_many_arguments)]
    pub fn draw_clipped(&mut self, c: &mut Canvas, face: Face, px: u32, x: f32, baseline: i32, color: u32, s: &str, clip: (i32, i32), fade: (i32, i32)) {
        let px = crate::type_scale::px(px);
        let s = self.translated(s);
        let mut pen = x;
        for ch in s.chars() {
            let (m, bitmap) = self.glyph(face, px, ch);
            let gx = pen.round() as i32 + m.xmin;
            pen += m.advance_width;
            if gx + m.width as i32 <= clip.0 || gx >= clip.1 {
                continue;
            }
            let gy = baseline - m.height as i32 - m.ymin;
            for col in 0..m.width {
                let cx = gx + col as i32;
                if cx < clip.0 || cx >= clip.1 {
                    continue;
                }
                let edge = |d: i32, f: i32| if f > 0 { (d * 255 / f).clamp(0, 255) } else { 255 };
                let k = edge(cx - clip.0, fade.0).min(edge(clip.1 - 1 - cx, fade.1)) as u32;
                for row in 0..m.height {
                    let a = bitmap[row * m.width + col] as u32 * k / 255;
                    if a > 0 {
                        c.blend(cx, gy + row as i32, color, a as u8);
                    }
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw_right(&mut self, c: &mut Canvas, face: Face, px: u32, right: i32, baseline: i32, color: u32, s: &str) {
        let px = crate::type_scale::px(px);
        let s = self.translated(s);
        let w = self.width_raw(face, px, &s);
        self.draw_raw(c, face, px, right - w, baseline, color, &s);
    }

    /// Breaks `s` into lines no wider than `max` pixels, at spaces.
    pub fn wrap(&mut self, face: Face, px: u32, s: &str, max: i32) -> Vec<String> {
        self.wrap_with(face, px, s, max, false)
    }

    /// `wrap` for sentences (explanations, notes, dialogs): a last line of one word takes the word before it along,
    /// when the two fit ("an ist.", not "ist." alone under a paragraph; the size sweep found them under German
    /// explanations). Not for titles and names, whose first line is the better one when it is longer ("Game Boy"
    /// over "Advance").
    pub fn wrap_prose(&mut self, face: Face, px: u32, s: &str, max: i32) -> Vec<String> {
        self.wrap_with(face, px, s, max, true)
    }

    fn wrap_with(&mut self, face: Face, px: u32, s: &str, max: i32, no_widow: bool) -> Vec<String> {
        let px = crate::type_scale::px(px);
        // A game's name stays as it is in pieces too: "Sonic Adventure" on two lines isn't "Sonic" and the word "Adventure".
        let name = !self.raw && crate::i18n::is_name(s);
        let s = self.translated(s);
        let translated = !self.raw && !matches!(s, crate::i18n::Tr::Same(_));
        let mut lines: Vec<String> = Vec::new();
        let mut line = String::new();
        for word in s.split_whitespace() {
            let candidate = if line.is_empty() { word.to_string() } else { format!("{line} {word}") };
            if self.width_raw(face, px, &candidate) <= max {
                line = candidate;
                continue;
            }
            if !line.is_empty() {
                lines.push(std::mem::take(&mut line));
            }
            // A word wider than the line on its own (a folder's path, larger text) is broken into pieces that fit,
            // after a "/", ".", "-" or "_" where it has one; only then is it measured alone.
            let mut pieces = self.pieces(face, px, word, max);
            line = pieces.pop().unwrap_or_default();
            lines.extend(pieces);
        }
        if !line.is_empty() {
            lines.push(line);
        }
        if let (true, [.., prev, last]) = (no_widow, lines.as_mut_slice()) {
            if !last.contains(' ') {
                if let Some((head, word)) = prev.rsplit_once(' ') {
                    let moved = format!("{word} {last}");
                    if self.width_raw(face, px, &moved) <= max {
                        *prev = head.to_string();
                        *last = moved;
                    }
                }
            }
        }
        // Lines of a translation are translated already when they are painted.
        if name {
            for l in &lines {
                self.tr.name_line(l);
            }
        } else if translated {
            for l in &lines {
                self.tr.is_translated(l);
            }
        }
        lines
    }

    /// `word` as it is when it fits into `max` pixels, else in pieces that do (`wrap`).
    fn pieces(&mut self, face: Face, px: u32, word: &str, max: i32) -> Vec<String> {
        if self.width_raw(face, px, word) <= max {
            return vec![word.to_string()];
        }
        let mut out = Vec::new();
        let mut piece = String::new();
        let mut torn = false;
        for ch in word.chars() {
            let mut next = piece.clone();
            next.push(ch);
            if self.width_raw(face, px, &next) <= max || piece.is_empty() {
                piece = next;
                continue;
            }
            // Cut after the last break mark in the piece, or here when it has none.
            let cut = piece.rfind(['/', '.', '-', '_']).map(|i| i + 1).filter(|i| *i < piece.len()).unwrap_or(piece.len());
            if cut == piece.len() && word.chars().all(char::is_alphanumeric) && crate::i18n::recording() && self.broken.len() < 512 {
                self.broken.insert(piece.clone());
            }
            torn |= cut == piece.len();
            let rest = piece.split_off(cut);
            out.push(std::mem::replace(&mut piece, rest));
            piece.push(ch);
        }
        if !piece.is_empty() {
            out.push(piece);
        }
        // A short word torn in the middle is a column with no room (a price down a strip one letter wide), not a path
        // that is too long: if its pieces are then painted, the size sweep reports it (`boxes`, `W`).
        if torn && word.chars().count() <= boxes::SHORT_WORD {
            boxes::broken_word(word, &out);
        }
        out
    }

    /// `wrap`, but at most `lines` lines: when there are more, the last one shown ends with "…" (a paragraph given
    /// only the room above the hints, with larger text). None when `lines` is 0.
    pub fn wrap_at_most(&mut self, face: Face, px: u32, s: &str, max: i32, lines: usize) -> Vec<String> {
        self.wrap_at_most_with(face, px, s, max, lines, false)
    }

    /// `wrap_at_most` for sentences (`wrap_prose`).
    pub fn wrap_prose_at_most(&mut self, face: Face, px: u32, s: &str, max: i32, lines: usize) -> Vec<String> {
        self.wrap_at_most_with(face, px, s, max, lines, true)
    }

    fn wrap_at_most_with(&mut self, face: Face, px: u32, s: &str, max: i32, lines: usize, no_widow: bool) -> Vec<String> {
        let mut all = self.wrap_with(face, px, s, max, no_widow);
        if all.len() > lines {
            all.truncate(lines);
            if let Some(last) = all.last_mut() {
                // Measured as it is painted: the lines are in the interface's language already.
                let raw = self.raw;
                self.raw = true;
                *last = self.fit(face, px, &format!("{last} …"), max);
                if !last.ends_with('…') {
                    last.push('…');
                }
                self.raw = raw;
            }
        }
        all
    }

    /// `s` wrapped to `max` pixels from the baseline `y` on, `step` apart, as many lines as have their baseline above
    /// `bottom` (the last one shown ends with "…" when that is not all of them); returns the baseline after the last.
    #[allow(clippy::too_many_arguments)]
    pub fn paragraph(&mut self, c: &mut Canvas, face: Face, px: u32, (x, mut y): (i32, i32), max: i32, step: i32, bottom: i32, color: u32, s: &str) -> i32 {
        let room = if y > bottom { 0 } else { ((bottom - y) / step.max(1) + 1) as usize };
        for line in self.wrap_at_most(face, px, s, max, room) {
            self.draw(c, face, px, x, y, color, &line);
            y += step;
        }
        y
    }

    /// A folder's path in `max` pixels with its distinguishing end kept: whole when it fits, else the front dropped
    /// part by part ("/storage/user/roms/gb" becomes "…/roms/gb", then "…/gb"), and only a last part wider than the
    /// room is cut with "…" like `fit` does. (A path cut at its end read "/storage/newu…" for every system.)
    pub fn fit_path(&mut self, face: Face, px: u32, path: &str, max: i32) -> String {
        let whole = self.fit(face, px, path, max);
        if !whole.ends_with('…') {
            return whole;
        }
        let parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
        for from in 1..parts.len() {
            let shown = format!("…/{}", parts[from..].join("/"));
            if self.fit(face, px, &shown, max) == shown {
                return shown;
            }
        }
        match parts.last() {
            Some(last) => self.fit(face, px, last, max),
            None => whole,
        }
    }

    /// The size for `title` from `px` down to `least` (in steps of 2) at which every word of it fits into `max` pixels
    /// on its own, so that `wrap` breaks it between words, never inside one ("PlayStatio / n Portable" in the wider
    /// Pixel font at Large text); `least` when none does.
    pub fn title_px(&mut self, face: Face, px: u32, title: &str, max: i32, least: u32) -> u32 {
        let mut size = px;
        while size > least && !self.words_fit(face, size, title, max) {
            size = size.saturating_sub(2).max(least);
        }
        size
    }

    /// Whether every word of `s` (as the interface's language has it) fits into `max` pixels at `px` on its own, so that
    /// `wrap` breaks it only between words.
    pub fn words_fit(&mut self, face: Face, px: u32, s: &str, max: i32) -> bool {
        let scaled = crate::type_scale::px(px);
        let s = self.translated(s).to_string();
        s.split_whitespace().all(|w| self.width_raw(face, scaled, w) <= max)
    }

    /// A title in at most two lines of `max` pixels (a long one's second line ends with "…"), for a card whose name
    /// is cut on one line ("Game Boy Adv…").
    pub fn title_lines(&mut self, face: Face, px: u32, title: &str, max: i32) -> Vec<String> {
        self.wrap_at_most(face, px, title, max, 2)
    }

    /// `english` in the first of `sizes` (largest first) it fits into `max` pixels with; in the last one cut short with "…"
    /// when none does. Layout before cutting: a line a little smaller reads better than one that ends in "…". Returns the
    /// size to draw it in and the text.
    pub fn fit_sizes(&mut self, face: Face, sizes: &[u32], english: &str, max: i32) -> (u32, String) {
        let last = *sizes.last().expect("at least one size");
        let px = sizes.iter().copied().find(|&px| self.width(face, px, english) <= max).unwrap_or(last);
        (px, self.fit(face, px, english, max))
    }

    /// Shortens `s` with an ellipsis until it fits into `max` pixels.
    pub fn fit(&mut self, face: Face, px: u32, english: &str, max: i32) -> String {
        let px = crate::type_scale::px(px);
        let s = self.translated(english);
        // What comes back of a translation is translated already when it is painted (`i18n::Cache::is_translated`).
        let translated = !self.raw && !matches!(s, crate::i18n::Tr::Same(_));
        if self.width_raw(face, px, &s) <= max {
            let whole = s.to_string();
            if translated {
                self.tr.is_translated(&whole);
            }
            return whole;
        }
        // For the German snapshot checks: cut short here (`i18n::note_cut`; nothing is written unless they run).
        crate::i18n::note_cut(english);
        let mut chars: Vec<char> = s.chars().collect();
        while !chars.is_empty() {
            chars.pop();
            let candidate: String = chars.iter().collect::<String>().trim_end().to_string() + "…";
            if self.width_raw(face, px, &candidate) <= max {
                if translated {
                    self.tr.is_translated(&candidate);
                }
                return candidate;
            }
        }
        String::new()
    }
}

/// `POCKETUI_TEXT_BOXES=<file>`: for the snapshot checks of Settings → Text size (tests/text_size.rs), where each
/// string of a picture was painted, so that text running into other text or off the picture is found by measuring
/// rather than by eye. Off (one test of a `OnceLock` per string) unless the variable is set.
pub mod boxes {
    use std::sync::{Mutex, OnceLock};

    use crate::gfx::Canvas;

    /// (x0, y0, x1, y1).
    type Rect = (i32, i32, i32, i32);

    /// A painted string: the address of its canvas's first pixel, the canvas's width, its ink in the canvas as far as
    /// the canvas shows it, whether the canvas's left or right edge cut it, the string.
    type Painted = (usize, i32, Rect, bool, String);

    fn record() -> Option<&'static Mutex<Vec<Painted>>> {
        static REC: OnceLock<Option<Mutex<Vec<Painted>>>> = OnceLock::new();
        REC.get_or_init(|| {
            ["POCKETUI_TEXT_BOXES", "POCKETUI_VISIBLE_TEXT", "POCKETUI_LAYOUT_AUDIT", "POCKETUI_TITLE_BLOCKS", crate::nav_probe::ENV].iter()
                .any(|key| std::env::var_os(key).is_some_and(|p| !p.is_empty()))
                .then(|| Mutex::new(Vec::new()))
        }).as_ref()
    }

    pub fn on() -> bool {
        record().is_some()
    }

    /// The ink of one string, glyph by glyph.
    pub struct Ink(Option<(i32, i32, i32, i32)>, bool);

    impl Ink {
        pub fn new(on: bool) -> Self {
            Ink(None, on)
        }

        pub fn add(&mut self, x: i32, y: i32, w: i32, h: i32) {
            if !self.1 || w == 0 || h == 0 {
                return;
            }
            let r = self.0.get_or_insert((x, y, x + w, y + h));
            *r = (r.0.min(x), r.1.min(y), r.2.max(x + w), r.3.max(y + h));
        }

        pub fn record(self, c: &Canvas, s: &str) {
            let (Some((x0, y0, x1, y1)), Some(rec)) = (self.0, record()) else { return };
            // What the canvas shows of it: a list's band cuts its rows at its edges on purpose, and a row that has
            // scrolled out of it is not on the picture. Cut at the left or right edge, it is.
            let (cx0, cy0, cx1, cy1) = (x0.max(0), y0.max(0), x1.min(c.w), y1.min(c.h));
            if cx1 <= cx0 || cy1 <= cy0 {
                return;
            }
            let cut = x0 < -2 || x1 > c.w + 2;
            if cut_vertically(y0, y1, c.h) {
                if let Ok(mut v) = v_cuts().lock() {
                    v.push((c.buf.as_ptr() as usize, c.w, c.h, s.to_string()));
                }
            }
            if let Ok(mut rec) = rec.lock() {
                rec.push((c.buf.as_ptr() as usize, c.w, (cx0, cy0, cx1, cy1), cut, s.to_string()));
            }
        }
    }

    /// A sheet or a dialog covers the picture painted so far (dimmed around it): text under it is not read.
    pub fn covered() {
        reset();
    }

    /// An area no other text may run into (a hint bar), recorded as if it were a string called `name`.
    pub fn area(c: &Canvas, (x, y, w, h): (i32, i32, i32, i32), name: &str) {
        let Some(Ok(mut rec)) = record().map(|r| r.lock()) else { return };
        rec.push((c.buf.as_ptr() as usize, c.w, (x.max(0), y.max(0), (x + w).min(c.w), (y + h).min(c.h)), false, name.to_string()));
    }

    /// Where the record is now, for `clip_since`.
    pub fn mark() -> usize {
        record().and_then(|r| r.lock().ok()).map_or(0, |r| r.len())
    }

    /// What was painted since `mark` is no string of its own: the soft edge of one painted again over it.
    pub fn forget_since(mark: usize) {
        if let Some(Ok(mut rec)) = record().map(|r| r.lock()) {
            rec.truncate(mark);
        }
    }

    /// What was painted into `c` since `mark` shows only between its rows `y0` and `y1` (a page that puts back what
    /// its lines drew over its edges, the manual's).
    pub fn clip_since(mark: usize, c: &Canvas, y0: i32, y1: i32) {
        let Some(Ok(mut rec)) = record().map(|r| r.lock()) else { return };
        let at = (c.buf.as_ptr() as usize / 4) as i64;
        let mut i = mark;
        while i < rec.len() {
            let (p, cw, r, _, _) = &mut rec[i];
            let dy = ((*p / 4) as i64 - at) / c.w.max(1) as i64;
            let (lo, hi) = ((y0 as i64 - dy) as i32, (y1 as i64 - dy) as i32);
            if *cw == c.w {
                r.1 = r.1.max(lo);
                r.3 = r.3.min(hi);
            }
            if r.3 <= r.1 {
                rec.remove(i);
            } else {
                i += 1;
            }
        }
    }

    /// What was painted since `mark` is cut by the picture's edge on purpose (the Cards look's neighbours peek in).
    pub fn cut_on_purpose(mark: usize) {
        let Some(Ok(mut rec)) = record().map(|r| r.lock()) else { return };
        for p in rec.iter_mut().skip(mark) {
            p.3 = false;
        }
    }

    /// `(x, y, w, h)` of `c` was painted over (a card over the bottom of the shelf): what was there is not seen.
    pub fn painted_over(c: &Canvas, rect: (i32, i32, i32, i32)) {
        painted_over_or_covered(usize::MAX, c, rect, true);
    }

    /// What was painted before `mark` and lies under `(x, y, w, h)` is not read: an overlay (the letter of a jump)
    /// covers the page there on purpose. What the overlay itself paints (after `mark`) stays.
    pub fn covered_since(mark: usize, c: &Canvas, rect: (i32, i32, i32, i32)) {
        painted_over_or_covered(mark, c, rect, false);
    }

    /// Drops what lies inside `rect` (`inside_only`) or anywhere under it, if it was painted before `mark`.
    fn painted_over_or_covered(mark: usize, c: &Canvas, (x, y, w, h): (i32, i32, i32, i32), inside_only: bool) {
        let Some(Ok(mut rec)) = record().map(|r| r.lock()) else { return };
        let at = (c.buf.as_ptr() as usize / 4) as i64;
        let before = mark.min(rec.len());
        let mut i = 0;
        rec.retain(|(p, cw, (x0, y0, x1, y1), _, _)| {
            let seen_before = i < before;
            i += 1;
            if !seen_before || *cw != c.w {
                return true;
            }
            let dy = ((*p / 4) as i64 - at) / c.w as i64;
            let (y0, y1) = (*y0 as i64 + dy, *y1 as i64 + dy);
            if inside_only {
                return !(*x0 >= x && *x1 <= x + w && y0 >= y as i64 && y1 <= (y + h) as i64);
            }
            // Under it at all (the same 2 px as `finish` counts as running into something).
            !(*x1.min(&(x + w)) - x0.max(&x) > 2 && y1.min((y + h) as i64) - y0.max(y as i64) > 2)
        });
    }

    /// What was painted into `from` (`from_w` wide, whole rows) now lies in `to` (`to_w` wide) at the same rows and
    /// `dx` pixels further right: a column copied into the middle of the screen (the binary's `app_column`).
    pub fn moved(from: &[u32], from_w: i32, to: &[u32], to_w: i32, dx: i32) {
        let Some(Ok(mut rec)) = record().map(|r| r.lock()) else { return };
        let (start, end) = (from.as_ptr() as usize, from.as_ptr() as usize + from.len() * 4);
        for (at, cw, (x0, _, x1, _), _, _) in rec.iter_mut() {
            if *at < start || *at >= end || *cw != from_w {
                continue;
            }
            let row = (*at - start) / 4 / from_w.max(1) as usize;
            *at = to.as_ptr() as usize + row * to_w as usize * 4;
            *cw = to_w;
            *x0 += dx;
            *x1 += dx;
        }
    }

    /// The pixel size `s` was painted at, for the layout audit's `S` (only while a record is on).
    pub fn note_px(px: u32, s: &str) {
        if !on() {
            return;
        }
        if let Ok(mut v) = sizes().lock() {
            if v.len() < 20_000 && !v.iter().any(|(p, t)| *p == px && t == s) {
                v.push((px, s.to_string()));
            }
        }
    }

    /// A line of the big game name (`title_block`) painted at `px`, its baseline at `y`, for tests/long_titles.rs:
    /// `POCKETUI_TITLE_BLOCKS` names the file the lines of the picture go to (size, main or sub, baseline, text).
    pub fn note_title(px: u32, sub: bool, y: i32, text: &str) {
        if std::env::var_os("POCKETUI_TITLE_BLOCKS").is_none() {
            return;
        }
        if let Ok(mut v) = titles().lock() {
            v.push(format!("{px}\t{}\t{y}\t{text}", if sub { "sub" } else { "main" }));
        }
    }

    fn titles() -> &'static Mutex<Vec<String>> {
        static TITLES: OnceLock<Mutex<Vec<String>>> = OnceLock::new();
        TITLES.get_or_init(|| Mutex::new(Vec::new()))
    }

    fn sizes() -> &'static Mutex<Vec<(u32, String)>> {
        static SIZES: OnceLock<Mutex<Vec<(u32, String)>>> = OnceLock::new();
        SIZES.get_or_init(|| Mutex::new(Vec::new()))
    }

    /// Strings whose ink a canvas's top or bottom edge cuts (more than 2 px): the canvas's first pixel, its width and
    /// height, the string. Only one painted straight into the whole picture is a fault (`V` in `finish`): a list's
    /// band cuts its rows at its edges on purpose.
    fn v_cuts() -> &'static Mutex<Vec<(usize, i32, i32, String)>> {
        static V: OnceLock<Mutex<Vec<(usize, i32, i32, String)>>> = OnceLock::new();
        V.get_or_init(|| Mutex::new(Vec::new()))
    }

    /// Ink from `y0` to `y1` in a canvas `h` high: the top or bottom edge cuts it by more than 2 px.
    pub(super) fn cut_vertically(y0: i32, y1: i32, h: i32) -> bool {
        y0 < -2 || y1 > h + 2
    }

    /// Of the strings cut at a canvas's top or bottom, those painted into the picture itself (first pixel `start`,
    /// `w` × `h`): a list's band or a layer cuts its own rows on purpose, the picture's edge cutting a heading is a fault.
    pub(super) fn whole_picture_cuts(v: &[(usize, i32, i32, String)], start: usize, w: i32, h: i32) -> Vec<&str> {
        v.iter().filter(|(at, cw, ch, s)| *at == start && *cw == w && *ch == h && !s.starts_with("‹hints"))
            .map(|(_, _, _, s)| s.as_str()).collect()
    }

    /// A new picture is painted: what was recorded before is not in it.
    pub fn reset() {
        if let Ok(mut v) = v_cuts().lock() {
            v.clear();
        }
        if let Ok(mut v) = sizes().lock() {
            v.clear();
        }
        if let Ok(mut v) = titles().lock() {
            v.clear();
        }
        if let Some(Ok(mut rec)) = record().map(|r| r.lock()) {
            rec.clear();
        }
        if let (Some(_), Ok(mut b)) = (record(), broken().lock()) {
            b.clear();
        }
    }

    /// Words up to this long are never broken in the middle: a column too narrow for them is the fault, not the word
    /// (a longer one may be a folder or a file name with no break mark).
    pub const SHORT_WORD: usize = 16;

    /// The short words torn across lines while measuring, with their pieces.
    type Torn = (String, Vec<String>);

    fn broken() -> &'static Mutex<Vec<Torn>> {
        static B: OnceLock<Mutex<Vec<Torn>>> = OnceLock::new();
        B.get_or_init(|| Mutex::new(Vec::new()))
    }

    /// `word`, short as it is, was torn across lines because its column was too narrow: the picture's record says so
    /// (`W` and the word), which the size sweep treats like text running into other text (a Game Boy Advance
    /// page, 2026-11-03: "US$99.99" one character a line down a strip beside the prices).
    pub fn broken_word(word: &str, pieces: &[String]) {
        if record().is_none() {
            return;
        }
        if let Ok(mut b) = broken().lock() {
            if b.len() < 256 && !b.iter().any(|(w, _)| w == word) {
                b.push((word.to_string(), pieces.to_vec()));
            }
        }
    }

    /// The picture `buf` (`w` × `h`) is done: writes to the file of `POCKETUI_TEXT_BOXES`, one line each, `O` and
    /// the two strings for text whose ink runs into another string's by more than 2 px each way, `W` and the word for a short
/// word torn across lines by a column too narrow for it, `T` and the two for
    /// strings on one line closer than 3 px (they read as one), `E` and the string
    /// for text cut by the left or right edge of what it was painted into, `V` and the string for text the picture's own
    /// top or bottom edge cuts. Strings painted into another buffer (a layer
    /// mixed in later) are left out.
    pub fn finish(buf: &[u32], w: i32, h: i32) {
        let Some(rec) = record() else { return };
        let Ok(rec) = rec.lock() else { return };
        let start = buf.as_ptr() as usize;
        let end = start + buf.len() * 4;
        let mut placed: Vec<(Rect, bool, &str)> = Vec::new();
        for (at, cw, (x0, y0, x1, y1), cut, s) in rec.iter() {
            if *cw != w || *at < start || *at >= end {
                continue;
            }
            let dy = ((at - start) / 4) as i32 / w;
            let r = (*x0, y0 + dy, *x1, y1 + dy);
            // Painted twice in the same place (a name under the moving highlight): one string.
            if !placed.iter().any(|(p, _, t)| *p == r && t == s) {
                placed.push((r, *cut, s));
            }
        }
        // The navigation explorer's record (`nav_probe`): every string and mark with its place, then left out of what follows,
        // which is about text.
        crate::nav_probe::write(&placed);
        placed.retain(|(_, _, s)| !crate::nav_probe::is_mark(s));
        // A transcript of strings actually painted into this frame, after clipping and covered-dialog resets.
        // Unlike translation diagnostics it excludes width measurements and off-screen text.
        if let Some(path) = std::env::var_os("POCKETUI_VISIBLE_TEXT") {
            let text = placed.iter().filter(|(_, _, s)| !s.starts_with("‹hints") && !s.starts_with("‹row") && *s != "‹frame›")
                .map(|(_, _, s)| *s).collect::<Vec<_>>().join("\n");
            let _ = std::fs::write(path, text);
        }
        if let (Some(path), Ok(v)) = (std::env::var_os("POCKETUI_TITLE_BLOCKS"), titles().lock()) {
            let _ = std::fs::write(path, v.join("\n"));
        }
        if let Some(path) = std::env::var_os("POCKETUI_LAYOUT_AUDIT") {
            let _ = std::fs::write(path, layout_audit(&placed));
        }
        let Some(path) = std::env::var_os("POCKETUI_TEXT_BOXES") else { return };
        let mut out = String::new();
        // Only a word whose pieces were really painted: a layout that tried a narrower room and took another is no fault.
        if let Ok(b) = broken().lock() {
            for (word, pieces) in b.iter() {
                if pieces.iter().all(|p| placed.iter().any(|(_, _, s)| *s == p.as_str())) {
                    out += &format!("{}\t{word}\n", 'W');
                }
            }
        }
        for (i, (a, cut, sa)) in placed.iter().enumerate() {
            if *cut {
                out += &format!("{}\t{sa}\n", 'E');
            }
            for (b, _, sb) in &placed[i + 1..] {
                // The hint bar and its own keys and labels, which lie inside it.
                let inside = |r: &Rect, o: &Rect| r.0 >= o.0 && r.1 >= o.1 && r.2 <= o.2 && r.3 <= o.3;
                if (sa.starts_with("‹hints") && inside(b, a)) || (sb.starts_with("‹hints") && inside(a, b)) {
                    continue;
                }
                // A row's band (`area` "‹row›") holds its own text and its neighbours stand close: only a hint bar over it is a fault.
                if (sa.starts_with("‹row") && !sb.starts_with("‹hints")) || (sb.starts_with("‹row") && !sa.starts_with("‹hints")) {
                    continue;
                }
                let (ox, oy) = (a.2.min(b.2) - a.0.max(b.0), a.3.min(b.3) - a.1.max(b.1));
                let (first, second) = if sa <= sb { (sa, sb) } else { (sb, sa) };
                if ox > 2 && oy > 2 {
                    out += &format!("{}\t{first}\t{second}\n", 'O');
                } else if ox > -3 && oy * 2 > (a.3 - a.1).min(b.3 - b.1) && !punctuation(sa) && !punctuation(sb) {
                    // Side by side on one line with less than a space between them: they read as one word.
                    out += &format!("{}\t{first}\t{second}\n", 'T');
                }
            }
        }
        // Cut by the picture's own top or bottom edge, a heading half off the screen: nothing overlaps there either.
        if let Ok(v) = v_cuts().lock() {
            for s in whole_picture_cuts(&v, start, w, h) {
                out += &format!("{}\t{s}\n", 'V');
            }
        }
        let _ = std::fs::write(path, out);
        // A full stop or comma painted on its own after a bold word or a link (the manual's "Manual."): it belongs to the
        // word before it, close by design (Nunito's letters stand closer to it than Barlow's).
        fn punctuation(s: &str) -> bool {
            let t = s.trim();
            !t.is_empty() && t.chars().all(|c| matches!(c, '.' | ',' | ';' | ':' | '!' | '?' | ')' | '…'))
        }
    }

    /// Text painted smaller than this (pixels, after the type scale) is reported as `S`.
    const SMALLEST_PX: u32 = 14;

    /// `POCKETUI_LAYOUT_AUDIT=<file>` (scripts/size-sweep.sh): what the O/T/E record cannot see because nothing overlaps.
    /// `N` and the strings: three or more strings stacked in one column that are each one or two characters (a value
    /// wrapped letter by letter, "US$99.99" as `U`/`S`/`$`...); `S`: a string of three or more letters painted
    /// smaller than `SMALLEST_PX` (a list row cut by its band has little ink left, so the ink's height says nothing).
    /// A word broken inside is the i18n record's `W`; text cut at a side edge, `E`.
    fn layout_audit(placed: &[(Rect, bool, &str)]) -> String {
        let mut out = String::new();
        let word = |s: &str| s.trim().chars().filter(|c| c.is_alphanumeric() || *c == '$').count();
        let below = |a: &Rect, b: &Rect| (b.0 - a.0).abs() <= 2 && b.1 >= a.1 && b.1 - a.1 <= (a.3 - a.1) * 2 + 4 && b.1 > a.1;
        for (a, _, sa) in placed.iter() {
            let n = sa.trim().chars().count();
            if sa.starts_with("‹hints") || sa.starts_with("‹row") {
                continue;
            }
            if (1..=2).contains(&n) && word(sa) > 0 {
                // The head of a column: nothing one or two characters wide right above it in the same column.
                let up = placed.iter().any(|(b, _, sb)| below(b, a) && (1..=2).contains(&sb.trim().chars().count()) && word(sb) > 0);
                let mut run = vec![*sa];
                let mut last = *a;
                if !up {
                    while let Some((b, _, sb)) = placed.iter().find(|(b, _, sb)| below(&last, b) && (1..=2).contains(&sb.trim().chars().count()) && word(sb) > 0) {
                        run.push(sb);
                        last = *b;
                    }
                    if run.len() >= 3 {
                        out += &format!("N\t{}\n", run.join(""));
                    }
                }
            }
        }
        if let Ok(v) = sizes().lock() {
            for (px, t) in v.iter().filter(|(px, t)| *px < SMALLEST_PX && t.chars().filter(|c| c.is_alphabetic()).count() >= 3 && !t.starts_with("‹hints")) {
                out += &format!("{}\t{px}\t{t}\n", 'S');
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_heading_half_off_the_top_of_the_picture_is_found_a_bands_cut_row_is_not() {
        // "Quick settings" painted with its baseline 30 px down in a 50 px face: ink from -18 to 36 (2026-11-03 report).
        assert!(boxes::cut_vertically(-18, 36, 480));
        assert!(!boxes::cut_vertically(10, 58, 480), "ink inside the canvas");
        assert!(!boxes::cut_vertically(-2, 40, 480), "up to 2 px is the glyphs' soft edge");
        assert!(boxes::cut_vertically(450, 500, 480), "the bottom edge too");
        let v = vec![(100, 640, 480, "Quick settings".to_string()), (900, 640, 120, "Row".to_string()), (100, 640, 480, "‹hints".to_string())];
        assert_eq!(boxes::whole_picture_cuts(&v, 100, 640, 480), vec!["Quick settings"], "only the picture's own, no hint bar");
    }

    #[test]
    fn a_long_path_keeps_its_end_and_a_long_title_takes_two_lines() {
        let mut t = Text::builtin();
        let path = "/storage/newuser-test/roms/gba";
        let room = t.width(Face::Body, 21, "…/roms/gba") + 4;
        assert!(t.width(Face::Body, 21, path) > room);
        assert_eq!(t.fit_path(Face::Body, 21, path, room), "…/roms/gba");
        let tight = t.width(Face::Body, 21, "…/gba") + 4;
        assert_eq!(t.fit_path(Face::Body, 21, path, tight), "…/gba");
        assert_eq!(t.fit_path(Face::Body, 21, path, 4000), path, "whole when it fits");
        assert!(t.fit_path(Face::Body, 21, "/storage/roms/a-very-long-folder-name", 60).ends_with('…'), "a last part wider than the room is cut");
        let room = t.width(Face::Title, 37, "Game Boy Adv") + 4;
        assert_eq!(t.title_lines(Face::Title, 37, "Game Boy Advance", room), ["Game Boy", "Advance"]);
        assert_eq!(t.title_lines(Face::Title, 37, "Game Boy", 4000), ["Game Boy"]);
    }

    #[test]
    fn a_wrapped_paragraph_does_not_end_on_one_word() {
        let mut t = Text::builtin();
        t.set_translate(false);
        let s = "Entfällt, solange die Farbkorrektur des Emulators an ist.";
        // The room that holds everything up to "an": the old wrap put "ist." alone on the second line.
        let room = t.width(Face::Body, 30, "Entfällt, solange die Farbkorrektur des Emulators an") + 2;
        assert_eq!(t.wrap_prose(Face::Body, 30, s, room), ["Entfällt, solange die Farbkorrektur des Emulators", "an ist."]);
        // A single word, or two words that fit on a line, is left alone.
        assert_eq!(t.wrap_prose(Face::Body, 30, "Zurück", 4000), ["Zurück"]);
        assert_eq!(t.wrap_prose(Face::Body, 30, s, 4000), [s]);
        assert_eq!(t.wrap(Face::Body, 30, s, room), ["Entfällt, solange die Farbkorrektur des Emulators an", "ist."], "plain wrap is as it was");
    }

    /// `draw` as it was before the inside-the-picture path: one `blend` (with its bounds test) per pixel.
    #[allow(clippy::too_many_arguments)]
    fn draw_by_blend(t: &mut Text, c: &mut Canvas, face: Face, px: u32, x: i32, baseline: i32, color: u32, s: &str) {
        let mut pen = x as f32;
        for ch in s.chars() {
            let (m, bitmap) = t.glyph(face, px, ch).clone();
            let (gx, gy) = (pen.round() as i32 + m.xmin, baseline - m.height as i32 - m.ymin);
            for row in 0..m.height {
                for col in 0..m.width {
                    let a = bitmap[row * m.width + col];
                    if a > 0 {
                        c.blend(gx + col as i32, gy + row as i32, color, a);
                    }
                }
            }
            pen += m.advance_width;
        }
    }

    #[test]
    fn text_inside_and_at_the_edges_of_the_picture_comes_out_pixel_for_pixel_as_blending_each_pixel() {
        let mut t = Text::builtin();
        // Colours with the top byte set too: the picture is 0RGB, what is painted keeps its three channels.
        for (face, px, color, bg) in [(Face::Body, 30, 0xece9e1u32, 0x12161bu32), (Face::Body, 36, 0x12161b, 0xf3efe7), (Face::Title, 50, 0xffffff, 0x000000)] {
            // Fully inside, and partly out on every side.
            for (x, base) in [(40, 60), (-12, 60), (200, 60), (40, 8), (40, 78), (150, 40)] {
                let (mut a, mut b) = (vec![bg; 220 * 80], vec![bg; 220 * 80]);
                t.draw(&mut Canvas::new(&mut a, 220, 80), face, px, x, base, color, "Wi-Fi 12:34 ÄÖ");
                draw_by_blend(&mut t, &mut Canvas::new(&mut b, 220, 80), face, px, x, base, color, "Wi-Fi 12:34 ÄÖ");
                assert!(a == b, "{} {px} at {x},{base}", if face == Face::Title { "title" } else { "body" });
            }
        }
    }

    /// First-run test 2026-10-24, finding #9: "Ünïcödé ✓ Spiel" showed an empty box for the tick.
    #[test]
    fn a_character_the_font_lacks_is_left_out_or_shown_as_a_question_mark() {
        let mut t = Text::builtin();
        for face in [Face::Body, Face::Title] {
            assert!(!t.has_glyph(face, '✓') && !t.has_glyph(face, 'ド'), "the test needs characters Barlow lacks");
            assert_eq!(t.width(face, 30, "A✓B\u{200b}"), t.width(face, 30, "AB"), "a mark: nothing, no box");
            assert_eq!(t.width(face, 30, "ドラ"), t.width(face, 30, "??"), "a letter: a question mark, so a name never comes out empty");
            let (mut a, mut b) = (vec![0u32; 300 * 60], vec![0u32; 300 * 60]);
            t.draw(&mut Canvas::new(&mut a, 300, 60), face, 30, 10, 40, 0xffffff, "Spiel ✓ ド");
            t.draw(&mut Canvas::new(&mut b, 300, 60), face, 30, 10, 40, 0xffffff, "Spiel  ?");
            assert!(a == b);
        }
    }

    /// The system's Noto Sans CJK for the tests: `POCKETUI_TEST_CJK_FONT`, else the one of an unpacked ROCKNIX image on
    /// the development computer. Without it the tests that need it say so and pass.
    pub(crate) fn test_cjk_font() -> Option<PathBuf> {
        let path = std::env::var_os("POCKETUI_TEST_CJK_FONT")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/tmp/rocknix-root-20261001").join(crate::text_fallback::SYSTEM_CJK.trim_start_matches('/')));
        if path.is_file() {
            Some(path)
        } else {
            eprintln!("SKIPPED: no Noto Sans CJK at {} (set POCKETUI_TEST_CJK_FONT to ROCKNIX's NotoSansCJKsc-Regular.otf)", path.display());
            None
        }
    }

    /// Where `s` puts ink, painted at `px` from x 10 on baseline 100: (x0, y0, x1, y1).
    fn ink(t: &mut Text, face: Face, px: u32, s: &str) -> (i32, i32, i32, i32) {
        let (w, h) = (1400, 200);
        let mut buf = vec![0u32; (w * h) as usize];
        t.draw(&mut Canvas::new(&mut buf, w as u32, h as u32), face, px, 10, 100, 0xffffff, s);
        let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0, 0);
        for (i, &p) in buf.iter().enumerate() {
            if p & 0xff > 96 {
                let (x, y) = (i as i32 % w, i as i32 / w);
                (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x + 1), y1.max(y + 1));
            }
        }
        (x0, y0, x1, y1)
    }

    #[test]
    fn a_japanese_letter_comes_from_the_fallback_font_and_latin_ones_stay_barlow() {
        let Some(font) = test_cjk_font() else { return };
        let mut plain = Text::builtin();
        let mut t = Text::builtin().with_fallback(Some(font));
        for face in [Face::Body, Face::Title] {
            // Latin letters and "?" itself, pixel for pixel as without the fallback.
            for s in ["Super Mario World", "Wi-Fi 12:34 ÄÖ ?"] {
                let (mut a, mut b) = (vec![0u32; 600 * 60], vec![0u32; 600 * 60]);
                t.draw(&mut Canvas::new(&mut a, 600, 60), face, 30, 10, 40, 0xffffff, s);
                plain.draw(&mut Canvas::new(&mut b, 600, 60), face, 30, 10, 40, 0xffffff, s);
                assert!(a == b, "{s}");
            }
            // Kana and kanji: each a glyph of its own with ink, no "?" any more.
            assert_ne!(t.width(face, 30, "ドラ"), t.width(face, 30, "??"));
            for ch in "ファイナルファンタジー国".chars() {
                let (x0, _, x1, _) = ink(&mut t, face, 37, &ch.to_string());
                assert!(x1 > x0, "{ch} has ink");
            }
            // Neither font has it (a game controller emoji): left out as before.
            assert_eq!(t.width(face, 30, "A🎮B"), t.width(face, 30, "AB"));
            assert_eq!(t.width(face, 30, "A\u{200b}B"), t.width(face, 30, "AB"), "a zero-width mark stays zero-width");
        }
        assert!(t.fallback_cache().1 > 20, "kept apart from Barlow's glyphs: {:?}", t.fallback_cache());
        assert_eq!(plain.fallback_cache(), (0, 0));
    }

    #[test]
    fn japanese_letters_stand_on_the_baseline_as_high_as_the_capitals_and_a_little_more() {
        let Some(font) = test_cjk_font() else { return };
        let mut t = Text::builtin().with_fallback(Some(font));
        for (face, px) in [(Face::Title, 37), (Face::Title, 67), (Face::Body, 28), (Face::Body, 36)] {
            let cap = ink(&mut t, face, px, "H");
            let kanji = ink(&mut t, face, px, "国");
            let (ch, kh) = (cap.3 - cap.1, kanji.3 - kanji.1);
            assert!(kh * 6 >= ch * 6 + ch / 2 && kh * 6 <= ch * 8, "{px}: 国 {kh} px high next to H {ch} px");
            let (cm, km) = (cap.1 + cap.3, kanji.1 + kanji.3);
            assert!((cm - km).abs() <= 2, "{px}: centred on the capitals: {cap:?} {kanji:?}");
            // A kana run and a Latin one on one line: no letter far above or below the other.
            let mixed = ink(&mut t, face, px, "ファイナルファンタジーVI");
            assert!(mixed.1 >= cap.1 - ch / 4 && mixed.3 <= cap.3 + ch / 4, "{px}: {mixed:?} next to H {cap:?}");
        }
        // The title face is the bolder one: its glyph has more ink at the same size.
        let ink_sum = |t: &mut Text, face: Face| {
            let mut buf = vec![0u32; 120 * 120];
            t.draw(&mut Canvas::new(&mut buf, 120, 120), face, 60, 10, 90, 0xffffff, "国");
            buf.iter().map(|p| p & 0xff).sum::<u32>()
        };
        assert!(ink_sum(&mut t, Face::Title) > ink_sum(&mut t, Face::Body));
    }

    #[test]
    fn without_the_fallback_font_a_japanese_name_is_painted_as_before() {
        let mut plain = Text::builtin();
        for path in [Some(PathBuf::from("/nonexistent/NotoSansCJKsc-Regular.otf")), None] {
            let mut t = Text::builtin().with_fallback(path.clone());
            for face in [Face::Body, Face::Title] {
                let s = "ファイナルファンタジーVI ✓";
                assert_eq!(t.width(face, 30, s), plain.width(face, 30, s), "{path:?}");
                assert_eq!(t.width(face, 30, "ドラ"), t.width(face, 30, "??"));
            }
            assert_eq!(t.fallback_cache(), (0, 0));
            t.prewarm_names(["スーパーマリオワールド"]);
            t.finish_prewarm();
            assert_eq!(t.fallback_cache(), (0, 0), "nothing drawn ahead either");
        }
    }

    #[test]
    fn the_fallback_glyphs_never_take_more_than_their_share_of_memory() {
        let Some(font) = test_cjk_font() else { return };
        let mut t = Text::builtin().with_fallback(Some(font));
        // Big kanji (the search's letter flash is 200 px): a few hundred pass the limit.
        let many: String = ('一'..='\u{4fff}').step_by(3).take(600).collect();
        let mut most = 0;
        for ch in many.chars() {
            t.width(Face::Title, 200, &ch.to_string());
            most = most.max(t.fallback_cache().0);
        }
        assert!(most <= FALLBACK_BYTES + 64 * 1024, "at most one glyph past the limit: {most}");
        assert!(t.fallback_cache().1 < 600, "emptied on the way: {:?}", t.fallback_cache());
        // Drawn again as they are needed, the same as the first time.
        let mut fresh = Text::builtin().with_fallback(test_cjk_font());
        assert_eq!(ink(&mut t, Face::Title, 50, "一丁"), ink(&mut fresh, Face::Title, 50, "一丁"));
    }

    #[test]
    #[ignore = "timing tier: scripts/test-timing.sh (serial)"]
    fn timing_the_letters_of_japanese_names_are_drawn_ahead_and_a_plain_library_starts_no_thread() {
        let mut t = Text::builtin().with_fallback(Some(PathBuf::from("/nonexistent/NotoSansCJKsc-Regular.otf")));
        t.prewarm_names(["Super Mario World", "Tetris", "Mega Man X"]);
        assert!(t.warm_names.is_none(), "only plain ASCII names: no thread");
        // Accents Barlow has: looked at on the thread, nothing kept, the font never needed.
        t.prewarm_names(["Pokémon Gold", "Ōkami"]);
        t.finish_prewarm();
        assert_eq!(t.fallback_cache(), (0, 0));
        let Some(font) = test_cjk_font() else { return };
        let names = ["ファイナルファンタジーVI", "スーパーマリオワールド", "ゼルダの伝説 神々のトライフォース", "Tetris"];
        let mut ahead = Text::builtin().with_fallback(Some(font.clone()));
        let t0 = std::time::Instant::now();
        ahead.prewarm_names(names);
        assert!(t0.elapsed() < crate::testutil::timeout(std::time::Duration::from_millis(20)), "the caller only starts a thread: {:?}", t0.elapsed());
        ahead.finish_prewarm();
        let before = ahead.drawn_on_the_spot();
        let mut plain = Text::builtin().with_fallback(Some(font));
        for name in names {
            assert_eq!(ink(&mut ahead, Face::Title, 37, name), ink(&mut plain, Face::Title, 37, name), "{name}");
        }
        let kana_drawn = ahead.drawn_on_the_spot() - before;
        assert!(plain.drawn_on_the_spot() > 25 + kana_drawn, "the CJK letters were there already ({kana_drawn} drawn, Barlow's own letters only)");
        let letters = letters_for_fallback(&ahead.title, &ahead.body, names.iter().copied(), 3);
        assert_eq!(letters.len(), 3);
        assert_eq!(letters[0], 'ー', "the most frequent first: {letters:?}");
    }

    /// Prints what the fallback font costs here (`cargo test --profile gate --lib fallback_cost -- --nocapture`).
    #[test]
    fn fallback_cost() {
        let Some(font) = test_cjk_font() else { return };
        let t0 = std::time::Instant::now();
        let f = crate::text_fallback::open(&font).expect("the font opens");
        let open = t0.elapsed();
        let fit = crate::text_fallback::Fit { scale: 0.9, rise: 0.0, bold: 0.028 };
        let letters: Vec<char> = "ファイナルファンタジースーパーマリオワールドゼルダの伝説神々のトライフォース".chars().collect();
        let t1 = std::time::Instant::now();
        for &ch in &letters {
            f.raster(ch, 37, fit);
        }
        let per37 = t1.elapsed() / letters.len() as u32;
        let t2 = std::time::Instant::now();
        for &ch in &letters {
            f.raster(ch, 67, fit);
        }
        let per67 = t2.elapsed() / letters.len() as u32;
        eprintln!("fallback cost: open {open:?} (first in this process: {}), a glyph at 37 px {per37:?}, at 67 px {per67:?}", open.as_micros() > 0);
    }

    #[test]
    fn glyphs_drawn_ahead_of_time_are_the_ones_a_frame_would_draw() {
        let mut ahead = Text::builtin();
        ahead.prewarm();
        let mut plain = Text::builtin();
        let (mut a, mut b) = (vec![0x12161bu32; 400 * 60], vec![0x12161bu32; 400 * 60]);
        // Wait for the thread (a glyph it has not drawn yet would be drawn on the spot, which proves nothing).
        ahead.finish_prewarm();
        ahead.draw(&mut Canvas::new(&mut a, 400, 60), Face::Body, 30, 10, 40, 0xece9e1, "Choose how the shelf looks, ‹ Dark ›");
        plain.draw(&mut Canvas::new(&mut b, 400, 60), Face::Body, 30, 10, 40, 0xece9e1, "Choose how the shelf looks, ‹ Dark ›");
        assert!(a == b);
        assert_eq!(ahead.drawn_on_the_spot(), 0, "every letter was there");
        assert!(plain.drawn_on_the_spot() > 10, "a Text without the thread draws each letter once");
    }

    #[test]
    fn a_word_wider_than_the_line_is_broken_after_a_slash_and_nothing_is_lost() {
        let mut t = Text::builtin();
        let path = "/storage/.config/pocket-ui.rescue-20261023-074130/and/a/very/long/folder/name";
        let lines = t.wrap(Face::Body, 28, &format!("Moved to {path}."), 400);
        assert!(lines.len() > 2, "{lines:?}");
        assert!(lines.iter().all(|l| t.width(Face::Body, 28, l) <= 400), "{lines:?}");
        assert_eq!(lines.concat().replace(' ', ""), format!("Movedto{path}."), "every letter is there, in order");
        assert!(lines[1..lines.len() - 1].iter().all(|l| l.ends_with(['/', '.', '-', '_'])), "broken after a mark: {lines:?}");
        assert_eq!(t.wrap(Face::Body, 28, "Short words only", 400), ["Short words only"], "as before when everything fits");
    }

    #[test]
    fn a_paragraph_given_fewer_lines_ends_its_last_one_with_an_ellipsis() {
        let mut t = Text::builtin();
        let s = "Nothing leaves the Pocket until you pair a device. BIOS stays home unless you share it.";
        let all = t.wrap(Face::Body, 28, s, 300);
        assert!(all.len() > 2);
        assert_eq!(t.wrap_at_most(Face::Body, 28, s, 300, 9), all, "room for all of them: as wrap");
        let two = t.wrap_at_most(Face::Body, 28, s, 300, 2);
        assert_eq!(two.len(), 2);
        assert_eq!(two[0], all[0]);
        assert!(two[1].ends_with('…') && t.width(Face::Body, 28, &two[1]) <= 300, "{two:?}");
        assert!(t.wrap_at_most(Face::Body, 28, s, 300, 0).is_empty());
    }

    /// How many pixels of `a` and `b` (painted at `px`) differ, with the pen on the same baseline, an ink pixel being
    /// one at half coverage or more; and the height of a capital in screen pixels.
    fn glyph_difference(t: &mut Text, face: Face, px: u32, a: char, b: char) -> (usize, usize) {
        let ((ma, ga), (mb, gb)) = (t.glyph(face, px, a).clone(), t.glyph(face, px, b).clone());
        let capital = t.glyph(face, px, 'H').0.height;
        // Ink at (x, y) from the pen on the baseline, y down, as `draw` places a glyph's rows.
        let at = |m: &Metrics, g: &[u8], x: i32, y: i32| {
            let (col, row) = (x - m.xmin, y + m.height as i32 + m.ymin);
            (0..m.width as i32).contains(&col) && (0..m.height as i32).contains(&row) && g[row as usize * m.width + col as usize] >= 128
        };
        let mut differ = 0;
        // Across both glyphs' ink wherever it starts: Departure Mono's letters stand a whole font pixel in (`xmin`).
        let (left, right) = (ma.xmin.min(mb.xmin) - 2, (ma.xmin + ma.width as i32).max(mb.xmin + mb.width as i32) + 2);
        for y in -(ma.height.max(mb.height) as i32) - 2..2 {
            for x in left..right {
                differ += usize::from(at(&ma, &ga, x, y) != at(&mb, &gb, x, y));
            }
        }
        (differ, capital)
    }

    /// Look-alike pairs and how many font pixels (n x n each) of ink they must differ by: the letters and digits of a
    /// pixel face that read as another (Retro clip, 06.11.2026; "Chrono Trigger" read "Ohrono", "description"
    /// "desoription", 07.11.2026). The small c is the o minus the three pixels of its right side, and the font's own
    /// outlines sit a few units off the pixel grid (a stroke of 102 units on a pixel of 92), so it gets two; the
    /// notch it had before was one.
    const LOOK_ALIKES: [(char, char, usize); 7] = [('8', 'S', 3), ('5', 'S', 3), ('0', 'O', 3), ('2', 'Z', 3), ('8', 'B', 3), ('C', 'O', 3), ('c', 'o', 2)];
    /// The sizes the pixel faces are painted at: the status corner, rows, headings, the titles on cards (50 and 67, a
    /// Large text and 1240x1080 grow them) and the biggest a title gets.
    const LOOK_ALIKE_SIZES: [u32; 15] = [21, 28, 30, 37, 44, 46, 50, 56, 67, 80, 96, 120, 150, 180, 240];

    /// Retro's letters as the Retro clip showed them (06.11.2026): Pixelify between whole pixels per font pixel came
    /// out with grey edges and strokes of two and three pixels, its "8" read as "S", "Z" as "2", "C" as "O". At every
    /// size, from the pause menu's notes to the biggest title, every pixel is ink or not, and the look-alikes differ by
    /// whole font pixels.
    /// Both of Retro's letters (`font_style::RetroLetters`, 0.3.2): Departure Mono, the default, and Pixelify Sans.
    #[test]
    fn retro_letters_are_crisp_and_digits_tell_apart_from_letters() {
        // Pixelify's capitals are seven font pixels high, Departure Mono's eight.
        for (letters, rows) in [(RetroLetters::Departure, 8), (RetroLetters::Pixelify, 7)] {
            font_style::set_retro_letters(letters);
            let mut t = Text::builtin().pinned_to(FontStyle::Retro);
            for face in [Face::Body, Face::Title] {
                for px in LOOK_ALIKE_SIZES {
                    for ch in "Slot 3 · heute 11:41 48% Zurück SELECT Chrono Trigger description ñ¿¡".chars() {
                        let (_, bitmap) = t.glyph(face, px, ch).clone();
                        assert!(bitmap.iter().all(|&v| v == 0 || v == 255), "{letters:?} {face:?} {px} px {ch:?}: grey edges");
                    }
                    for (a, b, pixels) in LOOK_ALIKES {
                        let (differ, capital) = glyph_difference(&mut t, face, px, a, b);
                        // Whole font pixels: a capital is `rows` of them high.
                        let n = (capital / rows).max(1);
                        assert!(differ >= pixels * n * n, "{letters:?} {face:?} {px} px: {a:?} and {b:?} differ by {differ} pixels only (font pixel {n})");
                    }
                }
            }
        }
        font_style::set_retro_letters(RetroLetters::Departure);
    }

    /// Retro's letters as painted (0.3.2): Departure Mono by default, every letter as far on as the next and every font
    /// pixel the same whole number of screen pixels; a Text in use follows a change to Pixelify by itself.
    #[test]
    fn retro_paints_departure_mono_on_its_grid_and_follows_a_change_to_pixelify() {
        font_style::set_retro_letters(RetroLetters::Departure);
        let mut t = Text::builtin().pinned_to(FontStyle::Retro);
        for face in [Face::Body, Face::Title] {
            for px in LOOK_ALIKE_SIZES {
                let (narrow, wide) = (t.width(face, px, "iiiiiiii"), t.width(face, px, "MMMMMMMM"));
                assert_eq!(narrow, wide, "{face:?} {px} px: Departure Mono's letters are all as wide");
                let (m, _) = t.glyph(face, px, 'H').clone();
                assert_eq!(m.height % 8, 0, "{face:?} {px} px: a capital of {} pixels is not 8 whole font pixels", m.height);
                assert_eq!(m.advance_width as usize % (m.height / 8), 0, "{face:?} {px} px: letters a whole number of font pixels apart");
            }
        }
        // A 640x480 screen shows the 1280x960 canvas at half (`scaled_layout::step`): every font pixel an even number of
        // canvas pixels, whole pixels of the screen, never one and a half; the Text in use draws its glyphs anew.
        crate::scaled_layout::set_step((1280, 960), (640, 480));
        for face in [Face::Body, Face::Title] {
            for px in LOOK_ALIKE_SIZES {
                let (m, g) = t.glyph(face, px, 'H').clone();
                let n = m.height / 8;
                assert_eq!((m.height % 8, n % 2), (0, 0), "{face:?} {px} px on 640x480: a font pixel of {n} canvas pixels");
                assert!(g.iter().all(|&v| v == 0 || v == 255), "{face:?} {px} px on 640x480: grey edges");
            }
        }
        // And where they land: drawn at an odd place, every font pixel still fills a whole 2x2 block of the canvas, one
        // pixel of the screen (the 640x480 picture showed half-grey letters before).
        for (face, px) in [(Face::Body, 28), (Face::Title, 37), (Face::Body, 46)] {
            let mut buf = vec![0u32; 320 * 90];
            t.draw(&mut Canvas::new(&mut buf, 320, 90), face, px, 11, 61, 0xffffff, "Systems 8 Shelf");
            let at = |x: usize, y: usize| buf[y * 320 + x];
            for y in (0..90).step_by(2) {
                for x in (0..320).step_by(2) {
                    let block = [at(x, y), at(x + 1, y), at(x, y + 1), at(x + 1, y + 1)];
                    assert!(block.iter().all(|&v| v == block[0]), "{face:?} {px} px: half a screen pixel at ({x}, {y}): {block:x?}");
                }
            }
        }
        crate::scaled_layout::set_step((1024, 768), (1024, 768));
        assert_eq!(t.glyph(Face::Title, 37, 'H').0.height, 24, "a native screen: three canvas pixels per font pixel at 37 px");
        font_style::set_retro_letters(RetroLetters::Pixelify);
        let (narrow, wide) = (t.width(Face::Body, 28, "iiiiiiii"), t.width(Face::Body, 28, "MMMMMMMM"));
        assert!(narrow < wide, "Pixelify chosen: its narrow i ({narrow} against {wide})");
        font_style::set_retro_letters(RetroLetters::Departure);
    }

    /// The Pixel look (Settings → Look → Font) paints Pixelify Sans' titles as the font draws them, at any size: its
    /// C, c, Z and 5 are the letters the Retro style's pixels use (`scripts/pixelify-glyphs.py`), so those pairs
    /// differ by whole font pixels at every size as well.
    #[test]
    fn pixel_font_look_alikes_tell_apart_at_every_size() {
        let mut t = Text::builtin().pinned_to(FontStyle::Pixel);
        for px in LOOK_ALIKE_SIZES {
            for (a, b, pixels) in LOOK_ALIKES.into_iter().filter(|&(a, b, _)| "Cc5".contains(a) || b == 'Z') {
                let (differ, _) = glyph_difference(&mut t, Face::Title, px, a, b);
                let font_pixel = px as f32 * crate::font_style::PIXELIFY_PIXEL;
                assert!(differ as f32 >= pixels as f32 * font_pixel * font_pixel, "Pixel {px} px: {a:?} and {b:?} differ by {differ} pixels only (font pixel {font_pixel})");
            }
        }
    }
}
