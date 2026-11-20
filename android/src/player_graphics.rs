//! GLES compositor for the Android player; native game pixels and a separately cached touch overlay.
use glow::HasContext;
use pocket_ui::{image::Image, touch::Area};
#[cfg(target_os = "android")]
#[link(name = "EGL")]
extern "C" { fn eglGetProcAddress(name: *const std::ffi::c_char) -> *const std::ffi::c_void; }
pub(super) struct Graphics {
    gl: glow::Context,
    program: glow::Program,
    solid_program: glow::Program,
    texture: glow::Texture,
    vao: glow::VertexArray,
    size: (i32, i32),
    game_texture: glow::Texture,
    game_size: (i32, i32),
    // 0xRRGGBB behind the picture and in the system-bar insets: the touch pause sheet's page colour, else black.
    pub(super) backdrop: u32,
    /// The Look as Linux's shader variant (`pause_options::look_variant`): the picture pass is pocket-core's
    /// (`video_program`, `shaders/video.frag`), not a shader of the player's own.
    pub(super) variant: pocket_ui::video_program::Variant,
    /// The console (its native lines for a 3D core's scanlines, `picture_fx::crt_lines`) and whether the picture comes
    /// from a 3D core rendering above them.
    pub(super) system: String,
    pub(super) upscaled_3d: bool,
    picture_programs: pocket_ui::video_program::Programs,
    /// The game texture's filters as last set, and that it changed since its mipmaps were made (`prepare_texture`).
    game_filters: Option<(u32, u32)>,
    game_mips_dirty: bool,
    // Linux's screen frames (`bezel::Layer`): the same frame code and the same GL layer behind the picture.
    frame: pocket_ui::bezel::Layer,
    // What the frame layer last reported (drawn or not yet), so a change is logged once: a frame that never shows must be findable.
    frame_state: Option<bool>,
}
impl Graphics {
    #[cfg(target_os = "android")]
    pub(super) fn new() -> Result<Self, String> {
        let gl = unsafe { glow::Context::from_loader_function_cstr(|name| eglGetProcAddress(name.as_ptr())) };
        Self::from_context(gl)
    }
    // Takes ownership of the frontend's current context; also used by the real GLES regression tests.
    pub(super) fn from_context(gl: glow::Context) -> Result<Self, String> {
        unsafe {
            let program = gl.create_program()?;
            for(kind,source)in[(glow::VERTEX_SHADER,"#version 300 es\nout vec2 uv;void main(){vec2 p=vec2(float((gl_VertexID<<1)&2),float(gl_VertexID&2));uv=vec2(p.x,1.0-p.y);gl_Position=vec4(p*2.0-1.0,0.0,1.0);}"),(glow::FRAGMENT_SHADER,"#version 300 es\nprecision mediump float;in vec2 uv;uniform sampler2D image;uniform bool overlay;out vec4 color;void main(){vec4 t=texture(image,uv);vec3 c=t.bgr;float a=overlay?(t.a>0.0?t.a:float(any(notEqual(c,vec3(0.0))))):1.0;color=vec4(c,a);}")] {
            let shader=gl.create_shader(kind)?;gl.shader_source(shader,source);gl.compile_shader(shader);
            if !gl.get_shader_compile_status(shader){return Err(gl.get_shader_info_log(shader));}
            gl.attach_shader(program,shader);gl.link_program(program);gl.delete_shader(shader);
        }
            if !gl.get_program_link_status(program) {
                return Err(gl.get_program_info_log(program));
            }
            let solid_program = gl.create_program()?;
            for (kind, source) in [
                (
                    glow::VERTEX_SHADER,
                    "#version 300 es\nvoid main(){vec2 p=vec2(float((gl_VertexID<<1)&2),float(gl_VertexID&2));gl_Position=vec4(p*2.0-1.0,0.0,1.0);}",
                ),
                (
                    glow::FRAGMENT_SHADER,
                    "#version 300 es\nprecision mediump float;uniform vec4 tint;out vec4 color;void main(){color=tint;}",
                ),
            ] {
                let shader = gl.create_shader(kind)?;
                gl.shader_source(shader, source);
                gl.compile_shader(shader);
                if !gl.get_shader_compile_status(shader) {
                    return Err(gl.get_shader_info_log(shader));
                }
                gl.attach_shader(solid_program, shader);
                gl.link_program(solid_program);
                gl.delete_shader(shader);
            }
            if !gl.get_program_link_status(solid_program) {
                return Err(gl.get_program_info_log(solid_program));
            }
            let texture = gl.create_texture()?;
            gl.bind_texture(glow::TEXTURE_2D, Some(texture));
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::NEAREST as i32);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::NEAREST as i32);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
            let game_texture = gl.create_texture()?;
            gl.bind_texture(glow::TEXTURE_2D, Some(game_texture));
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::NEAREST as i32);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::NEAREST as i32);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
            let picture_programs = pocket_ui::video_program::Programs::new(&gl)?;
            let vao = gl.create_vertex_array()?;
            #[cfg(target_os = "android")]
            log::info!(
                "player EGL: {} / {}",
                gl.get_parameter_string(glow::RENDERER),
                gl.get_parameter_string(glow::VERSION)
            );
            Ok(Self {
                gl,
                program,
                solid_program,
                texture,
                vao,
                size: (0, 0),
                game_texture,
                game_size: (0, 0),
                backdrop: 0,
                variant: pocket_ui::video_program::PLAIN,
                system: String::new(),
                upscaled_3d: false,
                picture_programs,
                game_filters: None,
                game_mips_dirty: true,
                frame_state: None,
                frame: {
                    // A frame that is not ready is drawn on a thread: the picture never waits for it (`Layer::set_async`).
                    let mut layer = pocket_ui::bezel::Layer::new();
                    layer.set_async(true);
                    layer
                },
            })
        }
    }
    /// A state was saved: the frame's power light flashes (frames without a light ignore it).
    #[cfg_attr(test, allow(dead_code))]
    pub(super) fn flash_frame_light(&mut self) {
        self.frame.flash_light();
    }
    /// Whether a frame still to be drawn is drawn on a thread (the player) or waited for (tests that compare it).
    #[cfg(test)]
    pub(super) fn frame_in_background(&mut self, on: bool) {
        self.frame.set_async(on);
    }
    unsafe fn upload(gl: &glow::Context, pixels: &[u32], size: (i32, i32), previous: &mut (i32, i32)) {
        // SAFETY: tightly packed u32 pixels cover exactly this texture; GLES consumes the slice synchronously.
        let bytes = unsafe { std::slice::from_raw_parts(pixels.as_ptr().cast::<u8>(), pixels.len() * 4) };
        unsafe {
            if *previous != size {
                gl.tex_image_2d(
                    glow::TEXTURE_2D,
                    0,
                    glow::RGBA8 as i32,
                    size.0,
                    size.1,
                    0,
                    glow::RGBA,
                    glow::UNSIGNED_BYTE,
                    Some(bytes),
                );
                *previous = size;
            } else {
                gl.tex_sub_image_2d(
                    glow::TEXTURE_2D,
                    0,
                    0,
                    0,
                    size.0,
                    size.1,
                    glow::RGBA,
                    glow::UNSIGNED_BYTE,
                    glow::PixelUnpackData::Slice(bytes),
                );
            }
        }
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) fn present(
        &mut self,
        pixels: &[u32],
        size: (i32, i32),
        view: (i32, i32, i32, i32),
        game: Option<(&Image, Area)>,
        dirty: bool,
        crt: &[pocket_ui::crt_on::Quad],
        frame: Option<&pocket_ui::bezel::Placed>,
        // The size and viewport the picture, its switch-on and its frame are in (`picture::Space`); the overlay is in
        // `size` on `view`.
        picture: ((i32, i32), (i32, i32, i32, i32)),
    ) {
        let backdrop = self.backdrop;
        self.draw_picture(backdrop, game, crt, frame, picture);
        unsafe {
            let gl = &self.gl;
            let overlay = gl.get_uniform_location(self.program, "overlay");
            gl.bind_texture(glow::TEXTURE_2D, Some(self.texture));
            if dirty || self.size != size {
                Self::upload(gl, pixels, size, &mut self.size);
            }
            gl.uniform_1_i32(overlay.as_ref(), i32::from(game.is_some()));
            gl.viewport(view.0, view.1, view.2, view.3);
            gl.draw_arrays(glow::TRIANGLES, 0, 3);
        }
    }
    /// The picture as the game shows it: the cleared screen in `backdrop`, the frame around it, the game with its look
    /// (`variant`), the frame's inlay, the switch-on. Returns whether a frame asked for was drawn (one still being made on its
    /// thread is not).
    fn draw_picture(
        &mut self,
        backdrop: u32,
        game: Option<(&Image, Area)>,
        crt: &[pocket_ui::crt_on::Quad],
        frame: Option<&pocket_ui::bezel::Placed>,
        picture: ((i32, i32), (i32, i32, i32, i32)),
    ) -> bool {
        let (pic_size, pic_view) = picture;
        let mut frame_drawn = true;
        unsafe {
            let gl = &self.gl;
            let channel = |shift: u32| ((backdrop >> shift) & 0xff) as f32 / 255.0;
            gl.clear_color(channel(16), channel(8), channel(0), 1.0);
            gl.clear(glow::COLOR_BUFFER_BIT);
            gl.enable(glow::BLEND);
            gl.blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);
            gl.use_program(Some(self.program));
            gl.bind_vertex_array(Some(self.vao));
            gl.active_texture(glow::TEXTURE0);
            // The frame first, on the cleared screen: it draws only the bands around the picture, never a pixel of it, and
            // never outside its room, which lies inside `view` (the system bars' insets stay clear).
            if let Some(f) = frame {
                let (x, y, w, h) = f.room;
                gl.viewport(pic_view.0 + x, pic_view.1 + y, w as i32, h as i32);
                self.frame.set_origin(pic_view.0 + x, pic_view.1 + y);
                let drawn = self.frame.draw(gl, w, h, f.hole, f.px, Some(f.style));
                frame_drawn = drawn;
                if self.frame_state != Some(drawn) {
                    self.frame_state = Some(drawn);
                    #[cfg(target_os = "android")]
                    log::info!("player frame: drawn={drawn} style={:?} room={:?} hole={:?} px={:.2} gl_error={:#x} shader_error={:?}", f.style, f.room, f.hole, f.px, gl.get_error(), self.frame.shader_error);
                }
                gl.use_program(Some(self.program));
                gl.active_texture(glow::TEXTURE0);
            }
            if let Some((image, rect)) = game {
                // Upload the core's native framebuffer, not a CPU enlargement of it. The phone's
                // physical resolution therefore does not multiply the per-frame copy or CPU blit cost.
                gl.bind_texture(glow::TEXTURE_2D, Some(self.game_texture));
                Self::upload(gl, &image.px, (image.w as i32, image.h as i32), &mut self.game_size);
                self.game_mips_dirty = true;
                pocket_ui::video_program::prepare_texture(gl, self.variant, &mut self.game_filters, &mut self.game_mips_dirty);
                // The same arithmetic as `picture::Space::on_window`.
                let (x, y, w, h) = (
                    pic_view.0 + rect.x * pic_view.2 / pic_size.0,
                    pic_view.1 + pic_view.3 - (rect.y + rect.h) * pic_view.3 / pic_size.1,
                    rect.w * pic_view.2 / pic_size.0,
                    rect.h * pic_view.3 / pic_size.1,
                );
                gl.viewport(x, y, w, h);
                // pocket-core's picture pass over the whole viewport: the texture holds the player's 0xRRGGBB words as
                // RGBA bytes, so B, G, R (Linux's XRGB8888 upload).
                let (iw, ih) = (image.w.max(1) as f32, image.h.max(1) as f32);
                let lines = pocket_ui::picture_fx::crt_lines(image.h, self.upscaled_3d, pocket_ui::picture_fx::native_lines(&self.system));
                let pass = pocket_ui::video_program::Pass { rect: [-1.0, -1.0, 1.0, 1.0], src: (iw, ih), scale: (w as f32 / iw, h as f32 / ih), turns: 0, bgr: true, lines };
                self.picture_programs.get(gl, self.variant).draw(gl, &pass);
                gl.use_program(Some(self.program));
            }
            // A frame's inlay (the DS frame's hinge in the gap between its screens) goes over the picture, as on Linux.
            if let Some(f) = frame {
                let (x, y, w, h) = f.room;
                gl.viewport(pic_view.0 + x, pic_view.1 + y, w as i32, h as i32);
                self.frame.draw_inlay(gl, w, h, f.hole);
                gl.use_program(Some(self.program));
            }
            // Shared CRT geometry over the game only; GPU quads keep the normal native-size upload path.
            if !crt.is_empty() {
                gl.use_program(Some(self.solid_program));
                let tint = gl.get_uniform_location(self.solid_program, "tint");
                for quad in crt {
                    let (x, y, w, h) = quad.rect;
                    let (sx, sy) = (pic_view.2 as f32 / pic_size.0 as f32, pic_view.3 as f32 / pic_size.1 as f32);
                    let x0 = (x * sx).floor() as i32;
                    let x1 = ((x + w) * sx).ceil() as i32;
                    let y0 = (y * sy).floor() as i32;
                    let y1 = ((y + h) * sy).ceil() as i32;
                    gl.viewport(pic_view.0 + x0, pic_view.1 + pic_view.3 - y1, x1 - x0, y1 - y0);
                    let [r, g, b] = quad.rgb_f32();
                    gl.uniform_4_f32(tint.as_ref(), r, g, b, quad.alpha);
                    gl.draw_arrays(glow::TRIANGLES, 0, 3);
                }
                gl.use_program(Some(self.program));
            }
        }
        frame_drawn
    }
    /// The Picture page's live preview (Linux: pocket-core's `preview`, `preview::from_screen`): the game drawn as it is
    /// played with the Picture choices as they are now (`variant`, `frame`, the place in `picture`), offscreen at the
    /// window's size, read back as RGBA rows top to bottom of the `view` (the window less the system bars). None when
    /// GLES can't make the offscreen picture; the bool says whether a frame asked for was drawn (one still being made on
    /// its thread is not yet: ask again).
    #[cfg_attr(test, allow(dead_code))]
    pub(super) fn render_preview(
        &mut self,
        window: (i32, i32),
        view: (i32, i32, i32, i32),
        game: (&Image, Area),
        frame: Option<&pocket_ui::bezel::Placed>,
        picture: ((i32, i32), (i32, i32, i32, i32)),
    ) -> Option<(Vec<u8>, bool)> {
        let (ww, wh) = (window.0.max(1), window.1.max(1));
        let (vx, vy, vw, vh) = view;
        if vw <= 0 || vh <= 0 {
            return None;
        }
        unsafe {
            let fbo = self.gl.create_framebuffer().ok()?;
            let Ok(target) = self.gl.create_texture() else {
                self.gl.delete_framebuffer(fbo);
                return None;
            };
            self.gl.bind_texture(glow::TEXTURE_2D, Some(target));
            self.gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::RGBA8 as i32, ww, wh, 0, glow::RGBA, glow::UNSIGNED_BYTE, None);
            self.gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::NEAREST as i32);
            self.gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::NEAREST as i32);
            self.gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
            self.gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(target), 0);
            let complete = self.gl.check_framebuffer_status(glow::FRAMEBUFFER) == glow::FRAMEBUFFER_COMPLETE;
            let mut out = None;
            if complete {
                let drawn = self.draw_picture(0, Some(game), &[], frame, picture);
                let mut rgba = vec![0u8; (vw * vh * 4) as usize];
                self.gl.read_pixels(vx, vy, vw, vh, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelPackData::Slice(&mut rgba));
                // GL's rows go bottom to top; the preview's top to bottom.
                let row = (vw * 4) as usize;
                let flipped: Vec<u8> = rgba.chunks_exact(row).rev().flatten().copied().collect();
                out = Some((flipped, drawn));
            }
            self.gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            self.gl.delete_framebuffer(fbo);
            self.gl.delete_texture(target);
            out
        }
    }
}
impl Drop for Graphics {
    fn drop(&mut self) {
        unsafe {
            self.gl.delete_texture(self.texture);
            self.gl.delete_texture(self.game_texture);
            self.gl.delete_program(self.program);
            self.gl.delete_program(self.solid_program);
            self.gl.delete_vertex_array(self.vao);
        }
    }
}
