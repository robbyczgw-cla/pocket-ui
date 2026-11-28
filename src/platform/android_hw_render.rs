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
    minor: c_uint,
    reset: Option<Callback>,
    destroy: Option<Callback>,
    depth: bool,
    stencil: bool,
    bottom_left: bool,
}
static REQUEST: Mutex<Option<Request>> = Mutex::new(None);
static FRAME: Mutex<Option<(u32, u32)>> = Mutex::new(None);
static FBO: AtomicU32 = AtomicU32::new(0);
static READ_BUFFER_OVERRIDES: AtomicU32 = AtomicU32::new(0);
pub fn read_buffer_overrides() -> u32 { READ_BUFFER_OVERRIDES.load(Ordering::Relaxed) }
#[link(name = "EGL")]
extern "C" {
    fn eglGetProcAddress(name: *const c_char) -> *const c_void;
    fn eglGetCurrentDisplay() -> *mut c_void;
    fn eglGetCurrentContext() -> *mut c_void;
    fn eglGetError() -> i32;
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
    READ_BUFFER_OVERRIDES.store(0, Ordering::Relaxed);
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
            if !matches!(cb.kind, 2 | 4 | 5) || cb.kind == 5 && (cb.major != 3 || cb.minor > 2) {
                return Some(false);
            }
            cb.framebuffer = Some(framebuffer);
            cb.proc_address = Some(proc_address);
            *REQUEST.lock().unwrap_or_else(|e| e.into_inner()) = Some(Request {
                minor: if cb.kind == 5 { cb.minor } else { 0 },
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
fn shared_entry_points(resolve: impl Fn(*const c_char) -> *const c_void) -> bool {
    [
        c"glFenceSync",
        c"glClientWaitSync",
        c"glDeleteSync",
        c"glGenSamplers",
        c"glSamplerParameteri",
        c"glBindSampler",
        c"glDeleteSamplers",
    ].iter().all(|name| !resolve(name.as_ptr()).is_null())
}

fn created_fence(result: Result<glow::Fence, String>) -> Option<glow::Fence> {
    // glow's native wrapper returns Ok even when the driver returns NULL.
    result.ok().filter(|fence| !fence.0.is_null())
}

/// The core may have a worker inside its library and its GPU objects are gone.
/// End the separate :player process without running core or GL destructors.
/// The launcher can reopen the last successfully saved moment.
pub fn stop_lost_player() -> ! {
    let error = unsafe { eglGetError() };
    eprintln!("Player EGL context is unavailable ({error:#x})");
    #[cfg(target_os = "android")]
    unsafe { libc::_exit(0) }
    #[cfg(not(target_os = "android"))]
    panic!("Player EGL context is unavailable ({error:#x})");
}

/// The texture is owned by this core session and shared with the Activity's GLES context.
#[derive(Clone, Copy)]
pub struct Picture {
    pub texture: glow::Texture,
    pub size: (u32, u32),
    pub bottom_left: bool,
    pub texture_size: (u32, u32),
    serial: u64,
}

#[derive(Clone, Copy)]
struct Crop {
    texture: glow::Texture,
    size: (u32, u32),
    serial: u64,
}

/// All methods are called on the Activity's EGL thread. The core may retain arbitrary GL state.
pub struct HwContext {
    display: *mut c_void,
    frontend: *mut c_void,
    detached_cleanup: Cell<bool>,
    core: *mut c_void,
    draw: Cell<*mut c_void>,
    read: Cell<*mut c_void>,
    gl: glow::Context,
    fbo: glow::Framebuffer,
    render_color: glow::Texture,
    held: Option<(glow::Texture, glow::Framebuffer)>,
    shared: Cell<bool>,
    cropped: Cell<Option<Crop>>,
    serial: Cell<u64>,
    captures: Cell<u64>,
    produced: Cell<Option<glow::Fence>>,
    sampled: Cell<Option<glow::Fence>>,
    depth: Option<glow::Renderbuffer>,
    request: Request,
    size: (u32, u32),
    initialized: bool,
}
impl HwContext {
    #[cfg(all(test, not(target_os = "android")))]
    pub(crate) fn for_test(width: u32, height: u32, bottom_left: bool) -> Self {
        reset();
        let mut cb = CallbackInfo {
            kind: 4, reset: None, framebuffer: None, proc_address: None,
            depth: true, stencil: false, bottom_left, major: 3, minor: 0,
            cache: false, destroy: None, debug: false,
        };
        assert_eq!(unsafe { environment(14, (&mut cb as *mut CallbackInfo).cast()) }, Some(true));
        Self::create(width, height).unwrap().unwrap()
    }

    pub fn use_cpu_capture(&self) {
        self.shared.set(false);
    }

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
            let core = eglCreateContext(display, config, frontend, [0x3098, 3, 0x30fb, request.minor as i32, 0x3038].as_ptr());
            if core.is_null() {
                return Err(format!("{} 3.{}", crate::i18n::tr("This core needs OpenGL ES"), request.minor));
            }
            if eglMakeCurrent(display, draw, read, core) == 0 {
                eglDestroyContext(display, core);
                return Err("Cannot activate core EGL context".into());
            }
            let gl = glow::Context::from_loader_function_cstr(|n| eglGetProcAddress(n.as_ptr()));
            // Once the EGL context exists, hold it in an owner before allocating GL objects.
            let allocation = (|| -> Result<_, String> {
                let fbo = gl.create_framebuffer()?;
                let allocate_color = || -> Result<glow::Texture, String> {
                    let color = gl.create_texture()?;
                    gl.bind_texture(glow::TEXTURE_2D, Some(color));
                    gl.tex_image_2d(
                        glow::TEXTURE_2D, 0, glow::RGBA8 as i32, width as i32, height as i32, 0,
                        glow::RGBA, glow::UNSIGNED_BYTE, None,
                    );
                    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::NEAREST as i32);
                    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::NEAREST as i32);
                    if gl.get_error() != glow::NO_ERROR {
                        gl.delete_texture(color);
                        return Err("Cannot allocate a GLES game texture".into());
                    }
                    Ok(color)
                };
                let render_color = allocate_color()?;
                gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
                gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(render_color), 0);
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
                // Keep the core's attachment unchanged for cores that update only part of it.
                // The spare texture is optional. A valid core FBO can still use CPU capture.
                let held = shared_entry_points(|name| eglGetProcAddress(name))
                    .then(allocate_color).and_then(Result::ok).and_then(|texture| {
                    let Ok(held_fbo) = gl.create_framebuffer() else {
                        gl.delete_texture(texture);
                        return None;
                    };
                    gl.bind_framebuffer(glow::FRAMEBUFFER, Some(held_fbo));
                    gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(texture), 0);
                    if gl.check_framebuffer_status(glow::FRAMEBUFFER) != glow::FRAMEBUFFER_COMPLETE {
                        gl.delete_framebuffer(held_fbo);
                        gl.delete_texture(texture);
                        return None;
                    }
                    Some((texture, held_fbo))
                });
                gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
                gl.bind_texture(glow::TEXTURE_2D, Some(render_color));
                Ok((fbo, render_color, held, depth))
            })();
            let (fbo, render_color, held, depth) = match allocation {
                Ok(v) => v,
                Err(e) => {
                    eglMakeCurrent(display, draw, read, frontend);
                    eglDestroyContext(display, core);
                    return Err(e);
                }
            };
            let mut result = Self {
                display,
                frontend,
                detached_cleanup: Cell::new(false),
                core,
                draw: Cell::new(draw),
                read: Cell::new(read),
                gl,
                fbo,
                render_color,
                held,
                shared: Cell::new(held.is_some()),
                cropped: Cell::new(None),
                serial: Cell::new(0),
                captures: Cell::new(0),
                produced: Cell::new(None),
                sampled: Cell::new(None),
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
    /// EGL may replace the window surface without replacing the share group.
    /// A different frontend context is a new session, even on the same thread.
    pub fn try_core_current(&self) -> bool {
        unsafe {
            let current = eglGetCurrentContext();
            if current == self.core {
                return true;
            }
            if current != self.frontend {
                return false;
            }
            let (draw, read) = (eglGetCurrentSurface(0x3059), eglGetCurrentSurface(0x305a));
            self.complete(&self.sampled);
            if eglMakeCurrent(self.display, draw, read, self.core) == 0 {
                return false;
            }
            self.draw.set(draw);
            self.read.set(read);
            true
        }
    }

    pub fn core_current(&self) {
        if !self.try_core_current() {
            stop_lost_player();
        }
    }

    /// GLSurfaceView can release its window before dropping the player thread.
    /// Finish normal core cleanup using the old context without its window if
    /// the driver still owns that context.
    pub fn activate_for_cleanup(&self) -> bool {
        if self.try_core_current() {
            return true;
        }
        unsafe {
            if !eglGetCurrentContext().is_null() || eglMakeCurrent(self.display, ptr::null_mut(), ptr::null_mut(), self.core) == 0 {
                return false;
            }
        }
        self.detached_cleanup.set(true);
        self.complete(&self.sampled);
        true
    }

    pub fn frontend_current(&self) {
        unsafe {
            if eglGetCurrentContext() != self.core {
                return;
            }
            self.complete(&self.produced);
            if eglMakeCurrent(self.display, self.draw.get(), self.read.get(), self.frontend) == 0 {
                stop_lost_player();
            }
        }
    }

    pub fn shared(&self) -> bool {
        self.shared.get()
    }

    /// Consume a new hardware delivery. The GPU copy keeps the last accepted
    /// image intact even when a core writes to its target and then duplicates.
    pub fn picture(&self) -> Option<Picture> {
        let size = FRAME.lock().unwrap_or_else(|e| e.into_inner()).take()?;
        let (width, height) = size;
        if width == 0 || height == 0 || width > self.size.0 || height > self.size.1 {
            return None;
        }
        self.core_current();
        let mut texture = self.render_color;
        if let Some((held, _)) = self.held.filter(|_| self.shared()) {
            if self.copy_to(self.fbo, held, size, false) {
                self.fence(&self.produced);
                if self.shared() {
                    texture = held;
                }
            } else {
                self.shared.set(false);
                eprintln!("Player shared GLES image copy failed; using CPU capture");
            }
        }
        let serial = self.serial.get() + 1;
        self.serial.set(serial);
        Some(Picture { texture, size, bottom_left: self.request.bottom_left, texture_size: self.size, serial })
    }

    fn copy_to(&self, source: glow::Framebuffer, texture: glow::Texture, size: (u32, u32), allocate: bool) -> bool {
        let gl = &self.gl;
        unsafe {
            let bound = gl.get_parameter_i32(glow::TEXTURE_BINDING_2D);
            gl.bind_texture(glow::TEXTURE_2D, Some(texture));
            self.with_color_attachment(source, || {
                let (w, h) = (size.0 as i32, size.1 as i32);
                if allocate {
                    gl.copy_tex_image_2d(glow::TEXTURE_2D, 0, glow::RGBA8, 0, 0, w, h, 0);
                } else {
                    gl.copy_tex_sub_image_2d(glow::TEXTURE_2D, 0, 0, 0, 0, 0, w, h);
                }
            });
            gl.bind_texture(glow::TEXTURE_2D, std::num::NonZeroU32::new(bound as u32).map(glow::NativeTexture));
            match gl.get_error() {
                glow::NO_ERROR => true,
                glow::CONTEXT_LOST => stop_lost_player(),
                _ => false,
            }
        }
    }

    /// Mipmaps use a separate texture so generating them cannot change the
    /// core's framebuffer storage or sample padding outside the visible area.
    pub fn picture_for_mipmaps(&self, picture: Picture) -> Option<Picture> {
        let gl = &self.gl;
        let mut crop = match self.cropped.get() {
            Some(crop) if crop.serial == picture.serial => {
                return Some(Picture { texture: crop.texture, texture_size: crop.size, ..picture });
            }
            Some(crop) => crop,
            None => {
                self.core_current();
                let texture = unsafe { gl.create_texture().ok()? };
                Crop { texture, size: (0, 0), serial: 0 }
            }
        };
        self.core_current();
        if !self.copy_to(self.read_fbo(picture), crop.texture, picture.size, picture.size != crop.size) {
            self.cropped.set(Some(crop));
            return None;
        }
        crop.size = picture.size;
        crop.serial = picture.serial;
        self.cropped.set(Some(crop));
        self.fence(&self.produced);
        self.shared().then_some(Picture { texture: crop.texture, texture_size: crop.size, ..picture })
    }

    fn fence(&self, pending: &Cell<Option<glow::Fence>>) {
        unsafe {
            if let Some(old) = pending.take() {
                self.gl.delete_sync(old);
            }
            match created_fence(self.gl.fence_sync(glow::SYNC_GPU_COMMANDS_COMPLETE, 0)) {
                Some(fence) => {
                    self.gl.flush();
                    pending.set(Some(fence));
                }
                None => self.finish_for_readback(),
            }
        }
    }

    fn complete(&self, pending: &Cell<Option<glow::Fence>>) {
        let Some(fence) = pending.take() else { return };
        unsafe {
            let budget = 250_000_000;
            let mut wait = self.gl.client_wait_sync(fence, glow::SYNC_FLUSH_COMMANDS_BIT, budget);
            if wait == glow::TIMEOUT_EXPIRED {
                self.shared.set(false);
                eprintln!("Player EGL fence exceeded its frame budget; using CPU capture");
                // Give the pending work one last bounded chance before any
                // readback or drawing can touch the same texture.
                wait = self.gl.client_wait_sync(fence, glow::SYNC_FLUSH_COMMANDS_BIT, budget);
            }
            self.gl.delete_sync(fence);
            match wait {
                glow::ALREADY_SIGNALED | glow::CONDITION_SATISFIED => (),
                glow::WAIT_FAILED => self.finish_for_readback(),
                _ => {
                    eprintln!("Player EGL frame fence still did not signal ({wait:#x})");
                    stop_lost_player();
                }
            }
        }
    }

    fn finish_for_readback(&self) {
        self.shared.set(false);
        unsafe {
            self.gl.finish();
            if self.gl.get_error() == glow::CONTEXT_LOST {
                stop_lost_player();
            }
        }
        eprintln!("Player EGL sync is unavailable; using CPU capture");
    }

    /// Called with the Activity context current, after the game texture was sampled.
    pub fn after_sampling(&self) {
        if self.shared() {
            self.fence(&self.sampled);
        } else {
            self.finish_for_readback();
        }
    }

    fn read_fbo(&self, picture: Picture) -> glow::Framebuffer {
        self.held.filter(|(texture, _)| *texture == picture.texture).map_or(self.fbo, |(_, held_fbo)| held_fbo)
    }

    fn with_color_attachment(&self, fbo: glow::Framebuffer, operation: impl FnOnce()) {
        unsafe {
            let gl = &self.gl;
            let old = gl.get_parameter_i32(glow::READ_FRAMEBUFFER_BINDING);
            gl.bind_framebuffer(glow::READ_FRAMEBUFFER, Some(fbo));
            let read_buffer = gl.get_parameter_i32(glow::READ_BUFFER);
            if read_buffer != glow::COLOR_ATTACHMENT0 as i32 {
                READ_BUFFER_OVERRIDES.fetch_add(1, Ordering::Relaxed);
                gl.read_buffer(glow::COLOR_ATTACHMENT0);
            }
            operation();
            gl.read_buffer(read_buffer as u32);
            gl.bind_framebuffer(glow::READ_FRAMEBUFFER, std::num::NonZeroU32::new(old as u32).map(glow::NativeFramebuffer));
        }
    }

    pub fn captures(&self) -> u64 { self.captures.get() }

    /// Read the latest picture on demand for a CPU preview or a screenshot.
    pub fn capture(&self, picture: Picture) -> crate::libretro::Frame {
        let (width, height) = picture.size;
        self.core_current();
        let mut data = vec![0; width as usize * height as usize * 4];
        self.captures.set(self.captures.get() + 1);
        self.with_color_attachment(self.read_fbo(picture), || unsafe {
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
        });
        for pixel in data.chunks_exact_mut(4) {
            pixel.swap(0, 2);
        }
        if picture.bottom_left {
            let stride = width as usize * 4;
            for y in 0..height as usize / 2 {
                let (a, b) = data.split_at_mut((height as usize - 1 - y) * stride);
                a[y * stride..(y + 1) * stride].swap_with_slice(&mut b[..stride]);
            }
        }
        crate::libretro::Frame {
            width,
            height,
            pitch: width as usize * 4,
            format: crate::libretro::PIXEL_XRGB8888,
            data,
        }
    }
    pub fn destroy_callback(&mut self) {
        if self.initialized && self.activate_for_cleanup() {
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
        if !self.activate_for_cleanup() {
            unsafe { eglDestroyContext(self.display, self.core); }
            FBO.store(0, Ordering::Relaxed);
            return;
        }
        self.destroy_callback();
        unsafe {
            self.gl.delete_framebuffer(self.fbo);
            self.gl.delete_texture(self.render_color);
            if let Some((texture, fbo)) = self.held {
                self.gl.delete_framebuffer(fbo);
                self.gl.delete_texture(texture);
            }
            if let Some(crop) = self.cropped.take() {
                self.gl.delete_texture(crop.texture);
            }
            if let Some(fence) = self.produced.take() {
                self.gl.delete_sync(fence);
            }
            if let Some(d) = self.depth {
                self.gl.delete_renderbuffer(d);
            }
            if self.detached_cleanup.get() {
                eglMakeCurrent(self.display, ptr::null_mut(), ptr::null_mut(), ptr::null_mut());
            } else {
                self.frontend_current();
            }
            eglDestroyContext(self.display, self.core);
        }
        FBO.store(0, Ordering::Relaxed);
    }
}


#[cfg(test)]
mod shared_tests {
    use super::*;

    #[test]
    fn missing_shared_entry_points_and_null_fences_select_cpu_capture() {
        let non_null = std::ptr::NonNull::<u8>::dangling().as_ptr();
        assert!(shared_entry_points(|_| non_null.cast()));
        assert!(!shared_entry_points(|name| {
            if unsafe { std::ffi::CStr::from_ptr(name) } == c"glClientWaitSync" {
                ptr::null()
            } else {
                non_null.cast()
            }
        }));
        assert!(created_fence(Ok(glow::NativeFence(ptr::null_mut()))).is_none());
        assert!(created_fence(Ok(glow::NativeFence(non_null.cast()))).is_some());
    }
}
