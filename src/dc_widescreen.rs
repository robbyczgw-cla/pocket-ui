//! Options → Picture → Widescreen for the Dreamcast: Off, Patch or Hack, the same on Linux and in Android's player. Both
//! are Flycast's own (`shell/libretro/libretro_core_options.h`, flyinghead/flycast c3763d8, the core pocket-ui ships):
//!
//! - Patch: `flycast_widescreen_cheats` ("Widescreen Cheats"): Flycast's list of per-game memory patches
//!   (`core/cheats.cpp` `widescreen_cheats`, some 230 entries), made for each game, clean. A game without one stays 4:3.
//!   With a patch Flycast stretches its picture to 16:9 (`ScreenStretching` 134, `core/emulator.cpp`).
//! - Hack: `flycast_widescreen_hack` ("Widescreen Hack"): every 3D game draws outside its 4:3, out to 16:9
//!   (`getOutputFramebufferAspectRatio`, `core/rend/transform_matrix.cpp`); the revealed sides can show gaps or pop-in.
//!
//! Never both: with a patch and the hack Flycast multiplies the two (16:9 × 1.34, a 2.4:1 picture). The core reports
//! the new shape itself (`retro_resize_renderer` → SET_GEOMETRY) and both players draw the core's aspect
//! (`hostsys::display_aspect`, Android's `picture.place`), so the picture is 16:9 at the console's Aspect scaling.
//!
//! The Android app's Flycast is the same core under its historical `reicast_` option names (`core_options::android_curation`):
//! the start sets both names. Flycast turns the hack off itself for vertical games (`rotate_game`) and both under
//! RetroAchievements hardcore. Takes effect the next time the game starts (the patch needs a fresh start).

/// The Picture row's key in the game settings files.
pub const KEY: &str = "dc_widescreen";

/// The row's values: Off, Patch, Hack.
pub const VALUES: [&str; 3] = ["off", "patch", "hack"];

/// The row's line (Options → Picture, Linux's and Android's).
pub const DESC: &str = "More of the game at the sides, in 16:9. Patch: a fix made for the game, if it has one. Hack: every 3D game, but the sides can show gaps. Takes effect the next time the game starts.";

/// The consoles the row is for: the Dreamcast (ROCKNIX's folder name).
pub fn is_dreamcast(system: &str) -> bool {
    system == "dreamcast"
}

/// Whether the row is there for `system` on this handheld: the Dreamcast wherever pocket-ui's player plays it with Flycast.
/// On ROCKNIX the H700 and other small chips leave the Dreamcast on the system's own emulator (`hostsys::route_on`,
/// `H700_STAYS_ON_ROCKNIX`), where the row would set nothing; dArkOS plays it with pocket-core's Flycast on every board
/// (deploy/darkos/systems.py).
pub fn applies(system: &str) -> bool {
    is_dreamcast(system) && plays_here()
}

#[cfg(all(target_os = "linux", not(target_os = "android")))]
fn plays_here() -> bool {
    crate::device::darkos_port() || !crate::device::current().model.is_some_and(|m| m.small_chip())
}
#[cfg(not(all(target_os = "linux", not(target_os = "android"))))]
fn plays_here() -> bool {
    true
}

/// The mode a start uses: the saved choice where the row is there and it is one of the row's, else Off.
pub fn mode(applies: bool, chosen: Option<&str>) -> &'static str {
    match chosen {
        Some(v) if applies => VALUES.iter().copied().find(|x| *x == v).unwrap_or("off"),
        _ => "off",
    }
}

/// The core options of a start for the mode, by core (`core_options::core_id`): Flycast's two switches under both of its
/// names, one of them on at most. Every other core: nothing.
pub fn core_options(core: &str, mode: &str) -> Vec<(&'static str, String)> {
    if core != "flycast" {
        return Vec::new();
    }
    let on = |m: &str| if mode == m { "enabled" } else { "disabled" }.to_string();
    vec![
        ("flycast_widescreen_cheats", on("patch")),
        ("flycast_widescreen_hack", on("hack")),
        ("reicast_widescreen_cheats", on("patch")),
        ("reicast_widescreen_hack", on("hack")),
    ]
}

/// The core option keys the row decides (the Emulator page leaves them out, one place for them).
pub const CORE_KEYS: &[&str] = &["flycast_widescreen_cheats", "flycast_widescreen_hack", "reicast_widescreen_cheats", "reicast_widescreen_hack"];

thread_local! {
    /// This thread's next core start carries the row's options ([`place`] with a Dreamcast game's), read once by
    /// [`take_row_set`]. Both players place the options and load the core on the same thread.
    static ROW_SET: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Whether the core starting now got its widescreen options from the row (a Dreamcast game, [`place`]): then the Emulator
/// page leaves them out (`OptionSet::row_decides`). A NAOMI or Atomiswave game on the same Flycast keeps them, whatever
/// keys its saved or RetroArch options hold. Clears the mark.
pub fn take_row_set(core: &str) -> bool {
    ROW_SET.with(|r| r.replace(false)) && core == "flycast"
}

/// Puts the mode's options first in a start's core options: the row's choice beats a saved Emulator-page value of the
/// same key (Mode 7's rule, `mode7_hd::place`). Marks the start as the row's when there are any ([`take_row_set`]).
pub fn place<'a>(start: &'a [(&'static str, String)], options: &mut Vec<(&'a str, &'a str)>) {
    ROW_SET.with(|r| r.set(!start.is_empty()));
    crate::mode7_hd::place(start, options);
}

/// The options of a start of `system`'s game with this core, from the row's saved value (the game's, else the console's).
pub fn for_start(system: &str, core: &str, chosen: Option<&str>) -> (&'static str, Vec<(&'static str, String)>) {
    let applies = applies(system);
    let mode = mode(applies, chosen);
    (mode, if applies { core_options(core, mode) } else { Vec::new() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn the_row_is_for_the_dreamcast_only() {
        assert!(applies("dreamcast"));
        for other in ["naomi", "atomiswave", "saturn", "psx", "n64"] {
            assert!(!applies(other), "{other}");
        }
    }

    /// The row's words are in every language of the interface (the journey test walks six of them through the menu).
    #[test]
    fn the_row_s_words_are_translated_in_every_language() {
        for lang in ["de", "es", "pt", "fr", "it", "ko", "zh", "vi", "ja"] {
            let tsv = std::fs::read_to_string(format!("{}/assets/i18n/{lang}.tsv", env!("CARGO_MANIFEST_DIR"))).unwrap();
            for en in ["Widescreen", "Off", "Patch", "Hack", DESC] {
                let line = tsv.lines().find_map(|l| l.split_once('\t').filter(|(k, _)| *k == en).map(|(_, v)| v));
                assert!(line.is_some_and(|v| !v.trim().is_empty()), "{lang}: no line for {en:?}");
            }
        }
    }

    #[test]
    fn a_start_uses_the_saved_choice_else_off() {
        assert_eq!(mode(true, None), "off", "nobody chose: the console's 4:3");
        assert_eq!(mode(true, Some("patch")), "patch");
        assert_eq!(mode(true, Some("hack")), "hack");
        assert_eq!(mode(true, Some("junk")), "off");
        assert_eq!(mode(false, Some("hack")), "off", "no row here: nothing changes");
    }

    /// Never both switches on (Flycast multiplies them into a 2.4:1 picture), both names set (the Android app's
    /// Flycast reads `reicast_`), and every key one the Emulator page leaves to the row.
    #[test]
    fn each_mode_sets_one_switch_under_both_names() {
        let get = |mode| core_options("flycast", mode).into_iter().collect::<BTreeMap<_, _>>();
        for (mode, patch, hack) in [("off", "disabled", "disabled"), ("patch", "enabled", "disabled"), ("hack", "disabled", "enabled")] {
            let o = get(mode);
            for ns in ["flycast", "reicast"] {
                assert_eq!(o[format!("{ns}_widescreen_cheats").as_str()], patch, "{mode} {ns}");
                assert_eq!(o[format!("{ns}_widescreen_hack").as_str()], hack, "{mode} {ns}");
            }
            assert_eq!(o.keys().copied().collect::<Vec<_>>(), { let mut k = CORE_KEYS.to_vec(); k.sort(); k });
        }
        assert!(core_options("mupen64plus_next", "hack").is_empty());
        assert_eq!(for_start("naomi", "flycast", Some("hack")), ("off", Vec::new()), "not the arcade boards: their Emulator page keeps it");
    }

    #[test]
    fn the_row_s_choice_beats_a_saved_emulator_value() {
        let (_, start) = for_start("dreamcast", "flycast", Some("hack"));
        let mut o = vec![("flycast_widescreen_hack", "disabled"), ("flycast_internal_resolution", "1280x960")];
        place(&start, &mut o);
        assert_eq!(o.iter().filter(|(k, _)| *k == "flycast_widescreen_hack").collect::<Vec<_>>(), [&("flycast_widescreen_hack", "enabled")]);
        assert!(o.contains(&("flycast_internal_resolution", "1280x960")));
        assert!(take_row_set("flycast"), "a Dreamcast start: the Emulator page leaves the switches to the row");
        assert!(!take_row_set("flycast"), "read once: the next start is its own");
    }

    /// A NAOMI or Atomiswave game on the same Flycast keeps its Emulator switches: the row isn't there for it, even when
    /// its options (RetroArch's file, say) hold all four widescreen keys.
    #[test]
    fn arcade_games_keep_the_emulator_switches() {
        let (_, dc) = for_start("dreamcast", "flycast", None);
        let mut o: Vec<(&str, &str)> = dc.iter().map(|(k, v)| (*k, v.as_str())).collect();
        let (_, start) = for_start("naomi", "flycast", Some("hack"));
        place(&start, &mut o);
        assert!(!take_row_set("flycast"), "every key there, but not the row's start");
        place(&dc, &mut o);
        assert!(!take_row_set("mupen64plus_next"), "another core");
        assert!(!take_row_set("flycast"), "the mark went with the read");
    }

    /// Changing the row in the game's pause menu (`GameSettings::step`, kept for this game, or for the console) decides
    /// the next start's Flycast options; the console's other games keep theirs.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_change_in_the_pause_menu_decides_the_next_start() {
        use crate::options::{setting, GameSettings};
        let dir = crate::testutil::TestDir::new("dc-widescreen-start");
        let (rom, other) = (std::path::Path::new("/roms/dreamcast/Sonic Adventure (USA).chd"), std::path::Path::new("/roms/dreamcast/Other.chd"));
        let start = |rom| {
            let s = GameSettings::load(&dir, "dreamcast", rom);
            let (mode, o) = for_start("dreamcast", "flycast", s.values.get(KEY).map(String::as_str));
            (mode, o.into_iter().find(|(k, _)| *k == "flycast_widescreen_hack").map(|(_, v)| v))
        };
        assert_eq!(start(rom), ("off", Some("disabled".into())), "nothing chosen: Off");
        let row = setting(KEY).unwrap();
        let step = |rom, delta, game: bool| {
            let mut s = GameSettings::load(&dir, "dreamcast", rom);
            let changed = s.step(row, delta);
            assert!(if game { s.save_for_game() } else { s.save_for_console() });
            changed
        };
        assert_eq!(step(rom, 1, true), vec![(KEY, "patch".to_string())]);
        assert_eq!(start(rom).0, "patch");
        assert_eq!(step(rom, 1, true), vec![(KEY, "hack".to_string())]);
        assert_eq!(start(rom), ("hack", Some("enabled".into())));
        assert_eq!(start(other).0, "off", "the console's other games keep theirs");
        assert_eq!(step(rom, 1, true), vec![(KEY, "off".to_string())], "round to Off");
        assert_eq!(start(rom).0, "off");
        // Kept for the console: every Dreamcast game without its own choice starts with it.
        assert_eq!(step(other, 1, false), vec![(KEY, "patch".to_string())]);
        assert_eq!(start(other).0, "patch");
        assert_eq!(start(std::path::Path::new("/roms/dreamcast/Third.chd")).0, "patch", "the console's choice");
        assert_eq!(start(rom).0, "off", "a game's own choice beats the console's");
    }
}
