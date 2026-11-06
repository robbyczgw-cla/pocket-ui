//! Cumulative output-backend observations. These are error *observations*, not a count of audible defects:
//! a delay query and a write may both see the same ALSA xrun. Queue starvation and graph errors belong
//! to their own counters. No logging, allocation or locks are needed on the writer's measurement path.
use std::sync::{
    Arc,
    atomic::{AtomicI64, AtomicU32, AtomicU64, Ordering::Relaxed},
};
use std::time::Instant;

#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct RangeSnapshot {
    pub samples: u64,
    pub failures: u64,
    pub min: Option<i64>,
    pub max: Option<i64>,
    pub latest: Option<i64>,
}

#[derive(Default)]
struct Range {
    samples: AtomicU64,
    failures: AtomicU64,
    // Encoding signed extrema with an explicit sample count preserves real negative PCM delays.
    min: AtomicI64,
    max: AtomicI64,
    latest: AtomicI64,
}
impl Range {
    fn record(&self, value: i64) {
        // A range has one producer (the output writer); snapshots may be taken concurrently.
        if self.samples.load(Relaxed) == 0 {
            self.min.store(value, Relaxed);
            self.max.store(value, Relaxed);
        } else {
            self.min.fetch_min(value, Relaxed);
            self.max.fetch_max(value, Relaxed);
        }
        self.latest.store(value, Relaxed);
        self.samples.fetch_add(1, Relaxed);
    }
    fn snapshot(&self) -> RangeSnapshot {
        let samples = self.samples.load(Relaxed);
        RangeSnapshot {
            samples,
            failures: self.failures.load(Relaxed),
            min: (samples != 0).then(|| self.min.load(Relaxed)),
            max: (samples != 0).then(|| self.max.load(Relaxed)),
            latest: (samples != 0).then(|| self.latest.load(Relaxed)),
        }
    }
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct AlsaSnapshot {
    /// Every snd_pcm_recover invocation, including silent recoveries.
    pub recover_calls: u64,
    pub recover_failures: u64,
    /// -EPIPE results of writes. Query errors are separate to avoid adding duplicate observations.
    pub xrun_errors: u64,
    pub query_xrun_errors: u64,
    pub write_errors: u64,
    pub short_writes: u64,
    pub zero_writes: u64,
    /// Last opened ALSA stream's negotiated parameters, not the requested values.
    pub pcm_rate: Option<u32>,
    pub buffer_frames: Option<u64>,
    pub period_frames: Option<u64>,
    pub config_query_failures: u64,
    pub avail_supported: bool,
    /// Signed frames sampled at most once per 100 ms, not inferred from the requested buffer.
    pub delay_frames: RangeSnapshot,
    pub avail_frames: RangeSnapshot,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct Snapshot {
    /// Most recently attached backend; "unopened" until a sink is attached.
    pub backend: &'static str,
    pub opens: u64,
    pub alsa_opens: u64,
    pub pulse_opens: u64,
    pub android_opens: u64,
    pub elapsed_us: u64,
    pub last_write_ago_us: Option<u64>,
    /// Calls to the sink writer, including failed calls and prefills.
    pub write_chunks: u64,
    /// Frames accepted by the backend, including a partial write before a failure.
    pub write_frames: u64,
    pub write_failures: u64,
    /// Time between successive write starts on the same stream. Reopens do not form an artificial gap.
    pub write_gap_us: RangeSnapshot,
    pub write_duration_us: RangeSnapshot,
    /// None means ALSA counters are unsupported (Pulse/Android), never a measured zero.
    /// If the latest backend changed, historical ALSA observations remain available with alsa_opens.
    pub alsa: Option<AlsaSnapshot>,
}

pub struct Diagnostics {
    started: Instant,
    backend: AtomicU32,
    opens: AtomicU64,
    alsa_opens: AtomicU64,
    pulse_opens: AtomicU64,
    android_opens: AtomicU64,
    write_chunks: AtomicU64,
    write_frames: AtomicU64,
    write_failures: AtomicU64,
    last_write_us: AtomicU64,
    write_gap_us: Range,
    write_duration_us: Range,
    recover_calls: AtomicU64,
    recover_failures: AtomicU64,
    xrun_errors: AtomicU64,
    query_xrun_errors: AtomicU64,
    write_errors: AtomicU64,
    short_writes: AtomicU64,
    zero_writes: AtomicU64,
    pcm_rate: AtomicU32,
    buffer_frames: AtomicU64,
    period_frames: AtomicU64,
    config_query_failures: AtomicU64,
    avail_supported: AtomicU32,
    delay_frames: Range,
    avail_frames: Range,
}
impl Default for Diagnostics {
    fn default() -> Self {
        Self {
            started: Instant::now(),
            backend: AtomicU32::new(0),
            opens: AtomicU64::new(0),
            alsa_opens: AtomicU64::new(0),
            pulse_opens: AtomicU64::new(0),
            android_opens: AtomicU64::new(0),
            write_chunks: AtomicU64::new(0),
            write_frames: AtomicU64::new(0),
            write_failures: AtomicU64::new(0),
            last_write_us: AtomicU64::new(0),
            write_gap_us: Range::default(),
            write_duration_us: Range::default(),
            recover_calls: AtomicU64::new(0),
            recover_failures: AtomicU64::new(0),
            xrun_errors: AtomicU64::new(0),
            query_xrun_errors: AtomicU64::new(0),
            write_errors: AtomicU64::new(0),
            short_writes: AtomicU64::new(0),
            zero_writes: AtomicU64::new(0),
            pcm_rate: AtomicU32::new(0),
            buffer_frames: AtomicU64::new(0),
            period_frames: AtomicU64::new(0),
            config_query_failures: AtomicU64::new(0),
            avail_supported: AtomicU32::new(0),
            delay_frames: Range::default(),
            avail_frames: Range::default(),
        }
    }
}
impl Diagnostics {
    /// A bounded, approximate snapshot: counters can advance while it is being read.
    pub fn snapshot(&self) -> Snapshot {
        let alsa_opens = self.alsa_opens.load(Relaxed);
        let rate = self.pcm_rate.load(Relaxed);
        let buffer = self.buffer_frames.load(Relaxed);
        let period = self.period_frames.load(Relaxed);
        let elapsed_us = micros(self.started.elapsed());
        let chunks = self.write_chunks.load(Relaxed);
        Snapshot {
            backend: match self.backend.load(Relaxed) {
                1 => "alsa",
                2 => "pulse",
                3 => "android",
                _ => "unopened",
            },
            opens: self.opens.load(Relaxed),
            alsa_opens,
            pulse_opens: self.pulse_opens.load(Relaxed),
            android_opens: self.android_opens.load(Relaxed),
            elapsed_us,
            last_write_ago_us: (chunks != 0).then(|| elapsed_us.saturating_sub(self.last_write_us.load(Relaxed))),
            write_chunks: chunks,
            write_frames: self.write_frames.load(Relaxed),
            write_failures: self.write_failures.load(Relaxed),
            write_gap_us: self.write_gap_us.snapshot(),
            write_duration_us: self.write_duration_us.snapshot(),
            alsa: (alsa_opens != 0).then(|| AlsaSnapshot {
                recover_calls: self.recover_calls.load(Relaxed),
                recover_failures: self.recover_failures.load(Relaxed),
                xrun_errors: self.xrun_errors.load(Relaxed),
                query_xrun_errors: self.query_xrun_errors.load(Relaxed),
                write_errors: self.write_errors.load(Relaxed),
                short_writes: self.short_writes.load(Relaxed),
                zero_writes: self.zero_writes.load(Relaxed),
                pcm_rate: (rate != 0).then_some(rate),
                buffer_frames: (buffer != 0).then_some(buffer),
                period_frames: (period != 0).then_some(period),
                config_query_failures: self.config_query_failures.load(Relaxed),
                delay_frames: self.delay_frames.snapshot(),
                avail_frames: self.avail_frames.snapshot(),
                avail_supported: self.avail_supported.load(Relaxed) != 0,
            }),
        }
    }
    pub(crate) fn opened(&self, backend: &'static str) {
        let (id, count) = match backend {
            "alsa" => (1, &self.alsa_opens),
            "pulse" => (2, &self.pulse_opens),
            "android" => (3, &self.android_opens),
            _ => return,
        };
        count.fetch_add(1, Relaxed);
        self.backend.store(id, Relaxed);
        self.opens.fetch_add(1, Relaxed);
    }
    pub(crate) fn alsa_config(&self, rate: Option<u32>, buffer: Option<u64>, period: Option<u64>, failures: u64) {
        self.pcm_rate.store(rate.unwrap_or(0), Relaxed);
        self.buffer_frames.store(buffer.unwrap_or(0), Relaxed);
        self.period_frames.store(period.unwrap_or(0), Relaxed);
        self.config_query_failures.fetch_add(failures, Relaxed);
    }
    pub(crate) fn alsa_write_error(&self, error: i64) {
        self.write_errors.fetch_add(1, Relaxed);
        if error == -32 {
            self.xrun_errors.fetch_add(1, Relaxed);
        }
    }
    pub(crate) fn alsa_recover(&self, failed: bool) {
        self.recover_calls.fetch_add(1, Relaxed);
        if failed {
            self.recover_failures.fetch_add(1, Relaxed);
        }
    }
    pub(crate) fn alsa_short_write(&self) {
        self.short_writes.fetch_add(1, Relaxed);
    }
    pub(crate) fn alsa_avail_supported(&self, supported: bool) {
        self.avail_supported.store(u32::from(supported), Relaxed);
    }
    pub(crate) fn alsa_zero_write(&self) {
        self.zero_writes.fetch_add(1, Relaxed);
    }
    pub(crate) fn accepted_frames(&self, frames: u64) {
        self.write_frames.fetch_add(frames, Relaxed);
    }
    pub(crate) fn alsa_delay(&self, result: Result<i64, i64>) {
        self.alsa_sample(&self.delay_frames, result);
    }
    pub(crate) fn alsa_avail(&self, result: Result<i64, i64>) {
        self.alsa_sample(&self.avail_frames, result);
    }
    fn alsa_sample(&self, range: &Range, result: Result<i64, i64>) {
        match result {
            Ok(frames) => range.record(frames),
            Err(error) => {
                range.failures.fetch_add(1, Relaxed);
                if error == -32 {
                    self.query_xrun_errors.fetch_add(1, Relaxed);
                }
            }
        }
    }
}

fn micros(duration: std::time::Duration) -> u64 {
    duration.as_micros().min(i64::MAX as u128) as u64
}

/// Per-stream timing state, while the attached Diagnostics survives stream reopens.
#[derive(Default)]
pub(crate) struct WriterObservation {
    pub(crate) diagnostics: Option<Arc<Diagnostics>>,
    last_write: Option<Instant>,
}
impl WriterObservation {
    pub(crate) fn attach(&mut self, diagnostics: Arc<Diagnostics>, backend: &'static str) {
        diagnostics.opened(backend);
        self.diagnostics = Some(diagnostics);
        self.last_write = None;
    }
    pub(crate) fn begin(&mut self) -> Option<Instant> {
        let diagnostics = self.diagnostics.as_ref()?;
        let now = Instant::now();
        if let Some(previous) = self.last_write {
            diagnostics.write_gap_us.record(micros(now.duration_since(previous)) as i64);
        }
        diagnostics.write_chunks.fetch_add(1, Relaxed);
        diagnostics.last_write_us.store(micros(diagnostics.started.elapsed()), Relaxed);
        self.last_write = Some(now);
        Some(now)
    }
    pub(crate) fn finish(&self, started: Option<Instant>, success: bool) {
        if let (Some(diagnostics), Some(started)) = (&self.diagnostics, started) {
            diagnostics.write_duration_us.record(micros(started.elapsed()) as i64);
            if !success {
                diagnostics.write_failures.fetch_add(1, Relaxed);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unsupported_counters_are_not_reported_as_zero_xruns() {
        let d = Diagnostics::default();
        assert!(d.snapshot().alsa.is_none());
        d.opened("pulse");
        assert!(d.snapshot().alsa.is_none());
        d.opened("android");
        assert!(d.snapshot().alsa.is_none());
    }
    #[test]
    fn backend_reopen_keeps_recoveries_but_not_stale_negotiated_values() {
        let d = Diagnostics::default();
        d.opened("alsa");
        d.alsa_config(Some(48000), Some(1536), Some(1024), 0);
        d.alsa_write_error(-32);
        d.alsa_recover(false);
        d.opened("alsa");
        d.alsa_config(None, None, None, 2);
        let s = d.snapshot();
        let alsa = s.alsa.unwrap();
        assert_eq!(s.opens, 2);
        assert_eq!(alsa.recover_calls, 1);
        assert_eq!(alsa.xrun_errors, 1);
        assert_eq!(alsa.pcm_rate, None);
        assert_eq!(alsa.buffer_frames, None);
        assert_eq!(alsa.config_query_failures, 2);
    }
    #[test]
    fn failed_queries_do_not_invent_zero_delay_or_lose_signed_measurements() {
        let d = Diagnostics::default();
        d.opened("alsa");
        d.alsa_delay(Err(-32));
        let s = d.snapshot().alsa.unwrap();
        assert_eq!(s.delay_frames.samples, 0);
        assert_eq!(s.delay_frames.min, None);
        assert_eq!(s.delay_frames.failures, 1);
        d.alsa_delay(Ok(-7));
        d.alsa_delay(Ok(120));
        let s = d.snapshot().alsa.unwrap();
        assert_eq!(s.delay_frames.min, Some(-7));
        assert_eq!(s.delay_frames.max, Some(120));
        assert_eq!(s.xrun_errors, 0);
        assert_eq!(s.query_xrun_errors, 1);
    }
}
