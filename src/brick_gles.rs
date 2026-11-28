//! 3D cores on the Brick (docs/TRIMUI-BRICK.md, "3D cores"): N64, PSP and Dreamcast cores render with GLES. pocket-core on the Brick
//! draws the picture on the CPU (`soft_screen`) into a window of `pocket-fb`, which has the screen, so the core gets a GLES 3 context of
//! its own on the vendor's EGL with an off-screen surface: it renders into our framebuffer (`hw_render`'s `get_current_framebuffer`), and
//! each frame is read back into an ordinary picture. From there everything is as for a 2D core: the window, scaling, the pause menu's
//! picture, save-state previews. The context is current on pocket-core's main thread, where the core runs. The log's `[hw] … frames read
//! back` line splits the time into the read (with the wait for the GPU) and the conversion. Only behind `brick`.

use std::ffi::{c_char, c_uint, c_void};
use std::time::{Duration, Instant};

use glow::HasContext;
use libloading::Library;

use crate::brick_gl::Egl;
use crate::hw_render::{self, Api, Request};
use crate::libretro::{Frame, PIXEL_XRGB8888};

const ENV_SET_HW_RENDER: c_uint = 14;
type GetProcAddress = unsafe extern "C" fn(*const c_char) -> *const c_void;

/// The start of `struct retro_hw_render_callback`, up to the field we replace.
#[repr(C)]
struct CallbackHead {
    context_type: c_uint,
    context_reset: Option<unsafe extern "C" fn()>,
    get_current_framebuffer: Option<unsafe extern "C" fn() -> usize>,
    get_proc_address: Option<GetProcAddress>,
}

/// What `get_proc_address` hands a core: the GLES library's own symbol first (an EGL 1.4 like the Brick's need not return core
/// functions from `eglGetProcAddress`), else EGL's, which has the extensions. pocket-core also exports it as
/// `eglGetProcAddress` (`src/bin/pocket_core.rs`): cores that ask EGL directly instead of the frontend (PPSSPP's GLES 3 loader,
/// `gl3stubInit`) get the same answer. The vendor's own function is reached through its library handle, never by name.
///
/// # Safety
/// `name` is null or a NUL-terminated string.
pub unsafe extern "C" fn get_proc_address(name: *const c_char) -> *const c_void {
    static LIBS: std::sync::OnceLock<(Option<Library>, Option<(Library, GetProcAddress)>)> = std::sync::OnceLock::new();
    if name.is_null() {
        return std::ptr::null();
    }
    let (gles, egl) = LIBS.get_or_init(|| {
        let gles = Library::new("libGLESv2.so.2").ok();
        let egl = Library::new(std::env::var("BRICK_EGL_LIB").unwrap_or_else(|_| "libEGL.so.1".into())).ok().and_then(|l| {
            let f = *l.get::<GetProcAddress>(b"eglGetProcAddress").ok()?;
            Some((l, f))
        });
        (gles, egl)
    });
    let bytes = std::ffi::CStr::from_ptr(name).to_bytes_with_nul();
    if let Some(p) = gles.as_ref().and_then(|l| l.get::<*const c_void>(bytes).ok()).map(|s| *s).filter(|p| !p.is_null()) {
        return p;
    }
    egl.as_ref().map_or(std::ptr::null(), |(_, f)| f(name))
}

/// Whether new colour textures the core makes get their memory cleared (`tex_storage_2d`): the N64 core's GLideN64 (`core_loading`).
static CLEAR_NEW_TEXTURES: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// The core about to load (its id, `core_options::core_id`). For Mupen64Plus-Next the textures GLideN64 makes for the N64's pictures
/// are cleared when they are made (`tex_storage_2d`); `POCKET_CORE_BRICK_TEXZERO=0` leaves them as the driver gives them.
pub fn core_loading(id: &str) {
    let on = id == "mupen64plus_next" && std::env::var_os("POCKET_CORE_BRICK_TEXZERO").map_or(true, |v| v != "0");
    CLEAR_NEW_TEXTURES.store(on, std::sync::atomic::Ordering::Relaxed);
}

/// The cores' `glTexStorage2D` (exported by pocket-core, `src/bin/pocket_core.rs`; GLideN64 reaches it through libretro's glsm as a
/// plain symbol). GLideN64 makes a texture for each picture the N64 draws and doesn't clear it (`FrameBuffer::init`), and shows
/// what the game never draws there: the lines and columns around many N64 pictures. Desktop drivers give zeroed memory; the Brick's
/// PowerVR driver gives memory as it was, old pictures and noise, which showed in the black frame around Super Mario 64's and
/// Banjo-Kazooie's pictures (Brick, 2026-11-26). So a new colour texture of picture size is black from the start, once, when made.
///
/// # Safety
/// A GL context is current; GL's contract for the arguments.
pub unsafe extern "C" fn tex_storage_2d(target: u32, levels: i32, internal: u32, width: i32, height: i32) {
    type TexStorage2d = unsafe extern "C" fn(u32, i32, u32, i32, i32);
    type GetIntegerv = unsafe extern "C" fn(u32, *mut i32);
    type PixelStorei = unsafe extern "C" fn(u32, i32);
    type BindBuffer = unsafe extern "C" fn(u32, u32);
    type TexSubImage2d = unsafe extern "C" fn(u32, i32, i32, i32, i32, i32, u32, u32, *const c_void);
    static FNS: std::sync::OnceLock<Option<(TexStorage2d, GetIntegerv, PixelStorei, BindBuffer, TexSubImage2d)>> = std::sync::OnceLock::new();
    let fns = FNS.get_or_init(|| {
        let get = |n: &[u8]| get_proc_address(n.as_ptr().cast());
        let p = [get(b"glTexStorage2D\0"), get(b"glGetIntegerv\0"), get(b"glPixelStorei\0"), get(b"glBindBuffer\0"), get(b"glTexSubImage2D\0")];
        // SAFETY: GL's own signatures for these names.
        (!p.iter().any(|f| f.is_null())).then(|| unsafe {
            (
                std::mem::transmute::<*const c_void, TexStorage2d>(p[0]),
                std::mem::transmute::<*const c_void, GetIntegerv>(p[1]),
                std::mem::transmute::<*const c_void, PixelStorei>(p[2]),
                std::mem::transmute::<*const c_void, BindBuffer>(p[3]),
                std::mem::transmute::<*const c_void, TexSubImage2d>(p[4]),
            )
        })
    });
    let Some((storage, get_integer, pixel_store, bind_buffer, sub_image)) = *fns else { return };
    storage(target, levels, internal, width, height);
    let Some((format, kind, bytes)) = picture_clear(CLEAR_NEW_TEXTURES.load(std::sync::atomic::Ordering::Relaxed), target, internal, width, height) else {
        return;
    };
    let zeros = vec![0u8; width as usize * height as usize * bytes];
    // The unpack state as the core has it, set to plain rows from our memory for the upload and put back.
    let names = [glow::UNPACK_ALIGNMENT, glow::UNPACK_ROW_LENGTH, glow::UNPACK_SKIP_ROWS, glow::UNPACK_SKIP_PIXELS];
    let mut kept = [0i32; 4];
    for (n, k) in names.iter().zip(kept.iter_mut()) {
        get_integer(*n, k);
    }
    let mut buffer = 0i32;
    get_integer(glow::PIXEL_UNPACK_BUFFER_BINDING, &mut buffer);
    bind_buffer(glow::PIXEL_UNPACK_BUFFER, 0);
    for (n, v) in names.iter().zip([1, 0, 0, 0]) {
        pixel_store(*n, v);
    }
    sub_image(target, 0, 0, 0, width, height, format, kind, zeros.as_ptr().cast());
    for (n, v) in names.iter().zip(kept) {
        pixel_store(*n, v);
    }
    bind_buffer(glow::PIXEL_UNPACK_BUFFER, buffer as u32);
    static SAID: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    if SAID.fetch_add(1, std::sync::atomic::Ordering::Relaxed) < 6 {
        eprintln!("[hw] the core's new {width}x{height} picture texture cleared");
    }
}

/// Whether `tex_storage_2d` clears a new texture, and how (format, type, bytes a pixel for the upload of zeros): only for the core
/// that wants it (`on`), a 2D colour texture of picture size (at least 256x128: GLideN64's pictures, never its small N64 textures,
/// which it fills itself). Depth and anything else stays as the driver made it.
pub fn picture_clear(on: bool, target: u32, internal: u32, width: i32, height: i32) -> Option<(u32, u32, usize)> {
    if !on || target != glow::TEXTURE_2D || width < 256 || height < 128 {
        return None;
    }
    match internal {
        glow::RGBA8 => Some((glow::RGBA, glow::UNSIGNED_BYTE, 4)),
        glow::RGB8 => Some((glow::RGB, glow::UNSIGNED_BYTE, 3)),
        glow::RGBA4 => Some((glow::RGBA, glow::UNSIGNED_SHORT_4_4_4_4, 2)),
        glow::RGB5_A1 => Some((glow::RGBA, glow::UNSIGNED_SHORT_5_5_5_1, 2)),
        glow::RGB565 => Some((glow::RGB, glow::UNSIGNED_SHORT_5_6_5, 2)),
        _ => None,
    }
}

/// After `hw_render` answered a command (`libretro::environment`): a granted `SET_HW_RENDER` gets our `get_proc_address`.
///
/// # Safety
/// `data` is what libretro.h specifies for `cmd`.
pub unsafe fn after_environment(cmd: c_uint, data: *mut c_void, granted: bool) {
    if cmd == ENV_SET_HW_RENDER && granted && !data.is_null() && std::env::var_os("POCKET_CORE_SOFTWARE").is_some() {
        (*(data as *mut CallbackHead)).get_proc_address = Some(get_proc_address);
    }
}

/// The core's context, its framebuffer and the read-back.
pub struct BrickGles {
    egl: Egl,
    request: Request,
    fbo: glow::Framebuffer,
    color: glow::Texture,
    depth: Option<glow::Renderbuffer>,
    size: (u32, u32),
    rgba: Vec<u8>,
    destroyed: bool,
    /// The driver reads B,G,R,A without converting (`IMPLEMENTATION_COLOR_READ_FORMAT`), the order XRGB8888 has in memory.
    bgra: bool,
    frames: u32,
    cost: Cost,
    /// Frames the core presented (diagnostics: `POCKET_CORE_BRICK_DUMP_AT`).
    seen: u32,
    /// A fence after each frame on the screen, the newest last (`before_run`).
    fences: std::collections::VecDeque<glow::Fence>,
    /// What `before_run` waited, summed, and how often it waited at all.
    waited: Duration,
    waits: u32,
    /// `POCKET_CORE_BRICK_DIRECT=1`: the prototype that shows frames on the screen itself (`Direct`).
    direct: Option<Direct>,
}

/// Frames read back the usual way before pocket-core takes the screen at a game's start: the game's window has its first pictures and
/// pocket-ui's start has handed over to it (`docs/TRIMUI-BRICK.md`, "The direct way").
pub const CLAIM_AFTER: u32 = 30;
/// After Continue pocket-ui gives the screen to the window it had in front, the native port: the next frame takes it.
pub const CLAIM_AFTER_RESUME: u32 = 1;
/// Frames the claim may wait for the frame around the picture (prepared on a thread, `Layer::ready`) before it takes the screen anyway.
pub const CLAIM_WAIT_MAX: u32 = 120;
/// Pictures kept on the GPU for a moment's photo (`BrickGles::keep`), as many as pocket-core's ring holds.
pub const KEPT: usize = 5;

/// Whether a 3D core puts its frames on the screen itself (`Direct`): `POCKET_CORE_BRICK_DIRECT=0` never, any other value wherever the
/// screen's surface is; unset on the TrimUI Brick, where it was measured (2026-11-25). The Smart Pro and the Brick Pro wait for a
/// device: the read-back as before.
pub fn direct_wanted(var: Option<&std::ffi::OsStr>, board: Option<crate::brick_profile::Board>) -> bool {
    match var {
        Some(v) => v != "0",
        None => board == Some(crate::brick_profile::Board::Brick),
    }
}

/// Frames the GPU may run behind the core on the direct way (`before_run`).
pub const GPU_BEHIND: usize = 0;

/// What a frame from the core does on the direct way (`Owner::frame`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    /// Read back into the window, as without the direct way.
    ReadBack,
    /// Take the screen now; `force`: even with the frame around the picture not prepared yet.
    Claim { force: bool },
    /// pocket-core has the screen: on it, nothing read back.
    Show,
}

/// What came of a claim (`Owner::claimed`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Claimed {
    /// The bridge stopped drawing: the screen is ours.
    Ours,
    /// Not yet: the picture isn't ready to look like the bridge's (`Layer::ready`); the next frame asks again.
    NotYet,
    /// The bridge said no, or the picture can't be drawn here: the read-back for the rest of the game.
    Refused,
}

/// Who has the screen while a 3D core runs, without GL (the rules have tests): the bridge (frames read back into the game's window)
/// or pocket-core (`Step::Show`). It is taken after `CLAIM_AFTER` frames of the game with nothing holding it (the screen dark, the game
/// held, its switch-on running), given back for the pause menu (`pause`) and taken again after Continue (`resume`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Owner {
    /// `pocket_port begin` was answered yes once: later claims are a focus.
    pub registered: bool,
    pub claimed: bool,
    countdown: u32,
    waited: u32,
    paused: bool,
    refused: bool,
}

impl Default for Owner {
    fn default() -> Owner {
        Owner { registered: false, claimed: false, countdown: CLAIM_AFTER, waited: 0, paused: false, refused: false }
    }
}

impl Owner {
    /// A new frame from the core; `held`: the screen dark, the game held or its switch-on running (the countdown waits).
    pub fn frame(&mut self, held: bool) -> Step {
        if self.claimed {
            return Step::Show;
        }
        if held || self.paused || self.refused {
            return Step::ReadBack;
        }
        self.countdown = self.countdown.saturating_sub(1);
        if self.countdown > 0 {
            return Step::ReadBack;
        }
        Step::Claim { force: self.waited >= CLAIM_WAIT_MAX }
    }

    /// What came of the claim `frame` asked for.
    pub fn claimed(&mut self, how: Claimed) {
        match how {
            Claimed::Ours => {
                self.claimed = true;
                self.registered = true;
                self.waited = 0;
            }
            Claimed::NotYet => self.waited += 1,
            Claimed::Refused => self.refused = true,
        }
    }

    /// The pause menu: the screen goes back to the bridge. True when pocket-core had it (the frame on show goes into the window).
    pub fn pause(&mut self) -> bool {
        self.paused = true;
        std::mem::replace(&mut self.claimed, false)
    }

    /// Continue: taken again with the next frame (`CLAIM_AFTER_RESUME`), or after the start's count when it was never taken.
    pub fn resume(&mut self) {
        if std::mem::replace(&mut self.paused, false) && self.registered {
            self.countdown = CLAIM_AFTER_RESUME;
        }
    }
}

/// The direct way: the core's context is on PVR's Null WSI, the screen, instead of a pbuffer. Once the game's window is up, pocket-core
/// takes the screen from `pocket-fb` the way a native port does (`pocket_port begin`: it stops drawing) and puts each frame there
/// itself: a second context, sharing the core's textures, runs the bridge's own picture renderer (`brick_picture::Layer`: scaling,
/// shaders, the screen frames, Ambient, orientation) on the core's framebuffer and the overlay (`overlay::Overlay`: the FPS line,
/// badges, toasts, the volume bar) over it, then swaps. Nothing is read back. The switch-on is drawn the usual way before the screen is
/// taken; photos, screenshots and save-state pictures read the one frame they need (`snapshot`, `keep`); the pause menu's picture and
/// the Picture page's preview are drawn by the same renderer (`shot`). The pause menu gets the screen back with one frame read back
/// (`release`), Continue takes it again (`resumed`), the end of the game gives it back (`destroy`: `pocket_port end`).
struct Direct {
    screen: (i32, i32),
    owner: Owner,
    /// The screen is dark or the game held (`hold_off`).
    held: bool,
    /// An empty vertex array of ours, bound while we draw on the core's context (its own may point at its memory).
    vao: Option<glow::VertexArray>,
    layer: crate::brick_picture::Layer,
    overlay: Option<crate::overlay::Overlay>,
    /// The size of the frame the layer has (`take_texture`).
    last: Option<(u32, u32)>,
    /// Pictures for a moment's photo, copied on the GPU (`keep`): textures with the size they hold; the next slot.
    kept: Vec<(glow::Texture, (u32, u32))>,
    kept_next: usize,
    /// The read and draw framebuffers of `keep`'s copies.
    copy_fbos: Option<(glow::Framebuffer, glow::Framebuffer)>,
    /// A frame from the core since the last `show`: copied into the renderer there.
    fresh: bool,
    shown: u32,
    /// What `show` took, summed: the picture with the overlay (with the copy), and the swap.
    draw: Duration,
    swap: Duration,
    worst: Duration,
}

/// The core's GL state that the direct way's drawing changes on the core's own context, read before and put back after (cores keep a
/// copy of their state, GLideN64's and PPSSPP's among them): bindings, the first texture unit, blending, the masks, the tests, the
/// pixel store. `neutral` then sets what our drawing needs whatever the core left (no depth or stencil test, no culling, all colours
/// written, no pack or unpack buffer, our empty vertex array).
struct Kept {
    ints: [i32; 14],
    pixel: [i32; 10],
    viewport: [i32; 4],
    scissor_box: [i32; 4],
    color_mask: [i32; 4],
    blend_func: [i32; 4],
    blend_eq: [i32; 2],
    clear: [f32; 4],
    blend_color: [f32; 4],
    caps: [bool; 9],
}

const KEPT_CAPS: [u32; 9] = [glow::BLEND, glow::SCISSOR_TEST, glow::DEPTH_TEST, glow::STENCIL_TEST, glow::CULL_FACE, glow::POLYGON_OFFSET_FILL, glow::SAMPLE_ALPHA_TO_COVERAGE, glow::SAMPLE_COVERAGE, glow::RASTERIZER_DISCARD];
const KEPT_PIXEL: [u32; 10] = [glow::UNPACK_ALIGNMENT, glow::UNPACK_ROW_LENGTH, glow::UNPACK_SKIP_ROWS, glow::UNPACK_SKIP_PIXELS, glow::UNPACK_IMAGE_HEIGHT, glow::UNPACK_SKIP_IMAGES, glow::PACK_ALIGNMENT, glow::PACK_ROW_LENGTH, glow::PACK_SKIP_ROWS, glow::PACK_SKIP_PIXELS];
/// Where `neutral` leaves the pixel store: GL's defaults.
const PIXEL_DEFAULTS: [i32; 10] = [4, 0, 0, 0, 0, 0, 4, 0, 0, 0];

impl Kept {
    /// # Safety
    /// A GL context is current on this thread.
    unsafe fn take(gl: &glow::Context) -> Kept {
        let int = |p| gl.get_parameter_i32(p);
        let active = int(glow::ACTIVE_TEXTURE);
        gl.active_texture(glow::TEXTURE0);
        let ints = [
            int(glow::DRAW_FRAMEBUFFER_BINDING),
            int(glow::READ_FRAMEBUFFER_BINDING),
            int(glow::RENDERBUFFER_BINDING),
            int(glow::CURRENT_PROGRAM),
            int(glow::VERTEX_ARRAY_BINDING),
            int(glow::ARRAY_BUFFER_BINDING),
            int(glow::PIXEL_PACK_BUFFER_BINDING),
            int(glow::PIXEL_UNPACK_BUFFER_BINDING),
            active,
            int(glow::TEXTURE_BINDING_2D),
            int(glow::SAMPLER_BINDING),
            int(glow::BLEND_EQUATION_RGB),
            int(glow::BLEND_EQUATION_ALPHA),
            0,
        ];
        let four = |p| {
            let mut v = [0i32; 4];
            gl.get_parameter_i32_slice(p, &mut v);
            v
        };
        let fourf = |p| {
            let mut v = [0f32; 4];
            gl.get_parameter_f32_slice(p, &mut v);
            v
        };
        Kept {
            ints,
            pixel: KEPT_PIXEL.map(int),
            viewport: four(glow::VIEWPORT),
            scissor_box: four(glow::SCISSOR_BOX),
            color_mask: four(glow::COLOR_WRITEMASK),
            blend_func: [int(glow::BLEND_SRC_RGB), int(glow::BLEND_DST_RGB), int(glow::BLEND_SRC_ALPHA), int(glow::BLEND_DST_ALPHA)],
            blend_eq: [ints[11], ints[12]],
            clear: fourf(glow::COLOR_CLEAR_VALUE),
            blend_color: fourf(glow::BLEND_COLOR),
            caps: KEPT_CAPS.map(|c| gl.is_enabled(c)),
        }
    }

    /// What our drawing needs, whatever the core left.
    ///
    /// # Safety
    /// The context `take` read is current.
    unsafe fn neutral(gl: &glow::Context, vao: Option<&mut Option<glow::VertexArray>>) {
        for c in KEPT_CAPS {
            gl.disable(c);
        }
        gl.color_mask(true, true, true, true);
        gl.blend_equation(glow::FUNC_ADD);
        gl.bind_buffer(glow::PIXEL_PACK_BUFFER, None);
        gl.bind_buffer(glow::PIXEL_UNPACK_BUFFER, None);
        for (p, v) in KEPT_PIXEL.iter().zip(PIXEL_DEFAULTS) {
            gl.pixel_store_i32(*p, v);
        }
        gl.active_texture(glow::TEXTURE0);
        gl.bind_sampler(0, None);
        // Copies and reads draw nothing: no vertex array for them.
        if let Some(vao) = vao {
            if vao.is_none() {
                *vao = gl.create_vertex_array().ok();
            }
            gl.bind_vertex_array(*vao);
        }
    }

    /// # Safety
    /// The context `take` read is current.
    unsafe fn put_back(&self, gl: &glow::Context) {
        let name = |n: i32| std::num::NonZeroU32::new(n as u32);
        let i = &self.ints;
        gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, name(i[0]).map(glow::NativeFramebuffer));
        gl.bind_framebuffer(glow::READ_FRAMEBUFFER, name(i[1]).map(glow::NativeFramebuffer));
        gl.bind_renderbuffer(glow::RENDERBUFFER, name(i[2]).map(glow::NativeRenderbuffer));
        gl.use_program(name(i[3]).map(glow::NativeProgram));
        gl.bind_vertex_array(name(i[4]).map(glow::NativeVertexArray));
        gl.bind_buffer(glow::ARRAY_BUFFER, name(i[5]).map(glow::NativeBuffer));
        gl.bind_buffer(glow::PIXEL_PACK_BUFFER, name(i[6]).map(glow::NativeBuffer));
        gl.bind_buffer(glow::PIXEL_UNPACK_BUFFER, name(i[7]).map(glow::NativeBuffer));
        gl.active_texture(glow::TEXTURE0);
        gl.bind_texture(glow::TEXTURE_2D, name(i[9]).map(glow::NativeTexture));
        gl.bind_sampler(0, name(i[10]).map(glow::NativeSampler));
        gl.active_texture(i[8] as u32);
        let v = self.viewport;
        gl.viewport(v[0], v[1], v[2], v[3]);
        let b = self.scissor_box;
        gl.scissor(b[0], b[1], b[2], b[3]);
        let c = self.clear;
        gl.clear_color(c[0], c[1], c[2], c[3]);
        let c = self.blend_color;
        gl.blend_color(c[0], c[1], c[2], c[3]);
        let f = self.blend_func.map(|v| v as u32);
        gl.blend_func_separate(f[0], f[1], f[2], f[3]);
        gl.blend_equation_separate(self.blend_eq[0] as u32, self.blend_eq[1] as u32);
        let m = self.color_mask.map(|v| v != 0);
        gl.color_mask(m[0], m[1], m[2], m[3]);
        for (c, on) in KEPT_CAPS.iter().zip(self.caps) {
            if on { gl.enable(*c) } else { gl.disable(*c) }
        }
        for (p, v) in KEPT_PIXEL.iter().zip(self.pixel) {
            gl.pixel_store_i32(*p, v);
        }
    }
}

/// What the read-back took, summed over the frames: `glReadPixels` (with the wait for the GPU to finish the frame: on the Brick most of
/// it is reading the GPU's memory, which the CPU doesn't cache), turning it into an upright XRGB8888 picture, and the worst frame.
#[derive(Default)]
struct Cost {
    read: Duration,
    convert: Duration,
    worst: Duration,
}

/// Pixels as read (`src`, `w`×`h`, rows of `w`×4 bytes; B,G,R,A when `bgra`, else R,G,B,A) as XRGB8888 (B,G,R,0) with the top row
/// first; `flip` when the first row read is the bottom one (a core whose origin is the bottom left).
pub fn upright_xrgb(src: &[u8], w: usize, h: usize, bgra: bool, flip: bool) -> Vec<u8> {
    let row = w * 4;
    let mut data = vec![0u8; row * h];
    for (y, dst) in data.chunks_exact_mut(row).enumerate() {
        let sy = if flip { h - 1 - y } else { y };
        let Some(s) = src.get(sy * row..(sy + 1) * row) else { break };
        for (d, s) in dst.as_chunks_mut::<4>().0.iter_mut().zip(s.as_chunks::<4>().0) {
            let v = u32::from_le_bytes(*s);
            let x = if bgra { v & 0x00ff_ffff } else { (v & 0xff00) | (v >> 16 & 0xff) | (v & 0xff) << 16 };
            *d = x.to_le_bytes();
        }
    }
    data
}

impl BrickGles {
    /// The context for the core that asked for one (`hw_render::requested`), a framebuffer at its largest frame, `context_reset`
    /// called. An error says why there is none: the game can't run without it.
    pub fn new(max: (u32, u32)) -> Result<BrickGles, String> {
        let request = hw_render::requested().ok_or("no hardware render request")?;
        if let Api::Gl { major, minor, .. } = request.api {
            return Err(format!("the core wants desktop OpenGL {major}.{minor}; this GPU has only GLES"));
        }
        let say: &mut dyn FnMut(String) = &mut |line| eprintln!("[hw] {line}");
        let (egl, direct) = match direct_wanted(std::env::var_os("POCKET_CORE_BRICK_DIRECT").as_deref(), crate::brick_profile::board()) {
            true => Self::open_direct(&mut *say),
            false => (None, None),
        };
        let egl = match egl {
            Some(egl) => egl,
            None => Egl::open("pbuffer", (16, 16), &mut *say).map_err(|f| format!("[hw] GL context: {f}"))?,
        };
        if !egl.es3 && matches!(request.api, Api::Gles(3, _)) {
            return Err("the core wants GLES 3; only GLES 2 here".into());
        }
        // SAFETY: the context of `egl` is current on this thread from here on.
        let (fbo, color) = unsafe { (egl.gl.create_framebuffer()?, egl.gl.create_texture()?) };
        let mut hw = BrickGles { egl, request, fbo, color, depth: None, size: (0, 0), rgba: Vec::new(), destroyed: false, bgra: false, frames: 0, cost: Cost::default(), seen: 0, fences: std::collections::VecDeque::new(), waited: Duration::ZERO, waits: 0, direct };
        hw.resize(max)?;
        // SAFETY: the context is current, our framebuffer bound (`resize`): the format is the one its reads have without converting.
        hw.bgra = unsafe { (hw.egl.gl.get_parameter_i32(glow::IMPLEMENTATION_COLOR_READ_FORMAT) as u32, hw.egl.gl.get_parameter_i32(glow::IMPLEMENTATION_COLOR_READ_TYPE) as u32) == (glow::BGRA, glow::UNSIGNED_BYTE) };
        eprintln!("[hw] {:?} for the core on the vendor's EGL, framebuffer {}x{}, read back to the CPU ({})", request.api, hw.size.0, hw.size.1, hw.way());
        if let Some(d) = &hw.direct {
            eprintln!("[hw] direct: on the screen ({}x{}) itself after {CLAIM_AFTER} frames, the bridge's renderer on the core's context", d.screen.0, d.screen.1);
        }
        // SAFETY: the core's context is current; the core binds its framebuffer through `get_current_framebuffer`.
        unsafe {
            hw.egl.gl.bind_framebuffer(glow::FRAMEBUFFER, Some(hw.fbo));
            hw.request.reset_context();
        }
        Ok(hw)
    }

    /// The core's context on the screen's surface with the second context's renderer and overlay (`Direct`); None for the direct way
    /// (and the reason in the log) where any of it is missing: the bridge without its GPU path, no Null WSI (a computer), a shared
    /// context or a shader refused. The context is the screen's then all the same only when everything is there.
    fn open_direct(say: &mut dyn FnMut(String)) -> (Option<Egl>, Option<Direct>) {
        // Three pages on the Null WSI: with two the swap waits for the flip (3.7 ms a frame for Super Mario 64, 52.9 fps), with three it
        // doesn't (1.5 ms, 54.9 fps; Brick, 2026-11-21). The variable is the WSI's own, read when the display is opened.
        if std::env::var_os("NULLWS_BUFFERS_COUNT").is_none() {
            std::env::set_var("NULLWS_BUFFERS_COUNT", "3");
        }
        let mut egl = match Egl::open("null", (16, 16), &mut *say) {
            Ok(egl) => egl,
            Err(f) => {
                eprintln!("[hw] direct: no screen surface ({f}), read back");
                return (None, None);
            }
        };
        // The pause menu, the shelf and the switch-on are the bridge's; it draws pictures on the GPU only with `--gl`.
        if !crate::brick_picture::shaders_available() {
            eprintln!("[hw] direct: the bridge has no GPU path, read back");
            return (None, None);
        }
        // SAFETY: the context is current; a window surface's first viewport is its size.
        let screen = unsafe {
            let mut v = [0i32; 4];
            egl.gl.get_parameter_i32_slice(glow::VIEWPORT, &mut v);
            (v[2], v[3])
        };
        // The core runs on a pbuffer as on the read-back; only our picture and the swap are on the window (`show`).
        if std::env::var_os("POCKET_CORE_BRICK_ON_WINDOW").is_none() {
            if let Err(e) = egl.add_pbuffer() {
                eprintln!("[hw] direct: {e}, read back");
                return (None, None);
            }
        }
        // The renderer and the overlay draw on the core's own context, its GL state kept around them (`Kept`): on PVR's driver a second
        // context sharing the core's textures cost a frame in ten more late (44.5 fps against 45.9 for Super Mario 64, 2026-11-25).
        let mut vao = None;
        // SAFETY: the core's context is current; the core has no state yet, kept all the same.
        let made = unsafe {
            let kept = Kept::take(&egl.gl);
            Kept::neutral(&egl.gl, Some(&mut vao));
            let layer = crate::brick_picture::Layer::new(egl.another_gl());
            let overlay = layer.as_ref().ok().and_then(|l| crate::overlay::Overlay::new(l.gl()).map_err(|e| eprintln!("[hw] direct: no overlay ({e})")).ok());
            kept.put_back(&egl.gl);
            layer.map(|l| (l, overlay))
        };
        match made {
            Ok((layer, overlay)) if egl.es3 && screen.0 > 0 && screen.1 > 0 => {
                let direct = Direct { screen, owner: Owner::default(), held: false, vao, layer, overlay, last: None, kept: Vec::new(), kept_next: 0, copy_fbos: None, fresh: false, shown: 0, draw: Duration::ZERO, swap: Duration::ZERO, worst: Duration::ZERO };
                (Some(egl), Some(direct))
            }
            made => {
                let why = made.err().unwrap_or_else(|| format!("GLES 3 {}, screen {}x{}", egl.es3, screen.0, screen.1));
                eprintln!("[hw] direct: {why}, read back");
                (None, None)
            }
        }
    }

    /// (Re)allocates the framebuffer for frames up to `w`×`h`; it keeps its name.
    fn resize(&mut self, (w, h): (u32, u32)) -> Result<(), String> {
        let (w, h) = (w.max(1), h.max(1));
        if (w, h) == self.size {
            return Ok(());
        }
        let gl = &self.egl.gl;
        // SAFETY: the core's context is current.
        unsafe {
            gl.bind_texture(glow::TEXTURE_2D, Some(self.color));
            gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::RGBA8 as i32, w as i32, h as i32, 0, glow::RGBA, glow::UNSIGNED_BYTE, None);
            for (p, v) in [(glow::TEXTURE_MIN_FILTER, glow::NEAREST), (glow::TEXTURE_MAG_FILTER, glow::NEAREST)] {
                gl.tex_parameter_i32(glow::TEXTURE_2D, p, v as i32);
            }
            gl.bind_texture(glow::TEXTURE_2D, None);
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(self.fbo));
            gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(self.color), 0);
            if self.request.depth || self.request.stencil {
                let rb = match self.depth {
                    Some(rb) => rb,
                    None => gl.create_renderbuffer()?,
                };
                self.depth = Some(rb);
                gl.bind_renderbuffer(glow::RENDERBUFFER, Some(rb));
                let format = if self.request.stencil { glow::DEPTH24_STENCIL8 } else { glow::DEPTH_COMPONENT24 };
                gl.renderbuffer_storage(glow::RENDERBUFFER, format, w as i32, h as i32);
                gl.bind_renderbuffer(glow::RENDERBUFFER, None);
                gl.framebuffer_renderbuffer(glow::FRAMEBUFFER, glow::DEPTH_ATTACHMENT, glow::RENDERBUFFER, Some(rb));
                if self.request.stencil {
                    gl.framebuffer_renderbuffer(glow::FRAMEBUFFER, glow::STENCIL_ATTACHMENT, glow::RENDERBUFFER, Some(rb));
                }
            }
            let status = gl.check_framebuffer_status(glow::FRAMEBUFFER);
            gl.clear_color(0.0, 0.0, 0.0, 1.0);
            gl.clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT | glow::STENCIL_BUFFER_BIT);
            if status != glow::FRAMEBUFFER_COMPLETE {
                return Err(format!("core framebuffer incomplete: {status:#x}"));
            }
        }
        hw_render::set_framebuffer(self.fbo.0.get());
        self.size = (w, h);
        Ok(())
    }

    /// After `retro_run`: the frame the core presented, read back as XRGB8888 with its top row first; None when it presented none
    /// (a duped frame: the last picture stays). `max` is the core's largest frame, for a geometry that changed.
    pub fn frame(&mut self, max: (u32, u32)) -> Option<Frame> {
        if let Err(e) = self.resize(max) {
            eprintln!("[hw] {e}");
        }
        let (w, h) = hw_render::take_frame()?;
        let (w, h) = (w.min(self.size.0).max(1), h.min(self.size.1).max(1));
        self.seen += 1;
        // Diagnostics: the core's framebuffer as it is after frame N, raw XRGB, to a file.
        if std::env::var("POCKET_CORE_BRICK_DUMP_AT").ok().and_then(|v| v.parse::<u32>().ok()) == Some(self.seen) {
            // SAFETY: the core's context is current; its state is put back.
            let mut rgba = vec![0u8; w as usize * h as usize * 4];
            unsafe {
                let gl = &self.egl.gl;
                let k = Kept::take(gl);
                Kept::neutral(gl, None);
                gl.bind_framebuffer(glow::FRAMEBUFFER, Some(self.fbo));
                gl.read_pixels(0, 0, w as i32, h as i32, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelPackData::Slice(&mut rgba));
                k.put_back(gl);
            }
            let to = std::env::var("POCKET_CORE_BRICK_DUMP_TO").unwrap_or_else(|_| "/tmp/pocket-core-dump.raw".into());
            let _ = std::fs::write(&to, &upright_xrgb(&rgba, w as usize, h as usize, false, self.request.bottom_left_origin));
            eprintln!("[hw] dump: frame {} {}x{} to {to}", self.seen, w, h);
        }
        let Some(d) = self.direct.as_mut() else { return Some(self.read_back(w, h)) };
        match d.owner.frame(d.held) {
            Step::ReadBack => {}
            Step::Show => {
                d.last = Some((w, h));
                d.fresh = true;
                return None;
            }
            Step::Claim { force } => {
                d.last = Some((w, h));
                d.fresh = true;
                if self.claim(force) {
                    return None;
                }
            }
        }
        Some(self.read_back(w, h))
    }

    /// The w×h frame in our framebuffer, read now (`glReadPixels` in the order the driver gives) and turned upright.
    fn read_back(&mut self, w: u32, h: u32) -> Frame {
        let format = if self.bgra { glow::BGRA } else { glow::RGBA };
        let t = Instant::now();
        self.rgba.resize(w as usize * h as usize * 4, 0);
        // SAFETY: the core's context is current; the buffer holds w×h RGBA. The core may have left a pack buffer or an alignment
        // bound: both are set for the read and the framebuffer bound again for the core after it.
        unsafe {
            let gl = &self.egl.gl;
            gl.bind_buffer(glow::PIXEL_PACK_BUFFER, None);
            gl.pixel_store_i32(glow::PACK_ALIGNMENT, 4);
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(self.fbo));
            gl.read_pixels(0, 0, w as i32, h as i32, format, glow::UNSIGNED_BYTE, glow::PixelPackData::Slice(&mut self.rgba));
        }
        let read = t.elapsed();
        let data = upright_xrgb(&self.rgba, w as usize, h as usize, self.bgra, self.request.bottom_left_origin);
        let took = t.elapsed();
        self.frames += 1;
        self.cost.read += read;
        self.cost.convert += took - read;
        self.cost.worst = self.cost.worst.max(took);
        Frame { width: w, height: h, pitch: w as usize * 4, format: PIXEL_XRGB8888, data }
    }

    /// The direct way is set up (`Direct`): its renderer wants the Picture settings (`set_picture`).
    pub fn direct_way(&self) -> bool {
        self.direct.is_some()
    }

    /// Whether pocket-core has the screen (`Direct`): then it draws nothing into its window.
    pub fn direct(&self) -> bool {
        self.direct.as_ref().is_some_and(|d| d.owner.claimed)
    }

    /// Screen off or the game held, or its switch-on still running (`held`): the screen isn't taken meanwhile.
    pub fn hold_off(&mut self, held: bool) {
        if let Some(d) = self.direct.as_mut() {
            d.held = held;
        }
    }

    /// The Picture settings the direct way draws with (the bridge's for this game, `brick_picture::Request`): taken when they differ
    /// from the ones it has, the shader they need compiled now.
    pub fn set_picture(&mut self, request: &crate::brick_picture::Request) {
        let gl = &self.egl.gl;
        let Some(d) = self.direct.as_mut().filter(|d| d.layer.request() != Some(request)) else { return };
        // SAFETY: the core's context is current; its state is put back.
        unsafe {
            let kept = Kept::take(gl);
            Kept::neutral(gl, Some(&mut d.vao));
            d.layer.set(request.clone());
            kept.put_back(gl);
        }
    }

    /// The frame the core presented last into the renderer, on the GPU, when a new one came (`fresh`); then the core's framebuffer is
    /// cleared. PVR's driver gives a texture that the GPU still reads (our copy) fresh memory when the core draws into it again, and keeps
    /// nothing of what was there: what the core doesn't draw each frame (the black around an N64 picture) showed old frames and noise
    /// (Brick, 2026-11-25). A whole clear after the copy makes that memory black. What needs the picture later reads the renderer's
    /// copy (`Layer::read_picture`).
    ///
    /// # Safety
    /// The core's context is current, its state kept and set for us (`Kept::neutral`).
    unsafe fn take_fresh(gl: &glow::Context, d: &mut Direct, (fbo, color): (glow::Framebuffer, glow::Texture), bottom_left: bool) {
        let (true, Some((w, h))) = (std::mem::take(&mut d.fresh), d.last) else { return };
        d.layer.take_texture(color, w, h, bottom_left);
        if std::env::var_os("POCKET_CORE_BRICK_NO_CLEAR").is_none() {
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
            gl.clear_color(0.0, 0.0, 0.0, 1.0);
            gl.clear(glow::COLOR_BUFFER_BIT);
        }
    }

    /// Takes the screen from `pocket-fb`: once the picture looks as the bridge drew it (unless `force`), registered as its native port
    /// (`pocket_port begin`: it stops drawing) or, after the pause menu, focused again. True: the screen is ours.
    fn claim(&mut self, force: bool) -> bool {
        use crate::control::ipc;
        let gl = &self.egl.gl;
        let target = (self.fbo, self.color);
        let bottom_left = self.request.bottom_left_origin;
        let Some(d) = self.direct.as_mut() else { return false };
        let screen = (d.screen.0 as u32, d.screen.1 as u32);
        // SAFETY: the core's context is current; its state is put back.
        let ready = unsafe {
            let kept = Kept::take(gl);
            Kept::neutral(gl, Some(&mut d.vao));
            Self::take_fresh(gl, d, target, bottom_left);
            let ready = force || d.layer.ready(screen);
            kept.put_back(gl);
            ready
        };
        if !ready {
            d.owner.claimed(Claimed::NotYet);
            return false;
        }
        let command = if d.owner.registered { r#"[app_id="pocket-native-port"] focus"#.to_string() } else { format!("pocket_port begin {}", std::process::id()) };
        let ok = ipc::request(ipc::RUN_COMMAND, &command, Duration::from_millis(300)).is_ok_and(|r| ipc::all_succeeded(&r));
        eprintln!("[hw] direct: {command}: {}{}", if ok { "the screen is ours" } else { "refused, read back" }, if force { " (the frame around the picture not ready)" } else { "" });
        d.owner.claimed(if ok { Claimed::Ours } else { Claimed::Refused });
        ok
    }

    /// The picture the renderer has (the frame on show), read back once: a photo, a screenshot, a save state's picture. None without
    /// the screen: the window's picture is the one then (`last_frame`).
    pub fn snapshot(&self) -> Option<Frame> {
        let d = self.direct.as_ref().filter(|d| d.owner.claimed)?;
        Self::read_picture(&self.egl.gl, d)
    }

    fn read_picture(gl: &glow::Context, d: &Direct) -> Option<Frame> {
        // SAFETY: the core's context is current; its state is put back.
        unsafe {
            let kept = Kept::take(gl);
            Kept::neutral(gl, None);
            let frame = d.layer.read_picture();
            kept.put_back(gl);
            frame
        }
    }

    /// With the screen taken: the frame on show copied on the GPU for a moment's photo (pocket-core keeps one every 2 s, `album_keep`),
    /// read only when a photo is taken (`kept`). The slot it went into; None without the screen (the read-back has `last_frame`).
    pub fn keep(&mut self) -> Option<usize> {
        let gl = &self.egl.gl;
        let d = self.direct.as_mut().filter(|d| d.owner.claimed)?;
        let (src, w, h) = d.layer.picture()?;
        let slot = d.kept_next % KEPT;
        // SAFETY: the core's context is current; the copies' framebuffers and the slot's texture are ours; the core's state is put back.
        let copied = unsafe {
            let kept = Kept::take(gl);
            Kept::neutral(gl, None);
            let copied = Self::copy_into(gl, d, slot, src, (w, h));
            kept.put_back(gl);
            copied
        };
        copied?;
        d.kept_next += 1;
        Some(slot)
    }

    /// `keep`'s copy, texture to texture on the GPU.
    ///
    /// # Safety
    /// The core's context is current, its state kept.
    unsafe fn copy_into(gl: &glow::Context, d: &mut Direct, slot: usize, src: glow::Texture, (w, h): (u32, u32)) -> Option<()> {
        let (read, draw) = match d.copy_fbos {
            Some(f) => f,
            None => *d.copy_fbos.insert((gl.create_framebuffer().ok()?, gl.create_framebuffer().ok()?)),
        };
        if d.kept.len() <= slot {
            let t = gl.create_texture().ok()?;
            d.kept.push((t, (0, 0)));
        }
        let (tex, size) = &mut d.kept[slot];
        if *size != (w, h) {
            gl.bind_texture(glow::TEXTURE_2D, Some(*tex));
            gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::RGBA8 as i32, w as i32, h as i32, 0, glow::RGBA, glow::UNSIGNED_BYTE, None);
            *size = (w, h);
        }
        gl.bind_framebuffer(glow::READ_FRAMEBUFFER, Some(read));
        gl.framebuffer_texture_2d(glow::READ_FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(src), 0);
        gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, Some(draw));
        gl.framebuffer_texture_2d(glow::DRAW_FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(*tex), 0);
        gl.blit_framebuffer(0, 0, w as i32, h as i32, 0, 0, w as i32, h as i32, glow::COLOR_BUFFER_BIT, glow::NEAREST);
        Some(())
    }

    /// The picture `keep` put in `slot`, read back now (XRGB8888, upright as the renderer holds it).
    pub fn kept(&self, slot: usize) -> Option<Frame> {
        let d = self.direct.as_ref()?;
        let (tex, (w, h)) = *d.kept.get(slot)?;
        let (read, _) = d.copy_fbos?;
        if w == 0 {
            return None;
        }
        let gl = &self.egl.gl;
        let mut data = vec![0u8; w as usize * h as usize * 4];
        // SAFETY: the core's context is current; the buffer holds w×h RGBA; the core's state is put back.
        unsafe {
            let kept = Kept::take(gl);
            Kept::neutral(gl, None);
            gl.bind_framebuffer(glow::READ_FRAMEBUFFER, Some(read));
            gl.framebuffer_texture_2d(glow::READ_FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(tex), 0);
            gl.read_pixels(0, 0, w as i32, h as i32, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelPackData::Slice(&mut data));
            kept.put_back(gl);
        }
        // RGBA to XRGB8888's B, G, R, X, as `Renderer::read_frame`.
        for px in data.chunks_exact_mut(4) {
            px.swap(0, 2);
        }
        Some(Frame { width: w, height: h, pitch: w as usize * 4, format: PIXEL_XRGB8888, data })
    }

    /// The pause menu's picture and the Picture page's preview on the direct way: the frame the renderer has, drawn at `w` by `h` with
    /// the settings as the menu has them now (`set_picture` first), as the screen shows it (RGBA, top row first), and where the game
    /// sits in it. None before the direct way had a frame: the bridge's renderer has the picture then.
    pub fn shot(&mut self, w: u32, h: u32) -> Option<(Vec<u8>, Option<crate::preview::GameRect>)> {
        let gl = &self.egl.gl;
        let target = (self.fbo, self.color);
        let bottom_left = self.request.bottom_left_origin;
        let d = self.direct.as_mut().filter(|d| d.last.is_some() && d.layer.active())?;
        // SAFETY: the core's context is current; its state is put back.
        unsafe {
            let kept = Kept::take(gl);
            Kept::neutral(gl, Some(&mut d.vao));
            Self::take_fresh(gl, d, target, bottom_left);
            let shot = d.layer.shot(w, h).map(|rgba| (rgba, d.layer.game_rect(w, h)));
            kept.put_back(gl);
            shot
        }
    }

    /// With the screen taken: the frame on it as the settings say with `hud` over it, then the swap. Called once a loop (pocket-core's
    /// `draw`), so a duped frame still shows the overlay moving.
    pub fn show(&mut self, hud: &mut crate::overlay::Hud) {
        let gl = &self.egl.gl;
        let target = (self.fbo, self.color);
        let bottom_left = self.request.bottom_left_origin;
        let Some(d) = self.direct.as_mut().filter(|d| d.owner.claimed) else { return };
        let t = Instant::now();
        let screen = (d.screen.0 as u32, d.screen.1 as u32);
        if let Err(e) = self.egl.on_window(true) {
            eprintln!("[hw] direct: {e}");
            return;
        }
        // SAFETY: the core's context is current; its state is kept around our drawing and put back after the swap.
        let kept = unsafe {
            let kept = Kept::take(gl);
            Kept::neutral(gl, Some(&mut d.vao));
            Self::take_fresh(gl, d, target, bottom_left);
            kept
        };
        let drawn = d.layer.draw_on(screen);
        if drawn.is_ok() {
            if let Some(o) = d.overlay.as_mut() {
                o.draw(d.layer.gl(), hud, screen.0, screen.1);
            }
        }
        let drew = t.elapsed();
        if let Err(e) = drawn.and_then(|()| self.egl.swap()) {
            eprintln!("[hw] direct: {e}");
        }
        // SAFETY: as above; the fence is `before_run`'s.
        unsafe {
            if let Ok(f) = gl.fence_sync(glow::SYNC_GPU_COMMANDS_COMPLETE, 0) {
                self.fences.push_back(f);
            }
            kept.put_back(gl);
        }
        if let Err(e) = self.egl.on_window(false) {
            eprintln!("[hw] direct: {e}");
        }
        let took = t.elapsed();
        d.shown += 1;
        d.draw += drew;
        d.swap += took - drew;
        d.worst = d.worst.max(took);
    }

    /// Pause: the screen goes back to `pocket-fb` (the pause menu draws there) with the frame on show read back, for the game's window
    /// and the previews. None when pocket-core didn't have the screen.
    pub fn release(&mut self) -> Option<Frame> {
        let gl = &self.egl.gl;
        let target = (self.fbo, self.color);
        let bottom_left = self.request.bottom_left_origin;
        let d = self.direct.as_mut()?;
        if !d.owner.pause() {
            return None;
        }
        // SAFETY: the core's context is current; its state is put back.
        unsafe {
            let kept = Kept::take(gl);
            Kept::neutral(gl, Some(&mut d.vao));
            Self::take_fresh(gl, d, target, bottom_left);
            kept.put_back(gl);
        }
        Self::read_picture(gl, d)
    }

    /// Before the core runs: on the direct way the GPU may be at most `GPU_BEHIND` frames behind (a fence after each frame on the screen,
    /// the oldest waited for). The read-back waited for the GPU every frame; without any wait the GPU fell further behind than the
    /// core's own unfenced reads allow. Usually done already: no wait.
    pub fn before_run(&mut self) {
        let behind = std::env::var("POCKET_CORE_BRICK_GPU_BEHIND").ok().and_then(|v| v.parse::<usize>().ok()).unwrap_or(GPU_BEHIND);
        if behind == 0 {
            while let Some(f) = self.fences.pop_front() {
                // SAFETY: the core's context is current; the fence is ours.
                unsafe { self.egl.gl.delete_sync(f) };
            }
            return;
        }
        if self.fences.len() < behind {
            return;
        }
        let Some(f) = self.fences.pop_front() else { return };
        let t = Instant::now();
        // SAFETY: the core's context is current; the fence is ours, waited for once and deleted.
        unsafe {
            let gl = &self.egl.gl;
            let done = gl.client_wait_sync(f, 0, 0);
            if done == glow::TIMEOUT_EXPIRED {
                gl.client_wait_sync(f, glow::SYNC_FLUSH_COMMANDS_BIT, 100_000_000);
                self.waits += 1;
                self.waited += t.elapsed();
            }
            gl.delete_sync(f);
        }
    }

    /// Continue: the screen is taken again with the next frame.
    pub fn resumed(&mut self) {
        if let Some(d) = self.direct.as_mut() {
            d.owner.resume();
        }
    }

    /// The order frames are read in, for the log.
    fn way(&self) -> &'static str {
        if self.bgra { "bgra" } else { "rgba" }
    }

    /// The core's `context_destroy`, once, before `retro_unload_game` (RetroArch's order); the read-back's numbers to the log.
    pub fn destroy(&mut self) {
        if std::mem::replace(&mut self.destroyed, true) {
            return;
        }
        if self.frames > 0 {
            let ms = |d: Duration| d.as_secs_f64() * 1000.0 / f64::from(self.frames);
            let c = &self.cost;
            eprintln!(
                "[hw] {} frames read back ({}, {}x{} max), {:.2} ms each: read {:.2}, convert {:.2}; worst {:.2} ms",
                self.frames,
                self.way(),
                self.size.0,
                self.size.1,
                ms(c.read + c.convert),
                ms(c.read),
                ms(c.convert),
                c.worst.as_secs_f64() * 1000.0
            );
        }
        if self.direct.as_ref().is_some_and(|d| d.owner.registered) {
            use crate::control::ipc;
            let _ = ipc::request(ipc::RUN_COMMAND, &format!("pocket_port end {}", std::process::id()), Duration::from_millis(300));
        }
        if let Some(d) = self.direct.as_ref().filter(|d| d.shown > 0) {
            let ms = |t: Duration| t.as_secs_f64() * 1000.0 / f64::from(d.shown);
            eprintln!("[hw] direct: {} frames on the screen, {:.2} ms each: picture and overlay {:.2}, swap {:.2}; worst {:.2} ms; waited for the GPU {} times, {:.2} ms each", d.shown, ms(d.draw + d.swap), ms(d.draw), ms(d.swap), d.worst.as_secs_f64() * 1000.0, self.waits, self.waited.as_secs_f64() * 1000.0 / f64::from(self.waits.max(1)));
        }
        // SAFETY: the core's context is current.
        unsafe { self.request.destroy_context() };
    }
}


#[cfg(test)]
mod tests {
    use super::{direct_wanted, upright_xrgb, Claimed, Owner, Step, CLAIM_AFTER, CLAIM_WAIT_MAX};
    use crate::brick_profile::Board;
    use std::ffi::OsStr;

    /// Frames until `frame` stops reading back; the step it ends on.
    fn until_not_read_back(o: &mut Owner, held: bool, max: u32) -> (u32, Step) {
        for n in 1..=max {
            let step = o.frame(held);
            if step != Step::ReadBack {
                return (n, step);
            }
        }
        (max, Step::ReadBack)
    }

    #[test]
    fn the_direct_way_is_the_bricks_by_default_and_the_switch_wins() {
        assert!(direct_wanted(None, Some(Board::Brick)), "measured there: on by default");
        assert!(!direct_wanted(None, Some(Board::SmartPro)) && !direct_wanted(None, Some(Board::BrickPro)), "no device to measure on: the read-back");
        assert!(!direct_wanted(None, None), "dArkOS and computers run the same pocket-core: never on their own");
        assert!(!direct_wanted(Some(OsStr::new("0")), Some(Board::Brick)), "0 switches it off");
        assert!(direct_wanted(Some(OsStr::new("1")), None), "any other value asks for it anywhere (it falls back without a screen surface)");
    }

    #[test]
    fn the_screen_is_taken_after_the_start_and_never_while_held() {
        let mut o = Owner::default();
        // The screen dark, the game held or its switch-on running: read back, the count waits.
        assert_eq!(until_not_read_back(&mut o, true, 500), (500, Step::ReadBack));
        assert_eq!(until_not_read_back(&mut o, false, 500), (CLAIM_AFTER, Step::Claim { force: false }), "the start's frames once nothing holds it");
        o.claimed(Claimed::Ours);
        assert!(o.claimed && o.registered);
        assert_eq!(o.frame(false), Step::Show);
        assert_eq!(o.frame(true), Step::Show, "a dark screen keeps it (pocket-core draws nothing then)");
    }

    #[test]
    fn a_claim_waits_for_the_frame_around_the_picture_but_not_for_ever() {
        let mut o = Owner::default();
        assert_eq!(until_not_read_back(&mut o, false, 500).1, Step::Claim { force: false });
        for _ in 0..CLAIM_WAIT_MAX {
            o.claimed(Claimed::NotYet);
            assert!(matches!(o.frame(false), Step::Claim { .. }), "asked again with the next frame");
        }
        o.claimed(Claimed::NotYet);
        assert_eq!(o.frame(false), Step::Claim { force: true }, "taken anyway after {CLAIM_WAIT_MAX} frames");
    }

    #[test]
    fn a_refused_claim_reads_back_for_the_rest_of_the_game() {
        let mut o = Owner::default();
        until_not_read_back(&mut o, false, 500);
        o.claimed(Claimed::Refused);
        assert_eq!(until_not_read_back(&mut o, false, 1000), (1000, Step::ReadBack));
        o.pause();
        o.resume();
        assert_eq!(until_not_read_back(&mut o, false, 1000), (1000, Step::ReadBack), "Continue doesn't ask again");
    }

    #[test]
    fn the_pause_menu_gets_the_screen_and_continue_takes_it_with_the_next_frame() {
        let mut o = Owner::default();
        until_not_read_back(&mut o, false, 500);
        o.claimed(Claimed::Ours);
        assert!(o.pause(), "had the screen: its frame goes into the window");
        assert!(!o.claimed);
        assert!(!o.pause(), "a second pause has nothing to give back");
        assert_eq!(until_not_read_back(&mut o, false, 100), (100, Step::ReadBack), "paused: the bridge's");
        o.resume();
        assert_eq!(o.frame(false), Step::Claim { force: false }, "pocket-ui gave the screen to the native port: the next frame takes it");
        o.claimed(Claimed::Ours);
        assert_eq!(o.frame(false), Step::Show);
    }

    #[test]
    fn a_pause_before_the_screen_was_taken_keeps_the_starts_count() {
        let mut o = Owner::default();
        assert_eq!(until_not_read_back(&mut o, false, 10), (10, Step::ReadBack));
        assert!(!o.pause(), "nothing to give back");
        o.resume();
        assert_eq!(until_not_read_back(&mut o, false, 500), (CLAIM_AFTER - 10, Step::Claim { force: false }), "the rest of the start's count: the window is the one on show");
    }

    /// Two rows of two pixels, red green / blue white, as the GPU's bottom row first.
    const RGBA_BOTTOM_FIRST: [u8; 16] = [0, 0, 255, 255, 255, 255, 255, 255, 255, 0, 0, 255, 0, 255, 0, 255];

    /// The N64 border (Brick, 2026-11-26): GLideN64's new picture textures came with the PowerVR driver's old memory, shown in the black
    /// frame around the picture. No test saw it: Mesa, where the tests run, hands out zeroed textures; only the policy is testable here.
    #[test]
    fn the_n64_cores_new_pictures_are_cleared_in_every_size_it_makes_and_nothing_else_is() {
        use super::picture_clear;
        for (w, h) in [(640, 480), (320, 240), (292, 240), (640, 526), (640, 580), (1280, 960)] {
            assert_eq!(picture_clear(true, glow::TEXTURE_2D, glow::RGBA8, w, h), Some((glow::RGBA, glow::UNSIGNED_BYTE, 4)), "{w}x{h}");
        }
        assert!(picture_clear(true, glow::TEXTURE_2D, glow::RGB565, 320, 240).is_some());
        assert_eq!(picture_clear(false, glow::TEXTURE_2D, glow::RGBA8, 640, 480), None, "other cores keep the driver's texture");
        assert_eq!(picture_clear(true, glow::TEXTURE_2D, glow::RGBA8, 64, 32), None, "the N64's own small textures are filled by the core");
        assert_eq!(picture_clear(true, glow::TEXTURE_2D, glow::DEPTH_COMPONENT24, 640, 480), None, "depth stays");
        assert_eq!(picture_clear(true, glow::TEXTURE_2D_ARRAY, glow::RGBA8, 640, 480), None);
    }

    #[test]
    fn rgba_read_from_the_bottom_comes_out_upright_as_xrgb() {
        let px = upright_xrgb(&RGBA_BOTTOM_FIRST, 2, 2, false, true);
        assert_eq!(px, [0, 0, 255, 0, 0, 255, 0, 0, 255, 0, 0, 0, 255, 255, 255, 0], "red green on top, blue white below");
    }

    #[test]
    fn bgra_is_kept_with_x_zero_and_the_rows_stay_without_flip() {
        let bgra: Vec<u8> = RGBA_BOTTOM_FIRST.chunks(4).flat_map(|p| [p[2], p[1], p[0], p[3]]).collect();
        assert_eq!(upright_xrgb(&bgra, 2, 2, true, false), [255, 0, 0, 0, 255, 255, 255, 0, 0, 0, 255, 0, 0, 255, 0, 0]);
        assert_eq!(upright_xrgb(&bgra, 2, 2, true, true), upright_xrgb(&RGBA_BOTTOM_FIRST, 2, 2, false, true));
    }

    #[test]
    fn a_short_read_leaves_the_missing_rows_black() {
        assert_eq!(upright_xrgb(&RGBA_BOTTOM_FIRST[..8], 2, 2, false, false), [255, 0, 0, 0, 255, 255, 255, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    }
}
