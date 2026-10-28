//! Audio out to PipeWire, loaded with dlopen so nothing is linked at build time. Two ways in:
//! - ALSA's `pipewire` device (default): a native PipeWire stream. ROCKNIX's pulse layer keeps every
//!   pulse client at a quantum of at least 1024 frames and about 90 ms of buffering; this way skips it.
//! - PulseAudio's simple API, as before (POCKET_CORE_AUDIO=pulse, or when ALSA can't be opened).
//!
//! The core's samples are resampled to 48 kHz. The ratio is nudged by up to ±0.5 % depending on how
//! full the buffer is, averaged over about a second (dynamic rate control, as RetroArch does), so audio follows the
//! 60 Hz video without crackles or growing latency even though a Game Boy runs at 59.73 Hz.
//!
//! The ALSA buffer is sized for the graph's cycle (`Ring`): 10 ms at the speaker's 256, 32 ms at ROCKNIX's 1024 on
//! Bluetooth. scripts/audio-bench measures the whole path off the device (docs/HOST.md "Bluetooth audio").

use std::collections::VecDeque;
use std::ffi::{c_char, c_int, c_long, c_uint, c_ulong, c_void};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use libloading::Library;

pub const OUT_RATE: u32 = 48_000;
/// Buffered audio the rate control steers towards, in ms (POCKET_CORE_AUDIO_MS overrides it).
/// It only has to bridge the core's frame-sized bursts; the output keeps its own small buffer.
/// Measured on the Mini V2: through pulse (30 s each, mGBA and snes9x) 24 ms had no underruns,
/// 16 ms one for snes9x; on the native stream at quantum 256, 16, 20 and 24 ms had none in 60 s
/// of snes9x, and 16 ms once one. 20 ms keeps a margin.
///
/// Only while the graph runs at the small quantum this game set (`Ring::Short`); otherwise `CYCLE_TARGET_MS`.
const TARGET_MS: usize = 20;
/// The queue while the graph runs at ROCKNIX's 1024 (Bluetooth, or the speaker when the quantum couldn't be lowered):
/// the writer then takes a whole cycle (21 ms) out of it at once, so 20 ms ran dry. scripts/audio-bench: at 2048, a
/// 20 ms queue underran 4 times in 10 s; 48 ms holds a cycle plus a core frame with room to spare. On Bluetooth the
/// codec and the radio add 100-250 ms anyway (docs/HOST.md "Bluetooth audio").
const CYCLE_TARGET_MS: usize = 48;
/// The same for 3D and other heavy cores (`Audio::open`'s `heavy`): their frame times vary much more
/// (shader compiles, scene loads, threaded renderers). Mupen64Plus-Next, Mario Kart 64, 25 s driving:
/// 20 ms underran once per run and 15 times while a state loaded; 48 ms never, and twice.
const HEAVY_TARGET_MS: usize = 48;
const SERVER_MS: u32 = 20;
const MAX_SKEW: f64 = 0.005;
/// How much of each new fill reading the rate control takes (an average over about a second at 60 pushes a second).
/// The queue's fill saws up and down by a core frame (17 ms) and by a writer chunk or a whole graph cycle (5 or 21 ms)
/// at rates that beat against each other; steering on the raw reading turned that into a pitch wobble of ±2.4 cents
/// on the speaker and ±8 cents on Bluetooth (scripts/audio-bench, 1 kHz tone). The queue itself drifts over seconds
/// (the loop's time constant is target / (MAX_SKEW × 48000), 4 s at 20 ms), so the average is fast enough to follow it.
const FILL_SMOOTH: f64 = 1.0 / 64.0;
const CHUNK_FRAMES: usize = 256;

#[repr(C)]
struct SampleSpec {
    format: c_int,
    rate: u32,
    channels: u8,
}

#[repr(C)]
struct BufferAttr {
    maxlength: u32,
    tlength: u32,
    prebuf: u32,
    minreq: u32,
    fragsize: u32,
}

const PA_STREAM_PLAYBACK: c_int = 1;
const PA_SAMPLE_S16LE: c_int = 3;

type PaNew = unsafe extern "C" fn(
    *const c_char,
    *const c_char,
    c_int,
    *const c_char,
    *const c_char,
    *const SampleSpec,
    *const c_void,
    *const BufferAttr,
    *mut c_int,
) -> *mut c_void;
type PaWrite = unsafe extern "C" fn(*mut c_void, *const c_void, usize, *mut c_int) -> c_int;
type PaLatency = unsafe extern "C" fn(*mut c_void, *mut c_int) -> u64;
type PaFree = unsafe extern "C" fn(*mut c_void);

type SndOpen = unsafe extern "C" fn(*mut *mut c_void, *const c_char, c_int, c_int) -> c_int;
type SndSetParams = unsafe extern "C" fn(*mut c_void, c_int, c_int, c_uint, c_uint, c_int, c_uint) -> c_int;
type SndWritei = unsafe extern "C" fn(*mut c_void, *const c_void, c_ulong) -> c_long;
type SndRecover = unsafe extern "C" fn(*mut c_void, c_int, c_int) -> c_int;
type SndDelay = unsafe extern "C" fn(*mut c_void, *mut c_long) -> c_int;
type SndClose = unsafe extern "C" fn(*mut c_void) -> c_int;
const SND_PCM_FORMAT_S16_LE: c_int = 2;
const SND_PCM_ACCESS_RW_INTERLEAVED: c_int = 3;
/// Buffer asked of the ALSA device while the graph runs at the game's small quantum (POCKET_CORE_ALSA_US overrides it).
const ALSA_LATENCY_US: c_uint = 10_000;
/// The same at ROCKNIX's 1024 (Bluetooth): PipeWire's ALSA plugin hands the graph what this buffer holds, and a buffer
/// shorter than one cycle (10 ms = 480 frames against 1024) left part of every cycle empty: the "extremely distorted"
/// games and the "quiet, distorted" menu sounds on Bluetooth headphones (2026-10-27). scripts/audio-bench at 1024:
/// 10 ms 340 dropouts in 7 s, 22, 27, 30 and 50 ms none; 32 ms keeps a cycle and two writer chunks.
const ALSA_CYCLE_US: c_uint = 32_000;

/// How much the ALSA device buffers: as little as the graph's cycle allows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Ring {
    /// `ALSA_LATENCY_US`, only while the graph runs at a quantum of 256 (the speaker, `audio_route`).
    Short,
    /// `ALSA_CYCLE_US`: holds a whole cycle of ROCKNIX's 1024 (Bluetooth, or no small quantum set).
    Cycle,
}

impl Ring {
    fn us(self) -> c_uint {
        match self {
            Ring::Short => ALSA_LATENCY_US,
            Ring::Cycle => ALSA_CYCLE_US,
        }
    }
}
/// PipeWire graph quantum while a game plays on the native stream (POCKET_CORE_QUANTUM, 0 keeps
/// ROCKNIX's). ROCKNIX forces 1024 frames (21 ms per cycle) with /usr/sbin/quantum at boot and wake.
/// Mini V2, mGBA, 20 s each, total latency: pulse 119 ms; native 98 ms at 1024; 43 ms at 256 with
/// a 16 ms queue; no underruns in any run. Only on the speaker: on a Bluetooth sink ROCKNIX's stays
/// (`audio_route`, which sets it on a thread of its own and follows a change of output while the game plays).
const GAME_QUANTUM: u32 = 256;

/// The quantum a game asks for on the speaker (`audio_route` leaves ROCKNIX's on Bluetooth); 0 for none.
fn game_quantum() -> u32 {
    std::env::var("POCKET_CORE_QUANTUM").ok().and_then(|v| v.parse().ok()).unwrap_or(GAME_QUANTUM)
}

/// Forces PipeWire's graph quantum to `frames` (`clock.force-quantum`); `restore_quantum` undoes it.
pub(crate) fn force_quantum(frames: u32) -> bool {
    crate::proc::status_with_timeout(
        std::process::Command::new("pw-metadata").args(["-n", "settings", "0", "clock.force-quantum", &frames.to_string()]),
        crate::proc::HELPER,
    )
    .is_ok_and(|s| s.success())
}

/// Puts ROCKNIX's quantum back, the way ROCKNIX sets it after a wake (its script toggles the value
/// so the driver picks it up). pocket-ui calls this too after every hosted game, in case
/// pocket-core died before it could.
pub fn restore_quantum() {
    let rocknix = std::path::Path::new("/usr/sbin/quantum");
    let _ = if rocknix.exists() {
        crate::proc::status_with_timeout(&mut std::process::Command::new(rocknix), crate::proc::HELPER)
    } else {
        crate::proc::status_with_timeout(
            std::process::Command::new("pw-metadata").args(["-n", "settings", "0", "clock.force-quantum", "0"]),
            crate::proc::HELPER,
        )
    };
}

/// Where the writer thread sends its chunks. pocket-ui's UI sounds (`ui_sound`) play through it too.
///
/// Dropping a sink closes the stream. That matters beyond tidiness: PipeWire's client library runs a thread
/// of its own for every stream, and a stream nobody closes keeps being woken by the graph at the quantum's
/// rate for as long as the process lives. A pocket-ui that played one menu sound and "closed" the output by
/// dropping the sink stayed at several percent of a CPU behind every game.
pub(crate) enum Sink {
    Pulse { _lib: Option<Library>, stream: *mut c_void, write: PaWrite, latency: PaLatency, free: PaFree },
    Alsa { _lib: Option<Library>, pcm: *mut c_void, writei: SndWritei, recover: SndRecover, delay: SndDelay, close: SndClose, ring: Ring },
}

// The handles are only ever used from the writer thread they are moved into.
unsafe impl Send for Sink {}

impl Drop for Sink {
    fn drop(&mut self) {
        // SAFETY: the handle came from the open call of the same library, which is still loaded (fields
        // drop after this), and it is closed exactly once, here.
        match self {
            Sink::Pulse { stream, free, .. } => unsafe { free(*stream) },
            Sink::Alsa { pcm, close, .. } => unsafe {
                close(*pcm);
            },
        }
    }
}

impl Sink {
    /// `ring`: see [`Ring`]; the pulse path has its own buffering.
    pub(crate) fn open(ring: Ring) -> Option<Sink> {
        match std::env::var("POCKET_CORE_AUDIO").as_deref() {
            Ok("pulse") => Self::pulse(),
            Ok("alsa") => Self::alsa(ring),
            _ => Self::alsa(ring).or_else(Self::pulse),
        }
    }

    /// The ALSA buffer this stream has; None on the pulse path.
    pub(crate) fn ring(&self) -> Option<Ring> {
        match self {
            Sink::Alsa { ring, .. } => Some(*ring),
            Sink::Pulse { .. } => None,
        }
    }

    fn pulse() -> Option<Sink> {
        let lib = unsafe { Library::new("libpulse-simple.so.0") }.ok()?;
        let (new, write, latency, free) = unsafe {
            (
                *lib.get::<PaNew>(b"pa_simple_new").ok()?,
                *lib.get::<PaWrite>(b"pa_simple_write").ok()?,
                *lib.get::<PaLatency>(b"pa_simple_get_latency").ok()?,
                *lib.get::<PaFree>(b"pa_simple_free").ok()?,
            )
        };
        let spec = SampleSpec { format: PA_SAMPLE_S16LE, rate: OUT_RATE, channels: 2 };
        let server_bytes = OUT_RATE * 4 * SERVER_MS / 1000;
        let attr = BufferAttr { maxlength: u32::MAX, tlength: server_bytes, prebuf: u32::MAX, minreq: u32::MAX, fragsize: u32::MAX };
        let mut err = 0;
        let stream = unsafe {
            new(std::ptr::null(), c"pocket-core".as_ptr(), PA_STREAM_PLAYBACK, std::ptr::null(), c"game".as_ptr(), &spec, std::ptr::null(), &attr, &mut err)
        };
        (!stream.is_null()).then(|| Sink::Pulse { _lib: Some(lib), stream, write, latency, free })
    }

    fn alsa(ring: Ring) -> Option<Sink> {
        let lib = unsafe { Library::new("libasound.so.2") }.ok()?;
        let (open, set_params, writei, recover, delay, close) = unsafe {
            (
                *lib.get::<SndOpen>(b"snd_pcm_open").ok()?,
                *lib.get::<SndSetParams>(b"snd_pcm_set_params").ok()?,
                *lib.get::<SndWritei>(b"snd_pcm_writei").ok()?,
                *lib.get::<SndRecover>(b"snd_pcm_recover").ok()?,
                *lib.get::<SndDelay>(b"snd_pcm_delay").ok()?,
                *lib.get::<SndClose>(b"snd_pcm_close").ok()?,
            )
        };
        let device = std::ffi::CString::new(std::env::var("POCKET_CORE_ALSA_DEVICE").unwrap_or_else(|_| "pipewire".into())).ok()?;
        let latency_us = std::env::var("POCKET_CORE_ALSA_US").ok().and_then(|v| v.parse().ok()).unwrap_or(ring.us());
        let mut pcm = std::ptr::null_mut();
        // The pipewire plugin names the stream after the program; PIPEWIRE_PROPS could name it, not needed.
        if unsafe { open(&mut pcm, device.as_ptr(), 0, 0) } < 0 || pcm.is_null() {
            return None;
        }
        if unsafe { set_params(pcm, SND_PCM_FORMAT_S16_LE, SND_PCM_ACCESS_RW_INTERLEAVED, 2, OUT_RATE, 1, latency_us) } < 0 {
            unsafe { close(pcm) };
            return None;
        }
        Some(Sink::Alsa { _lib: Some(lib), pcm, writei, recover, delay, close, ring })
    }

    pub fn name(&self) -> &'static str {
        match self {
            Sink::Pulse { .. } => "pulse",
            Sink::Alsa { .. } => "pipewire (alsa)",
        }
    }

    /// Blocks until the output has room; false when the stream is gone.
    pub(crate) fn write(&mut self, samples: &[i16]) -> bool {
        match self {
            Sink::Pulse { stream, write, .. } => {
                let mut err = 0;
                unsafe { write(*stream, samples.as_ptr() as *const c_void, samples.len() * 2, &mut err) >= 0 }
            }
            Sink::Alsa { pcm, writei, recover, .. } => {
                let mut rest = samples;
                while !rest.is_empty() {
                    let n = unsafe { writei(*pcm, rest.as_ptr() as *const c_void, (rest.len() / 2) as c_ulong) };
                    if n < 0 {
                        // An xrun (-EPIPE) or a suspend: recover and carry on with this chunk.
                        if unsafe { recover(*pcm, n as c_int, 1) } < 0 {
                            return false;
                        }
                        continue;
                    }
                    rest = &rest[n as usize * 2..];
                }
                true
            }
        }
    }

    /// Audio written but not yet played, in µs.
    pub(crate) fn latency_us(&mut self) -> u64 {
        match self {
            Sink::Pulse { stream, latency, .. } => {
                let mut err = 0;
                unsafe { latency(*stream, &mut err) }
            }
            Sink::Alsa { pcm, delay, .. } => {
                let mut frames: c_long = 0;
                if unsafe { delay(*pcm, &mut frames) } < 0 {
                    return 0;
                }
                frames.max(0) as u64 * 1_000_000 / OUT_RATE as u64
            }
        }
    }
}

/// How long the writer keeps trying to open a new stream after its stream failed, and how often.
const REOPEN_FOR: std::time::Duration = std::time::Duration::from_secs(5);
const REOPEN_EVERY: std::time::Duration = std::time::Duration::from_millis(200);

/// A new stream after the old one failed, tried for `REOPEN_FOR`; None when none comes or the audio is being closed.
fn reopen_stream(reopen: fn(Ring) -> Option<Sink>, ring: Ring, stopped: &impl Fn() -> bool) -> Option<Sink> {
    let t0 = std::time::Instant::now();
    eprintln!("audio: the stream failed; opening a new one");
    while !stopped() && t0.elapsed() < REOPEN_FOR {
        if let Some(s) = reopen(ring) {
            println!("audio: new stream ({}) after {} ms", s.name(), t0.elapsed().as_millis());
            crate::trace::step("audio: stream reopened");
            return Some(s);
        }
        std::thread::sleep(REOPEN_EVERY);
    }
    eprintln!("audio: no new stream; the game goes on without sound");
    None
}

/// The queue the rate control steers towards, in frames: `small` while the graph runs at the game's small quantum.
fn target_frames(heavy: bool, small: bool) -> usize {
    let base = if heavy { HEAVY_TARGET_MS } else { TARGET_MS };
    let ms = std::env::var("POCKET_CORE_AUDIO_MS").ok().and_then(|v| v.parse().ok()).unwrap_or(if small { base } else { base.max(CYCLE_TARGET_MS) });
    OUT_RATE as usize * ms.clamp(8, 200) / 1000
}

/// How fast the rate control's reference moves to the target, as a share of the frames pushed: 0.02 % of the pitch
/// (0.35 cents) at most. A queue that starts a few ms off its target (the first graph cycles, a stream just linked)
/// is brought there over a minute instead of with a glide of 2-4 cents over ten seconds (scripts/audio-bench).
const REFERENCE_SLEW: f64 = 0.0002;
/// Seconds of playing at the core's own pitch before the control starts and takes the averaged fill as its reference.
/// About the average's time constant; longer let a clock far off (0.3 %) empty the 20 ms queue in the meantime.
const WARM_S: f64 = 1.0;

/// Dynamic rate control: how far to step through the core's samples per output sample, from the queue's fill.
#[derive(Default)]
struct RateControl {
    /// The fill averaged over about a second (`FILL_SMOOTH`); None before the first push.
    fill: Option<f64>,
    /// The fill the control holds: the average once the first `WARM_S` have played, moving to the target by
    /// `REFERENCE_SLEW`; None until then.
    reference: Option<f64>,
    /// Output frames pushed since the writer started, for `WARM_S`.
    warm: f64,
    /// The target last seen: a new one (the output moved to or from Bluetooth) is taken at once.
    target: usize,
}

impl RateControl {
    /// `ratio` is the core's rate over the output's; `fill`, `target` and `pushed` (this push's output) in frames.
    /// `playing`: the writer has started; before that the queue only fills, and steering on it (towards more samples, a
    /// lower pitch) overfilled it and took seconds at full skew to drain: -8 then +9 cents after every start. For
    /// `WARM_S` after that the pitch stays the core's while the average settles.
    fn step(&mut self, ratio: f64, fill: usize, target: usize, pushed: f64, playing: bool) -> f64 {
        if !playing {
            *self = RateControl::default();
            return ratio;
        }
        let avg = match self.fill {
            Some(f) => f + (fill as f64 - f) * FILL_SMOOTH,
            None => fill as f64,
        };
        self.fill = Some(avg);
        let reference = match self.reference {
            Some(_) if target != self.target => target as f64,
            Some(r) => r + (target as f64 - r).clamp(-pushed * REFERENCE_SLEW, pushed * REFERENCE_SLEW),
            None => {
                self.warm += pushed;
                if self.warm < OUT_RATE as f64 * WARM_S {
                    self.target = target;
                    return ratio;
                }
                avg
            }
        };
        self.reference = Some(reference);
        self.target = target;
        let error = (reference - avg) / target as f64;
        // Running low → produce slightly more output per input sample, and the other way round.
        ratio * (1.0 - MAX_SKEW * error.clamp(-1.0, 1.0))
    }
}

/// The game's flags from `audio_route::Keeper` that the writer and the rate control go by.
#[derive(Clone)]
struct Route {
    small: Arc<std::sync::atomic::AtomicBool>,
    decided: Arc<std::sync::atomic::AtomicBool>,
}

impl Route {
    /// No keeper (the tests): as the speaker at the small quantum.
    fn ring(route: &Option<Route>) -> Ring {
        match route {
            Some(r) if !r.small.load(Ordering::Relaxed) => Ring::Cycle,
            _ => Ring::Short,
        }
    }
}

/// How long the writer waits for the keeper's first look before it starts playing (it is a directory read, and a
/// `pw-metadata` of about 11 ms with a Bluetooth link).
const ROUTE_WAIT: std::time::Duration = std::time::Duration::from_millis(500);

pub struct Audio {
    queue: Arc<Mutex<VecDeque<i16>>>,
    in_rate: f64,
    /// The queue's target in frames at the small quantum and at a whole cycle (`target_frames`).
    targets: (usize, usize),
    rate: RateControl,
    /// The writer plays (`RateControl::step`): false before it starts and during a pause.
    playing: Arc<std::sync::atomic::AtomicBool>,
    flags: Option<Route>,
    /// Fractional read position into `last` for the linear resampler.
    pos: f64,
    last: [i16; 2],
    pub underruns: Arc<Mutex<u64>>,
    /// Seconds after playback start of each underrun (first 1000).
    pub underrun_at: Arc<Mutex<Vec<f32>>>,
    /// What the server reports as still queued before the speaker, in µs (updated a few times a second).
    server_latency_us: Arc<AtomicU64>,
    paused: Arc<std::sync::atomic::AtomicBool>,
    pub backend: &'static str,
    /// Sets PipeWire's quantum for the output (speaker or Bluetooth) on a thread of its own, follows a change while
    /// the game plays and puts ROCKNIX's back when dropped (`audio_route::Keeper`). None in the tests.
    route: Option<crate::audio_route::Keeper>,
    /// Tells the writer thread to end (`Drop`), and hears when it has, with the stream closed.
    stop: Arc<std::sync::atomic::AtomicBool>,
    ended: std::sync::mpsc::Receiver<()>,
}

/// How long `Audio::drop` waits for the writer thread to close the stream; a write blocks for one chunk at most
/// unless the server hangs, and then the thread is left to end on its own.
const STOP_WAIT: std::time::Duration = std::time::Duration::from_millis(300);

impl Drop for Audio {
    fn drop(&mut self) {
        // The writer thread used to outlive this (SOL-13): waiting forever for the prefill, or writing silence.
        self.stop.store(true, Ordering::Relaxed);
        let _ = self.ended.recv_timeout(STOP_WAIT);
        // After the stream has closed, as before: ROCKNIX's quantum back unless `keep_quantum` (the keeper waits).
        drop(self.route.take());
    }
}

impl Audio {
    /// Leaves PipeWire's quantum as the game set it when the stream closes: the next pocket-core game, starting
    /// while this one ends (`fast_switch`), sets the same and must not have it put back under it.
    pub fn keep_quantum(&mut self) {
        if let Some(r) = &self.route {
            r.keep();
        }
    }

    /// The sound goes to a Bluetooth sink, as last seen (Pixel Path: its delay isn't the speaker's).
    pub fn bluetooth(&self) -> bool {
        self.route.as_ref().is_some_and(|r| r.bluetooth())
    }

    /// Opens a 48 kHz stereo stream; None when no audio server is available. `heavy` (3D cores) gets
    /// the longer queue, `HEAVY_TARGET_MS`.
    pub fn open(in_rate: f64, heavy: bool) -> Option<Audio> {
        // Short, as on the speaker: the writer opens it again with the longer buffer before it plays if the keeper
        // finds Bluetooth (or can't lower the quantum), and again whenever that changes.
        let sink = Sink::open(Ring::Short)?;
        // The pulse path can't go below 1024 per client anyway; only the native stream gains. The quantum is set on
        // the keeper's thread (it may ask PipeWire for the default sink first), not here: this only starts it.
        let frames = if matches!(sink, Sink::Alsa { .. }) { game_quantum() } else { 0 };
        let route = Some(crate::audio_route::Keeper::spawn(frames));
        Some(Audio::start(sink, in_rate, heavy, route, Sink::open))
    }

    /// The writer thread for `sink`, and the queue it plays from. `reopen` opens a new stream when this one fails
    /// (`REOPEN_FOR`; the tests give one that can't).
    fn start(sink: Sink, in_rate: f64, heavy: bool, route: Option<crate::audio_route::Keeper>, reopen: fn(Ring) -> Option<Sink>) -> Audio {
        let flags = route.as_ref().map(|k| Route { small: k.small_flag(), decided: k.decided_flag() });
        Self::start_with(sink, in_rate, heavy, route, flags, reopen)
    }

    /// `start` with the keeper's flags given apart (the tests give them without a keeper).
    fn start_with(
        mut sink: Sink,
        in_rate: f64,
        heavy: bool,
        route: Option<crate::audio_route::Keeper>,
        flags: Option<Route>,
        reopen: fn(Ring) -> Option<Sink>,
    ) -> Audio {
        let backend = sink.name();
        let queue = Arc::new(Mutex::new(VecDeque::with_capacity(OUT_RATE as usize)));
        let underruns = Arc::new(Mutex::new(0));
        let underrun_at = Arc::new(Mutex::new(Vec::new()));
        let server_latency_us = Arc::new(AtomicU64::new(0));
        let paused = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let (ended_tx, ended) = std::sync::mpsc::channel();
        let playing = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let (q, u, ua, lat, p, halt, fl, started) =
            (queue.clone(), underruns.clone(), underrun_at.clone(), server_latency_us.clone(), paused.clone(), stop.clone(), flags.clone(), playing.clone());
        let stopped = move || halt.load(Ordering::Relaxed);
        // The queue's target for each `Ring` (read once: POCKET_CORE_AUDIO_MS).
        let targets = (target_frames(heavy, true), target_frames(heavy, false));
        let target_for = move |ring: Ring| if ring == Ring::Short { targets.0 } else { targets.1 };
        std::thread::spawn(move || {
            // Runs until the stream fails or `Audio` is dropped; then the stream is closed and the drop hears of it.
            let mut play = || {
                let mut chunk = vec![0i16; CHUNK_FRAMES * 2];
                // The keeper's first look (speaker or Bluetooth, the quantum set) decides the buffer: waited for here,
                // beside the core's first frames, not in `open`.
                let t0 = std::time::Instant::now();
                while fl.as_ref().is_some_and(|f| !f.decided.load(Ordering::Relaxed)) && t0.elapsed() < ROUTE_WAIT {
                    if stopped() {
                        return;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(2));
                }
                // The server swallows data at wire speed until its own buffer is full (about 100 ms on this
                // PipeWire). Fill that with silence first, until a write blocks; otherwise it empties our
                // queue in one go and the first seconds glitch.
                let silence = vec![0i16; CHUNK_FRAMES * 2];
                let prefill = |sink: &mut Sink| {
                    for _ in 0..200 {
                        let t = std::time::Instant::now();
                        sink.write(&silence);
                        if t.elapsed() > std::time::Duration::from_millis(4) {
                            break;
                        }
                    }
                };
                // A buffer for the cycle the graph runs at (`Ring`), before the first sample and whenever the output
                // or the quantum changes while the game plays (headphones connected or switched off).
                let fit = |sink: &mut Sink| {
                    let want = Route::ring(&fl);
                    if sink.ring().is_some_and(|r| r != want) {
                        if let Some(new) = reopen(want) {
                            println!("audio: {want:?} buffer ({} ms)", want.us() / 1000);
                            *sink = new;
                            prefill(sink);
                        }
                    }
                };
                // Opened again before the queue fills, not after: the silence in it blocks until WirePlumber has linked
                // the new stream, and the core's frames meanwhile overfilled the queue by up to 100 ms, which the rate
                // control then drained at +5 cents for ten seconds (scripts/audio-bench).
                fit(&mut sink);
                // Start playing once the target is buffered: where the rate control holds it (1.5x the target, as
                // before 2026-10-28, cost a 4-cent glide every start while the excess drained).
                while q.lock().unwrap().len() < target_for(Route::ring(&fl)) * 2 {
                    if stopped() {
                        return;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(2));
                }
                prefill(&mut sink);
                started.store(true, Ordering::Relaxed);
                let start = std::time::Instant::now();
                let mut written = 0u64;
                let mut was_paused = false;
                while !stopped() {
                    {
                        let paused = p.load(std::sync::atomic::Ordering::Relaxed);
                        let mut q = q.lock().unwrap();
                        // Paused (pause menu): silence, entered and left with a short fade instead of a
                        // click. What is still queued plays on after the pause, where it belongs.
                        let ramp = |chunk: &mut [i16], out: bool| {
                            for (i, s) in chunk.iter_mut().enumerate() {
                                let g = (i / 2) as f32 / CHUNK_FRAMES as f32;
                                *s = (*s as f32 * if out { 1.0 - g } else { g }) as i16;
                            }
                        };
                        let pop = |q: &mut VecDeque<i16>, chunk: &mut [i16]| chunk.iter_mut().for_each(|s| *s = q.pop_front().unwrap_or(0));
                        let target = target_for(Route::ring(&fl));
                        if paused {
                            if !was_paused && q.len() >= chunk.len() {
                                pop(&mut q, &mut chunk);
                                ramp(&mut chunk, true);
                            } else {
                                chunk.fill(0);
                            }
                            was_paused = true;
                            // The rate control starts afresh after the pause (`RateControl::step`): the queue comes
                            // back up to a core frame off where it was, and steering that back glided the pitch 2 cents.
                            started.store(false, Ordering::Relaxed);
                        } else if was_paused {
                            // Back from a pause: wait until the core has refilled the queue, then fade in.
                            if q.len() >= target * 2 {
                                pop(&mut q, &mut chunk);
                                ramp(&mut chunk, false);
                                was_paused = false;
                                started.store(true, Ordering::Relaxed);
                                // After the pause menu or a wake (`latency.py measure --wake`: the sound is back).
                                crate::trace::step("audio: playing again");
                            } else {
                                chunk.fill(0);
                            }
                        } else if q.len() >= chunk.len() {
                            pop(&mut q, &mut chunk);
                        } else {
                            chunk.fill(0);
                            *u.lock().unwrap() += 1;
                            let mut at = ua.lock().unwrap();
                            if at.len() < 1000 {
                                at.push(start.elapsed().as_secs_f32());
                            }
                        }
                    }
                    // Blocks until the server has room: this thread runs at the sound card's pace.
                    if !sink.write(&chunk) {
                        // The stream is gone (the sound server was restarted, say around a deep sleep): a new one,
                        // instead of a game without sound until it is started again.
                        match reopen_stream(reopen, Route::ring(&fl), &stopped) {
                            Some(new) => sink = new,
                            None => return,
                        }
                    }
                    // Neither API is thread-safe, so the writer thread is the one that asks.
                    written += 1;
                    if written.is_multiple_of(32) {
                        lat.store(sink.latency_us(), Ordering::Relaxed);
                    }
                    fit(&mut sink);
                }
            };
            play();
            drop(sink);
            let _ = ended_tx.send(());
        });
        Audio {
            queue,
            in_rate,
            targets,
            rate: RateControl::default(),
            playing,
            flags,
            pos: 0.0,
            last: [0, 0],
            underruns,
            underrun_at,
            server_latency_us,
            paused,
            backend,
            route,
            stop,
            ended,
        }
    }

    /// After the system woke from sleep: ROCKNIX's wake hook (sleep.d/post/003-audio) has put the
    /// quantum back to 1024, so a game playing on the native stream to the speaker lowers it again (on the
    /// keeper's thread, pw-metadata takes a few ms), and one that now plays to Bluetooth leaves it.
    pub fn reassert_quantum(&self) {
        if let Some(r) = &self.route {
            r.wake();
        }
    }

    /// Silences the output while the game is paused (pause menu); the stream itself keeps running.
    pub fn pause(&self, on: bool) {
        self.paused.store(on, std::sync::atomic::Ordering::Relaxed);
    }

    /// The queue the rate control steers towards now, in frames (`target_frames`).
    fn target(&self) -> usize {
        if Route::ring(&self.flags) == Ring::Short {
            self.targets.0
        } else {
            self.targets.1
        }
    }

    /// Adds one video frame's worth of core samples (interleaved stereo).
    pub fn push(&mut self, samples: &[i16]) {
        let target = self.target();
        let mut q = self.queue.lock().unwrap();
        let fill = q.len() / 2;
        let ratio = self.in_rate / OUT_RATE as f64;
        let pushed = samples.len() as f64 / 2.0 / ratio;
        let step = self.rate.step(ratio, fill, target, pushed, self.playing.load(Ordering::Relaxed));
        // Keep latency bounded if the core ran far ahead (e.g. after a pause).
        if fill > target * 3 {
            let drop = (fill - target) * 2;
            q.drain(..drop);
            // The average would remember the overfull queue for a second.
            self.rate.fill = Some((fill - drop / 2) as f64);
        }
        for frame in samples.chunks_exact(2) {
            let next = [frame[0], frame[1]];
            while self.pos < 1.0 {
                let t = self.pos;
                q.push_back((self.last[0] as f64 * (1.0 - t) + next[0] as f64 * t) as i16);
                q.push_back((self.last[1] as f64 * (1.0 - t) + next[1] as f64 * t) as i16);
                self.pos += step;
            }
            self.pos -= 1.0;
            self.last = next;
        }
    }

    /// Effective rate of the samples pushed per second of real time.
    pub fn set_input_rate(&mut self, rate: f64) {
        self.in_rate = rate;
    }

    /// The rate set by `set_input_rate` (Pixel Path's Audio stage).
    pub fn input_rate(&self) -> f64 {
        self.in_rate
    }

    /// The queue the rate control steers towards, in ms.
    pub fn target_ms(&self) -> f64 {
        self.target() as f64 / OUT_RATE as f64 * 1000.0
    }

    pub fn buffered_ms(&self) -> f64 {
        self.queue.lock().unwrap().len() as f64 / 2.0 / OUT_RATE as f64 * 1000.0
    }

    /// Audio still in the server (PipeWire and the sound card) at the last measurement.
    pub fn server_ms(&self) -> f64 {
        self.server_latency_us.load(Ordering::Relaxed) as f64 / 1000.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    // Per test thread: the tests run in parallel, and a drop runs on the thread that owns the sink.
    thread_local! {
        static PCM_CLOSED: Cell<usize> = const { Cell::new(0) };
        static PULSE_FREED: Cell<usize> = const { Cell::new(0) };
    }
    fn closed() -> usize {
        PCM_CLOSED.with(Cell::get)
    }
    fn freed() -> usize {
        PULSE_FREED.with(Cell::get)
    }

    unsafe extern "C" fn fake_write(_: *mut c_void, _: *const c_void, _: usize, _: *mut c_int) -> c_int {
        0
    }
    unsafe extern "C" fn fake_latency(_: *mut c_void, _: *mut c_int) -> u64 {
        0
    }
    unsafe extern "C" fn fake_free(_: *mut c_void) {
        PULSE_FREED.with(|c| c.set(c.get() + 1));
    }
    unsafe extern "C" fn fake_writei(_: *mut c_void, _: *const c_void, _: c_ulong) -> c_long {
        0
    }
    unsafe extern "C" fn fake_recover(_: *mut c_void, err: c_int, _: c_int) -> c_int {
        err
    }
    unsafe extern "C" fn fake_delay(_: *mut c_void, _: *mut c_long) -> c_int {
        0
    }
    unsafe extern "C" fn fake_close(_: *mut c_void) -> c_int {
        PCM_CLOSED.with(|c| c.set(c.get() + 1));
        0
    }

    fn alsa_sink() -> Sink {
        Sink::Alsa {
            _lib: None,
            pcm: std::ptr::NonNull::<c_void>::dangling().as_ptr(),
            writei: fake_writei,
            recover: fake_recover,
            delay: fake_delay,
            close: fake_close,
            ring: Ring::Short,
        }
    }

    /// The idle-CPU bug: the UI sound mixer "closed" the output by dropping the sink, which left the
    /// PipeWire stream (and its data-loop thread, woken 160 times a second) alive behind every game.
    #[test]
    fn dropping_an_alsa_sink_closes_its_stream() {
        let before = closed();
        let sink = alsa_sink();
        assert_eq!(closed(), before, "open until dropped");
        drop(sink);
        assert_eq!(closed(), before + 1, "closed once, by the drop");
    }

    #[test]
    fn dropping_a_pulse_sink_frees_its_stream() {
        let before = freed();
        let sink = Sink::Pulse { _lib: None, stream: std::ptr::NonNull::<c_void>::dangling().as_ptr(), write: fake_write, latency: fake_latency, free: fake_free };
        drop(sink);
        assert_eq!(freed(), before + 1);
    }

    static WORKER_FREED: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    unsafe extern "C" fn paced_write(_: *mut c_void, _: *const c_void, _: usize, _: *mut c_int) -> c_int {
        std::thread::sleep(std::time::Duration::from_millis(1));
        0
    }
    unsafe extern "C" fn counted_free(_: *mut c_void) {
        WORKER_FREED.fetch_add(1, Ordering::SeqCst);
    }

    fn paced_sink() -> Sink {
        Sink::Pulse { _lib: None, stream: std::ptr::NonNull::<c_void>::dangling().as_ptr(), write: paced_write, latency: fake_latency, free: counted_free }
    }

    /// SOL-13: the writer thread kept its stream and ran on after `Audio` was dropped, waiting forever for the prefill
    /// or writing silence. It ends now, and the stream is closed, by the time the drop returns.
    #[test]
    fn dropping_audio_ends_its_writer_and_closes_the_stream() {
        // Dropped before a single sample came (the core never started).
        let before = WORKER_FREED.load(Ordering::SeqCst);
        let audio = Audio::start(paced_sink(), 32768.0, false, None, |_| None);
        std::thread::sleep(std::time::Duration::from_millis(20));
        assert_eq!(WORKER_FREED.load(Ordering::SeqCst), before, "open while the audio lives");
        drop(audio);
        assert_eq!(WORKER_FREED.load(Ordering::SeqCst), before + 1, "closed by the time the drop returns");
        // Dropped while playing.
        let mut audio = Audio::start(paced_sink(), 48000.0, false, None, |_| None);
        audio.push(&vec![100i16; 48000 / 10 * 2]);
        std::thread::sleep(std::time::Duration::from_millis(30));
        drop(audio);
        assert_eq!(WORKER_FREED.load(Ordering::SeqCst), before + 2);
    }

    static REOPENED_WRITES: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    unsafe extern "C" fn failing_write(_: *mut c_void, _: *const c_void, _: usize, _: *mut c_int) -> c_int {
        -1
    }
    unsafe extern "C" fn counted_write(_: *mut c_void, _: *const c_void, _: usize, _: *mut c_int) -> c_int {
        REOPENED_WRITES.fetch_add(1, Ordering::SeqCst);
        std::thread::sleep(std::time::Duration::from_millis(1));
        0
    }
    fn reopen_counted(_: Ring) -> Option<Sink> {
        Some(Sink::Pulse { _lib: None, stream: std::ptr::NonNull::<c_void>::dangling().as_ptr(), write: counted_write, latency: fake_latency, free: fake_free })
    }

    /// Instant on: a stream that fails (the sound server restarted around a deep sleep) is replaced by a new one, and
    /// the game goes on with sound instead of without it until it is started again.
    #[test]
    fn a_failed_stream_is_opened_again() {
        let broken = Sink::Pulse { _lib: None, stream: std::ptr::NonNull::<c_void>::dangling().as_ptr(), write: failing_write, latency: fake_latency, free: fake_free };
        let mut audio = Audio::start(broken, 48000.0, false, None, reopen_counted);
        audio.push(&vec![100i16; 48000 / 10 * 2]);
        let t0 = std::time::Instant::now();
        while REOPENED_WRITES.load(Ordering::SeqCst) == 0 && t0.elapsed() < std::time::Duration::from_secs(2) {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(REOPENED_WRITES.load(Ordering::SeqCst) > 0, "the new stream plays");
        drop(audio);
    }

    /// The queue as the rate control sees it: one core frame pushed per 60 Hz refresh (a GBA's 65536 Hz at 59.73 fps,
    /// fed at the display's rate), and the graph taking `quantum` frames at once every cycle, its clock `skew` off the
    /// display's. The writer starts with `start` (of the target) queued. Returns the wobble after the first minute
    /// (settled; rms and max cents from the mean), the furthest the pitch went from the core's own from the start, and
    /// the lowest fill.
    fn simulate(quantum: usize, target: usize, skew: f64, start: f64) -> (f64, f64, f64, usize) {
        let mut rc = RateControl::default();
        let in_ratio = 65536.0 * 60.0 / 59.7275 / OUT_RATE as f64;
        let per_push = 65536.0 / 59.7275;
        let (push_every, cycle_every) = (1.0 / 60.0, quantum as f64 / OUT_RATE as f64 * (1.0 + skew));
        let (mut fill, mut next_push, mut next_cycle, mut lowest) = (target as f64 * start, 0.0, 0.0, usize::MAX);
        let (mut steps, mut furthest) = (Vec::new(), 0.0f64);
        while next_push < 180.0 {
            if next_push <= next_cycle {
                let step = rc.step(in_ratio, fill as usize, target, per_push / in_ratio, true);
                fill += per_push / step;
                furthest = furthest.max((1200.0 * (step / in_ratio).log2()).abs());
                if next_push > 60.0 {
                    steps.push(step);
                }
                next_push += push_every;
            } else {
                fill -= quantum as f64;
                lowest = lowest.min(fill.max(0.0) as usize);
                next_cycle += cycle_every;
            }
        }
        let mean = steps.iter().sum::<f64>() / steps.len() as f64;
        let cents: Vec<f64> = steps.iter().map(|s| 1200.0 * (s / mean).log2()).collect();
        let rms = (cents.iter().map(|c| c * c).sum::<f64>() / cents.len() as f64).sqrt();
        (rms, cents.iter().fold(0.0f64, |m, c| m.max(c.abs())), furthest, lowest)
    }

    /// The pitch wobble heard as distortion on Bluetooth: the fill saws by a whole graph cycle against the 60 Hz
    /// pushes, and steering on each raw reading moved the pitch by up to ±8 cents (scripts/audio-bench). The averaged
    /// fill keeps it far below what can be heard (0.04 cents rms; each raw reading gave 0.66 rms and 1.3 max here, at 256),
    /// also with the graph's clock 0.3 % off, and the queue never runs dry.
    #[test]
    fn the_rate_control_holds_the_pitch_steady() {
        let short = OUT_RATE as usize * TARGET_MS / 1000;
        let cycle = OUT_RATE as usize * CYCLE_TARGET_MS / 1000;
        // Clocks ±0.3 % apart when started at the target, ±0.1 % when started 30 % below it (a stream just linked on
        // Bluetooth, seen in the bench): far more than crystals differ by (±0.01 %; pocket-core measures the display's
        // period). At 2048 (not a quantum ROCKNIX runs) a graph that much faster would empty the 48 ms queue.
        let cases = [(256, short), (1024, cycle), (2048, cycle)];
        let runs = [(0.0, 1.0), (0.003, 1.0), (-0.003, 1.0), (0.0, 0.7), (0.001, 0.7), (-0.001, 0.7)];
        for (quantum, target) in cases {
            for (skew, start) in runs {
                // 2048 only as it starts: its cycle is most of the 48 ms queue, so a start 30 % low or a fast clock
                // leaves it too little.
                if quantum == 2048 && (skew != 0.0 || start < 1.0) {
                    continue;
                }
                let (rms, max, furthest, lowest) = simulate(quantum, target, skew, start);
                assert!(rms < 0.2 && max < 0.5, "quantum {quantum}, skew {skew}: wobble {rms:.3} cents rms, {max:.3} max");
                // With the clocks in step the pitch stays within a cent of the core's from the first sample on: the
                // queue's start is taken as it is and brought to the target slowly (`REFERENCE_SLEW`). Before
                // 2026-10-28 it glided 8 cents each way after every start.
                // At 2048 (not a quantum ROCKNIX runs) the average hasn't quite settled after `WARM_S`: 1.35 cents.
                let limit = if quantum > 1024 { 1.5 } else { 1.0 };
                assert!(skew != 0.0 || furthest < limit, "quantum {quantum}, start {start}: {furthest:.2} cents from the core's pitch");
                assert!(lowest > 0, "quantum {quantum}, skew {skew}: the queue ran dry");
            }
        }
    }

    /// `out = None` is how the UI sound mixer closes the output (a game starts, or the sounds went quiet).
    #[test]
    fn the_mixers_way_of_closing_the_output_closes_the_stream() {
        let before = closed();
        let mut out = Some(alsa_sink());
        assert!(out.is_some());
        out = None;
        assert!(out.is_none());
        assert_eq!(closed(), before + 1);
    }
}
