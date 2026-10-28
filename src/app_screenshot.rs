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
