//! pocket-core's picture effects without a device: draws made-up test frames (drawn here, so free to share) through
//! the real renderer (`video::Renderer`, the shaders of `src/shaders/video.frag`) on a headless GLES 3 context
//! (`headless_gl`: Mesa's llvmpipe on a Linux machine), at the panel's 1240×1080, and writes each as a PNG.
//!
//!     cargo run --release --example picture_shots -- <out-dir> [scene-or-case-prefix …]
//!     cargo run --release --example picture_shots -- --time [frames]
//!
//! Cases are named `<scene>-<scaling>-<sharpness>-<effect>`, for example `snes-native-sharp-crt`, and `…-right` for the
//! picture of a Pocket held turned right (Screen orientation); scenes: snes, gba, gb, ps1, pattern, nds, n64 (640×480
//! through the 3D cores' path). `--time` draws every effect over the SNES scene at 4×, the PS1 scene at Aspect (3.875×)
//! and the N64 one, then the SNES scene at Fullscreen upright and turned (Screen orientation: the same pixels), each pass
//! until the GPU finished, and prints the wall time per picture; on llvmpipe that is the CPU's rasteriser (its timer
//! queries say nothing), so compare effects with each other, not with the device (docs/HOST.md "Picture effects":
//! `POCKET_CORE_GPU_TIMER=1` there).

use std::path::Path;
use std::time::Instant;

use pocket_ui::libretro::{Frame, PIXEL_XRGB8888};
use pocket_ui::video::{Renderer, VideoSettings};

const VIEW: (u32, u32) = (1240, 1080);

struct Scene {
    name: &'static str,
    system: &'static str,
    w: u32,
    h: u32,
    aspect: f32,
    px: Vec<u32>,
}

impl Scene {
    fn new(name: &'static str, system: &'static str, w: u32, h: u32, aspect: f32) -> Self {
        Scene { name, system, w, h, aspect, px: vec![0; (w * h) as usize] }
    }
    fn set(&mut self, x: i32, y: i32, c: u32) {
        if x >= 0 && y >= 0 && (x as u32) < self.w && (y as u32) < self.h {
            self.px[(y as u32 * self.w + x as u32) as usize] = c;
        }
    }
    fn rect(&mut self, x: i32, y: i32, w: i32, h: i32, c: u32) {
        for yy in y..y + h {
            for xx in x..x + w {
                self.set(xx, yy, c);
            }
        }
    }
    fn text(&mut self, x: i32, y: i32, s: &str, c: u32, shadow: Option<u32>) {
        for (i, ch) in s.chars().enumerate() {
            let glyph = font8x8::legacy::BASIC_LEGACY[(ch as usize).min(127)];
            for (row, bits) in glyph.iter().enumerate() {
                for col in 0..8 {
                    if bits >> col & 1 == 1 {
                        let (px, py) = (x + i as i32 * 8 + col, y + row as i32);
                        if let Some(s) = shadow {
                            self.set(px + 1, py + 1, s);
                        }
                        self.set(px, py, c);
                    }
                }
            }
        }
    }
    /// A sprite from rows of characters, each looked up in `pal` (space: transparent).
    fn sprite(&mut self, x: i32, y: i32, rows: &[&str], pal: &[(char, u32)]) {
        for (r, line) in rows.iter().enumerate() {
            for (c, ch) in line.chars().enumerate() {
                if let Some(&(_, col)) = pal.iter().find(|(k, _)| *k == ch) {
                    self.set(x + c as i32, y + r as i32, col);
                }
            }
        }
    }
    fn frame(&self) -> Frame {
        let data = self.px.iter().flat_map(|p| p.to_le_bytes()).collect();
        Frame { width: self.w, height: self.h, pitch: self.w as usize * 4, format: PIXEL_XRGB8888, data }
    }
}

fn rgb(r: u8, g: u8, b: u8) -> u32 {
    (r as u32) << 16 | (g as u32) << 8 | b as u32
}

fn lerp(a: u32, b: u32, t: f32) -> u32 {
    let ch = |s: u32| {
        let (x, y) = ((a >> s & 0xff) as f32, (b >> s & 0xff) as f32);
        ((x + (y - x) * t).round() as u32) << s
    };
    ch(16) | ch(8) | ch(0)
}

const HERO: &[&str] = &[
    "     RRRRR      ",
    "    RRRRRRRRR   ",
    "    HHHSSKS     ",
    "   HSHSSSKSSS   ",
    "   HSHHSSSKSSS  ",
    "   HHSSSSKKKK   ",
    "     SSSSSSS    ",
    "    RRBRRR      ",
    "   RRRBRRBRRR   ",
    "  RRRRBBBBRRRR  ",
    "  SSRBYBBYBRSS  ",
    "  SSSBBBBBBSSS  ",
    "  SSBBBBBBBBSS  ",
    "    BBB  BBB    ",
    "   HHH    HHH   ",
    "  HHHH    HHHH  ",
];
const HERO_PAL: &[(char, u32)] = &[('R', 0xd82800), ('H', 0x6b3a00), ('S', 0xfca044), ('K', 0x000000), ('B', 0x2038ec), ('Y', 0xfce0a8)];

/// A 16-bit home-console scene: a banded sky with a bright sun, hills, a dark cave wall, a sprite, coins and a HUD.
fn snes() -> Scene {
    let mut s = Scene::new("snes", "snes", 256, 224, 4.0 / 3.0);
    let bands = [0x1c2a6e, 0x26408c, 0x3a5aa8, 0x5276c0, 0x6c90d4, 0x8aaae0, 0xb4bcd8, 0xe0b890, 0xf09860];
    for y in 0..150 {
        let c = bands[(y as usize * bands.len() / 150).min(bands.len() - 1)];
        s.rect(0, y, 256, 1, c);
    }
    // Sun: white core, yellow rim.
    for y in -14..=14i32 {
        for x in -14..=14i32 {
            let d = ((x * x + y * y) as f32).sqrt();
            if d <= 14.0 {
                s.set(60 + x, 70 + y, if d < 9.0 { 0xfffff0 } else { 0xffd040 });
            }
        }
    }
    // Hills.
    for x in 0..256 {
        let h = 118.0 + 14.0 * ((x as f32) / 23.0).sin() + 6.0 * ((x as f32) / 7.0).cos();
        for y in h as i32..150 {
            s.set(x, y, if y < h as i32 + 2 { 0x8a6cb0 } else { 0x4a3a78 });
        }
    }
    // Ground: grass top, dirt tiles.
    for y in 150..224 {
        for x in 0..256 {
            let c = if y < 154 {
                0x58d838
            } else if y < 157 {
                0x30a020
            } else if (x / 16 + (y - 157) / 16) % 2 == 0 {
                0x8c5420
            } else {
                0x7a4818
            };
            s.set(x, y, c);
        }
    }
    // Dark cave wall on the right: bricks in deep greys (scanlines in dark areas).
    for y in 40..157 {
        for x in 176..256 {
            let (bx, by) = ((x - 176 + if (y / 8) % 2 == 0 { 0 } else { 8 }) % 16, y % 8);
            let c = if bx == 0 || by == 0 {
                0x101014
            } else if bx < 3 && by < 3 {
                0x40404c
            } else {
                0x262630
            };
            s.set(x, y, c);
        }
    }
    s.sprite(120, 134, HERO, HERO_PAL);
    for i in 0..4 {
        let cx = 90 + i * 22;
        for (dy, w) in [(0, 2), (1, 4), (2, 6), (3, 6), (4, 6), (5, 6), (6, 4), (7, 2)] {
            s.rect(cx + 3 - w / 2, 104 + dy, w, 1, 0xf8d820);
        }
        s.rect(cx + 3, 106, 1, 4, 0xa07000);
    }
    s.text(8, 8, "MARIO-ISH  x03", 0xffffff, Some(0x000000));
    s.text(152, 8, "SCORE 012300", 0xffffff, Some(0x000000));
    s.text(192, 140, "CAVE", 0xc8c8d0, None);
    s
}

/// A Game Boy Advance RPG scene: pastel field, a house, a text box.
fn gba() -> Scene {
    let mut s = Scene::new("gba", "gba", 240, 160, 1.5);
    for y in 0..160 {
        for x in 0..240 {
            let c = if y < 40 {
                lerp(0x78b8f8, 0xc8e8ff, y as f32 / 40.0)
            } else if (x / 8 + y / 8) % 2 == 0 {
                0x68c050
            } else {
                0x60b848
            };
            s.set(x, y, c);
        }
    }
    // House.
    s.rect(140, 30, 70, 50, 0xf0e0c0);
    for i in 0..20 {
        s.rect(135 + i, 30 - i, 80 - 2 * i, 1, if i % 3 == 0 { 0xb83820 } else { 0xd84830 });
    }
    s.rect(165, 55, 16, 25, 0x704020);
    s.rect(148, 44, 12, 10, 0x50a0e0);
    s.rect(190, 44, 12, 10, 0x50a0e0);
    // Path, flowers.
    for y in 80..120 {
        s.rect(166, y, 14, 1, 0xd8c890);
    }
    for (x, y, c) in [(30, 60, 0xff5080), (50, 75, 0xffe040), (80, 58, 0xff5080), (100, 90, 0xa060ff), (40, 100, 0xffe040)] {
        s.rect(x, y, 3, 3, c);
        s.set(x + 1, y + 1, 0xffffff);
    }
    s.sprite(60, 70, HERO, HERO_PAL);
    // Text box.
    s.rect(4, 118, 232, 38, 0xf8f8f8);
    s.rect(6, 120, 228, 34, 0x284878);
    s.rect(8, 122, 224, 30, 0xf8f8f8);
    s.text(14, 126, "Welcome to the village!", 0x303030, Some(0xc0c0c0));
    s.text(14, 140, "Skin, sky and grass.", 0x303030, Some(0xc0c0c0));
    s
}

/// A Game Boy scene in the four greens.
fn gb() -> Scene {
    let pal = [0xe0f8d0, 0x88c070, 0x346856, 0x081820];
    let mut s = Scene::new("gb", "gb", 160, 144, 160.0 / 144.0);
    for y in 0..144 {
        for x in 0..160 {
            s.set(x, y, pal[0]);
        }
    }
    for x in 0..160 {
        for y in 112..144 {
            s.set(x, y, if (x / 8 + y / 8) % 2 == 0 { pal[1] } else { pal[2] });
        }
    }
    for y in 24..64 {
        s.rect(12, y, 50, 1, pal[if y % 4 == 0 { 3 } else { 2 }]);
    }
    let gp = [('R', pal[3]), ('H', pal[3]), ('S', pal[1]), ('K', pal[3]), ('B', pal[2]), ('Y', pal[0])];
    s.sprite(90, 96, HERO, &gp);
    s.text(8, 4, "LEVEL 1-1", pal[3], None);
    s.text(80, 30, "PRESS", pal[3], None);
    s.text(80, 42, "START", pal[2], None);
    s
}

/// A PlayStation-like 3D frame: Gouraud-shaded shapes, a perspective floor, the console's 4×4 dither, and text.
fn ps1() -> Scene {
    let mut s = Scene::new("ps1", "psx", 320, 240, 4.0 / 3.0);
    const DITHER: [[i32; 4]; 4] = [[-4, 0, -3, 1], [2, -2, 3, -1], [-3, 1, -4, 0], [3, -1, 2, -2]];
    let dith = |c: u32, x: i32, y: i32| -> u32 {
        let d = DITHER[(y & 3) as usize][(x & 3) as usize] * 2;
        let ch = |sh: u32| ((((c >> sh & 0xff) as i32 + d).clamp(0, 255) as u32) & 0xf8) << sh;
        ch(16) | ch(8) | ch(0)
    };
    for y in 0..240 {
        for x in 0..320 {
            let c = if y < 120 {
                lerp(0x101830, 0x5070a8, y as f32 / 120.0)
            } else {
                // Floor: checker in perspective.
                let z = 60.0 / ((y - 110) as f32);
                let u = ((x as f32 - 160.0) * z / 8.0).floor() as i32;
                let v = (z * 8.0).floor() as i32;
                let base = if (u + v) % 2 == 0 { 0x907050 } else { 0x584030 };
                lerp(base, 0x202028, (z / 6.0).min(1.0))
            };
            s.set(x, y, dith(c, x, y));
        }
    }
    // A shaded sphere and a lit box.
    for y in -40..=40i32 {
        for x in -40..=40i32 {
            let r2 = (x * x + y * y) as f32 / 1600.0;
            if r2 <= 1.0 {
                let n = (1.0 - r2).sqrt();
                let l = (0.2 + 0.8 * (n * 0.7 - x as f32 / 40.0 * 0.3 - y as f32 / 40.0 * 0.5).max(0.0)).min(1.0);
                let spec = ((n * 0.7 - x as f32 / 40.0 * 0.3 - y as f32 / 40.0 * 0.5).max(0.0)).powf(30.0);
                let c = lerp(lerp(0x000000, 0xd03020, l), 0xffffff, spec);
                s.set(110 + x, 120 + y, dith(c, 110 + x, 120 + y));
            }
        }
    }
    for y in 80..170 {
        for x in 190..270 {
            let t = (x - 190) as f32 / 80.0;
            let c = if x < 230 { lerp(0x30c0a0, 0x186050, t * 2.0) } else { lerp(0x186050, 0x0c3028, (t - 0.5) * 2.0) };
            s.set(x, y, dith(c, x, y));
        }
    }
    s.text(12, 10, "LAP 2/3", 0xffffff, Some(0x000000));
    s.text(220, 10, "0'42\"18", 0xffe040, Some(0x000000));
    s.text(12, 222, "Gouraud + dither", 0xe0e0e0, Some(0x000000));
    s
}

/// A test pattern: grey ramp, colour bars, single-pixel lines and dots, text both ways round.
fn pattern() -> Scene {
    let mut s = Scene::new("pattern", "snes", 256, 224, 4.0 / 3.0);
    for i in 0..32 {
        let v = (i * 255 / 31) as u8;
        s.rect(i * 8, 0, 8, 40, rgb(v, v, v));
    }
    let bars = [0xffffff, 0xffff00, 0x00ffff, 0x00ff00, 0xff00ff, 0xff0000, 0x0000ff, 0x000000];
    for (i, c) in bars.iter().enumerate() {
        s.rect(i as i32 * 32, 40, 32, 40, *c);
    }
    for (i, c) in bars.iter().enumerate() {
        s.rect(i as i32 * 32, 80, 32, 20, lerp(0, *c, 0.35));
    }
    s.rect(0, 100, 256, 124, 0x000000);
    for y in 104..160 {
        if y % 2 == 0 {
            s.rect(4, y, 60, 1, 0xffffff);
        }
        if y % 4 == 0 {
            s.rect(68, y, 60, 1, 0xffffff);
        }
    }
    for x in 132..192 {
        if x % 2 == 0 {
            s.rect(x, 104, 1, 56, 0xffffff);
        }
    }
    for y in (104..160).step_by(6) {
        for x in (196..252).step_by(6) {
            s.set(x, y, 0xffffff);
        }
    }
    s.text(4, 168, "White on black 0123", 0xffffff, None);
    s.rect(0, 184, 256, 40, 0xffffff);
    s.text(4, 192, "Black on white 4567", 0x000000, None);
    s.text(4, 206, "Red on white", 0xd00000, None);
    s
}

/// Two GBA-like screens stacked, as the DS core sends them (256×384).
fn nds() -> Scene {
    let top = gba();
    let mut s = Scene::new("nds", "nds", 256, 384, 256.0 / 384.0);
    for y in 0..160i32 {
        for x in 0..240i32 {
            let c = top.px[(y * 240 + x) as usize];
            s.set(x + 8, y + 16, c);
            s.set(x + 8, y + 208, lerp(c, 0x204060, 0.4));
        }
    }
    s
}

/// A 3D core rendering at twice the console's lines: the PS1 scene drawn 2× (640×480) as an N64 core would hand it
/// over (a GL texture, `Renderer::copy_texture`), so the CRT draws the console's 240 lines, not 480.
fn n64() -> Scene {
    let small = ps1();
    let mut s = Scene::new("n64", "n64", 640, 480, 4.0 / 3.0);
    for y in 0..480 {
        for x in 0..640 {
            s.set(x, y, small.px[((y / 2) * 320 + x / 2) as usize]);
        }
    }
    s
}

fn scenes() -> Vec<Scene> {
    vec![snes(), gba(), gb(), ps1(), pattern(), nds(), n64()]
}

/// Hands the scene to the renderer: uploaded like a 2D core's frame, or as a 3D core's GL texture (n64).
fn give(r: &mut Renderer, scene: &Scene) {
    r.set_frame_source(scene.system, Path::new("/nonexistent"));
    if scene.name != "n64" {
        r.upload(&scene.frame());
        return;
    }
    // RGBA bytes for a texture of our own, then the hardware-rendering path.
    let rgba: Vec<u8> = scene.px.iter().flat_map(|p| [(p >> 16) as u8, (p >> 8) as u8, *p as u8, 255]).collect();
    unsafe {
        use glow::HasContext;
        let gl = r.gl();
        let tex = gl.create_texture().unwrap();
        gl.bind_texture(glow::TEXTURE_2D, Some(tex));
        gl.pixel_store_i32(glow::UNPACK_ROW_LENGTH, 0);
        gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 4);
        gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::RGBA8 as i32, scene.w as i32, scene.h as i32, 0, glow::RGBA, glow::UNSIGNED_BYTE, Some(&rgba));
        r.copy_texture(tex, scene.w, scene.h, false);
        r.gl().delete_texture(tex);
    }
}

/// (scaling, sharpness, effect) per scene: the scales that occur on 1240×1080.
fn cases(scene: &str) -> Vec<(&'static str, &'static str, &'static str)> {
    let mut v = Vec::new();
    let effects: &[&str] = match scene {
        "gba" | "gb" | "nds" => &["none", "grid"],
        _ => &["none", "scanlines", "crt"],
    };
    let scalings: &[&str] = match scene {
        "nds" | "n64" => &["aspect"],
        "gb" | "gba" => &["native", "integer-smaller", "aspect"],
        _ => &["native", "aspect"],
    };
    for &scaling in scalings {
        for &sharp in &["sharp", "crisp"] {
            for &effect in effects {
                v.push((scaling, sharp, effect));
            }
        }
    }
    v
}

fn settings(scaling: &str, sharpness: &str, effect: &str, extra: &[(&str, &str)]) -> VideoSettings {
    let mut s = VideoSettings::default();
    s.set("scaling", scaling);
    s.set("sharpness", sharpness);
    s.set("effect", effect);
    for (k, v) in extra {
        s.set(k, v);
    }
    s
}

fn write_png(path: &Path, w: u32, h: u32, rgba: &[u8]) {
    let file = std::fs::File::create(path).expect("create png");
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().unwrap().write_image_data(rgba).unwrap();
}

fn time(frames: u32) {
    let (gl, h) = pocket_ui::headless_gl::context().unwrap_or_else(|e| panic!("no headless GL: {e}"));
    println!("renderer: {}", h.renderer);
    let mut r = Renderer::new(gl).expect("renderer");
    r.set_gpu_timer(true);
    let pass = |r: &mut Renderer, scene: &Scene, s: &VideoSettings| {
        let _ = r.render_offscreen(VIEW.0, VIEW.1, scene.aspect, s); // compile, warm up
        r.gpu_timer_reset();
        let t0 = Instant::now();
        for _ in 0..frames {
            r.draw_offscreen(VIEW.0, VIEW.1, scene.aspect, s);
            // Until the GPU is done, so each pass is timed alone and the queries come back.
            unsafe { glow::HasContext::finish(r.gl()) };
        }
        let wall = t0.elapsed().as_secs_f32() * 1000.0 / frames as f32;
        (wall, r.gpu_timer_report().unwrap_or_else(|| "no timer query".into()))
    };
    const EFFECTS: [(&str, &str); 6] = [("sharp", "none"), ("crisp", "none"), ("soft", "none"), ("sharp", "scanlines"), ("sharp", "crt"), ("crisp", "crt")];
    for (scene, scaling) in [(snes(), "native"), (ps1(), "aspect"), (n64(), "aspect")] {
        give(&mut r, &scene);
        for (sharp, effect) in EFFECTS {
            let (wall, gpu) = pass(&mut r, &scene, &settings(scaling, sharp, effect, &[]));
            println!("{:<7} {:<7} {:<6} {:<10} wall {:>6.2} ms/frame   {}", scene.name, scaling, sharp, effect, wall, gpu);
        }
    }
    // Screen orientation turns the picture in the vertex shader. At Fullscreen both cover the whole panel, so the same
    // pass over the same pixels: upright and turned should cost the same.
    let scene = snes();
    give(&mut r, &scene);
    for (sharp, effect) in EFFECTS {
        for orientation in ["normal", "right"] {
            let (wall, gpu) = pass(&mut r, &scene, &settings("fullscreen", sharp, effect, &[("orientation", orientation)]));
            println!("{:<7} {:<10} {:<6} {:<10} {:<6} wall {:>6.2} ms/frame   {}", scene.name, "fullscreen", sharp, effect, orientation, wall, gpu);
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("--time") {
        time(args.get(1).and_then(|s| s.parse().ok()).unwrap_or(60));
        return;
    }
    let Some(out) = args.first() else {
        eprintln!("usage: picture_shots <out-dir> [scene-or-case-prefix …] | --time [frames]");
        std::process::exit(2);
    };
    let filters = &args[1..];
    let out = Path::new(out);
    std::fs::create_dir_all(out).expect("out dir");
    let (gl, h) = pocket_ui::headless_gl::context().unwrap_or_else(|e| panic!("no headless GL: {e}"));
    eprintln!("renderer: {}", h.renderer);
    let mut r = Renderer::new(gl).expect("renderer");
    for scene in scenes() {
        give(&mut r, &scene);
        let mut all = cases(scene.name);
        if scene.name == "gba" {
            all.push(("native", "sharp", "grid-colour"));
        }
        // Screen orientation: the picture held turned right, plain and with the scene's line effect.
        let turned_effect = if matches!(scene.name, "gba" | "gb" | "nds") { "grid" } else { "crt" };
        all.push(("native", "sharp", "none-right"));
        all.push(("native", "sharp", if turned_effect == "grid" { "grid-right" } else { "crt-right" }));
        for (scaling, sharp, effect) in all {
            let name = format!("{}-{scaling}-{sharp}-{effect}", scene.name);
            if !filters.is_empty() && !filters.iter().any(|f| name.starts_with(f.as_str())) {
                continue;
            }
            let s = match effect {
                "grid-colour" => settings(scaling, sharp, "grid", &[("lcd_colour", "on")]),
                e if e.ends_with("-right") => settings(scaling, sharp, e.trim_end_matches("-right"), &[("orientation", "right")]),
                _ => settings(scaling, sharp, effect, &[]),
            };
            let rgba = r.render_offscreen(VIEW.0, VIEW.1, scene.aspect, &s);
            write_png(&out.join(format!("{name}.png")), VIEW.0, VIEW.1, &rgba);
            println!("{name}");
        }
    }
}
