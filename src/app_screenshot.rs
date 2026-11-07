//! Part of the pocket-ui binary: HOME + A in a game that pocket-core does not host (`pocket_ui::screenshot`).
//!
//! pocket-core takes its own screenshots from the core's frame (`pocket_core.rs`, `App::screenshot`) and RetroArch
//! has shortcuts of its own, so this is for the standalone emulators (PlayStation 2, GameCube, Wii, 3DS) and
//! PortMaster ports: the game is on screen and pocket-ui is out of sight behind it, so `grim` reads the whole screen.
//! The read, the encode and the write all run on the screenshot thread; the UI thread only hands the request over.
//!
//! There is nothing to draw on: pocket-ui is not in front of these games, so no toast. The answer is a quiet shutter
//! click when Settings > Sounds is on, and the error tick when it did not work (the reason goes to the log).

use std::sync::OnceLock;

use pocket_ui::screenshot::{self, Outcome, Request, Saver, Source, Stamp};
use pocket_ui::ui_sound::{self, Sound};

use super::App;

/// The thread, started at the first screenshot (None: it could not be started).
static SAVER: OnceLock<Option<Saver>> = OnceLock::new();

/// Whether HOME + A is pocket-ui's in a running game: not pocket-core's (its own frame, its own toast), not
/// RetroArch's (its shortcuts), not an app (PC streaming, a tool).
pub(crate) fn applies(emulator: &str) -> bool {
    !matches!(emulator, "pocket-core" | "retroarch") && emulator != pocket_ui::moonlight::EMULATOR && emulator != pocket_ui::tools::EMULATOR
}

impl App {
    /// HOME + A went down in a game (the pause menu is not open).
    pub(crate) fn screenshot_key(&mut self) {
        #[cfg(feature = "brick")]
        if self.running.is_none() {
            self.ui_screenshot();
            return;
        }
        let Some(run) = self.running.as_ref().filter(|r| applies(&r.emulator)) else { return };
        let saver = SAVER.get_or_init(|| {
            let data = self.data_dir.clone();
            Saver::start(screenshot::dir(), move |outcome| {
                let sounds = ui_sound::Config::load(&data);
                match outcome {
                    Outcome::Saved(_) => {
                        ui_sound::play_over_game(Sound::Shutter, sounds);
                    }
                    Outcome::Failed(why) => {
                        eprintln!("{}", why.toast());
                        ui_sound::play_over_game(Sound::Error, sounds);
                    }
                }
            })
            .map_err(|e| eprintln!("screenshot: no thread: {e}"))
            .ok()
        });
        let game = run.path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| run.name.clone());
        let req = Request { system: run.system.clone(), game, stamp: Stamp::now(), source: Source::Screen };
        if !saver.as_ref().is_some_and(|s| s.submit(req)) {
            println!("screenshot: busy or no thread, press ignored");
        }
    }
}

/// What the UI's screenshots are filed under (`<screenshots>/pocket-ui/Pocket UI <time>.png`, the gallery lists them like a game's).
#[cfg(feature = "brick")]
const UI_SYSTEM: &str = "pocket-ui";
#[cfg(feature = "brick")]
const UI_NAME: &str = "Pocket UI";

#[cfg(feature = "brick")]
static UI_SAVER: OnceLock<Option<Saver>> = OnceLock::new();

#[cfg(feature = "brick")]
impl App {
    /// HOME + A in pocket-ui itself (the Brick's own card: nothing else takes the screen): the screen as `grim` hands it over (the card's
    /// stand-in asks pocket-fb for the window on show), a toast and the shutter when it is in place, or why not. Kids Mode has no key for it.
    fn ui_screenshot(&mut self) {
        if self.kids.cfg.on || !pocket_ui::device::brick() {
            return;
        }
        let saver = UI_SAVER.get_or_init(|| {
            let (data, proxy) = (self.data_dir.clone(), self.proxy.clone());
            Saver::start(screenshot::dir(), move |outcome| {
                let sounds = ui_sound::Config::load(&data);
                let failed = match outcome {
                    Outcome::Saved(_) => {
                        ui_sound::play_over_game(Sound::Shutter, sounds);
                        None
                    }
                    Outcome::Failed(why) => {
                        eprintln!("{}", why.toast());
                        Some(why.toast())
                    }
                };
                let _ = proxy.send_event(super::UserEvent::UiScreenshot(failed));
            })
            .map_err(|e| eprintln!("screenshot: no thread: {e}"))
            .ok()
        });
        let req = Request { system: UI_SYSTEM.into(), game: UI_NAME.into(), stamp: Stamp::now(), source: Source::Screen };
        if !saver.as_ref().is_some_and(|s| s.submit(req)) {
            println!("screenshot: busy or no thread, press ignored");
        }
    }

    /// The screenshot thread finished one: the toast.
    pub(crate) fn ui_screenshot_done(&mut self, failed: Option<&'static str>) {
        self.toast = Some((failed.unwrap_or("Screenshot saved").to_string(), std::time::Instant::now()));
        self.gallery_refresh("ui screenshot");
        self.request_redraw();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_games_pocket_core_and_retroarch_do_not_run_are_ours() {
        assert!(applies("armsx2") && applies("dolphin") && applies("azahar") && applies("portmaster"));
        assert!(!applies("pocket-core"), "its own frame and toast");
        assert!(!applies("retroarch"), "its own shortcuts");
        assert!(!applies(pocket_ui::moonlight::EMULATOR) && !applies(pocket_ui::tools::EMULATOR), "apps have no screenshot");
    }
}
