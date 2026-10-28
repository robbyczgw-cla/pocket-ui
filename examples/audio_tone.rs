//! A Game Boy Advance's sound path without the game: a sine at mGBA's 65536 Hz, one frame's worth per 60 Hz refresh
//! (as pocket-core feeds it, "one per refresh"), through `pocket_ui::audio::Audio` to the default PipeWire sink.
//! scripts/audio-bench records the sink and measures dropouts, clicks, THD+N and the pitch (its README.md).
//! `cargo run --release --example audio_tone -- [seconds] [hz]` (default 8 s, 1000 Hz).
//! AUDIO_TONE_PAUSES="3:4,6:6.5" pauses as the pause menu does (`Audio::pause`, no core frames meanwhile), from 3 s to
//! 4 s and so on. The tone fades in over 10 ms like a game's first sound; it ends the way a game is quit, through the
//! pause menu (`pause`, then the audio is dropped). AUDIO_TONE_REFRESH=60.06 runs the "display" 0.1 % fast against the
//! sound clock (the rate control has to make that up), default 60. The first frame comes 100 ms after `Audio::open`,
//! as in pocket-core, which opens the sound beside loading the core (its log: "first frame after 102-282 ms").

use pocket_ui::audio::Audio;
use std::time::{Duration, Instant};

const CORE_RATE: f64 = 65536.0;
const CORE_FPS: f64 = 59.7275;
const REFRESH: f64 = 60.0;

fn main() {
    let mut args = std::env::args().skip(1);
    let secs: f64 = args.next().and_then(|a| a.parse().ok()).unwrap_or(8.0);
    let hz: f64 = args.next().and_then(|a| a.parse().ok()).unwrap_or(1000.0);
    let pauses: Vec<(f64, f64)> = std::env::var("AUDIO_TONE_PAUSES")
        .unwrap_or_default()
        .split(',')
        .filter_map(|p| p.split_once(':').and_then(|(a, b)| Some((a.parse().ok()?, b.parse().ok()?))))
        .collect();
    let mut audio = Audio::open(CORE_RATE, false).expect("no audio server");
    audio.set_input_rate(CORE_RATE * REFRESH / CORE_FPS);
    println!("audio_tone: {hz} Hz for {secs} s via {}, pauses {pauses:?}", audio.backend);
    let per_frame = CORE_RATE / CORE_FPS;
    let fade_in = CORE_RATE * 0.010;
    let (mut owed, mut n, mut paused) = (0.0f64, 0u64, false);
    let refresh: f64 = std::env::var("AUDIO_TONE_REFRESH").ok().and_then(|v| v.parse().ok()).unwrap_or(REFRESH);
    let period = Duration::from_secs_f64(1.0 / refresh);
    std::thread::sleep(Duration::from_millis(100));
    let t0 = Instant::now();
    let mut next = t0;
    while t0.elapsed().as_secs_f64() < secs {
        let now = t0.elapsed().as_secs_f64();
        let pause = pauses.iter().any(|&(a, b)| now >= a && now < b);
        if pause != paused {
            audio.pause(pause);
            paused = pause;
        }
        if !paused {
            owed += per_frame;
            let count = owed as usize;
            owed -= count as f64;
            let mut frame = Vec::with_capacity(count * 2);
            for _ in 0..count {
                let gain = (n as f64 / fade_in).min(1.0);
                let s = ((n as f64 / CORE_RATE * hz * std::f64::consts::TAU).sin() * 16000.0 * gain) as i16;
                frame.extend([s, s]);
                n += 1;
            }
            audio.push(&frame);
        }
        next += period;
        if let Some(wait) = next.checked_duration_since(Instant::now()) {
            std::thread::sleep(wait);
        }
    }
    let (underruns, queue, server) = (*audio.underruns.lock().unwrap(), audio.buffered_ms(), audio.server_ms());
    audio.pause(true);
    std::thread::sleep(Duration::from_millis(150));
    drop(audio);
    println!("audio_tone: underruns {underruns}  queue {queue:.0} ms  server {server:.0} ms");
}
