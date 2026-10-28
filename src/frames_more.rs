//! More built-in screen frames, after the shape language of classic handhelds: Brick, Pocket slim,
//! Clamshell, Wide, Twin, Gloss, Swan, Watch and Goggles. Like those in `frames.rs` they are drawn in
//! code at the exact output size, stay inside the bands `bezel::bands` gives them (a strip of true
//! black remains at the panel edge) and never draw where the picture goes. Original designs: generic
//! bodies, glass, lines, grilles and hinges; no logos, lettering, buttons or exact outlines.
//!
//! Most of them are a *body* (a material: matte, clear, brushed, satin or piano plastic) with a
//! *glass* set into it around the picture, and the picture in a shallow well in the glass
//! (`Handheld`). Six have a power light (`frame_led`), which is painted live over the cached frame.

use crate::bezel::{bands, classic, noise, sd_round_rect, smoothstep, Bezel, BrickColour, ClamFinish, SlimFinish, Style, SwanFinish, WatchFinish};
use crate::frame_led::Spot;
use crate::frames::{add, grow, hole, lambert, mix, normal, scale, specular, vnoise, Box4, Canvas, Rgb, LIGHT};
use crate::video::Rect;

// --- Geometry ---------------------------------------------------------------------------------------

/// Signed distance to a rounded rectangle with its own radius per corner: top left, top right,
/// bottom right, bottom left.
pub(crate) fn sd_rr4(x: f32, y: f32, b: Box4, r: [f32; 4]) -> f32 {
    let (cx, cy) = ((b.0 + b.2) * 0.5, (b.1 + b.3) * 0.5);
    let k = match (x < cx, y < cy) {
        (true, true) => r[0],
        (false, true) => r[1],
        (false, false) => r[2],
        (true, false) => r[3],
    };
    sd_round_rect(x, y, b, k.min((b.2 - b.0) * 0.5).min((b.3 - b.1) * 0.5).max(0.0))
}

/// The largest corner radius of an outline whose corner is `a` px left of and `b` px above the
/// corner of something it must contain, with at least `m` px to spare.
pub(crate) fn corner_max(a: f32, b: f32, m: f32) -> f32 {
    if a <= m || b <= m {
        return (a.max(b) - m).max(0.0);
    }
    (a + b - m) + (2.0 * (a - m) * (b - m)).sqrt()
}

/// Corner radii for `outer` around `inner` (whose corners have radii `inner_r`): as close to `want`
/// as they can be while `inner` stays inside with `m` px to spare.
pub(crate) fn fit(want: [f32; 4], outer: Box4, inner: Box4, inner_r: [f32; 4], m: f32) -> [f32; 4] {
    let (l, t, r, b) = (inner.0 - outer.0, inner.1 - outer.1, outer.2 - inner.2, outer.3 - inner.3);
    let ab = [(l, t), (r, t), (r, b), (l, b)];
    std::array::from_fn(|i| {
        let (a, bb) = ab[i];
        want[i].min(corner_max(a, bb, m).max(inner_r[i] + a.min(bb) - m)).max(0.0)
    })
}

fn bump(x: f32, a: f32, peak: f32, b: f32) -> f32 {
    smoothstep(a, peak, x) * (1.0 - smoothstep(peak, b, x))
}

/// Distance from (x, y) to the picture rectangle, 0 inside.
fn to_pic(x: f32, y: f32, pic: Box4) -> f32 {
    let dx = (pic.0 - x).max(x - pic.2).max(0.0);
    let dy = (pic.1 - y).max(y - pic.3).max(0.0);
    (dx * dx + dy * dy).sqrt()
}

/// Distance to a segment from a to b, and how far along it (0..1).
fn sd_segment(x: f32, y: f32, (ax, ay): (f32, f32), (bx, by): (f32, f32)) -> (f32, f32) {
    let (px, py, dx, dy) = (x - ax, y - ay, bx - ax, by - ay);
    let t = ((px * dx + py * dy) / (dx * dx + dy * dy).max(1e-6)).clamp(0.0, 1.0);
    (((px - dx * t).powi(2) + (py - dy * t).powi(2)).sqrt(), t)
}

/// The size of a power light for a picture `px` output pixels per game pixel.
fn led_radius(px: f32) -> f32 {
    (px * 0.75).clamp(3.2, 5.5)
}

// --- Materials --------------------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum Surface {
    /// Moulded plastic with a faint texture and a parting seam.
    Matte,
    /// See-through plastic: darker insides behind an inner wall, milky rims.
    Clear,
    /// Brushed metal: fine streaks along x and a broad band of light.
    Brushed,
    /// Metallic paint: smooth, a broad highlight, a fine flake.
    Satin,
    /// Piano lacquer: near black, mirror-like, the room's light as gradients and streaks.
    Piano,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Material {
    pub base: Rgb,
    pub surface: Surface,
    pub gloss: f32,
}

impl Material {
    pub(crate) const fn new(base: Rgb, surface: Surface, gloss: f32) -> Self {
        Material { base, surface, gloss }
    }

    /// The body at (x, y): `d` px from its outline (negative inside), (nx, ny) the outline's outward
    /// normal there (only needed within the rim), a rounded edge `rim` px wide, (u, v) the position
    /// in the body's box (0..1).
    pub(crate) fn shade(&self, x: f32, y: f32, d: f32, (nx, ny): (f32, f32), rim: f32, (u, v): (f32, f32)) -> Rgb {
        let slope = (1.0 - (-d / rim).clamp(0.0, 1.0)).powf(1.7);
        let diffuse = lambert(nx, ny, slope).clamp(0.35, 1.45);
        let mut c = scale(self.base, (1.04 - 0.13 * v) * diffuse);
        let sheen = bump(u + 0.7 * v, 0.05, 0.35, 0.75);
        let seam = -d - rim * 1.1;
        let seam_line = |c: Rgb| {
            let c = scale(c, 1.0 - 0.22 * (-(seam / 0.7).powi(2)).exp());
            scale(c, 1.0 + 0.10 * (-((seam - 1.3) / 0.7).powi(2)).exp() * (-ny).max(0.3))
        };
        match self.surface {
            Surface::Matte => {
                c = add(c, [self.gloss * 0.12 * sheen; 3]);
                c = scale(c, 1.0 + 0.025 * (vnoise(x * 0.9, y * 0.9) - 0.5));
                c = seam_line(c);
            }
            Surface::Clear => {
                c = add(c, [self.gloss * 0.22 * sheen; 3]);
                c = mix(c, scale(self.base, 1.45), 0.30 * slope);
            }
            Surface::Brushed => {
                // Streaks along x: long ones that change from line to line, and shorter ones.
                let s = 0.10 * (vnoise(x * 0.004, y * 1.9) - 0.5) + 0.06 * (vnoise(x * 0.03, y * 0.7) - 0.5);
                c = scale(c, 1.0 + s);
                // Brushed metal throws the light as a band across the brushing.
                c = add(
                    c,
                    [0.07 * (-((v - 0.28) / 0.13).powi(2)).exp() + 0.03 * (-((v - 0.78) / 0.09).powi(2)).exp(); 3],
                );
            }
            Surface::Satin => {
                c = add(c, [self.gloss * 0.16 * sheen + 0.045 * (-((v - 0.12) / 0.2).powi(2)).exp(); 3]);
                c = scale(c, 1.0 + 0.05 * noise(x as u32 ^ 0x55, y as u32));
                c = seam_line(c);
            }
            Surface::Piano => {
                // The room's light as a long gradient, one broad diagonal streak and a thin one.
                c = add(c, [0.032 * (1.0 - v).powi(2); 3]);
                let s = u * 0.55 + v;
                c = add(c, [0.042 * bump(s, 0.16, 0.30, 0.48) + 0.020 * bump(s, 0.58, 0.62, 0.68); 3]);
            }
        }
        add(
            c,
            [self.gloss * 1.4 * specular(nx, ny, slope, if self.surface == Surface::Piano { 60.0 } else { 24.0 }) * slope; 3],
        )
    }
}

/// Inside a see-through body: darker insides beyond the inner wall `dw` px away (negative inside it),
/// the wall itself, and the light it lets through.
fn clear_insides(c: Rgb, base: Rgb, dw: f32) -> Rgb {
    let inside = smoothstep(1.5, -1.5, dw);
    let c = scale(c, 1.0 - 0.36 * inside);
    let c = scale(c, 1.0 - 0.32 * (-(dw / 1.6).powi(2)).exp());
    add(c, scale(base, 0.10 * (-(dw - 2.5).powi(2) / 4.0).exp()))
}

/// A round screw post seen through clear plastic.
fn boss(c: Rgb, base: Rgb, x: f32, y: f32, (bx, by): (f32, f32), r: f32) -> Rgb {
    let d = ((x - bx).powi(2) + (y - by).powi(2)).sqrt();
    let ring = (-((d - r) / 1.3).powi(2)).exp();
    let head = 1.0 - smoothstep(r * 0.45, r * 0.62, d);
    let c = scale(c, 1.0 - 0.38 * ring);
    let c = mix(c, scale(base, 0.55), 0.6 * head);
    add(
        c,
        [0.05 * head * (1.0 - smoothstep(0.0, r * 0.4, ((x - bx + 1.0).powi(2) + (y - by + 1.0).powi(2)).sqrt())); 3],
    )
}

/// Holes in the plastic (grille, slits): the dark inside, its lower wall catching the light.
fn holed(c: Rgb, base: Rgb, cover: f32, lit: f32) -> Rgb {
    let c = mix(c, add(scale(base, 0.16), [0.010; 3]), cover);
    add(c, scale(base, 0.22 * lit * cover))
}

// --- Handheld bodies --------------------------------------------------------------------------------

/// A body with a glass around the picture, the picture set into the glass.
struct Handheld<'a> {
    pic: Box4,
    px: f32,
    /// The body's box (for gradients) and its outline as a signed distance.
    plate: Box4,
    body: &'a (dyn Fn(f32, f32) -> f32 + Sync),
    rim: f32,
    mat: Material,
    glass: Box4,
    glass_r: [f32; 4],
    glass_tone: Rgb,
    glass_gloss: f32,
    /// See-through bodies: the inner wall (box, radius) and screw posts.
    wall: Option<(Box4, f32)>,
    bosses: Vec<(f32, f32)>,
    boss_r: f32,
}

/// Where a decoration is drawn.
#[derive(Clone, Copy, PartialEq)]
enum On {
    Body,
    Glass,
}

impl Handheld<'_> {
    /// The glass: a smooth dark pane, lighter at the top, a soft reflection across it, a fine line
    /// where it meets the body, and the well the picture sits in.
    fn glass_at(&self, x: f32, y: f32, dg: f32) -> Rgb {
        let g = self.glass;
        let (u, v) = ((x - g.0) / (g.2 - g.0), (y - g.1) / (g.3 - g.1));
        let mut c = scale(self.glass_tone, 1.07 - 0.14 * v);
        let s = u * 0.7 + v;
        c = add(
            c,
            [self.glass_gloss * (0.030 * bump(s, 0.12, 0.30, 0.52) + 0.012 * bump(s, 0.62, 0.66, 0.72)); 3],
        );
        c = scale(c, 1.0 - 0.40 * (-((dg + 0.9) / 0.8).powi(2)).exp());
        // The well: dark walls, the lower one lit, and a fine lip around it.
        let dh = to_pic(x, y, self.pic);
        let recess = (self.px * 0.7).clamp(2.0, 6.0);
        let below = if y > self.pic.3 {
            1.0
        } else if y < self.pic.1 {
            0.0
        } else {
            0.5
        };
        if dh < recess {
            let wall = 0.010 + 0.034 * below;
            c = [wall, wall, wall * 1.08];
        } else if dh < recess + 1.5 {
            c = add(c, [0.035 * below - 0.012 * (1.0 - below) + 0.01; 3]);
        }
        c
    }

    fn body_at(&self, x: f32, y: f32, d: f32, dg: f32) -> Rgb {
        let p = self.plate;
        let n = if -d < self.rim * 1.2 + 3.0 { normal(self.body, x, y) } else { (0.0, 0.0) };
        let (u, v) = (((x - p.0) / (p.2 - p.0)).clamp(0.0, 1.0), ((y - p.1) / (p.3 - p.1)).clamp(0.0, 1.0));
        let mut c = self.mat.shade(x, y, d, n, self.rim, (u, v));
        // The glass sits a little below the body: a soft contact shadow round it, a lit lip under it.
        c = scale(c, 1.0 - 0.38 * (-dg.max(0.0) / (1.6 + self.rim * 0.16)).exp());
        if y > self.glass.3 - 8.0 && dg < 2.5 {
            c = add(c, [0.045 * (1.0 - smoothstep(1.0, 2.5, dg)); 3]);
        }
        if let Some((wall, rw)) = self.wall {
            c = clear_insides(c, self.mat.base, sd_round_rect(x, y, wall, rw));
            // Through clear plastic the screen's own frame shows round the glass, darker.
            let module = (self.rim * 0.8).clamp(2.0, 10.0);
            c = scale(c, 1.0 - 0.16 * smoothstep(1.0, -1.0, dg - module));
            for &b in &self.bosses {
                c = boss(c, self.mat.base, x, y, b, self.boss_r);
            }
        }
        c
    }

    /// Paints the body and glass, with `decor` drawn over them (lines, grilles, slits).
    fn paint(&self, canvas: &mut Canvas, game: Rect, decor: impl Fn(f32, f32, Rgb, On) -> Rgb + Sync) {
        self.paint_in(canvas, game, None, decor);
    }

    /// `paint`, or with `over` the same over that part of the picture (an inlay).
    fn paint_in(&self, canvas: &mut Canvas, game: Rect, over: Option<Box4>, decor: impl Fn(f32, f32, Rgb, On) -> Rgb + Sync) {
        let shade = |x: f32, y: f32| {
            let d = (self.body)(x, y);
            let cover = (0.5 - d).clamp(0.0, 1.0);
            if cover <= 0.0 {
                return None;
            }
            let dg = sd_rr4(x, y, self.glass, self.glass_r);
            let in_glass = (0.5 - dg).clamp(0.0, 1.0);
            let c = if in_glass >= 1.0 {
                decor(x, y, self.glass_at(x, y, dg), On::Glass)
            } else if in_glass <= 0.0 {
                decor(x, y, self.body_at(x, y, d, dg), On::Body)
            } else {
                mix(self.body_at(x, y, d, dg), self.glass_at(x, y, dg), in_glass)
            };
            Some((c, cover))
        };
        match over {
            Some(r) => canvas.paint_over(r, shade),
            None => canvas.paint(self.plate, game, shade),
        }
    }
}

/// Screw posts in the corners of a see-through body's inner wall, where the corner has room.
fn bosses_for(wall: Box4, rw: f32, pic: Box4, b: [f32; 4], boss_r: f32, glass: Box4) -> Vec<(f32, f32)> {
    let corner_bands = [(b[0], b[2]), (b[1], b[2]), (b[0], b[3]), (b[1], b[3])];
    [(wall.0, wall.1), (wall.2, wall.1), (wall.0, wall.3), (wall.2, wall.3)]
        .iter()
        .zip(corner_bands)
        .filter(|(_, (a, c))| a.min(*c) >= boss_r * 5.0)
        .map(|((x, y), _)| {
            let off = rw * 0.3 + boss_r * 1.6;
            (if *x < pic.0 { x + off } else { x - off }, if *y < pic.1 { y + off } else { y - off })
        })
        // Not under the glass.
        .filter(|&(x, y)| sd_round_rect(x, y, glass, 0.0) > boss_r * 1.5)
        .collect()
}

/// The inner wall of a see-through body `plate` with corner radius `rb`.
fn wall_for(plate: Box4, rb: f32, b: [f32; 4]) -> (Box4, f32) {
    let thin = b.iter().cloned().fold(f32::MAX, f32::min);
    let inset = (thin * 0.3).clamp(3.0, 22.0);
    ((plate.0 + inset, plate.1 + inset, plate.2 - inset, plate.3 - inset), (rb - inset).max(2.0))
}

/// The light at `spot`, if the body has room for it there: inside the body and clear of the picture.
fn led_if(spot: Spot, body: &dyn Fn(f32, f32) -> f32, pic: Box4) -> Option<Spot> {
    (body(spot.x, spot.y) < -spot.r * 2.2 && to_pic(spot.x, spot.y, pic) > spot.r * 1.6).then_some(spot)
}

fn finish(canvas: Canvas, game: Rect, led: Option<Spot>) -> Bezel {
    let b = canvas.done(game);
    match led {
        Some(spot) => b.with_led(spot),
        None => b,
    }
}

// --- Brick ------------------------------------------------------------------------------------------

/// A brick-shaped handheld: a body in a colourway, darker screen glass with one large rounded corner
/// bottom right, two thin lines (deep purple, navy) across the glass above the picture and the power
/// light left of the picture.
pub(crate) fn brick(colour: BrickColour, view_w: u32, view_h: u32, game: Rect, px: f32, b: [f32; 4]) -> Bezel {
    let mut canvas = Canvas::new(view_w, view_h);
    let pic = hole(game);
    let plate = grow(pic, b);
    let h = game.h as f32;
    let gb = [b[0] * 0.52, b[1] * 0.52, b[2] * 0.6, b[3] * 0.46];
    let glass = grow(pic, gb);
    let small = h * 0.016;
    let big = (h * 0.15).min(corner_max(gb[1], gb[3], (gb[1].min(gb[3]) * 0.3).max(2.0)));
    let glass_r = fit([small, small, big, small], glass, pic, [0.0; 4], 1.5);
    let body_r = fit([h * 0.035, h * 0.035, h * 0.13, h * 0.035], plate, glass, glass_r, 1.5);
    let mat = match colour {
        BrickColour::Grey => Material::new([0.70, 0.69, 0.66], Surface::Matte, 0.10),
        BrickColour::Clear => Material::new([0.72, 0.75, 0.78], Surface::Clear, 0.34),
        BrickColour::Red => Material::new([0.70, 0.12, 0.13], Surface::Matte, 0.14),
        BrickColour::Yellow => Material::new([0.86, 0.70, 0.22], Surface::Matte, 0.12),
        BrickColour::Green => Material::new([0.22, 0.50, 0.30], Surface::Matte, 0.14),
        BrickColour::Black => Material::new([0.07, 0.07, 0.075], Surface::Matte, 0.22),
    };
    let thin = b.iter().cloned().fold(f32::MAX, f32::min);
    let body = move |x: f32, y: f32| sd_rr4(x, y, plate, body_r);
    let (wall, bosses, boss_r) = if mat.surface == Surface::Clear {
        let wall = wall_for(plate, body_r[0], b);
        let boss_r = (thin * 0.11).clamp(3.0, 10.0);
        (Some(wall), bosses_for(wall.0, wall.1, pic, b, boss_r, glass), boss_r)
    } else {
        (None, Vec::new(), 0.0)
    };
    let hh = Handheld {
        pic,
        px,
        plate,
        body: &body,
        rim: (thin * 0.5).clamp(3.0, 16.0),
        mat,
        glass,
        glass_r,
        glass_tone: [0.215, 0.222, 0.255],
        glass_gloss: 1.0,
        wall,
        bosses,
        boss_r,
    };
    // Two printed lines across the top of the glass, where there is room for them.
    let t = (gb[2] * 0.06).clamp(1.5, 4.5);
    let lines = (gb[2] >= 12.0).then(|| {
        let yc = glass.1 + gb[2] * 0.38;
        let (x0, x1) = (glass.0 + gb[0] * 0.5, glass.2 - gb[1] * 0.5);
        (yc - t * 1.3, yc + t * 1.3, x0, x1)
    });
    let r = led_radius(px);
    let led = led_if(
        Spot {
            x: glass.0 + gb[0] * 0.5,
            y: pic.1 + h * 0.30,
            r,
        },
        &body,
        pic,
    )
    .filter(|_| gb[0] >= r * 3.2)
    .or_else(|| {
        led_if(
            Spot {
                x: pic.0 + game.w as f32 * 0.035,
                y: glass.1 + gb[2] * 0.78,
                r,
            },
            &body,
            pic,
        )
        .filter(|_| gb[2] >= r * 4.5)
    });
    hh.paint(&mut canvas, game, |x, y, c, on| {
        let Some((y_purple, y_navy, x0, x1)) = lines else { return c };
        if on != On::Glass {
            return c;
        }
        let mut c = c;
        for (yl, ink) in [(y_purple, [0.47, 0.13, 0.42]), (y_navy, [0.12, 0.17, 0.50])] {
            let (d, _) = sd_segment(x, y, (x0, yl), (x1, yl));
            let k = (t * 0.5 + 0.5 - d).clamp(0.0, 1.0);
            c = mix(c, ink, k * 0.95);
        }
        c
    });
    finish(canvas, game, led)
}

// --- Pocket slim ------------------------------------------------------------------------------------

/// A slimmer body in brushed metal (or clear plastic), with tighter glass and the light top left.
pub(crate) fn pocket(finish_: SlimFinish, view_w: u32, view_h: u32, game: Rect, px: f32, b: [f32; 4]) -> Bezel {
    let mut canvas = Canvas::new(view_w, view_h);
    let pic = hole(game);
    let plate = grow(pic, b);
    let h = game.h as f32;
    let gb = [b[0] * 0.42, b[1] * 0.42, b[2] * 0.46, b[3] * 0.40];
    let glass = grow(pic, gb);
    let glass_r = fit([h * 0.022; 4], glass, pic, [0.0; 4], 1.5);
    let body_r = fit([h * 0.05; 4], plate, glass, glass_r, 1.5);
    let mat = match finish_ {
        SlimFinish::Silver => Material::new([0.70, 0.71, 0.73], Surface::Brushed, 0.18),
        SlimFinish::Clear => Material::new([0.66, 0.70, 0.74], Surface::Clear, 0.34),
    };
    let thin = b.iter().cloned().fold(f32::MAX, f32::min);
    let body = move |x: f32, y: f32| sd_rr4(x, y, plate, body_r);
    let (wall, bosses, boss_r) = if mat.surface == Surface::Clear {
        let wall = wall_for(plate, body_r[0], b);
        let boss_r = (thin * 0.10).clamp(3.0, 9.0);
        (Some(wall), bosses_for(wall.0, wall.1, pic, b, boss_r, glass), boss_r)
    } else {
        (None, Vec::new(), 0.0)
    };
    let hh = Handheld {
        pic,
        px,
        plate,
        body: &body,
        rim: (thin * 0.42).clamp(3.0, 12.0),
        mat,
        glass,
        glass_r,
        glass_tone: [0.19, 0.20, 0.215],
        glass_gloss: 1.0,
        wall,
        bosses,
        boss_r,
    };
    let r = led_radius(px);
    let led = led_if(
        Spot {
            x: pic.0 + game.w as f32 * 0.05,
            y: glass.1 + gb[2] * 0.5,
            r,
        },
        &body,
        pic,
    )
    .filter(|_| gb[2] >= r * 3.4)
    .or_else(|| {
        led_if(
            Spot {
                x: glass.0 + gb[0] * 0.5,
                y: pic.1 + h * 0.2,
                r,
            },
            &body,
            pic,
        )
        .filter(|_| gb[0] >= r * 3.2)
    });
    hh.paint(&mut canvas, game, |_, _, c, _| c);
    finish(canvas, game, led)
}

// --- Swan -------------------------------------------------------------------------------------------

/// A minimal body in pastel or smoky see-through plastic, a plain glass and the light top right.
pub(crate) fn swan(finish_: SwanFinish, view_w: u32, view_h: u32, game: Rect, px: f32, b: [f32; 4]) -> Bezel {
    let mut canvas = Canvas::new(view_w, view_h);
    let pic = hole(game);
    let plate = grow(pic, b);
    let h = game.h as f32;
    let gb = [b[0] * 0.5, b[1] * 0.5, b[2] * 0.5, b[3] * 0.42];
    let glass = grow(pic, gb);
    let glass_r = fit([h * 0.012; 4], glass, pic, [0.0; 4], 1.5);
    let body_r = fit([h * 0.07; 4], plate, glass, glass_r, 1.5);
    let mat = match finish_ {
        SwanFinish::Pastel => Material::new([0.70, 0.84, 0.82], Surface::Matte, 0.10),
        SwanFinish::Skeleton => Material::new([0.42, 0.45, 0.50], Surface::Clear, 0.34),
    };
    let thin = b.iter().cloned().fold(f32::MAX, f32::min);
    let body = move |x: f32, y: f32| sd_rr4(x, y, plate, body_r);
    let (wall, bosses, boss_r) = if mat.surface == Surface::Clear {
        let wall = wall_for(plate, body_r[0], b);
        let boss_r = (thin * 0.10).clamp(3.0, 9.0);
        (Some(wall), bosses_for(wall.0, wall.1, pic, b, boss_r, glass), boss_r)
    } else {
        (None, Vec::new(), 0.0)
    };
    let hh = Handheld {
        pic,
        px,
        plate,
        body: &body,
        rim: (thin * 0.45).clamp(3.0, 14.0),
        mat,
        glass,
        glass_r,
        glass_tone: [0.255, 0.27, 0.285],
        glass_gloss: 0.8,
        wall,
        bosses,
        boss_r,
    };
    let r = led_radius(px);
    let led = led_if(
        Spot {
            x: pic.2 - game.w as f32 * 0.04,
            y: glass.1 + gb[2] * 0.5,
            r,
        },
        &body,
        pic,
    )
    .filter(|_| gb[2] >= r * 3.4)
    .or_else(|| {
        led_if(
            Spot {
                x: glass.2 - gb[1] * 0.5,
                y: pic.1 + h * 0.2,
                r,
            },
            &body,
            pic,
        )
        .filter(|_| gb[1] >= r * 3.2)
    });
    hh.paint(&mut canvas, game, |_, _, c, _| c);
    finish(canvas, game, led)
}

// --- Clamshell --------------------------------------------------------------------------------------

/// An opened clamshell's lid in metallic paint, black glass round the picture, and the hinge, a
/// cylinder in three parts, along the bottom.
pub(crate) fn clamshell(finish_: ClamFinish, view_w: u32, view_h: u32, game: Rect, px: f32, b: [f32; 4]) -> Bezel {
    let mut canvas = Canvas::new(view_w, view_h);
    let pic = hole(game);
    let plate = grow(pic, b);
    let h = game.h as f32;
    let hinge_h = if b[3] >= 22.0 { (b[3] * 0.42).min(h * 0.09).max(10.0) } else { 0.0 };
    let gap = if hinge_h > 0.0 { 2.0 } else { 0.0 };
    let lid = (plate.0, plate.1, plate.2, plate.3 - hinge_h - gap);
    let lb = [b[0], b[1], b[2], b[3] - hinge_h - gap];
    let gb = [lb[0] * 0.5, lb[1] * 0.5, lb[2] * 0.5, lb[3] * 0.55];
    let glass = grow(pic, gb);
    let glass_r = fit([h * 0.02; 4], glass, pic, [0.0; 4], 1.5);
    let lid_r = fit([h * 0.055, h * 0.055, h * 0.012, h * 0.012], lid, glass, glass_r, 1.5);
    let mat = match finish_ {
        ClamFinish::Platinum => Material::new([0.74, 0.745, 0.76], Surface::Satin, 0.20),
        ClamFinish::Cobalt => Material::new([0.15, 0.23, 0.52], Surface::Satin, 0.24),
        ClamFinish::FlameRed => Material::new([0.70, 0.11, 0.09], Surface::Satin, 0.24),
        ClamFinish::Graphite => Material::new([0.20, 0.205, 0.22], Surface::Satin, 0.24),
    };
    let thin = lb.iter().cloned().fold(f32::MAX, f32::min);
    let lid_sd = move |x: f32, y: f32| sd_rr4(x, y, lid, lid_r);
    let hh = Handheld {
        pic,
        px,
        plate: lid,
        body: &lid_sd,
        rim: (thin * 0.45).clamp(3.0, 14.0),
        mat,
        glass,
        glass_r,
        glass_tone: [0.05, 0.05, 0.056],
        glass_gloss: 1.3,
        wall: None,
        bosses: Vec::new(),
        boss_r: 0.0,
    };
    hh.paint(&mut canvas, game, |_, _, c, _| c);
    // The hinge: a capsule across the whole width, its ends and middle parted by two fine gaps.
    let (hy0, hy1) = (plate.3 - hinge_h, plate.3);
    let hc = (hy0 + hy1) / 2.0;
    let hr = hinge_h / 2.0;
    let w = plate.2 - plate.0;
    let cuts = [plate.0 + w * 0.15, plate.2 - w * 0.15];
    let r = led_radius(px).min(hinge_h * 0.22);
    let led = (hinge_h >= 14.0).then_some(Spot {
        x: cuts[0] + hinge_h * 0.9,
        y: hc,
        r,
    });
    let led = led.or_else(|| {
        led_if(
            Spot {
                x: glass.0 + gb[0] * 0.5,
                y: pic.1 + h * 0.25,
                r: led_radius(px),
            },
            &lid_sd,
            pic,
        )
        .filter(|_| gb[0] >= led_radius(px) * 3.2)
    });
    if hinge_h > 0.0 {
        let hinge_base = scale(mat.base, 0.78);
        canvas.paint((plate.0, hy0 - 1.0, plate.2, hy1), game, |x, y| {
            let (d, _) = sd_segment(x, y, (plate.0 + hr, hc), (plate.2 - hr, hc));
            let d = d - hr;
            let cover = (0.5 - d).clamp(0.0, 1.0);
            if cover <= 0.0 {
                return None;
            }
            // A cylinder lying along x: its normal turns from up to down across it.
            let ny = ((y - hc) / hr).clamp(-1.0, 1.0);
            let nz = (1.0 - ny * ny).max(0.0).sqrt();
            let lit = ((ny * LIGHT[1] + nz * LIGHT[2]) / LIGHT[2]).clamp(0.25, 1.5);
            let h_ = [LIGHT[0], LIGHT[1], LIGHT[2] + 1.0];
            let hl = (h_[0] * h_[0] + h_[1] * h_[1] + h_[2] * h_[2]).sqrt();
            let spec = ((ny * h_[1] + nz * h_[2]) / hl).max(0.0).powf(40.0);
            let part = if x < cuts[0] || x > cuts[1] { mat.base } else { hinge_base };
            let mut c = add(scale(part, lit.min(1.25) * (1.0 - 0.10 * ((x - plate.0) / w))), [mat.gloss * 0.45 * spec; 3]);
            for cx in cuts {
                c = scale(c, 1.0 - 0.75 * (-((x - cx) / 0.9).powi(2)).exp());
            }
            // Its ends round off.
            c = scale(c, 0.75 + 0.25 * smoothstep(-hr, -hr * 0.4, d));
            Some((c, cover))
        });
    }
    finish(canvas, game, led)
}

// --- Wide -------------------------------------------------------------------------------------------

/// A landscape body with rounded wings left and right, two shoulder parts peeking over its top
/// corners, black glass round the picture, a round dot grille low on the right and the light on
/// the left wing.
pub(crate) fn wide(view_w: u32, view_h: u32, game: Rect, px: f32, b: [f32; 4]) -> Bezel {
    let mut canvas = Canvas::new(view_w, view_h);
    let pic = hole(game);
    let plate = grow(pic, b);
    let h = game.h as f32;
    // The shoulders peek out round the top corners: the body leaves room for them there.
    let bump_h = if b[2] >= 30.0 && b[0].min(b[1]) >= 30.0 {
        (b[2] * 0.075).clamp(5.0, 16.0)
    } else {
        0.0
    };
    let main = (plate.0 + bump_h, plate.1 + bump_h, plate.2 - bump_h, plate.3);
    let gb = [b[0] * 0.42, b[1] * 0.42, (b[2] - bump_h) * 0.38, b[3] * 0.40];
    let glass = grow(pic, gb);
    let glass_r = fit([h * 0.03; 4], glass, pic, [0.0; 4], 1.5);
    // The outline: |x/a|^p + |y/b|^q = 1, flat on top and below (q high), the wings as round as the
    // room allows (the smallest p that keeps the glass inside).
    let (cx, cy) = ((main.0 + main.2) / 2.0, (main.1 + main.3) / 2.0);
    let (a, bb) = ((main.2 - main.0) / 2.0, (main.3 - main.1) / 2.0);
    let q = 9.0f32;
    let (gx, gy) = ((glass.2 - cx).max(cx - glass.0) + 2.0, (glass.3 - cy).max(cy - glass.1) + 2.0);
    let mut p = 2.4f32;
    while p < 40.0 && (gx / a).powf(p) + (gy / bb).powf(q) > 0.9 {
        p += 0.25;
    }
    let body = move |x: f32, y: f32| {
        let (ux, uy) = (((x - cx).abs() / a).max(1e-4), ((y - cy).abs() / bb).max(1e-4));
        let f = ux.powf(p) + uy.powf(q);
        let gxv = p / a * ux.powf(p - 1.0);
        let gyv = q / bb * uy.powf(q - 1.0);
        (f - 1.0) / (gxv * gxv + gyv * gyv).sqrt().max(1e-4)
    };
    // Each shoulder: the outline grown by `bump_h` round the top corner, from a third of the way in
    // and down the wing to a third of its height, thinning out smoothly at both ends.
    let reach_in = a * 0.6;
    let reach_down = main.1 + bb * 0.62;
    let shoulder = move |x: f32, y: f32| {
        let k = smoothstep(reach_in, reach_in + a * 0.16, (x - cx).abs()) * smoothstep(reach_down, reach_down - bb * 0.3, y);
        body(x, y) - bump_h * k
    };
    let mat = Material::new([0.25, 0.225, 0.44], Surface::Matte, 0.14);
    let shoulder_mat = Material::new([0.36, 0.36, 0.38], Surface::Satin, 0.18);
    let thin = b.iter().cloned().fold(f32::MAX, f32::min);
    let hh = Handheld {
        pic,
        px,
        plate: main,
        body: &body,
        rim: (thin * 0.5).clamp(3.0, 16.0),
        mat,
        glass,
        glass_r,
        glass_tone: [0.055, 0.055, 0.062],
        glass_gloss: 1.3,
        wall: None,
        bosses: Vec::new(),
        boss_r: 0.0,
    };
    // A round grille of dots low on the right: on the wing if it is wide enough, else below.
    let wing = plate.2 - glass.2;
    let grille = if wing >= 46.0 {
        let gr = (wing * 0.3).min(h * 0.07);
        Some((glass.2 + wing * 0.45, pic.3 - gr * 1.4, gr))
    } else if plate.3 - glass.3 >= 30.0 {
        let gr = ((plate.3 - glass.3) * 0.32).min(h * 0.06);
        Some((pic.2 - game.w as f32 * 0.1, (glass.3 + plate.3) / 2.0, gr))
    } else {
        None
    }
    .filter(|&(gx, gy, gr)| body(gx + gr, gy + gr) < -3.0 && body(gx + gr, gy - gr) < -3.0);
    let r = led_radius(px);
    let led = led_if(
        Spot {
            x: (plate.0 + glass.0) / 2.0 + 2.0,
            y: pic.1 + h * 0.16,
            r,
        },
        &body,
        pic,
    )
    .filter(|_| wing >= r * 6.0)
    .or_else(|| {
        led_if(
            Spot {
                x: pic.0 + game.w as f32 * 0.03,
                y: (main.1 + glass.1) / 2.0,
                r,
            },
            &body,
            pic,
        )
        .filter(|_| glass.1 - main.1 >= r * 4.5)
    });
    // The shoulders first (they are behind), with a dark line where the body covers them.
    let mut behind = Canvas::new(view_w, view_h);
    if bump_h > 0.0 {
        behind.paint(plate, game, |x, y| {
            let d = shoulder(x, y);
            let cover = (0.5 - d).clamp(0.0, 1.0);
            if cover <= 0.0 || body(x, y) < -1.5 {
                return None;
            }
            let n = normal(shoulder, x, y);
            let c = shoulder_mat.shade(x, y, d, n, (bump_h * 0.7).max(2.0), ((x - plate.0) / (plate.2 - plate.0), 0.2));
            Some((scale(c, 1.0 - 0.5 * (-(body(x, y).max(0.0) / 1.5).powi(2)).exp()), cover))
        });
    }
    hh.paint(&mut canvas, game, |x, y, c, on| {
        let Some((gx, gy, gr)) = grille else { return c };
        if on != On::Body || (x - gx).abs() > gr + 2.0 || (y - gy).abs() > gr + 2.0 {
            return c;
        }
        // Dots on a hexagonal lattice inside a circle.
        let pitch = (gr * 0.3).clamp(5.0, 11.0);
        let row = ((y - gy) / (pitch * 0.866)).round();
        let shift = if (row as i32).rem_euclid(2) == 1 { 0.5 } else { 0.0 };
        let col = ((x - gx) / pitch - shift).round();
        let (hx, hy) = (gx + (col + shift) * pitch, gy + row * pitch * 0.866);
        if ((hx - gx).powi(2) + (hy - gy).powi(2)).sqrt() > gr - pitch * 0.3 {
            return c;
        }
        let hr = pitch * 0.26;
        let dh = ((x - hx).powi(2) + (y - hy).powi(2)).sqrt();
        let cover = 1.0 - smoothstep(hr - 0.7, hr + 0.7, dh);
        holed(c, mat.base, cover, smoothstep(0.0, hr, y - hy))
    });
    // The body on top of the shoulders.
    for (o, s) in canvas.rgba.chunks_exact_mut(4).zip(behind.rgba.chunks_exact(4)) {
        let a = o[3] as u32;
        if a < 255 && s[3] > 0 {
            for k in 0..4 {
                o[k] = (o[k] as u32 + s[k] as u32 * (255 - a) / 255).min(255) as u8;
            }
        }
    }
    finish(canvas, game, led)
}

// --- Twin -------------------------------------------------------------------------------------------

/// The gap between the two screens of a stacked Nintendo DS picture, in DS pixels: Some when the
/// picture is one screen wide (256) and two high (384) plus a gap of up to 126 lines.
pub fn stacked_gap(game: Rect, px: f32) -> Option<f32> {
    if px <= 0.0 {
        return None;
    }
    let (w, h) = (game.w as f32 / px, game.h as f32 / px);
    ((w - 256.0).abs() < 3.0 && (383.5..=510.5).contains(&h)).then(|| (h - 384.0).round().max(0.0))
}

/// Two bodies, one for each screen of a stacked Nintendo DS picture, with the hinge between them:
/// exactly in the gap between the screens when there is one (drawn over that strip, which holds no
/// game picture), beside the picture where they touch. Speaker slits beside the top screen. Other
/// layouts (side by side, one screen) get the plain lens.
pub(crate) fn twin(view_w: u32, view_h: u32, game: Rect, px: f32, b: [f32; 4]) -> Option<Bezel> {
    let Some(gap) = stacked_gap(game, px) else {
        return classic(Style::Handheld, view_w, view_h, game, px, bands(Style::Handheld, view_w, view_h, game));
    };
    let mut canvas = Canvas::new(view_w, view_h);
    let pic = hole(game);
    let plate = grow(pic, b);
    let h = game.h as f32;
    let screen = 192.0 * px;
    // The gap between the screens (rows of the picture that hold no game), and the hinge: a cylinder
    // centred in the gap, as thick as the gap up to a point (the lid and the base fill the rest), or
    // centred on the seam when the screens touch.
    let seam = pic.1 + screen;
    let gap_rows = ((pic.1 + screen).ceil(), (pic.1 + screen + gap * px).floor());
    let cyl = (h * 0.045).clamp(8.0, 48.0);
    let (hy0, hy1) = if gap >= 1.0 {
        let t = (gap_rows.1 - gap_rows.0).min(cyl);
        let c = (gap_rows.0 + gap_rows.1) / 2.0;
        ((c - t / 2.0).round(), (c + t / 2.0).round())
    } else {
        let t = cyl * 0.8;
        ((seam - t / 2.0).round(), (seam + t / 2.0).round())
    };
    let hc = (hy0 + hy1) / 2.0;
    let hr = (hy1 - hy0) / 2.0;
    let top = (plate.0, plate.1, plate.2, hy0 - 1.0);
    let bottom = (plate.0, hy1 + 1.0, plate.2, plate.3);
    let top_pic = (pic.0, pic.1, pic.2, pic.1 + screen);
    let bottom_pic = (pic.0, pic.3 - screen, pic.2, pic.3);
    let g = [b[0] * 0.24, b[1] * 0.24, b[2] * 0.4, b[3] * 0.4];
    // Each glass reaches a little past its screen into the gap, or to the hinge.
    let into_gap = |room: f32| if gap >= 1.0 { (room * 0.55).min(g[0]).max(0.0) } else { room.max(0.0) };
    let glass_top = (pic.0 - g[0], pic.1 - g[2], pic.2 + g[1], top_pic.3 + into_gap(top.3 - 2.0 - top_pic.3));
    let glass_bottom = (pic.0 - g[0], bottom_pic.1 - into_gap(bottom_pic.1 - bottom.1 - 2.0), pic.2 + g[1], pic.3 + g[3]);
    let gr_ = fit([h * 0.012; 4], glass_top, top_pic, [0.0; 4], 1.0);
    let gr_b = fit([h * 0.012; 4], glass_bottom, bottom_pic, [0.0; 4], 1.0);
    // The outer corners as round as the band allows, the ones at the hinge tighter.
    // With no band above or below (the picture as high as the screen) the corner takes the whole
    // band, so the body meets the panel edge only where the picture does.
    let outer = |a: f32, c: f32| if c < 4.0 { (a - 3.0).max(0.0) } else { corner_max(a, c, 4.0).min(h * 0.12) };
    let top_r = [outer(b[0], b[2]), outer(b[1], b[2]), (h * 0.02).min(b[1] * 0.5), (h * 0.02).min(b[0] * 0.5)];
    let bottom_r = [(h * 0.02).min(b[0] * 0.5), (h * 0.02).min(b[1] * 0.5), outer(b[1], b[3]), outer(b[0], b[3])];
    let mat = Material::new([0.66, 0.67, 0.70], Surface::Satin, 0.16);
    let thin = b[0].min(b[1]);
    let top_sd = move |x: f32, y: f32| sd_rr4(x, y, top, top_r);
    let bottom_sd = move |x: f32, y: f32| sd_rr4(x, y, bottom, bottom_r);
    let rim = (thin * 0.3).clamp(3.0, 14.0);
    let inlay = (gap >= 1.0 && gap_rows.1 > gap_rows.0).then_some(Rect {
        x: game.x,
        y: gap_rows.0 as u32,
        w: game.w,
        h: (gap_rows.1 - gap_rows.0) as u32,
    });
    let over = inlay.map(|r| (r.x as f32, r.y as f32, (r.x + r.w) as f32, (r.y + r.h) as f32));
    for (half, sd, glass, glass_r, scr) in [
        (top, &top_sd as &(dyn Fn(f32, f32) -> f32 + Sync), glass_top, gr_, top_pic),
        (bottom, &bottom_sd, glass_bottom, gr_b, bottom_pic),
    ] {
        let hh = Handheld {
            pic: scr,
            px,
            plate: half,
            body: sd,
            rim,
            mat,
            glass,
            glass_r,
            glass_tone: [0.06, 0.06, 0.066],
            glass_gloss: 1.2,
            wall: None,
            bosses: Vec::new(),
            boss_r: 0.0,
        };
        let is_top = half == top;
        // Speaker slits in the body beside the top screen, left and right.
        let side = (glass.0 - plate.0).min(plate.2 - glass.2);
        let len = (side * 0.42).min(h * 0.05);
        let t = (px * 0.9).clamp(2.5, 5.0);
        let pitch = t * 2.6;
        let yc = (top_pic.1 + top_pic.3) / 2.0;
        let slits = (is_top && side >= 24.0).then(|| [((plate.0 + glass.0) / 2.0, yc), ((plate.2 + glass.2) / 2.0, yc)]);
        let decor = |x: f32, y: f32, c: Rgb, on: On| {
            let Some(centres) = slits else { return c };
            if on != On::Body {
                return c;
            }
            let mut c = c;
            for (sx, sy) in centres {
                let row = ((y - sy) / pitch).round().clamp(-2.0, 2.0);
                let ry = sy + row * pitch;
                let (d, _) = sd_segment(x, y, (sx - len / 2.0, ry), (sx + len / 2.0, ry));
                let cover = (t * 0.5 + 0.5 - d).clamp(0.0, 1.0);
                if cover > 0.0 {
                    c = holed(c, mat.base, cover, smoothstep(0.0, t * 0.5, y - ry));
                }
            }
            c
        };
        hh.paint(&mut canvas, game, decor);
        if over.is_some() {
            hh.paint_in(&mut canvas, game, over, decor);
        }
    }
    // The hinge, a cylinder across the whole width, in a darker grey; its two ends belong to the lid.
    let hinge_c = [0.30, 0.305, 0.32];
    let w = plate.2 - plate.0;
    let cuts = [pic.0 - b[0] * 0.18, pic.2 + b[1] * 0.18];
    let hinge = move |x: f32, y: f32| {
        let (d, _) = sd_segment(x, y, (plate.0 + hr, hc), (plate.2 - hr, hc));
        let d = d - hr;
        let cover = (0.5 - d).clamp(0.0, 1.0);
        if cover <= 0.0 {
            return None;
        }
        let ny = ((y - hc) / hr.max(1.0)).clamp(-1.0, 1.0);
        let nz = (1.0 - ny * ny).max(0.0).sqrt();
        let lit = ((ny * LIGHT[1] + nz * LIGHT[2]) / LIGHT[2]).clamp(0.25, 1.5);
        let hv = [LIGHT[0], LIGHT[1], LIGHT[2] + 1.0];
        let hl = (hv[0] * hv[0] + hv[1] * hv[1] + hv[2] * hv[2]).sqrt();
        let spec = ((ny * hv[1] + nz * hv[2]) / hl).max(0.0).powf(30.0);
        let part = if x < cuts[0] || x > cuts[1] { scale(mat.base, 0.92) } else { hinge_c };
        let mut c = add(scale(part, lit.min(1.25) * (1.02 - 0.08 * ((x - plate.0) / w))), [0.07 * spec; 3]);
        for cx in cuts {
            c = scale(c, 1.0 - 0.7 * (-((x - cx) / 0.9).powi(2)).exp());
        }
        Some((scale(c, 0.75 + 0.25 * smoothstep(-hr, -hr * 0.4, d)), cover))
    };
    canvas.paint((plate.0, hy0 - 1.0, plate.2, hy1 + 1.0), game, hinge);
    if let Some(r) = over {
        // In the gap: the hinge, and black where neither it nor the lid or base reach (the seams).
        canvas.paint_over(r, move |x, y| Some(hinge(x, y).map_or(([0.0; 3], 1.0), |(c, k)| (scale(c, k), 1.0))));
    }
    let mut out = canvas.done(game);
    out.inlay = inlay;
    Some(out)
}

// --- Gloss ------------------------------------------------------------------------------------------

/// A piano-black wide body whose ends are as oval as the room allows, a soft diagonal sheen, a
/// matte black border round the picture with a fine bright edge, and the light low on the left.
pub(crate) fn gloss(view_w: u32, view_h: u32, game: Rect, px: f32, b: [f32; 4]) -> Bezel {
    let mut canvas = Canvas::new(view_w, view_h);
    let pic = hole(game);
    let plate = grow(pic, b);
    let h = game.h as f32;
    let gb = b.map(|v| (v * 0.2).max(v.min(2.0)));
    let glass = grow(pic, gb);
    let glass_r = fit([h * 0.012; 4], glass, pic, [0.0; 4], 1.0);
    let half = (plate.3 - plate.1) / 2.0;
    let body_r = fit([half; 4], plate, glass, glass_r, 3.0);
    let body = move |x: f32, y: f32| sd_rr4(x, y, plate, body_r);
    let mat = Material::new([0.020, 0.020, 0.023], Surface::Piano, 0.30);
    let thin = b.iter().cloned().fold(f32::MAX, f32::min);
    let hh = Handheld {
        pic,
        px,
        plate,
        body: &body,
        rim: (b.iter().cloned().fold(0.0, f32::max) * 0.06).clamp(3.0, 12.0).min(thin.max(3.0)),
        mat,
        glass,
        glass_r,
        glass_tone: [0.012, 0.012, 0.014],
        glass_gloss: 0.5,
        wall: None,
        bosses: Vec::new(),
        boss_r: 0.0,
    };
    let r = led_radius(px);
    let led = led_if(
        Spot {
            x: (plate.0 + glass.0) / 2.0,
            y: pic.3 - h * 0.12,
            r,
        },
        &body,
        pic,
    )
    .filter(|_| b[0] >= r * 7.0)
    .or_else(|| {
        led_if(
            Spot {
                x: pic.0 + game.w as f32 * 0.06,
                y: (glass.3 + plate.3) / 2.0,
                r,
            },
            &body,
            pic,
        )
        .filter(|_| plate.3 - glass.3 >= r * 4.5)
    });
    hh.paint(&mut canvas, game, |x, y, c, on| {
        if on != On::Glass {
            return c;
        }
        // A fine bright line where the border meets the lacquer.
        let dg = sd_rr4(x, y, glass, glass_r);
        add(c, [0.07 * (-((dg + 0.6) / 0.6).powi(2)).exp(); 3])
    });
    finish(canvas, game, led)
}

// --- Watch ------------------------------------------------------------------------------------------

/// A brushed gold or silver faceplate in a thin dark plastic rim, a dark window round the small
/// screen and one printed line around it.
pub(crate) fn watch(finish_: WatchFinish, view_w: u32, view_h: u32, game: Rect, px: f32, b: [f32; 4]) -> Bezel {
    let mut canvas = Canvas::new(view_w, view_h);
    let pic = hole(game);
    let plate = grow(pic, b);
    let h = game.h as f32;
    let thin = b.iter().cloned().fold(f32::MAX, f32::min);
    let rim_w = (thin * 0.14).clamp(2.5, 14.0);
    let face = (plate.0 + rim_w, plate.1 + rim_w, plate.2 - rim_w, plate.3 - rim_w);
    let wm = (thin * 0.14).clamp(2.0, 12.0);
    let window = grow(pic, [wm; 4]);
    let window_r = fit([(wm * 0.6).max(1.5); 4], window, pic, [0.0; 4], 0.5);
    let face_r = fit([h * 0.03; 4], face, window, window_r, 1.0);
    let body_r = face_r.map(|r| r + rim_w);
    let (metal, ink) = match finish_ {
        WatchFinish::Gold => ([0.78, 0.62, 0.34], [0.36, 0.13, 0.08]),
        WatchFinish::Silver => ([0.74, 0.75, 0.77], [0.12, 0.16, 0.30]),
    };
    let plate_mat = Material::new(metal, Surface::Brushed, 0.20);
    let rim_mat = Material::new([0.09, 0.09, 0.095], Surface::Matte, 0.22);
    let line_at = wm + ((thin - rim_w - wm) * 0.4).max(2.0);
    let line = grow(pic, [line_at; 4]);
    let line_r = fit([h * 0.012 + line_at * 0.2; 4], line, pic, [0.0; 4], 0.5);
    let lw = (thin * 0.012).clamp(1.2, 2.5);
    let show_line = thin - rim_w - wm >= 10.0;
    let px_ = px.max(1.0);
    canvas.paint(plate, game, |x, y| {
        let sd_body = |x: f32, y: f32| sd_rr4(x, y, plate, body_r);
        let d = sd_body(x, y);
        let cover = (0.5 - d).clamp(0.0, 1.0);
        if cover <= 0.0 {
            return None;
        }
        let (u, v) = ((x - plate.0) / (plate.2 - plate.0), (y - plate.1) / (plate.3 - plate.1));
        let df = sd_rr4(x, y, face, face_r);
        let dw = sd_rr4(x, y, window, window_r);
        let c = if dw < 0.5 {
            // The window: near black, the LCD's edge, a fine lit lip below.
            let below = if y > pic.3 { 1.0 } else { 0.0 };
            let c = [0.03 + 0.02 * below; 3];
            add(c, [0.02 * (-to_pic(x, y, pic) / (px_ * 2.0)).exp(); 3])
        } else if df < 0.5 {
            let n = if -df < 4.0 {
                normal(|x, y| sd_rr4(x, y, face, face_r), x, y)
            } else {
                (0.0, 0.0)
            };
            let mut c = plate_mat.shade(x, y, df, n, 2.0, (u, v));
            // The window is cut into the plate: its edge catches light on the lower side.
            let lip = (-((dw - 0.8) / 0.8).powi(2)).exp();
            c = add(c, [lip * if y > pic.3 { 0.10 } else { -0.06 }; 3]);
            if show_line {
                let dl = sd_rr4(x, y, line, line_r).abs();
                c = mix(c, ink, (lw * 0.5 + 0.5 - dl).clamp(0.0, 1.0) * 0.9);
            }
            // The plate sits in the rim: a fine shadow along its edge.
            scale(c, 1.0 - 0.35 * (-((df + 0.5) / 1.0).powi(2)).exp())
        } else {
            let n = normal(sd_body, x, y);
            rim_mat.shade(x, y, d, n, rim_w * 0.8, (u, v))
        };
        Some((c, cover))
    });
    canvas.done(game)
}

// --- Goggles ----------------------------------------------------------------------------------------

/// A visor: matte black with a soft light shield round the picture, a fine red trim line, a glossy
/// red band along the top and a notch for the nose at the bottom.
pub(crate) fn goggles(view_w: u32, view_h: u32, game: Rect, px: f32, b: [f32; 4]) -> Bezel {
    let mut canvas = Canvas::new(view_w, view_h);
    let pic = hole(game);
    let plate = grow(pic, b);
    let h = game.h as f32;
    let thin = b.iter().cloned().fold(f32::MAX, f32::min);
    let ow = (thin * 0.24).clamp(2.0, 26.0);
    let opening = grow(pic, [ow; 4]);
    let open_r = fit([h * 0.03; 4], opening, pic, [0.0; 4], 0.8);
    let body_r = fit([h * 0.06, h * 0.06, h * 0.16, h * 0.16], plate, opening, open_r, 2.0);
    let (cx, w) = ((plate.0 + plate.2) / 2.0, plate.2 - plate.0);
    let below = plate.3 - opening.3;
    let notch = (below >= 40.0).then(|| {
        let (nw, nh) = ((w * 0.13).min(below * 1.6), below * 0.5);
        ((cx - nw / 2.0, plate.3 - nh, cx + nw / 2.0, plate.3 + nh), nw * 0.48)
    });
    let body = move |x: f32, y: f32| {
        let d = sd_rr4(x, y, plate, body_r);
        match notch {
            Some((n, r)) => d.max(-sd_round_rect(x, y, n, r)),
            None => d,
        }
    };
    let band_end = plate.1 + (opening.1 - plate.1) * 0.34;
    let red_band = opening.1 - plate.1 >= 24.0;
    let trim_at = ow + ((thin - ow) * 0.3).clamp(2.0, 18.0);
    let trim = grow(pic, [trim_at; 4]);
    let trim_r = fit([h * 0.03 + trim_at * 0.5; 4], trim, pic, [0.0; 4], 0.5);
    let show_trim = thin - ow >= 8.0;
    let black = Material::new([0.062, 0.061, 0.066], Surface::Matte, 0.18);
    let red = Material::new([0.56, 0.035, 0.045], Surface::Piano, 0.34);
    let rim = (thin * 0.45).clamp(3.0, 14.0);
    let px_ = px.max(1.0);
    canvas.paint(plate, game, |x, y| {
        let d = body(x, y);
        let cover = (0.5 - d).clamp(0.0, 1.0);
        if cover <= 0.0 {
            return None;
        }
        let n = if -d < rim * 1.2 + 3.0 { normal(body, x, y) } else { (0.0, 0.0) };
        let (u, v) = ((x - plate.0) / w, (y - plate.1) / (plate.3 - plate.1));
        let dop = sd_rr4(x, y, opening, open_r);
        let c = if dop < 0.5 {
            // The light shield: falls away into the dark towards the picture, a faint red from it.
            let t = ((-dop) / ow).clamp(0.0, 1.0);
            let lit = if y > pic.3 { 0.035 } else { 0.012 };
            let c = [lit * (1.0 - t) + 0.006; 3];
            let glowr = 0.05 * (-to_pic(x, y, pic) / (px_ * 3.0)).exp();
            add(c, [glowr, 0.0, 0.0])
        } else {
            let mut c = black.shade(x, y, d, n, rim, (u, v));
            // A fine rubbery grain.
            c = scale(c, 1.0 + 0.10 * (vnoise(x * 0.6, y * 0.6) - 0.5));
            if red_band && y < band_end + 1.0 {
                let band = (band_end - y + 0.5).clamp(0.0, 1.0);
                let rc = red.shade(x, y, d, n, rim, (u, (y - plate.1) / (band_end - plate.1)));
                c = mix(c, rc, band);
                // A seam where the red part meets the black.
                c = scale(c, 1.0 - 0.5 * (-((y - band_end) / 0.8).powi(2)).exp());
            }
            if show_trim {
                let dl = sd_rr4(x, y, trim, trim_r).abs();
                c = mix(c, [0.62, 0.05, 0.06], (1.4 - dl).clamp(0.0, 1.0) * 0.9);
            }
            // The edge of the opening rolls over into the shield.
            add(c, [0.05 * (-((dop - 1.0) / 1.0).powi(2)).exp() * if y > pic.3 { 1.0 } else { 0.4 }; 3])
        };
        Some((c, cover))
    });
    canvas.done(game)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corner_radius_keeps_the_inner_corner_inside() {
        for (a, b, m) in [(10.0, 10.0, 1.0), (60.0, 20.0, 3.0), (124.0, 92.0, 4.0), (3.0, 50.0, 2.0)] {
            let r = corner_max(a, b, m);
            // The inner corner, `a` and `b` in from the outer one, is at least m inside the arc.
            let d = sd_round_rect(a, b, (0.0, 0.0, 1000.0, 1000.0), r);
            assert!(d <= -m + 0.01, "{a} {b} {m}: r {r}, d {d}");
        }
    }

    #[test]
    fn stacked_screens_are_found_with_and_without_a_gap() {
        // 256x384 at Aspect on 1240x1080: 720x1080, 2.8125 px per DS pixel.
        assert_eq!(stacked_gap(Rect { x: 260, y: 0, w: 720, h: 1080 }, 1080.0 / 384.0), Some(0.0));
        // A 64-line gap: 256x448, scaled to 1080 high.
        let px = 1080.0 / 448.0;
        assert_eq!(
            stacked_gap(
                Rect {
                    x: 311,
                    y: 0,
                    w: (256.0 * px) as u32,
                    h: 1080
                },
                px
            ),
            Some(64.0)
        );
        // Side by side (512x192) and one screen (256x192) are not stacked.
        assert_eq!(stacked_gap(Rect { x: 0, y: 307, w: 1240, h: 465 }, 465.0 / 192.0), None);
        assert_eq!(stacked_gap(Rect { x: 0, y: 0, w: 1240, h: 930 }, 930.0 / 192.0), None);
    }
}
