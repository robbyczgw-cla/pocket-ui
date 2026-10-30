//! Shared colours of the shelf design (see docs/DESIGN.md).
//!
//! Two layers:
//! - The constants (`VOID`, `CHALK`, ...) are the dark design. Overlays drawn over a game picture or over any
//!   screen use them directly and stay dark in both appearances: the OSD, notices, toasts, the power menu, and
//!   pocket-core's in-game overlays (pocket-core never sets an appearance).
//! - `pal()` is the palette of the appearance in use (Settings > Look > Appearance), the dark or light one of the
//!   colours in use (Settings > Look > Theme > Colours, `Colours`: Warm's are `DARK` and `LIGHT`), or a mix of the two
//!   while the screen crossfades from one to the other. Every full screen of pocket-ui (the shelf in its four looks,
//!   the game card, Settings, the manual, setup, the switcher) takes its colours from it. The colours' `Scheme` also
//!   tints the Cabinet's wood, its shelves' name plates and the light of its slot (`scheme`); box art, the cases'
//!   art and the systems' stripes keep theirs.

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
    // The neutrals lean warm, like the paper and the walnut of the shelf (2026-10-29, docs/DESIGN.md "Colour roles"):
    // the pills, their chips and the band were cool greys, the selected name a bluish white.
    soft: 0xd2cec6,
    quiet: 0xaaa79f,
    pill: 0x272523,
    chip: 0x938e85,
    band: 0x77736b,
    // The selected row's name is on the pulled-out label's paper, as everywhere on the shelf.
    select: LABEL,
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
    /// The palette in use and the colours (`Colours as u8`) it was mixed from.
    static CURRENT: Cell<(Palette, u8)> = const { Cell::new((DARK, 0)) };
}

/// The palette in use. Dark until `set_blend` says otherwise (and always in pocket-core); mixed again from the new
/// colours' when Settings → Look → Theme → Colours changed them (one atomic load a call otherwise).
pub fn pal() -> Palette {
    CURRENT.with(|c| {
        let (p, made) = c.get();
        let now = colours() as u8;
        if made == now {
            return p;
        }
        let s = Colours::from_u8(now).scheme();
        let p = Palette::mixed(&s.dark, &s.light, p.t);
        c.set((p, now));
        p
    })
}

/// Sets the palette in use: 0 dark, 255 light, anything between while crossfading.
pub fn set_blend(t: u8) {
    let now = colours();
    let s = now.scheme();
    CURRENT.with(|c| c.set((Palette::mixed(&s.dark, &s.light, t), now as u8)));
}

/// Runs `f` with the palette at `t` (0 dark, 255 light) and puts the one in use back afterwards.
pub fn scoped<R>(t: u8, f: impl FnOnce() -> R) -> R {
    let before = CURRENT.with(Cell::get);
    set_blend(t);
    let out = f();
    CURRENT.with(|c| c.set(before));
    out
}

/// Settings → Look & feel → Look → Theme → **Colours**: five sets of the tokens above, each a dark and a light
/// palette, with the Cabinet's wood, plates and light to match. Warm is the design's own (`DARK`, `LIGHT`); the others
/// lean on the colours of well-known consoles without their names. Every text pair of each is checked for WCAG AA in
/// both appearances (the tests below).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Colours {
    /// Paper and walnut.
    #[default]
    Warm,
    /// A deep indigo with lilac, like a small cube-shaped console.
    IndigoCube,
    /// A grey handheld's plastic with its dark green.
    GreyBrick,
    /// Green on black.
    GreenX,
    /// Grey plastic with a deep blue.
    BlueStation,
}

impl Colours {
    pub const ALL: [Colours; 5] = [Colours::Warm, Colours::IndigoCube, Colours::GreyBrick, Colours::GreenX, Colours::BlueStation];

    /// The value on the Theme page.
    pub fn label(self) -> &'static str {
        match self {
            Colours::Warm => "Warm",
            Colours::IndigoCube => "Indigo Cube",
            Colours::GreyBrick => "Grey Brick",
            Colours::GreenX => "Green X",
            Colours::BlueStation => "Blue Station",
        }
    }

    /// The line under the Theme page's list.
    pub fn blurb(self) -> &'static str {
        match self {
            Colours::Warm => "Paper and walnut, the shelf's own colours.",
            Colours::IndigoCube => "Deep indigo with lilac, like a little cube of a console.",
            Colours::GreyBrick => "Grey handheld plastic with its dark green.",
            Colours::GreenX => "Bright green on black.",
            Colours::BlueStation => "Grey plastic with a deep blue.",
        }
    }

    pub fn step(self, forward: bool) -> Colours {
        let i = Colours::ALL.iter().position(|c| *c == self).unwrap_or(0);
        let n = Colours::ALL.len();
        Colours::ALL[if forward { (i + 1) % n } else { (i + n - 1) % n }]
    }

    fn from_u8(v: u8) -> Colours {
        Colours::ALL.get(v as usize).copied().unwrap_or(Colours::Warm)
    }

    /// `POCKETUI_COLOURS=warm|indigo-cube|grey-brick|green-x|blue-station`: the snapshots' choice over settings.json's.
    pub fn from_env() -> Option<Colours> {
        let v = std::env::var("POCKETUI_COLOURS").ok()?;
        serde_json::from_str(&format!("\"{}\"", v.trim().to_ascii_lowercase())).ok()
    }

    pub fn scheme(self) -> &'static Scheme {
        match self {
            Colours::Warm => &WARM,
            Colours::IndigoCube => &INDIGO_CUBE,
            Colours::GreyBrick => &GREY_BRICK,
            Colours::GreenX => &GREEN_X,
            Colours::BlueStation => &BLUE_STATION,
        }
    }
}

/// The colours in use, process wide (the previews are painted on the UI thread too); Warm until settings.json is read.
/// Per thread in the unit tests, which run side by side.
#[cfg(not(test))]
static COLOURS: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);
#[cfg(test)]
thread_local! {
    static COLOURS: std::sync::atomic::AtomicU8 = const { std::sync::atomic::AtomicU8::new(0) };
}

pub fn set_colours(c: Colours) {
    #[cfg(not(test))]
    COLOURS.store(c as u8, std::sync::atomic::Ordering::Relaxed);
    #[cfg(test)]
    COLOURS.with(|v| v.store(c as u8, std::sync::atomic::Ordering::Relaxed));
}

pub fn colours() -> Colours {
    #[cfg(not(test))]
    let v = COLOURS.load(std::sync::atomic::Ordering::Relaxed);
    #[cfg(test)]
    let v = COLOURS.with(|v| v.load(std::sync::atomic::Ordering::Relaxed));
    Colours::from_u8(v)
}

/// The colours in use: their palettes and what they give the Cabinet.
pub fn scheme() -> &'static Scheme {
    colours().scheme()
}

/// One set of colours: the dark and the light palette, and the Cabinet's.
#[derive(Debug)]
pub struct Scheme {
    pub dark: Palette,
    pub light: Palette,
    /// The colour the Cabinet's wood is stained towards, dark and light page (`stain` how much, 0 none to 255), its
    /// grain and shading kept (`cabinet_art::wood`).
    pub wood: [u32; 2],
    pub stain: u8,
    /// A shelf's name plate: the holder, the card in it and the name on the card.
    pub holder: u32,
    pub card: u32,
    pub card_ink: u32,
    /// The light of the console's slot when a game goes in.
    pub lamp: u32,
}

pub const WARM: Scheme = Scheme { dark: DARK, light: LIGHT, wood: [0, 0], stain: 0, holder: 0x8a7350, card: 0xe6dfcf, card_ink: 0x3a3026, lamp: 0xfff4d8 };

pub const INDIGO_CUBE: Scheme = Scheme {
    dark: Palette {
        void: 0x15103a,
        spine: 0x2a2262,
        label: 0xe9e2ff,
        ink: 0x1a1240,
        chalk: 0xe0dbf7,
        dust: 0xa59ed4,
        keycap: 0x3a3080,
        alert: 0xf0786a,
        on_alert: INK,
        label_dust: 0x584c8c,
        off: 0x50489a,
        text: 0xffffff,
        soft: 0xdcd6f5,
        quiet: 0xb7b0df,
        pill: 0x251d5a,
        chip: 0xaa9cf0,
        band: 0x6448cc,
        select: 0xe9e2ff,
        amber: 0xc6aaff,
        star: 0xffd166,
        split_bg: 0x1b1546,
        split_dim: 0xb8b0e4,
        on_wash: 0xffffff,
        t: 0,
    },
    light: Palette {
        void: 0xe8e3fb,
        spine: 0xd7cff5,
        label: 0xfbf9ff,
        ink: 0x17152b,
        chalk: 0x27243f,
        dust: 0x544a86,
        keycap: 0xc9bff0,
        alert: 0xa63a28,
        on_alert: 0xfffdf8,
        label_dust: 0x544a86,
        off: 0xa4a0bf,
        text: 0x1c1a30,
        soft: 0x37334f,
        quiet: 0x4d4480,
        pill: 0xd8d0f6,
        chip: 0x4b3a9c,
        band: 0xbfb1f0,
        select: 0xfbf9ff,
        amber: 0x5a2fb8,
        star: 0x6f45d6,
        split_bg: 0xe0daf8,
        split_dim: 0x4d4480,
        on_wash: 0x1c1a30,
        t: 255,
    },
    wood: [0x4a3a96, 0x9d90dc],
    stain: 200,
    holder: 0x8a78e0,
    card: 0xece6ff,
    card_ink: 0x2a1f5a,
    lamp: 0xd8c8ff,
};

pub const GREY_BRICK: Scheme = Scheme {
    dark: Palette {
        void: 0x1b1f12,
        spine: 0x303624,
        label: 0xcfe0a8,
        ink: 0x151815,
        chalk: 0xd3d6cc,
        dust: 0x9aa092,
        keycap: 0x3a4130,
        alert: ALERT,
        on_alert: INK,
        label_dust: 0x4a5536,
        off: 0x4c5340,
        text: 0xffffff,
        soft: 0xcfd2c8,
        quiet: 0xb0b5a6,
        pill: 0x2a3020,
        chip: 0x9bbc0f,
        band: 0x3f6a2a,
        select: 0xcfe0a8,
        amber: 0x9bbc0f,
        star: 0xc8d84a,
        split_bg: 0x20251a,
        split_dim: 0xa9b2a0,
        on_wash: 0xffffff,
        t: 0,
    },
    light: Palette {
        void: 0xdbe3bf,
        spine: 0xcbd4ad,
        label: 0xf3f7e4,
        ink: 0x1a1c18,
        chalk: 0x272a25,
        dust: 0x484d40,
        keycap: 0xbcc59d,
        alert: 0xa63a28,
        on_alert: 0xfffdf8,
        label_dust: 0x484d40,
        off: 0x9fa196,
        text: 0x1d201b,
        soft: 0x363a33,
        quiet: 0x434839,
        pill: 0xcdd5af,
        chip: 0x285428,
        band: 0xb2c78c,
        select: 0xf3f7e4,
        amber: 0x355f10,
        star: 0x4f7514,
        split_bg: 0xd2dab4,
        split_dim: 0x434839,
        on_wash: 0x1d201b,
        t: 255,
    },
    wood: [0x7d805e, 0xb9bf92],
    stain: 225,
    holder: 0x306230,
    card: 0xcadc9f,
    card_ink: 0x0f380f,
    lamp: 0xe8ffd0,
};

pub const GREEN_X: Scheme = Scheme {
    dark: Palette {
        void: 0x07170c,
        spine: 0x14281a,
        label: 0x86dc66,
        ink: 0x0a140a,
        chalk: 0xd6e6d2,
        dust: 0x86a083,
        keycap: 0x1e3c25,
        alert: ALERT,
        on_alert: INK,
        label_dust: 0x24461d,
        off: 0x40563f,
        text: 0xffffff,
        soft: 0xcfe0ca,
        quiet: 0xa0b39b,
        pill: 0x11261a,
        chip: 0x5fbf3f,
        band: 0x1f6b1f,
        select: 0x86dc66,
        amber: 0x9bf00b,
        star: 0xb4ff3a,
        split_bg: 0x0b1d10,
        split_dim: 0x9fb59a,
        on_wash: 0xffffff,
        t: 0,
    },
    light: Palette {
        void: 0xdff1d6,
        spine: 0xcde6c1,
        label: 0xffffff,
        ink: 0x101a10,
        chalk: 0x1f2a1f,
        dust: 0x4a5747,
        keycap: 0xbfdcb2,
        alert: 0xa63a28,
        on_alert: 0xfffdf8,
        label_dust: 0x4a5747,
        off: 0x9fab9b,
        text: 0x162016,
        soft: 0x2f3a2e,
        quiet: 0x465344,
        pill: 0xd1e7c6,
        chip: 0x0b650b,
        band: 0xb5dca8,
        select: 0xffffff,
        amber: 0x1b6f10,
        star: 0x2e8b1f,
        split_bg: 0xd6ebcc,
        split_dim: 0x465344,
        on_wash: 0x162016,
        t: 255,
    },
    wood: [0x1f5a2a, 0xa8d49a],
    stain: 230,
    holder: 0x107c10,
    card: 0xe2f2dc,
    card_ink: 0x0b2a0b,
    lamp: 0xc8ffb0,
};

pub const BLUE_STATION: Scheme = Scheme {
    dark: Palette {
        void: 0x0d1628,
        spine: 0x1d283c,
        label: 0xdfe4ec,
        ink: 0x11141b,
        chalk: 0xd8dce3,
        dust: 0x9298a4,
        keycap: 0x2b3752,
        alert: ALERT,
        on_alert: INK,
        label_dust: 0x4c5466,
        off: 0x48536a,
        text: 0xffffff,
        soft: 0xd0d4dc,
        quiet: 0xa9aeb8,
        pill: 0x18223a,
        chip: 0x8fb0f0,
        band: 0x1d4fb8,
        select: 0xdfe4ec,
        amber: 0x4d9bff,
        star: 0x8cc0ff,
        split_bg: 0x111b2e,
        split_dim: 0xb0b6c0,
        on_wash: 0xffffff,
        t: 0,
    },
    light: Palette {
        void: 0xdae4f4,
        spine: 0xc8d4ea,
        label: 0xf7f9fc,
        ink: 0x12151c,
        chalk: 0x232733,
        dust: 0x4b5262,
        keycap: 0xbac8e2,
        alert: 0xa63a28,
        on_alert: 0xfffdf8,
        label_dust: 0x4b5262,
        off: 0x9da3b0,
        text: 0x1a1e28,
        soft: 0x343a48,
        quiet: 0x454c5b,
        pill: 0xccd7ec,
        chip: 0x00308a,
        band: 0xa9c0ee,
        select: 0xf7f9fc,
        amber: 0x0a46b0,
        star: 0x1a5ad0,
        split_bg: 0xd1dcf0,
        split_dim: 0x454c5b,
        on_wash: 0x1a1e28,
        t: 255,
    },
    wood: [0x5c6678, 0xb7c0d0],
    stain: 230,
    holder: 0x1d4fb8,
    card: 0xe6e9f0,
    card_ink: 0x162036,
    lamp: 0xcfe0ff,
};

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
        for c in Colours::ALL {
            let s = c.scheme();
            for (look, p) in [("dark", &s.dark), ("light", &s.light)] {
                for (what, fg, bg) in pairs(p) {
                    let r = contrast(fg, bg);
                    if r < 4.5 {
                        low.push(format!("{}: {look}: {what}: {fg:06x} on {bg:06x} is {r:.2}:1", c.label()));
                    }
                }
            }
            let r = contrast(s.card_ink, s.card);
            if r < 4.5 {
                low.push(format!("{}: the name on a Cabinet plate is {r:.2}:1", c.label()));
            }
        }
        assert!(low.is_empty(), "below 4.5:1:\n{}", low.join("\n"));
    }

    /// Each set is its own: a page colour of its own in each appearance (the Cabinet keeps its wood per page colour), a
    /// dark one that is dark and a light one that is light, and the selection, frames and accents tinted.
    #[test]
    fn the_colours_differ_and_follow_the_appearance() {
        let mut pages = std::collections::HashSet::new();
        for c in Colours::ALL {
            let s = c.scheme();
            assert!(pages.insert(s.dark.void) && pages.insert(s.light.void), "{}: a page colour of its own", c.label());
            assert_eq!((s.dark.t, s.light.t), (0, 255), "{}", c.label());
            assert!(luminance(s.dark.void) < 0.02 && luminance(s.light.void) > 0.7, "{}", c.label());
            if c != Colours::Warm {
                assert!(s.dark.band != DARK.band && s.dark.chip != DARK.chip && s.light.amber != LIGHT.amber, "{}: tinted", c.label());
                assert!(s.stain > 0, "{}: the wood is stained", c.label());
            }
            let json = serde_json::to_string(&c).unwrap();
            assert_eq!(serde_json::from_str::<Colours>(&json).unwrap(), c, "{json}");
        }
        assert_eq!(serde_json::to_string(&Colours::IndigoCube).unwrap(), "\"indigo-cube\"");
        assert_eq!(Colours::Warm.step(false), Colours::BlueStation);
    }

    #[test]
    fn the_palette_in_use_follows_the_colours_at_once() {
        let h = std::thread::spawn(|| {
            set_blend(255);
            assert_eq!(pal(), LIGHT);
            set_colours(Colours::GreenX);
            assert_eq!(pal(), GREEN_X.light, "the next call mixes the new colours, still light");
            set_blend(0);
            assert_eq!(pal(), GREEN_X.dark);
            assert_eq!(scoped(255, pal), GREEN_X.light);
            assert_eq!(pal(), GREEN_X.dark);
            set_colours(Colours::Warm);
            assert_eq!(pal(), DARK);
        });
        h.join().unwrap();
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
