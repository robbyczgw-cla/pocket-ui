//! Settings → Backup & reset: the state of the page and what a press asks for. The App does the work on a thread
//! (`backup::create`, `restore`, `reset`, `backup_saves`) and reports back with an `Event`; after a restore or a reset of
//! the settings pocket-ui starts again, so everything it holds in memory comes from the files. Painting is in the binary
//! (`app_backup.rs`).
//!
//! Saves take two steps: A looks (how much, does it fit, what would a restore change), the page says so, a second A does it.
//! The copying reads and writes on its thread with `Progress` beside it, so the page keeps moving and B stops it.

use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use crate::backup::Backup;
use crate::backup_saves::{Plan, Progress, RestorePlan, Restored, SavesBackup};
use crate::pad::Button;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Row {
    BackUp,
    BackUpSaves,
    Reset,
    /// A backup of saves, newest first.
    Saves(usize),
    /// A backup of the settings, newest first.
    Backup(usize),
}

/// What a finished job did.
#[derive(Debug)]
pub enum Done {
    BackedUp(Backup),
    /// Reset: the backup made before it.
    Reset(Backup),
    /// Restored: the backup of the state before it.
    Restored(Backup),
    SavesBackedUp(SavesBackup),
    SavesRestored(Restored),
}

#[derive(Debug)]
pub enum Event {
    Listed(Vec<Backup>, Vec<SavesBackup>),
    /// What a backup of the saves would hold.
    SavesMeasured(Plan),
    /// What a restore of the saves would change.
    SavesCompared(Result<RestorePlan, String>),
    Done(Result<Done, String>),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    Nothing,
    Close,
    BackUp,
    Reset,
    Restore(PathBuf),
    MeasureSaves,
    BackUpSaves(Plan),
    CompareSaves(PathBuf),
    RestoreSaves(RestorePlan),
}

#[derive(Default)]
pub struct BackupPage {
    /// None until listed.
    pub list: Option<Vec<Backup>>,
    pub saves: Vec<SavesBackup>,
    /// The saves were counted: what A would back up.
    pub plan: Option<Plan>,
    /// A saves backup was compared: what A would change.
    pub restore_plan: Option<RestorePlan>,
    /// Saves are being copied.
    pub progress: Option<Arc<Progress>>,
    pub sel: usize,
    /// A was pressed once on this row (Reset, a backup): the next A does it.
    pub confirm: Option<Row>,
    /// Work under way: what to say.
    pub busy: Option<String>,
    /// The last result: text, is an error.
    pub message: Option<(String, bool)>,
    /// A restore or reset is done: pocket-ui starts again in a moment; nothing more is taken.
    pub restarting: bool,
}

impl BackupPage {
    pub fn new() -> BackupPage {
        BackupPage::default()
    }

    pub fn rows(&self) -> Vec<Row> {
        let mut rows = vec![Row::BackUpSaves, Row::BackUp, Row::Reset];
        rows.extend((0..self.saves.len()).map(Row::Saves));
        rows.extend((0..self.list.as_ref().map_or(0, Vec::len)).map(Row::Backup));
        rows
    }

    pub fn row(&self) -> Row {
        self.rows().get(self.sel).copied().unwrap_or(Row::BackUpSaves)
    }

    pub fn backup(&self, i: usize) -> Option<&Backup> {
        self.list.as_ref()?.get(i)
    }

    pub fn press(&mut self, b: Button, confirm: Button, cancel: Button) -> Action {
        if self.restarting {
            return Action::Nothing;
        }
        // Copying saves: the page stays, whatever moves or closes it would leave the player guessing. B stops it.
        if let Some(p) = &self.progress {
            if b == cancel {
                p.cancel.store(true, Ordering::Relaxed);
                self.busy = Some("Stopping".into());
            }
            return Action::Nothing;
        }
        let n = self.rows().len();
        let before = self.sel;
        let action = match b {
            b if b == cancel && self.confirm.take().is_some() => Action::Nothing,
            b if b == cancel => Action::Close,
            Button::Up => {
                self.sel = (self.sel + n - 1) % n;
                Action::Nothing
            }
            Button::Down => {
                self.sel = (self.sel + 1) % n;
                Action::Nothing
            }
            Button::L1 => {
                self.sel = self.sel.saturating_sub(5);
                Action::Nothing
            }
            Button::R1 => {
                self.sel = (self.sel + 5).min(n - 1);
                Action::Nothing
            }
            _ if self.busy.is_some() => Action::Nothing,
            b if b == confirm => {
                let row = self.row();
                match row {
                    Row::BackUp => {
                        self.busy = Some("Backing up".into());
                        Action::BackUp
                    }
                    Row::BackUpSaves => match self.plan.take().filter(|_| self.confirm == Some(row)) {
                        Some(plan) => {
                            self.confirm = None;
                            self.progress = Some(Arc::default());
                            self.busy = Some("Backing up your saves".into());
                            Action::BackUpSaves(plan)
                        }
                        None => {
                            self.message = None;
                            self.busy = Some("Counting your saves".into());
                            Action::MeasureSaves
                        }
                    },
                    Row::Saves(i) => match self.restore_plan.take().filter(|_| self.confirm == Some(row)) {
                        Some(plan) => {
                            self.confirm = None;
                            self.progress = Some(Arc::default());
                            self.busy = Some("Putting your saves back".into());
                            Action::RestoreSaves(plan)
                        }
                        None => match self.saves.get(i).map(|b| b.dir.clone()) {
                            Some(dir) => {
                                self.message = None;
                                self.busy = Some("Comparing with what is on the card".into());
                                Action::CompareSaves(dir)
                            }
                            None => Action::Nothing,
                        },
                    },
                    // Reset and restore change settings: a second A first.
                    _ if self.confirm != Some(row) => {
                        self.confirm = Some(row);
                        Action::Nothing
                    }
                    Row::Reset => {
                        self.confirm = None;
                        self.busy = Some("Backing up, then resetting".into());
                        Action::Reset
                    }
                    Row::Backup(i) => {
                        self.confirm = None;
                        match self.backup(i).map(|b| b.dir.clone()) {
                            Some(dir) => {
                                self.busy = Some("Backing up now, then restoring".into());
                                Action::Restore(dir)
                            }
                            None => Action::Nothing,
                        }
                    }
                }
            }
            _ => Action::Nothing,
        };
        if self.sel != before {
            self.confirm = None;
            self.message = None;
        }
        // Taken back, or moved away: the next A looks again, what it saw may be old by then.
        if self.confirm.is_none() {
            self.plan = None;
            self.restore_plan = None;
        }
        action
    }

    pub fn event(&mut self, e: Event) {
        match e {
            Event::Listed(list, saves) => {
                self.list = Some(list);
                self.saves = saves;
                self.sel = self.sel.min(self.rows().len() - 1);
            }
            Event::SavesMeasured(plan) => {
                self.busy = None;
                if self.row() != Row::BackUpSaves {
                    return;
                }
                if plan.items.is_empty() {
                    self.message = Some(("No saves or states on this card yet.".into(), false));
                } else if !plan.fits() {
                    let free = crate::storage::size(plan.free.unwrap_or(0));
                    self.message = Some((format!("Not enough space on the card: this needs {}, {free} are free.", crate::storage::size(plan.needed)), true));
                } else {
                    self.plan = Some(plan);
                    self.confirm = Some(Row::BackUpSaves);
                }
            }
            Event::SavesCompared(result) => {
                self.busy = None;
                if !matches!(self.row(), Row::Saves(_)) {
                    return;
                }
                match result {
                    Err(e) => self.message = Some((e, true)),
                    Ok(plan) if plan.nothing_to_do() && plan.keep.is_empty() && plan.damaged == 0 => {
                        self.message = Some(("Everything in this backup is already on the card.".into(), false));
                    }
                    Ok(plan) if plan.nothing_to_do() => {
                        let text = if plan.damaged > 0 {
                            format!("Nothing to put back. Newer on the card: {}. Damaged in this backup: {}.", plan.keep.len(), plan.damaged)
                        } else {
                            format!("Nothing to put back. Newer on the card, staying: {}.", plan.keep.len())
                        };
                        self.message = Some((text, false));
                    }
                    Ok(plan) if !plan.fits() => {
                        let free = crate::storage::size(plan.free.unwrap_or(0));
                        self.message = Some((format!("Not enough space on the card: this needs {}, {free} are free.", crate::storage::size(plan.needed)), true));
                    }
                    Ok(plan) => {
                        self.restore_plan = Some(plan);
                        self.confirm = Some(self.row());
                    }
                }
            }
            Event::Done(result) => {
                self.busy = None;
                self.progress = None;
                match result {
                    Ok(Done::SavesBackedUp(b)) => {
                        let (n, unreadable) = (b.manifest.items.len(), b.manifest.unreadable.len());
                        self.message = Some(if unreadable > 0 {
                            (format!("Files saved: {n}. Could not be read and missing: {unreadable}, the first: {}", b.manifest.unreadable[0]), true)
                        } else {
                            (format!("Files saved: {n}. In {}", b.dir.display()), false)
                        });
                        self.insert_saves(b);
                    }
                    Ok(Done::SavesRestored(r)) => {
                        let err = !r.failed.is_empty();
                        self.message = Some((restored_text(&r), err));
                        if let Some(b) = r.before {
                            self.insert_saves(b);
                        }
                    }
                    Ok(Done::BackedUp(b)) => {
                        self.message = Some((format!("Saved: {}", b.dir.display()), false));
                        self.list.get_or_insert_with(Vec::new).insert(0, b);
                    }
                    Ok(Done::Reset(_)) => {
                        self.restarting = true;
                        self.message = Some(("Settings reset. pocket-ui starts again…".into(), false));
                    }
                    Ok(Done::Restored(_)) => {
                        self.restarting = true;
                        self.message = Some(("Restored. pocket-ui starts again…".into(), false));
                    }
                    Err(e) => self.message = Some((e, true)),
                }
            }
        }
    }

    /// A new saves backup on top of its list, the selection staying on the row it was on.
    fn insert_saves(&mut self, b: SavesBackup) {
        if matches!(self.row(), Row::Saves(_) | Row::Backup(_)) {
            self.sel += 1;
        }
        self.saves.insert(0, b);
    }

    /// The hint for A.
    pub fn a_hint(&self) -> &'static str {
        match self.row() {
            Row::BackUp => "Back up",
            r if self.confirm == Some(r) && r == Row::BackUpSaves => "Back up now",
            Row::BackUpSaves => "Count",
            r if self.confirm == Some(r) && r == Row::Reset => "Reset now",
            r if self.confirm == Some(r) => "Restore now",
            Row::Reset => "Reset",
            Row::Saves(_) | Row::Backup(_) => "Restore",
        }
    }
}

/// What a finished restore of saves says.
fn restored_text(r: &Restored) -> String {
    let (back, replaced, kept) = (r.put, r.replaced, r.kept);
    if let Some(first) = r.failed.first() {
        return format!("Back: {back}. Replaced: {replaced}. Could not be written: {}, the first: {first}", r.failed.len());
    }
    if r.stopped {
        return format!("Stopped. Back: {back}. Replaced: {replaced}. The rest is still in the backup.");
    }
    if kept > 0 {
        return format!("Back: {back}. Replaced: {replaced}. Newer on the card, stayed: {kept}. This backup still has its copies.");
    }
    format!("Back: {back}. Replaced: {replaced}.")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backup::{Manifest, Reason};
    use crate::backup_saves as saves;

    const A: Button = Button::East;
    const B: Button = Button::South;

    fn backup(name: &str, created: u64) -> Backup {
        Backup {
            dir: PathBuf::from(format!("/b/{name}")),
            manifest: Manifest { version: 1, created, reason: Reason::Manual, app: "1".into(), files: vec!["settings.json".into()] },
            bytes: 100,
        }
    }

    fn page() -> BackupPage {
        let mut p = BackupPage::new();
        p.event(Event::Listed(vec![backup("new", 2), backup("old", 1)], Vec::new()));
        p.sel = 1;
        p
    }

    fn item(path: &str) -> saves::Item {
        saves::Item { path: path.into(), stored: format!("files/{path}"), bytes: 5, modified: 1 }
    }

    fn plan(free: u64) -> Plan {
        Plan { items: vec![item("roms/gba/A.srm")], bytes: 5, free: Some(free), needed: 100, skipped: 0, second_card: false }
    }

    fn saves_backup(name: &str) -> SavesBackup {
        SavesBackup { dir: PathBuf::from(format!("/s/{name}")), manifest: saves::Manifest { version: 1, created: 5, reason: Reason::Manual, app: "1".into(), items: vec![item("roms/gba/A.srm")], unreadable: Vec::new() } }
    }

    fn restore_plan(new: usize, keep: usize) -> RestorePlan {
        RestorePlan { dir: "/s/one".into(), new: (0..new).map(|_| item("roms/gba/A.srm")).collect(), replace: Vec::new(), keep: (0..keep).map(|_| "roms/gba/B.srm".to_string()).collect(), same: 0, damaged: 0, free: Some(1000), needed: 100 }
    }

    #[test]
    fn saves_are_counted_first_and_the_size_is_on_the_page_before_a_second_a_copies() {
        let mut p = page();
        p.sel = 0;
        assert_eq!(p.row(), Row::BackUpSaves);
        assert_eq!(p.a_hint(), "Count");
        assert_eq!(p.press(A, A, B), Action::MeasureSaves);
        assert_eq!(p.press(A, A, B), Action::Nothing, "one at a time");
        p.event(Event::SavesMeasured(plan(1000)));
        assert!(p.busy.is_none() && p.plan.is_some());
        assert_eq!(p.a_hint(), "Back up now");
        assert_eq!(p.press(A, A, B), Action::BackUpSaves(plan(1000)));
        assert!(p.progress.is_some());
        // While it copies the page takes no other press, and it answers: B stops it.
        assert_eq!(p.press(Button::Down, A, B), Action::Nothing);
        assert_eq!(p.sel, 0);
        assert_eq!(p.press(B, A, B), Action::Nothing);
        assert!(p.progress.as_ref().unwrap().cancel.load(Ordering::Relaxed));
        p.event(Event::Done(Ok(Done::SavesBackedUp(saves_backup("made")))));
        assert!(p.progress.is_none() && p.busy.is_none());
        assert_eq!(p.saves.len(), 1);
        assert_eq!(p.rows()[3], Row::Saves(0));
        assert!(p.message.as_ref().is_some_and(|m| !m.1 && m.0.contains("/s/made")));
    }

    #[test]
    fn what_doesnt_fit_or_isnt_there_is_said_and_nothing_can_be_started() {
        let mut p = page();
        p.sel = 0;
        p.press(A, A, B);
        p.event(Event::SavesMeasured(plan(99)));
        assert!(p.plan.is_none() && p.confirm.is_none());
        assert!(p.message.as_ref().is_some_and(|m| m.1 && m.0.starts_with("Not enough space")), "{:?}", p.message);
        p.press(A, A, B);
        let mut none = plan(1000);
        none.items.clear();
        p.event(Event::SavesMeasured(none));
        assert!(p.plan.is_none() && p.message.as_ref().is_some_and(|m| !m.1 && m.0.starts_with("No saves")));
    }

    #[test]
    fn taking_it_back_or_moving_away_counts_again() {
        let mut p = page();
        p.sel = 0;
        p.press(A, A, B);
        p.event(Event::SavesMeasured(plan(1000)));
        assert_eq!(p.press(B, A, B), Action::Nothing);
        assert!(p.plan.is_none() && p.confirm.is_none());
        assert_eq!(p.press(A, A, B), Action::MeasureSaves);
        p.event(Event::SavesMeasured(plan(1000)));
        p.press(Button::Down, A, B);
        assert!(p.plan.is_none());
        p.press(Button::Up, A, B);
        assert_eq!(p.press(A, A, B), Action::MeasureSaves);
        // An answer for a row left in the meantime asks nothing of it.
        p.press(Button::Down, A, B);
        p.event(Event::SavesMeasured(plan(1000)));
        assert!(p.plan.is_none() && p.confirm.is_none() && p.busy.is_none());
    }

    #[test]
    fn a_restore_of_saves_says_what_it_would_change_first_and_keeping_newer_files_is_said() {
        let mut p = page();
        p.event(Event::Listed(vec![backup("new", 2)], vec![saves_backup("one")]));
        p.sel = 3;
        assert_eq!(p.row(), Row::Saves(0));
        assert_eq!(p.press(A, A, B), Action::CompareSaves(PathBuf::from("/s/one")));
        p.event(Event::SavesCompared(Ok(restore_plan(2, 1))));
        assert_eq!(p.a_hint(), "Restore now");
        assert_eq!(p.press(A, A, B), Action::RestoreSaves(restore_plan(2, 1)));
        p.event(Event::Done(Ok(Done::SavesRestored(Restored { put: 2, replaced: 0, kept: 1, failed: Vec::new(), stopped: false, before: Some(saves_backup("before")) }))));
        assert_eq!(p.saves[p.sel - 3].dir, PathBuf::from("/s/one"), "the selection stays on the backup just restored");
        assert!(!p.restarting, "saves need no new start");
        assert!(p.message.as_ref().is_some_and(|m| !m.1 && m.0.contains("Newer on the card, stayed: 1")), "{:?}", p.message);

        p.press(A, A, B);
        p.event(Event::SavesCompared(Ok(restore_plan(0, 0))));
        assert!(p.restore_plan.is_none() && p.message.as_ref().is_some_and(|m| m.0.contains("already on the card")));
        p.press(A, A, B);
        p.event(Event::SavesCompared(Ok(restore_plan(0, 3))));
        assert!(p.restore_plan.is_none() && p.message.as_ref().is_some_and(|m| m.0.contains("Newer on the card, staying: 3")));
        p.press(A, A, B);
        p.event(Event::SavesCompared(Err("The backup's list can't be read. Nothing was changed.".into())));
        assert!(p.message.as_ref().is_some_and(|m| m.1));
        // Failed files are named.
        p.progress = Some(Arc::default());
        p.event(Event::Done(Ok(Done::SavesRestored(Restored { put: 1, replaced: 0, kept: 0, failed: vec!["roms/gba/Zelda: x.srm".into()], stopped: false, before: None }))));
        assert!(p.message.as_ref().is_some_and(|m| m.1 && m.0.contains("Zelda: x.srm")));
    }

    #[test]
    fn backing_up_is_one_press_and_lands_on_top_of_the_list() {
        let mut p = page();
        assert_eq!(p.press(A, A, B), Action::BackUp);
        assert_eq!(p.press(A, A, B), Action::Nothing, "one at a time");
        p.event(Event::Done(Ok(Done::BackedUp(backup("newest", 3)))));
        assert_eq!(p.backup(0).unwrap().dir, PathBuf::from("/b/newest"));
        assert_eq!(p.rows().len(), 6);
    }

    #[test]
    fn reset_and_restore_take_a_second_a_and_then_pocket_ui_starts_again() {
        let mut p = page();
        p.sel = 2;
        assert_eq!(p.press(A, A, B), Action::Nothing);
        assert_eq!(p.a_hint(), "Reset now");
        assert_eq!(p.press(B, A, B), Action::Nothing, "B takes it back");
        assert_eq!(p.press(A, A, B), Action::Nothing);
        assert_eq!(p.press(A, A, B), Action::Reset);
        p.event(Event::Done(Ok(Done::Reset(backup("before", 4)))));
        assert!(p.restarting);
        assert_eq!(p.press(B, A, B), Action::Nothing, "nothing more while it starts again");

        let mut p = page();
        p.sel = 4;
        p.press(A, A, B);
        p.press(Button::Up, A, B);
        assert_eq!(p.confirm, None, "moving away takes it back");
        p.press(Button::Down, A, B);
        p.press(A, A, B);
        assert_eq!(p.press(A, A, B), Action::Restore(PathBuf::from("/b/old")));
        p.event(Event::Done(Err("The backup's list can't be read. Nothing was changed.".into())));
        assert!(!p.restarting && p.message.as_ref().is_some_and(|m| m.1));
    }
}
