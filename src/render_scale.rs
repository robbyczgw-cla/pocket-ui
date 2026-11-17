//! Options → Picture → Resolution for the 3D systems pocket-core plays itself (N64, PSP, Dreamcast, PlayStation): how many times the
//! console's own resolution the core draws at. The same row, words and "takes effect the next time the game starts" as the
//! Resolution of PS2, GameCube, Wii and Xbox (`emu_settings`), kept per console with the usual per-game override
//! (`options::GameSettings`), and read by pocket-core when the game starts (`core_options`, the core's own option).
//!
//! Which factors a handheld gets is measured, per chip class (docs/HOST.md, "Resolution of the 3D systems"): the highest
//! factor that held full speed with headroom is the default, the highest that held full speed at all is the last choice
//! (PlayStation: Original stays the default, its 2x is a renderer trick that drops pictures in some games).
//! Chips not measured keep what they had (no row, the core's option untouched): the H700, RK3566 and RK3326 handhelds
//! (their N64, PSP and Dreamcast stay on ROCKNIX's emulators anyway, `hostsys::H700_STAYS_ON_ROCKNIX`), the SM4450 Classic,
//! computers and unknown handhelds.

use crate::device::Model;

/// The Picture row's key in the game settings files.
pub const KEY: &str = "resolution";

/// What a chip class offers for one system: the default factor and the highest one offered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Range {
    pub default: u8,
    pub max: u8,
}

/// The chip classes with measured numbers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Class {
    /// Snapdragon 865 (Adreno 650): Retroid Pocket Mini V2 (measured), Mini, Pocket 5, Flip 2.
    Sm8250,
    /// Snapdragon 8 Gen 2 (Adreno 740): Retroid Pocket 6, Nova, AYN Odin 2 family. A far faster GPU than the
    /// SM8250's, given the SM8250's numbers until it is measured.
    Sm8550,
}

/// The chip class of a handheld, None where the row stays out.
pub fn class(model: Option<Model>) -> Option<Class> {
    match model? {
        Model::MiniV2 | Model::MiniV1 | Model::Pocket5 | Model::Flip2 => Some(Class::Sm8250),
        Model::Pocket6 | Model::Nova | Model::Odin2 | Model::Odin2Mini | Model::Odin2Portal => Some(Class::Sm8550),
        _ => None,
    }
}

/// The factors of a system on a chip class; None: the system has no row there.
pub fn range(class: Class, system: &str) -> Option<Range> {
    let r = |default, max| Some(Range { default, max });
    match (class, system) {
        // Measured on the Mini V2, 16.11.2026 (docs/HOST.md, "Resolution of the 3D systems"). N64: 2x paces frames as
        // Original does in all three games; 3x held full speed but Zelda's frames came less even (p99 19-20 ms); 4x
        // uneven in all three. PSP: clean up to 4x in Bomberman, but that is a light game and no heavy one could be
        // measured, so 2x (about the screen's width) and 3x at most. Dreamcast (Sonic Adventure 2, 70 % CPU): 2x
        // like Original; 3x made the GPU clock up.
        (Class::Sm8250 | Class::Sm8550, "n64") => r(2, 3),
        (Class::Sm8250 | Class::Sm8550, "psp") => r(2, 3),
        (Class::Sm8250 | Class::Sm8550, "dreamcast") => r(2, 2),
        // PCSX-ReARMed's Enhanced Resolution (2x, NEON renderer): Crash Bandicoot 2 at full speed (CPU 20 → 40 %), but its
        // pause screen lost the game picture behind it (black). Offered, not the default.
        (Class::Sm8250 | Class::Sm8550, "psx") => r(1, 2),
        _ => None,
    }
}

/// [`range`] on this handheld.
pub fn here(system: &str) -> Option<Range> {
    class(crate::device::current().model).and_then(|c| range(c, system))
}

/// The Picture row's values, by factor (Original, 2x, 3x, 4x).
pub const VALUES: [&str; 4] = ["1", "2", "3", "4"];

/// Whether the row offers `value` with this range: the factors up to the highest that ran at full speed.
pub fn offers(range: Option<Range>, value: &str) -> bool {
    range.is_some_and(|r| value.parse::<u8>().is_ok_and(|f| f >= 1 && f <= r.max))
}

/// The row's value when nobody chose one (the console's starting value), None where the row isn't there.
pub fn default_value(range: Option<Range>) -> Option<&'static str> {
    range.map(|r| VALUES[usize::from(r.default.clamp(1, 4)) - 1])
}

/// The core's own option for a factor: the key and the value pocket-core sets before the game loads. `core` is the core's
/// id (`core_options::core_id`); a factor of a system whose core isn't this one sets nothing.
pub fn core_option(core: &str, factor: u8) -> Option<(&'static str, String)> {
    let f = u32::from(factor.max(1));
    match core {
        // GLideN64's "native resolution factor": the N64's own 320x240 times this. 0 would mean the window size below.
        "mupen64plus_next" => Some(("mupen64plus-EnableNativeResFactor", f.to_string())),
        // The PSP's 480x272 times this.
        "ppsspp" => Some(("ppsspp_internal_resolution", format!("{}x{}", 480 * f, 272 * f))),
        // The Dreamcast's 640x480 times this.
        "flycast" => Some(("flycast_internal_resolution", format!("{}x{}", 640 * f, 480 * f))),
        // The PlayStation: Enhanced Resolution draws 3D at twice the console's resolution; it has no other factor.
        "pcsx_rearmed" => Some(("pcsx_rearmed_neon_enhancement_enable", if f >= 2 { "enabled" } else { "disabled" }.to_string())),
        _ => None,
    }
}

/// The resolution option of the game about to start, as pocket-core gives it to the core: the Picture row's value (the
/// game's, else its console's, else this handheld's default), capped at what the handheld offers. None: the row isn't
/// there (nothing changes), or the core is another one.
///
/// `chosen` is the row's saved value, if any. `first` says where it goes among the options: a choice of the row comes
/// before everything (it is the player's latest word on the resolution), the handheld's default after the player's own
/// saved core options, so an Emulator-page choice made before this row existed still holds until the row is changed.
pub fn for_start(range: Option<Range>, core: &str, chosen: Option<&str>) -> Option<StartOption> {
    let range = range?;
    let picked = chosen.and_then(|v| v.parse::<u8>().ok()).filter(|f| (1..=range.max).contains(f));
    let (key, value) = core_option(core, picked.unwrap_or(range.default))?;
    Some(StartOption { key, value, first: picked.is_some() })
}

#[derive(Debug, PartialEq, Eq)]
pub struct StartOption {
    pub key: &'static str,
    pub value: String,
    pub first: bool,
}

impl StartOption {
    /// Puts the option into the core options of the start (the player's saved ones first; whatever is in the list wins
    /// over RetroArch's, which are added beneath later): a choice of the row replaces a saved value of the same key, the
    /// handheld's default goes in only where none is there.
    pub fn place<'a>(&'a self, options: &mut Vec<(&'a str, &'a str)>) {
        if self.first {
            options.retain(|(k, _)| *k != self.key);
        }
        if !options.iter().any(|(k, _)| *k == self.key) {
            options.insert(0, (self.key, self.value.as_str()));
        }
    }
}

/// Whether the Resolution row is there for the system this core plays (the core's id, `core_options::core_id`).
pub fn row_for_core(core: &str) -> bool {
    let system = match core {
        "mupen64plus_next" => "n64",
        "ppsspp" => "psp",
        "flycast" => "dreamcast",
        "pcsx_rearmed" => "psx",
        _ => return false,
    };
    here(system).is_some()
}

/// The core option keys the row decides: the Emulator page leaves them out where the row is there (one place for it).
pub const CORE_KEYS: &[&str] =
    &["mupen64plus-EnableNativeResFactor", "ppsspp_internal_resolution", "flycast_internal_resolution", "pcsx_rearmed_neon_enhancement_enable"];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device::{H700_BOARDS, RK3566_BOARDS};

    #[test]
    fn strong_chips_get_the_row_for_the_3d_systems_and_weak_ones_keep_theirs() {
        for m in [Model::MiniV2, Model::MiniV1, Model::Pocket5, Model::Flip2, Model::Odin2Mini, Model::Pocket6] {
            let c = class(Some(m)).expect("strong chip");
            for sys in ["n64", "psp", "dreamcast", "psx"] {
                let r = range(c, sys).unwrap_or_else(|| panic!("{m:?} {sys}"));
                assert!(r.default >= 1 && r.default <= r.max, "{m:?} {sys} {r:?}");
            }
            for sys in ["gba", "nds", "snes", "ps2", "saturn"] {
                assert_eq!(range(c, sys), None, "{m:?} {sys}: no row");
            }
        }
        // Weak or unmeasured chips: no row, nothing set, whatever the system.
        let weak = [Some(Model::H700(&H700_BOARDS[0])), Some(Model::Rk3566(&RK3566_BOARDS[0])), Some(Model::RgDsPlus), Some(Model::PocketClassic), None];
        for m in weak {
            assert_eq!(class(m), None, "{m:?}");
        }
    }

    #[test]
    fn each_core_gets_its_own_option_for_a_factor() {
        assert_eq!(core_option("mupen64plus_next", 3), Some(("mupen64plus-EnableNativeResFactor", "3".into())));
        assert_eq!(core_option("ppsspp", 2), Some(("ppsspp_internal_resolution", "960x544".into())));
        assert_eq!(core_option("ppsspp", 3), Some(("ppsspp_internal_resolution", "1440x816".into())));
        assert_eq!(core_option("flycast", 1), Some(("flycast_internal_resolution", "640x480".into())));
        assert_eq!(core_option("flycast", 2), Some(("flycast_internal_resolution", "1280x960".into())));
        assert_eq!(core_option("parallel_n64", 2), None, "another N64 core: untouched");
        assert_eq!(core_option("pcsx_rearmed", 1), Some(("pcsx_rearmed_neon_enhancement_enable", "disabled".into())));
        assert_eq!(core_option("pcsx_rearmed", 2), Some(("pcsx_rearmed_neon_enhancement_enable", "enabled".into())));
        assert_eq!(core_option("swanstation", 2), None, "not the PS1 core pocket-core runs");
        for (core, _) in [("mupen64plus_next", 0), ("ppsspp", 0), ("flycast", 0), ("pcsx_rearmed", 0)] {
            assert!(CORE_KEYS.contains(&core_option(core, 1).unwrap().0));
        }
    }

    #[test]
    fn the_row_offers_up_to_the_highest_full_speed_factor_and_starts_at_the_default() {
        let n64 = range(Class::Sm8250, "n64");
        let max = n64.unwrap().max;
        for v in VALUES {
            assert_eq!(offers(n64, v), v.parse::<u8>().unwrap() <= max, "{v}");
        }
        assert!(!offers(n64, "0") && !offers(n64, "x") && !offers(None, "1"));
        assert_eq!(default_value(n64), Some(VALUES[usize::from(n64.unwrap().default) - 1]));
        assert_eq!(default_value(None), None, "no row: no starting value");
    }

    #[test]
    fn a_start_gets_the_choice_else_the_default_and_never_more_than_offered() {
        let r = Some(Range { default: 2, max: 3 });
        let n64 = |c| for_start(r, "mupen64plus_next", c);
        assert_eq!(n64(None), Some(StartOption { key: "mupen64plus-EnableNativeResFactor", value: "2".into(), first: false }));
        assert_eq!(n64(Some("1")), Some(StartOption { key: "mupen64plus-EnableNativeResFactor", value: "1".into(), first: true }));
        assert_eq!(n64(Some("3")).unwrap().value, "3");
        // A choice saved on a stronger handheld (the card moved) or a broken file: this handheld's default.
        assert_eq!(n64(Some("4")), Some(StartOption { key: "mupen64plus-EnableNativeResFactor", value: "2".into(), first: false }));
        assert_eq!(n64(Some("x")).unwrap().value, "2");
        assert_eq!(for_start(None, "mupen64plus_next", Some("3")), None, "no row here: the core keeps what it had");
        assert_eq!(for_start(r, "gambatte", None), None);
    }

    #[test]
    fn the_choice_beats_an_old_emulator_page_value_and_the_default_does_not() {
        let key = "mupen64plus-EnableNativeResFactor";
        let old_choice = vec![(key, "4"), ("mupen64plus-rdp-plugin", "gliden64")];
        let r = Some(Range { default: 2, max: 4 });

        let chosen = for_start(r, "mupen64plus_next", Some("3")).unwrap();
        let mut o = old_choice.clone();
        chosen.place(&mut o);
        assert_eq!(o, vec![(key, "3"), ("mupen64plus-rdp-plugin", "gliden64")], "the row's choice, once");

        let default = for_start(r, "mupen64plus_next", None).unwrap();
        let mut o = old_choice.clone();
        default.place(&mut o);
        assert_eq!(o, old_choice, "an Emulator-page choice made before the row stays until the row is changed");

        let mut o = vec![("mupen64plus-rdp-plugin", "gliden64")];
        default.place(&mut o);
        assert_eq!(o[0], (key, "2"), "nothing chosen anywhere: the handheld's default, above RetroArch's 1");
    }
}
