//! Settings → Look & feel → Look → Theme → **Button symbols**: how the hints show the four face buttons.
//!
//! pocket-ui's hints speak in roles (A confirms, B goes back, X and Y are the two extra actions, `button_layout`);
//! this only decides how those four keys look wherever a key is drawn (the hint bars of both families, dialogs, the
//! manual's key chips and its controls picture):
//!
//! | Style | A | B | X | Y |
//! |---|---|---|---|---|
//! | Nintendo | A | B | X | Y, as today |
//! | Xbox | A on green | B on red | X on blue | Y on yellow |
//! | PlayStation | ✕ | ○ | □ | △, shapes drawn here (no logo, no font) |
//!
//! The PlayStation shapes stand where an Xbox pad has the letter of the same role (✕ is A, ○ B, □ X, △ Y). Which
//! physical button confirms stays Settings → Playing → Button layout's business: the two are independent.
//!
//! The shapes are four characters of Unicode's private use area (`CROSS` ...) that `Text` rasterises with `raster`
//! instead of a font, at the size and cap height of the face they are drawn in, and caches like any letter: measuring,
//! fitting and the hint bars' label rules treat them as one letter each. An unset or wrong value follows the device:
//! Xbox on Retroid and AYN handhelds, Nintendo on Anbernic's and anything unknown. A saved choice wins.

use std::sync::atomic::{AtomicU8, Ordering};

use fontdue::Metrics;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PromptStyle {
    /// The letters as they are, on the hint's own key cap.
    Nintendo,
    /// The letters on caps in Xbox's four colours.
    Xbox,
    /// ✕ ○ □ △ instead of the letters.
    #[serde(rename = "playstation")]
    PlayStation,
}

impl Default for PromptStyle {
    fn default() -> Self {
        crate::device::current().prompt_style()
    }
}

impl PromptStyle {
    pub const ALL: [PromptStyle; 3] = [PromptStyle::Nintendo, PromptStyle::Xbox, PromptStyle::PlayStation];

    /// The value on the Theme page.
    pub fn label(self) -> &'static str {
        match self {
            PromptStyle::Nintendo => "Nintendo",
            PromptStyle::Xbox => "Xbox",
            PromptStyle::PlayStation => "PlayStation",
        }
    }

    /// The line under the Theme page's list.
    pub fn blurb(self) -> &'static str {
        match self {
            PromptStyle::Nintendo => "The hints show A, B, X and Y as plain letters.",
            PromptStyle::Xbox => "The hints show A, B, X and Y on green, red, blue and yellow.",
            PromptStyle::PlayStation => "The hints show cross, circle, square and triangle.",
        }
    }

    /// The next style, and the one before it (Left/Right on the Theme page).
    pub fn step(self, forward: bool) -> PromptStyle {
        let i = PromptStyle::ALL.iter().position(|s| *s == self).unwrap_or(0);
        let n = PromptStyle::ALL.len();
        PromptStyle::ALL[if forward { (i + 1) % n } else { (i + n - 1) % n }]
    }

    fn from_u8(v: u8) -> PromptStyle {
        PromptStyle::ALL.get(v as usize).copied().unwrap_or(PromptStyle::Nintendo)
    }

    /// `POCKETUI_PROMPTS=nintendo|xbox|playstation`: the snapshots' and tests' choice over settings.json's.
    pub fn from_env() -> Option<PromptStyle> {
        match std::env::var("POCKETUI_PROMPTS").ok()?.trim().to_ascii_lowercase().as_str() {
            "nintendo" => Some(PromptStyle::Nintendo),
            "xbox" => Some(PromptStyle::Xbox),
            "playstation" | "ps" => Some(PromptStyle::PlayStation),
            _ => None,
        }
    }
}

/// The style in use, process wide (read by every painter, set by the UI thread); Nintendo until settings.json is read.
/// Per thread in the unit tests, which run side by side.
#[cfg(not(test))]
static STYLE: AtomicU8 = AtomicU8::new(0);
#[cfg(test)]
thread_local! {
    static STYLE: AtomicU8 = const { AtomicU8::new(0) };
}

pub fn set(s: PromptStyle) {
    #[cfg(not(test))]
    STYLE.store(s as u8, Ordering::Relaxed);
    #[cfg(test)]
    STYLE.with(|v| v.store(s as u8, Ordering::Relaxed));
}

pub fn get() -> PromptStyle {
    #[cfg(not(test))]
    let v = STYLE.load(Ordering::Relaxed);
    #[cfg(test)]
    let v = STYLE.with(|v| v.load(Ordering::Relaxed));
    PromptStyle::from_u8(v)
}

/// The characters `Text` draws as the PlayStation shapes.
pub const CROSS: char = '\u{E100}';
pub const CIRCLE: char = '\u{E101}';
pub const SQUARE: char = '\u{E102}';
pub const TRIANGLE: char = '\u{E103}';

/// What a hint key `key` shows in the style in use: the PlayStation shape for A, B, X and Y, else the key as it is.
pub fn shown(key: &str) -> &str {
    shown_in(get(), key)
}

pub fn shown_in(style: PromptStyle, key: &str) -> &str {
    if style != PromptStyle::PlayStation {
        return key;
    }
    match key {
        "A" => "\u{E100}",
        "B" => "\u{E101}",
        "X" => "\u{E102}",
        "Y" => "\u{E103}",
        _ => key,
    }
}

/// The cap and letter colours of `key` in the Xbox style (`None`: the hint's own colours).
pub fn colours(key: &str) -> Option<(u32, u32)> {
    colours_in(get(), key)
}

/// Xbox's four, a shade deeper where white letters on them need it for 4.5:1 (the test below).
pub fn colours_in(style: PromptStyle, key: &str) -> Option<(u32, u32)> {
    if style != PromptStyle::Xbox {
        return None;
    }
    Some(match key {
        "A" => (0x107c10, 0xffffff),
        "B" => (0xc42b1c, 0xffffff),
        "X" => (0x0e6ec5, 0xffffff),
        "Y" => (0xf5b800, crate::palette::INK),
        _ => return None,
    })
}

/// The glyph of a shape character at `px` for a face whose capitals are `cap` px high: a square a little taller than
/// the capitals, centred on them, its outline anti-aliased by distance. `None` for every other character.
pub fn raster(ch: char, px: u32, cap: f32) -> Option<(Metrics, Vec<u8>)> {
    if !(CROSS..=TRIANGLE).contains(&ch) {
        return None;
    }
    let cap = if cap > 0.0 { cap } else { px as f32 * 0.7 };
    let s = (cap * 1.25).round().max(5.0) as usize;
    let side = (cap * 0.1).round().max(1.0) as usize;
    let sf = s as f32;
    let t = (sf * 0.15).max(1.6);
    let mut px_buf = vec![0u8; s * s];
    let seg = |p: (f32, f32), a: (f32, f32), b: (f32, f32)| {
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let k = (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / (dx * dx + dy * dy)).clamp(0.0, 1.0);
        ((p.0 - a.0 - k * dx).powi(2) + (p.1 - a.1 - k * dy).powi(2)).sqrt()
    };
    let edges = |p: (f32, f32), pts: &[(f32, f32)]| (0..pts.len()).map(|i| seg(p, pts[i], pts[(i + 1) % pts.len()])).fold(f32::MAX, f32::min);
    let m = t / 2.0 + sf * 0.06;
    for y in 0..s {
        for x in 0..s {
            let p = (x as f32 + 0.5, y as f32 + 0.5);
            let d = match ch {
                CROSS => seg(p, (m, m), (sf - m, sf - m)).min(seg(p, (m, sf - m), (sf - m, m))),
                CIRCLE => (((p.0 - sf / 2.0).powi(2) + (p.1 - sf / 2.0).powi(2)).sqrt() - (sf / 2.0 - t / 2.0 - 0.5)).abs(),
                SQUARE => {
                    let q = t / 2.0 + sf * 0.1;
                    edges(p, &[(q, q), (sf - q, q), (sf - q, sf - q), (q, sf - q)])
                }
                _ => {
                    // Equilateral, its centre of area a little low so it sits on the line like the others.
                    let q = t / 2.0 + 0.5;
                    let w = sf - 2.0 * q;
                    let h = w * 0.866;
                    let top = (sf - h) / 2.0 + sf * 0.02;
                    edges(p, &[(sf / 2.0, top), (q + w, top + h), (q, top + h)])
                }
            } - t / 2.0;
            px_buf[y * s + x] = ((0.5 - d).clamp(0.0, 1.0) * 255.0) as u8;
        }
    }
    let ymin = ((cap - sf) / 2.0).round() as i32;
    let metrics = Metrics {
        xmin: side as i32,
        ymin,
        width: s,
        height: s,
        advance_width: (s + 2 * side) as f32,
        advance_height: 0.0,
        bounds: fontdue::OutlineBounds { xmin: side as f32, ymin: ymin as f32, width: sf, height: sf },
    };
    Some((metrics, px_buf))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_style_is_kept_and_follows_the_device_when_unset() {
        let h = std::thread::spawn(|| {
            for s in PromptStyle::ALL {
                set(s);
                assert_eq!(get(), s);
            }
            set(PromptStyle::Nintendo);
        });
        h.join().unwrap();
        for s in PromptStyle::ALL {
            let json = serde_json::to_string(&s).unwrap();
            assert_eq!(serde_json::from_str::<PromptStyle>(&json).unwrap(), s, "{json}");
        }
        assert_eq!(serde_json::to_string(&PromptStyle::PlayStation).unwrap(), "\"playstation\"");
        assert_eq!(PromptStyle::Xbox.step(true), PromptStyle::PlayStation);
        assert_eq!(PromptStyle::Nintendo.step(false), PromptStyle::PlayStation);
    }

    #[test]
    fn only_the_four_face_keys_change() {
        assert_eq!(shown_in(PromptStyle::PlayStation, "A"), "\u{E100}");
        assert_eq!(shown_in(PromptStyle::PlayStation, "Y"), "\u{E103}");
        assert_eq!(shown_in(PromptStyle::PlayStation, "START"), "START");
        assert_eq!(shown_in(PromptStyle::Xbox, "A"), "A");
        assert_eq!(shown_in(PromptStyle::Nintendo, "B"), "B");
        assert!(colours_in(PromptStyle::Xbox, "L1").is_none() && colours_in(PromptStyle::Nintendo, "A").is_none());
        assert!(colours_in(PromptStyle::PlayStation, "A").is_none());
    }

    /// WCAG AA for the letters on the Xbox caps, which are the same in Dark and Light.
    #[test]
    fn the_xbox_letters_are_readable_on_their_caps() {
        for k in ["A", "B", "X", "Y"] {
            let (bg, fg) = colours_in(PromptStyle::Xbox, k).unwrap();
            let r = crate::palette::contrast(fg, bg);
            assert!(r >= 4.5, "{k}: {fg:06x} on {bg:06x} is {r:.2}:1");
        }
    }

    #[test]
    fn the_shapes_are_square_inked_and_centred_on_the_capitals() {
        for ch in [CROSS, CIRCLE, SQUARE, TRIANGLE] {
            for px in [21u32, 30, 36] {
                let cap = px as f32 * 0.7;
                let (m, bmp) = raster(ch, px, cap).unwrap();
                assert_eq!((m.width, m.height), (m.width, m.width));
                assert_eq!(bmp.len(), m.width * m.height);
                let ink = bmp.iter().filter(|v| **v > 128).count();
                assert!(ink > m.width * 2 && ink < m.width * m.width * 2 / 3, "{ch:?} at {px}: an outline, {ink} px");
                let mid = m.ymin as f32 + m.height as f32 / 2.0;
                assert!((mid - cap / 2.0).abs() <= 1.0, "{ch:?} at {px}: centred on the capitals");
                assert!(m.advance_width > m.width as f32);
            }
        }
        assert!(raster('A', 30, 21.0).is_none());
    }
}
