//! What the pause menu's Extras and Shortcuts settings do inside pocket-core: fast-forward, rewind,
//! Over the game (`over_game`: the FPS line, the sensors, Pixel Path's line), the CPU and fan profiles, run-ahead (`run_ahead`), Earlier (`timeline`) and the in-game hotkeys. `options.rs` lists the
//! choices; this turns the chosen values into behaviour.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use crate::hotkeys::{Enabled, Hotkeys};
use crate::over_game::Level;
use crate::power::{Fan, Performance, Power};
use crate::rewind::Rewind;
use crate::rumble::Strength;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FastForward {
    Times(u32),
    /// As many frames as fit into the time of one screen refresh.
    Max,
}

impl FastForward {
    pub fn parse(s: &str) -> Self {
        match s {
            "max" => Self::Max,
            s => s.parse::<u32>().ok().filter(|n| (2..=16).contains(n)).map_or(Self::Times(4), Self::Times),
        }
    }

    pub fn label(self) -> String {
        match self {
            Self::Times(n) => format!("{n}x"),
            Self::Max => "max".into(),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Settings {
    pub fast_forward: FastForward,
    pub rewind: bool,
    /// Over the game: Off, the FPS line, with the sensors, or Pixel Path's line (`over_game`).
    pub over_game: Level,
    pub rumble: Strength,
    pub performance: Performance,
    pub fan: Fan,
    pub shortcuts: Enabled,
    /// Frames to run ahead (0: off; `run_ahead`).
    pub run_ahead: u32,
    /// Earlier: a state every 5 minutes of play (`timeline`).
    pub timeline: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            fast_forward: FastForward::Times(4),
            rewind: false,
            over_game: Level::Off,
            rumble: Strength::Medium,
            performance: Performance::Auto,
            // Games: fan's start value (`perf_fan::games_fan_default`), as in options.rs.
            fan: Fan::Quiet,
            shortcuts: Enabled::default(),
            run_ahead: 0,
            timeline: true,
        }
    }
}

impl Settings {
    /// Missing or unknown values mean the defaults.
    pub fn from_values(values: &BTreeMap<String, String>) -> Self {
        let mut s = Settings::default();
        for (k, v) in values {
            s.set(k, v);
        }
        s
    }

    /// Applies one `key value` pair; false for a key that isn't ours.
    pub fn set(&mut self, key: &str, value: &str) -> bool {
        let on = value == "on";
        match key {
            "ff_speed" => self.fast_forward = FastForward::parse(value),
            "rewind" => self.rewind = on,
            crate::over_game::KEY => self.over_game = Level::parse(value),
            "rumble" => self.rumble = Strength::parse(value),
            "performance" => self.performance = Performance::parse(value),
            "fan" => self.fan = Fan::parse(value),
            "hk_save" => self.shortcuts.save = on,
            "hk_load" => self.shortcuts.load = on,
            "hk_slot" => self.shortcuts.slot = on,
            "hk_ff" => self.shortcuts.fast_forward = on,
            "hk_rewind" => self.shortcuts.rewind = on,
            crate::over_game::SHORTCUT_KEY => self.shortcuts.over_game = on,
            "hk_shot" => self.shortcuts.screenshot = on,
            // "off", "1", "2"; anything else is off.
            "run_ahead" => self.run_ahead = value.parse::<u32>().unwrap_or(0).min(crate::run_ahead::MAX_FRAMES),
            crate::timeline::KEY => self.timeline = on,
            _ => return false,
        }
        true
    }
}

/// Rewind keeps at most this much memory and this many frames (a minute at 60 fps).
const REWIND_BYTES: usize = 48 << 20;
const REWIND_FRAMES: usize = 3600;

/// Fast-forward "max" stops starting emulated frames after this long per screen refresh.
pub const MAX_FF_BUDGET_MS: f32 = 12.0;
pub const MAX_FF_RUNS: u32 = 24;

/// Running totals of the rewind captures.
#[derive(Default)]
pub struct Capture {
    pub count: u64,
    pub total_ms: f64,
    pub max_ms: f32,
}

impl Capture {
    pub fn add(&mut self, ms: f32) {
        self.count += 1;
        self.total_ms += ms as f64;
        self.max_ms = self.max_ms.max(ms);
    }
}

/// Test aid (`POCKET_CORE_REWIND_CHECK=1`): remembers a checksum of every frame shown while playing and
/// compares it with the frame that comes out when the same state is rewound to. They must be identical.
#[derive(Default)]
pub struct Verify {
    pub hashes: std::collections::VecDeque<u64>,
    pub same: u64,
    pub different: u64,
}

/// FNV-1a over the bytes of a frame.
pub fn frame_hash(data: &[u8]) -> u64 {
    data.iter().fold(0xcbf29ce484222325, |h, b| (h ^ *b as u64).wrapping_mul(0x100000001b3))
}

pub struct Extras {
    pub cfg: Settings,
    pub hotkeys: Arc<Mutex<Hotkeys>>,
    pub power: Power,
    pub rewind: Rewind,
    /// Fast-forward was switched on with its shortcut.
    pub ff_on: bool,
    /// The rewind shortcut was down on the previous frame.
    pub rewind_was_held: bool,
    /// Frames stepped back during the current rewind.
    pub rewound: u32,
    /// Scratch buffer for `retro_serialize`.
    pub scratch: Vec<u8>,
    /// Time spent capturing rewind states, for the exit report.
    pub capture: Capture,
    pub verify: Option<Verify>,
    /// Run-ahead (`run_ahead`): pocket-core's frame loop asks it each refresh.
    pub ahead: crate::run_ahead::RunAhead,
    /// What is drawn over the game now: the setting's level, moved on by HOME + Y until the setting changes.
    pub over_game: Level,
    /// pocket-ui drives the fan (Smooth fan, `fan_curve`): the game's fan choice is written here for it to follow,
    /// and the trips are left alone. None: `power.set_fan`, ROCKNIX's trips, as before.
    pub fan_request: Option<std::path::PathBuf>,
}

impl Extras {
    pub fn new(values: &BTreeMap<String, String>, power: Power) -> Self {
        let cfg = Settings::from_values(values);
        let ex = Extras {
            // What is in place before anything is applied: the system's own fan (Auto), so a Quiet game sets it.
            cfg: Settings {
                fan: Fan::Auto,
                ..Settings::default()
            },
            hotkeys: Arc::new(Mutex::new(Hotkeys::new(cfg.shortcuts))),
            power,
            rewind: Rewind::new(REWIND_BYTES, REWIND_FRAMES),
            ff_on: false,
            rewind_was_held: false,
            rewound: 0,
            scratch: Vec::new(),
            capture: Capture::default(),
            verify: std::env::var_os("POCKET_CORE_REWIND_CHECK").map(|_| Verify::default()),
            ahead: crate::run_ahead::RunAhead::new(),
            over_game: Level::Off,
            fan_request: crate::fan_curve::request_target(),
        };
        let mut ex = ex;
        ex.apply_settings(cfg);
        // Always, Auto too: pocket-ui otherwise goes by Games: fan, which this game may have overridden.
        if let Some(p) = &ex.fan_request {
            crate::fan_curve::request(p, ex.cfg.fan);
        }
        ex
    }

    /// Switches to `new`, doing only what changed.
    pub fn apply_settings(&mut self, new: Settings) {
        let old = std::mem::replace(&mut self.cfg, new);
        if new.rumble != old.rumble {
            crate::rumble::set_strength(new.rumble);
        }
        if new.performance != old.performance {
            self.power.set_performance(new.performance);
        }
        if new.fan != old.fan {
            match &self.fan_request {
                Some(p) => crate::fan_curve::request(p, new.fan),
                None => {
                    self.power.set_fan(new.fan);
                }
            }
        }
        if !new.rewind {
            self.rewind.clear();
            self.verify.iter_mut().for_each(|v| v.hashes.clear());
        }
        self.hotkeys.lock().unwrap().enabled = new.shortcuts;
        self.ahead.configure(new.run_ahead);
        if new.over_game != old.over_game {
            self.over_game = new.over_game;
        }
    }

    /// One `set <key> <value>` from the pause menu.
    pub fn set(&mut self, key: &str, value: &str) {
        let mut next = self.cfg;
        if next.set(key, value) {
            self.apply_settings(next);
        }
    }

    /// A line for the exit log: what rewind cost.
    pub fn report(&self) {
        if let Some(v) = &self.verify {
            println!("rewind check: {} frames came back identical, {} differed", v.same, v.different);
        }
        for line in [self.ahead.report_plain(), self.ahead.report()].into_iter().flatten() {
            println!("{line}");
        }
        let c = &self.capture;
        if c.count > 0 {
            println!(
                "rewind: {} captures, {:.2} ms each (max {:.2} ms), history {} frames in {:.1} MB",
                c.count,
                c.total_ms / c.count as f64,
                c.max_ms,
                self.rewind.len(),
                self.rewind.bytes() as f64 / 1048576.0
            );
        }
    }

    /// The speed to run at while fast-forward is on.
    pub fn fast_forward(&self) -> Option<FastForward> {
        self.ff_on.then_some(self.cfg.fast_forward)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::options::{GameSettings, GROUPS};
    use crate::testutil::TestDir;
    use std::path::Path;

    fn values(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    #[test]
    fn the_defaults_in_options_rs_are_the_defaults_here() {
        // A fresh install: no settings files at all.
        let dir = TestDir::new("extras-defaults");
        for system in ["gb", "gbc", "gba", "nes", "snes"] {
            let effective = GameSettings::load(&dir, system, Path::new("x.rom")).effective();
            assert_eq!(Settings::from_values(&effective), Settings::default(), "{system}");
        }
    }

    #[test]
    fn every_extras_and_shortcuts_choice_is_understood() {
        // Each choice of each setting of those two groups must change something or be the default.
        for g in GROUPS.iter().filter(|g| g.name == "Extras" || g.name == "Shortcuts") {
            for s in g.settings {
                let mut seen = Vec::new();
                for c in s.choices {
                    let mut st = Settings::default();
                    assert!(st.set(s.key, c.value), "{} is not handled", s.key);
                    seen.push(st);
                }
                assert!(seen.windows(2).any(|w| w[0] != w[1]), "{}: choices make no difference", s.key);
                let default = &s.choices[s.default];
                let mut st = Settings::default();
                st.set(s.key, default.value);
                assert_eq!(st, Settings::default(), "{}: the listed default is not the real default", s.key);
            }
        }
    }

    #[test]
    fn with_smooth_fan_the_game_tells_pocket_ui_its_fan_and_leaves_the_trips_alone() {
        let f = crate::power::fake::Fake::new("extras-curve");
        let mut ex = Extras::new(&values(&[]), f.power());
        let request = f.0.join("fan.game");
        ex.fan_request = Some(request.clone());
        ex.set("fan", "cool");
        assert_eq!(std::fs::read_to_string(&request).unwrap(), "cool");
        assert_eq!(f.trip(2), "52000", "the trips stay the system's");
        ex.set("fan", "auto");
        assert_eq!(std::fs::read_to_string(&request).unwrap(), "auto");
        // Without pocket-ui driving the fan: ROCKNIX's trips, as before.
        ex.fan_request = None;
        ex.set("fan", "cool");
        assert_eq!(f.trip(2), "40000");
        ex.power.restore();
    }

    #[test]
    fn over_the_game_follows_the_setting_and_home_y_moves_it_on_for_the_session() {
        let dir = TestDir::new("extras-over");
        for l in Level::ALL {
            assert_eq!(Settings::from_values(&values(&[("over_game", l.value())])).over_game, l);
        }
        assert_eq!(Settings::from_values(&values(&[("over_game", "junk")])).over_game, Level::Off);
        let mut ex = Extras::new(&values(&[("over_game", "pixel")]), Power::with_root(dir.to_path_buf(), dir.join("r.json")));
        assert_eq!(ex.over_game, Level::PixelPath);
        ex.over_game = ex.over_game.step(1); // HOME + Y
        assert_eq!(ex.over_game, Level::Off);
        ex.set("rewind", "on");
        assert_eq!(ex.over_game, Level::Off, "another setting leaves the shortcut's choice alone");
        ex.set("over_game", "fps");
        assert_eq!(ex.over_game, Level::Fps, "changing Over the game sets it again");
        ex.set("hk_over", "off");
        assert!(!ex.hotkeys.lock().unwrap().enabled.over_game);
        // Off: nothing at all is wanted, so pocket-core draws nothing, starts no reader and builds no report.
        let ex = Extras::new(&values(&[]), Power::with_root(dir.to_path_buf(), dir.join("r.json")));
        assert_eq!(ex.over_game, Level::Off);
        assert!(!ex.over_game.counter() && !ex.over_game.sensors() && !ex.over_game.strip());
    }

    #[test]
    fn values_parse_and_junk_falls_back() {
        let s = Settings::from_values(&values(&[("ff_speed", "max"), ("rewind", "on"), ("performance", "battery"), ("fan", "cool"), ("hk_save", "off"), ("hk_rewind", "off"), ("hk_shot", "off")]));
        assert_eq!(s.fast_forward, FastForward::Max);
        assert!(s.rewind && s.over_game == Level::Off);
        assert_eq!((s.performance, s.fan), (Performance::Battery, Fan::Cool));
        assert!(!s.shortcuts.save && s.shortcuts.load && !s.shortcuts.rewind);
        assert!(!s.shortcuts.screenshot && Settings::default().shortcuts.screenshot, "on unless switched off");
        assert_eq!(FastForward::parse("8"), FastForward::Times(8));
        for (v, want) in [("off", 0), ("1", 1), ("2", 2), ("3", 2), ("", 0), ("on", 0)] {
            assert_eq!(Settings::from_values(&values(&[("run_ahead", v)])).run_ahead, want, "{v:?}");
        }
        for junk in ["", "0", "1", "99", "fast", "-3"] {
            assert_eq!(FastForward::parse(junk), FastForward::Times(4), "{junk}");
        }
        assert!(!Settings::default().set("scaling", "aspect"), "picture keys belong to video.rs");
    }

    #[test]
    fn set_applies_hotkeys_and_clears_rewind_when_switched_off() {
        let dir = TestDir::new("extras");
        let mut ex = Extras::new(&values(&[("rewind", "on")]), Power::with_root(dir.to_path_buf(), dir.join("r.json")));
        assert!(ex.cfg.rewind);
        ex.rewind.push(&[1, 2, 3, 4]);
        ex.rewind.push(&[1, 2, 3, 5]);
        assert_eq!(ex.rewind.len(), 1);
        ex.set("hk_ff", "off");
        assert!(!ex.hotkeys.lock().unwrap().enabled.fast_forward);
        ex.set("rewind", "off");
        assert!(ex.rewind.is_empty() && ex.rewind.bytes() == 0);
        ex.set("fast_forward_typo", "x"); // ignored
        assert_eq!(ex.fast_forward(), None);
        ex.ff_on = true;
        ex.set("ff_speed", "3");
        assert_eq!(ex.fast_forward(), Some(FastForward::Times(3)));
        // Run-ahead reaches the frame loop's controller; it waits for the core to be looked at.
        ex.set("run_ahead", "2");
        assert_eq!(ex.cfg.run_ahead, 2);
        assert!(ex.ahead.needs_admit());
        ex.set("run_ahead", "off");
        assert!(!ex.ahead.needs_admit());
    }
}
