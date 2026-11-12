//! Settings → Report a problem in the phone layout: one text with what a bug report needs, shared with Android's share
//! sheet or copied (`pocket_ui::report_phone` writes it, the host hands it on, `Command::ReportShare` and
//! `ReportCopy`). The page always says in words where it stands; every step is a plain row.
use super::phone::{MenuRow, Room};
use super::*;
use pocket_ui::report_phone as report;

#[derive(Clone, Default, PartialEq, Eq, Debug)]
pub enum Step {
    #[default]
    Rest,
    /// The text is being written (logcat is read on a worker) and handed to Android.
    Busy,
    /// What happened last, a sentence in English (the page translates it); it stays until the page is left.
    Said(String),
}

#[derive(Default)]
pub struct ReportUi {
    pub step: Step,
}

impl Ui {
    /// The Settings row.
    pub(super) fn open_report(&mut self) -> Command {
        if self.report.step != Step::Busy {
            self.report.step = Step::Rest;
        }
        self.navigate(Screen::Report);
        Command::None
    }
    pub fn report_busy(&self) -> bool {
        self.report.step == Step::Busy
    }
    /// The host: it is over, and this is what happened (a sentence of `pocket_ui::report_phone`).
    pub fn report_said(&mut self, sentence: &str) {
        self.report.step = Step::Said(sentence.to_string());
        if self.screen == Screen::Report {
            self.menu = 0;
            self.follow = true;
        }
    }
    pub(super) fn report_rows(&self) -> Vec<MenuRow> {
        ["Share the report", "Copy the report", "Back"].into_iter().map(|label| MenuRow { label: tr(label), value: String::new(), check: None, stepper: false }).collect()
    }
    /// The words above the rows, already translated; several sentences are separate lines.
    pub(super) fn report_words(&self) -> String {
        match &self.report.step {
            Step::Rest => format!("{}\n{}", tr(report::ABOUT), tr(report::LEFT_OUT)),
            Step::Busy => tr(report::WRITING),
            Step::Said(sentence) => sentence.lines().map(tr).collect::<Vec<_>>().join("\n"),
        }
    }
    pub(super) fn report_press(&mut self, b: Button) -> Command {
        match b {
            Button::Up | Button::Left => self.move_by(-1),
            Button::Down | Button::Right => self.move_by(1),
            Button::South | Button::Start => self.navigate(if b == Button::Start { Screen::Library } else { Screen::Settings }),
            Button::East => {
                return match (self.menu, self.report.step == Step::Busy) {
                    (2, _) => {
                        self.navigate(Screen::Settings);
                        Command::None
                    }
                    (_, true) => Command::None,
                    (0, false) => {
                        self.report.step = Step::Busy;
                        Command::ReportShare
                    }
                    _ => {
                        self.report.step = Step::Busy;
                        Command::ReportCopy
                    }
                };
            }
            _ => (),
        }
        Command::None
    }
    pub(super) fn paint_report(&mut self, c: &mut Canvas, text: &mut Text, room: Room) {
        let words = self.report_words();
        let rows = self.report_rows();
        let dim = self.report.step == Step::Rest;
        self.paint_words_and_rows(c, text, room, &words, dim, rows);
    }
}
