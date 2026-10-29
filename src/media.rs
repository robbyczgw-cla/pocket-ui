//! The games' media as they stand on a shelf: a pixel-art template per kind of cartridge or case
//! (`assets/media`, drawn by `assets/media/tools/draw_media.py`), painted with the colour of the game's cover,
//! its title and its cover. Shared by the Shelf look (every entry a spine on one shelf, the selected one shown with
//! its front, `app_cabinet.rs` "the Shelf look") and the Cabinet look (cases standing on a cabinet's shelves).
//!
//! A template pixel is `SHELF_SCALE` screen pixels at the Cabinet's size (`Medium::size`, `k` = 1); the Shelf look
//! picks its own scale for the screen. Scaling is by nearest pixel, so pixel art stays pixel art. Templates are
//! decoded once, on first use.

use crate::cabinet::{ink_on, luma, Size};
use crate::cabinet_art::{self as art, Look, Pic};
use crate::gfx::mix;
use crate::image::Image;
use crate::system_art::Sprite;
use crate::text::Text;

/// The kinds of media, each with a template of its own.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Medium {
    Snes,
    N64,
    Nes,
    /// Game Boy and Game Boy Color, and the small cartridges of other handhelds.
    Gb,
    Gba,
    /// Any other console cartridge.
    Cart,
    /// The Mega Drive's clamshell case.
    MegaDrive,
    /// The Master System's and Game Gear's case with the white grid.
    MasterSystem,
    /// The thin PlayStation jewel case.
    Ps1,
    /// The Saturn's, Dreamcast's and Mega-CD's light disc case.
    SegaDisc,
    /// A jewel case for every other disc.
    Disc,
    /// The tall black DVD case.
    Ps2,
    GameCube,
    /// The white DVD case.
    Wii,
    /// A plain DVD case: other DVD systems, and Curator's boxes in Curator's colour.
    Keep,
    /// The PSP's slim UMD case.
    Umd,
    Ds,
    N3ds,
    /// The original Xbox's black DVD case with its green top band.
    Xbox,
}

/// Where things are on a template (template pixels): the spine's label and its band at the top (cases whose band
/// takes the cover's accent colour), the front's cover, and the spine's two ends that are not stretched.
struct Template {
    spine: (i32, i32),
    front: (i32, i32),
    spine_label: (i32, i32, i32, i32),
    front_label: (i32, i32, i32, i32),
    band: Option<(i32, i32, i32, i32)>,
    /// The spine's two ends (template rows) that a stretched spine would keep: printed by `draw_media.py` with the
    /// rest, checked by the tests; nothing stretches a spine at the moment.
    #[allow(dead_code)]
    caps: (i32, i32),
    /// Drawn in neutral grey, coloured with the system's plastic.
    tint: bool,
}

/// Screen pixels a template pixel on the Cabinet's shelf (`k` = 1 everywhere below): two and a half, so a shelf of
/// a few games still fills the cabinet and the titles on the spines are 30 px.
pub const SHELF_SCALE: f32 = 2.5;

/// The template's neutral plastic grey (`draw_media.py`'s GREY).
const GREY: u32 = 156;

/// Printed by `assets/media/tools/draw_media.py`, in the order of `Medium::ALL`.
#[rustfmt::skip]
const TEMPLATES: [Template; 19] = [
    // snes
    Template { spine: (22, 78), front: (88, 78), spine_label: (3, 21, 16, 52), front_label: (13, 14, 62, 57), band: None, caps: (20, 4), tint: true },
    // n64
    Template { spine: (21, 74), front: (88, 74), spine_label: (3, 17, 15, 52), front_label: (8, 16, 72, 52), band: None, caps: (16, 4), tint: true },
    // nes
    Template { spine: (22, 88), front: (75, 88), spine_label: (3, 21, 16, 61), front_label: (7, 18, 61, 45), band: None, caps: (20, 5), tint: true },
    // gb
    Template { spine: (18, 66), front: (58, 66), spine_label: (3, 11, 12, 50), front_label: (6, 12, 46, 39), band: None, caps: (10, 4), tint: true },
    // gba
    Template { spine: (18, 56), front: (75, 56), spine_label: (3, 10, 12, 42), front_label: (7, 7, 61, 36), band: None, caps: (9, 3), tint: true },
    // cart
    Template { spine: (22, 80), front: (85, 80), spine_label: (3, 21, 16, 54), front_label: (13, 14, 59, 59), band: None, caps: (20, 4), tint: true },
    // md
    Template { spine: (20, 118), front: (83, 118), spine_label: (2, 16, 16, 100), front_label: (3, 3, 77, 112), band: Some((2, 2, 16, 13)), caps: (16, 3), tint: false },
    // sms
    Template { spine: (20, 118), front: (83, 118), spine_label: (2, 15, 16, 101), front_label: (8, 8, 67, 102), band: None, caps: (15, 3), tint: false },
    // ps1
    Template { spine: (13, 88), front: (98, 88), spine_label: (1, 16, 11, 68), front_label: (7, 1, 90, 86), band: None, caps: (16, 5), tint: false },
    // segadisc
    Template { spine: (16, 100), front: (93, 100), spine_label: (2, 17, 12, 78), front_label: (7, 1, 85, 98), band: Some((2, 5, 12, 11)), caps: (17, 6), tint: false },
    // disc
    Template { spine: (15, 88), front: (98, 88), spine_label: (1, 15, 13, 69), front_label: (7, 1, 90, 86), band: Some((1, 4, 13, 10)), caps: (15, 5), tint: false },
    // ps2
    Template { spine: (19, 120), front: (84, 120), spine_label: (2, 19, 15, 99), front_label: (3, 3, 78, 114), band: None, caps: (19, 3), tint: false },
    // gc
    Template { spine: (18, 111), front: (81, 111), spine_label: (2, 15, 14, 94), front_label: (3, 3, 75, 105), band: None, caps: (15, 3), tint: false },
    // wii
    Template { spine: (19, 120), front: (84, 120), spine_label: (2, 17, 15, 101), front_label: (3, 3, 78, 114), band: None, caps: (17, 3), tint: false },
    // keep
    Template { spine: (19, 120), front: (84, 120), spine_label: (2, 17, 15, 101), front_label: (3, 3, 78, 114), band: Some((2, 2, 15, 14)), caps: (17, 3), tint: false },
    // umd
    Template { spine: (15, 103), front: (66, 103), spine_label: (1, 14, 13, 88), front_label: (3, 5, 60, 94), band: None, caps: (14, 2), tint: false },
    // ds
    Template { spine: (16, 95), front: (85, 95), spine_label: (2, 14, 12, 79), front_label: (3, 3, 79, 89), band: None, caps: (14, 3), tint: false },
    // 3ds
    Template { spine: (15, 93), front: (85, 93), spine_label: (1, 13, 13, 79), front_label: (3, 3, 79, 87), band: None, caps: (13, 2), tint: false },
    // xbox
    Template { spine: (19, 120), front: (84, 120), spine_label: (2, 19, 15, 99), front_label: (3, 17, 78, 100), band: None, caps: (19, 3), tint: false },
];

/// The templates' pictures: (spine, front) per medium, in the order of `Medium::ALL`.
const PNGS: [(&[u8], &[u8]); 19] = [
    (include_bytes!("../assets/media/snes-spine.png"), include_bytes!("../assets/media/snes-front.png")),
    (include_bytes!("../assets/media/n64-spine.png"), include_bytes!("../assets/media/n64-front.png")),
    (include_bytes!("../assets/media/nes-spine.png"), include_bytes!("../assets/media/nes-front.png")),
    (include_bytes!("../assets/media/gb-spine.png"), include_bytes!("../assets/media/gb-front.png")),
    (include_bytes!("../assets/media/gba-spine.png"), include_bytes!("../assets/media/gba-front.png")),
    (include_bytes!("../assets/media/cart-spine.png"), include_bytes!("../assets/media/cart-front.png")),
    (include_bytes!("../assets/media/md-spine.png"), include_bytes!("../assets/media/md-front.png")),
    (include_bytes!("../assets/media/sms-spine.png"), include_bytes!("../assets/media/sms-front.png")),
    (include_bytes!("../assets/media/ps1-spine.png"), include_bytes!("../assets/media/ps1-front.png")),
    (include_bytes!("../assets/media/segadisc-spine.png"), include_bytes!("../assets/media/segadisc-front.png")),
    (include_bytes!("../assets/media/disc-spine.png"), include_bytes!("../assets/media/disc-front.png")),
    (include_bytes!("../assets/media/ps2-spine.png"), include_bytes!("../assets/media/ps2-front.png")),
    (include_bytes!("../assets/media/gc-spine.png"), include_bytes!("../assets/media/gc-front.png")),
    (include_bytes!("../assets/media/wii-spine.png"), include_bytes!("../assets/media/wii-front.png")),
    (include_bytes!("../assets/media/keep-spine.png"), include_bytes!("../assets/media/keep-front.png")),
    (include_bytes!("../assets/media/umd-spine.png"), include_bytes!("../assets/media/umd-front.png")),
    (include_bytes!("../assets/media/ds-spine.png"), include_bytes!("../assets/media/ds-front.png")),
    (include_bytes!("../assets/media/3ds-spine.png"), include_bytes!("../assets/media/3ds-front.png")),
    (include_bytes!("../assets/media/xbox-spine.png"), include_bytes!("../assets/media/xbox-front.png")),
];

impl Medium {
    pub const ALL: [Medium; 19] = [
        Medium::Snes,
        Medium::N64,
        Medium::Nes,
        Medium::Gb,
        Medium::Gba,
        Medium::Cart,
        Medium::MegaDrive,
        Medium::MasterSystem,
        Medium::Ps1,
        Medium::SegaDisc,
        Medium::Disc,
        Medium::Ps2,
        Medium::GameCube,
        Medium::Wii,
        Medium::Keep,
        Medium::Umd,
        Medium::Ds,
        Medium::N3ds,
        Medium::Xbox,
    ];

    /// What a ROCKNIX system's games come in; a generic cartridge or disc case for the rest.
    pub fn of(system: &str) -> Medium {
        use crate::cabinet::FormFactor;
        match system {
            "snes" | "snesh" | "sfc" | "satellaview" | "sufami" => Medium::Snes,
            "n64" | "64dd" => Medium::N64,
            "nes" | "nesh" | "famicom" | "fds" => Medium::Nes,
            "gb" | "gbh" | "gbc" | "gbch" => Medium::Gb,
            "gba" | "gbah" => Medium::Gba,
            "megadrive" | "genesis" | "megadrive-japan" | "megadriveh" | "genh" | "sega32x" => Medium::MegaDrive,
            "mastersystem" | "gamegear" | "ggh" | "sg-1000" | "sg1000" => Medium::MasterSystem,
            "psx" | "ps1" | "playstation" => Medium::Ps1,
            "saturn" | "dreamcast" | "segacd" | "megacd" | "naomi" => Medium::SegaDisc,
            "ps2" => Medium::Ps2,
            "gamecube" | "gc" => Medium::GameCube,
            "wii" | "wiiware" | "wiiu" => Medium::Wii,
            "psp" | "pspockets" => Medium::Umd,
            "nds" => Medium::Ds,
            "3ds" | "n3ds" => Medium::N3ds,
            "xbox" => Medium::Xbox,
            _ => match FormFactor::of(system) {
                FormFactor::HandheldCart => Medium::Gb,
                FormFactor::ConsoleCart => Medium::Cart,
                FormFactor::DvdCase => Medium::Keep,
                FormFactor::UmdCase => Medium::Umd,
                FormFactor::CardBox => Medium::Ds,
                FormFactor::JewelCase => Medium::Disc,
            },
        }
    }

    fn index(self) -> usize {
        Medium::ALL.iter().position(|m| *m == self).unwrap_or(0)
    }

    fn template(self) -> &'static Template {
        &TEMPLATES[self.index()]
    }

    /// Its size on the Cabinet's shelf, in screen pixels: the template at `SHELF_SCALE`.
    pub fn size(self) -> Size {
        let t = self.template();
        let s = |v: i32| (v as f32 * SHELF_SCALE).round() as i32;
        Size {
            spine: s(t.spine.0),
            front: s(t.front.0),
            h: s(t.spine.1),
        }
    }

    /// The template's own size, one pixel a template pixel: the Shelf look scales it to the screen.
    pub fn template_size(self) -> Size {
        let t = self.template();
        Size {
            spine: t.spine.0,
            front: t.front.0,
            h: t.spine.1,
        }
    }

    pub fn is_cart(self) -> bool {
        self.template().tint
    }

    /// The decoded templates, (spine, front).
    fn pics(self) -> &'static (Sprite, Sprite) {
        static DECODED: [std::sync::OnceLock<(Sprite, Sprite)>; 19] = [const { std::sync::OnceLock::new() }; 19];
        let i = self.index();
        DECODED[i].get_or_init(|| {
            let empty = || Sprite { w: 1, h: 1, px: vec![0] };
            let (s, f) = PNGS[i];
            (crate::system_art::decode(s).unwrap_or_else(empty), crate::system_art::decode(f).unwrap_or_else(empty))
        })
    }
}

/// Decodes every template and prop now (on a thread of the caller's: the Cabinet's reader), so no frame decodes one.
pub fn warm() {
    for m in Medium::ALL {
        m.pics();
    }
    for p in Prop::ALL {
        p.pic();
    }
}

/// A template pixel in the system's plastic (cartridges) or as drawn (cases).
#[inline]
fn colour(p: u32, tint: Option<u32>) -> u32 {
    let Some(plastic) = tint else { return p & 0xffffff };
    // The grey's brightness, relative to the template's plastic grey, laid over the plastic.
    let l = luma(p & 0xffffff);
    let ch = |shift: u32| (((plastic >> shift & 0xff) * l / GREY).min(255)) << shift;
    ch(16) | ch(8) | ch(0)
}

/// Screen pixels of a template's span `from .. from + len` scaled by `f` (the pixels whose template pixel is in it).
fn span(from: i32, len: i32, f: f32) -> (i32, i32) {
    let a = (from as f32 * f).ceil() as i32;
    let b = ((from + len) as f32 * f).ceil() as i32;
    (a, (b - a).max(1))
}

fn scale_rect((x, y, w, h): (i32, i32, i32, i32), f: f32) -> (i32, i32, i32, i32) {
    let (x, w) = span(x, w, f);
    let (y, h) = span(y, h, f);
    (x, y, w, h)
}

/// A template scaled by `f`, nearest pixel (pixel art stays pixel art), coloured.
fn scaled(s: &Sprite, f: f32, tint: Option<u32>) -> Pic {
    let (w, h) = ((s.w as f32 * f).round() as i32, (s.h as f32 * f).round() as i32);
    let mut p = Pic::new(w, h, 0);
    // The template column of each screen column, worked out once (a front is half a million pixels at the Shelf look's
    // size).
    let cols: Vec<u32> = (0..w).map(|x| ((x as f32 / f) as u32).min(s.w - 1)).collect();
    for y in 0..h {
        let sy = ((y as f32 / f) as u32).min(s.h - 1);
        let src = &s.px[(sy * s.w) as usize..((sy + 1) * s.w) as usize];
        let dst = &mut p.px[(y * w) as usize..((y + 1) * w) as usize];
        for (d, &sx) in dst.iter_mut().zip(&cols) {
            let px = src[sx as usize];
            if px >> 24 != 0 {
                *d = 0xff00_0000 | colour(px, tint);
            }
        }
    }
    p
}

/// A picture scaled by `k`, nearest pixel (pixel art stays pixel art): the props at the Shelf look's scale.
pub fn rescaled(p: &Pic, k: f32) -> Pic {
    let (w, h) = ((p.w as f32 * k).round() as i32, (p.h as f32 * k).round() as i32);
    let mut out = Pic::new(w, h, 0);
    for y in 0..out.h {
        let sy = ((y as f32 / k) as i32).min(p.h - 1);
        for x in 0..out.w {
            out.px[(y * out.w + x) as usize] = p.px[(sy * p.w + ((x as f32 / k) as i32).min(p.w - 1)) as usize];
        }
    }
    out
}

/// A label's paper in `color`: a little lighter at the top, darker at the bottom, its edges catching the light.
fn paper(p: &mut Pic, (x, y, w, h): (i32, i32, i32, i32), color: u32) {
    for row in 0..h {
        let k = 1.05 - 0.13 * row as f32 / (h - 1).max(1) as f32;
        let c = scale(color, k);
        p.fill(x, y + row, w, 1, 0xff00_0000 | c);
    }
    // A printed label's edge: a thin darker line around it, its top catching the light.
    p.tint(x, y, w, 1, 0x000000, 70);
    p.tint(x, y + h - 1, w, 1, 0x000000, 80);
    p.tint(x, y, 1, h, 0x000000, 60);
    p.tint(x + w - 1, y, 1, h, 0x000000, 80);
    p.tint(x + 1, y + 1, w - 2, 1, 0xffffff, 50);
}

fn scale(c: u32, k: f32) -> u32 {
    let ch = |s: u32| (((c >> s & 0xff) as f32 * k).round().clamp(0.0, 255.0) as u32) << s;
    ch(16) | ch(8) | ch(0)
}

fn tint_of(m: Medium, look: Look) -> Option<u32> {
    m.is_cart().then_some(look.plastic)
}

/// The spine of a game's case standing on the shelf, `k` times its shelf size: the template, the label in the
/// cover's colour with the title read top to bottom, the band (on cases that have one) in the cover's accent.
pub fn spine(text: &mut Text, m: Medium, k: f32, look: Look, title: &str) -> Pic {
    // 24 px, or 21 on a narrow label (the Game Boy's, the thin jewel case's): the two sizes the shelf has always used,
    // so a new spine finds its letters drawn already. Bigger (a case out of the shelf) at the nearest size the app
    // paints anyway, never past the label.
    let (_, _, w, _) = scale_rect(m.template().spine_label, SHELF_SCALE * k);
    let want = (if (w as f32 / k) < 25.0 { 21.0 } else { 24.0 }) * k;
    spine_with(text, m, k, look, title, want)
}

/// The sizes a spine's title is written in: the ones the app paints anyway, so a spine made in a frame draws no
/// letters of its own (tests/shelf_moves.rs).
const SPINE_PX: [u32; 8] = [21, 24, 28, 30, 37, 44, 50, 67];

/// The title size for a spine label `w` pixels across: the largest of `SPINE_PX` up to `want` whose letters, capitals
/// to descenders, still fit across the label with a little paper on both sides; 21 at the least.
pub fn spine_px(w: i32, want: f32) -> u32 {
    SPINE_PX
        .into_iter()
        .rev()
        .find(|&s| s as f32 <= want + 0.5 && (s as f32 * 0.86).round() as i32 <= w - 6)
        .unwrap_or(SPINE_PX[0])
}

/// `spine` with the title at `want` pixels or the nearest size below it that fits the label (`spine_px`): the Shelf
/// look's spines, whose title follows Settings → Text size.
pub fn spine_with(text: &mut Text, m: Medium, k: f32, look: Look, title: &str, want: f32) -> Pic {
    let t = m.template();
    let f = SHELF_SCALE * k;
    let (spine, _) = m.pics();
    let mut p = scaled(spine, f, tint_of(m, look));
    if let Some(band) = t.band {
        let (x, y, w, h) = scale_rect(band, f);
        paper(&mut p, (x, y, w, h), look.band);
    }
    let (x, y, w, h) = scale_rect(t.spine_label, f);
    paper(&mut p, (x, y, w, h), look.color);
    let px = spine_px(w, want);
    let pad = (4.0 * k).round() as i32;
    art::spine_title(text, &mut p, title, px, x, w, y + pad, h - 2 * pad, ink_on(look.color));
    p
}

/// The size a case's front is made at for `k` times its shelf size.
pub fn front_size(m: Medium, k: f32) -> (i32, i32) {
    let t = m.template();
    let f = SHELF_SCALE * k;
    ((t.front.0 as f32 * f).round() as i32, (t.front.1 as f32 * f).round() as i32)
}

/// Where the cover sits on the front made at `k` times the shelf size: (x, y, width, height).
pub fn label_rect(m: Medium, k: f32) -> (i32, i32, i32, i32) {
    scale_rect(m.template().front_label, SHELF_SCALE * k)
}

/// The front of a case, `k` times its shelf size: the cover (`cover`, already `cabinet_art::cover_fit` to the
/// size of `label_rect`) in the case, or a plain label with the console (`art`) and the title.
pub fn front(text: &mut Text, m: Medium, k: f32, look: Look, title: &str, cover: Option<&Image>, console: Option<&Sprite>) -> Pic {
    let (_, tpl) = m.pics();
    let mut p = scaled(tpl, SHELF_SCALE * k, tint_of(m, look));
    let (lx, ly, lw, lh) = label_rect(m, k);
    match cover {
        Some(img) => {
            let w = lw.min(img.w as i32).min(p.w - lx).max(0) as usize;
            for y in 0..lh.min(img.h as i32).min(p.h - ly) {
                let src = &img.px[(y as u32 * img.w) as usize..][..w];
                let dst = &mut p.px[((ly + y) * p.w + lx) as usize..][..w];
                for (d, &s) in dst.iter_mut().zip(src) {
                    *d = 0xff00_0000 | s;
                }
            }
        }
        None => art::plain_label(text, &mut p, (lx, ly, lw, lh), look, title, console),
    }
    if !m.is_cart() {
        // The clear sleeve over the cover.
        art::sheen(&mut p, 18);
    }
    p
}

/// The two colours of a cover: the one it is mostly (`cabinet::spine_color`) and an accent, a clear colour of the
/// cover that is far from the first (the red of a logo on a blue cover); the first again when there is none.
pub fn cover_colors(px: &[u32]) -> Option<(u32, u32)> {
    let main = crate::cabinet::spine_color(px)?;
    let mut count = vec![0u32; 512];
    let mut sum = vec![[0u32; 3]; 512];
    let every = (px.len() / 40_000).max(1);
    let mut n = 0u32;
    for &p in px.iter().step_by(every) {
        let (r, g, b) = (p >> 16 & 0xff, p >> 8 & 0xff, p & 0xff);
        let k = ((r >> 5) << 6 | (g >> 5) << 3 | b >> 5) as usize;
        count[k] += 1;
        sum[k][0] += r;
        sum[k][1] += g;
        sum[k][2] += b;
        n += 1;
    }
    let dist = |a: u32, b: u32| {
        let d = |s: u32| (a >> s & 0xff) as i32 - (b >> s & 0xff) as i32;
        ((d(16).pow(2) + d(8).pow(2) + d(0).pow(2)) as f32).sqrt()
    };
    let accent = (0..512)
        // At least 1.5 % of the cover: a colour you see, not a stray pixel.
        .filter(|&k| count[k] * 66 >= n.max(1))
        .map(|k| {
            let c = count[k].max(1);
            let (r, g, b) = (sum[k][0] / c, sum[k][1] / c, sum[k][2] / c);
            let rgb = (r << 16) | (g << 8) | b;
            let (hi, lo) = (r.max(g).max(b), r.min(g).min(b));
            let sat = (hi - lo) as f32 / hi.max(1) as f32;
            let score = sat * dist(rgb, main).min(200.0) * (count[k] as f32).sqrt() * if (40..235).contains(&hi) { 1.0 } else { 0.3 };
            (rgb, score)
        })
        .filter(|(rgb, _)| dist(*rgb, main) > 70.0)
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .map_or(main, |(rgb, _)| rgb);
    Some((main, accent))
}

/// A lighter or darker shade of `c` that stands apart from `bg` (a band in the cover's own colour, say).
pub fn apart(c: u32, bg: u32) -> u32 {
    if luma(c).abs_diff(luma(bg)) > 40 {
        c
    } else if luma(bg) > 128 {
        mix(c, 0x000000, 110)
    } else {
        mix(c, 0xffffff, 90)
    }
}

/// The few things that stand on the Cabinet's shelves beside the games, at most two in view (`app_cabinet.rs`):
/// pixel art from PixelLab (`assets/media/props`, how each was made in its `pixellab-assets.json`).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Prop {
    Plant,
    Lamp,
    Cactus,
    /// Stands right after the last case of a shelf, holding the row up.
    Bookend,
}

impl Prop {
    pub const ALL: [Prop; 4] = [Prop::Plant, Prop::Lamp, Prop::Cactus, Prop::Bookend];

    /// The picture at the Cabinet's size (`SHELF_SCALE` screen pixels a pixel), decoded on first use.
    pub fn pic(self) -> &'static Pic {
        static DECODED: [std::sync::OnceLock<Pic>; 4] = [const { std::sync::OnceLock::new() }; 4];
        let i = Prop::ALL.iter().position(|p| *p == self).unwrap_or(0);
        DECODED[i].get_or_init(|| {
            let png: &[u8] = match self {
                Prop::Plant => include_bytes!("../assets/media/props/plant.png"),
                Prop::Lamp => include_bytes!("../assets/media/props/lamp.png"),
                Prop::Cactus => include_bytes!("../assets/media/props/cactus.png"),
                Prop::Bookend => include_bytes!("../assets/media/props/bookend.png"),
            };
            crate::system_art::decode(png).map_or_else(|| Pic::new(1, 1, 0), |s| scaled(&s, SHELF_SCALE, None))
        })
    }

    /// The prop that stands further along a shelf with room, after its bookend: a plant, a lamp or a cactus, picked by a
    /// number the shelf keeps (the same one every time), the tallest that fits `room_h` if the pick doesn't.
    pub fn beside(seed: u32, room_h: i32) -> Option<Prop> {
        let h = seed.wrapping_mul(2654435761) >> 7;
        let pick = [Prop::Plant, Prop::Lamp, Prop::Cactus, Prop::Plant][(h % 4) as usize];
        [pick, Prop::Plant, Prop::Cactus].into_iter().find(|p| p.pic().h <= room_h)
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_template_is_the_size_its_table_says_and_its_rectangles_are_inside() {
        for m in Medium::ALL {
            let t = m.template();
            let (s, f) = m.pics();
            assert_eq!((s.w as i32, s.h as i32), t.spine, "{m:?} spine");
            assert_eq!((f.w as i32, f.h as i32), t.front, "{m:?} front");
            for (what, (x, y, w, h), (pw, ph)) in [("spine label", t.spine_label, t.spine), ("front label", t.front_label, t.front)]
                .into_iter()
                .chain(t.band.map(|b| ("band", b, t.spine)))
            {
                assert!(x >= 0 && y >= 0 && w > 0 && h > 0 && x + w <= pw && y + h <= ph, "{m:?} {what}");
            }
            assert!(t.caps.0 + t.caps.1 < t.spine.1, "{m:?} caps");
        }
    }

    #[test]
    fn systems_get_their_own_media_and_the_rest_a_generic_one() {
        for (system, want) in [
            ("snes", Medium::Snes),
            ("sfc", Medium::Snes),
            ("n64", Medium::N64),
            ("famicom", Medium::Nes),
            ("gbc", Medium::Gb),
            ("gba", Medium::Gba),
            ("genesis", Medium::MegaDrive),
            ("gamegear", Medium::MasterSystem),
            ("psx", Medium::Ps1),
            ("dreamcast", Medium::SegaDisc),
            ("saturn", Medium::SegaDisc),
            ("ps2", Medium::Ps2),
            ("gamecube", Medium::GameCube),
            ("wii", Medium::Wii),
            ("psp", Medium::Umd),
            ("nds", Medium::Ds),
            ("3ds", Medium::N3ds),
            ("atari2600", Medium::Cart),
            ("ngp", Medium::Gb),
            ("xbox", Medium::Xbox),
            ("something-new", Medium::Disc),
        ] {
            assert_eq!(Medium::of(system), want, "{system}");
        }
    }

    #[test]
    fn heights_keep_their_real_order() {
        let h = |m: Medium| m.size().h;
        assert!(h(Medium::Gba) < h(Medium::Gb) && h(Medium::Gb) < h(Medium::Snes) && h(Medium::Snes) < h(Medium::Nes));
        assert!(h(Medium::Nes) < h(Medium::Ds) && h(Medium::Ds) < h(Medium::Umd) && h(Medium::Umd) < h(Medium::Ps2));
        assert!(h(Medium::GameCube) < h(Medium::Ps2) && h(Medium::Ps1) < h(Medium::GameCube));
    }

    /// A game without box art still shows something you can read: its system's colour on the label, its name on the
    /// spine, and a front with a plain label and its name (never an empty case).
    #[test]
    fn without_a_cover_a_case_still_shows_its_name() {
        let mut text = Text::new();
        let look = Look { color: 0x3a2a6a, band: 0x7a6cb0, plastic: 0x9c99a2 };
        for m in Medium::ALL {
            let spine = spine(&mut text, m, 1.0, look, "Chrono Trigger");
            let (x, y, w, h) = scale_rect(m.template().spine_label, SHELF_SCALE);
            let ink = ink_on(look.color);
            let inked = (y..y + h).flat_map(|r| (x..x + w).map(move |c| (c, r))).filter(|&(c, r)| spine.get(c, r) & 0xffffff == ink).count();
            assert!(inked > 20, "{m:?}: the title on the spine ({inked} pixels)");
            let front = front(&mut text, m, 1.0, look, "Chrono Trigger", None, None);
            let (fx, fy, fw, fh) = label_rect(m, 1.0);
            let colours: std::collections::HashSet<u32> = (fy..fy + fh).flat_map(|r| (fx..fx + fw).map(move |c| (c, r))).map(|(c, r)| front.get(c, r) & 0xffffff).collect();
            assert!(colours.len() > 8, "{m:?}: the plain front has a gradient and its name, not one flat colour");
        }
    }

    #[test]
    fn every_shelf_with_room_gets_a_prop_beside_its_bookend() {
        assert!((0..100).all(|s| Prop::beside(s, 300).is_some_and(|p| p != Prop::Bookend)));
        assert!((0..100).all(|s| Prop::beside(s, 80).is_none_or(|p| p.pic().h <= 80)), "never taller than the room");
        let kinds: std::collections::HashSet<Prop> = (0..100).filter_map(|s| Prop::beside(s, 300)).collect();
        assert!(kinds.len() >= 3, "not the same plant on every shelf: {kinds:?}");
        assert_eq!(Prop::beside(7, 300), Prop::beside(7, 300));
    }

    #[test]
    fn props_are_small() {
        for p in Prop::ALL {
            let pic = p.pic();
            assert!(pic.w > 25 && pic.h > 50 && pic.h < 175 && pic.w < 113, "{p:?} {}x{}", pic.w, pic.h);
        }
    }

    #[test]
    fn the_accent_is_a_clear_colour_apart_from_the_main_one() {
        // A blue cover with a red logo: blue, then red.
        let mut px = vec![0x2050c0u32; 9000];
        px.extend(std::iter::repeat_n(0xd02020, 900));
        px.extend(std::iter::repeat_n(0xffffff, 100));
        let (main, accent) = cover_colors(&px).unwrap();
        assert!(main & 0xff > 0x80 && accent >> 16 & 0xff > 0x80, "{main:06x} {accent:06x}");
        // One colour only: the accent is the same.
        let (main, accent) = cover_colors(&[0x2050c0; 400]).unwrap();
        assert_eq!(main, accent);
        assert!(cover_colors(&[]).is_none());
    }
}
