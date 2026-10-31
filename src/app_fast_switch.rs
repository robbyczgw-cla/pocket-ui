//! Part of the pocket-ui binary: the App's side of the hand-over between two pocket-core games in the switcher
//! (`pocket_ui::fast_switch`), and of the same hand-over when the pause menu's Quit goes back to the shelf. Kept
//! out of main.rs so `launch`, `game_exited`, the pause menu and the switcher only carry hooks.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use pocket_ui::control;
use pocket_ui::fast_switch::{self, Game};
use pocket_ui::quit::{self, Escalation, How};

use super::{App, host_bin};

/// A game that handed over and whose process is still ending.
struct Draining {
    pgid: i32,
    /// Its core (`fast_switch::Game::core`): what may start while it ends (`wait_for_drained`).
    core: String,
    system: String,
    path: PathBuf,
    /// The Quit's deadline carries on: SIGTERM, then SIGKILL, if the teardown hangs.
    quit: Escalation,
    saved_at: Instant,
}

/// A hand-over asked for.
struct Asked {
    pgid: i32,
    core: String,
    /// For the switcher's next game (`quit_for_handoff`), not for the shelf (`quit_to_shelf`).
    next_game: bool,
}

#[derive(Default)]
pub(crate) struct FastSwitch {
    /// The game asked for a hand-over, until its line comes or it ends.
    handoff: Option<Asked>,
    /// True while `game_exited` runs for a game that handed over: it is still running, but saved.
    pub(crate) handed_over: bool,
    /// The same, and the next game is the pocket-core game the switcher asked for (`quit_for_handoff`).
    to_next_game: bool,
    draining: Vec<Draining>,
    /// The game was asked to quit by its own command while the pause menu was up (`quit_to_shelf`): pocket-ui is
    /// in front and focused already, so its end needs no `swaymsg` (`ui_in_front`).
    in_front: bool,
}

impl App {
    /// The switcher's Quit for `target` as a hand-over, when the running game and `target` allow it
    /// (`fast_switch::overlap_allowed`). False: nothing was sent, quit the usual way.
    pub(crate) fn quit_for_handoff(&mut self, target: &(String, String, PathBuf)) -> bool {
        let Some(run) = self.running.as_ref().filter(|_| fast_switch::enabled()) else { return false };
        let (Some(pgid), true) = (run.pgid, run.hosted()) else { return false };
        let host = std::path::Path::new(host_bin()).exists();
        let route = |system: &str, path: &PathBuf| self.systems.iter().find(|s| s.name == system).map(|s| self.route(s, Some(path), host));
        let (Some((_, old_core)), Some((new_emu, new_core))) = (route(&run.system, &run.path), route(&target.0, &target.2)) else { return false };
        let old = Game { emulator: &run.emulator, core: &old_core, system: &run.system, rom: &run.path };
        let new = Game { emulator: &new_emu, core: &new_core, system: &target.0, rom: &target.2 };
        if !fast_switch::overlap_allowed(old, new) {
            return false;
        }
        let Some(menu) = self.menu.as_mut().filter(|m| !m.quitting) else { return false };
        if !control::pocket_core(fast_switch::QUIT_FOR_SWITCH) {
            return false;
        }
        menu.page = super::Page::Main;
        menu.quitting = true;
        self.quit = Some(Escalation::new(Instant::now(), quit::grace(How::Window)));
        self.fast.handoff = Some(Asked { pgid, core: old_core, next_game: true });
        true
    }

    /// The pause menu's Quit for a pocket-core game. A core that may hand over (`fast_switch::may_hand_over`) gets
    /// `quit handoff`: the shelf comes back as soon as its exit save is on the card and the CPU and fan are back
    /// (`game_saved`), while its core, GL, window and sound stream tear down next to it, which on the device takes
    /// longer than the save. Any other core gets `quit` and is waited for, as before. False: pocket-core didn't
    /// take the command (the caller closes its window instead).
    pub(crate) fn quit_to_shelf(&mut self) -> bool {
        let host = std::path::Path::new(host_bin()).exists();
        let hand_over = self.running.as_ref().filter(|r| fast_switch::enabled() && r.hosted()).and_then(|r| {
            let pgid = r.pgid?;
            let (_, core) = self.systems.iter().find(|s| s.name == r.system).map(|s| self.route(s, Some(&r.path), host))?;
            fast_switch::may_hand_over(&core, &r.system).then_some(Asked { pgid, core, next_game: false })
        });
        if !control::pocket_core(if hand_over.is_some() { fast_switch::QUIT_FOR_SWITCH } else { "quit" }) {
            return false;
        }
        self.fast.in_front = true;
        self.fast.handoff = hand_over;
        true
    }

    /// When a game has ended: whether pocket-ui is in front already, so `game_exited` needn't put it there with a
    /// `swaymsg` (after a hand-over, or a Quit by pocket-core's own command; not after RetroArch or a closed window,
    /// which took the focus to quit). Asked once per game end.
    pub(crate) fn ui_in_front(&mut self) -> bool {
        std::mem::take(&mut self.fast.in_front) || self.fast.handed_over
    }

    /// When a game has ended: PipeWire's quantum stays as it is for the next pocket-core game, which starts now
    /// (a hand-over in the switcher). Everywhere else pocket-ui puts it back: the old process leaves it to us.
    pub(crate) fn quantum_stays(&self) -> bool {
        self.fast.handed_over && self.fast.to_next_game
    }

    /// pocket-core `pgid` announced its exit save: if it is the game asked to hand over, it ends here for
    /// pocket-ui (the switcher starts the next game) while its process finishes on its own.
    pub(crate) fn game_saved(&mut self, pgid: i32) {
        let running = self.running.as_ref().and_then(|r| r.pgid);
        if self.fast.handoff.as_ref().map(|h| h.pgid) != Some(pgid) || running != Some(pgid) {
            return;
        }
        fast_switch::mark("hand-over received");
        let Some(Asked { core, next_game, .. }) = self.fast.handoff.take() else { return };
        let now = Instant::now();
        let quit = self.quit.take().unwrap_or_else(|| Escalation::new(now, quit::grace(How::Window)));
        let Some(run) = self.running.as_ref() else { return };
        let secs = run.started.elapsed().as_secs();
        self.fast.draining.push(Draining { pgid, core, system: run.system.clone(), path: run.path.clone(), quit, saved_at: now });
        (self.fast.handed_over, self.fast.to_next_game) = (true, next_game);
        self.game_exited(secs, true);
        (self.fast.handed_over, self.fast.to_next_game) = (false, false);
    }

    /// A game's process ended: true when it was one that had handed over already (nothing more to do for it).
    pub(crate) fn drained(&mut self, pgid: Option<i32>, ok: bool) -> bool {
        let Some(at) = pgid.and_then(|g| self.fast.draining.iter().position(|d| d.pgid == g)) else {
            // A game that ended before its line came (crashed or killed mid-save): the usual end.
            if pgid.is_some() && self.fast.handoff.as_ref().map(|h| h.pgid) == pgid {
                self.fast.handoff = None;
            }
            return false;
        };
        let d = self.fast.draining.remove(at);
        fast_switch::mark("hand-over: old game's process ended");
        println!(
            "hand-over: {} ended {:.0} ms after its save{}",
            d.path.display(),
            d.saved_at.elapsed().as_secs_f32() * 1000.0,
            if ok { "" } else { " (with an error; its save was complete)" }
        );
        true
    }

    /// Stops a game that hangs while ending after its hand-over, like any game that ignores Quit. Returns when
    /// to look again.
    pub(crate) fn drain_tick(&mut self, now: Instant) -> Option<Instant> {
        for d in &mut self.fast.draining {
            if let Some(sig) = d.quit.due(now) {
                println!("switcher: {} still ending, {sig:?}", d.path.display());
                control::terminate_game(d.pgid, sig == quit::Signal::Kill);
            }
        }
        self.fast.draining.iter().filter_map(|d| d.quit.next()).min()
    }

    /// Before a start: a game that handed over has to have ended first unless the new one may start next to it
    /// (`fast_switch::overlap_allowed`: another pocket-core game that shares no save with it), so nothing it still
    /// does can meet the new start (the same game again after a Quit to the shelf, RetroArch, a standalone
    /// emulator). Practically never waits: the teardown takes a fraction of a second, picking a game longer.
    pub(crate) fn wait_for_drained(&self, system: &str, path: &std::path::Path, emulator: &str, core: &str) {
        let next = Game { emulator, core, system, rom: path };
        for d in self.fast.draining.iter().filter(|d| !fast_switch::overlap_allowed(Game { emulator: "pocket-core", core: &d.core, system: &d.system, rom: &d.path }, next)) {
            fast_switch::mark("start waits for the old game's process");
            if !fast_switch::wait_ended(d.pgid, quit::WINDOW_GRACE) {
                control::terminate_game(d.pgid, true);
                fast_switch::wait_ended(d.pgid, Duration::from_secs(1));
            }
        }
    }
}
