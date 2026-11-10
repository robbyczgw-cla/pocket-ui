//! The browser demo's sound output (docs/WEB-DEMO.md), in place of the Linux sink for wasm32-unknown-emscripten
//! (`platform::audio_backend`). pocket-ui's UI mixer (`ui_sound`) writes 48 kHz stereo i16 into a ring in shared
//! memory; the page's AudioWorklet (web/demo/page/sound.js) reads it on the browser's audio thread. So the sounds, the
//! Retro style's blips, Settings → Sounds (on, off, volume) and the mixing are the device's own; only the last step is
//! the browser's. Until the page has started its AudioContext (a first press or tap: the browsers' autoplay rule) the
//! samples are dropped at the pace they would play: nothing piles up to sound late.

use crate::audio::OUT_RATE;
use crate::audio_diagnostics::{Diagnostics, WriterObservation};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Frames the ring holds (a power of two).
const FRAMES: usize = 8192;
/// How far the writer may get ahead of what the page plays: a press is heard within this much (plus the browser's own
/// output latency); the worker that writes is scheduled less tightly than ALSA's, so not the device's 10-32 ms.
const AHEAD: u32 = (OUT_RATE / 1000) * 45;
/// A page that stopped reading (tab hidden, AudioContext suspended) for this long counts as gone: samples are dropped.
const STALLED: Duration = Duration::from_millis(250);

/// The ring's samples. Written only by the one Sink's writer thread, read by the page; `WRITE`/`READ` say which part.
struct Samples(std::cell::UnsafeCell<[i16; FRAMES * 2]>);
// SAFETY: one writer (the mixer's thread) writes a region the reader doesn't read until `WRITE` is published.
unsafe impl Sync for Samples {}
static SAMPLES: Samples = Samples(std::cell::UnsafeCell::new([0; FRAMES * 2]));
/// Frames written and frames read, ever (wrapping); the page's worklet adds to `READ`.
static WRITE: AtomicU32 = AtomicU32::new(0);
static READ: AtomicU32 = AtomicU32::new(0);
/// 1 while the page plays the ring (its AudioContext runs).
static RUNNING: AtomicU32 = AtomicU32::new(0);

/// Where the ring is, for the page: [samples pointer, frames, WRITE pointer, READ pointer], four u32 at `out`.
#[no_mangle]
pub extern "C" fn pocket_web_audio(out: *mut u32) {
    let ring = [SAMPLES.0.get() as u32, FRAMES as u32, &WRITE as *const AtomicU32 as u32, &READ as *const AtomicU32 as u32];
    // SAFETY: the page passes four u32 it allocated.
    unsafe { std::ptr::copy_nonoverlapping(ring.as_ptr(), out, 4) };
}

/// The page started (1) or stopped (0) playing the ring. Stopping forgets what waits in it.
#[no_mangle]
pub extern "C" fn pocket_web_audio_running(on: u32) {
    if on == 0 {
        READ.store(WRITE.load(Ordering::Acquire), Ordering::Release);
    }
    RUNNING.store(on, Ordering::Release);
}

pub(crate) fn minimum_queue_ms() -> u32 {
    20
}
pub(crate) fn game_quantum() -> u32 {
    0
}
pub(crate) fn force_quantum(_: u32) -> bool {
    false
}
/// No system audio policy was changed.
pub fn restore_quantum() {}

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

pub(crate) struct Sink {
    ring: Ring,
    observation: WriterObservation,
}

impl Sink {
    pub(crate) fn observe(&mut self, diagnostics: Arc<Diagnostics>) {
        self.observation.attach(diagnostics, "webaudio");
    }
    pub(crate) fn uses_graph(&self) -> bool {
        false
    }
    pub(crate) fn open(ring: Ring) -> Option<Self> {
        Some(Sink { ring, observation: WriterObservation::default() })
    }
    pub fn name(&self) -> &'static str {
        "WebAudio"
    }
    pub(crate) fn ring(&self) -> Option<Ring> {
        Some(self.ring)
    }
    pub(crate) fn write(&mut self, samples: &[i16]) -> bool {
        let started = self.observation.begin();
        write_ring(samples);
        self.observation.finish(started, true);
        true
    }
    pub(crate) fn latency_us(&mut self) -> u64 {
        let ahead = WRITE.load(Ordering::Acquire).wrapping_sub(READ.load(Ordering::Acquire));
        u64::from(ahead) * 1_000_000 / u64::from(OUT_RATE)
    }
}

/// Puts interleaved stereo frames into the ring, waiting while it is `AHEAD` of the page; drops them at their own pace
/// while the page doesn't play (or stopped reading).
fn write_ring(samples: &[i16]) {
    let mut rest = samples;
    let mut waiting_since: Option<Instant> = None;
    while rest.len() >= 2 {
        let frames = rest.len() / 2;
        if RUNNING.load(Ordering::Acquire) == 0 || waiting_since.is_some_and(|t| t.elapsed() >= STALLED) {
            std::thread::sleep(Duration::from_micros(frames as u64 * 1_000_000 / u64::from(OUT_RATE)));
            return;
        }
        let (w, r) = (WRITE.load(Ordering::Acquire), READ.load(Ordering::Acquire));
        let ahead = w.wrapping_sub(r);
        if ahead >= AHEAD {
            waiting_since.get_or_insert_with(Instant::now);
            std::thread::sleep(Duration::from_millis(2));
            continue;
        }
        waiting_since = None;
        let n = frames.min((AHEAD - ahead) as usize);
        let base = SAMPLES.0.get().cast::<i16>();
        for i in 0..n {
            let at = (w as usize).wrapping_add(i) % FRAMES * 2;
            // SAFETY: `at` + 1 < FRAMES * 2, and the reader doesn't touch frames past `WRITE` until it is stored below.
            unsafe {
                *base.add(at) = rest[2 * i];
                *base.add(at + 1) = rest[2 * i + 1];
            }
        }
        WRITE.store(w.wrapping_add(n as u32), Ordering::Release);
        rest = &rest[2 * n..];
    }
}
