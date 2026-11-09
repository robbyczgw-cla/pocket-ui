//! Printed face-button layout shared by Linux and Android controls cards.
//! Physical menu-role remapping and device defaults live in Linux button_layout.rs.

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ButtonLayout {
    /// A on the right, as printed on the Mini V2 and original Odin 2.
    Nintendo,
    /// A at the bottom, B on the right, X on the left, Y on top, as on an Xbox controller.
    Xbox,
}

