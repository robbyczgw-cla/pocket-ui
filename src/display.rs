//! Display look: colour temperature (and contrast where the display can do it).
//!
//! What ROCKNIX on the Mini V2 gives us, from its sources:
//! - sway/wlroots offers `wlr-gamma-control-unstable-v1`: a client uploads one 16-bit ramp per colour
//!   channel and the compositor puts it into the CRTC's GAMMA_LUT. That is system-wide (games included)
//!   and takes temperature and contrast, but not saturation (a 1D ramp per channel cannot mix channels;
//!   that would need the CRTC's CTM, which neither the protocol nor sway exposes).
//! - It only works if the DRM driver has a GAMMA_LUT. The Mini V2's `msm` driver (DPU, panel `ch13726a`)
//!   exposes the CTM of the DSPP block but, in mainline, no gamma LUT, so `gamma_size` is probably 0 and
//!   the compositor answers `failed`. `gamma.rs` asks and reports; nothing else decides.
//! - The panel's DDIC (ch13726a) has no colour controls in the driver, and ROCKNIX has no helper for
//!   colour (`display.contrast/saturation/hue` in its default system.cfg are not read by any script on SM8250).
//!
//! So there are two backends: `Gamma` when the compositor takes ramps, otherwise `Soft`: pocket-ui multiplies
//! the colours of its own frames, and pocket-core draws a multiply layer over the game. That covers the
//! shelf, the menus and all 13 systems pocket-core hosts, but not RetroArch or the standalone emulators.
//! Soft can only darken channels, so it does temperature and nothing else; contrast and saturation need Gamma.
//!
//! The night light (`night_light.rs`) is not a third backend: `Look::night` is its white point while it is on, and
//! `Look::gains` multiplies it with the chosen temperature. Everything that shows the look (the ramps, the Soft
//! tables, pocket-core's multiply layer) goes through `gains`, so this file is the one place that works out the
//! final colours.

use std::path::Path;

use serde::{Deserialize, Serialize};

/// Steps each way from neutral.
pub const RANGE: i8 = 10;
/// Kelvin per step: +10 = 3500 K (candle-warm), -10 = 9500 K (cool).
pub const KELVIN_PER_STEP: i32 = 300;
pub const NEUTRAL_KELVIN: i32 = 6500;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct Look {
    /// -10 (cool) ..= 10 (warm), 0 = the panel's own white.
    #[serde(default)]
    pub warmth: i8,
    /// -10 ..= 10, 0 = unchanged. Only the Gamma backend can apply it.
    #[serde(default)]
    pub contrast: i8,
    /// pocket-core should draw the multiply layer (set by pocket-ui when the backend is Soft).
    #[serde(default)]
    pub soft: bool,
    /// The night light's white point in kelvin while it is on or fading, 0 = off (`night_light.rs`). Not part of
    /// `display.json`: it follows the clock, so each program works it out itself at its start.
    #[serde(skip)]
    pub night: u16,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Backend {
    /// The compositor takes gamma ramps of this many entries.
    Gamma(u32),
    Soft,
}

impl Backend {
    pub fn can_contrast(self) -> bool {
        matches!(self, Backend::Gamma(_))
    }
}

impl Look {
    pub const FILE: &'static str = "display.json";

    pub fn load(data_dir: &Path) -> Look {
        crate::fsutil::read_json_or_backup(&data_dir.join(Self::FILE))
            .map(Look::clamped)
            .unwrap_or_default()
    }

    pub fn save(&self, data_dir: &Path) {
        let _ = crate::fsutil::write_json(&data_dir.join(Self::FILE), self);
    }

    pub fn clamped(mut self) -> Look {
        self.warmth = self.warmth.clamp(-RANGE, RANGE);
        self.contrast = self.contrast.clamp(-RANGE, RANGE);
        self
    }

    pub fn kelvin(&self) -> i32 {
        NEUTRAL_KELVIN - self.warmth as i32 * KELVIN_PER_STEP
    }

    /// Nothing to apply.
    pub fn is_neutral(&self) -> bool {
        self.warmth == 0 && self.contrast == 0 && self.night == 0
    }

    /// Per-channel multipliers (0..=1) for the white point of `kelvin()` and of the night light, relative to 6500 K,
    /// never above 1: the two multiply (the night light is warmer on top of the chosen temperature), and the
    /// strongest channel stays at full, as it does for each alone, so the picture does not get darker than it must.
    pub fn gains(&self) -> [f32; 3] {
        let (a, b) = (white_gains(self.kelvin()), white_gains(if self.night == 0 { NEUTRAL_KELVIN } else { self.night as i32 }));
        let g = [a[0] * b[0], a[1] * b[1], a[2] * b[2]];
        let max = g.iter().cloned().fold(0.0f32, f32::max).max(1e-3);
        [g[0] / max, g[1] / max, g[2] / max]
    }

    /// Contrast as a slope around mid-grey: 1.0 unchanged, 0.5..1.5 at the ends.
    pub fn slope(&self) -> f32 {
        1.0 + self.contrast as f32 * 0.05
    }

    /// The three ramps (R, G, B) of `size` entries for wlr-gamma-control, 16 bits each.
    pub fn ramps(&self, size: usize) -> Vec<u16> {
        let gains = self.gains();
        let slope = self.slope();
        let mut v = Vec::with_capacity(size * 3);
        for gain in gains {
            for i in 0..size {
                let x = if size > 1 { i as f32 / (size - 1) as f32 } else { 0.0 };
                let y = ((x - 0.5) * slope + 0.5).clamp(0.0, 1.0) * gain;
                v.push((y.clamp(0.0, 1.0) * 65535.0 + 0.5) as u16);
            }
        }
        v
    }

    /// 256-entry lookup tables for the Soft backend (multiply only).
    pub fn tables(&self) -> [[u8; 256]; 3] {
        let gains = self.gains();
        let mut t = [[0u8; 256]; 3];
        for (c, gain) in gains.iter().enumerate() {
            for (i, out) in t[c].iter_mut().enumerate() {
                *out = (i as f32 * gain + 0.5).min(255.0) as u8;
            }
        }
        t
    }

    /// Applies the Soft look to a 0RGB frame.
    pub fn apply_soft(&self, buf: &mut [u32]) {
        if self.warmth == 0 && self.night == 0 {
            return;
        }
        let t = self.tables();
        for px in buf.iter_mut() {
            let p = *px;
            *px = (t[0][(p >> 16 & 0xff) as usize] as u32) << 16 | (t[1][(p >> 8 & 0xff) as usize] as u32) << 8 | t[2][(p & 0xff) as usize] as u32;
        }
    }

    /// Name for the value on screen: "Warm 3", "Cool 2", "Neutral".
    pub fn warmth_label(&self) -> String {
        match self.warmth {
            0 => "Neutral".into(),
            w if w > 0 => format!("Warm {w}"),
            w => format!("Cool {}", -w),
        }
    }

    pub fn contrast_label(&self) -> String {
        match self.contrast {
            0 => "Normal".into(),
            c if c > 0 => format!("+{c}"),
            c => format!("{c}"),
        }
    }
}

/// Multipliers (0..=1, at most one channel at 1) that move the panel's white from 6500 K to `kelvin`.
fn white_gains(kelvin: i32) -> [f32; 3] {
    if kelvin == NEUTRAL_KELVIN {
        return [1.0; 3];
    }
    let target = kelvin_rgb(kelvin);
    let base = kelvin_rgb(NEUTRAL_KELVIN);
    let g = [target[0] / base[0], target[1] / base[1], target[2] / base[2]];
    let max = g.iter().cloned().fold(0.0f32, f32::max).max(1e-3);
    [g[0] / max, g[1] / max, g[2] / max]
}

/// Colour of a black body at `kelvin` as 0..=255 channels (Tanner Helland's fit, good from 1000 K to 40000 K).
fn kelvin_rgb(kelvin: i32) -> [f32; 3] {
    let t = kelvin.clamp(1000, 40000) as f32 / 100.0;
    let r = if t <= 66.0 { 255.0 } else { 329.698_73 * (t - 60.0).powf(-0.133_204_76) };
    let g = if t <= 66.0 {
        99.470_8 * t.ln() - 161.119_57
    } else {
        288.122_17 * (t - 60.0).powf(-0.075_514_85)
    };
    let b = if t >= 66.0 {
        255.0
    } else if t <= 19.0 {
        0.0
    } else {
        138.517_73 * (t - 10.0).ln() - 305.044_8
    };
    [r.clamp(1.0, 255.0), g.clamp(1.0, 255.0), b.clamp(1.0, 255.0)]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;

    fn look(warmth: i8, contrast: i8) -> Look {
        Look { warmth, contrast, soft: false, night: 0 }
    }

    #[test]
    fn neutral_changes_nothing() {
        let l = look(0, 0);
        assert!(l.is_neutral());
        assert_eq!(l.gains(), [1.0; 3]);
        let mut px = [0x123456u32, 0xffffff, 0];
        l.apply_soft(&mut px);
        assert_eq!(px, [0x123456, 0xffffff, 0]);
        let ramps = l.ramps(256);
        assert_eq!(ramps.len(), 768);
        assert_eq!((ramps[0], ramps[255], ramps[256 + 128], ramps[767]), (0, 65535, 32896, 65535));
    }

    #[test]
    fn warm_takes_blue_first_and_cool_takes_red_first() {
        let warm = look(10, 0).gains();
        assert_eq!(warm[0], 1.0);
        assert!(warm[1] < 0.9 && warm[2] < warm[1], "{warm:?}");
        assert!(warm[2] > 0.0, "never fully black: {warm:?}");
        let cool = look(-10, 0).gains();
        assert_eq!(cool[2], 1.0);
        assert!(cool[0] < 0.9 && cool[0] < cool[1], "{cool:?}");
        let mild = look(2, 0).gains();
        assert!(mild[2] > warm[2] && mild[2] < 1.0, "steps are gradual: {mild:?}");
        assert!(mild.iter().all(|g| (0.0..=1.0).contains(g)));
    }

    #[test]
    fn night_light_off_changes_nothing_and_on_takes_blue_first() {
        let off = Look { night: 0, ..look(0, 0) };
        assert!(off.is_neutral());
        assert_eq!(off.gains(), [1.0; 3]);
        assert_eq!(off.ramps(256), look(0, 0).ramps(256), "identity when off");
        let on = |k: u16| Look { night: k, ..look(0, 0) };
        assert!(!on(4400).is_neutral());
        let mut last = [1.0f32; 3];
        for (i, k) in crate::night_light::STRENGTH_KELVIN.iter().enumerate() {
            let g = on(*k).gains();
            assert_eq!(g[0], 1.0, "strength {}: red stays", i + 1);
            assert!(g[2] < g[1] && g[1] < 1.0, "blue goes first: {g:?}");
            assert!(g[1] < last[1] && g[2] < last[2], "each strength warmer than the last: {g:?} after {last:?}");
            assert!(g[2] > 0.3, "never black: {g:?}");
            last = g;
        }
        // Strength 3 against its black body: red 1, green about .83, blue about .67 of the panel's white.
        let g = on(4400).gains();
        assert!((g[1] - 0.83).abs() < 0.05 && (g[2] - 0.67).abs() < 0.07, "{g:?}");
    }

    #[test]
    fn night_light_combines_with_the_chosen_temperature_and_never_fights_it() {
        let night = Look { night: 4400, ..look(0, 0) }.gains();
        // Warm 5 on its own, and with the night light on top: warmer than either, nothing above 1.
        let warm = look(5, 0).gains();
        let both = Look { night: 4400, ..look(5, 0) }.gains();
        assert!(both[2] < warm[2] && both[2] < night[2], "blue is lower with both: {both:?} {warm:?} {night:?}");
        assert_eq!(both.iter().cloned().fold(0.0f32, f32::max), 1.0, "the strongest channel stays at full");
        assert!(both.iter().all(|g| (0.0..=1.0).contains(g)));
        // Cool 10 (blue full, red down) with the night light: the result is still a valid tint, red is not boosted
        // past what the cool temperature leaves of it relative to green.
        let cool = Look { night: 4400, ..look(-10, 0) }.gains();
        assert!(cool.iter().all(|g| (0.0..=1.0).contains(g)) && cool[2] < 1.0 || cool[0] < 1.0, "{cool:?}");
        // The contrast is the ramp's slope and independent of the white point.
        let a = Look { night: 4400, ..look(0, 8) }.ramps(256);
        let b = look(0, 8).ramps(256);
        assert_eq!((a[0], b[0]), (0, 0));
        assert!(a[256 + 64] < b[256 + 64], "green is lowered by the gain, as a multiplier on the contrast curve");
        assert_eq!(a[64], b[64], "red is not touched");
        // The Soft tables take it the same way.
        let mut px = [0xffffffu32];
        Look { night: 3200, soft: true, ..look(0, 0) }.apply_soft(&mut px);
        assert_eq!(px[0] >> 16, 255);
        assert!((px[0] & 0xff) < (px[0] >> 8 & 0xff) && (px[0] >> 8 & 0xff) < 255, "{:06x}", px[0]);
    }

    #[test]
    fn the_night_light_is_not_written_to_display_json() {
        let dir = TestDir::new("display-night");
        Look { night: 3800, warmth: 2, ..look(0, 0) }.save(&dir);
        let text = std::fs::read_to_string(dir.join(Look::FILE)).unwrap();
        assert!(!text.contains("night"), "{text}");
        assert_eq!(Look::load(&dir).night, 0);
    }

    #[test]
    fn kelvin_follows_the_steps() {
        assert_eq!(look(0, 0).kelvin(), 6500);
        assert_eq!(look(10, 0).kelvin(), 3500);
        assert_eq!(look(-10, 0).kelvin(), 9500);
    }

    #[test]
    fn contrast_steepens_around_mid_grey() {
        let hi = look(0, 10).ramps(256);
        let lo = look(0, -10).ramps(256);
        // At a quarter of the range the stronger contrast is darker, the weaker brighter.
        assert!(hi[64] < 16384 && lo[64] > 16384, "{} {}", hi[64], lo[64]);
        assert_eq!(hi[128] / 256, 128);
        assert_eq!((hi[0], hi[255]), (0, 65535), "clipped, not wrapped");
        assert_eq!(lo[0] > 0, true, "less contrast lifts black");
    }

    #[test]
    fn soft_tint_scales_channels_and_keeps_black() {
        let l = look(10, 0);
        let mut px = [0xffffffu32, 0x000000, 0x808080];
        l.apply_soft(&mut px);
        let g = l.gains();
        assert_eq!(px[0] >> 16, 255);
        assert_eq!((px[0] >> 8 & 0xff) as f32, (255.0 * g[1] + 0.5).floor());
        assert!((px[0] & 0xff) < (px[0] >> 8 & 0xff));
        assert_eq!(px[1], 0);
    }

    #[test]
    fn file_roundtrip_and_clamping() {
        let dir = TestDir::new("display");
        assert_eq!(Look::load(&dir), Look::default());
        Look {
            warmth: 4,
            contrast: -2,
            soft: true,
            night: 0,
        }
        .save(&dir);
        assert_eq!(
            Look::load(&dir),
            Look {
                warmth: 4,
                contrast: -2,
                soft: true,
                night: 0
            }
        );
        std::fs::write(dir.join(Look::FILE), r#"{"warmth": 90, "contrast": -90}"#).unwrap();
        assert_eq!(
            Look::load(&dir),
            Look {
                warmth: 10,
                contrast: -10,
                soft: false,
                night: 0
            }
        );
        std::fs::write(dir.join(Look::FILE), "garbage").unwrap();
        assert_eq!(Look::load(&dir), Look { warmth: 4, contrast: -2, soft: true, night: 0 }, "garbage: the last saved look comes back");
        std::fs::remove_file(crate::fsutil::backup_path(&dir.join(Look::FILE))).unwrap();
        assert_eq!(Look::load(&dir), Look::default(), "garbage and no backup: the default");
    }

    #[test]
    fn labels() {
        assert_eq!(look(0, 0).warmth_label(), "Neutral");
        assert_eq!(look(3, 0).warmth_label(), "Warm 3");
        assert_eq!(look(-2, 0).warmth_label(), "Cool 2");
        assert_eq!(look(0, 4).contrast_label(), "+4");
        assert_eq!(look(0, -4).contrast_label(), "-4");
    }
}
