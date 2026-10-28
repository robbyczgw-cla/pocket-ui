//! pocket-core's picture shaders on a real GLES 3 driver: every variant compiles and links, and what they draw at
//! 1240×1080 keeps the promises of `picture_fx` (whose CPU reference has its own unit tests). Runs on a headless
//! context (`headless_gl`, Mesa's llvmpipe on a Linux machine, whose GLSL ES compiler is as strict as the spec).
//! Without one the tests say so and pass; `POCKET_REQUIRE_GL=1` makes that a failure (a build machine without a GPU has llvmpipe).
#![cfg(target_os = "linux")]

use std::path::Path;

use pocket_ui::libretro::{Frame, PIXEL_XRGB8888};
use pocket_ui::picture_fx as fx;
use pocket_ui::video::{Renderer, VideoSettings};

const W: u32 = 1240;
const H: u32 = 1080;

fn renderer() -> Option<(Renderer, pocket_ui::headless_gl::Headless)> {
    match pocket_ui::headless_gl::context() {
        Ok((gl, h)) => Some((Renderer::new(gl).expect("the plain variant compiles"), h)),
        Err(e) if std::env::var_os("POCKET_REQUIRE_GL").is_some() => panic!("no headless GLES 3: {e}"),
        Err(e) => {
            eprintln!("SKIPPED: no headless GLES 3 context ({e}); set POCKET_REQUIRE_GL=1 to fail instead");
            None
        }
    }
}

/// A frame of `w`×`h` from a colour function (0xRRGGBB).
fn frame(w: u32, h: u32, f: impl Fn(u32, u32) -> u32) -> Frame {
    let data = (0..h).flat_map(|y| (0..w).map(move |x| (x, y))).flat_map(|(x, y)| f(x, y).to_le_bytes()).collect();
    Frame { width: w, height: h, pitch: w as usize * 4, format: PIXEL_XRGB8888, data }
}

fn settings(pairs: &[(&str, &str)]) -> VideoSettings {
    let mut s = VideoSettings::default();
    for (k, v) in pairs {
        assert!(s.set(k, v));
    }
    s
}

/// Rendered RGBA, rows top to bottom, and a pixel's channel as 0..1.
struct Shot(Vec<u8>);
impl Shot {
    fn at(&self, x: u32, y: u32) -> [f32; 3] {
        let i = ((y * W + x) * 4) as usize;
        [0, 1, 2].map(|c| self.0[i + c] as f32 / 255.0)
    }
}

fn draw(r: &mut Renderer, system: &str, f: &Frame, aspect: f32, s: &VideoSettings) -> Shot {
    r.set_frame_source(system, Path::new("/nonexistent"));
    r.upload(f);
    Shot(r.render_offscreen(W, H, aspect, s))
}

#[test]
fn every_variant_compiles_and_links() {
    let Some((mut r, h)) = renderer() else { return };
    let n = r.check_variants().unwrap_or_else(|e| panic!("on {}: {e}", h.renderer));
    assert_eq!(n, 3 * 4 + 3, "3 sharpnesses × 4 effects, and the LCD grid with GBA colours");
}

#[test]
fn sharp_and_crisp_at_whole_scales_show_the_pixels_as_they_are() {
    let Some((mut r, _h)) = renderer() else { return };
    // Game Boy 7×: every texel a 7×7 block of exactly its colour, Sharp and Crisp alike.
    let f = frame(160, 144, |x, y| ((x * 37 + y * 11) % 256 * 0x10101) ^ ((x * y) % 7 * 0x200010));
    for sharpness in ["sharp", "crisp"] {
        let shot = draw(&mut r, "gb", &f, 160.0 / 144.0, &settings(&[("sharpness", sharpness)]));
        for (tx, ty) in [(0, 0), (13, 7), (80, 72), (159, 143)] {
            let src = f.data[((ty * 160 + tx) * 4) as usize..][..3].to_vec(); // B, G, R
            for (dx, dy) in [(0, 0), (6, 0), (0, 6), (3, 3), (6, 6)] {
                let (x, y) = (60 + tx * 7 + dx, 36 + ty * 7 + dy);
                let i = ((y * W + x) * 4) as usize;
                let got = [shot.0[i + 2], shot.0[i + 1], shot.0[i]];
                for c in 0..3 {
                    assert!(got[c].abs_diff(src[c]) <= 1, "{sharpness} texel ({tx},{ty}) pixel ({x},{y}): {got:?} vs {src:?}");
                }
            }
        }
    }
}

#[test]
fn scanlines_keep_a_flat_picture_its_brightness_and_stay_even_at_fractional_scales() {
    let Some((mut r, _h)) = renderer() else { return };
    // PS1 320×240 at Aspect: 1240×930 from y 75, 3.875 output pixels a line, 31 pixels every 8 lines. A flat grey.
    let grey = 0x808080;
    let f = frame(320, 240, |_, _| grey);
    let want = fx::lin(128.0 / 255.0);
    for effect in ["scanlines", "crt"] {
        let shot = draw(&mut r, "psx", &f, 4.0 / 3.0, &settings(&[("scaling", "aspect"), ("effect", effect)]));
        // Light per 31-pixel block (eight lines) down two columns in the middle (a magenta and a green one of the
        // CRT's mask), in linear light; the first and last block left out (the picture's edges).
        let blocks: Vec<f32> = (1..29)
            .map(|b| (0..31).map(|i| (fx::lin(shot.at(620, 75 + b * 31 + i)[1]) + fx::lin(shot.at(621, 75 + b * 31 + i)[1])) / 2.0).sum::<f32>() / 31.0)
            .collect();
        for (i, l) in blocks.iter().enumerate() {
            // Within 4 % (8-bit rounding of thin dark gaps, and the CRT's mask and glow): even lines, the grey kept.
            assert!((l / want - 1.0).abs() < 0.04, "{effect}: block {i} gives {l}, the grey is {want}");
        }
        let spread = blocks.iter().cloned().fold(0.0, f32::max) - blocks.iter().cloned().fold(9.0, f32::min);
        assert!(spread / want < 0.01, "{effect}: blocks differ by {spread}");
        // And lines are there: the middle of a line is brighter than the gap between two.
        let column: Vec<f32> = (540..549).map(|y| shot.at(620, y)[1]).collect();
        let (hi, lo) = (column.iter().cloned().fold(0.0, f32::max), column.iter().cloned().fold(9.0, f32::min));
        assert!(hi - lo > 0.15, "{effect}: no visible lines in {column:?}");
    }
}

#[test]
fn scanlines_on_the_gpu_match_the_reference() {
    let Some((mut r, _h)) = renderer() else { return };
    // SNES 256×224 at Integer: 4 pixels a line. Alternating line colours, so both beams of a pixel matter.
    let rows = [0x202020u32, 0xc0c0c0, 0x606060, 0xffffff];
    let f = frame(256, 224, |_, y| rows[(y % 4) as usize]);
    let shot = draw(&mut r, "snes", &f, 4.0 / 3.0, &settings(&[("effect", "scanlines")]));
    let px = 4.0;
    for y in 92 + 40..92 + 72 {
        // Line-space position of this row's centre; lines from the picture's top at y 92.
        let ly = (y - 92) as f32 / px + 0.5 / px - 0.5;
        let k0 = ly.floor();
        let v = |k: f32| (rows[(k as i32).rem_euclid(4) as usize] & 0xff) as f32 / 255.0;
        let want = fx::enc(fx::scanline(ly, v(k0), v(k0 + 1.0), px));
        let got = shot.at(600, y)[1];
        assert!((got - want).abs() <= 2.0 / 255.0, "row {y}: GPU {got}, reference {want}");
    }
}

#[test]
fn the_lcd_grid_darkens_the_start_of_each_pixel_and_keeps_its_middle() {
    let Some((mut r, _h)) = renderer() else { return };
    // GBA 240×160 at Integer: 5×, picture at (20, 140). A flat colour.
    let f = frame(240, 160, |_, _| 0x80c060);
    let plain = draw(&mut r, "gba", &f, 1.5, &settings(&[]));
    let grid = draw(&mut r, "gba", &f, 1.5, &settings(&[("effect", "grid")]));
    let (x0, y0) = (20 + 50 * 5, 140 + 40 * 5);
    let g = |dx: u32, dy: u32| fx::lin(grid.at(x0 + dx, y0 + dy)[1]);
    let p = fx::lin(plain.at(x0 + 2, y0 + 2)[1]);
    // The gap (first column and row of each texel) is darker; the cell's middle a little brighter (the gain).
    assert!(g(0, 2) < 0.6 * p && g(2, 0) < 0.6 * p, "gap {} {} vs {p}", g(0, 2), g(2, 0));
    assert!((g(2, 2) / p - fx::LCD_GAIN).abs() < 0.03, "cell {} vs {p}", g(2, 2));
    assert_eq!(g(3, 3), g(2, 2), "the cell is flat");
    // GBA colours: white stays nearly white, a middle grey gets darker.
    let gray = frame(240, 160, |x, _| if x < 120 { 0xffffff } else { 0x808080 });
    let off = draw(&mut r, "gba", &gray, 1.5, &settings(&[("effect", "grid")]));
    let on = draw(&mut r, "gba", &gray, 1.5, &settings(&[("effect", "grid"), ("lcd_colour", "on")]));
    let (w, m) = ((20 + 10 * 5 + 2, 302), (20 + 200 * 5 + 2, 302));
    assert!(on.at(w.0, w.1)[1] > 0.95 && on.at(m.0, m.1)[1] < off.at(m.0, m.1)[1] - 0.05, "{:?} {:?}", on.at(w.0, w.1), on.at(m.0, m.1));
    // Not on other consoles, and not while the emulator corrects colours itself.
    let gbc = draw(&mut r, "gbc", &gray, 1.5, &settings(&[("effect", "grid"), ("lcd_colour", "on")]));
    let plain_gbc = draw(&mut r, "gbc", &gray, 1.5, &settings(&[("effect", "grid")]));
    assert_eq!(gbc.at(m.0, m.1), plain_gbc.at(m.0, m.1));
    let mut core = settings(&[("effect", "grid"), ("lcd_colour", "on")]);
    core.core_colour = true;
    let stepped_back = draw(&mut r, "gba", &gray, 1.5, &core);
    assert_eq!(stepped_back.at(m.0, m.1), off.at(m.0, m.1));
}

#[test]
fn the_crt_mask_alternates_by_column_and_stays_neutral() {
    let Some((mut r, _h)) = renderer() else { return };
    let f = frame(256, 224, |_, _| 0xc0c0c0);
    let shot = draw(&mut r, "snes", &f, 4.0 / 3.0, &settings(&[("effect", "crt")]));
    let (a, b) = (shot.at(600, 500), shot.at(601, 500));
    assert!(a[1] < a[0] && b[1] > b[0], "magenta then green: {a:?} {b:?}");
    let pair = [0, 1, 2].map(|c| (fx::lin(a[c]) + fx::lin(b[c])) / 2.0);
    assert!((pair[0] - pair[1]).abs() < 0.01 && (pair[1] - pair[2]).abs() < 0.01, "a pair is grey: {pair:?}");
}

#[test]
fn a_3d_core_rendering_at_twice_its_lines_gets_the_consoles_lines() {
    let Some((mut r, _h)) = renderer() else { return };
    use glow::HasContext;
    // N64 at 640×480 through the hardware path: 240 lines of 3.875 px at Aspect, not 480 of 1.94 (which fade out).
    let rgba: Vec<u8> = (0..640 * 480).flat_map(|_| [128u8, 128, 128, 255]).collect();
    r.set_frame_source("n64", Path::new("/nonexistent"));
    unsafe {
        let gl = r.gl();
        let tex = gl.create_texture().unwrap();
        gl.bind_texture(glow::TEXTURE_2D, Some(tex));
        gl.pixel_store_i32(glow::UNPACK_ROW_LENGTH, 0);
        gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 4);
        gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::RGBA8 as i32, 640, 480, 0, glow::RGBA, glow::UNSIGNED_BYTE, Some(&rgba));
        r.copy_texture(tex, 640, 480, false);
        r.gl().delete_texture(tex);
    }
    let shot = Shot(r.render_offscreen(W, H, 4.0 / 3.0, &settings(&[("scaling", "aspect"), ("effect", "scanlines")])));
    let column: Vec<f32> = (540..549).map(|y| shot.at(620, y)[1]).collect();
    let (hi, lo) = (column.iter().cloned().fold(0.0, f32::max), column.iter().cloned().fold(9.0, f32::min));
    assert!(hi - lo > 0.15, "lines at the console's 240: {column:?}");
}

/// A Game Boy frame in four colours, one per quarter: red top left, green top right, blue bottom left, white bottom
/// right.
fn quarters() -> Frame {
    frame(160, 144, |x, y| match (x < 80, y < 72) {
        (true, true) => 0xff0000,
        (false, true) => 0x00ff00,
        (true, false) => 0x0000ff,
        (false, false) => 0xffffff,
    })
}

/// The colour of a shot's pixel as 0xRRGGBB.
fn rgb(s: &Shot, x: u32, y: u32) -> u32 {
    let c = s.at(x, y).map(|v| (v * 255.0).round() as u32);
    c[0] << 16 | c[1] << 8 | c[2]
}

#[test]
fn a_turned_picture_stands_upright_for_the_pocket_held_that_way() {
    let Some((mut r, _h)) = renderer() else { return };
    let f = quarters();
    // Held turned right, the picture is drawn a quarter counter-clockwise, 6x of its 144x160 turned shape: 864x960 at
    // (188, 60). Its top (red, green) runs down the left side, green at the top.
    let right = draw(&mut r, "gb", &f, 160.0 / 144.0, &settings(&[("orientation", "right")]));
    let rect = r.placement(W, H, 160.0 / 144.0, &settings(&[("orientation", "right")])).unwrap().0;
    assert_eq!((rect.x, rect.y, rect.w, rect.h), (188, 60, 864, 960));
    let corners = |s: &Shot| [rgb(s, 200, 70), rgb(s, 1040, 70), rgb(s, 200, 1010), rgb(s, 1040, 1010)];
    assert_eq!(corners(&right), [0x00ff00, 0xffffff, 0xff0000, 0x0000ff], "top left, top right, bottom left, bottom right");
    // Turned left, the other way: red top right.
    let left = draw(&mut r, "gb", &f, 160.0 / 144.0, &settings(&[("orientation", "left")]));
    assert_eq!(corners(&left), [0x0000ff, 0xff0000, 0xffffff, 0x00ff00]);
    // Beside it the panel stays black, and every texel is a whole 6x6 block (Sharp, whole scale on both sides).
    assert_eq!((rgb(&right, 187, 500), rgb(&right, 1052, 500), rgb(&right, 600, 59)), (0, 0, 0));
    for y in 60..60 + 960 {
        let at = |x| rgb(&right, x, y);
        // The edge between red and green lies 80 texels (480 px) from the rectangle's bottom: rows 60..540 green.
        assert_eq!(at(400), if y < 540 { 0x00ff00 } else { 0xff0000 }, "row {y}");
    }
}

#[test]
fn a_cores_own_turn_and_the_pockets_add_up() {
    let Some((mut r, _h)) = renderer() else { return };
    let f = quarters();
    let plain = draw(&mut r, "gb", &f, 0.0, &settings(&[]));
    // The core turns its picture a quarter (SET_ROTATION 1); the Pocket held turned left turns it back: as it is.
    r.set_rotation(1);
    let both = draw(&mut r, "gb", &f, 0.0, &settings(&[("orientation", "left")]));
    for (x, y) in [(70, 50), (1170, 50), (70, 1030), (1170, 1030), (620, 540), (30, 540)] {
        assert_eq!(rgb(&both, x, y), rgb(&plain, x, y), "({x}, {y})");
    }
    // The core's turn alone: upright for a Pocket held as usual, green top left.
    let core = draw(&mut r, "gb", &f, 0.0, &settings(&[]));
    assert_eq!(rgb(&core, 200, 70), 0x00ff00);
    r.set_rotation(0);
}

#[test]
fn scanlines_turn_with_the_picture() {
    let Some((mut r, _h)) = renderer() else { return };
    // SNES 256x224, alternating dark and bright lines; held turned right the lines run down the panel: the colour
    // changes across a panel row and stays down a panel column.
    let f = frame(256, 224, |_, y| if y % 2 == 0 { 0x303030 } else { 0xe0e0e0 });
    let shot = draw(&mut r, "snes", &f, 4.0 / 3.0, &settings(&[("effect", "scanlines"), ("orientation", "right")]));
    let across: Vec<f32> = (600..616).map(|x| shot.at(x, 540)[1]).collect();
    let (hi, lo) = (across.iter().cloned().fold(0.0, f32::max), across.iter().cloned().fold(9.0, f32::min));
    assert!(hi - lo > 0.3, "lines across the panel: {across:?}");
    let down: Vec<f32> = (500..580).map(|y| shot.at(605, y)[1]).collect();
    let spread = down.iter().cloned().fold(0.0, f32::max) - down.iter().cloned().fold(9.0, f32::min);
    assert!(spread < 0.02, "each line is even along its length: {down:?}");
}
