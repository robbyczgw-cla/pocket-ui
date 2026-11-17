//! Part of the pocket-ui binary: the App's side of `pocket_ui::web` and `pocket_ui::web_page`. Settings > Web
//! companion, starting and stopping the server, and the edits browsers ask for.
//!
//! The server runs on threads of its own (`web::server`). The App only stores what they read (whether a game runs,
//! what is playing, Curator's index) in `web::Shared`, a comparison per loop turn (`web_sync`), and applies the edits
//! that arrive as `UserEvent::Web`, each with the same functions a press on the Pocket uses.

use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};

use pocket_ui::gfx::Canvas;
use pocket_ui::pad::Button;
use pocket_ui::palette::*;
use pocket_ui::text::Face;
use pocket_ui::touch::Area;
use pocket_ui::web::{self, Edit, NowPlaying, Prefs, Shared};
use pocket_ui::web_page::{self, Action, Paired, View, WebPage, ROWS};

use super::{App, SettingsPage, UserEvent, CANCEL, CONFIRM, NU_BTN_PAD, NU_PAD, NU_PILL};

/// The name in the Settings list, to come back to.
pub(crate) const SETTINGS_NAME: &str = "Web companion";

/// What the page says while the companion is off: what switching it on gives and where it works (the home Wi-Fi, or
/// Tailscale; since 0.5 it no longer needs Tailscale).
const OFF_NOTE: &str = "Switch it on to open your library in a browser on the same Wi-Fi, or over Tailscale. The address to open shows here.";
/// On, but neither the Wi-Fi nor Tailscale is connected.
const WAITING: &str = "Waiting for Wi-Fi";
const WAITING_NOTE: &str = "It listens on your Wi-Fi and on Tailscale. Connect to Wi-Fi (Settings › Connections › Wi-Fi) or switch Tailscale on.";
/// The note under the address, by "Your Tailscale devices".
const NOTE_OPEN: &str = "On the Wi-Fi every browser needs a code; your own Tailscale devices get in without one. Not encrypted on the Wi-Fi: use it at home. Pauses while a game runs.";
const NOTE_CODE: &str = "Every browser needs a code, yours too. Not encrypted on the Wi-Fi: use it at home. Pauses while a game runs.";
/// The same on a handheld without Wi-Fi (`device::no_wifi_chip`): no Wi-Fi to wait for or to warn about; it needs a
/// network all the same (`device::NEEDS_NETWORK`, for the Off and Waiting notes).
const WAITING_NO_WIFI: &str = "Waiting for a network";
const NOTE_OPEN_NO_WIFI: &str = "Other browsers need a code; your own Tailscale devices get in without one. Pauses while a game runs.";
const NOTE_CODE_NO_WIFI: &str = "Every browser needs a code, yours too. Pauses while a game runs.";

/// The server starts this long after pocket-ui did, when it is on: never in the way of the first frame.
const START_AFTER: Duration = Duration::from_secs(3);

/// News for the App from the server's threads.
#[derive(Debug)]
pub(crate) enum Event {
    /// A browser asks for a change; the answer goes back on the channel.
    Edit(Edit, std::sync::mpsc::Sender<Result<(), String>>),
    /// Something the Settings page shows changed (paired, listening).
    Changed,
}

/// What the App keeps of the web companion.
pub(crate) struct WebUi {
    prefs: Prefs,
    data_dir: PathBuf,
    es_systems: PathBuf,
    /// Made when the server first starts or the page first opens.
    shared: Option<Arc<Shared>>,
    server: Option<web::server::Server>,
    page: Option<WebPage>,
    /// What `publish` last stored, to store only changes.
    seen_game: Option<PathBuf>,
    seen_index: usize,
    seen_private: bool,
    /// When a start failed: the next try waits [`RETRY_AFTER`] (never one per frame).
    failed_at: Option<Instant>,
    /// An upload added games: the library as the server read it, taken over at the next quiet moment.
    library_pending: Option<web::Systems>,
    /// Where uploads go and where it listens: the device's own (tests put temporary folders and 127.0.0.1 here).
    roots: web::files::Roots,
    listen: Option<Vec<web::net::Listen>>,
}

/// A start that failed is tried again this much later.
const RETRY_AFTER: Duration = Duration::from_secs(30);

/// The game on screen, as `WebUi::publish` takes it from the App.
#[derive(Clone, Copy)]
pub(crate) struct Playing<'a> {
    pub system: &'a str,
    pub name: &'a str,
    pub path: &'a Path,
    pub started: Instant,
}

impl WebUi {
    pub fn load(data_dir: &Path, es_systems: &Path) -> WebUi {
        WebUi {
            prefs: Prefs::load(data_dir),
            data_dir: data_dir.to_path_buf(),
            es_systems: es_systems.to_path_buf(),
            shared: None,
            server: None,
            page: None,
            seen_game: None,
            seen_index: 0,
            seen_private: false,
            failed_at: None,
            library_pending: None,
            roots: web::files::Roots::default(),
            listen: None,
        }
    }

    fn shared(&mut self) -> Arc<Shared> {
        self.shared.get_or_insert_with(|| Shared::new(self.data_dir.clone(), self.es_systems.clone(), self.prefs)).clone()
    }

    /// Whether the server should be started now: on, not running, the Pocket up for [`START_AFTER`], and no failed
    /// start in the last [`RETRY_AFTER`].
    fn due(&self, since_start: Duration) -> bool {
        self.prefs.enabled && self.server.is_none() && since_start >= START_AFTER && self.failed_at.is_none_or(|t| t.elapsed() >= RETRY_AFTER)
    }

    /// Starts the server (once): `install` puts the App's ends into the shared state first. Never calls back into
    /// the App's loop functions (`web_sync` once recursed into this, review F1).
    fn start(&mut self, install: impl FnOnce(&Shared)) {
        if self.server.is_some() {
            return;
        }
        let shared = self.shared();
        install(&shared);
        // `publish` stores the game, Curator's index and the screen afresh.
        self.seen_game = None;
        self.seen_index = 0;
        self.seen_private = false;
        *shared.problem.lock().unwrap() = None;
        let ctx = web::api::Ctx::new(shared.clone(), self.roots.clone(), Box::new(web::net::LocalApi::default()));
        match web::server::Server::start(ctx, self.listen.clone()) {
            Ok(s) => {
                println!("web: companion on");
                self.server = Some(s);
                self.failed_at = None;
            }
            Err(e) => {
                eprintln!("web: can't start: {e}");
                *shared.problem.lock().unwrap() = Some(format!("It could not start: {e}."));
                self.failed_at = Some(Instant::now());
            }
        }
    }

    fn stop(&mut self) {
        if let Some(s) = self.server.take() {
            s.stop();
            println!("web: companion off");
        }
        if let Some(shared) = &self.shared {
            *shared.pairing.lock().unwrap() = None;
        }
    }

    /// One loop turn of the App (`App::web_sync` calls exactly this): starts the server when it is due, then stores
    /// what changed for it. Starting never comes back here (review F1: the old `web_start` called `web_sync`, which
    /// called `web_start` again, until the stack ran out).
    fn turn(&mut self, since_start: Duration, install: impl FnOnce(&Shared), game: Option<Playing>, index: Option<&Arc<pocket_ui::curator::Index>>, private: bool) {
        if !self.prefs.enabled {
            return;
        }
        if self.server.is_none() {
            if !self.due(since_start) {
                return;
            }
            self.start(install);
        }
        self.publish(game, index, private);
    }

    /// Stores what changed for the server: the game, Curator's index, whether the screen shows something private.
    /// Comparisons only when nothing changed; nothing at all while the server is not running.
    fn publish(&mut self, game: Option<Playing>, index: Option<&Arc<pocket_ui::curator::Index>>, private: bool) {
        let Some(shared) = self.shared.clone().filter(|_| self.server.is_some()) else { return };
        if game.map(|g| g.path) != self.seen_game.as_deref() {
            shared.game_running.store(game.is_some(), Ordering::Relaxed);
            *shared.now_playing.lock().unwrap() = game.as_ref().map(|r| NowPlaying {
                system: r.system.to_string(),
                name: r.name.to_string(),
                path: r.path.to_path_buf(),
                since: pocket_ui::recents::now_secs().saturating_sub(r.started.elapsed().as_secs()),
            });
            self.seen_game = game.map(|g| g.path.to_path_buf());
        }
        let ix = index.map_or(0, |ix| Arc::as_ptr(ix) as usize);
        if ix != self.seen_index {
            *shared.curator.lock().unwrap() = index.cloned();
            self.seen_index = ix;
        }
        if private != self.seen_private {
            shared.screen_private.store(private, Ordering::Relaxed);
            self.seen_private = private;
        }
    }

    /// Makes a pairing code. The screen counts as private before the code is there: no screenshot ever shows it
    /// (review F4; `publish` says so every loop turn too).
    fn start_pairing(&mut self) {
        let shared = self.shared();
        shared.screen_private.store(true, Ordering::Relaxed);
        self.seen_private = true;
        match web::auth::Pairing::new() {
            Ok(p) => *shared.pairing.lock().unwrap() = Some(p),
            Err(e) => eprintln!("web: no pairing code: {e}"),
        }
    }

    /// New choices from the Settings page: saved, and the server started or stopped as they say.
    fn set_prefs(&mut self, prefs: Prefs, install: impl FnOnce(&Shared)) -> bool {
        let was = self.prefs;
        let saved = prefs.save(&self.data_dir);
        self.prefs = prefs;
        let shared = self.shared();
        *shared.prefs.lock().unwrap() = prefs;
        if prefs.trust_tailnet != was.trust_tailnet {
            // Who was let in by Tailscale is asked again (review F14).
            shared.trust_changed();
        }
        if prefs.enabled && !was.enabled {
            self.failed_at = None;
            self.start(install);
        } else if !prefs.enabled && was.enabled {
            self.stop();
        }
        saved
    }
}

/// The App's ends of the server: edits and news come back to the event loop as `UserEvent::Web`.
fn install(proxy: &winit::event_loop::EventLoopProxy<UserEvent>, shared: &Shared) {
    let (edits, notice) = (proxy.clone(), std::sync::Mutex::new(proxy.clone()));
    *shared.edits.lock().unwrap() = Some(Box::new(move |e, tx| edits.send_event(UserEvent::Web(Event::Edit(e, tx))).is_ok()));
    *shared.notice.lock().unwrap() = Some(Box::new(move || {
        let _ = notice.lock().unwrap().send_event(UserEvent::Web(Event::Changed));
    }));
}

impl App {
    /// A game was hidden, shown again or deleted on the Pocket: the server reads its game list again at the next
    /// request (`Shared::library_changed`), so the companion never lists a hidden game.
    pub(crate) fn web_library_changed(&self) {
        if let Some(shared) = self.web.loaded().and_then(|w| w.shared.as_ref()) {
            shared.library_changed();
        }
    }

    /// Text for the Settings list: "On >" / "Off >".
    pub(crate) fn web_list_value(&self) -> String {
        format!("{} >", web_page::on_off(self.web.prefs.enabled))
    }

    /// Stores the game, Curator's index and whether the screen is private (any Settings page: the pairing code,
    /// passwords, keys; the first-start steps: Wi-Fi passwords; review F4) for the server.
    fn web_publish(&mut self) {
        let game = self.running.as_ref().map(|r| Playing { system: &r.system, name: &r.name, path: &r.path, started: r.started });
        let private = self.settings_page.is_some() || self.setup.is_some();
        self.web.publish(game, self.curator.index.as_ref(), private);
    }

    /// Called every loop turn (`about_to_wait`): starts the server once the Pocket is up, and stores what changed
    /// for it. Comparisons only, when nothing changed.
    pub(crate) fn web_sync(&mut self) {
        // Read on a thread after the first frame (`startup::Later`): nothing to start before it is there.
        if !self.web.loaded().is_some_and(|w| w.prefs.enabled) {
            return;
        }
        let since = self.started.elapsed();
        let game = self.running.as_ref().map(|r| Playing { system: &r.system, name: &r.name, path: &r.path, started: r.started });
        let private = self.settings_page.is_some() || self.setup.is_some();
        let proxy = &self.proxy;
        self.web.turn(since, |s| install(proxy, s), game, self.curator.index.as_ref(), private);
        if self.web.server.is_none() {
            return;
        }
        // New games from an upload: the list the server read (on its thread) goes in on Home or the Settings list,
        // never in a game; nothing is read from the card here.
        if self.web.library_pending.is_some() && self.running.is_none() && self.card.is_none() && (self.screen == super::Screen::Home && self.settings_page.is_none() || matches!(self.settings_page, Some(SettingsPage::List(_) | SettingsPage::Web))) {
            if let Some(web::Systems(systems)) = self.web.library_pending.take() {
                println!("web: new games from an upload");
                // The same game stays selected: the new list's numbers may mean another one (as `library_apply_pending`).
                let was = super::app_library::KeepSelection::note(&self.items, self.sel, &self.systems);
                let in_front = self.big_in_front();
                self.systems = Arc::unwrap_or_clone(systems);
                // The next start shows them from the cache (`library_cache`), written on a thread.
                pocket_ui::library_cache::store_soon(&self.data_dir, &self.es_systems, self.systems.clone());
                self.recents.follow_library(&self.systems);
                self.curator.library_changed();
                self.find_library_changed();
                self.covers.clear();
                self.rebuild();
                was.restore(&self.items, &self.systems, &mut self.sel, &mut self.prev_sel);
                self.big_keep_part(in_front);
                self.auto_covers("games uploaded");
            }
        }
    }

    pub(crate) fn web_event(&mut self, e: Event) {
        match e {
            Event::Edit(edit, tx) => {
                // Switched off meanwhile: what was still on its way is not applied (review F7).
                let result = if self.web.server.is_some() { self.web_apply(edit) } else { Err("The web companion is off.".into()) };
                let _ = tx.send(result);
                self.request_redraw();
            }
            Event::Changed => {
                if self.web.page.is_some() {
                    self.web_page_refresh();
                    self.request_redraw();
                }
            }
        }
    }

    /// A browser's change, done as the Pocket's own button would do it.
    fn web_apply(&mut self, e: Edit) -> Result<(), String> {
        let io = |r: std::io::Result<()>| r.map_err(|e| format!("The Pocket could not save it: {e}"));
        match e {
            Edit::Favorite { system, name, path, on } => {
                if self.favorites.contains(&path) != on {
                    self.favorites.toggle(&system, &name, &path);
                    self.curator.touch();
                    self.rebuild();
                }
                Ok(())
            }
            Edit::Status { path, status } => {
                io(self.library.set_status(&path, status))?;
                self.curator.touch();
                Ok(())
            }
            Edit::Note { path, note } => io(self.library.set_note(&path, &note)),
            Edit::Rating { path, rating } => {
                io(self.library.set_rating(&path, rating))?;
                self.curator.touch();
                Ok(())
            }
            Edit::RenameShelf { index, was, name } => {
                let name: String = name.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(pocket_ui::curator::prefs::SHELF_NAME_MAX).collect();
                if name.is_empty() {
                    return Err("Give the shelf a name.".into());
                }
                match self.curator.prefs.shelves.get_mut(index).filter(|s| s.name == was) {
                    Some(s) => s.name = name,
                    None => return Err("That shelf changed on the Pocket. Reload the page.".into()),
                }
                self.curator_prefs_changed()
            }
            Edit::DeleteShelf { index, was } => {
                if self.curator.prefs.shelves.get(index).is_none_or(|s| s.name != was) {
                    return Err("That shelf changed on the Pocket. Reload the page.".into());
                }
                self.curator.prefs.shelves.remove(index);
                self.curator_prefs_changed()
            }
            Edit::Preferred { work, path } => {
                io(self.library.set_preferred(&work, &path))?;
                self.curator.touch();
                Ok(())
            }
            Edit::LibraryChanged(systems) => {
                self.web.library_pending = Some(systems);
                Ok(())
            }
        }
    }

    // --- Settings > Connections > Web companion ------------------------------------------------------------------------------

    pub(crate) fn open_web(&mut self) -> Option<SettingsPage> {
        self.web.page = Some(WebPage::new(self.web.prefs));
        self.web_page_refresh();
        Some(SettingsPage::Web)
    }

    /// The page's lists from the shared state: paired browsers, addresses, the code's time left.
    fn web_page_refresh(&mut self) {
        let shared = self.web.shared();
        let browsers: Vec<Paired> = shared
            .sessions
            .lock()
            .unwrap()
            .browsers
            .iter()
            .rev()
            .map(|b| Paired { id: b.id.clone(), name: b.name.clone(), when: web_page::short_date(b.paired) })
            .collect();
        let listening = shared.listening.lock().unwrap().clone();
        let lan = shared.lan.lock().unwrap().clone();
        let lan_name = shared.lan_name.lock().unwrap().clone();
        let name = shared.tailnet_name.lock().unwrap().clone();
        let pairing = shared.pairing.lock().unwrap().clone();
        let running = self.web.server.is_some();
        let problem = shared.problem.lock().unwrap().clone();
        let Some(page) = self.web.page.as_mut() else { return };
        page.problem = problem.filter(|_| page.prefs.enabled);
        page.browsers = browsers;
        if let View::Browsers { sel, .. } = &mut page.view {
            *sel = (*sel).min(page.browsers.len().saturating_sub(1));
        }
        // The Brick's userspace Tailscale: that socket is on 127.0.0.1, the address to open is the node's (shown once it is known).
        #[cfg(feature = "brick")]
        let userspace = pocket_ui::brick_profile::standalone();
        #[cfg(not(feature = "brick"))]
        let userspace = false;
        page.addresses = if running { page_addresses(&listening, &lan, lan_name.as_deref(), name.as_deref(), userspace) } else { Vec::new() };
        page.address = page.addresses.first().cloned();
        match pairing {
            Some(p) if !p.expired(Instant::now()) => {
                page.code = Some(p.code.clone());
                page.code_left = p.until.saturating_duration_since(Instant::now()).as_secs();
            }
            _ => {
                // Used, run out or five wrong tries: the code goes from the screen.
                if page.view == View::Pairing && page.code.is_some() {
                    page.view = View::List;
                }
                page.code = None;
            }
        }
    }

    pub(crate) fn web_input(&mut self, b: Button) -> Option<SettingsPage> {
        let back = Some(SettingsPage::List(super::settings_row(SETTINGS_NAME)));
        let Some(page) = self.web.page.as_mut() else { return back };
        let action = page.press(b, CONFIRM, CANCEL);
        match action {
            Action::Nothing => {}
            Action::Close => {
                self.web.page = None;
                if let Some(s) = &self.web.shared {
                    *s.pairing.lock().unwrap() = None;
                }
                return back;
            }
            Action::Save(prefs) => {
                let proxy = &self.proxy;
                if !self.web.set_prefs(prefs, |s| install(proxy, s)) {
                    pocket_ui::ui_sound::refuse();
                }
                self.web_publish();
                self.web_page_refresh();
            }
            Action::StartPairing => {
                self.web.start_pairing();
                self.web_page_refresh();
            }
            Action::StopPairing => {
                *self.web.shared().pairing.lock().unwrap() = None;
                self.web_page_refresh();
            }
            Action::Revoke(id) => {
                let ok = self.web.shared().sessions.lock().unwrap().revoke(&id);
                if !ok {
                    pocket_ui::ui_sound::refuse();
                }
                self.web_page_refresh();
            }
        }
        Some(SettingsPage::Web)
    }

    /// The pairing code counts down on screen: a frame a second while it shows (and the page notices when it ran out).
    pub(crate) fn web_wake(&mut self, now: Instant) -> Option<Instant> {
        let showing = self.settings_page == Some(SettingsPage::Web) && self.web.page.as_ref().is_some_and(|p| p.view == View::Pairing) && self.web.server.is_some();
        if !showing {
            return None;
        }
        self.request_redraw();
        Some(now + Duration::from_secs(1))
    }

    /// Where the page stands: a change is what the menu sounds answer.
    pub(crate) fn web_row(&self) -> Option<(usize, u8, Prefs)> {
        self.web.page.as_ref().map(|p| {
            let kind = match p.view {
                View::List => 0,
                View::Pairing => 1,
                View::Browsers { sel, confirm } => 2 + (sel as u8) * 2 + confirm as u8,
            };
            (p.sel, kind, p.prefs)
        })
    }

    /// `--snapshot web[:off|:on|:pair|:browsers|:revoke|:waiting|:busy|:none][:<row>]`: the page with made-up data.
    pub(crate) fn web_snapshot(&mut self, state: &str) -> bool {
        let Some(rest) = state.strip_prefix("web") else { return false };
        let mut parts = rest.trim_start_matches(':').split(':');
        let mode = parts.next().unwrap_or("");
        let row: usize = parts.next().and_then(|r| r.parse().ok()).unwrap_or(0);
        let on = mode != "off";
        let mut page = WebPage::new(Prefs { enabled: on, ..Prefs::default() });
        page.sel = row.min(ROWS.len() - 1);
        page.browsers = vec![
            Paired { id: "a1".into(), name: "Safari on iPhone (alexs-iphone)".into(), when: "21 Oct".into() },
            Paired { id: "b2".into(), name: "Firefox on Windows".into(), when: "23 Oct".into() },
        ];
        if mode == "busy" {
            page.problem = Some(web::server::why_not(&std::io::Error::from(std::io::ErrorKind::AddrInUse)));
        }
        if on && !matches!(mode, "waiting" | "busy") {
            page.addresses = page_addresses(
                &["192.0.2.42:80".parse().unwrap(), "198.51.100.34:80".parse().unwrap(), "[2001:db8::4f01:2a3b]:80".parse().unwrap()],
                &["192.0.2.42:80".parse().unwrap()],
                Some("sm8250.local"),
                Some("pocket"),
                false,
            );
            page.address = page.addresses.first().cloned();
        }
        match mode {
            "pair" => {
                page.view = View::Pairing;
                page.code = Some("042917".into());
                page.code_left = 287;
            }
            "browsers" => page.view = View::Browsers { sel: 1, confirm: false },
            "revoke" => page.view = View::Browsers { sel: 1, confirm: true },
            // First use: on, nobody paired yet.
            "none" => page.browsers.clear(),
            _ => {}
        }
        self.web.page = Some(page);
        self.settings_page = Some(SettingsPage::Web);
        true
    }

    pub(crate) fn paint_web(&mut self, c: &mut Canvas) {
        let Some(page) = self.web.page.take() else { return };
        // The code's time left, for the countdown.
        let page = if page.view == View::Pairing && self.web.server.is_some() {
            self.web.page = Some(page);
            self.web_page_refresh();
            self.web.page.take().unwrap()
        } else {
            page
        };
        c.clear(pal().void);
        match &page.view {
            View::List => self.paint_web_list(c, &page),
            View::Pairing => self.paint_web_pairing(c, &page),
            View::Browsers { sel, confirm } => self.paint_web_browsers(c, &page, *sel, *confirm),
        }
        self.web.page = Some(page);
    }

    fn paint_web_list(&mut self, c: &mut Canvas, page: &WebPage) {
        let (pad, row, font) = (NU_PAD, pocket_ui::type_scale::row(66, 36), 36u32);
        // On a short screen (768 lines) the address, its explanation and the note keep their place above the footer.
        let row = if pocket_ui::kit::short_screen(c.h) { row * 4 / 5 } else { row };
        self.sheet_header(c, SETTINGS_NAME);
        let (top, right) = (pad + NU_PILL + 12, c.w - pad);
        for (i, r) in ROWS.iter().enumerate() {
            let y = top + i as i32 * row;
            self.hits.row(Area::new(pad, y, right - pad, row), i);
            let base = y + row / 2 + pocket_ui::type_scale::mid(font);
            let name = page.label(*r);
            let selected = i == page.sel;
            let dim = matches!(r, web_page::Row::Pair) && !page.prefs.enabled;
            if selected {
                pocket_ui::kit::selection(c, pad, y, right - pad, row);
                self.text.draw(c, Face::Body, font, pad + NU_BTN_PAD, base, pal().ink, name);
            } else {
                self.text.draw(c, Face::Body, font, pad + NU_BTN_PAD, base, if dim { pal().quiet } else { pal().text }, name);
            }
            let value = page.value(*r);
            pocket_ui::kit::row_value(c, &mut self.text, font, right - NU_BTN_PAD, base, &value, selected);
        }
        self.hits.list(page.sel, ROWS.len(), 0.0, row as f32);

        // Where to open it, or why it isn't there yet.
        let (left, width) = (pad + NU_BTN_PAD, c.w - 2 * (pad + NU_BTN_PAD));
        let mut y = top + ROWS.len() as i32 * row + 40;
        if page.prefs.enabled {
            self.text.draw(c, Face::Body, 24, left, y, pal().dust, "Open on your phone or computer");
        }
        y += 58;
        match (&page.address, page.prefs.enabled) {
            (Some(a), _) => {
                let a = self.text.fit(Face::Title, 50, a, width);
                self.text.draw(c, Face::Title, 50, left, y, pal().chalk, &a);
                y += 48;
                let also: Vec<&str> = page.addresses.iter().map(String::as_str).filter(|x| Some(*x) != page.address.as_deref()).take(2).collect();
                if !also.is_empty() {
                    let line = self.text.fit(Face::Body, 26, &format!("or {}", also.join("  ·  ")), width);
                    self.text.draw(c, Face::Body, 26, left, y, pal().dust, &line);
                    y += 40;
                }
            }
            (None, true) if page.problem.is_some() => {
                self.text.draw(c, Face::Title, 44, left, y, pal().alert, "Not listening");
                y += 44;
                let why = format!("{} It tries again every 30 seconds.", page.problem.as_deref().unwrap_or_default());
                for l in self.text.wrap_prose(Face::Body, 26, &why, width) {
                    self.text.draw(c, Face::Body, 26, left, y, pal().dust, &l);
                    y += pocket_ui::type_scale::line(26, 8);
                }
            }
            (None, true) => {
                self.text.draw(c, Face::Title, 44, left, y, pal().dust, pocket_ui::device::wifi_or(WAITING, WAITING_NO_WIFI));
                y += 44;
                for l in self.text.wrap_prose(Face::Body, 26, pocket_ui::device::wifi_or(WAITING_NOTE, pocket_ui::device::NEEDS_NETWORK), width) {
                    self.text.draw(c, Face::Body, 26, left, y, pal().dust, &l);
                    y += pocket_ui::type_scale::line(26, 8);
                }
            }
            (None, false) => {
                self.text.draw(c, Face::Title, 44, left, y, pal().dust, "Off");
                y += 44;
                for l in self.text.wrap_prose(Face::Body, 26, pocket_ui::device::wifi_or(OFF_NOTE, pocket_ui::device::NEEDS_NETWORK), width) {
                    self.text.draw(c, Face::Body, 26, left, y, pal().dust, &l);
                    y += pocket_ui::type_scale::line(26, 8);
                }
            }
        }
        y += 16;
        let note = if page.prefs.trust_tailnet {
            pocket_ui::device::wifi_or(NOTE_OPEN, NOTE_OPEN_NO_WIFI)
        } else {
            pocket_ui::device::wifi_or(NOTE_CODE, NOTE_CODE_NO_WIFI)
        };
        // Pair opens only while the companion is on, the browsers' list only with a browser in it: no A where A does nothing
        // (the row's line says why; the navigation explorer, 2026-11-07).
        let hints: &[(&str, &str)] = match page.row() {
            web_page::Row::Pair if !page.prefs.enabled => &[("B", "Back")],
            web_page::Row::Browsers if page.browsers.is_empty() => &[("B", "Back")],
            web_page::Row::Pair | web_page::Row::Browsers => &[("B", "Back"), ("A", "Open")],
            _ => &[("B", "Back"), ("A", "Change")],
        };
        // The foot first: the note stops above it, shortened with "…" (at Large text on 768 lines it ran into the hints, the Brick).
        let foot = pocket_ui::kit::footer(c, &mut self.text, &mut self.hits, hints, page.describe(page.row()), 2, pal().text);
        self.text.paragraph(c, Face::Body, 24, (left, y), width, pocket_ui::type_scale::line(24, 8), foot - 8, pal().quiet, note);
    }

    /// The code in large digits beside a QR code of the address with the code in it (the page pairs by itself).
    fn paint_web_pairing(&mut self, c: &mut Canvas, page: &WebPage) {
        let pad = NU_PAD;
        self.sheet_header(c, "Pair a browser");
        let (left, top) = (pad + NU_BTN_PAD, pad + NU_PILL + 30);
        let address = page.address.clone().unwrap_or_else(|| "http://pocket".into());
        let Some(code) = page.code.clone() else {
            self.text.draw(c, Face::Title, 44, left, top + 80, pal().dust, "No code: the Pocket gave no random numbers.");
            self.nu_hints(c, &[("B", "Back")]);
            return;
        };
        let qr = 460;
        super::app_sync::paint_qr(c, left, top, qr, &format!("{address}/#pair={code}"));
        let tx = left + qr + 50;
        let width = c.w - pad - NU_BTN_PAD - tx;
        let mut y = top + 36;
        for s in ["1.  Scan this with your phone's camera,", "     or open this address in a browser:"] {
            let s = self.text.fit(Face::Body, 28, s, width);
            self.text.draw(c, Face::Body, 28, tx, y, pal().text, &s);
            y += 44;
        }
        let shown = self.text.fit(Face::Title, 37, &address, width);
        self.text.draw(c, Face::Title, 37, tx, y + 6, pal().chalk, &shown);
        y += 70;
        self.text.draw(c, Face::Body, 28, tx, y, pal().text, "2.  Type this code:");
        y += 120;
        let spaced = format!("{} {}", &code[..3], &code[3..]);
        self.text.draw(c, Face::Title, 120, tx, y, pal().chalk, &spaced);
        y += 64;
        let left_txt = format!("Valid for {}:{:02} more, for one browser.", page.code_left / 60, page.code_left % 60);
        let left_txt = self.text.fit(Face::Body, 24, &left_txt, width);
        self.text.draw(c, Face::Body, 24, tx, y, pal().dust, &left_txt);
        y += 34;
        for l in self.text.wrap_prose(Face::Body, 24, "Five wrong tries and the code is gone.", width) {
            self.text.draw(c, Face::Body, 24, tx, y, pal().dust, &l);
            y += pocket_ui::type_scale::line(24, 8);
        }
        self.nu_hints(c, &[("B", "Done")]);
    }

    fn paint_web_browsers(&mut self, c: &mut Canvas, page: &WebPage, sel: usize, confirm: bool) {
        let (pad, row, font) = (NU_PAD, pocket_ui::type_scale::row(76, 34), 34u32);
        self.sheet_header(c, "Paired browsers");
        let (top, right) = (pad + NU_PILL + 12, c.w - pad);
        let visible = ((c.h - top - pad - NU_PILL - 30) / row).max(1) as usize;
        let first = super::window_start(sel, visible, page.browsers.len());
        for (i, b) in page.browsers.iter().enumerate().skip(first).take(visible) {
            let y = top + (i - first) as i32 * row;
            self.hits.row(Area::new(pad, y, right - pad, row), i);
            let base = y + row / 2 + pocket_ui::type_scale::mid(font);
            let when = format!("paired {}", b.when);
            // The name leaves the date (or "A again: revoke") its room: a long name ("Safari on iPhone (alexs-iphone)")
            // is cut with "…" before it touches it (German's "gekoppelt 21. Okt." is wider than English's).
            let side_w = self.text.width(Face::Body, 28, &when).max(self.text.width(Face::Body, 28, "A again: revoke"));
            let name_room = right - pad - 2 * NU_BTN_PAD - side_w - 24;
            if i == sel {
                pocket_ui::retro::select(c, pad, y + 4, right - pad, row - 8, (row - 8) / 2, if confirm { pal().alert } else { pal().select });
                let name = self.text.fit(Face::Body, font, &b.name, name_room);
                let fg = if confirm { pal().on_alert } else { pal().ink };
                self.text.draw(c, Face::Body, font, pad + NU_BTN_PAD, base, fg, &name);
                let side = if confirm { "A again: revoke" } else { &when };
                self.text.draw_right(c, Face::Body, 28, right - NU_BTN_PAD, base, fg, side);
            } else {
                let name = self.text.fit(Face::Body, font, &b.name, name_room);
                self.text.draw(c, Face::Body, font, pad + NU_BTN_PAD, base, pal().text, &name);
                self.text.draw_right(c, Face::Body, 28, right - NU_BTN_PAD, base, pal().dust, &when);
            }
        }
        self.hits.list(sel, page.browsers.len(), 0.0, row as f32);
        let hints: &[(&str, &str)] = if confirm { &[("B", "Keep"), ("A", "Revoke")] } else { &[("B", "Back"), ("A", "Revoke")] };
        let desc = if confirm { "That browser will need a new code." } else { "A revoked browser needs a new code." };
        self.nu_foot(c, hints, desc, pal().text);
    }

    /// pocket-ui ends: the server stops first (its sockets close at once).
    pub(crate) fn web_exit(&mut self) {
        self.web.stop();
    }
}

/// Where to open the companion, the first one large on the page and in the QR code: the Wi-Fi's address
/// (`http://192.0.2.42`, which every browser at home can open) and its mDNS name, then the Pocket's tailnet name and
/// addresses. `userspace`: the Brick's userspace Tailscale, whose 127.0.0.1 socket is no address to open (its name is).
fn page_addresses(listening: &[std::net::SocketAddr], lan: &[std::net::SocketAddr], lan_name: Option<&str>, tailnet_name: Option<&str>, userspace: bool) -> Vec<String> {
    let mut out: Vec<String> = lan.iter().map(|a| address(*a)).collect();
    if let Some(n) = lan_name.filter(|_| !lan.is_empty()) {
        out.push(format!("http://{n}"));
    }
    let tailnet: Vec<&std::net::SocketAddr> = listening.iter().filter(|a| !lan.contains(a)).collect();
    let on_tailnet = tailnet.iter().any(|a| pocket_ui::web::net::is_tailnet(a.ip()) || (userspace && a.ip().is_loopback()));
    if let Some(n) = tailnet_name.filter(|n| on_tailnet && !n.is_empty()) {
        out.push(format!("http://{n}"));
    }
    out.extend(tailnet.iter().filter(|a| !(userspace && a.ip().is_loopback())).map(|a| address(**a)));
    out.dedup();
    out
}

/// "http://100.77.12.34", "http://[fd7a::1]" (port 80 left out).
fn address(a: std::net::SocketAddr) -> String {
    let host = match a.ip() {
        std::net::IpAddr::V4(v4) => v4.to_string(),
        std::net::IpAddr::V6(v6) => format!("[{v6}]"),
    };
    if a.port() == 80 {
        format!("http://{host}")
    } else {
        format!("http://{host}:{}", a.port())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};

    /// 0.5: the companion works on the home Wi-Fi too, so the Off page no longer says it needs Tailscale; it names
    /// the Wi-Fi first, in every language.
    #[test]
    fn the_off_page_names_the_wifi_and_tailscale_in_every_language() {
        assert!(OFF_NOTE.contains("Wi-Fi") && OFF_NOTE.contains("Tailscale") && !OFF_NOTE.contains("Needs Tailscale"));
        for (lang, wifi) in [("de", "WLAN"), ("es", "Wi-Fi"), ("pt", "Wi-Fi"), ("fr", "Wi-Fi"), ("it", "Wi-Fi")] {
            let tsv = std::fs::read_to_string(format!("{}/assets/i18n/{lang}.tsv", env!("CARGO_MANIFEST_DIR"))).unwrap();
            for line in [OFF_NOTE, WAITING, WAITING_NOTE, NOTE_OPEN, NOTE_CODE, WAITING_NO_WIFI, NOTE_OPEN_NO_WIFI, NOTE_CODE_NO_WIFI] {
                let row = tsv.lines().find(|l| l.starts_with(&format!("{line}\t"))).unwrap_or_else(|| panic!("{lang}: no translation of {line}"));
                assert!(row.split('\t').nth(1).is_some_and(|t| !t.is_empty() && t != line), "{lang}: {row}");
            }
            let off = tsv.lines().find(|l| l.starts_with(&format!("{OFF_NOTE}\t"))).unwrap();
            assert!(off.split('\t').nth(1).unwrap().contains(wifi), "{lang}: {off}");
        }
    }

    /// The address to open: the Wi-Fi's first (every browser at home can open it, and the QR code holds it), then its
    /// mDNS name and the tailnet name; without the Wi-Fi the tailnet name as before.
    #[test]
    fn the_wifi_address_comes_first_then_mdns_and_tailscale() {
        let sa = |s: &str| s.parse::<std::net::SocketAddr>().unwrap();
        let both = page_addresses(&[sa("192.0.2.42:80"), sa("100.77.12.34:80")], &[sa("192.0.2.42:80")], Some("sm8250.local"), Some("pocket"), false);
        assert_eq!(both, ["http://192.0.2.42", "http://sm8250.local", "http://pocket", "http://100.77.12.34"]);
        let no_avahi = page_addresses(&[sa("192.0.2.42:80")], &[sa("192.0.2.42:80")], None, Some("pocket"), false);
        assert_eq!(no_avahi, ["http://192.0.2.42"], "no .local without avahi, no tailnet name without Tailscale");
        let tailnet = page_addresses(&[sa("100.77.12.34:80")], &[], Some("sm8250.local"), Some("pocket"), false);
        assert_eq!(tailnet, ["http://pocket", "http://100.77.12.34"], "Tailscale alone: as before 0.5");
        // The Brick's userspace Tailscale: the loopback socket is no address; its name is.
        let brick = page_addresses(&[sa("127.0.0.1:80"), sa("192.168.1.77:80")], &[sa("192.168.1.77:80")], None, Some("brick"), true);
        assert_eq!(brick, ["http://192.168.1.77", "http://brick"]);
        assert!(page_addresses(&[], &[], None, None, false).is_empty());
    }

    /// A WebUi as `main` makes it, over a temporary data folder, listening on 127.0.0.1 instead of the tailnet.
    fn web_ui(name: &str, enabled: bool) -> (WebUi, tempfile::TempDir) {
        let guard = tempfile::Builder::new().prefix(&format!("pocketui-app-web-{name}-")).tempdir().unwrap();
        let root = guard.path().to_path_buf();
        std::fs::create_dir_all(root.join("data")).unwrap();
        std::fs::create_dir_all(root.join("roms")).unwrap();
        std::fs::write(root.join("es_systems.cfg"), "<systemList></systemList>").unwrap();
        if enabled {
            assert!(Prefs { enabled: true, ..Prefs::default() }.save(&root.join("data")));
        }
        let mut ui = WebUi::load(&root.join("data"), &root.join("es_systems.cfg"));
        ui.roots = web::files::Roots::under(&root.join("roms"));
        ui.listen = Some(vec![web::net::Listen { addr: "127.0.0.1:0".parse().unwrap(), iface: "lo".into(), index: 0, lan: false }]);
        (ui, guard)
    }

    /// The page from the running server, as a browser gets it.
    fn page_answers(ui: &WebUi) -> bool {
        let shared = ui.shared.clone().unwrap();
        let deadline = Instant::now() + crate::test_clock::timeout(Duration::from_secs(5));
        let addr = loop {
            if let Some(a) = shared.listening.lock().unwrap().first().copied() {
                break a;
            }
            assert!(Instant::now() < deadline, "the server never listened");
            std::thread::sleep(Duration::from_millis(10));
        };
        let Ok(mut s) = std::net::TcpStream::connect(addr) else { return false };
        s.set_read_timeout(Some(crate::test_clock::timeout(Duration::from_secs(5)))).unwrap();
        write!(s, "GET / HTTP/1.1\r\nHost: {addr}\r\n\r\n").unwrap();
        let mut out = String::new();
        let _ = s.read_to_string(&mut out);
        out.starts_with("HTTP/1.1 200")
    }

    /// The App's loop turn: `App::web_sync` calls `WebUi::turn` with its game, Curator's index and screen.
    fn loop_turn(ui: &mut WebUi, since_start: Duration, installs: &std::cell::Cell<u32>) {
        ui.turn(since_start, |_| installs.set(installs.get() + 1), None, None, false);
    }

    /// Review F1: on in web.json, the companion starts once, START_AFTER after pocket-ui, and the loop turns that
    /// follow only publish (the old code recursed between start and sync until the stack ran out).
    #[test]
    fn on_at_the_start_starts_once_after_three_seconds() {
        let (mut ui, _root_dir) = web_ui("boot", true);
        let installs = std::cell::Cell::new(0);
        loop_turn(&mut ui, Duration::from_millis(500), &installs);
        assert!(ui.server.is_none(), "not before the first frames");
        for i in 0..200 {
            loop_turn(&mut ui, START_AFTER + Duration::from_millis(i), &installs);
        }
        assert!(ui.server.is_some());
        assert_eq!(installs.get(), 1, "started once");
        assert!(page_answers(&ui));
        ui.stop();
        assert!(ui.server.is_none());
    }

    /// Off means nothing listens, on the Wi-Fi too: the Wi-Fi socket closes at once and the page's Wi-Fi address goes.
    #[test]
    fn off_nothing_listens_on_the_wifi() {
        let (mut ui, _root_dir) = web_ui("off-wifi", false);
        ui.listen = Some(vec![web::net::Listen { addr: "127.0.0.1:0".parse().unwrap(), iface: "lo".into(), index: 0, lan: true }]);
        let installs = std::cell::Cell::new(0);
        loop_turn(&mut ui, Duration::from_secs(600), &installs);
        assert!(ui.server.is_none() && ui.shared.is_none(), "off from the start: nothing at all");
        assert!(ui.set_prefs(Prefs { enabled: true, ..Prefs::default() }, |_| installs.set(installs.get() + 1)));
        assert!(page_answers(&ui));
        let shared = ui.shared.clone().unwrap();
        let addr = shared.lan.lock().unwrap().first().copied().expect("the Wi-Fi socket");
        ui.set_prefs(Prefs::default(), |_| unreachable!("switching off starts nothing"));
        assert!(shared.lan.lock().unwrap().is_empty() && shared.listening.lock().unwrap().is_empty());
        assert!(std::net::TcpStream::connect(addr).is_err(), "nothing listens on the Wi-Fi once it is off");
    }

    /// Review F1: switched on in Settings long after the start, off, and on again.
    #[test]
    fn switched_on_later_off_and_on_again() {
        let (mut ui, root_dir) = web_ui("toggle", false);
        let installs = std::cell::Cell::new(0);
        loop_turn(&mut ui, Duration::from_secs(600), &installs);
        assert!(ui.server.is_none(), "off: nothing runs");
        assert!(ui.shared.is_none(), "off: not even the shared state");
        assert!(ui.set_prefs(Prefs { enabled: true, ..Prefs::default() }, |_| installs.set(installs.get() + 1)));
        assert!(ui.server.is_some());
        for _ in 0..50 {
            loop_turn(&mut ui, Duration::from_secs(600), &installs);
        }
        assert_eq!(installs.get(), 1);
        assert!(page_answers(&ui));
        assert!(Prefs::load(&root_dir.path().join("data")).enabled, "saved");
        ui.set_prefs(Prefs::default(), |_| unreachable!("switching off starts nothing"));
        assert!(ui.server.is_none());
        assert!(ui.shared.as_ref().unwrap().listening.lock().unwrap().is_empty(), "the sockets are closed");
        ui.set_prefs(Prefs { enabled: true, ..Prefs::default() }, |_| installs.set(installs.get() + 1));
        assert!(page_answers(&ui), "on again");
        ui.stop();
    }

    /// A start that failed is not tried again on every frame.
    #[test]
    fn a_failed_start_waits() {
        let (mut ui, _root_dir) = web_ui("failed", true);
        ui.failed_at = Some(Instant::now());
        assert!(!ui.due(Duration::from_secs(600)));
        ui.failed_at = Some(Instant::now() - RETRY_AFTER);
        assert!(ui.due(Duration::from_secs(600)));
    }

    /// What the App publishes reaches the server: the game, and whether the screen is private (F4). Pairing makes
    /// the screen private before the code exists.
    #[test]
    fn publishing_the_game_and_a_private_screen() {
        let (mut ui, _root_dir) = web_ui("publish", true);
        ui.start(|_| {});
        let shared = ui.shared.clone().unwrap();
        let path = PathBuf::from("/storage/roms/gba/Golden Sun.gba");
        ui.publish(Some(Playing { system: "gba", name: "Golden Sun", path: &path, started: Instant::now() }), None, false);
        assert!(shared.playing());
        assert_eq!(shared.now_playing.lock().unwrap().as_ref().map(|n| n.name.clone()), Some("Golden Sun".into()));
        ui.publish(None, None, true);
        assert!(!shared.playing());
        assert!(shared.screen_private.load(Ordering::Relaxed));
        ui.publish(None, None, false);
        ui.start_pairing();
        assert!(shared.screen_private.load(Ordering::Relaxed), "private before the code is shown");
        assert!(shared.pairing.lock().unwrap().is_some());
        ui.stop();
        assert!(shared.pairing.lock().unwrap().is_none(), "switching off ends the code");
    }
}
