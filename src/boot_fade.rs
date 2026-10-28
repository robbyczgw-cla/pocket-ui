//! The first frame after the Pocket starts fades in from black (docs/BOOT-SPLASH.md, "Hand-over"): the boot screen
//! (pocket-splash) fades to black, sway comes up on black, and the shelf comes in over 300 ms instead of at once, in
//! the dark and in the light look alike. Only at pocket-ui's first start since the device started: a mark in `/run`
//! (a tmpfs, empty at every start of the device) tells a later start (Restart pocket-ui, the launcher after a crash,
//! an update) from it, and waking from sleep starts nothing. Motion Off shows the first frame as it is.

use std::path::Path;
use std::time::{Duration, Instant};

/// pocket-ui's mark that it has started since the device started.
pub const MARK: &str = "/run/pocket-ui-started";
/// The fade from black.
pub const FADE: Duration = Duration::from_millis(300);

/// Whether this is the first start since the device started; leaves the mark, so that the next one is not. A mark that
/// can't be written (no /run, not root: a desktop) means no fade.
pub fn first_since_boot(mark: &Path) -> bool {
    std::fs::OpenOptions::new().write(true).create_new(true).open(mark).is_ok()
}

/// The fade over the first frames.
#[derive(Debug, Default)]
pub struct BootFade {
    wanted: bool,
    /// When the first frame was painted.
    from: Option<Instant>,
}

impl BootFade {
    pub fn new(wanted: bool) -> Self {
        Self { wanted, from: None }
    }

    /// How far the frame painted at `now` is darkened: 255 black, 0 as painted; None once the fade is over (and
    /// without one). The first call is the first frame: the fade starts there, not at the program's start.
    pub fn darkness(&mut self, now: Instant) -> Option<u8> {
        if !self.wanted {
            return None;
        }
        let from = *self.from.get_or_insert(now);
        let p = now.saturating_duration_since(from).as_secs_f32() / FADE.as_secs_f32();
        if p >= 1.0 {
            self.wanted = false;
            return None;
        }
        // Eased out: it comes up quickly and settles softly.
        let shown = 1.0 - (1.0 - p) * (1.0 - p);
        Some(((1.0 - shown) * 255.0).round() as u8)
    }

    /// More frames are needed for the fade.
    pub fn active(&self) -> bool {
        self.wanted
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_first_start_since_the_boot_fades() {
        let dir = crate::testutil::TestDir::new("boot-fade");
        let mark = dir.join("pocket-ui-started");
        assert!(first_since_boot(&mark), "a fresh /run: the first start");
        assert!(!first_since_boot(&mark), "Restart pocket-ui, a crash, an update: no fade");
        assert!(!first_since_boot(&dir.join("missing/mark")), "a mark that can't be written: no fade");
    }

    #[test]
    fn the_first_frame_is_black_and_the_fade_ends_after_300_ms() {
        let t0 = Instant::now();
        let mut f = BootFade::new(true);
        assert!(f.active());
        assert_eq!(f.darkness(t0), Some(255), "the first frame: black, where sway's black was");
        let half = f.darkness(t0 + Duration::from_millis(150)).unwrap();
        assert!(half > 30 && half < 100, "eased out: more than half there at half time ({half})");
        assert!(f.darkness(t0 + Duration::from_millis(290)).unwrap() < 5);
        assert_eq!(f.darkness(t0 + FADE), None);
        assert!(!f.active());
        assert_eq!(f.darkness(t0 + Duration::from_millis(10)), None, "over is over");
        let mut none = BootFade::new(false);
        assert_eq!(none.darkness(t0), None);
        assert!(!none.active());
    }
}
