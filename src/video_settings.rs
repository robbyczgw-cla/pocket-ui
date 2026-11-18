//! The Picture settings as values (`scaling`, `sharpness`, `effect`, `frame` and its finishes, `lcd_colour`,
//! `orientation`): read and applied the same by pocket-core on Linux (`video`) and Android's player, so a choice draws the
//! same picture and the same frame on both. The files they come from are `options::GameSettings`'.

use std::collections::BTreeMap;

use crate::video_basics::Scaling;
use crate::video_program::{Effect, Sharpness};

/// Options → Picture → Screen orientation: how the player holds the Pocket, for games made for an upright screen
/// (vertical shooters, WonderSwan games played upright, DS books). The picture is drawn turned the other way on the
/// panel so it stands upright for the player, on top of any turn the core asks for (SET_ROTATION); the D-pad and sticks
/// of the Pocket's own controls turn with it (`controls::turn_buttons`).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Orientation {
    #[default]
    Normal,
    /// The Pocket turned a quarter to the left (counter-clockwise): the D-pad at the bottom.
    Left,
    /// The Pocket turned a quarter to the right (clockwise): the D-pad on top.
    Right,
}

impl Orientation {
    pub fn parse(s: &str) -> Self {
        match s {
            "left" => Self::Left,
            "right" => Self::Right,
            _ => Self::Normal,
        }
    }

    /// Quarter turns counter-clockwise the picture gets on the panel (the shader's `rot`): a Pocket turned right
    /// turns everything on it clockwise, so the picture is drawn turned counter-clockwise by as much.
    pub fn turns(self) -> u32 {
        match self {
            Self::Normal => 0,
            Self::Right => 1,
            Self::Left => 3,
        }
    }
}

/// Frame around a picture that doesn't fill the screen (see `bezel.rs` and `ambient.rs`).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum FrameMode {
    #[default]
    None,
    /// The installed pack's frame for the console, else the built-in one.
    Auto,
    /// The game's own colours, blurred and dimmed, like a TV's ambient light.
    Ambient,
    /// A handheld body around the screen lens, in the colourway of `frame_colour`.
    Shell,
    /// A curved-screen TV, in the cabinet of `frame_tv`.
    Crt,
    /// An arcade cabinet's glossy bezel with light strips.
    Arcade,
    /// A thin outline glowing in the console's colour.
    Glow,
    /// The handheld and TV frames of `frames_more.rs` and `frames_tv.rs`; those with finishes take
    /// them from their own setting (`frame_brick`, `frame_pocket`, `frame_clamshell`). Swan, Watch and
    /// Goggles are drawn by Auto only (a saved choice of them reads as Auto).
    Brick,
    PocketSlim,
    Clamshell,
    Wide,
    Twin,
    Gloss,
    Studio,
    PortableTv,
    Woodgrain,
}

impl FrameMode {
    pub fn parse(s: &str) -> Self {
        match s {
            // Swan, Watch and Goggles left the menu: files that name them mean Auto, which draws them for their consoles.
            "auto" | "swan" | "watch" | "goggles" => Self::Auto,
            "ambient" => Self::Ambient,
            "shell" => Self::Shell,
            "crt" => Self::Crt,
            "arcade" => Self::Arcade,
            "glow" => Self::Glow,
            "brick" => Self::Brick,
            "pocket" => Self::PocketSlim,
            "clamshell" => Self::Clamshell,
            "wide" => Self::Wide,
            "twin" => Self::Twin,
            "gloss" => Self::Gloss,
            "studio" => Self::Studio,
            "portable-tv" => Self::PortableTv,
            "woodgrain" => Self::Woodgrain,
            _ => Self::None,
        }
    }
}

/// The Picture settings of `options.rs`. Unknown or missing values mean the defaults.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct VideoSettings {
    pub scaling: Scaling,
    pub sharpness: Sharpness,
    pub effect: Effect,
    pub frame: FrameMode,
    pub frame_colour: crate::bezel::Colourway,
    pub frame_tv: crate::bezel::Cabinet,
    pub frame_brick: crate::bezel::BrickColour,
    pub frame_pocket: crate::bezel::SlimFinish,
    pub frame_clamshell: crate::bezel::ClamFinish,
    /// LCD grid with the Game Boy Advance screen's colours (`lcd_colour`; offered for the GBA only, see
    /// `options::applies`).
    pub lcd_colour: bool,
    /// Not a setting: the emulator corrects the colours itself (its colour correction option is on), so the LCD
    /// grid leaves its own out rather than doing it twice. pocket-core sets it from the core's options.
    pub core_colour: bool,
    /// Screen orientation (`orientation`).
    pub orientation: Orientation,
}

impl VideoSettings {
    pub fn from_values(values: &BTreeMap<String, String>) -> Self {
        let get = |k: &str| values.get(k).map(String::as_str).unwrap_or("");
        VideoSettings {
            scaling: Scaling::parse(get("scaling")),
            sharpness: Sharpness::parse(get("sharpness")),
            effect: Effect::parse(get("effect")),
            frame: FrameMode::parse(get("frame")),
            frame_colour: crate::bezel::Colourway::parse(get("frame_colour")),
            frame_tv: crate::bezel::Cabinet::parse(get("frame_tv")),
            frame_brick: crate::bezel::BrickColour::parse(get("frame_brick")),
            frame_pocket: crate::bezel::SlimFinish::parse(get("frame_pocket")),
            frame_clamshell: crate::bezel::ClamFinish::parse(get("frame_clamshell")),
            lcd_colour: get("lcd_colour") == "on",
            core_colour: false,
            orientation: Orientation::parse(get("orientation")),
        }
    }

    /// Applies one `set <key> <value>` of the pause menu; false for keys that are not Picture's.
    pub fn set(&mut self, key: &str, value: &str) -> bool {
        match key {
            "scaling" => self.scaling = Scaling::parse(value),
            "sharpness" => self.sharpness = Sharpness::parse(value),
            "effect" => self.effect = Effect::parse(value),
            "frame" => self.frame = FrameMode::parse(value),
            "frame_colour" => self.frame_colour = crate::bezel::Colourway::parse(value),
            "frame_tv" => self.frame_tv = crate::bezel::Cabinet::parse(value),
            "frame_brick" => self.frame_brick = crate::bezel::BrickColour::parse(value),
            "frame_pocket" => self.frame_pocket = crate::bezel::SlimFinish::parse(value),
            "frame_clamshell" => self.frame_clamshell = crate::bezel::ClamFinish::parse(value),
            "lcd_colour" => self.lcd_colour = value == "on",
            "orientation" => self.orientation = Orientation::parse(value),
            _ => return false,
        }
        true
    }

    /// The frame shows only when the picture leaves room around it: Fullscreen fills the screen, so it never
    /// gets one. A picture turned by Screen orientation keeps only the frames that have no up and down (Ambient,
    /// Glow): the others are drawn for a Pocket held as usual and would stand on their side.
    pub fn shows_frame(&self) -> bool {
        let upright = self.orientation == Orientation::Normal || matches!(self.frame, FrameMode::Ambient | FrameMode::Glow);
        self.frame != FrameMode::None && upright && matches!(self.scaling, Scaling::Native | Scaling::IntegerSmaller | Scaling::Aspect)
    }

    /// The built-in frame these settings ask for; None for Auto (a pack or the console's own
    /// style), Ambient (drawn live, see `ambient.rs`) and no frame.
    pub fn fixed_style(&self, system: &str) -> Option<crate::bezel::Style> {
        use crate::bezel::Style;
        match self.frame {
            FrameMode::Shell => Some(Style::Shell(self.frame_colour)),
            FrameMode::Crt => Some(Style::Crt(self.frame_tv)),
            FrameMode::Arcade => Some(Style::Arcade),
            FrameMode::Glow => Some(Style::Glow(crate::bezel::system_colour(system))),
            FrameMode::Brick => Some(Style::Brick(self.frame_brick)),
            FrameMode::PocketSlim => Some(Style::PocketSlim(self.frame_pocket)),
            FrameMode::Clamshell => Some(Style::Clamshell(self.frame_clamshell)),
            FrameMode::Wide => Some(Style::Wide),
            FrameMode::Twin => Some(Style::Twin),
            FrameMode::Gloss => Some(Style::Gloss),
            FrameMode::Studio => Some(Style::Studio),
            FrameMode::PortableTv => Some(Style::PortableTv),
            FrameMode::Woodgrain => Some(Style::Woodgrain),
            FrameMode::None | FrameMode::Auto | FrameMode::Ambient => None,
        }
    }

    /// Kids Mode: as large as fits in the game's shape, whatever is saved for the console or the game, and no frame
    /// (a frame would take room from the picture).
    pub fn for_kids(mut self) -> Self {
        self.scaling = Scaling::Aspect;
        self.frame = FrameMode::None;
        self
    }
}

