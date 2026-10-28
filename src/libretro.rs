//! Minimal libretro frontend side: loads a core (.so), feeds it a game and collects what it produces.
//! Software-rendered cores (Game Boy, NES, SNES, …); OpenGL ES rendering for 3D cores is in `hw_render.rs`.
//!
//! libretro callbacks carry no user pointer, so their state lives in a global (`SHARED`). The core
//! runs on the thread that calls `run()`; threaded cores call back from their own threads too.

use std::sync::{Mutex, OnceLock};
use std::ffi::{c_char, c_uint, c_void, CStr, CString};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use libloading::{Library, Symbol};

use crate::controls::Described;
use crate::core_options::{self, OptionSet};

// Environment commands we answer (libretro.h).
const ENV_GET_CAN_DUPE: c_uint = 3;
const ENV_SET_ROTATION: c_uint = 1;
const ENV_SET_PERFORMANCE_LEVEL: c_uint = 8;
const ENV_GET_SYSTEM_DIRECTORY: c_uint = 9;
const ENV_SET_PIXEL_FORMAT: c_uint = 10;
const ENV_SET_INPUT_DESCRIPTORS: c_uint = 11;
const ENV_SET_MESSAGE: c_uint = 6;
const ENV_GET_VARIABLE: c_uint = 15;
const ENV_SET_VARIABLES: c_uint = 16;
const ENV_GET_VARIABLE_UPDATE: c_uint = 17;
const ENV_SET_SUPPORT_NO_GAME: c_uint = 18;
const ENV_SET_FRAME_TIME_CALLBACK: c_uint = 21;
const ENV_GET_LOG_INTERFACE: c_uint = 27;
const ENV_GET_SAVE_DIRECTORY: c_uint = 31;
const ENV_SET_SYSTEM_AV_INFO: c_uint = 32;
const ENV_SET_CONTROLLER_INFO: c_uint = 35;
const ENV_SET_GEOMETRY: c_uint = 37;
const ENV_GET_LANGUAGE: c_uint = 39;
const ENV_GET_INPUT_BITMASKS: c_uint = 51 | 0x10000;
const ENV_GET_CORE_OPTIONS_VERSION: c_uint = 52;
const ENV_SET_CORE_OPTIONS: c_uint = 53;
const ENV_SET_CORE_OPTIONS_INTL: c_uint = 54;
const ENV_SET_CORE_OPTIONS_DISPLAY: c_uint = 55;
const ENV_SET_CORE_OPTIONS_V2: c_uint = 67;
const ENV_SET_CORE_OPTIONS_V2_INTL: c_uint = 68;
const ENV_SET_CORE_OPTIONS_UPDATE_DISPLAY_CALLBACK: c_uint = 69;
const ENV_SET_VARIABLE: c_uint = 70;
// RetroAchievements (`achievement_environment` below): where the core's memory is, and whether it supports them.
const ENV_SET_MEMORY_MAPS: c_uint = 36 | 0x10000;
const ENV_SET_SUPPORT_ACHIEVEMENTS: c_uint = 42 | 0x10000;

pub const PIXEL_0RGB1555: c_uint = 0;
pub const PIXEL_XRGB8888: c_uint = 1;
pub const PIXEL_RGB565: c_uint = 2;

const ENV_SET_DISK_CONTROL_INTERFACE: c_uint = 13;
const ENV_GET_INPUT_DEVICE_CAPABILITIES: c_uint = 24;
const ENV_GET_CONTENT_DIRECTORY: c_uint = 30;
const ENV_GET_VFS_INTERFACE: c_uint = 45 | 0x10000;
const ENV_SET_DISK_CONTROL_EXT_INTERFACE: c_uint = 58;
const ENV_SET_SERIALIZATION_QUIRKS: c_uint = 44;
/// RETRO_SERIALIZATION_QUIRK_* (libretro.h). The state is incomplete: fine for saving, not for run-ahead.
pub const QUIRK_INCOMPLETE: u64 = 1 << 0;
/// The core can save states only after it ran for a while; until then `retro_serialize` fails.
pub const QUIRK_MUST_INITIALIZE: u64 = 1 << 1;
/// The core's states vary in size. (Was `1 << 5` until 2026-10-23, which is ENDIAN_DEPENDENT.)
const QUIRK_CORE_VARIABLE_SIZE: u64 = 1 << 2;
/// RETRO_ENVIRONMENT_GET_AUDIO_VIDEO_ENABLE: which of picture and sound the core has to make this frame
/// (`Core::run_with`). Run-ahead and fast-forward throw most frames away; a core that asks can skip them.
const ENV_GET_AUDIO_VIDEO_ENABLE: c_uint = 47 | 0x10000;
/// retro_av_enable_flags: make the picture, make the sound, and (while run-ahead saves and loads) "this state
/// stays in memory and is loaded by the same core", which lets a core take shortcuts.
pub const AV_VIDEO: u32 = 1 << 0;
pub const AV_AUDIO: u32 = 1 << 1;
pub const AV_ALL: u32 = AV_VIDEO | AV_AUDIO;
const AV_FAST_SAVESTATES: c_uint = 1 << 2;

const DEVICE_JOYPAD: c_uint = 1;
const DEVICE_MOUSE: c_uint = 2;
const DEVICE_LIGHTGUN: c_uint = 4;
const DEVICE_ANALOG: c_uint = 5;
const DEVICE_POINTER: c_uint = 6;
const JOYPAD_MASK: c_uint = 256;
/// Ports after player 1's that get a pad (`players::MAX` players in all).
const OTHER_PORTS: usize = 3;
const MEMORY_SAVE_RAM: c_uint = 0;
/// The cartridge clock, for cores that keep it apart from the battery save (Gambatte: MBC3 and HuC3 cartridges such as
/// Pokémon Gold, Silver and Crystal; Snes9x: S-RTC and SPC7110; mGBA for Game Boy MBC3 only, a GBA game's clock is
/// the system time there). Kept as RetroArch does, `<game>.rtc` beside the battery save (`Core::rtc_path`).
const MEMORY_RTC: c_uint = 1;

#[repr(C)]
struct SystemInfo {
    library_name: *const c_char,
    library_version: *const c_char,
    valid_extensions: *const c_char,
    need_fullpath: bool,
    block_extract: bool,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct Geometry {
    pub base_width: c_uint,
    pub base_height: c_uint,
    pub max_width: c_uint,
    pub max_height: c_uint,
    pub aspect_ratio: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct Timing {
    pub fps: f64,
    pub sample_rate: f64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct AvInfo {
    pub geometry: Geometry,
    pub timing: Timing,
}

/// `retro_variable`: a key and a value; for `SET_VARIABLES` the value is "Label; first|second|third".
#[repr(C)]
struct Variable {
    key: *const c_char,
    value: *const c_char,
}

/// `retro_input_descriptor`: what a core calls one of its inputs.
#[repr(C)]
struct InputDescriptor {
    port: c_uint,
    device: c_uint,
    index: c_uint,
    id: c_uint,
    description: *const c_char,
}

/// `retro_frame_time_callback`: called before each frame with the time it stands for (TIC-80 won't load without it).
#[repr(C)]
#[derive(Clone, Copy)]
struct FrameTime {
    callback: Option<unsafe extern "C" fn(i64)>,
    /// The core's own frame time in microseconds.
    reference: i64,
}

#[repr(C)]
struct Message {
    msg: *const c_char,
    frames: c_uint,
}

#[repr(C)]
struct GameInfo {
    path: *const c_char,
    data: *const c_void,
    size: usize,
    meta: *const c_char,
}

/// `retro_log_callback`: the core calls `log` printf-style with any arguments.
#[repr(C)]
struct LogCallback {
    log: unsafe extern "C" fn(level: c_uint, fmt: *const c_char, ...),
}

extern "C" {
    /// The log callback for cores, in C (src/log_shim.c, built by build.rs): Rust can't define C-variadic
    /// functions on stable. It formats with the core's real arguments and passes the text to the sink.
    fn pocket_log_printf(level: c_uint, fmt: *const c_char, ...);
    /// Where its lines go, and the lowest level it formats.
    fn pocket_log_setup(sink: unsafe extern "C" fn(level: c_uint, text: *const c_char), min_level: c_uint);
}

/// One video frame as the core delivered it, copied out of the core's buffer.
#[derive(Clone)]
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub pitch: usize,
    pub format: c_uint,
    pub data: Vec<u8>,
}

/// A finger on the touch screen. `x` and `y` run -32767..32767 across the picture; `dx` and `dy` are the
/// movement since the last frame in the picture's own pixels (what a mouse reports).
#[derive(Default, Clone, Copy)]
struct Pointer {
    pressed: bool,
    x: i16,
    y: i16,
    dx: i16,
    dy: i16,
}

#[derive(Default)]
struct Shared {
    pixel_format: c_uint,
    frame: Option<Frame>,
    /// Interleaved stereo samples produced during the current `run()`.
    audio: Vec<i16>,
    /// RETRO_DEVICE_ID_JOYPAD_* bits of player 1.
    buttons: u16,
    /// Players 2 to 4 (ports 1..3): their joypad bits and sticks (`Core::set_player`; couch co-op, `players`).
    others: [(u16, [i16; 4]); OTHER_PORTS],
    system_dir: CString,
    save_dir: CString,
    av_changed: Option<AvInfo>,
    geometry_changed: Option<Geometry>,
    /// The core's options and their values; answers GET_VARIABLE (see `core_options`).
    opts: OptionSet,
    /// Called after a value changed, so the core can show or hide dependent options.
    update_display: Option<unsafe extern "C" fn()>,
    /// What the core calls the buttons of player 1, and whether that changed since it was last read.
    described: Vec<Described>,
    inputs_dirty: bool,
    /// Analog sticks (left x/y, right x/y) as libretro reads them.
    analog: [i16; 4],
    /// The touch screen, as the pointer (and mouse, and light gun) cores read it.
    pointer: Pointer,
    /// Log every environment command we don't answer (POCKET_CORE_TRACE=1), to see what a new core wants.
    trace: bool,
    traced: Vec<c_uint>,
    /// SET_ROTATION: 0..3, quarter turns counter-clockwise (vertical arcade games).
    rotation: c_uint,
    content_dir: CString,
    /// The last warning or error line the core logged, to explain a failed load.
    last_error: String,
    disk: Option<DiskControl>,
    /// (description, device id) of the controller types player 1 can have, as the core listed them.
    pad_types: Vec<(String, c_uint)>,
    /// The same for every port the core listed (SET_CONTROLLER_INFO), player 1's first.
    port_types: Vec<Vec<(String, c_uint)>>,
    /// What the core said about its states (SET_SERIALIZATION_QUIRKS); `Core::load_state` checks sizes by it.
    serialization_quirks: u64,
    /// SET_FRAME_TIME_CALLBACK; `Core::run` calls it before every frame.
    frame_time: Option<FrameTime>,
    /// AV_* bits the current frame does without (`Core::run_with`): 0, the default, means picture and sound.
    av_skip: u32,
    /// A run-ahead state is being saved or loaded (`Core::save_quick`): GET_AUDIO_VIDEO_ENABLE says "fast savestates".
    quick_state: bool,
    /// SET_MEMORY_MAPS, copied (the core's array is only lent for the call): RetroAchievements reads memory through it.
    memory_map: Vec<MemoryDescriptor>,
}

#[repr(C)]
struct ControllerDescription {
    desc: *const c_char,
    id: c_uint,
}

#[repr(C)]
struct ControllerInfo {
    types: *const ControllerDescription,
    num_types: c_uint,
}

/// The core's disc-swap callbacks (retro_disk_control_callback; the first five entries of the
/// extended version are the same).
#[derive(Clone, Copy)]
struct DiskControl {
    set_eject_state: unsafe extern "C" fn(bool) -> bool,
    #[allow(dead_code)]
    get_eject_state: unsafe extern "C" fn() -> bool,
    get_image_index: unsafe extern "C" fn() -> c_uint,
    set_image_index: unsafe extern "C" fn(c_uint) -> bool,
    get_num_images: unsafe extern "C" fn() -> c_uint,
}

/// The callbacks' state. A global behind a mutex, not a thread-local: threaded cores call input,
/// audio and environment callbacks from their own emulation thread (Mupen64Plus-Next with the
/// threaded renderer, Flycast), and those must reach the same state as `run()`.
struct Global(OnceLock<Mutex<Shared>>);

impl Global {
    fn with_borrow_mut<R>(&self, f: impl FnOnce(&mut Shared) -> R) -> R {
        f(&mut self.0.get_or_init(Default::default).lock().unwrap_or_else(|e| e.into_inner()))
    }

    fn with_borrow<R>(&self, f: impl FnOnce(&Shared) -> R) -> R {
        self.with_borrow_mut(|s| f(s))
    }
}

static SHARED: Global = Global(OnceLock::new());

/// A core's log line, formatted by `pocket_log_printf`: only warnings and errors come here (everything with
/// POCKET_CORE_TRACE). The last warning or error is kept to explain a failed load.
unsafe extern "C" fn log_line(level: c_uint, text: *const c_char) {
    let text = CStr::from_ptr(text).to_string_lossy().into_owned();
    eprint!("[core] {text}");
    if level >= 2 {
        SHARED.with_borrow_mut(|s| s.last_error = text.trim().to_string());
    }
}

/// The commands below that read or write `data`: a NULL pointer for them is refused (a broken core must not
/// crash pocket-core). Where NULL means something (SET_VARIABLE, the disc control interfaces, GET_INPUT_BITMASKS,
/// …), the command handles it itself; `hw_render` and `rumble` check their own.
const NEEDS_DATA: &[c_uint] = &[
    ENV_SET_ROTATION,
    ENV_GET_CAN_DUPE,
    ENV_SET_MESSAGE,
    ENV_GET_SYSTEM_DIRECTORY,
    ENV_SET_PIXEL_FORMAT,
    ENV_GET_VARIABLE,
    ENV_SET_VARIABLES,
    ENV_GET_INPUT_DEVICE_CAPABILITIES,
    ENV_GET_LOG_INTERFACE,
    ENV_GET_CONTENT_DIRECTORY,
    ENV_GET_SAVE_DIRECTORY,
    ENV_SET_SYSTEM_AV_INFO,
    ENV_SET_GEOMETRY,
    ENV_SET_CORE_OPTIONS,
    ENV_SET_CORE_OPTIONS_INTL,
    ENV_SET_CORE_OPTIONS_DISPLAY,
    ENV_SET_CORE_OPTIONS_V2,
    ENV_SET_CORE_OPTIONS_V2_INTL,
    ENV_SET_CORE_OPTIONS_UPDATE_DISPLAY_CALLBACK,
    ENV_GET_VFS_INTERFACE,
    ENV_SET_FRAME_TIME_CALLBACK,
    ENV_SET_MEMORY_MAPS,
];

unsafe extern "C" fn environment(cmd: c_uint, data: *mut c_void) -> bool {
    if let Some(handled) = crate::hw_render::environment(cmd, data) {
        return handled;
    }
    if let Some(handled) = crate::rumble::environment(cmd, data) {
        return handled;
    }
    if data.is_null() && NEEDS_DATA.contains(&cmd) {
        return false;
    }
    if let Some(handled) = achievement_environment(cmd, data) {
        return handled;
    }
    SHARED.with_borrow_mut(|s| match cmd {
        ENV_GET_CAN_DUPE => {
            *(data as *mut bool) = true;
            true
        }
        ENV_SET_PIXEL_FORMAT => {
            let fmt = *(data as *const c_uint);
            s.pixel_format = fmt;
            fmt <= PIXEL_RGB565
        }
        ENV_GET_SYSTEM_DIRECTORY => {
            *(data as *mut *const c_char) = s.system_dir.as_ptr();
            true
        }
        ENV_GET_SAVE_DIRECTORY => {
            *(data as *mut *const c_char) = s.save_dir.as_ptr();
            true
        }
        ENV_GET_LOG_INTERFACE => {
            pocket_log_setup(log_line, if s.trace { 0 } else { 2 });
            (*(data as *mut LogCallback)).log = pocket_log_printf;
            true
        }
        ENV_GET_INPUT_BITMASKS => true,
        ENV_SET_SYSTEM_AV_INFO => {
            s.av_changed = Some(*(data as *const AvInfo));
            true
        }
        // Resolution or aspect changed on the fly (SNES switching to hi-res, for example).
        ENV_SET_GEOMETRY => {
            s.geometry_changed = Some(*(data as *const Geometry));
            true
        }
        // Core options: every dialect lands in `OptionSet`, which also answers the core's reads.
        ENV_SET_VARIABLES => {
            let defs = core_options::parse_variables(data as *const core_options::RawVariable);
            s.opts.declare(Vec::new(), defs);
            true
        }
        ENV_SET_CORE_OPTIONS => {
            let defs = core_options::parse_v1(data as *const core_options::RawDefinition);
            s.opts.declare(Vec::new(), defs);
            true
        }
        ENV_SET_CORE_OPTIONS_INTL => {
            let defs = core_options::parse_v1((*(data as *const core_options::RawIntl)).us);
            s.opts.declare(Vec::new(), defs);
            true
        }
        ENV_SET_CORE_OPTIONS_V2 => {
            let (cats, defs) = core_options::parse_v2(data as *const core_options::RawV2);
            s.opts.declare(cats, defs);
            true
        }
        ENV_SET_CORE_OPTIONS_V2_INTL => {
            let (cats, defs) = core_options::parse_v2((*(data as *const core_options::RawV2Intl)).us);
            s.opts.declare(cats, defs);
            true
        }
        ENV_SET_CORE_OPTIONS_DISPLAY => {
            let d = &*(data as *const core_options::RawDisplay);
            if !d.key.is_null() {
                s.opts.set_visible(&CStr::from_ptr(d.key).to_string_lossy(), d.visible);
            }
            true
        }
        ENV_SET_CORE_OPTIONS_UPDATE_DISPLAY_CALLBACK => {
            // struct { void (*callback)(void); }
            s.update_display = *(data as *const Option<unsafe extern "C" fn()>);
            true
        }
        ENV_GET_VARIABLE => {
            let var = &mut *(data as *mut Variable);
            if var.key.is_null() {
                return false;
            }
            let key = CStr::from_ptr(var.key).to_string_lossy();
            // The string stays valid for the whole session (`OptionSet::get_c`): cores keep it.
            match s.opts.get_c(key.as_ref()) {
                Some(value) => {
                    var.value = value;
                    true
                }
                None => false,
            }
        }
        // A core setting one of its own options (a few do, after detecting the game).
        ENV_SET_VARIABLE => {
            // A NULL argument only asks whether we support it.
            if data.is_null() {
                return true;
            }
            let var = &*(data as *const Variable);
            if var.key.is_null() || var.value.is_null() {
                return false;
            }
            s.opts.set(&CStr::from_ptr(var.key).to_string_lossy(), &CStr::from_ptr(var.value).to_string_lossy())
        }
        ENV_GET_VARIABLE_UPDATE => {
            if !data.is_null() {
                *(data as *mut bool) = s.opts.take_changed();
            }
            true
        }
        ENV_GET_CORE_OPTIONS_VERSION => {
            if !data.is_null() {
                *(data as *mut c_uint) = 2;
            }
            true
        }
        ENV_SET_INPUT_DESCRIPTORS => {
            let mut d = data as *const InputDescriptor;
            let mut described = Vec::new();
            while !d.is_null() && !(*d).description.is_null() {
                let desc = &*d;
                if desc.port == 0 && desc.device == DEVICE_JOYPAD && desc.id < 16 && !described.iter().any(|x: &Described| x.id == desc.id as u8) {
                    described.push(Described { id: desc.id as u8, label: CStr::from_ptr(desc.description).to_string_lossy().into_owned() });
                }
                d = d.add(1);
            }
            if described != s.described {
                s.described = described;
                s.inputs_dirty = true;
            }
            true
        }
        ENV_SET_MESSAGE => {
            let m = &*(data as *const Message);
            if !m.msg.is_null() {
                eprintln!("[core] {}", CStr::from_ptr(m.msg).to_string_lossy());
            }
            true
        }
        ENV_GET_LANGUAGE => false,
        ENV_SET_ROTATION => {
            s.rotation = *(data as *const c_uint) & 3;
            true
        }
        // Cores that read files through libretro's VFS (Stella) get ours, version 3.
        ENV_GET_VFS_INTERFACE => {
            let info = &mut *(data as *mut crate::vfs::InterfaceInfo);
            let ok = info.required_interface_version <= crate::vfs::VERSION;
            if ok {
                info.required_interface_version = crate::vfs::VERSION;
                info.iface = &crate::vfs::INTERFACE;
            }
            ok
        }
        ENV_GET_CONTENT_DIRECTORY => {
            *(data as *mut *const c_char) = s.content_dir.as_ptr();
            true
        }
        // JOYPAD and ANALOG, so DualShock-style cores offer their analog pad.
        ENV_GET_INPUT_DEVICE_CAPABILITIES => {
            *(data as *mut u64) = (1 << DEVICE_JOYPAD) | (1 << DEVICE_ANALOG) | (1 << DEVICE_POINTER);
            true
        }
        // Disc swapping for multi-disc games: the callbacks are kept for `Core::next_disc`.
        ENV_SET_DISK_CONTROL_INTERFACE | ENV_SET_DISK_CONTROL_EXT_INTERFACE => {
            // A NULL pointer means "no disc control".
            s.disk = (!data.is_null()).then(|| *(data as *const DiskControl));
            true
        }
        // Each port's controller types (retro_controller_info array, ended by an entry without types), to find the
        // analog pad by name; player 1's are the first.
        ENV_SET_CONTROLLER_INFO => {
            let mut info = data as *const ControllerInfo;
            let mut ports = Vec::new();
            // At most 16 ports (RetroArch's MAX_USERS), in case a core forgets the end.
            while !info.is_null() && !(*info).types.is_null() && ports.len() < 16 {
                ports.push(
                    (0..(*info).num_types as usize)
                        .map(|i| {
                            let d = &*(*info).types.add(i);
                            (if d.desc.is_null() { String::new() } else { CStr::from_ptr(d.desc).to_string_lossy().into_owned() }, d.id)
                        })
                        .collect::<Vec<_>>(),
                );
                info = info.add(1);
            }
            if let Some(first) = ports.first() {
                s.pad_types = first.clone();
            }
            s.port_types = ports;
            true
        }
        // Read for `Core::load_state`, but not acknowledged: the answer stays "not supported", as before.
        ENV_SET_SERIALIZATION_QUIRKS => {
            if !data.is_null() {
                s.serialization_quirks = *(data as *const u64);
            }
            false
        }
        // The frame time: the core's reference every frame, as RetroArch passes it while it keeps pace (pocket-core
        // runs the core at its own rate, see `frames_due`). Fast-forward runs more frames, each still one frame long.
        ENV_SET_FRAME_TIME_CALLBACK => {
            let ft = *(data as *const FrameTime);
            s.frame_time = ft.callback.is_some().then_some(ft);
            true
        }
        // What this frame needs (`Core::run_with`). NULL only asks whether we answer it.
        ENV_GET_AUDIO_VIDEO_ENABLE => {
            // POCKET_CORE_TRACE=1: which cores ask (docs/HOST.md, Run-ahead).
            if s.trace && !s.traced.contains(&cmd) {
                s.traced.push(cmd);
                eprintln!("[env] the core asks GET_AUDIO_VIDEO_ENABLE");
            }
            if !data.is_null() {
                let mut flags = AV_ALL & !s.av_skip;
                if s.quick_state {
                    flags |= AV_FAST_SAVESTATES;
                }
                *(data as *mut c_uint) = flags;
            }
            true
        }
        ENV_SET_PERFORMANCE_LEVEL | ENV_SET_SUPPORT_NO_GAME => true,
        _ => {
            if s.trace && !s.traced.contains(&cmd) {
                s.traced.push(cmd);
                eprintln!("[env] unhandled command {} ({cmd:#x})", cmd & 0xffff);
            }
            false
        }
    })
}

// ---- RetroAchievements: the core's memory (src/achievements) ---------------------------------------------------

/// `retro_memory_descriptor`, with its pointers kept as addresses so the copy can live in `Shared` (which must be
/// `Send`). Same layout as the C struct: rcheevos reads an array of these (`achievements::session`).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MemoryDescriptor {
    pub flags: u64,
    pub ptr: usize,
    pub offset: usize,
    pub start: usize,
    pub select: usize,
    pub disconnect: usize,
    pub len: usize,
    /// `const char *addrspace`: the core's static string (or NULL).
    pub addrspace: usize,
}

/// `retro_memory_map`
#[repr(C)]
struct MemoryMap {
    descriptors: *const MemoryDescriptor,
    num_descriptors: c_uint,
}

/// What RetroAchievements needs to read the game's memory: the core's memory map (may be empty) and its
/// `retro_get_memory_data` / `retro_get_memory_size`. Valid while the `Core` it came from is loaded.
pub struct AchievementMemory {
    pub descriptors: Vec<MemoryDescriptor>,
    pub data: Option<unsafe extern "C" fn(c_uint) -> *mut c_void>,
    pub size: Option<unsafe extern "C" fn(c_uint) -> usize>,
}

/// The two environment calls that are about achievements; None for every other command.
unsafe fn achievement_environment(cmd: c_uint, data: *mut c_void) -> Option<bool> {
    match cmd {
        // Cores that say they support achievements: yes, we do too.
        ENV_SET_SUPPORT_ACHIEVEMENTS => Some(true),
        ENV_SET_MEMORY_MAPS => {
            let map = &*(data as *const MemoryMap);
            let list = if map.descriptors.is_null() { &[][..] } else { std::slice::from_raw_parts(map.descriptors, map.num_descriptors as usize) };
            SHARED.with_borrow_mut(|s| s.memory_map = list.to_vec());
            Some(true)
        }
        _ => None,
    }
}

unsafe extern "C" fn video_refresh(data: *const c_void, width: c_uint, height: c_uint, pitch: usize) {
    if crate::hw_render::video_refresh(data, width, height) {
        return;
    }
    // NULL means "dupe the previous frame".
    if data.is_null() {
        return;
    }
    SHARED.with_borrow_mut(|s| {
        let (w, h) = (width as usize, height as usize);
        // A frame nobody will see (run-ahead, fast-forward): not even copied.
        if w == 0 || h == 0 || s.av_skip & AV_VIDEO != 0 {
            return;
        }
        // The last line only reaches as far as the picture: a core's buffer may end there, short of `pitch`.
        let bpp = if s.pixel_format == PIXEL_XRGB8888 { 4 } else { 2 };
        let len = (h - 1) * pitch + w * bpp;
        let bytes = std::slice::from_raw_parts(data as *const u8, len);
        let frame = s.frame.get_or_insert_with(|| Frame { width: 0, height: 0, pitch: 0, format: 0, data: Vec::new() });
        frame.width = width;
        frame.height = height;
        frame.pitch = pitch;
        frame.format = s.pixel_format;
        frame.data.clear();
        frame.data.extend_from_slice(bytes);
        // Whole lines for the readers (`to_rgb`, the texture upload): the rest of the last one is padding.
        frame.data.resize(len.max(h * pitch), 0);
    });
}

// Sound of a frame nobody will hear (run-ahead, fast-forward) is dropped here.
unsafe extern "C" fn audio_sample(left: i16, right: i16) {
    SHARED.with_borrow_mut(|s| {
        if s.av_skip & AV_AUDIO == 0 {
            s.audio.extend_from_slice(&[left, right])
        }
    });
}

unsafe extern "C" fn audio_batch(data: *const i16, frames: usize) -> usize {
    SHARED.with_borrow_mut(|s| {
        if s.av_skip & AV_AUDIO == 0 && !data.is_null() {
            s.audio.extend_from_slice(std::slice::from_raw_parts(data, frames * 2))
        }
    });
    frames
}

unsafe extern "C" fn input_poll() {}

/// A joypad read of `buttons`: one button (`id` 0..15) or all of them (JOYPAD_MASK).
fn joypad_read(buttons: u16, id: c_uint) -> i16 {
    match id {
        JOYPAD_MASK => buttons as i16,
        0..=15 => ((buttons >> id) & 1) as i16,
        _ => 0,
    }
}

unsafe extern "C" fn input_state(port: c_uint, device: c_uint, index: c_uint, id: c_uint) -> i16 {
    // Cores may use sub-classes of a device type (DualShock = ANALOG + 1<<8); they query with the base type.
    let device = device & 0xff;
    // Players 2 to 4: their pad and sticks; the touch screen is player 1's.
    if port != 0 {
        let Some(p) = (port as usize).checked_sub(1).filter(|p| *p < OTHER_PORTS) else { return 0 };
        let (buttons, analog) = SHARED.with_borrow(|s| s.others[p]);
        return match device {
            DEVICE_JOYPAD => joypad_read(buttons, id),
            DEVICE_ANALOG if index < 2 && id < 2 => analog[(index * 2 + id) as usize],
            _ => 0,
        };
    }
    if device == DEVICE_ANALOG {
        // index 0 = left stick, 1 = right; id 0 = x, 1 = y.
        return if index < 2 && id < 2 { SHARED.with_borrow(|s| s.analog[(index * 2 + id) as usize]) } else { 0 };
    }
    if matches!(device, DEVICE_POINTER | DEVICE_MOUSE | DEVICE_LIGHTGUN) {
        // The touch screen: libretro's RETRO_DEVICE_ID_POINTER_*, MOUSE_* and LIGHTGUN_* ids.
        let p = SHARED.with_borrow(|s| s.pointer);
        return match (device, id) {
            (DEVICE_POINTER, _) if index != 0 => 0,
            (DEVICE_POINTER, 0) => p.x,
            (DEVICE_POINTER, 1) => p.y,
            (DEVICE_POINTER, 2) | (DEVICE_POINTER, 3) => p.pressed as i16,
            (DEVICE_MOUSE, 0) => p.dx,
            (DEVICE_MOUSE, 1) => p.dy,
            (DEVICE_MOUSE, 2) => p.pressed as i16,
            (DEVICE_LIGHTGUN, 13) => p.x,
            (DEVICE_LIGHTGUN, 14) => p.y,
            (DEVICE_LIGHTGUN, 2) => p.pressed as i16,
            (DEVICE_LIGHTGUN, 15) => !p.pressed as i16,
            _ => 0,
        };
    }
    if device != DEVICE_JOYPAD {
        return 0;
    }
    SHARED.with_borrow(|s| joypad_read(s.buttons, id))
}

/// The controller type to plug in from a port's list: the DualShock or another analog pad when `analog` (some cores
/// use sub-classes of DEVICE_ANALOG), else the plain joypad; Dreamcast and N64 cores name no "analog" type, their
/// standard pad has the sticks.
fn pad_for(types: &[(String, c_uint)], analog: bool) -> (String, c_uint) {
    let pick = |word: &str| types.iter().find(|(d, _)| d.to_ascii_lowercase().contains(word)).cloned();
    let joypad = || types.iter().find(|(_, id)| id & 0xff == DEVICE_JOYPAD).cloned();
    let analog_pad = if analog { pick("dualshock").or_else(|| pick("analog")) } else { None };
    analog_pad.or_else(joypad).unwrap_or_else(|| ("joypad".into(), DEVICE_JOYPAD))
}

/// A loaded core with a running game.
pub struct Core {
    lib: Library,
    pub av: AvInfo,
    pub name: String,
    /// The core's own version string (`library_version`), for RetroAchievements' user agent.
    pub version: String,
    sram_path: PathBuf,
    /// What is on disk, so unchanged saves aren't rewritten.
    sram_written: Vec<u8>,
    /// When the battery save read at start was written (`resume` compares it with the state's time).
    sram_time: Option<SystemTime>,
    /// The cartridge clock's file (`MEMORY_RTC`), and what is on disk: like `sram_path` and `sram_written`.
    rtc_path: PathBuf,
    rtc_written: Vec<u8>,
    /// Writes the 10 s flushes off the game thread, and keeps what became of every write (`save_trouble`).
    saver: crate::save_writer::SaveWriter,
    // Kept alive while the core may read it.
    _game_data: Vec<u8>,
    _game_path: CString,
    /// The patches applied to the game, by file name (`softpatch`), for Pixel Path; empty for an unpatched game.
    pub patches: Vec<String>,
    /// The patched copy for a core that opens the file itself, removed after the core unloaded (`Drop` runs first).
    _patched_copy: Option<crate::softpatch::TempDir>,
    /// Save states switched off for this system (`disable_states`).
    no_states: bool,
    /// What `resume` made of the auto-resume state and where it put it (`slots::taken_auto_path` or
    /// `slots::retry_auto_path`), for a quit without saving (`keep_resume_point`).
    taken: Option<(Resume, PathBuf)>,
    /// How often `save_quick` asked for the state size, and the ms it took (`take_size_queries`).
    size_queries: std::cell::Cell<(u32, f32)>,
}

type VoidFn = unsafe extern "C" fn();

/// One of the core's memory areas (`retro_get_memory_data`; 0 = battery save, 2 = work RAM). Mutable, so it
/// takes the core's library mutably: while the slice lives nothing else can call into the core (`run` and
/// the rest take `&Core`), and the core can't change the memory behind it. A free function, not a method,
/// so callers can keep using the other fields of `Core` meanwhile.
fn memory_of(lib: &mut Library, id: c_uint) -> Option<&mut [u8]> {
    unsafe {
        let data: Symbol<unsafe extern "C" fn(c_uint) -> *mut c_void> = lib.get(b"retro_get_memory_data").ok()?;
        let size: Symbol<unsafe extern "C" fn(c_uint) -> usize> = lib.get(b"retro_get_memory_size").ok()?;
        let (ptr, len) = (data(id), size(id));
        (!ptr.is_null() && len > 0).then(|| std::slice::from_raw_parts_mut(ptr as *mut u8, len))
    }
}

/// What became of the auto-resume state at start (`Core::resume`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Resume {
    /// There is none: the first start, or the last session ended without writing one.
    NoState,
    Resumed,
    /// Resumed from the state the last start loaded: that session crashed (or lost power) before it could
    /// write a new one. Tried once more only (`slots::retry_auto_path`).
    ResumedAgain,
    /// The core refused the state (another core version, a broken file).
    Failed,
    /// Not resumed: the battery save on disk is newer than the state and differs from the one inside it
    /// (written later in a session that crashed, or by RetroArch or a standalone emulator). The game
    /// starts fresh from that save instead.
    SaveIsNewer,
}

impl Core {
    /// Loads `core_path`, initialises it and starts `rom`. SRAM is read from `save_dir/<rom stem>.srm`.
    /// `options` are the user's saved core option values; they win over our curated defaults
    /// (`core_options::curation`) and the core's own.
    pub fn load(core_path: &Path, rom: &Path, system_dir: &Path, save_dir: &Path, options: &[(&str, &str)]) -> Result<Core, String> {
        crate::hw_render::reset();
        SHARED.with_borrow_mut(|s| {
            *s = Shared::default();
            let saved: Vec<(String, String)> = options.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
            s.opts = OptionSet::new(core_options::curation(&core_options::core_id(core_path)), &saved);
            s.trace = std::env::var_os("POCKET_CORE_TRACE").is_some();
            crate::vfs::TRACE.store(s.trace, std::sync::atomic::Ordering::Relaxed);
            s.system_dir = CString::new(system_dir.as_os_str().as_encoded_bytes()).unwrap_or_default();
            s.save_dir = CString::new(save_dir.as_os_str().as_encoded_bytes()).unwrap_or_default();
            s.content_dir = CString::new(rom.parent().unwrap_or(Path::new("/")).as_os_str().as_encoded_bytes()).unwrap_or_default();
        });
        let lib = unsafe { Library::new(core_path) }.map_err(|e| format!("load {}: {e}", core_path.display()))?;
        unsafe {
            let sym = |name: &[u8]| -> Result<Symbol<VoidFn>, String> {
                lib.get::<VoidFn>(name).map_err(|e| format!("{}: {e}", String::from_utf8_lossy(name)))
            };
            let set_env: Symbol<unsafe extern "C" fn(unsafe extern "C" fn(c_uint, *mut c_void) -> bool)> =
                lib.get(b"retro_set_environment").map_err(|e| e.to_string())?;
            set_env(environment);
            let set_video: Symbol<unsafe extern "C" fn(unsafe extern "C" fn(*const c_void, c_uint, c_uint, usize))> =
                lib.get(b"retro_set_video_refresh").map_err(|e| e.to_string())?;
            set_video(video_refresh);
            let set_audio: Symbol<unsafe extern "C" fn(unsafe extern "C" fn(i16, i16))> =
                lib.get(b"retro_set_audio_sample").map_err(|e| e.to_string())?;
            set_audio(audio_sample);
            let set_batch: Symbol<unsafe extern "C" fn(unsafe extern "C" fn(*const i16, usize) -> usize)> =
                lib.get(b"retro_set_audio_sample_batch").map_err(|e| e.to_string())?;
            set_batch(audio_batch);
            let set_poll: Symbol<unsafe extern "C" fn(unsafe extern "C" fn())> =
                lib.get(b"retro_set_input_poll").map_err(|e| e.to_string())?;
            set_poll(input_poll);
            let set_state: Symbol<unsafe extern "C" fn(unsafe extern "C" fn(c_uint, c_uint, c_uint, c_uint) -> i16)> =
                lib.get(b"retro_set_input_state").map_err(|e| e.to_string())?;
            set_state(input_state);
            sym(b"retro_init")?();

            let get_info: Symbol<unsafe extern "C" fn(*mut SystemInfo)> = lib.get(b"retro_get_system_info").map_err(|e| e.to_string())?;
            let mut info: SystemInfo = std::mem::zeroed();
            get_info(&mut info);
            let name = if info.library_name.is_null() { "core".into() } else { CStr::from_ptr(info.library_name).to_string_lossy().into_owned() };
            let version = if info.library_version.is_null() { String::new() } else { CStr::from_ptr(info.library_version).to_string_lossy().into_owned() };

            // Cores that take ROM data and don't open zip files themselves get the ROM unpacked (as RetroArch does);
            // the path then names the file inside the zip, so the core sees the real extension.
            let exts = if info.valid_extensions.is_null() { String::new() } else { CStr::from_ptr(info.valid_extensions).to_string_lossy().to_ascii_lowercase() };
            if SHARED.with_borrow(|s| s.trace) {
                eprintln!("[core] {name}: extensions {exts}, need_fullpath {}, block_extract {}", info.need_fullpath, info.block_extract);
            }
            let exts: Vec<&str> = exts.split('|').collect();
            let is_zip = rom.extension().is_some_and(|e| e.eq_ignore_ascii_case("zip"));
            // An empty ROM path starts the core without a game (a console's BIOS menu), for cores
            // that support it.
            let no_game = rom.as_os_str().is_empty();
            // Soft-patching (`softpatch`): an .ips/.bps/.ups with the game's name next to it is applied before the core
            // sees the game; one that doesn't apply stops the start with its reason, never a silent unpatched start.
            let patches = if no_game { Vec::new() } else { crate::softpatch::find(rom) };
            let mut patched = None;
            let (shown_path, game_data) = if !patches.is_empty() {
                let p = crate::softpatch::for_core(rom, &patches, &exts, info.need_fullpath, &std::env::temp_dir())?;
                println!("softpatch: {} applied", p.names.join(", "));
                patched = Some((p.names, p.temp));
                (p.path, p.data)
            } else if info.need_fullpath || no_game {
                (rom.to_path_buf(), Vec::new())
            } else if is_zip && !exts.contains(&"zip") {
                let (name, data) = crate::archive::extract_rom(rom, &exts)?;
                (rom.parent().unwrap_or(Path::new("")).join(name), data)
            } else {
                (rom.to_path_buf(), std::fs::read(rom).map_err(|e| format!("{}: {e}", rom.display()))?)
            };
            let game_path = CString::new(shown_path.as_os_str().as_encoded_bytes()).map_err(|e| e.to_string())?;
            let game = GameInfo {
                path: game_path.as_ptr(),
                data: if game_data.is_empty() { std::ptr::null() } else { game_data.as_ptr() as *const c_void },
                size: game_data.len(),
                meta: std::ptr::null(),
            };
            let load_game: Symbol<unsafe extern "C" fn(*const GameInfo) -> bool> = lib.get(b"retro_load_game").map_err(|e| e.to_string())?;
            if !load_game(if no_game { std::ptr::null() } else { &game }) {
                let why = SHARED.with_borrow(|s| s.last_error.clone());
                return Err(if why.is_empty() { format!("{name} could not load {}", rom.display()) } else { format!("{name}: {why}") });
            }
            // The core has read its options while loading: the first GET_VARIABLE_UPDATE has nothing new, as in
            // RetroArch. ScummVM rereads them on an update and on its first frame that crashes (docs/HOST.md).
            SHARED.with_borrow_mut(|s| s.opts.take_changed());
            let get_av: Symbol<unsafe extern "C" fn(*mut AvInfo)> = lib.get(b"retro_get_system_av_info").map_err(|e| e.to_string())?;
            let mut av = AvInfo::default();
            get_av(&mut av);

            // Saves keep the ROM file's name, patched or not (as RetroArch: a hack shares its base game's save).
            let stem = rom.file_stem().and_then(|s| s.to_str()).unwrap_or("game");
            let (patches, patched_copy) = patched.unwrap_or_default();
            let mut core = Core {
                lib,
                av,
                name,
                version,
                sram_path: save_dir.join(format!("{stem}.srm")),
                sram_written: Vec::new(),
                sram_time: None,
                rtc_path: save_dir.join(format!("{stem}.rtc")),
                rtc_written: Vec::new(),
                saver: crate::save_writer::SaveWriter::new(),
                _game_data: game_data,
                _game_path: game_path,
                patches,
                _patched_copy: patched_copy,
                no_states: false,
                taken: None,
                size_queries: Default::default(),
            };
            core.read_sram();
            core.read_rtc();
            Ok(core)
        }
    }

    /// One of the core's memory areas (battery save, work RAM), as `memory_of`.
    fn memory(&mut self, id: c_uint) -> Option<&mut [u8]> {
        memory_of(&mut self.lib, id)
    }

    fn read_sram(&mut self) {
        if let (Some(mem), Ok(saved)) = (memory_of(&mut self.lib, MEMORY_SAVE_RAM), std::fs::read(&self.sram_path)) {
            let n = mem.len().min(saved.len());
            mem[..n].copy_from_slice(&saved[..n]);
            self.sram_written = saved;
            self.sram_time = std::fs::metadata(&self.sram_path).and_then(|m| m.modified()).ok();
        }
    }

    /// The cartridge clock from `<game>.rtc`, before the first frame like the battery save. Nothing when the core
    /// has no clock memory (or one of size 0, as mGBA for GBA games) or there is no file yet.
    fn read_rtc(&mut self) {
        if let (Some(mem), Ok(saved)) = (memory_of(&mut self.lib, MEMORY_RTC), std::fs::read(&self.rtc_path)) {
            let n = mem.len().min(saved.len());
            mem[..n].copy_from_slice(&saved[..n]);
            self.rtc_written = saved;
        }
    }

    /// Whether the game's battery save in memory differs from the one on disk: in the length both have, or by
    /// anything but padding (0x00, 0xFF) in a file longer than the core's save (writing the memory would cut that off).
    fn sram_differs(&mut self) -> bool {
        memory_of(&mut self.lib, MEMORY_SAVE_RAM).is_some_and(|mem| {
            let n = mem.len().min(self.sram_written.len());
            mem[..n] != self.sram_written[..n] || self.sram_written[n..].iter().any(|b| *b != 0 && *b != 0xff)
        })
    }

    /// Keeps the battery save under another extension (`hostsys::HostSystem::save_ext`) and reads it from
    /// there. A `.srm` read at load stays in memory when no such file exists yet, so it carries over.
    /// For `.sav` (the DS) a DraStic `<game>.dsv` next to the game becomes that `.sav` first when there is none
    /// (`dsv::take_over`); true when it did, for the toast.
    pub fn use_save_extension(&mut self, ext: &str) -> bool {
        let mut took = false;
        if ext != "srm" {
            let srm = self.sram_path.clone();
            self.sram_path = srm.with_extension(ext);
            if ext == "sav" {
                match crate::dsv::take_over(&srm.with_extension("dsv"), &self.sram_path, &srm) {
                    crate::dsv::Outcome::Took { bytes, footer } => {
                        println!("took over the DraStic save {}: {bytes} bytes{}", srm.with_extension("dsv").display(), if footer { ", footer cut off" } else { "" });
                        took = true;
                    }
                    crate::dsv::Outcome::Refused(why) => println!("DraStic save not taken over: {why}"),
                    crate::dsv::Outcome::Nothing => {}
                }
            }
            self.read_sram();
        }
        took
    }

    /// Writes the game's battery save and its cartridge clock (each if the game has one and it changed, or its last
    /// write failed) next to RetroArch's, now and on this thread (quit, sleep), through `fsutil::write_atomic`: a power
    /// cut can't leave half a save behind, and a failed write leaves the last good file in place. A background write
    /// still running finishes first, so it can't land after this one. One try, never a wait for the card to recover:
    /// Err says why a file (this one or an earlier flush's) didn't reach the card (`save_trouble`).
    pub fn write_sram(&mut self) -> Result<(), crate::save_trouble::Trouble> {
        self.saver.wait();
        for id in [MEMORY_SAVE_RAM, MEMORY_RTC] {
            let (path, written) = if id == MEMORY_RTC { (&self.rtc_path, &mut self.rtc_written) } else { (&self.sram_path, &mut self.sram_written) };
            let Some(mem) = memory_of(&mut self.lib, id) else { continue };
            if !self.saver.failed(path) && mem == written.as_slice() {
                continue;
            }
            if self.saver.write_now(path, mem).is_ok() {
                *written = mem.to_vec();
            }
        }
        self.saver.trouble().map_or(Ok(()), Err)
    }

    /// The periodic flush (every 10 s): like `write_sram`, but the game thread only copies the bytes;
    /// `save_writer` writes them. A failed write is tried again at the next flush.
    pub fn write_sram_in_background(&mut self) {
        for id in [MEMORY_SAVE_RAM, MEMORY_RTC] {
            let (path, written) = if id == MEMORY_RTC { (&self.rtc_path, &mut self.rtc_written) } else { (&self.sram_path, &mut self.sram_written) };
            if self.saver.failed(path) {
                written.clear();
            }
            let Some(mem) = memory_of(&mut self.lib, id) else { continue };
            if mem == written.as_slice() {
                continue;
            }
            let bytes = mem.to_vec();
            self.saver.submit(path.clone(), bytes.clone());
            *written = bytes;
        }
    }

    /// Why the game's saves don't reach the card, if the newest write of one failed (the flushes so far, without
    /// waiting for one under way): pocket-core's line over the game and pocket-ui's note (`save_trouble`).
    pub fn save_trouble(&self) -> Option<crate::save_trouble::Trouble> {
        self.saver.trouble()
    }

    /// Snapshot of the whole emulated machine (libretro serialize).
    pub fn save_state(&self) -> Option<Vec<u8>> {
        if self.no_states {
            return None;
        }
        unsafe {
            let size: Symbol<unsafe extern "C" fn() -> usize> = self.lib.get(b"retro_serialize_size").ok()?;
            let save: Symbol<unsafe extern "C" fn(*mut c_void, usize) -> bool> = self.lib.get(b"retro_serialize").ok()?;
            let mut buf = vec![0u8; size()];
            (!buf.is_empty() && save(buf.as_mut_ptr() as *mut c_void, buf.len())).then_some(buf)
        }
    }

    /// The core's states vary in size though it didn't say so (`hostsys::HostSystem::states_vary`): shorter ones are
    /// handed to it, as RetroArch hands every state file whatever its length.
    pub fn states_vary_in_size(&self) {
        SHARED.with_borrow_mut(|s| s.serialization_quirks |= QUIRK_CORE_VARIABLE_SIZE);
    }

    /// No saving, loading or rewinding from here on, whatever the core offers (`hostsys::HostSystem::no_states`).
    pub fn disable_states(&mut self) {
        self.no_states = true;
    }

    /// Whether the core has save states at all (ScummVM's state size is 0) and they are not switched off.
    pub fn has_states(&self) -> bool {
        !self.no_states
            && unsafe { self.lib.get::<unsafe extern "C" fn() -> usize>(b"retro_serialize_size").is_ok_and(|size| size() > 0) }
    }

    /// The size of the core's state (`retro_serialize_size`), None without states. mGBA makes a whole state to tell
    /// it, so it costs about half a save there: asked once, by Earlier's cost guard (`timeline`).
    pub fn state_size(&self) -> Option<usize> {
        if self.no_states {
            return None;
        }
        unsafe { self.lib.get::<unsafe extern "C" fn() -> usize>(b"retro_serialize_size").ok().map(|size| size()) }
    }

    /// Like `save_state`, into a buffer that is reused from frame to frame (rewind takes one per frame).
    pub fn save_state_into(&self, buf: &mut Vec<u8>) -> bool {
        if self.no_states {
            return false;
        }
        unsafe {
            let Ok(size) = self.lib.get::<unsafe extern "C" fn() -> usize>(b"retro_serialize_size") else { return false };
            let Ok(save) = self.lib.get::<unsafe extern "C" fn(*mut c_void, usize) -> bool>(b"retro_serialize") else { return false };
            buf.resize(size(), 0);
            !buf.is_empty() && save(buf.as_mut_ptr() as *mut c_void, buf.len())
        }
    }

    /// Loads a state (libretro unserialize). One shorter than the core's state size (an empty or cut-off file,
    /// a state of another core version) is refused before the core sees it: many cores read their full size
    /// whatever length they are given. Cores whose states vary in size check that themselves. The error
    /// says why, short enough for a toast.
    pub fn load_state(&self, data: &[u8]) -> Result<(), String> {
        self.load_state_reached(data).map_err(|(_, why)| why)
    }

    /// `load_state`; the error also says whether the core was called (and may have changed part of the game).
    pub fn load_state_reached(&self, data: &[u8]) -> Result<(), (bool, String)> {
        let refused = |why: String| Err((false, why));
        if self.no_states {
            return refused("no save states here".into());
        }
        unsafe {
            let Ok(size) = self.lib.get::<unsafe extern "C" fn() -> usize>(b"retro_serialize_size") else { return refused("the core has no states".into()) };
            let Ok(load) = self.lib.get::<unsafe extern "C" fn(*const c_void, usize) -> bool>(b"retro_unserialize") else {
                return refused("the core has no states".into());
            };
            let variable = SHARED.with_borrow(|s| s.serialization_quirks & QUIRK_CORE_VARIABLE_SIZE != 0);
            let expected = size();
            if data.is_empty() {
                return refused("the file is empty".into());
            }
            if data.len() < expected && !variable {
                return refused(format!("wrong size ({} of {expected} bytes)", data.len()));
            }
            if load(data.as_ptr() as *const c_void, data.len()) {
                Ok(())
            } else {
                Err((true, "the core refused it".into()))
            }
        }
    }

    /// Instant resume at start: loads the auto-resume state `state` (`slots::auto_path`), unless the battery
    /// save is newer (`Resume::SaveIsNewer`).
    ///
    /// Most cores keep the battery save inside their states. A state older than the save on disk would
    /// put the older save back into the game, and the 10 s flush (`write_sram`) would then write it over
    /// the newer file: the in-game save made after the state was taken would be gone (a session that
    /// crashed after an in-game save, or one played in RetroArch or standalone melonDS in between).
    /// Restoring only the newer save into the resumed game is not enough either: the game keeps its own
    /// copy of the older one in its RAM and writes that back at its next save. So the game starts fresh.
    ///
    /// The state is taken (renamed to `slots::taken_auto_path`) before it loads; a clean exit or a sleep
    /// writes a new one and clears the taken ones (`slots::auto_written`). When a session ends without
    /// that (a crash, a dead battery), the next start finds only the taken state. It is still the quit point
    /// of the session before, so it is tried once more, renamed to `slots::retry_auto_path` first (the
    /// battery-save check above applies the same way). If that start crashes too, maybe because of the
    /// state, the one after starts fresh. A state the core refuses is not tried again.
    pub fn resume(&mut self, auto: &Path) -> Resume {
        let taken = crate::slots::taken_auto_path(auto);
        let retry = crate::slots::retry_auto_path(auto);
        // The exit that wrote the state couldn't write the battery save (a full or failing card): the state holds the
        // newer one (`slots::battery_unsaved`). Read once, whatever happens below.
        let battery_unsaved = crate::slots::take_battery_unsaved(auto);
        let (state, to, again) = if auto.exists() {
            (auto, taken.as_path(), false)
        } else if taken.exists() {
            (taken.as_path(), retry.as_path(), true)
        } else {
            return Resume::NoState;
        };
        let Ok(data) = std::fs::read(state) else { return Resume::NoState };
        let state_time = std::fs::metadata(state).and_then(|m| m.modified()).ok();
        let _ = std::fs::rename(state, to);
        let save_is_newer = matches!((self.sram_time, state_time), (Some(save_at), Some(state_at)) if save_at > state_at);
        // The game as it starts, to go back to: a whole snapshot only when the clock already says the battery save is
        // newer (it costs a serialize at start), else at least its battery save (small) and a reset.
        let fresh = if save_is_newer { self.save_state() } else { None };
        let sram_at_start = self.memories_now();
        if let Err((reached, why)) = self.load_state_reached(&data) {
            eprintln!("resume from {}: {why}", state.display());
            let _ = std::fs::rename(to, &retry);
            // The core may have put part of the state in place before it gave up, its battery save among it, which the
            // next flush would write over the one on disk (SOL-02): back to the start.
            if reached {
                self.back_to_start(fresh, sram_at_start);
            }
            return Resume::Failed;
        }
        // The contents decide, not the clock (SOL-02): a battery save on disk that differs from the one in the state
        // was written later (pocket-core writes the battery save before the state) or brought in from elsewhere.
        // Unless the state's own exit couldn't write the battery save and nothing wrote one since: then the state's is
        // the newer one, and the next flush puts it on the card.
        if !self.sram_differs() || (battery_unsaved && !save_is_newer) {
            let r = if again { Resume::ResumedAgain } else { Resume::Resumed };
            self.taken = Some((r, to.to_path_buf()));
            return r;
        }
        self.taken = Some((Resume::SaveIsNewer, to.to_path_buf()));
        self.back_to_start(fresh, sram_at_start);
        Resume::SaveIsNewer
    }

    /// The game as it started, after a state that must not stay: the snapshot taken before (`fresh`), else a reset with
    /// the battery save it had then.
    fn back_to_start(&mut self, fresh: Option<Vec<u8>>, sram_at_start: [Option<Vec<u8>>; 2]) {
        if fresh.is_some_and(|f| self.load_state(&f).is_ok()) {
            return;
        }
        self.reset();
        for (id, start) in [MEMORY_SAVE_RAM, MEMORY_RTC].into_iter().zip(sram_at_start) {
            if let (Some(mem), Some(start)) = (memory_of(&mut self.lib, id), start) {
                let n = mem.len().min(start.len());
                mem[..n].copy_from_slice(&start[..n]);
            }
        }
    }

    /// The battery save and the cartridge clock as they are now (`back_to_start` puts them back after a reset).
    fn memories_now(&mut self) -> [Option<Vec<u8>>; 2] {
        [MEMORY_SAVE_RAM, MEMORY_RTC].map(|id| memory_of(&mut self.lib, id).map(|m| m.to_vec()))
    }

    /// Quit without saving (the pause menu's "Quit without saving"; the battery save is written as on every
    /// exit, `write_sram` first): no new auto-resume state, so the state this session resumed from goes back to
    /// `auto` and the next start continues from the last saving quit again, as often as the player quits this
    /// way. Nothing is put back when a state was written in the session since (before a sleep): that is the
    /// newer resume point. A state that lost to a newer battery save (`Resume::SaveIsNewer`) is dropped instead:
    /// it would lose again at every start. Its time is kept, so `resume` still sees a battery save written in
    /// this session as the newer one and doesn't roll it back.
    pub fn keep_resume_point(&mut self, auto: &Path) {
        let Some((how, taken)) = self.taken.take() else { return };
        if auto.exists() || !taken.exists() {
            return;
        }
        match how {
            Resume::Resumed | Resume::ResumedAgain => {
                let _ = std::fs::rename(&taken, auto);
            }
            Resume::SaveIsNewer => {
                let _ = std::fs::remove_file(&taken);
                let _ = std::fs::remove_file(crate::slots::preview_path(auto));
            }
            Resume::NoState | Resume::Failed => {}
        }
    }

    /// retro_reset: the console's reset button.
    fn reset(&self) {
        unsafe {
            if let Ok(f) = self.lib.get::<VoidFn>(b"retro_reset") {
                f();
            }
        }
    }

    /// Loads a core without a game, only to read the options it declares (`pocket-core --dump-options`).
    pub fn probe_options(core_path: &Path) -> Result<OptionSet, String> {
        SHARED.with_borrow_mut(|s| {
            *s = Shared::default();
            s.opts = OptionSet::new(core_options::curation(&core_options::core_id(core_path)), &[]);
        });
        let lib = unsafe { Library::new(core_path) }.map_err(|e| format!("load {}: {e}", core_path.display()))?;
        unsafe {
            let set_env: Symbol<unsafe extern "C" fn(unsafe extern "C" fn(c_uint, *mut c_void) -> bool)> = lib.get(b"retro_set_environment").map_err(|e| e.to_string())?;
            set_env(environment);
            if let Ok(init) = lib.get::<VoidFn>(b"retro_init") {
                init();
            }
            if let Ok(deinit) = lib.get::<VoidFn>(b"retro_deinit") {
                deinit();
            }
        }
        Ok(SHARED.with_borrow_mut(|s| std::mem::take(&mut s.opts)))
    }

    /// The pause menu changed a core option. True when the value changed. The core is told afterwards,
    /// so it can show or hide the options that depend on this one.
    pub fn set_option(&self, key: &str, value: &str) -> bool {
        let changed = SHARED.with_borrow_mut(|s| s.opts.set(key, value));
        if changed {
            self.notify_options();
        }
        changed
    }

    /// Back to the defaults plus the saved `opt.*` values (after Save changes or Restore defaults).
    pub fn reload_options(&self, saved: &[(String, String)]) -> bool {
        let changed = SHARED.with_borrow_mut(|s| s.opts.reload(saved));
        if changed {
            self.notify_options();
        }
        changed
    }

    fn notify_options(&self) {
        // Outside the borrow: the core answers with SET_CORE_OPTIONS_DISPLAY calls.
        if let Some(cb) = SHARED.with_borrow(|s| s.update_display) {
            unsafe { cb() };
        }
        SHARED.with_borrow_mut(|s| s.opts.mark_dirty());
    }

    /// Options the menu should show now, and their values.
    pub fn options_snapshot(&self) -> (Vec<core_options::FileCategory>, Vec<core_options::FileOption>) {
        SHARED.with_borrow(|s| s.opts.snapshot())
    }

    /// True once after options, their visibility or the button names changed: time to rewrite the snapshot file.
    pub fn take_snapshot_dirty(&self) -> bool {
        SHARED.with_borrow_mut(|s| s.opts.take_dirty() | std::mem::take(&mut s.inputs_dirty))
    }

    /// The core's names for player 1's buttons.
    pub fn described_inputs(&self) -> Vec<Described> {
        SHARED.with_borrow(|s| s.described.clone())
    }

    /// Analog sticks for cores that read them: left x/y, right x/y.
    pub fn set_analog(&self, analog: [i16; 4]) {
        SHARED.with_borrow_mut(|s| s.analog = analog);
    }

    /// The finger on the touch screen for the next frame, as -32767..32767 across the picture (`None`: no
    /// finger). Cores read it as RETRO_DEVICE_POINTER, as a light gun's aim, or as mouse movement.
    pub fn set_pointer(&self, at: Option<(i16, i16)>) {
        let (w, h) = (self.av.geometry.base_width as f32, self.av.geometry.base_height as f32);
        SHARED.with_borrow_mut(|s| {
            let old = s.pointer;
            s.pointer = match at {
                Some((x, y)) if old.pressed => {
                    Pointer { pressed: true, x, y, dx: ((x as f32 - old.x as f32) / 65534.0 * w) as i16, dy: ((y as f32 - old.y as f32) / 65534.0 * h) as i16 }
                }
                Some((x, y)) => Pointer { pressed: true, x, y, dx: 0, dy: 0 },
                None => Pointer { pressed: false, dx: 0, dy: 0, ..old },
            };
        });
    }

    /// The current value of a core option.
    pub fn option_value(&self, key: &str) -> Option<String> {
        SHARED.with_borrow(|s| s.opts.get(key).map(String::from))
    }

    /// Plugs an analog pad (DualShock-style) into player 1's port. The core's own list of controller
    /// types decides the device id (some cores use sub-classes of DEVICE_ANALOG). Cores whose standard
    /// pad has the sticks already (Dreamcast, N64: no type named "analog") get that one, the plain
    /// joypad, as RetroArch plugs in: Flycast without it has no controller and so no memory card.
    /// Returns the description of the pad chosen.
    pub fn use_analog_pad(&self) -> String {
        let types = SHARED.with_borrow(|s| s.pad_types.clone());
        let (desc, id) = pad_for(&types, true);
        self.plug(0, id);
        format!("{desc} (device {id})")
    }

    /// Plugs a pad into the ports of players 2 to 4 that the core lists (SET_CONTROLLER_INFO), as RetroArch does at
    /// the start: the analog pad where player 1 has one (`analog`), else the plain joypad. Ports the core doesn't
    /// list keep what the core plugged in itself. Returns one line per port plugged, for the log.
    pub fn plug_other_players(&self, analog: bool) -> Vec<String> {
        let ports = SHARED.with_borrow(|s| s.port_types.clone());
        ports
            .iter()
            .enumerate()
            .skip(1)
            .take(OTHER_PORTS)
            .filter(|(_, types)| !types.is_empty())
            .map(|(port, types)| {
                let (desc, id) = pad_for(types, analog);
                self.plug(port as c_uint, id);
                format!("player {}: {desc} (device {id})", port + 1)
            })
            .collect()
    }

    fn plug(&self, port: c_uint, device: c_uint) {
        unsafe {
            if let Ok(f) = self.lib.get::<unsafe extern "C" fn(c_uint, c_uint)>(b"retro_set_controller_port_device") {
                f(port, device);
            }
        }
    }

    /// Players 2 to 4 (`player` 1..3) for the next frames: joypad bits and sticks. Player 1's buttons go with `run`.
    pub fn set_player(&self, player: usize, buttons: u16, analog: [i16; 4]) {
        if let Some(p) = player.checked_sub(1).filter(|p| *p < OTHER_PORTS) {
            SHARED.with_borrow_mut(|s| s.others[p] = (buttons, analog));
        }
    }

    /// One byte of the core's work RAM (test aid: shows whether a cheat has written its value).
    pub fn peek_ram(&mut self, offset: usize) -> Option<u8> {
        self.memory(2).and_then(|m| m.get(offset).copied())
    }

    /// Hands the core exactly these cheat codes (`retro_cheat_reset`, then `retro_cheat_set` for each, all enabled).
    pub fn set_cheats(&self, codes: &[String]) {
        unsafe {
            if let Ok(reset) = self.lib.get::<VoidFn>(b"retro_cheat_reset") {
                reset();
            }
            let Ok(set) = self.lib.get::<unsafe extern "C" fn(c_uint, bool, *const c_char)>(b"retro_cheat_set") else { return };
            for (i, code) in codes.iter().enumerate() {
                if let Ok(c) = CString::new(code.as_str()) {
                    set(i as c_uint, true, c.as_ptr());
                }
            }
        }
    }

    /// Where RetroAchievements reads the game's memory (`achievements::session`): the memory map the core gave
    /// (SET_MEMORY_MAPS) and its memory functions.
    pub fn achievement_memory(&self) -> AchievementMemory {
        unsafe {
            AchievementMemory {
                descriptors: SHARED.with_borrow(|s| s.memory_map.clone()),
                data: self.lib.get::<unsafe extern "C" fn(c_uint) -> *mut c_void>(b"retro_get_memory_data").ok().map(|f| *f),
                size: self.lib.get::<unsafe extern "C" fn(c_uint) -> usize>(b"retro_get_memory_size").ok().map(|f| *f),
            }
        }
    }

    /// Screen rotation the core asked for: quarter turns counter-clockwise (0 = upright).
    pub fn rotation(&self) -> u32 {
        SHARED.with_borrow(|s| s.rotation)
    }

    /// Number of discs the core knows about (0 when it has no disc control).
    pub fn disc_count(&self) -> u32 {
        SHARED.with_borrow(|s| s.disk).map_or(0, |d| unsafe { (d.get_num_images)() })
    }

    /// The disc in the drive (0-based, as the core says it) and how many discs the core knows; None without disc
    /// control. The pause menu's Disc row (`pocket_ui::disc`).
    pub fn disc(&self) -> Option<(u32, u32)> {
        let d = SHARED.with_borrow(|s| s.disk)?;
        unsafe { Some(((d.get_image_index)(), (d.get_num_images)())) }
    }

    /// Swaps to the next disc of a multi-disc game (wraps around); returns the new 1-based number.
    pub fn next_disc(&self) -> Option<u32> {
        let (at, n) = self.disc()?;
        let next = (at + 1) % n.max(1);
        self.set_disc(next).then_some(next + 1)
    }

    /// Puts disc `index` (0-based) in, the way the disk control interface asks for it: tray open, the disc set, tray
    /// closed, in one go as `next_disc` always did. False when the game has one disc, the core has no disc control or
    /// refuses.
    pub fn set_disc(&self, index: u32) -> bool {
        let Some(d) = SHARED.with_borrow(|s| s.disk) else { return false };
        unsafe {
            if index >= (d.get_num_images)() || (d.get_num_images)() < 2 {
                return false;
            }
            (d.set_eject_state)(true);
            let ok = (d.set_image_index)(index);
            (d.set_eject_state)(false);
            ok
        }
    }

    /// Runs one emulated frame with the given joypad bits; returns the new frame (if any) and audio.
    pub fn run(&self, buttons: u16) -> (Option<Frame>, Vec<i16>, Option<AvInfo>, Option<Geometry>) {
        self.run_with(buttons, AV_ALL)
    }

    /// `run`, making only what `enable` asks for (AV_VIDEO, AV_AUDIO): the rest is dropped, and a core that asks
    /// (GET_AUDIO_VIDEO_ENABLE) can skip drawing or mixing it. For frames that are never shown or heard:
    /// run-ahead's (`run_ahead`) and fast-forward's.
    pub fn run_with(&self, buttons: u16, enable: u32) -> (Option<Frame>, Vec<i16>, Option<AvInfo>, Option<Geometry>) {
        let frame_time = SHARED.with_borrow_mut(|s| {
            s.buttons = buttons;
            s.av_skip = AV_ALL & !enable;
            s.frame_time
        });
        unsafe {
            // Outside the lock: the core may call back into the environment from it.
            if let Some(FrameTime { callback: Some(cb), reference }) = frame_time {
                cb(reference);
            }
            if let Ok(run) = self.lib.get::<VoidFn>(b"retro_run") {
                run();
            }
        }
        SHARED.with_borrow_mut(|s| {
            s.av_skip = 0;
            (s.frame.take(), std::mem::take(&mut s.audio), s.av_changed.take(), s.geometry_changed.take())
        })
    }

    /// What the core said about its states (SET_SERIALIZATION_QUIRKS, `QUIRK_*`).
    pub fn serialization_quirks(&self) -> u64 {
        SHARED.with_borrow(|s| s.serialization_quirks)
    }

    /// `save_state_into` for run-ahead: the state stays in memory and this core loads it again
    /// (`load_quick`), which the core hears as "fast savestates" (GET_AUDIO_VIDEO_ENABLE).
    ///
    /// The buffer keeps the size of the last state and is tried first: the size is asked for only when the core
    /// refuses it (the first time, or a state that grew). mGBA makes a whole state to tell its size, so asking every
    /// time cost as much as the save itself.
    pub fn save_quick(&self, buf: &mut Vec<u8>) -> bool {
        if self.no_states {
            return false;
        }
        SHARED.with_borrow_mut(|s| s.quick_state = true);
        let ok = unsafe {
            let size = self.lib.get::<unsafe extern "C" fn() -> usize>(b"retro_serialize_size");
            let save = self.lib.get::<unsafe extern "C" fn(*mut c_void, usize) -> bool>(b"retro_serialize");
            match (size, save) {
                (Ok(size), Ok(save)) => {
                    let mut ok = !buf.is_empty() && save(buf.as_mut_ptr() as *mut c_void, buf.len());
                    if !ok {
                        let t = std::time::Instant::now();
                        let n = size();
                        let (calls, ms) = self.size_queries.get();
                        self.size_queries.set((calls + 1, ms + t.elapsed().as_secs_f32() * 1000.0));
                        buf.resize(n, 0);
                        ok = n > 0 && save(buf.as_mut_ptr() as *mut c_void, buf.len());
                    }
                    ok
                }
                _ => false,
            }
        };
        SHARED.with_borrow_mut(|s| s.quick_state = false);
        ok
    }

    /// How often `save_quick` asked for the state size since the last call, and how long that took (ms).
    pub fn take_size_queries(&self) -> (u32, f32) {
        self.size_queries.take()
    }

    /// Loads a state `save_quick` took, without `load_state`'s size checks (it is this core's own, from a moment ago).
    pub fn load_quick(&self, data: &[u8]) -> bool {
        if self.no_states || data.is_empty() {
            return false;
        }
        unsafe {
            let Ok(load) = self.lib.get::<unsafe extern "C" fn(*const c_void, usize) -> bool>(b"retro_unserialize") else { return false };
            SHARED.with_borrow_mut(|s| s.quick_state = true);
            let ok = load(data.as_ptr() as *const c_void, data.len());
            SHARED.with_borrow_mut(|s| s.quick_state = false);
            ok
        }
    }
}

impl Drop for Core {
    fn drop(&mut self) {
        let _ = self.write_sram();
        unsafe {
            if let Ok(f) = self.lib.get::<VoidFn>(b"retro_unload_game") {
                f();
            }
            if let Ok(f) = self.lib.get::<VoidFn>(b"retro_deinit") {
                f();
            }
        }
    }
}

/// RETRO_DEVICE_ID_JOYPAD_* bit numbers.
pub mod joypad {
    pub const B: u16 = 0;
    pub const Y: u16 = 1;
    pub const SELECT: u16 = 2;
    pub const START: u16 = 3;
    pub const UP: u16 = 4;
    pub const DOWN: u16 = 5;
    pub const LEFT: u16 = 6;
    pub const RIGHT: u16 = 7;
    pub const A: u16 = 8;
    pub const X: u16 = 9;
    pub const L: u16 = 10;
    pub const R: u16 = 11;
    pub const L2: u16 = 12;
    pub const R2: u16 = 13;
    pub const L3: u16 = 14;
    pub const R3: u16 = 15;
}

impl Frame {
    /// The frame as packed 8-bit RGB, for previews.
    pub fn to_rgb(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity((self.width * self.height * 3) as usize);
        for y in 0..self.height as usize {
            let row = &self.data[y * self.pitch..];
            for x in 0..self.width as usize {
                // Widen before scaling: a 5-bit value times 255 doesn't fit in a u8.
                let c = |v: u16, bits: u32| ((v as u32 * 255) / ((1 << bits) - 1)) as u8;
                let (r, g, b) = match self.format {
                    PIXEL_XRGB8888 => (row[x * 4 + 2], row[x * 4 + 1], row[x * 4]),
                    PIXEL_RGB565 => {
                        let v = u16::from_le_bytes([row[x * 2], row[x * 2 + 1]]);
                        (c((v >> 11) & 0x1f, 5), c((v >> 5) & 0x3f, 6), c(v & 0x1f, 5))
                    }
                    _ => {
                        let v = u16::from_le_bytes([row[x * 2], row[x * 2 + 1]]);
                        (c((v >> 10) & 0x1f, 5), c((v >> 5) & 0x1f, 5), c(v & 0x1f, 5))
                    }
                };
                out.extend_from_slice(&[r, g, b]);
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What a core reads of the touch screen as a pointer, a light gun and a mouse (port 0 only).
    #[test]
    fn the_touch_screen_answers_pointer_light_gun_and_mouse() {
        SHARED.with_borrow_mut(|s| s.pointer = Pointer { pressed: true, x: -1000, y: 2000, dx: 7, dy: -3 });
        unsafe {
            assert_eq!(input_state(0, DEVICE_POINTER, 0, 0), -1000);
            assert_eq!(input_state(0, DEVICE_POINTER, 0, 1), 2000);
            assert_eq!(input_state(0, DEVICE_POINTER, 0, 2), 1, "pressed");
            assert_eq!(input_state(0, DEVICE_POINTER, 1, 2), 0, "no second finger");
            assert_eq!(input_state(0, DEVICE_LIGHTGUN, 0, 13), -1000);
            assert_eq!(input_state(0, DEVICE_LIGHTGUN, 0, 2), 1, "trigger");
            assert_eq!(input_state(0, DEVICE_LIGHTGUN, 0, 15), 0, "on the screen");
            assert_eq!(input_state(0, DEVICE_MOUSE, 0, 0), 7);
            assert_eq!(input_state(0, DEVICE_MOUSE, 0, 2), 1, "left button");
            assert_eq!(input_state(1, DEVICE_POINTER, 0, 0), 0, "port 1 has no touch screen");
        }
        SHARED.with_borrow_mut(|s| s.pointer = Pointer { pressed: false, x: -1000, y: 2000, dx: 0, dy: 0 });
        unsafe {
            assert_eq!(input_state(0, DEVICE_POINTER, 0, 2), 0);
            assert_eq!(input_state(0, DEVICE_LIGHTGUN, 0, 15), 1, "off the screen when not touched");
        }
        SHARED.with_borrow_mut(|s| s.pointer = Pointer::default());
    }
}

#[cfg(test)]
mod resume_tests {
    //! Instant resume against the battery save, with a stub core built from C here: its battery save
    //! (SRAM) is part of its save state, as in most real cores, and `retro_run` moves the "game" on.
    use super::*;
    use crate::testutil::TestDir;
    use std::sync::Arc;
    use std::time::{Duration, SystemTime};

    const STUB: &str = r#"
#include <stdbool.h>
#include <stddef.h>
#include <string.h>
struct info { const char *name, *version, *exts; bool need_fullpath, block_extract; };
struct av { unsigned bw, bh, mw, mh; float aspect; double fps, rate; };
static unsigned char sram[64];
static unsigned char pos[16];
/* The cartridge clock: none unless built with -DRTC_SIZE=n; -DRTC_PTR with size 0 hands out a pointer of size 0 (as
   mGBA does for a GBA game). */
#ifndef RTC_SIZE
#define RTC_SIZE 0
#endif
static unsigned char rtc[RTC_SIZE + 1];
void retro_set_environment(void *f) {}
void retro_set_video_refresh(void *f) {}
void retro_set_audio_sample(void *f) {}
void retro_set_audio_sample_batch(void *f) {}
void retro_set_input_poll(void *f) {}
void retro_set_input_state(void *f) {}
void retro_init(void) { memset(sram, 0, sizeof sram); memset(pos, 0, sizeof pos); }
void retro_deinit(void) {}
void retro_get_system_info(struct info *i) { memset(i, 0, sizeof *i); i->name = "stub"; i->exts = "gb"; i->need_fullpath = true; }
void retro_get_system_av_info(struct av *a) { memset(a, 0, sizeof *a); a->bw = a->mw = 160; a->bh = a->mh = 144; a->fps = 60; a->rate = 48000; }
bool retro_load_game(const void *g) { return true; }
void retro_unload_game(void) {}
void retro_reset(void) { memset(pos, 0, sizeof pos); }
void retro_run(void) { pos[0]++; }
void *retro_get_memory_data(unsigned id) {
    if (id == 0) return sram;
#if RTC_SIZE > 0 || defined(RTC_PTR)
    if (id == 1) return rtc;
#endif
    return NULL;
}
size_t retro_get_memory_size(unsigned id) { return id == 0 ? sizeof sram : id == 1 ? RTC_SIZE : 0; }
size_t retro_serialize_size(void) { return sizeof sram + sizeof pos; }
bool retro_serialize(void *d, size_t n) {
    if (n < sizeof sram + sizeof pos) return false;
    memcpy(d, sram, sizeof sram); memcpy((char *)d + sizeof sram, pos, sizeof pos); return true;
}
bool retro_unserialize(const void *d, size_t n) {
    if (n != sizeof sram + sizeof pos) return false;
    /* A state marked 0xEE in pos[1]: the battery save is restored, then the core gives up (SOL-02). */
    memcpy(sram, d, sizeof sram);
    if (((const unsigned char *)d)[sizeof sram + 1] == 0xEE) return false;
    memcpy(pos, (const char *)d + sizeof sram, sizeof pos); return true;
}
"#;

    /// `Core::load` resets `hw_render`'s globals, which its own test checks.
    fn globals() -> std::sync::MutexGuard<'static, ()> {
        crate::hw_render::TEST_GLOBALS.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// One game in its own folder, with its own copy of the stub core (a core's globals are per file).
    struct Game {
        dir: crate::testutil::TestDir,
        core: PathBuf,
        rom: PathBuf,
        state: PathBuf,
        save_ext: &'static str,
    }

    impl Game {
        fn new(name: &str, save_ext: &'static str) -> Game {
            Game::built(name, save_ext, &[])
        }

        /// With the stub core built with `defines` (`-DRTC_SIZE=8`: a cartridge clock).
        fn built(name: &str, save_ext: &'static str, defines: &[&str]) -> Game {
            let dir = TestDir::new(&format!("resume-{name}"));
            std::fs::write(dir.join("stub.c"), STUB).unwrap();
            let core = dir.join("stub_libretro.so");
            let out = std::process::Command::new("cc")
                .args(["-shared", "-fPIC"])
                .args(defines)
                .arg("-o")
                .arg(&core)
                .arg(dir.join("stub.c"))
                .output()
                .expect("the resume tests build a stub core and need a C compiler (cc)");
            assert!(out.status.success(), "cc: {}", String::from_utf8_lossy(&out.stderr));
            let rom = dir.join("Game.gb");
            let state = crate::slots::auto_path(&dir.join("Game"));
            Game { dir, core, rom, state, save_ext }
        }

        /// pocket-core's start: load the core and the game, read the battery save, resume.
        fn start(&self) -> (Core, Resume) {
            let mut core = Core::load(&self.core, &self.rom, &self.dir, &self.dir, &[]).unwrap();
            core.use_save_extension(self.save_ext);
            let r = core.resume(&self.state);
            (core, r)
        }

        /// pocket-core's clean exit: battery save, then the auto-resume state.
        fn quit(&self, mut core: Core) {
            core.write_sram().unwrap();
            std::fs::write(&self.state, core.save_state().unwrap()).unwrap();
            crate::slots::auto_written(&self.state);
        }

        /// pocket-core's quit without saving (`App::exit_without_saving`): battery save, no new auto-resume state.
        fn quit_without_saving(&self, mut core: Core) {
            core.write_sram().unwrap();
            core.keep_resume_point(&self.state);
        }

        fn taken(&self) -> PathBuf {
            crate::slots::taken_auto_path(&self.state)
        }

        fn retry(&self) -> PathBuf {
            crate::slots::retry_auto_path(&self.state)
        }

        fn battery_save(&self) -> PathBuf {
            self.dir.join(format!("Game.{}", self.save_ext))
        }

        /// The battery save on disk.
        fn saved(&self) -> String {
            String::from_utf8_lossy(&std::fs::read(self.battery_save()).unwrap()[..6]).into_owned()
        }
    }


    /// Moves a file's time `secs` into the past (the time that passes between sessions).
    fn age(path: &Path, secs: u64) {
        let f = std::fs::File::options().write(true).open(path).unwrap();
        f.set_modified(SystemTime::now() - Duration::from_secs(secs)).unwrap();
    }

    fn set_sram(core: &mut Core, data: &[u8]) {
        core.memory(MEMORY_SAVE_RAM).unwrap()[..data.len()].copy_from_slice(data);
    }

    /// The battery save the game sees.
    fn sram(core: &mut Core) -> String {
        String::from_utf8_lossy(&core.memory(MEMORY_SAVE_RAM).unwrap()[..6]).into_owned()
    }

    /// How far the stub game has run (frames since boot or the loaded state).
    fn position(core: &Core) -> u8 {
        core.save_state().unwrap()[64]
    }

    fn play(core: &Core, frames: u32) {
        for _ in 0..frames {
            core.run(0);
        }
    }

    /// Session 1 ends normally a minute ago: battery save A, and a resume state 5 frames in with A in it.
    fn first_session(g: &Game) {
        let (mut core, r) = g.start();
        assert_eq!(r, Resume::NoState);
        set_sram(&mut core, b"SAVE-A");
        play(&core, 5);
        g.quit(core);
        age(&g.state, 60);
        age(&g.battery_save(), 60);
    }

    /// The battery save on disk replaced by `text` (padded to the stub's 64 bytes), its time `secs` in the past.
    fn put_battery_save(g: &Game, text: &[u8], secs: u64) {
        let mut bytes = text.to_vec();
        bytes.resize(64, 0);
        std::fs::write(g.battery_save(), bytes).unwrap();
        age(&g.battery_save(), secs);
    }

    #[test]
    fn a_state_the_core_gives_up_on_halfway_does_not_bring_back_its_battery_save() {
        // SOL-02 A: the core restored the state's battery save (A) and then refused the rest. The newer one on disk
        // (B) must stay, on disk and in the game.
        let _l = globals();
        let g = Game::new("late-refusal", "srm");
        first_session(&g);
        let mut state = std::fs::read(&g.state).unwrap();
        state[64 + 1] = 0xEE;
        std::fs::write(&g.state, state).unwrap();
        age(&g.state, 60);
        put_battery_save(&g, b"SAVE-B", 10);
        let (mut core, r) = g.start();
        assert_eq!(r, Resume::Failed);
        assert_eq!((position(&core), sram(&mut core).as_str()), (0, "SAVE-B"), "a fresh start with the save on disk");
        core.write_sram().unwrap();
        assert_eq!(g.saved(), "SAVE-B", "the battery save on disk is not rolled back");
    }

    #[test]
    fn a_different_battery_save_wins_whatever_the_clock_says() {
        // SOL-02 B: a newer battery save (another device, a copy that kept its time, a clock set back) whose time is
        // not after the state's: the contents differ, so the game starts fresh with it instead of rolling it back.
        for (name, secs) in [("same-time", 60), ("older-time", 3600)] {
            let _l = globals();
            let g = Game::new(name, "srm");
            first_session(&g);
            put_battery_save(&g, b"SAVE-B", secs);
            let (mut core, r) = g.start();
            assert_eq!(r, Resume::SaveIsNewer, "{name}");
            assert_eq!((position(&core), sram(&mut core).as_str()), (0, "SAVE-B"), "{name}");
            core.write_sram().unwrap();
            assert_eq!(g.saved(), "SAVE-B", "{name}");
        }
    }

    #[test]
    fn a_longer_battery_save_is_compared_to_its_end() {
        // Muse B1 rest: only the common length was compared, so a save that differed only past the core's length
        // counted as the same.
        let _l = globals();
        let g = Game::new("longer", "srm");
        first_session(&g);
        let mut bytes = b"SAVE-A".to_vec();
        bytes.resize(64, 0);
        bytes.extend_from_slice(b"MORE SAVE DATA");
        std::fs::write(g.battery_save(), &bytes).unwrap();
        age(&g.battery_save(), 10);
        let (_core, r) = g.start();
        assert_eq!(r, Resume::SaveIsNewer);
        // Padding past the end (0x00 or 0xFF) is no difference.
        let g = Game::new("padded", "srm");
        first_session(&g);
        let mut bytes = std::fs::read(g.battery_save()).unwrap();
        bytes.extend_from_slice(&[0xff; 64]);
        std::fs::write(g.battery_save(), &bytes).unwrap();
        let (_core, r) = g.start();
        assert_eq!(r, Resume::Resumed);
    }

    #[test]
    fn quit_and_start_again_resumes() {
        let _l = globals();
        let g = Game::new("quit", "srm");
        first_session(&g);
        let (mut core, r) = g.start();
        assert_eq!(r, Resume::Resumed);
        assert_eq!((position(&core), sram(&mut core).as_str()), (5, "SAVE-A"));
        play(&core, 2);
        g.quit(core);
        // A battery save written after the state with the same contents (exits before this fix) still resumes.
        age(&g.state, 30);
        let (core, r) = g.start();
        assert_eq!(r, Resume::Resumed);
        assert_eq!(position(&core), 7);
        drop(core);
        assert_eq!(g.saved(), "SAVE-A");
    }

    #[test]
    fn a_crash_after_an_in_game_save_keeps_the_newer_save() {
        let _l = globals();
        let g = Game::new("crash", "srm");
        first_session(&g);
        // Session 2 resumes, the player saves in the game (B), the 10 s flush writes it, then a crash:
        // no exit save.
        let (mut core, r) = g.start();
        assert_eq!(r, Resume::Resumed);
        set_sram(&mut core, b"SAVE-B");
        core.write_sram().unwrap();
        drop(core);
        assert_eq!(g.saved(), "SAVE-B");
        // Session 3 must not bring back A, neither on disk (the 10 s flush) nor in the game.
        let (mut core, r) = g.start();
        core.write_sram().unwrap();
        assert_eq!(g.saved(), "SAVE-B", "the battery save on disk is rolled back to the one from before session 2");
        assert_eq!(sram(&mut core), "SAVE-B", "the game sees the save from before the crash");
        assert_eq!(r, Resume::SaveIsNewer, "the state session 2 took is older than the save");
        assert_eq!(position(&core), 0, "a fresh start");
    }

    fn set_rtc(core: &mut Core, data: &[u8]) {
        core.memory(MEMORY_RTC).unwrap()[..data.len()].copy_from_slice(data);
    }

    fn rtc_file(g: &Game) -> PathBuf {
        g.dir.join("Game.rtc")
    }

    #[test]
    fn the_cartridge_clock_comes_back_after_an_in_game_save_and_a_cold_start() {
        // Pokémon Crystal on Gambatte: the clock (8 bytes, `RETRO_MEMORY_RTC`) is no part of the battery save. In-game
        // save, quit, and a start without a resume state (the state deleted, or Quit without saving on a first run).
        let _l = globals();
        for ext in ["srm", "sav"] {
            let g = Game::built(&format!("rtc-{ext}"), ext, &["-DRTC_SIZE=8"]);
            let (mut core, _) = g.start();
            set_sram(&mut core, b"SAVE-A");
            set_rtc(&mut core, b"CLOCK-01");
            g.quit(core);
            assert_eq!(std::fs::read(rtc_file(&g)).unwrap(), b"CLOCK-01", "RetroArch's name, beside the .{ext}");
            std::fs::remove_file(&g.state).unwrap();
            // Read before the first frame, like the battery save.
            let mut core = Core::load(&g.core, &g.rom, &g.dir, &g.dir, &[]).unwrap();
            core.use_save_extension(ext);
            assert_eq!(core.memory(MEMORY_RTC).unwrap(), b"CLOCK-01");
            assert_eq!(sram(&mut core), "SAVE-A");
            // The 10 s flush writes a clock the game set, as it writes the battery save.
            set_rtc(&mut core, b"CLOCK-02");
            core.write_sram_in_background();
            core.saver.wait();
            assert_eq!(std::fs::read(rtc_file(&g)).unwrap(), b"CLOCK-02");
        }
    }

    #[test]
    fn no_clock_or_a_clock_of_size_0_leaves_no_rtc_file() {
        let _l = globals();
        for (name, defines) in [("rtc-none", &[][..]), ("rtc-zero", &["-DRTC_PTR"][..])] {
            let g = Game::built(name, "srm", defines);
            let (mut core, _) = g.start();
            assert!(core.memory(MEMORY_RTC).is_none(), "{name}");
            set_sram(&mut core, b"SAVE-A");
            core.write_sram_in_background();
            g.quit(core);
            assert_eq!(g.saved(), "SAVE-A");
            assert!(!rtc_file(&g).exists(), "{name}: no .rtc");
        }
    }

    /// A card that is full while `full` says so (ENOSPC, nothing written, the file there left as it was), and the
    /// card itself otherwise. No real card is filled.
    fn card(core: &mut Core, full: &Arc<std::sync::atomic::AtomicBool>) {
        let full = full.clone();
        core.saver = crate::save_writer::SaveWriter::with(move |path, bytes| {
            if full.load(std::sync::atomic::Ordering::SeqCst) {
                Err(std::io::Error::from_raw_os_error(libc::ENOSPC))
            } else {
                crate::fsutil::write_atomic(path, bytes)
            }
        });
    }

    #[test]
    fn a_full_card_is_reported_keeps_the_last_good_save_and_is_tried_again() {
        use crate::save_trouble::Trouble;
        use std::sync::atomic::{AtomicBool, Ordering};
        let _l = globals();
        let g = Game::built("full", "srm", &["-DRTC_SIZE=8"]);
        first_session(&g);
        let (mut core, _) = g.start();
        let full = Arc::new(AtomicBool::new(true));
        card(&mut core, &full);
        set_sram(&mut core, b"SAVE-B");
        core.write_sram_in_background();
        core.saver.wait();
        assert_eq!(core.save_trouble(), Some(Trouble::Full), "the flush learns it, without waiting");
        assert_eq!(g.saved(), "SAVE-A", "the last good save stays");
        // Quit and sleep: one try, an answer, no waiting for the card.
        assert_eq!(core.write_sram(), Err(Trouble::Full));
        // Space freed: the next flush writes the same save again, and the trouble is over.
        full.store(false, Ordering::SeqCst);
        core.write_sram_in_background();
        core.saver.wait();
        assert_eq!((core.save_trouble(), g.saved().as_str()), (None, "SAVE-B"));
        assert_eq!(core.write_sram(), Ok(()));
        // A read-only card: the other kind.
        core.saver = crate::save_writer::SaveWriter::with(|_, _| Err(std::io::Error::from_raw_os_error(libc::EROFS)));
        set_sram(&mut core, b"SAVE-C");
        assert_eq!(core.write_sram(), Err(Trouble::Unwritable));
        assert_eq!(g.saved(), "SAVE-B");
        core.saver = crate::save_writer::SaveWriter::new();
    }

    #[test]
    fn a_state_written_when_the_battery_save_was_not_wins_at_the_next_start() {
        // A failing card took the exit's battery save (B) but the auto state with B in it got through. Without the
        // note the next start would take the older save on the card (A) for a newer one and throw B away.
        let _l = globals();
        for noted in [true, false] {
            let g = Game::new(if noted { "unsaved" } else { "unsaved-unnoted" }, "srm");
            first_session(&g);
            let (mut core, _) = g.start();
            let full = Arc::new(std::sync::atomic::AtomicBool::new(true));
            card(&mut core, &full);
            set_sram(&mut core, b"SAVE-B");
            play(&core, 3);
            // pocket-core's `save_auto`.
            let battery = core.write_sram();
            assert!(battery.is_err());
            std::fs::write(&g.state, core.save_state().unwrap()).unwrap();
            crate::slots::auto_written(&g.state);
            if noted {
                crate::slots::battery_unsaved(&g.state, battery.is_err());
            }
            // The card is still failing when the process ends (`Drop` tries once more).
            drop(core);
            let (mut core, r) = g.start();
            if noted {
                assert_eq!((r, sram(&mut core).as_str()), (Resume::Resumed, "SAVE-B"));
                core.write_sram().unwrap();
                assert_eq!(g.saved(), "SAVE-B", "and on the SD card now");
            } else {
                assert_eq!((r, sram(&mut core).as_str()), (Resume::SaveIsNewer, "SAVE-A"), "what the note prevents");
            }
        }
    }

    #[test]
    fn a_battery_save_written_after_the_noted_state_still_wins() {
        // The note says the state's exit couldn't write the save; a save written later (Syncthing, RetroArch) is newer.
        let _l = globals();
        let g = Game::new("unsaved-later", "srm");
        first_session(&g);
        crate::slots::battery_unsaved(&g.state, true);
        put_battery_save(&g, b"SAVE-C", 10);
        let (mut core, r) = g.start();
        assert_eq!((r, sram(&mut core).as_str()), (Resume::SaveIsNewer, "SAVE-C"));
        assert!(!crate::slots::take_battery_unsaved(&g.state), "read once");
    }

    #[test]
    fn the_10_s_flush_reaches_the_disk_and_quit_waits_for_it() {
        let _l = globals();
        let g = Game::new("flush", "srm");
        first_session(&g);
        let (mut core, _) = g.start();
        set_sram(&mut core, b"SAVE-B");
        core.write_sram_in_background();
        set_sram(&mut core, b"SAVE-C");
        core.write_sram_in_background();
        core.saver.wait();
        assert_eq!(core.save_trouble(), None);
        assert_eq!(g.saved(), "SAVE-C");
        set_sram(&mut core, b"SAVE-D");
        core.write_sram_in_background();
        g.quit(core);
        assert_eq!(g.saved(), "SAVE-D");
    }

    #[test]
    fn a_crash_without_an_in_game_save_resumes_from_the_quit_point() {
        let _l = globals();
        let g = Game::new("lost", "srm");
        first_session(&g);
        // Session 2 resumes and plays for an hour without saving in the game, then the battery dies.
        let (core, r) = g.start();
        assert_eq!(r, Resume::Resumed);
        play(&core, 30);
        drop(core);
        // Nothing would be rolled back: session 3 goes back to where session 1 quit.
        let (mut core, r) = g.start();
        assert_eq!(r, Resume::ResumedAgain);
        assert_eq!((position(&core), sram(&mut core).as_str()), (5, "SAVE-A"), "the quit point of session 1");
        assert!(!g.taken().exists() && g.retry().exists());
    }

    #[test]
    fn a_state_that_crashes_the_game_is_tried_twice_only() {
        let _l = globals();
        let g = Game::new("twice", "srm");
        first_session(&g);
        // The state "crashes" the game right after loading: no exit save, three starts in a row.
        let (core, r) = g.start();
        assert_eq!(r, Resume::Resumed);
        drop(core);
        let (core, _) = g.start();
        assert_eq!(position(&core), 5, "the second start tries the state once more");
        drop(core);
        let (core, r) = g.start();
        assert_eq!((r, position(&core)), (Resume::NoState, 0), "the third start is fresh");
    }

    #[test]
    fn a_new_resume_state_clears_the_taken_ones() {
        let _l = globals();
        let g = Game::new("clear", "srm");
        first_session(&g);
        let (core, _) = g.start();
        std::fs::write(g.retry(), b"from an earlier crash").unwrap();
        assert!(g.taken().exists());
        g.quit(core);
        assert!(g.state.exists());
        assert!(!g.taken().exists() && !g.retry().exists(), "about 40 MB each for a PSP game");
    }

    #[test]
    fn a_crash_after_a_sleep_autosave_keeps_the_newer_save() {
        let _l = globals();
        let g = Game::new("sleep", "srm");
        first_session(&g);
        // Session 2 writes the state before a suspend (`autosave`), plays on after waking, saves in the
        // game, the 10 s flush writes it, then a crash. This state was written in session 2, so it is still there.
        let (mut core, _) = g.start();
        play(&core, 3);
        core.write_sram().unwrap();
        std::fs::write(&g.state, core.save_state().unwrap()).unwrap();
        age(&g.state, 30);
        set_sram(&mut core, b"SAVE-B");
        core.write_sram().unwrap();
        drop(core);
        let (mut core, r) = g.start();
        assert_eq!(r, Resume::SaveIsNewer);
        assert_eq!((position(&core), sram(&mut core).as_str()), (0, "SAVE-B"), "a fresh start with the newer save");
        core.write_sram().unwrap();
        assert_eq!(g.saved(), "SAVE-B");
    }

    #[test]
    fn quit_without_saving_keeps_the_newer_in_game_save() {
        let _l = globals();
        let g = Game::new("nosave-b1", "srm");
        first_session(&g);
        // Session 2 resumes from session 1's quit, the player saves in the game (B) and quits without saving.
        let (mut core, r) = g.start();
        assert_eq!(r, Resume::Resumed);
        play(&core, 3);
        set_sram(&mut core, b"SAVE-B");
        g.quit_without_saving(core);
        assert_eq!(g.saved(), "SAVE-B", "the battery save is written as on every exit");
        // Session 3: the older state from session 1 must not bring back A, on disk or in the game.
        let (mut core, r) = g.start();
        assert_eq!(r, Resume::SaveIsNewer, "the auto state is older than the in-game save of session 2");
        assert_eq!((position(&core), sram(&mut core).as_str()), (0, "SAVE-B"), "a fresh start with the newer save");
        core.write_sram().unwrap();
        assert_eq!(g.saved(), "SAVE-B", "the battery save on disk is rolled back");
        // Quitting without saving again drops the state that lost: it would lose (and say so) at every start.
        g.quit_without_saving(core);
        assert!(!g.state.exists() && !g.taken().exists() && !g.retry().exists());
        let (mut core, r) = g.start();
        assert_eq!((r, sram(&mut core).as_str()), (Resume::NoState, "SAVE-B"));
    }

    #[test]
    fn quit_without_saving_resumes_from_the_last_saving_quit() {
        let _l = globals();
        let g = Game::new("nosave", "srm");
        first_session(&g);
        // Two sessions in a row end without saving: both times the next start is where session 1 quit.
        for _ in 0..2 {
            let (mut core, r) = g.start();
            assert_eq!(r, Resume::Resumed);
            assert_eq!((position(&core), sram(&mut core).as_str()), (5, "SAVE-A"));
            play(&core, 20);
            g.quit_without_saving(core);
            assert!(g.state.exists() && !g.taken().exists() && !g.retry().exists());
            assert_eq!(g.saved(), "SAVE-A");
        }
        // A saving quit after that moves the resume point on.
        let (core, _) = g.start();
        play(&core, 2);
        g.quit(core);
        let (core, r) = g.start();
        assert_eq!((r, position(&core)), (Resume::Resumed, 7));
    }

    #[test]
    fn quit_without_saving_after_a_crash_keeps_the_state_tried_again() {
        let _l = globals();
        let g = Game::new("nosave-again", "srm");
        first_session(&g);
        let (core, _) = g.start();
        drop(core);
        let (core, r) = g.start();
        assert_eq!(r, Resume::ResumedAgain);
        g.quit_without_saving(core);
        // The state loaded fine this time: it is the resume point again, not a last try.
        let (core, r) = g.start();
        assert_eq!((r, position(&core)), (Resume::Resumed, 5));
    }

    #[test]
    fn quit_without_saving_after_a_sleep_keeps_the_sleep_state() {
        let _l = globals();
        let g = Game::new("nosave-sleep", "srm");
        first_session(&g);
        // Session 2 writes the auto state before a suspend (`autosave`), plays on and quits without saving.
        let (mut core, _) = g.start();
        play(&core, 3);
        core.write_sram().unwrap();
        std::fs::write(&g.state, core.save_state().unwrap()).unwrap();
        crate::slots::auto_written(&g.state);
        play(&core, 10);
        g.quit_without_saving(core);
        let (core, r) = g.start();
        assert_eq!((r, position(&core)), (Resume::Resumed, 8), "the state written before the sleep");
    }

    #[test]
    fn a_state_the_core_refuses_is_tried_once() {
        let _l = globals();
        let g = Game::new("broken", "srm");
        std::fs::write(&g.state, b"not a state").unwrap();
        assert_eq!(g.start().1, Resume::Failed);
        assert_eq!(g.start().1, Resume::NoState);
    }

    /// A DraStic save as copied from Android: `SAVE-D` at the start, 512 bytes, with DeSmuME's footer.
    fn dsv_file(footer: bool) -> Vec<u8> {
        let mut v = [b"SAVE-D".as_slice(), &[0; 506]].concat();
        if footer {
            v.extend_from_slice(b"|<--Snip above here to create a raw sav by excluding this DeSmuME savedata footer:");
            for f in [512u32, 512, 1, 1, 512, 0] {
                v.extend_from_slice(&f.to_le_bytes());
            }
            v.extend_from_slice(b"|-DESMUME SAVE-|");
        }
        v
    }

    #[test]
    fn a_dsv_next_to_the_game_becomes_the_sav_once_and_the_game_sees_it() {
        let _l = globals();
        for footer in [true, false] {
            let g = Game::new(&format!("dsv-{footer}"), "sav");
            let dsv = dsv_file(footer);
            std::fs::write(g.dir.join("Game.dsv"), &dsv).unwrap();
            let mut core = Core::load(&g.core, &g.rom, &g.dir, &g.dir, &[]).unwrap();
            assert!(core.use_save_extension("sav"), "footer {footer}: the toast is due");
            assert_eq!(sram(&mut core), "SAVE-D", "footer {footer}");
            assert_eq!(std::fs::read(g.battery_save()).unwrap(), &dsv[..512], "footer {footer}: the raw save");
            assert_eq!(std::fs::read(g.dir.join("Game.dsv")).unwrap(), dsv, "the .dsv stays");
            // The next start finds the .sav: no second toast, and what the player saved since is kept.
            std::fs::write(g.battery_save(), [b"SAVE-E".as_slice(), &[0; 506]].concat()).unwrap();
            let mut core = Core::load(&g.core, &g.rom, &g.dir, &g.dir, &[]).unwrap();
            assert!(!core.use_save_extension("sav"), "footer {footer}: once");
            assert_eq!(sram(&mut core), "SAVE-E", "footer {footer}");
        }
    }

    #[test]
    fn a_dsv_does_not_replace_a_srm_carried_over_or_count_for_other_systems() {
        let _l = globals();
        // A RetroArch `.srm` is the save already; a `.srm` start (every system but the DS) never looks at a `.dsv`.
        for ext in ["sav", "srm"] {
            let g = Game::new(&format!("dsv-srm-{ext}"), ext);
            std::fs::write(g.dir.join("Game.dsv"), dsv_file(true)).unwrap();
            std::fs::write(g.dir.join("Game.srm"), [b"SAVE-R".as_slice(), &[0; 58]].concat()).unwrap();
            let mut core = Core::load(&g.core, &g.rom, &g.dir, &g.dir, &[]).unwrap();
            assert!(!core.use_save_extension(ext), "{ext}");
            assert_eq!(sram(&mut core), "SAVE-R", "{ext}");
            assert!(ext == "srm" || !g.battery_save().exists(), "{ext}: nothing written by the take-over");
        }
    }

    #[test]
    fn a_dsv_of_a_wrong_size_leaves_the_game_without_a_save_and_says_nothing_on_screen() {
        let _l = globals();
        let g = Game::new("dsv-odd", "sav");
        std::fs::write(g.dir.join("Game.dsv"), b"not a save").unwrap();
        let mut core = Core::load(&g.core, &g.rom, &g.dir, &g.dir, &[]).unwrap();
        assert!(!core.use_save_extension("sav"));
        assert!(!g.battery_save().exists());
        assert_eq!(std::fs::read(g.dir.join("Game.dsv")).unwrap(), b"not a save");
    }

    #[test]
    fn a_save_written_by_another_emulator_wins_over_an_older_state() {
        // RetroArch (`.srm`) or standalone melonDS (`.sav`, `HostSystem::save_ext`) played the game in between.
        let _l = globals();
        for ext in ["srm", "sav"] {
            let g = Game::new(&format!("switch-{ext}"), ext);
            first_session(&g);
            std::fs::write(g.battery_save(), [b"SAVE-B".as_slice(), &[0; 58]].concat()).unwrap();
            let (mut core, r) = g.start();
            core.write_sram().unwrap();
            assert_eq!(g.saved(), "SAVE-B", "{ext}: the battery save on disk is rolled back");
            assert_eq!(r, Resume::SaveIsNewer, "{ext}");
            assert_eq!((position(&core), sram(&mut core).as_str()), (0, "SAVE-B"), "{ext}: a fresh start with the newer save");
        }
    }
}

#[cfg(test)]
mod ffi_tests {
    //! The host side of the libretro ABI against a stub core built from C here: what a core may do with
    //! the pointers and sizes it gets, and what it may hand us.
    use super::*;
    use crate::testutil::TestDir;

    const STUB: &str = r#"
#include <stdbool.h>
#include <stddef.h>
#include <string.h>
#include <sys/mman.h>
#include <unistd.h>
typedef bool (*env_t)(unsigned, void *);
typedef void (*video_t)(const void *, unsigned, unsigned, size_t);
struct info { const char *name, *version, *exts; bool need_fullpath, block_extract; };
struct av { unsigned bw, bh, mw, mh; float aspect; double fps, rate; };
struct var { const char *key, *value; };
static env_t env;
static video_t video;
static struct var vars[] = { { "stub_speed", "Speed; slow|fast|turbo" }, { NULL, NULL } };
void retro_set_environment(env_t f) { env = f; }
void retro_set_video_refresh(video_t f) { video = f; }
typedef size_t (*batch_t)(const short *, size_t);
static batch_t batch;
void retro_set_audio_sample(void *f) {}
void retro_set_audio_sample_batch(batch_t f) { batch = f; }
void retro_set_input_poll(void *f) {}
typedef short (*input_t)(unsigned, unsigned, unsigned, unsigned);
static input_t input;
void retro_set_input_state(input_t f) { input = f; }
/* Couch co-op: the ports the host plugged pads into, and what a port reads. */
static unsigned plugged[8] = { 99, 99, 99, 99, 99, 99, 99, 99 };
void retro_set_controller_port_device(unsigned port, unsigned device) { if (port < 8) plugged[port] = device; }
unsigned stub_plugged(unsigned port) { return plugged[port]; }
short stub_read(unsigned port, unsigned device, unsigned index, unsigned id) { return input(port, device, index, id); }
struct ctype { const char *desc; unsigned id; };
struct cinfo { const struct ctype *types; unsigned num; };
bool stub_controller_info(void) {
    static const struct ctype pads[2] = { { "RetroPad", 1 }, { "DualShock", 517 } };
    static const struct ctype plain[1] = { { "Gamepad", 1 } };
    static const struct cinfo info[4] = { { pads, 2 }, { pads, 2 }, { plain, 1 }, { NULL, 0 } };
    return env(35, (void *)info);
}
void retro_init(void) {}
void retro_deinit(void) {}
void retro_get_system_info(struct info *i) { memset(i, 0, sizeof *i); i->name = "stub"; i->exts = "gb"; i->need_fullpath = true; }
void retro_get_system_av_info(struct av *a) { memset(a, 0, sizeof *a); a->bw = a->mw = 160; a->bh = a->mh = 144; a->fps = 60; a->rate = 48000; }
bool retro_load_game(const void *g) { return env(16, vars); }
void retro_unload_game(void) {}
static unsigned char state[80];
/* GET_AUDIO_VIDEO_ENABLE as the core saw it during its last frame and its last serialize/unserialize (-1: not answered). */
static int av_run = -1, av_state = -1;
static int ask_av(void) { int f = -1; return env(47 | 0x10000, &f) ? f : -1; }
static unsigned char pixels[160 * 144 * 2];
void retro_run(void) {
    static const short tone[4] = { 1, 2, 3, 4 };
    av_run = ask_av();
    /* The game moves on one step a frame (in its state), and shows how far it is in its first pixel. */
    state[0]++;
    pixels[0] = state[0];
    /* Draws and plays whatever it is told, as a core that doesn't ask would. */
    video(pixels, 160, 144, 320);
    if (batch) batch(tone, 2);
}
int stub_av_run(void) { return av_run; }
int stub_av_state(void) { return av_state; }
static bool variable_size;
size_t retro_serialize_size(void) { return sizeof state; }
bool retro_serialize(void *d, size_t n) { av_state = ask_av(); if (n < sizeof state) return false; memcpy(d, state, sizeof state); return true; }
/* Trusts the size, as many cores do; only a core with states of varying size reads fewer bytes. */
bool retro_unserialize(const void *d, size_t n) { av_state = ask_av(); memcpy(state, d, variable_size && n < sizeof state ? n : sizeof state); return true; }
/* Test hooks. */
typedef void (*log_t)(unsigned, const char *, ...);
void stub_log(unsigned level, const char *long_text) {
    struct { log_t log; } cb;
    if (!env(27, &cb)) return;
    if (long_text) cb.log(level, "%s", long_text);
    else cb.log(level, "%s %d %.2f %d %d %d %ld %s|\n", "text", 1, 2.5, 3, 4, 5, 6L, "seventh");
}
void stub_variable_size(void) { unsigned long long quirks = 1 << 2; variable_size = true; env(44, &quirks); }
/* ENDIAN_DEPENDENT only: says nothing about the size. */
void stub_endian_quirk(void) { unsigned long long quirks = 1 << 5; env(44, &quirks); }
/* Reads its own length, as DOSBox Pure does, without telling the host. */
void stub_quietly_variable(void) { variable_size = true; }
const char *stub_get(const char *key) { struct var v = { key, NULL }; return env(15, &v) ? v.value : NULL; }
void stub_declare(void) { env(16, vars); }
bool stub_updated(void) { bool u = false; env(17, &u); return u; }
typedef void (*frame_time_t)(long long);
static long long last_usec = -1;
static void on_frame_time(long long usec) { last_usec = usec; }
bool stub_frame_time(void) { struct { frame_time_t cb; long long reference; } ft = { on_frame_time, 16667 }; return env(21, &ft); }
long long stub_last_usec(void) { return last_usec; }
/* RetroAchievements: work RAM through retro_get_memory_data, and a memory map lent for the call only. */
static unsigned char wram[0x800];
void *retro_get_memory_data(unsigned id) { return id == 2 ? wram : NULL; }
size_t retro_get_memory_size(unsigned id) { return id == 2 ? sizeof wram : 0; }
void *stub_wram(void) { return wram; }
struct desc { unsigned long long flags; void *ptr; size_t offset, start, select, disconnect, len; const char *addrspace; };
bool stub_memory_map(void) {
    struct desc d[2] = { { 0, wram, 0, 0, 0, 0, 0x400, "W" }, { 0, wram + 0x400, 0, 0x6000, 0, 0, 0x400, NULL } };
    struct { const struct desc *d; unsigned n; } map = { d, 2 };
    bool ok = env(36 | 0x10000, &map);
    memset(d, 0xee, sizeof d);
    return ok;
}
bool stub_supports_achievements(void) { bool yes = true; return env(42 | 0x10000, &yes); }
/* A frame whose last line is only as long as the picture, right before an unmapped page. */
void stub_frame(unsigned w, unsigned h, size_t pitch, unsigned bpp) {
    unsigned format = bpp == 4 ? 1 : 2;
    env(10, &format);
    size_t page = (size_t)sysconf(_SC_PAGESIZE), need = (h - 1) * pitch + w * bpp, span = (need + page - 1) / page * page;
    unsigned char *m = mmap(NULL, span + page, PROT_READ | PROT_WRITE, MAP_PRIVATE | MAP_ANON, -1, 0);
    mprotect(m + span, page, PROT_NONE);
    unsigned char *buf = m + span - need;
    for (size_t i = 0; i < need; i++) buf[i] = (unsigned char)i;
    video(buf, w, h, pitch);
    munmap(m, span + page);
}
"#;

    fn globals() -> std::sync::MutexGuard<'static, ()> {
        crate::hw_render::TEST_GLOBALS.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The stub core in a folder of its own (a core's globals are per file), removed afterwards.
    struct Stub {
        dir: crate::testutil::TestDir,
        core: PathBuf,
    }

    impl Stub {
        fn new(name: &str) -> Stub {
            let dir = TestDir::new(&format!("ffi-{name}"));
            std::fs::write(dir.join("stub.c"), STUB).unwrap();
            let core = dir.join("stub_libretro.so");
            let out = std::process::Command::new("cc")
                .args(["-shared", "-fPIC", "-o"])
                .arg(&core)
                .arg(dir.join("stub.c"))
                .output()
                .expect("the FFI tests build a stub core and need a C compiler (cc)");
            assert!(out.status.success(), "cc: {}", String::from_utf8_lossy(&out.stderr));
            Stub { dir, core }
        }

        fn load(&self) -> Core {
            Core::load(&self.core, &self.dir.join("Game.gb"), &self.dir, &self.dir, &[]).unwrap()
        }
    }


    /// One of the stub's test hooks.
    unsafe fn hook<T: Copy>(core: &Core, name: &str) -> T {
        *core.lib.get::<T>(name.as_bytes()).unwrap()
    }

    /// Allocates strings of the sizes involved, so freed memory gets reused.
    fn churn() -> Vec<CString> {
        (0..4096).map(|i| CString::new(["XXXX", "YYYYY", "ZZZZZZ"][i % 3]).unwrap()).collect()
    }

    /// B6: libretro lets a core keep the string GET_VARIABLE returned (many compare it with the next one, or
    /// read it later). A change from the pause menu, or the core declaring its options again, must not free it.
    #[test]
    fn option_values_handed_to_the_core_stay_valid() {
        let _l = globals();
        let stub = Stub::new("options");
        let core = stub.load();
        unsafe {
            let get: unsafe extern "C" fn(*const c_char) -> *const c_char = hook(&core, "stub_get");
            let declare: unsafe extern "C" fn() = hook(&core, "stub_declare");
            let key = c"stub_speed".as_ptr();
            let first = get(key);
            assert_eq!(CStr::from_ptr(first), c"slow");
            assert!(core.set_option("stub_speed", "fast"), "the pause menu changes the option");
            let second = get(key);
            assert_eq!(CStr::from_ptr(second), c"fast");
            declare();
            let third = get(key);
            let _reuse = churn();
            assert_eq!(CStr::from_ptr(first), c"slow", "the value read before the menu changed it");
            assert_eq!(CStr::from_ptr(second), c"fast", "the value read before SET_VARIABLES came again");
            assert_eq!(CStr::from_ptr(third), c"fast");
        }
    }

    /// ScummVM reads its settings again when GET_VARIABLE_UPDATE says so, and on its first frame that touches a
    /// system object that does not exist yet (a crash). As in RetroArch, the options the core read while loading
    /// are no update; a change after that is.
    #[test]
    fn no_option_update_right_after_load() {
        let _l = globals();
        let stub = Stub::new("update");
        let core = stub.load();
        unsafe {
            let updated: unsafe extern "C" fn() -> bool = hook(&core, "stub_updated");
            assert!(!updated(), "declared and read during load: nothing new on the first frame");
            assert!(core.set_option("stub_speed", "fast"));
            assert!(updated(), "the pause menu changed it");
            assert!(!updated());
        }
    }

    /// TIC-80's states hold only its 1 KB of persistent memory, not the game (a load changes nothing on screen):
    /// a system can switch them off, and then nothing saves, loads or rewinds.
    #[test]
    fn states_can_be_switched_off() {
        let _l = globals();
        let stub = Stub::new("nostates");
        let mut core = stub.load();
        assert!(core.has_states());
        let state = core.save_state().expect("the stub has states");
        core.disable_states();
        assert!(!core.has_states());
        assert!(core.save_state().is_none());
        assert!(!core.save_state_into(&mut Vec::new()));
        assert!(core.load_state(&state).is_err());
    }

    /// Couch co-op: players 2 to 4 read their own pad and sticks from ports 1 to 3; the ports the core lists get a pad
    /// plugged in (the analog one where player 1 has one), the others are left alone.
    #[test]
    fn players_two_to_four_have_their_own_ports() {
        let _l = globals();
        let stub = Stub::new("players");
        let core = stub.load();
        unsafe {
            let info: unsafe extern "C" fn() -> bool = hook(&core, "stub_controller_info");
            let plugged: unsafe extern "C" fn(c_uint) -> c_uint = hook(&core, "stub_plugged");
            let read: unsafe extern "C" fn(c_uint, c_uint, c_uint, c_uint) -> i16 = hook(&core, "stub_read");
            assert!(info());
            assert_eq!(core.plug_other_players(true), ["player 2: DualShock (device 517)", "player 3: Gamepad (device 1)"]);
            assert_eq!((plugged(1), plugged(2), plugged(3)), (517, 1, 99), "port 4 isn't listed: left as the core has it");
            assert_eq!(plugged(0), 99, "player 1's port is `use_analog_pad`'s");
            core.plug_other_players(false);
            assert_eq!(plugged(1), 1, "no analog pad for a system without sticks");
            core.set_player(1, (1 << joypad::A) | (1 << joypad::UP), [100, -200, 300, -400]);
            core.set_player(3, 1 << joypad::START, [0; 4]);
            core.run(1 << joypad::B);
            assert_eq!(read(1, DEVICE_JOYPAD, 0, joypad::A as c_uint), 1);
            assert_eq!(read(1, DEVICE_JOYPAD, 0, joypad::B as c_uint), 0, "player 1's B stays player 1's");
            assert_eq!(read(1, DEVICE_JOYPAD, 0, JOYPAD_MASK), ((1 << joypad::A) | (1 << joypad::UP)) as i16);
            assert_eq!(read(1, DEVICE_ANALOG | (1 << 8), 1, 1), -400, "a DualShock reads the right stick's y");
            assert_eq!(read(1, DEVICE_JOYPAD, 0, 40), 0, "an id past the buttons");
            assert_eq!(read(3, DEVICE_JOYPAD, 0, joypad::START as c_uint), 1);
            assert_eq!(read(2, DEVICE_JOYPAD, 0, JOYPAD_MASK), 0);
            assert_eq!(read(4, DEVICE_JOYPAD, 0, JOYPAD_MASK), 0, "no fifth player");
            assert_eq!(read(0, DEVICE_JOYPAD, 0, JOYPAD_MASK), 1 << joypad::B);
            assert_eq!(read(1, DEVICE_POINTER, 0, 2), 0, "the touch screen is player 1's");
            core.set_player(0, 0xffff, [0; 4]);
            core.set_player(7, 0xffff, [0; 4]);
            assert_eq!(read(0, DEVICE_JOYPAD, 0, JOYPAD_MASK), 1 << joypad::B, "player 1 only through run");
            core.set_player(1, 0, [0; 4]);
            core.set_player(3, 0, [0; 4]);
        }
    }

    /// TIC-80 refuses to load without RETRO_ENVIRONMENT_SET_FRAME_TIME_CALLBACK; it gets the frame time before every frame.
    #[test]
    fn frame_time_callback_hears_of_every_frame() {
        let _l = globals();
        let stub = Stub::new("frametime");
        let core = stub.load();
        unsafe {
            let register: unsafe extern "C" fn() -> bool = hook(&core, "stub_frame_time");
            let last: unsafe extern "C" fn() -> i64 = hook(&core, "stub_last_usec");
            assert!(register(), "the host takes the callback");
            assert_eq!(last(), -1);
            core.run(0);
            assert_eq!(last(), 16667, "the core's own reference frame time");
        }
    }

    /// B13: the last line of a frame only has to be as long as the picture, and cores may end their buffer
    /// there. Before an unmapped page, so reading further crashes.
    #[test]
    fn a_frame_is_read_only_as_far_as_the_picture_goes() {
        let _l = globals();
        let stub = Stub::new("frame");
        let core = stub.load();
        for bpp in [2, 4] {
            unsafe {
                let frame: unsafe extern "C" fn(c_uint, c_uint, usize, c_uint) = hook(&core, "stub_frame");
                frame(160, 144, 1024, bpp);
            }
            let f = SHARED.with_borrow_mut(|s| s.frame.take()).expect("a frame");
            assert_eq!((f.width, f.height, f.pitch), (160, 144, 1024));
            let last = 143 * 1024 + 160 * bpp as usize - 1;
            assert_eq!((f.data[last], f.data[5 * 1024 + 7]), ((last % 256) as u8, 7), "{bpp} bytes per pixel");
            assert!(f.data.len() >= 144 * 1024, "whole lines for the readers");
            assert_eq!(f.to_rgb().len(), 160 * 144 * 3);
        }
    }

    /// B14: a NULL `data` for a command whose data we read or write (a broken core) is refused, not a crash.
    #[test]
    fn null_data_is_refused() {
        let _l = globals();
        let stub = Stub::new("null");
        let core = stub.load();
        // The 17 commands that crashed in the analysis, then the others that read or write `data`.
        let crashed = [1, 6, 9, 10, 14, 15, 24, 27, 30, 31, 32, 37, 54, 55, 68, 69, 45 | 0x10000];
        let others = [3, 16, 53, 56, 67, 3 | 0x800000];
        unsafe {
            for cmd in crashed.into_iter().chain(others) {
                assert!(!environment(cmd, std::ptr::null_mut()), "command {cmd} ({cmd:#x})");
            }
            let mut var: Variable = std::mem::zeroed();
            let no_key = &mut var as *mut Variable as *mut c_void;
            assert!(!environment(ENV_GET_VARIABLE, no_key), "GET_VARIABLE without a key");
            // Where NULL has a meaning, it keeps it.
            assert!(environment(ENV_SET_VARIABLE, std::ptr::null_mut()), "asks whether SET_VARIABLE is supported");
            assert!(environment(ENV_GET_INPUT_BITMASKS, std::ptr::null_mut()), "cores ask for bitmasks with NULL");
            assert!(environment(ENV_SET_DISK_CONTROL_INTERFACE, std::ptr::null_mut()), "no disc control");
        }
        let speed = core.option_value("stub_speed");
        assert_eq!(speed.as_deref(), Some("slow"), "a NULL SET_VARIABLES leaves the options alone");
    }

    /// The pause menu's Disc row: a core with three discs is asked to open the tray, set the disc and close it again,
    /// in that order; a disc past the last, a one-disc game and a core without disc control are refused.
    #[test]
    fn a_disc_is_put_in_through_the_cores_disc_control() {
        use std::sync::atomic::{AtomicU32, Ordering::SeqCst};
        static AT: AtomicU32 = AtomicU32::new(0);
        static COUNT: AtomicU32 = AtomicU32::new(3);
        static TRAY: Mutex<Vec<String>> = Mutex::new(Vec::new());
        unsafe extern "C" fn eject(open: bool) -> bool {
            TRAY.lock().unwrap().push(if open { "open" } else { "close" }.into());
            true
        }
        unsafe extern "C" fn ejected() -> bool {
            false
        }
        unsafe extern "C" fn index() -> c_uint {
            AT.load(SeqCst)
        }
        unsafe extern "C" fn set(i: c_uint) -> bool {
            TRAY.lock().unwrap().push(format!("disc {i}"));
            AT.store(i, SeqCst);
            true
        }
        unsafe extern "C" fn count() -> c_uint {
            COUNT.load(SeqCst)
        }
        let _l = globals();
        let stub = Stub::new("disc");
        let core = stub.load();
        assert_eq!(core.disc(), None, "no disc control: no row");
        assert!(!core.set_disc(0));
        let mut dc = DiskControl { set_eject_state: eject, get_eject_state: ejected, get_image_index: index, set_image_index: set, get_num_images: count };
        unsafe { assert!(environment(ENV_SET_DISK_CONTROL_INTERFACE, &mut dc as *mut DiskControl as *mut c_void)) };
        assert_eq!(core.disc(), Some((0, 3)));
        assert!(core.set_disc(2));
        assert_eq!(*TRAY.lock().unwrap(), ["open", "disc 2", "close"]);
        assert_eq!(core.disc(), Some((2, 3)));
        assert_eq!(core.next_disc(), Some(1), "disc next wraps around to the first");
        assert!(!core.set_disc(3), "there is no fourth disc");
        COUNT.store(1, SeqCst);
        assert!(!core.set_disc(0), "a game with one disc has nothing to change");
        assert_eq!(core.next_disc(), None);
        unsafe { assert!(environment(ENV_SET_DISK_CONTROL_INTERFACE, std::ptr::null_mut())) };
    }

    /// B15: a state shorter than the core's state size (an empty or cut-off file, another core version) is
    /// refused: a core that trusts the size would read past its end.
    #[test]
    fn a_state_of_the_wrong_size_is_refused() {
        let _l = globals();
        let stub = Stub::new("state");
        let core = stub.load();
        let state = core.save_state().unwrap();
        assert_eq!(state.len(), 80);
        assert_eq!(core.load_state(&[]), Err("the file is empty".into()));
        assert_eq!(core.load_state(&state[..40]), Err("wrong size (40 of 80 bytes)".into()), "a cut-off one");
        assert_eq!(core.load_state(&state), Ok(()));
        assert_eq!(core.load_state(&[state.as_slice(), &[0; 8]].concat()), Ok(()), "longer: for the core to judge");
        // A core whose states vary in size (SET_SERIALIZATION_QUIRKS) checks them itself.
        unsafe { hook::<unsafe extern "C" fn()>(&core, "stub_variable_size")() };
        assert_eq!(core.load_state(&state[..40]), Ok(()));
        assert!(core.load_state(&[]).is_err(), "still nothing to load");
    }

    /// DOSBox Pure's state size grows while a game runs (a state of 364903 bytes against a size of 391727 a little
    /// later) without it saying so (SET_SERIALIZATION_QUIRKS); it reads its own length. Such a system says it.
    #[test]
    fn a_core_can_be_known_to_vary_in_state_size() {
        let _l = globals();
        let stub = Stub::new("varies");
        let core = stub.load();
        let state = core.save_state().unwrap();
        unsafe { hook::<unsafe extern "C" fn()>(&core, "stub_quietly_variable")() };
        assert!(core.load_state(&state[..40]).is_err(), "the host doesn't know yet");
        core.states_vary_in_size();
        assert_eq!(core.load_state(&state[..40]), Ok(()), "shorter states go to the core");
        assert!(core.load_state(&[]).is_err(), "still nothing to load");
    }

    /// GET_AUDIO_VIDEO_ENABLE: a frame run for nobody (run-ahead, fast-forward) says so, and what the core makes
    /// anyway is dropped; the next ordinary frame has both again. Run-ahead's states are "fast savestates".
    #[test]
    fn frames_for_nobody_are_dropped_and_the_core_is_told() {
        let _l = globals();
        let stub = Stub::new("avenable");
        let core = stub.load();
        let (av_run, av_state): (unsafe extern "C" fn() -> i32, unsafe extern "C" fn() -> i32) = unsafe { (hook(&core, "stub_av_run"), hook(&core, "stub_av_state")) };
        let (frame, samples, ..) = core.run(0);
        assert!(frame.is_some() && samples.len() == 4, "an ordinary frame");
        assert_eq!(unsafe { av_run() }, 3, "video and audio");
        let (frame, samples, ..) = core.run_with(0, 0);
        assert!(frame.is_none() && samples.is_empty(), "nothing kept");
        assert_eq!(unsafe { av_run() }, 0);
        let (frame, samples, ..) = core.run_with(0, AV_VIDEO);
        assert!(frame.is_some() && samples.is_empty());
        assert_eq!(unsafe { av_run() }, 1);
        let (frame, samples, ..) = core.run(0);
        assert!(frame.is_some() && samples.len() == 4, "back to normal");
        // States: ordinary ones (slots, rewind) are not quick; run-ahead's are, both ways.
        let mut buf = Vec::new();
        assert!(core.save_state_into(&mut buf));
        assert_eq!(unsafe { av_state() }, 3);
        assert!(core.save_quick(&mut buf));
        assert_eq!(unsafe { av_state() }, 3 | 4, "fast savestates while saving");
        assert!(core.load_quick(&buf));
        assert_eq!(unsafe { av_state() }, 3 | 4, "and while loading");
        assert!(core.load_state(&buf).is_ok());
        assert_eq!(unsafe { av_state() }, 3);
        // NULL asks whether we answer it.
        assert!(unsafe { environment(ENV_GET_AUDIO_VIDEO_ENABLE, std::ptr::null_mut()) });
    }

    /// Run-ahead (`run_ahead`) through the real host side: the picture shown is two frames ahead, the game
    /// itself (its state) is where the real frames left it, one frame of sound per refresh, and the pictures
    /// ahead match the real frames that come later.
    #[test]
    fn run_ahead_shows_frames_ahead_and_leaves_the_game_where_it_was() {
        use crate::run_ahead::RunAhead;
        let _l = globals();
        let stub = Stub::new("runahead");
        let mut core = stub.load();
        let mut ra = RunAhead::new();
        ra.set_core(&stub.core, "gbc");
        ra.configure(2);
        ra.admit(core.has_states(), false, core.serialization_quirks());
        for f in 1..=20u8 {
            let (mut shown, mut heard, mut real) = (None, 0, 0);
            let ran = ra.step(&mut core, 1, 0, 0, 16.7, |(frame, samples, ..), keep| {
                if keep.video {
                    shown = frame.map(|f| f.data[0]);
                }
                if keep.audio {
                    heard += samples.len();
                }
                real += keep.real as u32;
            });
            assert!(ran);
            assert_eq!((shown, heard, real), (Some(f + 2), 4, 1), "refresh {f}");
            assert_eq!(core.save_state().unwrap()[0], f, "the game is at frame {f}");
        }
        assert_eq!((ra.same, ra.different), (4, 0), "every 4th frame compared");
        // The state size was asked for once: later saves reuse the buffer (`save_quick`).
        assert_eq!(ra.stats.size_calls, 1);
        assert_eq!(ra.state()[0], 20, "the state rewind keeps is the real game's");
    }

    /// SET_SERIALIZATION_QUIRKS by libretro.h's numbers: CORE_VARIABLE_SIZE is bit 2; bit 5 (ENDIAN_DEPENDENT) says
    /// nothing about sizes, so a short state is still refused.
    #[test]
    fn quirks_are_read_by_their_real_bits() {
        let _l = globals();
        let stub = Stub::new("quirks");
        let core = stub.load();
        let state = core.save_state().unwrap();
        unsafe { hook::<unsafe extern "C" fn()>(&core, "stub_endian_quirk")() };
        assert_eq!(core.serialization_quirks(), 1 << 5);
        assert!(core.load_state(&state[..40]).is_err(), "endian-dependent is no variable size");
        unsafe { hook::<unsafe extern "C" fn()>(&core, "stub_variable_size")() };
        assert_eq!(core.serialization_quirks(), QUIRK_CORE_VARIABLE_SIZE);
        assert_eq!(core.load_state(&state[..40]), Ok(()));
    }

    /// B16: the core's log lines are printf-style with any arguments (strings, doubles, more than six); a
    /// warning or error ends up in `last_error`, which the player may see.
    /// RetroAchievements reads memory through the map a core lends during SET_MEMORY_MAPS: kept as a copy, and the
    /// memory functions come along.
    #[test]
    fn a_memory_map_is_copied_and_achievements_are_supported() {
        let _g = globals();
        let stub = Stub::new("memmap");
        let core = stub.load();
        assert!(core.achievement_memory().descriptors.is_empty(), "no map yet");
        unsafe {
            assert!(hook::<unsafe extern "C" fn() -> bool>(&core, "stub_supports_achievements")());
            assert!(hook::<unsafe extern "C" fn() -> bool>(&core, "stub_memory_map")());
        }
        let mem = core.achievement_memory();
        let wram = unsafe { hook::<unsafe extern "C" fn() -> *mut c_void>(&core, "stub_wram")() } as usize;
        assert_eq!(mem.descriptors.len(), 2);
        assert_eq!((mem.descriptors[0].ptr, mem.descriptors[0].len, mem.descriptors[0].start), (wram, 0x400, 0));
        assert_eq!((mem.descriptors[1].ptr, mem.descriptors[1].start, mem.descriptors[1].addrspace), (wram + 0x400, 0x6000, 0));
        assert_eq!(unsafe { CStr::from_ptr(mem.descriptors[0].addrspace as *const c_char) }.to_str(), Ok("W"));
        let (data, size) = (mem.data.expect("retro_get_memory_data"), mem.size.expect("retro_get_memory_size"));
        assert_eq!(unsafe { (data(2) as usize, size(2), size(0)) }, (wram, 0x800, 0));
        drop(core);
        assert!(Stub::new("memmap2").load().achievement_memory().descriptors.is_empty(), "a new game starts without the old map");
    }

    #[test]
    fn log_lines_are_formatted_with_all_their_arguments() {
        let _l = globals();
        let stub = Stub::new("log");
        let _core = stub.load();
        let last = || SHARED.with_borrow(|s| s.last_error.clone());
        unsafe {
            let log: unsafe extern "C" fn(c_uint, *const c_char) = hook(&_core, "stub_log");
            log(3, std::ptr::null());
            assert_eq!(last(), "text 1 2.50 3 4 5 6 seventh|");
            let long = CString::new("x".repeat(2000)).unwrap();
            log(2, long.as_ptr());
            assert_eq!(last(), "x".repeat(511), "cut to the buffer");
            log(1, c"just info".as_ptr());
            assert_eq!(last(), "x".repeat(511), "info lines are no errors");
        }
    }
}
