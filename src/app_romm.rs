//! Part of the pocket-ui binary: the App's side of `pocket_ui::romm` and `pocket_ui::romm_page` (docs/ROMM.md).
//! Settings → Games → RomM with its pairing screen (the code, a QR code of RomM's approval page), the list's refresh (after
//! pairing, from the page, and once a while after each start when online), and a RomM game's download at its first start:
//! the launch's preparation fetches it (`app_launch::prepare`) while the start's scene runs, with its bar over the scene
//! as Java's first load has (`app_loading`), and B stops it. Every request runs on a thread; the UI thread hands work over
//! and paints.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use pocket_ui::gfx::Canvas;
use pocket_ui::pad::Button;
use pocket_ui::palette::*;
use pocket_ui::romm::{self, Account, Catalog, Entry, Failure, Poll, Progress};
use pocket_ui::romm_page::{self, Action, Event, Row, RommPage, Summary, View};
use pocket_ui::text::Face;
use pocket_ui::touch::Area;

use super::{App, SettingsPage, UserEvent, CANCEL, CONFIRM, NU_BTN_PAD, NU_PAD};

/// The name in Settings (a product's name: the same in every language).
pub(crate) const SETTINGS_NAME: &str = "RomM";
/// The typed address before it is paired, in `<data>/romm/`.
const SERVER_FILE: &str = "server";
/// After a start, the list is read again this long after the first frame (when paired and online, no game running).
const REFRESH_AFTER_START: Duration = Duration::from_secs(40);

/// News from the threads.
#[derive(Debug)]
pub(crate) enum RommNews {
    Page(Event),
    /// Paired: the account and its token (already kept on the card by the thread).
    Paired(Box<Result<(Account, String), Failure>>),
    /// The list was read (and kept), or why not.
    Listed(Result<Arc<Catalog>, Failure>),
    /// Covers came from RomM: the shelf looks again.
    Covers,
    /// Time for the list's read after a start.
    StartRefresh,
    /// Unpaired: the account, token and list are gone from the card; the shelf is read again.
    Forgotten,
}

/// One RomM game's download at its first start, shared by the launch's preparation and the screen.
pub(crate) struct RommFetch {
    pub progress: Progress,
    /// The download began (the file wasn't there): the bar shows.
    pub started: AtomicBool,
    entry: Entry,
    server: String,
    token: String,
}

impl RommFetch {
    /// Runs in the launch's preparation: the game is fetched unless it is on the card already.
    pub(crate) fn run(&self, path: &Path) -> Result<(), Failure> {
        if path.exists() {
            return Ok(());
        }
        self.started.store(true, Ordering::Relaxed);
        println!("romm: downloading {} ({} bytes)", self.entry.file, self.entry.size);
        let net = pocket_ui::romm_net::system();
        let result = romm::download(&*net, &self.server, &self.token, &self.entry, path, &self.progress);
        match &result {
            Ok(()) => println!("romm: {} downloaded", self.entry.file),
            Err(f) => println!("romm: {} not downloaded: {f:?}", self.entry.file),
        }
        result
    }

    /// "4.2 MB / 312 MB": no words, so nothing to translate (a pattern "{s} of {s2}" turned "The Legend of Zelda" German).
    pub(crate) fn line(&self) -> String {
        let (done, total) = (self.progress.done.load(Ordering::Relaxed), self.progress.total.load(Ordering::Relaxed));
        if self.progress.stop.load(Ordering::Relaxed) {
            return "Stopping".into();
        }
        if total == 0 {
            return "Asking RomM".into();
        }
        format!("{} / {}", pocket_ui::storage_space::size(done), pocket_ui::storage_space::size(total))
    }
}

/// What the App keeps of RomM: the pairing, the list for starts, the open page.
pub(crate) struct RommUi {
    pub(crate) account: Option<Account>,
    token: Option<String>,
    pub(crate) catalog: Option<Arc<Catalog>>,
    /// The address typed before pairing.
    server: Option<String>,
    page: Option<RommPage>,
    refreshing: bool,
    /// The download of the start under way, if it is a RomM game.
    pub(crate) fetch: Option<Arc<RommFetch>>,
}

impl RommUi {
    pub(crate) fn load(data_dir: &Path) -> RommUi {
        let account = romm::account(data_dir);
        let server = std::fs::read_to_string(romm::dir(data_dir).join(SERVER_FILE)).ok().and_then(|s| romm::normalize_server(&s).ok());
        RommUi {
            token: account.as_ref().and_then(|_| romm::token(data_dir)),
            catalog: account.as_ref().and_then(|_| romm::load_catalog(data_dir)).map(Arc::new),
            account,
            server,
            page: None,
            refreshing: false,
            fetch: None,
        }
    }
}

/// The name RomM shows for this Pocket in its devices.
fn device_name() -> String {
    match pocket_ui::device::current().model {
        Some(m) => format!("pocket-ui ({})", m.name()),
        None => "pocket-ui".into(),
    }
}

fn spawn(name: &str, f: impl FnOnce() + Send + 'static) {
    if let Err(e) = std::thread::Builder::new().name(name.into()).spawn(f) {
        eprintln!("romm: can't start a thread for {name}: {e}");
    }
}

impl App {
    fn send_romm(proxy: &winit::event_loop::EventLoopProxy<UserEvent>, n: RommNews) {
        let _ = proxy.send_event(UserEvent::Romm(n));
    }

    /// "12 games >", "Not paired >" for the Settings list, from memory.
    pub(crate) fn romm_list_value(&self) -> String {
        let r = self.romm.get();
        match (&r.account, &r.catalog) {
            (Some(_), Some(c)) => format!("{} >", romm_page::games_line(c.games.len())),
            (Some(_), None) => "Paired >".into(),
            (None, _) => "Not paired >".into(),
        }
    }

    pub(crate) fn open_romm(&mut self) -> Option<SettingsPage> {
        let r = self.romm.get_mut();
        let summary = r.catalog.as_deref().map(Summary::of);
        let mut page = RommPage::new(r.account.clone(), r.server.clone(), summary);
        if r.refreshing {
            page.busy = Some("Reading the list of games");
        }
        r.page = Some(page);
        Some(SettingsPage::Romm)
    }

    pub(crate) fn romm_input(&mut self, b: Button) -> Option<SettingsPage> {
        let Some(page) = self.romm.get_mut().page.as_mut() else {
            return Some(SettingsPage::List(super::settings_row(SETTINGS_NAME)));
        };
        let action = page.press(b, CONFIRM, CANCEL);
        let cancel = page.cancel.clone();
        let (proxy, data) = (self.proxy.clone(), self.data_dir.clone());
        match action {
            Action::Nothing => {}
            Action::Close => {
                self.romm.get_mut().page = None;
                return Some(SettingsPage::List(super::settings_row(SETTINGS_NAME)));
            }
            Action::SaveServer(server) => {
                self.romm.get_mut().server = Some(server.clone());
                spawn("romm-server", move || {
                    let _ = std::fs::create_dir_all(romm::dir(&data));
                    let _ = pocket_ui::fsutil::write_atomic(&romm::dir(&data).join(SERVER_FILE), server.as_bytes());
                });
            }
            Action::StartPairing { server } => {
                self.romm.get_mut().server = Some(server.clone());
                spawn("romm-pair", move || {
                    let net = pocket_ui::romm_net::system();
                    let _ = std::fs::create_dir_all(romm::dir(&data));
                    let _ = pocket_ui::fsutil::write_atomic(&romm::dir(&data).join(SERVER_FILE), server.as_bytes());
                    let result = pair_by_qr(&*net, &data, &server, &cancel, &|e| Self::send_romm(&proxy, RommNews::Page(e)));
                    Self::send_romm(&proxy, RommNews::Paired(Box::new(result)));
                });
            }
            Action::ExchangeCode { server, code } => {
                spawn("romm-code", move || {
                    let net = pocket_ui::romm_net::system();
                    let result = romm::exchange_code(&*net, &server, &code).and_then(|(token, device_id)| finish_pairing(&*net, &data, &server, token, device_id));
                    Self::send_romm(&proxy, RommNews::Paired(Box::new(result)));
                });
            }
            Action::Refresh => self.romm_refresh(true),
            Action::Unpair => {
                let r = self.romm.get_mut();
                println!("romm: unpaired from {}", r.account.as_ref().map_or("", |a| a.server.as_str()));
                r.account = None;
                r.token = None;
                r.catalog = None;
                let dirs: Vec<PathBuf> = pocket_ui::library::all_systems(&self.es_systems).unwrap_or_default().into_iter().map(|s| s.path).collect();
                let proxy = self.proxy.clone();
                // The shelf is read again only once the files are gone (a scan before would still find the list).
                spawn("romm-unpair", move || {
                    romm::forget(&data);
                    romm::forget_parts(&dirs);
                    Self::send_romm(&proxy, RommNews::Forgotten);
                });
            }
        }
        Some(SettingsPage::Romm)
    }

    /// Reads the server's list on a thread (once at a time). `asked`: from the page (said there), else quietly.
    pub(crate) fn romm_refresh(&mut self, asked: bool) {
        let r = self.romm.get_mut();
        let (Some(account), Some(token)) = (r.account.clone(), r.token.clone()) else { return };
        if r.refreshing {
            return;
        }
        r.refreshing = true;
        if asked {
            if let Some(p) = r.page.as_mut() {
                p.busy = Some("Reading the list of games");
            }
        }
        let (proxy, data, es) = (self.proxy.clone(), self.data_dir.clone(), self.es_systems.clone());
        spawn("romm-list", move || {
            let net = pocket_ui::romm_net::system();
            let systems = pocket_ui::library::all_systems(&es).unwrap_or_default();
            let result = romm::fetch_catalog(&*net, &account.server, &token, &systems, &AtomicBool::new(false)).and_then(|cat| {
                romm::save_catalog(&data, &cat).map_err(|_| Failure::CantWrite)?;
                Ok(Arc::new(cat))
            });
            match &result {
                Ok(c) => println!("romm: list read, {} games ({} not for this Pocket)", c.games.len(), c.skipped),
                Err(f) => println!("romm: list not read: {f:?}"),
            }
            let cat = result.as_ref().ok().cloned();
            Self::send_romm(&proxy, RommNews::Listed(result));
            // Then RomM's covers for the games that have none yet, quietly, never while a game runs.
            let Some(cat) = cat else { return };
            let dirs: std::collections::HashMap<String, PathBuf> = systems.into_iter().map(|s| (s.id, s.path)).collect();
            let mut got = 0;
            for e in &cat.games {
                if pocket_ui::covers::game_on() {
                    break;
                }
                let Some(dir) = dirs.get(&e.system) else { continue };
                if romm::fetch_cover(&*net, &account.server, &token, &data, e, &e.path_in(dir)) {
                    got += 1;
                }
            }
            if got > 0 {
                println!("romm: {got} covers from RomM");
                Self::send_romm(&proxy, RommNews::Covers);
            }
        });
    }

    pub(crate) fn romm_news(&mut self, n: RommNews) {
        match n {
            RommNews::Page(e) => {
                if let Some(p) = self.romm.get_mut().page.as_mut() {
                    p.event(e);
                }
            }
            RommNews::Paired(result) => {
                let r = self.romm.get_mut();
                match *result {
                    Ok((account, token)) => {
                        println!("romm: paired with {}", account.server);
                        r.account = Some(account.clone());
                        r.token = Some(token);
                        r.catalog = None;
                        if let Some(p) = r.page.as_mut() {
                            p.event(Event::Paired(Ok(account)));
                        }
                        self.romm_refresh(false);
                    }
                    Err(f) => {
                        if let Some(p) = r.page.as_mut() {
                            p.event(Event::Paired(Err(f)));
                        }
                    }
                }
            }
            RommNews::Listed(result) => {
                let r = self.romm.get_mut();
                r.refreshing = false;
                let summary = result.as_ref().map(|c| Summary::of(c)).map_err(Clone::clone);
                if let Ok(cat) = &result {
                    r.catalog = Some(cat.clone());
                }
                if let Some(p) = r.page.as_mut() {
                    p.event(Event::Listed(summary));
                }
                if result.is_ok() {
                    self.library_rescan("RomM list read");
                }
            }
            RommNews::Covers => {
                self.covers.clear();
                self.pictures_changed();
                self.cabinet.covers_changed();
                self.look_previews = None;
            }
            RommNews::Forgotten => {
                self.library_rescan("RomM unpaired");
            }
            RommNews::StartRefresh => {
                if self.romm.loaded().is_none_or(|r| r.account.is_none()) {
                    return;
                }
                let online = pocket_ui::status_corner::status().wifi || pocket_ui::device::android_profile();
                if self.running.is_none() && self.pending_launch.is_none() && online {
                    self.romm_refresh(false);
                }
            }
        }
        self.request_redraw();
    }

    /// After the first frame: the list is read once again a while later, when paired.
    pub(crate) fn romm_after_start(&mut self) {
        // Whether it is paired is asked when the time comes (`StartRefresh`): nothing of `<data>/romm/` is read here.
        let proxy = self.proxy.clone();
        spawn("romm-later", move || {
            std::thread::sleep(REFRESH_AFTER_START);
            Self::send_romm(&proxy, RommNews::StartRefresh);
        });
    }

    /// A start of `path`: its download when it is a RomM game (the preparation fetches it unless it is on the card).
    pub(crate) fn romm_fetch_for(&mut self, system: &str, path: &Path) -> Option<Arc<RommFetch>> {
        // `get`: a start right after the first frame waits the few milliseconds the read of `<data>/romm/` may still take.
        let r = self.romm.get();
        let (Some(account), Some(token), Some(cat)) = (&r.account, &r.token, &r.catalog) else { return None };
        let entry = cat.find(system, path)?.clone();
        Some(Arc::new(RommFetch { progress: Progress::default(), started: AtomicBool::new(false), entry, server: account.server.clone(), token: token.clone() }))
    }

    /// The start's download is under way (its bar shows instead of "Loading the game…").
    pub(crate) fn romm_downloading(&self) -> bool {
        self.romm.loaded().and_then(|r| r.fetch.as_ref()).is_some_and(|f| f.started.load(Ordering::Relaxed))
    }

    /// B while it downloads: it stops at the next piece; the start ends with the note, and the part stays for the next one.
    pub(crate) fn romm_press(&mut self, b: Button) -> bool {
        let Some(f) = self.romm.loaded().and_then(|r| r.fetch.as_ref()).filter(|f| b == CANCEL && f.started.load(Ordering::Relaxed)) else { return false };
        println!("romm: download stopped with B");
        f.progress.stop.store(true, Ordering::Relaxed);
        self.request_redraw();
        true
    }

    /// "Getting the game from RomM", the game's name, a bar and how far; B to stop. In the dark plate of Java's load.
    pub(crate) fn paint_romm_fetch(&mut self, c: &mut Canvas, fetch: &RommFetch) {
        const TITLE: &str = "Getting the game from RomM, once";
        let line = fetch.line();
        let tw = self.text.width(Face::Body, 28, TITLE);
        let lw = self.text.width(Face::Body, 24, &line);
        let w = tw.max(lw).max(360).min(c.w - 2 * pocket_ui::kit::PAD);
        let (px, small) = (pocket_ui::type_scale::px(28) as i32, pocket_ui::type_scale::px(24) as i32);
        let h = px + 22 + 12 + 20 + small + 20;
        let x = (c.w - w) / 2;
        let y = pocket_ui::kit::hint_bar_top(c.h) - 24 - h;
        let fraction = fetch.progress.fraction();
        pocket_ui::palette::scoped(0, || {
            c.round_rect(x - 24, y, w + 48, h + 16, 12, 0x161616);
            let mut at = y + 12 + px * 4 / 5;
            self.text.draw(c, Face::Body, 28, x, at, pal().chalk, TITLE);
            at += 22;
            c.round_rect(x, at, w, 12, 6, 0x3a3a3a);
            let filled = (w as f32 * fraction) as i32;
            if filled > 0 {
                c.round_rect(x, at, filled.max(12), 12, 6, pal().chalk);
            }
            at += 12 + 20 + small * 4 / 5;
            let line = self.text.fit(Face::Body, 24, &line, w);
            self.text.draw(c, Face::Body, 24, x, at, pal().dust, &line);
        });
        self.paint_hint_row(c, &[("B", "Stop")]);
    }

    /// Something moves on the page: a thread works, or the pairing waits for its code or the approval.
    pub(crate) fn romm_animating(&self) -> bool {
        self.romm.loaded().and_then(|r| r.page.as_ref()).is_some_and(|p| p.busy.is_some() || matches!(p.view, View::Pairing { .. }))
    }

    /// Where the page's selection stands: a change is what the menu sounds answer.
    pub(crate) fn romm_row(&self) -> Option<(usize, bool, u8)> {
        self.romm.loaded().and_then(|r| r.page.as_ref()).map(|p| (p.sel, p.paired(), match p.view {
            View::List => 0,
            View::Pairing { .. } => 1,
            View::Keyboard(..) => 2,
        }))
    }

    /// `--snapshot romm[:paired|:pairing|:code|:typing|:error|:old|:signedout|:busy|:unpair|:nolist|:empty]` draws the page with
    /// made-up data.
    pub(crate) fn romm_snapshot(&mut self, state: &str) -> bool {
        let Some(rest) = state.strip_prefix("romm") else { return false };
        let mode = rest.trim_start_matches(':');
        let account = Account { server: "http://192.0.2.10:8080".into(), user: "sam".into(), device_id: None };
        let summary = Summary { games: 214, systems: 9, skipped: 12, fetched: now_secs().saturating_sub(300), cut: false };
        let paired = matches!(mode, "paired" | "busy" | "unpair" | "signedout" | "nolist" | "empty");
        let server = (!mode.is_empty() && mode != "typing").then(|| account.server.clone());
        let summary = match mode {
            "nolist" => None,
            "empty" => Some(Summary { games: 0, systems: 0, skipped: 7, ..summary }),
            _ => paired.then_some(summary),
        };
        let mut page = RommPage::new(paired.then(|| account.clone()), server, summary);
        let (a, b) = (Button::East, Button::South);
        match mode {
            "pairing" => {
                page.sel = 1;
                page.press(a, a, b);
            }
            "code" => {
                page.sel = 1;
                page.press(a, a, b);
                page.event(Event::Code(Ok(romm::Pairing {
                    device_code: "d".into(),
                    user_code: "M9JXERPY".into(),
                    url: "http://192.0.2.10:8080/pair/device?user_code=M9JXERPY".into(),
                    interval: 5,
                    expires_in: 600,
                })));
            }
            "typing" => {
                page.press(a, a, b);
                if let View::Keyboard(_, kb, _) = &mut page.view {
                    kb.text = "192.0.2.10:8080".into();
                }
            }
            "error" => page.error = Some(Failure::Unreachable.words()),
            "old" => {
                page.sel = 1;
                page.press(a, a, b);
                page.event(Event::Code(Err(Failure::OldServer)));
            }
            "signedout" => page.error = Some(Failure::SignedOut.words()),
            "busy" => page.busy = Some("Reading the list of games"),
            "unpair" => {
                page.sel = 2;
                page.press(a, a, b);
            }
            _ => {}
        }
        let r = self.romm.get_mut();
        r.account = paired.then_some(account);
        r.page = Some(page);
        self.settings_page = Some(SettingsPage::Romm);
        true
    }

    pub(crate) fn paint_romm(&mut self, c: &mut Canvas) {
        let Some(page) = self.romm.get_mut().page.take() else { return };
        c.clear(pal().void);
        match &page.view {
            View::List => self.paint_romm_list(c, &page),
            View::Pairing { code } => self.paint_romm_pairing(c, code.as_ref()),
            View::Keyboard(typing, kb, _) => {
                let (heading, note) = match typing {
                    romm_page::Typing::Server => ("RomM server", "The address you open RomM at in a browser, e.g. 192.0.2.10:8080 or romm.example.com."),
                    romm_page::Typing::Code => ("Pairing code", "In RomM: your profile › Client API Tokens › Pair. The code works once, for 5 minutes."),
                };
                self.text.draw(c, Face::Title, 50, MARGIN, 110, pal().chalk, heading);
                pocket_ui::setup::paint_keyboard_noted(c, &mut self.text, kb, "Done", note);
                pocket_ui::setup::hints(c, &mut self.text, &[("A", "Type"), ("B", "Delete"), ("X", "Shift"), ("Y", "Symbols"), ("START", "Done")]);
            }
        }
        self.romm.get_mut().page = Some(page);
    }

    fn paint_romm_list(&mut self, c: &mut Canvas, page: &RommPage) {
        let (pad, font) = (NU_PAD, 34u32);
        self.sheet_header(c, SETTINGS_NAME);
        let hints = [("B", "Back"), ("A", if page.row() == Row::Server { "Change" } else { "Open" })];
        let foot_top = pocket_ui::kit::footer(c, &mut self.text, &mut self.hits, &hints, page.describe(page.row()), 2, pal().text);
        let (top, right) = (pocket_ui::kit::header_bottom(c.h) + 12, c.w - pad);
        let rows = page.rows();
        let row = pocket_ui::type_scale::row(54, font);
        for (i, r) in rows.iter().enumerate() {
            let y = top + i as i32 * row;
            self.hits.row(Area::new(pad, y, right - pad, row), i);
            let base = y + row / 2 + pocket_ui::type_scale::mid(font);
            let selected = i == page.sel;
            if selected {
                pocket_ui::kit::selection(c, pad, y, right - pad, row);
            }
            let name = page.label(*r);
            self.text.draw(c, Face::Body, font, pad + NU_BTN_PAD, base, if selected { pal().ink } else { pal().text }, name);
            let name_w = self.text.width(Face::Body, font, name);
            let value = self.text.fit(Face::Body, font, &page.value(*r), (right - pad - 3 * NU_BTN_PAD - name_w - 40).max(80));
            pocket_ui::kit::row_value(c, &mut self.text, font, right - NU_BTN_PAD, base, &value, selected);
        }
        self.hits.list(page.sel, rows.len(), 0.0, row as f32);
        self.paint_romm_status(c, page, top + rows.len() as i32 * row + 14, foot_top);
    }

    /// Under the rows: what the Pocket is paired with and how the list looks, or what went wrong, or what this is.
    fn paint_romm_status(&mut self, c: &mut Canvas, page: &RommPage, y0: i32, limit: i32) {
        let left = NU_PAD + NU_BTN_PAD;
        let width = c.w - NU_PAD - NU_BTN_PAD - left;
        let short = pocket_ui::kit::short_screen(c.h);
        let panel_h = (if short { 96 } else { 104 }) + pocket_ui::type_scale::grow(44);
        pocket_ui::kit::panel(c, NU_PAD, y0 - 10, c.w - 2 * NU_PAD, panel_h);
        self.text.draw(c, Face::Body, 24, left, y0 + 20, pal().dust, "Status");
        let dots = ".".repeat(1 + (self.started.elapsed().as_millis() / 400 % 3) as usize);
        let (status, colour) = match (&page.error, page.busy, &page.account) {
            (_, Some(b), _) => (format!("{b}{dots}"), pal().dust),
            (Some(_), None, _) => ("Something went wrong".to_string(), pal().alert),
            (None, None, Some(a)) if !a.user.is_empty() => (format!("Paired as {}", a.user), pal().chalk),
            (None, None, Some(_)) => ("Paired".to_string(), pal().chalk),
            (None, None, None) => ("Not paired".to_string(), pal().dust),
        };
        let status = self.text.fit(Face::Title, 44, &status, width);
        self.text.draw(c, Face::Title, 44, left, y0 + 64, colour, &status);
        let mut y = y0 - 10 + panel_h + if short { 30 } else { 44 };
        let mut lines: Vec<(String, u32)> = Vec::new();
        if let Some(e) = &page.error {
            lines.push((e.clone(), pal().text));
        }
        match (&page.account, &page.summary) {
            (Some(_), Some(s)) => {
                lines.push((format!("{} · {}", romm_page::summary_line(s), romm_page::age_line(s.fetched, now_secs())), pal().dust));
                if s.games == 0 {
                    lines.push(("RomM has no games for the systems this Pocket plays.".into(), pal().dust));
                }
                if s.skipped > 0 {
                    lines.push((format!("{} more are for systems this Pocket doesn't play.", s.skipped), pal().dust));
                }
                if s.cut {
                    lines.push(("Only the first 50000 are on the shelf.".into(), pal().dust));
                }
                lines.push(("Games download on first launch, then stay cached on your SD card.".into(), pal().dust));
            }
            (Some(_), None) => lines.push(("The list of games hasn't been read yet.".into(), pal().dust)),
            (None, _) => {
                if pocket_ui::device::no_wifi_chip() {
                    lines.push((pocket_ui::device::NEEDS_NETWORK.into(), pal().dust));
                }
                lines.push(("RomM is a self-hosted game server. Pair once and your RomM games appear right beside local ones.".into(), pal().dust));
            }
        }
        for (text, colour) in lines {
            let px = 24;
            let step = pocket_ui::type_scale::line(px, 8);
            let wrapped = self.text.wrap_prose(Face::Body, px, &text, width);
            if y + (wrapped.len() as i32 - 1) * step + 12 > limit {
                break;
            }
            for l in wrapped {
                self.text.draw(c, Face::Body, px, left, y, colour, &l);
                y += step;
            }
            y += 10;
        }
    }

    /// The pairing screen: RomM's approval page as a QR code, its code large, and what to do.
    fn paint_romm_pairing(&mut self, c: &mut Canvas, code: Option<&romm::Pairing>) {
        let pad = NU_PAD;
        self.sheet_header(c, "Pair with RomM");
        let (left, top) = (pad + NU_BTN_PAD, pocket_ui::kit::header_bottom(c.h) + 30);
        let dots = ".".repeat(1 + (self.started.elapsed().as_millis() / 400 % 3) as usize);
        match code {
            Some(p) => {
                // The words beside the code are measured first: the QR code gets smaller until they all stand above the
                // hint bar, in every text size and language (Large German at 1024x768 ran the last line into the bar).
                let limit = pocket_ui::kit::hint_bar_top(c.h) - 16;
                let page = romm::shown_server(p.url.split('?').next().unwrap_or(&p.url)).to_string();
                let waiting = format!("Waiting for approval{dots}");
                let steps = ["1.  Open the camera on your phone.", "2.  Point it at the code.", "3.  Sign in to RomM and approve this Pocket."];
                let mut qr = (c.h - top - 140).clamp(200, 520);
                let lines = loop {
                    let tx = left + qr + 50;
                    let width = c.w - pad - NU_BTN_PAD - tx;
                    // (face, px, colour, text, baseline)
                    let mut out: Vec<(Face, u32, u32, String, i32)> = Vec::new();
                    let mut y = top + pocket_ui::type_scale::px(30) as i32;
                    for s in steps {
                        for line in self.text.wrap_prose_at_most(Face::Body, 30, s, width, 2) {
                            out.push((Face::Body, 30, pal().text, line, y));
                            y += pocket_ui::type_scale::line(30, 10);
                        }
                        y += 12;
                    }
                    y += 12;
                    for line in self.text.wrap_prose_at_most(Face::Body, 24, "Or open this page and type the code:", width, 2) {
                        out.push((Face::Body, 24, pal().dust, line, y));
                        y += pocket_ui::type_scale::line(24, 10);
                    }
                    // The page's address whole (it is typed from here): a wider QR code cut it at 1024x768 (0.6.1).
                    let whole = self.text.width(Face::Body, 26, &page) <= width;
                    // Cut only where nothing smaller is left to try (a cut measured on the way would be noted as one).
                    let shown = if whole || qr <= 200 { self.text.fit(Face::Body, 26, &page, width) } else { page.clone() };
                    out.push((Face::Body, 26, pal().text, shown, y + 4));
                    y += pocket_ui::type_scale::line(26, 10) + pocket_ui::type_scale::px(50) as i32 - 6;
                    // As RomM's page takes it: the letters alone, no space a player might type along.
                    out.push((Face::Title, 50, pal().chalk, self.text.fit(Face::Title, 50, &p.user_code, width), y));
                    y += pocket_ui::type_scale::line(28, 22);
                    for line in self.text.wrap_prose_at_most(Face::Body, 28, &waiting, width, 2) {
                        out.push((Face::Body, 28, pal().dust, line, y));
                        y += pocket_ui::type_scale::line(28, 10);
                    }
                    // The last baseline's descenders, then the bar.
                    let bottom = y - pocket_ui::type_scale::line(28, 10) + pocket_ui::type_scale::px(28) as i32 / 3;
                    if (bottom <= limit && whole) || qr <= 200 {
                        break (tx, out);
                    }
                    qr -= 20;
                };
                super::app_sync::paint_qr(c, left, top, qr, &p.url);
                pocket_ui::shares::mark("romm-qr", left, top, qr, qr);
                let (tx, out) = lines;
                // For tests/brick_proportions.rs: the page's address stands whole beside the code.
                let page_w = self.text.width(Face::Body, 26, &page);
                if page_w <= c.w - pad - NU_BTN_PAD - tx {
                    pocket_ui::shares::mark("romm-page", tx, top, page_w, 1);
                }
                for (face, px, colour, line, y) in out {
                    self.text.draw(c, face, px, tx, y, colour, &line);
                }
            }
            None => {
                self.text.draw(c, Face::Title, 44, left, top + 80, pal().chalk, &format!("Asking RomM for a code{dots}"));
            }
        }
        self.nu_hints(c, &[("B", "Cancel")]);
    }
}

fn now_secs() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// RomM's device flow to its end: the code for the screen, then a question every few seconds until the player approved it
/// on the phone, declined it, the code ran out or B ended it. Blocks: the pairing thread's.
fn pair_by_qr(net: &dyn romm::Net, data: &Path, server: &str, cancel: &AtomicBool, say: &dyn Fn(Event)) -> Result<(Account, String), Failure> {
    let ident = romm::device_identifier(data);
    let p = match romm::device_init(net, server, &ident, &device_name()) {
        Ok(p) => p,
        Err(f) => {
            say(Event::Code(Err(f.clone())));
            return Err(Failure::Stopped);
        }
    };
    say(Event::Code(Ok(p.clone())));
    let until = std::time::Instant::now() + Duration::from_secs(p.expires_in);
    let mut interval = Duration::from_secs(p.interval);
    loop {
        let next = std::time::Instant::now() + interval;
        while std::time::Instant::now() < next {
            if cancel.load(Ordering::Relaxed) {
                return Err(Failure::Stopped);
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        if std::time::Instant::now() > until {
            return Err(Failure::Expired);
        }
        match romm::device_poll(net, server, &p.device_code) {
            Poll::Waiting => {}
            Poll::SlowDown => interval += Duration::from_secs(5),
            Poll::Approved { token, device_id } => return finish_pairing(net, data, server, token, device_id),
            Poll::Failed(f) => return Err(f),
        }
    }
}

/// A token came: who it is for, then kept on the card (the token readable by its owner only).
fn finish_pairing(net: &dyn romm::Net, data: &Path, server: &str, token: String, device_id: Option<String>) -> Result<(Account, String), Failure> {
    // `me.read` may be left out of what was approved: the name is a nicety, not a condition.
    let user = match romm::whoami(net, server, &token) {
        Ok(u) => u,
        Err(Failure::SignedOut) => return Err(Failure::SignedOut),
        Err(_) => String::new(),
    };
    let account = Account { server: server.to_string(), user, device_id };
    romm::save_account(data, &account, &token).map_err(|_| Failure::CantWrite)?;
    let _ = std::fs::remove_file(romm::dir(data).join(SERVER_FILE));
    Ok((account, token))
}

#[cfg(test)]
mod file_source_tests {
    use super::*;

    /// The address typed before pairing, read back at start (`<data>/romm/server`), through the shared faults: never a
    /// panic, the good one is the server, a broken one is no address (typed again) rather than a wrong one.
    #[test]
    fn file_source_rule_romm_server_file() {
        let d = tempfile::tempdir().unwrap();
        let file = romm::dir(d.path()).join(SERVER_FILE);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        for (label, bytes) in crate::file_faults::faults(b"http://192.0.2.10:8080") {
            let _ = std::fs::remove_file(&file);
            if let Some(bytes) = bytes {
                std::fs::write(&file, bytes).unwrap();
            }
            let ui = RommUi::load(d.path());
            if label == "valid" {
                assert_eq!(ui.server.as_deref(), Some("http://192.0.2.10:8080"));
            }
            if matches!(label.as_str(), "missing" | "empty" | "encoding") {
                assert_eq!(ui.server, None, "{label}");
            }
            assert!(ui.account.is_none());
        }
    }
}

