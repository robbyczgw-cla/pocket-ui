//! Settings → Look → Style (settings.json `"font"`): the type pocket-ui paints with, Barlow or Retro. Retro is more
//! than type: it switches the whole Retro style on (`retro`). Rounded and Pixel went with 0.4; a saved one is Barlow.
//!
//! | Style | Titles, spines, labels (`Face::Title`) | Meta text, hints (`Face::Body`) |
//! |---|---|---|
//! | Barlow (default) | Barlow Condensed SemiBold | Barlow Medium |
//! | Retro | Departure Mono | Departure Mono |
//! | Retro Pixelify (`RetroLetters::Pixelify`) | Pixelify Sans SemiBold | Pixelify Sans Regular |
//!
//! Retro's letters are a choice of their own (settings.json `"retro_letters"`, `RetroLetters`): Departure Mono since
//! 0.3.2, or Pixelify Sans as before. The Style row shows it as a third value, "Retro Pixelify", right after "Retro"
//! (`ROW`), so that Look keeps its nine rows.
//!
//! The fonts are SIL OFL 1.1, compiled in as static, Latin-only instances (`assets/fonts/`, 104 KB for the two new
//! families); `assets/fonts/OFL.txt` carries the licences. A style's faces are parsed the first time it is used, once per
//! process, never per frame. **Fitted to Barlow:** each alternative face is painted at a size scaled so that its capitals
//! stand as high as the Barlow face's at the same size ([`Alt::scale`], the same idea as `text_fallback::Fit` for CJK
//! letters), so baselines, `type_scale` sizes and row heights built for Barlow keep working; only the widths differ
//! (`Text::fit`, `wrap` and the painters' measuring deal with that).
//!
//! The style is process wide like `button_layout`: `set` at the start (settings.json `"font"`) and from the Theme page,
//! and every `Text` notices the change by one atomic compare at its next glyph, swaps its faces and drops its glyph cache.
//! A store theme's `fonts/title.ttf` and `body.ttf` still win over the built-in style, and `Text::builtin()` (safe mode)
//! stays Barlow.

use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, OnceLock};

use fontdue::{Font, FontSettings};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FontStyle {
    /// Barlow Condensed SemiBold and Barlow Medium, as before the choice existed.
    /// Also what a settings file says for the styles that went with 0.4, Rounded and Pixel: their letters were type
    /// on the Barlow page, not the Retro style's colours and frames.
    #[default]
    #[serde(alias = "rounded", alias = "pixel")]
    Barlow,
    /// Retro (`retro`): pixel letters for both faces (`RetroLetters`), and with it the Retro style's colours, pixel frames
    /// and icons.
    Retro,
}

/// The Retro style's letters (settings.json `"retro_letters"`): Departure Mono when unset, so whoever has Retro on gets
/// it with 0.3.2; Pixelify Sans, Retro's letters before, stays a choice.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RetroLetters {
    /// Departure Mono: even, square letters on an 11-pixel grid, every letter as wide as the next.
    #[default]
    Departure,
    /// Pixelify Sans, rounder and narrower.
    Pixelify,
}

impl RetroLetters {
    pub const ALL: [RetroLetters; 2] = [RetroLetters::Departure, RetroLetters::Pixelify];

    /// `POCKETUI_RETRO_LETTERS=departure|pixelify` for the snapshots and the text-size runs; `None` when unset or wrong.
    pub fn from_env() -> Option<RetroLetters> {
        match std::env::var("POCKETUI_RETRO_LETTERS").ok()?.trim().to_ascii_lowercase().as_str() {
            "departure" => Some(RetroLetters::Departure),
            "pixelify" => Some(RetroLetters::Pixelify),
            _ => None,
        }
    }

    fn from_u8(v: u8) -> RetroLetters {
        Self::ALL.get(v as usize).copied().unwrap_or_default()
    }
}

/// Settings → Look → Style in order, with one row for each of Retro's letter sets.
pub const ROW: [(FontStyle, RetroLetters); 3] = [
    (FontStyle::Barlow, RetroLetters::Departure),
    (FontStyle::Retro, RetroLetters::Departure),
    (FontStyle::Retro, RetroLetters::Pixelify),
];

/// Where on `ROW` the style in use stands (the letters count only for Retro).
fn row_index() -> usize {
    let (style, letters) = (get(), retro_letters());
    ROW.iter().position(|&(s, l)| s == style && (s != FontStyle::Retro || l == letters)).unwrap_or(0)
}

/// The Style row's value: the style's name, "Retro Pixelify" for Retro in Pixelify's letters.
pub fn row_label() -> &'static str {
    match ROW[row_index()] {
        (FontStyle::Retro, RetroLetters::Pixelify) => "Retro Pixelify",
        (s, _) => s.label(),
    }
}

/// The next (`forward`) or previous value of the Style row, used from the next glyph on: the style, and Retro's
/// letters when it lands on Retro (the other styles keep them for the next time).
pub fn step_row(forward: bool) {
    let n = ROW.len();
    let i = row_index();
    let (style, letters) = ROW[if forward { (i + 1) % n } else { (i + n - 1) % n }];
    if style == FontStyle::Retro {
        set_retro_letters(letters);
    }
    set(style);
}

impl FontStyle {
    pub const ALL: [FontStyle; 2] = [FontStyle::Barlow, FontStyle::Retro];

    /// The value on the Settings row.
    pub fn label(self) -> &'static str {
        match self {
            FontStyle::Barlow => "Barlow",
            FontStyle::Retro => "Retro",
        }
    }

    /// One short line under the row.
    pub fn blurb(self) -> &'static str {
        match self {
            FontStyle::Barlow => "Clean and narrow, the look pocket-ui started with.",
            FontStyle::Retro => "Pixel letters, frames and icons.",
        }
    }

    /// The next style in `ALL`, wrapping around.
    pub fn next(self) -> FontStyle {
        let i = Self::ALL.iter().position(|s| *s == self).unwrap_or(0);
        Self::ALL[(i + 1) % Self::ALL.len()]
    }

    /// The style before it in `ALL`, wrapping around (Left on the Theme page).
    pub fn prev(self) -> FontStyle {
        let i = Self::ALL.iter().position(|s| *s == self).unwrap_or(0);
        Self::ALL[(i + Self::ALL.len() - 1) % Self::ALL.len()]
    }

    /// The style named `name` (`POCKETUI_FONT=barlow|pixel|retro`, case ignored).
    pub fn parse(name: &str) -> Option<FontStyle> {
        Self::ALL.into_iter().find(|s| s.label().eq_ignore_ascii_case(name.trim()))
    }

    fn to_u8(self) -> u8 {
        self as u8
    }

    fn from_u8(v: u8) -> FontStyle {
        Self::ALL.into_iter().find(|s| s.to_u8() == v).unwrap_or_default()
    }

    /// The faces of this style besides Barlow's own, parsed the first time (`None` for Barlow).
    pub(crate) fn fitted(self) -> Option<&'static Fitted> {
        static RETRO: OnceLock<Fitted> = OnceLock::new();
        static RETRO_DEPARTURE: OnceLock<Fitted> = OnceLock::new();
        match self {
            FontStyle::Retro if retro_letters() == RetroLetters::Departure => Some(RETRO_DEPARTURE.get_or_init(|| {
                // One face for titles and body text, on its 11-pixel grid like Pixelify's (`Alt::grid`), with its own
                // digits and capitals: they tell apart without the LCD set (`Alt::lcd`). Its letters stand one font
                // pixel apart instead of two (`Alt::tight`) and a little lower than Barlow's capitals: every letter
                // as wide as an "M" ran 35 % wider than Pixelify, and pages laid out for Barlow and Pixelify cut lines
                // at 1024x768 in Large text (the text-size test's Retro runs, 09.11.2026).
                let mut f = Fitted::new(Some(DEPARTURE), Some(DEPARTURE));
                for alt in [f.title.as_mut(), f.body.as_mut()].into_iter().flatten() {
                    alt.grid = DEPARTURE_PIXEL;
                    alt.tight = true;
                }
                if let Some(title) = f.title.as_mut() {
                    title.scale *= DEPARTURE_TITLE;
                }
                if let Some(body) = f.body.as_mut() {
                    body.scale *= DEPARTURE_BODY;
                }
                f
            })),
            FontStyle::Retro => Some(RETRO.get_or_init(|| {
                let mut f = Fitted::new(Some(include_bytes!("../assets/fonts/PixelifySans-SemiBold.ttf")), Some(include_bytes!("../assets/fonts/PixelifySans-Regular.ttf")));
                // Retro paints Pixelify on its own pixel grid: whole screen pixels per font pixel, no grey edges (`Alt::grid`).
                for alt in [f.title.as_mut(), f.body.as_mut()].into_iter().flatten() {
                    alt.grid = PIXELIFY_PIXEL;
                    alt.lcd = true;
                }
                // Pixelify's letters run 21 % wider than Barlow Medium's at equal capitals: the body text stands a tenth
                // lower, so pages laid out for Barlow keep their lines (docs/RETRO-STYLE.md, the text-size test's Retro run).
                if let Some(body) = f.body.as_mut() {
                    body.scale *= RETRO_BODY;
                }
                f
            })),
            FontStyle::Barlow => None,
        }
    }
}

/// How high Retro's body text stands against Barlow's capitals in Pixelify's letters (`fitted`).
pub(crate) const RETRO_BODY: f32 = 0.9;
/// The same in Departure Mono's, and its titles against Barlow Condensed's capitals: its letters, one font pixel apart,
/// then run about as wide as Pixelify's at the sizes pocket-ui paints (`fitted`).
pub(crate) const DEPARTURE_BODY: f32 = 0.85;
pub(crate) const DEPARTURE_TITLE: f32 = 0.9;

/// `POCKETUI_RETRO_TRACKING=wide`: Departure Mono's letters as far apart as the font sets them (`Alt::tight` off for
/// the spacing only), for comparing pictures; read once.
pub(crate) fn departure_wide() -> bool {
    static WIDE: OnceLock<bool> = OnceLock::new();
    *WIDE.get_or_init(|| std::env::var("POCKETUI_RETRO_TRACKING").is_ok_and(|v| v.trim().eq_ignore_ascii_case("wide")))
}

/// Departure Mono 1.500, the Retro style's letters (`RetroLetters::Departure`).
pub(crate) const DEPARTURE: &[u8] = include_bytes!("../assets/fonts/DepartureMono-Regular.otf");
/// One font pixel of Departure Mono in ems: 50 of its 550 units, the 11-pixel grid it is drawn on (capitals eight high).
pub(crate) const DEPARTURE_PIXEL: f32 = 50.0 / 550.0;

pub(crate) const BARLOW_TITLE: &[u8] = include_bytes!("../assets/fonts/BarlowCondensed-SemiBold.ttf");
pub(crate) const BARLOW_BODY: &[u8] = include_bytes!("../assets/fonts/Barlow-Medium.ttf");

/// One font pixel of Pixelify Sans in ems: its capitals stand seven pixels high (an "H" spans 643 of 1000 units).
pub(crate) const PIXELIFY_PIXEL: f32 = 643.0 / 7.0 / 1000.0;

/// One alternative face and the factor that fits it to Barlow's.
pub(crate) struct Alt {
    pub font: Arc<Font>,
    /// The face is painted at `px * scale`, so that its capitals are as high as Barlow's at `px`.
    pub scale: f32,
    /// A pixel font's pixel in ems (`PIXELIFY_PIXEL`), 0 for any other face: then `text::raster` rounds the size to
    /// whole screen pixels per font pixel and paints the letters without grey edges. Pixelify between two such sizes
    /// came out with strokes of two and three pixels and blurred edges: "Z" read as "2", digits were hard to tell
    /// apart, worst at 640x480 (Retro clip, 06.11.2026).
    pub grid: f32,
    /// Retro paints this pixel face's digits and its look-alike capitals from the LCD set (`retro::LCD`,
    /// `retro::lcd_letter`): Pixelify's, whose own read as other letters; Departure Mono's are its own.
    pub lcd: bool,
    /// Departure Mono's way (`text::raster`): its letters one font pixel closer than the font sets them where their ink
    /// leaves the room (two pixels between letters become one; "%", "&", "→", which fill their cell, keep theirs), and
    /// every font pixel a whole number of the *screen's* pixels, a step smaller when in between (`scaled_layout::step`:
    /// a 640x480 panel shows half the canvas).
    pub tight: bool,
}

/// A style's faces: `None` keeps Barlow's own for that face.
pub(crate) struct Fitted {
    pub title: Option<Alt>,
    pub body: Option<Alt>,
}

impl Fitted {
    fn new(title: Option<&'static [u8]>, body: Option<&'static [u8]>) -> Fitted {
        let face = |bytes: &'static [u8], barlow: &[u8]| {
            let font = Font::from_bytes(bytes, FontSettings::default()).expect("embedded font");
            Alt { font: Arc::new(font), scale: fit_scale(bytes, barlow), grid: 0.0, lcd: false, tight: false }
        };
        Fitted { title: title.map(|b| face(b, BARLOW_TITLE)), body: body.map(|b| face(b, BARLOW_BODY)) }
    }
}

/// The height of an "H" (its bounds, bottom to top) in ems.
fn cap_height(font: &[u8]) -> Option<f32> {
    let face = ttf_parser::Face::parse(font, 0).ok()?;
    let r = face.glyph_bounding_box(face.glyph_index('H')?)?;
    Some((r.y_max - r.y_min) as f32 / face.units_per_em() as f32)
}

/// How much larger than Barlow's size `font` is painted to have capitals as high as `barlow`'s: the bounds of an "H"
/// are made equal, so the capitals' top and bottom are each within a fraction of a pixel of Barlow's at the sizes
/// pocket-ui paints (a pixel font's stems dip a hair under the baseline, which the fit shares out).
pub(crate) fn fit_scale(font: &[u8], barlow: &[u8]) -> f32 {
    match (cap_height(font), cap_height(barlow)) {
        (Some(f), Some(b)) if f > 0.0 => b / f,
        _ => 1.0,
    }
}

/// The style in use. Process wide, because `Text`s on several threads read it and the UI thread sets it. Per thread in
/// the unit tests, which run side by side (the Theme page's test switches it).
#[cfg(not(test))]
static STYLE: AtomicU8 = AtomicU8::new(0);
#[cfg(test)]
thread_local! {
    static STYLE: AtomicU8 = const { AtomicU8::new(0) };
}

/// Retro's letters (`RetroLetters`), process wide as the style is; kept while another style is in use.
#[cfg(not(test))]
static LETTERS: AtomicU8 = AtomicU8::new(0);
#[cfg(test)]
thread_local! {
    static LETTERS: AtomicU8 = const { AtomicU8::new(0) };
}

/// Retro's letters from the next glyph on (settings.json, the Style row); every `Text` in Retro follows by itself.
pub fn set_retro_letters(l: RetroLetters) {
    #[cfg(not(test))]
    LETTERS.store(l as u8, Ordering::Relaxed);
    #[cfg(test)]
    LETTERS.with(|v| v.store(l as u8, Ordering::Relaxed));
}

pub fn retro_letters() -> RetroLetters {
    #[cfg(not(test))]
    let v = LETTERS.load(Ordering::Relaxed);
    #[cfg(test)]
    let v = LETTERS.with(|v| v.load(Ordering::Relaxed));
    RetroLetters::from_u8(v)
}

/// The style in use from the next glyph on (Settings, the start); every `Text` follows by itself.
pub fn set(s: FontStyle) {
    #[cfg(not(test))]
    STYLE.store(s.to_u8(), Ordering::Relaxed);
    #[cfg(test)]
    STYLE.with(|v| v.store(s.to_u8(), Ordering::Relaxed));
}

pub fn get() -> FontStyle {
    #[cfg(not(test))]
    let v = STYLE.load(Ordering::Relaxed);
    #[cfg(test)]
    let v = STYLE.with(|v| v.load(Ordering::Relaxed));
    FontStyle::from_u8(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn barlow_is_the_default_and_the_names_are_plain() {
        assert_eq!(FontStyle::default(), FontStyle::Barlow);
        let labels: Vec<_> = FontStyle::ALL.iter().map(|s| s.label()).collect();
        assert_eq!(labels, ["Barlow", "Retro"]);
        for s in FontStyle::ALL {
            assert!(!s.blurb().is_empty() && s.blurb().len() < 80, "{s:?}");
            assert_eq!(FontStyle::from_u8(s.to_u8()), s);
            assert_eq!(FontStyle::parse(s.label()), Some(s));
            assert_eq!(FontStyle::parse(&s.label().to_lowercase()), Some(s));
        }
        assert_eq!(FontStyle::parse("comic"), None);
        assert_eq!(FontStyle::from_u8(200), FontStyle::Barlow, "a wrong number is Barlow");
        assert_eq!(FontStyle::Barlow.next().next(), FontStyle::Barlow);
    }

    /// Retro's letters (0.3.2): Departure Mono unless Pixelify is chosen, and on the Style row Pixelify is one step on
    /// from Retro, one step back is Departure Mono again; another style leaves the letters for the next time.
    #[test]
    fn retro_is_departure_mono_and_pixelify_is_one_step_on_the_style_row() {
        assert_eq!(RetroLetters::default(), RetroLetters::Departure);
        set(FontStyle::Retro);
        set_retro_letters(RetroLetters::default());
        assert_eq!(row_label(), "Retro");
        assert!(std::ptr::eq(FontStyle::Retro.fitted().unwrap(), FontStyle::Retro.fitted().unwrap()), "parsed once");
        let departure = FontStyle::Retro.fitted().unwrap().body.as_ref().unwrap().font.clone();
        assert_eq!(departure.metrics('i', 22.0).advance_width, departure.metrics('M', 22.0).advance_width, "Departure Mono: every letter as wide");
        step_row(true);
        assert_eq!((get(), retro_letters(), row_label()), (FontStyle::Retro, RetroLetters::Pixelify, "Retro Pixelify"));
        let pixelify = FontStyle::Retro.fitted().unwrap().body.as_ref().unwrap();
        assert!(pixelify.lcd && pixelify.font.metrics('i', 22.0).advance_width < pixelify.font.metrics('M', 22.0).advance_width, "Pixelify's own letters");
        step_row(false);
        assert_eq!((get(), retro_letters(), row_label()), (FontStyle::Retro, RetroLetters::Departure, "Retro"));
        // All the way round: each value once, Barlow after Retro Pixelify.
        set_retro_letters(RetroLetters::Pixelify);
        step_row(true);
        assert_eq!(get(), FontStyle::Barlow);
        let mut seen = vec![row_label()];
        for _ in 1..ROW.len() {
            step_row(true);
            seen.push(row_label());
        }
        assert_eq!(seen, ["Barlow", "Retro", "Retro Pixelify"]);
        set(FontStyle::Barlow);
        set_retro_letters(RetroLetters::Departure);
    }

    #[test]
    fn every_font_has_the_german_letters_and_marks_barlow_has() {
        let need = "ÄÖÜäöüß„“”–…·‹›×°";
        let barlow = [BARLOW_TITLE, BARLOW_BODY];
        for (s, letters) in [(FontStyle::Retro, RetroLetters::Departure), (FontStyle::Retro, RetroLetters::Pixelify)] {
            set_retro_letters(letters);
            let fitted = s.fitted().unwrap();
            for face in [&fitted.title, &fitted.body].into_iter().flatten() {
                for ch in need.chars().chain('!'..='~') {
                    assert_ne!(face.font.lookup_glyph_index(ch), 0, "{s:?} {letters:?} lacks {ch:?}");
                }
                // Whatever Barlow can paint, the alternative can (the arrow is in neither: both leave it out).
                for b in barlow {
                    let b = Font::from_bytes(b, FontSettings::default()).unwrap();
                    for ch in "ÄÖÜäöüß…‹›·–×°→".chars() {
                        assert!(face.font.lookup_glyph_index(ch) != 0 || b.lookup_glyph_index(ch) == 0, "{s:?} lacks {ch:?}");
                    }
                }
            }
        }
    }

    /// A letter a font lacks comes out as a hole in a word, in the fonts of Settings → Look → Style as much as in Barlow.
    #[test]
    fn every_font_has_every_letter_of_the_spanish_table() {
        let table = include_str!("../assets/i18n/es.tsv");
        let translated = table.lines().filter(|l| !l.starts_with('#')).filter_map(|l| l.split_once('\t')).flat_map(|(_, t)| t.chars());
        let mut need: Vec<char> = "ñÑ¿¡áéíóúüÁÉÍÓÚÜ“”…".chars().chain(translated.filter(|c| !c.is_ascii())).collect();
        need.sort_unstable();
        need.dedup();
        let barlow = [BARLOW_TITLE, BARLOW_BODY].map(|b| Font::from_bytes(b, FontSettings::default()).unwrap());
        let mut faces: Vec<(String, &Font)> = barlow.iter().map(|f| ("Barlow".to_string(), f)).collect();
        for (s, letters) in [(FontStyle::Retro, RetroLetters::Departure), (FontStyle::Retro, RetroLetters::Pixelify)] {
            set_retro_letters(letters);
            let fitted = s.fitted().unwrap();
            faces.extend([&fitted.title, &fitted.body].into_iter().flatten().map(|f| (format!("{s:?} {letters:?}"), &*f.font)));
        }
        let missing: Vec<String> = faces.iter().flat_map(|(name, font)| need.iter().filter(|c| font.lookup_glyph_index(**c) == 0).map(move |c| format!("{name} lacks {c:?}"))).collect();
        assert!(missing.is_empty(), "{}", missing.join("\n"));
    }
}
