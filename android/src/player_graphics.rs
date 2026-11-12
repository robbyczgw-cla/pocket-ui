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
    pub(super) effect: usize,
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
            for(kind,source)in[(glow::VERTEX_SHADER,"#version 300 es\nout vec2 uv;void main(){vec2 p=vec2(float((gl_VertexID<<1)&2),float(gl_VertexID&2));uv=vec2(p.x,1.0-p.y);gl_Position=vec4(p*2.0-1.0,0.0,1.0);}"),(glow::FRAGMENT_SHADER,"#version 300 es\nprecision mediump float;in vec2 uv;uniform sampler2D image;uniform bool overlay;uniform int effect;uniform float sourceHeight;out vec4 color;void main(){vec4 t=texture(image,uv);vec3 c=t.bgr;if(!overlay){if(effect==2||effect==3){float line=sin(uv.y*sourceHeight*3.14159265*2.0);c*=0.86+0.14*line;}if(effect==2){vec2 q=uv*2.0-1.0;c*=1.0-0.16*dot(q,q);c=pow(c,vec3(0.94));}}float a=overlay?(t.a>0.0?t.a:float(any(notEqual(c,vec3(0.0))))):1.0;color=vec4(c,a);}")] {
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
                effect: 0,
            })
        }
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
    ) {
        unsafe {
            let gl = &self.gl;
            let channel = |shift: u32| ((self.backdrop >> shift) & 0xff) as f32 / 255.0;
            gl.clear_color(channel(16), channel(8), channel(0), 1.0);
            gl.clear(glow::COLOR_BUFFER_BIT);
            gl.enable(glow::BLEND);
            gl.blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);
            gl.use_program(Some(self.program));
            gl.bind_vertex_array(Some(self.vao));
            gl.active_texture(glow::TEXTURE0);
            let overlay = gl.get_uniform_location(self.program, "overlay");
            if let Some((image, rect)) = game {
                // Upload the core's native framebuffer, not a CPU enlargement of it. The phone's
                // physical resolution therefore does not multiply the per-frame copy or CPU blit cost.
                gl.bind_texture(glow::TEXTURE_2D, Some(self.game_texture));
                Self::upload(gl, &image.px, (image.w as i32, image.h as i32), &mut self.game_size);
                let filter = if matches!(self.effect, 1 | 2) { glow::LINEAR } else { glow::NEAREST };
                gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, filter as i32);
                gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, filter as i32);
                gl.uniform_1_i32(gl.get_uniform_location(self.program, "effect").as_ref(), self.effect as i32);
                gl.uniform_1_f32(gl.get_uniform_location(self.program, "sourceHeight").as_ref(), image.h as f32);
                gl.uniform_1_i32(overlay.as_ref(), 0);
                gl.viewport(
                    view.0 + rect.x * view.2 / size.0,
                    view.1 + view.3 - (rect.y + rect.h) * view.3 / size.1,
                    rect.w * view.2 / size.0,
                    rect.h * view.3 / size.1,
                );
                gl.draw_arrays(glow::TRIANGLES, 0, 3);
            }
            // Shared CRT geometry over the game only; GPU quads keep the normal native-size upload path.
            if !crt.is_empty() {
                gl.use_program(Some(self.solid_program));
                let tint = gl.get_uniform_location(self.solid_program, "tint");
                for quad in crt {
                    let (x, y, w, h) = quad.rect;
                    let (sx, sy) = (view.2 as f32 / size.0 as f32, view.3 as f32 / size.1 as f32);
                    let x0 = (x * sx).floor() as i32;
                    let x1 = ((x + w) * sx).ceil() as i32;
                    let y0 = (y * sy).floor() as i32;
                    let y1 = ((y + h) * sy).ceil() as i32;
                    gl.viewport(view.0 + x0, view.1 + view.3 - y1, x1 - x0, y1 - y0);
                    let value = if quad.white { 1.0 } else { 0.0 };
                    gl.uniform_4_f32(tint.as_ref(), value, value, value, quad.alpha);
                    gl.draw_arrays(glow::TRIANGLES, 0, 3);
                }
                gl.use_program(Some(self.program));
            }
            // Picture effects never filter the pause sheet or touch controls.
            gl.uniform_1_i32(gl.get_uniform_location(self.program, "effect").as_ref(), 0);
            gl.bind_texture(glow::TEXTURE_2D, Some(self.texture));
            if dirty || self.size != size {
                Self::upload(gl, pixels, size, &mut self.size);
            }
            gl.uniform_1_i32(overlay.as_ref(), i32::from(game.is_some()));
            gl.viewport(view.0, view.1, view.2, view.3);
            gl.draw_arrays(glow::TRIANGLES, 0, 3);
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
