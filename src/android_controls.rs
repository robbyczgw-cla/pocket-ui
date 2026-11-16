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

/// What the Activity said for the media volume: "index,max,muted", for example "7,15,false".
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Levels {
    pub index: u32,
    pub max: u32,
    pub muted: bool,
}

impl Levels {
    /// Anything unreadable, or a device with no volume steps (`max` 0), is nothing known: the row is not shown then.
    pub fn parse(line: &str) -> Option<Levels> {
        let mut parts = line.trim().split(',').map(str::trim);
        let index = parts.next()?.parse::<u32>().ok()?;
        let max = parts.next()?.parse::<u32>().ok().filter(|m| *m > 0)?;
        let muted = parts.next().is_some_and(|m| m == "true");
        Some(Levels { index: index.min(max), max, muted })
    }

    /// The row's figure: the step's share of the whole, rounded to the nearest percent (7 of 15 is 47).
    pub fn percent(&self) -> u8 {
        ((self.index * 100 + self.max / 2) / self.max).min(100) as u8
    }
}

/// The step to set for the wish `pct` when the volume stands at `now`: the nearest step; on a device with fewer than twelve
/// steps never the same one while the wish differs from where it stands (7 steps: 5 % is a third of a step; the key still
/// has to move). Louder wishes go up,
/// quieter ones down, by at least one step; 0 % is step 0 and 100 % the last.
pub fn index_for(pct: u8, now: Levels) -> u32 {
    let pct = u32::from(pct.min(100));
    let target = ((pct * now.max + 50) / 100).min(now.max);
    // From twelve steps up a 5 % press rounds to the next step by itself, and the row's own count (it adds 5 % to what it showed
    // a moment ago) stays within a step of Android's: no extra step, so no snapping when the row reads Android again.
    if now.max >= 12 {
        return target;
    }
    let shown = u32::from(now.percent());
    match pct.cmp(&shown) {
        std::cmp::Ordering::Greater if target <= now.index => (now.index + 1).min(now.max),
        std::cmp::Ordering::Less if target >= now.index => now.index.saturating_sub(1),
        _ => target,
    }
}

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

    fn at(index: u32, max: u32) -> Levels {
        Levels { index, max, muted: false }
    }

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
    fn the_row_shows_the_nearest_percent() {
        assert_eq!(at(0, 15).percent(), 0);
        assert_eq!(at(7, 15).percent(), 47);
        assert_eq!(at(15, 15).percent(), 100);
        assert_eq!(at(1, 7).percent(), 14);
        assert_eq!(at(50, 100).percent(), 50);
    }

    #[test]
    fn every_press_of_the_row_moves_the_volume_one_step_at_least_and_never_skips() {
        // Whatever Android's number of steps: from the bottom to the top every press moves it, it arrives within as many
        // presses as there are steps, and no press moves it by more than 5 % and a step and a half (no skipped step).
        for max in [3u32, 7, 10, 11, 12, 14, 15, 16, 25, 30, 100] {
            let mut now = at(0, max);
            let mut presses = 0;
            while now.index < max {
                let wish = (now.percent() + 5).min(100);
                let to = index_for(wish, now);
                assert!(to > now.index, "max {max}: press up from {} did not move (wish {wish})", now.index);
                let by = to - now.index;
                assert!(f64::from(by) <= f64::from(max) * 0.05 + 1.5, "max {max}: jumped {by} steps from {}", now.index);
                now = at(to, max);
                presses += 1;
                assert!(presses <= max + 1, "max {max}: never arrives");
            }
            while now.index > 0 {
                let wish = now.percent().saturating_sub(5);
                let to = index_for(wish, now);
                assert!(to < now.index, "max {max}: press down from {} did not move (wish {wish})", now.index);
                now = at(to, max);
            }
        }
    }

    #[test]
    fn the_ends_are_the_ends_and_a_wish_that_is_where_it_stands_changes_nothing() {
        assert_eq!(index_for(100, at(14, 15)), 15);
        assert_eq!(index_for(0, at(1, 15)), 0);
        assert_eq!(index_for(100, at(15, 15)), 15, "already at the top");
        assert_eq!(index_for(0, at(0, 15)), 0, "already silent");
        assert_eq!(index_for(47, at(7, 15)), 7, "the wish is what it shows");
        assert_eq!(index_for(60, at(0, 15)), 9, "a big wish (a jump) goes to its own step");
        assert_eq!(index_for(30, at(15, 15)), 5);
    }

    #[test]
    fn a_brightness_comes_out_between_the_floor_and_full() {
        assert_eq!(brightness_percent(0.0), MIN_BRIGHTNESS);
        assert_eq!(brightness_percent(0.4), 40);
        assert_eq!(brightness_percent(1.0), 100);
        assert_eq!(brightness_percent(7.0), 100);
        assert_eq!(brightness_percent(-1.0), MIN_BRIGHTNESS);
    }

    #[test]
    fn the_rows_own_count_stays_within_a_step_of_androids() {
        // The row adds 5 % to what it showed a moment ago while Android moves its steps: three quick presses from 7 % on a
        // 15-step device must not leave Android a whole step ahead of the row (it used to: 22 % shown, 27 % real).
        let mut shown = 7u32;
        let mut real = at(1, 15);
        for _ in 0..3 {
            shown += 5;
            real = at(index_for(shown as u8, real), 15);
        }
        assert!((i64::from(real.percent()) - i64::from(shown)).abs() <= 4, "row {shown} %, Android {} %", real.percent());
    }
}
