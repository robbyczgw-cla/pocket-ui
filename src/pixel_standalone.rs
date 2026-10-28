//! Pixel Path for the standalone emulators (docs/CURATOR.md, "Pixel Path for PS2, GameCube, Wii and 3DS"): ARMSX2,
//! Dolphin and Azahar draw the picture and play the sound themselves and tell pocket-ui nothing while they run, so
//! their chain is read from their own settings files: the values the game was started with, or that Options set
//! since (they write the same files).
//!
//! - [`read`]: the running game's files → a [`Setup`], or why nothing could be read; then the pause menu doesn't list
//!   Pixel Path for this game. Runs on a thread, never on the UI thread: a while after the game started
//!   ([`prefetch`]) and each time the pause menu opens ([`refresh`]); the menu only looks at the last result
//!   ([`cached`]).
//! - [`path`]: a `Setup` → the seven stages. A stage is graded only where its settings alone decide it (a stretched
//!   picture, a resolution above or below the console's, an effect added, unfiltered textures); the others show
//!   their values without a grade, and the header says "Partly graded". Display and Audio add what pocket-ui knows
//!   itself (the panel, PipeWire); Latency isn't measured here and says so.
//!
//! Which file wins (per-game overrides first):
//! - ARMSX2: its own per-game INI (`gamesettings/<serial>_<CRC>.ini`, set in its menu), then PCSX2.ini, which
//!   start_armsx2.sh rewrites from system.cfg (the game's, then the platform's values) at every start and PINE
//!   changes from Options are saved to.
//! - Dolphin: the game's INI (`GameSettings/<GAMEID>.ini`, where Options keeps its values), then GFX.ini and
//!   Dolphin.ini, which the start scripts rewrite from system.cfg at every start.
//! - Azahar: qt-config.ini only: start_azahar.sh and pocket-ui write the game's values into it before every start,
//!   and Azahar changes nothing while it runs. A key with `\default=true` is Azahar's built-in default.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use crate::emu_settings::{self, ini_get, Files};
use crate::pixel_path::{self, Grade, Kind, Stage};
use crate::standalone::Emu;

const GFX_INI: &str = "/storage/.config/dolphin-emu/GFX.ini";
const DOLPHIN_INI: &str = "/storage/.config/dolphin-emu/Dolphin.ini";

/// How long after the start the first reading is taken: the start script has rewritten the INIs by then.
const PREFETCH_AFTER: Duration = Duration::from_secs(10);

/// The internal resolution as set.
#[derive(Clone, Debug, PartialEq)]
pub enum Resolution {
    /// A multiple of the console's own (Dolphin's 2 is 1× on ROCKNIX, see `dolphin`).
    Times(f32),
    /// Follows the window's size (Dolphin's and Azahar's 0).
    Window,
}

/// The aspect ratio setting.
#[derive(Clone, Debug, PartialEq)]
pub enum Aspect {
    /// The game's own shape; the words say how the emulator decides it.
    Auto(&'static str),
    /// A fixed shape, "4:3", "16:9".
    Fixed(String),
    /// Stretched to the window.
    Stretch,
    /// A shape set by hand ("Custom").
    Custom(String),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TextureKind {
    /// Filtered as the game asks.
    AsGame,
    /// Never filtered, though the game asks for it.
    Nearest,
    /// Filtered or upscaled beyond what the game asks.
    Added,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Textures {
    pub label: String,
    pub kind: TextureKind,
}

/// The emulator's own sound settings.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SetupAudio {
    /// "Cubeb", "Pulse", "SDL2"; None: not set (the emulator picks).
    pub backend: Option<String>,
    pub buffer_ms: Option<u32>,
    pub latency_ms: Option<u32>,
    pub stretch: Option<bool>,
}

/// What the emulator's settings files say about the running game's picture and sound.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Setup {
    /// "ARMSX2", "Dolphin", "Azahar".
    pub emulator: String,
    pub system: String,
    /// The files the values came from, as the page names them ("PCSX2.ini", "GameSettings/GM4E01.ini").
    pub sources: Vec<String>,
    pub renderer: Option<String>,
    pub resolution: Option<Resolution>,
    pub aspect: Option<Aspect>,
    /// ARMSX2's widescreen patches, Dolphin's widescreen hack: the game draws more to the sides.
    pub widescreen: Option<bool>,
    /// How the finished picture is scaled to the window: "smooth", "sharp bilinear", "no smoothing".
    pub output_filter: Option<String>,
    pub integer_scaling: Option<bool>,
    /// 3DS: how the two screens share the panel, and whether it is one of the layouts that keep both screens' shape.
    pub layout: Option<(String, bool)>,
    pub textures: Option<Textures>,
    /// What is added to the picture: "FXAA", "4× MSAA", "Lottes CRT", …
    pub effects: Vec<String>,
    /// A look setting was found in the files, so "nothing added" is what they say, not a guess.
    pub look_read: bool,
    pub vsync: Option<bool>,
    /// The frame limiter's target: 1.0 the console's own speed, 0 unlimited.
    pub speed: Option<f32>,
    /// Dolphin's VBI skip (frames skipped when the game runs slow).
    pub frame_skip: Option<bool>,
    pub audio: SetupAudio,
}

impl Setup {
    /// Nothing at all was found: the menu leaves Pixel Path out rather than show seven empty stages.
    fn is_empty(&self) -> bool {
        self.renderer.is_none()
            && self.resolution.is_none()
            && self.aspect.is_none()
            && self.widescreen.is_none()
            && self.output_filter.is_none()
            && self.layout.is_none()
            && self.textures.is_none()
            && self.effects.is_empty()
            && !self.look_read
            && self.vsync.is_none()
            && self.speed.is_none()
            && self.frame_skip.is_none()
            && self.audio == SetupAudio::default()
    }
}

/// The emulator's name as players know it.
pub fn emulator_name(emu: Emu) -> &'static str {
    match emu {
        Emu::Armsx2 => "ARMSX2",
        Emu::Dolphin => "Dolphin",
        Emu::Azahar => "Azahar",
    }
}

/// A settings file as text: None when it isn't there; Err when it is there but not text (garbled).
fn text(path: &Path) -> Result<Option<String>, String> {
    match std::fs::read(path) {
        Ok(bytes) => String::from_utf8(bytes)
            .map(Some)
            .map_err(|_| format!("{} is not a text file", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}

/// The settings of a game on `emu`. Err: why nothing could be read (for the log; the menu then hides Pixel Path).
/// `disc` is ARMSX2's "<serial> (<CRC>)" once the game runs (`standalone::Running::disc`).
pub fn read(emu: Emu, system: &str, rom: &Path, disc: Option<&str>, files: &Files) -> Result<Setup, String> {
    let mut s = match emu {
        Emu::Armsx2 => armsx2(disc, files)?,
        Emu::Dolphin => dolphin(rom, files)?,
        Emu::Azahar => azahar(files)?,
    };
    s.emulator = emulator_name(emu).to_string();
    s.system = system.to_string();
    Ok(s)
}

/// The main settings file, which has to be there and be text.
fn main_file(files: &Files, abs: &str) -> Result<String, String> {
    let path = files.at(abs);
    text(&path)?.ok_or_else(|| format!("{} is not there", path.display()))
}

/// An optional per-game file: missing or garbled is the same as not set.
fn game_file(files: &Files, abs: &str) -> Option<String> {
    let path = files.at(abs);
    match text(&path) {
        Ok(t) => t,
        Err(why) => {
            println!("pixel path: {why}; the game's own settings are left out");
            None
        }
    }
}

fn flag(v: &str) -> Option<bool> {
    match v.trim().to_ascii_lowercase().as_str() {
        "true" | "1" => Some(true),
        "false" | "0" => Some(false),
        _ => None,
    }
}

/// A number as Dolphin and Qt may write it: "4", "0x00000004", "1.000000".
fn number(v: &str) -> Option<f32> {
    let v = v.trim().trim_matches('"');
    match v.strip_prefix("0x").or_else(|| v.strip_prefix("0X")) {
        Some(hex) => u32::from_str_radix(hex, 16).ok().map(|n| n as f32),
        None => v.parse::<f32>().ok().filter(|f| f.is_finite()),
    }
}

fn whole(v: &str) -> Option<i64> {
    number(v).filter(|f| f.fract() == 0.0).map(|f| f as i64)
}

/// A per-game INI over a base one: the value of the first that has the key.
struct Layers<'a> {
    game: Option<&'a str>,
    base: &'a str,
}

impl Layers<'_> {
    fn get(&self, section: &str, key: &str) -> Option<String> {
        self.game
            .and_then(|g| ini_get(g, section, key))
            .or_else(|| ini_get(self.base, section, key))
            .filter(|v| !v.trim().is_empty())
    }
}

// ---- ARMSX2 (PCSX2.ini) --------------------------------------------------------------------------------

fn armsx2(disc: Option<&str>, files: &Files) -> Result<Setup, String> {
    let base = main_file(files, emu_settings::PCSX2_INI)?;
    let game_path = disc.and_then(emu_settings::armsx2_game_ini);
    let game = game_path.as_deref().and_then(|p| game_file(files, p));
    let ini = Layers { game: game.as_deref(), base: &base };
    let gs = |key: &str| ini.get("EmuCore/GS", key);
    let mut s = Setup {
        sources: vec!["PCSX2.ini".into()],
        ..Setup::default()
    };
    if let (Some(p), Some(_)) = (&game_path, &game) {
        s.sources.push(format!("gamesettings/{}", p.rsplit('/').next().unwrap_or(p)));
    }
    // GSRendererType: -1 Auto (Vulkan on this GPU, measured), 12 OpenGL, 13 Software, 14 Vulkan.
    s.renderer = gs("Renderer").and_then(|v| whole(&v)).and_then(|n| match n {
        -1 => Some("Vulkan (Auto)".to_string()),
        12 => Some("OpenGL".into()),
        13 => Some("Software".into()),
        14 => Some("Vulkan".into()),
        _ => None,
    });
    s.resolution = if s.renderer.as_deref() == Some("Software") {
        // The software renderer draws at the console's own size whatever the multiplier says.
        Some(Resolution::Times(1.0))
    } else {
        gs("upscale_multiplier").and_then(|v| number(&v)).filter(|f| *f > 0.0).map(Resolution::Times)
    };
    s.aspect = gs("AspectRatio").map(|v| match v.trim() {
        "Auto 4:3/3:2" => Aspect::Auto("4:3, 3:2 for progressive games"),
        "Stretch" => Aspect::Stretch,
        r @ ("4:3" | "16:9" | "10:7") => Aspect::Fixed(r.to_string()),
        other => Aspect::Custom(other.to_string()),
    });
    s.widescreen = ini.get("EmuCore", "EnableWideScreenPatches").and_then(|v| flag(&v));
    // GSPostBilinearMode: Off, BilinearSmooth, BilinearSharp.
    s.output_filter = gs("linear_present_mode").and_then(|v| whole(&v)).and_then(|n| match n {
        0 => Some("no smoothing".to_string()),
        1 => Some("smooth".into()),
        2 => Some("sharp bilinear".into()),
        _ => None,
    });
    s.integer_scaling = gs("IntegerScaling").and_then(|v| flag(&v));
    // BiFiltering: Nearest, Forced, PS2, Forced_But_Sprite.
    if let Some(n) = gs("filter").and_then(|v| whole(&v)) {
        s.look_read = true;
        s.textures = match n {
            0 => Some(Textures { label: "Nearest textures".into(), kind: TextureKind::Nearest }),
            1 => Some(Textures { label: "Bilinear textures (forced)".into(), kind: TextureKind::Added }),
            2 => Some(Textures { label: "Bilinear textures (as the PS2)".into(), kind: TextureKind::AsGame }),
            3 => Some(Textures { label: "Bilinear textures (forced, not sprites)".into(), kind: TextureKind::Added }),
            _ => None,
        };
    }
    let mut look = |found: Option<String>, effect: Option<String>| {
        if found.is_some() {
            s.look_read = true;
        }
        s.effects.extend(effect);
    };
    let fxaa = gs("fxaa");
    look(fxaa.clone(), fxaa.and_then(|v| flag(&v)).filter(|on| *on).map(|_| "FXAA".to_string()));
    let cas = gs("CASMode");
    let sharpness = gs("CASSharpness").and_then(|v| whole(&v));
    look(
        cas.clone(),
        cas.and_then(|v| whole(&v)).filter(|n| *n > 0).map(|n| {
            let what = if n == 2 { "CAS sharpening and resize" } else { "CAS sharpening" };
            sharpness.map_or(what.to_string(), |p| format!("{what} {p}%"))
        }),
    );
    // GSTVShader.
    let tv = gs("TVShader");
    look(
        tv.clone(),
        tv.and_then(|v| whole(&v)).filter(|n| *n > 0).map(|n| match n {
            1 => "Scanline filter".to_string(),
            2 => "Diagonal filter".into(),
            3 => "Triangular filter".into(),
            4 => "Wave filter".into(),
            5 => "Lottes CRT".into(),
            6 => "4xRGSS".into(),
            7 => "NxAGSS".into(),
            n => format!("TV shader {n}"),
        }),
    );
    let boost = gs("ShadeBoost");
    look(boost.clone(), boost.and_then(|v| flag(&v)).filter(|on| *on).map(|_| "Shade boost".to_string()));
    let aniso = gs("MaxAnisotropy");
    look(aniso.clone(), aniso.and_then(|v| whole(&v)).filter(|n| *n > 1).map(|n| format!("{n}× anisotropic")));
    s.vsync = gs("VsyncEnable").and_then(|v| flag(&v));
    s.speed = ini.get("Framerate", "NominalScalar").and_then(|v| number(&v)).filter(|f| *f >= 0.0);
    let out = |key: &str| ini.get("SPU2/Output", key);
    s.audio = SetupAudio {
        backend: out("Backend").map(|v| v.trim().to_string()),
        buffer_ms: out("BufferMS").and_then(|v| whole(&v)).map(|n| n.max(0) as u32),
        latency_ms: if out("OutputLatencyMinimal").and_then(|v| flag(&v)) == Some(true) {
            None
        } else {
            out("OutputLatencyMS").and_then(|v| whole(&v)).map(|n| n.max(0) as u32)
        },
        stretch: None,
    };
    if s.is_empty() {
        return Err(format!(
            "{}: none of the settings Pixel Path reads are in it",
            files.at(emu_settings::PCSX2_INI).display()
        ));
    }
    Ok(s)
}

// ---- Dolphin (GFX.ini, Dolphin.ini, GameSettings/<ID>.ini) ---------------------------------------------

fn dolphin(rom: &Path, files: &Files) -> Result<Setup, String> {
    let gfx = main_file(files, GFX_INI)?;
    let main = text(&files.at(DOLPHIN_INI)).ok().flatten().unwrap_or_default();
    let header = std::fs::File::open(rom).ok().and_then(|mut f| emu_settings::dolphin_game_id_in(&mut f));
    let id = emu_settings::dolphin_settings_id(rom, header);
    let game = id.as_deref().and_then(|id| game_file(files, &emu_settings::dolphin_game_ini(id)));
    let mut s = Setup {
        sources: vec!["GFX.ini".into()],
        ..Setup::default()
    };
    if !main.is_empty() {
        s.sources.push("Dolphin.ini".into());
    }
    if let (Some(id), Some(_)) = (&id, &game) {
        s.sources.push(format!("GameSettings/{id}.ini"));
    }
    // The game INI names GFX.ini's sections "Video_<section>", Dolphin.ini's as they are.
    let video = |section: &str, key: &str| {
        game.as_deref()
            .and_then(|g| ini_get(g, &format!("Video_{section}"), key))
            .or_else(|| ini_get(&gfx, section, key))
            .filter(|v| !v.trim().is_empty())
    };
    let core = Layers { game: game.as_deref(), base: &main };
    s.renderer = core.get("Core", "GFXBackend").map(|v| match v.trim() {
        "OGL" => "OpenGL".to_string(),
        "Software Renderer" => "Software".into(),
        other => other.to_string(),
    });
    // ROCKNIX's patch 004-enable-240p-res halves the scale: InternalResolution 2 is 1×, 4 is 2×, 1 is 0.5×.
    s.resolution = video("Settings", "InternalResolution").and_then(|v| whole(&v)).and_then(|n| match n {
        0 => Some(Resolution::Window),
        n if n > 0 => Some(Resolution::Times(n as f32 / 2.0)),
        _ => None,
    });
    // AspectMode: Auto, ForceWide, ForceStandard, Stretch, Custom, CustomStretch.
    s.aspect = video("Settings", "AspectRatio").and_then(|v| whole(&v)).map(|n| match n {
        0 => Aspect::Auto("the game says 4:3 or 16:9"),
        1 => Aspect::Fixed("16:9".into()),
        2 => Aspect::Fixed("4:3".into()),
        3 => Aspect::Stretch,
        n => Aspect::Custom(format!("custom ({n})")),
    });
    s.widescreen = video("Settings", "wideScreenHack").and_then(|v| flag(&v));
    // OutputResamplingMode: Default, Bilinear, BSpline, MitchellNetravali, CatmullRom, SharpBilinear, AreaSampling.
    s.output_filter = video("Enhancements", "OutputResampling").and_then(|v| whole(&v)).and_then(|n| match n {
        0 => Some("default resampling".to_string()),
        1 => Some("bilinear".into()),
        2 => Some("bicubic (B-spline)".into()),
        3 => Some("bicubic (Mitchell-Netravali)".into()),
        4 => Some("bicubic (Catmull-Rom)".into()),
        5 => Some("sharp bilinear".into()),
        6 => Some("area sampling".into()),
        _ => None,
    });
    // TextureFilteringMode: Default, Nearest, Linear; older Dolphins wrote a bool (True: linear).
    if let Some(v) = video("Enhancements", "ForceTextureFiltering") {
        s.look_read = true;
        s.textures = match v.trim() {
            "0" | "False" | "false" => Some(Textures { label: "Textures as the game asks".into(), kind: TextureKind::AsGame }),
            "1" => Some(Textures { label: "Nearest textures (forced)".into(), kind: TextureKind::Nearest }),
            "2" | "True" | "true" => Some(Textures { label: "Linear textures (forced)".into(), kind: TextureKind::Added }),
            _ => None,
        };
    }
    // AnisotropicFilteringMode: -1 as the game asks, 0..4 = 1×, 2×, 4×, 8×, 16×.
    if let Some(n) = video("Enhancements", "MaxAnisotropy").and_then(|v| whole(&v)) {
        s.look_read = true;
        if (1..=4).contains(&n) {
            s.effects.push(format!("{}× anisotropic", 1 << n));
        }
    }
    if let Some(v) = video("Settings", "MSAA").and_then(|v| whole(&v)) {
        s.look_read = true;
        if v > 1 {
            let ss = video("Settings", "SSAA").and_then(|v| flag(&v)) == Some(true);
            s.effects.push(format!("{v}× {}", if ss { "SSAA" } else { "MSAA" }));
        }
    }
    if let Some(v) = video("Enhancements", "PostProcessingShader") {
        s.look_read = true;
        let name = v.trim().trim_matches('"');
        if !name.is_empty() && !name.eq_ignore_ascii_case("(off)") {
            s.effects.push(format!("Shader {name}"));
        }
    }
    s.vsync = video("Hardware", "VSync").and_then(|v| flag(&v));
    s.speed = core.get("Core", "EmulationSpeed").and_then(|v| number(&v)).filter(|f| *f >= 0.0);
    s.frame_skip = video("Hacks", "VISkip").and_then(|v| flag(&v));
    s.audio = SetupAudio {
        backend: core.get("DSP", "Backend").map(|v| v.trim().to_string()),
        buffer_ms: None,
        latency_ms: core.get("Core", "AudioLatency").and_then(|v| whole(&v)).map(|n| n.max(0) as u32),
        stretch: core.get("Core", "AudioStretch").and_then(|v| flag(&v)),
    };
    if s.is_empty() {
        return Err(format!("{}: none of the settings Pixel Path reads are in it", files.at(GFX_INI).display()));
    }
    Ok(s)
}

// ---- Azahar (qt-config.ini) ------------------------------------------------------------------------------

/// Azahar's built-in defaults (`settings.h`), for keys whose `\default` is true. A key not listed here with
/// `\default=true` is left out: its value is not known.
fn azahar_default(key: &str) -> Option<&'static str> {
    Some(match key {
        "resolution_factor" => "1",
        "texture_filter" => "0",
        "filter_mode" => "true",
        "use_vsync_new" => "true",
        "frame_limit" => "100",
        "layout_option" => "0",
        "enable_audio_stretching" => "true",
        "output_type" => "0",
        _ => return None,
    })
}

/// A Qt settings value as Azahar takes it: the built-in default where `key\default` is true, else the value.
fn qt(text: &str, section: &str, key: &str) -> Option<String> {
    if ini_get(text, section, &format!("{key}\\default")).and_then(|v| flag(&v)) == Some(true) {
        return azahar_default(key).map(str::to_string);
    }
    ini_get(text, section, key).map(|v| v.trim().trim_matches('"').to_string()).filter(|v| !v.is_empty())
}

fn azahar(files: &Files) -> Result<Setup, String> {
    let ini = main_file(files, emu_settings::AZAHAR_QT)?;
    let r = |key: &str| qt(&ini, "Renderer", key);
    let mut s = Setup {
        sources: vec!["qt-config.ini".into()],
        ..Setup::default()
    };
    // GraphicsAPI: Software, OpenGL, Vulkan.
    s.renderer = r("graphics_api").and_then(|v| whole(&v)).and_then(|n| match n {
        0 => Some("Software".to_string()),
        1 => Some("OpenGL".into()),
        2 => Some("Vulkan".into()),
        _ => None,
    });
    s.resolution = r("resolution_factor").and_then(|v| whole(&v)).and_then(|n| match n {
        0 => Some(Resolution::Window),
        n if n > 0 => Some(Resolution::Times(n as f32)),
        _ => None,
    });
    // LayoutOption: Default (stacked), SingleScreen, LargeScreen; the others are named by number.
    s.layout = qt(&ini, "Layout", "layout_option").and_then(|v| whole(&v)).map(|n| match n {
        0 => ("Stacked".to_string(), true),
        1 => ("Top only".into(), true),
        2 => ("Large top".into(), true),
        n => (format!("Layout {n}"), false),
    });
    s.output_filter = r("filter_mode").and_then(|v| flag(&v)).map(|smooth| if smooth { "smooth" } else { "no smoothing" }.to_string());
    // TextureFilter: None, Anime4K, Bicubic, ScaleForce, xBRZ, MMPX.
    if let Some(n) = r("texture_filter").and_then(|v| whole(&v)) {
        s.look_read = true;
        s.textures = match n {
            0 => Some(Textures { label: "Textures as the game draws them".into(), kind: TextureKind::AsGame }),
            1 => Some(Textures { label: "Anime4K textures".into(), kind: TextureKind::Added }),
            2 => Some(Textures { label: "Bicubic textures".into(), kind: TextureKind::Added }),
            3 => Some(Textures { label: "ScaleForce textures".into(), kind: TextureKind::Added }),
            4 => Some(Textures { label: "xBRZ textures".into(), kind: TextureKind::Added }),
            5 => Some(Textures { label: "MMPX textures".into(), kind: TextureKind::Added }),
            _ => None,
        };
    }
    if let Some(name) = r("pp_shader_name") {
        s.look_read = true;
        if !name.to_ascii_lowercase().starts_with("none") {
            s.effects.push(format!("Shader {name}"));
        }
    }
    s.vsync = r("use_vsync_new").and_then(|v| flag(&v));
    s.speed = r("frame_limit").and_then(|v| number(&v)).filter(|f| *f >= 0.0).map(|p| p / 100.0);
    let a = |key: &str| qt(&ini, "Audio", key);
    // SinkType: Auto, Null, Cubeb, OpenAL, SDL2.
    s.audio = SetupAudio {
        backend: a("output_type").and_then(|v| whole(&v)).and_then(|n| match n {
            1 => Some("no sound".to_string()),
            2 => Some("Cubeb".into()),
            3 => Some("OpenAL".into()),
            4 => Some("SDL2".into()),
            _ => None,
        }),
        buffer_ms: None,
        latency_ms: None,
        stretch: a("enable_audio_stretching").and_then(|v| flag(&v)),
    };
    if s.is_empty() {
        return Err(format!(
            "{}: none of the settings Pixel Path reads are in it",
            files.at(emu_settings::AZAHAR_QT).display()
        ));
    }
    Ok(s)
}

// ---- the newest reading, off the UI thread -------------------------------------------------------------

/// The game and what its last reading found.
pub type Reading = (PathBuf, Result<Setup, String>);

static LAST: Mutex<Option<Reading>> = Mutex::new(None);

/// The last reading for `rom`, if one was taken.
pub fn cached(rom: &Path) -> Option<Result<Setup, String>> {
    let last = LAST.lock().ok()?;
    last.as_ref().filter(|(r, _)| r == rom).map(|(_, s)| s.clone())
}

/// Whether the pause menu lists Pixel Path for this standalone game: only when its last reading found something.
/// Not read yet, or nothing to read: not listed (seven empty stages would say nothing).
pub fn listed(rom: &Path) -> bool {
    matches!(cached(rom), Some(Ok(_)))
}

/// Keeps a reading; says in the log what changed (why Pixel Path isn't listed for this game, or that it is again).
pub fn remember(rom: &Path, emulator: &str, result: Result<Setup, String>) {
    let Ok(mut last) = LAST.lock() else { return };
    let same = last.as_ref().is_some_and(|(r, s)| r == rom && *s == result);
    if !same {
        match &result {
            Ok(s) => println!("pixel path: {emulator}'s settings read ({})", s.sources.join(", ")),
            Err(why) => println!("pixel path: not listed for {emulator}: {why}"),
        }
    }
    *last = Some((rom.to_path_buf(), result));
}

/// Reads the running standalone game's settings now and keeps the result. On a thread: it reads files.
pub fn refresh(files: &Files) -> Option<Reading> {
    let run = crate::standalone::session()?;
    let result = read(run.emu, &run.system, &run.rom, run.disc.as_deref(), files);
    remember(&run.rom, emulator_name(run.emu), result.clone());
    Some((run.rom, result))
}

/// A standalone game has just been started: a thread takes the first reading once its start script has rewritten
/// the INIs, so the first pause menu already knows whether to list Pixel Path.
pub fn prefetch() {
    let Some(rom) = crate::standalone::session().map(|s| s.rom) else { return };
    let spawned = std::thread::Builder::new().name("pixel-setup".into()).spawn(move || {
        let step = Duration::from_millis(250);
        let mut waited = Duration::ZERO;
        while waited < PREFETCH_AFTER {
            std::thread::sleep(step);
            waited += step;
            if crate::standalone::session().map(|s| s.rom).as_deref() != Some(rom.as_path()) {
                return;
            }
        }
        refresh(&Files::default());
    });
    if let Err(e) = spawned {
        eprintln!("pixel path: can't start the settings reader: {e}");
    }
}

// ---- the stages ----------------------------------------------------------------------------------------

/// The panel as pocket-ui knows it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Screen {
    pub w: u32,
    pub h: u32,
    pub hz: f64,
}

impl Default for Screen {
    fn default() -> Screen {
        // The device's screen until sway says otherwise (`device`; the Mini V2's 1240x1080 when unknown).
        let (w, h) = crate::device::current().screen();
        Screen { w, h, hz: 60.0 }
    }
}

fn stage(kind: Kind, grade: Grade, value: impl Into<String>, line: impl Into<String>, about: impl Into<String>) -> Stage {
    Stage {
        kind,
        grade,
        value: value.into(),
        line: line.into(),
        about: about.into(),
    }
}

/// The console's own picture an internal resolution multiplies: Dolphin's 640×528 frame buffer, the 3DS's top
/// screen. PS2 games draw at sizes of their own (512×448, 640×448, 640×512, …), so there is none for it.
fn native_size(system: &str) -> Option<(u32, u32)> {
    match system {
        "gamecube" | "wii" => Some((640, 528)),
        "3ds" => Some((400, 240)),
        _ => None,
    }
}

fn times(f: f32) -> String {
    if (f - f.round()).abs() < 0.01 {
        return format!("{:.0}×", f.round());
    }
    let digits = format!("{f:.2}");
    format!("{}×", digits.trim_end_matches('0').trim_end_matches('.'))
}

fn list(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [init @ .., last] => format!("{} and {last}", init.join(", ")),
    }
}

/// The chain of a standalone game, from its settings. `console` is the system's name ("Sony PlayStation 2").
pub fn path(s: &Setup, console: &str, screen: Screen) -> pixel_path::Path {
    let console = if console.is_empty() { "console" } else { console };
    let stages = vec![
        core_stage(s, console),
        source_stage(s, console),
        scale_stage(s, screen),
        look_stage(s),
        display_stage(s, screen),
        latency_stage(s, screen),
        audio_stage(s),
    ];
    let overall = pixel_path::overall(stages.iter().map(|st| st.grade));
    let partial = stages.iter().any(|st| st.grade == Grade::Unknown);
    pixel_path::Path {
        stages,
        overall,
        live: Some(format!("Read from {}.", list(&s.sources))),
        partial,
    }
}

fn core_stage(s: &Setup, console: &str) -> Stage {
    let emu = &s.emulator;
    let value = match &s.renderer {
        Some(r) => format!("{emu} · {r}"),
        None => emu.clone(),
    };
    let drawn = s.renderer.as_ref().map(|r| format!(" It draws with {r}.")).unwrap_or_default();
    stage(
        Kind::Core,
        Grade::Unknown,
        value,
        format!("The {console}, run by {emu} itself."),
        format!(
            "{emu} runs this game on its own: pocket-ui starts it and draws the pause menu over it, but the picture, the timing and \
             the sound are {emu}'s.{drawn} What Pixel Path shows here is read from {emu}'s settings ({}): the values this game runs \
             with, Options' changes included. Nothing is measured, so a stage is graded only where its settings alone decide it; \
             how exact the emulation is can't be graded at all.",
            list(&s.sources)
        ),
    )
}

fn source_stage(s: &Setup, console: &str) -> Stage {
    let wide = s.widescreen == Some(true);
    let wide_name = if s.emulator == "Dolphin" { "widescreen hack" } else { "widescreen patch" };
    let wide_about = if wide {
        format!(
            " The {wide_name} is on: the game draws more to the left and right than it did on the {console}, for a 16:9 picture."
        )
    } else {
        String::new()
    };
    let Some(res) = &s.resolution else {
        let value = if wide { format!("Not set · {wide_name}") } else { "Not in the settings".to_string() };
        return stage(
            Kind::Source,
            if wide { Grade::Enhanced } else { Grade::Unknown },
            value,
            "The internal resolution isn't in the settings.",
            format!("{}'s settings don't say how large it renders.{wide_about}", s.emulator),
        );
    };
    let Resolution::Times(f) = *res else {
        return stage(
            Kind::Source,
            if wide { Grade::Enhanced } else { Grade::Unknown },
            if wide { format!("Window size · {wide_name}") } else { "Window size".to_string() },
            "Follows the window's size.",
            format!(
                "{} renders at a size it works out from its window, so the setting alone doesn't say how large.{wide_about}",
                s.emulator
            ),
        );
    };
    let x = times(f);
    let size = native_size(&s.system).map(|(w, h)| ((w as f32 * f).round() as u32, (h as f32 * f).round() as u32));
    let mut value = match size {
        Some((w, h)) => format!("{x} · {w}×{h}"),
        None => format!("{x} internal"),
    };
    if wide {
        value.push_str(&format!(" · {wide_name}"));
    }
    let what = match (s.system.as_str(), size) {
        ("3ds", Some((w, h))) => format!(
            "The 3DS's top screen is 400×240 and its bottom screen 320×240; {} renders both at {x}, the top one at {w}×{h}.",
            s.emulator
        ),
        ("gamecube" | "wii", Some((w, h))) => format!(
            "{}'s 1× is the {console}'s own 640×528 frame buffer; at {x} it renders {w}×{h}. (ROCKNIX's Dolphin counts this \
             setting in half steps: InternalResolution {} is {x}.)",
            s.emulator,
            (f * 2.0).round()
        ),
        _ => format!(
            "{} renders the 3D scenes at {x} the size the game draws them on the {console}: a game that draws 640×448 becomes {}×{}.",
            s.emulator,
            (640.0 * f).round(),
            (448.0 * f).round()
        ),
    };
    let (grade, line) = if f >= 1.5 {
        (Grade::Enhanced, format!("Drawn at {x} the {console}'s resolution."))
    } else if f < 0.9 {
        (Grade::Compromised, format!("Below the {console}'s own resolution."))
    } else if wide {
        (Grade::Enhanced, format!("The {console}'s own resolution, widened."))
    } else {
        (Grade::Pristine, format!("The {console}'s own resolution."))
    };
    let meaning = match grade {
        Grade::Compromised => " Fewer lines than the console drew: edges and text are coarser than they were. Options raises it.",
        Grade::Enhanced if f >= 1.5 => " Edges and textures are finer than they ever were on the console; menus and 2D parts may stay at their own size. Options changes it.",
        _ => "",
    };
    stage(Kind::Source, grade, value, line, format!("{what}{meaning}{wide_about}"))
}

fn scale_stage(s: &Setup, screen: Screen) -> Stage {
    let emu = &s.emulator;
    let panel = format!("{}×{}", screen.w, screen.h);
    let filter = s
        .output_filter
        .as_ref()
        .map(|f| format!(" Output filter: {f}."))
        .unwrap_or_default();
    let mut parts: Vec<String> = Vec::new();
    let free = " A 3D picture has no pixel grid of the console's to keep, so a size between whole steps loses little; Pixel Path grades pocket-core's 3D systems the same way.";
    let (grade, line, about) = if let Some((name, keeps)) = &s.layout {
        parts.push(name.clone());
        if *keeps {
            (
                Grade::Faithful,
                "Each screen keeps its shape.".to_string(),
                format!("{emu} lays the 3DS's two screens out on the {panel} panel ({name}), each scaled to fit with its own shape.{filter}{free}"),
            )
        } else {
            (
                Grade::Unknown,
                "A layout Pixel Path doesn't know.".to_string(),
                format!("{emu} uses a screen layout ({name}) Pixel Path can't tell the shape of.{filter}"),
            )
        }
    } else {
        match &s.aspect {
            None => (
                Grade::Unknown,
                "The aspect ratio isn't in the settings.".to_string(),
                format!("{emu}'s settings don't say which shape the picture is drawn in.{filter}"),
            ),
            Some(Aspect::Stretch) => {
                parts.push("Stretch".into());
                (
                    Grade::Compromised,
                    "Stretched to fill the screen.".to_string(),
                    format!(
                        "The aspect ratio is Stretch: the picture is pulled to the {panel} panel's shape, wider than the game was made, so \
                         circles become ovals. 4:3 or Auto in Options keeps the shape.{filter}"
                    ),
                )
            }
            Some(Aspect::Auto(how)) => {
                parts.push("Auto".into());
                (
                    Grade::Faithful,
                    "The game's own shape, scaled to fit.".to_string(),
                    format!("The aspect ratio is Auto ({how}): {emu} keeps the game's shape and scales it to fit the {panel} panel.{filter}{free}"),
                )
            }
            Some(Aspect::Fixed(r)) if r == "16:9" && s.widescreen != Some(true) => {
                parts.push(r.clone());
                (
                    Grade::Unknown,
                    "16:9: right for widescreen games, stretched for others.".to_string(),
                    format!(
                        "The aspect ratio is 16:9 without the widescreen {}. Games that draw widescreen themselves look right; a game that \
                         draws 4:3 is stretched. Which one this game does isn't in the settings, so this stage has no grade.{filter}",
                        if emu == "Dolphin" { "hack" } else { "patch" }
                    ),
                )
            }
            Some(Aspect::Fixed(r)) => {
                parts.push(r.clone());
                let why = if r == "16:9" {
                    "the shape the widescreen patch draws".to_string()
                } else {
                    format!("{r}, the TV's shape the game was made for")
                };
                (
                    Grade::Faithful,
                    format!("{r}, scaled to fit."),
                    format!(
                        "The aspect ratio is {why}; {emu} scales the picture to fit the {panel} panel. A game switched to widescreen in its own \
                         options looks squeezed at 4:3, as it did on a 4:3 TV.{filter}{free}"
                    ),
                )
            }
            Some(Aspect::Custom(r)) => {
                parts.push(r.clone());
                (
                    Grade::Unknown,
                    "A shape set by hand.".to_string(),
                    format!("The aspect ratio is {r}: whether that is the game's shape isn't in the settings.{filter}"),
                )
            }
        }
    };
    parts.push(format!("fit to {panel}"));
    if s.integer_scaling == Some(true) {
        parts.push("whole steps".into());
    }
    parts.extend(s.output_filter.clone());
    stage(Kind::Scale, grade, parts.join(" · "), line, about)
}

fn look_stage(s: &Setup) -> Stage {
    let mut parts: Vec<String> = Vec::new();
    if let Some(t) = s.textures.as_ref().filter(|t| t.kind != TextureKind::AsGame) {
        parts.push(t.label.clone());
    }
    parts.extend(s.effects.iter().cloned());
    let nearest = s.textures.as_ref().is_some_and(|t| t.kind == TextureKind::Nearest);
    let added = !s.effects.is_empty() || s.textures.as_ref().is_some_and(|t| t.kind == TextureKind::Added);
    let as_game = s
        .textures
        .as_ref()
        .filter(|t| t.kind == TextureKind::AsGame)
        .map(|t| format!(" {}.", t.label))
        .unwrap_or_default();
    if nearest {
        let more = if parts.len() > 1 { format!(" Also on: {}.", list(&parts[1..])) } else { String::new() };
        return stage(
            Kind::Look,
            Grade::Compromised,
            parts.join(" · "),
            "Textures unfiltered, though the game smooths them.",
            format!(
                "{} draws every texture with hard pixels, even where the game asks for them to be smoothed: blocky surfaces and \
                 shimmering it never had.{more}",
                s.emulator
            ),
        );
    }
    if added {
        return stage(
            Kind::Look,
            Grade::Enhanced,
            parts.join(" · "),
            "Added to the picture on purpose.",
            format!(
                "{} adds to the picture what the console didn't have: {}. It changes how the game looks rather than taking \
                 something away.{as_game}",
                s.emulator,
                list(&parts)
            ),
        );
    }
    if s.look_read {
        return stage(
            Kind::Look,
            Grade::Pristine,
            "Nothing added",
            "No filter, shader or smoothing over the picture.",
            format!(
                "No anti-aliasing, sharpening, shader or texture filter of {}'s is on: the 3D scenes look as the game draws them.{as_game}",
                s.emulator
            ),
        );
    }
    stage(
        Kind::Look,
        Grade::Unknown,
        "Not in the settings",
        "Filters and shaders aren't in the settings.",
        format!("{}'s settings don't say whether a filter or shader is on.", s.emulator),
    )
}

fn display_stage(s: &Setup, screen: Screen) -> Stage {
    let emu = &s.emulator;
    let mut parts = vec![format!("{}×{} @ {:.0} Hz", screen.w, screen.h, screen.hz)];
    match s.vsync {
        Some(true) => parts.push("VSync on".into()),
        Some(false) => parts.push("VSync off".into()),
        None => {}
    }
    let speed = s.speed.filter(|f| (f - 1.0).abs() > 0.005);
    if let Some(f) = speed {
        parts.push(if f == 0.0 { "unlimited speed".to_string() } else { format!("speed {:.0}%", f * 100.0) });
    }
    if s.frame_skip == Some(true) {
        parts.push("skips when slow".into());
    }
    let line = match s.vsync {
        Some(true) => format!("{emu} waits for each refresh of the screen."),
        Some(false) => format!("{emu} keeps its own pace; the screen shows its newest picture."),
        None => format!("How {emu} keeps pace isn't in the settings."),
    };
    let pace = match s.vsync {
        Some(true) => format!(
            "With VSync on, {emu} waits for the panel's refresh before it shows a picture: even, but it can add a refresh of delay."
        ),
        Some(false) => format!(
            "With VSync off, {emu} doesn't wait for the panel: each refresh shows the newest picture it has, so now and then one stays \
             for two refreshes or is replaced before it is seen, a small stutter."
        ),
        None => String::new(),
    };
    let ps2 = if s.system == "ps2" && s.vsync == Some(false) {
        " pocket-ui keeps it off for ARMSX2: with it, the save on Quit could hang while the menu covers the game."
    } else {
        ""
    };
    let speed_note = speed
        .map(|f| {
            if f == 0.0 {
                " The frame limiter is off: the game runs as fast as it can.".to_string()
            } else {
                format!(" The frame limiter is set to {:.0}% of the console's speed.", f * 100.0)
            }
        })
        .unwrap_or_default();
    let skip = if s.frame_skip == Some(true) {
        " Frame skip (VBI skip) is on: when the game runs slow, pictures are left out to keep the sound smooth."
    } else {
        ""
    };
    stage(
        Kind::Display,
        Grade::Unknown,
        parts.join(" · "),
        line,
        format!(
            "The panel shows {:.0} pictures a second at {}×{}. {emu} draws into its own window; pocket-ui isn't in between. {pace}{ps2}{speed_note}{skip} \
             Which pictures came late isn't measured for this game, so this stage has no grade.",
            screen.hz, screen.w, screen.h
        )
        .replace("  ", " "),
    )
}

fn latency_stage(s: &Setup, screen: Screen) -> Stage {
    let refresh_ms = 1000.0 / screen.hz.max(1.0);
    let vsync = if s.vsync == Some(true) {
        format!(" With VSync on, add up to one refresh ({refresh_ms:.0} ms) of waiting.")
    } else {
        String::new()
    };
    stage(
        Kind::Latency,
        Grade::Unknown,
        "Not measured",
        "pocket-ui doesn't run this game's frames, so it can't time them.",
        format!(
            "For pocket-core games, Pixel Path works out the time from a button press to the screen from how pocket-core is built. {} is built \
             differently and reads the buttons itself, and pocket-ui can't see inside it, so it gives no number rather than a guess.{vsync}",
            s.emulator
        ),
    )
}

fn audio_stage(s: &Setup) -> Stage {
    let a = &s.audio;
    let emu = &s.emulator;
    let mut parts: Vec<String> = Vec::new();
    match &a.backend {
        Some(b) if b == "no sound" => parts.push("No sound (set in the emulator)".into()),
        Some(b) => parts.push(format!("{b} to PipeWire")),
        None => parts.push("PipeWire".into()),
    }
    if let Some(ms) = a.buffer_ms {
        parts.push(format!("{ms} ms buffer"));
    }
    if let Some(ms) = a.latency_ms {
        parts.push(format!("{ms} ms latency"));
    }
    if a.stretch == Some(true) {
        parts.push("stretching".into());
    }
    if a.backend.as_deref() == Some("no sound") {
        return stage(
            Kind::Audio,
            Grade::Compromised,
            parts.join(" · "),
            "The emulator's sound output is off.",
            format!("{emu}'s sound output is set to none: the game plays without sound."),
        );
    }
    let via = a.backend.as_ref().map(|b| format!(" through its {b} output")).unwrap_or_default();
    let asked = {
        let mut v = Vec::new();
        if let Some(ms) = a.buffer_ms {
            v.push(format!("a {ms} ms buffer"));
        }
        if let Some(ms) = a.latency_ms {
            v.push(format!("{ms} ms of output latency"));
        }
        if a.stretch == Some(true) {
            v.push("time stretching (slow stretches of the game slow the sound down instead of breaking it up)".to_string());
        }
        if v.is_empty() {
            String::new()
        } else {
            format!(" Its settings ask for {}.", list(&v))
        }
    };
    stage(
        Kind::Audio,
        Grade::Unknown,
        parts.join(" · "),
        format!("As {emu} is set; gaps aren't counted here."),
        format!(
            "{emu} plays the sound itself{via}, into ROCKNIX's sound server, PipeWire, and on to the speaker.{asked} pocket-ui doesn't \
             see the sound on its way, so gaps aren't counted and this stage has no grade."
        ),
    )
}

/// The sample settings of `testdata/pixel-path/<case>` (`--snapshot pixel-path:<armsx2|dolphin|azahar>`): the files
/// the tests read, so the page shows exactly what they check.
pub fn demo(case: &str) -> Option<(Setup, &'static str)> {
    let root = Path::new("testdata/pixel-path").to_path_buf();
    let (emu, system, dir, rom, disc, console) = match case {
        "armsx2" => (Emu::Armsx2, "ps2", "armsx2", "Burnout 3 - Takedown (USA).chd", Some("SLUS-21050 (12345678)"), "Sony PlayStation 2"),
        "dolphin" => (Emu::Dolphin, "gamecube", "dolphin", "Mario Kart - Double Dash!! (USA).iso", None, "Nintendo GameCube"),
        "azahar" => (Emu::Azahar, "3ds", "azahar", "Mario Kart 7 (USA).3ds", None, "Nintendo 3DS"),
        _ => return None,
    };
    let files = Files {
        system_cfg: root.join(dir).join("storage/.config/system/configs/system.cfg"),
        root: root.join(dir),
    };
    let rom = root.join(dir).join("roms").join(rom);
    read(emu, system, &rom, disc, &files).ok().map(|s| (s, console))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;

    fn data(dir: &str) -> Files {
        let root = Path::new("testdata/pixel-path").to_path_buf().join(dir);
        Files {
            system_cfg: root.join("storage/.config/system/configs/system.cfg"),
            root,
        }
    }

    /// A folder of its own under the temp dir, with `entries` written (device paths, relative to the root).
    fn scratch(name: &str, entries: &[(&str, &[u8])]) -> (Files, crate::testutil::TestDir) {
        let root = TestDir::new(&format!("pixel-setup-{name}"));
        for (path, bytes) in entries {
            let p = root.join(path.trim_start_matches('/'));
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, bytes).unwrap();
        }
        let files = Files {
            system_cfg: root.join("storage/.config/system/configs/system.cfg"),
            root: root.to_path_buf(),
        };
        (files, root)
    }

    fn grade(p: &pixel_path::Path, k: Kind) -> Grade {
        p.stages.iter().find(|s| s.kind == k).unwrap().grade
    }

    fn value(p: &pixel_path::Path, k: Kind) -> String {
        p.stages.iter().find(|s| s.kind == k).unwrap().value.clone()
    }

    #[test]
    fn armsx2_reads_pcsx2_ini_and_the_games_own_ini_wins() {
        let (s, console) = demo("armsx2").unwrap();
        assert_eq!(s.sources, ["PCSX2.ini", "gamesettings/SLUS-21050_12345678.ini"]);
        assert_eq!(s.renderer.as_deref(), Some("Vulkan"));
        // PCSX2.ini says 2, the game's own INI 3: the game's wins.
        assert_eq!(s.resolution, Some(Resolution::Times(3.0)));
        assert_eq!(s.aspect, Some(Aspect::Fixed("4:3".into())));
        assert_eq!(s.widescreen, Some(false));
        assert_eq!(s.output_filter.as_deref(), Some("smooth"));
        assert_eq!(s.textures.as_ref().map(|t| t.kind), Some(TextureKind::AsGame));
        assert_eq!(s.effects, ["FXAA"]);
        assert_eq!(s.vsync, Some(false));
        assert_eq!(s.audio, SetupAudio { backend: Some("Cubeb".into()), buffer_ms: Some(50), latency_ms: Some(20), stretch: None });

        let p = path(&s, console, Screen::default());
        assert_eq!(value(&p, Kind::Core), "ARMSX2 · Vulkan");
        assert_eq!(value(&p, Kind::Source), "3× internal");
        assert_eq!(grade(&p, Kind::Source), Grade::Enhanced);
        assert_eq!(value(&p, Kind::Scale), "4:3 · fit to 1240×1080 · smooth");
        assert_eq!(grade(&p, Kind::Scale), Grade::Faithful);
        assert_eq!((value(&p, Kind::Look), grade(&p, Kind::Look)), ("FXAA".to_string(), Grade::Enhanced));
        assert_eq!(value(&p, Kind::Display), "1240×1080 @ 60 Hz · VSync off");
        assert_eq!(value(&p, Kind::Latency), "Not measured");
        assert_eq!(value(&p, Kind::Audio), "Cubeb to PipeWire · 50 ms buffer · 20 ms latency");
        for k in [Kind::Core, Kind::Display, Kind::Latency, Kind::Audio] {
            assert_eq!(grade(&p, k), Grade::Unknown, "{k:?} is not graded from settings");
        }
        assert!(p.partial);
        assert_eq!(p.overall, Grade::Enhanced, "the worst of the graded stages");
    }

    #[test]
    fn armsx2_without_the_disc_known_reads_pcsx2_ini_alone() {
        let files = data("armsx2");
        let s = read(Emu::Armsx2, "ps2", Path::new("x.chd"), None, &files).unwrap();
        assert_eq!(s.sources, ["PCSX2.ini"]);
        assert_eq!(s.resolution, Some(Resolution::Times(2.0)));
    }

    #[test]
    fn dolphin_reads_the_game_ini_over_gfx_ini_and_halves_rocknix_scale() {
        let (s, console) = demo("dolphin").unwrap();
        assert_eq!(s.sources, ["GFX.ini", "Dolphin.ini", "GameSettings/GM4E01.ini"]);
        // GFX.ini: OGL and 2 (1×); the game's INI: Vulkan and 4 (2×).
        assert_eq!(s.renderer.as_deref(), Some("Vulkan"));
        assert_eq!(s.resolution, Some(Resolution::Times(2.0)));
        assert_eq!(s.aspect, Some(Aspect::Auto("the game says 4:3 or 16:9")));
        assert_eq!(s.audio.backend.as_deref(), Some("Pulse"));
        assert_eq!(s.audio.latency_ms, Some(20));
        let p = path(&s, console, Screen::default());
        assert_eq!(value(&p, Kind::Source), "2× · 1280×1056");
        assert_eq!(grade(&p, Kind::Source), Grade::Enhanced);
        assert_eq!(grade(&p, Kind::Scale), Grade::Faithful);
        assert_eq!((value(&p, Kind::Look), grade(&p, Kind::Look)), ("Nothing added".to_string(), Grade::Pristine));
        assert!(value(&p, Kind::Display).ends_with("VSync off"));
    }

    #[test]
    fn dolphin_homebrew_is_found_by_its_file_name() {
        let gfx = b"[Settings]\nInternalResolution = 2\nAspectRatio = 3\n" as &[u8];
        let game = b"[Video_Settings]\nInternalResolution = 1\n" as &[u8];
        let (files, _dir) = scratch("dol", &[("/storage/.config/dolphin-emu/GFX.ini", gfx), ("/storage/.config/dolphin-emu/GameSettings/ID-Blockamok.ini", game)]);
        let s = read(Emu::Dolphin, "wii", Path::new("/storage/roms/wii/Blockamok.dol"), None, &files).unwrap();
        assert_eq!(s.resolution, Some(Resolution::Times(0.5)));
        let p = path(&s, "Nintendo Wii", Screen::default());
        assert_eq!(value(&p, Kind::Source), "0.5× · 320×264");
        assert_eq!(grade(&p, Kind::Source), Grade::Compromised);
        assert_eq!(grade(&p, Kind::Scale), Grade::Compromised, "Stretch");
        assert_eq!(p.overall, Grade::Compromised);
    }

    #[test]
    fn azahar_reads_qt_config_and_its_defaults() {
        let (s, console) = demo("azahar").unwrap();
        assert_eq!(s.sources, ["qt-config.ini"]);
        assert_eq!(s.renderer.as_deref(), Some("Vulkan"));
        assert_eq!(s.resolution, Some(Resolution::Times(2.0)));
        assert_eq!(s.layout, Some(("Large top".to_string(), true)));
        assert_eq!(s.textures.as_ref().map(|t| t.label.as_str()), Some("xBRZ textures"));
        // `filter_mode\default=true`: Azahar's default (smooth), whatever the line says.
        assert_eq!(s.output_filter.as_deref(), Some("smooth"));
        assert_eq!(s.vsync, Some(true));
        let p = path(&s, console, Screen::default());
        assert_eq!(value(&p, Kind::Source), "2× · 800×480");
        assert_eq!(grade(&p, Kind::Scale), Grade::Faithful);
        assert_eq!(grade(&p, Kind::Look), Grade::Enhanced);
        assert_eq!(value(&p, Kind::Scale), "Large top · fit to 1240×1080 · smooth");
    }

    #[test]
    fn a_missing_file_says_why_and_lists_nothing() {
        let (files, _dir) = scratch("missing", &[]);
        for emu in [Emu::Armsx2, Emu::Dolphin, Emu::Azahar] {
            let why = read(emu, "ps2", Path::new("x.iso"), None, &files).unwrap_err();
            assert!(why.contains("is not there"), "{emu:?}: {why}");
        }
    }

    #[test]
    fn a_garbled_file_lists_nothing() {
        let junk: &[u8] = &[0xff, 0xfe, 0x00, 0x13, 0x37, 0xc3, 0x28, b'[', 0xa0];
        let words: &[u8] = b"this is not an ini file\nRenderer: Vulkan\n";
        let (files, _dir) = scratch(
            "garbled",
            &[
                (emu_settings::PCSX2_INI, junk),
                (GFX_INI, words),
                (emu_settings::AZAHAR_QT, b"[Renderer]\ngraphics_api=banana\n"),
            ],
        );
        let ps2 = read(Emu::Armsx2, "ps2", Path::new("x.iso"), None, &files).unwrap_err();
        assert!(ps2.contains("not a text file"), "{ps2}");
        let gc = read(Emu::Dolphin, "gamecube", Path::new("x.iso"), None, &files).unwrap_err();
        assert!(gc.contains("none of the settings"), "{gc}");
        let n3ds = read(Emu::Azahar, "3ds", Path::new("x.3ds"), None, &files).unwrap_err();
        assert!(n3ds.contains("none of the settings"), "{n3ds}");
    }

    #[test]
    fn a_garbled_game_ini_falls_back_to_the_base() {
        let (files, _dir) = scratch(
            "garbled-game",
            &[
                (emu_settings::PCSX2_INI, b"[EmuCore/GS]\nupscale_multiplier = 2\nAspectRatio = Stretch\n"),
                ("/storage/.config/ARMSX2/gamesettings/SLUS-21050_12345678.ini", &[0xff, 0xfe, 0xfd]),
            ],
        );
        let s = read(Emu::Armsx2, "ps2", Path::new("x.chd"), Some("SLUS-21050 (12345678)"), &files).unwrap();
        assert_eq!(s.sources, ["PCSX2.ini"]);
        assert_eq!(s.resolution, Some(Resolution::Times(2.0)));
        let p = path(&s, "PlayStation 2", Screen::default());
        assert_eq!(grade(&p, Kind::Scale), Grade::Compromised, "stretched");
        assert_eq!(grade(&p, Kind::Look), Grade::Unknown, "no look setting in the file: no guess");
    }

    #[test]
    fn sixteen_nine_is_graded_only_with_the_widescreen_patch() {
        let mut s = demo("armsx2").unwrap().0;
        s.aspect = Some(Aspect::Fixed("16:9".into()));
        let p = path(&s, "PS2", Screen::default());
        assert_eq!(grade(&p, Kind::Scale), Grade::Unknown);
        s.widescreen = Some(true);
        let p = path(&s, "PS2", Screen::default());
        assert_eq!(grade(&p, Kind::Scale), Grade::Faithful);
        assert!(value(&p, Kind::Source).ends_with("widescreen patch"));
    }

    #[test]
    fn one_times_is_the_consoles_own_and_nearest_textures_are_compromised() {
        let mut s = demo("armsx2").unwrap().0;
        s.resolution = Some(Resolution::Times(1.0));
        s.textures = Some(Textures { label: "Nearest textures".into(), kind: TextureKind::Nearest });
        let p = path(&s, "PS2", Screen::default());
        assert_eq!(grade(&p, Kind::Source), Grade::Pristine);
        assert_eq!(grade(&p, Kind::Look), Grade::Compromised);
        assert_eq!(value(&p, Kind::Look), "Nearest textures · FXAA");
    }

    #[test]
    fn the_last_reading_is_kept_per_game() {
        let rom = Path::new("/storage/roms/ps2/cache-test.chd");
        assert!(!listed(rom), "not read yet: not listed");
        remember(rom, "ARMSX2", Err("PCSX2.ini is not there".into()));
        assert!(cached(rom).unwrap().is_err());
        assert!(!listed(rom), "nothing could be read: the menu leaves Pixel Path out");
        assert!(cached(Path::new("/storage/roms/ps2/other.chd")).is_none(), "another game's reading is not this one's");
        remember(rom, "ARMSX2", Ok(demo("armsx2").unwrap().0));
        assert!(cached(rom).unwrap().is_ok());
        assert!(listed(rom), "read: listed");
        assert!(!listed(Path::new("/storage/roms/ps2/other.chd")));
    }

    #[test]
    fn small_formats() {
        assert_eq!(times(2.0), "2×");
        assert_eq!(times(1.25), "1.25×");
        assert_eq!(times(1.5), "1.5×");
        assert_eq!(number("0x00000004"), Some(4.0));
        assert_eq!(number("1.000000"), Some(1.0));
        assert_eq!(list(&["a".into(), "b".into(), "c".into()]), "a, b and c");
    }
}
