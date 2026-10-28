//! Shared colours of the shelf design (see docs/DESIGN.md).
//!
//! Two layers:
//! - The constants (`VOID`, `CHALK`, ...) are the dark design. Overlays drawn over a game picture or over any
//!   screen use them directly and stay dark in both appearances: the OSD, notices, toasts, the power menu, and
//!   pocket-core's in-game overlays (pocket-core never sets an appearance).
//! - `pal()` is the palette of the appearance in use (Settings > Look > Appearance), `DARK` or `LIGHT`, or a
//!   mix of the two while the screen crossfades from one to the other. Every full screen of pocket-ui (the
//!   shelf in its four looks, the game card, Settings, the manual, setup, the switcher) takes its colours
//!   from it.

use std::cell::Cell;

use crate::gfx::mix;

pub const VOID: u32 = 0x000000;
pub const SPINE: u32 = 0x1f1f22;
pub const LABEL: u32 = 0xece9e1;
pub const INK: u32 = 0x151515;
pub const CHALK: u32 = 0xd9d6ce;
pub const DUST: u32 = 0x8a877f;
pub const KEYCAP: u32 = 0x2a2a2e;
pub const ALERT: u32 = 0xe0705c;

pub const MARGIN: i32 = 72;
pub const STRIPE: i32 = 12;

/// The colours of one appearance. Field names follow the design tokens; `t` says where the palette sits
/// between dark (0) and light (255), for `pick`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Palette {
    /// Page background.
    pub void: u32,
    /// Unselected spine, row and tile.
    pub spine: u32,
    /// The pulled-out paper label (and anything selected the same way).
    pub label: u32,
    /// Text on the label.
    pub ink: u32,
    /// Main text on the page.
    pub chalk: u32,
    /// Secondary text and hints on the page.
    pub dust: u32,
    /// Key caps of the hints, the selected look's frame.
    pub keycap: u32,
    /// Errors.
    pub alert: u32,
    /// Text on an `alert` band (a row asking to confirm something that can't be undone).
    pub on_alert: u32,
    /// Secondary text on the label.
    pub label_dust: u32,
    /// Rows that can't be used right now.
    pub off: u32,
    /// Strong text in the Settings style lists (white on dark).
    pub text: u32,
    /// Text in the status, heading and hint pills.
    pub soft: u32,
    /// Explanations under a headline (Battery).
    pub quiet: u32,
    /// Status, heading and hint pills; scroll bar tracks.
    pub pill: u32,
    /// The key chips in the hint pill and scroll bar thumbs; their letters are `pill`.
    pub chip: u32,
    /// The band of the selected row in the Settings style lists.
    pub band: u32,
    /// The name pill of the selected row; its text is `ink`.
    pub select: u32,
    /// Favorites, "Playing? Press Y".
    pub amber: u32,
    /// A starred game's star next to page text.
    pub star: u32,
    /// Split: the page, and the unselected names.
    pub split_bg: u32,
    pub split_dim: u32,
    /// The bright end of the dark design's white text on coloured cards and panels; dark text in light.
    pub on_wash: u32,
    pub t: u8,
}

pub const DARK: Palette = Palette {
    void: VOID,
    spine: SPINE,
    label: LABEL,
    ink: INK,
    chalk: CHALK,
    dust: DUST,
    keycap: KEYCAP,
    alert: ALERT,
    on_alert: INK,
    label_dust: 0x5a5852,
    off: 0x4a4945,
    text: 0xffffff,
    soft: 0xcccccc,
    quiet: 0xaaa79f,
    pill: 0x262626,
    chip: 0x8f8f8f,
    band: 0x71757b,
    select: 0xe9f2f1,
    amber: 0xd39a2c,
    star: 0xf2c14e,
    split_bg: 0x12161b,
    split_dim: 0xaab3bd,
    on_wash: 0xffffff,
    t: 0,
};

/// Warm paper: an off-white page, spines a shade darker, the label a brighter sheet with ink on it.
pub const LIGHT: Palette = Palette {
    void: 0xf3efe7,
    spine: 0xe6e1d6,
    label: 0xfffdf8,
    ink: 0x1a1916,
    chalk: 0x2b2925,
    dust: 0x625e56,
    keycap: 0xdcd6ca,
    alert: 0xa63a28,
    on_alert: 0xfffdf8,
    label_dust: 0x5f5b53,
    off: 0xa8a397,
    text: 0x1f1d1a,
    soft: 0x3c3a35,
    quiet: 0x57534c,
    pill: 0xe3ded3,
    chip: 0x5e5a53,
    band: 0xcfc8ba,
    select: 0xfffdf8,
    amber: 0x8f5e0c,
    star: 0xb07a12,
    split_bg: 0xece7dd,
    split_dim: 0x58544d,
    on_wash: 0x1f1d1a,
    t: 255,
};

impl Palette {
    /// `a` mixed towards `b` by `t` (0–255), field by field.
    pub fn mixed(a: &Palette, b: &Palette, t: u8) -> Palette {
        let m = |x: u32, y: u32| mix(x, y, t as u32);
        Palette {
            void: m(a.void, b.void),
            spine: m(a.spine, b.spine),
            label: m(a.label, b.label),
            ink: m(a.ink, b.ink),
            chalk: m(a.chalk, b.chalk),
            dust: m(a.dust, b.dust),
            keycap: m(a.keycap, b.keycap),
            alert: m(a.alert, b.alert),
            on_alert: m(a.on_alert, b.on_alert),
            label_dust: m(a.label_dust, b.label_dust),
            off: m(a.off, b.off),
            text: m(a.text, b.text),
            soft: m(a.soft, b.soft),
            quiet: m(a.quiet, b.quiet),
            pill: m(a.pill, b.pill),
            chip: m(a.chip, b.chip),
            band: m(a.band, b.band),
            select: m(a.select, b.select),
            amber: m(a.amber, b.amber),
            star: m(a.star, b.star),
            split_bg: m(a.split_bg, b.split_bg),
            split_dim: m(a.split_dim, b.split_dim),
            on_wash: m(a.on_wash, b.on_wash),
            t: ((a.t as u32 * (255 - t as u32) + b.t as u32 * t as u32) / 255) as u8,
        }
    }

    /// One colour that has no token of its own: `dark` in the dark appearance, `light` in the light one.
    pub fn pick(&self, dark: u32, light: u32) -> u32 {
        mix(dark, light, self.t as u32)
    }

    /// True once the palette is more light than dark.
    pub fn is_light(&self) -> bool {
        self.t >= 128
    }
}

thread_local! {
    static CURRENT: Cell<Palette> = const { Cell::new(DARK) };
}

/// The palette in use. Dark until `set_blend` says otherwise (and always in pocket-core).
pub fn pal() -> Palette {
    CURRENT.with(Cell::get)
}

/// Sets the palette in use: 0 dark, 255 light, anything between while crossfading.
pub fn set_blend(t: u8) {
    CURRENT.with(|c| c.set(Palette::mixed(&DARK, &LIGHT, t)));
}

/// Runs `f` with the palette at `t` (0 dark, 255 light) and puts the one in use back afterwards.
pub fn scoped<R>(t: u8, f: impl FnOnce() -> R) -> R {
    let before = pal();
    set_blend(t);
    let out = f();
    CURRENT.with(|c| c.set(before));
    out
}

/// WCAG 2 relative luminance of a 0RGB colour.
pub fn luminance(c: u32) -> f64 {
    let ch = |s: u32| {
        let v = ((c >> s) & 0xff) as f64 / 255.0;
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * ch(16) + 0.7152 * ch(8) + 0.0722 * ch(0)
}

/// WCAG 2 contrast ratio of two colours (1 to 21).
pub fn contrast(a: u32, b: u32) -> f64 {
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every text/background pair of the palette that a screen paints: the text token, the background token, and where.
    /// A new token for text or for something text sits on belongs here, so its contrast is checked in both
    /// appearances. Not here: `off` (rows that can't be used, which WCAG exempts), `star` (a mark, not text), and text
    /// on a system's colour (`on_wash` on cards and panels, Split's selected name on its bar): those backgrounds are
    /// the systems' stripes, not palette tokens.
    fn pairs(p: &Palette) -> Vec<(&'static str, u32, u32)> {
        vec![
            // The page: titles, meta lines, the top line, hint labels, Settings-style lists, explanations, notes.
            ("chalk on the page", p.chalk, p.void),
            ("dust on the page", p.dust, p.void),
            ("text on the page", p.text, p.void),
            ("quiet text on the page", p.quiet, p.void),
            ("alert on the page", p.alert, p.void),
            ("amber on the page", p.amber, p.void),
            // Spines: titles, the count or system at the right end, Curator's rows on the card, the Lens's counts.
            ("chalk on a spine", p.chalk, p.spine),
            ("dust on a spine", p.dust, p.spine),
            // The pulled-out label and every row selected the same way.
            ("ink on the label", p.ink, p.label),
            ("label dust on the label", p.label_dust, p.label),
            // Hints: the key letter in its cap; the status corner and hint pills; their key chips.
            ("chalk on a key cap", p.chalk, p.keycap),
            ("pill text on a pill", p.soft, p.pill),
            ("key letters on a key chip", p.pill, p.chip),
            // The selected row of the Settings-style lists: the name on its pill, the value on the band.
            ("ink on the selected row's name pill", p.ink, p.select),
            ("text on the selected row's band", p.text, p.band),
            ("text on an alert band", p.on_alert, p.alert),
            // Split: names and meta, the top line.
            ("Split: names on the page", p.text, p.split_bg),
            ("Split: unselected names", p.split_dim, p.split_bg),
            ("Split: the top line", p.dust, p.split_bg),
        ]
    }

    #[test]
    fn contrast_of_known_colours() {
        assert!((contrast(0xffffff, 0x000000) - 21.0).abs() < 0.01);
        assert!((contrast(0x777777, 0x777777) - 1.0).abs() < 0.001);
    }

    /// WCAG AA (4.5:1) for every pair, in both appearances, without exceptions: on a 3.92" panel held at arm's length
    /// even the 36 px values read like small text.
    #[test]
    fn every_text_pair_is_readable_wcag_aa_in_dark_and_light() {
        let mut low = Vec::new();
        for (look, p) in [("dark", &DARK), ("light", &LIGHT)] {
            for (what, fg, bg) in pairs(p) {
                let r = contrast(fg, bg);
                if r < 4.5 {
                    low.push(format!("{look}: {what}: {fg:06x} on {bg:06x} is {r:.2}:1"));
                }
            }
        }
        assert!(low.is_empty(), "below 4.5:1:\n{}", low.join("\n"));
    }

    #[test]
    fn the_palette_starts_dark_and_blends_both_ways() {
        let h = std::thread::spawn(|| {
            assert_eq!(pal(), DARK, "pocket-core and every new thread: dark");
            set_blend(255);
            assert_eq!(pal(), LIGHT);
            set_blend(128);
            let mid = pal();
            assert!(mid.void != DARK.void && mid.void != LIGHT.void);
            assert_eq!(scoped(0, pal), DARK, "an overlay painted dark inside a light screen");
            assert_eq!(pal(), mid, "and the screen's palette is back afterwards");
            assert_eq!(DARK.pick(0x111111, 0xeeeeee), 0x111111);
            assert_eq!(LIGHT.pick(0x111111, 0xeeeeee), 0xeeeeee);
        });
        h.join().unwrap();
    }

    #[test]
    fn the_dark_tokens_are_the_old_constants() {
        assert_eq!((DARK.void, DARK.spine, DARK.label, DARK.ink), (VOID, SPINE, LABEL, INK));
        assert_eq!((DARK.chalk, DARK.dust, DARK.keycap, DARK.alert), (CHALK, DUST, KEYCAP, ALERT));
    }
}
