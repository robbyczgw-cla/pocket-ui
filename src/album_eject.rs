//! The moment a photo is taken, over the running game (`album`'s camera kept one): a soft flash, then the instant photo
//! slides up out of the screen's bottom edge as out of an instant camera's slot, develops from milky grey to its colours
//! while the game goes on, and slides back down into the edge (into the case, where the album keeps it).
//!
//! Physics (`tests/scene_rests.rs`): the photo always comes out of the edge, its bottom never above the screen's bottom
//! edge, so it never floats over the game. The same painter draws it in pocket-core, the Android player and the
//! demo's frames (`examples/album_eject_frames.rs`).

use std::time::Duration;

use crate::cabinet_art::{self as art, Pic};
use crate::gfx::{mix, Canvas};
use crate::image::Image;

/// The whole moment, from the flash until the photo is back in the edge.
pub const LENGTH: Duration = Duration::from_millis(5800);
/// The flash: up at once, gone after this.
const FLASH: f32 = 0.28;
/// How bright the flash gets at most (of 255): soft, never a white screen.
const FLASH_MAX: f32 = 70.0;
/// The photo comes out of the slot between these (seconds), as an instant camera pushes it out.
const OUT: (f32, f32) = (0.18, 1.55);
/// It develops from milky to its colours over this, from when it starts to come out.
const DEVELOP: f32 = 3.6;
/// It slides back into the edge between these.
const BACK: (f32, f32) = (5.05, 5.8);
/// How much of the photo is out of the slot when it stands: its lowest strip stays in the edge.
const SHOWN: f32 = 0.88;

/// Where the photo stands at `t` seconds into the moment on a screen area `(x, y, w, h)`, for a photo `pw` x `ph`:
/// its top left. None before it comes out and after it is back.
pub fn place(t: f32, (x, y, w, h): (i32, i32, i32, i32), (pw, ph): (i32, i32)) -> Option<(i32, i32)> {
    let out = if t < OUT.0 {
        return None;
    } else if t < OUT.1 {
        // Pushed out by a motor: steady, a little slower at the end.
        let k = (t - OUT.0) / (OUT.1 - OUT.0);
        1.0 - (1.0 - k).powf(1.6)
    } else if t < BACK.0 {
        1.0
    } else if t < BACK.1 {
        let k = (t - BACK.0) / (BACK.1 - BACK.0);
        1.0 - k * k
    } else {
        return None;
    };
    let shown = (ph as f32 * SHOWN * out).round() as i32;
    (shown > 0).then(|| (x + w - pw - w / 14, y + h - shown))
}

/// How far the picture has developed at `t`, 0 (milky) to 1 (its colours).
pub fn developed(t: f32) -> f32 {
    let k = ((t - OUT.0) / DEVELOP).clamp(0.0, 1.0);
    k * k * (3.0 - 2.0 * k)
}

/// The picture as it develops: from a milky blue-grey through a pale, washed-out picture to its colours.
pub fn develop(img: &Image, d: f32) -> Image {
    let (shapes, colour) = parts(d);
    develop_parts(img, shapes, colour)
}

/// How far the shapes (light and dark) and the colours have come at development `d`: first the shapes, then the colours.
fn parts(d: f32) -> (f32, f32) {
    ((d * 1.6).min(1.0), ((d - 0.35) / 0.65).clamp(0.0, 1.0))
}

/// The picture with its shapes `shapes` and its colours `colour` of the way out of the milk.
pub fn develop_parts(img: &Image, shapes: f32, colour: f32) -> Image {
    const MILK: u32 = 0x9aa3ab;
    let px = img
        .px
        .iter()
        .map(|&p| {
            let (r, g, b) = ((p >> 16) & 0xff, (p >> 8) & 0xff, p & 0xff);
            let l = (r * 3 + g * 6 + b) / 10;
            let grey = mix(MILK, l * 0x010101, (shapes * 200.0) as u32);
            mix(grey, p & 0xff_ffff, (colour * 255.0) as u32)
        })
        .collect();
    Image { w: img.w, h: img.h, px }
}

/// The photo's scale on a screen `h` tall: the frame's art times this (a 1080-line screen: 5, 99 x 5 = 495 lines).
pub fn scale(h: i32) -> i32 {
    ((h as f32 * 0.46 / crate::album::FRAME.1 as f32).round() as i32).max(1)
}

/// Paints the moment `t` seconds in over the game in `area` (the whole screen): the flash, the photo in or out of the
/// slot. `picture` is the photo's picture as the album keeps it (its size gives the photo's shape).
pub fn paint(c: &mut Canvas, picture: &Image, t: f32, area: (i32, i32, i32, i32)) {
    let (x, y, w, h) = area;
    if t < FLASH {
        let a = FLASH_MAX * (1.0 - t / FLASH).powi(2);
        c.fill_rects_at(&[(x, y, w, h)], 0xfff8f0, a as u8);
    }
    let s = scale(h);
    let pic = photo(picture, s, developed(t));
    let Some((px, py)) = place(t, area, (pic.w, pic.h)) else { return };
    let clip = (y, y + h);
    // A soft shadow on the game beside it, then the photo, cut at the screen's edge (the rest is still in the slot).
    art::soft_shadow(c, px + s * 3, py + s * 4, pic.w, pic.h, s * 6, 0.45, clip);
    art::blit(c, &pic, px, py, 255, clip);
}

/// The photo's three stages for a screen `h` tall, made once off the game's thread (pocket-core's overlay blends them
/// on the GPU instead of painting each moment): milky, the shapes in grey, the colours; ARGB rows of `w` x `h`.
pub struct Stages {
    pub w: i32,
    pub h: i32,
    pub milky: Vec<u32>,
    pub shapes: Vec<u32>,
    pub colour: Vec<u32>,
}

impl Stages {
    pub fn new(picture: &Image, screen_h: i32) -> Stages {
        let s = scale(screen_h);
        let at = |shapes: f32, colour: f32| crate::album_paint::make_photo(s, (picture.w, picture.h), Some(&develop_parts(picture, shapes, colour)));
        let (m, g, c) = (at(0.0, 0.0), at(1.0, 0.0), at(1.0, 1.0));
        Stages { w: c.w, h: c.h, milky: m.px, shapes: g.px, colour: c.px }
    }
}

/// How opaque the shapes and the colours lie over the milky stage at `t` (the GPU's blend of `Stages`), as `develop`
/// mixes them.
pub fn stage_alphas(t: f32) -> (f32, f32) {
    parts(developed(t))
}

/// How bright the flash is at `t`, 0 to 1 of white.
pub fn flash(t: f32) -> f32 {
    if t < FLASH { FLASH_MAX / 255.0 * (1.0 - t / FLASH).powi(2) } else { 0.0 }
}

/// The instant photo of `picture` at scale `s`, developed `d`.
pub fn photo(picture: &Image, s: i32, d: f32) -> Pic {
    let img = develop(picture, d);
    crate::album_paint::make_photo(s, (picture.w, picture.h), Some(&img))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Nothing floats: at every moment the photo's bottom is in the screen's bottom edge (it comes out of the slot and
    /// goes back into it), and it is never wholly out.
    #[test]
    fn the_photo_always_comes_out_of_the_edge() {
        for h in [480, 768, 1080] {
            let area = (0, 0, h * 4 / 3, h);
            let ph = scale(h) * crate::album::FRAME.1;
            let pw = scale(h) * crate::album::frame_w((240, 160));
            let mut seen = 0;
            for i in 0..=(LENGTH.as_millis() as i32 / 5) {
                let t = i as f32 * 0.005;
                if let Some((_, top)) = place(t, area, (pw, ph)) {
                    assert!(top + ph >= h, "{h}: at {t}s the photo's bottom ({}) is above the edge", top + ph);
                    assert!(top > h - ph, "{h}: at {t}s the whole photo is out of the slot");
                    seen += 1;
                }
            }
            assert!(seen > 500, "it shows for most of the moment");
            assert!(place(LENGTH.as_secs_f32() + 0.01, area, (pw, ph)).is_none(), "and is gone after it");
        }
    }

    #[test]
    fn it_develops_from_milk_to_its_colours() {
        let img = Image { w: 2, h: 1, px: vec![0xd02020, 0x2040e0] };
        let milky = develop(&img, 0.0);
        assert_eq!(milky.px, vec![0x9aa3ab, 0x9aa3ab]);
        assert_eq!(develop(&img, 1.0).px, img.px);
        assert!(developed(0.0) == 0.0 && developed(OUT.0 + DEVELOP) == 1.0);
    }
}
