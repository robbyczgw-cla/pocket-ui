//! Settings → Saves in the phone layout: one file out with every game's saves, and back in (`pocket_ui::save_transfer`
//! decides what goes where; the host asks Android for the file and copies, `Command::SavesExport` and after). The page
//! always says where it stands in words above its rows, and every step is a plain row: no state is only a toast.
use super::phone::{MenuRow, Room};
use super::*;
use pocket_ui::save_transfer;

/// What the words above the rows say while nothing has happened yet.
pub const ABOUT: &str = "Your saves and save states are kept inside this app, and Android deletes them when the app is removed. Export writes them into one file. Import brings them back.";
pub const WRITING: &str = "Writing the backup. Choose where it goes when Android asks.";
pub const READING: &str = "Reading the backup. Choose the file when Android asks.";
pub const STOPPED: &str = "Nothing was changed.";
pub const FAILED_WRITE: &str = "The backup could not be written. Your saves are as they were. Try another folder.";
pub const FAILED_READ: &str = "The backup could not be read. Your saves are as they were.";
pub const COPIES_ON: &str = "Copies of your saves are kept in pocket-ui saves, inside your games folder. They stay when the app is removed.";
pub const COPIES_OFF: &str = "Copies are off: pocket-ui may not write into your games folder. Choose the folder again and allow changes.";
pub const COPIES_GONE: &str = "The copies' folder can't be reached. Is the card in, and the folder still there?";
pub const COPYING: &str = "Copying your saves.";
pub const NO_ROOM: &str = "Not enough room on this phone for the backup's saves. Nothing was changed.";

#[derive(Clone, Default, PartialEq, Eq, Debug)]
pub enum Step {
    #[default]
    Rest,
    /// Android's picker or the copy is on its way: export (true) or import.
    Busy(bool),
    /// The picked backup has games that already have saves here.
    Ask { games: usize, here: usize },
    /// What happened last; it stays until the page is left.
    Said(String),
}

/// Whether the saves' copies in the games folder work (the launcher looks: `PocketActivity.copyTargets`).
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub enum Copies {
    #[default]
    Unknown,
    On,
    /// No folder chosen, or one that pocket-ui may only read.
    Off,
    Gone,
}

#[derive(Default)]
pub struct SavesUi {
    /// Games with saves on this phone: the Export row's value.
    pub games: usize,
    pub step: Step,
    pub copies: Copies,
    /// What the last copy or look for copies said (conflicts and troubles stay until they are gone).
    pub copy_words: String,
    /// The copies are on their way.
    pub copying: bool,
}

impl Ui {
    /// The Settings row.
    pub(super) fn open_saves(&mut self) -> Command {
        self.saves.games = save_transfer::games_with_saves(&super::super::data_dir());
        if !matches!(self.saves.step, Step::Busy(_) | Step::Ask { .. }) {
            self.saves.step = Step::Rest;
        }
        self.navigate(Screen::Saves);
        Command::None
    }
    /// The launcher: what the copies' state is, and what the last run said (empty: nothing to say).
    pub fn saves_copies(&mut self, copies: Copies, words: &str) {
        self.saves.copies = copies;
        self.saves.copy_words = words.to_string();
        self.saves.copying = false;
    }
    pub fn saves_busy(&self) -> bool {
        matches!(self.saves.step, Step::Busy(_))
    }
    /// The host: a backup was read and some of its games have saves here.
    pub fn saves_ask(&mut self, games: usize, here: usize) {
        self.saves.step = Step::Ask { games, here };
        if self.screen == Screen::Saves {
            self.menu = 0;
            self.follow = true;
        }
    }
    /// The host: it is over, and this is what happened (a sentence in English; the page translates it).
    pub fn saves_said(&mut self, sentence: &str) {
        self.saves.games = save_transfer::games_with_saves(&super::super::data_dir());
        self.saves.step = Step::Said(sentence.to_string());
        if self.screen == Screen::Saves {
            self.menu = 0;
            self.follow = true;
        }
        // The Continue shelf and the cases that stand open follow what came back.
        self.frozen_stale = true;
    }
    pub(super) fn saves_rows(&self) -> Vec<MenuRow> {
        let row = |label: &str, value: String| MenuRow { label: tr(label), value, check: None, stepper: false };
        match self.saves.step {
            Step::Ask { .. } => vec![row("Keep this phone's saves", String::new()), row("Use the backup's saves", String::new()), row("Cancel", String::new())],
            _ => {
                let games = match self.saves.games {
                    0 => tr("Nothing yet"),
                    1 => tr("1 game"),
                    n => tr(&format!("{n} games")),
                };
                let copies = match (self.saves.copying, self.saves.copies) {
                    (true, _) => tr("Copying"),
                    (_, Copies::Unknown) => String::new(),
                    (_, Copies::On) => tr("On"),
                    (_, Copies::Off) => tr("Off"),
                    (_, Copies::Gone) => tr("Folder missing"),
                };
                vec![row("Export saves", games), row("Import saves", String::new()), row("Saved copies", copies), row("Back", String::new())]
            }
        }
    }
    /// The words above the rows, already translated; several sentences are separate lines.
    pub(super) fn saves_words(&self) -> String {
        match &self.saves.step {
            Step::Rest => {
                // What the last run said replaces the state's sentence (which it repeats: no access, no folder); the page
                // keeps the part of its words that fits above its rows.
                let state = match self.saves.copies {
                    Copies::Unknown => "",
                    Copies::On => COPIES_ON,
                    Copies::Off => COPIES_OFF,
                    Copies::Gone => COPIES_GONE,
                };
                let state = if self.saves.copying { COPYING } else if self.saves.copy_words.is_empty() { state } else { "" };
                [tr(ABOUT)]
                    .into_iter()
                    .chain((!state.is_empty()).then(|| tr(state)))
                    .chain(self.saves.copy_words.lines().map(tr))
                    .collect::<Vec<_>>()
                    .join("\n")
            }
            Step::Busy(true) => tr(WRITING),
            Step::Busy(false) => tr(READING),
            Step::Ask { games, here } => format!("{}\n{}", tr(&format!("Games in this backup: {games}")), tr(&format!("Of these, already with saves on this phone: {here}"))),
            Step::Said(sentence) => sentence.lines().map(tr).collect::<Vec<_>>().join("\n"),
        }
    }
    pub(super) fn saves_press(&mut self, b: Button) -> Command {
        let asking = matches!(self.saves.step, Step::Ask { .. });
        match b {
            Button::Up | Button::Left => self.move_by(-1),
            Button::Down | Button::Right => self.move_by(1),
            Button::South if asking => return self.saves_cancel(),
            Button::South | Button::Start => self.navigate(if b == Button::Start { Screen::Library } else { Screen::Settings }),
            Button::East => {
                return match (self.saves.step.clone(), self.menu) {
                    (Step::Ask { .. }, 0) => Command::SavesAnswer(false),
                    (Step::Ask { .. }, 1) => Command::SavesAnswer(true),
                    (Step::Ask { .. }, _) => self.saves_cancel(),
                    (_, 3) => {
                        self.navigate(Screen::Settings);
                        Command::None
                    }
                    (Step::Busy(_), _) => Command::None,
                    // Off or gone: choosing the folder again is what brings the permission (and the folder) back.
                    (_, 2) if matches!(self.saves.copies, Copies::Off | Copies::Gone) => Command::Folder,
                    (_, 2) if self.saves.copying => Command::None,
                    (_, 2) => {
                        self.saves.copying = true;
                        Command::SavesCopyNow
                    }
                    (_, 0) if self.saves.games == 0 => {
                        self.saves.step = Step::Said(save_transfer::NOTHING_YET.into());
                        Command::None
                    }
                    (_, 0) => {
                        self.saves.step = Step::Busy(true);
                        Command::SavesExport
                    }
                    _ => {
                        self.saves.step = Step::Busy(false);
                        Command::SavesImport
                    }
                };
            }
            _ => (),
        }
        Command::None
    }
    fn saves_cancel(&mut self) -> Command {
        self.saves.step = Step::Said(STOPPED.into());
        self.menu = 0;
        Command::SavesCancel
    }
    /// The words, whole, then the rows in the room that is left (they scroll when a big type leaves little).
    pub(super) fn paint_saves(&mut self, c: &mut Canvas, text: &mut Text, room: Room) {
        let words = self.saves_words();
        let rows = self.saves_rows();
        let dim = matches!(self.saves.step, Step::Rest);
        self.paint_words_and_rows(c, text, room, &words, dim, rows);
    }
    /// A page of words above a few rows (Saves, Report a problem): the words whole, in `dust` while `dim` (nothing has
    /// happened yet), then the rows in the room that is left.
    pub(super) fn paint_words_and_rows(&mut self, c: &mut Canvas, text: &mut Text, room: Room, words: &str, dim: bool, rows: Vec<MenuRow>) {
        let w = c.w - 2 * room.margin;
        let mut y = room.top + self.d(4.0);
        let colour = if dim { pal().dust } else { pal().chalk };
        // The words get the room the rows can spare (`words_limit`).
        let (lines, size, step, most) = self.words_lines(text, &room, words, w);
        for line in lines.iter().take(most) {
            text.draw(c, Face::Body, size, room.margin, y + size as i32, colour, line);
            y += step;
        }
        self.cut.extend(lines.iter().skip(most).cloned());
        self.target(Area::new(room.margin, room.top, w, y - room.top), &words.replace('\n', " "), Action::Label);
        self.paint_menu_phone(c, text, Room { top: y + self.d(6.0), ..room }, rows);
    }
}
