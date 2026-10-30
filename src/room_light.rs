//! Light by time of day on the Shelf and the Cabinet (Settings → Look & feel → Extras): the real clock gives the
//! room its light. Morning cool and bright, afternoon neutral, evening warm with the lamp glowing, night dim with the
//! lamp as the main light. Very subtle on purpose: it falls on the wood (boards, the cabinet's walls) and the page
//! colour behind nothing else; cases get only half its dimming and no tint, covers in the showcase, titles and hints
//! none at all, so text and covers keep their contrast.
//!
//! Cheap by construction: a [`Light`] is three channel gains and a lamp strength, quantised so it changes in small
//! steps every few minutes (the moves in between are too small to see, so a step never reads as a jump). The wood is
//! tinted once when it is made and kept in its cache under [`Light::key`]; a frame does no extra work. The app looks
//! at the clock once a minute while a Shelf or the Cabinet is on screen and asks for a frame only when the key
//! changed (`app_room_extras`); with Motion off it doesn't wake by itself, the light follows the next frame painted
//! anyway.

/// The room's light: gains per channel (256 = unchanged) for the wood, and how strongly the lamp glows (0-255).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Light {
    pub r: u16,
    pub g: u16,
    pub b: u16,
    pub lamp: u8,
}

impl Default for Light {
    fn default() -> Light {
        Light::NEUTRAL
    }
}

impl Light {
    pub const NEUTRAL: Light = Light { r: 256, g: 256, b: 256, lamp: 0 };

    /// What the caches of tinted pictures are keyed by.
    pub fn key(self) -> u64 {
        (self.r as u64) << 40 | (self.g as u64) << 24 | (self.b as u64) << 8 | self.lamp as u64
    }

    /// `p` (0RGB or ARGB) in this light, its alpha kept.
    #[inline]
    pub fn tint(self, p: u32) -> u32 {
        if self == Light::NEUTRAL {
            return p;
        }
        let ch = |v: u32, g: u16| ((v * g as u32) >> 8).min(255);
        p & 0xff00_0000 | ch(p >> 16 & 0xff, self.r) << 16 | ch(p >> 8 & 0xff, self.g) << 8 | ch(p & 0xff, self.b)
    }

    /// Every pixel of a picture in this light.
    pub fn tint_all(self, px: &mut [u32]) {
        if self == Light::NEUTRAL {
            return;
        }
        for p in px {
            *p = self.tint(*p);
        }
    }

    /// The shade (256 = unchanged, as `cabinet_art::blit` takes it) the cases and props stand in: half the wood's
    /// dimming and none of its brightening or tint, so a cover never loses contrast.
    pub fn case_shade(self) -> u32 {
        let luma = (self.r as u32 * 3 + self.g as u32 * 6 + self.b as u32) / 10;
        (128 + luma.min(256) / 2).min(256)
    }

    /// `shade` (a shelf's own darkening, 256 = none) with the room's light on it.
    pub fn shaded(self, shade: u32) -> u32 {
        (shade * self.case_shade()) >> 8
    }
}

/// Moments of the day (minute after midnight) and the light then: gains as fractions and the lamp 0-1. Between two
/// the light eases over (smoothstep), so morning, evening and night each come in over one to two hours.
const DAY: [(u32, [f32; 3], f32); 9] = [
    (0, [0.86, 0.84, 0.82], 1.0),
    (5 * 60, [0.86, 0.84, 0.82], 1.0),
    // Morning: a little brighter and cooler.
    (7 * 60 + 30, [1.0, 1.02, 1.05], 0.0),
    (10 * 60 + 30, [1.0, 1.02, 1.05], 0.0),
    // Afternoon: as made.
    (13 * 60, [1.0, 1.0, 1.0], 0.0),
    (16 * 60 + 30, [1.0, 1.0, 1.0], 0.0),
    // Evening: warmer, the lamp on.
    (18 * 60 + 30, [1.04, 0.97, 0.88], 0.7),
    (21 * 60, [1.02, 0.95, 0.86], 0.8),
    // Night: dim, the lamp the main light.
    (22 * 60 + 30, [0.86, 0.84, 0.82], 1.0),
];

/// The room's light at `minute` after midnight (fractions allowed, wraps at 24 h). `light_page`: the Light appearance,
/// where it is half as strong, so dark text on the light page keeps its contrast.
pub fn at(minute: f32, light_page: bool) -> Light {
    let m = minute.rem_euclid(1440.0);
    let i = DAY.iter().rposition(|k| k.0 as f32 <= m).unwrap_or(0);
    let (a, b) = (DAY[i], DAY[(i + 1) % DAY.len()]);
    let end = if b.0 <= a.0 { b.0 + 1440 } else { b.0 };
    let t = ((m - a.0 as f32) / (end - a.0) as f32).clamp(0.0, 1.0);
    let t = t * t * (3.0 - 2.0 * t);
    let k = if light_page { 0.5 } else { 1.0 };
    let gain = |c: usize| {
        let g = a.1[c] + (b.1[c] - a.1[c]) * t;
        // Quantised to 1/128: steps of under 1 % a channel, a few minutes apart.
        ((1.0 + (g - 1.0) * k) * 128.0).round() as u16 * 2
    };
    let lamp = a.2 + (b.2 - a.2) * t;
    Light { r: gain(0), g: gain(1), b: gain(2), lamp: ((lamp * 15.0).round() as u8) * 17 }
}

/// The minute of the day now on this device's clock (its time zone, `POCKETUI_CLOCK_SHIFT` as Automatic appearance
/// takes it); `POCKETUI_ROOM_TIME=HH:MM` sets it outright (snapshots, trying it without waiting).
pub fn minute_now() -> f32 {
    if let Some(m) = std::env::var("POCKETUI_ROOM_TIME").ok().as_deref().and_then(parse_hhmm) {
        return m;
    }
    let unix = crate::appearance::shifted_unix_now();
    let off = {
        let _zone = crate::timezone::zone_lock();
        crate::appearance::utc_offset_now(unix)
    };
    (unix + off as i64).rem_euclid(86_400) as f32 / 60.0
}

/// "21:30" as minutes after midnight.
pub fn parse_hhmm(s: &str) -> Option<f32> {
    let (h, m) = s.trim().split_once(':')?;
    let (h, m) = (h.parse::<u32>().ok()?, m.parse::<u32>().ok()?);
    (h < 24 && m < 60).then(|| (h * 60 + m) as f32)
}

/// The lamp's glow colour: a warm incandescent yellow.
pub const LAMP_GLOW: u32 = 0xffc46a;

#[cfg(test)]
mod tests {
    use super::*;

    fn warmth(l: Light) -> i32 {
        l.r as i32 - l.b as i32
    }

    #[test]
    fn the_day_runs_cool_morning_neutral_afternoon_warm_evening_dim_night() {
        let morning = at(9.0 * 60.0, false);
        let afternoon = at(15.0 * 60.0, false);
        let evening = at(20.0 * 60.0, false);
        let night = at(2.0 * 60.0, false);
        assert!(warmth(morning) < 0 && morning.g >= 256 && morning.lamp == 0, "{morning:?}");
        assert_eq!(afternoon, Light::NEUTRAL);
        assert!(warmth(evening) > 20 && evening.lamp > 150, "{evening:?}");
        assert!(night.r < 230 && night.g < 230 && night.lamp == 255, "{night:?}");
        assert!(night.case_shade() > evening.case_shade().min(256) - 40);
    }

    #[test]
    fn it_never_jumps_and_wraps_at_midnight() {
        // A minute later is at most one quantisation step per channel: no visible jumps.
        let mut prev = at(0.0, false);
        for m in 1..=1440 * 2 {
            let l = at(m as f32 / 2.0, false);
            for (a, b) in [(prev.r, l.r), (prev.g, l.g), (prev.b, l.b)] {
                assert!((a as i32 - b as i32).abs() <= 2, "minute {}: {prev:?} -> {l:?}", m as f32 / 2.0);
            }
            assert!((prev.lamp as i32 - l.lamp as i32).abs() <= 17);
            prev = l;
        }
        assert_eq!(at(1440.0 + 30.0, false), at(30.0, false));
        assert_eq!(at(-30.0, false), at(1410.0, false));
    }

    #[test]
    fn subtle_enough_for_text_and_covers() {
        for m in (0..1440).step_by(10) {
            for page in [false, true] {
                let l = at(m as f32, page);
                for g in [l.r, l.g, l.b] {
                    let (lo, hi) = if page { (230, 270) } else { (210, 270) };
                    assert!((lo..=hi).contains(&g), "{m} {page}: {l:?}");
                }
                // Cases lose at most a tenth of their brightness.
                assert!(l.case_shade() >= 230 && l.case_shade() <= 256, "{m}: {}", l.case_shade());
            }
        }
        // The light page gets half of it.
        assert!(at(120.0, true).r > at(120.0, false).r);
    }

    #[test]
    fn tint_keeps_alpha_saturates_and_neutral_is_free() {
        let l = Light { r: 280, g: 256, b: 200, lamp: 0 };
        assert_eq!(l.tint(0x80ff_8040) >> 24, 0x80);
        assert_eq!(l.tint(0x00ff_8040) >> 16 & 0xff, 0xff);
        assert_eq!(l.tint(0x0000_0080) & 0xff, 0x80 * 200 / 256);
        assert_eq!(Light::NEUTRAL.tint(0x1234_5678), 0x1234_5678);
        assert_ne!(at(120.0, false).key(), Light::NEUTRAL.key());
    }

    #[test]
    fn reads_hh_mm() {
        assert_eq!(parse_hhmm("21:30"), Some(1290.0));
        assert_eq!(parse_hhmm(" 7:05 "), Some(425.0));
        assert_eq!(parse_hhmm("24:00"), None);
        assert_eq!(parse_hhmm("noon"), None);
    }
}
