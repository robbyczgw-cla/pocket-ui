//! Settings > Sync: the state of the page and what a press on it asks for. The App turns an `Action` into work
//! on a thread and reports back with an `Event`; painting is in the binary (`app_sync.rs`).

use crate::pad::Button;
use crate::sync::{Config, Outcome, Prefs, Problem, Share, Snapshot};

/// A row of the page, top to bottom.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Row {
    /// Syncthing on or off.
    Switch,
    /// Rescan the save folders when a game ends.
    AfterPlay,
    /// Saves with two versions (`sync_conflicts`): how many wait, A lists them.
    Versions,
    Share(Share),
}

pub const ROWS: [Row; 10] = [
    Row::Switch,
    Row::AfterPlay,
    Row::Versions,
    Row::Share(Share::Saves),
    Row::Share(Share::States),
    Row::Share(Share::Ps2Cards),
    Row::Share(Share::Dolphin),
    Row::Share(Share::Azahar),
    Row::Share(Share::Library),
    Row::Share(Share::Bios),
];

impl Row {
    pub fn label(self) -> &'static str {
        match self {
            Row::Switch => "Syncthing",
            Row::AfterPlay => "Sync after playing",
            Row::Versions => "Two versions",
            Row::Share(s) => s.label(),
        }
    }
}

/// What a press asks the App to do.
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    Nothing,
    Close,
    SetEnabled(bool),
    SetAfterPlay(bool),
    SetShare(Share, bool),
    /// The saves with two versions, to choose one each (`sync_conflict_page`).
    OpenVersions,
}

/// News for the page from its threads.
#[derive(Debug)]
pub enum Event {
    /// Syncthing was switched; the password of its web interface when it was switched on.
    Switched { on: bool, result: Result<Option<String>, String> },
    /// A share was switched on or off in Syncthing.
    ShareApplied(Share, Result<(), String>),
    /// What the poller read: the config file, whether the service runs, and the status.
    Poll {
        cfg: Option<Config>,
        snap: Snapshot,
        password: Option<String>,
    },
    /// The scan after a game finished.
    AfterGame(Outcome),
    /// The search for saves with two versions found these (`app_sync_conflicts`).
    Conflicts(Vec<crate::sync_conflicts::Group>),
    /// A version was kept: the entry's file, and the backup folder or what went wrong.
    Kept {
        original: std::path::PathBuf,
        result: Result<std::path::PathBuf, String>,
    },
    /// Library data: a merge with the other devices found changes to apply here (`app_library_sync`).
    Library(Box<crate::library_sync::Outcome>),
}

pub struct SyncPage {
    pub sel: usize,
    pub prefs: Prefs,
    pub cfg: Option<Config>,
    pub snap: Option<Snapshot>,
    pub password: Option<String>,
    pub ip: Option<String>,
    /// "Turning Syncthing on ..." while the switch works.
    pub busy: Option<&'static str>,
    pub error: Option<String>,
    /// Stops the poller thread when the page closes.
    pub alive: std::sync::Arc<std::sync::atomic::AtomicBool>,
    /// Saves with two versions waiting for a choice (set by the App from its last search).
    pub waiting: usize,
}

impl SyncPage {
    pub fn new(prefs: Prefs, ip: Option<String>) -> SyncPage {
        SyncPage {
            sel: 0,
            prefs,
            cfg: None,
            snap: None,
            password: None,
            ip,
            busy: None,
            error: None,
            alive: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true)),
            waiting: 0,
        }
    }

    /// Saves waiting for a choice, as far as the page says: none while Syncthing is off.
    pub fn waiting(&self) -> usize {
        if self.on() {
            self.waiting
        } else {
            0
        }
    }

    /// Syncthing is on as far as the user can tell: switched on here, or running on its own (ROCKNIX's setting).
    pub fn on(&self) -> bool {
        self.prefs.enabled || self.snap.is_some_and(|s| s.running)
    }

    pub fn row(&self) -> Row {
        ROWS[self.sel.min(ROWS.len() - 1)]
    }

    /// The web interface is reachable from the network in a way that needs fixing.
    pub fn problems(&self) -> Vec<Problem> {
        self.cfg
            .as_ref()
            .filter(|_| self.snap.is_some_and(|s| s.running))
            .map(Config::problems)
            .unwrap_or_default()
    }

    pub fn press(&mut self, b: Button, confirm: Button, cancel: Button) -> Action {
        let n = ROWS.len();
        if b == Button::Up {
            self.sel = (self.sel + n - 1) % n;
        } else if b == Button::Down {
            self.sel = (self.sel + 1) % n;
        } else if b == cancel {
            self.alive.store(false, std::sync::atomic::Ordering::Relaxed);
            return Action::Close;
        } else if b == confirm {
            return self.activate();
        }
        Action::Nothing
    }

    fn activate(&mut self) -> Action {
        if self.busy.is_some() {
            return Action::Nothing;
        }
        match self.row() {
            Row::Switch => {
                self.error = None;
                Action::SetEnabled(!self.on())
            }
            Row::AfterPlay => {
                self.prefs.after_play = !self.prefs.after_play;
                Action::SetAfterPlay(self.prefs.after_play)
            }
            Row::Versions if self.waiting() > 0 => Action::OpenVersions,
            Row::Versions => Action::Nothing,
            Row::Share(s) => {
                let on = !self.prefs.shared(s);
                self.prefs.set_shared(s, on);
                Action::SetShare(s, on)
            }
        }
    }

    /// The value at the right of a row.
    pub fn value(&self, row: Row) -> String {
        let yes_no = |b: bool| if b { "On" } else { "Off" }.to_string();
        match row {
            Row::Switch => yes_no(self.on()),
            Row::AfterPlay => yes_no(self.prefs.after_play),
            Row::Versions => match self.waiting() {
                0 => crate::sync_conflict_page::waiting_text(0),
                n => format!("{} >", crate::sync_conflict_page::waiting_text(n)),
            },
            Row::Share(s) => if self.prefs.shared(s) { "Shared" } else { "Not shared" }.to_string(),
        }
    }

    /// The line under the list for the selected row.
    pub fn describe(&self, row: Row) -> &'static str {
        match row {
            Row::Switch => "Saves and states, in step across devices.",
            Row::AfterPlay => "Look for new saves as soon as a game ends.",
            Row::Versions if self.waiting() > 0 => "Saves changed on two devices. Choose which to keep.",
            Row::Versions => "Nothing is waiting for a choice.",
            Row::Share(s) => s.blurb(),
        }
    }

    pub fn event(&mut self, e: Event) {
        match e {
            Event::Switched { on, result } => {
                self.busy = None;
                match result {
                    Ok(password) => {
                        self.prefs.enabled = on;
                        self.error = None;
                        if on {
                            self.password = password;
                        } else {
                            self.snap = None;
                        }
                    }
                    Err(msg) => {
                        self.prefs.enabled = false;
                        self.error = Some(msg);
                    }
                }
            }
            Event::ShareApplied(share, Err(msg)) => self.error = Some(format!("{}: {msg}", share.label())),
            Event::ShareApplied(_, Ok(())) => {}
            Event::Poll { cfg, snap, password } => {
                self.cfg = cfg;
                if self.busy.is_none() {
                    self.snap = Some(snap);
                }
                self.password = password;
            }
            Event::AfterGame(_) | Event::Conflicts(_) | Event::Kept { .. } | Event::Library(_) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONFIRM: Button = Button::East;
    const CANCEL: Button = Button::South;

    fn page() -> SyncPage {
        SyncPage::new(Prefs::default(), Some("192.0.2.57".into()))
    }

    fn go_to(p: &mut SyncPage, row: Row) {
        while p.row() != row {
            p.press(Button::Down, CONFIRM, CANCEL);
        }
    }

    #[test]
    fn the_switch_asks_for_on_then_for_off() {
        let mut p = page();
        assert_eq!(p.value(Row::Switch), "Off");
        assert_eq!(p.press(CONFIRM, CONFIRM, CANCEL), Action::SetEnabled(true));
        p.event(Event::Switched {
            on: true,
            result: Ok(Some("maple-river-stone-482".into())),
        });
        assert_eq!(p.value(Row::Switch), "On");
        assert_eq!(p.password.as_deref(), Some("maple-river-stone-482"));
        assert_eq!(p.press(CONFIRM, CONFIRM, CANCEL), Action::SetEnabled(false));
    }

    #[test]
    fn a_failed_switch_stays_off_and_says_why() {
        let mut p = page();
        p.press(CONFIRM, CONFIRM, CANCEL);
        p.busy = Some("Turning Syncthing on");
        assert_eq!(p.press(CONFIRM, CONFIRM, CANCEL), Action::Nothing, "busy: a second press does nothing");
        p.event(Event::Switched {
            on: true,
            result: Err("systemctl failed".into()),
        });
        assert!(!p.on());
        assert_eq!(p.error.as_deref(), Some("systemctl failed"));
    }

    #[test]
    fn the_other_rows_flip_their_choice() {
        let mut p = page();
        go_to(&mut p, Row::AfterPlay);
        assert_eq!(p.press(CONFIRM, CONFIRM, CANCEL), Action::SetAfterPlay(false));
        assert_eq!(p.value(Row::AfterPlay), "Off");
        go_to(&mut p, Row::Share(Share::States));
        assert_eq!(p.press(CONFIRM, CONFIRM, CANCEL), Action::SetShare(Share::States, false));
        go_to(&mut p, Row::Share(Share::Bios));
        assert_eq!(p.value(Row::Share(Share::Bios)), "Not shared", "never by default");
        assert_eq!(p.press(CONFIRM, CONFIRM, CANCEL), Action::SetShare(Share::Bios, true));
    }

    #[test]
    fn the_big_systems_have_a_row_each_and_bios_stays_last() {
        let labels: Vec<&str> = ROWS.iter().map(|r| r.label()).collect();
        assert_eq!(
            labels,
            [
                "Syncthing",
                "Sync after playing",
                "Two versions",
                "Saves",
                "Save states",
                "PS2 memory cards",
                "GameCube & Wii saves",
                "3DS saves",
                "Library data",
                "BIOS files"
            ]
        );
        let mut p = page();
        for share in [Share::Ps2Cards, Share::Dolphin, Share::Azahar] {
            assert_eq!(p.value(Row::Share(share)), "Shared", "on by default");
            go_to(&mut p, Row::Share(share));
            assert_eq!(p.press(CONFIRM, CONFIRM, CANCEL), Action::SetShare(share, false));
        }
    }

    #[test]
    fn library_data_is_off_at_the_start_and_says_what_it_shares() {
        let mut p = page();
        assert_eq!(p.value(Row::Share(Share::Library)), "Not shared", "opt-in");
        assert_eq!(p.describe(Row::Share(Share::Library)), "Favourites, status, notes, stickers, play history.");
        go_to(&mut p, Row::Share(Share::Library));
        assert_eq!(p.press(CONFIRM, CONFIRM, CANCEL), Action::SetShare(Share::Library, true));
        assert_eq!(p.value(Row::Share(Share::Library)), "Shared");
        assert!(p.prefs.library);
        // Not a save: never held while a game runs, never searched for two versions, never rescanned after a game.
        assert!(!Share::Library.saves() && !Share::Library.recommended());
        assert!(crate::sync::held_for(&p.prefs, "gba").iter().all(|id| *id != "pocket-library"));
    }

    #[test]
    fn the_versions_row_counts_what_waits_and_opens_only_with_something_there() {
        let mut p = page();
        go_to(&mut p, Row::Versions);
        // Nothing found, or Syncthing off: nothing waits, and A does nothing.
        assert_eq!(p.value(Row::Versions), "Nothing waiting");
        assert_eq!(p.describe(Row::Versions), "Nothing is waiting for a choice.");
        assert_eq!(p.press(CONFIRM, CONFIRM, CANCEL), Action::Nothing);
        p.waiting = 2;
        assert_eq!(p.value(Row::Versions), "Nothing waiting", "Syncthing is off");
        assert_eq!(p.press(CONFIRM, CONFIRM, CANCEL), Action::Nothing);
        p.prefs.enabled = true;
        assert_eq!(p.value(Row::Versions), "2 to choose >");
        assert_eq!(p.press(CONFIRM, CONFIRM, CANCEL), Action::OpenVersions);
    }

    #[test]
    fn up_and_down_wrap_and_back_closes_and_stops_the_poller() {
        let mut p = page();
        p.press(Button::Up, CONFIRM, CANCEL);
        assert_eq!(p.row(), Row::Share(Share::Bios));
        p.press(Button::Down, CONFIRM, CANCEL);
        assert_eq!(p.row(), Row::Switch);
        assert!(p.alive.load(std::sync::atomic::Ordering::Relaxed));
        assert_eq!(p.press(CANCEL, CONFIRM, CANCEL), Action::Close);
        assert!(!p.alive.load(std::sync::atomic::Ordering::Relaxed));
    }

    #[test]
    fn syncthing_running_on_its_own_counts_as_on_and_an_open_interface_is_reported() {
        let mut p = page();
        let rocknix = Config::parse(include_str!("fixtures/syncthing-rocknix.xml")).unwrap();
        p.event(Event::Poll {
            cfg: Some(rocknix),
            snap: Snapshot {
                running: true,
                ..Default::default()
            },
            password: None,
        });
        assert!(p.on(), "started by ROCKNIX's own setting");
        assert_eq!(p.problems(), vec![Problem::RocknixLogin, Problem::ClearText]);
        assert_eq!(p.press(CONFIRM, CONFIRM, CANCEL), Action::SetEnabled(false));
        let ours = Config::parse(include_str!("fixtures/syncthing-pocket.xml")).unwrap();
        p.event(Event::Poll {
            cfg: Some(ours),
            snap: Snapshot {
                running: true,
                ..Default::default()
            },
            password: None,
        });
        assert_eq!(p.problems(), vec![]);
        // Not running: its config file says nothing about what is reachable.
        let rocknix = Config::parse(include_str!("fixtures/syncthing-rocknix.xml")).unwrap();
        p.event(Event::Poll {
            cfg: Some(rocknix),
            snap: Snapshot::default(),
            password: None,
        });
        assert_eq!(p.problems(), vec![]);
    }
}
