//! pocket-core inside the APK: shared libretro host, save semantics and audio engine on the EGL thread.
use glow::HasContext;
use jni::{
    objects::{JObject, JString},
    sys::{jboolean, jfloat, jint, jstring},
    JNIEnv,
};
use pocket_ui::{
    core_options::FileOption,
    gfx::Canvas,
    image::Image,
    libretro::Core,
    palette::{self, pal},
    text::{Face, Text},
    type_scale,
    touch::Area,
};
use std::{
    cell::RefCell,
    collections::HashMap,
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
#[path = "picture.rs"]
mod picture;
thread_local! {static PLAYER:RefCell<Option<Player>>=const{RefCell::new(None)};}
#[link(name = "EGL")]
extern "C" {
    fn eglGetProcAddress(name: *const std::ffi::c_char) -> *const std::ffi::c_void;
}
struct Graphics {
    gl: glow::Context,
    program: glow::Program,
    solid_program: glow::Program,
    texture: glow::Texture,
    vao: glow::VertexArray,
    size: (i32, i32),
    game_texture: glow::Texture,
    game_size: (i32, i32),
    // 0xRRGGBB behind the picture and in the system-bar insets: the touch pause sheet's page colour, else black.
    backdrop: u32,
    effect: usize,
}
impl Graphics {
    fn new() -> Result<Self, String> {
        unsafe {
            let gl = glow::Context::from_loader_function_cstr(|name| eglGetProcAddress(name.as_ptr()));
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
    fn present(
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
#[derive(Clone, Copy, PartialEq)]
enum Menu {
    Pause,
    Options,
}
// Dereferencing the core makes its isolated context current for every entry point,
// including save/load, options and destruction; composition explicitly restores the Activity.
fn core_hw_frame(core: &CoreSession) -> Option<pocket_ui::libretro::Frame> {
    core.hw.as_ref()?.capture()
}
struct CoreSession {
    core: std::mem::ManuallyDrop<Core>,
    hw: Option<pocket_ui::hw_render::HwContext>,
}
impl CoreSession {
    fn new(core: Core) -> Result<Self, String> {
        let g = core.av.geometry;
        let hw = pocket_ui::hw_render::HwContext::create(g.max_width.max(g.base_width), g.max_height.max(g.base_height))?;
        log::info!(
            "Android core renderer: {}",
            if hw.is_some() { "GLES 3 (isolated EGL context)" } else { "software" }
        );
        Ok(Self {
            core: std::mem::ManuallyDrop::new(core),
            hw,
        })
    }
    fn frontend_current(&self) {
        if let Some(hw) = &self.hw {
            hw.frontend_current();
        }
    }
}
impl std::ops::Deref for CoreSession {
    type Target = Core;
    fn deref(&self) -> &Core {
        if let Some(hw) = &self.hw {
            hw.core_current();
        }
        &self.core
    }
}
impl std::ops::DerefMut for CoreSession {
    fn deref_mut(&mut self) -> &mut Core {
        if let Some(hw) = &self.hw {
            hw.core_current();
        }
        &mut self.core
    }
}
impl Drop for CoreSession {
    fn drop(&mut self) {
        if let Some(hw) = &mut self.hw {
            hw.destroy_callback();
            hw.core_current();
        }
        unsafe {
            std::mem::ManuallyDrop::drop(&mut self.core);
        }
    }
}
struct Player {
    core: CoreSession,
    audio: Option<pocket_ui::audio::Audio>,
    gfx: Graphics,
    text: Text,
    pixels: Vec<u32>,
    overlay_size: Option<(u32, u32)>,
    touch_visible: bool,
    frame: Option<Image>,
    game_area: Option<Area>,
    stylus_finger: Option<i32>,
    title: String,
    system: String,
    base: PathBuf,
    data: PathBuf,
    key: String,
    paused: bool,
    crt_started: Option<Instant>,
    crt_done: bool,
    light: bool,
    menu: Menu,
    row: usize,
    slot: usize,
    filled_slots: [bool; 8],
    slot_icon: Option<pocket_ui::system_art::Sprite>,
    sounds: pocket_ui::ui_sound::Config,
    sounds_muted: bool,
    message: String,
    quit: bool,
    picture: picture::Settings,
    options: Vec<FileOption>,
    saved_options: std::collections::BTreeMap<String, String>,
    options_path: PathBuf,
    buttons: u16,
    pulses: u16,
    stick_buttons: u16,
    stick_direction: i32,
    menu_drag: Option<(i32, f32, f32)>,
    // Touch pause menu: the finger whose tap only revealed it, the Options page's pixel scroll and its drag, and the
    // targets of the last paint (slot arrows, option arrows `(area, row, step, label)`).
    swallow: Option<i32>,
    scroll: i32,
    scroll_max: i32,
    /// The options page's "Advanced (this emulator)" group is open.
    advanced_open: bool,
    /// Overlay pixels per dp (from the last frame's density).
    dp: f32,
    /// The touch D-pad: its centre and radius in overlay pixels (8-way from the finger's angle).
    dpad: Option<(f32, f32, f32)>,
    scroll_base: i32,
    dragging: bool,
    slot_arrows: [Area; 2],
    option_arrows: Vec<(Area, usize, i32, String)>,
    fingers: HashMap<i32, u16>,
    controls: Vec<(Area, u16)>,
    menu_button: Area,
    hits: Vec<(Area, usize, String)>,
    semantics: String,
    top: i32,
    view: (i32, i32, i32, i32),
    logical: (u32, u32),
    next: Instant,
    period: Duration,
    frames: u64,
    last_flush: Instant,
    last_earlier: Instant,
    earlier: usize,
}
impl Player {
    fn new(core_path: &str, rom: &str, system: String, key: String, title: String, light: bool) -> Result<Self, String> {
        if key.len() != 16 || !key.bytes().all(|c| c.is_ascii_hexdigit()) {
            return Err("Invalid game identity".into());
        }
        let data = pocket_ui::platform::data_dir();
        // Decode shared effects beside core loading, before the first visible-frame CRT hook.
        pocket_ui::ui_sound::start();
        let saves = data.join("saves").join(&key);
        let states = data.join("states").join(&key);
        let bios = data.join("bios");
        for path in [&saves, &states, &bios] {
            std::fs::create_dir_all(path).map_err(|e| e.to_string())?;
        }
        let base = states.join(Path::new(rom).file_stem().unwrap_or_default());
        let alternative = Path::new(core_path).file_name().and_then(|n| n.to_str()) == Some("libgambatte_libretro.so");
        let options_name = if alternative {
            format!("{system}-gambatte.json")
        } else {
            format!("{system}.json")
        };
        let options_path = data.join("core-options").join(options_name);
        let saved_options: std::collections::BTreeMap<String, String> = pocket_ui::fsutil::read_json_or_backup(&options_path).unwrap_or_default();
        let selected: Vec<_> = saved_options.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        let mut core = CoreSession::new(Core::load(Path::new(core_path), Path::new(rom), &bios, &saves, &selected)?)?;
        let (_, _, av, geometry) = core.run(0);
        if let Some(av) = av {
            core.av = av;
        }
        if let Some(geometry) = geometry {
            core.av.geometry = geometry;
        }
        let resume = core.resume(&pocket_ui::slots::auto_path(&base));
        let period = Duration::from_secs_f64(1.0 / core.av.timing.fps.clamp(10.0, 240.0));
        let audio = pocket_ui::audio::Audio::open(core.av.timing.sample_rate, false);
        if audio.is_none() {
            return Err("AAudio output unavailable".into());
        }
        let options = core.options_snapshot().1;
        log::info!(
            "pocket-core in-app: {} {} fps={} sample_rate={} resume={resume:?}",
            core.name,
            core.version,
            core.av.timing.fps,
            core.av.timing.sample_rate
        );
        core.frontend_current();
        let gfx = Graphics::new()?;
        let now = Instant::now();
        let initial = core
            .save_state()
            .and_then(|bytes| pocket_ui::fsutil::write_atomic(&base.with_extension("earlier0"), &bytes).ok().map(|_| 1))
            .unwrap_or(0);
        let filled_slots = std::array::from_fn(|i| pocket_ui::slots::state_path(&base, i).is_file());
        let slot_icon = pocket_ui::system_art::get(&system).map(|sprite| sprite.fitted(26, 26));
        let sounds = pocket_ui::ui_sound::Config::load(&data);
        let picture = picture::Settings::load(&data, &system, &key);
        Ok(Self {
            core,
            audio,
            gfx,
            text: Text::builtin().following_font(),
            pixels: vec![],
            overlay_size: None,
            touch_visible: pocket_ui::platform::android::string("hasGamepad") != "true",
            frame: None,
            game_area: None,
            stylus_finger: None,
            title,
            system,
            base,
            data,
            key,
            paused: false,
            crt_started: None,
            crt_done: false,
            light,
            menu: Menu::Pause,
            row: 0,
            slot: 0,
            filled_slots,
            slot_icon,
            sounds,
            sounds_muted: false,
            message: String::new(),
            quit: false,
            picture,
            options,
            saved_options,
            options_path,
            buttons: 0,
            pulses: 0,
            stick_buttons: 0,
            stick_direction: 0,
            menu_drag: None,
            swallow: None,
            scroll: 0,
            scroll_max: 0,
            advanced_open: false,
            dp: 2.2,
            dpad: None,
            scroll_base: 0,
            dragging: false,
            slot_arrows: [Area::new(0, 0, 0, 0); 2],
            option_arrows: vec![],
            fingers: HashMap::new(),
            controls: vec![],
            menu_button: Area::new(0, 0, 1, 1),
            hits: vec![],
            semantics: String::new(),
            top: 0,
            view: (0, 0, 1, 1),
            logical: (1, 1),
            next: now,
            period,
            frames: 0,
            last_flush: now,
            last_earlier: now,
            earlier: initial,
        })
    }
    fn state(&mut self, path: &Path) -> Result<(), String> {
        let bytes = self.core.save_state().ok_or("This core could not save its state")?;
        pocket_ui::fsutil::write_atomic(path, &bytes).map_err(|e| e.to_string())?;
        if let Some(image) = &self.frame {
            png(&pocket_ui::slots::preview_path(path), image)?;
        }
        self.refresh_slots();
        log::info!("player state saved: {} bytes={}", path.display(), bytes.len());
        Ok(())
    }
    fn automatic(&mut self) -> Result<(), String> {
        let path = pocket_ui::slots::auto_path(&self.base);
        self.state(&path)?;
        self.core.write_sram().map_err(|e| format!("{e:?}"))?;
        pocket_ui::slots::auto_written(&path);
        Ok(())
    }
    fn load(&mut self, path: &Path) -> Result<(), String> {
        let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
        let undo = self.core.save_state().ok_or("Cannot preserve the current state")?;
        let undo_path = pocket_ui::slots::undo_path(&self.base);
        pocket_ui::fsutil::write_atomic(&undo_path, &undo).map_err(|e| e.to_string())?;
        if let Err(error) = self.core.load_state(&bytes) {
            let _ = self.core.load_state(&undo);
            return Err(error);
        }
        log::info!("player state loaded: {}", path.display());
        Ok(())
    }
    fn refresh_slots(&mut self) {
        self.filled_slots = std::array::from_fn(|i| pocket_ui::slots::state_path(&self.base, i).is_file());
    }
    fn sound(&self, sound: pocket_ui::ui_sound::Sound) {
        // The shared mixer uses Android AAudio and the app sound preference; Android owns master volume.
        let _ = pocket_ui::ui_sound::play(
            sound,
            &pocket_ui::ui_sound::Ctx {
                config: pocket_ui::ui_sound::Config {
                    on: self.sounds.on && !self.sounds_muted,
                    ..self.sounds
                },
                in_game: true,
                pause_menu: true,
                asleep: false,
            },
        );
    }
    fn pause(&mut self, on: bool) {
        if !on && self.audio.is_none() {
            self.audio = pocket_ui::audio::Audio::open(self.core.av.timing.sample_rate, false);
        }
        if on {
            self.crt_done = true;
            self.refresh_slots();
            self.sounds = pocket_ui::ui_sound::Config::load(&self.data);
        }
        self.sound(if on {
            pocket_ui::ui_sound::Sound::Menu
        } else {
            pocket_ui::ui_sound::Sound::Back
        });
        self.paused = on;
        self.overlay_size = None;
        self.buttons = 0;
        self.pulses = 0;
        self.stick_buttons = 0;
        self.stick_direction = 0;
        self.core.set_analog([0; 4]);
        self.core.set_pointer(None);
        self.stylus_finger = None;
        self.menu_drag = None;
        self.fingers.clear();
        self.next = Instant::now();
        if let Some(audio) = &self.audio {
            audio.pause(on);
        }
        self.menu = Menu::Pause;
        self.row = 0;
        self.scroll = 0;
        self.swallow = None;
        log::info!("player paused={on}");
    }
    fn analog(&mut self, axes: [f32; 4], hat: [f32; 2]) -> bool {
        if axes.iter().chain(hat.iter()).any(|v| v.abs() > 0.55) {
            self.crt_done = true;
        }
        self.core.set_analog(axes.map(|v| (v.clamp(-1.0, 1.0) * 32767.0) as i16));
        let x = if hat[0].abs() > 0.5 { hat[0] } else { axes[0] };
        let y = if hat[1].abs() > 0.5 { hat[1] } else { axes[1] };
        self.stick_buttons = (if x < -0.55 {
            1 << 6
        } else if x > 0.55 {
            1 << 7
        } else {
            0
        }) | (if y < -0.55 {
            1 << 4
        } else if y > 0.55 {
            1 << 5
        } else {
            0
        });
        let direction = if x.abs() > y.abs() {
            if x < -0.55 {
                21
            } else if x > 0.55 {
                22
            } else {
                0
            }
        } else if y < -0.55 {
            19
        } else if y > 0.55 {
            20
        } else {
            0
        };
        if direction != self.stick_direction {
            self.stick_direction = direction;
            if self.paused && direction != 0 {
                self.key(direction, true);
            }
        }
        !self.paused
    }
    fn key(&mut self, key: i32, down: bool) -> bool {
        if down {
            self.crt_done = true;
        }
        if matches!(key, 4 | 82 | 110) {
            if down {
                if self.paused && self.menu == Menu::Options {
                    self.menu = Menu::Pause;
                    self.row = 0;
                } else {
                    self.pause(!self.paused);
                }
            }
            return !self.paused;
        }
        if self.paused {
            if !down {
                return false;
            }
            if matches!(key, 19..=22 | 102 | 103) {
                self.sound(pocket_ui::ui_sound::Sound::Nav);
            }
            match key {
                19 | 20 if self.menu == Menu::Options => {
                    // Up/Down follow the page's order (by section), then Back.
                    let options = self.display_options();
                    let mut order = option_page(&options, self.advanced_open);
                    order.push(options.len());
                    let at = order.iter().position(|r| *r == self.row).unwrap_or(0);
                    let next = if key == 19 { at.saturating_sub(1) } else { (at + 1).min(order.len() - 1) };
                    self.row = order[next];
                }
                19 => self.row = self.row.saturating_sub(1),
                20 => self.row = (self.row + 1).min(self.rows().len().saturating_sub(1)),
                21 | 102 => {
                    if self.menu == Menu::Pause {
                        self.slot = (self.slot + 7) % 8
                    } else {
                        self.change_option(-1)
                    }
                }
                22 | 103 => {
                    if self.menu == Menu::Pause {
                        self.slot = (self.slot + 1) % 8
                    } else {
                        self.change_option(1)
                    }
                }
                97 | 111 => {
                    if self.menu == Menu::Options {
                        self.menu = Menu::Pause;
                        self.row = 0;
                    } else {
                        self.pause(false)
                    }
                }
                23 | 66 | 96 => self.activate(),
                _ => (),
            }
        } else if let Some(bit) = key_bit(key) {
            if down {
                self.buttons |= 1 << bit;
                self.pulses |= 1 << bit;
            } else {
                self.buttons &= !(1 << bit);
            }
        }
        !self.paused
    }
    fn display_options(&self) -> Vec<FileOption> {
        let mut options = self.picture.options();
        options.extend(self.options.iter().cloned());
        for (row, option) in options.iter_mut().enumerate() { option.category = Some(Self::section(row).into()); }
        options
    }
    fn section(row: usize) -> &'static str {
        if row < picture::ROWS { "Picture" } else { "Emulator" }
    }
    fn rows(&self) -> Vec<String> {
        if self.menu == Menu::Options {
            let mut rows: Vec<_> = self.display_options().iter().map(|o| format!("{} · {}", o.label, o.label_of(&o.current))).collect();
            rows.push("Back".into());
            return rows;
        }
        vec![
            "Continue".into(),
            format!("Save · Slot {}", self.slot + 1),
            format!("Load · Slot {}", self.slot + 1),
            "Earlier".into(),
            "Options".into(),
            "Screenshot".into(),
            "Quit".into(),
        ]
    }
    fn change_option(&mut self, delta: i32) {
        if self.row < picture::ROWS {
            if let Err(error) = self.picture.change(self.row, delta) { self.message = error; }
            self.overlay_size = None;
            return;
        }
        let Some(option) = self.options.get_mut(self.row - picture::ROWS) else { return };
        if option.values.is_empty() {
            return;
        }
        let at = option.values.iter().position(|v| v.value == option.current).unwrap_or(0);
        let i = (at as i32 + delta).rem_euclid(option.values.len() as i32) as usize;
        let value = option.values[i].value.clone();
        if self.core.set_option(&option.key, &value) {
            option.current = value.clone();
            self.saved_options.insert(option.key.clone(), value);
            pocket_ui::fsutil::write_json_soon(&self.options_path, &self.saved_options);
        }
    }
    fn activate(&mut self) {
        if self.menu == Menu::Options {
            if self.row == self.options.len() + picture::ROWS + 1 {
                // "Advanced (this emulator)": opens or closes its group.
                self.advanced_open = !self.advanced_open;
                self.sound(pocket_ui::ui_sound::Sound::Confirm);
            } else if self.row >= self.options.len() + picture::ROWS {
                self.menu = Menu::Pause;
                self.row = 0;
            } else {
                self.change_option(1);
                self.sound(pocket_ui::ui_sound::Sound::Confirm);
            }
            return;
        }
        let result = match self.row {
            0 => {
                self.pause(false);
                return;
            }
            1 => {
                let path = pocket_ui::slots::state_path(&self.base, self.slot);
                self.state(&path)
            }
            2 => {
                let path = pocket_ui::slots::state_path(&self.base, self.slot);
                self.load(&path)
            }
            3 => {
                let path = self.base.with_extension(format!("earlier{}", self.earlier.saturating_sub(1) % 6));
                let result = self.load(&path);
                if result.is_ok() {
                    self.earlier = self.earlier.saturating_sub(1);
                }
                result
            }
            4 => {
                self.options = self.core.options_snapshot().1;
                self.menu = Menu::Options;
                self.row = 0;
                self.scroll = 0;
                return;
            }
            5 => {
                let path = self.data.join("screenshots").join(format!(
                    "{}-{}.png",
                    self.key,
                    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
                ));
                self.frame.as_ref().ok_or_else(|| "No frame yet".into()).and_then(|image| png(&path, image))
            }
            _ => {
                let saved = self.automatic();
                if saved.is_ok() {
                    self.quit = true;
                    self.audio.take();
                }
                saved
            }
        };
        self.sound(if result.is_err() {
            pocket_ui::ui_sound::Sound::Error
        } else if self.row == 5 {
            pocket_ui::ui_sound::Sound::Shutter
        } else {
            pocket_ui::ui_sound::Sound::Confirm
        });
        let slot = self.slot + 1;
        self.message = match (result, self.row) {
            (Ok(()), 1) => format!("Saved to slot {slot}"),
            (Ok(()), 2) => format!("Loaded slot {slot}"),
            (Ok(()), 3) => "Loaded an earlier moment".into(),
            (Ok(()), 5) => "Screenshot saved".into(),
            (Ok(()), _) => "Saved".into(),
            (Err(e), _) => e,
        };
    }
    fn input_mode(&mut self, touch: bool) {
        if self.touch_visible != touch {
            self.touch_visible = touch;
            self.overlay_size = None;
            self.fingers.clear();
            self.core.set_pointer(None);
            self.stylus_finger = None;
            self.pulses = 0;
            self.menu_drag = None;
        }
    }
    fn slot_controls(&self) -> Vec<(Area, i32, &'static str)> {
        if !self.paused || self.menu != Menu::Pause {
            return Vec::new();
        }
        // The sheet's slot picker arrows, for touch and controller alike.
        vec![
            (self.slot_arrows[0], 102, "Previous slot"),
            (self.slot_arrows[1], 103, "Next slot"),
        ]
    }
    fn touch(&mut self, id: i32, x: f32, y: f32, phase: i32) -> bool {
        if phase == 0 {
            self.crt_done = true;
        }
        if phase == 3 {
            self.fingers.clear();
            self.core.set_pointer(None);
            self.stylus_finger = None;
            self.pulses = 0;
            self.menu_drag = None;
            self.swallow = None;
            self.dragging = false;
            return !self.paused;
        }
        if phase == 0 && !self.touch_visible {
            self.input_mode(true);
            // A first touch reveals the controls (or the touch menu) without firing an invisible button.
            if !self.paused {
                return true;
            }
            self.swallow = Some(id);
            return false;
        }
        if self.swallow == Some(id) {
            if phase == 2 {
                self.swallow = None;
            }
            return false;
        }
        let x = (x - self.view.0 as f32) * self.logical.0 as f32 / self.view.2.max(1) as f32;
        // View stores the bottom inset for GL; input's top inset is kept in menu_button's map by frame().
        let top = self.input_top();
        let y = (y - top) * self.logical.1 as f32 / self.view.3.max(1) as f32;
        if self.paused {
            // A tap activates directly. A finger that travelled is a drag: it scrolls the Options page, else it is nothing.
            if phase == 0 {
                self.menu_drag = Some((id, x, y));
                self.scroll_base = self.scroll;
                self.dragging = false;
            }
            if phase == 1 {
                if let Some((_, _, sy)) = self.menu_drag.filter(|(finger, _, _)| *finger == id) {
                    if self.dragging || (y - sy).abs() > 24.0 {
                        self.dragging = true;
                        if self.menu == Menu::Options {
                            let scroll = (self.scroll_base as f32 - (y - sy)) as i32;
                            self.scroll = scroll.clamp(0, self.scroll_max);
                        }
                    }
                }
            }
            if phase == 2 {
                let drag = self.menu_drag.take().filter(|(finger, _, _)| *finger == id);
                let moved = drag.is_some_and(|(_, sx, sy)| self.dragging || (x - sx).abs().max((y - sy).abs()) > 35.0);
                self.dragging = false;
                if drag.is_none() || moved {
                    return false;
                }
                if let Some(key) = self.slot_controls().iter().find(|(a, _, _)| a.contains(x, y)).map(|(_, key, _)| *key) {
                    self.key(key, true);
                } else if let Some((row, step)) = self.option_arrows.iter().find(|(a, _, _, _)| a.contains(x, y)).map(|(_, row, step, _)| (*row, *step)) {
                    if self.menu == Menu::Options {
                        self.row = row;
                        self.change_option(step);
                        self.sound(pocket_ui::ui_sound::Sound::Nav);
                    }
                } else if let Some(row) = self.hits.iter().find(|(a, _, _)| a.contains(x, y)).map(|(_, row, _)| *row) {
                    self.row = row;
                    self.activate();
                }
            }
            return !self.paused;
        }
        if phase == 0 && self.menu_button.contains(x, y) {
            self.pause(true);
            return false;
        }
        let control_bits = self.bits_at(x, y);
        if self.system == "nds" {
            if phase == 0 && control_bits == 0 && self.stylus_finger.is_none() && self.game_area.is_some_and(|a| a.contains(x, y)) {
                self.stylus_finger = Some(id);
            }
            if self.stylus_finger == Some(id) {
                let point = if phase == 2 {
                    None
                } else {
                    self.game_area.filter(|a| a.contains(x, y)).map(|a| {
                        (
                            ((x - a.x as f32) / a.w.max(1) as f32 * 65534.0 - 32767.0) as i16,
                            ((y - a.y as f32) / a.h.max(1) as f32 * 65534.0 - 32767.0) as i16,
                        )
                    })
                };
                self.core.set_pointer(point);
                if phase == 2 {
                    self.stylus_finger = None;
                }
                return true;
            }
        }
        if phase == 2 {
            if let Some(bits) = self.fingers.remove(&id) {
                self.pulses |= bits;
                if bits != 0 {
                    self.overlay_size = None;
                }
            }
        } else {
            let bits = control_bits;
            let before = self.fingers.insert(id, bits).unwrap_or(0);
            if bits & !before != 0 {
                // A tick for each new press (and for sliding onto another direction or button).
                let _ = pocket_ui::platform::android::strings("haptic", &[]);
            }
            if bits != before {
                // The pressed control lights up: the cached overlay is painted again.
                self.overlay_size = None;
            }
        }
        !self.paused
    }
    /// The pad bits under (x, y): the buttons' areas, and the D-pad by the finger's angle from its centre (8 ways;
    /// a small dead middle; generous edges so a thumb can slide round it).
    fn bits_at(&self, x: f32, y: f32) -> u16 {
        let mut bits = self.controls.iter().filter(|(a, _)| a.contains(x, y)).fold(0u16, |v, (_, bit)| v | 1 << bit);
        if let Some((cx, cy, r)) = self.dpad {
            let (dx, dy) = (x - cx, y - cy);
            let d = (dx * dx + dy * dy).sqrt();
            if d > r * 0.18 && d < r * 1.35 {
                let sector = ((dy.atan2(dx).to_degrees() + 360.0 + 22.5) % 360.0 / 45.0) as u8;
                // 0 right, 1 down-right, 2 down, 3 down-left, 4 left, 5 up-left, 6 up, 7 up-right.
                let (right, down, left, up) = (1 << 7, 1 << 5, 1 << 6, 1 << 4);
                bits |= [right, right | down, down, down | left, left, left | up, up, up | right][sector as usize % 8];
            }
        }
        bits
    }
    fn input_top(&self) -> f32 {
        self.top as f32
    }
    fn frame(&mut self, w: i32, h: i32, density: f32, insets: [i32; 4]) -> i32 {
        if self.quit {
            return 2;
        }
        let [left, top, right, bottom] = insets;
        self.top = top;
        let view = (left, bottom, (w - left - right).max(1), (h - top - bottom).max(1));
        if self.view != view {
            log::info!("player viewport: physical={w}x{h} insets={insets:?} density={density}");
        }
        self.view = view;
        self.logical = pocket_ui::scaled_layout::mobile_size((self.view.2 as u32, self.view.3 as u32), density);
        // Overlay pixels per dp: the touch controls are sized in dp (at least 56 dp a button).
        self.dp = self.logical.0 as f32 / (self.view.2.max(1) as f32 / density.max(0.5));
        // The compositor can run below the core rate (notably SwiftShader on high-DPI phones).
        // Catch up bounded emulation ticks before painting once; audio must not lose samples when
        // a presentation misses vsync. Core calls remain on the EGL thread for hardware cores.
        let mut ticks = 0;
        while !self.paused && Instant::now() >= self.next && ticks < 4 {
            ticks += 1;
            let touch = self.fingers.values().fold(self.pulses, |v, b| v | b);
            let mut buttons = self.buttons | touch | self.stick_buttons;
            self.pulses = 0;
            if self.system == "n64" && self.touch_visible {
                let axis = |negative: u32, positive: u32| {
                    if touch & (1u16 << negative) != 0 {
                        -32767
                    } else if touch & (1u16 << positive) != 0 {
                        32767
                    } else {
                        0
                    }
                };
                self.core.set_analog([axis(6, 7), axis(4, 5), axis(8, 13), axis(9, 14)]);
                // Touch C buttons use the core's right stick, independent of its face-button map.
                buttons &= !((1 << 4) | (1 << 5) | (1 << 6) | (1 << 7) | (1 << 8) | (1 << 9) | (1 << 13) | (1 << 14));
            }
            let (frame, samples, av, geometry) = self.core.run(buttons);
            if let Some(av) = av {
                self.core.av = av;
                self.period = Duration::from_secs_f64(1.0 / av.timing.fps.clamp(10.0, 240.0));
                if let Some(audio) = &mut self.audio {
                    audio.set_input_rate(av.timing.sample_rate);
                }
            }
            if let Some(geometry) = geometry {
                self.core.av.geometry = geometry;
            }
            let frame = core_hw_frame(&self.core).or(frame);
            if let Some(frame) = frame {
                let rgb = frame.to_rgb();
                let px = rgb
                    .as_chunks::<3>()
                    .0
                    .iter()
                    .map(|p| ((p[0] as u32) << 16) | ((p[1] as u32) << 8) | p[2] as u32)
                    .collect();
                self.frame = Some(Image {
                    w: frame.width,
                    h: frame.height,
                    px,
                });
            }
            if let Some(audio) = &mut self.audio {
                audio.push(&samples);
            }
            self.frames += 1;
            self.next += self.period;
            if self.last_flush.elapsed() > Duration::from_secs(10) {
                self.core.write_sram_in_background();
                self.last_flush = Instant::now();
                if let Some(audio) = &self.audio {
                    let underruns = *audio.underruns.lock().unwrap_or_else(|e| e.into_inner());
                    let queue_ms = audio.buffered_ms();
                    log::info!("player audio: frames={} underruns={} queue_ms={queue_ms:.2}", self.frames, underruns);
                }
            }
            if self.last_earlier.elapsed() > Duration::from_secs(300) {
                let path = self.base.with_extension(format!("earlier{}", self.earlier % 6));
                if self.state(&path).is_ok() {
                    self.earlier += 1;
                }
                self.last_earlier = Instant::now();
            }
        }
        self.paint();
        if self.paused {
            1
        } else {
            0
        }
    }
    fn control_label(&self, bit: u16) -> &'static str {
        if self.system == "n64" {
            match bit {
                0 => return "A",
                1 => return "B",
                8 => return "C<",
                9 => return "C^",
                13 => return "C>",
                14 => return "Cv",
                12 => return "Z",
                _ => (),
            }
        }
        if self.system == "psp" {
            match bit {
                0 => return "×",
                8 => return "○",
                1 => return "□",
                9 => return "△",
                _ => (),
            }
        }
        if self.system == "megadrive" {
            match bit {
                1 => return "A",
                0 => return "B",
                8 => return "C",
                10 => return "X",
                9 => return "Y",
                11 => return "Z",
                _ => (),
            }
        }
        [
            "B", "Y", "Select", "Start", "Up", "Down", "Left", "Right", "A", "X", "L", "R", "L2", "R2", "L3", "R3",
        ][bit as usize]
    }
    fn semantic_nodes(&self) -> String {
        let areas: Vec<_> = if self.paused {
            let mut nodes: Vec<_> = self.hits.iter().map(|(a, i, label)| (*a, label.clone(), !self.touch_visible && *i == self.row)).collect();
            nodes.extend(self.slot_controls().into_iter().map(|(a, _, label)| (a, label.to_string(), false)));
            if self.menu == Menu::Options {
                nodes.extend(self.option_arrows.iter().map(|(a, _, _, label)| (*a, label.clone(), false)));
            }
            nodes
        } else {
            let mut nodes: Vec<_> = self.controls.iter().map(|(a, bit)| (*a, self.control_label(*bit).to_string(), false)).collect();
            if self.touch_visible {
                nodes.push((self.menu_button, "Pause".into(), false));
            }
            nodes
        };
        let nodes:Vec<_>=areas.into_iter().enumerate().map(|(id,(a,label,selected))|serde_json::json!({"id":id,"label":label,"selected":selected,"x":self.view.0+a.x*self.view.2/self.logical.0 as i32,"y":self.top+a.y*self.view.3/self.logical.1 as i32,"w":a.w*self.view.2/self.logical.0 as i32,"h":a.h*self.view.3/self.logical.1 as i32})).collect();
        serde_json::to_string(&nodes).unwrap_or_default()
    }
    fn paint(&mut self) {
        palette::set_blend(if self.light { 255 } else { 0 });
        let (w, h) = (self.logical.0 as i32, self.logical.1 as i32);
        let game_rect = self.frame.as_ref().map(|image| self.picture.rect(
            (w,h), (self.view.2,self.view.3), (image.w,image.h),
            self.core.av.geometry.aspect_ratio as f64, self.touch_visible,
        ));
        self.gfx.effect = self.picture.effect;
        let crt = if let Some(rect) = game_rect.filter(|_| !self.paused && !self.crt_done) {
            let started = if let Some(started) = self.crt_started {
                started
            } else {
                let now = Instant::now();
                self.crt_started = Some(now);
                pocket_ui::crt_on::start();
                self.sound(pocket_ui::ui_sound::Sound::CrtOn);
                now
            };
            let quads = pocket_ui::crt_on::plan(started.elapsed(), (rect.x as f32, rect.y as f32, rect.w as f32, rect.h as f32));
            if quads.is_empty() {
                self.crt_done = true;
            }
            quads
        } else {
            Vec::new()
        };
        if let Some(rect) = game_rect {
            let changed = self.game_area.is_none_or(|old| (old.x,old.y,old.w,old.h)!=(rect.x,rect.y,rect.w,rect.h));
            if changed { log::info!("player picture: system={} logical={}x{} rect={},{},{}x{} physical={}x{} scaling={} effect={}", self.system,w,h,rect.x,rect.y,rect.w,rect.h,self.view.2,self.view.3,self.picture.scaling,self.picture.effect); }
        }
        self.game_area = game_rect;
        let game = if self.paused { None } else { self.frame.as_ref().zip(game_rect) };
        if !self.paused && self.overlay_size == Some(self.logical) {
            self.core.frontend_current();
            self.gfx.present(&self.pixels, (w, h), self.view, game, false, &crt);
            return;
        }
        self.pixels.resize((w * h) as usize, 0);
        let rows = self.rows();
        let display_options = self.display_options();
        let mut c = Canvas::new(&mut self.pixels, w as u32, h as u32);
        c.clear(0);
        if let Some(rect) = game_rect { self.picture.paint_frame(&mut c, rect, &self.system); }
        // Paused backgrounds are painted only when the menu changes; active frames are scaled by GLES.
        if self.paused {
            if let Some((image, rect)) = self.frame.as_ref().zip(game_rect) {
                c.blit_scaled(image, rect.x, rect.y, rect.w, rect.h);
            }
        }
        self.controls.clear();
        self.menu_button = if self.touch_visible {
            Area::new(w / 2 - 70, h - 78, 140, 64)
        } else {
            Area::new(0, 0, 0, 0)
        };
        if !self.paused && self.touch_visible {
            let pressed = self.fingers.values().fold(0u16, |v, b| v | b);
            let out = touch_controls(&mut c, &mut self.text, &self.system, (w, h), self.dp, game_rect, pressed);
            self.controls = out.0;
            self.dpad = out.1;
            self.menu_button = out.2;
        } else if self.paused {
            // A flat sheet for touch and controller alike (a controller gets a focus ring and a hint line): the
            // paused picture is only a thumbnail (portrait), never behind the buttons.
            c.clear(pal().void);
            let icon = self
                .slot_icon
                .as_ref()
                .map_or(pocket_ui::save_media::Icon::None, pocket_ui::save_media::Icon::Console);
            let view = TouchView {
                size: (w, h),
                title: &self.title,
                message: &self.message,
                rows: &rows,
                menu: self.menu,
                slot: self.slot,
                filled: &self.filled_slots,
                kind: pocket_ui::save_media::Kind::of(&self.system),
                system: &self.system,
                icon,
                options: &display_options,
                scroll: self.scroll,
                frame: self.frame.as_ref().zip(game_rect).map(|(image, rect)| (image, rect.w as f64 / rect.h.max(1) as f64)),
                focus: (!self.touch_visible).then_some(self.row),
                advanced_open: self.advanced_open,
                card: pocket_ui::cozy_extras::on(pocket_ui::cozy_extras::Extra::PauseMemoryCard),
            };
            let out = touch_pause(&mut c, &mut self.text, &view);
            self.hits = out.hits;
            self.option_arrows = out.option_arrows;
            self.scroll = out.scroll;
            self.scroll_max = out.scroll_max;
            if let [Some(a), Some(b)] = out.slot_arrows {
                self.slot_arrows = [a, b];
            }
        }
        self.core.frontend_current();
        self.gfx.backdrop = if self.paused { pal().void } else { 0 };
        self.gfx.present(&self.pixels, (w, h), self.view, game, true, &crt);
        self.overlay_size = if self.paused { None } else { Some(self.logical) };
    }
}
// Touch pause menu. Sizes are logical pixels (about 2.2 per dp): every target is at least 106 (48 dp) high with
// gaps of 20 (9 dp); the primary button is 128 (58 dp).
const EDGE: i32 = 40;
const GAP: i32 = 20;
const TARGET_MIN: i32 = 106;
struct TouchView<'a> {
    size: (i32, i32),
    title: &'a str,
    message: &'a str,
    rows: &'a [String],
    menu: Menu,
    slot: usize,
    filled: &'a [bool; 8],
    kind: pocket_ui::save_media::Kind,
    system: &'a str,
    icon: pocket_ui::save_media::Icon<'a>,
    options: &'a [FileOption],
    scroll: i32,
    // The paused picture and its display aspect: a thumbnail in the room a portrait phone leaves above the buttons.
    frame: Option<(&'a Image, f64)>,
    // The controller's row (a ring round it and a hint line at the bottom); None under a finger.
    focus: Option<usize>,
    advanced_open: bool,
    // Cozy extra "Memory card in the pause menu": the whole card or cartridge under the slot name.
    card: bool,
}
#[derive(Default)]
struct TouchOut {
    hits: Vec<(Area, usize, String)>,
    slot_arrows: [Option<Area>; 2],
    option_arrows: Vec<(Area, usize, i32, String)>,
    scroll: i32,
    scroll_max: i32,
}
fn centered(c: &mut Canvas, text: &mut Text, face: Face, px: u32, area: Area, color: u32, label: &str) {
    let label = text.fit(face, px, label, area.w - 24);
    let width = text.width(face, px, &label);
    text.draw(c, face, px, area.x + (area.w - width) / 2, area.y + area.h / 2 + type_scale::mid(px), color, &label);
}
fn chevron(c: &mut Canvas, cx: i32, cy: i32, right: bool, color: u32) {
    let (reach, thick) = (18, 7);
    for i in 0..=reach {
        let x = if right { cx - reach / 2 + i } else { cx + reach / 2 - i - thick + 1 };
        c.rect(x, cy - reach + i, thick, 1, color);
        c.rect(x, cy + reach - i, thick, 1, color);
    }
}
fn pill(c: &mut Canvas, text: &mut Text, area: Area, label: &str, primary: bool) {
    // The primary button is the paper selection of the design language; on a light page paper would vanish, so ink.
    let (fill, on_fill) = if pal().is_light() { (pal().ink, pal().select) } else { (pal().select, pal().ink) };
    c.round_rect(area.x, area.y, area.w, area.h, area.h / 2, if primary { fill } else { pal().pill });
    if primary {
        centered(c, text, Face::Title, 52, area, on_fill, label);
    } else {
        centered(c, text, Face::Body, 38, area, pal().text, label);
    }
}
// ---- Touch controls ------------------------------------------------------------------------------------------------
// Drawn into the overlay with real transparency: the top byte of an overlay pixel is its alpha (the shader blends it);
// pixels without one (text, the picture's frame) stay opaque as before.

/// The controls' paper, warm like the shelf's label.
const PAD_PAPER: u32 = 0xece9e1;
/// Blends `color` at `alpha` (0-255) over the overlay pixel (x, y), keeping the result's alpha in the top byte.
fn put(c: &mut Canvas, x: i32, y: i32, color: u32, alpha: u32) {
    if x < 0 || y < 0 || x >= c.w || y >= c.h || alpha == 0 {
        return;
    }
    let p = &mut c.buf[(y * c.w + x) as usize];
    let (old, old_a) = (*p & 0xff_ffff, *p >> 24);
    if old != 0 && old_a == 0 {
        // An opaque pixel already (text or a frame): mix into it and keep it opaque.
        *p = pocket_ui::gfx::mix(old, color, alpha) & 0xff_ffff;
        return;
    }
    let a = alpha + old_a * (255 - alpha) / 255;
    let channel = |shift: u32| {
        let (s, d) = ((color >> shift) & 255, (old >> shift) & 255);
        ((s * alpha + d * old_a * (255 - alpha) / 255) / a.max(1)).min(255)
    };
    *p = (a.max(1) << 24) | (channel(16) << 16) | (channel(8) << 8) | channel(0);
}
/// Paints the pixels whose signed distance `sd(x, y)` (negative inside) is below zero, smoothed over one pixel.
fn fill(c: &mut Canvas, (x0, y0, x1, y1): (i32, i32, i32, i32), color: u32, alpha: u32, sd: impl Fn(f32, f32) -> f32) {
    for y in y0.max(0)..y1.min(c.h) {
        for x in x0.max(0)..x1.min(c.w) {
            let cover = (0.5 - sd(x as f32 + 0.5, y as f32 + 0.5)).clamp(0.0, 1.0);
            if cover > 0.0 {
                put(c, x, y, color, (alpha as f32 * cover) as u32);
            }
        }
    }
}
fn round_box(px: f32, py: f32, (cx, cy, hw, hh, r): (f32, f32, f32, f32, f32)) -> f32 {
    let (qx, qy) = ((px - cx).abs() - hw + r, (py - cy).abs() - hh + r);
    (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt() + qx.max(qy).min(0.0) - r
}
fn soft_disc(c: &mut Canvas, (cx, cy, r): (f32, f32, f32), color: u32, alpha: u32) {
    let b = (cx - r - 2.0, cy - r - 2.0, cx + r + 2.0, cy + r + 2.0);
    fill(c, (b.0 as i32, b.1 as i32, b.2 as i32 + 1, b.3 as i32 + 1), color, alpha, |x, y| ((x - cx).powi(2) + (y - cy).powi(2)).sqrt() - r);
}
fn soft_ring(c: &mut Canvas, (cx, cy, r): (f32, f32, f32), t: f32, color: u32, alpha: u32) {
    let b = (cx - r - 2.0, cy - r - 2.0, cx + r + 2.0, cy + r + 2.0);
    fill(c, (b.0 as i32, b.1 as i32, b.2 as i32 + 1, b.3 as i32 + 1), color, alpha, |x, y| (((x - cx).powi(2) + (y - cy).powi(2)).sqrt() - (r - t / 2.0)).abs() - t / 2.0);
}
fn soft_pill(c: &mut Canvas, a: Area, color: u32, alpha: u32) {
    let shape = (a.x as f32 + a.w as f32 / 2.0, a.y as f32 + a.h as f32 / 2.0, a.w as f32 / 2.0, a.h as f32 / 2.0, a.h as f32 / 2.0);
    fill(c, (a.x - 1, a.y - 1, a.x + a.w + 1, a.y + a.h + 1), color, alpha, |x, y| round_box(x, y, shape));
}
/// A small filled triangle pointing from (cx, cy) towards (dx, dy) (a unit direction), `s` across.
fn arrow(c: &mut Canvas, (cx, cy): (f32, f32), (dx, dy): (f32, f32), s: f32, color: u32, alpha: u32) {
    let (tip, base) = ((cx + dx * s * 0.55, cy + dy * s * 0.55), (cx - dx * s * 0.35, cy - dy * s * 0.35));
    let (nx, ny) = (-dy, dx);
    let pts = [tip, (base.0 + nx * s * 0.5, base.1 + ny * s * 0.5), (base.0 - nx * s * 0.5, base.1 - ny * s * 0.5)];
    let edge = |p: (f32, f32), a: (f32, f32), b: (f32, f32)| {
        let (ex, ey) = (b.0 - a.0, b.1 - a.1);
        let len = (ex * ex + ey * ey).sqrt().max(0.001);
        ((p.0 - a.0) * ey - (p.1 - a.1) * ex) / len
    };
    let (x0, y0) = ((cx - s) as i32, (cy - s) as i32);
    fill(c, (x0, y0, x0 + 2 * s as i32 + 2, y0 + 2 * s as i32 + 2), color, alpha, |x, y| {
        let p = (x, y);
        let d = [edge(p, pts[0], pts[1]), edge(p, pts[1], pts[2]), edge(p, pts[2], pts[0])];
        // Inside when every edge agrees (either winding).
        if d.iter().all(|v| *v <= 0.0) { d.iter().cloned().fold(f32::MIN, f32::max) } else if d.iter().all(|v| *v >= 0.0) { -d.iter().cloned().fold(f32::MAX, f32::min) } else { 1.0 }
    });
}
/// One round face button: translucent paper, a brighter rim, the letter (or PlayStation mark) small in the middle.
fn face_button(c: &mut Canvas, text: &mut Text, (cx, cy, r): (f32, f32, f32), label: &str, psp: Option<u16>, on: bool) {
    soft_disc(c, (cx, cy, r), PAD_PAPER, if on { 120 } else { 52 });
    soft_ring(c, (cx, cy, r), (r * 0.06).max(2.0), PAD_PAPER, if on { 220 } else { 130 });
    let glyph = 0xf4f1ea;
    let (x, y) = (cx as i32, cy as i32);
    let g = (r * 0.36) as i32;
    match psp {
        Some(8) => soft_ring(c, (cx, cy, g as f32), (r * 0.09).max(3.0), glyph, 230),
        Some(1) => {
            let t = (r * 0.09).max(3.0) as i32;
            for (ax, ay, aw, ah) in [(x - g, y - g, 2 * g, t), (x - g, y + g - t, 2 * g, t), (x - g, y - g, t, 2 * g), (x + g - t, y - g, t, 2 * g)] {
                fill(c, (ax, ay, ax + aw, ay + ah), glyph, 230, |_, _| -1.0);
            }
        }
        Some(9) => {
            let t = (r * 0.09).max(3.0);
            let (top, left, right) = ((cx, cy - g as f32), (cx - g as f32, cy + g as f32 * 0.75), (cx + g as f32, cy + g as f32 * 0.75));
            for (a, b) in [(top, left), (left, right), (right, top)] {
                let n = 64;
                for i in 0..=n {
                    let f = i as f32 / n as f32;
                    soft_disc(c, (a.0 + (b.0 - a.0) * f, a.1 + (b.1 - a.1) * f, t / 2.0), glyph, 60);
                }
            }
        }
        Some(0) => {
            let t = (r * 0.09).max(3.0);
            for i in -40..=40 {
                let f = i as f32 / 40.0 * g as f32;
                soft_disc(c, (cx + f, cy + f, t / 2.0), glyph, 45);
                soft_disc(c, (cx + f, cy - f, t / 2.0), glyph, 45);
            }
        }
        _ => {
            let size = ((r * 0.62) as u32).clamp(18, 44);
            let w = text.width(Face::Title, size, label);
            text.draw(c, Face::Title, size, x - w / 2, y + type_scale::mid(size), glyph, label);
        }
    }
}
/// The on-screen pad for a phone: an 8-way D-pad, the system's face buttons in its own layout, shoulder and
/// Start/Select pills and Pause, at least 56 dp a button, by the thumbs: either side of the picture in landscape,
/// under it in portrait. Returns the buttons' touch areas (with their pad bits), the D-pad's circle and Pause.
/// The pad's touch areas with their bits, the D-pad's circle (centre and radius) and the Pause button.
type PadLayout = (Vec<(Area, u16)>, Option<(f32, f32, f32)>, Area);
fn touch_controls(c: &mut Canvas, text: &mut Text, system: &str, (w, h): (i32, i32), dp: f32, game: Option<Area>, pressed: u16) -> PadLayout {
    let mut controls = Vec::new();
    let d = |v: f32| (v * dp) as i32;
    let portrait = w < h;
    let edge = d(18.0);
    // The clusters: D-pad diameter and face-button size.
    let face_r = d(30.0) as f32;
    // Portrait: the clusters low, where the thumbs rest, shoulders over them, the small pills under them; a tall
    // picture (two DS screens) leaves less room and the D-pad shrinks, never under 112 dp.
    let below = game.map_or(h / 2, |g| g.y + g.h) + d(10.0);
    let dpad_d = if portrait { (h - below - d(12.0 + 44.0 + 24.0 + 16.0 + 52.0)).clamp(d(112.0), d(150.0)) } else { d(150.0) } as f32;
    let (left_c, right_c, shoulder_y, util_y) = if portrait {
        let util_y = h - d(12.0) - d(44.0);
        let cy = util_y - d(24.0) - dpad_d as i32 / 2;
        let shoulder_y = cy - dpad_d as i32 / 2 - d(16.0) - d(52.0);
        ((edge as f32 + dpad_d / 2.0, cy as f32), ((w - edge) as f32 - dpad_d / 2.0, cy as f32), shoulder_y, util_y)
    } else {
        let cy = h as f32 * 0.58;
        ((edge as f32 + dpad_d / 2.0 + d(8.0) as f32, cy), ((w - edge) as f32 - dpad_d / 2.0 - d(8.0) as f32, cy), d(14.0), h - d(14.0) - d(44.0))
    };
    // D-pad: one cross, translucent, a dimple in the middle and four small arrows.
    let (cx, cy, r) = (left_c.0, left_c.1, dpad_d / 2.0);
    let arm = r * 0.72;
    let dir_on = |bit: u16| pressed & (1 << bit) != 0;
    let any = (4..8).any(dir_on);
    fill(c, ((cx - r) as i32 - 2, (cy - r) as i32 - 2, (cx + r) as i32 + 3, (cy + r) as i32 + 3), PAD_PAPER, if any { 96 } else { 52 }, |x, y| {
        round_box(x, y, (cx, cy, r, arm / 2.0, arm / 3.0)).min(round_box(x, y, (cx, cy, arm / 2.0, r, arm / 3.0)))
    });
    soft_disc(c, (cx, cy, arm * 0.28), 0x222222, 40);
    for (bit, dir) in [(4u16, (0.0, -1.0)), (5, (0.0, 1.0)), (6, (-1.0, 0.0)), (7, (1.0, 0.0))] {
        arrow(c, (cx + dir.0 * r * 0.66, cy + dir.1 * r * 0.66), dir, arm * 0.32, 0xf4f1ea, if dir_on(bit) { 255 } else { 170 });
    }
    let dpad = Some((cx, cy, r));
    // Face buttons around the right cluster's centre, in the system's own positions.
    let (fx, fy) = right_c;
    let s = face_r * 1.25;
    let buttons: Vec<(&str, u16, f32, f32, f32)> = match system {
        // Mega Drive six-button pad: X Y Z over A B C, the rows slanting up to the right like the pad's.
        "megadrive" => vec![("X", 10, -1.75, -0.7, 0.82), ("Y", 9, 0.0, -0.95, 0.82), ("Z", 11, 1.75, -1.2, 0.82), ("A", 1, -1.75, 1.0, 0.9), ("B", 0, 0.0, 0.75, 0.9), ("C", 8, 1.75, 0.5, 0.9)],
        // N64: A and B large, the four C buttons small above them (turned into the right stick before retro_run).
        "n64" => vec![("B", 1, -1.2, 0.9, 1.0), ("A", 0, 0.35, 1.45, 1.0), ("◀", 8, 0.0, -0.9, 0.62), ("▲", 9, 1.0, -1.85, 0.62), ("▶", 13, 2.0, -0.9, 0.62), ("▼", 14, 1.0, 0.05, 0.62)],
        "snes" | "psx" | "nds" | "psp" => vec![("Y", 1, -1.45, 0.0, 1.0), ("X", 9, 0.0, -1.45, 1.0), ("B", 0, 0.0, 1.45, 1.0), ("A", 8, 1.45, 0.0, 1.0)],
        _ => vec![("B", 0, -0.95, 0.55, 1.0), ("A", 8, 0.95, -0.55, 1.0)],
    };
    let psp = system == "psp";
    // The cluster stays inside the screen: a wide one (six buttons) moves in from the edge.
    let reach = buttons.iter().map(|(_, _, ox, _, k)| ox * s + face_r * k).fold(0.0f32, f32::max);
    let fx = fx.min((w - edge) as f32 - reach);
    for (label, bit, ox, oy, k) in buttons {
        let (bx, by, br) = (fx + ox * s, fy + oy * s, face_r * k);
        // The N64's C marks are not in the UI font: small drawn arrows instead.
        if system == "n64" && k < 0.9 {
            soft_disc(c, (bx, by, br), 0xe8c66a, if pressed & (1 << bit) != 0 { 140 } else { 70 });
            let dir = match bit { 8 => (-1.0, 0.0), 9 => (0.0, -1.0), 13 => (1.0, 0.0), _ => (0.0, 1.0) };
            arrow(c, (bx, by), dir, br * 0.55, 0xf4f1ea, 220);
        } else {
            face_button(c, text, (bx, by, br), label, psp.then_some(bit), pressed & (1 << bit) != 0);
        }
        let hit = (br * 1.25) as i32;
        controls.push((Area::new(bx as i32 - hit, by as i32 - hit, 2 * hit, 2 * hit), bit));
    }
    // Shoulders: pills at the top corners (landscape) or over the clusters (portrait).
    let shoulders: &[(&str, u16, bool)] = match system {
        "gba" | "snes" | "psx" | "nds" | "psp" | "dreamcast" => &[("L", 10, false), ("R", 11, true)],
        "n64" => &[("L", 10, false), ("Z", 12, false), ("R", 11, true)],
        _ => &[],
    };
    let (sw, sh) = (d(96.0), d(52.0));
    let mut left_x = edge;
    for (label, bit, right) in shoulders {
        let x = if *right { w - edge - sw } else { left_x };
        if !right {
            left_x += sw + d(10.0);
        }
        let area = Area::new(x, shoulder_y, sw, sh);
        let on = pressed & (1 << bit) != 0;
        soft_pill(c, area, PAD_PAPER, if on { 120 } else { 52 });
        let size = (d(18.0) as u32).clamp(16, 40);
        let tw = text.width(Face::Title, size, label);
        text.draw(c, Face::Title, size, area.x + (area.w - tw) / 2, area.y + area.h / 2 + type_scale::mid(size), 0xf4f1ea, label);
        controls.push((area, *bit));
    }
    // Select, Pause and Start: small pills at the bottom, in the middle in portrait, under the clusters in landscape.
    let (uw, uh) = (d(76.0), d(44.0));
    let utility: Vec<(&str, Option<u16>, i32)> = if portrait {
        vec![("SELECT", Some(2), w / 2 - uw * 3 / 2 - d(10.0)), ("Pause", None, w / 2 - uw / 2), ("START", Some(3), w / 2 + uw / 2 + d(10.0))]
    } else {
        vec![("SELECT", Some(2), left_c.0 as i32 - uw / 2), ("Pause", None, w / 2 - uw / 2), ("START", Some(3), right_c.0 as i32 - uw / 2)]
    };
    let mut menu = Area::new(0, 0, 0, 0);
    for (label, bit, x) in utility {
        if system == "n64" && bit == Some(2) {
            continue;
        }
        // Pause in landscape sits under the left shoulder, beside the picture rather than over it.
        let area = if bit.is_none() && !portrait {
            Area::new(left_c.0 as i32 - uw / 2, shoulder_y + d(52.0) + d(10.0), uw, uh)
        } else {
            Area::new(x, util_y, uw, uh)
        };
        let on = bit.is_some_and(|b| pressed & (1 << b) != 0);
        soft_pill(c, area, PAD_PAPER, if on { 120 } else { if bit.is_none() { 64 } else { 44 } });
        let size = (d(11.0) as u32).clamp(14, 28);
        let tw = text.width(Face::Body, size, label);
        text.draw(c, Face::Body, size, area.x + (area.w - tw) / 2, area.y + area.h / 2 + type_scale::mid(size), 0xf4f1ea, label);
        match bit {
            Some(b) => controls.push((area, b)),
            None => menu = area,
        }
    }
    (controls, dpad, menu)
}

/// The options page's sections, in page order: the player's own picture settings first, then the core's.
const SECTIONS: [&str; 5] = ["Picture", "Sound", "Controls", "Speed", "Advanced (this emulator)"];
/// The section that starts closed: what is particular to one emulator (its model, BIOS, presets, restarts).
const ADVANCED: usize = 4;
/// Which section of `SECTIONS` option `row` of `display_options` goes in, by its key and name.
fn option_section(option: &FileOption, row: usize) -> usize {
    if row < picture::ROWS {
        return 0;
    }
    let s = format!("{} {}", option.key, option.label).to_lowercase();
    let has = |words: &[&str]| words.iter().any(|w| s.contains(w));
    // The emulator's own hardware choices and anything that needs a restart: for those who know that core.
    if option_name(&option.label).1
        || has(&["bios", "model", "hardware", "preset", "sgb", "super game boy", "region", "boot", "firmware", "rtc", "system type", "revision", "memory", "ram ", "language", "date", "clock", "debug", "log"])
    {
        return ADVANCED;
    }
    if has(&["audio", "sound", "volume", "apu", "channel", "sample", "music", "sfx", "psg", "reverb", "stereo", "mono"]) {
        1
    } else if has(&["input", "controller", "pad", "turbo", "analog", "rumble", "mouse", "touch", "stylus", "button", "joystick", "lightgun", "gun", "keyboard", "deadzone"]) {
        2
    } else if has(&["frameskip", "frame skip", "overclock", "cpu", "speed", "cycle", "runahead", "sync", "lag", "timing", "accuracy", "underclock", "threads"]) {
        3
    } else if has(&["palette", "color", "colour", "filter", "blend", "video", "scale", "border", "aspect", "overscan", "crop", "resolution", "render", "screen", "display", "layout", "gamma", "lcd", "ghost", "interlac", "scanline", "texture", "upscal", "brightness", "contrast", "sprite"]) {
        0
    } else {
        4
    }
}
/// The options' rows in page order: by section, each section in the core's own order.
fn option_order(options: &[FileOption]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..options.len()).collect();
    order.sort_by_key(|&i| option_section(&options[i], i));
    order
}
/// What a controller steps through on the options page: the clear sections, then the "Advanced (this emulator)"
/// row (`options.len() + 1`), then its options when it is open. Back (`options.len()`) comes after.
fn option_page(options: &[FileOption], open: bool) -> Vec<usize> {
    let order = option_order(options);
    let (clear, advanced): (Vec<usize>, Vec<usize>) = order.into_iter().partition(|&i| option_section(&options[i], i) != ADVANCED);
    let mut page = clear;
    if !advanced.is_empty() {
        page.push(options.len() + 1);
        if open {
            page.extend(advanced);
        }
    }
    page
}
/// A core option's name without the handheld's "(Restart)" note, and whether it only works after a restart.
fn option_name(label: &str) -> (String, bool) {
    for mark in [" (Restart)", "(Restart)", " (restart)", " (Requires Restart)", " (requires restart)"] {
        if let Some(name) = label.strip_suffix(mark).or_else(|| label.find(mark).map(|at| &label[..at])) {
            return (name.trim().to_string(), true);
        }
    }
    (label.to_string(), false)
}
/// A value as a player reads it: On/Off instead of enabled/disabled, a capital first letter.
fn option_value(value: &str) -> String {
    match value.to_lowercase().as_str() {
        "enabled" | "on" | "true" => "On".into(),
        "disabled" | "off" | "false" => "Off".into(),
        _ => {
            let mut chars = value.chars();
            chars.next().map(|c| c.to_uppercase().collect::<String>() + chars.as_str()).unwrap_or_default()
        }
    }
}
/// The controller's focus: a ring round `area` in the text colour.
fn ring(c: &mut Canvas, area: Area) {
    let grow = 7;
    c.round_rect(area.x - grow, area.y - grow, area.w + 2 * grow, area.h + 2 * grow, (area.h + 2 * grow) / 2, pal().text);
    c.round_rect(area.x - 2, area.y - 2, area.w + 4, area.h + 4, (area.h + 4) / 2, pal().void);
}
fn touch_pause(c: &mut Canvas, text: &mut Text, v: &TouchView) -> TouchOut {
    let (w, h) = v.size;
    let portrait = w < h;
    let mut out = TouchOut::default();
    let inner = w - 2 * EDGE;
    let title = if v.menu == Menu::Options { "Options" } else { v.title };
    let title = text.fit(Face::Title, 46, title, if v.menu == Menu::Options && !portrait { inner - 420 } else { inner });
    text.draw(c, Face::Title, 46, EDGE, 92, pal().chalk, &title);
    let top = 170;
    // A controller's keys go in a line at the bottom; a finger needs none.
    let hint_h = if v.focus.is_some() { 56 } else { 0 };
    let bottom = h - 36 - hint_h;
    if v.focus.is_some() {
        let hint = if v.menu == Menu::Options { "A Choose · ‹ › Change · B Back" } else { "A Choose · B Continue · L / R Slot" };
        let hint = text.fit(Face::Body, 28, hint, inner);
        text.draw(c, Face::Body, 28, EDGE, h - 40, pal().dust, &hint);
    }
    if v.menu == Menu::Options {
        let tall = ((bottom - top - GAP - 100) / 6).clamp(TARGET_MIN, 124);
        // Back: the bottom of a portrait page, the top right of a landscape one (rows keep the full height).
        let back = if portrait {
            Area::new(EDGE, bottom - tall, inner, tall)
        } else {
            Area::new(w - EDGE - 400, 28, 400, tall.min(112))
        };
        let cols = if portrait { 1 } else { 2 };
        let (win_top, win_bottom) = (if portrait { top } else { back.y + back.h + GAP }, if portrait { back.y - GAP } else { bottom });
        let col_w = (inner - (cols - 1) * GAP) / cols;
        let (zone, row_h, head_h) = (116, 128, 76);
        // The page: a heading per section ("Picture", "Sound", "Controls", "Speed", "Game"), its rows under it.
        let order = option_order(v.options);
        let mut places: Vec<(usize, i32, i32)> = Vec::new();
        let mut heads: Vec<(&str, i32)> = Vec::new();
        let mut toggle: Option<(i32, usize)> = None;
        let mut y = 0;
        for (s, name) in SECTIONS.iter().enumerate() {
            let rows: Vec<usize> = order.iter().copied().filter(|&i| option_section(&v.options[i], i) == s).collect();
            if rows.is_empty() {
                continue;
            }
            if s == ADVANCED {
                // A row of its own that opens the group: "Advanced (this emulator) · 6".
                toggle = Some((y + 8, rows.len()));
                places.push((v.options.len() + 1, 0, y + 8));
                y += 8 + 104 + GAP;
                if !v.advanced_open {
                    continue;
                }
            } else {
                heads.push((name, y));
                y += head_h;
            }
            for (n, i) in rows.iter().enumerate() {
                places.push((*i, n as i32 % cols, y + n as i32 / cols * (row_h + GAP)));
            }
            y += (rows.len() as i32 + cols - 1) / cols * (row_h + GAP) + 8;
        }
        let content = y;
        out.scroll_max = (content - (win_bottom - win_top)).max(0);
        let mut scroll = v.scroll;
        if let Some((_, _, fy)) = v.focus.and_then(|f| places.iter().find(|(i, _, _)| *i == f)) {
            // The controller's row stays in view (its heading too when it is the first of a section).
            if fy - head_h < scroll {
                scroll = fy - head_h;
            } else if fy + row_h > scroll + (win_bottom - win_top) {
                scroll = fy + row_h - (win_bottom - win_top);
            }
        }
        out.scroll = scroll.clamp(0, out.scroll_max);
        let visible = |a: Area| {
            let (y0, y1) = (a.y.max(win_top), (a.y + a.h).min(win_bottom));
            (y1 > y0).then(|| Area::new(a.x, y0, a.w, y1 - y0))
        };
        for (name, hy) in &heads {
            let base = win_top + hy - out.scroll + 52;
            if base > win_top + 30 && base < win_bottom {
                text.draw(c, Face::Title, 40, EDGE + 8, base, pal().chalk, name);
            }
        }
        if let Some((ty, count)) = toggle {
            let area = Area::new(EDGE, win_top + ty - out.scroll, inner, 104);
            if area.y + area.h >= win_top && area.y <= win_bottom {
                if v.focus == Some(v.options.len() + 1) {
                    ring(c, area);
                }
                c.round_rect(area.x, area.y, area.w, area.h, area.h / 2, pal().spine);
                let name = text.translated("Advanced (this emulator)").to_string();
                let label = format!("{name} · {count}");
                text.draw(c, Face::Title, 38, area.x + 48, area.y + area.h / 2 + type_scale::mid(38), pal().chalk, &label);
                // A drawn chevron: down while closed, up while open.
                let (cx, cy) = (area.x + area.w - 70, area.y + area.h / 2);
                for i in 0..=16 {
                    let dy = if v.advanced_open { 8 - i / 2 } else { -8 + i / 2 };
                    c.rect(cx - 16 + i, cy + dy - 3, 3, 6, pal().text);
                    c.rect(cx + 16 - i - 2, cy + dy - 3, 3, 6, pal().text);
                }
                if let Some(a) = visible(area) {
                    out.hits.push((a, v.options.len() + 1, text.translated("Advanced (this emulator)").to_string()));
                }
            }
        }
        for &(i, col, py) in &places {
            if i >= v.options.len() {
                continue;
            }
            let option = &v.options[i];
            let area = Area::new(EDGE + col * (col_w + GAP), win_top + py - out.scroll, col_w, row_h);
            if area.y + area.h < win_top || area.y > win_bottom {
                continue;
            }
            if v.focus == Some(i) {
                ring(c, area);
            }
            c.round_rect(area.x, area.y, area.w, area.h, row_h / 2, pal().pill);
            let many = option.values.len() > 1;
            let arrow = if many { pal().text } else { pal().off };
            chevron(c, area.x + zone / 2 + 6, area.y + row_h / 2, false, arrow);
            chevron(c, area.x + area.w - zone / 2 - 6, area.y + row_h / 2, true, arrow);
            let middle = Area::new(area.x + zone, area.y, area.w - 2 * zone, row_h);
            let (name, restart) = option_name(&option.label);
            let name = if restart {
                let after = text.translated("after a restart").to_string();
                format!("{name} · {after}")
            } else {
                name
            };
            let label = text.fit(Face::Body, 26, &name, middle.w);
            let width = text.width(Face::Body, 26, &label);
            text.draw(c, Face::Body, 26, middle.x + (middle.w - width) / 2, area.y + 46, pal().soft, &label);
            let value = text.fit(Face::Body, 36, &option_value(option.label_of(&option.current)), middle.w);
            let width = text.width(Face::Body, 36, &value);
            text.draw(c, Face::Body, 36, middle.x + (middle.w - width) / 2, area.y + 96, pal().text, &value);
            // Taps only count inside the visible window, not under the header or the Back button.
            if many {
                if let Some(a) = visible(Area::new(area.x, area.y, zone, row_h)) {
                    out.option_arrows.push((a, i, -1, format!("{} · previous value", option.label)));
                }
                if let Some(a) = visible(Area::new(area.x + area.w - zone, area.y, zone, row_h)) {
                    out.option_arrows.push((a, i, 1, format!("{} · next value", option.label)));
                }
            }
            if let Some(a) = visible(middle) {
                out.hits.push((a, i, v.rows[i].clone()));
            }
        }
        // Rows slide under the header and the Back button: cover them with the page.
        c.rect(0, 0, w, win_top, pal().void);
        c.rect(0, win_bottom, w, (if portrait { back.y } else { bottom }) - win_bottom, pal().void);
        if portrait {
            c.rect(0, back.y - GAP, w, back.h + GAP, pal().void);
        }
        text.draw(c, Face::Title, 46, EDGE, 92, pal().chalk, &title);
        if v.focus == Some(v.options.len()) {
            ring(c, back);
        }
        pill(c, text, back, "Back", false);
        out.hits.push((back, v.options.len(), text.translated("Back").to_string()));
        let window = win_bottom - win_top;
        if content > window {
            let bar = (window * window / content).max(40);
            let at = win_top + (window - bar) * out.scroll / out.scroll_max.max(1);
            c.round_rect(w - 14, at, 6, bar, 3, pal().chip);
        }
        return out;
    }
    let message = text.fit(Face::Body, 28, v.message, inner);
    text.draw(c, Face::Body, 28, EDGE, 140, pal().dust, &message);
    // With the whole memory card the slot picker is taller, to show the medium.
    let (picker_extra, primary_extra) = (if v.card { 38 + 110 } else { 38 }, 16);
    let labels = ["Continue", "Save", "Load", "Earlier", "Options", "Screenshot", "Quit"];
    let picker = |c: &mut Canvas, text: &mut Text, out: &mut TouchOut, area: Area| {
        c.round_rect(area.x, area.y, area.w, area.h, 28, pal().pill);
        let zone = 120.min(area.w / 4);
        let (left, right) = (Area::new(area.x, area.y, zone, area.h), Area::new(area.x + area.w - zone, area.y, zone, area.h));
        chevron(c, left.x + zone / 2 + 6, area.y + area.h / 2, false, pal().text);
        chevron(c, right.x + zone / 2 - 6, area.y + area.h / 2, true, pal().text);
        out.slot_arrows = [Some(left), Some(right)];
        let middle = Area::new(area.x + zone, area.y, area.w - 2 * zone, area.h);
        let name = format!("Slot {}", v.slot + 1);
        centered(c, text, Face::Body, 36, Area::new(middle.x, area.y + 6, middle.w, 62), pal().text, &name);
        if v.card {
            // The medium with its eight slots (as the info page shows it), in the room under the name.
            let room = Area::new(middle.x, area.y + 70, middle.w, area.h - 70 - 14);
            let l = pocket_ui::save_media::Layout::fitted(v.kind, v.filled.len(), room.w, room.h);
            let slots = pocket_ui::save_media::Slots {
                filled: v.filled,
                ring: Some(v.slot),
                icon: v.icon,
                stripe: pocket_ui::document_library::system(v.system).map_or(pal().chip, |s| s.3),
                numbers: true,
            };
            pocket_ui::save_media::paint(c, text, v.kind, room.x + (room.w - l.w) / 2, room.y + (room.h - l.h) / 2, &l, &slots);
        } else {
            let b = ((middle.w - 7 * 14) / 8).clamp(20, 44);
            pocket_ui::save_media::paint_row(c, v.kind, middle.x + middle.w / 2, area.y + area.h - 46, b, 14, v.filled, Some(v.slot), v.icon);
        }
    };
    let button = |c: &mut Canvas, text: &mut Text, out: &mut TouchOut, area: Area, row: usize| {
        if v.focus == Some(row) {
            ring(c, area);
        }
        pill(c, text, area, labels[row], row == 0);
        // The accessibility label follows the interface language, like the painted one.
        out.hits.push((area, row, text.translated(&v.rows[row]).to_string()));
    };
    if portrait {
        let avail = bottom - top;
        let bh = ((avail - 6 * GAP - picker_extra - primary_extra) / 7).clamp(TARGET_MIN, 124);
        let total = 7 * bh + 6 * GAP + picker_extra + primary_extra;
        let mut y = (bottom - total).max(top);
        if let Some((image, aspect)) = v.frame {
            let (room_top, room_bottom) = (top + 20, y - 2 * GAP);
            let ih = ((room_bottom - room_top) as f64).min(inner as f64 / aspect);
            if ih >= 200.0 {
                let iw = (ih * aspect) as i32;
                let ih = ih as i32;
                c.blit_scaled(image, (w - iw) / 2, room_top + (room_bottom - room_top - ih) / 2, iw, ih);
            }
        }
        button(c, text, &mut out, Area::new(EDGE, y, inner, bh + primary_extra), 0);
        y += bh + primary_extra + GAP;
        picker(c, text, &mut out, Area::new(EDGE, y, inner, bh + picker_extra));
        y += bh + picker_extra + GAP;
        let half = (inner - GAP) / 2;
        button(c, text, &mut out, Area::new(EDGE, y, half, bh), 1);
        button(c, text, &mut out, Area::new(EDGE + half + GAP, y, half, bh), 2);
        y += bh + GAP;
        for row in 3..7 {
            button(c, text, &mut out, Area::new(EDGE, y, inner, bh), row);
            y += bh + GAP;
        }
    } else {
        // Two columns that always fit the height: Continue, the slot and Save | Load at the left; Earlier, Options,
        // Screenshot and Quit at the right. The buttons shrink to fit, never below 48 dp, before anything scrolls.
        let avail = bottom - top;
        let col_w = (inner - 2 * EDGE) / 2;
        let bh = ((avail - 3 * GAP) / 4).clamp(TARGET_MIN, 124);
        let right_h = 4 * bh + 3 * GAP;
        let y0 = top + ((avail - right_h) / 2).max(0);
        let x2 = w - EDGE - col_w;
        for (n, row) in (3..7).enumerate() {
            button(c, text, &mut out, Area::new(x2, y0 + n as i32 * (bh + GAP), col_w, bh), row);
        }
        // Left column: the same height as the right one, the spare room shared between its gaps.
        let (cont, pick) = (bh + primary_extra.min((right_h - 3 * bh - 2 * GAP) / 2).max(0), bh + picker_extra.min((right_h - 3 * bh - 2 * GAP) / 2).max(0));
        // The whole memory card: the picker takes two rows' height, Continue and Save | Load keep a plain one.
        let (cont, pick) = if v.card { (bh, right_h - 2 * bh - 2 * GAP) } else { (cont, pick) };
        let gap = ((right_h - cont - pick - bh) / 2).max(GAP / 2);
        button(c, text, &mut out, Area::new(EDGE, y0, col_w, cont), 0);
        let y = y0 + cont + gap;
        picker(c, text, &mut out, Area::new(EDGE, y, col_w, pick));
        let y = y + pick + gap;
        let half = (col_w - GAP) / 2;
        button(c, text, &mut out, Area::new(EDGE, y, half, bh), 1);
        button(c, text, &mut out, Area::new(EDGE + half + GAP, y, half, bh), 2);
    }
    out
}
fn key_bit(key: i32) -> Option<u16> {
    Some(match key {
        // Android A/B/X/Y are south/east/west/north; libretro uses B/A/Y/X.
        96 => 0,
        97 => 8,
        99 => 1,
        100 => 9,
        19 => 4,
        20 => 5,
        21 => 6,
        22 => 7,
        108 => 3,
        109 => 2,
        102 => 10,
        103 => 11,
        104 => 12,
        105 => 13,
        _ => return None,
    })
}
fn png(path: &Path, image: &Image) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, image.w, image.h);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
        let rgb: Vec<u8> = image.px.iter().flat_map(|p| [(p >> 16) as u8, (p >> 8) as u8, *p as u8]).collect();
        writer.write_image_data(&rgb).map_err(|e| e.to_string())?;
    }
    pocket_ui::fsutil::write_atomic(path, &bytes).map_err(|e| e.to_string())
}
fn string(env: &mut JNIEnv, value: JString) -> String {
    env.get_string(&value).map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
}
#[no_mangle]
pub extern "system" fn Java_dev_pocket_spike_PlayerActivity_nativeCreate(
    mut env: JNIEnv,
    activity: JObject,
    core: JString,
    rom: JString,
    system: JString,
    key: JString,
    title: JString,
) -> jstring {
    android_logger::init_once(android_logger::Config::default().with_tag("PocketMvp").with_max_level(log::LevelFilter::Info));
    let initialized = env
        .get_java_vm()
        .map_err(|e| e.to_string())
        .and_then(|vm| unsafe { pocket_ui::platform::android::initialize(vm.get_java_vm_pointer(), activity.as_raw()) });
    if let Err(error) = initialized {
        return env.new_string(error).map(|s| s.into_raw()).unwrap_or(std::ptr::null_mut());
    }
    // The launch carries the current preferences, so a fast launch cannot race the async disk writer.
    let prefs: serde_json::Value = serde_json::from_str(&pocket_ui::platform::android::string("playerSettings"))
        .unwrap_or_else(|_| pocket_ui::fsutil::read_json_or_backup(&pocket_ui::platform::data_dir().join("launcher.json")).unwrap_or_default());
    let language = prefs["german"].as_bool().map_or_else(
        || pocket_ui::i18n::Lang::from_locale(&pocket_ui::platform::android::string("language")),
        |g| if g { pocket_ui::i18n::Lang::De } else { pocket_ui::i18n::Lang::En },
    );
    pocket_ui::i18n::set(language);
    let font_scale = pocket_ui::platform::android::string("fontScale").parse::<f32>().unwrap_or(1.0);
    let text_size = prefs["text_size"].as_str().unwrap_or("normal");
    pocket_ui::type_scale::set(if font_scale >= 1.25 || text_size == "large" {
        pocket_ui::text_size::TextSize::Large
    } else if font_scale >= 1.12 || text_size == "medium" {
        pocket_ui::text_size::TextSize::Medium
    } else {
        pocket_ui::text_size::TextSize::Normal
    });
    // The launcher's look settings (colours, font, hint symbols, motion, Cozy extras) apply here too.
    macro_rules! pref {
        ($ty:ty, $key:expr) => {
            serde_json::from_value::<$ty>(prefs[$key].clone()).unwrap_or_default()
        };
    }
    let extras = pref!(pocket_ui::cozy_extras::Extras, "extras");
    pocket_ui::palette::set_colours(pref!(pocket_ui::palette::Colours, "colours"));
    pocket_ui::font_style::set(pref!(pocket_ui::font_style::FontStyle, "font"));
    pocket_ui::button_prompts::set(pref!(pocket_ui::button_prompts::PromptStyle, "prompts"));
    pocket_ui::motion::set_level(pref!(pocket_ui::motion::Motion, "motion"));
    pocket_ui::motion::set_cozy(prefs["cozy"].as_bool().unwrap_or(true));
    pocket_ui::cozy_extras::set(extras);
    let core = string(&mut env, core);
    let rom = string(&mut env, rom);
    let system = string(&mut env, system);
    let key = string(&mut env, key);
    let title = string(&mut env, title);
    let error = PLAYER.with(|slot| {
        slot.borrow_mut().take();
        match Player::new(&core, &rom, system, key, title, prefs["light"].as_bool().unwrap_or(false)) {
            Ok(mut player) => {
                player.sounds_muted = prefs["muted"].as_bool().unwrap_or(false);
                player.crt_done = !prefs["cozy"].as_bool().unwrap_or(true) || !extras.tv_switch_on;
                player.core.frontend_current();
                *slot.borrow_mut() = Some(player);
                String::new()
            }
            Err(e) => e,
        }
    });
    env.new_string(error).map(|s| s.into_raw()).unwrap_or(std::ptr::null_mut())
}
#[no_mangle]
pub extern "system" fn Java_dev_pocket_spike_PlayerActivity_nativeFrame(
    mut env: JNIEnv,
    activity: JObject,
    w: jint,
    h: jint,
    density: jfloat,
    left: jint,
    top: jint,
    right: jint,
    bottom: jint,
) -> jint {
    PLAYER.with(|slot| {
        let mut slot = slot.borrow_mut();
        let result = slot.as_mut().map_or(2, |p| p.frame(w, h, density, [left, top, right, bottom]));
        if let Some(p) = slot.as_mut() {
            let nodes = p.semantic_nodes();
            if nodes != p.semantics {
                p.semantics = nodes;
                if let Ok(text) = env.new_string(&p.semantics) {
                    let object = JObject::from(text);
                    let _ = env.call_method(&activity, "publishSemantics", "(Ljava/lang/String;)V", &[jni::objects::JValue::Object(&object)]);
                }
            }
        }

        if result == 2 {
            slot.take();
        }
        result
    })
}
#[no_mangle]
pub extern "system" fn Java_dev_pocket_spike_PlayerActivity_nativeKey(_: JNIEnv, _: JObject, key: jint, down: jboolean, controller: jboolean) -> jboolean {
    PLAYER.with(|slot| {
        slot.borrow_mut().as_mut().is_some_and(|p| {
            if controller != 0 && down != 0 {
                p.input_mode(false);
            }
            let result = p.key(key, down != 0);
            p.core.frontend_current();
            result
        })
    }) as jboolean
}
#[no_mangle]
pub extern "system" fn Java_dev_pocket_spike_PlayerActivity_nativeAnalog(
    _: JNIEnv,
    _: JObject,
    x: jfloat,
    y: jfloat,
    rx: jfloat,
    ry: jfloat,
    hx: jfloat,
    hy: jfloat,
) -> jboolean {
    PLAYER.with(|slot| {
        slot.borrow_mut().as_mut().is_some_and(|p| {
            if [x, y, rx, ry, hx, hy].iter().any(|v| v.abs() > 0.01) {
                p.input_mode(false);
            }
            let result = p.analog([x, y, rx, ry], [hx, hy]);
            p.core.frontend_current();
            result
        })
    }) as jboolean
}
#[no_mangle]
pub extern "system" fn Java_dev_pocket_spike_PlayerActivity_nativeTouch(_: JNIEnv, _: JObject, id: jint, x: jfloat, y: jfloat, phase: jint) -> jboolean {
    PLAYER.with(|slot| {
        slot.borrow_mut().as_mut().is_some_and(|p| {
            let result = p.touch(id, x, y, phase);
            p.core.frontend_current();
            result
        })
    }) as jboolean
}
#[no_mangle]
pub extern "system" fn Java_dev_pocket_spike_PlayerActivity_nativeSuspend(_: JNIEnv, _: JObject) {
    PLAYER.with(|slot| {
        if let Some(p) = slot.borrow_mut().as_mut() {
            p.pause(true);
            p.audio.take();
            if let Err(e) = p.automatic() {
                log::error!("suspend save: {e}");
                p.message = e;
            }
            p.core.frontend_current();
        }
    });
}
