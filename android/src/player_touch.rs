//! The production touch-pad painter, shared with host-side rendering tests.
use pocket_ui::{gfx::Canvas, text::{Face, Text}, touch::Area, type_scale};
use super::play_cards;
// ---- Touch controls ------------------------------------------------------------------------------------------------
// Drawn into the overlay with real transparency: the top byte of an overlay pixel is its alpha (the shader blends it);
// pixels without one (text, the picture's frame) stay opaque as before.

/// The controls' paper, warm like the shelf's label.
const PAD_PAPER: u32 = 0xece9e1;
/// Blends `color` at `alpha` (0-255) over the overlay pixel (x, y), keeping the result's alpha in the top byte.
fn put(c: &mut Canvas, x: i32, y: i32, color: u32, alpha: u32) {
    if x < 0 || y < 0 || x >= c.w || y >= c.h || alpha == 0 {
        return;
    }
    let p = &mut c.buf[(y * c.w + x) as usize];
    let (old, old_a) = (*p & 0xff_ffff, *p >> 24);
    if old != 0 && old_a == 0 {
        // An opaque pixel already (text or a frame): mix into it and keep it opaque.
        *p = pocket_ui::gfx::mix(old, color, alpha) & 0xff_ffff;
        return;
    }
    let a = alpha + old_a * (255 - alpha) / 255;
    let channel = |shift: u32| {
        let (s, d) = ((color >> shift) & 255, (old >> shift) & 255);
        ((s * alpha + d * old_a * (255 - alpha) / 255) / a.max(1)).min(255)
    };
    *p = (a.max(1) << 24) | (channel(16) << 16) | (channel(8) << 8) | channel(0);
}
/// Paints the pixels whose signed distance `sd(x, y)` (negative inside) is below zero, smoothed over one pixel.
fn fill(c: &mut Canvas, (x0, y0, x1, y1): (i32, i32, i32, i32), color: u32, alpha: u32, sd: impl Fn(f32, f32) -> f32) {
    for y in y0.max(0)..y1.min(c.h) {
        for x in x0.max(0)..x1.min(c.w) {
            let cover = (0.5 - sd(x as f32 + 0.5, y as f32 + 0.5)).clamp(0.0, 1.0);
            if cover > 0.0 {
                put(c, x, y, color, (alpha as f32 * cover) as u32);
            }
        }
    }
}
fn round_box(px: f32, py: f32, (cx, cy, hw, hh, r): (f32, f32, f32, f32, f32)) -> f32 {
    let (qx, qy) = ((px - cx).abs() - hw + r, (py - cy).abs() - hh + r);
    (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt() + qx.max(qy).min(0.0) - r
}
fn soft_disc(c: &mut Canvas, (cx, cy, r): (f32, f32, f32), color: u32, alpha: u32) {
    let b = (cx - r - 2.0, cy - r - 2.0, cx + r + 2.0, cy + r + 2.0);
    fill(c, (b.0 as i32, b.1 as i32, b.2 as i32 + 1, b.3 as i32 + 1), color, alpha, |x, y| ((x - cx).powi(2) + (y - cy).powi(2)).sqrt() - r);
}
fn soft_ring(c: &mut Canvas, (cx, cy, r): (f32, f32, f32), t: f32, color: u32, alpha: u32) {
    let b = (cx - r - 2.0, cy - r - 2.0, cx + r + 2.0, cy + r + 2.0);
    fill(c, (b.0 as i32, b.1 as i32, b.2 as i32 + 1, b.3 as i32 + 1), color, alpha, |x, y| (((x - cx).powi(2) + (y - cy).powi(2)).sqrt() - (r - t / 2.0)).abs() - t / 2.0);
}
fn soft_pill(c: &mut Canvas, a: Area, color: u32, alpha: u32) {
    let shape = (a.x as f32 + a.w as f32 / 2.0, a.y as f32 + a.h as f32 / 2.0, a.w as f32 / 2.0, a.h as f32 / 2.0, a.h as f32 / 2.0);
    fill(c, (a.x - 1, a.y - 1, a.x + a.w + 1, a.y + a.h + 1), color, alpha, |x, y| round_box(x, y, shape));
}
/// A small filled triangle pointing from (cx, cy) towards (dx, dy) (a unit direction), `s` across.
fn arrow(c: &mut Canvas, (cx, cy): (f32, f32), (dx, dy): (f32, f32), s: f32, color: u32, alpha: u32) {
    let (tip, base) = ((cx + dx * s * 0.55, cy + dy * s * 0.55), (cx - dx * s * 0.35, cy - dy * s * 0.35));
    let (nx, ny) = (-dy, dx);
    let pts = [tip, (base.0 + nx * s * 0.5, base.1 + ny * s * 0.5), (base.0 - nx * s * 0.5, base.1 - ny * s * 0.5)];
    let edge = |p: (f32, f32), a: (f32, f32), b: (f32, f32)| {
        let (ex, ey) = (b.0 - a.0, b.1 - a.1);
        let len = (ex * ex + ey * ey).sqrt().max(0.001);
        ((p.0 - a.0) * ey - (p.1 - a.1) * ex) / len
    };
    let (x0, y0) = ((cx - s) as i32, (cy - s) as i32);
    fill(c, (x0, y0, x0 + 2 * s as i32 + 2, y0 + 2 * s as i32 + 2), color, alpha, |x, y| {
        let p = (x, y);
        let d = [edge(p, pts[0], pts[1]), edge(p, pts[1], pts[2]), edge(p, pts[2], pts[0])];
        // Inside when every edge agrees (either winding).
        if d.iter().all(|v| *v <= 0.0) { d.iter().cloned().fold(f32::MIN, f32::max) } else if d.iter().all(|v| *v >= 0.0) { -d.iter().cloned().fold(f32::MAX, f32::min) } else { 1.0 }
    });
}
/// One round face button: translucent paper, a brighter rim, the letter (or PlayStation mark) small in the middle.
fn face_button(c: &mut Canvas, text: &mut Text, (cx, cy, r): (f32, f32, f32), label: &str, psp: Option<u16>, on: bool) {
    soft_disc(c, (cx, cy, r), PAD_PAPER, if on { 120 } else { 52 });
    soft_ring(c, (cx, cy, r), (r * 0.06).max(2.0), PAD_PAPER, if on { 220 } else { 130 });
    let glyph = 0xf4f1ea;
    let (x, y) = (cx as i32, cy as i32);
    let g = (r * 0.36) as i32;
    match psp {
        Some(8) => soft_ring(c, (cx, cy, g as f32), (r * 0.09).max(3.0), glyph, 230),
        Some(1) => {
            let t = (r * 0.09).max(3.0) as i32;
            for (ax, ay, aw, ah) in [(x - g, y - g, 2 * g, t), (x - g, y + g - t, 2 * g, t), (x - g, y - g, t, 2 * g), (x + g - t, y - g, t, 2 * g)] {
                fill(c, (ax, ay, ax + aw, ay + ah), glyph, 230, |_, _| -1.0);
            }
        }
        Some(9) => {
            let t = (r * 0.09).max(3.0);
            let (top, left, right) = ((cx, cy - g as f32), (cx - g as f32, cy + g as f32 * 0.75), (cx + g as f32, cy + g as f32 * 0.75));
            for (a, b) in [(top, left), (left, right), (right, top)] {
                let n = 64;
                for i in 0..=n {
                    let f = i as f32 / n as f32;
                    soft_disc(c, (a.0 + (b.0 - a.0) * f, a.1 + (b.1 - a.1) * f, t / 2.0), glyph, 60);
                }
            }
        }
        Some(0) => {
            let t = (r * 0.09).max(3.0);
            for i in -40..=40 {
                let f = i as f32 / 40.0 * g as f32;
                soft_disc(c, (cx + f, cy + f, t / 2.0), glyph, 45);
                soft_disc(c, (cx + f, cy - f, t / 2.0), glyph, 45);
            }
        }
        _ => {
            let size = ((r * 0.62) as u32).clamp(18, 44);
            let w = text.width(Face::Title, size, label);
            text.draw(c, Face::Title, size, x - w / 2, y + type_scale::mid(size), glyph, label);
        }
    }
}
/// The on-screen pad for a phone: an 8-way D-pad, the system's face buttons in its own layout, shoulder and
/// Start/Select pills and Pause, at least 56 dp a button, by the thumbs: either side of the picture in landscape,
/// under it in portrait. Returns the buttons' touch areas (with their pad bits), the D-pad's circle and Pause.
/// The pad's touch areas with their bits, the D-pad's circle (centre and radius) and the Pause button.
pub(super) type PadLayout = (Vec<(Area, u16)>, Option<(f32, f32, f32)>, Area);
pub(super) fn touch_controls(c: &mut Canvas, text: &mut Text, system: &str, (w, h): (i32, i32), dp: f32, game: Option<Area>, pressed: u16) -> PadLayout {
    let mut controls = Vec::new();
    let d = |v: f32| (v * dp) as i32;
    let portrait = w < h;
    let edge = d(18.0);
    let buttons = play_cards::touch_faces(system).to_vec();
    let shoulders = play_cards::touch_shoulders(system);
    let (w_dp, h_dp) = (w as f32 / dp, h as f32 / dp);
    // How far the face buttons reach from the cluster's centre at a button size (dp): (left, right, up, down).
    let reach_at = |face: f32| buttons.iter().fold((0.0f32, 0.0f32, 0.0f32, 0.0f32), |(l, r, u, dn), (_, _, ox, oy, k)| {
        let s = face * 1.25;
        (l.min(ox * s - face * k), r.max(ox * s + face * k), u.min(oy * s - face * k), dn.max(oy * s + face * k))
    });
    // The clusters: D-pad diameter and face-button size, full size where the screen has the room. A phone 300-340 dp
    // wide cannot hold a 150 dp D-pad beside a face cluster 170 dp wide: the buttons shrink (never under 48 dp across,
    // the D-pad never under 96 dp) rather than lie on each other.
    let (mut face_dp, mut dpad_wide) = (30.0f32, 150.0f32);
    if portrait {
        for f in [30.0f32, 27.0, 24.0] {
            let (left, right, _, _) = reach_at(f);
            face_dp = f;
            dpad_wide = (w_dp - 36.0 - 8.0 - (right - left)).min(150.0);
            if dpad_wide >= 112.0 {
                break;
            }
        }
        dpad_wide = dpad_wide.max(96.0);
    }
    let face_r = d(face_dp) as f32;
    // Portrait: the clusters low, where the thumbs rest, shoulders over them, the small pills under them; a tall
    // picture (two DS screens) leaves less room and the D-pad shrinks, never under 112 dp.
    let below = game.map_or(h / 2, |g| g.y + g.h) + d(10.0);
    // Landscape on a short screen (a 20:9 phone is 360 dp high turned, less its gesture bar): the D-pad leaves room for
    // Pause above it and Select below.
    let dpad_d = if portrait { (h - below - d(12.0 + 44.0 + 24.0 + 16.0 + 52.0)).clamp(d(112.0), d(150.0)).min(d(dpad_wide)) } else { d(150.0f32.min(h_dp - 186.0).max(96.0)) } as f32;
    let (left_c, right_c, shoulder_y, util_y) = if portrait {
        let util_y = h - d(12.0) - d(44.0);
        let cy = util_y - d(24.0) - dpad_d as i32 / 2;
        // The shoulders stand over the highest of the clusters (the N64's C-up reaches higher than the D-pad does).
        let (_, _, up, _) = reach_at(face_dp);
        let shoulder_y = cy - (dpad_d as i32 / 2).max(d(-up)) - d(16.0) - d(52.0);
        ((edge as f32 + dpad_d / 2.0, cy as f32), ((w - edge) as f32 - dpad_d / 2.0, cy as f32), shoulder_y, util_y)
    } else {
        let (_, _, up, down) = reach_at(face_dp);
        let util_top = h_dp - 14.0 - 44.0;
        // Both clusters at 58 % of the height where it fits; the D-pad under Pause, the buttons between the shoulders
        // and Select/Start.
        let cy_left = (h_dp * 0.58).min(util_top - 4.0 - dpad_d / dp / 2.0).max(124.0 + dpad_d / dp / 2.0);
        let cy_right = (h_dp * 0.58).min(util_top - 4.0 - down).max(70.0 - up);
        ((edge as f32 + dpad_d / 2.0 + d(8.0) as f32, cy_left * dp), ((w - edge) as f32 - dpad_d / 2.0 - d(8.0) as f32, cy_right * dp), d(14.0), h - d(14.0) - d(44.0))
    };
    // D-pad: one cross, translucent, a dimple in the middle and four small arrows.
    let (cx, cy, r) = (left_c.0, left_c.1, dpad_d / 2.0);
    let arm = r * 0.72;
    let dir_on = |bit: u16| pressed & (1 << bit) != 0;
    let any = (4..8).any(dir_on);
    fill(c, ((cx - r) as i32 - 2, (cy - r) as i32 - 2, (cx + r) as i32 + 3, (cy + r) as i32 + 3), PAD_PAPER, if any { 96 } else { 52 }, |x, y| {
        round_box(x, y, (cx, cy, r, arm / 2.0, arm / 3.0)).min(round_box(x, y, (cx, cy, arm / 2.0, r, arm / 3.0)))
    });
    soft_disc(c, (cx, cy, arm * 0.28), 0x222222, 40);
    for (bit, dir) in [(4u16, (0.0, -1.0)), (5, (0.0, 1.0)), (6, (-1.0, 0.0)), (7, (1.0, 0.0))] {
        arrow(c, (cx + dir.0 * r * 0.66, cy + dir.1 * r * 0.66), dir, arm * 0.32, 0xf4f1ea, if dir_on(bit) { 255 } else { 170 });
    }
    let dpad = Some((cx, cy, r));
    // Face buttons around the right cluster's centre, in the system's own positions.
    let (fx, fy) = right_c;
    let s = face_r * 1.25;
    let psp = system == "psp";
    // The cluster stays inside the screen: a wide one (six buttons) moves in from the edge, and off the D-pad.
    let reach = buttons.iter().map(|(_, _, ox, _, k)| ox * s + face_r * k).fold(0.0f32, f32::max);
    let left_reach = buttons.iter().map(|(_, _, ox, _, k)| ox * s - face_r * k).fold(0.0f32, f32::min);
    let fx = fx.max(left_c.0 + dpad_d / 2.0 + d(8.0) as f32 - left_reach).min((w - edge) as f32 - reach);
    for (label, bit, ox, oy, k) in buttons {
        let (bx, by, br) = (fx + ox * s, fy + oy * s, face_r * k);
        // The N64's C marks are not in the UI font: small drawn arrows instead.
        if system == "n64" && k < 0.9 {
            soft_disc(c, (bx, by, br), 0xe8c66a, if pressed & (1 << bit) != 0 { 140 } else { 70 });
            let dir = match bit { 8 => (-1.0, 0.0), 9 => (0.0, -1.0), 13 => (1.0, 0.0), _ => (0.0, 1.0) };
            arrow(c, (bx, by), dir, br * 0.55, 0xf4f1ea, 220);
        } else {
            face_button(c, text, (bx, by, br), label, psp.then_some(bit), pressed & (1 << bit) != 0);
        }
        let hit = (br * 1.25) as i32;
        controls.push((Area::new(bx as i32 - hit, by as i32 - hit, 2 * hit, 2 * hit), bit));
    }
    // Shoulders: pills at the top corners (landscape) or over the clusters (portrait).
    // Three pills (N64: L, Z, R) in 300 dp are 81 dp wide each, not 96: they stand side by side, not on each other.
    let sw_dp = (96.0f32).min((w_dp - 36.0 - 10.0 * (shoulders.len().max(1) as f32 - 1.0)) / shoulders.len().max(1) as f32).max(64.0);
    let (sw, sh) = (d(sw_dp), d(52.0));
    let mut left_x = edge;
    for (label, bit, right) in shoulders {
        let x = if *right { w - edge - sw } else { left_x };
        if !right {
            left_x += sw + d(10.0);
        }
        let area = Area::new(x, shoulder_y, sw, sh);
        let on = pressed & (1 << bit) != 0;
        soft_pill(c, area, PAD_PAPER, if on { 120 } else { 52 });
        let size = (d(18.0) as u32).clamp(16, 40);
        let tw = text.width(Face::Title, size, label);
        text.draw(c, Face::Title, size, area.x + (area.w - tw) / 2, area.y + area.h / 2 + type_scale::mid(size), 0xf4f1ea, label);
        controls.push((area, *bit));
    }
    // Select, Pause and Start: small pills at the bottom, in the middle in portrait, under the clusters in landscape.
    let (uw, uh) = (d(76.0), d(44.0));
    let utility: Vec<(&str, Option<u16>, i32)> = if portrait {
        vec![("SELECT", Some(2), w / 2 - uw * 3 / 2 - d(10.0)), ("Pause", None, w / 2 - uw / 2), ("START", Some(3), w / 2 + uw / 2 + d(10.0))]
    } else {
        vec![("SELECT", Some(2), left_c.0 as i32 - uw / 2), ("Pause", None, w / 2 - uw / 2), ("START", Some(3), right_c.0 as i32 - uw / 2)]
    };
    let mut menu = Area::new(0, 0, 0, 0);
    for (label, bit, x) in utility {
        if matches!(system,"n64"|"saturn"|"dreamcast") && bit == Some(2) {
            continue;
        }
        // Pause in landscape sits under the left shoulder, beside the picture rather than over it.
        let area = if bit.is_none() && !portrait {
            Area::new(left_c.0 as i32 - uw / 2, shoulder_y + d(52.0) + d(10.0), uw, uh)
        } else {
            Area::new(x, util_y, uw, uh)
        };
        let on = bit.is_some_and(|b| pressed & (1 << b) != 0);
        soft_pill(c, area, PAD_PAPER, if on { 120 } else { if bit.is_none() { 64 } else { 44 } });
        let size = (d(11.0) as u32).clamp(14, 28);
        let tw = text.width(Face::Body, size, label);
        text.draw(c, Face::Body, size, area.x + (area.w - tw) / 2, area.y + area.h / 2 + type_scale::mid(size), 0xf4f1ea, label);
        match bit {
            Some(b) => controls.push((area, b)),
            None => menu = area,
        }
    }
    (controls, dpad, menu)
}
