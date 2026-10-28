//! Makes a battery history and a fake `/sys/class/power_supply/battery` for looking at the Battery page
//! (`POCKETUI_BATTERY=<sysfs dir>`): the virtual device has no battery, and a real one takes hours to fill.
//!
//! ```text
//! cargo run --release --example battery_fake -- <empty|partial|day|charging> <data dir> <sysfs dir>
//! ```
//! The times are relative to now, so run it just before looking.

use pocket_ui::battery::Sample;
use pocket_ui::battery_history::{History, FILE};

/// One stretch of the timeline, oldest first.
enum Part {
    /// Hours of play (or menus) on battery at `rate` points an hour, with what ran.
    Play(f32, f32, Option<&'static str>),
    /// Hours of charging at `rate` points an hour (it stops at 100 and stays "full").
    Charge(f32, f32),
    /// Hours without samples (asleep, off), losing `drop` points.
    Gap(f32, f32),
}

fn sample(t: u64, cap: f32, charging: bool, full: bool, ma: i32, noise: &mut u32) -> Sample {
    *noise = noise.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
    let jitter = (*noise >> 24) as i32 % 40 - 20;
    Sample {
        t,
        capacity: cap.round().clamp(0.0, 100.0) as u8,
        charging,
        full,
        mv: if charging { 4180 } else { (3600.0 + cap * 5.2) as u16 },
        ma: if charging { -(ma + jitter) } else { ma + jitter },
        temp: Some(if charging { 335 } else { 290 + (cap as i16 % 7) * 3 }),
        tag: 0,
    }
}

fn build(parts: &[Part], start_cap: f32, now: u64) -> (History, f32, bool) {
    let total: f32 = parts
        .iter()
        .map(|p| match p {
            Part::Play(h, _, _) | Part::Charge(h, _) | Part::Gap(h, _) => *h,
        })
        .sum();
    let mut t = now - (total * 3600.0) as u64;
    t = t / 60 * 60;
    let (mut h, mut cap, mut noise, mut plugged) = (History::new(), start_cap, 7u32, false);
    for p in parts {
        match *p {
            Part::Play(hours, rate, tag) => {
                plugged = false;
                for _ in 0..(hours * 60.0) as u32 {
                    h.push(sample(t, cap, false, false, (380.0 + rate * 35.0) as i32, &mut noise), tag);
                    t += 60;
                    cap -= rate / 60.0;
                }
            }
            Part::Charge(hours, rate) => {
                plugged = true;
                for _ in 0..(hours * 60.0) as u32 {
                    let full = cap >= 99.5;
                    h.push(sample(t, cap, !full, full, if full { 0 } else { 1900 }, &mut noise), None);
                    t += 60;
                    cap = (cap + rate / 60.0).min(100.0);
                }
            }
            Part::Gap(hours, drop) => {
                t += (hours * 3600.0) as u64;
                cap -= drop;
            }
        }
    }
    (h, cap, plugged)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [scenario, data, sysfs] = args.as_slice() else {
        eprintln!("usage: battery_fake <empty|partial|day|charging> <data dir> <sysfs dir>");
        std::process::exit(2);
    };
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    use Part::*;
    let (history, cap, charging, full) = match scenario.as_str() {
        "empty" => (History::new(), 74.0, false, false),
        // Just started: 12 minutes of samples.
        "partial" => {
            let (h, cap, _) = build(&[Play(0.2, 14.0, None)], 74.0, now);
            (h, cap, false, false)
        }
        // Two days: play, sleep, charging, games of different weight, the last hour in the menus.
        "day" => {
            let parts = [
                Play(2.5, 8.0, Some("gba")),
                Gap(8.0, 3.0),
                Charge(2.0, 26.0),
                Gap(2.0, 0.0),
                Play(3.0, 18.0, Some("ps2")),
                Gap(8.0, 5.0),
                Play(0.6, 6.0, None),
                Charge(3.2, 24.0),
                Gap(7.0, 0.0),
                Play(2.4, 15.0, Some("n64")),
                Play(0.5, 6.0, None),
                Gap(3.0, 2.0),
                Play(1.6, 18.0, Some("ps2")),
                Play(0.9, 7.0, None),
            ];
            let (h, cap, _) = build(&parts, 97.0, now);
            (h, cap, false, false)
        }
        // Charging for the last 40 minutes after a game.
        "charging" => {
            let parts = [Play(2.0, 16.0, Some("ps2")), Play(0.4, 6.0, None), Charge(0.67, 52.0)];
            let (h, cap, _) = build(&parts, 60.0, now);
            (h, cap, true, false)
        }
        other => {
            eprintln!("unknown scenario {other}");
            std::process::exit(2);
        }
    };
    let data = std::path::Path::new(data);
    std::fs::create_dir_all(data).unwrap();
    let _ = std::fs::remove_file(data.join("battery.ring"));
    if history.samples().is_empty() {
        let _ = std::fs::remove_file(data.join(FILE));
        let _ = std::fs::remove_file(data.join(format!("{FILE}.bak")));
    } else {
        history.save(&data.join(FILE)).unwrap();
    }
    let sys = std::path::Path::new(sysfs);
    std::fs::create_dir_all(sys).unwrap();
    let cap = cap.round().clamp(0.0, 100.0) as u8;
    let (status, ma) = if charging {
        ("Charging", -1850)
    } else if full {
        ("Full", 0)
    } else {
        ("Discharging", 410)
    };
    for (name, value) in [
        ("capacity", cap.to_string()),
        ("status", status.to_string()),
        ("voltage_now", (if charging { 4_180_000 } else { 3_600_000 + cap as i64 * 5_200 }).to_string()),
        ("current_now", (ma as i64 * 1000).to_string()),
        ("temp", "296".to_string()),
    ] {
        std::fs::write(sys.join(name), format!("{value}\n")).unwrap();
    }
    println!("{scenario}: {} samples, now {cap} % {status}", history.samples().len());
}
