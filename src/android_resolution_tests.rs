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
        let mut s = picture::Settings::load(&dir, system, std::path::Path::new(KEY), Default::default());
        let default = if r.unwrap().default == 2 { doubled } else { original };
        assert_eq!(start(&s, &[]).as_deref(), Some(default), "{system}: nothing chosen, the handheld's default");
        // An Emulator-page value from before the row stays until the row is changed (Linux's rule).
        assert_eq!(start(&s, &[(key, "old")]).as_deref(), Some("old"), "{system}");

        for (factor, want) in [(1, original), (2, doubled)] {
            s.set_resolution(factor).unwrap();
            // The next start reads the file again.
            let again = picture::Settings::load(&dir, system, std::path::Path::new(KEY), Default::default());
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
    let mut s = picture::Settings::load(&dir, "n64", std::path::Path::new(KEY), Default::default());
    assert!(!s.earlier_on("n64") && s.earlier_on("gba"));
    s.set_earlier(true).unwrap();
    s.set_resolution(1).unwrap();
    // Keep for starts on this game, as Linux's menu with nothing saved.
    s.set_earlier(true).unwrap();
    let saved = picture::Settings::load(&dir, "n64", std::path::Path::new(KEY), Default::default());
    assert!(saved.earlier_on("n64") && saved.for_game());
    assert_eq!(saved.resolution, Some(1));

    s.restore_defaults().unwrap();
    assert!(!s.earlier_on("n64") && s.for_game() && s.resolution.is_none());
    let fresh = picture::Settings::load(&dir, "n64", std::path::Path::new(KEY), Default::default());
    assert!(!fresh.earlier_on("n64") && fresh.resolution.is_none() && fresh.for_game() && !fresh.over_game);
    // Linux's defaults: Integer, no frame, Clean.
    assert_eq!((fresh.scaling, fresh.frame, fresh.variant("n64").1), (1, 0, crate::video::Effect::None));
    assert!(!dir.join("game-settings/n64.json").exists() && !dir.join(format!("game-settings/n64/{KEY}.json")).exists());
}

/// Options › Controls on Android keeps its choices where Linux keeps them (`ctl.*` in `game-settings`, as
/// `emu_menu::control_step`): for this game, or after Keep for → This console for every game of the console; Linux's own
/// `options::GameSettings` reads the very values (one file for both), and Restore defaults takes them away. Until 18.11.2026
/// the player kept them in a file of its own, per game only.
#[test]
fn the_button_choices_are_kept_in_linux_s_files_for_the_game_or_the_console() {
    use crate::controls::{ControlsChange, ControlsPage, Mapping, KEY_PREFIX};
    let dir = TestDir::new("android-controls-keep");
    let rom = Path::new("/sdcard/ROMs/snes/Super Metroid.sfc");
    let load = |rom: &Path| picture::Settings::load(&dir, "snes", rom, Default::default());
    // A one step on: it presses B, which swaps (`ControlsPage::step`), kept as the player's step_control keeps it.
    let mut s = load(rom);
    let mut page = ControlsPage { mapping: Mapping::from_values("snes", &s.values()), targets: crate::controls::targets_from(&[]), system: "snes".into() };
    let ControlsChange::Set(entries) = page.step(page.stick_rows(), 1) else { panic!("a button row") };
    let set: Vec<(String, String)> = entries.iter().map(|(k, v, _)| (format!("{KEY_PREFIX}{k}"), v.clone())).collect();
    s.set_controls(&set, &[], false).unwrap();
    assert!(s.for_game(), "Keep for starts on this game, as Linux's");
    // The next start of this game has the choice; Linux's settings of the same game read it too.
    let again = load(rom);
    assert_eq!(Mapping::from_values("snes", &again.values()), page.mapping);
    let linux = crate::options::GameSettings::load(&dir, "snes", rom);
    assert_eq!(Mapping::from_values("snes", &linux.values), page.mapping, "one file for both");
    // Another game of the console: its own buttons, until Keep for says This console.
    let other = Path::new("/sdcard/ROMs/snes/Chrono Trigger.sfc");
    assert_eq!(Mapping::from_values("snes", &load(other).values()), Mapping::defaults("snes"));
    let mut s = load(rom);
    s.change(picture::ROWS - 1, 1).unwrap();
    assert!(!s.for_game());
    // This game's file still wins for itself (Linux's `GameSettings::load`); a game without one takes the console's.
    assert_eq!(Mapping::from_values("snes", &load(other).values()), page.mapping, "the console's choice reaches its other games");
    // Reset controls takes every ctl.* away; Restore defaults the files.
    let mut s = load(other);
    s.set_controls(&[], &[], true).unwrap();
    assert!(!s.values().keys().any(|k| k.starts_with(KEY_PREFIX)));
    let mut s = load(rom);
    s.restore_defaults().unwrap();
    assert_eq!(Mapping::from_values("snes", &load(rom).values()), Mapping::defaults("snes"));
}
