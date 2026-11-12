//! Settings → RetroAchievements in the phone layout: sign in once with the user name and password (typed on the app's own
//! keyboard, which a controller and a finger both use), log out, and what the page says in between. The page's state and
//! its presses are `pocket_ui::achievements_page` (shared with Linux, with its tests); only the token is kept (0600,
//! `achievements::account`), never the password. The games themselves are the player's business (`player_achievements`).
use super::phone::{type_mid, MenuRow, Room};
use super::*;
use pocket_ui::achievements::account::Account;
use pocket_ui::achievements_page::{self as page, AchievementsPage, Tone, Typing, View};

/// What the page says about RetroAchievements on Android, under the status (their German and Spanish lines are in
/// `assets/i18n`).
pub const ABOUT: [&str; 2] = [
    "Earn achievements in the games pocket-ui plays itself.",
    "Your password is sent once and never kept.",
];

pub struct RaUi {
    pub page: AchievementsPage,
    /// The account's name for the Settings row (read when the app starts and when the page changes it, never in the painter).
    pub name: String,
}

impl Default for RaUi {
    fn default() -> Self {
        // Nothing is read from disk before the page is opened (`open_achievements`).
        RaUi { page: AchievementsPage::new(None), name: String::new() }
    }
}

impl Ui {
    /// The Settings row.
    pub(super) fn open_achievements(&mut self) -> Command {
        // A sign-in on its way keeps its page; otherwise the file is the truth (another process may have changed it).
        if !matches!(self.ra.page.view, View::SigningIn) {
            let account = Account::load(&super::super::data_dir());
            self.ra.name = account.as_ref().map(|a| a.shown_name().to_string()).unwrap_or_default();
            self.ra.page = AchievementsPage::new(account);
        }
        self.navigate(Screen::Achievements);
        Command::None
    }
    /// The Settings row's value: the account's name when signed in.
    pub(super) fn achievements_value(&self) -> String {
        self.ra.name.clone()
    }
    /// The host, at start: the account that is signed in (None: nobody).
    pub fn achievements_remember(&mut self, account: Option<&Account>) {
        self.ra.name = account.map(|a| a.shown_name().to_string()).unwrap_or_default();
    }
    pub(super) fn achievements_rows(&self) -> Vec<MenuRow> {
        vec![
            MenuRow { label: tr(self.ra.page.label()), value: self.ra.page.value(), check: None, stepper: false },
            MenuRow { label: tr("Back"), value: String::new(), check: None, stepper: false },
        ]
    }
    /// The on-screen keyboard shows what the page's keyboard holds (the page owns it; the painter and the touch targets
    /// read `Ui::keyboard`).
    fn achievements_keyboard(&mut self) {
        self.keyboard = match &self.ra.page.view {
            View::Keyboard(_, kb) => Some(Keyboard { text: kb.text.clone(), row: kb.row, col: kb.col, shift: kb.shift, symbols: kb.symbols, search: false }),
            _ => None,
        };
    }
    /// A key of the on-screen keyboard was touched: the page's keyboard moves there before the press.
    pub(super) fn achievements_key_at(&mut self, row: usize, col: usize) {
        if let View::Keyboard(_, kb) = &mut self.ra.page.view {
            kb.row = row;
            kb.col = col;
        }
    }
    fn achievements_do(&mut self, action: page::Action) -> Command {
        self.achievements_keyboard();
        self.menu = 0;
        match action {
            page::Action::Nothing => Command::None,
            page::Action::Close => {
                self.navigate(Screen::Settings);
                Command::None
            }
            page::Action::Login { username, password } => Command::RaLogin(username, password),
            page::Action::Logout => Command::RaLogout,
        }
    }
    pub(super) fn achievements_press(&mut self, b: Button) -> Command {
        if matches!(self.ra.page.view, View::Keyboard(..)) {
            let action = self.ra.page.press(b, Button::East, Button::South);
            return self.achievements_do(action);
        }
        match b {
            Button::Up | Button::Left => {
                self.ra.page.press(b, Button::East, Button::South);
                self.move_by(-1);
            }
            Button::Down | Button::Right => {
                self.ra.page.press(b, Button::East, Button::South);
                self.move_by(1);
            }
            Button::Start => self.navigate(Screen::Library),
            Button::East if self.menu == 1 => self.navigate(Screen::Settings),
            Button::East | Button::South => {
                let was = self.menu;
                let action = self.ra.page.press(b, Button::East, Button::South);
                let command = self.achievements_do(action);
                // The keyboard opens on its first key; a "Log out" that only asks keeps its row.
                if b == Button::East && !matches!(self.ra.page.view, View::Keyboard(..)) {
                    self.menu = was;
                }
                return command;
            }
            _ => (),
        }
        Command::None
    }
    /// The host: a sign-in ended (the account is saved already), or why not.
    pub fn achievements_login_done(&mut self, result: Result<Account, String>) {
        if let Ok(account) = &result {
            self.ra.name = account.shown_name().to_string();
        }
        self.ra.page.event(page::Event::LoggedIn(result));
        if self.screen == Screen::Achievements {
            self.menu = 0;
            self.follow = true;
            self.achievements_keyboard();
        }
    }
    /// The host: the key was deleted.
    pub fn achievements_logged_out(&mut self) {
        self.ra.name.clear();
        self.ra.page = AchievementsPage::new(None);
        if self.screen == Screen::Achievements {
            self.menu = 0;
        }
    }
    /// The words above the rows: where the page stands, then what RetroAchievements is here, already translated.
    pub(super) fn achievements_words(&self) -> String {
        let (status, _) = self.ra.page.status();
        let mut words = vec![tr(&status)];
        if self.ra.page.confirm_logout {
            words.push(tr("Press A again to log out. B keeps you signed in."));
        } else {
            words.extend(ABOUT.iter().map(|line| tr(line)));
        }
        words.join("\n")
    }
    pub(super) fn paint_achievements(&mut self, c: &mut Canvas, text: &mut Text, room: Room) {
        if let View::Keyboard(typing, kb) = &self.ra.page.view {
            // One line of what is asked, and the field as typed (a password as dots, the last letter readable).
            let (typing, typed) = (*typing, kb.text.clone());
            let heading = match typing {
                Typing::User => tr("User name"),
                Typing::Password => tr(&format!("Password for {}", self.ra.page.username)),
            };
            let note = match typing {
                Typing::User => tr("Your user name on retroachievements.org."),
                Typing::Password => tr("Sent once to sign in, never kept."),
            };
            let shown = if typing == Typing::Password { page::masked(&typed) } else { typed };
            let size = self.sp(17.0);
            let w = c.w - 2 * room.margin;
            let mut y = room.top + self.d(4.0);
            let heading = text.fit(Face::Title, self.sp(20.0), &heading, w);
            text.draw(c, Face::Title, self.sp(20.0), room.margin, y + self.sp(20.0) as i32, pal().chalk, &heading);
            self.target(Area::new(room.margin, y, w, self.sp(20.0) as i32 + self.d(6.0)), &heading, Action::Label);
            y += self.sp(20.0) as i32 + self.d(14.0);
            let field = Area::new(room.margin, y, w, self.d(52.0));
            c.round_rect(field.x, field.y, field.w, field.h, field.h / 2, pal().pill);
            let tx = field.x + self.d(20.0);
            let fit = text.fit(Face::Body, size, &shown, field.w - self.d(40.0));
            let end = text.draw(c, Face::Body, size, tx, field.y + field.h / 2 + type_mid(size), pal().chalk, &fit);
            c.rect(tx + end + self.d(2.0), field.y + self.d(14.0), self.d(2.0), field.h - self.d(28.0), pal().chalk);
            // The text says what a screen reader hears of the field: the heading and how many characters (never a password).
            let said = if typing == Typing::Password { format!("{heading}: {}", shown.chars().count()) } else { format!("{heading}: {shown}") };
            self.target(field, &said, Action::Label);
            y += field.h + self.d(10.0);
            // The line under the field only where it fits above the keyboard, whole.
            if y + self.sp(14.0) as i32 + self.d(8.0) <= c.h - self.keyboard_height(c) {
                let note = text.fit(Face::Body, self.sp(14.0), &note, w);
                text.draw(c, Face::Body, self.sp(14.0), room.margin, y + self.sp(14.0) as i32, pal().dust, &note);
                self.target(Area::new(room.margin, y, w, self.sp(14.0) as i32 + self.d(8.0)), &note, Action::Label);
            }
            return;
        }
        let (_, tone) = self.ra.page.status();
        let words = self.achievements_words();
        let rows = self.achievements_rows();
        self.paint_words_and_rows(c, text, room, &words, matches!(tone, Tone::Quiet), rows);
    }
}
