//! The game picture's shader, shared by both players: pocket-core's renderer on Linux (`video::Renderer`) and Android's
//! player (`android/src/player_graphics.rs`). One quad, one fragment pass (`shaders/video.frag`), a compile-time variant per
//! sharpness, effect and the GBA screen's colours, the effects' constants from `picture_fx`; the texture filters and
//! mipmaps a variant needs; the uniforms set the same way. A Look draws the same on a ROCKNIX card and in the APK.
//!
//! Until 18.11.2026 Android's player had a one-line shader of its own (Old TV and Scanlines as a sine, Smooth as plain
//! bilinear, no LCD grid or Bloom: `scratch/android-shared-audit/REPORT.md` #1).

use std::collections::HashMap;

use glow::HasContext;

use crate::video_basics::glsl_for;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum Sharpness {
    /// Nearest neighbour.
    #[default]
    Sharp,
    /// Sharp bilinear: flat texels, one output pixel of blending at their edges, in linear light.
    Crisp,
    /// Plain bilinear.
    Soft,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum Effect {
    #[default]
    None,
    Grid,
    Scanlines,
    Crt,
    /// A halo around bright parts, the picture otherwise clear (`SoftGlow` in the shader: EFFECT 4).
    SoftGlow,
    /// Rounded stair-steps on diagonal edges, flat pixel art left sharp (EFFECT 5).
    Smooth,
}

impl Sharpness {
    pub fn parse(s: &str) -> Self {
        match s {
            "crisp" => Self::Crisp,
            "soft" => Self::Soft,
            _ => Self::Sharp,
        }
    }
}

impl Effect {
    pub fn parse(s: &str) -> Self {
        match s {
            "grid" => Self::Grid,
            "scanlines" => Self::Scanlines,
            "crt" => Self::Crt,
            "softglow" => Self::SoftGlow,
            "smooth" => Self::Smooth,
            _ => Self::None,
        }
    }
}

pub(crate) const VS: &str = include_str!("shaders/video.vert");
pub(crate) const FS: &str = include_str!("shaders/video.frag");

/// Inserts `defines` after the `#version` line, which must stay first.
pub(crate) fn with_defines(src: &str, defines: &str) -> String {
    let (version, rest) = src.split_once('\n').unwrap_or((src, ""));
    format!("{version}\n{defines}{rest}")
}

/// Variant key: (sharpness, effect, the GBA screen's colours).
pub type Variant = (Sharpness, Effect, bool);
pub const PLAIN: Variant = (Sharpness::Sharp, Effect::None, false);

/// Every variant the shader has: sharpness × effect, and the LCD grid with the GBA's colours.
pub fn all_variants() -> Vec<Variant> {
    let mut v = Vec::new();
    for sharpness in [Sharpness::Sharp, Sharpness::Crisp, Sharpness::Soft] {
        for effect in [Effect::None, Effect::Grid, Effect::Scanlines, Effect::Crt, Effect::SoftGlow, Effect::Smooth] {
            v.push((sharpness, effect, false));
            if effect == Effect::Grid {
                v.push((sharpness, effect, true));
            }
        }
    }
    v
}

/// Texture filters a variant needs: (magnification, minification).
pub fn filters((sharpness, effect, _): Variant) -> (u32, u32) {
    // Scanlines and CRT steer the texture unit across a line (crisp) and sample between rows for a 3D core's
    // groups of rows; the CRT's glow reads the mipmaps.
    let mag = if sharpness != Sharpness::Sharp || matches!(effect, Effect::Scanlines | Effect::Crt) { glow::LINEAR } else { glow::NEAREST };
    let min = if effect == Effect::Crt {
        glow::LINEAR_MIPMAP_NEAREST
    } else if effect == Effect::SoftGlow {
        glow::LINEAR_MIPMAP_LINEAR // a fractional level blends two mip levels: a smooth halo from one fetch
    } else {
        mag
    };
    (mag, min)
}

/// The `#define`s of a variant, then the effects' constants (`picture_fx::defines`).
pub(crate) fn variant_defines((sharpness, effect, lcd_colour): Variant) -> String {
    let sharpness = match sharpness {
        Sharpness::Sharp => 0,
        Sharpness::Crisp => 1,
        Sharpness::Soft => 2,
    };
    let mut d = format!("#define SHARPNESS {sharpness}\n#define EFFECT {}\n", effect as u8);
    if lcd_colour {
        d.push_str("#define LCD_COLOUR\n");
    }
    d + &crate::picture_fx::defines()
}

/// A compiled variant and its uniform locations.
pub struct Program {
    pub program: glow::Program,
    rect: Option<glow::UniformLocation>,
    src: Option<glow::UniformLocation>,
    scale: Option<glow::UniformLocation>,
    swizzle: Option<glow::UniformLocation>,
    tex: Option<glow::UniformLocation>,
    rot: Option<glow::UniformLocation>,
    lines: Option<glow::UniformLocation>,
}

/// What one picture pass draws: where (NDC, `rect` = x0, y0, x1, y1), the texture's size, output pixels per texel
/// along the texture's own x and y, the quarter turns, whether the texture holds XRGB8888 as RGBA (B, G, R bytes), and
/// the scanlines' count (`picture_fx::crt_lines`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pass {
    pub rect: [f32; 4],
    pub src: (f32, f32),
    pub scale: (f32, f32),
    pub turns: u32,
    pub bgr: bool,
    pub lines: f32,
}

impl Program {
    pub fn compile(gl: &glow::Context, variant: Variant) -> Result<Program, String> {
        let defines = variant_defines(variant);
        unsafe {
            let program = gl.create_program()?;
            let mut shaders = Vec::new();
            for (kind, src) in [(glow::VERTEX_SHADER, VS.to_string()), (glow::FRAGMENT_SHADER, with_defines(FS, &defines))] {
                let src = glsl_for(gl, &src);
                let sh = gl.create_shader(kind)?;
                gl.shader_source(sh, &src);
                gl.compile_shader(sh);
                if !gl.get_shader_compile_status(sh) {
                    let log = gl.get_shader_info_log(sh);
                    gl.delete_shader(sh);
                    gl.delete_program(program);
                    return Err(log);
                }
                gl.attach_shader(program, sh);
                shaders.push(sh);
            }
            gl.link_program(program);
            let linked = gl.get_program_link_status(program);
            for sh in shaders {
                gl.detach_shader(program, sh);
                gl.delete_shader(sh);
            }
            if !linked {
                let log = gl.get_program_info_log(program);
                gl.delete_program(program);
                return Err(log);
            }
            let loc = |name: &str| gl.get_uniform_location(program, name);
            Ok(Program { program, rect: loc("rect"), src: loc("src"), scale: loc("scale"), swizzle: loc("swizzle"), tex: loc("tex"), rot: loc("rot"), lines: loc("lines") })
        }
    }

    /// Draws one pass with the texture bound to unit 0 (the caller binds it, sets the viewport and filters).
    pub fn draw(&self, gl: &glow::Context, p: &Pass) {
        unsafe {
            gl.use_program(Some(self.program));
            gl.uniform_4_f32(self.rect.as_ref(), p.rect[0], p.rect[1], p.rect[2], p.rect[3]);
            gl.uniform_1_i32(self.tex.as_ref(), 0);
            gl.uniform_2_f32(self.src.as_ref(), p.src.0, p.src.1);
            gl.uniform_2_f32(self.scale.as_ref(), p.scale.0, p.scale.1);
            gl.uniform_1_i32(self.rot.as_ref(), (p.turns & 3) as i32);
            gl.uniform_1_i32(self.swizzle.as_ref(), i32::from(p.bgr));
            gl.uniform_1_f32(self.lines.as_ref(), p.lines);
            gl.draw_arrays(glow::TRIANGLE_STRIP, 0, 4);
        }
    }
}

/// The variants compiled so far: compiled on first use (when the menu applies a change, not per frame); one the driver
/// rejects falls back to the plain one (Crisp then shows as Sharp).
pub struct Programs {
    map: HashMap<Variant, Option<Program>>,
}

impl Programs {
    /// Compiles the plain variant; an error carries the shader log.
    pub fn new(gl: &glow::Context) -> Result<Self, String> {
        let mut map = HashMap::new();
        map.insert(PLAIN, Some(Program::compile(gl, PLAIN)?));
        Ok(Programs { map })
    }

    /// Compiles `variant` if it isn't yet; a failure is logged once and remembered.
    pub fn warm(&mut self, gl: &glow::Context, variant: Variant) {
        if !self.map.contains_key(&variant) {
            let compiled = Program::compile(gl, variant);
            if let Err(log) = &compiled {
                eprintln!("video: shader variant {variant:?} failed, using the plain one: {log}");
            }
            self.map.insert(variant, compiled.ok());
        }
    }

    /// The program for `variant`, compiled now if it isn't yet.
    pub fn get(&mut self, gl: &glow::Context, variant: Variant) -> &Program {
        self.warm(gl, variant);
        match self.map.get(&variant) {
            Some(Some(_)) => self.map[&variant].as_ref().unwrap(),
            _ => self.map[&PLAIN].as_ref().expect("the plain variant"),
        }
    }

    /// Compiles every variant that isn't yet, for tests: the number, or the first one the driver rejects with its log.
    pub fn check_all(&mut self, gl: &glow::Context) -> Result<usize, String> {
        let all = all_variants();
        for v in &all {
            if !matches!(self.map.get(v), Some(Some(_))) {
                let p = Program::compile(gl, *v).map_err(|log| format!("{v:?}: {log}"))?;
                self.map.insert(*v, Some(p));
            }
        }
        Ok(all.len())
    }
}

/// The bound texture's state for a variant: its filters, and fresh mipmaps where the variant reads them (CRT, Bloom).
/// `last` is the filters as last set, `mips_dirty` that the texture changed since its mipmaps were made.
pub fn prepare_texture(gl: &glow::Context, variant: Variant, last: &mut Option<(u32, u32)>, mips_dirty: &mut bool) {
    let f = filters(variant);
    unsafe {
        if matches!(f.1, glow::LINEAR_MIPMAP_NEAREST | glow::LINEAR_MIPMAP_LINEAR) && *mips_dirty {
            gl.generate_mipmap(glow::TEXTURE_2D);
            *mips_dirty = false;
        }
        if *last != Some(f) {
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, f.0 as i32);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, f.1 as i32);
            *last = Some(f);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defines_go_after_the_version_line() {
        let s = with_defines("#version 300 es\nvoid main() {}", "#define A 1\n");
        assert!(s.starts_with("#version 300 es\n#define A 1\n"));
    }

    #[test]
    fn every_variant_is_listed_once() {
        let v = all_variants();
        assert_eq!(v.len(), 3 * 6 + 3);
        assert!(v.contains(&PLAIN));
        assert!(v.iter().filter(|x| x.2).all(|x| x.1 == Effect::Grid), "the GBA's colours with the LCD grid only");
    }
}
