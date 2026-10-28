//! The unlock sound (and the screenshot's shutter, `Player::start_samples`): a short bright chime, mixed into the game's own sound in pocket-core (pocket-ui's mixer is not
//! used in games). It follows Settings > Sounds: off there, no chime; its volume is the UI sounds' level.
//! Synthesised like `ui_sound`'s, no sample file: three sine notes up (G5, D6, G6) with soft edges.

/// Samples of the chime (mono) at `rate` Hz, peak at `gain` of full scale.
pub fn synth(rate: f64, gain: f32) -> Vec<i16> {
    // (start ms, length ms, Hz, level)
    const NOTES: [(f64, f64, f64, f32); 4] = [
        (0.0, 140.0, 784.0, 0.8),
        (70.0, 160.0, 1175.0, 0.8),
        (140.0, 320.0, 1568.0, 1.0),
        (140.0, 320.0, 3136.0, 0.12),
    ];
    let n_of = |ms: f64| (ms * rate / 1000.0).round() as usize;
    let total = NOTES.iter().map(|(at, len, _, _)| n_of(*at) + n_of(*len)).max().unwrap_or(0);
    let mut mix = vec![0f32; total];
    for &(at, len, hz, level) in &NOTES {
        let (start, n) = (n_of(at), n_of(len));
        let attack = n_of(6.0).max(1);
        for i in 0..n {
            let t = i as f64 / rate;
            let env = if i < attack {
                i as f64 / attack as f64
            } else {
                (-(t * 1000.0 - 6.0) / (len * 0.35)).exp()
            };
            // The last 4 ms fade to exactly zero.
            let tail = ((n - 1 - i) as f64 / n_of(4.0).max(1) as f64).min(1.0);
            mix[start + i] += level * (env * tail) as f32 * (std::f64::consts::TAU * hz * t).sin() as f32;
        }
    }
    let peak = mix.iter().fold(0f32, |m, v| m.max(v.abs())).max(1e-6);
    let scale = gain.clamp(0.0, 1.0) * 0.5 * i16::MAX as f32 / peak;
    mix.iter().map(|v| (v * scale).round() as i16).collect()
}

/// Plays the chime over the game's sound, a frame's samples at a time.
#[derive(Default)]
pub struct Player {
    samples: Vec<i16>,
    at: usize,
}

impl Player {
    /// Starts the chime (again, from its beginning).
    pub fn start(&mut self, rate: f64, gain: f32) {
        if gain > 0.0 {
            self.samples = synth(rate, gain);
            self.at = 0;
        }
    }

    /// Plays other mono `samples` the same way (the screenshot's shutter), scaled by `gain`; nothing at gain 0.
    pub fn start_samples(&mut self, samples: Vec<i16>, gain: f32) {
        if gain > 0.0 {
            self.samples = samples.into_iter().map(|s| (s as f32 * gain.min(1.0)).round() as i16).collect();
            self.at = 0;
        }
    }

    pub fn playing(&self) -> bool {
        self.at < self.samples.len()
    }

    /// Adds the next part of the chime to interleaved stereo `frame`, saturating.
    pub fn mix(&mut self, frame: &mut [i16]) {
        for pair in frame.chunks_exact_mut(2) {
            let Some(&s) = self.samples.get(self.at) else { break };
            for c in pair {
                *c = c.saturating_add(s);
            }
            self.at += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_chime_is_short_soft_edged_and_follows_the_volume() {
        let s = synth(48000.0, 1.0);
        assert!((20_000..24_000).contains(&s.len()), "{} samples, under half a second", s.len());
        assert_eq!(s[0], 0);
        assert_eq!(*s.last().unwrap(), 0, "ends silent, no click");
        let peak = s.iter().map(|v| v.unsigned_abs()).max().unwrap();
        assert!((16_000..=16_400).contains(&peak), "{peak}: half of full scale at the loudest");
        let quiet = synth(48000.0, 0.25).iter().map(|v| v.unsigned_abs()).max().unwrap();
        assert!(quiet < peak / 3, "{quiet}");
        assert!(synth(32768.0, 1.0).len() < s.len(), "made for the core's own rate");
    }

    #[test]
    fn the_player_mixes_into_both_channels_and_stops() {
        let mut p = Player::default();
        p.start(48000.0, 0.0);
        assert!(!p.playing(), "Sounds off: nothing");
        p.start(48000.0, 1.0);
        let total = p.samples.len();
        let mut frame = vec![i16::MAX - 10; 1600];
        let mut played = 0;
        while p.playing() {
            frame.iter_mut().for_each(|s| *s = 100);
            p.mix(&mut frame);
            played += 800;
            assert!(frame.chunks(2).all(|c| c[0] == c[1]));
        }
        assert!(played >= total);
        let mut after = vec![5i16; 4];
        p.mix(&mut after);
        assert_eq!(after, vec![5; 4], "done");
        let mut loud = vec![i16::MAX; 2];
        p.start(48000.0, 1.0);
        p.at = 3000;
        p.mix(&mut loud);
        assert!(loud.iter().all(|v| *v >= i16::MAX - 20_000), "saturates instead of wrapping");
    }
}
