//! The pause menu's Pixel Path page: which stage is selected, whether its explanation is open, the newest report.
//! The switch that shows it at all is Settings → Curator → "Pixel Path in the pause menu" (`Prefs::pixel_path`).
//! Y puts Pixel Path's line over the game (`pixel_live`) or takes it off: Over the game's Pixel Path level (`over_game`),
//! as if changed in Options.
//! Painting is in `app_pixel_path.rs`; the stages come from `pixel_path` (pocket-core's report) or from
//! `pixel_standalone` (a standalone emulator's settings).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::pad::Button;
use crate::pixel_path::{self, Kind, Report};
use crate::pixel_standalone::{Screen, Setup};

pub enum Action {
    Nothing,
    /// Back to the pause menu.
    Close,
}

pub struct PixelPathPage {
    pub sel: usize,
    /// The selected stage's explanation is open (A).
    pub about: bool,
    /// The newest report for this game; None until pocket-core answered (or for a game it doesn't run).
    pub report: Option<Report>,
    /// A standalone emulator's game: what its settings say, the system's name and the panel.
    pub standalone: Option<(Setup, String, Screen)>,
    /// The reader thread runs while this is true; dropping the page stops it.
    pub alive: Arc<AtomicBool>,
    /// What pocket-ui adds, fixed instead of looked up (snapshots).
    pub known: Option<pixel_path::Known>,
    /// Over the game as it was before Y put it on Pixel Path, to go back to (Off, FPS, FPS + sensors).
    pub before_strip: Option<String>,
}

impl Drop for PixelPathPage {
    fn drop(&mut self) {
        self.alive.store(false, Ordering::Relaxed);
    }
}

impl PixelPathPage {
    pub fn new() -> PixelPathPage {
        PixelPathPage {
            sel: 0,
            about: false,
            report: None,
            standalone: None,
            alive: Arc::new(AtomicBool::new(true)),
            known: None,
            before_strip: None,
        }
    }

    /// The page of a standalone emulator's game: nothing to ask pocket-core, the stages come from `setup`.
    pub fn standalone(setup: Setup, system_name: &str, screen: Screen) -> PixelPathPage {
        let mut p = PixelPathPage::new();
        p.alive.store(false, Ordering::Relaxed);
        p.standalone = Some((setup, system_name.to_string(), screen));
        p
    }

    /// A newer reading of the emulator's settings: true when it changes what the page shows.
    pub fn receive_setup(&mut self, setup: Setup) -> bool {
        match self.standalone.as_mut() {
            Some((old, _, _)) if *old != setup => {
                *old = setup;
                true
            }
            _ => false,
        }
    }

    pub fn press(&mut self, b: Button, confirm: Button, cancel: Button) -> Action {
        let n = Kind::ALL.len();
        if self.about {
            // The explanation is a sheet over the page: A or B closes it, Up/Down go on to the next stage's.
            match b {
                Button::Up => self.sel = (self.sel + n - 1) % n,
                Button::Down => self.sel = (self.sel + 1) % n,
                b if b == confirm || b == cancel => self.about = false,
                _ => {}
            }
            return Action::Nothing;
        }
        match b {
            Button::Up => self.sel = (self.sel + n - 1) % n,
            Button::Down => self.sel = (self.sel + 1) % n,
            b if b == confirm => self.about = true,
            b if b == cancel => return Action::Close,
            _ => {}
        }
        Action::Nothing
    }

    /// A new report: true when it changes what the page shows.
    pub fn receive(&mut self, r: Report) -> bool {
        if self.report.as_ref().is_some_and(|old| old.same_as(&r)) {
            return false;
        }
        self.report = Some(r);
        true
    }

    /// The stages as they are to be shown, None while the first report is on its way.
    pub fn path(&self, known: &pixel_path::Known) -> Option<pixel_path::Path> {
        if let Some((setup, system, screen)) = &self.standalone {
            return Some(crate::pixel_standalone::path(setup, system, *screen));
        }
        self.report.as_ref().map(|r| pixel_path::path(r, known))
    }

    /// Y: the Over the game value that puts Pixel Path over the game, or takes it off again, from `current`. Off again
    /// goes back to what it was before (the FPS line, or nothing).
    pub fn toggle_over_game(&mut self, current: &str) -> String {
        if current == "pixel" {
            self.before_strip.take().unwrap_or_else(|| "off".into())
        } else {
            self.before_strip = Some(current.to_string());
            "pixel".into()
        }
    }

    /// A fingerprint for the menu sounds: a move or the sheet opening is a change.
    pub fn mark(&self) -> (usize, bool) {
        (self.sel, self.about)
    }
}

impl Default for PixelPathPage {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: Button = Button::East;
    const B: Button = Button::South;

    #[test]
    fn up_down_walk_the_chain_a_explains_b_goes_back() {
        let mut p = PixelPathPage::new();
        p.press(Button::Up, A, B);
        assert_eq!(p.sel, 6, "wraps to Audio");
        p.press(Button::Down, A, B);
        p.press(Button::Down, A, B);
        assert_eq!(p.sel, 1);
        p.press(A, A, B);
        assert!(p.about);
        // Up/Down with the sheet open read the next stage's explanation.
        p.press(Button::Down, A, B);
        assert!(p.about && p.sel == 2);
        assert!(matches!(p.press(B, A, B), Action::Nothing), "B closes the sheet first");
        assert!(!p.about);
        assert!(matches!(p.press(B, A, B), Action::Close));
    }

    #[test]
    fn y_puts_pixel_path_over_the_game_and_back_to_what_was_there() {
        let mut p = PixelPathPage::new();
        assert_eq!(p.toggle_over_game("sensors"), "pixel");
        assert_eq!(p.toggle_over_game("pixel"), "sensors", "the FPS line comes back");
        assert_eq!(p.toggle_over_game("off"), "pixel");
        assert_eq!(p.toggle_over_game("pixel"), "off");
        // Already on when the page opened (saved for the console): off is off.
        assert_eq!(PixelPathPage::new().toggle_over_game("pixel"), "off");
    }

    #[test]
    fn dropping_the_page_stops_its_reader() {
        let p = PixelPathPage::new();
        let alive = p.alive.clone();
        assert!(alive.load(Ordering::Relaxed));
        drop(p);
        assert!(!alive.load(Ordering::Relaxed));
    }

    #[test]
    fn only_a_changed_report_repaints() {
        let mut p = PixelPathPage::new();
        let (mut r, known) = pixel_path::demo("gba").unwrap();
        assert!(p.path(&known).is_none(), "nothing until pocket-core answers");
        assert!(p.receive(r.clone()));
        r.seq += 1;
        assert!(!p.receive(r.clone()), "the same numbers under a new request number");
        r.live.cpu += 1.0;
        assert!(p.receive(r));
        assert_eq!(p.path(&known).unwrap().stages.len(), 7);
    }

    #[test]
    fn a_standalone_game_shows_its_settings_at_once_and_takes_a_newer_reading() {
        let (setup, console) = crate::pixel_standalone::demo("dolphin").unwrap();
        let mut p = PixelPathPage::standalone(setup.clone(), console, Screen::default());
        assert!(!p.alive.load(Ordering::Relaxed), "no reader thread: nothing to ask pocket-core");
        let path = p.path(&pixel_path::Known::default()).unwrap();
        assert!(path.partial && path.stages.len() == 7);
        assert!(!p.receive_setup(setup.clone()), "the same reading changes nothing");
        let mut newer = setup;
        newer.vsync = Some(true);
        assert!(p.receive_setup(newer));
        assert!(p.path(&pixel_path::Known::default()).unwrap().stages[4].value.contains("VSync on"));
    }
}
