//! Part of the pocket-ui binary: the App's side of `pocket_ui::quick_profiles`, Quick settings' Performance and Fan
//! rows.
//! - Opening the panel over a paused game hands it what the rows need, all from memory: the pause menu's own
//!   settings (`GameSettings` of a pocket-core game, the standalone Options page), a port's choice (`port_perf`'s
//!   store). Nothing is read from a file or sysfs on the way.
//! - A change (`Sys::take_profile_change`, looked at in `perf_fan_sync`) becomes Settings → Playing → Performance & fan's
//!   choices at once in memory; the file is written and the running game told on a worker thread: pocket-core through
//!   its command FIFO (`set performance …`, `set fan …`, as the pause menu's Options do), a PS2, GameCube, Wii or 3DS
//!   game through `standalone::set_performance`. Smooth fan's plan and the fan outside games follow the choices in
//!   memory (`perf_fan_sync`).

use std::sync::OnceLock;
use std::time::Instant;

use pocket_ui::control;
use pocket_ui::options::Level;
use pocket_ui::perf_fan::Prefs;
use pocket_ui::power::{Fan, Performance};
use pocket_ui::quick_profiles::{self, Effect, GameView, Kind, Whose};
use pocket_ui::system_ctl::Worker;

use super::App;

/// Saving the choices and telling the game, one job after the other, off the UI thread.
fn worker() -> &'static Worker {
    static W: OnceLock<Worker> = OnceLock::new();
    W.get_or_init(Worker::new)
}

impl App {
    /// The pause menu's "Quick settings": the panel over the paused game.
    pub(crate) fn open_quick_settings_over_game(&mut self) {
        let view = self.quick_game_view();
        self.sys.open_quick_over_game(Instant::now(), view);
    }

    /// The running game as the Performance and Fan rows see it, from what the pause menu holds.
    fn quick_game_view(&self) -> GameView {
        let run = self.running.as_ref();
        let menu = self.menu.as_ref();
        let kind = match run {
            Some(r) if r.hosted() => Kind::PocketCore,
            Some(r) if r.system == pocket_ui::port_perf::SYSTEM => Kind::Port,
            Some(_) if pocket_ui::standalone::active() || pocket_ui::standalone::guest_active() => Kind::Standalone,
            Some(r) if r.emulator == "retroarch" => Kind::RetroArch,
            _ => Kind::Other,
        };
        let mut view = GameView {
            kind,
            own_performance: None,
            own_fan: None,
        };
        match kind {
            Kind::PocketCore => {
                if let Some(s) = menu.and_then(|m| m.settings.as_ref()) {
                    let whose = if s.level == Level::Game { Whose::Game } else { Whose::Console };
                    view.own_performance = s.own("performance").and_then(Performance::from_value).map(|p| (p, whose));
                    view.own_fan = s.own("fan").map(|f| (Fan::parse(f), whose));
                }
            }
            Kind::Standalone => {
                view.own_performance = menu
                    .and_then(|m| m.emu_set.as_ref())
                    .and_then(|e| e.own_performance())
                    .map(|p| (quick_profiles::from_standalone(p), Whose::Game));
            }
            Kind::Port => {
                if let Some(r) = run {
                    // From memory, never waited for. In the moments before the ports' settings are read (a port
                    // continued at the start) the port was started with the defaults (`port_perf::Wanted::Later`),
                    // which is what the rows then say; its performance is its own once its thread has read them.
                    if let Some((c, _)) = self.port_state(&r.path) {
                        view.own_performance = c.own_performance.then_some((c.performance, Whose::Port));
                    }
                    // The fan the port runs with now: its own, or the default that follows these rows.
                    view.own_fan = self.port_perf.fan.and_then(|(f, own)| own.then_some((f, Whose::Port)));
                }
            }
            Kind::RetroArch | Kind::Other => {}
        }
        view
    }

    /// From `perf_fan_sync` (every turn of the event loop, a comparison when nothing changed): takes a change of the
    /// rows, and keeps the rows showing the choices as they are (Settings → Playing → Performance & fan changes them too).
    pub(crate) fn quick_profiles_sync(&mut self) {
        if let Some(new) = self.sys.take_profile_change() {
            self.quick_profile_changed(new);
        }
        self.sys.set_profile_prefs(self.perf_prefs());
    }

    fn quick_profile_changed(&mut self, new: Prefs) {
        let old = self.perf_prefs();
        if old == new {
            return;
        }
        self.perf_set_prefs(new);
        // The pause menu's pages, when the panel goes back to them, show the new defaults under their own choices.
        if let Some(m) = self.menu.as_mut() {
            if let Some(s) = m.settings.as_mut() {
                s.set_global("performance", new.games_performance.value());
                s.set_global("fan", new.games_fan.value());
            }
            if let Some(e) = m.emu_set.as_mut() {
                e.default_performance_changed();
            }
        }
        let game = self.sys.profile_game();
        let effects = quick_profiles::effects(&old, &new, game.as_ref());
        println!(
            "quick settings: games performance {}, games fan {}, fan outside games {}{}",
            new.games_performance.value(),
            new.games_fan.value(),
            new.outside_fan.value(),
            if effects.is_empty() { String::new() } else { format!(" ({effects:?})") }
        );
        let dir = self.data_dir.clone();
        // The newest choices replace a save still waiting (the fsync on the card is the slow part).
        worker().submit("quick-profiles-save", move || {
            if !new.save(&dir) {
                eprintln!("quick settings: performance & fan not saved");
            }
        });
        for e in effects {
            worker().submit("", move || match e {
                Effect::PocketCore(line) => {
                    if !control::pocket_core(&line) {
                        eprintln!("quick settings: pocket-core didn't take \"{line}\"");
                    }
                }
                Effect::Standalone(level) => {
                    pocket_ui::standalone::set_performance(level);
                }
            });
        }
    }

    /// `--snapshot game-quick[:<row>][:own]`: the game under the panel, a pocket-core game; `own`: it has its own
    /// Performance (Max) in Options.
    pub(crate) fn quick_profiles_snapshot(&mut self, state: &str) {
        let own = state.split(':').any(|p| p == "own");
        self.sys.set_profile_game(Some(GameView {
            kind: Kind::PocketCore,
            own_performance: own.then_some((Performance::Performance, Whose::Game)),
            own_fan: None,
        }));
    }
}
