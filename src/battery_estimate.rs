//! What the battery history means in time: how long is left at the current rate, how long until full,
//! and what each system costs per hour.
//!
//! The estimate is data-honest. It needs 20 minutes on battery without a break and a drop of two points
//! before it says anything, otherwise "not enough data yet". The rate is the slope of `capacity` over the
//! newest run of samples (up to 90 minutes), a least-squares line so the gauge's whole numbers don't
//! matter, weighted so that every 20 minutes further back count half as much (half-life 20 min):
//! a game started ten minutes ago shows in the estimate, a long idle spell doesn't hold it back.
//!
//! A run ends at a gap in the samples (the device slept: what happened across the gap is not a rate), at
//! a change between battery and charger, and at a jump of the gauge (recalibration, a battery that
//! relearns its charge) that no real drain produces within three minutes. The first sample after waking
//! is therefore "not enough data yet", and a jump never ends up in a rate.
//!
//! `current_now` is not used for the time left: it is an instantaneous reading, and the Mini V2's gauge
//! offers no remaining-charge counter to turn it into hours (see `battery`). It is averaged per system
//! for the watts shown next to the rate.

use crate::battery::Sample;
use crate::battery_history::{History, GAP, KEEP};

/// At least this much continuous time on battery before there is a time left.
pub const MIN_SPAN: u64 = 20 * 60;
/// ... and a drop of this many points, or the rate is a rounding.
const MIN_DROP: f32 = 2.0;
/// Below this many points an hour nothing is said about hours left.
const MIN_RATE: f32 = 0.3;
/// While charging the capacity moves fast, so a few minutes tell enough.
const CHARGE_MIN_SPAN: u64 = 6 * 60;
const CHARGE_MIN_RISE: f32 = 3.0;
/// The newest samples used, at most.
const WINDOW: u64 = 90 * 60;
/// How much older samples count less: after this many seconds a sample counts half.
const HALF_LIFE: f64 = 20.0 * 60.0;
/// Two neighbouring samples further apart in capacity than this are not one continuous drain or charge.
const JUMP: i32 = 6;
/// A system needs this much time on battery before its rate is shown.
const DRAW_MIN: u64 = 15 * 60;

/// What can honestly be said about the time left.
#[derive(Clone, Debug, PartialEq)]
pub enum Estimate {
    /// On battery: the drain in points an hour over `over` seconds, and the seconds until empty at that rate.
    Left {
        per_hour: f32,
        secs: u64,
        over: u64,
    },
    /// Charging, with the time until full when the rise could be measured.
    Charging {
        full_in: Option<u64>,
        per_hour: f32,
    },
    Full,
    /// On the charger and not charging below full: the kernel says "Full" when charging stopped early (the Mini V2
    /// at 58 %), see [`Sample::held`](crate::battery::Sample::held).
    PluggedIn,
    /// Not enough samples on battery yet; `have` seconds of the [`MIN_SPAN`] needed.
    NotEnough {
        have: u64,
    },
}

/// Whether `a` → `b` is one stretch of drain or charge: no gap, no change between battery and charger, no jump of the gauge.
pub fn continuous(a: &Sample, b: &Sample) -> bool {
    b.t.saturating_sub(a.t) <= GAP && a.plugged() == b.plugged() && (a.capacity as i32 - b.capacity as i32).abs() <= JUMP
}

/// The newest run of samples in one state (all on battery, or all plugged in) without gaps or jumps, within `WINDOW`.
fn recent_run(samples: &[Sample]) -> &[Sample] {
    let Some(last) = samples.last() else { return &[] };
    let mut start = samples.len() - 1;
    while start > 0 && continuous(&samples[start - 1], &samples[start]) && samples[start - 1].t + WINDOW >= last.t {
        start -= 1;
    }
    &samples[start..]
}

/// Weighted least-squares slope of capacity over time, in points per hour; newer samples count more.
fn slope(run: &[Sample]) -> f32 {
    let last = run[run.len() - 1].t as f64;
    let (mut sw, mut sx, mut sy) = (0.0, 0.0, 0.0);
    let point = |s: &Sample| ((s.t as f64 - last) / 3600.0, s.capacity as f64, 0.5f64.powf((last - s.t as f64) / HALF_LIFE));
    for s in run {
        let (x, y, w) = point(s);
        sw += w;
        sx += w * x;
        sy += w * y;
    }
    let (mx, my) = (sx / sw, sy / sw);
    let (mut sxy, mut sxx) = (0.0, 0.0);
    for s in run {
        let (x, y, w) = point(s);
        sxy += w * (x - mx) * (y - my);
        sxx += w * (x - mx) * (x - mx);
    }
    if sxx > 0.0 {
        (sxy / sxx) as f32
    } else {
        0.0
    }
}

pub fn estimate(samples: &[Sample], now: u64) -> Option<Estimate> {
    let last = samples.last()?;
    if now.saturating_sub(last.t) > GAP {
        return Some(Estimate::NotEnough { have: 0 });
    }
    if last.held() {
        return Some(Estimate::PluggedIn);
    }
    if last.full {
        return Some(Estimate::Full);
    }
    let run = recent_run(samples);
    let span = last.t - run[0].t;
    let change = last.capacity as f32 - run[0].capacity as f32;
    let rate = slope(run);
    if last.charging {
        let ok = span >= CHARGE_MIN_SPAN && change >= CHARGE_MIN_RISE && rate > 1.0;
        let full_in = ok.then(|| ((100.0 - last.capacity as f32).max(0.0) / rate * 3600.0) as u64);
        return Some(Estimate::Charging {
            full_in,
            per_hour: rate.max(0.0),
        });
    }
    if span < MIN_SPAN || -change < MIN_DROP || -rate < MIN_RATE {
        return Some(Estimate::NotEnough { have: span });
    }
    Some(Estimate::Left {
        per_hour: -rate,
        secs: (last.capacity as f32 / -rate * 3600.0) as u64,
        over: span,
    })
}

/// What one system (or the menus) costs: points an hour while it ran on battery, the average draw in
/// watts when the gauge reports it, and the minutes that was measured over.
#[derive(Clone, Debug, PartialEq)]
pub struct Draw {
    /// The system id ("ps2"); None for the menus.
    pub tag: Option<String>,
    pub per_hour: f32,
    pub watts: Option<f32>,
    pub minutes: u64,
    /// When it last ran (unix seconds), to tell the game played last from the one played longest.
    pub last: u64,
}

/// The draw per system over the history's last 48 hours, the one that ran longest first. Only time on
/// battery counts, between neighbouring samples without a gap or jump and with the same system running;
/// a system with under 15 minutes of that or a drop under two points has no rate yet.
pub fn draws(h: &History, now: u64) -> Vec<Draw> {
    struct Sum {
        secs: u64,
        drop: i32,
        mw: u64,
        mw_n: u64,
        last: u64,
    }
    let mut sums: Vec<(u8, Sum)> = Vec::new();
    for pair in h.recent(now, KEEP).windows(2) {
        let (a, b) = (&pair[0], &pair[1]);
        if a.tag != b.tag || a.plugged() || b.plugged() || !continuous(a, b) {
            continue;
        }
        let i = match sums.iter().position(|(t, _)| *t == b.tag) {
            Some(i) => i,
            None => {
                sums.push((
                    b.tag,
                    Sum {
                        secs: 0,
                        drop: 0,
                        mw: 0,
                        mw_n: 0,
                        last: 0,
                    },
                ));
                sums.len() - 1
            }
        };
        let sum = &mut sums[i].1;
        sum.secs += b.t - a.t;
        sum.last = b.t;
        sum.drop += a.capacity as i32 - b.capacity as i32;
        if b.ma != 0 && b.mv != 0 {
            sum.mw += b.mw() as u64;
            sum.mw_n += 1;
        }
    }
    let mut out: Vec<Draw> = sums
        .into_iter()
        .filter(|(_, s)| s.secs >= DRAW_MIN && s.drop >= MIN_DROP as i32)
        .filter_map(|(tag, s)| {
            let name = if tag == 0 { None } else { Some(h.tag_name(tag)?.to_string()) };
            Some(Draw {
                tag: name,
                per_hour: s.drop as f32 * 3600.0 / s.secs as f32,
                watts: (s.mw_n > 0).then(|| (s.mw / s.mw_n) as f32 / 1000.0),
                minutes: s.secs / 60,
                last: s.last,
            })
        })
        .collect();
    out.sort_by(|a, b| b.minutes.cmp(&a.minutes));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: u64 = 1_790_000_000 / 60 * 60;

    fn s(t: u64, cap: f32, charging: bool) -> Sample {
        Sample {
            t,
            capacity: cap.round().clamp(0.0, 100.0) as u8,
            charging,
            full: false,
            mv: 3900,
            ma: 500,
            temp: None,
            tag: 0,
        }
    }

    /// A sample a minute for `minutes`, ending at `end`, falling (or, charging, rising) `per_hour` points an hour from `start`.
    fn curve(end: u64, minutes: u64, start: f32, per_hour: f32, charging: bool) -> Vec<Sample> {
        (0..=minutes)
            .map(|m| {
                let drift = per_hour * m as f32 / 60.0;
                s(end - (minutes - m) * 60, if charging { start + drift } else { start - drift }, charging)
            })
            .collect()
    }

    fn left(v: &[Sample]) -> (f32, u64, u64) {
        match estimate(v, v.last().unwrap().t) {
            Some(Estimate::Left { per_hour, secs, over }) => (per_hour, secs, over),
            other => panic!("expected Left, got {other:?}"),
        }
    }

    #[test]
    fn steady_discharge() {
        // 20 points an hour for an hour, from 80 to 60: 3 hours left.
        let v = curve(NOW, 60, 80.0, 20.0, false);
        let (rate, secs, over) = left(&v);
        assert!((rate - 20.0).abs() < 1.5, "{rate}");
        assert!((secs as f32 - 10_800.0).abs() < 900.0, "{secs}");
        assert_eq!(over, 3600);
        // 6 points an hour from 100 %: a long day.
        let v = curve(NOW, 60, 100.0, 6.0, false);
        let (rate, secs, _) = left(&v);
        assert!((rate - 6.0).abs() < 1.0, "{rate}");
        assert!((secs as f32 / 3600.0 - 15.7).abs() < 3.0, "{secs}");
    }

    #[test]
    fn the_whole_numbers_of_a_fuel_gauge_are_smoothed_out() {
        // 12 points an hour, but the gauge adds a point of noise every other minute.
        let v: Vec<Sample> = curve(NOW, 75, 90.0, 12.0, false)
            .into_iter()
            .enumerate()
            .map(|(i, mut x)| {
                if i % 2 == 1 {
                    x.capacity = (x.capacity + 1).min(100);
                }
                x
            })
            .collect();
        let (rate, secs, _) = left(&v);
        assert!((rate - 12.0).abs() < 2.5, "{rate}");
        assert!((secs as f32 / 3600.0 - 6.5).abs() < 1.5, "{secs}");
    }

    #[test]
    fn the_recent_drain_counts_more_than_the_old() {
        // An hour of menus at 10 points an hour, then half an hour of a heavy game at 30.
        let mut v = curve(NOW - 1800, 60, 90.0, 10.0, false);
        let from = v.last().unwrap().capacity as f32;
        v.extend(curve(NOW, 30, from, 30.0, false).into_iter().skip(1));
        let (rate, _, _) = left(&v);
        // A plain average over the 90 minutes would be 16.7; the newer half hour pulls the rate up, not all the way to 30.
        let plain = (v[0].capacity as f32 - v.last().unwrap().capacity as f32) / 1.5;
        assert!(rate > plain + 1.5 && rate < 30.0, "plain {plain}, weighted {rate}");
    }

    #[test]
    fn too_little_data_says_so() {
        // 10 minutes on battery: too short.
        let v = curve(NOW, 10, 80.0, 40.0, false);
        assert_eq!(estimate(&v, NOW), Some(Estimate::NotEnough { have: 600 }));
        // 30 minutes but the gauge did not move: no honest rate.
        let v: Vec<Sample> = (0..=30).map(|m| s(NOW - 1800 + m * 60, 80.0, false)).collect();
        assert!(matches!(estimate(&v, NOW), Some(Estimate::NotEnough { .. })));
        // A drop of one point in an hour is a rounding, not a rate.
        let v: Vec<Sample> = (0..=60).map(|m| s(NOW - 3600 + m * 60, if m < 30 { 80.0 } else { 79.0 }, false)).collect();
        assert!(matches!(estimate(&v, NOW), Some(Estimate::NotEnough { .. })));
        // The newest sample is old (pocket-ui wasn't running): nothing to say about now.
        let v = curve(NOW - 3600, 30, 80.0, 20.0, false);
        assert_eq!(estimate(&v, NOW), Some(Estimate::NotEnough { have: 0 }));
        // One sample, none.
        assert_eq!(estimate(&v[..1], NOW - 3600), Some(Estimate::NotEnough { have: 0 }));
        assert_eq!(estimate(&[], NOW), None);
        // Going up while "discharging" (a gauge catching up) is no drain.
        let v = curve(NOW, 40, 50.0, -10.0, false);
        assert!(matches!(estimate(&v, NOW), Some(Estimate::NotEnough { .. })), "{:?}", estimate(&v, NOW));
    }

    #[test]
    fn charging_gives_the_time_until_full() {
        // One point a minute: 60 % -> 100 % takes 40 more minutes.
        let v = curve(NOW, 20, 40.0, 60.0, true);
        let Some(Estimate::Charging { full_in: Some(secs), per_hour }) = estimate(&v, NOW) else {
            panic!("{:?}", estimate(&v, NOW))
        };
        assert!((per_hour - 60.0).abs() < 6.0, "{per_hour}");
        assert!((secs as f32 - 2400.0).abs() < 400.0, "{secs}");
        // Only just plugged in: no time yet, but it is known to be charging.
        let v = curve(NOW, 3, 40.0, 60.0, true);
        assert!(
            matches!(estimate(&v, NOW), Some(Estimate::Charging { full_in: None, .. })),
            "{:?}",
            estimate(&v, NOW)
        );
        // Full on the charger.
        let mut v = curve(NOW, 20, 90.0, 30.0, true);
        v.last_mut().unwrap().full = true;
        assert_eq!(estimate(&v, NOW), Some(Estimate::Full));
        // "Full" well below full: charging stopped early; plugged in, not full.
        let mut v = curve(NOW, 20, 58.0, 0.0, true);
        v.last_mut().unwrap().charging = false;
        v.last_mut().unwrap().full = true;
        assert_eq!(estimate(&v, NOW), Some(Estimate::PluggedIn));
    }

    #[test]
    fn plugging_in_starts_a_new_run() {
        let mut v = curve(NOW - 600, 60, 80.0, 20.0, false);
        v.extend(curve(NOW, 10, 62.0, 60.0, true).into_iter().skip(1));
        // 10 minutes of charging after an hour on battery: charging, and not "20 % an hour" mixed in.
        assert!(matches!(estimate(&v, NOW), Some(Estimate::Charging { .. })), "{:?}", estimate(&v, NOW));
        // Unplugged again for 5 minutes: not enough data, the earlier hour is another run.
        let mut w = v.clone();
        w.extend(curve(NOW + 300, 5, 72.0, 20.0, false).into_iter().skip(1));
        assert_eq!(estimate(&w, NOW + 300), Some(Estimate::NotEnough { have: 240 }));
    }

    #[test]
    fn a_jump_after_sleep_is_not_a_rate() {
        // 5 hours of play, 8 hours asleep (60 % -> 40 %), then the device wakes.
        let mut v = curve(NOW - 8 * 3600 - 3 * 60, 5 * 60, 90.0, 6.0, false);
        let before = v.last().unwrap().capacity;
        assert!((before as i32 - 60).abs() <= 1);
        // Just woke: three samples, no estimate and no jump of 20 points in it.
        v.extend(curve(NOW, 3, 40.0, 12.0, false));
        let got = estimate(&v, NOW);
        assert_eq!(got, Some(Estimate::NotEnough { have: 180 }), "only what happened since the wake counts");
        // Half an hour later: the rate of the half hour, 12 an hour, at 34 % = 2.8 h; not the jump.
        let mut w = v.clone();
        w.truncate(v.len() - 4);
        w.extend(curve(NOW + 1800, 30, 40.0, 12.0, false));
        let (rate, secs, over) = left(&w);
        assert!((rate - 12.0).abs() < 2.0, "{rate}");
        assert!((secs as f32 / 3600.0 - 34.0 / 12.0).abs() < 0.7, "{secs}");
        assert_eq!(over, 1800);
        // Charged while off: 30 % before, 80 % after. Same thing the other way round.
        let mut c = curve(NOW - 5 * 3600, 30, 45.0, 30.0, false);
        c.extend(curve(NOW, 40, 80.0, 12.0, false));
        let (rate, _, over) = left(&c);
        assert!((rate - 12.0).abs() < 2.0, "{rate}");
        assert_eq!(over, 2400);
    }

    #[test]
    fn a_jump_without_a_gap_starts_a_new_run_too() {
        // The gauge re-learns the battery: 25 minutes at 12 an hour, +9 points at once, 25 more minutes.
        let mut v = curve(NOW - 1500, 25, 70.0, 12.0, false);
        let top = v.last().unwrap().capacity as f32;
        v.extend(curve(NOW, 25, top + 9.0, 12.0, false).into_iter());
        let (rate, _, over) = left(&v);
        assert!((rate - 12.0).abs() < 2.0, "{rate}");
        assert_eq!(over, 1500);
    }

    fn history(parts: &[(&str, u64, f32, bool)]) -> (History, u64) {
        // (activity, minutes, points an hour, charging), one after the other, 10 min of sleep between none.
        let mut h = History::new();
        let mut t = NOW - 6 * 3600;
        let mut cap = 95.0f32;
        for (activity, minutes, rate, charging) in parts {
            for _ in 0..*minutes {
                let mut x = s(t, cap, *charging);
                x.ma = 1000;
                x.mv = 4000;
                h.push(x, if *activity == "-" { None } else { Some(activity) });
                t += 60;
                cap += if *charging { *rate / 60.0 } else { -*rate / 60.0 };
            }
        }
        (h, t)
    }

    #[test]
    fn each_system_gets_its_own_rate_and_watts() {
        let (h, now) = history(&[("-", 40, 6.0, false), ("ps2", 90, 18.0, false), ("gba", 60, 5.0, false), ("-", 10, 6.0, true)]);
        let d = draws(&h, now);
        let get = |t: Option<&str>| d.iter().find(|x| x.tag.as_deref() == t).unwrap_or_else(|| panic!("{t:?} in {d:?}"));
        assert!((get(Some("ps2")).per_hour - 18.0).abs() < 2.0, "{d:?}");
        assert!((get(Some("gba")).per_hour - 5.0).abs() < 1.5, "{d:?}");
        assert!((get(None).per_hour - 6.0).abs() < 2.0, "{d:?}");
        assert_eq!(get(Some("ps2")).minutes, 89);
        assert!((get(Some("ps2")).watts.unwrap() - 4.0).abs() < 0.01);
        // The one that ran longest first.
        assert_eq!(d[0].tag.as_deref(), Some("ps2"));
    }

    #[test]
    fn a_game_played_for_five_minutes_has_no_rate_yet() {
        let (h, now) = history(&[("-", 60, 6.0, false), ("n64", 5, 30.0, false)]);
        let d = draws(&h, now);
        assert!(d.iter().all(|x| x.tag.as_deref() != Some("n64")), "{d:?}");
        assert!(d.iter().any(|x| x.tag.is_none()));
    }

    #[test]
    fn charging_and_sleeping_do_not_count_as_a_system_using_power() {
        let (mut h, now) = history(&[("ps2", 30, 30.0, false)]);
        // Slept for 5 hours with the game paused, 30 points lost.
        for m in 0..30u64 {
            let mut x = s(now + 5 * 3600 + m * 60, 65.0 - m as f32 * 0.5, false);
            x.ma = 1000;
            h.push(x, Some("ps2"));
        }
        // Then plugged in during the game.
        for m in 0..30u64 {
            h.push(s(now + 6 * 3600 + m * 60, 50.0 + m as f32, true), Some("ps2"));
        }
        let end = now + 6 * 3600 + 30 * 60;
        let d = draws(&h, end);
        let ps2 = d.iter().find(|x| x.tag.as_deref() == Some("ps2")).unwrap();
        assert_eq!(ps2.minutes, 29 + 29, "{d:?}: only minutes on battery, none across the sleep");
        assert!(ps2.per_hour < 32.0, "{ps2:?}");
    }
}
