//! Settings → Look → Font: the type pocket-ui paints with, one of three styles.
//!
//! | Style | Titles, spines, labels (`Face::Title`) | Meta text, hints (`Face::Body`) |
//! |---|---|---|
//! | Barlow (default) | Barlow Condensed SemiBold | Barlow Medium |
//! | Pixel | Pixelify Sans SemiBold | Barlow Medium (stays readable) |
//! | Rounded | Nunito ExtraBold | Nunito SemiBold |
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
    #[default]
    Barlow,
    /// A pixel font for titles, spines and labels; the body text stays Barlow Medium.
    Pixel,
    /// Nunito for both faces.
    Rounded,
}

impl FontStyle {
    pub const ALL: [FontStyle; 3] = [FontStyle::Barlow, FontStyle::Pixel, FontStyle::Rounded];

    /// The value on the Settings row.
    pub fn label(self) -> &'static str {
        match self {
            FontStyle::Barlow => "Barlow",
            FontStyle::Pixel => "Pixel",
            FontStyle::Rounded => "Rounded",
        }
    }

    /// One short line under the row.
    pub fn blurb(self) -> &'static str {
        match self {
            FontStyle::Barlow => "Clean and narrow, the look pocket-ui started with.",
            FontStyle::Pixel => "Pixel letters for titles and labels; the small text stays plain.",
            FontStyle::Rounded => "Soft, round letters everywhere.",
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

    /// The style named `name` (`POCKETUI_FONT=barlow|pixel|rounded`, case ignored).
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
        static PIXEL: OnceLock<Fitted> = OnceLock::new();
        static ROUNDED: OnceLock<Fitted> = OnceLock::new();
        match self {
            FontStyle::Barlow => None,
            FontStyle::Pixel => Some(PIXEL.get_or_init(|| {
                Fitted::new(Some(include_bytes!("../assets/fonts/PixelifySans-SemiBold.ttf")), None)
            })),
            FontStyle::Rounded => Some(ROUNDED.get_or_init(|| {
                Fitted::new(Some(include_bytes!("../assets/fonts/Nunito-ExtraBold.ttf")), Some(include_bytes!("../assets/fonts/Nunito-SemiBold.ttf")))
            })),
        }
    }
}

pub(crate) const BARLOW_TITLE: &[u8] = include_bytes!("../assets/fonts/BarlowCondensed-SemiBold.ttf");
pub(crate) const BARLOW_BODY: &[u8] = include_bytes!("../assets/fonts/Barlow-Medium.ttf");

/// One alternative face and the factor that fits it to Barlow's.
pub(crate) struct Alt {
    pub font: Arc<Font>,
    /// The face is painted at `px * scale`, so that its capitals are as high as Barlow's at `px`.
    pub scale: f32,
}

/// A style's faces: `None` keeps Barlow's own for that face (Pixel's body).
pub(crate) struct Fitted {
    pub title: Option<Alt>,
    pub body: Option<Alt>,
}

impl Fitted {
    fn new(title: Option<&'static [u8]>, body: Option<&'static [u8]>) -> Fitted {
        let face = |bytes: &'static [u8], barlow: &[u8]| {
            let font = Font::from_bytes(bytes, FontSettings::default()).expect("embedded font");
            Alt { font: Arc::new(font), scale: fit_scale(bytes, barlow) }
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
        assert_eq!(labels, ["Barlow", "Pixel", "Rounded"]);
        for s in FontStyle::ALL {
            assert!(!s.blurb().is_empty() && s.blurb().len() < 80, "{s:?}");
            assert_eq!(FontStyle::from_u8(s.to_u8()), s);
            assert_eq!(FontStyle::parse(s.label()), Some(s));
            assert_eq!(FontStyle::parse(&s.label().to_lowercase()), Some(s));
        }
        assert_eq!(FontStyle::parse("comic"), None);
        assert_eq!(FontStyle::from_u8(200), FontStyle::Barlow, "a wrong number is Barlow");
        assert_eq!(FontStyle::Barlow.next().next().next(), FontStyle::Barlow);
    }

    #[test]
    fn every_font_has_the_german_letters_and_marks_barlow_has() {
        let need = "ÄÖÜäöüß„“”–…·‹›×°";
        let barlow = [BARLOW_TITLE, BARLOW_BODY];
        for s in [FontStyle::Pixel, FontStyle::Rounded] {
            let fitted = s.fitted().unwrap();
            for face in [&fitted.title, &fitted.body].into_iter().flatten() {
                for ch in need.chars().chain('!'..='~') {
                    assert_ne!(face.font.lookup_glyph_index(ch), 0, "{s:?} lacks {ch:?}");
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

    #[test]
    fn the_capitals_are_as_high_as_barlows_within_a_pixel() {
        for (s, face, barlow) in [
            (FontStyle::Pixel, FontStyle::Pixel.fitted().unwrap().title.as_ref().unwrap(), BARLOW_TITLE),
            (FontStyle::Rounded, FontStyle::Rounded.fitted().unwrap().title.as_ref().unwrap(), BARLOW_TITLE),
            (FontStyle::Rounded, FontStyle::Rounded.fitted().unwrap().body.as_ref().unwrap(), BARLOW_BODY),
        ] {
            let reference = Font::from_bytes(barlow, FontSettings::default()).unwrap();
            for px in [21.0f32, 28.0, 30.0, 36.0, 37.0, 50.0, 67.0] {
                let want = reference.metrics('H', px);
                let got = face.font.metrics('H', px * face.scale);
                let (top_w, top_g) = (want.height as f32 + want.ymin as f32, got.height as f32 + got.ymin as f32);
                assert!((top_w - top_g).abs() <= 1.0, "{s:?} at {px}: the capitals end {top_g} high, Barlow's {top_w}");
                assert!((want.height as f32 - got.height as f32).abs() <= 1.0, "{s:?} at {px}: {} high, Barlow's {}", got.height, want.height);
            }
        }
    }
}
