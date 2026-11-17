//! Android's player and Options → Picture → Resolution, Earlier states and Restore defaults: its picture file
//! (`android/src/picture.rs`) keeps the choices, and the core gets the Resolution at the start by the rule pocket-core
//! follows on Linux (`render_scale::for_start`, `StartOption::place`).

#[path = "../android/src/picture.rs"]
#[allow(dead_code)]
mod picture;

use crate::render_scale::{range, Class};
use crate::testutil::TestDir;
use std::path::Path;

const KEY: &str = "0123456789abcdef";

/// The Mini V2 (SM8250) for each 3D system: nothing chosen gives the handheld's default to the core, a choice in the
/// player's Picture page goes in at the next start over an old Emulator-page value, for this console or this game.
#[test]
fn the_chosen_resolution_is_the_core_s_option_at_the_next_start() {
    let dir = TestDir::new("android-resolution");
    for (system, core, key, original, doubled) in [
        ("n64", "libmupen64plus_next_libretro.so", "mupen64plus-EnableNativeResFactor", "1", "2"),
        ("psp", "libppsspp_libretro.so", "ppsspp_internal_resolution", "480x272", "960x544"),
        ("dreamcast", "libflycast_libretro.so", "flycast_internal_resolution", "640x480", "1280x960"),
        ("psx", "libpcsx_rearmed_libretro.so", "pcsx_rearmed_neon_enhancement_enable", "disabled", "enabled"),
    ] {
        let r = range(Class::Sm8250, system);
        let core = Path::new(core);
        let start = |s: &picture::Settings, saved: &[(&'static str, &'static str)]| {
            let o = s.start_resolution(r, core).expect("the row is there");
            let mut options = saved.to_vec();
            o.place(&mut options);
            options.iter().find(|(k, _)| *k == key).map(|(_, v)| v.to_string())
        };
        let mut s = picture::Settings::load(&dir, system, KEY);
        let default = if r.unwrap().default == 2 { doubled } else { original };
        assert_eq!(start(&s, &[]).as_deref(), Some(default), "{system}: nothing chosen, the handheld's default");
        // An Emulator-page value from before the row stays until the row is changed (Linux's rule).
        assert_eq!(start(&s, &[(key, "old")]).as_deref(), Some("old"), "{system}");

        for (factor, want) in [(1, original), (2, doubled)] {
            s.set_resolution(factor).unwrap();
            // The next start reads the file again.
            let again = picture::Settings::load(&dir, system, KEY);
            assert_eq!(again.resolution, Some(factor), "{system}");
            assert_eq!(start(&again, &[]).as_deref(), Some(want), "{system} {factor}x");
            assert_eq!(start(&again, &[(key, "old")]).as_deref(), Some(want), "{system} {factor}x over an Emulator-page value");
        }
        // Not measured here (another chip): no row, nothing set.
        assert!(s.start_resolution(None, core).is_none(), "{system}");
    }
}

/// Earlier states starts as Linux's does (Off for the big states) and a choice is kept; Restore defaults takes every
/// choice of the console and the game away.
#[test]
fn earlier_and_restore_defaults_keep_and_clear_the_choices() {
    let dir = TestDir::new("android-restore");
    let mut s = picture::Settings::load(&dir, "n64", KEY);
    assert!(!s.earlier_on("n64") && s.earlier_on("gba"));
    s.set_earlier(true).unwrap();
    s.set_resolution(1).unwrap();
    s.change(3, 1).unwrap(); // Keep for: this game
    s.set_earlier(true).unwrap();
    let saved = picture::Settings::load(&dir, "n64", KEY);
    assert!(saved.earlier_on("n64") && saved.for_game());
    assert_eq!(saved.resolution, Some(1));

    s.restore_defaults().unwrap();
    assert!(!s.earlier_on("n64") && !s.for_game() && s.resolution.is_none());
    let fresh = picture::Settings::load(&dir, "n64", KEY);
    assert!(!fresh.earlier_on("n64") && fresh.resolution.is_none() && !fresh.for_game() && !fresh.over_game);
    assert_eq!((fresh.scaling, fresh.frame, fresh.effect), (0, 0, 0));
    assert!(!dir.join("picture/systems/n64.json").exists() && !dir.join(format!("picture/games/{KEY}.json")).exists());
}
