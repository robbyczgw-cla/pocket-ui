//! GLES negotiation for the private Android player. The core owns a shared EGL context;
//! the Activity's context is restored before composing the launcher-owned controls.
use glow::HasContext;
use std::{
    cell::Cell,
    ffi::{c_char, c_uint, c_void},
    ptr,
    sync::{
        atomic::{AtomicU32, Ordering},
        Mutex,
    },
};
type Callback = unsafe extern "C" fn();
#[repr(C)]
struct CallbackInfo {
    kind: c_uint,
    reset: Option<Callback>,
    framebuffer: Option<unsafe extern "C" fn() -> usize>,
    proc_address: Option<unsafe extern "C" fn(*const c_char) -> *const c_void>,
    depth: bool,
    stencil: bool,
    bottom_left: bool,
    major: c_uint,
    minor: c_uint,
    cache: bool,
    destroy: Option<Callback>,
    debug: bool,
}
#[derive(Clone, Copy)]
struct Request {
    reset: Option<Callback>,
    destroy: Option<Callback>,
    depth: bool,
    stencil: bool,
    bottom_left: bool,
}
static REQUEST: Mutex<Option<Request>> = Mutex::new(None);
static FRAME: Mutex<Option<(u32, u32)>> = Mutex::new(None);
static FBO: AtomicU32 = AtomicU32::new(0);
#[link(name = "EGL")]
extern "C" {
    fn eglGetProcAddress(name: *const c_char) -> *const c_void;
    fn eglGetCurrentDisplay() -> *mut c_void;
    fn eglGetCurrentContext() -> *mut c_void;
    fn eglGetCurrentSurface(which: i32) -> *mut c_void;
    fn eglQueryContext(display: *mut c_void, context: *mut c_void, attribute: i32, value: *mut i32) -> u32;
    fn eglChooseConfig(display: *mut c_void, attributes: *const i32, configs: *mut *mut c_void, count: i32, found: *mut i32) -> u32;
    fn eglCreateContext(display: *mut c_void, config: *mut c_void, share: *mut c_void, attributes: *const i32) -> *mut c_void;
    fn eglMakeCurrent(display: *mut c_void, draw: *mut c_void, read: *mut c_void, context: *mut c_void) -> u32;
    fn eglDestroyContext(display: *mut c_void, context: *mut c_void) -> u32;
}
unsafe extern "C" fn framebuffer() -> usize {
    FBO.load(Ordering::Relaxed) as usize
}
unsafe extern "C" fn proc_address(name: *const c_char) -> *const c_void {
    if name.is_null() {
        ptr::null()
    } else {
        eglGetProcAddress(name)
    }
}
unsafe extern "C" fn clear_waits(_: c_uint, _: *mut c_void) -> bool {
    true
}
pub fn reset() {
    *REQUEST.lock().unwrap_or_else(|e| e.into_inner()) = None;
    *FRAME.lock().unwrap_or_else(|e| e.into_inner()) = None;
    FBO.store(0, Ordering::Relaxed);
}
pub fn video_refresh(data: *const c_void, width: c_uint, height: c_uint) -> bool {
    if data as usize != usize::MAX {
        return false;
    }
    *FRAME.lock().unwrap_or_else(|e| e.into_inner()) = Some((width, height));
    true
}
/// # Safety
/// `data` follows the libretro environment command's ABI.
pub unsafe fn environment(cmd: c_uint, data: *mut c_void) -> Option<bool> {
    Some(match cmd {
        14 => {
            if data.is_null() {
                return Some(false);
            }
            let cb = &mut *data.cast::<CallbackInfo>();
            if !matches!(cb.kind, 2 | 4 | 5) || cb.kind == 5 && (cb.major > 3 || cb.major == 3 && cb.minor > 0) {
                return Some(false);
            }
            cb.framebuffer = Some(framebuffer);
            cb.proc_address = Some(proc_address);
            *REQUEST.lock().unwrap_or_else(|e| e.into_inner()) = Some(Request {
                reset: cb.reset,
                destroy: cb.destroy,
                depth: cb.depth,
                stencil: cb.stencil,
                bottom_left: cb.bottom_left,
            });
            true
        }
        56 => {
            if data.is_null() {
                false
            } else {
                *data.cast::<c_uint>() = 4;
                true
            }
        }
        0x1002c => true,
        0x800003 => {
            if data.is_null() {
                false
            } else {
                *data.cast::<Option<unsafe extern "C" fn(c_uint, *mut c_void) -> bool>>() = Some(clear_waits);
                true
            }
        }
        0x10029 | 0x1002b => false,
        _ => return None,
    })
}
/// All methods are called on the Activity's EGL thread. The core may retain arbitrary GL state.
pub struct HwContext {
    display: *mut c_void,
    frontend: Cell<*mut c_void>,
    core: *mut c_void,
    draw: Cell<*mut c_void>,
    read: Cell<*mut c_void>,
    gl: glow::Context,
    fbo: glow::Framebuffer,
    color: glow::Texture,
    depth: Option<glow::Renderbuffer>,
    request: Request,
    size: (u32, u32),
    initialized: bool,
}
impl HwContext {
    pub fn create(width: u32, height: u32) -> Result<Option<Self>, String> {
        let Some(request) = *REQUEST.lock().unwrap_or_else(|e| e.into_inner()) else {
            return Ok(None);
        };
        let (width, height) = (width.max(1), height.max(1));
        if width > 4096 || height > 4096 {
            return Err("Core framebuffer exceeds 4096 pixels".into());
        }
        unsafe {
            let display = eglGetCurrentDisplay();
            let frontend = eglGetCurrentContext();
            let (draw, read) = (eglGetCurrentSurface(0x3059), eglGetCurrentSurface(0x305a));
            let mut id = 0;
            let mut count = 0;
            let mut config = ptr::null_mut();
            if display.is_null()
                || frontend.is_null()
                || eglQueryContext(display, frontend, 0x3028, &mut id) == 0
                || eglChooseConfig(display, [0x3028, id, 0x3038].as_ptr(), &mut config, 1, &mut count) == 0
                || count != 1
            {
                return Err("Cannot find the Activity EGL config".into());
            }
            let core = eglCreateContext(display, config, frontend, [0x3098, 3, 0x3038].as_ptr());
            if core.is_null() {
                return Err("Cannot create the core GLES 3 context".into());
            }
            if eglMakeCurrent(display, draw, read, core) == 0 {
                eglDestroyContext(display, core);
                return Err("Cannot activate core EGL context".into());
            }
            let gl = glow::Context::from_loader_function_cstr(|n| eglGetProcAddress(n.as_ptr()));
            // Once the EGL context exists, hold it in an owner before allocating GL objects.
            let allocation = (|| -> Result<_, String> {
                let fbo = gl.create_framebuffer()?;
                let color = gl.create_texture()?;
                gl.bind_texture(glow::TEXTURE_2D, Some(color));
                gl.tex_image_2d(
                    glow::TEXTURE_2D,
                    0,
                    glow::RGBA8 as i32,
                    width as i32,
                    height as i32,
                    0,
                    glow::RGBA,
                    glow::UNSIGNED_BYTE,
                    None,
                );
                gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::NEAREST as i32);
                gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::NEAREST as i32);
                gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
                gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(color), 0);
                let depth = if request.depth || request.stencil {
                    let d = gl.create_renderbuffer()?;
                    gl.bind_renderbuffer(glow::RENDERBUFFER, Some(d));
                    let (format, attachment) = if request.stencil {
                        (glow::DEPTH24_STENCIL8, glow::DEPTH_STENCIL_ATTACHMENT)
                    } else {
                        (glow::DEPTH_COMPONENT24, glow::DEPTH_ATTACHMENT)
                    };
                    gl.renderbuffer_storage(glow::RENDERBUFFER, format, width as i32, height as i32);
                    gl.framebuffer_renderbuffer(glow::FRAMEBUFFER, attachment, glow::RENDERBUFFER, Some(d));
                    Some(d)
                } else {
                    None
                };
                if gl.check_framebuffer_status(glow::FRAMEBUFFER) != glow::FRAMEBUFFER_COMPLETE {
                    return Err("Incomplete core framebuffer".into());
                }
                Ok((fbo, color, depth))
            })();
            let (fbo, color, depth) = match allocation {
                Ok(v) => v,
                Err(e) => {
                    eglMakeCurrent(display, draw, read, frontend);
                    eglDestroyContext(display, core);
                    return Err(e);
                }
            };
            let mut result = Self {
                display,
                frontend: Cell::new(frontend),
                core,
                draw: Cell::new(draw),
                read: Cell::new(read),
                gl,
                fbo,
                color,
                depth,
                request,
                size: (width, height),
                initialized: false,
            };
            FBO.store(fbo.0.get(), Ordering::Relaxed);
            if let Some(reset) = request.reset {
                reset();
            }
            result.initialized = true;
            result.frontend_current();
            eprintln!("Android core GLES 3 framebuffer: {width}x{height}");
            Ok(Some(result))
        }
    }
    pub fn core_current(&self) {
        unsafe {
            if eglGetCurrentContext() != self.core {
                // GLSurfaceView can replace its window surface on rotation/resume.
                self.frontend.set(eglGetCurrentContext());
                self.draw.set(eglGetCurrentSurface(0x3059));
                self.read.set(eglGetCurrentSurface(0x305a));
                assert_ne!(
                    eglMakeCurrent(self.display, self.draw.get(), self.read.get(), self.core),
                    0,
                    "Core EGL context was lost"
                );
            }
        }
    }
    pub fn frontend_current(&self) {
        unsafe {
            if eglGetCurrentContext() != self.core {
                return;
            }
            assert_ne!(
                eglMakeCurrent(self.display, self.draw.get(), self.read.get(), self.frontend.get()),
                0,
                "Activity EGL context was lost"
            );
        }
    }
    /// First GPU milestone uses a bounded native-size readback so pause previews and screenshots
    /// share the software frame path. Direct shared-texture composition can replace this copy.
    pub fn capture(&self) -> Option<crate::libretro::Frame> {
        let (width, height) = FRAME.lock().unwrap_or_else(|e| e.into_inner()).take()?;
        if width == 0 || height == 0 || width > self.size.0 || height > self.size.1 {
            return None;
        }
        self.core_current();
        let mut data = vec![0; width as usize * height as usize * 4];
        unsafe {
            let old = self.gl.get_parameter_i32(glow::READ_FRAMEBUFFER_BINDING);
            self.gl.bind_framebuffer(glow::READ_FRAMEBUFFER, Some(self.fbo));
            let old_pack = self.gl.get_parameter_i32(glow::PIXEL_PACK_BUFFER_BINDING);
            let packing =
                [glow::PACK_ALIGNMENT, glow::PACK_ROW_LENGTH, glow::PACK_SKIP_ROWS, glow::PACK_SKIP_PIXELS].map(|p| (p, self.gl.get_parameter_i32(p)));
            self.gl.bind_buffer(glow::PIXEL_PACK_BUFFER, None);
            for (parameter, _) in packing {
                self.gl.pixel_store_i32(parameter, if parameter == glow::PACK_ALIGNMENT { 1 } else { 0 });
            }
            self.gl.read_pixels(
                0,
                0,
                width as i32,
                height as i32,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelPackData::Slice(&mut data),
            );
            for (parameter, value) in packing {
                self.gl.pixel_store_i32(parameter, value);
            }
            self.gl
                .bind_buffer(glow::PIXEL_PACK_BUFFER, std::num::NonZeroU32::new(old_pack as u32).map(glow::NativeBuffer));
            self.gl
                .bind_framebuffer(glow::READ_FRAMEBUFFER, std::num::NonZeroU32::new(old as u32).map(glow::NativeFramebuffer));
        }
        for pixel in data.chunks_exact_mut(4) {
            pixel.swap(0, 2);
        }
        if self.request.bottom_left {
            let stride = width as usize * 4;
            for y in 0..height as usize / 2 {
                let (a, b) = data.split_at_mut((height as usize - 1 - y) * stride);
                a[y * stride..(y + 1) * stride].swap_with_slice(&mut b[..stride]);
            }
        }
        Some(crate::libretro::Frame {
            width,
            height,
            pitch: width as usize * 4,
            format: crate::libretro::PIXEL_XRGB8888,
            data,
        })
    }
    pub fn destroy_callback(&mut self) {
        if self.initialized {
            self.core_current();
            if let Some(destroy) = self.request.destroy {
                unsafe {
                    destroy();
                }
            }
            self.initialized = false;
        }
    }
}
impl Drop for HwContext {
    fn drop(&mut self) {
        self.destroy_callback();
        self.core_current();
        unsafe {
            self.gl.delete_framebuffer(self.fbo);
            self.gl.delete_texture(self.color);
            if let Some(d) = self.depth {
                self.gl.delete_renderbuffer(d);
            }
            self.frontend_current();
            eglDestroyContext(self.display, self.core);
        }
        FBO.store(0, Ordering::Relaxed);
    }
}
