//! Controller state, fed by evdev on Linux and by the keyboard on the desktop preview.

use std::collections::VecDeque;
use std::time::Instant;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Button {
    Up, Down, Left, Right,
    South, East, North, West,
    L1, R1, L2, R2, L3, R3,
    Select, Start, Mode, Back,
    Power, VolUp, VolDown,
}

impl Button {
    pub const ALL: [Button; 21] = [
        Button::Up, Button::Down, Button::Left, Button::Right,
        Button::South, Button::East, Button::North, Button::West,
        Button::L1, Button::R1, Button::L2, Button::R2, Button::L3, Button::R3,
        Button::Select, Button::Start, Button::Mode, Button::Back,
        Button::Power, Button::VolUp, Button::VolDown,
    ];

    fn index(self) -> usize {
        Self::ALL.iter().position(|b| *b == self).unwrap()
    }

    /// This button's bit in a mask of `Button`s (`Mux::buttons_of`).
    pub fn bit(self) -> u32 {
        1 << self.index()
    }

    pub fn label(self) -> &'static str {
        match self {
            Button::Up => "UP", Button::Down => "DOWN", Button::Left => "LEFT", Button::Right => "RIGHT",
            Button::South => "SOUTH", Button::East => "EAST", Button::North => "NORTH", Button::West => "WEST",
            Button::L1 => "L1", Button::R1 => "R1", Button::L2 => "L2", Button::R2 => "R2",
            Button::L3 => "L3", Button::R3 => "R3",
            Button::Select => "SELECT", Button::Start => "START", Button::Mode => "HOME", Button::Back => "BACK",
            Button::Power => "POWER", Button::VolUp => "VOL+", Button::VolDown => "VOL-",
        }
    }
}

pub const STICK_DEADZONE: f32 = 0.15;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Axis { LeftX, LeftY, RightX, RightY, L2, R2 }

impl Axis {
    pub const ALL: [Axis; 6] = [Axis::LeftX, Axis::LeftY, Axis::RightX, Axis::RightY, Axis::L2, Axis::R2];

    fn index(self) -> usize {
        Self::ALL.iter().position(|a| *a == self).unwrap()
    }
}

#[derive(Clone, Debug)]
pub enum PadEvent {
    Button { source: usize, button: Button, pressed: bool },
    Axis { source: usize, axis: Axis, value: f32 },
    /// A key or axis we do not map yet, shown raw in the log.
    Raw { source: usize, text: String },
    /// `supervise` opened a device: which controller it belongs to. Comes before any other event of `source`.
    Connected { source: usize, controller: Controller },
    /// The device of `source` is gone; the releases of what it held came just before.
    Disconnected { source: usize },
    /// The Flip 2's lid (`SW_LID` of its hall sensor, a `gpio-keys` device): closed or opened. Only sleep acts on it
    /// (docs/SLEEP.md "Lid"); nothing else reads it.
    Lid { source: usize, closed: bool },
}

/// The key of the Pocket's own controls: every device that is part of the handheld (InputPlumber's virtual pad and
/// keyboard, the volume and power keys) belongs to this one controller.
pub const BUILTIN: &str = "builtin";

/// One controller, as both programs name it (`linux::identify`): several devices can be one controller (the Pocket's
/// pad, its keyboard and its keys are all `BUILTIN`), a Bluetooth or USB controller is one of its own.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Controller {
    /// The same in pocket-ui and pocket-core and across reconnects (`controller_key`): `builtin`, `bt:<address>`,
    /// `usb:…`. Players are remembered by it (`players`).
    pub key: String,
    /// What the device calls itself ("Sony Interactive Entertainment DualSense Wireless Controller").
    pub name: String,
    /// A gamepad (it has a south face button), not only keys (a headset's volume buttons): only gamepads are players.
    pub gamepad: bool,
    /// The event node, for its rumble (`rumble`).
    pub node: String,
}

impl Controller {
    /// What a source nobody introduced belongs to: the desktop preview's keyboard, the command FIFO, tests.
    pub fn builtin() -> Controller {
        Controller { key: BUILTIN.into(), name: "Pocket".into(), gamepad: true, node: String::new() }
    }

    pub fn is_builtin(&self) -> bool {
        self.key == BUILTIN
    }
}

/// `BUS_USB` and `BUS_BLUETOOTH` of linux/input.h.
pub const BUS_USB: u16 = 0x03;
pub const BUS_BLUETOOTH: u16 = 0x05;

/// Which controller a device is part of, from what the kernel says about it: `uniq` (a Bluetooth controller's
/// address), `phys`, the bus and the device's place in sysfs (`/sys/class/input/eventN/device`, resolved).
/// - Bluetooth: its own controller, also when bluetoothd makes it through uhid (Bluetooth LE pads such as newer Xbox
///   controllers are virtual devices on the Bluetooth bus).
/// - USB and not virtual: a controller on the USB-C port.
/// - Everything else is the Pocket: the platform's keys and InputPlumber's virtual pad and keyboard (InputPlumber
///   presents the built-in pad as a virtual DualSense on the USB bus, through uhid, so the name is no help).
pub fn controller_key(bus: u16, vendor: u16, product: u16, uniq: &str, phys: &str, sysfs: &str) -> String {
    let uniq = uniq.trim().to_ascii_lowercase();
    let own = |kind: &str| {
        if uniq.is_empty() {
            format!("{kind}:{vendor:04x}:{product:04x}:{}", phys.trim())
        } else {
            format!("{kind}:{uniq}")
        }
    };
    if bus == BUS_BLUETOOTH {
        own("bt")
    } else if bus == BUS_USB && !sysfs.is_empty() && !sysfs.contains("/virtual/") {
        own("usb")
    } else {
        BUILTIN.into()
    }
}

/// What one device has down right now.
struct Source {
    controller: Controller,
    /// When it was connected, for the order of `Mux::controllers`.
    seq: u64,
    buttons: u32,
    axes: [f32; 6],
}

/// Several devices as one pad (review SOL-09): every device's buttons and sticks are kept apart, and what goes on is
/// the state of all of them together. A button is down while any device holds it, so a second controller pressing and
/// letting go of a button the first one holds changes nothing; a stick or trigger is the one pushed furthest. A device
/// that goes away (`Disconnected`) lets go of only what it held itself.
///
/// pocket-ui feeds every pad event through one (`PadState::merge`); pocket-core through `players::Inputs`, which also
/// reads each controller on its own for its player.
#[derive(Default)]
pub struct Mux {
    sources: std::collections::BTreeMap<usize, Source>,
    /// The device whose press made a button go down: the release is reported under its number too.
    owner: [Option<usize>; Button::ALL.len()],
    next_seq: u64,
}

impl Mux {
    /// Takes one event of one device; returns what changed for all of them together: nothing (another device holds
    /// the button already), the event itself, or for a device that went away the releases it causes.
    /// `Connected` and `Disconnected` themselves are not passed on.
    pub fn feed(&mut self, ev: PadEvent) -> Vec<PadEvent> {
        let mut out = Vec::new();
        match ev {
            PadEvent::Connected { source, controller } => {
                // Source numbers are never reused; a second introduction only renames.
                let seq = self.next_seq;
                self.next_seq += 1;
                match self.sources.get_mut(&source) {
                    Some(s) => s.controller = controller,
                    None => {
                        self.sources.insert(source, Source { controller, seq, buttons: 0, axes: [0.0; 6] });
                    }
                }
            }
            PadEvent::Disconnected { source } => {
                if !self.sources.contains_key(&source) {
                    return out;
                }
                let buttons = self.buttons();
                let axes: Vec<f32> = Axis::ALL.iter().map(|a| self.axis(*a)).collect();
                self.sources.remove(&source);
                for b in Button::ALL {
                    if buttons & b.bit() != 0 && !self.is_pressed(b) {
                        let owner = self.owner[b.index()].take().unwrap_or(source);
                        out.push(PadEvent::Button { source: owner, button: b, pressed: false });
                    }
                }
                for (a, was) in Axis::ALL.iter().zip(axes) {
                    let now = self.axis(*a);
                    if now != was {
                        out.push(PadEvent::Axis { source, axis: *a, value: now });
                    }
                }
            }
            PadEvent::Button { source, button, pressed } => {
                let was = self.is_pressed(button);
                let s = self.source(source);
                if pressed {
                    s.buttons |= button.bit();
                } else {
                    s.buttons &= !button.bit();
                }
                let now = self.is_pressed(button);
                if now != was {
                    let at = button.index();
                    let source = if now { *self.owner[at].insert(source) } else { self.owner[at].take().unwrap_or(source) };
                    out.push(PadEvent::Button { source, button, pressed: now });
                }
            }
            PadEvent::Axis { source, axis, value } => {
                let was = self.axis(axis);
                self.source(source).axes[axis.index()] = value;
                let now = self.axis(axis);
                if now != was {
                    out.push(PadEvent::Axis { source, axis, value: now });
                }
            }
            PadEvent::Raw { .. } | PadEvent::Lid { .. } => out.push(ev),
        }
        out
    }

    fn source(&mut self, source: usize) -> &mut Source {
        let seq = self.next_seq;
        let s = self.sources.entry(source).or_insert_with(|| Source { controller: Controller::builtin(), seq, buttons: 0, axes: [0.0; 6] });
        if s.seq == seq {
            self.next_seq += 1;
        }
        s
    }

    /// Down on any device.
    pub fn is_pressed(&self, b: Button) -> bool {
        self.buttons() & b.bit() != 0
    }

    fn buttons(&self) -> u32 {
        self.sources.values().fold(0, |m, s| m | s.buttons)
    }

    /// The axis of the device that pushes it furthest.
    pub fn axis(&self, a: Axis) -> f32 {
        furthest(self.sources.values().map(|s| s.axes[a.index()]))
    }

    /// The buttons (`Button::bit`) one controller holds, all its devices together.
    pub fn buttons_of(&self, key: &str) -> u32 {
        self.sources.values().filter(|s| s.controller.key == key).fold(0, |m, s| m | s.buttons)
    }

    /// One controller's sticks and triggers, in `Axis::ALL` order.
    pub fn axes_of(&self, key: &str) -> [f32; 6] {
        std::array::from_fn(|i| furthest(self.sources.values().filter(|s| s.controller.key == key).map(|s| s.axes[i])))
    }

    /// The key of the controller a device belongs to (`controller_of` without the copy).
    pub fn key_of(&self, source: usize) -> &str {
        self.sources.get(&source).map_or(BUILTIN, |s| s.controller.key.as_str())
    }

    /// The controller a device belongs to; the Pocket for one that was never introduced.
    pub fn controller_of(&self, source: usize) -> Controller {
        self.sources.get(&source).map_or_else(Controller::builtin, |s| s.controller.clone())
    }

    /// The controllers connected now, each once, in the order they came (the first device of each counts).
    pub fn controllers(&self) -> Vec<Controller> {
        let mut by_seq: Vec<&Source> = self.sources.values().collect();
        by_seq.sort_by_key(|s| s.seq);
        let mut out: Vec<Controller> = Vec::new();
        for s in by_seq {
            match out.iter_mut().find(|c| c.key == s.controller.key) {
                // A controller is a gamepad when one of its devices is; its node is that device's (for rumble).
                Some(c) if s.controller.gamepad && !c.gamepad => *c = s.controller.clone(),
                Some(_) => {}
                None => out.push(s.controller.clone()),
            }
        }
        out
    }

    /// Forgets what is held (input that may have been missed: pocket-ui after a game, see `PadState::let_go`); the
    /// devices stay known.
    pub fn let_go(&mut self) {
        for s in self.sources.values_mut() {
            s.buttons = 0;
            s.axes = [0.0; 6];
        }
        self.owner = Default::default();
    }
}

/// The value furthest from rest (0); the first of equals.
fn furthest(values: impl Iterator<Item = f32>) -> f32 {
    values.fold(0.0, |best: f32, v| if v.abs() > best.abs() { v } else { best })
}

/// pocket-ui's pad: every device merged (`Mux`), the sticks with their dead zone, and the last events for hello-pad.
pub struct PadState {
    mux: Mux,
    pub left: (f32, f32),
    pub right: (f32, f32),
    pub l2: f32,
    pub r2: f32,
    pub sources: Vec<String>,
    pub log: VecDeque<(Instant, String)>,
    pub events: u64,
}

impl PadState {
    pub fn new(sources: Vec<String>) -> Self {
        Self {
            mux: Mux::default(),
            left: (0.0, 0.0),
            right: (0.0, 0.0),
            l2: 0.0,
            r2: 0.0,
            sources,
            log: VecDeque::new(),
            events: 0,
        }
    }

    /// Down on any controller.
    pub fn is_pressed(&self, b: Button) -> bool {
        self.mux.is_pressed(b)
    }

    /// The controllers connected now, in the order they came.
    pub fn controllers(&self) -> Vec<Controller> {
        self.mux.controllers()
    }

    pub fn controller_of(&self, source: usize) -> Controller {
        self.mux.controller_of(source)
    }

    /// Takes one event of one device and returns what changed for the pad as a whole (`Mux::feed`): what pocket-ui
    /// acts on. A second controller's press of a held button, and its release, come back as nothing.
    pub fn merge(&mut self, ev: PadEvent) -> Vec<PadEvent> {
        self.events += 1;
        let line = match &ev {
            PadEvent::Button { source, button, pressed } => Some(format!("{} {}  src {}", button.label(), if *pressed { "down" } else { "up" }, source)),
            PadEvent::Raw { source, text } => Some(format!("{}  src {}", text, source)),
            PadEvent::Connected { source, controller } => Some(format!("{} connected  src {}", controller.name, source)),
            PadEvent::Disconnected { source } => Some(format!("disconnected  src {}", source)),
            PadEvent::Lid { source, closed } => Some(format!("lid {}  src {}", if *closed { "closed" } else { "open" }, source)),
            PadEvent::Axis { .. } => None,
        };
        if let Some(line) = line {
            self.log.push_front((Instant::now(), line));
            self.log.truncate(9);
        }
        let out = self.mux.feed(ev);
        for e in &out {
            if let PadEvent::Axis { axis, value, .. } = *e {
                // The Mini V2 left stick rests around +0.12, so small values count as centered.
                let value = if matches!(axis, Axis::L2 | Axis::R2) || value.abs() >= STICK_DEADZONE { value } else { 0.0 };
                match axis {
                    Axis::LeftX => self.left.0 = value,
                    Axis::LeftY => self.left.1 = value,
                    Axis::RightX => self.right.0 = value,
                    Axis::RightY => self.right.1 = value,
                    Axis::L2 => self.l2 = value,
                    Axis::R2 => self.r2 = value,
                }
            }
        }
        out
    }

    /// `merge`, for callers that only keep the state.
    pub fn apply(&mut self, ev: &PadEvent) {
        self.merge(ev.clone());
    }

    /// Lets go of everything (pocket-ui after a game or an app: stick movements were thinned out while it ran, see
    /// `idle::StickThrottle`). The controllers stay known.
    pub fn let_go(&mut self) {
        self.mux.let_go();
        (self.left, self.right, self.l2, self.r2) = ((0.0, 0.0), (0.0, 0.0), 0.0, 0.0);
    }
}

/// Which buttons and sticks one device left "down", so they can be let go when it disappears.
#[derive(Default)]
pub struct Held {
    buttons: Vec<Button>,
    axes: Vec<(Axis, f32)>,
}

impl Held {
    pub fn observe(&mut self, ev: &PadEvent) {
        match *ev {
            PadEvent::Button { button, pressed, .. } => {
                self.buttons.retain(|b| *b != button);
                if pressed {
                    self.buttons.push(button);
                }
            }
            PadEvent::Axis { axis, value, .. } => {
                self.axes.retain(|(a, _)| *a != axis);
                if value != 0.0 {
                    self.axes.push((axis, value));
                }
            }
            PadEvent::Raw { .. } | PadEvent::Connected { .. } | PadEvent::Disconnected { .. } | PadEvent::Lid { .. } => {}
        }
    }

    /// Events that undo everything still held (button up, stick and trigger back to rest); empties the tracker.
    pub fn release(&mut self, source: usize) -> Vec<PadEvent> {
        let buttons = std::mem::take(&mut self.buttons).into_iter().map(|button| PadEvent::Button { source, button, pressed: false });
        let axes = std::mem::take(&mut self.axes).into_iter().map(|(axis, _)| PadEvent::Axis { source, axis, value: 0.0 });
        buttons.chain(axes).collect()
    }
}

/// Input that comes before the UI thread can take it. The pads are opened first thing in `main`, on a thread, before
/// the event loop exists (its proxy is what carries events to the UI thread): a press reaches the kernel's queue of a
/// device only once the device is open, and one that came before was lost. On the Mini V2 (2026-10-26) Home was on
/// the screen at about 190 ms after `systemctl start essway`, but a press at about 210 ms did nothing; the pads were
/// opened only after the event loop had been built. Until `attach` the events wait here in the order they came,
/// then go through the deliverer; from then on straight through, on the reader's thread as before.
pub struct Early<E> {
    state: std::sync::Mutex<EarlyState<E>>,
}

type Deliver<E> = Box<dyn Fn(E) -> bool + Send + Sync>;

enum EarlyState<E> {
    Waiting(Vec<E>),
    Attached(Deliver<E>),
}

impl<E> Default for Early<E> {
    fn default() -> Self {
        Early { state: std::sync::Mutex::new(EarlyState::Waiting(Vec::new())) }
    }
}

impl<E> Early<E> {
    /// Events kept before `attach`; later ones are dropped (a stuck event loop is not waited for with all memory).
    pub const LIMIT: usize = 4096;

    /// An event from a reader: kept until `attach`, else delivered. False when the deliverer says nobody listens.
    pub fn send(&self, e: E) -> bool {
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        match &mut *state {
            EarlyState::Waiting(kept) => {
                if kept.len() < Self::LIMIT {
                    kept.push(e);
                }
                true
            }
            EarlyState::Attached(deliver) => deliver(e),
        }
    }

    /// The UI thread can take events now: what waited goes first, in order (under the same lock, so an event that
    /// comes meanwhile waits for them), then everything else as it comes.
    pub fn attach(&self, deliver: impl Fn(E) -> bool + Send + Sync + 'static) {
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        if let EarlyState::Waiting(kept) = &mut *state {
            for e in kept.drain(..) {
                deliver(e);
            }
        }
        *state = EarlyState::Attached(Box::new(deliver));
    }

    /// How many events wait for `attach` (0 once attached).
    pub fn waiting(&self) -> usize {
        match &*self.state.lock().unwrap_or_else(|p| p.into_inner()) {
            EarlyState::Waiting(kept) => kept.len(),
            EarlyState::Attached(_) => 0,
        }
    }
}

/// Closes an input device on a thread of its own (`input-close`) and returns at once. Closing an evdev node waits
/// in the kernel for an RCU grace period, and for the driver to power the device down when it was its last reader:
/// on the Mini V2 (2026-10-26) 7 to 45 ms per node, about 110 ms for the six nodes the pad scan opens, looks at and
/// doesn't keep (the haptics, the DualSense's touchpad and jack, a volume key, the touch panel, the DP jack). Closed
/// in turn on the scanning thread, they kept InputPlumber's pad, whose node comes last, closed until about 220 ms after
/// the start, well after the first frame (about 60 ms): a press in between was lost. The touch screen's search
/// opens every node and keeps one, the same way. When the thread can't be started the device is closed here.
pub fn close_later(device: Box<dyn Send>) {
    static CLOSER: std::sync::OnceLock<Option<std::sync::mpsc::Sender<Box<dyn Send>>>> = std::sync::OnceLock::new();
    let closer = CLOSER.get_or_init(|| {
        let (tx, rx) = std::sync::mpsc::channel::<Box<dyn Send>>();
        std::thread::Builder::new().name("input-close".into()).spawn(move || rx.into_iter().for_each(drop)).ok().map(|_| tx)
    });
    if let Some(tx) = closer {
        if let Err(back) = tx.send(device) {
            drop(back.0);
        }
    }
}

/// Why a pump returned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PumpEnd {
    /// Reading failed: the device was unplugged, unpaired or destroyed.
    DeviceGone,
    /// `send` refused an event: the receiver is gone, there is no point in reading on.
    Closed,
}

/// One event node as the supervisor sees it: its path and an identity that changes when the node is
/// deleted and created again (rdev, inode, ctime).
pub type Node = (String, u64);

/// What probing one node found.
pub enum Probe<D> {
    /// A gamepad or a power/volume device: its display name and the open device.
    Wanted(String, D),
    /// Opened fine, but not a device we read (touchscreen, sensor); not probed again until the node is recreated.
    NotWanted,
    /// Could not be opened (just removed, no permission, dead node); tried again next time.
    Failed,
}

/// The `event*` nodes in `dir` (/dev/input), sorted, each with an identity that changes when the node is deleted and
/// created again (a pad that came back has another inode or ctime). A controller paired or reconnected over
/// Bluetooth is a new node here: the supervisor opens it on its next look (`Supervisor::tick`, once a second).
pub fn event_nodes(dir: &std::path::Path) -> Vec<Node> {
    use std::os::unix::fs::MetadataExt;
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut nodes: Vec<Node> = entries
        .filter_map(Result::ok)
        .filter(|e| e.file_name().to_string_lossy().starts_with("event"))
        .filter_map(|e| {
            let m = e.metadata().ok()?;
            let id = m.rdev() ^ m.ino().rotate_left(17) ^ (m.ctime() as u64).rotate_left(31) ^ (m.ctime_nsec() as u64).rotate_left(47);
            Some((e.path().to_string_lossy().into_owned(), id))
        })
        .collect();
    nodes.sort();
    nodes
}

/// Wakes the device supervisor when something changes in a directory (/dev/input): inotify, so a pad that comes back
/// (after a deep sleep, InputPlumber made its virtual pad again) is opened at once instead of at the next once-a-second
/// look.
#[cfg(target_os = "linux")]
pub struct DirWatch {
    fd: std::os::fd::OwnedFd,
}

#[cfg(target_os = "linux")]
impl DirWatch {
    /// None when inotify can't be had (then the caller looks once a second, as before).
    pub fn new(dir: &std::path::Path) -> Option<DirWatch> {
        use std::os::fd::FromRawFd;
        use std::os::unix::ffi::OsStrExt;
        // SAFETY: inotify_init1 takes flags only and returns a new descriptor or -1.
        let raw = unsafe { libc::inotify_init1(libc::IN_NONBLOCK | libc::IN_CLOEXEC) };
        if raw < 0 {
            return None;
        }
        // SAFETY: `raw` is a descriptor we own from here on.
        let fd = unsafe { std::os::fd::OwnedFd::from_raw_fd(raw) };
        let path = std::ffi::CString::new(dir.as_os_str().as_bytes()).ok()?;
        let mask = libc::IN_CREATE | libc::IN_DELETE | libc::IN_ATTRIB | libc::IN_MOVED_TO | libc::IN_MOVED_FROM;
        // SAFETY: a valid descriptor and a NUL-terminated path that lives through the call.
        if unsafe { libc::inotify_add_watch(std::os::fd::AsRawFd::as_raw_fd(&fd), path.as_ptr(), mask) } < 0 {
            return None;
        }
        Some(DirWatch { fd })
    }

    /// Returns when something in the directory changed, or after `timeout`. Changes that come in a burst (udev
    /// creates the node, then sets its owner and mode) are taken together: one look after the burst.
    pub fn wait(&self, timeout: std::time::Duration) -> bool {
        use std::os::fd::AsRawFd;
        let fd = self.fd.as_raw_fd();
        let poll = |ms: i32| {
            let mut p = libc::pollfd { fd, events: libc::POLLIN, revents: 0 };
            // SAFETY: one pollfd that lives through the call.
            unsafe { libc::poll(&mut p, 1, ms) > 0 && p.revents & libc::POLLIN != 0 }
        };
        let drain = || {
            let mut buf = [0u8; 4096];
            // SAFETY: reads into a buffer of the length given; the descriptor is non-blocking.
            while unsafe { libc::read(fd, buf.as_mut_ptr().cast(), buf.len()) } > 0 {}
        };
        if !poll(timeout.as_millis().min(i32::MAX as u128) as i32) {
            return false;
        }
        drain();
        // The rest of the burst, a few ms behind its first event (at most 200 ms of it: then it is looked at anyway).
        for _ in 0..10 {
            if !poll(20) {
                break;
            }
            drain();
        }
        true
    }
}

/// The device directory, fakeable in tests.
pub trait DeviceBus {
    type Dev;
    fn nodes(&mut self) -> Vec<Node>;
    fn open(&mut self, node: &str) -> Probe<Self::Dev>;
}

pub struct Started<D> {
    pub source: usize,
    pub node: String,
    pub name: String,
    pub dev: D,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Lost {
    pub source: usize,
    pub node: String,
    pub name: String,
}

pub struct Tick<D> {
    pub started: Vec<Started<D>>,
    pub lost: Vec<Lost>,
}

/// Decides which devices to read: new ones are opened, dead ones forgotten, nothing is opened twice.
#[derive(Default)]
pub struct Supervisor {
    /// Devices being read, by source number (never reused, so a late "ended" cannot hit a newer device).
    live: std::collections::BTreeMap<usize, LiveDev>,
    /// Nodes that opened fine but are not ours, with the identity they had.
    rejected: std::collections::HashMap<String, u64>,
    next_source: usize,
}

struct LiveDev {
    node: String,
    id: u64,
    name: String,
}

impl Supervisor {
    /// Looks at the device directory: forgets devices whose node is gone or was recreated, opens new ones.
    pub fn tick<B: DeviceBus>(&mut self, bus: &mut B) -> Tick<B::Dev> {
        let nodes = bus.nodes();
        let mut tick = Tick { started: Vec::new(), lost: Vec::new() };
        let gone: Vec<usize> = self.live.iter().filter(|(_, l)| !nodes.iter().any(|(n, id)| *n == l.node && *id == l.id)).map(|(s, _)| *s).collect();
        tick.lost.extend(gone.into_iter().filter_map(|s| self.ended(s)));
        self.rejected.retain(|n, id| nodes.iter().any(|(m, mid)| m == n && mid == id));
        for (node, id) in nodes {
            if self.live.values().any(|l| l.node == node) || self.rejected.get(&node) == Some(&id) {
                continue;
            }
            match bus.open(&node) {
                Probe::Wanted(name, dev) => {
                    let source = self.next_source;
                    self.next_source += 1;
                    self.live.insert(source, LiveDev { node: node.clone(), id, name: name.clone() });
                    tick.started.push(Started { source, node, name, dev });
                }
                Probe::NotWanted => {
                    self.rejected.insert(node, id);
                }
                Probe::Failed => {}
            }
        }
        tick
    }

    /// The pump of `source` ended (read error): forget it so its node can be opened again.
    /// `None` when it was already forgotten because its node vanished.
    pub fn ended(&mut self, source: usize) -> Option<Lost> {
        self.live.remove(&source).map(|l| Lost { source, node: l.node, name: l.name })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;
    use std::collections::HashMap;

    #[derive(Clone, Copy, PartialEq)]
    enum Kind {
        Pad,
        Touch,
        /// Node exists but opening fails (a dead uinput node that was not deleted).
        Dead,
    }

    #[derive(Default)]
    struct Fake {
        nodes: Vec<(String, u64, Kind)>,
        opens: HashMap<String, usize>,
    }

    impl Fake {
        fn put(&mut self, path: &str, id: u64, kind: Kind) {
            self.nodes.retain(|(p, _, _)| p != path);
            self.nodes.push((path.to_string(), id, kind));
        }
        fn remove(&mut self, path: &str) {
            self.nodes.retain(|(p, _, _)| p != path);
        }
        fn opens(&self, path: &str) -> usize {
            self.opens.get(path).copied().unwrap_or(0)
        }
    }

    impl DeviceBus for Fake {
        type Dev = u64;
        fn nodes(&mut self) -> Vec<Node> {
            self.nodes.iter().map(|(p, id, _)| (p.clone(), *id)).collect()
        }
        fn open(&mut self, node: &str) -> Probe<u64> {
            *self.opens.entry(node.to_string()).or_default() += 1;
            match self.nodes.iter().find(|(p, _, _)| p == node) {
                Some((_, id, Kind::Pad)) => Probe::Wanted(format!("pad {node}"), *id),
                Some((_, _, Kind::Touch)) => Probe::NotWanted,
                _ => Probe::Failed,
            }
        }
    }

    fn started(t: &Tick<u64>) -> Vec<(usize, &str)> {
        t.started.iter().map(|s| (s.source, s.node.as_str())).collect()
    }

    #[test]
    fn pad_appears_disappears_reappears() {
        let mut bus = Fake::default();
        let mut sup = Supervisor::default();
        bus.put("/dev/input/event3", 1, Kind::Pad);

        let t = sup.tick(&mut bus);
        assert_eq!(started(&t), [(0, "/dev/input/event3")]);

        // Still there: not opened a second time.
        assert!(sup.tick(&mut bus).started.is_empty());
        assert_eq!(bus.opens("/dev/input/event3"), 1);

        // Pad removed: the pump of source 0 dies, the node is gone.
        bus.remove("/dev/input/event3");
        let lost = sup.ended(0).expect("loss is reported");
        assert_eq!((lost.source, lost.node.as_str()), (0, "/dev/input/event3"));
        let t = sup.tick(&mut bus);
        assert!(t.started.is_empty() && t.lost.is_empty());

        // Created again, the kernel may even hand out the same eventN: a new source, opened once.
        bus.put("/dev/input/event3", 2, Kind::Pad);
        let t = sup.tick(&mut bus);
        assert_eq!(started(&t), [(1, "/dev/input/event3")]);
        assert!(sup.tick(&mut bus).started.is_empty());
        assert_eq!(bus.opens("/dev/input/event3"), 2);
    }

    #[test]
    fn node_that_outlives_its_device_is_retried_until_it_works() {
        // The virtual Mini V2 keeps the mknod'ed node after the uinput pad is gone, with the same identity.
        let mut bus = Fake::default();
        let mut sup = Supervisor::default();
        bus.put("/dev/input/event4", 7, Kind::Pad);
        assert_eq!(sup.tick(&mut bus).started.len(), 1);

        bus.put("/dev/input/event4", 7, Kind::Dead);
        assert!(sup.ended(0).is_some());
        for _ in 0..3 {
            assert!(sup.tick(&mut bus).started.is_empty());
        }
        assert!(bus.opens("/dev/input/event4") >= 4, "a failed open is retried every tick");

        bus.put("/dev/input/event4", 7, Kind::Pad);
        let t = sup.tick(&mut bus);
        assert_eq!(started(&t), [(1, "/dev/input/event4")]);
    }

    #[test]
    fn node_that_vanishes_before_its_pump_notices_is_reported_once() {
        let mut bus = Fake::default();
        let mut sup = Supervisor::default();
        bus.put("/dev/input/event5", 1, Kind::Pad);
        sup.tick(&mut bus);

        bus.remove("/dev/input/event5");
        let t = sup.tick(&mut bus);
        assert_eq!(t.lost.len(), 1);
        assert_eq!(t.lost[0].source, 0);
        // The pump of source 0 only now notices; nothing more to report.
        assert!(sup.ended(0).is_none());

        // The replacement under the same name must not be killed by the late news about the old one.
        bus.put("/dev/input/event5", 2, Kind::Pad);
        assert_eq!(sup.tick(&mut bus).started.len(), 1);
        assert!(sup.ended(0).is_none());
        assert!(sup.tick(&mut bus).started.is_empty());
        assert_eq!(bus.opens("/dev/input/event5"), 2);
    }

    #[test]
    fn node_reused_for_another_device_is_reopened() {
        let mut bus = Fake::default();
        let mut sup = Supervisor::default();
        bus.put("/dev/input/event6", 1, Kind::Pad);
        sup.tick(&mut bus);
        bus.put("/dev/input/event6", 2, Kind::Pad); // deleted and created between two looks
        let t = sup.tick(&mut bus);
        assert_eq!(t.lost.len(), 1);
        assert_eq!(started(&t), [(1, "/dev/input/event6")]);
    }

    #[test]
    fn devices_we_do_not_read_are_probed_once_per_node() {
        let mut bus = Fake::default();
        let mut sup = Supervisor::default();
        bus.put("/dev/input/event0", 1, Kind::Touch);
        bus.put("/dev/input/event1", 2, Kind::Pad);
        for _ in 0..5 {
            sup.tick(&mut bus);
        }
        assert_eq!(bus.opens("/dev/input/event0"), 1);
        assert_eq!(bus.opens("/dev/input/event1"), 1);

        // A new node under the same name is looked at again, and may now be a pad.
        bus.put("/dev/input/event0", 9, Kind::Pad);
        assert_eq!(started(&sup.tick(&mut bus)), [(1, "/dev/input/event0")]);
    }

    #[test]
    fn two_pads_at_once_get_two_sources_and_only_the_gone_one_is_lost() {
        let mut bus = Fake::default();
        let mut sup = Supervisor::default();
        bus.put("/dev/input/event2", 1, Kind::Pad);
        bus.put("/dev/input/event8", 2, Kind::Pad);
        let t = sup.tick(&mut bus);
        assert_eq!(t.started.len(), 2);
        let bt = t.started.iter().find(|s| s.node.ends_with("event8")).unwrap().source;
        bus.remove("/dev/input/event8");
        let lost = sup.ended(bt).unwrap();
        assert_eq!(lost.node, "/dev/input/event8");
        assert!(sup.tick(&mut bus).lost.is_empty());
        assert_eq!(bus.opens("/dev/input/event2"), 1);
    }

    #[test]
    fn a_controller_paired_while_running_is_opened_on_the_next_look() {
        // The built-in pad and the touch screen from the start; nothing else for a while.
        let mut bus = Fake::default();
        let mut sup = Supervisor::default();
        bus.put("/dev/input/event0", 1, Kind::Touch);
        bus.put("/dev/input/event3", 2, Kind::Pad);
        assert_eq!(started(&sup.tick(&mut bus)), [(0, "/dev/input/event3")]);
        for _ in 0..3 {
            assert!(sup.tick(&mut bus).started.is_empty());
        }
        // Paired over Bluetooth: a node that never existed before, plus the controller's motion sensors (not a pad).
        bus.put("/dev/input/event9", 3, Kind::Pad);
        bus.put("/dev/input/event10", 4, Kind::Touch);
        let t = sup.tick(&mut bus);
        assert_eq!(started(&t), [(1, "/dev/input/event9")]);
        assert!(t.lost.is_empty(), "the built-in pad keeps going");
        assert!(sup.tick(&mut bus).started.is_empty());
        assert_eq!((bus.opens("/dev/input/event9"), bus.opens("/dev/input/event10"), bus.opens("/dev/input/event3")), (1, 1, 1));
    }

    #[test]
    fn a_new_controller_that_cannot_be_opened_yet_is_tried_again() {
        // udev may not have given the new node its group yet: the first open fails, a later one works.
        let mut bus = Fake::default();
        let mut sup = Supervisor::default();
        bus.put("/dev/input/event11", 5, Kind::Dead);
        assert!(sup.tick(&mut bus).started.is_empty());
        bus.put("/dev/input/event11", 5, Kind::Pad);
        assert_eq!(started(&sup.tick(&mut bus)), [(0, "/dev/input/event11")]);
    }

    #[test]
    fn the_device_directory_shows_new_nodes_and_recreated_ones_as_new() {
        let dir = TestDir::new("pad-nodes");
        let path = |n: &str| dir.join(n).to_string_lossy().into_owned();
        std::fs::write(dir.join("event0"), b"").unwrap();
        std::fs::write(dir.join("mice"), b"").unwrap();
        let before = event_nodes(&dir);
        assert_eq!(before.iter().map(|(n, _)| n.clone()).collect::<Vec<_>>(), [path("event0")], "only event nodes");
        // A controller connects: a brand-new node.
        std::fs::write(dir.join("event12"), b"").unwrap();
        let after = event_nodes(&dir);
        assert_eq!(after.len(), 2);
        assert!(after.contains(&before[0]), "the old node keeps its identity");
        let new = after.iter().find(|(n, _)| *n == path("event12")).unwrap().clone();
        // It goes and comes back under the same name (sleep, reconnect): another identity, so it is opened again.
        std::fs::remove_file(dir.join("event12")).unwrap();
        std::fs::write(dir.join("keep"), b"").unwrap(); // takes the old inode number, if the file system reuses it
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(dir.join("event12"), b"").unwrap();
        let again = event_nodes(&dir).into_iter().find(|(n, _)| *n == path("event12")).unwrap();
        assert_ne!(again.1, new.1);
        assert!(event_nodes(&dir.join("missing")).is_empty());
    }

    /// Instant on: a pad whose node is made again after a deep sleep is looked at when the node appears, not at the
    /// next once-a-second look.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_new_device_node_wakes_the_supervisor_at_once() {
        let dir = TestDir::new("pad-watch");
        let watch = DirWatch::new(&dir).expect("inotify");
        assert!(!watch.wait(std::time::Duration::from_millis(30)), "nothing changed: the timeout");
        let d = dir.to_path_buf();
        let maker = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(30));
            std::fs::write(d.join("event9"), b"").unwrap();
        });
        let t0 = std::time::Instant::now();
        assert!(watch.wait(std::time::Duration::from_secs(5)));
        assert!(t0.elapsed() < std::time::Duration::from_millis(900), "{:?}", t0.elapsed());
        maker.join().unwrap();
        assert!(DirWatch::new(&dir.join("missing")).is_none());
    }

    fn press(source: usize, button: Button, pressed: bool) -> PadEvent {
        PadEvent::Button { source, button, pressed }
    }

    fn pad(key: &str, gamepad: bool) -> Controller {
        Controller { key: key.into(), name: format!("pad {key}"), gamepad, node: format!("/dev/input/{key}") }
    }

    /// What one device's reader gets: the kernel queues a device's events for a reader only from its open on.
    fn read_after_open(events: &[(u64, PadEvent)], opened_ms: u64) -> Vec<PadEvent> {
        events.iter().filter(|(t, _)| *t >= opened_ms).map(|(_, e)| e.clone()).collect()
    }

    /// What the shelf acts on: the presses that come out of the merged pad.
    fn presses_acted_on(events: Vec<PadEvent>) -> Vec<Button> {
        let mut p = PadState::new(vec![]);
        events.into_iter().flat_map(|e| p.merge(e)).filter_map(|e| match e {
            PadEvent::Button { button, pressed: true, .. } => Some(button),
            _ => None,
        }).collect()
    }

    #[test]
    fn a_press_before_the_pad_is_open_leaves_only_its_release_and_the_shelf_does_nothing() {
        // The Mini V2's cold start, 2026-10-26, ms after `systemctl start essway`: Home on the screen at 192, Down
        // pressed at 208 for 100 ms, again at 569. With the pads opened late, only the second press did anything.
        let input = [(208, press(9, Button::Down, true)), (308, press(9, Button::Down, false)), (569, press(9, Button::Down, true)), (669, press(9, Button::Down, false))];
        assert_eq!(presses_acted_on(read_after_open(&input, 250)), [Button::Down], "opened at 250 ms: the first press is lost");
        assert_eq!(presses_acted_on(read_after_open(&input, 150)), [Button::Down, Button::Down], "opened before the press: both");
    }

    #[test]
    fn events_before_the_ui_can_take_them_wait_and_come_in_order() {
        let early = Early::default();
        assert!(early.send(press(1, Button::Down, true)) && early.send(press(1, Button::Down, false)));
        assert_eq!(early.waiting(), 2);
        let got = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let sink = got.clone();
        early.attach(move |e: PadEvent| {
            sink.lock().unwrap().push(e);
            true
        });
        assert_eq!(early.waiting(), 0);
        assert!(early.send(press(1, Button::South, true)));
        assert_eq!(presses_acted_on(got.lock().unwrap().clone()), [Button::Down, Button::South]);
        // Nobody listens any more (the event loop is gone): the reader is told, as with the proxy.
        let closed = Early::default();
        closed.attach(|_: PadEvent| false);
        assert!(!closed.send(press(1, Button::Down, true)));
    }

    #[test]
    fn a_press_while_the_ui_thread_still_starts_is_acted_on_when_it_is_ready() {
        // Real threads, the start's order: the reader opens the pad first and gets a press (down at 20 ms, up at
        // 40 ms) while the UI thread is still building its event loop (until 80 ms); a second press comes after.
        use std::time::Duration;
        let early = std::sync::Arc::new(Early::default());
        let t0 = Instant::now();
        let at = move |ms: u64| std::thread::sleep(Duration::from_millis(ms).saturating_sub(t0.elapsed()));
        let reader = {
            let early = early.clone();
            std::thread::spawn(move || {
                for (ms, e) in [(20, press(3, Button::Down, true)), (40, press(3, Button::Down, false)), (120, press(3, Button::Down, true)), (140, press(3, Button::Down, false))] {
                    at(ms);
                    early.send(e);
                }
            })
        };
        at(80);
        assert_eq!(early.waiting(), 2, "the first press waited for the event loop");
        let (tx, rx) = std::sync::mpsc::channel();
        early.attach(move |e| tx.send(e).is_ok());
        reader.join().unwrap();
        let got: Vec<PadEvent> = rx.try_iter().collect();
        assert_eq!(got.len(), 4);
        assert_eq!(presses_acted_on(got), [Button::Down, Button::Down], "both presses, the early one first");
    }

    #[test]
    fn events_sent_while_the_waiting_ones_are_handed_over_keep_their_order() {
        let early = std::sync::Arc::new(Early::default());
        // The reader holds at 2000 until the hand-over starts, so the waiting ones stay under Early::LIMIT even on a
        // loaded machine (past it, early events are dropped by design), and it sends on while attach runs.
        let go = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let reader = {
            let (early, go) = (early.clone(), go.clone());
            std::thread::spawn(move || {
                for i in 0..20_000usize {
                    early.send(i);
                    if i == 2000 {
                        while !go.load(std::sync::atomic::Ordering::Acquire) {
                            std::thread::yield_now();
                        }
                    }
                }
            })
        };
        while early.waiting() < 1000 {
            std::thread::yield_now();
        }
        let (tx, rx) = std::sync::mpsc::channel();
        go.store(true, std::sync::atomic::Ordering::Release);
        early.attach(move |i| tx.send(i).is_ok());
        reader.join().unwrap();
        let got: Vec<usize> = rx.try_iter().collect();
        assert!(got.windows(2).all(|w| w[0] < w[1]), "in order");
        assert_eq!(got.first(), Some(&0));
        assert_eq!(got.last(), Some(&19_999), "nothing after the hand-over is lost");
    }

    /// A node the scan looks at and lets go of, closed as slowly as the Mini V2's kernel closes it.
    struct SlowClose(u64, std::sync::Arc<std::sync::atomic::AtomicUsize>);

    impl Drop for SlowClose {
        fn drop(&mut self) {
            std::thread::sleep(std::time::Duration::from_millis(self.0));
            self.1.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }
    }

    #[test]
    fn the_pad_is_open_before_the_first_frame_although_closing_the_other_nodes_takes_the_kernel_long() {
        // /dev/input of the Mini V2 in the scan's (name) order, 2026-10-26: the close of each node the scan doesn't
        // keep, in ms; InputPlumber's pad (event9) comes last. Closed on the scanning thread, it was open only at
        // about 220 ms after the start, the first frame at about 60 ms (`start.pads` of the speed guard).
        let nodes: [(&str, Option<u64>); 11] = [
            ("spmi_haptics", Some(37)),
            ("pm8941_pwrkey", None),
            ("DualSense Touchpad", Some(7)),
            ("DualSense Headset Jack", Some(24)),
            ("pm8941_resin", Some(7)),
            ("gpio-keys", None),
            ("generic ft5x06 (8d)", Some(15)),
            ("RetroidPocket Headset Jack", None),
            ("RetroidPocket DP0 Jack", Some(23)),
            ("InputPlumber Keyboard", None),
            ("DualSense Wireless Controller", None),
        ];
        let closed = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let t0 = Instant::now();
        let mut kept = Vec::new();
        for (name, close_ms) in nodes {
            match close_ms {
                Some(ms) => close_later(Box::new(SlowClose(ms, closed.clone()))),
                None => kept.push(name),
            }
        }
        let pad_open = t0.elapsed();
        assert_eq!(kept.last(), Some(&"DualSense Wireless Controller"));
        assert!(pad_open < std::time::Duration::from_millis(30), "the pad open {pad_open:?} after the scan began, the closes waited for");
        // They are all closed, in the background, in about the time they take.
        let wait = Instant::now();
        while closed.load(std::sync::atomic::Ordering::SeqCst) < 6 {
            assert!(wait.elapsed() < std::time::Duration::from_secs(5), "the nodes are closed in the end");
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }

    /// Both scans that open every node hand the ones they don't keep to `close_later` (the pads' and the touch
    /// screen's): a plain drop there is the 110-150 ms of closes on the scanning thread again.
    #[test]
    fn the_scans_let_go_of_the_nodes_they_dont_keep_on_the_closing_thread() {
        let pad = include_str!("pad.rs");
        let open = &pad[pad.rfind("fn open(&mut self, node: &str) -> Probe<Device>").expect("EvdevBus::open")..];
        let open = &open[..open.find("\n        }\n").expect("its end")];
        assert!(open.contains("close_later(") && !open.contains("Ok(_) => Probe::NotWanted"), "{open}");
        let touch = include_str!("touch.rs");
        let open = &touch[touch.find("pub fn open() -> Option<(String, Device)>").expect("touch::linux::open")..];
        let open = &open[..open.find("\n    }\n").expect("its end")];
        assert!(open.contains("close_later(") && !open.contains(".filter(|d|"), "{open}");
    }

    #[test]
    fn a_second_pad_letting_go_keeps_the_first_pads_button() {
        // Review SOL-09, its public API test: pad 1 holds South, pad 2 presses and lets go of South.
        let mut p = PadState::new(vec![]);
        for (source, pressed) in [(1, true), (2, true), (2, false)] {
            p.apply(&press(source, Button::South, pressed));
        }
        assert!(p.is_pressed(Button::South), "source 1 still holds South");
        p.apply(&press(1, Button::South, false));
        assert!(!p.is_pressed(Button::South));
    }

    #[test]
    fn a_pad_disconnecting_mid_press_lets_go_of_only_its_own_buttons() {
        let mut p = PadState::new(vec![]);
        p.apply(&PadEvent::Connected { source: 1, controller: Controller::builtin() });
        p.apply(&PadEvent::Connected { source: 2, controller: pad("bt:aa", true) });
        p.apply(&press(1, Button::South, true));
        p.apply(&press(2, Button::South, true));
        p.apply(&press(2, Button::Up, true));
        p.apply(&PadEvent::Axis { source: 1, axis: Axis::LeftX, value: 0.8 });
        p.apply(&PadEvent::Axis { source: 2, axis: Axis::LeftX, value: 0.0 });
        assert_eq!(p.left.0, 0.8, "a centred stick on pad 2 doesn't centre pad 1's");
        // Pad 2 vanishes without its releases (the pump's own releases come first normally; this is the worst case).
        let out = p.merge(PadEvent::Disconnected { source: 2 });
        assert!(p.is_pressed(Button::South), "pad 1 still holds South");
        assert!(!p.is_pressed(Button::Up), "pad 2's Up is let go");
        assert_eq!(p.left.0, 0.8, "pad 1's stick stays where it is");
        assert!(matches!(out.as_slice(), [PadEvent::Button { source: 2, button: Button::Up, pressed: false }]), "{out:?}");
        // And pad 1 going away too releases the rest.
        let out = p.merge(PadEvent::Disconnected { source: 1 });
        assert!(!p.is_pressed(Button::South) && p.left.0 == 0.0, "{out:?}");
        assert!(p.merge(PadEvent::Disconnected { source: 1 }).is_empty(), "gone once");
    }

    #[test]
    fn the_merged_pad_reports_only_changes_and_releases_under_the_pressing_source() {
        let mut m = Mux::default();
        assert_eq!(m.feed(press(1, Button::Down, true)).len(), 1);
        assert!(m.feed(press(2, Button::Down, true)).is_empty(), "already down");
        assert!(m.feed(press(1, Button::Down, false)).is_empty(), "pad 2 still holds it");
        let out = m.feed(press(2, Button::Down, false));
        assert!(matches!(out.as_slice(), [PadEvent::Button { source: 1, button: Button::Down, pressed: false }]), "under the source that pressed it: {out:?}");
        // Sticks: the one pushed furthest counts.
        assert_eq!(m.feed(PadEvent::Axis { source: 1, axis: Axis::RightY, value: -0.9 }).len(), 1);
        assert!(m.feed(PadEvent::Axis { source: 2, axis: Axis::RightY, value: 0.3 }).is_empty());
        let out = m.feed(PadEvent::Axis { source: 1, axis: Axis::RightY, value: 0.0 });
        assert!(matches!(out.as_slice(), [PadEvent::Axis { source: 1, axis: Axis::RightY, value }] if *value == 0.3), "{out:?}");
        assert!(m.feed(PadEvent::Connected { source: 3, controller: pad("bt:bb", true) }).is_empty(), "introductions are not input");
    }

    #[test]
    fn the_pockets_devices_are_one_controller_and_each_bluetooth_pad_its_own() {
        let mut m = Mux::default();
        let keys = |m: &Mux| m.controllers().iter().map(|c| c.key.clone()).collect::<Vec<_>>();
        m.feed(PadEvent::Connected { source: 0, controller: Controller { gamepad: false, ..Controller::builtin() } });
        m.feed(PadEvent::Connected { source: 1, controller: pad("bt:aa", true) });
        m.feed(PadEvent::Connected { source: 2, controller: Controller { node: "/dev/input/event9".into(), ..Controller::builtin() } });
        m.feed(PadEvent::Connected { source: 3, controller: pad("bt:cc", false) });
        assert_eq!(keys(&m), ["builtin", "bt:aa", "bt:cc"]);
        let builtin = &m.controllers()[0];
        assert!(builtin.gamepad && builtin.node == "/dev/input/event9", "the Pocket's pad, not its keys, for rumble");
        // Each controller's own buttons and sticks, all its devices together.
        m.feed(press(0, Button::VolUp, true));
        m.feed(press(2, Button::East, true));
        m.feed(press(1, Button::East, true));
        m.feed(PadEvent::Axis { source: 1, axis: Axis::LeftY, value: -1.0 });
        assert_eq!(m.buttons_of(BUILTIN), Button::VolUp.bit() | Button::East.bit());
        assert_eq!(m.buttons_of("bt:aa"), Button::East.bit());
        assert_eq!(m.axes_of("bt:aa")[1], -1.0);
        assert_eq!(m.axes_of(BUILTIN)[1], 0.0);
        m.feed(PadEvent::Disconnected { source: 1 });
        assert_eq!(keys(&m), ["builtin", "bt:cc"]);
        assert_eq!(m.controller_of(1), Controller::builtin(), "a source nobody knows is the Pocket");
        // Let go after a game: nothing held, everyone still known.
        m.let_go();
        assert_eq!(m.buttons_of(BUILTIN), 0);
        assert_eq!(keys(&m), ["builtin", "bt:cc"]);
    }

    #[test]
    fn controllers_are_told_apart_by_bus_address_and_place() {
        // A DualSense over classic Bluetooth (hid-playstation): its address is its name.
        assert_eq!(controller_key(BUS_BLUETOOTH, 0x054c, 0x0ce6, "3C:4D:BE:01:02:03", "a0:b1:c2:d3:e4:f5", "/sys/devices/platform/soc/x/bluetooth/hci0/hci0:256/0005:054C:0CE6.0003/input/input12"), "bt:3c:4d:be:01:02:03");
        // Bluetooth LE through bluetoothd's uhid: virtual in sysfs, still a controller of its own.
        assert_eq!(controller_key(BUS_BLUETOOTH, 0x045e, 0x0b13, "", "a0:b1:c2:d3:e4:f5", "/sys/devices/virtual/misc/uhid/0005:045E:0B13.0004/input/input20"), "bt:045e:0b13:a0:b1:c2:d3:e4:f5");
        // A pad on the USB-C port.
        assert_eq!(controller_key(BUS_USB, 0x045e, 0x028e, "", "usb-xhci-hcd.1.auto-1/input0", "/sys/devices/platform/soc/a600000.usb/xhci-hcd.1.auto/usb1/1-1/1-1:1.0/input/input21"), "usb:045e:028e:usb-xhci-hcd.1.auto-1/input0");
        // InputPlumber's virtual DualSense for the built-in pad (USB bus, uhid), its keyboard, the keys, the test pad.
        assert_eq!(controller_key(BUS_USB, 0x054c, 0x0ce6, "", "", "/sys/devices/virtual/misc/uhid/0003:054C:0CE6.0001/input/input9"), BUILTIN);
        assert_eq!(controller_key(0x06, 0, 0, "", "", "/sys/devices/virtual/input/input8"), BUILTIN);
        assert_eq!(controller_key(0x19, 0, 0, "", "gpio-keys/input0", "/sys/devices/platform/gpio-keys/input/input3"), BUILTIN);
        assert_eq!(controller_key(BUS_USB, 0x1234, 0x5678, "", "", "/sys/devices/virtual/input/input30"), BUILTIN);
        assert_eq!(controller_key(BUS_USB, 0x1234, 0x5678, "", "", ""), BUILTIN, "nothing known about it: the Pocket");
    }

    #[test]
    fn a_vanished_pad_lets_go_of_everything_it_held() {
        let mut state = PadState::new(vec![]);
        let mut held = Held::default();
        let feed = |state: &mut PadState, held: &mut Held, ev: PadEvent| {
            held.observe(&ev);
            state.apply(&ev);
        };
        feed(&mut state, &mut held, PadEvent::Button { source: 2, button: Button::South, pressed: true });
        feed(&mut state, &mut held, PadEvent::Button { source: 2, button: Button::Right, pressed: true });
        feed(&mut state, &mut held, PadEvent::Button { source: 2, button: Button::L1, pressed: true });
        feed(&mut state, &mut held, PadEvent::Button { source: 2, button: Button::L1, pressed: false });
        feed(&mut state, &mut held, PadEvent::Axis { source: 2, axis: Axis::LeftX, value: 0.9 });
        feed(&mut state, &mut held, PadEvent::Axis { source: 2, axis: Axis::R2, value: 1.0 });
        feed(&mut state, &mut held, PadEvent::Axis { source: 2, axis: Axis::RightY, value: 0.0 });
        assert!(state.is_pressed(Button::South) && state.left.0 > 0.8 && state.r2 == 1.0);

        let undo = held.release(2);
        assert_eq!(undo.len(), 4, "South, Right, LeftX and R2; not L1 (already up) or RightY (at rest): {undo:?}");
        for ev in &undo {
            state.apply(ev);
        }
        assert!(!state.is_pressed(Button::South) && !state.is_pressed(Button::Right) && !state.is_pressed(Button::L1));
        assert_eq!((state.left, state.right, state.l2, state.r2), ((0.0, 0.0), (0.0, 0.0), 0.0, 0.0));
        assert!(undo.iter().all(|e| matches!(e, PadEvent::Button { source: 2, pressed: false, .. } | PadEvent::Axis { source: 2, value: 0.0, .. })));

        assert!(held.release(2).is_empty(), "released once is enough");
    }
}

#[cfg(target_os = "linux")]
pub mod linux {
    //! Reads every evdev device that looks like a gamepad or carries power/volume keys.
    //! Devices are read without grabbing, so sway and logind still see them.

    use super::{Axis, Button, Controller, DeviceBus, Held, Node, PadEvent, Probe, PumpEnd, Supervisor};
    use evdev::{AbsoluteAxisCode, Device, EventSummary, InputEvent, KeyCode, SwitchCode};

    /// The one place that decides which input devices we read: gamepads, the power and volume keys, and a lid switch
    /// (the Flip 2's hall sensor), plus InputPlumber's keyboard aliases. Touch has its own reader.
    pub fn is_wanted(d: &Device) -> bool {
        d.supported_keys().is_some_and(|k| wants_keys(|c| k.contains(c), d.name().is_some_and(|n| n.contains("InputPlumber"))))
            || d.supported_switches().is_some_and(|s| s.contains(SwitchCode::SW_LID))
    }

    /// The key part of `is_wanted`, for a device with these keys. Vol- counts on its own: on the Mini V2 the two volume
    /// keys are two devices, Vol+ is `gpio-keys` and Vol- is `pm8941_resin` (that one has nothing else), so without it
    /// the OSD never saw Vol- from the real key (only what InputPlumber's keyboard echoes of it, now and then).
    pub(super) fn wants_keys(has: impl Fn(KeyCode) -> bool, inputplumber: bool) -> bool {
        has(KeyCode::BTN_SOUTH)
            || has(KeyCode::KEY_POWER)
            || has(KeyCode::KEY_VOLUMEUP)
            || has(KeyCode::KEY_VOLUMEDOWN)
            || (inputplumber && (has(KeyCode::KEY_F1) || has(KeyCode::KEY_HOME)))
    }

    /// A switch event as ours: `SW_LID` 1 is the lid closed, 0 opened; every other switch (headphones, a tablet mode)
    /// is not ours.
    pub fn switch(source: usize, ev: &InputEvent) -> Option<PadEvent> {
        match ev.destructure() {
            EventSummary::Switch(_, SwitchCode::SW_LID, value) => Some(PadEvent::Lid { source, closed: value != 0 }),
            _ => None,
        }
    }

    fn display_name(d: &Device, path: &std::path::Path) -> String {
        format!("{} ({})", d.name().unwrap_or("unknown"), path.display())
    }

    /// Which controller the device at `node` is part of (`super::controller_key`).
    pub fn identify(d: &Device, node: &str) -> Controller {
        let id = d.input_id();
        let event = std::path::Path::new(node).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let sysfs = std::fs::canonicalize(format!("/sys/class/input/{event}/device")).map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
        Controller {
            key: super::controller_key(id.bus_type().0, id.vendor(), id.product(), d.unique_name().unwrap_or(""), d.physical_path().unwrap_or(""), &sysfs),
            name: d.name().unwrap_or("Controller").trim().to_string(),
            gamepad: d.supported_keys().is_some_and(|k| k.contains(KeyCode::BTN_SOUTH)),
            node: node.to_string(),
        }
    }

    /// Every wanted device right now, once (hello-pad; the real programs use `supervise`).
    pub fn open() -> Vec<(String, Device)> {
        evdev::enumerate().filter(|(_, d)| is_wanted(d)).map(|(path, d)| (display_name(&d, &path), d)).collect()
    }

    /// /dev/input as the supervisor sees it.
    struct EvdevBus;

    impl DeviceBus for EvdevBus {
        type Dev = Device;

        fn nodes(&mut self) -> Vec<Node> {
            super::event_nodes(std::path::Path::new("/dev/input"))
        }

        fn open(&mut self, node: &str) -> Probe<Device> {
            match Device::open(node) {
                Ok(d) if is_wanted(&d) => Probe::Wanted(display_name(&d, std::path::Path::new(node)), d),
                // Not on this thread: each close takes the kernel 7-45 ms, and the pad may be the next node.
                Ok(d) => {
                    super::close_later(Box::new(d));
                    Probe::NotWanted
                }
                Err(_) => Probe::Failed,
            }
        }
    }

    /// Reads every wanted device and keeps doing so: devices that appear later (a Bluetooth pad paired or reconnected,
    /// the virtual pad after an InputPlumber restart, a deep sleep that restarted Bluetooth) are opened within a second,
    /// dead ones are dropped and let go of their buttons. Returns the names found now; `send` gets every event and
    /// returns false when nobody listens any more.
    pub fn supervise(send: impl Fn(PadEvent) -> bool + Send + Sync + 'static) -> Vec<String> {
        let send = std::sync::Arc::new(send);
        let (done_tx, done_rx) = std::sync::mpsc::channel::<(usize, PumpEnd)>();
        let mut sup = Supervisor::default();
        let mut bus = EvdevBus;
        let step = move |sup: &mut Supervisor, bus: &mut EvdevBus| -> Option<Vec<String>> {
            let mut closed = false;
            while let Ok((source, end)) = done_rx.try_recv() {
                closed |= end == PumpEnd::Closed;
                if let Some(l) = sup.ended(source) {
                    println!("pad: lost {} (source {})", l.name, l.source);
                }
            }
            if closed {
                return None;
            }
            let tick = sup.tick(bus);
            for l in &tick.lost {
                println!("pad: lost {} (source {})", l.name, l.source);
                crate::trace::step("pad: lost");
            }
            Some(
                tick.started
                    .into_iter()
                    .map(|s| {
                        // Which controller it is, before any of its events (`Mux`, `players`).
                        let controller = identify(&s.dev, &s.node);
                        println!("pad: source {} is {} ({})", s.source, controller.key, controller.name);
                        send(PadEvent::Connected { source: s.source, controller });
                        let (send, done_tx) = (send.clone(), done_tx.clone());
                        std::thread::spawn(move || {
                            let end = pump(s.source, s.dev, |e| send(e));
                            let _ = done_tx.send((s.source, end));
                        });
                        s.name
                    })
                    .collect(),
            )
        };
        // Watched before the first look, so a node that appears in between is not missed.
        let watch = super::DirWatch::new(std::path::Path::new("/dev/input"));
        let names = step(&mut sup, &mut bus).unwrap_or_default();
        for n in &names {
            println!("pad: found {n}");
        }
        std::thread::spawn(move || {
            while let Some(names) = {
                // A node created, removed or given its permissions (a pad re-enumerated after a deep sleep, InputPlumber's
                // virtual pad made again, a Bluetooth pad) is looked at at once; otherwise once a second as before (a
                // pump that ended, a node that could not be opened yet). It used to be up to a second after any change.
                match &watch {
                    Some(w) => {
                        w.wait(std::time::Duration::from_secs(1));
                    }
                    None => std::thread::sleep(std::time::Duration::from_secs(1)),
                }
                step(&mut sup, &mut bus)
            } {
                for n in names {
                    println!("pad: connected {n}");
                    crate::trace::step("pad: connected");
                }
            }
        });
        names
    }

    pub(super) fn button(code: KeyCode) -> Option<Button> {
        Some(match code {
            KeyCode::BTN_DPAD_UP => Button::Up,
            KeyCode::BTN_DPAD_DOWN => Button::Down,
            KeyCode::BTN_DPAD_LEFT => Button::Left,
            KeyCode::BTN_DPAD_RIGHT => Button::Right,
            KeyCode::BTN_SOUTH => Button::South,
            KeyCode::BTN_EAST => Button::East,
            KeyCode::BTN_NORTH => Button::North,
            KeyCode::BTN_WEST => Button::West,
            KeyCode::BTN_TL => Button::L1,
            KeyCode::BTN_TR => Button::R1,
            KeyCode::BTN_TL2 => Button::L2,
            KeyCode::BTN_TR2 => Button::R2,
            KeyCode::BTN_THUMBL => Button::L3,
            KeyCode::BTN_THUMBR => Button::R3,
            KeyCode::BTN_SELECT => Button::Select,
            KeyCode::BTN_START => Button::Start,
            KeyCode::BTN_MODE | KeyCode::KEY_HOME => Button::Mode,
            KeyCode::BTN_BACK => Button::Back,
            // InputPlumber's ROCKNIX profile turns the Retroid/AYN BACK button into F1 on its virtual keyboard.
            KeyCode::KEY_F1 => Button::Back,
            KeyCode::KEY_POWER => Button::Power,
            KeyCode::KEY_VOLUMEUP => Button::VolUp,
            KeyCode::KEY_VOLUMEDOWN => Button::VolDown,
            _ => return None,
        })
    }

    /// Blocks on one device and forwards its events; meant to run on its own thread. When the device goes away
    /// (or `send` says nobody listens), everything it still held is released first, so no button stays "pressed",
    /// and then `Disconnected` says it is gone.
    pub fn pump(source: usize, dev: Device, send: impl Fn(PadEvent) -> bool) -> PumpEnd {
        let held = std::cell::RefCell::new(Held::default());
        let tracked = |e: PadEvent| {
            held.borrow_mut().observe(&e);
            send(e)
        };
        let end = read_loop(source, dev, &tracked);
        let undo = held.borrow_mut().release(source);
        for e in undo {
            send(e);
        }
        send(PadEvent::Disconnected { source });
        end
    }

    fn read_loop(source: usize, mut dev: Device, send: &impl Fn(PadEvent) -> bool) -> PumpEnd {
        let ranges: Vec<(AbsoluteAxisCode, i32, i32)> = dev
            .get_absinfo()
            .map(|it| it.map(|(code, info)| (code, info.minimum(), info.maximum())).collect())
            .unwrap_or_default();
        let norm = |code: AbsoluteAxisCode, v: i32| -> f32 {
            match ranges.iter().find(|(c, _, _)| *c == code) {
                Some((_, lo, hi)) if hi > lo => (v - lo) as f32 / (hi - lo) as f32,
                _ => 0.0,
            }
        };
        loop {
            let Ok(events) = dev.fetch_events() else { return PumpEnd::DeviceGone };
            let batch: Vec<_> = events.collect();
            for ev in batch {
                // The power key's own time stamp, the kernel's (evdev's clock is CLOCK_REALTIME unless asked otherwise):
                // after a wake by the key, how long the resume took before anything of ours ran (`sleep`'s log line).
                if let EventSummary::Key(_, KeyCode::KEY_POWER, 1) = ev.destructure() {
                    if let Ok(age) = std::time::SystemTime::now().duration_since(ev.timestamp()) {
                        if age < std::time::Duration::from_secs(60) {
                            crate::sleep::note_power_key(age);
                        }
                    }
                }
                let out = match ev.destructure() {
                    EventSummary::Key(_, code, value) if value != 2 => match button(code) {
                        Some(b) => PadEvent::Button { source, button: b, pressed: value == 1 },
                        None => PadEvent::Raw { source, text: format!("{:?} {}", code, value) },
                    },
                    EventSummary::AbsoluteAxis(_, code, value) => {
                        let centered = |v: i32| norm(code, v) * 2.0 - 1.0;
                        match code {
                            AbsoluteAxisCode::ABS_X => PadEvent::Axis { source, axis: Axis::LeftX, value: centered(value) },
                            AbsoluteAxisCode::ABS_Y => PadEvent::Axis { source, axis: Axis::LeftY, value: centered(value) },
                            AbsoluteAxisCode::ABS_RX => PadEvent::Axis { source, axis: Axis::RightX, value: centered(value) },
                            AbsoluteAxisCode::ABS_RY => PadEvent::Axis { source, axis: Axis::RightY, value: centered(value) },
                            AbsoluteAxisCode::ABS_Z => PadEvent::Axis { source, axis: Axis::L2, value: norm(code, value) },
                            AbsoluteAxisCode::ABS_RZ => PadEvent::Axis { source, axis: Axis::R2, value: norm(code, value) },
                            AbsoluteAxisCode::ABS_HAT0X => {
                                send(PadEvent::Button { source, button: Button::Left, pressed: value < 0 });
                                PadEvent::Button { source, button: Button::Right, pressed: value > 0 }
                            }
                            AbsoluteAxisCode::ABS_HAT0Y => {
                                send(PadEvent::Button { source, button: Button::Up, pressed: value < 0 });
                                PadEvent::Button { source, button: Button::Down, pressed: value > 0 }
                            }
                            _ => continue,
                        }
                    }
                    EventSummary::Switch(..) => match switch(source, &ev) {
                        Some(lid) => lid,
                        None => continue,
                    },
                    _ => continue,
                };
                if !send(out) {
                    return PumpEnd::Closed;
                }
            }
        }
    }
}

#[cfg(all(test, target_os = "linux"))]
mod lid_tests {
    use super::linux::switch;
    use super::PadEvent;
    use evdev::{EventType, InputEvent, SwitchCode};

    /// The Flip 2's hall sensor as the kernel reports it (`gpio-keys`, `linux,code = <SW_LID>`, EV_SW): closed, a
    /// SYN_REPORT, opened. Other switches and the report are not ours.
    #[test]
    fn the_lid_switch_reads_as_closed_and_opened() {
        let stream = [
            InputEvent::new(EventType::SWITCH.0, SwitchCode::SW_LID.0, 1),
            InputEvent::new(EventType::SYNCHRONIZATION.0, 0, 0),
            InputEvent::new(EventType::SWITCH.0, SwitchCode::SW_HEADPHONE_INSERT.0, 1),
            InputEvent::new(EventType::SWITCH.0, SwitchCode::SW_LID.0, 0),
        ];
        let got: Vec<PadEvent> = stream.iter().filter_map(|e| switch(7, e)).collect();
        assert!(matches!(got[..], [PadEvent::Lid { source: 7, closed: true }, PadEvent::Lid { source: 7, closed: false }]), "{got:?}");
    }
}

#[cfg(all(test, target_os = "linux"))]
mod ayn_input_tests {
    use super::{linux, Button};
    use evdev::KeyCode;

    #[test]
    fn inputplumber_ayn_aliases_keep_home_back_start_and_stick_clicks_distinct() {
        for (code, want) in [
            (KeyCode::BTN_MODE, Button::Mode), (KeyCode::KEY_HOME, Button::Mode),
            (KeyCode::KEY_F1, Button::Back), (KeyCode::BTN_BACK, Button::Back),
            (KeyCode::BTN_SELECT, Button::Select), (KeyCode::BTN_START, Button::Start),
            (KeyCode::BTN_THUMBL, Button::L3), (KeyCode::BTN_THUMBR, Button::R3),
        ] {
            assert_eq!(linux::button(code), Some(want));
        }
        // Plain ds5 target currently drops the Edge-only paddles. Do not turn them into face/menu buttons.
        assert_eq!(linux::button(KeyCode::BTN_Z), None);
        assert_eq!(linux::button(KeyCode::BTN_C), None);
    }
}

#[cfg(all(test, target_os = "linux"))]
mod volume_key_device_tests {
    use super::linux::wants_keys;
    use evdev::KeyCode;

    /// The Mini V2 (2026-10-28, `/proc/bus/input/devices`): Vol+ is `gpio-keys`, Vol- is `pm8941_resin`, a device with
    /// that one key. Alex's "Vol- changes the volume and shows nothing": the resin node was not read.
    #[test]
    fn each_device_with_a_volume_key_is_read_also_when_it_has_only_vol_down() {
        let keys = |list: &'static [KeyCode]| move |c: KeyCode| list.contains(&c);
        assert!(wants_keys(keys(&[KeyCode::KEY_VOLUMEDOWN]), false), "pm8941_resin");
        assert!(wants_keys(keys(&[KeyCode::KEY_VOLUMEUP]), false), "gpio-keys");
        assert!(wants_keys(keys(&[KeyCode::KEY_POWER]), false), "pm8941_pwrkey");
        assert!(wants_keys(keys(&[KeyCode::BTN_SOUTH]), false), "a pad");
        assert!(!wants_keys(keys(&[KeyCode::KEY_A, KeyCode::KEY_MUTE]), false), "a keyboard");
        assert!(!wants_keys(keys(&[KeyCode::KEY_F1]), false), "F1 alone is not ours");
        assert!(wants_keys(keys(&[KeyCode::KEY_F1]), true), "InputPlumber's keyboard");
    }
}
