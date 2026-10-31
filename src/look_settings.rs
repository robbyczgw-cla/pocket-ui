//! Portable look values, re-exported by settings on both targets.
/// Looks the shelf can take; Settings > Look picks one.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, serde::Serialize, serde::Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    #[default]
    Shelf,
    Minimal,
    Split,
    Cards,
    Cabinet,
}

impl Theme {
    pub const ALL: [Theme; 5] = [Theme::Shelf, Theme::Minimal, Theme::Split, Theme::Cards, Theme::Cabinet];

    pub fn name(self) -> &'static str {
        match self {
            Theme::Shelf => "Shelf",
            Theme::Minimal => "Minimal",
            Theme::Split => "Split",
            Theme::Cards => "Cards",
            Theme::Cabinet => "Cabinet",
        }
    }

    /// One line for the theme choice in setup.
    pub fn blurb(self) -> &'static str {
        match self {
            Theme::Shelf => "Your games standing on a shelf. The one you pick shows its cover.",
            Theme::Minimal => "Just the names, nothing else.",
            Theme::Split => "List on the left, details on the right.",
            Theme::Cards => "Big cards you flip through sideways.",
            Theme::Cabinet => "Your games in their cases on a wooden shelf. Pull one out to see its cover.",
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
            Appearance::Dark => "Black pages. Uses the least battery.",
            Appearance::Light => "Paper pages for daylight. Uses more battery.",
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
            Motion::Full => "Selections glide, pages slide in.",
            Motion::Reduced => "Only fades. Nothing slides or grows.",
            Motion::Off => "No animation at all.",
        }
    }

    /// The next choice (R1 on the look step).
    pub fn next(self) -> Motion {
        Motion::ALL[(Motion::ALL.iter().position(|m| *m == self).unwrap_or(0) + 1) % Motion::ALL.len()]
    }
}
