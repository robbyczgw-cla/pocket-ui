//! Vibration: game rumble for pocket-core (libretro `GET_RUMBLE_INTERFACE`) and the subtle tick of
//! pocket-ui's menus, both through the Mini V2's vibration motor with evdev force feedback.
//!
//! Which device: the Retroid Pocket Mini V2's device tree has a Qualcomm PMI8998 haptics block (ERM
//! motor, `status = "okay"`), and ROCKNIX's udev rules name its input device `pmi8998_haptics`. That
//! device is preferred; failing it, any other input device with force feedback (InputPlumber's virtual
//! DualSense, the virtual device's test pad) is used. `POCKET_RUMBLE_DEVICE=/dev/input/eventN` forces one.
//! The effect type follows what the device advertises: `FF_RUMBLE`, else sine `FF_PERIODIC`, else
//! `FF_CONSTANT`.
//!
//! Every player rumbles their own controller (couch co-op, `players`): libretro port N goes to `Target` N of
//! `set_targets`, the Pocket's motor for the built-in controls, a Bluetooth pad's own force feedback (its event node,
//! hid-playstation and xpad offer `FF_RUMBLE`) for that pad, nothing for a player without one. Before pocket-core
//! seated anyone, and in pocket-ui, port 0 is the motor as it always was.
//!
//! Nothing here blocks the emulation thread: the libretro callback only posts a message to a worker
//! thread, which owns the devices. Rumble is a lease of `LEASE_MS` that the worker renews while it is
//! on, so a crashed pocket-core cannot leave the motor running (and closing the device erases it anyway).

use std::ffi::{c_uint, c_void};
use std::sync::atomic::{AtomicBool, AtomicU16, AtomicU64, AtomicU8, Ordering};
use std::sync::mpsc::{channel, RecvTimeoutError, Sender};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

/// `RETRO_ENVIRONMENT_GET_RUMBLE_INTERFACE`.
const ENV_GET_RUMBLE_INTERFACE: c_uint = 23;

/// How strongly games shake the pad (pause menu > Options > Extras > Rumble).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Strength {
    Off,
    Low,
    Medium,
    High,
}

impl Strength {
    pub fn parse(s: &str) -> Strength {
        match s {
            "off" => Strength::Off,
            "low" => Strength::Low,
            "high" => Strength::High,
            _ => Strength::Medium,
        }
    }

    fn gain(self) -> f32 {
        match self {
            Strength::Off => 0.0,
            Strength::Low => 0.4,
            Strength::Medium => 0.7,
            Strength::High => 1.0,
        }
    }

    fn to_u8(self) -> u8 {
        self as u8
    }

    fn from_u8(v: u8) -> Strength {
        [Strength::Off, Strength::Low, Strength::Medium, Strength::High]
            .get(v as usize)
            .copied()
            .unwrap_or(Strength::Medium)
    }
}

/// What a request from the core becomes: its 0..65535 strength, scaled by the setting.
pub fn scaled(core_strength: u16, strength: Strength) -> u16 {
    (core_strength as f32 * strength.gain()).round().min(65535.0) as u16
}

// ---- state shared with the callbacks ------------------------------------------------------------------

/// Players that can rumble (libretro ports 0..3, `players::MAX`).
pub const PORTS: usize = 4;

static STRENGTH: AtomicU8 = AtomicU8::new(2);
#[allow(clippy::declare_interior_mutable_const)]
const ZERO: AtomicU16 = AtomicU16::new(0);
static STRONG: [AtomicU16; PORTS] = [ZERO; PORTS];
static WEAK: [AtomicU16; PORTS] = [ZERO; PORTS];
/// The level last sent to the worker, so a core that repeats itself every frame doesn't flood it.
static SENT: [AtomicU16; PORTS] = [ZERO; PORTS];
static MENU: AtomicBool = AtomicBool::new(false);
static LAST_TICK_MS: AtomicU64 = AtomicU64::new(0);

/// Where one player's rumble goes.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum Target {
    /// The player has no controller (or one without force feedback): nothing.
    #[default]
    None,
    /// The Pocket's vibration motor (`Haptics::open`).
    Motor,
    /// A controller's own event node.
    Node(String),
}

static TARGETS: Mutex<[Target; PORTS]> = Mutex::new([Target::Motor, Target::None, Target::None, Target::None]);

/// Which device each player's rumble goes to (`players::Inputs` after the seats changed). A rumble that is on moves
/// with its player.
pub fn set_targets(targets: [Target; PORTS]) {
    {
        let mut t = TARGETS.lock().unwrap_or_else(|e| e.into_inner());
        if *t == targets {
            return;
        }
        *t = targets;
    }
    if SENT.iter().any(|s| s.load(Ordering::Relaxed) > 0) {
        send(Cmd::Retarget);
    }
}

fn targets() -> [Target; PORTS] {
    TARGETS.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

enum Cmd {
    /// Rumble this player at this magnitude (already scaled) until told otherwise.
    Level { port: usize, magnitude: u16 },
    /// One short burst of the Pocket's motor.
    Pulse {
        ms: u16,
        magnitude: u16,
    },
    Stop,
    /// `set_targets` moved players to other devices.
    Retarget,
}

fn worker() -> &'static Mutex<Sender<Cmd>> {
    static W: OnceLock<Mutex<Sender<Cmd>>> = OnceLock::new();
    W.get_or_init(|| {
        let (tx, rx) = channel();
        std::thread::Builder::new().name("rumble".into()).spawn(move || run_worker(rx)).ok();
        Mutex::new(tx)
    })
}

fn send(cmd: Cmd) {
    let _ = worker().lock().unwrap_or_else(|e| e.into_inner()).send(cmd);
}

/// The strength setting of the game that runs (from its settings, `Extras` applies it).
pub fn set_strength(s: Strength) {
    STRENGTH.store(s.to_u8(), Ordering::Relaxed);
    if s == Strength::Off {
        stop();
    } else {
        // A rumble that is already on follows the new strength.
        for port in 0..PORTS {
            update_level(port);
        }
    }
}

pub fn strength() -> Strength {
    Strength::from_u8(STRENGTH.load(Ordering::Relaxed))
}

fn update_level(port: usize) {
    let raw = STRONG[port].load(Ordering::Relaxed).max(WEAK[port].load(Ordering::Relaxed));
    let level = scaled(raw, strength());
    if SENT[port].swap(level, Ordering::Relaxed) != level {
        send(Cmd::Level { port, magnitude: level });
    }
}

/// A core's request for player `port`: `effect` 0 is the strong motor, 1 the weak one; the Mini V2 has one motor
/// (and we drive a pad's two together), so the stronger of the two counts.
fn core_rumble(port: usize, effect: c_uint, strength: u16) {
    if port >= PORTS {
        return;
    }
    match effect {
        0 => STRONG[port].store(strength, Ordering::Relaxed),
        _ => WEAK[port].store(strength, Ordering::Relaxed),
    }
    update_level(port);
}

/// Every motor off now (menu opened, game ended).
pub fn stop() {
    let mut was_on = false;
    for port in 0..PORTS {
        STRONG[port].store(0, Ordering::Relaxed);
        WEAK[port].store(0, Ordering::Relaxed);
        was_on |= SENT[port].swap(0, Ordering::Relaxed) != 0;
    }
    if was_on {
        send(Cmd::Stop);
    }
}

// ---- libretro ------------------------------------------------------------------------------------------

#[repr(C)]
struct RumbleInterface {
    set_rumble_state: unsafe extern "C" fn(port: c_uint, effect: c_uint, strength: u16) -> bool,
}

unsafe extern "C" fn set_rumble_state(port: c_uint, effect: c_uint, strength: u16) -> bool {
    core_rumble(port as usize, effect, strength);
    true
}

/// Answers `GET_RUMBLE_INTERFACE`; None for every other command.
///
/// # Safety
/// `data` is the pointer the core passed to its environment callback.
pub unsafe fn environment(cmd: c_uint, data: *mut c_void) -> Option<bool> {
    if cmd != ENV_GET_RUMBLE_INTERFACE {
        return None;
    }
    if !data.is_null() {
        (*(data as *mut RumbleInterface)).set_rumble_state = set_rumble_state;
    }
    Some(true)
}

// ---- menu haptics --------------------------------------------------------------------------------------

/// A tick of the menus: this long, this strong (of 65535). Felt, not heard; tune on the device with
/// `POCKET_HAPTIC_TICK=<ms>,<strength>`.
const TICK_MS: u16 = 22;
const TICK_STRENGTH: u16 = 36000;
/// Menus repeat at about 14 Hz when a direction is held; ticks keep at least this much apart.
const TICK_GAP_MS: u64 = 55;

fn tick_params() -> (u16, u16) {
    static P: OnceLock<(u16, u16)> = OnceLock::new();
    *P.get_or_init(|| {
        std::env::var("POCKET_HAPTIC_TICK")
            .ok()
            .and_then(|v| {
                let (a, b) = v.split_once(',')?;
                Some((a.trim().parse().ok()?, b.trim().parse().ok()?))
            })
            .unwrap_or((TICK_MS, TICK_STRENGTH))
    })
}

fn haptics_file(data_dir: &std::path::Path) -> std::path::PathBuf {
    data_dir.join("haptics.json")
}

/// Reads the menu haptics switch (`<data>/haptics.json`, `{"menu": true}`); off when there is no file.
pub fn load_menu_haptics(data_dir: &std::path::Path) -> bool {
    let on = std::fs::read(haptics_file(data_dir))
        .ok()
        .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
        .and_then(|v| v["menu"].as_bool())
        .unwrap_or(false);
    MENU.store(on, Ordering::Relaxed);
    on
}

pub fn menu_haptics() -> bool {
    MENU.load(Ordering::Relaxed)
}

/// Switches the menu ticks and remembers the choice. A tick confirms "on".
pub fn set_menu_haptics(data_dir: &std::path::Path, on: bool) {
    MENU.store(on, Ordering::Relaxed);
    let _ = crate::fsutil::write_atomic(&haptics_file(data_dir), format!("{{\"menu\": {on}}}\n").as_bytes());
    if on {
        tick();
    }
}

/// The menu moved: a very short, soft buzz, if menu haptics are on.
pub fn tick() {
    if !menu_haptics() {
        return;
    }
    static START: OnceLock<Instant> = OnceLock::new();
    // Offset by a second so that 0 means "no tick yet".
    let now = START.get_or_init(Instant::now).elapsed().as_millis() as u64 + 1000;
    if now < LAST_TICK_MS.load(Ordering::Relaxed) + TICK_GAP_MS {
        return;
    }
    LAST_TICK_MS.store(now, Ordering::Relaxed);
    let (ms, magnitude) = tick_params();
    send(Cmd::Pulse { ms, magnitude });
}

// ---- the evdev side -------------------------------------------------------------------------------------

const EV_FF: u16 = 0x15;
const FF_RUMBLE: u16 = 0x50;
const FF_PERIODIC: u16 = 0x51;
const FF_CONSTANT: u16 = 0x52;
const FF_SINE: u16 = 0x5a;
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
const FF_MAX: usize = 0x7f;

/// A play request lasts this long; the worker renews it while the rumble is on.
const LEASE_MS: u16 = 1500;
const RENEW: Duration = Duration::from_millis(1000);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Rumble,
    Periodic,
    Constant,
}

/// `struct ff_effect` for 64-bit Linux, 48 bytes: `type`, `id`, `direction`, trigger, replay
/// (`length`, `delay`) and then the union at offset 16.
pub fn effect_bytes(kind: Kind, id: i16, magnitude: u16, ms: u16) -> [u8; 48] {
    let mut b = [0u8; 48];
    let put16 = |b: &mut [u8; 48], at: usize, v: u16| b[at..at + 2].copy_from_slice(&v.to_ne_bytes());
    put16(&mut b, 2, id as u16);
    put16(&mut b, 10, ms);
    let half = (magnitude >> 1).min(0x7fff);
    match kind {
        Kind::Rumble => {
            put16(&mut b, 0, FF_RUMBLE);
            // One motor: it is driven by the strong channel, the weak one follows it.
            put16(&mut b, 16, magnitude);
            put16(&mut b, 18, magnitude);
        }
        Kind::Periodic => {
            put16(&mut b, 0, FF_PERIODIC);
            put16(&mut b, 16, FF_SINE);
            put16(&mut b, 18, 40); // period in ms
            put16(&mut b, 20, half);
        }
        Kind::Constant => {
            put16(&mut b, 0, FF_CONSTANT);
            put16(&mut b, 16, half);
        }
    }
    b
}

/// `struct input_event` with an EV_FF type: play (`value` 1) or stop (0) effect `id`.
pub fn play_event(id: i16, value: i32) -> [u8; 24] {
    let mut b = [0u8; 24];
    b[16..18].copy_from_slice(&EV_FF.to_ne_bytes());
    b[18..20].copy_from_slice(&(id as u16).to_ne_bytes());
    b[20..24].copy_from_slice(&value.to_ne_bytes());
    b
}

#[cfg(target_os = "linux")]
mod dev {
    use super::*;
    use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};

    /// `_IOC(dir, 'E', nr, size)`.
    const fn eio(dir: u32, nr: u32, size: u32) -> libc::c_ulong {
        ((dir << 30) | (size << 16) | ((b'E' as u32) << 8) | nr) as libc::c_ulong
    }
    const EVIOCSFF: libc::c_ulong = eio(1, 0x80, 48);
    const EVIOCRMFF: libc::c_ulong = eio(1, 0x81, 4);
    const fn eviocgbit(ev: u32, len: u32) -> libc::c_ulong {
        eio(2, 0x20 + ev, len)
    }
    const fn eviocgname(len: u32) -> libc::c_ulong {
        eio(2, 0x06, len)
    }

    pub struct Haptics {
        fd: OwnedFd,
        pub kind: Kind,
        pub name: String,
        pub path: String,
        id: i16,
    }

    fn open_rw(path: &str) -> Option<OwnedFd> {
        let c = std::ffi::CString::new(path).ok()?;
        let fd = unsafe { libc::open(c.as_ptr(), libc::O_RDWR | libc::O_NONBLOCK | libc::O_CLOEXEC) };
        (fd >= 0).then(|| unsafe { OwnedFd::from_raw_fd(fd) })
    }

    fn probe(path: &str) -> Option<(OwnedFd, Kind, String)> {
        let fd = open_rw(path)?;
        let mut bits = [0u8; FF_MAX / 8 + 1];
        if unsafe { libc::ioctl(fd.as_raw_fd(), eviocgbit(EV_FF as u32, bits.len() as u32), bits.as_mut_ptr()) } < 0 {
            return None;
        }
        let has = |code: u16| bits[code as usize / 8] & (1 << (code % 8)) != 0;
        let kind = if has(FF_RUMBLE) {
            Kind::Rumble
        } else if has(FF_PERIODIC) && has(FF_SINE) {
            Kind::Periodic
        } else if has(FF_CONSTANT) {
            Kind::Constant
        } else {
            return None;
        };
        let mut name = [0u8; 128];
        let n = unsafe { libc::ioctl(fd.as_raw_fd(), eviocgname(name.len() as u32), name.as_mut_ptr()) };
        let name = if n > 0 {
            String::from_utf8_lossy(&name[..(n as usize - 1).min(name.len())]).into_owned()
        } else {
            String::new()
        };
        Some((fd, kind, name))
    }

    /// Rank of a device name: the haptics motor first, then pads.
    fn rank(name: &str) -> u8 {
        let n = name.to_ascii_lowercase();
        if n.contains("haptic") || n.contains("vibra") {
            0
        } else if n.contains("pad") || n.contains("controller") || n.contains("dualsense") || n.contains("gamepad") {
            1
        } else {
            2
        }
    }

    impl Haptics {
        pub fn open() -> Option<Haptics> {
            Self::open_excluding(&[])
        }

        /// The device at `path` (a player's controller), if it has force feedback.
        pub fn open_at(path: &str) -> Option<Haptics> {
            probe(path).map(|(fd, kind, name)| Haptics { fd, kind, name, path: path.to_string(), id: -1 })
        }

        /// The Pocket's motor, not one of `exclude` (the controllers other players rumble).
        pub fn open_excluding(exclude: &[String]) -> Option<Haptics> {
            let mut paths: Vec<String> = match std::env::var("POCKET_RUMBLE_DEVICE") {
                Ok(p) if !p.is_empty() => vec![p],
                _ => std::fs::read_dir("/dev/input")
                    .ok()?
                    .flatten()
                    .map(|e| e.path().to_string_lossy().into_owned())
                    .filter(|p| p.contains("/event") && !exclude.contains(p))
                    .collect(),
            };
            paths.sort();
            let mut best: Option<(u8, String, OwnedFd, Kind, String)> = None;
            for p in paths {
                if let Some((fd, kind, name)) = probe(&p) {
                    let r = rank(&name);
                    if best.as_ref().is_none_or(|b| r < b.0) {
                        best = Some((r, p, fd, kind, name));
                    }
                }
            }
            best.map(|(_, path, fd, kind, name)| Haptics { fd, kind, name, path, id: -1 })
        }

        /// Uploads the effect (updating the one already there) and starts it.
        pub fn play(&mut self, magnitude: u16, ms: u16) -> bool {
            let mut eff = effect_bytes(self.kind, self.id, magnitude, ms);
            if unsafe { libc::ioctl(self.fd.as_raw_fd(), EVIOCSFF, eff.as_mut_ptr()) } < 0 {
                // The driver may have lost it (device reset): ask for a new one next time.
                self.id = -1;
                return false;
            }
            self.id = i16::from_ne_bytes([eff[2], eff[3]]);
            self.write(1)
        }

        pub fn stop(&mut self) {
            if self.id >= 0 {
                self.write(0);
            }
        }

        fn write(&self, value: i32) -> bool {
            let ev = play_event(self.id, value);
            unsafe { libc::write(self.fd.as_raw_fd(), ev.as_ptr() as *const c_void, ev.len()) == ev.len() as isize }
        }
    }

    impl Drop for Haptics {
        fn drop(&mut self) {
            if self.id >= 0 {
                let id = self.id as libc::c_int;
                unsafe { libc::ioctl(self.fd.as_raw_fd(), EVIOCRMFF, id as libc::c_ulong) };
            }
        }
    }
}

#[cfg(not(target_os = "linux"))]
mod dev {
    use super::*;
    pub struct Haptics {
        pub kind: Kind,
        pub name: String,
        pub path: String,
    }
    impl Haptics {
        pub fn open() -> Option<Haptics> {
            None
        }
        pub fn open_at(_: &str) -> Option<Haptics> {
            None
        }
        pub fn open_excluding(_: &[String]) -> Option<Haptics> {
            None
        }
        pub fn play(&mut self, _: u16, _: u16) -> bool {
            false
        }
        pub fn stop(&mut self) {}
    }
}

pub use dev::Haptics;

/// One device the worker has open (or looked for in vain), by what it is for.
struct Dev {
    target: Target,
    haptics: Option<Haptics>,
    tried: Instant,
}

/// The device of `target`, opened when first needed; a missing one is looked for again after a few seconds.
fn device<'a>(devs: &'a mut Vec<Dev>, target: &Target) -> Option<&'a mut Haptics> {
    if *target == Target::None {
        return None;
    }
    let at = match devs.iter().position(|d| d.target == *target) {
        Some(i) if devs[i].haptics.is_none() && devs[i].tried.elapsed() > Duration::from_secs(5) => {
            devs.remove(i);
            None
        }
        found => found,
    };
    let at = at.unwrap_or_else(|| {
        // The motor is never one of the controllers the other players have.
        let others: Vec<String> = targets().iter().filter_map(|t| if let Target::Node(n) = t { Some(n.clone()) } else { None }).collect();
        let haptics = match target {
            Target::Motor => Haptics::open_excluding(&others),
            Target::Node(path) => Haptics::open_at(path),
            Target::None => None,
        };
        match haptics.as_ref() {
            Some(h) => eprintln!("[rumble] {:?}: {} ({}), {:?}", target, h.name, h.path, h.kind),
            None => eprintln!("[rumble] {target:?}: no force-feedback device"),
        }
        devs.push(Dev { target: target.clone(), haptics, tried: Instant::now() });
        devs.len() - 1
    });
    devs[at].haptics.as_mut()
}

fn run_worker(rx: std::sync::mpsc::Receiver<Cmd>) {
    let mut devs: Vec<Dev> = Vec::new();
    // Each player's level, and the device it is playing on.
    let mut level = [0u16; PORTS];
    let mut on: [Target; PORTS] = Default::default();
    loop {
        let cmd = if level.iter().any(|l| *l > 0) {
            match rx.recv_timeout(RENEW) {
                Ok(c) => Some(c),
                Err(RecvTimeoutError::Timeout) => None,
                Err(RecvTimeoutError::Disconnected) => break,
            }
        } else {
            match rx.recv() {
                Ok(c) => Some(c),
                Err(_) => break,
            }
        };
        match cmd {
            Some(Cmd::Level { port, magnitude }) => {
                level[port] = magnitude;
                if magnitude == 0 {
                    if let Some(h) = device(&mut devs, &on[port]) {
                        h.stop();
                    }
                } else {
                    on[port] = targets()[port].clone();
                    if let Some(h) = device(&mut devs, &on[port]) {
                        h.play(magnitude, LEASE_MS);
                    }
                }
            }
            Some(Cmd::Pulse { ms, magnitude }) => {
                if let Some(h) = device(&mut devs, &Target::Motor) {
                    h.play(magnitude, ms);
                }
            }
            Some(Cmd::Stop) => {
                level = [0; PORTS];
                for d in devs.iter_mut() {
                    if let Some(h) = d.haptics.as_mut() {
                        h.stop();
                    }
                }
            }
            // Players moved to other controllers: what rumbles stops where it was and goes on where it is now; devices
            // nobody uses any more are closed (which also erases their effect).
            Some(Cmd::Retarget) => {
                let now = targets();
                for port in 0..PORTS {
                    if level[port] > 0 && on[port] != now[port] {
                        if let Some(h) = device(&mut devs, &on[port]) {
                            h.stop();
                        }
                        on[port] = now[port].clone();
                        if let Some(h) = device(&mut devs, &on[port]) {
                            h.play(level[port], LEASE_MS);
                        }
                    }
                }
                devs.retain(|d| d.target == Target::Motor || now.contains(&d.target));
            }
            // The lease is about to run out while a rumble is still on.
            None => {
                for port in 0..PORTS {
                    if level[port] > 0 {
                        if let Some(h) = device(&mut devs, &on[port]) {
                            h.play(level[port], LEASE_MS);
                        }
                    }
                }
            }
        }
    }
}

/// `pocket-core --rumble-test [ms]`: goes through the same path a core's rumble takes, so the motor can be
/// checked without a game. Reports the device it found. Returns the exit code.
pub fn selftest(args: &[String]) -> i32 {
    let hold: u64 = args.first().and_then(|a| a.parse().ok()).unwrap_or(700);
    let found = Haptics::open();
    match &found {
        Some(h) => println!("device {} ({}), effect {:?}", h.name, h.path, h.kind),
        None => {
            println!("no force-feedback device found");
            return 1;
        }
    }
    drop(found);
    let mut iface = RumbleInterface { set_rumble_state };
    unsafe { environment(ENV_GET_RUMBLE_INTERFACE, &mut iface as *mut RumbleInterface as *mut c_void) };
    let wait = |ms: u64| std::thread::sleep(Duration::from_millis(ms));
    for s in [Strength::Low, Strength::Medium, Strength::High] {
        set_strength(s);
        println!("core rumble 65535 at {s:?} for {hold} ms");
        unsafe { (iface.set_rumble_state)(0, 0, 65535) };
        wait(hold);
        unsafe { (iface.set_rumble_state)(0, 0, 0) };
        wait(500);
    }
    set_strength(Strength::Medium);
    println!("weak motor only, 30000");
    unsafe { (iface.set_rumble_state)(0, 1, 30000) };
    wait(hold);
    stop();
    wait(400);
    MENU.store(true, Ordering::Relaxed);
    println!("5 menu ticks");
    for _ in 0..5 {
        tick();
        wait(300);
    }
    wait(200);
    0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;

    #[test]
    fn strength_scales_the_cores_request() {
        assert_eq!(scaled(65535, Strength::Off), 0);
        assert_eq!(scaled(65535, Strength::High), 65535);
        assert!(scaled(65535, Strength::Low) < scaled(65535, Strength::Medium));
        assert!(scaled(65535, Strength::Medium) < scaled(65535, Strength::High));
        assert_eq!(scaled(0, Strength::High), 0);
        assert_eq!(Strength::parse("low"), Strength::Low);
        assert_eq!(Strength::parse("nonsense"), Strength::Medium);
    }

    #[test]
    fn rumble_effect_has_the_kernels_layout() {
        let b = effect_bytes(Kind::Rumble, -1, 0x1234, 1500);
        assert_eq!(u16::from_ne_bytes([b[0], b[1]]), 0x50);
        assert_eq!(i16::from_ne_bytes([b[2], b[3]]), -1);
        assert_eq!(u16::from_ne_bytes([b[10], b[11]]), 1500, "replay.length");
        assert_eq!(u16::from_ne_bytes([b[16], b[17]]), 0x1234, "strong");
        assert_eq!(u16::from_ne_bytes([b[18], b[19]]), 0x1234, "weak");
    }

    #[test]
    fn periodic_and_constant_use_signed_magnitudes() {
        let p = effect_bytes(Kind::Periodic, 3, 65535, 20);
        assert_eq!(u16::from_ne_bytes([p[0], p[1]]), 0x51);
        assert_eq!(u16::from_ne_bytes([p[16], p[17]]), 0x5a, "sine");
        assert_eq!(i16::from_ne_bytes([p[20], p[21]]), 32767);
        let c = effect_bytes(Kind::Constant, 0, 1000, 20);
        assert_eq!(u16::from_ne_bytes([c[0], c[1]]), 0x52);
        assert_eq!(i16::from_ne_bytes([c[16], c[17]]), 500);
    }

    #[test]
    fn play_event_is_an_input_event() {
        let e = play_event(5, 1);
        assert_eq!(u16::from_ne_bytes([e[16], e[17]]), 0x15);
        assert_eq!(u16::from_ne_bytes([e[18], e[19]]), 5);
        assert_eq!(i32::from_ne_bytes([e[20], e[21], e[22], e[23]]), 1);
    }

    #[test]
    fn libretro_interface_is_answered_only_for_its_command() {
        unsafe extern "C" fn refuse(_: c_uint, _: c_uint, _: u16) -> bool {
            false
        }
        let mut iface = RumbleInterface { set_rumble_state: refuse };
        unsafe {
            assert_eq!(environment(24, &mut iface as *mut _ as *mut c_void), None);
            assert_eq!(environment(23, &mut iface as *mut _ as *mut c_void), Some(true));
            assert!((iface.set_rumble_state)(1, 0, 0), "other players are accepted");
            assert!((iface.set_rumble_state)(9, 0, 65535), "and ports beyond the fourth ignored");
        }
    }

    #[test]
    fn menu_haptics_are_remembered() {
        let dir = TestDir::new("haptics");
        assert!(!load_menu_haptics(&dir), "off by default");
        MENU.store(false, Ordering::Relaxed);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(haptics_file(&dir), "{\"menu\": true}").unwrap();
        assert!(load_menu_haptics(&dir));
        std::fs::write(haptics_file(&dir), "junk").unwrap();
        assert!(!load_menu_haptics(&dir));
    }
}
