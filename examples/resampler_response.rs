//! Measure passband gain of the complete integer stereo converter at 65 frequencies per rate/skew.
//! rustc --edition=2021 -O examples/resampler_response.rs -o /tmp/response; /tmp/response > response.csv
#[path = "../src/resample.rs"]
#[allow(dead_code)]
mod resample;

fn main() {
    println!("rate,skew,hz,gain_db");
    for rate in [22050.0f64, 32040.0, 32768.0, 44100.0, 48000.0, 65536.0] {
        for skew in [-0.005, 0.0, 0.005] {
            for k in 0..=64 {
                let hz = rate.min(48000.0) * (0.02 + 0.43 * k as f64 / 64.0);
                let input: Vec<i16> = (0..(rate * 0.15) as usize).flat_map(|i| {
                    let s = (23000.0 * (std::f64::consts::TAU * hz * i as f64 / rate).sin()).round() as i16;
                    [s, -s]
                }).collect();
                let mut r = resample::Resampler::new(true);
                r.prepare(rate / 48000.0);
                let mut out = Vec::new();
                for chunk in input.chunks(2 * 546) {
                    r.process(chunk, rate / 48000.0 * (1.0 + skew), &mut out);
                }
                let (mut ss, mut cc, mut sc, mut ys, mut yc) = (0.0, 0.0, 0.0, 0.0, 0.0);
                for (i, sample) in out.chunks_exact(2).enumerate().skip(480) {
                    let phase = std::f64::consts::TAU * hz * (1.0 + skew) * i as f64 / 48000.0;
                    let (s, c) = phase.sin_cos();
                    ss += s * s; cc += c * c; sc += s * c;
                    ys += sample[0] as f64 * s; yc += sample[0] as f64 * c;
                }
                let determinant = ss * cc - sc * sc;
                let a = (ys * cc - yc * sc) / determinant;
                let b = (yc * ss - ys * sc) / determinant;
                println!("{rate},{skew},{hz:.6},{:.8}", 20.0 * (a.hypot(b) / 23000.0).log10());
            }
        }
    }
}
