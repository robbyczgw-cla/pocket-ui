//! The one caution on "Add your games": a system this handheld runs, though not every game of it runs smoothly.
//! and a system the handheld cannot play at all get nothing (routing is not touched here: hostsys.rs decides who plays).
//!
//! Sources of the table (a general caution, so a solid consensus is enough; no numbers are claimed):
//! - Our own runs: POCKET-BENCH-2026-10-31 (Mini V2: PS2 mixed, fan starts in several games; GameCube/Wii/3DS/Xbox start,
//!   Xbox stutters), ODIN-BENCH-2026-10-31 (Odin 2 Mini: GameCube 7 games, 2 of them short of full speed; PS2 14 games,
//!   about half at full speed or playable, 3 too slow, 2 stopped by the fan), DS-H700-2026-10-31 (H700: melonDS
//!   too slow, DraStic about 80-100 %), docs/MEASURED.md, docs/BIG-EMULATORS.md, docs/DEVICES.md.
//! - Public handheld compatibility lists and reviews, by chip: Snapdragon 865 (Retroid Pocket 5 / Mini family: PS2 and
//!   GameCube playable for many games, heavy ones not; Wii and 3DS mixed; Dreamcast, PSP, N64, DS fine), Snapdragon 8 Gen 2
//!   (Odin 2 family, Pocket 6: PS2, GameCube, Wii, 3DS, Dreamcast, PSP broadly fine; PS3, Wii U, Vita and Xbox for
//!   selected games), Allwinner H700 (RG35XX / RG40XX / RG34XX: up to PlayStation smooth; N64, PSP, Dreamcast, DS and
//!   Saturn only some games, at reduced speed or frameskip), RK3568 (RG DS Plus, reviews: up to PlayStation and DS at full
//!   speed, N64 playable, Dreamcast and PSP only some games).
use crate::device_model::Model;

/// The sentence (one string, translated in assets/i18n).
pub const NOTE: &str = "Runs on this handheld, but not every game runs smoothly.";

/// A chip family of `docs/DEVICES.md`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Family {
    Sd865,
    Sd8Gen2,
    H700,
    /// The RG DS Plus's RK3568 (four Cortex-A55): DS full speed on DraStic, N64/PSP/Dreamcast only some games.
    Rk3566,
}

fn family(model: Model) -> Option<Family> {
    match model {
        Model::MiniV2 | Model::MiniV1 | Model::Pocket5 | Model::Flip2 => Some(Family::Sd865),
        Model::Pocket6 | Model::Nova | Model::Odin2 | Model::Odin2Mini | Model::Odin2Portal => Some(Family::Sd8Gen2),
        Model::H700(_) => Some(Family::H700),
        Model::RgDsPlus => Some(Family::Rk3566),
        Model::PocketClassic => None,
        #[cfg(feature = "brick")] Model::Brick(_) => None,
    }
}

/// ROCKNIX system ids with the note, per family.
const TABLE: &[(Family, &[&str])] = &[
    (Family::Sd865, &["ps2", "gamecube", "wii", "3ds", "xbox"]),
    (Family::Sd8Gen2, &["ps2", "gamecube", "xbox", "ps3", "wiiu", "psvita"]),
    (Family::H700, &["n64", "psp", "dreamcast", "nds", "saturn"]),
    (Family::Rk3566, &["n64", "psp", "dreamcast", "saturn"]),
];

/// The note for `system` on `model` (None: an unknown handheld, a system without a caution).
pub fn note_for(model: Option<Model>, system: &str) -> Option<&'static str> {
    let fam = family(model?)?;
    TABLE.iter().any(|(f, ids)| *f == fam && ids.contains(&system)).then_some(NOTE)
}

/// `note_for` on this handheld.
pub fn note(system: &str) -> Option<&'static str> {
    note_for(crate::device::current().model, system)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device_model::h700_board;

    fn rg28() -> Option<Model> {
        Some(Model::H700(h700_board("Anbernic RG28XX", &[]).unwrap()))
    }

    fn systems(model: Option<Model>) -> Vec<&'static str> {
        ["psx", "n64", "psp", "dreamcast", "nds", "saturn", "ps2", "gamecube", "wii", "3ds", "xbox", "ps3", "wiiu", "psvita", "gba"]
            .into_iter()
            .filter(|s| note_for(model, s).is_some())
            .collect()
    }

    #[test]
    fn mini_v2_gets_it_on_the_big_emulators() {
        assert_eq!(systems(Some(Model::MiniV2)), ["ps2", "gamecube", "wii", "3ds", "xbox"]);
    }

    #[test]
    fn odin_2_mini_gets_it_where_benches_and_reviews_say_selected_games() {
        assert_eq!(systems(Some(Model::Odin2Mini)), ["ps2", "gamecube", "xbox", "ps3", "wiiu", "psvita"]);
    }

    #[test]
    fn rg28xx_gets_it_on_the_heavier_systems_and_not_on_playstation() {
        assert_eq!(systems(rg28()), ["n64", "psp", "dreamcast", "nds", "saturn"]);
        assert_eq!(note_for(rg28(), "psx"), None);
    }

    #[test]
    fn rg_ds_plus_gets_it_on_the_heavier_systems_but_not_on_ds() {
        assert_eq!(systems(Some(Model::RgDsPlus)), ["n64", "psp", "dreamcast", "saturn"]);
    }

    #[test]
    fn the_deck_and_unknown_handhelds_get_nothing() {
        assert!(systems(None).is_empty());
    }

    #[test]
    fn every_listed_handheld_has_a_family() {
        for m in [Model::MiniV1, Model::Pocket5, Model::Flip2, Model::Pocket6, Model::Nova, Model::Odin2, Model::Odin2Portal, Model::RgDsPlus] {
            assert!(family(m).is_some());
        }
    }
}
