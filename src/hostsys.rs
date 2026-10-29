//! The systems pocket-core can host besides the handhelds it started with: which libretro core plays
//! each ROCKNIX system, which BIOS files that needs, which pad it presents and which core options
//! differ from the core's own defaults.
//!
//! `verified` says whether a system has been played through on the device and measured against
//! RetroArch (see docs/HOST.md), or, for the systems added on 2026-10-23 (`VERIFIED_2026_10_23`), checked in the
//! virtual device with homebrew and self-made test ROMs and still to be confirmed on the Mini V2. pocket-ui only
//! routes verified systems to pocket-core (`host_core`, `route`); the others stay on their default emulator until
//! they have been checked.

use std::path::Path;

/// ROCKNIX's BIOS folder, the system directory pocket-core gives the cores.
pub const BIOS_DIR: &str = "/storage/roms/bios";

/// One BIOS requirement: any one of `files` (in the bios folder) satisfies it.
pub struct Bios {
    pub label: &'static str,
    pub files: &'static [&'static str],
}

pub struct HostSystem {
    /// ROCKNIX system names that share this core and setup (they are also the ROM folder names).
    pub names: &'static [&'static str],
    /// libretro core, `<core>_libretro.so` in /usr/lib/libretro.
    pub core: &'static str,
    /// BIOS files the game cannot start without.
    pub bios: &'static [Bios],
    /// DualShock-style pad: the sticks go to the core. Otherwise the left stick acts as the D-pad.
    pub analog: bool,
    /// Core options that differ from the core's own defaults.
    pub options: &'static [(&'static str, &'static str)],
    /// Extension of the battery save next to the ROM: RetroArch's `srm`, or the default emulator's where
    /// that differs, so switching between the two keeps the save (standalone melonDS: `.sav`).
    pub save_ext: &'static str,
    pub verified: bool,
    /// The system's default emulator plays it without the BIOS (standalone YabaSanshiro's HLE BIOS): with
    /// the BIOS missing, pocket-ui leaves the game there (`route`) instead of showing the BIOS message.
    pub default_without_bios: bool,
    /// The console's button names by RetroPad id (what the Mini V2 button of that id presses), for the pause
    /// menu's Controls page when the core names none (`controls::targets_for`). Empty: the standard names.
    pub buttons: &'static [(u8, &'static str)],
    /// What the pause menu says instead of saving, for a core without save states (ScummVM: the game's own saves).
    pub no_states: Option<&'static str>,
    /// The core's state size grows while a game runs and it reads its own length, without saying so
    /// (SET_SERIALIZATION_QUIRKS): a saved state shorter than the size now is fine (DOSBox Pure).
    pub states_vary: bool,
    /// Extensions of the files pocket-core takes; the others the shelf lists stay with the default emulator
    /// (ROCKNIX's `.sh` scripts in ScummVM's list). Empty: all of them.
    pub exts: &'static [&'static str],
    /// Extensions the shelf lists besides es_systems.cfg's (DOSBox Pure's zips, which ROCKNIX leaves out).
    pub extra_exts: &'static [&'static str],
    /// Folders that are games: DOS games unpacked into a folder, ScummVM games not scanned by ROCKNIX yet.
    pub folders: Folders,
    /// The system's default emulator is a program the user adds (the official PICO-8): while it is there
    /// pocket-ui leaves the game with it (`route`), otherwise `Official::otherwise` stands in as the default.
    pub official: Option<Official>,
}

/// Where a system's game folders are (`HostSystem::folders`).
pub enum Folders {
    No,
    /// Folders in the system's ROM folder (roms/pc).
    Here,
    /// Folders in this folder of `/storage/roms` (ScummVM's list lives in /storage/.config, its games in roms/scummvm).
    Under(&'static str),
}

/// A default emulator the user installs (`HostSystem::official`).
pub struct Official {
    /// Whether it is installed, given `/storage/roms`.
    pub installed: fn(&Path) -> bool,
    /// The emulator and core (as in es_systems.cfg) that stand in for it when it isn't.
    pub otherwise: (&'static str, &'static str),
}

const NO_BIOS: &[Bios] = &[];
const NO_OPTS: &[(&str, &str)] = &[];

const SEGA_CD_BIOS: &[Bios] = &[Bios {
    label: "Sega CD",
    files: &["bios_CD_U.bin", "bios_CD_E.bin", "bios_CD_J.bin"],
}];
const PCE_CD_BIOS: &[Bios] = &[Bios {
    label: "PC Engine CD",
    files: &["syscard3.pce", "syscard2.pce", "syscard1.pce", "gexpress.pce"],
}];

const SATURN_BIOS: &[Bios] = &[Bios {
    label: "Saturn",
    files: &["saturn_bios.bin", "sega_101.bin", "mpr-17933.bin"],
}];

/// Nestopia reads the Disk System's BIOS from roms/bios itself (its info file: `disksys.rom`, md5
/// ca30b50f880eb660a320674ed365ef7a); without it no disk boots, in RetroArch either.
const FDS_BIOS: &[Bios] = &[Bios {
    label: "Famicom Disk System",
    files: &["disksys.rom"],
}];

/// Flycast reads the arcade BIOS sets (MAME's zips) from `bios/dc`, the folder ROCKNIX's standalone Flycast uses as
/// its data folder too (start_flycast.sh links roms/dreamcast/data to it). NAOMI 2 games want naomi2.zip; a few
/// boards have their own set as well (hod2bios.zip, f355bios.zip, airlbios.zip), which Flycast names when it is missing.
const NAOMI_BIOS: &[Bios] = &[Bios {
    label: "NAOMI",
    files: &["dc/naomi.zip", "dc/naomi2.zip"],
}];
const ATOMISWAVE_BIOS: &[Bios] = &[Bios {
    label: "Atomiswave",
    files: &["dc/awbios.zip"],
}];

/// The Saturn pad as yabasanshiro lays it on the RetroPad (RetroArch's layout too): bottom row A B C on
/// B, A, L1; top row X Y Z on Y, X, R1; the shoulder buttons L and R on L2 / R2.
const SATURN_PAD: &[(u8, &str)] = &[(0, "A"), (8, "B"), (10, "C"), (1, "X"), (9, "Y"), (11, "Z"), (12, "L"), (13, "R"), (3, "Start")];

const fn system(names: &'static [&'static str], core: &'static str, bios: &'static [Bios], analog: bool, verified: bool) -> HostSystem {
    HostSystem {
        names,
        core,
        bios,
        analog,
        options: NO_OPTS,
        save_ext: "srm",
        verified,
        default_without_bios: false,
        buttons: &[],
        no_states: None,
        states_vary: false,
        exts: &[],
        extra_exts: &[],
        folders: Folders::No,
        official: None,
    }
}

pub const SYSTEMS: &[HostSystem] = &[
    // Played on the device with a real game (Old Towers, Mega Drive homebrew) and measured.
    system(
        &["megadrive", "megadrive-japan", "genesis", "genh", "megadriveh"],
        "genesis_plus_gx",
        NO_BIOS,
        false,
        true,
    ),
    // ROCKNIX's default core for these is gearsystem. Checked with a self-made test ROM (scripts/testroms.py).
    system(&["mastersystem", "gamegear", "ggh", "sg-1000"], "gearsystem", NO_BIOS, false, true),
    // Needs a Sega CD BIOS; checked only as far as the missing-BIOS message.
    system(&["segacd", "megacd"], "genesis_plus_gx", SEGA_CD_BIOS, false, false),
    system(&["sega32x"], "picodrive", NO_BIOS, false, false),
    // Real games: Reflectron and Santatlantean.
    system(&["pcengine", "tg16"], "beetle_pce_fast", NO_BIOS, false, true),
    system(&["pcenginecd", "tg16cd"], "beetle_pce_fast", PCE_CD_BIOS, false, false),
    system(&["ngp", "ngpc"], "beetle_ngp", NO_BIOS, false, false),
    // Self-made test ROM.
    system(&["wonderswan", "wonderswancolor"], "beetle_wswan", NO_BIOS, false, true),
    system(&["atarilynx"], "handy", NO_BIOS, false, false),
    // Self-made test ROM.
    system(&["atari2600"], "stella", NO_BIOS, false, true),
    system(&["atari7800"], "prosystem", NO_BIOS, false, false),
    system(&["pokemini"], "pokemini", NO_BIOS, false, false),
    // Real game (Crash Bandicoot 2, PAL). DualShock pad.
    system(&["psx"], "pcsx_rearmed", NO_BIOS, true, true),
    system(&["neogeo", "cps1", "cps2", "cps3", "fbn"], "fbneo", NO_BIOS, false, false),
    // 3D and dual-screen systems, measured against ROCKNIX's default emulator (docs/HOST.md).
    // N64 (default RetroArch): Mario Kart 64, less CPU and steadier frames than RetroArch.
    system(&["n64"], "mupen64plus_next", NO_BIOS, true, true),
    // PSP (default standalone PPSSPP, Vulkan): Bomberman, same CPU, 60 fps either way. Both use roms/psp as
    // the memory stick, so game saves are shared.
    system(&["psp"], "ppsspp", NO_BIOS, true, true),
    // Dreamcast (default standalone Flycast, Vulkan): Sonic Adventure 2, same CPU, far steadier frames.
    // Boots without bios/dc/dc_boot.bin too (Flycast's HLE BIOS). Memory cards shared (vfs.rs).
    system(&["dreamcast"], "flycast", NO_BIOS, true, true),
    // NDS (default standalone melonDS): New Super Mario Bros., a third less CPU, both screens stacked
    // and larger. Touch: right stick moves the pointer, R2 touches. Saves as melonDS's `.sav`.
    HostSystem {
        save_ext: "sav",
        ..system(&["nds"], "melondsds", NO_BIOS, false, true)
    },
    // Saturn (default standalone YabaSanshiro): yabasanshiro, the same emulator as a libretro core. Checked with
    // Twelve Snakes (homebrew) in HLE mode, as there is no Saturn BIOS on the device (docs/HOST.md). The core
    // refuses to start without a BIOS file; without one the game stays on the standalone emulator (`route`).
    HostSystem {
        default_without_bios: true,
        buttons: SATURN_PAD,
        ..system(&["saturn"], "yabasanshiro", SATURN_BIOS, false, true)
    },
    // DOS (default RetroArch with the same core). Folders and zips are games too; the touch screen is DOSBox
    // Pure's touchpad (drag moves, tap clicks), L3 its on-screen keyboard (docs/HOST.md "DOS, ScummVM, PICO-8, TIC-80").
    HostSystem {
        options: DOS_OPTS,
        extra_exts: &["zip"],
        folders: Folders::Here,
        states_vary: true,
        ..system(&["pc"], "dosbox_pure", NO_BIOS, false, true)
    },
    // ScummVM (default standalone ScummVM). ROCKNIX's list is /storage/.config/scummvm/games, written by its
    // "Scan ScummVM Games" tool; folders in roms/scummvm play without the scan. The core has no save states.
    HostSystem {
        options: SCUMMVM_OPTS,
        no_states: Some("No save states for ScummVM: START opens its own Save and Load"),
        exts: &["scummvm", "svm"],
        folders: Folders::Under("scummvm"),
        ..system(&["scummvm"], "scummvm", NO_BIOS, false, true)
    },
    // PICO-8: fake08, unless the official PICO-8 is in roms/pico-8 (ROCKNIX's start_pico8.sh then, as before).
    // ROCKNIX's fake08 writes its states without the cart's Lua state (its size field is 0, any cart): a load puts
    // the PICO-8 memory back and nothing of the game. So no slots and no resume, as for TIC-80.
    HostSystem {
        official: Some(Official {
            installed: pico8_installed,
            otherwise: ("retroarch", "fake08"),
        }),
        no_states: Some("No save states for PICO-8: carts keep their own saves"),
        ..system(&["pico-8"], "fake08", NO_BIOS, false, true)
    },
    // TIC-80 (default RetroArch with the same core).
    // Its states hold only the cart's 1 KB of persistent memory, not the game: a load changes nothing on screen.
    // So no slots and no resume (docs/HOST.md); a game's own saves are that memory.
    HostSystem {
        options: TIC80_OPTS,
        no_states: Some("No save states for TIC-80: games keep their own saves"),
        ..system(&["tic-80"], "tic80", NO_BIOS, false, true)
    },
    // ROCKNIX's second folders for the consoles of main.rs `HOST_CORES` (Famicom, the "hacks" folders, Super Famicom,
    // GBA Video): the same default core, so the same setup as roms/nes, roms/snes, roms/gb and roms/gba.
    system(&["famicom", "nesh"], "nestopia", NO_BIOS, false, VERIFIED_2026_10_23),
    system(&["sfc", "snesh"], "snes9x", NO_BIOS, false, VERIFIED_2026_10_23),
    system(&["gbh", "gbch"], "gambatte", NO_BIOS, false, VERIFIED_2026_10_23),
    system(&["gbah", "gbav"], "mgba", NO_BIOS, false, VERIFIED_2026_10_23),
    // Famicom Disk System (default RetroArch with the same core): Nestopia, as for the NES, with disksys.rom. Its input
    // descriptors name L1 "(FDS) Disk Side Change" and R1 "(FDS) Eject Disk". Its states vary in size like the NES's
    // (`core_states_vary`).
    system(&["fds"], "nestopia", FDS_BIOS, false, VERIFIED_2026_10_23),
    // SuperGrafx (default RetroArch with the same core): HuCards (.pce, .sgx). CD games (.cue, .ccd, .chd) need a
    // System Card and stay with RetroArch, like roms/pcenginecd.
    HostSystem {
        exts: &["pce", "sgx", "zip"],
        ..system(&["supergrafx"], "beetle_supergrafx", NO_BIOS, false, VERIFIED_2026_10_23)
    },
    // Virtual Boy (default RetroArch with the same core): one eye's picture in red on black, the core's default
    // (`vb_3dmode` anaglyph without a colour preset). The right stick works the right D-pad (`VB_OPTS`).
    HostSystem {
        options: VB_OPTS,
        ..system(&["virtualboy"], "beetle_vb", NO_BIOS, false, VERIFIED_2026_10_23)
    },
    // NAOMI and Atomiswave (default standalone Flycast): the Dreamcast's core, with MAME's BIOS sets in bios/dc. Not
    // verified: no BIOS here to boot one (docs/HOST.md "NAOMI and Atomiswave"); POCKET_UI_HOST_ALL=1 routes them.
    system(&["naomi"], "flycast", NAOMI_BIOS, true, false),
    system(&["atomiswave"], "flycast", ATOMISWAVE_BIOS, true, false),
];

/// The systems checked in the virtual device on 2026-10-23 (docs/HOST.md "More systems"). One switch so that a
/// finding on the Mini V2 can send them all back to their default emulator at once.
const VERIFIED_2026_10_23: bool = true;

/// Virtual Boy: the right stick works the right D-pad (otherwise on L2 / R2 / L3 / R3, beetle_vb's layout).
const VB_OPTS: &[(&str, &str)] = &[("vb_right_analog_to_digital", "enabled")];

/// DOSBox Pure: the touch screen as a touchpad (drag to move, tap to click, two-finger tap right-clicks), L3 opens
/// the on-screen keyboard (its menu for disks and the pad mapper is a tab away), 4:3 as a CRT showed it.
const DOS_OPTS: &[(&str, &str)] = &[
    ("dosbox_pure_mouse_input", "pad"),
    ("dosbox_pure_on_screen_keyboard", "keyboard"),
    ("dosbox_pure_aspect_correction", "true"),
];

/// ScummVM: a tap moves the cursor there and clicks; A (right) clicks, B (bottom) right-clicks, L1 skips a line of
/// dialogue ("."). The core's own layout otherwise: sticks and D-pad move the cursor, X F5 (game menu), Y Escape,
/// Select its keyboard, Start ScummVM's menu (save, load), R1 right click, R2 slow cursor.
const SCUMMVM_OPTS: &[(&str, &str)] = &[
    ("scummvm_pointer_device", "pointer"),
    ("scummvm_mapper_a", "RETROKE_LEFT_BUTTON"),
    ("scummvm_mapper_b", "RETROKE_RIGHT_BUTTON"),
    ("scummvm_mapper_l", "RETROK_PERIOD"),
];

/// TIC-80: the touch screen is its mouse (absolute), not the relative mouse the core reads by default.
const TIC80_OPTS: &[(&str, &str)] = &[("tic80_pointer_device", "touchscreen")];

/// ROCKNIX's start_pico8.sh runs `roms/pico-8/aarch64/pico8_64` when that folder exists, else `roms/pico-8/pico8_64`.
fn pico8_installed(roms: &Path) -> bool {
    let dir = roms.join("pico-8");
    let arch = dir.join("aarch64");
    if arch.is_dir() { arch } else { dir }.join("pico8_64").is_file()
}

/// Cores whose state size changes while a game runs, wherever pocket-ui routes them (the HOST_CORES systems such as
/// GBA have no `HostSystem`). mGBA measures `retro_serialize_size` by writing a state, and the save data in it grows
/// once the game touches it: a resume state written at quit was refused at the next start as "wrong size (430144 of
/// 528448 bytes)" (F-Zero: Maximum Velocity on the device, 2026-10-23), so GBA games never resumed. mGBA reads
/// states through a memory file of the given length, so a shorter one is safe to hand over.
/// Nestopia's states are 9 bytes longer or shorter from one moment to the next (NES and Disk System alike): the
/// state written at quit was refused as "wrong size (46370 of 46379 bytes)" at the next start in one of two runs
/// (Nova the Squirrel, virtual device, 2026-10-23; FDS: 177462 of 177471). Nestopia reads a state from a stream of
/// the given length, so a shorter one is safe too.
pub fn core_states_vary(core_id: &str) -> bool {
    matches!(core_id, "mgba" | "nestopia")
}

/// The systems pocket-core started with (the handhelds and the two Nintendo home consoles) and their cores: pocket-ui's
/// `HOST_CORES` (src/main.rs) is this list.
pub const HANDHELD_CORES: &[(&str, &str)] = &[("gb", "gambatte"), ("gbc", "gambatte"), ("gba", "mgba"), ("nes", "nestopia"), ("snes", "snes9x")];

/// Every core pocket-ui starts games with on pocket-core (`HANDHELD_CORES` and the verified `SYSTEMS`), each with the
/// systems it plays: what an OS update must still have (`os_compat`).
pub fn routed_cores() -> Vec<(&'static str, Vec<&'static str>)> {
    let mut out: Vec<(&'static str, Vec<&'static str>)> = Vec::new();
    let all = HANDHELD_CORES.iter().map(|(s, c)| (*c, vec![*s])).chain(SYSTEMS.iter().filter(|s| s.verified).map(|s| (s.core, s.names.to_vec())));
    for (core, systems) in all {
        match out.iter_mut().find(|(c, _)| *c == core) {
            Some((_, have)) => have.extend(systems.into_iter().filter(|s| !have.contains(s)).collect::<Vec<_>>()),
            None => out.push((core, systems)),
        }
    }
    out
}

/// The setup for a ROCKNIX system, verified or not.
pub fn find(system: &str) -> Option<&'static HostSystem> {
    SYSTEMS.iter().find(|s| s.names.contains(&system))
}

/// The core pocket-ui should launch a system with in pocket-core, if the system has been verified.
/// POCKET_UI_HOST_ALL=1 (testing) also routes the systems that are wired but not verified yet.
pub fn host_core(system: &str) -> Option<&'static str> {
    let all = std::env::var_os("POCKET_UI_HOST_ALL").is_some();
    find(system).filter(|s| s.verified || all).map(|s| s.core)
}

/// `host_core`, except for a system whose default emulator does without the BIOS that pocket-core's core
/// needs: with that BIOS missing from `bios_dir` the game stays on the default emulator.
/// The official PICO-8 in `roms/pico-8` keeps its games as well (`HostSystem::official`). `bios_dir` is `<roms>/bios`.
pub fn route(system: &str, bios_dir: &Path) -> Option<&'static str> {
    route_on(system, bios_dir, crate::device::current().model.is_some_and(|m| m.is_h700()))
}

/// Systems whose pocket-core cores are far heavier than an H700's four Cortex-A53 allow (docs/DEVICES.md): on those
/// handhelds they stay on ROCKNIX's own emulators (DraStic for NDS and so on), as an unverified system does.
pub const H700_STAYS_ON_ROCKNIX: &[&str] = &["n64", "psp", "dreamcast", "nds"];

/// `route` for an H700 handheld (`h700`) or any other device.
pub fn route_on(system: &str, bios_dir: &Path, h700: bool) -> Option<&'static str> {
    if h700 && H700_STAYS_ON_ROCKNIX.contains(&system) {
        return None;
    }
    let core = host_core(system)?;
    let s = find(system)?;
    let without_bios = s.default_without_bios && s.missing_bios(bios_dir).is_some();
    let official = s.official.as_ref().is_some_and(|o| (o.installed)(roms_of(bios_dir)));
    (!without_bios && !official).then_some(core)
}

/// `/storage/roms` for its bios folder.
fn roms_of(bios_dir: &Path) -> &Path {
    bios_dir.parent().unwrap_or(bios_dir)
}

/// The emulator and core that play a game pocket-core doesn't (or can't), when es_systems.cfg's default can't:
/// without the official PICO-8, start_pico8.sh has nothing to start, so RetroArch's fake08 does; Xbox games go to hakuX
/// once its core is installed. None: the default.
pub fn default_emulator(system: &str, bios_dir: &Path) -> Option<(&'static str, &'static str)> {
    // Xbox: ROCKNIX's xemu doesn't run on this hardware; with the hakuX core installed (`hakux`) it plays, in RetroArch.
    if system == "xbox" && crate::hakux::installed() {
        return Some(("hakux", "hakux"));
    }
    let o = find(system)?.official.as_ref()?;
    (!(o.installed)(roms_of(bios_dir))).then_some(o.otherwise)
}

/// What pocket-core hands the core for `rom`. ROCKNIX's ScummVM list holds standalone ScummVM's arguments
/// (`--path="/storage/roms/scummvm/FOTAQ" queen`, see start_scummvm.sh), which the libretro core can't read:
/// it gets the game folder, where it finds the game itself. Everything else goes as it is.
pub fn content_path(system: &str, rom: &Path) -> std::path::PathBuf {
    if system == "scummvm" && rom.is_file() {
        if let Some(dir) = std::fs::read_to_string(rom).ok().as_deref().and_then(scummvm_path) {
            return dir.into();
        }
    }
    rom.to_path_buf()
}

/// The `--path="..."` of a ROCKNIX `.scummvm` file.
fn scummvm_path(text: &str) -> Option<&str> {
    let rest = &text[text.find("--path=")? + 7..];
    match rest.strip_prefix('"') {
        Some(quoted) => quoted.split('"').next(),
        None => rest.split_whitespace().next(),
    }
    .filter(|p| !p.is_empty())
}

/// Where standalone ScummVM keeps its saves on ROCKNIX (its default, `$HOME/.local/share/scummvm/saves`; ROCKNIX's
/// scummvm.ini sets none). The core saves there too, so a game saved in one continues in the other.
pub const SCUMMVM_SAVES: &str = "/storage/.local/share/scummvm/saves";
/// ROCKNIX's ScummVM data: engine files (BASS's sky.cpt, the virtual keyboard) and the GUI themes.
const SCUMMVM_DATA: &str = "/usr/share/scummvm";

/// The ScummVM core reads `scummvm.ini` from the BIOS folder and looks for its engine data only in
/// `bios/scummvm/extra`, which ROCKNIX doesn't have. Returns the ini with `extrapath` and `themepath` pointing at
/// ROCKNIX's copy and `savepath` at standalone ScummVM's saves wherever they are missing or name no folder that
/// `exists`; None when nothing needs changing. Other keys and the game sections are kept as they are.
pub fn scummvm_ini(text: &str, exists: impl Fn(&Path) -> bool) -> Option<String> {
    let wanted = [("extrapath", SCUMMVM_DATA), ("themepath", SCUMMVM_DATA), ("savepath", SCUMMVM_SAVES)];
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    let start = match lines.iter().position(|l| l.trim() == "[scummvm]") {
        Some(i) => i,
        None => {
            lines.insert(0, "[scummvm]".into());
            0
        }
    };
    let end = lines[start + 1..]
        .iter()
        .position(|l| l.trim_start().starts_with('['))
        .map_or(lines.len(), |i| start + 1 + i);
    let mut changed = !text.contains("[scummvm]");
    let mut add = Vec::new();
    for (key, value) in wanted {
        let at = (start + 1..end).find(|&i| lines[i].split_once('=').is_some_and(|(k, _)| k.trim() == key));
        let good = at.is_some_and(|i| {
            lines[i]
                .split_once('=')
                .is_some_and(|(_, v)| !v.trim().is_empty() && exists(Path::new(v.trim())))
        });
        if !good {
            changed = true;
            if let Some(i) = at {
                lines[i] = format!("{key}={value}");
            } else {
                add.push(format!("{key}={value}"));
            }
        }
    }
    if !changed {
        return None;
    }
    for (n, line) in add.into_iter().enumerate() {
        lines.insert(start + 1 + n, line);
    }
    Some(lines.join("\n") + "\n")
}

/// `scummvm_ini` on `<bios_dir>/scummvm.ini`, creating standalone ScummVM's save folder when it is missing.
pub fn prepare_scummvm(bios_dir: &Path) {
    let _ = std::fs::create_dir_all(SCUMMVM_SAVES);
    let ini = bios_dir.join("scummvm.ini");
    let text = std::fs::read_to_string(&ini).unwrap_or_default();
    if let Some(new) = scummvm_ini(&text, |p| p.is_dir()) {
        if let Err(e) = crate::fsutil::write_atomic(&ini, new.as_bytes()) {
            eprintln!("{}: {e}", ini.display());
        }
    }
}

/// Whether pocket-core takes this file of the system (`HostSystem::exts`); folders always.
pub fn hosts_file(system: &str, path: &Path) -> bool {
    let Some(s) = find(system) else { return true };
    let ext = path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase);
    s.exts.is_empty() || ext.is_none() || path.is_dir() || ext.is_some_and(|e| s.exts.contains(&e.as_str()))
}

/// Extensions the shelf lists for the system besides es_systems.cfg's.
pub fn shelf_extensions(system: &str) -> &'static [&'static str] {
    find(system).map_or(&[], |s| s.extra_exts)
}

/// The aspect ratio the picture is shown at: what the core reports (`reported`, width / height), except that a ScummVM
/// game of 200 or 400 lines (320x200, 640x400: the VGA adventures) is 4:3. Its core reports the game's own pixels
/// (1.6) and draws the picture at that shape, but those games were made for monitors that stretched 200 lines to 240;
/// DOSBox Pure's `dosbox_pure_aspect_correction` is the same thing (`DOS_OPTS`). The launcher (1280x720) and the
/// higher resolutions are left as they are.
pub fn display_aspect(system: &str, base_w: u32, base_h: u32, reported: f32) -> f32 {
    if system == "scummvm" && matches!((base_w, base_h), (320, 200) | (640, 400)) {
        return 4.0 / 3.0;
    }
    reported
}

/// Files with a listed extension that are no game: DOSBox Pure's own saves (`<game>.pure.zip`, next to the game).
pub fn not_a_game(system: &str, path: &Path) -> bool {
    system == "pc"
        && path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.to_ascii_lowercase().ends_with(".pure.zip"))
}

/// Whether the folders of this system are games that pocket-core starts (`folder_games`), so the shelf must not also
/// look into them for files.
pub fn folders_are_games(system: &str) -> bool {
    find(system).is_some_and(|s| !matches!(s.folders, Folders::No))
}

/// Game folders of a system pocket-core hosts, sorted: the folders in `shelf` (its ROM folder) or in a folder of
/// `roms`, without hidden ones and without those a file in `listed` already starts (a ROCKNIX `.scummvm` file).
pub fn folder_games(system: &str, shelf: &Path, roms: &Path, listed: &[std::path::PathBuf]) -> Vec<std::path::PathBuf> {
    let Some(s) = find(system).filter(|_| host_core(system).is_some()) else {
        return Vec::new();
    };
    let root = match s.folders {
        Folders::No => return Vec::new(),
        Folders::Here => shelf.to_path_buf(),
        Folders::Under(dir) => roms.join(dir),
    };
    let started: Vec<std::path::PathBuf> = listed.iter().map(|f| content_path(system, f)).collect();
    let mut dirs: Vec<std::path::PathBuf> = std::fs::read_dir(&root)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir() && !p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with('.')))
        .filter(|p| !started.contains(p))
        .collect();
    dirs.sort();
    dirs
}

impl HostSystem {
    /// A message naming the missing BIOS file(s), or None when everything required is in `bios_dir`.
    pub fn missing_bios(&self, bios_dir: &Path) -> Option<String> {
        let missing: Vec<String> = self
            .bios
            .iter()
            .filter(|b| !b.files.iter().any(|f| bios_dir.join(f).is_file()))
            .map(|b| {
                let (last, rest) = b.files.split_last().unwrap();
                let names = if rest.is_empty() {
                    last.to_string()
                } else {
                    format!("{} or {last}", rest.join(", "))
                };
                format!("{} needs a BIOS file in roms/bios: {names}", b.label)
            })
            .collect();
        (!missing.is_empty()).then(|| missing.join("; "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;

    #[test]
    fn an_h700_leaves_the_heavy_systems_to_rocknix() {
        let bios = Path::new("/nonexistent/bios");
        for system in H700_STAYS_ON_ROCKNIX {
            assert!(find(system).is_some_and(|s| s.verified), "{system} is routed elsewhere, so the list matters");
            assert!(route_on(system, bios, false).is_some(), "{system} on another device");
            assert_eq!(route_on(system, bios, true), None, "{system} on an H700");
        }
        for light in ["gb", "gba", "snes", "nes", "megadrive"] {
            assert_eq!(route_on(light, bios, true), route_on(light, bios, false), "{light}");
        }
    }

    #[test]
    fn routed_cores_are_the_handhelds_and_the_verified_systems_once_each() {
        let cores = routed_cores();
        // The GB/GBC hack folders (feat/more-systems) play on the same core.
        assert_eq!(cores.iter().find(|(c, _)| *c == "gambatte").unwrap().1, ["gb", "gbc", "gbh", "gbch"]);
        let gpgx = &cores.iter().find(|(c, _)| *c == "genesis_plus_gx").unwrap().1;
        assert!(gpgx.contains(&"megadrive") && !gpgx.contains(&"segacd"), "Sega CD is not verified: {gpgx:?}");
        assert!(!cores.iter().any(|(c, _)| *c == "picodrive"), "32X is not verified");
        let mut names: Vec<&str> = cores.iter().map(|(c, _)| *c).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), cores.len(), "each core once");
    }

    #[test]
    fn names_are_unique_across_systems() {
        let mut seen = std::collections::HashSet::new();
        for s in SYSTEMS {
            for n in s.names {
                assert!(seen.insert(*n), "{n} listed twice");
            }
        }
    }

    #[test]
    fn missing_bios_names_the_files() {
        let dir = TestDir::new("bios");
        let cd = find("segacd").unwrap();
        let msg = cd.missing_bios(&dir).unwrap();
        assert_eq!(msg, "Sega CD needs a BIOS file in roms/bios: bios_CD_U.bin, bios_CD_E.bin or bios_CD_J.bin");
        std::fs::write(dir.join("bios_CD_E.bin"), b"x").unwrap();
        assert!(cd.missing_bios(&dir).is_none(), "any one region's BIOS is enough");
        assert!(find("megadrive").unwrap().missing_bios(&dir).is_none());
    }

    #[test]
    fn only_verified_systems_are_routed() {
        std::env::remove_var("POCKET_UI_HOST_ALL");
        assert_eq!(host_core("nonexistent"), None);
        for s in SYSTEMS {
            for n in s.names {
                assert_eq!(host_core(n).is_some(), s.verified, "{n}");
            }
        }
        assert_eq!(host_core("megadrive"), Some("genesis_plus_gx"));
        assert_eq!(host_core("segacd"), None, "no BIOS to check with: stays on RetroArch");
        assert_eq!(host_core("nds"), Some("melondsds"));
        assert_eq!(find("nds").unwrap().save_ext, "sav", "shared with standalone melonDS");
        assert_eq!(find("psp").unwrap().save_ext, "srm");
    }

    fn bios_dir(name: &str) -> crate::testutil::TestDir {
        TestDir::new(&format!("bios-{name}"))
    }

    #[test]
    fn saturn_is_hosted_by_yabasanshiro() {
        let s = find("saturn").unwrap();
        assert_eq!(s.core, "yabasanshiro", "GLES/GL renderer and an ARM64 dynarec; beetle_saturn is software only, kronos wants GL 4.3");
        assert!(s.verified);
        assert!(!s.analog, "the Saturn pad is digital: the left stick works the D-pad");
        assert_eq!(s.save_ext, "srm");
        assert!(s.default_without_bios, "standalone YabaSanshiro boots without a BIOS (HLE)");
    }

    #[test]
    fn saturn_bios_check_names_the_files() {
        let dir = bios_dir("saturn");
        let s = find("saturn").unwrap();
        assert_eq!(
            s.missing_bios(&dir).unwrap(),
            "Saturn needs a BIOS file in roms/bios: saturn_bios.bin, sega_101.bin or mpr-17933.bin"
        );
        // yabasanshiro looks for these three, in this order, in roms/bios itself.
        for f in ["saturn_bios.bin", "sega_101.bin", "mpr-17933.bin"] {
            std::fs::write(dir.join(f), b"x").unwrap();
            assert!(s.missing_bios(&dir).is_none(), "{f} is enough");
            std::fs::remove_file(dir.join(f)).unwrap();
        }
        // Kronos keeps its copy in a subfolder; yabasanshiro doesn't look there.
        std::fs::create_dir_all(dir.join("kronos")).unwrap();
        std::fs::write(dir.join("kronos/saturn_bios.bin"), b"x").unwrap();
        assert!(s.missing_bios(&dir).is_some());
    }

    #[test]
    fn saturn_without_bios_stays_on_its_default_emulator() {
        std::env::remove_var("POCKET_UI_HOST_ALL");
        let dir = bios_dir("route");
        assert_eq!(route("saturn", &dir), None, "no BIOS: standalone YabaSanshiro (HLE) plays it, no error card");
        std::fs::write(dir.join("sega_101.bin"), b"x").unwrap();
        assert_eq!(route("saturn", &dir), Some("yabasanshiro"));
        // Systems whose default emulator needs the BIOS as well are routed as before (pocket-core shows the message).
        assert_eq!(route("megadrive", &dir), Some("genesis_plus_gx"));
        assert_eq!(route("segacd", &dir), None, "not verified");
    }

    #[test]
    fn saturn_pad_has_every_button_once() {
        let pad = find("saturn").unwrap().buttons;
        let names: Vec<&str> = pad.iter().map(|(_, n)| *n).collect();
        assert_eq!(names, ["A", "B", "C", "X", "Y", "Z", "L", "R", "Start"]);
        // yabasanshiro's own layout (RetroArch's): bottom row A B C on B, A, L1; top row X Y Z on Y, X, R1;
        // the shoulder buttons on L2 / R2.
        let id = |n: &str| pad.iter().find(|(_, name)| *name == n).unwrap().0;
        assert_eq!([id("A"), id("B"), id("C")], [0, 8, 10]);
        assert_eq!([id("X"), id("Y"), id("Z")], [1, 9, 11]);
        assert_eq!([id("L"), id("R"), id("Start")], [12, 13, 3]);
        let mut ids: Vec<u8> = pad.iter().map(|(i, _)| *i).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), pad.len(), "two Saturn buttons on one Mini V2 button");
        assert!(find("megadrive").unwrap().buttons.is_empty(), "other systems: the core's names");
    }

    /// A scratch `/storage/roms` with an empty bios folder; `route` and `default_emulator` look next to it.
    fn roms_dir(name: &str) -> (crate::testutil::TestDir, std::path::PathBuf) {
        let roms = TestDir::new(&format!("roms-{name}"));
        std::fs::create_dir_all(roms.join("bios")).unwrap();
        let bios = roms.join("bios");
        (roms, bios)
    }

    #[test]
    fn dos_scummvm_pico8_and_tic80_are_hosted() {
        std::env::remove_var("POCKET_UI_HOST_ALL");
        // ROCKNIX's system names (es_systems.cfg) and its libretro cores for them.
        assert_eq!(host_core("pc"), Some("dosbox_pure"));
        assert_eq!(host_core("scummvm"), Some("scummvm"));
        assert_eq!(host_core("tic-80"), Some("tic80"));
        assert_eq!(
            find("pico-8").unwrap().core,
            "fake08",
            "the PICO-8 reimplementation; the official binary is not a core"
        );
        for s in ["pc", "scummvm", "pico-8", "tic-80"] {
            let h = find(s).unwrap();
            assert!(h.bios.is_empty() && h.verified, "{s}");
            assert_eq!(h.save_ext, "srm", "{s}");
        }
    }

    #[test]
    fn pico8_stays_with_the_official_binary_when_it_is_there() {
        std::env::remove_var("POCKET_UI_HOST_ALL");
        let (roms, bios) = roms_dir("pico8");
        // No official PICO-8: fake08 on pocket-core, and ROCKNIX's default (start_pico8.sh) is no use as the
        // fallback either, so RetroArch's fake08 stands in for it.
        assert_eq!(route("pico-8", &bios), Some("fake08"));
        assert_eq!(default_emulator("pico-8", &bios), Some(("retroarch", "fake08")));
        // start_pico8.sh runs roms/pico-8/aarch64/pico8_64 when that folder exists, else roms/pico-8/pico8_64.
        std::fs::create_dir_all(roms.join("pico-8")).unwrap();
        std::fs::write(roms.join("pico-8/pico8_64"), b"x").unwrap();
        assert_eq!(route("pico-8", &bios), None, "the user's own PICO-8: ROCKNIX's start_pico8.sh, as before");
        assert_eq!(default_emulator("pico-8", &bios), None, "es_systems' default (pico-8 / pico8) applies");
        std::fs::create_dir_all(roms.join("pico-8/aarch64")).unwrap();
        assert_eq!(
            route("pico-8", &bios),
            Some("fake08"),
            "an aarch64 folder without the binary: start_pico8.sh would fail"
        );
        std::fs::write(roms.join("pico-8/aarch64/pico8_64"), b"x").unwrap();
        assert_eq!(route("pico-8", &bios), None);
        // Other systems have no stand-in.
        assert_eq!(default_emulator("pc", &bios), None);
        assert_eq!(default_emulator("saturn", &bios), None);
    }

    #[test]
    fn scummvm_shows_200_line_games_at_4_3_like_a_monitor() {
        // The core reports the game's pixels (320x200 = 1.6); the monitors those games were made for stretched
        // them to 4:3 (non-square pixels), and so does DOSBox Pure's aspect correction.
        assert_eq!(display_aspect("scummvm", 320, 200, 1.6), 4.0 / 3.0);
        assert_eq!(display_aspect("scummvm", 640, 400, 1.6), 4.0 / 3.0);
        // Already 4:3, higher resolution, or the launcher at 16:9: as reported.
        assert_eq!(display_aspect("scummvm", 640, 480, 4.0 / 3.0), 4.0 / 3.0);
        assert_eq!(display_aspect("scummvm", 800, 600, 1.3333334), 1.3333334);
        assert_eq!(display_aspect("scummvm", 1280, 720, 16.0 / 9.0), 16.0 / 9.0);
        // Other systems keep what their core says, even at 320x200.
        assert_eq!(display_aspect("pc", 320, 200, 1.6), 1.6);
        assert_eq!(display_aspect("gba", 320, 200, 1.6), 1.6);
    }

    #[test]
    fn scummvm_gets_the_game_folder_from_rocknix_files() {
        let (roms, _) = roms_dir("svm");
        let games = roms.join("games");
        std::fs::create_dir_all(&games).unwrap();
        // What ROCKNIX's "Scan ScummVM Games" writes (start_scummvm.sh create): standalone ScummVM's arguments.
        let rocknix = games.join("Flight of the Amazon Queen (queen).scummvm");
        std::fs::write(&rocknix, "--path=\"/storage/roms/scummvm/FOTAQ\" queen\n").unwrap();
        assert_eq!(content_path("scummvm", &rocknix), Path::new("/storage/roms/scummvm/FOTAQ"));
        // The libretro core's own hook file (just a game id) and a game folder go to the core as they are.
        let hook = games.join("sky.scummvm");
        std::fs::write(&hook, "sky\n").unwrap();
        assert_eq!(content_path("scummvm", &hook), hook);
        assert_eq!(
            content_path("scummvm", Path::new("/storage/roms/scummvm/BASS")),
            Path::new("/storage/roms/scummvm/BASS")
        );
        assert_eq!(content_path("pc", &rocknix), rocknix, "only ScummVM");
    }

    #[test]
    fn hosted_files_and_shelf_entries() {
        // ScummVM's folder also holds ROCKNIX scripts (.sh): those stay with runemu.sh.
        assert!(hosts_file("scummvm", Path::new("/storage/.config/scummvm/games/Game (id).scummvm")));
        assert!(hosts_file("scummvm", Path::new("/storage/roms/scummvm/BASS")));
        assert!(!hosts_file("scummvm", Path::new("/storage/.config/scummvm/games/Scan.sh")));
        assert!(hosts_file("pc", Path::new("/storage/roms/pc/Game.zip")));
        assert!(hosts_file("megadrive", Path::new("/storage/roms/megadrive/Game.md")));
        // DOSBox Pure's main format is a zip, which ROCKNIX's list leaves out; its own saves are zips too.
        assert_eq!(shelf_extensions("pc"), ["zip"]);
        assert!(shelf_extensions("megadrive").is_empty());
        assert!(not_a_game("pc", Path::new("/storage/roms/pc/Game.pure.zip")));
        assert!(!not_a_game("pc", Path::new("/storage/roms/pc/Game.zip")));
    }

    #[test]
    fn game_folders_are_games() {
        std::env::remove_var("POCKET_UI_HOST_ALL");
        let (roms, _) = roms_dir("folders");
        let pc = roms.join("pc");
        for d in ["Keen", ".hidden", "Doom"] {
            std::fs::create_dir_all(pc.join(d)).unwrap();
        }
        std::fs::write(pc.join("Game.zip"), b"x").unwrap();
        assert_eq!(
            folder_games("pc", &pc, &roms, &[]),
            [pc.join("Doom"), pc.join("Keen")],
            "sorted, hidden ones left out"
        );
        // ScummVM: the folders under roms/scummvm, except those a .scummvm file of the shelf already starts.
        let svm = roms.join("scummvm");
        std::fs::create_dir_all(svm.join("FOTAQ")).unwrap();
        std::fs::create_dir_all(svm.join("BASS")).unwrap();
        let listed = roms.join("Flight of the Amazon Queen (queen).scummvm");
        std::fs::write(&listed, format!("--path=\"{}\" queen\n", svm.join("FOTAQ").display())).unwrap();
        let shelf = Path::new("/storage/.config/scummvm/games");
        assert_eq!(folder_games("scummvm", shelf, &roms, &[listed]), [svm.join("BASS")]);
        assert!(folder_games("tic-80", &pc, &roms, &[]).is_empty(), "other systems: files only");
    }

    #[test]
    fn scummvm_finds_rocknix_engine_data_and_standalone_saves() {
        let exists = |p: &Path| p.starts_with("/usr/share/scummvm") || p == Path::new(SCUMMVM_SAVES);
        // No scummvm.ini in roms/bios yet: the core gets one that points it at ROCKNIX's engine data (sky.cpt, the
        // virtual keyboard, themes), and at standalone ScummVM's saves.
        assert_eq!(
            scummvm_ini("", exists).unwrap(),
            "[scummvm]\nextrapath=/usr/share/scummvm\nthemepath=/usr/share/scummvm\nsavepath=/storage/.local/share/scummvm/saves\n"
        );
        // The copy ROCKNIX's scan makes of standalone's ini: the keys go into [scummvm], the games stay as they are.
        let rocknix = "[scummvm]\nmusic_volume=192\n\n[queen]\npath=/storage/roms/scummvm/FOTAQ\n";
        let out = scummvm_ini(rocknix, exists).unwrap();
        assert!(
            out.starts_with(
                "[scummvm]\nextrapath=/usr/share/scummvm\nthemepath=/usr/share/scummvm\nsavepath=/storage/.local/share/scummvm/saves\nmusic_volume=192\n"
            ),
            "{out}"
        );
        assert!(out.ends_with("[queen]\npath=/storage/roms/scummvm/FOTAQ\n"), "{out}");
        assert_eq!(scummvm_ini(&out, exists), None, "nothing to change the second time");
        // A path the user set that exists stays; one that doesn't is replaced.
        let own = "[scummvm]\nsavepath=/storage/mysaves\nextrapath=/gone\n";
        let out = scummvm_ini(own, |p| exists(p) || p == Path::new("/storage/mysaves")).unwrap();
        assert!(
            out.contains("savepath=/storage/mysaves\n") && !out.contains("/gone") && out.contains("extrapath=/usr/share/scummvm\n"),
            "{out}"
        );
    }

    #[test]
    fn more_systems_of_2026_10_23() {
        std::env::remove_var("POCKET_UI_HOST_ALL");
        // ROCKNIX's default core for each (es_systems.cfg 20261001), so saves and states stay where they were.
        for (system, core) in [
            ("famicom", "nestopia"),
            ("nesh", "nestopia"),
            ("fds", "nestopia"),
            ("sfc", "snes9x"),
            ("snesh", "snes9x"),
            ("gbh", "gambatte"),
            ("gbch", "gambatte"),
            ("gbah", "mgba"),
            ("gbav", "mgba"),
            ("supergrafx", "beetle_supergrafx"),
            ("virtualboy", "beetle_vb"),
        ] {
            assert_eq!(host_core(system), Some(core), "{system}");
            assert!(crate::run_ahead::available_for(system), "{system}: a light 2D core");
        }
        // NAOMI and Atomiswave are wired but stay on standalone Flycast: no BIOS to boot a game with here.
        assert_eq!(find("naomi").unwrap().core, "flycast");
        assert_eq!(host_core("naomi"), None);
        assert_eq!(host_core("atomiswave"), None);
        assert!(find("naomi").unwrap().analog && find("atomiswave").unwrap().analog, "racing games: the sticks go to the core");
    }

    #[test]
    fn disk_system_and_arcade_boards_name_their_bios() {
        let dir = bios_dir("more");
        let fds = find("fds").unwrap();
        assert_eq!(fds.missing_bios(&dir).unwrap(), "Famicom Disk System needs a BIOS file in roms/bios: disksys.rom");
        assert!(find("famicom").unwrap().missing_bios(&dir).is_none(), "cartridges need none");
        // Flycast looks for MAME's sets in bios/dc, where ROCKNIX's standalone Flycast keeps them too.
        assert_eq!(
            find("naomi").unwrap().missing_bios(&dir).unwrap(),
            "NAOMI needs a BIOS file in roms/bios: dc/naomi.zip or dc/naomi2.zip"
        );
        assert_eq!(find("atomiswave").unwrap().missing_bios(&dir).unwrap(), "Atomiswave needs a BIOS file in roms/bios: dc/awbios.zip");
        std::fs::write(dir.join("disksys.rom"), b"x").unwrap();
        std::fs::create_dir_all(dir.join("dc")).unwrap();
        std::fs::write(dir.join("dc/naomi2.zip"), b"x").unwrap();
        std::fs::write(dir.join("awbios.zip"), b"x").unwrap();
        assert!(fds.missing_bios(&dir).is_none());
        assert!(find("naomi").unwrap().missing_bios(&dir).is_none(), "a NAOMI 2 set is enough to start");
        assert!(find("atomiswave").unwrap().missing_bios(&dir).is_some(), "not in bios/dc");
        // The default emulators need them as well: a missing BIOS is a message, not a reason to switch.
        assert!(!fds.default_without_bios && !find("naomi").unwrap().default_without_bios);
    }

    #[test]
    fn supergrafx_takes_hucards_and_leaves_cds_to_retroarch() {
        for f in ["Game.pce", "Game.SGX", "Game.zip"] {
            assert!(hosts_file("supergrafx", Path::new(f)), "{f}");
        }
        for f in ["Game.cue", "Game.ccd", "Game.chd"] {
            assert!(!hosts_file("supergrafx", Path::new(f)), "{f}: needs a System Card");
        }
    }

    #[test]
    fn virtual_boy_has_the_right_d_pad_on_the_right_stick() {
        let vb = find("virtualboy").unwrap();
        assert_eq!(vb.options, [("vb_right_analog_to_digital", "enabled")]);
        assert!(!vb.analog, "the left stick works the left D-pad");
        assert!(vb.buttons.is_empty(), "beetle_vb names its buttons itself (Left / Right D-Pad ...)");
    }

    #[test]
    fn pointer_and_keyboard_options() {
        let opt = |s: &str, k: &str| find(s).unwrap().options.iter().find(|(key, _)| *key == k).map(|(_, v)| *v);
        // A tap is a click where the finger is (ScummVM's pointer, DOSBox Pure's touchpad mode, TIC-80's touch screen).
        assert_eq!(opt("scummvm", "scummvm_pointer_device"), Some("pointer"));
        assert_eq!(opt("pc", "dosbox_pure_mouse_input"), Some("pad"));
        assert_eq!(opt("tic-80", "tic80_pointer_device"), Some("touchscreen"));
        // ScummVM on the pad: A (right) clicks, B (bottom) right-clicks, the core's own mapper does the rest.
        assert_eq!(opt("scummvm", "scummvm_mapper_a"), Some("RETROKE_LEFT_BUTTON"));
        assert_eq!(opt("scummvm", "scummvm_mapper_b"), Some("RETROKE_RIGHT_BUTTON"));
        // DOSBox Pure: L3 opens its on-screen keyboard (its menu is one tab away).
        assert_eq!(opt("pc", "dosbox_pure_on_screen_keyboard"), Some("keyboard"));
        assert!(
            find("scummvm").unwrap().no_states.is_some(),
            "the core has no save states: the menu says where saving is"
        );
        assert!(
            find("tic-80").unwrap().no_states.is_some(),
            "its states keep only the persistent memory, not the game"
        );
        // ROCKNIX's fake08 writes states without the cart's Lua state (its size field is 0): a load restores the
        // PICO-8 memory and nothing of the game.
        assert!(find("pico-8").unwrap().no_states.is_some());
        assert!(find("pc").unwrap().no_states.is_none());
        assert!(find("pc").unwrap().states_vary, "DOSBox Pure's states grow while the game runs");
        assert!(!find("pico-8").unwrap().states_vary && !find("megadrive").unwrap().states_vary);
        assert!(core_states_vary("mgba"), "mGBA's states grow with the game's save data (GBA resume)");
        assert!(core_states_vary("nestopia"), "Nestopia's states differ by 9 bytes from moment to moment (NES resume)");
        assert!(!core_states_vary("gambatte") && !core_states_vary("snes9x"));
    }
}
