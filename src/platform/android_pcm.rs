//! Bounded stereo handoff to Android's real-time callback. Atomics only in the reader.
use std::sync::atomic::{AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;

/// Two full endpoint bursts, rounded to callbacks; at least three callbacks bridge worker jitter.
pub(crate) fn handoff_frames(burst: usize, callback: usize) -> usize {
    (burst.div_ceil(callback) * callback * 2).max(callback * 3)
}

pub(crate) struct Pcm {
    frames: Box<[AtomicU32]>,
    capacity: usize,
    read: AtomicUsize,
    written: AtomicUsize,
    pub(crate) underruns: AtomicU64,
}
impl Pcm {
    pub(crate) fn new(capacity: usize) -> Arc<Self> {
        assert!(capacity > 0);
        Arc::new(Self {
            frames: (0..capacity.next_power_of_two()).map(|_| AtomicU32::new(0)).collect(),
            capacity,
            read: AtomicUsize::new(0), written: AtomicUsize::new(0), underruns: AtomicU64::new(0),
        })
    }
    pub(crate) fn queued(&self) -> usize {
        let read = self.read.load(Ordering::Acquire);
        self.written.load(Ordering::Acquire).wrapping_sub(read).min(self.capacity)
    }
    /// Called by exactly one producer, the mixer's worker. Never overwrites unheard PCM.
    pub(crate) fn push(&self, samples: &[i16]) -> usize {
        let written = self.written.load(Ordering::Relaxed);
        let read = self.read.load(Ordering::Acquire);
        let count = (samples.len() / 2).min(self.capacity - written.wrapping_sub(read));
        for (i, pair) in samples.chunks_exact(2).take(count).enumerate() {
            let packed = pair[0] as u16 as u32 | ((pair[1] as u16 as u32) << 16);
            self.frames[written.wrapping_add(i) % self.frames.len()].store(packed, Ordering::Relaxed);
        }
        self.written.store(written.wrapping_add(count), Ordering::Release);
        count
    }
}
pub(crate) struct Reader {
    pcm: Arc<Pcm>,
    priming: bool,
}
impl Reader {
    pub(crate) fn new(pcm: Arc<Pcm>) -> Self { Self { pcm, priming: true } }
    /// Called by exactly one consumer. The platform supplies whole stereo frames.
    pub(crate) fn render(&mut self, out: &mut [i16]) {
        out.fill(0);
        let requested = out.len() / 2;
        let read = self.pcm.read.load(Ordering::Relaxed);
        let available = self.pcm.written.load(Ordering::Acquire).wrapping_sub(read);
        if self.priming && available < requested.min(self.pcm.capacity) { return; }
        let fading_in = self.priming;
        self.priming = false;
        let count = requested.min(available);
        for i in 0..count {
            let packed = self.pcm.frames[read.wrapping_add(i) % self.pcm.frames.len()].load(Ordering::Relaxed);
            out[i * 2] = packed as u16 as i16;
            out[i * 2 + 1] = (packed >> 16) as u16 as i16;
        }
        self.pcm.read.store(read.wrapping_add(count), Ordering::Release);
        if fading_in {
            let ramp = count.min(64);
            for i in 0..ramp {
                for channel in 0..2 { out[i * 2 + channel] = (out[i * 2 + channel] as i32 * i as i32 / ramp as i32) as i16; }
            }
        }
        if count < requested {
            // Keep every available sample, fade its end, then wait for a whole callback again.
            let ramp = count.min(64);
            for i in 0..ramp {
                for channel in 0..2 {
                    let index = (count - ramp + i) * 2 + channel;
                    out[index] = (out[index] as i32 * (ramp - i - 1) as i32 / ramp as i32) as i16;
                }
            }
            self.pcm.underruns.fetch_add(1, Ordering::Relaxed);
            self.priming = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn block(frames: usize, value: i16) -> Vec<i16> { vec![value; frames * 2] }
    #[test]
    fn a_slow_endpoint_can_pull_two_bursts_without_waiting_for_the_worker() {
        for burst in [144, 866, 960, 1024] {
            let capacity = handoff_frames(burst, 256);
            let pcm = Pcm::new(capacity);
            let mut reader = Reader::new(pcm.clone());
            assert_eq!(pcm.push(&block(capacity, 1000)), capacity);
            for _ in 0..(burst.div_ceil(256) * 2) {
                reader.render(&mut block(256, 0));
            }
            assert_eq!(pcm.underruns.load(Ordering::Relaxed), 0, "burst {burst}");
        }
    }
    #[test]
    fn callback_keeps_stereo_order_and_never_overwrites_unheard_sound() {
        let pcm = Pcm::new(3);
        let mut reader = Reader::new(pcm.clone());
        let samples = [1, -1, 2, -2, 3, -3, 4, -4, 5, -5];
        assert_eq!(pcm.push(&samples), 3);
        assert_eq!(pcm.push(&samples), 0);
        let mut out = [0; 4];
        reader.render(&mut out);
        // The first samples fade in, so verify order after its short startup ramp in the other test.
        assert_eq!(pcm.queued(), 1);
        assert_eq!(pcm.push(&samples[6..]), 2);
        let mut out = [0; 6];
        reader.render(&mut out);
        assert_eq!(out, [3, -3, 4, -4, 5, -5]);
    }
    #[test]
    fn callback_and_worker_keep_every_frame_across_counter_wrap() {
        let pcm = Pcm::new(192);
        let start = usize::MAX - 127;
        pcm.read.store(start, Ordering::Relaxed);
        pcm.written.store(start, Ordering::Relaxed);
        let producer = pcm.clone();
        let worker = std::thread::spawn(move || {
            for i in 0..20_000u16 {
                let pair = [i as i16, !(i as i16)];
                while producer.push(&pair) == 0 { std::thread::yield_now(); }
            }
        });
        let mut reader = Reader::new(pcm.clone());
        reader.priming = false; // Verify PCM identity separately from the tested fade policy.
        for i in 0..20_000u16 {
            while pcm.queued() == 0 { std::thread::yield_now(); }
            let mut out = [0; 2];
            reader.render(&mut out);
            assert_eq!(out, [i as i16, !(i as i16)]);
        }
        worker.join().unwrap();
        assert_eq!(pcm.queued(), 0);
        assert_eq!(pcm.underruns.load(Ordering::Relaxed), 0);
    }
    #[test]
    fn callback_silences_only_the_missing_tail_and_refills_after_one_underrun() {
        let pcm = Pcm::new(256);
        let mut reader = Reader::new(pcm.clone());
        let mut out = block(128, -1);
        reader.render(&mut out);
        assert!(out.iter().all(|s| *s == 0));
        assert_eq!(pcm.underruns.load(Ordering::Relaxed), 0, "startup isn't starvation");
        assert_eq!(pcm.push(&block(192, 1000)), 192);
        reader.render(&mut out);
        assert_eq!(&out[128..], &block(64, 1000));
        reader.render(&mut out);
        assert!(out[..128].iter().any(|s| *s != 0), "play the available prefix");
        assert!(out[128..].iter().all(|s| *s == 0), "zero the missing tail");
        reader.render(&mut out);
        assert!(out.iter().all(|s| *s == 0));
        assert_eq!(pcm.underruns.load(Ordering::Relaxed), 1, "count a gap once");
        pcm.push(&block(128, 1000));
        reader.render(&mut out);
        assert_eq!(out[0], 0, "fade back without a click");
        assert_eq!(out[200], 1000);
        assert_eq!(pcm.underruns.load(Ordering::Relaxed), 1);
    }
}
