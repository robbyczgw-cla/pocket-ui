//! Performance and fan for PortMaster ports, which have no pause menu: each port's own choice (from its card, START →
//! Performance & fan, `port_perf_page`), applied while it runs and put back after it, and a cheap look at how it uses
//! the processor, for one calm hint when a port keeps a core busy for nothing.
//!
//! * **The choice** is kept as pocket-core's are (`options::GameSettings`, `game-settings/ports/<port>.json`, keys
//!   `performance` and `fan`), so the order is the same: the port's own, a file for all ports, Settings →
//!   Performance & fan's Games: performance and Games: fan, the system's. [`Store`] holds every port's choice and
//!   note in memory (read on a thread at the start with no lock held, then published whole; written through the card
//!   writer), so neither the card, the page nor a port's start reads a file on the UI thread or waits for one: until
//!   the store is there [`with`] says None, the card says "Loading…" and a port's start decides on its thread
//!   ([`Wanted::Later`]).
//! * **Performance** goes on once the port's own process runs ([`Tracker`]: seen as `bash <launcher>`, or, for a
//!   launcher that `exec`s its game before a look sees it, as a lasting child of runemu.sh that is none of its
//!   helpers): ROCKNIX's runemu.sh switches every cluster to `performance` when it starts, sets the port's governor
//!   (`cpugovernor`) just before it runs the port's script and `performance` again after it, and the system's governor
//!   when it quits; it never writes `scaling_max_freq`. So Battery's caps (`power::Power::set_port`) don't fight it, and
//!   Performance's governors are only put back where they are still ours. When it can't be applied the log says why.
//! * **One session at a time**: the session's thread holds the CPU's lease (`power::Lease`) from before its note until
//!   it has put everything back, and checks that the port still runs only once it holds it. The next game (another
//!   port, pocket-core, a big emulator) takes that lease before it notes the CPU, so it never notes a port's caps as the
//!   system's, and an ended session never writes over a newer one's settings or note. The UI thread only says the
//!   session ended ([`ended`]). The note [`RESTORE_FILE`] (with the port's process group in [`PGID_FILE`]) lets a crash
//!   be undone: [`recover`] at pocket-ui's start once that group is gone, and `pocket-core --restore-system` in the
//!   launcher's clean-up after it stopped the game.
//! * **The fan** is Smooth fan's curve (`fan_curve`): the App gives the port's choice to the plan, and a choice of the
//!   port's own wins over ROCKNIX's per-game cooling profile (`fan_curve::Plan::own`).
//! * **Busy-wait** ([`Meter`]): from 30 s after the port started, every 10 s, the CPU time of the port's threads (its
//!   process group) and of the whole Pocket (`/proc/stat`). Twelve windows (two minutes), then it stops: when in most
//!   of them one thread used a whole core while everything else together used little, the port spins (GameMaker's
//!   gmloader waiting for the next frame, for one). The verdict is kept in [`NOTES_FILE`]; the card says "Busy core?
//!   Press START" until the page has been opened once, and the page explains it ([`HINT`]) and offers Battery in one
//!   press. Nothing changes by itself.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::power::{Fan, Lease, Performance, Power};

/// ROCKNIX's system id for PortMaster ports.
pub const SYSTEM: &str = "ports";
/// What `Power::set_port` changed while a port runs.
pub const RESTORE_FILE: &str = "/tmp/pocket-ui-port.restore";
/// The process group of that port: the note is only undone once it is gone.
pub const PGID_FILE: &str = "/tmp/pocket-ui-port.pgid";
/// The verdicts of the busy-wait look, per port (`<data>/port-cpu.json`).
pub const NOTES_FILE: &str = "port-cpu.json";
/// The card's and the page's hint.
pub const HINT: &str = "This port keeps one core busy all the time. Battery performance keeps it cooler and quieter; if it stutters, go back to Auto.";

/// How long the port gets to start before its first window, and how long a window is.
pub const WARMUP: Duration = Duration::from_secs(30);
pub const WINDOW: Duration = Duration::from_secs(10);
/// Windows looked at before the verdict.
pub const WINDOWS: usize = 12;
/// One thread at least this much of a core ...
pub const BUSY_TOP: f32 = 0.85;
/// ... while everything else on the Pocket together uses at most this many cores ...
pub const REST_MAX: f32 = 0.6;
/// ... in at least this share of the windows.
const BUSY_SHARE: f32 = 0.75;
/// How long the port's own process gets to appear after runemu.sh started.
const START_WAIT: Duration = Duration::from_secs(90);

// ---- the choice and the notes, in memory --------------------------------------------------------------

/// What a port runs with, and which of it is the port's own (else it comes from Settings → Performance & fan).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Choice {
    pub performance: Performance,
    pub fan: Fan,
    pub own_performance: bool,
    pub own_fan: bool,
}

/// The settings files' value of a performance or fan choice; None for anything else (a hand-edited file). The old
/// "balanced" is Auto (`Performance::from_value`).
fn performance_of(v: &str) -> Option<Performance> {
    Performance::from_value(v)
}

fn fan_of(v: &str) -> Option<Fan> {
    [Fan::VeryQuiet, Fan::Quiet, Fan::Auto, Fan::Cool].into_iter().find(|f| f.value() == v)
}

/// Every port's own choice and what the busy-core look found, in memory, so the card, the page and a port's start
/// never read a file on the UI thread. Loaded once on a thread at pocket-ui's start ([`Store::load`], a few small
/// files); a change is made here first and handed to the card writer (`fsutil::write_json_soon`).
///
/// The files are pocket-core's (`options::GameSettings`): `game-settings/ports/<port>.json` (the port's own, keys
/// `performance` and `fan`), else `game-settings/ports.json` (a file for every port, written by no page); below them
/// Settings → Performance & fan's Games: performance and Games: fan, which the App keeps in memory and passes in.
#[derive(Clone, Debug, Default)]
pub struct Store {
    dir: PathBuf,
    console: BTreeMap<String, String>,
    /// By the launcher's name without its extension ("Deltarune"), as `GameSettings` names the file.
    games: HashMap<String, BTreeMap<String, String>>,
    notes: Notes,
}

/// The App's store and the busy-core look's thread share it; None until it is loaded.
pub type Shared = Arc<Mutex<Option<Store>>>;

/// The launcher's name without its extension: the port's settings file.
fn stem(rom: &Path) -> String {
    rom.file_stem().and_then(|n| n.to_str()).unwrap_or_default().to_string()
}

impl Store {
    /// Reads the ports' settings and notes of `data_dir`. Not on the UI thread: [`spawn_load`].
    pub fn load(data_dir: &Path) -> Store {
        let dir = data_dir.join("game-settings");
        let read = |p: &Path| crate::fsutil::read_json_or_backup::<BTreeMap<String, String>>(p);
        let games = std::fs::read_dir(dir.join(SYSTEM))
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| {
                let p = e.path();
                let name = p.file_name()?.to_str()?.strip_suffix(".json")?.to_string();
                Some((name, read(&p)?))
            })
            .collect();
        Store {
            dir: data_dir.to_path_buf(),
            console: read(&dir.join(format!("{SYSTEM}.json"))).unwrap_or_default(),
            games,
            notes: crate::fsutil::read_json_or_backup(&data_dir.join(NOTES_FILE)).unwrap_or_default(),
        }
    }

    /// The choice for the port `rom` (its launcher, `/storage/roms/ports/Deltarune.sh`); `defaults` are Settings →
    /// Performance & fan's Games: performance and Games: fan. As `GameSettings`: the port's own file, else the file
    /// for every port, else the defaults; a value that isn't a choice counts as not there.
    pub fn choice(&self, rom: &Path, defaults: (Performance, Fan)) -> Choice {
        // An empty file of its own (left by "Use the defaults") is no choice of its own.
        let own = self.games.get(&stem(rom)).filter(|m| !m.is_empty());
        let values = own.unwrap_or(&self.console);
        let performance = values.get("performance").and_then(|v| performance_of(v));
        let fan = values.get("fan").and_then(|v| fan_of(v));
        Choice {
            performance: performance.unwrap_or(defaults.0),
            fan: fan.unwrap_or(defaults.1),
            own_performance: own.is_some() && performance.is_some(),
            own_fan: own.is_some() && fan.is_some(),
        }
    }

    /// Sets the port's own `key` ("performance" or "fan") to `value`; None: back to the defaults (both keys go, the
    /// file stays with whatever else it holds, `{}` at least, which counts as nothing of its own).
    pub fn set(&mut self, rom: &Path, change: Option<(&str, &str)>) {
        let values = self.games.entry(stem(rom)).or_default();
        match change {
            Some((key, value)) => {
                values.insert(key.to_string(), value.to_string());
            }
            None => {
                values.remove("performance");
                values.remove("fan");
            }
        }
        let path = self.dir.join("game-settings").join(SYSTEM).join(format!("{}.json", stem(rom)));
        crate::fsutil::write_json_soon(&path, values);
    }

    /// What the busy-core look found for the port, if it looked.
    pub fn note(&self, rom: &Path) -> Option<&Note> {
        self.notes.get(&key(rom))
    }

    /// Keeps a new verdict; whether the hint was seen stays as it was.
    pub fn keep(&mut self, rom: &Path, mut n: Note) {
        n.seen = self.note(rom).is_some_and(|old| old.seen);
        self.notes.insert(key(rom), n);
        self.write_notes();
    }

    /// The hint was shown on the page: the card no longer shows it.
    pub fn mark_seen(&mut self, rom: &Path) {
        if let Some(n) = self.notes.get_mut(&key(rom)).filter(|n| !n.seen) {
            n.seen = true;
            self.write_notes();
        }
    }

    fn write_notes(&self) {
        crate::fsutil::write_json_soon(&self.dir.join(NOTES_FILE), &self.notes);
    }
}

/// A store, read on a thread of its own and published whole once it is read: nobody waits for the files.
pub fn spawn_load(data_dir: &Path) -> Shared {
    let dir = data_dir.to_path_buf();
    spawn_load_with(move || Store::load(&dir))
}

/// [`spawn_load`] with the reading given (tests: a slow card).
fn spawn_load_with(load: impl FnOnce() -> Store + Send + 'static) -> Shared {
    let shared = Shared::default();
    let publish = {
        let s = shared.clone();
        move |store: Store| {
            let mut guard = s.lock().unwrap_or_else(|e| e.into_inner());
            if guard.is_none() {
                *guard = Some(store);
            }
        }
    };
    // The files are read with no lock held (review SOL 2026-10-24 #6): the UI only ever takes the lock for a look
    // at memory, and finds None until the finished store is put there.
    let load = Arc::new(Mutex::new(Some(load)));
    let take = |l: &Mutex<Option<_>>| l.lock().unwrap_or_else(|e| e.into_inner()).take();
    let spawned = {
        let (load, publish) = (load.clone(), publish.clone());
        std::thread::Builder::new().name("port-perf-load".into()).spawn(move || {
            if let Some(f) = take(&load) {
                publish(f());
            }
        })
    };
    if spawned.is_err() {
        // No thread to be had (never seen): read here once, rather than "loading" for ever.
        if let Some(f) = take(&load) {
            publish(f());
        }
    }
    shared
}

/// `f` with the store, from memory. None while it is still being read (the first moments after pocket-ui's start):
/// the caller shows that, or leaves what needs it for later. Never reads a file and never waits for the reading.
pub fn with<T>(shared: &Shared, f: impl FnOnce(&mut Store) -> T) -> Option<T> {
    shared.lock().unwrap_or_else(|e| e.into_inner()).as_mut().map(f)
}

/// [`with`] for a worker thread: waits (up to `within`) for the store to be read. None if it still isn't, or
/// `give_up` says so.
fn with_when_loaded<T>(shared: &Shared, within: Duration, give_up: impl Fn() -> bool, f: impl FnOnce(&mut Store) -> T) -> Option<T> {
    let end = Instant::now() + within;
    let mut f = Some(f);
    loop {
        if let Some(v) = with(shared, |s| (f.take().expect("called once"))(s)) {
            return Some(v);
        }
        if give_up() || Instant::now() >= end {
            return None;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

// ---- reading /proc ----------------------------------------------------------------------------------------

/// (comm, the fields after it) of a `/proc/<pid>/stat` line.
fn stat_fields(stat: &str) -> Option<(&str, Vec<&str>)> {
    let comm = stat.get(stat.find('(')? + 1..stat.rfind(')')?)?;
    let rest = stat.get(stat.rfind(')')? + 2..)?;
    Some((comm, rest.split(' ').collect()))
}

/// The process group of a stat line.
fn pgrp(stat: &str) -> Option<i32> {
    stat_fields(stat)?.1.get(2)?.parse().ok()
}

/// The parent and the start time (clock ticks after boot; the same before and after an `exec`) of a stat line.
fn parent_and_start(stat: &str) -> Option<(i32, u64)> {
    let (_, f) = stat_fields(stat)?;
    Some((f.get(1)?.parse().ok()?, f.get(19)?.parse().ok()?))
}

/// utime + stime of a stat line, in clock ticks.
fn ticks(stat: &str) -> Option<u64> {
    let (_, f) = stat_fields(stat)?;
    Some(f.get(11)?.parse::<u64>().ok()? + f.get(12)?.parse::<u64>().ok()?)
}

/// The processes of group `pgid` below `proc` (`/proc`).
fn members(proc: &Path, pgid: i32) -> Vec<i32> {
    std::fs::read_dir(proc)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let pid: i32 = e.file_name().to_str()?.parse().ok()?;
            let stat = std::fs::read_to_string(e.path().join("stat")).ok()?;
            (pgrp(&stat) == Some(pgid)).then_some(pid)
        })
        .collect()
}

/// Whether the group still runs: its leader (runemu.sh, which lives until its clean-up after the port is done) is
/// there. One `stat` of a folder, so it can be asked twice a second.
pub fn group_alive(proc: &Path, pgid: i32) -> bool {
    proc.join(pgid.to_string()).is_dir()
}

/// The port's own process in the group: runemu.sh runs it as `bash <launcher>` (perhaps through `taskset`, which
/// becomes bash, or shows as `taskset ... bash <launcher>` for a moment), an AppImage as itself. Not runemu.sh
/// (`bash /usr/bin/runemu.sh <launcher> ...`), nor the `sed` and `chmod` it runs on the launcher before.
pub fn is_port_process(argv: &[&str], rom: &str) -> bool {
    let shell = |a: &str| a.ends_with("bash") || a.ends_with("/sh") || a == "sh";
    match argv {
        [] => false,
        [exe, ..] if *exe == rom => true,
        _ => !argv.iter().any(|a| a.ends_with("runemu.sh")) && argv.windows(2).any(|w| shell(w[0]) && w[1] == rom),
    }
}

/// The arguments of process `pid` (none for a zombie or a kernel thread).
fn argv_of(proc: &Path, pid: i32) -> Vec<String> {
    let cmdline = std::fs::read(proc.join(pid.to_string()).join("cmdline")).unwrap_or_default();
    cmdline
        .split(|b| *b == 0)
        .filter(|a| !a.is_empty())
        .filter_map(|a| std::str::from_utf8(a).ok().map(str::to_string))
        .collect()
}

fn launcher(proc: &Path, pid: i32, rom: &str) -> bool {
    let argv = argv_of(proc, pid);
    is_port_process(&argv.iter().map(String::as_str).collect::<Vec<_>>(), rom)
}

/// The pid of the port's own process in group `pgid`, once runemu.sh has started it (seen as its launcher).
pub fn port_process(proc: &Path, pgid: i32, rom: &Path) -> Option<i32> {
    let rom = rom.to_str()?;
    members(proc, pgid).into_iter().find(|pid| launcher(proc, *pid, rom))
}

/// What runemu.sh runs around a port besides the port itself, by program name: its own subshells and the tools of
/// its preparation and clean-up. Anything handed the launcher's path counts as one of these too (`sed`, `chmod`,
/// `game-guides-tool`); a game started by the launcher's `exec` isn't handed it.
const HELPERS: &[&str] = &[
    "bash", "sh", "busybox", "sed", "chmod", "chown", "systemctl", "taskset", "nice", "ionice", "sleep", "cat", "grep",
    "awk", "cut", "head", "tail", "tr", "echo", "printf", "mkdir", "rm", "cp", "mv", "ln", "touch", "kill", "pkill",
    "killall", "pgrep", "pidof", "timeout", "flock", "sync", "date", "basename", "dirname", "find", "ls", "jq",
    "set_setting", "get_setting", "fancontrol", "mangohud", "swaymsg", "amixer", "pactl", "wpctl", "evtest",
];

/// How long a child of runemu.sh that isn't one of [`HELPERS`] has to live to count as the port: runemu.sh's own
/// steps before the port take milliseconds each.
pub const EXEC_LIFE: Duration = Duration::from_secs(3);

/// Whether `argv`, a direct child of runemu.sh, may be the port's launcher after it `exec`ed the game.
fn exec_candidate(argv: &[String], rom: &str) -> bool {
    let Some(exe) = argv.first() else { return false };
    let name = exe.rsplit('/').next().unwrap_or(exe);
    !(name.ends_with(".sh") || HELPERS.contains(&name) || argv.iter().any(|a| a.contains(rom)))
}

/// How the port's own process was found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Found {
    /// Seen as its launcher (`bash <launcher>`, or the AppImage).
    Launcher(i32),
    /// A child of runemu.sh that is no helper and lived [`EXEC_LIFE`]: the launcher after `exec /path/to/game`, which
    /// replaced `bash <launcher>` faster than a look every 250 ms could see it (review SOL 2026-10-24 #10). A process
    /// keeps its pid, parent and start time through an `exec`, so this is that same process.
    AfterExec(i32),
}

/// Looks for the port's own process, look after look: the launcher itself, or a lasting child of runemu.sh.
#[derive(Debug, Default)]
pub struct Tracker {
    /// Children of runemu.sh that may be the port, by (pid, start time): when they were first seen.
    seen: HashMap<(i32, u64), Instant>,
}

impl Tracker {
    /// One look below `proc` at group `pgid` (its leader is runemu.sh) at `now`.
    pub fn look(&mut self, proc: &Path, pgid: i32, rom: &Path, now: Instant) -> Option<Found> {
        let rom = rom.to_str()?;
        let mut still = Vec::new();
        for pid in members(proc, pgid) {
            if launcher(proc, pid, rom) {
                return Some(Found::Launcher(pid));
            }
            let stat = std::fs::read_to_string(proc.join(pid.to_string()).join("stat")).unwrap_or_default();
            let Some((parent, start)) = parent_and_start(&stat) else { continue };
            if pid == pgid || parent != pgid || !exec_candidate(&argv_of(proc, pid), rom) {
                continue;
            }
            let first = *self.seen.entry((pid, start)).or_insert(now);
            still.push((pid, start));
            if now.saturating_duration_since(first) >= EXEC_LIFE {
                return Some(Found::AfterExec(pid));
            }
        }
        self.seen.retain(|k, _| still.contains(k));
        None
    }
}

/// One look at the CPU: every thread of the group (pid, tid) with its ticks, and the whole Pocket's.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Snapshot {
    threads: HashMap<(i32, i32), u64>,
    /// Busy and all ticks of all CPUs together, and how many CPUs.
    busy: u64,
    all: u64,
    cpus: usize,
}

/// (busy, all, cpus) from `/proc/stat`.
fn cpu_totals(stat: &str) -> Option<(u64, u64, usize)> {
    let first = stat.lines().next()?.strip_prefix("cpu ")?;
    let v: Vec<u64> = first.split_whitespace().filter_map(|n| n.parse().ok()).collect();
    let at = |i: usize| v.get(i).copied().unwrap_or(0);
    // user nice system idle iowait irq softirq steal (guest time is in user already)
    let busy = at(0) + at(1) + at(2) + at(5) + at(6) + at(7);
    let all = busy + at(3) + at(4);
    let cpus = stat
        .lines()
        .filter(|l| l.starts_with("cpu") && l.as_bytes().get(3).is_some_and(u8::is_ascii_digit))
        .count();
    Some((busy, all, cpus.max(1)))
}

impl Snapshot {
    pub fn read(proc: &Path, pgid: i32) -> Snapshot {
        let mut threads = HashMap::new();
        for pid in members(proc, pgid) {
            let tasks = proc.join(pid.to_string()).join("task");
            for t in std::fs::read_dir(&tasks).into_iter().flatten().flatten() {
                let Some(tid) = t.file_name().to_str().and_then(|n| n.parse::<i32>().ok()) else {
                    continue;
                };
                if let Some(n) = std::fs::read_to_string(t.path().join("stat")).ok().as_deref().and_then(ticks) {
                    threads.insert((pid, tid), n);
                }
            }
        }
        let (busy, all, cpus) = std::fs::read_to_string(proc.join("stat"))
            .ok()
            .as_deref()
            .and_then(cpu_totals)
            .unwrap_or((0, 0, 1));
        Snapshot { threads, busy, all, cpus }
    }
}

/// One window between two looks, in cores: the busiest thread of the port, and the whole Pocket.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Window {
    pub top: f32,
    pub total: f32,
}

/// The window from `a` to `b`; None when no time passed (or the counters went back).
pub fn window(a: &Snapshot, b: &Snapshot) -> Option<Window> {
    let all = b.all.checked_sub(a.all).filter(|d| *d > 0)? as f32;
    let cores = |ticks: u64| ticks as f32 * b.cpus as f32 / all;
    let top = b
        .threads
        .iter()
        .filter_map(|(id, n)| n.checked_sub(*a.threads.get(id)?))
        .max()
        .map_or(0.0, cores);
    let total = cores(b.busy.saturating_sub(a.busy));
    Some(Window { top, total: total.max(top) })
}

/// The busy-wait look over [`WINDOWS`] windows.
#[derive(Clone, Debug, Default)]
pub struct Meter {
    windows: Vec<Window>,
}

/// What the look found for a port.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Note {
    /// One thread kept a core busy while the rest of the Pocket idled.
    pub busy: bool,
    /// The busiest thread on average, in cores, and everything else together.
    pub top: f32,
    pub rest: f32,
    /// When it was measured (unix seconds).
    pub at: u64,
    /// The hint was shown on the page once: the card doesn't show it again.
    #[serde(default)]
    pub seen: bool,
}

impl Meter {
    pub fn push(&mut self, w: Window) {
        self.windows.push(w);
    }

    /// After [`WINDOWS`] windows: busy or not, with the averages; None before.
    pub fn verdict(&self, now: u64) -> Option<Note> {
        if self.windows.len() < WINDOWS {
            return None;
        }
        let n = self.windows.len() as f32;
        let spinning = self.windows.iter().filter(|w| w.top >= BUSY_TOP && w.total - w.top <= REST_MAX).count() as f32;
        let top = self.windows.iter().map(|w| w.top).sum::<f32>() / n;
        let rest = self.windows.iter().map(|w| w.total - w.top).sum::<f32>() / n;
        Some(Note {
            busy: spinning >= n * BUSY_SHARE,
            top,
            rest,
            at: now,
            seen: false,
        })
    }
}

// ---- the notes ------------------------------------------------------------------------------------------

/// The verdicts per port, by the launcher's file name ("Deltarune.sh").
pub type Notes = BTreeMap<String, Note>;

fn key(rom: &Path) -> String {
    rom.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_string()
}

/// Whether to offer Battery: the port spins and doesn't run on Battery already.
pub fn offers_battery(note: Option<&Note>, performance: Performance) -> bool {
    note.is_some_and(|n| n.busy) && performance != Performance::Battery
}

/// Whether the card shows the hint: as [`offers_battery`], until the page has shown it once.
pub fn card_hint(note: Option<&Note>, performance: Performance) -> bool {
    offers_battery(note, performance) && note.is_some_and(|n| !n.seen)
}

// ---- the session ------------------------------------------------------------------------------------------

/// The performance a starting port runs with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wanted {
    /// Known at the start (the store was read).
    Mode(Performance),
    /// The store was still being read at the start: the port's choice once it is, with these defaults (Settings →
    /// Performance & fan's Games: performance and Games: fan), decided on the session's thread.
    Later(Performance, Fan),
}

/// Where a session reads and writes: the real system, or a fake one in tests.
#[derive(Clone, Debug)]
pub struct Paths {
    /// Below it `sys/devices/system/cpu/cpufreq` and `sys/class/devfreq` (`power::Power`).
    pub root: PathBuf,
    pub proc: PathBuf,
    pub restore: PathBuf,
    pub pgid: PathBuf,
    /// The CPU's lease (`power::Lease`): held from before the note until everything is back.
    pub lease: PathBuf,
}

impl Paths {
    pub fn system() -> Paths {
        Paths {
            root: "/".into(),
            proc: "/proc".into(),
            restore: RESTORE_FILE.into(),
            pgid: PGID_FILE.into(),
            lease: crate::power::LEASE_FILE.into(),
        }
    }
}

/// A session's end, said once by the UI thread ([`ended`]) and seen at once by its thread (no 500 ms poll to wait out).
#[derive(Debug, Default)]
pub struct End {
    ended: Mutex<bool>,
    changed: Condvar,
}

impl End {
    fn set(&self) {
        *self.ended.lock().unwrap_or_else(|e| e.into_inner()) = true;
        self.changed.notify_all();
    }

    fn is_set(&self) -> bool {
        *self.ended.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Waits up to `d`; true as soon as the session has ended.
    fn wait(&self, d: Duration) -> bool {
        let guard = self.ended.lock().unwrap_or_else(|e| e.into_inner());
        let (guard, _) = self.changed.wait_timeout_while(guard, d, |ended| !*ended).unwrap_or_else(|e| e.into_inner());
        *guard
    }
}

/// The session running now: only its end signal. Everything it changed lives on its own thread, which alone puts it
/// back, holding the CPU's lease from before its note until then (review SOL 2026-10-24 #1).
static SESSION: Mutex<Option<Arc<End>>> = Mutex::new(None);

/// How often the thread looks for the port's own process.
const POLL: Duration = Duration::from_millis(250);

/// A port has started in process group `pgid` with `wanted`: on a thread, wait for its own process, apply the
/// performance, then look at how it uses the processor (and keep the verdict in `store`). A session still running
/// (no end was said) ends here first.
pub fn start(pgid: Option<i32>, rom: &Path, wanted: Wanted, store: &Shared) {
    let Some(pgid) = pgid else { return };
    let end = Arc::new(End::default());
    if let Some(old) = SESSION.lock().unwrap_or_else(|e| e.into_inner()).replace(end.clone()) {
        old.set();
    }
    spawn_session(end, pgid, rom, wanted, store, Paths::system());
}

fn spawn_session(end: Arc<End>, pgid: i32, rom: &Path, wanted: Wanted, store: &Shared, paths: Paths) -> Option<std::thread::JoinHandle<()>> {
    let (rom, store) = (rom.to_path_buf(), store.clone());
    std::thread::Builder::new()
        .name("port-perf".into())
        .spawn(move || run(&end, pgid, &rom, wanted, &store, &paths))
        .ok()
}

fn run(end: &End, pgid: i32, rom: &Path, wanted: Wanted, store: &Shared, paths: &Paths) {
    let proc = paths.proc.as_path();
    let since = Instant::now();
    let mut tracker = Tracker::default();
    let wants = |m: Performance| if m == Performance::Auto { String::new() } else { format!("; performance {} not applied", m.value()) };
    let what = match wanted {
        Wanted::Mode(m) => wants(m),
        Wanted::Later(..) => "; its performance not applied".to_string(),
    };
    let found = loop {
        if let Some(f) = tracker.look(proc, pgid, rom, Instant::now()) {
            break f;
        }
        if end.wait(POLL) || !group_alive(proc, pgid) {
            println!("ports: {} ended before its own process was seen{what}", rom.display());
            return;
        }
        if since.elapsed() > START_WAIT {
            println!("ports: {}: its own process didn't show in {} s{what}", rom.display(), START_WAIT.as_secs());
            return;
        }
    };
    let how = match found {
        Found::Launcher(pid) => format!("pid {pid}"),
        Found::AfterExec(pid) => format!("pid {pid}, its launcher's exec"),
    };
    println!("ports: {} runs ({how}, {:.1} s after its start)", rom.display(), since.elapsed().as_secs_f32());
    let mode = match wanted {
        Wanted::Mode(m) => m,
        Wanted::Later(p, f) => match with_when_loaded(store, START_WAIT, || end.is_set(), |s| s.choice(rom, (p, f)).performance) {
            Some(m) => m,
            None => {
                println!("ports: {}: the ports' settings weren't read in time; performance not applied", rom.display());
                Performance::Auto
            }
        },
    };
    let applied = if mode == Performance::Auto { None } else { apply(end, pgid, rom, mode, paths) };
    if let Some(v) = measure(end, proc, pgid) {
        println!(
            "ports: {} {} (busiest thread {:.2} cores, the rest {:.2})",
            rom.display(),
            if v.busy { "keeps a core busy" } else { "doesn't spin" },
            v.top,
            v.rest
        );
        // Read for minutes by now; never a reason to hold the CPU after the port ended.
        if with_when_loaded(store, START_WAIT, || end.is_set(), |s| s.keep(rom, v)).is_none() {
            eprintln!("ports: the ports' settings weren't read; the verdict for {} isn't kept", rom.display());
        }
    }
    if let Some((power, lease)) = applied {
        // Ours until the port ends: the session's end, or its group gone (runemu.sh has ended).
        while !end.wait(Duration::from_millis(500)) && group_alive(proc, pgid) {}
        put_back(power, lease, paths);
    }
}

/// Takes the CPU's lease (after the game before has put everything back), checks the session is still the running
/// one, then notes the group and applies `mode` (the restore note first, `Power::set_port`). None when nothing was
/// changed.
fn apply(end: &End, pgid: i32, rom: &Path, mode: Performance, paths: &Paths) -> Option<(Power, Lease)> {
    let Some(lease) = Lease::take(&paths.lease, crate::power::LEASE_WAIT, || end.is_set()) else {
        if !end.is_set() {
            println!("ports: the CPU is still another game's; performance {} not applied for {}", mode.value(), rom.display());
        }
        return None;
    };
    // Checked with the lease held and before any write: a session that ended meanwhile never touches the system,
    // and the next one (which waits for this lease) finds it as the one before left it.
    if end.is_set() {
        return None;
    }
    // A note left by a crashed pocket-ui is undone first: this session's note goes in its place.
    if paths.restore.exists() && Power::with_root(&paths.root, &paths.restore).recover() {
        println!("ports: performance profile of an earlier port undone");
    }
    // The group goes first: with it the note is only undone (`recover_now`) once this port no longer runs.
    if let Err(e) = crate::fsutil::write_atomic(&paths.pgid, pgid.to_string().as_bytes()) {
        eprintln!("ports: can't note the port's group in {} ({e}); performance {} not applied", paths.pgid.display(), mode.value());
        return None;
    }
    let mut p = Power::with_root(&paths.root, &paths.restore);
    let ok = p.set_port(mode);
    println!("ports: performance {}: {}", mode.value(), if ok { "set" } else { "not possible here" });
    if !paths.restore.exists() {
        // Nothing noted, so nothing was written (`Power::set_port` said why).
        let _ = std::fs::remove_file(&paths.pgid);
        return None;
    }
    Some((p, lease))
}

/// What `apply` set goes back where it is still as it was set; then the lease goes, and the next game may note the CPU.
fn put_back(mut power: Power, lease: Lease, paths: &Paths) {
    let ok = power.restore();
    let _ = std::fs::remove_file(&paths.pgid);
    drop(lease);
    println!("ports: performance put back{}", if ok { "" } else { " (not all of it; the note stays)" });
}

/// The busy-wait look: [`WARMUP`], then [`WINDOWS`] windows; None when the session or the group ended before.
fn measure(end: &End, proc: &Path, pgid: i32) -> Option<Note> {
    let wait = |d: Duration| !end.wait(d) && group_alive(proc, pgid);
    if !wait(WARMUP) {
        return None;
    }
    let mut meter = Meter::default();
    let mut last = Snapshot::read(proc, pgid);
    loop {
        if !wait(WINDOW) {
            return None;
        }
        let now = Snapshot::read(proc, pgid);
        if let Some(w) = window(&last, &now) {
            meter.push(w);
        }
        last = now;
        if let Some(v) = meter.verdict(crate::recents::now_secs()) {
            return Some(v);
        }
    }
}

/// The port ended (by any route): its thread puts back what it set, at once (the UI thread only says so). The next
/// game's CPU settings wait for that through the lease, the next port's and pocket-core's alike.
pub fn ended() {
    if let Some(end) = SESSION.lock().unwrap_or_else(|e| e.into_inner()).take() {
        end.set();
    }
}

/// Undoes what a port's session left behind (pocket-ui crashed), unless its process group still runs or the CPU is
/// another game's now (it holds the lease). True when something was undone. For `pocket-core --restore-system` and
/// [`recover`].
pub fn recover_now() -> bool {
    recover_with(&Paths::system(), Duration::from_millis(200))
}

fn recover_with(paths: &Paths, wait: Duration) -> bool {
    if !paths.restore.exists() {
        return false;
    }
    let pgid = std::fs::read_to_string(&paths.pgid).ok().and_then(|s| s.trim().parse::<i32>().ok());
    if pgid.is_some_and(|g| group_alive(&paths.proc, g)) {
        return false;
    }
    let Some(_lease) = Lease::take(&paths.lease, wait, || false) else { return false };
    // Looked at again with the lease: a session may have put it back (or taken over) meanwhile.
    if !paths.restore.exists() {
        return false;
    }
    let done = Power::with_root(&paths.root, &paths.restore).recover();
    let _ = std::fs::remove_file(&paths.pgid);
    done
}

/// At pocket-ui's start: a port's profile left behind by a crash is undone, once that port no longer runs and no
/// game holds the CPU (a game continued at the start: then after it, or by `pocket-core --restore-system`).
pub fn recover() {
    if !Path::new(RESTORE_FILE).exists() {
        return;
    }
    let _ = std::thread::Builder::new().name("port-perf-recover".into()).spawn(|| {
        let paths = Paths::system();
        while paths.restore.exists() {
            if recover_with(&paths, Duration::from_secs(1)) {
                println!("ports: performance profile of an earlier port undone");
                return;
            }
            std::thread::sleep(Duration::from_secs(5));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;

    fn dir(name: &str) -> crate::testutil::TestDir {
        TestDir::new(&format!("portperf-{name}"))
    }

    const ROM: &str = "/storage/roms/ports/Deltarune.sh";

    #[test]
    fn a_port_has_its_own_choice_below_settings_and_back_to_the_defaults() {
        let d = dir("choice");
        let rom = Path::new(ROM);
        let auto = (Performance::Auto, Fan::Auto);
        // A first start: no files at all. Every port runs with the defaults, nothing of its own.
        let mut st = Store::load(&d);
        let c = st.choice(rom, auto);
        assert_eq!(
            (c.performance, c.fan, c.own_performance, c.own_fan),
            (Performance::Auto, Fan::Auto, false, false)
        );
        // Settings → Performance & fan: Games: performance Performance, Games: fan Quiet.
        let page = (Performance::Performance, Fan::Quiet);
        let c = st.choice(rom, page);
        assert_eq!(
            (c.performance, c.fan, c.own_performance),
            (Performance::Performance, Fan::Quiet, false),
            "the page's for every port"
        );
        // Battery for this port: its own; the fan still the page's.
        st.set(rom, Some(("performance", "battery")));
        let c = st.choice(rom, page);
        assert_eq!(
            (c.performance, c.fan, c.own_performance, c.own_fan),
            (Performance::Battery, Fan::Quiet, true, false)
        );
        st.set(rom, Some(("fan", "cool")));
        assert_eq!(st.choice(rom, page).fan, Fan::Cool);
        assert_eq!(
            st.choice(Path::new("/storage/roms/ports/Other.sh"), page).performance,
            Performance::Performance,
            "other ports keep the page's"
        );
        // Written by the card writer, in pocket-core's place and shape: a restart reads it back.
        crate::fsutil::settle();
        let file: BTreeMap<String, String> = crate::fsutil::read_json_or_backup(&d.join("game-settings/ports/Deltarune.json")).unwrap();
        assert_eq!((file["performance"].as_str(), file["fan"].as_str()), ("battery", "cool"));
        assert_eq!(Store::load(&d).choice(rom, page), st.choice(rom, page));
        let gs = crate::options::GameSettings::load(&d, SYSTEM, rom);
        assert_eq!(
            gs.values.get("performance").map(String::as_str),
            Some("battery"),
            "options::GameSettings reads the same file"
        );
        // Back to the defaults: both keys go, the page's values count again, also after a restart.
        st.set(rom, None);
        let c = st.choice(rom, page);
        assert_eq!(
            (c.performance, c.fan, c.own_performance, c.own_fan),
            (Performance::Performance, Fan::Quiet, false, false)
        );
        crate::fsutil::settle();
        assert_eq!(Store::load(&d).choice(rom, page), c);
    }

    #[test]
    fn a_file_for_every_port_and_a_hand_edited_value() {
        let d = dir("console");
        let rom = Path::new(ROM);
        let auto = (Performance::Auto, Fan::Auto);
        std::fs::create_dir_all(d.join("game-settings/ports")).unwrap();
        std::fs::write(d.join("game-settings/ports.json"), r#"{"performance": "battery", "fan": "cool"}"#).unwrap();
        std::fs::write(d.join("game-settings/ports/Odd.json"), r#"{"performance": "turbo"}"#).unwrap();
        // Auto was called Balanced until 2026-10-23.
        std::fs::write(d.join("game-settings/ports/Old.json"), r#"{"performance": "balanced"}"#).unwrap();
        let st = Store::load(&d);
        let old = st.choice(Path::new("/storage/roms/ports/Old.sh"), (Performance::Battery, Fan::Auto));
        assert_eq!((old.performance, old.own_performance), (Performance::Auto, true), "an old Balanced is the port's own Auto");
        let c = st.choice(rom, auto);
        assert_eq!(
            (c.performance, c.fan, c.own_performance, c.own_fan),
            (Performance::Battery, Fan::Cool, false, false),
            "every port's file, not this port's own"
        );
        // A port's own file replaces the one for every port, as in GameSettings; a value that is no choice is not there.
        let c = st.choice(Path::new("/storage/roms/ports/Odd.sh"), auto);
        assert_eq!(
            (c.performance, c.fan, c.own_performance, c.own_fan),
            (Performance::Auto, Fan::Auto, false, false)
        );
        // "Use the defaults" on a port: the file for every port counts for it again.
        let mut st = st;
        st.set(rom, Some(("fan", "quiet")));
        assert_eq!(st.choice(rom, auto).performance, Performance::Auto, "its own file replaces the one for every port");
        st.set(rom, None);
        assert_eq!(st.choice(rom, auto).performance, Performance::Battery);
        crate::fsutil::settle();
    }

    #[test]
    fn the_store_is_read_on_a_thread_and_nobody_waits_for_it() {
        // Review SOL 2026-10-24 #6: the loader held the lock while it read, and `with` waited for it or read the
        // files itself. A slow card (half a second here): the UI's look is None at once, never a wait or a read.
        let d = dir("shared");
        std::fs::create_dir_all(d.join("game-settings/ports")).unwrap();
        std::fs::write(d.join("game-settings/ports/Deltarune.json"), r#"{"fan": "quiet"}"#).unwrap();
        let slow = d.to_path_buf();
        let shared = spawn_load_with(move || {
            std::thread::sleep(Duration::from_millis(500));
            Store::load(&slow)
        });
        std::thread::sleep(Duration::from_millis(50));
        let t0 = Instant::now();
        assert_eq!(with(&shared, |s| s.choice(Path::new(ROM), (Performance::Auto, Fan::Auto)).fan), None, "still being read");
        assert!(t0.elapsed() < Duration::from_millis(100), "no wait: {:?}", t0.elapsed());
        // A worker waits for it; then everyone sees the finished store.
        let own = with_when_loaded(&shared, Duration::from_secs(5), || false, |s| s.choice(Path::new(ROM), (Performance::Auto, Fan::Auto)).own_fan);
        assert_eq!(own, Some(true));
        assert_eq!(with(&shared, |s| s.choice(Path::new(ROM), (Performance::Auto, Fan::Auto)).fan), Some(Fan::Quiet));
        // The real loader.
        let shared = spawn_load(&d);
        assert_eq!(with_when_loaded(&shared, Duration::from_secs(5), || false, |s| s.choice(Path::new(ROM), (Performance::Auto, Fan::Auto)).fan), Some(Fan::Quiet));
    }

    #[test]
    fn the_ports_own_process_is_found_not_runemu_or_its_helpers() {
        let runemu = [
            "/usr/bin/bash",
            "/usr/bin/runemu.sh",
            ROM,
            "-Pports",
            "--core=portmaster",
            "--emulator=portmaster",
            "--controllers=",
        ];
        assert!(!is_port_process(&runemu, ROM));
        assert!(!is_port_process(
            &[
                "sed",
                "-i",
                "/^ACTIVE_GAME=/c\\ACTIVE_GAME=\"/storage/roms/ports/Deltarune.sh\"",
                "/storage/.config/PortMaster/mapper.txt"
            ],
            ROM
        ));
        assert!(!is_port_process(&["chmod", "+x", ROM], ROM));
        assert!(!is_port_process(&["/usr/bin/game-guides-tool", ROM], ROM));
        assert!(is_port_process(&["/usr/bin/bash", ROM], ROM));
        assert!(is_port_process(&["bash", ROM], ROM), "through taskset (cores: big) it is still bash");
        assert!(is_port_process(&["taskset", "-c", "4-7", "/usr/bin/bash", ROM], ROM), "or taskset, a moment before its exec");
        assert!(is_port_process(&["/storage/roms/ports/Game.AppImage"], "/storage/roms/ports/Game.AppImage"));
        assert!(!is_port_process(&["/usr/bin/bash", "/storage/roms/ports/Other.sh"], ROM));
        assert!(!is_port_process(&[], ROM));
    }

    /// One process of a fake /proc: pid, pgid, argv, threads as (tid, ticks).
    type FakeProc<'a> = (i32, i32, &'a [&'a str], &'a [(i32, u64)]);

    /// A fake /proc with `procs`; `busy` and `idle` the whole Pocket's ticks.
    fn fake_proc(root: &Path, procs: &[FakeProc], busy: u64, idle: u64) {
        let _ = std::fs::remove_dir_all(root);
        for (pid, pgid, argv, threads) in procs {
            let p = root.join(pid.to_string());
            std::fs::create_dir_all(p.join("task")).unwrap();
            // Every member's parent is the group's leader (runemu.sh), the leader's is init.
            let ppid = if pid == pgid { 1 } else { *pgid };
            let line = |ticks: u64| format!("{pid} (x y) S {ppid} {pgid} {pgid} 0 -1 4194560 100 0 0 0 {ticks} 0 0 0 20 0 1 0 {} 1000 10", 100 + pid);
            std::fs::write(p.join("stat"), line(0)).unwrap();
            std::fs::write(p.join("cmdline"), argv.join("\0") + "\0").unwrap();
            for (tid, ticks) in *threads {
                let t = p.join("task").join(tid.to_string());
                std::fs::create_dir_all(&t).unwrap();
                std::fs::write(t.join("stat"), line(*ticks)).unwrap();
            }
        }
        let cpus: String = (0..8).map(|i| format!("cpu{i} 1 0 1 1 0 0 0 0 0 0\n")).collect();
        std::fs::write(root.join("stat"), format!("cpu  {busy} 0 0 {idle} 0 0 0 0 0 0\n{cpus}intr 1\n")).unwrap();
    }

    #[test]
    fn reading_proc_finds_the_group_its_threads_and_the_busiest_one() {
        let root = dir("proc");
        let runemu: &[&str] = &["/usr/bin/bash", "/usr/bin/runemu.sh", ROM, "-Pports"];
        let port: &[&str] = &["/usr/bin/bash", ROM];
        fake_proc(&root, &[(100, 100, runemu, &[(100, 5)]), (120, 999, &["sway"], &[(120, 50)])], 1000, 7000);
        assert!(group_alive(&root, 100));
        assert_eq!(port_process(&root, 100, Path::new(ROM)), None, "only runemu.sh so far");
        fake_proc(
            &root,
            &[
                (100, 100, runemu, &[(100, 5)]),
                (130, 100, port, &[(130, 2)]),
                (140, 100, &["./gmloader", "game.port"], &[(140, 100), (141, 10)]),
            ],
            1000,
            7000,
        );
        assert_eq!(port_process(&root, 100, Path::new(ROM)), Some(130));
        let a = Snapshot::read(&root, 100);
        assert_eq!(a.threads.len(), 4, "sway is not in the group");
        assert_eq!(a.cpus, 8);
        // 10 s later on 8 CPUs (8000 ticks in all): gmloader's main thread used 1000 (a whole core), the Pocket 1300.
        fake_proc(
            &root,
            &[
                (100, 100, runemu, &[(100, 5)]),
                (130, 100, port, &[(130, 2)]),
                (140, 100, &["./gmloader", "game.port"], &[(140, 1100), (141, 30)]),
            ],
            2300,
            13_700,
        );
        let b = Snapshot::read(&root, 100);
        let w = window(&a, &b).unwrap();
        assert!((w.top - 1.0).abs() < 0.01, "{w:?}");
        assert!((w.total - 1.3).abs() < 0.01, "{w:?}");
        assert_eq!(window(&b, &b), None, "no time passed");
        assert!(!group_alive(&root, 4242));
    }

    #[test]
    fn a_launcher_that_execs_its_game_is_still_found() {
        // Review SOL 2026-10-24 #10: `exec ./game` replaced `bash <launcher>` between two looks; nothing was ever
        // applied. The same process (pid, parent runemu.sh, start time) lives on as the game.
        let root = dir("exec");
        let runemu: &[&str] = &["/usr/bin/bash", "/usr/bin/runemu.sh", ROM, "-Pports"];
        let rom = Path::new(ROM);
        let t0 = Instant::now();
        let at = |s: u64| t0 + Duration::from_millis(s);
        // runemu.sh's helpers first: none of them is the port, however long they take.
        fake_proc(
            &root,
            &[
                (100, 100, runemu, &[]),
                (110, 100, &["sed", "-i", "/^ACTIVE_GAME=/c\\ACTIVE_GAME=\"/storage/roms/ports/Deltarune.sh\"", "mapper.txt"], &[]),
                (111, 100, &["/usr/bin/game-guides-tool", ROM], &[]),
                (112, 100, &["systemctl", "restart", "fancontrol"], &[]),
                (113, 100, &["/usr/bin/bash", "/usr/bin/runemu.sh", ROM, "-Pports"], &[]),
            ],
            0,
            0,
        );
        let mut t = Tracker::default();
        for ms in [0, 250, 4000, 9000] {
            assert_eq!(t.look(&root, 100, rom, at(ms)), None, "{ms} ms");
        }
        // The launcher has exec'd gmloader before any look saw `bash <launcher>`.
        fake_proc(&root, &[(100, 100, runemu, &[]), (130, 100, &["./gmloader", "game.port"], &[(130, 5)])], 0, 0);
        assert_eq!(t.look(&root, 100, rom, at(10_000)), None, "a moment could still be a helper");
        assert_eq!(t.look(&root, 100, rom, at(11_000)), None);
        assert_eq!(t.look(&root, 100, rom, at(13_000)), Some(Found::AfterExec(130)), "lived {EXEC_LIFE:?}: the port");
        // A pid that went and came back as another process (another start time) starts its wait anew.
        let mut t = Tracker::default();
        assert_eq!(t.look(&root, 100, rom, at(0)), None);
        let stat = std::fs::read_to_string(root.join("130/stat")).unwrap().replace(" 230 1000 10", " 999 1000 10");
        std::fs::write(root.join("130/stat"), stat).unwrap();
        assert_eq!(t.look(&root, 100, rom, at(3500)), None, "not the process seen first");
        assert_eq!(t.look(&root, 100, rom, at(7000)), Some(Found::AfterExec(130)));
        // The launcher itself (not exec'd) is found at once, whatever runs beside it.
        fake_proc(
            &root,
            &[(100, 100, runemu, &[]), (130, 100, &["/usr/bin/bash", ROM], &[]), (140, 100, &["./gmloader"], &[])],
            0,
            0,
        );
        assert_eq!(Tracker::default().look(&root, 100, rom, at(0)), Some(Found::Launcher(130)));
    }

    // ---- review SOL 2026-10-24 #1: a port's session and the next game

    /// A fake system for sessions: sysfs (`power::fake`), /proc with runemu.sh (`pgid`) and the port, the notes.
    fn system(name: &str) -> (crate::power::fake::Fake, Paths) {
        let f = crate::power::fake::Fake::new(&format!("portsession-{name}"));
        let paths = Paths {
            root: f.0.to_path_buf(),
            proc: f.0.join("proc"),
            restore: f.0.join("port.restore"),
            pgid: f.0.join("port.pgid"),
            lease: f.0.join("perf.lock"),
        };
        (f, paths)
    }

    fn port_runs(paths: &Paths, groups: &[i32]) {
        let runemu: &[&str] = &["/usr/bin/bash", "/usr/bin/runemu.sh", ROM, "-Pports"];
        let port: &[&str] = &["/usr/bin/bash", ROM];
        let procs: Vec<FakeProc> = groups.iter().flat_map(|g| [(*g, *g, runemu, &[][..]), (*g + 30, *g, port, &[][..])]).collect();
        fake_proc(&paths.proc, &procs, 1000, 7000);
    }

    fn until(what: &str, ok: impl Fn() -> bool) {
        let end = Instant::now() + Duration::from_secs(10);
        while !ok() {
            assert!(Instant::now() < end, "never: {what}");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn loaded_store() -> Shared {
        Arc::new(Mutex::new(Some(Store::default())))
    }

    #[test]
    fn a_ports_caps_are_back_before_the_next_game_notes_the_cpu() {
        // The switcher from a Battery port to a pocket-core game on Battery: the port's session ends (the UI thread
        // only says so) and the next game starts at once. pocket-core takes the lease before it notes the CPU, so it
        // notes the system's limits, not the port's caps, and the port's thread can't raise them under it later.
        let (f, paths) = system("switch");
        port_runs(&paths, &[100]);
        let end = Arc::new(End::default());
        let worker = spawn_session(end.clone(), 100, Path::new(ROM), Wanted::Mode(Performance::Battery), &loaded_store(), paths.clone()).unwrap();
        until("the port's caps", || f.cpu("policy4", "scaling_max_freq") == "1286400" && paths.pgid.exists());
        assert_eq!(std::fs::read_to_string(&paths.pgid).unwrap(), "100");
        end.set();
        // pocket-core's start (`Lease::for_game`, then `Extras::new`).
        let lease = Lease::take(&paths.lease, crate::power::LEASE_WAIT, || false).expect("the port's thread let go");
        let mut core = Power::with_root(&f.0, f.0.join("core.restore"));
        assert!(core.set_performance(Performance::Battery));
        let note: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(f.0.join("core.restore")).unwrap()).unwrap();
        assert_eq!(note["cpu"]["policy4"][1], "2419200", "the system's limit, not the port's cap");
        assert_eq!(note["cpu"]["policy0"][1], "1804800");
        assert!(!paths.restore.exists() && !paths.pgid.exists(), "the port's note and group are gone");
        // Its own Battery holds (nobody raises it now), and comes back to the system's when it ends.
        assert_eq!(f.cpu("policy4", "scaling_max_freq"), "1286400");
        worker.join().unwrap();
        core.restore();
        drop(lease);
        assert_eq!(f.cpu("policy4", "scaling_max_freq"), "2419200");
        assert_eq!(f.cpu("policy0", "scaling_max_freq"), "1804800");
    }

    #[test]
    fn an_ended_session_never_touches_the_next_ones_settings_or_note() {
        // Session A (group 100) ends while it waits for the CPU (another game still holds it); session B (group 200)
        // starts. A must write nothing: not its caps, not the group file, and it must not delete B's.
        let (f, paths) = system("cancel");
        port_runs(&paths, &[100, 200]);
        let held = Lease::take(&paths.lease, Duration::ZERO, || false).unwrap();
        let a_end = Arc::new(End::default());
        let a = spawn_session(a_end.clone(), 100, Path::new(ROM), Wanted::Mode(Performance::Battery), &loaded_store(), paths.clone()).unwrap();
        std::thread::sleep(Duration::from_millis(400));
        a_end.set();
        let b_end = Arc::new(End::default());
        let b = spawn_session(b_end.clone(), 200, Path::new(ROM), Wanted::Mode(Performance::Performance), &loaded_store(), paths.clone()).unwrap();
        a.join().unwrap();
        assert!(!paths.restore.exists() && !paths.pgid.exists(), "A wrote nothing");
        assert_eq!(f.cpu("policy4", "scaling_max_freq"), "2419200", "no cap of A's");
        drop(held);
        until("B's profile", || f.cpu("policy4", "scaling_governor") == "performance");
        assert_eq!(std::fs::read_to_string(&paths.pgid).unwrap(), "200", "B's group, untouched");
        assert!(paths.restore.exists());
        b_end.set();
        b.join().unwrap();
        assert_eq!(f.cpu("policy4", "scaling_governor"), "ondemand");
        assert!(!paths.restore.exists() && !paths.pgid.exists());
    }

    #[test]
    fn the_next_port_applies_only_after_the_one_before_put_it_back() {
        let (f, paths) = system("port-to-port");
        port_runs(&paths, &[100, 200]);
        let a_end = Arc::new(End::default());
        let a = spawn_session(a_end.clone(), 100, Path::new(ROM), Wanted::Mode(Performance::Battery), &loaded_store(), paths.clone()).unwrap();
        until("A's caps", || f.cpu("policy4", "scaling_max_freq") == "1286400");
        a_end.set();
        let b_end = Arc::new(End::default());
        let b = spawn_session(b_end.clone(), 200, Path::new(ROM), Wanted::Mode(Performance::Battery), &loaded_store(), paths.clone()).unwrap();
        a.join().unwrap();
        until("B's caps", || std::fs::read_to_string(&paths.pgid).is_ok_and(|g| g == "200") && f.cpu("policy4", "scaling_max_freq") == "1286400");
        let note: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&paths.restore).unwrap()).unwrap();
        assert_eq!(note["ours"]["sys/devices/system/cpu/cpufreq/policy4/scaling_max_freq"][1], "2419200", "B noted the system's limit");
        b_end.set();
        b.join().unwrap();
        assert_eq!(f.cpu("policy4", "scaling_max_freq"), "2419200");
    }

    #[test]
    fn a_port_started_before_the_settings_were_read_gets_its_own_choice_on_its_thread() {
        let (f, paths) = system("later");
        port_runs(&paths, &[100]);
        let d = dir("later-store");
        std::fs::create_dir_all(d.join("game-settings/ports")).unwrap();
        std::fs::write(d.join("game-settings/ports/Deltarune.json"), r#"{"performance": "battery"}"#).unwrap();
        let slow = d.to_path_buf();
        let store = spawn_load_with(move || {
            std::thread::sleep(Duration::from_millis(300));
            Store::load(&slow)
        });
        let end = Arc::new(End::default());
        let w = spawn_session(end.clone(), 100, Path::new(ROM), Wanted::Later(Performance::Auto, Fan::Auto), &store, paths.clone()).unwrap();
        until("its own Battery", || f.cpu("policy4", "scaling_max_freq") == "1286400");
        end.set();
        w.join().unwrap();
        assert_eq!(f.cpu("policy4", "scaling_max_freq"), "2419200");
    }

    fn meter(windows: &[(f32, f32)]) -> Option<Note> {
        let mut m = Meter::default();
        for (top, total) in windows {
            m.push(Window { top: *top, total: *total });
        }
        m.verdict(1)
    }

    #[test]
    fn a_spinning_port_is_told_from_a_busy_one_and_from_a_calm_one() {
        // GameMaker's gmloader: one thread at a whole core, the Pocket otherwise idle.
        let spin = meter(&[(0.98, 1.2); WINDOWS]).unwrap();
        assert!(spin.busy, "{spin:?}");
        assert!((spin.top - 0.98).abs() < 0.01 && (spin.rest - 0.22).abs() < 0.01);
        // A demanding port: a full core, and three more busy too. Not a spin, the work is real.
        assert!(!meter(&[(1.0, 4.0); WINDOWS]).unwrap().busy);
        // A calm port: nothing near a whole core.
        assert!(!meter(&[(0.35, 0.8); WINDOWS]).unwrap().busy);
        // Loading screens in between: most windows are enough.
        let mut mixed = vec![(0.97, 1.1); WINDOWS];
        mixed[2] = (0.6, 2.5);
        mixed[7] = (0.5, 3.0);
        assert!(meter(&mixed).unwrap().busy);
        // Half the time only: no.
        let half: Vec<(f32, f32)> = (0..WINDOWS).map(|i| if i % 2 == 0 { (0.97, 1.1) } else { (0.3, 0.6) }).collect();
        assert!(!meter(&half).unwrap().busy);
        // Before twelve windows: no verdict yet.
        assert_eq!(meter(&[(0.98, 1.2); WINDOWS - 1]), None);
    }

    #[test]
    fn the_hint_shows_once_and_only_when_battery_would_change_something() {
        let d = dir("notes");
        let rom = Path::new(ROM);
        let mut st = Store::load(&d);
        assert_eq!(st.note(rom), None, "never looked at: no hint, no offer");
        assert!(!card_hint(st.note(rom), Performance::Auto) && !offers_battery(st.note(rom), Performance::Auto));
        let busy = Note {
            busy: true,
            top: 0.98,
            rest: 0.2,
            at: 5,
            seen: false,
        };
        st.keep(rom, busy.clone());
        let n = st.note(rom).cloned();
        assert!(card_hint(n.as_ref(), Performance::Auto) && offers_battery(n.as_ref(), Performance::Auto));
        assert!(!card_hint(n.as_ref(), Performance::Battery), "on Battery already");
        assert!(card_hint(n.as_ref(), Performance::Performance));
        // Shown on the page once: the card is quiet about it, the page still offers Battery; a restart keeps that.
        st.mark_seen(rom);
        crate::fsutil::settle();
        let mut st = Store::load(&d);
        let n = st.note(rom).cloned();
        assert!(!card_hint(n.as_ref(), Performance::Auto) && offers_battery(n.as_ref(), Performance::Auto));
        // A new measurement keeps it seen; a calm one offers nothing.
        st.keep(rom, busy.clone());
        assert!(st.note(rom).unwrap().seen);
        st.keep(rom, Note { busy: false, ..busy });
        assert!(!offers_battery(st.note(rom), Performance::Auto));
        assert!(st.note(Path::new("/storage/roms/ports/Other.sh")).is_none(), "an unknown port has no note");
        crate::fsutil::settle();
    }

    #[test]
    fn proc_stat_reads_whole_cpus_and_their_count() {
        let stat = "cpu  100 5 50 800 20 3 2 0 0 0\ncpu0 1 1 1 1 1 1 1 0 0 0\ncpu1 1 1 1 1 1 1 1 0 0 0\nintr 9\nctxt 4\n";
        assert_eq!(cpu_totals(stat), Some((160, 980, 2)));
        assert_eq!(cpu_totals("nonsense"), None);
        let line = "4321 (Main Thread) R 1 4300 4300 0 -1 4194560 100 0 0 0 1234 56 0 0 20 0 30 0 100 1000 10";
        assert_eq!(
            (pgrp(line), ticks(line)),
            (Some(4300), Some(1290)),
            "a name with a space and parentheses' content"
        );
    }
}
