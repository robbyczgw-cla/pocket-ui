//! The heat guard of the TrimUI Brick's own card (`sleep::Sleep::spawn_heat_guard`): the CPU's thermal zone read every [`EVERY`]; at or
//! over the limit for the readings it takes, the game is saved and the Brick switched off, as the low-battery guard does. On the real
//! Brick (A133P, 03.11.) `cpu_thermal_zone` has trips passive 58 and 63 °C and critical 90 °C. Dark the limit is 63 °C for over a minute
//! (a Brick in a bag must not get warm; it got very hot on 03.11.), in play 80 °C for half a minute, both under the kernel's emergency
//! stop (which saves nothing).

use std::path::{Path, PathBuf};
use std::time::Duration;

pub const EVERY: Duration = Duration::from_secs(10);
/// Readings in a row: in play three (20-30 s, a short peak such as a game loading is no reason); dark seven (over a minute).
pub const CONFIRM_PLAY: u32 = 3;
pub const CONFIRM_DARK: u32 = 7;
/// In play, where the zone says nothing better.
const PLAY_LIMIT_C: u32 = 80;
/// Dark, without a passive trip.
const DARK_DEFAULT_C: u32 = 65;

/// The screen is dark (`sleep` sets it): the lower limit applies, a Brick in a bag must not get warm at all.
static DARK: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn set_dark(dark: bool) {
    DARK.store(dark, std::sync::atomic::Ordering::SeqCst);
}

pub fn dark() -> bool {
    DARK.load(std::sync::atomic::Ordering::SeqCst)
}

/// The CPU's zone below `root`: the one whose `type` names the CPU (`cpu_thermal_zone` on the Brick), else the first zone.
pub fn cpu_zone(root: &Path) -> Option<PathBuf> {
    let dir = root.join("sys/class/thermal");
    let mut zones: Vec<PathBuf> = std::fs::read_dir(&dir).ok()?.flatten().map(|e| e.path()).filter(|p| p.file_name().is_some_and(|n| n.to_string_lossy().starts_with("thermal_zone"))).collect();
    zones.sort();
    let kind = |p: &Path| std::fs::read_to_string(p.join("type")).unwrap_or_default().to_ascii_lowercase();
    zones.iter().find(|p| kind(p).contains("cpu")).or(zones.first()).cloned()
}

/// The zone's temperature in whole °C (the file says millidegrees).
pub fn read_c(zone: &Path) -> Option<u32> {
    let m: i64 = std::fs::read_to_string(zone.join("temp")).ok()?.trim().parse().ok()?;
    (m > 0).then(|| (m / 1000) as u32)
}

fn trips(zone: &Path, kind: &str) -> Vec<u32> {
    (0..10)
        .filter_map(|i| {
            let k = std::fs::read_to_string(zone.join(format!("trip_point_{i}_type"))).ok()?;
            (k.trim() == kind).then_some(())?;
            std::fs::read_to_string(zone.join(format!("trip_point_{i}_temp"))).ok()?.trim().parse::<u32>().ok().map(|m| m / 1000)
        })
        .collect()
}

/// The switch-off temperatures (dark, in play): dark the zone's highest passive trip (where the kernel starts to slow the CPU, 63 °C
/// on the Brick: a dark Brick should never get there), in play 80 °C, both kept under the critical trip less 5.
pub fn limits(zone: &Path) -> (u32, u32) {
    let ceiling = trips(zone, "critical").into_iter().min().map_or(90, |c| c.saturating_sub(5));
    let dark = trips(zone, "passive").into_iter().max().unwrap_or(DARK_DEFAULT_C).min(ceiling);
    (dark, PLAY_LIMIT_C.min(ceiling))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;

    fn zone(root: &Path, n: u32, kind: &str, temp: &str, trips: &[(&str, &str)]) {
        let z = root.join(format!("sys/class/thermal/thermal_zone{n}"));
        std::fs::create_dir_all(&z).unwrap();
        std::fs::write(z.join("type"), format!("{kind}\n")).unwrap();
        std::fs::write(z.join("temp"), format!("{temp}\n")).unwrap();
        for (i, (t, k)) in trips.iter().enumerate() {
            std::fs::write(z.join(format!("trip_point_{i}_temp")), format!("{t}\n")).unwrap();
            std::fs::write(z.join(format!("trip_point_{i}_type")), format!("{k}\n")).unwrap();
        }
    }

    #[test]
    fn the_bricks_cpu_zone_and_its_limit_under_the_critical_trip() {
        // As read on the real Brick (03.11.): cpu, gpu, ddr and the battery's zone; the CPU's trips 58/63 passive, 90 critical.
        let d = TestDir::new("heat-brick");
        zone(&d, 0, "cpu_thermal_zone", "48602", &[("58000", "passive"), ("63000", "passive"), ("90000", "critical")]);
        zone(&d, 1, "gpu_thermal_zone", "50009", &[]);
        zone(&d, 3, "axp2202-battery", "40700", &[]);
        let z = cpu_zone(&d).unwrap();
        assert!(z.ends_with("thermal_zone0"));
        assert_eq!(read_c(&z), Some(48));
        assert_eq!(limits(&z), (63, 80), "dark at the higher passive trip, in play 80");
    }

    #[test]
    fn without_a_critical_trip_or_a_cpu_zone() {
        let d = TestDir::new("heat-plain");
        zone(&d, 0, "soc", "71000", &[("70000", "passive")]);
        let z = cpu_zone(&d).unwrap();
        assert_eq!(limits(&z), (70, 80), "no critical trip: the passive one dark, 80 in play");
        assert_eq!(read_c(&z), Some(71));
        assert_eq!(cpu_zone(&TestDir::new("heat-none")), None, "no thermal zones: no guard");
    }
}
