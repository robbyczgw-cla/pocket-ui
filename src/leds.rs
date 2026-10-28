//! The lights around the analog sticks (Retroid Pocket Mini V2, SM8250): Settings > LEDs.
//!
//! What the hardware is: eight RGB LEDs, four per stick, as multicolour LED class devices
//! `/sys/devices/platform/multi-ledl{1..4}/leds/rgb:l{n}` and `multi-ledr{1..4}/leds/rgb:r{n}`. Each has `brightness`
//! (0..255) and `multi_intensity` in the order **blue green red** (ROCKNIX's scripts swap it, so a caller of
//! `analog_sticks_ledcontrol` gives red, green, blue).
//!
//! What ROCKNIX does with them (20261001, `quirks/platforms/SM8250`):
//!
//! | what | where |
//! |---|---|
//! | `/usr/bin/ledcontrol off\|rgb\|battery` | zeroes the LEDs, applies `analogsticks.led`, or starts `batteryledstatus.service`; stores the mode as `led.color` in system.cfg |
//! | `/usr/bin/analog_sticks_ledcontrol B R G B R G B` | brightness, right stick RGB, left stick RGB |
//! | `analogsticks.led` | those seven numbers, what EmulationStation's "Analog sticks LED colour" saves |
//! | `500-enable-led` (boot) | `ledcontrol $led.color`, and **`battery` when nothing is stored**: full brightness green, which is why a fresh install glows |
//! | `031-analog_leds` (boot) | `analog_sticks_ledcontrol $analogsticks.led` |
//! | `sleep.d/pre,post/001-led` | off before suspend, `ledcontrol $sleep.led.color` after |
//!
//! So pocket-ui talks to the same two scripts and the same two system.cfg keys: what the player picks here is what
//! the next boot applies, and EmulationStation (if it is ever opened) shows the same state. pocket-ui keeps its own
//! copy of the choice in `<data>/leds.json`; the file existing means "configured", and without it the first start
//! switches the lights off once (`default_off_if_unset`), like `system_ctl::cap_gpu_if_unset` does for the GPU.
//!
//! The scripts are the seam: `Leds` runs them below a root (`/`, or a fake tree in the tests).

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;

use crate::fsutil;

/// The saved choice.
const FILE: &str = "leds.json";

const LEDCONTROL: &str = "usr/bin/ledcontrol";
const STICKS: &str = "usr/bin/analog_sticks_ledcontrol";
const SYSTEMCTL: &str = "usr/bin/systemctl";
/// First LED of the left stick; there when the board has the lights.
const SYSFS_PROBE: &str = "sys/devices/platform/multi-ledl1/leds/rgb:l1/multi_intensity";
/// The service that keeps the LEDs on the battery colour while `led.color` is `battery`.
const BATTERY_UNIT: &str = "batteryledstatus.service";

/// A field that falls back to its default when its value doesn't fit (a colour a later version added), instead of
/// failing the whole file and losing the other choices.
fn lenient<'de, D, T>(d: D) -> Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::de::DeserializeOwned + Default,
{
    let v = <serde_json::Value as serde::Deserialize>::deserialize(d)?;
    Ok(serde_json::from_value(v).unwrap_or_default())
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    /// Dark. The default.
    #[default]
    Off,
    /// One colour on both sticks (`ledcontrol rgb`).
    Colour,
    /// ROCKNIX's battery colours (`ledcontrol battery`): green, orange, red.
    Battery,
}

impl Mode {
    pub const ALL: [Mode; 3] = [Mode::Off, Mode::Colour, Mode::Battery];

    pub fn label(self) -> &'static str {
        match self {
            Mode::Off => "Off",
            Mode::Colour => "Colour",
            Mode::Battery => "Battery",
        }
    }

    pub fn blurb(self) -> &'static str {
        match self {
            Mode::Off => "The lights around the sticks stay dark.",
            Mode::Colour => "Both sticks glow in the colour you pick.",
            Mode::Battery => "Green when charged, orange and then red as it runs low. Always at full brightness.",
        }
    }
}

/// The colours on offer: calm ones, and the game's own.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Hue {
    #[default]
    Blue,
    Warm,
    Teal,
    Green,
    Violet,
    Rose,
    /// Warm in the menus; while a game runs, the colour of its system's stripe.
    Game,
}

impl Hue {
    pub const ALL: [Hue; 7] = [Hue::Blue, Hue::Warm, Hue::Teal, Hue::Green, Hue::Violet, Hue::Rose, Hue::Game];

    pub fn label(self) -> &'static str {
        match self {
            Hue::Blue => "Blue",
            Hue::Warm => "Warm",
            Hue::Teal => "Teal",
            Hue::Green => "Green",
            Hue::Violet => "Violet",
            Hue::Rose => "Rose",
            Hue::Game => "The game's",
        }
    }

    /// Red, green, blue as the LEDs show them (their reds are strong, so the pinks and the warm white lean on green and blue).
    pub fn rgb(self) -> (u8, u8, u8) {
        match self {
            Hue::Blue => (30, 90, 255),
            Hue::Warm => (255, 150, 60),
            Hue::Teal => (0, 200, 170),
            Hue::Green => (50, 220, 80),
            Hue::Violet => (140, 70, 255),
            Hue::Rose => (255, 60, 130),
            Hue::Game => Hue::Warm.rgb(),
        }
    }

    pub fn blurb(self) -> &'static str {
        match self {
            Hue::Game => "Warm light in the menus. While a game runs, the colour of its system.",
            _ => "The colour of both sticks.",
        }
    }
}

/// Brightness steps in percent. The LED's own scale is 0..255.
pub const LEVELS: [u8; 4] = [25, 50, 75, 100];
const DEFAULT_LEVEL: usize = 1;

/// The LED class `brightness` for a step (index into `LEVELS`).
pub fn raw_brightness(level: usize) -> u8 {
    (LEVELS[level.min(LEVELS.len() - 1)] as u32 * 255 / 100) as u8
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, serde::Serialize, serde::Deserialize)]
pub struct Config {
    #[serde(default, deserialize_with = "lenient")]
    pub mode: Mode,
    #[serde(default, deserialize_with = "lenient")]
    pub hue: Hue,
    /// Index into `LEVELS`.
    #[serde(default = "default_level", deserialize_with = "lenient_level")]
    pub level: usize,
}

fn default_level() -> usize {
    DEFAULT_LEVEL
}

fn lenient_level<'de, D: serde::Deserializer<'de>>(d: D) -> Result<usize, D::Error> {
    let v = <serde_json::Value as serde::Deserialize>::deserialize(d)?;
    Ok(v.as_u64().map_or(DEFAULT_LEVEL, |n| (n as usize).min(LEVELS.len() - 1)))
}

impl Default for Config {
    fn default() -> Self {
        Config {
            mode: Mode::Off,
            hue: Hue::default(),
            level: DEFAULT_LEVEL,
        }
    }
}

/// `delta` steps along `all` from `current`, wrapping.
fn cycle<T: Copy + PartialEq>(all: &[T], current: T, delta: i32) -> T {
    let at = all.iter().position(|x| *x == current).unwrap_or(0) as i32;
    all[(at + delta.signum()).rem_euclid(all.len() as i32) as usize]
}

impl Config {
    pub fn load(data_dir: &Path) -> Option<Config> {
        fsutil::read_json_or_backup(&data_dir.join(FILE))
    }

    pub fn save(&self, data_dir: &Path) {
        if let Err(e) = fsutil::write_json(&data_dir.join(FILE), self) {
            eprintln!("leds: can't save {FILE}: {e}");
        }
    }

    pub fn step_mode(&mut self, delta: i32) {
        self.mode = cycle(&Mode::ALL, self.mode, delta);
    }

    pub fn step_hue(&mut self, delta: i32) {
        self.hue = cycle(&Hue::ALL, self.hue, delta);
    }

    /// Brightness stops at the ends: wrapping from full to dim would be a jump into the dark.
    pub fn step_level(&mut self, delta: i32) {
        self.level = (self.level as i32 + delta.signum()).clamp(0, LEVELS.len() as i32 - 1) as usize;
    }

    /// The value of the Settings row: "Off", "Battery", or the colour.
    pub fn summary(&self) -> &'static str {
        match self.mode {
            Mode::Off => "Off",
            Mode::Battery => "Battery",
            Mode::Colour => self.hue.label(),
        }
    }

    pub fn brightness_label(&self) -> String {
        format!("{}%", LEVELS[self.level.min(LEVELS.len() - 1)])
    }

    /// The seven numbers of `analogsticks.led`: brightness, right RGB, left RGB.
    pub fn sticks_args(&self) -> Vec<String> {
        sticks_args(self.level, self.hue.rgb())
    }
}

fn sticks_args(level: usize, (r, g, b): (u8, u8, u8)) -> Vec<String> {
    [raw_brightness(level), r, g, b, r, g, b].iter().map(|n| n.to_string()).collect()
}

/// `0xRRGGBB` of a stripe colour.
pub fn unpack(rgb: u32) -> (u8, u8, u8) {
    ((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

/// One thing to do, in order.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Step {
    /// ROCKNIX's `set_setting key value` (system.cfg, written by `system_cfg` under ROCKNIX's lock).
    Setting { key: &'static str, value: String },
    /// A program below the root. `needed` false: a failure doesn't make the whole change count as failed.
    Run { program: &'static str, args: Vec<String>, needed: bool },
}

fn run(program: &'static str, args: &[&str]) -> Step {
    Step::Run {
        program,
        args: args.iter().map(|s| s.to_string()).collect(),
        needed: true,
    }
}

/// The battery colours loop would write over a colour or the dark for up to 15 s (ten while it flashes at 10 %):
/// stopped first. ROCKNIX's `ledcontrol battery` starts it again.
fn stop_battery_colours() -> Step {
    Step::Run {
        program: SYSTEMCTL,
        args: vec!["stop".into(), BATTERY_UNIT.into()],
        needed: false,
    }
}

/// What a choice means for ROCKNIX's scripts.
pub fn plan(c: &Config) -> Vec<Step> {
    match c.mode {
        Mode::Off => vec![stop_battery_colours(), run(LEDCONTROL, &["off"])],
        // `ledcontrol rgb` reads `analogsticks.led` itself, so the boot applies the same.
        Mode::Colour => vec![
            Step::Setting {
                key: "analogsticks.led",
                value: c.sticks_args().join(" "),
            },
            stop_battery_colours(),
            run(LEDCONTROL, &["rgb"]),
        ],
        Mode::Battery => vec![run(LEDCONTROL, &["battery"])],
    }
}

/// One colour on both sticks right now (brightness step, red green blue), without touching the stored choice: a
/// game's own colour while it runs (a boot, or waking from sleep, brings the menu colour back).
pub fn lights_plan(level: usize, rgb: (u8, u8, u8)) -> Vec<Step> {
    vec![Step::Run {
        program: STICKS,
        args: sticks_args(level, rgb),
        needed: true,
    }]
}

/// The lights, through the scripts below `root`.
pub struct Leds {
    root: PathBuf,
}

impl Leds {
    pub fn system() -> Self {
        Self::with_root("/")
    }

    /// Scripts and sysfs below `root` (a fake tree in tests).
    pub fn with_root(root: impl Into<PathBuf>) -> Self {
        Leds { root: root.into() }
    }

    /// ROCKNIX's script is there (any SM8250 image; the lights themselves may still be missing, see `present`).
    pub fn has_scripts(&self) -> bool {
        self.root.join(LEDCONTROL).exists()
    }

    /// The board has the stick lights.
    pub fn present(&self) -> bool {
        self.root.join(SYSFS_PROBE).exists()
    }

    /// Runs the steps in order and stops at the first failing needed one. True when all needed steps worked.
    pub fn run(&self, steps: &[Step]) -> bool {
        for step in steps {
            let (ok, needed) = match step {
                Step::Setting { key, value } => (self.set_setting(key, value), true),
                Step::Run { program, args, needed } => (self.exec(program, args), *needed),
            };
            if !ok && needed {
                eprintln!("leds: {step:?} failed");
                return false;
            }
        }
        true
    }

    pub fn apply(&self, c: &Config) -> bool {
        self.run(&plan(c))
    }

    fn exec(&self, program: &str, args: &[String]) -> bool {
        Command::new(self.root.join(program))
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    }

    /// ROCKNIX's `set_setting`, done by `system_cfg` on the system.cfg below the root (ROCKNIX's own file and lock
    /// for "/").
    fn set_setting(&self, key: &str, value: &str) -> bool {
        let cfg = self.root.join(crate::system_cfg::PATH.trim_start_matches('/'));
        crate::system_cfg::SystemCfg::at(&cfg).set(key, value).inspect_err(|e| eprintln!("leds: system.cfg: {e}")).is_ok()
    }
}

/// What the first start did.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FirstStart {
    /// A choice is saved (by the player, or by an earlier first start): nothing touched.
    Configured,
    /// ROCKNIX's script isn't here (not a ROCKNIX image): nothing touched, tried again next start.
    NoScripts,
    /// The lights were switched off and "Off" saved.
    SwitchedOff,
    /// The script failed: nothing saved, tried again next start.
    Failed,
}

/// ROCKNIX switches the lights on at full brightness when nobody chose (`500-enable-led` falls back to the battery
/// colours). If the player never chose here, they go dark once, through ROCKNIX's script, which also stores `led.color`
/// so the next boot starts dark. A saved choice is never touched.
pub fn default_off_if_unset(leds: &Leds, data_dir: &Path) -> FirstStart {
    if Config::load(data_dir).is_some() {
        return FirstStart::Configured;
    }
    if !leds.has_scripts() {
        return FirstStart::NoScripts;
    }
    let off = Config::default();
    if !leds.apply(&off) {
        return FirstStart::Failed;
    }
    off.save(data_dir);
    FirstStart::SwitchedOff
}

/// `default_off_if_unset` on the real system, with a line in the log.
pub fn first_start(data_dir: &Path) {
    let r = default_off_if_unset(&Leds::system(), data_dir);
    println!("leds: first start: {r:?}");
}

// ---------------------------------------------------------------------------------------------------
// Applying off the UI thread

/// Something to do for the lights.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Job {
    /// A choice from the page.
    Choice(Config),
    /// Colours straight to the sticks, nothing stored: a game's own colour when it starts, the menu colour when it ends.
    Direct(usize, (u8, u8, u8)),
}

impl Job {
    pub fn steps(self) -> Vec<Step> {
        match self {
            Job::Choice(c) => plan(&c),
            Job::Direct(level, rgb) => lights_plan(level, rgb),
        }
    }
}

/// The newest job waits here while the one before it runs (ROCKNIX's scripts take a moment, and the player can
/// press faster than that): only the last one matters, and they never run at the same time.
#[derive(Default)]
pub struct Queue {
    pending: Option<Job>,
    running: bool,
}

impl Queue {
    /// Puts `job` first in line. True when the caller must start the worker.
    pub fn push(&mut self, job: Job) -> bool {
        self.pending = Some(job);
        !std::mem::replace(&mut self.running, true)
    }

    /// The worker's next job; `None` (and the worker ends) when nothing waits.
    pub fn next(&mut self) -> Option<Job> {
        let job = self.pending.take();
        if job.is_none() {
            self.running = false;
        }
        job
    }
}

static QUEUE: Mutex<Queue> = Mutex::new(Queue { pending: None, running: false });

/// Applies `job` on a thread of its own.
pub fn apply_in_background(job: Job) {
    let start = QUEUE.lock().map(|mut q| q.push(job)).unwrap_or(false);
    if !start {
        return;
    }
    std::thread::spawn(|| {
        let leds = Leds::system();
        while let Some(job) = QUEUE.lock().ok().and_then(|mut q| q.next()) {
            if !leds.run(&job.steps()) {
                eprintln!("leds: {job:?} failed");
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;

    fn dir(name: &str) -> crate::testutil::TestDir {
        TestDir::new(&format!("leds-{name}"))
    }

    #[cfg(unix)]
    fn script(root: &Path, rel: &str, body: &str) {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        crate::testutil::write_executable(&p, format!("#!/bin/sh\n{body}\n"));
    }

    /// A tree where ROCKNIX's scripts only write down what they were called with, in `calls.log`.
    #[cfg(unix)]
    fn fake_rocknix(name: &str) -> crate::testutil::TestDir {
        let root = dir(name);
        let log = root.join("calls.log");
        for prog in [LEDCONTROL, STICKS, SYSTEMCTL] {
            script(&root, prog, &format!(r#"echo "$(basename "$0") $*" >> '{}'"#, log.display()));
        }
        // system.cfg below the root, written by `system_cfg`.
        let cfg = root.join(crate::system_cfg::PATH.trim_start_matches('/'));
        std::fs::create_dir_all(cfg.parent().unwrap()).unwrap();
        root
    }

    fn calls(root: &Path) -> Vec<String> {
        std::fs::read_to_string(root.join("calls.log"))
            .unwrap_or_default()
            .lines()
            .map(String::from)
            .collect()
    }

    fn colour(hue: Hue, level: usize) -> Config {
        Config {
            mode: Mode::Colour,
            hue,
            level,
        }
    }

    // --- the mapping -------------------------------------------------------------------------------

    #[test]
    fn off_is_ledcontrol_off_after_the_battery_loop_is_stopped() {
        assert_eq!(
            plan(&Config::default()),
            vec![
                Step::Run {
                    program: SYSTEMCTL,
                    args: vec!["stop".into(), BATTERY_UNIT.into()],
                    needed: false
                },
                Step::Run {
                    program: LEDCONTROL,
                    args: vec!["off".into()],
                    needed: true
                },
            ]
        );
    }

    #[test]
    fn a_colour_is_stored_where_ledcontrol_rgb_and_the_boot_read_it() {
        // Blue at the 50 % step: brightness 127, the same colour for the right and the left stick.
        let steps = plan(&colour(Hue::Blue, 1));
        assert_eq!(
            steps[0],
            Step::Setting {
                key: "analogsticks.led",
                value: "127 30 90 255 30 90 255".into()
            }
        );
        assert_eq!(
            steps.last(),
            Some(&Step::Run {
                program: LEDCONTROL,
                args: vec!["rgb".into()],
                needed: true
            })
        );
        assert!(steps.contains(&Step::Run {
            program: SYSTEMCTL,
            args: vec!["stop".into(), BATTERY_UNIT.into()],
            needed: false
        }));
        // Setting first (ledcontrol rgb reads it), service stopped before the lights are written.
        assert!(matches!(steps[0], Step::Setting { .. }));
        assert!(matches!(steps[1], Step::Run { program: SYSTEMCTL, .. }));
    }

    #[test]
    fn battery_mode_is_ledcontrol_battery_alone() {
        let c = Config {
            mode: Mode::Battery,
            ..Config::default()
        };
        assert_eq!(
            plan(&c),
            vec![Step::Run {
                program: LEDCONTROL,
                args: vec!["battery".into()],
                needed: true
            }]
        );
    }

    #[test]
    fn every_colour_and_step_gives_seven_numbers_in_range() {
        for hue in Hue::ALL {
            for level in 0..LEVELS.len() {
                let args = colour(hue, level).sticks_args();
                assert_eq!(args.len(), 7, "{hue:?} {level}");
                assert!(args.iter().all(|a| a.parse::<u8>().is_ok()));
                assert_eq!(args[1..4], args[4..7], "both sticks the same");
            }
        }
        assert_eq!(raw_brightness(0), 63);
        assert_eq!(raw_brightness(3), 255, "full is the LED's own maximum");
        assert_eq!(raw_brightness(99), 255, "a step out of range is the top");
    }

    #[test]
    fn the_game_colour_is_the_menu_colour_until_a_game_runs() {
        assert_eq!(Hue::Game.rgb(), Hue::Warm.rgb());
        // A game's stripe goes straight to the sticks, nothing stored.
        let steps = lights_plan(1, unpack(0x2b46a8));
        assert_eq!(
            steps,
            vec![Step::Run {
                program: STICKS,
                args: ["127", "43", "70", "168", "43", "70", "168"].map(String::from).to_vec(),
                needed: true
            }]
        );
        assert_eq!(unpack(0x2b46a8), (0x2b, 0x46, 0xa8));
    }

    #[test]
    fn the_page_steps_through_modes_colours_and_brightness() {
        let mut c = Config::default();
        c.step_mode(1);
        assert_eq!(c.mode, Mode::Colour);
        c.step_mode(1);
        c.step_mode(1);
        assert_eq!(c.mode, Mode::Off, "wraps");
        c.step_mode(-1);
        assert_eq!(c.mode, Mode::Battery);
        c.step_hue(-1);
        assert_eq!(c.hue, Hue::Game, "wraps backwards from the first");
        c.step_hue(1);
        assert_eq!(c.hue, Hue::Blue);
        c.level = 3;
        c.step_level(1);
        assert_eq!(c.level, 3, "brightness stops at full");
        c.level = 0;
        c.step_level(-1);
        assert_eq!(c.level, 0, "and at the dimmest");
        assert_eq!(c.brightness_label(), "25%");
    }

    // --- the scripts, against a fake root ----------------------------------------------------------

    #[cfg(unix)]
    #[test]
    fn off_colour_and_battery_call_the_scripts_in_order() {
        let root = fake_rocknix("calls");
        let leds = Leds::with_root(&root);
        assert!(leds.has_scripts());
        assert!(leds.apply(&Config::default()));
        assert_eq!(calls(&root), ["systemctl stop batteryledstatus.service", "ledcontrol off"]);

        std::fs::remove_file(root.join("calls.log")).unwrap();
        assert!(leds.apply(&colour(Hue::Rose, 3)));
        assert_eq!(
            calls(&root),
            ["systemctl stop batteryledstatus.service", "ledcontrol rgb"]
        );
        assert_eq!(
            std::fs::read_to_string(root.join(crate::system_cfg::PATH.trim_start_matches('/'))).unwrap().trim(),
            "analogsticks.led=255 255 60 130 255 60 130"
        );

        std::fs::remove_file(root.join("calls.log")).unwrap();
        assert!(leds.apply(&Config {
            mode: Mode::Battery,
            ..Config::default()
        }));
        assert_eq!(calls(&root), ["ledcontrol battery"]);

        std::fs::remove_file(root.join("calls.log")).unwrap();
        assert!(leds.run(&Job::Direct(2, unpack(0x8a9a3b)).steps()));
        assert_eq!(calls(&root), ["analog_sticks_ledcontrol 191 138 154 59 138 154 59"]);
    }

    #[cfg(unix)]
    #[test]
    fn a_missing_battery_service_does_not_fail_the_change_but_a_failing_script_does() {
        let root = fake_rocknix("failing");
        std::fs::remove_file(root.join(SYSTEMCTL)).unwrap();
        let leds = Leds::with_root(&root);
        assert!(leds.apply(&Config::default()), "no systemctl (the virtual device): off still works");
        assert_eq!(calls(&root), ["ledcontrol off"]);

        script(&root, LEDCONTROL, "exit 1");
        assert!(!leds.apply(&Config::default()));
        let empty_dir = dir("empty");
        let empty = Leds::with_root(empty_dir.to_path_buf());
        assert!(!empty.has_scripts());
        assert!(!empty.apply(&colour(Hue::Teal, 0)), "no script, no success");
    }

    #[test]
    fn the_sticks_lights_are_found_by_their_sysfs_node() {
        let root = dir("sysfs");
        assert!(!Leds::with_root(&root).present());
        let p = root.join(SYSFS_PROBE);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, "0 0 0\n").unwrap();
        assert!(Leds::with_root(&root).present());
    }

    // --- persistence and the first start ------------------------------------------------------------

    #[test]
    fn the_choice_is_kept_over_a_restart() {
        let data = dir("persist");
        assert_eq!(Config::load(&data), None, "never configured");
        let c = Config {
            mode: Mode::Colour,
            hue: Hue::Violet,
            level: 2,
        };
        c.save(&data);
        assert_eq!(Config::load(&data), Some(c));
    }

    #[test]
    fn an_unknown_colour_or_step_falls_back_without_losing_the_mode() {
        let data = dir("lenient");
        std::fs::write(data.join(FILE), r#"{"mode":"colour","hue":"ultraviolet","level":9}"#).unwrap();
        assert_eq!(
            Config::load(&data),
            Some(Config {
                mode: Mode::Colour,
                hue: Hue::Blue,
                level: 3
            })
        );
        std::fs::write(data.join(FILE), r#"{"mode":"disco"}"#).unwrap();
        assert_eq!(Config::load(&data), Some(Config::default()), "a file that exists still counts as configured");
        std::fs::write(data.join(FILE), "{}").unwrap();
        assert_eq!(Config::load(&data), Some(Config::default()));
    }

    #[cfg(unix)]
    #[test]
    fn the_first_start_switches_off_once_and_remembers_it() {
        let root = fake_rocknix("first");
        let data = dir("first-data");
        let leds = Leds::with_root(&root);
        assert_eq!(default_off_if_unset(&leds, &data), FirstStart::SwitchedOff);
        assert_eq!(calls(&root), ["systemctl stop batteryledstatus.service", "ledcontrol off"]);
        assert_eq!(Config::load(&data), Some(Config::default()));

        // The second start: nothing runs.
        std::fs::remove_file(root.join("calls.log")).unwrap();
        assert_eq!(default_off_if_unset(&leds, &data), FirstStart::Configured);
        assert!(calls(&root).is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn a_choice_made_in_pocket_ui_is_never_overridden() {
        let root = fake_rocknix("kept");
        let data = dir("kept-data");
        let chosen = Config {
            mode: Mode::Battery,
            ..Config::default()
        };
        chosen.save(&data);
        assert_eq!(default_off_if_unset(&Leds::with_root(&root), &data), FirstStart::Configured);
        assert!(calls(&root).is_empty(), "no script ran");
        assert_eq!(Config::load(&data), Some(chosen));
        // Also when the player chose Off themselves: that is a choice too, and the default has nothing to add.
        Config::default().save(&data);
        assert_eq!(default_off_if_unset(&Leds::with_root(&root), &data), FirstStart::Configured);
    }

    #[cfg(unix)]
    #[test]
    fn the_first_start_tries_again_when_it_could_not_switch_off() {
        let data = dir("retry-data");
        // Not a ROCKNIX image: nothing saved, so a later start on the device still gets its turn.
        let empty_dir = dir("retry-empty");
        assert_eq!(default_off_if_unset(&Leds::with_root(empty_dir.to_path_buf()), &data), FirstStart::NoScripts);
        assert_eq!(Config::load(&data), None);

        let root = fake_rocknix("retry");
        script(&root, LEDCONTROL, "exit 1");
        assert_eq!(default_off_if_unset(&Leds::with_root(&root), &data), FirstStart::Failed);
        assert_eq!(Config::load(&data), None);
    }

    // --- the queue ---------------------------------------------------------------------------------

    #[test]
    fn only_the_newest_job_waits_and_one_worker_runs_at_a_time() {
        let (a, b, c) = (Config::default(), colour(Hue::Teal, 0), colour(Hue::Green, 1));
        let mut q = Queue::default();
        assert!(q.push(Job::Choice(a)), "the first push starts the worker");
        assert!(!q.push(Job::Choice(b)), "a worker is running: no second one");
        assert!(!q.push(Job::Choice(c)));
        assert_eq!(q.next(), Some(Job::Choice(c)), "b was overtaken");
        assert_eq!(q.next(), None);
        assert!(q.push(Job::Direct(1, (0, 0, 0))), "the worker ended: the next push starts a new one");
    }
}
