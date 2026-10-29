//! The Extras that touch the system: the CPU performance profile and the fan, through sysfs.
//!
//! How ROCKNIX does it (SM8250, `runemu.sh` and `bin/fancontrol`):
//! - CPU: it writes a governor (`ondemand`, `schedutil`, `performance`, `powersave`) into
//!   `cpufreq/policy{0,4,7}/scaling_governor` for the little, big and prime cluster, per game, and sets
//!   `performance` while a game loads. `ondemand` is its default.
//! - Fan: the PWM fan (`hwmon/pwmfan`) is a thermal cooling device of the cluster1 zone. The profiles
//!   quiet, moderate and aggressive only differ in the temperatures of the zone's six `active` trip
//!   points; `fancontrol` writes them to `trip_point_N_temp` and leaves the fan to the kernel.
//!
//! We do the same writes, but keep what we found and put it back when the game ends (`restore`), so
//! nothing outlives the game. The originals are also written to a file, so pocket-ui can undo a
//! change after a crash or a kill (`pocket-core --restore-system`).
//!
//! pocket-ui itself uses the fan half for Settings > Performance & fan's "Fan outside games"
//! (`perf_fan::FanKeeper`), with its own note in [`OUTSIDE_RESTORE_FILE`]. Only the zone's `active` trip
//! points are ever written; its `passive`, `hot` and `critical` trips stay as the kernel set them.
//!
//! **The note comes first** (review SOL 2026-10-24 #2): the whole restore plan is written atomically
//! (`fsutil::write_atomic`) before the first sysfs write; when that fails nothing is changed (the game runs
//! with the system's settings, and the log says so). The note goes only once everything is back; a note that
//! can't be read is kept as `<note>.bad` and the CPU goes to safe settings (`schedutil`, full speed).
//!
//! **One owner at a time** (review SOL 2026-10-24 #1): whoever changes the CPU for a game (pocket-core, a port's
//! session in pocket-ui, a PS2/GameCube/Wii/3DS game) holds the [`Lease`] (`flock` on [`LEASE_FILE`]) from before
//! it notes the originals until it has put them back. So the next game notes the system's own limits, never the
//! caps of the one before that is still putting them back, and two owners never share a note.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

pub const RESTORE_FILE: &str = "/tmp/pocket-core.restore";
/// pocket-ui's note while "Fan outside games" has changed the fan (`perf_fan::FanKeeper`); `pocket-core
/// --restore-system` undoes it too, so a pocket-ui that crashed leaves nothing behind (`deploy/run-pocket-ui.sh`).
pub const OUTSIDE_RESTORE_FILE: &str = "/tmp/pocket-ui-fan.restore";
/// The lock whoever changes the CPU for a game holds until it is back ([`Lease`]).
pub const LEASE_FILE: &str = "/tmp/pocket-perf.lock";
/// How long a game's start waits for the one before to put the CPU back (milliseconds in practice).
pub const LEASE_WAIT: Duration = Duration::from_secs(3);
/// The governor a note that can't be read is undone with ([`Power::recover`]).
const SAFE_GOVERNOR: &str = "schedutil";

/// The right to change the CPU's governors and limits for a game, held from before the originals are noted until
/// they are back: an exclusive `flock` on [`LEASE_FILE`], released when this is dropped (or the process ends,
/// a crash included). Each [`Lease::take`] opens the file anew, so two threads of one process exclude each other too.
#[derive(Debug)]
pub struct Lease {
    _file: std::fs::File,
}

impl Lease {
    /// Waits up to `wait` for the lease (polling every few milliseconds, so `give_up` can end the wait early: a
    /// port that ended meanwhile). None when it didn't come, `give_up` said so, or the lock file can't be opened.
    pub fn take(path: &Path, wait: Duration, give_up: impl Fn() -> bool) -> Option<Lease> {
        use std::os::unix::io::AsRawFd;
        let file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(path)
            .map_err(|e| eprintln!("power: can't open {}: {e}", path.display()))
            .ok()?;
        let end = Instant::now() + wait;
        loop {
            // SAFETY: flock on a descriptor this function owns.
            if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 {
                return Some(Lease { _file: file });
            }
            if give_up() || Instant::now() >= end {
                return None;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    /// A game's start (pocket-core, a standalone emulator): the lease on the real system, after the game before has
    /// put the CPU back. Says in the log when it had to wait, and when it went on without (after [`LEASE_WAIT`]).
    pub fn for_game(who: &str) -> Option<Lease> {
        let t0 = Instant::now();
        let lease = Lease::take(Path::new(LEASE_FILE), LEASE_WAIT, || false);
        let ms = t0.elapsed().as_secs_f32() * 1000.0;
        match &lease {
            Some(_) if ms >= 5.0 => println!("{who}: waited {ms:.0} ms for the game before to put the CPU back"),
            Some(_) => {}
            None => eprintln!("{who}: the CPU is still another game's after {ms:.0} ms; going on without the lease"),
        }
        lease
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Performance {
    /// Lower top speeds: enough for Game Boy, Game Boy Color and NES.
    Battery,
    /// Leave the system's governor and limits alone: the system decides. Shown as "Auto"; files written before
    /// 2026-10-23 call it "balanced" (read as Auto, `from_value`).
    #[default]
    #[serde(alias = "balanced")]
    Auto,
    /// Highest clocks all the time. Shown as "Max"; the files keep "performance".
    Performance,
}

impl Performance {
    pub fn parse(s: &str) -> Self {
        Self::from_value(s).unwrap_or(Self::Auto)
    }

    /// The settings files' value, None for anything else (a hand-edited file). "balanced", the old name of Auto,
    /// is still Auto.
    pub fn from_value(s: &str) -> Option<Self> {
        match s {
            "battery" => Some(Self::Battery),
            "auto" | "balanced" => Some(Self::Auto),
            "performance" => Some(Self::Performance),
            _ => None,
        }
    }

    /// The value in the settings files (`options.rs`' choice values).
    pub fn value(self) -> &'static str {
        match self {
            Self::Battery => "battery",
            Self::Auto => "auto",
            Self::Performance => "performance",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Fan {
    /// Off until it gets warm (60 °C): the device is noticeably warmer, the fan is rarely heard.
    VeryQuiet,
    /// Starts later: the device gets warmer, the fan stays quiet.
    Quiet,
    /// Whatever the system's cooling profile is.
    #[default]
    Auto,
    /// Starts early and runs faster.
    Cool,
}

impl Fan {
    pub fn parse(s: &str) -> Self {
        match s {
            "veryquiet" => Self::VeryQuiet,
            "quiet" => Self::Quiet,
            "cool" => Self::Cool,
            _ => Self::Auto,
        }
    }

    /// The value in the settings files (`options.rs`' choice values).
    pub fn value(self) -> &'static str {
        match self {
            Self::VeryQuiet => "veryquiet",
            Self::Quiet => "quiet",
            Self::Auto => "auto",
            Self::Cool => "cool",
        }
    }

    /// Where the fan starts, in °C: the first trip of the profile (None for Auto, the system's own).
    pub fn starts_at(self) -> Option<u32> {
        self.trips().map(|t| t[0] / 1000)
    }

    /// Trip temperatures (millidegrees) of ROCKNIX's quiet and aggressive cooling profiles.
    fn trips(self) -> Option<[u32; 6]> {
        match self {
            // Smooth fan off: the six trips 8 °C above Quiet's first (top two stay below `hot` 90 °C).
            Fan::VeryQuiet => Some([60_000, 68_000, 74_000, 79_000, 83_000, 86_000]),
            Fan::Quiet => Some([52_000, 62_000, 69_000, 75_000, 80_000, 85_000]),
            Fan::Cool => Some([40_000, 48_000, 55_000, 61_000, 66_000, 71_000]),
            Fan::Auto => None,
        }
    }
}

/// The share of a cluster's top speed that Battery allows.
const BATTERY_SHARE: f64 = 0.5;

#[derive(Serialize, Deserialize, Default, Debug, PartialEq, Clone)]
struct Saved {
    /// policy directory → (governor, scaling_max_freq) as we found them
    cpu: BTreeMap<String, (String, String)>,
    /// trip file name (`trip_point_2_temp`) → temperature, in the fan's thermal zone
    fan_zone: String,
    fan: BTreeMap<String, String>,
    /// Set by `set_standalone`: `restore` then only undoes what is still as we wrote it.
    #[serde(default)]
    standalone: bool,
    /// The GPU's devfreq device → governor as we found it.
    #[serde(default)]
    gpu: BTreeMap<String, String>,
    /// File below the root → (what we wrote, what was there before).
    #[serde(default)]
    ours: BTreeMap<String, (String, String)>,
}

pub struct Power {
    root: PathBuf,
    restore_file: PathBuf,
    saved: Saved,
}

fn read(p: &Path) -> Option<String> {
    std::fs::read_to_string(p).ok().map(|s| s.trim().to_string())
}

fn write(p: &Path, v: &str) -> bool {
    std::fs::write(p, v).is_ok()
}

impl Power {
    /// The real system.
    pub fn system() -> Self {
        Self::with_root("/", RESTORE_FILE)
    }

    /// Paths below `root` (a fake sysfs in tests); originals are noted in `restore_file`.
    pub fn with_root(root: impl Into<PathBuf>, restore_file: impl Into<PathBuf>) -> Self {
        Power { root: root.into(), restore_file: restore_file.into(), saved: Saved::default() }
    }

    fn policies(&self) -> Vec<PathBuf> {
        let dir = self.root.join("sys/devices/system/cpu/cpufreq");
        let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("policy")))
            .collect();
        v.sort();
        v
    }

    /// The thermal zone the PWM fan is a cooling device of.
    fn fan_zone(&self) -> Option<PathBuf> {
        fan_zone(&self.root)
    }

    /// The `active` trip points of the fan zone, in order: (file name of the temperature, temperature).
    pub(crate) fn active_trips(zone: &Path) -> Vec<(String, String)> {
        let mut v: Vec<(usize, String, String)> = Vec::new();
        for e in std::fs::read_dir(zone).into_iter().flatten().flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            let Some(idx) = name
                .strip_prefix("trip_point_")
                .and_then(|n| n.strip_suffix("_type"))
                .and_then(|n| n.parse::<usize>().ok())
            else {
                continue;
            };
            if read(&e.path()).as_deref() != Some("active") {
                continue;
            }
            let temp_file = format!("trip_point_{idx}_temp");
            if let Some(t) = read(&zone.join(&temp_file)) {
                v.push((idx, temp_file, t));
            }
        }
        v.sort();
        v.into_iter().map(|(_, f, t)| (f, t)).collect()
    }

    /// Writes the whole restore plan (`saved`) atomically. False (and a line on stderr) when it isn't on disk: the
    /// caller then changes nothing.
    fn persist(&self) -> bool {
        let written = serde_json::to_vec_pretty(&self.saved)
            .map_err(std::io::Error::other)
            .and_then(|json| crate::fsutil::write_atomic(&self.restore_file, &json));
        match written {
            Ok(()) => true,
            Err(e) => {
                eprintln!("power: can't write the restore note {}: {e}", self.restore_file.display());
                false
            }
        }
    }

    /// The plan in `self.saved` is on disk, or everything is as before: `before` is what was saved until now.
    /// False: nothing may be written (the caller says what wasn't applied).
    fn journal_or_rollback(&mut self, before: Saved, what: &str) -> bool {
        if self.persist() {
            return true;
        }
        self.saved = before;
        eprintln!("power: {what} not applied: without its restore note it couldn't be undone after a crash");
        false
    }

    /// Notes the governors and limits before the first change (in memory; the caller writes the note).
    fn note_cpu(&mut self) {
        if !self.saved.cpu.is_empty() {
            return;
        }
        for p in self.policies() {
            if let (Some(gov), Some(max)) = (read(&p.join("scaling_governor")), read(&p.join("scaling_max_freq"))) {
                self.saved.cpu.insert(p.file_name().unwrap().to_string_lossy().into_owned(), (gov, max));
            }
        }
    }

    fn note_fan(&mut self) -> Option<PathBuf> {
        let zone = self.fan_zone()?;
        if self.saved.fan.is_empty() {
            // Smooth fan drives the fan (its backstop is in the trips): these are not the system's trips to note.
            let backstop = crate::fan_curve::BACKSTOP_TRIP.to_string();
            if Self::active_trips(&zone).iter().any(|(_, t)| *t == backstop) {
                return None;
            }
            self.saved.fan_zone = zone.file_name().unwrap().to_string_lossy().into_owned();
            self.saved.fan = Self::active_trips(&zone).into_iter().collect();
        }
        Some(zone)
    }

    /// Applies a CPU profile. Returns false when nothing could be written (or the restore note couldn't be, so
    /// nothing was).
    pub fn set_performance(&mut self, mode: Performance) -> bool {
        if mode == Performance::Auto && self.saved.cpu.is_empty() {
            return true; // never touched: nothing to undo
        }
        let before = self.saved.clone();
        self.note_cpu();
        // Auto writes the originals back: that needs no note. Anything else does, before the first write.
        if mode != Performance::Auto && !self.journal_or_rollback(before, &format!("performance {}", mode.value())) {
            return false;
        }
        let mut ok = false;
        for p in self.policies() {
            let key = p.file_name().unwrap().to_string_lossy().into_owned();
            let Some((gov, max)) = self.saved.cpu.get(&key).cloned() else { continue };
            let (want_gov, want_max) = match mode {
                Performance::Auto => (gov, max),
                Performance::Performance => ("performance".to_string(), max),
                Performance::Battery => (gov, battery_cap(&p, &max)),
            };
            // Raising the limit first lets a governor change to `performance` take its full effect.
            ok |= write(&p.join("scaling_max_freq"), &want_max) & write(&p.join("scaling_governor"), &want_gov);
        }
        ok
    }

    /// Applies a fan profile (the trip temperatures of the fan's thermal zone).
    pub fn set_fan(&mut self, fan: Fan) -> bool {
        if crate::device::rocknix_cooling(&self.root) {
            return true;
        }
        if fan == Fan::Auto && self.saved.fan.is_empty() {
            return true;
        }
        let before = self.saved.clone();
        let Some(zone) = self.note_fan() else { return false };
        if fan != Fan::Auto && !self.journal_or_rollback(before, &format!("fan {}", fan.value())) {
            return false;
        }
        let originals = self.saved.fan.clone();
        let now = Self::active_trips(&zone);
        let mut ok = !now.is_empty();
        for (i, (file, _)) in now.iter().enumerate() {
            let want = match fan.trips() {
                Some(t) => t.get(i).map(|v| v.to_string()),
                None => originals.get(file).cloned(),
            };
            if let Some(v) = want {
                ok &= write(&zone.join(file), &v);
            }
        }
        ok
    }

    /// The GPU's devfreq devices (`/sys/class/devfreq/3d00000.gpu` on the SM8250).
    fn gpus(&self) -> Vec<PathBuf> {
        let mut v: Vec<PathBuf> = std::fs::read_dir(self.root.join("sys/class/devfreq"))
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.file_name().is_some_and(|n| n.to_string_lossy().contains("gpu")))
            .collect();
        v.sort();
        v
    }

    fn note_gpu(&mut self) {
        if !self.saved.gpu.is_empty() {
            return;
        }
        for g in self.gpus() {
            if let Some(gov) = read(&g.join("governor")) {
                self.saved.gpu.insert(g.file_name().unwrap().to_string_lossy().into_owned(), gov);
            }
        }
    }

    /// Notes `value` for `file` (below the root) as ours, with what was there before; written once the note is.
    fn plan_ours(&mut self, plan: &mut Vec<(PathBuf, String)>, file: PathBuf, value: &str, original: &str) {
        let rel = file.strip_prefix(&self.root).unwrap_or(&file).to_string_lossy().into_owned();
        self.saved.ours.insert(rel, (value.to_string(), original.to_string()));
        plan.push((file, value.to_string()));
    }

    /// A standalone emulator's game (PS2, GameCube, Wii, 3DS), after runemu.sh has set ROCKNIX's governor:
    /// Performance puts the big and prime clusters (every cpufreq policy but the first, the little one) and the
    /// GPU on `performance`; Battery caps those clusters like `set_performance`; Auto is what we found. The GPU
    /// stays under its own `max_freq` (pocket-ui's stock cap, `system_ctl::cap_gpu_if_unset`).
    pub fn set_standalone(&mut self, mode: Performance) -> bool {
        self.set_guest(mode, false)
    }

    /// A PortMaster port (`port_perf`), once its own process runs and runemu.sh has set ROCKNIX's governor: as
    /// [`Power::set_standalone`], except that Battery caps every cluster, the little one too, as pocket-core's Battery
    /// does. A cap is a `scaling_max_freq`, which ROCKNIX's governor switches (`performance`, `ondemand`, ...) leave alone.
    pub fn set_port(&mut self, mode: Performance) -> bool {
        self.set_guest(mode, true)
    }

    /// `set_standalone` and `set_port`: what is written is noted as ours, so `restore` only undoes what nobody changed since.
    /// The whole plan (every file, what goes in, what was there) is noted and written to the note first; only then
    /// is sysfs touched, so a crash at any point leaves a note that undoes all of it.
    fn set_guest(&mut self, mode: Performance, battery_caps_all: bool) -> bool {
        let before = self.saved.clone();
        self.note_cpu();
        self.note_gpu();
        self.saved.standalone = true;
        let policies = self.policies();
        let skip = usize::from(policies.len() > 1 && !(battery_caps_all && mode == Performance::Battery));
        // Per cluster: the limit, then the governor (raising the limit first lets `performance` take full effect).
        let mut clusters = Vec::new();
        for p in policies.into_iter().skip(skip) {
            let key = p.file_name().unwrap().to_string_lossy().into_owned();
            let Some((gov, max)) = self.saved.cpu.get(&key).cloned() else { continue };
            let (want_gov, want_max) = match mode {
                Performance::Auto => (gov.clone(), max.clone()),
                Performance::Performance => ("performance".to_string(), max.clone()),
                Performance::Battery => (gov.clone(), battery_cap(&p, &max)),
            };
            let mut pair = Vec::new();
            self.plan_ours(&mut pair, p.join("scaling_max_freq"), &want_max, &max);
            self.plan_ours(&mut pair, p.join("scaling_governor"), &want_gov, &gov);
            clusters.push(pair);
        }
        let mut gpus = Vec::new();
        for (dev, gov) in self.saved.gpu.clone() {
            let want = if mode == Performance::Performance { "performance" } else { gov.as_str() };
            self.plan_ours(&mut gpus, self.root.join("sys/class/devfreq").join(&dev).join("governor"), want, &gov);
        }
        if !self.journal_or_rollback(before, &format!("performance {}", mode.value())) {
            return false;
        }
        let mut ok = false;
        for pair in clusters {
            ok |= pair.iter().fold(true, |all, (f, v)| write(f, v) & all);
        }
        for (f, v) in gpus {
            ok &= write(&f, &v);
        }
        ok
    }

    /// Puts back everything we changed. After a standalone game only what is still as we wrote it: runemu.sh puts
    /// ROCKNIX's own governors back when the emulator ends, and those stay. The note goes only when every write
    /// back worked; otherwise it stays for the next `recover` (and the log says so). True when all is back.
    pub fn restore(&mut self) -> bool {
        let mut ok = true;
        if self.saved.standalone {
            for (rel, (wrote, original)) in std::mem::take(&mut self.saved.ours) {
                let file = self.root.join(&rel);
                if wrote != original && read(&file).as_deref() == Some(wrote.as_str()) {
                    ok &= write(&file, &original);
                }
            }
        } else if !self.saved.cpu.is_empty() {
            for p in self.policies() {
                let key = p.file_name().unwrap().to_string_lossy().into_owned();
                if let Some((gov, max)) = self.saved.cpu.get(&key) {
                    ok &= write(&p.join("scaling_max_freq"), max) & write(&p.join("scaling_governor"), gov);
                }
            }
        }
        ok &= self.set_fan(Fan::Auto);
        self.saved = Saved::default();
        if ok {
            let _ = std::fs::remove_file(&self.restore_file);
        } else {
            eprintln!("power: not everything went back; {} stays for the next try", self.restore_file.display());
        }
        ok
    }

    /// After a crash: undoes what a previous pocket-core (or port, or big game) left behind, if anything. A note
    /// that can't be read is never just deleted: it is kept as `<note>.bad` and the CPU goes to safe settings
    /// ([`SAFE_GOVERNOR`], every cluster's full speed), since what it held is unknown. True when something was undone.
    pub fn recover(&mut self) -> bool {
        let text = match std::fs::read_to_string(&self.restore_file) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return false,
            Err(e) => format!("unreadable: {e}"),
        };
        match serde_json::from_str::<Saved>(&text) {
            Ok(saved) => {
                self.saved = saved;
                self.restore();
                true
            }
            Err(e) => {
                let bad = PathBuf::from(format!("{}.bad", self.restore_file.display()));
                let kept = std::fs::rename(&self.restore_file, &bad).is_ok();
                let ok = self.safe_defaults();
                eprintln!(
                    "power: {} can't be read ({e}); {}; the CPU goes back to {SAFE_GOVERNOR} at full speed{}",
                    self.restore_file.display(),
                    if kept { format!("kept as {}", bad.display()) } else { "left where it is".to_string() },
                    if ok { "" } else { " (not every write worked)" }
                );
                true
            }
        }
    }

    /// Every cluster on [`SAFE_GOVERNOR`] with its full speed allowed (`cpuinfo_max_freq`). True when all worked.
    fn safe_defaults(&self) -> bool {
        let mut ok = true;
        for p in self.policies() {
            if let Some(top) = read(&p.join("cpuinfo_max_freq")) {
                ok &= write(&p.join("scaling_max_freq"), &top);
            }
            ok &= write(&p.join("scaling_governor"), SAFE_GOVERNOR);
        }
        ok
    }
}

/// The thermal zone below `root` the PWM fan is a cooling device of (also `fan_curve`'s).
pub(crate) fn fan_zone(root: &Path) -> Option<PathBuf> {
    let dir = root.join("sys/class/thermal");
    let mut zones: Vec<PathBuf> = std::fs::read_dir(dir).ok()?.flatten().map(|e| e.path()).filter(|p| p.file_name().is_some_and(|n| n.to_string_lossy().starts_with("thermal_zone"))).collect();
    zones.sort();
    zones.into_iter().find(|z| {
        std::fs::read_dir(z).into_iter().flatten().flatten().any(|e| {
            let name = e.file_name();
            let name = name.to_string_lossy();
            name.starts_with("cdev") && !name.contains('_') && read(&e.path().join("type")).as_deref() == Some("pwm-fan")
        })
    })
}

/// The fan zone's `active` trip temperatures now, in millidegrees and in order (read only; the live readout of
/// Settings > Performance & fan shows the first as "starts at").
pub fn fan_trips(root: &Path) -> Vec<i64> {
    fan_zone(root).map(|z| Power::active_trips(&z).into_iter().filter_map(|(_, t)| t.parse().ok()).collect()).unwrap_or_default()
}

/// `scaling_max_freq` for Battery: the lowest available frequency that is at least `BATTERY_SHARE` of the top one.
fn battery_cap(policy: &Path, current_max: &str) -> String {
    let top: u64 = read(&policy.join("cpuinfo_max_freq")).and_then(|s| s.parse().ok()).or_else(|| current_max.parse().ok()).unwrap_or(0);
    let want = (top as f64 * BATTERY_SHARE) as u64;
    let freqs: Vec<u64> = read(&policy.join("scaling_available_frequencies")).map(|s| s.split_whitespace().filter_map(|f| f.parse().ok()).collect()).unwrap_or_default();
    freqs.into_iter().filter(|f| *f >= want).min().map_or_else(|| current_max.to_string(), |f| f.to_string())
}

/// A fake sysfs for tests here and in `perf_fan`: three cpufreq clusters and the SM8250's fan zone (`hot` and
/// `critical` first, then the six `active` trips of ROCKNIX's quiet profile, then a `passive` one).
#[cfg(test)]
pub(crate) mod fake {
    use super::*;
    use crate::testutil::TestDir;

    pub struct Fake(pub crate::testutil::TestDir);

    impl Fake {
        pub fn new(name: &str) -> Fake {
            let root = TestDir::new(&format!("power-{name}"));
            for (p, gov, freqs) in [("policy0", "ondemand", "300000 979200 1804800"), ("policy4", "ondemand", "710400 1286400 1382400 2419200"), ("policy7", "schedutil", "844800 1401600 1516800 2841600")] {
                let d = root.join("sys/devices/system/cpu/cpufreq").join(p);
                std::fs::create_dir_all(&d).unwrap();
                let top = freqs.split(' ').next_back().unwrap();
                std::fs::write(d.join("scaling_governor"), format!("{gov}\n")).unwrap();
                std::fs::write(d.join("scaling_max_freq"), format!("{top}\n")).unwrap();
                std::fs::write(d.join("cpuinfo_max_freq"), format!("{top}\n")).unwrap();
                std::fs::write(d.join("scaling_available_frequencies"), format!("{freqs} \n")).unwrap();
            }
            // thermal_zone6 drives the fan (cdev0..cdev5 are all the pwm-fan); thermal_zone2 does not.
            let z = root.join("sys/class/thermal/thermal_zone6");
            std::fs::create_dir_all(&z).unwrap();
            let other = root.join("sys/class/thermal/thermal_zone2/cdev0");
            std::fs::create_dir_all(&other).unwrap();
            std::fs::write(other.join("type"), "cpufreq-cpu0\n").unwrap();
            for i in 0..6 {
                let c = z.join(format!("cdev{i}"));
                std::fs::create_dir_all(&c).unwrap();
                std::fs::write(c.join("type"), "pwm-fan\n").unwrap();
                std::fs::write(z.join(format!("cdev{i}_trip_point")), "2\n").unwrap();
            }
            for (i, (kind, t)) in [("hot", 90000), ("critical", 110000), ("active", 52000), ("active", 62000), ("active", 69000), ("active", 75000), ("active", 80000), ("active", 85000), ("passive", 95000)].iter().enumerate() {
                std::fs::write(z.join(format!("trip_point_{i}_type")), format!("{kind}\n")).unwrap();
                std::fs::write(z.join(format!("trip_point_{i}_temp")), format!("{t}\n")).unwrap();
            }
            Fake(root)
        }

        pub fn power(&self) -> Power {
            Power::with_root(&self.0, self.0.join("restore.json"))
        }

        pub fn cpu(&self, policy: &str, file: &str) -> String {
            read(&self.0.join("sys/devices/system/cpu/cpufreq").join(policy).join(file)).unwrap()
        }

        pub fn trip(&self, i: usize) -> String {
            read(&self.0.join(format!("sys/class/thermal/thermal_zone6/trip_point_{i}_temp"))).unwrap()
        }
    }

}

#[cfg(test)]
mod tests {
    use super::fake::Fake;
    use super::*;
    use crate::testutil::TestDir;

    #[test]
    fn sm8550_discovers_policy_3_and_never_writes_fan_trips() {
        let f = Fake::new("sm8550");
        let policies = f.0.join("sys/devices/system/cpu/cpufreq");
        std::fs::rename(policies.join("policy4"), policies.join("policy3")).unwrap();
        let dt = f.0.join("sys/firmware/devicetree/base");
        std::fs::create_dir_all(&dt).unwrap();
        std::fs::write(dt.join("compatible"), b"retroidpocket,rp6\0qcom,qcs8550\0qcom,sm8550\0").unwrap();
        let before: Vec<_> = (0..9).map(|i| f.trip(i)).collect();
        let mut power = f.power();
        assert!(power.set_fan(Fan::Cool));
        assert!(power.set_fan(Fan::Quiet));
        assert!(power.set_performance(Performance::Performance));
        for policy in ["policy0", "policy3", "policy7"] {
            assert_eq!(f.cpu(policy, "scaling_governor"), "performance");
        }
        assert!(power.restore());
        assert_eq!(f.cpu("policy3", "scaling_governor"), "ondemand");
        assert_eq!((0..9).map(|i| f.trip(i)).collect::<Vec<_>>(), before);
    }

    #[test]
    fn auto_performance_and_auto_fan_touch_nothing() {
        let f = Fake::new("untouched");
        let mut p = f.power();
        assert!(p.set_performance(Performance::Auto) && p.set_fan(Fan::Auto));
        assert!(!f.0.join("restore.json").exists(), "no files, no restore note");
        assert_eq!((f.cpu("policy0", "scaling_governor"), f.trip(2)), ("ondemand".to_string(), "52000".to_string()));
    }

    #[test]
    fn performance_sets_every_cluster_and_restore_brings_back_each_governor() {
        let f = Fake::new("perf");
        let mut p = f.power();
        assert!(p.set_performance(Performance::Performance));
        for pol in ["policy0", "policy4", "policy7"] {
            assert_eq!(f.cpu(pol, "scaling_governor"), "performance");
        }
        assert_eq!(f.cpu("policy7", "scaling_max_freq"), "2841600");
        assert!(f.0.join("restore.json").exists());
        p.restore();
        assert_eq!(f.cpu("policy0", "scaling_governor"), "ondemand");
        assert_eq!(f.cpu("policy7", "scaling_governor"), "schedutil", "the prime cluster had its own");
        assert!(!f.0.join("restore.json").exists());
    }

    #[test]
    fn battery_caps_every_cluster_at_half_its_top_speed() {
        let f = Fake::new("battery");
        let mut p = f.power();
        assert!(p.set_performance(Performance::Battery));
        assert_eq!(f.cpu("policy0", "scaling_max_freq"), "979200"); // >= 902400
        assert_eq!(f.cpu("policy4", "scaling_max_freq"), "1286400"); // >= 1209600
        assert_eq!(f.cpu("policy7", "scaling_max_freq"), "1516800"); // >= 1420800
        assert_eq!(f.cpu("policy4", "scaling_governor"), "ondemand", "governor untouched");
        // Changing the profile starts from the originals, not from the capped values.
        assert!(p.set_performance(Performance::Performance));
        assert_eq!((f.cpu("policy4", "scaling_max_freq"), f.cpu("policy4", "scaling_governor")), ("2419200".to_string(), "performance".to_string()));
        assert!(p.set_performance(Performance::Auto));
        assert_eq!((f.cpu("policy4", "scaling_max_freq"), f.cpu("policy4", "scaling_governor")), ("2419200".to_string(), "ondemand".to_string()));
    }

    #[test]
    fn fan_profiles_write_the_six_active_trips_and_leave_the_others() {
        let f = Fake::new("fan");
        let mut p = f.power();
        assert!(p.set_fan(Fan::Cool));
        let got: Vec<String> = (0..9).map(|i| f.trip(i)).collect();
        assert_eq!(got, ["90000", "110000", "40000", "48000", "55000", "61000", "66000", "71000", "95000"]);
        assert!(p.set_fan(Fan::Quiet));
        assert_eq!(f.trip(2), "52000");
        assert_eq!(f.trip(7), "85000");
        assert!(p.set_fan(Fan::Cool));
        assert!(p.set_fan(Fan::Auto));
        assert_eq!((f.trip(2), f.trip(5), f.trip(7)), ("52000".to_string(), "75000".to_string(), "85000".to_string()), "Auto gives back the system's own");
    }

    /// The trips that are not the fan's (`hot`, `critical`, `passive`) as the fake has them.
    fn others(f: &Fake) -> [String; 3] {
        [f.trip(0), f.trip(1), f.trip(8)]
    }

    #[test]
    fn hot_critical_and_passive_trips_are_never_written() {
        let f = Fake::new("others");
        let before = others(&f);
        assert_eq!(before, ["90000", "110000", "95000"]);
        let mut p = f.power();
        for fan in [Fan::Cool, Fan::Quiet, Fan::Cool, Fan::Auto, Fan::Cool] {
            assert!(p.set_fan(fan));
            assert_eq!(others(&f), before, "{fan:?}");
        }
        p.restore();
        assert_eq!(others(&f), before);
        p.set_fan(Fan::Quiet);
        drop(p);
        assert!(f.power().recover());
        assert_eq!(others(&f), before, "a recovery after a crash writes only the active trips too");
        assert_eq!(fan_trips(&f.0), [52000, 62000, 69000, 75000, 80000, 85000]);
    }

    #[test]
    fn the_fan_trips_are_read_in_order_and_the_profiles_say_where_they_start() {
        let f = Fake::new("trips");
        let mut p = f.power();
        p.set_fan(Fan::Cool);
        assert_eq!(fan_trips(&f.0)[0], 40000);
        assert_eq!((Fan::Quiet.starts_at(), Fan::Cool.starts_at(), Fan::Auto.starts_at()), (Some(52), Some(40), None));
        let empty = TestDir::new("power-notrips");
        assert!(fan_trips(&empty).is_empty());
    }

    #[test]
    fn trip_profiles_keep_off_while_smooth_fan_drives_the_fan() {
        let f = Fake::new("curve-owned");
        for i in 2..8 {
            std::fs::write(f.0.join(format!("sys/class/thermal/thermal_zone6/trip_point_{i}_temp")), "83500\n").unwrap();
        }
        let mut p = f.power();
        assert!(!p.set_fan(Fan::Cool), "the curve's backstop is never noted as the system's trips");
        assert_eq!(f.trip(2), "83500");
        assert!(!f.0.join("restore.json").exists());
    }

    #[test]
    fn values_are_the_ones_of_the_settings_files() {
        for p in [Performance::Battery, Performance::Auto, Performance::Performance] {
            assert_eq!(Performance::parse(p.value()), p);
            assert_eq!(serde_json::to_string(&p).unwrap(), format!("\"{}\"", p.value()));
        }
        // Auto was called Balanced until 2026-10-23: files that say so still read as Auto, and are written as "auto".
        assert_eq!(Performance::parse("balanced"), Performance::Auto);
        assert_eq!(Performance::from_value("balanced"), Some(Performance::Auto));
        assert_eq!(serde_json::from_str::<Performance>("\"balanced\"").unwrap(), Performance::Auto);
        assert_eq!(serde_json::to_string(&Performance::Auto).unwrap(), "\"auto\"");
        for f in [Fan::Quiet, Fan::Auto, Fan::Cool] {
            assert_eq!(Fan::parse(f.value()), f);
            assert_eq!(serde_json::from_str::<Fan>(&format!("\"{}\"", f.value())).unwrap(), f);
        }
    }

    #[test]
    fn restore_puts_back_a_custom_profile_too() {
        let f = Fake::new("custom");
        std::fs::write(f.0.join("sys/class/thermal/thermal_zone6/trip_point_3_temp"), "58000\n").unwrap();
        let mut p = f.power();
        p.set_fan(Fan::Cool);
        assert_eq!(f.trip(3), "48000");
        p.restore();
        assert_eq!(f.trip(3), "58000");
    }

    #[test]
    fn a_crashed_game_is_undone_from_the_restore_note() {
        let f = Fake::new("crash");
        let mut first = f.power();
        first.set_performance(Performance::Performance);
        first.set_fan(Fan::Cool);
        drop(first); // pocket-core was killed: nothing restored
        assert_eq!(f.cpu("policy0", "scaling_governor"), "performance");
        let mut second = f.power();
        assert!(second.recover());
        assert_eq!((f.cpu("policy0", "scaling_governor"), f.cpu("policy7", "scaling_governor"), f.trip(2)), ("ondemand".to_string(), "schedutil".to_string(), "52000".to_string()));
        assert!(!second.recover(), "nothing left to undo");
    }

    #[test]
    fn systems_without_a_fan_or_cpufreq_do_not_fail_loudly() {
        let root = TestDir::new("power-empty");
        let mut p = Power::with_root(root.to_path_buf(), root.join("r.json"));
        assert!(!p.set_fan(Fan::Cool));
        assert!(!p.set_performance(Performance::Performance));
        p.restore();
    }

    // ---- standalone emulators (PS2, GameCube, Wii, 3DS)

    fn with_gpu(f: &Fake) {
        for (dev, gov) in [("3d00000.gpu", "simple_ondemand"), ("soc:qcom,cpu-cpu-llcc-bw", "bw_hwmon")] {
            let d = f.0.join("sys/class/devfreq").join(dev);
            std::fs::create_dir_all(&d).unwrap();
            std::fs::write(d.join("governor"), format!("{gov}\n")).unwrap();
        }
    }

    fn gpu(f: &Fake, dev: &str) -> String {
        read(&f.0.join("sys/class/devfreq").join(dev).join("governor")).unwrap()
    }

    #[test]
    fn max_for_a_big_game_takes_the_big_cores_and_the_gpu_and_leaves_the_little_ones() {
        let f = Fake::new("standalone");
        with_gpu(&f);
        let mut p = f.power();
        assert!(p.set_standalone(Performance::Performance));
        assert_eq!(f.cpu("policy0", "scaling_governor"), "ondemand", "little cores as ROCKNIX set them");
        assert_eq!(f.cpu("policy4", "scaling_governor"), "performance");
        assert_eq!(f.cpu("policy7", "scaling_governor"), "performance");
        assert_eq!(gpu(&f, "3d00000.gpu"), "performance");
        assert_eq!(gpu(&f, "soc:qcom,cpu-cpu-llcc-bw"), "bw_hwmon", "only the GPU's devfreq");
        assert!(f.0.join("restore.json").exists());
        // Battery caps the big cores instead, from the originals; Auto is ROCKNIX's own again.
        assert!(p.set_standalone(Performance::Battery));
        assert_eq!((f.cpu("policy4", "scaling_governor"), f.cpu("policy4", "scaling_max_freq")), ("ondemand".to_string(), "1286400".to_string()));
        assert_eq!(f.cpu("policy0", "scaling_max_freq"), "1804800");
        assert_eq!(gpu(&f, "3d00000.gpu"), "simple_ondemand");
        assert!(p.set_standalone(Performance::Auto));
        assert_eq!((f.cpu("policy7", "scaling_governor"), f.cpu("policy7", "scaling_max_freq")), ("schedutil".to_string(), "2841600".to_string()));
        p.restore();
        assert!(!f.0.join("restore.json").exists());
    }

    #[test]
    fn a_port_on_battery_caps_every_cluster_and_gets_back_what_it_found() {
        let f = Fake::new("port");
        with_gpu(&f);
        let mut p = f.power();
        assert!(p.set_port(Performance::Battery));
        assert_eq!(f.cpu("policy0", "scaling_max_freq"), "979200", "the little cores too, as pocket-core's Battery");
        assert_eq!(f.cpu("policy4", "scaling_max_freq"), "1286400");
        assert_eq!(f.cpu("policy7", "scaling_max_freq"), "1516800");
        assert_eq!(f.cpu("policy7", "scaling_governor"), "schedutil", "ROCKNIX's governor stays");
        assert_eq!(gpu(&f, "3d00000.gpu"), "simple_ondemand");
        // runemu.sh's clean-up after the port: `performance`, then the system's governor. Only our caps go back.
        let cpufreq = f.0.join("sys/devices/system/cpu/cpufreq");
        for pol in ["policy0", "policy4", "policy7"] {
            std::fs::write(cpufreq.join(pol).join("scaling_governor"), "ondemand\n").unwrap();
        }
        p.restore();
        assert_eq!((f.cpu("policy0", "scaling_max_freq"), f.cpu("policy7", "scaling_max_freq")), ("1804800".to_string(), "2841600".to_string()));
        assert_eq!(f.cpu("policy7", "scaling_governor"), "ondemand", "runemu's choice stays");
        assert!(!f.0.join("restore.json").exists());
        // Performance: the big and prime clusters and the GPU, as for PS2; the little ones as ROCKNIX set them.
        let mut p = f.power();
        assert!(p.set_port(Performance::Performance));
        assert_eq!((f.cpu("policy0", "scaling_governor"), f.cpu("policy4", "scaling_governor")), ("ondemand".to_string(), "performance".to_string()));
        assert_eq!(gpu(&f, "3d00000.gpu"), "performance");
        drop(p); // pocket-ui crashed with the port running
        assert!(f.power().recover());
        assert_eq!((f.cpu("policy4", "scaling_governor"), gpu(&f, "3d00000.gpu")), ("ondemand".to_string(), "simple_ondemand".to_string()));
    }

    #[test]
    fn a_big_game_ending_puts_back_only_what_is_still_ours() {
        let f = Fake::new("standalone-end");
        with_gpu(&f);
        let mut p = f.power();
        p.set_standalone(Performance::Performance);
        // runemu.sh's own clean-up after the emulator ended: its system governor on every cluster, the GPU on auto.
        let cpufreq = f.0.join("sys/devices/system/cpu/cpufreq");
        std::fs::write(cpufreq.join("policy4/scaling_governor"), "schedutil\n").unwrap();
        std::fs::write(f.0.join("sys/class/devfreq/3d00000.gpu/governor"), "msm-adreno-tz\n").unwrap();
        p.restore();
        assert_eq!(f.cpu("policy4", "scaling_governor"), "schedutil", "runemu's choice stays");
        assert_eq!(gpu(&f, "3d00000.gpu"), "msm-adreno-tz");
        assert_eq!(f.cpu("policy7", "scaling_governor"), "schedutil", "still ours: back to what it was");
        assert!(!f.0.join("restore.json").exists());
    }

    #[test]
    fn a_big_game_left_behind_by_a_crash_is_undone_at_the_next_start() {
        let f = Fake::new("standalone-crash");
        with_gpu(&f);
        let mut first = Power::with_root(&f.0, f.0.join("standalone.json"));
        first.set_standalone(Performance::Performance);
        drop(first); // pocket-ui died, and runemu.sh was killed with the game
        let mut second = Power::with_root(&f.0, f.0.join("standalone.json"));
        assert!(second.recover());
        assert_eq!((f.cpu("policy4", "scaling_governor"), f.cpu("policy7", "scaling_governor")), ("ondemand".to_string(), "schedutil".to_string()));
        assert_eq!(gpu(&f, "3d00000.gpu"), "simple_ondemand");
        assert!(!second.recover());
    }

    // ---- review SOL 2026-10-24 #2: the note first, and never thrown away unread

    #[test]
    fn without_its_restore_note_nothing_is_changed() {
        let f = Fake::new("no-note");
        with_gpu(&f);
        let tops = || {
            (
                f.cpu("policy0", "scaling_max_freq"),
                f.cpu("policy4", "scaling_max_freq"),
                gpu(&f, "3d00000.gpu"),
            )
        };
        let before = tops();
        // /tmp is full: the note can't be written. A port's Battery, pocket-core's Battery and a fan profile all
        // leave the system as it was (the game still runs, with the system's settings).
        let mut p = f.power();
        crate::fsutil::fail_next_write();
        assert!(!p.set_port(Performance::Battery));
        assert_eq!(tops(), before, "no cap without a note");
        crate::fsutil::fail_next_write();
        assert!(!p.set_port(Performance::Performance));
        assert_eq!(f.cpu("policy4", "scaling_governor"), "ondemand");
        assert_eq!(gpu(&f, "3d00000.gpu"), "simple_ondemand", "the GPU neither");
        crate::fsutil::fail_next_write();
        assert!(!p.set_performance(Performance::Battery));
        assert_eq!(tops(), before);
        crate::fsutil::fail_next_write();
        assert!(!p.set_fan(Fan::Cool));
        assert_eq!(f.trip(2), "52000");
        assert!(!f.0.join("restore.json").exists());
        // Nothing was noted either: a restore now changes nothing.
        p.restore();
        assert_eq!(tops(), before);
        // With the note written, the whole plan is in it (every cluster and the GPU), so a crash right after the
        // first write is undone completely.
        let mut p = f.power();
        assert!(p.set_port(Performance::Performance));
        let note: Saved = serde_json::from_str(&std::fs::read_to_string(f.0.join("restore.json")).unwrap()).unwrap();
        assert!(note.ours.keys().any(|k| k.ends_with("3d00000.gpu/governor")), "{:?}", note.ours.keys());
        assert!(note.ours.keys().any(|k| k.ends_with("policy7/scaling_governor")));
        assert!(p.restore());
        assert!(!f.0.join("restore.json").exists());
    }

    #[test]
    fn a_note_that_cant_be_read_is_kept_and_the_cpu_goes_to_safe_settings() {
        let f = Fake::new("bad-note");
        let cpufreq = f.0.join("sys/devices/system/cpu/cpufreq");
        // A port's Battery and a Performance governor left behind; the note is cut off.
        std::fs::write(cpufreq.join("policy4/scaling_max_freq"), "1286400\n").unwrap();
        std::fs::write(cpufreq.join("policy7/scaling_governor"), "performance\n").unwrap();
        std::fs::write(f.0.join("restore.json"), b"{\"cpu\": {\"policy4\": [\"ondem").unwrap();
        assert!(f.power().recover());
        assert_eq!(std::fs::read(f.0.join("restore.json.bad")).unwrap(), b"{\"cpu\": {\"policy4\": [\"ondem", "kept, not deleted");
        assert!(!f.0.join("restore.json").exists());
        for pol in ["policy0", "policy4", "policy7"] {
            assert_eq!(f.cpu(pol, "scaling_governor"), "schedutil", "{pol}");
            assert_eq!(f.cpu(pol, "scaling_max_freq"), f.cpu(pol, "cpuinfo_max_freq"), "{pol}: full speed");
        }
    }

    #[test]
    fn a_restore_that_fails_keeps_its_note_for_the_next_try() {
        let f = Fake::new("restore-fails");
        let mut p = f.power();
        assert!(p.set_performance(Performance::Battery));
        // The cluster's limit can't be written right now.
        let max = f.0.join("sys/devices/system/cpu/cpufreq/policy4/scaling_max_freq");
        std::fs::remove_file(&max).unwrap();
        std::fs::create_dir(&max).unwrap();
        assert!(!p.restore());
        assert!(f.0.join("restore.json").exists(), "what isn't back yet stays noted");
        std::fs::remove_dir(&max).unwrap();
        std::fs::write(&max, "1286400\n").unwrap();
        assert!(f.power().recover());
        assert_eq!(f.cpu("policy4", "scaling_max_freq"), "2419200");
        assert!(!f.0.join("restore.json").exists());
    }

    // ---- review SOL 2026-10-24 #1: one owner of the CPU at a time

    #[test]
    fn the_next_game_notes_the_cpu_only_after_the_game_before_put_it_back() {
        // The switcher's hand-over between two pocket-core games: the old process puts the CPU back and only then
        // says "saved", and the new one starts at once. It takes the lease before it notes the originals, so it
        // never notes the old game's caps as the system's.
        let f = Fake::new("handover");
        let lease = f.0.join("perf.lock");
        let (applied, go) = (std::sync::mpsc::channel(), std::sync::mpsc::channel::<()>());
        let old = {
            let (root, lease) = (f.0.to_path_buf(), lease.to_path_buf());
            let (applied, go) = (applied.0, go.1);
            std::thread::spawn(move || {
                let l = Lease::take(&lease, LEASE_WAIT, || false).expect("free");
                let mut p = Power::with_root(root.to_path_buf(), root.join("old.json"));
                assert!(p.set_performance(Performance::Battery));
                applied.send(()).unwrap();
                go.recv().unwrap();
                // The exit save takes a moment; then the CPU goes back, then the lease.
                std::thread::sleep(Duration::from_millis(150));
                assert!(p.restore());
                drop(l);
            })
        };
        applied.1.recv().unwrap();
        assert_eq!(f.cpu("policy4", "scaling_max_freq"), "1286400", "the old game's cap");
        go.0.send(()).unwrap();
        // The new game's start.
        let t0 = Instant::now();
        let l = Lease::take(&lease, LEASE_WAIT, || false).expect("after the old game's restore");
        assert!(t0.elapsed() >= Duration::from_millis(100), "it waited for the restore");
        let mut p = Power::with_root(&f.0, f.0.join("new.json"));
        assert!(p.set_performance(Performance::Battery));
        let note: Saved = serde_json::from_str(&std::fs::read_to_string(f.0.join("new.json")).unwrap()).unwrap();
        assert_eq!(note.cpu["policy4"].1, "2419200", "the system's limit, not the old game's cap");
        p.restore();
        drop(l);
        old.join().unwrap();
        assert_eq!(f.cpu("policy4", "scaling_max_freq"), "2419200");
    }

    #[test]
    fn the_lease_is_one_owners_and_a_wait_can_be_given_up() {
        let f = Fake::new("lease");
        let path = f.0.join("perf.lock");
        let a = Lease::take(&path, Duration::ZERO, || false).expect("free");
        assert!(Lease::take(&path, Duration::from_millis(30), || false).is_none(), "held");
        // A wait ends because it was given up, not because the clock ran out: the callback ending it is the event (the
        // wall clock said how loaded the machine was, and this took over 1 s in a gate).
        let gave_up = std::sync::atomic::AtomicBool::new(false);
        assert!(Lease::take(&path, Duration::from_secs(5), || {
            gave_up.store(true, std::sync::atomic::Ordering::SeqCst);
            true
        })
        .is_none());
        assert!(gave_up.load(std::sync::atomic::Ordering::SeqCst), "the 5 s wait was not given up");
        drop(a);
        // Free again: the next attempt gets it. Not the *first* one: a child another test thread forked (every
        // `Command::spawn` in the tests) has a copy of this descriptor until it execs, and flock is released only when
        // the last copy is gone (the window that makes `testutil::write_executable` write through a child). So this
        // waits for the lease as the games do, and a lease that is never given back still fails here.
        assert!(Lease::take(&path, LEASE_WAIT, || false).is_some(), "free again");
        assert!(Lease::take(&f.0.join("no/such/dir/lock"), Duration::ZERO, || false).is_none());
    }

    #[test]
    fn choices_parse_with_fallbacks() {
        assert_eq!(Performance::parse("battery"), Performance::Battery);
        assert_eq!(Performance::parse("performance"), Performance::Performance);
        assert_eq!(Performance::parse("whatever"), Performance::Auto);
        assert_eq!(Performance::from_value("whatever"), None);
        assert_eq!((Fan::parse("quiet"), Fan::parse("cool"), Fan::parse("auto"), Fan::parse("")), (Fan::Quiet, Fan::Cool, Fan::Auto, Fan::Auto));
    }
}
