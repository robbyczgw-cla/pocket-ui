//! Settings > RetroAchievements and the pause menu's Achievements list: the state of the pages and what a press asks
//! for. The App turns an `Action` into work on a thread (the login, `achievements::api`) and reports back with an
//! `Event`; painting is in the binary (`app_achievements.rs`). Nothing here touches the network or the disk.

use std::path::PathBuf;

use crate::achievements::account::Account;
use crate::achievements::snapshot::{Snapshot, State, Summary};
use crate::keyboard::{Keyboard, Outcome};
use crate::pad::Button;

/// What the keyboard is typing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Typing {
    User,
    Password,
}

pub enum View {
    List,
    Keyboard(Typing, Keyboard),
    /// The login request is on its way.
    SigningIn,
}

/// What a press asks the App to do.
#[derive(Debug, PartialEq, Eq)]
pub enum Action {
    Nothing,
    Close,
    /// Sign in once with the password; only the token the server answers is kept.
    Login {
        username: String,
        password: String,
    },
    /// Delete the token.
    Logout,
    /// Hardcore switched (the account says so already; the App keeps it): `achievements::hardcore`.
    Hardcore(bool),
}

/// News from the App's threads.
#[derive(Debug)]
pub enum Event {
    /// The login finished: the account (already saved), or why not.
    LoggedIn(Result<Account, String>),
    /// A game's tally was looked up for its card (`achievements::lookup`); None when that didn't work out.
    Tally { system: String, rom: PathBuf, summary: Option<Summary> },
}

pub struct AchievementsPage {
    pub view: View,
    pub account: Option<Account>,
    /// The user name typed last, offered again after a failed login.
    pub username: String,
    pub error: Option<String>,
    /// The first A on "Log out" asks; the second does it.
    pub confirm_logout: bool,
    /// The Pocket has an address (`wifi::ip`, set by the app before each press). Without one, Sign in says so at once
    /// rather than after the user name and password were typed on the pad (the first-day run, 2026-10-26).
    pub online: bool,
    /// The selected row: 0 Sign in / Log out, 1 Hardcore (signed in only; [`AchievementsPage::step`] moves it).
    pub sel: usize,
    /// The page has the Hardcore row (`achievements::hardcore::OFFERED`; tests and snapshots switch it on).
    pub hardcore_offered: bool,
}

/// The Hardcore row's line beside the hints, off and on.
pub const HARDCORE_OFF: &str = "Off: save states, rewind and cheats work. Unlocks count as softcore.";
pub const HARDCORE_ON: &str = "On: no save states, rewind or cheats. Continue starts from the in-game save.";

/// Sign in pressed without Wi-Fi.
pub const OFFLINE: &str = "Connect to Wi-Fi first to sign in (Settings › Connections › Wi-Fi).";
/// The same on a handheld without Wi-Fi (`device::no_wifi_chip`): RetroAchievements needs the internet, said plainly
/// (also as the status before anything is pressed, while there is no network).
pub const OFFLINE_NO_WIFI: &str = "RetroAchievements needs the internet, and this handheld has no Wi-Fi.";

/// What the page says about RetroAchievements, under the status.
pub const ABOUT: [&str; 3] = if crate::achievements::hardcore::OFFERED {
    [
    "Earn achievements in the games pocket-core plays: unlocks are checked as you play and count on retroachievements.org.",
    "With Hardcore off, save states, rewind and cheats stay usable. With it on they are off, a game starts from its in-game save, and the site counts your unlocks as hardcore. Without Wi-Fi, unlocks are kept and sent the next time a game starts online.",
    "You need a free account from retroachievements.org. Your password is sent once to sign in and is not kept; the Pocket keeps only the sign-in key the site gives back.",
    ]
} else {
    [
    "Earn achievements in the games pocket-core plays: unlocks are checked as you play and count on retroachievements.org.",
    "Save states, rewind and cheats stay usable. Without Wi-Fi, unlocks are kept and sent the next time a game starts online.",
    "You need a free account from retroachievements.org. Your password is sent once to sign in and is not kept; the Pocket keeps only the sign-in key the site gives back.",
    ]
};

/// [`ABOUT`] in Android's handheld view: the games of pocket-ui's own player (there is no pocket-core), its pause menu's save
/// states (no rewind or cheats there), and no "Pocket" (the Odin 2 Mini's page named all three, 13.11.).
pub const ANDROID_ABOUT: [&str; 3] = if crate::achievements::hardcore::OFFERED {
    [
    "Earn achievements in the games pocket-ui's own player plays: unlocks are checked as you play and count on retroachievements.org.",
    "With Hardcore off, save states stay usable. With it on they are off, a game starts from its in-game save, and the site counts your unlocks as hardcore. Without a connection, unlocks are kept and sent the next time a game starts online.",
    "You need a free account from retroachievements.org. Your password is sent once to sign in and is not kept; pocket-ui keeps only the sign-in key the site gives back.",
    ]
} else {
    [
    "Earn achievements in the games pocket-ui's own player plays: unlocks are checked as you play and count on retroachievements.org.",
    "Save states stay usable. Without a connection, unlocks are kept and sent the next time a game starts online.",
    "You need a free account from retroachievements.org. Your password is sent once to sign in and is not kept; pocket-ui keeps only the sign-in key the site gives back.",
    ]
};

/// What the page says on this device ([`ABOUT`], or [`ANDROID_ABOUT`] in Android's profile).
pub fn about_in(android: bool) -> [&'static str; 3] {
    about_on(android, crate::device::no_wifi_chip())
}

/// [`about_in`] on a handheld with (`no_chip` false) or without Wi-Fi.
pub fn about_on(android: bool, no_chip: bool) -> [&'static str; 3] {
    match (android, no_chip) {
        (true, _) => ANDROID_ABOUT,
        // No promise of unlocks sent later over Wi-Fi: the status line says it needs the internet.
        (false, true) => [ABOUT[0], NO_WIFI_ABOUT, ABOUT[2]],
        (false, false) => ABOUT,
    }
}

/// [`ABOUT`]'s middle paragraph on a handheld without Wi-Fi ([`about_on`]).
const NO_WIFI_ABOUT: &str = if crate::achievements::hardcore::OFFERED {
    "With Hardcore off, save states, rewind and cheats stay usable. With it on they are off, a game starts from its in-game save, and the site counts your unlocks as hardcore."
} else {
    "Save states, rewind and cheats stay usable."
};

/// The line under the password on the pad keyboard.
pub fn password_note_in(android: bool) -> &'static str {
    if android { "Sent once to sign in, never kept. pocket-ui keeps only the sign-in key." } else { "Sent once to sign in, never kept. The Pocket keeps only the sign-in key." }
}

/// A typed password as it shows: dots, with the last character readable (it was just typed on a pad).
pub fn masked(text: &str) -> String {
    let n = text.chars().count();
    text.chars().enumerate().map(|(i, c)| if i + 1 == n { c } else { '•' }).collect()
}

/// Overwrites a string's bytes before it is dropped (the password).
pub fn wipe(s: &mut String) {
    let len = s.len();
    s.clear();
    s.extend(std::iter::repeat_n('\0', len));
    s.clear();
}

impl AchievementsPage {
    pub fn new(account: Option<Account>) -> AchievementsPage {
        AchievementsPage {
            username: account.as_ref().map(|a| a.username.clone()).unwrap_or_default(),
            view: View::List,
            account,
            error: None,
            confirm_logout: false,
            online: true,
            sel: 0,
            hardcore_offered: crate::achievements::hardcore::OFFERED,
        }
    }

    /// How many rows the page has: Sign in, or Log out (and Hardcore where it is offered).
    pub fn rows(&self) -> usize {
        if self.signed_in() && self.hardcore_offered { 2 } else { 1 }
    }

    /// Up or Down on the list (`by` rows, wrapping): a question asked on the row left behind is taken back.
    pub fn step(&mut self, by: i32) {
        let n = self.rows() as i32;
        self.sel = (self.sel as i32 + by).rem_euclid(n) as usize;
        self.confirm_logout = false;
    }

    /// Whether Hardcore is on for the account signed in.
    pub fn hardcore(&self) -> bool {
        self.account.as_ref().is_some_and(|a| a.hardcore)
    }

    pub fn signed_in(&self) -> bool {
        self.account.is_some()
    }

    pub fn press(&mut self, b: Button, confirm: Button, cancel: Button) -> Action {
        // The keyboard has its own buttons (A types, B deletes, START is done).
        let typed = match &mut self.view {
            View::Keyboard(typing, kb) => Some((*typing, kb.press(b))),
            _ => None,
        };
        if let Some((typing, outcome)) = typed {
            return match outcome {
                Outcome::Editing => Action::Nothing,
                // B on an empty field: one step back.
                Outcome::Cancelled => {
                    self.view = match typing {
                        Typing::Password => View::Keyboard(
                            Typing::User,
                            Keyboard {
                                text: self.username.clone(),
                                ..Default::default()
                            },
                        ),
                        Typing::User => View::List,
                    };
                    Action::Nothing
                }
                Outcome::Done(mut text) => {
                    let action = self.typed(typing, &text);
                    wipe(&mut text);
                    action
                }
            };
        }
        if matches!(self.view, View::SigningIn) {
            // The request runs out by itself (timeouts); the page waits for it.
            return Action::Nothing;
        }
        if b != confirm {
            self.confirm_logout = false;
        }
        if b == cancel {
            return Action::Close;
        }
        let hardcore_row = self.rows() > 1 && self.sel == 1;
        if b != confirm && !(hardcore_row && matches!(b, Button::Left | Button::Right)) {
            return Action::Nothing;
        }
        self.error = None;
        if hardcore_row {
            let on = !self.hardcore();
            if let Some(a) = self.account.as_mut() {
                a.hardcore = on;
            }
            return Action::Hardcore(on);
        }
        if self.signed_in() {
            if !self.confirm_logout {
                self.confirm_logout = true;
                return Action::Nothing;
            }
            self.confirm_logout = false;
            self.account = None;
            self.sel = 0;
            return Action::Logout;
        }
        if !self.online {
            self.error = Some(crate::device::wifi_or(OFFLINE, OFFLINE_NO_WIFI).into());
            return Action::Nothing;
        }
        self.view = View::Keyboard(
            Typing::User,
            Keyboard {
                text: self.username.clone(),
                ..Default::default()
            },
        );
        Action::Nothing
    }

    /// The keyboard is done with `text`.
    fn typed(&mut self, typing: Typing, text: &str) -> Action {
        match typing {
            Typing::User => {
                let name = text.trim();
                if name.is_empty() {
                    return Action::Nothing;
                }
                self.username = name.to_string();
                self.view = View::Keyboard(Typing::Password, Keyboard::default());
                Action::Nothing
            }
            Typing::Password => {
                if text.is_empty() {
                    return Action::Nothing;
                }
                self.view = View::SigningIn;
                Action::Login {
                    username: self.username.clone(),
                    password: text.to_string(),
                }
            }
        }
    }

    pub fn event(&mut self, e: Event) {
        if let Event::LoggedIn(result) = e {
            if matches!(self.view, View::SigningIn) {
                self.view = View::List;
            }
            match result {
                Ok(account) => {
                    self.error = None;
                    self.username = account.username.clone();
                    self.account = Some(account);
                }
                Err(e) => self.error = Some(e),
            }
        }
    }

    /// The row: "Sign in", or "Log out" with the account at the right.
    pub fn label(&self) -> &'static str {
        if self.signed_in() {
            "Log out"
        } else {
            "Sign in"
        }
    }

    pub fn value(&self) -> String {
        self.account.as_ref().map(|a| a.shown_name().to_string()).unwrap_or_default()
    }

    /// The Hardcore row's value.
    pub fn hardcore_value(&self) -> &'static str {
        if self.hardcore() { "On" } else { "Off" }
    }

    /// The line next to the hints.
    pub fn describe(&self) -> &'static str {
        if self.rows() > 1 && self.sel == 1 {
            return if self.hardcore() { HARDCORE_ON } else { HARDCORE_OFF };
        }
        match (self.signed_in(), self.confirm_logout) {
            (true, true) => "Press A again to log out. B keeps you signed in.",
            (true, false) => "Removes the sign-in key from the Pocket.",
            (false, _) => "With your retroachievements.org account.",
        }
    }

    /// The status under the list: the text and whether it is good news, waiting or an error.
    pub fn status(&self) -> (String, Tone) {
        if let Some(e) = &self.error {
            return (e.clone(), Tone::Alert);
        }
        match (&self.view, &self.account) {
            (View::SigningIn, _) => ("Signing in".into(), Tone::Busy),
            (_, Some(a)) => (format!("Signed in as {}", a.shown_name()), Tone::Good),
            (_, None) if !self.online && crate::device::no_wifi_chip() => (OFFLINE_NO_WIFI.into(), Tone::Quiet),
            (_, None) => ("Not signed in".into(), Tone::Quiet),
        }
    }
}

/// How the status line is coloured.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Good,
    Quiet,
    Busy,
    Alert,
}

/// The pause menu's Achievements page: (name, value, explanation) per row, from pocket-core's snapshot. Locked first
/// (with their progress when the set measures one), then unlocked.
pub fn menu_rows(snap: &Snapshot) -> Vec<(String, String, String)> {
    if snap.state != State::Ready || snap.achievements.is_empty() {
        let (name, desc) = match snap.state {
            State::Loading => ("Looking up the game", snap.state_text()),
            State::NoSet => ("No achievements", snap.state_text()),
            State::Offline => ("Offline", snap.state_text()),
            State::SignedOut => ("Sign in again", snap.state_text()),
            State::Ready => ("No achievements", snap.state_text()),
            State::Failed => ("Not available", snap.state_text()),
        };
        return vec![(name.to_string(), String::new(), desc)];
    }
    snap.achievements
        .iter()
        .map(|a| {
            let points = match a.points {
                1 => "1 point".to_string(),
                p => format!("{p} points"),
            };
            let value = if a.unlocked {
                "Unlocked".to_string()
            } else if a.unsupported {
                "Not supported".to_string()
            } else if !a.progress.is_empty() {
                a.progress.clone()
            } else {
                points.clone()
            };
            let state = if a.unlocked {
                "Unlocked."
            } else if a.unsupported {
                "This version can't check this one."
            } else {
                "Locked."
            };
            let desc = format!("{}\n{points} · {state}", a.description);
            (a.title.clone(), value, desc)
        })
        .collect()
}

/// The same list as lines of text, for a page that scrolls plain lines (the Android player's pause menu): the heading
/// with the totals, then per achievement its name and value and its description, a blank line between. The words the
/// site provides (names, descriptions) stay as they are; ours go through the translation.
pub fn list_lines(snap: &Snapshot) -> Vec<String> {
    let tr = crate::i18n::tr;
    let mut lines = vec![tr(&snap.headline())];
    if !snap.user.is_empty() {
        lines.push(snap.user.clone());
    }
    lines.push(String::new());
    for (name, value, explanation) in menu_rows(snap) {
        let name = if snap.state == State::Ready && !snap.achievements.is_empty() { name } else { tr(&name) };
        lines.push(if value.is_empty() { name } else { format!("{name} · {}", tr(&value)) });
        let ready = snap.state == State::Ready && !snap.achievements.is_empty();
        for (i, part) in explanation.split('\n').enumerate() {
            // A set's description is the site's own words; the line under it ("5 points · Locked.") is ours.
            lines.push(if ready && i == 0 { part.to_string() } else { tr(part) });
        }
        lines.push(String::new());
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A handheld without Wi-Fi (15.11.): the page said "Without Wi-Fi, unlocks are kept and sent the next time a game
    /// starts online" under "RetroAchievements needs the internet, and this handheld has no Wi-Fi." It says what it
    /// needs, once.
    #[test]
    fn without_a_wifi_chip_the_page_does_not_promise_unlocks_sent_later_over_wifi() {
        for p in about_on(false, true) {
            assert!(!p.contains("Wi-Fi"), "{p}");
        }
        assert!(about_on(false, false)[1].contains("Without Wi-Fi"), "with a chip the usual words");
        assert!(about_on(false, true)[1].contains("Save states"), "the rest of the paragraph stays");
    }
    use crate::achievements::snapshot::Entry;

    const CONFIRM: Button = Button::East;
    const CANCEL: Button = Button::South;

    fn press(p: &mut AchievementsPage, b: Button) -> Action {
        p.press(b, CONFIRM, CANCEL)
    }

    fn type_text(p: &mut AchievementsPage, text: &str) {
        if let View::Keyboard(_, kb) = &mut p.view {
            kb.text = text.into();
        } else {
            panic!("no keyboard");
        }
    }

    #[test]
    fn signing_in_without_wifi_says_so_before_any_typing() {
        let mut p = AchievementsPage::new(None);
        p.online = false;
        assert_eq!(press(&mut p, CONFIRM), Action::Nothing);
        assert!(matches!(p.view, View::List), "no keyboard: the name and password would go nowhere");
        assert_eq!(p.status(), (OFFLINE.into(), Tone::Alert));
        assert!(crate::i18n::german().translate(OFFLINE).is_some(), "OFFLINE has a German line");
        // Online again: A opens the keyboard as before.
        p.online = true;
        assert_eq!(press(&mut p, CONFIRM), Action::Nothing);
        assert!(matches!(&p.view, View::Keyboard(Typing::User, _)));
    }

    #[test]
    fn signing_in_asks_name_then_password_and_sends_them_once() {
        let mut p = AchievementsPage::new(None);
        assert_eq!(p.label(), "Sign in");
        assert_eq!(p.status(), ("Not signed in".into(), Tone::Quiet));
        assert_eq!(press(&mut p, CONFIRM), Action::Nothing);
        assert!(matches!(&p.view, View::Keyboard(Typing::User, kb) if kb.text.is_empty()));
        type_text(&mut p, " Alex ");
        assert_eq!(press(&mut p, Button::Start), Action::Nothing);
        assert!(matches!(&p.view, View::Keyboard(Typing::Password, kb) if kb.text.is_empty()));
        // An empty password isn't sent.
        assert_eq!(press(&mut p, Button::Start), Action::Nothing);
        type_text(&mut p, "Pa$$word");
        assert_eq!(
            press(&mut p, Button::Start),
            Action::Login {
                username: "Alex".into(),
                password: "Pa$$word".into()
            }
        );
        assert!(matches!(p.view, View::SigningIn));
        assert_eq!(p.status().1, Tone::Busy);
        assert_eq!(press(&mut p, CANCEL), Action::Nothing, "waits for the answer");
        p.event(Event::LoggedIn(Ok(Account {
            username: "Alex".into(),
            token: "Tk".into(),
            display_name: String::new(),
            hardcore: false,
        })));
        assert!(matches!(p.view, View::List));
        assert_eq!((p.label(), p.value()), ("Log out", "Alex".to_string()));
        assert_eq!(p.status(), ("Signed in as Alex".into(), Tone::Good));
    }

    #[test]
    fn a_failed_login_says_why_and_keeps_the_name() {
        let mut p = AchievementsPage::new(None);
        press(&mut p, CONFIRM);
        type_text(&mut p, "Alex");
        press(&mut p, Button::Start);
        type_text(&mut p, "wrong");
        press(&mut p, Button::Start);
        p.event(Event::LoggedIn(Err("Invalid user/password combination.".into())));
        assert_eq!(p.status(), ("Invalid user/password combination.".into(), Tone::Alert));
        assert!(!p.signed_in());
        press(&mut p, CONFIRM);
        assert!(matches!(&p.view, View::Keyboard(Typing::User, kb) if kb.text == "Alex"), "offered again");
        assert_eq!(p.error, None);
    }

    #[test]
    fn b_steps_back_through_the_keyboards() {
        let mut p = AchievementsPage::new(None);
        press(&mut p, CONFIRM);
        type_text(&mut p, "Alex");
        press(&mut p, Button::Start);
        // B on the empty password goes back to the name, B on an empty name to the list.
        press(&mut p, CANCEL);
        assert!(matches!(&p.view, View::Keyboard(Typing::User, kb) if kb.text == "Alex"));
        type_text(&mut p, "");
        press(&mut p, CANCEL);
        assert!(matches!(p.view, View::List));
        assert_eq!(press(&mut p, CANCEL), Action::Close);
    }

    /// Hardcore is one row under Log out: A (or Left/Right) switches it, the account keeps it, and the line says what
    /// it changes. Signed out there is no such row.
    #[test]
    fn the_hardcore_row_switches_and_says_what_it_does() {
        let mut p = AchievementsPage::new(None);
        assert_eq!(p.rows(), 1);
        p.step(1);
        assert_eq!(p.sel, 0, "signed out: one row");
        let mut p = AchievementsPage::new(Some(Account { username: "Alex".into(), token: "Tk".into(), display_name: String::new(), hardcore: false }));
        assert_eq!(p.rows(), if crate::achievements::hardcore::OFFERED { 2 } else { 1 }, "the row only where pocket-ui offers Hardcore");
        p.hardcore_offered = true;
        assert_eq!(p.rows(), 2);
        p.step(1);
        assert_eq!((p.sel, p.hardcore_value(), p.describe()), (1, "Off", HARDCORE_OFF));
        assert_eq!(press(&mut p, CONFIRM), Action::Hardcore(true));
        assert_eq!((p.hardcore_value(), p.describe()), ("On", HARDCORE_ON));
        assert!(p.account.as_ref().unwrap().hardcore);
        assert_eq!(press(&mut p, Button::Right), Action::Hardcore(false));
        assert_eq!(press(&mut p, Button::Up), Action::Nothing, "Up is the App's to move the row");
        // Back on Log out, A asks as before (never logs out from the Hardcore row).
        p.step(-1);
        assert_eq!(press(&mut p, CONFIRM), Action::Nothing);
        assert!(p.confirm_logout);
        p.step(1);
        assert!(!p.confirm_logout, "the question goes with the row");
        for line in [HARDCORE_OFF, HARDCORE_ON] {
            assert!(crate::i18n::german().translate(line).is_some(), "{line} has a German line");
        }
    }

    #[test]
    fn logging_out_asks_first() {
        let mut p = AchievementsPage::new(Some(Account {
            username: "Alex".into(),
            token: "Tk".into(),
            display_name: "Rob".into(),
            hardcore: false,
        }));
        assert_eq!(p.value(), "Rob");
        assert_eq!(press(&mut p, CONFIRM), Action::Nothing);
        assert!(p.describe().contains("again"));
        assert_eq!(press(&mut p, Button::Down), Action::Nothing);
        assert!(!p.confirm_logout, "any other button keeps you in");
        press(&mut p, CONFIRM);
        assert_eq!(press(&mut p, CONFIRM), Action::Logout);
        assert!(!p.signed_in());
        assert_eq!(p.label(), "Sign in");
    }

    #[test]
    fn the_password_shows_as_dots_and_is_wiped() {
        assert_eq!(masked(""), "");
        assert_eq!(masked("a"), "a");
        assert_eq!(masked("secret"), "•••••t");
        let mut s = String::from("secret");
        wipe(&mut s);
        assert!(s.is_empty());
    }

    #[test]
    fn the_list_reads_as_lines_for_a_page_that_scrolls_text() {
        let snap = Snapshot {
            state: State::Ready,
            total: 2,
            unlocked: 1,
            points: 15,
            points_unlocked: 10,
            user: "Alex".into(),
            achievements: vec![
                Entry { id: 1, title: "Collect coins".into(), description: "Collect 100 coins.".into(), points: 5, progress: "37/100".into(), ..Default::default() },
                Entry { id: 3, title: "First steps".into(), description: "Start.".into(), points: 10, unlocked: true, ..Default::default() },
            ],
            ..Default::default()
        };
        let lines = list_lines(&snap);
        assert_eq!(lines[0], "1 of 2 unlocked · 10 of 15 points");
        assert_eq!(lines[1], "Alex");
        let at = lines.iter().position(|l| l == "Collect coins · 37/100").expect("locked first, with its progress");
        assert_eq!(lines[at + 1], "Collect 100 coins.");
        assert!(lines[at + 2].starts_with("5 points"), "{lines:?}");
        assert!(lines.iter().any(|l| l == "First steps · Unlocked"));
        // No set yet: one honest line of what is going on, never an empty page.
        let waiting = list_lines(&Snapshot::default());
        assert!(waiting.iter().filter(|l| !l.is_empty()).count() >= 2, "{waiting:?}");
        assert!(waiting.iter().any(|l| l.contains("Looking up")), "{waiting:?}");
    }

    #[test]
    fn the_menu_lists_locked_then_unlocked_with_progress() {
        let snap = Snapshot {
            state: State::Ready,
            total: 3,
            unlocked: 1,
            achievements: vec![
                Entry {
                    id: 1,
                    title: "Collect coins".into(),
                    description: "Collect 100 coins.".into(),
                    points: 5,
                    progress: "37/100".into(),
                    ..Default::default()
                },
                Entry {
                    id: 2,
                    title: "Beat the boss".into(),
                    description: "Beat it.".into(),
                    points: 1,
                    ..Default::default()
                },
                Entry {
                    id: 3,
                    title: "First steps".into(),
                    description: "Start.".into(),
                    points: 10,
                    unlocked: true,
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        let rows = menu_rows(&snap);
        assert_eq!(rows.iter().map(|r| r.1.as_str()).collect::<Vec<_>>(), vec!["37/100", "1 point", "Unlocked"]);
        assert_eq!(rows[0].2, "Collect 100 coins.\n5 points · Locked.");
        assert!(rows[2].2.ends_with("Unlocked."));
        for state in [State::Loading, State::NoSet, State::Offline, State::SignedOut, State::Failed] {
            let rows = menu_rows(&Snapshot { state, ..Default::default() });
            assert_eq!(rows.len(), 1, "{state:?}");
            assert!(!rows[0].0.is_empty() && !rows[0].2.is_empty());
        }
    }
}
