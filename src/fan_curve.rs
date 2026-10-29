//! Smooth fan: pocket-ui drives the PWM fan along a temperature curve instead of ROCKNIX's six on/off steps.
//! The design, the evidence for the mechanism and the device test are in docs/FAN-CURVE.md; in short:
//!
//! - **Mechanism.** ROCKNIX's SM8250 kernel (7.2) has only the `step_wise` and `power_allocator` thermal governors
//!   (`user_space` and `bang_bang` are not built), and the pwm-fan is a cooling device of the `cluster1` zone with six
//!   `active` trips, each mapped to one fixed cooling level. step_wise only writes the cooling device when one of its
//!   targets changes. So while pocket-ui drives the fan it moves those six `active` trips to [`BACKSTOP_TRIP`]
//!   (83.5 °C, all six: crossing it runs the fan at full speed, the kernel's own backstop) and writes `pwm1` itself.
//!   The zone stays enabled and keeps its `hot` and `critical` trips; the CPU and GPU cooling devices and their
//!   zones are never touched. Before every trip write the PWM is parked at the lowest level (`PARK_PWM`), so the
//!   kernel's evaluation that the write causes sees the fan's lowest state and has nothing to step down.
//! - **Curve.** [`Controller`]: the hottest CPU/GPU/cluster zone, smoothed (EMA, faster up than down); a
//!   piecewise-linear curve per [`Shape`] from its start temperature to full speed at [`GUARD_ON`]; hysteresis on the
//!   curve and on stopping; slew limits; a start at [`START_PWM`] (a level the fan is known to start from) settling to
//!   a lower running speed; stall detection on the tachometer with a kick and a higher running minimum.
//! - **Guard.** Any CPU/GPU zone at [`GUARD_ON`] or above: full speed at once, whatever the profile, until all are
//!   below [`GUARD_OFF`].
//! - **Hand-back.** [`NOTE_FILE`] holds what to put back, in a line format the launcher reads too
//!   (deploy/run-pocket-ui.sh `fan_handback`): the six trips, the park, and "off below the lowest trip". It exists
//!   only while pocket-ui drives the fan. Asleep, powering off, Smooth fan switched off, ROCKNIX's `custom` profile,
//!   a sensor that can't be read, or the kernel changing the fan again and again: the fan goes back to the kernel at
//!   once. pocket-ui ending or crashing: the launcher's clean-up; the controller hanging: the launcher's watchdog
//!   ([`BEAT_FILE`] stops counting).
//!
//! [`Engine`] does all of that for one moment (`tick`) on files below a root, so tests run it against a fake sysfs
//! with simulated time; [`spawn`] runs it on its own thread for pocket-ui.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use crate::logs::{dir_names, read_i64, read_trim};
use crate::power::Fan;

/// What to put back while pocket-ui drives the fan (the line format of [`Note`]). The launcher reads it too.
pub const NOTE_FILE: &str = "/tmp/pocket-ui-fan-curve.restore";
/// `<count> <unix seconds> <state> <shape> <smoothed tenths °C> <pwm>`, rewritten every tick. The launcher's watchdog
/// hands the fan back when the count stops moving while the note exists; Settings > Performance & fan shows the state.
pub const BEAT_FILE: &str = "/tmp/pocket-ui-fan.beat";
/// A pocket-core game's fan choice (`veryquiet`, `quiet`, `auto`, `cool`), written by pocket-core when pocket-ui told it to
/// ([`REQUEST_ENV`]) instead of changing the trips itself.
pub const GAME_REQUEST_FILE: &str = "/tmp/pocket-ui-fan.game";
/// Set by pocket-ui for the pocket-core games it starts while Smooth fan is on: where pocket-core writes its choice.
pub const REQUEST_ENV: &str = "POCKETUI_FAN_REQUEST";
/// The sysfs root (`/` on the device): a folder with mock files for the virtual device (docs/FAN-CURVE.md).
pub const ROOT_ENV: &str = "POCKETUI_FAN_ROOT";
/// The lowest running PWM for this fan (device tuning, docs/FAN-CURVE.md); [`MIN_RUN_PWM`] without it.
pub const MIN_PWM_ENV: &str = "POCKETUI_FAN_MIN_PWM";

/// Any CPU, GPU or cluster zone this hot (°C, raw, not smoothed): full speed at once.
pub const GUARD_ON: f32 = 78.0;
/// The guard ends when every zone is below this again.
pub const GUARD_OFF: f32 = 72.0;
/// Where the six `active` trips go while pocket-ui drives the fan (millidegrees). Above the guard, below the zone's
/// `hot` (90 °C) and the CPUs' passive trips; not a value of any ROCKNIX profile, so it is recognisably ours.
pub const BACKSTOP_TRIP: i64 = 83_500;
/// The kernel's lowest cooling level on the Retroid boards (`cooling-levels = <0 51 77 ...>`): the PWM the fan is
/// known to start from (ROCKNIX's boot patch also starts it there). Starts use it for [`START_SECS`].
pub const START_PWM: u8 = 51;
/// The running minimum before the device has been measured: below the kernel's level 1, so the curve's quiet end is
/// quieter than the old first step. Stalls (tachometer at 0) raise it; without a tachometer it is [`START_PWM`].
pub const MIN_RUN_PWM: u8 = 40;
/// Parked here around trip writes: the kernel's cooling state 1 or lower, which step_wise has nothing to step down from.
const PARK_PWM: u8 = START_PWM;
/// The running minimum never goes above this by stalls.
const MAX_MIN_PWM: u8 = 102;
const START_SECS: f32 = 2.0;
/// Smoothing time constants (s): temperatures rising are followed faster than falling ones.
const TAU_UP: f32 = 5.0;
const TAU_DOWN: f32 = 15.0;
/// PWM steps per second, up and down: 40 → 255 takes about 27 s outside the guard.
pub const SLEW_UP: f32 = 8.0;
pub const SLEW_DOWN: f32 = 4.0;
/// The curve on the way down is read this much (°C) hotter: small wobbles don't move the fan.
const HYST: f32 = 3.0;
/// The fan stops only after the smoothed temperature has been below the shape's stop temperature this long (s).
const STOP_HOLD: f32 = 45.0;
/// Tachometer at 0 this long while the fan should turn: a stall.
const STALL_SECS: f32 = 3.0;
const KICK_SECS: f32 = 2.0;
/// The kernel set the fan this many times within [`INTERVENTION_WINDOW`] s: it is handed back for [`BACKOFF_SECS`].
const MAX_INTERVENTIONS: usize = 6;
const INTERVENTION_WINDOW: f64 = 60.0;
const BACKOFF_SECS: f64 = 600.0;
/// After "no fan here" and similar: looked at again this much later.
const RETRY_SECS: f64 = 60.0;

// ---- the curves ---------------------------------------------------------------------------------------------

/// A curve. Quiet, Balanced and Cool follow ROCKNIX's quiet, moderate and aggressive profiles: they start at the
/// same temperature as the profile's first trip, but rise gradually instead of in six steps.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Shape {
    /// Off until 60 °C, the gentlest ramp (the "Very quiet" choice).
    VeryQuiet,
    Quiet,
    Balanced,
    Cool,
}

impl Shape {
    /// (°C, PWM 0-255), rising; the last point is full speed at the guard.
    pub fn points(self) -> &'static [(f32, f32)] {
        match self {
            // Starts at 60 (Quiet's second point), reaches Quiet's 100 at 74 instead of 71 and its 160 at 76.5 instead
            // of 75; full speed at the guard like the others. Never above Quiet's PWM at the same temperature.
            Shape::VeryQuiet => &[(60.0, 40.0), (66.0, 44.0), (71.0, 60.0), (74.0, 100.0), (76.5, 160.0), (78.0, 255.0)],
            Shape::Quiet => &[(52.0, 40.0), (60.0, 46.0), (66.0, 64.0), (71.0, 100.0), (75.0, 160.0), (78.0, 255.0)],
            Shape::Balanced => &[(46.0, 40.0), (55.0, 56.0), (62.0, 85.0), (68.0, 128.0), (73.0, 185.0), (78.0, 255.0)],
            Shape::Cool => &[
                (40.0, 46.0),
                (48.0, 70.0),
                (55.0, 100.0),
                (61.0, 138.0),
                (66.0, 180.0),
                (71.0, 220.0),
                (78.0, 255.0),
            ],
        }
    }

    /// The smoothed temperature the fan starts at.
    pub fn start(self) -> f32 {
        self.points()[0].0
    }

    /// Below this (smoothed, for [`STOP_HOLD`]) it stops again.
    pub fn stop(self) -> f32 {
        match self {
            Shape::VeryQuiet => 55.0,
            Shape::Quiet => 46.0,
            Shape::Balanced => 40.0,
            Shape::Cool => 35.0,
        }
    }

    /// The curve at `t` °C: the first point's PWM below the start, linear between points, full speed above.
    pub fn pwm_at(self, t: f32) -> f32 {
        let p = self.points();
        if t <= p[0].0 {
            return p[0].1;
        }
        for w in p.windows(2) {
            let ((t0, v0), (t1, v1)) = (w[0], w[1]);
            if t <= t1 {
                return v0 + (v1 - v0) * (t - t0) / (t1 - t0);
            }
        }
        255.0
    }

    pub fn name(self) -> &'static str {
        match self {
            Shape::VeryQuiet => "veryquiet",
            Shape::Quiet => "quiet",
            Shape::Balanced => "balanced",
            Shape::Cool => "cool",
        }
    }

    pub fn parse(s: &str) -> Option<Shape> {
        [Shape::VeryQuiet, Shape::Quiet, Shape::Balanced, Shape::Cool].into_iter().find(|x| x.name() == s)
    }
}

/// The curve for a fan choice: Quiet and Cool are ours; Auto follows ROCKNIX's `cooling.profile` (quiet → Quiet,
/// aggressive → Cool, moderate or unset → Balanced). ROCKNIX's `custom` profile runs its own fan loop: None, the fan
/// stays ROCKNIX's.
pub fn shape_for(fan: Fan, rocknix: &str) -> Option<Shape> {
    match fan {
        Fan::VeryQuiet => Some(Shape::VeryQuiet),
        Fan::Quiet => Some(Shape::Quiet),
        Fan::Cool => Some(Shape::Cool),
        Fan::Auto => match rocknix {
            "custom" => None,
            "quiet" => Some(Shape::Quiet),
            "aggressive" => Some(Shape::Cool),
            _ => Some(Shape::Balanced),
        },
    }
}

// ---- the controller (pure) ----------------------------------------------------------------------------------

/// What the fan is doing.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Off,
    /// Starting (a few seconds at [`START_PWM`]) or kicked after a stall.
    Start,
    Curve,
    /// Too hot: full speed.
    Guard,
}

impl Mode {
    pub fn name(self) -> &'static str {
        match self {
            Mode::Off => "off",
            Mode::Start => "start",
            Mode::Curve => "curve",
            Mode::Guard => "guard",
        }
    }
}

/// The curve with its smoothing, hysteresis, slew limits, start and stall handling. No I/O: `step` is called with the
/// time since the last step, the hottest zone's raw temperature and the tachometer.
#[derive(Clone, Debug)]
pub struct Controller {
    /// The smoothed temperature, °C.
    pub ema: Option<f32>,
    /// The PWM commanded now (before rounding).
    pub pwm: f32,
    spinning: bool,
    below_for: f32,
    guard: bool,
    /// The lowest running PWM; raised by stalls.
    pub min_run: f32,
    /// Seconds left at the start level.
    start_left: f32,
    stall_for: f32,
    /// Stalls seen.
    pub stalls: u32,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Output {
    pub pwm: u8,
    pub mode: Mode,
}

impl Controller {
    /// Takes over a fan that runs at `now_pwm`: the curve starts from there, without a jump.
    pub fn new(now_pwm: u8, min_run: u8) -> Controller {
        Controller {
            ema: None,
            pwm: now_pwm as f32,
            spinning: now_pwm > 0,
            below_for: 0.0,
            guard: false,
            min_run: min_run.clamp(1, MAX_MIN_PWM) as f32,
            start_left: 0.0,
            stall_for: 0.0,
            stalls: 0,
        }
    }

    pub fn step(&mut self, dt: f32, hottest: f32, rpm: Option<i64>, shape: Shape) -> Output {
        let dt = dt.clamp(0.0, 5.0);
        let ema = match self.ema {
            None => hottest,
            Some(e) => {
                let tau = if hottest > e { TAU_UP } else { TAU_DOWN };
                e + (1.0 - (-dt / tau).exp()) * (hottest - e)
            }
        };
        self.ema = Some(ema);

        // The guard reads the raw temperature: no smoothing, no slew.
        if hottest >= GUARD_ON {
            self.guard = true;
        } else if hottest < GUARD_OFF {
            self.guard = false;
        }
        if self.guard {
            self.pwm = 255.0;
            self.spinning = true;
            self.below_for = 0.0;
            self.start_left = 0.0;
            self.stall_for = 0.0;
            return Output { pwm: 255, mode: Mode::Guard };
        }

        // On at the start temperature; off only after a while below the (lower) stop temperature.
        if ema >= shape.start() {
            self.spinning = true;
            self.below_for = 0.0;
        } else if self.spinning && ema < shape.stop() {
            self.below_for += dt;
            if self.below_for >= STOP_HOLD {
                self.spinning = false;
                self.below_for = 0.0;
            }
        } else {
            self.below_for = 0.0;
        }

        // The target, with a hysteresis band: up along the curve, down along the curve read HYST hotter.
        let target = if self.spinning {
            let up = shape.pwm_at(ema).max(self.min_run);
            let down = shape.pwm_at(ema + HYST).max(self.min_run);
            if up > self.pwm {
                up
            } else if down < self.pwm {
                down
            } else {
                self.pwm
            }
        } else {
            0.0
        };

        let mut mode = Mode::Curve;
        if target > 0.0 && self.pwm < 1.0 {
            // A start from standstill: a level the fan surely starts from, then down to the curve.
            self.pwm = target.max(START_PWM as f32);
            self.start_left = START_SECS;
        } else if self.start_left > 0.0 {
            self.start_left -= dt;
            mode = Mode::Start;
            self.pwm = self.pwm.max(target);
        } else if target > self.pwm {
            self.pwm = target.min(self.pwm + SLEW_UP * dt);
        } else if target == 0.0 {
            // Down to the running minimum, then stop.
            self.pwm = (self.pwm - SLEW_DOWN * dt).max(self.min_run);
            if self.pwm <= self.min_run + 0.01 {
                self.pwm = 0.0;
            }
        } else {
            self.pwm = target.max(self.pwm - SLEW_DOWN * dt);
        }
        if self.start_left > 0.0 {
            mode = Mode::Start;
        }

        // A stall: the tachometer says 0 while the fan should turn (not while starting).
        match rpm {
            Some(0) if self.pwm >= 1.0 && self.start_left <= 0.0 => {
                self.stall_for += dt;
                if self.stall_for >= STALL_SECS {
                    self.stall_for = 0.0;
                    self.stalls += 1;
                    self.min_run = (self.min_run + 6.0).min(MAX_MIN_PWM as f32);
                    self.pwm = self.pwm.max(self.min_run).max(START_PWM as f32 + 20.0 * (self.stalls.min(5) - 1) as f32);
                    self.start_left = KICK_SECS;
                    mode = Mode::Start;
                }
            }
            _ => self.stall_for = 0.0,
        }
        if self.pwm < 1.0 {
            mode = Mode::Off;
        }
        Output {
            pwm: self.pwm.round().clamp(0.0, 255.0) as u8,
            mode,
        }
    }
}

// ---- the note: what goes back ---------------------------------------------------------------------------------

/// What [`NOTE_FILE`] holds, one instruction per line, in the order the hand-back carries them out:
///
/// ```text
/// # pocket-ui smooth fan: what to put back (src/fan_curve.rs, deploy/run-pocket-ui.sh)
/// park /sys/class/hwmon/hwmon3/pwm1 51
/// trip /sys/class/thermal/thermal_zone6/trip_point_2_temp 52000
/// ...
/// off-below /sys/class/thermal/thermal_zone6/temp 52000 /sys/class/hwmon/hwmon3/pwm1
/// ```
///
/// `park`: lower the PWM to that if it is higher. `trip`: the trip's own temperature back (the kernel then sets the
/// level its trips ask for). `off-below`: if the zone is cooler than the lowest trip, the kernel wants the fan off.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Note {
    pub pwm: PathBuf,
    pub trips: Vec<(PathBuf, i64)>,
    pub zone_temp: PathBuf,
}

impl Note {
    pub fn text(&self) -> String {
        let mut s = String::from("# pocket-ui smooth fan: what to put back (src/fan_curve.rs, deploy/run-pocket-ui.sh)\n");
        s.push_str(&format!("park {} {PARK_PWM}\n", self.pwm.display()));
        for (p, t) in &self.trips {
            s.push_str(&format!("trip {} {t}\n", p.display()));
        }
        let lowest = self.trips.iter().map(|(_, t)| *t).min().unwrap_or(0);
        s.push_str(&format!("off-below {} {lowest} {}\n", self.zone_temp.display(), self.pwm.display()));
        s
    }

    /// Reads a note; only paths below `root/sys` count, as in the launcher.
    pub fn parse(text: &str, root: &Path) -> Option<Note> {
        let sys = root.join("sys");
        let ok = |p: &str| {
            let p = PathBuf::from(p);
            (p.starts_with(&sys) && !p.components().any(|c| c == std::path::Component::ParentDir)).then_some(p)
        };
        let mut n = Note::default();
        for line in text.lines() {
            let w: Vec<&str> = line.split_whitespace().collect();
            match w.as_slice() {
                ["park", p, _] => n.pwm = ok(p)?,
                ["trip", p, t] => n.trips.push((ok(p)?, t.parse().ok()?)),
                ["off-below", z, _, _] => n.zone_temp = ok(z)?,
                _ => {}
            }
        }
        (!n.trips.is_empty()).then_some(n)
    }
}

fn write(p: &Path, v: impl std::fmt::Display) -> bool {
    std::fs::write(p, v.to_string()).is_ok()
}

/// Writes `text` to `path` through a temporary file and a rename: a reader sees the old or the new content.
fn replace(path: &Path, text: &str) -> bool {
    let tmp = path.with_extension(format!("tmp{}", std::process::id()));
    if std::fs::write(&tmp, text).is_err() {
        return false;
    }
    std::fs::rename(&tmp, path).is_ok()
}

/// Gives the fan back to the kernel as the note says, and deletes the note. False when there was no note.
pub fn hand_back(note_file: &Path, root: &Path) -> bool {
    if crate::device::rocknix_cooling(root) {
        return false;
    }
    let Some(text) = std::fs::read_to_string(note_file).ok() else { return false };
    if let Some(n) = Note::parse(&text, root) {
        if read_i64(&n.pwm).is_some_and(|v| v > PARK_PWM as i64) {
            write(&n.pwm, PARK_PWM);
        }
        for (p, t) in &n.trips {
            write(p, t);
        }
        let lowest = n.trips.iter().map(|(_, t)| *t).min().unwrap_or(i64::MIN);
        if read_i64(&n.zone_temp).is_some_and(|t| t < lowest) {
            write(&n.pwm, 0);
        }
    }
    let _ = std::fs::remove_file(note_file);
    true
}

// ---- the hardware -------------------------------------------------------------------------------------------

/// The fan and the sensors below a root, found once per take-over.
#[derive(Clone, Debug)]
struct Hw {
    pwm: PathBuf,
    rpm: Option<PathBuf>,
    zone: PathBuf,
    trips: Vec<PathBuf>,
    /// `temp` of every CPU, GPU and cluster zone (and the fan's zone).
    sensors: Vec<PathBuf>,
}

fn discover(root: &Path) -> Result<Hw, &'static str> {
    let hwmon = root.join("sys/class/hwmon");
    let fan = dir_names(&hwmon, "hwmon")
        .into_iter()
        .map(|h| hwmon.join(h))
        .find(|d| read_trim(&d.join("name")).as_deref() == Some("pwmfan") && d.join("pwm1").exists())
        .ok_or("no PWM fan")?;
    if read_trim(&fan.join("pwm1_enable")).as_deref() == Some("0") {
        return Err("the fan is switched off (pwm1_enable 0)");
    }
    let zone = crate::power::fan_zone(root).ok_or("no thermal zone drives the fan")?;
    if read_trim(&zone.join("mode")).as_deref() == Some("disabled") {
        return Err("ROCKNIX's own fan loop runs (its zone is disabled)");
    }
    let trips: Vec<PathBuf> = crate::power::Power::active_trips(&zone).into_iter().map(|(f, _)| zone.join(f)).collect();
    if trips.is_empty() {
        return Err("the fan's zone has no active trips");
    }
    let thermal = root.join("sys/class/thermal");
    let mut sensors: Vec<PathBuf> = dir_names(&thermal, "thermal_zone")
        .into_iter()
        .map(|z| thermal.join(z))
        .filter(|z| read_trim(&z.join("type")).is_some_and(|t| t.starts_with("cpu") || t.starts_with("gpu") || t.starts_with("cluster")))
        .map(|z| z.join("temp"))
        .collect();
    if !sensors.contains(&zone.join("temp")) {
        sensors.push(zone.join("temp"));
    }
    let rpm = Some(fan.join("fan1_input")).filter(|p| p.exists());
    Ok(Hw {
        pwm: fan.join("pwm1"),
        rpm,
        zone,
        trips,
        sensors,
    })
}

/// ROCKNIX's trips for its profiles (bin/fancontrol), for a take-over that finds our own backstop there without a note
/// (a crash whose note was lost): what the system would have.
fn rocknix_trips(profile: &str) -> [i64; 6] {
    match profile {
        "quiet" => [52000, 62000, 69000, 75000, 80000, 85000],
        "aggressive" => [40000, 48000, 55000, 61000, 66000, 71000],
        _ => [45000, 55000, 62000, 68000, 73000, 78000],
    }
}

// ---- the engine ---------------------------------------------------------------------------------------------

/// What pocket-ui wants now (the App's side, `app_perf_fan.rs`).
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Plan {
    /// Smooth fan is on and the Pocket is awake: drive the fan. False: the kernel's, at once.
    pub on: bool,
    /// `pocket-core --restore-system` is putting back a killed game's trips: no take-over before it is done.
    pub settling: bool,
    /// The choice for now: Fan outside games on the shelf, Games: fan in a game.
    pub fan: Fan,
    pub game: Game,
    /// Counts the games started, so a new game is told from the last one.
    pub game_id: u64,
    /// `fan` is the running game's own choice (a PortMaster port's Performance & fan, `port_perf`): it wins over a
    /// cooling profile ROCKNIX's runemu.sh switched to for the game.
    pub own: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Game {
    #[default]
    None,
    /// A pocket-core game: its own choice in [`GAME_REQUEST_FILE`], else the plan's.
    Hosted,
    /// RetroArch, a standalone emulator, a PortMaster port (runemu.sh).
    Other,
}

/// Where the engine reads and writes.
#[derive(Clone, Debug)]
pub struct Paths {
    pub root: PathBuf,
    pub note: PathBuf,
    pub beat: PathBuf,
    pub request: PathBuf,
    pub system_cfg: PathBuf,
}

/// The sysfs root: [`ROOT_ENV`], else `/`.
pub fn sysfs_root() -> PathBuf {
    std::env::var_os(ROOT_ENV).map_or_else(|| PathBuf::from("/"), PathBuf::from)
}

impl Paths {
    pub fn system() -> Paths {
        Paths {
            root: sysfs_root(),
            note: NOTE_FILE.into(),
            beat: BEAT_FILE.into(),
            request: GAME_REQUEST_FILE.into(),
            system_cfg: crate::system_cfg::PATH.into(),
        }
    }

    /// Everything below `dir` (tests).
    pub fn under(dir: &Path) -> Paths {
        Paths {
            root: dir.to_path_buf(),
            note: dir.join("tmp/fan.restore"),
            beat: dir.join("tmp/fan.beat"),
            request: dir.join("tmp/fan.game"),
            system_cfg: dir.join("system.cfg"),
        }
    }
}

/// What the engine is doing, for the log, the beat and the page.
#[derive(Clone, Debug, PartialEq)]
pub enum Status {
    /// The kernel drives the fan (ROCKNIX's profile), and why.
    System(&'static str),
    /// It can't be driven here (no fan, ROCKNIX's custom loop, the kernel kept changing it).
    Unavailable(String),
    Driving {
        shape: Shape,
        mode: Mode,
        ema: f32,
        pwm: u8,
    },
}

pub struct Engine {
    rocknix_cooling: bool,
    paths: Paths,
    hw: Option<Hw>,
    ctl: Option<Controller>,
    note: Note,
    /// What we wrote last; None right after a trip write (the kernel may have reacted to it).
    written: Option<u8>,
    interventions: VecDeque<f64>,
    /// Trips someone else wrote while we drove the fan, seen once: noted when they read the same twice.
    foreign: Option<Vec<i64>>,
    retry_at: f64,
    last_tick: Option<f64>,
    recovered: bool,
    /// (game_id, ROCKNIX's cooling.profile when that game started) for runemu games.
    baseline: Option<(u64, String)>,
    cfg_cache: Option<(std::time::SystemTime, u64, String)>,
    min_run: u8,
    seq: u64,
    pub status: Status,
}

impl Engine {
    pub fn new(paths: Paths) -> Engine {
        let min_run = std::env::var(MIN_PWM_ENV).ok().and_then(|v| v.trim().parse::<u8>().ok()).unwrap_or_else(|| crate::device::current().fan_min_run());
        Engine::with_min_run(paths, min_run)
    }

    pub fn with_min_run(paths: Paths, min_run: u8) -> Engine {
        Engine {
            rocknix_cooling: crate::device::rocknix_cooling(&paths.root),
            paths,
            hw: None,
            ctl: None,
            note: Note::default(),
            written: None,
            interventions: VecDeque::new(),
            foreign: None,
            retry_at: f64::NEG_INFINITY,
            last_tick: None,
            recovered: false,
            baseline: None,
            cfg_cache: None,
            min_run,
            seq: 0,
            status: Status::System("starting"),
        }
    }

    /// Whether pocket-ui drives the fan now (the trips are ours and the note exists).
    pub fn driving(&self) -> bool {
        self.ctl.is_some()
    }

    pub fn controller(&self) -> Option<&Controller> {
        self.ctl.as_ref()
    }

    /// ROCKNIX's `cooling.profile`, read again only when system.cfg changed.
    fn cooling_profile(&mut self) -> String {
        let meta = std::fs::metadata(&self.paths.system_cfg).ok();
        let key = meta.as_ref().and_then(|m| Some((m.modified().ok()?, m.len())));
        if let (Some((t, len)), Some((ct, clen, v))) = (key, &self.cfg_cache) {
            if t == *ct && len == *clen {
                return v.clone();
            }
        }
        let text = std::fs::read_to_string(&self.paths.system_cfg).unwrap_or_default();
        // In the order ROCKNIX's get_setting looks (001-functions): system., the key, global.
        let v = ["system.cooling.profile", "cooling.profile", "global.cooling.profile"]
            .iter()
            .find_map(|k| crate::system_cfg::value(&text, k).map(str::trim).filter(|v| !v.is_empty()))
            .unwrap_or("")
            .to_string();
        self.cfg_cache = key.map(|(t, len)| (t, len, v.clone()));
        v
    }

    /// The curve for this moment, or None (ROCKNIX's custom profile).
    fn shape(&mut self, plan: &Plan) -> Option<Shape> {
        let rocknix = self.cooling_profile();
        let fan = match plan.game {
            Game::Hosted => read_trim(&self.paths.request).map_or(plan.fan, |v| Fan::parse(&v)),
            _ => plan.fan,
        };
        if plan.game == Game::Other && !plan.own {
            // runemu.sh switches cooling.profile for a game that has its own in ROCKNIX (and back at its end):
            // that choice for this game wins over Games: fan, like a game's own choice in the pause menu.
            if self.baseline.as_ref().is_none_or(|(id, _)| *id != plan.game_id) {
                self.baseline = Some((plan.game_id, rocknix.clone()));
            }
            if self.baseline.as_ref().is_some_and(|(_, at_start)| *at_start != rocknix) {
                return shape_for(Fan::Auto, &rocknix);
            }
        }
        shape_for(fan, &rocknix)
    }

    /// One moment: takes over, drives, or hands back as `plan` says. `now`: seconds on any steady clock. Returns how
    /// long until the next tick should be.
    pub fn tick(&mut self, now: f64, plan: &Plan) -> Duration {
        if self.rocknix_cooling {
            return Duration::from_secs(5);
        }
        let dt = self.last_tick.map_or(0.5, |t| (now - t).max(0.0)) as f32;
        self.last_tick = Some(now);
        let next = self.decide(now, dt, plan);
        self.beat();
        next
    }

    fn decide(&mut self, now: f64, dt: f32, plan: &Plan) -> Duration {
        let idle = Duration::from_millis(1000);
        // Not driving and nothing to do soon: a plan change wakes the thread at once anyway.
        let resting = Duration::from_secs(5);
        if !self.recovered && !plan.settling {
            // A pocket-ui that crashed without the launcher's clean-up (started by hand) left its note.
            self.recovered = true;
            if self.ctl.is_none() && hand_back(&self.paths.note, &self.paths.root) {
                println!("fan curve: put back what a previous pocket-ui left behind");
            }
        }
        if !plan.on {
            self.release("asleep, or Smooth fan off");
            return resting;
        }
        if self.ctl.is_none() && plan.settling {
            return Duration::from_millis(250);
        }
        let Some(shape) = self.shape(plan) else {
            self.release("ROCKNIX's custom fan profile");
            return Duration::from_secs(2);
        };
        if self.ctl.is_none() {
            if now < self.retry_at {
                return resting;
            }
            if let Err(why) = self.take_over() {
                self.unavailable(why.to_string(), now, RETRY_SECS);
                return resting;
            }
        }
        if !self.trips_still_ours() {
            return Duration::from_millis(500);
        }
        let hw = self.hw.clone().expect("taken over");
        let Some(hottest) = hw.sensors.iter().filter_map(|p| read_i64(p)).max() else {
            self.release("no temperature to go by");
            self.retry_at = now + RETRY_SECS;
            return idle;
        };
        let rpm = hw.rpm.as_ref().and_then(|p| read_i64(p));
        // Someone else set the PWM since our last write (the kernel stepping its cooling device).
        if let (Some(w), Some(now_pwm)) = (self.written, read_i64(&hw.pwm)) {
            if (now_pwm - w as i64).abs() > 2 {
                self.interventions.push_back(now);
                println!(
                    "fan curve: the PWM was {now_pwm}, not {w}: the kernel set the fan ({} in the last minute)",
                    self.interventions.len()
                );
            }
        }
        while self.interventions.front().is_some_and(|t| now - t > INTERVENTION_WINDOW) {
            self.interventions.pop_front();
        }
        if self.interventions.len() >= MAX_INTERVENTIONS {
            self.release("the kernel kept setting the fan");
            self.unavailable("the kernel kept setting the fan".into(), now, BACKOFF_SECS);
            self.interventions.clear();
            return idle;
        }
        let ctl = self.ctl.as_mut().expect("taken over");
        let out = ctl.step(dt, hottest as f32 / 1000.0, rpm, shape);
        let ema = ctl.ema.unwrap_or(0.0);
        let stalls = ctl.stalls;
        if self.written != Some(out.pwm) {
            write(&hw.pwm, out.pwm);
            self.written = Some(out.pwm);
        }
        let was = std::mem::replace(
            &mut self.status,
            Status::Driving {
                shape,
                mode: out.mode,
                ema,
                pwm: out.pwm,
            },
        );
        if !matches!(was, Status::Driving { shape: s, mode: m, .. } if s == shape && m == out.mode) {
            println!(
                "fan curve: {} {} at {ema:.1} °C, PWM {}{}",
                shape.name(),
                out.mode.name(),
                out.pwm,
                if stalls > 0 { format!(" ({stalls} stalls)") } else { String::new() }
            );
        }
        if out.mode == Mode::Off && ema < shape.stop() {
            idle
        } else {
            Duration::from_millis(500)
        }
    }

    fn unavailable(&mut self, why: String, now: f64, again: f64) {
        if self.status != Status::Unavailable(why.clone()) {
            println!("fan curve: not here: {why}; the system's fan control stays");
        }
        self.status = Status::Unavailable(why);
        self.retry_at = now + again;
    }

    /// Notes the trips and moves them to the backstop. The beat is written first, so the watchdog sees life before
    /// the note appears.
    fn take_over(&mut self) -> Result<(), &'static str> {
        let hw = discover(&self.paths.root)?;
        let found: Vec<i64> = hw
            .trips
            .iter()
            .map(|p| read_i64(p))
            .collect::<Option<_>>()
            .ok_or("the fan's trips can't be read")?;
        let profile = self.cooling_profile();
        let rocknix = rocknix_trips(&profile);
        let trips: Vec<(PathBuf, i64)> = hw
            .trips
            .iter()
            .zip(&found)
            .enumerate()
            .map(|(i, (p, t))| {
                (
                    p.clone(),
                    if *t == BACKSTOP_TRIP {
                        rocknix.get(i).copied().unwrap_or(rocknix[5])
                    } else {
                        *t
                    },
                )
            })
            .collect();
        let start = read_i64(&hw.pwm).unwrap_or(0).clamp(0, 255) as u8;
        self.note = Note {
            pwm: hw.pwm.clone(),
            trips,
            zone_temp: hw.zone.join("temp"),
        };
        self.seq += 1;
        self.beat();
        if let Some(d) = self.paths.note.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        if !replace(&self.paths.note, &self.note.text()) {
            return Err("can't write the note in /tmp");
        }
        if !self.raise(&hw, start) {
            hand_back(&self.paths.note, &self.paths.root);
            return Err("the fan's trips can't be changed");
        }
        let min_run = if hw.rpm.is_some() { self.min_run } else { self.min_run.max(START_PWM) };
        println!(
            "fan curve: driving the fan ({} trips of {} at {} °C, were {:?}; running minimum PWM {min_run})",
            hw.trips.len(),
            hw.zone.file_name().unwrap_or_default().to_string_lossy(),
            BACKSTOP_TRIP as f64 / 1000.0,
            self.note.trips.iter().map(|(_, t)| t / 1000).collect::<Vec<_>>()
        );
        self.ctl = Some(Controller::new(start, min_run));
        self.hw = Some(hw);
        self.foreign = None;
        self.interventions.clear();
        Ok(())
    }

    /// Parks the PWM, moves every active trip to the backstop, and puts the PWM back. True when all read back.
    fn raise(&mut self, hw: &Hw, pwm: u8) -> bool {
        write(&hw.pwm, pwm.min(PARK_PWM));
        let mut ok = true;
        for p in &hw.trips {
            ok &= write(p, BACKSTOP_TRIP) && read_i64(p) == Some(BACKSTOP_TRIP);
        }
        write(&hw.pwm, pwm);
        // The kernel's own reaction to the trips can't be told from a step of ours on the next tick.
        self.written = None;
        ok
    }

    /// Our trips still in place? If something else wrote them (runemu.sh restarting ROCKNIX's fancontrol at the start
    /// or end of a game), its values are the system's now: noted once they read the same twice, and ours again.
    fn trips_still_ours(&mut self) -> bool {
        let hw = self.hw.clone().expect("taken over");
        let now: Vec<i64> = hw.trips.iter().map(|p| read_i64(p).unwrap_or(BACKSTOP_TRIP)).collect();
        if now.iter().all(|t| *t == BACKSTOP_TRIP) {
            self.foreign = None;
            return true;
        }
        if self.foreign.as_ref() != Some(&now) {
            self.foreign = Some(now);
            self.written = None;
            return false;
        }
        for ((_, t), new) in self.note.trips.iter_mut().zip(&now) {
            if *new != BACKSTOP_TRIP {
                *t = *new;
            }
        }
        let pwm = self.ctl.as_ref().map_or(0, |c| c.pwm.round() as u8);
        let noted = replace(&self.paths.note, &self.note.text());
        if !noted || !self.raise(&hw, pwm) {
            self.release("the fan's trips can't be changed");
            return false;
        }
        println!(
            "fan curve: the system set its fan trips again ({:?} °C): noted, the curve goes on",
            now.iter().map(|t| t / 1000).collect::<Vec<_>>()
        );
        self.foreign = None;
        true
    }

    /// Hands the fan back to the kernel, if it is ours.
    pub fn release(&mut self, why: &'static str) {
        if self.ctl.take().is_some() {
            hand_back(&self.paths.note, &self.paths.root);
            println!("fan curve: the system's fan control again ({why})");
        }
        self.hw = None;
        self.written = None;
        self.foreign = None;
        self.status = Status::System(why);
    }

    /// The beat file. If it can't be written the watchdog would hand the fan back in a loop: hand it back now.
    fn beat(&mut self) {
        self.seq += 1;
        let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
        let line = match &self.status {
            Status::Driving { shape, mode, ema, pwm } => {
                format!("{} {secs} {} {} {} {pwm}\n", self.seq, mode.name(), shape.name(), (ema * 10.0).round() as i64)
            }
            Status::System(_) => format!("{} {secs} system - - -\n", self.seq),
            Status::Unavailable(_) => format!("{} {secs} unavailable - - -\n", self.seq),
        };
        if let Some(d) = self.paths.beat.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        if !replace(&self.paths.beat, &line) && self.ctl.is_some() {
            self.release("can't write the heartbeat");
        }
    }
}

// ---- the beat, as the page reads it ----------------------------------------------------------------------------

/// The engine's last beat.
#[derive(Clone, Debug, PartialEq)]
pub struct Beat {
    pub driving: bool,
    pub mode: Option<Mode>,
    pub shape: Option<Shape>,
    /// Smoothed temperature, millidegrees.
    pub ema: Option<i64>,
}

impl Beat {
    /// The beat in `path`, if it was written within the last 12 s of `now_secs` (unix time; a resting engine beats
    /// every 5 s).
    pub fn read(path: &Path, now_secs: u64) -> Option<Beat> {
        let text = read_trim(path)?;
        let w: Vec<&str> = text.split_whitespace().collect();
        let [_, at, state, shape, ema, _] = w.as_slice() else { return None };
        let at: u64 = at.parse().ok()?;
        if now_secs.abs_diff(at) > 12 {
            return None;
        }
        let mode = [Mode::Off, Mode::Start, Mode::Curve, Mode::Guard].into_iter().find(|m| m.name() == *state);
        Some(Beat {
            driving: mode.is_some(),
            mode,
            shape: Shape::parse(shape),
            ema: ema.parse::<i64>().ok().map(|t| t * 100),
        })
    }

    pub fn now(path: &Path) -> Option<Beat> {
        let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
        Beat::read(path, secs)
    }
}

// ---- pocket-core's side ---------------------------------------------------------------------------------------

/// Where this pocket-core writes its fan choice, if pocket-ui drives the fan ([`REQUEST_ENV`]).
pub fn request_target() -> Option<PathBuf> {
    std::env::var_os(REQUEST_ENV).filter(|v| !v.is_empty()).map(PathBuf::from)
}

/// pocket-core's fan choice for its game, for the engine to follow.
pub fn request(path: &Path, fan: Fan) {
    if !replace(path, fan.value()) {
        eprintln!("fan curve: can't write {}", path.display());
    }
}

// ---- the thread ---------------------------------------------------------------------------------------------

struct Shared {
    plan: Mutex<(Plan, bool)>,
    wake: Condvar,
    driving: AtomicBool,
    /// Held for each tick, so a hand-back can wait for a take-over under way.
    ticking: Mutex<()>,
    /// The handle is gone: the thread hands back and ends.
    quit: AtomicBool,
}

/// pocket-ui's handle on the engine's thread.
pub struct Handle {
    shared: Arc<Shared>,
    paths: Paths,
}

/// Starts the engine on its own thread. It reads a dozen small sysfs files every half second while the fan turns
/// or the Pocket is warm, once a second otherwise, and nothing of the UI thread's.
pub fn spawn(paths: Paths) -> Handle {
    let shared = Arc::new(Shared {
        plan: Mutex::new((Plan::default(), false)),
        wake: Condvar::new(),
        driving: AtomicBool::new(false),
        ticking: Mutex::new(()),
        quit: AtomicBool::new(false),
    });
    let (s, p) = (shared.clone(), paths.clone());
    std::thread::Builder::new()
        .name("fan-curve".into())
        .spawn(move || {
            let mut engine = Engine::new(p);
            let t0 = Instant::now();
            loop {
                if s.quit.load(Ordering::Acquire) {
                    engine.release("pocket-ui ends");
                    s.driving.store(false, Ordering::Release);
                    return;
                }
                let next = {
                    let _tick = s.ticking.lock();
                    let plan = s.plan.lock().map(|g| g.0).unwrap_or_default();
                    let next = engine.tick(t0.elapsed().as_secs_f64(), &plan);
                    s.driving.store(engine.driving(), Ordering::Release);
                    next
                };
                let Ok(g) = s.plan.lock() else { return };
                if let Ok((mut g, _)) = s.wake.wait_timeout_while(g, next, |(_, changed)| !*changed) {
                    g.1 = false;
                }
            }
        })
        .expect("fan curve thread");
    Handle { shared, paths }
}

impl Handle {
    /// The plan for now; the thread wakes at once when it changed.
    pub fn set(&self, plan: Plan) {
        if let Ok(mut g) = self.shared.plan.lock() {
            if g.0 != plan {
                *g = (plan, true);
                self.shared.wake.notify_all();
            }
        }
    }

    pub fn driving(&self) -> bool {
        self.shared.driving.load(Ordering::Acquire)
    }

    /// Hands the fan back and waits for it (pocket-ui ending, Smooth fan switched off before the old trip profiles take
    /// over). If the thread doesn't answer within `timeout`, the note is carried out here.
    pub fn release_and_wait(&self, timeout: Duration) {
        let was_on = self.shared.plan.lock().map_or(true, |mut g| {
            let on = g.0.on;
            if on {
                g.0.on = false;
                g.1 = true;
                self.shared.wake.notify_all();
            }
            on
        });
        // Already told and handed back: nothing to wait for (this runs every frame while Smooth fan is off).
        if !was_on && !self.driving() {
            return;
        }
        let until = Instant::now() + timeout;
        // A tick under way (perhaps a take-over) ends first; the next one sees the plan and hands back.
        while Instant::now() < until {
            match self.shared.ticking.try_lock() {
                Ok(_) => break,
                Err(_) => std::thread::sleep(Duration::from_millis(2)),
            }
        }
        while self.driving() && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(5));
        }
        if self.driving() && hand_back(&self.paths.note, &self.paths.root) {
            println!("fan curve: the thread didn't answer, handed back from the note");
        }
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        self.shared.quit.store(true, Ordering::Release);
        if let Ok(mut g) = self.shared.plan.lock() {
            g.1 = true;
        }
        self.shared.wake.notify_all();
    }
}

// ---- simulation ---------------------------------------------------------------------------------------------

/// A thermal model of the Mini V2 for comparing the old trip steps with the curve (docs/FAN-CURVE.md, "Simulation";
/// `cargo run --release --example fan_curve_sim`). Not a measurement: a heatsink with one time constant, a fan whose
/// cooling grows with its duty, the hottest core a few degrees above the heatsink with load spikes and sensor noise,
/// and the cluster sensor the kernel goes by close to the heatsink. Tuned so the old behaviour matches what was seen
/// on the device with the Deltarune port: about 56-58 °C on the prime core, the fan going off and on.
pub mod sim {
    use super::*;

    /// One sample, every [`DT`] seconds.
    #[derive(Clone, Copy, Debug, Default)]
    pub struct Sample {
        pub t: f32,
        /// The hottest core the curve reads, °C.
        pub hottest: f32,
        /// The cluster zone the kernel's trips read, °C.
        pub cluster: f32,
        pub pwm: u8,
    }

    pub const DT: f32 = 0.1;
    const AMBIENT: f32 = 25.0;
    /// Heat in (°C/s at the heatsink), passive and full-fan conductance (1/s).
    const HEAT: f32 = 1.867;
    const G_PASSIVE: f32 = 0.0667;
    const G_FAN: f32 = 0.158;

    /// A small deterministic noise source.
    struct Lcg(u64);
    impl Lcg {
        fn next(&mut self) -> f32 {
            self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            ((self.0 >> 33) as f32 / (1u64 << 31) as f32) * 2.0 - 1.0
        }
    }

    /// The load: steady play with a spike (a scene change, a battle) of a few seconds every 35-55 s.
    fn load(t: f32) -> f32 {
        let period = 45.0;
        let phase = t % period;
        let n = (t / period) as u32;
        let len = 3.0 + (n % 3) as f32;
        if phase > 20.0 && phase < 20.0 + len {
            1.35
        } else {
            1.0
        }
    }

    /// ROCKNIX's kernel: step_wise on the cluster zone with six active trips (5 °C hysteresis), each mapped to one
    /// cooling level of `cooling-levels = <0 51 77 102 128 153 179 204 255>`.
    pub struct Kernel {
        trips: [f32; 6],
        on: [bool; 6],
    }
    const LEVEL_OF_TRIP: [usize; 6] = [1, 2, 3, 4, 6, 8];
    const LEVELS: [u8; 9] = [0, 51, 77, 102, 128, 153, 179, 204, 255];

    impl Kernel {
        pub fn quiet() -> Kernel {
            Kernel {
                trips: [52.0, 62.0, 69.0, 75.0, 80.0, 85.0],
                on: [false; 6],
            }
        }

        pub fn pwm(&mut self, cluster: f32) -> u8 {
            let mut state = 0;
            for ((trip, on), level) in self.trips.iter().zip(self.on.iter_mut()).zip(LEVEL_OF_TRIP) {
                if cluster > *trip {
                    *on = true;
                } else if cluster < trip - 5.0 {
                    *on = false;
                }
                if *on {
                    state = state.max(level);
                }
            }
            LEVELS[state]
        }
    }

    /// Who drives the fan in a run.
    pub enum Driver {
        Kernel(Kernel),
        /// The curve, stepped every half second like the engine.
        Curve(Controller, Shape),
    }

    /// `secs` of the Deltarune case: the heatsink starts warm (45 °C), as after a few minutes of play.
    pub fn run(mut driver: Driver, secs: f32, seed: u64) -> Vec<Sample> {
        let mut rng = Lcg(seed);
        let mut heatsink = 45.0f32;
        let mut pwm = 0u8;
        let mut since_tick = 0.5f32;
        let mut out = Vec::new();
        let mut t = 0.0f32;
        while t < secs {
            let l = load(t);
            let s = pwm as f32 / 255.0;
            heatsink += DT * (HEAT * l - (G_PASSIVE + G_FAN * s) * (heatsink - AMBIENT));
            let hottest = heatsink + 4.0 + 6.0 * (l - 1.0) / 0.35 * 0.5 + 0.6 * rng.next();
            let cluster = heatsink + 1.0 * (l - 1.0) / 0.35 + 0.3 * rng.next();
            match &mut driver {
                Driver::Kernel(k) => pwm = k.pwm(cluster),
                Driver::Curve(c, shape) => {
                    since_tick += DT;
                    if since_tick >= 0.5 - 1e-4 {
                        pwm = c.step(since_tick, hottest, Some(if pwm > 0 { 2000 } else { 0 }), *shape).pwm;
                        since_tick = 0.0;
                    }
                }
            }
            out.push(Sample { t, hottest, cluster, pwm });
            t += DT;
        }
        out
    }

    /// The numbers the report compares.
    #[derive(Clone, Copy, Debug, Default, PartialEq)]
    pub struct Summary {
        pub starts: u32,
        pub stops: u32,
        /// The biggest change of the PWM within one second.
        pub max_step_per_s: u8,
        /// Share of the time with the fan off.
        pub off_share: f32,
        pub mean_pwm: f32,
        pub max_pwm: u8,
        pub mean_hottest: f32,
        pub max_hottest: f32,
    }

    /// Over the samples from `from` seconds on (after the start-up).
    pub fn summary(s: &[Sample], from: f32) -> Summary {
        let s: Vec<&Sample> = s.iter().filter(|x| x.t >= from).collect();
        let n = s.len().max(1) as f32;
        let mut r = Summary::default();
        let per_s = (1.0 / DT).round() as usize;
        for w in s.windows(2) {
            if w[0].pwm == 0 && w[1].pwm > 0 {
                r.starts += 1;
            }
            if w[0].pwm > 0 && w[1].pwm == 0 {
                r.stops += 1;
            }
        }
        for i in per_s..s.len() {
            r.max_step_per_s = r.max_step_per_s.max(s[i].pwm.abs_diff(s[i - per_s].pwm));
        }
        r.off_share = s.iter().filter(|x| x.pwm == 0).count() as f32 / n;
        r.mean_pwm = s.iter().map(|x| x.pwm as f32).sum::<f32>() / n;
        r.max_pwm = s.iter().map(|x| x.pwm).max().unwrap_or(0);
        r.mean_hottest = s.iter().map(|x| x.hottest).sum::<f32>() / n;
        r.max_hottest = s.iter().map(|x| x.hottest).fold(0.0, f32::max);
        r
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::power::fake::Fake;
    use crate::testutil::TestDir;

    /// scripts/rocknix-contract.sh checks ROCKNIX's fancontrol against these trips (its trips rows).
    #[test]
    fn rocknix_trips_are_the_contract_scripts() {
        let mut seen = Vec::new();
        for r in crate::os_compat::tests::contract_rows().into_iter().filter(|r| r[0] == "trips") {
            let mut w = r[4].split_whitespace();
            let profile = w.next().unwrap().to_string();
            let trips: Vec<i64> = w.map(|t| t.parse().unwrap()).collect();
            assert_eq!(trips, rocknix_trips(&profile).to_vec(), "trips row of {profile} vs fan_curve::rocknix_trips");
            seen.push(profile);
        }
        seen.sort();
        assert_eq!(seen, ["aggressive", "moderate", "quiet"]);
    }

    // ---- the curve and the controller

    #[test]
    fn every_curve_rises_from_its_start_to_full_speed_at_the_guard() {
        for s in [Shape::VeryQuiet, Shape::Quiet, Shape::Balanced, Shape::Cool] {
            let p = s.points();
            assert!(p.windows(2).all(|w| w[1].0 > w[0].0 && w[1].1 > w[0].1), "{s:?} rises");
            assert_eq!(p.last().unwrap(), &(GUARD_ON, 255.0), "{s:?} meets the guard");
            assert!(s.stop() < s.start() - 4.0, "{s:?}: a wide gap between start and stop");
            assert_eq!(s.pwm_at(s.start() - 10.0), p[0].1);
            assert_eq!(s.pwm_at(90.0), 255.0);
            // Linear between the points.
            let (a, b) = (p[1], p[2]);
            assert!((s.pwm_at((a.0 + b.0) / 2.0) - (a.1 + b.1) / 2.0).abs() < 0.01);
            assert_eq!(Shape::parse(s.name()), Some(s));
        }
        // The profiles start where ROCKNIX's do: quiet 52, moderate 45-46, aggressive 40.
        assert_eq!((Shape::Quiet.start(), Shape::Balanced.start(), Shape::Cool.start()), (52.0, 46.0, 40.0));
        // Very quiet starts at 60 and stops below 55; the others are unchanged.
        assert_eq!((Shape::VeryQuiet.start(), Shape::VeryQuiet.stop()), (60.0, 55.0));
        // Cool is never quieter than Balanced, Balanced never quieter than Quiet, Quiet never than Very quiet.
        for t in 30..90 {
            let t = t as f32;
            assert!(Shape::Quiet.pwm_at(t) >= Shape::VeryQuiet.pwm_at(t), "{t}");
            assert!(
                Shape::Cool.pwm_at(t) >= Shape::Balanced.pwm_at(t) && Shape::Balanced.pwm_at(t) >= Shape::Quiet.pwm_at(t),
                "{t}"
            );
        }
    }

    #[test]
    fn very_quiet_stays_off_until_60_and_does_not_toggle_around_it() {
        let vq = Shape::VeryQuiet;
        // 59 °C for ten minutes: the fan never starts (Quiet would be running at 46).
        let mut c = Controller::new(0, MIN_RUN_PWM);
        assert_eq!(hold(&mut c, 59.0, 600.0, vq), Output { pwm: 0, mode: Mode::Off });
        assert!(hold(&mut Controller::new(0, MIN_RUN_PWM), 59.0, 60.0, Shape::Quiet).pwm > 0);
        // 61 °C: it starts, at the start level, then settles on the curve's gentle end.
        let mut c = Controller::new(0, MIN_RUN_PWM);
        hold(&mut c, 50.0, 30.0, vq);
        let mut started = false;
        for _ in 0..120 {
            let o = c.step(0.5, 62.0, Some(if c.pwm > 0.0 { 2000 } else { 0 }), vq);
            started |= o.mode == Mode::Start && o.pwm == START_PWM;
        }
        assert!(started, "start pulse kept");
        let settled = hold(&mut c, 62.0, 30.0, vq);
        assert!((MIN_RUN_PWM..=48).contains(&settled.pwm), "running minimum on the gentlest ramp: {settled:?}");
        // Hysteresis: wobbling between 56 and 59 °C (above the stop at 55) keeps it turning without any new start.
        let mut starts = 0;
        for i in 0..400 {
            let t = if i % 20 < 10 { 56.0 } else { 59.0 };
            let o = c.step(0.5, t, Some(2000), vq);
            starts += (o.mode == Mode::Start) as u32;
            assert!(o.pwm > 0, "{t}: still turning above the stop temperature");
        }
        assert_eq!(starts, 0);
        // Below 55 for the hold time (45 s): off, and it takes the full hold.
        assert!(hold(&mut c, 50.0, 30.0, vq).pwm > 0, "not yet");
        assert_eq!(hold(&mut c, 50.0, 60.0, vq).pwm, 0, "then off");
        // Off again, and 57 °C (between stop and start) does not start it.
        assert_eq!(hold(&mut c, 57.0, 120.0, vq).pwm, 0);
    }

    #[test]
    fn very_quiet_is_gentle_and_the_guard_still_wins() {
        let vq = Shape::VeryQuiet;
        assert!(vq.pwm_at(70.0) < 64.0 && vq.pwm_at(73.0) < 100.0 && vq.pwm_at(76.0) < 160.0, "later than Quiet's points");
        // Full speed from the guard, whatever the profile.
        let mut c = Controller::new(0, MIN_RUN_PWM);
        hold(&mut c, 50.0, 30.0, vq);
        assert_eq!(c.step(0.5, GUARD_ON, Some(0), vq), Output { pwm: 255, mode: Mode::Guard });
        assert_eq!(c.step(0.5, 74.0, Some(5000), vq).mode, Mode::Guard, "held until below {GUARD_OFF}");
    }

    #[test]
    fn very_quiet_parses_and_names_round_trip() {
        assert_eq!(Fan::parse("veryquiet"), Fan::VeryQuiet);
        assert_eq!(Fan::VeryQuiet.value(), "veryquiet");
        assert_eq!(shape_for(Fan::VeryQuiet, "aggressive"), Some(Shape::VeryQuiet), "an explicit choice is ours");
        assert_eq!(shape_for(Fan::VeryQuiet, "custom"), Some(Shape::VeryQuiet));
    }

    #[test]
    fn very_quiet_leaves_the_kernel_backstop_hot_and_critical_exactly_as_with_quiet() {
        let rig = Rig::new("veryquiet-backstop");
        rig.temps(57.0, 53.0);
        let mut e = rig.engine();
        e.tick(0.0, &on(Fan::VeryQuiet));
        assert!(e.driving());
        assert_eq!(rig.active(), ["83500"; 6], "the same backstop");
        assert_eq!(rig.others(), OTHERS, "hot, critical and passive untouched");
        assert_eq!(rig.pwm(), 0, "57 °C: below Very quiet's 60 °C, the fan stays off");
        rig.temps(64.0, 60.0);
        run(&mut e, &rig, &on(Fan::VeryQuiet), 0.5, 30.0);
        assert!(rig.pwm() >= MIN_RUN_PWM as i64, "64 °C: running");
        assert_eq!(rig.active(), ["83500"; 6]);
        e.release("test");
        assert_eq!(rig.active(), SYSTEM);
        assert_eq!(rig.others(), OTHERS);
    }

    #[test]
    fn auto_follows_rocknixs_profile_and_custom_stays_rocknixs() {
        assert_eq!(shape_for(Fan::Auto, "quiet"), Some(Shape::Quiet));
        assert_eq!(shape_for(Fan::Auto, "moderate"), Some(Shape::Balanced));
        assert_eq!(shape_for(Fan::Auto, ""), Some(Shape::Balanced));
        assert_eq!(shape_for(Fan::Auto, "aggressive"), Some(Shape::Cool));
        assert_eq!(shape_for(Fan::Auto, "custom"), None);
        assert_eq!(shape_for(Fan::Quiet, "aggressive"), Some(Shape::Quiet));
        assert_eq!(shape_for(Fan::Cool, "custom"), Some(Shape::Cool), "an explicit choice is ours");
    }

    /// Steps `c` for `secs` at a constant temperature, half a second each; the last output.
    fn hold(c: &mut Controller, t: f32, secs: f32, shape: Shape) -> Output {
        let mut out = Output { pwm: 0, mode: Mode::Off };
        for _ in 0..(secs / 0.5) as usize {
            out = c.step(0.5, t, Some(if c.pwm > 0.0 { 2000 } else { 0 }), shape);
        }
        out
    }

    #[test]
    fn a_cool_pocket_keeps_the_fan_off_and_a_warm_one_starts_it_gently() {
        let mut c = Controller::new(0, MIN_RUN_PWM);
        assert_eq!(hold(&mut c, 45.0, 60.0, Shape::Quiet), Output { pwm: 0, mode: Mode::Off });
        // 56 °C: once the smoothed temperature reaches 52 (about 5 s), the start level for two seconds, then down to
        // the curve's low running speed.
        let mut waited = 0.0;
        let first = loop {
            let out = c.step(0.5, 56.0, Some(0), Shape::Quiet);
            waited += 0.5;
            if out.pwm > 0 {
                break out;
            }
        };
        assert!((4.0..=6.0).contains(&waited), "smoothed: {waited} s");
        assert_eq!(first.pwm, START_PWM, "starts from the level the fan surely starts from");
        assert_eq!(first.mode, Mode::Start);
        let settled = hold(&mut c, 56.0, 30.0, Shape::Quiet);
        assert_eq!(settled.mode, Mode::Curve);
        assert!((40..=46).contains(&settled.pwm), "the quiet end of the curve: {settled:?}");
    }

    #[test]
    fn hysteresis_a_wobble_around_a_temperature_does_not_move_the_fan() {
        let mut c = Controller::new(0, MIN_RUN_PWM);
        hold(&mut c, 66.0, 120.0, Shape::Balanced);
        let at = c.pwm.round() as u8;
        let mut seen = std::collections::BTreeSet::new();
        for i in 0..240 {
            let t = 66.0 + if i % 8 < 4 { 1.2 } else { -1.2 };
            seen.insert(c.step(0.5, t, Some(2000), Shape::Balanced).pwm);
        }
        let (lo, hi) = (*seen.first().unwrap(), *seen.last().unwrap());
        assert!(hi - lo <= 6, "±1.2 °C wobble moves the PWM by at most a few steps: {lo}..{hi} (from {at})");
        // And the fan doesn't stop when the temperature dips below the start for a moment.
        let mut q = Controller::new(0, MIN_RUN_PWM);
        hold(&mut q, 55.0, 60.0, Shape::Quiet);
        for _ in 0..40 {
            assert!(q.step(0.5, 49.0, Some(2000), Shape::Quiet).pwm > 0, "49 °C is above Quiet's stop (46)");
        }
        assert!(hold(&mut q, 40.0, 30.0, Shape::Quiet).pwm > 0, "below the stop: still turning before STOP_HOLD");
        assert_eq!(hold(&mut q, 40.0, 60.0, Shape::Quiet).pwm, 0, "then it stops");
    }

    #[test]
    fn the_pwm_moves_at_most_by_the_slew_limits_outside_starts_stops_and_the_guard() {
        let mut c = Controller::new(0, MIN_RUN_PWM);
        let mut last = 0u8;
        let mut t = 50.0f32;
        let mut time = 0.0f32;
        let mut max_up = 0.0f32;
        let mut max_down = 0.0f32;
        // Up to 76 °C (just under the guard) in a minute, then down again.
        while time < 500.0 {
            t = if time < 60.0 {
                50.0 + 26.0 * time / 60.0
            } else if time < 200.0 {
                76.0
            } else {
                (76.0 - (time - 200.0) * 0.2).max(40.0)
            };
            let out = c.step(0.5, t, Some(if c.pwm > 0.0 { 2000 } else { 0 }), Shape::Quiet);
            let d = out.pwm as f32 - last as f32;
            if last > 0 && out.pwm > 0 && out.mode == Mode::Curve {
                max_up = max_up.max(d);
                max_down = max_down.max(-d);
            }
            last = out.pwm;
            time += 0.5;
        }
        assert!(max_up <= SLEW_UP * 0.5 + 1.0, "up {max_up} per half second");
        assert!(max_down <= SLEW_DOWN * 0.5 + 1.0, "down {max_down} per half second");
        assert!(t <= 40.0 && last == 0, "and it stopped in the end");
    }

    #[test]
    fn the_guard_runs_full_speed_at_once_whatever_the_profile_and_lets_go_lower() {
        for shape in [Shape::Quiet, Shape::Balanced, Shape::Cool] {
            let mut c = Controller::new(0, MIN_RUN_PWM);
            hold(&mut c, 45.0, 30.0, shape);
            assert_eq!(
                c.step(0.5, GUARD_ON, Some(0), shape),
                Output { pwm: 255, mode: Mode::Guard },
                "{shape:?}: no slew, no smoothing"
            );
            assert_eq!(
                c.step(0.5, 74.0, Some(5000), shape).mode,
                Mode::Guard,
                "{shape:?}: held until below {GUARD_OFF}"
            );
            let after = c.step(0.5, 70.0, Some(5000), shape);
            assert_eq!(after.mode, Mode::Curve);
            assert!(after.pwm >= 250, "and down from there slowly: {after:?}");
        }
        // Even from a warm, smoothed state that says otherwise.
        let mut c = Controller::new(0, MIN_RUN_PWM);
        hold(&mut c, 40.0, 60.0, Shape::Quiet);
        assert_eq!(c.step(0.5, 85.0, None, Shape::Quiet).pwm, 255);
    }

    #[test]
    fn the_fan_never_stands_still_above_the_start_temperature() {
        // A pseudo-random walk through 30-80 °C: whenever the smoothed temperature is at or above the start, the fan
        // turns at its running minimum or more; at the guard it runs full.
        let mut seed = 7u64;
        for shape in [Shape::Quiet, Shape::Balanced, Shape::Cool] {
            let mut c = Controller::new(0, MIN_RUN_PWM);
            let mut t = 45.0f32;
            for _ in 0..20_000 {
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                t = (t + ((seed >> 40) as f32 / (1u64 << 24) as f32 - 0.5) * 2.0).clamp(30.0, 82.0);
                let out = c.step(0.5, t, Some(if c.pwm > 0.0 { 1500 } else { 0 }), shape);
                if c.ema.unwrap() >= shape.start() {
                    assert!(
                        out.pwm as f32 >= c.min_run.min(START_PWM as f32),
                        "{shape:?} at {t} (smoothed {:?}): {out:?}",
                        c.ema
                    );
                }
                if t >= GUARD_ON {
                    assert_eq!(out.pwm, 255);
                }
            }
        }
    }

    #[test]
    fn a_stall_kicks_the_fan_and_raises_its_running_minimum() {
        let mut c = Controller::new(0, 30);
        hold(&mut c, 56.0, 20.0, Shape::Quiet);
        assert!((40..=46).contains(&(c.pwm.round() as u8)), "on the curve: {}", c.pwm);
        // The tachometer reads 0: after STALL_SECS a kick, and a higher minimum.
        let mut kicked = None;
        for i in 0..20 {
            let out = c.step(0.5, 56.0, Some(0), Shape::Quiet);
            if out.mode == Mode::Start {
                kicked = Some((i, out.pwm));
                break;
            }
        }
        let (i, pwm) = kicked.expect("kicked");
        assert!(i as f32 * 0.5 >= STALL_SECS - 0.5, "not at once: {i}");
        assert!(pwm >= START_PWM, "{pwm}");
        assert_eq!(c.stalls, 1);
        assert_eq!(c.min_run, 36.0);
        // Without a tachometer nothing is assumed.
        let mut n = Controller::new(0, 40);
        hold(&mut n, 56.0, 30.0, Shape::Quiet);
        assert_eq!(n.stalls, 0);
    }

    #[test]
    fn taking_over_a_running_fan_continues_from_its_speed() {
        let mut c = Controller::new(102, MIN_RUN_PWM);
        let out = c.step(0.5, 56.0, Some(3000), Shape::Quiet);
        assert!(out.pwm >= 100, "no drop: {out:?}");
        assert_eq!(out.mode, Mode::Curve);
    }

    // ---- the engine on a fake sysfs

    struct Rig {
        f: Fake,
        paths: Paths,
    }

    impl Rig {
        fn new(name: &str) -> Rig {
            let f = Fake::new(&format!("curve-{name}"));
            let r = &f.0;
            put(r, "sys/class/hwmon/hwmon3/name", "pwmfan");
            put(r, "sys/class/hwmon/hwmon3/pwm1", "0");
            put(r, "sys/class/hwmon/hwmon3/pwm1_enable", "2");
            put(r, "sys/class/hwmon/hwmon3/fan1_input", "0");
            put(r, "sys/class/thermal/thermal_zone6/type", "cluster1-thermal");
            put(r, "sys/class/thermal/thermal_zone6/mode", "enabled");
            put(r, "sys/class/thermal/thermal_zone2/type", "cpu7-top-thermal");
            put(r, "sys/class/thermal/thermal_zone9/type", "camera-thermal");
            put(r, "system.cfg", "cooling.profile=quiet\n");
            let rig = Rig { paths: Paths::under(&f.0), f };
            rig.temps(45.0, 44.0);
            put(&rig.f.0, "sys/class/thermal/thermal_zone9/temp", "95000");
            rig
        }

        /// The prime core's and the cluster zone's temperature.
        fn temps(&self, core: f32, cluster: f32) {
            put(&self.f.0, "sys/class/thermal/thermal_zone2/temp", &format!("{}", (core * 1000.0) as i64));
            put(&self.f.0, "sys/class/thermal/thermal_zone6/temp", &format!("{}", (cluster * 1000.0) as i64));
        }

        fn pwm(&self) -> i64 {
            read_i64(&self.f.0.join("sys/class/hwmon/hwmon3/pwm1")).unwrap()
        }

        fn active(&self) -> Vec<String> {
            (2..8).map(|i| self.f.trip(i)).collect()
        }

        fn others(&self) -> [String; 3] {
            [self.f.trip(0), self.f.trip(1), self.f.trip(8)]
        }

        fn engine(&self) -> Engine {
            Engine::with_min_run(self.paths.clone(), MIN_RUN_PWM)
        }

        /// The fan turns when it is driven, like a real one would report.
        fn spin(&self) {
            put(&self.f.0, "sys/class/hwmon/hwmon3/fan1_input", if self.pwm() > 0 { "2100" } else { "0" });
        }
    }

    fn put(root: &Path, rel: &str, v: &str) {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, v).unwrap();
    }

    const SYSTEM: [&str; 6] = ["52000", "62000", "69000", "75000", "80000", "85000"];
    const OTHERS: [&str; 3] = ["90000", "110000", "95000"];

    fn on(fan: Fan) -> Plan {
        Plan {
            on: true,
            fan,
            ..Plan::default()
        }
    }

    /// Ticks every half second from `from` for `secs`; the time after.
    fn run(e: &mut Engine, rig: &Rig, plan: &Plan, from: f64, secs: f64) -> f64 {
        let mut t = from;
        while t < from + secs {
            e.tick(t, plan);
            rig.spin();
            t += 0.5;
        }
        t
    }

    #[test]
    fn sm8550_leaves_pwm_trips_and_stale_recovery_notes_alone() {
        let rig = Rig::new("sm8550");
        rig.temps(90.0, 90.0);
        put(&rig.f.0, "etc/os-release", "HW_DEVICE=\"SM8550\"\n");
        let before = rig.active();
        let mut e = rig.engine();
        for fan in [Fan::Quiet, Fan::Cool, Fan::Auto] { e.tick(1.0, &on(fan)); }
        assert!(!e.driving());
        assert_eq!(rig.pwm(), 0);
        assert_eq!(rig.active(), before);
        // A valid SM8250 recovery note must not be applied to an SM8550 card either.
        let note = Note { pwm: rig.f.0.join("sys/class/hwmon/hwmon3/pwm1"), ..Note::default() };
        std::fs::create_dir_all(rig.paths.note.parent().unwrap()).unwrap();
        std::fs::write(&rig.paths.note, note.text()).unwrap();
        assert!(!hand_back(&rig.paths.note, &rig.f.0));
        assert!(rig.paths.note.exists());
        assert_eq!(rig.pwm(), 0);
    }

    #[test]
    fn taking_over_moves_only_the_active_trips_and_hands_back_everything() {
        let rig = Rig::new("takeover");
        rig.temps(57.0, 53.0);
        let mut e = rig.engine();
        e.tick(0.0, &on(Fan::Auto));
        assert!(e.driving());
        assert_eq!(rig.active(), ["83500"; 6], "six trips at the backstop");
        assert_eq!(rig.others(), OTHERS, "hot, critical and passive untouched");
        let note = std::fs::read_to_string(&rig.paths.note).unwrap();
        assert!(note.contains("trip_point_2_temp 52000") && note.contains("off-below"), "{note}");
        assert_eq!(rig.pwm(), START_PWM as i64, "57 °C on Quiet (ROCKNIX's quiet): started");
        run(&mut e, &rig, &on(Fan::Auto), 0.5, 30.0);
        assert!((40..=48).contains(&rig.pwm()), "the curve's quiet end: {}", rig.pwm());
        // Asleep: the kernel's at once.
        e.tick(31.0, &Plan::default());
        assert!(!e.driving());
        assert_eq!(rig.active(), SYSTEM);
        assert_eq!(rig.others(), OTHERS);
        assert!(!rig.paths.note.exists());
        assert!(rig.pwm() > 0, "53 °C is above the lowest trip: the kernel's fan, not stopped by us");
    }

    #[test]
    fn handing_back_below_the_lowest_trip_turns_the_fan_off_as_the_kernel_would() {
        let rig = Rig::new("handback-cool");
        rig.temps(60.0, 56.0);
        let mut e = rig.engine();
        run(&mut e, &rig, &on(Fan::Cool), 0.0, 60.0);
        assert!(rig.pwm() > 60);
        rig.temps(49.0, 48.0);
        e.release("test");
        assert_eq!(rig.pwm(), 0, "48 °C < 52 °C: off, as ROCKNIX's quiet trips say");
        assert_eq!(rig.active(), SYSTEM);
        // Above the lowest trip the PWM is only parked; the kernel's step_wise sets its level from there.
        let mut e = rig.engine();
        rig.temps(60.0, 58.0);
        run(&mut e, &rig, &on(Fan::Cool), 0.0, 60.0);
        e.release("test");
        assert_eq!(rig.pwm(), START_PWM as i64);
    }

    #[test]
    fn the_launcher_and_the_engine_read_the_same_note() {
        let rig = Rig::new("note");
        let mut e = rig.engine();
        rig.temps(60.0, 58.0);
        e.tick(0.0, &on(Fan::Quiet));
        let text = std::fs::read_to_string(&rig.paths.note).unwrap();
        let n = Note::parse(&text, &rig.f.0).unwrap();
        assert_eq!(n.trips.len(), 6);
        assert_eq!(n.text(), text);
        assert!(text.lines().next().unwrap().starts_with('#'));
        // Only paths below <root>/sys count.
        assert!(Note::parse("trip /etc/passwd 1\n", &rig.f.0).is_none());
        assert!(Note::parse(&format!("trip {}/sys/../etc/x 1\n", rig.f.0.display()), &rig.f.0).is_none());
    }

    #[test]
    fn the_launchers_shell_hand_back_carries_out_a_note_the_engine_wrote() {
        let script = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/deploy/run-pocket-ui.sh")).unwrap();
        let start = script.find("fan_handback() {").expect("fan_handback in the launcher");
        let end = start + script[start..].find("\n}\n").unwrap() + 3;
        let func = &script[start..end];
        for (core, cluster, pwm_after) in [(60.0, 58.0, START_PWM as i64), (49.0, 47.0, 0)] {
            let rig = Rig::new(&format!("shell-{pwm_after}"));
            rig.temps(65.0, 60.0);
            let mut e = rig.engine();
            run(&mut e, &rig, &on(Fan::Cool), 0.0, 60.0);
            assert!(rig.pwm() > 80);
            drop(e); // pocket-ui died
            rig.temps(core, cluster);
            let out = std::process::Command::new("sh")
                .arg("-c")
                .arg(format!("log() {{ echo \"$*\"; }}\n{func}\nfan_handback test"))
                .env("POCKETUI_FAN_ROOT", &*rig.f.0)
                .env("FAN_NOTE", &rig.paths.note)
                .output()
                .unwrap();
            assert!(String::from_utf8_lossy(&out.stdout).contains("back to ROCKNIX"), "{out:?}");
            assert_eq!(rig.active(), SYSTEM);
            assert_eq!(rig.others(), OTHERS);
            assert_eq!(rig.pwm(), pwm_after, "at {cluster} °C");
            assert!(!rig.paths.note.exists());
        }
    }

    #[test]
    fn a_crashed_pocket_ui_is_undone_by_the_note_alone() {
        let rig = Rig::new("crash");
        rig.temps(60.0, 58.0);
        let mut first = rig.engine();
        run(&mut first, &rig, &on(Fan::Quiet), 0.0, 10.0);
        drop(first); // killed: nothing handed back
        assert_eq!(rig.active(), ["83500"; 6]);
        // The launcher's clean-up does what `hand_back` does (deploy/test-run-pocket-ui.sh has the shell side).
        assert!(hand_back(&rig.paths.note, &rig.f.0));
        assert_eq!(rig.active(), SYSTEM);
        assert!(!hand_back(&rig.paths.note, &rig.f.0), "nothing left");
        // Without the clean-up: the next pocket-ui's engine does it before it notes anything.
        let mut second = rig.engine();
        run(&mut second, &rig, &on(Fan::Quiet), 0.0, 5.0);
        drop(second);
        let mut third = rig.engine();
        third.tick(0.0, &on(Fan::Quiet));
        third.release("test");
        assert_eq!(rig.active(), SYSTEM, "the system's trips were noted, not the dead run's backstop");
    }

    #[test]
    fn a_lost_note_with_our_backstop_in_place_notes_rocknixs_trips() {
        let rig = Rig::new("lost-note");
        for i in 2..8 {
            put(&rig.f.0, &format!("sys/class/thermal/thermal_zone6/trip_point_{i}_temp"), "83500");
        }
        let mut e = rig.engine();
        e.tick(0.0, &on(Fan::Quiet));
        e.release("test");
        assert_eq!(rig.active(), SYSTEM, "ROCKNIX's quiet profile, from cooling.profile");
    }

    #[test]
    fn the_guard_overrides_quiet_and_the_kernel_backstop_stays_above_it() {
        let rig = Rig::new("guard");
        rig.temps(55.0, 52.0);
        let mut e = rig.engine();
        let t = run(&mut e, &rig, &on(Fan::Quiet), 0.0, 30.0);
        assert!(rig.pwm() < 60);
        rig.temps(79.0, 70.0);
        e.tick(t, &on(Fan::Quiet));
        assert_eq!(rig.pwm(), 255, "one tick: full speed");
        assert!(matches!(e.status, Status::Driving { mode: Mode::Guard, .. }));
        assert!(BACKSTOP_TRIP as f32 / 1000.0 > GUARD_ON && BACKSTOP_TRIP < 90_000);
    }

    #[test]
    fn settling_waits_and_a_rocknix_custom_profile_is_left_alone() {
        let rig = Rig::new("settle");
        rig.temps(60.0, 58.0);
        let mut e = rig.engine();
        e.tick(
            0.0,
            &Plan {
                settling: true,
                ..on(Fan::Quiet)
            },
        );
        assert!(!e.driving() && rig.active() == SYSTEM, "no take-over while --restore-system runs");
        e.tick(0.5, &on(Fan::Quiet));
        assert!(e.driving());
        e.tick(
            1.0,
            &Plan {
                settling: true,
                ..on(Fan::Quiet)
            },
        );
        assert!(e.driving(), "a clean-up after a game doesn't interrupt a curve that runs");
        put(&rig.f.0, "system.cfg", "cooling.profile=custom\n");
        e.tick(1.5, &on(Fan::Auto));
        assert!(!e.driving(), "custom: ROCKNIX's own loop");
        assert_eq!(rig.active(), SYSTEM);
        // A zone ROCKNIX disabled for its loop is never taken.
        put(&rig.f.0, "sys/class/thermal/thermal_zone6/mode", "disabled");
        put(&rig.f.0, "system.cfg", "cooling.profile=quiet\n");
        e.tick(100.0, &on(Fan::Quiet));
        assert!(!e.driving());
        assert!(matches!(&e.status, Status::Unavailable(w) if w.contains("ROCKNIX")));
    }

    #[test]
    fn no_fan_no_writes() {
        let root = TestDir::new("curve-nofan");
        std::fs::create_dir_all(root.join("sys/class/thermal/thermal_zone0")).unwrap();
        std::fs::write(root.join("sys/class/thermal/thermal_zone0/temp"), "60000").unwrap();
        let paths = Paths::under(&root);
        let mut e = Engine::with_min_run(paths.clone(), MIN_RUN_PWM);
        e.tick(0.0, &on(Fan::Cool));
        assert!(!e.driving());
        assert_eq!(e.status, Status::Unavailable("no PWM fan".into()));
        assert!(!paths.note.exists());
        let files: Vec<_> = walk(&root.join("sys"));
        assert_eq!(files.len(), 1, "nothing written below sys: {files:?}");
    }

    fn walk(d: &Path) -> Vec<PathBuf> {
        let mut v = Vec::new();
        for e in std::fs::read_dir(d).into_iter().flatten().flatten() {
            if e.path().is_dir() {
                v.extend(walk(&e.path()));
            } else {
                v.push(e.path());
            }
        }
        v
    }

    #[test]
    fn rocknix_rewriting_its_trips_mid_game_is_noted_and_the_curve_goes_on() {
        let rig = Rig::new("fancontrol");
        rig.temps(60.0, 58.0);
        let mut e = rig.engine();
        let plan = Plan {
            game: Game::Other,
            game_id: 1,
            ..on(Fan::Quiet)
        };
        let t = run(&mut e, &rig, &plan, 0.0, 20.0);
        // runemu.sh's `systemctl restart fancontrol` at the end of a game: the aggressive profile's trips.
        let aggressive = ["40000", "48000", "55000", "61000", "66000", "71000"];
        for (i, v) in aggressive.iter().enumerate() {
            put(&rig.f.0, &format!("sys/class/thermal/thermal_zone6/trip_point_{}_temp", i + 2), v);
        }
        e.tick(t, &plan);
        assert_eq!(rig.active(), aggressive, "seen once: waits");
        e.tick(t + 0.5, &plan);
        assert_eq!(rig.active(), ["83500"; 6], "the same twice: noted and ours again");
        assert!(e.driving());
        e.release("test");
        assert_eq!(rig.active(), aggressive, "the system's latest trips come back");
    }

    #[test]
    fn the_kernel_setting_the_fan_again_and_again_gives_it_back() {
        let rig = Rig::new("fight");
        rig.temps(65.0, 60.0);
        let mut e = rig.engine();
        let mut t = run(&mut e, &rig, &on(Fan::Cool), 0.0, 10.0);
        for _ in 0..MAX_INTERVENTIONS {
            put(&rig.f.0, "sys/class/hwmon/hwmon3/pwm1", "204");
            e.tick(t, &on(Fan::Cool));
            t += 0.5;
        }
        assert!(!e.driving());
        assert_eq!(rig.active(), SYSTEM);
        assert!(matches!(&e.status, Status::Unavailable(w) if w.contains("kernel")));
        e.tick(t + 1.0, &on(Fan::Cool));
        assert!(!e.driving(), "and stays so for a while");
        e.tick(t + BACKOFF_SECS + 1.0, &on(Fan::Cool));
        assert!(e.driving(), "then tries again");
    }

    #[test]
    fn games_follow_their_choice_ports_follow_games_fan_or_rocknixs_own_switch() {
        let rig = Rig::new("games");
        rig.temps(50.0, 48.0);
        let mut e = rig.engine();
        // A pocket-core game that chose Cool in its pause menu (pocket-core wrote the request).
        put(&rig.f.0, "tmp/fan.game", "cool");
        e.tick(
            0.0,
            &Plan {
                game: Game::Hosted,
                game_id: 1,
                ..on(Fan::Quiet)
            },
        );
        assert!(matches!(e.status, Status::Driving { shape: Shape::Cool, .. }), "{:?}", e.status);
        // A port: Games: fan (Quiet here).
        e.tick(
            0.5,
            &Plan {
                game: Game::Other,
                game_id: 2,
                ..on(Fan::Quiet)
            },
        );
        assert!(matches!(e.status, Status::Driving { shape: Shape::Quiet, .. }));
        // runemu.sh switched cooling.profile for this game (ROCKNIX's own per-game setting): that wins.
        put(&rig.f.0, "system.cfg", "cooling.profile=aggressive\n");
        e.tick(
            1.0,
            &Plan {
                game: Game::Other,
                game_id: 2,
                ..on(Fan::Quiet)
            },
        );
        assert!(matches!(e.status, Status::Driving { shape: Shape::Cool, .. }), "{:?}", e.status);
        // A port with a fan choice of its own (its card's Performance & fan): that wins over ROCKNIX's switch.
        e.tick(
            1.2,
            &Plan {
                game: Game::Other,
                game_id: 2,
                own: true,
                ..on(Fan::Quiet)
            },
        );
        assert!(matches!(e.status, Status::Driving { shape: Shape::Quiet, .. }), "{:?}", e.status);
        // The next game starts with that profile in place: it is that game's baseline, Games: fan counts again.
        e.tick(
            1.5,
            &Plan {
                game: Game::Other,
                game_id: 3,
                ..on(Fan::Quiet)
            },
        );
        assert!(matches!(e.status, Status::Driving { shape: Shape::Quiet, .. }));
        // Outside games, Auto follows ROCKNIX's profile.
        e.tick(2.0, &on(Fan::Auto));
        assert!(matches!(e.status, Status::Driving { shape: Shape::Cool, .. }));
        // Looked up as ROCKNIX's get_setting does: system.cooling.profile first.
        put(&rig.f.0, "system.cfg", "cooling.profile=aggressive\nsystem.cooling.profile=quiet\n");
        e.tick(2.5, &on(Fan::Auto));
        assert!(matches!(e.status, Status::Driving { shape: Shape::Quiet, .. }), "{:?}", e.status);
    }

    #[test]
    fn the_beat_counts_and_says_what_the_fan_does() {
        let rig = Rig::new("beat");
        rig.temps(60.0, 58.0);
        let mut e = rig.engine();
        e.tick(0.0, &on(Fan::Quiet));
        let first: u64 = std::fs::read_to_string(&rig.paths.beat).unwrap().split(' ').next().unwrap().parse().unwrap();
        e.tick(0.5, &on(Fan::Quiet));
        let line = std::fs::read_to_string(&rig.paths.beat).unwrap();
        let second: u64 = line.split(' ').next().unwrap().parse().unwrap();
        assert!(second > first);
        let now: u64 = line.split(' ').nth(1).unwrap().parse().unwrap();
        let b = Beat::read(&rig.paths.beat, now).unwrap();
        assert!(b.driving);
        assert_eq!(b.shape, Some(Shape::Quiet));
        assert!(b.ema.is_some_and(|t| (55_000..65_000).contains(&t)));
        assert_eq!(Beat::read(&rig.paths.beat, now + 60), None, "stale");
        e.tick(1.0, &Plan::default());
        let b = Beat::read(&rig.paths.beat, now).unwrap();
        assert!(!b.driving && b.mode.is_none());
    }

    #[test]
    fn the_thread_hands_back_when_asked_and_waits_for_it() {
        let rig = Rig::new("thread");
        rig.temps(60.0, 58.0);
        let h = spawn(rig.paths.clone());
        h.set(on(Fan::Quiet));
        let until = Instant::now() + Duration::from_secs(3);
        while !h.driving() && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(h.driving());
        assert_eq!(rig.active(), ["83500"; 6]);
        h.release_and_wait(Duration::from_secs(2));
        assert!(!h.driving());
        assert_eq!(rig.active(), SYSTEM);
        assert!(!rig.paths.note.exists());
        // Dropping the handle ends the thread, and it hands back if it had taken the fan again.
        h.set(on(Fan::Quiet));
        let until = Instant::now() + Duration::from_secs(3);
        while !h.driving() && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(10));
        }
        drop(h);
        let until = Instant::now() + Duration::from_secs(3);
        while rig.paths.note.exists() && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(rig.active(), SYSTEM);
    }

    // ---- the simulation

    #[test]
    fn simulated_deltarune_the_old_steps_cycle_the_curve_does_not() {
        let old = sim::summary(&sim::run(sim::Driver::Kernel(sim::Kernel::quiet()), 900.0, 1), 120.0);
        let new = sim::summary(&sim::run(sim::Driver::Curve(Controller::new(0, MIN_RUN_PWM), Shape::Quiet), 900.0, 1), 120.0);
        assert!(old.starts >= 10, "the old trips switch the fan on and off: {old:?}");
        assert_eq!((new.starts, new.stops), (0, 0), "the curve keeps it turning: {new:?}");
        assert!(new.max_step_per_s <= (SLEW_UP as u8) + 1, "{new:?}");
        assert!(old.max_step_per_s >= START_PWM, "{old:?}");
        assert!(new.max_pwm < old.max_pwm.max(77), "never louder than the old top step: {new:?} vs {old:?}");
        assert!(new.max_hottest < GUARD_ON, "{new:?}");
    }
}
