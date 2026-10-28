//! pocket-core's picture: uploads each core frame into a GLES 3 texture and draws it as one quad,
//! sized and filtered by the pause menu's Picture settings (scaling, sharpness, effect).
//!
//! Everything happens in one fragment shader pass; sharpness and effect select a compile-time
//! variant (see `src/shaders/video.frag`, its numbers in `picture_fx.rs`) so a plain game pays for nothing it does
//! not use. Variants are compiled on first use, which is when the menu applies a change, not per frame.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use glow::HasContext;

use crate::libretro::{Frame, PIXEL_0RGB1555, PIXEL_XRGB8888};
use crate::options::GameSettings;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Scaling {
    /// Whole-number scale of the picture height, in the core's aspect ratio.
    #[default]
    Native,
    /// One whole-number step below Native (GBA 4x instead of 5x), so a frame has room. Never
    /// below 1x.
    IntegerSmaller,
    /// As large as fits, in the core's aspect ratio.
    Aspect,
    /// As large as fits, in the screen's aspect ratio. The viewport is the screen, so this fills
    /// it, like Fullscreen; they differ only when the viewport is not the whole panel.
    /// The whole viewport; pixels stretch.
    Fullscreen,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum Sharpness {
    /// Nearest neighbour.
    #[default]
    Sharp,
    /// Sharp bilinear: flat texels, one output pixel of blending at their edges, in linear light.
    Crisp,
    /// Plain bilinear.
    Soft,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum Effect {
    #[default]
    None,
    Grid,
    Scanlines,
    Crt,
}

/// Options → Picture → Screen orientation: how the player holds the Pocket, for games made for an upright screen
/// (vertical shooters, WonderSwan games played upright, DS books). The picture is drawn turned the other way on the
/// panel so it stands upright for the player, on top of any turn the core asks for (SET_ROTATION); the D-pad and sticks
/// of the Pocket's own controls turn with it (`controls::turn_buttons`).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Orientation {
    #[default]
    Normal,
    /// The Pocket turned a quarter to the left (counter-clockwise): the D-pad at the bottom.
    Left,
    /// The Pocket turned a quarter to the right (clockwise): the D-pad on top.
    Right,
}

impl Orientation {
    pub fn parse(s: &str) -> Self {
        match s {
            "left" => Self::Left,
            "right" => Self::Right,
            _ => Self::Normal,
        }
    }

    /// Quarter turns counter-clockwise the picture gets on the panel (the shader's `rot`): a Pocket turned right
    /// turns everything on it clockwise, so the picture is drawn turned counter-clockwise by as much.
    pub fn turns(self) -> u32 {
        match self {
            Self::Normal => 0,
            Self::Right => 1,
            Self::Left => 3,
        }
    }
}

/// The shape a turned frame takes on the panel: (width, height, aspect) to scale (`dest_rect`), for a `fw`×`fh` frame
/// the core shows at `aspect`, turned `core` quarter turns by the core's request and `user` more by Screen
/// orientation. A core that turns its picture gives the aspect of the picture as it is meant to be seen, upright
/// (FBNeo's vertical games 3:4, Beetle WonderSwan's portrait mode 9:14, as RetroArch reads it), so only the user's
/// turn swaps the aspect; the frame's own sides swap with every odd turn. An aspect of 0 or less (the frame's own
/// shape) stays so and follows the swapped sides.
pub fn turned_shape(fw: u32, fh: u32, aspect: f32, core: u32, user: u32) -> (u32, u32, f32) {
    let (w, h) = if (core + user) & 1 == 1 { (fh, fw) } else { (fw, fh) };
    let aspect = if user & 1 == 1 && aspect > 0.0 { 1.0 / aspect } else { aspect };
    (w, h, aspect)
}

/// Where a point of the turned picture lies in the frame: `(x, y)` as fractions (0..1, y down) of the picture's
/// rectangle on the panel, `turns` quarter turns counter-clockwise, to fractions of the frame. The same mapping as the
/// vertex shader's (`shaders/video.vert`), for touch.
pub fn frame_point(turns: u32, x: f32, y: f32) -> (f32, f32) {
    match turns & 3 {
        1 => (1.0 - y, x),
        2 => (1.0 - x, 1.0 - y),
        3 => (y, 1.0 - x),
        _ => (x, y),
    }
}

/// A `w`×`h` picture (rows top to bottom) turned `turns` quarter turns counter-clockwise, as the renderer draws it
/// (`frame_point`): (width, height, pixels). For the pause menu's preview stand-in in snapshots.
pub fn turn_pixels<T: Copy>(w: u32, h: u32, px: &[T], turns: u32) -> (u32, u32, Vec<T>) {
    let (tw, th) = if turns & 1 == 1 { (h, w) } else { (w, h) };
    let out = (0..th)
        .flat_map(|y| (0..tw).map(move |x| (x, y)))
        .map(|(x, y)| {
            let (fx, fy) = frame_point(turns, (x as f32 + 0.5) / tw as f32, (y as f32 + 0.5) / th as f32);
            let (sx, sy) = (((fx * w as f32) as u32).min(w - 1), ((fy * h as f32) as u32).min(h - 1));
            px[(sy * w + sx) as usize]
        })
        .collect();
    (tw, th, out)
}

impl Scaling {
    pub fn parse(s: &str) -> Self {
        match s {
            "integer-smaller" => Self::IntegerSmaller,
            "aspect" => Self::Aspect,
            // "Aspect screen" (the screen's own shape) always filled the screen exactly like Fullscreen and was
            // dropped on 2026-10-24; a saved choice of it reads as Fullscreen.
            "aspect-screen" | "fullscreen" => Self::Fullscreen,
            _ => Self::Native,
        }
    }
}

impl Sharpness {
    pub fn parse(s: &str) -> Self {
        match s {
            "crisp" => Self::Crisp,
            "soft" => Self::Soft,
            _ => Self::Sharp,
        }
    }
}

impl Effect {
    pub fn parse(s: &str) -> Self {
        match s {
            "grid" => Self::Grid,
            "scanlines" => Self::Scanlines,
            "crt" => Self::Crt,
            _ => Self::None,
        }
    }
}

/// Frame around a picture that doesn't fill the screen (see `bezel.rs` and `ambient.rs`).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum FrameMode {
    #[default]
    None,
    /// The installed pack's frame for the console, else the built-in one.
    Auto,
    /// The game's own colours, blurred and dimmed, like a TV's ambient light.
    Ambient,
    /// A handheld body around the screen lens, in the colourway of `frame_colour`.
    Shell,
    /// A curved-screen TV, in the cabinet of `frame_tv`.
    Crt,
    /// An arcade cabinet's glossy bezel with light strips.
    Arcade,
    /// A thin outline glowing in the console's colour.
    Glow,
    /// The handheld and TV frames of `frames_more.rs` and `frames_tv.rs`; those with finishes take
    /// them from their own setting (`frame_brick`, `frame_pocket`, `frame_clamshell`, `frame_swan`,
    /// `frame_watch`).
    Brick,
    PocketSlim,
    Clamshell,
    Wide,
    Twin,
    Gloss,
    Swan,
    Watch,
    Goggles,
    Studio,
    PortableTv,
    Woodgrain,
}

impl FrameMode {
    pub fn parse(s: &str) -> Self {
        match s {
            "auto" => Self::Auto,
            "ambient" => Self::Ambient,
            "shell" => Self::Shell,
            "crt" => Self::Crt,
            "arcade" => Self::Arcade,
            "glow" => Self::Glow,
            "brick" => Self::Brick,
            "pocket" => Self::PocketSlim,
            "clamshell" => Self::Clamshell,
            "wide" => Self::Wide,
            "twin" => Self::Twin,
            "gloss" => Self::Gloss,
            "swan" => Self::Swan,
            "watch" => Self::Watch,
            "goggles" => Self::Goggles,
            "studio" => Self::Studio,
            "portable-tv" => Self::PortableTv,
            "woodgrain" => Self::Woodgrain,
            _ => Self::None,
        }
    }
}

/// The Picture settings of `options.rs`. Unknown or missing values mean the defaults.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct VideoSettings {
    pub scaling: Scaling,
    pub sharpness: Sharpness,
    pub effect: Effect,
    pub frame: FrameMode,
    pub frame_colour: crate::bezel::Colourway,
    pub frame_tv: crate::bezel::Cabinet,
    pub frame_brick: crate::bezel::BrickColour,
    pub frame_pocket: crate::bezel::SlimFinish,
    pub frame_clamshell: crate::bezel::ClamFinish,
    pub frame_swan: crate::bezel::SwanFinish,
    pub frame_watch: crate::bezel::WatchFinish,
    /// LCD grid with the Game Boy Advance screen's colours (`lcd_colour`; offered for the GBA only, see
    /// `options::applies`).
    pub lcd_colour: bool,
    /// Not a setting: the emulator corrects the colours itself (its colour correction option is on), so the LCD
    /// grid leaves its own out rather than doing it twice. pocket-core sets it from the core's options.
    pub core_colour: bool,
    /// Screen orientation (`orientation`).
    pub orientation: Orientation,
}

impl VideoSettings {
    pub fn from_values(values: &BTreeMap<String, String>) -> Self {
        let get = |k: &str| values.get(k).map(String::as_str).unwrap_or("");
        VideoSettings {
            scaling: Scaling::parse(get("scaling")),
            sharpness: Sharpness::parse(get("sharpness")),
            effect: Effect::parse(get("effect")),
            frame: FrameMode::parse(get("frame")),
            frame_colour: crate::bezel::Colourway::parse(get("frame_colour")),
            frame_tv: crate::bezel::Cabinet::parse(get("frame_tv")),
            frame_brick: crate::bezel::BrickColour::parse(get("frame_brick")),
            frame_pocket: crate::bezel::SlimFinish::parse(get("frame_pocket")),
            frame_clamshell: crate::bezel::ClamFinish::parse(get("frame_clamshell")),
            frame_swan: crate::bezel::SwanFinish::parse(get("frame_swan")),
            frame_watch: crate::bezel::WatchFinish::parse(get("frame_watch")),
            lcd_colour: get("lcd_colour") == "on",
            core_colour: false,
            orientation: Orientation::parse(get("orientation")),
        }
    }

    /// Whether the picture gets the GBA screen's colours: LCD grid with LCD colours on, on the GBA, and the
    /// emulator not correcting them already.
    pub fn lcd_colours(&self, system: &str) -> bool {
        self.effect == Effect::Grid && self.lcd_colour && !self.core_colour && crate::options::applies("lcd_colour", system)
    }

    /// Applies one `set <key> <value>` of the pause menu; false for keys that are not Picture's.
    pub fn set(&mut self, key: &str, value: &str) -> bool {
        match key {
            "scaling" => self.scaling = Scaling::parse(value),
            "sharpness" => self.sharpness = Sharpness::parse(value),
            "effect" => self.effect = Effect::parse(value),
            "frame" => self.frame = FrameMode::parse(value),
            "frame_colour" => self.frame_colour = crate::bezel::Colourway::parse(value),
            "frame_tv" => self.frame_tv = crate::bezel::Cabinet::parse(value),
            "frame_brick" => self.frame_brick = crate::bezel::BrickColour::parse(value),
            "frame_pocket" => self.frame_pocket = crate::bezel::SlimFinish::parse(value),
            "frame_clamshell" => self.frame_clamshell = crate::bezel::ClamFinish::parse(value),
            "frame_swan" => self.frame_swan = crate::bezel::SwanFinish::parse(value),
            "frame_watch" => self.frame_watch = crate::bezel::WatchFinish::parse(value),
            "lcd_colour" => self.lcd_colour = value == "on",
            "orientation" => self.orientation = Orientation::parse(value),
            _ => return false,
        }
        true
    }

    /// The frame shows only when the picture leaves room around it: Fullscreen fills the screen, so it never
    /// gets one. A picture turned by Screen orientation keeps only the frames that have no up and down (Ambient,
    /// Glow): the others are drawn for a Pocket held as usual and would stand on their side.
    pub fn shows_frame(&self) -> bool {
        let upright = self.orientation == Orientation::Normal || matches!(self.frame, FrameMode::Ambient | FrameMode::Glow);
        self.frame != FrameMode::None && upright && matches!(self.scaling, Scaling::Native | Scaling::IntegerSmaller | Scaling::Aspect)
    }

    /// The built-in frame these settings ask for; None for Auto (a pack or the console's own
    /// style), Ambient (drawn live, see `ambient.rs`) and no frame.
    pub fn fixed_style(&self, system: &str) -> Option<crate::bezel::Style> {
        use crate::bezel::Style;
        match self.frame {
            FrameMode::Shell => Some(Style::Shell(self.frame_colour)),
            FrameMode::Crt => Some(Style::Crt(self.frame_tv)),
            FrameMode::Arcade => Some(Style::Arcade),
            FrameMode::Glow => Some(Style::Glow(crate::bezel::system_colour(system))),
            FrameMode::Brick => Some(Style::Brick(self.frame_brick)),
            FrameMode::PocketSlim => Some(Style::PocketSlim(self.frame_pocket)),
            FrameMode::Clamshell => Some(Style::Clamshell(self.frame_clamshell)),
            FrameMode::Wide => Some(Style::Wide),
            FrameMode::Twin => Some(Style::Twin),
            FrameMode::Gloss => Some(Style::Gloss),
            FrameMode::Swan => Some(Style::Swan(self.frame_swan)),
            FrameMode::Watch => Some(Style::Watch(self.frame_watch)),
            FrameMode::Goggles => Some(Style::Goggles),
            FrameMode::Studio => Some(Style::Studio),
            FrameMode::PortableTv => Some(Style::PortableTv),
            FrameMode::Woodgrain => Some(Style::Woodgrain),
            FrameMode::None | FrameMode::Auto | FrameMode::Ambient => None,
        }
    }

    /// Reads the console's settings, or the game's own file when it has one (see `options.rs`).
    pub fn load(data_dir: &Path, system: &str, rom: &Path) -> Self {
        Self::from_values(&GameSettings::load(data_dir, system, rom).effective())
    }
}

/// A destination rectangle in viewport pixels, origin top-left.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

impl Rect {
    fn centered(view_w: u32, view_h: u32, w: f32, h: f32) -> Rect {
        let w = (w.round() as u32).clamp(1, view_w.max(1));
        let h = (h.round() as u32).clamp(1, view_h.max(1));
        Rect { x: (view_w - w) / 2, y: (view_h - h) / 2, w, h }
    }
}

/// Where the picture goes. `aspect` is the core's display aspect ratio (width / height); a value
/// of 0 or less means the frame's own shape.
pub fn dest_rect(scaling: Scaling, view_w: u32, view_h: u32, src_w: u32, src_h: u32, aspect: f32) -> Rect {
    let (vw, vh) = (view_w.max(1), view_h.max(1));
    let (sw, sh) = (src_w.max(1), src_h.max(1));
    let aspect = if aspect > 0.0 { aspect } else { sw as f32 / sh as f32 };
    let fit = |aspect: f32| {
        let (vwf, vhf) = (vw as f32, vh as f32);
        if vwf / vhf > aspect {
            Rect::centered(vw, vh, vhf * aspect, vhf)
        } else {
            Rect::centered(vw, vh, vwf, vwf / aspect)
        }
    };
    match scaling {
        Scaling::Fullscreen => Rect { x: 0, y: 0, w: vw, h: vh },
        Scaling::Aspect => fit(aspect),
        Scaling::Native | Scaling::IntegerSmaller => {
            // Largest whole multiple of the picture height whose width, in the core's aspect
            // ratio, still fits. Most consoles' pixels are not square, so the width is rarely a
            // whole multiple too; Crisp sharpness hides that unevenness.
            let width_at = |n: u32| (n * sh) as f32 * aspect;
            let mut n = vh / sh;
            while n > 1 && width_at(n).round() > vw as f32 {
                n -= 1;
            }
            if n == 0 || width_at(n).round() > vw as f32 {
                fit(aspect) // not even 1x fits: scale down instead
            } else {
                if scaling == Scaling::IntegerSmaller {
                    n = (n - 1).max(1);
                }
                Rect::centered(vw, vh, width_at(n), (n * sh) as f32)
            }
        }
    }
}

const VS: &str = include_str!("shaders/video.vert");
const FS: &str = include_str!("shaders/video.frag");

/// Our shaders are GLSL ES 3.00. On a desktop OpenGL context (a core that renders with it, see
/// `hw_render.rs`) the same source runs as GLSL 3.30 core, which accepts the precision qualifiers.
/// Every shader pocket-core compiles (picture, frame, overlay) goes through this.
pub fn glsl_for(gl: &glow::Context, src: &str) -> String {
    if gl.version().is_embedded {
        src.to_string()
    } else {
        desktop_glsl(src)
    }
}

fn desktop_glsl(src: &str) -> String {
    src.replacen("#version 300 es", "#version 330 core", 1)
}

/// Inserts `defines` after the `#version` line, which must stay first.
fn with_defines(src: &str, defines: &str) -> String {
    let (version, rest) = src.split_once('\n').unwrap_or((src, ""));
    format!("{version}\n{defines}{rest}")
}

/// A compiled variant and its uniform locations.
struct Program {
    program: glow::Program,
    rect: Option<glow::UniformLocation>,
    src: Option<glow::UniformLocation>,
    scale: Option<glow::UniformLocation>,
    swizzle: Option<glow::UniformLocation>,
    tex: Option<glow::UniformLocation>,
    rot: Option<glow::UniformLocation>,
    lines: Option<glow::UniformLocation>,
}

/// Variant key: (sharpness, effect, the GBA screen's colours).
type Variant = (Sharpness, Effect, bool);
const PLAIN: Variant = (Sharpness::Sharp, Effect::None, false);

/// Texture filters a variant needs: (magnification, minification).
fn filters((sharpness, effect, _): Variant) -> (u32, u32) {
    // Scanlines and CRT steer the texture unit across a line (crisp) and sample between rows for a 3D core's
    // groups of rows; the CRT's glow reads the mipmaps.
    let mag = if sharpness != Sharpness::Sharp || matches!(effect, Effect::Scanlines | Effect::Crt) { glow::LINEAR } else { glow::NEAREST };
    let min = if effect == Effect::Crt { glow::LINEAR_MIPMAP_NEAREST } else { mag };
    (mag, min)
}

pub struct Renderer {
    gl: glow::Context,
    texture: glow::Texture,
    programs: HashMap<Variant, Option<Program>>,
    /// (width, height, pixel format) of the texture's current storage; width 0 until a frame came.
    tex_size: (u32, u32, u32),
    swizzle: i32,
    /// The texture's (magnification, minification) filters as last set.
    filters: Option<(u32, u32)>,
    /// The texture changed since its mipmaps were made (the CRT's glow needs them).
    mips_dirty: bool,
    /// Quarter turns counter-clockwise the core asked for (SET_ROTATION), applied when drawing with Screen
    /// orientation's (`turns`).
    rotation: u32,
    /// Screen frame behind the picture.
    bezel: crate::bezel::Layer,
    /// The last `draw` to the screen drew no frame because it is still being prepared (`set_frame_async`).
    frame_pending: bool,
    /// The Ambient frame, drawn live from the picture.
    ambient: crate::ambient::Layer,
    /// The framebuffer `draw` draws into: the screen, or the preview's offscreen one.
    target: Option<glow::Framebuffer>,
    /// Offscreen target for previews: (framebuffer, its texture, size).
    offscreen: Option<(glow::Framebuffer, glow::Texture, (u32, u32))>,
    /// Read and draw framebuffers for `copy_texture` (hardware-rendered cores).
    copy_fbos: Option<(glow::Framebuffer, glow::Framebuffer)>,
    /// GPU time of the picture pass, when asked for (`POCKET_CORE_GPU_TIMER=1`, `set_gpu_timer`).
    timer: Option<GpuTimer>,
}

/// GL timer queries around the picture pass (`GL_EXT_disjoint_timer_query` on GLES, `GL_ARB_timer_query` on desktop
/// GL), read back a few frames later so nothing waits for the GPU. Debug only: `POCKET_CORE_GPU_TIMER=1` makes
/// pocket-core log "gpu picture: …" every 600 pictures.
struct GpuTimer {
    free: Vec<glow::Query>,
    pending: std::collections::VecDeque<glow::Query>,
    /// Milliseconds per picture pass since the last report.
    samples: Vec<f32>,
    /// Log a report every this many samples (0: never; the caller asks with `gpu_timer_report`).
    log_every: usize,
}

/// `GL_TIME_ELAPSED(_EXT)` and `GL_GPU_DISJOINT_EXT`.
const TIME_ELAPSED: u32 = 0x88BF;
const GPU_DISJOINT: u32 = 0x8FBB;

impl GpuTimer {
    fn new(gl: &glow::Context, log_every: usize) -> Option<Self> {
        let ext = gl.supported_extensions();
        if !(ext.contains("GL_EXT_disjoint_timer_query") || ext.contains("GL_ARB_timer_query") || (!gl.version().is_embedded && gl.version().major >= 4)) {
            eprintln!("video: no timer queries on this GL ({}), GPU timing is off", unsafe { gl.get_parameter_string(glow::RENDERER) });
            return None;
        }
        let free = (0..6).filter_map(|_| unsafe { gl.create_query().ok() }).collect();
        Some(GpuTimer { free, pending: Default::default(), samples: Vec::new(), log_every })
    }

    /// Reads the results that are in, oldest first, without waiting.
    fn collect(&mut self, gl: &glow::Context, wait: bool) {
        unsafe {
            // A disjoint event (the GPU's clock changed) spoils the results in flight.
            let disjoint = gl.version().is_embedded && gl.get_parameter_i32(GPU_DISJOINT) != 0;
            while let Some(&q) = self.pending.front() {
                if !wait && gl.get_query_parameter_u32(q, glow::QUERY_RESULT_AVAILABLE) == 0 {
                    break;
                }
                let ns = gl.get_query_parameter_u32(q, glow::QUERY_RESULT);
                if !disjoint {
                    self.samples.push(ns as f32 / 1e6);
                }
                self.free.push(self.pending.pop_front().unwrap());
            }
        }
    }

    fn report(&self) -> Option<String> {
        if self.samples.is_empty() {
            return None;
        }
        let mut v = self.samples.clone();
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let mean = v.iter().sum::<f32>() / v.len() as f32;
        Some(format!("gpu picture: {:.3} ms mean, median {:.3}, p99 {:.3} ({} passes)", mean, v[v.len() / 2], v[(v.len() * 99 / 100).min(v.len() - 1)], v.len()))
    }
}

/// `tex_size` format of a frame copied from a hardware-rendered core (RGBA8).
const FORMAT_HW: u32 = 0x1000;

impl Renderer {
    /// Compiles the plain variant (sharp, no effect) and creates the texture. The context must be
    /// current. An error carries the shader log.
    pub fn new(gl: glow::Context) -> Result<Self, String> {
        let texture = unsafe {
            let texture = gl.create_texture()?;
            gl.bind_texture(glow::TEXTURE_2D, Some(texture));
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
            texture
        };
        // Desktop OpenGL (for a core that renders with it, see `hw_render.rs`): the core profile
        // draws nothing without a vertex array object bound, even when no attribute is used.
        if !gl.version().is_embedded {
            unsafe {
                let vao = gl.create_vertex_array()?;
                gl.bind_vertex_array(Some(vao));
            }
        }
        let base = Self::compile(&gl, PLAIN)?;
        let mut programs = HashMap::new();
        programs.insert(PLAIN, Some(base));
        Ok(Renderer {
            gl,
            texture,
            programs,
            tex_size: (0, 0, u32::MAX),
            swizzle: 0,
            filters: None,
            mips_dirty: true,
            rotation: 0,
            bezel: Default::default(),
            frame_pending: false,
            ambient: Default::default(),
            target: None,
            offscreen: None,
            copy_fbos: None,
            timer: None,
        })
        .map(|mut r: Renderer| {
            if std::env::var_os("POCKET_CORE_GPU_TIMER").is_some() {
                r.timer = GpuTimer::new(&r.gl, 600);
            }
            r
        })
    }

    /// Measures the GPU time of every picture pass from now on (or stops). For tools; pocket-core uses
    /// `POCKET_CORE_GPU_TIMER=1`.
    pub fn set_gpu_timer(&mut self, on: bool) {
        self.timer = if on { GpuTimer::new(&self.gl, 0) } else { None };
    }

    /// Forgets the timings so far.
    pub fn gpu_timer_reset(&mut self) {
        if let Some(t) = self.timer.as_mut() {
            t.collect(&self.gl, true);
            t.samples.clear();
        }
    }

    /// The timings since the last reset, waiting for those still in flight; None without timer queries.
    pub fn gpu_timer_report(&mut self) -> Option<String> {
        let t = self.timer.as_mut()?;
        t.collect(&self.gl, true);
        t.report()
    }

    fn compile(gl: &glow::Context, variant: Variant) -> Result<Program, String> {
        let defines = variant_defines(variant);
        unsafe {
            let program = gl.create_program()?;
            let mut shaders = Vec::new();
            for (kind, src) in [(glow::VERTEX_SHADER, VS.to_string()), (glow::FRAGMENT_SHADER, with_defines(FS, &defines))] {
                let src = glsl_for(gl, &src);
                let sh = gl.create_shader(kind)?;
                gl.shader_source(sh, &src);
                gl.compile_shader(sh);
                if !gl.get_shader_compile_status(sh) {
                    let log = gl.get_shader_info_log(sh);
                    gl.delete_shader(sh);
                    gl.delete_program(program);
                    return Err(log);
                }
                gl.attach_shader(program, sh);
                shaders.push(sh);
            }
            gl.link_program(program);
            let linked = gl.get_program_link_status(program);
            for sh in shaders {
                gl.detach_shader(program, sh);
                gl.delete_shader(sh);
            }
            if !linked {
                let log = gl.get_program_info_log(program);
                gl.delete_program(program);
                return Err(log);
            }
            let loc = |name: &str| gl.get_uniform_location(program, name);
            Ok(Program { program, rect: loc("rect"), src: loc("src"), scale: loc("scale"), swizzle: loc("swizzle"), tex: loc("tex"), rot: loc("rot"), lines: loc("lines") })
        }
    }

    /// Compiles every variant (sharpness × effect, and the LCD grid with the GBA's colours) that isn't yet, for
    /// tests: the number compiled, or the first one the driver rejects with its log.
    pub fn check_variants(&mut self) -> Result<usize, String> {
        let mut n = 0;
        for sharpness in [Sharpness::Sharp, Sharpness::Crisp, Sharpness::Soft] {
            for effect in [Effect::None, Effect::Grid, Effect::Scanlines, Effect::Crt] {
                for lcd in [false, true] {
                    if lcd && effect != Effect::Grid {
                        continue;
                    }
                    let v = (sharpness, effect, lcd);
                    match self.programs.get(&v) {
                        Some(Some(_)) => {}
                        _ => {
                            let p = Self::compile(&self.gl, v).map_err(|log| format!("{v:?}: {log}"))?;
                            self.programs.insert(v, Some(p));
                        }
                    }
                    n += 1;
                }
            }
        }
        Ok(n)
    }

    /// Turns the picture by `quarter_turns` x 90 degrees counter-clockwise, as the core asked (SET_ROTATION: vertical
    /// arcade games, WonderSwan's portrait mode).
    pub fn set_rotation(&mut self, quarter_turns: u32) {
        self.rotation = quarter_turns & 3;
    }

    /// The console being played (picks the frame style) and where frame packs are installed.
    pub fn set_frame_source(&mut self, system: &str, frames_dir: &Path) {
        self.bezel.set_source(system, frames_dir);
    }

    /// Prepare a frame that isn't ready on a thread and draw the picture without it meanwhile (`bezel::Layer::set_async`):
    /// pocket-core's first moments. Previews (`render_offscreen`) always wait for theirs.
    pub fn set_frame_async(&mut self, on: bool) {
        self.bezel.set_async(on);
    }

    /// The last frame drawn to the screen lacked its screen frame, which is still being prepared on a thread.
    pub fn frame_pending(&self) -> bool {
        self.frame_pending
    }

    /// Draws exactly what `draw` would put on a `view_w`×`view_h` screen into an offscreen buffer
    /// and reads it back: RGBA, rows top to bottom. For the pause menu's preview.
    pub fn render_offscreen(&mut self, view_w: u32, view_h: u32, aspect: f32, settings: &VideoSettings) -> Vec<u8> {
        let (w, h) = (view_w.max(1), view_h.max(1));
        let mut out = vec![0u8; (w * h * 4) as usize];
        if !self.draw_offscreen(w, h, aspect, settings) {
            return out;
        }
        unsafe {
            let (fb, _, _) = self.offscreen.unwrap();
            self.gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fb));
            self.gl.pixel_store_i32(glow::PACK_ALIGNMENT, 4);
            self.gl.read_pixels(0, 0, w as i32, h as i32, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelPackData::Slice(&mut out));
            self.gl.bind_framebuffer(glow::FRAMEBUFFER, None);
        }
        // GL rows run bottom to top.
        let row = (w * 4) as usize;
        for y in 0..(h as usize / 2) {
            let (a, b) = out.split_at_mut((h as usize - 1 - y) * row);
            a[y * row..(y + 1) * row].swap_with_slice(&mut b[..row]);
        }
        out
    }

    /// Draws what `draw` would put on a `view_w`×`view_h` screen into the offscreen buffer, without reading it back
    /// (`render_offscreen`, and timing the shaders in tools). False when the buffer can't be made.
    pub fn draw_offscreen(&mut self, view_w: u32, view_h: u32, aspect: f32, settings: &VideoSettings) -> bool {
        let (w, h) = (view_w.max(1), view_h.max(1));
        unsafe {
            if let Some((fb, tex, size)) = self.offscreen {
                if size != (w, h) {
                    self.gl.delete_framebuffer(fb);
                    self.gl.delete_texture(tex);
                    self.offscreen = None;
                }
            }
            if self.offscreen.is_none() {
                let (Ok(fb), Ok(tex)) = (self.gl.create_framebuffer(), self.gl.create_texture()) else { return false };
                self.gl.bind_texture(glow::TEXTURE_2D, Some(tex));
                self.gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::RGBA8 as i32, w as i32, h as i32, 0, glow::RGBA, glow::UNSIGNED_BYTE, None);
                self.gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fb));
                self.gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(tex), 0);
                self.offscreen = Some((fb, tex, (w, h)));
            }
            let (fb, _, _) = self.offscreen.unwrap();
            self.gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fb));
            // A preview shows the Ambient frame of this very picture, not an average with older ones.
            self.ambient.reset();
            self.target = Some(fb);
            // A preview waits for its frame; what the screen showed last stays what `frame_pending` says.
            let (was_async, was_pending) = (self.bezel.set_async(false), self.frame_pending);
            self.draw(w, h, aspect, settings);
            self.bezel.set_async(was_async);
            self.frame_pending = was_pending;
            self.target = None;
            self.gl.bind_framebuffer(glow::FRAMEBUFFER, None);
        }
        true
    }

    /// The GL context, for things drawn on top of the picture (see `overlay`).
    pub fn gl(&self) -> &glow::Context {
        &self.gl
    }

    /// Width and height of the last frame (a 3D core's internal resolution); (0, 0) before the first.
    pub fn frame_size(&self) -> (u32, u32) {
        (self.tex_size.0, self.tex_size.1)
    }

    /// True once a frame has been uploaded.
    pub fn has_frame(&self) -> bool {
        self.tex_size.0 > 0
    }

    /// Copies a core frame into the texture (RGB565, XRGB8888 or 0RGB1555).
    pub fn upload(&mut self, frame: &Frame) {
        let gl = &self.gl;
        unsafe {
            gl.bind_texture(glow::TEXTURE_2D, Some(self.texture));
            let (bpp, internal, format, kind, swizzle) = match frame.format {
                PIXEL_XRGB8888 => (4, glow::RGBA8, glow::RGBA, glow::UNSIGNED_BYTE, 1),
                _ => (2, glow::RGB565, glow::RGB, glow::UNSIGNED_SHORT_5_6_5, 0),
            };
            let converted;
            let data: &[u8] = if frame.format == PIXEL_0RGB1555 {
                converted = rgb1555_to_565(frame);
                &converted
            } else {
                &frame.data
            };
            let row = if frame.format == PIXEL_0RGB1555 { frame.width as i32 } else { (frame.pitch / bpp) as i32 };
            gl.pixel_store_i32(glow::UNPACK_ROW_LENGTH, row);
            gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, if bpp == 4 { 4 } else { 2 });
            let size = (frame.width, frame.height, frame.format);
            if self.tex_size != size {
                gl.tex_image_2d(glow::TEXTURE_2D, 0, internal as i32, frame.width as i32, frame.height as i32, 0, format, kind, Some(data));
                self.tex_size = size;
            } else {
                gl.tex_sub_image_2d(glow::TEXTURE_2D, 0, 0, 0, frame.width as i32, frame.height as i32, format, kind, glow::PixelUnpackData::Slice(data));
            }
            self.swizzle = swizzle;
        }
        self.mips_dirty = true;
    }

    /// Takes a frame from a hardware-rendered core: copies the `w`×`h` corner of its framebuffer
    /// texture `src` into the frame texture, turned upright when the core drew it bottom-up
    /// (OpenGL's own origin; libretro cores usually render top-down).
    pub fn copy_texture(&mut self, src: glow::Texture, w: u32, h: u32, bottom_left_origin: bool) {
        let gl = &self.gl;
        let (w, h) = (w.max(1), h.max(1));
        unsafe {
            if self.tex_size != (w, h, FORMAT_HW) {
                gl.bind_texture(glow::TEXTURE_2D, Some(self.texture));
                gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::RGBA8 as i32, w as i32, h as i32, 0, glow::RGBA, glow::UNSIGNED_BYTE, None);
                self.tex_size = (w, h, FORMAT_HW);
            }
            let (read, draw) = match self.copy_fbos {
                Some(f) => f,
                None => match (gl.create_framebuffer(), gl.create_framebuffer()) {
                    (Ok(r), Ok(d)) => *self.copy_fbos.insert((r, d)),
                    _ => return,
                },
            };
            gl.bind_framebuffer(glow::READ_FRAMEBUFFER, Some(read));
            gl.framebuffer_texture_2d(glow::READ_FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(src), 0);
            gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, Some(draw));
            gl.framebuffer_texture_2d(glow::DRAW_FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(self.texture), 0);
            let (y0, y1) = if bottom_left_origin { (h as i32, 0) } else { (0, h as i32) };
            gl.blit_framebuffer(0, 0, w as i32, h as i32, 0, y0, w as i32, y1, glow::COLOR_BUFFER_BIT, glow::NEAREST);
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
        }
        self.swizzle = 0;
        self.mips_dirty = true;
    }

    /// The frame texture read back as an XRGB8888 `Frame` (save-state previews of hardware-rendered
    /// cores, which deliver no frame in memory).
    pub fn read_frame(&self) -> Option<Frame> {
        let (w, h, format) = self.tex_size;
        if w == 0 || format != FORMAT_HW {
            return None;
        }
        let (read, _) = self.copy_fbos?;
        let gl = &self.gl;
        let mut data = vec![0u8; (w * h * 4) as usize];
        unsafe {
            gl.bind_framebuffer(glow::READ_FRAMEBUFFER, Some(read));
            gl.framebuffer_texture_2d(glow::READ_FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(self.texture), 0);
            gl.pixel_store_i32(glow::PACK_ALIGNMENT, 4);
            gl.read_pixels(0, 0, w as i32, h as i32, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelPackData::Slice(&mut data));
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
        }
        // RGBA bytes to XRGB8888's B, G, R, X.
        for px in data.chunks_exact_mut(4) {
            px.swap(0, 2);
        }
        Some(Frame { width: w, height: h, pitch: w as usize * 4, format: PIXEL_XRGB8888, data })
    }

    /// Quarter turns counter-clockwise the picture is drawn with: the core's request (SET_ROTATION) and Screen
    /// orientation's.
    pub fn turns(&self, settings: &VideoSettings) -> u32 {
        (self.rotation + settings.orientation.turns()) & 3
    }

    /// Where `draw` puts the last frame, in view pixels, and whether it is turned by a quarter (SET_ROTATION, Screen
    /// orientation); `None` before the first frame.
    pub fn placement(&self, view_w: u32, view_h: u32, aspect: f32, settings: &VideoSettings) -> Option<(Rect, bool)> {
        let (fw, fh, _) = self.tex_size;
        if fw == 0 {
            return None;
        }
        let (dw, dh, aspect) = turned_shape(fw, fh, aspect, self.rotation, settings.orientation.turns());
        Some((dest_rect(settings.scaling, view_w.max(1), view_h.max(1), dw, dh, aspect), self.turns(settings) & 1 == 1))
    }

    /// The picture on screen for touch input: its rectangle, the frame's size and how it is turned. `None` before
    /// the first frame.
    pub fn picture(&self, view_w: u32, view_h: u32, aspect: f32, settings: &VideoSettings) -> Option<crate::touch::Picture> {
        let (r, _) = self.placement(view_w, view_h, aspect, settings)?;
        let (fw, fh, _) = self.tex_size;
        Some(crate::touch::Picture { x: r.x as f32, y: r.y as f32, w: r.w as f32, h: r.h as f32, frame_w: fw, frame_h: fh, turns: self.turns(settings) })
    }

    /// Clears the viewport to black and draws the last uploaded frame. `aspect` is the core's
    /// display aspect ratio (0: the frame's own).
    pub fn draw(&mut self, view_w: u32, view_h: u32, aspect: f32, settings: &VideoSettings) {
        let (view_w, view_h) = (view_w.max(1), view_h.max(1));
        self.frame_pending = false;
        let placement = self.placement(view_w, view_h, aspect, settings);
        let variant = (settings.sharpness, settings.effect, settings.lcd_colours(self.bezel.system()));
        // Sharp samples texels as they are; Soft and the scanline effects need the hardware's bilinear (they
        // steer where they sample); Crisp fetches texels itself; the CRT's glow reads the mipmaps, made here
        // from each new picture (a few µs for a 2D console's picture).
        let filters = filters(variant);
        if placement.is_some() {
            unsafe {
                self.gl.bind_texture(glow::TEXTURE_2D, Some(self.texture));
                if filters.1 == glow::LINEAR_MIPMAP_NEAREST && self.mips_dirty {
                    self.gl.generate_mipmap(glow::TEXTURE_2D);
                    self.mips_dirty = false;
                }
                if self.filters != Some(filters) {
                    self.gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, filters.0 as i32);
                    self.gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, filters.1 as i32);
                    self.filters = Some(filters);
                }
            }
        }
        // Ambient samples the picture into its own small images first, then comes back here.
        let ambient = settings.shows_frame() && settings.frame == FrameMode::Ambient;
        if let (true, Some((r, _))) = (ambient, placement) {
            self.ambient.update(&self.gl, self.target, view_w, view_h, r, self.texture, self.swizzle, self.turns(settings));
        }
        unsafe {
            self.gl.viewport(0, 0, view_w as i32, view_h as i32);
            self.gl.clear_color(0.0, 0.0, 0.0, 1.0);
            self.gl.clear(glow::COLOR_BUFFER_BIT);
        }
        let (fw, fh, _) = self.tex_size;
        let turns = self.turns(settings);
        let Some((r, turned)) = placement else { return };
        let dh = if turned { fw } else { fh };
        if ambient {
            self.ambient.show(&self.gl, view_w, view_h, r);
        } else if settings.shows_frame() {
            let fixed = settings.fixed_style(self.bezel.system());
            self.frame_pending = !self.bezel.draw(&self.gl, view_w, view_h, r, r.h as f32 / dh as f32, fixed);
        }

        if !self.programs.contains_key(&variant) {
            let compiled = Self::compile(&self.gl, variant);
            if let Err(log) = &compiled {
                eprintln!("video: shader variant {variant:?} failed, using the plain one: {log}");
            }
            self.programs.insert(variant, compiled.ok());
        }
        // A variant the driver rejects falls back to the plain one (Crisp then shows as Soft).
        let prog = match self.programs.get(&variant) {
            Some(Some(p)) => p,
            _ => self.programs[&PLAIN].as_ref().expect("the plain variant"),
        };

        let gl = &self.gl;
        // The GPU timer (debug) brackets the picture pass alone, not the frame around it or the HUD.
        let timed = self.timer.as_mut().and_then(|t| t.free.pop());
        if let Some(q) = timed {
            unsafe { gl.begin_query(TIME_ELAPSED, q) };
        }
        unsafe {
            gl.use_program(Some(prog.program));
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, Some(self.texture));
            // Pixel rectangle (top-left origin) to NDC (bottom-left, -1..1).
            let (vw, vh) = (view_w as f32, view_h as f32);
            let ndc = |px: u32, total: f32| px as f32 / total * 2.0 - 1.0;
            gl.uniform_4_f32(prog.rect.as_ref(), ndc(r.x, vw), -ndc(r.y + r.h, vh), ndc(r.x + r.w, vw), -ndc(r.y, vh));
            gl.uniform_1_i32(prog.tex.as_ref(), 0);
            gl.uniform_2_f32(prog.src.as_ref(), fw as f32, fh as f32);
            // Output pixels per source texel along the source's own x and y.
            let (sx, sy) = if turned { (r.h as f32 / fw as f32, r.w as f32 / fh as f32) } else { (r.w as f32 / fw as f32, r.h as f32 / fh as f32) };
            gl.uniform_2_f32(prog.scale.as_ref(), sx, sy);
            gl.uniform_1_i32(prog.rot.as_ref(), turns as i32);
            gl.uniform_1_i32(prog.swizzle.as_ref(), self.swizzle);
            let upscaled_3d = self.tex_size.2 == FORMAT_HW;
            let lines = crate::picture_fx::crt_lines(fh, upscaled_3d, crate::pixel_path::native_lines(self.bezel.system()));
            gl.uniform_1_f32(prog.lines.as_ref(), lines);
            gl.draw_arrays(glow::TRIANGLE_STRIP, 0, 4);
        }
        if let (Some(q), Some(t)) = (timed, self.timer.as_mut()) {
            unsafe { gl.end_query(TIME_ELAPSED) };
            t.pending.push_back(q);
            t.collect(gl, false);
            if t.log_every > 0 && t.samples.len() >= t.log_every {
                if let Some(r) = t.report() {
                    eprintln!("{r}");
                }
                t.samples.clear();
            }
        }
        // A frame may draw over a strip of the picture that holds no game (the Twin frame's hinge in
        // the gap between the DS screens).
        if settings.shows_frame() && !ambient {
            self.bezel.draw_inlay(&self.gl, view_w, view_h, r);
        }
    }

    /// A state was saved: the frame's power light flashes.
    pub fn flash_frame_light(&mut self) {
        self.bezel.flash_light();
    }
}

/// The `#define`s of a variant, then the effects' constants (`picture_fx::defines`).
fn variant_defines((sharpness, effect, lcd_colour): Variant) -> String {
    let sharpness = match sharpness {
        Sharpness::Sharp => 0,
        Sharpness::Crisp => 1,
        Sharpness::Soft => 2,
    };
    let mut d = format!("#define SHARPNESS {sharpness}\n#define EFFECT {}\n", effect as u8);
    if lcd_colour {
        d.push_str("#define LCD_COLOUR\n");
    }
    d + &crate::picture_fx::defines()
}

/// 0RGB1555 has no GL upload format; widen the 5-bit green to 6 bits and use RGB565.
pub fn rgb1555_to_565(frame: &Frame) -> Vec<u8> {
    let mut out = Vec::with_capacity((frame.width * frame.height * 2) as usize);
    for y in 0..frame.height as usize {
        let row = &frame.data[y * frame.pitch..y * frame.pitch + frame.width as usize * 2];
        for px in row.chunks_exact(2) {
            let v = u16::from_le_bytes([px[0], px[1]]);
            let (r, g, b) = ((v >> 10) & 0x1f, (v >> 5) & 0x1f, v & 0x1f);
            out.extend_from_slice(&(r << 11 | (g << 1 | g >> 4) << 5 | b).to_le_bytes());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;

    const R: fn(u32, u32, u32, u32) -> Rect = |x, y, w, h| Rect { x, y, w, h };

    #[test]
    fn native_game_boy_is_7x_centered() {
        let r = dest_rect(Scaling::Native, 1240, 1080, 160, 144, 160.0 / 144.0);
        assert_eq!(r, R(60, 36, 1120, 1008));
    }

    #[test]
    fn native_snes_has_whole_height_in_4_3() {
        let r = dest_rect(Scaling::Native, 1240, 1080, 256, 224, 4.0 / 3.0);
        assert_eq!((r.h, r.h % 224), (896, 0));
        assert_eq!(r.w, 1195);
        assert_eq!((r.x, r.y), (22, 92));
    }

    #[test]
    fn native_steps_down_until_the_width_fits() {
        // NES 256x240 in 4:3: 4x would be 960 tall but 1280 wide, wider than the screen.
        let r = dest_rect(Scaling::Native, 1240, 1080, 256, 240, 4.0 / 3.0);
        assert_eq!((r.w, r.h), (960, 720));
        // A very wide frame: 2x is the largest that fits.
        let r = dest_rect(Scaling::Native, 1240, 1080, 480, 144, 480.0 / 144.0);
        assert_eq!((r.w, r.h), (960, 288));
    }

    #[test]
    fn native_falls_back_to_aspect_when_1x_does_not_fit() {
        let r = dest_rect(Scaling::Native, 1240, 1080, 1920, 1080, 16.0 / 9.0);
        assert_eq!(r, dest_rect(Scaling::Aspect, 1240, 1080, 1920, 1080, 16.0 / 9.0));
        assert_eq!((r.w, r.h), (1240, 698));
    }

    #[test]
    fn on_a_tv_the_picture_scales_by_whole_numbers_to_1080_and_2160_lines() {
        // TV mode asks for 1920x1080 (`display_out::choose_mode`); pocket-core draws at its window's size each frame.
        // (system, frame, aspect, scale on the panel, on a 1080p TV, on a 4K output)
        let cases = [
            ("GB", (160, 144), 160.0 / 144.0, 7, 7, 15),
            ("GBA", (240, 160), 1.5, 5, 6, 13),
            ("NES", (256, 240), 4.0 / 3.0, 3, 4, 9),
            ("SNES", (256, 224), 4.0 / 3.0, 4, 4, 9),
            ("Mega Drive", (320, 224), 4.0 / 3.0, 4, 4, 9),
            ("PlayStation", (320, 240), 4.0 / 3.0, 3, 4, 9),
            ("PS1 480i, N64 hi-res", (640, 480), 4.0 / 3.0, 1, 2, 4),
        ];
        for (name, (w, h), aspect, panel, tv, uhd) in cases {
            for ((vw, vh), n) in [((1240, 1080), panel), ((1920, 1080), tv), ((3840, 2160), uhd)] {
                let r = dest_rect(Scaling::Native, vw, vh, w, h, aspect);
                assert_eq!((r.h, r.h / h), (h * n, n), "{name} on {vw}x{vh}");
                assert!(r.w <= vw && r.x + r.w <= vw && r.y + r.h <= vh, "{name} on {vw}x{vh}");
            }
        }
        // GBA on a 1080p TV: 1440x960 in the middle.
        assert_eq!(dest_rect(Scaling::Native, 1920, 1080, 240, 160, 1.5), R(240, 60, 1440, 960));
    }

    /// The experimental SM8250 siblings (docs/DEVICES.md): the Pocket 5 and Flip 2 at 1920x1080 scale as a 1080p TV
    /// does (320x240 at 4x, as 3x on the Mini V2); the Mini V1's 1280x960 fills the whole screen with 320x240 and
    /// 640x480 games and gives the Game Boy 6x.
    #[test]
    fn on_the_pocket_5_and_the_mini_v1_the_picture_scales_by_whole_numbers() {
        // (system, frame, aspect, scale at 1920x1080, at 1280x960)
        let cases = [
            ("GB", (160, 144), 160.0 / 144.0, 7, 6),
            ("GBA", (240, 160), 1.5, 6, 5),
            ("NES", (256, 240), 4.0 / 3.0, 4, 4),
            ("SNES", (256, 224), 4.0 / 3.0, 4, 4),
            ("PlayStation", (320, 240), 4.0 / 3.0, 4, 4),
            ("PS1 480i, N64 hi-res", (640, 480), 4.0 / 3.0, 2, 2),
            ("PSP", (480, 272), 480.0 / 272.0, 3, 2),
        ];
        for (name, (w, h), aspect, rp5, mini) in cases {
            for ((vw, vh), n) in [((1920, 1080), rp5), ((1280, 960), mini)] {
                let r = dest_rect(Scaling::Native, vw, vh, w, h, aspect);
                assert_eq!((r.h, r.h / h), (h * n, n), "{name} on {vw}x{vh}");
                assert!(r.w <= vw && r.x + r.w <= vw && r.y + r.h <= vh, "{name} on {vw}x{vh}");
            }
        }
        // 320x240 on the Mini V1: the whole screen, nothing left for a frame.
        assert_eq!(dest_rect(Scaling::Native, 1280, 960, 320, 240, 4.0 / 3.0), R(0, 0, 1280, 960));
        // GB on the Pocket 5: 7x in the middle, 400 px either side.
        assert_eq!(dest_rect(Scaling::Native, 1920, 1080, 160, 144, 160.0 / 144.0), R(400, 36, 1120, 1008));
    }

    #[test]
    fn integer_smaller_is_one_whole_step_below_integer() {
        // GBA 5x -> 4x: 960x640, room for a frame on every side.
        assert_eq!(dest_rect(Scaling::IntegerSmaller, 1240, 1080, 240, 160, 1.5), R(140, 220, 960, 640));
        assert_eq!(dest_rect(Scaling::IntegerSmaller, 1240, 1080, 160, 144, 160.0 / 144.0), R(140, 108, 960, 864));
        // SNES 4x -> 3x: the height stays a whole multiple.
        assert_eq!(dest_rect(Scaling::IntegerSmaller, 1240, 1080, 256, 224, 4.0 / 3.0), R(172, 204, 896, 672));
        // NES is 3x at Integer (4x is too wide), so 2x.
        assert_eq!(dest_rect(Scaling::IntegerSmaller, 1240, 1080, 256, 240, 4.0 / 3.0), R(300, 300, 640, 480));
        // Never below 1x, and a picture too big even for 1x is scaled to fit as with Integer.
        for (w, h, a) in [(640, 480, 4.0 / 3.0), (1920, 1080, 16.0 / 9.0)] {
            assert_eq!(dest_rect(Scaling::IntegerSmaller, 1240, 1080, w, h, a), dest_rect(Scaling::Native, 1240, 1080, w, h, a));
        }
        assert_eq!(Scaling::parse("integer-smaller"), Scaling::IntegerSmaller);
    }

    #[test]
    fn every_frame_shows_whenever_the_picture_leaves_room() {
        use FrameMode::*;
        for frame in [Auto, Ambient, Shell, Crt, Arcade, Glow] {
            for (scaling, shows) in [(Scaling::Native, true), (Scaling::IntegerSmaller, true), (Scaling::Aspect, true), (Scaling::Fullscreen, false)] {
                let s = VideoSettings { frame, scaling, ..Default::default() };
                assert_eq!(s.shows_frame(), shows, "{frame:?} {scaling:?}");
            }
        }
        assert!(!VideoSettings { frame: None, ..Default::default() }.shows_frame());
    }

    #[test]
    fn frame_settings_parse_and_pick_the_built_in_style() {
        use crate::bezel::{Cabinet, Colourway, Style};
        let mut m = BTreeMap::new();
        for (k, v) in [("frame", "shell"), ("frame_colour", "atomic-purple"), ("frame_tv", "wood"), ("scaling", "integer-smaller")] {
            m.insert(k.to_string(), v.to_string());
        }
        let mut s = VideoSettings::from_values(&m);
        assert_eq!((s.frame, s.frame_colour, s.frame_tv, s.scaling), (FrameMode::Shell, Colourway::AtomicPurple, Cabinet::Wood, Scaling::IntegerSmaller));
        assert_eq!(s.fixed_style("gba"), Some(Style::Shell(Colourway::AtomicPurple)));
        assert!(s.set("frame", "crt") && s.fixed_style("snes") == Some(Style::Crt(Cabinet::Wood)));
        assert!(s.set("frame", "glow") && s.fixed_style("gbc") == Some(Style::Glow(0x2e8c9a)), "the shelf's stripe colour");
        assert!(s.set("frame", "arcade") && s.fixed_style("nes") == Some(Style::Arcade));
        for (v, mode) in [("auto", FrameMode::Auto), ("ambient", FrameMode::Ambient), ("none", FrameMode::None), ("bogus", FrameMode::None)] {
            assert!(s.set("frame", v));
            assert_eq!((s.frame, s.fixed_style("gba")), (mode, None));
        }
        assert!(!s.set("hk_save", "on"), "not a Picture setting");
        use crate::bezel::{BrickColour, ClamFinish, SlimFinish, SwanFinish, WatchFinish};
        for (k, v) in [("frame_brick", "yellow"), ("frame_pocket", "clear"), ("frame_clamshell", "cobalt"), ("frame_swan", "skeleton"), ("frame_watch", "silver")] {
            assert!(s.set(k, v), "{k}");
        }
        for (v, style) in [
            ("brick", Style::Brick(BrickColour::Yellow)),
            ("pocket", Style::PocketSlim(SlimFinish::Clear)),
            ("clamshell", Style::Clamshell(ClamFinish::Cobalt)),
            ("wide", Style::Wide),
            ("twin", Style::Twin),
            ("gloss", Style::Gloss),
            ("swan", Style::Swan(SwanFinish::Skeleton)),
            ("watch", Style::Watch(WatchFinish::Silver)),
            ("goggles", Style::Goggles),
            ("studio", Style::Studio),
            ("portable-tv", Style::PortableTv),
            ("woodgrain", Style::Woodgrain),
        ] {
            assert!(s.set("frame", v));
            assert_eq!(s.fixed_style("gb"), Some(style), "{v}");
        }
        // A settings file from before these frames: the finishes take their first value.
        let old = VideoSettings::from_values(&[("frame".to_string(), "brick".to_string())].into_iter().collect());
        assert_eq!(old.fixed_style("gb"), Some(Style::Brick(BrickColour::Grey)));
        assert_eq!((Colourway::parse("x"), Cabinet::parse("x")), (Colourway::Grey, Cabinet::Plastic));
    }

    /// What the renderer does for a frame: its shape turned (`turned_shape`), then scaled.
    fn turned_rect(scaling: Scaling, fw: u32, fh: u32, aspect: f32, core: u32, user: Orientation) -> Rect {
        let (w, h, a) = turned_shape(fw, fh, aspect, core, user.turns());
        dest_rect(scaling, 1240, 1080, w, h, a)
    }

    #[test]
    fn orientation_parses_and_turns_the_picture_against_the_pocket() {
        let parsed = ["left", "right", "", "up"].map(Orientation::parse);
        assert_eq!(parsed, [Orientation::Left, Orientation::Right, Orientation::Normal, Orientation::Normal]);
        // Held turned right, the Pocket turns its picture clockwise for the player: drawn a quarter counter-clockwise.
        assert_eq!((Orientation::Normal.turns(), Orientation::Right.turns(), Orientation::Left.turns()), (0, 1, 3));
        let mut s = VideoSettings::default();
        assert!(s.set("orientation", "right") && s.orientation == Orientation::Right);
        let m: BTreeMap<String, String> = [("orientation".to_string(), "left".to_string())].into_iter().collect();
        assert_eq!(VideoSettings::from_values(&m).orientation, Orientation::Left);
        assert_eq!(VideoSettings::from_values(&BTreeMap::new()).orientation, Orientation::Normal, "a file from before: Normal");
    }

    #[test]
    fn frame_points_follow_the_vertex_shader() {
        // The shader's mapping for the four corners of the rectangle (0..1, y down) at each turn: the frame's top-left
        // texel lands top-left (0), bottom-left (1, a quarter counter-clockwise), bottom-right (2), top-right (3).
        let corner_of_top_left = |turns| [(0.0, 0.0), (0.0, 1.0), (1.0, 0.0), (1.0, 1.0)].into_iter().find(|&(x, y)| frame_point(turns, x, y) == (0.0, 0.0));
        assert_eq!([0, 1, 2, 3].map(corner_of_top_left), [Some((0.0, 0.0)), Some((0.0, 1.0)), Some((1.0, 1.0)), Some((1.0, 0.0))]);
        assert_eq!(frame_point(1, 0.25, 0.5), (0.5, 0.25));
        assert_eq!(frame_point(5, 0.25, 0.5), frame_point(1, 0.25, 0.5));
        // Each turn undone by its opposite.
        for t in 0..4 {
            let (x, y) = frame_point(t, 0.3, 0.8);
            let back = frame_point((4 - t) % 4, x, y);
            assert!((back.0 - 0.3).abs() < 1e-6 && (back.1 - 0.8).abs() < 1e-6, "{t}: {back:?}");
        }
        let vs = VS.replace(' ', "");
        assert!(vs.contains("if(rot==1)v=vec2(1.0-d.y,d.x);") && vs.contains("elseif(rot==3)v=vec2(d.y,1.0-d.x);"), "the shader still maps as frame_point");
    }

    #[test]
    fn turned_pixels_match_the_frame_points() {
        // 3x2: a b c / d e f. A quarter counter-clockwise: c f / b e / a d.
        let px = ['a', 'b', 'c', 'd', 'e', 'f'];
        assert_eq!(turn_pixels(3, 2, &px, 1), (2, 3, vec!['c', 'f', 'b', 'e', 'a', 'd']));
        assert_eq!(turn_pixels(3, 2, &px, 3), (2, 3, vec!['d', 'a', 'e', 'b', 'f', 'c']));
        assert_eq!(turn_pixels(3, 2, &px, 2), (3, 2, vec!['f', 'e', 'd', 'c', 'b', 'a']));
        assert_eq!(turn_pixels(3, 2, &px, 0).2, px);
    }

    #[test]
    fn a_turned_vertical_game_scales_by_whole_numbers_and_gets_much_larger() {
        // A vertical game delivered upright, 224x288 in 3:4: Normal is 3x (864 tall, 648 wide); turned, its 224
        // columns run down the panel at 4x: 896 tall and 1195 wide, nearly twice the area.
        let normal = turned_rect(Scaling::Native, 224, 288, 0.75, 0, Orientation::Normal);
        assert_eq!(normal, R(296, 108, 648, 864));
        for o in [Orientation::Left, Orientation::Right] {
            let r = turned_rect(Scaling::Native, 224, 288, 0.75, 0, o);
            assert_eq!(r, R(22, 92, 1195, 896), "{o:?}");
            assert!(r.w * r.h > normal.w * normal.h * 19 / 10, "nearly twice the area");
            assert_eq!(r.h % 224, 0, "whole multiples of the frame's columns");
            let smaller = turned_rect(Scaling::IntegerSmaller, 224, 288, 0.75, 0, o);
            assert_eq!((smaller.w, smaller.h), (896, 672), "one whole step down: 3x");
        }
        // 224x256 at square pixels: 4x either way, but turned both sides are whole multiples and it is wider.
        assert_eq!(turned_rect(Scaling::Native, 224, 256, 0.0, 0, Orientation::Normal), R(172, 28, 896, 1024));
        assert_eq!(turned_rect(Scaling::Native, 224, 256, 0.0, 0, Orientation::Right), R(108, 92, 1024, 896));
        // Aspect: as large as fits in the turned shape (the game's 3:4 lying down is 4:3).
        let r = turned_rect(Scaling::Aspect, 224, 288, 0.75, 0, Orientation::Left);
        assert_eq!((r.w, r.h), (1240, 930));
        // Fullscreen fills the panel whatever the turn.
        assert_eq!(turned_rect(Scaling::Fullscreen, 224, 288, 0.75, 0, Orientation::Right), R(0, 0, 1240, 1080));
    }

    #[test]
    fn a_turned_handheld_picture_is_its_shape_lying_down() {
        // Game Boy 160x144 turned: 144 wide, 160 tall, 6x (960 tall) in its own aspect turned (0.9).
        let r = turned_rect(Scaling::Native, 160, 144, 160.0 / 144.0, 0, Orientation::Right);
        assert_eq!((r.w, r.h), (864, 960));
        // A WonderSwan game (224x144) held upright: the frame stands 144 wide, 224 tall: 4x, 576x896.
        let r = turned_rect(Scaling::Native, 224, 144, 14.0 / 9.0, 0, Orientation::Left);
        assert_eq!((r.w, r.h), (576, 896));
    }

    #[test]
    fn a_cores_own_turn_keeps_the_aspect_it_gives_for_the_upright_picture() {
        // FBNeo's vertical games: the frame as the board draws it, 288x224 lying down, turned by SET_ROTATION, with
        // the aspect of the picture seen upright (3:4). Drawn as the same 224x288 picture as one delivered upright.
        let core_turned = turned_rect(Scaling::Native, 288, 224, 0.75, 1, Orientation::Normal);
        assert_eq!(core_turned, turned_rect(Scaling::Native, 224, 288, 0.75, 0, Orientation::Normal));
        assert_eq!(turned_shape(288, 224, 0.75, 1, 0), (224, 288, 0.75));
        assert_eq!(turned_shape(288, 224, 0.75, 3, 0), (224, 288, 0.75));
        // Beetle WonderSwan's portrait mode (SET_ROTATION 1, aspect 9:14): upright on the panel, 4x tall.
        assert_eq!(turned_rect(Scaling::Native, 224, 144, 9.0 / 14.0, 1, Orientation::Normal), R(332, 92, 576, 896));
        // Held turned on top of it (Turned left undoes the core's quarter turn): the frame lies as it is, its aspect
        // turned with the Pocket, so the picture is the large one, 5x: 1120x720 instead of 576x896.
        let r = turned_rect(Scaling::Native, 224, 144, 9.0 / 14.0, 1, Orientation::Left);
        let (w, h, a) = turned_shape(224, 144, 9.0 / 14.0, 1, 3);
        assert!((w, h) == (224, 144) && (a - 14.0 / 9.0).abs() < 1e-5, "{w}x{h} {a}");
        assert_eq!(r, R(60, 180, 1120, 720));
        // The frame's own shape (aspect 0) follows the turned sides.
        assert_eq!(turned_shape(320, 240, 0.0, 1, 0), (240, 320, 0.0));
        assert_eq!(turned_shape(320, 240, 0.0, 1, 1), (320, 240, 0.0));
        assert_eq!(turned_shape(320, 240, 4.0 / 3.0, 2, 2), (320, 240, 4.0 / 3.0), "upside down twice is upright");
    }

    #[test]
    fn turned_pictures_keep_only_frames_without_an_up_and_down() {
        for (frame, shows) in [(FrameMode::Ambient, true), (FrameMode::Glow, true), (FrameMode::Auto, false), (FrameMode::Shell, false), (FrameMode::Arcade, false)] {
            let s = VideoSettings { frame, orientation: Orientation::Left, ..Default::default() };
            assert_eq!(s.shows_frame(), shows, "{frame:?}");
            assert!(VideoSettings { orientation: Orientation::Normal, ..s }.shows_frame(), "{frame:?} upright");
        }
    }

    #[test]
    fn aspect_fills_the_limiting_side() {
        // Game Boy (1.111) is narrower than the screen (1.148), so the height limits.
        assert_eq!(dest_rect(Scaling::Aspect, 1240, 1080, 160, 144, 160.0 / 144.0), R(20, 0, 1200, 1080));
        // 16:9 is wider than the screen, so the width limits.
        assert_eq!(dest_rect(Scaling::Aspect, 1240, 1080, 320, 180, 16.0 / 9.0), R(0, 191, 1240, 698));
    }

    #[test]
    fn aspect_zero_uses_the_frame_shape() {
        assert_eq!(dest_rect(Scaling::Aspect, 1240, 1080, 160, 144, 0.0), dest_rect(Scaling::Aspect, 1240, 1080, 160, 144, 160.0 / 144.0));
        assert_eq!(dest_rect(Scaling::Native, 1240, 1080, 160, 144, -1.0), R(60, 36, 1120, 1008));
    }

    #[test]
    fn fullscreen_fills_the_viewport() {
        assert_eq!(dest_rect(Scaling::Fullscreen, 1240, 1080, 160, 144, 160.0 / 144.0), R(0, 0, 1240, 1080));
    }

    #[test]
    fn tiny_or_degenerate_viewports_do_not_panic() {
        for s in [Scaling::Native, Scaling::IntegerSmaller, Scaling::Aspect, Scaling::Fullscreen] {
            let r = dest_rect(s, 0, 0, 160, 144, 1.1);
            assert!(r.w >= 1 && r.h >= 1);
            let r = dest_rect(s, 100, 50, 160, 144, 1.1);
            assert!(r.x + r.w <= 100 && r.y + r.h <= 50, "{s:?} {r:?}");
        }
    }

    #[test]
    fn settings_parse_values_and_default_unknown() {
        let mut m = BTreeMap::new();
        assert_eq!(VideoSettings::from_values(&m), VideoSettings::default());
        for (k, v) in [("scaling", "aspect-screen"), ("sharpness", "crisp"), ("effect", "crt")] {
            m.insert(k.to_string(), v.to_string());
        }
        let s = VideoSettings::from_values(&m);
        assert_eq!((s.scaling, s.sharpness, s.effect), (Scaling::Fullscreen, Sharpness::Crisp, Effect::Crt), "aspect-screen reads as Fullscreen");
        for (k, v) in [("scaling", "bogus"), ("sharpness", ""), ("effect", "CRT")] {
            m.insert(k.to_string(), v.to_string());
        }
        assert_eq!(VideoSettings::from_values(&m), VideoSettings::default());
        assert_eq!((Scaling::parse("aspect"), Scaling::parse("fullscreen"), Sharpness::parse("soft"), Effect::parse("grid"), Effect::parse("scanlines")), (Scaling::Aspect, Scaling::Fullscreen, Sharpness::Soft, Effect::Grid, Effect::Scanlines));
    }

    #[test]
    fn settings_load_game_file_over_console_file() {
        let dir = TestDir::new("video");
        let rom = Path::new("/roms/gb/Tetris.gb");
        std::fs::create_dir_all(dir.join("game-settings/gb")).unwrap();
        std::fs::write(dir.join("game-settings/gb.json"), r#"{"scaling":"aspect","effect":"grid"}"#).unwrap();
        let s = VideoSettings::load(&dir, "gb", rom);
        assert_eq!((s.scaling, s.effect), (Scaling::Aspect, Effect::Grid));
        std::fs::write(dir.join("game-settings/gb/Tetris.json"), r#"{"sharpness":"soft"}"#).unwrap();
        // The game file replaces the console file; unset keys fall back to the defaults (no effect, no frame).
        assert_eq!(VideoSettings::load(&dir, "gb", rom), VideoSettings { sharpness: Sharpness::Soft, ..Default::default() });
        assert_eq!(VideoSettings::load(&dir, "snes", rom), VideoSettings::default(), "no files: a clean picture");
    }

    #[test]
    fn defines_go_after_the_version_line() {
        let s = with_defines("#version 300 es\nvoid main() {}", "#define A 1\n");
        assert_eq!(s, "#version 300 es\n#define A 1\nvoid main() {}");
        assert!(FS.starts_with("#version 300 es\n") && VS.starts_with("#version 300 es\n"));
    }

    #[test]
    fn desktop_gl_gets_glsl_330_with_defines_kept() {
        let s = desktop_glsl(&with_defines(FS, "#define EFFECT 1\n"));
        assert!(s.starts_with("#version 330 core\n#define EFFECT 1\n"), "{}", &s[..40]);
        assert!(!s.contains("300 es"));
        for src in [VS, crate::bezel::FS, crate::overlay::VS, crate::overlay::FS, crate::ambient::SAMPLE_FS, crate::ambient::BLUR_FS, crate::ambient::SHOW_FS, crate::ambient::BLIT_FS] {
            assert!(desktop_glsl(src).starts_with("#version 330 core\n"));
        }
    }

    #[test]
    fn rgb1555_widens_green_and_honors_the_pitch() {
        // Two pixels per row plus 2 bytes of padding: white, then pure green (5 bits all set).
        let px = |v: u16| v.to_le_bytes();
        let mut data = Vec::new();
        for _ in 0..2 {
            data.extend_from_slice(&px(0x7fff));
            data.extend_from_slice(&px(0x03e0));
            data.extend_from_slice(&[0xaa, 0xaa]);
        }
        let frame = Frame { width: 2, height: 2, pitch: 6, format: PIXEL_0RGB1555, data };
        let out = rgb1555_to_565(&frame);
        assert_eq!(out.len(), 8);
        assert_eq!(u16::from_le_bytes([out[0], out[1]]), 0xffff);
        assert_eq!(u16::from_le_bytes([out[2], out[3]]), 0x07e0);
    }
}
