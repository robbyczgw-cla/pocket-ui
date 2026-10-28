//! Over the game → FPS + sensors (`over_game`): the device's state added to the in-game line, "60 FPS   CPU 23%   62 °C   Fan 45%   4.8 W".
//!
//! - [`Sensors::read`] reads what `perf_fan::Readout` reads, the same way, but only the values the line needs:
//!   the hottest thermal zone, the fan's PWM duty or rpm, the battery's current x voltage. A handful of small sysfs
//!   files.
//! - [`Watcher`] does that on a thread of its own, twice a second (the FPS counter's cadence), for as long as the
//!   FPS + sensors level is on, and keeps the last reading and its line. The emulation thread never opens a file or
//!   formats anything for it: `overlay::Hud` only asks [`Watcher::text`], a copy of the line. Switching the level off
//!   drops the watcher and the thread ends.
//! - A value the device doesn't have (no fan, no battery gauge, a gauge without current) is left out of the line,
//!   never shown as 0 or "?".
//! - The words are in the interface's language (Settings → Language, which pocket-core reads at its start): "Lüfter 45%",
//!   "lädt" in German. The painter translates a notice whole (`i18n`), but this line's numbers change twice a second,
//!   so [`Sensors::text_in`] looks each part with words up in the table itself (`assets/i18n/de.tsv`, "Fan {n}%").
//!   Numbers and units stay as they are; "FPS" and "CPU" before it are the same in German.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{Thread, ThreadId};
use std::time::Duration;

use crate::battery;
use crate::i18n::Lang;
use crate::logs::read_i64;
use crate::perf_fan::{fan_dir, hottest_zone};

/// How often the thread reads: the FPS counter's half-second.
pub const EVERY: Duration = Duration::from_millis(500);

/// The battery's part of the line.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Power {
    /// On the charger and charging: the draw means nothing then.
    Charging,
    /// What the device draws from the battery, milliwatts (more than 0).
    Draw(u32),
}

/// One reading; every part is None where the device has no such sensor.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Sensors {
    /// The hottest thermal zone, millidegrees (more than 0).
    pub hottest: Option<i64>,
    pub fan_rpm: Option<i64>,
    /// PWM duty, 0 to 255.
    pub fan_pwm: Option<i64>,
    pub power: Option<Power>,
}

impl Sensors {
    /// Reads sysfs below `root` (`/` on the device) and the battery in `battery_dir`.
    pub fn read(root: &Path, battery_dir: &Path) -> Sensors {
        let fan = fan_dir(root);
        Sensors {
            hottest: hottest_zone(root).filter(|t| *t > 0),
            fan_rpm: fan.as_ref().and_then(|d| read_i64(&d.join("fan1_input"))).filter(|v| *v >= 0),
            fan_pwm: fan.as_ref().and_then(|d| read_i64(&d.join("pwm1"))).filter(|v| *v >= 0),
            power: battery::read(battery_dir, 0).and_then(|b| {
                if b.charging {
                    Some(Power::Charging)
                } else if b.full {
                    None
                } else {
                    Some(b.mw()).filter(|mw| *mw > 0).map(Power::Draw)
                }
            }),
        }
    }

    /// "62 °C".
    pub fn temperature(&self) -> Option<String> {
        self.hottest.map(|t| format!("{} °C", (t + 500).div_euclid(1000)))
    }

    /// "Fan 45%", "Fan off", "Fan 4400 rpm" (no PWM file). Off when the fan reports 0 rpm, or has no rpm and 0 PWM.
    pub fn fan(&self) -> Option<String> {
        let pct = self.fan_pwm.map(|p| (p.clamp(0, 255) * 100 + 127) / 255);
        match (self.fan_rpm, pct) {
            (Some(0), _) | (None, Some(0)) => Some("Fan off".to_string()),
            (_, Some(p)) => Some(format!("Fan {p}%")),
            (Some(rpm), None) => Some(format!("Fan {rpm} rpm")),
            (None, None) => None,
        }
    }

    /// "4.8 W" (to a tenth), "charging".
    pub fn battery(&self) -> Option<String> {
        match self.power? {
            Power::Charging => Some("charging".to_string()),
            Power::Draw(mw) => Some(format!("{}.{} W", (mw + 50) / 1000, (mw + 50) % 1000 / 100)),
        }
    }

    /// The parts that exist, in the counter's spacing: "62 °C   Fan 45%   4.8 W"; empty when none does.
    pub fn text(&self) -> String {
        self.text_in(Lang::En)
    }

    /// [`text`](Self::text) in `lang`: the fan and "charging" through the table ("62 °C   Lüfter 45%   4.8 W"), the
    /// temperature and the watts, only a number and a unit, as they are. A part the table lacks stays English.
    pub fn text_in(&self, lang: Lang) -> String {
        let words = |part: Option<String>| match (lang, part) {
            (Lang::De, Some(p)) => Some(crate::i18n::german().translate(&p).map_or(p, |t| t.into_owned())),
            (_, part) => part,
        };
        let battery = if self.power == Some(Power::Charging) { words(self.battery()) } else { self.battery() };
        [self.temperature(), words(self.fan()), battery].into_iter().flatten().collect::<Vec<_>>().join("   ")
    }
}

struct Shared {
    latest: Mutex<Sensors>,
    /// `latest` as the line, in the language the watcher started with.
    line: Mutex<String>,
    stop: AtomicBool,
    /// Readings made so far, and by which thread (tests: never the one that asks for the text).
    reads: AtomicU64,
    reader: Mutex<Option<ThreadId>>,
}

/// Reads [`Sensors`] on its own thread until dropped.
pub struct Watcher {
    shared: Arc<Shared>,
    thread: Option<Thread>,
}

impl Watcher {
    /// Starts reading at once, then every `every`, and words the line in `lang`. Returns without waiting for the first
    /// reading.
    pub fn start(root: PathBuf, battery_dir: PathBuf, every: Duration, lang: Lang) -> Watcher {
        let shared = Arc::new(Shared {
            latest: Mutex::new(Sensors::default()),
            line: Mutex::new(String::new()),
            stop: AtomicBool::new(false),
            reads: AtomicU64::new(0),
            reader: Mutex::new(None),
        });
        let s = shared.clone();
        let spawned = std::thread::Builder::new().name("osd-sensors".into()).spawn(move || {
            *s.reader.lock().unwrap() = Some(std::thread::current().id());
            while !s.stop.load(Ordering::Relaxed) {
                let reading = Sensors::read(&root, &battery_dir);
                let line = reading.text_in(lang);
                *s.latest.lock().unwrap() = reading;
                *s.line.lock().unwrap() = line;
                s.reads.fetch_add(1, Ordering::Relaxed);
                std::thread::park_timeout(every);
            }
        });
        let thread = match spawned {
            Ok(h) => Some(h.thread().clone()),
            Err(e) => {
                eprintln!("osd sensors: can't start the reader: {e}");
                None
            }
        };
        Watcher { shared, thread }
    }

    /// The device's own: sysfs below `fan_curve::sysfs_root`, the battery of `battery::dir`, twice a second, in the
    /// interface's language.
    pub fn system() -> Watcher {
        Watcher::start(crate::fan_curve::sysfs_root(), battery::dir(), EVERY, crate::i18n::current())
    }

    /// The last reading. Cached numbers only.
    pub fn latest(&self) -> Sensors {
        *self.shared.latest.lock().unwrap()
    }

    /// The last reading as the line's tail ("" before the first reading, or when the device has no sensor), worded on
    /// the reading thread.
    pub fn text(&self) -> String {
        self.shared.line.lock().unwrap().clone()
    }

    /// Readings made so far.
    pub fn reads(&self) -> u64 {
        self.shared.reads.load(Ordering::Relaxed)
    }

    /// The thread that reads (None until it has started).
    pub fn reader(&self) -> Option<ThreadId> {
        *self.shared.reader.lock().unwrap()
    }
}

impl Drop for Watcher {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Relaxed);
        // Not joined: the thread is in a sysfs read at worst, and ends by itself.
        if let Some(t) = self.thread.take() {
            t.unpark();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;
    use std::fs;
    use std::time::Instant;

    fn put(root: &Path, rel: &str, v: &str) {
        let p = root.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, v).unwrap();
    }

    fn dir(name: &str) -> crate::testutil::TestDir {
        let d = TestDir::new(&format!("osd-sensors-{name}"));
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn full(power: Power) -> Sensors {
        Sensors { hottest: Some(62_400), fan_rpm: Some(3200), fan_pwm: Some(115), power: Some(power) }
    }

    #[test]
    fn every_sensor_is_in_the_line_in_its_unit() {
        assert_eq!(full(Power::Draw(4_760)).text(), "62 °C   Fan 45%   4.8 W");
        assert_eq!(full(Power::Charging).text(), "62 °C   Fan 45%   charging");
    }

    #[test]
    fn rounding_goes_to_the_nearest_degree_percent_and_tenth_of_a_watt() {
        let t = |m| Sensors { hottest: Some(m), ..Default::default() }.temperature().unwrap();
        assert_eq!((t(62_499).as_str(), t(62_500).as_str(), t(999).as_str()), ("62 °C", "63 °C", "1 °C"));
        let f = |p| Sensors { fan_pwm: Some(p), ..Default::default() }.fan().unwrap();
        assert_eq!((f(255).as_str(), f(128).as_str(), f(300).as_str(), f(0).as_str()), ("Fan 100%", "Fan 50%", "Fan 100%", "Fan off"));
        let w = |mw| Sensors { power: Some(Power::Draw(mw)), ..Default::default() }.battery().unwrap();
        assert_eq!((w(4_749).as_str(), w(4_750).as_str(), w(950).as_str(), w(10_040).as_str(), w(80).as_str()), ("4.7 W", "4.8 W", "1.0 W", "10.0 W", "0.1 W"));
    }

    #[test]
    fn the_fan_is_off_at_zero_and_falls_back_to_rpm() {
        let fan = |rpm, pwm| Sensors { fan_rpm: rpm, fan_pwm: pwm, ..Default::default() }.fan();
        assert_eq!(fan(Some(0), Some(0)).as_deref(), Some("Fan off"));
        assert_eq!(fan(None, Some(0)).as_deref(), Some("Fan off"));
        assert_eq!(fan(Some(0), None).as_deref(), Some("Fan off"));
        assert_eq!(fan(Some(0), Some(90)).as_deref(), Some("Fan off"), "spun down: the rpm says");
        assert_eq!(fan(Some(4400), Some(51)).as_deref(), Some("Fan 20%"));
        assert_eq!(fan(Some(4400), None).as_deref(), Some("Fan 4400 rpm"));
        assert_eq!(fan(None, None), None);
    }

    #[test]
    fn a_missing_sensor_is_left_out_never_zero_or_a_question_mark() {
        let s = full(Power::Draw(4_760));
        assert_eq!(Sensors { hottest: None, ..s }.text(), "Fan 45%   4.8 W");
        assert_eq!(Sensors { fan_rpm: None, fan_pwm: None, ..s }.text(), "62 °C   4.8 W");
        assert_eq!(Sensors { power: None, ..s }.text(), "62 °C   Fan 45%");
        assert_eq!(Sensors { hottest: None, power: None, ..s }.text(), "Fan 45%");
        assert_eq!(Sensors::default().text(), "");
        assert!(!Sensors { power: None, ..s }.text().contains('?'));
    }

    /// Seen on a German Pocket (2026-10-25): "Fan off" and "charging" in English over the game.
    #[test]
    fn the_line_is_german_on_a_german_pocket_with_its_numbers_and_units_as_they_are() {
        let de = |s: Sensors| s.text_in(Lang::De);
        assert_eq!(de(full(Power::Draw(4_760))), "62 °C   Lüfter 45%   4.8 W");
        assert_eq!(de(full(Power::Charging)), "62 °C   Lüfter 45%   lädt");
        let fan = |rpm, pwm| Sensors { fan_rpm: rpm, fan_pwm: pwm, ..Default::default() };
        assert_eq!(de(fan(Some(0), Some(0))), "Lüfter aus");
        assert_eq!(de(fan(Some(4400), None)), "Lüfter 4400 rpm");
        assert_eq!(de(fan(None, Some(255))), "Lüfter 100%");
        assert_eq!(de(Sensors { hottest: Some(9_000), power: Some(Power::Draw(10_040)), ..Default::default() }), "9 °C   10.0 W");
        assert_eq!(de(Sensors::default()), "", "nothing to show is still nothing");
        // English stays as it was.
        assert_eq!(full(Power::Charging).text_in(Lang::En), "62 °C   Fan 45%   charging");
        // Every word of the line has its German line in the table.
        for s in [fan(Some(0), None), fan(None, Some(115)), fan(Some(3200), None), Sensors { power: Some(Power::Charging), ..Default::default() }] {
            let (en, de) = (s.text(), de(s));
            assert_ne!(en, de, "{en} has a German line");
        }
    }

    #[test]
    fn the_watcher_words_its_line_in_the_language_it_started_with_on_its_own_thread() {
        let root = dir("watch-de");
        put(&root, "sys/class/hwmon/hwmon3/name", "pwmfan\n");
        put(&root, "sys/class/hwmon/hwmon3/pwm1", "0\n");
        let w = Watcher::start(root.to_path_buf(), root.join("battery"), Duration::from_millis(5), Lang::De);
        let until = Instant::now() + Duration::from_secs(5);
        while w.reads() < 1 && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(w.text(), "Lüfter aus");
        drop(w);
    }

    #[test]
    fn reading_a_device_without_sensors_gives_nothing_and_one_with_them_everything() {
        let root = dir("read");
        let bat = root.join("battery");
        assert_eq!(Sensors::read(&root, &bat), Sensors::default(), "nothing there: nothing is made up");
        // Thermal zones: the hottest counts; a zone that reads 0 is no reading.
        put(&root, "sys/class/thermal/thermal_zone0/temp", "41000\n");
        put(&root, "sys/class/thermal/thermal_zone1/temp", "62400\n");
        assert_eq!(Sensors::read(&root, &bat).text(), "62 °C");
        fs::remove_dir_all(root.join("sys")).unwrap();
        put(&root, "sys/class/thermal/thermal_zone0/temp", "0\n");
        assert_eq!(Sensors::read(&root, &bat).text(), "", "a zone at 0 is not a temperature");
        // Fan, as Readout finds it: the pwmfan's hwmon.
        put(&root, "sys/class/hwmon/hwmon3/name", "pwmfan\n");
        put(&root, "sys/class/hwmon/hwmon3/pwm1", "115\n");
        put(&root, "sys/class/hwmon/hwmon3/fan1_input", "3200\n");
        assert_eq!(Sensors::read(&root, &bat).text(), "Fan 45%");
        // Battery: current x voltage while discharging, "charging" on the charger, nothing when full or without current.
        let gauge = |status: &str, ua: &str| {
            put(&bat, "capacity", "80\n");
            put(&bat, "status", &format!("{status}\n"));
            put(&bat, "voltage_now", "4000000\n");
            put(&bat, "current_now", &format!("{ua}\n"));
        };
        gauge("Discharging", "1200000");
        assert_eq!(Sensors::read(&root, &bat).text(), "Fan 45%   4.8 W");
        gauge("Discharging", "-1200000");
        assert_eq!(Sensors::read(&root, &bat).battery().as_deref(), Some("4.8 W"), "the gauge's sign differs between drivers");
        gauge("Charging", "-900000");
        assert_eq!(Sensors::read(&root, &bat).text(), "Fan 45%   charging");
        gauge("Full", "10000");
        assert_eq!(Sensors::read(&root, &bat).text(), "Fan 45%");
        gauge("Discharging", "0");
        assert_eq!(Sensors::read(&root, &bat).text(), "Fan 45%", "no current: no draw");
    }

    #[test]
    fn the_readings_come_from_a_thread_of_their_own_and_stop_with_the_watcher() {
        let root = dir("watch");
        put(&root, "sys/class/thermal/thermal_zone0/temp", "55000\n");
        let w = Watcher::start(root.to_path_buf(), root.join("battery"), Duration::from_millis(5), Lang::En);
        let until = Instant::now() + Duration::from_secs(5);
        while w.reads() < 3 && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(w.reads() >= 3, "it keeps reading on its cadence");
        assert!(w.reader().is_some_and(|id| id != std::thread::current().id()), "not the thread that asks");
        assert_eq!(w.text(), "55 °C");
        let shared = w.shared.clone();
        drop(w);
        std::thread::sleep(Duration::from_millis(50));
        let at = shared.reads.load(Ordering::Relaxed);
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(shared.reads.load(Ordering::Relaxed), at, "no reading after the watcher is gone");
    }
}
