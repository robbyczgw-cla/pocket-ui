//! Settings → Backup & reset: the state of the page and what a press asks for. The App does the work on a thread
//! (`backup::create`, `restore`, `reset`) and reports back with an `Event`; after a restore or a reset pocket-ui starts
//! again, so everything it holds in memory comes from the files. Painting is in the binary (`app_backup.rs`).

use std::path::PathBuf;

use crate::backup::Backup;
use crate::pad::Button;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Row {
    BackUp,
    Reset,
    /// A backup of the list, newest first.
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
}

#[derive(Debug)]
pub enum Event {
    Listed(Vec<Backup>),
    Done(Result<Done, String>),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    Nothing,
    Close,
    BackUp,
    Reset,
    Restore(PathBuf),
}

#[derive(Default)]
pub struct BackupPage {
    /// None until listed.
    pub list: Option<Vec<Backup>>,
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
        let mut rows = vec![Row::BackUp, Row::Reset];
        rows.extend((0..self.list.as_ref().map_or(0, Vec::len)).map(Row::Backup));
        rows
    }

    pub fn row(&self) -> Row {
        self.rows().get(self.sel).copied().unwrap_or(Row::BackUp)
    }

    pub fn backup(&self, i: usize) -> Option<&Backup> {
        self.list.as_ref()?.get(i)
    }

    pub fn press(&mut self, b: Button, confirm: Button, cancel: Button) -> Action {
        if self.restarting {
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
        action
    }

    pub fn event(&mut self, e: Event) {
        match e {
            Event::Listed(list) => {
                self.list = Some(list);
                self.sel = self.sel.min(self.rows().len() - 1);
            }
            Event::Done(result) => {
                self.busy = None;
                match result {
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

    /// The hint for A.
    pub fn a_hint(&self) -> &'static str {
        match self.row() {
            Row::BackUp => "Back up",
            r if self.confirm == Some(r) && r == Row::Reset => "Reset now",
            r if self.confirm == Some(r) => "Restore now",
            Row::Reset => "Reset",
            Row::Backup(_) => "Restore",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backup::{Manifest, Reason};

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
        p.event(Event::Listed(vec![backup("new", 2), backup("old", 1)]));
        p
    }

    #[test]
    fn backing_up_is_one_press_and_lands_on_top_of_the_list() {
        let mut p = page();
        assert_eq!(p.press(A, A, B), Action::BackUp);
        assert_eq!(p.press(A, A, B), Action::Nothing, "one at a time");
        p.event(Event::Done(Ok(Done::BackedUp(backup("newest", 3)))));
        assert_eq!(p.backup(0).unwrap().dir, PathBuf::from("/b/newest"));
        assert_eq!(p.rows().len(), 5);
    }

    #[test]
    fn reset_and_restore_take_a_second_a_and_then_pocket_ui_starts_again() {
        let mut p = page();
        p.sel = 1;
        assert_eq!(p.press(A, A, B), Action::Nothing);
        assert_eq!(p.a_hint(), "Reset now");
        assert_eq!(p.press(B, A, B), Action::Nothing, "B takes it back");
        assert_eq!(p.press(A, A, B), Action::Nothing);
        assert_eq!(p.press(A, A, B), Action::Reset);
        p.event(Event::Done(Ok(Done::Reset(backup("before", 4)))));
        assert!(p.restarting);
        assert_eq!(p.press(B, A, B), Action::Nothing, "nothing more while it starts again");

        let mut p = page();
        p.sel = 3;
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
