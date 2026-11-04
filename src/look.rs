//! Options → Picture → Look: one row that answers "how should the picture look?" and sets the two saved values behind it,
//! `sharpness` and `effect` (and `lcd_colour` for the GBA's LCD grid). Nothing here is saved: the Look is worked out from the
//! values in use (`derive`), and choosing one writes the two keys in their canonical form (`canonical`). The saved strings,
//! their parsers (`video::Sharpness::parse`, `Effect::parse`) and the shader are untouched, so no picture changes until the
//! player picks a Look. Concept and wording: PICTURE-OPTIONS-CLARITY.md, Variant B ("Review Opus", "Review Sol").
//!
//! Which Looks a console is offered depends on its kind (`Kind`): a handheld gets LCD grid, a TV console, arcade or
//! computer gets Scanlines and Old TV, and neither gets the other's (historically wrong, and a shorter row). A saved
//! combination the kind doesn't offer shows as Custom with its rows open, as does every Soft + effect pair.

use crate::options::HANDHELDS;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Look {
    Clean,
    Smooth,
    Soft,
    Bloom,
    LcdGrid,
    Scanlines,
    OldTv,
    /// Sharpness and Effect set one by one; nothing is written by choosing it.
    Custom,
}

/// The order of the Look row's choices, one axis from clean and modern to heavily worked and retro, Custom last.
pub const ALL: [Look; 8] = [Look::Clean, Look::Smooth, Look::Soft, Look::Bloom, Look::LcdGrid, Look::Scanlines, Look::OldTv, Look::Custom];

impl Look {
    /// The value of the Look row's choice (never saved).
    pub fn value(self) -> &'static str {
        match self {
            Look::Clean => "clean",
            Look::Smooth => "smooth",
            Look::Soft => "soft",
            Look::Bloom => "bloom",
            Look::LcdGrid => "lcd",
            Look::Scanlines => "scanlines",
            Look::OldTv => "oldtv",
            Look::Custom => "custom",
        }
    }

    pub fn from_value(v: &str) -> Option<Look> {
        ALL.into_iter().find(|l| l.value() == v)
    }

    /// The `sharpness` and `effect` choosing this Look saves; None for Custom, which saves nothing.
    /// Crisp wherever it looks the same as Sharp at Integer scaling and under Scanlines and Old TV (the shader steers
    /// those by Crisp anyway) and gives equal-sized pixels at Aspect; LCD grid keeps the hard texels under the grid.
    pub fn canonical(self) -> Option<(&'static str, &'static str)> {
        match self {
            Look::Clean => Some(("crisp", "none")),
            Look::Smooth => Some(("crisp", "smooth")),
            Look::Soft => Some(("soft", "none")),
            Look::Bloom => Some(("crisp", "softglow")),
            Look::LcdGrid => Some(("sharp", "grid")),
            Look::Scanlines => Some(("crisp", "scanlines")),
            Look::OldTv => Some(("crisp", "crt")),
            Look::Custom => None,
        }
    }
}

/// What a console is: decides which Looks the row offers.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    /// The handhelds (`options::HANDHELDS`), the DS and the PSP: LCD grid.
    Handheld,
    /// Everything else (TV consoles, arcade, computers): Scanlines and Old TV.
    Tv,
}

pub fn kind(system: &str) -> Kind {
    if HANDHELDS.contains(&system) || matches!(system, "nds" | "psp") {
        Kind::Handheld
    } else {
        Kind::Tv
    }
}

/// Whether `look` is a choice of the row for `system`; with no console (None) every Look is.
pub fn offered(look: Look, system: Option<&str>) -> bool {
    !matches!((look, system.map(kind)), (Look::LcdGrid, Some(Kind::Tv)) | (Look::Scanlines | Look::OldTv, Some(Kind::Handheld)))
}

/// The Look that shows for the values in use. Sharp and Crisp count as the same (they are, at Integer scaling and under
/// the TV effects; the canonical form is only written when a Look is chosen); Soft is a Look on its own with no effect, and
/// Custom with any. A pair the console's kind isn't offered shows as Custom, values unchanged.
pub fn derive(sharpness: &str, effect: &str, system: Option<&str>) -> Look {
    let look = match (sharpness, effect) {
        ("sharp" | "crisp", "none") => Look::Clean,
        ("soft", "none") => Look::Soft,
        ("sharp" | "crisp", "smooth") => Look::Smooth,
        ("sharp" | "crisp", "softglow") => Look::Bloom,
        ("sharp" | "crisp", "grid") => Look::LcdGrid,
        ("sharp" | "crisp", "scanlines") => Look::Scanlines,
        ("sharp" | "crisp", "crt") => Look::OldTv,
        _ => Look::Custom,
    };
    if offered(look, system) {
        look
    } else {
        Look::Custom
    }
}

/// One line for the explanation under the Look row; the other rows' per-value lines are in `options::value_desc`.
pub fn explain(look: Look, gba_colours: bool) -> &'static str {
    match look {
        Look::Clean => "Plain pixels, nothing added, all the same size at any scaling.",
        Look::Smooth => "Pixels stay sharp, but stair-steps on slanted edges get rounded off.",
        Look::Soft => "The whole picture slightly blurred, no hard pixel squares.",
        Look::Bloom => "A clear picture where bright parts glow softly around the edges.",
        Look::LcdGrid if gba_colours => "Thin lines between the pixels, like a handheld's screen, in the GBA screen's colours.",
        Look::LcdGrid => "Thin lines between the pixels, like a handheld's screen.",
        Look::Scanlines => "Dark lines between the rows, like a TV. Fades out in hi-res menus.",
        Look::OldTv => "Scanlines plus the glow of a tube TV. The TV around it is a Screen frame.",
        Look::Custom => "Pick sharpness and effect yourself in the two rows below.",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHARPNESS: [&str; 3] = ["sharp", "crisp", "soft"];
    const EFFECTS: [&str; 6] = ["none", "grid", "scanlines", "crt", "softglow", "smooth"];

    #[test]
    fn every_look_but_custom_has_its_canonical_pair_and_derives_back_to_itself() {
        for look in ALL {
            assert_eq!(Look::from_value(look.value()), Some(look));
            let Some((sharpness, effect)) = look.canonical() else {
                assert_eq!(look, Look::Custom);
                continue;
            };
            // Parsed by the very parsers the shader's settings use: the strings are real ones.
            assert!(["sharp", "crisp", "soft"].contains(&sharpness) && EFFECTS.contains(&effect), "{look:?}");
            assert_eq!(derive(sharpness, effect, None), look, "{look:?}");
            for system in ["gb", "gba", "nds", "snes", "psx"] {
                let want = if offered(look, Some(system)) { look } else { Look::Custom };
                assert_eq!(derive(sharpness, effect, Some(system)), want, "{look:?} on {system}");
            }
        }
    }

    #[test]
    fn all_eighteen_saved_pairs_show_a_look_or_custom_and_none_is_lost() {
        for system in [None, Some("gba"), Some("snes")] {
            for s in SHARPNESS {
                for e in EFFECTS {
                    let look = derive(s, e, system);
                    // Sharp and Crisp are one Look; Soft with an effect is always Custom.
                    if s == "soft" && e != "none" {
                        assert_eq!(look, Look::Custom, "{s}+{e}");
                    }
                    if s != "soft" || e == "none" {
                        assert_eq!(derive("sharp", e, system), derive("crisp", e, system), "{e}");
                    }
                    // The Look's own pair is one of the 18; Custom keeps whatever was saved.
                    if let Some(pair) = look.canonical() {
                        assert_eq!(derive(pair.0, pair.1, system), look);
                    }
                    assert!(offered(look, system), "{s}+{e} derives to a Look the row doesn't offer");
                }
            }
        }
    }

    #[test]
    fn the_kinds_get_their_looks_and_the_rest_is_left_out() {
        for system in ["gb", "gbc", "gba", "gamegear", "atarilynx", "ngp", "wonderswan", "nds", "psp"] {
            assert_eq!(kind(system), Kind::Handheld, "{system}");
            let offered: Vec<_> = ALL.into_iter().filter(|l| offered(*l, Some(system))).map(Look::value).collect();
            assert_eq!(offered, ["clean", "smooth", "soft", "bloom", "lcd", "custom"], "{system}");
        }
        for system in ["nes", "snes", "megadrive", "psx", "n64", "dreamcast", "arcade", "fbneo", "scummvm"] {
            assert_eq!(kind(system), Kind::Tv, "{system}");
            let offered: Vec<_> = ALL.into_iter().filter(|l| offered(*l, Some(system))).map(Look::value).collect();
            assert_eq!(offered, ["clean", "smooth", "soft", "bloom", "scanlines", "oldtv", "custom"], "{system}");
        }
        assert_eq!(ALL.into_iter().filter(|l| offered(*l, None)).count(), 8, "no console: all of them");
    }

    #[test]
    fn a_pair_the_kind_does_not_offer_shows_custom() {
        assert_eq!(derive("sharp", "grid", Some("psx")), Look::Custom, "an LCD grid on a PlayStation");
        assert_eq!(derive("crisp", "crt", Some("gba")), Look::Custom, "Old TV on a Game Boy Advance");
        assert_eq!(derive("crisp", "scanlines", Some("gb")), Look::Custom);
        assert_eq!(derive("sharp", "grid", Some("gba")), Look::LcdGrid);
    }
}
