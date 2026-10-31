//! Shared rating-star painter used by the switcher and game card/box.
use crate::gfx::Canvas;

pub fn star(c: &mut Canvas, cx: i32, cy: i32, r: i32, color: u32) {
    let pts: Vec<(f32, f32)> = (0..10)
        .map(|k| {
            let a = -std::f32::consts::FRAC_PI_2 + k as f32 * std::f32::consts::PI / 5.0;
            let rr = if k % 2 == 0 { r as f32 } else { r as f32 * 0.42 };
            (cx as f32 + rr * a.cos(), cy as f32 + rr * a.sin())
        })
        .collect();
    let inside = |px: f32, py: f32| {
        let mut odd = false;
        for i in 0..pts.len() {
            let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
            if (a.1 > py) != (b.1 > py) && px < a.0 + (py - a.1) * (b.0 - a.0) / (b.1 - a.1) {
                odd = !odd;
            }
        }
        odd
    };
    for y in cy - r - 1..=cy + r + 1 {
        for x in cx - r - 1..=cx + r + 1 {
            // 4 x 4 samples per pixel.
            let n = (0..16)
                .filter(|k| inside(x as f32 + (k % 4) as f32 / 4.0 + 0.125, y as f32 + (k / 4) as f32 / 4.0 + 0.125))
                .count();
            if n > 0 {
                c.blend(x, y, color, (n * 255 / 16) as u8);
            }
        }
    }
}
