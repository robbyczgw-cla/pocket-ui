//! Linux audio sink. Queueing, resampling and rate control stay in audio.rs.
use crate::audio::OUT_RATE;
use crate::audio_diagnostics::{Diagnostics, WriterObservation};
use libloading::Library;
use std::ffi::{c_char, c_int, c_long, c_uint, c_ulong, c_void};
use std::sync::Arc;
use std::time::{Duration, Instant};
const SERVER_MS: u32 = 20;
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
#[cfg(feature = "brick")]
type SndControl = unsafe extern "C" fn(*mut c_void) -> c_int;
type SndAvail = unsafe extern "C" fn(*mut c_void) -> c_long;
type SndGetParams = unsafe extern "C" fn(*mut c_void, *mut c_ulong, *mut c_ulong) -> c_int;
type SndHwMalloc = unsafe extern "C" fn(*mut *mut c_void) -> c_int;
type SndHwFree = unsafe extern "C" fn(*mut c_void);
type SndHwCurrent = unsafe extern "C" fn(*mut c_void, *mut c_void) -> c_int;
type SndHwRate = unsafe extern "C" fn(*const c_void, *mut c_uint, *mut c_int) -> c_int;

/// Per-stream function pointers and timing. Missing optional ALSA queries never prevent playback.
#[derive(Default)]
pub(crate) struct Observation {
    writer: WriterObservation,
    avail: Option<SndAvail>,
    rate: Option<u32>,
    buffer: Option<u64>,
    period: Option<u64>,
    config_failures: u64,
    last_sample: Option<Instant>,
    #[cfg(feature = "brick")]
    native_pause: Option<(SndControl, SndControl)>,
}
impl Observation {
    #[cfg(all(test, feature = "brick"))]
    pub(crate) fn with_native_pause(drain: SndControl, prepare: SndControl) -> Self {
        Self { native_pause: Some((drain, prepare)), ..Self::default() }
    }
    fn alsa(lib: &Library, pcm: *mut c_void) -> Self {
        let mut out = Self::default();
        // SAFETY: symbols are used only while the Sink retains this Library, on its owning writer.
        unsafe {
            out.avail = lib.get::<SndAvail>(b"snd_pcm_avail_update").ok().map(|s| *s);
            if let Ok(get) = lib.get::<SndGetParams>(b"snd_pcm_get_params") {
                let (mut buffer, mut period) = (0, 0);
                if get(pcm, &mut buffer, &mut period) >= 0 && buffer != 0 && period != 0 {
                    out.buffer = Some(buffer as u64);
                    out.period = Some(period as u64);
                } else {
                    out.config_failures += 1;
                }
            } else {
                out.config_failures += 1;
            }
            let funcs = (
                lib.get::<SndHwMalloc>(b"snd_pcm_hw_params_malloc"),
                lib.get::<SndHwFree>(b"snd_pcm_hw_params_free"),
                lib.get::<SndHwCurrent>(b"snd_pcm_hw_params_current"),
                lib.get::<SndHwRate>(b"snd_pcm_hw_params_get_rate"),
            );
            if let (Ok(malloc), Ok(free), Ok(current), Ok(get_rate)) = funcs {
                let mut params = std::ptr::null_mut();
                if malloc(&mut params) >= 0 && !params.is_null() {
                    let (mut rate, mut direction) = (0, 0);
                    if current(pcm, params) >= 0 && get_rate(params, &mut rate, &mut direction) >= 0 && rate != 0 {
                        out.rate = Some(rate);
                    }
                    free(params);
                }
            }
            if out.rate.is_none() {
                out.config_failures += 1;
            }
        }
        out
    }
    fn sample(&mut self, pcm: *mut c_void, delay: SndDelay) {
        let Some(d) = self.writer.diagnostics.as_ref() else {
            return;
        };
        let now = Instant::now();
        if self.last_sample.is_some_and(|last| now.duration_since(last) < Duration::from_millis(100)) {
            return;
        }
        self.last_sample = Some(now);
        let mut frames = 0;
        let result = unsafe { delay(pcm, &mut frames) };
        d.alsa_delay(if result < 0 { Err(result as i64) } else { Ok(frames as i64) });
        if let Some(avail) = self.avail {
            let result = unsafe { avail(pcm) };
            d.alsa_avail(if result < 0 { Err(result as i64) } else { Ok(result as i64) });
        }
        // A missing optional avail symbol has no samples (None), rather than a successful zero.
    }
}
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
    /// A whole cycle of ROCKNIX's 1024: 32 ms normally, 64 ms on H700 for scheduling headroom.
    Cycle,
}

impl Ring {
    pub(crate) fn us(self) -> c_uint {
        match self {
            Ring::Short => ALSA_LATENCY_US,
            Ring::Cycle => cycle_latency_us(is_h700()),
        }
    }
}
/// PipeWire graph quantum while a game plays on the native stream (POCKET_CORE_QUANTUM, 0 keeps
/// ROCKNIX's). ROCKNIX forces 1024 frames (21 ms per cycle) with /usr/sbin/quantum at boot and wake.
/// Mini V2, mGBA, 20 s each, total latency: pulse 119 ms; native 98 ms at 1024; 43 ms at 256 with
/// a 16 ms queue; no underruns in any run. Only on the speaker: on a Bluetooth sink ROCKNIX's stays
/// (`audio_route`, which sets it on a thread of its own and follows a change of output while the game plays).
const GAME_QUANTUM: u32 = 256;

/// The quantum a game asks for on the speaker: 1024 on H700, 256 elsewhere.
/// `audio_route` leaves ROCKNIX's on Bluetooth; an explicit 0 requests no change.
pub(crate) fn game_quantum() -> u32 {
    let h700 = is_h700();
    std::env::var("POCKET_CORE_QUANTUM").ok().and_then(|v| v.parse().ok()).unwrap_or(default_game_quantum(h700))
}

fn is_h700() -> bool {
    crate::device::current().model.is_some_and(|m| m.is_h700())
}

fn cycle_latency_us(h700: bool) -> c_uint {
    // Match the threaded-ALSA reference's PCM reserve on Cortex-A53. A 32 ms client ring at a
    // 1024-frame graph cycle can still run dry without an ALSA EPIPE; the waveform bench catches it.
    if h700 { 64_000 } else { ALSA_CYCLE_US }
}

fn default_game_quantum(h700: bool) -> u32 {
    // At 1024, audio_route selects the cycle ring (64 ms on H700) and 48 ms software queue. This
    // quarters graph wakeups on Cortex-A53 and leaves room for a late core frame, as threaded ALSA does.
    if h700 { 1024 } else { GAME_QUANTUM }
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
    Pulse {
        _lib: Option<Library>,
        stream: *mut c_void,
        write: PaWrite,
        latency: PaLatency,
        free: PaFree,
        observation: Observation,
    },
    Alsa {
        _lib: Option<Library>,
        pcm: *mut c_void,
        writei: SndWritei,
        recover: SndRecover,
        delay: SndDelay,
        close: SndClose,
        ring: Ring,
        observation: Observation,
    },
}

// The handles are only ever used from the writer thread they are moved into.
unsafe impl Send for Sink {}

impl Drop for Sink {
    fn drop(&mut self) {
        // SAFETY: the handle came from the open call of the same library, which is still loaded (fields
        // drop after this), and it is closed exactly once, here.
        match self {
            // Null: closed already (`close`, the Brick's suspend).
            Sink::Pulse { stream, .. } | Sink::Alsa { pcm: stream, .. } if stream.is_null() => {}
            Sink::Pulse { stream, free, .. } => unsafe { free(*stream) },
            Sink::Alsa { pcm, close, .. } => unsafe {
                close(*pcm);
            },
        }
    }
}

impl Sink {
    /// Attach the same session object after every reopen so successful recoveries cannot disappear.
    pub(crate) fn observe(&mut self, diagnostics: Arc<Diagnostics>) {
        match self {
            Sink::Pulse { observation, .. } => observation.writer.attach(diagnostics, "pulse"),
            Sink::Alsa { observation, .. } => {
                diagnostics.alsa_config(observation.rate, observation.buffer, observation.period, observation.config_failures);
                diagnostics.alsa_avail_supported(observation.avail.is_some());
                observation.writer.attach(diagnostics, "alsa");
            }
        }
    }
    /// Closes the stream now and leaves this sink empty: writes fail until it is replaced (`Audio::hold`). The Brick's
    /// kernel suspends a PCM that is open across `mem` and the first write after the wake fails (-ESTRPIPE, then -EPIPE),
    /// so nothing of ours stays open while it sleeps (docs/TRIMUI-BRICK-SLEEP.md).
    #[cfg(feature = "brick")]
    pub(crate) fn close(&mut self) {
        match self {
            Sink::Pulse { stream, free, .. } if !stream.is_null() => unsafe { free(std::mem::replace(stream, std::ptr::null_mut())) },
            Sink::Alsa { pcm, close, .. } if !pcm.is_null() => unsafe {
                close(std::mem::replace(pcm, std::ptr::null_mut()));
            },
            _ => {}
        }
    }

    #[cfg(feature = "brick")]
    fn closed(&self) -> bool {
        match self {
            Sink::Pulse { stream, .. } => stream.is_null(),
            Sink::Alsa { pcm, .. } => pcm.is_null(),
        }
    }

    pub(crate) fn uses_graph(&self) -> bool {
        matches!(self, Sink::Alsa { .. })
    }
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
        let spec = SampleSpec {
            format: PA_SAMPLE_S16LE,
            rate: OUT_RATE,
            channels: 2,
        };
        let server_bytes = OUT_RATE * 4 * SERVER_MS / 1000;
        let attr = BufferAttr {
            maxlength: u32::MAX,
            tlength: server_bytes,
            prebuf: u32::MAX,
            minreq: u32::MAX,
            fragsize: u32::MAX,
        };
        let mut err = 0;
        let stream = unsafe {
            new(
                std::ptr::null(),
                c"pocket-core".as_ptr(),
                PA_STREAM_PLAYBACK,
                std::ptr::null(),
                c"game".as_ptr(),
                &spec,
                std::ptr::null(),
                &attr,
                &mut err,
            )
        };
        (!stream.is_null()).then(|| Sink::Pulse {
            _lib: Some(lib),
            stream,
            write,
            latency,
            free,
            observation: Observation::default(),
        })
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
        #[cfg(not(feature = "brick"))]
        let device = std::ffi::CString::new(std::env::var("POCKET_CORE_ALSA_DEVICE").unwrap_or_else(|_| "pipewire".into())).ok()?;
        // The Brick: its headphones while brick-bluetooth names one (`brick_device`), read at every open.
        #[cfg(feature = "brick")]
        let device = std::ffi::CString::new(brick_device(crate::audio_route::brick_route().as_deref(), std::env::var("POCKET_CORE_ALSA_DEVICE").ok())).ok()?;
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
        #[allow(unused_mut)]
        let mut observation = Observation::alsa(&lib, pcm);
        // Brick's "pipewire" name is its native chip, not a PipeWire graph.
        // BlueALSA keeps its existing streaming/pause behaviour.
        #[cfg(feature = "brick")]
        if !device.to_bytes().starts_with(b"pocket_bt:") {
            unsafe {
                if let (Ok(drain), Ok(prepare)) = (lib.get::<SndControl>(b"snd_pcm_drain"), lib.get::<SndControl>(b"snd_pcm_prepare")) {
                    observation.native_pause = Some((*drain, *prepare));
                }
            }
        }
        Some(Sink::Alsa {
            _lib: Some(lib),
            pcm,
            writei,
            recover,
            delay,
            close,
            ring,
            observation,
        })
    }

    #[cfg(not(feature = "brick"))]
    pub fn name(&self) -> &'static str {
        match self {
            Sink::Pulse { .. } => "pulse",
            Sink::Alsa { .. } => "pipewire (alsa)",
        }
    }

    #[cfg(feature = "brick")]
    pub fn name(&self) -> &'static str {
        match self {
            Sink::Pulse { .. } => "pulse",
            Sink::Alsa { .. } => { #[cfg(feature = "brick")] { if crate::audio_route::brick_route().is_some() { "bluetooth (bluealsa)" } else if std::env::var("POCKET_CORE_ALSA_DEVICE").is_ok_and(|d| d != "pipewire") { "alsa" } else { "pipewire (alsa)" } } #[cfg(not(feature = "brick"))] { "pipewire (alsa)" } }
        }
    }

    /// The native chip needs no graph-linking writes before the game queue is
    /// ready. Its prepared PCM stays stopped until the first startup prefill.
    #[cfg(feature = "brick")]
    pub(crate) fn is_native_brick_pcm(&self) -> bool {
        matches!(self, Sink::Alsa { observation, .. } if observation.native_pause.is_some())
    }

    /// Finish the pause fade, then leave the native Brick PCM prepared with its
    /// clock stopped. A menu needs no continuously running stream of silence.
    /// Pulse/BlueALSA retain their streaming policy. Game buffer sizes stay fixed.
    #[cfg(feature = "brick")]
    pub(crate) fn park_after_pause(&mut self) -> bool {
        let Sink::Alsa { pcm, recover, observation, .. } = self else { return false };
        let Some((drain, prepare)) = observation.native_pause else { return false };
        if pcm.is_null() { return false; }
        for operation in [drain, prepare] {
            let result = unsafe { operation(*pcm) };
            if result < 0 {
                // Preserve real PCM failures even on the pause-control path.
                if let Some(d) = &observation.writer.diagnostics { d.alsa_write_error(result as i64); }
                let failed = unsafe { recover(*pcm, result, 1) } < 0;
                if let Some(d) = &observation.writer.diagnostics { d.alsa_recover(failed); }
                return false;
            }
        }
        true
    }

    /// Blocks until the output has room; false when the stream is gone.
    pub(crate) fn write(&mut self, samples: &[i16]) -> bool {
        #[cfg(feature = "brick")]
        if self.closed() {
            return false;
        }
        match self {
            Sink::Pulse {
                stream, write, observation, ..
            } => {
                let started = observation.writer.begin();
                let mut err = 0;
                let success = unsafe { write(*stream, samples.as_ptr() as *const c_void, samples.len() * 2, &mut err) >= 0 };
                if success {
                    if let Some(d) = &observation.writer.diagnostics {
                        d.accepted_frames((samples.len() / 2) as u64);
                    }
                }
                observation.writer.finish(started, success);
                success
            }
            Sink::Alsa {
                pcm,
                writei,
                recover,
                delay,
                observation,
                ..
            } => {
                let started = observation.writer.begin();
                let success = write_alsa(*pcm, *writei, *recover, samples, observation.writer.diagnostics.as_deref());
                observation.writer.finish(started, success);
                observation.sample(*pcm, *delay);
                success
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
            Sink::Alsa { pcm, delay, observation, .. } => {
                let mut frames: c_long = 0;
                let result = unsafe { delay(*pcm, &mut frames) };
                if result < 0 {
                    // The rate-control query also used to hide these failures behind a zero latency.
                    // Preserve that playback fallback, but retain every failed query in the evidence.
                    if let Some(d) = &observation.writer.diagnostics {
                        d.alsa_delay(Err(result as i64));
                    }
                    return 0;
                }
                frames.max(0) as u64 * 1_000_000 / OUT_RATE as u64
            }
        }
    }
}

fn write_alsa(pcm: *mut c_void, writei: SndWritei, recover: SndRecover, samples: &[i16], diagnostics: Option<&Diagnostics>) -> bool {
    let mut rest = samples;
    while !rest.is_empty() {
        let requested = rest.len() / 2;
        let n = unsafe { writei(pcm, rest.as_ptr() as *const c_void, requested as c_ulong) };
        if n < 0 {
            if let Some(d) = diagnostics {
                d.alsa_write_error(n as i64);
            }
            // An xrun (-EPIPE) or a suspend: count even a silent successful recovery, then retry the chunk.
            let failed = unsafe { recover(pcm, n as c_int, 1) } < 0;
            if let Some(d) = diagnostics {
                d.alsa_recover(failed);
            }
            if failed {
                return false;
            }
            continue;
        }
        if n == 0 {
            if let Some(d) = diagnostics {
                d.alsa_zero_write();
            }
            // No progress must reopen through the caller, rather than spin forever on this writer.
            return false;
        }
        if n as usize > requested {
            if let Some(d) = diagnostics {
                d.alsa_write_error(n as i64);
            }
            return false;
        }
        if let Some(d) = diagnostics {
            d.accepted_frames(n as u64);
            if (n as usize) < requested {
                d.alsa_short_write();
            }
        }
        rest = &rest[n as usize * 2..];
    }
    true
}

/// The ALSA name the Brick opens: "pocket_bt:DEV=<address>" (its asoundrc: BlueALSA behind a software volume) while a headset is the
/// route, else the given device or "pipewire" (the chip). Only a plain Bluetooth address goes into the name: anything else could add
/// arguments, so it counts as no headset.
#[cfg(feature = "brick")]
pub(crate) fn brick_device(route: Option<&str>, env: Option<String>) -> String {
    let address = route.filter(|a| a.len() == 17 && a.bytes().enumerate().all(|(i, b)| if i % 3 == 2 { b == b':' } else { b.is_ascii_hexdigit() }));
    match address {
        Some(a) => format!("pocket_bt:DEV={a}"),
        None => env.unwrap_or_else(|| "pipewire".into()),
    }
}

#[cfg(all(test, feature = "brick"))]
mod brick_tests {
    use super::brick_device;

    #[test]
    fn the_brick_opens_its_headphones_by_address_and_nothing_odd_goes_into_the_name() {
        assert_eq!(brick_device(Some("AA:BB:CC:DD:EE:0f"), None), "pocket_bt:DEV=AA:BB:CC:DD:EE:0f");
        assert_eq!(brick_device(None, None), "pipewire", "no headset: the chip");
        assert_eq!(brick_device(None, Some("hw:0".into())), "hw:0");
        assert_eq!(brick_device(Some("AA:BB:CC:DD:EE:FF,PROFILE=sco"), None), "pipewire", "an argument smuggled in: no headset");
        assert_eq!(brick_device(Some("AA-BB-CC-DD-EE-FF"), None), "pipewire");
    }
}

/// Linux already sizes its queue for the selected PipeWire quantum.
pub(crate) fn minimum_queue_ms() -> u32 {
    0
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;


    #[test]
    fn h700_pcm_holds_a_graph_cycle_and_two_writer_chunks() {
        let quantum = default_game_quantum(true);
        assert!(!crate::audio_route::short_quantum(Some(quantum)), "the short ring cannot hold this cycle");
        let pcm_frames = cycle_latency_us(true) as usize * OUT_RATE as usize / 1_000_000;
        assert!(pcm_frames >= quantum as usize + 2 * crate::audio::CHUNK_FRAMES, "PCM must fit a graph cycle plus two writer chunks");
        assert!(pcm_frames >= quantum as usize * 2, "H700 PCM must hold two graph cycles");
        assert!(crate::audio_route::short_quantum(Some(default_game_quantum(false))), "other devices retain the measured small cycle");
    }

    struct FakePcm {
        writes: VecDeque<c_long>,
        recover_result: c_int,
        recover_silent: Vec<c_int>,
        write_calls: usize,
        delay_result: c_int,
        delay_frames: c_long,
        avail_result: c_long,
    }
    impl FakePcm {
        fn new(writes: &[c_long]) -> Self {
            Self {
                writes: writes.iter().copied().collect(),
                recover_result: 0,
                recover_silent: Vec::new(),
                write_calls: 0,
                delay_result: 0,
                delay_frames: 120,
                avail_result: 360,
            }
        }
    }
    unsafe extern "C" fn writei(pcm: *mut c_void, _: *const c_void, frames: c_ulong) -> c_long {
        let state = unsafe { &mut *pcm.cast::<FakePcm>() };
        state.write_calls += 1;
        state.writes.pop_front().unwrap_or(frames as c_long)
    }
    unsafe extern "C" fn recover(pcm: *mut c_void, _: c_int, silent: c_int) -> c_int {
        let state = unsafe { &mut *pcm.cast::<FakePcm>() };
        state.recover_silent.push(silent);
        state.recover_result
    }
    unsafe extern "C" fn delay(pcm: *mut c_void, frames: *mut c_long) -> c_int {
        let state = unsafe { &mut *pcm.cast::<FakePcm>() };
        unsafe {
            *frames = state.delay_frames;
        }
        state.delay_result
    }
    unsafe extern "C" fn avail(pcm: *mut c_void) -> c_long {
        unsafe { (*pcm.cast::<FakePcm>()).avail_result }
    }
    unsafe extern "C" fn close(_: *mut c_void) -> c_int {
        0
    }
    fn sink(state: &mut FakePcm, diagnostics: &Arc<Diagnostics>) -> Sink {
        let observation = Observation {
            avail: Some(avail),
            rate: Some(44100),
            buffer: Some(2048),
            period: Some(512),
            ..Observation::default()
        };
        let mut sink = Sink::Alsa {
            _lib: None,
            pcm: std::ptr::from_mut(state).cast(),
            writei,
            recover,
            delay,
            close,
            ring: Ring::Short,
            observation,
        };
        sink.observe(Arc::clone(diagnostics));
        sink
    }

    /// ALSA prints nothing with silent=1; the old producer-only summary still said zero underruns.
    /// The same accepted chunk and later reopen must retain evidence of that successful recovery.
    #[test]
    fn silent_epipe_recovery_is_counted_and_survives_reopen() {
        let diagnostics = Arc::new(Diagnostics::default());
        let mut state = FakePcm::new(&[-32, 2, 2]);
        {
            let mut sink = sink(&mut state, &diagnostics);
            assert!(sink.write(&[100; 8]));
        }
        assert_eq!(state.recover_silent, [1]);
        let mut next_state = FakePcm::new(&[]);
        {
            let mut sink = sink(&mut next_state, &diagnostics);
            assert!(sink.write(&[100; 8]));
        }
        let snapshot = diagnostics.snapshot();
        let alsa = snapshot.alsa.unwrap();
        assert_eq!(snapshot.opens, 2);
        assert_eq!(snapshot.write_frames, 8);
        assert_eq!(snapshot.write_chunks, 2);
        assert_eq!(alsa.xrun_errors, 1);
        assert_eq!(alsa.write_errors, 1);
        assert_eq!(alsa.recover_calls, 1);
        assert_eq!(alsa.recover_failures, 0);
        assert_eq!(alsa.short_writes, 1);
        assert_eq!(alsa.pcm_rate, Some(44100)); // Negotiated value can differ from OUT_RATE.
        assert_eq!(alsa.buffer_frames, Some(2048));
        assert_eq!(alsa.period_frames, Some(512));
        assert_eq!(alsa.delay_frames.samples, 2);
        assert_eq!(alsa.avail_frames.latest, Some(360));
    }

    #[test]
    fn failed_recovery_and_queries_are_visible_without_inventing_zero_delay() {
        let diagnostics = Arc::new(Diagnostics::default());
        let mut state = FakePcm::new(&[-32]);
        state.recover_result = -5;
        state.delay_result = -32;
        state.avail_result = -32;
        let mut sink = sink(&mut state, &diagnostics);
        assert!(!sink.write(&[100; 8]));
        let snapshot = diagnostics.snapshot();
        let alsa = snapshot.alsa.unwrap();
        assert_eq!(snapshot.write_failures, 1);
        assert_eq!(snapshot.write_frames, 0);
        assert_eq!(alsa.recover_calls, 1);
        assert_eq!(alsa.recover_failures, 1);
        assert_eq!(alsa.xrun_errors, 1);
        assert_eq!(alsa.query_xrun_errors, 2); // Two error observations, not two extra physical xruns.
        assert_eq!(alsa.delay_frames.failures, 1);
        assert_eq!(alsa.avail_frames.failures, 1);
        assert_eq!(alsa.delay_frames.latest, None);
        assert_eq!(alsa.avail_frames.latest, None);
    }

    #[test]
    fn zero_progress_fails_once_instead_of_spinning_on_the_writer() {
        let diagnostics = Arc::new(Diagnostics::default());
        let mut state = FakePcm::new(&[0]);
        {
            let mut sink = sink(&mut state, &diagnostics);
            assert!(!sink.write(&[100; 8]));
        }
        assert_eq!(state.write_calls, 1);
        let snapshot = diagnostics.snapshot();
        assert_eq!(snapshot.write_failures, 1);
        assert_eq!(snapshot.alsa.unwrap().zero_writes, 1);
    }

    #[test]
    fn rate_control_delay_failures_are_counted_between_periodic_samples() {
        let diagnostics = Arc::new(Diagnostics::default());
        let mut state = FakePcm::new(&[]);
        let mut sink = sink(&mut state, &diagnostics);
        assert!(sink.write(&[100; 8]));
        state.delay_result = -32;
        assert_eq!(sink.latency_us(), 0); // Existing playback fallback remains unchanged.
        let alsa = diagnostics.snapshot().alsa.unwrap();
        assert_eq!(alsa.delay_frames.samples, 1);
        assert_eq!(alsa.delay_frames.failures, 1);
        assert_eq!(alsa.delay_frames.latest, Some(120));
        assert_eq!(alsa.query_xrun_errors, 1);
        assert_eq!(alsa.recover_calls, 0);
    }
}
