//! Battery page and the battery's sysfs reading: one `Sample` of capacity, charging state, voltage and
//! current; how it is shown (headline, graph, details); and the hooks the rest of the app uses.
//!
//! The history behind the page (48 hours, one sample a minute, kept over restarts) is
//! `battery_history`; what it means in time (time left, full in, the draw of a system) is
//! `battery_estimate`. The sign of `current_now` differs between fuel gauges (the Mini V2's reads
//! positive on battery and negative while charging), so the charging state comes from `status`, never
//! from the sign.
//!
//! What the Mini V2 exposes in `/sys/class/power_supply/battery` (checked on the device, 22 Oct 2026):
//! `capacity` (whole percent), `status`, `voltage_now` (µV), `current_now` (µA), `temp` (0.1 °C),
//! `charge_full_design` (3 850 000 µAh) and the design voltage limits. There is no `charge_now`,
//! `energy_now` or `power_now`: the percentage is the only measure of what is left, so the estimate
//! reads the slope of `capacity`; `current_now` x `voltage_now` is only the instantaneous draw
//! (noisy; shown, and averaged per system, but never used for the time left).

use std::path::{Path, PathBuf};

use crate::battery_estimate::{Draw, Estimate};
use crate::gfx::Canvas;
use crate::text::{Face, Text};

pub use crate::battery_history::{GAP, KEEP};

/// Below this a status of "Full" is not a full battery but charging that stopped early: the Mini V2's kernel reported
/// "Full" at 58 % on the charger (device check, 2026-10-26; pocket-ui sets no charge limit, see docs/STATUS.md).
/// Shown as plugged in, not charging.
pub const FULL_FROM: u8 = 95;

/// The battery's directory in sysfs; `POCKETUI_BATTERY` points somewhere else (tests, the virtual device).
pub fn dir() -> PathBuf {
    PathBuf::from(std::env::var("POCKETUI_BATTERY").unwrap_or_else(|_| "/sys/class/power_supply/battery".into()))
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sample {
    /// Unix seconds.
    pub t: u64,
    pub capacity: u8,
    pub charging: bool,
    pub full: bool,
    pub mv: u16,
    /// As the fuel gauge reports it (sign depends on the driver).
    pub ma: i32,
    /// Tenths of a degree Celsius.
    pub temp: Option<i16>,
    /// What was running when the sample was taken: 0 = nothing (the menus), n = the n-th name of the
    /// history's tag table (`battery_history::History::tag`), a system id such as "ps2".
    pub tag: u8,
}

/// Bytes of one stored sample.
pub const RECORD: usize = 16;

impl Sample {
    /// `u32 unix seconds | u8 capacity % | u8 flags (1 = charging, 2 = full) | u16 mV | i32 mA |
    /// i16 temperature in 0.1 °C (i16::MIN = unknown) | u8 tag | u8 reserved`, little endian. The first
    /// version of the battery ring had the same layout with the tag byte reserved, so its records read as they are.
    pub fn encode(&self) -> [u8; RECORD] {
        let mut r = [0u8; RECORD];
        r[0..4].copy_from_slice(&(self.t as u32).to_le_bytes());
        r[4] = self.capacity;
        r[5] = self.charging as u8 | (self.full as u8) << 1;
        r[6..8].copy_from_slice(&self.mv.to_le_bytes());
        r[8..12].copy_from_slice(&self.ma.to_le_bytes());
        r[12..14].copy_from_slice(&self.temp.unwrap_or(i16::MIN).to_le_bytes());
        r[14] = self.tag;
        r
    }

    pub fn decode(r: &[u8]) -> Option<Sample> {
        if r.len() < RECORD {
            return None;
        }
        let t = u32::from_le_bytes(r[0..4].try_into().ok()?) as u64;
        let temp = i16::from_le_bytes(r[12..14].try_into().ok()?);
        (t != 0 && r[4] <= 100).then(|| Sample {
            t,
            capacity: r[4],
            charging: r[5] & 1 != 0,
            full: r[5] & 2 != 0,
            mv: u16::from_le_bytes([r[6], r[7]]),
            ma: i32::from_le_bytes(r[8..12].try_into().unwrap()),
            temp: (temp != i16::MIN).then_some(temp),
            tag: r[14],
        })
    }

    /// Plugged in: charging, or full on the charger.
    pub fn plugged(&self) -> bool {
        self.charging || self.full
    }

    /// On the charger and not charging, well below full: the kernel's "Full" under [`FULL_FROM`].
    pub fn held(&self) -> bool {
        self.full && self.capacity < FULL_FROM
    }

    /// The draw at the moment of the sample in milliwatts, 0 when the gauge reports no current or voltage.
    pub fn mw(&self) -> u32 {
        (self.mv as u64 * self.ma.unsigned_abs() as u64 / 1000) as u32
    }
}

/// Reads the battery now; None without a battery (the virtual device without its mock).
pub fn read(dir: &Path, t: u64) -> Option<Sample> {
    let get = |name: &str| std::fs::read_to_string(dir.join(name)).ok().map(|s| s.trim().to_string());
    let num = |name: &str| get(name).and_then(|s| s.parse::<i64>().ok());
    let capacity = num("capacity")?.clamp(0, 100) as u8;
    let status = get("status").unwrap_or_default();
    Some(Sample {
        t,
        capacity,
        charging: status == "Charging",
        full: status == "Full",
        mv: num("voltage_now").map_or(0, |uv| (uv / 1000).clamp(0, u16::MAX as i64) as u16),
        ma: num("current_now").map_or(0, |ua| (ua / 1000).clamp(i32::MIN as i64, i32::MAX as i64) as i32),
        temp: num("temp").map(|t| t.clamp(i16::MIN as i64 + 1, i16::MAX as i64) as i16),
        tag: 0,
    })
}

/// One sample after a wake, on its own thread (`battery_history::sample_soon`): the caller doesn't wait for it.
pub fn sample_soon(data_dir: &Path) {
    crate::battery_history::sample_soon(data_dir);
}

/// Samples the battery every minute for as long as pocket-ui runs, game or not.
pub fn spawn_sampler(data_dir: PathBuf) {
    crate::battery_history::start(data_dir);
}

/// "about 3 h 20 min", rounded to 10 minutes (5 under an hour): the estimate is not more exact than that.
/// A day or more is only "more than a day".
pub fn about(secs: u64) -> String {
    let m = secs / 60;
    if m >= 24 * 60 {
        "more than a day".to_string()
    } else if m < 60 {
        format!("about {} min", (m / 5 * 5).max(5))
    } else {
        let m = (m + 5) / 10 * 10;
        match m % 60 {
            0 => format!("about {} h", m / 60),
            r => format!("about {} h {r:02} min", m / 60),
        }
    }
}

/// "About 3 h 20 min left", "More than a day left".
fn left_phrase(secs: u64) -> String {
    let a = about(secs);
    let mut chars = a.chars();
    let first = chars.next().map(|c| c.to_uppercase().collect::<String>()).unwrap_or_default();
    format!("{first}{} left", chars.as_str())
}

/// The headline alone, for a battery that is there: "About 3 h 20 min left", "Full in about 40 min", "Charging",
/// "Full", "Plugged in", "Not enough data yet" (the Battery page, the power menu).
pub fn headline(est: Option<&Estimate>) -> String {
    match est {
        Some(Estimate::Left { secs, .. }) => left_phrase(*secs),
        Some(Estimate::Charging { full_in: Some(f), .. }) => format!("Full in {}", about(*f)),
        Some(Estimate::Charging { full_in: None, .. }) => "Charging".into(),
        Some(Estimate::Full) => "Full".into(),
        Some(Estimate::PluggedIn) => "Plugged in".into(),
        Some(Estimate::NotEnough { .. }) | None => "Not enough data yet".into(),
    }
}

/// The headline and the line under it, for the page and for anyone else who wants the estimate.
pub fn describe(now: Option<&Sample>, est: Option<&Estimate>) -> (String, String) {
    let Some(s) = now else {
        return ("No battery found".into(), "This device doesn't report a battery.".into());
    };
    match est {
        Some(Estimate::Left { per_hour, secs, over }) => (
            left_phrase(*secs),
            if *per_hour < 1.0 {
                format!("At the current rate: under 1% an hour, measured over the last {} min.", over / 60)
            } else {
                format!("At the current rate: {per_hour:.0}% an hour, measured over the last {} min.", over / 60)
            },
        ),
        Some(Estimate::Charging { full_in: Some(f), per_hour }) => (
            format!("Full in {}", about(*f)),
            format!("On the charger, about {per_hour:.0}% an hour at the moment."),
        ),
        Some(Estimate::Charging { full_in: None, .. }) => ("Charging".into(), "Time until full needs a few more minutes on the charger.".into()),
        Some(Estimate::Full) => ("Full".into(), "On the charger.".into()),
        Some(Estimate::PluggedIn) => ("Plugged in".into(), "On the charger, not charging at the moment.".into()),
        Some(Estimate::NotEnough { have }) => (
            "Not enough data yet".into(),
            if s.plugged() {
                "An estimate needs time on battery.".into()
            } else {
                format!(
                    "An estimate needs {} min on battery without sleep; {} min so far.",
                    crate::battery_estimate::MIN_SPAN / 60,
                    have / 60
                )
            },
        ),
        None => ("Not enough data yet".into(), "Samples are taken once a minute.".into()),
    }
}

/// "PS2 uses about 9 % an hour, 1.6 W · about 5 h 30 min left at that rate"; `name` is what the person calls
/// the system, `capacity` the charge now (without it, no time left).
pub fn draw_line(name: &str, d: &Draw, capacity: Option<u8>) -> String {
    let verb = if d.tag.is_none() { "use" } else { "uses" };
    let watts = d.watts.map_or(String::new(), |w| format!(", {w:.1} W"));
    let left = capacity.filter(|_| d.per_hour >= 1.0).map_or(String::new(), |c| {
        format!("  ·  {} left at that rate", about((c as f32 / d.per_hour * 3600.0) as u64))
    });
    format!("{name} {verb} about {:.0}% an hour{watts}{left}", d.per_hour)
}

/// Hours the graph can show; L1/R1 step through them.
pub const RANGES: [u64; 5] = [3, 6, 12, 24, 48];
/// The range the page opens with when there is a day of history: the last 24 hours.
pub const DEFAULT_RANGE: usize = 3;

/// The range to open with for a history that spans `span` seconds: the smallest that shows all of it, at most a day
/// (twelve minutes of samples are a sliver on 24 hours, and the first hours of a new install look empty).
pub fn range_for(span: u64) -> usize {
    RANGES.iter().position(|h| h * 3600 >= span).unwrap_or(RANGES.len() - 1).min(DEFAULT_RANGE)
}

const LINE: u32 = 0xe9e6de;
const CHARGE: u32 = 0x6fbf8a;
const GRID: u32 = 0x2c2c30;
const LABEL: u32 = 0x8a877f;

/// The capacity graph of the last `hours` into the box (x, y, w, h): a line with a soft fill under it,
/// green where the device was charging, and nothing across a gap (asleep or switched off): the line
/// stops, a faint band marks the time with no samples.
pub fn paint_graph(c: &mut Canvas, text: &mut Text, samples: &[Sample], now: u64, hours: u64, (x, y, w, h): (i32, i32, i32, i32)) {
    // Dark: the constants above; light: ink on paper.
    let p = crate::palette::pal();
    let (line, charge, grid, label_c, hatch) = (p.pick(LINE, 0x2b2925), p.pick(CHARGE, 0x2f7a4b), p.pick(GRID, 0xd6d0c4), p.pick(LABEL, 0x625e56), p.pick(0xffffff, 0x000000));
    let span = hours * 3600;
    let t0 = now.saturating_sub(span);
    let px = |t: u64| x + ((t.saturating_sub(t0)) as f64 / span as f64 * w as f64) as i32;
    let py = |cap: f32| y + h - (h as f32 * cap / 100.0) as i32;
    let visible: Vec<&Sample> = samples.iter().filter(|s| s.t >= t0).collect();
    // Time without samples inside the span (after the first one): a hatch of faint lines, so a gap doesn't
    // look like a fault and can't be taken for the soft fill under the line.
    let band = |c: &mut Canvas, from: u64, to: u64| {
        let (xa, xb) = (px(from.max(t0)), px(to.min(now)).min(x + w));
        for gx in xa.max(x)..xb {
            if (gx - x) % 12 == 0 {
                for fy in y..y + h {
                    c.blend(gx, fy, hatch, 20);
                }
            }
        }
    };
    for pair in visible.windows(2) {
        if pair[1].t - pair[0].t > GAP {
            band(c, pair[0].t, pair[1].t);
        }
    }
    if let Some(last) = visible.last().filter(|l| now.saturating_sub(l.t) > GAP) {
        band(c, last.t, now);
    }
    // Grid: 0, 50 and 100 %, and a tick per hour (every 3 h on the long ranges, every 6 h on two days).
    for (pct, label) in [(100, "100%"), (50, "50%"), (0, "0%")] {
        let gy = y + h - h * pct / 100;
        c.rect(x, gy, w, 2, grid);
        text.draw_right(c, Face::Body, 24, x - 14, gy + 9, label_c, label);
    }
    let step = match hours {
        0..=6 => 1,
        7..=24 => 3,
        _ => 6,
    };
    for k in (0..=hours).step_by(step as usize) {
        let gx = x + w - (w as u64 * k / hours) as i32;
        c.rect(gx, y, 2, h, grid);
        let label = if k == 0 { "now".to_string() } else { format!("-{k} h") };
        let lw = text.width(Face::Body, 24, &label);
        text.draw(c, Face::Body, 24, (gx - lw / 2).clamp(x - 20, x + w - lw), y + h + 36, label_c, &label);
    }
    // On the long ranges several samples share a column; each column is drawn once, or the fill adds up.
    let mut done = i32::MIN;
    for pair in visible.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        if b.t - a.t > GAP {
            continue;
        }
        let color = if b.plugged() { charge } else { line };
        let (xa, xb) = (px(a.t), px(b.t).max(px(a.t) + 1));
        for gx in xa.max(done + 1)..xb {
            done = gx;
            let f = (gx - xa) as f32 / (xb - xa) as f32;
            let gy = py(a.capacity as f32 + f * (b.capacity as f32 - a.capacity as f32));
            // Soft fill down to the axis, then the line itself, 4 px thick.
            for fy in gy..y + h {
                c.blend(gx, fy, color, 28);
            }
            c.rect(gx, gy - 2, 1, 4, color);
        }
    }
    // The newest sample is a dot, if it is recent.
    if let Some(s) = visible.last().filter(|l| now.saturating_sub(l.t) <= GAP) {
        c.disc(px(s.t).min(x + w - 1), py(s.capacity as f32), 6, if s.plugged() { charge } else { line });
    }
}

/// "3.98 V · 1050 mA · 1.6 W · 31 °C" from the newest sample, leaving out what the device doesn't report.
pub fn details(s: &Sample) -> String {
    let mut parts = Vec::new();
    if s.mv > 0 {
        parts.push(format!("{:.2} V", s.mv as f32 / 1000.0));
    }
    if s.ma != 0 {
        parts.push(format!("{} mA", s.ma.unsigned_abs()));
        if s.mv > 0 {
            parts.push(format!("{:.1} W", s.mw() as f32 / 1000.0));
        }
    }
    if let Some(t) = s.temp {
        parts.push(format!("{:.0} °C", t as f32 / 10.0));
    }
    parts.join("  ·  ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;

    fn s(t: u64, capacity: u8, charging: bool) -> Sample {
        Sample {
            t,
            capacity,
            charging,
            full: false,
            mv: 3900,
            ma: 500,
            temp: Some(310),
            tag: 0,
        }
    }

    #[test]
    fn records_round_trip() {
        let a = Sample {
            t: 1_790_000_000,
            capacity: 61,
            charging: true,
            full: false,
            mv: 4051,
            ma: -1088,
            temp: None,
            tag: 3,
        };
        assert_eq!(Sample::decode(&a.encode()), Some(a));
        assert_eq!(Sample::decode(&[0u8; RECORD]), None);
        assert_eq!(Sample::decode(&[1u8; 5]), None, "a short record is no record");
    }

    #[test]
    fn reads_sysfs() {
        let dir = TestDir::new("bat-sysfs");
        for (f, v) in [
            ("capacity", "61\n"),
            ("status", "Charging\n"),
            ("voltage_now", "4051259\n"),
            ("current_now", "-1088378\n"),
        ] {
            std::fs::write(dir.join(f), v).unwrap();
        }
        let r = read(&dir, 5).unwrap();
        assert_eq!((r.capacity, r.charging, r.mv, r.ma, r.temp), (61, true, 4051, -1088, None));
        assert_eq!(details(&r), "4.05 V  ·  1088 mA  ·  4.4 W");
        let _ = std::fs::remove_dir_all(&dir);
        assert!(read(&dir, 5).is_none());
    }

    #[test]
    fn the_draw_is_voltage_times_current_whatever_the_sign() {
        assert_eq!(s(1, 50, false).mw(), 3900 * 500 / 1000);
        let mut c = s(1, 50, true);
        c.ma = -500;
        assert_eq!(c.mw(), 1950);
        c.ma = 0;
        assert_eq!(c.mw(), 0);
    }

    #[test]
    fn the_graph_opens_on_the_smallest_range_that_shows_the_history_up_to_a_day() {
        let hours = |h: f32| (h * 3600.0) as u64;
        let got: Vec<u64> = [0.0, 0.2, 3.0, 3.1, 5.0, 10.0, 20.0, 47.0].iter().map(|h| RANGES[range_for(hours(*h))]).collect();
        assert_eq!(got, vec![3, 3, 3, 6, 6, 12, 24, 24]);
    }

    #[test]
    fn about_rounds_honestly() {
        assert_eq!(about(60), "about 5 min");
        assert_eq!(about(23 * 60), "about 20 min");
        assert_eq!(about(3600), "about 1 h");
        assert_eq!(about(3 * 3600 + 17 * 60), "about 3 h 20 min");
        assert_eq!(about(30 * 3600), "more than a day");
    }

    #[test]
    fn the_headline_says_the_rate_it_is_based_on() {
        let now = s(100, 50, false);
        let (h, sub) = describe(
            Some(&now),
            Some(&Estimate::Left {
                per_hour: 15.0,
                secs: 3 * 3600 + 20 * 60,
                over: 2400,
            }),
        );
        assert_eq!(h, "About 3 h 20 min left");
        assert_eq!(sub, "At the current rate: 15% an hour, measured over the last 40 min.");
        let (h, _) = describe(
            Some(&now),
            Some(&Estimate::Left {
                per_hour: 1.5,
                secs: 40 * 3600,
                over: 2400,
            }),
        );
        assert_eq!(h, "More than a day left");
        let (h, sub) = describe(
            Some(&s(100, 50, true)),
            Some(&Estimate::Charging {
                full_in: Some(45 * 60),
                per_hour: 66.0,
            }),
        );
        assert_eq!(h, "Full in about 45 min");
        assert!(sub.starts_with("On the charger"), "{sub}");
        let (h, sub) = describe(Some(&now), Some(&Estimate::NotEnough { have: 600 }));
        assert_eq!(h, "Not enough data yet");
        assert!(sub.contains("20 min") && sub.contains("10 min so far"), "{sub}");
        assert_eq!(describe(None, None).0, "No battery found");
    }

    /// The Mini V2 on the charger, charging stopped early: the kernel says "Full" at 58 %. Every place that shows the
    /// battery says plugged in, not full, and the low-battery warning still knows a charger is there.
    #[test]
    fn a_full_status_at_58_reads_as_plugged_in_everywhere() {
        let dir = TestDir::new("battery-held");
        std::fs::write(dir.join("capacity"), "58\n").unwrap();
        std::fs::write(dir.join("status"), "Full\n").unwrap();
        let now = read(&dir, 1_000_000).unwrap();
        assert!(now.held() && now.plugged() && !now.charging, "{now:?}");
        let est = crate::battery_estimate::estimate(&[now], now.t);
        assert_eq!(est, Some(Estimate::PluggedIn));
        assert_eq!(headline(est.as_ref()), "Plugged in", "the Battery page and the power menu");
        assert_eq!(describe(Some(&now), est.as_ref()), ("Plugged in".to_string(), "On the charger, not charging at the moment.".to_string()));
        assert_eq!(crate::power_menu::battery_line(Some(58), true, est.as_ref()).time, "Plugged in");
        let qs = crate::system_ctl::Battery { percent: 58, charge: crate::system_ctl::parse_charge("Full") };
        assert_eq!(qs.label(), "58%, plugged in");
        let corner = crate::status_corner::parse_battery(Some("58"), Some("Full")).unwrap();
        assert_eq!(corner.label(), "58%", "the status corner: the level, no charging mark");
        assert!(crate::low_battery::read(&dir).unwrap().external_power, "no low-battery warning on the charger");
        // Really full stays "Full".
        std::fs::write(dir.join("capacity"), "100\n").unwrap();
        let full = read(&dir, 1_000_000).unwrap();
        assert!(!full.held());
        assert_eq!(headline(crate::battery_estimate::estimate(&[full], full.t).as_ref()), "Full");
        assert_eq!(crate::system_ctl::Battery { percent: 100, charge: crate::system_ctl::Charge::Full }.label(), "Full");
    }

    #[test]
    fn a_draw_reads_as_a_sentence() {
        let d = Draw {
            tag: Some("ps2".into()),
            per_hour: 9.2,
            watts: None,
            minutes: 90,
            last: 0,
        };
        assert_eq!(draw_line("PS2", &d, None), "PS2 uses about 9% an hour");
        assert_eq!(
            draw_line("PS2", &d, Some(50)),
            "PS2 uses about 9% an hour  ·  about 5 h 30 min left at that rate"
        );
        let d = Draw {
            tag: None,
            per_hour: 6.0,
            watts: Some(1.64),
            minutes: 90,
            last: 0,
        };
        assert_eq!(draw_line("Menus", &d, None), "Menus use about 6% an hour, 1.6 W");
    }

    #[test]
    fn graph_paints_without_panicking() {
        let mut buf = vec![0u32; 400 * 300];
        let mut c = Canvas::new(&mut buf, 400, 300);
        let mut text = Text::new();
        let now = 100_000;
        let v: Vec<Sample> = (0..200).map(|m| s(now - 12_000 + m * 60, (90 - m / 4) as u8, m > 150)).collect();
        paint_graph(&mut c, &mut text, &v, now, 6, (40, 20, 340, 220));
        paint_graph(&mut c, &mut text, &[], now, 3, (40, 20, 340, 220));
        assert!(buf.iter().any(|p| *p == LINE));
    }

    #[test]
    fn the_graph_leaves_a_gap_empty_and_draws_no_line_across_it() {
        let mut buf = vec![0u32; 1000 * 300];
        let mut c = Canvas::new(&mut buf, 1000, 300);
        let mut text = Text::new();
        let now = 200_000;
        // 3 h of samples, 6 h of nothing, 3 h of samples, over a 12 h graph from x = 0 (60 px an hour... 80 px).
        let mut v: Vec<Sample> = (0..180).map(|m| s(now - 12 * 3600 + m * 60, 90, false)).collect();
        v.extend((0..180).map(|m| s(now - 3 * 3600 + m * 60, 40, false)));
        paint_graph(&mut c, &mut text, &v, now, 12, (0, 20, 960, 240));
        let col = |gx: usize| (0..300).any(|gy| buf[gy * 1000 + gx] == LINE);
        // The middle of the 6 h hole (hour 6 of 12) has no line, both ends have.
        assert!(!col(480), "no line across the gap");
        assert!(col(60) && col(900));
        // ... and the hole is marked: a hatch of faint lines, not plain black.
        assert!(buf[100 * 1000 + 480] != 0, "the gap is hatched");
        assert!(buf[100 * 1000 + 485] == 0, "between the hatch lines it is empty");
    }
}
