//! Settings → Help in the phone layout: the few ways a player needs most, each a short answer in plain words. A topic is a
//! row; A shows its answer above the rows, the way Licences shows a core. Every text is a constant with a German and a Spanish
//! line (`assets/i18n`), and the page fits the Retroid and Odin screens whole (`android_help_*` tests).
use super::phone::{MenuRow, Room};
use super::*;

pub const ABOUT: &str = "Short answers to the usual questions. Choose a topic.";

/// The topics, in the order the page lists them: the row (also the answer's heading) and the answer.
pub const TOPICS: [(&str, &str); 8] = [
    (
        "Choose your games folder",
        "Settings › Choose your ROM folder: pick the folder with one folder per system (gba, snes, psx). Games stay where they are; nothing is copied.",
    ),
    (
        "Load more systems",
        "PSP, DS, Nintendo 64 and Dreamcast are optional. Settings › Cozy extras › Download now loads one over Wi-Fi; B cancels. All other systems are already in the app.",
    ),
    (
        "Pause menu",
        "In a game press Back or your handheld's Menu button. Continue, Save, Load, Options, Controls and Save & quit are in it. With a controller, A chooses and B continues.",
    ),
    (
        "Save and keep playing",
        "Save & quit keeps your place and the game's own saves. Next time the game is under Continue and starts where you stopped. Save and Load have eight slots.",
    ),
    (
        "Copy of your saves",
        "When a game ends, pocket-ui copies its saves into the folder pocket-ui saves in your games folder. It stays if the app is removed. Check: Settings › Save backup.",
    ),
    (
        "Plays with",
        "Settings › Plays with: for each system, A switches between pocket-ui's own player and an emulator app you installed (RetroArch, PPSSPP, DuckStation). Only installed apps appear.",
    ),
    (
        "Leave another emulator app",
        "PS2, GameCube, Wii, 3DS and Xbox games open in their own app. Leave through its menu: ARMSX2 Close, Dolphin Exit Emulation, Azahar Close Game and OK, hakuX Exit.",
    ),
    (
        "Report a problem",
        "Settings › Report a problem: Share sends one text with your device and logs to a chat or mail, Copy puts it on the clipboard for a GitHub issue. Nothing goes out by itself.",
    ),
];

#[derive(Default)]
pub struct HelpUi {
    /// The topic whose answer is shown above the rows.
    pub open: Option<usize>,
}

impl Ui {
    /// The Settings row.
    pub(super) fn open_help(&mut self) -> Command {
        self.help.open = None;
        self.navigate(Screen::Help);
        Command::None
    }
    pub(super) fn help_rows(&self) -> Vec<MenuRow> {
        let row = |label: String| MenuRow { label, value: String::new(), check: None, stepper: false };
        TOPICS.iter().map(|(title, _)| row(tr(title))).chain([row(tr("Back"))]).collect()
    }
    /// The words above the rows, already translated: the chosen topic's answer, else what the page is.
    pub(super) fn help_words(&self) -> String {
        match self.help.open.and_then(|i| TOPICS.get(i)) {
            Some((title, answer)) => format!("{}\n{}", tr(title), tr(answer)),
            None => tr(ABOUT),
        }
    }
    pub(super) fn help_press(&mut self, b: Button) -> Command {
        match b {
            Button::Up | Button::Left => self.move_by(-1),
            Button::Down | Button::Right => self.move_by(1),
            Button::South | Button::Start => self.navigate(if b == Button::Start { Screen::Library } else { Screen::Settings }),
            Button::East => match self.menu {
                m if m < TOPICS.len() => self.help.open = Some(m),
                _ => self.navigate(Screen::Settings),
            },
            _ => (),
        }
        Command::None
    }
    pub(super) fn paint_help(&mut self, c: &mut Canvas, text: &mut Text, room: Room) {
        let words = self.help_words();
        let rows = self.help_rows();
        let dim = self.help.open.is_none();
        self.paint_words_and_rows(c, text, room, &words, dim, rows);
    }
}
