//! Settings > Performance & fan: what games start with, the fan outside games, and the live readout.
//!
//! - **Games: performance / Games: fan** are the defaults below the pause menu's Options > Extras. Which value a
//!   pocket-core game uses: this game's file, else its console's file, else these. Their start values: Auto
//!   performance (the system's own) and a Quiet fan (the owner, 2026-10-23, measured with Burnout on ARMSX2: Quiet
//!   averaged PWM 52 against Auto's 64, at most 71 against 80, the CPU at 60–70 °C without throttling). A file that
//!   names a fan keeps it, "auto" too: the file can't tell a choice from a value written with another change, so
//!   only a missing key (or no file, a new install) starts on Quiet. `options::GameSettings::load` reads them (`game_defaults`), so pocket-core, the pause
//!   menu and its "Restore defaults" all see the same order without knowing about this page.
//! - **Fan outside games** is pocket-ui's own: [`FanKeeper`] writes the profile while no game runs and pocket-ui is
//!   awake (shelf, lists, Settings), and puts the system's profile back for every game, for light and deep sleep,
//!   and when pocket-ui ends. It uses `power::Power` with its own note ([`power::OUTSIDE_RESTORE_FILE`]), so a crash is
//!   undone by `pocket-core --restore-system` (run by the launcher after pocket-ui ends, and by pocket-ui at its
//!   start) like a crashed game's changes are.
//! - **Smooth fan** (on by default) makes Quiet, Auto and Cool smooth curves that pocket-ui drives itself
//!   (`fan_curve`), in the menus and in every game, PortMaster ports and standalone emulators included. Off: the trip
//!   profiles above, as before.
//! - **The readout** ([`Readout`]) is read off the UI thread once a second while the page is open.
//!
//! The choices are kept in `<data>/performance.json`, written through `fsutil`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::logs::{dir_names, read_i64, read_trim};
use crate::power::{Fan, Performance, Power};

pub const FILE: &str = "performance.json";

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Prefs {
    /// What games run with unless their console or the game itself chose in the pause menu.
    #[serde(default, deserialize_with = "crate::settings::lenient")]
    pub games_performance: Performance,
    /// Quiet unless the file names another (`games_fan_default`).
    #[serde(default = "games_fan_default", deserialize_with = "games_fan_or_quiet")]
    pub games_fan: Fan,
    /// The fan while no game runs and the Pocket is awake.
    #[serde(default, deserialize_with = "crate::settings::lenient")]
    pub outside_fan: Fan,
    /// The fan choices as smooth curves pocket-ui drives (`fan_curve`); false: ROCKNIX's trip steps.
    #[serde(default = "on", deserialize_with = "on_unless_false")]
    pub smooth_fan: bool,
}

impl Default for Prefs {
    fn default() -> Prefs {
        Prefs {
            games_performance: Performance::default(),
            games_fan: games_fan_default(),
            outside_fan: Fan::default(),
            smooth_fan: true,
        }
    }
}

/// Games: fan's start value: Quiet. Auto still means the system's own profile, when chosen.
pub fn games_fan_default() -> Fan {
    Fan::Quiet
}

/// Games: fan as the file says, Quiet for a value that is no fan choice.
fn games_fan_or_quiet<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Fan, D::Error> {
    let v = <serde_json::Value as Deserialize>::deserialize(d)?;
    Ok(serde_json::from_value(v).unwrap_or_else(|_| games_fan_default()))
}

fn on() -> bool {
    true
}

/// Smooth fan stays on unless the file says `false`.
fn on_unless_false<'de, D: serde::Deserializer<'de>>(d: D) -> Result<bool, D::Error> {
    Ok(<serde_json::Value as Deserialize>::deserialize(d)? != serde_json::Value::Bool(false))
}

impl Prefs {
    /// The saved choices; the defaults (the system's own everywhere) without a file.
    pub fn load(data_dir: &Path) -> Prefs {
        crate::fsutil::read_json_or_backup(&data_dir.join(FILE)).unwrap_or_default()
    }

    pub fn save(&self, data_dir: &Path) -> bool {
        match crate::fsutil::write_json(&data_dir.join(FILE), self) {
            Ok(()) => true,
            Err(e) => {
                eprintln!("performance: can't save {FILE}: {e}");
                false
            }
        }
    }

    /// The pause menu's keys (`options.rs`) with the values games start with when neither the game nor its
    /// console chose one.
    pub fn game_defaults(&self) -> BTreeMap<String, String> {
        [("performance", self.games_performance.value()), ("fan", self.games_fan.value())]
            .into_iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }
}

/// [`Prefs::game_defaults`] of the saved choices: what `options::GameSettings::load` puts below the console's file.
pub fn game_defaults(data_dir: &Path) -> BTreeMap<String, String> {
    Prefs::load(data_dir).game_defaults()
}

/// What pocket-ui is doing, as far as the fan outside games cares.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Moment {
    /// A game (or PC streaming) runs, or is about to start: its own profile, or the system's.
    pub in_game: bool,
    /// Light or deep sleep, or powering off.
    pub asleep: bool,
    /// `pocket-core --restore-system` is still putting back what a killed game left: the system's profile
    /// must be in place before ours is noted on top of it.
    pub settling: bool,
}

/// The profile the fan should have now: the choice on the shelf and in the menus, the system's otherwise.
pub fn outside_fan(choice: Fan, m: Moment) -> Fan {
    if m.in_game || m.asleep || m.settling {
        Fan::Auto
    } else {
        choice
    }
}

/// Applies "Fan outside games" and takes it back. Each change from the system's profile to ours notes the system's
/// trips first (`Power::set_fan`), and Auto puts them back and deletes the note (`Power::restore`), so the note only
/// exists while our profile is in place.
pub struct FanKeeper {
    power: Power,
    /// What was applied last; None before the first `sync`.
    applied: Option<Fan>,
}

impl FanKeeper {
    pub fn system() -> FanKeeper {
        FanKeeper::new(Power::with_root("/", crate::power::OUTSIDE_RESTORE_FILE))
    }

    pub fn new(power: Power) -> FanKeeper {
        FanKeeper { power, applied: None }
    }

    pub fn applied(&self) -> Option<Fan> {
        self.applied
    }

    /// Makes `want` the fan profile, if it isn't already. The first call undoes what a pocket-ui that crashed left
    /// behind (its note), in case nobody else did. Returns true when something was done.
    pub fn sync(&mut self, want: Fan) -> bool {
        if self.applied == Some(want) {
            return false;
        }
        if self.applied.is_none() && self.power.recover() {
            println!("performance: the fan profile a previous pocket-ui left behind is undone");
        }
        let ok = match want {
            Fan::Auto => {
                self.power.restore();
                true
            }
            fan => self.power.set_fan(fan),
        };
        println!("performance: fan outside games {}{}", want.value(), if ok { "" } else { " (no fan here)" });
        self.applied = Some(want);
        true
    }
}

// ---- the readout ----------------------------------------------------------------------------------------

/// One cpufreq policy (a cluster of cores) as it runs now.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Cluster {
    /// The policy's number (its first core): 0, 4 and 7 on the SM8250.
    pub policy: u32,
    pub mhz: Option<i64>,
    /// The limit now (`scaling_max_freq`): lower than the top speed under Battery.
    pub max_mhz: Option<i64>,
}

/// The numbers at the bottom of the page. Only what the device reports; a missing file is a None, never a 0.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Readout {
    /// The hottest thermal zone, millidegrees.
    pub hottest: Option<i64>,
    pub fan_rpm: Option<i64>,
    /// PWM duty, 0 to 255.
    pub fan_pwm: Option<i64>,
    /// The first `active` trip of the fan's zone, millidegrees: where the fan starts with the profile in place.
    pub fan_starts: Option<i64>,
    /// Smooth fan's last beat (`fan_curve::Beat`), if it is recent: then pocket-ui drives the fan.
    pub curve: Option<crate::fan_curve::Beat>,
    pub clusters: Vec<Cluster>,
    pub gpu_mhz: Option<i64>,
    /// Battery draw in milliwatts, and whether it is on the charger (then the draw means nothing).
    pub battery_mw: Option<u32>,
    pub plugged: bool,
}

/// The hottest thermal zone below `root`, millidegrees.
pub fn hottest_zone(root: &Path) -> Option<i64> {
    let thermal = root.join("sys/class/thermal");
    dir_names(&thermal, "thermal_zone").iter().filter_map(|z| read_i64(&thermal.join(z).join("temp"))).max()
}

/// The fan's hwmon directory below `root`: the `pwmfan`, else any with a `fan1_input`.
pub fn fan_dir(root: &Path) -> Option<PathBuf> {
    let hwmon = root.join("sys/class/hwmon");
    let hwmons = dir_names(&hwmon, "hwmon");
    hwmons
        .iter()
        .find(|h| read_trim(&hwmon.join(h).join("name")).is_some_and(|n| n == "pwmfan"))
        .or_else(|| hwmons.iter().find(|h| hwmon.join(h).join("fan1_input").exists()))
        .map(|h| hwmon.join(h))
}

impl Readout {
    /// Reads sysfs below `root` (`/` on the device) and the battery in `battery_dir`. A handful of small files.
    pub fn read(root: &Path, battery_dir: &Path) -> Readout {
        let hottest = hottest_zone(root);
        let fan_dir = fan_dir(root);
        let cpufreq = root.join("sys/devices/system/cpu/cpufreq");
        let clusters = dir_names(&cpufreq, "policy")
            .iter()
            .filter_map(|p| {
                let policy = p.trim_start_matches("policy").parse().ok()?;
                let khz = |f: &str| read_i64(&cpufreq.join(p).join(f)).map(|k| k / 1000);
                Some(Cluster {
                    policy,
                    mhz: khz("scaling_cur_freq"),
                    max_mhz: khz("scaling_max_freq"),
                })
            })
            .collect();
        let devfreq = root.join("sys/class/devfreq");
        let gpu_mhz = dir_names(&devfreq, "")
            .iter()
            .filter(|n| n.ends_with(".gpu"))
            .find_map(|n| read_i64(&devfreq.join(n).join("cur_freq")))
            .map(|hz| hz / 1_000_000);
        let battery = crate::battery::read(battery_dir, 0);
        Readout {
            hottest,
            fan_rpm: fan_dir.as_ref().and_then(|d| read_i64(&d.join("fan1_input"))),
            fan_pwm: fan_dir.as_ref().and_then(|d| read_i64(&d.join("pwm1"))),
            fan_starts: crate::power::fan_trips(root).first().copied(),
            curve: None,
            clusters,
            gpu_mhz,
            battery_mw: battery.map(|b| b.mw()).filter(|mw| *mw > 0),
            plugged: battery.is_some_and(|b| b.plugged()),
        }
    }

    /// Made-up numbers for `--snapshot perf` (a GBA game's worth, on battery).
    pub fn demo() -> Readout {
        Readout {
            hottest: Some(47_300),
            fan_rpm: Some(4400),
            fan_pwm: Some(51),
            fan_starts: Some(52_000),
            curve: None,
            clusters: vec![
                Cluster {
                    policy: 0,
                    mhz: Some(1804),
                    max_mhz: Some(1804),
                },
                Cluster {
                    policy: 4,
                    mhz: Some(1286),
                    max_mhz: Some(2419),
                },
                Cluster {
                    policy: 7,
                    mhz: Some(1132),
                    max_mhz: Some(2841),
                },
            ],
            gpu_mhz: Some(305),
            battery_mw: Some(2100),
            plugged: false,
        }
    }

    /// "47.3 °C".
    pub fn temperature(&self) -> Option<String> {
        self.hottest.map(|t| format!("{}.{} °C", t / 1000, (t % 1000).abs() / 100))
    }

    /// "4400 rpm, 20 %", "Off", "20 %"; and who drives it: "smooth, Quiet" (Smooth fan; "full speed, hot" in its
    /// guard), else where the system's trips start it: "starts at 52 °C".
    pub fn fan(&self) -> Option<String> {
        let pct = self.fan_pwm.map(|p| (p.clamp(0, 255) * 100 + 127) / 255);
        let speed = match (self.fan_rpm, pct) {
            (Some(0), _) | (None, Some(0)) => Some("Off".to_string()),
            (Some(rpm), Some(p)) => Some(format!("{rpm} rpm, {p}%")),
            (Some(rpm), None) => Some(format!("{rpm} rpm")),
            (None, Some(p)) => Some(format!("{p}%")),
            (None, None) => None,
        };
        let starts = match self.curve.as_ref().filter(|b| b.driving) {
            Some(b) if b.mode == Some(crate::fan_curve::Mode::Guard) => Some("full speed, hot".to_string()),
            Some(b) => Some(format!(
                "smooth, {}",
                match b.shape {
                    Some(crate::fan_curve::Shape::Quiet) => "Quiet",
                    Some(crate::fan_curve::Shape::Cool) => "Cool",
                    _ => "Auto",
                }
            )),
            None => self.fan_starts.map(|t| format!("starts at {} °C", t / 1000)),
        };
        match (speed, starts) {
            (Some(s), Some(t)) => Some(format!("{s} · {t}")),
            (s, t) => s.or(t),
        }
    }

    /// "Little 1.80 · Big 1.29 · Prime 1.13 GHz · GPU 305 MHz".
    pub fn cpu(&self) -> Option<String> {
        let names = cluster_names(self.clusters.len());
        let parts: Vec<String> = self
            .clusters
            .iter()
            .zip(names)
            .map(|(c, name)| match c.mhz {
                Some(m) => format!("{name} {:.2}", m as f64 / 1000.0),
                None => format!("{name} ?"),
            })
            .collect();
        let mut line = (!parts.is_empty()).then(|| format!("{} GHz", parts.join(" · ")))?;
        if let Some(g) = self.gpu_mhz {
            line.push_str(&format!(" · GPU {g} MHz"));
        }
        Some(line)
    }

    /// "2.1 W", or "On the charger".
    pub fn battery(&self) -> Option<String> {
        if self.plugged {
            return Some("On the charger".into());
        }
        self.battery_mw.map(|mw| format!("{:.1} W", mw as f64 / 1000.0))
    }
}

/// Names for `n` clusters in policy order: the SM8250's little, big and prime ones.
fn cluster_names(n: usize) -> Vec<String> {
    match n {
        1 => vec!["CPU".into()],
        2 => vec!["Little".into(), "Big".into()],
        3 => vec!["Little".into(), "Big".into(), "Prime".into()],
        n => (0..n).map(|i| format!("CPU{i}")).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::options::{setting, GameSettings, Level};
    use crate::power::fake::Fake;
    use crate::testutil::TestDir;

    fn dir(name: &str) -> crate::testutil::TestDir {
        TestDir::new(&format!("perffan-{name}"))
    }

    // ---- the choices and where games get theirs

    #[test]
    fn choices_survive_a_restart_and_a_damaged_file() {
        let d = dir("prefs");
        assert_eq!(Prefs::load(&d), Prefs::default(), "nothing saved: the start values");
        assert_eq!(
            Prefs::default(),
            Prefs {
                games_performance: Performance::Auto,
                games_fan: Fan::Quiet,
                outside_fan: Fan::Auto,
                smooth_fan: true,
            }
        );
        let chosen = Prefs {
            games_performance: Performance::Battery,
            games_fan: Fan::Cool,
            outside_fan: Fan::Quiet,
            smooth_fan: false,
        };
        assert!(chosen.save(&d));
        assert_eq!(Prefs::load(&d), chosen);
        assert!(crate::fsutil::backup_path(&d.join(FILE)).exists(), "written through fsutil, with a backup");
        let text = std::fs::read_to_string(d.join(FILE)).unwrap();
        assert!(
            text.contains("\"battery\"") && text.contains("\"cool\"") && text.contains("\"quiet\""),
            "{text}"
        );
        let whole = std::fs::read(d.join(FILE)).unwrap();
        std::fs::write(d.join(FILE), &whole[..whole.len() / 2]).unwrap();
        assert_eq!(Prefs::load(&d), chosen, "cut off: the backup");
    }

    #[test]
    fn an_unknown_value_falls_back_alone() {
        let d = dir("lenient");
        std::fs::write(d.join(FILE), br#"{"games_performance": "turbo", "games_fan": "cool", "outside_fan": 3, "smooth_fan": "yes"}"#).unwrap();
        assert_eq!(
            Prefs::load(&d),
            Prefs {
                games_fan: Fan::Cool,
                ..Prefs::default()
            }
        );
        // Games: fan that is no fan choice: its start value, Quiet.
        std::fs::write(d.join(FILE), br#"{"games_fan": "turbo"}"#).unwrap();
        assert_eq!(Prefs::load(&d).games_fan, Fan::Quiet);
        // A file from before Smooth fan existed: on. Only an explicit false turns it off.
        std::fs::write(d.join(FILE), br#"{"games_fan": "quiet"}"#).unwrap();
        assert!(Prefs::load(&d).smooth_fan);
        std::fs::write(d.join(FILE), br#"{"smooth_fan": false}"#).unwrap();
        assert!(!Prefs::load(&d).smooth_fan);
    }

    fn effective(data: &Path, system: &str, rom: &str) -> (String, String) {
        let v = GameSettings::load(data, system, Path::new(rom)).effective();
        (v["performance"].clone(), v["fan"].clone())
    }

    fn pair(p: &str, f: &str) -> (String, String) {
        (p.to_string(), f.to_string())
    }

    #[test]
    fn games_fan_starts_on_quiet_but_a_file_that_says_auto_keeps_auto() {
        let d = dir("games-fan-default");
        assert_eq!(Prefs::load(&d).games_fan, Fan::Quiet, "a new install");
        // A file from before the start value changed, without the key: Quiet too.
        std::fs::write(d.join(FILE), br#"{"games_performance": "battery", "outside_fan": "cool"}"#).unwrap();
        let p = Prefs::load(&d);
        assert_eq!((p.games_performance, p.games_fan, p.outside_fan), (Performance::Battery, Fan::Quiet, Fan::Cool));
        // "auto" in the file may have been chosen (or written along with another change; the file can't tell):
        // it stays Auto, the system's own profile.
        std::fs::write(d.join(FILE), br#"{"games_fan": "auto"}"#).unwrap();
        assert_eq!(Prefs::load(&d).games_fan, Fan::Auto);
        let p = Prefs {
            games_fan: Fan::Auto,
            ..Prefs::default()
        };
        assert!(p.save(&d));
        assert_eq!(Prefs::load(&d).games_fan, Fan::Auto, "Auto chosen and saved stays Auto");
        // What a game gets with nothing chosen anywhere.
        let e = dir("games-fan-effective");
        assert_eq!(effective(&e, "gba", "/r/gba/A.gba"), pair("auto", "quiet"));
        // Auto still means the system's own profile: with Smooth fan the curve of ROCKNIX's cooling profile.
        assert_eq!(crate::fan_curve::shape_for(Fan::Auto, "moderate"), Some(crate::fan_curve::Shape::Balanced));
    }

    #[test]
    fn a_game_beats_its_console_which_beats_this_page_which_beats_the_system() {
        let d = dir("precedence");
        let rom = "/storage/roms/gba/Game.gba";
        assert_eq!(effective(&d, "gba", rom), pair("auto", "quiet"), "nothing chosen anywhere: the start values");

        Prefs {
            games_performance: Performance::Performance,
            games_fan: Fan::Cool,
            outside_fan: Fan::Auto,
            smooth_fan: true,
        }
        .save(&d);
        assert_eq!(effective(&d, "gba", rom), pair("performance", "cool"), "this page");
        assert_eq!(
            effective(&d, "snes", "/storage/roms/snes/Other.sfc"),
            pair("performance", "cool"),
            "for every console"
        );

        // The console saved Battery in the pause menu; its fan was never touched, so the page's still counts.
        let perf = setting("performance").unwrap();
        let mut s = GameSettings::load(&d, "gba", Path::new(rom));
        assert_eq!(s.level, Level::Defaults);
        assert_eq!(perf.choices[s.index(perf)].value, "performance", "the pause menu shows the page's value too");
        s.values.insert("performance".into(), "battery".into());
        assert!(s.save_for_console());
        assert_eq!(effective(&d, "gba", rom), pair("battery", "cool"));
        assert_eq!(
            effective(&d, "snes", "/storage/roms/snes/Other.sfc"),
            pair("performance", "cool"),
            "other consoles keep the page's"
        );

        // This game chose Auto and Quiet: that wins over both.
        let mut g = GameSettings::load(&d, "gba", Path::new(rom));
        g.values.insert("performance".into(), "auto".into());
        g.values.insert("fan".into(), "quiet".into());
        assert!(g.save_for_game());
        assert_eq!(effective(&d, "gba", rom), pair("auto", "quiet"));
        assert_eq!(effective(&d, "gba", "/storage/roms/gba/Another.gba"), pair("battery", "cool"));

        // Restore defaults in the pause menu: back to this page, not to the system.
        g.restore_defaults();
        assert_eq!(effective(&d, "gba", rom), pair("performance", "cool"));
        // And the page back on its start values: Auto performance (the system's own) and a Quiet fan.
        Prefs::default().save(&d);
        assert_eq!(effective(&d, "gba", rom), pair("auto", "quiet"));
    }

    #[test]
    fn settings_files_from_before_the_page_keep_working() {
        let d = dir("old-files");
        // A console file and a game file as the pause menu wrote them before this page existed.
        std::fs::create_dir_all(d.join("game-settings/gbc")).unwrap();
        std::fs::write(d.join("game-settings/gbc.json"), br#"{"effect": "grid", "fan": "cool"}"#).unwrap();
        std::fs::write(d.join("game-settings/gbc/Tetris.json"), br#"{"performance": "battery"}"#).unwrap();
        assert_eq!(effective(&d, "gbc", "/r/gbc/Zelda.gbc"), pair("auto", "cool"));
        assert_eq!(
            effective(&d, "gbc", "/r/gbc/Tetris.gbc"),
            pair("battery", "quiet"),
            "a game file replaces the console's, as before"
        );
        Prefs {
            games_performance: Performance::Performance,
            games_fan: Fan::Quiet,
            outside_fan: Fan::Auto,
            smooth_fan: true,
        }
        .save(&d);
        assert_eq!(effective(&d, "gbc", "/r/gbc/Zelda.gbc"), pair("performance", "cool"));
        assert_eq!(effective(&d, "gbc", "/r/gbc/Tetris.gbc"), pair("battery", "quiet"));
        let v = GameSettings::load(&d, "gbc", Path::new("/r/gbc/Zelda.gbc")).effective();
        assert_eq!(v["effect"], "grid", "the other settings are untouched");
        // pocket-core turns the values into its Extras the same way.
        let ex = crate::extras::Settings::from_values(&v);
        assert_eq!((ex.performance, ex.fan), (Performance::Performance, Fan::Cool));
    }

    // ---- the fan outside games

    #[test]
    fn the_fan_outside_games_is_the_systems_while_playing_asleep_or_settling() {
        let awake = Moment::default();
        assert_eq!(outside_fan(Fan::Cool, awake), Fan::Cool);
        assert_eq!(outside_fan(Fan::Quiet, awake), Fan::Quiet);
        for m in [
            Moment { in_game: true, ..awake },
            Moment { asleep: true, ..awake },
            Moment { settling: true, ..awake },
        ] {
            assert_eq!(outside_fan(Fan::Cool, m), Fan::Auto, "{m:?}");
        }
        assert_eq!(outside_fan(Fan::Auto, awake), Fan::Auto);
    }

    fn keeper(f: &Fake) -> FanKeeper {
        FanKeeper::new(Power::with_root(&f.0, f.0.join("outside.json")))
    }

    fn trips(f: &Fake) -> Vec<String> {
        (0..9).map(|i| f.trip(i)).collect()
    }

    const SYSTEM: [&str; 9] = ["90000", "110000", "52000", "62000", "69000", "75000", "80000", "85000", "95000"];
    const COOL: [&str; 9] = ["90000", "110000", "40000", "48000", "55000", "61000", "66000", "71000", "95000"];

    #[test]
    fn the_shelf_gets_the_choice_and_a_game_gets_the_systems_profile_back_first() {
        let f = Fake::new("outside");
        let note = f.0.join("outside.json");
        let mut k = keeper(&f);
        let at = |in_game: bool, asleep: bool| Moment {
            in_game,
            asleep,
            settling: false,
        };
        // The shelf: Cool.
        assert!(k.sync(outside_fan(Fan::Cool, at(false, false))));
        assert_eq!(trips(&f), COOL);
        assert!(note.exists(), "the system's trips are noted while ours are in place");
        assert!(!k.sync(Fan::Cool), "nothing to do twice");
        // A game starts: the system's trips are back before it runs, and the note is gone, so pocket-core
        // notes the system's trips as its originals.
        assert!(k.sync(outside_fan(Fan::Cool, at(true, false))));
        assert_eq!(trips(&f), SYSTEM);
        assert!(!note.exists());
        // pocket-core's own Quiet for the game, and its restore when the game ends.
        let mut core = Power::with_root(&f.0, f.0.join("core.json"));
        core.set_fan(Fan::Quiet);
        core.restore();
        assert_eq!(trips(&f), SYSTEM);
        // Back on the shelf: Cool again, from the system's trips.
        k.sync(outside_fan(Fan::Cool, at(false, false)));
        assert_eq!(trips(&f), COOL);
        // Light or deep sleep: the system's; awake again: Cool.
        k.sync(outside_fan(Fan::Cool, at(false, true)));
        assert_eq!(trips(&f), SYSTEM);
        k.sync(outside_fan(Fan::Cool, at(false, false)));
        assert_eq!(trips(&f), COOL);
        // Switched to Auto on the page, or pocket-ui ends: the system's, no note left.
        k.sync(Fan::Auto);
        assert_eq!(trips(&f), SYSTEM);
        assert!(!note.exists());
    }

    #[test]
    fn a_crashed_pocket_ui_is_undone_by_the_restore_note() {
        let f = Fake::new("outside-crash");
        let mut first = keeper(&f);
        first.sync(Fan::Cool);
        drop(first); // killed on the shelf
        assert_eq!(trips(&f), COOL);
        // `pocket-core --restore-system` (the launcher's clean-up) recovers the same note.
        assert!(Power::with_root(&f.0, f.0.join("outside.json")).recover());
        assert_eq!(trips(&f), SYSTEM);

        // Without that clean-up, the next pocket-ui's first sync does it before it notes anything.
        let mut second = keeper(&f);
        second.sync(Fan::Quiet);
        second.sync(Fan::Cool);
        drop(second);
        let mut third = keeper(&f);
        assert_eq!(third.applied(), None);
        third.sync(Fan::Quiet);
        third.sync(Fan::Auto);
        assert_eq!(trips(&f), SYSTEM, "the system's trips, not the crashed run's Cool, were noted");
    }

    #[test]
    fn a_game_killed_mid_way_is_cleaned_up_before_the_shelf_notes_its_trips() {
        let f = Fake::new("outside-settle");
        let mut k = keeper(&f);
        k.sync(Fan::Auto);
        // pocket-core set Cool for its game and was killed: its note is still there.
        let mut core = Power::with_root(&f.0, f.0.join("core.json"));
        core.set_fan(Fan::Cool);
        drop(core);
        // While `--restore-system` runs, the shelf waits with the system's profile...
        let settling = Moment {
            settling: true,
            ..Moment::default()
        };
        assert!(!k.sync(outside_fan(Fan::Quiet, settling)));
        // ...which puts the system's trips back; only then the shelf's Quiet, and the right originals are noted.
        assert!(Power::with_root(&f.0, f.0.join("core.json")).recover());
        k.sync(outside_fan(Fan::Quiet, Moment::default()));
        k.sync(Fan::Auto);
        assert_eq!(trips(&f), SYSTEM);
    }

    #[test]
    fn auto_never_touches_a_thing_and_no_fan_is_no_failure() {
        let f = Fake::new("outside-auto");
        let mut k = keeper(&f);
        assert!(k.sync(Fan::Auto));
        assert_eq!(trips(&f), SYSTEM);
        assert!(!f.0.join("outside.json").exists());
        let empty = dir("nofan");
        let mut k = FanKeeper::new(Power::with_root(&empty, empty.join("n.json")));
        k.sync(Fan::Cool);
        assert_eq!(k.applied(), Some(Fan::Cool), "remembered, so it isn't tried again every frame");
    }

    // ---- the readout

    fn put(root: &Path, rel: &str, v: &str) {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, v).unwrap();
    }

    #[test]
    fn the_readout_reads_what_sensors_log_reads() {
        let f = Fake::new("readout");
        let root = &f.0;
        put(root, "sys/class/thermal/thermal_zone0/temp", "41000\n");
        put(root, "sys/class/thermal/thermal_zone6/temp", "61300\n");
        put(root, "sys/class/hwmon/hwmon1/name", "other\n");
        put(root, "sys/class/hwmon/hwmon3/name", "pwmfan\n");
        put(root, "sys/class/hwmon/hwmon3/fan1_input", "4400\n");
        put(root, "sys/class/hwmon/hwmon3/pwm1", "51\n");
        put(root, "sys/class/devfreq/3d00000.gpu/cur_freq", "587000000\n");
        for (p, cur) in [("policy0", "1804800"), ("policy4", "1286400"), ("policy7", "1132800")] {
            put(root, &format!("sys/devices/system/cpu/cpufreq/{p}/scaling_cur_freq"), cur);
        }
        put(root, "bat/capacity", "81\n");
        put(root, "bat/status", "Discharging\n");
        put(root, "bat/voltage_now", "3951000\n");
        put(root, "bat/current_now", "420000\n");
        let r = Readout::read(root, &root.join("bat"));
        assert_eq!(r.temperature().as_deref(), Some("61.3 °C"));
        assert_eq!(r.fan().as_deref(), Some("4400 rpm, 20% · starts at 52 °C"));
        assert_eq!(r.cpu().as_deref(), Some("Little 1.80 · Big 1.29 · Prime 1.13 GHz · GPU 587 MHz"));
        assert_eq!(r.clusters[1].max_mhz, Some(2419), "the limit now, for Battery");
        assert_eq!(r.battery().as_deref(), Some("1.7 W"));
        // With the shelf's Cool in place, the readout says where the fan starts now.
        let mut k = keeper(&f);
        k.sync(Fan::Cool);
        assert!(Readout::read(root, &root.join("bat")).fan().unwrap().ends_with("starts at 40 °C"));
        put(root, "bat/status", "Charging\n");
        assert_eq!(Readout::read(root, &root.join("bat")).battery().as_deref(), Some("On the charger"));
    }

    #[test]
    fn with_smooth_fan_driving_the_readout_says_so_instead_of_the_trips() {
        use crate::fan_curve::{Beat, Mode, Shape};
        let beat = |mode, shape| Beat {
            driving: true,
            mode: Some(mode),
            shape: Some(shape),
            ema: Some(56_100),
        };
        let r = Readout {
            fan_rpm: Some(1900),
            fan_pwm: Some(43),
            fan_starts: Some(83_500),
            curve: Some(beat(Mode::Curve, Shape::Quiet)),
            ..Readout::default()
        };
        assert_eq!(r.fan().as_deref(), Some("1900 rpm, 17% · smooth, Quiet"));
        let hot = Readout {
            fan_pwm: Some(255),
            curve: Some(beat(Mode::Guard, Shape::Quiet)),
            ..r.clone()
        };
        assert_eq!(hot.fan().as_deref(), Some("1900 rpm, 100% · full speed, hot"));
        // The system's again (asleep, Smooth fan off): its trips.
        let system = Readout {
            fan_starts: Some(52_000),
            curve: Some(Beat {
                driving: false,
                mode: None,
                shape: None,
                ema: None,
            }),
            ..r
        };
        assert_eq!(system.fan().as_deref(), Some("1900 rpm, 17% · starts at 52 °C"));
    }

    #[test]
    fn a_device_without_the_files_reads_as_unknown_not_as_zero() {
        let empty = dir("readout-empty");
        let r = Readout::read(&empty, &empty.join("bat"));
        assert_eq!(r, Readout::default());
        assert_eq!((r.temperature(), r.fan(), r.cpu(), r.battery()), (None, None, None, None));
        let off = Readout {
            fan_rpm: Some(0),
            fan_pwm: Some(0),
            ..Readout::default()
        };
        assert_eq!(off.fan().as_deref(), Some("Off"));
        let pwm_only = Readout {
            fan_pwm: Some(255),
            ..Readout::default()
        };
        assert_eq!(pwm_only.fan().as_deref(), Some("100%"));
        let two = Readout {
            clusters: vec![
                Cluster {
                    policy: 0,
                    mhz: Some(1000),
                    max_mhz: None,
                },
                Cluster {
                    policy: 4,
                    mhz: None,
                    max_mhz: None,
                },
            ],
            ..Readout::default()
        };
        assert_eq!(two.cpu().as_deref(), Some("Little 1.00 · Big ? GHz"));
        assert_eq!(cluster_names(4), ["CPU0", "CPU1", "CPU2", "CPU3"]);
    }

    #[test]
    fn the_demo_numbers_are_complete() {
        let d = Readout::demo();
        assert!(d.temperature().is_some() && d.fan().is_some() && d.cpu().is_some() && d.battery().is_some());
    }
}
