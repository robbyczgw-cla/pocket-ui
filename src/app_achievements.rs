//! Part of the pocket-ui binary: the App's side of `pocket_ui::achievements` and `pocket_ui::achievements_page`.
//! Settings > RetroAchievements (sign in with the gamepad keyboard, log out), "12 of 40" on the game card and the
//! console page, and the pause menu's Achievements list. The login and the card's lookup run on threads; the UI thread
//! only reads small files and paints.
//!
//! The case line (`achievements::case_line`): "34 of 60 achievements · 56%" on the box's back, the card, beside Continue
//! and on "Where was I?" for a game whose own save says nothing (`app_case_knows` answered and has no line). The tally is
//! the kept file, read once per game and then kept here; a lookup on one worker thread renews it, on Wi-Fi only.

use std::cell::{Cell, OnceCell, RefCell};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use pocket_ui::achievements::account::Account;
use pocket_ui::cheats::CheatMenu;
use pocket_ui::control;
use pocket_ui::achievements::snapshot::{self, Snapshot, Summary, SNAPSHOT_FILE};
use pocket_ui::achievements::{api, case_line, http, lookup};
use pocket_ui::achievements_page::{self as page, AchievementsPage, Action, Event, Tone, Typing, View};
use pocket_ui::gfx::Canvas;
use pocket_ui::keyboard::Keyboard;
use pocket_ui::pad::Button;
use pocket_ui::palette::*;
use pocket_ui::text::Face;
use pocket_ui::touch::Area;

use super::{App, QuickMenu, SettingsPage, UserEvent, CANCEL, CONFIRM, NU_BTN_PAD, NU_PAD};
use pocket_ui::achievements::hardcore;
use pocket_ui::achievements::snapshot::State;
use pocket_ui::pause_saves::Saves;

/// The name in the Settings list, to come back to.
pub(crate) const SETTINGS_NAME: &str = "RetroAchievements";

/// A game to look up on the worker: (system, ROM, fresh: it just ended).
type Job = (String, PathBuf, bool);

/// How long an answer to "on Wi-Fi?" is kept: the shelf asks for each game it paints.
const WIFI_KEPT: Duration = Duration::from_secs(10);

/// What the App keeps: the open page, the account's name for the Settings list, the lookups under way, the tallies the
/// case lines read.
pub(crate) struct AchievementsUi {
    page: Option<AchievementsPage>,
    signed_in: Option<String>,
    /// Games asked about whose answer hasn't come back (one question at a time per game).
    looking_up: RefCell<HashSet<PathBuf>>,
    /// The kept tallies, by game: read at the first look, replaced by each answer (None: no file).
    tallies: RefCell<HashMap<PathBuf, Option<Summary>>>,
    /// Games that ended since their numbers were asked: asked fresh at the next look on Wi-Fi.
    ended: RefCell<HashSet<PathBuf>>,
    worker: OnceCell<mpsc::Sender<Job>>,
    wifi: Cell<Option<(Instant, bool)>>,
    /// `POCKETUI_SNAPSHOT_RA=<unlocked>/<total>`: every game's tally (snapshots only; signed in, nothing asked).
    pub(crate) made_up: Option<Summary>,
}

impl AchievementsUi {
    pub fn load(data_dir: &Path) -> AchievementsUi {
        AchievementsUi {
            page: None,
            signed_in: Account::load(data_dir).map(|a| a.shown_name().to_string()),
            looking_up: RefCell::default(),
            tallies: RefCell::default(),
            ended: RefCell::default(),
            worker: OnceCell::new(),
            wifi: Cell::new(None),
            made_up: None,
        }
    }

    /// `POCKETUI_SNAPSHOT_RA=34/60`: every game has that tally and the player is signed in (`--snapshot`).
    pub fn make_up(&mut self, spec: &str) {
        let Some((unlocked, total)) = spec.split_once('/').and_then(|(u, t)| Some((u.trim().parse().ok()?, t.trim().parse().ok()?))) else { return };
        self.made_up = Some(Summary { game_id: 1, total, unlocked, updated: u64::MAX, ..Default::default() });
        self.signed_in.get_or_insert_with(|| "RetroFan".into());
    }

    /// The game's kept tally: its file at the first look (offline too), then what the answers brought.
    pub fn tally(&self, data_dir: &Path, system: &str, rom: &Path) -> Option<Summary> {
        self.tallies.borrow_mut().entry(rom.to_path_buf()).or_insert_with(|| Summary::load(data_dir, system, rom)).clone()
    }

    /// Whether the game is to be looked up now, and then whether fresh (it just ended): signed in, on Wi-Fi, its
    /// numbers old (`lookup::wanted`) or the game ended, and not asked already (`case_line::ask`). `wifi` is asked only
    /// when all else says yes, and kept a moment (the shelf looks at every game it paints). Marks the game asked.
    pub fn due(&self, rom: &Path, known: Option<&Summary>, now: u64, wifi: impl FnOnce() -> bool) -> Option<bool> {
        if self.made_up.is_some() || self.signed_in.is_none() || self.looking_up.borrow().contains(rom) {
            return None;
        }
        let (wanted, ended) = (lookup::wanted(known, now), self.ended.borrow().contains(rom));
        if !(wanted || ended) {
            return None;
        }
        let up = match self.wifi.get() {
            Some((at, up)) if at.elapsed() < WIFI_KEPT => up,
            _ => {
                let up = wifi();
                self.wifi.set(Some((Instant::now(), up)));
                up
            }
        };
        if !case_line::ask(self.signed_in.is_some(), up, wanted, ended) {
            return None;
        }
        self.looking_up.borrow_mut().insert(rom.to_path_buf());
        self.ended.borrow_mut().remove(rom);
        Some(ended)
    }

    /// A game ended: its tally is read again from its file (pocket-core keeps it after playing) and asked fresh.
    pub fn forget(&self, rom: &Path) {
        self.tallies.borrow_mut().remove(rom);
        self.ended.borrow_mut().insert(rom.to_path_buf());
    }
}

/// Wi-Fi for the lookups: Android's handheld view asks Android (unmetered Wi-Fi); Linux its status corner (memory).
fn wifi_up() -> bool {
    #[cfg(all(target_os = "android", feature = "handheld"))]
    return super::app_android::unmetered_wifi();
    #[cfg(not(all(target_os = "android", feature = "handheld")))]
    pocket_ui::status_corner::status().wifi
}

/// The one thread that looks games up, one after the other (a shelf of games is one request per console, `lookup`).
fn start_worker(proxy: winit::event_loop::EventLoopProxy<UserEvent>, data: PathBuf) -> mpsc::Sender<Job> {
    let (tx, rx) = mpsc::channel::<Job>();
    let _ = std::thread::Builder::new().name("ra-lookup".into()).spawn(move || {
        let curl = http::system(http::user_agent(None));
        for (system, rom, fresh) in rx {
            // Signed out since it was asked: nothing goes out, and the game is free to be asked again.
            let summary = Account::load(&data).and_then(|account| lookup::lookup(&*curl, &data, &account, &system, &rom, now_secs(), fresh));
            let _ = proxy.send_event(UserEvent::Achievements(Event::Tally { system, rom, summary }));
        }
    });
    tx
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
        if matches!(page.view, View::List) && matches!(b, Button::Up | Button::Down) {
            page.step(if b == Button::Up { -1 } else { 1 });
            return Some(SettingsPage::Achievements);
        }
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
                    let curl = http::system(http::user_agent(None));
                    let result = api::login(&*curl, &username, &password);
                    page::wipe(&mut password);
                    let result = result
                        .map_err(|e| e.to_string())
                        .map(|a| a.keeping_choices(&data))
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
            // Hardcore for every game from the next start (no game runs while Settings is open).
            Action::Hardcore(on) => {
                let kept = Account::load(&self.data_dir).is_some_and(|mut a| {
                    a.hardcore = on;
                    a.save(&self.data_dir).is_ok()
                });
                if kept {
                    println!("achievements: hardcore {}", if on { "on" } else { "off" });
                } else {
                    pocket_ui::ui_sound::refuse();
                    if let Some(a) = page.account.as_mut() {
                        a.hardcore = !on;
                    }
                }
                // The shelf's open cases follow: in Hardcore no game continues from its moment.
                self.frozen_refresh();
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
                self.achievements.looking_up.borrow_mut().remove(&rom);
                let Some(summary) = summary else { return };
                if let Some(open) = self.card.as_mut().filter(|o| o.path == rom && o.system == system) {
                    open.card.data.achievements = summary.text();
                }
                let before = self.achievements.tallies.borrow_mut().insert(rom, Some(summary.clone()));
                if before.flatten().and_then(|b| case_line::line(&b)) != case_line::line(&summary) {
                    self.request_redraw();
                }
            }
        }
    }

    /// Where the page stands: a change is what the menu sounds and haptics answer.
    pub(crate) fn achievements_row(&self) -> Option<(u8, bool, bool, usize, bool)> {
        self.achievements.page.as_ref().map(|p| (view_kind(&p.view), p.signed_in(), p.confirm_logout, p.sel, p.hardcore()))
    }

    /// "12 of 40 achievements" for a game's card: what is known now, and a lookup when that is old or missing
    /// (signed in, on Wi-Fi; `lookup::wanted` keeps it to once an hour per game).
    pub(crate) fn card_achievements(&mut self, system: &str, path: &Path) -> Option<String> {
        pocket_ui::achievements::consoles::console_id(system)?;
        let known = match &self.achievements.made_up {
            Some(m) => Some(m.clone()),
            None => Summary::load(&self.data_dir, system, path),
        };
        self.achievements.tallies.borrow_mut().insert(path.to_path_buf(), known.clone());
        self.achievements_refresh(system, path, known.as_ref());
        known.and_then(|s| s.text())
    }

    /// Asks the worker about the game when `AchievementsUi::due` says so. Never in a snapshot.
    fn achievements_refresh(&self, system: &str, path: &Path, known: Option<&Summary>) {
        if super::snapshot_run() {
            return;
        }
        let Some(fresh) = self.achievements.due(path, known, now_secs(), wifi_up) else { return };
        let tx = self.achievements.worker.get_or_init(|| start_worker(self.proxy.clone(), self.data_dir.clone()));
        let _ = tx.send((system.to_string(), path.to_path_buf(), fresh));
    }

    /// "34 of 60 achievements · 56%" for a game with a set and at least one unlock, when signed in (`case_line`). Its kept
    /// tally, read once; renewed in the background (`achievements_refresh`). Progress from saves off (Cozy extras):
    /// nothing, as for the line from the save.
    pub(crate) fn achievements_line(&self, system: &str, path: &Path) -> Option<String> {
        if !pocket_ui::cozy_extras::on(pocket_ui::cozy_extras::Extra::ProgressFromSaves) {
            return None;
        }
        self.achievements.signed_in.as_ref()?;
        pocket_ui::achievements::consoles::console_id(system)?;
        if let Some(m) = &self.achievements.made_up {
            return case_line::line(m);
        }
        let known = self.achievements.tally(&self.data_dir, system, path);
        self.achievements_refresh(system, path, known.as_ref());
        known.as_ref().and_then(case_line::line)
    }

    /// A game ended: its tally is read again and asked fresh (`AchievementsUi::forget`).
    pub(crate) fn achievements_forget(&self, rom: &Path) {
        self.achievements.forget(rom);
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
            hardcore: false,
        };
        // "hardcore" / "hardcore-on": signed in, the Hardcore row selected, off or on.
        let signed_in = matches!(mode, "in" | "confirm" | "hardcore" | "hardcore-on");
        let account = Account { hardcore: mode == "hardcore-on", ..account };
        let mut p = AchievementsPage::new(signed_in.then_some(account));
        // The row shows here even while pocket-ui doesn't offer Hardcore (`hardcore::OFFERED`): its words are checked.
        if mode.starts_with("hardcore") {
            p.hardcore_offered = true;
            p.sel = 1;
        }
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
        let note = match typing {
            Typing::User => "Your user name on retroachievements.org.",
            Typing::Password => page::password_note_in(pocket_ui::device::android_profile()),
        };
        pocket_ui::setup::paint_keyboard_noted(c, &mut self.text, &shown, done, note);
        pocket_ui::setup::hints(
            c,
            &mut self.text,
            &[("A", "Type"), ("B", "Delete"), ("X", "Shift"), ("Y", "Symbols"), ("START", done)],
        );
    }

    fn paint_achievements_page(&mut self, c: &mut Canvas, page: &AchievementsPage) {
        let (pad, row, font) = (NU_PAD, pocket_ui::type_scale::row(66, 36), 36u32);
        self.sheet_header(c, SETTINGS_NAME);
        let (top, right) = (pocket_ui::kit::header_bottom(c.h) + 12, c.w - pad);
        // Sign in, or Log out with the account at the right and Hardcore under it (`achievements::hardcore`).
        let rows: Vec<(&str, String)> = std::iter::once((page.label(), page.value()))
            .chain((page.rows() > 1).then(|| ("Hardcore", page.hardcore_value().to_string())))
            .collect();
        for (i, (name, value)) in rows.iter().enumerate() {
            let y = top + i as i32 * row;
            self.hits.row(Area::new(pad, y, right - pad, row), i);
            let base = y + row / 2 + pocket_ui::type_scale::mid(font);
            let selected = i == page.sel;
            let ink = if selected { pal().ink } else { pal().text };
            let w = self.text.width(Face::Body, font, name) + 2 * NU_BTN_PAD;
            if selected {
                pocket_ui::kit::selection(c, pad, y, right - pad, row);
            }
            self.text.draw(c, Face::Body, font, pad + NU_BTN_PAD, base, ink, name);
            let value = self.text.fit(Face::Body, font, value, right - pad - 2 * NU_BTN_PAD - w - 40);
            self.text.draw_right(c, Face::Body, font, right - NU_BTN_PAD, base, ink, &value);
        }
        self.hits.list(page.sel, rows.len(), 0.0, row as f32);

        // The status, then what RetroAchievements is and how it works here.
        let (left, width) = (pad + NU_BTN_PAD, c.w - 2 * (pad + NU_BTN_PAD));
        let mut y = top + rows.len() as i32 * row + 44;
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
        // Hangul, hanzi and Vietnamese stacked marks stand taller than capitals: down by the difference, clear of the
        // label (`Text::cjk_rise`; 0 for every other line).
        y += 48 + lines.first().map_or(0, |l| self.text.cjk_rise(Face::Title, 44, l));
        for line in lines.iter().take(2) {
            self.text.draw(c, Face::Title, 44, left, y, color, line);
            y += pocket_ui::type_scale::line(44, 8);
        }
        y = y.max(panel_top + panel_h + 40);
        let action = match (&page.view, page.signed_in()) {
            (View::SigningIn, _) => None,
            (_, true) if page.sel == 1 => Some("Change"),
            (_, true) => Some("Log out"),
            (_, false) => Some("Sign in"),
        };
        let hints: Vec<(&str, &str)> = std::iter::once(("B", "Back")).chain(action.map(|a| ("A", a))).collect();
        let foot_top = pocket_ui::kit::footer(c, &mut self.text, &mut self.hits, &hints, page.describe(), 2, pal().text);
        // A short screen (768 lines) sets the three paragraphs smaller so that they end above the footer; where they
        // still don't fit (a wide face, a small screen) one size smaller, and what is left over is not painted.
        let first = if pocket_ui::kit::short_screen(c.h) { 24 } else { 28 };
        let room = foot_top - 12 - y;
        let height = |text: &mut pocket_ui::text::Text, px: u32| -> i32 {
            page::about_in(pocket_ui::device::android_profile()).iter().map(|p| text.wrap_prose(Face::Body, px, p, width).len() as i32 * pocket_ui::type_scale::line(px, 10) + 14).sum()
        };
        let about = if height(&mut self.text, first) <= room { first } else { first - 4 };
        'paragraphs: for p in page::about_in(pocket_ui::device::android_profile()) {
            for line in self.text.wrap_prose(Face::Body, about, p, width) {
                if y + pocket_ui::type_scale::px(about) as i32 / 4 > foot_top - 12 {
                    break 'paragraphs;
                }
                self.text.draw(c, Face::Body, about, left, y, pal().dust, &line);
                y += pocket_ui::type_scale::line(about, 10);
            }
            y += 14;
        }
    }
}

/// Hardcore (`achievements::hardcore`): pocket-core's slots aren't in the menu then, one line says why.
pub(crate) fn hardcore_saves(saves: Saves, hardcore: bool) -> Saves {
    if hardcore && saves == Saves::Slots {
        Saves::InGame(hardcore::MENU_LINE)
    } else {
        saves
    }
}

/// The pause menu's Achievements page's first row, while the game's achievements load or run: Hardcore, On or Off,
/// with what A does. None for a game they don't run in (nothing for Hardcore to keep).
pub(crate) fn hardcore_row(menu: &QuickMenu) -> Option<(String, String, String)> {
    if !hardcore::OFFERED {
        return None;
    }
    let snap = menu.achievements.as_ref().filter(|s| menu.hosted && matches!(s.state, State::Loading | State::Ready))?;
    let (value, desc) = match (snap.hardcore, menu.hardcore_ask) {
        (true, _) => ("On", "No save states, rewind or cheats; unlocks count as hardcore.\nA turns it off for every game: states and rewind come back."),
        (false, true) => ("Start again?", "Press A again: the game starts again in Hardcore, from its power-on.\nWhat you did since your last in-game save is lost. B keeps Hardcore off."),
        (false, false) => ("Off", "Hardcore: unlocks count as hardcore on the site, without states, rewind or cheats.\nA turns it on for every game; this game starts again."),
    };
    Some(("Hardcore".into(), value.into(), desc.into()))
}

/// The pause menu's Achievements page: the Hardcore row first (when there is one), then the game's list.
pub(crate) fn menu_page_rows(menu: &QuickMenu) -> Vec<(String, String, String)> {
    let snap = menu.achievements.clone().unwrap_or_default();
    hardcore_row(menu).into_iter().chain(page::menu_rows(&snap)).collect()
}

impl App {
    /// A on the pause menu's Hardcore row. Off → asks once, then keeps Hardcore for every game, tells pocket-core (which
    /// starts the game again, as RetroAchievements asks) and goes back to the game. On → off at once: Save, Load,
    /// Earlier and Cheats are back in the menu.
    pub(crate) fn hardcore_pressed(&mut self) {
        let Some(menu) = self.menu.as_mut() else { return };
        let on = menu.achievements.as_ref().is_some_and(|a| a.hardcore);
        if !on && !menu.hardcore_ask {
            menu.hardcore_ask = true;
            return;
        }
        menu.hardcore_ask = false;
        let kept = Account::load(&self.data_dir).is_some_and(|mut a| {
            a.hardcore = !on;
            a.save(&self.data_dir).is_ok()
        });
        if !kept || !control::pocket_core(&format!("{} {}", hardcore::COMMAND, if on { "off" } else { "on" })) {
            pocket_ui::ui_sound::refuse();
            return;
        }
        println!("achievements: hardcore {} in the game", if on { "off" } else { "on" });
        if !on {
            self.resume_game(None);
            return;
        }
        if let Some(a) = menu.achievements.as_mut() {
            a.hardcore = false;
        }
        menu.saves = Saves::Slots;
        if let Some(s) = menu.settings.as_mut() {
            s.hardcore = false;
        }
        menu.without_saving = !self.remember_quit;
        if let Some(run) = self.running.as_ref() {
            menu.cheats = CheatMenu::load(&self.data_dir, &run.system, &run.path);
            menu.earlier = super::app_earlier::row_at_open(run, menu.saves, menu.settings.as_ref());
        }
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
        hardcore: false,
        achievements,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    fn signed_in(dir: &Path) -> AchievementsUi {
        AchievementsUi { signed_in: Some("RetroFan".into()), ..AchievementsUi::load(dir) }
    }

    fn tally(unlocked: u32, updated: u64) -> Summary {
        Summary { game_id: 7, hash: "ab".into(), total: 60, unlocked, updated, ..Default::default() }
    }

    /// Without Wi-Fi nothing is asked, whatever else; signed out, Wi-Fi isn't even looked at. On Wi-Fi a game is asked
    /// once (until its answer), fresh numbers are not asked again, and a game that just ended is asked fresh.
    #[test]
    fn games_are_asked_on_wifi_only_once_and_fresh_after_playing() {
        let dir = tempfile::tempdir().unwrap();
        let rom = Path::new("/roms/gba/Fusion.gba");
        let looks = Cell::new(0);
        let wifi = |up: bool| {
            let looks = &looks;
            move || {
                looks.set(looks.get() + 1);
                up
            }
        };
        let out = AchievementsUi::load(dir.path());
        assert_eq!(out.due(rom, None, 1000, wifi(true)), None, "signed out");
        assert_eq!(looks.get(), 0, "signed out: Wi-Fi not even asked");

        let ui = signed_in(dir.path());
        assert_eq!(ui.due(rom, None, 1000, wifi(false)), None, "no Wi-Fi: nothing goes out");
        ui.ended.borrow_mut().insert(rom.to_path_buf());
        assert_eq!(ui.due(rom, Some(&tally(3, 0)), 1000, wifi(false)), None, "no Wi-Fi after a game either");
        assert!(ui.ended.borrow().contains(rom), "the game waits to be asked fresh");
        assert!(ui.looking_up.borrow().is_empty());

        let ui = signed_in(dir.path());
        assert_eq!(ui.due(rom, None, 1000, wifi(true)), Some(false));
        assert_eq!(ui.due(rom, None, 1000, wifi(true)), None, "asked already");
        ui.looking_up.borrow_mut().clear();
        assert_eq!(ui.due(rom, Some(&tally(3, 900)), 1000, wifi(true)), None, "fresh numbers stay");
        ui.forget(rom);
        assert_eq!(ui.due(rom, Some(&tally(3, 900)), 1000, wifi(true)), Some(true), "just ended: asked fresh");
        assert!(!ui.ended.borrow().contains(rom));

        // A snapshot's made-up numbers never ask.
        let mut snap = AchievementsUi::load(dir.path());
        snap.make_up("34/60");
        assert_eq!(snap.signed_in.as_deref(), Some("RetroFan"));
        assert_eq!(snap.due(rom, None, 1000, wifi(true)), None);
    }

    /// Offline, the kept tally is the line: read from its file once, kept, and read again after a game.
    #[test]
    fn the_kept_tally_answers_offline_and_after_a_game_is_read_again() {
        let dir = tempfile::tempdir().unwrap();
        let rom = Path::new("/roms/gba/Fusion.gba");
        let ui = signed_in(dir.path());
        assert_eq!(ui.tally(dir.path(), "gba", rom), None);
        tally(34, 1).save(dir.path(), "gba", rom);
        assert_eq!(ui.tally(dir.path(), "gba", rom), None, "kept: the file is read once");
        ui.forget(rom);
        let t = ui.tally(dir.path(), "gba", rom).unwrap();
        assert_eq!(case_line::line(&t).as_deref(), Some("34 of 60 achievements · 56%"));
        // pocket-core keeps 35 after the next game; the line follows once the game is over.
        tally(35, 2).save(dir.path(), "gba", rom);
        ui.forget(rom);
        assert_eq!(ui.tally(dir.path(), "gba", rom).unwrap().unlocked, 35);
    }
}
