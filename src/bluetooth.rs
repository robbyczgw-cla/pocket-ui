//! Bluetooth devices on ROCKNIX, driven from Settings > Bluetooth (docs/user/bluetooth.md).
//!
//! ROCKNIX runs BlueZ (`bluetoothd`, `bluetoothctl`) with `storagedir=/storage/.cache/bluetooth`, `AutoEnable=true`
//! and `JustWorksRepairing=always`. Its `rocknix-bluetooth enable|disable` starts or stops `bluetooth.service` and
//! `bluetooth-agent.service` and stores `controllers.bluetooth.enabled`; that is the on/off switch here too, the
//! same one the quick settings use (`system_ctl::Hw::set_bluetooth`). The agent (`rocknix-bluetooth-agent`) is the
//! default BlueZ agent with the capability NoInputNoOutput: it accepts every pairing request and answers a PIN
//! request with 0000. Its own pairing only acts on an address written to `/run/bt_device` (EmulationStation's
//! "pair" screen), so it stays out of the way of what is done here.
//!
//! Every call is a non-interactive `bluetoothctl <command>`. Such a call registers no agent of its own, so a pairing
//! is answered by ROCKNIX's agent. Pairing a controller is three calls: `pair`, `trust` (so that it may connect by
//! itself later; ROCKNIX stops and starts Bluetooth on every deep sleep, `sleep.sh`) and `connect`. `bluetooth.service`
//! keeps a copy of `/storage/.cache/bluetooth` in `/storage/roms/backups/bluetooth.tar` (saved when it stops,
//! restored before it starts), so forgetting a device also refreshes that copy (`rocknix-bluetooth save`);
//! otherwise the device would come back from the copy at the next start.
//!
//! `bluetoothctl` waits for `bluetoothd` forever when the service is down, and a command given with `--timeout` only
//! ends at that timeout. So every call runs under a timeout of our own (`proc`), on a worker thread: the UI never
//! waits for one.

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::proc;

/// How long "Search" looks for devices.
pub const SEARCH_SECS: u64 = 15;

/// What a device is, as far as its icon, class or appearance tell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Gamepad,
    Keyboard,
    Mouse,
    Audio,
    Phone,
    Computer,
    Other,
}

impl Kind {
    /// A word for the list.
    pub fn word(self) -> &'static str {
        match self {
            Kind::Gamepad => "Controller",
            Kind::Keyboard => "Keyboard",
            Kind::Mouse => "Mouse",
            Kind::Audio => "Audio",
            Kind::Phone => "Phone",
            Kind::Computer => "Computer",
            Kind::Other => "Device",
        }
    }
}

/// One device BlueZ knows: paired ones, and the ones a search has seen lately.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Device {
    /// "E4:17:D8:0B:1D:0E", upper case.
    pub address: String,
    /// The name it gives itself; `None` when it has none (BlueZ then shows the address with dashes).
    pub name: Option<String>,
    /// BlueZ's icon name ("input-gaming", "audio-card", ...).
    pub icon: Option<String>,
    /// The Class of Device of a classic Bluetooth device.
    pub class: Option<u32>,
    /// The GAP appearance of a Bluetooth LE device.
    pub appearance: Option<u16>,
    pub paired: bool,
    pub trusted: bool,
    pub connected: bool,
    pub rssi: Option<i16>,
}

impl Device {
    /// A controller: BlueZ's icon "input-gaming", a class of peripheral/joystick or gamepad, or the LE appearance
    /// of a joystick or gamepad.
    pub fn is_gamepad(&self) -> bool {
        self.icon.as_deref() == Some("input-gaming")
            || self.class.is_some_and(|c| (c >> 8) & 0x1f == 0x05 && matches!((c >> 2) & 0x0f, 0x01 | 0x02))
            || self.appearance.is_some_and(|a| a == 0x03c3 || a == 0x03c4)
    }

    pub fn kind(&self) -> Kind {
        if self.is_gamepad() {
            return Kind::Gamepad;
        }
        let icon = self.icon.as_deref().unwrap_or_default();
        let major = self.class.map(|c| (c >> 8) & 0x1f);
        match icon {
            "input-keyboard" => Kind::Keyboard,
            "input-mouse" | "input-tablet" => Kind::Mouse,
            "phone" => Kind::Phone,
            "computer" => Kind::Computer,
            i if i.starts_with("audio-") => Kind::Audio,
            _ => match major {
                Some(0x01) => Kind::Computer,
                Some(0x02) => Kind::Phone,
                Some(0x04) => Kind::Audio,
                _ => Kind::Other,
            },
        }
    }

    /// The name, or the address when it has none.
    pub fn label(&self) -> &str {
        self.name.as_deref().unwrap_or(&self.address)
    }
}

// --- reading bluetoothctl -------------------------------------------------------------------------------------

/// Removes colour codes, carriage returns and the line clearing `bluetoothctl` writes around its prompt.
pub fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\u{1b}' => {
                // CSI: ESC [ parameters final-byte. Anything else after ESC: drop the one character.
                if chars.peek() == Some(&'[') {
                    chars.next();
                    for d in chars.by_ref() {
                        if ('@'..='~').contains(&d) {
                            break;
                        }
                    }
                } else {
                    chars.next();
                }
            }
            '\r' | '\u{1}' | '\u{2}' => {}
            c => out.push(c),
        }
    }
    out
}

/// "e4:17:d8:0b:1d:0e" or "E4:17:D8:0B:1D:0E": six pairs of hex digits. Upper case, or `None`.
pub fn address(s: &str) -> Option<String> {
    let parts: Vec<&str> = s.split(':').collect();
    (parts.len() == 6 && parts.iter().all(|p| p.len() == 2 && p.bytes().all(|b| b.is_ascii_hexdigit()))).then(|| s.to_ascii_uppercase())
}

/// A name BlueZ made up from the address ("7A-41-0F-0A-0B-0C") is no name.
fn real_name(name: &str, address: &str) -> Option<String> {
    let name = name.trim();
    let made_up = name.is_empty() || name.eq_ignore_ascii_case(address) || name.eq_ignore_ascii_case(&address.replace(':', "-"));
    (!made_up).then(|| name.to_string())
}

/// The devices in the output of `bluetoothctl devices [Paired|Connected|Trusted]`, and the `[NEW]` ones of a scan,
/// as (address, name). Lines that change (`[CHG]`) or remove (`[DEL]`) a device, prompts and anything that is not a
/// device line are left out.
pub fn parse_devices(out: &str) -> Vec<(String, Option<String>)> {
    let mut found: Vec<(String, Option<String>)> = Vec::new();
    for line in strip_ansi(out).lines() {
        let mut line = line.trim();
        if let Some(rest) = line.strip_prefix('[') {
            let Some((tag, rest)) = rest.split_once(']') else { continue };
            if tag != "NEW" {
                continue;
            }
            line = rest.trim();
        }
        let Some(rest) = line.strip_prefix("Device ") else { continue };
        let (addr, name) = rest.split_once(' ').unwrap_or((rest, ""));
        let Some(addr) = address(addr.trim()) else { continue };
        let name = real_name(name, &addr);
        match found.iter_mut().find(|(a, _)| *a == addr) {
            Some(known) => known.1 = name.or(known.1.take()),
            None => found.push((addr, name)),
        }
    }
    found
}

/// The first number of "0x00002508 (9480)" or "0xffffffc4 (-60)": the one in brackets, else the hex.
fn number(value: &str) -> Option<i64> {
    if let Some(inner) = value.split_once('(').and_then(|(_, r)| r.split_once(')')).map(|(n, _)| n.trim()) {
        if let Ok(n) = inner.parse::<i64>() {
            return Some(n);
        }
    }
    let hex = value.split_whitespace().next()?.strip_prefix("0x")?;
    i64::from_str_radix(hex, 16).ok()
}

/// The output of `bluetoothctl info <address>`; `None` for "Device ... not available" or anything unreadable.
pub fn parse_info(out: &str) -> Option<Device> {
    let text = strip_ansi(out);
    let mut lines = text.lines().map(str::trim).filter(|l| !l.is_empty());
    let head = lines.by_ref().find(|l| l.starts_with("Device "))?;
    if head.contains("not available") {
        return None;
    }
    let addr = address(head.trim_start_matches("Device ").split_whitespace().next()?)?;
    let mut d = Device {
        address: addr.clone(),
        ..Device::default()
    };
    let (mut name, mut alias) = (None, None);
    for line in lines {
        let Some((key, value)) = line.split_once(':') else { continue };
        let value = value.trim();
        let yes = value == "yes";
        match key.trim() {
            "Name" => name = real_name(value, &addr),
            "Alias" => alias = real_name(value, &addr),
            "Class" => d.class = number(value).and_then(|n| u32::try_from(n).ok()),
            "Appearance" => d.appearance = number(value).and_then(|n| u16::try_from(n).ok()),
            "Icon" => d.icon = Some(value.to_string()).filter(|i| !i.is_empty()),
            "Paired" => d.paired = yes,
            "Trusted" => d.trusted = yes,
            "Connected" => d.connected = yes,
            "RSSI" => d.rssi = number(value).and_then(|n| i16::try_from(n).ok()),
            _ => {}
        }
    }
    // The alias is what the user (or BlueZ) calls it; it is the name unless renamed.
    d.name = alias.or(name);
    Some(d)
}

// --- errors in plain words -------------------------------------------------------------------------------------

/// A step of the work, for the progress shown and for the words of an error.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    /// The device was no longer known to BlueZ: a short search for it.
    Looking,
    Pairing,
    Trusting,
    Connecting,
    Disconnecting,
    Forgetting,
}

/// What `bluetoothctl` printed when something failed ("Failed to pair: org.bluez.Error.AuthenticationFailed"),
/// said so that someone holding a controller knows what to do.
pub fn explain(step: Step, raw: &str) -> String {
    let has = |words: &[&str]| words.iter().any(|w| raw.contains(w));
    let msg = if has(&["not available"]) {
        "The device is out of range. Put it in pairing mode, search again, then pick it."
    } else if has(&["No default controller", "NotReady", "Waiting to connect to bluetoothd"]) {
        "Bluetooth is not ready. Switch it off and on, then try again."
    } else if has(&["InProgress", "Busy", "busy"]) {
        "Bluetooth is busy with something else. Wait a moment and try again."
    } else if has(&["AuthenticationFailed", "AuthenticationRejected", "AuthenticationCanceled", "Rejected"]) {
        "The device declined the pairing. Put it in pairing mode first, then try again."
    } else if has(&[
        "AuthenticationTimeout",
        "ConnectionAttemptFailed",
        "page-timeout",
        "Page Timeout",
        "Host is down",
        "connection-abort",
        "timed out",
        "Timeout",
    ]) {
        match step {
            Step::Connecting => "The device did not answer. Switch it on (controllers: press the home button) and keep it close.",
            _ => "The device did not answer. Put it in pairing mode first (a controller's light flashes fast) and keep it close.",
        }
    } else {
        match step {
            Step::Looking | Step::Pairing => "Pairing did not work. Put the device in pairing mode first, then try again.",
            Step::Trusting => "Paired, but it could not be set to reconnect by itself. Forget it and pair it again.",
            Step::Connecting => "Paired, but it did not connect. Switch the device on and press A again.",
            Step::Disconnecting => "It did not disconnect. Switch the device off instead.",
            Step::Forgetting => "It could not be forgotten. Switch Bluetooth off and on, then try again.",
        }
    };
    msg.to_string()
}

// --- the seam ---------------------------------------------------------------------------------------------------

/// What the page needs from BlueZ. `Bluetoothctl` is the real one, `MockBluetooth` an imagined room full of devices
/// for tests, `--snapshot` and the virtual device. Every method may take seconds: call them from a worker thread.
/// Errors are what `bluetoothctl` printed; `explain` says them in plain words.
pub trait Bluetooth: Send + Sync {
    /// Bluetooth is up and its adapter switched on (switches it on when it is not). Before a search.
    fn ready(&self) -> Result<(), String>;
    /// Every device BlueZ knows right now.
    fn devices(&self) -> Result<Vec<Device>, String>;
    /// One device, or `None` when BlueZ does not know it (any more).
    fn info(&self, address: &str) -> Option<Device>;
    /// Looks for devices for `secs` seconds, or until `stop` is set. Found devices show up in `devices`.
    fn scan(&self, secs: u64, stop: &AtomicBool) -> Result<(), String>;
    fn pair(&self, address: &str) -> Result<(), String>;
    fn trust(&self, address: &str) -> Result<(), String>;
    fn connect(&self, address: &str) -> Result<(), String>;
    fn disconnect(&self, address: &str) -> Result<(), String>;
    /// Unpairs and forgets it, also in ROCKNIX's copy of the pairings.
    fn remove(&self, address: &str) -> Result<(), String>;
}

/// How the pairing waits between tries; zero in tests.
#[derive(Clone, Copy, Debug)]
pub struct Pace {
    /// Between two tries to connect.
    pub retry: Duration,
    /// How long to look for a device BlueZ has already forgotten again.
    pub look_secs: u64,
}

impl Default for Pace {
    fn default() -> Self {
        Pace {
            retry: Duration::from_millis(1500),
            look_secs: 8,
        }
    }
}

/// Tries to connect after pairing. Some controllers connect by themselves a moment after pairing, others
/// only answer the second call (ROCKNIX's agent tries up to five times).
const CONNECT_TRIES: usize = 3;

/// A found device becomes a paired, trusted and connected one: `pair`, `trust`, `connect`, with what is being done
/// told to `progress`. A device that is already paired is not an error. One that BlueZ dropped since the search
/// (it forgets unpaired devices 30 s after a search) is looked for once more. The error is in plain words.
pub fn pair_trust_connect(bt: &dyn Bluetooth, addr: &str, pace: Pace, progress: &mut dyn FnMut(Step)) -> Result<(), String> {
    progress(Step::Pairing);
    if let Err(e) = bt.pair(addr) {
        if e.contains("AlreadyExists") {
            // Paired before: go on with trust and connect.
        } else if e.contains("not available") {
            progress(Step::Looking);
            let _ = bt.scan(pace.look_secs, &AtomicBool::new(false));
            progress(Step::Pairing);
            bt.pair(addr)
                .or_else(|e| if e.contains("AlreadyExists") { Ok(()) } else { Err(e) })
                .map_err(|e| explain(Step::Pairing, &e))?;
        } else {
            return Err(explain(Step::Pairing, &e));
        }
    }
    progress(Step::Trusting);
    bt.trust(addr).map_err(|e| explain(Step::Trusting, &e))?;
    progress(Step::Connecting);
    connect_with_retries(bt, addr, pace)
}

/// `connect`, tried again a few times; connected by itself in the meantime counts too.
pub fn connect_with_retries(bt: &dyn Bluetooth, addr: &str, pace: Pace) -> Result<(), String> {
    let mut last = String::new();
    for attempt in 0..CONNECT_TRIES {
        if attempt > 0 {
            std::thread::sleep(pace.retry);
        }
        match bt.connect(addr) {
            Ok(()) => return Ok(()),
            Err(e) if e.contains("AlreadyConnected") => return Ok(()),
            Err(e) => last = e,
        }
        if bt.info(addr).is_some_and(|d| d.connected) {
            return Ok(());
        }
    }
    Err(explain(Step::Connecting, &last))
}

/// Runs `work` on its own thread and hands its result to `done` there. Never blocks the caller.
pub fn spawn<T: Send + 'static>(name: &str, work: impl FnOnce() -> T + Send + 'static, done: impl FnOnce(T) + Send + 'static) {
    let spawned = std::thread::Builder::new().name(name.to_string()).spawn(move || done(work()));
    if let Err(e) = spawned {
        eprintln!("bluetooth: can't start a thread for {name}: {e}");
    }
}

// --- the real one: bluetoothctl -----------------------------------------------------------------------------------

/// A `bluetoothctl` call that only asks: `devices`, `info`, `show`.
const ASK: Duration = Duration::from_secs(4);
/// `pair` waits for the controller to answer; a page timeout alone takes about 5 s, an LE pairing can take 20.
const PAIR: Duration = Duration::from_secs(40);
const CONNECT: Duration = Duration::from_secs(25);
const SHORT: Duration = Duration::from_secs(10);
/// At most this many `info` calls per look at the devices; the rest are asked the next time.
const INFO_PER_LOOK: usize = 12;
/// A device whose `info` told nothing about its kind (most nameless LE noise) is asked again after this long.
const ASK_AGAIN: Duration = Duration::from_secs(10);

pub struct Bluetoothctl {
    program: PathBuf,
    /// ROCKNIX's `rocknix-bluetooth`, for `save` after forgetting a device. Not on every system.
    rocknix: Option<PathBuf>,
    /// What `info` said about each device (icon, class, appearance), and when: asked once per device.
    known: Mutex<HashMap<String, (Device, Instant)>>,
}

impl Bluetoothctl {
    /// `None` where there is no `bluetoothctl` (a desktop, the virtual device).
    pub fn system() -> Option<Bluetoothctl> {
        let program = ["/usr/bin/bluetoothctl", "/bin/bluetoothctl"].iter().map(PathBuf::from).find(|p| p.exists())?;
        let rocknix = Some(PathBuf::from("/usr/bin/rocknix-bluetooth")).filter(|p| p.exists());
        Some(Bluetoothctl::with(program, rocknix))
    }

    /// A given program for `bluetoothctl` (a stand-in script in tests).
    pub fn with(program: impl Into<PathBuf>, rocknix: Option<PathBuf>) -> Bluetoothctl {
        Bluetoothctl {
            program: program.into(),
            rocknix,
            known: Mutex::new(HashMap::new()),
        }
    }

    /// stdout and stderr together; `Err` with that text when it failed, or "timed out".
    fn run(&self, args: &[&str], timeout: Duration) -> Result<String, String> {
        let mut cmd = Command::new(&self.program);
        cmd.args(args);
        let out = proc::run_with_timeout(&mut cmd, timeout).map_err(|e| match e.kind() {
            std::io::ErrorKind::TimedOut => format!("bluetoothctl {}: timed out", args.join(" ")),
            _ => format!("bluetoothctl: {e}"),
        })?;
        let text = strip_ansi(&format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr)));
        if out.status.success() {
            Ok(text)
        } else {
            let text = text.trim();
            Err(if text.is_empty() {
                format!("bluetoothctl {} failed", args.join(" "))
            } else {
                text.to_string()
            })
        }
    }

    /// The addresses `devices <filter>` lists; `None` when this BlueZ does not know the filter (before 5.66).
    fn listed(&self, filter: &str) -> Option<Vec<String>> {
        let out = self.run(&["devices", filter], ASK).ok()?;
        if out.contains("Invalid argument") {
            return None;
        }
        Some(parse_devices(&out).into_iter().map(|(a, _)| a).collect())
    }
}

/// Checks the action's output for the words of success, since a failed call does not always end with an error code.
fn expect(out: Result<String, String>, success: &[&str]) -> Result<(), String> {
    let out = out?;
    if success.iter().any(|w| out.contains(w)) {
        Ok(())
    } else {
        Err(out.trim().to_string())
    }
}

impl Bluetooth for Bluetoothctl {
    fn ready(&self) -> Result<(), String> {
        let out = self.run(&["show"], ASK)?;
        if out.contains("No default controller") {
            return Err(out);
        }
        if out.contains("Powered: yes") {
            return Ok(());
        }
        expect(self.run(&["power", "on"], SHORT), &["succeeded"])
    }

    fn devices(&self) -> Result<Vec<Device>, String> {
        let out = self.run(&["devices"], ASK)?;
        if out.contains("No default controller") {
            return Err(out.trim().to_string());
        }
        let listed = parse_devices(&out);
        let sets = (self.listed("Paired"), self.listed("Connected"), self.listed("Trusted"));
        let mut known = self.known.lock().unwrap_or_else(|e| e.into_inner());
        known.retain(|a, _| listed.iter().any(|(l, _)| l == a));
        // Without the filters (an old BlueZ) every device is asked, a few per look.
        let all_need_info = sets.0.is_none() || sets.1.is_none() || sets.2.is_none();
        let mut asked = 0;
        let mut devices = Vec::with_capacity(listed.len());
        for (addr, name) in listed {
            let stale = known.get(&addr).is_none_or(|(d, at)| {
                let kindless = d.icon.is_none() && d.class.is_none() && d.appearance.is_none();
                all_need_info || (kindless && at.elapsed() >= ASK_AGAIN)
            });
            if stale && asked < INFO_PER_LOOK {
                asked += 1;
                let info = self.run(&["info", &addr], ASK).ok().as_deref().and_then(parse_info);
                known.insert(addr.clone(), (info.unwrap_or_default(), Instant::now()));
            }
            let mut d = known.get(&addr).map(|(d, _)| d.clone()).unwrap_or_default();
            d.address = addr.clone();
            d.name = name.or(d.name);
            if let (Some(p), Some(c), Some(t)) = (&sets.0, &sets.1, &sets.2) {
                d.paired = p.contains(&addr);
                d.connected = c.contains(&addr);
                d.trusted = t.contains(&addr);
            }
            devices.push(d);
        }
        Ok(devices)
    }

    fn info(&self, address: &str) -> Option<Device> {
        self.run(&["info", address], ASK)
            .ok()
            .as_deref()
            .and_then(parse_info)
            .filter(|d| d.address.eq_ignore_ascii_case(address))
    }

    fn scan(&self, secs: u64, stop: &AtomicBool) -> Result<(), String> {
        let mut cmd = Command::new(&self.program);
        cmd.args(["--timeout", &secs.to_string(), "scan", "on"]);
        // `--timeout` ends it; ours is a little longer, for the day it does not.
        match proc::run_until(&mut cmd, Duration::from_secs(secs + 5), stop) {
            Ok(out) => {
                let text = strip_ansi(&String::from_utf8_lossy(&out.stdout));
                match text
                    .lines()
                    .find(|l| l.contains("Failed to start discovery") || l.contains("No default controller"))
                {
                    Some(line) => Err(line.trim().to_string()),
                    None => Ok(()),
                }
            }
            Err(e) if matches!(e.kind(), std::io::ErrorKind::Interrupted | std::io::ErrorKind::TimedOut) => Ok(()),
            Err(e) => Err(format!("bluetoothctl: {e}")),
        }
    }

    fn pair(&self, address: &str) -> Result<(), String> {
        expect(self.run(&["pair", address], PAIR), &["Pairing successful"])
    }

    fn trust(&self, address: &str) -> Result<(), String> {
        expect(self.run(&["trust", address], SHORT), &["succeeded"])
    }

    fn connect(&self, address: &str) -> Result<(), String> {
        expect(self.run(&["connect", address], CONNECT), &["Connection successful"])
    }

    fn disconnect(&self, address: &str) -> Result<(), String> {
        expect(self.run(&["disconnect", address], SHORT), &["Disconnection successful"])
    }

    fn remove(&self, address: &str) -> Result<(), String> {
        expect(self.run(&["remove", address], SHORT), &["has been removed"])?;
        self.known.lock().unwrap_or_else(|e| e.into_inner()).remove(address);
        if let Some(script) = &self.rocknix {
            let saved = proc::status_with_timeout(Command::new(script).arg("save"), SHORT).is_ok_and(|s| s.success());
            if !saved {
                eprintln!("bluetooth: rocknix-bluetooth save failed; {address} may come back from the backup");
            }
        }
        Ok(())
    }
}

// --- the imagined one ---------------------------------------------------------------------------------------------

/// A device of the mock, and how it behaves.
#[derive(Clone, Debug)]
pub struct MockDevice {
    pub device: Device,
    /// Known before any search (paired), or seen by one.
    pub seen: bool,
    /// In pairing mode: a pairing works. One that is not refuses the first time, then works (as if the
    /// player put it in pairing mode after reading the message).
    pub pairing_mode: bool,
}

pub struct MockBluetooth {
    devices: Mutex<Vec<MockDevice>>,
    /// How long each action takes.
    delay: Duration,
    /// How long a search runs at most (it also stops when told to).
    search: Duration,
    pub calls: Mutex<Vec<String>>,
}

impl MockBluetooth {
    pub fn new(devices: Vec<MockDevice>, delay: Duration, search: Duration) -> MockBluetooth {
        MockBluetooth {
            devices: Mutex::new(devices),
            delay,
            search,
            calls: Mutex::new(Vec::new()),
        }
    }

    /// A room with one paired controller and, once searched, three more, a speaker and some nameless noise.
    pub fn demo(delay: Duration, search: Duration) -> MockBluetooth {
        MockBluetooth::new(demo_devices(), delay, search)
    }

    fn log(&self, call: String) {
        self.calls.lock().unwrap_or_else(|e| e.into_inner()).push(call);
    }

    fn with<R>(&self, f: impl FnOnce(&mut Vec<MockDevice>) -> R) -> R {
        f(&mut self.devices.lock().unwrap_or_else(|e| e.into_inner()))
    }

    fn wait(&self) {
        if !self.delay.is_zero() {
            std::thread::sleep(self.delay);
        }
    }

    /// Changes the device at `address` if a search has seen it; else the error `bluetoothctl` prints.
    fn change(&self, address: &str, f: impl FnOnce(&mut MockDevice) -> Result<(), String>) -> Result<(), String> {
        self.with(|all| match all.iter_mut().find(|d| d.seen && d.device.address == address) {
            Some(d) => f(d),
            None => Err(format!("Device {address} not available")),
        })
    }
}

/// The devices of `MockBluetooth::demo`.
pub fn demo_devices() -> Vec<MockDevice> {
    let dev = |address: &str, name: Option<&str>, icon: Option<&str>, class: Option<u32>, rssi: i16| Device {
        address: address.into(),
        name: name.map(str::to_string),
        icon: icon.map(str::to_string),
        class,
        rssi: Some(rssi),
        ..Device::default()
    };
    let found = |device: Device| MockDevice {
        device,
        seen: false,
        pairing_mode: true,
    };
    let mut xbox = dev("4C:B9:9B:0A:1B:2C", Some("Xbox Wireless Controller"), Some("input-gaming"), None, -58);
    (xbox.paired, xbox.trusted) = (true, true);
    let mut pro = found(dev("98:B6:E9:12:34:56", Some("Pro Controller"), Some("input-gaming"), Some(0x002508), -64));
    pro.pairing_mode = false;
    vec![
        MockDevice {
            device: xbox,
            seen: true,
            pairing_mode: true,
        },
        found(dev(
            "E4:17:D8:0B:1D:0E",
            Some("8BitDo Ultimate 2C Wireless"),
            Some("input-gaming"),
            Some(0x002508),
            -52,
        )),
        found(dev("3C:4D:BE:01:02:03", Some("DualSense Wireless Controller"), None, Some(0x002508), -70)),
        pro,
        found(dev("A0:5A:5D:11:22:33", Some("JBL Flip 5"), Some("audio-card"), Some(0x240414), -66)),
        found(dev("7A:41:0F:0A:0B:0C", None, None, None, -90)),
        found(dev("5F:12:C0:AA:BB:CC", None, None, None, -84)),
        found(dev("6E:00:11:22:33:44", None, None, None, -95)),
    ]
}

impl Bluetooth for MockBluetooth {
    fn ready(&self) -> Result<(), String> {
        self.log("ready".into());
        Ok(())
    }

    fn devices(&self) -> Result<Vec<Device>, String> {
        Ok(self.with(|all| all.iter().filter(|d| d.seen).map(|d| d.device.clone()).collect()))
    }

    fn info(&self, address: &str) -> Option<Device> {
        self.with(|all| all.iter().find(|d| d.seen && d.device.address == address).map(|d| d.device.clone()))
    }

    fn scan(&self, secs: u64, stop: &AtomicBool) -> Result<(), String> {
        self.log(format!("scan {secs}"));
        self.with(|all| all.iter_mut().for_each(|d| d.seen = true));
        let end = Instant::now() + self.search.min(Duration::from_secs(secs));
        while Instant::now() < end && !stop.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_millis(50));
        }
        Ok(())
    }

    fn pair(&self, address: &str) -> Result<(), String> {
        self.log(format!("pair {address}"));
        self.wait();
        self.change(address, |d| {
            if d.device.paired {
                return Err("Failed to pair: org.bluez.Error.AlreadyExists".into());
            }
            if !d.pairing_mode {
                d.pairing_mode = true;
                return Err("Failed to pair: org.bluez.Error.AuthenticationFailed".into());
            }
            d.device.paired = true;
            Ok(())
        })
    }

    fn trust(&self, address: &str) -> Result<(), String> {
        self.log(format!("trust {address}"));
        self.change(address, |d| {
            d.device.trusted = true;
            Ok(())
        })
    }

    fn connect(&self, address: &str) -> Result<(), String> {
        self.log(format!("connect {address}"));
        self.wait();
        self.change(address, |d| {
            d.device.connected = true;
            Ok(())
        })
    }

    fn disconnect(&self, address: &str) -> Result<(), String> {
        self.log(format!("disconnect {address}"));
        self.wait();
        self.change(address, |d| {
            d.device.connected = false;
            Ok(())
        })
    }

    fn remove(&self, address: &str) -> Result<(), String> {
        self.log(format!("remove {address}"));
        self.wait();
        let known = self.with(|all| {
            let before = all.len();
            all.retain(|d| !(d.seen && d.device.address == address));
            all.len() < before
        });
        if known {
            Ok(())
        } else {
            Err(format!("Device {address} not available"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;
    use std::path::Path;

    const DEVICES: &str = include_str!("fixtures/bluetoothctl-devices.txt");
    const PAIRED: &str = include_str!("fixtures/bluetoothctl-devices-paired.txt");
    const INFO_8BITDO: &str = include_str!("fixtures/bluetoothctl-info-8bitdo.txt");
    const INFO_XBOX: &str = include_str!("fixtures/bluetoothctl-info-xbox.txt");
    const INFO_DUALSENSE: &str = include_str!("fixtures/bluetoothctl-info-dualsense.txt");
    const INFO_NOISE: &str = include_str!("fixtures/bluetoothctl-info-noise.txt");
    const INFO_SPEAKER: &str = include_str!("fixtures/bluetoothctl-info-speaker.txt");
    const INFO_MISSING: &str = include_str!("fixtures/bluetoothctl-info-missing.txt");
    const SCAN: &str = include_str!("fixtures/bluetoothctl-scan.txt");

    // --- parsing ---

    #[test]
    fn the_device_list_gives_addresses_and_names_and_no_made_up_names() {
        let d = parse_devices(DEVICES);
        assert_eq!(d.len(), 7);
        assert_eq!(d[0], ("E4:17:D8:0B:1D:0E".into(), Some("8BitDo Ultimate 2C Wireless".into())));
        assert_eq!(d[3], ("7A:41:0F:0A:0B:0C".into(), None), "the address with dashes is no name");
        assert_eq!(parse_devices(PAIRED).len(), 2);
        assert!(parse_devices("").is_empty());
        assert!(parse_devices("No default controller available\n").is_empty());
    }

    #[test]
    fn a_scan_gives_its_new_devices_only_without_colours_or_prompts() {
        let d = parse_devices(SCAN);
        let addrs: Vec<&str> = d.iter().map(|(a, _)| a.as_str()).collect();
        assert_eq!(addrs, ["3C:4D:BE:01:02:03", "7A:41:0F:0A:0B:0C", "A0:5A:5D:11:22:33", "6E:00:11:22:33:44"]);
        assert_eq!(d[0].1.as_deref(), Some("DualSense Wireless Controller"));
        assert!(d.iter().all(|(_, n)| n.as_deref().is_none_or(|n| !n.contains('\u{1b}'))));
    }

    #[test]
    fn junk_lines_and_broken_addresses_are_left_out() {
        let out = "Device ZZ:17:D8:0B:1D:0E Fake\nDevice E4:17:D8 Short\nAgent registered\n[bluetooth]# Device\nDevice e4:17:d8:0b:1d:0e lower case\n\u{1b}[0;94m[CHG]\u{1b}[0m Device E4:17:D8:0B:1D:0E Connected: yes\n";
        assert_eq!(parse_devices(out), [("E4:17:D8:0B:1D:0E".to_string(), Some("lower case".to_string()))]);
    }

    #[test]
    fn info_of_a_paired_classic_controller() {
        let d = parse_info(INFO_8BITDO).unwrap();
        assert_eq!(d.address, "E4:17:D8:0B:1D:0E");
        assert_eq!(d.name.as_deref(), Some("8BitDo Ultimate 2C Wireless"));
        assert_eq!(d.class, Some(0x2508));
        assert_eq!(d.icon.as_deref(), Some("input-gaming"));
        assert!(d.paired && d.trusted && d.connected);
        assert_eq!(d.rssi, Some(-60));
        assert!(d.is_gamepad());
        assert_eq!(d.kind(), Kind::Gamepad);
    }

    #[test]
    fn info_of_an_le_controller_by_its_appearance() {
        let d = parse_info(INFO_XBOX).unwrap();
        assert_eq!(d.appearance, Some(0x03c4));
        assert!(d.paired && d.trusted && !d.connected);
        let no_icon = Device { icon: None, ..d };
        assert!(no_icon.is_gamepad(), "the appearance alone says gamepad");
    }

    #[test]
    fn a_gamepad_by_its_class_alone_and_other_kinds() {
        let d = parse_info(INFO_DUALSENSE).unwrap();
        assert_eq!(d.icon, None);
        assert!(d.is_gamepad(), "class 0x2508: peripheral, gamepad");
        assert!(!d.paired);
        let speaker = parse_info(INFO_SPEAKER).unwrap();
        assert!(!speaker.is_gamepad());
        assert_eq!(speaker.kind(), Kind::Audio);
        let joystick = Device {
            class: Some(0x0504),
            ..Device::default()
        };
        assert!(joystick.is_gamepad());
        let keyboard = Device {
            class: Some(0x0540),
            ..Device::default()
        };
        assert!(!keyboard.is_gamepad(), "a keyboard is a peripheral too");
        assert_eq!(
            Device {
                icon: Some("input-keyboard".into()),
                ..Device::default()
            }
            .kind(),
            Kind::Keyboard
        );
        assert_eq!(
            Device {
                class: Some(0x5a020c),
                ..Device::default()
            }
            .kind(),
            Kind::Phone
        );
    }

    #[test]
    fn info_of_nameless_noise_and_of_an_unknown_device() {
        let d = parse_info(INFO_NOISE).unwrap();
        assert_eq!(d.name, None);
        assert_eq!(d.label(), "7A:41:0F:0A:0B:0C");
        assert_eq!(d.rssi, Some(-90));
        assert_eq!(d.kind(), Kind::Other);
        assert_eq!(parse_info(INFO_MISSING), None);
        assert_eq!(parse_info(""), None);
        assert_eq!(parse_info("garbage\n\tPaired: yes\n"), None);
    }

    #[test]
    fn colour_codes_are_removed() {
        assert_eq!(strip_ansi("\u{1b}[0;92m[NEW]\u{1b}[0m Device"), "[NEW] Device");
        assert_eq!(strip_ansi("\r\u{1b}[K[bluetooth]# "), "[bluetooth]# ");
        assert_eq!(strip_ansi("plain"), "plain");
    }

    #[test]
    fn errors_are_said_in_plain_words() {
        let pair = |raw| explain(Step::Pairing, raw);
        assert!(pair("Failed to pair: org.bluez.Error.AuthenticationFailed").contains("pairing mode first"));
        assert!(pair("Failed to pair: org.bluez.Error.ConnectionAttemptFailed").contains("pairing mode first"));
        assert!(pair("Failed to pair: org.bluez.Error.AuthenticationTimeout").contains("did not answer"));
        assert!(pair("bluetoothctl pair X: timed out").contains("did not answer"));
        assert!(pair("Device E4:17:D8:0B:1D:0E not available").contains("search again"));
        assert!(pair("Failed to pair: org.bluez.Error.InProgress").contains("busy"));
        assert!(pair("No default controller available").contains("not ready"));
        assert!(pair("something new").contains("pairing mode first"), "unknown errors still say what to do");
        assert!(explain(Step::Connecting, "Failed to connect: org.bluez.Error.Failed br-connection-page-timeout").contains("Switch it on"));
        for step in [
            Step::Looking,
            Step::Pairing,
            Step::Trusting,
            Step::Connecting,
            Step::Disconnecting,
            Step::Forgetting,
        ] {
            let msg = explain(step, "???");
            assert!(!msg.contains("org.bluez") && msg.ends_with('.'), "{msg}");
        }
    }

    // --- the pairing, step by step ---

    fn quick() -> Pace {
        Pace {
            retry: Duration::ZERO,
            look_secs: 0,
        }
    }

    fn mock() -> MockBluetooth {
        MockBluetooth::demo(Duration::ZERO, Duration::ZERO)
    }

    fn searched() -> MockBluetooth {
        let m = mock();
        m.scan(1, &AtomicBool::new(false)).unwrap();
        m
    }

    #[test]
    fn pairing_pairs_trusts_and_connects_in_that_order() {
        let m = searched();
        let mut steps = Vec::new();
        pair_trust_connect(&m, "E4:17:D8:0B:1D:0E", quick(), &mut |s| steps.push(s)).unwrap();
        assert_eq!(steps, [Step::Pairing, Step::Trusting, Step::Connecting]);
        let d = m.info("E4:17:D8:0B:1D:0E").unwrap();
        assert!(d.paired && d.trusted && d.connected, "trusted, so it reconnects by itself after sleep");
        let calls = m.calls.lock().unwrap().clone();
        assert_eq!(calls[1..], ["pair E4:17:D8:0B:1D:0E", "trust E4:17:D8:0B:1D:0E", "connect E4:17:D8:0B:1D:0E"]);
    }

    #[test]
    fn a_controller_not_in_pairing_mode_gets_a_clear_message_and_the_next_try_works() {
        let m = searched();
        let err = pair_trust_connect(&m, "98:B6:E9:12:34:56", quick(), &mut |_| {}).unwrap_err();
        assert!(err.contains("pairing mode first"), "{err}");
        assert!(!m.info("98:B6:E9:12:34:56").unwrap().paired);
        pair_trust_connect(&m, "98:B6:E9:12:34:56", quick(), &mut |_| {}).unwrap();
    }

    #[test]
    fn a_device_already_paired_is_trusted_and_connected_without_an_error() {
        let m = searched();
        pair_trust_connect(&m, "4C:B9:9B:0A:1B:2C", quick(), &mut |_| {}).unwrap();
        assert!(m.info("4C:B9:9B:0A:1B:2C").unwrap().connected);
    }

    #[test]
    fn a_device_bluez_forgot_since_the_search_is_looked_for_once_more() {
        let m = mock(); // not searched: BlueZ does not know the 8BitDo (any more)
        let mut steps = Vec::new();
        pair_trust_connect(&m, "E4:17:D8:0B:1D:0E", quick(), &mut |s| steps.push(s)).unwrap();
        assert_eq!(steps, [Step::Pairing, Step::Looking, Step::Pairing, Step::Trusting, Step::Connecting]);
        let gone = pair_trust_connect(&m, "00:11:22:33:44:55", quick(), &mut |_| {}).unwrap_err();
        assert!(gone.contains("search again"), "{gone}");
    }

    /// A Bluetooth whose `connect` fails a given number of times, and may connect by itself meanwhile.
    struct Stubborn {
        fails: Mutex<usize>,
        connects_itself: bool,
        tries: Mutex<usize>,
    }

    impl Bluetooth for Stubborn {
        fn ready(&self) -> Result<(), String> {
            Ok(())
        }
        fn devices(&self) -> Result<Vec<Device>, String> {
            Ok(Vec::new())
        }
        fn info(&self, address: &str) -> Option<Device> {
            Some(Device {
                address: address.into(),
                connected: self.connects_itself,
                ..Device::default()
            })
        }
        fn scan(&self, _: u64, _: &AtomicBool) -> Result<(), String> {
            Ok(())
        }
        fn pair(&self, _: &str) -> Result<(), String> {
            Ok(())
        }
        fn trust(&self, _: &str) -> Result<(), String> {
            Ok(())
        }
        fn connect(&self, _: &str) -> Result<(), String> {
            *self.tries.lock().unwrap() += 1;
            let mut fails = self.fails.lock().unwrap();
            if *fails > 0 {
                *fails -= 1;
                return Err("Failed to connect: org.bluez.Error.Failed br-connection-busy".into());
            }
            Ok(())
        }
        fn disconnect(&self, _: &str) -> Result<(), String> {
            Ok(())
        }
        fn remove(&self, _: &str) -> Result<(), String> {
            Ok(())
        }
    }

    fn stubborn(fails: usize, connects_itself: bool) -> Stubborn {
        Stubborn {
            fails: Mutex::new(fails),
            connects_itself,
            tries: Mutex::new(0),
        }
    }

    #[test]
    fn connecting_is_tried_again_and_a_controller_that_connected_itself_counts() {
        let s = stubborn(2, false);
        connect_with_retries(&s, "E4:17:D8:0B:1D:0E", quick()).unwrap();
        assert_eq!(*s.tries.lock().unwrap(), 3);
        let s = stubborn(5, false);
        let err = connect_with_retries(&s, "E4:17:D8:0B:1D:0E", quick()).unwrap_err();
        assert_eq!(*s.tries.lock().unwrap(), CONNECT_TRIES);
        assert!(err.contains("busy"), "{err}");
        let s = stubborn(5, true);
        connect_with_retries(&s, "E4:17:D8:0B:1D:0E", quick()).unwrap();
        assert_eq!(*s.tries.lock().unwrap(), 1, "it said no, but it is connected: fine");
    }

    #[test]
    fn the_mock_forgets_and_a_search_stops_when_told() {
        let m = searched();
        m.remove("4C:B9:9B:0A:1B:2C").unwrap();
        assert!(m.info("4C:B9:9B:0A:1B:2C").is_none());
        assert!(m.remove("4C:B9:9B:0A:1B:2C").is_err());
        let slow = MockBluetooth::demo(Duration::ZERO, Duration::from_secs(10));
        let t0 = Instant::now();
        slow.scan(15, &AtomicBool::new(true)).unwrap();
        assert!(t0.elapsed() < Duration::from_secs(1));
    }

    // --- the real one, against a stand-in bluetoothctl ---

    /// A shell script in place of `bluetoothctl`: answers from the fixtures and logs its arguments.
    fn fake_bluetoothctl(name: &str, script: &str) -> (TestDir, PathBuf, PathBuf) {
        let dir = TestDir::new(&format!("bt-{name}"));
        let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/fixtures");
        let log = dir.join("calls");
        let body = format!("#!/bin/sh\nF={}\necho \"$*\" >> {}\n{script}\n", fixtures.display(), log.display());
        let prog = dir.join("bluetoothctl");
        crate::testutil::write_executable(&prog, body);
        (dir, prog, log)
    }

    const FAKE: &str = r#"case "$*" in
  "devices") cat $F/bluetoothctl-devices.txt ;;
  "devices Paired"|"devices Trusted") cat $F/bluetoothctl-devices-paired.txt ;;
  "devices Connected") echo "Device E4:17:D8:0B:1D:0E 8BitDo Ultimate 2C Wireless" ;;
  "info E4:17:D8:0B:1D:0E") cat $F/bluetoothctl-info-8bitdo.txt ;;
  "info 4C:B9:9B:0A:1B:2C") cat $F/bluetoothctl-info-xbox.txt ;;
  "info 3C:4D:BE:01:02:03") cat $F/bluetoothctl-info-dualsense.txt ;;
  "info A0:5A:5D:11:22:33") cat $F/bluetoothctl-info-speaker.txt ;;
  info*) cat $F/bluetoothctl-info-noise.txt ;;
  "pair 3C:4D:BE:01:02:03") echo "Attempting to pair with 3C:4D:BE:01:02:03"; echo "Pairing successful" ;;
  "pair 98:B6:E9:12:34:56") echo "Attempting to pair with 98:B6:E9:12:34:56"; echo "Failed to pair: org.bluez.Error.AuthenticationFailed"; exit 1 ;;
  "trust "*) echo "Changing $2 trust succeeded" ;;
  "connect "*) echo "Attempting to connect to $2"; echo "Connection successful" ;;
  "remove "*) echo "Device has been removed" ;;
  "--timeout 1 scan on") echo "Discovery started"; sleep 1 ;;
  "--timeout 30 scan on") sleep 30 ;;
  "pair 11:11:11:11:11:11") sleep 60 ;;
  *) echo "Invalid command"; exit 1 ;;
esac"#;

    #[test]
    fn the_real_backend_reads_devices_with_their_kind_and_state() {
        let (_dir, prog, log) = fake_bluetoothctl("devices", FAKE);
        let bt = Bluetoothctl::with(&prog, None);
        let devices = bt.devices().unwrap();
        assert_eq!(devices.len(), 7);
        let by = |a: &str| devices.iter().find(|d| d.address == a).unwrap().clone();
        let eight = by("E4:17:D8:0B:1D:0E");
        assert!(eight.paired && eight.trusted && eight.connected && eight.is_gamepad());
        let xbox = by("4C:B9:9B:0A:1B:2C");
        assert!(xbox.paired && !xbox.connected && xbox.is_gamepad());
        assert!(by("3C:4D:BE:01:02:03").is_gamepad() && !by("3C:4D:BE:01:02:03").paired);
        assert_eq!(by("7A:41:0F:0A:0B:0C").name, None);
        assert_eq!(
            by("5F:12:C0:AA:BB:CC").address,
            "5F:12:C0:AA:BB:CC",
            "the listed address wins over what info said"
        );
        // `info` is asked once per device; the next look only lists (nameless noise is asked again after a while).
        let infos = || std::fs::read_to_string(&log).unwrap().lines().filter(|l| l.starts_with("info")).count();
        assert_eq!(infos(), 7);
        bt.devices().unwrap();
        assert_eq!(infos(), 7);
    }

    #[test]
    fn the_real_backend_pairs_step_by_step_and_says_why_it_failed() {
        let (_dir, prog, log) = fake_bluetoothctl("pair", FAKE);
        let bt = Bluetoothctl::with(&prog, None);
        pair_trust_connect(&bt, "3C:4D:BE:01:02:03", quick(), &mut |_| {}).unwrap();
        let calls = std::fs::read_to_string(&log).unwrap();
        assert_eq!(calls, "pair 3C:4D:BE:01:02:03\ntrust 3C:4D:BE:01:02:03\nconnect 3C:4D:BE:01:02:03\n");
        let err = pair_trust_connect(&bt, "98:B6:E9:12:34:56", quick(), &mut |_| {}).unwrap_err();
        assert!(err.contains("pairing mode first"), "{err}");
        bt.remove("4C:B9:9B:0A:1B:2C").unwrap();
    }

    #[test]
    fn the_real_backend_never_hangs_and_a_search_can_be_stopped() {
        let (_dir, prog, _) = fake_bluetoothctl("hang", FAKE);
        let bt = Bluetoothctl::with(&prog, None);
        bt.scan(1, &AtomicBool::new(false)).unwrap();
        let stop = std::sync::Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(200));
            flag.store(true, Ordering::Relaxed);
        });
        let t0 = Instant::now();
        bt.scan(30, &stop).unwrap();
        assert!(t0.elapsed() < Duration::from_secs(3), "stopped, not waited for: {:?}", t0.elapsed());
        // A dead `bluetoothd` makes bluetoothctl wait forever: every call has its own end.
        let (_dir, stuck, _) = fake_bluetoothctl("stuck", "sleep 60");
        let bt = Bluetoothctl::with(&stuck, None);
        let t0 = Instant::now();
        let err = bt.devices().unwrap_err();
        assert!(err.contains("timed out"), "{err}");
        assert!(t0.elapsed() < ASK + Duration::from_secs(2));
        assert!(explain(Step::Pairing, &err).contains("did not answer"));
    }
}
