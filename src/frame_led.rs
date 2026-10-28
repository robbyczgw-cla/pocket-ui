//! The power light of the handheld frames (Brick, Pocket slim, Clamshell, Wide, Gloss, Swan), live:
//! green above 20 % battery, red at or below 20 %, a slow blink at or below 5 %, and a short bright
//! flash when a state is saved (a slot or the auto state).
//!
//! The frame itself is drawn once per size and cached on the GPU (`bezel::Layer`). Only the light's
//! small square (`Led::rect`, about 60 px across) is painted again, from a copy of the frame under it
//! without the light, and uploaded with one `glTexSubImage2D` when what it shows changes: never while
//! it is steady, once a second while it blinks, four times in the half second of a flash. The battery
//! is read every 10 s (one sysfs file).

use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::bezel::{noise, smoothstep};
use crate::video::Rect;

/// Where a frame's light sits (output pixels) and how large it is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spot {
    pub x: f32,
    pub y: f32,
    pub r: f32,
}

/// The light of a drawn frame: its spot, the square it may paint (inside the image, never on the
/// picture) and that square of the frame without the light (premultiplied RGBA).
#[derive(Clone, Debug, PartialEq)]
pub struct Led {
    pub spot: Spot,
    pub rect: Rect,
    pub base: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lamp {
    Green,
    Red,
}

/// What the light shows. `flash` counts down 3, 2, 1 over a save's flash, 0 otherwise.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Look {
    pub lamp: Lamp,
    pub lit: bool,
    pub flash: u8,
}

impl Default for Look {
    /// Steady green: plenty of battery, or no battery to read (the desktop, the virtual device).
    fn default() -> Self {
        Look {
            lamp: Lamp::Green,
            lit: true,
            flash: 0,
        }
    }
}

/// Red at or below this many percent.
pub const LOW: u8 = 20;
/// Blinking at or below this many percent.
pub const CRITICAL: u8 = 5;
/// One blink: lit for the first half.
pub const BLINK_MS: u64 = 2000;
/// A save's flash, in three steps.
pub const FLASH_MS: u64 = 450;
/// How often the battery is read.
pub const READ_EVERY: Duration = Duration::from_secs(10);

/// What the light shows at `capacity` percent (None: no battery), `ms` after the clock started,
/// `since_flash` ms after the last save (None: none yet).
pub fn look_at(capacity: Option<u8>, ms: u64, since_flash: Option<u64>) -> Look {
    let lamp = match capacity {
        Some(c) if c <= LOW => Lamp::Red,
        _ => Lamp::Green,
    };
    let lit = !matches!(capacity, Some(c) if c <= CRITICAL) || ms % BLINK_MS < BLINK_MS / 2;
    let flash = match since_flash {
        Some(t) if t < FLASH_MS => 3 - (t * 3 / FLASH_MS) as u8,
        _ => 0,
    };
    Look { lamp, lit, flash }
}

/// The light's state over time: reads the battery now and then and remembers the last save.
pub struct Clock {
    dir: PathBuf,
    start: Instant,
    capacity: Option<u8>,
    read_at: Option<Instant>,
    flash_at: Option<Instant>,
    /// A save asked for a flash that hasn't been shown yet.
    flash_pending: bool,
}

impl Clock {
    /// Reads the battery in `dir` (`battery::dir()` on the device).
    pub fn new(dir: PathBuf) -> Self {
        Clock {
            dir,
            start: Instant::now(),
            capacity: None,
            read_at: None,
            flash_at: None,
            flash_pending: false,
        }
    }

    /// A state was saved: flash, from the next time the light is drawn. A save from the pause menu
    /// happens while the game (and its frame) isn't drawn, so the flash shows as the game comes back.
    pub fn flash(&mut self) {
        self.flash_pending = true;
    }

    /// What the light shows at `now`; reads the battery when the last reading is 10 s old.
    pub fn look(&mut self, now: Instant) -> Look {
        if self.read_at.is_none_or(|t| now.duration_since(t) >= READ_EVERY) {
            self.capacity = std::fs::read_to_string(self.dir.join("capacity"))
                .ok()
                .and_then(|s| s.trim().parse::<i64>().ok())
                .map(|c| c.clamp(0, 100) as u8);
            self.read_at = Some(now);
        }
        if std::mem::take(&mut self.flash_pending) {
            self.flash_at = Some(now);
        }
        let ms = now.duration_since(self.start).as_millis() as u64;
        look_at(self.capacity, ms, self.flash_at.map(|t| now.duration_since(t).as_millis() as u64))
    }
}

/// How far the halo of a flash reaches, in light radii: the square the light may paint.
const REACH: f32 = 6.0;

/// The square around `spot` the light may paint, inside a `w`x`h` image.
pub fn rect_for(spot: Spot, w: u32, h: u32) -> Rect {
    let half = (spot.r * REACH).ceil();
    let x0 = (spot.x - half).floor().clamp(0.0, w as f32) as u32;
    let y0 = (spot.y - half).floor().clamp(0.0, h as f32) as u32;
    let x1 = (spot.x + half).ceil().clamp(0.0, w as f32) as u32;
    let y1 = (spot.y + half).ceil().clamp(0.0, h as f32) as u32;
    Rect {
        x: x0,
        y: y0,
        w: x1 - x0,
        h: y1 - y0,
    }
}

/// Copies the light's square out of a frame image (before the light is painted).
pub fn capture(spot: Spot, rgba: &[u8], w: u32, h: u32) -> Led {
    let rect = rect_for(spot, w, h);
    let mut base = Vec::with_capacity((rect.w * rect.h * 4) as usize);
    for y in rect.y..rect.y + rect.h {
        let i = ((y * w + rect.x) * 4) as usize;
        base.extend_from_slice(&rgba[i..i + (rect.w * 4) as usize]);
    }
    Led { spot, rect, base }
}

/// The light's square showing `look`: premultiplied RGBA, `led.rect` large. Pixels inside `hole` (the
/// picture) and pixels the frame doesn't cover stay as they are.
pub fn paint(led: &Led, look: Look, hole: Rect) -> Vec<u8> {
    let mut out = led.base.clone();
    let Spot { x: lx, y: ly, r } = led.spot;
    let colour = match look.lamp {
        Lamp::Green => [0.32, 0.93, 0.42],
        Lamp::Red => [0.97, 0.22, 0.16],
    };
    let f = look.flash as f32 / 3.0;
    // Lit: the dome in its colour with a hot centre and a soft halo in the plastic around it. A
    // flash brightens it towards white and widens the halo. Unlit: a dark dome.
    let core = mix(colour, [1.0; 3], 0.18 + 0.55 * f);
    let (halo_k, halo_r) = (0.20 + 0.45 * f, r * (2.1 + 1.6 * f));
    let on = look.lit || look.flash > 0;
    for py in 0..led.rect.h {
        for px in 0..led.rect.w {
            let (x, y) = (led.rect.x + px, led.rect.y + py);
            if x >= hole.x && x < hole.x + hole.w && y >= hole.y && y < hole.y + hole.h {
                continue;
            }
            let i = ((py * led.rect.w + px) * 4) as usize;
            let a = led.base[i + 3] as f32 / 255.0;
            if a <= 0.0 {
                continue;
            }
            let mut c: [f32; 3] = std::array::from_fn(|k| led.base[i + k] as f32 / 255.0 / a);
            let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
            let d = ((fx - lx).powi(2) + (fy - ly).powi(2)).sqrt();
            // The hole in the plastic the light sits in: a fine dark ring.
            let ring = (-((d - r - 0.7) / 0.9).powi(2)).exp();
            c = scale(c, 1.0 - 0.45 * ring);
            let dome = 1.0 - smoothstep(r - 0.8, r + 0.6, d);
            let lens = if on {
                let hot = 1.0 - smoothstep(0.0, r * 0.9, d);
                mix(scale(colour, 0.78), core, hot)
            } else {
                // Off: the lens is dark, a hint of its colour, lit from above.
                add(scale(colour, 0.07), [0.035; 3])
            };
            c = mix(c, lens, dome);
            if on {
                // Faded out before the square's edge, so the square never shows.
                let halo = halo_k * (-(d / halo_r).powi(2)).exp() * (1.0 - smoothstep(r * 3.8, r * (REACH - 0.6), d));
                c = add(c, scale(colour, halo * (1.0 - dome)));
            }
            // The dome's own highlight, up and to the left.
            let hl = 1.0 - smoothstep(0.0, r * 0.5, ((fx - lx + r * 0.32).powi(2) + (fy - ly + r * 0.32).powi(2)).sqrt());
            c = add(c, [(if on { 0.22 } else { 0.10 }) * hl * dome; 3]);
            let dither = noise(x, y) / 255.0;
            for k in 0..3 {
                out[i + k] = (((c[k].clamp(0.0, 1.0) + dither).clamp(0.0, 1.0)) * a * 255.0 + 0.5) as u8;
            }
        }
    }
    out
}

/// Puts a painted square back into a whole frame image.
pub fn blit(rgba: &mut [u8], w: u32, rect: Rect, patch: &[u8]) {
    for row in 0..rect.h {
        let o = (((rect.y + row) * w + rect.x) * 4) as usize;
        let s = (row * rect.w * 4) as usize;
        rgba[o..o + (rect.w * 4) as usize].copy_from_slice(&patch[s..s + (rect.w * 4) as usize]);
    }
}

fn mix(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

fn scale(a: [f32; 3], s: f32) -> [f32; 3] {
    [a[0] * s, a[1] * s, a[2] * s]
}

fn add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;

    #[test]
    fn green_above_twenty_red_at_or_below_and_blinking_at_five() {
        assert_eq!(look_at(None, 0, None), Look::default(), "no battery: steady green");
        assert_eq!(
            look_at(Some(100), 0, None),
            Look {
                lamp: Lamp::Green,
                lit: true,
                flash: 0
            }
        );
        assert_eq!(
            look_at(Some(21), 1500, None),
            Look {
                lamp: Lamp::Green,
                lit: true,
                flash: 0
            }
        );
        assert_eq!(
            look_at(Some(20), 1500, None),
            Look {
                lamp: Lamp::Red,
                lit: true,
                flash: 0
            },
            "20% is red, and steady"
        );
        assert!(look_at(Some(6), 1500, None).lit);
        // At 5 % and below: lit for the first half of every 2 s.
        for (ms, lit) in [(0, true), (999, true), (1000, false), (1999, false), (2000, true), (7300, false)] {
            assert_eq!(
                look_at(Some(5), ms, None),
                Look {
                    lamp: Lamp::Red,
                    lit,
                    flash: 0
                },
                "{ms} ms"
            );
        }
        assert!(!look_at(Some(0), 1200, None).lit);
    }

    #[test]
    fn a_save_flashes_in_three_steps_even_while_blinking_off() {
        let steps: Vec<u8> = [0, 149, 150, 299, 300, 449, 450, 5000]
            .iter()
            .map(|&t| look_at(Some(50), 0, Some(t)).flash)
            .collect();
        assert_eq!(steps, [3, 3, 2, 2, 1, 1, 0, 0]);
        let off = look_at(Some(3), 1500, Some(100));
        assert!(!off.lit && off.flash == 3, "the flash shows over the blink's dark half");
    }

    #[test]
    fn the_clock_reads_the_battery_every_ten_seconds_and_flashes_on_save() {
        let dir = TestDir::new("led");
        std::fs::write(dir.join("capacity"), "64\n").unwrap();
        let mut clock = Clock::new(dir.to_path_buf());
        let t0 = clock.start;
        assert_eq!(clock.look(t0).lamp, Lamp::Green);
        std::fs::write(dir.join("capacity"), "12\n").unwrap();
        assert_eq!(clock.look(t0 + Duration::from_secs(5)).lamp, Lamp::Green, "not read again yet");
        assert_eq!(clock.look(t0 + Duration::from_secs(11)).lamp, Lamp::Red);
        clock.flash();
        // The flash starts when the light is next drawn (here 3 s after the save was asked for).
        assert_eq!(clock.look(t0 + Duration::from_millis(15_000)).flash, 3);
        assert_eq!(clock.look(t0 + Duration::from_millis(15_200)).flash, 2);
        assert_eq!(clock.look(t0 + Duration::from_millis(15_500)).flash, 0);
        // No battery at all: steady green.
        let mut none = Clock::new(PathBuf::from("/nonexistent/battery"));
        assert_eq!(none.look(Instant::now()), Look::default());
    }

    #[test]
    fn the_light_paints_only_its_square_and_never_the_picture() {
        let (w, h) = (200u32, 120u32);
        let hole = Rect { x: 60, y: 10, w: 120, h: 100 };
        // An opaque grey frame everywhere but the picture.
        let mut rgba = vec![0u8; (w * h * 4) as usize];
        for y in 0..h {
            for x in 0..w {
                if !(x >= hole.x && x < hole.x + hole.w && y >= hole.y && y < hole.y + hole.h) {
                    let i = ((y * w + x) * 4) as usize;
                    rgba[i..i + 4].copy_from_slice(&[60, 60, 60, 255]);
                }
            }
        }
        // A light close to the picture: its square reaches into it.
        let led = capture(Spot { x: 50.0, y: 40.0, r: 4.0 }, &rgba, w, h);
        assert!(led.rect.x + led.rect.w > hole.x, "the square overlaps the picture");
        for look in [
            Look::default(),
            Look {
                lamp: Lamp::Red,
                lit: false,
                flash: 0,
            },
            Look {
                lamp: Lamp::Red,
                lit: true,
                flash: 3,
            },
        ] {
            let patch = paint(&led, look, hole);
            let mut out = rgba.clone();
            blit(&mut out, w, led.rect, &patch);
            for y in 0..h {
                for x in 0..w {
                    let i = ((y * w + x) * 4) as usize;
                    let inside = x >= led.rect.x && x < led.rect.x + led.rect.w && y >= led.rect.y && y < led.rect.y + led.rect.h;
                    let in_hole = x >= hole.x && x < hole.x + hole.w && y >= hole.y && y < hole.y + hole.h;
                    if !inside || in_hole {
                        assert_eq!(&out[i..i + 4], &rgba[i..i + 4], "{look:?} touched {x},{y}");
                    }
                }
            }
            let centre = (((40 * w) + 50) * 4) as usize;
            let px = &out[centre..centre + 3];
            match (look.lamp, look.lit) {
                (Lamp::Green, true) => assert!(px[1] > 180 && px[1] > px[0] + 40, "{px:?}"),
                (Lamp::Red, false) => assert!(px.iter().all(|&v| v < 60), "off is dark: {px:?}"),
                (Lamp::Red, true) => assert!(px[0] > 230, "{px:?}"),
                _ => {}
            }
        }
        // A flash lights up more of the plastic around the light than the steady light.
        let lit_sum = |look| {
            paint(&led, look, hole)
                .chunks_exact(4)
                .map(|p| p[0] as u32 + p[1] as u32 + p[2] as u32)
                .sum::<u32>()
        };
        assert!(
            lit_sum(Look {
                lamp: Lamp::Green,
                lit: true,
                flash: 3
            }) > lit_sum(Look::default())
        );
    }
}
