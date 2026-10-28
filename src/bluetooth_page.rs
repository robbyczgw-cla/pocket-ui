//! Settings > Bluetooth: the state of the page and what a press on it asks for. The App turns an `Action` into work
//! on a thread (`bluetooth::spawn`) and reports back with an `Event`; painting is in the binary (`app_bluetooth.rs`).
//! Nothing here touches the system, so every path is a plain test.
//!
//! The list: the switch, "Search for devices", then the devices BlueZ knows. Controllers come first (paired
//! ones before found ones), then other paired devices, then other found ones. Found devices without a name
//! (phones, watches and beacons nearby send nameless adverts all the time) are not shown; their number is.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use crate::bluetooth::{Device, Step, SEARCH_SECS};
use crate::pad::Button;

/// What the page knows of Bluetooth from its last look.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Snapshot {
    /// The service runs (`systemctl is-active bluetooth`, as the quick settings read it); `None`: no Bluetooth.
    pub on: Option<bool>,
    pub devices: Vec<Device>,
    /// Why `devices` is empty while Bluetooth is on (bluetoothctl did not answer).
    pub error: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Row {
    /// Bluetooth on or off: the quick settings' switch.
    Switch,
    /// Looks for devices for 15 s; A again stops.
    Search,
    /// A device of `BluetoothPage::shown`.
    Device(usize),
}

/// Work under way, and what it is about.
#[derive(Clone, Debug, PartialEq)]
pub enum Busy {
    Switching(bool),
    /// Pairing, trusting and connecting one device; `Step` says which part.
    Pairing {
        address: String,
        step: Step,
    },
    Connecting(String),
    Disconnecting(String),
    Forgetting(String),
}

impl Busy {
    pub fn address(&self) -> Option<&str> {
        match self {
            Busy::Switching(_) => None,
            Busy::Pairing { address, .. } | Busy::Connecting(address) | Busy::Disconnecting(address) | Busy::Forgetting(address) => Some(address),
        }
    }
}

/// What a press asks the App to do.
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    Nothing,
    Close,
    SetEnabled(bool),
    /// Look for devices; switch Bluetooth on first when it is off.
    Search {
        switch_on: bool,
    },
    /// Stop the search under way (the page has already set `scan_stop`).
    StopSearch,
    /// Pair, trust and connect.
    Pair(String),
    Connect(String),
    Disconnect(String),
    Forget(String),
}

/// News for the page from its threads.
#[derive(Debug)]
pub enum Event {
    Poll(Snapshot),
    Switched {
        on: bool,
        result: Result<(), String>,
    },
    SearchDone(Result<(), String>),
    /// A pairing moved on to its next step.
    Progress {
        address: String,
        step: Step,
    },
    Paired {
        address: String,
        result: Result<(), String>,
    },
    Connected {
        address: String,
        connected: bool,
        result: Result<(), String>,
    },
    Forgotten {
        address: String,
        result: Result<(), String>,
    },
}

pub struct BluetoothPage {
    pub sel: usize,
    pub snap: Option<Snapshot>,
    /// The devices in the list, in their order.
    pub shown: Vec<Device>,
    /// Found devices without a name, left out of the list.
    pub hidden: usize,
    pub busy: Option<Busy>,
    /// A search runs, since then.
    pub searching: Option<Instant>,
    pub error: Option<String>,
    /// A line of good news ("Connected. It reconnects by itself after sleep.").
    pub notice: Option<String>,
    /// X was pressed on this paired device: X again forgets it.
    pub confirm_forget: Option<String>,
    /// Ends the running search when set; a new one for each search.
    pub scan_stop: Arc<AtomicBool>,
    /// Stops the poller thread when the page closes.
    pub alive: Arc<AtomicBool>,
    /// A poll that was already under way when something changed still tells the old state: not believed.
    skip_polls: u8,
}

impl Default for BluetoothPage {
    fn default() -> Self {
        Self::new()
    }
}

/// Where a device goes in the list: controllers first, paired before found, named before nameless, then by name.
fn order(d: &Device) -> (bool, bool, bool, String) {
    (!d.is_gamepad(), !d.paired, d.name.is_none(), d.label().to_lowercase())
}

impl BluetoothPage {
    pub fn new() -> BluetoothPage {
        BluetoothPage {
            sel: 0,
            snap: None,
            shown: Vec::new(),
            hidden: 0,
            busy: None,
            searching: None,
            error: None,
            notice: None,
            confirm_forget: None,
            scan_stop: Arc::new(AtomicBool::new(false)),
            alive: Arc::new(AtomicBool::new(true)),
            skip_polls: 0,
        }
    }

    /// Switched on as far as the user can tell: the service runs.
    pub fn on(&self) -> bool {
        self.snap.as_ref().and_then(|s| s.on) == Some(true)
    }

    pub fn rows(&self) -> Vec<Row> {
        let mut rows = vec![Row::Switch, Row::Search];
        rows.extend((0..self.shown.len()).map(Row::Device));
        rows
    }

    pub fn row(&self) -> Row {
        let rows = self.rows();
        rows[self.sel.min(rows.len() - 1)]
    }

    pub fn device(&self, row: Row) -> Option<&Device> {
        match row {
            Row::Device(i) => self.shown.get(i),
            _ => None,
        }
    }

    fn selected_address(&self) -> Option<String> {
        self.device(self.row()).map(|d| d.address.clone())
    }

    /// Puts the devices of the snapshot in order, keeping the selection on the same device.
    fn rebuild(&mut self) {
        let keep = self.selected_address();
        let devices = self.snap.as_ref().filter(|s| s.on == Some(true)).map(|s| s.devices.clone()).unwrap_or_default();
        let (shown, hidden): (Vec<Device>, Vec<Device>) = devices.into_iter().partition(|d| d.name.is_some() || d.paired || d.is_gamepad());
        self.shown = shown;
        self.shown.sort_by_key(order);
        self.hidden = hidden.len();
        if let Some(addr) = keep {
            if let Some(i) = self.shown.iter().position(|d| d.address == addr) {
                self.sel = 2 + i;
            }
        }
        self.sel = self.sel.min(self.rows().len() - 1);
        if self
            .confirm_forget
            .as_ref()
            .is_some_and(|a| !self.shown.iter().any(|d| d.address == *a && d.paired))
        {
            self.confirm_forget = None;
        }
    }

    /// Changes the device at `address` in the snapshot as the work that ended says, without waiting for a poll.
    fn update(&mut self, address: &str, f: impl FnOnce(&mut Device)) {
        if let Some(d) = self.snap.as_mut().and_then(|s| s.devices.iter_mut().find(|d| d.address == address)) {
            f(d);
        }
        self.skip_polls = 1;
        self.rebuild();
    }

    fn name_of(&self, address: &str) -> String {
        self.snap
            .as_ref()
            .and_then(|s| s.devices.iter().find(|d| d.address == address))
            .map_or_else(|| address.to_string(), |d| d.label().to_string())
    }

    fn stop_search(&mut self) {
        self.scan_stop.store(true, Ordering::Relaxed);
        self.searching = None;
    }

    pub fn press(&mut self, b: Button, confirm: Button, cancel: Button) -> Action {
        if self.confirm_forget.is_some() {
            if b == Button::North {
                if let Some(addr) = self.confirm_forget.take().filter(|a| self.selected_address().as_ref() == Some(a)) {
                    self.error = None;
                    self.notice = None;
                    self.busy = Some(Busy::Forgetting(addr.clone()));
                    return Action::Forget(addr);
                }
                return Action::Nothing;
            }
            // Any other button takes the question back; B only that.
            self.confirm_forget = None;
            if b == cancel {
                return Action::Nothing;
            }
        }
        let n = self.rows().len();
        if b == Button::Up {
            self.sel = (self.sel + n - 1) % n;
        } else if b == Button::Down {
            self.sel = (self.sel + 1) % n;
        } else if b == cancel {
            self.alive.store(false, Ordering::Relaxed);
            self.stop_search();
            return Action::Close;
        } else if b == confirm {
            return self.activate();
        } else if b == Button::North {
            // X on a paired device asks whether to forget it.
            if let Some(d) = self.device(self.row()).filter(|d| d.paired && self.busy.is_none()) {
                self.confirm_forget = Some(d.address.clone());
            }
        }
        Action::Nothing
    }

    fn activate(&mut self) -> Action {
        if self.busy.is_some() {
            return Action::Nothing;
        }
        self.error = None;
        self.notice = None;
        match self.row() {
            Row::Switch => {
                let on = !self.on();
                if !on {
                    self.stop_search();
                }
                self.busy = Some(Busy::Switching(on));
                Action::SetEnabled(on)
            }
            Row::Search if self.searching.is_some() => {
                self.stop_search();
                Action::StopSearch
            }
            Row::Search => {
                let switch_on = !self.on();
                if switch_on {
                    self.busy = Some(Busy::Switching(true));
                }
                self.scan_stop = Arc::new(AtomicBool::new(false));
                self.searching = Some(Instant::now());
                Action::Search { switch_on }
            }
            Row::Device(i) => {
                let Some(d) = self.shown.get(i).cloned() else { return Action::Nothing };
                if !d.paired {
                    // Discovery gets in the way of pairing: the search stops first.
                    self.stop_search();
                    self.busy = Some(Busy::Pairing {
                        address: d.address.clone(),
                        step: Step::Pairing,
                    });
                    Action::Pair(d.address)
                } else if d.connected {
                    self.busy = Some(Busy::Disconnecting(d.address.clone()));
                    Action::Disconnect(d.address)
                } else {
                    self.stop_search();
                    self.busy = Some(Busy::Connecting(d.address.clone()));
                    Action::Connect(d.address)
                }
            }
        }
    }

    pub fn label(&self, row: Row) -> String {
        match row {
            Row::Switch => "Bluetooth".into(),
            Row::Search if self.searching.is_some() => "Stop searching".into(),
            Row::Search => "Search for devices".into(),
            Row::Device(i) => self.shown.get(i).map(|d| d.label().to_string()).unwrap_or_default(),
        }
    }

    /// The value at the right of a row.
    pub fn value(&self, row: Row) -> String {
        match row {
            Row::Switch => match (&self.busy, &self.snap) {
                (Some(Busy::Switching(true)), _) => "Turning on".into(),
                (Some(Busy::Switching(false)), _) => "Turning off".into(),
                (_, None) => String::new(),
                (_, Some(s)) if s.on.is_none() => "Not available".into(),
                (_, Some(_)) if self.on() => "On".into(),
                _ => "Off".into(),
            },
            Row::Search => match self.searching {
                Some(since) => format!("{} s", SEARCH_SECS.saturating_sub(since.elapsed().as_secs()).max(1)),
                None => String::new(),
            },
            Row::Device(i) => {
                let Some(d) = self.shown.get(i) else { return String::new() };
                match &self.busy {
                    Some(Busy::Pairing { address, step }) if *address == d.address => match step {
                        Step::Looking => "Looking".into(),
                        Step::Trusting => "Trusting".into(),
                        Step::Connecting => "Connecting".into(),
                        _ => "Pairing".into(),
                    },
                    Some(Busy::Connecting(a)) if *a == d.address => "Connecting".into(),
                    Some(Busy::Disconnecting(a)) if *a == d.address => "Disconnecting".into(),
                    Some(Busy::Forgetting(a)) if *a == d.address => "Forgetting".into(),
                    _ if d.connected => "Connected".into(),
                    _ if d.paired => "Paired".into(),
                    _ => d.kind().word().into(),
                }
            }
        }
    }

    /// The line beside the hints for the selected row.
    pub fn describe(&self, row: Row) -> String {
        match row {
            Row::Switch => "The same switch as in the quick settings.".into(),
            Row::Search if self.searching.is_some() => "A stops the search.".into(),
            Row::Search => format!("Looks for devices in pairing mode for {SEARCH_SECS} seconds."),
            Row::Device(i) => {
                let Some(d) = self.shown.get(i) else { return String::new() };
                if self.confirm_forget.as_deref() == Some(d.address.as_str()) {
                    "X again forgets it. B keeps it.".into()
                } else if !d.paired {
                    "A pairs it and connects it.".into()
                } else if d.connected {
                    "A disconnects it. X forgets it.".into()
                } else {
                    "A connects it. X forgets it.".into()
                }
            }
        }
    }

    /// The hint for A on the selected row.
    pub fn a_hint(&self) -> &'static str {
        match self.row() {
            Row::Switch => "Switch",
            Row::Search if self.searching.is_some() => "Stop",
            Row::Search => "Search",
            Row::Device(i) => match self.shown.get(i) {
                Some(d) if !d.paired => "Pair",
                Some(d) if d.connected => "Disconnect",
                _ => "Connect",
            },
        }
    }

    /// X forgets the selected device: it is paired and nothing else runs.
    pub fn can_forget(&self) -> bool {
        self.busy.is_none() && self.device(self.row()).is_some_and(|d| d.paired)
    }

    pub fn event(&mut self, e: Event) {
        match e {
            Event::Poll(_) if self.skip_polls > 0 => self.skip_polls -= 1,
            Event::Poll(snap) => {
                if snap.on == Some(false) && !matches!(self.busy, Some(Busy::Switching(true))) {
                    // Switched off elsewhere (the quick settings, a deep sleep): no search can run.
                    self.searching = None;
                }
                self.snap = Some(snap);
                self.rebuild();
            }
            Event::Switched { on, result } => {
                if matches!(self.busy, Some(Busy::Switching(_))) {
                    self.busy = None;
                }
                match result {
                    Ok(()) => {
                        let snap = self.snap.get_or_insert_with(Snapshot::default);
                        snap.on = Some(on);
                        snap.error = None;
                        if !on {
                            snap.devices.clear();
                            self.searching = None;
                        }
                        self.skip_polls = 1;
                        self.rebuild();
                    }
                    Err(msg) => {
                        self.error = Some(msg);
                        self.searching = None;
                    }
                }
            }
            Event::SearchDone(result) => {
                self.searching = None;
                if let Err(msg) = result {
                    self.error = Some(msg);
                }
            }
            Event::Progress { address, step } => {
                if let Some(Busy::Pairing { address: a, step: s }) = self.busy.as_mut() {
                    if *a == address {
                        *s = step;
                    }
                }
            }
            Event::Paired { address, result } => {
                self.busy = None;
                match result {
                    Ok(()) => {
                        self.notice = Some(format!("{} is connected. It reconnects by itself, even after sleep.", self.name_of(&address)));
                        self.update(&address, |d| (d.paired, d.trusted, d.connected) = (true, true, true));
                    }
                    Err(msg) => self.error = Some(msg),
                }
            }
            Event::Connected { address, connected, result } => {
                self.busy = None;
                match result {
                    Ok(()) => {
                        let name = self.name_of(&address);
                        self.notice = Some(if connected {
                            format!("{name} is connected.")
                        } else {
                            format!("{name} is disconnected.")
                        });
                        self.update(&address, |d| d.connected = connected);
                    }
                    Err(msg) => self.error = Some(msg),
                }
            }
            Event::Forgotten { address, result } => {
                self.busy = None;
                match result {
                    Ok(()) => {
                        self.notice = Some(format!("Forgot {}. Pair it again to use it.", self.name_of(&address)));
                        if let Some(s) = self.snap.as_mut() {
                            s.devices.retain(|d| d.address != address);
                        }
                        self.skip_polls = 1;
                        self.rebuild();
                    }
                    Err(msg) => self.error = Some(msg),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bluetooth::demo_devices;

    const CONFIRM: Button = Button::East;
    const CANCEL: Button = Button::South;

    /// The demo room, every device seen.
    fn room() -> Vec<Device> {
        demo_devices().into_iter().map(|d| d.device).collect()
    }

    fn snap(on: bool, devices: Vec<Device>) -> Snapshot {
        Snapshot {
            on: Some(on),
            devices,
            error: None,
        }
    }

    fn page_with(s: Snapshot) -> BluetoothPage {
        let mut p = BluetoothPage::new();
        p.event(Event::Poll(s));
        p
    }

    fn press(p: &mut BluetoothPage, b: Button) -> Action {
        p.press(b, CONFIRM, CANCEL)
    }

    fn go_to(p: &mut BluetoothPage, name: &str) {
        for _ in 0..p.rows().len() {
            if p.device(p.row()).is_some_and(|d| d.label() == name) {
                return;
            }
            press(p, Button::Down);
        }
        panic!("{name} is not in the list");
    }

    fn names(p: &BluetoothPage) -> Vec<&str> {
        p.shown.iter().map(|d| d.label()).collect()
    }

    #[test]
    fn controllers_come_first_paired_before_found_and_nameless_noise_is_left_out() {
        let p = page_with(snap(true, room()));
        assert_eq!(
            names(&p),
            [
                "Xbox Wireless Controller",
                "8BitDo Ultimate 2C Wireless",
                "DualSense Wireless Controller",
                "Pro Controller",
                "JBL Flip 5"
            ]
        );
        assert_eq!(p.hidden, 3, "three nameless devices nearby");
        assert_eq!(p.value(Row::Device(0)), "Paired");
        assert_eq!(p.value(Row::Device(1)), "Controller");
        assert_eq!(p.value(Row::Device(4)), "Audio");
        // A nameless controller is still shown, by its address.
        let mut nameless = room()[1].clone();
        nameless.name = None;
        let q = page_with(snap(true, vec![nameless]));
        assert_eq!(names(&q), ["E4:17:D8:0B:1D:0E"]);
    }

    #[test]
    fn off_shows_no_devices_and_the_switch_asks_for_on_then_off() {
        let mut p = page_with(snap(false, room()));
        assert!(p.shown.is_empty());
        assert_eq!(p.rows(), [Row::Switch, Row::Search]);
        assert_eq!(p.value(Row::Switch), "Off");
        assert_eq!(press(&mut p, CONFIRM), Action::SetEnabled(true));
        assert_eq!(p.value(Row::Switch), "Turning on");
        assert_eq!(press(&mut p, CONFIRM), Action::Nothing, "busy: a second press does nothing");
        p.event(Event::Switched { on: true, result: Ok(()) });
        assert_eq!(p.value(Row::Switch), "On");
        // The poll that was under way still says off: not believed.
        p.event(Event::Poll(snap(false, Vec::new())));
        assert!(p.on());
        p.event(Event::Poll(snap(true, room())));
        assert_eq!(p.shown.len(), 5);
        assert_eq!(press(&mut p, CONFIRM), Action::SetEnabled(false));
        p.event(Event::Switched { on: false, result: Ok(()) });
        assert!(p.shown.is_empty() && !p.on());
    }

    #[test]
    fn a_failed_switch_says_why() {
        let mut p = page_with(snap(false, Vec::new()));
        press(&mut p, CONFIRM);
        p.event(Event::Switched {
            on: true,
            result: Err("Bluetooth did not start.".into()),
        });
        assert_eq!(p.error.as_deref(), Some("Bluetooth did not start."));
        assert_eq!(p.busy, None);
        assert_eq!(p.value(Row::Switch), "Off");
        press(&mut p, CONFIRM);
        assert_eq!(p.error, None, "a new try clears the old message");
    }

    #[test]
    fn search_runs_for_a_while_and_a_second_press_stops_it() {
        let mut p = page_with(snap(true, Vec::new()));
        press(&mut p, Button::Down);
        assert_eq!(p.row(), Row::Search);
        assert_eq!(press(&mut p, CONFIRM), Action::Search { switch_on: false });
        assert!(p.searching.is_some());
        assert_eq!(p.label(Row::Search), "Stop searching");
        assert_eq!(p.value(Row::Search), "15 s");
        let flag = p.scan_stop.clone();
        assert_eq!(press(&mut p, CONFIRM), Action::StopSearch);
        assert!(flag.load(Ordering::Relaxed), "the running search is told to stop");
        assert!(p.searching.is_none());
        // A new search gets a new flag, and ends by itself.
        press(&mut p, CONFIRM);
        assert!(!p.scan_stop.load(Ordering::Relaxed));
        p.event(Event::SearchDone(Ok(())));
        assert!(p.searching.is_none());
        assert_eq!(p.label(Row::Search), "Search for devices");
    }

    #[test]
    fn searching_while_off_switches_on_first() {
        let mut p = page_with(snap(false, Vec::new()));
        press(&mut p, Button::Down);
        assert_eq!(press(&mut p, CONFIRM), Action::Search { switch_on: true });
        assert_eq!(p.busy, Some(Busy::Switching(true)));
        // The poll during the start still says off: the search goes on.
        p.event(Event::Poll(snap(false, Vec::new())));
        assert!(p.searching.is_some());
        p.event(Event::Switched { on: true, result: Ok(()) });
        assert!(p.on() && p.busy.is_none() && p.searching.is_some());
        // Switching on failed: no search either.
        let mut q = page_with(snap(false, Vec::new()));
        press(&mut q, Button::Down);
        press(&mut q, CONFIRM);
        q.event(Event::Switched {
            on: true,
            result: Err("no".into()),
        });
        assert!(q.searching.is_none() && q.error.is_some());
    }

    #[test]
    fn a_on_a_found_controller_stops_the_search_and_pairs_it_with_progress() {
        let mut p = page_with(snap(true, room()));
        press(&mut p, Button::Down);
        press(&mut p, CONFIRM);
        let scan = p.scan_stop.clone();
        go_to(&mut p, "8BitDo Ultimate 2C Wireless");
        assert_eq!(p.a_hint(), "Pair");
        assert_eq!(press(&mut p, CONFIRM), Action::Pair("E4:17:D8:0B:1D:0E".into()));
        assert!(scan.load(Ordering::Relaxed) && p.searching.is_none(), "discovery gets in the way of pairing");
        assert_eq!(p.value(p.row()), "Pairing");
        assert_eq!(press(&mut p, CONFIRM), Action::Nothing, "one thing at a time");
        for (step, word) in [(Step::Trusting, "Trusting"), (Step::Connecting, "Connecting")] {
            p.event(Event::Progress {
                address: "E4:17:D8:0B:1D:0E".into(),
                step,
            });
            assert_eq!(p.value(p.row()), word);
        }
        p.event(Event::Paired {
            address: "E4:17:D8:0B:1D:0E".into(),
            result: Ok(()),
        });
        assert_eq!(p.busy, None);
        assert_eq!(
            p.device(p.row()).unwrap().label(),
            "8BitDo Ultimate 2C Wireless",
            "the selection follows the device"
        );
        assert_eq!(p.value(p.row()), "Connected");
        assert!(p.notice.as_deref().unwrap().contains("reconnects by itself"));
        // The poll that was under way still says unpaired: not believed.
        p.event(Event::Poll(snap(true, room())));
        assert_eq!(p.value(p.row()), "Connected");
    }

    #[test]
    fn a_failed_pairing_shows_the_plain_reason() {
        let mut p = page_with(snap(true, room()));
        go_to(&mut p, "Pro Controller");
        press(&mut p, CONFIRM);
        p.event(Event::Paired {
            address: "98:B6:E9:12:34:56".into(),
            result: Err("The device declined the pairing. Put it in pairing mode first, then try again.".into()),
        });
        assert!(p.error.as_deref().unwrap().contains("pairing mode first"));
        assert_eq!(p.value(p.row()), "Controller", "still just found");
        assert_eq!(press(&mut p, CONFIRM), Action::Pair("98:B6:E9:12:34:56".into()), "and can be tried again");
        assert_eq!(p.error, None);
    }

    #[test]
    fn a_on_a_paired_device_connects_or_disconnects() {
        let mut p = page_with(snap(true, room()));
        go_to(&mut p, "Xbox Wireless Controller");
        assert_eq!(p.a_hint(), "Connect");
        assert_eq!(press(&mut p, CONFIRM), Action::Connect("4C:B9:9B:0A:1B:2C".into()));
        assert_eq!(p.value(p.row()), "Connecting");
        p.event(Event::Connected {
            address: "4C:B9:9B:0A:1B:2C".into(),
            connected: true,
            result: Ok(()),
        });
        assert_eq!(p.value(p.row()), "Connected");
        assert_eq!(p.a_hint(), "Disconnect");
        assert_eq!(press(&mut p, CONFIRM), Action::Disconnect("4C:B9:9B:0A:1B:2C".into()));
        p.event(Event::Connected {
            address: "4C:B9:9B:0A:1B:2C".into(),
            connected: false,
            result: Err("It did not disconnect. Switch the device off instead.".into()),
        });
        assert_eq!(p.value(p.row()), "Connected", "unchanged when it failed");
        assert!(p.error.is_some());
    }

    #[test]
    fn x_asks_and_x_again_forgets_a_paired_device() {
        let mut p = page_with(snap(true, room()));
        go_to(&mut p, "Xbox Wireless Controller");
        assert!(p.can_forget());
        assert_eq!(press(&mut p, Button::North), Action::Nothing, "the first X only asks");
        assert!(p.describe(p.row()).contains("X again"));
        assert_eq!(press(&mut p, CANCEL), Action::Nothing, "B takes the question back, it does not close the page");
        assert!(p.confirm_forget.is_none() && p.alive.load(Ordering::Relaxed));
        press(&mut p, Button::North);
        press(&mut p, Button::Down);
        assert!(p.confirm_forget.is_none(), "moving away takes it back too");
        press(&mut p, Button::Up);
        press(&mut p, Button::North);
        assert_eq!(press(&mut p, Button::North), Action::Forget("4C:B9:9B:0A:1B:2C".into()));
        assert_eq!(p.value(p.row()), "Forgetting");
        p.event(Event::Forgotten {
            address: "4C:B9:9B:0A:1B:2C".into(),
            result: Ok(()),
        });
        assert!(!names(&p).contains(&"Xbox Wireless Controller"));
        assert!(p.notice.as_deref().unwrap().starts_with("Forgot Xbox"));
        // Found devices cannot be forgotten.
        go_to(&mut p, "JBL Flip 5");
        assert!(!p.can_forget());
        assert_eq!(press(&mut p, Button::North), Action::Nothing);
        assert!(p.confirm_forget.is_none());
    }

    #[test]
    fn b_closes_the_page_and_stops_the_poller_and_the_search() {
        let mut p = page_with(snap(true, Vec::new()));
        press(&mut p, Button::Down);
        press(&mut p, CONFIRM);
        let scan = p.scan_stop.clone();
        assert_eq!(press(&mut p, CANCEL), Action::Close);
        assert!(!p.alive.load(Ordering::Relaxed));
        assert!(scan.load(Ordering::Relaxed));
    }

    #[test]
    fn switched_off_elsewhere_ends_the_search_and_empties_the_list() {
        let mut p = page_with(snap(true, room()));
        press(&mut p, Button::Down);
        press(&mut p, CONFIRM);
        go_to(&mut p, "JBL Flip 5");
        p.event(Event::Poll(snap(false, Vec::new())));
        assert!(p.searching.is_none());
        assert!(p.shown.is_empty());
        assert_eq!(p.sel, 1, "the selection stays inside the shorter list");
    }

    #[test]
    fn the_selection_stays_on_its_device_when_the_list_changes() {
        let mut p = page_with(snap(true, room()));
        go_to(&mut p, "JBL Flip 5");
        let mut more = room();
        more.push(Device {
            address: "11:22:33:44:55:66".into(),
            name: Some("Another Gamepad".into()),
            icon: Some("input-gaming".into()),
            ..Device::default()
        });
        p.event(Event::Poll(snap(true, more)));
        assert_eq!(p.device(p.row()).unwrap().label(), "JBL Flip 5");
    }

    #[test]
    fn up_and_down_wrap_and_every_row_has_words() {
        let mut p = page_with(snap(true, room()));
        press(&mut p, Button::Up);
        assert_eq!(p.row(), Row::Device(4));
        press(&mut p, Button::Down);
        assert_eq!(p.row(), Row::Switch);
        for r in p.rows() {
            assert!(!p.label(r).is_empty());
            assert!(!p.describe(r).is_empty());
        }
        assert_eq!(BluetoothPage::new().value(Row::Switch), "", "nothing known yet");
        assert_eq!(page_with(Snapshot::default()).value(Row::Switch), "Not available");
    }
}
