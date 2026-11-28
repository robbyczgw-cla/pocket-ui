//! Portable look values, re-exported by settings on both targets.
/// Looks the shelf can take; Settings > Look picks one.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, serde::Serialize, serde::Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    #[default]
    Shelf,
    /// Also what a settings file from before 0.4 says for the two looks that went then, Minimal and Cards: both were
    /// lists or flat fronts with quick animations, and Split is the one left like them (`settings::retired_look`).
    #[serde(alias = "minimal", alias = "cards")]
    Split,
    Cabinet,
    /// "Easy": one big game at a time (`big_shelf`).
    Big,
}

impl Theme {
    pub const ALL: [Theme; 4] = [Theme::Shelf, Theme::Big, Theme::Split, Theme::Cabinet];
    /// Phone looks, including the single-game Easy view.
    pub const PHONE: [Theme; 4] = Self::ALL;

    pub fn name(self) -> &'static str {
        match self {
            Theme::Shelf => "Shelf",
            Theme::Split => "Split",
            Theme::Cabinet => "Cabinet",
            Theme::Big => "Easy",
        }
    }

    /// One line for the theme choice in setup.
    pub fn blurb(self) -> &'static str {
        match self {
            Theme::Shelf => "Games lined up on a shelf, showing the selected game's box art.",
            Theme::Split => "List on the left, details on the right.",
            Theme::Cabinet => "Cases lined up on a wooden shelf. Pull any case out to see its box art.",
            Theme::Big => "One big case at a time, zero clutter.",
        }
    }
}

/// Settings › Look › Layout › Order: Home's shelves are the systems, or one shelf per game series (`series_shelf`) in
/// Shelf, Split and Cabinet. Easy and Kids Mode keep their own order.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, serde::Serialize, serde::Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ShelfOrder {
    #[default]
    Console,
    Series,
}

impl ShelfOrder {
    pub const ALL: [ShelfOrder; 2] = [ShelfOrder::Console, ShelfOrder::Series];

    pub fn name(self) -> &'static str {
        match self {
            ShelfOrder::Console => "By console",
            ShelfOrder::Series => "By series",
        }
    }

    /// One line about the choice, beside its pills.
    pub fn blurb(self) -> &'static str {
        match self {
            ShelfOrder::Console => "A shelf for each console.",
            ShelfOrder::Series => "A shelf for each series, every console's games in release order.",
        }
    }
}

/// Settings > Look > Appearance.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, serde::Serialize, serde::Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Appearance {
    #[default]
    Dark,
    Light,
    Auto,
}

impl Appearance {
    pub const ALL: [Appearance; 3] = [Appearance::Dark, Appearance::Light, Appearance::Auto];

    pub fn name(self) -> &'static str {
        match self {
            Appearance::Dark => "Dark",
            Appearance::Light => "Light",
            Appearance::Auto => "Automatic",
        }
    }

    /// One line about the choice, under the row in the look step.
    pub fn blurb(self) -> &'static str {
        match self {
            Appearance::Dark => "Dark pages. Best for battery.",
            Appearance::Light => "Light pages for daylight. Uses more battery.",
            Appearance::Auto => "Light from 07:00, dark from 20:00, local time.",
        }
    }
}

/// Settings > Look > Motion: how much the menus move (`crate::motion`).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, serde::Serialize, serde::Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Motion {
    #[default]
    Full,
    Reduced,
    Off,
}

impl Motion {
    pub const ALL: [Motion; 3] = [Motion::Full, Motion::Reduced, Motion::Off];

    pub fn name(self) -> &'static str {
        match self {
            Motion::Full => "Full",
            Motion::Reduced => "Reduced",
            Motion::Off => "Off",
        }
    }

    /// One line about the choice, after the row in the look step.
    pub fn blurb(self) -> &'static str {
        match self {
            Motion::Full => "Smooth glides and sliding pages.",
            Motion::Reduced => "Gentle fades only, no sliding.",
            Motion::Off => "No animation at all.",
        }
    }

    /// The next choice (R1 on the look step).
    pub fn next(self) -> Motion {
        Motion::ALL[(Motion::ALL.iter().position(|m| *m == self).unwrap_or(0) + 1) % Motion::ALL.len()]
    }
}
