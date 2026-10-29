//! `<data>/settings.json`: what Settings and the first-start setup leave behind (theme, update channel,
//! store catalog, and whether the setup has been done). Lives in the library so the load and save can
//! be tested; `main.rs` only uses it.

use std::path::Path;

use crate::fsutil;

/// A field that falls back to its default when its value doesn't fit (a theme this version doesn't know,
/// a number where a string should be), instead of failing the whole file and losing the other settings.
pub(crate) fn lenient<'de, D, T>(d: D) -> Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::de::DeserializeOwned + Default,
{
    let v = <serde_json::Value as serde::Deserialize>::deserialize(d)?;
    Ok(serde_json::from_value(v).unwrap_or_default())
}

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

/// Settings → Screen frames: where a game's screen frame is Auto when neither its console nor the game has a frame
/// of its own (Options → Picture → Screen frame). `options::GameSettings` reads it.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, serde::Serialize, serde::Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Frames {
    /// The start setting (the owner, 2026-10-23: a clean picture out of the box): no console starts with a frame.
    #[default]
    Off,
    /// Auto on the handhelds (`options::HANDHELDS`), None on the home consoles.
    Handhelds,
    /// Auto on every console: the home consoles get the monitor surround (`bezel::style_for`).
    Every,
}

impl Frames {
    pub const ALL: [Frames; 3] = [Frames::Off, Frames::Handhelds, Frames::Every];

    /// The value on the Settings row.
    pub fn label(self) -> &'static str {
        match self {
            Frames::Off => "Off",
            Frames::Handhelds => "Handhelds",
            Frames::Every => "Every console",
        }
    }

    /// The next value on the Settings row (A, Right): Off → Handhelds → Every console → Off.
    pub fn toggled(self) -> Frames {
        match self {
            Frames::Off => Frames::Handhelds,
            Frames::Handhelds => Frames::Every,
            Frames::Every => Frames::Off,
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize, Default, Debug, PartialEq)]
pub struct Settings {
    #[serde(default, deserialize_with = "lenient")]
    pub theme: Theme,
    /// First-start setup has been completed or skipped.
    #[serde(default, deserialize_with = "lenient")]
    pub onboarded: bool,
    /// Update channel (update.json); the built-in one when unset.
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    pub update_url: Option<String>,
    /// Store catalog (catalog.json, `file://` works too); the built-in one when unset.
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    pub store_url: Option<String>,
    /// Settings > Look > Systems on the shelf: system ids left off the shelf (all shown when empty).
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Vec::is_empty")]
    pub hidden_systems: Vec<String>,
    /// Settings > Look > Systems on the shelf: "PC streaming" (Moonlight) on Home. Off until ticked, because
    /// ROCKNIX installs Moonlight on every device and most players have no PC to stream from.
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "std::ops::Not::not")]
    pub streaming_on_home: bool,
    /// Settings > Look > Appearance; dark when unset, so a settings.json from before it keeps the old look.
    #[serde(default, deserialize_with = "lenient")]
    pub appearance: Appearance,
    /// Settings > On start: Home, or the game played last (`on_start`); Home when unset.
    #[serde(default, deserialize_with = "lenient")]
    pub on_start: crate::on_start::OnStart,
    /// Settings > Remember where I quit, kept as its opposite so that unset (a new device, a settings.json from before
    /// it) is On: Quit keeps the moment to resume from. Off: Quit writes the in-game saves but no new resume point,
    /// where the emulator can leave it out (`quit::offers_quit_without_saving`; not RetroArch).
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "std::ops::Not::not")]
    pub forget_where_quit: bool,
    /// Settings > Playing > Button layout: which face button confirms in pocket-ui's menus (`button_layout`); Nintendo
    /// (A on the right) except Odin 2 Mini/Portal (Xbox) when unset. An explicit saved choice wins.
    #[serde(default, deserialize_with = "lenient")]
    pub button_layout: crate::button_layout::ButtonLayout,
    /// Settings > Look > Motion; Full when unset.
    #[serde(default, deserialize_with = "lenient")]
    pub motion: Motion,
    /// Settings > Screen frames; Off when unset (the start setting since 2026-10-23, also for a settings.json from before it).
    #[serde(default, deserialize_with = "lenient")]
    pub frames: Frames,
    /// The version of pocket-ui whose "New in this version" cards were shown (or that ran the first start); unset on
    /// a device set up before the cards existed, which counts as `features::BASELINE` (`features::due`).
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    pub seen_version: Option<String>,
    /// Feature cards put off with "Later" (`features::Feature::id`); Settings → Tour counts them.
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Vec::is_empty")]
    pub tour_later: Vec<String>,
    /// Settings → Language (`crate::i18n`); unset until chosen, which follows the system's language
    /// (`Lang::system_default`).
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    pub language: Option<crate::i18n::Lang>,
    /// Settings > Games > Extras > Game pictures: "Box art only" instead of "Box art and screenshots". Kept as the
    /// opposite so that unset (a new device, a settings.json from before it) shows the screenshots and title
    /// screens too, as the same background download that fetches the box art brings them.
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "std::ops::Not::not")]
    pub box_art_only: bool,
    /// Settings > Games > Extras > Pictures: "Off", nothing is downloaded by itself (`Art::Off`, which also means box
    /// art only). Written only with `box_art_only`, so a settings.json from before it downloads as it always did.
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "std::ops::Not::not")]
    pub art_off: bool,
    /// Settings → Look & feel → Text size; Normal when unset (and not written then), so nothing changes for a
    /// settings.json from before it.
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "is_normal")]
    pub text_size: TextSize,
    /// Settings → Playing → Over the game: what pocket-core draws over every game that has no choice of its own in
    /// Options (`over_game`). Off when unset (and not written then).
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "is_off")]
    pub over_game: crate::over_game::Level,
    /// Settings → Look & feel → Launch animation, kept as its opposite so that unset is On: in the Cabinet and the
    /// Shelf look a game's case goes into its console while the game loads (`app_cabinet`).
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "std::ops::Not::not")]
    pub no_launch_animation: bool,
    /// The game card opens as a page instead of the game's box (`game_box`): SELECT on the card switches, and the
    /// choice is kept here; unset is the box.
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "std::ops::Not::not")]
    pub details_page: bool,
}

fn is_off(l: &crate::over_game::Level) -> bool {
    *l == crate::over_game::Level::Off
}

fn is_normal(t: &TextSize) -> bool {
    *t == TextSize::Normal
}

const FILE: &str = "settings.json";

impl Settings {
    /// The saved settings. A file that is there but unreadable (0 bytes, cut off) is replaced by its last good
    /// copy (`settings.json.bak`); with none, the setup is still counted as done, because the file is only
    /// ever written after it ran. Either way it's said on stderr.
    pub fn load(data_dir: &Path) -> Self {
        let path = data_dir.join(FILE);
        fsutil::read_json_or_backup(&path).unwrap_or_else(|| {
            let mut s = Settings::default();
            if path.exists() {
                eprintln!(
                    "settings: {} is damaged and has no backup; starting from the defaults, without the first-start setup",
                    path.display()
                );
                s.onboarded = true;
            }
            s
        })
    }

    pub fn save(&self, data_dir: &Path) {
        if let Err(e) = fsutil::write_json(&data_dir.join(FILE), self) {
            eprintln!("settings: can't save {FILE}: {e}");
        }
    }
}

/// Settings → Screen frames as saved, for the Options of a game (`options::GameSettings::load`, in pocket-ui and in
/// pocket-core). Only the one field is read; a missing or damaged file is the start setting.
pub fn frames(data_dir: &Path) -> Frames {
    #[derive(serde::Deserialize, Default)]
    struct Only {
        #[serde(default, deserialize_with = "lenient")]
        frames: Frames,
    }
    std::fs::read(data_dir.join(FILE)).ok().and_then(|b| serde_json::from_slice::<Only>(&b).ok()).unwrap_or_default().frames
}

/// Settings → Playing → Over the game as saved, for the Options of a game (`options::GameSettings::load`, in pocket-ui
/// and in pocket-core). Only the one field is read; a missing or damaged file is Off.
pub fn over_game(data_dir: &Path) -> crate::over_game::Level {
    #[derive(serde::Deserialize, Default)]
    struct Only {
        #[serde(default, deserialize_with = "lenient")]
        over_game: crate::over_game::Level,
    }
    std::fs::read(data_dir.join(FILE)).ok().and_then(|b| serde_json::from_slice::<Only>(&b).ok()).unwrap_or_default().over_game
}

/// Settings > Games > Extras > Pictures: what pocket-ui fetches from the libretro thumbnail collection by itself.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Art {
    /// Box art only.
    BoxArt,
    /// Box art, screenshots and title screens: the start setting.
    #[default]
    Pictures,
    /// Nothing by itself. The Cover art row still looks up box art when asked.
    Off,
}

impl Art {
    /// In the order of the row's choices.
    pub const ALL: [Art; 3] = [Art::BoxArt, Art::Pictures, Art::Off];

    pub fn label(self) -> &'static str {
        match self {
            Art::BoxArt => "Box art",
            Art::Pictures => "Box art and screenshots",
            Art::Off => "Off",
        }
    }

    /// From the two saved flags (`Off` wins over `box_art_only`).
    pub fn from_flags(box_art_only: bool, off: bool) -> Art {
        if off {
            Art::Off
        } else if box_art_only {
            Art::BoxArt
        } else {
            Art::Pictures
        }
    }

    /// (`box_art_only`, `art_off`) as saved.
    pub fn flags(self) -> (bool, bool) {
        (self != Art::Pictures, self == Art::Off)
    }

    /// The next choice in `ALL`, wrapping.
    pub fn next(self) -> Art {
        Art::ALL[(Art::ALL.iter().position(|a| *a == self).unwrap_or(0) + 1) % Art::ALL.len()]
    }

    /// The previous choice in `ALL`, wrapping.
    pub fn prev(self) -> Art {
        Art::ALL[(Art::ALL.iter().position(|a| *a == self).unwrap_or(0) + Art::ALL.len() - 1) % Art::ALL.len()]
    }
}

/// Whether the screenshots and title screens beside the box art are on (Settings → Games → Extras → Game pictures), as
/// saved; on when unset. For the web companion, which reads only this field.
pub fn game_pictures(data_dir: &Path) -> bool {
    #[derive(serde::Deserialize, Default)]
    struct Only {
        #[serde(default, deserialize_with = "lenient")]
        box_art_only: bool,
    }
    !std::fs::read(data_dir.join(FILE)).ok().and_then(|b| serde_json::from_slice::<Only>(&b).ok()).unwrap_or_default().box_art_only
}

/// Settings → Look & feel → Text size as saved, Normal when unset or unreadable; for safe mode, which reads only the
/// fields it needs.
pub fn text_size(data_dir: &Path) -> TextSize {
    #[derive(serde::Deserialize, Default)]
    struct Only {
        #[serde(default, deserialize_with = "lenient")]
        text_size: TextSize,
    }
    std::fs::read(data_dir.join(FILE)).ok().and_then(|b| serde_json::from_slice::<Only>(&b).ok()).unwrap_or_default().text_size
}

/// Settings → Language as saved, or the system's when none was chosen; for pocket-core and safe mode, which read only
/// this field.
pub fn language(data_dir: &Path) -> crate::i18n::Lang {
    #[derive(serde::Deserialize, Default)]
    struct Only {
        #[serde(default, deserialize_with = "lenient")]
        language: Option<crate::i18n::Lang>,
    }
    std::fs::read(data_dir.join(FILE))
        .ok()
        .and_then(|b| serde_json::from_slice::<Only>(&b).ok())
        .unwrap_or_default()
        .language
        .unwrap_or_else(crate::i18n::Lang::system_default)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;

    fn dir(name: &str) -> crate::testutil::TestDir {
        TestDir::new(&format!("settings-{name}"))
    }

    fn chosen() -> Settings {
        Settings {
            theme: Theme::Cards,
            onboarded: true,
            update_url: Some("https://example.test/update.json".into()),
            store_url: Some("file:///store/catalog.json".into()),
            hidden_systems: vec!["ports".into(), "pico-8".into()],
            streaming_on_home: true,
            appearance: Appearance::Light,
            on_start: crate::on_start::OnStart::Continue,
            forget_where_quit: true,
            button_layout: crate::button_layout::ButtonLayout::Xbox,
            motion: Motion::Reduced,
            frames: Frames::Every,
            seen_version: Some("0.2.0".into()),
            tour_later: vec!["pixel_path".into()],
            language: Some(crate::i18n::Lang::De),
            box_art_only: true,
            art_off: false,
            text_size: TextSize::Large,
            over_game: crate::over_game::Level::Sensors,
            no_launch_animation: true,
            details_page: true,
        }
    }

    #[test]
    fn the_text_size_is_normal_until_chosen_survives_a_restart_and_falls_back_when_unknown() {
        let d = dir("text-size");
        assert_eq!(Settings::load(&d).text_size, TextSize::Normal, "a new device");
        std::fs::write(d.join(FILE), br#"{"theme": "split", "onboarded": true}"#).unwrap();
        assert_eq!(Settings::load(&d).text_size, TextSize::Normal, "a settings.json from before it");
        Settings { text_size: TextSize::Large, ..chosen() }.save(&d);
        assert_eq!(Settings::load(&d).text_size, TextSize::Large);
        assert!(std::fs::read_to_string(d.join(FILE)).unwrap().contains(r#""text_size": "large""#));
        Settings { text_size: TextSize::Medium, ..chosen() }.save(&d);
        assert_eq!(Settings::load(&d).text_size, TextSize::Medium);
        assert!(std::fs::read_to_string(d.join(FILE)).unwrap().contains(r#""text_size": "medium""#));
        Settings { text_size: TextSize::Normal, ..chosen() }.save(&d);
        assert!(!std::fs::read_to_string(d.join(FILE)).unwrap().contains("text_size"), "the default is not written");
        std::fs::write(d.join(FILE), br#"{"theme": "cards", "text_size": "huge"}"#).unwrap();
        let s = Settings::load(&d);
        assert_eq!((s.text_size, s.theme), (TextSize::Normal, Theme::Cards), "a size this version doesn't know is dropped, not the file");
        assert_eq!(TextSize::ALL.map(|t| t.next()), [TextSize::Medium, TextSize::Large, TextSize::Normal]);
        assert_eq!(TextSize::ALL.map(|t| t.previous()), [TextSize::Large, TextSize::Normal, TextSize::Medium]);
        assert_eq!(text_size(&d), TextSize::Normal, "safe mode's read: the unknown value is Normal");
        std::fs::write(d.join(FILE), br#"{"text_size": "large", "theme": 7}"#).unwrap();
        assert_eq!(text_size(&d), TextSize::Large, "the one field, whatever else is wrong");
        std::fs::write(d.join(FILE), "{damaged").unwrap();
        assert_eq!(text_size(&d), TextSize::Normal);
    }

    #[test]
    fn the_button_layout_is_kept_and_nintendo_when_unset_or_wrong() {
        use crate::button_layout::ButtonLayout;
        let d = dir("button-layout");
        std::fs::write(d.join(FILE), br#"{"theme": "split", "onboarded": true}"#).unwrap();
        assert_eq!(Settings::load(&d).button_layout, ButtonLayout::Nintendo, "a settings.json from before it: as printed");
        assert_eq!(Settings::load(&d.join("nothing")).button_layout, ButtonLayout::Nintendo, "a new device");
        for wrong in [r#""playstation""#, "7", "null", r#"{"a": 1}"#, r#""XBOX""#] {
            std::fs::write(d.join(FILE), format!(r#"{{"theme": "cards", "button_layout": {wrong}}}"#)).unwrap();
            let s = Settings::load(&d);
            assert_eq!((s.button_layout, s.theme), (ButtonLayout::Nintendo, Theme::Cards), "{wrong}: the rest is kept");
        }
        std::fs::write(d.join(FILE), "{broken").unwrap();
        assert_eq!(Settings::load(&d).button_layout, ButtonLayout::Nintendo, "a damaged file");
        Settings { button_layout: ButtonLayout::Xbox, ..chosen() }.save(&d);
        assert_eq!(Settings::load(&d).button_layout, ButtonLayout::Xbox);
        std::fs::write(d.join(FILE), br#"{"button_layout": "xbox"}"#).unwrap();
        assert_eq!(Settings::load(&d).button_layout, ButtonLayout::Xbox);
    }

    #[test]
    fn the_language_is_kept_and_unset_until_chosen() {
        let d = dir("language");
        std::fs::write(d.join(FILE), br#"{"theme": "split", "onboarded": true}"#).unwrap();
        assert_eq!(Settings::load(&d).language, None, "a settings.json from before it: the system's language");
        std::fs::write(d.join(FILE), br#"{"language": "de"}"#).unwrap();
        assert_eq!(Settings::load(&d).language, Some(crate::i18n::Lang::De));
        assert_eq!(language(&d), crate::i18n::Lang::De);
        std::fs::write(d.join(FILE), br#"{"language": "klingon", "theme": "split"}"#).unwrap();
        let s = Settings::load(&d);
        assert_eq!((s.language, s.theme), (None, Theme::Split), "a language this version doesn't know is dropped, not the file");
    }

    #[test]
    fn frames_seen_version_and_put_off_cards_survive_a_restart_and_default_quietly() {
        let d = dir("tour");
        let s = Settings::load(&d);
        assert_eq!((s.frames, s.seen_version.as_deref(), s.tour_later.len()), (Frames::Off, None, 0), "a new device: no frames");
        assert_eq!(frames(&d), Frames::Off, "no file: the start setting");
        // A settings.json from before them: nothing changes, nothing is shown as new.
        std::fs::write(d.join(FILE), br#"{"theme": "split", "onboarded": true}"#).unwrap();
        let s = Settings::load(&d);
        assert_eq!((s.frames, s.seen_version, s.theme), (Frames::Off, None, Theme::Split));
        chosen().save(&d);
        let s = Settings::load(&d);
        assert_eq!((s.frames, s.seen_version.as_deref(), s.tour_later.clone()), (Frames::Every, Some("0.2.0"), vec!["pixel_path".to_string()]));
        assert_eq!(frames(&d), Frames::Every);
        std::fs::write(d.join(FILE), br#"{"onboarded": true, "frames": "sometimes", "tour_later": 3}"#).unwrap();
        let s = Settings::load(&d);
        assert_eq!((s.frames, s.tour_later.len(), s.onboarded), (Frames::Off, 0, true), "unknown values fall back");
        assert_eq!(frames(&d), Frames::Off);
        std::fs::write(d.join(FILE), br#"{"onboarded": true, "frames": "handhelds"}"#).unwrap();
        assert_eq!(frames(&d), Frames::Handhelds, "a saved choice stays");
        assert_eq!((Frames::Off.toggled(), Frames::Handhelds.toggled(), Frames::Every.toggled()), (Frames::Handhelds, Frames::Every, Frames::Off));
        assert_eq!(Frames::ALL.map(Frames::label), ["Off", "Handhelds", "Every console"]);
    }

    #[test]
    fn over_the_game_is_off_until_chosen_survives_a_restart_and_falls_back_when_unknown() {
        use crate::over_game::Level;
        let d = dir("over-game");
        assert_eq!((Settings::load(&d).over_game, over_game(&d)), (Level::Off, Level::Off), "a new device");
        std::fs::write(d.join(FILE), br#"{"theme": "split", "onboarded": true}"#).unwrap();
        assert_eq!(over_game(&d), Level::Off, "a settings.json from before it");
        for l in Level::ALL {
            Settings { over_game: l, ..chosen() }.save(&d);
            assert_eq!((Settings::load(&d).over_game, over_game(&d)), (l, l));
        }
        assert!(std::fs::read_to_string(d.join(FILE)).unwrap().contains(r#""over_game": "pixel""#));
        Settings { over_game: Level::Off, ..chosen() }.save(&d);
        assert!(!std::fs::read_to_string(d.join(FILE)).unwrap().contains("over_game"), "the default is not written");
        std::fs::write(d.join(FILE), br#"{"theme": "cards", "over_game": "everything"}"#).unwrap();
        assert_eq!((Settings::load(&d).theme, over_game(&d)), (Theme::Cards, Level::Off), "an unknown level is dropped, not the file");
    }

    #[test]
    fn game_pictures_are_on_until_box_art_only_is_chosen_and_survive_a_restart() {
        let d = dir("pictures");
        assert!(!Settings::load(&d).box_art_only, "a new device: with screenshots");
        assert!(game_pictures(&d));
        std::fs::write(d.join(FILE), br#"{"theme": "cards", "onboarded": true}"#).unwrap();
        assert!(!Settings::load(&d).box_art_only, "a settings.json from before it");
        Settings { box_art_only: true, ..chosen() }.save(&d);
        assert!(Settings::load(&d).box_art_only);
        assert!(!game_pictures(&d));
        Settings { box_art_only: false, ..chosen() }.save(&d);
        assert!(game_pictures(&d));
        assert!(!std::fs::read_to_string(d.join(FILE)).unwrap().contains("box_art_only"), "the default is not written");
        std::fs::write(d.join(FILE), br#"{"onboarded": true, "box_art_only": "yes please"}"#).unwrap();
        assert!(game_pictures(&d), "a value this version doesn't know is dropped, not the file");
    }

    #[test]
    fn the_pictures_choice_is_the_two_flags_and_off_is_only_written_when_chosen() {
        for a in Art::ALL {
            let (only, off) = a.flags();
            assert_eq!(Art::from_flags(only, off), a, "{a:?} reads back");
            assert_ne!(a.next(), a);
            assert_eq!(a.next().prev(), a);
        }
        assert_eq!(Art::from_flags(false, true), Art::Off, "Off wins over a hand-edited file");
        assert_eq!(Art::default(), Art::Pictures, "the start setting: box art and screenshots");
        // Off is box art only that also downloads nothing, so the web companion and the shelf agree.
        assert_eq!(Art::Off.flags(), (true, true));
        let d = dir("art-off");
        assert!(!Settings::load(&d).art_off, "a new device downloads");
        std::fs::write(d.join(FILE), br#"{"theme": "cards", "onboarded": true, "box_art_only": true}"#).unwrap();
        assert!(!Settings::load(&d).art_off, "box art only from before Off existed still downloads the box art");
        Settings { box_art_only: true, art_off: true, ..chosen() }.save(&d);
        assert!(Settings::load(&d).art_off);
        assert!(!game_pictures(&d));
        Settings { box_art_only: true, art_off: false, ..chosen() }.save(&d);
        assert!(!std::fs::read_to_string(d.join(FILE)).unwrap().contains("art_off"), "the default is not written");
        std::fs::write(d.join(FILE), br#"{"onboarded": true, "art_off": "maybe"}"#).unwrap();
        assert!(!Settings::load(&d).art_off, "a value this version doesn't know is dropped, not the file");
    }

    #[test]
    fn motion_is_full_until_chosen_survives_a_restart_and_falls_back_when_unknown() {
        let d = dir("motion");
        assert_eq!(Settings::load(&d).motion, Motion::Full, "a new device: full");
        for m in Motion::ALL {
            Settings { motion: m, ..chosen() }.save(&d);
            assert_eq!(Settings::load(&d).motion, m);
        }
        std::fs::write(d.join(FILE), br#"{"theme": "cards", "onboarded": true, "motion": "wobbly"}"#).unwrap();
        let s = Settings::load(&d);
        assert_eq!((s.motion, s.theme), (Motion::Full, Theme::Cards));
        assert_eq!(Motion::Full.next(), Motion::Reduced);
        assert_eq!(Motion::Reduced.next(), Motion::Off);
        assert_eq!(Motion::Off.next(), Motion::Full);
    }

    #[test]
    fn pc_streaming_is_off_home_until_ticked_and_stays_ticked() {
        let d = dir("streaming");
        assert!(!Settings::load(&d).streaming_on_home, "off on a new device");
        // A settings.json from before the tick existed: still off, although Moonlight is installed.
        std::fs::write(d.join(FILE), br#"{"theme": "split", "onboarded": true}"#).unwrap();
        assert!(!Settings::load(&d).streaming_on_home);
        chosen().save(&d);
        assert!(Settings::load(&d).streaming_on_home);
    }

    #[test]
    fn every_system_is_on_the_shelf_by_default() {
        let d = dir("hidden-default");
        assert!(Settings::load(&d).hidden_systems.is_empty());
        // A settings.json from before the switch existed.
        std::fs::write(d.join(FILE), br#"{"theme": "split", "onboarded": true}"#).unwrap();
        let s = Settings::load(&d);
        assert!(s.hidden_systems.is_empty());
        assert_eq!(s.theme, Theme::Split);
    }

    #[test]
    fn hidden_systems_survive_a_restart_and_a_damaged_file() {
        let d = dir("hidden-saved");
        chosen().save(&d);
        assert_eq!(Settings::load(&d).hidden_systems, vec!["ports".to_string(), "pico-8".to_string()]);
        assert!(crate::fsutil::backup_path(&d.join(FILE)).exists(), "written through fsutil, with a backup");
        // Cut off by a power loss: the backup written with it has the same list.
        let whole = std::fs::read(d.join(FILE)).unwrap();
        std::fs::write(d.join(FILE), &whole[..whole.len() - 8]).unwrap();
        assert_eq!(Settings::load(&d).hidden_systems, vec!["ports".to_string(), "pico-8".to_string()]);
    }

    #[test]
    fn a_hidden_list_of_the_wrong_shape_does_not_drop_the_rest() {
        let d = dir("hidden-wrong");
        std::fs::write(d.join(FILE), br#"{"theme": "cards", "onboarded": true, "hidden_systems": "ports"}"#).unwrap();
        let s = Settings::load(&d);
        assert!(s.hidden_systems.is_empty());
        assert_eq!(s.theme, Theme::Cards);
    }

    #[test]
    fn appearance_is_dark_until_chosen_and_survives_a_restart() {
        let d = dir("appearance");
        assert_eq!(Settings::load(&d).appearance, Appearance::Dark, "a new device: dark");
        // A settings.json from before the choice existed: dark, so nothing changes for anyone.
        std::fs::write(d.join(FILE), br#"{"theme": "split", "onboarded": true}"#).unwrap();
        let s = Settings::load(&d);
        assert_eq!((s.appearance, s.theme), (Appearance::Dark, Theme::Split));
        for a in [Appearance::Light, Appearance::Auto, Appearance::Dark] {
            Settings { appearance: a, ..chosen() }.save(&d);
            assert_eq!(Settings::load(&d).appearance, a);
        }
        assert!(crate::fsutil::backup_path(&d.join(FILE)).exists(), "written through fsutil, with a backup");
        let text = std::fs::read_to_string(d.join(FILE)).unwrap();
        Settings { appearance: Appearance::Auto, ..chosen() }.save(&d);
        let auto = std::fs::read_to_string(d.join(FILE)).unwrap();
        assert!(auto.contains(r#""appearance": "auto""#) || auto.contains(r#""appearance":"auto""#), "{auto}");
        assert!(!text.is_empty());
    }

    #[test]
    fn an_unknown_appearance_falls_back_to_dark_and_keeps_the_rest() {
        let d = dir("appearance-unknown");
        std::fs::write(d.join(FILE), br#"{"theme": "cards", "onboarded": true, "appearance": "sepia"}"#).unwrap();
        let s = Settings::load(&d);
        assert_eq!((s.appearance, s.theme, s.onboarded), (Appearance::Dark, Theme::Cards, true));
        std::fs::write(d.join(FILE), br#"{"theme": "cards", "onboarded": true, "appearance": 1}"#).unwrap();
        assert_eq!(Settings::load(&d).appearance, Appearance::Dark);
    }

    #[test]
    fn on_start_is_home_until_chosen_and_survives_a_restart() {
        use crate::on_start::OnStart;
        let d = dir("on-start");
        assert_eq!(Settings::load(&d).on_start, OnStart::Home, "a new device: Home");
        // A settings.json from before the choice existed, and one with a value this version doesn't know.
        std::fs::write(d.join(FILE), br#"{"theme": "split", "onboarded": true}"#).unwrap();
        assert_eq!(Settings::load(&d).on_start, OnStart::Home);
        std::fs::write(d.join(FILE), br#"{"theme": "split", "onboarded": true, "on_start": "menu"}"#).unwrap();
        let s = Settings::load(&d);
        assert_eq!((s.on_start, s.theme), (OnStart::Home, Theme::Split));
        Settings { on_start: OnStart::Continue, ..chosen() }.save(&d);
        assert_eq!(Settings::load(&d).on_start, OnStart::Continue);
    }

    #[test]
    fn remember_where_i_quit_is_on_until_switched_off_and_survives_a_restart() {
        let d = dir("remember-quit");
        assert!(!Settings::load(&d).forget_where_quit, "a new device: On");
        // A settings.json from before the switch, and one with a value this version doesn't read.
        std::fs::write(d.join(FILE), br#"{"theme": "split", "onboarded": true}"#).unwrap();
        assert!(!Settings::load(&d).forget_where_quit);
        std::fs::write(d.join(FILE), br#"{"theme": "split", "onboarded": true, "forget_where_quit": "yes"}"#).unwrap();
        assert!(!Settings::load(&d).forget_where_quit);
        Settings { forget_where_quit: true, ..chosen() }.save(&d);
        assert!(Settings::load(&d).forget_where_quit);
        Settings { forget_where_quit: false, ..chosen() }.save(&d);
        assert!(!std::fs::read_to_string(d.join(FILE)).unwrap().contains("forget_where_quit"), "On is not written");
    }

    #[test]
    fn appearance_choices_in_order() {
        assert_eq!(Appearance::ALL, [Appearance::Dark, Appearance::Light, Appearance::Auto]);
        assert_eq!(Appearance::ALL.map(Appearance::name), ["Dark", "Light", "Automatic"]);
    }

    #[test]
    fn a_fresh_install_starts_the_setup() {
        let d = dir("fresh");
        let s = Settings::load(&d);
        assert!(!s.onboarded, "no file: first start");
        assert_eq!(s, Settings::default());
    }

    #[test]
    fn save_and_load_round_trip() {
        let d = dir("roundtrip");
        chosen().save(&d);
        assert_eq!(Settings::load(&d), chosen());
    }

    #[test]
    fn an_empty_settings_file_does_not_bring_the_setup_back() {
        let d = dir("empty");
        chosen().save(&d);
        std::fs::write(d.join(FILE), b"").unwrap();
        assert_eq!(Settings::load(&d), chosen(), "0 bytes: the last good copy is used");
    }

    #[test]
    fn a_truncated_settings_file_keeps_theme_and_update_url() {
        let d = dir("truncated");
        chosen().save(&d);
        let whole = std::fs::read(d.join(FILE)).unwrap();
        std::fs::write(d.join(FILE), &whole[..whole.len() / 2]).unwrap();
        assert_eq!(Settings::load(&d), chosen());
    }

    #[test]
    fn a_damaged_file_without_a_backup_still_counts_as_set_up() {
        let d = dir("nobackup");
        std::fs::write(d.join(FILE), b"{\"theme\": \"ca").unwrap();
        let s = Settings::load(&d);
        assert!(s.onboarded, "a settings file only exists once the setup ran");
    }

    #[test]
    fn an_unknown_theme_name_does_not_drop_the_other_settings() {
        let d = dir("unknown-theme");
        std::fs::write(
            d.join(FILE),
            br#"{"theme": "neon", "onboarded": true, "update_url": "https://example.test/u.json"}"#,
        )
        .unwrap();
        let s = Settings::load(&d);
        assert!(s.onboarded);
        assert_eq!(s.update_url.as_deref(), Some("https://example.test/u.json"));
        assert_eq!(s.theme, Theme::Shelf);
    }

    #[test]
    fn the_cabinet_is_the_fifth_look_and_is_kept() {
        assert_eq!(Theme::ALL.len(), 5);
        assert_eq!(Theme::ALL[4], Theme::Cabinet, "after the four there were, so their places in the picker stay");
        assert_eq!(Theme::Cabinet.name(), "Cabinet");
        assert!(!Theme::Cabinet.blurb().is_empty());
        let d = dir("cabinet");
        let chosen = Settings { theme: Theme::Cabinet, onboarded: true, ..Default::default() };
        chosen.save(&d);
        let saved = std::fs::read_to_string(d.join(FILE)).unwrap();
        assert!(saved.contains(r#""cabinet""#), "{saved}");
        assert_eq!(Settings::load(&d).theme, Theme::Cabinet);
    }
}
