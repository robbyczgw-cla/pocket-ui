//! Core options: the settings a libretro core declares for itself (palette, colour correction,
//! sprite limit, …). pocket-core collects the declarations, answers the core's questions about
//! their values and writes a JSON snapshot the pause menu reads; pocket-ui shows it as the
//! "Emulator" group and sends changes back as `opt <key> <value>` over the command FIFO.
//!
//! Declarations come in five dialects: `SET_VARIABLES` (v0, "Label; a|b|c"), `SET_CORE_OPTIONS`
//! (v1), `_INTL` (v1 with a translation) and `SET_CORE_OPTIONS_V2` / `_V2_INTL` (v2, with categories
//! and per-option visibility). The `Raw*` structs mirror libretro.h byte for byte; everything is
//! copied out of the core's memory at once, because the core may free its tables afterwards.
//!
//! A value's origin, lowest to highest: the core's own default, our curated default for the core
//! (`curation`), what the user saved for the console or the game (`opt.<key>` in the game settings).

use std::collections::{BTreeMap, HashMap};
use std::ffi::{c_char, CStr, CString};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::controls::ControlsFile;

/// Prefix of core option keys inside the per-console / per-game settings files.
pub const KEY_PREFIX: &str = "opt.";

/// `RETRO_NUM_CORE_OPTION_VALUES_MAX`.
const MAX_VALUES: usize = 128;

#[repr(C)]
pub struct RawVariable {
    pub key: *const c_char,
    pub value: *const c_char,
}

#[repr(C)]
pub struct RawValue {
    pub value: *const c_char,
    pub label: *const c_char,
}

/// `retro_core_option_definition` (v1).
#[repr(C)]
pub struct RawDefinition {
    pub key: *const c_char,
    pub desc: *const c_char,
    pub info: *const c_char,
    pub values: [RawValue; MAX_VALUES],
    pub default_value: *const c_char,
}

/// `retro_core_options_intl`.
#[repr(C)]
pub struct RawIntl {
    pub us: *const RawDefinition,
    pub local: *const RawDefinition,
}

#[repr(C)]
pub struct RawCategory {
    pub key: *const c_char,
    pub desc: *const c_char,
    pub info: *const c_char,
}

/// `retro_core_option_v2_definition`.
#[repr(C)]
pub struct RawDefinitionV2 {
    pub key: *const c_char,
    pub desc: *const c_char,
    pub desc_categorised: *const c_char,
    pub info: *const c_char,
    pub info_categorised: *const c_char,
    pub category_key: *const c_char,
    pub values: [RawValue; MAX_VALUES],
    pub default_value: *const c_char,
}

/// `retro_core_options_v2`.
#[repr(C)]
pub struct RawV2 {
    pub categories: *const RawCategory,
    pub definitions: *const RawDefinitionV2,
}

/// `retro_core_options_v2_intl`.
#[repr(C)]
pub struct RawV2Intl {
    pub us: *const RawV2,
    pub local: *const RawV2,
}

/// `retro_core_option_display`.
#[repr(C)]
pub struct RawDisplay {
    pub key: *const c_char,
    pub visible: bool,
}

/// # Safety
/// `p` is null or points to a NUL-terminated string.
unsafe fn cstr(p: *const c_char) -> Option<String> {
    (!p.is_null()).then(|| CStr::from_ptr(p).to_string_lossy().into_owned())
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Category {
    pub key: String,
    pub label: String,
    pub info: String,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct OptionDef {
    pub key: String,
    pub label: String,
    pub info: String,
    pub category: Option<String>,
    /// (value, label)
    pub values: Vec<(String, String)>,
    /// Our default: the curated one when the core offers that value, else the core's own.
    pub default: String,
    /// The core can hide options that don't apply right now (`SET_CORE_OPTIONS_DISPLAY`).
    pub visible: bool,
    /// Left out of the menu by our curation; the default still applies.
    pub hidden: bool,
}

impl OptionDef {
    fn has_value(&self, v: &str) -> bool {
        self.values.iter().any(|(value, _)| value == v)
    }

    pub fn label_of<'a>(&'a self, value: &'a str) -> &'a str {
        self.values.iter().find(|(v, _)| v == value).map_or(value, |(_, l)| l.as_str())
    }
}

/// # Safety
/// `values` came from a core's definition table.
unsafe fn read_values(values: &[RawValue; MAX_VALUES]) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for v in values {
        let Some(value) = cstr(v.value) else { break };
        let label = cstr(v.label).filter(|l| !l.is_empty()).unwrap_or_else(|| value.clone());
        out.push((value, label));
    }
    out
}

fn finish(
    key: String,
    label: Option<String>,
    info: Option<String>,
    category: Option<String>,
    values: Vec<(String, String)>,
    default: Option<String>,
) -> OptionDef {
    let default = default.filter(|d| values.iter().any(|(v, _)| v == d)).or_else(|| values.first().map(|(v, _)| v.clone())).unwrap_or_default();
    OptionDef {
        label: label.filter(|l| !l.is_empty()).unwrap_or_else(|| key.clone()),
        key,
        info: info.unwrap_or_default(),
        category,
        values,
        default,
        visible: true,
        hidden: false,
    }
}

/// `SET_VARIABLES`: an array of {key, "Label; first|second|…"} ended by a NULL key. The first value is the default.
///
/// # Safety
/// `v` points to a table ended by a NULL key, as libretro requires.
pub unsafe fn parse_variables(mut v: *const RawVariable) -> Vec<OptionDef> {
    let mut out = Vec::new();
    while !v.is_null() && !(*v).key.is_null() && !(*v).value.is_null() {
        let key = cstr((*v).key).unwrap_or_default();
        let decl = cstr((*v).value).unwrap_or_default();
        let (label, vals) = decl.split_once("; ").unwrap_or((key.as_str(), ""));
        let values: Vec<(String, String)> = vals.split('|').filter(|s| !s.is_empty()).map(|s| (s.to_string(), s.to_string())).collect();
        out.push(finish(key.clone(), Some(label.to_string()), None, None, values, None));
        v = v.add(1);
    }
    out
}

/// `SET_CORE_OPTIONS` (v1): definitions ended by a NULL key.
///
/// # Safety
/// `d` points to a table ended by a NULL key.
pub unsafe fn parse_v1(mut d: *const RawDefinition) -> Vec<OptionDef> {
    let mut out = Vec::new();
    while !d.is_null() && !(*d).key.is_null() {
        let def = &*d;
        let values = read_values(&def.values);
        out.push(finish(cstr(def.key).unwrap_or_default(), cstr(def.desc), cstr(def.info), None, values, cstr(def.default_value)));
        d = d.add(1);
    }
    out
}

/// `SET_CORE_OPTIONS_V2`. Inside a category the shorter `*_categorised` texts are used, because the
/// category page already gives the context.
///
/// # Safety
/// `raw` points to a valid `retro_core_options_v2`.
pub unsafe fn parse_v2(raw: *const RawV2) -> (Vec<Category>, Vec<OptionDef>) {
    if raw.is_null() {
        return (Vec::new(), Vec::new());
    }
    let mut cats = Vec::new();
    let mut c = (*raw).categories;
    while !c.is_null() && !(*c).key.is_null() {
        let key = cstr((*c).key).unwrap_or_default();
        cats.push(Category { label: cstr((*c).desc).filter(|l| !l.is_empty()).unwrap_or_else(|| key.clone()), info: cstr((*c).info).unwrap_or_default(), key });
        c = c.add(1);
    }
    let mut out = Vec::new();
    let mut d = (*raw).definitions;
    while !d.is_null() && !(*d).key.is_null() {
        let def = &*d;
        let category = cstr(def.category_key).filter(|k| cats.iter().any(|c| c.key == *k));
        let (label, info) = if category.is_some() {
            (cstr(def.desc_categorised).or_else(|| cstr(def.desc)), cstr(def.info_categorised).or_else(|| cstr(def.info)))
        } else {
            (cstr(def.desc), cstr(def.info))
        };
        let values = read_values(&def.values);
        out.push(finish(cstr(def.key).unwrap_or_default(), label, info, category, values, cstr(def.default_value)));
        d = d.add(1);
    }
    (cats, out)
}

/// Our curation of one core: values that suit a handheld better than the core's own defaults, and
/// options that only confuse there (they keep their default and stay out of the menu). Keys a core
/// doesn't have, and values it doesn't offer, are ignored, so entries for newer or older versions
/// of a core are harmless. A key ending in `*` stands for every key with that start.
#[derive(Clone, Copy, Default)]
pub struct Curation {
    pub defaults: &'static [(&'static str, &'static str)],
    pub hidden: &'static [&'static str],
    /// When not empty, the menu shows only these (the other way round from `hidden`, for cores with dozens of knobs).
    pub only: &'static [&'static str],
}

fn key_matches(pattern: &str, key: &str) -> bool {
    match pattern.strip_suffix('*') {
        Some(prefix) => key.starts_with(prefix),
        None => pattern == key,
    }
}

impl Curation {
    fn shows(&self, key: &str) -> bool {
        (self.only.is_empty() || self.only.iter().any(|p| key_matches(p, key))) && !self.hidden.iter().any(|p| key_matches(p, key))
    }
}

/// Curation by the core's file name without `_libretro.so` (`gambatte`, `mgba`, …).
///
/// Checked against the option lists on the device (`pocket-core --dump-options`, 2026-10-21): gambatte,
/// mgba, nestopia, snes9x, genesis_plus_gx, beetle_pce_fast, pcsx_rearmed, picodrive. Cores that declare
/// their options only once a game is loaded (fbneo) and the 3D cores are not curated yet.
pub fn curation(core: &str) -> Curation {
    match core {
        // Game Boy and Game Boy Color. A DMG game gets its game-specific colours when the core knows
        // them (Auto), else the classic green. The start-up logo animation of an official boot ROM
        // costs seconds on every start, so it is off.
        "gambatte" => Curation {
            defaults: &[("gambatte_gb_colorization", "auto"), ("gambatte_gb_bootloader", "disabled")],
            hidden: &[
                "gambatte_audio_resampler",
                "gambatte_gb_hwmode",
                "gambatte_gb_bootloader",
                // Fan palette packs: each has its own option, which this version of the core shows whatever
                // "Internal Palette" says. The built-in GB and GBC palettes are plenty.
                "gambatte_gb_palette_*",
                "gambatte_turbo_period",
                "gambatte_rumble_level",
                "gambatte_show_gb_link_settings",
                "gambatte_gb_link_*",
            ],
            only: &[],
        },
        // Game Boy Advance. mGBA also plays Game Boy games, but those run on gambatte here, so the
        // Game Boy palette and model options are left out, as are BIOS choices (the intro is skipped)
        // and frameskip (the host paces frames itself).
        "mgba" => Curation {
            defaults: &[("mgba_skip_bios", "ON")],
            hidden: &["mgba_gb_*", "mgba_use_bios", "mgba_skip_bios", "mgba_sgb_borders", "mgba_force_gbp", "mgba_idle_optimization", "mgba_frameskip*"],
            only: &[],
        },
        // NES. Removing the sprite limit stops the flicker many games show, at no cost in accuracy that anyone sees.
        "nestopia" => Curation {
            defaults: &[("nestopia_nospritelimit", "enabled")],
            hidden: &[
                "nestopia_fds_savefile_format",
                "nestopia_dmc_pop_reducer",
                "nestopia_genie_distortion",
                "nestopia_show_advanced_av_settings",
                "nestopia_audio_vol_*",
                "nestopia_select_adapter",
                "nestopia_button_shift",
                "nestopia_arkanoid_*",
                "nestopia_zapper_device",
                "nestopia_show_crosshair",
                "nestopia_turbo_pulse",
                "nestopia_ram_power_state",
            ],
            only: &[],
        },
        // SNES. Light guns, per-channel volumes and layer switches are for debugging or for hardware this device doesn't have.
        "snes9x" => Curation {
            defaults: &[],
            hidden: &[
                "snes9x_region",
                "snes9x_audio_interpolation",
                "snes9x_superfx_timing",
                "snes9x_randomize_memory",
                "snes9x_block_invalid_vram_access",
                "snes9x_echo_buffer_hack",
                "snes9x_msu1_enhanced_audio",
                "snes9x_show_lightgun_settings",
                "snes9x_lightgun_mode",
                "snes9x_superscope_*",
                "snes9x_justifier*",
                "snes9x_rifle_*",
                "snes9x_show_advanced_av_settings",
                "snes9x_layer_*",
                "snes9x_gfx_transp",
                "snes9x_sndchan_volume_*",
            ],
            only: &[],
        },
        // Mega Drive, Master System, Game Gear (and Mega CD). The low-pass filter takes the harshness out of the FM chip, as the
        // analog output stage of the real console did.
        "genesis_plus_gx" | "genesis_plus_gx_wide" => Curation {
            defaults: &[("genesis_plus_gx_no_sprite_limit", "enabled"), ("genesis_plus_gx_audio_filter", "low-pass")],
            hidden: &[
                "genesis_plus_gx_system_hw",
                "genesis_plus_gx_vdp_mode",
                "genesis_plus_gx_bios",
                "genesis_plus_gx_system_bram",
                "genesis_plus_gx_cart_bram",
                "genesis_plus_gx_cart_size",
                "genesis_plus_gx_add_on",
                "genesis_plus_gx_lock_on",
                "genesis_plus_gx_render",
                "genesis_plus_gx_frameskip*",
                "genesis_plus_gx_ym2413*",
                "genesis_plus_gx_cdda_volume",
                "genesis_plus_gx_pcm_volume",
                "genesis_plus_gx_audio_eq_*",
                "genesis_plus_gx_gun_*",
                "genesis_plus_gx_invert_mouse",
                "genesis_plus_gx_enhanced_vscroll*",
                "genesis_plus_gx_overclock",
                "genesis_plus_gx_force_dtack",
                "genesis_plus_gx_addr_error",
                "genesis_plus_gx_cd_*",
                "genesis_plus_gx_show_advanced_audio_settings",
                "genesis_plus_gx_*_channel_*",
            ],
            only: &[],
        },
        // PC Engine / TurboGrafx-16.
        "beetle_pce_fast" => Curation {
            defaults: &[("pce_fast_nospritelimit", "enabled")],
            hidden: &[],
            only: &["pce_fast_palette", "pce_fast_nospritelimit", "pce_fast_default_joypad_type_p1", "pce_fast_cdbios", "pce_fast_hoverscan"],
        },
        // PlayStation: only what changes the picture or the sound; the rest are CPU tricks and gun calibration.
        "pcsx_rearmed" => Curation {
            defaults: &[("pcsx_rearmed_show_bios_bootlogo", "disabled"), ("pcsx_rearmed_display_info", "disabled")],
            hidden: &[],
            only: &[
                "pcsx_rearmed_neon_enhancement_enable",
                "pcsx_rearmed_dithering",
                "pcsx_rearmed_spu_reverb",
                "pcsx_rearmed_spu_interpolation",
                "pcsx_rearmed_region",
                "pcsx_rearmed_memcard1",
            ],
        },
        "picodrive" => Curation {
            defaults: &[("picodrive_sprlim", "enabled")],
            hidden: &[],
            only: &[
                "picodrive_region",
                "picodrive_aspect",
                "picodrive_ggghost",
                "picodrive_sprlim",
                "picodrive_audio_filter",
                "picodrive_lowpass_range",
                "picodrive_input1",
                "picodrive_input2",
            ],
        },
        // Dreamcast. One memory card per game, as ROCKNIX's standalone Flycast keeps them; pocket-core
        // links it to that emulator's file (`vfs::share_flycast_vmus`), so saves carry over both ways.
        "flycast" => Curation {
            defaults: &[("flycast_per_content_vmus", "VMU A1")],
            hidden: &[
                "flycast_per_content_vmus",
                "flycast_vmu*",
                "flycast_lightgun*",
                "flycast_show_*",
                "flycast_network_output",
                "flycast_upnp",
                "flycast_emulate_bba",
            ],
            only: &[],
        },
        // Nintendo DS. The 1240x1080 screen is nearly square, so the two screens stacked fill it best
        // (720x1080, each screen 2.8x); R3 cycles to side by side (the standalone melonDS layout) and to
        // the top screen alone. Touch: right stick moves the pointer, R2 touches (or the touch screen).
        "melondsds" => Curation {
            defaults: &[
                ("melonds_number_of_screen_layouts", "3"),
                ("melonds_screen_layout1", "top-bottom"),
                ("melonds_screen_layout2", "left-right"),
                ("melonds_screen_layout3", "top"),
                ("melonds_touch_mode", "auto"),
                ("melonds_show_cursor", "timeout"),
            ],
            hidden: &[
                "melonds_jit_*",
                "melonds_network_mode",
                "melonds_mac_address_mode",
                "melonds_firmware_wfc_dns",
                "melonds_dsi_*",
                "melonds_homebrew_*",
                "melonds_sysfile_mode",
                "melonds_battery_update_interval",
                "melonds_ds_battery_ok_threshold",
                "melonds_solar_sensor_host_sensor",
                "melonds_show_unsupported_features",
                "melonds_show_bios_warnings",
                "melonds_audio_bitdepth",
            ],
            only: &[],
        },
        _ => Curation::default(),
    }
}

/// The core's id as used in `curation`: `/usr/lib/libretro/mgba_libretro.so` → `mgba`.
pub fn core_id(core_path: &Path) -> String {
    let stem = core_path.file_stem().and_then(|s| s.to_str()).unwrap_or_default();
    stem.strip_suffix("_libretro").unwrap_or(stem).to_string()
}

/// All options of the running core and their current values.
#[derive(Default)]
pub struct OptionSet {
    pub categories: Vec<Category>,
    pub options: Vec<OptionDef>,
    current: HashMap<String, String>,
    /// Every value `GET_VARIABLE` has handed to the core, by value. libretro lets a core keep that pointer (many
    /// compare it with the next one, or read it again later), so none is freed while the core runs: a change
    /// from the pause menu or a repeated `SET_VARIABLES` leaves them alone. One copy per distinct value, so
    /// this stays as small as the core's value lists. A `CString` keeps its bytes in place when the map grows.
    handed_out: HashMap<String, CString>,
    curated: HashMap<String, String>,
    curation: Curation,
    /// User-saved values (`opt.*` of the console/game file), without the prefix.
    saved: HashMap<String, String>,
    /// A value changed since the core last asked (`GET_VARIABLE_UPDATE`).
    changed: bool,
    /// The snapshot file is out of date.
    dirty: bool,
}

impl OptionSet {
    pub fn new(curation: Curation, saved: &[(String, String)]) -> Self {
        OptionSet {
            curated: curation.defaults.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
            curation,
            saved: saved.iter().cloned().collect(),
            ..Default::default()
        }
    }

    /// The core declares (or re-declares) its options. Values already chosen stay when still valid.
    pub fn declare(&mut self, categories: Vec<Category>, defs: Vec<OptionDef>) {
        let old_visible: HashMap<String, bool> = self.options.iter().map(|o| (o.key.clone(), o.visible)).collect();
        self.categories = categories;
        self.options.clear();
        for mut def in defs {
            if def.values.is_empty() {
                continue;
            }
            if let Some(c) = self.curated.get(&def.key).filter(|c| def.has_value(c)) {
                def.default = c.clone();
            }
            def.hidden = !self.curation.shows(&def.key);
            def.visible = old_visible.get(&def.key).copied().unwrap_or(true);
            let value = self
                .current
                .get(&def.key)
                .map(String::as_str)
                .filter(|v| def.has_value(v))
                .or_else(|| self.saved.get(&def.key).map(String::as_str).filter(|v| def.has_value(v)))
                .unwrap_or(&def.default)
                .to_string();
            self.current.insert(def.key.clone(), value);
            if let Some(slot) = self.options.iter_mut().find(|o| o.key == def.key) {
                *slot = def;
            } else {
                self.options.push(def);
            }
        }
        self.current.retain(|k, _| self.options.iter().any(|o| o.key == *k));
        self.changed = true;
        self.dirty = true;
    }

    pub fn def(&self, key: &str) -> Option<&OptionDef> {
        self.options.iter().find(|o| o.key == key)
    }

    /// The answer to `GET_VARIABLE`: the current value as a C string that stays valid as long as this option
    /// set (until the next `Core::load`), see `handed_out`.
    pub fn get_c(&mut self, key: &str) -> Option<*const c_char> {
        let value = self.current.get(key)?;
        if let Some(c) = self.handed_out.get(value) {
            return Some(c.as_ptr());
        }
        let c = CString::new(value.as_str()).unwrap_or_default();
        let ptr = c.as_ptr();
        self.handed_out.insert(value.clone(), c);
        Some(ptr)
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.current.get(key).map(String::as_str)
    }

    /// Sets a value if the core offers it. Returns true when it changed.
    pub fn set(&mut self, key: &str, value: &str) -> bool {
        let Some(def) = self.def(key) else { return false };
        if !def.has_value(value) || self.get(key) == Some(value) {
            return false;
        }
        self.current.insert(key.to_string(), value.to_string());
        self.changed = true;
        self.dirty = true;
        true
    }

    /// The core shows or hides an option (`SET_CORE_OPTIONS_DISPLAY`).
    pub fn set_visible(&mut self, key: &str, visible: bool) {
        if let Some(o) = self.options.iter_mut().find(|o| o.key == key) {
            if o.visible != visible {
                o.visible = visible;
                self.dirty = true;
            }
        }
    }

    /// Back to default + the given saved values (the pause menu's Restore defaults or a reload after saving).
    /// Returns true when any value changed.
    pub fn reload(&mut self, saved: &[(String, String)]) -> bool {
        self.saved = saved.iter().cloned().collect();
        let mut any = false;
        let wanted: Vec<(String, String)> = self
            .options
            .iter()
            .map(|o| (o.key.clone(), self.saved.get(&o.key).filter(|v| o.has_value(v)).cloned().unwrap_or_else(|| o.default.clone())))
            .collect();
        for (k, v) in wanted {
            any |= self.set(&k, &v);
        }
        any
    }

    /// True once after a value changed; this is the answer to `GET_VARIABLE_UPDATE`.
    pub fn take_changed(&mut self) -> bool {
        std::mem::take(&mut self.changed)
    }

    /// True once after something the snapshot file shows has changed.
    pub fn take_dirty(&mut self) -> bool {
        std::mem::take(&mut self.dirty)
    }

    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    /// What the menu shows: visible, not curated away, and with a real choice.
    pub fn snapshot(&self) -> (Vec<FileCategory>, Vec<FileOption>) {
        let options: Vec<FileOption> = self
            .options
            .iter()
            .filter(|o| o.visible && !o.hidden && o.values.len() > 1)
            .map(|o| FileOption {
                key: o.key.clone(),
                label: o.label.clone(),
                info: o.info.clone(),
                category: o.category.clone(),
                values: o.values.iter().map(|(value, label)| FileValue { value: value.clone(), label: label.clone() }).collect(),
                default: o.default.clone(),
                current: self.get(&o.key).unwrap_or(&o.default).to_string(),
            })
            .collect();
        let categories = self
            .categories
            .iter()
            .filter(|c| options.iter().any(|o| o.category.as_deref() == Some(&c.key)))
            .map(|c| FileCategory { key: c.key.clone(), label: c.label.clone(), info: c.info.clone() })
            .collect();
        (categories, options)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct FileValue {
    pub value: String,
    pub label: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct FileCategory {
    pub key: String,
    pub label: String,
    #[serde(default)]
    pub info: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct FileOption {
    pub key: String,
    pub label: String,
    #[serde(default)]
    pub info: String,
    #[serde(default)]
    pub category: Option<String>,
    pub values: Vec<FileValue>,
    pub default: String,
    pub current: String,
}

impl FileOption {
    pub fn label_of<'a>(&'a self, value: &'a str) -> &'a str {
        self.values.iter().find(|v| v.value == value).map_or(value, |v| v.label.as_str())
    }
}

/// What pocket-core tells the pause menu about the running game: options and button names.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct CoreOptionsFile {
    pub core: String,
    /// The ROM this snapshot belongs to, so a file left by an earlier game is not mistaken for this one.
    pub rom: String,
    #[serde(default)]
    pub categories: Vec<FileCategory>,
    #[serde(default)]
    pub options: Vec<FileOption>,
    #[serde(default)]
    pub controls: ControlsFile,
}

pub fn file_path(data_dir: &Path, system: &str) -> PathBuf {
    data_dir.join("core-options").join(format!("{system}.json"))
}

impl CoreOptionsFile {
    /// Written through a temporary file, so the menu never reads half a file.
    pub fn write(&self, path: &Path) -> bool {
        serde_json::to_vec_pretty(self).is_ok_and(|json| crate::fsutil::write_atomic(path, &json).is_ok())
    }

    /// The snapshot for `rom`, if pocket-core has written one for that very game.
    pub fn read(path: &Path, rom: &Path) -> Option<CoreOptionsFile> {
        let file: CoreOptionsFile = serde_json::from_slice(&std::fs::read(path).ok()?).ok()?;
        (file.rom == rom.to_string_lossy()).then_some(file)
    }
}

/// `opt.<key>` entries of a settings map, as (key, value) pairs.
pub fn saved_from(values: &BTreeMap<String, String>) -> Vec<(String, String)> {
    values.iter().filter_map(|(k, v)| k.strip_prefix(KEY_PREFIX).map(|k| (k.to_string(), v.clone()))).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;

    struct Strings(Vec<CString>);
    impl Strings {
        fn p(&mut self, s: &str) -> *const c_char {
            self.0.push(CString::new(s).unwrap());
            self.0.last().unwrap().as_ptr()
        }
    }

    fn values(s: &mut Strings, list: &[(&str, &str)]) -> [RawValue; MAX_VALUES] {
        let mut out: [RawValue; MAX_VALUES] = std::array::from_fn(|_| RawValue { value: std::ptr::null(), label: std::ptr::null() });
        for (i, (v, l)) in list.iter().enumerate() {
            out[i] = RawValue { value: s.p(v), label: if l.is_empty() { std::ptr::null() } else { s.p(l) } };
        }
        out
    }

    #[test]
    fn parses_the_old_variable_format() {
        let mut s = Strings(Vec::new());
        let table = [
            RawVariable { key: s.p("nes_palette"), value: s.p("Palette; consumer|ntsc|pal") },
            RawVariable { key: s.p("nes_clip"), value: s.p("Clip overscan; enabled|disabled") },
            RawVariable { key: std::ptr::null(), value: std::ptr::null() },
        ];
        let defs = unsafe { parse_variables(table.as_ptr()) };
        assert_eq!(defs.len(), 2);
        assert_eq!((defs[0].key.as_str(), defs[0].label.as_str(), defs[0].default.as_str()), ("nes_palette", "Palette", "consumer"));
        assert_eq!(defs[0].values.len(), 3);
        assert_eq!(defs[1].values[1].0, "disabled");
    }

    #[test]
    fn parses_v1_with_labels_default_and_missing_info() {
        let mut s = Strings(Vec::new());
        let table = [
            RawDefinition {
                key: s.p("gb_pal"),
                desc: s.p("GB palette"),
                info: s.p("Pick one.\nIt applies at once."),
                values: values(&mut s, &[("a", "Alpha"), ("b", ""), ("c", "Gamma")]),
                default_value: s.p("b"),
            },
            RawDefinition {
                key: std::ptr::null(),
                desc: std::ptr::null(),
                info: std::ptr::null(),
                values: values(&mut s, &[]),
                default_value: std::ptr::null(),
            },
        ];
        let defs = unsafe { parse_v1(table.as_ptr()) };
        assert_eq!(defs.len(), 1);
        let d = &defs[0];
        assert_eq!((d.label.as_str(), d.default.as_str()), ("GB palette", "b"));
        assert_eq!(d.values[1], ("b".to_string(), "b".to_string()), "a missing label falls back to the value");
        assert!(d.info.contains('\n'));
    }

    #[test]
    fn parses_v2_categories_and_uses_the_categorised_texts() {
        let mut s = Strings(Vec::new());
        let cats = [
            RawCategory { key: s.p("video"), desc: s.p("Video"), info: s.p("Picture options.") },
            RawCategory { key: std::ptr::null(), desc: std::ptr::null(), info: std::ptr::null() },
        ];
        let defs = [
            RawDefinitionV2 {
                key: s.p("color"),
                desc: s.p("Colour correction (video)"),
                desc_categorised: s.p("Colour correction"),
                info: s.p("Long info."),
                info_categorised: s.p("Short info."),
                category_key: s.p("video"),
                values: values(&mut s, &[("off", "Off"), ("on", "On")]),
                default_value: std::ptr::null(),
            },
            RawDefinitionV2 {
                key: s.p("loose"),
                desc: s.p("Loose option"),
                desc_categorised: s.p("Loose (cat)"),
                info: s.p("Info."),
                info_categorised: std::ptr::null(),
                category_key: s.p("no-such-category"),
                values: values(&mut s, &[("x", "X")]),
                default_value: std::ptr::null(),
            },
            RawDefinitionV2 {
                key: std::ptr::null(),
                desc: std::ptr::null(),
                desc_categorised: std::ptr::null(),
                info: std::ptr::null(),
                info_categorised: std::ptr::null(),
                category_key: std::ptr::null(),
                values: values(&mut s, &[]),
                default_value: std::ptr::null(),
            },
        ];
        let raw = RawV2 { categories: cats.as_ptr(), definitions: defs.as_ptr() };
        let (c, d) = unsafe { parse_v2(&raw) };
        assert_eq!(c.len(), 1);
        assert_eq!((d[0].label.as_str(), d[0].info.as_str(), d[0].category.as_deref()), ("Colour correction", "Short info.", Some("video")));
        assert_eq!(d[0].default, "off", "no default means the first value");
        assert_eq!((d[1].label.as_str(), d[1].category.clone()), ("Loose option", None), "an unknown category is no category");
    }

    fn def(key: &str, values: &[&str]) -> OptionDef {
        finish(key.into(), Some(key.into()), None, None, values.iter().map(|v| (v.to_string(), v.to_string())).collect(), None)
    }

    #[test]
    fn value_precedence_core_then_curated_then_saved() {
        let cur = Curation { defaults: &[("pal", "green"), ("sprites", "limited")], hidden: &["tech"], only: &[] };
        let mut o = OptionSet::new(cur, &[("pal".to_string(), "blue".to_string()), ("bogus".to_string(), "x".to_string())]);
        o.declare(vec![], vec![def("pal", &["grey", "green", "blue"]), def("sprites", &["off", "on"]), def("tech", &["a", "b"]), def("plain", &["1", "2"])]);
        assert_eq!(o.get("pal"), Some("blue"), "saved wins");
        assert_eq!(o.def("pal").unwrap().default, "green", "the menu's default is the curated one");
        assert_eq!(o.get("sprites"), Some("off"), "a curated value the core doesn't offer is ignored");
        assert_eq!(o.get("plain"), Some("1"));
        let (_, shown) = o.snapshot();
        assert!(shown.iter().all(|s| s.key != "tech"), "curated away");
        assert_eq!(o.get("tech"), Some("a"), "but still has a value");
    }

    #[test]
    fn set_validates_and_flags_the_core() {
        let mut o = OptionSet::default();
        o.declare(vec![], vec![def("a", &["x", "y"])]);
        assert!(o.take_changed(), "declaring counts as a change: the core reads everything once");
        assert!(!o.take_changed());
        assert!(!o.set("a", "nope") && !o.set("missing", "x") && !o.set("a", "x"));
        assert!(!o.take_changed());
        assert!(o.set("a", "y"));
        assert!(o.take_changed() && !o.take_changed());
        assert_eq!(unsafe { CStr::from_ptr(o.get_c("a").unwrap()) }, c"y");
    }

    #[test]
    fn values_handed_to_the_core_outlive_changes() {
        let mut o = OptionSet::default();
        o.declare(vec![], vec![def("a", &["x", "y"])]);
        let x = o.get_c("a").unwrap();
        assert_eq!(o.get_c("a"), Some(x), "the same value is handed out once");
        o.set("a", "y");
        let y = o.get_c("a").unwrap();
        o.declare(vec![], vec![def("a", &["x", "y"])]);
        o.set("a", "x");
        assert_eq!(o.get_c("a"), Some(x), "back to a value handed out before: the same string");
        unsafe {
            assert_eq!((CStr::from_ptr(x), CStr::from_ptr(y)), (c"x", c"y"));
        }
    }

    #[test]
    fn redeclaring_keeps_chosen_values_and_visibility() {
        let mut o = OptionSet::default();
        o.declare(vec![], vec![def("a", &["x", "y"]), def("b", &["1", "2"])]);
        o.set("a", "y");
        o.set_visible("b", false);
        o.declare(vec![], vec![def("a", &["x", "y", "z"]), def("b", &["1", "2"])]);
        assert_eq!(o.get("a"), Some("y"));
        assert!(!o.def("b").unwrap().visible);
        let (_, shown) = o.snapshot();
        assert_eq!(shown.iter().map(|s| s.key.as_str()).collect::<Vec<_>>(), ["a"], "hidden options stay out of the menu");
    }

    #[test]
    fn reload_goes_back_to_defaults_plus_saved() {
        let mut o = OptionSet::default();
        o.declare(vec![], vec![def("a", &["x", "y"]), def("b", &["1", "2", "3"])]);
        o.set("a", "y");
        o.set("b", "3");
        o.take_changed();
        assert!(o.reload(&[("b".to_string(), "2".to_string())]));
        assert_eq!((o.get("a"), o.get("b")), (Some("x"), Some("2")));
        assert!(o.take_changed());
        assert!(!o.reload(&[("b".to_string(), "2".to_string())]), "nothing left to change");
    }

    #[test]
    fn single_value_options_and_empty_categories_are_not_shown() {
        let mut o = OptionSet::default();
        let mut one = def("one", &["only"]);
        one.category = Some("cat".into());
        let mut two = def("two", &["a", "b"]);
        two.category = Some("other".into());
        o.declare(
            vec![
                Category { key: "cat".into(), label: "Cat".into(), info: String::new() },
                Category { key: "other".into(), label: "Other".into(), info: String::new() },
            ],
            vec![one, two],
        );
        let (cats, opts) = o.snapshot();
        assert_eq!((cats.len(), opts.len()), (1, 1));
        assert_eq!(cats[0].key, "other");
    }

    #[test]
    fn file_roundtrip_and_rom_check() {
        let dir = TestDir::new("coreopts");
        let path = file_path(&dir, "gbc");
        let file = CoreOptionsFile {
            core: "Gambatte".into(),
            rom: "/roms/gbc/a.gbc".into(),
            categories: vec![],
            options: vec![FileOption {
                key: "k".into(),
                label: "K".into(),
                values: vec![FileValue { value: "v".into(), label: "V".into() }],
                default: "v".into(),
                current: "v".into(),
                ..Default::default()
            }],
            controls: Default::default(),
        };
        assert!(file.write(&path));
        assert_eq!(CoreOptionsFile::read(&path, Path::new("/roms/gbc/a.gbc")), Some(file));
        assert_eq!(CoreOptionsFile::read(&path, Path::new("/roms/gbc/b.gbc")), None, "another game's snapshot is ignored");
    }

    #[test]
    fn saved_keys_are_picked_out_by_prefix() {
        let mut m = BTreeMap::new();
        m.insert("opt.gambatte_x".to_string(), "1".to_string());
        m.insert("scaling".to_string(), "native".to_string());
        m.insert("ctl.a".to_string(), "b".to_string());
        assert_eq!(saved_from(&m), vec![("gambatte_x".to_string(), "1".to_string())]);
    }

    #[test]
    fn curation_hides_by_name_prefix_or_whitelist() {
        let c = Curation { defaults: &[], hidden: &["x_vol_*", "x_one"], only: &[] };
        assert!(c.shows("x_other") && !c.shows("x_vol_3") && !c.shows("x_one") && c.shows("x_one_more"));
        let c = Curation { defaults: &[], hidden: &["x_bad"], only: &["x_good*", "x_bad"] };
        assert!(c.shows("x_good_1") && !c.shows("x_bad") && !c.shows("x_none"));
    }

    #[test]
    fn every_curated_default_names_a_real_looking_key() {
        // A typo in the table would silently do nothing: keys start with their core's prefix.
        for (core, prefix) in [
            ("gambatte", "gambatte_"),
            ("mgba", "mgba_"),
            ("nestopia", "nestopia_"),
            ("snes9x", "snes9x_"),
            ("genesis_plus_gx", "genesis_plus_gx_"),
            ("beetle_pce_fast", "pce_fast_"),
            ("pcsx_rearmed", "pcsx_rearmed_"),
            ("picodrive", "picodrive_"),
        ] {
            let c = curation(core);
            for (k, v) in c.defaults {
                assert!(k.starts_with(prefix) && !v.is_empty(), "{core}: {k}");
            }
            for k in c.hidden.iter().chain(c.only) {
                assert!(k.starts_with(prefix), "{core}: {k}");
            }
        }
    }

    #[test]
    fn core_id_strips_the_libretro_suffix() {
        assert_eq!(core_id(Path::new("/usr/lib/libretro/mgba_libretro.so")), "mgba");
        assert_eq!(core_id(Path::new("/x/custom.so")), "custom");
    }
}
