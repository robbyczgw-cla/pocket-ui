//! The start's word when a game plays with something else than what was chosen, or than pocket-ui would normally pick,
//! because that isn't here (Alex, 25.11.2026, for 0.8: no system swaps its emulator silently). Xbox went to a crashing
//! xemu from 0.2.1 to 0.7.2 because hakuX was never in the packages, and nobody was told.
//!
//! Said for: a saved game or console choice the resolver passes over (`emu_choice::dropped_choice`), Saturn without its
//! BIOS (YabaSanshiro's built-in one), PICO-8 carts without the official player (FAKE-08), H700 GBA without pocket-ui's
//! gpSP (mGBA), and a saved Mode 7 HD without bsnes-hd (Snes9x). The documented intentions stay quiet: Jeanne d'Arc on
//! PPSSPP standalone (`emu_choice::game_default`), the Brick's platform cores (`brick_profile`) and the GBA choice per game
//! (mGBA kept for a game's mGBA moments, `h700_gba::default_core`); so does a pocket-core start that failed before
//! (`fallback::notice` says that one).
//!
//! How it shows (docs/DESIGN.md "Messages"): a pocket-core game shows it over its first seconds as pocket-core's note
//! ([`ENV`], `Hud::note`), which goes by itself; an emulator of the system's own gets a notice on the shelf that is still
//! there when the game ends, once per pocket-ui run (`App::launch_prepared`).

use std::path::Path;

use crate::emu_choice::{self, Dropped, From, Kind, Runner};

/// pocket-core shows this variable's text as a note when the game's window is up.
pub const ENV: &str = "POCKETUI_START_NOTE";

/// How a player knows what plays now: the core's name in pocket-core, RetroArch with its core, a standalone's name.
pub fn plays_with(r: &Runner) -> String {
    match r.kind() {
        Kind::PocketCore => emu_choice::core_label(&r.core),
        Kind::RetroArch => format!("RetroArch ({})", emu_choice::core_label(&r.core)),
        Kind::Standalone => emu_choice::emulator_label(&r.emulator),
    }
}

/// The line for a saved choice the resolver passed over; `now` is what plays instead.
pub fn dropped(d: &Dropped, now: &Runner) -> String {
    let now = plays_with(now);
    match d {
        Dropped::Emulator(e) => unavailable(&emu_choice::emulator_label(e), &now),
        Dropped::Core(c) => unavailable(&emu_choice::core_label(c), &now),
        Dropped::FileType(ext) if !ext.is_empty() => format!("pocket-core doesn't open .{ext} files, so this game plays with {now}."),
        Dropped::FileType(_) => format!("pocket-core doesn't open this file, so this game plays with {now}."),
    }
}

/// Saturn without its BIOS on a standalone emulator: what plays, and that a BIOS brings pocket-core's full pause menu
/// (Save, Load and the rest; the standalone YabaSanshiro has only Options).
pub fn saturn_without_bios(now: &str) -> String {
    format!("No Saturn BIOS in roms/bios, so this game plays with {now} and its built-in BIOS. With a Saturn BIOS there, it plays in pocket-core with the full pause menu.")
}

fn unavailable(was: &str, now: &str) -> String {
    format!("{was} isn't available here, so this game plays with {now}.")
}

/// What the platform checks read: the card's BIOS folder, whether this is an H700 row and pocket-ui's gpSP is beside the
/// programs (`h700_gba::bundled_path`), and whether ROCKNIX's official PICO-8 start is there at all.
pub struct Here<'a> {
    pub bios_dir: &'a Path,
    pub h700: bool,
    pub gpsp_bundled: bool,
    pub pico8_launcher: bool,
    /// The saved Mode 7 asks for HD while bsnes-hd isn't there (`mode7_hd::chosen_without_core`, read only for SNES).
    pub mode7_without_core: &'a dyn Fn() -> bool,
}

/// The platform's own swaps: what `now` (the resolver's runner, `from` its reason) stands in for, or None. Only for the
/// default (a player's explicit choice is theirs).
pub fn platform(system: &str, now: &Runner, from: From, here: &Here) -> Option<String> {
    if from != From::Default {
        return None;
    }
    let pocket = now.is_pocket_core();
    if system == "saturn" && !pocket {
        let s = crate::hostsys::find(system)?;
        let hosted = crate::hostsys::host_core(system).is_some();
        if hosted && s.default_without_bios && s.missing_bios(here.bios_dir).is_some() {
            return Some(saturn_without_bios(&plays_with(now)));
        }
        return None;
    }
    if system == "pico-8" && now.core == "fake08" && here.pico8_launcher {
        return Some("The official PICO-8 isn't in roms/pico-8, so this cart plays with FAKE-08.".into());
    }
    if crate::h700_gba::is_gba(system) && pocket && now.core == "mgba" && here.h700 && !here.gpsp_bundled {
        return Some(unavailable("gpSP", &plays_with(now)));
    }
    if crate::mode7_hd::is_snes(system) && pocket && now.core == "snes9x" && (here.mode7_without_core)() {
        return Some("bsnes-hd isn't available here, so this game plays with Snes9x without Mode 7 HD.".into());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;

    fn here<'a>(bios: &'a Path, f: &'a dyn Fn() -> bool) -> Here<'a> {
        Here { bios_dir: bios, h700: false, gpsp_bundled: false, pico8_launcher: true, mode7_without_core: f }
    }

    #[test]
    fn a_dropped_choice_names_what_was_chosen_and_what_plays() {
        let ra = Runner::new("retroarch", "mgba");
        assert_eq!(
            dropped(&Dropped::Emulator("mednafen".into()), &ra),
            "Mednafen isn't available here, so this game plays with RetroArch (mGBA)."
        );
        assert_eq!(
            dropped(&Dropped::Core("gpsp".into()), &Runner::new("pocket-core", "mgba")),
            "gpSP isn't available here, so this game plays with mGBA."
        );
        assert_eq!(dropped(&Dropped::FileType("rar".into()), &ra), "pocket-core doesn't open .rar files, so this game plays with RetroArch (mGBA).");
        for d in [Dropped::Emulator("x".into()), Dropped::Core("y".into()), Dropped::FileType(String::new())] {
            let line = dropped(&d, &ra);
            assert!(line.ends_with('.') && !line.contains('—') && !line.contains('–'), "{line}");
        }
    }

    /// Every platform swap of the brief says something, and only for the default: a player's own choice stays quiet.
    #[test]
    fn each_platform_swap_is_said_and_only_for_the_default() {
        let d = TestDir::new("swap-note");
        let bios = d.join("roms/bios");
        std::fs::create_dir_all(&bios).unwrap();
        let no = || false;
        let yes = || true;
        let yaba = Runner::new("yabasanshiro", "yabasanshiro");
        let saturn = platform("saturn", &yaba, From::Default, &here(&bios, &no));
        assert!(saturn.as_deref().is_some_and(|s| s.contains("Saturn BIOS") && s.contains("YabaSanshiro")), "{saturn:?}");
        assert_eq!(platform("saturn", &yaba, From::Console, &here(&bios, &no)), None, "the player chose it");
        let fake = Runner::new("pocket-core", "fake08");
        assert!(platform("pico-8", &fake, From::Default, &here(&bios, &no)).is_some_and(|s| s.contains("FAKE-08")));
        assert!(platform("pico-8", &Runner::new("retroarch", "fake08"), From::Default, &here(&bios, &no)).is_some());
        let mut brick = here(&bios, &no);
        brick.pico8_launcher = false;
        assert_eq!(platform("pico-8", &fake, From::Default, &brick), None, "no official PICO-8 to have on this card at all");
        let snes = Runner::new("pocket-core", "snes9x");
        assert!(platform("snes", &snes, From::Default, &here(&bios, &yes)).is_some_and(|s| s.contains("bsnes-hd")));
        assert_eq!(platform("snes", &snes, From::Default, &here(&bios, &no)), None, "Original chosen, or bsnes-hd there");
        let mut h700 = here(&bios, &no);
        h700.h700 = true;
        let gba = platform("gba", &Runner::new("pocket-core", "mgba"), From::Default, &h700);
        assert_eq!(gba.as_deref(), Some("gpSP isn't available here, so this game plays with mGBA."));
        h700.gpsp_bundled = true;
        assert_eq!(platform("gba", &Runner::new("pocket-core", "mgba"), From::Default, &h700), None, "gpSP there: mGBA is the game's own (its moments)");
        assert_eq!(platform("gba", &Runner::new("pocket-core", "mgba"), From::Default, &here(&bios, &no)), None, "not an H700 row");
        assert_eq!(platform("psp", &Runner::new("ppsspp", "ppsspp-sa"), From::Default, &here(&bios, &no)), None, "Jeanne d'Arc's PPSSPP is intended");
    }
}
