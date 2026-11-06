//! Scoped workaround for ROCKNIX 20261001's H616 UCM Speaker mixer assignment.
//! All helpers run on one worker. No mixer watchdog: unchanged wishes never overwrite an external ALSA edit.
//! See docs/H700-VOLUME.md for the dB curve, ownership, handover and hardware-test limits.

use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Provisional: the clean H700 listening reference so far is raw 44 (-22.04 dB).
/// Final calibration is pending the operator's loud-speaker A/B. This is the only ceiling knob.
pub const SPEAKER_MAX_RAW: u8 = 44;

const POLL: Duration = Duration::from_millis(500);
const RAMP_STEP: Duration = Duration::from_millis(20);
const ACTIVE_POLL: Duration = Duration::from_millis(100);
static REVISION: AtomicU64 = AtomicU64::new(0);
static REPAIR: AtomicU64 = AtomicU64::new(0);
static WAKE: Mutex<Option<Sender<Message>>> = Mutex::new(None);

/// A deliberate volume write. No helper I/O here.
pub fn key_activity() {
    notify();
}

pub fn changed() {
    REVISION.fetch_add(1, Ordering::Relaxed);
    notify();
}

/// Recheck after sleep hooks without treating the repair as permission to undo a hardware mute.
pub fn repair() {
    REPAIR.fetch_add(1, Ordering::Relaxed);
    notify();
}

fn notify() {
    if let Ok(wake) = WAKE.lock() {
        if let Some(tx) = wake.as_ref() {
            let _ = tx.send(Message::Wake);
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Control {
    Speaker,
    Dac,
    DacOn,
    Line,
    LineOn,
}
const CONTROLS: [Control; 5] = [Control::Speaker, Control::Dac, Control::DacOn, Control::Line, Control::LineOn];
type Raw = [u8; 2];
impl Control {
    fn index(self) -> usize {
        self as usize
    }
    fn name(self) -> &'static str {
        match self {
            Self::Speaker => "Speaker Switch",
            Self::Dac => "DAC Playback Volume",
            Self::DacOn => "DAC Playback Switch",
            Self::Line => "Line Out Playback Volume",
            Self::LineOn => "Line Out Playback Switch",
        }
    }
    fn channels(self) -> usize {
        if matches!(self, Self::DacOn | Self::LineOn) {
            2
        } else {
            1
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Mixer {
    values: [Raw; 5],
    jack: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Wish {
    sink: String,
    percent: u8,
    muted: bool,
}
impl Wish {
    fn internal(&self) -> bool {
        // Exact codec + UCM device: never treat an unknown, USB, HDMI or Bluetooth sink as the speaker.
        self.sink.starts_with("alsa_output.")
            && self.sink.contains("5096000.codec")
            && (self.sink.ends_with(".HiFi__Speaker__sink") || self.sink.ends_with(".HiFi__Headphones__sink"))
    }
    fn headphones(&self) -> bool {
        self.sink.ends_with(".HiFi__Headphones__sink")
    }
    fn silent(&self) -> bool {
        self.muted || self.percent == 0
    }
}

trait Alsa {
    fn read(&mut self) -> Option<Mixer>;
    fn write(&mut self, control: Control, value: Raw) -> bool;
}

/// DAC TLV: -73.08 dB + raw * 1.16 dB. Use ALSA's cubic UI mapping over the available finite dB range.
/// Raw zero is not mute: zero percent uses the switches, while nonzero values retain useful key steps.
#[cfg(test)]
fn speaker_dac(percent: u8) -> u8 {
    speaker_dac_at(percent, SPEAKER_MAX_RAW)
}

fn speaker_dac_at(percent: u8, max_raw: u8) -> u8 {
    if percent == 0 {
        return 0;
    }
    let ceiling = max_raw.clamp(1, SPEAKER_MAX_RAW) as f64;
    let floor = 10.0_f64.powf(-ceiling * 1.16 / 60.0);
    let normalized = percent.min(100) as f64 / 100.0 * (1.0 - floor) + floor;
    (ceiling + 60.0 * normalized.log10() / 1.16).floor().clamp(1.0, ceiling) as u8
}

#[derive(Default)]
struct Session {
    /// Downward-only diagnostic ceiling; never persisted as a player preference.
    max_raw: Option<u8>,
    original: Option<Mixer>,
    first_wish: Option<(u8, bool)>,
    first_sink: Option<String>,
    pending: bool,
    owned: [Option<Raw>; 5],
    last: Option<(Wish, bool, u64)>,
    /// An external mixer edit gives up ownership until the next deliberate wish.
    yielded: bool,
    force: bool,
    authorized_sound: bool,
    next_gain: Option<Instant>,
}
impl Session {
    fn dac_target(&self, percent: u8) -> u8 {
        speaker_dac_at(percent, self.max_raw.unwrap_or(SPEAKER_MAX_RAW))
    }
    fn put(&mut self, io: &mut impl Alsa, mixer: &mut Mixer, c: Control, value: Raw) -> bool {
        let i = c.index();
        if c == Control::Speaker && value == [1; 2] && mixer.jack {
            return false;
        }
        if mixer.values[i] == value {
            return true;
        }
        if !io.write(c, value) {
            eprintln!("h700 volume: could not write {}", c.name());
            return false;
        }
        // Read back before claiming ownership; failed/clamped writes must not be restored later.
        let Some(actual) = io.read() else { return false };
        *mixer = actual;
        if mixer.values[i] != value {
            return false;
        }
        self.owned[i] = Some(value);
        true
    }

    fn update_at(&mut self, io: &mut impl Alsa, wish: Wish, revision: u64, now: Instant) {
        let was_pending = std::mem::replace(&mut self.pending, true);
        let Some(mut mixer) = io.read() else { return };
        if !wish.internal() {
            // Keep the inactive codec quiet; never copy a Bluetooth/HDMI volume into its DAC.
            if self.original.is_some() && self.last.as_ref().is_some_and(|(w, _, _)| w.internal()) {
                let speaker = self.put(io, &mut mixer, Control::Speaker, [0; 2]);
                let dac = self.put(io, &mut mixer, Control::DacOn, [0; 2]);
                if !speaker || !dac {
                    return;
                }
            }
            self.last = Some((wish, mixer.jack, revision));
            self.force = false;
            self.pending = false;
            return;
        }
        let initial = self.original.is_none();
        if initial {
            self.original = Some(mixer.clone());
            self.first_wish = Some((wish.percent, wish.muted));
            self.first_sink = Some(wish.sink.clone());
        }
        let key = (wish.clone(), mixer.jack, revision);
        if self.last.as_ref() == Some(&key) && !self.force && !was_pending {
            self.pending = false;
            return;
        }
        let route_changed = self.last.as_ref().is_some_and(|(w, jack, _)| w.sink != wish.sink || *jack != mixer.jack);
        let mut ramping = false;
        let deliberate = self
            .last
            .as_ref()
            .is_some_and(|(w, _, r)| *r != revision || (!self.force && w.sink == wish.sink && (w.percent != wish.percent || w.muted != wish.muted)));
        // A raw amixer edit is not a request for us to turn anything back on. Route changes alone cannot take it back.
        let external = self.owned.iter().enumerate().any(|(i, v)| v.is_some_and(|v| v != mixer.values[i]));
        self.yielded |= external && !deliberate;
        if deliberate {
            self.yielded = false;
        }
        let headphones = mixer.jack || wish.headphones();
        let silent = wish.silent();
        if deliberate && !silent {
            self.authorized_sound = true;
        }
        let was_on = mixer.values[Control::Speaker.index()] == [1; 2];
        let dac_on = mixer.values[Control::DacOn.index()] == [1; 2];
        // Silence / jack insertion first, before changing any gain. Never enable a physically absent headphone route.
        if !silent && headphones && !self.put(io, &mut mixer, Control::Speaker, [0; 2]) {
            self.put(io, &mut mixer, Control::DacOn, [0; 2]);
            return;
        }
        if silent {
            // Try every independent way to silence even if a switch/helper fails.
            let mut ok = true;
            for c in [Control::Speaker, Control::DacOn, Control::Dac, Control::LineOn] {
                ok &= self.put(io, &mut mixer, c, [0; 2]);
            }
            if !ok {
                return;
            }
        } else if self.yielded {
            if route_changed
                && !headphones
                && mixer.values[Control::Dac.index()][0] > self.dac_target(wish.percent)
                && !self.put(io, &mut mixer, Control::Dac, [self.dac_target(wish.percent); 2])
            {
                return;
            }
        } else {
            let ours_off = |c: Control| {
                !initial
                    && self.owned[c.index()] == Some([0; 2])
                    && (self.authorized_sound || self.original.as_ref().is_some_and(|m| m.values[c.index()] == [1; 2]))
            };
            let restore_dac = ours_off(Control::DacOn);
            let restore_speaker = ours_off(Control::Speaker);
            let target = if headphones { 63 } else { self.dac_target(wish.percent) };
            // At attach we may only turn down; an existing hardware mute belongs to the user.
            let target = if initial { target.min(mixer.values[Control::Dac.index()][0]) } else { target };
            let current = mixer.values[Control::Dac.index()][0];
            let next = if initial || (route_changed && !headphones && current > target) {
                // Jack removal must lower the former headphone gain before enabling the speaker.
                target
            } else if target > current {
                current + 1
            } else if target < current {
                current - 1
            } else {
                target
            };
            ramping = next != target;
            if current != next && !initial && self.next_gain.is_some_and(|at| now < at) {
                self.last = Some(key);
                self.force = false;
                return; // pending: a later tick applies the latest target, never a queue of old key presses.
            }
            if !self.put(io, &mut mixer, Control::Dac, [next; 2]) {
                return;
            }
            if current != next {
                self.next_gain = Some(now + RAMP_STEP);
            }
            if (deliberate || restore_dac || (!initial && dac_on)) && !self.put(io, &mut mixer, Control::DacOn, [1; 2]) {
                return;
            }
            if !headphones && (deliberate || restore_speaker || (!initial && was_on)) && !self.put(io, &mut mixer, Control::Speaker, [1; 2]) {
                return;
            }
            // Line Out stays at PipeWire's attenuation. In particular never set it to full gain for a speaker.
            // Only undo our own mute when the user explicitly asks for sound.
            if (deliberate || restore_dac) && self.owned[Control::LineOn.index()] == Some([0; 2]) && !self.put(io, &mut mixer, Control::LineOn, [1; 2]) {
                return;
            }
        }
        self.last = Some(key);
        self.force = false;
        self.pending = ramping;
    }

    fn restore(&mut self, io: &mut impl Alsa) {
        let (Some(original), Some((wish, jack, _)), Some(now)) = (&self.original, &self.last, io.read()) else {
            return;
        };
        // No stale speaker route after jack/sink changes, no unmute after the player asked for silence.
        if self.pending || self.first_sink.as_ref() != Some(&wish.sink) || !wish.internal() || wish.silent() || now.jack != *jack || now.jack != original.jack {
            return;
        }
        // Restore the snapshot only if the whole owned group is still ours, and volume hasn't changed.
        // Returning a full-scale DAC after a volume-down would undo the player's wish on this broken UCM.
        if self.yielded || self.owned.iter().enumerate().any(|(i, v)| v.is_some_and(|v| v != now.values[i])) {
            return;
        }
        let Some((first_percent, first_mute)) = self.first_wish else { return };
        if wish.percent != first_percent || wish.muted != first_mute {
            return;
        }
        // Speaker is restored last. Jack checked again before every possible unmute.
        for c in [Control::DacOn, Control::LineOn, Control::Line, Control::Dac, Control::Speaker] {
            let i = c.index();
            if self.owned[i].is_none() {
                continue;
            }
            let Some(current) = io.read() else { return };
            if current.jack != original.jack || current.values[i] != self.owned[i].unwrap() {
                return;
            }
            if !io.write(c, original.values[i]) {
                return;
            }
        }
    }
}

/// Persistent ALSA control handle: a ramp step never forks amixer/pactl or touches a PCM stream.
mod native {
    use super::{Control, Mixer, Raw, CONTROLS};
    use libloading::Library;
    use std::ffi::{c_char, c_int, c_long, c_uint, c_void, CString};
    type Open = unsafe extern "C" fn(*mut *mut c_void, *const c_char, c_int) -> c_int;
    type Close = unsafe extern "C" fn(*mut c_void) -> c_int;
    type Malloc = unsafe extern "C" fn(*mut *mut c_void) -> c_int;
    type Free = unsafe extern "C" fn(*mut c_void);
    type Interface = unsafe extern "C" fn(*mut c_void, c_int);
    type Name = unsafe extern "C" fn(*mut c_void, *const c_char);
    type Io = unsafe extern "C" fn(*mut c_void, *mut c_void) -> c_int;
    type Integer = unsafe extern "C" fn(*const c_void, c_uint) -> c_long;
    type Boolean = unsafe extern "C" fn(*const c_void, c_uint) -> c_int;
    type Set = unsafe extern "C" fn(*mut c_void, c_uint, c_long);

    pub(super) struct Controls {
        _lib: Library,
        ctl: *mut c_void,
        value: *mut c_void,
        close: Close,
        free: Free,
        clear: Free,
        interface: Interface,
        name: Name,
        read: Io,
        write: Io,
        integer: Integer,
        boolean: Boolean,
        set_integer: Set,
        set_boolean: Set,
    }
    impl Controls {
        pub(super) fn open() -> Option<Self> {
            // SAFETY: exact ALSA control.h signatures; library retained until both ALSA objects are released.
            unsafe {
                let lib = Library::new("libasound.so.2").ok()?;
                let open = *lib.get::<Open>(b"snd_ctl_open").ok()?;
                let close = *lib.get::<Close>(b"snd_ctl_close").ok()?;
                let malloc = *lib.get::<Malloc>(b"snd_ctl_elem_value_malloc").ok()?;
                let free = *lib.get::<Free>(b"snd_ctl_elem_value_free").ok()?;
                let clear = *lib.get::<Free>(b"snd_ctl_elem_value_clear").ok()?;
                let interface = *lib.get::<Interface>(b"snd_ctl_elem_value_set_interface").ok()?;
                let name = *lib.get::<Name>(b"snd_ctl_elem_value_set_name").ok()?;
                let read = *lib.get::<Io>(b"snd_ctl_elem_read").ok()?;
                let write = *lib.get::<Io>(b"snd_ctl_elem_write").ok()?;
                let integer = *lib.get::<Integer>(b"snd_ctl_elem_value_get_integer").ok()?;
                let boolean = *lib.get::<Boolean>(b"snd_ctl_elem_value_get_boolean").ok()?;
                let set_integer = *lib.get::<Set>(b"snd_ctl_elem_value_set_integer").ok()?;
                let set_boolean = *lib.get::<Set>(b"snd_ctl_elem_value_set_boolean").ok()?;
                let mut ctl = std::ptr::null_mut();
                if open(&mut ctl, c"hw:Codec".as_ptr(), 0) < 0 || ctl.is_null() {
                    return None;
                }
                let mut value = std::ptr::null_mut();
                if malloc(&mut value) < 0 || value.is_null() {
                    close(ctl);
                    return None;
                }
                Some(Self {
                    _lib: lib,
                    ctl,
                    value,
                    close,
                    free,
                    clear,
                    interface,
                    name,
                    read,
                    write,
                    integer,
                    boolean,
                    set_integer,
                    set_boolean,
                })
            }
        }
        fn prepare(&self, name: &str, card: bool) {
            let name = CString::new(name).expect("constant ALSA name");
            // SAFETY: value belongs to this worker, clear resets numid/index before resolving the named control.
            unsafe {
                (self.clear)(self.value);
                (self.interface)(self.value, if card { 0 } else { 2 }); // CARD / MIXER, ALSA control.h.
                (self.name)(self.value, name.as_ptr());
            }
        }
        fn get(&self, name: &str, card: bool, channels: usize, integer: bool, max: u8) -> Option<Raw> {
            self.prepare(name, card);
            // SAFETY: control shape/ranges were validated before this handle was opened; indices are bounded.
            unsafe {
                if (self.read)(self.ctl, self.value) < 0 {
                    return None;
                }
                let mut result = [0; 2];
                for (channel, slot) in result.iter_mut().enumerate().take(channels) {
                    let raw = if integer {
                        (self.integer)(self.value, channel as c_uint)
                    } else {
                        (self.boolean)(self.value, channel as c_uint) as c_long
                    };
                    *slot = u8::try_from(raw).ok().filter(|v| *v <= max)?;
                }
                if channels == 1 {
                    result[1] = result[0];
                }
                Some(result)
            }
        }
        pub(super) fn read(&self) -> Option<Mixer> {
            let mut values = [[0; 2]; 5];
            for control in CONTROLS {
                let integer = matches!(control, Control::Dac | Control::Line);
                let max = match control {
                    Control::Dac => 63,
                    Control::Line => 31,
                    _ => 1,
                };
                values[control.index()] = self.get(control.name(), false, control.channels(), integer, max)?;
            }
            Some(Mixer {
                values,
                jack: self.get("Headphone Jack", true, 1, false, 1)?[0] != 0,
            })
        }
        pub(super) fn write(&self, control: Control, raw: Raw) -> bool {
            self.prepare(control.name(), false);
            let setter = if matches!(control, Control::Dac | Control::Line) {
                self.set_integer
            } else {
                self.set_boolean
            };
            // SAFETY: validated fixed control shape, value buffer/ctl stay alive for the call on this worker only.
            unsafe {
                for (channel, value) in raw.into_iter().enumerate().take(control.channels()) {
                    setter(self.value, channel as c_uint, value as c_long);
                }
                (self.write)(self.ctl, self.value) >= 0
            }
        }
    }
    impl Drop for Controls {
        fn drop(&mut self) {
            // SAFETY: the worker owns both live allocations; their library is dropped only after this destructor.
            unsafe {
                (self.free)(self.value);
                (self.close)(self.ctl);
            }
        }
    }
}

struct Real {
    native: native::Controls,
    unavailable: bool,
}
impl Real {
    fn open() -> Option<Self> {
        // Validate the known control ABI/TLV once; no subprocesses on the DAC ramp path.
        parse_mixer(&output("amixer", &["-c", "Codec", "contents"])?)?;
        Some(Self {
            native: native::Controls::open()?,
            unavailable: false,
        })
    }
}
fn output(program: &str, args: &[&str]) -> Option<String> {
    let out = crate::proc::run_with_timeout(Command::new(program).args(args).env("LC_ALL", "C"), crate::proc::HELPER).ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// cget/contents values are raw, not amixer's perceptual percentages; the jack has CARD rather than MIXER iface.
fn values(text: &str, name: &str, count: usize, max: u8) -> Option<Raw> {
    let header = format!("name='{name}'");
    let section = text.split("numid=").find(|s| s.lines().next().is_some_and(|l| l.ends_with(&header)))?;
    let raw = section.lines().find_map(|l| l.trim().strip_prefix(": values="))?;
    let parts: Vec<u8> = raw
        .split(',')
        .map(|v| match v.trim() {
            "off" => Some(0),
            "on" => Some(1),
            v => v.parse::<u8>().ok().filter(|v| *v <= max),
        })
        .collect::<Option<_>>()?;
    if parts.len() != count {
        return None;
    }
    Some([parts[0], *parts.get(1).unwrap_or(&parts[0])])
}
fn parse_mixer(text: &str) -> Option<Mixer> {
    // Refuse a different codec/control ABI instead of guessing at raw gain.
    let section = |name: &str| {
        text.split("numid=")
            .find(|s| s.lines().next().is_some_and(|l| l.ends_with(&format!("name='{name}'"))))
    };
    let dac = section("DAC Playback Volume")?;
    let line = section("Line Out Playback Volume")?;
    if !dac.contains("min=0,max=63") || !dac.contains("dBscale-min=-73.08dB,step=1.16dB") || !line.contains("min=0,max=31") {
        return None;
    }
    let mut raw = [[0; 2]; 5];
    for c in CONTROLS {
        let max = match c {
            Control::Dac => 63,
            Control::Line => 31,
            _ => 1,
        };
        raw[c.index()] = values(text, c.name(), c.channels(), max)?;
    }
    Some(Mixer {
        values: raw,
        jack: values(text, "Headphone Jack", 1, 1)?[0] != 0,
    })
}
impl Alsa for Real {
    fn read(&mut self) -> Option<Mixer> {
        let mixer = self.native.read();
        if mixer.is_none() && !self.unavailable {
            eprintln!("h700 volume: Codec controls unavailable; hardware volume/mute unverified");
        }
        self.unavailable = mixer.is_none();
        mixer
    }
    fn write(&mut self, control: Control, value: Raw) -> bool {
        self.native.write(control, value)
    }
}
fn wish() -> Option<Wish> {
    let sink = output("pactl", &["get-default-sink"])?;
    parse_wish(sink.trim(), &output("pactl", &["-f", "json", "list", "sinks"])?)
}
fn parse_wish(name: &str, json: &str) -> Option<Wish> {
    let sinks: serde_json::Value = serde_json::from_str(json).ok()?;
    let sink = sinks.as_array()?.iter().find(|s| s["name"].as_str() == Some(name))?;
    let percent = sink["volume"]
        .as_object()?
        .values()
        .map(|v| {
            v["value_percent"]
                .as_str()?
                .trim()
                .strip_suffix('%')?
                .trim()
                .parse::<u32>()
                .ok()
                .map(|v| v.min(100) as u8)
        })
        .collect::<Option<Vec<_>>>()?
        .into_iter()
        .max()?;
    Some(Wish {
        sink: name.into(),
        percent,
        muted: sink["mute"].as_bool()?,
    })
}
fn affected_ucm(text: &str) -> bool {
    let speaker = text.split("SectionDevice.\"Speaker\"").nth(1).and_then(|s| s.split("SectionDevice.").next());
    speaker
        .is_some_and(|s| s.contains("PlaybackMixerElem \"Line Out\"") && s.contains("name='DAC Playback Volume' 63") && s.contains("name='Speaker Switch' on"))
}

/// A lab may only lower the compiled speaker ceiling. Zero/invalid values do not enable a test mode.
fn diagnostic_ceiling(value: Option<&str>) -> Option<u8> {
    value?.parse::<u8>().ok().filter(|v| (1..=SPEAKER_MAX_RAW).contains(v))
}

enum Message {
    Start,
    Wake,
    Stop,
}
/// One UI lifetime, including games and standalones. Stop explicitly before process::exit.
pub struct Keeper {
    tx: Sender<Message>,
    worker: Option<std::thread::JoinHandle<()>>,
}
impl Keeper {
    pub fn start() -> Option<Self> {
        if std::env::var_os("POCKETUI_MOCK").is_some() || !crate::device::current().model.is_some_and(|m| m.is_h700()) {
            return None;
        }
        let (tx, rx) = mpsc::channel();
        let start = tx.clone();
        crate::startup::after_first_frame("h700-volume-start", move || {
            let _ = start.send(Message::Start);
        });
        let worker = std::thread::Builder::new()
            .name("h700-volume".into())
            .spawn(move || {
                loop {
                    match rx.recv() {
                        Ok(Message::Start) => break,
                        Ok(Message::Wake) => {}
                        _ => return,
                    }
                }
                let path = "/usr/share/alsa/ucm2/Allwinner/sun4i-h616/HiFi.conf";
                if !std::fs::read_to_string(path).ok().is_some_and(|s| affected_ucm(&s)) {
                    return;
                }
                let mut warned = false;
                let mut io = loop {
                    if let Some(io) = Real::open() {
                        break io;
                    }
                    if !warned {
                        eprintln!("h700 volume: waiting for validated Codec controls; hardware mute unverified");
                        warned = true;
                    }
                    match rx.recv_timeout(POLL) {
                        Ok(Message::Stop) | Err(RecvTimeoutError::Disconnected) => return,
                        _ => {}
                    }
                };
                let max_raw = diagnostic_ceiling(std::env::var("POCKETUI_H700_SPEAKER_MAX_RAW").ok().as_deref());
                let mut session = Session { max_raw, ..Session::default() };
                let mut repaired = REPAIR.load(Ordering::Relaxed);
                let mut cached = None;
                let mut next_poll = Instant::now();
                let mut last_poll = next_poll;
                let mut active_until = next_poll;
                let mut stop = false;
                while !stop {
                    let now = Instant::now();
                    let revision = REVISION.load(Ordering::Relaxed);
                    let repair = REPAIR.load(Ordering::Relaxed);
                    session.force |= repaired != repair;
                    repaired = repair;
                    if now >= next_poll {
                        cached = wish();
                        last_poll = now;
                        next_poll = now + if now < active_until { ACTIVE_POLL } else { POLL };
                    }
                    if let Some(wish) = cached.as_ref() {
                        session.update_at(&mut io, wish.clone(), revision, now);
                    }
                    let deadline = if session.pending {
                        next_poll.min(Instant::now() + RAMP_STEP)
                    } else {
                        next_poll
                    };
                    match rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
                        Ok(Message::Stop) | Err(RecvTimeoutError::Disconnected) => stop = true,
                        Ok(Message::Wake) => {
                            active_until = Instant::now() + Duration::from_secs(1);
                            next_poll = next_poll.min((last_poll + ACTIVE_POLL).max(Instant::now()));
                        }
                        _ => {}
                    }
                    // Coalesce held-key notifications; never queue old percentages or one helper per message.
                    for message in rx.try_iter() {
                        match message {
                            Message::Stop => stop = true,
                            Message::Wake => {
                                active_until = Instant::now() + Duration::from_secs(1);
                                next_poll = next_poll.min((last_poll + ACTIVE_POLL).max(Instant::now()));
                            }
                            Message::Start => {}
                        }
                    }
                }
                if let Some(wish) = wish() {
                    session.update_at(&mut io, wish, REVISION.load(Ordering::Relaxed), Instant::now());
                    session.restore(&mut io);
                }
            })
            .ok()?;
        *WAKE.lock().unwrap_or_else(|e| e.into_inner()) = Some(tx.clone());
        Some(Self { tx, worker: Some(worker) })
    }
}
impl Drop for Keeper {
    fn drop(&mut self) {
        WAKE.lock().unwrap_or_else(|e| e.into_inner()).take();
        let _ = self.tx.send(Message::Stop);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const SPEAKER: &str = "alsa_output._sys_devices_platform_soc_5096000.codec_sound_card0.HiFi__Speaker__sink";
    const HEADPHONES: &str = "alsa_output._sys_devices_platform_soc_5096000.codec_sound_card0.HiFi__Headphones__sink";
    const CAPTURE: &str = include_str!("../testdata/audio/h616-muted-amixer.txt");

    struct Fake {
        mixer: Mixer,
        writes: Vec<(Control, Raw)>,
        fail: Option<Control>,
        ignored: Option<Control>,
        readable: bool,
        now: Instant,
    }
    impl Fake {
        fn playing() -> Self {
            Self {
                mixer: Mixer {
                    values: [[1; 2], [63; 2], [1; 2], [20; 2], [1; 2]],
                    jack: false,
                },
                writes: vec![],
                fail: None,
                ignored: None,
                readable: true,
                now: Instant::now(),
            }
        }
        fn value(&self, c: Control) -> Raw {
            self.mixer.values[c.index()]
        }
        fn set(&mut self, c: Control, v: u8) {
            self.mixer.values[c.index()] = [v; 2];
        }
        fn audible(&self) -> bool {
            self.value(Control::Speaker) == [1; 2] && self.value(Control::DacOn).contains(&1)
        }
    }
    impl Alsa for Fake {
        fn read(&mut self) -> Option<Mixer> {
            self.readable.then(|| self.mixer.clone())
        }
        fn write(&mut self, c: Control, value: Raw) -> bool {
            self.writes.push((c, value));
            if self.fail == Some(c) {
                return false;
            }
            if self.ignored != Some(c) {
                self.mixer.values[c.index()] = value;
            }
            // The fake intentionally does NOT connect Line Out to Speaker: this reproduces the actual UCM bug.
            true
        }
    }
    fn apply(session: &mut Session, fake: &mut Fake, wish: Wish, revision: u64) {
        for _ in 0..=64 {
            fake.now += RAMP_STEP;
            let now = fake.now;
            session.update_at(fake, wish.clone(), revision, now);
            if !session.pending || fake.fail.is_some() || fake.ignored.is_some() || !fake.readable {
                break;
            }
        }
    }

    fn volume(p: u8) -> Wish {
        Wish {
            sink: SPEAKER.into(),
            percent: p,
            muted: false,
        }
    }

    #[test]
    fn silent_lab_ceiling_cannot_be_raised_by_volume_keys_or_exceed_the_production_limit() {
        assert_eq!(diagnostic_ceiling(Some("1")), Some(1));
        assert_eq!(diagnostic_ceiling(Some(&(SPEAKER_MAX_RAW + 1).to_string())), None);
        for invalid in ["0", "255", "invalid"] {
            assert_eq!(diagnostic_ceiling(Some(invalid)), None);
        }
        let (mut s, mut f) = (Session { max_raw: Some(1), ..Session::default() }, Fake::playing());
        for (revision, percent) in [35, 40, 55, 100, 5].into_iter().enumerate() {
            apply(&mut s, &mut f, volume(percent), revision as u64);
            assert_eq!(f.value(Control::Dac), [1; 2]);
            assert!(f.audible(), "enabled PCM path with silent digital source is a lab responsibility");
        }
    }

    #[test]
    fn volume_keys_and_quick_settings_reduce_the_actual_speaker_gain() {
        let (mut s, mut f) = (Session::default(), Fake::playing());
        apply(&mut s, &mut f, volume(100), 0);
        assert_eq!(f.value(Control::Dac), [SPEAKER_MAX_RAW; 2]); // Calibrated ceiling applies even at 100%.
        assert_eq!(f.value(Control::Line), [20; 2]); // PipeWire's attenuation is retained.
        let loud = f.value(Control::Dac)[0];
        apply(&mut s, &mut f, volume(50), 0); // ROCKNIX volume key: observed sink changed.
        assert!(f.value(Control::Dac)[0] < loud);
        let medium = f.value(Control::Dac)[0];
        apply(&mut s, &mut f, volume(10), 1); // Quick settings: explicit request.
        assert!(f.value(Control::Dac)[0] < medium);
        assert!(f.audible());
        let original = f.value(Control::Dac);
        s.restore(&mut f);
        assert_eq!(f.value(Control::Dac), original, "exit cannot undo volume-down");
    }

    #[test]
    fn zero_and_mute_really_silence_a_speaker_that_bypasses_line_out() {
        for muted in [false, true] {
            let (mut s, mut f) = (Session::default(), Fake::playing());
            let mut w = volume(if muted { 60 } else { 0 });
            w.muted = muted;
            apply(&mut s, &mut f, w, 0);
            assert!(!f.audible());
            assert_eq!(f.value(Control::Dac), [0; 2]);
            assert_eq!(f.value(Control::DacOn), [0; 2]);
            assert_eq!(f.value(Control::LineOn), [0; 2]);
            s.restore(&mut f);
            assert!(!f.audible(), "exit must stay silent");
            apply(&mut s, &mut f, volume(15), 1);
            assert!(f.audible(), "deliberate volume/unmute can restore sound");
            assert_eq!(f.value(Control::LineOn), [1; 2]);
            assert!(f.value(Control::Dac)[0] < SPEAKER_MAX_RAW);
        }
    }

    #[test]
    fn attaching_to_existing_hardware_mute_never_turns_it_on() {
        let (mut s, mut f) = (Session::default(), Fake::playing());
        f.set(Control::Speaker, 0);
        f.set(Control::DacOn, 0);
        f.set(Control::Dac, 0);
        apply(&mut s, &mut f, volume(60), 0);
        assert!(f.writes.is_empty());
        apply(&mut s, &mut f, volume(60), 0);
        assert!(!f.audible());
        apply(&mut s, &mut f, volume(65), 0);
        assert!(f.audible());
    }

    #[test]
    fn jack_insertion_silences_speaker_before_headphone_gain_and_unplug_caps_ucm_reset() {
        let (mut s, mut f) = (Session::default(), Fake::playing());
        apply(&mut s, &mut f, volume(40), 0);
        f.writes.clear();
        f.mixer.jack = true; // Physical jack arrives before PipeWire changes the default sink.
        apply(&mut s, &mut f, volume(40), 0);
        assert!(!f.audible());
        assert_eq!(f.writes[0], (Control::Speaker, [0; 2]));
        assert_eq!(f.value(Control::Dac), [63; 2]);
        assert_eq!(f.value(Control::Line), [20; 2]);
        let mut hp = volume(40);
        hp.sink = HEADPHONES.into();
        apply(&mut s, &mut f, hp.clone(), 0);
        // With no UCM reset, lower our headphone gain in one write BEFORE re-enabling the speaker.
        f.mixer.jack = false;
        f.writes.clear();
        f.now += Duration::from_secs(3);
        let now = f.now;
        s.update_at(&mut f, volume(40), 0, now);
        assert_eq!(f.writes[0], (Control::Dac, [speaker_dac(40); 2]));
        assert!(f.audible());
        f.mixer.jack = true;
        apply(&mut s, &mut f, hp.clone(), 0);
        // UCM reenables Speaker and resets DAC to 63 when unplugging. Limit it even though UCM changed ownership.
        f.mixer.jack = false;
        f.set(Control::Speaker, 1);
        f.set(Control::Dac, 63);
        apply(&mut s, &mut f, volume(40), 0);
        assert!(f.value(Control::Dac)[0] <= speaker_dac(40));
        assert!(f.audible());
    }

    #[test]
    fn mute_survives_jack_route_changes_in_both_orders() {
        let (mut s, mut f) = (Session::default(), Fake::playing());
        apply(&mut s, &mut f, volume(0), 0);
        for (jack, sink) in [(false, HEADPHONES), (true, HEADPHONES), (false, HEADPHONES), (false, SPEAKER)] {
            f.mixer.jack = jack;
            f.set(Control::Dac, 63);
            f.set(Control::Speaker, 1);
            f.set(Control::DacOn, 1);
            let mut w = volume(0);
            w.sink = sink.into();
            apply(&mut s, &mut f, w, 0);
            assert!(!f.audible());
            assert_eq!(f.value(Control::DacOn), [0; 2]);
        }
    }

    #[test]
    fn external_edits_are_left_alone_until_a_new_volume_request() {
        let (mut s, mut f) = (Session::default(), Fake::playing());
        apply(&mut s, &mut f, volume(70), 0);
        f.set(Control::Dac, 4);
        f.set(Control::Speaker, 0);
        f.writes.clear();
        for _ in 0..10 {
            apply(&mut s, &mut f, volume(70), 0);
        }
        s.restore(&mut f);
        assert!(f.writes.is_empty(), "no watchdog or stale restore against the user's amixer");
        apply(&mut s, &mut f, volume(75), 0);
        assert!(f.audible());
        assert_eq!(f.value(Control::Dac), [speaker_dac(75); 2]);
    }

    #[test]
    fn other_outputs_never_borrow_their_volume_for_the_codec() {
        for sink in [
            "bluez_output.example.1",
            "alsa_output.usb-Test",
            "alsa_output.hdmi",
            "",
            "alsa_output.unknown.HiFi__Speaker__sink",
        ] {
            let (mut s, mut f) = (Session::default(), Fake::playing());
            let mut w = volume(100);
            w.sink = sink.into();
            apply(&mut s, &mut f, w.clone(), 0);
            assert!(f.writes.is_empty());
            apply(&mut s, &mut f, volume(20), 0);
            apply(&mut s, &mut f, w, 1);
            assert!(!f.audible());
            f.writes.clear();
            s.restore(&mut f);
            assert!(f.writes.is_empty());
        }
    }

    #[test]
    fn restore_is_conditional_and_restores_only_successful_writes() {
        let (mut s, mut f) = (Session::default(), Fake::playing());
        let original = f.mixer.clone();
        apply(&mut s, &mut f, volume(50), 0);
        s.restore(&mut f);
        assert_eq!(f.mixer, original);
        for ignored in [false, true] {
            let (mut s, mut f) = (Session::default(), Fake::playing());
            if ignored {
                f.ignored = Some(Control::Dac);
            } else {
                f.fail = Some(Control::Dac);
            }
            apply(&mut s, &mut f, volume(50), 0);
            f.writes.clear();
            s.restore(&mut f);
            assert!(f.writes.is_empty());
        }
    }

    #[test]
    fn unreadable_controls_never_guess_and_wake_reapplies_the_users_volume() {
        let (mut s, mut f) = (Session::default(), Fake::playing());
        f.readable = false;
        apply(&mut s, &mut f, volume(50), 0);
        assert!(f.writes.is_empty());
        f.readable = true;
        apply(&mut s, &mut f, volume(50), 0);
        f.set(Control::Dac, 63); // ROCKNIX wake hook.
        apply(&mut s, &mut f, volume(50), 1);
        assert_eq!(f.value(Control::Dac), [speaker_dac(50); 2]);
    }

    #[test]
    fn captured_amixer_data_is_parsed_without_percent_or_card_number_guesses() {
        let mixer = parse_mixer(CAPTURE).unwrap();
        assert!(!mixer.jack);
        assert_eq!(mixer.values, [[0; 2], [0; 2], [1; 2], [0; 2], [0; 2]]);
        for text in [
            CAPTURE.replace("max=63", "max=255"),
            CAPTURE.replace("step=1.16dB", "step=1.00dB"),
            CAPTURE.replace(": values=on,on", ": values=on"),
            CAPTURE.replace("name='Headphone Jack'", "name='Other Jack'"),
        ] {
            assert!(parse_mixer(&text).is_none());
        }
    }

    #[test]
    fn failed_speaker_switch_still_mutes_dac_and_failed_dac_switch_still_mutes_speaker() {
        for failed in [Control::Speaker, Control::DacOn, Control::Dac, Control::LineOn] {
            let (mut s, mut f) = (Session::default(), Fake::playing());
            f.fail = Some(failed);
            apply(&mut s, &mut f, volume(0), 0);
            assert!(!f.audible(), "one failed control must not skip other mute mechanisms: {failed:?}");
        }
    }

    #[test]
    fn headphone_unplug_and_return_from_bluetooth_restore_only_our_switches() {
        for external_mute in [false, true] {
            let (mut s, mut f) = (Session::default(), Fake::playing());
            apply(&mut s, &mut f, volume(40), 0);
            f.mixer.jack = true;
            apply(&mut s, &mut f, volume(40), 0);
            assert!(!f.audible());
            if external_mute {
                f.set(Control::DacOn, 0);
            }
            f.mixer.jack = false;
            apply(&mut s, &mut f, volume(40), 0);
            assert_eq!(f.audible(), !external_mute);
            if !external_mute {
                let mut bt = volume(40);
                bt.sink = "bluez_output.example.1".into();
                apply(&mut s, &mut f, bt, 0);
                assert!(!f.audible());
                apply(&mut s, &mut f, volume(40), 0);
                assert!(f.audible());
                assert_eq!(f.value(Control::Dac), [speaker_dac(40); 2]);
            }
        }
    }

    #[test]
    fn volume_curve_is_monotonic_and_each_key_step_changes_speaker_gain() {
        let mut before = speaker_dac(0);
        for percent in 1..=100 {
            let raw = speaker_dac(percent);
            assert!(raw >= before && raw <= SPEAKER_MAX_RAW);
            if percent % 5 == 0 && percent > 5 {
                assert!(raw > speaker_dac(percent - 5));
            }
            before = raw;
        }
    }

    #[test]
    fn wake_repair_keeps_external_hardware_mute_but_restores_our_temporary_mute() {
        let (mut s, mut f) = (Session::default(), Fake::playing());
        f.set(Control::DacOn, 0);
        f.set(Control::Speaker, 0);
        apply(&mut s, &mut f, volume(50), 0);
        s.force = true;
        apply(&mut s, &mut f, volume(50), 0);
        assert!(!f.audible(), "wake is not a volume-up request");
        let mut temporary_mute = volume(50);
        temporary_mute.muted = true;
        s.force = true;
        apply(&mut s, &mut f, temporary_mute, 0);
        s.force = true;
        apply(&mut s, &mut f, volume(50), 0);
        assert!(!f.audible(), "temporary sleep mute/unmute must preserve the initial hardware mute");
        apply(&mut s, &mut f, volume(55), 1);
        assert!(f.audible());
        let mut muted = volume(55);
        muted.muted = true;
        s.force = true;
        apply(&mut s, &mut f, muted, 1);
        assert!(!f.audible());
        s.force = true;
        apply(&mut s, &mut f, volume(55), 1);
        assert!(f.audible(), "restore a temporary mute after the player explicitly enabled sound");
    }

    #[test]
    fn exit_does_not_restore_a_speaker_after_sink_change_even_before_the_jack_event() {
        let (mut s, mut f) = (Session::default(), Fake::playing());
        apply(&mut s, &mut f, volume(40), 0);
        let mut hp = volume(40);
        hp.sink = HEADPHONES.into();
        apply(&mut s, &mut f, hp.clone(), 0);
        assert!(!f.audible());
        f.writes.clear();
        s.restore(&mut f);
        assert!(f.writes.is_empty());
        assert!(!f.audible());
    }

    #[test]
    fn exit_does_not_undo_a_partially_failed_final_mute() {
        let (mut s, mut f) = (Session::default(), Fake::playing());
        apply(&mut s, &mut f, volume(40), 0);
        f.fail = Some(Control::Dac);
        apply(&mut s, &mut f, volume(0), 1);
        assert!(!f.audible());
        f.writes.clear();
        s.restore(&mut f);
        assert!(f.writes.is_empty(), "failed final mute is not permission to restore the previous unmuted wish");
        assert!(!f.audible());
    }

    #[test]
    fn nonzero_changes_ramp_one_dac_step_without_mute_or_route_flicker() {
        let (mut s, mut f) = (Session::default(), Fake::playing());
        apply(&mut s, &mut f, volume(100), 0);
        f.writes.clear();
        let target = speaker_dac(20);
        let mut previous = f.value(Control::Dac)[0];
        for _ in 0..64 {
            f.now += RAMP_STEP;
            let now = f.now;
            s.update_at(&mut f, volume(20), 1, now);
            let raw = f.value(Control::Dac)[0];
            assert!(previous.abs_diff(raw) <= 1);
            assert!(f.audible());
            previous = raw;
            if !s.pending {
                break;
            }
        }
        assert_eq!(previous, target);
        assert!(
            f.writes.iter().all(|(control, _)| *control == Control::Dac),
            "ordinary steps touch one control only"
        );
    }

    #[test]
    fn repeated_notifications_are_rate_limited_and_new_targets_replace_old_targets() {
        let (mut s, mut f) = (Session::default(), Fake::playing());
        apply(&mut s, &mut f, volume(100), 0);
        f.now += RAMP_STEP;
        let now = f.now;
        s.update_at(&mut f, volume(20), 1, now);
        f.writes.clear();
        for millisecond in 1..20 {
            s.update_at(&mut f, volume(10), 2, now + Duration::from_millis(millisecond));
        }
        assert!(f.writes.is_empty(), "a repeat burst cannot bypass the DAC rate limit");
        apply(&mut s, &mut f, volume(80), 3);
        assert_eq!(
            f.value(Control::Dac),
            [speaker_dac(80); 2],
            "the newest wish wins, without replaying old key levels"
        );
    }

    #[test]
    fn mute_preempts_a_ramp_and_external_mixer_edits_cancel_it() {
        let (mut s, mut f) = (Session::default(), Fake::playing());
        apply(&mut s, &mut f, volume(100), 0);
        f.now += RAMP_STEP;
        let now = f.now;
        s.update_at(&mut f, volume(10), 1, now);
        assert!(s.pending);
        s.update_at(&mut f, volume(0), 2, now); // No waiting for the ramp timer.
        assert!(!f.audible());
        assert_eq!(f.value(Control::DacOn), [0; 2]);
        apply(&mut s, &mut f, volume(80), 3);
        f.now += RAMP_STEP;
        let now = f.now;
        s.update_at(&mut f, volume(10), 4, now);
        f.set(Control::Dac, 2);
        f.writes.clear();
        s.update_at(&mut f, volume(10), 4, now + RAMP_STEP);
        assert!(f.writes.is_empty(), "a user ALSA edit cancels our pending ramp");
        assert!(!s.pending);
    }

    #[test]
    fn json_volume_and_mute_are_read_from_the_named_sink_and_loudest_channel() {
        let json = r#"[{"name":"other","mute":false,"volume":{"front-left":{"value_percent":"100%"}}},{"name":"codec","mute":true,"volume":{"front-left":{"value_percent":"0%"},"front-right":{"value_percent":"35%"}}}]"#;
        assert_eq!(
            parse_wish("codec", json),
            Some(Wish {
                sink: "codec".into(),
                percent: 35,
                muted: true
            })
        );
        assert!(parse_wish("missing", json).is_none());
    }

    #[test]
    fn corrected_ucm_does_not_activate_the_workaround() {
        let broken = "SectionDevice.\"Speaker\" { PlaybackMixerElem \"Line Out\" cset \"name='DAC Playback Volume' 63\" cset \"name='Speaker Switch' on\" }";
        assert!(affected_ucm(broken));
        assert!(!affected_ucm(&broken.replace("PlaybackMixerElem \"Line Out\"", "PlaybackMixerElem \"DAC\"")));
    }
}
