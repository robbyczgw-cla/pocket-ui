//! Ending a game after Quit in the pause menu: how long each kind of emulator gets to close by itself, and
//! the escalation to SIGTERM and SIGKILL when it doesn't.

use std::time::{Duration, Instant};

/// From the SIGTERM at the deadline to the SIGKILL.
pub const KILL_AFTER: Duration = Duration::from_secs(2);
/// A power off waits this long for a game to end, unless it is writing its exit save.
pub const POWER_OFF_WAIT: Duration = Duration::from_secs(25);
/// A standalone emulator closed through its window gets this long.
pub const WINDOW_GRACE: Duration = Duration::from_secs(5);
/// While pocket-core says it is writing its exit save (`fast_switch::saving`) nothing stops it, up to this long: a killed
/// writer loses the moment, and a card busy with something else can take many seconds. Only a card that has really gone stops it.
pub const SAVE_LIMIT: Duration = Duration::from_secs(120);

/// How the game was asked to quit.
#[derive(Clone, Copy, Debug)]
pub enum How {
    /// SIGTERM to RetroArch, which writes its auto state and exits.
    RetroArch,
    /// ARMSX2 and Dolphin: resume state first, then their own shutdown (`standalone::quit`).
    Standalone,
    /// Cemu, RPCS3 and Vita3K (`standalone::guest_quit`): the window's close request, then SIGTERM and SIGKILL.
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
    /// A PortMaster port: it saves by itself, Quit ends its whole process group.
    Port,
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

/// Before a power off: whether to go on waiting for the game to end. `POWER_OFF_WAIT`, and as long as it is still writing its exit
/// save up to `SAVE_LIMIT`: switching off under a writer loses what Quit keeps.
pub fn power_off_waits(waited: Duration, alive: bool, saving: bool) -> bool {
    alive && (waited < POWER_OFF_WAIT || (saving && waited < SAVE_LIMIT))
}

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
    since: Instant,
    term_at: Instant,
    kill_at: Instant,
    sent: Option<Signal>,
}

impl Escalation {
    pub fn new(now: Instant, grace: Duration) -> Escalation {
        Escalation { since: now, term_at: now + grace, kill_at: now + grace + KILL_AFTER, sent: None }
    }

    /// The game is writing its exit save right now: nothing is due yet, up to `SAVE_LIMIT` after Quit. When it is done the game
    /// has `KILL_AFTER`-style room again to close.
    pub fn still_saving(&mut self, now: Instant) {
        if now < self.since + SAVE_LIMIT {
            self.term_at = self.term_at.max(now + WINDOW_GRACE);
            self.kill_at = self.term_at + KILL_AFTER;
        }
    }

    /// How long since Quit was chosen: what the screen says about the wait.
    pub fn waited(&self, now: Instant) -> Duration {
        now.saturating_duration_since(self.since)
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

    /// A card busy with something else: the Quit took 11 s, SIGKILL at 7 s killed the writer and the moment was lost.
    #[test]
    fn nothing_stops_a_game_while_it_writes_its_exit_save() {
        let t = Instant::now();
        let mut q = Escalation::new(t, WINDOW_GRACE);
        for s in 0..60u64 {
            let now = t + Duration::from_secs(s);
            q.still_saving(now);
            assert_eq!(q.due(now), None, "{s} s into the save");
        }
        // Written: it has its grace to close, then it is stopped as ever.
        let done = t + Duration::from_secs(60);
        assert_eq!(q.due(done + WINDOW_GRACE - Duration::from_millis(1100)), None);
        assert_eq!(q.due(done + WINDOW_GRACE), Some(Signal::Term));
        assert_eq!(q.due(done + WINDOW_GRACE + KILL_AFTER), Some(Signal::Kill));
    }

    #[test]
    fn a_save_that_never_ends_is_stopped_after_the_limit() {
        let t = Instant::now();
        let mut q = Escalation::new(t, WINDOW_GRACE);
        let mut signals = Vec::new();
        for s in 0..200u64 {
            let now = t + Duration::from_secs(s);
            q.still_saving(now);
            signals.extend(q.due(now).map(|sig| (s, sig)));
        }
        assert_eq!(signals.first().map(|(_, sig)| *sig), Some(Signal::Term));
        assert!(signals.first().unwrap().0 >= SAVE_LIMIT.as_secs(), "{signals:?}");
        assert_eq!(signals.last().map(|(_, sig)| *sig), Some(Signal::Kill));
    }

    #[test]
    fn a_power_off_waits_for_a_writer_and_not_for_ever() {
        let s = Duration::from_secs;
        assert!(power_off_waits(s(10), true, false));
        assert!(!power_off_waits(s(25), true, false), "a game that doesn't end and doesn't write: as before");
        assert!(power_off_waits(s(60), true, true), "still writing past the 25 s");
        assert!(!power_off_waits(SAVE_LIMIT, true, true), "bounded");
        assert!(!power_off_waits(s(1), false, true), "ended");
    }

    #[test]
    fn a_game_that_is_not_saving_is_stopped_as_before() {
        let t = Instant::now();
        let mut q = Escalation::new(t, WINDOW_GRACE);
        assert_eq!(q.due(t + WINDOW_GRACE), Some(Signal::Term));
        assert_eq!(q.due(t + WINDOW_GRACE + KILL_AFTER), Some(Signal::Kill));
    }


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
        assert!(!offers_quit_without_saving(Saver::Port));
    }

    #[test]
    fn every_quit_has_a_deadline() {
        for how in [How::RetroArch, How::Standalone, How::Window] {
            assert!(grace(how) < Duration::from_secs(60), "{how:?} quits without a deadline");
        }
    }
}
