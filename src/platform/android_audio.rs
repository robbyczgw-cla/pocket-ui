//! AAudio callback output. The mixer worker feeds a bounded atomic PCM handoff; the callback never waits.
use crate::audio::OUT_RATE;
use crate::android_pcm::{handoff_frames, Pcm, Reader};
use crate::audio_diagnostics::{Diagnostics, WriterObservation};
use std::sync::{Arc, OnceLock};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
static MIN_QUEUE_MS: AtomicU32 = AtomicU32::new(0);
static LOW_LATENCY: AtomicBool = AtomicBool::new(false);
/// The game's stream as AAudio reports it, for the player's log line (a test on a real handheld reads it over adb):
/// xruns since it opened, how far what was written is ahead of what was heard, burst and buffer in frames.
static GAME_CALLBACK_UNDERRUNS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static GAME_HANDOFF: AtomicU32 = AtomicU32::new(0);
static GAME_XRUNS: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);
static GAME_AHEAD_US: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static GAME_REPORTED_AHEAD_US: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static GAME_TIMESTAMP_FALLBACK: AtomicBool = AtomicBool::new(false);
static GAME_BURST: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);
static GAME_BUFFER: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);
/// ` out_ms=… aaudio_xruns=… burst=… buffer=…`, or nothing before a game's stream has opened.
pub(crate) fn game_line() -> String {
    let burst = GAME_BURST.load(Ordering::Relaxed);
    if burst == 0 {
        return String::new();
    }
    format!(
        " out_ms={:.1} aaudio_xruns={} burst={burst} buffer={} callback_underruns={} handoff_ms={:.1} reported_out_ms={:.1} timestamp_fallback={}",
        GAME_AHEAD_US.load(Ordering::Relaxed) as f64 / 1000.0,
        GAME_XRUNS.load(Ordering::Relaxed),
        GAME_BUFFER.load(Ordering::Relaxed),
        GAME_CALLBACK_UNDERRUNS.load(Ordering::Relaxed),
        GAME_HANDOFF.load(Ordering::Relaxed) as f64 / 48.0,
        GAME_REPORTED_AHEAD_US.load(Ordering::Relaxed) as f64 / 1000.0,
        GAME_TIMESTAMP_FALLBACK.load(Ordering::Relaxed)
    )
}
pub(crate) fn minimum_queue_ms() -> u32 {
    MIN_QUEUE_MS.load(Ordering::Relaxed)
}
pub(crate) fn low_latency() -> bool {
    LOW_LATENCY.load(Ordering::Relaxed)
}
use ndk::audio::{AudioDirection, AudioFormat, AudioPerformanceMode, AudioSharingMode, AudioStream, AudioStreamBuilder, AudioCallbackResult};
pub(crate) fn game_quantum() -> u32 {
    0
}
pub(crate) fn force_quantum(_: u32) -> bool {
    false
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Ring {
    Short,
    Cycle,
}
impl Ring {
    pub(crate) fn us(self) -> u32 {
        if self == Ring::Short { 10_000 } else { 32_000 }
    }
}
pub fn restore_quantum() {} // No global system audio policy was changed.
pub(crate) struct Sink {
    stream: AudioStream,
    ring: Ring,
    pcm: Arc<Pcm>,
    failed: Arc<AtomicBool>,
    capture: Option<Vec<i16>>,
    capture_limit: usize,
    xruns: Vec<(usize, i32)>,
    /// With the capture: (capture frame, monotonic ms, AAudio's presented frame, its time in ms), a few a second.
    trace: Vec<(usize, f64, i64, f64)>,
    observation: WriterObservation,
    /// The game's stream (not the menu sounds'): its numbers go into `game_line`, about once a second.
    game: bool,
    since_report: usize,
    local_fast: bool,
    timestamp_fallbacks: u64,
}
// SAFETY: the owning writer thread is the only caller; the stream and its close move together.
unsafe impl Send for Sink {}
impl Sink {
    /// ALSA counters remain explicitly unsupported on Android; writer heartbeat is backend independent.
    pub(crate) fn observe(&mut self, diagnostics: Arc<Diagnostics>) {
        self.observation.attach(diagnostics, "android");
    }
    pub(crate) fn uses_graph(&self) -> bool {
        false
    }
    pub(crate) fn open(ring: Ring) -> Option<Self> {
        let handoff = Arc::new(OnceLock::<Arc<Pcm>>::new());
        let failed = Arc::new(AtomicBool::new(false));
        let open = |sharing| {
            let handoff = handoff.clone();
            let mut reader: Option<Reader> = None;
            let error = failed.clone();
            Some(AudioStreamBuilder::new().ok()?
                .direction(AudioDirection::Output)
                .format(AudioFormat::PCM_I16)
                .channel_count(2)
                .sample_rate(OUT_RATE as i32)
                .sharing_mode(sharing)
                .performance_mode(AudioPerformanceMode::LowLatency)
                .frames_per_data_callback(crate::audio::CHUNK_FRAMES as i32)
                .data_callback(Box::new(move |_, data, frames| {
                    // SAFETY: AAudio supplies frames of the negotiated stereo PCM_I16 format.
                    let out = unsafe { std::slice::from_raw_parts_mut(data.cast::<i16>(), frames.max(0) as usize * 2) };
                    // Initialized before request_start. Reader construction only clones an Arc; no heap work.
                    if let Some(pcm) = handoff.get() {
                        reader.get_or_insert_with(|| Reader::new(pcm.clone())).render(out);
                    } else {
                        out.fill(0);
                    }
                    AudioCallbackResult::Continue
                }))
                .error_callback(Box::new(move |_, _| { error.store(true, Ordering::Release); }))
                .open_stream().ok()?)
        };
        let stream = open(AudioSharingMode::Exclusive).or_else(|| open(AudioSharingMode::Shared))?;
        // A slow AudioTrack or Bluetooth endpoint can request several fixed callbacks back to back.
        // Size for its actual burst rather than imposing the Odin's small 16 ms handoff on every route.
        let pcm = Pcm::new(handoff_frames(stream.frames_per_burst().max(1) as usize, crate::audio::CHUNK_FRAMES));
        let _ = handoff.set(pcm.clone());
        let target = (i64::from(ring.us()) * i64::from(OUT_RATE) / 1_000_000) as i32;
        // AudioTrack fallback devices may reject resizing. Keep their valid stream and size the shared queue
        // for their burst, rather than refusing all sound or treating them as a low-latency endpoint.
        // Two bursts reported an Android startup xrun on the Odin; three retain FAST with a 9 ms buffer.
        let _ = stream.set_buffer_size_in_frames((stream.frames_per_burst() * 3).max(if ring == Ring::Cycle { target } else { 0 }));
        if stream.sample_rate() != OUT_RATE as i32 {
            return None;
        }
        let burst_ms = (stream.frames_per_burst().max(1) as u32 * 1000).div_ceil(OUT_RATE);
        // The Odin needs two core frames at startup: 20 ms starved before the GL thread settled.
        MIN_QUEUE_MS.store((burst_ms + 8).max(32), Ordering::Relaxed);
        LOW_LATENCY.store(
            stream.performance_mode() == AudioPerformanceMode::LowLatency && burst_ms <= 10,
            Ordering::Relaxed,
        );
        stream.request_start().ok()?;
        let local_fast = stream.performance_mode() == AudioPerformanceMode::LowLatency && burst_ms <= 10
            && crate::platform::android::strings("localAudioOutput", &[&stream.device_id().to_string()]).is_ok_and(|v| v == "true");
        let capture_seconds = crate::platform::android::string("diagnosticAudioSeconds").parse::<usize>().unwrap_or(30).clamp(1, 240);
        let capture_limit = OUT_RATE as usize * 2 * capture_seconds;
        let capture = (crate::platform::android::string("diagnosticAudio") == "true").then(|| Vec::with_capacity(capture_limit));
        // In logcat, where a test on a real handheld can read it without root (stderr reaches no log in the player's process).
        log_line(
            4,
            &format!(
                "AAudio opened: rate={} burst={} buffer={} capacity={} mode={:?} sharing={:?} device={} xruns={} local_fast={local_fast}",
                stream.sample_rate(),
                stream.frames_per_burst(),
                stream.buffer_size_in_frames(),
                stream.buffer_capacity_in_frames(),
                stream.performance_mode(),
                stream.sharing_mode(),
                stream.device_id(),
                stream.x_run_count()
            ),
        );
        Some(Self {
            stream,
            ring,
            pcm,
            failed,
            capture,
            capture_limit,
            xruns: Vec::new(),
            trace: Vec::new(),
            observation: WriterObservation::default(),
            game: false,
            since_report: 0,
            local_fast,
            timestamp_fallbacks: 0,
        })
    }
    pub fn name(&self) -> &'static str {
        "AAudio"
    }
    /// This is the game's stream: `game_line` reports it.
    pub(crate) fn report_as_game(&mut self) {
        self.game = true;
        GAME_CALLBACK_UNDERRUNS.store(0, Ordering::Relaxed);
        GAME_BURST.store(self.stream.frames_per_burst().max(1), Ordering::Relaxed);
        GAME_BUFFER.store(self.stream.buffer_size_in_frames(), Ordering::Relaxed);
        GAME_XRUNS.store(self.stream.x_run_count().max(0), Ordering::Relaxed);
    }
    pub(crate) fn ring(&self) -> Option<Ring> {
        Some(self.ring)
    }
    pub(crate) fn write(&mut self, samples: &[i16]) -> bool {
        let started = self.observation.begin();
        let success = self.write_inner(samples);
        self.observation.finish(started, success);
        success
    }
    fn write_inner(&mut self, samples: &[i16]) -> bool {
        if let Some(capture) = &mut self.capture {
            // Where this write starts in the capture, when, and what AAudio says was presented when (its timestamp):
            // what the launch's sync measurement reads (scripts/android/insert-sync.py).
            if capture.len() < self.capture_limit
                && self.trace.len() < self.capture_limit / (OUT_RATE as usize / 25) + 16
                && capture.len() % (OUT_RATE as usize / 25) < samples.len() {
                let presented = self
                    .stream
                    .timestamp(ndk::audio::Clockid::Monotonic)
                    .ok()
                    .map_or((-1, 0.0), |t| (t.frame_position, t.time_nanoseconds as f64 / 1e6));
                self.trace
                    .push((capture.len() / 2, crate::platform::android::monotonic_ms(), presented.0, presented.1));
            }
            let n = samples.len().min(self.capture_limit.saturating_sub(capture.len()));
            capture.extend_from_slice(&samples[..n]);
            let xruns = self.stream.x_run_count();
            if xruns >= 0 {
                if self.xruns.last().is_none_or(|(_, n)| *n != xruns) {
                    self.xruns.push((capture.len() / 2, xruns));
                }
            }
        }
        let mut rest = samples;
        let mut waiting = std::time::Instant::now();
        while !rest.is_empty() {
            if self.failed.load(Ordering::Acquire) { return false; }
            let wrote = self.pcm.push(rest);
            if wrote == 0 {
                // Only the ordinary mixer worker waits. The high-priority callback never blocks.
                // Bound the wait so a disconnected or stalled output can be reopened.
                if waiting.elapsed() > std::time::Duration::from_millis(100) { return false; }
                std::thread::sleep(std::time::Duration::from_micros(500));
                continue;
            }
            waiting = std::time::Instant::now();
            if let Some(diagnostics) = &self.observation.diagnostics {
                diagnostics.accepted_frames(wrote as u64);
            }
            rest = &rest[wrote * 2..];
        }
        if self.game {
            self.since_report += samples.len() / 2;
            if self.since_report >= OUT_RATE as usize {
                self.since_report = 0;
                GAME_CALLBACK_UNDERRUNS.store(self.pcm.underruns.load(Ordering::Relaxed), Ordering::Relaxed);
                GAME_HANDOFF.store(self.pcm.queued() as u32, Ordering::Relaxed);
                GAME_XRUNS.store(self.stream.x_run_count().max(0), Ordering::Relaxed);
                GAME_BUFFER.store(self.stream.buffer_size_in_frames(), Ordering::Relaxed);
                let ahead = self.ahead_us();
                GAME_AHEAD_US.store(ahead, Ordering::Relaxed);
            }
        }
        true
    }
    /// Scheduling lead: presentation timestamp, unless a confirmed local Fast route's small
    /// written/read queue contradicts an implausible estimate. Raw driver data stays in diagnostics.
    pub(crate) fn ahead_us(&mut self) -> u64 {
        let timestamp = self.stream.timestamp(ndk::audio::Clockid::Monotonic).ok().map(|t| {
            let since_ms = crate::platform::android::monotonic_ms() - t.time_nanoseconds as f64 / 1e6;
            let heard = t.frame_position as f64 + since_ms * OUT_RATE as f64 / 1000.0;
            ((self.stream.frames_written() as f64 - heard).max(0.0) * 1e6 / OUT_RATE as f64) as u64
        });
        let us = |frames: u64| frames * 1_000_000 / OUT_RATE as u64;
        let counter = us((self.stream.frames_written() - self.stream.frames_read()).max(0) as u64);
        let handoff = us(self.pcm.queued() as u64);
        let buffer = us(self.stream.buffer_size_in_frames().max(0) as u64);
        let ahead = crate::android_audio_timing::ahead_us(timestamp, counter, handoff, buffer,
            us(self.stream.frames_per_burst().max(1) as u64), self.local_fast);
        let rejected = timestamp.is_some_and(|raw| ahead != raw.saturating_add(handoff));
        if rejected {
            if self.timestamp_fallbacks == 0 {
                log_line(5, &format!("AAudio timestamp rejected for scheduling: reported_us={} counter_us={counter} handoff_us={handoff} buffer_us={buffer} device={}", timestamp.unwrap_or_default(), self.stream.device_id()));
            }
            self.timestamp_fallbacks = self.timestamp_fallbacks.saturating_add(1);
        }
        if self.game {
            GAME_REPORTED_AHEAD_US.store(timestamp.map_or(ahead, |raw| raw.saturating_add(handoff)), Ordering::Relaxed);
            GAME_TIMESTAMP_FALLBACK.store(rejected, Ordering::Relaxed);
        }
        ahead
    }
    pub(crate) fn latency_us(&mut self) -> u64 {
        ((self.stream.frames_written() - self.stream.frames_read()).max(0) as u64 + self.pcm.queued() as u64) * 1_000_000 / OUT_RATE as u64
    }
}

impl Drop for Sink {
    fn drop(&mut self) {
        let _ = self.stream.request_stop();
        if let Some(capture) = &self.capture {
            // Beside the app's other files on shared storage when Android has such a place: a test on a real handheld
            // can fetch it over adb without root. Only a diagnostic launch writes here.
            let shared = crate::platform::android::string("diagnosticsPath");
            let dir = if shared.is_empty() { crate::platform::data_dir() } else { std::path::PathBuf::from(shared) }.join("audio-diagnostics");
            let id = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos();
            // SAFETY: samples are contiguous native-endian i16; Android ARM64 is little endian.
            let bytes = unsafe { std::slice::from_raw_parts(capture.as_ptr().cast::<u8>(), capture.len() * 2) };
            let _ = crate::fsutil::write_atomic(&dir.join(format!("{id}.s16le")), bytes);
            let meta = serde_json::json!({"rate":OUT_RATE,"channels":2,"samples":capture.len(),"capture_limit_seconds":self.capture_limit / (OUT_RATE as usize * 2),"frames_per_burst":self.stream.frames_per_burst(),"buffer_frames":self.stream.buffer_size_in_frames(),"xruns":self.xruns,"trace":self.trace,"local_fast":self.local_fast,"timestamp_fallbacks":self.timestamp_fallbacks});
            crate::fsutil::write_json_soon(&dir.join(format!("{id}.json")), &meta);
        }
    }
}

#[link(name = "log")]
extern "C" {
    fn __android_log_write(priority: i32, tag: *const std::ffi::c_char, text: *const std::ffi::c_char) -> i32;
}
/// One line in logcat under the app's tag (4 info, 6 error).
pub(crate) fn log_line(priority: i32, message: &str) {
    if let Ok(text) = std::ffi::CString::new(message) {
        // SAFETY: both strings are NUL-terminated and live through the call.
        unsafe {
            __android_log_write(priority, c"PocketMvp".as_ptr(), text.as_ptr());
        }
    }
}
