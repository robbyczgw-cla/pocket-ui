//! Ending a game after Quit in the pause menu: how long each kind of emulator gets to close by itself, and
//! the escalation to SIGTERM and SIGKILL when it doesn't.

use std::time::{Duration, Instant};

/// From the SIGTERM at the deadline to the SIGKILL.
pub const KILL_AFTER: Duration = Duration::from_secs(2);
/// A standalone emulator closed through its window gets this long.
pub const WINDOW_GRACE: Duration = Duration::from_secs(5);

/// How the game was asked to quit.
#[derive(Clone, Copy, Debug)]
pub enum How {
    /// SIGTERM to RetroArch, which writes its auto state and exits.
    RetroArch,
    /// ARMSX2 and Dolphin: resume state first, then their own shutdown (`standalone::quit`).
    Standalone,
    /// Cemu and RPCS3 (`standalone::guest_quit`): the window's close request, then SIGTERM and SIGKILL.
    Guest,
    /// The window's close request (pocket-core and the other emulators).
    Window,
}

/// The emulator a game runs in, as far as a quit without a new resume state goes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Saver {
    /// pocket-ui's own host; `states`: the system has save states (not ScummVM, PICO-8, TIC-80: `hostsys`).
    PocketCore { states: bool },
    RetroArch,
    /// ARMSX2, Dolphin, Azahar (`standalone`).
    Standalone,
    /// Any other emulator: it keeps its own saves, Quit only closes it.
    Other,
}

/// Whether the pause menu's Quit can leave out the resume state (Settings > Remember where I quit off; `main.rs`
/// `quit_leaves_out_resume`): where Quit writes a resume state and a quit can leave it out while the in-game saves
/// are still written. pocket-core skips its auto state (`quit nosave`),
/// the standalone emulators skip the save step and shut down as always. Not RetroArch: with ROCKNIX's
/// `global.autosave=1` (`savestate_auto_save`, written into its config at every launch by `setsettings.sh`) it
/// writes `.state.auto` on every exit, a setting it reads only at start, and none of its network commands
/// skips it. Restoring the old file afterwards is no way out either: RetroArch loads it at the next start
/// without looking at the battery save, so an in-game save made in between would be rolled back. Nor the
/// other emulators, whose Quit saves nothing to begin with.
pub fn offers_quit_without_saving(saver: Saver) -> bool {
    matches!(saver, Saver::PocketCore { states: true } | Saver::Standalone)
}

/// RetroArch exits within ~250 ms of SIGTERM; writing the auto state can take a second or two more.
pub const RETROARCH_GRACE: Duration = Duration::from_secs(10);
/// After the SIGKILL: how long a game that is still there keeps the system awake.
const SETTLE: Duration = Duration::from_secs(3);

/// How long the game gets after Quit before it is stopped.
pub fn grace(how: How) -> Duration {
    match how {
        How::RetroArch => RETROARCH_GRACE,
        How::Standalone => crate::standalone::QUIT_GRACE,
        How::Window => WINDOW_GRACE,
        How::Guest => crate::standalone::GUEST_GRACE,
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Signal {
    Term,
    Kill,
}

/// After Quit: when to stop a game that hasn't closed by itself. SIGTERM at the deadline, SIGKILL
/// `KILL_AFTER` later, each once; both times are fixed when Quit is chosen.
#[derive(Debug)]
pub struct Escalation {
    term_at: Instant,
    kill_at: Instant,
    sent: Option<Signal>,
}

impl Escalation {
    pub fn new(now: Instant, grace: Duration) -> Escalation {
        Escalation { term_at: now + grace, kill_at: now + grace + KILL_AFTER, sent: None }
    }

    /// The signal that is due at `now`, if any.
    pub fn due(&mut self, now: Instant) -> Option<Signal> {
        let due = match self.sent {
            None if now >= self.kill_at => Signal::Kill,
            None if now >= self.term_at => Signal::Term,
            Some(Signal::Term) if now >= self.kill_at => Signal::Kill,
            _ => return None,
        };
        self.sent = Some(due);
        Some(due)
    }

    /// When `due` has something next.
    pub fn next(&self) -> Option<Instant> {
        match self.sent {
            None => Some(self.term_at),
            Some(Signal::Term) => Some(self.kill_at),
            Some(Signal::Kill) => None,
        }
    }

    /// Quitting still counts as work that keeps the system awake; not any more once a game outlived SIGKILL.
    pub fn busy(&self, now: Instant) -> bool {
        now < self.kill_at + SETTLE
    }
}

/// After an update was installed: whether pocket-ui may end now so the launcher starts the new version.
/// Not while a game runs: it would go on without anyone to pause, save or end it; the restart follows its end.
pub fn restart_due(at: Option<Instant>, now: Instant, game_running: bool) -> bool {
    !game_running && at.is_some_and(|t| now >= t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_update_restart_waits_for_the_game_to_end() {
        let t0 = Instant::now();
        let at = Some(t0 + Duration::from_millis(1500));
        assert!(!restart_due(at, t0, false));
        assert!(restart_due(at, t0 + Duration::from_secs(2), false));
        assert!(!restart_due(at, t0 + Duration::from_secs(2), true), "the restart would orphan the running game");
        assert!(!restart_due(None, t0 + Duration::from_secs(2), false));
    }

    /// Every signal the escalation sends in the first minute, polled as often as the main loop would.
    fn run(grace: Duration) -> (Vec<(Duration, Signal)>, Escalation, Instant) {
        let t0 = Instant::now();
        let mut e = Escalation::new(t0, grace);
        let mut sent = Vec::new();
        let mut now = t0;
        while now < t0 + Duration::from_secs(60) {
            if let Some(s) = e.due(now) {
                sent.push((now - t0, s));
            }
            now += Duration::from_millis(100);
        }
        (sent, e, now)
    }

    #[test]
    fn escalation_reaches_sigkill() {
        let (sent, _, _) = run(Duration::from_secs(5));
        let signals: Vec<Signal> = sent.iter().map(|(_, s)| *s).collect();
        assert_eq!(signals, [Signal::Term, Signal::Kill], "sent {sent:?}");
        assert!(sent[0].0 >= Duration::from_secs(5) && sent[0].0 < Duration::from_millis(5200));
        assert!(sent[1].0 >= Duration::from_secs(5) + KILL_AFTER && sent[1].0 < Duration::from_millis(7200));
    }

    #[test]
    fn quitting_stops_blocking_sleep_once_the_escalation_is_through() {
        let t0 = Instant::now();
        let e = Escalation::new(t0, Duration::from_secs(5));
        assert!(e.busy(t0 + Duration::from_secs(1)));
        let (_, e, end) = run(Duration::from_secs(5));
        assert!(!e.busy(end), "a game that survived SIGKILL keeps the system awake forever");
        assert_eq!(e.next(), None, "nothing left to wake up for");
    }

    #[test]
    fn quit_without_saving_only_where_a_resume_state_can_be_left_out_safely() {
        assert!(offers_quit_without_saving(Saver::PocketCore { states: true }));
        assert!(offers_quit_without_saving(Saver::Standalone));
        assert!(!offers_quit_without_saving(Saver::PocketCore { states: false }), "ScummVM: nothing to leave out");
        assert!(!offers_quit_without_saving(Saver::RetroArch), "RetroArch writes .state.auto on every exit");
        assert!(!offers_quit_without_saving(Saver::Other));
    }

    #[test]
    fn every_quit_has_a_deadline() {
        for how in [How::RetroArch, How::Standalone, How::Window] {
            assert!(grace(how) < Duration::from_secs(60), "{how:?} quits without a deadline");
        }
    }
}
