//! Part of the pocket-ui binary: the App's side of `pocket_ui::achievements` and `pocket_ui::achievements_page`.
//! Settings > RetroAchievements (sign in with the gamepad keyboard, log out), "12 of 40" on the game card and the
//! console page, and the pause menu's Achievements list. The login and the card's lookup run on threads; the UI thread
//! only reads small files and paints.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use pocket_ui::achievements::account::Account;
use pocket_ui::achievements::snapshot::{self, Snapshot, Summary, SNAPSHOT_FILE};
use pocket_ui::achievements::{api, http, lookup};
use pocket_ui::achievements_page::{self as page, AchievementsPage, Action, Event, Tone, Typing, View};
use pocket_ui::gfx::Canvas;
use pocket_ui::keyboard::Keyboard;
use pocket_ui::pad::Button;
use pocket_ui::palette::*;
use pocket_ui::text::Face;
use pocket_ui::touch::Area;

use super::{App, SettingsPage, UserEvent, CANCEL, CONFIRM, NU_BTN_PAD, NU_PAD, NU_PILL, NU_SMALL};

/// The name in the Settings list, to come back to.
pub(crate) const SETTINGS_NAME: &str = "RetroAchievements";

/// What the App keeps: the open page, the account's name for the Settings list, the card lookups under way.
pub(crate) struct AchievementsUi {
    page: Option<AchievementsPage>,
    signed_in: Option<String>,
    looking_up: HashSet<PathBuf>,
}

impl AchievementsUi {
    pub fn load(data_dir: &Path) -> AchievementsUi {
        AchievementsUi {
            page: None,
            signed_in: Account::load(data_dir).map(|a| a.shown_name().to_string()),
            looking_up: HashSet::new(),
        }
    }
}

fn view_kind(v: &View) -> u8 {
    match v {
        View::List => 0,
        View::Keyboard(Typing::User, _) => 1,
        View::Keyboard(Typing::Password, _) => 2,
        View::SigningIn => 3,
    }
}

fn now_secs() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

impl App {
    /// Text for the Settings list: the account's name, or "Off".
    pub(crate) fn achievements_list_value(&self) -> String {
        format!("{} >", self.achievements.signed_in.as_deref().unwrap_or("Off"))
    }

    pub(crate) fn open_achievements(&mut self) -> Option<SettingsPage> {
        self.achievements.page = Some(AchievementsPage::new(Account::load(&self.data_dir)));
        Some(SettingsPage::Achievements)
    }

    pub(crate) fn achievements_input(&mut self, b: Button) -> Option<SettingsPage> {
        let back = Some(SettingsPage::List(super::settings_row(SETTINGS_NAME)));
        let Some(page) = self.achievements.page.as_mut() else { return back };
        // From memory, never waits (`wifi::ip`).
        page.online = pocket_ui::wifi::ip().is_some();
        match page.press(b, CONFIRM, CANCEL) {
            Action::Nothing => {}
            Action::Close => {
                self.achievements.page = None;
                return back;
            }
            Action::Login { username, mut password } => {
                let (proxy, data) = (self.proxy.clone(), self.data_dir.clone());
                let spawned = std::thread::Builder::new().name("ra-login".into()).spawn(move || {
                    // One request with the password; the token it brings back is saved, the password forgotten.
                    let curl = http::Curl::new(http::user_agent(None));
                    let result = api::login(&curl, &username, &password);
                    page::wipe(&mut password);
                    let result = result
                        .map_err(|e| e.to_string())
                        .and_then(|a| a.save(&data).map(|_| a).map_err(|e| format!("Couldn't keep the sign-in: {e}")));
                    match &result {
                        Ok(a) => println!("achievements: signed in as {}", a.username),
                        Err(e) => println!("achievements: sign-in failed: {e}"),
                    }
                    let _ = proxy.send_event(UserEvent::Achievements(Event::LoggedIn(result)));
                });
                if spawned.is_err() {
                    page.event(Event::LoggedIn(Err("Couldn't start the sign-in".into())));
                }
            }
            Action::Logout => {
                if !Account::remove(&self.data_dir) {
                    pocket_ui::ui_sound::refuse();
                }
                println!("achievements: signed out");
                self.achievements.signed_in = None;
            }
        }
        Some(SettingsPage::Achievements)
    }

    pub(crate) fn achievements_event(&mut self, e: Event) {
        match e {
            Event::LoggedIn(result) => {
                if let Ok(a) = &result {
                    self.achievements.signed_in = Some(a.shown_name().to_string());
                }
                if let Some(p) = self.achievements.page.as_mut() {
                    p.event(Event::LoggedIn(result));
                }
            }
            Event::Tally { system, rom, summary } => {
                self.achievements.looking_up.remove(&rom);
                if let Some(open) = self.card.as_mut().filter(|o| o.path == rom && o.system == system) {
                    if let Some(summary) = summary {
                        open.card.data.achievements = summary.text();
                    }
                }
            }
        }
    }

    /// Where the page stands: a change is what the menu sounds and haptics answer.
    pub(crate) fn achievements_row(&self) -> Option<(u8, bool, bool)> {
        self.achievements.page.as_ref().map(|p| (view_kind(&p.view), p.signed_in(), p.confirm_logout))
    }

    /// "12 of 40" for a game's card: what is known now, and a lookup in the background when that is old or missing
    /// (signed in only; `lookup::wanted` keeps it to once an hour per game).
    pub(crate) fn card_achievements(&mut self, system: &str, path: &Path) -> Option<String> {
        pocket_ui::achievements::consoles::console_id(system)?;
        let known = Summary::load(&self.data_dir, system, path);
        if self.achievements.signed_in.is_some() && lookup::wanted(known.as_ref(), now_secs()) && self.achievements.looking_up.insert(path.to_path_buf()) {
            if let Some(account) = Account::load(&self.data_dir) {
                let (proxy, data, system, rom) = (self.proxy.clone(), self.data_dir.clone(), system.to_string(), path.to_path_buf());
                let _ = std::thread::Builder::new().name("ra-lookup".into()).spawn(move || {
                    let curl = http::Curl::new(http::user_agent(None));
                    let summary = lookup::lookup(&curl, &data, &account, &system, &rom, now_secs());
                    let _ = proxy.send_event(UserEvent::Achievements(Event::Tally { system, rom, summary }));
                });
            }
        }
        known.and_then(|s| s.text())
    }

    /// "35 of 120 achievements in 3 games" for a console page, from the games' tallies.
    pub(crate) fn console_achievements(&self, s: usize) -> Option<String> {
        let sys = self.systems.get(s)?;
        pocket_ui::achievements::consoles::console_id(&sys.name)?;
        snapshot::console_tally(&self.data_dir, &sys.name, sys.games.iter().map(|g| g.path.as_path())).map(snapshot::console_text)
    }

    /// The running game's achievements for the pause menu, when pocket-core has written them for this very game.
    pub(crate) fn menu_achievements(rom: &Path) -> Option<Snapshot> {
        Snapshot::read(Path::new(SNAPSHOT_FILE), rom)
    }

    /// `--snapshot achievements[:in|:user|:password|:signing|:error|:confirm]`: the page with made-up data.
    pub(crate) fn achievements_snapshot(&mut self, state: &str) -> bool {
        let Some(rest) = state.strip_prefix("achievements").filter(|r| r.is_empty() || r.starts_with(':')) else {
            return false;
        };
        let mode = rest.trim_start_matches(':');
        let account = Account {
            username: "RetroFan".into(),
            token: "x".into(),
            display_name: String::new(),
        };
        let signed_in = matches!(mode, "in" | "confirm");
        let mut p = AchievementsPage::new(signed_in.then_some(account));
        match mode {
            "user" => {
                p.press(CONFIRM, CONFIRM, CANCEL);
                if let View::Keyboard(_, kb) = &mut p.view {
                    kb.text = "RetroFan".into();
                }
            }
            "password" | "signing" => {
                p.press(CONFIRM, CONFIRM, CANCEL);
                if let View::Keyboard(_, kb) = &mut p.view {
                    kb.text = "RetroFan".into();
                }
                p.press(Button::Start, CONFIRM, CANCEL);
                if let View::Keyboard(_, kb) = &mut p.view {
                    kb.text = "hunter2".into();
                }
                if mode == "signing" {
                    p.press(Button::Start, CONFIRM, CANCEL);
                }
            }
            "offline" => {
                p.online = false;
                p.press(CONFIRM, CONFIRM, CANCEL);
            }
            "error" => p.event(Event::LoggedIn(Err("Invalid user/password combination. Please try again.".into()))),
            "confirm" => {
                p.press(CONFIRM, CONFIRM, CANCEL);
            }
            _ => {}
        }
        self.achievements.signed_in = p.account.as_ref().map(|a| a.shown_name().to_string());
        self.achievements.page = Some(p);
        self.settings_page = Some(SettingsPage::Achievements);
        true
    }

    pub(crate) fn paint_achievements(&mut self, c: &mut Canvas) {
        let Some(page) = self.achievements.page.take() else { return };
        c.clear(pal().void);
        match &page.view {
            View::Keyboard(typing, kb) => self.paint_achievements_keyboard(c, *typing, kb, &page.username),
            _ => self.paint_achievements_page(c, &page),
        }
        self.achievements.page = Some(page);
    }

    fn paint_achievements_keyboard(&mut self, c: &mut Canvas, typing: Typing, kb: &Keyboard, username: &str) {
        let heading = match typing {
            Typing::User => "User name".to_string(),
            Typing::Password => format!("Password for {username}"),
        };
        let heading = self.text.fit(Face::Title, 50, &heading, c.w - 2 * MARGIN);
        self.text.draw(c, Face::Title, 50, MARGIN, 110, pal().chalk, &heading);
        // The password shows as dots, the character just typed stays readable.
        let shown = Keyboard {
            text: if typing == Typing::Password {
                page::masked(&kb.text)
            } else {
                kb.text.clone()
            },
            row: kb.row,
            col: kb.col,
            shift: kb.shift,
            symbols: kb.symbols,
            search: kb.search,
        };
        let done = if typing == Typing::User { "Next" } else { "Sign in" };
        pocket_ui::setup::paint_keyboard_with(c, &mut self.text, &shown, done);
        let note = match typing {
            Typing::User => "Your user name on retroachievements.org.",
            Typing::Password => "Sent once to sign in, never kept. The Pocket keeps only the sign-in key.",
        };
        let note = self.text.fit(Face::Body, 24, note, c.w - 2 * MARGIN);
        self.text.draw(c, Face::Body, 24, MARGIN, 790, pal().dust, &note);
        pocket_ui::setup::hints(
            c,
            &mut self.text,
            &[("A", "Type"), ("B", "Delete"), ("X", "Shift"), ("Y", "Symbols"), ("START", done)],
        );
    }

    fn paint_achievements_page(&mut self, c: &mut Canvas, page: &AchievementsPage) {
        let (pad, row, font) = (NU_PAD, pocket_ui::type_scale::row(66, 36), 36u32);
        self.sheet_header(c, SETTINGS_NAME);
        let (top, right) = (pad + NU_PILL + 12, c.w - pad);
        // One row: Sign in, or Log out with the account at the right.
        let y = top;
        self.hits.row(Area::new(pad, y, right - pad, row), 0);
        let base = y + row / 2 + pocket_ui::type_scale::mid(font);
        let name = page.label();
        let w = self.text.width(Face::Body, font, name) + 2 * NU_BTN_PAD;
        c.round_rect(pad, y + 4, right - pad, row - 8, (row - 8) / 2, pal().band);
        c.round_rect(pad, y + 4, w, row - 8, (row - 8) / 2, pal().select);
        self.text.draw(c, Face::Body, font, pad + NU_BTN_PAD, base, pal().ink, name);
        let value = self.text.fit(Face::Body, font, &page.value(), right - pad - 2 * NU_BTN_PAD - w - 40);
        self.text.draw_right(c, Face::Body, font, right - NU_BTN_PAD, base, pal().ink, &value);
        self.hits.list(0, 1, 0.0, row as f32);

        // The status, then what RetroAchievements is and how it works here.
        let (left, width) = (pad + NU_BTN_PAD, c.w - 2 * (pad + NU_BTN_PAD));
        let mut y = top + row + 44;
        let (status, tone) = page.status();
        let (status, color) = match tone {
            Tone::Good => (status, pal().chalk),
            Tone::Quiet => (status, pal().dust),
            Tone::Busy => (format!("{status}…"), pal().dust),
            Tone::Alert => (status, pal().alert),
        };
        // The state in a panel, as Performance & fan's readout (`kit::panel`).
        let lines = self.text.wrap(Face::Title, 44, &status, width);
        let shown = lines.len().clamp(1, 2) as i32;
        let panel_top = y - 30;
        let panel_h = 30 + 48 + (shown - 1) * pocket_ui::type_scale::line(44, 8) + 24 + pocket_ui::type_scale::grow(44);
        pocket_ui::kit::panel(c, pad, panel_top, c.w - 2 * pad, panel_h);
        self.text.draw(c, Face::Body, 24, left, y, pal().dust, "Status");
        y += 48;
        for line in lines.iter().take(2) {
            self.text.draw(c, Face::Title, 44, left, y, color, line);
            y += pocket_ui::type_scale::line(44, 8);
        }
        y = y.max(panel_top + panel_h + 40);
        for p in page::ABOUT {
            for line in self.text.wrap(Face::Body, 28, p, width) {
                self.text.draw(c, Face::Body, 28, left, y, pal().dust, &line);
                y += pocket_ui::type_scale::line(28, 10);
            }
            y += 14;
        }

        let action = match (&page.view, page.signed_in()) {
            (View::SigningIn, _) => None,
            (_, true) => Some("Log out"),
            (_, false) => Some("Sign in"),
        };
        let hints: Vec<(&str, &str)> = std::iter::once(("B", "Back")).chain(action.map(|a| ("A", a))).collect();
        let hints_at = self.nu_hints(c, &hints);
        let desc = self.text.fit(Face::Body, NU_SMALL, page.describe(), hints_at - 24 - left);
        self.text.draw(
            c,
            Face::Body,
            NU_SMALL,
            left,
            c.h - pad - NU_PILL / 2 + pocket_ui::type_scale::mid(NU_SMALL),
            pal().text,
            &desc,
        );
    }
}

/// A made-up game's list for `--snapshot achievements-menu` and `achievements-list:<row>`.
pub(crate) fn demo_snapshot() -> Snapshot {
    use pocket_ui::achievements::snapshot::{Entry, State};
    let e = |id: u32, title: &str, description: &str, points: u32, unlocked: bool, progress: &str| Entry {
        id,
        title: title.into(),
        description: description.into(),
        points,
        unlocked,
        progress: progress.into(),
        percent: 0.0,
        unsupported: false,
    };
    let achievements = vec![
        e(4, "Town planner", "Build a town of 1,000 people.", 10, false, "612/1000"),
        e(5, "Power for all", "Connect every building to a power plant.", 5, false, ""),
        e(6, "Clean air", "Reach a year without pollution warnings.", 25, false, ""),
        e(7, "Metropolis", "Grow the city to 10,000 people.", 50, false, "612/10000"),
        e(1, "First street", "Build your first road.", 1, true, ""),
        e(2, "Open for business", "Zone a commercial area.", 2, true, ""),
        e(3, "Ribbon cutting", "Open a school.", 5, true, ""),
    ];
    let points = achievements.iter().map(|a| a.points).sum();
    let points_unlocked = achievements.iter().filter(|a| a.unlocked).map(|a| a.points).sum();
    Snapshot {
        rom: String::new(),
        state: State::Ready,
        message: String::new(),
        user: "RetroFan".into(),
        game: "uCity".into(),
        game_id: 1,
        unlocked: 3,
        total: achievements.len() as u32,
        points,
        points_unlocked,
        waiting: 0,
        achievements,
    }
}
