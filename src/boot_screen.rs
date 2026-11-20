//! Settings → Playing → Console boot screen: a fresh start of a Game Boy, Game Boy Color, Game Boy Advance, PlayStation, Dreamcast,
//! Saturn, Mega-CD or PC Engine CD game runs through the console's own boot screen (the Nintendo logo, the PlayStation start-up, the
//! Dreamcast's swirl, the Saturn's and the Mega-CD's logo, the System Card's title), when the player's own BIOS file is in the BIOS
//! folder. pocket-ui ships no BIOS file and never fetches one; the cores' options are what makes them run it.
//!
//! Decided in pocket-core (and the Android player) before the core loads, because the BIOS is a load-time option of the core
//! ("restart" options): `plan` names the core options (the one place that knows which core says it how) and whether the file is
//! there. A start that continues a moment never runs it (`starts_fresh`), so Continue stays as quick as it was, and with Continue
//! where I quit Off, Quit keeps no moment and every start is fresh: that is the "boot to the last game, with the logo" the setting
//! is for.
//!
//! The disc consoles' cores use the player's BIOS by themselves once it is there (Flycast, the Saturn cores, the Mega-CD and PC
//! Engine CD ones need it for every start anyway); their options here make sure no replacement BIOS skips the boot screen. The
//! DS is not here: its cores either boot the game directly (DSperate) or stop in the DS's menu and its Health and Safety screen
//! (melonDS), which wait for a touch, so the start would not go on by itself.

use std::path::Path;

/// One core option: (key, value).
pub type Opt = (&'static str, &'static str);

/// What a start does about the console's boot screen.
#[derive(Debug, PartialEq, Eq)]
pub enum Boot {
    /// A console without a boot screen to run here (SNES, N64, Mega Drive, …): the start stays as it is.
    NotHere,
    /// The BIOS file is there: these core options run the boot screen.
    Screen(&'static [Opt]),
    /// The player's file isn't there (or isn't the original's size): the game starts as before, and the note names the file.
    Missing(&'static str),
}

/// The note when the file is missing; `{s}` is the file's name.
pub fn missing_note(file: &str) -> String {
    format!("No boot screen: needs the original {file} in roms/bios")
}

/// Whether this start runs a game from the beginning: no moment is going to be loaded. False when a moment is kept for the game
/// (`slots::auto_candidates`, the files `Core::resume` reads) or the Cabinet's TV hands one over; true when pocket-ui said the
/// kept moment is another file's or emulator's (`frozen::SKIP_RESUME_ENV`).
pub fn starts_fresh(auto: &Path, skip_resume: bool, handed_over: bool) -> bool {
    if handed_over {
        return false;
    }
    skip_resume || !crate::slots::auto_candidates(auto, false).iter().any(|p| p.is_file())
}

/// The Game Boy BIOS this game runs through, by the Color flag of its header (what the core goes by); the ROM's system when the
/// header can't be read (a zip). (file, size in bytes)
fn gb_bios(rom: &Path, system: &str) -> (&'static str, u64) {
    use std::io::{Read, Seek, SeekFrom};
    const DMG: (&str, u64) = ("gb_bios.bin", 256);
    const CGB: (&str, u64) = ("gbc_bios.bin", 2304);
    let flag = std::fs::File::open(rom).ok().and_then(|mut f| {
        let mut b = [0u8; 1];
        f.seek(SeekFrom::Start(0x143)).ok()?;
        f.read_exact(&mut b).ok()?;
        Some(b[0])
    });
    match flag {
        Some(b) if b & 0x80 != 0 => CGB,
        Some(_) => DMG,
        None if system == "gbc" => CGB,
        None => DMG,
    }
}

/// `file` of exactly `bytes` in `dir`.
fn has(dir: &Path, file: &str, bytes: u64) -> bool {
    std::fs::metadata(dir.join(file)).is_ok_and(|m| m.is_file() && m.len() == bytes)
}

/// The first of `files` (name, size in bytes) that is in `dir`.
fn first_of(dir: &Path, files: &[(&'static str, u64)]) -> Option<&'static str> {
    files.iter().find(|(f, b)| has(dir, f, *b)).map(|(f, _)| *f)
}

/// The Saturn BIOS names the cores look for, the Japanese one first (Yabause and YabaSanshiro also read `saturn_bios.bin`).
const SATURN: &[(&str, u64)] = &[("saturn_bios.bin", 512 << 10), ("sega_101.bin", 512 << 10), ("mpr-17933.bin", 512 << 10)];
/// The PC Engine CD's System Card 3, which every CD game boots through.
const SYSCARD: &[(&str, u64)] = &[("syscard3.pce", 256 << 10)];
/// Flycast's Dreamcast BIOS, in its `dc` folder.
const DC_BOOT: &[(&str, u64)] = &[("dc/dc_boot.bin", 2 << 20)];

/// The Mega-CD BIOS a disc boots through: the one of its region, read from the file's name ("(Europe)", "(Japan)", "(USA)" and
/// the short tags), as the cores pick it by the disc's region; any of the three when the name says none. (files, the one a note names)
fn mega_cd(rom: &Path) -> (&'static [(&'static str, u64)], &'static str) {
    const E: (&str, u64) = ("bios_CD_E.bin", 128 << 10);
    const U: (&str, u64) = ("bios_CD_U.bin", 128 << 10);
    const J: (&str, u64) = ("bios_CD_J.bin", 128 << 10);
    let name = rom.file_name().map(|n| n.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    let tag = |long: &str, short: &str| name.contains(&format!("({long}")) || name.contains(&format!("({short})"));
    if tag("europe", "e") {
        (&[E], E.0)
    } else if tag("japan", "j") {
        (&[J], J.0)
    } else if tag("usa", "u") {
        (&[U], U.0)
    } else {
        (&[U, E, J], U.0)
    }
}

/// What the start of `rom` (`system`, run by `core`, the file name without `_libretro.so`) does about the console's boot screen
/// with the BIOS files of `bios_dir`. The files are the ones the cores look for by these names; the PlayStation takes any of its
/// usual BIOS names (`document_library::is_ps1_bios_name`).
pub fn plan(core: &str, system: &str, rom: &Path, bios_dir: &Path) -> Boot {
    const GAMBATTE: &[Opt] = &[("gambatte_gb_bootloader", "enabled")];
    const MGBA: &[Opt] = &[("mgba_use_bios", "ON"), ("mgba_skip_bios", "OFF")];
    const GPSP: &[Opt] = &[("gpsp_bios", "official"), ("gpsp_boot_mode", "bios")];
    const PSX: &[Opt] = &[("pcsx_rearmed_show_bios_bootlogo", "enabled")];
    // Both spellings of Flycast's switch (newer: use the real BIOS; older: no replacement BIOS), and never its menu instead of the game.
    const FLYCAST: &[Opt] = &[("flycast_use_real_bios", "enabled"), ("flycast_hle_bios", "disabled"), ("reicast_hle_bios", "disabled"), ("flycast_boot_to_bios", "disabled")];
    const YABAUSE: &[Opt] = &[("yabause_force_hle_bios", "disabled")];
    const YABASANSHIRO: &[Opt] = &[("yabasanshiro_force_hle_bios", "disabled")];
    const GPGX: &[Opt] = &[("genesis_plus_gx_bios", "enabled")];
    const PCE_CD: &[Opt] = &[("pce_fast_cdbios", "System Card 3"), ("pce_cdbios", "System Card 3")];
    const NONE: &[Opt] = &[];
    const GBA_BIOS: (&str, u64) = ("gba_bios.bin", 16384);
    let found = |options: &'static [Opt], files: &[(&'static str, u64)], named: &'static str| match first_of(bios_dir, files) {
        Some(_) => Boot::Screen(options),
        None => Boot::Missing(named),
    };
    let one = |options: &'static [Opt], (file, bytes): (&'static str, u64)| if has(bios_dir, file, bytes) { Boot::Screen(options) } else { Boot::Missing(file) };
    match (core, system) {
        ("gambatte", _) => one(GAMBATTE, gb_bios(rom, system)),
        ("mgba", _) => one(MGBA, GBA_BIOS),
        ("gpsp", _) => one(GPSP, GBA_BIOS),
        ("pcsx_rearmed", _) => {
            if crate::document_library::has_ps1_bios(bios_dir) { Boot::Screen(PSX) } else { Boot::Missing("scph5501.bin") }
        }
        ("flycast", _) => found(FLYCAST, DC_BOOT, "dc/dc_boot.bin"),
        ("yabause", _) => found(YABAUSE, SATURN, "saturn_bios.bin"),
        ("yabasanshiro", _) => found(YABASANSHIRO, SATURN, "saturn_bios.bin"),
        // Beetle Saturn always boots through the real BIOS and reads only the two console names.
        ("beetle_saturn" | "mednafen_saturn", _) => found(NONE, &SATURN[1..], "sega_101.bin"),
        ("genesis_plus_gx" | "genesis_plus_gx_wide" | "picodrive", "segacd" | "megacd") => {
            let (files, named) = mega_cd(rom);
            found(if core == "picodrive" { NONE } else { GPGX }, files, named)
        }
        ("beetle_pce_fast" | "mednafen_pce_fast" | "beetle_pce" | "mednafen_pce", "pcenginecd" | "tg16cd" | "pcecd") => found(PCE_CD, SYSCARD, "syscard3.pce"),
        _ => Boot::NotHere,
    }
}

/// What pocket-core does at a game's start: nothing (`NotHere`) while Console boot screen is Off or this start loads a moment, else
/// `plan`. The one call of the binary, so that "fresh or Continue, On or Off" is tested in the library.
pub struct Start<'a> {
    pub on: bool,
    pub auto: &'a Path,
    pub skip_resume: bool,
    pub handed_over: bool,
    pub core: &'a str,
    pub system: &'a str,
    pub rom: &'a Path,
    pub bios_dir: &'a Path,
}

pub fn for_start(s: &Start) -> Boot {
    if !s.on || !starts_fresh(s.auto, s.skip_resume, s.handed_over) {
        return Boot::NotHere;
    }
    plan(s.core, s.system, s.rom, s.bios_dir)
}

/// Puts `boot`'s core options into `options` for the core to read, over a saved or curated value of the same key (Console boot screen
/// is what the player chose for this start).
pub fn apply<'a>(options: &mut Vec<(&'a str, &'a str)>, screen: &'static [Opt]) {
    for (key, value) in screen {
        options.retain(|(k, _)| k != key);
        options.push((key, value));
    }
}

/// The start's display or TV switches on with the boot screen's first light, not on the black before it (a Dreamcast's or a
/// PlayStation's BIOS shows black for a while first): pocket-core keeps the picture dark and the switch-on waiting until the
/// picture has light ([`lit`]), at most [`Wait::MAX`] after the first frame. Only for a start that runs the boot screen.
#[derive(Debug, Default)]
pub struct Wait {
    since: Option<std::time::Instant>,
    lit: bool,
}

impl Wait {
    /// The longest the switch-on waits for light: a boot screen that stays black longer starts as before.
    pub const MAX: std::time::Duration = std::time::Duration::from_millis(2500);

    /// The picture painted at `now` was `lit` or not; the first call is the first frame.
    pub fn see(&mut self, lit: bool, now: std::time::Instant) {
        let since = *self.since.get_or_insert(now);
        self.lit |= lit || now.duration_since(since) >= Self::MAX;
    }

    /// Whether the display or TV may switch on now.
    pub fn ready(&self) -> bool {
        self.lit
    }
}

/// Whether a picture (RGBA bytes, as `glReadPixels` reads them) has light in it: one pixel in 400 at least clearly above black
/// (a logo's first pixels, a fade's first step), so the noise of a black screen and a stray pixel don't count.
pub fn lit(rgba: &[u8]) -> bool {
    let pixels = rgba.len() / 4;
    if pixels == 0 {
        return false;
    }
    let bright = rgba.chunks_exact(4).filter(|p| (p[0] as u32 * 3 + p[1] as u32 * 6 + p[2] as u32) / 10 > 24).count();
    bright * 400 >= pixels
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    use crate::testutil::TestDir;

    /// A scratch folder with the helpers these tests share; `.0` is the folder.
    struct Dir(PathBuf, TestDir);
    impl Dir {
        fn new(name: &str) -> Dir {
            let t = TestDir::new(&format!("boot-screen-{name}"));
            Dir(t.to_path_buf(), t)
        }
        fn file(&self, name: &str, bytes: usize) -> PathBuf {
            let p = self.0.join(name);
            std::fs::write(&p, vec![0u8; bytes]).unwrap();
            p
        }
        /// A Game Boy ROM whose header has this Color flag.
        fn gb_rom(&self, name: &str, flag: u8) -> PathBuf {
            let mut rom = vec![0u8; 0x200];
            rom[0x143] = flag;
            let p = self.0.join(name);
            std::fs::write(&p, rom).unwrap();
            p
        }
    }

    fn screen(boot: Boot) -> &'static [Opt] {
        match boot {
            Boot::Screen(o) => o,
            other => panic!("expected the boot screen, got {other:?}"),
        }
    }

    /// Every core with a boot screen gets its own option names; the values are what the cores list (curation's own spelling).
    #[test]
    fn each_core_gets_its_own_options_when_the_players_bios_is_there() {
        let d = Dir::new("cores");
        d.file("gb_bios.bin", 256);
        d.file("gbc_bios.bin", 2304);
        d.file("gba_bios.bin", 16384);
        d.file("scph5501.bin", 512 << 10);
        let dmg = d.gb_rom("Tetris.gb", 0x00);
        assert_eq!(screen(plan("gambatte", "gb", &dmg, &d.0)), &[("gambatte_gb_bootloader", "enabled")]);
        let rom = d.file("Zelda.gba", 100);
        assert_eq!(screen(plan("mgba", "gba", &rom, &d.0)), &[("mgba_use_bios", "ON"), ("mgba_skip_bios", "OFF")]);
        assert_eq!(screen(plan("gpsp", "gba", &rom, &d.0)), &[("gpsp_bios", "official"), ("gpsp_boot_mode", "bios")]);
        let disc = d.file("Crash.cue", 10);
        assert_eq!(screen(plan("pcsx_rearmed", "psx", &disc, &d.0)), &[("pcsx_rearmed_show_bios_bootlogo", "enabled")]);
    }

    /// The consoles with no boot screen to run here, and cores this doesn't know, are left alone.
    #[test]
    fn other_consoles_start_as_before() {
        let d = Dir::new("others");
        d.file("gba_bios.bin", 16384);
        let rom = d.file("Game.sfc", 100);
        for core in ["snes9x", "mupen64plus_next", "genesis_plus_gx", "picodrive", "nestopia", "melonds", "melondsds", "dsperate", ""] {
            assert_eq!(plan(core, "snes", &rom, &d.0), Boot::NotHere, "{core}");
        }
        // The Mega Drive and the PC Engine's cards have no boot screen of their own here: only their CD systems do.
        d.file("bios_CD_U.bin", 128 << 10);
        d.file("syscard3.pce", 256 << 10);
        assert_eq!(plan("genesis_plus_gx", "megadrive", &rom, &d.0), Boot::NotHere);
        assert_eq!(plan("picodrive", "sega32x", &rom, &d.0), Boot::NotHere);
        assert_eq!(plan("beetle_pce_fast", "pcengine", &rom, &d.0), Boot::NotHere);
    }

    /// Without the player's file (or with another file of that name) the game starts as before and the note names the file.
    #[test]
    fn a_missing_or_wrong_bios_is_named_and_never_enabled() {
        let d = Dir::new("missing");
        let rom = d.gb_rom("Tetris.gb", 0);
        assert_eq!(plan("gambatte", "gb", &rom, &d.0), Boot::Missing("gb_bios.bin"));
        assert_eq!(plan("gpsp", "gba", &rom, &d.0), Boot::Missing("gba_bios.bin"));
        assert_eq!(plan("mgba", "gba", &rom, &d.0), Boot::Missing("gba_bios.bin"));
        assert_eq!(plan("pcsx_rearmed", "psx", &rom, &d.0), Boot::Missing("scph5501.bin"));
        d.file("gba_bios.bin", 5000);
        d.file("gb_bios.bin", 0);
        d.file("scph1001.bin", 1000);
        assert_eq!(plan("mgba", "gba", &rom, &d.0), Boot::Missing("gba_bios.bin"), "a file of another size is not the original");
        assert_eq!(plan("gambatte", "gb", &rom, &d.0), Boot::Missing("gb_bios.bin"));
        assert_eq!(plan("pcsx_rearmed", "psx", &rom, &d.0), Boot::Missing("scph5501.bin"));
        assert_eq!(missing_note("gba_bios.bin"), "No boot screen: needs the original gba_bios.bin in roms/bios");
    }

    /// A Game Boy Color game boots through the Color BIOS, a plain Game Boy game through the original's; the other file doesn't help.
    #[test]
    fn a_game_boy_game_uses_the_bios_of_its_own_header() {
        let d = Dir::new("gb-header");
        let mono = d.gb_rom("Mono.gb", 0x00);
        let both = d.gb_rom("Both.gbc", 0x80);
        let colour = d.gb_rom("Colour.gbc", 0xC0);
        d.file("gb_bios.bin", 256);
        assert!(matches!(plan("gambatte", "gb", &mono, &d.0), Boot::Screen(_)));
        assert_eq!(plan("gambatte", "gbc", &both, &d.0), Boot::Missing("gbc_bios.bin"));
        assert_eq!(plan("gambatte", "gbc", &colour, &d.0), Boot::Missing("gbc_bios.bin"));
        d.file("gbc_bios.bin", 2304);
        assert!(matches!(plan("gambatte", "gbc", &colour, &d.0), Boot::Screen(_)));
        // A zip can't be read for its header: the system folder decides.
        std::fs::write(d.0.join("Zipped.zip"), b"PK").unwrap();
        assert!(matches!(plan("gambatte", "gbc", &d.0.join("Zipped.zip"), &d.0), Boot::Screen(_)));
        assert!(matches!(plan("gambatte", "gb", &d.0.join("Absent.gb"), &d.0), Boot::Screen(_)));
    }

    /// A fresh start runs it; a kept moment (Continue where I quit On, a crash's retry moment) or the TV's hand-over never does;
    /// a moment of another file counts as fresh.
    #[test]
    fn only_a_start_that_loads_no_moment_is_fresh() {
        let d = Dir::new("fresh");
        let base = d.0.join("Game");
        let auto = crate::slots::auto_path(&base);
        assert!(starts_fresh(&auto, false, false), "no moment kept: Continue where I quit is Off, or the first start");
        std::fs::write(&auto, b"moment").unwrap();
        assert!(!starts_fresh(&auto, false, false), "Continue: the moment is loaded at once, no boot screen");
        assert!(starts_fresh(&auto, true, false), "the moment is another file's: it is set aside, the game starts fresh");
        assert!(!starts_fresh(&auto, true, true), "the Cabinet TV's hand-over continues");
        std::fs::remove_file(&auto).unwrap();
        std::fs::write(crate::slots::taken_auto_path(&auto), b"taken").unwrap();
        assert!(!starts_fresh(&auto, false, false), "a moment taken by the session before still resumes");
        std::fs::remove_file(crate::slots::taken_auto_path(&auto)).unwrap();
        std::fs::write(crate::slots::retry_auto_path(&auto), b"retry").unwrap();
        assert!(starts_fresh(&auto, false, false), "Continue doesn't read the retired retry moment");
    }

    /// Console boot screen On or Off, a fresh start or Continue: only On with nothing to continue runs it, for every console.
    #[test]
    fn on_and_fresh_runs_it_off_or_continue_never_does() {
        let d = Dir::new("for-start");
        d.file("gb_bios.bin", 256);
        d.file("gba_bios.bin", 16384);
        d.file("scph5501.bin", 512 << 10);
        std::fs::create_dir_all(d.0.join("dc")).unwrap();
        d.file("dc/dc_boot.bin", 2 << 20);
        d.file("saturn_bios.bin", 512 << 10);
        d.file("bios_CD_U.bin", 128 << 10);
        d.file("syscard3.pce", 256 << 10);
        let auto = crate::slots::auto_path(&d.0.join("Game"));
        let rom = d.gb_rom("Game.gb", 0);
        let cores = [("gambatte", "gb"), ("gpsp", "gba"), ("mgba", "gba"), ("pcsx_rearmed", "psx"), ("flycast", "dreamcast"), ("yabasanshiro", "saturn"),
            ("genesis_plus_gx", "segacd"), ("beetle_pce_fast", "pcenginecd")];
        for (core, system) in cores {
            let start = |on: bool, skip_resume: bool| Start { on, auto: &auto, skip_resume, handed_over: false, core, system, rom: &rom, bios_dir: &d.0 };
            let fresh_on = for_start(&start(true, false));
            assert!(matches!(fresh_on, Boot::Screen(_)), "{core}: On, fresh start");
            assert_eq!(for_start(&start(false, false)), Boot::NotHere, "{core}: Off");
            std::fs::write(&auto, b"moment").unwrap();
            assert_eq!(for_start(&start(true, false)), Boot::NotHere, "{core}: On, but a moment is continued");
            assert_eq!(for_start(&start(true, true)), fresh_on, "{core}: On, the moment is another file's");
            std::fs::remove_file(&auto).unwrap();
        }
        // A fresh start without the BIOS says which file is missing; a console without a boot screen says nothing.
        let none = Dir::new("for-start-none");
        let start = |core: &'static str| Start { on: true, auto: &auto, skip_resume: false, handed_over: false, core, system: "x", rom: &rom, bios_dir: &none.0 };
        assert_eq!(for_start(&start("gpsp")), Boot::Missing("gba_bios.bin"));
        assert_eq!(for_start(&start("snes9x")), Boot::NotHere);
    }

    /// The disc consoles, one by one: the player's file there, missing, or of another size (not the original).
    #[test]
    fn each_disc_console_runs_it_with_its_own_file_and_names_it_when_missing_or_wrong() {
        // (core, system, file, size, the options it gets, the file a missing note names)
        let cases: &[(&str, &str, &str, usize, &[Opt], &str)] = &[
            ("flycast", "dreamcast", "dc/dc_boot.bin", 2 << 20, &[("flycast_use_real_bios", "enabled"), ("flycast_hle_bios", "disabled"), ("reicast_hle_bios", "disabled"), ("flycast_boot_to_bios", "disabled")], "dc/dc_boot.bin"),
            ("yabasanshiro", "saturn", "saturn_bios.bin", 512 << 10, &[("yabasanshiro_force_hle_bios", "disabled")], "saturn_bios.bin"),
            ("yabause", "saturn", "mpr-17933.bin", 512 << 10, &[("yabause_force_hle_bios", "disabled")], "saturn_bios.bin"),
            ("beetle_saturn", "saturn", "sega_101.bin", 512 << 10, &[], "sega_101.bin"),
            ("genesis_plus_gx", "segacd", "bios_CD_U.bin", 128 << 10, &[("genesis_plus_gx_bios", "enabled")], "bios_CD_U.bin"),
            ("picodrive", "segacd", "bios_CD_J.bin", 128 << 10, &[], "bios_CD_U.bin"),
            ("beetle_pce_fast", "pcenginecd", "syscard3.pce", 256 << 10, &[("pce_fast_cdbios", "System Card 3"), ("pce_cdbios", "System Card 3")], "syscard3.pce"),
        ];
        for (i, &(core, system, file, size, options, named)) in cases.iter().enumerate() {
            let d = Dir::new(&format!("disc-{i}"));
            std::fs::create_dir_all(d.0.join("dc")).unwrap();
            let disc = d.file("Game.cue", 10);
            assert_eq!(plan(core, system, &disc, &d.0), Boot::Missing(named), "{core}: no file");
            d.file(file, size - 1);
            assert_eq!(plan(core, system, &disc, &d.0), Boot::Missing(named), "{core}: a file of another size is not the original");
            d.file(file, size);
            assert_eq!(screen(plan(core, system, &disc, &d.0)), options, "{core}: the player's file is there");
        }
        // Beetle Saturn doesn't read Yabause's own name.
        let d = Dir::new("disc-beetle-name");
        d.file("saturn_bios.bin", 512 << 10);
        assert_eq!(plan("beetle_saturn", "saturn", &d.0.join("Game.cue"), &d.0), Boot::Missing("sega_101.bin"));
    }

    /// A Mega-CD disc boots through the BIOS of its region, which its name says; one without a region tag through any of them.
    #[test]
    fn a_mega_cd_disc_needs_the_bios_of_its_region() {
        let d = Dir::new("mega-cd");
        let eu = d.0.join("Sonic CD (Europe).chd");
        let jp = d.0.join("Sonic CD (Japan).chd");
        let us = d.0.join("Sonic CD (USA).chd");
        let plain = d.0.join("Sonic CD.chd");
        d.file("bios_CD_E.bin", 128 << 10);
        assert!(matches!(plan("genesis_plus_gx", "segacd", &eu, &d.0), Boot::Screen(_)));
        assert!(matches!(plan("genesis_plus_gx", "segacd", &plain, &d.0), Boot::Screen(_)), "no tag: any region's file");
        assert_eq!(plan("genesis_plus_gx", "segacd", &jp, &d.0), Boot::Missing("bios_CD_J.bin"));
        assert_eq!(plan("genesis_plus_gx", "segacd", &us, &d.0), Boot::Missing("bios_CD_U.bin"));
        assert_eq!(plan("genesis_plus_gx", "segacd", &d.0.join("Game (E).cue"), &d.0), Boot::Screen(&[("genesis_plus_gx_bios", "enabled")]));
    }

    /// The switch-on waits for the boot screen's first light, and never longer than `Wait::MAX`.
    #[test]
    fn the_switch_on_waits_for_the_first_light_of_the_boot_screen() {
        use std::time::{Duration, Instant};
        let black = vec![0u8; 64 * 36 * 4];
        let mut noise = black.clone();
        noise[0] = 200;
        noise[4 * 50 + 1] = 30;
        assert!(!lit(&black) && !lit(&noise) && !lit(&[]), "black, its noise and a stray pixel are dark");
        let mut logo = black.clone();
        for p in logo.chunks_exact_mut(4).skip(900).take(12) {
            p.copy_from_slice(&[40, 40, 60, 255]);
        }
        assert!(lit(&logo), "a logo's first dim pixels are light");
        let t = Instant::now();
        let mut w = Wait::default();
        w.see(false, t);
        w.see(false, t + Duration::from_millis(900));
        assert!(!w.ready(), "black: still waiting");
        w.see(lit(&logo), t + Duration::from_millis(950));
        assert!(w.ready(), "the first light: the display switches on");
        let mut stays = Wait::default();
        stays.see(false, t);
        stays.see(false, t + Wait::MAX);
        assert!(stays.ready(), "a boot screen that stays black doesn't hold the start longer");
    }

    /// The boot options win over what was saved or curated for the same key, and the other options stay.
    #[test]
    fn the_boot_options_replace_saved_values_of_the_same_key() {
        let mut options: Vec<(&str, &str)> = vec![("gpsp_boot_mode", "game"), ("gpsp_color_correction", "off")];
        apply(&mut options, &[("gpsp_bios", "official"), ("gpsp_boot_mode", "bios")]);
        assert_eq!(options.iter().filter(|(k, _)| *k == "gpsp_boot_mode").collect::<Vec<_>>(), [&("gpsp_boot_mode", "bios")]);
        assert!(options.contains(&("gpsp_color_correction", "off")) && options.contains(&("gpsp_bios", "official")));
    }
}
