//! Settings → Playing → Console boot screen: a fresh start of a Game Boy, Game Boy Color, Game Boy Advance or PlayStation game runs
//! through the console's own boot screen (the Nintendo logo, the PlayStation start-up), when the player's own BIOS file is in the BIOS
//! folder. pocket-ui ships no BIOS file; the cores' options are what makes them run it.
//!
//! Decided in pocket-core before the core loads, because the BIOS is a load-time option of the core ("restart" options): `plan`
//! names the core options (the one place that knows which core says it how) and whether the file is there. A start that continues a
//! moment never runs it (`starts_fresh`), so Continue stays as quick as it was, and with Continue where I quit Off, Quit keeps no
//! moment and every start is fresh: that is the "boot to the last game, with the logo" the setting is for.

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

/// What the start of `rom` (`system`, run by `core`, the file name without `_libretro.so`) does about the console's boot screen
/// with the BIOS files of `bios_dir`. The Game Boy and Game Boy Advance files are the ones the cores look for by these names;
/// the PlayStation takes any of its usual BIOS names (`document_library::is_ps1_bios_name`).
pub fn plan(core: &str, system: &str, rom: &Path, bios_dir: &Path) -> Boot {
    const GAMBATTE: &[Opt] = &[("gambatte_gb_bootloader", "enabled")];
    const MGBA: &[Opt] = &[("mgba_use_bios", "ON"), ("mgba_skip_bios", "OFF")];
    const GPSP: &[Opt] = &[("gpsp_bios", "official"), ("gpsp_boot_mode", "bios")];
    const PSX: &[Opt] = &[("pcsx_rearmed_show_bios_bootlogo", "enabled")];
    const GBA_BIOS: (&str, u64) = ("gba_bios.bin", 16384);
    let (options, (file, bytes)) = match core {
        "gambatte" => (GAMBATTE, gb_bios(rom, system)),
        "mgba" => (MGBA, GBA_BIOS),
        "gpsp" => (GPSP, GBA_BIOS),
        "pcsx_rearmed" => {
            return if crate::document_library::has_ps1_bios(bios_dir) { Boot::Screen(PSX) } else { Boot::Missing("scph5501.bin") };
        }
        _ => return Boot::NotHere,
    };
    if has(bios_dir, file, bytes) { Boot::Screen(options) } else { Boot::Missing(file) }
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
        for core in ["snes9x", "mupen64plus_next", "genesis_plus_gx", "picodrive", "nestopia", "flycast", ""] {
            assert_eq!(plan(core, "snes", &rom, &d.0), Boot::NotHere, "{core}");
        }
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
        let auto = crate::slots::auto_path(&d.0.join("Game"));
        let rom = d.gb_rom("Game.gb", 0);
        for (core, system) in [("gambatte", "gb"), ("gpsp", "gba"), ("mgba", "gba"), ("pcsx_rearmed", "psx")] {
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

    /// The boot options win over what was saved or curated for the same key, and the other options stay.
    #[test]
    fn the_boot_options_replace_saved_values_of_the_same_key() {
        let mut options: Vec<(&str, &str)> = vec![("gpsp_boot_mode", "game"), ("gpsp_color_correction", "off")];
        apply(&mut options, &[("gpsp_bios", "official"), ("gpsp_boot_mode", "bios")]);
        assert_eq!(options.iter().filter(|(k, _)| *k == "gpsp_boot_mode").collect::<Vec<_>>(), [&("gpsp_boot_mode", "bios")]);
        assert!(options.contains(&("gpsp_color_correction", "off")) && options.contains(&("gpsp_bios", "official")));
    }
}
