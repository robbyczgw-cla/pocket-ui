//! The battery's own discharge curve: how much charge each point of the gauge holds on this Pocket, learned from its
//! history, so the time left follows the real curve instead of a straight line through the percentages.
//!
//! Why: the gauge's points are not equal. The RG35XX Plus (axp717) fell 30 points an hour around 3.7 V and 2 to 5
//! points an hour under 20 %, at the same steady draw all night (23./24.11.2026, about 423 mA with the screen dark): at
//! 40 % the straight line said 2 h 21 min left, it ran 6 h 10 min more.
//!
//! * [`Curve::learn`] walks the history: within one stretch on battery (no gap, no charger, no jump, the gauge never
//!   going up) the charge used between two drops of the gauge belongs to the point it showed meanwhile. The charge is
//!   current x time where the gauge reports a current (the Plus, the Mini V2), time alone where it doesn't (the Brick:
//!   there the curve also holds what ran meanwhile). The first drop of a stretch starts the count, 100 % is never
//!   learned (time at 100 is the charger's top-up as much as use). Each point keeps a running mean of its last
//!   [`KEEP`] observations, so an older battery or a new firmware moves it. Only drops after [`Curve::upto`] are taken,
//!   so a history walked again counts nothing twice.
//! * [`Curve::usable`] is the share of the usable charge left at a reading, 100 at full and 0 at the low-battery
//!   switch-off (`low_battery::CRITICAL_AT`); points not learned yet count as the mean of those learned. Under
//!   [`MIN_POINTS`] learned points there is no curve and the estimate stays the straight line.
//!
//! The file (`battery-curve.json`, about 2 kB) is written with the battery per game, through `fsutil`.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::battery::Sample;
use crate::battery_estimate::continuous;

pub const FILE: &str = "battery-curve.json";
/// Observations a point keeps in its mean.
pub const KEEP: u8 = 4;
/// Learned points needed before the curve is used.
pub const MIN_POINTS: usize = 30;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Curve {
    /// Charge of each point 0..=100, in mAh with `amps`, in hours without; 0 = not learned.
    charge: Vec<f32>,
    /// Observations in each point's mean, at most [`KEEP`].
    seen: Vec<u8>,
    /// Whether the charge is current x time (the gauge reports a current) or time alone. Set by the first learning.
    amps: Option<bool>,
    /// Unix seconds of the newest sample learned from.
    pub upto: u64,
}

impl Curve {
    pub fn load(data_dir: &Path) -> Curve {
        crate::fsutil::read_json_or_backup(&data_dir.join(FILE)).unwrap_or_default()
    }

    pub fn save(&self, data_dir: &Path) -> std::io::Result<()> {
        crate::fsutil::write_json(&data_dir.join(FILE), self)
    }

    /// Learns from `samples` (oldest first) what their drops after [`Self::upto`] say; true when a point changed.
    pub fn learn(&mut self, samples: &[Sample]) -> bool {
        if self.charge.len() != 101 || self.seen.len() != 101 {
            (self.charge, self.seen) = (vec![0.0; 101], vec![0; 101]);
        }
        let amps = *self.amps.get_or_insert_with(|| samples.iter().filter(|s| !s.plugged()).any(|s| s.ma != 0));
        let mut changed = false;
        // The charge used since the last drop of this stretch; None before its first drop.
        let mut since: Option<f64> = None;
        for pair in samples.windows(2) {
            let (a, b) = (&pair[0], &pair[1]);
            if !continuous(a, b) || a.plugged() || b.plugged() || b.capacity > a.capacity || (amps && (a.ma == 0 || b.ma == 0)) {
                since = None;
                continue;
            }
            let hours = (b.t - a.t) as f64 / 3600.0;
            // mAh: the mean of the two readings over the time between them.
            let used = if amps { hours * (a.ma.unsigned_abs() as f64 + b.ma.unsigned_abs() as f64) / 2.0 } else { hours };
            if let Some(x) = since.as_mut() {
                *x += used;
            }
            if b.capacity < a.capacity {
                if let Some(x) = since.filter(|_| a.capacity < 100 && b.t > self.upto) {
                    let k = (a.capacity - b.capacity) as f64;
                    for p in b.capacity + 1..=a.capacity {
                        self.observe(p as usize, (x / k) as f32);
                    }
                    changed = true;
                }
                since = Some(0.0);
            }
        }
        if let Some(last) = samples.last() {
            self.upto = self.upto.max(last.t);
        }
        changed
    }

    fn observe(&mut self, point: usize, charge: f32) {
        let n = (self.seen[point] + 1).min(KEEP);
        self.charge[point] += (charge - self.charge[point]) / n as f32;
        self.seen[point] = n;
    }

    /// The share of the usable charge left at `capacity`, 0..=100 (0 at the low-battery switch-off); None while too
    /// little is learned.
    pub fn usable(&self, capacity: u8) -> Option<f32> {
        let floor = crate::low_battery::CRITICAL_AT as usize;
        let learned: Vec<f32> = (floor + 1..100).filter(|&p| self.seen.get(p).is_some_and(|&n| n > 0)).map(|p| self.charge[p]).collect();
        if learned.len() < MIN_POINTS {
            return None;
        }
        let mean = learned.iter().sum::<f32>() / learned.len() as f32;
        let of = |p: usize| if p < 100 && self.seen[p] > 0 { self.charge[p] } else { mean };
        let total: f32 = (floor + 1..=100).map(of).sum();
        let left: f32 = (floor + 1..=(capacity as usize).min(100)).map(of).sum();
        Some(100.0 * left / total)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const T0: u64 = 1_790_000_000 / 60 * 60;

    fn s(min: u64, capacity: u8, ma: i32) -> Sample {
        Sample { t: T0 + min * 60, capacity, charging: false, full: false, mv: 3800, ma, temp: None, tag: 0 }
    }

    /// A gauge whose points 40..=21 go twice as fast as the rest, at a steady 400 mA: a sample a minute.
    fn steep_night() -> Vec<Sample> {
        let mut v = Vec::new();
        let mut t = 0;
        for cap in (3..=99u8).rev() {
            let minutes = if (21..=40).contains(&cap) { 3 } else { 6 };
            for _ in 0..minutes {
                v.push(s(t, cap, 400));
                t += 1;
            }
        }
        v.push(s(t, 2, 400));
        v
    }

    #[test]
    fn a_steep_stretch_of_the_gauge_holds_less_charge() {
        let mut c = Curve::default();
        assert!(c.learn(&steep_night()));
        let at40 = c.usable(40).unwrap();
        // Points 3..=20 hold 6 minutes each, 21..=40 three: 18*6 + 20*3 = 168 of 18*6 + 20*3 + 59*6 + 6 (100, the mean).
        let want = 100.0 * 168.0 / (168.0 + 59.0 * 6.0 + 6.0);
        assert!((at40 - want).abs() < 0.5, "{at40} vs {want}");
        assert_eq!(c.usable(2), Some(0.0), "the switch-off");
        assert!((c.usable(100).unwrap() - 100.0).abs() < 0.01);
    }

    /// The Plus's case in small: at 40 %, just where the gauge starts to run fast, the straight line through the
    /// percentages of the last 90 minutes promises far too much time; the curve learned the night before is right.
    #[test]
    fn the_time_left_follows_the_curve() {
        let night = steep_night();
        let mut c = Curve::default();
        c.learn(&night);
        let at = night.iter().position(|x| x.capacity == 40).unwrap();
        let real = night.iter().find(|x| x.capacity == 2).unwrap().t - night[at].t;
        let secs = |curve: Option<&Curve>| match crate::battery_estimate::estimate_with(&night[..=at], night[at].t, curve) {
            Some(crate::battery_estimate::Estimate::Left { secs, curve: used, .. }) => (secs, used),
            other => panic!("{other:?}"),
        };
        let (line, used) = secs(None);
        assert!(!used);
        let (by_curve, used) = secs(Some(&c));
        assert!(used);
        let off = |s: u64| (s as f64 - real as f64).abs() / real as f64;
        assert!(off(by_curve) < 0.1, "curve {by_curve} s, real {real} s");
        assert!(off(line) > 0.3, "the straight line {line} s, real {real} s: this case shows nothing");
    }

    /// The pause menu's time left for a game (its rate from the ledger, over all its play) goes by the same usable charge
    /// as the Battery page: at 40 % on the Plus-like gauge the straight line is 43 % too long, the curve right.
    #[test]
    fn a_games_time_left_follows_the_curve_too() {
        use crate::battery_games::{left_with, pair, Basis, Left, Rate};
        let night = steep_night();
        let mut c = Curve::default();
        c.learn(&night);
        let at = night.iter().position(|x| x.capacity == 40).unwrap();
        let real = (night.iter().find(|x| x.capacity == 2).unwrap().t - night[at].t) as f64;
        // The ledger as the sampler feeds it: points, and usable charge through the curve.
        let mut ledger = crate::battery_games::Ledger::default();
        let key = crate::battery_games::Key::new("gba", std::path::Path::new("a.gba"), "pocket-core");
        for w in night[..=at].windows(2) {
            if let Some((secs, drop)) = pair(&w[0], &w[1]) {
                let used = (c.usable(w[0].capacity).unwrap() - c.usable(w[1].capacity).unwrap()) as f64;
                ledger.add_used(&key, secs, drop, used, w[1].t);
            }
        }
        let rate: Rate = ledger.rate(&key).unwrap();
        assert_eq!(rate.basis, Basis::Game);
        let time = |curve: Option<&Curve>| match left_with(Some(rate), Some(&night[at]), night[at].t, curve) {
            Some(Left::Time { secs, .. }) => secs as f64,
            other => panic!("{other:?}"),
        };
        assert!((time(Some(&c)) - real).abs() / real < 0.05, "curve {} s, real {real} s", time(Some(&c)));
        assert!((time(None) - real).abs() / real > 0.3, "the straight line {} s, real {real} s", time(None));
    }

    #[test]
    fn too_little_learned_is_no_curve() {
        let mut c = Curve::default();
        c.learn(&steep_night()[..6 * 20]);
        assert_eq!(c.usable(50), None, "18 points learned");
    }

    /// A history walked again (every minute, and at each start) counts nothing twice.
    #[test]
    fn walking_the_same_history_again_learns_nothing_new() {
        let night = steep_night();
        let mut c = Curve::default();
        c.learn(&night);
        let once = c.clone();
        assert!(!c.learn(&night));
        assert_eq!(c, once);
    }

    /// The charger, a gap (the Pocket slept) or a gauge that went up ends the count: what lies across them is no point's.
    #[test]
    fn the_charger_a_gap_or_a_rise_starts_the_count_again() {
        let mut v = vec![s(0, 50, 400), s(1, 49, 400), s(2, 49, 400)];
        let mut plugged = s(3, 49, 400);
        plugged.charging = true;
        v.push(plugged);
        v.extend([s(4, 49, 400), s(5, 48, 400), s(400, 47, 400), s(401, 48, 400), s(402, 47, 400)]);
        let mut c = Curve::default();
        assert!(!c.learn(&v), "no point has a whole stretch between two drops");
    }

    /// The Brick's gauge reports no current: time alone is the charge.
    #[test]
    fn without_a_current_time_is_the_charge() {
        let night: Vec<Sample> = steep_night().into_iter().map(|mut x| { x.ma = 0; x }).collect();
        let mut c = Curve::default();
        c.learn(&night);
        assert_eq!(c.amps, Some(false));
        assert!((c.charge[30] - 3.0 / 60.0).abs() < 1e-4, "{}", c.charge[30]);
    }
}
