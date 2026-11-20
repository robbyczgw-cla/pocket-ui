//! Part of the pocket-ui binary: the App's side of Pocket-to-Pocket sync (`pocket_ui::pocket_sync`, docs/SYNC-OWN.md).
//! Settings > Connections > Pockets: the switch, pairing with a code, the paired Pockets (A syncs now, X forgets).
//!
//! Threads: the service (`net::serve`) and the answer to "who is there?" (`net::answer_discovery`) run while the switch
//! is on; pairing, a sync and looking for Pockets nearby each run on a short thread of their own and report back
//! with a `UserEvent::PocketSync`. Nothing runs on the UI thread but reading the paired list. While a game runs the
//! service writes nothing (`busy`, set each loop turn from `App::running`).

use std::collections::HashSet;
use std::net::{SocketAddr, TcpListener, UdpSocket};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use pocket_ui::gfx::Canvas;
use winit::event_loop::EventLoopProxy;
use pocket_ui::pad::Button;
use pocket_ui::palette::*;
use pocket_ui::pocket_sync::net::{self, Me, Pairing, Peers, Report, Service, Store, SyncError};
use pocket_ui::pocket_sync::log::Log;
use pocket_ui::pocket_sync::Layout;
use pocket_ui::pocket_sync_page::{self as page, Action, Fault, Known, Mark, Nearby, Outcome, PocketPage, Prefs, Row, Seen, View, Why};
use pocket_ui::text::Face;
use pocket_ui::touch::Area;

use super::{App, SettingsPage, UserEvent, CANCEL, CONFIRM, NU_BTN_PAD, NU_PAD};

/// The name in the Settings list, to come back to.
pub(crate) const SETTINGS_NAME: &str = "Pocket Sync";
/// The service starts this long after pocket-ui (the shelf comes first).
const START_AFTER: Duration = Duration::from_secs(3);
/// Android: the service starts again this long after it stopped in the background (its sockets are closed by then).
const RESTART_AFTER: Duration = Duration::from_secs(1);

/// What a thread reports.
#[derive(Debug)]
pub(crate) enum Event {
    Paired(Result<String, SyncError>),
    Synced(String, Result<Report, SyncError>),
    Nearby(Vec<net::Found>),
    /// Every paired Pocket, after a game or a start: (id, name, result) each.
    All(Why, Vec<(String, String, Result<Report, SyncError>)>),
    /// Another Pocket sent saves here (its own sync after a game there): `Store::take_received` has them.
    Received,
}

/// The saves listed on a Pocket's page.
const SEEN_MAX: usize = 8;

pub(crate) struct PocketSyncUi {
    dir: PathBuf,
    prefs: Prefs,
    page: Option<PocketPage>,
    store: Option<Arc<Store>>,
    svc: Option<Arc<Service>>,
    stop: Option<Arc<AtomicBool>>,
    busy: Arc<AtomicBool>,
    /// Pockets nearby that show a code, with their addresses (the page gets names only).
    nearby: Vec<net::Found>,
    /// Set while the pairing view is open: the thread looking for Pockets nearby stops when it is cleared.
    looking: Option<Arc<AtomicBool>>,
    /// The Pockets a sync with runs now, by id.
    syncing: HashSet<String>,
    /// What came and went and how each Pocket stands (`pocket_sync::log`), read again after each sync.
    log: Log,
    problem: Option<String>,
    line: Option<String>,
    /// Snapshots and tests: no service, no threads.
    still: bool,
    /// A sync with every paired Pocket is running: a second one waits for the next occasion.
    all_running: Arc<AtomicBool>,
    /// The service started since the last sync with every Pocket: one runs when it is up.
    start_due: bool,
    /// Android: the service stopped when the view went to the background (it listens only in front). Its sockets close
    /// within a moment; it starts again a second later.
    stopped_at: Option<Instant>,
    /// The last sync with every Pocket began: the service coming up right after a game's sync needs no second one.
    last_all: Option<Instant>,
}

impl PocketSyncUi {
    pub(crate) fn load(data_dir: &Path) -> PocketSyncUi {
        let dir = data_dir.join("pocket-sync");
        PocketSyncUi {
            prefs: Prefs::load(&dir),
            log: Log::load(&dir),
            dir,
            page: None,
            store: None,
            svc: None,
            stop: None,
            busy: Arc::new(AtomicBool::new(false)),
            nearby: Vec::new(),
            looking: None,
            syncing: HashSet::new(),
            problem: None,
            line: None,
            // Snapshots and tests (`POCKETUI_MOCK`) run no service, unless a demo take asks for it.
            still: std::env::var_os("POCKETUI_MOCK").is_some() && std::env::var_os("POCKETUI_POCKET_SYNC_LIVE").is_none(),
            all_running: Arc::new(AtomicBool::new(false)),
            start_due: false,
            stopped_at: None,
            last_all: None,
        }
    }

    /// This Pocket's side: its id and name, the roms folder, its record.
    fn store(&mut self) -> Option<Arc<Store>> {
        if self.store.is_none() {
            #[cfg(not(target_os = "android"))]
            let name = pocket_ui::device::current().model.map_or("Pocket".to_string(), |m| m.name().to_string());
            // Android: the handheld's own name ("Retroid Pocket Mini V2"); no device tree to read it from.
            #[cfg(target_os = "android")]
            let name = Some(pocket_ui::render_scale::android_property("ro.product.model")).filter(|n| !n.trim().is_empty()).unwrap_or_else(|| "Pocket".into());
            let me = Me::load_or_new(&self.dir, Some(name.as_str())).ok()?;
            // `POCKETUI_POCKET_SYNC_ROMS`: the saves of a demo take's made-up library instead of the card's.
            #[cfg(not(target_os = "android"))]
            let layout = std::env::var_os("POCKETUI_POCKET_SYNC_ROMS").map_or_else(card_layout, |r| Layout::beside(PathBuf::from(r)));
            // Android's handheld view: the games in the ROMs folders, their saves in the player's folder by key.
            #[cfg(target_os = "android")]
            let layout = super::app_android::pocket_sync_layout();
            self.store = Some(Arc::new(Store::new(self.dir.clone(), layout, me)));
        }
        self.store.clone()
    }

    fn running(&self) -> bool {
        self.svc.is_some()
    }

    /// The service and the answer to "who is there?", on threads of their own.
    fn start(&mut self, proxy: &EventLoopProxy<UserEvent>) {
        if self.running() || self.still {
            return;
        }
        let Some(store) = self.store() else { return };
        let listener = match TcpListener::bind(("0.0.0.0", net::PORT)) {
            Ok(l) => l,
            Err(e) => {
                self.problem = Some(format!("Port {} is taken ({e}).", net::PORT));
                return;
            }
        };
        self.problem = None;
        let proxy = proxy.clone();
        store.on_received(Box::new(move || {
            let _ = proxy.send_event(UserEvent::PocketSync(Event::Received));
        }));
        let busy = self.busy.clone();
        let svc = Arc::new(Service::new(store, Box::new(move || busy.load(Ordering::Relaxed)), net::PORT));
        let stop = Arc::new(AtomicBool::new(false));
        let (s, t) = (svc.clone(), stop.clone());
        let _ = std::thread::Builder::new().name("pocket-sync".into()).spawn(move || net::serve(listener, s, t));
        if let Ok(u) = UdpSocket::bind(("0.0.0.0", net::DISCOVERY_PORT)) {
            let (s, t) = (svc.clone(), stop.clone());
            let _ = std::thread::Builder::new().name("pocket-sync-find".into()).spawn(move || net::answer_discovery(u, s, t));
        }
        self.svc = Some(svc);
        self.stop = Some(stop);
        self.start_due = true;
    }

    fn stop(&mut self) {
        if let Some(s) = self.stop.take() {
            s.store(true, Ordering::Relaxed);
        }
        if let Some(l) = self.looking.take() {
            l.store(false, Ordering::Relaxed);
        }
        self.svc = None;
    }

    fn peers(&self) -> Vec<net::Peer> {
        match &self.svc {
            Some(s) => s.peers.lock().map(|p| p.0.clone()).unwrap_or_default(),
            None => Peers::load(&self.dir).0,
        }
    }
}

/// Where this card keeps the games and their saves. ROCKNIX: the saves beside the games. The Brick and dArkOS (`brick`
/// builds, docs/TRIMUI-BRICK.md, docs/DARKOS.md): the games in the view's `/storage/roms/<system>`, read only, and
/// pocket-core's saves in `POCKET_CORE_SAVE_DIR/<system>` (`bin/pocket_core.rs`, `case_knows_read::save_places`).
#[cfg(not(target_os = "android"))]
fn card_layout() -> Layout {
    #[cfg(feature = "brick")]
    if let Some(saves) = std::env::var_os("POCKET_CORE_SAVE_DIR").filter(|s| !s.is_empty()) {
        return Layout::saves_root(PathBuf::from("/storage/roms"), PathBuf::from(saves));
    }
    Layout::rocknix()
}

/// Unix seconds now, and the local zone's offset (for "since 14:05").
fn now_and_offset() -> (u64, i32) {
    let now = pocket_ui::play_time::now_secs();
    (now, pocket_ui::timezone::utc_offset_now(now as i64))
}

/// One Pocket's sync, for the shelf line: the games by name.
fn outcome(name: &str, r: &Result<Report, SyncError>, game: &dyn Fn(&str) -> String) -> Option<Outcome> {
    let fault = match r {
        Err(SyncError::Unpaired) => Some(Fault::Unpaired),
        Err(SyncError::Bad) => Some(Fault::Bad),
        _ => None,
    };
    let rep = match r {
        Ok(rep) => rep,
        Err(_) if fault.is_some() => return Some(Outcome { name: name.into(), fault, ..Outcome::default() }),
        Err(_) => return None,
    };
    let games = rep.fetched.iter().chain(&rep.sent).map(|rel| game(rel)).collect();
    Some(Outcome { name: name.into(), sent: rep.sent.len(), fetched: rep.fetched.len(), asked: rep.asked.len(), games, failed: rep.failed.len(), fault })
}

impl App {
    /// The paired Pockets by the first seven letters of their id (the device part of a copy's name), for the page of
    /// two versions: "From TrimUI Brick".
    pub(crate) fn pocket_sync_names(&self) -> Vec<(String, String)> {
        self.pocket_sync.peers().into_iter().map(|p| (p.id.chars().take(7).collect::<String>().to_uppercase(), p.name)).collect()
    }

    /// This Pocket's side of sync, when it is on, for recording a choice made on the page of two versions.
    pub(crate) fn pocket_sync_store(&mut self) -> Option<Arc<Store>> {
        if !self.pocket_sync.prefs.enabled {
            return None;
        }
        self.pocket_sync.store()
    }

    /// Android: the player's key of a game (`pocket_sync::Layout::android`), for the "Two versions" page to find a save's game
    /// by its folder. None elsewhere (the saves are beside their games) and with Pockets off.
    pub(crate) fn pocket_sync_key(&mut self) -> Option<pocket_ui::pocket_sync::KeyFn> {
        if !self.pocket_sync_on() {
            return None;
        }
        self.pocket_sync_store()?.layout.keyed().map(|k| k.key.clone())
    }

    /// The switch, for the look for saves with two versions (`app_sync_conflicts`).
    pub(crate) fn pocket_sync_on(&self) -> bool {
        self.pocket_sync.prefs.enabled
    }

    /// A save's game by its name on the shelf (`gba/Golden Sun (USA, Europe).srm` → "Golden Sun"); a save of a game
    /// the library does not list, by its file name.
    pub(crate) fn pocket_sync_game(&self, rel: &str) -> String {
        self.systems
            .iter()
            .flat_map(|s| s.games.iter().map(move |g| (s, g)))
            .find(|(s, g)| pocket_ui::pocket_sync::log::save_of(rel, &s.name, &g.path))
            .map(|(_, g)| g.name.clone())
            .unwrap_or_else(|| {
                let file = rel.rsplit('/').next().unwrap_or(rel);
                file.rsplit_once('.').map_or(file, |(s, _)| s).to_string()
            })
    }

    /// The game card's Pocket Sync line: the last time a save of this game came from or went to a paired Pocket ("With
    /// TrimUI Brick at 14:05"). None with the switch off or before any.
    pub(crate) fn pocket_sync_card_line(&self, system: &str, path: &Path) -> Option<String> {
        let ui = &self.pocket_sync;
        if !ui.prefs.enabled {
            return None;
        }
        let e = ui.log.last_for(system, path)?;
        let name = ui.peers().into_iter().find(|p| p.id == e.peer)?.name;
        let (now, offset) = now_and_offset();
        Some(page::card_line(&name, e.at, now, offset))
    }

    pub(crate) fn pocket_sync_list_value(&self) -> String {
        web_on_off(self.pocket_sync.prefs.enabled)
    }

    pub(crate) fn open_pocket_sync(&mut self) -> Option<SettingsPage> {
        self.pocket_sync.page = Some(PocketPage::new(self.pocket_sync.prefs));
        self.pocket_sync_refresh();
        Some(SettingsPage::Pockets)
    }

    /// The page from the service: paired Pockets and their lines, the code and its time, a pairing that just happened.
    fn pocket_sync_refresh(&mut self) {
        let peers = self.pocket_sync.peers();
        let (now, offset) = now_and_offset();
        // The open Pocket's page: what came and went, by game.
        let open = match self.pocket_sync.page.as_ref().map(|p| (&p.view, &p.pockets)) {
            Some((View::Pocket(i), pockets)) => pockets.get(*i).map(|k| (k.id.clone(), k.name.clone())),
            _ => None,
        };
        let seen: Vec<Seen> = open.map_or_else(Vec::new, |(id, name)| {
            self.pocket_sync
                .log
                .of(&id)
                .take(SEEN_MAX)
                .map(|e| Seen { game: self.pocket_sync_game(&e.rel), line: page::seen_line(e.dir, &name, e.at, now) })
                .collect()
        });
        let ui = &mut self.pocket_sync;
        let (code, paired) = match &ui.svc {
            Some(s) => (
                s.pairing.lock().ok().and_then(|p| p.clone()).filter(|p| !p.expired(Instant::now())),
                s.paired.lock().ok().and_then(|mut p| p.take()),
            ),
            None => (None, None),
        };
        if let Some(name) = paired {
            ui.line = Some(format!("Paired with {name}."));
        }
        let Some(page) = ui.page.as_mut() else { return };
        page.pockets = peers
            .iter()
            .map(|p| {
                let (mark, status) = page::status(&ui.log.state(&p.id), ui.syncing.contains(&p.id), now, offset);
                Known { id: p.id.clone(), name: p.name.clone(), status, mark }
            })
            .collect();
        page.seen = seen;
        if let View::Pocket(i) = page.view {
            if i >= page.pockets.len() {
                page.view = View::List;
            }
        }
        page.sel = page.sel.min(page.rows().len() - 1);
        page.nearby = ui.nearby.iter().map(|f| Nearby { id: f.id.clone(), name: f.name.clone() }).collect();
        if let View::Pairing { sel } = &mut page.view {
            *sel = (*sel).min(page.nearby.len().saturating_sub(1));
        }
        page.problem = ui.problem.clone().filter(|_| page.prefs.enabled);
        page.line = ui.line.clone();
        match code {
            Some(p) => {
                page.code = Some(p.code.clone());
                page.code_left = p.until.saturating_duration_since(Instant::now()).as_secs();
            }
            None if matches!(page.view, View::Pairing { .. } | View::Code { .. }) && page.code.is_some() => {
                // Used or run out: a new one while the view is open.
                if let Some(s) = &ui.svc {
                    if let Ok(p) = Pairing::new() {
                        page.code = Some(p.code.clone());
                        page.code_left = p.until.saturating_duration_since(Instant::now()).as_secs();
                        *s.pairing.lock().unwrap_or_else(|e| e.into_inner()) = Some(p);
                    }
                }
            }
            None => page.code = None,
        }
    }

    pub(crate) fn pocket_sync_input(&mut self, b: Button) -> Option<SettingsPage> {
        let back = Some(SettingsPage::List(super::settings_row(SETTINGS_NAME)));
        let Some(p) = self.pocket_sync.page.as_mut() else { return back };
        match p.press(b, CONFIRM, CANCEL) {
            Action::Nothing => {}
            Action::Close => {
                self.pocket_sync_stop_pairing();
                self.pocket_sync.page = None;
                return back;
            }
            Action::SetEnabled(on) => {
                let ui = &mut self.pocket_sync;
                ui.prefs.enabled = on;
                if ui.prefs.save(&ui.dir).is_err() {
                    pocket_ui::ui_sound::refuse();
                }
                if on {
                    ui.start(&self.proxy);
                } else {
                    ui.stop();
                }
            }
            Action::StartPairing => self.pocket_sync_start_pairing(),
            Action::StopPairing => self.pocket_sync_stop_pairing(),
            Action::Pair { id, code } => {
                let found = self.pocket_sync.nearby.iter().find(|f| f.id == id).cloned();
                let (Some(store), Some(svc), Some(found)) = (self.pocket_sync.store(), self.pocket_sync.svc.clone(), found) else {
                    pocket_ui::ui_sound::refuse();
                    return Some(SettingsPage::Pockets);
                };
                self.pocket_sync.line = Some(format!("Pairing with {}…", found.name));
                let proxy = self.proxy.clone();
                let _ = std::thread::Builder::new().name("pocket-sync-pair".into()).spawn(move || {
                    let res = {
                        let mut peers = svc.peers.lock().unwrap_or_else(|e| e.into_inner());
                        net::pair(&store, &mut peers, found.addr, &code, net::PORT).map(|p| p.name)
                    };
                    let _ = proxy.send_event(UserEvent::PocketSync(Event::Paired(res)));
                });
            }
            Action::SyncNow(id) => self.pocket_sync_now(&id),
            Action::Forget(id) => {
                let ok = match &self.pocket_sync.svc {
                    Some(s) => s.forget(&id).is_ok(),
                    None => {
                        let mut p = Peers::load(&self.pocket_sync.dir);
                        p.forget(&id);
                        p.save(&self.pocket_sync.dir).is_ok()
                    }
                };
                if !ok {
                    pocket_ui::ui_sound::refuse();
                }
                // The service forgets its log part too; off, the page does it.
                let ui = &mut self.pocket_sync;
                if ui.svc.is_none() {
                    let mut log = Log::load(&ui.dir);
                    log.forget(&id);
                    let _ = log.save(&ui.dir);
                }
                ui.log = Log::load(&ui.dir);
                ui.syncing.remove(&id);
            }
            Action::Open(_) => {}
        }
        self.pocket_sync_refresh();
        Some(SettingsPage::Pockets)
    }

    fn pocket_sync_start_pairing(&mut self) {
        let ui = &mut self.pocket_sync;
        let Some(svc) = ui.svc.clone() else { return };
        if let Ok(p) = Pairing::new() {
            *svc.pairing.lock().unwrap_or_else(|e| e.into_inner()) = Some(p);
        }
        ui.nearby.clear();
        let looking = Arc::new(AtomicBool::new(true));
        ui.looking = Some(looking.clone());
        let proxy = self.proxy.clone();
        let me = svc.store.me.id.clone();
        let to = SocketAddr::from(([255, 255, 255, 255], net::DISCOVERY_PORT));
        let _ = std::thread::Builder::new().name("pocket-sync-look".into()).spawn(move || {
            // The tailnet's devices too, asked one by one (no broadcast crosses Tailscale): a Pocket at work finds the one at
            // home. Read again every 10 looks (a device that just came online).
            let mut tailnet: Vec<SocketAddr> = Vec::new();
            let mut round = 0u32;
            while looking.load(Ordering::Relaxed) {
                if round % 10 == 0 {
                    tailnet = tailnet_addrs();
                }
                round += 1;
                let mut targets = vec![to];
                targets.extend(tailnet.iter().copied());
                let found: Vec<net::Found> = net::discover_at(&targets, Duration::from_millis(1500)).into_iter().filter(|f| f.pairing && f.id != me).collect();
                if proxy.send_event(UserEvent::PocketSync(Event::Nearby(found))).is_err() {
                    return;
                }
            }
        });
    }

    fn pocket_sync_stop_pairing(&mut self) {
        let ui = &mut self.pocket_sync;
        if let Some(l) = ui.looking.take() {
            l.store(false, Ordering::Relaxed);
        }
        if let Some(s) = &ui.svc {
            *s.pairing.lock().unwrap_or_else(|e| e.into_inner()) = None;
        }
        ui.nearby.clear();
    }

    /// A sync with one paired Pocket, on a thread.
    fn pocket_sync_now(&mut self, id: &str) {
        let peer = self.pocket_sync.peers().into_iter().find(|p| p.id == id);
        let (Some(store), Some(peer)) = (self.pocket_sync.store(), peer) else { return };
        self.pocket_sync.syncing.insert(peer.id.clone());
        let busy = self.pocket_sync.busy.clone();
        let proxy = self.proxy.clone();
        let _ = std::thread::Builder::new().name("pocket-sync-now".into()).spawn(move || {
            let r = net::sync_with(&store, &peer, &|| busy.load(Ordering::Relaxed));
            let _ = proxy.send_event(UserEvent::PocketSync(Event::Synced(peer.id.clone(), r)));
        });
    }

    pub(crate) fn pocket_sync_event(&mut self, e: Event) {
        match e {
            Event::Paired(r) => {
                self.pocket_sync.line = Some(match r {
                    Ok(name) => format!("Paired with {name}."),
                    Err(SyncError::Unpaired) => "Wrong code, or it no longer shows one.".into(),
                    Err(SyncError::Unreachable) => "That Pocket did not answer.".into(),
                    Err(_) => "Not paired.".into(),
                });
            }
            Event::Synced(id, r) => {
                // A question was put here: the card and the shelf show it as soon as the look finds the copy.
                if r.as_ref().is_ok_and(|rep| !rep.asked.is_empty()) {
                    let prefs = self.sync.prefs.clone();
                    self.sync_conflicts_look(&prefs, false);
                }
                if r.as_ref().is_ok_and(|rep| !rep.fetched.is_empty()) {
                    self.curator.touch();
                }
                self.pocket_sync.syncing.remove(&id);
                self.pocket_sync.log = Log::load(&self.pocket_sync.dir);
            }
            Event::All(why, done) => {
                self.pocket_sync.log = Log::load(&self.pocket_sync.dir);
                let mut outcomes = Vec::new();
                let mut asked = false;
                for (_, name, r) in &done {
                    asked |= r.as_ref().is_ok_and(|rep| !rep.asked.is_empty());
                    outcomes.extend(outcome(name, r, &|rel| self.pocket_sync_game(rel)));
                }
                if asked {
                    let prefs = self.sync.prefs.clone();
                    self.sync_conflicts_look(&prefs, false);
                }
                // The line only on the shelf, and never over a game.
                if let Some(line) = page::shelf_line(why, &outcomes) {
                    if self.running.is_none() {
                        self.notice = Some(pocket_ui::notice::Notice::new(line));
                    }
                }
                if !done.iter().all(|(_, _, r)| r.as_ref().is_ok_and(|rep| rep.fetched.is_empty())) {
                    // What came changes the cases (saves read by "the case knows") and Continue: look again.
                    self.curator.touch();
                }
            }
            Event::Received => {
                let got = self.pocket_sync.store.as_ref().map(|s| s.take_received()).unwrap_or_default();
                self.pocket_sync.log = Log::load(&self.pocket_sync.dir);
                let names = self.pocket_sync.peers();
                let outcomes: Vec<Outcome> = got
                    .iter()
                    .map(|(id, rels)| Outcome {
                        // Only a paired Pocket can send (the service checks its key); one forgotten meanwhile is named by its id.
                        name: names.iter().find(|p| &p.id == id).map_or_else(|| id.chars().take(7).collect(), |p| p.name.clone()),
                        fetched: rels.len(),
                        games: rels.iter().map(|r| self.pocket_sync_game(r)).collect(),
                        ..Outcome::default()
                    })
                    .collect();
                if let Some(line) = page::shelf_line(Why::Start, &outcomes) {
                    if self.running.is_none() {
                        self.notice = Some(pocket_ui::notice::Notice::new(line));
                    }
                }
                if !got.is_empty() {
                    self.curator.touch();
                }
            }
            Event::Nearby(found) => {
                if self.pocket_sync.looking.is_none() {
                    return;
                }
                self.pocket_sync.nearby = found;
            }
        }
        if self.settings_page == Some(SettingsPage::Pockets) {
            self.pocket_sync_refresh();
            self.request_redraw();
        }
    }

    /// Each loop turn: the service follows the switch (a little after the start), and knows whether a game runs. Once
    /// it is up, this Pocket fetches what the others saved meanwhile.
    pub(crate) fn pocket_sync_tick(&mut self) {
        self.pocket_sync.busy.store(self.running.is_some(), Ordering::Relaxed);
        let ui = &mut self.pocket_sync;
        let settled = ui.stopped_at.map_or(true, |t| t.elapsed() >= RESTART_AFTER);
        if ui.prefs.enabled && !ui.running() && ui.problem.is_none() && settled && self.started.elapsed() >= START_AFTER {
            ui.start(&self.proxy);
        }
        if self.pocket_sync.start_due && self.running.is_none() {
            self.pocket_sync.start_due = false;
            // Android: back from a game, its sync has just begun; the service coming up after it fetches nothing new.
            if self.pocket_sync.last_all.map_or(true, |t| t.elapsed() >= Duration::from_secs(30)) {
                self.pocket_sync_all(Why::Start);
            }
        }
    }

    /// Android: the view went to the background (Home, a game, the screen off). The service stops: it listens only while
    /// pocket-ui is in front, and no background service keeps the handheld awake. In front again it starts by itself
    /// (`pocket_sync_tick`) and fetches what the others saved meanwhile.
    #[cfg_attr(not(target_os = "android"), allow(dead_code))]
    pub(crate) fn pocket_sync_background(&mut self) {
        if self.pocket_sync.running() {
            self.pocket_sync.stop();
            self.pocket_sync.stopped_at = Some(Instant::now());
        }
    }

    /// A game ended: what it saved goes to every paired Pocket that answers (`app.rs` calls this where Syncthing is
    /// asked too). Back on the shelf at once; the line comes when the syncs are done.
    pub(crate) fn pocket_sync_after_game(&mut self) {
        self.pocket_sync_all(Why::AfterGame);
    }

    /// The Pocket woke from sleep: the others may have saved meanwhile.
    pub(crate) fn pocket_sync_woke(&mut self) {
        self.pocket_sync_all(Why::Start);
    }

    /// A sync with every paired Pocket, one after the other, on one thread. Not while a game runs, not twice at once.
    fn pocket_sync_all(&mut self, why: Why) {
        let ui = &mut self.pocket_sync;
        // Android: after a game the service is not up yet (it stopped in the background); the sync is this Pocket asking,
        // it needs no service of its own.
        let up = ui.running() || (cfg!(target_os = "android") && !ui.still);
        if !ui.prefs.enabled || !up || self.running.is_some() || ui.all_running.swap(true, Ordering::AcqRel) {
            return;
        }
        ui.last_all = Some(Instant::now());
        // No game runs (checked above): the flag says so now, not at the next loop turn (Android calls this from the way
        // back from the player, before a turn ran; the sync would have stopped at once as "busy").
        ui.busy.store(false, Ordering::Relaxed);
        let peers = ui.peers();
        let Some(store) = ui.store() else {
            ui.all_running.store(false, Ordering::Release);
            return;
        };
        if peers.is_empty() {
            ui.all_running.store(false, Ordering::Release);
            return;
        }
        let (busy, flag, proxy) = (ui.busy.clone(), ui.all_running.clone(), self.proxy.clone());
        let _ = std::thread::Builder::new().name("pocket-sync-all".into()).spawn(move || {
            let mut done = Vec::new();
            for p in peers {
                let r = net::sync_with(&store, &p, &|| busy.load(Ordering::Relaxed));
                done.push((p.id.clone(), p.name.clone(), r));
            }
            flag.store(false, Ordering::Release);
            let _ = proxy.send_event(UserEvent::PocketSync(Event::All(why, done)));
        });
    }

    /// The code's countdown and new Pockets nearby: a frame a second while the pairing view shows.
    pub(crate) fn pocket_sync_wake(&mut self, now: Instant) -> Option<Instant> {
        let showing = self.settings_page == Some(SettingsPage::Pockets)
            && self.pocket_sync.page.as_ref().is_some_and(|p| !matches!(p.view, View::List) || p.pockets.iter().any(|k| k.status == page::SYNCING));
        if !showing {
            return None;
        }
        self.pocket_sync_refresh();
        self.request_redraw();
        Some(now + Duration::from_secs(1))
    }

    /// Where the page stands: a change is what the menu sounds answer.
    pub(crate) fn pocket_sync_row(&self) -> Option<(usize, u8, bool)> {
        self.pocket_sync.page.as_ref().map(|p| {
            let kind = match &p.view {
                View::List => p.forget.map_or(0, |i| 1 + i as u8),
                View::Pairing { sel } => 100 + *sel as u8,
                View::Code { at, digits, .. } => 200u8.wrapping_add(*at as u8 * 10).wrapping_add(digits[*at]),
                View::What => 250,
            View::Pocket(i) => 240u8.wrapping_add(*i as u8).wrapping_add(if p.forget.is_some() { 5 } else { 0 }),
            };
            (p.sel, kind, p.prefs.enabled)
        })
    }

    /// `--snapshot pockets[:off|:on|:none|:pair|:nobody|:code|:what|:forget|:taken|:faults|:syncing|:pocket|:pocket-empty][:<row>]`:
    /// the page with made-up Pockets.
    pub(crate) fn pocket_sync_snapshot(&mut self, state: &str) -> bool {
        let Some(rest) = state.strip_prefix("pockets") else { return false };
        let mut parts = rest.trim_start_matches(':').split(':');
        let mode = parts.next().unwrap_or("");
        let row: usize = parts.next().and_then(|r| r.parse().ok()).unwrap_or(0);
        let mut p = PocketPage::new(Prefs { enabled: mode != "off" });
        if mode != "none" {
            // Made-up states, "now" 13:05 UTC (the lines are `page::status`'s).
            use pocket_ui::pocket_sync::log::{Last, State};
            let now = 1_763_557_500;
            let st = |last, ok_at, down_since| page::status(&State { last, ok_at, down_since }, false, now, 0);
            let k = |id: &str, name: &str, (mark, status): (Mark, String)| Known { id: id.into(), name: name.into(), status, mark };
            p.pockets = vec![
                k("A1", "RG35XX Plus", st(Last::Synced, Some(now - 180), None)),
                k("B2", "TrimUI Brick", st(Last::Unreachable, Some(now - 7200), Some(now - 3600))),
                k("C3", "Retroid Pocket Mini V2", st(Last::Busy, Some(now - 86_400), None)),
            ];
            if mode == "faults" {
                p.pockets = vec![
                    k("A1", "RG35XX Plus", st(Last::Failed(2), Some(now - 60), None)),
                    k("B2", "TrimUI Brick", st(Last::Unpaired, None, None)),
                    k("C3", "Retroid Pocket Mini V2", st(Last::Bad, None, None)),
                    k("D4", "RG35XX SP", st(Last::Asked, Some(now - 600), None)),
                    k("E5", "R36S", st(Last::Never, None, None)),
                ];
            }
            if mode == "syncing" {
                p.pockets[0].status = page::SYNCING.into();
                p.pockets[0].mark = Mark::None;
            }
        }
        p.sel = row.min(p.rows().len() - 1);
        let nearby = vec![Nearby { id: "N1".into(), name: "TrimUI Brick".into() }, Nearby { id: "N2".into(), name: "RG35XX Plus".into() }];
        match mode {
            "pair" => {
                p.view = View::Pairing { sel: 0 };
                p.code = Some("854311".into());
                p.code_left = 287;
                p.nearby = nearby;
            }
            "nobody" => {
                p.view = View::Pairing { sel: 0 };
                p.code = Some("854311".into());
                p.code_left = 287;
            }
            "code" => {
                p.view = View::Code { pocket: 0, digits: [8, 5, 4, 3, 0, 0], at: 3 };
                p.code = Some("207114".into());
                p.code_left = 251;
                p.nearby = nearby;
            }
            "what" => p.view = View::What,
            "pocket" | "pocket-empty" => {
                p.view = View::Pocket(0);
                p.sel = 3;
                if mode == "pocket" {
                    use pocket_ui::pocket_sync::log::Dir;
                    let now = 1_763_557_500;
                    let s = |game: &str, dir, ago| Seen { game: game.into(), line: page::seen_line(dir, "RG35XX Plus", now - ago, now) };
                    p.seen = vec![
                        s("Golden Sun", Dir::From, 180),
                        s("Pokémon Gold Version", Dir::To, 3_000),
                        s("The Legend of Zelda: The Minish Cap", Dir::From, 90_000),
                        s("Mother 3", Dir::To, 260_000),
                    ];
                }
            }
            "forget" => {
                p.sel = 4;
                p.forget = Some(1);
            }
            "taken" => p.problem = Some(format!("Port {} is taken ({}).", net::PORT, std::io::Error::from(std::io::ErrorKind::AddrInUse))),
            "on" => p.line = Some(format!("Paired with {}.", "TrimUI Brick")),
            _ => {}
        }
        self.pocket_sync.page = Some(p);
        self.pocket_sync.still = true;
        self.settings_page = Some(SettingsPage::Pockets);
        true
    }

    pub(crate) fn paint_pocket_sync(&mut self, c: &mut Canvas) {
        let Some(p) = self.pocket_sync.page.take() else { return };
        c.clear(pal().void);
        match p.view.clone() {
            View::List => self.paint_pocket_list(c, &p),
            View::Pairing { sel } => self.paint_pocket_pairing(c, &p, sel),
            View::Code { pocket, digits, at } => self.paint_pocket_code(c, &p, pocket, digits, at),
            View::What => self.paint_pocket_what(c, &p),
            View::Pocket(i) => self.paint_pocket_one(c, &p, i),
        }
        self.pocket_sync.page = Some(p);
    }

    fn paint_pocket_list(&mut self, c: &mut Canvas, p: &PocketPage) {
        let (pad, font) = (NU_PAD, 36u32);
        let row = pocket_ui::type_scale::row(66, 36);
        let row = if pocket_ui::kit::short_screen(c.h) { row * 4 / 5 } else { row };
        self.sheet_header(c, SETTINGS_NAME);
        let (top, right) = (pocket_ui::kit::header_bottom(c.h) + 12, c.w - pad);
        let rows = p.rows();
        let (left, width) = (pad + NU_BTN_PAD, c.w - 2 * (pad + NU_BTN_PAD));
        // The paired Pockets under a small label; the list stops above the note.
        let label_gap = 44;
        let visible = ((pocket_ui::kit::hint_bar_top(c.h) - top - label_gap - 150) / row).max(3) as usize;
        let first = super::window_start(p.sel, visible, rows.len());
        let mut y = top;
        for (i, r) in rows.iter().enumerate().skip(first).take(visible) {
            if matches!(r, Row::Pocket(0)) {
                self.text.draw(c, Face::Body, 24, left, y + 30, pal().dust, "Paired");
                y += label_gap;
            }
            self.hits.row(Area::new(pad, y, right - pad, row), i);
            let base = y + row / 2 + pocket_ui::type_scale::mid(font);
            let selected = i == p.sel;
            let dim = !p.prefs.enabled && !matches!(r, Row::Switch);
            let forget = matches!(r, Row::Pocket(k) if p.forget == Some(*k));
            let value = p.value(*r);
            let mark = match r {
                Row::Pocket(k) if p.forget != Some(*k) => p.pockets.get(*k).map_or(Mark::None, |k| k.mark),
                _ => Mark::None,
            };
            let mark_w = if mark == Mark::None { 0 } else { MARK_W };
            let value_w = self.text.width(Face::Body, 30, &value) + mark_w;
            let name = self.text.fit(Face::Body, font, &p.label(*r), right - pad - 2 * NU_BTN_PAD - value_w - 24);
            if selected {
                let color = if forget { pal().alert } else { pal().select };
                pocket_ui::retro::select(c, pad, y + 4, right - pad, row - 8, (row - 8) / 2, color);
                let fg = if forget { pal().on_alert } else { pal().ink };
                self.text.draw(c, Face::Body, font, left, base, fg, &name);
                self.text.draw_right(c, Face::Body, 30, right - NU_BTN_PAD, base, fg, &value);
                paint_mark(c, right - NU_BTN_PAD - value_w + MARK_W / 2 - 6, base - pocket_ui::type_scale::mid(30), mark, fg);
            } else {
                self.text.draw(c, Face::Body, font, left, base, if dim { pal().quiet } else { pal().text }, &name);
                let vcol = if matches!(r, Row::Pocket(_)) { pal().dust } else { pal().text };
                let vcol = if mark == Mark::Bad { pal().alert } else { vcol };
                self.text.draw_right(c, Face::Body, 30, right - NU_BTN_PAD, base, vcol, &value);
                let mcol = if mark == Mark::Good { pal().text } else { vcol };
                paint_mark(c, right - NU_BTN_PAD - value_w + MARK_W / 2 - 6, base - pocket_ui::type_scale::mid(30), mark, mcol);
            }
            y += row;
        }
        self.hits.list(p.sel, rows.len(), 0.0, row as f32);
        y += 30;
        let note: String = match (&p.problem, &p.line, p.prefs.enabled, p.pockets.is_empty()) {
            (Some(why), ..) => format!("Not listening. {why}"),
            (None, Some(line), ..) => line.clone(),
            (None, None, false, _) => "Off. Switch it on on every Pocket that should share saves.".into(),
            (None, None, true, true) => "No Pocket paired yet. Choose Pair a Pocket on this one and on the other.".into(),
            (None, None, true, false) => pocket_ui::device::wifi_or("Only your Pockets on the same Wi-Fi or in your Tailscale. No cloud.", pocket_ui::device::NEEDS_NETWORK).into(),
        };
        let hints = p.hints();
        let foot = pocket_ui::kit::footer(c, &mut self.text, &mut self.hits, &hints, p.describe(p.row()), 2, pal().text);
        let color = if p.problem.is_some() { pal().alert } else { pal().quiet };
        self.text.paragraph(c, Face::Body, 26, (left, y), width, pocket_ui::type_scale::line(26, 8), foot - 8, color, &note);
    }

    /// This Pocket's code in large digits, and the Pockets nearby that show one.
    fn paint_pocket_pairing(&mut self, c: &mut Canvas, p: &PocketPage, sel: usize) {
        let pad = NU_PAD;
        self.sheet_header(c, "Pair a Pocket");
        let (left, top, right) = (pad + NU_BTN_PAD, pocket_ui::kit::header_bottom(c.h) + 30, c.w - pad);
        let width = right - NU_BTN_PAD - left;
        let mut y = top + 30;
        self.text.draw(c, Face::Body, 26, left, y, pal().dust, "This Pocket's code");
        y += 110;
        let code = p.code.clone().unwrap_or_else(|| "------".into());
        let spaced = format!("{} {}", &code[..3], &code[3..]);
        self.text.draw(c, Face::Title, 110, left, y, pal().chalk, &spaced);
        y += 50;
        let time_left = format!("{}:{:02}", p.code_left / 60, p.code_left % 60);
        let valid = format!("Valid for {time_left} more, for one Pocket.");
        let valid = self.text.fit(Face::Body, 24, &valid, width);
        self.text.draw(c, Face::Body, 24, left, y, pal().dust, &valid);
        y += 60;
        self.text.draw(c, Face::Body, 26, left, y, pal().dust, "Pockets nearby showing a code");
        y += 20;
        let (row, font) = (pocket_ui::type_scale::row(64, 34), 34u32);
        if p.nearby.is_empty() {
            y += 50;
            let looking = pocket_ui::device::wifi_or("Looking… Open Pair a Pocket on the other Pocket too, on the same Wi-Fi.", pocket_ui::device::NEEDS_NETWORK);
            for l in self.text.wrap_prose(Face::Body, 28, looking, width) {
                self.text.draw(c, Face::Body, 28, left, y, pal().text, &l);
                y += pocket_ui::type_scale::line(28, 8);
            }
        }
        let visible = ((pocket_ui::kit::hint_bar_top(c.h) - y - 20) / row).max(1) as usize;
        let first = super::window_start(sel, visible, p.nearby.len());
        for (i, n) in p.nearby.iter().enumerate().skip(first).take(visible) {
            let ry = y + (i - first) as i32 * row;
            self.hits.row(Area::new(pad, ry, right - pad, row), i);
            let base = ry + row / 2 + pocket_ui::type_scale::mid(font);
            let name = self.text.fit(Face::Body, font, &n.name, width);
            if i == sel {
                pocket_ui::kit::selection(c, pad, ry, right - pad, row);
                self.text.draw(c, Face::Body, font, left, base, pal().ink, &name);
            } else {
                self.text.draw(c, Face::Body, font, left, base, pal().text, &name);
            }
        }
        self.hits.list(sel, p.nearby.len(), 0.0, row as f32);
        let line = p.line.clone().unwrap_or_default();
        let desc = if line.is_empty() { "Pick the other Pocket and type its code." } else { line.as_str() };
        let hints = p.hints();
        self.nu_foot(c, &hints, desc, pal().text);
    }

    /// Six boxes, the one Up/Down changes marked.
    fn paint_pocket_code(&mut self, c: &mut Canvas, p: &PocketPage, pocket: usize, digits: [u8; 6], at: usize) {
        let pad = NU_PAD;
        self.sheet_header(c, "Pair a Pocket");
        let (left, top) = (pad + NU_BTN_PAD, pocket_ui::kit::header_bottom(c.h) + 30);
        let name = p.nearby.get(pocket).map_or(String::new(), |n| n.name.clone());
        let title = format!("The code {name} shows");
        let title = self.text.fit(Face::Body, 30, &title, c.w - 2 * left);
        self.text.draw(c, Face::Body, 30, left, top + 40, pal().text, &title);
        let (bw, bh, gap) = (120, 160, 24);
        let total = 6 * bw + 5 * gap;
        let x0 = (c.w - total) / 2;
        let by = top + 110;
        for (i, d) in digits.iter().enumerate() {
            let x = x0 + i as i32 * (bw + gap);
            let color = if i == at { pal().select } else { pal().band };
            pocket_ui::retro::select(c, x, by, bw, bh, 12, color);
            pocket_ui::retro::select(c, x + 6, by + 6, bw - 12, bh - 12, 8, pal().void);
            let s = d.to_string();
            let w = self.text.width(Face::Title, 96, &s);
            let fg = if i == at { pal().select } else { pal().chalk };
            self.text.draw(c, Face::Title, 96, x + (bw - w) / 2, by + bh / 2 + 34, fg, &s);
        }
        pocket_ui::shares::mark("pocket-code", x0, by, total, bh);
        let mut y = by + bh + 70;
        for l in self.text.wrap_prose(Face::Body, 26, "Up and Down change the digit, Left and Right move. A pairs. Five wrong tries and the code is gone.", c.w - 2 * left) {
            self.text.draw(c, Face::Body, 26, left, y, pal().dust, &l);
            y += pocket_ui::type_scale::line(26, 8);
        }
        let hints = p.hints();
        self.nu_hints(c, &hints);
    }

    /// A paired Pocket's page: its name, how it stands (with the sign), then the last saves that came and went.
    fn paint_pocket_one(&mut self, c: &mut Canvas, p: &PocketPage, i: usize) {
        let pad = NU_PAD;
        let Some(k) = p.pockets.get(i) else { return };
        self.sheet_header(c, SETTINGS_NAME);
        let (left, right) = (pad + NU_BTN_PAD, c.w - pad - NU_BTN_PAD);
        let width = right - left;
        let mut y = pocket_ui::kit::header_bottom(c.h) + 60;
        let name = self.text.fit(Face::Title, 44, &k.name, width);
        self.text.draw(c, Face::Title, 44, left, y, pal().chalk, &name);
        y += 56;
        let forget = p.forget == Some(i);
        let (line, color) = if forget {
            ("X again: forget".to_string(), pal().alert)
        } else {
            (k.status.clone(), if k.mark == Mark::Bad { pal().alert } else { pal().text })
        };
        let mark = if forget { Mark::None } else { k.mark };
        let x = if mark == Mark::None { left } else { left + MARK_W };
        paint_mark(c, left + 10, y - pocket_ui::type_scale::mid(30), mark, if mark == Mark::Good { pal().text } else { color });
        let line = self.text.fit(Face::Body, 30, &line, right - x);
        self.text.draw(c, Face::Body, 30, x, y, color, &line);
        y += 64;
        self.text.draw(c, Face::Body, 24, left, y, pal().dust, "Last saves");
        y += 14;
        let foot = pocket_ui::kit::hint_bar_top(c.h) - 12;
        let row = pocket_ui::type_scale::row(76, 30);
        if p.seen.is_empty() {
            y += 44;
            let none = if p.prefs.enabled { "None yet. A syncs now." } else { "None yet. Switch Pocket Sync on first." };
            self.text.paragraph(c, Face::Body, 28, (left, y), width, pocket_ui::type_scale::line(28, 8), foot, pal().text, none);
        }
        for s in &p.seen {
            if y + row > foot {
                break;
            }
            let game = self.text.fit(Face::Body, 30, &s.game, width);
            self.text.draw(c, Face::Body, 30, left, y + 38, pal().text, &game);
            let line = self.text.fit(Face::Body, 24, &s.line, width);
            self.text.draw(c, Face::Body, 24, left, y + 38 + pocket_ui::type_scale::line(24, 10), pal().dust, &line);
            y += row;
        }
        let hints = p.hints();
        self.nu_hints(c, &hints);
    }

    /// What is synced, what not yet, which Pockets: a heading and a short paragraph each (`page::WHAT`).
    fn paint_pocket_what(&mut self, c: &mut Canvas, p: &PocketPage) {
        let pad = NU_PAD;
        self.sheet_header(c, "What Pocket Sync syncs");
        let (left, width) = (pad + NU_BTN_PAD, c.w - 2 * (pad + NU_BTN_PAD));
        let foot = pocket_ui::kit::hint_bar_top(c.h) - 16;
        let top = pocket_ui::kit::header_bottom(c.h) + 40;
        let what = page::what(pocket_ui::device::no_wifi_chip());
        // The largest size at which all four paragraphs fit above the hint bar (Large text, a long language, 640x480).
        let (px, gap) = [(26u32, 34), (24, 26), (22, 20), (20, 14), (18, 10)]
            .into_iter()
            .find(|&(px, gap)| {
                let step = pocket_ui::type_scale::line(px, 8);
                let lines: i32 = what.iter().flat_map(|(_, ts)| ts.iter()).map(|t| self.text.wrap_prose(Face::Body, px, t, width).len() as i32).sum::<i32>() + what.len() as i32;
                top + lines * step + 3 * gap <= foot + step
            })
            .unwrap_or((18, 10));
        let step = pocket_ui::type_scale::line(px, 8);
        let mut y = top;
        for (head, texts) in &what {
            if y + 30 > foot {
                break;
            }
            self.text.draw(c, Face::Body, px, left, y, pal().amber, head);
            y += step;
            for text in texts {
                y = self.text.paragraph(c, Face::Body, px, (left, y), width, step, foot, pal().text, text);
            }
            y += gap;
        }
        let hints = p.hints();
        self.nu_hints(c, &hints);
    }

    /// pocket-ui ends: the service stops (its sockets close at once).
    pub(crate) fn pocket_sync_exit(&mut self) {
        self.pocket_sync.stop();
    }
}

/// Every online device of this Pocket's tailnet, at the port that answers "who is there?" (`tailscale status`; none on
/// Android, whose Tailscale app tells apps nothing: there the other Pocket finds this one).
fn tailnet_addrs() -> Vec<SocketAddr> {
    #[cfg(target_os = "android")]
    return Vec::new();
    #[cfg(not(target_os = "android"))]
    pocket_ui::tailscale::online_peers(&pocket_ui::tailscale::Tools::default())
        .into_iter()
        .map(|ip| SocketAddr::from((ip, net::DISCOVERY_PORT)))
        .collect()
}

fn web_on_off(on: bool) -> String {
    pocket_ui::web_page::on_off(on).to_string()
}

/// Room for the sign before a Pocket's line.
const MARK_W: i32 = 34;

/// The sign before a Pocket's line, centred on (cx, cy): a tick, a cross, or nothing (the font has neither).
fn paint_mark(c: &mut Canvas, cx: i32, cy: i32, mark: Mark, color: u32) {
    let stroke = |c: &mut Canvas, pts: &[(i32, i32)]| {
        for seg in pts.windows(2) {
            let ((x0, y0), (x1, y1)) = (seg[0], seg[1]);
            let n = (x1 - x0).abs().max((y1 - y0).abs()).max(1);
            for k in 0..=n {
                c.disc(x0 + (x1 - x0) * k / n, y0 + (y1 - y0) * k / n, 2, color);
            }
        }
    };
    match mark {
        Mark::None => {}
        Mark::Good => stroke(c, &[(cx - 9, cy), (cx - 3, cy + 7), (cx + 9, cy - 8)]),
        Mark::Bad => {
            stroke(c, &[(cx - 7, cy - 7), (cx + 7, cy + 7)]);
            stroke(c, &[(cx - 7, cy + 7), (cx + 7, cy - 7)]);
        }
    }
}
