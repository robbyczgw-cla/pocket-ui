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
        let egl = Egl::open("pbuffer", (16, 16), &mut |line| eprintln!("[hw] {line}")).map_err(|f| format!("[hw] GL context: {f}"))?;
        if !egl.es3 && matches!(request.api, Api::Gles(3, _)) {
            return Err("the core wants GLES 3; only GLES 2 here".into());
        }
        // SAFETY: the context of `egl` is current on this thread from here on.
        let (fbo, color) = unsafe { (egl.gl.create_framebuffer()?, egl.gl.create_texture()?) };
        let mut hw = BrickGles { egl, request, fbo, color, depth: None, size: (0, 0), rgba: Vec::new(), destroyed: false, bgra: false, frames: 0, cost: Cost::default() };
        hw.resize(max)?;
        // SAFETY: the context is current, our framebuffer bound (`resize`): the format is the one its reads have without converting.
        hw.bgra = unsafe { (hw.egl.gl.get_parameter_i32(glow::IMPLEMENTATION_COLOR_READ_FORMAT) as u32, hw.egl.gl.get_parameter_i32(glow::IMPLEMENTATION_COLOR_READ_TYPE) as u32) == (glow::BGRA, glow::UNSIGNED_BYTE) };
        eprintln!("[hw] {:?} for the core on the vendor's EGL, framebuffer {}x{}, read back to the CPU ({})", request.api, hw.size.0, hw.size.1, hw.way());
        // SAFETY: the core's context is current; the core binds its framebuffer through `get_current_framebuffer`.
        unsafe {
            hw.egl.gl.bind_framebuffer(glow::FRAMEBUFFER, Some(hw.fbo));
            hw.request.reset_context();
        }
        Ok(hw)
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
        // SAFETY: the core's context is current.
        unsafe { self.request.destroy_context() };
    }
}


#[cfg(test)]
mod tests {
    use super::upright_xrgb;

    /// Two rows of two pixels, red green / blue white, as the GPU's bottom row first.
    const RGBA_BOTTOM_FIRST: [u8; 16] = [0, 0, 255, 255, 255, 255, 255, 255, 255, 0, 0, 255, 0, 255, 0, 255];

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
