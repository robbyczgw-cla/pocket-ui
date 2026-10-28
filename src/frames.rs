//! The richer built-in screen frames (Shell, CRT, Arcade, Glow), drawn in code at the exact output
//! size like the two original ones in `bezel.rs`. Original designs: generic shapes, rounded corners
//! and shading only; no logos, lettering, buttons or console outlines.
//!
//! Every frame stays inside the bands `bezel::bands` gives it, so a strip of true black remains at
//! the panel edge, and never draws where the picture goes. Colours are sRGB 0..1; the output is
//! premultiplied RGBA, dithered (dark gradients band visibly on the OLED panel).

use crate::bezel::{bands, classic, noise, sd_round_rect, smoothstep, Bezel, Cabinet, Colourway, Style};
use crate::video::Rect;

pub(crate) type Box4 = (f32, f32, f32, f32);
pub(crate) type Rgb = [f32; 3];

/// The picture rectangle in float pixels.
pub(crate) fn hole(game: Rect) -> Box4 {
    (game.x as f32, game.y as f32, (game.x + game.w) as f32, (game.y + game.h) as f32)
}

pub(crate) fn grow((x0, y0, x1, y1): Box4, [l, r, t, b]: [f32; 4]) -> Box4 {
    (x0 - l, y0 - t, x1 + r, y1 + b)
}

/// A corner radius of at most `want`, shrunk where both bands of a corner are narrower, so every
/// corner of the hole stays inside the rounded plate.
pub(crate) fn radius(want: f32, [l, r, t, b]: [f32; 4]) -> f32 {
    let mut radius = want;
    for (a, b) in [(l, t), (r, t), (l, b), (r, b)] {
        if a < radius && b < radius {
            radius = a.max(b);
        }
    }
    radius
}

pub(crate) fn mix(a: Rgb, b: Rgb, t: f32) -> Rgb {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

pub(crate) fn scale(a: Rgb, s: f32) -> Rgb {
    [a[0] * s, a[1] * s, a[2] * s]
}

pub(crate) fn add(a: Rgb, b: Rgb) -> Rgb {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

pub(crate) fn rgb(c: u32) -> Rgb {
    [((c >> 16) & 255) as f32 / 255.0, ((c >> 8) & 255) as f32 / 255.0, (c & 255) as f32 / 255.0]
}

/// Smooth value noise in 0..1 on a lattice of 1 unit (for wood grain and moulding).
pub(crate) fn vnoise(x: f32, y: f32) -> f32 {
    let (ix, iy) = (x.floor(), y.floor());
    let (fx, fy) = (x - ix, y - iy);
    let (ux, uy) = (fx * fx * (3.0 - 2.0 * fx), fy * fy * (3.0 - 2.0 * fy));
    let h = |dx: f32, dy: f32| noise((ix + dx) as i32 as u32, (iy + dy) as i32 as u32) + 0.5;
    let top = h(0.0, 0.0) + (h(1.0, 0.0) - h(0.0, 0.0)) * ux;
    let bottom = h(0.0, 1.0) + (h(1.0, 1.0) - h(0.0, 1.0)) * ux;
    top + (bottom - top) * uy
}

/// Outward unit normal of a signed distance field at (x, y), by central differences.
pub(crate) fn normal(f: impl Fn(f32, f32) -> f32, x: f32, y: f32) -> (f32, f32) {
    let nx = f(x + 1.0, y) - f(x - 1.0, y);
    let ny = f(x, y + 1.0) - f(x, y - 1.0);
    let l = (nx * nx + ny * ny).sqrt().max(1e-6);
    (nx / l, ny / l)
}

/// Light falls from above and a little from the left, towards the screen.
pub(crate) const LIGHT: [f32; 3] = [-0.28, -0.72, 0.64];

/// Brightness of a surface tilted by `slope` (0 flat .. 1 steep) towards the outward direction
/// (nx, ny), relative to a flat one: 1 on the flat, brighter where it faces the light.
pub(crate) fn lambert(nx: f32, ny: f32, slope: f32) -> f32 {
    let k = slope * 1.6;
    let (x, y, z) = (nx * k, ny * k, 1.0);
    let l = (x * x + y * y + z * z).sqrt();
    ((x * LIGHT[0] + y * LIGHT[1] + z * LIGHT[2]) / l / LIGHT[2]).max(0.0)
}

/// Specular highlight of the same tilted surface, viewed straight on.
pub(crate) fn specular(nx: f32, ny: f32, slope: f32, power: f32) -> f32 {
    let k = slope * 1.6;
    let (x, y, z) = (nx * k, ny * k, 1.0);
    let l = (x * x + y * y + z * z).sqrt();
    let h = [LIGHT[0], LIGHT[1], LIGHT[2] + 1.0];
    let hl = (h[0] * h[0] + h[1] * h[1] + h[2] * h[2]).sqrt();
    ((x * h[0] + y * h[1] + z * h[2]) / l / hl).max(0.0).powf(power)
}

/// The output image: premultiplied RGBA at the viewport's size.
pub(crate) struct Canvas {
    pub(crate) w: u32,
    pub(crate) h: u32,
    pub(crate) rgba: Vec<u8>,
}

impl Canvas {
    pub(crate) fn new(w: u32, h: u32) -> Self {
        Canvas { w, h, rgba: vec![0; (w * h * 4) as usize] }
    }

    /// Shades every pixel of `plate` outside the picture with `shade(x, y)` (pixel centres), which
    /// gives a colour and its coverage, and puts it beneath what is already there (premultiplied
    /// "under", so a lens drawn first stays on top). Rows are split over the CPU's cores: a frame is
    /// drawn when the picture's geometry or the setting changes, and that should not stall the game.
    pub(crate) fn paint(&mut self, plate: Box4, game: Rect, shade: impl Fn(f32, f32) -> Option<(Rgb, f32)> + Sync) {
        self.paint_skipping(plate, Some(game), shade);
    }

    /// Like `paint`, but over the picture too: for an inlay (`Bezel::inlay`), which is drawn after it.
    pub(crate) fn paint_over(&mut self, plate: Box4, shade: impl Fn(f32, f32) -> Option<(Rgb, f32)> + Sync) {
        self.paint_skipping(plate, None, shade);
    }

    fn paint_skipping(&mut self, plate: Box4, skip: Option<Rect>, shade: impl Fn(f32, f32) -> Option<(Rgb, f32)> + Sync) {
        let game = skip.unwrap_or(Rect { x: 0, y: 0, w: 0, h: 0 });
        let (w, h) = (self.w, self.h);
        let (x0, x1) = (plate.0.floor().max(0.0) as u32, plate.2.ceil().min(w as f32) as u32);
        let (y0, y1) = (plate.1.floor().max(0.0) as u32, plate.3.ceil().min(h as f32) as u32);
        if x0 >= x1 || y0 >= y1 {
            return;
        }
        let threads = std::thread::available_parallelism().map_or(1, |n| n.get()).clamp(1, 8) as u32;
        let rows = (y1 - y0).div_ceil(threads);
        let row_bytes = (w * 4) as usize;
        let region = &mut self.rgba[y0 as usize * row_bytes..y1 as usize * row_bytes];
        let shade = &shade;
        std::thread::scope(|s| {
            for (i, chunk) in region.chunks_mut(rows as usize * row_bytes).enumerate() {
                s.spawn(move || {
                    for (r, row) in chunk.chunks_mut(row_bytes).enumerate() {
                        let y = y0 + i as u32 * rows + r as u32;
                        // On the picture's rows, its span is skipped in one step: the picture covers it.
                        let on_picture = y >= game.y && y < game.y + game.h;
                        let spans = if on_picture { [(x0, x1.min(game.x)), ((game.x + game.w).max(x0), x1)] } else { [(x0, x1), (x1, x1)] };
                        for x in spans.into_iter().flat_map(|(a, b)| a..b) {
                            let i = x as usize * 4;
                            let above = row[i + 3] as f32 / 255.0;
                            if above >= 1.0 {
                                continue;
                            }
                            let Some((c, cover)) = shade(x as f32 + 0.5, y as f32 + 0.5) else { continue };
                            if cover <= 0.0 {
                                continue;
                            }
                            let dither = noise(x, y) / 255.0;
                            let k = cover * (1.0 - above);
                            for (j, v) in c.iter().enumerate() {
                                let v = ((v.clamp(0.0, 1.0) + dither).clamp(0.0, 1.0)) * k;
                                row[i + j] = (row[i + j] as f32 + v * 255.0 + 0.5).min(255.0) as u8;
                            }
                            row[i + 3] = ((above + k) * 255.0 + 0.5).min(255.0) as u8;
                        }
                    }
                });
            }
        });
    }

    pub(crate) fn done(self, game: Rect) -> Bezel {
        Bezel::new(self.w, self.h, self.rgba, game)
    }
}

// --- Shell ------------------------------------------------------------------------------------------

struct Plastic {
    base: Rgb,
    /// See-through: the insides show, the rims glow.
    clear: bool,
    gloss: f32,
}

fn plastic(c: Colourway) -> Plastic {
    let (base, clear, gloss) = match c {
        Colourway::Grey => ([0.585, 0.585, 0.565], false, 0.10),
        Colourway::Indigo => ([0.165, 0.165, 0.37], false, 0.16),
        Colourway::ClearPurple => ([0.50, 0.40, 0.70], true, 0.34),
        Colourway::AtomicPurple => ([0.40, 0.15, 0.62], true, 0.34),
        Colourway::Black => ([0.068, 0.068, 0.074], false, 0.22),
        Colourway::White => ([0.835, 0.835, 0.815], false, 0.10),
        Colourway::GlacierBlue => ([0.52, 0.72, 0.84], true, 0.34),
    };
    Plastic { base, clear, gloss }
}

/// The handheld lens of `Style::Handheld`, set into a wider moulded body in a colourway.
pub(crate) fn shell(colour: Colourway, view_w: u32, view_h: u32, game: Rect, px: f32, b: [f32; 4]) -> Option<Bezel> {
    let hb = bands(Style::Handheld, view_w, view_h, game);
    // The lens takes up to 42 % of each side (at most its usual width), the body the rest.
    let lb: [f32; 4] = std::array::from_fn(|i| hb[i].min(b[i] * 0.42).max(b[i].min(3.0)));
    let mut canvas = Canvas::new(view_w, view_h);
    if let Some(lens) = classic(Style::Handheld, view_w, view_h, game, px, lb) {
        canvas.rgba = lens.rgba;
    }
    let p = plastic(colour);
    let pic = hole(game);
    let body = grow(pic, b);
    let rb = radius(game.h as f32 * 0.075, b);
    let lens = grow(pic, lb);
    let rl = radius(game.h as f32 * 0.03, lb);
    let thin = b.iter().cloned().fold(f32::MAX, f32::min);
    // Width of the rounded rim along the outer edge.
    let rim = (thin * 0.5).clamp(3.0, 16.0);
    // The inner wall a see-through shell shows, and the screw posts in its corners.
    let wall_in = (thin * 0.3).clamp(3.0, 22.0);
    let wall = (body.0 + wall_in, body.1 + wall_in, body.2 - wall_in, body.3 - wall_in);
    let rw = (rb - wall_in).max(2.0);
    let boss_r = (thin * 0.11).clamp(3.0, 10.0);
    // A small speaker grille of round holes, low on the right, where the body below the lens is
    // tall enough: (centre x, centre y, pitch).
    let below = body.3 - lens.3;
    let grille = (below >= 34.0 && b[1] >= 12.0).then(|| {
        let pitch = (below * 0.15).clamp(6.0, 11.0);
        (pic.2 - game.w as f32 * 0.1, (lens.3 + body.3) / 2.0, pitch)
    });
    let corner_bands = [(b[0], b[2]), (b[1], b[2]), (b[0], b[3]), (b[1], b[3])];
    let bosses: Vec<(f32, f32)> = [(wall.0, wall.1), (wall.2, wall.1), (wall.0, wall.3), (wall.2, wall.3)]
        .iter()
        .zip(corner_bands)
        .filter(|(_, (a, c))| a.min(*c) >= boss_r * 5.0)
        .map(|((x, y), _)| {
            let off = rw * 0.3 + boss_r * 1.6;
            (if *x < pic.0 { x + off } else { x - off }, if *y < pic.1 { y + off } else { y - off })
        })
        .collect();
    let height = body.3 - body.1;
    canvas.paint(body, game, |fx, fy| {
        let sd = |x: f32, y: f32| sd_round_rect(x, y, body, rb);
        let d = sd(fx, fy);
        let cover = (0.5 - d).clamp(0.0, 1.0);
        if cover <= 0.0 {
            return None;
        }
        // Flat beyond the rim and the seam: no normal needed there.
        let (nx, ny) = if -d < rim * 1.2 + 3.0 { normal(sd, fx, fy) } else { (0.0, 0.0) };
        let t = (-d / rim).clamp(0.0, 1.0);
        let slope = (1.0 - t).powf(1.7);
        let vt = ((fy - body.1) / height).clamp(0.0, 1.0);
        let diffuse = lambert(nx, ny, slope).clamp(0.35, 1.45);
        let mut c = scale(p.base, (1.04 - 0.13 * vt) * diffuse);
        // Moulded plastic: a faint, broad sheen across the upper left.
        let u = (fx - body.0) / (body.2 - body.0) + 0.7 * vt;
        let sheen = smoothstep(0.05, 0.35, u) * (1.0 - smoothstep(0.35, 0.75, u));
        c = add(c, [p.gloss * if p.clear { 0.22 } else { 0.12 } * sheen; 3]);
        // The lit rim along the top edge.
        c = add(c, [p.gloss * 1.4 * specular(nx, ny, slope, 24.0) * slope; 3]);
        // Distance outside the lens: a soft contact shadow where it sits in the body, and a lit lip
        // below it.
        let dl = sd_round_rect(fx, fy, lens, rl);
        let ao = 1.0 - 0.42 * (-dl.max(0.0) / (1.6 + rim * 0.18)).exp();
        c = scale(c, ao);
        if fy > lens.3 - rl && dl < 2.5 {
            c = add(c, [0.05 * (1.0 - smoothstep(1.0, 2.5, dl)); 3]);
        }
        if p.clear {
            // See-through: the dark insides show between the inner wall and the lens, the plastic is
            // lighter (and milky) towards the rims, where light passes through its edge.
            let dw = sd_round_rect(fx, fy, wall, rw);
            let inside = smoothstep(1.5, -1.5, dw);
            c = scale(c, 1.0 - 0.36 * inside);
            c = scale(c, 1.0 - 0.32 * (-(dw / 1.6).powi(2)).exp());
            c = add(c, scale(p.base, 0.10 * (-(dw - 2.5).powi(2) / 4.0).exp()));
            c = mix(c, scale(p.base, 1.45), 0.30 * slope);
            for (bx, by) in &bosses {
                let r = ((fx - bx).powi(2) + (fy - by).powi(2)).sqrt();
                // A round post: a darker ring, a lighter screw head with a soft edge.
                let ring = (-((r - boss_r) / 1.3).powi(2)).exp();
                let head = 1.0 - smoothstep(boss_r * 0.45, boss_r * 0.62, r);
                c = scale(c, 1.0 - 0.38 * ring);
                c = mix(c, scale(p.base, 0.55), 0.6 * head);
                c = add(c, [0.05 * head * (1.0 - smoothstep(0.0, boss_r * 0.4, ((fx - bx + 1.0).powi(2) + (fy - by + 1.0).powi(2)).sqrt())); 3]);
            }
        } else {
            // Matte: a whisper of moulding texture, and the seam where the two halves meet.
            c = scale(c, 1.0 + 0.025 * (vnoise(fx * 0.9, fy * 0.9) - 0.5));
            let seam = -d - rim * 1.1;
            c = scale(c, 1.0 - 0.22 * (-(seam / 0.7).powi(2)).exp());
            c = scale(c, 1.0 + 0.10 * (-((seam - 1.3) / 0.7).powi(2)).exp() * (-ny).max(0.3));
        }
        if let Some((gx, gy, pitch)) = grille {
            // Five by three holes, every other row shifted by half a pitch.
            let row = ((fy - gy) / pitch + 1.0).round();
            if (0.0..=2.0).contains(&row) {
                let shift = if row as i32 % 2 == 1 { 0.5 } else { 0.0 };
                let col = ((fx - gx) / pitch + 2.0 - shift).round();
                if (0.0..=4.0).contains(&col) && !(shift > 0.0 && col > 3.0) {
                    let (hx, hy) = (gx + (col - 2.0 + shift) * pitch, gy + (row - 1.0) * pitch);
                    let r = pitch * 0.28;
                    let dh = ((fx - hx).powi(2) + (fy - hy).powi(2)).sqrt();
                    let hole = 1.0 - smoothstep(r - 0.7, r + 0.7, dh);
                    // Inside, the lower wall of the hole catches the light.
                    let lit = smoothstep(0.0, r, fy - hy) * hole;
                    c = mix(c, add(scale(p.base, 0.18), [0.012; 3]), hole);
                    c = add(c, scale(p.base, 0.25 * lit));
                }
            }
        }
        Some((c, cover))
    });
    Some(canvas.done(game))
}

// --- CRT --------------------------------------------------------------------------------------------

/// A curved-screen TV around the picture: the tube's dark glass in a rounded opening that bulges a
/// little, a sloped inner bezel and the cabinet in dark plastic or wood veneer.
pub(crate) fn crt(cabinet: Cabinet, view_w: u32, view_h: u32, game: Rect, px: f32, b: [f32; 4]) -> Bezel {
    let mut canvas = Canvas::new(view_w, view_h);
    let pic = hole(game);
    let plate = grow(pic, b);
    let rp = radius(game.h as f32 * 0.06, b);
    let (cx, cy) = ((pic.0 + pic.2) / 2.0, (pic.1 + pic.3) / 2.0);
    let (hw, hh) = ((pic.2 - pic.0) / 2.0, (pic.3 - pic.1) / 2.0);
    let (bx, by) = (b[0].min(b[1]), b[2].min(b[3]));
    // The opening: a superellipse a little larger than the picture, as round as the room allows
    // while every corner of the picture stays inside.
    let (ax, ay) = (hw + (bx * 0.2).clamp(1.5, 26.0), hh + (by * 0.2).clamp(1.5, 26.0));
    let mut n = 4.0f32;
    while n < 80.0 && (hw / ax).powf(n) + (hh / ay).powf(n) > 0.97 {
        n += 0.5;
    }
    let g = |x: f32, y: f32| (((x - cx).abs() / ax).powf(n) + ((y - cy).abs() / ay).powf(n)).powf(1.0 / n);
    // Signed distance to the opening's edge (positive outside), from the field's gradient.
    let d_open = |x: f32, y: f32| {
        let v = g(x, y);
        let gx = (g(x + 0.5, y) - g(x - 0.5, y)).abs();
        let gy = (g(x, y + 0.5) - g(x, y - 0.5)).abs();
        (v - 1.0) / (gx * gx + gy * gy).sqrt().max(1e-6)
    };
    let thin = bx.min(by);
    let ring = (thin * 0.36).clamp(2.5, 34.0);
    let rim = (thin * 0.22).clamp(2.0, 10.0);
    let height = plate.3 - plate.1;
    let dark = [0.078, 0.077, 0.082];
    let px = px.max(1.0);
    canvas.paint(plate, game, |fx, fy| {
        let sd = |x: f32, y: f32| sd_round_rect(x, y, plate, rp);
        let d = sd(fx, fy);
        let cover = (0.5 - d).clamp(0.0, 1.0);
        if cover <= 0.0 {
            return None;
        }
        // Well outside the opening's box only the cabinet is drawn: skip the costly exact distance.
        let (ox, oy) = (((fx - cx).abs() - ax).max(0.0), ((fy - cy).abs() - ay).max(0.0));
        let boxed = (ox * ox + oy * oy).sqrt();
        let o = if boxed > ring + 6.0 { boxed } else { d_open(fx, fy) };
        let vt = ((fy - plate.1) / height).clamp(0.0, 1.0);
        let c = if o < 0.0 {
            // Tube glass: near black, darker under the bezel's shadow, a faint glow from the picture
            // and a soft reflection in the upper left.
            let dx = (pic.0 - fx).max(fx - pic.2).max(0.0);
            let dy = (pic.1 - fy).max(fy - pic.3).max(0.0);
            let near = (-(dx * dx + dy * dy).sqrt() / (px * 2.5)).exp();
            let (u, v) = ((fx - cx) / ax, (fy - cy) / ay);
            let refl = 0.03 * (-((u + 0.55).powi(2) + (v + 0.6).powi(2)) / 0.18).exp();
            let shade = 0.55 + 0.45 * smoothstep(0.0, 6.0, -o);
            let glass = scale([0.034, 0.038, 0.042], shade);
            let c = add(glass, [0.024 * near + refl; 3]);
            // The opening's edge, anti-aliased against the bezel.
            mix(c, scale(dark, 0.6), smoothstep(-1.0, 0.0, o))
        } else if o < ring {
            // The inner bezel slopes down into the tube: its top half faces down (dark), its bottom
            // half up into the light.
            let (nx, ny) = normal(d_open, fx, fy);
            let t = o / ring;
            let slope = 0.75 * (1.0 - t) + 0.25;
            let lit = lambert(-nx, -ny, slope).clamp(0.25, 1.9);
            let mut c = scale(dark, lit);
            // A fine crest where the bezel meets the cabinet, and a groove behind it.
            c = add(c, [0.05 * (-((o - ring + 1.5) / 0.9).powi(2)).exp() * (0.4 + 0.6 * (-ny).max(0.0)); 3]);
            c
        } else {
            let (nx, ny) = if -d < rim + 1.0 { normal(sd, fx, fy) } else { (0.0, 0.0) };
            let slope = (1.0 - (-d / rim).clamp(0.0, 1.0)).powf(1.6);
            let lit = lambert(nx, ny, slope).clamp(0.4, 1.5);
            let groove = 1.0 - 0.55 * (-((o - ring) / 1.4).powi(2)).exp();
            let base = match cabinet {
                Cabinet::Plastic => scale([0.112, 0.108, 0.104], (1.06 - 0.16 * vt) * (1.0 + 0.03 * (vnoise(fx * 0.7, fy * 0.7) - 0.5))),
                Cabinet::Wood => wood(fx, fy, vt),
            };
            let mut c = scale(base, lit * groove);
            c = add(c, [0.05 * specular(nx, ny, slope, 20.0) * slope; 3]);
            c
        };
        // Vignette: the frame falls off a little towards its outer corners.
        let (u, v) = ((fx - cx) / (plate.2 - cx), (fy - cy) / (plate.3 - cy));
        let c = scale(c, 1.0 - 0.18 * ((u * u + v * v) / 2.0).powf(1.5));
        Some((c, cover))
    });
    canvas.done(game)
}

/// Walnut veneer: grain running across, gently waving, with a satin finish lit from above.
pub(crate) fn wood(x: f32, y: f32, vt: f32) -> Rgb {
    // Growth lines: across the panel, bending gently, unevenly spaced, each a thin darker line.
    let warp = 14.0 * vnoise(x * 0.0035, y * 0.012) + 5.0 * vnoise(x * 0.011, y * 0.03);
    let r = ((y + warp) * 0.085 + 0.6 * vnoise(x * 0.002, y * 0.05)).fract();
    let line = smoothstep(0.62, 0.9, r) * (1.0 - smoothstep(0.9, 1.0, r));
    // Long fibres along the grain, and slow changes of tone.
    let fibres = vnoise(x * 0.012, y * 0.9) - 0.5;
    let tone = vnoise(x * 0.0025, y * 0.008);
    let t = (0.55 + 0.35 * (tone - 0.5) - 0.55 * line + 0.22 * fibres).clamp(0.0, 1.0);
    let c = mix([0.135, 0.072, 0.038], [0.335, 0.192, 0.100], t);
    // Satin clear coat: a broad soft highlight near the top.
    add(scale(c, 1.05 - 0.15 * vt), [0.035 * (-((vt - 0.1) / 0.12).powi(2)).exp(); 3])
}

// --- Arcade -----------------------------------------------------------------------------------------

/// An arcade cabinet's bezel: glossy black with a long soft reflection, a fine trim around the
/// picture and two light strips (at the sides, or above and below when the sides are thin).
pub(crate) fn arcade(view_w: u32, view_h: u32, game: Rect, px: f32, b: [f32; 4]) -> Bezel {
    let _ = px;
    let mut canvas = Canvas::new(view_w, view_h);
    let pic = hole(game);
    let plate = grow(pic, b);
    let rp = radius(game.h as f32 * 0.02, b);
    let (w, h) = (plate.2 - plate.0, plate.3 - plate.1);
    let sides = b[0].min(b[1]) >= 18.0;
    // Strips: (along x? , position across, start, end, band width).
    let strips: Vec<(bool, f32, f32, f32, f32)> = if sides {
        let (s, e) = (pic.1 + game.h as f32 * 0.04, pic.3 - game.h as f32 * 0.04);
        vec![(false, plate.0 + b[0] * 0.3, s, e, b[0]), (false, plate.2 - b[1] * 0.3, s, e, b[1])]
    } else {
        let (s, e) = (pic.0 + game.w as f32 * 0.04, pic.2 - game.w as f32 * 0.04);
        vec![(true, plate.1 + b[2] * 0.3, s, e, b[2]), (true, plate.3 - b[3] * 0.3, s, e, b[3])]
    };
    let cool = [0.22, 0.52, 1.0];
    let warm = [0.86, 0.26, 0.82];
    canvas.paint(plate, game, |fx, fy| {
        let sd = |x: f32, y: f32| sd_round_rect(x, y, plate, rp);
        let d = sd(fx, fy);
        let cover = (0.5 - d).clamp(0.0, 1.0);
        if cover <= 0.0 {
            return None;
        }
        let (u, v) = ((fx - plate.0) / w, (fy - plate.1) / h);
        // Gloss black: the room's light as a long gradient, and one soft diagonal streak.
        let mut c = [0.012 + 0.030 * (1.0 - v).powi(2); 3];
        let streak = u * 0.55 + v;
        c = add(c, [0.022 * smoothstep(0.18, 0.3, streak) * (1.0 - smoothstep(0.3, 0.5, streak)); 3]);
        // The outer edge: a thin lit bevel on top, a shadow at the bottom.
        let (_, ny) = normal(sd, fx, fy);
        let edge = smoothstep(-3.0, -0.5, d) * (1.0 - smoothstep(-0.5, 0.5, d));
        c = add(c, [edge * (0.08 * (-ny).max(0.0) - 0.01 * ny.max(0.0)); 3]);
        // Trim around the picture: a black gap, then a fine bright line, brighter on top.
        let dx = (pic.0 - fx).max(fx - pic.2).max(0.0);
        let dy = (pic.1 - fy).max(fy - pic.3).max(0.0);
        let dp = (dx * dx + dy * dy).sqrt();
        let chamfer = (game.h as f32 * 0.006).clamp(2.0, 5.0);
        if dp < 2.0 {
            c = [0.0; 3];
        } else if dp < 2.0 + chamfer {
            // A bevel cut down to the picture: the lower one faces up into the light.
            let up = if fy > pic.3 { 0.11 } else if fy < pic.1 { 0.025 } else { 0.05 };
            c = [up * (1.0 - 0.4 * (dp - 2.0) / chamfer); 3];
        } else {
            let trim = (-((dp - 2.0 - chamfer - 0.6) / 0.7).powi(2)).exp();
            c = add(c, [trim * if fy < pic.1 { 0.16 } else if fy > pic.3 { 0.06 } else { 0.10 }; 3]);
        }
        // Light strips: a bright core, a soft halo and their reflection on the gloss.
        for &(along_x, at, s, e, bw) in &strips {
            let (a, across) = if along_x { (fx, fy) } else { (fy, fx) };
            let da = (s - a).max(a - e).max(0.0);
            let dc = (across - at).abs();
            let dist = (da * da + dc * dc).sqrt();
            let k = ((a - s) / (e - s)).clamp(0.0, 1.0);
            let colour = mix(cool, warm, k);
            // A slot with a frosted diffuser in it, brightest down its middle.
            let half = (bw * 0.035).clamp(1.5, 3.2);
            let inside = 1.0 - smoothstep(half - 0.6, half + 0.6, dist);
            let profile = (1.0 - (dc / half).powi(2)).max(0.0) * (1.0 - smoothstep(0.0, half, da));
            let lit = scale(mix(colour, [1.0; 3], 0.4 * profile), 0.35 + 0.6 * profile);
            let halo = 0.24 * (-(dist / (bw * 0.12).max(2.5)).powi(2)).exp() + 0.08 * (-dist / (bw * 0.32).max(3.0)).exp();
            c = mix(c, lit, inside);
            c = add(c, scale(colour, halo * (1.0 - inside)));
        }
        Some((c, cover))
    });
    canvas.done(game)
}

// --- Glow -------------------------------------------------------------------------------------------

/// A thin rounded outline a few pixels off the picture, glowing softly in `rgb` (the console's
/// stripe colour), on true black.
pub(crate) fn glow(rgb_hex: u32, view_w: u32, view_h: u32, game: Rect, px: f32, b: [f32; 4]) -> Bezel {
    let _ = px;
    let mut canvas = Canvas::new(view_w, view_h);
    let pic = hole(game);
    let thin = b.iter().cloned().fold(f32::MAX, f32::min);
    let gap = (game.h as f32 * 0.011).clamp(4.0, 9.0).min((thin * 0.45).max(2.0));
    let line = grow(pic, [gap; 4]);
    // As round as possible while the arc stays clear of the picture's corners.
    let rr = ((1.414 * gap - 1.2) / 0.414).clamp(1.0, 18.0);
    let plate = grow(pic, b);
    // The stripe colours are muted for the shelf; light them up to full brightness.
    let base = rgb(rgb_hex);
    let peak = base.iter().cloned().fold(0.0, f32::max).max(0.05);
    let lit = scale(base, 1.0 / peak);
    let core = mix(lit, [1.0; 3], 0.22);
    canvas.paint(plate, game, |fx, fy| {
        let s = sd_round_rect(fx, fy, line, rr);
        let d = (s.abs() - 0.8).max(0.0);
        let stroke = (1.0 - d).clamp(0.0, 1.0);
        // Distance from the picture outward: the glow fades out before the band's end.
        let dx = (pic.0 - fx).max(fx - pic.2).max(0.0);
        let dy = (pic.1 - fy).max(fy - pic.3).max(0.0);
        let dp = (dx * dx + dy * dy).sqrt();
        let side = if fx < pic.0 {
            b[0]
        } else if fx > pic.2 {
            b[1]
        } else if fy < pic.1 {
            b[2]
        } else {
            b[3]
        };
        let fade = 1.0 - smoothstep(side * 0.55, side, dp);
        let halo = (0.40 * (-(d / 3.2).powi(2)).exp() + 0.20 * (-d / 11.0).exp()) * fade;
        let c = add(scale(core, stroke * 0.9), scale(lit, halo * (1.0 - stroke)));
        Some((c, 1.0))
    });
    canvas.done(game)
}
