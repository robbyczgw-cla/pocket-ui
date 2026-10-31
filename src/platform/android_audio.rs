//! AAudio output for the existing UI mixer. All blocking writes happen on its worker, never the UI thread.
use crate::audio::OUT_RATE;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
static MIN_QUEUE_MS: AtomicU32 = AtomicU32::new(0);
static LOW_LATENCY: AtomicBool = AtomicBool::new(false);
pub(crate) fn minimum_queue_ms() -> u32 {
    MIN_QUEUE_MS.load(Ordering::Relaxed)
}
pub(crate) fn low_latency() -> bool {
    LOW_LATENCY.load(Ordering::Relaxed)
}
use ndk::audio::{AudioDirection, AudioFormat, AudioPerformanceMode, AudioSharingMode, AudioStream, AudioStreamBuilder};
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
        if self == Ring::Short {
            10_000
        } else {
            32_000
        }
    }
}
pub fn restore_quantum() {} // No global system audio policy was changed.
pub(crate) struct Sink {
    stream: AudioStream,
    ring: Ring,
    capture: Option<Vec<i16>>,
    xruns: Vec<(usize, i32)>,
}
// SAFETY: the owning writer thread is the only caller; the stream and its close move together.
unsafe impl Send for Sink {}
impl Sink {
    pub(crate) fn uses_graph(&self) -> bool {
        false
    }
    pub(crate) fn open(ring: Ring) -> Option<Self> {
        let stream = AudioStreamBuilder::new()
            .ok()?
            .direction(AudioDirection::Output)
            .format(AudioFormat::PCM_I16)
            .channel_count(2)
            .sample_rate(OUT_RATE as i32)
            .sharing_mode(AudioSharingMode::Shared)
            .performance_mode(AudioPerformanceMode::LowLatency)
            .open_stream()
            .ok()?;
        let target = (i64::from(ring.us()) * i64::from(OUT_RATE) / 1_000_000) as i32;
        // AudioTrack fallback devices may reject resizing. Keep their valid stream and size the shared queue
        // for their burst, rather than refusing all sound or treating them as a low-latency endpoint.
        let _ = stream.set_buffer_size_in_frames(target.max(stream.frames_per_burst() * 2));
        if stream.sample_rate() != OUT_RATE as i32 {
            return None;
        }
        let burst_ms = (stream.frames_per_burst().max(1) as u32 * 1000).div_ceil(OUT_RATE);
        MIN_QUEUE_MS.store(burst_ms + 20, Ordering::Relaxed);
        LOW_LATENCY.store(
            stream.performance_mode() == AudioPerformanceMode::LowLatency && burst_ms <= 10,
            Ordering::Relaxed,
        );
        stream.request_start().ok()?;
        let capture = (crate::platform::android::string("diagnosticAudio") == "true").then(|| Vec::with_capacity(OUT_RATE as usize * 60));
        eprintln!(
            "AAudio opened: rate={} burst={} buffer={}",
            stream.sample_rate(),
            stream.frames_per_burst(),
            stream.buffer_size_in_frames()
        );
        Some(Self {
            stream,
            ring,
            capture,
            xruns: Vec::new(),
        })
    }
    pub fn name(&self) -> &'static str {
        "AAudio"
    }
    pub(crate) fn ring(&self) -> Option<Ring> {
        Some(self.ring)
    }
    pub(crate) fn write(&mut self, samples: &[i16]) -> bool {
        if let Some(capture) = &mut self.capture {
            let n = samples.len().min(OUT_RATE as usize * 60 - capture.len());
            capture.extend_from_slice(&samples[..n]);
            let xruns = self.stream.x_run_count();
            if xruns >= 0 {
                if self.xruns.last().is_none_or(|(_, n)| *n != xruns) {
                    self.xruns.push((capture.len() / 2, xruns));
                }
            }
        }
        let mut rest = samples;
        while !rest.is_empty() {
            // SAFETY: stereo i16 stream and the slice contains at least the supplied frame count.
            // ndk 0.9 routes AAudio's positive frame count through its status-code decoder, which wraps
            // successful writes as AudioError::__Unknown(n). Recover the original signed API result, accepting
            // only a positive count within this request. Real (negative) AAudio errors still reopen the stream.
            let requested = (rest.len() / 2) as i32;
            let wrote = match unsafe { self.stream.write(rest.as_ptr().cast(), requested, 100_000_000) } {
                Ok(frames) => frames as i32,
                Err(error) => i32::from(error),
            };
            if wrote <= 0 || wrote > requested {
                log_error(&format!("AAudio write failed: {wrote}"));
                return false;
            }
            rest = &rest[wrote as usize * 2..];
        }
        true
    }
    pub(crate) fn latency_us(&mut self) -> u64 {
        (self.stream.frames_written() - self.stream.frames_read()).max(0) as u64 * 1_000_000 / OUT_RATE as u64
    }
}

impl Drop for Sink {
    fn drop(&mut self) {
        let _ = self.stream.request_stop();
        if let Some(capture) = &self.capture {
            let dir = crate::platform::data_dir().join("audio-diagnostics");
            let id = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos();
            // SAFETY: samples are contiguous native-endian i16; Android ARM64 is little endian.
            let bytes = unsafe { std::slice::from_raw_parts(capture.as_ptr().cast::<u8>(), capture.len() * 2) };
            let _ = crate::fsutil::write_atomic(&dir.join(format!("{id}.s16le")), bytes);
            let meta = serde_json::json!({"rate":OUT_RATE,"channels":2,"samples":capture.len(),"frames_per_burst":self.stream.frames_per_burst(),"buffer_frames":self.stream.buffer_size_in_frames(),"xruns":self.xruns});
            crate::fsutil::write_json_soon(&dir.join(format!("{id}.json")), &meta);
        }
    }
}

#[link(name = "log")]
extern "C" {
    fn __android_log_write(priority: i32, tag: *const std::ffi::c_char, text: *const std::ffi::c_char) -> i32;
}
fn log_error(message: &str) {
    if let Ok(text) = std::ffi::CString::new(message) {
        unsafe {
            __android_log_write(6, c"PocketMvp".as_ptr(), text.as_ptr());
        }
    }
}
