//! Which emulator plays a game, and the RetroArch settings of the games RetroArch plays.
//!
//! ROCKNIX keeps the player's choice where EmulationStation (ES) puts it, in `system.cfg`
//! (`/storage/.config/system/configs/system.cfg`):
//!
//! * per system `<system>.emulator=<emulator>` and `<system>.core=<core>`,
//! * per game `<system>["<file name>"].emulator` / `.core` (ES `FileData::getConfigurationName`: the file name with
//!   `=` and `#` taken out),
//!
//! and lists what a system can run with in es_systems.cfg: `<emulators><emulator name="retroarch"><cores><core
//! default="true">mgba</core>…`. ES resolves the pair (ES `SystemData::getEmulator/getCore`,
//! `FileData::getEmulator/getCore`) and starts `runemu.sh <rom> -P<system> --core=<core> --emulator=<emulator>`;
//! runemu.sh itself never reads the keys. pocket-ui does the same resolving here (`resolve`), with its own host on
//! top: `pocket-core` is one more emulator for the systems it hosts, written as `<system>.emulator=pocket-core`
//! (ES ignores a name its list doesn't have and falls back to its default, so the two never disagree about a
//! RetroArch or standalone choice).
//!
//! The RetroArch settings (`RA_SETTINGS`) are the keys ROCKNIX's setsettings.sh reads at every RetroArch start through
//! `get_setting <key> <system> <file>`: the game's key, the system's, then `system.<key>`, `<key>` and `global.<key>`.
//!
//! Everything here is plain data and text, so every path has a test; the App reads the files on threads
//! (`preload`, `app_emu_choice.rs`) and writes with `write_system_cfg`.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex, RwLock};

/// ROCKNIX's settings file, read by ES, runemu.sh and setsettings.sh.
pub const SYSTEM_CFG: &str = "/storage/.config/system/configs/system.cfg";
/// The copy ROCKNIX ships: what "Restore defaults" goes back to (`set_setting` starts from it, too).
pub const SHIPPED_CFG: &str = "/usr/config/system/configs/system.cfg";
/// The emulator name pocket-ui's own host has in system.cfg.
pub const POCKET_CORE: &str = "pocket-core";

// ---- what a system can run with ---------------------------------------------------------------------------------

/// One `<emulator>` of a system in es_systems.cfg, with its cores in order and the one marked `default="true"`.
#[derive(Clone, Debug, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub struct EsEmulator {
    pub name: String,
    pub cores: Vec<String>,
    pub default_core: Option<usize>,
}

/// An emulator and its core, as runemu.sh takes them (`--emulator=`, `--core=`); `pocket-core` and the libretro core
/// for pocket-ui's own host.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Default)]
pub struct Runner {
    pub emulator: String,
    pub core: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    PocketCore,
    RetroArch,
    Standalone,
}

impl Runner {
    pub fn new(emulator: &str, core: &str) -> Runner {
        Runner {
            emulator: emulator.to_string(),
            core: core.to_string(),
        }
    }

    pub fn kind(&self) -> Kind {
        match self.emulator.as_str() {
            POCKET_CORE => Kind::PocketCore,
            "retroarch" => Kind::RetroArch,
            _ => Kind::Standalone,
        }
    }

    pub fn is_pocket_core(&self) -> bool {
        self.kind() == Kind::PocketCore
    }
}

/// ES `SystemData::getDefaultEmulator` and `getDefaultCore`: the emulator with a core marked default (else the first
/// emulator), and that core (else its first).
pub fn es_default(emulators: &[EsEmulator]) -> Option<Runner> {
    let emu = emulators.iter().find(|e| e.default_core.is_some()).or_else(|| emulators.first())?;
    Some(Runner::new(&emu.name, default_core(emu)))
}

fn default_core(emu: &EsEmulator) -> &str {
    emu.default_core
        .and_then(|i| emu.cores.get(i))
        .or_else(|| emu.cores.first())
        .map_or("", String::as_str)
}

fn emulator<'a>(emulators: &'a [EsEmulator], name: &str) -> Option<&'a EsEmulator> {
    emulators.iter().find(|e| e.name == name)
}

/// Every emulator/core pair of es_systems.cfg, in its order.
pub fn es_pairs(emulators: &[EsEmulator]) -> Vec<Runner> {
    let mut out = Vec::new();
    for e in emulators {
        if e.cores.is_empty() {
            out.push(Runner::new(&e.name, ""));
        }
        for c in &e.cores {
            out.push(Runner::new(&e.name, c));
        }
    }
    out
}

/// The `<emulators>` of one `<system>` node of es_systems.cfg.
pub fn parse_emulators(system: roxmltree::Node) -> Vec<EsEmulator> {
    system
        .descendants()
        .filter(|n| n.has_tag_name("emulator"))
        .filter_map(|e| {
            let name = e.attribute("name")?.trim().to_string();
            let mut emu = EsEmulator { name, ..Default::default() };
            for c in e.descendants().filter(|n| n.has_tag_name("core")) {
                let Some(core) = c.text().map(str::trim).filter(|t| !t.is_empty()) else {
                    continue;
                };
                if c.attribute("default") == Some("true") && emu.default_core.is_none() {
                    emu.default_core = Some(emu.cores.len());
                }
                emu.cores.push(core.to_string());
            }
            Some(emu)
        })
        .collect()
}

// ---- system.cfg ------------------------------------------------------------------------------------------------

/// system.cfg as `key → value`, the way ES's `SystemConf::loadSystemConf` reads it: `key=value` lines, the first `=`
/// splits, comments (`#`, `;`) and empty values are skipped, a key given twice keeps its last value.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Cfg {
    map: HashMap<String, String>,
}

impl Cfg {
    pub fn parse(text: &str) -> Cfg {
        let mut map = HashMap::new();
        for line in text.lines() {
            if line.starts_with('#') || line.starts_with(';') {
                continue;
            }
            let Some((k, v)) = line.split_once('=') else { continue };
            if !k.is_empty() && !v.is_empty() {
                map.insert(k.to_string(), v.to_string());
            }
        }
        Cfg { map }
    }

    /// Only the emulator and core keys: what a launch needs (`preload`).
    pub fn choices_only(text: &str) -> Cfg {
        let mut c = Cfg::parse(text);
        c.map.retain(|k, _| k.ends_with(".emulator") || k.ends_with(".core"));
        c
    }

    pub fn read(path: &Path) -> Cfg {
        Cfg::parse(&std::fs::read_to_string(path).unwrap_or_default())
    }

    /// The value of `key`, or None when unset, empty or "auto" (ES writes no line for AUTO, and treats a stray
    /// "auto" as unset).
    pub fn get(&self, key: &str) -> Option<&str> {
        self.map.get(key).map(|v| v.trim()).filter(|v| !v.is_empty() && *v != "auto")
    }

    /// Applies edits as `write_system_cfg` makes them on the file: None removes the key.
    pub fn apply(&mut self, edits: &[(String, Option<String>)]) {
        for (k, v) in edits {
            match v {
                Some(v) if !v.is_empty() => {
                    self.map.insert(k.clone(), v.clone());
                }
                _ => {
                    self.map.remove(k);
                }
            }
        }
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// The emulator/core keys set, for the log at start ("gba.core=gpsp").
    pub fn choice_lines(&self) -> Vec<String> {
        let mut v: Vec<String> = self
            .map
            .iter()
            .filter(|(k, _)| k.ends_with(".emulator") || k.ends_with(".core"))
            .map(|(k, v)| format!("{k}={v}"))
            .collect();
        v.sort();
        v
    }
}

/// ES's name for a game's keys: `<system>["<file name>"]`, with `=` and `#` taken out of the name.
pub fn game_config_name(system: &str, file: &str) -> String {
    let file: String = file.chars().filter(|c| *c != '=' && *c != '#').collect();
    format!("{system}[\"{file}\"]")
}

/// Which level of system.cfg a key belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    Console,
    Game,
}

/// The key of `name` ("emulator", "ratio") at `level` for this system and file.
pub fn key(system: &str, file: Option<&str>, level: Level, name: &str) -> String {
    match (level, file) {
        (Level::Game, Some(f)) => format!("{}.{name}", game_config_name(system, f)),
        _ => format!("{system}.{name}"),
    }
}

// ---- resolving -------------------------------------------------------------------------------------------------

/// What `resolve` needs to know about a system and a game besides system.cfg.
pub struct Ctx<'a> {
    pub system: &'a str,
    /// es_systems.cfg's emulators for the system.
    pub emulators: &'a [EsEmulator],
    /// The libretro core pocket-core plays the system with here: hosted and verified, its BIOS in place where the
    /// default emulator does without one, and pocket-core installed (`hostsys::route`, `HOST_CORES`). None: pocket-core
    /// is not a choice for this system.
    pub pocket_core: Option<&'a str>,
    /// pocket-core takes this game's file (not a 7z, a file type it opens). True for the system as a whole.
    pub file_ok: bool,
    /// pocket-core couldn't start this game before (`fallback.json`): it goes to the next choice down.
    pub fell_back: bool,
    /// What stands in for es_systems' default where that can't run (PICO-8 without the official binary:
    /// RetroArch's fake08, `hostsys::default_emulator`).
    pub default_override: Option<(&'a str, &'a str)>,
}

impl Ctx<'_> {
    /// pocket-core may play this game.
    fn pocket_ok(&self) -> bool {
        self.pocket_core.is_some() && self.file_ok && !self.fell_back
    }

    fn pocket(&self) -> Option<Runner> {
        self.pocket_core.map(|c| Runner::new(POCKET_CORE, c))
    }

    /// ROCKNIX's own default for the system, without pocket-core.
    fn es_default(&self) -> Runner {
        match self.default_override {
            Some((e, c)) => Runner::new(e, c),
            None => es_default(self.emulators).unwrap_or_default(),
        }
    }
}

/// Where the runner of a game came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum From {
    /// This game's own choice.
    Game,
    /// The console's choice.
    Console,
    /// Nobody chose: pocket-core where it hosts the system, else ROCKNIX's default.
    Default,
    /// pocket-core was the choice (or the default), but it couldn't start this game before: ROCKNIX's default.
    Fallback,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Resolved {
    pub runner: Runner,
    pub from: From,
}

/// The pair chosen at one level, the ES way, or None when nothing (valid) is chosen there.
///
/// * `pocket-core`: pocket-ui's host, with the core it uses for the system (only where it hosts the system).
/// * An emulator of the system's list: the chosen core when that emulator has it, else the emulator's default core.
///   (ES itself takes the next level's core when a game names an emulator without a core, which can pair an
///   emulator with another emulator's core; pocket-ui keeps the pair valid instead.)
/// * Only a core (ES writes both, people editing by hand may not): that core under `base`'s emulator when it has
///   it (ES `getCore`: the core is looked up under the emulator in use).
/// * Anything else (a name es_systems.cfg doesn't list, "auto"): nothing chosen, as in ES.
fn chosen(ctx: &Ctx, cfg: &Cfg, emu_key: &str, core_key: &str, base: &Runner) -> Option<Runner> {
    let emu = cfg.get(emu_key);
    let core = cfg.get(core_key);
    match emu {
        Some(POCKET_CORE) => ctx.pocket(),
        Some(name) => {
            let e = emulator(ctx.emulators, name)?;
            let core = core.filter(|c| e.cores.iter().any(|x| x == c)).unwrap_or_else(|| default_core(e));
            Some(Runner::new(&e.name, core))
        }
        None => {
            let core = core?;
            let e = emulator(ctx.emulators, &base.emulator)?;
            e.cores.iter().any(|x| x == core).then(|| Runner::new(&e.name, core))
        }
    }
}

/// The console's own choice, if it made one that means something here.
pub fn console_choice(ctx: &Ctx, cfg: &Cfg) -> Option<Runner> {
    let base = ctx.es_default();
    chosen(
        ctx,
        cfg,
        &key(ctx.system, None, Level::Console, "emulator"),
        &key(ctx.system, None, Level::Console, "core"),
        &base,
    )
}

/// The game's own choice.
pub fn game_choice(ctx: &Ctx, cfg: &Cfg, file: &str) -> Option<Runner> {
    let base = console_choice(ctx, cfg).filter(|r| !r.is_pocket_core()).unwrap_or_else(|| ctx.es_default());
    let f = Some(file);
    chosen(
        ctx,
        cfg,
        &key(ctx.system, f, Level::Game, "emulator"),
        &key(ctx.system, f, Level::Game, "core"),
        &base,
    )
}

/// The emulator and core a game (`file`, its file name) or the system as a whole (None) starts with: the game's
/// choice, else the console's, else the default (pocket-core where it hosts the system, else ROCKNIX's default). A
/// pocket-core choice the game can't take (a 7z, a game pocket-core couldn't start) goes to the next level down.
pub fn resolve(ctx: &Ctx, cfg: &Cfg, file: Option<&str>) -> Resolved {
    let mut blocked = false;
    let game = file.and_then(|f| game_choice(ctx, cfg, f));
    for (choice, from) in [(game, From::Game), (console_choice(ctx, cfg), From::Console)] {
        if let Some(r) = choice {
            if r.is_pocket_core() && !ctx.pocket_ok() {
                blocked |= ctx.fell_back;
                continue;
            }
            return Resolved { runner: r, from };
        }
    }
    if ctx.pocket_ok() {
        return Resolved {
            runner: ctx.pocket().expect("pocket_ok"),
            from: From::Default,
        };
    }
    let from = if blocked || (ctx.fell_back && ctx.pocket_core.is_some() && ctx.file_ok) {
        From::Fallback
    } else {
        From::Default
    };
    Resolved {
        runner: ctx.es_default(),
        from,
    }
}

/// The default of the system as a whole (what "Restore defaults" and an unset console give): pocket-core where it
/// hosts the system, else ROCKNIX's default.
pub fn system_default(ctx: &Ctx) -> Runner {
    ctx.pocket().unwrap_or_else(|| ctx.es_default())
}

/// What the player can pick for this system or game, in this order: pocket-core (where it hosts the system and
/// takes the file), then es_systems.cfg's pairs.
pub fn choices(ctx: &Ctx) -> Vec<Runner> {
    let mut v = Vec::new();
    if let (Some(p), true) = (ctx.pocket(), ctx.file_ok) {
        v.push(p);
    }
    v.extend(es_pairs(ctx.emulators));
    if let Some((e, c)) = ctx.default_override {
        let r = Runner::new(e, c);
        if !v.contains(&r) {
            v.push(r);
        }
    }
    v
}

/// Whether ROCKNIX can start this pair on this Pocket: RetroArch needs the core's library (runemu.sh loads
/// `/tmp/cores/<core>_libretro.so`, an overlay of `/usr/lib/libretro` and `/storage/cores`); a standalone emulator its
/// start script (runemu.sh runs `/usr/bin/start_<core without its last -part>.sh`, Mednafen `start_mednafen.sh`,
/// GameCube and Wii Dolphin's own two). pocket-core is there when it is offered. `exists` stands in for the file
/// system in tests.
pub fn installed(r: &Runner, system: &str, exists: impl Fn(&Path) -> bool) -> bool {
    match r.kind() {
        Kind::PocketCore => true,
        Kind::RetroArch => {
            let so = format!("{}_libretro.so", r.core);
            ["/tmp/cores", "/usr/lib/libretro", "/storage/cores"]
                .iter()
                .any(|d| exists(&Path::new(d).join(&so)))
        }
        // hakuX is started by pocket-ui itself: its core in pocket-ui's folder and ROCKNIX's RetroArch.
        Kind::Standalone if r.emulator == "hakux" => exists(&crate::hakux::core_path()) && exists(Path::new(crate::hakux::RETROARCH)),
        Kind::Standalone => {
            let script = match (r.emulator.as_str(), system) {
                ("mednafen", _) => "start_mednafen.sh".to_string(),
                (_, "gamecube" | "triforce") => "start_dolphin_gc.sh".to_string(),
                (_, "wii" | "wiiware") => "start_dolphin_wii.sh".to_string(),
                _ => {
                    let c = r.core.rsplit_once('-').map_or(r.core.as_str(), |(head, _)| head);
                    format!("start_{c}.sh")
                }
            };
            exists(&Path::new("/usr/bin").join(script))
        }
    }
}

// ---- names -----------------------------------------------------------------------------------------------------

/// A libretro core as people know it; distinct for the variants a list shows side by side.
pub fn core_label(core: &str) -> String {
    let known = match core {
        "mgba" => "mGBA",
        "gpsp" => "gpSP",
        "vbam" => "VBA-M",
        "vba_next" => "VBA Next",
        "beetle_gba" => "Beetle GBA",
        "skyemu" => "SkyEmu",
        "gambatte" => "Gambatte",
        "sameboy" => "SameBoy",
        "gearboy" => "Gearboy",
        "tgbdual" => "TGB Dual",
        "DoubleCherryGB" => "DoubleCherryGB",
        "mesen-s" => "Mesen-S",
        "mesen" => "Mesen",
        "supersnes9x" => "SuperSnes9x",
        "snes9x" => "Snes9x",
        "snes9x2010" => "Snes9x 2010",
        "snes9x2002" => "Snes9x 2002",
        "snes9x2005_plus" => "Snes9x 2005 Plus",
        "bsnes" => "bsnes",
        "bsnes_hd_beta" => "bsnes HD beta",
        "bsnes_mercury_accuracy" => "bsnes-mercury Accuracy",
        "bsnes_mercury_balanced" => "bsnes-mercury Balanced",
        "bsnes_mercury_performance" => "bsnes-mercury Performance",
        "bsnes2014_accuracy" => "bsnes 2014 Accuracy",
        "bsnes2014_balanced" => "bsnes 2014 Balanced",
        "bsnes2014_performance" => "bsnes 2014 Performance",
        "beetle_supafaust" => "Beetle Supafaust",
        "nestopia" => "Nestopia",
        "fceumm" => "FCEUmm",
        "quicknes" => "QuickNES",
        "genesis_plus_gx" => "Genesis Plus GX",
        "genesis_plus_gx_wide" => "Genesis Plus GX Wide",
        "picodrive" => "PicoDrive",
        "gearsystem" => "Gearsystem",
        "smsplus" => "SMS Plus",
        "beetle_pce_fast" => "Beetle PCE Fast",
        "beetle_pce" => "Beetle PCE",
        "beetle_supergrafx" => "Beetle SuperGrafx",
        "beetle_vb" => "Beetle VB",
        "geargrafx" => "Geargrafx",
        "beetle_ngp" => "Beetle NeoPop",
        "race" => "RACE",
        "beetle_wswan" => "Beetle WonderSwan",
        "handy" => "Handy",
        "beetle_lynx" => "Beetle Lynx",
        "gearlynx" => "Gearlynx",
        "stella" => "Stella",
        "prosystem" => "ProSystem",
        "pokemini" => "PokeMini",
        "pcsx_rearmed" => "PCSX ReARMed",
        "pcsx_rearmed32" => "PCSX ReARMed 32-bit",
        "beetle_psx" => "Beetle PSX",
        "duckstation" => "DuckStation",
        "swanstation" => "SwanStation",
        "fbneo" => "FinalBurn Neo",
        "fbalpha2012" => "FB Alpha 2012",
        "fbalpha2019" => "FB Alpha 2019",
        "mame" => "MAME",
        "mame2003_plus" => "MAME 2003-Plus",
        "mame2010" => "MAME 2010",
        "mame2015" => "MAME 2015",
        "mupen64plus_next" => "Mupen64Plus-Next",
        "parallel_n64" => "ParaLLEl N64",
        "ppsspp" => "PPSSPP",
        "flycast" => "Flycast",
        "flycast2021" => "Flycast 2021",
        "melondsds" => "melonDS DS",
        "melonds" => "melonDS",
        "desmume" => "DeSmuME",
        "yabasanshiro" => "YabaSanshiro",
        "beetle_saturn" => "Beetle Saturn",
        "kronos" => "Kronos",
        "dosbox_pure" => "DOSBox Pure",
        "dosbox_core" => "DOSBox Core",
        "scummvm" => "ScummVM",
        "fake08" => "FAKE-08",
        "tic80" => "TIC-80",
        "dolphin" => "Dolphin",
        "puae" => "PUAE",
        "puae2021" => "PUAE 2021",
        "uae4arm" => "UAE4ARM",
        "neocd" => "NeoCD",
        "geolith" => "Geolith",
        "opera" => "Opera",
        "bluemsx" => "blueMSX",
        "fmsx" => "fMSX",
        "virtualjaguar" => "Virtual Jaguar",
        "hatari" => "Hatari",
        "atari800" => "Atari800",
        "a5200" => "a5200",
        "gearcoleco" => "Gearcoleco",
        "freeintv" => "FreeIntv",
        "vecx" => "vecx",
        "prboom" => "PrBoom",
        "daphne" => "Daphne",
        "ffmpeg" => "FFmpeg",
        other => return other.replace('_', " "),
    };
    known.to_string()
}

/// A standalone emulator as people know it, from es_systems.cfg's emulator name.
pub fn emulator_label(emulator: &str) -> String {
    let known = match emulator {
        "retroarch" => "RetroArch",
        POCKET_CORE => "pocket-core",
        "mednafen" => "Mednafen",
        "vicesa" => "VICE",
        "hatarisa" => "Hatari",
        "AdvanceMame" => "AdvanceMAME",
        "hypseus-singe" => "Hypseus Singe",
        "gzdoom" => "GZDoom",
        "nanoboyadvance" => "NanoBoyAdvance",
        "skyemu" => "SkyEmu",
        "dsperate" => "DSPerate",
        "daedalusx64" => "DaedalusX64",
        "bigpemu" => "BigPEmu",
        "amiberry" => "Amiberry",
        "OpenBOR" => "OpenBOR",
        "touchhle" => "touchHLE",
        "scummvm-sa" => "ScummVM",
        "pico-8" => "PICO-8",
        "aethersx2" => "AetherSX2",
        "rpcs3" => "RPCS3",
        "vita3k" => "Vita3K",
        "cemu" => "Cemu",
        "xemu" => "xemu",
        "hakux" => "hakuX (RetroArch)",
        "supermodel" => "Supermodel",
        "mplayer" => "MPlayer",
        "drastic" => "DraStic",
        _ => return crate::consoles::emulator_name(emulator),
    };
    known.to_string()
}

/// "pocket-core · mGBA", "RetroArch · gpSP", "Standalone · PPSSPP"; a standalone emulator with more than one way to
/// start (Dolphin's Qt version, Mednafen's cores) adds which: "Standalone · Dolphin (Qt)".
pub fn label(r: &Runner, emulators: &[EsEmulator]) -> String {
    match r.kind() {
        Kind::PocketCore => format!("pocket-core · {}", core_label(&r.core)),
        Kind::RetroArch => format!("RetroArch · {}", core_label(&r.core)),
        Kind::Standalone => {
            let name = emulator_label(&r.emulator);
            let several = emulator(emulators, &r.emulator).is_some_and(|e| e.cores.len() > 1);
            if several && !r.core.is_empty() {
                let variant = if r.core.contains("-qt") {
                    "Qt".to_string()
                } else {
                    r.core.trim_end_matches("-sa").to_string()
                };
                format!("Standalone · {name} ({variant})")
            } else {
                format!("Standalone · {name}")
            }
        }
    }
}

/// What the choice means in pocket-ui, one or two short sentences for the page.
pub fn note(r: &Runner, system: &str) -> String {
    match r.kind() {
        // ScummVM, PICO-8, TIC-80: pocket-core runs them without save states, so no slots and no continuing
        // (`hostsys::HostSystem::no_states`, the pause menu's line: `pause_saves`).
        Kind::PocketCore if crate::hostsys::find(system).is_some_and(|h| h.no_states.is_some()) => {
            "pocket-ui's own host, with Options. No save states for these games: save in the game before you quit.".to_string()
        }
        Kind::PocketCore => "pocket-ui's own host: continues at once where you left off, eight save slots, Options, run-ahead, \
             fast switching and achievements."
            .to_string(),
        Kind::RetroArch => "ROCKNIX's RetroArch: one save, Quit saves the moment. Starts slower; no pocket-ui slots, Options, run-ahead, \
             fast switching or achievements."
            .to_string(),
        Kind::Standalone => {
            let name = emulator_label(&r.emulator);
            match crate::standalone::Emu::detect(system, &r.emulator, &r.core) {
                // Azahar has its quick slot and no shortcut for another (docs/user/pause-menu.md "The 3DS has one slot").
                Some(crate::standalone::Emu::Azahar) => {
                    format!("{name} on its own, with pocket-ui's Save (one slot), Load, Options and continuing where you left off.")
                }
                Some(crate::standalone::Emu::Hakux) => {
                    "Xbox games in RetroArch with the hakuX core. Experimental: Continue and Quit from pocket-ui's menu, saves in the game, no save states yet."
                        .to_string()
                }
                Some(_) => format!("{name} on its own, with pocket-ui's Save, Load, Options and continuing where you left off."),
                None => {
                    format!("{name} on its own, with its own saves and settings. Save in the game before you quit: pocket-ui can't continue it.")
                }
            }
        }
    }
}

// ---- RetroArch settings ------------------------------------------------------------------------------------------

pub struct RaChoice {
    /// What system.cfg holds (ES's values).
    pub value: &'static str,
    pub label: &'static str,
}

pub struct RaSetting {
    /// The key after `<system>.` (setsettings.sh's `game_setting <key>`).
    pub key: &'static str,
    pub name: &'static str,
    pub desc: &'static str,
    pub choices: &'static [RaChoice],
    /// The value setsettings.sh ends up with when no level sets one.
    pub unset: &'static str,
    /// Systems setsettings.sh switches it off for (its NO_REWIND / NO_RUNAHEAD): the row isn't shown there.
    pub not_for: &'static [&'static str],
}

const fn ch(value: &'static str, label: &'static str) -> RaChoice {
    RaChoice { value, label }
}

/// setsettings.sh's NO_REWIND.
const NO_REWIND: &[&str] = &[
    "atomiswave",
    "dreamcast",
    "mame",
    "n64",
    "naomi",
    "neogeocd",
    "odyssey2",
    "psp",
    "pspockets",
    "saturn",
    "sega32x",
    "zxspectrum",
];
/// setsettings.sh's NO_RUNAHEAD.
const NO_RUNAHEAD: &[&str] = &["atomiswave", "dreamcast", "n64", "naomi", "neogeocd", "psp", "saturn", "sega32x"];
const ON_OFF: &[RaChoice] = &[ch("0", "Off"), ch("1", "On")];
/// The shader row's "none"; the presets come from the Pocket (`shader_presets`).
pub const SHADER_NONE: &str = "none";

/// The RetroArch settings the page offers, in its order. setsettings.sh turns them into retroarch.cfg lines at every
/// RetroArch start (`set_aspectratio`, `set_integerscale`, `set_filtering`, `set_shader`, `set_rewind`,
/// `set_runahead`, `set_autosave`).
pub const RA_SETTINGS: &[RaSetting] = &[
    RaSetting {
        key: "ratio",
        name: "Aspect ratio",
        desc: "The shape of the picture. Core provided is what the emulator reports.",
        // setsettings.sh's CORE_RATIOS names; RetroArch's aspect_ratio_index follows from them.
        choices: &[
            ch("core", "Core provided"),
            ch("4/3", "4:3"),
            ch("3/2", "3:2"),
            ch("16/9", "16:9"),
            ch("squarepixel", "Square pixels"),
            ch("full", "Full screen"),
        ],
        unset: "core",
        not_for: &[],
    },
    RaSetting {
        key: "integerscale",
        name: "Integer scaling",
        desc: "Scales by whole numbers only: every pixel the same size, with black borders.",
        choices: ON_OFF,
        unset: "0",
        not_for: &[],
    },
    RaSetting {
        key: "smooth",
        name: "Smoothing",
        desc: "Bilinear filtering: softer pixel edges.",
        choices: ON_OFF,
        unset: "0",
        not_for: &[],
    },
    RaSetting {
        key: "shaderset",
        name: "Shader",
        desc: "A RetroArch shader preset over the picture, from ROCKNIX's shaders on this Pocket.",
        choices: &[ch(SHADER_NONE, "None")],
        unset: SHADER_NONE,
        not_for: &[],
    },
    RaSetting {
        key: "rewind",
        name: "Rewind",
        desc: "Keeps a history to play backwards: hold HOME + L2. Costs processor time.",
        choices: ON_OFF,
        unset: "0",
        not_for: NO_REWIND,
    },
    RaSetting {
        key: "runahead",
        name: "Run-ahead",
        desc: "RetroArch's own run-ahead: the game responds 1 or 2 frames sooner, at the cost of processor time.",
        choices: &[ch("0", "Off"), ch("1", "1 frame"), ch("2", "2 frames")],
        unset: "0",
        not_for: NO_RUNAHEAD,
    },
    RaSetting {
        key: "autosave",
        name: "Auto save and load",
        desc: "Saves the moment when the game ends and loads it at the next start. Off: pocket-ui can't continue the game where you left off.",
        choices: &[ch("1", "On"), ch("0", "Off")],
        unset: "0",
        not_for: &[],
    },
];

impl RaSetting {
    pub fn applies(&self, system: &str) -> bool {
        !self.not_for.contains(&system)
    }

    /// The label of `value`: a choice's, a shader's file name, else the value as ES wrote it ("8/7").
    pub fn value_label(&self, value: &str) -> String {
        if let Some(c) = self.choices.iter().find(|c| c.value == value) {
            return c.label.to_string();
        }
        if self.key == "shaderset" {
            return shader_label(value);
        }
        value.replace('/', ":")
    }
}

/// Every key below a console's own: `system.<key>`, `<key>`, `global.<key>`, in get_setting's order.
pub fn below_console(cfg: &Cfg, name: &str) -> Option<String> {
    [format!("system.{name}"), name.to_string(), format!("global.{name}")]
        .iter()
        .find_map(|k| cfg.get(k).map(str::to_string))
}

/// What setsettings.sh gets for `s` at the console level (the game's key left out).
pub fn console_value(cfg: &Cfg, system: &str, s: &RaSetting) -> String {
    cfg.get(&key(system, None, Level::Console, s.key))
        .map(str::to_string)
        .or_else(|| below_console(cfg, s.key))
        .unwrap_or_else(|| s.unset.to_string())
}

/// What setsettings.sh gets for `s` for this game (get_setting with the file name), or for the console.
pub fn effective_value(cfg: &Cfg, system: &str, file: Option<&str>, s: &RaSetting) -> String {
    file.and_then(|f| cfg.get(&key(system, Some(f), Level::Game, s.key)).map(str::to_string))
        .unwrap_or_else(|| console_value(cfg, system, s))
}

/// Whether ROCKNIX's `get_setting` can find a per-game key for this file name (`emu_settings::es_findable`): if not,
/// RetroArch settings for this game would never be read, so the page keeps them for the console only.
pub fn findable(file: &str) -> bool {
    crate::emu_settings::es_findable(file)
}

// ---- shader presets ----------------------------------------------------------------------------------------------

/// Where ROCKNIX's RetroArch finds shaders: an overlay of the shipped shader packages and `/storage/shaders`
/// (retroarch's tmp-shaders.mount). setsettings.sh passes `--set-shader /tmp/shaders/<value>`.
pub const SHADER_DIR: &str = "/tmp/shaders";
/// The device's profile says which kind of preset its RetroArch takes (`UI_SHADER`, SM8250: slangp).
const UI_SHADER_PROFILE: &str = "/storage/.config/profile.d/091-ui_shader";

/// The presets the page offers, by file name without the extension, in this order: a short list of the well-known
/// CRT, LCD and sharp-scaling presets. ROCKNIX ships hundreds (ES lists them all, from `/usr/bin/getshaders`); a
/// list that long can't be stepped through with Left and Right, so only these are offered, and a preset chosen in
/// ES is kept and shown.
const SHADER_PICKS: &[&str] = &[
    "zfast-crt",
    "zfast_crt_standard",
    "crt-geom",
    "fake-crt-geom",
    "crt-easymode",
    "crt-lottes",
    "crt-pi",
    "scanline",
    "snes_scanline",
    "lcd-grid-v2",
    "zfast-lcd",
    "zfast_lcd_standard",
    "gameboy",
    "dot",
    "gba-color",
    "sharp-bilinear-simple",
];

/// The presets of `SHADER_PICKS` under `root`, as setsettings.sh's values (the path under `/tmp/shaders`), at most
/// one per name (the shortest path), in `SHADER_PICKS` order. `exts`: the preset kinds RetroArch takes here.
pub fn scan_shaders(root: &Path, exts: &[&str]) -> Vec<String> {
    let mut found: HashMap<&'static str, String> = HashMap::new();
    let mut stack = vec![(root.to_path_buf(), 0u32)];
    while let Some((dir, depth)) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for e in entries.flatten() {
            let path = e.path();
            let Ok(ft) = e.file_type() else { continue };
            if ft.is_dir() {
                if depth < 4 {
                    stack.push((path, depth + 1));
                }
                continue;
            }
            let ext = path.extension().and_then(|x| x.to_str()).unwrap_or_default();
            if !exts.contains(&ext) {
                continue;
            }
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else { continue };
            let Some(pick) = SHADER_PICKS.iter().find(|p| **p == stem) else { continue };
            let Ok(rel) = path.strip_prefix(root) else { continue };
            let rel = rel.to_string_lossy().into_owned();
            let better = found
                .get(pick)
                .is_none_or(|old| rel.len() < old.len() || (rel.len() == old.len() && rel < *old));
            if better {
                found.insert(pick, rel);
            }
        }
    }
    SHADER_PICKS.iter().filter_map(|p| found.remove(p)).collect()
}

/// "crt-geom" for "crt/crt-geom.slangp".
pub fn shader_label(value: &str) -> String {
    if value == SHADER_NONE {
        return "None".to_string();
    }
    let name = value.rsplit('/').next().unwrap_or(value);
    name.rsplit_once('.').map_or(name, |(stem, _)| stem).to_string()
}

/// The preset kinds the device's RetroArch takes: its `UI_SHADER`, else both.
fn shader_exts() -> Vec<&'static str> {
    let profile = std::fs::read_to_string(UI_SHADER_PROFILE).unwrap_or_default();
    if profile.contains("\"slangp\"") {
        vec!["slangp"]
    } else if profile.contains("\"glslp\"") {
        vec!["glslp"]
    } else {
        vec!["slangp", "glslp"]
    }
}

static SHADERS: Mutex<Option<Arc<Vec<String>>>> = Mutex::new(None);

/// Starts finding the presets on a thread, once (the page asks when it opens; a few thousand files on the card).
pub fn find_shaders_in_background() {
    {
        let mut s = SHADERS.lock().unwrap_or_else(|e| e.into_inner());
        if s.is_some() {
            return;
        }
        // Taken: a second page opening doesn't start another walk.
        *s = Some(Arc::new(Vec::new()));
    }
    let _ = std::thread::Builder::new().name("shader-presets".into()).spawn(|| {
        let started = std::time::Instant::now();
        let found = scan_shaders(Path::new(SHADER_DIR), &shader_exts());
        println!("emulator choice: {} shader presets in {:?}", found.len(), started.elapsed());
        *SHADERS.lock().unwrap_or_else(|e| e.into_inner()) = Some(Arc::new(found));
    });
}

/// The presets found so far (empty until the thread is done).
pub fn shader_presets() -> Arc<Vec<String>> {
    SHADERS.lock().unwrap_or_else(|e| e.into_inner()).clone().unwrap_or_default()
}

/// Tests and snapshots: the presets as given.
pub fn set_shader_presets(v: Vec<String>) {
    *SHADERS.lock().unwrap_or_else(|e| e.into_inner()) = Some(Arc::new(v));
}

// ---- the cache the launch reads ----------------------------------------------------------------------------------

static CHOICES: RwLock<Option<Arc<Cfg>>> = RwLock::new(None);

/// Reads system.cfg's emulator and core keys on a thread at start, so neither a launch nor the shelf reads the file
/// on the UI thread. Logs what it found: a choice made earlier in EmulationStation is honoured from now on.
pub fn preload() {
    let _ = std::thread::Builder::new().name("emu-choices".into()).spawn(|| {
        let started = std::time::Instant::now();
        let cfg = Arc::new(Cfg::choices_only(&std::fs::read_to_string(SYSTEM_CFG).unwrap_or_default()));
        let lines = cfg.choice_lines();
        let mut slot = CHOICES.write().unwrap_or_else(|e| e.into_inner());
        if slot.is_none() {
            *slot = Some(cfg);
        }
        println!(
            "emulator choices from system.cfg ({:?}): {}",
            started.elapsed(),
            if lines.is_empty() { "none".to_string() } else { lines.join(", ") }
        );
    });
}

/// The emulator and core keys: the preloaded ones, or (only if a game starts before the thread is done) read now.
pub fn cached() -> Arc<Cfg> {
    if let Some(c) = CHOICES.read().unwrap_or_else(|e| e.into_inner()).as_ref() {
        return c.clone();
    }
    let cfg = Arc::new(Cfg::choices_only(&std::fs::read_to_string(SYSTEM_CFG).unwrap_or_default()));
    *CHOICES.write().unwrap_or_else(|e| e.into_inner()) = Some(cfg.clone());
    cfg
}

/// The page saved: the next launch sees the edits at once, before the file is written.
pub fn apply_to_cache(edits: &[(String, Option<String>)]) {
    let mut slot = CHOICES.write().unwrap_or_else(|e| e.into_inner());
    let mut cfg = slot.as_deref().cloned().unwrap_or_default();
    cfg.apply(edits);
    cfg.map.retain(|k, _| k.ends_with(".emulator") || k.ends_with(".core"));
    *slot = Some(Arc::new(cfg));
}

/// Tests and snapshots: the keys as given.
pub fn set_cached(cfg: Cfg) {
    *CHOICES.write().unwrap_or_else(|e| e.into_inner()) = Some(Arc::new(cfg));
}

// ---- writing system.cfg -------------------------------------------------------------------------------------------

/// ROCKNIX's shell functions (`set_setting`, `wait_lock`, `J_CONF`, `J_CONF_LOCK`).
pub const FUNCTIONS: &str = "/etc/profile.d/001-functions";

/// The shell side of `write_system_cfg`. `$1` the functions file, `$2`/`$3` the file and lock to use instead of
/// ROCKNIX's (tests; empty on the device), then key/value pairs; an empty value removes the key.
///
/// Why not `set_setting` for each key: its `del_setting` deletes with `sed "/^<key>=/d"`, and a per-game key
/// (`gba["Game (USA).gba"].core`) is a bracket expression to sed, so the old line stays and a second one is added.
/// This does what set_setting does (start from the shipped file when there is none, take ROCKNIX's lock with its own
/// `wait_lock`, drop the old line, append `key=value`), matching the key as text.
const WRITE_SCRIPT: &str = r#". "$1" >/dev/null 2>&1 || exit 2
[ -n "$2" ] && J_CONF=$2
[ -n "$3" ] && J_CONF_LOCK=$3
shift 3
if [ ! -e "$J_CONF" ]; then
  mkdir -p "${J_CONF%/*}"
  cp -f /usr/config/system/configs/system.cfg "$J_CONF" 2>/dev/null || : >"$J_CONF"
fi
wait_lock
tmp="$J_CONF.pocket-ui.$$"
cp "$J_CONF" "$tmp" || exit 3
while [ $# -ge 2 ]; do
  K=$1 V=$2
  shift 2
  K="$K" awk 'index($0, ENVIRON["K"] "=") != 1' "$tmp" >"$tmp.n" && mv "$tmp.n" "$tmp" || exit 4
  [ -n "$V" ] && printf '%s=%s\n' "$K" "$V" >>"$tmp"
done
mv "$tmp" "$J_CONF" || exit 5
sync
rm -f "$J_CONF_LOCK"
"#;

/// Writes `edits` into system.cfg (None or "" removes the key) the way ROCKNIX's `set_setting` would, under its
/// lock `/tmp/.system.cfg.lock`. Blocking: call it on a thread. `functions`, `file`, `lock`: ROCKNIX's when None.
///
/// SYSTEM-CFG WRITER: feat/rescue-path builds the one locked system.cfg writer for pocket-ui. When that lands, the
/// main session switches this call over to it (the edits are plain key/value pairs; removal is a None).
pub fn write_system_cfg(edits: &[(String, Option<String>)], functions: Option<&Path>, file: Option<&Path>, lock: Option<&Path>) -> Result<(), String> {
    if edits.is_empty() {
        return Ok(());
    }
    let s = |p: Option<&Path>| p.map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
    let mut cmd = std::process::Command::new("sh");
    cmd.arg("-c").arg(WRITE_SCRIPT).arg("pocket-ui-system-cfg");
    cmd.arg(functions.map_or_else(|| FUNCTIONS.to_string(), |p| p.to_string_lossy().into_owned()));
    cmd.arg(s(file)).arg(s(lock));
    for (k, v) in edits {
        if k.contains('\n') || v.as_deref().is_some_and(|v| v.contains('\n')) {
            return Err(format!("{k}: a line break can't go into system.cfg"));
        }
        cmd.arg(k).arg(v.as_deref().unwrap_or(""));
    }
    let out = crate::proc::run_with_timeout(&mut cmd, std::time::Duration::from_secs(20)).map_err(|e| format!("system.cfg: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!(
            "system.cfg: the writer ended with {} {}",
            out.status,
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

/// `write_system_cfg` on a thread of its own, for the UI.
pub fn write_in_background(edits: Vec<(String, Option<String>)>) {
    let _ = std::thread::Builder::new().name("system-cfg".into()).spawn(move || {
        let started = std::time::Instant::now();
        match write_system_cfg(&edits, None, None, None) {
            Ok(()) => println!("emulator choice: wrote {} system.cfg key(s) in {:?}", edits.len(), started.elapsed()),
            Err(e) => eprintln!("emulator choice: {e}"),
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;

    /// es_systems.cfg as ROCKNIX 20261001 generates it for the Mini V2 (SM8250), three systems.
    const ES_SYSTEMS: &str = r#"<systemList>
  <system><name>gba</name><emulators>
    <emulator name="retroarch"><cores><core default="true">mgba</core><core>vbam</core><core>vba_next</core><core>beetle_gba</core><core>skyemu</core><core>gpsp</core></cores></emulator>
    <emulator name="nanoboyadvance"><cores><core>nanoboyadvance-sa</core></cores></emulator>
    <emulator name="mednafen"><cores><core>gba</core></cores></emulator>
    <emulator name="skyemu"><cores><core>skyemu-sa</core></cores></emulator>
  </emulators></system>
  <system><name>nds</name><emulators>
    <emulator name="melonds"><cores><core default="true">melonds-sa</core></cores></emulator>
    <emulator name="drastic"><cores><core>drastic-sa</core></cores></emulator>
    <emulator name="retroarch"><cores><core>melonds</core><core>melondsds</core><core>desmume</core></cores></emulator>
  </emulators></system>
  <system><name>gamecube</name><emulators>
    <emulator name="dolphin"><cores><core default="true">dolphin-sa-gc</core><core>dolphin-qt-gc</core></cores></emulator>
    <emulator name="retroarch"><cores><core>dolphin</core></cores></emulator>
  </emulators></system>
</systemList>"#;

    fn emulators(system: &str) -> Vec<EsEmulator> {
        let doc = roxmltree::Document::parse(ES_SYSTEMS).unwrap();
        let node = doc
            .descendants()
            .find(|n| n.has_tag_name("system") && n.children().any(|c| c.has_tag_name("name") && c.text() == Some(system)))
            .unwrap();
        parse_emulators(node)
    }

    fn ctx<'a>(system: &'a str, emus: &'a [EsEmulator], pocket: Option<&'a str>) -> Ctx<'a> {
        Ctx {
            system,
            emulators: emus,
            pocket_core: pocket,
            file_ok: true,
            fell_back: false,
            default_override: None,
        }
    }

    fn run(r: &Resolved) -> (&str, &str, From) {
        (r.runner.emulator.as_str(), r.runner.core.as_str(), r.from)
    }

    #[test]
    fn es_systems_lists_are_read_with_their_defaults() {
        let gba = emulators("gba");
        assert_eq!(gba.len(), 4);
        assert_eq!(gba[0].cores.len(), 6);
        assert_eq!(es_default(&gba), Some(Runner::new("retroarch", "mgba")));
        // The default emulator is the one with a default core, not the first (NDS: standalone melonDS).
        assert_eq!(es_default(&emulators("nds")), Some(Runner::new("melonds", "melonds-sa")));
        assert_eq!(es_pairs(&emulators("nds")).len(), 5);
        assert_eq!(es_default(&[]), None);
    }

    #[test]
    fn nothing_chosen_is_pocket_core_where_it_hosts_else_rocknix_default() {
        let gba = emulators("gba");
        let cfg = Cfg::default();
        assert_eq!(
            run(&resolve(&ctx("gba", &gba, Some("mgba")), &cfg, Some("Zelda.gba"))),
            ("pocket-core", "mgba", From::Default)
        );
        assert_eq!(
            run(&resolve(&ctx("gba", &gba, None), &cfg, Some("Zelda.gba"))),
            ("retroarch", "mgba", From::Default)
        );
        // A 7z: pocket-core doesn't open it.
        let c = Ctx {
            file_ok: false,
            ..ctx("gba", &gba, Some("mgba"))
        };
        assert_eq!(run(&resolve(&c, &cfg, Some("Zelda.7z"))), ("retroarch", "mgba", From::Default));
        // PICO-8 without the official binary: RetroArch's fake08 stands in for the default.
        let pico = vec![
            EsEmulator {
                name: "pico-8".into(),
                cores: vec!["pico8".into()],
                default_core: Some(0),
            },
            EsEmulator {
                name: "retroarch".into(),
                cores: vec!["fake08".into()],
                default_core: None,
            },
        ];
        let c = Ctx {
            default_override: Some(("retroarch", "fake08")),
            ..ctx("pico-8", &pico, None)
        };
        assert_eq!(run(&resolve(&c, &cfg, Some("cart.p8"))), ("retroarch", "fake08", From::Default));
    }

    #[test]
    fn a_choice_made_in_emulationstation_is_honoured() {
        let gba = emulators("gba");
        let c = ctx("gba", &gba, Some("mgba"));
        // What ES's "Per system advanced configuration" writes.
        let cfg = Cfg::parse("gba.emulator=retroarch\ngba.core=gpsp\n");
        assert_eq!(run(&resolve(&c, &cfg, Some("Zelda.gba"))), ("retroarch", "gpsp", From::Console));
        assert_eq!(
            run(&resolve(&c, &cfg, None)),
            ("retroarch", "gpsp", From::Console),
            "the console page says so too"
        );
        // And its game options, for one game (the file name, = and # taken out).
        let cfg = Cfg::parse("gba.emulator=retroarch\ngba.core=gpsp\ngba[\"Metroid 1 (USA).gba\"].emulator=mednafen\ngba[\"Metroid 1 (USA).gba\"].core=gba\n");
        assert_eq!(run(&resolve(&c, &cfg, Some("Metroid #1 (USA).gba"))), ("mednafen", "gba", From::Game));
        assert_eq!(run(&resolve(&c, &cfg, Some("Other.gba"))), ("retroarch", "gpsp", From::Console));
        assert_eq!(game_config_name("gba", "A=B #2.gba"), "gba[\"AB 2.gba\"]");
        // AUTO is no line; a stray "auto" is no choice either.
        let cfg = Cfg::parse("gba.emulator=auto\ngba.core=auto\n");
        assert_eq!(run(&resolve(&c, &cfg, Some("Zelda.gba"))).2, From::Default);
    }

    #[test]
    fn choices_are_checked_like_es_checks_them() {
        let gba = emulators("gba");
        let c = ctx("gba", &gba, Some("mgba"));
        let r = |cfg: &str| {
            run(&resolve(&c, &Cfg::parse(cfg), Some("Zelda.gba"))).0.to_string() + "/" + &resolve(&c, &Cfg::parse(cfg), Some("Zelda.gba")).runner.core
        };
        assert_eq!(r("gba.emulator=retroarch\n"), "retroarch/mgba", "an emulator without a core: its default core");
        assert_eq!(
            r("gba.emulator=retroarch\ngba.core=nosuchcore\n"),
            "retroarch/mgba",
            "a core it doesn't have: its default"
        );
        assert_eq!(r("gba.core=vbam\n"), "retroarch/vbam", "a core alone: under the default emulator (ES getCore)");
        assert_eq!(
            r("gba.core=nanoboyadvance-sa\n"),
            "pocket-core/mgba",
            "not a core of the default emulator: nothing chosen"
        );
        assert_eq!(
            r("gba.emulator=nosuchemulator\ngba.core=x\n"),
            "pocket-core/mgba",
            "a name es_systems.cfg doesn't list: ignored, as ES does"
        );
        assert_eq!(r("gba.emulator=skyemu\n"), "skyemu/skyemu-sa");
        // A game naming only an emulator gets that emulator's default core (ES would pair it with the console's).
        assert_eq!(
            r("gba.emulator=retroarch\ngba.core=vbam\ngba[\"Zelda.gba\"].emulator=mednafen\n"),
            "mednafen/gba"
        );
        // A game naming only a core: under the console's emulator.
        assert_eq!(r("gba.emulator=retroarch\ngba.core=vbam\ngba[\"Zelda.gba\"].core=gpsp\n"), "retroarch/gpsp");
        // `.cores` is ROCKNIX's CPU-core key, not a choice.
        assert_eq!(Cfg::choices_only("gba.cores=big\ngba.core=vbam\n").choice_lines(), ["gba.core=vbam"]);
    }

    #[test]
    fn pocket_core_choices_follow_what_pocket_core_can_take() {
        let gba = emulators("gba");
        let cfg = Cfg::parse("gba.emulator=retroarch\ngba.core=vbam\ngba[\"Zelda.gba\"].emulator=pocket-core\ngba[\"Zelda.gba\"].core=mgba\n");
        let c = ctx("gba", &gba, Some("mgba"));
        assert_eq!(
            run(&resolve(&c, &cfg, Some("Zelda.gba"))),
            ("pocket-core", "mgba", From::Game),
            "one game on pocket-core, the console on RetroArch"
        );
        assert_eq!(run(&resolve(&c, &cfg, Some("Other.gba"))), ("retroarch", "vbam", From::Console));
        // The game's pocket-core can't take a 7z: the console's choice.
        let z = Ctx {
            file_ok: false,
            ..ctx("gba", &gba, Some("mgba"))
        };
        assert_eq!(run(&resolve(&z, &cfg, Some("Zelda.gba"))), ("retroarch", "vbam", From::Console));
        // pocket-core not installed (or the system not hosted): a pocket-core line means nothing.
        let none = ctx("gba", &gba, None);
        assert_eq!(run(&resolve(&none, &cfg, Some("Zelda.gba"))), ("retroarch", "vbam", From::Console));
        // pocket-core couldn't start the game before: the next level down, said as such.
        let fb = Ctx {
            fell_back: true,
            ..ctx("gba", &gba, Some("mgba"))
        };
        assert_eq!(run(&resolve(&fb, &Cfg::default(), Some("Zelda.gba"))), ("retroarch", "mgba", From::Fallback));
        let console_pc = Cfg::parse("gba.emulator=pocket-core\ngba.core=mgba\n");
        assert_eq!(run(&resolve(&fb, &console_pc, Some("Zelda.gba"))), ("retroarch", "mgba", From::Fallback));
        assert_eq!(
            run(&resolve(&fb, &cfg, Some("Zelda.gba"))),
            ("retroarch", "vbam", From::Console),
            "a RetroArch choice stays one"
        );
        // What can be picked: pocket-core first (not for a 7z), then es_systems.cfg's pairs.
        let list = choices(&c);
        assert_eq!(list[0], Runner::new("pocket-core", "mgba"));
        assert_eq!(list.len(), 1 + 6 + 3);
        assert_eq!(choices(&z).len(), 9);
    }

    #[test]
    fn resolving_costs_microseconds() {
        // The launch path resolves once per start; it must not cost the start anything measurable (< 1 ms).
        let gba = emulators("gba");
        let mut text = String::new();
        for i in 0..300 {
            text.push_str(&format!("sys{i}.emulator=retroarch\nsys{i}.core=x\ngba[\"Game {i}.gba\"].core=vbam\n"));
        }
        let cfg = Cfg::choices_only(&text);
        let c = ctx("gba", &gba, Some("mgba"));
        let started = std::time::Instant::now();
        for i in 0..1000 {
            let _ = resolve(&c, &cfg, Some(&format!("Game {i}.gba")));
        }
        let each = started.elapsed() / 1000;
        assert!(each < std::time::Duration::from_micros(200), "{each:?} per resolve");
    }

    #[test]
    fn installed_looks_where_runemu_looks() {
        let have = |paths: &'static [&'static str]| move |p: &Path| paths.iter().any(|x| Path::new(x) == p);
        let fs = have(&[
            "/usr/lib/libretro/mgba_libretro.so",
            "/storage/cores/vbam_libretro.so",
            "/usr/bin/start_melonds.sh",
            "/usr/bin/start_mednafen.sh",
            "/usr/bin/start_dolphin_gc.sh",
            "/usr/bin/start_x64sc.sh",
        ]);
        assert!(installed(&Runner::new("retroarch", "mgba"), "gba", fs));
        assert!(installed(&Runner::new("retroarch", "vbam"), "gba", fs), "a core the player added");
        assert!(!installed(&Runner::new("retroarch", "gpsp"), "gba", fs));
        assert!(installed(&Runner::new("melonds", "melonds-sa"), "nds", fs), "start_<core without -sa>.sh");
        assert!(!installed(&Runner::new("drastic", "drastic-sa"), "nds", fs));
        assert!(installed(&Runner::new("mednafen", "gba"), "gba", fs));
        assert!(
            installed(&Runner::new("dolphin", "dolphin-qt-gc"), "gamecube", fs),
            "GameCube: Dolphin's own script"
        );
        assert!(installed(&Runner::new("vicesa", "x64sc"), "c64", fs));
        assert!(installed(&Runner::new("pocket-core", "mgba"), "gba", fs));
    }

    #[test]
    fn labels_say_what_will_run() {
        let gc = emulators("gamecube");
        assert_eq!(label(&Runner::new("pocket-core", "mgba"), &[]), "pocket-core · mGBA");
        assert_eq!(label(&Runner::new("retroarch", "gpsp"), &[]), "RetroArch · gpSP");
        assert_eq!(label(&Runner::new("ppsspp", "ppsspp-sa"), &[]), "Standalone · PPSSPP");
        assert_eq!(label(&Runner::new("dolphin", "dolphin-qt-gc"), &gc), "Standalone · Dolphin (Qt)");
        assert_eq!(label(&Runner::new("dolphin", "dolphin-sa-gc"), &gc), "Standalone · Dolphin (dolphin-sa-gc)");
        assert_eq!(label(&Runner::new("retroarch", "bsnes2014_balanced"), &[]), "RetroArch · bsnes 2014 Balanced");
        assert_eq!(label(&Runner::new("retroarch", "some_new_core"), &[]), "RetroArch · some new core");
        assert!(note(&Runner::new("dolphin", "dolphin-sa-gc"), "gamecube").contains("pocket-ui's Save, Load, Options"));
        assert!(note(&Runner::new("ppsspp", "ppsspp-sa"), "psp").contains("with its own saves"));
        assert!(note(&Runner::new("retroarch", "mgba"), "gba").contains("no pocket-ui slots, Options"));
        // What the Runs on page says is what the pause menu does: no slots for ScummVM, one for Azahar (`pause_saves`).
        assert!(note(&Runner::new(POCKET_CORE, "scummvm"), "scummvm").contains("No save states"));
        assert!(!note(&Runner::new(POCKET_CORE, "scummvm"), "scummvm").contains("eight save slots"));
        assert!(note(&Runner::new(POCKET_CORE, "gambatte"), "gb").contains("eight save slots"));
    }

    #[test]
    fn retroarch_values_are_read_like_get_setting() {
        let cfg = Cfg::parse("global.ratio=core\ngba.ratio=3/2\ngba[\"Zelda.gba\"].ratio=16/9\nsystem.integerscale=1\nrewind=1\nglobal.rewind=0\n");
        let s = |k: &str| RA_SETTINGS.iter().find(|s| s.key == k).unwrap();
        assert_eq!(effective_value(&cfg, "gba", Some("Zelda.gba"), s("ratio")), "16/9");
        assert_eq!(effective_value(&cfg, "gba", Some("Other.gba"), s("ratio")), "3/2");
        assert_eq!(effective_value(&cfg, "snes", None, s("ratio")), "core");
        assert_eq!(effective_value(&cfg, "gba", None, s("integerscale")), "1", "system.<key> before global.");
        assert_eq!(effective_value(&cfg, "gba", None, s("rewind")), "1", "<key> before global.");
        assert_eq!(effective_value(&cfg, "gba", None, s("smooth")), "0", "unset: what setsettings.sh ends up with");
        assert_eq!(s("ratio").value_label("8/7"), "8:7", "a ratio set in ES");
        assert_eq!(s("shaderset").value_label("crt/crt-geom.slangp"), "crt-geom");
        assert!(!s("rewind").applies("n64") && s("rewind").applies("segacd"));
        assert!(!s("runahead").applies("saturn"));
    }

    #[test]
    fn shader_presets_are_found_by_name() {
        let root = TestDir::new("shaders");
        for f in [
            "crt/crt-geom.slangp",
            "crt/shaders/crt-geom/crt-geom.slangp",
            "handheld/lcd-grid-v2.slangp",
            "crt/crt-royale.slangp",
            "zfast_crt_standard.glslp",
            "misc/readme.txt",
        ] {
            let p = root.join(f);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, b"x").unwrap();
        }
        assert_eq!(scan_shaders(&root, &["slangp"]), ["crt/crt-geom.slangp", "handheld/lcd-grid-v2.slangp"]);
        assert_eq!(
            scan_shaders(&root, &["slangp", "glslp"]),
            ["zfast_crt_standard.glslp", "crt/crt-geom.slangp", "handheld/lcd-grid-v2.slangp"]
        );
    }

    /// A stand-in for ROCKNIX's 001-functions with its `wait_lock` (same lock protocol, test paths).
    const FUNCTIONS_STUB: &str = r#"J_CONF=/nonexistent/system.cfg
J_CONF_LOCK=/nonexistent/lock
wait_lock() {
  while true; do
    if (set -o noclobber; echo "$$" > "${J_CONF_LOCK}") 2>/dev/null; then
      trap 'rc=$?; [ "$(cat "${J_CONF_LOCK}" 2>/dev/null)" = "$$" ] && rm -f "${J_CONF_LOCK}"; exit "${rc}"' INT TERM EXIT
      break
    else
      sleep 1
    fi
  done
}
"#;

    #[test]
    fn system_cfg_round_trip_keeps_everything_else_and_reads_back() {
        let dir = TestDir::new("syscfg");
        let (funcs, file, lock) = (dir.join("001-functions"), dir.join("system.cfg"), dir.join("lock"));
        std::fs::write(&funcs, FUNCTIONS_STUB).unwrap();
        let before = "# a comment\naudio.volume=60\ngba.ratio=3/2\ngba[\"Zelda (USA) [!].gba\"].core=gpsp\ngba[\"Zelda (USA) [!].gba\"].emulator=retroarch\nwifi.enabled=1\n";
        std::fs::write(&file, before).unwrap();
        let game = game_config_name("gba", "Zelda (USA) [!].gba");
        let edits = vec![
            (format!("{game}.core"), Some("vbam".to_string())),
            (format!("{game}.emulator"), Some("retroarch".to_string())),
            ("gba.ratio".to_string(), None),
            ("gba.emulator".to_string(), Some("pocket-core".to_string())),
            ("gba.core".to_string(), Some("mgba".to_string())),
        ];
        write_system_cfg(&edits, Some(&funcs), Some(&file), Some(&lock)).unwrap();
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(!lock.exists(), "the lock is released");
        assert_eq!(
            text.matches("].core=").count(),
            1,
            "the per-game line is replaced, not doubled (set_setting's sed would): {text}"
        );
        assert!(
            text.starts_with("# a comment\naudio.volume=60\n") && text.contains("wifi.enabled=1\n"),
            "{text}"
        );
        assert!(!text.contains("gba.ratio"));
        let cfg = Cfg::parse(&text);
        assert_eq!(cfg.get(&format!("{game}.core")), Some("vbam"));
        assert_eq!(cfg.get("gba.emulator"), Some("pocket-core"));
        // ROCKNIX's get_setting (as emu_settings::es_get reads it) and ES (Cfg) agree.
        assert_eq!(
            crate::emu_settings::es_get(&text, "gba", "Zelda (USA) [!].gba", "core").as_deref(),
            Some("vbam")
        );
        // And the cache gets the same edits.
        let mut cached = Cfg::choices_only(before);
        cached.apply(&edits);
        assert_eq!(cached.choice_lines(), Cfg::choices_only(&text).choice_lines());
        // A held lock is waited for, not broken.
        std::fs::write(&lock, "1").unwrap();
        let (l2, f2, fu2) = (lock.clone(), file.clone(), funcs.clone());
        let writer = std::thread::spawn(move || write_system_cfg(&[("x.y".into(), Some("1".into()))], Some(&fu2), Some(&f2), Some(&l2)));
        std::thread::sleep(std::time::Duration::from_millis(300));
        assert!(
            !std::fs::read_to_string(&file).unwrap().contains("x.y=1"),
            "waits while the lock is someone else's"
        );
        std::fs::remove_file(&lock).unwrap();
        writer.join().unwrap().unwrap();
        assert!(std::fs::read_to_string(&file).unwrap().contains("x.y=1\n"));
        assert!(write_system_cfg(&[("a\nb".into(), Some("1".into()))], Some(&funcs), Some(&file), Some(&lock)).is_err());
    }
}
