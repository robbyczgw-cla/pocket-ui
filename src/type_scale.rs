//! Settings → Look → Text size: one scale for the whole type of pocket-ui's screens (docs/DESIGN.md, Type).
//!
//! The painters keep asking for the sizes of the design (Body 28, Title 37, ...); `Text` turns each into the size it
//! paints, measures, fits and wraps at ([`px`]), so every string grows by the same rule and every width a layout
//! measures is the width it gets. The layouts take their row heights and line steps from the same rule ([`row`],
//! [`line`]), so a list with larger type has taller rows and fewer of them on a page.
//!
//! **Normal** is the design as it is: [`px`] gives the size back unchanged and nothing else differs (one
//! thread-local read per string). **Medium** is the step between: every size under 64 px about an eighth larger
//! (1.12), at most 6 px larger, on the same rules as Large: Body 28 is 31, the hint bar's 21 is 24, Title 37 is 41, 50 is
//! 56. **Large** sets every size under 64 px a quarter larger, at most 10 px larger: Body
//! 28 is 35 (capitals about 1.5 mm high instead of 1.2 mm), the hint bar's 21 is 26, Title 37 is 46, 50 is 60. Sizes
//! of 64 px and more (the pulled-out title, the big letter of a jump, the battery figure) are large already and stay.
//!
//! Per thread, as `motion`'s level: the screens are painted on the UI thread, and tests running side by side each get
//! their own.

use std::cell::Cell;

pub use crate::settings::TextSize;

thread_local! {
    static SIZE: Cell<TextSize> = const { Cell::new(TextSize::Normal) };
    /// Off while something drawn to a fixed physical size is painted (`unscaled`): a case's spine, a key cap.
    static FIXED: Cell<bool> = const { Cell::new(false) };
}

/// The size in use from the next frame on (Settings, the start; `App::set_text_size`).
pub fn set(size: TextSize) {
    SIZE.with(|s| s.set(size));
}

pub fn get() -> TextSize {
    SIZE.with(|s| s.get())
}

/// Whether type is painted larger than the design right now (Medium or Large).
pub fn scaled() -> bool {
    get() != TextSize::Normal && !FIXED.with(|f| f.get())
}

/// The size `Text` paints a design size `px` at.
#[inline]
pub fn px(px: u32) -> u32 {
    if scaled() {
        grown_in(get(), px)
    } else {
        px
    }
}

/// `px` in `size`: Large a quarter more, at most 10 px more; Medium 1.12 times, at most 6 px more; never past 64 px,
/// and nothing from 64 px up. Normal is the design.
pub fn grown_in(size: TextSize, px: u32) -> u32 {
    if px >= 64 {
        return px;
    }
    match size {
        TextSize::Normal => px,
        TextSize::Medium => ((px * 28 + 12) / 25).min(px + 6).min(64),
        TextSize::Large => ((px * 5 + 2) / 4).min(px + 10).min(64),
    }
}

/// A row of a list that was `h` pixels high around type of `font` px: taller by the type's growth and a fifth of it
/// again for the air around it (Settings' 66 px rows at 36 px are 76 in Large). `h` itself in Normal.
#[inline]
pub fn row(h: i32, font: u32) -> i32 {
    h + grow(font) * 6 / 5
}

/// The step from one line of `font` px to the next, `gap` pixels between them in the design: the line grows with
/// its type and the gap stays (`NU_SMALL + 12` is 42 in Normal, 50 in Large).
#[inline]
pub fn line(font: u32, gap: i32) -> i32 {
    px(font) as i32 + gap
}

/// A width `w` that was made to hold text of `font` px (a column of labels, a picker), grown as the type is.
#[inline]
pub fn wide(w: i32, font: u32) -> i32 {
    if scaled() {
        w * px(font) as i32 / font as i32
    } else {
        w
    }
}

/// How many pixels `font` grows by now (0 in Normal).
#[inline]
pub fn grow(font: u32) -> i32 {
    px(font) as i32 - font as i32
}

/// Where a line of `font` px sits in a row: the distance from the row's middle to the baseline (the painters'
/// `font * 7 / 20`, for the size that is painted).
#[inline]
pub fn mid(font: u32) -> i32 {
    px(font) as i32 * 7 / 20
}

/// A baseline `design` pixels below a row's middle in the design, for type of `font` px: lower by the part of the
/// type's growth that is above its baseline, so the larger type stays centred in its (taller) row.
#[inline]
pub fn base(design: i32, font: u32) -> i32 {
    design + grow(font) * 7 / 20
}

/// How many hints of a hint bar, from the first, keep their label in `room` pixels: `parts` are each hint's key (with
/// the space after it) and label (with the space after it) widths. All of them when they fit, as always in Normal;
/// otherwise labels go from the last one on and the keys stay: a key alone still says which button does something,
/// and the bar never runs off the screen or under other text.
pub fn hint_labels_that_fit(parts: &[(i32, i32)], room: i32) -> usize {
    let keys: i32 = parts.iter().map(|p| p.0).sum();
    let mut shown = parts.len();
    while shown > 0 && keys + parts[..shown].iter().map(|p| p.1).sum::<i32>() > room {
        shown -= 1;
    }
    shown
}

/// Runs `paint` with the design's sizes whatever the setting: for type that belongs to a drawing of a fixed size (a
/// case's spine on the Cabinet's shelf, which is as thick as the case; a feature card's picture of a screen).
pub fn unscaled<R>(paint: impl FnOnce() -> R) -> R {
    let _fixed = fixed();
    paint()
}

/// The design's sizes until the value is dropped (`unscaled` for a whole function).
pub fn fixed() -> Fixed {
    Fixed(FIXED.with(|f| f.replace(true)))
}

pub struct Fixed(bool);

impl Drop for Fixed {
    fn drop(&mut self) {
        FIXED.with(|f| f.set(self.0));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normal_paints_the_design_and_large_grows_the_reading_sizes_only() {
        set(TextSize::Normal);
        for size in [20, 21, 24, 28, 30, 36, 37, 46, 50, 67, 96, 200] {
            assert_eq!(px(size), size, "Normal is the design");
        }
        assert_eq!((row(66, 36), line(30, 12), mid(36)), (66, 42, 12));
        set(TextSize::Large);
        let pairs: Vec<(u32, u32)> = [21, 24, 28, 30, 36, 37, 46, 50, 63, 64, 67, 96, 200].iter().map(|s| (*s, px(*s))).collect();
        assert_eq!(pairs, [(21, 26), (24, 30), (28, 35), (30, 38), (36, 45), (37, 46), (46, 56), (50, 60), (63, 64), (64, 64), (67, 67), (96, 96), (200, 200)]);
        assert_eq!((row(66, 36), line(30, 12)), (76, 50));
        set(TextSize::Medium);
        let pairs: Vec<(u32, u32)> = [21, 24, 28, 30, 36, 37, 46, 50, 63, 64, 67, 96, 200].iter().map(|s| (*s, px(*s))).collect();
        assert_eq!(pairs, [(21, 24), (24, 27), (28, 31), (30, 34), (36, 40), (37, 41), (46, 52), (50, 56), (63, 64), (64, 64), (67, 67), (96, 96), (200, 200)]);
        assert_eq!((row(66, 36), line(30, 12), mid(36)), (70, 46, 14));
        assert_eq!(unscaled(|| px(24)), 24);
        for s in 10..200 {
            assert!(grown_in(TextSize::Normal, s) <= grown_in(TextSize::Medium, s) && grown_in(TextSize::Medium, s) <= grown_in(TextSize::Large, s), "{s}: the steps stay in order");
        }
        // The order of the sizes is kept: a heading never comes out smaller than the text under it.
        for s in 10..200 {
            assert!(px(s + 1) >= px(s), "{s}");
        }
        set(TextSize::Medium);
        for s in 10..200 {
            assert!(px(s + 1) >= px(s), "{s}");
        }
        set(TextSize::Large);
        assert_eq!(unscaled(|| px(24)), 24, "a case's spine stays as thick as the case");
        assert_eq!(px(24), 30);
        set(TextSize::Normal);
    }

    #[test]
    fn hint_labels_go_from_the_last_one_when_the_bar_is_too_wide() {
        let parts = [(50, 100), (50, 120), (80, 90)];
        assert_eq!(hint_labels_that_fit(&parts, 490), 3, "all of them fit");
        assert_eq!(hint_labels_that_fit(&parts, 489), 2);
        assert_eq!(hint_labels_that_fit(&parts, 400), 2);
        assert_eq!(hint_labels_that_fit(&parts, 300), 1);
        assert_eq!(hint_labels_that_fit(&parts, 100), 0, "keys only");
        assert_eq!(hint_labels_that_fit(&[], 0), 0);
    }
}
