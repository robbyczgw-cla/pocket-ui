//! Audio out to PipeWire, loaded with dlopen so nothing is linked at build time. Two ways in:
//! - ALSA's `pipewire` device (default): a native PipeWire stream. ROCKNIX's pulse layer keeps every
//!   pulse client at a quantum of at least 1024 frames and about 90 ms of buffering; this way skips it.
//! - PulseAudio's simple API, as before (POCKET_CORE_AUDIO=pulse, or when ALSA can't be opened).
//!
//! The core's samples are resampled to 48 kHz (windowed sinc, `resample`;
//! `POCKET_CORE_RESAMPLER=linear` is the old linear interpolation). The ratio is nudged by up to ±0.5 % depending on how
//! full the buffer is, averaged over about a second (dynamic rate control, as RetroArch does), so audio follows the
//! 60 Hz video without crackles or growing latency even though a Game Boy runs at 59.73 Hz.
//!
//! The ALSA buffer is sized for the graph's cycle (`Ring`): 10 ms at quantum 256, 32 ms at the larger
//! Bluetooth cycle, 64 ms on H700. scripts/audio-bench measures the whole path off the device (docs/HOST.md "Bluetooth audio").

use std::collections::VecDeque;
#[cfg(test)]
use std::ffi::{c_int,c_long,c_ulong,c_void};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};


pub const OUT_RATE: u32 = 48_000;
/// Buffered audio the rate control steers towards, in ms (POCKET_CORE_AUDIO_MS overrides it).
/// It only has to bridge the core's frame-sized bursts; the output keeps its own small buffer.
/// Measured on the Mini V2: through pulse (30 s each, mGBA and snes9x) 24 ms had no underruns,
/// 16 ms one for snes9x; on the native stream at quantum 256, 16, 20 and 24 ms had none in 60 s
/// of snes9x, and 16 ms once one. This is measured before a delivery; the writer's start reserve
/// includes that delivery (`restart_frames`) so the first rate-control reference is not a frame low.
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
const MAX_SKEW: f64 = 0.005;
/// How much of each new fill reading the rate control takes (an average over about a second at 60 pushes a second).
/// The queue's fill saws up and down by a core frame (17 ms) and by a writer chunk or a whole graph cycle (5 or 21 ms)
/// at rates that beat against each other; steering on the raw reading turned that into a pitch wobble of ±2.4 cents
/// on the speaker and ±8 cents on Bluetooth (scripts/audio-bench, 1 kHz tone). The queue itself drifts over seconds
/// (the loop's time constant is target / (MAX_SKEW × 48000), 4 s at 20 ms), so the average is fast enough to follow it.
const FILL_SMOOTH: f64 = 1.0 / 64.0;
pub(crate) const CHUNK_FRAMES: usize = 256;
/// Set (any value): `Audio::open` samples PipeWire from the first moment (`Audio::observe_pipewire`), for the benches.
pub const PW_STATS_ENV: &str = "POCKET_CORE_AUDIO_PW";

pub(crate) use crate::platform::audio_backend::{Sink, Ring, force_quantum, game_quantum};
pub use crate::platform::audio_backend::restore_quantum;

/// How long the writer keeps trying to open a new stream after its stream failed, and how often.
#[cfg(not(feature = "brick"))]
const REOPEN_FOR: std::time::Duration = std::time::Duration::from_secs(5);
/// The Brick: a headset that goes away fails the stream before its Bluetooth service points ALSA back at the speaker (it looks
/// every 3 s, `brick-bluetooth`).
#[cfg(feature = "brick")]
const REOPEN_FOR: std::time::Duration = std::time::Duration::from_secs(12);
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
    OUT_RATE as usize * ms.max(crate::platform::audio_backend::minimum_queue_ms() as usize).clamp(8, 200) / 1000
}

fn cadence_target_frames(core_fps: f64, display_hz: f64) -> usize {
    if (0.95..1.05).contains(&(display_hz / core_fps)) {
        return 0;
    }
    let ms = (2000.0 / core_fps.max(1.0)).ceil().clamp(CYCLE_TARGET_MS as f64, 200.0);
    OUT_RATE as usize * ms as usize / 1000
}

/// The regular delivery, never the sometimes very large first SPU buffer after loading a state.
fn delivery_frames(core_fps: f64, display_hz: f64) -> usize {
    // Match pocket-core's sync_audio_rate: near the display rate, run once per refresh.
    let fps = if (0.95..1.05).contains(&(display_hz / core_fps)) {
        display_hz
    } else {
        core_fps.min(display_hz)
    };
    (OUT_RATE as f64 / fps.max(1.0)).ceil() as usize
}

/// The writer tests the fill after a push, while rate control observes it before a push. Start
/// with that target plus one regular delivery; otherwise its one-second warmup adopts a
/// reference up to a source frame too low and takes over a minute to recover. This applies
/// to the 48 ms PAL/cycle target too, not only the short speaker queue.
fn restart_frames(target: usize, delivery: usize) -> usize {
    // The steady target is a before-push measurement; startup/resume is tested after a push.
    target.saturating_add(delivery)
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
    /// The Brick: headphones or the speaker (`Keeper::bluetooth_flag`).
    #[cfg(feature = "brick")]
    bluetooth: Arc<std::sync::atomic::AtomicBool>,
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
    /// Time-paced cores (e.g. PAL 50 Hz on a 60 Hz display) skip some display ticks. Their audio
    /// arrives less often than the writer's short reserve can bridge.
    cadence_target: Arc<AtomicUsize>,
    delivery: Arc<AtomicUsize>,
    rate: RateControl,
    /// The writer plays (`RateControl::step`): false before it starts and during a pause.
    playing: Arc<std::sync::atomic::AtomicBool>,
    flags: Option<Route>,
    /// The producer hands over only after the chosen route has played and its smaller reserve fits.
    /// A first writer pop alone is too early: route selection can shrink 48 ms to 20 ms meanwhile.
    starting: bool,
    /// Core rate to 48 kHz (`resample`); `scratch` is its output before it goes into the queue.
    resampler: crate::resample::Resampler,
    scratch: Vec<i16>,
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
    writer_finished: Arc<std::sync::atomic::AtomicBool>,
    diagnostics: Arc<crate::audio_diagnostics::Diagnostics>,
    queue_health: Arc<QueueHealth>,
    pw_stats: Option<crate::audio_pw_stats::Collector>,
}

/// Queue observations are separate from device xruns. Startup/refill waits and intentional silence must stay
/// visible even when the old playing-only `underruns` counter is zero.
#[derive(Default)]
struct QueueHealth {
    route_wait_polls: AtomicU64,
    prefill_waits: AtomicU64,
    prefill_missing_max: AtomicU64,
    prefill_silence_chunks: AtomicU64,
    paused_silence_chunks: AtomicU64,
    refill_silence_chunks: AtomicU64,
    playing_underruns: AtomicU64,
    playing_missing_frames: AtomicU64,
    input_frames: AtomicU64,
    resampled_frames: AtomicU64,
    dropped_frames: AtomicU64,
    preplay_dropped_frames: AtomicU64,
    startup_backpressure_us: AtomicU64,
    active_dropped_frames: AtomicU64,
    ever_played: std::sync::atomic::AtomicBool,
    /// Stored as fill + 1 so the default zero means there has not yet been a playing pop.
    min_playing_fill: AtomicU64,
    state: AtomicU64,
}

impl QueueHealth {
    /// Both pause fades emit real PCM too, including a pause entered during startup prefill.
    fn fade_chunk(&self, queue: &mut VecDeque<i16>, chunk: &mut [i16]) {
        self.ever_played.store(true, Ordering::Relaxed);
        for sample in chunk { *sample = queue.pop_front().unwrap_or(0); }
    }

    fn enqueue(&self, queue: &mut VecDeque<i16>, samples: &[i16], target: usize, restart: usize) -> usize {
        // A repeated display picture (PAL on a 60 Hz screen) supplies nothing. Trimming here
        // threw away the resume reserve without replacing it, then starved the next real frame.
        if samples.is_empty() {
            return 0;
        }
        // A deliberately tiny bench target must still be able to reach its restart threshold.
        let limit = (target * 3).max(restart);
        let active = self.ever_played.load(Ordering::Relaxed);
        let fill = queue.len() / 2;
        let dropped = if active && fill > limit { fill - target } else { 0 };
        queue.drain(..dropped * 2);
        queue.extend(samples);
        self.record_drop(dropped, active);
        dropped
    }

    fn record_drop(&self, dropped: usize, active: bool) {
        self.dropped_frames.fetch_add(dropped as u64, Ordering::Relaxed);
        let counter = if active { &self.active_dropped_frames } else { &self.preplay_dropped_frames };
        counter.fetch_add(dropped as u64, Ordering::Relaxed);
    }

    /// Before the first sample, route selection/linking may take longer than the queue holds.
    /// Feed the complete first PCM in bounded pieces and let the writer make room. Cutting the
    /// prefix here skipped hundreds of milliseconds on a slow link; keeping an unbounded backlog
    /// instead would merely move that skip to the first active overflow check.
    fn enqueue_startup(&self, queue: &Mutex<VecDeque<i16>>, samples: &[i16], target: usize, restart: usize) -> usize {
        let mut remaining = samples;
        let mut progress = std::time::Instant::now();
        while !remaining.is_empty() {
            {
                let mut q = queue.lock().unwrap();
                let room = (restart * 2).saturating_sub(q.len());
                let count = remaining.len().min(room) / 2 * 2;
                if count > 0 {
                    self.enqueue(&mut q, &remaining[..count], target, restart);
                    remaining = &remaining[count..];
                    progress = std::time::Instant::now();
                    continue;
                }
            }
            // A broken/closed output must not trap the emulation loop. Keep the loss visible,
            // as with the active overflow counter, rather than claim an intact PCM stream.
            if self.state.load(Ordering::Relaxed) == 4 || progress.elapsed() >= std::time::Duration::from_secs(2) {
                let dropped = remaining.len() / 2;
                self.record_drop(dropped, self.ever_played.load(Ordering::Relaxed));
                eprintln!("audio: output did not accept startup PCM ({dropped} frames lost)");
                return dropped;
            }
            let t = std::time::Instant::now();
            std::thread::sleep(std::time::Duration::from_millis(1));
            self.startup_backpressure_us.fetch_add(t.elapsed().as_micros() as u64, Ordering::Relaxed);
        }
        0
    }

    /// A late core frame must not throw away the audio already available. Like a threaded ALSA FIFO,
    /// consume the valid prefix and pad only the missing tail, keeping channel pairs in order.
    fn playing_chunk(&self, queue: &mut VecDeque<i16>, chunk: &mut [i16]) -> usize {
        self.ever_played.store(true, Ordering::Relaxed);
        let available = queue.len().min(chunk.len());
        for sample in &mut chunk[..available] {
            *sample = queue.pop_front().unwrap();
        }
        chunk[available..].fill(0);
        let missing = (chunk.len() - available) / 2;
        self.observe_fill(queue.len() / 2);
        if missing > 0 {
            self.playing_underruns.fetch_add(1, Ordering::Relaxed);
            self.playing_missing_frames.fetch_add(missing as u64, Ordering::Relaxed);
        }
        missing
    }

    fn observe_fill(&self, frames: usize) {
        let value = frames as u64 + 1;
        // Only the writer updates this minimum; diagnostic readers never reset it.
        let old = self.min_playing_fill.load(Ordering::Relaxed);
        self.min_playing_fill.store(if old == 0 { value } else { old.min(value) }, Ordering::Relaxed);
    }

    fn prefill_wait(&self, missing: usize) {
        self.prefill_waits.fetch_add(1, Ordering::Relaxed);
        self.prefill_missing_max.fetch_max(missing as u64, Ordering::Relaxed);
    }

    fn snapshot(&self) -> serde_json::Value {
        let read = |a: &AtomicU64| a.load(Ordering::Relaxed);
        serde_json::json!({
            "state": match read(&self.state) { 0 => "prefill", 1 => "playing", 2 => "paused", 3 => "refill", _ => "stopped" },
            "prefill_wait_polls": read(&self.prefill_waits),
            "route_wait_polls": read(&self.route_wait_polls),
            "prefill_missing_max_frames": read(&self.prefill_missing_max),
            "prefill_silence_chunks": read(&self.prefill_silence_chunks),
            "paused_silence_chunks": read(&self.paused_silence_chunks),
            "refill_silence_chunks": read(&self.refill_silence_chunks),
            "playing_underruns": read(&self.playing_underruns),
            "playing_silence_frames": read(&self.playing_missing_frames),
            "playing_missing_frames": read(&self.playing_missing_frames),
            "input_frames": read(&self.input_frames), "resampled_frames": read(&self.resampled_frames),
            "dropped_frames": read(&self.dropped_frames),
            "preplay_dropped_frames": read(&self.preplay_dropped_frames),
            "startup_backpressure_us": read(&self.startup_backpressure_us),
            "active_dropped_frames": read(&self.active_dropped_frames),
            "min_playing_fill_frames": read(&self.min_playing_fill).checked_sub(1)
        })
    }
}

/// How long `Audio::drop` waits for the writer thread to close the stream; a write blocks for one chunk at most
/// unless the server hangs, and then the thread is left to end on its own.
const STOP_WAIT: std::time::Duration = std::time::Duration::from_millis(300);

impl Drop for Audio {
    fn drop(&mut self) {
        // The writer thread used to outlive this (SOL-13): waiting forever for the prefill, or writing silence.
        self.stop.store(true, Ordering::Relaxed);
        let _ = self.ended.recv_timeout(STOP_WAIT);
        let pw = self.pw_stats.as_mut().map(|s| s.finish());
        println!("audio session {}", self.diagnostics_value(pw));
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
        // Fit a known large cycle at the first open. Opening a 10 ms ring under a configured 512/1024
        // quantum and then replacing it could xrun during startup, before a single core sample played.
        // Android keeps its own low-latency stream policy.
        let configured = game_quantum();
        #[cfg(target_os = "linux")]
        let ring = if !crate::audio_route::short_quantum(Some(configured)) { Ring::Cycle } else { Ring::Short };
        #[cfg(not(target_os = "linux"))]
        let ring = Ring::Short;
        let sink = Sink::open(ring)?;
        // The pulse path can't go below 1024 per client anyway; only the native stream gains. The quantum is set on
        // the keeper's thread (it may ask PipeWire for the default sink first), not here: this only starts it.
        let frames = if sink.uses_graph() { configured } else { 0 };
        let route = Some(crate::audio_route::Keeper::spawn(frames));
        let mut audio = Audio::start(sink, in_rate, heavy, route, Sink::open);
        audio.opened(std::env::var_os(PW_STATS_ENV).is_some());
        Some(audio)
    }

    /// After `open`: the PipeWire sampler only for a bench that asks (`PW_STATS_ENV`, or `audio stats` later through
    /// `observe_pipewire`). In every game it ran `pw-dump` twice and `pw-top` every 2 s, about 7 processes a second
    /// beside the game (vdev, review 0.3), on the H700's A53 cores too.
    fn opened(&mut self, observe: bool) {
        if observe {
            self.observe_pipewire();
        }
    }

    /// Starts sampling PipeWire's per-node errors for `diagnostics` (`audio_pw_stats`), once; a no-op when running.
    pub fn observe_pipewire(&mut self) {
        if self.pw_stats.is_none() {
            self.pw_stats = Some(crate::audio_pw_stats::Collector::start());
        }
    }

    /// The writer thread for `sink`, and the queue it plays from. `reopen` opens a new stream when this one fails
    /// (`REOPEN_FOR`; the tests give one that can't).
    fn start(sink: Sink, in_rate: f64, heavy: bool, route: Option<crate::audio_route::Keeper>, reopen: fn(Ring) -> Option<Sink>) -> Audio {
        #[cfg(not(feature = "brick"))]
        let flags = route.as_ref().map(|k| Route { small: k.small_flag(), decided: k.decided_flag() });
        #[cfg(feature = "brick")]
        let flags = route.as_ref().map(|k| Route { small: k.small_flag(), decided: k.decided_flag(), bluetooth: k.bluetooth_flag() });
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
        let diagnostics = Arc::new(crate::audio_diagnostics::Diagnostics::default());
        sink.observe(diagnostics.clone());
        let queue_health = Arc::new(QueueHealth::default());
        let health = queue_health.clone();
        let writer_diagnostics = diagnostics.clone();
        let queue = Arc::new(Mutex::new(VecDeque::with_capacity(OUT_RATE as usize)));
        let underruns = Arc::new(Mutex::new(0));
        let underrun_at = Arc::new(Mutex::new(Vec::new()));
        let server_latency_us = Arc::new(AtomicU64::new(0));
        let paused = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let (ended_tx, ended) = std::sync::mpsc::channel();
        let playing = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let writer_finished = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let finished = writer_finished.clone();
        let (q, u, ua, lat, p, halt, fl, started) =
            (queue.clone(), underruns.clone(), underrun_at.clone(), server_latency_us.clone(), paused.clone(), stop.clone(), flags.clone(), playing.clone());
        let stopped = move || halt.load(Ordering::Relaxed);
        // The queue's target for each `Ring` (read once: POCKET_CORE_AUDIO_MS).
        let targets = (target_frames(heavy, true), target_frames(heavy, false));
        let cadence_target = Arc::new(AtomicUsize::new(0));
        let delivery = Arc::new(AtomicUsize::new(OUT_RATE as usize / 60));
        let writer_delivery = delivery.clone();
        let cadence = cadence_target.clone();
        let target_for = move |ring: Ring| {
            let base = if ring == Ring::Short { targets.0 } else { targets.1 };
            base.max(cadence.load(Ordering::Relaxed))
        };
        std::thread::spawn(move || {
            // Runs until the stream fails or `Audio` is dropped; then the stream is closed and the drop hears of it.
            let mut play = || {
                let mut chunk = vec![0i16; CHUNK_FRAMES * 2];
                // The keeper's first look (speaker or Bluetooth, the quantum set) decides the buffer: waited for here,
                // beside the core's first frames, not in `open`.
                let t0 = std::time::Instant::now();
                while fl.as_ref().is_some_and(|f| !f.decided.load(Ordering::Relaxed)) && t0.elapsed() < ROUTE_WAIT {
                    health.route_wait_polls.fetch_add(1, Ordering::Relaxed);
                    if stopped() {
                        return;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(2));
                }
                crate::trace::step("audio: route wait ended");
                // The server swallows data at wire speed until its own buffer is full (about 100 ms on this
                // PipeWire). Fill that with silence first; otherwise it empties our queue in one go
                // and the first seconds glitch. The first blocked write can still be stream linking:
                // a second paced write proves that the following fast buffer-fill has ended too.
                let silence = vec![0i16; CHUNK_FRAMES * 2];
                let prefill = |sink: &mut Sink| {
                    let mut paced = 0;
                    for _ in 0..200 {
                        if stopped() { break; }
                        let t = std::time::Instant::now();
                        health.prefill_silence_chunks.fetch_add(1, Ordering::Relaxed);
                        sink.write(&silence);
                        let elapsed = t.elapsed();
                        crate::mark!("audio-prime", "write_us={}", elapsed.as_micros());
                        if elapsed > std::time::Duration::from_millis(4) {
                            paced += 1;
                            if paced >= 2 { break; }
                        }
                    }
                };
                // A buffer for the cycle the graph runs at (`Ring`), before the first sample and whenever the output
                // or the quantum changes while the game plays (headphones connected or switched off).
                #[cfg(not(feature = "brick"))]
                let fit = |sink: &mut Sink| {
                    let want = Route::ring(&fl);
                    if sink.ring().is_some_and(|r| r != want) {
                        if let Some(mut new) = reopen(want) {
                            new.observe(writer_diagnostics.clone());
                            println!("audio: {want:?} buffer ({} ms)", want.us() / 1000);
                            *sink = new;
                            prefill(sink);
                        }
                    }
                };
                // The Brick also opens it again when the headphones came or went: ALSA's "pipewire" then names BlueALSA or the
                // chip (`audio_route::brick_route`). A headset whose stream won't open yet is tried again every half second.
                #[cfg(feature = "brick")]
                let mut on_bt = fl.as_ref().is_some_and(|r| r.bluetooth.load(Ordering::Relaxed));
                #[cfg(feature = "brick")]
                let mut next_try = std::time::Instant::now();
                #[cfg(feature = "brick")]
                let mut fit = |sink: &mut Sink| {
                    let want = Route::ring(&fl);
                    let bt = fl.as_ref().is_some_and(|r| r.bluetooth.load(Ordering::Relaxed));
                    let moved = bt != on_bt;
                    if (sink.ring().is_some_and(|r| r != want) || moved) && std::time::Instant::now() >= next_try {
                        if let Some(mut new) = reopen(want) {
                            new.observe(writer_diagnostics.clone());
                            println!("audio: {want:?} buffer ({} ms){}", want.us() / 1000, if moved { if bt { ", to the headphones" } else { ", to the speaker" } } else { "" });
                            *sink = new;
                            on_bt = bt;
                            prefill(sink);
                        } else {
                            next_try = std::time::Instant::now() + std::time::Duration::from_millis(500);
                        }
                    }
                };
                // Opened again before the queue fills, not after: the silence in it blocks until WirePlumber has linked
                // the new stream, and the core's frames meanwhile overfilled the queue by up to 100 ms, which the rate
                // control then drained at +5 cents for ten seconds (scripts/audio-bench).
                fit(&mut sink);
                // Match the before-push rate-control reference, rather than starting a delivery low.
                loop {
                    let buffered = q.lock().unwrap().len() / 2;
                    let target = restart_frames(target_for(Route::ring(&fl)), writer_delivery.load(Ordering::Relaxed));
                    if buffered >= target { break; }
                    health.prefill_wait(target - buffered);
                    if stopped() {
                        return;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(2));
                }
                // Prime immediately before consuming PCM: doing this before the queue waits
                // lets a newly running server drain its silence and burst-read the first PCM.
                prefill(&mut sink);
                crate::trace::step("audio: stream primed");
                started.store(true, Ordering::Relaxed);
                health.state.store(1, Ordering::Relaxed);
                let start = std::time::Instant::now();
                let mut written = 0u64;
                // Startup uses the same short fade-in as resume, including after streaming a large
                // restored SPU packet. A pause before the first sample stays silent too.
                let mut was_paused = true;
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
                        let target = target_for(Route::ring(&fl));
                        if paused {
                            health.state.store(2, Ordering::Relaxed);
                            if !was_paused && q.len() >= chunk.len() {
                                health.fade_chunk(&mut q, &mut chunk);
                                ramp(&mut chunk, true);
                            } else {
                                chunk.fill(0);
                                health.paused_silence_chunks.fetch_add(1, Ordering::Relaxed);
                            }
                            was_paused = true;
                            // The rate control starts afresh after the pause (`RateControl::step`): the queue comes
                            // back up to a core frame off where it was, and steering that back glided the pitch 2 cents.
                            started.store(false, Ordering::Relaxed);
                        } else if was_paused {
                            // Back from a pause: wait until the core has refilled the queue, then fade in.
                            if q.len() >= restart_frames(target, writer_delivery.load(Ordering::Relaxed)) * 2 {
                                health.fade_chunk(&mut q, &mut chunk);
                                health.observe_fill(q.len() / 2);
                                ramp(&mut chunk, false);
                                was_paused = false;
                                started.store(true, Ordering::Relaxed);
                                health.state.store(1, Ordering::Relaxed);
                                // After the pause menu or a wake (`latency.py measure --wake`: the sound is back).
                                crate::trace::step("audio: playing again");
                            } else {
                                chunk.fill(0);
                                health.state.store(3, Ordering::Relaxed);
                                health.refill_silence_chunks.fetch_add(1, Ordering::Relaxed);
                            }
                        } else if health.playing_chunk(&mut q, &mut chunk) != 0 {
                            crate::mark!("audio-underrun", "missing_total={}", health.playing_missing_frames.load(Ordering::Relaxed));
                            *u.lock().unwrap() += 1;
                            let mut at = ua.lock().unwrap();
                            if at.len() < 1000 {
                                at.push(start.elapsed().as_secs_f32());
                            }
                        }
                    }
                    crate::mark!("audio-write", "queued={} state={}", q.lock().unwrap().len() / 2,
                        health.state.load(Ordering::Relaxed));
                    // Blocks until the server has room: this thread runs at the sound card's pace.
                    if !sink.write(&chunk) {
                        // The stream is gone (the sound server was restarted, say around a deep sleep): a new one,
                        // instead of a game without sound until it is started again.
                        match reopen_stream(reopen, Route::ring(&fl), &stopped) {
                            Some(mut new) => {
                                new.observe(writer_diagnostics.clone());
                                sink = new;
                            }
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
            started.store(false, Ordering::Relaxed);
            health.state.store(4, Ordering::Relaxed);
            drop(sink);
            // The queue loop can stop before a blocking close returns. Only the closed stream acknowledges
            // completion; otherwise a timed-out Drop must keep writer_finished=false in the session evidence.
            finished.store(true, Ordering::Release);
            let _ = ended_tx.send(());
        });
        let mut resampler = crate::resample::Resampler::from_env();
        resampler.prepare(in_rate / OUT_RATE as f64);
        Audio {
            queue,
            in_rate,
            targets,
            cadence_target,
            delivery,
            rate: RateControl::default(),
            playing,
            flags,
            starting: true,
            resampler,
            scratch: Vec::new(),
            underruns,
            underrun_at,
            server_latency_us,
            paused,
            backend,
            route,
            stop,
            ended,
            writer_finished,
            diagnostics,
            queue_health,
            pw_stats: None,
        }
    }

    fn diagnostics_value(&self, pw: Option<crate::audio_pw_stats::Snapshot>) -> serde_json::Value {
        // Observe completion before reading its counters, so the release after close publishes the final writes.
        let writer_finished = self.writer_finished.load(Ordering::Acquire);
        serde_json::json!({
            "schema": 1, "backend": self.diagnostics.snapshot(), "queue": self.queue_health.snapshot(),
            "pipewire": pw, "playing": self.playing.load(Ordering::Relaxed),
            "queued_frames": self.queue.lock().unwrap().len() / 2,
            "writer_finished": writer_finished,
        })
    }

    /// Live session counters for benches. Null PipeWire data means no collector, never zero client xruns.
    pub fn diagnostics(&self) -> serde_json::Value {
        self.diagnostics_value(self.pw_stats.as_ref().map(|s| s.snapshot()))
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
        crate::mark!("audio-pause", "{on}");
        self.paused.store(on, std::sync::atomic::Ordering::Relaxed);
    }

    /// The queue the rate control steers towards now, in frames (`target_frames`).
    fn target(&self) -> usize {
        let base = if Route::ring(&self.flags) == Ring::Short {
            self.targets.0
        } else {
            self.targets.1
        };
        base.max(self.cadence_target.load(Ordering::Relaxed))
    }

    /// Adds one video frame's worth of core samples (interleaved stereo).
    pub fn push(&mut self, samples: &[i16]) {
        let target = self.target();
        let fill = self.queue.lock().unwrap().len() / 2;
        let ratio = self.in_rate / OUT_RATE as f64;
        let pushed = samples.len() as f64 / 2.0 / ratio;
        let step = self.rate.step(ratio, fill, target, pushed, self.playing.load(Ordering::Relaxed));
        // Converted before the queue is locked again: the writer thread must not wait for the filter.
        self.scratch.clear();
        self.resampler.process(samples, step, &mut self.scratch);
        self.queue_health.input_frames.fetch_add(samples.len() as u64 / 2, Ordering::Relaxed);
        self.queue_health.resampled_frames.fetch_add(self.scratch.len() as u64 / 2, Ordering::Relaxed);
        let restart = restart_frames(target, self.delivery.load(Ordering::Relaxed));
        let dropped = if self.starting {
            let dropped = self.queue_health.enqueue_startup(&self.queue, &self.scratch, target, restart);
            // Keep flow control through the first playing pop. The producer may have queued for
            // 48 ms before the keeper chose 20 ms; the next call must drain that valid prefix,
            // not feed it to the active overflow cap. Recheck the live target after the wait.
            if self.queue_health.ever_played.load(Ordering::Relaxed)
                && self.flags.as_ref().is_none_or(|f| f.decided.load(Ordering::Relaxed))
                && target == self.target()
                && self.queue.lock().unwrap().len() / 2 <= restart
            {
                self.starting = false;
            }
            dropped
        } else {
            // Keep the existing active latency cap if the core ran far ahead.
            self.queue_health.enqueue(&mut self.queue.lock().unwrap(), &self.scratch, target, restart)
        };
        if dropped > 0 {
            // The average would remember the overfull queue for a second.
            self.rate.fill = Some(target as f64);
        }
        crate::mark!("audio-push", "input={} output={} queued={} playing={}", samples.len() / 2,
            self.scratch.len() / 2, self.queue.lock().unwrap().len() / 2, self.playing.load(Ordering::Relaxed));
    }

    /// Effective rate of the samples pushed per second of real time.
    pub fn set_input_rate(&mut self, rate: f64) {
        self.resampler.prepare(rate / OUT_RATE as f64);
        self.in_rate = rate;
    }

    /// Match the reserve to the producer's cadence, also when a core changes video mode. Near the
    /// display rate it supplies every tick; otherwise use the longer queue and at least two source
    /// frames. This is a queue minimum, like the backend's minimum, not a resampler ratio change.
    pub fn set_frame_timing(&self, core_fps: f64, display_hz: f64) {
        self.cadence_target.store(cadence_target_frames(core_fps, display_hz), Ordering::Relaxed);
        self.delivery.store(delivery_frames(core_fps, display_hz), Ordering::Relaxed);
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

    #[test]
    fn pausing_during_prefill_keeps_the_pcm_after_the_fade_continuous() {
        let health = QueueHealth::default();
        let mut queue = VecDeque::from(vec![1234; 2304 * 2]);
        let mut chunk = [0; CHUNK_FRAMES * 2];
        health.fade_chunk(&mut queue, &mut chunk);
        assert!(chunk.iter().all(|&sample| sample == 1234));
        // Some sound already played through the fade. A later packet cannot be treated as
        // inaudible startup backlog and discard the remainder of that PCM prefix.
        health.enqueue(&mut queue, &vec![2345; 8070 * 2], 2304, 2304);
        assert_eq!(health.playing_chunk(&mut queue, &mut chunk), 0);
        assert!(chunk.iter().all(|&sample| sample == 1234), "startup trim skipped PCM after a pause fade");
    }

    /// The queue must preserve a restored SPU packet's beginning while waiting for a stream.
    /// Production feeds it in bounded pieces (`enqueue_startup`), tested with the real writer below.
    #[test]
    fn an_oversized_state_packet_keeps_its_beginning_before_playback() {
        let health = QueueHealth::default();
        let mut queue = VecDeque::new();
        let target = 2304;
        let first: Vec<i16> = (1..=8070i16).flat_map(|sample| [sample, -sample]).collect();
        assert_eq!(health.enqueue(&mut queue, &first, target, target), 0);
        let mut expected = VecDeque::from(first.clone());
        let mut chunk = [0; CHUNK_FRAMES * 2];
        assert_eq!(health.playing_chunk(&mut queue, &mut chunk), 0);
        for sample in chunk { assert_eq!(Some(sample), expected.pop_front()); }
        // Let the writer make room before the next regular producer packet.
        while queue.len() / 2 > target {
            assert_eq!(health.playing_chunk(&mut queue, &mut chunk), 0);
            for sample in chunk { assert_eq!(Some(sample), expected.pop_front()); }
        }
        let next = vec![9000; 960 * 2];
        assert_eq!(health.enqueue(&mut queue, &next, target, target), 0);
        expected.extend(&next);
        while queue.len() >= chunk.len() {
            assert_eq!(health.playing_chunk(&mut queue, &mut chunk), 0);
            for sample in chunk { assert_eq!(Some(sample), expected.pop_front()); }
        }
        assert_eq!(health.preplay_dropped_frames.load(Ordering::Relaxed), 0);
        assert_eq!(health.active_dropped_frames.load(Ordering::Relaxed), 0);
    }

    /// Stream linking must preserve the first sound, also for a restored SPU packet larger
    /// than the software queue. Exercise the real producer and writer, including the fade.
    #[test]
    fn a_delayed_stream_plays_the_first_pcm_without_discarding_the_start() {
        struct Capture {
            chunks: Arc<Mutex<Vec<Vec<i16>>>>,
            first: bool,
        }
        unsafe extern "C" fn write(stream: *mut c_void, data: *const c_void, bytes: usize, _: *mut c_int) -> c_int {
            let capture = unsafe { &mut *stream.cast::<Capture>() };
            let ms = if capture.first { capture.first = false; 100 } else { 1 };
            std::thread::sleep(std::time::Duration::from_millis(ms));
            let samples = unsafe { std::slice::from_raw_parts(data.cast::<i16>(), bytes / 2) };
            capture.chunks.lock().unwrap_or_else(|e| e.into_inner()).push(samples.to_vec());
            0
        }
        unsafe extern "C" fn free(stream: *mut c_void) {
            drop(unsafe { Box::from_raw(stream.cast::<Capture>()) });
        }
        let chunks = Arc::new(Mutex::new(Vec::new()));
        let sink = Sink::Pulse {
            _lib: None,
            stream: Box::into_raw(Box::new(Capture { chunks: chunks.clone(), first: true })).cast(),
            write, latency: fake_latency, free, observation: Default::default(),
        };
        let mut audio = Audio::start(sink, 48000.0, true, None, |_| None);
        let input: Vec<i16> = (1000..9070i16).flat_map(|n| [n, -n]).collect();
        audio.push(&input);
        let expected = audio.scratch.clone();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        loop {
            let pcm = chunks.lock().unwrap();
            let sounding: Vec<_> = pcm.iter().filter(|c| c.iter().any(|&s| s != 0)).collect();
            if sounding.len() >= 2 {
                let second = sounding[1].to_vec();
                drop(pcm);
                assert!(second.as_slice() == &expected[CHUNK_FRAMES * 2..CHUNK_FRAMES * 4],
                    "the first PCM prefix was replaced by a later part of the game sound");
                break;
            }
            drop(pcm);
            assert!(std::time::Instant::now() < deadline, "first game sound never played");
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert_eq!(audio.queue_health.dropped_frames.load(Ordering::Relaxed), 0,
            "stream setup discarded game sound");
    }

    /// A linking write can block before the server starts accepting its real buffer. Do not
    /// spend the beginning of the game filling the fast writes that follow that first block.
    #[test]
    fn linking_the_stream_does_not_burst_consume_the_opening_pcm() {
        struct Connecting { writes: usize }
        unsafe extern "C" fn write(stream: *mut c_void, _: *const c_void, _: usize, _: *mut c_int) -> c_int {
            let server = unsafe { &mut *stream.cast::<Connecting>() };
            if server.writes == 0 { std::thread::sleep(std::time::Duration::from_millis(100)); }
            else if server.writes >= 18 { std::thread::sleep(std::time::Duration::from_millis(6)); }
            server.writes += 1;
            0
        }
        unsafe extern "C" fn free(stream: *mut c_void) {
            drop(unsafe { Box::from_raw(stream.cast::<Connecting>()) });
        }
        let sink = Sink::Pulse { _lib: None,
            stream: Box::into_raw(Box::new(Connecting { writes: 0 })).cast(),
            write, latency: fake_latency, free, observation: Default::default() };
        let mut audio = Audio::start(sink, 48000.0, true, None, |_| None);
        audio.push(&vec![1234; 8070 * 2]);
        for _ in 0..8 {
            std::thread::sleep(std::time::Duration::from_millis(16));
            audio.push(&vec![2345; 800 * 2]);
        }
        audio.pause(true);
        assert_eq!(audio.queue_health.playing_missing_frames.load(Ordering::Relaxed), 0,
            "the first game sound was consumed at linking speed instead of the PCM clock");
        assert_eq!(audio.queue_health.dropped_frames.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn startup_backpressure_preserves_stereo_with_a_small_override() {
        use std::sync::atomic::AtomicBool;
        let health = Arc::new(QueueHealth::default());
        let queue = Arc::new(Mutex::new(VecDeque::new()));
        let done = Arc::new(AtomicBool::new(false));
        let (h, q, end) = (health.clone(), queue.clone(), done.clone());
        let restart = restart_frames(384, 800);
        let writer = std::thread::spawn(move || {
            let mut pcm = Vec::new();
            let mut playing = false;
            loop {
                {
                    let mut q = q.lock().unwrap();
                    assert!(q.len() / 2 <= restart, "startup accumulated a long audio delay");
                    playing |= q.len() / 2 >= restart;
                    if playing && q.len() >= CHUNK_FRAMES * 2 {
                        let mut chunk = [0; CHUNK_FRAMES * 2];
                        assert_eq!(h.playing_chunk(&mut q, &mut chunk), 0);
                        pcm.extend(chunk);
                    } else if end.load(Ordering::Acquire) {
                        pcm.extend(q.drain(..));
                        break;
                    }
                }
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
            pcm
        });
        let input: Vec<i16> = (1..=8070i16).flat_map(|n| [n, -n]).collect();
        let dropped = health.enqueue_startup(&queue, &input, 384, restart);
        done.store(true, Ordering::Release);
        let heard = writer.join().unwrap();
        assert_eq!(dropped, 0);
        assert_eq!(heard, input, "the opening sound lost or reordered stereo samples");
    }

    #[test]
    fn a_short_route_preserves_pcm_queued_before_the_first_playing_pop() {
        use std::sync::atomic::AtomicBool;
        let small = Arc::new(AtomicBool::new(false));
        let decided = Arc::new(AtomicBool::new(false));
        let sink = Sink::Pulse { _lib: None, stream: std::ptr::NonNull::<c_void>::dangling().as_ptr(),
            write: fake_write, latency: fake_latency, free: fake_free, observation: Default::default() };
        let mut audio = Audio::start_with(sink, 48000.0, false, None,
            Some(Route { small: small.clone(), decided, #[cfg(feature = "brick")] bluetooth: Arc::new(AtomicBool::new(false)) }), |_| None);
        audio.pause(true);
        // The producer filled for the still-undecided 48 ms route. The writer then chose
        // 20 ms and played one block before the producer's next call (pocket SNES startup).
        let prefix: Vec<i16> = (1..=3104i16).flat_map(|n| [n, -n]).collect();
        audio.queue.lock().unwrap().extend(&prefix);
        let mut first = [0; CHUNK_FRAMES * 2];
        audio.queue_health.playing_chunk(&mut audio.queue.lock().unwrap(), &mut first);
        small.store(true, Ordering::Relaxed);
        let done = Arc::new(AtomicBool::new(false));
        let (queue, end) = (audio.queue.clone(), done.clone());
        let consumer = std::thread::spawn(move || {
            // Give the producer both calls before draining: the old active path overflowed
            // on the second. Startup backpressure instead waits for this PCM to play.
            std::thread::sleep(std::time::Duration::from_millis(20));
            let mut pcm = Vec::new();
            loop {
                let finished = end.load(Ordering::Acquire);
                pcm.extend(queue.lock().unwrap().drain(..));
                if finished { break; }
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
            pcm
        });
        let input: Vec<i16> = (9000..9800i16).flat_map(|n| [n, -n]).collect();
        let mut expected = prefix;
        for _ in 0..2 {
            audio.push(&input);
            expected.extend(&audio.scratch);
        }
        done.store(true, Ordering::Release);
        let mut heard = first.to_vec();
        heard.extend(consumer.join().unwrap());
        assert_eq!(audio.queue_health.dropped_frames.load(Ordering::Relaxed), 0,
            "route selection cut the beginning after playback had already started");
        assert!(heard == expected, "the route handover lost or reordered the opening stereo PCM");
    }

    #[test]
    fn a_closed_stream_does_not_trap_startup_and_reports_the_unplayed_tail() {
        let health = QueueHealth::default();
        health.state.store(4, Ordering::Relaxed);
        let queue = Mutex::new(VecDeque::new());
        let samples = vec![1234; 8070 * 2];
        let lost = health.enqueue_startup(&queue, &samples, 2304, 2304);
        assert_eq!(lost + queue.lock().unwrap().len() / 2, 8070);
        assert!(lost > 0, "a closed output cannot claim it played the whole opening sound");
        assert_eq!(health.preplay_dropped_frames.load(Ordering::Relaxed), lost as u64);
    }

    /// An explicit 8 ms bench override still reaches playback and preserves the pending prefix,
    /// including when the regular delivery is conservatively rounded up.
    #[test]
    fn a_small_override_reaches_playback_without_cutting_the_waiting_prefix() {
        assert_eq!(delivery_frames(59.7275, 60.0), 800);
        for delivery in [800, 801, 804] {
            let target = 384;
            let restart = restart_frames(target, delivery);
            let health = QueueHealth::default();
            let mut queue = VecDeque::from(vec![1234; 4000 * 2]);
            for _ in 0..2 {
                health.enqueue(&mut queue, &vec![2345; 800 * 2], target, restart);
                if queue.len() / 2 >= restart { break; }
            }
            assert!(queue.len() / 2 >= restart, "writer stuck in prefill with delivery {delivery}");
            assert_eq!(queue.front(), Some(&1234), "the first sound was discarded");
        }
    }

    /// A rate-control target is measured before a delivery, not at the writer's after-push start.
    /// Preserve valid stereo samples through the fourth writer block before the next delivery.
    #[test]
    fn a_sixty_hz_frame_and_the_writers_next_block_fit_without_a_zero_tail() {
        let health = QueueHealth::default();
        let target = OUT_RATE as usize * TARGET_MS / 1000;
        let mut queue = VecDeque::from(vec![1234i16; restart_frames(target, 800) * 2]);
        let mut chunk = [0; CHUNK_FRAMES * 2];
        for _ in 0..4 {
            assert_eq!(health.playing_chunk(&mut queue, &mut chunk), 0,
                "writer inserted silence just before the next 60 Hz frame");
            assert!(chunk.iter().all(|&s| s == 1234));
        }
        health.enqueue(&mut queue, &vec![2345; 800 * 2], target, restart_frames(target, 800));
        assert_eq!(health.playing_chunk(&mut queue, &mut chunk), 0);
        assert!(chunk.iter().all(|&s| s == 1234 || s == 2345));
    }

    /// Use the real resampler, controller and queue over 20 resumes and a long final play window.
    /// Deliveries can be 10 ms late within an otherwise regular 60 Hz display interval; the
    /// writer may consume two blocks together. The old restart rule adopts a reference one
    /// delivery too low. Waiting for a delivery fixes that without raising the steady target.
    #[test]
    fn repeated_resumes_keep_pcm_continuous_after_rate_control_has_settled() {
        fn replay(reserve: bool, phase: u64) -> (u64, u64) {
            let target = 960;
            let threshold = if reserve { restart_frames(target, delivery_frames(59.7275, 60.0)) } else { target };
            let health = QueueHealth::default();
            let mut queue = VecDeque::new();
            let mut expected = VecDeque::new();
            let mut rate = RateControl::default();
            let mut resampler = crate::resample::Resampler::new(true);
            let ratio = (32768.0 * 60.0 / 59.7275) / OUT_RATE as f64;
            resampler.prepare(ratio);
            let (mut writer_index, mut next_write) = (0u64, phase);
            let (mut source_frame, mut input_frames) = (0, 0);
            let (mut playing, mut paused) = (false, false);
            let (mut missing, mut produced, mut consumed) = (0, 0, 0);
            let is_paused = |t: u64| (1..=20).any(|n| (n * 2_000_000..n * 2_000_000 + 500_000).contains(&t));
            let mut output = Vec::new();
            for tick in 0..10_800u64 {
                let deadline = tick * 1_000_000 / 60;
                let at = deadline + if tick % 6 == 3 { 10_000 } else { 0 };
                while next_write <= at {
                    let pause = is_paused(next_write);
                    if pause && !paused && queue.len() >= CHUNK_FRAMES * 2 {
                        queue.drain(..CHUNK_FRAMES * 2);
                        if reserve { expected.drain(..CHUNK_FRAMES * 2); }
                        consumed += CHUNK_FRAMES;
                    }
                    paused = pause;
                    if paused { playing = false; }
                    else {
                        if !playing && queue.len() >= threshold * 2 { playing = true; }
                        if playing {
                            for _ in 0..2 {
                                let mut chunk = [0; CHUNK_FRAMES * 2];
                                let absent = health.playing_chunk(&mut queue, &mut chunk);
                                missing += absent;
                                consumed += CHUNK_FRAMES - absent;
                                if reserve {
                                    for sample in chunk {
                                        assert_eq!(Some(sample), expected.pop_front(), "PCM lost or reordered after resume");
                                    }
                                }
                            }
                        }
                    }
                    writer_index += 1;
                    next_write = phase + writer_index * (2 * CHUNK_FRAMES) as u64 * 1_000_000 / OUT_RATE as u64;
                }
                if !is_paused(deadline) {
                    let end = ((source_frame + 1) as f64 * 32768.0 / 59.7275).floor() as usize;
                    let count = end - input_frames;
                    input_frames = end;
                    source_frame += 1;
                    let input: Vec<i16> = (0..count).flat_map(|i| {
                        let sample = 1000 + ((input_frames + i) % 3000) as i16;
                        [sample, -sample]
                    }).collect();
                    let step = rate.step(ratio, queue.len() / 2, target, count as f64 / ratio, playing);
                    output.clear();
                    resampler.process(&input, step, &mut output);
                    produced += output.len() / 2;
                    if reserve { expected.extend(output.iter().copied()); }
                    if health.enqueue(&mut queue, &output, target, threshold) > 0 { rate.fill = Some(target as f64); }
                }
            }
            let drops = health.dropped_frames.load(Ordering::Relaxed);
            assert_eq!(produced as u64, consumed as u64 + queue.len() as u64 / 2 + drops);
            (missing as u64, drops)
        }
        assert!(replay(false, 7_000).0 > 0, "the original starvation pattern was not reproduced");
        for phase in [0, 7_000] {
            assert_eq!(replay(true, phase), (0, 0), "audible loss at writer phase {phase}");
        }
        for fps in [50.0, 59.7275, 60.0] {
            assert_eq!(restart_frames(2304, delivery_frames(fps, 60.0)), 2304 + delivery_frames(fps, 60.0),
                "the 48 ms target needs the same before/after-push alignment");
        }
    }

    /// PAL uses the longer target but formerly omitted the delivery from its restart reserve.
    /// The warm controller then held a reference below the intended target. A skipped refresh
    /// and a 12 ms late delivery expose the deficit over repeated pauses, without any dropped
    /// packet or backend xrun. Keep every generated stereo sample, including after warmup.
    #[test]
    fn pal_resumes_keep_pcm_when_the_warm_reference_has_settled() {
        fn replay(aligned: bool, source_phase: u64) -> (u64, u64) {
            let target = cadence_target_frames(50.0, 60.0);
            let delivery = delivery_frames(50.0, 60.0);
            let restart = if aligned { restart_frames(target, delivery) } else { target };
            let health = QueueHealth::default();
            let mut q = VecDeque::new();
            let mut expected = VecDeque::new();
            let mut rate = RateControl::default();
            let mut resampler = crate::resample::Resampler::new(true);
            let ratio = 44100.0 / OUT_RATE as f64;
            resampler.prepare(ratio);
            let mut output = Vec::new();
            let (mut writer, mut next_write) = (0u64, 6000u64);
            let (mut playing, mut paused) = (false, false);
            let mut owed = source_phase as f64 / 300.0;
            let (mut generated, mut consumed, mut missing, mut input_frames) = (0u64, 0u64, 0u64, 0usize);
            let is_paused = |t: u64| (0..20).any(|n| (8_000_000 + n * 8_200_000..11_000_000 + n * 8_200_000).contains(&t));
            for tick in 0..14_400u64 {
                let deadline = tick * 1_000_000 / 60;
                let at = deadline + if tick % 6 == 3 { 12_000 } else { 0 };
                while next_write <= at {
                    let pause = is_paused(next_write);
                    if pause && !paused && q.len() >= CHUNK_FRAMES * 2 {
                        let mut chunk = [0; CHUNK_FRAMES * 2];
                        health.fade_chunk(&mut q, &mut chunk);
                        if aligned { expected.drain(..CHUNK_FRAMES * 2); }
                        consumed += CHUNK_FRAMES as u64;
                    }
                    paused = pause;
                    if paused { playing = false; }
                    else {
                        for _ in 0..2 {
                            if !playing && q.len() / 2 >= restart { playing = true; }
                            if playing {
                                let mut chunk = [0; CHUNK_FRAMES * 2];
                                let absent = health.playing_chunk(&mut q, &mut chunk);
                                missing += absent as u64;
                                consumed += (CHUNK_FRAMES - absent) as u64;
                                if aligned {
                                    for sample in chunk { assert_eq!(Some(sample), expected.pop_front(), "PAL resume lost or reordered PCM"); }
                                }
                            }
                        }
                    }
                    writer += 1;
                    next_write = 6000 + writer * (CHUNK_FRAMES * 2) as u64 * 1_000_000 / OUT_RATE as u64;
                }
                if !is_paused(deadline) {
                    // Match pocket-core::frames_due: PAL repeats one display picture in six.
                    owed += 1.0 / 60.0;
                    let runs = (owed / 0.02) as usize;
                    owed -= runs as f64 * 0.02;
                    let input: Vec<i16> = (0..runs * 882).flat_map(|i| {
                        let sample = 1000 + ((input_frames + i) % 3000) as i16;
                        [sample, -sample]
                    }).collect();
                    input_frames += input.len() / 2;
                    let step = rate.step(ratio, q.len() / 2, target, input.len() as f64 / 2.0 / ratio, playing);
                    output.clear();
                    resampler.process(&input, step, &mut output);
                    generated += output.len() as u64 / 2;
                    if aligned { expected.extend(output.iter().copied()); }
                    if health.enqueue(&mut q, &output, target, restart) > 0 { rate.fill = Some(target as f64); }
                }
            }
            let dropped = health.dropped_frames.load(Ordering::Relaxed);
            assert_eq!(generated, consumed + q.len() as u64 / 2 + dropped, "incomplete PCM accounting");
            (missing, dropped)
        }
        for phase in [2, 3] {
            assert!(replay(false, phase).0 > 0, "the old PAL reserve did not reproduce starvation");
            assert_eq!(replay(true, phase), (0, 0), "PCM loss after PAL pause phase {phase}");
        }
    }

    /// Device trace: after resume a PAL frame left 2925 output frames. Five ms later the display
    /// repeated a picture (no new samples). The old overflow cap cut the queue to 960 on that empty
    /// tick, then the writer ran dry before the next PAL frame. Replay its valid stereo prefix.
    #[test]
    fn a_repeated_pal_picture_after_resume_keeps_the_audio_until_the_next_core_frame() {
        let health = QueueHealth::default();
        let mut q = VecDeque::from(vec![1234i16; 2925 * 2]);
        health.enqueue(&mut q, &[], 960, 960);
        let mut chunk = [0; CHUNK_FRAMES * 2];
        for _ in 0..7 {
            assert_eq!(health.playing_chunk(&mut q, &mut chunk), 0, "silence before the next PAL frame");
            assert!(chunk.iter().all(|&s| s == 1234));
        }
        assert_eq!(health.snapshot()["dropped_frames"], 0, "a repeated picture discarded audible samples");
        health.enqueue(&mut q, &vec![2345; 960 * 2], 960, 960);
        assert_eq!(health.playing_chunk(&mut q, &mut chunk), 0);
    }

    /// Shift the first resumed PAL frame through every phase of a 60 Hz display. Its skipped
    /// refresh plus a 7 ms scheduling delay is longer than the old 20 ms queue, even though every
    /// emulated frame still supplies exactly 882 samples at 44100 Hz. The writer must keep playing
    /// the queued prefix until that next burst, with no inserted zeros.
    #[test]
    fn pal_resume_has_reserve_for_a_skipped_refresh_and_a_late_frame() {
        for refresh_phase in 0..6 {
            let target = cadence_target_frames(50.0, 60.0);
            let health = QueueHealth::default();
            let mut queue = VecDeque::from(vec![1234i16; target * 2]);
            let mut chunk = [0; CHUNK_FRAMES * 2];
            let gap_frames = OUT_RATE as usize / 60 * if refresh_phase == 0 { 2 } else { 1 }
                + OUT_RATE as usize * 7 / 1000;
            for _ in 0..gap_frames.div_ceil(CHUNK_FRAMES) {
                assert_eq!(health.playing_chunk(&mut queue, &mut chunk), 0,
                    "PAL resume phase {refresh_phase} ran out before its next source frame");
                assert!(chunk.iter().all(|&s| s == 1234));
            }
        }
        // A 60 Hz source on a 60 Hz display and a Game Boy near it keep the short queue.
        assert_eq!(cadence_target_frames(60.0, 60.0), 0);
        assert_eq!(cadence_target_frames(59.7275, 60.0), 0);
    }

    /// Review 0.3: every game sampled PipeWire with helper programs every 2 s (about 7 processes a second, `vdev`
    /// measure in REVIEW-OPUS-0.3-MERGES.md). A game's sound starts none of that unless a bench asks.
    #[test]
    fn a_game_starts_no_pipewire_sampler_unless_a_bench_asks() {
        let mut audio = Audio::start(paced_sink(), 48000.0, false, None, |_| None);
        audio.opened(false);
        assert!(audio.pw_stats.is_none(), "no helper programs beside a game");
        assert!(audio.diagnostics()["pipewire"].is_null(), "unknown, never zero");
        audio.observe_pipewire();
        assert!(audio.pw_stats.is_some(), "`audio stats` starts it");
        let mut bench = Audio::start(paced_sink(), 48000.0, false, None, |_| None);
        bench.opened(true);
        assert!(bench.pw_stats.is_some(), "{PW_STATS_ENV} starts it at the open");
    }

    #[test]
    fn a_late_frame_preserves_the_queued_audio_and_counts_only_missing_frames() {
        let health = QueueHealth::default();
        let mut queue = VecDeque::from(vec![100, -100, 200, -200, 300, -300]);
        let mut chunk = [99; 8];
        assert_eq!(health.playing_chunk(&mut queue, &mut chunk), 1);
        assert_eq!(chunk, [100, -100, 200, -200, 300, -300, 0, 0]);
        assert!(queue.is_empty(), "a late frame left older samples queued");
        let observed = health.snapshot();
        assert_eq!(observed["playing_underruns"], 1);
        assert_eq!(observed["playing_missing_frames"], 1);
        assert_eq!(observed["playing_silence_frames"], 1);
        // The next core frame resumes in order; no old audio leaks after the missing tail.
        queue.extend([400, -400, 500, -500, 600, -600, 700, -700]);
        assert_eq!(health.playing_chunk(&mut queue, &mut chunk), 0);
        assert_eq!(chunk, [400, -400, 500, -500, 600, -600, 700, -700]);
        assert_eq!(health.snapshot()["playing_underruns"], 1);
    }

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
            observation: Default::default(),
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
        let sink = Sink::Pulse { _lib: None, stream: std::ptr::NonNull::<c_void>::dangling().as_ptr(), write: fake_write, latency: fake_latency, free: fake_free, observation: Default::default() };
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
        Sink::Pulse { _lib: None, stream: std::ptr::NonNull::<c_void>::dangling().as_ptr(), write: paced_write, latency: fake_latency, free: counted_free, observation: Default::default() }
    }

    #[test]
    fn starving_a_running_writer_and_resuming_without_samples_are_observable() {
        let wait = |condition: &dyn Fn() -> bool| {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
            while !condition() {
                assert!(std::time::Instant::now() < deadline, "writer did not reach the requested phase");
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
        };
        let mut audio = Audio::start(paced_sink(), 48000.0, false, None, |_| None);
        wait(&|| audio.queue_health.prefill_waits.load(Ordering::Relaxed) > 0);
        assert_eq!(*audio.underruns.lock().unwrap(), 0, "old playing-only count misses startup waits");
        audio.push(&vec![100; 4800 * 2]);
        wait(&|| audio.queue_health.playing_underruns.load(Ordering::Relaxed) > 0);
        audio.pause(true);
        wait(&|| audio.queue_health.state.load(Ordering::Relaxed) == 2);
        audio.pause(false);
        wait(&|| audio.queue_health.refill_silence_chunks.load(Ordering::Relaxed) > 0);
        let counters = audio.diagnostics();
        assert!(counters["queue"]["playing_underruns"].as_u64().unwrap() > 0);
        assert!(counters["queue"]["refill_silence_chunks"].as_u64().unwrap() > 0);
        assert_eq!(counters["queue"]["state"], "refill");
        assert!(counters["pipewire"].is_null(), "a fake sink cannot prove client xruns");
    }

    #[test]
    fn a_stopped_queue_does_not_claim_a_blocked_stream_has_closed() {
        use std::sync::atomic::AtomicBool;
        struct Close {
            entered: Arc<AtomicBool>,
            release: Arc<AtomicBool>,
        }
        unsafe extern "C" fn blocked_close(stream: *mut c_void) {
            // The fake stream is owned by this one close callback, just like an opened backend handle.
            let close = unsafe { Box::from_raw(stream.cast::<Close>()) };
            close.entered.store(true, Ordering::Release);
            while !close.release.load(Ordering::Acquire) {
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
        }
        struct ReleaseOnDrop(Arc<AtomicBool>);
        impl Drop for ReleaseOnDrop {
            fn drop(&mut self) { self.0.store(true, Ordering::Release); }
        }
        let entered = Arc::new(AtomicBool::new(false));
        let release = Arc::new(AtomicBool::new(false));
        let _release_on_panic = ReleaseOnDrop(release.clone());
        let stream = Box::into_raw(Box::new(Close { entered: entered.clone(), release: release.clone() })).cast();
        let sink = Sink::Pulse { _lib: None, stream, write: paced_write, latency: fake_latency, free: blocked_close, observation: Default::default() };
        let audio = Audio::start(sink, 48000.0, false, None, |_| None);
        audio.stop.store(true, Ordering::Relaxed);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while !entered.load(Ordering::Acquire) {
            assert!(std::time::Instant::now() < deadline, "stream did not enter close");
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        let closing = audio.diagnostics();
        assert_eq!(closing["queue"]["state"], "stopped");
        assert_eq!(closing["writer_finished"], false, "queue stop is not a closed stream");
        release.store(true, Ordering::Release);
        while !audio.writer_finished.load(Ordering::Acquire) {
            assert!(std::time::Instant::now() < deadline, "stream did not finish close");
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert_eq!(audio.diagnostics()["writer_finished"], true);
    }

    /// SOL-13: the writer thread kept its stream and ran on after `Audio` was dropped, waiting forever for the prefill
    /// or writing silence. It ends now, and the stream is closed, by the time the drop returns.
    #[test]
    #[ignore = "timing tier: scripts/test-timing.sh (serial)"]
    fn timing_dropping_audio_ends_its_writer_and_closes_the_stream() {
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
        Some(Sink::Pulse { _lib: None, stream: std::ptr::NonNull::<c_void>::dangling().as_ptr(), write: counted_write, latency: fake_latency, free: fake_free, observation: Default::default() })
    }

    static SAVE_PCM_ACCEPTED: AtomicUsize = AtomicUsize::new(0);
    static SAVE_PCM_FAILED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    unsafe extern "C" fn save_pcm_write(_: *mut c_void, data: *const c_void, bytes: usize, _: *mut c_int) -> c_int {
        let pcm = unsafe { std::slice::from_raw_parts(data as *const i16, bytes / 2) };
        std::thread::sleep(std::time::Duration::from_millis(5));
        if pcm.iter().any(|s| *s != 0) {
            let frames = pcm.len() / 2;
            if !SAVE_PCM_FAILED.swap(true, Ordering::SeqCst) {
                SAVE_PCM_ACCEPTED.fetch_add(frames / 2, Ordering::SeqCst);
                return -1; // A backend may have accepted a prefix before its write failed.
            }
            SAVE_PCM_ACCEPTED.fetch_add(frames, Ordering::SeqCst);
        }
        0
    }
    fn save_pcm_sink(_: Ring) -> Option<Sink> {
        Some(Sink::Pulse { _lib: None, stream: std::ptr::NonNull::<c_void>::dangling().as_ptr(),
            write: save_pcm_write, latency: fake_latency, free: fake_free, observation: Default::default() })
    }

    #[test]
    #[ignore = "known failure: reopen loses the failed PCM tail; audio behavior needs separate measured review"]
    fn save_safety_audio_reopen_keeps_unaccepted_pcm() {
        SAVE_PCM_ACCEPTED.store(0, Ordering::SeqCst);
        SAVE_PCM_FAILED.store(false, Ordering::SeqCst);
        let audio = Audio::start(save_pcm_sink(Ring::Short).unwrap(), 48000.0, false, None, save_pcm_sink);
        audio.queue.lock().unwrap().extend(std::iter::repeat_n(12000, 4096 * 2));
        let until = std::time::Instant::now() + crate::testutil::timeout(std::time::Duration::from_secs(3));
        while (!audio.queue.lock().unwrap().is_empty() || SAVE_PCM_ACCEPTED.load(Ordering::SeqCst) == 0)
            && std::time::Instant::now() < until {
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        // Let the final popped block reach the backend before close.
        while SAVE_PCM_ACCEPTED.load(Ordering::SeqCst) < 3968 && std::time::Instant::now() < until {
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        drop(audio);
        assert_eq!(SAVE_PCM_ACCEPTED.load(Ordering::SeqCst), 4096, "reopen must not discard an unaccepted tail");
    }

    #[cfg(feature = "brick")]
    static BT_REOPENS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    #[cfg(feature = "brick")]
    fn bt_sink() -> Sink {
        Sink::Pulse { _lib: None, stream: std::ptr::NonNull::<c_void>::dangling().as_ptr(), write: paced_write, latency: fake_latency, free: fake_free, observation: Default::default() }
    }
    #[cfg(feature = "brick")]
    fn reopen_bt(_: Ring) -> Option<Sink> {
        BT_REOPENS.fetch_add(1, Ordering::SeqCst);
        Some(bt_sink())
    }

    /// The Brick has no PipeWire to move a stream: when its headphones connect or go (`audio_route::brick_route`), the game's
    /// stream is opened again, so ALSA's "pipewire" name is read afresh and the sound follows; nothing reopens without a change.
    #[cfg(feature = "brick")]
    #[test]
    fn the_brick_opens_the_game_stream_again_when_headphones_come_and_go() {
        let bluetooth = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flags = Route { small: Arc::new(false.into()), decided: Arc::new(true.into()), bluetooth: bluetooth.clone() };
        let mut audio = Audio::start_with(bt_sink(), 48000.0, false, None, Some(flags), reopen_bt);
        let wait = |n: usize| {
            let t0 = std::time::Instant::now();
            while BT_REOPENS.load(Ordering::SeqCst) < n && t0.elapsed() < std::time::Duration::from_secs(3) {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            BT_REOPENS.load(Ordering::SeqCst)
        };
        for _ in 0..10 {
            audio.push(&vec![100i16; 4800 * 2]);
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert_eq!(BT_REOPENS.load(Ordering::SeqCst), 0, "the speaker as it was: nothing opened again");
        bluetooth.store(true, Ordering::Relaxed);
        audio.push(&vec![100i16; 4800 * 2]);
        assert_eq!(wait(1), 1, "headphones connected: one new stream");
        // The new stream reports to the same counters (audio stats): without that every Brick game read 0 frames written.
        let written = |a: &Audio| a.diagnostics()["backend"]["write_frames"].as_u64().unwrap_or(0);
        let before = written(&audio);
        let t0 = std::time::Instant::now();
        while written(&audio) <= before && t0.elapsed() < std::time::Duration::from_secs(3) {
            audio.push(&vec![100i16; 4800 * 2]);
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(written(&audio) > before, "the headphones' stream is counted: {}", audio.diagnostics()["backend"]);
        bluetooth.store(false, Ordering::Relaxed);
        audio.push(&vec![100i16; 4800 * 2]);
        assert_eq!(wait(2), 2, "headphones gone: back to the speaker with one more");
        drop(audio);
    }

    /// Instant on: a stream that fails (the sound server restarted around a deep sleep) is replaced by a new one, and
    /// the game goes on with sound instead of without it until it is started again.
    #[test]
    #[ignore = "timing tier: scripts/test-timing.sh (serial)"]
    fn timing_a_failed_stream_is_opened_again() {
        let broken = Sink::Pulse { _lib: None, stream: std::ptr::NonNull::<c_void>::dangling().as_ptr(), write: failing_write, latency: fake_latency, free: fake_free, observation: Default::default() };
        let mut audio = Audio::start(broken, 48000.0, false, None, reopen_counted);
        audio.push(&vec![100i16; 48000 / 10 * 2]);
        let t0 = std::time::Instant::now();
        while REOPENED_WRITES.load(Ordering::SeqCst) == 0 && t0.elapsed() < crate::testutil::timeout(std::time::Duration::from_secs(2)) {
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
