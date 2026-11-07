//! The Brick's GPU through its vendor's EGL (docs/TRIMUI-BRICK.md, "The GPU"). The system's `libEGL` and `libGLESv2` are opened by
//! name on the device and never shipped. `Egl` is the context on a window kind; `Presenter` puts a whole client picture on the
//! screen through a texture: what `pocket-fb --gl` does instead of copying to fb0, and what `brick-egl --panel` measures.
//!
//! PVR's Null WSI draws straight into pages of fb0 and pans between them (read 2026-11-02: `fb0/pan` moves from 0,0 to 0,768 during a
//! run, the visible page holds the GL picture), so a swap is a real flip without a copy. Only behind the `brick` feature.

use std::ffi::{c_void, CStr, CString};

use glow::HasContext;
use libloading::Library;

type Handle = *mut c_void;
const EGL_NONE: i32 = 0x3038;
const EGL_SURFACE_TYPE: i32 = 0x3033;
const EGL_RENDERABLE_TYPE: i32 = 0x3040;
const EGL_RED_SIZE: i32 = 0x3024;
const EGL_GREEN_SIZE: i32 = 0x3023;
const EGL_BLUE_SIZE: i32 = 0x3022;
const EGL_ALPHA_SIZE: i32 = 0x3021;
const EGL_WIDTH: i32 = 0x3057;
const EGL_HEIGHT: i32 = 0x3056;
const EGL_WINDOW_BIT: i32 = 4;
const EGL_PBUFFER_BIT: i32 = 1;
const EGL_OPENGL_ES2_BIT: i32 = 4;
const EGL_OPENGL_ES3_BIT: i32 = 0x40;
const EGL_CONTEXT_CLIENT_VERSION: i32 = 0x3098;
const EGL_OPENGL_ES_API: u32 = 0x30A0;
const EGL_QUERIES: [(&str, i32); 4] = [("EGL_VENDOR", 0x3053), ("EGL_VERSION", 0x3054), ("EGL_CLIENT_APIS", 0x308D), ("EGL_EXTENSIONS", 0x3055)];

/// `libEGL.so.1`, or what `BRICK_EGL_LIB` names (the test of a system without the library).
fn egl_library() -> String {
    std::env::var("BRICK_EGL_LIB").unwrap_or_else(|_| "libEGL.so.1".into())
}

#[repr(C)]
struct FbdevWindow {
    width: u16,
    height: u16,
}

/// Where the set-up stopped: the step, the exit code `brick-egl` ends with, what the system said.
#[derive(Debug)]
pub struct Failure {
    pub step: String,
    pub code: i32,
    pub detail: String,
}

impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.step, self.detail)
    }
}

fn failure(step: &str, code: i32, detail: impl std::fmt::Display) -> Failure {
    Failure { step: step.into(), code, detail: detail.to_string() }
}

/// One symbol of a library, as the function type the caller says. A symbol that's missing is a failure with its name.
macro_rules! sym {
    ($lib:expr, $name:literal, $ty:ty) => {{
        // SAFETY: the type is EGL's own signature for this name.
        *unsafe { $lib.get::<$ty>(concat!($name, "\0").as_bytes()) }.map_err(|e| failure($name, 11, e))?
    }};
}

/// An EGL display, context and surface, current on this thread until dropped.
pub struct Egl {
    pub gl: glow::Context,
    /// False when only GLES 2 was to be had.
    pub es3: bool,
    display: Handle,
    surface: Handle,
    context: Handle,
    swap: unsafe extern "C" fn(Handle, Handle) -> u32,
    get_error: unsafe extern "C" fn() -> i32,
    make_current: unsafe extern "C" fn(Handle, Handle, Handle, Handle) -> u32,
    destroy_surface: unsafe extern "C" fn(Handle, Handle) -> u32,
    destroy_context: unsafe extern "C" fn(Handle, Handle) -> u32,
    terminate: unsafe extern "C" fn(Handle) -> u32,
    get_proc: unsafe extern "C" fn(*const std::ffi::c_char) -> *const c_void,
    // Last: the function pointers above live in these.
    _native: Box<FbdevWindow>,
    _egl: Library,
    _gles: Library,
}

impl Egl {
    /// `window`: `null` (PVR's Null WSI, the screen), `fbdev` (`{ u16 width, u16 height }`, refused by this blob) or `pbuffer` (off
    /// screen, for a computer). Every step goes to `say`.
    pub fn open(window: &str, (w, h): (i32, i32), say: &mut dyn FnMut(String)) -> Result<Egl, Failure> {
        // SAFETY: loading the system's EGL and GLES; their initialisers are what any EGL program runs.
        let egl = unsafe { Library::new(egl_library()) }.map_err(|e| failure("libEGL.so.1", 10, e))?;
        let gles = unsafe { Library::new("libGLESv2.so.2") }.map_err(|e| failure("libGLESv2.so.2", 10, e))?;
        say("libraries loaded".into());

        let get_display = sym!(egl, "eglGetDisplay", unsafe extern "C" fn(*mut c_void) -> Handle);
        let initialize = sym!(egl, "eglInitialize", unsafe extern "C" fn(Handle, *mut i32, *mut i32) -> u32);
        let query = sym!(egl, "eglQueryString", unsafe extern "C" fn(Handle, i32) -> *const std::ffi::c_char);
        let bind_api = sym!(egl, "eglBindAPI", unsafe extern "C" fn(u32) -> u32);
        let choose = sym!(egl, "eglChooseConfig", unsafe extern "C" fn(Handle, *const i32, *mut Handle, i32, *mut i32) -> u32);
        let get_attrib = sym!(egl, "eglGetConfigAttrib", unsafe extern "C" fn(Handle, Handle, i32, *mut i32) -> u32);
        let create_context = sym!(egl, "eglCreateContext", unsafe extern "C" fn(Handle, Handle, Handle, *const i32) -> Handle);
        let create_window = sym!(egl, "eglCreateWindowSurface", unsafe extern "C" fn(Handle, Handle, *mut c_void, *const i32) -> Handle);
        let create_pbuffer = sym!(egl, "eglCreatePbufferSurface", unsafe extern "C" fn(Handle, Handle, *const i32) -> Handle);
        let make_current = sym!(egl, "eglMakeCurrent", unsafe extern "C" fn(Handle, Handle, Handle, Handle) -> u32);
        let swap = sym!(egl, "eglSwapBuffers", unsafe extern "C" fn(Handle, Handle) -> u32);
        let swap_interval = sym!(egl, "eglSwapInterval", unsafe extern "C" fn(Handle, i32) -> u32);
        let get_error = sym!(egl, "eglGetError", unsafe extern "C" fn() -> i32);
        let get_proc = sym!(egl, "eglGetProcAddress", unsafe extern "C" fn(*const std::ffi::c_char) -> *const c_void);
        let destroy_surface = sym!(egl, "eglDestroySurface", unsafe extern "C" fn(Handle, Handle) -> u32);
        let destroy_context = sym!(egl, "eglDestroyContext", unsafe extern "C" fn(Handle, Handle) -> u32);
        let terminate = sym!(egl, "eglTerminate", unsafe extern "C" fn(Handle) -> u32);

        let error = || format!("EGL error 0x{:x}", unsafe { get_error() });
        let null: Handle = std::ptr::null_mut();
        let mut native = Box::new(FbdevWindow { width: w as u16, height: h as u16 });
        // SAFETY: every call below is plain EGL in its documented order; each result is checked before it's used, and the
        // native window lives as long as the surface (`_native`).
        unsafe {
            let display = get_display(null);
            if display.is_null() {
                return Err(failure("eglGetDisplay", 12, error()));
            }
            let (mut major, mut minor) = (0, 0);
            if initialize(display, &mut major, &mut minor) == 0 {
                return Err(failure("eglInitialize", 13, error()));
            }
            say(format!("EGL {major}.{minor}"));
            for (name, q) in EGL_QUERIES {
                let s = query(display, q);
                if !s.is_null() {
                    say(format!("{name}: {}", CStr::from_ptr(s).to_string_lossy()));
                }
            }
            bind_api(EGL_OPENGL_ES_API);
            let kind = if window == "pbuffer" { EGL_PBUFFER_BIT } else { EGL_WINDOW_BIT };
            let attrs = |renderable: i32| [EGL_SURFACE_TYPE, kind, EGL_RENDERABLE_TYPE, renderable, EGL_RED_SIZE, 5, EGL_GREEN_SIZE, 6, EGL_BLUE_SIZE, 5, EGL_NONE];
            let mut config: Handle = null;
            let mut count = 0;
            let es3_attrs = attrs(EGL_OPENGL_ES3_BIT);
            let mut es3 = choose(display, es3_attrs.as_ptr(), &mut config, 1, &mut count) != 0 && count > 0;
            if !es3 {
                say(format!("no GLES 3 config ({}), trying GLES 2", error()));
                let es2_attrs = attrs(EGL_OPENGL_ES2_BIT);
                if choose(display, es2_attrs.as_ptr(), &mut config, 1, &mut count) == 0 || count == 0 {
                    terminate(display);
                    return Err(failure("eglChooseConfig", 14, error()));
                }
            }
            let size = |attr: i32| {
                let mut v = 0;
                get_attrib(display, config, attr, &mut v);
                v
            };
            say(format!("config R{} G{} B{} A{}", size(EGL_RED_SIZE), size(EGL_GREEN_SIZE), size(EGL_BLUE_SIZE), size(EGL_ALPHA_SIZE)));
            let mut context = create_context(display, config, null, [EGL_CONTEXT_CLIENT_VERSION, 3, EGL_NONE].as_ptr());
            if context.is_null() {
                say(format!("no GLES 3 context ({})", error()));
                es3 = false;
                context = create_context(display, config, null, [EGL_CONTEXT_CLIENT_VERSION, 2, EGL_NONE].as_ptr());
                if context.is_null() {
                    terminate(display);
                    return Err(failure("eglCreateContext", 15, error()));
                }
            }
            let surface = match window {
                "pbuffer" => create_pbuffer(display, config, [EGL_WIDTH, w, EGL_HEIGHT, h, EGL_NONE].as_ptr()),
                "fbdev" => create_window(display, config, (&mut *native as *mut FbdevWindow).cast(), [EGL_NONE].as_ptr()),
                _ => create_window(display, config, null, [EGL_NONE].as_ptr()),
            };
            if surface.is_null() {
                let detail = error();
                destroy_context(display, context);
                terminate(display);
                return Err(failure("create surface", 16, detail));
            }
            if make_current(display, surface, surface, context) == 0 {
                let detail = error();
                destroy_surface(display, surface);
                destroy_context(display, context);
                terminate(display);
                return Err(failure("eglMakeCurrent", 17, detail));
            }
            say(format!("context current on a {window} window"));
            swap_interval(display, 1);
            let gl = glow::Context::from_loader_function(|name| {
                let c = CString::new(name).unwrap();
                let p = get_proc(c.as_ptr());
                if !p.is_null() {
                    return p;
                }
                gles.get::<*const c_void>(c.as_bytes_with_nul()).map_or(std::ptr::null(), |s| *s)
            });
            for (name, v) in [("GL_VERSION", glow::VERSION), ("GL_RENDERER", glow::RENDERER), ("GL_VENDOR", glow::VENDOR), ("GL_SHADING_LANGUAGE_VERSION", glow::SHADING_LANGUAGE_VERSION)] {
                say(format!("{name}: {}", gl.get_parameter_string(v)));
            }
            Ok(Egl { gl, es3, display, surface, context, swap, get_error, make_current, destroy_surface, destroy_context, terminate, get_proc, _native: native, _egl: egl, _gles: gles })
        }
    }

    /// Another `glow::Context` on the same EGL context, for a renderer that wants to own one (`brick_picture`). They share every GL
    /// object and state; the caller keeps its own bindings straight.
    pub fn another_gl(&self) -> glow::Context {
        // SAFETY: the loader is `eglGetProcAddress` and the GLES library, both alive in `self`, which outlives the context.
        unsafe {
            glow::Context::from_loader_function(|name| {
                let c = CString::new(name).unwrap();
                let p = (self.get_proc)(c.as_ptr());
                if !p.is_null() {
                    return p;
                }
                self._gles.get::<*const c_void>(c.as_bytes_with_nul()).map_or(std::ptr::null(), |s| *s)
            })
        }
    }

    pub fn swap(&self) -> Result<(), String> {
        // SAFETY: display and surface are alive and current for as long as `self`.
        if unsafe { (self.swap)(self.display, self.surface) } == 0 {
            return Err(format!("eglSwapBuffers: EGL error 0x{:x}", unsafe { (self.get_error)() }));
        }
        Ok(())
    }
}

impl Drop for Egl {
    fn drop(&mut self) {
        let null: Handle = std::ptr::null_mut();
        // SAFETY: what `open` made, once, in the order EGL wants.
        unsafe {
            (self.make_current)(self.display, null, null, null);
            (self.destroy_surface)(self.display, self.surface);
            (self.destroy_context)(self.display, self.context);
            (self.terminate)(self.display);
        }
    }
}

/// Compiles and links, or says why not.
///
/// # Safety
/// A context is current on this thread.
pub unsafe fn program(gl: &glow::Context, vertex: &str, fragment: &str) -> Result<glow::Program, String> {
    let program = gl.create_program()?;
    for (kind, source) in [(glow::VERTEX_SHADER, vertex), (glow::FRAGMENT_SHADER, fragment)] {
        let s = gl.create_shader(kind)?;
        gl.shader_source(s, source);
        gl.compile_shader(s);
        if !gl.get_shader_compile_status(s) {
            return Err(gl.get_shader_info_log(s));
        }
        gl.attach_shader(program, s);
    }
    gl.link_program(program);
    if gl.get_program_link_status(program) {
        Ok(program)
    } else {
        Err(gl.get_program_info_log(program))
    }
}

/// One triangle over the whole viewport, from gl_VertexID. The texture's first row is the picture's top.
const VERTEX: &str = "#version 300 es\nout vec2 uv; void main() { vec2 p = vec2(gl_VertexID == 1 ? 3.0 : -1.0, gl_VertexID == 2 ? 3.0 : -1.0); uv = vec2(p.x * 0.5 + 0.5, 0.5 - p.y * 0.5); gl_Position = vec4(p, 0.0, 1.0); }";
/// wl_shm's XRGB8888 is B, G, R, X in memory, uploaded as RGBA without a conversion: the shader puts the channels back. `picture` is the
/// newest picture, slid by `shift` (a part of the screen; the edge repeats, as the CPU's `shift_row` does) and faded towards the page colour
/// `bg`; `mixing` shows it over the picture `before` (1 only the new one: a page transition; between: a crossfade between two windows).
const FRAGMENT: &str = "#version 300 es\nprecision mediump float; uniform sampler2D picture; uniform sampler2D before; uniform vec2 shift; uniform float fade; uniform vec3 bg; uniform float mixing; in vec2 uv; out vec4 colour; void main() { vec3 now = mix(texture(picture, clamp(uv - shift, 0.0, 1.0)).bgr, bg, fade); vec3 was = texture(before, uv).bgr; colour = vec4(mix(was, now, mixing), 1.0); }";

/// How a frame shows the pictures: the newest at rest is `Look::STILL`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Look {
    /// The newest picture moved by this many pixels (right and down).
    pub shift: (i32, i32),
    /// 0 the picture, 1 the page colour.
    pub fade: f32,
    /// The page colour, 0xRRGGBB.
    pub bg: u32,
    /// 1 the newest picture only, 0 the one before only.
    pub mixing: f32,
}

impl Look {
    pub const STILL: Look = Look { shift: (0, 0), fade: 0.0, bg: 0, mixing: 1.0 };
}

/// A client's XRGB8888 picture through a texture onto the surface, and the picture before it for a crossfade.
pub struct Presenter {
    pub egl: Egl,
    screen: (i32, i32),
    program: glow::Program,
    texture: glow::Texture,
    /// The picture shown before `texture` (`keep_previous`).
    previous: glow::Texture,
    /// The texture's size, what the last client sent.
    size: (i32, i32),
    previous_size: (i32, i32),
    uniforms: [Option<glow::UniformLocation>; 6],
}

impl Presenter {
    pub fn new(window: &str, screen: (i32, i32), say: &mut dyn FnMut(String)) -> Result<Presenter, Failure> {
        let egl = Egl::open(window, screen, say)?;
        if !egl.es3 {
            return Err(failure("GLES 3", 3, "only GLES 2 here"));
        }
        // SAFETY: the context of `egl` is current on this thread for as long as the Presenter lives.
        unsafe {
            let gl = &egl.gl;
            let program = program(gl, VERTEX, FRAGMENT).map_err(|e| failure("shaders", 18, e))?;
            let mut textures = [None, None];
            for t in &mut textures {
                let texture = gl.create_texture().map_err(|e| failure("texture", 19, e))?;
                gl.bind_texture(glow::TEXTURE_2D, Some(texture));
                for (what, value) in [(glow::TEXTURE_MIN_FILTER, glow::NEAREST), (glow::TEXTURE_MAG_FILTER, glow::NEAREST), (glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE), (glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE)] {
                    gl.tex_parameter_i32(glow::TEXTURE_2D, what, value as i32);
                }
                // A black pixel until a picture comes: a crossfade from nothing fades from black.
                gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::RGBA8 as i32, 1, 1, 0, glow::RGBA, glow::UNSIGNED_BYTE, Some(&[0, 0, 0, 255]));
                *t = Some(texture);
            }
            gl.use_program(Some(program));
            let uniforms = ["picture", "before", "shift", "fade", "bg", "mixing"].map(|n| gl.get_uniform_location(program, n));
            gl.uniform_1_i32(uniforms[0].as_ref(), 0);
            gl.uniform_1_i32(uniforms[1].as_ref(), 1);
            Ok(Presenter { egl, screen, program, texture: textures[0].unwrap(), previous: textures[1].unwrap(), size: (1, 1), previous_size: (1, 1), uniforms })
        }
    }

    /// The picture on show becomes the one before; the next upload goes into the other texture.
    pub fn keep_previous(&mut self) {
        std::mem::swap(&mut self.texture, &mut self.previous);
        std::mem::swap(&mut self.size, &mut self.previous_size);
    }

    /// Uploads the picture (top left, what is larger than the screen is cut). No drawing.
    pub fn upload(&mut self, pixels: &[u8], width: u32, height: u32, stride: usize) -> Result<(), String> {
        let (w, h) = ((width as i32).min(self.screen.0), (height as i32).min(self.screen.1));
        if w <= 0 || h <= 0 || stride < width as usize * 4 || stride % 4 != 0 {
            return Err(format!("a picture of {width}x{height}, {stride} bytes per row"));
        }
        if pixels.len() < stride * (h as usize - 1) + w as usize * 4 {
            return Err(format!("{} bytes for {w}x{h} at {stride} per row", pixels.len()));
        }
        // SAFETY: the context is current; the checks above keep the upload inside `pixels`.
        unsafe {
            let gl = &self.egl.gl;
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, Some(self.texture));
            gl.pixel_store_i32(glow::UNPACK_ROW_LENGTH, (stride / 4) as i32);
            if self.size != (w, h) {
                gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::RGBA8 as i32, w, h, 0, glow::RGBA, glow::UNSIGNED_BYTE, Some(pixels));
                self.size = (w, h);
            } else {
                gl.tex_sub_image_2d(glow::TEXTURE_2D, 0, 0, 0, w, h, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelUnpackData::Slice(pixels));
            }
            gl.pixel_store_i32(glow::UNPACK_ROW_LENGTH, 0);
        }
        Ok(())
    }

    /// Draws the newest picture as `look` says, black around a picture smaller than the screen. No swap.
    pub fn compose(&mut self, look: Look) -> Result<(), String> {
        let (w, h) = self.size;
        // SAFETY: the context is current.
        unsafe {
            let gl = &self.egl.gl;
            gl.use_program(Some(self.program));
            gl.active_texture(glow::TEXTURE1);
            gl.bind_texture(glow::TEXTURE_2D, Some(self.previous));
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, Some(self.texture));
            let u = &self.uniforms;
            gl.uniform_2_f32(u[2].as_ref(), look.shift.0 as f32 / w.max(1) as f32, look.shift.1 as f32 / h.max(1) as f32);
            gl.uniform_1_f32(u[3].as_ref(), look.fade);
            let c = |s: u32| ((look.bg >> s) & 0xff) as f32 / 255.0;
            gl.uniform_3_f32(u[4].as_ref(), c(16), c(8), c(0));
            gl.uniform_1_f32(u[5].as_ref(), look.mixing);
            gl.viewport(0, 0, self.screen.0, self.screen.1);
            if (w, h) != self.screen {
                gl.clear_color(0.0, 0.0, 0.0, 1.0);
                gl.clear(glow::COLOR_BUFFER_BIT);
            }
            // Top left: GL's origin is the bottom left.
            gl.viewport(0, self.screen.1 - h, w, h);
            gl.draw_arrays(glow::TRIANGLES, 0, 3);
            gl.viewport(0, 0, self.screen.0, self.screen.1);
            match gl.get_error() {
                glow::NO_ERROR => Ok(()),
                e => Err(format!("GL error 0x{e:x}")),
            }
        }
    }

    /// Upload and draw at rest, no swap (the panel test draws over it).
    pub fn draw(&mut self, pixels: &[u8], width: u32, height: u32, stride: usize) -> Result<(), String> {
        self.upload(pixels, width, height, stride)?;
        self.compose(Look::STILL)
    }

    pub fn show(&mut self, pixels: &[u8], width: u32, height: u32, stride: usize) -> Result<(), String> {
        self.draw(pixels, width, height, stride)?;
        self.egl.swap()
    }

    /// The whole surface in one colour, swapped.
    pub fn flood(&mut self, (r, g, b): (u8, u8, u8)) -> Result<(), String> {
        // SAFETY: the context is current.
        unsafe {
            let c = |v: u8| f32::from(v) / 255.0;
            self.egl.gl.clear_color(c(r), c(g), c(b), 1.0);
            self.egl.gl.clear(glow::COLOR_BUFFER_BIT);
        }
        self.egl.swap()
    }

    /// The drawn picture as XRGB8888, top row first: the test on a computer, whose pbuffer goes to no screen.
    pub fn read_xrgb(&self) -> Vec<u8> {
        let (w, h) = (self.screen.0 as usize, self.screen.1 as usize);
        let mut rgba = vec![0u8; w * h * 4];
        // SAFETY: the context is current; the buffer holds the whole surface.
        unsafe {
            self.egl.gl.read_pixels(0, 0, w as i32, h as i32, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelPackData::Slice(&mut rgba));
        }
        let mut out = Vec::with_capacity(rgba.len());
        for row in rgba.chunks_exact(w * 4).rev() {
            for p in row.chunks_exact(4) {
                out.extend_from_slice(&[p[2], p[1], p[0], 0]);
            }
        }
        out
    }
}
