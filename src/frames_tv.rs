//! Built-in screen frames after home TVs and monitors: Studio monitor, Portable TV and Woodgrain.
//! Same rules as `frames.rs`: drawn in code at the output size, inside the bands, never on the
//! picture; original designs with knobs, dials and grilles but no lettering or logos.

use crate::bezel::{smoothstep, Bezel};
use crate::frames::{add, grow, hole, lambert, mix, normal, scale, specular, vnoise, wood, Box4, Canvas, Rgb, LIGHT};
use crate::frames_more::{corner_max, fit, sd_rr4, Material, Surface};
use crate::video::Rect;

fn thinnest(b: [f32; 4]) -> f32 {
    b.iter().cloned().fold(f32::MAX, f32::min)
}

fn to_pic(x: f32, y: f32, pic: Box4) -> f32 {
    let dx = (pic.0 - x).max(x - pic.2).max(0.0);
    let dy = (pic.1 - y).max(y - pic.3).max(0.0);
    (dx * dx + dy * dy).sqrt()
}

/// Tube glass between the picture and the opening's edge `dop` px away (negative inside): near
/// black, a faint glow from the picture, a crisp reflection along its top.
fn tube(x: f32, y: f32, pic: Box4, opening: Box4, dop: f32, px: f32, crisp: f32) -> Rgb {
    let near = (-to_pic(x, y, pic) / (px * 2.5)).exp();
    let v = (y - opening.1) / (opening.3 - opening.1);
    let shade = 0.55 + 0.45 * smoothstep(0.0, 5.0, -dop);
    let mut c = add(scale([0.034, 0.037, 0.041], shade), [0.022 * near; 3]);
    // A thin bright reflection just under the top edge of the glass.
    c = add(c, [crisp * (-((dop + 2.2) / 0.9).powi(2)).exp() * (1.0 - smoothstep(0.0, 0.1, v)); 3]);
    c
}

/// A sloped inner bezel `t` (0 at the opening .. 1 at the body) round the opening, facing into the
/// tube: dark on top, lit below. (nx, ny) is the opening's outward normal.
fn inner_bezel(base: Rgb, (nx, ny): (f32, f32), t: f32) -> Rgb {
    let slope = 0.75 * (1.0 - t) + 0.25;
    scale(base, lambert(-nx, -ny, slope).clamp(0.25, 1.9))
}

/// A round knob seen from the front: a soft shadow, a ribbed rim, a domed cap and a pointer line.
fn knob(c: Rgb, x: f32, y: f32, (kx, ky): (f32, f32), r: f32, angle: f32, cap: Rgb) -> Rgb {
    let (dx, dy) = (x - kx, y - ky);
    let d = (dx * dx + dy * dy).sqrt();
    // Shadow down and to the right.
    let sd = ((dx - r * 0.18).powi(2) + (dy - r * 0.25).powi(2)).sqrt();
    let mut c = scale(c, 1.0 - 0.45 * (1.0 - smoothstep(r * 0.9, r * 1.35, sd)));
    let cover = 1.0 - smoothstep(r - 0.7, r + 0.5, d);
    if cover <= 0.0 {
        return c;
    }
    let (nx, ny) = (dx / r, dy / r);
    let nz = (1.0 - (nx * nx + ny * ny).min(1.0)).sqrt();
    let lit = ((nx * LIGHT[0] + ny * LIGHT[1] + nz * LIGHT[2]) / LIGHT[2]).clamp(0.3, 1.4);
    let mut k = scale(cap, lit);
    // Ribs round the rim, a flat top.
    let rim = smoothstep(r * 0.62, r * 0.72, d);
    let phi = dy.atan2(dx);
    k = scale(k, 1.0 - rim * 0.18 * (0.5 + 0.5 * (phi * 28.0).cos()));
    k = add(
        k,
        [0.10 * (1.0 - smoothstep(0.0, r * 0.45, ((dx + r * 0.3).powi(2) + (dy + r * 0.35).powi(2)).sqrt())); 3],
    );
    let (px, py) = (angle.cos(), angle.sin());
    let along = dx * px + dy * py;
    let across = (dx * py - dy * px).abs();
    if along > r * 0.15 && along < r * 0.62 {
        k = mix(k, [0.05; 3], (1.0 - smoothstep(0.6, 1.4, across)) * 0.85);
    }
    c = mix(c, k, cover);
    c
}

// --- Studio monitor ---------------------------------------------------------------------------------

/// A grey professional monitor: a flat-ish tube with crisp glass in a rectangular opening, a narrow
/// sloped inner bezel, warm grey plastic, and below the picture a recessed strip with knobs and push
/// buttons.
pub(crate) fn studio(view_w: u32, view_h: u32, game: Rect, px: f32, b: [f32; 4]) -> Bezel {
    let mut canvas = Canvas::new(view_w, view_h);
    let pic = hole(game);
    let plate = grow(pic, b);
    let h = game.h as f32;
    let thin = thinnest(b);
    let g = (thin * 0.14).clamp(2.0, 16.0);
    let opening = grow(pic, [g; 4]);
    let open_r = fit([h * 0.012; 4], opening, pic, [0.0; 4], 0.8);
    let ring = (thin * 0.2).clamp(2.5, 20.0);
    let outer_ring = grow(opening, [ring; 4]);
    let ring_r = open_r.map(|r| r + ring * 0.6);
    let body_r = fit([h * 0.022; 4], plate, outer_ring, ring_r, 1.0);
    let mat = Material::new([0.60, 0.598, 0.583], Surface::Matte, 0.10);
    let bezel_c = [0.21, 0.21, 0.215];
    let rim = (thin * 0.4).clamp(3.0, 12.0);
    // The control strip: under the picture, right-aligned, when the band below has room.
    let room = plate.3 - outer_ring.3;
    let strip = (room >= 30.0).then(|| {
        let sh = (room * 0.5).min(h * 0.07);
        let y0 = outer_ring.3 + (room - sh) / 2.0;
        (pic.0 + game.w as f32 * 0.46, y0, pic.2 - game.w as f32 * 0.02, y0 + sh)
    });
    let px_ = px.max(1.0);
    canvas.paint(plate, game, |x, y| {
        let body = |x: f32, y: f32| sd_rr4(x, y, plate, body_r);
        let d = body(x, y);
        let cover = (0.5 - d).clamp(0.0, 1.0);
        if cover <= 0.0 {
            return None;
        }
        let dop = sd_rr4(x, y, opening, open_r);
        let c = if dop < 0.0 {
            tube(x, y, pic, opening, dop, px_, 0.06)
        } else if dop < ring {
            let n = normal(|x, y| sd_rr4(x, y, opening, open_r), x, y);
            let mut c = inner_bezel(bezel_c, n, dop / ring);
            c = mix(c, [0.02; 3], smoothstep(1.0, 0.0, dop));
            // The crest where the bezel meets the body.
            add(c, [0.04 * (-((dop - ring + 1.0) / 0.9).powi(2)).exp(); 3])
        } else {
            let n = if -d < rim * 1.2 + 3.0 { normal(body, x, y) } else { (0.0, 0.0) };
            let (u, v) = ((x - plate.0) / (plate.2 - plate.0), (y - plate.1) / (plate.3 - plate.1));
            let mut c = mat.shade(x, y, d, n, rim, (u, v));
            // A groove round the bezel.
            c = scale(c, 1.0 - 0.45 * (-((dop - ring - 0.8) / 1.2).powi(2)).exp());
            if let Some(s) = strip {
                c = controls(c, x, y, s);
            }
            c
        };
        Some((c, cover))
    });
    canvas.done(game)
}

/// The studio monitor's control strip: a recessed dark panel with five knobs and three push buttons.
fn controls(c: Rgb, x: f32, y: f32, s: Box4) -> Rgb {
    let sh = s.3 - s.1;
    let ds = sd_rr4(x, y, s, [sh * 0.18; 4]);
    if ds > sh * 0.5 {
        return c;
    }
    let mut c = c;
    // The recess: a dark panel, its top wall in shadow, its lower edge lit.
    if ds < 0.5 {
        let k = (0.5 - ds).clamp(0.0, 1.0);
        let top = (-((y - s.1) / 2.5).powi(2)).exp();
        let panel = scale([0.15, 0.15, 0.155], 1.0 - 0.4 * top);
        c = mix(c, panel, k);
    }
    c = add(
        c,
        [0.06 * (-((ds - 0.8) / 0.8).powi(2)).exp() * if y > (s.1 + s.3) / 2.0 { 1.0 } else { -0.5 }; 3],
    );
    let kr = (sh * 0.3).clamp(3.0, 12.0);
    let cy = (s.1 + s.3) / 2.0;
    let w = s.2 - s.0;
    // Three small push buttons on the left, five knobs to the right.
    for i in 0..3 {
        let bx = s.0 + sh * 0.55 + i as f32 * kr * 2.4;
        let bb = (bx - kr * 0.7, cy - kr * 0.55, bx + kr * 0.7, cy + kr * 0.55);
        let db = sd_rr4(x, y, bb, [kr * 0.18; 4]);
        let k = (0.5 - db).clamp(0.0, 1.0);
        if k > 0.0 || db < 2.0 {
            let shade = if y < cy { 0.62 } else { 0.50 };
            c = scale(c, 1.0 - 0.4 * (-((db - 1.0) / 1.0).powi(2)).exp() * if y > cy { 1.0 } else { 0.0 });
            c = mix(c, [shade, shade, shade * 0.98], k);
        }
    }
    let knobs_x0 = s.0 + sh * 0.55 + 3.0 * kr * 2.4 + kr * 1.2;
    let step = ((s.2 - sh * 0.5 - kr - knobs_x0) / 4.0).max(kr * 2.4);
    for i in 0..5 {
        let kx = knobs_x0 + i as f32 * step;
        if kx + kr > s.0 + w - 1.0 {
            break;
        }
        let angle = -2.2 + i as f32 * 0.55;
        c = knob(c, x, y, (kx, cy), kr, angle, [0.30, 0.30, 0.31]);
    }
    c
}

// --- Portable TV ------------------------------------------------------------------------------------

/// A small portable TV: a charcoal body, a rounded tube, a brushed panel on the right with a tuning
/// dial and speaker slots (or both below when the sides are narrow), and a telescopic antenna rising
/// from the top right corner, all within the band above the picture.
pub(crate) fn portable(view_w: u32, view_h: u32, game: Rect, px: f32, b: [f32; 4]) -> Bezel {
    let mut canvas = Canvas::new(view_w, view_h);
    let pic = hole(game);
    let plate = grow(pic, b);
    let h = game.h as f32;
    let ant_room = if b[2] >= 44.0 { b[2] * 0.36 } else { 0.0 };
    let body_box = (plate.0, plate.1 + ant_room, plate.2, plate.3);
    let bb = [b[0], b[1], b[2] - ant_room, b[3]];
    let thin = thinnest(bb);
    let g = (thin * 0.16).clamp(2.0, 16.0);
    let opening = grow(pic, [g; 4]);
    let open_r = fit([h * 0.05; 4], opening, pic, [0.0; 4], 0.8);
    let ring = (thin * 0.24).clamp(2.5, 24.0);
    let outer_ring = grow(opening, [ring; 4]);
    let ring_r = open_r.map(|r| r + ring * 0.7);
    let body_r = fit([h * 0.045; 4], body_box, outer_ring, ring_r, 1.0);
    let mat = Material::new([0.125, 0.125, 0.135], Surface::Matte, 0.20);
    let panel_mat = Material::new([0.62, 0.63, 0.65], Surface::Brushed, 0.16);
    let rim = (thin * 0.42).clamp(3.0, 14.0);
    let side = body_box.2 - outer_ring.2;
    let below = body_box.3 - outer_ring.3;
    // The control panel and what is on it: the dial (centre, radius) and the speaker slots (box).
    let pad = |room: f32| (room * 0.12).clamp(3.0, 12.0);
    let (panel, dial, slots) = if side >= 64.0 {
        let p = pad(side);
        let panel = (
            outer_ring.2 + p * 0.6,
            body_box.1 + p + rim * 0.5,
            body_box.2 - p - rim * 0.5,
            body_box.3 - p - rim * 0.5,
        );
        let pw = panel.2 - panel.0;
        let dr = pw * 0.36;
        let dial = ((panel.0 + panel.2) / 2.0, panel.1 + pw * 0.14 + dr, dr);
        let top = dial.1 + dr + pw * 0.22;
        let slots = (panel.0 + pw * 0.2, top, panel.2 - pw * 0.2, (top + (panel.3 - top) * 0.55).max(top + 10.0));
        (Some(panel), Some(dial), Some(slots))
    } else if below >= 46.0 {
        let p = pad(below);
        let dr = (below - 2.0 * p) * 0.36;
        let dial = (pic.2 - dr * 1.6, (outer_ring.3 + body_box.3) / 2.0, dr);
        let slots = (pic.0 + game.w as f32 * 0.04, dial.1 - dr * 0.7, pic.0 + game.w as f32 * 0.3, dial.1 + dr * 0.7);
        (None, Some(dial), Some(slots))
    } else {
        (None, None, None)
    };
    let px_ = px.max(1.0);
    let body = move |x: f32, y: f32| sd_rr4(x, y, body_box, body_r);
    canvas.paint(body_box, game, |x, y| {
        let d = body(x, y);
        let cover = (0.5 - d).clamp(0.0, 1.0);
        if cover <= 0.0 {
            return None;
        }
        let dop = sd_rr4(x, y, opening, open_r);
        let c = if dop < 0.0 {
            tube(x, y, pic, opening, dop, px_, 0.035)
        } else if dop < ring {
            let n = normal(|x, y| sd_rr4(x, y, opening, open_r), x, y);
            let c = inner_bezel([0.07, 0.07, 0.075], n, dop / ring);
            add(
                mix(c, [0.02; 3], smoothstep(1.0, 0.0, dop)),
                [0.045 * (-((dop - ring + 1.0) / 0.9).powi(2)).exp(); 3],
            )
        } else {
            let n = if -d < rim * 1.2 + 3.0 { normal(body, x, y) } else { (0.0, 0.0) };
            let (u, v) = ((x - body_box.0) / (body_box.2 - body_box.0), (y - body_box.1) / (body_box.3 - body_box.1));
            let mut c = mat.shade(x, y, d, n, rim, (u, v));
            c = scale(c, 1.0 - 0.4 * (-((dop - ring - 0.8) / 1.2).powi(2)).exp());
            if let Some(p) = panel {
                let pr = ((p.2 - p.0) * 0.08).min(10.0);
                let dp = sd_rr4(x, y, p, [pr; 4]);
                if dp < 0.5 {
                    let pn = if -dp < 4.0 {
                        normal(|x, y| sd_rr4(x, y, p, [pr; 4]), x, y)
                    } else {
                        (0.0, 0.0)
                    };
                    let pc = panel_mat.shade(x, y, dp, pn, 2.5, ((x - p.0) / (p.2 - p.0), (y - p.1) / (p.3 - p.1)));
                    c = mix(c, pc, (0.5 - dp).clamp(0.0, 1.0));
                }
                c = scale(c, 1.0 - 0.35 * (-((dp - 1.0) / 1.2).powi(2)).exp() * if y > p.1 + 2.0 { 0.6 } else { 0.0 });
            }
            if let Some(s) = slots {
                let sh = s.3 - s.1;
                // Fine slots, as many as fit.
                let pitch = ((s.2 - s.0) * 0.09).clamp(4.0, 8.0);
                let n = (sh / pitch).floor().max(1.0);
                let t = pitch * 0.42;
                if x > s.0 - t && x < s.2 + t && y > s.1 - t && y < s.3 + t {
                    let row = ((y - s.1) / pitch - 0.5).round().clamp(0.0, n - 1.0);
                    let ry = s.1 + (row + 0.5) * pitch;
                    let dx = (s.0 + t * 0.5 - x).max(x - s.2 + t * 0.5).max(0.0);
                    let dd = (dx * dx + (y - ry).powi(2)).sqrt();
                    let k = (t * 0.5 + 0.5 - dd).clamp(0.0, 1.0);
                    c = mix(c, [0.025; 3], k);
                    c = add(c, [0.05 * k * smoothstep(0.0, t * 0.5, y - ry); 3]);
                }
            }
            if let Some((dx, dy, dr)) = dial {
                c = tuning_dial(c, x, y, (dx, dy), dr);
            }
            c
        };
        Some((c, cover))
    });
    // The antenna: a swivel on the body's top, three chrome sections getting thinner, collars at the
    // joints and a ball at the tip. It lies low, so the whole of it stays inside the band.
    if ant_room > 0.0 {
        let sx = body_box.2 - body_r[1].max(rim * 2.0) - ant_room * 0.4;
        let sy = body_box.1 + 1.0;
        let t0 = (ant_room * 0.12).clamp(2.5, 7.0);
        let tip_r = t0 * 0.95;
        let rise = ant_room - tip_r - 4.0;
        let len = (game.w as f32 * 0.62).min(sx - plate.0 - 10.0);
        let (ex, ey) = (sx - (len * len - rise * rise).max(1.0).sqrt(), sy - rise);
        let joints = [0.0, 0.42, 0.74, 1.0];
        let widths = [t0, t0 * 0.78, t0 * 0.6];
        let region = (ex - tip_r - 2.0, ey - tip_r - 2.0, sx + ant_room * 0.5, sy + 2.0);
        let (ux, uy) = ((ex - sx) / len, (ey - sy) / len);
        canvas.paint(region, game, |x, y| {
            let (px, py) = (x - sx, y - sy);
            let along = (px * ux + py * uy) / len;
            let across = px * -uy + py * ux;
            let mut best: Option<(Rgb, f32)> = None;
            // The rod.
            if (-0.02..=1.0).contains(&along) {
                let k = joints.iter().rposition(|&j| along >= j).unwrap_or(0).min(2);
                let half = widths[k] / 2.0;
                let s = across / half;
                let cover = (half + 0.5 - across.abs()).clamp(0.0, 1.0);
                if cover > 0.0 {
                    // Chrome: a bright line along it, dark edges, the sky above it lighter.
                    let s = s.clamp(-1.0, 1.0);
                    let v = 0.30 + 0.55 * (1.0 - (s + 0.35).abs()).max(0.0).powf(1.5) + 0.35 * (-((s + 0.45) / 0.18).powi(2)).exp();
                    let mut c = [v * 0.92, v * 0.94, v];
                    // Collars at the joints.
                    for &j in &joints[1..3] {
                        let dj = (along - j).abs() * len;
                        if dj < 2.2 {
                            c = scale(c, 1.15 - 0.3 * (dj / 2.2));
                        }
                    }
                    best = Some((c, cover));
                }
            }
            // The ball at the tip.
            let dt = ((x - ex).powi(2) + (y - ey).powi(2)).sqrt();
            if dt < tip_r + 0.5 {
                let cover = (tip_r + 0.5 - dt).clamp(0.0, 1.0);
                let (nx, ny) = ((x - ex) / tip_r, (y - ey) / tip_r);
                let nz = (1.0 - (nx * nx + ny * ny).min(1.0)).sqrt();
                let spec = (nx * -0.4 + ny * -0.6 + nz * 0.7).max(0.0).powf(12.0);
                let c = add(scale([0.55, 0.56, 0.6], 0.5 + 0.5 * nz), [0.5 * spec; 3]);
                best = Some((c, cover));
            }
            // The swivel: a dark block with a round pivot.
            let ds = sd_rr4(x, y, (sx - t0 * 1.8, sy - t0 * 2.4, sx + t0 * 1.8, sy + 2.0), [t0 * 0.8; 4]);
            if ds < 0.5 && best.is_none() {
                let c = scale(
                    [0.10, 0.10, 0.11],
                    1.0 + 0.6 * (-((ds + 1.0) / 0.8).powi(2)).exp() * if y < sy - t0 { 1.0 } else { 0.2 },
                );
                best = Some((c, (0.5 - ds).clamp(0.0, 1.0)));
            }
            best
        });
    }
    canvas.done(game)
}

/// The tuning dial: a knurled ring, a brushed face with a window of tick marks and a pointer.
fn tuning_dial(c: Rgb, x: f32, y: f32, (cx, cy): (f32, f32), r: f32) -> Rgb {
    let (dx, dy) = (x - cx, y - cy);
    let d = (dx * dx + dy * dy).sqrt();
    let shadow = ((dx - r * 0.1).powi(2) + (dy - r * 0.16).powi(2)).sqrt();
    let mut c = scale(c, 1.0 - 0.5 * (1.0 - smoothstep(r * 0.95, r * 1.25, shadow)));
    let cover = 1.0 - smoothstep(r - 0.7, r + 0.5, d);
    if cover <= 0.0 {
        return c;
    }
    let phi = dy.atan2(dx);
    let (nx, ny) = (dx / r, dy / r);
    let k = if d > r * 0.8 {
        // Knurled ring: fine ridges all round, rounded over.
        let ridge = 0.5 + 0.5 * (phi * 48.0).cos();
        let nz = (1.0 - ((d - r * 0.8) / (r * 0.2)).clamp(0.0, 1.0).powi(2)).sqrt();
        let lit = ((nx * LIGHT[0] + ny * LIGHT[1]) * (1.0 - nz) / LIGHT[2] + nz).clamp(0.3, 1.5);
        scale([0.16, 0.16, 0.17], lit * (0.75 + 0.35 * ridge))
    } else {
        // The face: brushed in circles, a window of ticks along the top, a pointer.
        let swirl = 0.06 * (vnoise(d * 1.3, phi * 2.0) - 0.5);
        let mut f = scale([0.70, 0.70, 0.72], 1.0 + swirl - 0.12 * (dy / r));
        let in_window = d > r * 0.42 && d < r * 0.7 && phi < -0.35 && phi > -2.8;
        if in_window {
            f = [0.08, 0.08, 0.085];
            let tick = (phi * 9.0).rem_euclid(std::f32::consts::PI / 2.0);
            let major = (phi * 9.0 / (std::f32::consts::PI / 2.0)).round() as i32 % 2 == 0;
            if tick < 0.35 && d < r * if major { 0.66 } else { 0.56 } {
                f = [0.78, 0.74, 0.62];
            }
        }
        // The pointer, at a channel somewhere in the middle.
        let ang = -1.35f32;
        let (pxv, pyv) = (ang.cos(), ang.sin());
        let along = dx * pxv + dy * pyv;
        let across = (dx * pyv - dy * pxv).abs();
        if along > r * 0.3 && along < r * 0.74 && across < 1.2 {
            f = mix(f, [0.85, 0.22, 0.12], 1.0 - smoothstep(0.4, 1.2, across));
        }
        add(
            f,
            [0.10 * (1.0 - smoothstep(0.0, r * 0.5, ((dx + r * 0.3).powi(2) + (dy + r * 0.35).powi(2)).sqrt())); 3],
        )
    };
    c = mix(c, k, cover);
    c
}

// --- Woodgrain --------------------------------------------------------------------------------------

/// A 70s faux-wood front: walnut veneer at the sides and below, a ribbed black band along the top
/// with a fine bright trim under it, and a black gloss bezel round the picture.
pub(crate) fn woodgrain(view_w: u32, view_h: u32, game: Rect, px: f32, b: [f32; 4]) -> Bezel {
    let _ = px;
    let mut canvas = Canvas::new(view_w, view_h);
    let pic = hole(game);
    let plate = grow(pic, b);
    let h = game.h as f32;
    let thin = thinnest(b);
    let g = (thin * 0.22).clamp(2.0, 20.0);
    let opening = grow(pic, [g; 4]);
    let open_r = fit([h * 0.01; 4], opening, pic, [0.0; 4], 0.6);
    let body_r = fit([h * 0.02; 4], plate, opening, open_r, 1.0).map(|r| r.min(corner_max(b[0], b[2], 2.0)));
    let top_band = b[2] >= 16.0;
    let ribs_end = plate.1 + (opening.1 - plate.1) * 0.62;
    let pitch = ((ribs_end - plate.1) / 7.0).clamp(3.0, 9.0);
    let rim = (thin * 0.35).clamp(2.5, 10.0);
    let black = Material::new([0.05, 0.05, 0.055], Surface::Matte, 0.2);
    canvas.paint(plate, game, |x, y| {
        let body = |x: f32, y: f32| sd_rr4(x, y, plate, body_r);
        let d = body(x, y);
        let cover = (0.5 - d).clamp(0.0, 1.0);
        if cover <= 0.0 {
            return None;
        }
        let n = if -d < rim * 1.2 + 3.0 { normal(body, x, y) } else { (0.0, 0.0) };
        let slope = (1.0 - (-d / rim).clamp(0.0, 1.0)).powf(1.6);
        let (u, v) = ((x - plate.0) / (plate.2 - plate.0), (y - plate.1) / (plate.3 - plate.1));
        let dop = sd_rr4(x, y, opening, open_r);
        let c = if dop < 0.5 {
            // Black gloss bezel round the picture, sloping in: lit along the bottom.
            let t = (-dop / g).clamp(0.0, 1.0);
            let below = if y > pic.3 {
                1.0
            } else if y < pic.1 {
                0.0
            } else {
                0.4
            };
            let c = [0.02 + 0.05 * below * (1.0 - t); 3];
            add(c, [0.06 * (-((dop + 1.0) / 0.8).powi(2)).exp(); 3])
        } else if top_band && y < ribs_end {
            // Ribs running across: each a rounded ridge lit on top.
            let phase = ((y - plate.1) / pitch).fract();
            let ny = -((phase - 0.5) * 2.0);
            let lit = ((ny * LIGHT[1] * 1.3 + LIGHT[2]) / LIGHT[2]).clamp(0.35, 2.2);
            let mut c = black.shade(x, y, d, n, rim, (u, v));
            c = scale(c, lit);
            c = scale(
                c,
                1.0 - 0.55 * (-((phase - 0.0) / 0.12).powi(2)).exp() - 0.55 * (-((phase - 1.0) / 0.12).powi(2)).exp(),
            );
            c
        } else {
            let mut c = wood(x, y, v);
            c = scale(c, lambert(n.0, n.1, slope).clamp(0.4, 1.5));
            c = add(c, [0.05 * specular(n.0, n.1, slope, 20.0) * slope; 3]);
            // A groove where the veneer meets the bezel.
            c = scale(c, 1.0 - 0.6 * (-((dop - 1.2) / 1.1).powi(2)).exp());
            if top_band {
                // The bright trim between the ribbed band and the wood.
                let dt = y - ribs_end;
                let trim = (-((dt - 1.2) / 1.0).powi(2)).exp();
                c = mix(c, [0.78, 0.76, 0.72], trim * 0.85);
                c = scale(c, 1.0 - 0.35 * (-((dt - 3.0) / 1.2).powi(2)).exp());
            }
            c
        };
        Some((c, cover))
    });
    canvas.done(game)
}
