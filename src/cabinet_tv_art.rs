//! Portable cabinet TV body, CRT glass and composition; no file lookups or worker ownership.
use crate::cabinet_art::Pic;
use crate::image::Image;
use crate::system_art::Sprite;
use std::time::Duration;

/// The crossfade from one game's picture to the next.
pub const FADE: Duration = Duration::from_millis(220);
/// Screens kept, by game and size.
pub const KEEP: usize = 12;
/// The glass's marker colour in `tv.png`.
pub(crate) const GLASS: u32 = 0xff00ff;
/// The glass where there is no picture (yet): a dark, slightly green-grey tube.
pub const TUBE: u32 = 0x1c211e;

const PNG: &[u8] = include_bytes!("../assets/media/tv/tv.png");

/// The TV's picture as drawn: `body` with the glass marked, `glass` its bounding box (x, y, w, h).
pub struct Template {
    pub body: Sprite,
    pub glass: (i32, i32, i32, i32),
}

pub fn template() -> &'static Template {
    static T: std::sync::OnceLock<Template> = std::sync::OnceLock::new();
    T.get_or_init(|| {
        let body = crate::system_art::decode(PNG).unwrap_or(Sprite { w: 1, h: 1, px: vec![0] });
        let (mut x0, mut y0, mut x1, mut y1) = (i32::MAX, i32::MAX, -1, -1);
        for y in 0..body.h as i32 {
            for x in 0..body.w as i32 {
                if is_glass(body.px[(y * body.w as i32 + x) as usize]) {
                    (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
                }
            }
        }
        let glass = if x1 < 0 { (0, 0, 0, 0) } else { (x0, y0, x1 - x0 + 1, y1 - y0 + 1) };
        Template { body, glass }
    })
}

fn is_glass(p: u32) -> bool {
    p >> 24 != 0 && p & 0xffffff == GLASS
}

/// The TV `f` screen pixels a template pixel (nearest): its body with the glass as the dark tube, and a mask of the
/// glass (true where the screen shows) over the glass's box, scaled the same way.
pub struct Body {
    pub pic: Pic,
    /// The glass's box on `pic`.
    pub glass: (i32, i32, i32, i32),
    /// `glass.2 * glass.3` entries: whether that pixel of the box is glass.
    pub mask: Vec<bool>,
}

pub fn body(f: f32) -> Body {
    let t = template();
    let s = &t.body;
    let (w, h) = (((s.w as f32 * f).round() as i32).max(1), ((s.h as f32 * f).round() as i32).max(1));
    let mut pic = Pic::new(w, h, 0);
    let src = |x: i32, y: i32| {
        let sx = ((x as f32 / f) as u32).min(s.w - 1);
        let sy = ((y as f32 / f) as u32).min(s.h - 1);
        s.px[(sy * s.w + sx) as usize]
    };
    for y in 0..h {
        for x in 0..w {
            let p = src(x, y);
            pic.px[(y * w + x) as usize] = if is_glass(p) { 0xff00_0000 | TUBE } else { p };
        }
    }
    let (gx, gy, gw, gh) = t.glass;
    let glass = (
        (gx as f32 * f).ceil() as i32,
        (gy as f32 * f).ceil() as i32,
        ((gx + gw) as f32 * f).ceil() as i32,
        ((gy + gh) as f32 * f).ceil() as i32,
    );
    let glass = (glass.0, glass.1, (glass.2 - glass.0).max(1), (glass.3 - glass.1).max(1));
    let mut mask = vec![false; (glass.2 * glass.3) as usize];
    for y in 0..glass.3 {
        for x in 0..glass.2 {
            mask[(y * glass.2 + x) as usize] = is_glass(src(glass.0 + x, glass.1 + y));
        }
    }
    Body { pic, glass, mask }
}

/// The CRT look over `img`, `w` x `h`: the picture cut to fill the glass (its middle kept), on a slightly curved tube
/// (the corners pulled in), scanlines every second row, darker towards the rim, a warm tint and a soft glow.
pub fn crt(img: &Image, w: i32, h: i32) -> Pic {
    let (w, h) = (w.max(1), h.max(1));
    let mut out = Pic::new(w, h, 0xff00_0000 | TUBE);
    if img.w == 0 || img.h == 0 {
        return out;
    }
    // The part of the picture the glass shows: as much as fills it, from the middle.
    let (iw, ih) = (img.w as f32, img.h as f32);
    let k = (w as f32 / iw).max(h as f32 / ih);
    let (cw, ch) = (w as f32 / k, h as f32 / k);
    let (ox, oy) = ((iw - cw) / 2.0, (ih - ch) / 2.0);
    // Bilinear sample of the picture at (u, v) in 0..1 of the cut.
    let sample = |u: f32, v: f32| -> (f32, f32, f32) {
        let x = (ox + u * cw - 0.5).clamp(0.0, iw - 1.0);
        let y = (oy + v * ch - 0.5).clamp(0.0, ih - 1.0);
        let (x0, y0) = (x as u32, y as u32);
        let (x1, y1) = ((x0 + 1).min(img.w - 1), (y0 + 1).min(img.h - 1));
        let (fx, fy) = (x - x0 as f32, y - y0 as f32);
        let px = |x: u32, y: u32| img.px[(y * img.w + x) as usize];
        let ch = |p: u32, s: u32| ((p >> s) & 0xff) as f32;
        let mut c = [0.0f32; 3];
        for (i, s) in [16u32, 8, 0].into_iter().enumerate() {
            let top = ch(px(x0, y0), s) * (1.0 - fx) + ch(px(x1, y0), s) * fx;
            let bot = ch(px(x0, y1), s) * (1.0 - fx) + ch(px(x1, y1), s) * fx;
            c[i] = top * (1.0 - fy) + bot * fy;
        }
        (c[0], c[1], c[2])
    };
    // Scanlines at the picture's own scale on a big screen, every second screen row on a small one.
    let line = ((h as f32 / 120.0).round() as i32).max(1);
    let mut lit = vec![(0.0f32, 0.0f32, 0.0f32); (w * h) as usize];
    for y in 0..h {
        for x in 0..w {
            // -1..1 across the glass, then the barrel: further out is pulled in a little more.
            let (nx, ny) = (2.0 * (x as f32 + 0.5) / w as f32 - 1.0, 2.0 * (y as f32 + 0.5) / h as f32 - 1.0);
            let r2 = nx * nx + ny * ny;
            let bend = 1.0 + 0.045 * r2;
            let (bx, by) = (nx * bend, ny * bend);
            if bx.abs() > 1.0 || by.abs() > 1.0 {
                continue;
            }
            let (r, g, b) = sample((bx + 1.0) / 2.0, (by + 1.0) / 2.0);
            let scan = if (y / line) % 2 == 1 { 0.74 } else { 1.0 };
            let vignette = (1.0 - 0.30 * r2.min(2.0) / 2.0).max(0.55);
            let k = scan * vignette;
            // A warm tube: a little more red, a little less blue.
            lit[(y * w + x) as usize] = (r * k * 1.04, g * k, b * k * 0.90);
        }
    }
    // The glow: each pixel takes a fifth of its neighbours' light (a 3 x 3 box), so bright parts bleed softly.
    for y in 0..h {
        for x in 0..w {
            let (mut sr, mut sg, mut sb, mut n) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
            for (dx, dy) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
                let (xx, yy) = (x + dx, y + dy);
                if xx >= 0 && yy >= 0 && xx < w && yy < h {
                    let (r, g, b) = lit[(yy * w + xx) as usize];
                    (sr, sg, sb, n) = (sr + r, sg + g, sb + b, n + 1.0);
                }
            }
            let (r, g, b) = lit[(y * w + x) as usize];
            let glow = |c: f32, s: f32| (c * 0.85 + s / n.max(1.0) * 0.25).min(255.0) as u32;
            let tube = |c: u32, t: u32| c.max(t / 2);
            let rgb = tube(glow(r, sr), TUBE >> 16 & 0xff) << 16 | tube(glow(g, sg), TUBE >> 8 & 0xff) << 8 | tube(glow(b, sb), TUBE & 0xff);
            out.px[(y * w + x) as usize] = 0xff00_0000 | rgb;
        }
    }
    out
}

/// The screen of a game with no picture at all (or of a system's box): its console's pixel art on the dark tube,
/// through the same CRT look, so the TV never stands there blank.
pub fn console_screen(system: &str, w: i32, h: i32) -> Option<Pic> {
    let s = crate::system_art::get(system)?;
    // A 4:3 picture, the console about half its height, as a menu would show it.
    let (iw, ih) = (160u32, 120u32);
    let mut img = Image {
        w: iw,
        h: ih,
        px: vec![0x14191a; (iw * ih) as usize],
    };
    let f = (ih as f32 * 0.55 / s.h as f32).min(iw as f32 * 0.6 / s.w as f32);
    let (sw, sh) = ((s.w as f32 * f) as u32, (s.h as f32 * f) as u32);
    let (ox, oy) = ((iw - sw.min(iw)) / 2, (ih - sh.min(ih)) / 2);
    for y in 0..sh.min(ih) {
        for x in 0..sw.min(iw) {
            let p = s.px[(((y as f32 / f) as u32).min(s.h - 1) * s.w + ((x as f32 / f) as u32).min(s.w - 1)) as usize];
            if p >> 24 != 0 {
                img.px[((oy + y) * iw + ox + x) as usize] = p & 0xffffff;
            }
        }
    }
    Some(crt(&img, w, h))
}

/// The TV's size for a screen: `f` (screen pixels a template pixel) or None when there is no room for it. It stands
/// on the top board in `room` = (width beside the case out of the shelf, height of the showcase); below 60 px of glass
/// height (a 640 x 480 screen with a tall case out) it is left out rather than shown as a smudge.
pub fn scale_for(room_w: i32, room_h: i32, wanted: f32) -> Option<f32> {
    let t = template();
    let (tw, th) = (t.body.w as f32, t.body.h as f32);
    let f = wanted.min(room_w as f32 / tw).min(room_h as f32 / th);
    let glass_h = t.glass.3 as f32 * f;
    (glass_h >= 60.0).then_some(f)
}

/// Copies `screen` (the glass box's size) into the glass of `body` at `alpha` (0-255) over what is there.
pub fn show(body: &mut Pic, glass: (i32, i32, i32, i32), mask: &[bool], screen: &Pic, alpha: u32) {
    let (gx, gy, gw, gh) = glass;
    for y in 0..gh.min(screen.h) {
        for x in 0..gw.min(screen.w) {
            if !mask[(y * gw + x) as usize] {
                continue;
            }
            let s = screen.px[(y * screen.w + x) as usize];
            body.blend(gx + x, gy + y, s & 0xffffff, alpha);
        }
    }
}
