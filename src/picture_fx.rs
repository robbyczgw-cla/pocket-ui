//! The numbers behind pocket-core's screen effects (`src/shaders/video.frag`), and the same maths on the CPU for
//! tests: one place for every constant, handed to the shader as `#define`s (`defines`), so the shader and its
//! reference can't drift apart.
//!
//! Tuned for the Mini V2's panel: 1240×1080 at about 310 pixels per inch, looked at from 25-35 cm. One output pixel
//! is 0.08 mm there, so a pattern with a period of two pixels (the CRT mask) is beyond what the eye resolves, and a
//! scanline needs about three pixels to be a line rather than a texture.
//!
//! - **Scanlines and CRT**: each line of the picture is a beam whose height grows with its brightness (thin dark
//!   lines with gaps between them, bright lines that touch), normalised so that a line gives off the light of its
//!   colour whatever its height. The beam's profile is a plateau with soft edges (a trapezoid), averaged exactly
//!   over each output pixel's height (its integral is a few multiplications), so every line gives off exactly its
//!   light at any scale and phase: lines stay even at 4× SNES, 3.875× for 4:3 pictures at Aspect (1240×930) and
//!   every other size, without
//!   the colour of one line bleeding into the next. Computed in linear light, in the picture's own line space,
//!   from the two nearest lines; they fade out where the screen can't draw them (fewer than about 2.5 pixels a
//!   line). The CRT adds a
//!   two-pixel magenta/green mask (invisible at arm's length, it only takes the flatness off) and a faint glow
//!   from a blurred copy of the picture (its mipmaps).
//! - **LCD grid**: a gap at the start of each pixel, a fixed share of the pixel (so it grows with the scale), drawn
//!   by how much of each output pixel it covers, in linear light. Optional for the Game Boy Advance: its screen's
//!   colours (Pokefan531's public-domain GBA profile, darkened less than the original).
//! - **Crisp**: flat texels with a one-pixel blend where they meet, blended in (approximately) linear light, so a
//!   bright pixel on a dark ground keeps its weight whatever its phase on the screen.

/// Gamma of the effects' linear light: game colours are decoded with it, mixed and encoded again. 2, not the panel's
/// 2.2: a square and a square root instead of two `pow`s, which halved the CRT's cost on the host's rasteriser, and
/// mixing within a few per cent of the same light.
pub const GAMMA: f32 = 2.0;

/// Beam half-height (in lines, at half its strength) of a black line and of a full-brightness one; 0.5 closes the
/// gap. And the width of its soft edges (at least twice the dark half-height).
pub const BEAM_DARK: f32 = 0.25;
pub const BEAM_BRIGHT: f32 = 0.5;
pub const BEAM_EDGE: f32 = 0.45;
/// Scanlines fade in between these many output pixels per line.
pub const SCAN_FADE: (f32, f32) = (2.0, 3.0);

/// CRT mask: how much the off colours of a magenta/green pixel pair are dimmed, in linear light.
pub const MASK: f32 = 0.12;
/// CRT glow: share of a blurred picture mixed in, and the height in texels of the mip level it comes from (each of
/// its texels 1/28 of the picture's height: 8 lines of a SNES picture).
pub const HALO: f32 = 0.07;
pub const HALO_LINES: f32 = 28.0;

/// LCD grid: the gap's share of a pixel, how dark it is (share of light it takes, linear), the brightness given
/// back inside the cell, and the scales (output pixels per game pixel) over which it fades in.
pub const LCD_GAP: f32 = 0.18;
pub const LCD_DARK: f32 = 0.75;
pub const LCD_GAIN: f32 = 1.08;
pub const LCD_FADE: (f32, f32) = (1.5, 2.5);

/// Game Boy Advance screen colours: the profile's gamma, extra decoding gamma (the unlit screen's darkness; Pokefan531's profile uses
/// 1.0, which is very dark on an OLED), overall level and the colour matrix (columns: what red, green and blue of
/// the game become).
pub const GBA_GAMMA: f32 = 2.2;
pub const GBA_DARKEN: f32 = 0.5;
pub const GBA_LUM: f32 = 0.94;
pub const GBA_MATRIX: [[f32; 3]; 3] = [[0.82, 0.125, 0.195], [0.24, 0.665, 0.075], [-0.06, 0.21, 0.73]];

/// The constants as GLSL `#define`s, inserted after the shader's `#version` line.
pub fn defines() -> String {
    let m = GBA_MATRIX;
    format!(
        "#define GAMMA {}\n#define BEAM_DARK {}\n#define BEAM_BRIGHT {}\n#define SCAN_FADE_LO {}\n#define SCAN_FADE_HI {}\n\
         #define BEAM_EDGE {}\n#define MASK {}\n#define HALO {}\n#define HALO_LINES {}\n\
         #define LCD_GAP {}\n#define LCD_DARK {}\n#define LCD_GAIN {}\n#define LCD_FADE_LO {}\n#define LCD_FADE_HI {}\n\
         #define GBA_GAMMA {}\n#define GBA_DARKEN {}\n#define GBA_LUM {}\n#define GBA_MATRIX mat3({}, {}, {}, {}, {}, {}, {}, {}, {})\n",
        f(GAMMA),
        f(BEAM_DARK),
        f(BEAM_BRIGHT),
        f(SCAN_FADE.0),
        f(SCAN_FADE.1),
        f(BEAM_EDGE),
        f(MASK),
        f(HALO),
        f(HALO_LINES),
        f(LCD_GAP),
        f(LCD_DARK),
        f(LCD_GAIN),
        f(LCD_FADE.0),
        f(LCD_FADE.1),
        f(GBA_GAMMA),
        f(GBA_DARKEN),
        f(GBA_LUM),
        f(m[0][0]),
        f(m[0][1]),
        f(m[0][2]),
        f(m[1][0]),
        f(m[1][1]),
        f(m[1][2]),
        f(m[2][0]),
        f(m[2][1]),
        f(m[2][2]),
    )
}

/// A float as GLSL wants it: always with a decimal point.
fn f(x: f32) -> String {
    let s = format!("{x:?}");
    if s.contains('.') || s.contains('e') {
        s
    } else {
        format!("{s}.0")
    }
}

/// How many scanlines the CRT and Scanlines effects draw for a picture `src_h` rows tall. A 3D core that renders
/// above its console's lines (`native`, `pixel_path::native_lines`; N64 at 2× = 480 rows for 240 lines) gets the
/// console's lines, each from a whole group of rows; everything else one line per row (a 2D core's 448- or 480-row
/// interlaced pictures then have lines too fine for the screen, and the effect leaves them out, like a TV did).
pub fn crt_lines(src_h: u32, upscaled_3d: bool, native: Option<u32>) -> f32 {
    let h = src_h.max(1) as f32;
    match native {
        Some(n) if upscaled_3d && n > 0 => h / (h / n as f32).round().max(1.0),
        _ => h,
    }
}

/// Decoding and encoding with `GAMMA`.
pub fn lin(c: f32) -> f32 {
    c.max(0.0).powf(GAMMA)
}
pub fn enc(c: f32) -> f32 {
    c.max(0.0).powf(1.0 / GAMMA)
}

/// Luminance weights (Rec. 709) of linear colour.
pub fn luma(c: [f32; 3]) -> f32 {
    0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]
}

/// Beam half-height (lines) of one colour channel whose encoded value is `e`: each channel its own, like a TV's
/// three guns, so a bright saturated line is as tall as a white one in its colour and its plateau never has to go
/// above full brightness (a beam whose height followed the line's brightness would clip a bright red and dim it).
/// Squared: mid-tones keep visible lines, as on a TV; the gap closes towards white.
pub fn beam_width(e: f32) -> f32 {
    let t = e.clamp(0.0, 1.0);
    BEAM_DARK + (BEAM_BRIGHT - BEAM_DARK) * t * t
}

/// Antiderivative of the beam's edge ramp (0 below -EDGE/2, 1 above EDGE/2, linear between).
fn ramp_integral(t: f32) -> f32 {
    let q = (t + 0.5 * BEAM_EDGE).clamp(0.0, BEAM_EDGE);
    q * q / (2.0 * BEAM_EDGE) + (t - 0.5 * BEAM_EDGE).max(0.0)
}

/// Light of a beam of half-height `w` (lines) seen by an output pixel `px_per_line` pixels a line tall whose
/// centre is `d` lines from the beam's centre: the trapezoid profile averaged over the pixel, divided by its area
/// (2w), so a line gives off the same light whatever its height.
pub fn beam(d: f32, w: f32, px_per_line: f32) -> f32 {
    let h = 1.0 / px_per_line.max(0.25);
    let g = |x: f32| ramp_integral(w + x) - ramp_integral(w - x) - x;
    (g(d + 0.5 * h) - g(d - 0.5 * h)) / h / (2.0 * w)
}

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// One channel of the scanline effect for an output pixel whose centre is at line-space position `y` (line centres
/// at whole numbers), between line values `c0` (line floor(y)) and `c1` (the next), encoded. Returns linear light.
pub fn scanline(y: f32, c0: f32, c1: f32, px_per_line: f32) -> f32 {
    let d0 = y - y.floor();
    let (w0, w1) = (beam_width(c0), beam_width(c1));
    let b = lin(c0) * beam(d0, w0, px_per_line) + lin(c1) * beam(d0 - 1.0, w1, px_per_line);
    let flat = if d0 < 0.5 { lin(c0) } else { lin(c1) };
    let fade = smoothstep(SCAN_FADE.0, SCAN_FADE.1, px_per_line);
    flat + (b - flat) * fade
}

/// The CRT mask of output column `x`: magenta on even columns, green on odd ones, scaled so a white pair averages
/// white (before the clamp to 1).
pub fn mask(x: u32) -> [f32; 3] {
    let g = 1.0 / (1.0 - MASK / 2.0);
    let off = (1.0 - MASK) * g;
    if x.is_multiple_of(2) {
        [g, off, g]
    } else {
        [off, g, off]
    }
}

/// Crisp weight of the far texel for an output pixel at `t` (0..1) between two texel centres, at `s` output pixels
/// per texel: flat, with a one-pixel ramp around the edge between them.
pub fn crisp_weight(t: f32, s: f32) -> f32 {
    ((t - 0.5) * s.max(1.0) + 0.5).clamp(0.0, 1.0)
}

/// Share of the output pixel centred at texel position `p` (texel units) that the LCD gap covers along one axis, at
/// `s` output pixels per texel. Gaps sit at the start of each texel, `LCD_GAP` wide.
pub fn lcd_cover(p: f32, s: f32) -> f32 {
    let hp = 0.5 / s;
    let (a, b) = (p - hp, p + hp);
    let k = a.floor();
    let ov = |lo: f32, hi: f32| (b.min(hi) - a.max(lo)).max(0.0);
    ((ov(k, k + LCD_GAP) + ov(k + 1.0, k + 1.0 + LCD_GAP)) * s).min(1.0)
}

/// Light let through by the LCD grid for a pixel covering `cx` and `cy` of the gaps (0..1 each).
pub fn lcd_transmission(cx: f32, cy: f32, scale: f32) -> f32 {
    let cover = cx + cy - cx * cy;
    let fade = smoothstep(LCD_FADE.0, LCD_FADE.1, scale);
    1.0 + fade * (LCD_GAIN - 1.0 - LCD_DARK * cover * LCD_GAIN)
}

/// Game Boy Advance screen colours of an encoded game colour, in linear light (`GAMMA`).
pub fn gba_colour(c: [f32; 3]) -> [f32; 3] {
    let l = c.map(|x| (x.max(0.0).powf(GBA_GAMMA + GBA_DARKEN) * GBA_LUM).min(1.0));
    let m = GBA_MATRIX;
    [0, 1, 2].map(|row| (m[0][row] * l[0] + m[1][row] * l[1] + m[2][row] * l[2]).clamp(0.0, 1.0).powf(GAMMA / GBA_GAMMA))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_line_gives_off_the_light_of_its_colour_at_any_scale() {
        // Integer (SNES 4×), 4:3 at Aspect (PS1 240 lines: 3.875×, SNES 224: 4.15×), Integer smaller 3×, an odd
        // 3.37×, and the fade's lower end: the pixels a beam lights add up to exactly one line's light, at any phase.
        for px in [4.0, 3.875, 930.0 / 224.0, 3.0, 3.37, 2.0] {
            for w in [BEAM_DARK, 0.27, 0.38, BEAM_BRIGHT] {
                for phase in [0.0, 0.11, 0.37, 0.5, 0.83] {
                    let h = 1.0 / px;
                    let light: f32 = (-8..8).map(|r| beam(phase + (r as f32 + 0.5) * h, w, px) * h).sum();
                    assert!((light - 1.0).abs() < 1e-4, "{px} px/line, half-height {w}, phase {phase}: {light}");
                }
            }
        }
        // A flat field stays its colour on average.
        for px in [4.0, 3.875, 4.15] {
            for c in [0.1, 0.5, 1.0] {
                let rows = (40.0 * px) as usize;
                let mean = (0..rows).map(|r| scanline((r as f32 + 0.5) / px - 0.5, c, c, px)).sum::<f32>() / rows as f32;
                assert!((mean / lin(c) - 1.0).abs() < 0.01, "{px} {c}: {mean}");
            }
        }
    }

    #[test]
    fn lines_at_fractional_scales_beat_by_less_than_half_a_percent() {
        // A flat field at 4:3 Aspect (PS1 3.875 px a line, SNES 4.15), PAL 288 lines (3.23), 2.8 and 3.37: the strongest slow
        // ripple (periods from 6 px to the whole screen, the lines' own period left out) stays below 0.5 % of the
        // light (the eye's threshold at these 5-10 cycles per degree is about that).
        for px in [3.875f32, 930.0 / 224.0, 930.0 / 288.0, 3.37, 4.5, 2.8, 5.4] {
            for w in [BEAM_DARK, 0.35, 0.45] {
                let rows = 1080;
                let sig: Vec<f32> = (0..rows)
                    .map(|r| {
                        let y = (r as f32 + 0.5) / px - 0.5;
                        let d = y - y.floor();
                        beam(d, w, px) + beam(d - 1.0, w, px)
                    })
                    .collect();
                let mean = sig.iter().sum::<f32>() / rows as f32;
                let mut worst = (0.0f32, 0.0f32);
                for k in 1..rows / 6 {
                    let period = rows as f32 / k as f32;
                    if [1.0, 2.0].iter().any(|m| (period / (px * m) - 1.0).abs() < 0.03) {
                        continue;
                    }
                    let (mut re, mut im) = (0.0f32, 0.0f32);
                    for (i, v) in sig.iter().enumerate() {
                        let a = std::f32::consts::TAU * i as f32 / period;
                        re += v * a.cos();
                        im += v * a.sin();
                    }
                    let amp = 2.0 * (re * re + im * im).sqrt() / rows as f32 / mean;
                    if amp > worst.0 {
                        worst = (amp, period);
                    }
                }
                // The thinnest beams belong to near-black lines, where the eye needs several times the contrast.
                let limit = if w == BEAM_DARK { 0.01 } else { 0.005 };
                assert!(worst.0 < limit, "{px} px/line, half-height {w}: {:.2} % at {:.1} px", worst.0 * 100.0, worst.1);
            }
        }
    }

    #[test]
    fn lines_are_visible_in_the_dark_and_nearly_closed_in_highlights() {
        // Contrast between the middle of a line and the gap, at SNES 4×-ish fine sampling.
        let contrast = |c: f32| {
            let mid = scanline(0.0, c, c, 32.0);
            let gap = scanline(0.5, c, c, 32.0);
            gap / mid
        };
        assert!(contrast(0.15) < 0.1, "dark lines have clear gaps: {}", contrast(0.15));
        assert!((contrast(1.0) - 1.0).abs() < 0.01, "white lines touch: {}", contrast(1.0));
        assert!(contrast(0.5) > contrast(0.15) && contrast(0.5) < contrast(1.0));
    }

    #[test]
    fn scanlines_fade_out_where_the_screen_cannot_draw_them() {
        // 480 interlaced rows on 1080 pixels: 2.25 px a line, no lines.
        for y in [0.0, 0.3, 0.5, 0.8] {
            assert_eq!(scanline(y, 0.5, 0.5, 1.9), lin(0.5));
        }
    }

    #[test]
    fn the_beam_widens_with_brightness_and_stays_within_its_neighbours() {
        assert_eq!(beam_width(0.0), BEAM_DARK);
        assert_eq!(beam_width(1.0), BEAM_BRIGHT);
        assert!(beam_width(0.5) < (BEAM_DARK + BEAM_BRIGHT) / 2.0, "mid-tones keep their gaps");
        // The plateau (light / height) never goes above full brightness, so no colour is clipped and dimmed.
        for i in 0..=100 {
            let e = i as f32 / 100.0;
            assert!(lin(e) / (2.0 * beam_width(e)) <= 1.0 + 1e-6, "{e}");
        }
        // The trapezoid needs edges no wider than twice the thinnest beam; the widest beam ends before the next
        // line's neighbour (only two lines are read), even seen through a pixel 0.4 lines tall.
        const _: () = assert!(BEAM_EDGE <= 2.0 * BEAM_DARK);
        const _: () = assert!(BEAM_BRIGHT + BEAM_EDGE / 2.0 + 0.2 < 1.0);
        assert!(beam(1.0, BEAM_BRIGHT, 2.5).abs() < 1e-6, "nothing of a line reaches the middle of the next");
        // Area 1: the light of a line is its colour.
        let n = 2000;
        let area: f32 = (0..n).map(|i| beam(-1.0 + 2.0 * (i as f32 + 0.5) / n as f32, 0.33, 400.0) * 2.0 / n as f32).sum();
        assert!((area - 1.0).abs() < 1e-3, "{area}");
    }

    #[test]
    fn the_mask_averages_to_white_and_alternates_by_column() {
        let (a, b) = (mask(0), mask(1));
        for ch in 0..3 {
            assert!(((a[ch] + b[ch]) / 2.0 - 1.0).abs() < 1e-6);
        }
        assert_eq!(mask(2), a);
        assert!(a[1] < a[0] && b[0] < b[1], "magenta, then green");
        // Luminance of the two columns within 10 %: no visible stripes at 0.08 mm a pixel.
        assert!((luma(a) / luma(b) - 1.0).abs() < 0.10, "{} {}", luma(a), luma(b));
    }

    #[test]
    fn crisp_is_nearest_at_whole_scales_and_one_pixel_wide_otherwise() {
        for s in [2.0f32, 3.0, 4.0, 5.0, 7.0] {
            for i in 0..(s as u32) * 2 {
                // Output pixel centres between two texel centres.
                let t = (i as f32 + 0.5) / s - 0.5;
                let t = t - t.floor();
                let w = crisp_weight(t, s);
                assert!(w.min(1.0 - w) < 1e-5, "{s}×: pixel {i} t {t} blends {w}");
            }
        }
        // 4.67×: the ramp is one output pixel wide.
        let s = 4.67;
        assert_eq!(crisp_weight(0.5 - 0.5 / s, s), 0.0);
        assert_eq!(crisp_weight(0.5 + 0.5 / s, s), 1.0);
        assert!((crisp_weight(0.5, s) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn crisp_in_linear_light_keeps_a_pixels_weight_at_every_phase() {
        // A white texel on black at 4.67×: its light, summed over output columns, is the same at every phase in
        // linear light; blending encoded values (what the texture unit does) loses up to a tenth.
        let s = 4.67f32;
        let mut linear = Vec::new();
        let mut encoded = Vec::new();
        for phase in 0..10 {
            let off = phase as f32 / 10.0;
            let (mut sl, mut se) = (0.0, 0.0);
            for x in 0..20 {
                // Texel space position of the output pixel centre; the white texel is texel 2.
                let p = off + (x as f32 + 0.5) / s - 0.5;
                let i = p.floor();
                let w = crisp_weight(p - i, s);
                let val = |k: f32| if k == 2.0 { 1.0f32 } else { 0.0 };
                let (a, b) = (val(i), val(i + 1.0));
                sl += a * a + (b * b - a * a) * w; // linear (gamma 2), as the shader blends
                se += lin(a + (b - a) * w); // encoded blend, then shown
            }
            linear.push(sl / s);
            encoded.push(se / s);
        }
        let spread = |v: &[f32]| v.iter().cloned().fold(0.0, f32::max) - v.iter().cloned().fold(9.0, f32::min);
        assert!(spread(&linear) < 0.01, "{linear:?}");
        assert!(spread(&encoded) > 0.03 && encoded.iter().all(|e| *e < 0.97), "{encoded:?}");
    }

    #[test]
    fn the_lcd_gap_grows_with_the_scale_and_is_exact_at_whole_scales() {
        // 5× (GBA): 0.8 px of gap, the first pixel of each texel covered 80 %, the rest clear.
        let s = 5.0;
        let cover: Vec<f32> = (0..5).map(|i| lcd_cover((i as f32 + 0.5) / s, s)).collect();
        assert!((cover[0] - LCD_GAP * s).abs() < 1e-5 && cover[1..].iter().all(|c| *c == 0.0), "{cover:?}");
        // The gap's light share is LCD_GAP of a pixel at every scale and phase.
        for s in [2.8125f32, 4.5, 5.0, 7.0] {
            for phase in [0.0, 0.13, 0.5] {
                // A whole number of texels and of pixels: 4 texels at 2.8125× are 11.25 pixels, so 16 and 45.
                let texels = if s == 2.8125 { 16.0 } else if s == 4.5 { 2.0 } else { 1.0 };
                let n = (s * texels).round() as usize;
                let share: f32 = (0..n).map(|i| lcd_cover(phase + (i as f32 + 0.5) / s, s)).sum::<f32>() / s / texels;
                assert!((share - LCD_GAP).abs() < 0.002, "{s}× phase {phase}: {share}");
            }
        }
    }

    #[test]
    fn the_lcd_grid_fades_where_it_cannot_resolve() {
        assert_eq!(lcd_transmission(1.0, 1.0, 1.0), 1.0);
        assert!((lcd_transmission(0.0, 0.0, 5.0) - LCD_GAIN).abs() < 1e-6);
        assert!(lcd_transmission(1.0, 0.0, 5.0) < 0.5);
    }

    #[test]
    fn gba_colours_keep_white_and_darken_the_middle() {
        let w = gba_colour([1.0, 1.0, 1.0]);
        for c in w {
            assert!((c - GBA_LUM.powf(GAMMA / GBA_GAMMA)).abs() < 0.01, "{w:?}");
        }
        let g = gba_colour([0.5, 0.5, 0.5]);
        assert!(g[1] < lin(0.5), "the middle greys get darker: {g:?}");
        // Pure red picks up some green and blue: the GBA screen's colours were less saturated.
        let r = gba_colour([1.0, 0.0, 0.0]);
        assert!(r[1] > 0.1 && r[2] > 0.15, "{r:?}");
    }

    #[test]
    fn crt_lines_follow_the_console_not_the_internal_resolution() {
        assert_eq!(crt_lines(224, false, None), 224.0);
        assert_eq!(crt_lines(480, false, Some(240)), 480.0, "a 2D core's interlaced picture keeps its rows");
        assert_eq!(crt_lines(480, true, Some(240)), 240.0, "N64 at 2×");
        assert_eq!(crt_lines(960, true, Some(240)), 240.0);
        assert_eq!(crt_lines(544, true, Some(272)), 272.0, "PSP at 2×");
        assert_eq!(crt_lines(480, true, Some(480)), 480.0, "Dreamcast at 1×");
        assert_eq!(crt_lines(240, true, Some(240)), 240.0);
        assert_eq!(crt_lines(0, false, None), 1.0);
    }

    #[test]
    fn defines_are_glsl_floats() {
        let d = defines();
        assert!(d.contains("#define GAMMA 2.0\n") && d.contains("#define BEAM_DARK 0.25\n"), "{d}");
        assert!(d.contains("#define GBA_MATRIX mat3(0.82, 0.125, 0.195, 0.24,"), "{d}");
        for line in d.lines() {
            let v = line.split_whitespace().nth(2).unwrap();
            assert!(v.starts_with("mat3(") || v.contains('.'), "{line}");
        }
        assert_eq!(f(1.0), "1.0");
    }
}
