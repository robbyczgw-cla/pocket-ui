//! Linux audio sink. Queueing, resampling and rate control stay in audio.rs.
use std::ffi::{c_char,c_int,c_long,c_uint,c_ulong,c_void};
use libloading::Library;
use crate::audio::OUT_RATE;
const SERVER_MS:u32=20;
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
    pub(crate) fn us(self) -> c_uint {
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
pub(crate) fn game_quantum() -> u32 {
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
    pub(crate) fn uses_graph(&self) -> bool { matches!(self, Sink::Alsa { .. }) }
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


/// Linux already sizes its queue for the selected PipeWire quantum.
pub(crate) fn minimum_queue_ms()->u32 {0}
