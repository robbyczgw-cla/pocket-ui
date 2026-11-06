//! The resampler on its own, off the device: test signals at the cores' real rates through the linear and the sinc
//! converter (`pocket_ui::resample`), written as raw s16le (left channel) for scripts/audio-bench/resampler.py, plus
//! the latency of each and its CPU time per second of audio.
//! `cargo run --release --example resampler_bench -- OUTDIR`
//! Files: `{linear,sinc}_{rate}_{case}[_s].s16` (48 kHz out), `in_{rate}_{case}.s16` (the input), `meta.txt`.
//! `_s`: the step 0.4 % off, as far as the rate control ever takes it (for 48000 that is the only case that is not
//! the identity). Cases: tone1000/5000/10000/14000, sweep (log, 20 Hz to 0.49 of the rate), noise, impulse.

#[path = "../src/resample.rs"]
#[allow(dead_code)]
mod resample;
use resample::Resampler;
use std::io::Write;
use std::time::Instant;

const OUT: f64 = 48000.0;
const RATES: [u32; 6] = [22050, 32040, 32768, 44100, 48000, 65536];
/// Frames per push: one 60 Hz refresh of a 32 kHz core.
const CHUNK: usize = 546;

fn run(sinc: bool, rate: f64, skew: f64, input: &[f32]) -> (Vec<i16>, f64) {
    let mut r = Resampler::new(sinc);
    r.prepare(rate / OUT);
    let step = rate / OUT * (1.0 + skew);
    let (mut out, mut lag, mut n) = (Vec::new(), 0.0, 0);
    let stereo: Vec<i16> = input.iter().flat_map(|&v| { let s = v.round().clamp(-32768.0, 32767.0) as i16; [s, s] }).collect();
    let mut m = 0usize;
    let mut tmp = Vec::new();
    for chunk in stereo.chunks(2 * CHUNK) {
        tmp.clear();
        r.process(chunk, step, &mut tmp);
        out.extend(tmp.chunks_exact(2).map(|f| f[0]));
        m += chunk.len() / 2;
        // Input seconds delivered minus output seconds emitted, from the middle of the run on.
        if m > stereo.len() / 4 {
            lag += m as f64 / rate - out.len() as f64 / OUT * (1.0 + skew);
            n += 1;
        }
    }
    (out, lag / n.max(1) as f64 * 1000.0)
}

fn save(dir: &str, name: &str, v: &[i16]) {
    let bytes: Vec<u8> = v.iter().flat_map(|s| s.to_le_bytes()).collect();
    std::fs::write(format!("{dir}/{name}.s16"), bytes).unwrap();
}

fn main() {
    let dir = std::env::args().nth(1).expect("OUTDIR");
    std::fs::create_dir_all(&dir).unwrap();
    let mut meta = std::fs::File::create(format!("{dir}/meta.txt")).unwrap();
    let mut seed = 0x9E3779B97F4A7C15u64;
    let mut rnd = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        (seed >> 11) as f64 / (1u64 << 53) as f64 - 0.5
    };
    for &rate in &RATES {
        let rf = rate as f64;
        let secs = 3.0;
        let n = (rf * secs) as usize;
        let mut cases: Vec<(String, Vec<f32>)> = Vec::new();
        for hz in [1000.0, 5000.0, 10000.0, 14000.0] {
            let tone = (0..n).map(|i| (0.7 * 32767.0 * (2.0 * std::f64::consts::PI * hz * i as f64 / rf).sin()) as f32).collect();
            cases.push((format!("tone{}", hz as u32), tone));
        }
        if rate == 65536 {
            for hz in [20000.0, 26000.0, 30000.0] {
                let tone = (0..n).map(|i| (0.7 * 32767.0 * (2.0 * std::f64::consts::PI * hz * i as f64 / rf).sin()) as f32).collect();
                cases.push((format!("tone{}", hz as u32), tone));
            }
        }
        // Log sweep over 6 s.
        let sn = (rf * 6.0) as usize;
        let (f0, f1) = (20.0f64, 0.49 * rf);
        let k = (f1 / f0).ln() / 6.0;
        let sweep = (0..sn).map(|i| {
            let t = i as f64 / rf;
            let ph = 2.0 * std::f64::consts::PI * f0 * ((k * t).exp() - 1.0) / k;
            (0.5 * 32767.0 * ph.sin()) as f32
        }).collect();
        cases.push(("sweep".into(), sweep));
        // Noise, white, about -14 dBFS rms (sum of four uniforms).
        let noise = (0..(rf * 10.0) as usize).map(|_| ((rnd() + rnd() + rnd() + rnd()) * 0.5 * 32767.0 * 0.8) as f32).collect();
        cases.push(("noise".into(), noise));
        let mut imp = vec![0f32; 16000];
        imp[4000] = 20000.0;
        cases.push(("impulse".into(), imp));
        for (name, input) in &cases {
            let i16s: Vec<i16> = input.iter().map(|&v| v.round().clamp(-32768.0, 32767.0) as i16).collect();
            save(&dir, &format!("in_{rate}_{name}"), &i16s);
            for (mode, sinc) in [("linear", false), ("sinc", true)] {
                for (suffix, skew) in [("", 0.0), ("_s", 0.004)] {
                    let (out, lag) = run(sinc, rf, skew, input);
                    save(&dir, &format!("{mode}_{rate}_{name}{suffix}"), &out);
                    writeln!(meta, "{mode} {rate} {name}{suffix} lag_ms {lag:.4}").unwrap();
                }
            }
        }
        // CPU: 60 s of noise, stereo, in 60 Hz pushes; the best of three.
        let noise: Vec<f32> = (0..(rf * 60.0) as usize).map(|_| (rnd() * 20000.0) as f32).collect();
        for (mode, sinc) in [("linear", false), ("sinc", true)] {
            let best = (0..3).map(|_| { let t = Instant::now(); run(sinc, rf, 0.001, &noise); t.elapsed().as_secs_f64() }).fold(f64::MAX, f64::min);
            writeln!(meta, "{mode} {rate} cpu_ms_per_audio_s {:.3}", best / 60.0 * 1000.0).unwrap();
        }
    }
}
