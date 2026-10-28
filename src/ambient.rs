//! The Ambient screen frame: the room around the picture glows with the game's own colours, like a
//! TV's ambient light. Cheap by construction: every few frames the picture is boxed down to a tiny
//! image (48 px on its long side) and blended into a running average (temporal smoothing, so it
//! never flickers); that average is blurred on the tiny image and drawn, smoothly upscaled, dimmed
//! and a little more saturated, only on the bands around the picture.

use crate::video::Rect;

/// When to take a new sample of the picture, and how much of it goes into the running average.
#[derive(Clone, Debug)]
pub struct Cadence {
    every: u32,
    alpha: f32,
    count: u32,
    fresh: bool,
}

impl Cadence {
    pub const fn new(every: u32, alpha: f32) -> Self {
        Cadence { every, alpha, count: 0, fresh: true }
    }

    /// Called once per drawn frame: the weight of a new sample this frame, or None.
    pub fn tick(&mut self) -> Option<f32> {
        if self.fresh {
            self.fresh = false;
            self.count = 0;
            return Some(1.0);
        }
        self.count += 1;
        if self.count >= self.every.max(1) {
            self.count = 0;
            Some(self.alpha)
        } else {
            None
        }
    }

    /// The next frame takes a full sample (a new game geometry, a preview).
    pub fn reset(&mut self) {
        self.fresh = true;
    }
}

/// Every 3rd frame (20 times a second at 60 Hz), a third of the way to the new picture: settles
/// in about a fifth of a second, slow enough that flashes in the game don't strobe the border.
pub const CADENCE: Cadence = Cadence::new(3, 0.33);

/// Size of the tiny sampled picture: 48 px on its long side, the picture's shape.
pub fn tiny_size(game: Rect) -> (u32, u32) {
    const LONG: f32 = 48.0;
    let (w, h) = (game.w.max(1) as f32, game.h.max(1) as f32);
    let short = |a: f32, b: f32| ((LONG * a / b).round() as u32).max(4);
    if w >= h {
        (LONG as u32, short(h, w))
    } else {
        (short(w, h), LONG as u32)
    }
}

/// Where the blurred picture is drawn (x, y, w, h in viewport pixels): centred on the picture, in
/// its shape, just large enough to cover the whole screen, so the colour next to each edge of the
/// picture is the colour at that edge.
pub fn cover(view_w: u32, view_h: u32, game: Rect) -> (f32, f32, f32, f32) {
    let (gw, gh) = (game.w.max(1) as f32, game.h.max(1) as f32);
    let (cx, cy) = (game.x as f32 + gw / 2.0, game.y as f32 + gh / 2.0);
    // Half the size needed on each axis to reach the farther screen edge from the centre.
    let half_w = cx.max(view_w as f32 - cx);
    let half_h = cy.max(view_h as f32 - cy);
    let s = (2.0 * half_w / gw).max(2.0 * half_h / gh);
    let (w, h) = (gw * s, gh * s);
    (cx - w / 2.0, cy - h / 2.0, w, h)
}

const VS: &str = include_str!("shaders/video.vert");
pub(crate) const SAMPLE_FS: &str = include_str!("shaders/ambient_sample.frag");
pub(crate) const BLUR_FS: &str = include_str!("shaders/ambient_blur.frag");
pub(crate) const SHOW_FS: &str = include_str!("shaders/ambient_show.frag");
pub(crate) const BLIT_FS: &str = include_str!("shaders/ambient_blit.frag");

/// The composed glow is kept at a third of the screen's resolution: it is smooth throughout.
pub const GLOW_DIVISOR: u32 = 3;

use glow::HasContext;

/// A program and the uniform locations it uses, by name.
struct Program {
    program: glow::Program,
    loc: Vec<(&'static str, Option<glow::UniformLocation>)>,
}

impl Program {
    fn new(gl: &glow::Context, fs: &str, names: &[&'static str]) -> Result<Self, String> {
        let program = crate::bezel::compile(gl, VS, fs)?;
        let loc = names.iter().map(|n| (*n, unsafe { gl.get_uniform_location(program, n) })).collect();
        Ok(Program { program, loc })
    }

    fn at(&self, name: &str) -> Option<&glow::UniformLocation> {
        self.loc.iter().find(|(n, _)| *n == name).and_then(|(_, l)| l.as_ref())
    }
}

/// An image the GPU draws into: a texture and its framebuffer.
struct Target {
    size: (u32, u32),
    tex: glow::Texture,
    fbo: glow::Framebuffer,
}

impl Target {
    /// Half floats where the GPU can render to them (no banding in the slow blend of the running
    /// average and the dark glow), else 8 bits. Linear filtering, clamped edges.
    fn new(gl: &glow::Context, size: (u32, u32)) -> Option<Self> {
        let ext = gl.supported_extensions();
        let half = !gl.version().is_embedded || ext.contains("GL_EXT_color_buffer_half_float") || ext.contains("GL_EXT_color_buffer_float");
        let (internal, kind) = if half { (glow::RGBA16F, glow::HALF_FLOAT) } else { (glow::RGBA8, glow::UNSIGNED_BYTE) };
        unsafe {
            let tex = gl.create_texture().ok()?;
            gl.bind_texture(glow::TEXTURE_2D, Some(tex));
            gl.tex_image_2d(glow::TEXTURE_2D, 0, internal as i32, size.0 as i32, size.1 as i32, 0, glow::RGBA, kind, None);
            for (k, v) in [
                (glow::TEXTURE_MIN_FILTER, glow::LINEAR),
                (glow::TEXTURE_MAG_FILTER, glow::LINEAR),
                (glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE),
                (glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE),
            ] {
                gl.tex_parameter_i32(glow::TEXTURE_2D, k, v as i32);
            }
            let fbo = gl.create_framebuffer().ok()?;
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
            gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(tex), 0);
            Some(Target { size, tex, fbo })
        }
    }

    fn delete(self, gl: &glow::Context) {
        unsafe {
            gl.delete_texture(self.tex);
            gl.delete_framebuffer(self.fbo);
        }
    }
}

/// Keeps `slot` at `size`, recreating it when the size changed.
fn sized<'a>(gl: &glow::Context, slot: &'a mut Option<Target>, size: (u32, u32)) -> Option<&'a Target> {
    if slot.as_ref().is_some_and(|t| t.size != size) {
        slot.take().unwrap().delete(gl);
    }
    if slot.is_none() {
        *slot = Target::new(gl, size);
    }
    slot.as_ref()
}

struct Programs {
    sample: Program,
    blur: Program,
    show: Program,
    blit: Program,
}

/// The Ambient frame on the GPU. `update` does the work when the cadence says so (sample, blur and
/// compose the glow); `show` puts the composed glow on the bands around the picture every frame.
#[derive(Default)]
pub struct Layer {
    programs: Option<Option<Programs>>,
    /// The tiny images: the running average, half blurred, blurred.
    avg: Option<Target>,
    half: Option<Target>,
    blurred: Option<Target>,
    /// The composed glow for the whole screen, at 1 / GLOW_DIVISOR of its size.
    glow: Option<Target>,
    cadence: Option<Cadence>,
    game: Option<(Rect, u32, u32)>,
}

impl Layer {
    /// The next frame takes a full sample (for previews: no blending with an older picture).
    pub fn reset(&mut self) {
        self.cadence.get_or_insert(CADENCE).reset();
    }

    fn ready(&mut self, gl: &glow::Context) -> bool {
        if self.programs.is_none() {
            let p = (|| {
                Ok::<_, String>(Programs {
                    sample: Program::new(gl, SAMPLE_FS, &["rect", "rot", "tex", "swizzle", "fp"])?,
                    blur: Program::new(gl, BLUR_FS, &["rect", "tex", "dir", "tsize"])?,
                    show: Program::new(gl, SHOW_FS, &["rect", "tex", "tsize", "view", "game", "cover"])?,
                    blit: Program::new(gl, BLIT_FS, &["rect", "tex"])?,
                })
            })();
            if let Err(log) = &p {
                eprintln!("ambient: shader failed: {log}");
            }
            self.programs = Some(p.ok());
        }
        matches!(self.programs, Some(Some(_)))
    }

    /// When a new sample is due: boxes the game texture `src` (drawn with `rot` quarter turns) into
    /// the running average, blurs it and composes the glow for a `view_w`×`view_h` screen with the
    /// picture at `game`. Leaves `target` bound; the caller sets its viewport again.
    #[allow(clippy::too_many_arguments)]
    pub fn update(&mut self, gl: &glow::Context, target: Option<glow::Framebuffer>, view_w: u32, view_h: u32, game: Rect, src: glow::Texture, swizzle: i32, rot: u32) {
        if !self.ready(gl) {
            return;
        }
        let cadence = self.cadence.get_or_insert(CADENCE);
        if self.game != Some((game, view_w, view_h)) {
            self.game = Some((game, view_w, view_h));
            cadence.reset();
        }
        let Some(alpha) = cadence.tick() else { return };
        let size = tiny_size(game);
        let big = (view_w.div_ceil(GLOW_DIVISOR).max(1), view_h.div_ceil(GLOW_DIVISOR).max(1));
        let ok = sized(gl, &mut self.avg, size).is_some()
            && sized(gl, &mut self.half, size).is_some()
            && sized(gl, &mut self.blurred, size).is_some()
            && sized(gl, &mut self.glow, big).is_some();
        let (Some(Some(p)), Some(avg), Some(half), Some(blurred), Some(glow)) = (&self.programs, &self.avg, &self.half, &self.blurred, &self.glow) else { return };
        if !ok {
            return;
        }
        let (tw, th) = (size.0 as f32, size.1 as f32);
        unsafe {
            gl.viewport(0, 0, size.0 as i32, size.1 as i32);
            gl.active_texture(glow::TEXTURE0);
            // The picture, boxed down and blended into the running average.
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(avg.fbo));
            gl.use_program(Some(p.sample.program));
            gl.bind_texture(glow::TEXTURE_2D, Some(src));
            gl.uniform_4_f32(p.sample.at("rect"), -1.0, -1.0, 1.0, 1.0);
            gl.uniform_1_i32(p.sample.at("rot"), rot as i32);
            gl.uniform_1_i32(p.sample.at("tex"), 0);
            gl.uniform_1_i32(p.sample.at("swizzle"), swizzle);
            let (fx, fy) = if rot & 1 == 1 { (1.0 / th, 1.0 / tw) } else { (1.0 / tw, 1.0 / th) };
            gl.uniform_2_f32(p.sample.at("fp"), fx, fy);
            gl.enable(glow::BLEND);
            gl.blend_color(0.0, 0.0, 0.0, alpha);
            gl.blend_func(glow::CONSTANT_ALPHA, glow::ONE_MINUS_CONSTANT_ALPHA);
            gl.draw_arrays(glow::TRIANGLE_STRIP, 0, 4);
            gl.disable(glow::BLEND);
            // Blur: across, then down; steps of 1.6 texels for a wide, soft spread.
            gl.use_program(Some(p.blur.program));
            gl.uniform_4_f32(p.blur.at("rect"), -1.0, -1.0, 1.0, 1.0);
            gl.uniform_1_i32(p.blur.at("tex"), 0);
            gl.uniform_2_f32(p.blur.at("tsize"), tw, th);
            for (from, to, dir) in [(avg, half, (1.6 / tw, 0.0)), (half, blurred, (0.0, 1.6 / th))] {
                gl.bind_framebuffer(glow::FRAMEBUFFER, Some(to.fbo));
                gl.bind_texture(glow::TEXTURE_2D, Some(from.tex));
                gl.uniform_2_f32(p.blur.at("dir"), dir.0, dir.1);
                gl.draw_arrays(glow::TRIANGLE_STRIP, 0, 4);
            }
            // The glow for the whole screen, graded, at a third of its size.
            gl.viewport(0, 0, glow.size.0 as i32, glow.size.1 as i32);
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(glow.fbo));
            gl.use_program(Some(p.show.program));
            gl.bind_texture(glow::TEXTURE_2D, Some(blurred.tex));
            let (cx, cy, cw, ch) = cover(view_w, view_h, game);
            gl.uniform_4_f32(p.show.at("rect"), -1.0, -1.0, 1.0, 1.0);
            gl.uniform_1_i32(p.show.at("tex"), 0);
            gl.uniform_2_f32(p.show.at("tsize"), tw, th);
            gl.uniform_2_f32(p.show.at("view"), view_w as f32, view_h as f32);
            gl.uniform_4_f32(p.show.at("game"), game.x as f32, game.y as f32, (game.x + game.w) as f32, (game.y + game.h) as f32);
            gl.uniform_4_f32(p.show.at("cover"), cx, cy, cw, ch);
            gl.draw_arrays(glow::TRIANGLE_STRIP, 0, 4);
            gl.bind_framebuffer(glow::FRAMEBUFFER, target);
        }
    }

    /// Draws the composed glow on the bands around the picture (after the screen is cleared).
    pub fn show(&mut self, gl: &glow::Context, view_w: u32, view_h: u32, game: Rect) {
        let (Some(Some(p)), Some(glow)) = (&self.programs, &self.glow) else { return };
        unsafe {
            gl.use_program(Some(p.blit.program));
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, Some(glow.tex));
            gl.uniform_4_f32(p.blit.at("rect"), -1.0, -1.0, 1.0, 1.0);
            gl.uniform_1_i32(p.blit.at("tex"), 0);
        }
        crate::bezel::draw_around(gl, view_w, view_h, game);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GBA_5X: Rect = Rect { x: 20, y: 140, w: 1200, h: 800 };
    const GB_7X: Rect = Rect { x: 60, y: 36, w: 1120, h: 1008 };

    #[test]
    fn samples_at_once_then_every_third_frame() {
        let mut c = Cadence::new(3, 0.33);
        let got: Vec<Option<f32>> = (0..10).map(|_| c.tick()).collect();
        assert_eq!(got, [Some(1.0), None, None, Some(0.33), None, None, Some(0.33), None, None, Some(0.33)]);
        // A new geometry or a preview: the very next frame is a full sample, then the rhythm again.
        c.tick();
        c.reset();
        assert_eq!((c.tick(), c.tick(), c.tick(), c.tick()), (Some(1.0), None, None, Some(0.33)));
    }

    #[test]
    fn smoothing_follows_a_scene_change_without_flicker() {
        // A cut from black to white: the border follows within a fifth of a second, a step at a time.
        let mut c = CADENCE.clone();
        let mut level = 0.0f32;
        c.tick();
        let mut biggest_step = 0.0f32;
        for frame in 1..=12 {
            if let Some(a) = c.tick() {
                let next = level + (1.0 - level) * a;
                biggest_step = biggest_step.max(next - level);
                level = next;
            }
            if frame == 12 {
                assert!(level > 0.79, "after 12 frames: {level}");
            }
        }
        assert!(biggest_step < 0.34, "no jump: {biggest_step}");
        // Two thirds of the frames cost nothing beyond the final draw.
        let mut c = CADENCE.clone();
        let samples = (0..60).filter(|_| c.tick().is_some()).count();
        assert_eq!(samples, 20, "20 samples a second at 60 Hz");
    }

    #[test]
    fn the_tiny_picture_keeps_the_shape() {
        assert_eq!(tiny_size(GBA_5X), (48, 32));
        assert_eq!(tiny_size(GB_7X), (48, 43));
        assert_eq!(tiny_size(Rect { x: 0, y: 0, w: 400, h: 1080 }), (18, 48));
        assert_eq!(tiny_size(Rect { x: 0, y: 0, w: 1240, h: 10 }), (48, 4), "never thinner than 4");
    }

    #[test]
    fn the_glow_covers_the_screen_centred_on_the_picture() {
        for game in [GBA_5X, GB_7X, Rect { x: 140, y: 220, w: 960, h: 640 }, Rect { x: 300, y: 300, w: 640, h: 480 }] {
            let (x, y, w, h) = cover(1240, 1080, game);
            assert!(x <= 0.0 && y <= 0.0 && x + w >= 1240.0 && y + h >= 1080.0, "{game:?}: {x} {y} {w} {h}");
            let (cx, cy) = (game.x as f32 + game.w as f32 / 2.0, game.y as f32 + game.h as f32 / 2.0);
            assert!((x + w / 2.0 - cx).abs() < 0.01 && (y + h / 2.0 - cy).abs() < 0.01);
            assert!((w / h - game.w as f32 / game.h as f32).abs() < 0.001);
            // Just large enough: one side touches the screen's edges exactly.
            assert!((w - 1240.0).abs() < 0.01 || (h - 1080.0).abs() < 0.01, "{game:?}");
        }
        // GBA 5x: the height limits (1080 / 800 = 1.35).
        assert_eq!(cover(1240, 1080, GBA_5X), (-190.0, 0.0, 1620.0, 1080.0));
    }
}
