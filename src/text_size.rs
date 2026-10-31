/// Settings → Look & feel → Text size: the type of every screen as designed, an eighth larger, or a quarter larger
/// (`crate::type_scale`).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, serde::Serialize, serde::Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum TextSize {
    #[default]
    Normal,
    Medium,
    Large,
}

impl TextSize {
    pub const ALL: [TextSize; 3] = [TextSize::Normal, TextSize::Medium, TextSize::Large];

    pub fn label(self) -> &'static str {
        match self {
            TextSize::Normal => "Normal",
            TextSize::Medium => "Medium",
            TextSize::Large => "Large",
        }
    }

    /// The next size, Large going round to Normal (A, right, R1 on the first start).
    pub fn next(self) -> TextSize {
        TextSize::ALL[(TextSize::ALL.iter().position(|t| *t == self).unwrap_or(0) + 1) % TextSize::ALL.len()]
    }

    /// The size before, Normal going round to Large (left).
    pub fn previous(self) -> TextSize {
        TextSize::ALL[(TextSize::ALL.iter().position(|t| *t == self).unwrap_or(0) + TextSize::ALL.len() - 1) % TextSize::ALL.len()]
    }
}
