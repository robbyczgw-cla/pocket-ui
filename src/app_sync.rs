//! Part of the pocket-ui binary: the App's side of `pocket_ui::sync` and `pocket_ui::sync_page`. Settings > Sync,
//! the poller that keeps its status current, and the scan after a game with the small notice it leaves on Home.

use std::path::Path;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use pocket_ui::gfx::Canvas;
use pocket_ui::pad::Button;
use pocket_ui::palette::*;
use pocket_ui::sync::{self, Config, CurlApi, Outcome, Prefs, Problem, Snapshot};
use pocket_ui::sync_page::{Action, Event, SyncPage, ROWS};
use pocket_ui::text::Face;
use pocket_ui::touch::Area;

use super::{App, SettingsPage, UserEvent, CANCEL, CONFIRM, NU_BTN_PAD, NU_PAD, NU_PILL, NU_SMALL};

/// How long the notice after a game stays on Home.
const NOTE_FOR: Duration = Duration::from_secs(6);
/// How often the open page asks Syncthing how it is.
const POLL_EVERY: Duration = Duration::from_secs(3);

/// What the App keeps of sync: the choices, the open page, the notice.
pub(crate) struct SyncUi {
    pub prefs: Prefs,
    pub(crate) page: Option<SyncPage>,
    note: Option<(String, Instant)>,
    /// Saves with two versions (`app_sync_conflicts`).
    pub conflicts: super::app_sync_conflicts::ConflictsUi,
    /// Library data merged while a game ran: applied when it ends (`app_library_sync`).
    pub library_waiting: Option<Box<pocket_ui::library_sync::Outcome>>,
}

impl SyncUi {
    pub fn load(data_dir: &Path) -> SyncUi {
        sync::release_leftover_holds();
        SyncUi {
            prefs: Prefs::load(data_dir),
            page: None,
            note: None,
            conflicts: super::app_sync_conflicts::ConflictsUi::load(data_dir),
            library_waiting: None,
        }
    }
}

/// Syncthing as the scan sees it when there is no readable configuration: not running.
struct NoSyncthing;

impl sync::Api for NoSyncthing {
    fn running(&mut self) -> bool {
        false
    }
    fn scan(&mut self, _: &sync::ScanReq) -> Result<(), String> {
        Err("no configuration".into())
    }
    fn peers(&mut self) -> Result<sync::Peers, String> {
        Err("no configuration".into())
    }
    fn outstanding(&mut self, _: &[String]) -> Result<u64, String> {
        Err("no configuration".into())
    }
    fn set_paused(&mut self, _: &str, _: bool) -> Result<(), String> {
        Err("no configuration".into())
    }
    fn add_folder(&mut self, _: &'static sync::Dir) -> Result<(), String> {
        Err("no configuration".into())
    }
}

impl App {
    pub(crate) fn open_sync(&mut self) -> Option<SettingsPage> {
        let mut page = SyncPage::new(self.sync.prefs.clone(), pocket_ui::wifi::ip());
        // What the last search found; a new one now, as the page may be opened to look (`app_sync_conflicts`).
        page.waiting = self.sync.conflicts.found.len();
        self.sync_conflicts_look(&self.sync.prefs, false);
        // Library data merged now too, when it is on.
        self.library_sync_look(&self.sync.prefs, false);
        let (alive, proxy, prefs) = (page.alive.clone(), self.proxy.clone(), self.sync.prefs.clone());
        // The page reads Syncthing's state now and then while it is open; the UI thread never waits for it.
        std::thread::spawn(move || {
            while alive.load(Ordering::Relaxed) {
                let cfg = Config::read().ok();
                let snap = sync::snapshot(&prefs);
                let _ = proxy.send_event(UserEvent::Sync(Event::Poll {
                    cfg,
                    snap,
                    password: sync::password(),
                }));
                let until = Instant::now() + POLL_EVERY;
                while alive.load(Ordering::Relaxed) && Instant::now() < until {
                    std::thread::sleep(Duration::from_millis(100));
                }
            }
        });
        self.sync.page = Some(page);
        Some(SettingsPage::Sync)
    }

    pub(crate) fn sync_input(&mut self, b: Button) -> Option<SettingsPage> {
        // The saves with two versions, opened from the page's row.
        if self.conflicts_over_sync() {
            self.conflicts_input(b);
            return Some(SettingsPage::Sync);
        }
        let Some(page) = self.sync.page.as_mut() else {
            return Some(super::SettingsPage::List(super::settings_row("Sync")));
        };
        let action = page.press(b, CONFIRM, CANCEL);
        match action {
            Action::Nothing => {}
            Action::Close => {
                self.sync.page = None;
                return Some(SettingsPage::List(super::settings_row("Sync")));
            }
            Action::SetEnabled(on) => {
                page.busy = Some(if on { "Turning Syncthing on" } else { "Turning Syncthing off" });
                let (proxy, prefs) = (self.proxy.clone(), page.prefs.clone());
                std::thread::spawn(move || {
                    let result = if on {
                        sync::enable().and_then(|pw| sync::apply_folders(&prefs).map(|()| Some(pw)))
                    } else {
                        sync::disable().map(|()| None)
                    };
                    let _ = proxy.send_event(UserEvent::Sync(Event::Switched { on, result }));
                });
            }
            Action::SetAfterPlay(_) => self.keep_sync_prefs(),
            Action::OpenVersions => self.open_conflicts_all(),
            Action::SetShare(share, on) => {
                self.keep_sync_prefs();
                if self.sync.page.as_ref().is_some_and(|p| p.on()) {
                    let proxy = self.proxy.clone();
                    std::thread::spawn(move || {
                        let _ = proxy.send_event(UserEvent::Sync(Event::ShareApplied(share, sync::apply_share(share, on))));
                    });
                }
            }
        }
        Some(SettingsPage::Sync)
    }

    /// The page's choices are the saved ones.
    fn keep_sync_prefs(&mut self) {
        // Only a change is written: the page's poller reports every few seconds.
        if let Some(page) = self.sync.page.as_ref().filter(|p| p.prefs != self.sync.prefs) {
            self.sync.prefs = page.prefs.clone();
            self.sync.prefs.save(&self.data_dir);
            // Syncthing or a share switched: what waits for a choice follows (none while it is off), and Library data
            // starts or stops merging.
            self.sync_conflicts_look(&self.sync.prefs, false);
            self.library_sync_look(&self.sync.prefs, false);
        }
    }

    pub(crate) fn sync_event(&mut self, e: Event) {
        match e {
            Event::AfterGame(outcome) => {
                println!("sync after game: {outcome:?}");
                self.sync.note = outcome.notice().map(|text| (text, Instant::now()));
            }
            Event::Conflicts(groups) => self.conflicts_found(groups),
            Event::Kept { original, result } => self.conflict_kept(original, result),
            Event::Library(out) => self.library_merged(out),
            e => {
                if let Some(page) = self.sync.page.as_mut() {
                    page.event(e);
                }
                self.keep_sync_prefs();
            }
        }
    }

    /// A game has ended: ask Syncthing to look for the new saves. Spawns a thread and returns; the shelf is
    /// already on its way back and never waits for this.
    pub(crate) fn sync_after_game(&mut self, game: &Path) {
        let prefs = self.sync.prefs.clone();
        // Saves with two versions: looked for again now and once Syncthing has pulled what came meanwhile.
        self.sync_conflicts_look(&prefs, true);
        // Library data: what came while the game ran is applied, and merged again (`app_library_sync`).
        self.library_sync_after_game();
        self.library_sync_look(&prefs, true);
        if !sync::after_game_due(&prefs, game, Path::new(sync::HELD_PATH)) {
            sync::want_held(&prefs, None);
            return;
        }
        let proxy = self.proxy.clone();
        let done = move |outcome: Outcome| {
            let _ = proxy.send_event(UserEvent::Sync(Event::AfterGame(outcome)));
        };
        match Config::read() {
            Ok(cfg) => {
                let api = CurlApi::new(&cfg);
                sync::after_game(prefs, Some(cfg), game.to_path_buf(), api, done);
            }
            Err(_) => {
                sync::after_game(prefs, None, game.to_path_buf(), NoSyncthing, done);
            }
        }
    }

    /// A game starts: the folders it writes (battery saves, states, a standalone emulator's own) are paused until
    /// it ends, so nothing Syncthing brings in meanwhile is overwritten by the game (review N1).
    pub(crate) fn sync_game_started(&mut self, system: &str) {
        sync::hold_for_game(&self.sync.prefs, system);
        self.sync_conflicts_playing();
        self.library_sync_playing();
    }

    /// Where the page's selection and switches stand: a change is what the menu sounds answer.
    pub(crate) fn sync_row(&self) -> Option<(usize, bool, bool, [bool; 7])> {
        self.sync
            .page
            .as_ref()
            .map(|p| (p.sel, p.on(), p.prefs.after_play, sync::Share::ALL.map(|s| p.prefs.shared(s))))
    }

    /// The small line on Home after a game: "Saves synced".
    pub(crate) fn sync_note(&self) -> Option<String> {
        // Painted on Home: never waits for sync's state (read after the first frame, `startup::Later`).
        self.sync.loaded()?.note.as_ref().filter(|(_, at)| at.elapsed() < NOTE_FOR).map(|(text, _)| text.clone())
    }

    /// Something of sync's moves on screen: the notice is up (it ends on its own), or the switch is working.
    pub(crate) fn sync_note_active(&self) -> bool {
        self.sync_note().is_some() || self.sync.loaded().and_then(|s| s.page.as_ref()).is_some_and(|p| p.busy.is_some())
    }

    /// `--snapshot sync[:off|:on|:busy|:open|:bios|:library|:conflicts|:conflicts-list|:conflicts-card]` draws the page with made-up
    /// data, for checking the layout (`conflicts…`: two saves with two versions, `App::sync_conflicts_snapshot`).
    pub(crate) fn sync_snapshot(&mut self, state: &str) -> bool {
        let Some(rest) = state.strip_prefix("sync") else { return false };
        let mut parts = rest.trim_start_matches(':').split(':');
        let mode = parts.next().unwrap_or_default();
        let row: Option<usize> = parts.next().and_then(|r| r.parse().ok());
        let mut prefs = Prefs {
            enabled: mode != "off" && mode != "",
            ..Prefs::default()
        };
        if mode == "bios" {
            prefs.bios = true;
        }
        // Library data shared, its row selected.
        if mode == "library" {
            prefs.library = true;
        }
        let mut page = SyncPage::new(prefs, Some("192.0.2.57".into()));
        if page.prefs.enabled {
            let xml = include_str!("fixtures/syncthing-pocket.xml");
            page.cfg = Config::parse(xml).ok();
            page.password = Some("maple-river-stone-482".into());
            let peers = sync::Peers {
                configured: 2,
                connected: if mode == "waiting" { 0 } else { 1 },
            };
            page.snap = Some(Snapshot {
                running: true,
                peers,
                outstanding: if mode == "syncing" { 3 } else { 0 },
            });
        }
        if mode == "busy" {
            page.prefs.enabled = false;
            page.busy = Some("Turning Syncthing on");
        }
        if mode == "open" {
            page.cfg = Config::parse(include_str!("fixtures/syncthing-rocknix.xml")).ok();
        }
        // "sync:on:<row>" selects a row.
        page.sel = match (mode, row) {
            (_, Some(row)) => row.min(ROWS.len() - 1),
            ("bios", _) => ROWS.len() - 1,
            ("library", _) => ROWS.iter().position(|r| *r == pocket_ui::sync_page::Row::Share(sync::Share::Library)).unwrap_or(0),
            _ => 0,
        };
        self.sync.page = Some(page);
        self.settings_page = Some(SettingsPage::Sync);
        if let Some(what) = mode.strip_prefix("conflicts") {
            self.sync_conflicts_snapshot(what.trim_start_matches('-'));
        }
        true
    }

    pub(crate) fn paint_sync(&mut self, c: &mut Canvas) {
        if self.conflicts_over_sync() {
            return self.paint_conflicts(c);
        }
        let Some(page) = self.sync.page.take() else { return };
        c.clear(pal().void);
        // Rows a little tighter than other pages' 54 px: nine of them leave room for the device ID and the web interface
        // (taller with Settings → Text size, where the list scrolls instead).
        let (pad, row, font) = (NU_PAD, pocket_ui::type_scale::row(49, 34), 34u32);
        self.sheet_header(c, "Sync");
        let (top, right) = (pad + NU_PILL + 12, c.w - pad);
        // The details under the list keep their room above the hints: with larger text (Settings → Text size) the
        // list shows fewer rows and scrolls, with a bar at the edge.
        let need = DETAILS_H + 6 * pocket_ui::type_scale::grow(28);
        let visible = (((c.h - pad - NU_PILL - 10 - need - 14 - top) / row).max(3) as usize).min(ROWS.len());
        let first = super::window_start(page.sel, visible, ROWS.len());
        if ROWS.len() > visible {
            let track = visible as i32 * row;
            pocket_ui::kit::scrollbar(c, top, track, first, visible, ROWS.len());
        }
        for (i, r) in ROWS.iter().enumerate().skip(first).take(visible) {
            let y = top + (i - first) as i32 * row;
            self.hits.row(Area::new(pad, y, right - pad, row), i);
            let base = y + row / 2 + pocket_ui::type_scale::mid(font);
            let name = r.label();
            if i == page.sel {
                let w = self.text.width(Face::Body, font, name) + 2 * NU_BTN_PAD;
                c.round_rect(pad, y + 4, right - pad, row - 8, (row - 8) / 2, pal().band);
                c.round_rect(pad, y + 4, w, row - 8, (row - 8) / 2, pal().select);
                self.text.draw(c, Face::Body, font, pad + NU_BTN_PAD, base, pal().ink, name);
            } else {
                self.text.draw(c, Face::Body, font, pad + NU_BTN_PAD, base, pal().text, name);
            }
            // The switch row is the status of the service; the recommended shares are marked.
            let value = page.value(*r);
            self.text.draw_right(c, Face::Body, font, right - NU_BTN_PAD, base, pal().text, &value);
        }
        self.hits.list(page.sel, ROWS.len(), 0.0, row as f32);

        self.paint_sync_details(c, &page, top + visible as i32 * row + 14);

        let a = if page.row() == pocket_ui::sync_page::Row::Versions { "Open" } else { "Switch" };
        let hints_at = self.nu_hints(c, &[("B", "Back"), ("A", a)]);
        let x = pad + NU_BTN_PAD;
        let desc = self.text.fit(Face::Body, NU_SMALL, page.describe(page.row()), hints_at - 24 - x);
        self.text
            .draw(c, Face::Body, NU_SMALL, x, c.h - pad - NU_PILL / 2 + pocket_ui::type_scale::mid(NU_SMALL), pal().text, &desc);
        self.sync.page = Some(page);
    }

    /// Below the list: the status, then (when it is on) the device ID with its QR code and the web interface.
    fn paint_sync_details(&mut self, c: &mut Canvas, page: &SyncPage, y0: i32) {
        // Each line a little lower than the one before with larger text: by the part of its type's growth above the
        // baseline (`d1` .. `d6`, all 0 in Normal).
        let g = |px: u32| pocket_ui::type_scale::grow(px) * 7 / 10;
        let d1 = g(44);
        let d2 = d1 + g(24);
        let d3 = d2 + g(37);
        let id_step = pocket_ui::type_scale::line(37, 5);
        let d4 = d3 + pocket_ui::type_scale::grow(37) + g(24);
        let d5 = d4 + g(37);
        let d6 = d5 + g(28);
        let left = NU_PAD + NU_BTN_PAD;
        let right = c.w - NU_PAD - NU_BTN_PAD;
        let label = |app: &mut App, c: &mut Canvas, y: i32, s: &str| {
            app.text.draw(c, Face::Body, 24, left, y, pal().dust, s);
        };
        // Status
        label(self, c, y0 + 20, "Status");
        let (status, color) = match (&page.error, page.busy) {
            (Some(e), _) => (self.text.fit(Face::Title, 37, e, right - left), pal().alert),
            (None, Some(b)) => (format!("{b}{}", ".".repeat(1 + (self.started.elapsed().as_millis() / 400 % 3) as usize)), pal().dust),
            (None, None) => (sync::status_line(page.on(), page.snap.as_ref()), if page.on() { pal().chalk } else { pal().dust }),
        };
        self.text.draw(c, Face::Title, 44, left, y0 + 64 + d1, color, &status);

        if !page.on() {
            let lines = [
                "Syncthing keeps your saves and save states the same on your computer, phone and other handhelds.",
                "Turn it on, then add this Pocket in Syncthing on your other device: scan the QR code or type the device ID.",
                "Nothing leaves the Pocket until you pair a device. BIOS stays home unless you share it.",
            ];
            let mut y = y0 + 110 + d1 + g(26);
            // Above the hints' line (and the Wi-Fi line under them when there is one); with larger text the last
            // paragraph that reaches it is shortened with "…".
            let step = pocket_ui::type_scale::line(26, 8);
            let bottom = c.h - NU_PAD - NU_PILL - 10 - if page.ip.is_none() { step + 10 } else { 0 };
            for l in lines {
                let room = if y > bottom { 0 } else { ((bottom - y) / step + 1) as usize };
                for line in self.text.wrap_at_most(Face::Body, 26, l, right - left, room) {
                    self.text.draw(c, Face::Body, 26, left, y, pal().dust, &line);
                    y += step;
                }
                y += 10;
            }
            if page.ip.is_none() {
                self.text
                    .draw(c, Face::Body, 28, left, y + 10, pal().alert, "Connect to Wi-Fi first (Settings › Connections › Wi-Fi).");
            }
            return;
        }

        // Device ID: two lines of four groups, with the QR code at the right.
        let qr = 260;
        let qx = right - qr;
        label(self, c, y0 + 104 + d2, "Device ID");
        match page.cfg.as_ref() {
            Some(cfg) if !cfg.device_id.is_empty() => {
                let groups: Vec<&str> = cfg.device_id.split('-').collect();
                for (i, line) in groups.chunks(4).enumerate() {
                    self.text.draw(c, Face::Title, 37, left, y0 + 146 + d3 + i as i32 * id_step, pal().chalk, &line.join("-"));
                }
                paint_qr(c, qx, y0 + 4, qr, &cfg.device_id);
            }
            _ => {
                self.text.draw(c, Face::Title, 37, left, y0 + 146 + d3, pal().dust, "Waiting for Syncthing");
            }
        }
        // Web interface
        label(self, c, y0 + 232 + d4, "Web interface (to pair from a computer)");
        let url = page.cfg.as_ref().map_or_else(
            || format!("https://{}:{}", page.ip.as_deref().unwrap_or("<device address>"), sync::GUI_PORT),
            |cfg| cfg.gui_url(page.ip.as_deref()),
        );
        self.text.draw(c, Face::Title, 37, left, y0 + 274 + d5, pal().chalk, &url);
        // The login is what the config says; the password is ours only when the user is (ROCKNIX's script logs in as root).
        let user = page.cfg.as_ref().map_or(sync::GUI_USER, |cfg| cfg.gui_user.as_str());
        let password = if user == sync::GUI_USER {
            page.password.as_deref().unwrap_or("(not set yet)")
        } else {
            "root's password"
        };
        let login = format!("User {user}   Password {password}");
        let problems = page.problems();
        // With larger text (Settings → Text size) the last line can run into the hints: a problem then takes the
        // login's line, and the note about the certificate is left out.
        let (login_y, note_y) = (y0 + 314 + d6, y0 + 352 + d6 + g(24));
        let last = if note_y > c.h - NU_PAD - NU_PILL - 10 { login_y } else { note_y };
        if last == note_y || problems.is_empty() {
            // Cut with "…" where it is wider than the page (larger text, a long password).
            let login = self.text.fit(Face::Body, 28, &login, right - left);
            self.text.draw(c, Face::Body, 28, left, login_y, pal().chalk, &login);
        }
        if let Some(p) = problems.first() {
            let msg = match p {
                Problem::NoPassword => "The web interface has no password. Turn Syncthing off and on again.",
                Problem::RocknixLogin => "The web interface uses ROCKNIX's login (root). Turn Syncthing off and on again to secure it.",
                Problem::ClearText => "The web interface is not encrypted. Turn Syncthing off and on again.",
            };
            let msg = self.text.fit(Face::Body, 24, msg, right - left);
            self.text.draw(c, Face::Body, 24, left, last, pal().alert, &msg);
        } else if last == note_y {
            self.text.draw(
                c,
                Face::Body,
                24,
                left,
                note_y,
                pal().dust,
                "Your browser will warn about the certificate: that is Syncthing's own, accept it once.",
            );
        }
    }
}

/// The details under the Sync list, from the list's end to the baseline of the login line (Normal sizes).
const DETAILS_H: i32 = 314;

/// A QR code of `data`, `size` pixels square including a white margin; dark modules in black.
pub(crate) fn paint_qr(c: &mut Canvas, x: i32, y: i32, size: i32, data: &str) {
    use qrcode::{Color, EcLevel, QrCode};
    let Ok(code) = QrCode::with_error_correction_level(data, EcLevel::M) else {
        return;
    };
    let modules = code.width() as i32;
    let quiet = 2;
    let cell = (size / (modules + 2 * quiet)).max(1);
    let total = cell * (modules + 2 * quiet);
    let (x, y) = (x + (size - total) / 2, y);
    c.round_rect(x, y, total, total, 12, 0xffffff);
    for (i, color) in code.to_colors().into_iter().enumerate() {
        if color == Color::Dark {
            let (mx, my) = (i as i32 % modules, i as i32 / modules);
            c.rect(x + (quiet + mx) * cell, y + (quiet + my) * cell, cell, cell, 0x000000);
        }
    }
}
