//! Settings → Games → Game check: the state of the page and what a press asks for. The App runs the check on a thread
//! (`game_check::check`) and reports with an `Event`; painting is in the binary (`app_game_check.rs`). Three levels: the
//! groups (or the green result), a group's games, one game's detail.

use crate::game_check::{Finding, Fix, Group, Report};
use crate::pad::Button;

/// News for the page from its thread.
#[derive(Debug)]
pub enum Event {
    Progress { done: usize, total: usize, system: String },
    Done(Report),
    /// The list of systems could not be read.
    Failed,
}

/// What a press asks the App to do.
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    Nothing,
    Close,
    /// Look at everything again (X; and A on the offer).
    Start,
    /// Take this fix for the finding on screen.
    Fix(Fix),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Level {
    /// "12 new games": nothing runs until A.
    Offer(usize),
    Running,
    /// The system list could not be read.
    Failed,
    Groups,
    Items(Group),
    Detail(Group),
}

pub struct GameCheckPage {
    pub level: Level,
    pub report: Option<Report>,
    /// (done, total, system) while it runs.
    pub progress: (usize, usize, String),
    /// Selected group row, selected row of the open group.
    pub group_sel: usize,
    pub item_sel: usize,
}

impl GameCheckPage {
    pub fn running() -> Self {
        GameCheckPage { level: Level::Running, report: None, progress: (0, 0, String::new()), group_sel: 0, item_sel: 0 }
    }

    pub fn offer(added: usize) -> Self {
        GameCheckPage { level: Level::Offer(added), ..Self::running() }
    }

    pub fn event(&mut self, e: Event) {
        match e {
            Event::Progress { done, total, system } => self.progress = (done, total, system),
            Event::Done(report) => {
                self.report = Some(report);
                self.level = Level::Groups;
                self.group_sel = 0;
                self.item_sel = 0;
            }
            Event::Failed => self.level = Level::Failed,
        }
    }

    /// The first list's rows. A green result has only its tips.
    pub fn groups(&self) -> Vec<(Group, usize)> {
        self.report.as_ref().map_or(Vec::new(), Report::groups)
    }

    pub fn items(&self, g: Group) -> Vec<&Finding> {
        self.report.as_ref().map_or(Vec::new(), |r| r.in_group(g))
    }

    pub fn finding(&self) -> Option<&Finding> {
        match self.level {
            Level::Detail(g) => self.items(g).get(self.item_sel).copied(),
            _ => None,
        }
    }

    /// A marks what is selected; the App's hints follow this.
    pub fn press(&mut self, b: Button, confirm: Button, cancel: Button) -> Action {
        let cancelled = b == cancel;
        match self.level {
            Level::Offer(_) => {
                if cancelled {
                    return Action::Close;
                }
                if b == confirm {
                    self.level = Level::Running;
                    return Action::Start;
                }
            }
            Level::Running => {
                if cancelled {
                    return Action::Close;
                }
            }
            Level::Failed => {
                if cancelled {
                    return Action::Close;
                }
                if b == Button::North {
                    self.level = Level::Running;
                    return Action::Start;
                }
            }
            Level::Groups => {
                let rows = self.groups();
                if cancelled {
                    return Action::Close;
                }
                if b == Button::North {
                    self.level = Level::Running;
                    self.progress = (0, 0, String::new());
                    return Action::Start;
                }
                if b == confirm {
                    if let Some((g, _)) = rows.get(self.group_sel) {
                        self.item_sel = 0;
                        self.level = Level::Items(*g);
                    }
                    return Action::Nothing;
                }
                self.group_sel = moved(self.group_sel, rows.len(), b);
            }
            Level::Items(g) => {
                let n = self.items(g).len();
                if cancelled {
                    self.level = Level::Groups;
                    return Action::Nothing;
                }
                if b == confirm && n > 0 {
                    self.level = Level::Detail(g);
                    return Action::Nothing;
                }
                self.item_sel = moved(self.item_sel, n, b);
            }
            Level::Detail(g) => {
                if cancelled {
                    self.level = Level::Items(g);
                    return Action::Nothing;
                }
                if b == confirm {
                    if let Some(fix) = self.finding().and_then(|f| f.fix.clone()) {
                        return Action::Fix(fix);
                    }
                }
            }
        }
        Action::Nothing
    }

    /// The fix on screen was taken: the finding is gone, the page goes back to what is left.
    pub fn fixed(&mut self) {
        let Level::Detail(g) = self.level else { return };
        let Some(gone) = self.finding().cloned() else { return };
        if let Some(r) = self.report.as_mut() {
            r.findings.retain(|f| *f != gone);
        }
        let left = self.items(g).len();
        self.item_sel = self.item_sel.min(left.saturating_sub(1));
        self.level = if left > 0 { Level::Items(g) } else { Level::Groups };
        self.group_sel = self.group_sel.min(self.groups().len().saturating_sub(1));
    }

    /// What the UI sounds and the repaint watch.
    pub fn mark(&self) -> (Level, usize, usize, usize) {
        (self.level, self.group_sel, self.item_sel, self.progress.0)
    }
}

fn moved(sel: usize, n: usize, b: Button) -> usize {
    if n == 0 {
        return 0;
    }
    match b {
        Button::Up => (sel + n - 1) % n,
        Button::Down => (sel + 1) % n,
        Button::L1 => sel.saturating_sub(5),
        Button::R1 => (sel + 5).min(n - 1),
        _ => sel,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_check::demo;

    const A: Button = Button::East;
    const B: Button = Button::South;

    #[test]
    fn an_offer_starts_nothing_until_a() {
        let mut p = GameCheckPage::offer(12);
        assert_eq!(p.press(Button::Down, A, B), Action::Nothing);
        assert_eq!(p.level, Level::Offer(12));
        assert_eq!(p.press(B, A, B), Action::Close, "not now");
        let mut p = GameCheckPage::offer(12);
        assert_eq!(p.press(A, A, B), Action::Start);
        assert_eq!(p.level, Level::Running);
    }

    #[test]
    fn groups_open_their_games_and_a_game_its_detail_and_b_comes_back_up() {
        let mut p = GameCheckPage::running();
        assert_eq!(p.press(B, A, B), Action::Close, "B stops a running check");
        p.event(Event::Done(demo(false)));
        assert!(p.groups().len() <= 8, "the first list stays within eight rows");
        let twice = p.groups().iter().position(|(g, _)| *g == Group::Twice).unwrap();
        for _ in 0..twice {
            p.press(Button::Down, A, B);
        }
        p.press(A, A, B);
        assert_eq!(p.level, Level::Items(Group::Twice));
        p.press(A, A, B);
        assert_eq!(p.level, Level::Detail(Group::Twice));
        let fix = p.press(A, A, B);
        assert!(matches!(fix, Action::Fix(Fix::Hide(_))), "{fix:?}");
        let before = p.report.as_ref().unwrap().findings.len();
        p.fixed();
        assert_eq!(p.report.as_ref().unwrap().findings.len(), before - 1);
        assert_eq!(p.level, Level::Groups, "the group was left empty");
        assert!(!p.groups().iter().any(|(g, _)| *g == Group::Twice));
        assert_eq!(p.press(B, A, B), Action::Close);
    }

    #[test]
    fn a_detail_without_a_fix_offers_nothing_and_x_looks_again() {
        let mut p = GameCheckPage::running();
        p.event(Event::Done(demo(false)));
        p.press(A, A, B);
        p.press(A, A, B);
        assert_eq!(p.press(A, A, B), Action::Nothing, "a BIOS is never fetched or fixed for the player");
        p.press(B, A, B);
        p.press(B, A, B);
        assert_eq!(p.press(Button::North, A, B), Action::Start);
        assert_eq!(p.level, Level::Running);
    }
}
