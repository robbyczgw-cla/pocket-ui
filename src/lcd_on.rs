//! A handheld's display switching on and off as the real one did (Settings → Look → Cozy extras → Handheld switch-on):
//! the Game Boy's unlit olive glass going dark green before its picture rises out of it, the Game Boy Color's and the
//! GBA's reflective glass coming up from grey, the SP's front light glowing on before the picture, the PSP's backlight
//! fading the picture in softly, the Game Gear's and the Lynx's backlight flickering on over a slow, smeary LCD, the
//! DS's two backlights. Off goes the other way: the Game Boy fades back into its olive, the SP's front light drops, the
//! PSP fades to black, the Game Gear's backlight cuts out.
//!
//! Each moment is a few washes over the picture ([`Wash`]: a colour at an opacity, painted in order): pocket-core and
//! Android's player draw them as solid quads over the game (`crt_on::Quad::colored`, `Overlay::draw_solid`), the way
//! back paints them over the last picture (`eject::paint_lcd_off`). So the cost is a few fill-rate passes over the
//! picture for half a second, nothing per pixel on the CPU in the game.
//!
//! The switch-on runs where the handheld's start runs (`crt_on`'s handheld branch: Cozy, Motion Full, Old-TV
//! switch-on's rules); pocket-ui tells pocket-core the panel in [`ENV`] (absent: Handheld switch-on off, the plain
//! rise out of the dark). Without a screen frame or in full screen it runs the same: it is the display, not the frame.

use std::time::Duration;

/// Set by pocket-ui for a handheld's game with Handheld switch-on on: the panel's key ([`Panel::key`]).
pub const ENV: &str = "POCKET_LCD_PANEL";

/// The kind of display a handheld had, as far as switching it on and off shows it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Panel {
    /// The original Game Boy: a reflective STN glass, olive when unlit, slow.
    Dmg,
    /// Game Boy Pocket, Light and Color: reflective, grey-green when unlit, a little quicker.
    Color,
    /// The Game Boy Advance: reflective and dark; its picture comes up softly out of a dark grey.
    Agb,
    /// The Game Boy Advance SP: a front light that glows on before the picture.
    Sp,
    /// The PSP: a backlit TFT, the picture fades in softly from black.
    Psp,
    /// The Game Gear: a cold-cathode backlight that flickers on, a slow, smeary colour LCD.
    GameGear,
    /// The Lynx: a backlit LCD, warmer, one flicker.
    Lynx,
    /// The DS (and the 3DS): two backlights, quick.
    Ds,
    /// Any other handheld (and Handheld switch-on off): the picture comes up out of the dark.
    Plain,
}

impl Panel {
    pub const ALL: [Panel; 9] = [Panel::Dmg, Panel::Color, Panel::Agb, Panel::Sp, Panel::Psp, Panel::GameGear, Panel::Lynx, Panel::Ds, Panel::Plain];

    /// The panel of the handheld whose picture a start shows (`insert_scene::scene_system`'s ids: `gb`, `gba-sp`, …).
    pub fn of(system: &str) -> Panel {
        // The picture's id: the GBA's is the SP's when Settings → Look → Cozy extras → GBA console says so.
        match crate::system_art::picture_id(crate::insert_scene::scene_system(system)) {
            "gb" | "gbh" | "pokemini" => Panel::Dmg,
            "gbc" | "gbch" => Panel::Color,
            "gba" | "gbah" => Panel::Agb,
            "gba-sp" => Panel::Sp,
            "psp" | "psvita" => Panel::Psp,
            "gamegear" => Panel::GameGear,
            "atarilynx" | "lynx" => Panel::Lynx,
            "nds" | "3ds" | "ndsiware" => Panel::Ds,
            _ => Panel::Plain,
        }
    }

    /// Its key in [`ENV`].
    pub fn key(self) -> &'static str {
        match self {
            Panel::Dmg => "dmg",
            Panel::Color => "color",
            Panel::Agb => "agb",
            Panel::Sp => "sp",
            Panel::Psp => "psp",
            Panel::GameGear => "gamegear",
            Panel::Lynx => "lynx",
            Panel::Ds => "ds",
            Panel::Plain => "plain",
        }
    }

    pub fn from_key(key: &str) -> Panel {
        Panel::ALL.into_iter().find(|p| p.key() == key).unwrap_or(Panel::Plain)
    }

    /// How long its switch-on lasts (the game runs meanwhile).
    pub fn on_len(self) -> Duration {
        Duration::from_millis(match self {
            Panel::Dmg => 560,
            Panel::Color | Panel::Agb => 380,
            Panel::Sp => 460,
            Panel::Psp => 520,
            Panel::GameGear => 600,
            Panel::Lynx => 460,
            Panel::Ds => 340,
            Panel::Plain => 200,
        })
    }
}

/// What pocket-ui asked for (pocket-core): the panel in [`ENV`], [`Panel::Plain`] without it; read once.
pub fn panel() -> Panel {
    static PANEL: std::sync::OnceLock<Panel> = std::sync::OnceLock::new();
    *PANEL.get_or_init(|| std::env::var(ENV).map_or(Panel::Plain, |k| Panel::from_key(&k)))
}

/// The switch-on `since` it began as solid quads over the picture `area` (x, y, w, h): what pocket-core and Android's
/// player draw in place of `crt_on`'s handheld rise; empty once it is over. One quad: the washes folded into one ([`fold`]),
/// so the Brick's software picture takes one pass, not one a wash (19.11.: the Game Boy's four were 11.5 ms a frame there).
pub fn quads(panel: Panel, since: Duration, area: (f32, f32, f32, f32)) -> Vec<crate::crt_on::Quad> {
    fold(&on(panel, since)).into_iter().map(|w| crate::crt_on::Quad::colored(area, w.rgb, w.alpha)).collect()
}

/// `washes` painted in order as one wash: each is `p' = p (1 - a) + c a`, so together `p' = p k + s` with `k` the product
/// of the `(1 - a)`, and that is one wash of colour `s / (1 - k)` at `1 - k` (rounded to the colour's 8 bits).
pub fn fold(washes: &[Wash]) -> Option<Wash> {
    let (mut keep, mut add) = (1.0f32, [0.0f32; 3]);
    for w in washes {
        let a = w.alpha.clamp(0.0, 1.0);
        let c = [(w.rgb >> 16 & 255) as f32, (w.rgb >> 8 & 255) as f32, (w.rgb & 255) as f32];
        for k in 0..3 {
            add[k] = add[k] * (1.0 - a) + c[k] * a;
        }
        keep *= 1.0 - a;
    }
    let alpha = 1.0 - keep;
    if alpha <= 0.002 {
        return None;
    }
    let ch = |v: f32| ((v / alpha).round().clamp(0.0, 255.0)) as u32;
    Some(Wash { rgb: ch(add[0]) << 16 | ch(add[1]) << 8 | ch(add[2]), alpha })
}

/// The switch-on pocket-core draws over the game `since` its first picture on `screen` (the GL path's overlay and the Brick's
/// software picture alike): a handheld's own display when pocket-ui asked for one ([`panel`]), else `crt_on`'s tube or
/// plain rise; empty once it is over.
pub fn switch_on(screen: crate::crt_on::Screen, since: Duration, area: (f32, f32, f32, f32)) -> Vec<crate::crt_on::Quad> {
    match panel() {
        p if screen == crate::crt_on::Screen::Lcd && p != Panel::Plain => quads(p, since, area),
        _ => crate::crt_on::plan_for(screen, since, area),
    }
}

/// How long [`switch_on`] lasts on `screen`.
pub fn switch_on_len(screen: crate::crt_on::Screen) -> Duration {
    match panel() {
        p if screen == crate::crt_on::Screen::Lcd && p != Panel::Plain => p.on_len(),
        _ => crate::crt_on::len(screen),
    }
}

/// One layer over the picture: `rgb` (0xRRGGBB) at `alpha` (0..=1), painted over what is below.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Wash {
    pub rgb: u32,
    pub alpha: f32,
}

const BLACK: u32 = 0x000000;
/// The Game Boy's glass unlit, and a moment later with the power on it, a darker green.
const DMG_UNLIT: u32 = 0x8d9862;
const DMG_POWERED: u32 = 0x5f6c3a;
/// The slow green the Game Boy's picture keeps for a moment as it rises.
const DMG_GREEN: u32 = 0x8bac0f;
const COLOR_UNLIT: u32 = 0x9aa197;
const AGB_UNLIT: u32 = 0x4e524d;
/// The SP's front light on a dark glass: a cool white-blue.
const SP_LIGHT: u32 = 0xd6e2ff;
const SP_UNLIT: u32 = 0x24272c;
/// A backlit panel's glow with no picture yet: the backlight bleeding through the black.
const PSP_BLEED: u32 = 0x161b26;
const GG_LIGHT: u32 = 0xcfe0ff;
const LYNX_LIGHT: u32 = 0xfff0d8;

/// Soft at both ends.
fn smooth(p: f32) -> f32 {
    let p = p.clamp(0.0, 1.0);
    p * p * (3.0 - 2.0 * p)
}

/// `p` mapped from `a..b` to 0..1, clamped.
fn span(p: f32, a: f32, b: f32) -> f32 {
    ((p - a) / (b - a)).clamp(0.0, 1.0)
}

/// The washes over the picture `since` the switch-on began; empty once it is over.
pub fn on(panel: Panel, since: Duration) -> Vec<Wash> {
    let t = since.as_secs_f32() / panel.on_len().as_secs_f32();
    if t >= 1.0 {
        return Vec::new();
    }
    let w = |rgb: u32, alpha: f32| Wash { rgb, alpha: alpha.clamp(0.0, 1.0) };
    let mut out = match panel {
        Panel::Dmg => {
            // The unlit olive glass, a breath of the power darkening it to green, then the picture rising out of it, keeping
            // a green cast a little longer (the slow STN).
            let powered = smooth(span(t, 0.08, 0.28)) * (1.0 - smooth(span(t, 0.30, 0.62)));
            let unlit = 1.0 - smooth(span(t, 0.30, 0.72));
            vec![w(DMG_UNLIT, unlit), w(DMG_POWERED, powered * unlit.max(0.4) * 0.85), w(DMG_GREEN, 0.34 * (1.0 - smooth(span(t, 0.45, 1.0))))]
        }
        Panel::Color => {
            let unlit = 1.0 - smooth(span(t, 0.12, 0.78));
            vec![w(COLOR_UNLIT, unlit), w(BLACK, 0.18 * (1.0 - smooth(span(t, 0.3, 1.0))))]
        }
        Panel::Agb => {
            // Out of a dark grey glass, the contrast settling last: softly, as if a light came up.
            let unlit = 1.0 - smooth(span(t, 0.08, 0.7));
            vec![w(AGB_UNLIT, unlit), w(BLACK, 0.28 * (1.0 - smooth(span(t, 0.25, 1.0))))]
        }
        Panel::Sp => {
            // Dark glass; the front light glows on, cool and even; the picture comes up under it; the glow settles.
            let dark = 1.0 - smooth(span(t, 0.30, 0.82));
            let glow = 0.42 * smooth(span(t, 0.06, 0.30)) * (1.0 - smooth(span(t, 0.38, 1.0)));
            vec![w(SP_UNLIT, dark), w(SP_LIGHT, glow)]
        }
        Panel::Psp => {
            // Black; the backlight comes on (the black turns a lit black); the picture fades in softly.
            // Black (the backlight off) gives way to the backlight's lit black, which then clears softly off the picture.
            let cover = 1.0 - smooth(span(t, 0.22, 0.95));
            let unlit = 1.0 - smooth(span(t, 0.03, 0.2));
            vec![w(PSP_BLEED, cover), w(BLACK, unlit)]
        }
        Panel::GameGear | Panel::Lynx => {
            let (light, flickers): (u32, &[(f32, f32, f32)]) = match panel {
                Panel::GameGear => (GG_LIGHT, &[(0.06, 0.10, 0.30), (0.10, 0.14, 0.04), (0.14, 0.20, 0.34)]),
                _ => (LYNX_LIGHT, &[(0.06, 0.12, 0.28), (0.12, 0.16, 0.06)]),
            };
            // The tube's flicker as the backlight strikes, then its glow settling over a picture that smears in.
            let strike = flickers.iter().find(|(a, b, _)| t >= *a && t < *b).map(|f| f.2);
            let settle = 0.30 * (1.0 - smooth(span(t, 0.2, 1.0)));
            let glow = if t < 0.06 { 0.0 } else { strike.unwrap_or(settle) };
            let cover = 1.0 - smooth(span(t, 0.18, 0.92));
            vec![w(BLACK, cover), w(light, glow)]
        }
        Panel::Ds => {
            let cover = 1.0 - smooth(span(t, 0.1, 0.8));
            vec![w(BLACK, cover), w(0xffffff, 0.06 * smooth(span(t, 0.1, 0.4)) * (1.0 - smooth(span(t, 0.4, 1.0))))]
        }
        Panel::Plain => {
            // Dark a moment (the scene's fade left it so), then the picture comes up: `crt_on`'s handheld switch-on.
            vec![w(BLACK, 1.0 - smooth(span(t, 0.1, 1.0)))]
        }
    };
    // The reflective glasses come out of the dark the launch's fade left, not at once (no pop from black to olive or grey).
    if matches!(panel, Panel::Dmg | Panel::Color | Panel::Agb) {
        out.push(w(BLACK, 1.0 - smooth(span(t, 0.0, 0.14))));
    }
    out.retain(|w| w.alpha > 0.002);
    out
}

/// The washes over the last picture `p` (0 to 1) into its switching off (`eject::LCD_OFF`): black at the end.
pub fn off(panel: Panel, p: f32) -> Vec<Wash> {
    let p = p.clamp(0.0, 1.0);
    let w = |rgb: u32, alpha: f32| Wash { rgb, alpha: alpha.clamp(0.0, 1.0) };
    let end = smooth(span(p, 0.55, 1.0));
    let mut out = match panel {
        // The picture sinks back into the olive glass, slowly, and then the whole is dark.
        Panel::Dmg => vec![w(DMG_POWERED, 0.62 * smooth(span(p, 0.0, 0.55))), w(BLACK, end)],
        Panel::Color => vec![w(COLOR_UNLIT, 0.35 * smooth(span(p, 0.0, 0.5))), w(BLACK, smooth(span(p, 0.2, 1.0)))],
        Panel::Agb => vec![w(AGB_UNLIT, 0.5 * smooth(span(p, 0.0, 0.5))), w(BLACK, smooth(span(p, 0.25, 1.0)))],
        // The front light goes out at once: most of the picture drops; what the room lights fades after.
        Panel::Sp => vec![w(BLACK, 0.6 * smooth(span(p, 0.0, 0.14)) + 0.4 * smooth(span(p, 0.3, 1.0)))],
        Panel::Psp | Panel::Ds => vec![w(PSP_BLEED, 0.5 * smooth(span(p, 0.0, 0.4))), w(BLACK, smooth(span(p, 0.1, 0.9)))],
        // The backlight cuts out: dark at once, the slow LCD's afterimage fading in the dark.
        Panel::GameGear | Panel::Lynx => vec![w(BLACK, 0.84 * smooth(span(p, 0.0, 0.08)) + 0.16 * smooth(span(p, 0.2, 1.0)))],
        Panel::Plain => vec![w(BLACK, crate::eject::lcd_dark(p))],
    };
    out.retain(|w| w.alpha > 0.002);
    out
}

/// Paints `washes` over the `rect` (x, y, w, h) of an 0RGB picture `stride` pixels wide, in software (the Brick's picture
/// has no overlay, `soft_screen`): folded into one multiply and one add per pixel, so a frame costs one pass over the picture.
pub fn paint_washes(buf: &mut [u32], stride: usize, rect: (u32, u32, u32, u32), washes: &[Wash]) {
    if washes.is_empty() || stride == 0 {
        return;
    }
    // p' = p * keep + add, channel by channel, the washes folded in order (in 1/256ths).
    let (mut keep, mut add) = (1.0f32, [0.0f32; 3]);
    for w in washes {
        let a = w.alpha.clamp(0.0, 1.0);
        let c = [(w.rgb >> 16 & 255) as f32, (w.rgb >> 8 & 255) as f32, (w.rgb & 255) as f32];
        for k in 0..3 {
            add[k] = add[k] * (1.0 - a) + c[k] * a;
        }
        keep *= 1.0 - a;
    }
    let keep = (keep * 256.0).round() as u32;
    let add = add.map(|v| (v * 256.0).round() as u32);
    let (x, y, w, h) = rect;
    let rows = buf.len() / stride;
    for row in (y as usize..(y + h) as usize).take_while(|r| *r < rows) {
        let line = &mut buf[row * stride..][..stride];
        for p in line.iter_mut().skip(x as usize).take(w as usize) {
            let ch = |shift: u32| ((((*p >> shift) & 255) * keep + add[(2 - shift / 8) as usize]) >> 8).min(255) << shift;
            *p = ch(16) | ch(8) | ch(0);
        }
    }
}

/// How much of the picture shows through `washes` (1: all of it): what a frame's change is measured in.
pub fn shows(washes: &[Wash]) -> f32 {
    washes.iter().fold(1.0, |s, w| s * (1.0 - w.alpha))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    /// Every switch-on starts with no picture showing, ends with nothing over it, and is gone at its length.
    #[test]
    fn each_switch_on_starts_dark_and_ends_with_the_whole_picture() {
        for p in Panel::ALL {
            assert!(shows(&on(p, ms(0))) < 0.05, "{p:?}: nothing of the picture at first");
            assert!(on(p, p.on_len()).is_empty(), "{p:?}: over at its length");
            assert!(shows(&on(p, p.on_len().mul_f32(0.97))) > 0.85, "{p:?}: nearly the whole picture at its end");
            assert!(p.on_len() <= ms(600), "{p:?}: short, the game runs meanwhile");
        }
    }

    /// No frame at 60 Hz shows much more or less of the picture than the one before, but for the Game Gear's and the
    /// Lynx's backlight striking (their flicker is the point).
    #[test]
    fn no_frame_jumps_at_60_hz() {
        for p in Panel::ALL.into_iter().filter(|p| !matches!(p, Panel::GameGear | Panel::Lynx)) {
            let s: Vec<f32> = (0..=40).map(|k| shows(&on(p, Duration::from_micros(k * 16_667)))).collect();
            for (k, pair) in s.windows(2).enumerate() {
                assert!((pair[1] - pair[0]).abs() <= 0.16, "{p:?} frame {k}: {} → {}", pair[0], pair[1]);
            }
        }
    }

    /// The looks that make them themselves: the Game Boy's green, the SP's cool front light before its picture, the
    /// Game Gear's flicker, the PSP's soft fade.
    #[test]
    fn each_panel_looks_like_its_handheld() {
        let green = |ws: &[Wash]| ws.iter().any(|w| w.rgb == DMG_GREEN && w.alpha > 0.2) || ws.iter().any(|w| w.rgb == DMG_POWERED && w.alpha > 0.3);
        assert!((100..300).step_by(20).all(|t| green(&on(Panel::Dmg, ms(t)))), "the Game Boy rises green");
        let sp = |t| on(Panel::Sp, ms(t)).iter().find(|w| w.rgb == SP_LIGHT).map_or(0.0, |w| w.alpha);
        assert!(sp(130) > 0.3, "the SP's front light glows before the picture: {}", sp(130));
        assert!(shows(&on(Panel::Sp, ms(130))) < 0.5, "while its picture is still mostly dark");
        let gg: Vec<f32> = (30..130).step_by(8).map(|t| on(Panel::GameGear, ms(t)).iter().find(|w| w.rgb == GG_LIGHT).map_or(0.0, |w| w.alpha)).collect();
        assert!(gg.windows(2).any(|p| p[0] - p[1] > 0.2), "the Game Gear's backlight flickers: {gg:?}");
        let psp: Vec<f32> = (0..=26).map(|k| shows(&on(Panel::Psp, ms(k * 20)))).collect();
        assert!(psp.windows(2).all(|p| p[1] >= p[0] - 0.01), "the PSP's picture only comes up: {psp:?}");
        assert!(Panel::Psp.on_len() > Panel::Plain.on_len(), "softer than the plain rise");
    }

    /// Off: each ends black, the SP's and the Game Gear's lights drop at once, the Game Boy sinks into green first.
    #[test]
    fn each_switch_off_ends_black_its_own_way() {
        for p in Panel::ALL {
            assert!(shows(&off(p, 0.0)) > 0.99, "{p:?}: the last picture as it was");
            assert!(shows(&off(p, 1.0)) < 0.01, "{p:?}: black at the end");
        }
        assert!(shows(&off(Panel::Sp, 0.15)) < 0.45, "the front light is out at once");
        assert!(shows(&off(Panel::GameGear, 0.1)) < 0.2, "the backlight cuts out");
        assert!(shows(&off(Panel::Psp, 0.1)) > 0.6, "the PSP fades instead");
        assert!(off(Panel::Dmg, 0.4).iter().any(|w| w.rgb == DMG_POWERED && w.alpha > 0.3), "the Game Boy sinks into its green");
    }

    /// The Brick's software picture: the washes over the picture's part only, as the GPU's quads would paint them.
    #[test]
    fn washes_in_software_paint_the_picture_and_nothing_around_it() {
        let (w, h) = (8usize, 6usize);
        let mut buf = vec![0x0080_8080u32; w * h];
        paint_washes(&mut buf, w, (2, 1, 4, 3), &[Wash { rgb: 0x000000, alpha: 0.5 }, Wash { rgb: 0x00ff00, alpha: 0.25 }]);
        assert_eq!(buf[0], 0x0080_8080, "outside the picture: as it was");
        assert_eq!(buf[5 * w + 7], 0x0080_8080);
        let p = buf[2 * w + 3];
        let (r, g, b) = (p >> 16 & 255, p >> 8 & 255, p & 255);
        // 0x80 halved to 0x40, then a quarter of the way to green: r = b = 0x30, g = 0x30 + 0x40 (0x70).
        assert!(r.abs_diff(0x30) <= 1 && b.abs_diff(0x30) <= 1 && g.abs_diff(0x6f) <= 2, "{p:06x}");
        let mut none = vec![0x0012_3456u32; w * h];
        paint_washes(&mut none, w, (0, 0, w as u32, h as u32), &[]);
        assert!(none.iter().all(|&p| p == 0x0012_3456), "no washes: untouched");
        // A rectangle past the buffer's end is cut, never a panic.
        paint_washes(&mut none, w, (0, 4, 8, 10), &[Wash { rgb: 0, alpha: 1.0 }]);
        assert_eq!(none[5 * w], 0);
    }

    /// Folded, the washes paint what they paint one after the other (within a colour step), in one pass.
    #[test]
    fn the_washes_folded_into_one_paint_the_same() {
        for p in Panel::ALL {
            for k in 0..40u64 {
                let ws = on(p, Duration::from_millis(k * 15));
                let one: Vec<Wash> = fold(&ws).into_iter().collect();
                for px in [0x0000_0000u32, 0x00ff_ffff, 0x0040_8020, 0x009b_bc0f] {
                    let (mut a, mut b) = (vec![px], vec![px]);
                    paint_washes(&mut a, 1, (0, 0, 1, 1), &ws);
                    for w in &ws {
                        paint_washes(&mut b, 1, (0, 0, 1, 1), std::slice::from_ref(w));
                    }
                    let (mut c, ch) = (vec![px], |v: u32, s: u32| (v >> s & 255) as i32);
                    paint_washes(&mut c, 1, (0, 0, 1, 1), &one);
                    for s in [16, 8, 0] {
                        assert!((ch(c[0], s) - ch(b[0], s)).abs() <= 3, "{p:?} {k}: folded {:06x} against one by one {:06x}", c[0], b[0]);
                    }
                    let _ = a;
                }
            }
        }
        assert_eq!(fold(&[]), None);
    }

    #[test]
    fn the_panels_by_system_and_their_keys() {
        assert_eq!(Panel::of("gb"), Panel::Dmg);
        assert_eq!(Panel::of("supervision"), Panel::Dmg, "a handheld shown as the Game Boy");
        assert_eq!(Panel::of("gbc"), Panel::Color);
        assert_eq!(Panel::of("psp"), Panel::Psp);
        assert_eq!(Panel::of("gamegear"), Panel::GameGear);
        assert_eq!(Panel::of("atarilynx"), Panel::Lynx);
        assert_eq!(Panel::of("nds"), Panel::Ds);
        assert_eq!(Panel::of("wonderswan"), Panel::Plain);
        for p in Panel::ALL {
            assert_eq!(Panel::from_key(p.key()), p);
        }
        assert_eq!(Panel::from_key("something"), Panel::Plain);
    }
}
