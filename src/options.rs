//! In-game options, NextUI-style: a few groups of settings with fixed choices and a one-line
//! explanation each. Values are kept per console, optionally overridden per game, and read by
//! pocket-core when a game starts.
//!
//! Files: `<data>/game-settings/<system>.json` and `<data>/game-settings/<system>/<game>.json`,
//! each a flat JSON object of key → value. A game file replaces the console file entirely.
//!
//! A value nobody saved comes from, in this order: Settings > Playing > Performance & fan for Performance and Fan
//! (`perf_fan::game_defaults`, `<data>/performance.json`), the console's own default (`console_default`), the
//! setting's default here.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::look;

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

const fn c(value: &'static str, label: &'static str) -> Choice {
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
                desc: "Integer scales by whole numbers (integer scaling): every pixel the same size, the sharpest.\nInteger, smaller is one whole step down, leaving room for a frame.\nAspect is as large as fits, in the console's shape.\nFullscreen fills the screen and stretches pixels.",
                choices: &[c("native", "Integer"), c("integer-smaller", "Integer, smaller"), c("aspect", "Aspect"), c("fullscreen", "Fullscreen")],
                default: 0,
                shown_when: None,
            },
            // Not a saved value: the Look worked out from `sharpness` + `effect` (`look::derive`), whose choosing writes both
            // (`GameSettings::step`). Left out of `effective()` and never sent to pocket-core as "set look".
            Setting {
                key: LOOK,
                name: "Look",
                desc: "How the picture looks. Pick one; Custom opens Sharpness and Effect.",
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
                desc: "How the edges of the blown-up pixels look.",
                choices: &[c("sharp", "Sharp"), c("crisp", "Crisp"), c("soft", "Soft")],
                default: 0,
                shown_when: Some((LOOK, "custom")),
            },
            Setting {
                key: "effect",
                name: "Effect",
                desc: "What is laid over the picture.",
                choices: &[c("none", "None"), c("grid", "LCD grid"), c("scanlines", "Scanlines"), c("crt", "Old TV"), c("softglow", "Bloom"), c("smooth", "Smooth")],
                default: 0,
                shown_when: Some((LOOK, "custom")),
            },
            // Only for the Game Boy Advance (`applies`): its screen's colours, with the LCD grid (the Look, or Custom's Effect).
            Setting {
                key: "lcd_colour",
                name: "LCD colours",
                desc: "The colours of the Game Boy Advance's own screen:\nsofter and darker, as the games were made for.\nLeft out while the emulator's own colour correction is on.",
                choices: &[c("off", "Off"), c("on", "On")],
                default: 0,
                shown_when: Some((LOOK, "lcd")),
            },
            Setting {
                key: "frame",
                name: "Screen frame",
                desc: "Fills the space around the picture. Auto suits the console.\nAmbient glows with the game's colours; Glow is a thin light in its colour.\nDMG to PSP: handhelds, each for its console. CRT to Woodgrain: TVs. Arcade: a cabinet.",
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
                desc: "The colour of the Game Boy Color shell around the screen.\nClear purple, atomic purple and glacier blue are see-through.",
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
                desc: "The cabinet of the CRT frame: dark plastic or wood veneer.",
                choices: &[c("plastic", "Dark plastic"), c("wood", "Wood")],
                default: 0,
                shown_when: Some(("frame", "crt")),
            },
            Setting {
                key: "frame_brick",
                name: "DMG colour",
                desc: "The colour of the original Game Boy. Clear is see-through.\nIts power light shows the battery: red at 20%, blinking at 5%.",
                choices: &[c("grey", "Grey"), c("clear", "Clear"), c("red", "Red"), c("yellow", "Yellow"), c("green", "Green"), c("black", "Black")],
                default: 0,
                shown_when: Some(("frame", "brick")),
            },
            Setting {
                key: "frame_pocket",
                name: "GB Pocket finish",
                desc: "The Game Boy Pocket: brushed silver metal or clear plastic.\nIts power light shows the battery: red at 20%, blinking at 5%.",
                choices: &[c("silver", "Brushed silver"), c("clear", "Clear")],
                default: 0,
                shown_when: Some(("frame", "pocket")),
            },
            Setting {
                key: "frame_clamshell",
                name: "GBA SP finish",
                desc: "The paint of the opened Game Boy Advance SP.\nIts power light, in the hinge, shows the battery.",
                choices: &[c("platinum", "Platinum"), c("cobalt", "Cobalt"), c("flame", "Flame red"), c("graphite", "Graphite")],
                default: 0,
                shown_when: Some(("frame", "clamshell")),
            },
            // How the Pocket is held (`video::Orientation`): the picture turns on the panel, the Pocket's D-pad and
            // sticks with it.
            Setting {
                key: "orientation",
                name: "Screen orientation",
                desc: "For games made for an upright screen, such as vertical shooters.\nTurned left: D-pad at the bottom. Turned right: D-pad on top.\nThe D-pad turns too, menus don't. Frames: only Ambient and Glow.",
                choices: &[c("normal", "Normal"), c("left", "Turned left"), c("right", "Turned right")],
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
                desc: "Hold HOME + L2 to play backwards.\nRewind itself is switched on under Speed, rumble, fan.",
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
                key: crate::over_game::SHORTCUT_KEY,
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
        desc: "Settings you rarely need.",
        settings: &[
            Setting {
                key: "ff_speed",
                name: "Fast-forward speed",
                desc: "How fast the game runs while fast-forward is on.\nSound is muted meanwhile.",
                choices: &[c("2", "2x"), c("3", "3x"), c("4", "4x"), c("8", "8x"), c("max", "As fast as possible")],
                default: 2,
                shown_when: None,
            },
            Setting {
                key: "rewind",
                name: "Rewind",
                desc: "Remembers the last minute so you can play it backwards.\nHold HOME + L2. Uses some extra power.",
                choices: &[c("off", "Off"), c("on", "On")],
                default: 0,
                shown_when: None,
            },
            // Earlier (`timeline`): On, but Off for the consoles with big states (`timeline::BIG_STATES`, `console_default`).
            Setting {
                key: crate::timeline::KEY,
                name: "Earlier states",
                desc: "Keeps a state every 5 minutes of play, half an hour back,\nand where each session began: Earlier in this menu.\nOff by default for N64, PSP, DS, Dreamcast and Saturn (big states).",
                choices: &[c("off", "Off"), c("on", "On")],
                default: 1,
                shown_when: None,
            },
            // Was Show FPS (key "show_fps", On for FPS) until 2026-10-25 (`over_game::migrate`); the choices are
            // `over_game::Level`'s. What a game starts with: Settings > Playing > Over the game (`load`).
            Setting {
                key: crate::over_game::KEY,
                name: "Over the game",
                desc: "FPS shows frames per second and CPU load in the top left corner.\nFPS + sensors adds the temperature, the fan and the battery draw.\nPixel Path shows a line beside the picture: emulator, scale,\nframe rate, latency and sound; orange when something slips.",
                choices: &[c("off", "Off"), c("fps", "FPS"), c("sensors", "FPS + sensors"), c("pixel", "Pixel Path")],
                default: 0,
                shown_when: None,
            },
            Setting {
                key: "rumble",
                name: "Rumble",
                desc: "How strongly games shake the console, for games that rumble.\nOff keeps it still.",
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
                desc: "The game responds to your buttons 1 or 2 frames sooner.\nIt runs ahead and goes back every frame, so it uses more power.\nIt switches itself off where a game can't keep up.\n1 frame suits most games; 2 only the lighter ones.",
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
        // Earlier: their states are several to tens of MB (docs/HOST.md "Earlier").
        crate::timeline::KEY if !crate::timeline::default_on(system) => Some("off"),
        _ => None,
    }
}

impl Group {
    /// The settings shown while the values are `values`: a sub-choice only while the setting it
    /// belongs to has its value (the shell colour while the frame is Shell, Sharpness and Effect while the Look is Custom).
    pub fn visible(&self, values: Option<&GameSettings>) -> Vec<&Setting> {
        self.settings
            .iter()
            .filter(|s| values.is_none_or(|v| applies(s.key, &v.system)))
            .filter(|s| match s.shown_when {
                None => true,
                Some((key, value)) => {
                    let parent = setting(key).expect("shown_when names a setting");
                    let i = values.map_or(parent.default, |v| v.index(parent));
                    parent.choices[i].value == value || values.is_some_and(|v| lcd_colours_under_custom(s, v))
                }
            })
            .collect()
    }

    /// The most rows the group shows at once for these values' console: every setting that is always shown, plus
    /// the sub-choices of each setting that has them. A setting's sub-choices belong to different values of it, so only
    /// the largest set shows at a time: the frame's colour, or Sharpness and Effect (and on the GBA the LCD grid's colours)
    /// under Custom. The preview stays where it is while the Look or the frame changes.
    pub fn max_rows(&self, values: Option<&GameSettings>) -> usize {
        let applying: Vec<&Setting> = self.settings.iter().filter(|s| values.is_none_or(|v| applies(s.key, &v.system))).collect();
        let always = applying.iter().filter(|s| s.shown_when.is_none()).count();
        let mut under: BTreeMap<(&str, &str), usize> = BTreeMap::new();
        for s in &applying {
            if let Some(when) = s.shown_when {
                *under.entry(when).or_default() += 1;
            }
            // Custom's Effect = LCD grid shows them too, beside Sharpness and Effect.
            if s.key == "lcd_colour" {
                *under.entry((LOOK, "custom")).or_default() += 1;
            }
        }
        let mut largest: BTreeMap<&str, usize> = BTreeMap::new();
        for ((parent, _), n) in under {
            let m = largest.entry(parent).or_default();
            *m = (*m).max(n);
        }
        always + largest.values().sum::<usize>()
    }

    /// Every explanation the group can show, for the room the longest needs.
    pub fn descs(&self) -> Vec<&'static str> {
        self.settings.iter().flat_map(|s| std::iter::once(s.desc).chain(value_descs(s.key).iter().map(|(_, d)| *d)).chain(extra_descs(s.key))).collect()
    }
}

/// LCD colours also show under Look = Custom while Effect is the LCD grid (the GBA's), not only under the LCD grid Look.
fn lcd_colours_under_custom(s: &Setting, v: &GameSettings) -> bool {
    s.key == "lcd_colour" && v.look() == look::Look::Custom && v.value_of("effect") == "grid"
}

/// The explanation for each value of a setting that has one of its own (`describe`); the others have the row's `desc`.
fn value_descs(key: &str) -> &'static [(&'static str, &'static str)] {
    match key {
        "sharpness" => &[
            ("sharp", "Hard pixel squares, exactly as the console drew them."),
            ("crisp", "All pixels the same size at any scaling; at Integer it looks like Sharp."),
            ("soft", "Blurs every pixel edge a little."),
        ],
        "effect" => &[
            ("none", "The plain picture, nothing laid over it."),
            ("grid", "Thin dark gaps between the pixels, like a handheld's screen."),
            ("scanlines", "Rows like a TV's beam: dark gaps in dark parts, rows that close up where it's bright."),
            ("crt", "Scanlines plus a faint glow, like an old tube TV. The TV around it is a Screen frame."),
            ("softglow", "A soft halo around bright parts; the rest stays clear."),
            ("smooth", "Rounds the stair-steps on slanted edges. Flat pixels and straight edges stay sharp."),
        ],
        _ => &[],
    }
}

/// Explanations a row shows besides `value_descs` and its `desc`: the Look's (`look::explain`).
fn extra_descs(key: &str) -> Vec<&'static str> {
    if key != LOOK {
        return Vec::new();
    }
    let mut all: Vec<&'static str> = look::ALL.iter().map(|l| look::explain(*l, false)).collect();
    all.push(look::explain(look::Look::LcdGrid, true));
    all
}

/// What the row says about its current value: the Look's and the Custom rows' value explanation, else the row's own `desc`.
pub fn describe(setting: &Setting, values: Option<&GameSettings>) -> String {
    if setting.key == LOOK {
        let look = values.map_or(look::Look::Clean, |v| v.look());
        let gba_colours = values.is_some_and(|v| v.value_of("lcd_colour") == "on" && applies("lcd_colour", &v.system));
        return look::explain(look, gba_colours).to_string();
    }
    let table = value_descs(setting.key);
    let current = values.map_or(setting.choices[setting.default].value, |v| setting.choices[v.index(setting)].value);
    table.iter().find(|(value, _)| *value == current).map_or(setting.desc, |(_, d)| d).to_string()
}

/// Whether the row is drawn a step in: Sharpness and Effect, which open under Custom (LCD colours, like a frame's colour,
/// stand at the row's own level).
pub fn indented(setting: &Setting) -> bool {
    setting.shown_when == Some((LOOK, "custom"))
}

/// The console family each console-shaped frame draws: the Game Boy family (DMG, GB Pocket, GBC), the
/// Game Boy Advance (GBA, GBA SP), the Nintendo DS (DS; its two stacked screens) and the PSP.
fn frame_systems(value: &str) -> Option<&'static [&'static str]> {
    match value {
        "brick" | "pocket" | "shell" => Some(&["gb", "gbh", "gbc", "gbch"]),
        "wide" | "clamshell" => Some(&["gba", "gbah", "gbav"]),
        "twin" => Some(&["nds"]),
        "gloss" => Some(&["psp"]),
        _ => None,
    }
}

/// Whether `value` of `setting` is offered for `system`: a console's own frame only for that console.
/// A saved value is kept either way.
pub fn offered(setting: &Setting, value: &str, system: &str) -> bool {
    match setting.key {
        "frame" => frame_systems(value).is_none_or(|systems| systems.contains(&system)),
        // LCD grid for the handhelds, Scanlines and Old TV for the rest (`look::kind`).
        LOOK => look::Look::from_value(value).is_none_or(|l| look::offered(l, Some(system))),
        _ => true,
    }
}

/// Values that were once choices and are read as another: the Swan, Watch and Goggles frames left the list
/// (Auto still draws them for WonderSwan, Game & Watch and Virtual Boy), so a file that has one means Auto.
fn current_value<'a>(key: &str, value: &'a str) -> &'a str {
    match (key, value) {
        ("frame", "swan" | "watch" | "goggles") => "auto",
        _ => value,
    }
}

/// Whether a setting means anything for `system`: run-ahead only for the light systems, Earlier only with save states.
pub fn applies(key: &str, system: &str) -> bool {
    match key {
        "run_ahead" => crate::run_ahead::available_for(system),
        // Game Boy Color colours are the emulator's (gambatte corrects them out of the box).
        "lcd_colour" => system == "gba",
        // Earlier keeps save states: not for the systems that have none (ScummVM, PICO-8, TIC-80).
        crate::timeline::KEY => crate::hostsys::find(system).is_none_or(|h| h.no_states.is_none()),
        // No fan (the H700 handhelds, the Brick): the row set a fan that isn't there (settings walk 06.11.).
        "fan" => crate::device::current().has_fan(),
        _ => true,
    }
}

pub fn setting(key: &str) -> Option<&'static Setting> {
    GROUPS.iter().flat_map(|g| g.settings.iter()).find(|s| s.key == key)
}

/// Where the values in use came from.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Level {
    Defaults,
    Console,
    Game,
}

pub struct GameSettings {
    dir: PathBuf,
    system: String,
    game: String,
    pub values: BTreeMap<String, String>,
    pub level: Level,
    /// Settings > Playing > Performance & fan and Settings > Screen frames: what games start with when neither file has a
    /// value (`perf_fan`, `settings::frames`).
    globals: BTreeMap<String, String>,
    /// The Look row was turned to Custom (or the saved values fit no Look) while this page is open: Sharpness and Effect stay
    /// open, even if their values come to match a Look. Not saved and not sent to pocket-core; `close_custom` forgets it.
    custom_open: bool,
    /// Over the game offers its Pixel Path level (`over_game::pixel_path_offered`, read at `load`).
    pixel_path: bool,
}

impl GameSettings {
    pub fn load(data_dir: &Path, system: &str, rom: &Path) -> Self {
        let dir = data_dir.join("game-settings");
        let game = rom.file_stem().and_then(|s| s.to_str()).unwrap_or("game").to_string();
        let read = |p: PathBuf| crate::fsutil::read_json_or_backup::<BTreeMap<String, String>>(&p);
        let (values, level) = match (read(dir.join(system).join(format!("{game}.json"))), read(dir.join(format!("{system}.json")))) {
            (Some(v), _) => (v, Level::Game),
            (None, Some(v)) => (v, Level::Console),
            (None, None) => (BTreeMap::new(), Level::Defaults),
        };
        let mut values = values;
        // Auto was called Balanced until 2026-10-23: an old file's choice stays the choice it was.
        if let Some(v) = values.get_mut("performance").filter(|v| v.as_str() == "balanced") {
            *v = crate::power::Performance::Auto.value().to_string();
        }
        // Show FPS and the Pixel Path shortcut became Over the game on 2026-10-25: an old choice is the closest level.
        crate::over_game::migrate(&mut values);
        // "Aspect screen" was dropped on 2026-10-24 (it always matched Fullscreen): an old choice shows as Fullscreen.
        if values.get("scaling").map(String::as_str) == Some("aspect-screen") {
            values.insert("scaling".into(), "fullscreen".into());
        }
        let mut globals = crate::perf_fan::game_defaults(data_dir);
        // Settings → Screen frames: Auto below the console's and the game's own choice, on every console or only on
        // the handhelds; Off (the start setting) leaves every console without a frame, and no console starts with
        // a screen effect either (the owner, 2026-10-23: a clean picture out of the box).
        match crate::settings::frames(data_dir) {
            crate::settings::Frames::Every => {
                globals.insert("frame".into(), "auto".into());
            }
            crate::settings::Frames::Handhelds if HANDHELDS.contains(&system) => {
                globals.insert("frame".into(), "auto".into());
            }
            _ => {}
        }
        // Settings → Playing → Over the game: what a game draws over itself unless it or its console has a choice.
        let over = crate::settings::over_game(data_dir);
        if over != crate::over_game::Level::Off {
            globals.insert(crate::over_game::KEY.into(), over.value().into());
        }
        // Its Pixel Path level only with the pause menu's Pixel Path (Enthusiast): otherwise a saved one is FPS, here
        // and in the game (`over_game::Level::offered`), and the Options row walks past it (`step`).
        let pixel_path = crate::over_game::pixel_path_offered(data_dir);
        for v in [values.get_mut(crate::over_game::KEY), globals.get_mut(crate::over_game::KEY)].into_iter().flatten() {
            *v = crate::over_game::Level::parse(v).offered(pixel_path).value().to_string();
        }
        GameSettings { dir, system: system.to_string(), game, values, level, globals, custom_open: false, pixel_path }
    }

    /// Index of the current choice for `setting`: saved value (this game's, else this console's), else the default
    /// of Settings > Playing > Performance & fan, else the console's default, else the setting's own.
    pub fn index(&self, setting: &Setting) -> usize {
        if setting.key == LOOK {
            let value = self.look().value();
            return setting.choices.iter().position(|c| c.value == value).unwrap_or(setting.default);
        }
        let find = |v: &str| setting.choices.iter().position(|c| c.value == v);
        self.values
            .get(setting.key)
            .and_then(|v| find(current_value(setting.key, v)))
            .or_else(|| self.globals.get(setting.key).and_then(|v| find(current_value(setting.key, v))))
            .or_else(|| console_default(&self.system, setting.key).and_then(find))
            .unwrap_or(setting.default)
    }

    /// Current value of every setting, defaults filled in.
    pub fn effective(&self) -> BTreeMap<String, String> {
        GROUPS
            .iter()
            .flat_map(|g| g.settings.iter())
            .filter(|s| s.key != LOOK)
            .map(|s| (s.key.to_string(), s.choices[self.index(s)].value.to_string()))
            .collect()
    }

    /// The value in use of a setting by key ("" for an unknown key).
    pub fn value_of(&self, key: &str) -> &'static str {
        setting(key).map_or("", |s| s.choices[self.index(s)].value)
    }

    /// The Look shown for the values in use: Custom while it is open, else worked out from Sharpness and Effect.
    pub fn look(&self) -> look::Look {
        if self.custom_open {
            look::Look::Custom
        } else {
            look::derive(self.value_of("sharpness"), self.value_of("effect"), Some(&self.system))
        }
    }

    /// The page is opened again (or the settings were reset): the Look is worked out from the values anew.
    pub fn close_custom(&mut self) {
        self.custom_open = false;
    }

    /// Moves `setting` one choice left (-1) or right (+1), wrapping around, past choices that aren't
    /// offered for this console (`offered`). Returns the (key, value) pairs that changed, to send to the running game:
    /// one for a setting, the two or three behind a Look (`look::Look::canonical`, and `lcd_colour` on for the GBA's LCD
    /// grid), none for Custom, which only opens Sharpness and Effect and changes no picture.
    pub fn step(&mut self, setting: &Setting, delta: i32) -> Vec<(&'static str, String)> {
        let n = setting.choices.len() as i32;
        let mut i = self.index(setting) as i32;
        for _ in 0..n {
            i = (i + delta).rem_euclid(n);
            let value = setting.choices[i as usize].value;
            if offered(setting, value, &self.system) && (setting.key != crate::over_game::KEY || self.pixel_path || value != "pixel") {
                break;
            }
        }
        let value = setting.choices[i as usize].value;
        if setting.key == LOOK {
            let Some((sharpness, effect)) = look::Look::from_value(value).and_then(look::Look::canonical) else {
                self.custom_open = true;
                return Vec::new();
            };
            self.custom_open = false;
            let mut changed = vec![("sharpness", sharpness.to_string()), ("effect", effect.to_string())];
            // Choosing the GBA's LCD grid takes its screen's colours too; they can be turned off after, the Look stays.
            if effect == "grid" && applies("lcd_colour", &self.system) {
                changed.push(("lcd_colour", "on".to_string()));
            }
            for (k, v) in &changed {
                self.values.insert((*k).to_string(), v.clone());
            }
            return changed;
        }
        self.values.insert(setting.key.to_string(), value.to_string());
        vec![(setting.key, value.to_string())]
    }

    fn write(path: &Path, values: &BTreeMap<String, String>) -> bool {
        crate::fsutil::write_json(path, values).is_ok()
    }

    fn game_file(&self) -> PathBuf {
        self.dir.join(&self.system).join(format!("{}.json", self.game))
    }

    /// Makes the current values the console's settings; drops this game's own file.
    pub fn save_for_console(&mut self) -> bool {
        crate::fsutil::remove_json(&self.game_file());
        let ok = Self::write(&self.dir.join(format!("{}.json", self.system)), &self.values);
        if ok {
            self.level = Level::Console;
        }
        ok
    }

    /// Keeps the values in use where `level` says (the console's file for `Console`, else this game's), after a change in
    /// the pause menu: every change stays at once, as in Settings. The write goes to the card writer, so the menu does
    /// not wait for the card.
    pub fn keep(&mut self) {
        if self.level == Level::Console {
            crate::fsutil::write_json_soon(&self.dir.join(format!("{}.json", self.system)), &self.values);
        } else {
            crate::fsutil::write_json_soon(&self.game_file(), &self.values);
            self.level = Level::Game;
        }
    }

    /// "Keep for" moved to the other side: from this game to the whole console or back. The values in use are written
    /// there now. Returns the new level, None when the card refused.
    pub fn keep_elsewhere(&mut self) -> Option<Level> {
        let ok = if self.level == Level::Console { self.save_for_game() } else { self.save_for_console() };
        ok.then_some(self.level)
    }

    pub fn save_for_game(&mut self) -> bool {
        let ok = Self::write(&self.game_file(), &self.values);
        if ok {
            self.level = Level::Game;
        }
        ok
    }

    /// This game's or its console's own value of `key` (`level` says whose), None when it comes from Settings >
    /// Performance & fan or the defaults (Quick settings say so, `quick_profiles`).
    pub fn own(&self, key: &str) -> Option<&str> {
        self.values.get(key).map(String::as_str)
    }

    /// The value of Settings > Playing > Performance & fan below the console's and the game's own, changed while this was
    /// loaded (Quick settings over the paused game).
    pub fn set_global(&mut self, key: &str, value: &str) {
        self.globals.insert(key.to_string(), value.to_string());
    }

    /// Takes this game off its own settings: its file goes and the console's (or the defaults) are read again. For the
    /// "This game" page's "Use the console's settings".
    pub fn forget_game(&mut self, data_dir: &Path, rom: &Path) {
        crate::fsutil::remove_json(&self.game_file());
        let fresh = GameSettings::load(data_dir, &self.system, rom);
        self.values = fresh.values;
        self.level = fresh.level;
        self.custom_open = false;
    }

    /// Back to the built-in defaults: removes both files for this game's console and the game.
    pub fn restore_defaults(&mut self) {
        crate::fsutil::remove_json(&self.game_file());
        crate::fsutil::remove_json(&self.dir.join(format!("{}.json", self.system)));
        self.values.clear();
        self.level = Level::Defaults;
        self.custom_open = false;
    }
}

/// The Options of the running game between two visits to the pause menu. A change applies to the game at once
/// (`set <key> <value>` to pocket-core) and is kept at once (`keep`: for this game, or for the console). So the menu, when
/// it opens again, starts from what the game uses now (the files hold the same): the row shows the changed value and
/// Left/Right step on from it. A new start of the game begins with a new `Session`, from the saved values.
#[derive(Default)]
pub struct Session {
    /// The values the menu left the last time it closed; None before the first visit.
    kept: Option<BTreeMap<String, String>>,
}

impl Session {
    /// The Options as the pause menu opens them: the saved files (and Settings' defaults below them, read again),
    /// then this session's values, saved or not.
    pub fn open(&self, data_dir: &Path, system: &str, rom: &Path) -> GameSettings {
        let mut settings = GameSettings::load(data_dir, system, rom);
        // The level stays the files': it says where the changes are kept.
        if let Some(kept) = &self.kept {
            settings.values = kept.clone();
        }
        settings
    }

    /// The pause menu closes over the running game: its values are what the game goes on with.
    pub fn keep(&mut self, settings: &GameSettings) {
        self.kept = Some(settings.values.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;

    #[test]
    fn an_old_balanced_performance_reads_as_auto_and_a_global_can_change_below_it() {
        let dir = TestDir::new("balanced");
        std::fs::create_dir_all(dir.join("game-settings/gba")).unwrap();
        std::fs::write(dir.join("game-settings/gba/Old.json"), br#"{"performance": "balanced"}"#).unwrap();
        let perf = setting("performance").unwrap();
        let mut s = GameSettings::load(&dir, "gba", Path::new("/r/gba/Old.gba"));
        assert_eq!(perf.choices[s.index(perf)].label, "Auto", "Balanced, as Auto was called, stays the game's choice");
        assert_eq!(s.own("performance"), Some("auto"));
        assert_eq!(s.effective()["performance"], "auto");
        // Quick settings change the default under it: the game's own choice still wins.
        s.set_global("performance", "performance");
        assert_eq!(s.effective()["performance"], "auto");
        let mut other = GameSettings::load(&dir, "gba", Path::new("/r/gba/New.gba"));
        assert_eq!(other.own("performance"), None);
        other.set_global("performance", "battery");
        assert_eq!(other.effective()["performance"], "battery");
        assert_eq!(perf.choices.iter().map(|c| c.label).collect::<Vec<_>>(), ["Battery", "Auto", "Max"]);
    }

    #[test]
    fn pending_changes_cannot_recreate_an_override_after_keep_for_or_reset() {
        use std::sync::mpsc;
        use std::time::Duration;
        for reset in [false, true] {
            let dir = TestDir::new(if reset { "queued-reset" } else { "queued-console" });
            let rom = Path::new("/r/gba/Keep.gba");
            let mut settings = GameSettings::load(&dir, "gba", rom);
            // A slow card keeps the actual settings write queued until after the
            // player asks to change its scope or restore the defaults.
            let (started_tx, started_rx) = mpsc::channel();
            let (release_tx, release_rx) = mpsc::channel();
            crate::fsutil::write_json_checked_soon(&dir.join("writer.json"), &0, move |_| {
                started_tx.send(()).unwrap();
                let _ = release_rx.recv_timeout(crate::testutil::timeout(Duration::from_secs(10)));
            }).unwrap();
            started_rx.recv_timeout(crate::testutil::timeout(Duration::from_secs(10))).unwrap();
            settings.values.insert("rumble".into(), "off".into());
            settings.keep();
            let (done_tx, done_rx) = mpsc::channel();
            let (thread_tx, thread_rx) = mpsc::channel();
            let change = std::thread::spawn(move || {
                thread_tx.send(std::thread::current().id()).unwrap();
                if reset {
                    settings.restore_defaults();
                } else {
                    assert_eq!(settings.keep_elsewhere(), Some(Level::Console));
                }
                let _ = done_tx.send(());
            });
            let thread = thread_rx.recv_timeout(crate::testutil::timeout(Duration::from_secs(10))).unwrap();
            crate::testutil::clock::wait_for(|| {
                match done_rx.try_recv() {
                    Ok(()) => panic!("the scope change skipped the pending-write barrier"),
                    Err(mpsc::TryRecvError::Disconnected) => panic!("scope-change worker failed"),
                    Err(mpsc::TryRecvError::Empty) => {}
                }
                crate::fsutil::waiting_for_writes(thread)
            }, Duration::from_secs(10), "scope change reached the blocked card writer");
            release_tx.send(()).unwrap();
            change.join().unwrap();
            crate::fsutil::settle();
            let loaded = GameSettings::load(&dir, "gba", rom);
            assert_eq!(loaded.level, if reset { Level::Defaults } else { Level::Console },
                "an older queued change must not bring back this game's override");
            assert_eq!(loaded.own("rumble"), if reset { None } else { Some("off") });
        }
    }

    #[test]
    fn a_change_is_kept_at_once_for_the_game_and_keep_for_moves_it_to_the_console_and_back() {
        let dir = TestDir::new("keep-for");
        let rom = Path::new("/r/gba/Keep.gba");
        let mut s = GameSettings::load(&dir, "gba", rom);
        assert_eq!(s.level, Level::Defaults);
        s.values.insert("rumble".into(), "off".into());
        s.keep();
        crate::fsutil::settle();
        assert_eq!(s.level, Level::Game, "the first change is kept for this game");
        assert_eq!(GameSettings::load(&dir, "gba", rom).own("rumble"), Some("off"));
        assert_eq!(GameSettings::load(&dir, "gba", Path::new("/r/gba/Other.gba")).own("rumble"), None, "not for the console");
        assert_eq!(s.keep_elsewhere(), Some(Level::Console));
        assert_eq!(GameSettings::load(&dir, "gba", Path::new("/r/gba/Other.gba")).own("rumble"), Some("off"), "now every game of the console");
        s.values.insert("rumble".into(), "on".into());
        s.keep();
        crate::fsutil::settle();
        assert_eq!(GameSettings::load(&dir, "gba", Path::new("/r/gba/Other.gba")).own("rumble"), Some("on"), "kept where Keep for says");
        assert_eq!(s.keep_elsewhere(), Some(Level::Game));
        assert_eq!(GameSettings::load(&dir, "gba", rom).level, Level::Game);
    }

    #[test]
    fn screen_frames_on_every_console_is_auto_below_the_consoles_and_the_games_own_choice() {
        let dir = TestDir::new("frames-every");
        let frame = |sys: &str, rom: &str| GameSettings::load(&dir, sys, Path::new(rom)).effective()["frame"].clone();
        assert_eq!((frame("snes", "a.sfc").as_str(), frame("gba", "a.gba").as_str()), ("none", "none"), "Off, the start setting");
        crate::settings::Settings { frames: crate::settings::Frames::Handhelds, ..Default::default() }.save(&dir);
        assert_eq!((frame("snes", "a.sfc").as_str(), frame("gba", "a.gba").as_str(), frame("gbh", "a.gb").as_str()), ("none", "auto", "auto"), "Handhelds");
        crate::settings::Settings { frames: crate::settings::Frames::Every, ..Default::default() }.save(&dir);
        assert_eq!((frame("snes", "a.sfc").as_str(), frame("nds", "a.nds").as_str(), frame("gba", "a.gba").as_str()), ("auto", "auto", "auto"));
        // A frame chosen for the console or the game still wins.
        let mut s = GameSettings::load(&dir, "snes", Path::new("b.sfc"));
        s.values.insert("frame".into(), "crt".into());
        assert!(s.save_for_game());
        assert_eq!((frame("snes", "b.sfc").as_str(), frame("snes", "a.sfc").as_str()), ("crt", "auto"));
    }

    #[test]
    fn over_the_game_starts_from_settings_and_a_console_or_game_choice_wins() {
        use crate::over_game::Level;
        let dir = TestDir::new("over-game-default");
        let over = |rom: &str| GameSettings::load(&dir, "gba", Path::new(rom)).effective()["over_game"].clone();
        assert_eq!(over("a.gba"), "off", "a new device: nothing over the game");
        crate::settings::Settings { over_game: Level::Sensors, ..Default::default() }.save(&dir);
        assert_eq!(over("a.gba"), "sensors");
        let mut s = GameSettings::load(&dir, "gba", Path::new("b.gba"));
        s.values.insert("over_game".into(), "fps".into());
        assert!(s.save_for_game());
        assert_eq!((over("b.gba").as_str(), over("a.gba").as_str()), ("fps", "sensors"), "a game's own choice wins");
        // Pixel Path (an Enthusiast's, with the pause menu's Pixel Path): drawn as saved; for everyone else the FPS line.
        crate::settings::Settings { over_game: Level::PixelPath, ..Default::default() }.save(&dir);
        assert_eq!(over("a.gba"), "fps", "Pixel Path without the pause menu's Pixel Path: FPS");
        std::fs::write(dir.join("curator.json"), br#"{"pixel_path": true}"#).unwrap();
        assert_eq!(over("a.gba"), "pixel");
    }

    #[test]
    fn show_fps_saved_for_a_console_or_a_game_reads_as_over_the_game() {
        let dir = TestDir::new("over-game-migrate");
        std::fs::create_dir_all(dir.join("game-settings/snes")).unwrap();
        std::fs::write(dir.join("game-settings/gba.json"), br#"{"show_fps": "on", "hk_pixel": "off", "rewind": "on"}"#).unwrap();
        std::fs::write(dir.join("game-settings/snes/Zelda.json"), br#"{"show_fps": "sensors"}"#).unwrap();
        std::fs::write(dir.join("game-settings/nes.json"), br#"{"show_fps": "pixel"}"#).unwrap();
        let load = |sys: &str, rom: &str| GameSettings::load(&dir, sys, Path::new(rom));
        let gba = load("gba", "x.gba");
        let v = gba.effective();
        assert_eq!((v["over_game"].as_str(), v["hk_over"].as_str(), v["rewind"].as_str()), ("fps", "off", "on"), "On is FPS; the shortcut stays off");
        assert!(!v.contains_key("show_fps") && !v.contains_key("hk_pixel"));
        assert_eq!(load("snes", "Zelda.sfc").effective()["over_game"], "sensors");
        assert_eq!(load("nes", "x.nes").effective()["over_game"], "fps", "Pixel Path is the FPS line without the pause menu's Pixel Path (0.4)");
        // Saved again, the file has the new keys only.
        let mut gba = gba;
        assert!(gba.save_for_console());
        let text = std::fs::read_to_string(dir.join("game-settings/gba.json")).unwrap();
        assert!(text.contains("\"over_game\": \"fps\"") && !text.contains("show_fps") && !text.contains("hk_pixel"), "{text}");
    }

    #[test]
    fn scummvm_fills_the_screen_instead_of_stepping_by_whole_numbers() {
        // A 320x200 adventure at Native stops at 3x (960 of 1240 px wide); smoothed to fit it fills the screen.
        let dir = TestDir::new("defaults-svm");
        let s = GameSettings::load(&dir, "scummvm", Path::new("x")).effective();
        assert_eq!((s["scaling"].as_str(), s["sharpness"].as_str(), s["effect"].as_str()), ("aspect", "crisp", "none"));
    }

    #[test]
    fn consoles_start_with_their_own_look() {
        let dir = TestDir::new("defaults");
        let effect = |sys: &str| GameSettings::load(&dir, sys, Path::new("x.rom")).effective()["effect"].clone();
        // A clean picture out of the box (Alex, 2026-10-23): no console starts with a screen effect.
        for sys in ["gbc", "gba", "snes", "megadrive-japan", "sg-1000", "atari2600", "neogeo", "ggh", "pokemini", "atarilynx", "n64"] {
            assert_eq!(effect(sys), "none", "{sys}");
        }
        for sys in ["segacd", "pcenginecd", "tg16cd", "psx", "n64", "psp", "dreamcast"] {
            let s = GameSettings::load(&dir, sys, Path::new("x.cue")).effective();
            assert_eq!((s["scaling"].as_str(), s["sharpness"].as_str()), ("aspect", "crisp"), "{sys}");
        }
        let frame = |sys: &str| GameSettings::load(&dir, sys, Path::new("x.rom")).effective()["frame"].clone();
        assert_eq!((frame("gba").as_str(), frame("gb").as_str(), frame("snes").as_str()), ("none", "none", "none"), "no frames out of the box");
        assert_eq!(GameSettings::load(&dir, "psx", Path::new("x.cue")).effective()["scaling"], "aspect");
        let nds = GameSettings::load(&dir, "nds", Path::new("x.nds")).effective();
        assert_eq!((nds["scaling"].as_str(), nds["sharpness"].as_str(), nds["effect"].as_str(), nds["frame"].as_str()), ("aspect", "crisp", "none", "none"));
    }

    #[test]
    fn frames_offer_every_style_and_keep_their_defaults() {
        let frame = setting("frame").unwrap();
        let values: Vec<&str> = frame.choices.iter().map(|c| c.value).collect();
        assert_eq!(
            values,
            ["none", "auto", "ambient", "brick", "pocket", "shell", "wide", "clamshell", "twin", "gloss", "crt", "studio", "portable-tv", "woodgrain", "arcade", "glow"]
        );
        // Every value means a frame to pocket-core, and every finish is one its frame knows.
        for v in &values {
            assert!(*v == "none" || crate::video::FrameMode::parse(v) != crate::video::FrameMode::None, "{v}");
        }
        use crate::bezel::{BrickColour, ClamFinish, SlimFinish};
        let finishes = |key: &str| setting(key).unwrap().choices.iter().map(|c| c.value).collect::<Vec<_>>();
        assert_eq!(finishes("frame_brick"), BrickColour::ALL.iter().map(|f| f.value()).collect::<Vec<_>>());
        assert_eq!(finishes("frame_pocket"), SlimFinish::ALL.iter().map(|f| f.value()).collect::<Vec<_>>());
        assert_eq!(finishes("frame_clamshell"), ClamFinish::ALL.iter().map(|f| f.value()).collect::<Vec<_>>());
        assert!(setting("frame_swan").is_none() && setting("frame_watch").is_none(), "Swan and Watch have no finish rows");
        let scaling: Vec<&str> = setting("scaling").unwrap().choices.iter().map(|c| c.value).collect();
        assert_eq!(scaling, ["native", "integer-smaller", "aspect", "fullscreen"]);
        let colours: Vec<&str> = setting("frame_colour").unwrap().choices.iter().map(|c| c.value).collect();
        assert_eq!(colours, ["grey", "indigo", "clear-purple", "atomic-purple", "black", "white", "glacier"]);
        // Out of the box no console has a frame (Settings → Screen frames: Off).
        let dir = TestDir::new("framedefaults");
        for (sys, want) in [("gba", "none"), ("gbc", "none"), ("gb", "none"), ("snes", "none"), ("nes", "none"), ("psx", "none")] {
            let v = GameSettings::load(&dir, sys, Path::new("x.rom")).effective();
            assert_eq!(v["frame"], want, "{sys}");
            assert_ne!(v["scaling"], "integer-smaller", "{sys}");
            assert_eq!((v["frame_colour"].as_str(), v["frame_tv"].as_str()), ("grey", "plastic"));
        }
    }

    #[test]
    fn sub_choices_show_only_under_their_frame() {
        let dir = TestDir::new("subchoice");
        let picture = GROUPS.iter().find(|g| g.name == "Picture").unwrap();
        let keys = |s: &GameSettings| picture.visible(Some(s)).iter().map(|s| s.key).collect::<Vec<_>>();
        let mut s = GameSettings::load(&dir, "gba", Path::new("x.gba"));
        assert_eq!(keys(&s), ["scaling", "look", "frame", "orientation"]);
        s.values.insert("frame".into(), "shell".into());
        assert_eq!(keys(&s), ["scaling", "look", "frame", "frame_colour", "orientation"]);
        s.values.insert("frame".into(), "crt".into());
        assert_eq!(keys(&s), ["scaling", "look", "frame", "frame_tv", "orientation"]);
        for (frame, sub) in [("brick", "frame_brick"), ("pocket", "frame_pocket"), ("clamshell", "frame_clamshell")] {
            s.values.insert("frame".into(), frame.into());
            assert_eq!(keys(&s), ["scaling", "look", "frame", sub, "orientation"], "{frame}");
        }
        for frame in ["wide", "twin", "gloss", "studio", "portable-tv", "woodgrain"] {
            s.values.insert("frame".into(), frame.into());
            assert_eq!(keys(&s).len(), 4, "{frame} has no sub-choice");
        }
        assert_eq!(picture.visible(None).len(), 4, "defaults: Look Clean, frame None, no sub-choice");
        let snes = GameSettings::load(&dir, "snes", Path::new("x.sfc"));
        // Always: scaling, look, frame, orientation; the frame's colour; Sharpness and Effect under Custom.
        assert_eq!(picture.max_rows(Some(&snes)), 7, "a frame's colour, Custom's two rows; LCD colours are the GBA's alone");
        assert_eq!(picture.max_rows(Some(&s)), 8, "the GBA: and the LCD grid's colours beside Custom's rows");
        s.values.insert("effect".into(), "grid".into());
        assert_eq!(keys(&s), ["scaling", "look", "lcd_colour", "frame", "orientation"], "LCD colours under the LCD grid look");
        assert_eq!(picture.visible(Some(&snes)).len(), 4);
        let extras = GROUPS.iter().find(|g| g.name == "Speed, rumble, fan").unwrap();
        assert_eq!(extras.max_rows(None), extras.settings.len());
    }

    fn look_of(s: &GameSettings) -> &'static str {
        s.value_of(LOOK)
    }

    fn visible_keys(s: &GameSettings) -> Vec<&'static str> {
        GROUPS.iter().find(|g| g.name == "Picture").unwrap().visible(Some(s)).iter().map(|s| s.key).collect()
    }

    /// Every saved pair of sharpness and effect, on a handheld and a TV console: the Look shown, the rows open, and the values
    /// the picture is drawn with (what `VideoSettings` reads) untouched by showing it.
    #[test]
    fn every_saved_pair_shows_a_look_or_custom_and_no_picture_value_changes() {
        let dir = TestDir::new("look-derive");
        std::fs::create_dir_all(dir.join("game-settings")).unwrap();
        let want = |sys: &str, s: &str, e: &str| -> &'static str {
            match (s, e) {
                (_, "none") if s != "soft" => "clean",
                ("soft", "none") => "soft",
                ("soft", _) => "custom",
                (_, "smooth") => "smooth",
                (_, "softglow") => "bloom",
                (_, "grid") if crate::look::kind(sys) == crate::look::Kind::Handheld => "lcd",
                (_, "scanlines") if crate::look::kind(sys) == crate::look::Kind::Tv => "scanlines",
                (_, "crt") if crate::look::kind(sys) == crate::look::Kind::Tv => "oldtv",
                _ => "custom",
            }
        };
        for sys in ["gb", "gba", "nds", "snes", "psx"] {
            for sharpness in ["sharp", "crisp", "soft"] {
                for effect in ["none", "grid", "scanlines", "crt", "softglow", "smooth"] {
                    std::fs::write(dir.join(format!("game-settings/{sys}.json")), format!(r#"{{"sharpness": "{sharpness}", "effect": "{effect}"}}"#)).unwrap();
                    let mut s = GameSettings::load(&dir, sys, Path::new("x.rom"));
                    let before = s.effective();
                    assert_eq!(look_of(&s), want(sys, sharpness, effect), "{sys} {sharpness}+{effect}");
                    let keys = visible_keys(&s);
                    assert_eq!(keys.contains(&"sharpness") && keys.contains(&"effect"), look_of(&s) == "custom", "{sys} {sharpness}+{effect}: Custom opens the rows");
                    // Only a look shows; nothing is written or changed by reading, painting or opening the page.
                    s.close_custom();
                    assert_eq!(s.effective(), before, "{sys} {sharpness}+{effect}");
                    assert_eq!((before["sharpness"].as_str(), before["effect"].as_str()), (sharpness, effect));
                    assert!(!before.contains_key(LOOK), "look is no saved value");
                }
            }
        }
    }

    #[test]
    fn console_starts_show_clean_and_restore_defaults_is_not_crisp() {
        let dir = TestDir::new("look-starts");
        for sys in ["gb", "gba", "snes", "nes", "psx", "n64", "nds", "scummvm"] {
            let s = GameSettings::load(&dir, sys, Path::new("x.rom"));
            assert_eq!(look_of(&s), "clean", "{sys} starts on Clean, never Custom");
            assert!(!visible_keys(&s).contains(&"sharpness"), "{sys}");
        }
        let mut s = GameSettings::load(&dir, "gb", Path::new("x.gb"));
        assert_eq!(s.effective()["sharpness"], "sharp");
        s.step(setting(LOOK).unwrap(), 1);
        s.step(setting(LOOK).unwrap(), -1);
        assert_eq!(s.effective()["sharpness"], "crisp", "choosing Clean writes its canonical form");
        s.restore_defaults();
        assert_eq!((s.effective()["sharpness"].as_str(), s.effective()["effect"].as_str()), ("sharp", "none"), "Restore defaults keeps the console's own start");
        assert_eq!(look_of(&s), "clean");
    }

    #[test]
    fn choosing_a_look_writes_the_two_keys_and_custom_writes_nothing() {
        let dir = TestDir::new("look-step");
        let look = setting(LOOK).unwrap();
        let mut s = GameSettings::load(&dir, "snes", Path::new("x.sfc"));
        // Right from Clean: Smooth, Soft, Bloom, (LCD grid is a handheld's, left out), Scanlines, Old TV, Custom.
        let mut walk = Vec::new();
        for _ in 0..7 {
            let changed = s.step(look, 1);
            walk.push((look_of(&s), changed.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join(" ")));
        }
        assert_eq!(
            walk,
            [
                ("smooth", "sharpness=crisp effect=smooth".to_string()),
                ("soft", "sharpness=soft effect=none".into()),
                ("bloom", "sharpness=crisp effect=softglow".into()),
                ("scanlines", "sharpness=crisp effect=scanlines".into()),
                ("oldtv", "sharpness=crisp effect=crt".into()),
                ("custom", String::new()),
                ("clean", "sharpness=crisp effect=none".into()),
            ]
        );
        assert_eq!(s.values["effect"], "none");
        // Custom: nothing is written, the picture stays as it is, the rows open.
        let mut s = GameSettings::load(&dir, "snes", Path::new("y.sfc"));
        s.values.insert("sharpness".into(), "crisp".into());
        s.values.insert("effect".into(), "crt".into());
        let before = s.values.clone();
        assert_eq!(look_of(&s), "oldtv");
        assert!(s.step(look, 1).is_empty(), "Old TV → Custom");
        assert_eq!((look_of(&s), &s.values), ("custom", &before), "writes nothing, the old look's values stay");
        assert_eq!(visible_keys(&s), ["scaling", "look", "sharpness", "effect", "frame", "orientation"]);
        // Edited to match a look, it stays Custom while the page is open: no row folds away under the cursor.
        let (sharpness, effect) = (setting("sharpness").unwrap(), setting("effect").unwrap());
        s.step(effect, -1);
        s.step(effect, -1);
        s.step(effect, -1);
        assert_eq!(s.values["effect"], "none");
        s.step(sharpness, 1);
        s.step(sharpness, -1);
        assert_eq!((look_of(&s), s.values["sharpness"].as_str()), ("custom", "crisp"));
        assert!(visible_keys(&s).contains(&"effect"));
        // Opened again, the values decide.
        s.close_custom();
        assert_eq!(look_of(&s), "clean");
    }

    #[test]
    fn the_lcd_grid_look_takes_the_gba_colours_and_keeps_its_name_without_them() {
        let dir = TestDir::new("look-lcd");
        let look = setting(LOOK).unwrap();
        let mut gba = GameSettings::load(&dir, "gba", Path::new("x.gba"));
        // Clean → Smooth → Soft → Bloom → LCD grid.
        let changed: Vec<_> = (0..4).flat_map(|_| gba.step(look, 1)).collect();
        assert_eq!(gba.value_of(LOOK), "lcd");
        assert_eq!(changed.iter().rev().take(3).map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>(), ["lcd_colour=on", "effect=grid", "sharpness=sharp"]);
        assert_eq!(visible_keys(&gba), ["scaling", "look", "lcd_colour", "frame", "orientation"]);
        gba.values.insert("lcd_colour".into(), "off".into());
        assert_eq!(gba.value_of(LOOK), "lcd", "turning the colours off leaves the Look");
        assert_eq!(describe(look, Some(&gba)), "Thin lines between the pixels, like a handheld's screen.");
        gba.values.insert("lcd_colour".into(), "on".into());
        assert!(describe(look, Some(&gba)).ends_with("in the GBA screen's colours."));
        // Other consoles have no colours row; a gb has the grid but no lcd_colour.
        let mut gb = GameSettings::load(&dir, "gb", Path::new("x.gb"));
        let changed: Vec<_> = (0..4).flat_map(|_| gb.step(look, 1)).collect();
        assert_eq!(gb.value_of(LOOK), "lcd");
        assert!(changed.iter().all(|(k, _)| *k != "lcd_colour"));
        assert!(!visible_keys(&gb).contains(&"lcd_colour"));
        // Custom with the grid shows the row too, beside Sharpness and Effect.
        let mut custom = GameSettings::load(&dir, "gba", Path::new("c.gba"));
        custom.values.insert("effect".into(), "grid".into());
        custom.values.insert("sharpness".into(), "soft".into());
        assert_eq!(visible_keys(&custom), ["scaling", "look", "sharpness", "effect", "lcd_colour", "frame", "orientation"]);
    }

    /// A game's file replaces the console's whole (no key-by-key fallback): the Look follows the file that is used.
    #[test]
    fn the_look_follows_the_file_in_use_game_over_console_over_defaults() {
        let dir = TestDir::new("look-files");
        std::fs::create_dir_all(dir.join("game-settings/gb")).unwrap();
        std::fs::write(dir.join("game-settings/gb.json"), br#"{"effect": "grid", "sharpness": "crisp"}"#).unwrap();
        std::fs::write(dir.join("game-settings/gb/Tetris.json"), br#"{"sharpness": "soft"}"#).unwrap();
        std::fs::write(dir.join("game-settings/gb/Empty.json"), b"{}").unwrap();
        let look = |rom: &str| look_of(&GameSettings::load(&dir, "gb", Path::new(rom)));
        assert_eq!(look("Other.gb"), "lcd", "the console's file");
        assert_eq!(look("Tetris.gb"), "soft", "the game's file replaces it: soft + the default effect");
        assert_eq!(look("Empty.gb"), "clean", "an empty game file is still the game's");
        assert_eq!(look_of(&GameSettings::load(&TestDir::new("look-none"), "psx", Path::new("a.cue"))), "clean");
    }

    #[test]
    fn the_custom_rows_describe_their_value_and_the_looks_are_all_described() {
        let dir = TestDir::new("look-desc");
        let mut s = GameSettings::load(&dir, "snes", Path::new("x.sfc"));
        let (sharpness, effect) = (setting("sharpness").unwrap(), setting("effect").unwrap());
        s.values.insert("sharpness".into(), "crisp".into());
        assert_eq!(describe(sharpness, Some(&s)), "All pixels the same size at any scaling; at Integer it looks like Sharp.");
        for _ in 0..4 {
            s.step(effect, 1);
        }
        assert_eq!(s.value_of("effect"), "softglow");
        assert_eq!(describe(effect, Some(&s)), "A soft halo around bright parts; the rest stays clear.");
        for sh in sharpness.choices {
            assert!(value_descs("sharpness").iter().any(|(v, _)| *v == sh.value), "{}", sh.value);
        }
        for e in effect.choices {
            assert!(value_descs("effect").iter().any(|(v, _)| *v == e.value), "{}", e.value);
        }
        // Names of the Custom rows, and the labels the player reads (the shader's words are the saved strings).
        let labels: Vec<_> = effect.choices.iter().map(|c| c.label).collect();
        assert_eq!(labels, ["None", "LCD grid", "Scanlines", "Old TV", "Bloom", "Smooth"]);
        assert_eq!(setting(LOOK).unwrap().choices.iter().map(|c| c.value).collect::<Vec<_>>(), look::ALL.iter().map(|l| l.value()).collect::<Vec<_>>());
        assert!(indented(sharpness) && indented(effect) && !indented(setting(LOOK).unwrap()) && !indented(setting("frame_colour").unwrap()));
        let picture = GROUPS.iter().find(|g| g.name == "Picture").unwrap();
        assert!(picture.descs().len() > 15, "every explanation counts for the room under the list");
    }

    /// Over the game's shortcut is there for everyone, its Pixel Path level since 0.4 only with the pause menu's Pixel
    /// Path (`curator.json`'s `pixel_path`, the Enthusiast profile's): without it the Options row walks Off, FPS and
    /// FPS + sensors.
    #[test]
    fn over_the_game_offers_pixel_path_only_with_the_pause_menus_pixel_path() {
        let dir = TestDir::new("over-game-levels");
        let over = setting("over_game").unwrap();
        let shortcuts = GROUPS.iter().find(|g| g.name == "Shortcuts").unwrap();
        let walk = |s: &mut GameSettings| -> Vec<String> {
            (0..4)
                .map(|_| {
                    s.step(over, 1);
                    s.values["over_game"].clone()
                })
                .collect()
        };
        let mut s = GameSettings::load(&dir, "gba", Path::new("x.gba"));
        assert_eq!(walk(&mut s), ["fps", "sensors", "off", "fps"]);
        s.step(over, -1);
        assert_eq!(s.values["over_game"], "off", "left goes back");
        s.step(over, -1);
        assert_eq!(s.values["over_game"], "sensors", "and past Pixel Path");
        std::fs::write(dir.join("curator.json"), br#"{"pixel_path": true}"#).unwrap();
        let mut s = GameSettings::load(&dir, "gba", Path::new("x.gba"));
        assert_eq!(walk(&mut s), ["fps", "sensors", "pixel", "off"]);
        s.step(over, -1);
        assert_eq!(s.values["over_game"], "pixel", "left goes back");
        std::fs::remove_file(dir.join("curator.json")).unwrap();
        let loaded = GameSettings::load(&dir, "gba", Path::new("x.gba"));
        assert!(shortcuts.visible(Some(&loaded)).iter().any(|s| s.key == "hk_over"));
        assert_eq!(loaded.effective()["over_game"], "off", "starts off");
        // The Options' choices are the levels, with their names.
        let levels: Vec<(&str, &str)> = crate::over_game::Level::ALL.iter().map(|l| (l.value(), l.label())).collect();
        assert_eq!(over.choices.iter().map(|c| (c.value, c.label)).collect::<Vec<_>>(), levels);
    }

    #[test]
    fn console_frames_are_offered_for_their_console_only() {
        let dir = TestDir::new("twin");
        let frame = setting("frame").unwrap();
        // Walking right through the list from None shows only what the console may have.
        let walk = |system: &str| {
            let mut s = GameSettings::load(&dir, system, Path::new("x.rom"));
            let mut seen = vec![];
            loop {
                s.step(frame, 1);
                if s.values["frame"] == "none" {
                    break;
                }
                seen.push(s.values["frame"].clone());
            }
            seen.retain(|v| ["brick", "pocket", "shell", "wide", "clamshell", "twin", "gloss"].contains(&v.as_str()));
            seen
        };
        assert_eq!(walk("nds"), ["twin"]);
        assert_eq!(walk("gba"), ["wide", "clamshell"]);
        assert_eq!(walk("gb"), ["brick", "pocket", "shell"]);
        assert_eq!(walk("gbc"), ["brick", "pocket", "shell"]);
        assert_eq!(walk("psp"), ["gloss"]);
        assert!(walk("snes").is_empty() && walk("psx").is_empty());
        // Swan, Watch and Goggles left the list: a saved one reads as Auto, in the menu and for pocket-core.
        let mut s = GameSettings::load(&dir, "wonderswan", Path::new("x.ws"));
        for old in ["swan", "watch", "goggles"] {
            s.values.insert("frame".into(), old.into());
            assert_eq!(frame.choices[s.index(frame)].value, "auto", "{old}");
            assert_eq!(s.effective()["frame"], "auto", "{old}");
            assert!(crate::video::VideoSettings::from_values(&s.effective()).fixed_style("wonderswan").is_none());
        }
        // Old and hand-made settings files keep working: a saved Twin elsewhere stays as it is.
        let mut s = GameSettings::load(&dir, "gba", Path::new("x.gba"));
        s.values.insert("frame".into(), "twin".into());
        assert_eq!(frame.choices[s.index(frame)].value, "twin");
        let old: BTreeMap<String, String> = [("frame", "shell"), ("frame_colour", "indigo")].iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        let v = crate::video::VideoSettings::from_values(&old);
        assert_eq!(v.fixed_style("gb"), Some(crate::bezel::Style::Shell(crate::bezel::Colourway::Indigo)), "an explicit Shell stays Shell");
    }

    #[test]
    fn run_ahead_is_offered_for_light_systems_only_and_starts_off() {
        let dir = TestDir::new("runahead-rows");
        let extras = GROUPS.iter().find(|g| g.name == "Speed, rumble, fan").unwrap();
        let has = |sys: &str| extras.visible(Some(&GameSettings::load(&dir, sys, Path::new("x.rom")))).iter().any(|s| s.key == "run_ahead");
        for sys in ["nes", "snes", "gb", "gbc", "gba", "megadrive", "mastersystem", "gamegear", "pcengine"] {
            assert!(has(sys), "{sys}");
            assert_eq!(GameSettings::load(&dir, sys, Path::new("x.rom")).effective()["run_ahead"], "off", "{sys}");
        }
        for sys in ["n64", "psp", "psx", "nds", "dreamcast", "pc", "scummvm"] {
            assert!(!has(sys), "{sys}");
        }
        assert_eq!(extras.visible(None).len(), extras.settings.len(), "without settings every row");
        let values: Vec<&str> = setting("run_ahead").unwrap().choices.iter().map(|c| c.value).collect();
        assert_eq!(values, ["off", "1", "2"]);
    }

    /// Per game over per console over Settings > Playing > Performance & fan (which has no run-ahead) over the default.
    #[test]
    fn run_ahead_follows_game_then_console_then_default() {
        let dir = TestDir::new("runahead-levels");
        let (rom, other) = (Path::new("/roms/snes/A.smc"), Path::new("/roms/snes/B.smc"));
        let ra = setting("run_ahead").unwrap();
        let value = |rom: &Path| GameSettings::load(&dir, "snes", rom).effective()["run_ahead"].clone();
        // The global page's file (performance.json) is there and sets other things: run-ahead stays at its default.
        let mut prefs = crate::perf_fan::Prefs::load(&dir);
        prefs.games_performance = crate::power::Performance::Performance;
        assert!(prefs.save(&dir));
        assert_eq!(value(rom), "off");
        let mut s = GameSettings::load(&dir, "snes", rom);
        s.step(ra, 1);
        assert!(s.save_for_console());
        assert_eq!((value(rom).as_str(), value(other).as_str()), ("1", "1"), "the console's value for every game");
        let mut s = GameSettings::load(&dir, "snes", rom);
        s.step(ra, 1);
        assert!(s.save_for_game());
        assert_eq!((value(rom).as_str(), value(other).as_str()), ("2", "1"), "this game's own");
        assert_eq!(GameSettings::load(&dir, "snes", rom).effective()["performance"], "performance", "the global page still reaches the game");
        GameSettings::load(&dir, "snes", rom).restore_defaults();
        assert_eq!(value(rom), "off");
    }

    #[test]
    fn screen_orientation_is_saved_per_game_or_per_console_and_starts_normal() {
        let dir = TestDir::new("orientation");
        let o = setting("orientation").unwrap();
        let values = |c: &[Choice]| c.iter().map(|c| c.value).collect::<Vec<_>>();
        assert_eq!(values(o.choices), ["normal", "left", "right"]);
        let (shmup, other) = (Path::new("/roms/wonderswan/Judgement Silversword.ws"), Path::new("/roms/wonderswan/Other.ws"));
        let mut s = GameSettings::load(&dir, "wonderswan", shmup);
        assert_eq!(s.effective()["orientation"], "normal", "every game starts upright");
        s.step(o, 1);
        assert!(s.save_for_game());
        let read = |rom: &Path| crate::video::VideoSettings::load(&dir, "wonderswan", rom).orientation;
        assert_eq!(read(shmup), crate::video::Orientation::Left, "Turned left kept for this game");
        assert_eq!(read(other), crate::video::Orientation::Normal, "and for it alone");
        let mut c = GameSettings::load(&dir, "wonderswan", other);
        c.step(o, -1);
        assert!(c.save_for_console());
        assert_eq!(read(other), crate::video::Orientation::Right, "Turned right for the console");
        assert_eq!(read(shmup), crate::video::Orientation::Left, "the game's own file still wins");
    }

    #[test]
    fn game_overrides_console_and_restore_clears_both() {
        let dir = TestDir::new("opts");
        let rom = Path::new("/roms/gbc/Game.gbc");
        let scaling = setting("scaling").unwrap();

        let mut s = GameSettings::load(&dir, "gbc", rom);
        assert_eq!(s.level, Level::Defaults);
        assert_eq!(s.index(scaling), 0);
        s.step(scaling, 1);
        assert!(s.save_for_console());

        let mut s = GameSettings::load(&dir, "gbc", rom);
        assert_eq!((s.level, s.index(scaling)), (Level::Console, 1));
        s.step(scaling, -2);
        assert!(s.save_for_game());
        assert_eq!(GameSettings::load(&dir, "gbc", rom).index(scaling), 3, "wraps and the game file wins");

        s.restore_defaults();
        assert_eq!(GameSettings::load(&dir, "gbc", rom).level, Level::Defaults);
    }

    /// Found on the Mini V2 (2026-10-25): Extras → Over the game and Picture → Screen orientation changed, the page
    /// left, the game kept the change, but the page opened again showed the saved value and stepped on from it.
    #[test]
    fn a_change_not_saved_is_what_the_pages_show_when_the_menu_opens_again() {
        let dir = TestDir::new("opts-session");
        let rom = Path::new("/roms/wonderswan/Judgement Silversword.ws");
        let label = |s: &GameSettings, key: &str| {
            let set = setting(key).unwrap();
            set.choices[s.index(set)].label
        };
        let mut session = Session::default();
        let mut menu = session.open(&dir, "wonderswan", rom);
        assert_eq!((label(&menu, "orientation"), label(&menu, "over_game")), ("Normal", "Off"));
        menu.step(setting("orientation").unwrap(), 1);
        menu.step(setting("over_game").unwrap(), 1);
        session.keep(&menu);
        // Back to the game, then the menu again.
        let mut menu = session.open(&dir, "wonderswan", rom);
        assert_eq!((label(&menu, "orientation"), label(&menu, "over_game")), ("Turned left", "FPS"), "what the game uses now");
        assert_eq!(menu.level, Level::Defaults, "nothing was saved");
        menu.step(setting("orientation").unwrap(), 1);
        menu.step(setting("over_game").unwrap(), -1);
        assert_eq!((label(&menu, "orientation"), label(&menu, "over_game")), ("Turned right", "Off"), "stepped on from there");
        session.keep(&menu);
        // Not saved: the next start of the game begins from the files.
        let start = Session::default().open(&dir, "wonderswan", rom);
        assert_eq!((label(&start, "orientation"), label(&start, "over_game")), ("Normal", "Off"));
        assert!(!dir.join("game-settings").exists(), "no file written");
        // Saved, changed again and left: the change shows, the level is the saved one.
        let mut menu = session.open(&dir, "wonderswan", rom);
        assert_eq!(label(&menu, "orientation"), "Turned right");
        assert!(menu.save_for_game());
        menu.step(setting("effect").unwrap(), 1);
        session.keep(&menu);
        let menu = session.open(&dir, "wonderswan", rom);
        assert_eq!((menu.level, label(&menu, "orientation"), label(&menu, "effect")), (Level::Game, "Turned right", "LCD grid"));
        assert_eq!(label(&Session::default().open(&dir, "wonderswan", rom), "effect"), "None", "the effect was not saved");
    }

    #[test]
    fn every_row_of_every_page_keeps_its_change_between_visits_to_the_menu() {
        let dir = TestDir::new("opts-session-all");
        // The GBA has every row (its LCD colours, run-ahead, Earlier).
        let rom = Path::new("/roms/gba/Game.gba");
        for group in GROUPS {
            for set in group.settings.iter().filter(|s| applies(s.key, "gba")) {
                for delta in [1, -1] {
                    let mut session = Session::default();
                    let mut menu = session.open(&dir, "gba", rom);
                    let before = menu.index(set);
                    menu.step(set, delta);
                    let changed = menu.index(set);
                    assert_ne!(before, changed, "{} steps", set.key);
                    // Custom only opens Sharpness and Effect for this visit; the Look is worked out again when the page opens.
                    if menu.look() == look::Look::Custom && set.key == LOOK {
                        continue;
                    }
                    session.keep(&menu);
                    let mut again = session.open(&dir, "gba", rom);
                    assert_eq!(again.index(set), changed, "{} > {} shows the change when it opens again", group.name, set.name);
                    again.step(set, delta);
                    menu.step(set, delta);
                    assert_eq!(again.index(set), menu.index(set), "{} steps on from the change", set.key);
                }
            }
        }
    }

    #[test]
    fn an_old_aspect_screen_choice_shows_as_fullscreen() {
        let dir = TestDir::new("opts-aspect");
        std::fs::create_dir_all(dir.join("game-settings")).unwrap();
        std::fs::write(dir.join("game-settings/gba.json"), br#"{"scaling": "aspect-screen"}"#).unwrap();
        let scaling = setting("scaling").unwrap();
        let s = GameSettings::load(&dir, "gba", Path::new("/roms/gba/Game.gba"));
        assert_eq!(scaling.choices[s.index(scaling)].value, "fullscreen");
    }
}
