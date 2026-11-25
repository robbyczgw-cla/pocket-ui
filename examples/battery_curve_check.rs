//! How well the time left matches what a battery really did: today's straight line through the percentages against the
//! Pocket's own discharge curve (`battery_curve`), at 60, 40 and 20 % of each discharge in a history file.
//!
//! `cargo run --example battery_curve_check -- LEARN.bin[,LEARN2.bin...] TEST.bin`
//!
//! The curve is learned from the LEARN files (`battery-history.bin` copied off a device), then each stretch of TEST on
//! battery that falls by 30 points or more is walked: when it first reads 60, 40 and 20 %, both estimates are made from
//! the samples up to then, as the Battery page would, and set against the time it really ran on until 2 % (the
//! low-battery switch-off), or until the stretch ended when it never got there (marked so). The pause menu's time left
//! for a game is checked the same way, with the rate a game played since the stretch began would have in the ledger.

use pocket_ui::battery::Sample;
use pocket_ui::battery_curve::Curve;
use pocket_ui::battery_estimate::{continuous, estimate_with, Estimate};
use pocket_ui::battery_games::{left_with, pair, Basis, Left, Rate};
use pocket_ui::battery_history::History;

fn hm(secs: Option<u64>) -> String {
    secs.map_or("–".into(), |s| format!("{} h {:02} min", s / 3600, s % 3600 / 60))
}

fn left(samples: &[Sample], curve: Option<&Curve>) -> Option<u64> {
    let last = samples.last()?;
    match estimate_with(samples, last.t, curve) {
        Some(Estimate::Left { secs, .. }) => Some(secs),
        _ => None,
    }
}

/// The pause menu's time left for a game played from the start of the stretch `run` up to its last sample: the game's
/// rate is the ledger's (points, and usable charge with the curve, over seconds of all its pairs).
fn game_left(run: &[Sample], curve: Option<&Curve>) -> Option<u64> {
    // As the sampler feeds the ledger: each pair's drop in points, and in usable charge where the curve knows it.
    let usable = |x: &Sample| curve.and_then(|c| c.usable(x.capacity)).map_or(x.capacity as f64, f64::from);
    let (secs, drop, used) = run
        .windows(2)
        .filter_map(|w| pair(&w[0], &w[1]).map(|(s, d)| (s, d, usable(&w[0]) - usable(&w[1]))))
        .fold((0u64, 0i32, 0.0), |(s, d, u), (a, b, c)| (s + a, d + b, u + c));
    let hours = secs.max(1) as f64 / 3600.0;
    let rate = Rate { per_hour: drop as f64 / hours, used_per_hour: used / hours, basis: Basis::Game, secs: secs as f64 };
    let last = run.last()?;
    match left_with(Some(rate), Some(last), last.t, curve) {
        Some(Left::Time { secs, .. }) => Some(secs),
        _ => None,
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [learn, test] = args.as_slice() else {
        eprintln!("usage: battery_curve_check LEARN.bin[,LEARN2.bin...] TEST.bin");
        std::process::exit(2);
    };
    let mut curve = Curve::default();
    for file in learn.split(',') {
        curve.learn(History::load(std::path::Path::new(file)).samples());
    }
    println!("curve: {}", if curve.usable(50).is_some() { "learned" } else { "too little to use" });
    let floor = pocket_ui::low_battery::CRITICAL_AT;
    let h = History::load(std::path::Path::new(test));
    let s = h.samples();
    let mut i = 0;
    while i < s.len() {
        if s[i].plugged() {
            i += 1;
            continue;
        }
        let mut j = i;
        while j + 1 < s.len() && continuous(&s[j], &s[j + 1]) && !s[j + 1].plugged() {
            j += 1;
        }
        if s[i].capacity.saturating_sub(s[j].capacity) >= 30 {
            let run = &s[i..=j];
            let end = run.iter().find(|x| x.capacity <= floor);
            println!(
                "\n{} % -> {} % over {}{}",
                run[0].capacity,
                run[run.len() - 1].capacity,
                hm(Some(run[run.len() - 1].t - run[0].t)),
                if end.is_none() { " (never reached 2 %: actual = to the stretch's end)" } else { "" }
            );
            println!("| at | really ran on | page: straight line (today) | page: own curve | game: today | game: own curve |\n|---|---|---|---|---|---|");
            let end_t = end.unwrap_or(&run[run.len() - 1]).t;
            for at in [60u8, 40, 20] {
                let Some(k) = (i..=j).find(|&k| s[k].capacity <= at) else { continue };
                // The game starts under 100 %: hours at 100 on a charger that said "Not charging" were "on battery" before
                // `battery::read` knew better, and no game's ledger holds them now.
                let g = (i..=k).find(|&g| s[g].capacity < 100).unwrap_or(i);
                println!(
                    "| {at} % | {} | {} | {} | {} | {} |",
                    hm(Some(end_t - s[k].t)),
                    hm(left(&s[..=k], None)),
                    hm(left(&s[..=k], Some(&curve))),
                    hm(game_left(&s[g..=k], None)),
                    hm(game_left(&s[g..=k], Some(&curve)))
                );
            }
        }
        i = j + 1;
    }
}
