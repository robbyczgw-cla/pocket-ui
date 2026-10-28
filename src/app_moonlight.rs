//! Part of the pocket-ui binary: the App's side of `pocket_ui::moonlight` (pairing) and `pocket_ui::moonlight_page`.
//! Settings > PC streaming: pair a PC on the device with its PIN shown big, the paired PCs, unpairing, and the
//! PC's games (written for ROCKNIX too, and streamed from here). Every call to Moonlight runs on a thread.

use std::io::BufRead;
use std::path::Path;
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use pocket_ui::gfx::Canvas;
use pocket_ui::moonlight::{self, PairNews};
use pocket_ui::moonlight_page::{Action, Event, Loaded, MoonlightPage, Row};
use pocket_ui::pad::Button;
use pocket_ui::palette::*;
use pocket_ui::text::Face;
use pocket_ui::touch::Area;

use super::{App, SettingsPage, UserEvent, CANCEL, CONFIRM, NU_BTN_PAD, NU_PAD, NU_PILL, NU_SMALL};

/// The name in the Settings list, to come back to.
pub(crate) const SETTINGS_NAME: &str = "PC streaming";
/// Below the list: what the selected row does, or the last result.
const DETAILS_H: i32 = 250;

/// What the App keeps: the open page, and the process group of a pairing under way (0: none), to stop it.
#[derive(Default)]
pub(crate) struct MoonlightUi {
    page: Option<MoonlightPage>,
    pairing: Arc<AtomicI32>,
}

fn send(proxy: &winit::event_loop::EventLoopProxy<UserEvent>, e: Event) {
    let _ = proxy.send_event(UserEvent::Moonlight(e));
}

/// What the page shows when it opens: our list of PCs (with ROCKNIX's `moonlight.host` in it), and its games.
fn load(data_dir: &Path) -> Loaded {
    let default = moonlight::host();
    let mut pcs = moonlight::load_pcs(data_dir);
    if let Some(d) = default.as_ref().filter(|d| !pcs.contains(d)) {
        pcs.insert(0, d.clone());
    }
    let games = if default.is_some() { moonlight::written_games(Path::new(moonlight::GAMES_DIR)) } else { Vec::new() };
    Loaded { installed: moonlight::installed(), pcs, default, games }
}

/// Makes `host` the PC that streaming on Home uses (ROCKNIX's `moonlight.host`, which EmulationStation used too), keeps
/// it on our list, and writes its games like ROCKNIX's menu did.
fn use_pc(data_dir: &Path, host: &str) -> Result<Vec<String>, String> {
    let mut pcs = moonlight::load_pcs(data_dir);
    if !pcs.iter().any(|p| p == host) {
        pcs.push(host.to_string());
        if let Err(e) = moonlight::save_pcs(data_dir, &pcs) {
            eprintln!("pc streaming: can't keep the list of PCs: {e}");
        }
    }
    // SYSTEM.CFG HELPER: the one marked writer of feat/tools-wifi (`wifi::set_rocknix_setting`).
    pocket_ui::wifi::set_rocknix_setting("moonlight.host", host);
    let apps = moonlight::list_apps(&moonlight::bin(), host)?;
    moonlight::write_games(Path::new(moonlight::GAMES_DIR), host, &apps).map_err(|e| format!("Couldn't write the games: {e}"))?;
    Ok(moonlight::written_games(Path::new(moonlight::GAMES_DIR)))
}

/// `moonlight pair [host]` with its lines sent to the page; stopped after `moonlight::PAIR_SECS` or by `pgid` being
/// signalled (B). Returns the PC it paired with.
fn pair(host: Option<String>, pgid: &AtomicI32, proxy: &winit::event_loop::EventLoopProxy<UserEvent>) -> Result<String, String> {
    use std::os::unix::process::CommandExt;
    let mut cmd = moonlight::pair_command(&moonlight::bin(), host.as_deref());
    cmd.stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::null()).process_group(0);
    #[cfg(target_os = "linux")]
    pocket_ui::control::die_with_parent(&mut cmd);
    let mut child = cmd.spawn().map_err(|e| format!("Moonlight didn't start ({e})."))?;
    let id = child.id() as i32;
    pgid.store(id, Ordering::SeqCst);
    // The PIN waits for the PC for so long at most.
    let watch = Arc::new(AtomicI32::new(id));
    let w = watch.clone();
    std::thread::spawn(move || {
        let until = Instant::now() + Duration::from_secs(moonlight::PAIR_SECS);
        while Instant::now() < until && w.load(Ordering::SeqCst) == id {
            std::thread::sleep(Duration::from_millis(250));
        }
        if w.load(Ordering::SeqCst) == id {
            // SAFETY: kill(2) on the process group pair started above.
            unsafe { libc::kill(-id, libc::SIGTERM) };
        }
    });
    let (mut found, mut ended): (Option<String>, Option<Result<(), String>>) = (host, None);
    if let Some(out) = child.stdout.take() {
        for line in std::io::BufReader::new(out).lines().map_while(Result::ok) {
            println!("pc streaming: moonlight pair: {line}");
            let Some(news) = moonlight::parse_pair_line(&line) else { continue };
            match &news {
                PairNews::Connecting(h) => found = Some(h.clone()),
                PairNews::Paired => ended = Some(Ok(())),
                PairNews::Failed(why) => ended = Some(Err(why.clone())),
                _ => {}
            }
            send(proxy, Event::Pair(news));
        }
    }
    let status = child.wait();
    watch.store(0, Ordering::SeqCst);
    let stopped = pgid.swap(0, Ordering::SeqCst) == -1;
    match ended {
        Some(Ok(())) => found.filter(|h| moonlight::valid_host(h)).ok_or_else(|| "Paired, but Moonlight didn't say with which PC.".to_string()),
        Some(Err(why)) => Err(why),
        None if stopped => Err("Pairing stopped.".into()),
        None if status.as_ref().is_ok_and(|s| s.success()) => Err("Moonlight ended without pairing.".into()),
        None => Err("The PIN wasn't entered in time, or Moonlight stopped. Try again.".into()),
    }
}

impl App {
    pub(crate) fn open_pc_streaming(&mut self) -> Option<SettingsPage> {
        self.moonlight.page = Some(MoonlightPage::new());
        let (proxy, data_dir) = (self.proxy.clone(), self.data_dir.clone());
        std::thread::spawn(move || send(&proxy, Event::Loaded(load(&data_dir))));
        Some(SettingsPage::PcStreaming)
    }

    pub(crate) fn pc_streaming_input(&mut self, b: Button) -> Option<SettingsPage> {
        let back = Some(SettingsPage::List(super::settings_row(SETTINGS_NAME)));
        let Some(page) = self.moonlight.page.as_mut() else { return back };
        let (proxy, data_dir) = (self.proxy.clone(), self.data_dir.clone());
        // From memory, never waits (`wifi::ip`).
        page.offline = pocket_ui::wifi::ip().is_none();
        match page.press(b, CONFIRM, CANCEL) {
            Action::Nothing => {}
            Action::Close => {
                self.moonlight.page = None;
                return back;
            }
            Action::Pair(host) => {
                let pgid = self.moonlight.pairing.clone();
                std::thread::spawn(move || {
                    let result = pair(host, &pgid, &proxy);
                    let paired = result.clone();
                    send(&proxy, Event::PairEnded(result));
                    if let Ok(host) = paired {
                        let result = use_pc(&data_dir, &host);
                        send(&proxy, Event::Games { host, result });
                    }
                });
            }
            Action::CancelPair => {
                let id = self.moonlight.pairing.swap(-1, Ordering::SeqCst);
                if id > 0 {
                    // SAFETY: kill(2) on the process group of the pairing started by `pair`.
                    unsafe { libc::kill(-id, libc::SIGTERM) };
                }
            }
            Action::Games(host) => {
                std::thread::spawn(move || {
                    let result = use_pc(&data_dir, &host);
                    send(&proxy, Event::Games { host, result });
                });
            }
            Action::Unpair(host) => {
                std::thread::spawn(move || {
                    let result = moonlight::unpair(&moonlight::bin(), &host);
                    let mut pcs = moonlight::load_pcs(&data_dir);
                    pcs.retain(|p| *p != host);
                    let _ = moonlight::save_pcs(&data_dir, &pcs);
                    if moonlight::host().as_deref() == Some(host.as_str()) {
                        pocket_ui::wifi::set_rocknix_setting("moonlight.host", "default");
                    }
                    send(&proxy, Event::Unpaired { host, result });
                });
            }
            Action::Stream { host, app } => {
                self.settings_aside_for_app();
                self.start_streaming_with(moonlight::start_app(&host, &app), app);
                // Settings input sets the page from this: none while the stream runs.
                return None;
            }
        }
        Some(SettingsPage::PcStreaming)
    }

    /// Why a stream started from this page ended at once: said on the page.
    pub(crate) fn pc_streaming_note(&mut self, why: String) {
        if let Some(page) = self.moonlight.page.as_mut() {
            page.message = Some((why, true));
        }
    }

    pub(crate) fn pc_streaming_event(&mut self, e: Event) {
        if let Some(page) = self.moonlight.page.as_mut() {
            page.event(e);
        }
    }

    /// Where the page's selection stands: a change is what the menu sounds and haptics answer.
    pub(crate) fn pc_streaming_row(&self) -> Option<(usize, bool, bool, Option<String>)> {
        self.moonlight.page.as_ref().map(|p| (p.sel, p.address.is_some(), p.pairing.is_some(), p.address.as_ref().map(|k| format!("{}{}{}", k.text, k.row, k.col))))
    }

    /// Pairing waits, or work runs: the dots move.
    pub(crate) fn pc_streaming_animating(&self) -> bool {
        self.moonlight.page.as_ref().is_some_and(|p| p.pairing.is_some() || p.busy.is_some() || p.loaded.is_none())
    }

    /// `--snapshot pc-streaming[:<row>][:pin|:searching|:address|:unpair|:done|:none|:missing]`.
    pub(crate) fn pc_streaming_snapshot(&mut self, state: &str) -> bool {
        let Some(rest) = state.strip_prefix("pc-streaming") else { return false };
        let mut parts = rest.trim_start_matches(':').split(':');
        let first = parts.next().unwrap_or("");
        let (row, mode) = match first.parse::<usize>() {
            Ok(r) => (r, parts.next().unwrap_or("")),
            Err(_) => (0, first),
        };
        let mut page = MoonlightPage::new();
        let demo = Loaded {
            installed: mode != "missing",
            pcs: if matches!(mode, "none" | "missing") { Vec::new() } else { vec!["192.168.1.20".into(), "office-pc.lan".into()] },
            default: (!matches!(mode, "none" | "missing")).then(|| "192.168.1.20".into()),
            games: if matches!(mode, "none" | "missing") { Vec::new() } else { vec!["Desktop".into(), "Hades II".into(), "Steam Big Picture".into()] },
        };
        page.event(Event::Loaded(demo));
        page.sel = row.min(page.rows().len() - 1);
        match mode {
            "address" => {
                page.sel = 0;
                page.press(Button::East, Button::East, Button::South);
                if let Some(kb) = page.address.as_mut() {
                    kb.text = "192.168.1.".into();
                }
            }
            "searching" | "pin" => {
                page.sel = 0;
                page.press(Button::East, Button::East, Button::South);
                page.press(Button::Start, Button::East, Button::South);
                page.event(Event::Pair(PairNews::Searching));
                if mode == "pin" {
                    page.event(Event::Pair(PairNews::Connecting("192.168.1.30".into())));
                    page.event(Event::Pair(PairNews::Pin("4712".into())));
                }
            }
            "unpair" => {
                page.sel = 2;
                page.press(Button::North, Button::East, Button::South);
            }
            "done" => {
                page.event(Event::PairEnded(Ok("192.168.1.30".into())));
                page.event(Event::Games { host: "192.168.1.30".into(), result: Ok(vec!["Desktop".into(), "Steam Big Picture".into()]) });
                page.sel = 3;
            }
            _ => {}
        }
        self.moonlight.page = Some(page);
        self.settings_page = Some(SettingsPage::PcStreaming);
        true
    }

    pub(crate) fn paint_pc_streaming(&mut self, c: &mut Canvas) {
        let Some(page) = self.moonlight.page.take() else { return };
        c.clear(pal().void);
        if let Some(kb) = &page.address {
            self.text.draw(c, Face::Title, 50, 72, 110, pal().chalk, "Address of your PC");
            pocket_ui::setup::paint_keyboard_with(c, &mut self.text, kb, "Pair");
            let note = match &page.message {
                Some((m, true)) => m.clone(),
                _ => "Leave it empty to let Moonlight look for the PC on the network.".into(),
            };
            let color = if page.message.as_ref().is_some_and(|m| m.1) { pal().alert } else { pal().dust };
            let note = self.text.fit(Face::Body, NU_SMALL, &note, c.w - 144);
            self.text.draw(c, Face::Body, NU_SMALL, 72, 800, color, &note);
            pocket_ui::setup::hints(c, &mut self.text, &[("A", "Type"), ("B", "Delete"), ("X", "Shift"), ("Y", "Symbols"), ("START", "Pair")]);
        } else if let Some(p) = &page.pairing {
            self.paint_pairing(c, p);
        } else {
            self.paint_pc_list(c, &page);
        }
        self.moonlight.page = Some(page);
    }

    /// The PIN, big, while Moonlight waits for the PC.
    fn paint_pairing(&mut self, c: &mut Canvas, p: &pocket_ui::moonlight_page::Pairing) {
        let left = NU_PAD + NU_BTN_PAD;
        let dots = ".".repeat(1 + (self.started.elapsed().as_millis() / 400 % 3) as usize);
        self.status_pill(c);
        self.text.draw(c, Face::Title, 50, left, NU_PAD + 58, pal().chalk, "Pair a PC");
        let width = c.w - 2 * left;
        match &p.pin {
            Some(pin) => {
                let pc = p.host.clone().unwrap_or_else(|| "your PC".into());
                let head = self.text.fit(Face::Body, 34, &format!("Type this PIN on {pc}"), width);
                self.text.draw(c, Face::Body, 34, left, 250, pal().text, &head);
                // The digits apart, as they are typed, on a paper label.
                let spaced: String = pin.chars().map(|d| d.to_string()).collect::<Vec<_>>().join(" ");
                let w = self.text.width(Face::Title, 200, &spaced);
                let (bx, by, bw, bh) = ((c.w - w) / 2 - 60, 300, w + 120, 260);
                c.round_rect(bx, by, bw, bh, 28, pal().label);
                self.text.draw(c, Face::Title, 200, (c.w - w) / 2, by + 205, pal().ink, &spaced);
                let mut y = by + bh + 70;
                for line in [
                    "Sunshine: its web page (https://localhost:47990 on the PC), the PIN tab.".to_string(),
                    "GeForce Experience: the box it shows on the PC.".to_string(),
                    format!("Waiting for the PC{dots}"),
                ] {
                    for l in self.text.wrap(Face::Body, NU_SMALL, &line, width) {
                        self.text.draw(c, Face::Body, NU_SMALL, left, y, pal().dust, &l);
                        y += pocket_ui::type_scale::line(NU_SMALL, 14);
                    }
                }
            }
            None => {
                let head = match (&p.host, p.searching) {
                    (Some(h), _) => format!("Connecting to {h}{dots}"),
                    (None, true) => format!("Looking for your PC on the network{dots}"),
                    (None, false) => format!("Starting Moonlight{dots}"),
                };
                let head = self.text.fit(Face::Title, 44, &head, width);
                self.text.draw(c, Face::Title, 44, left, 320, pal().chalk, &head);
                for (i, l) in self.text.wrap(Face::Body, NU_SMALL, "The PC must be on, on the same network, with Sunshine or GeForce Experience running. The PIN comes up here in a moment.", width).iter().enumerate() {
                    self.text.draw(c, Face::Body, NU_SMALL, left, 380 + i as i32 * (pocket_ui::type_scale::line(NU_SMALL, 14)), pal().dust, l);
                }
            }
        }
        self.nu_hints(c, &[("B", "Stop")]);
    }

    fn paint_pc_list(&mut self, c: &mut Canvas, page: &MoonlightPage) {
        let (pad, row, font) = (NU_PAD, pocket_ui::type_scale::row(58, 34), 34u32);
        self.status_pill(c);
        self.text.draw(c, Face::Title, 50, pad + NU_BTN_PAD, pad + 58, pal().chalk, SETTINGS_NAME);
        let (top, right) = (pad + NU_PILL + 12, c.w - pad);
        let rows = page.rows();
        let list_bottom = c.h - pad - NU_PILL - 16 - DETAILS_H;
        let visible = (((list_bottom - top) / row) as usize).max(3);
        let first = super::window_start(page.sel, visible, rows.len());
        let dots = ".".repeat(1 + (self.started.elapsed().as_millis() / 400 % 3) as usize);
        for (i, r) in rows.iter().enumerate().skip(first).take(visible) {
            let y = top + (i - first) as i32 * row;
            self.hits.row(Area::new(pad, y, right - pad, row), i);
            let base = y + row / 2 + pocket_ui::type_scale::mid(font);
            let (name, value) = match r {
                Row::Pair => ("Pair a PC".to_string(), ">".to_string()),
                Row::Pc(k) => {
                    let host = page.pc(*k).unwrap_or_default().to_string();
                    let value = if page.default_pc() == Some(host.as_str()) { "PC · used on Home" } else { "PC" };
                    (host, value.to_string())
                }
                Row::Game(k) => (page.game(*k).unwrap_or_default().to_string(), "Game".to_string()),
            };
            let value_w = self.text.width(Face::Body, font, &value) + 40;
            let name = self.text.fit(Face::Body, font, &name, right - pad - 2 * NU_BTN_PAD - value_w);
            let selected = i == page.sel;
            if selected {
                let w = self.text.width(Face::Body, font, &name) + 2 * NU_BTN_PAD;
                c.round_rect(pad, y + 4, right - pad, row - 8, (row - 8) / 2, pal().band);
                c.round_rect(pad, y + 4, w, row - 8, (row - 8) / 2, pal().select);
            }
            self.text.draw(c, Face::Body, font, pad + NU_BTN_PAD, base, if selected { pal().ink } else { pal().text }, &name);
            self.text.draw_right(c, Face::Body, font, right - NU_BTN_PAD, base, if selected { pal().text } else { pal().dust }, &value);
        }
        self.hits.list(page.sel, rows.len(), 0.0, row as f32);

        // Below the list: the question, the work under way, the last result, or what the row does.
        let left = pad + NU_BTN_PAD;
        let width = c.w - pad - NU_BTN_PAD - left;
        let y0 = list_bottom + 14;
        c.rect(left, y0, width, 2, pal().spine);
        let (head, lines, color): (String, Vec<String>, u32) = if let Some(h) = &page.confirm_unpair {
            (format!("Unpair {h}?"), vec!["Press X again: the PC forgets this Pocket. To stream from it after that, pair it again. B keeps it.".into()], pal().alert)
        } else if let Some(b) = &page.busy {
            (format!("{b}{dots}"), vec!["This takes a few seconds.".into()], pal().chalk)
        } else if let Some((m, err)) = &page.message {
            (if *err { "That did not work".into() } else { "Done".into() }, vec![m.clone()], if *err { pal().alert } else { pal().chalk })
        } else if page.loaded.is_none() {
            (format!("Looking{dots}"), Vec::new(), pal().dust)
        } else if !page.installed() {
            ("Moonlight isn't on this system".into(), vec!["ROCKNIX's image includes Moonlight; this one doesn't, so there is nothing to pair.".into()], pal().dust)
        } else {
            match page.row() {
                Row::Pair => (
                    "Pair a PC".into(),
                    vec!["Once per PC, with a PIN shown here. The PC needs Sunshine (any PC) or GeForce Experience, on the same network.".into()],
                    pal().chalk,
                ),
                Row::Pc(k) => (
                    page.pc(k).unwrap_or_default().to_string(),
                    vec!["A: PC streaming on Home uses this PC, and its games are listed here. X: unpair.".into()],
                    pal().chalk,
                ),
                Row::Game(k) => (
                    page.game(k).unwrap_or_default().to_string(),
                    vec![format!("A streams it from {}. To stop: hold START + SELECT + L1 + R1.", page.default_pc().unwrap_or("the PC"))],
                    pal().chalk,
                ),
            }
        };
        let head = self.text.fit(Face::Title, 44, &head, width);
        self.text.draw(c, Face::Title, 44, left, y0 + 58, color, &head);
        let mut y = y0 + 108;
        for text in lines {
            for line in self.text.wrap(Face::Body, NU_SMALL, &text, width).into_iter().take(3) {
                self.text.draw(c, Face::Body, NU_SMALL, left, y, pal().text, &line);
                y += pocket_ui::type_scale::line(NU_SMALL, 12);
            }
        }
        let mut hints = vec![("B", "Back")];
        // Without Moonlight there is nothing to pair; a PC or game row can only be there with it.
        if page.installed() || page.row() != Row::Pair {
            hints.push(("A", page.a_hint()));
        }
        if matches!(page.row(), Row::Pc(_)) {
            hints.push(("X", if page.confirm_unpair.is_some() { "Unpair now" } else { "Unpair" }));
        }
        self.nu_hints(c, &hints);
    }
}
