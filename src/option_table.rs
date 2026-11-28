//! The game settings' table, as both players read it: the groups and rows of Options (`GROUPS`), their values and words,
//! which consoles a row or a value is for (`applies_in`, `offered_in`), the consoles' starting values. pocket-ui and
//! pocket-core use it through `options` (with `GameSettings`, the rows' filtering by the values in use, the explanations);
//! Android's player reads the same table for its pause menu's Picture page, so its rows are Linux's rows.

use crate::look;

/// Over the game's keys (`over_game::KEY`, `over_game::SHORTCUT_KEY`; `over_game` itself is Linux's).
pub const OVER_GAME_KEY: &str = "over_game";
pub const OVER_GAME_SHORTCUT_KEY: &str = "hk_over";

/// Run-ahead is pocket-core's (`run_ahead`): the player has none.
#[cfg(any(target_os = "linux", target_os = "emscripten", all(target_os = "android", feature = "handheld")))]
fn run_ahead_for(system: &str) -> bool {
    crate::run_ahead::available_for(system)
}
#[cfg(all(target_os = "android", not(feature = "handheld")))]
fn run_ahead_for(_: &str) -> bool {
    false
}

/// A fan to set (`device`): the player leaves the fan to Android.
#[cfg(any(target_os = "linux", target_os = "emscripten", all(target_os = "android", feature = "handheld")))]
fn has_fan() -> bool {
    crate::device::current().has_fan()
}
#[cfg(all(target_os = "android", not(feature = "handheld")))]
fn has_fan() -> bool {
    false
}

/// What a game's "This game" page offers on Android's handheld view: the settings of the picture the Android player draws
/// (scaling, the Look with the GBA's LCD colours, the frame with its finish or cabinet; every Look but Custom, so Sharpness and
/// Effect are not there). Everything else of Linux's list is pocket-core's: the
/// player has no rewind or Earlier setting, no fast-forward, rumble, fan or run-ahead, no screen turn and no HOME shortcuts, so a
/// row for them would be a switch that does nothing. Over the game (FPS) is in the player's own Options, kept by the player.
pub const ANDROID_PLAYER_SETTINGS: &[&str] = &["scaling", "look", "lcd_colour", "frame", "frame_colour", "frame_tv", "frame_brick", "frame_pocket", "frame_clamshell"];

/// Whether a game setting is out of the page, given Android's profile.
pub fn game_setting_out_in(key: &str, android: bool) -> bool {
    android && !ANDROID_PLAYER_SETTINGS.contains(&key)
}

/// Whether the console's games have save states (ScummVM, PICO-8 and TIC-80 have none: `hostsys`).
#[cfg(any(target_os = "linux", target_os = "emscripten", all(target_os = "android", feature = "handheld")))]
fn has_states(system: &str) -> bool {
    crate::hostsys::find(system).is_none_or(|h| h.no_states.is_none())
}
#[cfg(all(target_os = "android", not(feature = "handheld")))]
fn has_states(_: &str) -> bool {
    true
}

pub struct Choice {
    pub value: &'static str,
    pub label: &'static str,
}

pub struct Setting {
    pub key: &'static str,
    pub name: &'static str,
    pub desc: &'static str,
    pub choices: &'static [Choice],
    /// Index into `choices`.
    pub default: usize,
    /// Shown only while another setting has this value, (key, value): a sub-choice such as the
    /// shell colour, which means nothing for the other frames.
    pub shown_when: Option<(&'static str, &'static str)>,
}

pub struct Group {
    pub name: &'static str,
    pub desc: &'static str,
    pub settings: &'static [Setting],
}

/// The Picture group's Look row: derived, never saved (`look`).
pub const LOOK: &str = "look";

pub(crate) const fn c(value: &'static str, label: &'static str) -> Choice {
    Choice { value, label }
}

pub const GROUPS: &[Group] = &[
    Group {
        name: "Picture",
        desc: "Size, look, frame and orientation.",
        settings: &[
            Setting {
                key: "scaling",
                name: "Screen scaling",
                desc: "Integer uses whole-number scaling for razor-sharp, even pixels.\nInteger, smaller steps down once to leave room for a frame.\nAspect scales as big as fits in the original ratio.\nFullscreen fills the display by stretching.",
                choices: &[c("native", "Integer"), c("integer-smaller", "Integer, smaller"), c("aspect", "Aspect"), c("fullscreen", "Fullscreen")],
                default: 0,
                shown_when: None,
            },
            // Not a saved value: the Look worked out from `sharpness` + `effect` (`look::derive`), whose choosing writes both
            // (`GameSettings::step`). Left out of `effective()` and never sent to pocket-core as "set look".
            Setting {
                key: LOOK,
                name: "Look",
                desc: "Pick a display preset, or Custom for Sharpness and Effect.",
                choices: &[
                    c("clean", "Clean"),
                    c("smooth", "Smooth"),
                    c("soft", "Soft"),
                    c("bloom", "Bloom"),
                    c("lcd", "LCD grid"),
                    c("scanlines", "Scanlines"),
                    c("oldtv", "Old TV"),
                    c("custom", "Custom"),
                ],
                default: 0,
                shown_when: None,
            },
            // Under Look = Custom only. The frame is another row (Screen frame); this is the picture itself.
            Setting {
                key: "sharpness",
                name: "Sharpness",
                desc: "How scaled pixel edges are rendered.",
                choices: &[c("sharp", "Sharp"), c("crisp", "Crisp"), c("soft", "Soft")],
                default: 0,
                shown_when: Some((LOOK, "custom")),
            },
            Setting {
                key: "effect",
                name: "Effect",
                desc: "Shader overlay on the image.",
                choices: &[c("none", "None"), c("grid", "LCD grid"), c("scanlines", "Scanlines"), c("crt", "Old TV"), c("softglow", "Bloom"), c("smooth", "Smooth")],
                default: 0,
                shown_when: Some((LOOK, "custom")),
            },
            // Only for the Game Boy Advance (`applies`): its screen's colours, with the LCD grid (the Look, or Custom's Effect).
            Setting {
                key: "lcd_colour",
                name: "LCD colours",
                desc: "Matches the real Game Boy Advance screen:\nsofter, darker colours as games were designed.\nSkipped if core colour correction is already on.",
                choices: &[c("off", "Off"), c("on", "On")],
                default: 0,
                shown_when: Some((LOOK, "lcd")),
            },
            Setting {
                key: "frame",
                name: "Screen frame",
                desc: "Frames the borders around the game. Auto picks per console.\nAmbient glows with the game's colours; Glow adds a subtle edge.\nDMG to PSP: handheld bezels. CRT to Woodgrain: TVs. Arcade: cabinet.",
                choices: &[
                    c("none", "None"),
                    c("auto", "Auto"),
                    c("ambient", "Ambient"),
                    c("brick", "DMG"),
                    c("pocket", "GB Pocket"),
                    c("shell", "GBC"),
                    c("wide", "GBA"),
                    c("clamshell", "GBA SP"),
                    c("twin", "DS"),
                    c("gloss", "PSP"),
                    c("crt", "CRT"),
                    c("studio", "Studio monitor"),
                    c("portable-tv", "Portable TV"),
                    c("woodgrain", "Woodgrain"),
                    c("arcade", "Arcade"),
                    c("glow", "Glow"),
                ],
                default: 0,
                shown_when: None,
            },
            Setting {
                key: "frame_colour",
                name: "GBC colour",
                desc: "Shell colour for the Game Boy Color frame.\nClear purple, atomic purple and glacier blue are translucent.",
                choices: &[
                    c("grey", "Classic grey"),
                    c("indigo", "Indigo"),
                    c("clear-purple", "Clear purple"),
                    c("atomic-purple", "Atomic purple"),
                    c("black", "Black"),
                    c("white", "White"),
                    c("glacier", "Glacier blue"),
                ],
                default: 0,
                shown_when: Some(("frame", "shell")),
            },
            Setting {
                key: "frame_tv",
                name: "TV cabinet",
                desc: "CRT TV cabinet style: dark plastic or wood veneer.",
                choices: &[c("plastic", "Dark plastic"), c("wood", "Wood")],
                default: 0,
                shown_when: Some(("frame", "crt")),
            },
            Setting {
                key: "frame_brick",
                name: "DMG colour",
                desc: "Original Game Boy shell colour. Clear is translucent.\nIts power LED tracks battery: red at 20%, blinking at 5%.",
                choices: &[c("grey", "Grey"), c("clear", "Clear"), c("red", "Red"), c("yellow", "Yellow"), c("green", "Green"), c("black", "Black")],
                default: 0,
                shown_when: Some(("frame", "brick")),
            },
            Setting {
                key: "frame_pocket",
                name: "GB Pocket finish",
                desc: "Game Boy Pocket shell: brushed silver or clear plastic.\nIts power LED tracks battery: red at 20%, blinking at 5%.",
                choices: &[c("silver", "Brushed silver"), c("clear", "Clear")],
                default: 0,
                shown_when: Some(("frame", "pocket")),
            },
            Setting {
                key: "frame_clamshell",
                name: "GBA SP finish",
                desc: "Shell finish for the Game Boy Advance SP.\nIts hinge LED shows live battery status.",
                choices: &[c("platinum", "Platinum"), c("cobalt", "Cobalt"), c("flame", "Flame red"), c("graphite", "Graphite")],
                default: 0,
                shown_when: Some(("frame", "clamshell")),
            },
            // How the Pocket is held (`video::Orientation`): the picture turns on the panel, the Pocket's D-pad and
            // sticks with it.
            Setting {
                key: "orientation",
                name: "Screen orientation",
                desc: "TATE mode for vertical arcade games and shmups.\nTurned left: D-pad at bottom. Turned right: D-pad on top.\nD-pad turns too; menus stay upright. Frames: only Ambient and Glow.",
                choices: &[c("normal", "Normal"), c("left", "Turned left"), c("right", "Turned right")],
                default: 0,
                shown_when: None,
            },
            // N64, PSP, Dreamcast and PlayStation on the handhelds whose GPU was measured for it (`render_scale`): the same row as the
            // Resolution of PS2, GameCube, Wii and Xbox. Its default and the choices offered are the handheld's
            // (`console_default`, `offered_in`); the core reads it when the game loads.
            Setting {
                key: crate::render_scale::KEY,
                name: "Resolution",
                desc: "Internal 3D resolution. Original matches the real console; 2x and up look sharper but work the GPU harder. Applies on next launch.",
                choices: &[c("1", "Original"), c("2", "2x"), c("3", "3x"), c("4", "4x")],
                default: 0,
                shown_when: None,
            },
            // Super Nintendo on the handhelds measured for it (`mode7_hd`): Super Mario Kart's track, F-Zero's, Pilotwings'
            // ground drawn finer, and wider on a wide screen. On a card HD starts the game with bsnes-hd instead of Snes9x.
            Setting {
                key: crate::mode7_hd::KEY,
                name: "Mode 7",
                desc: crate::mode7_hd::DESC,
                choices: &[c("original", "Original"), c("hd", "HD"), c("wide", "HD widescreen")],
                default: 0,
                shown_when: None,
            },
            // The Dreamcast wherever pocket-ui's player plays it with Flycast (`dc_widescreen`): Flycast's widescreen
            // patches or its widescreen hack, never both; the core reports the 16:9 shape itself.
            Setting {
                key: crate::dc_widescreen::KEY,
                name: "Widescreen",
                desc: crate::dc_widescreen::DESC,
                choices: &[c("off", "Off"), c("patch", "Patch"), c("hack", "Hack")],
                default: 0,
                shown_when: None,
            },
        ],
    },
    Group {
        name: "Shortcuts",
        desc: "Hold HOME and press another button while you play.",
        settings: &[
            Setting {
                key: "hk_save",
                name: "Save state",
                desc: "HOME + R1 saves to the slot chosen in the menu.",
                choices: &[c("on", "HOME + R1"), c("off", "Off")],
                default: 0,
                shown_when: None,
            },
            Setting {
                key: "hk_load",
                name: "Load state",
                desc: "HOME + L1 loads the slot chosen in the menu.",
                choices: &[c("on", "HOME + L1"), c("off", "Off")],
                default: 0,
                shown_when: None,
            },
            Setting {
                key: "hk_slot",
                name: "Change slot",
                desc: "HOME + Left and HOME + Right pick the save slot, 1 to 8.",
                choices: &[c("on", "HOME + Left/Right"), c("off", "Off")],
                default: 0,
                shown_when: None,
            },
            Setting {
                key: "hk_ff",
                name: "Fast-forward",
                desc: "HOME + R2 turns fast-forward on and off.\nThe speed is set under Speed, rumble, fan.",
                choices: &[c("on", "HOME + R2"), c("off", "Off")],
                default: 0,
                shown_when: None,
            },
            Setting {
                key: "hk_rewind",
                name: "Rewind",
                desc: "Hold HOME + L2 to rewind.\nEnable Rewind under Speed, rumble, fan.",
                choices: &[c("on", "HOME + L2"), c("off", "Off")],
                default: 0,
                shown_when: None,
            },
            Setting {
                key: "hk_shot",
                name: "Screenshot",
                desc: "HOME + A saves a picture of the game to the SD card.\nThe pause menu does not open.",
                choices: &[c("on", "HOME + A"), c("off", "Off")],
                default: 0,
                shown_when: None,
            },
            // Was "Pixel Path" (key "hk_pixel") until 2026-10-25 (`over_game::migrate`).
            Setting {
                key: OVER_GAME_SHORTCUT_KEY,
                name: "Over the game",
                desc: "HOME + Y cycles through Off, FPS, FPS + sensors\nand Pixel Path, until you quit or change Over the game.",
                choices: &[c("on", "HOME + Y"), c("off", "Off")],
                default: 0,
                shown_when: None,
            },
            // pocket-ui's own (`app_zoom`): off unless chosen, as Zoom is a niche help.
            Setting {
                key: "hk_zoom",
                name: "Zoom",
                desc: "HOME + X shows the paused game's picture larger.\nB goes back to the game.",
                choices: &[c("on", "HOME + X"), c("off", "Off")],
                default: 1,
                shown_when: None,
            },
        ],
    },
    Group {
        name: "Speed, rumble, fan",
        // What is in it, not "Settings you rarely need." (review 19.11. #2); without a fan `EXTRAS_DESC_NO_FAN` (`group_desc`).
        desc: EXTRAS_DESC_FAN,
        settings: &[
            Setting {
                key: "ff_speed",
                name: "Fast-forward speed",
                desc: "Game speed while fast-forwarding.\nAudio is muted meanwhile.",
                choices: &[c("2", "2x"), c("3", "3x"), c("4", "4x"), c("8", "8x"), c("max", "As fast as possible")],
                default: 2,
                shown_when: None,
            },
            Setting {
                key: "rewind",
                name: "Rewind",
                desc: "Buffers the last minute so you can rewind.\nHold HOME + L2. Uses extra battery.",
                choices: &[c("off", "Off"), c("on", "On")],
                default: 0,
                shown_when: None,
            },
            // Earlier (`timeline`): On, but Off for the consoles with big states (`timeline::BIG_STATES`, `console_default`).
            Setting {
                key: crate::timeline_policy::KEY,
                name: "Earlier states",
                desc: "Auto-saves every 5 min (up to 30 min back) plus\nsession start, under Earlier in this menu.\nOff by default on N64, PSP, DS, Dreamcast & Saturn (large states).",
                choices: &[c("off", "Off"), c("on", "On")],
                default: 1,
                shown_when: None,
            },
            // Was Show FPS (key "show_fps", On for FPS) until 2026-10-25 (`over_game::migrate`); the choices are
            // `over_game::Level`'s. What a game starts with: Settings > Playing > Over the game (`load`).
            Setting {
                key: OVER_GAME_KEY,
                name: "Over the game",
                desc: "FPS shows frame rate and CPU load in the top-left corner.\nFPS + sensors adds temperature, fan speed and power draw.\nPixel Path adds a live strip: core, scale, FPS, latency\nand audio, lighting up orange if anything slips.",
                choices: &[c("off", "Off"), c("fps", "FPS"), c("sensors", "FPS + sensors"), c("pixel", "Pixel Path")],
                default: 0,
                shown_when: None,
            },
            Setting {
                key: "rumble",
                name: "Rumble",
                desc: "Rumble strength for games that support vibration.\nOff disables it.",
                choices: &[c("off", "Off"), c("low", "Low"), c("medium", "Medium"), c("high", "High")],
                default: 2,
                shown_when: None,
            },
            Setting {
                key: "performance",
                name: "Performance",
                desc: "Auto: the system decides how fast the CPU runs.\nBattery limits the CPU to half speed: enough for Game Boy and NES.\nMax keeps the CPU at full speed for demanding games.\nWhat games use unless you save one here: Quick settings, or Settings › Playing › Performance & fan.",
                // "auto" was "balanced" until 2026-10-23: `GameSettings::load` reads the old value as "auto".
                choices: &[c("battery", "Battery"), c("auto", "Auto"), c("performance", "Max")],
                default: 1,
                shown_when: None,
            },
            Setting {
                key: "fan",
                name: "Fan",
                desc: "Auto keeps the system's cooling profile.\nVery quiet: fan only when it gets warm. Quiet starts it later, Cool earlier.\nThe system's setting comes back when you quit.\nWhat games use unless you save one here: Quick settings, or Settings › Playing › Performance & fan.",
                choices: &[c("veryquiet", "Very quiet"), c("quiet", "Quiet"), c("auto", "Auto"), c("cool", "Cool")],
                // Quiet, as Games: fan starts (`perf_fan::games_fan_default`).
                default: 1,
                shown_when: None,
            },
            // Only for the light systems (`run_ahead::SYSTEMS`, see `applies`).
            Setting {
                key: "run_ahead",
                name: "Run-ahead",
                desc: "Cuts 1–2 frames of input lag by predicting ahead.\nWorks the CPU harder, so it uses a bit more battery.\nAuto-disables if a game can't keep full speed.\n1 frame fits most games; 2 for lighter consoles.",
                choices: &[c("off", "Off"), c("1", "1 frame"), c("2", "2 frames")],
                default: 0,
                shown_when: None,
            },
        ],
    },
];

/// The handheld consoles: Settings → Screen frames on Handhelds gives them frame Auto.
pub const HANDHELDS: &[&str] = &[
    "gb", "gbc", "gba", "gbh", "gbch", "gbah", "gbav", "gamegear", "ggh", "ngp", "ngpc", "wonderswan", "wonderswancolor", "atarilynx", "pokemini",
];

/// Starting values per console where the global default isn't the best look on this screen.
/// No console starts with a screen effect or a frame (a clean picture; Settings → Screen frames adds frames);
/// disc systems with changing resolutions are scaled to fit and kept crisp, the DS's two screens likewise. Names are ROCKNIX system names.
pub fn console_default(system: &str, key: &str) -> Option<&'static str> {
    // 3D systems render at changing, often non-integer resolutions too (and higher than native).
    const DISC: &[&str] = &["psx", "segacd", "megacd", "pcenginecd", "tg16cd", "saturn", "n64", "psp", "dreamcast", "naomi", "atomiswave"];
    // 320x200 adventures: whole-number scaling stops at 3x, 960 of the 1240 px, with black all around.
    const ADVENTURE: &[&str] = &["scummvm"];
    // Two screens stacked (256x384): as large as fits (720x1080, 2.8x), crisp, no scanlines or frame.
    const DUAL_SCREEN: &[&str] = &["nds"];
    match key {
        "scaling" if DUAL_SCREEN.contains(&system) => Some("aspect"),
        "sharpness" if DUAL_SCREEN.contains(&system) => Some("crisp"),
        "scaling" if DISC.contains(&system) || ADVENTURE.contains(&system) => Some("aspect"),
        "sharpness" if DISC.contains(&system) || ADVENTURE.contains(&system) => Some("crisp"),
        // The handheld's measured default (`render_scale`), only where the row is there.
        crate::render_scale::KEY => crate::render_scale::default_value(crate::render_scale::here(system)),
        // Earlier: their states are several to tens of MB (docs/HOST.md "Earlier").
        crate::timeline_policy::KEY if !crate::timeline_policy::default_on(system) => Some("off"),
        _ => None,
    }
}




/// The explanation for each value of a setting that has one of its own (`describe`); the others have the row's `desc`.
pub fn value_descs(key: &str) -> &'static [(&'static str, &'static str)] {
    match key {
        "sharpness" => &[
            ("sharp", "Razor-sharp raw pixels, exactly as rendered."),
            ("crisp", "Even pixels at any scale; matches Sharp at Integer scaling."),
            ("soft", "Softly smooths every pixel edge."),
        ],
        "effect" => &[
            ("none", "Clean image with no shader overlay."),
            ("grid", "Subtle pixel grid like a classic handheld LCD."),
            ("scanlines", "Beam-style scanlines: deep gaps in shadows that bloom in highlights."),
            ("crt", "Scanlines with a soft CRT glow. Pair with a Screen frame for the full TV."),
            ("softglow", "Soft glow around bright highlights; crisp elsewhere."),
            ("smooth", "Smooths jagged diagonal edges while keeping flat pixel art sharp."),
        ],
        _ => &[],
    }
}

/// Explanations a row shows besides `value_descs` and its `desc`: the Look's (`look::explain`).
pub fn extra_descs(key: &str) -> Vec<&'static str> {
    if key != LOOK {
        return Vec::new();
    }
    let mut all: Vec<&'static str> = look::ALL.iter().map(|l| look::explain(*l, false)).collect();
    all.push(look::explain(look::Look::LcdGrid, true));
    all
}



/// Whether the row is drawn a step in: Sharpness and Effect, which open under Custom (LCD colours, like a frame's colour,
/// stand at the row's own level).
pub fn indented(setting: &Setting) -> bool {
    setting.shown_when == Some((LOOK, "custom"))
}

/// The console family each console-shaped frame draws: the Game Boy family (DMG, GB Pocket, GBC), the
/// Game Boy Advance (GBA, GBA SP), the Nintendo DS (DS; its two stacked screens) and the PSP.
pub(crate) fn frame_systems(value: &str) -> Option<&'static [&'static str]> {
    match value {
        "brick" | "pocket" | "shell" => Some(&["gb", "gbh", "gbc", "gbch"]),
        "wide" | "clamshell" => Some(&["gba", "gbah", "gbav"]),
        "twin" => Some(&["nds"]),
        "gloss" => Some(&["psp"]),
        _ => None,
    }
}

/// What the Android player draws of the picture rows, row by row: since 18.11.2026 Linux's own picture pass and frames
/// (`video_program`, `VideoSettings::fixed_style`), so every Look but Custom (Sharpness and Effect one by one) and every frame
/// but Ambient (drawn live from the picture by pocket-core's renderer); the three scalings its touch layout knows. Android's
/// handheld view offers these and no others: every other Linux value reads as the one that draws the same (`android_reading`).
pub const ANDROID_VALUES: &[(&str, &[&str])] = &[
    ("scaling", &["native", "aspect", "fullscreen"]),
    (LOOK, &["clean", "smooth", "soft", "bloom", "lcd", "scanlines", "oldtv"]),
    ("frame", &["none", "auto", "brick", "pocket", "shell", "wide", "clamshell", "twin", "gloss", "crt", "studio", "portable-tv", "woodgrain", "arcade", "glow"]),
];

/// A saved value as Android's player draws it, in the values it offers (`ANDROID_VALUES`): what a Linux card or an older
/// version saved reads as the value that looks the same there.
pub fn android_reading<'a>(key: &str, value: &'a str) -> &'a str {
    match (key, value) {
        // The player has one Integer.
        ("scaling", "integer-smaller") => "native",
        // Ambient is pocket-core's fill around the picture, drawn live: the player draws none.
        ("frame", "ambient") => "none",
        _ => value,
    }
}

/// The Look Android's player draws for these values.
pub fn android_look(sharpness: &str, effect: &str) -> look::Look {
    // Every Look but Custom: a Custom pair (Sharpness and Effect one by one) reads as its effect's Look, drawn as saved.
    match (effect, sharpness) {
        ("scanlines", _) => look::Look::Scanlines,
        ("crt", _) => look::Look::OldTv,
        ("grid", _) => look::Look::LcdGrid,
        ("softglow", _) => look::Look::Bloom,
        ("smooth", _) => look::Look::Smooth,
        (_, "soft") => look::Look::Soft,
        _ => look::Look::Clean,
    }
}

/// The explanation of a picture row on Android: only the player's values (`pause_options::shown_rows` is the other side).
pub(crate) fn android_desc(key: &str) -> Option<&'static str> {
    match key {
        "scaling" => Some("Integer uses whole-number scaling for razor-sharp, even pixels.\nAspect scales as big as fits in the original ratio.\nFullscreen fills the display by stretching."),
        // Linux's lines without Ambient's, which the player doesn't draw (each line is Linux's, translated as it is).
        "frame" => Some("Frames the borders around the game. Auto picks per console.\nDMG to PSP: handheld bezels. CRT to Woodgrain: TVs. Arcade: cabinet."),
        // The app's bsnes has HD built in: no other emulator, no widescreen.
        crate::mode7_hd::KEY => Some(crate::mode7_hd::ANDROID_DESC),
        _ => None,
    }
}

/// The choice's name on this device: Linux's on every one (Android's "Console" frame was Linux's Auto until 18.11.2026).
pub fn label_in(setting: &Setting, value: &str, android: bool) -> &'static str {
    let _ = android;
    setting.choices.iter().find(|c| c.value == value).map_or("", |c| c.label)
}


/// [`offered`] with Android's profile given.
pub fn offered_in(setting: &Setting, value: &str, system: &str, android: bool) -> bool {
    if android && ANDROID_VALUES.iter().any(|(k, vs)| *k == setting.key && !vs.contains(&value)) {
        return false;
    }
    match setting.key {
        "frame" => frame_systems(value).is_none_or(|systems| systems.contains(&system)),
        // LCD grid for the handhelds, Scanlines and Old TV for the rest (`look::kind`).
        LOOK => look::Look::from_value(value).is_none_or(|l| look::offered(l, Some(system))),
        // Only the factors that run at full speed on this handheld.
        crate::render_scale::KEY => crate::render_scale::offers(crate::render_scale::here(system), value),
        // HD widescreen only on a wide screen (`mode7_hd::offer`).
        crate::mode7_hd::KEY => crate::mode7_hd::offers(crate::mode7_hd::here_for(system), value),
        _ => true,
    }
}

/// Values that were once choices and are read as another: the Swan, Watch and Goggles frames left the list
/// (Auto still draws them for WonderSwan, Game & Watch and Virtual Boy), so a file that has one means Auto.
pub(crate) fn current_value<'a>(key: &str, value: &'a str) -> &'a str {
    match (key, value) {
        ("frame", "swan" | "watch" | "goggles") => "auto",
        _ => value,
    }
}


/// [`applies`] with Android's profile given: its handheld view lists only what its player draws (`device::ANDROID_PLAYER_SETTINGS`).
pub fn applies_in(key: &str, system: &str, android: bool) -> bool {
    if game_setting_out_in(key, android) {
        return false;
    }
    match key {
        "run_ahead" => run_ahead_for(system),
        // Game Boy Color colours are the emulator's (gambatte corrects them out of the box).
        "lcd_colour" => system == "gba",
        // Earlier keeps save states: not for the systems that have none (ScummVM, PICO-8, TIC-80).
        crate::timeline_policy::KEY => has_states(system),
        // No fan (the H700 handhelds, the Brick): the row set a fan that isn't there (settings walk 06.11.).
        "fan" => has_fan(),
        // N64, PSP, Dreamcast and PlayStation on a handheld measured for it (`render_scale`); nowhere else, Android included (above).
        crate::render_scale::KEY => crate::render_scale::here(system).is_some(),
        // Super Nintendo on a handheld measured for it, with a core that has HD Mode 7 (`mode7_hd`); Android's page asks
        // `pause_options` (above).
        crate::mode7_hd::KEY => crate::mode7_hd::here_for(system).is_some(),
        // The Dreamcast where Flycast plays it (`dc_widescreen`); Android's page asks `pause_options` (above).
        crate::dc_widescreen::KEY => crate::dc_widescreen::applies(system),
        _ => true,
    }
}

pub fn setting(key: &str) -> Option<&'static Setting> {
    GROUPS.iter().flat_map(|g| g.settings.iter()).find(|s| s.key == key)
}

/// Speed, rumble, fan's line with a fan to set: what the group holds.
pub const EXTRAS_DESC_FAN: &str = "Fast-forward, rewind, earlier states, the frame rate, rumble, performance and the fan.";
/// The same without one (`device::no_fan`); its name drops the fan too (`i18n::FANLESS_NAMES`).
pub const EXTRAS_DESC_NO_FAN: &str = "Fast-forward, rewind, earlier states, the frame rate, rumble and performance.";
