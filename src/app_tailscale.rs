//! Part of the pocket-ui binary: the App's side of `pocket_ui::tailscale` and `pocket_ui::tailscale_page`.
//! Settings > Tailscale, the poller that keeps its status current, and the sign-in screen with its QR code.
//! Every call into the system runs through `tailscale::spawn`: the UI thread only hands work over and paints.

use std::path::Path;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use pocket_ui::gfx::Canvas;
use pocket_ui::pad::Button;
use pocket_ui::palette::*;
use pocket_ui::tailscale::{self, Login, Prefs, Snapshot, State, Status, Tools};
use pocket_ui::tailscale_page::{Action, Event, Row, TailscalePage, View, ROWS};
use pocket_ui::text::Face;
use pocket_ui::touch::Area;

use super::{App, SettingsPage, UserEvent, CANCEL, CONFIRM, NU_BTN_PAD, NU_PAD, NU_PILL, NU_SMALL};

/// How often the open page asks Tailscale how it is.
const POLL_EVERY: Duration = Duration::from_secs(2);

/// What the App keeps of Tailscale: the name chosen, the open page, and whether it was on the last time we looked.
pub(crate) struct TailscaleUi {
    prefs: Prefs,
    tools: Tools,
    page: Option<TailscalePage>,
    /// Shown in the Settings list without opening the page: the setting ROCKNIX starts it by, then what the page saw.
    on: bool,
    /// The last poll found a network that is not Wi-Fi (a cable): the status corner only reads Wi-Fi.
    wired: bool,
}

impl TailscaleUi {
    pub fn load(data_dir: &Path) -> TailscaleUi {
        let tools = Tools::default();
        let on = std::fs::read_to_string(&tools.system_cfg).is_ok_and(|cfg| tailscale::boot_enabled(&cfg));
        TailscaleUi {
            prefs: Prefs::load(data_dir),
            tools,
            page: None,
            on,
            wired: false,
        }
    }
}

/// A small number for what the page shows, for the haptics and sounds that answer a change.
fn view_kind(v: &View) -> u8 {
    match v {
        View::List => 0,
        View::Login { .. } => 1,
        View::Keyboard(..) => 2,
    }
}

impl App {
    /// Text for the Settings list: "On >" / "Off >", and "No network yet >" while it is on but Wi-Fi is not joined.
    /// From memory only: the status corner's last read of Wi-Fi, and what the page last saw of a cable.
    pub(crate) fn tailscale_list_value(&self) -> String {
        let t = &self.tailscale;
        let offline = t.on && !pocket_ui::status_corner::status().wifi && !t.wired;
        format!("{} >", if offline { tailscale::NO_NETWORK } else if t.on { "On" } else { "Off" })
    }

    pub(crate) fn open_tailscale(&mut self) -> Option<SettingsPage> {
        let page = TailscalePage::new(self.tailscale.prefs.hostname.clone());
        let (alive, proxy, tools) = (page.alive.clone(), self.proxy.clone(), self.tailscale.tools.clone());
        // The page asks now and then while it is open. `snapshot` is bounded by timeouts, and the UI never waits for it.
        std::thread::spawn(move || {
            while alive.load(Ordering::Relaxed) {
                let snap = tailscale::snapshot(&tools);
                let _ = proxy.send_event(UserEvent::Tailscale(Event::Poll(snap)));
                let until = Instant::now() + POLL_EVERY;
                while alive.load(Ordering::Relaxed) && Instant::now() < until {
                    std::thread::sleep(Duration::from_millis(100));
                }
            }
        });
        self.tailscale.page = Some(page);
        Some(SettingsPage::Tailscale)
    }

    fn send_tailscale(proxy: &winit::event_loop::EventLoopProxy<UserEvent>, e: Event) {
        let _ = proxy.send_event(UserEvent::Tailscale(e));
    }

    pub(crate) fn tailscale_input(&mut self, b: Button) -> Option<SettingsPage> {
        let (proxy, tools) = (self.proxy.clone(), self.tailscale.tools.clone());
        let hostname = self.tailscale.prefs.hostname.clone();
        let Some(page) = self.tailscale.page.as_mut() else {
            return Some(SettingsPage::List(super::settings_row("Tailscale")));
        };
        let action = page.press(b, CONFIRM, CANCEL);
        match action {
            Action::Nothing => {}
            Action::Close => {
                self.tailscale.page = None;
                return Some(SettingsPage::List(super::settings_row("Tailscale")));
            }
            Action::SetEnabled(on) => {
                let tx = proxy.clone();
                tailscale::spawn(
                    "tailscale-switch",
                    move || {
                        if on {
                            tailscale::enable(&tools, hostname.as_deref())
                        } else {
                            tailscale::disable(&tools)
                        }
                    },
                    move |result| Self::send_tailscale(&tx, Event::Switched { on, result }),
                );
            }
            Action::StartLogin | Action::LoginWithKey(_) => {
                let key = if let Action::LoginWithKey(key) = &action { Some(key.clone()) } else { None };
                let cancel = page.cancel.clone();
                let tx = proxy.clone();
                tailscale::spawn(
                    "tailscale-login",
                    move || {
                        // Switching on first keeps "Sign in" one step: the service runs, then `tailscale up` asks for the login.
                        tailscale::enable(&tools, hostname.as_deref())?;
                        let url_tx = proxy.clone();
                        let login = Login {
                            hostname: hostname.as_deref(),
                            auth_key: key.as_deref(),
                        };
                        let result = tailscale::login(&tools, &login, &cancel, &mut |url| Self::send_tailscale(&url_tx, Event::LoginUrl(url)));
                        // The network went while it ran: that is the news, not a timeout or a dial error.
                        if result.as_ref().is_err_and(|e| e != "Cancelled") { tailscale::failed_offline(&tools, result) } else { result }
                    },
                    move |result| Self::send_tailscale(&tx, Event::LoginDone(result)),
                );
            }
            Action::Logout => {
                tailscale::spawn(
                    "tailscale-logout",
                    move || tailscale::failed_offline(&tools, tailscale::logout(&tools)),
                    move |result| Self::send_tailscale(&proxy, Event::LoggedOut(result)),
                );
            }
            Action::SetHostname(name) => {
                // While the service runs the new name is applied at once; otherwise it is used at the next sign-in.
                let running = page.on();
                let sent = name.clone();
                tailscale::spawn(
                    "tailscale-hostname",
                    move || if running { tailscale::set_hostname(&tools, &sent) } else { Ok(()) },
                    move |result| Self::send_tailscale(&proxy, Event::HostnameSet { name, result }),
                );
            }
        }
        Some(SettingsPage::Tailscale)
    }

    pub(crate) fn tailscale_event(&mut self, e: Event) {
        if let Event::HostnameSet { name, result: Ok(()) } = &e {
            self.tailscale.prefs.hostname = Some(name.clone());
            self.tailscale.prefs.save(&self.data_dir);
        }
        if let Event::Poll(snap) = &e {
            // Online while the corner shows no Wi-Fi: a cable.
            self.tailscale.wired = !snap.offline && !pocket_ui::status_corner::status().wifi;
        }
        if let Some(page) = self.tailscale.page.as_mut() {
            page.event(e);
            // The page knows which poll to believe (a poll under way during a switch is not); the list shows what it shows.
            if page.snap.is_some() {
                self.tailscale.on = page.on();
            }
        }
    }

    /// Where the page's selection and switch stand: a change is what the menu sounds answer.
    pub(crate) fn tailscale_row(&self) -> Option<(usize, bool, u8)> {
        self.tailscale.page.as_ref().map(|p| (p.sel, p.on(), view_kind(&p.view)))
    }

    /// Something moves on screen: the switch is working, or the sign-in waits for its address.
    pub(crate) fn tailscale_animating(&self) -> bool {
        // Asked after every frame: never waits for the state read after the first frame (`startup::Later`).
        self.tailscale
            .loaded()
            .and_then(|t| t.page.as_ref())
            .is_some_and(|p| p.busy.is_some() || matches!(p.view, View::Login { url: None, .. }))
    }

    /// `--snapshot tailscale[:off|:loggedout|:login|:connected|:busy|:key|:name|:error|:offline|:offline-loggedout|:offline-note]` draws the page with made-up data.
    pub(crate) fn tailscale_snapshot(&mut self, state: &str) -> bool {
        let Some(rest) = state.strip_prefix("tailscale") else { return false };
        let mode = rest.trim_start_matches(':');
        let status = |json: &str| Some(Status::parse(json).expect("fixture"));
        let mut snap = Snapshot {
            installed: true,
            running: mode != "off" && !mode.is_empty(),
            boot: true,
            status: None,
            error: None,
            offline: mode.starts_with("offline"),
        };
        snap.status = match mode {
            // Offline: Tailscale still says Running, with its old address. That is what it does, and what must not show.
            "connected" | "busy" | "name" | "offline" => status(include_str!("fixtures/tailscale-status-running.json")),
            "loggedout" | "login" | "key" | "error" | "offline-loggedout" | "offline-note" => {
                status(include_str!("fixtures/tailscale-status-needslogin.json"))
            }
            _ => None,
        };
        let mut page = TailscalePage::new(None);
        page.event(Event::Poll(snap));
        match mode {
            "login" => {
                page.sel = 1;
                page.press(Button::East, Button::East, Button::South);
                page.event(Event::LoginUrl("https://login.tailscale.com/a/1b2c3d4e5f6a7b".into()));
            }
            "key" => {
                page.sel = 2;
                page.press(Button::East, Button::East, Button::South);
                if let View::Keyboard(_, kb) = &mut page.view {
                    kb.text = concat!("tsk", "ey-auth-kQ7xN2").into();
                }
            }
            "name" => {
                page.sel = 3;
                page.press(Button::East, Button::East, Button::South);
            }
            "offline-note" => {
                page.sel = 1;
                page.press(Button::East, Button::East, Button::South);
            }
            "busy" => page.busy = Some("Turning Tailscale on"),
            "error" => page.error = Some("Timed out".into()),
            _ => {}
        }
        self.tailscale.page = Some(page);
        self.settings_page = Some(SettingsPage::Tailscale);
        true
    }

    pub(crate) fn paint_tailscale(&mut self, c: &mut Canvas) {
        let Some(page) = self.tailscale.page.take() else { return };
        c.clear(pal().void);
        match &page.view {
            View::List => self.paint_tailscale_list(c, &page),
            View::Login { url, by_key } => self.paint_tailscale_login(c, url.as_deref(), *by_key),
            View::Keyboard(typing, kb) => {
                let heading = match typing {
                    pocket_ui::tailscale_page::Typing::AuthKey => "Auth key",
                    pocket_ui::tailscale_page::Typing::Hostname => "Name on your tailnet",
                };
                self.text.draw(c, Face::Title, 50, MARGIN, 110, pal().chalk, heading);
                pocket_ui::setup::paint_keyboard_with(c, &mut self.text, kb, "Done");
                let note = match typing {
                    pocket_ui::tailscale_page::Typing::AuthKey => "A key made in the Tailscale admin console (Settings › Keys). It is not kept on the Pocket.",
                    pocket_ui::tailscale_page::Typing::Hostname => "Letters, digits and hyphens. The Pocket will answer to this name on your other devices.",
                };
                let note = self.text.fit(Face::Body, 24, note, c.w - 2 * MARGIN);
                self.text.draw(c, Face::Body, 24, MARGIN, 790, pal().dust, &note);
                pocket_ui::setup::hints(
                    c,
                    &mut self.text,
                    &[("A", "Type"), ("B", "Delete"), ("X", "Shift"), ("Y", "Symbols"), ("START", "Done")],
                );
            }
        }
        self.tailscale.page = Some(page);
    }

    fn paint_tailscale_list(&mut self, c: &mut Canvas, page: &TailscalePage) {
        let (pad, row, font) = (NU_PAD, pocket_ui::type_scale::row(54, 34), 34u32);
        self.sheet_header(c, "Tailscale");
        let (top, right) = (pad + NU_PILL + 12, c.w - pad);
        for (i, r) in ROWS.iter().enumerate() {
            let y = top + i as i32 * row;
            self.hits.row(Area::new(pad, y, right - pad, row), i);
            let base = y + row / 2 + pocket_ui::type_scale::mid(font);
            let name = page.label(*r);
            if i == page.sel {
                let w = self.text.width(Face::Body, font, name) + 2 * NU_BTN_PAD;
                c.round_rect(pad, y + 4, right - pad, row - 8, (row - 8) / 2, pal().band);
                c.round_rect(pad, y + 4, w, row - 8, (row - 8) / 2, pal().select);
                self.text.draw(c, Face::Body, font, pad + NU_BTN_PAD, base, pal().ink, name);
            } else {
                self.text.draw(c, Face::Body, font, pad + NU_BTN_PAD, base, pal().text, name);
            }
            let value = page.value(*r);
            let value = self.text.fit(Face::Body, font, &value, right - pad - 2 * NU_BTN_PAD - 420);
            self.text.draw_right(c, Face::Body, font, right - NU_BTN_PAD, base, pal().text, &value);
        }
        self.hits.list(page.sel, ROWS.len(), 0.0, row as f32);

        self.paint_tailscale_details(c, page, top + ROWS.len() as i32 * row + 14);

        let hints_at = self.nu_hints(c, &[("B", "Back"), ("A", if page.row() == Row::Switch { "Switch" } else { "Open" })]);
        let x = pad + NU_BTN_PAD;
        let desc = self.text.fit(Face::Body, NU_SMALL, page.describe(page.row()), hints_at - 24 - x);
        self.text
            .draw(c, Face::Body, NU_SMALL, x, c.h - pad - NU_PILL / 2 + pocket_ui::type_scale::mid(NU_SMALL), pal().text, &desc);
    }

    /// Below the list: the status, and what to do or what the Pocket is called.
    fn paint_tailscale_details(&mut self, c: &mut Canvas, page: &TailscalePage, y0: i32) {
        let left = NU_PAD + NU_BTN_PAD;
        let right = c.w - NU_PAD - NU_BTN_PAD;
        let snap = page.snap.clone().unwrap_or_default();
        // The state in a panel, as Performance & fan's readout (`kit::panel`): with the address, the name and the
        // devices while connected.
        let connected_rows = snap.status.as_ref().filter(|_| snap.state() == State::Connected).map_or(0, |s| if s.peers_total > 0 { 3 } else { 2 });
        let panel_h = 104 + pocket_ui::type_scale::grow(44) + connected_rows * 50;
        pocket_ui::kit::panel(c, NU_PAD, y0 - 10, c.w - 2 * NU_PAD, panel_h);
        // What follows the panel starts this far under it.
        let below = y0 - 10 + panel_h + 40;
        self.text.draw(c, Face::Body, 24, left, y0 + 20, pal().dust, "Status");
        let (status, color) = match (&page.error, page.busy) {
            (Some(e), _) => (self.text.fit(Face::Title, 37, e, right - left), pal().alert),
            (None, Some(b)) => (format!("{b}{}", ".".repeat(1 + (self.started.elapsed().as_millis() / 400 % 3) as usize)), pal().dust),
            (None, None) => {
                let line = if page.snap.is_some() { snap.status_line() } else { "Looking".to_string() };
                (
                    self.text.fit(Face::Title, 44, &line, right - left),
                    if snap.state() == State::Connected { pal().chalk } else { pal().dust },
                )
            }
        };
        self.text.draw(c, Face::Title, 44, left, y0 + 64, color, &status);

        let mut y = y0 + 110 + pocket_ui::type_scale::grow(44);
        let status = snap.status.as_ref().filter(|_| snap.state() == State::Connected);
        if let Some(s) = status {
            let label_w = 190;
            let mut line = |app: &mut App, c: &mut Canvas, name: &str, value: &str| {
                app.text.draw(c, Face::Body, 26, left, y, pal().dust, name);
                let value = app.text.fit(Face::Title, 37, value, right - left - label_w);
                app.text.draw(c, Face::Title, 37, left + label_w, y + 2, pal().chalk, &value);
                y += 50;
            };
            line(self, c, "Address", s.ipv4().unwrap_or("(none yet)"));
            line(self, c, "Name", if s.dns_name.is_empty() { &s.hostname } else { &s.dns_name });
            if s.peers_total > 0 {
                line(self, c, "Devices", &format!("{} of {} online", s.peers_online, s.peers_total));
            }
            y = below;
            let note = "Tailscale only sets up the network. To log in with SSH, switch on Remote access too.";
            for l in self.text.wrap(Face::Body, 24, note, right - left) {
                self.text.draw(c, Face::Body, 24, left, y, pal().dust, &l);
                y += pocket_ui::type_scale::line(24, 8);
            }
            return;
        }
        y = below;
        // What a press could not do without a network: said calmly, in the page's own colour, not as an error.
        if let Some(note) = page.note {
            for line in self.text.wrap(Face::Body, 26, note, right - left) {
                self.text.draw(c, Face::Body, 26, left, y, pal().text, &line);
                y += pocket_ui::type_scale::line(26, 8);
            }
            y += 10;
        }
        // The service runs but does not answer: say what `tailscale` said, under the status.
        if let Some(e) = snap.error.as_ref().filter(|_| snap.state() == State::Starting) {
            let e = self.text.fit(Face::Body, 26, e, right - left);
            self.text.draw(c, Face::Body, 26, left, y, pal().alert, &e);
            y += 44;
        }
        let lines: &[&str] = match snap.state() {
            State::NotInstalled => &["This system image has no Tailscale."],
            State::NoNetwork => &["Tailscale connects once you're online. Connect to Wi-Fi or a hotspot in Settings › Connections."],
            State::LoggedOut => &["Select Sign in: a QR code appears. Scan it with your phone and approve this Pocket there."],
            State::WaitingForApproval => &["Approve this Pocket in the Tailscale admin console, then it connects by itself."],
            _ => &[
                "Tailscale links the Pocket with your phone, computer and other devices in one private network, over Wi-Fi or a hotspot, wherever you are.",
                "Switch it on, then sign in with a QR code. It stays on after a restart.",
            ],
        };
        for l in lines {
            for line in self.text.wrap(Face::Body, 26, l, right - left) {
                self.text.draw(c, Face::Body, 26, left, y, pal().dust, &line);
                y += pocket_ui::type_scale::line(26, 8);
            }
            y += 10;
        }
        // No network to reach the tailnet over: the small radio asleep under the words, aligned with them, when there
        // is room (`ui_art`; docs/DESIGN.md "Empty pages").
        if snap.state() == State::NoNetwork {
            let k = pocket_ui::ui_art::empty_scale(c.h);
            let (_, ah) = pocket_ui::ui_art::size(pocket_ui::ui_art::Art::Offline, k);
            let y = y + 24;
            if y + ah <= c.h - NU_PAD - NU_PILL - 24 {
                pocket_ui::ui_art::paint(c, pocket_ui::ui_art::Art::Offline, left, y, k);
            }
        }
    }

    /// The sign-in screen: the address as a QR code, the same address as short text, and what to do.
    fn paint_tailscale_login(&mut self, c: &mut Canvas, url: Option<&str>, by_key: bool) {
        let pad = NU_PAD;
        self.sheet_header(c, "Sign in to Tailscale");
        let (left, top) = (pad + NU_BTN_PAD, pad + NU_PILL + 30);
        let dots = ".".repeat(1 + (self.started.elapsed().as_millis() / 400 % 3) as usize);
        match url {
            Some(url) if !by_key => {
                let qr = 520;
                super::app_sync::paint_qr(c, left, top, qr, url);
                let tx = left + qr + 50;
                let width = c.w - pad - NU_BTN_PAD - tx;
                let steps = [
                    "1.  Open the camera on your phone.",
                    "2.  Point it at the code.",
                    "3.  Sign in and approve this Pocket.",
                ];
                let mut y = top + 40;
                for s in steps {
                    let s = self.text.fit(Face::Body, 30, s, width);
                    self.text.draw(c, Face::Body, 30, tx, y, pal().text, &s);
                    y += 52;
                }
                y += 24;
                self.text.draw(c, Face::Body, 24, tx, y, pal().dust, "Or type this address into a browser");
                y += 52;
                let short = url.trim_start_matches("https://").trim_start_matches("http://");
                // Host on one line, the path on the next: the whole address is wider than the space left of the code.
                let (host, path) = short.split_once('/').map_or((short, String::new()), |(h, p)| (h, format!("/{p}")));
                for line in [host, path.as_str()].into_iter().filter(|l| !l.is_empty()) {
                    let line = self.text.fit(Face::Title, 33, line, width);
                    self.text.draw(c, Face::Title, 33, tx, y, pal().chalk, &line);
                    y += 44;
                }
                y += 30;
                let waiting = format!("Waiting for approval{dots}");
                self.text.draw(c, Face::Body, 28, tx, y, pal().dust, &waiting);
            }
            _ => {
                let msg = if by_key { "Signing in with the key" } else { "Getting the sign-in address" };
                self.text.draw(c, Face::Title, 44, left, top + 80, pal().chalk, &format!("{msg}{dots}"));
            }
        }
        self.nu_hints(c, &[("B", "Cancel")]);
    }
}
