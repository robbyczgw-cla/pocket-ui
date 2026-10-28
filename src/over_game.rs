//! Over the game: what pocket-core draws over a running game besides its messages, one setting with four levels
//! (docs/HOST.md "Over the game"). Until 2026-10-25 these were three things with switches of their own: Options →
//! Extras → Show FPS (On, Sensors, Pixel Path), the HOME + Y shortcut for Pixel Path, and Settings → Curator →
//! "Pixel Path over every game".
//!
//! - **Off**: nothing is drawn, no reader thread, no report (`overlay::Overlay::draw` returns at once).
//! - **FPS**: "60 FPS   CPU 23%" in the top left corner, twice a second (`overlay::Hud::frame`).
//! - **FPS + sensors**: the same line with the temperature, the fan and the battery draw (`osd_sensors`).
//! - **Pixel Path**: Pixel Path's line beside the picture (`pixel_live`), which has the frame rate too.
//!
//! Where it is set: Settings → Playing → Over the game (`settings::Settings::over_game`, what every game starts with),
//! Options → Extras → Over the game (this console or this game, saved with Save changes as every option, key
//! [`KEY`]), and HOME + Y, which goes on to the next level for the running game until it quits or the option changes
//! (`hotkeys::Action::NextOverGame`). The pause menu's Pixel Path page (Settings → Curator → Pixel Path in the pause
//! menu) stays a switch of its own; its Y puts the Pixel Path level on or takes it off.

use std::collections::BTreeMap;

/// The Options key (`<data>/game-settings/…`): "off", "fps", "sensors", "pixel".
pub const KEY: &str = "over_game";
/// The Options key of the HOME + Y shortcut: "on" or "off".
pub const SHORTCUT_KEY: &str = "hk_over";
/// The keys these had before 2026-10-25; [`migrate`] turns them into the new ones.
const OLD_KEY: &str = "show_fps";
const OLD_SHORTCUT_KEY: &str = "hk_pixel";

/// What is drawn over the game.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default, serde::Serialize, serde::Deserialize)]
pub enum Level {
    #[default]
    #[serde(rename = "off")]
    Off,
    #[serde(rename = "fps")]
    Fps,
    #[serde(rename = "sensors")]
    Sensors,
    #[serde(rename = "pixel")]
    PixelPath,
}

impl Level {
    pub const ALL: [Level; 4] = [Level::Off, Level::Fps, Level::Sensors, Level::PixelPath];

    /// The value saved in settings.json and the Options files.
    pub fn value(self) -> &'static str {
        match self {
            Level::Off => "off",
            Level::Fps => "fps",
            Level::Sensors => "sensors",
            Level::PixelPath => "pixel",
        }
    }

    /// A saved value; Show FPS's "on" is FPS, anything unknown is Off.
    pub fn parse(s: &str) -> Level {
        match s {
            "fps" | "on" => Level::Fps,
            "sensors" => Level::Sensors,
            "pixel" => Level::PixelPath,
            _ => Level::Off,
        }
    }

    /// The name on the Settings row, in Options and in the HOME + Y message.
    pub fn label(self) -> &'static str {
        match self {
            Level::Off => "Off",
            Level::Fps => "FPS",
            Level::Sensors => "FPS + sensors",
            Level::PixelPath => "Pixel Path",
        }
    }

    /// The next level (`step` 1, A and Right on the Settings row, HOME + Y) or the one before (-1, Left), round.
    pub fn step(self, step: i32) -> Level {
        let n = Level::ALL.len() as i32;
        let i = Level::ALL.iter().position(|l| *l == self).unwrap_or(0) as i32;
        Level::ALL[(i + step).rem_euclid(n) as usize]
    }

    /// The FPS line in the top left corner.
    pub fn counter(self) -> bool {
        matches!(self, Level::Fps | Level::Sensors)
    }

    /// The device's state on the FPS line (a reader thread runs for it, `osd_sensors`).
    pub fn sensors(self) -> bool {
        self == Level::Sensors
    }

    /// Pixel Path's line beside the picture (a report twice a second, `pixel_live`).
    pub fn strip(self) -> bool {
        self == Level::PixelPath
    }
}

/// An Options file from before 2026-10-25: Show FPS becomes Over the game (Off, On → FPS, Sensors → FPS + sensors,
/// Pixel Path), and the Pixel Path shortcut the Over the game shortcut (switched off stays off). A value of the new
/// key wins over an old one. True when something changed.
pub fn migrate(values: &mut BTreeMap<String, String>) -> bool {
    let mut changed = false;
    if let Some(old) = values.remove(OLD_KEY) {
        values.entry(KEY.to_string()).or_insert_with(|| Level::parse(&old).value().to_string());
        changed = true;
    }
    if let Some(old) = values.remove(OLD_SHORTCUT_KEY) {
        values.entry(SHORTCUT_KEY.to_string()).or_insert(old);
        changed = true;
    }
    changed
}

/// Settings → Curator → "Pixel Path over every game" (until 2026-10-25, `curator.json`'s `pixel_live`) as the level
/// it meant: Pixel Path in every game without a choice of its own, but only while Pixel Path itself was on (without
/// it nothing was shown).
pub fn from_curator(pixel_path: bool, pixel_live: bool) -> Option<Level> {
    pixel_live.then_some(if pixel_path { Level::PixelPath } else { Level::Off })
}

/// pocket-ui's start: moves Settings → Curator → "Pixel Path over every game" into Settings → Over the game, once
/// (a level chosen there already stays). Writes settings.json first, then curator.json without the old switch, so a
/// stop in between only does it again. True when it moved something.
pub fn adopt_curator(data_dir: &std::path::Path, settings: &mut crate::settings::Settings, prefs: &mut crate::curator::prefs::Prefs) -> bool {
    let Some(level) = from_curator(prefs.pixel_path, prefs.pixel_live) else { return false };
    if settings.over_game == Level::Off {
        settings.over_game = level;
    }
    settings.save(data_dir);
    prefs.pixel_live = false;
    prefs.save(data_dir);
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;

    fn values(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    #[test]
    fn levels_go_round_both_ways_and_read_back_from_their_values() {
        assert_eq!(Level::ALL.map(Level::label), ["Off", "FPS", "FPS + sensors", "Pixel Path"]);
        for l in Level::ALL {
            assert_eq!(Level::parse(l.value()), l);
            assert_eq!(l.step(1).step(-1), l);
            assert_eq!(serde_json::to_string(&l).unwrap(), format!("\"{}\"", l.value()));
        }
        assert_eq!(Level::Off.step(-1), Level::PixelPath);
        assert_eq!(Level::PixelPath.step(1), Level::Off);
        let mut seen = vec![Level::Off];
        while seen.len() < 5 {
            seen.push(seen.last().unwrap().step(1));
        }
        assert_eq!(seen, [Level::Off, Level::Fps, Level::Sensors, Level::PixelPath, Level::Off], "HOME + Y goes round");
        assert_eq!(Level::parse("junk"), Level::Off);
    }

    #[test]
    fn each_level_draws_only_its_piece_and_off_draws_nothing() {
        let pieces = |l: Level| (l.counter(), l.sensors(), l.strip());
        assert_eq!(pieces(Level::Off), (false, false, false));
        assert_eq!(pieces(Level::Fps), (true, false, false));
        assert_eq!(pieces(Level::Sensors), (true, true, false));
        assert_eq!(pieces(Level::PixelPath), (false, false, true), "the strip has the frame rate itself");
    }

    #[test]
    fn show_fps_and_the_pixel_path_shortcut_become_over_the_game() {
        for (old, new) in [("off", "off"), ("on", "fps"), ("sensors", "sensors"), ("pixel", "pixel"), ("junk", "off")] {
            let mut v = values(&[("show_fps", old), ("rewind", "on")]);
            assert!(migrate(&mut v));
            assert_eq!(v, values(&[("over_game", new), ("rewind", "on")]), "{old}");
        }
        let mut v = values(&[("hk_pixel", "off")]);
        assert!(migrate(&mut v));
        assert_eq!(v, values(&[("hk_over", "off")]), "a shortcut switched off stays off");
        let mut v = values(&[("show_fps", "pixel"), ("over_game", "fps")]);
        migrate(&mut v);
        assert_eq!(v, values(&[("over_game", "fps")]), "the new key wins");
        let mut v = values(&[("over_game", "sensors")]);
        assert!(!migrate(&mut v), "nothing to do for a new file");
    }

    #[test]
    fn pixel_path_over_every_game_becomes_the_pixel_path_level_only_with_pixel_path_on() {
        assert_eq!(from_curator(true, true), Some(Level::PixelPath));
        assert_eq!(from_curator(false, true), Some(Level::Off), "nothing was shown without Pixel Path");
        assert_eq!(from_curator(true, false), None, "not switched on: nothing to move");
        assert_eq!(from_curator(false, false), None);
    }

    #[test]
    fn the_curator_switch_moves_to_settings_once_and_is_not_written_again() {
        use crate::curator::prefs::Prefs;
        use crate::settings::Settings;
        let d = TestDir::new("over-game-adopt");
        std::fs::write(d.join("curator.json"), br#"{"pixel_path": true, "pixel_live": true, "lens": false}"#).unwrap();
        std::fs::write(d.join("settings.json"), br#"{"onboarded": true, "theme": "cards"}"#).unwrap();
        let (mut s, mut p) = (Settings::load(&d), Prefs::load(&d));
        assert!(p.pixel_live, "read from a file of before");
        assert!(adopt_curator(&d, &mut s, &mut p));
        assert_eq!((s.over_game, crate::settings::over_game(&d)), (Level::PixelPath, Level::PixelPath));
        let p = Prefs::load(&d);
        assert!(!p.pixel_live && p.pixel_path && !p.lens, "the rest of Curator stays as it was");
        assert!(!std::fs::read_to_string(d.join("curator.json")).unwrap().contains("pixel_live"));
        let (mut s, mut p) = (Settings::load(&d), Prefs::load(&d));
        assert!(!adopt_curator(&d, &mut s, &mut p), "once");
        assert!(Settings::load(&d).onboarded, "the rest of settings.json stays");
        // A level chosen in Settings already is kept; the old switch only goes.
        std::fs::write(d.join("curator.json"), br#"{"pixel_path": true, "pixel_live": true}"#).unwrap();
        let (mut s, mut p) = (Settings { over_game: Level::Fps, ..Settings::load(&d) }, Prefs::load(&d));
        assert!(adopt_curator(&d, &mut s, &mut p));
        assert_eq!(crate::settings::over_game(&d), Level::Fps);
    }
}
