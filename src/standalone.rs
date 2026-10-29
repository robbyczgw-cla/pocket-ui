//! PS2 (ARMSX2), GameCube and Wii (Dolphin) and 3DS (Azahar) as first-class guests of the pause menu: Save,
//! Load, save on Quit and resume on the next start, each through the emulator's own control channel. See
//! docs/BIG-EMULATORS.md.
//!
//! - ARMSX2 (a PCSX2 fork) keeps PCSX2's PINE socket: save/load a slot, ask for the status and the disc.
//! - ROCKNIX's Dolphin nogui has pad hotkeys patched in: HOTKEY+R saves, HOTKEY+L loads, HOTKEY+Up/Down pick
//!   the slot (HOTKEY is Select unless `hotkey_enable_button` says Guide). We press them through
//!   InputPlumber, which feeds the virtual pad Dolphin reads. InputPlumber can only press buttons, and GC L/R
//!   come from the analog triggers, so the controller profiles also take HOTKEY + a stick click as L and R.
//!   Wii games read the same GameCube pad for the hotkeys; the emulated Wiimote is told to ignore what our
//!   chords press while a stick click is held.
//! - Azahar has Qt keyboard shortcuts (Quick Save Alt+S, Quick Load Alt+L) and one quick slot. We press the
//!   keys on InputPlumber's keyboard; the resume state is kept in a file of our own and swapped into the
//!   quick slot only while Azahar saves or loads it.
//!
//! States (`states.rs`): Save writes manual slots 1 to 8 (Azahar has one), the state written on Quit and before
//! sleep has a place of its own, never a manual slot, and is loaded again shortly after the next start, and Load
//! first keeps what it replaces as "Before load" until the next Quit.
//! - ARMSX2: PINE saves and loads by slot number; slots 1 to 8 are its own, the auto state is 99, "Before load" 98.
//! - Dolphin: slots 1 to 8 are stepped to with the hotkeys (`dolphin::chords`); the auto and undo states are files
//!   of ours (`<game>.pocket-auto`, `.pocket-undo`) that pass through the slot ROCKNIX's hotkeys start on (`.s00`).
//! - Azahar: its quick slot is the one manual slot; the other states are files of ours, passed through it.

use crate::image::Image;
use crate::states::{StateSet, Target};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// How long the main loop waits after Quit before it stops the whole process group itself: saving the state,
/// shutting the VM down and flushing the memory cards take 2–6 s, and `quit` gives up on its own after at most
/// `quit_worst` (30.5 s for ARMSX2). Stopping the game sooner would cut that short.
pub const QUIT_GRACE: Duration = Duration::from_secs(35);

/// How long `quit` waits for each step before it moves on: the state to be written, ARMSX2's VM to shut down,
/// the process to end after the second SIGTERM, Azahar to close its window, and before SIGKILL.
const SAVE_WAIT: Duration = Duration::from_secs(12);
const VM_SHUTDOWN_WAIT: Duration = Duration::from_secs(10);
const EXIT_WAIT: Duration = Duration::from_millis(500);
const AZAHAR_EXIT_WAIT: Duration = Duration::from_secs(8);
const KILL_WAIT: Duration = Duration::from_secs(8);
/// RetroArch's `QUIT` closes the core (the HDD image and caches are flushed) and the window in about a second.
const RA_EXIT_WAIT: Duration = Duration::from_secs(10);
/// The longest an ARMSX2 Quit takes when every step runs out its time.
pub const ARMSX2_QUIT_WORST: Duration = SAVE_WAIT.saturating_add(VM_SHUTDOWN_WAIT).saturating_add(EXIT_WAIT).saturating_add(KILL_WAIT);

/// How long the main loop waits after a guest's Quit (`guest::WORST` plus slack) before it stops the process group.
pub const GUEST_GRACE: Duration = Duration::from_secs(20);

const SYSTEM_CFG: &str = "/storage/.config/system/configs/system.cfg";

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Emu {
    Armsx2,
    Dolphin,
    Azahar,
    /// Xbox: hakuX in ROCKNIX's RetroArch (`hakux.rs`).
    Hakux,
}

impl Emu {
    /// The emulators handled here. Dolphin's Qt build has no pad hotkeys; WiiWare and xemu stay
    /// without (Continue/Quit only); Wii U and PS3 are `guest`s. Xbox's hakuX has Continue/Quit and gets the rest with save states (`hakux`).
    pub fn detect(system: &str, emulator: &str, core: &str) -> Option<Emu> {
        match (system, emulator) {
            ("ps2", "armsx2") => Some(Emu::Armsx2),
            ("gamecube" | "wii", "dolphin") if !core.contains("qt") => Some(Emu::Dolphin),
            ("3ds", "azahar") => Some(Emu::Azahar),
            ("xbox", "hakux") => Some(Emu::Hakux),
            _ => None,
        }
    }

    /// Whether the state file names of this game are known: Dolphin names WAD channels after an id inside the
    /// file, Azahar names CIA and compressed games after a title id we don't read.
    fn handles(self, rom: &Path) -> bool {
        matches!(self, Emu::Armsx2 | Emu::Hakux) || state_base(self, rom).is_some()
    }

    /// Start of the emulator's process name (`comm` is cut at 15 characters).
    fn process(self) -> &'static str {
        match self {
            Emu::Armsx2 => "armsx2-qt",
            Emu::Dolphin => "dolphin-emu-nog",
            Emu::Azahar => "azahar",
            Emu::Hakux => "retroarch",
        }
    }
}

/// The part of the state file names before the slot, as far as the game file tells it. ARMSX2 names its
/// states after the disc, which is learned once the game runs.
fn state_base(emu: Emu, rom: &Path) -> Option<String> {
    match emu {
        Emu::Armsx2 => None,
        Emu::Dolphin => dolphin::state_base(rom),
        Emu::Azahar => azahar::title_id(rom).map(|t| format!("{t:016X}")),
        // RetroArch names its states after the content without its extension.
        Emu::Hakux => rom.file_stem().and_then(|s| s.to_str()).map(str::to_string),
    }
}

/// ROCKNIX's `get_setting`: the game's own value, then the system's, then the global ones; empty is unset.
fn rocknix_setting<'a>(cfg: &'a str, key: &str, system: &str, game: &str) -> Option<&'a str> {
    let keys = [
        format!("{system}[\"{game}\"].{key}"),
        format!("{system}.{key}"),
        format!("system.{key}"),
        key.to_string(),
        format!("global.{key}"),
    ];
    keys.iter().find_map(|k| {
        cfg.lines()
            .find_map(|l| l.strip_prefix(k.as_str())?.strip_prefix('='))
            .map(str::trim)
            .filter(|v| !v.is_empty())
    })
}

/// Whether ROCKNIX's `hotkey_enable_button` makes Guide Dolphin's hotkey button for this game (Select otherwise).
fn hotkey_is_guide(system: &str, rom: &Path) -> bool {
    let cfg = std::fs::read_to_string(SYSTEM_CFG).unwrap_or_default();
    let game = rom.file_name().and_then(|n| n.to_str()).unwrap_or_default();
    rocknix_setting(&cfg, "hotkey_enable_button", system, game) == Some("mode")
}

#[derive(Clone)]
struct Session {
    emu: Emu,
    system: String,
    rom: PathBuf,
    /// State names without the slot: from the game file, or for ARMSX2 "<serial> (<CRC>)" once the game runs.
    base: Option<String>,
    /// Which launch this is, so a thread left over from an earlier game does nothing.
    id: u64,
}

static SESSION: Mutex<Option<Session>> = Mutex::new(None);
static LAUNCHES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn current() -> Option<Session> {
    SESSION.lock().ok()?.clone()
}

fn still(id: u64) -> bool {
    current().is_some_and(|s| s.id == id)
}

/// One action on the emulator at a time (review N4 / SOL-03): Save, Load, Quit, the resume load, the save before
/// sleep, an options restart, and the setting hotkeys all step Dolphin's slot with chords it counts globally and pass
/// states through the same transit and quick-slot files; two at once hit the wrong slot or park one file over
/// another's backup. Every worker takes its [`Turn`] first and waits for the one before to finish.
static ACTIONS: Mutex<()> = Mutex::new(());

thread_local! {
    /// The launch the action on this thread belongs to; `chord` and PINE refuse to reach a later game's emulator.
    static BOUND: std::cell::Cell<Option<u64>> = const { std::cell::Cell::new(None) };
}

/// A worker's turn at the emulator, for the game of one launch. Dropped: the next worker's turn.
struct Turn {
    _one: std::sync::MutexGuard<'static, ()>,
}

/// Waits for the actions before to finish, then the turn of `s`; None (and nothing done) when its game has ended
/// meanwhile, so a worker left over from an earlier game never presses keys or sends PINE to the next one.
fn turn(s: &Session) -> Option<Turn> {
    let one = ACTIONS.lock().unwrap_or_else(|e| e.into_inner());
    if !still(s.id) {
        println!("standalone: an action of a game that has ended is dropped");
        return None;
    }
    BOUND.set(Some(s.id));
    Some(Turn { _one: one })
}

impl Drop for Turn {
    fn drop(&mut self) {
        BOUND.set(None);
    }
}

/// False when this thread acts for a launch whose game has ended (a new one may be running in the same emulator).
fn bound_game_runs() -> bool {
    BOUND.get().is_none_or(still)
}

impl Session {
    /// The states of this game, once the names are known (ARMSX2: once the game runs).
    fn states(&self) -> Option<StateSet> {
        self.states_with(self.emu == Emu::Hakux && crate::hakux::core_has_states())
    }

    /// `states`, with whether hakuX's core has save states given (the core's `.info`, `hakux::states_supported`).
    fn states_with(&self, hakux_states: bool) -> Option<StateSet> {
        let base = self.base.as_deref()?;
        Some(match self.emu {
            Emu::Armsx2 => StateSet::armsx2(base),
            Emu::Dolphin => StateSet::dolphin(&self.system, base),
            Emu::Azahar => StateSet::azahar(base, self.rom.file_name()?.to_str()?),
            // Only once the core has save states: until then the menu offers none (`no_states_line`).
            Emu::Hakux if hakux_states => StateSet::hakux(base),
            Emu::Hakux => return None,
        })
    }

    /// Where `target` is written.
    fn state_path(&self, target: Target) -> Option<PathBuf> {
        self.states()?.target_path(target).map(Path::to_path_buf)
    }
}

/// Called before the game is started: writes what the integration needs (and, once, the tuned defaults) into
/// the emulator's config, and starts the thread that resumes from the Quit state. False for other emulators.
pub fn launch(system: &str, emulator: &str, core: &str, rom: &Path, data_dir: &Path) -> bool {
    let Some(emu) = Emu::detect(system, emulator, core).filter(|e| e.handles(rom)) else {
        exited();
        guest::launch(system, emulator, rom);
        return false;
    };
    defaults::apply(emu, system, rom, data_dir);
    if emu == Emu::Hakux && crate::hakux::prepare(data_dir, rom).is_none() {
        println!("standalone: hakuX's folders or config could not be written");
    }
    let id = LAUNCHES.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
    let session = Session {
        emu,
        system: system.to_string(),
        rom: rom.to_path_buf(),
        base: state_base(emu, rom),
        id,
    };
    migrate(&session, data_dir);
    // A slot parked by a Save or Load that pocket-ui didn't see through (Azahar's quick slot, Dolphin's `.s00`, SOL-04)
    // goes back before anything passes through it again.
    if let Some((quick, auto, undo)) = transit_of(&session) {
        azahar::recover(&quick, &auto, &undo);
    }
    // "Before load" of a session that didn't end with a Quit is gone with it.
    clear_undo(&session);
    if let Ok(mut s) = SESSION.lock() {
        *s = Some(session.clone());
    }
    std::thread::spawn(move || resume(session));
    true
}

/// The emulator slot our own states pass through, parked meanwhile (`azahar::park`), with the auto and undo states
/// that pass: Azahar's quick slot, Dolphin's `.s00`. None for ARMSX2 (PINE saves by number) or without state names.
fn transit_of(s: &Session) -> Option<(PathBuf, PathBuf, PathBuf)> {
    let states = s.states()?;
    let quick = match s.emu {
        Emu::Azahar => states.slots.first()?.clone(),
        Emu::Dolphin => StateSet::dolphin_transit(&s.system, s.base.as_deref()?),
        Emu::Armsx2 | Emu::Hakux => return None,
    };
    Some((quick, states.auto.first()?.clone(), states.undo.clone()))
}

/// The game ended (by any route).
pub fn exited() {
    if let Ok(mut s) = SESSION.lock() {
        *s = None;
    }
    guest::ended();
    restore_performance();
}

/// What `set_performance` changed while a standalone game ran, noted in this file until it is undone, so a crash of
/// pocket-ui (and of runemu.sh with it) is undone at its next start (`recover_performance`).
const PERFORMANCE_RESTORE: &str = "/tmp/pocket-ui-standalone.restore";
static POWER: Mutex<Option<crate::power::Power>> = Mutex::new(None);
/// The CPU's lease (`power::Lease`) while `POWER` has changed it: the game before has put it back first, and the
/// next one waits for `restore_performance`. Taken on the worker that applies, never on the UI thread.
static LEASE: Mutex<Option<crate::power::Lease>> = Mutex::new(None);

/// The CPU and GPU profile of the running PS2, GameCube, Wii or 3DS game: `max`, `balanced` (shown as Auto) or
/// `battery` (`emu_settings`, the Options row "Performance"; Quick settings' Performance for a game without its
/// own). Only once the emulator runs: runemu.sh sets ROCKNIX's governor before it starts it, which would replace ours.
pub fn set_performance(level: &str) -> bool {
    use crate::power::{Performance, Power};
    let mode = match level {
        "max" => Performance::Performance,
        "battery" => Performance::Battery,
        _ => Performance::Auto,
    };
    // Before `POWER` is locked: the UI thread's `restore_performance` must never wait for this.
    if LEASE.lock().is_ok_and(|l| l.is_none()) {
        let lease = crate::power::Lease::for_game("standalone");
        if let Ok(mut l) = LEASE.lock() {
            if l.is_none() {
                *l = lease;
            }
        }
    }
    let Ok(mut p) = POWER.lock() else { return false };
    let ok = p.get_or_insert_with(|| Power::with_root("/", PERFORMANCE_RESTORE)).set_standalone(mode);
    println!("standalone: performance {level}: {}", if ok { "set" } else { "not possible here" });
    ok
}

/// The game ended: what `set_performance` wrote and is still as it wrote it goes back (runemu.sh has put ROCKNIX's
/// own governors back by then, and those stay).
fn restore_performance() {
    if let Some(mut p) = POWER.lock().ok().and_then(|mut p| p.take()) {
        p.restore();
    }
    if let Ok(mut l) = LEASE.lock() {
        *l = None;
    }
}

/// At pocket-ui's start: undoes a profile left behind by a crash, once no big emulator runs any more.
pub fn recover_performance() {
    if !Path::new(PERFORMANCE_RESTORE).exists() {
        return;
    }
    std::thread::spawn(|| {
        while emulator_running() {
            std::thread::sleep(Duration::from_secs(5));
        }
        let _lease = crate::power::Lease::for_game("standalone recovery");
        if crate::power::Power::with_root("/", PERFORMANCE_RESTORE).recover() {
            println!("standalone: performance profile of an earlier game undone");
        }
    });
}

/// The Performance choice of the game: its own, its system's, or Games: performance (Auto is Max here,
/// `emu_settings::default_performance`).
fn chosen_performance(system: &str, rom: &Path) -> &'static str {
    let cfg = std::fs::read_to_string(SYSTEM_CFG).unwrap_or_default();
    crate::emu_settings::game_performance(system, &cfg, rom.file_name().and_then(|f| f.to_str()).unwrap_or_default())
}

/// A PS2, GameCube, Wii or 3DS game with Save/Load support is running.
pub fn active() -> bool {
    current().is_some()
}

/// The running game's states, if their names are known yet (`Session::states`).
pub fn states() -> Option<StateSet> {
    current()?.states()
}

/// Save from the pause menu into manual slot `target` (the game has just been resumed). `frame` is the picture
/// the menu was opened on, kept next to the state as its preview.
pub fn save(target: Target, frame: Option<Image>) {
    let Some(s) = current() else { return };
    std::thread::spawn(move || {
        let Some(_turn) = turn(&s) else { return };
        if save_target(&s, target) {
            write_preview(&s, target, frame);
        }
    });
}

/// Load from the pause menu (the game has just been resumed): the state about to be replaced is kept first as
/// "Before load" (`frame`, the picture the menu was opened on, is its preview), and a Quit after the load writes the
/// auto state from the game as it is then.
pub fn load(target: Target, frame: Option<Image>) {
    let Some(s) = current() else { return };
    std::thread::spawn(move || {
        let Some(_turn) = turn(&s) else { return };
        if s.states().is_some_and(|st| st.path(target).is_some()) {
            // Loading "Before load" itself leaves it as it is: it is what the next Load of it brings back.
            if target != Target::Undo && save_target(&s, Target::Undo) {
                write_preview(&s, Target::Undo, frame);
            }
            load_target(&s, target);
        }
    });
}

/// What Quit does, in order. Every step after the first is skipped once the emulator has ended.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Step {
    /// Write the state the next start resumes from.
    SaveResume,
    /// Ask the emulator's window to close, as its own close button would.
    CloseWindow,
    Signal(i32),
    /// Wait until PINE no longer reports a running VM (ARMSX2), at most this long.
    VmDown(Duration),
    /// Wait until the process has ended, at most this long.
    Exit(Duration),
    /// RetroArch's own `QUIT` command, on this game's network port.
    RaQuit,
    /// SIGKILL to the whole process group of the game: whatever is left of RetroArch and the core.
    KillGroup,
    /// SIGKILL every process of the emulator's name in the game's group (not runemu.sh, which restores the governor):
    /// RPCS3 starts a second `rpcs3-sa --error` for its error dialog, which outlives the first and stays on screen.
    KillSame,
}

/// Resume state first, then the emulator's own graceful shutdown (memory cards, settings, caches); force it
/// only if it hangs. Only the emulator is signalled, so runemu.sh lives on to restore the CPU governor and
/// the rest of its clean-up.
fn quit_plan(emu: Emu) -> &'static [Step] {
    use Step::*;
    match emu {
        // The first SIGTERM shuts the VM down (memory cards flushed) but leaves ARMSX2's own game list up;
        // a second one ends the process once the VM is gone.
        Emu::Armsx2 => &[
            SaveResume,
            Signal(libc::SIGTERM),
            VmDown(VM_SHUTDOWN_WAIT),
            Exit(EXIT_WAIT),
            Signal(libc::SIGTERM),
            Exit(KILL_WAIT),
            Signal(libc::SIGKILL),
        ],
        Emu::Dolphin => &[SaveResume, Signal(libc::SIGTERM), Exit(KILL_WAIT), Signal(libc::SIGKILL)],
        // Azahar has no SIGTERM handler (it would end on the spot); its window's close request stops the
        // emulation and saves its config (qt-config.ini, `confirmClose` off so no question comes up).
        Emu::Azahar => &[
            SaveResume,
            CloseWindow,
            Exit(AZAHAR_EXIT_WAIT),
            Signal(libc::SIGTERM),
            Exit(KILL_WAIT),
            Signal(libc::SIGKILL),
        ],
        // The state of a Quit only when the core has states (`Session::states`; else the step finds no file name and
        // does nothing). RetroArch's own QUIT first (it leaves the core cleanly), then the signals, then the group.
        Emu::Hakux => &[SaveResume, RaQuit, Exit(RA_EXIT_WAIT), Signal(libc::SIGTERM), Exit(KILL_WAIT), KillGroup],
    }
}

/// The steps of a Quit (`save`) or of a "Quit without saving", which leaves out the resume state and goes
/// straight to the emulator's own shutdown (its memory cards and in-game saves are flushed as on every exit).
fn quit_steps(emu: Emu, save: bool) -> impl Iterator<Item = Step> {
    quit_plan(emu).iter().copied().filter(move |s| save || *s != Step::SaveResume)
}

/// The longest a Quit takes when every step runs out its time.
#[cfg(test)]
fn quit_worst(emu: Emu) -> Duration {
    quit_worst_of(emu, true)
}

fn quit_worst_of(emu: Emu, save: bool) -> Duration {
    quit_steps(emu, save).fold(Duration::ZERO, |sum, step| match step {
        Step::SaveResume => sum + SAVE_WAIT,
        Step::VmDown(d) | Step::Exit(d) => sum + d,
        Step::CloseWindow | Step::Signal(_) | Step::RaQuit | Step::KillGroup | Step::KillSame => sum,
    })
}

/// Quit from the pause menu (the game has just been resumed): write the resume state (`save`; "Quit without
/// saving" leaves the one of the last Quit as it is), then let the emulator shut down by itself; force it only
/// if it hangs.
pub fn quit(pgid: Option<i32>, frame: Option<Image>, save: bool) {
    let Some(s) = current() else { return };
    std::thread::spawn(move || {
        // After a Save or Load still under way, never in the middle of it.
        let Some(_turn) = turn(&s) else { return };
        let started = Instant::now();
        println!("standalone: quitting {:?}{}, at most {:?}", s.emu, if save { "" } else { " without saving" }, quit_worst_of(s.emu, save));
        if quit_steps(s.emu, save).next() == Some(Step::SaveResume) && save_target(&s, Target::Auto) {
            write_preview(&s, Target::Auto, frame);
            println!("standalone: resume state written after {:?}", started.elapsed());
        }
        // "Before load" ends with a Quit.
        clear_undo(&s);
        close(s.emu, pgid);
        println!("standalone: {:?} closed after {:?}", s.emu, started.elapsed());
    });
}

/// Before the system sleeps or powers off (`sleep.rs`): writes the state that the next start resumes from,
/// the same slot Quit writes, and waits until it is complete (2–6 s). The emulator must be running (not
/// stopped with SIGSTOP). Blocks; call it from a worker thread. False when nothing was written.
pub fn save_resume_state() -> bool {
    let Some(s) = current() else { return false };
    let Some(_turn) = turn(&s) else { return false };
    save_target(&s, Target::Auto)
}

/// ARMSX2's slot number of a state (`StateSet::armsx2`).
fn armsx2_slot(target: Target) -> u8 {
    match target {
        Target::Auto => crate::states::ARMSX2_AUTO,
        Target::Undo => crate::states::ARMSX2_UNDO,
        Target::Slot(n) => n,
    }
}

/// RetroArch's slot number of a state (`StateSet::hakux`).
fn hakux_slot(target: Target) -> i32 {
    match target {
        Target::Auto => crate::hakux::AUTO_SLOT,
        Target::Undo => crate::hakux::UNDO_SLOT,
        Target::Slot(n) => i32::from(n),
    }
}

/// What the pause menu says in place of Save and Load for the running game, if its emulator has none yet.
pub fn no_states_line() -> Option<&'static str> {
    current().filter(|s| s.emu == Emu::Hakux && s.states().is_none()).map(|_| crate::hakux::NO_STATES)
}

/// Writes `target` and waits until the file is complete. False if the emulator didn't write it. Called with the
/// session's [`Turn`] held.
fn save_target(s: &Session, target: Target) -> bool {
    let Some(dest) = s.state_path(target) else { return false };
    // What the file is before we ask: only another version of it afterwards confirms the save (SOL-12).
    let before = crate::fsutil::stamp(&dest);
    let ok = match (s.emu, target) {
        (Emu::Armsx2, _) => pine::call(pine::SAVE_STATE, &[armsx2_slot(target)]).is_some() && wait_written(s.emu, &dest, before),
        (Emu::Dolphin, Target::Slot(n)) => {
            dolphin::in_slot(n, dolphin::Action::Save, hotkey_is_guide(&s.system, &s.rom)) && wait_written(s.emu, &dest, before)
        }
        (Emu::Dolphin, _) => {
            let Some(transit) = s.base.as_deref().map(|b| StateSet::dolphin_transit(&s.system, b)) else { return false };
            dolphin::through_transit_save(&transit, &dest, hotkey_is_guide(&s.system, &s.rom))
        }
        (Emu::Azahar, Target::Slot(_)) => s.state_path(Target::Slot(1)).is_some_and(|quick| azahar::save(&quick, None)),
        (Emu::Azahar, _) => match s.state_path(Target::Slot(1)) {
            Some(quick) => azahar::save(&quick, Some(&dest)),
            None => false,
        },
        (Emu::Hakux, _) => crate::hakux::send(&crate::hakux::save_command(hakux_slot(target))) && wait_written(s.emu, &dest, before),
    };
    println!("standalone: save {target:?}: {}", if ok { "written" } else { "failed" });
    ok
}

fn load_target(s: &Session, target: Target) -> bool {
    let Some(path) = s.states().and_then(|st| st.path(target).map(Path::to_path_buf)) else {
        return false;
    };
    let ok = match (s.emu, target) {
        (Emu::Armsx2, _) => pine::call(pine::LOAD_STATE, &[armsx2_slot(target)]).is_some(),
        (Emu::Dolphin, Target::Slot(n)) => dolphin::in_slot(n, dolphin::Action::Load, hotkey_is_guide(&s.system, &s.rom)),
        (Emu::Dolphin, _) => match s.base.as_deref().map(|b| StateSet::dolphin_transit(&s.system, b)) {
            Some(transit) => dolphin::through_transit_load(&transit, &path, hotkey_is_guide(&s.system, &s.rom)),
            None => false,
        },
        (Emu::Azahar, Target::Slot(_)) => s.state_path(Target::Slot(1)).is_some_and(|quick| azahar::load(&quick, None)),
        (Emu::Azahar, _) => match s.state_path(Target::Slot(1)) {
            Some(quick) => azahar::load(&quick, Some(path.as_path())),
            None => false,
        },
        (Emu::Hakux, _) => crate::hakux::send(&crate::hakux::load_command(hakux_slot(target))),
    };
    println!("standalone: load {target:?}: {}", if ok { "sent" } else { "failed" });
    ok
}

/// Drops "Before load" (a Quit ends it, and so does the start of a game).
fn clear_undo(s: &Session) {
    if let Some(undo) = s.state_path(Target::Undo) {
        let _ = std::fs::remove_file(crate::slots::preview_path(&undo));
        let _ = std::fs::remove_file(undo);
    }
}

/// Moves the states an older version wrote where they are now (see `migrate_armsx2_in`, `migrate_dolphin_in`).
fn migrate(s: &Session, data_dir: &Path) {
    let (Some(base), dir) = (s.base.as_deref(), Path::new(crate::slots::STATES_DIR).join(&s.system)) else { return };
    if s.emu == Emu::Dolphin {
        migrate_dolphin_in(&dir, base, &data_dir.join("states-v2").join(&s.system).join(base));
    }
}

/// ARMSX2 wrote the state of a Quit to slot 0, where a player's own slot 0 lives (Android's). The one pocket-ui wrote
/// has a preview picture beside it (`<state>.png`, which ARMSX2 doesn't write); it moves to the auto state (slot 99)
/// unless that exists already. A slot 0 without a preview is the player's and stays.
fn migrate_armsx2_in(dir: &Path, base: &str) {
    let old = dir.join(format!("{base}.{:02}.p2s", crate::states::ARMSX2_OLD_AUTO));
    let new = dir.join(format!("{base}.{:02}.p2s", crate::states::ARMSX2_AUTO));
    let (old_png, new_png) = (crate::slots::preview_path(&old), crate::slots::preview_path(&new));
    if old.exists() && old_png.exists() && !new.exists() && std::fs::rename(&old, &new).is_ok() {
        let _ = std::fs::rename(old_png, new_png);
    }
}

/// Dolphin's old layout: Save wrote `<game>.s00`, Quit `<game>.s01`. Now `.s01` to `.s08` are the manual slots and
/// the auto state is a file of ours. Once per game (`marker`): an `.s01` with a preview beside it (pocket-ui wrote the
/// state of a Quit with one; Dolphin writes none) becomes the auto state, and the old Save in `.s00` becomes slot 1
/// where that is free. An `.s01` without a preview is a player's own and stays; nothing is overwritten.
fn migrate_dolphin_in(dir: &Path, base: &str, marker: &Path) {
    if marker.exists() {
        return;
    }
    let file = |suffix: &str| dir.join(format!("{base}{suffix}"));
    let (s00, s01, auto) = (file(".s00"), file(".s01"), file(".pocket-auto"));
    let png = crate::slots::preview_path;
    if s01.exists() && png(&s01).exists() && !auto.exists() && std::fs::rename(&s01, &auto).is_ok() {
        let _ = std::fs::rename(png(&s01), png(&auto));
    }
    if s00.exists() && !s01.exists() && std::fs::rename(&s00, &s01).is_ok() && png(&s00).exists() {
        let _ = std::fs::rename(png(&s00), png(&s01));
    }
    let _ = crate::fsutil::write_atomic(marker, b"1\n");
}

/// Waits for the game to run, learns the state names, and loads the state written on the last Quit.
fn resume(mut s: Session) {
    let deadline = Instant::now() + Duration::from_secs(120);
    let wait_for = |ready: &dyn Fn() -> bool| {
        while !ready() {
            if Instant::now() > deadline || !still(s.id) {
                return false;
            }
            std::thread::sleep(Duration::from_millis(250));
        }
        true
    };
    match s.emu {
        Emu::Armsx2 => {
            // PINE answers once the emulator is up; the VM reports Running when the game boots.
            if !wait_for(&|| pine::status() == Some(pine::RUNNING)) {
                return;
            }
            let (Some(serial), Some(crc)) = (pine::string(pine::ID), pine::string(pine::UUID)) else {
                return;
            };
            s.base = Some(format!("{serial} ({})", crc.to_uppercase()));
            if let Ok(mut cur) = SESSION.lock() {
                match cur.as_mut() {
                    Some(c) if c.id == s.id => c.base = s.base.clone(),
                    _ => return,
                }
            }
            migrate_armsx2_in(Path::new(&format!("{}/ps2", crate::slots::STATES_DIR)), s.base.as_deref().unwrap_or_default());
            clear_undo(&s);
        }
        Emu::Dolphin => {
            // The hotkeys are read by Dolphin's main loop once the pad is up, about when its window appears.
            if !wait_for(&dolphin::window_open) {
                return;
            }
            std::thread::sleep(dolphin::SETTLE);
        }
        Emu::Azahar => {
            // The window title names the game once it has booted.
            if !wait_for(&azahar::game_running) {
                return;
            }
            std::thread::sleep(azahar::SETTLE);
        }
        Emu::Hakux => {
            // RetroArch answers GET_STATUS once the core runs the game; the core needs a moment to boot the console.
            if !wait_for(&|| crate::hakux::status().is_some_and(|st| crate::hakux::playing(&st))) {
                return;
            }
            std::thread::sleep(Duration::from_secs(2));
        }
    }
    if let Some(_turn) = turn(&s) {
        // The emulator runs, so runemu.sh has set ROCKNIX's governor already: ours goes on top of it.
        set_performance(chosen_performance(&s.system, &s.rom));
        load_target(&s, Target::Auto);
    }
}

/// Runs the rest of `quit_plan` on the emulator's process.
fn close(emu: Emu, pgid: Option<i32>) {
    run_plan(quit_plan(emu), emu.process(), pgid);
}

/// Runs `plan` on the process named `process` (in the game's group `pgid`) until it has ended.
fn run_plan(plan: &[Step], process: &str, pgid: Option<i32>) {
    let Some(pid) = find_pid(pgid, process) else { return };
    let alive = || Path::new(&format!("/proc/{pid}")).exists();
    let wait = |within: Duration, done: &dyn Fn() -> bool| {
        let end = Instant::now() + within;
        while Instant::now() < end && !done() {
            std::thread::sleep(Duration::from_millis(100));
        }
    };
    for step in plan {
        // Whatever else of the emulator's name is left (a second process of RPCS3) goes even if the first one is gone.
        if !alive() && *step != Step::KillSame {
            continue;
        }
        match *step {
            Step::SaveResume => {}
            Step::KillSame => kill_same(process, pgid),
            // Every process of the name that has a window (RPCS3's error dialog is a second one).
            Step::CloseWindow => {
                for p in pids_of(pgid, process) {
                    azahar::close_window(p);
                }
            }
            // SAFETY: kill(2) on the emulator's own process.
            Step::Signal(sig) => unsafe {
                libc::kill(pid, sig);
            },
            Step::VmDown(within) => wait(within, &|| !matches!(pine::status(), Some(s) if s != pine::SHUTDOWN)),
            Step::Exit(within) => wait(within, &|| !alive()),
            Step::RaQuit => {
                // The menu still covers the game's window, and RetroArch/hakuX hangs in presenting to a covered surface:
                // QUIT and SIGTERM then wait until SIGKILL (18 s, measured on the Mini V2). In front, it ends in ~0.3 s.
                crate::control::show_game();
                crate::hakux::send("QUIT");
            }
            // SAFETY: kill(2) on the process group pocket-ui started for this game.
            Step::KillGroup => unsafe {
                if let Some(g) = pgid.filter(|g| *g > 1) {
                    libc::kill(-g, libc::SIGKILL);
                }
            },
        }
    }
}

/// SIGKILL to every process whose name starts with `name` in the group `pgid` (any group when None).
fn kill_same(name: &str, pgid: Option<i32>) {
    while let Some(pid) = find_pid(pgid, name) {
        // SAFETY: kill(2) on a process of the emulator's own name in the game's group.
        unsafe {
            libc::kill(pid, libc::SIGKILL);
        }
        std::thread::sleep(Duration::from_millis(100));
        if Path::new(&format!("/proc/{pid}")).exists() {
            break;
        }
    }
}

/// The process of `pgid` whose name starts with `name` (any process of that name without a group).
fn find_pid(pgid: Option<i32>, name: &str) -> Option<i32> {
    pids_of(pgid, name).first().copied()
}

/// Every process of `pgid` whose name starts with `name`.
fn pids_of(pgid: Option<i32>, name: &str) -> Vec<i32> {
    let Ok(dir) = std::fs::read_dir("/proc") else { return Vec::new() };
    dir.flatten().filter_map(|e| {
        let pid: i32 = e.file_name().to_str()?.parse().ok()?;
        let stat = std::fs::read_to_string(e.path().join("stat")).ok()?;
        let (comm, rest) = (stat.get(stat.find('(')? + 1..stat.rfind(')')?)?, stat.get(stat.rfind(')')? + 2..)?);
        let group: i32 = rest.split(' ').nth(2)?.parse().ok()?;
        (comm.starts_with(name) && pgid.is_none_or(|g| g == group)).then_some(pid)
    }).collect()
}

/// True once `emu` has written a new version of `path` (not the one `before` stamped) and finished it: the same in two
/// looks, and no longer open in the emulator (`fsutil::wait_new_version`). At most `SAVE_WAIT`.
fn wait_written(emu: Emu, path: &Path, before: Option<crate::fsutil::Stamp>) -> bool {
    crate::fsutil::wait_new_version(path, before, SAVE_WAIT, Duration::from_millis(300), find_pid(None, emu.process()))
}

/// `<state>.png` at half the screen size, like RetroArch's and pocket-core's previews.
fn write_preview(s: &Session, target: Target, frame: Option<Image>) {
    let (Some(frame), Some(state)) = (frame, s.state_path(target)) else { return };
    let small = frame.resized(frame.w as i32 / 2, frame.h as i32 / 2);
    let mut path = state.into_os_string();
    path.push(".png");
    let rgb: Vec<u8> = small.px.iter().flat_map(|p| [(p >> 16) as u8, (p >> 8) as u8, *p as u8]).collect();
    let mut png_bytes = Vec::new();
    let mut enc = png::Encoder::new(&mut png_bytes, small.w, small.h);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    // A new temporary file, never one (or a link) found there (review N3).
    if enc.write_header().and_then(|mut w| w.write_image_data(&rgb)).is_ok() {
        let _ = crate::fsutil::replace_file(Path::new(&path), &png_bytes);
    }
}

/// Presses `buttons` (InputPlumber capability names, pad buttons or keyboard keys) together: pressed in
/// order, released in reverse. Returns once the last release has gone out and the emulator has seen it.
fn chord(buttons: &[&str]) -> bool {
    if !bound_game_runs() {
        println!("standalone: keys of a game that has ended are not pressed");
        return false;
    }
    let ok = crate::proc::status_with_timeout(
        std::process::Command::new("busctl")
            .args(["call", "org.shadowblip.InputPlumber", "/org/shadowblip/InputPlumber/CompositeDevice0"])
            .args(["org.shadowblip.Input.CompositeDevice", "SendButtonChord", "as"])
            .arg(buttons.len().to_string())
            .args(buttons),
        crate::proc::HELPER,
    )
    .is_ok_and(|s| s.success());
    std::thread::sleep(chord_time(buttons.len()));
    ok
}

/// SendButtonChord answers at once and sends one event every 80 ms; then Dolphin's loop has to see the
/// hotkey released before it takes the next combination.
fn chord_time(buttons: usize) -> Duration {
    Duration::from_millis(80 * (2 * buttons as u64).saturating_sub(1) + 90)
}

/// PCSX2's PINE protocol: `[u32 size][u8 opcode][args]` in, `[u32 size][u8 0 = ok][data]` out.
mod pine {
    use std::io::{Read, Write};
    use std::os::unix::net::UnixStream;
    use std::time::Duration;

    pub const SAVE_STATE: u8 = 0x9;
    pub const LOAD_STATE: u8 = 0xA;
    pub const ID: u8 = 0xC;
    pub const UUID: u8 = 0xD;
    pub const STATUS: u8 = 0xF;
    pub const RUNNING: u32 = 0;
    pub const SHUTDOWN: u32 = 2;

    fn socket() -> String {
        let dir = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".into());
        format!("{dir}/pcsx2.sock")
    }

    pub fn call(op: u8, args: &[u8]) -> Option<Vec<u8>> {
        // A worker of a game that has ended never reaches the next one's ARMSX2 (`super::Turn`).
        if !super::bound_game_runs() {
            return None;
        }
        let mut s = UnixStream::connect(socket()).ok()?;
        s.set_read_timeout(Some(Duration::from_secs(3))).ok()?;
        s.set_write_timeout(Some(Duration::from_secs(3))).ok()?;
        let mut req = ((args.len() + 5) as u32).to_le_bytes().to_vec();
        req.push(op);
        req.extend_from_slice(args);
        s.write_all(&req).ok()?;
        let mut head = [0u8; 4];
        s.read_exact(&mut head).ok()?;
        let mut body = vec![0; (u32::from_le_bytes(head) as usize).checked_sub(4)?];
        s.read_exact(&mut body).ok()?;
        (body.first() == Some(&0)).then(|| body[1..].to_vec())
    }

    /// Replies carrying a string: `[u32 length][bytes][NUL]`.
    pub fn string(op: u8) -> Option<String> {
        let reply = call(op, &[])?;
        let text = reply.get(4..)?;
        let end = text.iter().position(|b| *b == 0).unwrap_or(text.len());
        Some(String::from_utf8_lossy(&text[..end]).into_owned()).filter(|s| !s.is_empty())
    }

    /// 0 running, 1 paused, 2 no game.
    pub fn status() -> Option<u32> {
        let reply = call(STATUS, &[])?;
        Some(u32::from_le_bytes(reply.get(..4)?.try_into().ok()?))
    }
}

/// ROCKNIX's pad hotkeys in Dolphin nogui (patch 000-hotkeys), pressed through InputPlumber. Wii games too:
/// the patch reads the GameCube pad of port 1, which start_dolphin_wii.sh configures for exactly that.
mod dolphin {
    use std::path::Path;
    use std::process::Command;
    use std::time::Duration;

    /// Time between the window appearing and the hotkeys being read reliably.
    pub const SETTLE: Duration = Duration::from_secs(3);
    const L3: &str = "Gamepad:Button:LeftStick";
    const R3: &str = "Gamepad:Button:RightStick";
    const UP: &str = "Gamepad:Button:DPadUp";
    const DOWN: &str = "Gamepad:Button:DPadDown";

    #[derive(Clone, Copy, PartialEq, Debug)]
    pub enum Action {
        Save,
        Load,
    }

    pub fn hotkey_name(guide: bool) -> &'static str {
        if guide {
            "Gamepad:Button:Guide"
        } else {
            "Gamepad:Button:Select"
        }
    }

    /// HOTKEY + Up/Down with both stick clicks held, which the profiles take as neither L nor R
    /// (`defaults::with_extra_triggers`) and the Wiimote as nothing (`defaults::with_quiet_wiimote`).
    fn step(hk: &'static str, dir: &'static str) -> Vec<&'static str> {
        vec![L3, R3, hk, dir]
    }

    /// The stick click first, then HOTKEY: GC R (save) or L (load) through the profiles' extra binding,
    /// and the Wiimote never sees HOTKEY alone.
    fn act(action: Action, hk: &'static str) -> Vec<&'static str> {
        match action {
            Action::Save => vec![R3, hk],
            Action::Load => vec![L3, hk],
        }
    }

    /// The chords that run `action` on `slot`. Dolphin starts every game on slot 0 and only steps it up and
    /// down, so other slots are reached by stepping there and back.
    pub fn chords(slot: u8, action: Action, hk: &'static str) -> Vec<Vec<&'static str>> {
        let mut v: Vec<_> = (0..slot).map(|_| step(hk, UP)).collect();
        v.push(act(action, hk));
        v.extend((0..slot).map(|_| step(hk, DOWN)));
        v
    }

    /// Presses `chords(slot, action, …)`; if a step up fails, the action is left out (it would hit the wrong
    /// slot) and only the steps that went out are undone.
    pub fn in_slot(slot: u8, action: Action, guide: bool) -> bool {
        let plan = chords(slot, action, hotkey_name(guide));
        let (ups, rest) = plan.split_at(slot as usize);
        let (act, downs) = rest.split_first().expect("chords() always holds the action");
        let sent = ups.iter().take_while(|c| super::chord(c)).count();
        let ok = sent == ups.len() && super::chord(act);
        for c in &downs[..sent] {
            super::chord(c);
        }
        ok
    }

    /// One of ROCKNIX's setting hotkeys (HOTKEY + GC Y, X or B; `button` is the InputPlumber button the
    /// profile binds to it), with both stick clicks held first like the slot steps: the profiles take that as
    /// neither L nor R, so it never saves or loads by accident.
    pub fn toggle(button: &'static str, hk: &'static str) -> Vec<&'static str> {
        vec![L3, R3, hk, button]
    }

    /// How long a load from the transit slot may take to start; Dolphin reads the file whole.
    const LOAD_WAIT: Duration = Duration::from_secs(10);
    const LOADING: Duration = Duration::from_millis(25);

    /// Saves into the slot the hotkeys start on (`.s00`, no stepping) and moves the file to `dest`; what was in
    /// `.s00` is put back.
    pub fn through_transit_save(transit: &Path, dest: &Path, guide: bool) -> bool {
        let Ok(backup) = super::azahar::park(transit) else { return false };
        // Parked: whatever appears in the transit slot now is the state just asked for.
        let ok = in_slot(0, Action::Save, guide) && super::wait_written(super::Emu::Dolphin, transit, None) && std::fs::rename(transit, dest).is_ok();
        if !ok {
            let _ = std::fs::remove_file(transit);
        }
        super::azahar::unpark(transit, backup);
        ok
    }

    /// Puts `from` into `.s00` (a hard link) and loads that slot; once Dolphin has opened the file it goes again and
    /// what was in `.s00` comes back.
    pub fn through_transit_load(transit: &Path, from: &Path, guide: bool) -> bool {
        let Ok(backup) = super::azahar::park(transit) else { return false };
        let linked = std::fs::hard_link(from, transit).or_else(|_| std::fs::copy(from, transit).map(|_| ())).is_ok();
        let watch = if linked { super::azahar::opened::Watch::new(transit) } else { None };
        let ok = linked && in_slot(0, Action::Load, guide);
        if ok {
            let loaded = watch.map(|w| w.held_open(LOADING, LOAD_WAIT));
            println!("standalone: dolphin opened the state: {loaded:?}");
        }
        let _ = std::fs::remove_file(transit);
        super::azahar::unpark(transit, backup);
        ok
    }

    /// Dolphin's state names (ROCKNIX patch 002): a disc image's file name; executables (DOL, ELF) use
    /// Dolphin's game id "ID-<name>"; a playlist the first disc. WAD channels are named after the id in the
    /// file, which we don't read.
    pub fn state_base(rom: &Path) -> Option<String> {
        let ext = rom.extension().and_then(|e| e.to_str()).unwrap_or_default().to_ascii_lowercase();
        match ext.as_str() {
            "dol" | "elf" => Some(format!("ID-{}", rom.file_stem()?.to_str()?)),
            "wad" => None,
            "m3u" => {
                let list = std::fs::read_to_string(rom).ok()?;
                let first = list.lines().map(str::trim).find(|l| !l.is_empty() && !l.starts_with('#'))?;
                Some(Path::new(first).file_name()?.to_str()?.to_string())
            }
            _ => Some(rom.file_name()?.to_str()?.to_string()),
        }
    }

    /// Dolphin's render window is mapped in sway (it has no app_id; its title starts with "Dolphin").
    pub fn window_open() -> bool {
        crate::proc::run_with_timeout(Command::new("swaymsg").args(["-t", "get_tree"]), crate::control::SWAY_TIMEOUT)
            .is_ok_and(|o| String::from_utf8_lossy(&o.stdout).contains("\"name\": \"Dolphin"))
    }
}

/// Azahar (ROCKNIX's azahar-sa, Qt): its keyboard shortcuts pressed on InputPlumber's keyboard, its one quick
/// slot, and the window it runs in.
mod azahar {
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use std::time::Duration;

    /// Quick Save and Quick Load (slot 0) in ROCKNIX's qt-config.ini; ROCKNIX's HOME+R1/L1 send the same.
    pub const SAVE: &[&str] = &["Keyboard:KeyLeftAlt", "Keyboard:KeyS"];
    pub const LOAD: &[&str] = &["Keyboard:KeyLeftAlt", "Keyboard:KeyL"];
    /// Time between the game's title appearing and a state load being taken.
    pub const SETTLE: Duration = Duration::from_secs(3);
    /// How long a load of the resume state may take to start.
    const LOAD_WAIT: Duration = Duration::from_secs(10);
    /// Azahar reads a state's header several times (listing the slots) before it loads the whole file, which
    /// keeps it open for the read and the decompression; a header read takes microseconds.
    const LOADING: Duration = Duration::from_millis(25);

    /// Program id of a CCI/CXI game (Azahar names states `<id as 16 hex digits>.NN.cst`); 0 for 3DSX and
    /// ELF homebrew, which have none. None for CIA (Azahar can't boot it) and compressed images.
    pub fn title_id(rom: &Path) -> Option<u64> {
        let ext = rom.extension().and_then(|e| e.to_str()).unwrap_or_default().to_ascii_lowercase();
        match ext.as_str() {
            "3dsx" | "z3dsx" | "elf" | "axf" => Some(0),
            "3ds" | "cci" | "cxi" | "app" => {
                use std::io::{Read, Seek, SeekFrom};
                let mut f = std::fs::File::open(rom).ok()?;
                let mut read_at = |at: u64| {
                    let mut head = [0u8; 0x200];
                    f.seek(SeekFrom::Start(at)).ok()?;
                    f.read_exact(&mut head).ok()?;
                    Some(head)
                };
                let head = read_at(0)?;
                // NCSD: partition 0's offset (in 0x200-byte units) at 0x120; its NCCH header carries the id.
                let ncch = match &head[0x100..0x104] {
                    b"NCCH" => head,
                    b"NCSD" => read_at(u32::from_le_bytes(head[0x120..0x124].try_into().ok()?) as u64 * 0x200)?,
                    _ => return None,
                };
                (&ncch[0x100..0x104] == b"NCCH").then(|| u64::from_le_bytes(ncch[0x118..0x120].try_into().unwrap()))
            }
            _ => None,
        }
    }

    /// Sets `key=value` in `[section]` of a Qt settings file, and `key\default=false`: with `\default=true`
    /// Azahar ignores the value and uses its built-in default.
    pub fn qt_set(text: &str, section: &str, key: &str, value: &str) -> String {
        let t = qt_one(text, section, key, value);
        qt_one(&t, section, &format!("{key}\\default"), "false")
    }

    fn qt_one(text: &str, section: &str, key: &str, value: &str) -> String {
        let header = format!("[{section}]");
        let line = format!("{key}={value}");
        let mut out: Vec<String> = Vec::new();
        let (mut in_section, mut done) = (false, false);
        for l in text.lines() {
            if l.starts_with('[') {
                if in_section && !done {
                    let at = out.iter().rposition(|o| !o.trim().is_empty()).map_or(out.len(), |i| i + 1);
                    out.insert(at, line.clone());
                    done = true;
                }
                in_section = l.trim() == header;
            } else if in_section && !done && l.split_once('=').is_some_and(|(k, _)| k == key) {
                out.push(line.clone());
                done = true;
                continue;
            }
            out.push(l.to_string());
        }
        if !done {
            if !in_section {
                out.push(String::new());
                out.push(header);
            }
            out.push(line);
        }
        out.join("\n") + "\n"
    }

    /// A window titled "Azahar <version> | <game>" (it has no game part before one has booted).
    pub fn game_window(tree: &str) -> bool {
        fn find(node: &serde_json::Value) -> bool {
            node["name"].as_str().is_some_and(|n| n.starts_with("Azahar") && n.contains(" | "))
                || node["nodes"]
                    .as_array()
                    .into_iter()
                    .chain(node["floating_nodes"].as_array())
                    .flatten()
                    .any(find)
        }
        serde_json::from_str::<serde_json::Value>(tree).is_ok_and(|t| find(&t))
    }

    pub fn game_running() -> bool {
        crate::proc::run_with_timeout(Command::new("swaymsg").args(["-t", "get_tree"]), crate::control::SWAY_TIMEOUT)
            .is_ok_and(|o| game_window(&String::from_utf8_lossy(&o.stdout)))
    }

    fn pid() -> Option<i32> {
        super::find_pid(None, super::Emu::Azahar.process())
    }

    fn swaymsg(cmd: &str) -> bool {
        crate::proc::run_with_timeout(Command::new("swaymsg").arg(cmd), crate::control::SWAY_TIMEOUT).is_ok_and(|o| o.status.success())
    }

    /// Keyboard shortcuts reach the focused window only, and sway doesn't focus a window hidden behind
    /// pocket-ui's: Azahar is put in front while it saves or loads. With `restore` (Quit, sleep, resume)
    /// pocket-ui goes back in front after if it was there; the menu's own Save and Load leave the game shown
    /// (the menu is closing, and may still look focused when this starts).
    fn in_front<T>(restore: bool, f: impl FnOnce() -> T) -> T {
        let ui_in_front = crate::control::focused_window(crate::control::SWAY_TIMEOUT).is_none();
        if let Some(pid) = pid() {
            let own = crate::control::OWN_APP_ID;
            if swaymsg(&format!(
                r#"[app_id="{own}"] fullscreen disable; [pid={pid}] focus; [pid={pid}] fullscreen enable"#
            )) && ui_in_front
            {
                // Qt takes the keyboard focus a moment after sway gives it.
                std::thread::sleep(Duration::from_millis(300));
            }
        }
        let result = f();
        if restore && ui_in_front {
            crate::control::show_ui();
        }
        result
    }

    /// Asks Azahar's window to close (Exit, without the question: `confirmClose` is off).
    pub fn close_window(pid: i32) -> bool {
        swaymsg(&format!("[pid={pid}] kill"))
    }

    /// The same for every process of a port's group in one call (whichever of them owns the window).
    pub fn close_windows(pids: &[i32]) -> bool {
        let cmd = pids.iter().map(|p| format!("[pid={p}] kill")).collect::<Vec<_>>().join("; ");
        swaymsg(&cmd)
    }

    fn backup_of(quick: &Path) -> PathBuf {
        PathBuf::from(format!("{}.pocket-ui", quick.display()))
    }

    /// Moves the quick slot aside while the resume state passes through it. None when there is none. Refused while
    /// an earlier backup is still there (it is the player's own slot; `recover` puts it back first): parking again
    /// would replace it (SOL-04).
    pub fn park(quick: &Path) -> std::io::Result<Option<PathBuf>> {
        let backup = backup_of(quick);
        if backup.symlink_metadata().is_ok() {
            eprintln!("standalone: {} is still parked; not parking over it", quick.display());
            return Err(std::io::Error::from(std::io::ErrorKind::AlreadyExists));
        }
        if !quick.exists() {
            return Ok(None);
        }
        std::fs::rename(quick, &backup)?;
        Ok(Some(backup))
    }

    pub fn unpark(quick: &Path, backup: Option<PathBuf>) {
        if let Some(b) = backup {
            let _ = std::fs::rename(b, quick);
        }
    }

    fn same_file(a: &Path, b: &Path) -> bool {
        use std::os::unix::fs::MetadataExt;
        matches!((std::fs::metadata(a), std::fs::metadata(b)), (Ok(x), Ok(y)) if x.dev() == y.dev() && x.ino() == y.ino())
    }

    /// After pocket-ui stopped in the middle of a Quit save or a resume: the quick slot back in place, a
    /// resume state written meanwhile kept as such. A link of the auto or undo state (a load) just goes.
    pub fn recover(quick: &Path, auto: &Path, undo: &Path) {
        let backup = backup_of(quick);
        if !backup.exists() {
            return;
        }
        if quick.exists() {
            if same_file(quick, auto) || same_file(quick, undo) {
                let _ = std::fs::remove_file(quick);
            } else {
                let _ = std::fs::rename(quick, auto);
            }
        }
        let _ = std::fs::rename(&backup, quick);
    }

    /// Quick Save into `quick`; with `to`, the state is moved there and the quick slot kept as it was.
    pub fn save(quick: &Path, to: Option<&Path>) -> bool {
        let Some(to) = to else {
            let before = crate::fsutil::stamp(quick);
            return in_front(false, || super::chord(SAVE) && super::wait_written(super::Emu::Azahar, quick, before));
        };
        let Ok(backup) = park(quick) else { return false };
        let ok = in_front(true, || super::chord(SAVE) && super::wait_written(super::Emu::Azahar, quick, None)) && std::fs::rename(quick, to).is_ok();
        if !ok {
            let _ = std::fs::remove_file(quick);
        }
        unpark(quick, backup);
        ok
    }

    /// Quick Load from `quick`; with `from`, that state is put into the quick slot (a hard link) until
    /// Azahar has opened it, then the quick slot is put back.
    pub fn load(quick: &Path, from: Option<&Path>) -> bool {
        let Some(from) = from else {
            return in_front(false, || super::chord(LOAD));
        };
        let Ok(backup) = park(quick) else { return false };
        let linked = std::fs::hard_link(from, quick).or_else(|_| std::fs::copy(from, quick).map(|_| ())).is_ok();
        let watch = if linked { opened::Watch::new(quick) } else { None };
        let ok = linked && in_front(true, || super::chord(LOAD));
        if ok {
            let loaded = watch.map(|w| w.held_open(LOADING, LOAD_WAIT));
            println!("standalone: azahar opened the resume state: {loaded:?}");
        }
        let _ = std::fs::remove_file(quick);
        unpark(quick, backup);
        ok
    }

    /// Tells when a file is being read as a whole, through inotify.
    #[cfg(target_os = "linux")]
    pub mod opened {
        use std::os::unix::ffi::OsStrExt;
        use std::path::Path;
        use std::time::{Duration, Instant};

        pub struct Watch(i32);

        impl Watch {
            pub fn new(path: &Path) -> Option<Watch> {
                let name = std::ffi::CString::new(path.as_os_str().as_bytes()).ok()?;
                // SAFETY: plain inotify calls; the descriptor is owned by Watch and closed on drop.
                let fd = unsafe { libc::inotify_init1(libc::IN_NONBLOCK | libc::IN_CLOEXEC) };
                if fd < 0 {
                    return None;
                }
                let w = Watch(fd);
                (unsafe { libc::inotify_add_watch(fd, name.as_ptr(), libc::IN_OPEN | libc::IN_CLOSE) } >= 0).then_some(w)
            }

            /// True once the file has stayed open for `held`; false if that didn't happen `within`.
            pub fn held_open(&self, held: Duration, within: Duration) -> bool {
                let end = Instant::now() + within;
                let mut open_since: Option<Instant> = None;
                loop {
                    let now = Instant::now();
                    if open_since.is_some_and(|t| now >= t + held) {
                        return true;
                    }
                    if now >= end {
                        return false;
                    }
                    let until = open_since.map_or(end, |t| (t + held).min(end));
                    let mut pfd = libc::pollfd {
                        fd: self.0,
                        events: libc::POLLIN,
                        revents: 0,
                    };
                    let ms = until.saturating_duration_since(now).as_millis().clamp(1, 1000) as i32;
                    // SAFETY: one pollfd on our own descriptor.
                    if unsafe { libc::poll(&mut pfd, 1, ms) } <= 0 {
                        continue;
                    }
                    let mut buf = [0u8; 4096];
                    // SAFETY: reads into a buffer of the given size.
                    let n = unsafe { libc::read(self.0, buf.as_mut_ptr().cast(), buf.len()) };
                    let mut at = 0usize;
                    // struct inotify_event: wd i32, mask u32, cookie u32, len u32, name[len].
                    while n > 0 && at + 16 <= n as usize {
                        let mask = u32::from_ne_bytes(buf[at + 4..at + 8].try_into().unwrap());
                        let len = u32::from_ne_bytes(buf[at + 12..at + 16].try_into().unwrap()) as usize;
                        if mask & libc::IN_OPEN != 0 {
                            open_since = Some(Instant::now());
                        }
                        if mask & libc::IN_CLOSE != 0 {
                            open_since = None;
                        }
                        at += 16 + len;
                    }
                }
            }
        }

        impl Drop for Watch {
            fn drop(&mut self) {
                // SAFETY: closes the descriptor this Watch owns.
                unsafe { libc::close(self.0) };
            }
        }
    }

    /// Elsewhere (unit tests on the Mac): a fixed wait.
    #[cfg(not(target_os = "linux"))]
    pub mod opened {
        use std::path::Path;
        use std::time::Duration;

        pub struct Watch;

        impl Watch {
            pub fn new(_path: &Path) -> Option<Watch> {
                Some(Watch)
            }

            pub fn held_open(&self, _held: Duration, _within: Duration) -> bool {
                std::thread::sleep(Duration::from_secs(3));
                true
            }
        }
    }
}

/// Settings written into the emulators' own config files under /storage, the way ROCKNIX's start scripts and
/// setsettings do, so ROCKNIX doesn't overwrite them.
mod defaults {
    use super::Emu;
    use std::path::Path;

    const PCSX2_INI: &str = "/storage/.config/ARMSX2/inis/PCSX2.ini";
    /// start_dolphin_gc.sh copies one of these to GCPadNew.ini on every start (it only creates them when
    /// missing); start_dolphin_wii.sh always the `south.stacked` one, for the hotkeys.
    const DOLPHIN_PROFILES: &str = "/storage/.config/dolphin-emu/GamecubeControllerProfiles";
    /// start_dolphin_wii.sh copies one of these to WiimoteNew.ini on every start; `custom.ini` is the
    /// player's own and stays as it is.
    const WII_PROFILES: &str = "/storage/.config/dolphin-emu/WiiControllerProfiles";
    const WII_STOCK_PROFILES: [&str; 4] = ["classic.ini", "hremote.ini", "nunchuck.ini", "vremote.ini"];
    /// start_azahar.sh copies ROCKNIX's config here when missing.
    const AZAHAR_STOCK: &str = "/usr/config/azahar";
    const AZAHAR_DIR: &str = "/storage/.config/azahar";

    /// Measured on the Mini V2 (docs/BIG-EMULATORS.md). Keys that ROCKNIX's start scripts rewrite on every
    /// launch go into system.cfg, where the scripts read them; the rest into the emulator's own INI.
    const PS2_SYSTEM: &[(&str, &str)] = &[("ps2.internal_resolution", "2")];
    const PS2_INI: &[(&str, &str, Option<&str>)] = &[("EmuCore/GS", "Renderer", Some("14")), ("EmuCore", "EnableWideScreenPatches", Some("false"))];
    const GC_SYSTEM: &[(&str, &str)] = &[("gamecube.graphics_backend", "vulkan"), ("gamecube.internal_resolution", "2")];
    /// Bumped when the values above change, so they are offered once more.
    const VERSION: u32 = 2;

    /// ARMSX2's INI keys that are set on every start. `OsdMessagesPos = 0` (None) drops ARMSX2's on-screen toasts:
    /// GameDB's "Current Blending Accuracy is Basic. Recommended Blending Accuracy for this game is High" is posted
    /// by every `VMManager::ApplySettings` (each PINE SetSetting, so each option pocket-ui applies) whenever the
    /// game's recommended level is above ours (2x, Basic), and ARMSX2 (commit 19c83bf) has no switch for it
    /// (`WarnAboutUnsafeSettings` is another list of warnings); the same toast logic covers the alpha-test and AA1
    /// recommendations. The FPS counter is not a toast (`OsdPerformancePos`); pocket-ui shows its own notices.
    pub(super) const ARMSX2_ENFORCED: &[(&str, &str, Option<&str>)] = &[
        ("EmuCore", "EnablePINE", Some("true")),
        ("UI", "ConfirmShutdown", Some("false")),
        ("Pad1", "Analog", None),
        ("EmuCore/GS", "VsyncEnable", Some("false")),
        ("EmuCore/GS", "OsdMessagesPos", Some("0")),
    ];

    /// What the pause menu depends on is enforced on every start; the tuned defaults (PS2 and GameCube only,
    /// Wii and 3DS are not measured) are written once per `VERSION`, so whatever the player changes
    /// afterwards (in ROCKNIX or the emulator) stays.
    pub fn apply(emu: Emu, system: &str, rom: &Path, data_dir: &Path) {
        let tuned = match (emu, system) {
            (Emu::Armsx2, _) => Some(("defaults-ps2", PS2_SYSTEM, PS2_INI)),
            (Emu::Dolphin, "gamecube") => Some(("defaults-gamecube", GC_SYSTEM, &[][..])),
            _ => None,
        };
        if let Some((name, system_kvs, ini)) = tuned {
            let marker = data_dir.join(name);
            if std::fs::read_to_string(&marker).ok().and_then(|v| v.trim().parse::<u32>().ok()) != Some(VERSION) {
                let ok = set_system_defaults(system_kvs) && (ini.is_empty() || edit_ini(PCSX2_INI, ini));
                if ok {
                    let _ = crate::fsutil::write_atomic(&marker, format!("{VERSION}\n").as_bytes());
                }
            }
        }
        match emu {
            // Everything hakuX needs is in the appendconfig `hakux::prepare` writes at every start.
            Emu::Hakux => {}
            // PINE for save/load; no "are you sure" box on quit; HOME (Guide) belongs to pocket-ui, not to the
            // DualShock's analog button. No vsync: with it, ARMSX2's GS thread waits for the compositor, which
            // shows nothing of the game while the pause menu is up, so the Quit save never finished.
            Emu::Armsx2 => {
                edit_ini(PCSX2_INI, ARMSX2_ENFORCED);
            }
            Emu::Dolphin => {
                let hotkey = if super::hotkey_is_guide(system, rom) { "Guide" } else { "Back" };
                edit_files(
                    DOLPHIN_PROFILES,
                    |name| name.starts_with("GCPadNew.ini.") && !name.ends_with("backup"),
                    |t| with_extra_triggers(t, hotkey),
                );
                if system == "wii" {
                    edit_files(WII_PROFILES, |name| WII_STOCK_PROFILES.contains(&name), with_quiet_wiimote);
                }
            }
            Emu::Azahar => azahar_config(),
        }
    }

    /// Rewrites the files of `dir` that `pick` names through `edit`, where that changes them.
    fn edit_files(dir: &str, pick: impl Fn(&str) -> bool, edit: impl Fn(&str) -> String) {
        for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if !path.file_name().and_then(|n| n.to_str()).is_some_and(&pick) {
                continue;
            }
            if let Ok(text) = std::fs::read_to_string(&path) {
                let new = edit(&text);
                if new != text {
                    write_atomic(&path.to_string_lossy(), &new);
                }
            }
        }
    }

    /// Azahar's qt-config.ini (created from ROCKNIX's like start_azahar.sh does, so it is there to edit on
    /// the first start): no "close Azahar?" question on Quit, no save-state warning box on the first load.
    fn azahar_config() {
        let ini = format!("{AZAHAR_DIR}/qt-config.ini");
        if !Path::new(&ini).exists() && std::fs::create_dir_all(AZAHAR_DIR).is_ok() {
            for entry in std::fs::read_dir(AZAHAR_STOCK).into_iter().flatten().flatten() {
                let to = Path::new(AZAHAR_DIR).join(entry.file_name());
                if entry.file_type().is_ok_and(|t| t.is_file()) && !to.exists() {
                    let _ = std::fs::copy(entry.path(), to);
                }
            }
        }
        let Ok(old) = std::fs::read_to_string(&ini) else { return };
        let new = [("confirmClose", "false"), ("saveStateWarning", "false")]
            .iter()
            .fold(old.clone(), |t, (k, v)| super::azahar::qt_set(&t, "UI", k, v));
        if new != old {
            write_atomic(&ini, &new);
        }
    }

    /// Adds each `key=value` to ROCKNIX's system.cfg unless the key is set already (the player chose it).
    /// Under ROCKNIX's lock (`system_cfg`), so a runemu.sh writing at the same moment loses nothing.
    fn set_system_defaults(kvs: &[(&str, &str)]) -> bool {
        if !Path::new(super::SYSTEM_CFG).exists() {
            return false;
        }
        crate::system_cfg::SystemCfg::at(Path::new(super::SYSTEM_CFG)).edit(|old| Some(with_system_defaults(old, kvs))).is_ok()
    }

    pub fn with_system_defaults(cfg: &str, kvs: &[(&str, &str)]) -> String {
        let mut out = cfg.to_string();
        for (key, value) in kvs {
            if !cfg.lines().any(|l| l.split_once('=').is_some_and(|(k, _)| k.trim() == *key)) {
                if !out.is_empty() && !out.ends_with('\n') {
                    out.push('\n');
                }
                out.push_str(&format!("{key}={value}\n"));
            }
        }
        out
    }

    /// What our chords press besides the stick clicks, by Dolphin's names: HOTKEY (Select `Back` or `Guide`),
    /// D-pad Up/Down for the slot steps, and the face buttons the setting hotkeys use (GC Y, X, B of the south
    /// profile: InputPlumber West, East, North, which Dolphin sees as `Button N`, `Button E`, `Button W`).
    const CHORD_INPUTS: [&str; 7] = ["`Back`", "`Guide`", "`Pad N`", "`Pad S`", "`Button N`", "`Button E`", "`Button W`"];
    const WIIMOTE_GUARD: &str = " & !`Thumb L` & !`Thumb R`";

    /// In Wii games the pad drives the emulated Wiimote too, where Select is "-", Guide is HOME, the D-pad is
    /// the D-pad and the face buttons are A, B, 1, 2 (or a Classic Controller's): our chords would press them.
    /// The stock profiles bind neither stick click, and every chord holds one from its first press to its last
    /// release, so these bindings are made to ignore the pad while a stick click is held.
    pub fn with_quiet_wiimote(profile: &str) -> String {
        let lines: Vec<String> = profile
            .lines()
            .map(|line| match line.split_once('=') {
                Some((key, value)) if CHORD_INPUTS.iter().any(|i| value.contains(i)) && !value.trim_end().ends_with(WIIMOTE_GUARD) => {
                    format!("{} = ({}){WIIMOTE_GUARD}", key.trim(), value.trim())
                }
                _ => line.to_string(),
            })
            .collect();
        lines.join("\n") + "\n"
    }

    /// ROCKNIX's Dolphin saves on HOTKEY+R and loads on HOTKEY+L, but L and R are the analog triggers, which
    /// InputPlumber can't press. So a controller profile's L and R also answer to HOTKEY + one stick click
    /// (`hotkey` is Dolphin's name for the hotkey button, "Back" or "Guide"); with both clicks held (the slot
    /// steps) they stay off. GC games have no use for the stick clicks.
    pub fn with_extra_triggers(profile: &str, hotkey: &str) -> String {
        let lines: Vec<String> = profile
            .lines()
            .map(|line| {
                let (key, stick, other) = match line.split('=').next().map(str::trim) {
                    Some("Triggers/L") => ("Triggers/L", "Thumb L", "Thumb R"),
                    Some("Triggers/R") => ("Triggers/R", "Thumb R", "Thumb L"),
                    _ => return line.to_string(),
                };
                let bound = line.split_once('=').map_or("", |(_, v)| v.trim());
                // What the profile itself binds, without an extra binding written earlier.
                let own = bound.find(" | (`").map_or(bound, |at| &bound[..at]).trim();
                let extra = format!("(`{hotkey}` & `{stick}` & !`{other}`)");
                if own.is_empty() || own.starts_with("(`") {
                    format!("{key} = {extra}")
                } else {
                    format!("{key} = {own} | {extra}")
                }
            })
            .collect();
        lines.join("\n") + "\n"
    }

    /// Sets (or with `None` removes) `key` in `[section]`; adds the key, or the section, when missing.
    pub fn ini_set(text: &str, section: &str, key: &str, value: Option<&str>) -> String {
        let mut out: Vec<String> = Vec::new();
        let (mut in_section, mut done, mut seen_section) = (false, false, false);
        let header = format!("[{section}]");
        for line in text.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with('[') {
                if in_section && !done {
                    if let Some(v) = value {
                        // Before the blank lines that end the section.
                        let at = out.iter().rposition(|l| !l.trim().is_empty()).map_or(out.len(), |i| i + 1);
                        out.insert(at, format!("{key} = {v}"));
                    }
                    done = true;
                }
                in_section = trimmed == header;
                seen_section |= in_section;
            } else if in_section && !done && trimmed.split('=').next().map(str::trim) == Some(key) && trimmed.contains('=') {
                if let Some(v) = value {
                    out.push(format!("{key} = {v}"));
                }
                done = true;
                continue;
            }
            out.push(line.to_string());
        }
        if !done {
            if let Some(v) = value {
                if !seen_section {
                    out.push(String::new());
                    out.push(header);
                }
                out.push(format!("{key} = {v}"));
            }
        }
        let mut s = out.join("\n");
        s.push('\n');
        s
    }

    /// Applies `edits` to an INI file, writing it (through a temporary file) only when something changed.
    pub fn edit_ini(path: &str, edits: &[(&str, &str, Option<&str>)]) -> bool {
        let Ok(old) = std::fs::read_to_string(path) else { return false };
        let new = edits.iter().fold(old.clone(), |t, (s, k, v)| ini_set(&t, s, k, *v));
        new == old || write_atomic(path, &new)
    }

    fn write_atomic(path: &str, text: &str) -> bool {
        crate::fsutil::write_atomic(Path::new(path), text.as_bytes()).is_ok()
    }
}

// What the pause menu's emulator settings (`emu_settings.rs`) use of the running game.

/// The running standalone game as `emu_settings` sees it.
pub struct Running {
    pub emu: Emu,
    pub system: String,
    pub rom: PathBuf,
    /// ARMSX2, once the game runs: the disc name "<serial> (<CRC>)".
    pub disc: Option<String>,
}

pub fn session() -> Option<Running> {
    current().map(|s| Running { emu: s.emu, disc: s.base.filter(|_| s.emu == Emu::Armsx2), system: s.system, rom: s.rom })
}

/// A Wii Remote profile as pocket-ui rewrites the stock ones (`defaults::with_quiet_wiimote`).
pub fn quiet_wiimote(profile: &str) -> String {
    defaults::with_quiet_wiimote(profile)
}

/// One of ARMSX2, Dolphin nogui or Azahar is running (started by us or not).
pub fn emulator_running() -> bool {
    [Emu::Armsx2, Emu::Dolphin, Emu::Azahar, Emu::Hakux].iter().any(|e| find_pid(None, e.process()).is_some())
        || [guest::Guest::Cemu, guest::Guest::Rpcs3, guest::Guest::Vita3k].iter().any(|g| find_pid(None, g.process()).is_some())
}

/// A Wii U (Cemu), PS3 (RPCS3) or PS Vita (Vita3K) game is running as a guest (`guest`): Continue and Quit, no states.
pub fn guest_active() -> bool {
    guest::current().is_some()
}

/// Quit from the pause menu for a guest (the game has just been resumed): the emulator's window is closed as its
/// own close button would, then SIGTERM, then SIGKILL (`guest::PLAN`).
pub fn guest_quit(pgid: Option<i32>) {
    guest::quit(pgid);
}

/// Sets `key=value` in `[section]` of Azahar's qt-config.ini, with `key\default=false` so Azahar takes it.
pub fn azahar_qt_set(text: &str, section: &str, key: &str, value: &str) -> String {
    azahar::qt_set(text, section, key, value)
}

/// One PINE request to ARMSX2 (see `mod pine`).
pub fn pine_call(op: u8, args: &[u8]) -> Option<Vec<u8>> {
    pine::call(op, args)
}

/// ARMSX2's VM over PINE: 0 running, 1 paused, 2 no game; None when it doesn't answer.
pub fn pine_status() -> Option<u32> {
    pine::status()
}

/// Presses `buttons` together on InputPlumber's virtual pad (InputPlumber names, e.g. "Gamepad:Button:North").
pub fn press_chord(buttons: &[&str]) -> bool {
    match current() {
        Some(s) => turn(&s).is_some_and(|_turn| chord(buttons)),
        None => chord(buttons),
    }
}

/// One of ROCKNIX's Dolphin setting hotkeys: HOTKEY + `button`, stick clicks held first (`dolphin::toggle`).
pub fn dolphin_toggle(button: &'static str) -> bool {
    let Some(s) = current() else { return false };
    // Not between the slot steps of a Save or Load (Dolphin counts the hotkeys globally).
    let Some(_turn) = turn(&s) else { return false };
    chord(&dolphin::toggle(button, dolphin::hotkey_name(hotkey_is_guide(&s.system, &s.rom))))
}

/// Applying settings that need a new start: writes the resume state as Quit does, and only once it is
/// complete closes the emulator (the main loop then starts the game again, which loads the state).
/// `done(false)`: nothing was written and the emulator keeps running.
pub fn save_and_close(pgid: Option<i32>, frame: Option<Image>, done: impl FnOnce(bool) + Send + 'static) {
    let Some(s) = current().filter(|s| quit_plan(s.emu).first() == Some(&Step::SaveResume)) else { return done(false) };
    std::thread::spawn(move || {
        let Some(_turn) = turn(&s) else { return done(false) };
        let saved = save_target(&s, Target::Auto);
        if saved {
            write_preview(&s, Target::Auto, frame);
        }
        done(saved);
        if saved {
            close(s.emu, pgid);
        }
    });
}

/// Wii U (Cemu), PS3 (RPCS3) and PS Vita (Vita3K) on the SM8550 handhelds: guests of the pause menu without states. Neither emulator
/// has a save state we could press or ask for (Cemu has none at all; RPCS3 has no savestates, and no IPC), so the
/// menu offers Continue and Quit and says honestly that the game keeps its own saves (`pause_saves::OWN_SAVES`).
/// pocket-ui starts them through ROCKNIX's `runemu.sh` as ever; what it adds is the CPU/GPU profile of the other big
/// emulators (Games: performance, Quick settings) and a Quit that closes the emulator's window and escalates, with
/// the process found by name inside the game's process group. Code paths in `main.rs`: `Quit if guest_active()`.
mod guest {
    use super::*;

    #[derive(Clone, Copy, PartialEq, Debug)]
    pub enum Guest {
        Cemu,
        Rpcs3,
        /// A PortMaster port: a launch script with children (gptokeyb, the engine, box86), found by its group, not a name.
        Port,
        Vita3k,
    }

    impl Guest {
        pub fn detect(system: &str, emulator: &str) -> Option<Guest> {
            match (system, emulator) {
                ("wiiu", "cemu") => Some(Guest::Cemu),
                ("ps3", "rpcs3") => Some(Guest::Rpcs3),
                ("ports", _) => Some(Guest::Port),
                ("psvita", "vita3k") => Some(Guest::Vita3k),
                _ => None,
            }
        }

        /// Start of the emulator's process name (`comm`; `/usr/bin/cemu`, `/usr/bin/rpcs3-sa`, `/usr/bin/vita3k-sa`).
        pub fn process(self) -> &'static str {
            match self {
                Guest::Cemu => "cemu",
                Guest::Rpcs3 => "rpcs3-sa",
                // No process of one name: `quit_port` works on the whole group.
                Guest::Port => "",
                Guest::Vita3k => "vita3k-sa",
            }
        }
    }

    /// How long each step of a guest's Quit waits before it moves on.
    const CLOSE_WAIT: Duration = Duration::from_secs(8);
    const TERM_WAIT: Duration = Duration::from_secs(5);
    /// The longest a guest's Quit takes when every step runs out its time (the main loop stops it after this plus slack).
    pub(super) const WORST: Duration = CLOSE_WAIT.saturating_add(TERM_WAIT);
    /// The same for all: the window's close request (Cemu's File > Exit, RPCS3 with its exit question off, Vita3K's SDL
    /// quit event) flushes the shader caches and settings; SIGTERM and SIGKILL only if it hangs.
    pub(super) const PLAN: &[Step] = &[
        Step::CloseWindow,
        Step::Exit(CLOSE_WAIT),
        Step::Signal(libc::SIGTERM),
        Step::Exit(TERM_WAIT),
        Step::Signal(libc::SIGKILL),
        Step::KillSame,
    ];

    static GUEST: Mutex<Option<(u64, Guest)>> = Mutex::new(None);

    pub fn current() -> Option<(u64, Guest)> {
        *GUEST.lock().ok()?
    }

    /// A game of a guest emulator starts: noted, and its performance profile set once the emulator's process runs.
    pub fn launch(system: &str, emulator: &str, rom: &Path) {
        let Some(g) = Guest::detect(system, emulator) else { return };
        let id = LAUNCHES.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
        if let Ok(mut cur) = GUEST.lock() {
            *cur = Some((id, g));
        }
        // A port's performance and fan are applied by `port_perf` (its own choice per port), not here.
        if g == Guest::Port {
            return;
        }
        let (system, rom) = (system.to_string(), rom.to_path_buf());
        std::thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(120);
            while find_pid(None, g.process()).is_none() {
                if Instant::now() > deadline || current().is_none_or(|(i, _)| i != id) {
                    return;
                }
                std::thread::sleep(Duration::from_millis(250));
            }
            // runemu.sh has set ROCKNIX's governor before it started the emulator: ours goes on top of it.
            if current().is_some_and(|(i, _)| i == id) {
                set_performance(chosen_performance(&system, &rom));
            }
        });
    }

    pub fn ended() {
        if let Ok(mut cur) = GUEST.lock() {
            *cur = None;
        }
    }

    /// Runs [`PLAN`] on the emulator's process, on a thread (the caller is the UI).
    pub fn quit(pgid: Option<i32>) {
        let Some((_, g)) = current() else { return };
        if g == Guest::Port {
            std::thread::spawn(move || quit_port(pgid));
            return;
        }
        std::thread::spawn(move || {
            let started = Instant::now();
            println!("standalone: quitting {g:?}, at most {WORST:?}");
            // A window hidden behind our menu may ignore the close request (the Xbox guest's lesson): in front first.
            crate::control::show_game();
            run_plan(PLAN, g.process(), pgid);
            println!("standalone: {g:?} closed after {:?}", started.elapsed());
        });
    }
}

/// Waits per step of a port's Quit. Most engines end on SIGTERM at once; the window's close request comes first for the
/// ones that write their save on it (SDL's quit event).
const PORT_CLOSE_WAIT: Duration = Duration::from_millis(2500);
const PORT_TERM_WAIT: Duration = Duration::from_millis(2500);
/// The longest a port's Quit takes (the main loop's backstop, `GUEST_GRACE`, comes after it).
pub(crate) const PORT_WORST: Duration = PORT_CLOSE_WAIT.saturating_add(PORT_TERM_WAIT).saturating_add(Duration::from_secs(1));
/// What a port's launch script starts next to the game and may leave behind when it is killed from outside: the
/// button-to-key translator and the helpers that wait for the engine.
const PORT_HELPERS: &[&str] = &["gptokeyb", "oga_controls", "sdl2imgui"];

/// Members of the process group `pgid` that still run (zombies are gone for our purposes).
fn group_members(pgid: i32) -> Vec<i32> {
    let Ok(dir) = std::fs::read_dir("/proc") else { return Vec::new() };
    dir.flatten()
        .filter_map(|e| {
            let pid: i32 = e.file_name().to_str()?.parse().ok()?;
            let stat = std::fs::read_to_string(e.path().join("stat")).ok()?;
            let rest = stat.get(stat.rfind(')')? + 2..)?;
            let mut f = rest.split(' ');
            let (state, _ppid, group) = (f.next()?, f.next()?, f.next()?.parse::<i32>().ok()?);
            (group == pgid && state != "Z").then_some(pid)
        })
        .collect()
}

/// Quit for a PortMaster port: the window is brought in front and asked to close (SDL ports save on it), then SIGTERM
/// to the whole group, then SIGKILL to the group and to the helpers that left it. Done when no process of the group is
/// left, in a few seconds at most.
fn quit_port(pgid: Option<i32>) {
    let started = Instant::now();
    println!("standalone: quitting a port, at most {PORT_WORST:?}");
    let Some(g) = pgid.filter(|g| *g > 1) else { return };
    let gone = |within: Duration| {
        let end = Instant::now() + within;
        while Instant::now() < end && !group_members(g).is_empty() {
            std::thread::sleep(Duration::from_millis(100));
        }
        group_members(g).is_empty()
    };
    // A window hidden behind our menu may ignore the close request: in front first.
    crate::control::show_game();
    let members = group_members(g);
    if !members.is_empty() {
        azahar::close_windows(&members);
    }
    if !gone(PORT_CLOSE_WAIT) {
        // SAFETY: kill(2) on the process group pocket-ui started for this port.
        unsafe {
            libc::kill(-g, libc::SIGTERM);
        }
        if !gone(PORT_TERM_WAIT) {
            // SAFETY: as above.
            unsafe {
                libc::kill(-g, libc::SIGKILL);
            }
        }
    }
    // gptokeyb and friends may have been started in a group of their own (sudo, setsid): by name.
    for name in PORT_HELPERS {
        kill_same(name, None);
    }
    println!("standalone: port closed after {:?}", started.elapsed());
}

#[cfg(test)]
mod tests {
    use super::defaults::ini_set;
    use super::*;
    use crate::testutil::TestDir;

    const INI: &str =
        "[UI]\nConfirmShutdown = true\n\n\n[EmuCore]\nEnablePINE = false\nPINESlot = 28011\n\n[Pad1]\nCross = SDL-0/FaceSouth\nAnalog = SDL-0/Guide\n";

    fn session(id: u64) -> Session {
        Session {
            emu: Emu::Dolphin,
            system: "gamecube".into(),
            rom: PathBuf::from("/storage/roms/gamecube/Game.iso"),
            base: Some("Game.iso".into()),
            id,
        }
    }

    #[test]
    fn actions_take_turns_and_a_finished_games_worker_does_nothing() {
        // Review N4 / SOL-03: Save, Load and Quit each ran on a thread of their own at once (Dolphin's slot steps of
        // two saves landed both in slot 2), and a slow worker of a game that had ended acted on the next one.
        let a = session(1_000_001);
        *SESSION.lock().unwrap() = Some(a.clone());
        let first = turn(&a).expect("the running game's turn");
        assert!(bound_game_runs());
        let (tx, rx) = std::sync::mpsc::channel();
        let second = {
            let a = a.clone();
            std::thread::spawn(move || {
                let t = turn(&a);
                tx.send(t.is_some()).unwrap();
            })
        };
        assert!(rx.recv_timeout(Duration::from_millis(200)).is_err(), "the second action waits for the first");
        drop(first);
        assert_eq!(rx.recv_timeout(Duration::from_secs(5)), Ok(true));
        second.join().unwrap();
        assert_eq!(BOUND.get(), None, "a turn given back unbinds the thread");

        // A worker of game A still holding its turn when A ends and B starts in the same emulator: its next chord or
        // PINE request is refused.
        let t = turn(&a).unwrap();
        *SESSION.lock().unwrap() = Some(session(1_000_002));
        assert!(!bound_game_runs());
        assert!(!chord(&["Gamepad:Button:South"]));
        assert_eq!(pine::call(pine::STATUS, &[]), None);
        drop(t);
        // And a worker of A that only now gets its turn does nothing at all.
        assert!(turn(&a).is_none());
        assert_eq!(BOUND.get(), None);
        *SESSION.lock().unwrap() = None;
    }

    #[test]
    fn quit_grace_outlasts_the_slowest_armsx2_quit() {
        // The main loop stops the game at QUIT_GRACE; before ARMSX2 has saved and closed, that loses the state.
        assert!(
            QUIT_GRACE >= ARMSX2_QUIT_WORST + Duration::from_secs(2),
            "QUIT_GRACE {QUIT_GRACE:?}, ARMSX2 may need {ARMSX2_QUIT_WORST:?}"
        );
    }

    #[test]
    fn ini_replaces_in_the_right_section() {
        let t = ini_set(INI, "EmuCore", "EnablePINE", Some("true"));
        assert!(t.contains("[EmuCore]\nEnablePINE = true\nPINESlot = 28011"));
        assert!(t.contains("ConfirmShutdown = true"));
    }

    #[test]
    fn armsx2_shows_no_toasts_over_the_game() {
        // GameDB's "Current Blending Accuracy is Basic. Recommended ... High" is posted on every ApplySettings (so on
        // every PINE SetSetting) and has no switch of its own; `OsdMessagesPos = None` drops all of ARMSX2's toasts.
        let t = defaults::ARMSX2_ENFORCED.iter().fold(INI.to_string(), |t, (s, k, v)| ini_set(&t, s, k, *v));
        assert!(t.contains("[EmuCore/GS]\nOsdMessagesPos = 0\n") || t.contains("\nOsdMessagesPos = 0\n"), "{t}");
        assert!(t.contains("EnablePINE = true"), "the PINE switch is still there");
    }

    #[test]
    fn wii_u_ps3_and_vita_are_guests_that_close_by_their_window() {
        use super::guest::{Guest, PLAN, WORST};
        assert_eq!(Guest::detect("wiiu", "cemu"), Some(Guest::Cemu));
        assert_eq!(Guest::detect("ps3", "rpcs3"), Some(Guest::Rpcs3));
        assert_eq!(Guest::detect("psvita", "vita3k"), Some(Guest::Vita3k));
        assert_eq!(Guest::Vita3k.process(), "vita3k-sa");
        assert_eq!(Guest::detect("psvita", "retroarch"), None);
        assert_eq!(Guest::detect("ps3", "retroarch"), None);
        assert_eq!(Guest::detect("wii", "dolphin"), None);
        assert_eq!(PLAN.first(), Some(&Step::CloseWindow), "the emulator's own close first");
        assert_eq!(PLAN.last(), Some(&Step::KillSame), "RPCS3's error-dialog process too");
        assert!(PLAN.iter().all(|s| *s != Step::SaveResume), "no states to write");
        assert!(WORST < GUEST_GRACE, "the main loop's backstop comes after the plan");
    }

    #[test]
    fn ports_are_guests_found_by_their_group() {
        assert_eq!(guest::Guest::detect("ports", "sh"), Some(guest::Guest::Port));
        assert_eq!(guest::Guest::detect("ports", "anything"), Some(guest::Guest::Port));
        assert!(PORT_WORST < GUEST_GRACE, "the main loop's backstop comes after the port's plan");
    }

    #[test]
    fn quit_ends_the_whole_port_a_script_and_the_children_it_left_behind() {
        use std::os::unix::process::CommandExt;
        // A launch script with a background child that ignores SIGTERM (an engine, gptokeyb) and a second one.
        let mut script = std::process::Command::new("sh")
            .args(["-c", "trap '' TERM; sleep 60 & sleep 60 & wait"])
            .process_group(0)
            .spawn()
            .unwrap();
        let g = script.id() as i32;
        let waiter = std::thread::spawn(move || script.wait());
        let end = Instant::now() + Duration::from_secs(5);
        while group_members(g).len() < 3 && Instant::now() < end {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(group_members(g).len() >= 3, "script and two children are in the group");
        let started = Instant::now();
        quit_port(Some(g));
        assert!(group_members(g).is_empty(), "nothing of the port is left: {:?}", group_members(g));
        assert!(started.elapsed() < PORT_WORST + Duration::from_secs(2), "took {:?}", started.elapsed());
        let _ = waiter.join();
    }

    #[test]
    fn a_port_that_is_gone_is_quit_at_once() {
        let started = Instant::now();
        quit_port(None);
        quit_port(Some(1));
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn ini_adds_missing_keys_and_sections() {
        let t = ini_set(INI, "UI", "StartPaused", Some("false"));
        assert!(t.contains("[UI]\nConfirmShutdown = true\nStartPaused = false\n\n\n[EmuCore]"));
        let t = ini_set(INI, "Hotkeys", "X", Some("y"));
        assert!(t.ends_with("[Hotkeys]\nX = y\n"));
    }

    #[test]
    fn ini_removes_keys() {
        let t = ini_set(INI, "Pad1", "Analog", None);
        assert!(!t.contains("Analog"));
        assert!(t.contains("Cross = SDL-0/FaceSouth"));
        assert_eq!(ini_set(&t, "Pad1", "Analog", None), t);
    }

    #[test]
    fn system_defaults_never_override_the_player() {
        let cfg = "global.autosave=1\nps2.internal_resolution=3";
        let t = defaults::with_system_defaults(cfg, &[("ps2.internal_resolution", "2"), ("gamecube.vsync", "1")]);
        assert_eq!(t, "global.autosave=1\nps2.internal_resolution=3\ngamecube.vsync=1\n");
        assert_eq!(defaults::with_system_defaults(&t, &[("gamecube.vsync", "0")]), t);
    }

    #[test]
    fn dolphin_profiles_get_a_second_l_and_r() {
        let p = "[GCPad1]\nButtons/Z = `Shoulder R`\nTriggers/L = `Trigger L`\nTriggers/R = `Trigger R`\nTriggers/L-Analog = `Trigger L`\n";
        let t = defaults::with_extra_triggers(p, "Back");
        // One stick click: L or R. Both held (the slot steps) must be neither.
        assert!(t.contains("Triggers/L = `Trigger L` | (`Back` & `Thumb L` & !`Thumb R`)\n"), "{t}");
        assert!(t.contains("Triggers/R = `Trigger R` | (`Back` & `Thumb R` & !`Thumb L`)\n"), "{t}");
        assert!(t.contains("Triggers/L-Analog = `Trigger L`\n"));
        assert_eq!(defaults::with_extra_triggers(&t, "Back"), t);
        let g = defaults::with_extra_triggers(&t, "Guide");
        assert!(g.contains("Triggers/R = `Trigger R` | (`Guide` & `Thumb R` & !`Thumb L`)\n"));
        // Bindings written by earlier versions are replaced, not stacked.
        for old in [
            "Triggers/R = `Trigger R` | (`Misc 1` & `Thumb R`)\n",
            "Triggers/R = `Trigger R` | (`Back` & `Thumb R`)\n",
        ] {
            assert_eq!(
                defaults::with_extra_triggers(old, "Back"),
                "Triggers/R = `Trigger R` | (`Back` & `Thumb R` & !`Thumb L`)\n"
            );
        }
    }

    #[test]
    fn the_emulated_wiimote_does_not_see_our_chords() {
        // Every chord holds a stick click first; the stock Wii profiles bind neither, so while one is held
        // the Wiimote ignores Select (-), HOME and the D-pad our chords press.
        let p = "[Wiimote1]\nDevice = SDL/0/x\nButtons/A = `Button N`\nButtons/2 = `Button S`\nButtons/- = `Back`\nButtons/Home = `Guide`\nD-Pad/Up = `Pad W` | `Left X-`\nD-Pad/Right = `Pad N` | `Left Y+`\nClassic/Buttons/- = `Back`\nClassic/D-Pad/Down = `Pad S`\nRumble/Motor = Motor\n";
        let t = defaults::with_quiet_wiimote(p);
        let guard = " & !`Thumb L` & !`Thumb R`";
        assert!(t.contains(&format!("Buttons/- = (`Back`){guard}\n")), "{t}");
        assert!(t.contains(&format!("Buttons/Home = (`Guide`){guard}\n")));
        assert!(t.contains(&format!("D-Pad/Right = (`Pad N` | `Left Y+`){guard}\n")));
        assert!(t.contains(&format!("Classic/Buttons/- = (`Back`){guard}\n")));
        assert!(t.contains(&format!("Classic/D-Pad/Down = (`Pad S`){guard}\n")));
        // The face buttons of the setting hotkeys too (GC Y on the south profile is `Button N`).
        assert!(t.contains(&format!("Buttons/A = (`Button N`){guard}\n")), "{t}");
        // What our chords never press stays as it was.
        assert!(t.contains("Buttons/2 = `Button S`\n") && t.contains("D-Pad/Up = `Pad W` | `Left X-`\n") && t.contains("Device = SDL/0/x\n"));
        assert_eq!(defaults::with_quiet_wiimote(&t), t);
    }

    #[test]
    fn dolphin_chords_hold_a_stick_click_before_the_hotkey() {
        use dolphin::Action::*;
        let hk = dolphin::hotkey_name(false);
        assert_eq!(hk, "Gamepad:Button:Select");
        assert_eq!(dolphin::hotkey_name(true), "Gamepad:Button:Guide");
        let (l3, r3) = ("Gamepad:Button:LeftStick", "Gamepad:Button:RightStick");
        let (up, down) = ("Gamepad:Button:DPadUp", "Gamepad:Button:DPadDown");
        // Slot 0: one chord, R3 then HOTKEY (SendButtonChord presses in order and releases in reverse).
        assert_eq!(dolphin::chords(0, Save, hk), vec![vec![r3, hk]]);
        assert_eq!(dolphin::chords(0, Load, hk), vec![vec![l3, hk]]);
        // Slot 1: step up with both sticks held (neither L nor R), act, step back down.
        let step = |d| vec![l3, r3, hk, d];
        assert_eq!(dolphin::chords(1, Save, hk), vec![step(up), vec![r3, hk], step(down)]);
        assert_eq!(dolphin::chords(2, Load, hk), vec![step(up), step(up), vec![l3, hk], step(down), step(down)]);
    }

    #[test]
    fn hotkey_setting_is_read_like_rocknix_get_setting() {
        let cfg = "global.hotkey_enable_button=select\nwii.hotkey_enable_button=mode\nwii[\"Foo.wbfs\"].hotkey_enable_button=select\n";
        assert_eq!(rocknix_setting(cfg, "hotkey_enable_button", "wii", "Bar.iso"), Some("mode"));
        assert_eq!(rocknix_setting(cfg, "hotkey_enable_button", "wii", "Foo.wbfs"), Some("select"));
        assert_eq!(rocknix_setting(cfg, "hotkey_enable_button", "gamecube", "Bar.iso"), Some("select"));
        assert_eq!(
            rocknix_setting("gamecube.hotkey_enable_button=\n", "hotkey_enable_button", "gamecube", "x"),
            None
        );
        assert_eq!(rocknix_setting("", "hotkey_enable_button", "wii", "x"), None);
    }

    #[test]
    fn wii_and_3ds_are_first_class_too() {
        assert_eq!(Emu::detect("wii", "dolphin", "dolphin-sa-wii"), Some(Emu::Dolphin));
        assert_eq!(Emu::detect("wii", "dolphin", "dolphin-qt-wii"), None);
        assert_eq!(Emu::detect("3ds", "azahar", "azahar-sa"), Some(Emu::Azahar));
        // Experimental: no pause-menu Save/Load.
        assert_eq!(Emu::detect("wiiware", "dolphin", "dolphin-sa-wii"), None);
        assert_eq!(Emu::detect("wiiu", "cemu", "cemu-sa"), None, "Cemu and RPCS3 are guests without states (`guest`)");
        assert_eq!(Emu::detect("ps3", "rpcs3", "rpcs3-sa"), None);
        assert_eq!(Emu::detect("psvita", "vita3k", "vita3k-sa"), None);
        assert_eq!(Emu::detect("xbox", "xemu", "xemu-sa"), None);
        assert_eq!(Emu::detect("xbox", "hakux", "hakux"), Some(Emu::Hakux));
        assert_eq!(Emu::detect("ps2", "hakux", "hakux"), None, "only Xbox games");
        // Games whose state file name can't be told from the file stay without, too.
        assert!(Emu::Dolphin.handles(Path::new("/storage/roms/wii/Game.wbfs")));
        assert!(!Emu::Dolphin.handles(Path::new("/storage/roms/wii/Channel.wad")));
        assert!(Emu::Azahar.handles(Path::new("/storage/roms/3ds/BlockamokRemix.3dsx")));
        assert!(!Emu::Azahar.handles(Path::new("/storage/roms/3ds/Game.cia")));
        assert!(!Emu::Azahar.handles(Path::new("/storage/roms/3ds/Game.zcci")));
    }

    #[test]
    fn dolphin_state_names_follow_the_boot_type() {
        assert_eq!(
            state_base(Emu::Dolphin, Path::new("/storage/roms/wii/Game (EU).wbfs")).as_deref(),
            Some("Game (EU).wbfs")
        );
        // DOL and ELF homebrew: Dolphin's "ID-<name>" game id.
        assert_eq!(state_base(Emu::Dolphin, Path::new("/storage/roms/wii/boot.dol")).as_deref(), Some("ID-boot"));
        assert_eq!(state_base(Emu::Dolphin, Path::new("/storage/roms/wii/Channel.wad")), None);
        let wii = Session {
            emu: Emu::Dolphin,
            system: "wii".into(),
            rom: "/storage/roms/wii/boot.dol".into(),
            base: Some("ID-boot".into()),
            id: 1,
        };
        assert_eq!(wii.state_path(Target::Slot(1)).unwrap(), PathBuf::from("/storage/roms/savestates/wii/ID-boot.s01"));
    }

    fn scratch(tag: &str) -> crate::testutil::TestDir {
        TestDir::new(&format!("standalone-{tag}"))
    }

    #[test]
    fn azahar_states_are_named_after_the_title_id() {
        let dir = scratch("tid");
        // NCCH (.cxi): magic at 0x100, program id at 0x118.
        let mut cxi = vec![0u8; 0x200];
        cxi[0x100..0x104].copy_from_slice(b"NCCH");
        cxi[0x118..0x120].copy_from_slice(&0x0004_0000_0012_3400u64.to_le_bytes());
        std::fs::write(dir.join("a.cxi"), &cxi).unwrap();
        assert_eq!(azahar::title_id(&dir.join("a.cxi")), Some(0x0004_0000_0012_3400));
        // NCSD (.3ds): partition 0 at the offset in 0x200 units at 0x120, its NCCH header there.
        let mut cci = vec![0u8; 0x400];
        cci[0x100..0x104].copy_from_slice(b"NCSD");
        cci[0x120..0x124].copy_from_slice(&1u32.to_le_bytes());
        cci[0x200..0x400].copy_from_slice(&cxi);
        std::fs::write(dir.join("b.3ds"), &cci).unwrap();
        assert_eq!(azahar::title_id(&dir.join("b.3ds")), Some(0x0004_0000_0012_3400));
        std::fs::write(dir.join("bad.3ds"), [0u8; 0x400]).unwrap();
        assert_eq!(azahar::title_id(&dir.join("bad.3ds")), None);
        // 3DSX and ELF have no program id: Azahar uses 0.
        assert_eq!(azahar::title_id(Path::new("/nonexistent/x.3dsx")), Some(0));
        assert_eq!(state_base(Emu::Azahar, &dir.join("b.3ds")).as_deref(), Some("0004000000123400"));
        let s = Session {
            emu: Emu::Azahar,
            system: "3ds".into(),
            rom: "/storage/roms/3ds/Blockamok.3dsx".into(),
            base: Some("0000000000000000".into()),
            id: 1,
        };
        // Save/Load: Azahar's quick slot (HOME+R1/L1); Quit/resume and "Before load": files of our own, named after the ROM.
        assert_eq!(s.state_path(Target::Slot(1)).unwrap(), PathBuf::from("/storage/roms/savestates/3ds/0000000000000000.00.cst"));
        assert_eq!(s.state_path(Target::Auto).unwrap(), PathBuf::from("/storage/roms/savestates/3ds/Blockamok.3dsx.auto.cst"));
        assert_eq!(s.state_path(Target::Undo).unwrap(), PathBuf::from("/storage/roms/savestates/3ds/Blockamok.3dsx.undo.cst"));
        assert_eq!(s.state_path(Target::Slot(2)), None, "one slot");
    }

    #[test]
    fn azahar_is_driven_by_its_own_keyboard_shortcuts() {
        // qt-config.ini: Quick Save = Alt+S, Quick Load = Alt+L (what ROCKNIX's HOME+R1/L1 send via gptokeyb).
        assert_eq!(azahar::SAVE, &["Keyboard:KeyLeftAlt", "Keyboard:KeyS"]);
        assert_eq!(azahar::LOAD, &["Keyboard:KeyLeftAlt", "Keyboard:KeyL"]);
        // Azahar reads `key\default=true` as "use the built-in default", so both lines are written.
        let ini = "[UI]\nconfirmClose=true\nconfirmClose\\default=true\nfullscreen=true\n[Utility]\nx=1\n";
        let t = azahar::qt_set(ini, "UI", "confirmClose", "false");
        assert_eq!(t, "[UI]\nconfirmClose=false\nconfirmClose\\default=false\nfullscreen=true\n[Utility]\nx=1\n");
        assert_eq!(azahar::qt_set(&t, "UI", "confirmClose", "false"), t);
        let t = azahar::qt_set(ini, "UI", "saveStateWarning", "false");
        assert!(
            t.starts_with(
                "[UI]\nconfirmClose=true\nconfirmClose\\default=true\nfullscreen=true\nsaveStateWarning=false\nsaveStateWarning\\default=false\n[Utility]"
            ),
            "{t}"
        );
    }

    #[test]
    fn azahar_game_window_is_the_one_with_a_title() {
        let idle = r#"{"nodes":[{"name":"pocket-ui","app_id":"pocket-ui","nodes":[]},{"name":"Azahar 2126.2","app_id":"org.azahar_emu.Azahar","nodes":[]}]}"#;
        let game = r#"{"nodes":[{"nodes":[{"name":"Azahar 2126.2 | Blockamok Remix","app_id":"org.azahar_emu.Azahar","nodes":[]}]}]}"#;
        assert!(!azahar::game_window(idle));
        assert!(azahar::game_window(game));
        assert!(!azahar::game_window("not json"));
    }

    #[test]
    fn dolphins_parked_slot_is_recovered_at_the_next_start_and_never_parked_over() {
        // SOL-04: `.s00` (a player's own state, A) parked as `.s00.pocket-ui` while our auto state B passes through;
        // pocket-ui ends in between. Only Azahar's quick slot was recovered at the next start, so the next transit
        // parked B over A's backup.
        let dir = scratch("dolphin-transit");
        let (transit, auto, undo) = (dir.join("Game.iso.s00"), dir.join("Game.iso.pocket-auto"), dir.join("Game.iso.pocket-undo"));
        std::fs::write(&transit, "PLAYER-A").unwrap();
        let backup = azahar::park(&transit).unwrap().expect("parked");
        std::fs::write(&transit, "AUTO-B").unwrap(); // Dolphin's save into the transit slot, then pocket-ui ends
        // A second park while that backup is there refuses rather than replacing it.
        assert!(azahar::park(&transit).is_err());
        assert_eq!(std::fs::read_to_string(&backup).unwrap(), "PLAYER-A");
        // The next start puts it back, keeping what was saved meanwhile as the auto state.
        let wii = Session {
            emu: Emu::Dolphin,
            system: "wii".into(),
            rom: "/storage/roms/wii/Game.iso".into(),
            base: Some("Game.iso".into()),
            id: 1,
        };
        assert_eq!(
            transit_of(&wii),
            Some((
                PathBuf::from("/storage/roms/savestates/wii/Game.iso.s00"),
                PathBuf::from("/storage/roms/savestates/wii/Game.iso.pocket-auto"),
                PathBuf::from("/storage/roms/savestates/wii/Game.iso.pocket-undo")
            ))
        );
        azahar::recover(&transit, &auto, &undo);
        assert_eq!(std::fs::read_to_string(&transit).unwrap(), "PLAYER-A");
        assert_eq!(std::fs::read_to_string(&auto).unwrap(), "AUTO-B");
        assert!(!backup.exists());
        // Interrupted while loading "Before load" (a hard link of it in the transit slot): the link goes, the file stays.
        std::fs::write(&undo, "UNDO").unwrap();
        let backup = azahar::park(&transit).unwrap();
        std::fs::hard_link(&undo, &transit).unwrap();
        assert!(backup.is_some());
        azahar::recover(&transit, &auto, &undo);
        assert_eq!(std::fs::read_to_string(&transit).unwrap(), "PLAYER-A");
        assert_eq!(std::fs::read_to_string(&undo).unwrap(), "UNDO");
        assert_eq!(std::fs::read_to_string(&auto).unwrap(), "AUTO-B", "the auto state is not replaced by the undo");
    }

    #[test]
    fn azahar_quit_save_leaves_the_quick_slot_alone() {
        let dir = scratch("park");
        let (quick, auto) = (dir.join("0000000000000000.00.cst"), dir.join("Game.3dsx.auto.cst"));
        std::fs::write(&quick, "manual").unwrap();
        let backup = azahar::park(&quick).unwrap();
        assert!(backup.is_some() && !quick.exists());
        std::fs::write(&quick, "resume").unwrap(); // what Azahar's Quick Save writes meanwhile
        std::fs::rename(&quick, &auto).unwrap();
        azahar::unpark(&quick, backup);
        assert_eq!(std::fs::read_to_string(&quick).unwrap(), "manual");
        assert_eq!(std::fs::read_to_string(&auto).unwrap(), "resume");
        // Nothing to park: nothing restored.
        let empty = dir.join("1.00.cst");
        assert_eq!(azahar::park(&empty).unwrap(), None);
    }

    #[test]
    fn azahar_leftovers_of_an_interrupted_quit_or_resume_are_put_back() {
        let dir = scratch("recover");
        let (quick, auto) = (dir.join("T.00.cst"), dir.join("G.auto.cst"));
        let backup = PathBuf::from(format!("{}.pocket-ui", quick.display()));
        // Interrupted resume: the quick slot is a link to the resume state.
        std::fs::write(&auto, "resume").unwrap();
        std::fs::write(&backup, "manual").unwrap();
        std::fs::hard_link(&auto, &quick).unwrap();
        azahar::recover(&quick, &auto, &dir.join("G.undo.cst"));
        assert_eq!(std::fs::read_to_string(&quick).unwrap(), "manual");
        assert_eq!(std::fs::read_to_string(&auto).unwrap(), "resume");
        assert!(!backup.exists());
        // Interrupted Quit: the new resume state is still in the quick slot.
        std::fs::write(&backup, "manual").unwrap();
        std::fs::write(&quick, "newer resume").unwrap();
        azahar::recover(&quick, &auto, &dir.join("G.undo.cst"));
        assert_eq!(std::fs::read_to_string(&quick).unwrap(), "manual");
        assert_eq!(std::fs::read_to_string(&auto).unwrap(), "newer resume");
        // Nothing parked: nothing touched.
        azahar::recover(&quick, &auto, &dir.join("G.undo.cst"));
        assert_eq!(std::fs::read_to_string(&quick).unwrap(), "manual");
    }

    #[test]
    fn quit_saves_first_then_asks_then_forces() {
        use Step::*;
        for emu in [Emu::Armsx2, Emu::Dolphin, Emu::Azahar] {
            let plan = quit_plan(emu);
            assert_eq!(plan.first(), Some(&SaveResume), "{emu:?}");
            assert_eq!(plan.last(), Some(&Signal(libc::SIGKILL)), "{emu:?}");
            // SIGKILL only after a wait for the process to end by itself.
            assert!(matches!(plan[plan.len() - 2], Exit(_)), "{emu:?}");
            assert!(QUIT_GRACE >= quit_worst(emu) + Duration::from_secs(2), "{emu:?}: {:?}", quit_worst(emu));
        }
        // Quit without saving: the same shutdown and escalation, only the resume state is left out.
        for emu in [Emu::Armsx2, Emu::Dolphin, Emu::Azahar] {
            let plain: Vec<Step> = quit_steps(emu, false).collect();
            assert!(!plain.contains(&SaveResume), "{emu:?}");
            assert_eq!(plain, quit_plan(emu)[1..], "{emu:?}");
            assert_eq!(quit_steps(emu, true).collect::<Vec<_>>(), quit_plan(emu), "{emu:?}");
            assert_eq!(quit_worst_of(emu, false) + SAVE_WAIT, quit_worst(emu), "{emu:?}: closing starts without the save wait");
        }
        // ARMSX2: the first SIGTERM ends the VM, a second one the process.
        assert_eq!(
            quit_plan(Emu::Armsx2),
            &[
                SaveResume,
                Signal(libc::SIGTERM),
                VmDown(VM_SHUTDOWN_WAIT),
                Exit(EXIT_WAIT),
                Signal(libc::SIGTERM),
                Exit(KILL_WAIT),
                Signal(libc::SIGKILL)
            ]
        );
        assert_eq!(quit_worst(Emu::Armsx2), ARMSX2_QUIT_WORST);
        assert_eq!(
            quit_plan(Emu::Dolphin),
            &[SaveResume, Signal(libc::SIGTERM), Exit(KILL_WAIT), Signal(libc::SIGKILL)]
        );
        // Azahar: its window is asked to close (Qt saves its config); SIGTERM would end it on the spot.
        assert_eq!(quit_plan(Emu::Azahar)[1], CloseWindow);
        assert_eq!(quit_plan(Emu::Azahar)[3], Signal(libc::SIGTERM));
    }

    #[test]
    fn state_names_follow_each_emulator() {
        let rom = PathBuf::from("/storage/roms/gamecube/Mario Kart - Double Dash!! (USA).nkit.iso");
        let gc = Session {
            emu: Emu::Dolphin,
            system: "gamecube".into(),
            base: state_base(Emu::Dolphin, &rom),
            rom,
            id: 1,
        };
        // Slot 1 is Dolphin's slot 1 (.s01); the auto state is a file of ours, never one of the slots.
        assert_eq!(
            gc.state_path(Target::Slot(1)).unwrap(),
            PathBuf::from("/storage/roms/savestates/gamecube/Mario Kart - Double Dash!! (USA).nkit.iso.s01")
        );
        assert_eq!(
            gc.state_path(Target::Auto).unwrap(),
            PathBuf::from("/storage/roms/savestates/gamecube/Mario Kart - Double Dash!! (USA).nkit.iso.pocket-auto")
        );
        let ps2 = Session {
            emu: Emu::Armsx2,
            system: "ps2".into(),
            rom: "/x.chd".into(),
            base: None,
            id: 1,
        };
        assert_eq!(ps2.state_path(Target::Slot(1)), None);
        let ps2 = Session {
            base: Some("SLUS-21050 (12345678)".into()),
            ..ps2
        };
        assert_eq!(
            ps2.state_path(Target::Slot(1)).unwrap(),
            PathBuf::from("/storage/roms/savestates/ps2/SLUS-21050 (12345678).01.p2s")
        );
        // The autosave is slot 99 and "Before load" 98: the player's slots 0 to 10 are never touched by them.
        assert_eq!(ps2.state_path(Target::Auto).unwrap(), PathBuf::from("/storage/roms/savestates/ps2/SLUS-21050 (12345678).99.p2s"));
        assert_eq!(ps2.state_path(Target::Undo).unwrap(), PathBuf::from("/storage/roms/savestates/ps2/SLUS-21050 (12345678).98.p2s"));
        assert_eq!([Target::Slot(1), Target::Slot(8), Target::Auto, Target::Undo].map(armsx2_slot), [1, 8, 99, 98]);
    }

    #[test]
    fn no_state_the_menu_writes_lands_in_a_manual_slot_but_save_itself() {
        // Auto (Quit, sleep) and Before load (Load) have places of their own in every emulator, so a Save to slot N
        // is the only thing that ever writes slot N.
        let rom = PathBuf::from("/storage/roms/gamecube/Game.iso");
        let sessions = [
            Session { emu: Emu::Armsx2, system: "ps2".into(), rom: "/x.chd".into(), base: Some("SLUS-1 (AB)".into()), id: 1 },
            Session { emu: Emu::Dolphin, system: "gamecube".into(), base: state_base(Emu::Dolphin, &rom), rom, id: 1 },
            Session { emu: Emu::Azahar, system: "3ds".into(), rom: "/storage/roms/3ds/G.3dsx".into(), base: Some("0000000000000000".into()), id: 1 },
        ];
        for s in &sessions {
            let st = s.states().unwrap();
            let manual: Vec<PathBuf> = st.save_targets().into_iter().filter_map(|t| s.state_path(t)).collect();
            assert!(!manual.is_empty());
            for t in [Target::Auto, Target::Undo] {
                let p = s.state_path(t).unwrap();
                assert!(!manual.contains(&p), "{:?}: {t:?} is {p:?}, a manual slot", s.emu);
            }
        }
        // Dolphin's hotkey start slot (.s00) is not one of the eight either: states only pass through it.
        let gc = sessions[1].states().unwrap();
        assert!(!gc.slots.contains(&StateSet::dolphin_transit("gamecube", sessions[1].base.as_deref().unwrap())));
        assert_eq!(gc.slots.len(), 8);
    }

    #[test]
    fn armsx2_moves_its_old_auto_state_out_of_slot_0_only_when_pocket_ui_wrote_it() {
        let dir = scratch("armsx2-migrate");
        let f = |n: &str| dir.join(format!("SLUS-1 (AB).{n}.p2s"));
        // A player's own slot 0 (no preview): stays, and no auto state appears.
        std::fs::write(f("00"), b"player").unwrap();
        migrate_armsx2_in(&dir, "SLUS-1 (AB)");
        assert_eq!(std::fs::read(f("00")).unwrap(), b"player");
        assert!(!f("99").exists());
        // pocket-ui's (with the preview it writes): moves with its preview, byte for byte.
        std::fs::write(f("00"), b"auto").unwrap();
        std::fs::write(crate::slots::preview_path(&f("00")), b"png").unwrap();
        std::fs::write(f("01"), b"manual 1").unwrap();
        migrate_armsx2_in(&dir, "SLUS-1 (AB)");
        assert_eq!(std::fs::read(f("99")).unwrap(), b"auto");
        assert_eq!(std::fs::read(crate::slots::preview_path(&f("99"))).unwrap(), b"png");
        assert!(!f("00").exists());
        assert_eq!(std::fs::read(f("01")).unwrap(), b"manual 1", "manual slots are not touched");
        // A new auto state is never replaced by an old one.
        std::fs::write(f("00"), b"old").unwrap();
        std::fs::write(crate::slots::preview_path(&f("00")), b"png").unwrap();
        migrate_armsx2_in(&dir, "SLUS-1 (AB)");
        assert_eq!(std::fs::read(f("99")).unwrap(), b"auto");
        assert_eq!(std::fs::read(f("00")).unwrap(), b"old");
    }

    #[test]
    fn dolphin_moves_its_old_states_once_and_never_overwrites() {
        let dir = scratch("dolphin-migrate");
        let marker = dir.join("marker");
        let f = |sfx: &str| dir.join(format!("Game.iso{sfx}"));
        let png = crate::slots::preview_path;
        // Before: Save wrote .s00, Quit wrote .s01 (both with previews).
        std::fs::write(f(".s00"), b"old save").unwrap();
        std::fs::write(png(&f(".s00")), b"p0").unwrap();
        std::fs::write(f(".s01"), b"old auto").unwrap();
        std::fs::write(png(&f(".s01")), b"p1").unwrap();
        std::fs::write(f(".s02"), b"manual 2").unwrap();
        migrate_dolphin_in(&dir, "Game.iso", &marker);
        assert_eq!(std::fs::read(f(".pocket-auto")).unwrap(), b"old auto", "the resume state is kept");
        assert_eq!(std::fs::read(png(&f(".pocket-auto"))).unwrap(), b"p1");
        assert_eq!(std::fs::read(f(".s01")).unwrap(), b"old save", "the old Save is slot 1 now");
        assert_eq!(std::fs::read(png(&f(".s01"))).unwrap(), b"p0");
        assert!(!f(".s00").exists());
        assert_eq!(std::fs::read(f(".s02")).unwrap(), b"manual 2");
        // Once: a later .s00 (a way through) and a new slot 1 are left alone.
        std::fs::write(f(".s00"), b"in transit").unwrap();
        migrate_dolphin_in(&dir, "Game.iso", &marker);
        assert_eq!(std::fs::read(f(".s00")).unwrap(), b"in transit");
        assert_eq!(std::fs::read(f(".s01")).unwrap(), b"old save");
        // A slot 1 without a preview is a player's (Dolphin writes none): it is not the auto state, and .s00 doesn't land on it.
        let dir2 = scratch("dolphin-migrate-player");
        let g = |sfx: &str| dir2.join(format!("G.iso{sfx}"));
        std::fs::write(g(".s00"), b"old save").unwrap();
        std::fs::write(g(".s01"), b"player's slot 1").unwrap();
        migrate_dolphin_in(&dir2, "G.iso", &dir2.join("marker"));
        assert!(!g(".pocket-auto").exists());
        assert_eq!(std::fs::read(g(".s01")).unwrap(), b"player's slot 1");
        assert_eq!(std::fs::read(g(".s00")).unwrap(), b"old save", "nothing is lost");
    }

    #[test]
    fn only_tested_emulators_are_handled() {
        assert_eq!(Emu::detect("ps2", "armsx2", "armsx2-sa"), Some(Emu::Armsx2));
        assert_eq!(Emu::detect("ps2", "aethersx2", "aethersx2-sa"), None);
        assert_eq!(Emu::detect("gamecube", "dolphin", "dolphin-sa-gc"), Some(Emu::Dolphin));
        assert_eq!(Emu::detect("gamecube", "dolphin", "dolphin-qt-gc"), None);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_header_peek_is_not_taken_for_the_state_load() {
        use std::io::Read;
        let dir = scratch("opened");
        let path = dir.join("T.00.cst");
        std::fs::write(&path, vec![7u8; 1 << 16]).unwrap();
        let watch = azahar::opened::Watch::new(&path).unwrap();
        let (peeked, peeks_done) = std::sync::mpsc::channel();
        let reader = {
            let path = path.clone();
            std::thread::spawn(move || {
                // Azahar lists the slots first (header reads), then loads: the file stays open meanwhile. The reads are
                // over before pocket-ui looks (it is busy with its own frame), so their open/close pairs are there as
                // events and cancel out instead of being timed one by one: a peek lasts microseconds, but a thread
                // paused by a loaded machine must not look like a load (a gate saw exactly that, 2026-10-28).
                for _ in 0..3 {
                    let mut head = [0u8; 256];
                    std::fs::File::open(&path).unwrap().read_exact(&mut head).unwrap();
                    std::thread::sleep(Duration::from_millis(40));
                }
                peeked.send(()).unwrap();
                let mut f = std::fs::File::open(&path).unwrap();
                let started = Instant::now();
                std::thread::sleep(Duration::from_millis(200));
                let _ = f.read_to_end(&mut Vec::new());
                started
            })
        };
        peeks_done.recv().unwrap();
        assert!(watch.held_open(Duration::from_millis(25), Duration::from_secs(3)));
        let seen = Instant::now();
        let load_started = reader.join().unwrap();
        // Recognised during the load, not at one of the header reads before it.
        assert!(seen >= load_started, "taken for a header read");
        // Nobody opens it: no load.
        let quiet = azahar::opened::Watch::new(&path).unwrap();
        assert!(!quiet.held_open(Duration::from_millis(25), Duration::from_millis(300)));
    }

    // ---- Wii: what the emulated Wiimote sees of our chords

    /// The name Dolphin gives an InputPlumber button (evdev; West and North cross over, measured on the device).
    fn dolphin_input(button: &str) -> &'static str {
        match button {
            "Gamepad:Button:LeftStick" => "Thumb L",
            "Gamepad:Button:RightStick" => "Thumb R",
            "Gamepad:Button:Select" => "Back",
            "Gamepad:Button:Guide" => "Guide",
            "Gamepad:Button:DPadUp" => "Pad N",
            "Gamepad:Button:DPadDown" => "Pad S",
            "Gamepad:Button:South" => "Button S",
            "Gamepad:Button:East" => "Button E",
            "Gamepad:Button:West" => "Button N",
            "Gamepad:Button:North" => "Button W",
            other => panic!("unknown button {other}"),
        }
    }

    /// Dolphin's control expressions as the stock profiles use them: `` `Input` ``, `!`, `&`, `|`, parentheses.
    /// Inputs not in `held` (axes included) are off.
    fn eval(expr: &str, held: &[&str]) -> bool {
        fn or(t: &mut &str, held: &[&str]) -> bool {
            let mut v = and(t, held);
            while let Some(rest) = t.trim_start().strip_prefix('|') {
                *t = rest;
                v |= and(t, held);
            }
            v
        }
        fn and(t: &mut &str, held: &[&str]) -> bool {
            let mut v = not(t, held);
            while let Some(rest) = t.trim_start().strip_prefix('&') {
                *t = rest;
                v &= not(t, held);
            }
            v
        }
        fn not(t: &mut &str, held: &[&str]) -> bool {
            *t = t.trim_start();
            if let Some(rest) = t.strip_prefix('!') {
                *t = rest;
                return !not(t, held);
            }
            if let Some(rest) = t.strip_prefix('(') {
                *t = rest;
                let v = or(t, held);
                *t = t.trim_start().strip_prefix(')').expect("closing parenthesis");
                return v;
            }
            let rest = t.strip_prefix('`').expect("an input in backticks");
            let end = rest.find('`').expect("closing backtick");
            let name = &rest[..end];
            *t = &rest[end + 1..];
            held.contains(&name)
        }
        let mut t = expr;
        let v = or(&mut t, held);
        assert!(t.trim().is_empty(), "left over in {expr:?}: {t:?}");
        v
    }

    /// Every chord pocket-ui sends to Dolphin in a Wii game: save and load on every slot it uses (0, which carries
    /// Auto and Before load, and the player's 1 to 8), and the three setting hotkeys with the buttons of the GC
    /// profile start_dolphin_wii.sh copies.
    fn wii_chords(hk: &'static str) -> Vec<Vec<&'static str>> {
        use dolphin::Action::*;
        let gc = include_str!("fixtures/dolphin-gcpad-south.ini");
        let mut v = Vec::new();
        for slot in 0..=8u8 {
            for action in [Save, Load] {
                v.extend(dolphin::chords(slot, action, hk));
            }
        }
        for gc_button in ["Y", "X", "B"] {
            let button = crate::emu_settings::profile_button(gc, gc_button).expect("the stock profile binds it");
            v.push(dolphin::toggle(button, hk));
        }
        v
    }

    /// What is held after each step of a chord: pressed in order, released in reverse.
    fn chord_steps(chord: &[&str]) -> Vec<Vec<&'static str>> {
        let names: Vec<&'static str> = chord.iter().map(|b| dolphin_input(b)).collect();
        let mut steps: Vec<Vec<&'static str>> = (1..=names.len()).map(|n| names[..n].to_vec()).collect();
        steps.extend((0..names.len()).rev().map(|n| names[..n].to_vec()));
        steps
    }

    #[test]
    fn no_chord_of_ours_presses_a_wiimote_button() {
        let profiles = [
            ("classic", include_str!("fixtures/dolphin-wii-classic.ini")),
            ("hremote", include_str!("fixtures/dolphin-wii-hremote.ini")),
            ("nunchuck", include_str!("fixtures/dolphin-wii-nunchuck.ini")),
            ("vremote", include_str!("fixtures/dolphin-wii-vremote.ini")),
        ];
        for (name, stock) in profiles {
            let quiet = defaults::with_quiet_wiimote(stock);
            assert_eq!(defaults::with_quiet_wiimote(&quiet), quiet, "{name}: a second start changes nothing");
            let bindings: Vec<(&str, &str)> = quiet
                .lines()
                .filter_map(|l| l.split_once('='))
                .map(|(k, v)| (k.trim(), v.trim()))
                .filter(|(_, v)| v.contains('`'))
                .collect();
            for hk in [dolphin::hotkey_name(false), dolphin::hotkey_name(true)] {
                for chord in wii_chords(hk) {
                    for held in chord_steps(&chord) {
                        for (key, expr) in &bindings {
                            assert!(!eval(expr, &held), "{name}: {key} = {expr} is pressed by {chord:?} while {held:?} are held");
                        }
                    }
                }
            }
            // The game itself still works: every button the profile binds still presses its Wiimote button.
            for (key, expr) in quiet.lines().filter_map(|l| l.split_once('=')).map(|(k, v)| (k.trim(), v.trim())) {
                let Some(input) = stock
                    .lines()
                    .find_map(|l| l.split_once('=').filter(|(k, _)| k.trim() == key))
                    .and_then(|(_, v)| v.split('`').nth(1))
                else {
                    continue;
                };
                assert!(eval(expr, &[input]), "{name}: {key} no longer answers to {input}");
            }
        }
    }

    #[test]
    fn setting_hotkeys_hold_both_stick_clicks_first() {
        // Like the slot steps: with both clicks held the profiles' extra L/R bindings stay off.
        let c = dolphin::toggle("Gamepad:Button:North", dolphin::hotkey_name(true));
        assert_eq!(c, ["Gamepad:Button:LeftStick", "Gamepad:Button:RightStick", "Gamepad:Button:Guide", "Gamepad:Button:North"]);
        let p = defaults::with_extra_triggers("Triggers/L = `Trigger L`\nTriggers/R = `Trigger R`\n", "Guide");
        assert!(p.contains("(`Guide` & `Thumb L` & !`Thumb R`)") && p.contains("(`Guide` & `Thumb R` & !`Thumb L`)"));
    }

    fn xbox_session() -> Session {
        Session {
            emu: Emu::Hakux,
            system: "xbox".into(),
            rom: PathBuf::from("/storage/roms/xbox/Jade Empire (USA).iso"),
            base: state_base(Emu::Hakux, Path::new("/storage/roms/xbox/Jade Empire (USA).iso")),
            id: 7,
        }
    }

    #[test]
    fn only_hakux_brings_the_game_window_forward_before_quitting() {
        // `Step::RaQuit` is what shows the game's window (control::show_game) before RetroArch's QUIT; no other guest's plan has it.
        for emu in [Emu::Armsx2, Emu::Dolphin, Emu::Azahar] {
            assert!(!quit_plan(emu).contains(&Step::RaQuit), "{emu:?}");
        }
        assert_eq!(quit_plan(Emu::Hakux).iter().filter(|s| **s == Step::RaQuit).count(), 1);
    }

    #[test]
    fn hakux_quits_with_retroarchs_own_command_then_signals_then_the_group() {
        use Step::*;
        let plan = quit_plan(Emu::Hakux);
        let at = |s: Step| plan.iter().position(|p| *p == s).unwrap();
        assert_eq!(plan.first(), Some(&SaveResume));
        assert!(at(RaQuit) < at(Signal(libc::SIGTERM)), "RetroArch's QUIT before any signal");
        assert_eq!(plan.last(), Some(&KillGroup), "the process group is the last resort");
        assert!(matches!(plan[plan.len() - 2], Exit(_)), "a wait before the group goes");
        assert!(QUIT_GRACE >= quit_worst(Emu::Hakux) + Duration::from_secs(2), "{:?}", quit_worst(Emu::Hakux));
        assert_eq!(Emu::Hakux.process(), "retroarch");
    }

    #[test]
    fn hakux_offers_save_and_load_only_when_the_core_has_states() {
        let s = xbox_session();
        assert_eq!(s.base.as_deref(), Some("Jade Empire (USA)"), "RetroArch names states after the content without its extension");
        // The core reports serialize_size 0 today: no states, and the menu says why.
        assert!(s.states_with(false).is_none());
        let line = Some(crate::hakux::NO_STATES);
        assert_eq!(crate::pause_saves::Saves::of(crate::quit::Saver::Standalone, false, line), crate::pause_saves::Saves::InGame(crate::hakux::NO_STATES));
        // Once the core's info says it has them, Save, Load and the auto state of Quit appear without another change.
        let st = s.states_with(true).expect("states with a core that has them");
        assert_eq!(st.slot_count(), 8);
        assert_eq!(st.target_path(Target::Slot(1)), Some(Path::new("/storage/roms/savestates/xbox/Jade Empire (USA).state1")));
        assert_eq!(st.target_path(Target::Auto), Some(Path::new("/storage/roms/savestates/xbox/Jade Empire (USA).state.auto")));
        assert_eq!(st.target_path(Target::Undo), Some(Path::new("/storage/roms/savestates/xbox/Jade Empire (USA).state98")));
        assert_eq!(crate::pause_saves::Saves::of(crate::quit::Saver::Standalone, true, line), crate::pause_saves::Saves::Slots);
        assert_eq!((hakux_slot(Target::Auto), hakux_slot(Target::Undo), hakux_slot(Target::Slot(3))), (-1, 98, 3));
    }

    #[test]
    fn the_shipped_hakux_info_does_not_claim_states() {
        // If this fails the core has been updated with save states: the pause menu then offers them (`Session::states`),
        // so the device test of docs/XBOX.md "Save states" has to be done first.
        assert!(!crate::hakux::states_supported(include_str!("../deploy/hakux/hakux_libretro.info")));
    }
}

