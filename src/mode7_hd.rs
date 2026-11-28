//! Options → Picture → Mode 7 for the Super Nintendo: Original, HD, or HD widescreen. Mode 7 is the SNES's turned and
//! tilted background (Super Mario Kart's track, F-Zero's, Pilotwings' ground); HD draws it at a multiple of the console's
//! resolution, the very computation the SNES does, only finer (no new pictures, no filter). HD widescreen also draws the
//! Mode 7 scenes wider than the console's 4:3, out to 16:9; other scenes keep their 4:3 with black at the sides.
//!
//! The cores: bsnes-hd (libretro `bsnes_hd_beta`, a bsnes fork) has all three. On a ROCKNIX card it is in the image
//! (`/usr/lib/libretro/bsnes_hd_beta_libretro.so`); pocket-core plays SNES with Snes9x, which has no HD Mode 7, so HD
//! and HD widescreen start the game with bsnes-hd instead ([`linux_core`]). Original keeps Snes9x. The bsnes of the
//! Android app has HD Mode 7 built in but no widescreen: there the row offers Original and HD.
//!
//! Like Resolution (`render_scale`), only on the chip classes measured for it, with the factor of the class; it takes
//! effect the next time the game starts. Widescreen only where the screen is wide: on the Mini V2's 1240x1080 or a 4:3
//! screen a 16:9 picture would only be smaller.

use crate::render_scale::Class;

/// The Picture row's key in the game settings files.
pub const KEY: &str = "mode7";

/// The row's values: Original, HD, HD widescreen.
pub const VALUES: [&str; 3] = ["original", "hd", "wide"];

/// The row's line (Options → Picture, Linux's and Android's).
pub const DESC: &str = "The turning, tilting floors of Super Mario Kart, F-Zero and Pilotwings. HD draws them sharper and plays the game with bsnes-hd instead of Snes9x; HD widescreen also fills a wide screen's sides. Takes effect the next time the game starts.";

/// The row's line on Android, whose bsnes has HD built in (no other emulator, no widescreen).
pub const ANDROID_DESC: &str = "The turning, tilting floors of Super Mario Kart, F-Zero and Pilotwings. HD draws them sharper. Takes effect the next time the game starts.";

/// The core pocket-core starts SNES games with for HD and HD widescreen on a ROCKNIX card.
pub const LINUX_CORE: &str = "bsnes_hd_beta";

/// Where pocket-ui's own bsnes-hd lies, beside the programs in the version folder as gpSP and DSperate do
/// (`scripts/bsnes-hd-files.sh`, every ROCKNIX package and the app update): a card whose image has none still has HD.
/// The packaging scripts look for this name in the program to require the core (`make-install-zip.sh --bsnes-hd`).
pub const BUNDLED: &str = "cores/bsnes_hd_beta_libretro.so";

/// [`BUNDLED`] beside the running program (`POCKETUI_BSNES_HD_CORE` for tests).
pub fn bundled_path() -> std::path::PathBuf {
    std::env::var_os("POCKETUI_BSNES_HD_CORE").map(std::path::PathBuf::from).unwrap_or_else(|| {
        std::env::current_exe().ok().and_then(|p| p.parent().map(std::path::Path::to_path_buf)).unwrap_or_default().join(BUNDLED)
    })
}

/// The SNES folders (ROCKNIX's `snes`, `sfc` and `snesh`).
pub fn is_snes(system: &str) -> bool {
    matches!(system, "snes" | "sfc" | "snesh")
}

/// What a handheld offers: the factor HD draws Mode 7 at, and whether HD widescreen is there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Offer {
    pub scale: u8,
    pub wide: bool,
}

/// The offer of a chip class on a screen of this landscape size; None: no row (unmeasured chips keep the console's Mode 7).
///
/// Factors: the Mini V2 under Android (17.11.2026, docs/HOST.md "Mode 7 HD"): 2x and 3x both at full speed in F-Zero's and
/// Super Mario Kart's races (core p99 12.6 and 11.8 ms, no repeated pictures), but 3x heated the chip by 13-18 °C in 150 s
/// against 3-4 at 2x: the SM8250 gets 2x. The SM8550 3x (estimated from headless frame times, not yet on a handheld).
pub fn offer(class: Option<Class>, screen: (u32, u32)) -> Option<Offer> {
    let scale = match class? {
        Class::Sm8250 => 2,
        Class::Sm8550 => 3,
    };
    Some(Offer { scale, wide: wide_screen(screen) })
}

/// A screen wide enough for a 16:9 picture to be larger than the 4:3 one: 16:10 and wider.
pub fn wide_screen((w, h): (u32, u32)) -> bool {
    h > 0 && w * 10 >= h * 16
}

/// This handheld's offer: the chip class as Resolution reads it (`render_scale::here_class`), the screen the board's
/// (`device::current`). Android passes its window's size to [`offer`] itself.
pub fn here() -> Option<Offer> {
    #[cfg(test)]
    if let Some(o) = TEST_OFFER.with(|t| t.get()) {
        return o;
    }
    // Android: the app's bsnes has no widescreen, so the screen's shape doesn't matter there (Original and HD).
    #[cfg(target_os = "android")]
    let screen = (1240, 1080);
    #[cfg(not(target_os = "android"))]
    let screen = crate::device::current().screen();
    offer(crate::render_scale::here_class(), screen).filter(|_| available())
}

/// The row's offer for a console on this handheld: [`here`] for the Super Nintendo, None for every other console.
pub fn here_for(system: &str) -> Option<Offer> {
    if is_snes(system) { here() } else { None }
}

#[cfg(test)]
thread_local! {
    /// Tests: this thread's handheld offers this (Some(None): a handheld without the row), whatever the machine is.
    static TEST_OFFER: std::cell::Cell<Option<Option<Offer>>> = const { std::cell::Cell::new(None) };
}

/// Tests: run `f` as on a handheld that offers `offer` (`here`).
#[cfg(test)]
pub fn with_offer<T>(offer: Option<Offer>, f: impl FnOnce() -> T) -> T {
    TEST_OFFER.with(|t| t.set(Some(offer)));
    let out = f();
    TEST_OFFER.with(|t| t.set(None));
    out
}

/// Whether a core with HD Mode 7 is there: the app's bsnes on Android; on a card bsnes-hd in the image, installed, or
/// pocket-ui's own beside the programs ([`bundled_path`], `store::core_path`).
pub fn available() -> bool {
    #[cfg(target_os = "android")]
    {
        true
    }
    #[cfg(not(any(target_os = "android", target_os = "linux")))]
    {
        false
    }
    #[cfg(all(target_os = "linux", not(target_os = "android")))]
    {
        static THERE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        *THERE.get_or_init(|| crate::store::core_path(LINUX_CORE).is_file())
    }
}

/// Whether the row offers `value` with this offer.
pub fn offers(offer: Option<Offer>, value: &str) -> bool {
    match (offer, value) {
        (None, _) => false,
        (Some(_), "original" | "hd") => true,
        (Some(o), "wide") => o.wide,
        _ => false,
    }
}

/// The mode a start uses: the saved choice where the handheld offers it, else Original (a choice made on a wide screen,
/// the card moved to a Mini V2: HD, not a smaller picture).
pub fn mode(offer: Option<Offer>, chosen: Option<&str>) -> &'static str {
    match chosen {
        _ if offer.is_none() => "original",
        Some("wide") if offers(offer, "wide") => "wide",
        Some("wide" | "hd") => "hd",
        _ => "original",
    }
}

/// The core pocket-core starts an SNES game with on a ROCKNIX card: bsnes-hd for HD and HD widescreen where the image has
/// it, else the core it would use anyway. `installed` says whether bsnes-hd's file is there.
pub fn linux_core<'a>(system: &str, usual: &'a str, mode: &str, installed: bool) -> &'a str {
    if is_snes(system) && usual == "snes9x" && mode != "original" && installed {
        LINUX_CORE
    } else {
        usual
    }
}

/// The core pocket-core starts a card's game with: the Mode 7 the game (else its console) keeps in its settings file, as
/// the pause menu wrote it (`options::GameSettings`), turned into the core ([`linux_core`]); `offer` is this handheld's
/// for the console ([`here_for`]), `installed` whether bsnes-hd's file is there.
#[cfg(any(target_os = "linux", target_os = "emscripten", all(target_os = "android", feature = "handheld")))]
pub fn start_core<'a>(data: &std::path::Path, system: &str, rom: &std::path::Path, usual: &'a str, offer: Option<Offer>, installed: bool) -> &'a str {
    if offer.is_none() || !is_snes(system) {
        return usual;
    }
    let settings = crate::options::GameSettings::load(data, system, rom);
    linux_core(system, usual, mode(offer, settings.values.get(KEY).map(String::as_str)), installed)
}

/// A saved HD or HD widescreen the handheld would offer, while bsnes-hd isn't there: the start plays Original on Snes9x
/// ([`here`] hides the row then), and the start says so (`swap_note`) instead of dropping the choice quietly.
#[cfg(all(target_os = "linux", not(target_os = "android")))]
pub fn chosen_without_core(data: &std::path::Path, system: &str, rom: &std::path::Path) -> bool {
    if !is_snes(system) || available() {
        return false;
    }
    let Some(offer) = offer(crate::render_scale::here_class(), crate::device::current().screen()) else { return false };
    let settings = crate::options::GameSettings::load(data, system, rom);
    mode(Some(offer), settings.values.get(KEY).map(String::as_str)) != "original"
}

/// The core options of a start for the mode, by core (`core_options::core_id`): bsnes-hd gets scale and widescreen,
/// the Android app's bsnes the scale only (it has no widescreen). Every other core: nothing.
///
/// bsnes-hd's own defaults are HD 2x with widescreen on and 216 lines, so Original sets everything back: the console's
/// Mode 7, no widescreen, 224 lines as Snes9x and bsnes show them (bsnes-hd's "overscan on" crops 8 lines, as bsnes's
/// off does). Pixel aspect correction on: bsnes-hd reports the picture with the console's 8:7 pixels (256 x 224 a hair
/// narrower than Snes9x's 4:3) and its 16:9 is 48 columns a side, shown at 16:9; off, it would be 72 and wider than 16:9.
pub fn core_options(core: &str, mode: &str, scale: u8) -> Vec<(&'static str, String)> {
    let x = format!("{}x", scale.clamp(1, 8));
    match core {
        "bsnes_hd_beta" => {
            let (s, ws_mode, ws) = match mode {
                "wide" => (x.as_str(), "Mode 7", "16:9"),
                "hd" => (x.as_str(), "none", "none"),
                _ => ("disable", "none", "none"),
            };
            vec![
                ("bsnes_mode7_scale", s.to_string()),
                ("bsnes_mode7_wsMode", ws_mode.to_string()),
                ("bsnes_mode7_widescreen", ws.to_string()),
                ("bsnes_ppu_show_overscan", "ON".to_string()),
                ("bsnes_video_aspectcorrection", "ON".to_string()),
            ]
        }
        "bsnes" => vec![("bsnes_mode7_scale", if mode == "original" { "1x".to_string() } else { x })],
        _ => Vec::new(),
    }
}

/// The core option keys the row decides (the Emulator page leaves them out where the row is there).
pub const CORE_KEYS: &[&str] = &[
    "bsnes_mode7_scale",
    "bsnes_mode7_wsMode",
    "bsnes_mode7_widescreen",
    "bsnes_ppu_show_overscan",
    "bsnes_video_aspectcorrection",
];

/// Puts the mode's options first in a start's core options (a row's choice beats a saved Emulator-page value of the
/// same key, as Resolution's does).
pub fn place<'a>(start: &'a [(&'static str, String)], options: &mut Vec<(&'a str, &'a str)>) {
    options.retain(|(k, _)| !start.iter().any(|(s, _)| s == k));
    for (i, (k, v)) in start.iter().enumerate() {
        options.insert(i, (k, v.as_str()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_row_is_there_on_measured_chips_and_widescreen_only_on_wide_screens() {
        let mini_v2 = offer(Some(Class::Sm8250), (1240, 1080)).unwrap();
        assert!(!mini_v2.wide, "Mini V2: a 16:9 picture would be smaller than the 4:3 one");
        assert!(!offer(Some(Class::Sm8250), (1280, 960)).unwrap().wide, "Mini V1, a 4:3 screen");
        assert!(offer(Some(Class::Sm8250), (1920, 1080)).unwrap().wide, "Pocket 5, Flip 2");
        assert!(offer(Some(Class::Sm8550), (1920, 1080)).unwrap().wide, "Odin 2 family, Pocket 6");
        assert!(offer(Some(Class::Sm8250), (2400, 1080)).unwrap().wide, "a phone in landscape");
        assert!(wide_screen((1920, 1200)), "16:10");
        assert_eq!(offer(None, (1920, 1080)), None, "unmeasured chip: no row, the console's Mode 7");
        for o in [Some(mini_v2), Some(Offer { scale: 3, wide: true })] {
            assert!(offers(o, "original") && offers(o, "hd"));
            assert_eq!(offers(o, "wide"), o.unwrap().wide);
            assert!(!offers(o, "x"));
        }
        assert!(!offers(None, "original"));
    }

    #[test]
    fn a_choice_the_handheld_does_not_offer_starts_the_nearest_one() {
        let narrow = Some(Offer { scale: 2, wide: false });
        let wide = Some(Offer { scale: 2, wide: true });
        assert_eq!(mode(wide, Some("wide")), "wide");
        assert_eq!(mode(narrow, Some("wide")), "hd", "card moved from a Pocket 5 to a Mini V2: HD, not smaller");
        assert_eq!(mode(narrow, Some("hd")), "hd");
        assert_eq!(mode(narrow, None), "original", "nobody chose: the console's Mode 7");
        assert_eq!(mode(narrow, Some("junk")), "original");
        assert_eq!(mode(None, Some("hd")), "original", "card moved to an unmeasured handheld");
    }

    #[test]
    fn only_hd_switches_a_card_s_snes_game_to_bsnes_hd() {
        assert_eq!(linux_core("snes", "snes9x", "hd", true), LINUX_CORE);
        assert_eq!(linux_core("sfc", "snes9x", "wide", true), LINUX_CORE);
        assert_eq!(linux_core("snes", "snes9x", "original", true), "snes9x", "Original keeps Snes9x and its moments");
        assert_eq!(linux_core("snes", "snes9x", "hd", false), "snes9x", "an image without bsnes-hd: as before");
        assert_eq!(linux_core("gba", "mgba", "hd", true), "mgba");
        assert_eq!(linux_core("snes", "snes9x2010", "hd", true), "snes9x2010", "another core chosen on purpose stays");
    }

    #[test]
    fn each_mode_sets_bsnes_hd_fully_and_bsnes_its_scale() {
        let get = |core, mode| core_options(core, mode, 2).into_iter().collect::<std::collections::BTreeMap<_, _>>();
        let o = get("bsnes_hd_beta", "original");
        assert_eq!(o["bsnes_mode7_scale"], "disable", "the console's own Mode 7, not bsnes-hd's default 2x");
        assert_eq!(o["bsnes_mode7_wsMode"], "none", "not bsnes-hd's default widescreen");
        assert_eq!(o["bsnes_ppu_show_overscan"], "ON", "224 lines, as Snes9x");
        let h = get("bsnes_hd_beta", "hd");
        assert_eq!((h["bsnes_mode7_scale"].as_str(), h["bsnes_mode7_wsMode"].as_str()), ("2x", "none"));
        let w = get("bsnes_hd_beta", "wide");
        assert_eq!((w["bsnes_mode7_scale"].as_str(), w["bsnes_mode7_wsMode"].as_str(), w["bsnes_mode7_widescreen"].as_str()), ("2x", "Mode 7", "16:9"));
        assert_eq!(get("bsnes", "hd")["bsnes_mode7_scale"], "2x");
        assert_eq!(get("bsnes", "original")["bsnes_mode7_scale"], "1x");
        assert!(core_options("snes9x", "hd", 2).is_empty());
        for core in ["bsnes_hd_beta", "bsnes"] {
            for (k, _) in core_options(core, "wide", 3) {
                assert!(CORE_KEYS.contains(&k), "{k}");
            }
        }
    }

    /// Changing the row in the game's pause menu (`GameSettings::step`, kept for this game) decides the next start: HD
    /// starts bsnes-hd with its HD options, back to Original starts Snes9x again; the console's other games keep theirs;
    /// a handheld without the row (the card in an H700) keeps Snes9x whatever the file says.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_change_in_the_pause_menu_decides_the_next_start() {
        use crate::options::{setting, GameSettings};
        let dir = crate::testutil::TestDir::new("mode7-start");
        let (rom, other) = (std::path::Path::new("/roms/snes/F-Zero (USA).sfc"), std::path::Path::new("/roms/snes/Other.sfc"));
        let card = Some(Offer { scale: 2, wide: true });
        let start = |rom| start_core(&dir, "snes", rom, "snes9x", card, true);
        assert_eq!(start(rom), "snes9x", "nothing chosen: Original, Snes9x");
        let row = setting(KEY).unwrap();
        let step = |delta| {
            with_offer(card, || {
                let mut s = GameSettings::load(&dir, "snes", rom);
                let changed = s.step(row, delta);
                assert!(s.save_for_game(), "kept for this game");
                changed
            })
        };
        assert_eq!(step(1), vec![(KEY, "hd".to_string())]);
        assert_eq!(start(rom), LINUX_CORE, "HD: bsnes-hd");
        assert_eq!(GameSettings::load(&dir, "snes", rom).values.get(KEY).map(String::as_str), Some("hd"));
        let options: std::collections::BTreeMap<_, _> = core_options(LINUX_CORE, mode(card, Some("hd")), 2).into_iter().collect();
        assert_eq!(options["bsnes_mode7_scale"], "2x");
        assert_eq!(start(other), "snes9x", "the console's other games keep theirs");
        assert_eq!(step(1), vec![(KEY, "wide".to_string())]);
        assert_eq!(start(rom), LINUX_CORE);
        assert_eq!(step(1), vec![(KEY, "original".to_string())], "round to Original");
        assert_eq!(start(rom), "snes9x", "back on Snes9x");
        step(1);
        assert_eq!(start_core(&dir, "snes", rom, "snes9x", None, true), "snes9x", "no row on this handheld: Snes9x");
        assert_eq!(start_core(&dir, "snes", rom, "snes9x", card, false), "snes9x", "an image without bsnes-hd: Snes9x");
    }

    #[test]
    fn the_row_s_choice_comes_first_and_once() {
        let start = core_options("bsnes", "hd", 2);
        let mut o = vec![("bsnes_mode7_scale", "8x"), ("bsnes_blur_emulation", "ON")];
        place(&start, &mut o);
        assert_eq!(o, vec![("bsnes_mode7_scale", "2x"), ("bsnes_blur_emulation", "ON")]);
    }
}
