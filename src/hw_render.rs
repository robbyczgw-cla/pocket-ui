//! Hardware-rendered cores (N64, PSP, Dreamcast …): libretro's `SET_HW_RENDER` for OpenGL and OpenGL ES.
//!
//! The core gets its own GL context, shared with pocket-core's, and renders into a framebuffer
//! object we own (colour texture plus depth/stencil as it asks). After each frame the picture is
//! copied out of that texture into the `Renderer`'s, so scaling, sharpness and effects apply as for
//! software cores, and a duped frame simply shows the last copy again.
//!
//! Two contexts instead of one: the core may leave any GL state behind (bound VAO, scissor, pixel
//! store …) and may expect its own to survive between frames (`SET_HW_SHARED_CONTEXT`); with its
//! own context neither side can disturb the other. The core's context is current whenever the core
//! runs (retro_run, serialize, unload); ours only while we copy, draw and swap.
//!
//! Hooks: `environment` and `video_refresh` from `libretro.rs`, `HwContext` from pocket-core.

use std::ffi::{c_char, c_uint, c_void};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, OnceLock};

use glow::HasContext;
use glutin::config::Config;
use glutin::context::{ContextApi, ContextAttributesBuilder, GlProfile, NotCurrentGlContext, PossiblyCurrentContext, PossiblyCurrentGlContext, Version};
use glutin::display::{Display, GlDisplay};
use glutin::surface::{Surface, WindowSurface};
use raw_window_handle::RawWindowHandle;

const ENV_SET_HW_RENDER: c_uint = 14;
const ENV_EXPERIMENTAL: c_uint = 0x10000;
/// RetroArch-private block (libretro.h: RETRO_ENVIRONMENT_RETROARCH_START_BLOCK).
const ENV_GET_CLEAR_ALL_THREAD_WAITS_CB: c_uint = 3 | 0x800000;
const ENV_GET_HW_RENDER_INTERFACE: c_uint = 41 | ENV_EXPERIMENTAL;
const ENV_SET_HW_RENDER_CONTEXT_NEGOTIATION_INTERFACE: c_uint = 43 | ENV_EXPERIMENTAL;
const ENV_SET_HW_SHARED_CONTEXT: c_uint = 44 | ENV_EXPERIMENTAL;
const ENV_GET_PREFERRED_HW_RENDER: c_uint = 56;

// enum retro_hw_context_type
const CONTEXT_OPENGL: c_uint = 1;
const CONTEXT_OPENGLES2: c_uint = 2;
const CONTEXT_OPENGL_CORE: c_uint = 3;
const CONTEXT_OPENGLES3: c_uint = 4;
const CONTEXT_OPENGLES_VERSION: c_uint = 5;

/// `video_refresh`'s data pointer when the frame is in the hardware framebuffer.
const FRAME_BUFFER_VALID: usize = usize::MAX;

type ResetFn = unsafe extern "C" fn();

/// `struct retro_hw_render_callback`.
#[repr(C)]
struct HwRenderCallback {
    context_type: c_uint,
    context_reset: Option<ResetFn>,
    get_current_framebuffer: Option<unsafe extern "C" fn() -> usize>,
    get_proc_address: Option<unsafe extern "C" fn(*const c_char) -> *const c_void>,
    depth: bool,
    stencil: bool,
    bottom_left_origin: bool,
    version_major: c_uint,
    version_minor: c_uint,
    cache_context: bool,
    context_destroy: Option<ResetFn>,
    debug_context: bool,
}

/// The kind of GL context a core renders with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Api {
    Gles(u8, u8),
    /// Desktop OpenGL, core profile (or compatibility profile when `compat`).
    Gl { major: u8, minor: u8, compat: bool },
}

impl Api {
    /// glutin attributes for a context of this kind (`share`: the context to share objects with).
    fn attributes(self, share: Option<&PossiblyCurrentContext>, window: Option<RawWindowHandle>) -> glutin::context::ContextAttributes {
        let mut b = ContextAttributesBuilder::new();
        if let Some(share) = share {
            b = b.with_sharing(share);
        }
        match self {
            Api::Gles(major, minor) => b.with_context_api(ContextApi::Gles(Some(Version::new(major, minor)))),
            Api::Gl { major, minor, compat } => b
                .with_context_api(ContextApi::OpenGl(Some(Version::new(major, minor))))
                .with_profile(if compat { GlProfile::Compatibility } else { GlProfile::Core }),
        }
        .build(window)
    }
}

/// Attributes for pocket-core's own context: GLES 3.0, or desktop OpenGL 3.3 core when the core
/// renders with desktop OpenGL (contexts share objects only within one API).
pub fn main_context_attributes(window: Option<RawWindowHandle>) -> glutin::context::ContextAttributes {
    let api = match requested().map(|r| r.api) {
        Some(Api::Gl { major, minor, .. }) => {
            let (major, minor) = if (major, minor) < (3, 3) { (3, 3) } else { (major, minor) };
            Api::Gl { major, minor, compat: false }
        }
        _ => Api::Gles(3, 0),
    };
    api.attributes(None, window)
}

/// What the core asked for in `SET_HW_RENDER`.
#[derive(Clone, Copy, Debug)]
pub struct Request {
    pub api: Api,
    pub depth: bool,
    pub stencil: bool,
    pub bottom_left_origin: bool,
    context_reset: Option<ResetFn>,
    context_destroy: Option<ResetFn>,
}

/// A `Cell` that any thread may use: threaded cores call back from their own threads.
struct SyncCell<T>(Mutex<T>);

impl<T: Copy + Default> SyncCell<T> {
    const fn new(v: T) -> Self {
        SyncCell(Mutex::new(v))
    }
    fn lock(&self) -> std::sync::MutexGuard<'_, T> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }
    fn set(&self, v: T) {
        *self.lock() = v;
    }
    fn get(&self) -> T {
        *self.lock()
    }
    fn take(&self) -> T {
        std::mem::take(&mut *self.lock())
    }
}

static REQUEST: SyncCell<Option<Request>> = SyncCell::new(None);
static SHARED_CONTEXT: SyncCell<bool> = SyncCell::new(false);
/// Size of the frame the core rendered during the last `retro_run`, if it presented one.
static FRAME: SyncCell<Option<(u32, u32)>> = SyncCell::new(None);
/// Held by tests that touch the globals above (`Core::load` resets them).
#[cfg(test)]
pub(crate) static TEST_GLOBALS: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Name of the framebuffer object the core renders into (get_current_framebuffer).
static FBO: AtomicU32 = AtomicU32::new(0);

unsafe extern "C" fn get_current_framebuffer() -> usize {
    FBO.load(Ordering::Relaxed) as usize
}

type GetProcAddress = unsafe extern "C" fn(*const c_char) -> *const c_void;

/// libEGL's eglGetProcAddress; Mesa resolves core GL functions through it too.
fn egl_get_proc_address() -> Option<GetProcAddress> {
    static GPA: OnceLock<Option<(libloading::Library, GetProcAddress)>> = OnceLock::new();
    GPA.get_or_init(|| unsafe {
        let lib = libloading::Library::new("libEGL.so.1").ok()?;
        let f = *lib.get::<GetProcAddress>(b"eglGetProcAddress").ok()?;
        Some((lib, f))
    })
    .as_ref()
    .map(|(_, f)| *f)
}

unsafe extern "C" fn get_proc_address(name: *const c_char) -> *const c_void {
    match egl_get_proc_address() {
        Some(f) if !name.is_null() => f(name),
        _ => std::ptr::null(),
    }
}

/// RetroArch lets threaded cores (Mupen64Plus-Next's threaded renderer) release a frontend thread
/// that waits on them (its threaded audio driver). Nothing of ours waits on the core, so there is
/// nothing to release; but the core calls this unchecked in serialize and unload.
unsafe extern "C" fn clear_thread_waits(_clear: c_uint, _data: *mut c_void) -> bool {
    true
}

/// Answers the hardware-rendering environment commands; `None` for every other command.
///
/// # Safety
/// `data` must be what libretro.h specifies for `cmd`.
pub unsafe fn environment(cmd: c_uint, data: *mut c_void) -> Option<bool> {
    // These read or write `data`: refused without it (a broken core must not crash pocket-core).
    if data.is_null() && matches!(cmd, ENV_SET_HW_RENDER | ENV_GET_PREFERRED_HW_RENDER | ENV_GET_CLEAR_ALL_THREAD_WAITS_CB) {
        return Some(false);
    }
    Some(match cmd {
        ENV_SET_HW_RENDER => {
            let cb = &mut *(data as *mut HwRenderCallback);
            let (major, minor) = (cb.version_major.min(4) as u8, cb.version_minor.min(6) as u8);
            let api = match cb.context_type {
                CONTEXT_OPENGLES2 => Api::Gles(2, 0),
                CONTEXT_OPENGLES3 => Api::Gles(3, 0),
                CONTEXT_OPENGLES_VERSION if major <= 3 => Api::Gles(major, minor),
                CONTEXT_OPENGL_CORE => Api::Gl { major: major.max(3), minor, compat: false },
                // Plain "OpenGL": any version, compatibility profile.
                CONTEXT_OPENGL => Api::Gl { major: major.max(3), minor: if major < 3 { 0 } else { minor }, compat: true },
                // Vulkan isn't offered: cores then retry with OpenGL.
                other => {
                    eprintln!("[hw] refused context type {other} ({}.{})", cb.version_major, cb.version_minor);
                    return Some(false);
                }
            };
            cb.get_current_framebuffer = Some(get_current_framebuffer);
            cb.get_proc_address = Some(get_proc_address);
            let req = Request {
                api,
                depth: cb.depth,
                stencil: cb.stencil,
                bottom_left_origin: cb.bottom_left_origin,
                context_reset: cb.context_reset,
                context_destroy: cb.context_destroy,
            };
            eprintln!("[hw] {api:?} context, depth {} stencil {} bottom-left origin {}", req.depth, req.stencil, req.bottom_left_origin);
            REQUEST.set(Some(req));
            true
        }
        // No preference: every core then picks its best API and falls back on refusal. Answering
        // "OpenGL core", as RetroArch's glcore driver here does, sends ROCKNIX's GLES build of
        // PPSSPP to its software renderer. POCKET_CORE_PREFERRED_HW=<context type> overrides
        // this for experiments.
        ENV_GET_PREFERRED_HW_RENDER => match std::env::var("POCKET_CORE_PREFERRED_HW").ok().and_then(|v| v.parse::<c_uint>().ok()) {
            Some(t) if t > 0 => {
                *(data as *mut c_uint) = t;
                true
            }
            _ => false,
        },
        // The core always gets a context of its own, which is what this asks for.
        ENV_SET_HW_SHARED_CONTEXT => {
            SHARED_CONTEXT.set(true);
            true
        }
        ENV_GET_CLEAR_ALL_THREAD_WAITS_CB => {
            *(data as *mut Option<unsafe extern "C" fn(c_uint, *mut c_void) -> bool>) = Some(clear_thread_waits);
            true
        }
        // Only Vulkan and Direct3D have render interfaces.
        ENV_GET_HW_RENDER_INTERFACE | ENV_SET_HW_RENDER_CONTEXT_NEGOTIATION_INTERFACE => false,
        _ => return None,
    })
}

/// Handles `video_refresh` for hardware frames; returns true when it did (the frame is in our FBO).
pub fn video_refresh(data: *const c_void, width: c_uint, height: c_uint) -> bool {
    if data as usize != FRAME_BUFFER_VALID {
        return false;
    }
    FRAME.set(Some((width, height)));
    true
}

/// The core asked for hardware rendering while loading its game.
pub fn requested() -> Option<Request> {
    REQUEST.get()
}

/// Forgets a previous core's request (one process runs one core, but tests load several).
pub fn reset() {
    REQUEST.set(None);
    SHARED_CONTEXT.set(false);
    FRAME.set(None);
}

/// The size of the frame rendered by the last `retro_run`, taken once.
pub fn take_frame() -> Option<(u32, u32)> {
    FRAME.take()
}

/// The core's GL context and the framebuffer it draws into.
pub struct HwContext {
    context: PossiblyCurrentContext,
    gl: glow::Context,
    fbo: glow::Framebuffer,
    color: glow::Texture,
    depth: Option<glow::Renderbuffer>,
    size: (u32, u32),
    pub request: Request,
    destroyed: bool,
}

impl HwContext {
    /// Creates the core's context next to `share` (ours), makes it current on `surface`, builds the
    /// framebuffer at the core's maximum frame size and calls the core's `context_reset`.
    /// Leaves the core's context current.
    pub fn new(
        display: &Display,
        config: &Config,
        share: &PossiblyCurrentContext,
        surface: &Surface<WindowSurface>,
        window: Option<RawWindowHandle>,
        max: (u32, u32),
    ) -> Result<Self, String> {
        let request = requested().ok_or("no hardware render request")?;
        let attrs = request.api.attributes(Some(share), window);
        let context = unsafe { display.create_context(config, &attrs) }.map_err(|e| format!("core GL context: {e}"))?;
        let context = context.make_current(surface).map_err(|e| format!("core GL context current: {e}"))?;
        let gl = unsafe { glow::Context::from_loader_function_cstr(|s| display.get_proc_address(s)) };
        let (fbo, color) = unsafe { (gl.create_framebuffer()?, gl.create_texture()?) };
        let mut hw = HwContext { context, gl, fbo, color, depth: None, size: (0, 0), request, destroyed: false };
        hw.resize(max.0, max.1)?;
        unsafe {
            eprintln!(
                "[hw] {} / {}, framebuffer {}x{}{}",
                hw.gl.get_parameter_string(glow::RENDERER),
                hw.gl.get_parameter_string(glow::VERSION),
                hw.size.0,
                hw.size.1,
                if SHARED_CONTEXT.get() { ", shared context" } else { "" }
            );
            hw.gl.bind_framebuffer(glow::FRAMEBUFFER, Some(hw.fbo));
            if let Some(reset) = request.context_reset {
                reset();
            }
        }
        Ok(hw)
    }

    /// (Re)allocates the framebuffer's storage for frames up to `w`×`h`. The core's context must be
    /// current. The FBO keeps its name, so the core needs no new `context_reset`.
    pub fn resize(&mut self, w: u32, h: u32) -> Result<(), String> {
        let (w, h) = (w.max(1), h.max(1));
        if (w, h) == self.size {
            return Ok(());
        }
        let gl = &self.gl;
        unsafe {
            // Unsized RGBA is valid in GLES 2 and 3 alike (RGBA8 in 3); desktop GL wants RGBA8.
            let internal = if matches!(self.request.api, Api::Gles(2, _)) { glow::RGBA } else { glow::RGBA8 };
            gl.bind_texture(glow::TEXTURE_2D, Some(self.color));
            gl.tex_image_2d(glow::TEXTURE_2D, 0, internal as i32, w as i32, h as i32, 0, glow::RGBA, glow::UNSIGNED_BYTE, None);
            for (p, v) in [
                (glow::TEXTURE_MIN_FILTER, glow::NEAREST),
                (glow::TEXTURE_MAG_FILTER, glow::NEAREST),
                (glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE),
                (glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE),
            ] {
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
                // Separate attachments: DEPTH_STENCIL_ATTACHMENT doesn't exist in GLES 2.
                gl.framebuffer_renderbuffer(glow::FRAMEBUFFER, glow::DEPTH_ATTACHMENT, glow::RENDERBUFFER, Some(rb));
                if self.request.stencil {
                    gl.framebuffer_renderbuffer(glow::FRAMEBUFFER, glow::STENCIL_ATTACHMENT, glow::RENDERBUFFER, Some(rb));
                }
            }
            let status = gl.check_framebuffer_status(glow::FRAMEBUFFER);
            gl.clear_color(0.0, 0.0, 0.0, 1.0);
            gl.clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT | glow::STENCIL_BUFFER_BIT);
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            if status != glow::FRAMEBUFFER_COMPLETE {
                return Err(format!("core framebuffer incomplete: {status:#x}"));
            }
        }
        FBO.store(self.fbo.0.get(), Ordering::Relaxed);
        self.size = (w, h);
        Ok(())
    }

    /// The colour texture the core renders into (shared with our context).
    pub fn texture(&self) -> glow::Texture {
        self.color
    }

    pub fn size(&self) -> (u32, u32) {
        self.size
    }

    /// Makes the core's context current again, after we drew.
    pub fn make_current(&self, surface: &Surface<WindowSurface>) {
        if let Err(e) = self.context.make_current(surface) {
            eprintln!("[hw] core context: {e}");
        }
    }

    /// Calls the core's `context_destroy` (once) with its context current, as RetroArch does
    /// before `retro_unload_game`. The context itself lives until pocket-core exits.
    pub fn destroy(&mut self, surface: &Surface<WindowSurface>) {
        if self.destroyed {
            return;
        }
        self.destroyed = true;
        self.make_current(surface);
        if let Some(destroy) = self.request.context_destroy {
            unsafe { destroy() };
        }
    }
}

/// Reads the core options RetroArch would use for this core, so pocket-core runs it with the same
/// settings (resolution factor, renderer, frame skip …) the system's RetroArch setup chose: the
/// per-core file `config/<Core>/<Core>.opt`, else the global `retroarch-core-options.cfg`, which
/// is what RetroArch falls back to as well. `core_so` is the core's path; its `.info` names it.
pub fn retroarch_options(core_so: &std::path::Path) -> Vec<(String, String)> {
    const RA: &str = "/storage/.config/retroarch";
    let info = core_so.with_extension("info");
    let Some(name) = std::fs::read_to_string(&info).ok().and_then(|s| {
        s.lines().find_map(|l| l.strip_prefix("corename").map(|v| v.trim().trim_start_matches('=').trim().trim_matches('"').to_string()))
    }) else {
        return Vec::new();
    };
    let per_core = format!("{RA}/config/{name}/{name}.opt");
    let text = std::fs::read_to_string(&per_core).or_else(|_| std::fs::read_to_string(format!("{RA}/retroarch-core-options.cfg"))).unwrap_or_default();
    parse_options(&text)
}

/// `key = "value"` lines of a RetroArch options file.
fn parse_options(text: &str) -> Vec<(String, String)> {
    text.lines()
        .filter_map(|l| {
            let (k, v) = l.split_once('=')?;
            let v = v.trim();
            let v = v.strip_prefix('"')?.strip_suffix('"')?;
            Some((k.trim().to_string(), v.to_string()))
        })
        .collect()
}

/// Cores that may render with OpenGL: they get RetroArch's options (see `retroarch_options`).
pub fn is_3d_core(core_so: &std::path::Path) -> bool {
    let name = core_so.file_name().and_then(|n| n.to_str()).unwrap_or("");
    ["mupen64plus", "parallel_n64", "ppsspp", "flycast", "yabasanshiro", "yabause", "beetle_saturn", "kronos", "melonds", "desmume"]
        .iter()
        .any(|c| name.starts_with(c))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_retroarch_option_lines() {
        let o = parse_options("mupen64plus-43screensize = \"640x480\"\nbogus line\nflycast_internal_resolution = \"640x480\"\nempty = \"\"\n");
        assert_eq!(o, vec![("mupen64plus-43screensize".into(), "640x480".into()), ("flycast_internal_resolution".into(), "640x480".into()), ("empty".into(), "".into())]);
    }

    #[test]
    // One test for everything that touches the global state, so parallel tests can't interleave.
    fn set_hw_render_and_frames() {
        let _l = TEST_GLOBALS.lock().unwrap_or_else(|e| e.into_inner());
        reset();
        assert!(!video_refresh(std::ptr::null(), 320, 240), "NULL is a dupe, not a hardware frame");
        assert!(video_refresh(usize::MAX as *const c_void, 640, 480));
        assert_eq!(take_frame(), Some((640, 480)));
        assert_eq!(take_frame(), None, "a frame is taken once");

        reset();
        let mut cb: HwRenderCallback = unsafe { std::mem::zeroed() };
        cb.context_type = 6; // Vulkan
        assert_eq!(unsafe { environment(ENV_SET_HW_RENDER, &mut cb as *mut _ as *mut c_void) }, Some(false));
        assert!(requested().is_none());
        cb.context_type = CONTEXT_OPENGLES3;
        cb.depth = true;
        assert_eq!(unsafe { environment(ENV_SET_HW_RENDER, &mut cb as *mut _ as *mut c_void) }, Some(true));
        assert!(cb.get_current_framebuffer.is_some() && cb.get_proc_address.is_some());
        let r = requested().unwrap();
        assert_eq!((r.api, r.depth, r.stencil), (Api::Gles(3, 0), true, false));
        cb.context_type = CONTEXT_OPENGL_CORE;
        cb.version_major = 3;
        cb.version_minor = 3;
        assert_eq!(unsafe { environment(ENV_SET_HW_RENDER, &mut cb as *mut _ as *mut c_void) }, Some(true));
        assert_eq!(requested().unwrap().api, Api::Gl { major: 3, minor: 3, compat: false });
        let mut preferred: c_uint = 0;
        assert_eq!(unsafe { environment(ENV_GET_PREFERRED_HW_RENDER, &mut preferred as *mut _ as *mut c_void) }, Some(false), "no preference");
        assert_eq!(unsafe { environment(10, std::ptr::null_mut()) }, None);
    }
}
