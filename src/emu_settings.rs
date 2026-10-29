//! Options of the standalone emulators in the pause menu (PS2 on ARMSX2, GameCube and Wii on Dolphin nogui,
//! 3DS on Azahar): a short list per emulator (resolution, renderer, screens, aspect ratio, frame skip, FPS
//! counter, ...), kept per game, applied live where the emulator allows it and otherwise by a restart that
//! resumes where the player was. Plus "Advanced…", the emulator's own menu where there is one. See
//! docs/BIG-EMULATORS.md, "Options".
//!
//! Each emulator is one `Table` with its per-game store.

use crate::standalone::Emu;
use std::path::{Path, PathBuf};

/// One value of a setting: its label, what goes into the per-game store, and what the running emulator
/// is told (PINE value, or the INI value Dolphin reads). `also`: other spellings that mean this choice
/// when the current value is read back.
#[derive(Debug)]
pub struct Choice {
    pub label: &'static str,
    pub stored: &'static str,
    pub emu: &'static str,
    pub also: &'static [&'static str],
}

const fn c(label: &'static str, stored: &'static str, emu: &'static str) -> Choice {
    Choice { label, stored, emu, also: &[] }
}

/// Where the choice for one game is kept.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Store {
    /// ROCKNIX's per-game setting in system.cfg, `<platform>["<rom file>"].<key>=<value>`, which the start
    /// script writes into the emulator's INI at launch (start_armsx2.sh does this for the PS2 keys).
    Es(&'static str),
    /// `[section] key` in the game's own INI (Dolphin's `GameSettings/<GAMEID>.ini`).
    GameIni(&'static str, &'static str),
}

/// The emulator's own INI that holds the value in effect when no per-game choice is set.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BaseFile {
    Pcsx2,
    DolphinGfx,
    DolphinMain,
    /// Azahar's qt-config.ini.
    AzaharQt,
    /// A plain key of ROCKNIX's system.cfg (the section is unused).
    SystemCfg,
}

/// The ROCKNIX Dolphin hotkeys (patch 000-hotkeys) that switch a setting while the game runs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Toggle {
    /// HOTKEY + GC Y: FPS counter on, off.
    Fps,
    /// HOTKEY + GC X: aspect ratio Stretch, Auto.
    Aspect,
    /// HOTKEY + GC B: internal resolution 1x, 2x, 3x, 0.5x (INI values 2, 4, 6, 1).
    Resolution,
}

/// How a change reaches the running game.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Live {
    /// ARMSX2's PINE SetSetting (0x12): written into its base INI layer and applied at once.
    Pine,
    /// A Dolphin hotkey; values it can't reach need a restart.
    Toggle(Toggle),
    /// Only at the next start: save, quit, start again, load.
    Restart,
    /// Only the next time the game starts, with no restart offered: a game without save states (Xbox, `hakux`) would lose
    /// where the player is. The row's description says so.
    NextStart,
    /// pocket-ui's own CPU and GPU profile for the running game (`standalone::set_performance`).
    Power,
}

#[derive(Debug)]
pub struct Setting {
    pub name: &'static str,
    pub desc: &'static str,
    pub store: Store,
    /// Where the emulator keeps the value: file, section, key (PINE takes the same section and key).
    pub ini: (BaseFile, &'static str, &'static str),
    pub choices: &'static [Choice],
    pub live: Live,
}

/// The emulator's own settings menu, opened from "Advanced…".
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Advanced {
    /// InputPlumber buttons pressed together (ARMSX2's OpenPauseMenu hotkey).
    Chord(&'static [&'static str]),
}

#[derive(Debug)]
pub struct Table {
    pub title: &'static str,
    /// ROCKNIX's system name, the prefix of its system.cfg keys.
    pub platform: &'static str,
    pub settings: &'static [Setting],
    pub advanced: Option<Advanced>,
    /// A page of further settings behind a "More…" row (Dolphin: ROCKNIX's per-game keys).
    pub more: Option<&'static Table>,
}

const OFF_ON_LOWER: &[Choice] = &[c("Off", "false", "false"), c("On", "true", "true")];
const OFF_ON_DOLPHIN: &[Choice] = &[c("Off", "False", "False"), c("On", "True", "True")];

/// system.cfg key of the CPU and GPU profile of a standalone game: pocket-ui's own (`standalone::set_performance`).
const PERFORMANCE_KEY: &str = "pocket_performance";

/// How hard the device works for a PS2, GameCube, Wii or 3DS game. Unless chosen here: Games: performance of Settings
/// → Performance & fan and Quick settings, where Auto is Max for these systems ([`default_performance`]). Measured on
/// the Mini V2 with Mario Kart: Double Dash: ROCKNIX's ondemand left the prime core at 0.96 GHz and the GPU at 305–400
/// MHz. The middle choice was called "Balanced" until 2026-10-23; system.cfg keeps its value `balanced`.
const PERFORMANCE: Setting = Setting {
    name: "Performance",
    desc: "Max: CPU and GPU at full speed, smoothest, but warmer and uses more battery. Auto: the system decides (ROCKNIX's own setting for this system). Battery: lower top speeds, light games only.",
    store: Store::Es(PERFORMANCE_KEY),
    ini: (BaseFile::SystemCfg, "", PERFORMANCE_KEY),
    choices: &[
        c("Max", "max", "max"),
        c("Auto", "balanced", "balanced"),
        c("Battery", "battery", "battery"),
    ],
    live: Live::Power,
};

/// Games: performance (Settings → Playing → Performance & fan, Quick settings) as the index of its [`PERFORMANCE`] choice for a
/// PS2, GameCube, Wii or 3DS game without one of its own: Battery is Battery, Auto and Max are Max (these systems need
/// full speed: "Auto: the system decides; for PS2, GameCube and 3DS that is full speed"). Set by the App whenever the
/// choice changes; read by the Options page and by `standalone` when a game starts, without a file.
static DEFAULT_PERFORMANCE: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// The stored value (`max`, `battery`) a PS2, GameCube, Wii or 3DS game without a Performance of its own runs with
/// when Games: performance is `games`.
pub fn performance_for_default(games: crate::power::Performance) -> &'static str {
    PERFORMANCE.choices[default_index(games)].stored
}

fn default_index(games: crate::power::Performance) -> usize {
    match games {
        crate::power::Performance::Battery => 2,
        _ => 0,
    }
}

/// Games: performance changed (or was read at the start): what the big systems' games without a choice of their own
/// run with from now on.
pub fn set_default_performance(games: crate::power::Performance) {
    DEFAULT_PERFORMANCE.store(default_index(games), std::sync::atomic::Ordering::Relaxed);
}

/// The stored value of [`set_default_performance`]'s choice.
pub fn default_performance() -> &'static str {
    PERFORMANCE.choices[DEFAULT_PERFORMANCE.load(std::sync::atomic::Ordering::Relaxed).min(PERFORMANCE.choices.len() - 1)].stored
}

/// The Performance choice of a game (`max`, `balanced`, `battery`): its own (or its platform's), else `default`.
pub fn game_performance_or(platform: &str, cfg: &str, file: &str, default: &'static str) -> &'static str {
    let value = es_get(cfg, platform, file, PERFORMANCE_KEY);
    PERFORMANCE
        .choices
        .iter()
        .find(|c| value.as_deref() == Some(c.stored))
        .map_or(default, |c| c.stored)
}

/// [`game_performance_or`] with the default of the moment ([`default_performance`]).
pub fn game_performance(platform: &str, cfg: &str, file: &str) -> &'static str {
    game_performance_or(platform, cfg, file, default_performance())
}

/// PS2: every value is ROCKNIX's own per-game setting (start_armsx2.sh writes it into PCSX2.ini at launch),
/// and live through PINE SetSetting, which ARMSX2 applies on its CPU thread (a renderer switch reopens the GS).
pub static PS2: Table = Table {
    title: "ARMSX2",
    platform: "ps2",
    settings: &[
        Setting {
            name: "Resolution",
            desc: "Internal resolution. 2x matches the screen; 3x makes the GPU clock up for a sharper picture.",
            store: Store::Es("internal_resolution"),
            ini: (BaseFile::Pcsx2, "EmuCore/GS", "upscale_multiplier"),
            // Quarter steps as in ARMSX2's own menu; start_armsx2.sh copies the value into PCSX2.ini as it is.
            choices: &[
                c("1x", "1", "1"),
                c("1.25x", "1.25", "1.25"),
                c("1.5x", "1.5", "1.5"),
                c("1.75x", "1.75", "1.75"),
                c("2x", "2", "2"),
                c("2.25x", "2.25", "2.25"),
                c("2.5x", "2.5", "2.5"),
                c("2.75x", "2.75", "2.75"),
                c("3x", "3", "3"),
            ],
            live: Live::Pine,
        },
        Setting {
            name: "Renderer",
            desc: "Vulkan is the fast one here. OpenGL runs at about half speed on this GPU.",
            store: Store::Es("graphics_backend"),
            ini: (BaseFile::Pcsx2, "EmuCore/GS", "Renderer"),
            choices: &[
                Choice {
                    label: "Vulkan",
                    stored: "2",
                    emu: "14",
                    also: &["-1"],
                },
                c("OpenGL", "1", "12"),
            ],
            live: Live::Pine,
        },
        Setting {
            name: "Widescreen",
            desc: "Widescreen patch for games that have one in ARMSX2's patch list. Set the aspect ratio to 16:9 with it.",
            store: Store::Es("enable_widescreen_patches"),
            ini: (BaseFile::Pcsx2, "EmuCore", "EnableWideScreenPatches"),
            choices: OFF_ON_LOWER,
            live: Live::Pine,
        },
        Setting {
            name: "Aspect ratio",
            desc: "Shape of the picture. 16:9 for widescreen games or the widescreen patch.",
            store: Store::Es("aspect_ratio"),
            ini: (BaseFile::Pcsx2, "EmuCore/GS", "AspectRatio"),
            choices: &[
                Choice {
                    label: "4:3",
                    stored: "0",
                    emu: "4:3",
                    also: &["Auto 4:3/3:2"],
                },
                c("16:9", "1", "16:9"),
                c("Stretch", "2", "Stretch"),
            ],
            live: Live::Pine,
        },
        Setting {
            name: "Show FPS",
            desc: "Frames per second in the corner.",
            store: Store::Es("show_fps"),
            ini: (BaseFile::Pcsx2, "EmuCore/GS", "OsdShowFPS"),
            choices: OFF_ON_LOWER,
            live: Live::Pine,
        },
        PERFORMANCE,
    ],
    // ROCKNIX's PCSX2.ini binds OpenPauseMenu to Guide + FaceNorth (HOME + X on the Mini V2). InputPlumber's West
    // is what arrives as the north face button (measured, as with Dolphin's hotkeys).
    advanced: Some(Advanced::Chord(&["Gamepad:Button:Guide", "Gamepad:Button:West"])),
    more: None,
};

/// GameCube and Wii: the game's own INI in Dolphin's user GameSettings, which wins over GFX.ini and Dolphin.ini
/// (start_dolphin_gc.sh and start_dolphin_wii.sh rewrite those two on every start, but never the game INIs).
const DOLPHIN_SETTINGS: &[Setting] = &[
    Setting {
        name: "Resolution",
        desc: "Internal resolution. 1x is the console's own 480 lines; 2x is about the screen's height.",
        store: Store::GameIni("Video_Settings", "InternalResolution"),
        ini: (BaseFile::DolphinGfx, "Settings", "InternalResolution"),
        // ROCKNIX's patch 004 doubles the scale: 2 is 1x.
        // Half steps; 1.5x and 2.5x are not on Dolphin's resolution hotkey, so they apply by a restart.
        choices: &[
            c("1x", "2", "2"),
            c("1.5x", "3", "3"),
            c("2x", "4", "4"),
            c("2.5x", "5", "5"),
            c("3x", "6", "6"),
        ],
        live: Live::Toggle(Toggle::Resolution),
    },
    Setting {
        name: "Renderer",
        desc: "Vulkan runs steadier here than OpenGL.",
        store: Store::GameIni("Core", "GFXBackend"),
        ini: (BaseFile::DolphinMain, "Core", "GFXBackend"),
        choices: &[c("Vulkan", "Vulkan", "Vulkan"), c("OpenGL", "OGL", "OGL")],
        live: Live::Restart,
    },
    Setting {
        name: "Widescreen",
        desc: "Dolphin's widescreen hack: draws more to the sides in 3D games. Set the aspect ratio to 16:9 with it.",
        store: Store::GameIni("Video_Settings", "wideScreenHack"),
        ini: (BaseFile::DolphinGfx, "Settings", "wideScreenHack"),
        choices: OFF_ON_DOLPHIN,
        live: Live::Restart,
    },
    Setting {
        name: "Aspect ratio",
        desc: "Shape of the picture. Auto follows the game.",
        store: Store::GameIni("Video_Settings", "AspectRatio"),
        ini: (BaseFile::DolphinGfx, "Settings", "AspectRatio"),
        choices: &[c("Auto", "0", "0"), c("16:9", "1", "1"), c("4:3", "2", "2"), c("Stretch", "3", "3")],
        live: Live::Toggle(Toggle::Aspect),
    },
    Setting {
        name: "Frame skip",
        desc: "Dolphin's VBI skip: when the game runs slow, it skips frames to keep the sound smooth.",
        store: Store::GameIni("Video_Hacks", "VISkip"),
        ini: (BaseFile::DolphinGfx, "Hacks", "VISkip"),
        choices: &[c("Off", "False", "False"), c("When slow", "True", "True")],
        live: Live::Restart,
    },
    Setting {
        name: "Show FPS",
        desc: "Frames per second in the corner.",
        store: Store::GameIni("Video_Settings", "ShowFPS"),
        ini: (BaseFile::DolphinGfx, "Settings", "ShowFPS"),
        choices: OFF_ON_DOLPHIN,
        live: Live::Toggle(Toggle::Fps),
    },
    PERFORMANCE,
];

pub static GAMECUBE: Table = Table {
    title: "Dolphin",
    platform: "gamecube",
    settings: DOLPHIN_SETTINGS,
    // Dolphin nogui has no settings UI of its own: ROCKNIX's per-game keys stand in for it.
    advanced: None,
    more: Some(&GC_MORE),
};

/// Wii: the same Dolphin, the same game INIs and the same hotkeys (they read the GameCube pad of port 1, which
/// start_dolphin_wii.sh sets up for them). The emulated Wiimote is kept deaf to the hotkey chords
/// (`standalone::defaults::with_quiet_wiimote`); with the player's own Wiimote profile the hotkeys are not
/// pressed at all and every change applies by a restart (`hotkey_profile`).
pub static WII: Table = Table {
    title: "Dolphin",
    platform: "wii",
    settings: DOLPHIN_SETTINGS,
    advanced: None,
    more: Some(&WII_MORE),
};

// "More…" for Dolphin: the per-game keys start_dolphin_gc.sh and start_dolphin_wii.sh (ROCKNIX 20261001) read through
// `get_setting` on every start and write into GCPadNew.ini / WiimoteNew.ini, Dolphin.ini and GFX.ini. Each first
// choice is what the script does when the key is unset; the stored values are the ones the script tests for.
// Everything here applies by a restart (the INIs are only read at boot).

const fn more(name: &'static str, desc: &'static str, key: &'static str, choices: &'static [Choice]) -> Setting {
    Setting {
        name,
        desc,
        store: Store::Es(key),
        // Not set anywhere: ROCKNIX's script falls back to its own default, the first choice.
        ini: (BaseFile::SystemCfg, "", key),
        choices,
        live: Live::Restart,
    }
}

/// `gamecube_controller_profile`: "east" and "custom" copy those profiles, anything else the south one.
const GC_LAYOUT: Setting = more(
    "Button layout",
    "A right: GameCube A on the right button, as printed. A bottom: A on the bottom button. Custom: your own profile.",
    "gamecube_controller_profile",
    &[c("A bottom", "south", "south"), c("A right", "east", "east"), c("Custom", "custom", "custom")],
);
/// Index of "Custom" in `GC_LAYOUT` and `WII_CONTROLLER`: offered only when the player made a profile of their own.
const GC_CUSTOM: usize = 2;
const WII_CUSTOM: usize = 4;
/// `swap_shoulders`: "true" takes the `inline` variant of the profile, anything else `stacked`.
const GC_SHOULDERS: Setting = more(
    "Shoulders",
    "Triggers: GameCube L and R on L2 and R2, Z on R1. Bumpers: L and R on L1 and R1, Z on R2.",
    "swap_shoulders",
    &[c("Triggers", "false", "false"), c("Bumpers", "true", "true")],
);
/// `wii_controller_profile`: classic, vremote, nunchuck, custom; anything else the sideways Wii Remote (hremote).
const WII_CONTROLLER: Setting = more(
    "Controller",
    "How the Wii Remote is held: sideways, upright, with a Nunchuk, or a Classic Controller. Custom: your own profile.",
    "wii_controller_profile",
    &[
        c("Sideways", "hremote", "hremote"),
        c("Upright", "vremote", "vremote"),
        c("Nunchuk", "nunchuck", "nunchuck"),
        c("Classic", "classic", "classic"),
        c("Custom", "custom", "custom"),
    ],
);
/// `rumble`: "false" deletes the profile's Rumble lines.
const RUMBLE: Setting = more(
    "Rumble",
    "Vibration when the game rumbles.",
    "rumble",
    &[c("On", "true", "true"), c("Off", "false", "false")],
);
/// `shader_mode` → GFX.ini ShaderCompilationMode (0 synchronous, 1 exclusive and 2 hybrid ubershaders, 3 asynchronous
/// skip drawing); unset keeps ROCKNIX's GFX.ini, which has 0. pocket-ui sets 3 for both platforms at start when unset
/// (`START_DEFAULTS`): with 0 Mario Kart: Double Dash kept dropping to 30 fps on the Mini V2.
const SHADERS: Setting = more(
    "Shaders",
    "A new effect needs a shader first. Skip until ready: no stutter, an effect can be missing for a moment. Pause to compile: a stutter each time. Ubershaders: no stutter, much more GPU work, often too much here.",
    "shader_mode",
    &[
        c("Pause to compile", "0", "0"),
        c("Skip until ready", "3", "3"),
        c("Hybrid ubershaders", "2", "2"),
        c("Ubershaders only", "1", "1"),
    ],
);
/// `shader_precompile` → WaitForShadersBeforeStarting; unset keeps ROCKNIX's GFX.ini, which has True.
const SHADERS_AT_START: Setting = more(
    "Shaders at start",
    "Compiles the shaders this game used before while it starts: a longer start, fewer stutters later.",
    "shader_precompile",
    &[c("On", "true", "true"), c("Off", "false", "false")],
);
/// `vsync`: "1" → VSync = True, anything else False.
const VSYNC: Setting = more(
    "VSync",
    "Waits for the screen: no tearing, a little more input lag. Off is what was tested with the pause menu.",
    "vsync",
    &[c("Off", "0", "0"), c("On", "1", "1")],
);
/// `clock_speed` 050 075 100 125 150 200 300 400 → Overclock; anything else leaves it at 100 %.
const CPU_CLOCK: Setting = more(
    "CPU clock",
    "The emulated console's CPU. Higher can fix a game that slows down by itself, but costs this device's CPU. Lower can help heavy games; some then run wrong.",
    "clock_speed",
    &[c("100%", "100", "100"), c("125%", "125", "125"), c("150%", "150", "150"), c("200%", "200", "200"), c("75%", "075", "075")],
);
/// `sync_gpu_thread`: "true" → SyncGPU = True, anything else False.
const SYNC_GPU: Setting = more(
    "Sync GPU thread",
    "Keeps the emulated CPU and GPU in step. Fixes crashes and glitches in a few games, costs speed.",
    "sync_gpu_thread",
    &[c("Off", "false", "false"), c("On", "true", "true")],
);
/// `skip_efb_cpu_access`: "false" → EFBAccessEnable = False (Dolphin's "Skip EFB Access from CPU" ticked);
/// anything else True. ROCKNIX's name reads backwards: its "false" is the skipping, faster one.
const EFB_READS: Setting = more(
    "EFB reads by CPU",
    "Accuracy vs speed. Some games read the picture back (aiming, lens flares); Fast skips that and breaks them.",
    "skip_efb_cpu_access",
    &[c("Accurate", "true", "true"), c("Fast", "false", "false")],
);
/// `store_efb_to_texture_only`: "false" → EFBToTextureEnable = False; anything else True.
const EFB_COPIES: Setting = more(
    "EFB copies",
    "Accuracy vs speed. Fast keeps effect copies on the GPU; a few games need them in memory, for a map or a camera.",
    "store_efb_to_texture_only",
    &[c("Fast", "true", "true"), c("Accurate", "false", "false")],
);
/// `store_xfb_to_texture_only`: "false" → XFBToTextureEnable = False; anything else True.
const XFB_COPIES: Setting = more(
    "XFB copies",
    "Accuracy vs speed. Fast keeps the finished picture on the GPU; videos or screen effects in a few games need it in memory.",
    "store_xfb_to_texture_only",
    &[c("Fast", "true", "true"), c("Accurate", "false", "false")],
);
/// `texture_cache_accuracy`: 0 and 512 as they are, anything else 128 → SafeTextureCacheColorSamples.
const TEXTURE_CACHE: Setting = more(
    "Texture cache",
    "Accuracy vs speed. Fast can show a wrong texture in a few games; Accurate checks every texture and is slower.",
    "texture_cache_accuracy",
    &[c("Fast", "128", "128"), c("Medium", "512", "512"), c("Accurate", "0", "0")],
);

/// GameCube's "More…". Left out: cheats (ROCKNIX's game INIs list cheats but nothing here picks them) and
/// auto disc swap (on unless set; turning it off only breaks two-disc games).
pub static GC_MORE: Table = Table {
    title: "Dolphin   More",
    platform: "gamecube",
    settings: &[
        GC_LAYOUT,
        GC_SHOULDERS,
        RUMBLE,
        SHADERS,
        SHADERS_AT_START,
        VSYNC,
        CPU_CLOCK,
        SYNC_GPU,
        EFB_READS,
        EFB_COPIES,
        XFB_COPIES,
        TEXTURE_CACHE,
    ],
    advanced: None,
    more: None,
};

/// Wii's "More…": start_dolphin_wii.sh reads no GameCube layout or shoulders (it always copies the south profile
/// for the hotkeys), so those rows give way to the Wii Remote's.
pub static WII_MORE: Table = Table {
    title: "Dolphin   More",
    platform: "wii",
    settings: &[
        WII_CONTROLLER,
        RUMBLE,
        SHADERS,
        SHADERS_AT_START,
        VSYNC,
        CPU_CLOCK,
        SYNC_GPU,
        EFB_READS,
        EFB_COPIES,
        XFB_COPIES,
        TEXTURE_CACHE,
    ],
    advanced: None,
    more: None,
};

/// Platform values pocket-ui sets at start where the player has none: GameCube A on the right button (`east`), like
/// the letters on the device and its other systems; Dolphin's asynchronous shaders (`3`) for GameCube and Wii.
pub const START_DEFAULTS: &[(&str, &str)] = &[
    ("gamecube.gamecube_controller_profile", "east"),
    ("gamecube.shader_mode", "3"),
    ("wii.shader_mode", "3"),
];

/// system.cfg with each of `defaults` added where the key has no value (a choice is never replaced). None:
/// nothing to change.
pub fn with_unset_defaults(cfg: &str, defaults: &[(&str, &str)]) -> Option<String> {
    let new = defaults.iter().fold(cfg.to_string(), |t, (key, value)| {
        if es_line(&t, key).map(str::trim).is_some_and(|v| !v.is_empty()) {
            t
        } else {
            es_set(&t, key, Some(value))
        }
    });
    (new != cfg).then_some(new)
}

/// The button layout alone (`START_DEFAULTS`' first).
pub fn with_gamecube_layout_default(cfg: &str) -> Option<String> {
    with_unset_defaults(cfg, &START_DEFAULTS[..1])
}

/// At pocket-ui's start (`START_DEFAULTS`), like the GPU's stock clock: never over a choice.
/// Written under ROCKNIX's lock (`system_cfg`).
/// [`apply_start_defaults`] once per run: put off at start until the first frame (on a thread), and called again
/// before a game starts, which waits for it if that thread is still at it.
pub fn apply_start_defaults_once() {
    static DONE: std::sync::Once = std::sync::Once::new();
    DONE.call_once(apply_start_defaults);
}

pub fn apply_start_defaults() {
    if !Path::new(SYSTEM_CFG).exists() {
        return;
    }
    match crate::system_cfg::edit(|cfg| with_unset_defaults(cfg, START_DEFAULTS)) {
        Ok(false) => {}
        Ok(true) => println!("emu_settings: defaults for what nobody chose (GameCube A right, Dolphin shaders): set"),
        Err(e) => println!("emu_settings: defaults for what nobody chose (GameCube A right, Dolphin shaders): failed ({e})"),
    }
}

/// Whether a custom controller profile is the player's own and not ROCKNIX's copy of a stock one (the start scripts
/// make `custom` from the south GameCube profile and from the upright Wii Remote one).
pub fn custom_profile_is_own(custom: &str, stock: &str) -> bool {
    let lines = |t: &str| t.lines().map(str::trim).filter(|l| !l.is_empty()).map(str::to_string).collect::<Vec<_>>();
    !lines(custom).is_empty() && lines(custom) != lines(stock)
}

/// system.cfg key of the 3DS texture filter: pocket-ui's own, ROCKNIX has none.
const AZAHAR_TEXTURE_FILTER: &str = "pocket_texture_filter";
/// ROCKNIX's MangoHud switch: runemu.sh starts the emulator through `mangohud` when it is 1.
const MANGOHUD_KEY: &str = "rocknix.mangohud.enabled";

/// 3DS: Azahar has no channel for settings while a game runs (Qt shortcuts only), so every change applies by a
/// restart. Resolution, screens and renderer are ROCKNIX's own per-game keys, which start_azahar.sh writes into
/// qt-config.ini on every start; the texture filter is written there by pocket-ui before each start
/// (`azahar_launch_config`), and the FPS counter is ROCKNIX's MangoHud switch with a config of ours that shows
/// only the FPS (`game_env`): Azahar's own counter sits in its status bar, which fullscreen hides.
pub static N3DS: Table = Table {
    title: "Azahar",
    platform: "3ds",
    settings: &[
        Setting {
            name: "Resolution",
            desc: "Internal resolution. 1x is the 3DS's own 400x240; 2x and 3x are sharper.",
            store: Store::Es("resolution_scale"),
            ini: (BaseFile::AzaharQt, "Renderer", "resolution_factor"),
            choices: &[c("1x", "1", "1"), c("2x", "2", "2"), c("3x", "3", "3")],
            live: Live::Restart,
        },
        Setting {
            name: "Screens",
            desc: "How the two 3DS screens share this one. The bottom one is the touch screen.",
            store: Store::Es("screen_layout"),
            ini: (BaseFile::AzaharQt, "Layout", "layout_option"),
            // start_azahar.sh: 0 (its default) top above bottom, 2 large top and small bottom, 1a top only.
            choices: &[c("Stacked", "0", "0"), c("Large top", "2", "2"), c("Top only", "1a", "1")],
            live: Live::Restart,
        },
        Setting {
            name: "Renderer",
            desc: "Vulkan is ROCKNIX's default here; OpenGL for a game that draws wrong with it.",
            store: Store::Es("graphics_backend"),
            ini: (BaseFile::AzaharQt, "Renderer", "graphics_api"),
            choices: &[c("Vulkan", "2", "2"), c("OpenGL", "1", "1")],
            live: Live::Restart,
        },
        Setting {
            name: "Texture filter",
            desc: "Smooths textures when they are scaled up. ScaleForce and xBRZ cost GPU time.",
            store: Store::Es(AZAHAR_TEXTURE_FILTER),
            ini: (BaseFile::AzaharQt, "Renderer", "texture_filter"),
            // Azahar's TextureFilter: 0 none, 2 Bicubic, 3 ScaleForce, 4 xBRZ (1 Anime4K and 5 MMPX left out).
            choices: &[c("Off", "0", "0"), c("Bicubic", "2", "2"), c("ScaleForce", "3", "3"), c("xBRZ", "4", "4")],
            live: Live::Restart,
        },
        Setting {
            name: "Show FPS",
            desc: "Frames per second in the corner (MangoHud).",
            store: Store::Es(MANGOHUD_KEY),
            ini: (BaseFile::SystemCfg, "", MANGOHUD_KEY),
            choices: &[c("Off", "0", "0"), c("On", "1", "1")],
            live: Live::Restart,
        },
        PERFORMANCE,
    ],
    // Azahar's menu bar is hidden in fullscreen, and no shortcut opens its settings.
    advanced: None,
    more: None,
};

/// Xbox: hakuX in RetroArch (`hakux.rs`). Both rows are pocket-ui's own per-game keys in system.cfg, written into RetroArch's
/// appendconfig and the core's options at every start; the Xbox default is the same key without a game (`xbox.<key>`) or the
/// first choice. The core reads its resolution once before the console boots, and RetroArch has no command for the picture
/// size, so both apply at the next start (`Live::NextStart`: a restart would lose the game, there are no save states yet).
pub static XBOX: Table = Table {
    title: "hakuX",
    platform: "xbox",
    settings: &[
        Setting {
            name: "Resolution",
            desc: "The resolution the Xbox's picture is drawn at. Original is the Xbox's own 480p; 2x (960p) is sharper and costs the GPU more. Takes effect the next time the game starts.",
            store: Store::Es(crate::hakux::RESOLUTION_KEY),
            ini: (BaseFile::SystemCfg, "", crate::hakux::RESOLUTION_KEY),
            choices: &[c("Original (480p)", "1", "1"), c("2x (960p)", "2", "2")],
            live: Live::NextStart,
        },
        Setting {
            name: "Screen scaling",
            desc: "Aspect is as large as fits, in the picture's own shape (4:3), with black bars.\nFullscreen fills the screen and stretches the picture.\nInteger scales by whole numbers. Takes effect the next time the game starts.",
            store: Store::Es(crate::hakux::SCALING_KEY),
            ini: (BaseFile::SystemCfg, "", crate::hakux::SCALING_KEY),
            choices: &[c("Aspect", "aspect", "aspect"), c("Fullscreen", "fullscreen", "fullscreen"), c("Integer", "integer", "integer")],
            live: Live::NextStart,
        },
        PERFORMANCE,
    ],
    advanced: None,
    more: None,
};

/// The options of a system's emulator: PS2, GameCube, Wii, 3DS and Xbox.
pub fn table(emu: Emu, system: &str) -> Option<&'static Table> {
    match (emu, system) {
        (Emu::Armsx2, _) => Some(&PS2),
        (Emu::Dolphin, "gamecube") => Some(&GAMECUBE),
        (Emu::Dolphin, "wii") => Some(&WII),
        (Emu::Azahar, "3ds") => Some(&N3DS),
        (Emu::Hakux, "xbox") => Some(&XBOX),
        _ => None,
    }
}

// ---------------------------------------------------------------------------------------------------------
// INI files (PCSX2.ini, Dolphin's INIs): `key = value` under `[section]`, '#' and ';' comments.

/// The key of a `key = value` line; None for comments, sections and cheat lines (`$name`, `+$name`).
fn ini_key(line: &str) -> Option<&str> {
    let t = line.trim();
    if t.is_empty() || t.starts_with(['#', ';', '[', '$', '+', '*']) {
        return None;
    }
    t.split_once('=').map(|(k, _)| k.trim())
}

/// The value of `key` in `[section]`.
pub fn ini_get(text: &str, section: &str, key: &str) -> Option<String> {
    let header = format!("[{section}]");
    let mut inside = false;
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            inside = t == header;
        } else if inside && ini_key(line) == Some(key) {
            return t.split_once('=').map(|(_, v)| v.trim().to_string());
        }
    }
    None
}

/// Sets (or with `None` removes) `key` in `[section]`, adding the key or the section when missing; every
/// other line stays as it was. A section that appears more than once counts as one, as INI readers merge it.
pub fn ini_set(text: &str, section: &str, key: &str, value: Option<&str>) -> String {
    let header = format!("[{section}]");
    let lines: Vec<&str> = text.lines().collect();
    // The key's lines in every part of the section, and where the first part ends.
    let (mut inside, mut found, mut first_end, mut seen) = (false, Vec::new(), None, false);
    for (i, line) in lines.iter().enumerate() {
        if line.trim().starts_with('[') {
            if inside && first_end.is_none() {
                first_end = Some(i);
            }
            inside = line.trim() == header;
            seen |= inside;
        } else if inside && ini_key(line) == Some(key) {
            found.push(i);
        }
    }
    if inside && first_end.is_none() {
        first_end = Some(lines.len());
    }
    let new_line = value.map(|v| format!("{key} = {v}"));
    let mut out: Vec<String> = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if found.first() == Some(&i) {
            out.extend(new_line.clone());
        } else if !found.contains(&i) {
            out.push(line.to_string());
        }
    }
    if let (Some(new), true) = (new_line, found.is_empty()) {
        match first_end.filter(|_| seen) {
            Some(end) => {
                // After the section's last line, before the blank lines that separate it from the next one.
                let at = (0..end).rev().find(|&i| !lines[i].trim().is_empty()).map_or(end, |i| i + 1);
                out.insert(at, new);
            }
            None => {
                if out.iter().any(|l| !l.trim().is_empty()) {
                    out.push(String::new());
                } else {
                    out.clear();
                }
                out.push(header);
                out.push(new);
            }
        }
    }
    let mut s = out.join("\n");
    s.push('\n');
    s
}

// ---------------------------------------------------------------------------------------------------------
// ROCKNIX's system.cfg

pub const SYSTEM_CFG: &str = "/storage/.config/system/configs/system.cfg";

/// `<platform>["<file>"].<key>`, ROCKNIX's per-game key.
pub fn es_game_key(platform: &str, file: &str, key: &str) -> String {
    format!("{platform}[\"{file}\"].{key}")
}

fn es_line<'a>(cfg: &'a str, key: &str) -> Option<&'a str> {
    cfg.lines().find_map(|l| l.strip_prefix(key).and_then(|rest| rest.strip_prefix('=')))
}

/// The value ROCKNIX's `get_setting <key> <platform> <file>` finds: the game's, the platform's, then
/// `system.` and `global.`.
pub fn es_get(cfg: &str, platform: &str, file: &str, key: &str) -> Option<String> {
    [
        es_game_key(platform, file, key),
        format!("{platform}.{key}"),
        format!("system.{key}"),
        format!("global.{key}"),
    ]
    .iter()
    .find_map(|k| es_line(cfg, k).map(str::trim).filter(|v| !v.is_empty()))
    .map(str::to_string)
}

/// Sets (or with `None` removes) the line `key=value` of system.cfg.
pub fn es_set(cfg: &str, key: &str, value: Option<&str>) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut done = false;
    for line in cfg.lines() {
        if line.strip_prefix(key).is_some_and(|r| r.starts_with('=')) {
            if let (Some(v), false) = (value, done) {
                out.push(format!("{key}={v}"));
            }
            done = true;
        } else {
            out.push(line.to_string());
        }
    }
    if let (Some(v), false) = (value, done) {
        out.push(format!("{key}={v}"));
    }
    let mut s = out.join("\n");
    if !s.is_empty() {
        s.push('\n');
    }
    s
}

/// Whether ROCKNIX's `get_setting` can find a per-game key for this file name: it builds an awk regex from
/// the name escaping only `'`, `(`, `)` and `&`, after an unquoted `echo` that squeezes blanks.
pub fn es_findable(file: &str) -> bool {
    let regex_or_glob = |ch: char| "[]*?+{}|^$\\\"/".contains(ch) || ch.is_control();
    !file.is_empty() && file.trim() == file && !file.contains("  ") && !file.chars().any(regex_or_glob)
}

// ---------------------------------------------------------------------------------------------------------
// Per-game file names

pub const PCSX2_INI: &str = "/storage/.config/ARMSX2/inis/PCSX2.ini";
const ARMSX2_GAMESETTINGS: &str = "/storage/.config/ARMSX2/gamesettings";
const DOLPHIN_DIR: &str = "/storage/.config/dolphin-emu";

/// ARMSX2's per-game INI for the disc that standalone.rs knows as "<serial> (<CRC>)":
/// `gamesettings/<serial>_<CRC>.ini`, as VMManager::GetGameSettingsPath names it.
pub fn armsx2_game_ini(disc: &str) -> Option<String> {
    let (serial, crc) = disc.rsplit_once(" (")?;
    let crc = u32::from_str_radix(crc.strip_suffix(')')?, 16).ok()?;
    // Path::SanitizeFileName: characters a file name can't hold become '_'.
    let serial: String = serial
        .chars()
        .map(|ch| if "\\/:*?\"<>|".contains(ch) || ch.is_control() { '_' } else { ch })
        .collect();
    Some(format!("{ARMSX2_GAMESETTINGS}/{serial}_{crc:08X}.ini"))
}

/// Dolphin's user INI of a game: `GameSettings/<GAMEID>.ini`, which wins over GFX.ini and Dolphin.ini.
pub fn dolphin_game_ini(id: &str) -> String {
    format!("{DOLPHIN_DIR}/GameSettings/{id}.ini")
}

/// The game ID Dolphin looks up the game INI by: the disc's (`header`, see `dolphin_game_id`), or for a DOL or
/// ELF executable "ID-<file stem>" (`SConfig::MakeGameID`, the name ROCKNIX's state names use for them too).
pub fn dolphin_settings_id(rom: &Path, header: Option<String>) -> Option<String> {
    let ext = rom.extension().and_then(|e| e.to_str()).unwrap_or_default().to_ascii_lowercase();
    match ext.as_str() {
        "dol" | "elf" => Some(format!("ID-{}", rom.file_stem()?.to_str()?)),
        _ => header,
    }
}

// ---------------------------------------------------------------------------------------------------------
// Azahar (3DS)

pub const AZAHAR_QT: &str = "/storage/.config/azahar/qt-config.ini";
/// MangoHud config for the 3DS "Show FPS": the counter alone, not ROCKNIX's full HUD.
pub const MANGOHUD_FPS_CONF: &str = "/storage/.config/pocket-ui/mangohud-fps.conf";

pub fn mangohud_fps_config() -> &'static str {
    "# Written by pocket-ui: the FPS counter of the pause menu's Show FPS (3DS).\nfps_only\nposition=top-right\nfont_size=32\nbackground_alpha=0.4\n"
}

/// qt-config.ini as this game starts with it: the texture filter chosen for it (or the 3DS platform's, or none),
/// written before every start because start_azahar.sh leaves that key alone. Every other line stays.
pub fn azahar_launch_config(qt: &str, cfg: &str, file: &str) -> String {
    let filter = N3DS
        .settings
        .iter()
        .find(|s| s.store == Store::Es(AZAHAR_TEXTURE_FILTER))
        .expect("the 3DS table has a texture filter");
    let value = es_get(cfg, N3DS.platform, file, AZAHAR_TEXTURE_FILTER)
        .and_then(|v| filter.choices.iter().find(|c| c.stored == v))
        .unwrap_or(&filter.choices[0]);
    crate::standalone::azahar_qt_set(qt, filter.ini.1, filter.ini.2, value.emu)
}

/// Environment for the game's start: with "Show FPS" on for this 3DS game, MangoHud (which runemu.sh starts it
/// through, from the same per-game key) reads our FPS-only config instead of ROCKNIX's full HUD.
pub fn game_env(system: &str, cfg: &str, file: &str) -> Vec<(&'static str, String)> {
    let fps = system == N3DS.platform && es_line(cfg, &es_game_key(system, file, MANGOHUD_KEY)).map(str::trim) == Some("1");
    if fps {
        vec![("MANGOHUD_CONFIGFILE", MANGOHUD_FPS_CONF.to_string())]
    } else {
        Vec::new()
    }
}

/// `game_env` for a game about to start, from ROCKNIX's system.cfg.
pub fn launch_env(system: &str, rom: &Path) -> Vec<(&'static str, String)> {
    game_env(
        system,
        &read(Path::new(SYSTEM_CFG)),
        rom.file_name().and_then(|f| f.to_str()).unwrap_or_default(),
    )
}

fn game_id_at(bytes: &[u8], at: usize) -> Option<String> {
    let id = bytes.get(at..at + 6)?;
    id.iter()
        .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
        .then(|| String::from_utf8_lossy(id).into_owned())
}

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

fn u64_at(b: &[u8], at: usize) -> Option<u64> {
    Some(u64::from_le_bytes(b.get(at..at + 8)?.try_into().ok()?))
}

/// The 6-character game ID Dolphin names a game's INI after, read from the disc image's header (plain
/// ISO/GCM, RVZ/WIA, CISO, WBFS, GCZ).
pub fn dolphin_game_id(image: &[u8]) -> Option<String> {
    dolphin_game_id_in(&mut std::io::Cursor::new(image))
}

/// `dolphin_game_id` on a file: reads only the header (and, for GCZ, the first block).
pub fn dolphin_game_id_in<R: std::io::Read + std::io::Seek>(r: &mut R) -> Option<String> {
    use std::io::{Read, SeekFrom};
    let mut read = |at: u64, len: usize| -> Option<Vec<u8>> {
        r.seek(SeekFrom::Start(at)).ok()?;
        let mut buf = Vec::with_capacity(len);
        r.by_ref().take(len as u64).read_to_end(&mut buf).ok()?;
        Some(buf)
    };
    let head = read(0, 0x60)?;
    match head.get(..4)? {
        // WIA header 1 is 0x48 bytes; header 2 starts with 4 u32s, then the disc header.
        b"RVZ\x01" | b"WIA\x01" => game_id_at(&head, 0x58),
        b"CISO" => game_id_at(&read(0x8000, 6)?, 0),
        // The disc header follows the 512-byte WBFS head.
        b"WBFS" => game_id_at(&read(0x200, 6)?, 0),
        _ if u32_at(&head, 0) == Some(0xB10B_C001) => {
            // GCZ: header, u64 block offsets, u32 hashes, then blocks; the top bit marks a stored block.
            let blocks = u32_at(&head, 0x1c)? as u64;
            let data = 0x20 + blocks * 12;
            let first = u64_at(&read(0x20, 8)?, 0)?;
            let end = if blocks > 1 {
                u64_at(&read(0x28, 8)?, 0)? & !(1 << 63)
            } else {
                u64_at(&head, 8)?
            };
            let offset = first & !(1 << 63);
            let block = read(data + offset, end.checked_sub(offset)?.min(1 << 20) as usize)?;
            if first >> 63 == 1 {
                game_id_at(&block, 0)
            } else {
                game_id_at(&miniz_oxide::inflate::decompress_to_vec_zlib(&block).ok()?, 0)
            }
        }
        // A plain image carries the GameCube (0x1C) or Wii (0x18) magic word.
        _ if head.get(0x1c..0x20) == Some(&[0xc2, 0x33, 0x9f, 0x3d]) || head.get(0x18..0x1c) == Some(&[0x5d, 0x1c, 0x9e, 0xa3]) => game_id_at(&head, 0),
        _ => None,
    }
}

// ---------------------------------------------------------------------------------------------------------
// PINE

pub const PINE_SET_SETTING: u8 = 0x12;

/// Arguments of PINE's Get/SetSetting: each string as `[u32 length][bytes]`.
pub fn pine_strings(parts: &[&str]) -> Vec<u8> {
    parts.iter().flat_map(|p| (p.len() as u32).to_le_bytes().into_iter().chain(p.bytes())).collect()
}

// ---------------------------------------------------------------------------------------------------------
// Dolphin's hotkeys

/// What ROCKNIX's hotkey loop remembers between presses (its `static int`s), from Dolphin's start.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Toggles {
    fps: bool,
    stretch: bool,
    ires: u8,
}

impl Toggles {
    /// One press of `t`; returns the INI value Dolphin switches to.
    pub fn press(&mut self, t: Toggle) -> &'static str {
        match t {
            Toggle::Fps => {
                self.fps = !self.fps;
                if self.fps {
                    "True"
                } else {
                    "False"
                }
            }
            Toggle::Aspect => {
                self.stretch = !self.stretch;
                if self.stretch {
                    "3"
                } else {
                    "0"
                }
            }
            Toggle::Resolution => {
                let (next, value) = match self.ires {
                    0 => (2, "2"),
                    2 => (4, "4"),
                    4 => (6, "6"),
                    _ => (0, "1"),
                };
                self.ires = next;
                value
            }
        }
    }

    /// How many presses of `t` take the setting from `now` to `want` (0 if it is there), or None if the
    /// hotkey never gets there (Dolphin's aspect hotkey only knows Auto and Stretch).
    pub fn presses(&self, t: Toggle, now: &str, want: &str) -> Option<u32> {
        if now.eq_ignore_ascii_case(want) {
            return Some(0);
        }
        let mut sim = *self;
        (1..=8).find(|_| sim.press(t).eq_ignore_ascii_case(want))
    }
}

/// The InputPlumber button for a GC face button in a Dolphin controller profile (`Buttons/Y = `Button N``).
/// Dolphin names buttons by their evdev code, and InputPlumber's West arrives as BTN_NORTH and its North as
/// BTN_WEST, so those two cross over (measured on the device).
pub fn profile_button(profile: &str, gc: &str) -> Option<&'static str> {
    let key = format!("Buttons/{gc}");
    let value = profile
        .lines()
        .find_map(|l| l.split_once('=').filter(|(k, _)| k.trim() == key).map(|(_, v)| v.trim()))?;
    let token = value.strip_prefix('`')?.split('`').next()?;
    Some(match token {
        "Button S" => "Gamepad:Button:South",
        "Button E" => "Gamepad:Button:East",
        "Button W" => "Gamepad:Button:North",
        "Button N" => "Gamepad:Button:West",
        _ => return None,
    })
}

/// The GC button behind each hotkey, as patch 000-hotkeys reads them.
fn toggle_gc_button(t: Toggle) -> &'static str {
    match t {
        Toggle::Fps => "Y",
        Toggle::Aspect => "X",
        Toggle::Resolution => "B",
    }
}

// ---------------------------------------------------------------------------------------------------------
// Applying by restart: save the resume state, let the emulator close, start the game again; its start loads
// the resume state (standalone.rs, the same path as Quit and the next start).

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Phase {
    /// The emulator writes the resume state (the game runs behind "Applying…").
    Saving,
    /// Saved; the emulator is shutting down.
    Closing,
    /// The game is being started again.
    Starting,
    /// Started again; the resume state is being loaded once the game runs.
    Loading,
    Done,
    /// Nothing was saved: the game goes on unchanged and the new values wait for the next start.
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Event {
    Saved(bool),
    Exited,
    Started,
    Loaded(bool),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Cmd {
    /// Write the resume state, then close the emulator once it is complete.
    SaveAndClose,
    /// Start the same game again.
    Relaunch,
    /// Load the resume state into the game that was started again (standalone.rs does this on every start
    /// of a standalone game, as after Quit).
    LoadResume,
    /// Give the game back as it was.
    BackToGame,
    /// The game is gone without a resume state: back to the shelf like any exit.
    Shelf,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Restart {
    pub phase: Phase,
}

impl Restart {
    pub fn start() -> (Restart, Cmd) {
        (Restart { phase: Phase::Saving }, Cmd::SaveAndClose)
    }

    pub fn on(&mut self, e: Event) -> Option<Cmd> {
        let (phase, cmd) = match (self.phase, e) {
            (Phase::Saving, Event::Saved(true)) => (Phase::Closing, None),
            (Phase::Saving, Event::Saved(false)) => (Phase::Failed, Some(Cmd::BackToGame)),
            (Phase::Saving, Event::Exited) => (Phase::Failed, Some(Cmd::Shelf)),
            (Phase::Closing, Event::Exited) => (Phase::Starting, Some(Cmd::Relaunch)),
            (Phase::Starting, Event::Started) => (Phase::Loading, Some(Cmd::LoadResume)),
            (Phase::Loading, Event::Loaded(_)) => (Phase::Done, None),
            (phase, _) => (phase, None),
        };
        self.phase = phase;
        cmd
    }
}

// ---------------------------------------------------------------------------------------------------------
// The running game

/// What this launch has done to the emulator: Dolphin's hotkey state and the values it runs with after them
/// (None: as it started).
struct Session {
    rom: std::path::PathBuf,
    toggles: Toggles,
    running: Vec<Option<String>>,
    /// The values the game started with, read when the menu first opens.
    boot: Option<Vec<String>>,
}

static SESSION: std::sync::Mutex<Option<Session>> = std::sync::Mutex::new(None);

/// Called before every launch (from main, next to `standalone::launch`). For PS2 it makes sure every key of
/// the table has a platform value in system.cfg, so ROCKNIX's start script rewrites each of them at every
/// launch: a per-game value (or one set live through PINE) never sticks to the next game. Names ROCKNIX's
/// lookup can't match get their per-game values through PINE once the game runs.
pub fn launch(emu: Option<Emu>, system: &str, rom: &Path) {
    let emu = emu.filter(|e| table(*e, system).is_some());
    if let Ok(mut s) = SESSION.lock() {
        *s = emu.map(|_| Session {
            rom: rom.to_path_buf(),
            toggles: Toggles::default(),
            running: Vec::new(),
            boot: None,
        });
    }
    if emu == Some(Emu::Azahar) {
        azahar_launch(rom);
    }
    if emu != Some(Emu::Armsx2) {
        return;
    }
    if let Ok(cfg) = std::fs::read_to_string(SYSTEM_CFG) {
        // Under ROCKNIX's lock, on the file as it is then (`system_cfg`).
        let _ = crate::system_cfg::edit(|now| {
            let new = PS2_PLATFORM_DEFAULTS
                .iter()
                .fold(now.to_string(), |t, (k, v)| if es_line(&t, k).is_some() { t } else { es_set(&t, k, Some(v)) });
            (new != now).then_some(new)
        });
        let file = rom.file_name().and_then(|f| f.to_str()).unwrap_or_default().to_string();
        let own: Vec<(&'static str, &'static str, String)> = PS2
            .settings
            .iter()
            .filter_map(|s| {
                let Store::Es(key) = s.store else { return None };
                let stored = es_line(&cfg, &es_game_key("ps2", &file, key))?;
                Some((s.ini.1, s.ini.2, s.choices.iter().find(|c| c.stored == stored.trim())?.emu.to_string()))
            })
            .collect();
        if !es_findable(&file) && !own.is_empty() {
            let rom = rom.to_path_buf();
            std::thread::spawn(move || {
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
                while crate::standalone::pine_status() != Some(0) {
                    if std::time::Instant::now() > deadline || !same_launch(&rom) {
                        return;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(250));
                }
                for (section, key, value) in own {
                    pine_set(section, key, &value);
                }
            });
        }
    }
}

/// Platform values for the PS2 keys, added to system.cfg only when the player has none: the same as
/// pocket-ui's tuned defaults (Vulkan) and ARMSX2's (4:3, no FPS counter).
const PS2_PLATFORM_DEFAULTS: &[(&str, &str)] = &[("ps2.graphics_backend", "2"), ("ps2.aspect_ratio", "0"), ("ps2.show_fps", "false")];

/// Before a 3DS start: this game's texture filter into qt-config.ini (created from ROCKNIX's by
/// `standalone::launch` just before), and the FPS-only MangoHud config when the game shows FPS.
fn azahar_launch(rom: &Path) {
    let cfg = read(Path::new(SYSTEM_CFG));
    let file = rom.file_name().and_then(|f| f.to_str()).unwrap_or_default();
    if let Ok(old) = std::fs::read_to_string(AZAHAR_QT) {
        let new = azahar_launch_config(&old, &cfg, file);
        if new != old {
            let _ = crate::fsutil::write_atomic(Path::new(AZAHAR_QT), new.as_bytes());
        }
    }
    if !game_env(N3DS.platform, &cfg, file).is_empty() && read(Path::new(MANGOHUD_FPS_CONF)) != mangohud_fps_config() {
        let _ = crate::fsutil::write_atomic(Path::new(MANGOHUD_FPS_CONF), mangohud_fps_config().as_bytes());
    }
}

fn same_launch(rom: &Path) -> bool {
    SESSION.lock().ok().is_some_and(|s| s.as_ref().is_some_and(|s| s.rom == rom))
}

fn pine_set(section: &str, key: &str, value: &str) -> bool {
    let ok = crate::standalone::pine_call(PINE_SET_SETTING, &pine_strings(&[section, key, value])).is_some();
    println!("emu_settings: PINE {section}/{key} = {value}: {}", if ok { "applied" } else { "failed" });
    ok
}

// ---------------------------------------------------------------------------------------------------------
// The pause menu page

/// Where this game's choices go.
#[derive(Clone, Debug)]
enum Target {
    /// system.cfg keys of this ROM file name (PS2), plus ARMSX2's own per-game INI when the disc is known.
    Es { file: String, armsx2_ini: Option<String> },
    /// Dolphin's GameSettings/<GAMEID>.ini, and the ROM file name for the system.cfg rows (Performance).
    DolphinIni(String, String),
    /// No per-game store found; why, for the page.
    None(&'static str),
}

/// A change and how it reaches the game.
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    Pine {
        section: &'static str,
        key: &'static str,
        value: &'static str,
    },
    /// HOTKEY + `button` (InputPlumber), `times` presses, moving Dolphin's `toggle` state along.
    Hotkey {
        toggle: Toggle,
        button: &'static str,
        times: u32,
        setting: usize,
    },
    /// The CPU and GPU profile (`max`, `balanced`, `battery`).
    Power(&'static str),
}

/// What leaving the menu has to do.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Plan {
    pub actions: Vec<Action>,
    pub restart: bool,
}

pub struct Menu {
    emu: Emu,
    table: &'static Table,
    target: Target,
    /// Current choice per setting (None: a value none of the choices stands for, shown as it is).
    values: Vec<Option<usize>>,
    raw: Vec<String>,
    applied: Vec<Option<usize>>,
    /// PS2: the setting is also in ARMSX2's own per-game INI (set in its menu), which PINE can't override.
    in_emu_ini: Vec<bool>,
    /// The value is one of ours in system.cfg (this game's, its platform's), not the emulator's or a default.
    own: Vec<bool>,
    /// Dolphin: the InputPlumber button of each hotkey (Fps, Aspect, Resolution).
    buttons: [Option<&'static str>; 3],
    /// How many of each setting's choices Left/Right step through ("Custom" only with a profile of one's own).
    limit: Vec<usize>,
    /// The "More…" page (`Table::more`), and whether it is the one showing.
    more: Option<Box<Menu>>,
    in_more: bool,
}

/// Files the menu reads, so tests and the snapshot can point it elsewhere.
pub struct Files {
    pub system_cfg: PathBuf,
    pub root: PathBuf,
}

impl Default for Files {
    fn default() -> Files {
        Files {
            system_cfg: PathBuf::from(SYSTEM_CFG),
            root: PathBuf::from("/"),
        }
    }
}

impl Files {
    /// Where the absolute device path `abs` is under this root (Pixel Path's settings reader uses the same roots).
    pub fn at(&self, abs: &str) -> PathBuf {
        self.root.join(abs.trim_start_matches('/'))
    }

    fn base(&self, f: BaseFile) -> PathBuf {
        self.at(match f {
            BaseFile::Pcsx2 => PCSX2_INI,
            BaseFile::DolphinGfx => "/storage/.config/dolphin-emu/GFX.ini",
            BaseFile::DolphinMain => "/storage/.config/dolphin-emu/Dolphin.ini",
            BaseFile::AzaharQt => AZAHAR_QT,
            BaseFile::SystemCfg => return self.system_cfg.clone(),
        })
    }
}

/// Sets these `key=value` lines of the system.cfg at `path` (replaced where they are, added otherwise), under ROCKNIX's
/// lock and on the file as it is then (`system_cfg`). Nothing to set: nothing written.
fn write_cfg_lines(path: &Path, lines: &[(String, &str)]) -> Result<(), String> {
    if lines.is_empty() {
        return Ok(());
    }
    crate::system_cfg::SystemCfg::at(path)
        .edit(|now| {
            let new = lines.iter().fold(now.to_string(), |t, (k, v)| es_set(&t, k, Some(v)));
            (new != now).then_some(new)
        })
        .map(|_| ())
        .map_err(|e| format!("system.cfg: {e}"))
}

fn read(p: &Path) -> String {
    std::fs::read_to_string(p).unwrap_or_default()
}

fn find_choice(s: &Setting, value: &str, stored: bool) -> Option<usize> {
    let num = |v: &str| v.trim().parse::<f32>().ok();
    let same = |a: &str, b: &str| a.trim().eq_ignore_ascii_case(b.trim()) || num(a).is_some_and(|x| num(b) == Some(x));
    s.choices.iter().position(|c| {
        if stored {
            same(c.stored, value)
        } else {
            same(c.emu, value) || c.also.iter().any(|a| same(a, value))
        }
    })
}

impl Menu {
    /// The page for the running standalone game, if its emulator has one.
    pub fn for_running(files: &Files) -> Option<Menu> {
        let crate::standalone::Running { emu, system, rom, disc } = crate::standalone::session()?;
        let menu = Menu::for_game(emu, &system, &rom, disc.as_deref(), files)?;
        if let Ok(mut s) = SESSION.lock() {
            if let Some(s) = s.as_mut().filter(|s| s.rom == rom && s.boot.is_none()) {
                s.boot = Some(menu.raw.clone());
                s.running = vec![None; menu.raw.len()];
            }
        }
        Some(menu)
    }

    /// The page for a game that is not running (the game's "This game" page on the shelf): the same rows, values and
    /// store as the pause menu's. `disc`: ARMSX2's "<serial> (<CRC>)", known only once the game runs.
    pub fn for_game(emu: Emu, system: &str, rom: &Path, disc: Option<&str>, files: &Files) -> Option<Menu> {
        let system = system.to_string();
        let rom = rom.to_path_buf();
        let disc = disc.map(str::to_string);
        let table = table(emu, &system)?;
        let target = match emu {
            Emu::Armsx2 => Target::Es {
                file: rom.file_name()?.to_str()?.to_string(),
                armsx2_ini: disc.as_deref().and_then(armsx2_game_ini),
            },
            Emu::Dolphin => {
                let header = std::fs::File::open(&rom).ok().and_then(|mut f| dolphin_game_id_in(&mut f));
                match dolphin_settings_id(&rom, header) {
                    Some(id) => Target::DolphinIni(dolphin_game_ini(&id), rom.file_name()?.to_str()?.to_string()),
                    None => Target::None("This game's file doesn't name its disc, so there is no place to keep settings for it."),
                }
            }
            // ROCKNIX's start script looks the per-game keys up by the file name, and can't with some characters.
            Emu::Azahar | Emu::Hakux => match rom.file_name()?.to_str()? {
                file if es_findable(file) => Target::Es {
                    file: file.to_string(),
                    armsx2_ini: None,
                },
                _ => Target::None("ROCKNIX can't look up options for a file name with [ ] * ? or a double space in it. Rename the file to keep them."),
            },
        };
        let mut menu = Menu::load(emu, table, target, files);
        if emu == Emu::Dolphin {
            menu.buttons = [Toggle::Fps, Toggle::Aspect, Toggle::Resolution].map(|t| hotkey_button(&system, &rom, t));
        }
        if let Some(more) = table.more {
            let target = match rom.file_name()?.to_str()? {
                file if es_findable(file) => Target::Es {
                    file: file.to_string(),
                    armsx2_ini: None,
                },
                _ => Target::None("ROCKNIX can't look up settings for a file name with [ ] * ? or a double space in it. Rename the file to keep them."),
            };
            let mut page = Menu::load(emu, more, target, files);
            // "Custom" only when the player made a profile of their own (the scripts copy a stock one there).
            let dir = |d: &str, f: &str| read(&files.at(&format!("{DOLPHIN_DIR}/{d}/{f}")));
            let (row, own) = if system == "wii" {
                // pocket-ui guards the stock Wii Remote profiles (`with_quiet_wiimote`), not custom.ini.
                let custom = crate::standalone::quiet_wiimote(&dir("WiiControllerProfiles", "custom.ini"));
                (WII_CUSTOM, custom_profile_is_own(&custom, &dir("WiiControllerProfiles", "vremote.ini")))
            } else {
                let custom = dir("GamecubeControllerProfiles", "GCPadNew.ini.custom");
                (
                    GC_CUSTOM,
                    custom_profile_is_own(&custom, &dir("GamecubeControllerProfiles", "GCPadNew.ini.south.stacked")),
                )
            };
            if !own {
                page.limit_choices(0, row);
            }
            menu.attach_more(page);
        }
        Some(menu)
    }

    /// The page with made-up values, for `--snapshot`.
    pub fn demo(emu: Emu, system: &str) -> Menu {
        let target = match emu {
            Emu::Armsx2 | Emu::Azahar => Target::Es {
                file: "demo.iso".into(),
                armsx2_ini: None,
            },
            _ => Target::DolphinIni("/nonexistent/GameSettings/DEMO01.ini".into(), "demo.iso".into()),
        };
        let mut m = Menu::load(
            emu,
            table(emu, system).unwrap_or(&GAMECUBE),
            target,
            &Files {
                system_cfg: "/nonexistent".into(),
                root: "/nonexistent".into(),
            },
        );
        m.values = m.table.settings.iter().map(|s| Some(if s.name == "Resolution" { 1 } else { 0 })).collect();
        m.applied = m.values.clone();
        // ROCKNIX's south profile: GC Y, X, B on North, East, West.
        m.buttons = [Toggle::Fps, Toggle::Aspect, Toggle::Resolution]
            .map(|t| profile_button("Buttons/B = `Button W`\nButtons/X = `Button E`\nButtons/Y = `Button N`\n", toggle_gc_button(t)));
        if let Some(more) = m.table.more {
            let files = Files {
                // GameCube A on the right, as pocket-ui sets it when nothing was chosen.
                system_cfg: "/nonexistent".into(),
                root: "/nonexistent".into(),
            };
            let mut page = Menu::load(
                emu,
                more,
                Target::Es {
                    file: "demo.iso".into(),
                    armsx2_ini: None,
                },
                &files,
            );
            // As pocket-ui's start defaults leave a device: GameCube A on the right, asynchronous shaders.
            for (key, value) in START_DEFAULTS
                .iter()
                .filter_map(|(k, v)| Some((k.strip_prefix(more.platform)?.strip_prefix('.')?, *v)))
            {
                if let Some(row) = more.settings.iter().position(|s| s.store == Store::Es(key)) {
                    let i = more.settings[row].choices.iter().position(|c| c.stored == value);
                    page.values[row] = i;
                    page.applied[row] = i;
                }
            }
            page.limit_choices(0, if more.platform == "wii" { WII_CUSTOM } else { GC_CUSTOM });
            m.attach_more(page);
        }
        m
    }

    fn load(emu: Emu, table: &'static Table, target: Target, files: &Files) -> Menu {
        let cfg = read(&files.system_cfg);
        let game_ini = match &target {
            Target::Es { armsx2_ini: Some(p), .. } | Target::DolphinIni(p, _) => read(&files.at(p)),
            _ => String::new(),
        };
        let mut values = Vec::new();
        let mut raw = Vec::new();
        let mut in_emu_ini = Vec::new();
        let mut own = Vec::new();
        for s in table.settings {
            in_emu_ini.push(matches!(target, Target::Es { armsx2_ini: Some(_), .. }) && ini_get(&game_ini, s.ini.1, s.ini.2).is_some());
            let base = || match s.ini.0 {
                BaseFile::SystemCfg => es_line(&cfg, s.ini.2).map(|v| v.trim().to_string()).unwrap_or_default(),
                f => ini_get(&read(&files.base(f)), s.ini.1, s.ini.2).unwrap_or_default(),
            };
            // What the game runs with: the emulator's own per-game INI first (PS2: set in ARMSX2's menu),
            // then our per-game value, then the emulator's base INI.
            let (value, stored) = match (s.store, &target) {
                (Store::Es(key), Target::Es { file, .. }) => match ini_get(&game_ini, s.ini.1, s.ini.2) {
                    Some(v) => (v, false),
                    None => es_get(&cfg, table.platform, file, key).map_or_else(|| (base(), false), |v| (v, true)),
                },
                (Store::Es(key), Target::DolphinIni(_, file)) => es_get(&cfg, table.platform, file, key).map_or_else(|| (base(), false), |v| (v, true)),
                (Store::GameIni(sec, key), _) => ini_get(&game_ini, sec, key).map_or_else(|| (base(), false), |v| (v, false)),
                _ => (base(), false),
            };
            // Not set anywhere: the emulator's own default, which each table lists first; Performance's comes from
            // Games: performance (`default_performance`).
            let first = if s.store == Store::Es(PERFORMANCE_KEY) {
                DEFAULT_PERFORMANCE.load(std::sync::atomic::Ordering::Relaxed).min(s.choices.len() - 1)
            } else if s.store == Store::Es(crate::hakux::RESOLUTION_KEY) {
                // Xbox: the device's default (2x on SM8550, `hakux::default_scale`), not always the first choice.
                s.choices.iter().position(|c| c.stored == crate::hakux::default_scale()).unwrap_or(0)
            } else {
                0
            };
            let i = if value.trim().is_empty() { Some(first) } else { find_choice(s, &value, stored) };
            let value = if value.trim().is_empty() { s.choices[first].emu.to_string() } else { value };
            own.push(stored);
            raw.push(i.map_or(value.clone(), |i| if stored { s.choices[i].emu.to_string() } else { value.clone() }));
            values.push(i);
        }
        Menu {
            emu,
            table,
            target,
            applied: values.clone(),
            values,
            raw,
            in_emu_ini,
            own,
            buttons: [None; 3],
            limit: table.settings.iter().map(|s| s.choices.len()).collect(),
            more: None,
            in_more: false,
        }
    }

    /// Adds the page behind "More…".
    pub fn attach_more(&mut self, more: Menu) {
        self.more = Some(Box::new(more));
    }

    /// Left/Right on `row` stop after its first `n` choices.
    pub fn limit_choices(&mut self, row: usize, n: usize) {
        if let Some(l) = self.limit.get_mut(row) {
            *l = n.max(1);
        }
    }

    /// The page showing: this one, or "More…".
    fn page(&self) -> &Menu {
        self.more.as_deref().filter(|_| self.in_more).unwrap_or(self)
    }

    pub fn title(&self) -> &'static str {
        self.page().table.title
    }

    /// Whether `row` is "More…" (on the first page).
    pub fn is_more(&self, row: usize) -> bool {
        !self.in_more && self.more.is_some() && row == self.table.settings.len()
    }

    /// Shows the "More…" page.
    pub fn open_more(&mut self) {
        self.in_more = self.more.is_some();
    }

    /// Back from "More…" to the first page: the row to select there (None: it wasn't showing).
    pub fn close_more(&mut self) -> Option<usize> {
        std::mem::take(&mut self.in_more).then_some(self.table.settings.len())
    }

    /// Rows: (name, value, explanation), the settings, then "More…" and "Advanced…" where the emulator has them.
    pub fn rows(&self) -> Vec<(String, String, String)> {
        if self.in_more {
            return self.page().rows();
        }
        let mut rows: Vec<(String, String, String)> = self
            .table
            .settings
            .iter()
            .zip(&self.values)
            .zip(&self.raw)
            .map(|((s, v), raw)| {
                let value = v.map_or_else(|| raw.clone(), |i| s.choices[i].label.to_string());
                let desc = match &self.target {
                    Target::None(why) => why.to_string(),
                    _ => s.desc.to_string(),
                };
                (s.name.to_string(), value, desc)
            })
            .collect();
        if self.more.is_some() {
            rows.push((
                "More…".into(),
                ">".into(),
                "Controls, shaders, CPU clock and accuracy. They apply when the game starts again.".into(),
            ));
        }
        if self.table.advanced.is_some() {
            rows.push((
                "Advanced…".into(),
                ">".into(),
                format!(
                    "{}'s own menu, over the game. Close it there to play on; pocket-ui's menu stays on HOME.",
                    self.table.title
                ),
            ));
        }
        rows
    }

    pub fn len(&self) -> usize {
        let page = self.page();
        page.table.settings.len() + usize::from(page.table.advanced.is_some()) + usize::from(!self.in_more && self.more.is_some())
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn is_advanced(&self, row: usize) -> bool {
        !self.in_more && self.table.advanced.is_some() && row == self.table.settings.len() + usize::from(self.more.is_some())
    }

    pub fn advanced(&self) -> Option<Advanced> {
        self.table.advanced
    }

    /// Left/Right on a row. False when nothing changes (no place to keep it, "More…" or "Advanced…").
    pub fn step(&mut self, row: usize, delta: i32) -> bool {
        if self.in_more {
            return self.more.as_mut().is_some_and(|m| m.step(row, delta));
        }
        let Some(s) = self.table.settings.get(row) else { return false };
        if matches!(self.target, Target::None(_)) {
            return false;
        }
        let n = self.limit[row].min(s.choices.len()) as i32;
        let now = self.values[row].map_or(if delta > 0 { -1 } else { 0 }, |i| i as i32);
        self.values[row] = Some((now + delta).rem_euclid(n) as usize);
        true
    }

    pub fn changed(&self) -> bool {
        self.values != self.applied || self.more.as_ref().is_some_and(|m| m.changed())
    }

    /// What leaving the menu now would do: nothing, a live change, or a restart. A restart for "More…" takes
    /// the first page's changes along.
    pub fn preview(&self) -> Plan {
        let mut plan = self.plan(&self.toggles());
        if self.more.as_ref().is_some_and(|m| m.preview().restart) {
            plan.restart = true;
            plan.actions.clear();
        }
        plan
    }

    /// A line under the list once something changed.
    pub fn note(&self) -> Option<&'static str> {
        if !self.changed() {
            return None;
        }
        Some(if self.preview().restart {
            "Applies when you go back to the game: it restarts where you are (a state is saved and loaded)."
        } else if self.next_start_pending() {
            "Saved. This takes effect the next time the game starts."
        } else {
            "Applies as soon as you go back to the game."
        })
    }

    /// A changed row that only the next start applies (`Live::NextStart`).
    fn next_start_pending(&self) -> bool {
        self.table.settings.iter().enumerate().any(|(i, s)| s.live == Live::NextStart && self.values[i] != self.applied[i])
    }

    fn toggles(&self) -> Toggles {
        SESSION.lock().ok().and_then(|s| s.as_ref().map(|s| s.toggles)).unwrap_or_default()
    }

    /// The value the running game has for setting `i`.
    fn running(&self, i: usize) -> String {
        let s = SESSION.lock().ok();
        let s = s.as_ref().and_then(|s| s.as_ref());
        s.and_then(|s| s.running.get(i).cloned().flatten().or_else(|| s.boot.as_ref().and_then(|b| b.get(i).cloned())))
            .unwrap_or_else(|| self.raw[i].clone())
    }

    fn plan(&self, toggles: &Toggles) -> Plan {
        let mut plan = Plan::default();
        for (i, s) in self.table.settings.iter().enumerate() {
            let Some(want) = self.values[i].filter(|_| self.values[i] != self.applied[i]) else {
                continue;
            };
            let choice = &s.choices[want];
            match (s.live, &self.target) {
                // A key set in ARMSX2's own per-game INI wins over the base layer PINE writes to.
                (Live::Pine, _) if self.in_emu_ini[i] => plan.restart = true,
                (Live::Pine, _) => plan.actions.push(Action::Pine {
                    section: s.ini.1,
                    key: s.ini.2,
                    value: choice.emu,
                }),
                (Live::Toggle(t), _) => match (toggles.presses(t, &self.running(i), choice.emu), self.buttons[t as usize]) {
                    (Some(0), _) => {}
                    (Some(times), Some(button)) => plan.actions.push(Action::Hotkey {
                        toggle: t,
                        button,
                        times,
                        setting: i,
                    }),
                    _ => plan.restart = true,
                },
                (Live::Restart, _) => plan.restart = true,
                (Live::NextStart, _) => {}
                (Live::Power, _) => plan.actions.push(Action::Power(choice.stored)),
            }
        }
        if plan.restart {
            // The new start reads every value from the files.
            plan.actions.clear();
        }
        plan
    }

    /// The game's own Performance (`max`, `balanced`, `battery`) when it has one in system.cfg; None when it runs with
    /// Games: performance ([`default_performance`]). For Quick settings over the paused game (`quick_profiles`).
    pub fn own_performance(&self) -> Option<&'static str> {
        let i = self.table.settings.iter().position(|s| s.store == Store::Es(PERFORMANCE_KEY))?;
        self.own.get(i).copied().filter(|o| *o)?;
        self.values.get(i).copied().flatten().map(|v| self.table.settings[i].choices[v].stored)
    }

    /// Games: performance changed while this page was loaded: a Performance row without a value of its own shows the
    /// new default (and isn't a change to save).
    pub fn default_performance_changed(&mut self) {
        let Some(i) = self.table.settings.iter().position(|s| s.store == Store::Es(PERFORMANCE_KEY)) else { return };
        if self.own.get(i) == Some(&false) && self.values[i] == self.applied[i] {
            let d = Some(DEFAULT_PERFORMANCE.load(std::sync::atomic::Ordering::Relaxed));
            self.values[i] = d;
            self.applied[i] = d;
            if let (Some(raw), Some(v)) = (self.raw.get_mut(i), d) {
                *raw = self.table.settings[i].choices[v].emu.to_string();
            }
        }
    }

    /// Writes the changed values into this game's store and says what applying them takes. The values are
    /// kept even if applying fails: they take effect at the next start.
    pub fn commit(&mut self) -> Result<Plan, String> {
        self.commit_to(&Files::default())
    }

    /// `commit` with the files somewhere else (tests).
    pub fn commit_to(&mut self, files: &Files) -> Result<Plan, String> {
        let mut plan = self.commit_page(files)?;
        if let Some(more) = self.more.as_mut() {
            if more.commit_page(files)?.restart {
                plan.restart = true;
                plan.actions.clear();
            }
        }
        Ok(plan)
    }

    fn commit_page(&mut self, files: &Files) -> Result<Plan, String> {
        if self.values == self.applied {
            return Ok(Plan::default());
        }
        let plan = self.plan(&self.toggles());
        let cfg_path = files.system_cfg.as_path();
        match &self.target {
            Target::Es { file, armsx2_ini } => {
                // system.cfg's lines are applied to the file as it is when the lock is ours (`system_cfg`), so a
                // runemu.sh that wrote in the meantime keeps its change.
                let mut cfg_lines: Vec<(String, &str)> = Vec::new();
                let mut ini_old = armsx2_ini.as_deref().map(|p| read(&files.at(p)));
                let mut ini_new = ini_old.clone();
                for (i, s) in self.table.settings.iter().enumerate() {
                    let (Some(v), Store::Es(key)) = (self.values[i], s.store) else { continue };
                    if self.values[i] == self.applied[i] {
                        continue;
                    }
                    cfg_lines.push((es_game_key(self.table.platform, file, key), s.choices[v].stored));
                    // So our value is the one ARMSX2 uses from the next start on.
                    if let Some(t) = ini_new.as_mut().filter(|_| s.ini.0 == BaseFile::Pcsx2) {
                        *t = ini_set(t, s.ini.1, s.ini.2, None);
                    }
                }
                write_cfg_lines(cfg_path, &cfg_lines)?;
                if let (Some(p), Some(new)) = (armsx2_ini, ini_new.take()) {
                    if ini_old.take().is_some_and(|o| o != new && !o.is_empty()) {
                        crate::fsutil::write_atomic(&files.at(p), new.as_bytes()).map_err(|e| format!("{p}: {e}"))?;
                    }
                }
            }
            Target::DolphinIni(p, file) => {
                let old = read(&files.at(p));
                let mut t = old.clone();
                let mut cfg_lines: Vec<(String, &str)> = Vec::new();
                for (i, s) in self.table.settings.iter().enumerate() {
                    let Some(v) = self.values[i].filter(|_| self.values[i] != self.applied[i]) else {
                        continue;
                    };
                    match s.store {
                        Store::GameIni(sec, key) => t = ini_set(&t, sec, key, Some(s.choices[v].stored)),
                        Store::Es(key) => cfg_lines.push((es_game_key(self.table.platform, file, key), s.choices[v].stored)),
                    }
                }
                if t != old {
                    crate::fsutil::write_atomic(&files.at(p), t.as_bytes()).map_err(|e| format!("{p}: {e}"))?;
                }
                write_cfg_lines(cfg_path, &cfg_lines)?;
            }
            Target::None(why) => return Err(why.to_string()),
        }
        println!(
            "emu_settings: {:?} saved for this game; {}",
            self.emu,
            if plan.restart { "restart" } else { "live" }
        );
        for (own, (now, before)) in self.own.iter_mut().zip(self.values.iter().zip(&self.applied)) {
            *own |= now != before;
        }
        self.applied = self.values.clone();
        Ok(plan)
    }
}

/// The GC controller profile (`GamecubeControllerProfiles/GCPadNew.ini.<name>`) that ROCKNIX copies at launch,
/// whose face buttons the hotkeys read. start_dolphin_wii.sh always takes the south one. None: the hotkeys are
/// not to be pressed, because the Wii game's Wiimote profile is the player's own (`custom`), which
/// `with_quiet_wiimote` leaves alone, so a chord could press a Wiimote button.
pub fn hotkey_profile(system: &str, cfg: &str, file: &str) -> Option<&'static str> {
    if system == "wii" {
        return (es_get(cfg, system, file, "wii_controller_profile").as_deref() != Some("custom")).then_some("south.stacked");
    }
    // start_dolphin_gc.sh: `swap_shoulders=true` takes the inline variant (the face buttons are the same in both).
    let inline = es_get(cfg, "gamecube", file, "swap_shoulders").as_deref() == Some("true");
    Some(match (es_get(cfg, "gamecube", file, "gamecube_controller_profile").as_deref(), inline) {
        (Some("east"), false) => "east.stacked",
        (Some("east"), true) => "east.inline",
        (Some("custom"), _) => "custom",
        (_, true) => "south.inline",
        _ => "south.stacked",
    })
}

/// The InputPlumber button that is the hotkey's GC button in the controller profile ROCKNIX copies at launch
/// (the face buttons are the same in its stacked and inline variants).
fn hotkey_button(system: &str, rom: &Path, t: Toggle) -> Option<&'static str> {
    let cfg = read(Path::new(SYSTEM_CFG));
    let file = rom.file_name().and_then(|f| f.to_str()).unwrap_or_default();
    let name = hotkey_profile(system, &cfg, file)?;
    let profile = read(&Path::new(DOLPHIN_DIR).join("GamecubeControllerProfiles").join(format!("GCPadNew.ini.{name}")));
    let gc = toggle_gc_button(t);
    profile_button(&profile, gc).or_else(|| profile_button("Buttons/B = `Button W`\nButtons/X = `Button E`\nButtons/Y = `Button N`\n", gc))
}

/// Carries out the live part of a plan once the game runs again (blocks; call it from a worker thread).
pub fn apply_live(actions: &[Action]) {
    if actions.iter().any(|a| matches!(a, Action::Hotkey { .. })) {
        // Dolphin reads the pad again once it is running and in front.
        std::thread::sleep(std::time::Duration::from_millis(300));
    }
    for a in actions {
        match *a {
            Action::Pine { section, key, value } => {
                pine_set(section, key, value);
            }
            Action::Power(level) => {
                crate::standalone::set_performance(level);
            }
            Action::Hotkey {
                toggle,
                button,
                times,
                setting,
            } => {
                for _ in 0..times {
                    if !crate::standalone::dolphin_toggle(button) {
                        println!("emu_settings: hotkey {button} failed");
                        break;
                    }
                    if let Ok(mut s) = SESSION.lock() {
                        if let Some(s) = s.as_mut() {
                            let value = s.toggles.press(toggle).to_string();
                            if let Some(slot) = s.running.get_mut(setting) {
                                *slot = Some(value);
                            }
                        }
                    }
                }
            }
        }
    }
}

/// "Advanced…": opens the emulator's own menu over the running game (call once the game runs again, from a
/// worker thread). For ARMSX2, waits for that menu to close; if the player ended the game in it, ARMSX2 is
/// left in its game list, so it is closed and pocket-ui comes back.
pub fn open_advanced(adv: Advanced, pgid: Option<i32>) {
    let Advanced::Chord(buttons) = adv;
    std::thread::sleep(std::time::Duration::from_millis(300));
    let sent = crate::standalone::press_chord(buttons);
    println!("emu_settings: emulator menu {}", if sent { "asked for" } else { "not reached" });
    if !sent {
        return;
    }
    // PINE: 0 running, 1 paused (the menu pauses the VM), 2 no VM.
    let opened_by = std::time::Instant::now() + std::time::Duration::from_secs(3);
    let mut seen_menu = false;
    while crate::standalone::active() {
        match crate::standalone::pine_status() {
            Some(1) => seen_menu = true,
            Some(0) if seen_menu || std::time::Instant::now() > opened_by => return,
            Some(2) => {
                println!("emu_settings: game closed in the emulator's menu");
                crate::standalone::quit(pgid, None, true);
                return;
            }
            _ => {}
        }
        std::thread::sleep(std::time::Duration::from_millis(400));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;

    #[test]
    fn ps2_advanced_presses_what_reaches_armsx2_as_face_north() {
        // Measured on the Mini V2: InputPlumber's West arrives as the north face button (as with Dolphin).
        assert_eq!(PS2.advanced, Some(Advanced::Chord(&["Gamepad:Button:Guide", "Gamepad:Button:West"])));
    }

    #[test]
    fn resolution_has_steps_between_the_whole_ones() {
        let labels = |t: &Table| {
            t.settings
                .iter()
                .find(|s| s.name == "Resolution")
                .unwrap()
                .choices
                .iter()
                .map(|c| c.label)
                .collect::<Vec<_>>()
        };
        // ARMSX2 takes a fractional multiplier, live through PINE: quarter steps like its own menu.
        assert_eq!(labels(&PS2), ["1x", "1.25x", "1.5x", "1.75x", "2x", "2.25x", "2.5x", "2.75x", "3x"]);
        let ps2 = PS2.settings.iter().find(|s| s.name == "Resolution").unwrap();
        assert_eq!(ps2.choices[1].emu, "1.25");
        // Dolphin's scale is whole numbers, halved by ROCKNIX's patch: half steps, the in-between ones by a restart.
        assert_eq!(labels(&GAMECUBE), ["1x", "1.5x", "2x", "2.5x", "3x"]);
        let t = Toggles::default();
        assert_eq!(t.presses(Toggle::Resolution, "2", "5"), None, "2.5x is not on the hotkey: a restart applies it");
    }

    // ---- INI read/write per emulator

    /// The shape of ROCKNIX's PCSX2.ini: sections with blank lines between, `key = value`.
    const PCSX2: &str = "[UI]\nConfirmShutdown = false\n\n\n[EmuCore]\nEnablePINE = true\nEnableWideScreenPatches = false\n\n[EmuCore/GS]\n\
                         AspectRatio = Auto 4:3/3:2\nOsdShowFPS = false\nRenderer = 14\nupscale_multiplier = 2\n\n[Hotkeys]\n\
                         OpenPauseMenu = SDL-0/Guide & SDL-0/FaceNorth\n";

    /// A user GameSettings INI as ROCKNIX ships them: a title comment, cheat sections with `$name` lines.
    const DOLPHIN_GAME: &str = "# GM4E01 - Mario Kart: Double Dash!!\n\n[OnFrame]\n# Add memory patches to be applied every frame here.\n\n\
                                [ActionReplay]\n# Add action replay cheats here.\n$Everything Unlocked\n023BC1C0 0003FFFF\n\n\
                                [Video_Settings]\nSafeTextureCacheColorSamples = 512\n";

    #[test]
    fn pcsx2_ini_reads_and_writes_one_key() {
        assert_eq!(ini_get(PCSX2, "EmuCore/GS", "upscale_multiplier").as_deref(), Some("2"));
        assert_eq!(ini_get(PCSX2, "EmuCore/GS", "AspectRatio").as_deref(), Some("Auto 4:3/3:2"));
        assert_eq!(ini_get(PCSX2, "EmuCore", "upscale_multiplier"), None, "same key, other section");
        let t = ini_set(PCSX2, "EmuCore/GS", "upscale_multiplier", Some("3"));
        assert_eq!(t, PCSX2.replace("upscale_multiplier = 2", "upscale_multiplier = 3"));
        // The hotkey binding with its '&' and '/' is untouched.
        assert!(t.contains("OpenPauseMenu = SDL-0/Guide & SDL-0/FaceNorth\n"));
    }

    #[test]
    fn dolphin_game_ini_keeps_comments_and_cheats() {
        let t = ini_set(DOLPHIN_GAME, "Video_Settings", "InternalResolution", Some("4"));
        assert_eq!(t, format!("{DOLPHIN_GAME}InternalResolution = 4\n"));
        let t = ini_set(&t, "Core", "GFXBackend", Some("OGL"));
        assert!(t.starts_with(DOLPHIN_GAME), "{t}");
        assert!(t.ends_with("InternalResolution = 4\n\n[Core]\nGFXBackend = OGL\n"), "{t}");
        assert_eq!(ini_get(&t, "Core", "GFXBackend").as_deref(), Some("OGL"));
        // A cheat line isn't a key, even with '=' in it.
        let cheat = "[Gecko]\n$Speed = fast\n04000000 00000000\n";
        assert_eq!(ini_get(cheat, "Gecko", "$Speed"), None);
        assert_eq!(ini_set(cheat, "Gecko", "$Speed", None), cheat);
        // Removing a key leaves the rest, removing it again changes nothing.
        let t = ini_set(&t, "Video_Settings", "InternalResolution", None);
        assert!(t.contains("SafeTextureCacheColorSamples = 512\n") && !t.contains("InternalResolution"));
        assert_eq!(ini_set(&t, "Video_Settings", "InternalResolution", None), t);
    }

    #[test]
    fn a_section_written_twice_counts_once() {
        // INI readers merge a section that appears twice; a key in its second part is still that key.
        let t = "[EmuCore/GS]\nAccurateBlendingUnit = 2\n\n[EmuCore]\nEnableCheats = true\n[EmuCore/GS]\nupscale_multiplier = 1\n";
        assert_eq!(ini_get(t, "EmuCore/GS", "upscale_multiplier").as_deref(), Some("1"));
        let removed = ini_set(t, "EmuCore/GS", "upscale_multiplier", None);
        assert_eq!(ini_get(&removed, "EmuCore/GS", "upscale_multiplier"), None, "{removed}");
        assert!(removed.contains("AccurateBlendingUnit = 2\n") && removed.contains("EnableCheats = true\n"));
        let set = ini_set(t, "EmuCore/GS", "upscale_multiplier", Some("3"));
        assert_eq!(set, t.replace("upscale_multiplier = 1", "upscale_multiplier = 3"));
        let twice = format!("{t}upscale_multiplier = 2\n");
        assert_eq!(
            ini_set(&twice, "EmuCore/GS", "upscale_multiplier", Some("3"))
                .matches("upscale_multiplier")
                .count(),
            1
        );
    }

    #[test]
    fn new_game_ini_from_nothing() {
        assert_eq!(ini_set("", "Video_Settings", "ShowFPS", Some("True")), "[Video_Settings]\nShowFPS = True\n");
        // Dolphin's own spelling "Key=Value" without blanks is read too.
        assert_eq!(ini_get("[Settings]\nShowFPS=True\n", "Settings", "ShowFPS").as_deref(), Some("True"));
    }

    // ---- system.cfg and the ROCKNIX start script

    #[test]
    fn es_keys_per_game_and_platform() {
        let file = "Burnout 3 - Takedown (USA).chd";
        assert_eq!(
            es_game_key("ps2", file, "internal_resolution"),
            "ps2[\"Burnout 3 - Takedown (USA).chd\"].internal_resolution"
        );
        let cfg = "global.autosave=1\nps2.internal_resolution=2\nps2[\"Other (USA).chd\"].internal_resolution=1\n";
        assert_eq!(
            es_get(cfg, "ps2", file, "internal_resolution").as_deref(),
            Some("2"),
            "falls back to the platform"
        );
        let cfg = es_set(cfg, &es_game_key("ps2", file, "internal_resolution"), Some("3"));
        assert_eq!(es_get(&cfg, "ps2", file, "internal_resolution").as_deref(), Some("3"));
        assert_eq!(es_get(&cfg, "ps2", "Other (USA).chd", "internal_resolution").as_deref(), Some("1"));
        assert!(cfg.starts_with("global.autosave=1\nps2.internal_resolution=2\n"));
        let cfg = es_set(&cfg, &es_game_key("ps2", file, "internal_resolution"), None);
        assert_eq!(es_get(&cfg, "ps2", file, "internal_resolution").as_deref(), Some("2"));
        assert_eq!(es_get("global.show_fps=true\n", "ps2", file, "show_fps").as_deref(), Some("true"));
    }

    /// start_armsx2.sh of ROCKNIX 20261001, the part that rewrites PCSX2.ini on every launch, for the keys of
    /// the PS2 table. (Its `[ "$IRES" > "0" ]` test only holds from a writable working directory; pocket-ui
    /// starts games from /tmp.)
    fn rocknix_start_armsx2(cfg: &str, file: &str, ini: &str) -> String {
        let get = |k| es_get(cfg, "ps2", file, k);
        let mut t = ini.to_string();
        let mut set = |k: &str, v: &str| t = ini_set(&t, if k == "EnableWideScreenPatches" { "EmuCore" } else { "EmuCore/GS" }, k, Some(v));
        match get("aspect_ratio").as_deref() {
            Some("0") => set("AspectRatio", "4:3"),
            Some("1") => set("AspectRatio", "16:9"),
            Some("2") => set("AspectRatio", "Stretch"),
            _ => {}
        }
        match get("graphics_backend").as_deref() {
            Some("0") => set("Renderer", "-1"),
            Some("1") => set("Renderer", "12"),
            Some("2") => set("Renderer", "14"),
            Some("3") => set("Renderer", "13"),
            _ => {}
        }
        match get("internal_resolution") {
            Some(r) => set("upscale_multiplier", &r),
            None => set("upscale_multiplier", "1"),
        }
        match get("show_fps").as_deref() {
            Some("false") => set("OsdShowFPS", "false"),
            Some("true") => set("OsdShowFPS", "true"),
            _ => {}
        }
        let ws = get("enable_widescreen_patches").as_deref() == Some("true");
        set("EnableWideScreenPatches", if ws { "true" } else { "false" });
        t
    }

    #[test]
    fn rocknix_rewrite_carries_the_per_game_choice() {
        let file = "SSX Tricky (USA).iso";
        let cfg = "ps2.internal_resolution=2\n";
        // Writing PCSX2.ini directly is lost at the next launch: the script resets upscale_multiplier.
        let direct = ini_set(PCSX2, "EmuCore/GS", "upscale_multiplier", Some("3"));
        let after = rocknix_start_armsx2(cfg, file, &direct);
        assert_eq!(ini_get(&after, "EmuCore/GS", "upscale_multiplier").as_deref(), Some("2"));
        // The per-game key is what the script itself writes: every PS2 setting survives the launch.
        let mut cfg = cfg.to_string();
        for (s, choice) in PS2.settings.iter().zip([2, 1, 1, 1, 1]) {
            let Store::Es(key) = s.store else { panic!("PS2 settings live in system.cfg") };
            cfg = es_set(&cfg, &es_game_key("ps2", file, key), Some(s.choices[choice].stored));
        }
        let after = rocknix_start_armsx2(&cfg, file, PCSX2);
        for (s, choice) in PS2.settings.iter().zip([2, 1, 1, 1, 1]) {
            assert_eq!(ini_get(&after, s.ini.1, s.ini.2).as_deref(), Some(s.choices[choice].emu), "{}", s.name);
        }
        // Another game keeps the platform's values.
        let other = rocknix_start_armsx2(&cfg, "Ico (USA).iso", PCSX2);
        assert_eq!(ini_get(&other, "EmuCore/GS", "upscale_multiplier").as_deref(), Some("2"));
        assert_eq!(ini_get(&other, "EmuCore/GS", "Renderer").as_deref(), Some("14"));
    }

    #[test]
    fn rocknix_finds_ordinary_names_only() {
        assert!(es_findable("Burnout 3 - Takedown (USA).chd"));
        assert!(es_findable("Tom Clancy's Splinter Cell & Co (Europe) (En,Fr,De).iso"));
        assert!(!es_findable("Game [!].iso"), "'[' opens a character class in awk");
        assert!(!es_findable("Two  Spaces.iso"), "echo squeezes blanks");
        assert!(!es_findable("What?.iso"));
        assert!(!es_findable("Quote\".iso"));
    }

    // ---- per-game file names

    #[test]
    fn armsx2_game_ini_is_serial_and_crc() {
        assert_eq!(
            armsx2_game_ini("SLUS-21050 (A1B2C3D4)").as_deref(),
            Some("/storage/.config/ARMSX2/gamesettings/SLUS-21050_A1B2C3D4.ini")
        );
        assert_eq!(
            armsx2_game_ini("SLUS-21050 (1b2c3d4)").as_deref(),
            Some("/storage/.config/ARMSX2/gamesettings/SLUS-21050_01B2C3D4.ini")
        );
        assert_eq!(armsx2_game_ini("SLUS-21050"), None);
    }

    fn header_at(offset: usize, id: &[u8]) -> Vec<u8> {
        let mut v = vec![0u8; offset + 0x40];
        v[offset..offset + id.len()].copy_from_slice(id);
        v
    }

    #[test]
    fn dolphin_ids_from_each_image_format() {
        let mut iso = header_at(0, b"GM4E01");
        iso[0x1c..0x20].copy_from_slice(&[0xc2, 0x33, 0x9f, 0x3d]);
        assert_eq!(dolphin_game_id(&iso).as_deref(), Some("GM4E01"));
        let mut rvz = header_at(0x58, b"GZLE01");
        rvz[..4].copy_from_slice(b"RVZ\x01");
        assert_eq!(dolphin_game_id(&rvz).as_deref(), Some("GZLE01"));
        let mut wia = rvz.clone();
        wia[..4].copy_from_slice(b"WIA\x01");
        assert_eq!(dolphin_game_id(&wia).as_deref(), Some("GZLE01"));
        let mut ciso = header_at(0x8000, b"G8ME01");
        ciso[..4].copy_from_slice(b"CISO");
        assert_eq!(dolphin_game_id(&ciso).as_deref(), Some("G8ME01"));
        let mut wbfs = header_at(0x200, b"RMGE01");
        wbfs[..4].copy_from_slice(b"WBFS");
        assert_eq!(dolphin_game_id(&wbfs).as_deref(), Some("RMGE01"));
        // GCZ: header, one block pointer and hash, then the zlib-compressed first block.
        let block = miniz_oxide::deflate::compress_to_vec_zlib(&header_at(0, b"GALE01"), 6);
        let mut gcz = Vec::new();
        gcz.extend_from_slice(&0xB10B_C001u32.to_le_bytes());
        gcz.extend_from_slice(&0u32.to_le_bytes());
        gcz.extend_from_slice(&(block.len() as u64).to_le_bytes());
        gcz.extend_from_slice(&0x40u64.to_le_bytes());
        gcz.extend_from_slice(&0x4000u32.to_le_bytes());
        gcz.extend_from_slice(&1u32.to_le_bytes());
        gcz.extend_from_slice(&0u64.to_le_bytes());
        gcz.extend_from_slice(&0u32.to_le_bytes());
        gcz.extend_from_slice(&block);
        assert_eq!(dolphin_game_id(&gcz).as_deref(), Some("GALE01"));
        // Not a disc: a DOL, or text.
        assert_eq!(dolphin_game_id(b"hello world, not a disc image at all"), None);
        assert_eq!(dolphin_game_id(&header_at(0, b"gm4e01")), None);
    }

    // ---- PINE

    #[test]
    fn pine_strings_are_length_prefixed() {
        assert_eq!(pine_strings(&["EmuCore/GS", "upscale_multiplier"])[..4], 10u32.to_le_bytes());
        let set = pine_strings(&["EmuCore", "OsdShowFPS", "true"]);
        assert_eq!(
            set,
            [
                &7u32.to_le_bytes()[..],
                b"EmuCore",
                &10u32.to_le_bytes(),
                b"OsdShowFPS",
                &4u32.to_le_bytes(),
                b"true"
            ]
            .concat()
        );
    }

    // ---- Dolphin's hotkeys

    #[test]
    fn dolphin_fps_hotkey_from_either_start() {
        let t = Toggles::default();
        assert_eq!(t.presses(Toggle::Fps, "False", "True"), Some(1));
        assert_eq!(t.presses(Toggle::Fps, "False", "False"), Some(0));
        // Shown from the start (game INI): the hotkey's first press switches it "on" again, the second off.
        assert_eq!(t.presses(Toggle::Fps, "True", "False"), Some(2));
        let mut t = t;
        assert_eq!(t.press(Toggle::Fps), "True");
        assert_eq!(t.presses(Toggle::Fps, "True", "False"), Some(1));
    }

    #[test]
    fn dolphin_aspect_hotkey_knows_auto_and_stretch() {
        let mut t = Toggles::default();
        assert_eq!(t.presses(Toggle::Aspect, "0", "3"), Some(1));
        assert_eq!(t.presses(Toggle::Aspect, "1", "0"), Some(2), "16:9 -> Stretch -> Auto");
        assert_eq!(t.presses(Toggle::Aspect, "0", "1"), None, "16:9 needs a restart");
        assert_eq!(t.press(Toggle::Aspect), "3");
        assert_eq!(t.press(Toggle::Aspect), "0");
    }

    #[test]
    fn dolphin_resolution_hotkey_cycles() {
        let mut t = Toggles::default();
        // From 1x (2): the first press sets 2 again, then 4 and 6.
        assert_eq!(t.presses(Toggle::Resolution, "2", "6"), Some(3));
        assert_eq!(t.presses(Toggle::Resolution, "4", "4"), Some(0));
        assert_eq!(t.presses(Toggle::Resolution, "6", "2"), Some(1));
        assert_eq!(
            [
                t.press(Toggle::Resolution),
                t.press(Toggle::Resolution),
                t.press(Toggle::Resolution),
                t.press(Toggle::Resolution)
            ],
            ["2", "4", "6", "1"]
        );
        assert_eq!(t.presses(Toggle::Resolution, "1", "4"), Some(2));
        assert_eq!(t.presses(Toggle::Resolution, "2", "3"), None, "1.5x is not on the hotkey");
    }

    #[test]
    fn hotkey_buttons_follow_the_profile() {
        // Measured on the Mini V2 (2026-10-22): InputPlumber's West reaches Dolphin as `Button N` and its North as
        // `Button W` (Select+West toggled the FPS counter, Select+North cycled the resolution).
        let south = "[GCPad1]\nButtons/A = `Button S`\nButtons/B = `Button W`\nButtons/X = `Button E`\nButtons/Y = `Button N`\n";
        assert_eq!(profile_button(south, "Y"), Some("Gamepad:Button:West"));
        assert_eq!(profile_button(south, "B"), Some("Gamepad:Button:North"));
        assert_eq!(profile_button(south, "X"), Some("Gamepad:Button:East"));
        let east = "Buttons/B = `Button S`\nButtons/X = `Button N`\nButtons/Y = `Button W`\n";
        assert_eq!(profile_button(east, "Y"), Some("Gamepad:Button:North"));
        assert_eq!(profile_button(east, "X"), Some("Gamepad:Button:West"));
        assert_eq!(profile_button("Buttons/Y = `Shoulder L`\n", "Y"), None);
    }

    // ---- the apply sequence

    #[test]
    fn restart_saves_quits_relaunches_and_loads() {
        let (mut r, first) = Restart::start();
        assert_eq!((r.phase, first), (Phase::Saving, Cmd::SaveAndClose));
        assert_eq!(r.on(Event::Started), None, "nothing else moves it on while saving");
        assert_eq!(r.on(Event::Saved(true)), None);
        assert_eq!(r.phase, Phase::Closing);
        assert_eq!(r.on(Event::Exited), Some(Cmd::Relaunch));
        assert_eq!(r.on(Event::Started), Some(Cmd::LoadResume));
        assert_eq!(r.phase, Phase::Loading);
        assert_eq!(r.on(Event::Loaded(true)), None);
        assert_eq!(r.phase, Phase::Done);
        assert_eq!(r.on(Event::Exited), None, "a later exit is an ordinary one");
    }

    #[test]
    fn restart_without_a_state_keeps_the_game() {
        let (mut r, _) = Restart::start();
        assert_eq!(r.on(Event::Saved(false)), Some(Cmd::BackToGame));
        assert_eq!(r.phase, Phase::Failed);
        // The emulator died while saving: no state to come back to, so no restart either.
        let (mut r, _) = Restart::start();
        assert_eq!(r.on(Event::Exited), Some(Cmd::Shelf));
        assert_eq!(r.phase, Phase::Failed);
    }

    fn files(name: &str, entries: &[(&str, &str)]) -> (Files, crate::testutil::TestDir) {
        let root = TestDir::new(&format!("emu-settings-{name}"));
        for (path, text) in entries {
            let p = root.join(path.trim_start_matches('/'));
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, text).unwrap();
        }
        let files = Files {
            system_cfg: root.join("storage/.config/system/configs/system.cfg"),
            root: root.to_path_buf(),
        };
        (files, root)
    }

    #[test]
    fn ps2_menu_reads_layers_and_goes_live_through_pine() {
        let (f, _dir) = files(
            "ps2",
            &[
                (SYSTEM_CFG, "ps2.internal_resolution=2\nps2[\"SSX Tricky (USA).iso\"].show_fps=true\n"),
                (PCSX2_INI, PCSX2),
            ],
        );
        let target = Target::Es {
            file: "SSX Tricky (USA).iso".into(),
            armsx2_ini: Some(armsx2_game_ini("SLUS-20095 (4C9EE7DF)").unwrap()),
        };
        let mut m = Menu::load(Emu::Armsx2, &PS2, target.clone(), &f);
        let values: Vec<String> = m.rows().into_iter().map(|r| r.1).collect();
        assert_eq!(values, ["2x", "Vulkan", "Off", "4:3", "On", "Max", ">"]);
        assert!(m.is_advanced(6) && !m.is_advanced(5));
        // 2x → 3x is four quarter steps.
        assert!(m.step(0, 4) && m.step(1, 1));
        let plan = m.preview();
        assert!(!plan.restart);
        assert_eq!(
            plan.actions,
            [
                Action::Pine {
                    section: "EmuCore/GS",
                    key: "upscale_multiplier",
                    value: "3"
                },
                Action::Pine {
                    section: "EmuCore/GS",
                    key: "Renderer",
                    value: "12"
                }
            ]
        );
        // Set in ARMSX2's own per-game INI (its game layer beats PINE's base layer): only a new start helps.
        let ini = "/storage/.config/ARMSX2/gamesettings/SLUS-20095_4C9EE7DF.ini";
        let (f, _dir) = files("ps2-own", &[(PCSX2_INI, PCSX2), (ini, "[EmuCore/GS]\nupscale_multiplier = 1\n")]);
        let mut m = Menu::load(Emu::Armsx2, &PS2, target, &f);
        assert_eq!(m.rows()[0].1, "1x");
        m.step(0, 1);
        assert!(m.preview().restart);
        m.step(0, -1);
        assert!(!m.changed());
        m.step(4, 1);
        assert_eq!(
            m.preview(),
            Plan {
                actions: vec![Action::Pine {
                    section: "EmuCore/GS",
                    key: "OsdShowFPS",
                    value: "true"
                }],
                restart: false
            }
        );
    }

    #[test]
    fn gamecube_menu_uses_hotkeys_where_rocknix_has_them() {
        // As ROCKNIX ships it: no VISkip, so Dolphin's default (off) applies.
        let gfx = "[Settings]\nAspectRatio = 0\nInternalResolution = 2\nwideScreenHack = False\nShowFPS = False\n[Hacks]\nEFBToTextureEnable = True\n";
        let game = dolphin_game_ini("GM4E01");
        let (f, _dir) = files(
            "gc",
            &[
                ("/storage/.config/dolphin-emu/GFX.ini", gfx),
                ("/storage/.config/dolphin-emu/Dolphin.ini", "[Core]\nGFXBackend = Vulkan\n"),
                (&game, DOLPHIN_GAME),
            ],
        );
        let mut m = Menu::load(
            Emu::Dolphin,
            &GAMECUBE,
            Target::DolphinIni(game, "Mario Kart - Double Dash!! (USA).rvz".into()),
            &f,
        );
        m.buttons = [Some("Gamepad:Button:North"), Some("Gamepad:Button:East"), Some("Gamepad:Button:West")];
        let values: Vec<String> = m.rows().into_iter().map(|r| r.1).collect();
        assert_eq!(values, ["1x", "Vulkan", "Off", "Auto", "Off", "Off", "Max"]);
        m.step(5, 1);
        // 1x → 2x (past 1.5x, which only a restart applies): two presses of the resolution hotkey.
        m.step(0, 2);
        assert_eq!(
            m.preview().actions,
            [
                Action::Hotkey {
                    toggle: Toggle::Resolution,
                    button: "Gamepad:Button:West",
                    times: 2,
                    setting: 0
                },
                Action::Hotkey {
                    toggle: Toggle::Fps,
                    button: "Gamepad:Button:North",
                    times: 1,
                    setting: 5
                }
            ]
        );
        // 16:9 is not on the aspect hotkey, and the renderer never is: both restart (and nothing is pressed).
        m.step(3, 1);
        assert_eq!(
            m.preview(),
            Plan {
                actions: vec![],
                restart: true
            }
        );
        m.step(3, -1);
        m.step(1, 1);
        assert!(m.preview().restart);
    }

    #[test]
    fn every_big_system_has_options_with_an_fps_counter() {
        assert_eq!(table(Emu::Armsx2, "ps2").map(|t| t.title), Some("ARMSX2"));
        assert_eq!(table(Emu::Dolphin, "gamecube").map(|t| t.title), Some("Dolphin"));
        let names = |t: &Table| t.settings.iter().map(|s| s.name).collect::<Vec<_>>();
        // Wii: the same Dolphin and the same game INIs as GameCube, so the same rows.
        let wii = table(Emu::Dolphin, "wii").expect("Wii has options");
        assert_eq!(wii.platform, "wii");
        assert_eq!(names(wii), names(&GAMECUBE));
        // 3DS: a short list, all applied by a new start (Azahar has no live channel), no Advanced row.
        let ds = table(Emu::Azahar, "3ds").expect("3DS has options");
        assert_eq!(names(ds), ["Resolution", "Screens", "Renderer", "Texture filter", "Show FPS", "Performance"]);
        assert!(
            ds.settings.iter().all(|s| s.live == Live::Restart || s.name == "Performance"),
            "only the CPU/GPU profile is live"
        );
        assert!(ds.advanced.is_none());
        assert!(table(Emu::Dolphin, "wiiware").is_none());
        for t in [&PS2, &GAMECUBE, &WII, &N3DS] {
            assert!(t.settings.iter().any(|s| s.name == "Show FPS"), "{}", t.platform);
        }
    }

    #[test]
    fn wii_hotkeys_use_the_south_profile_and_never_a_custom_wiimote() {
        // start_dolphin_wii.sh always copies the south profile for the hotkeys; the Wiimote profile is what
        // `with_quiet_wiimote` makes deaf to our chords, which it never does to the player's own.
        let cfg = "wii.wii_controller_profile=nunchuck\ngamecube.gamecube_controller_profile=east\n";
        assert_eq!(hotkey_profile("wii", cfg, "Game.wbfs"), Some("south.stacked"));
        assert_eq!(hotkey_profile("gamecube", cfg, "Game.iso"), Some("east.stacked"));
        assert_eq!(hotkey_profile("gamecube", "", "Game.iso"), Some("south.stacked"));
        let custom = "wii[\"Mine.wbfs\"].wii_controller_profile=custom\n";
        assert_eq!(hotkey_profile("wii", custom, "Mine.wbfs"), None);
        assert_eq!(hotkey_profile("wii", custom, "Other.wbfs"), Some("south.stacked"));
    }

    #[test]
    fn wii_menu_goes_live_through_hotkeys_or_restarts_without_them() {
        let gfx = "[Settings]\nAspectRatio = 0\nInternalResolution = 2\nShowFPS = False\n";
        let game = dolphin_game_ini("RMGE01");
        let (f, _dir) = files("wii", &[("/storage/.config/dolphin-emu/GFX.ini", gfx), (&game, "")]);
        let mut m = Menu::load(Emu::Dolphin, &WII, Target::DolphinIni(game.clone(), "Mario Kart Wii (USA).wbfs".into()), &f);
        m.buttons = [Some("Gamepad:Button:West"), Some("Gamepad:Button:East"), Some("Gamepad:Button:North")];
        m.step(5, 1);
        assert_eq!(
            m.preview(),
            Plan {
                actions: vec![Action::Hotkey {
                    toggle: Toggle::Fps,
                    button: "Gamepad:Button:West",
                    times: 1,
                    setting: 5
                }],
                restart: false
            }
        );
        // The player's own Wiimote profile: no hotkey is pressed, the game restarts instead.
        let mut m = Menu::load(Emu::Dolphin, &WII, Target::DolphinIni(game, "Mario Kart - Double Dash!! (USA).rvz".into()), &f);
        m.step(5, 1);
        assert_eq!(
            m.preview(),
            Plan {
                actions: vec![],
                restart: true
            }
        );
    }

    #[test]
    fn dolphin_executables_keep_options_under_their_game_id() {
        assert_eq!(
            dolphin_settings_id(Path::new("/storage/roms/wii/BlockamokRemix.dol"), None).as_deref(),
            Some("ID-BlockamokRemix")
        );
        assert_eq!(dolphin_settings_id(Path::new("/storage/roms/gamecube/x.elf"), None).as_deref(), Some("ID-x"));
        assert_eq!(
            dolphin_settings_id(Path::new("/storage/roms/wii/Game.wbfs"), Some("RMGE01".into())).as_deref(),
            Some("RMGE01")
        );
        assert_eq!(dolphin_settings_id(Path::new("/storage/roms/wii/Game.wbfs"), None), None);
    }

    // ---- 3DS: Azahar's qt-config.ini and ROCKNIX's start_azahar.sh

    /// The parts of ROCKNIX's qt-config.ini the 3DS options touch, with neighbours that must stay.
    const QT: &str = "[Layout]\nfilter_mode=true\nfilter_mode\\default=true\nlayout_option=0\nlayout_option\\default=false\n\n\
                      [Renderer]\ngraphics_api=2\ngraphics_api\\default=false\nresolution_factor=1\nresolution_factor\\default=false\n\
                      texture_filter=0\ntexture_filter\\default=true\nuse_vsync=true\nuse_vsync\\default=true\n\n[UI]\nconfirmClose=false\n";

    #[test]
    fn azahar_launch_writes_this_games_texture_filter_and_keeps_every_other_key() {
        let file = "Pilotwings Resort (Europe).3ds";
        let cfg = es_set("3ds.resolution_scale=1\n", &es_game_key("3ds", file, "pocket_texture_filter"), Some("2"));
        let t = azahar_launch_config(QT, &cfg, file);
        assert_eq!(ini_get(&t, "Renderer", "texture_filter").as_deref(), Some("2"));
        // Azahar ignores a value whose `\default` is true.
        assert_eq!(ini_get(&t, "Renderer", "texture_filter\\default").as_deref(), Some("false"));
        let others = |s: &str| s.lines().filter(|l| !l.starts_with("texture_filter")).collect::<Vec<_>>().join("\n");
        assert_eq!(others(&t), others(QT), "nothing else changes");
        // The next game without a choice of its own is back at the platform's value (none: off).
        let t2 = azahar_launch_config(&t, &cfg, "Other (USA).3ds");
        assert_eq!(ini_get(&t2, "Renderer", "texture_filter").as_deref(), Some("0"));
        assert_eq!(azahar_launch_config(&t2, &cfg, "Other (USA).3ds"), t2, "a second start changes nothing");
        assert_eq!(others(&t2), others(QT));
    }

    /// start_azahar.sh of ROCKNIX 20261001, for the keys of the 3DS table: it rewrites them on every start.
    fn rocknix_start_azahar(cfg: &str, file: &str, ini: &str) -> String {
        let get = |k| es_get(cfg, "3ds", file, k);
        let section = |k: &str| if k == "layout_option" || k == "swap_screen" { "Layout" } else { "Renderer" };
        let set = |t: &str, k: &str, v: &str| crate::standalone::azahar_qt_set(t, section(k), k, v);
        let mut t = ini.to_string();
        let res = match get("resolution_scale").as_deref() {
            Some(r @ ("0" | "2" | "3")) => r.to_string(),
            _ => "1".into(),
        };
        t = set(&t, "resolution_factor", &res);
        let (layout, swap) = match get("screen_layout").as_deref() {
            Some("1a") => ("1", "false"),
            Some("1b") => ("1", "true"),
            Some("2") => ("2", "false"),
            Some("3") => ("3", "false"),
            _ => ("0", "false"),
        };
        t = set(&t, "layout_option", layout);
        t = set(&t, "swap_screen", swap);
        set(&t, "graphics_api", if get("graphics_backend").as_deref() == Some("1") { "1" } else { "2" })
    }

    #[test]
    fn the_3ds_menu_reads_rocknixs_keys_and_azahars_ini_and_restarts() {
        let file = "Game (USA).3ds";
        let cfg = format!(
            "rocknix.mangohud.enabled=0\n3ds.screen_layout=2\n{}=3\n",
            es_game_key("3ds", file, "resolution_scale")
        );
        let (f, _dir) = files("3ds", &[(SYSTEM_CFG, &cfg), (AZAHAR_QT, QT)]);
        let target = Target::Es {
            file: file.into(),
            armsx2_ini: None,
        };
        let mut m = Menu::load(Emu::Azahar, &N3DS, target, &f);
        let values: Vec<String> = m.rows().into_iter().map(|r| r.1).collect();
        assert_eq!(values, ["3x", "Large top", "Vulkan", "Off", "Off", "Max"]);
        assert_eq!(m.len(), 6, "no Advanced row");
        m.step(4, 1);
        assert_eq!(
            m.preview(),
            Plan {
                actions: vec![],
                restart: true
            }
        );
        // Every choice of the ROCKNIX-handled rows survives start_azahar.sh as the value it writes.
        let mut cfg = cfg.clone();
        for (s, choice) in N3DS.settings.iter().zip([2, 2, 1, 0, 0]).take(3) {
            let Store::Es(key) = s.store else { panic!("3DS settings live in system.cfg") };
            cfg = es_set(&cfg, &es_game_key("3ds", file, key), Some(s.choices[choice].stored));
        }
        let after = rocknix_start_azahar(&cfg, file, QT);
        for (s, choice) in N3DS.settings.iter().zip([2, 2, 1]) {
            assert_eq!(ini_get(&after, s.ini.1, s.ini.2).as_deref(), Some(s.choices[choice].emu), "{}", s.name);
        }
    }

    #[test]
    fn azahar_fps_is_rocknixs_mangohud_showing_only_the_fps() {
        let file = "Game (USA).3ds";
        assert!(game_env("3ds", "", file).is_empty());
        let cfg = es_set("rocknix.mangohud.enabled=0\n", &es_game_key("3ds", file, "rocknix.mangohud.enabled"), Some("1"));
        assert_eq!(game_env("3ds", &cfg, file), vec![("MANGOHUD_CONFIGFILE", MANGOHUD_FPS_CONF.to_string())]);
        assert!(game_env("3ds", &cfg, "Other.3ds").is_empty(), "only the game it was chosen for");
        assert!(game_env("ps2", &cfg, file).is_empty());
        assert!(mangohud_fps_config().lines().any(|l| l.trim() == "fps_only"));
        assert!(!mangohud_fps_config().lines().any(|l| l.trim() == "no_display"));
    }

    // ---- Dolphin's "More…" page: ROCKNIX's per-game keys of start_dolphin_gc.sh / start_dolphin_wii.sh

    #[test]
    fn gamecube_a_goes_right_unless_a_layout_was_chosen() {
        // Unset (or empty, which ROCKNIX's get_setting skips): east, A on the right button.
        assert_eq!(with_gamecube_layout_default("").as_deref(), Some("gamecube.gamecube_controller_profile=east\n"));
        let cfg = "global.autosave=1\ngamecube.gamecube_controller_profile=\nps2.internal_resolution=2\n";
        assert_eq!(
            with_gamecube_layout_default(cfg).as_deref(),
            Some("global.autosave=1\ngamecube.gamecube_controller_profile=east\nps2.internal_resolution=2\n")
        );
        // A choice, made anywhere, stays.
        for chosen in ["south", "east", "custom"] {
            let cfg = format!("global.autosave=1\ngamecube.gamecube_controller_profile={chosen}\n");
            assert_eq!(with_gamecube_layout_default(&cfg), None, "{chosen}");
        }
        // A per-game choice doesn't count as the platform's: the others still get the default.
        let per_game = "gamecube[\"Game.iso\"].gamecube_controller_profile=south\n";
        assert_eq!(
            with_gamecube_layout_default(per_game).as_deref(),
            Some("gamecube[\"Game.iso\"].gamecube_controller_profile=south\ngamecube.gamecube_controller_profile=east\n")
        );
    }

    #[test]
    fn dolphin_starts_with_asynchronous_shaders_unless_a_mode_was_chosen() {
        // Measured on the Mini V2 with Mario Kart: Double Dash: ROCKNIX's synchronous mode (0) dropped to 30 fps.
        let set = with_unset_defaults("", START_DEFAULTS).unwrap();
        for key in ["gamecube.shader_mode=3", "wii.shader_mode=3", "gamecube.gamecube_controller_profile=east"] {
            assert!(set.lines().any(|l| l == key), "{key} in {set}");
        }
        let chosen = "gamecube.shader_mode=0\nwii.shader_mode=2\ngamecube.gamecube_controller_profile=south\n";
        assert_eq!(with_unset_defaults(chosen, START_DEFAULTS), None, "a choice is never replaced");
        let partly = with_unset_defaults("gamecube.shader_mode=1\n", START_DEFAULTS).unwrap();
        assert_eq!(es_get(&partly, "gamecube", "x.iso", "shader_mode").as_deref(), Some("1"));
        assert_eq!(es_get(&partly, "wii", "x.wbfs", "shader_mode").as_deref(), Some("3"));
        // The More row offers all four of Dolphin's modes.
        let shaders = GC_MORE.settings.iter().find(|s| s.name == "Shaders").unwrap();
        let mut modes: Vec<&str> = shaders.choices.iter().map(|c| c.stored).collect();
        modes.sort();
        assert_eq!(modes, ["0", "1", "2", "3"]);
    }

    #[test]
    fn performance_is_max_unless_chosen_and_applies_live() {
        for t in [&PS2, &GAMECUBE, &WII, &N3DS] {
            let s = t
                .settings
                .iter()
                .find(|s| s.name == "Performance")
                .unwrap_or_else(|| panic!("{} has Performance", t.platform));
            assert_eq!(s.live, Live::Power, "{}", t.platform);
            assert_eq!(s.choices.iter().map(|c| c.label).collect::<Vec<_>>(), ["Max", "Auto", "Battery"]);
        }
        // Kept per game in system.cfg, also for GameCube, whose other options live in the game's INI.
        let file = "Mario Kart - Double Dash!! (USA).rvz";
        let game = dolphin_game_ini("GM4E01");
        let (f, _dir) = files("perf", &[(SYSTEM_CFG, "global.autosave=1\n"), (&game, DOLPHIN_GAME)]);
        let target = Target::DolphinIni(game.clone(), file.into());
        let mut m = Menu::load(Emu::Dolphin, &GAMECUBE, target.clone(), &f);
        let row = GAMECUBE.settings.iter().position(|s| s.name == "Performance").unwrap();
        assert_eq!(m.rows()[row].1, "Max");
        assert_eq!(m.own_performance(), None, "nothing of its own: Games: performance counts");
        m.step(row, 1);
        assert_eq!(
            m.preview(),
            Plan {
                actions: vec![Action::Power("balanced")],
                restart: false
            }
        );
        m.commit_to(&f).unwrap();
        let cfg = std::fs::read_to_string(&f.system_cfg).unwrap();
        assert_eq!(cfg, format!("global.autosave=1\ngamecube[\"{file}\"].pocket_performance=balanced\n"));
        assert_eq!(std::fs::read_to_string(f.at(&game)).unwrap(), DOLPHIN_GAME, "the game's INI is untouched");
        assert_eq!(m.own_performance(), Some("balanced"), "its own once saved (Quick settings say so)");
        let again = Menu::load(Emu::Dolphin, &GAMECUBE, target, &f);
        assert_eq!(again.rows()[row].1, "Auto", "Auto, stored as balanced (its name until 2026-10-23)");
        assert_eq!(again.own_performance(), Some("balanced"));
        assert_eq!(game_performance("gamecube", &cfg, file), "balanced");
        assert_eq!(game_performance("gamecube", &cfg, "Other.iso"), "max");
        // Without a choice of its own a game follows Games: performance; Auto is Max for these systems.
        use crate::power::Performance;
        assert_eq!(game_performance_or("gamecube", &cfg, "Other.iso", performance_for_default(Performance::Battery)), "battery");
        assert_eq!(game_performance_or("gamecube", &cfg, file, performance_for_default(Performance::Battery)), "balanced", "its own wins");
        assert_eq!(performance_for_default(Performance::Auto), "max");
        assert_eq!(performance_for_default(Performance::Performance), "max");
    }

    #[test]
    fn the_more_pages_rows_are_rocknixs_keys_and_all_restart() {
        for t in [&GC_MORE, &WII_MORE] {
            for s in t.settings {
                assert_eq!(s.live, Live::Restart, "{} {}", t.platform, s.name);
                assert!(matches!(s.store, Store::Es(_)), "{} {}: a system.cfg key", t.platform, s.name);
                assert!(!s.desc.is_empty());
            }
            assert!(t.more.is_none() && t.advanced.is_none());
        }
        let keys = |t: &Table| -> Vec<&str> {
            t.settings
                .iter()
                .map(|s| match s.store {
                    Store::Es(k) => k,
                    _ => "",
                })
                .collect()
        };
        assert_eq!(
            keys(&GC_MORE),
            [
                "gamecube_controller_profile",
                "swap_shoulders",
                "rumble",
                "shader_mode",
                "shader_precompile",
                "vsync",
                "clock_speed",
                "sync_gpu_thread",
                "skip_efb_cpu_access",
                "store_efb_to_texture_only",
                "store_xfb_to_texture_only",
                "texture_cache_accuracy"
            ]
        );
        // start_dolphin_wii.sh reads no GameCube layout or shoulders: it always copies the south profile.
        assert_eq!(keys(&WII_MORE)[0], "wii_controller_profile");
        assert!(!keys(&WII_MORE).contains(&"gamecube_controller_profile") && !keys(&WII_MORE).contains(&"swap_shoulders"));
        assert_eq!(GAMECUBE.more.map(|t| t.platform), Some("gamecube"));
        assert_eq!(WII.more.map(|t| t.platform), Some("wii"));
    }

    /// start_dolphin_gc.sh of ROCKNIX 20261001 for the More keys: what each per-game value turns into.
    fn rocknix_start_dolphin_gc(cfg: &str, file: &str) -> Vec<(&'static str, String)> {
        let get = |k| es_get(cfg, "gamecube", file, k).unwrap_or_default();
        let shoulders = if get("swap_shoulders") == "true" { "inline" } else { "stacked" };
        let profile = match get("gamecube_controller_profile").as_str() {
            "east" => format!("east.{shoulders}"),
            "custom" => "custom".into(),
            _ => format!("south.{shoulders}"),
        };
        let tf = |b: bool| if b { "True" } else { "False" }.to_string();
        vec![
            ("profile", profile),
            ("rumble", tf(get("rumble") != "false")),
            ("VSync", tf(get("vsync") == "1")),
            ("SyncGPU", tf(get("sync_gpu_thread") == "true")),
            ("EFBAccessEnable", tf(get("skip_efb_cpu_access") != "false")),
            ("EFBToTextureEnable", tf(get("store_efb_to_texture_only") != "false")),
            ("XFBToTextureEnable", tf(get("store_xfb_to_texture_only") != "false")),
            (
                "SafeTextureCacheColorSamples",
                match get("texture_cache_accuracy").as_str() {
                    v @ ("0" | "512") => v.to_string(),
                    _ => "128".into(),
                },
            ),
        ]
    }

    #[test]
    fn the_more_page_keeps_its_values_per_game_and_leaves_other_keys_alone() {
        let file = "Mario Kart - Double Dash!! (USA).rvz";
        let other = "gamecube[\"F-Zero GX (USA).iso\"].vsync=1";
        let cfg = format!("global.autosave=1\ngamecube.gamecube_controller_profile=east\n{other}\nps2.internal_resolution=2\n");
        let (f, _dir) = files("more", &[(SYSTEM_CFG, &cfg)]);
        let target = Target::Es {
            file: file.into(),
            armsx2_ini: None,
        };
        let mut m = Menu::load(Emu::Dolphin, &GC_MORE, target.clone(), &f);
        let values: Vec<String> = m.rows().into_iter().map(|r| r.1).collect();
        // The platform's layout (east) shows; everything else is what start_dolphin_gc.sh does when unset.
        assert_eq!(
            values,
            [
                "A right",
                "Triggers",
                "On",
                "Pause to compile",
                "On",
                "Off",
                "100%",
                "Off",
                "Accurate",
                "Fast",
                "Fast",
                "Fast"
            ]
        );
        let before = rocknix_start_dolphin_gc(&cfg, file);
        assert_eq!(before[0].1, "east.stacked");
        m.step(1, 1); // shoulders: bumpers
        m.step(5, 1); // VSync on
        m.step(6, 1); // CPU clock up a step
        assert!(m.changed());
        assert_eq!(
            m.preview(),
            Plan {
                actions: vec![],
                restart: true
            }
        );
        m.commit_to(&f).unwrap();
        let after = std::fs::read_to_string(&f.system_cfg).unwrap();
        // The rest of system.cfg is as it was, line for line, the other game's key included.
        let ours = |l: &&str| l.starts_with(&format!("gamecube[\"{file}\"]"));
        assert_eq!(after.lines().filter(|l| !ours(l)).collect::<Vec<_>>(), cfg.lines().collect::<Vec<_>>());
        assert_eq!(after.lines().filter(ours).count(), 3, "{after}");
        assert_eq!(es_get(&after, "gamecube", file, "swap_shoulders").as_deref(), Some("true"));
        assert_eq!(es_get(&after, "gamecube", file, "vsync").as_deref(), Some("1"));
        assert_eq!(es_get(&after, "gamecube", file, "clock_speed").as_deref(), Some("125"));
        // What ROCKNIX's script makes of it at the next start.
        let start = rocknix_start_dolphin_gc(&after, file);
        assert_eq!(start[0].1, "east.inline");
        assert_eq!(start[2].1, "True", "VSync");
        // Read back: the page shows the stored choice.
        let m = Menu::load(Emu::Dolphin, &GC_MORE, target, &f);
        assert_eq!(m.rows()[1].1, "Bumpers");
        assert_eq!(m.rows()[6].1, "125%");
    }

    #[test]
    fn custom_layouts_are_offered_only_when_there_is_one() {
        let stock = include_str!("fixtures/dolphin-gcpad-south.ini");
        assert!(
            !custom_profile_is_own(stock, stock),
            "ROCKNIX's copy of the south profile is not a layout of one's own"
        );
        assert!(custom_profile_is_own(&stock.replace("Buttons/A = `Button S`", "Buttons/A = `Button E`"), stock));
        assert!(!custom_profile_is_own("", stock), "missing");
        let (f, _dir) = files("custom", &[(SYSTEM_CFG, "")]);
        let target = Target::Es {
            file: "Game.iso".into(),
            armsx2_ini: None,
        };
        let mut m = Menu::load(Emu::Dolphin, &GC_MORE, target, &f);
        m.limit_choices(0, 2);
        let seen: Vec<String> = (0..4)
            .map(|_| {
                m.step(0, 1);
                m.rows()[0].1.clone()
            })
            .collect();
        assert_eq!(seen, ["A right", "A bottom", "A right", "A bottom"], "no Custom to step to");
    }

    #[test]
    fn hotkeys_follow_the_east_layout() {
        let cfg = with_gamecube_layout_default("").unwrap();
        assert_eq!(hotkey_profile("gamecube", &cfg, "Game.iso"), Some("east.stacked"));
        let swapped = es_set(&cfg, &es_game_key("gamecube", "Game.iso", "swap_shoulders"), Some("true"));
        assert_eq!(hotkey_profile("gamecube", &swapped, "Game.iso"), Some("east.inline"));
        assert_eq!(hotkey_profile("gamecube", &swapped, "Other.iso"), Some("east.stacked"));
        // East binds GC Y, X, B to `Button W`, `Button N`, `Button S`: InputPlumber North, West, South.
        let east = include_str!("fixtures/dolphin-gcpad-east.ini");
        assert_eq!(profile_button(east, "Buttons/A".trim_start_matches("Buttons/")), Some("Gamepad:Button:East"));
        assert_eq!(profile_button(east, toggle_gc_button(Toggle::Fps)), Some("Gamepad:Button:North"));
        assert_eq!(profile_button(east, toggle_gc_button(Toggle::Aspect)), Some("Gamepad:Button:West"));
        assert_eq!(profile_button(east, toggle_gc_button(Toggle::Resolution)), Some("Gamepad:Button:South"));
        // Wii ignores the GameCube layout: start_dolphin_wii.sh always copies the south profile.
        assert_eq!(hotkey_profile("wii", &cfg, "Game.wbfs"), Some("south.stacked"));
    }

    #[test]
    fn more_is_a_page_of_the_options_and_applies_with_them() {
        let (f, _dir) = files("morepage", &[(SYSTEM_CFG, "")]);
        let mut m = Menu::load(Emu::Dolphin, &GAMECUBE, Target::DolphinIni(dolphin_game_ini("GM4E01"), "Game.iso".into()), &f);
        m.attach_more(Menu::load(
            Emu::Dolphin,
            &GC_MORE,
            Target::Es {
                file: "Game.iso".into(),
                armsx2_ini: None,
            },
            &f,
        ));
        m.buttons = [Some("Gamepad:Button:North"), Some("Gamepad:Button:West"), Some("Gamepad:Button:South")];
        let n = GAMECUBE.settings.len();
        assert_eq!(m.len(), n + 1);
        assert_eq!(m.rows()[n].0, "More…");
        assert!(m.is_more(n) && !m.is_more(0) && !m.is_advanced(n));
        // A live change on the first page alone goes live...
        let fps = GAMECUBE.settings.iter().position(|s| s.name == "Show FPS").unwrap();
        m.step(fps, 1);
        assert!(!m.preview().restart && m.preview().actions.len() == 1);
        // ...until a More row changes too: then everything comes with the restart.
        m.open_more();
        assert_eq!(m.title(), "Dolphin   More");
        assert_eq!(m.len(), GC_MORE.settings.len());
        assert_eq!(m.rows()[0].0, "Button layout");
        m.step(2, 1);
        assert_eq!(
            m.preview(),
            Plan {
                actions: vec![],
                restart: true
            }
        );
        assert_eq!(m.close_more(), Some(n), "back on the More… row");
        assert_eq!(m.rows()[n].0, "More…");
        assert!(m.preview().restart);
    }

    #[test]
    fn every_table_is_complete() {
        for t in [&PS2, &GAMECUBE, &WII, &N3DS, &GC_MORE, &WII_MORE] {
            assert!(!t.settings.is_empty());
            for s in t.settings {
                // Not set anywhere means the emulator's default, listed first; every stored value is distinct.
                let stored: Vec<&str> = s.choices.iter().map(|c| c.stored).collect();
                assert!(stored.iter().enumerate().all(|(i, v)| !stored[..i].contains(v)), "{} {}", t.platform, s.name);
                assert!(s.choices.len() >= 2, "{} {}", t.title, s.name);
                if let Live::Toggle(tg) = s.live {
                    // Each value the hotkey reaches is a choice of the setting.
                    let mut st = Toggles::default();
                    for _ in 0..4 {
                        let v = st.press(tg);
                        assert!(tg == Toggle::Resolution && v == "1" || s.choices.iter().any(|c| c.emu == v), "{} {v}", s.name);
                    }
                }
            }
        }
    }

    #[test]
    fn the_xbox_menu_has_resolution_and_scaling_that_wait_for_the_next_start() {
        let t = table(Emu::Hakux, "xbox").expect("Xbox has options");
        assert_eq!(t.title, "hakuX");
        assert!(table(Emu::Hakux, "ps2").is_none());
        let names: Vec<&str> = t.settings.iter().map(|s| s.name).collect();
        assert_eq!(names, ["Resolution", "Screen scaling", "Performance"]);
        for s in t.settings.iter().filter(|s| s.name != "Performance") {
            assert_eq!(s.live, Live::NextStart, "{}: no restart, the game has no state to come back to", s.name);
            assert!(s.desc.contains("next time the game starts"), "{}: the row says when it applies", s.name);
        }
        // The first choice is the default: the labels are pocket-core's.
        let scaling = &t.settings[1];
        assert_eq!(scaling.choices[0].stored, "aspect");
        assert_eq!(scaling.choices.iter().map(|c| c.label).collect::<Vec<_>>(), ["Aspect", "Fullscreen", "Integer"]);
        assert_eq!(t.settings[0].choices[0].stored, crate::hakux::DEFAULT_SCALE, "the first choice is the default (measured)");
        assert_eq!(t.settings[0].choices.iter().map(|c| c.stored).collect::<Vec<_>>().len(), 2);
    }

}
