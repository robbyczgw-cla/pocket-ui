//! Sample-rate conversion for pocket-core's sound (`audio::Audio::push`): the core's rate to 48 kHz, with a ratio that
//! the rate control nudges by up to ±0.5 % at every push.
//!
//! Two converters behind one type. The default is a windowed-sinc interpolator: a Kaiser-windowed sinc of 72 taps
//! (β = 11, cutoff at half the core's rate), tabulated in 512 phases, two neighbouring phases blended by the
//! fractional position. It keeps what is below about 0.45 of the core's rate flat and puts everything the core's
//! rate cannot carry (images above half of it) about 100 dB down. A core rate above the output's (65536 Hz)
//! lowers the cutoff and widens the kernel by the ratio. The position into the input is continuous, so any change of
//! ratio between pushes is as smooth as the ratio itself: no click, no state to reset. Delay: 36 input samples
//! (1.1 ms at 32 kHz, 0.55 ms at 65 kHz).
//!
//! The other is the linear interpolation pocket-core used before (images at -68 dB); `POCKET_CORE_RESAMPLER=linear`
//! switches back to it, for an emergency only (no menu entry).
//!
//! The kernel is our own code (no crate); rubato (MIT) was the alternative, and its sinc interpolator is the same idea.

use std::sync::OnceLock;

/// Taps on each side of the interpolation point; the kernel is 2 * HALF long.
pub const HALF: usize = 36;
/// Phases the kernel is tabulated in between two input samples.
const PHASES: usize = 512;
/// Kaiser window shape: about 100 dB of stopband for 72 taps.
const BETA: f64 = 11.0;
/// Widest ratio (core rate over output rate) with a proper low-pass; above it the sound is not filtered further.
const MAX_STEP: f64 = 4.0;
/// Samples kept in front of the read position: the widest kernel half.
const KEEP: usize = HALF * MAX_STEP as usize;

struct Tables {
    /// `h(i / PHASES)` for the whole half kernel: sinc times the Kaiser window.
    half: Vec<f32>,
    /// `PHASES + 1` rows of `2 * HALF` taps, each row summing to one: the kernel for a fractional position of
    /// `row / PHASES` (row `PHASES` is a whole sample further).
    rows: Vec<f32>,
}

fn bessel_i0(x: f64) -> f64 {
    let (mut sum, mut term, q) = (1.0, 1.0, x * x / 4.0);
    for k in 1..60 {
        term *= q / (k * k) as f64;
        sum += term;
        if term < sum * 1e-17 {
            break;
        }
    }
    sum
}

fn tables() -> &'static Tables {
    static T: OnceLock<Tables> = OnceLock::new();
    T.get_or_init(|| {
        let norm = bessel_i0(BETA);
        let h: Vec<f64> = (0..=HALF * PHASES)
            .map(|i| {
                let x = i as f64 / PHASES as f64;
                let sinc = if i == 0 { 1.0 } else { (std::f64::consts::PI * x).sin() / (std::f64::consts::PI * x) };
                let r = x / HALF as f64;
                sinc * if r >= 1.0 { 0.0 } else { bessel_i0(BETA * (1.0 - r * r).sqrt()) / norm }
            })
            .collect();
        let mut rows = Vec::with_capacity((PHASES + 1) * 2 * HALF);
        for phi in 0..=PHASES {
            let start = rows.len();
            for k in 0..2 * HALF {
                // The tap k sits (HALF - 1 - k) input samples before the whole sample below the position.
                let i = (phi as i64 + (HALF as i64 - 1 - k as i64) * PHASES as i64).unsigned_abs() as usize;
                rows.push(h[i]);
            }
            let sum: f64 = rows[start..].iter().sum();
            rows[start..].iter_mut().for_each(|c| *c /= sum);
        }
        let mut half: Vec<f32> = h.iter().map(|&c| c as f32).collect();
        half.push(0.0);
        Tables { half, rows: rows.into_iter().map(|c| c as f32).collect() }
    })
}

/// Eight partial sums so the compiler can vectorise.
fn dot(a: &[f32], x: &[f32]) -> f32 {
    let mut acc = [0f32; 8];
    for (a, x) in a.chunks_exact(8).zip(x.chunks_exact(8)) {
        for i in 0..8 {
            acc[i] += a[i] * x[i];
        }
    }
    acc.iter().sum()
}

fn to_i16(v: f32) -> i16 {
    v.round().clamp(-32768.0, 32767.0) as i16
}

/// Stereo converter. `step` is how far to move through the input per output sample (the core's rate over the
/// output's, times the rate control's nudge).
pub struct Resampler {
    sinc: Option<Sinc>,
    /// Linear: fractional read position into `last`.
    pos: f64,
    last: [i16; 2],
}

struct Sinc {
    l: Vec<f32>,
    r: Vec<f32>,
    /// Read position in `l` / `r` (input samples).
    pos: f64,
}

impl Resampler {
    /// `sinc` false: the old linear interpolation.
    pub fn new(sinc: bool) -> Resampler {
        let sinc = sinc.then(|| {
            tables();
            Sinc { l: vec![0.0; KEEP], r: vec![0.0; KEEP], pos: KEEP as f64 }
        });
        Resampler { sinc, pos: 0.0, last: [0, 0] }
    }

    /// The converter the environment asks for: sinc unless `POCKET_CORE_RESAMPLER=linear`.
    pub fn from_env() -> Resampler {
        Resampler::new(std::env::var("POCKET_CORE_RESAMPLER").map_or(true, |v| v != "linear"))
    }

    /// Converts `input` (interleaved stereo) and appends the output (interleaved) to `out`.
    pub fn process(&mut self, input: &[i16], step: f64, out: &mut Vec<i16>) {
        match &mut self.sinc {
            None => {
                for frame in input.chunks_exact(2) {
                    let next = [frame[0], frame[1]];
                    while self.pos < 1.0 {
                        let t = self.pos;
                        out.push((self.last[0] as f64 * (1.0 - t) + next[0] as f64 * t) as i16);
                        out.push((self.last[1] as f64 * (1.0 - t) + next[1] as f64 * t) as i16);
                        self.pos += step;
                    }
                    self.pos -= 1.0;
                    self.last = next;
                }
            }
            Some(s) => s.process(input, step, out),
        }
    }
}

impl Sinc {
    fn process(&mut self, input: &[i16], step: f64, out: &mut Vec<i16>) {
        self.l.extend(input.chunks_exact(2).map(|f| f[0] as f32));
        self.r.extend(input.chunks_exact(2).map(|f| f[1] as f32));
        let step = step.clamp(1.0 / MAX_STEP, MAX_STEP);
        // Up to 2 % over the output's rate (a 48 kHz core, nudged) keeps the fast path: its cutoff is the output's
        // Nyquist, and the 1 % of band between that and the wider one holds nothing.
        let wide = if step <= 1.02 { 1.0 } else { step };
        let need = (HALF as f64 * wide).ceil() as usize;
        let t = tables();
        out.reserve((self.l.len() as f64 / step) as usize * 2 + 2);
        while (self.pos as usize) + need < self.l.len() {
            let n0 = self.pos as usize;
            let f = self.pos - n0 as f64;
            if wide == 1.0 {
                let fi = f * PHASES as f64;
                let i0 = fi as usize;
                let fr = (fi - i0 as f64) as f32;
                let a = &t.rows[i0 * 2 * HALF..][..2 * HALF];
                let b = &t.rows[(i0 + 1) * 2 * HALF..][..2 * HALF];
                let base = n0 + 1 - HALF;
                for ch in [&self.l, &self.r] {
                    let x = &ch[base..base + 2 * HALF];
                    let (ya, yb) = (dot(a, x), dot(b, x));
                    out.push(to_i16(ya + fr * (yb - ya)));
                }
            } else {
                // Rate above the output's: low-pass at the output's Nyquist, a kernel `wide` times as long.
                let scale = 1.0 / wide;
                let lo = (self.pos - HALF as f64 * wide).ceil() as usize;
                let hi = (self.pos + HALF as f64 * wide).floor() as usize;
                let (mut yl, mut yr, mut sum) = (0f32, 0f32, 0f32);
                for j in lo..=hi {
                    let u = (self.pos - j as f64).abs() * scale * PHASES as f64;
                    let i = u as usize;
                    let w = t.half[i] + (u - i as f64) as f32 * (t.half[i + 1] - t.half[i]);
                    yl += w * self.l[j];
                    yr += w * self.r[j];
                    sum += w;
                }
                out.push(to_i16(yl / sum));
                out.push(to_i16(yr / sum));
            }
            self.pos += step;
        }
        let drop = (self.pos as usize).saturating_sub(KEEP);
        if drop > 0 {
            self.l.drain(..drop);
            self.r.drain(..drop);
            self.pos -= drop as f64;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(hz: f64, rate: f64, secs: f64, amp: f64) -> Vec<i16> {
        (0..(rate * secs) as usize)
            .flat_map(|n| {
                let v = (amp * (2.0 * std::f64::consts::PI * hz * n as f64 / rate).sin()).round() as i16;
                [v, v]
            })
            .collect()
    }

    /// Goertzel magnitude of the left channel at `hz`, normalised to the amplitude of a sine.
    fn level(out: &[i16], hz: f64, rate: f64) -> f64 {
        let (mut re, mut im) = (0.0, 0.0);
        let n = out.len() / 2;
        for i in 0..n {
            let w = 0.5 - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / n as f64).cos();
            let ph = 2.0 * std::f64::consts::PI * hz * i as f64 / rate;
            re += w * out[2 * i] as f64 * ph.cos();
            im += w * out[2 * i] as f64 * ph.sin();
        }
        (re * re + im * im).sqrt() * 2.0 / (n as f64 * 0.5)
    }

    fn run(r: &mut Resampler, input: &[i16], step: f64) -> Vec<i16> {
        let mut out = Vec::new();
        for chunk in input.chunks(2 * 548) {
            r.process(chunk, step, &mut out);
        }
        out
    }

    #[test]
    fn rows_sum_to_one_and_are_symmetric() {
        let t = tables();
        for phi in [0, 1, 255, 256, 511, 512] {
            let row = &t.rows[phi * 2 * HALF..][..2 * HALF];
            let sum: f32 = row.iter().sum();
            assert!((sum - 1.0).abs() < 1e-5, "row {phi} sums to {sum}");
        }
        // The half-way row is symmetric about the middle.
        let row = &t.rows[PHASES / 2 * 2 * HALF..][..2 * HALF];
        for k in 0..2 * HALF {
            assert!((row[k] - row[2 * HALF - 1 - k]).abs() < 1e-6);
        }
    }

    #[test]
    fn a_tone_keeps_its_level_and_its_image_is_far_down() {
        // GBA's 32768 Hz to 48 kHz: 10 kHz passes at unity, its image (32768 - 10000 = 22768 Hz) is below -90 dB.
        let step = 32768.0 / 48000.0;
        let mut r = Resampler::new(true);
        let out = run(&mut r, &tone(10_000.0, 32768.0, 2.0, 16000.0), step);
        let out = &out[2 * 4800..]; // past the start
        assert!((level(out, 10_000.0, 48000.0) / 16000.0 - 1.0).abs() < 0.001);
        let image = level(out, 22_768.0, 48000.0) / 16000.0;
        assert!(20.0 * image.log10() < -90.0, "image at {} dB", 20.0 * image.log10());
        // The linear one leaves it much higher.
        let mut l = Resampler::new(false);
        let out = run(&mut l, &tone(10_000.0, 32768.0, 2.0, 16000.0), step);
        let image = level(&out[2 * 4800..], 22_768.0, 48000.0) / 16000.0;
        assert!(20.0 * image.log10() > -60.0);
    }

    #[test]
    fn a_core_rate_above_the_output_is_filtered_down() {
        // 65536 Hz: a 30 kHz tone is above the output's Nyquist and must not fold to 18 kHz.
        let step = 65536.0 / 48000.0;
        let mut r = Resampler::new(true);
        let out = run(&mut r, &tone(30_000.0, 65536.0, 2.0, 16000.0), step);
        let fold = level(&out[2 * 4800..], 48_000.0 - 30_000.0, 48000.0) / 16000.0;
        assert!(20.0 * fold.log10() < -80.0, "fold at {} dB", 20.0 * fold.log10());
        let out = run(&mut Resampler::new(true), &tone(5_000.0, 65536.0, 2.0, 16000.0), step);
        assert!((level(&out[2 * 4800..], 5_000.0, 48000.0) / 16000.0 - 1.0).abs() < 0.001);
    }

    #[test]
    fn the_output_count_follows_the_step_and_chunks_do_not_matter() {
        let input = tone(1000.0, 32768.0, 1.0, 10000.0);
        let step = 32768.0 / 48000.0;
        let whole = run(&mut Resampler::new(true), &input, step);
        let mut one = Vec::new();
        let mut r = Resampler::new(true);
        for chunk in input.chunks(2 * 37) {
            r.process(chunk, step, &mut one);
        }
        assert_eq!(whole, one, "chunking changes the result");
        let frames = whole.len() as f64 / 2.0;
        // One second in, minus the kernel's HALF samples still to come.
        let expect = 48000.0 - HALF as f64 / step;
        assert!((frames - expect).abs() < 3.0, "{frames} frames for one second, expected {expect}");
    }

    #[test]
    fn a_ratio_change_makes_no_click() {
        // The rate control moves the step by tenths of a percent between pushes: the output stays a clean sine.
        let mut r = Resampler::new(true);
        let input = tone(1000.0, 32768.0, 2.0, 16000.0);
        let mut out = Vec::new();
        for (i, chunk) in input.chunks(2 * 546).enumerate() {
            let step = 32768.0 / 48000.0 * (1.0 + 0.005 * (i as f64 * 0.7).sin());
            r.process(chunk, step, &mut out);
        }
        // Second differences of a 1 kHz sine at 48 kHz stay below (2 pi f / fs)^2 * amp * 1.2; a click would exceed it.
        let l: Vec<f64> = out.chunks_exact(2).skip(2000).map(|f| f[0] as f64).collect();
        let bound = (2.0 * std::f64::consts::PI * 1000.0 / 48000.0f64).powi(2) * 16000.0 * 1.1 + 3.0 + 16000.0 * 0.005 * 0.1;
        let worst = l.windows(3).map(|w| (w[0] - 2.0 * w[1] + w[2]).abs()).fold(0.0, f64::max);
        assert!(worst < bound * 1.5, "second difference {worst} vs {bound}");
    }

    #[test]
    fn the_emergency_switch_is_the_old_linear_interpolation() {
        let mut r = Resampler::new(false);
        let out = run(&mut r, &[0, 0, 100, 100, 200, 200], 0.5);
        assert_eq!(out.iter().step_by(2).copied().collect::<Vec<_>>(), [0, 0, 0, 50, 100, 150]);
    }

    #[test]
    fn silence_stays_silent_and_full_scale_does_not_wrap() {
        let mut r = Resampler::new(true);
        let out = run(&mut r, &vec![0i16; 20000], 0.7);
        assert!(out.iter().all(|&s| s == 0));
        let out = run(&mut r, &vec![i16::MAX; 20000], 0.7);
        assert!(out.iter().skip(200).all(|&s| s > 32000), "DC full scale");
    }
}
