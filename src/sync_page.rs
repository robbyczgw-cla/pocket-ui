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

/// The TrimUI Brick's rows: no PS2, GameCube & Wii or 3DS saves, systems it doesn't play (their folders never exist there, so
/// nothing was synced; the rows only offered what the Brick can't use).
#[cfg(feature = "brick")]
const BRICK_ROWS: [Row; 7] = [
    Row::Switch,
    Row::AfterPlay,
    Row::Versions,
    Row::Share(Share::Saves),
    Row::Share(Share::States),
    Row::Share(Share::Library),
    Row::Share(Share::Bios),
];

/// The rows this device shows.
pub fn rows() -> &'static [Row] {
    #[cfg(feature = "brick")]
    if crate::device::brick() {
        return &BRICK_ROWS;
    }
    &ROWS
}

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
    /// X on a paused share row: resume every paused folder of it (`sync::resume`).
    Resume(Share),
}

/// A share row's folders paused in Syncthing, as the poller last read them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Paused {
    /// How many of the row's folders are paused (GameCube & Wii has two).
    pub folders: usize,
    /// Every one of them written down as paused by pocket-ui for a game (`sync::HELD_PATH`). Otherwise someone paused one in
    /// Syncthing, or pocket-ui stopped between its pause and writing it down: the page can't tell those apart.
    pub ours: bool,
}

/// News for the page from its threads.
#[derive(Debug)]
pub enum Event {
    /// Syncthing was switched; the password of its web interface when it was switched on.
    Switched { on: bool, result: Result<Option<String>, String> },
    /// A share was switched on or off in Syncthing.
    ShareApplied(Share, Result<(), String>),
    /// What the poller read: the config file, whether the service runs, and the status; `held`, the folders pocket-ui paused.
    Poll {
        cfg: Option<Config>,
        snap: Snapshot,
        password: Option<String>,
        held: Vec<String>,
    },
    /// Resume on a row went through (how many folders) or not.
    Resumed(Share, Result<usize, String>),
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
    /// The folders pocket-ui paused for a game (`sync::held_now`), from the last poll.
    pub held: Vec<String>,
    /// The row whose folders are being resumed, and the one resumed last (its line says so until the selection moves).
    pub resuming: Option<Share>,
    pub resumed: Option<Share>,
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
            held: Vec::new(),
            resuming: None,
            resumed: None,
        }
    }

    /// The row's paused folders, while Syncthing is on.
    pub fn paused(&self, share: Share) -> Option<Paused> {
        let cfg = self.cfg.as_ref().filter(|_| self.on())?;
        let ids: Vec<&str> = share.dirs().iter().map(|d| d.id).filter(|id| cfg.folders.iter().any(|f| f.id == *id && f.paused)).collect();
        (!ids.is_empty()).then(|| Paused {
            folders: ids.len(),
            ours: ids.iter().all(|id| self.held.iter().any(|h| h == id)),
        })
    }

    /// X resumes the selected row: its folders are paused, Syncthing answers and nothing else is under way.
    pub fn can_resume(&self) -> bool {
        let answering = self.snap.is_some_and(|s| s.running);
        self.busy.is_none() && self.resuming.is_none() && answering && matches!(self.row(), Row::Share(s) if self.paused(s).is_some())
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
        rows()[self.sel.min(rows().len() - 1)]
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
        let n = rows().len();
        if b == Button::Up {
            self.sel = (self.sel + n - 1) % n;
            self.resumed = None;
        } else if b == Button::Down {
            self.sel = (self.sel + 1) % n;
            self.resumed = None;
        } else if b == Button::North {
            if let (true, Row::Share(s)) = (self.can_resume(), self.row()) {
                self.error = None;
                self.resuming = Some(s);
                return Action::Resume(s);
            }
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
            Row::Share(s) if self.resuming == Some(s) => "Resuming".to_string(),
            Row::Share(s) if self.paused(s).is_some() => "Paused".to_string(),
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
            Row::Share(s) => self.describe_pause(s).unwrap_or(s.blurb()),
        }
    }

    /// A share row's line while it is paused, resuming or just resumed.
    fn describe_pause(&self, s: Share) -> Option<&'static str> {
        let both = s.dirs().len() > 1;
        if self.resuming == Some(s) {
            return Some(if both { "Resuming both folders…" } else { "Resuming…" });
        }
        let Some(p) = self.paused(s) else {
            return (self.resumed == Some(s)).then_some(if both { "Both folders resumed. Syncing again." } else { "Resumed. Syncing again." });
        };
        let answering = self.snap.is_some_and(|s| s.running);
        Some(match (p.ours, answering, p.folders > 1) {
            (true, false, _) => "Paused by pocket-ui. Syncthing isn't answering.",
            (false, false, _) => "Paused, not by pocket-ui. Syncthing isn't answering.",
            (true, true, false) => "Paused by pocket-ui for a game. X resumes it.",
            (true, true, true) => "Paused by pocket-ui for a game. X resumes both folders.",
            (false, true, false) => "Paused, not by pocket-ui. X resumes it.",
            (false, true, true) => "Paused, not by pocket-ui. X resumes both folders.",
        })
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
            Event::Poll { cfg, snap, password, held } => {
                self.cfg = cfg;
                if self.busy.is_none() {
                    self.snap = Some(snap);
                }
                self.password = password;
                self.held = held;
            }
            Event::Resumed(share, result) => {
                self.resuming = None;
                match result {
                    Ok(_) => {
                        // Not paused any more, before the next poll reads it.
                        let ids: Vec<&str> = share.dirs().iter().map(|d| d.id).collect();
                        for f in self.cfg.iter_mut().flat_map(|c| c.folders.iter_mut()).filter(|f| ids.contains(&f.id.as_str())) {
                            f.paused = false;
                        }
                        self.held.retain(|h| !ids.contains(&h.as_str()));
                        self.resumed = Some(share);
                    }
                    // The row stays selected and says Paused: the status says why, without the row's name (room beside the QR code).
                    Err(msg) => self.error = Some(msg),
                }
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
        assert_eq!(p.describe(Row::Share(Share::Library)), "Favourites, status, notes, labels, play history.");
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

    /// Syncthing on, answering or not, with these folders paused and `held` naming ours.
    fn paused_page(paused: &[&str], held: &[&str], answering: bool) -> SyncPage {
        let mut p = SyncPage::new(Prefs { enabled: true, ..Prefs::default() }, None);
        let mut cfg = Config::parse(include_str!("fixtures/syncthing-pocket.xml")).unwrap();
        for id in ["pocket-gc-cards", "pocket-wii-saves"] {
            cfg.folders.push(crate::sync::Folder { id: id.into(), label: String::new(), path: String::new(), paused: false });
        }
        for f in &mut cfg.folders {
            f.paused = paused.contains(&f.id.as_str());
        }
        p.event(Event::Poll {
            cfg: Some(cfg),
            snap: Snapshot { running: answering, ..Default::default() },
            password: None,
            held: held.iter().map(|h| h.to_string()).collect(),
        });
        p
    }

    #[test]
    fn a_pause_that_is_not_ours_shows_on_its_row_and_x_resumes_only_when_chosen() {
        // Paused by hand in Syncthing, or in the gap between our pause and its marker: the same to the page, and never resumed
        // by itself (the poll alone asks for nothing).
        let mut p = paused_page(&["pocket-gc-cards", "pocket-wii-saves"], &[], true);
        let dolphin = Row::Share(Share::Dolphin);
        assert_eq!(p.value(dolphin), "Paused");
        assert_eq!(p.paused(Share::Dolphin), Some(Paused { folders: 2, ours: false }));
        assert_eq!(p.value(Row::Share(Share::Saves)), "Shared", "the other rows as they were");
        go_to(&mut p, dolphin);
        assert_eq!(p.describe(dolphin), "Paused, not by pocket-ui. X resumes both folders.");
        assert!(p.can_resume());
        // A still flips the share as before; X resumes.
        assert_eq!(p.press(Button::North, CONFIRM, CANCEL), Action::Resume(Share::Dolphin));
        assert_eq!(p.value(dolphin), "Resuming");
        assert_eq!(p.describe(dolphin), "Resuming both folders…");
        assert_eq!(p.press(Button::North, CONFIRM, CANCEL), Action::Nothing, "one at a time");
        p.event(Event::Resumed(Share::Dolphin, Ok(2)));
        assert_eq!(p.value(dolphin), "Shared");
        assert_eq!(p.describe(dolphin), "Both folders resumed. Syncing again.");
        assert!(!p.can_resume());
        // On a row with nothing paused X does nothing.
        go_to(&mut p, Row::Share(Share::Saves));
        assert_eq!(p.press(Button::North, CONFIRM, CANCEL), Action::Nothing);
        go_to(&mut p, dolphin);
        assert_eq!(p.press(CONFIRM, CONFIRM, CANCEL), Action::SetShare(Share::Dolphin, false), "the share switch stays");
    }

    #[test]
    fn our_own_pause_says_so_and_one_folder_is_it() {
        let mut p = paused_page(&["pocket-saves"], &["pocket-saves"], true);
        go_to(&mut p, Row::Share(Share::Saves));
        assert_eq!(p.paused(Share::Saves), Some(Paused { folders: 1, ours: true }));
        assert_eq!(p.describe(Row::Share(Share::Saves)), "Paused by pocket-ui for a game. X resumes it.");
        assert_eq!(p.press(Button::North, CONFIRM, CANCEL), Action::Resume(Share::Saves));
        // One folder of ours and one not: not all ours, so "not by pocket-ui".
        let p = paused_page(&["pocket-gc-cards", "pocket-wii-saves"], &["pocket-gc-cards"], true);
        assert_eq!(p.paused(Share::Dolphin), Some(Paused { folders: 2, ours: false }));
    }

    #[test]
    fn no_resume_while_syncthing_does_not_answer_or_is_off_and_a_failure_says_why() {
        let mut p = paused_page(&["pocket-wii-saves"], &[], false);
        let dolphin = Row::Share(Share::Dolphin);
        go_to(&mut p, dolphin);
        assert_eq!(p.value(dolphin), "Paused", "what the config file says is still shown");
        assert_eq!(p.describe(dolphin), "Paused, not by pocket-ui. Syncthing isn't answering.");
        assert!(!p.can_resume());
        assert_eq!(p.press(Button::North, CONFIRM, CANCEL), Action::Nothing);
        // Off: nothing syncs, nothing is paused as far as the page says.
        p.prefs.enabled = false;
        assert_eq!(p.value(dolphin), "Shared");
        // Answering, then the resume fails (a game started, Syncthing refused): still paused, and the status says why.
        let mut p = paused_page(&["pocket-wii-saves"], &[], true);
        go_to(&mut p, dolphin);
        assert_eq!(p.describe(dolphin), "Paused, not by pocket-ui. X resumes it.");
        assert_eq!(p.press(Button::North, CONFIRM, CANCEL), Action::Resume(Share::Dolphin));
        p.event(Event::Resumed(Share::Dolphin, Err(crate::sync::PLAYING.into())));
        assert_eq!(p.error.as_deref(), Some("Not while a game runs."));
        assert_eq!(p.value(dolphin), "Paused");
        assert!(p.can_resume(), "it can be tried again");
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
            held: Vec::new(),
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
            held: Vec::new(),
        });
        assert_eq!(p.problems(), vec![]);
        // Not running: its config file says nothing about what is reachable.
        let rocknix = Config::parse(include_str!("fixtures/syncthing-rocknix.xml")).unwrap();
        p.event(Event::Poll {
            cfg: Some(rocknix),
            snap: Snapshot::default(),
            password: None,
            held: Vec::new(),
        });
        assert_eq!(p.problems(), vec![]);
    }
}
