//! Part of the pocket-ui binary: the App's side of `pocket_ui::bluetooth` and `pocket_ui::bluetooth_page`.
//! Settings > Bluetooth: the switch the quick settings have too, a search, and pairing controllers in one press.
//! Every call into the system runs on a thread (`bluetooth::spawn`): the UI thread only hands work over and paints.
//! A controller that connects is picked up by the pad supervisor (`pad::linux::supervise`) within a second.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use pocket_ui::bluetooth::{self, Bluetooth, Bluetoothctl, MockBluetooth, Pace, Step, SEARCH_SECS};
use pocket_ui::bluetooth_page::{Action, BluetoothPage, Busy, Event, Snapshot};
use pocket_ui::gfx::Canvas;
use pocket_ui::kept::Kept;
use pocket_ui::pad::Button;
use pocket_ui::palette::*;
use pocket_ui::system_ctl::SharedHw;
use pocket_ui::text::Face;
use pocket_ui::touch::Area;

use super::{App, SettingsPage, UserEvent, CANCEL, CONFIRM, NU_BTN_PAD, NU_PAD, NU_PILL, NU_SMALL};

/// How often the open page looks at the devices.
const POLL_EVERY: Duration = Duration::from_secs(2);
/// How long a search waits for the adapter after Bluetooth was switched on.
const READY_WITHIN: Duration = Duration::from_secs(10);
/// Where ROCKNIX keeps `controllers.bluetooth.enabled`, which `rocknix-bluetooth enable|disable` sets.
const SYSTEM_CFG: &str = "/storage/.config/system/configs/system.cfg";
/// The Settings list has the switch read from system.cfg (on a thread, `kept`) at most this often.
const LIST_VALUE_FOR: Duration = Duration::from_secs(2);
/// Below the list: status and what to do.
const DETAILS_H: i32 = 270;

/// What the App keeps of Bluetooth: the backend, the open page, and the switch as the Settings list shows it.
pub(crate) struct BluetoothUi {
    hw: SharedHw,
    bt: Arc<dyn Bluetooth>,
    /// `bt` is `bluetoothctl`; otherwise the mock (desktop, virtual device, `--snapshot`).
    real: bool,
    page: Option<BluetoothPage>,
    /// On or off for the Settings list: read on a thread (`kept`), set by the page when it saw the switch.
    seen: Arc<Kept<bool>>,
}

impl BluetoothUi {
    /// `service`: the system has ROCKNIX's Bluetooth service (`system_ctl::Caps::bluetooth`).
    pub fn load(hw: SharedHw, service: bool) -> BluetoothUi {
        let ctl = if service { Bluetoothctl::system() } else { None };
        let real = ctl.is_some();
        let bt: Arc<dyn Bluetooth> = match ctl {
            Some(c) => Arc::new(c),
            // The virtual device and the desktop: an imagined room, at a believable pace.
            None => Arc::new(MockBluetooth::demo(Duration::from_millis(1200), Duration::from_secs(SEARCH_SECS))),
        };
        println!("bluetooth: {}", if real { "bluetoothctl" } else { "mock devices" });
        BluetoothUi {
            hw,
            bt,
            real,
            page: None,
            seen: Kept::new("bluetooth-row"),
        }
    }
}

/// `controllers.bluetooth.enabled=1` in ROCKNIX's system.cfg.
fn enabled_in(cfg: &str) -> bool {
    cfg.lines().any(|l| l.trim() == "controllers.bluetooth.enabled=1")
}

/// What the page sees: the switch as the quick settings read it, and BlueZ's devices while it is on.
fn look(hw: &SharedHw, bt: &dyn Bluetooth) -> Snapshot {
    let on = hw.bluetooth();
    if on != Some(true) {
        return Snapshot { on, ..Snapshot::default() };
    }
    match bt.devices() {
        Ok(devices) => Snapshot { on, devices, error: None },
        Err(e) => Snapshot {
            on,
            devices: Vec::new(),
            error: Some(e),
        },
    }
}

/// Switches with `rocknix-bluetooth`, like the quick settings, and says whether it took.
fn switch(hw: &SharedHw, on: bool) -> Result<(), String> {
    hw.set_bluetooth(on);
    match hw.bluetooth() {
        Some(now) if now == on => Ok(()),
        _ if on => Err("Bluetooth did not start. Try again in a moment.".into()),
        _ => Err("Bluetooth did not stop. Try again in a moment.".into()),
    }
}

/// Waits until the adapter is up (after switching on it takes a moment), then searches.
fn search(bt: &dyn Bluetooth, stop: &AtomicBool) -> Result<(), String> {
    let until = Instant::now() + READY_WITHIN;
    loop {
        match bt.ready() {
            Ok(()) => break,
            Err(_) if stop.load(Ordering::Relaxed) => return Ok(()),
            Err(_) if Instant::now() < until => std::thread::sleep(Duration::from_millis(500)),
            Err(e) => {
                eprintln!("bluetooth: not ready for a search: {e}");
                return Err("Bluetooth is not ready. Switch it off and on, then try again.".into());
            }
        }
    }
    bt.scan(SEARCH_SECS, stop).map_err(|e| {
        eprintln!("bluetooth: search: {e}");
        "The search did not start. Switch Bluetooth off and on, then try again.".to_string()
    })
}

impl App {
    /// Text for the Settings list: "On >" / "Off >", from memory; ">" alone until the first read is in. The switch is
    /// read from system.cfg (the mock: from memory) on a thread, never here (review 2026-10-24).
    pub(crate) fn bluetooth_list_value(&mut self) -> String {
        let (real, hw, proxy) = (self.bluetooth.real, self.bluetooth.hw.clone(), self.proxy.clone());
        self.bluetooth.seen.refresh(
            LIST_VALUE_FOR,
            move || if real { std::fs::read_to_string(SYSTEM_CFG).is_ok_and(|c| enabled_in(&c)) } else { hw.bluetooth() == Some(true) },
            move || {
                let _ = proxy.send_event(UserEvent::SettingsValues);
            },
        );
        match self.bluetooth.seen.get() {
            Some(on) => format!("{} >", if on { "On" } else { "Off" }),
            None => ">".into(),
        }
    }

    /// `--snapshot settings…`: the row's value read at once (never so in the running app).
    pub(crate) fn bluetooth_row_now(&self) {
        let ui = &self.bluetooth;
        ui.seen.set(if ui.real { std::fs::read_to_string(SYSTEM_CFG).is_ok_and(|c| enabled_in(&c)) } else { ui.hw.bluetooth() == Some(true) });
    }

    pub(crate) fn open_bluetooth(&mut self) -> Option<SettingsPage> {
        let page = BluetoothPage::new();
        let (alive, proxy, hw, bt) = (page.alive.clone(), self.proxy.clone(), self.bluetooth.hw.clone(), self.bluetooth.bt.clone());
        // Every call is bounded by a timeout, and the UI never waits for one.
        std::thread::spawn(move || {
            while alive.load(Ordering::Relaxed) {
                let snap = look(&hw, &*bt);
                let _ = proxy.send_event(UserEvent::Bluetooth(Event::Poll(snap)));
                let until = Instant::now() + POLL_EVERY;
                while alive.load(Ordering::Relaxed) && Instant::now() < until {
                    std::thread::sleep(Duration::from_millis(100));
                }
            }
        });
        self.bluetooth.page = Some(page);
        Some(SettingsPage::Bluetooth)
    }

    fn send_bluetooth(proxy: &winit::event_loop::EventLoopProxy<UserEvent>, e: Event) {
        let _ = proxy.send_event(UserEvent::Bluetooth(e));
    }

    pub(crate) fn bluetooth_input(&mut self, b: Button) -> Option<SettingsPage> {
        let (proxy, hw, bt) = (self.proxy.clone(), self.bluetooth.hw.clone(), self.bluetooth.bt.clone());
        let Some(page) = self.bluetooth.page.as_mut() else {
            return Some(SettingsPage::List(super::settings_row("Bluetooth")));
        };
        let was_searching = page.searching.is_some();
        let action = page.press(b, CONFIRM, CANCEL);
        let connect = matches!(action, Action::Connect(_));
        match action {
            Action::Nothing | Action::StopSearch => {}
            Action::Close => {
                self.bluetooth.page = None;
                // Read again for the list (the page may have switched it); shown as last seen until then.
                self.bluetooth.seen.stale();
                return Some(SettingsPage::List(super::settings_row("Bluetooth")));
            }
            Action::SetEnabled(on) => {
                bluetooth::spawn(
                    "bt-switch",
                    move || switch(&hw, on),
                    move |result| Self::send_bluetooth(&proxy, Event::Switched { on, result }),
                );
            }
            Action::Search { switch_on } => {
                let stop = page.scan_stop.clone();
                let tx = proxy.clone();
                bluetooth::spawn(
                    "bt-search",
                    move || {
                        if switch_on {
                            let result = switch(&hw, true);
                            Self::send_bluetooth(
                                &tx,
                                Event::Switched {
                                    on: true,
                                    result: result.clone(),
                                },
                            );
                            result?;
                        }
                        search(&*bt, &stop)
                    },
                    move |result| Self::send_bluetooth(&proxy, Event::SearchDone(result)),
                );
            }
            Action::Pair(address) => {
                let tx = proxy.clone();
                bluetooth::spawn(
                    "bt-pair",
                    move || {
                        // The search was told to stop; give its bluetoothctl a moment to end the discovery.
                        if was_searching {
                            std::thread::sleep(Duration::from_millis(500));
                        }
                        let progress = address.clone();
                        let result = bluetooth::pair_trust_connect(&*bt, &address, Pace::default(), &mut |step| {
                            Self::send_bluetooth(
                                &tx,
                                Event::Progress {
                                    address: progress.clone(),
                                    step,
                                },
                            );
                        });
                        (address, result)
                    },
                    move |(address, result)| Self::send_bluetooth(&proxy, Event::Paired { address, result }),
                );
            }
            Action::Connect(address) | Action::Disconnect(address) => {
                bluetooth::spawn(
                    "bt-connect",
                    move || {
                        let result = if connect {
                            if was_searching {
                                std::thread::sleep(Duration::from_millis(500));
                            }
                            match bt.connect(&address) {
                                Err(e) if !e.contains("AlreadyConnected") && !bt.info(&address).is_some_and(|d| d.connected) => {
                                    Err(bluetooth::explain(Step::Connecting, &e))
                                }
                                _ => Ok(()),
                            }
                        } else {
                            bt.disconnect(&address).map_err(|e| bluetooth::explain(Step::Disconnecting, &e))
                        };
                        (address, result)
                    },
                    move |(address, result)| {
                        Self::send_bluetooth(
                            &proxy,
                            Event::Connected {
                                address,
                                connected: connect,
                                result,
                            },
                        )
                    },
                );
            }
            Action::Forget(address) => {
                bluetooth::spawn(
                    "bt-forget",
                    move || {
                        let result = bt.remove(&address).map_err(|e| bluetooth::explain(Step::Forgetting, &e));
                        (address, result)
                    },
                    move |(address, result)| Self::send_bluetooth(&proxy, Event::Forgotten { address, result }),
                );
            }
        }
        Some(SettingsPage::Bluetooth)
    }

    pub(crate) fn bluetooth_event(&mut self, e: Event) {
        let ui = self.bluetooth.get_mut();
        if let Some(page) = ui.page.as_mut() {
            page.event(e);
            if page.snap.is_some() {
                ui.seen.set(page.on());
            }
        }
    }

    /// Where the page's selection and switch stand: a change is what the menu sounds answer.
    pub(crate) fn bluetooth_row(&self) -> Option<(usize, bool, bool)> {
        self.bluetooth.page.as_ref().map(|p| (p.sel, p.on(), p.confirm_forget.is_some()))
    }

    /// Something moves on screen: work under way, or the search counting down.
    pub(crate) fn bluetooth_animating(&self) -> bool {
        // Asked after every frame: never waits for the state read after the first frame (`startup::Later`).
        self.bluetooth.loaded().and_then(|b| b.page.as_ref()).is_some_and(|p| p.busy.is_some() || p.searching.is_some())
    }

    /// `--snapshot bluetooth[:off|:empty|:search|:found|:pairing|:connected|:error|:forget]` draws the page with the
    /// mock's devices.
    pub(crate) fn bluetooth_snapshot(&mut self, state: &str) -> bool {
        let Some(rest) = state.strip_prefix("bluetooth") else { return false };
        let mode = rest.trim_start_matches(':');
        let room: Vec<_> = bluetooth::demo_devices().into_iter().map(|d| d.device).collect();
        let (on, devices) = match mode {
            "off" => (false, Vec::new()),
            "empty" => (true, room.iter().filter(|d| d.paired).cloned().collect()),
            _ => (true, room),
        };
        let mut page = BluetoothPage::new();
        page.event(Event::Poll(Snapshot {
            on: Some(on),
            devices,
            error: None,
        }));
        let at = |page: &mut BluetoothPage, name: &str| {
            page.sel = 2 + page.shown.iter().position(|d| d.label() == name).unwrap_or(0);
        };
        match mode {
            "search" => {
                page.sel = 1;
                page.press(Button::East, Button::East, Button::South);
                page.searching = Instant::now().checked_sub(Duration::from_secs(4));
            }
            "pairing" => {
                at(&mut page, "8BitDo Ultimate 2C Wireless");
                page.press(Button::East, Button::East, Button::South);
                page.event(Event::Progress {
                    address: "E4:17:D8:0B:1D:0E".into(),
                    step: Step::Trusting,
                });
            }
            "connected" => {
                at(&mut page, "8BitDo Ultimate 2C Wireless");
                page.press(Button::East, Button::East, Button::South);
                page.event(Event::Paired {
                    address: "E4:17:D8:0B:1D:0E".into(),
                    result: Ok(()),
                });
            }
            "error" => {
                at(&mut page, "Pro Controller");
                page.press(Button::East, Button::East, Button::South);
                page.event(Event::Paired {
                    address: "98:B6:E9:12:34:56".into(),
                    result: Err(bluetooth::explain(Step::Pairing, "Failed to pair: org.bluez.Error.AuthenticationFailed")),
                });
            }
            "forget" => {
                at(&mut page, "Xbox Wireless Controller");
                page.press(Button::North, Button::East, Button::South);
            }
            "found" => at(&mut page, "8BitDo Ultimate 2C Wireless"),
            _ => {}
        }
        self.bluetooth.page = Some(page);
        self.settings_page = Some(SettingsPage::Bluetooth);
        true
    }

    pub(crate) fn paint_bluetooth(&mut self, c: &mut Canvas) {
        let Some(page) = self.bluetooth.page.take() else { return };
        c.clear(pal().void);
        self.paint_bluetooth_list(c, &page);
        self.bluetooth.page = Some(page);
    }

    fn paint_bluetooth_list(&mut self, c: &mut Canvas, page: &BluetoothPage) {
        let (pad, row, font) = (NU_PAD, pocket_ui::type_scale::row(58, 34), 34u32);
        self.sheet_header(c, "Bluetooth");
        let (top, right) = (pad + NU_PILL + 12, c.w - pad);
        let rows = page.rows();
        let list_bottom = c.h - pad - NU_PILL - 16 - DETAILS_H;
        let visible = (((list_bottom - top) / row) as usize).max(3);
        let first = super::window_start(page.sel, visible, rows.len());
        if rows.len() > visible {
            let track = visible as i32 * row;
            pocket_ui::kit::scrollbar(c, top, track, first, visible, rows.len());
        }
        // Device names start after the place of the controller mark, so they line up whatever kind they are.
        let mark_w = 56;
        for (i, r) in rows.iter().enumerate().skip(first).take(visible) {
            let y = top + (i - first) as i32 * row;
            self.hits.row(Area::new(pad, y, right - pad, row), i);
            let base = y + row / 2 + pocket_ui::type_scale::mid(font);
            let device = page.device(*r);
            let indent = if device.is_some() { mark_w } else { 0 };
            let value = page.value(*r);
            let value_w = if value.is_empty() {
                0
            } else {
                self.text.width(Face::Body, font, &value) + 40
            };
            let name = page.label(*r);
            let name = self.text.fit(Face::Body, font, &name, right - pad - 2 * NU_BTN_PAD - indent - value_w);
            let selected = i == page.sel;
            let color = if selected { pal().ink } else { pal().text };
            if selected {
                let w = self.text.width(Face::Body, font, &name) + 2 * NU_BTN_PAD + indent;
                c.round_rect(pad, y + 4, right - pad, row - 8, (row - 8) / 2, pal().band);
                c.round_rect(pad, y + 4, w, row - 8, (row - 8) / 2, pal().select);
            }
            if device.is_some_and(|d| d.is_gamepad()) {
                gamepad_mark(c, pad + NU_BTN_PAD, y + row / 2, color, if selected { pal().select } else { pal().void });
            }
            self.text.draw(c, Face::Body, font, pad + NU_BTN_PAD + indent, base, color, &name);
            let quiet = device.is_some_and(|d| !d.paired) && !matches!(&page.busy, Some(b) if b.address() == device.map(|d| d.address.as_str()));
            // Found devices say their kind quietly; on the selected row's band everything is in full colour.
            self.text.draw_right(
                c,
                Face::Body,
                font,
                right - NU_BTN_PAD,
                base,
                if quiet && !selected { pal().dust } else { pal().text },
                &value,
            );
        }
        self.hits.list(page.sel, rows.len(), 0.0, row as f32);

        self.paint_bluetooth_details(c, page, list_bottom + 14);

        let mut hints = vec![("B", "Back"), ("A", page.a_hint())];
        if page.confirm_forget.is_some() {
            hints.push(("X", "Forget now"));
        } else if page.can_forget() {
            hints.push(("X", "Forget"));
        }
        let hints_at = self.nu_hints(c, &hints);
        let x = pad + NU_BTN_PAD;
        let desc = self.text.fit(Face::Body, NU_SMALL, &page.describe(page.row()), hints_at - 24 - x);
        let color = if page.confirm_forget.is_some() { pal().alert } else { pal().text };
        self.text
            .draw(c, Face::Body, NU_SMALL, x, c.h - pad - NU_PILL / 2 + pocket_ui::type_scale::mid(NU_SMALL), color, &desc);
    }

    /// Below the list: what is going on, or what to do.
    fn paint_bluetooth_details(&mut self, c: &mut Canvas, page: &BluetoothPage, y0: i32) {
        let left = NU_PAD + NU_BTN_PAD;
        let width = c.w - NU_PAD - NU_BTN_PAD - left;
        c.rect(left, y0, width, 2, pal().spine);
        let dots = ".".repeat(1 + (self.started.elapsed().as_millis() / 400 % 3) as usize);
        let name = |address: &str| {
            page.shown
                .iter()
                .find(|d| d.address == address)
                .map_or(address.to_string(), |d| d.label().to_string())
        };
        let mut y = y0 + 62;
        let mut headline = |app: &mut App, c: &mut Canvas, text: &str, color: u32| {
            let text = app.text.fit(Face::Title, 44, text, width);
            app.text.draw(c, Face::Title, 44, left, y, color, &text);
            y += 52;
        };
        let (lines, color): (Vec<String>, u32) = if let Some(a) = &page.confirm_forget {
            headline(self, c, &format!("Forget {}?", name(a)), pal().alert);
            (
                vec!["Press X again to forget it. To use it after that, pair it again. B keeps it.".into()],
                pal().text,
            )
        } else if let Some(e) = &page.error {
            headline(self, c, "That did not work", pal().alert);
            (vec![e.clone()], pal().text)
        } else if let Some(busy) = &page.busy {
            match busy {
                Busy::Pairing { address, step } => {
                    headline(self, c, &format!("Pairing {}{dots}", name(address)), pal().chalk);
                    self.paint_pair_steps(c, left, y, *step);
                    y += 46;
                    (
                        vec!["Keep the device close and switched on. This takes up to half a minute.".into()],
                        pal().dust,
                    )
                }
                Busy::Switching(on) => {
                    headline(
                        self,
                        c,
                        &format!("{}{dots}", if *on { "Turning Bluetooth on" } else { "Turning Bluetooth off" }),
                        pal().dust,
                    );
                    (vec!["This takes a few seconds.".into()], pal().dust)
                }
                Busy::Connecting(a) => {
                    headline(self, c, &format!("Connecting {}{dots}", name(a)), pal().chalk);
                    (
                        vec!["If it does not connect, switch the device on (controllers: press the home button).".into()],
                        pal().dust,
                    )
                }
                Busy::Disconnecting(a) => {
                    headline(self, c, &format!("Disconnecting {}{dots}", name(a)), pal().chalk);
                    (Vec::new(), pal().dust)
                }
                Busy::Forgetting(a) => {
                    headline(self, c, &format!("Forgetting {}{dots}", name(a)), pal().chalk);
                    (Vec::new(), pal().dust)
                }
            }
        } else if let Some(since) = page.searching {
            let left_s = SEARCH_SECS.saturating_sub(since.elapsed().as_secs()).max(1);
            headline(self, c, &format!("Searching, {left_s} s left"), pal().chalk);
            (
                vec!["Put the device in pairing mode now. Controllers come first in the list; pick yours and press A.".into()],
                pal().dust,
            )
        } else if let Some(n) = &page.notice {
            (vec![n.clone()], pal().chalk)
        } else if !page.on() {
            let off = page.snap.as_ref().is_some_and(|s| s.on.is_some());
            headline(
                self,
                c,
                if page.snap.is_none() {
                    "Looking"
                } else if off {
                    "Bluetooth is off"
                } else {
                    "No Bluetooth here"
                },
                pal().dust,
            );
            (vec!["Switch it on, or just select Search: that switches it on first.".into()], pal().dust)
        } else if let Some(e) = page.snap.as_ref().and_then(|s| s.error.as_ref()) {
            headline(self, c, "Bluetooth does not answer", pal().alert);
            (vec![bluetooth::explain(Step::Looking, e)], pal().dust)
        } else if page.shown.iter().any(|d| !d.paired) {
            let hidden = match page.hidden {
                0 => String::new(),
                1 => " One device without a name is not shown.".into(),
                n => format!(" {n} devices without a name are not shown."),
            };
            (
                vec![format!(
                    "Pick the device and press A: it is paired, trusted and connected in one go.{hidden}"
                )],
                pal().dust,
            )
        } else {
            (
                vec![
                    "To pair a device, put it in pairing mode, then select Search:".into(),
                    "Xbox: hold the pair button on top until the Xbox button flashes fast.".into(),
                    "PlayStation: hold Share (Create on a DualSense) and PS until the light flashes.".into(),
                    "Switch Pro, 8BitDo and most others: hold the pair button for 3 seconds.".into(),
                ],
                pal().dust,
            )
        };
        let y_end = y0 + DETAILS_H - 12;
        // The good news reads larger, and on its own.
        let (face, px, step) = if page.notice.is_some() && page.error.is_none() && page.busy.is_none() && page.searching.is_none() {
            (Face::Title, 37, 46)
        } else {
            (Face::Body, 26, 34)
        };
        for l in &lines {
            for line in self.text.wrap(face, px, l, width) {
                if y > y_end {
                    return;
                }
                self.text.draw(c, face, px, left, y, color, &line);
                y += step;
            }
            y += 6;
        }
    }

    /// "Pair (done)  ›  Trust  ›  Connect": the step under way bright and underlined, the others quiet.
    fn paint_pair_steps(&mut self, c: &mut Canvas, x: i32, y: i32, now: Step) {
        let at = match now {
            Step::Looking | Step::Pairing => 0,
            Step::Trusting => 1,
            _ => 2,
        };
        let mut x = x;
        for (i, word) in ["Pair", "Trust", "Connect"].iter().enumerate() {
            if i > 0 {
                x += self.text.draw(c, Face::Body, 28, x, y, pal().dust, "   ›   ");
            }
            let w = self.text.draw(c, Face::Body, 28, x, y, if i == at { pal().text } else { pal().dust }, word);
            if i == at {
                c.rect(x, y + 8, w, 3, pal().text);
            }
            x += w;
            if i < at {
                x += self.text.draw(c, Face::Body, 28, x, y, pal().dust, " (done)");
            }
        }
    }
}

/// A small controller drawn in `color` at `x`, centred on `cy`; `bg` cuts out the d-pad and buttons.
fn gamepad_mark(c: &mut Canvas, x: i32, cy: i32, color: u32, bg: u32) {
    let (w, h) = (40, 22);
    c.round_rect(x, cy - h / 2, w, h, h / 2, color);
    // The grips.
    c.disc(x + 8, cy + 7, 8, color);
    c.disc(x + w - 8, cy + 7, 8, color);
    // D-pad and two buttons.
    c.rect(x + 6, cy - 2, 11, 4, bg);
    c.rect(x + 9, cy - 5, 4, 10, bg);
    c.disc(x + w - 13, cy - 1, 2, bg);
    c.disc(x + w - 8, cy - 4, 2, bg);
    c.disc(x + w - 8, cy + 2, 2, bg);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_switch_in_system_cfg() {
        assert!(enabled_in("wifi.enabled=1\ncontrollers.bluetooth.enabled=1\n"));
        assert!(!enabled_in("controllers.bluetooth.enabled=0\n"));
        assert!(!enabled_in("#controllers.bluetooth.enabled=1\n"));
        assert!(!enabled_in(""));
    }
}
