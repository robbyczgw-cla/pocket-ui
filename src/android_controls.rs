//! Volume and brightness of Android's handheld view (android/handheld): the Quick settings rows act on what Android lets
//! an app change without a permission, and say nothing where it doesn't.
//!
//! - **Volume** is Android's media volume (`AudioManager`, STREAM_MUSIC), the one the device's own volume keys move too:
//!   the Activity says "index,max,muted" and takes an index. Android has few steps (15 is usual, some devices 25 or 7), so a
//!   5 % step of the row must still move at least one of them (`index_for`).
//! - **Brightness** is this app's window (`WindowManager.LayoutParams.screenBrightness`, no permission), kept for the next
//!   start and for the player, which the shelf starts (`HandheldActivity`). Android's own brightness stays as it is.
//!
//! Pure data and rounding here, so a host test covers it; the bridge calls are Android's only.

use crate::system_ctl::MIN_BRIGHTNESS;
pub use crate::volume_steps::{index_for, key_step, Levels};

/// A brightness the row shows: 5..=100 (`MIN_BRIGHTNESS`, the same floor as on the handhelds: a black screen can't be
/// undone with a pad).
pub fn brightness_percent(window: f32) -> u8 {
    ((window.clamp(0.0, 1.0) * 100.0).round() as u8).clamp(MIN_BRIGHTNESS, 100)
}

/// What the Activity says for the brightness: a percent, or nothing ("") where the window has no level of its own and
/// Android's can't be read. Rubbish is nothing known.
pub fn parse_brightness(line: &str) -> Option<u8> {
    let v = line.trim().parse::<i32>().ok().filter(|v| (0..=100).contains(v))?;
    Some((v as u8).clamp(MIN_BRIGHTNESS, 100))
}

// ---------------------------------------------------------------------------------------------------------------------
// The Activity (HandheldActivity's PocketBase): Android only.

#[cfg(target_os = "android")]
mod bridge {
    use super::*;

    fn ask(name: &str, args: &[&str]) -> Option<String> {
        crate::platform::android::strings(name, args).ok()
    }

    /// The media volume now; None where the Activity doesn't answer (the row is then not shown).
    pub fn levels() -> Option<Levels> {
        Levels::parse(&ask("volumeLevels", &[])?)
    }

    /// Sets the media volume to the step nearest `pct` (at least one step away from where it is when `pct` differs).
    pub fn set_volume(pct: u8) {
        let Some(now) = levels() else { return };
        let to = index_for(pct, now);
        if to != now.index {
            let _ = ask("setVolumeIndex", &[&to.to_string()]);
        }
    }

    pub fn brightness() -> Option<u8> {
        parse_brightness(&ask("brightnessState", &[])?)
    }

    /// This window's brightness now; the Activity keeps it for the next start and the player.
    pub fn set_brightness(pct: u8) {
        let _ = ask("setBrightness", &[&pct.clamp(MIN_BRIGHTNESS, 100).to_string()]);
    }
}

#[cfg(target_os = "android")]
pub use bridge::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_activitys_line_is_read_and_rubbish_is_nothing() {
        assert_eq!(Levels::parse("7,15,false"), Some(Levels { index: 7, max: 15, muted: false }));
        assert_eq!(Levels::parse("0,15,true"), Some(Levels { index: 0, max: 15, muted: true }));
        assert_eq!(Levels::parse("20,15,false").unwrap().index, 15, "never above the last step");
        assert_eq!(Levels::parse("3,0,false"), None, "no steps: no row");
        assert_eq!(Levels::parse(""), None);
        assert_eq!(Levels::parse("x,y"), None);
        assert_eq!(parse_brightness("40"), Some(40));
        assert_eq!(parse_brightness("0"), Some(MIN_BRIGHTNESS), "the floor of the handhelds");
        assert_eq!(parse_brightness(""), None);
        assert_eq!(parse_brightness("120"), None);
    }

    #[test]
    fn a_brightness_comes_out_between_the_floor_and_full() {
        assert_eq!(brightness_percent(0.0), MIN_BRIGHTNESS);
        assert_eq!(brightness_percent(0.4), 40);
        assert_eq!(brightness_percent(1.0), 100);
        assert_eq!(brightness_percent(7.0), 100);
        assert_eq!(brightness_percent(-1.0), MIN_BRIGHTNESS);
    }
}
