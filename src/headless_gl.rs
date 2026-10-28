//! An OpenGL ES 3 context without a window or a display server: EGL's device platform (Mesa's software device on a
//! development machine, llvmpipe), made current without a surface. For tests and tools that render pocket-core's
//! picture offscreen (`tests/picture_shaders.rs`, `examples/picture_shots.rs`); pocket-core itself never uses it.

use std::ffi::CString;

use glutin::api::egl::{context::PossiblyCurrentContext, device::Device, display::Display};
use glutin::config::{ConfigSurfaceTypes, ConfigTemplateBuilder};
use glutin::context::{ContextApi, ContextAttributesBuilder, Version};
use glutin::display::GlDisplay;

/// Keeps the context current and alive: hold it as long as the `glow::Context` from `context` is used.
pub struct Headless {
    /// GL_RENDERER, for the record ("llvmpipe (LLVM 20.1.2, 256 bits)").
    pub renderer: String,
    _context: PossiblyCurrentContext,
    _display: Display,
}

/// The first EGL device that gives a GLES 3 context, current on this thread. An error says why there is none (no
/// libEGL, no device platform, no GLES 3), so a caller can skip with a reason.
pub fn context() -> Result<(glow::Context, Headless), String> {
    let devices = Device::query_devices().map_err(|e| format!("no EGL devices: {e}"))?;
    let mut why = String::from("no EGL device");
    for device in devices {
        match unsafe { try_device(&device) } {
            Ok(h) => return Ok(h),
            Err(e) => why = e,
        }
    }
    Err(why)
}

unsafe fn try_device(device: &Device) -> Result<(glow::Context, Headless), String> {
    let display = Display::with_device(device, None).map_err(|e| format!("EGL display: {e}"))?;
    let template = ConfigTemplateBuilder::new().with_surface_type(ConfigSurfaceTypes::empty()).build();
    let config = display.find_configs(template).map_err(|e| format!("EGL configs: {e}"))?.next().ok_or("no EGL config")?;
    let attrs = ContextAttributesBuilder::new().with_context_api(ContextApi::Gles(Some(Version::new(3, 0)))).build(None);
    let context = display.create_context(&config, &attrs).map_err(|e| format!("GLES 3 context: {e}"))?;
    let context = context.make_current_surfaceless().map_err(|e| format!("make current: {e}"))?;
    let gl = glow::Context::from_loader_function(|name| {
        let name = CString::new(name).unwrap();
        display.get_proc_address(&name)
    });
    let renderer = glow::HasContext::get_parameter_string(&gl, glow::RENDERER);
    Ok((gl, Headless { renderer, _context: context, _display: display }))
}
