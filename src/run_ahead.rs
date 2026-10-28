//! Run-ahead: the game answers the buttons one or two frames sooner (pause menu, Options > Extras > Run-ahead).
//!
//! Most games react to a button press a frame or two after it, inside the emulated machine. Run-ahead hides that
//! delay the way RetroArch's single-instance "Run-Ahead" does: every screen refresh pocket-core
//!
//! 1. runs the real frame(s) with the buttons as they are now, unseen and unheard,
//! 2. saves the machine's state (`retro_serialize`) into memory,
//! 3. runs 1 or 2 more frames with the same buttons; the last one is the picture shown,
//! 4. loads the saved state back (`retro_unserialize`), so the game goes on from the real frame.
//!
//! What is shown is always N frames ahead of the real game, computed with the buttons held now; a press shows up
//! N frames sooner. The sound is the one of the last real frames and the frames ahead, one refresh's worth, every
//! refresh: as many samples as without run-ahead, so the sound never drifts (see `Step`).
//!
//! Frames nobody sees or hears are run without picture or sound (`libretro::Core::run_with`,
//! RETRO_ENVIRONMENT_GET_AUDIO_VIDEO_ENABLE), so a core that asks can skip drawing and mixing them.
//!
//! **Safety.** Run-ahead switches itself off, with a short notice over the game, when
//! - the system or its core is not suited: not one of the light systems (`SYSTEMS`), a 3D core, no save
//!   states, a core that says its states are incomplete (SET_SERIALIZATION_QUIRKS), or a core whose RetroArch
//!   info file doesn't call its states deterministic (`savestate_features`; RetroArch uses the same rule);
//! - saving and loading the state takes too long (`SERIALIZE_SHARE` of a refresh, on average);
//! - the whole run (real frames, frames ahead, save, load) keeps going over its budget in more than
//!   `OVER_LIMIT` of the last `WINDOW` refreshes. Counted in CPU time of the game thread, so the system taking
//!   the CPU away for a moment doesn't count. A refresh is over when it takes more than `BUDGET_SHARE` of a
//!   refresh at the clock it ran at: the rest is enough for drawing and the swap (on the Mini V2, frames whose
//!   core took 9-12 ms all made their vsync, docs/HOST.md). Not scaled by the CPU's clock: a clock read now and
//!   then describes neither the refresh measured nor the cluster (the game thread moves between the little, big
//!   and prime cores, which differ in speed at the same MHz); the clock is only logged (`Clock`). Timing is
//!   not judged while things settle: `SETTLE_START`
//!   refreshes after switching on (the game's start, the resume state, the governor), `SETTLE_EVENT` after
//!   anything from outside (a load, an option, the pause) or a gap in the refreshes (fast-forward, rewind,
//!   the menu);
//! - the game doesn't replay exactly: now and then the picture shown ahead is compared with the real frame
//!   that comes out later with the same buttons. When at least `MISMATCH_LIMIT` of the last `COMPARED`
//!   comparisons and a quarter of them differ, it is off. A state that misses part of the machine differs nearly
//!   every time; a single difference is no proof (gambatte with uCity, 2 frames ahead: 2 of 237 differed at the
//!   start, probably the stale picture of a Game Boy screen that is switched off, which is no part of the state).
//!
//! Choosing another value in the menu tries again. Fast-forward and rewind run as before (run-ahead pauses
//! meanwhile); save slots, the auto state and rewind always get the real game, since states are taken
//! between refreshes, after step 4. Rewind reuses the state of step 2 instead of saving its own.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::libretro::{AV_AUDIO, AV_VIDEO, QUIRK_INCOMPLETE, QUIRK_MUST_INITIALIZE};

/// Systems run-ahead is offered for: the light 2D ones, whose cores RetroArch lists with deterministic states
/// and which cost well under a millisecond a frame (docs/HOST.md, Run-ahead). ROCKNIX system names, as in
/// `hostsys` and pocket-ui's `HOST_CORES`.
#[rustfmt::skip]
pub const SYSTEMS: &[&str] = &[
    "nes", "snes", "gb", "gbc", "gba",
    "megadrive", "megadrive-japan", "genesis", "genh", "megadriveh",
    "mastersystem", "gamegear", "ggh", "sg-1000",
    "pcengine", "tg16",
    // Cheap as well (deterministic states, small machines).
    "atari2600", "ngp", "ngpc", "wonderswan", "wonderswancolor", "pokemini",
    // The second folders of the consoles above, the Disk System, SuperGrafx and Virtual Boy (`hostsys`, 2026-10-23;
    // beetle_vb's info file says nothing about its states, so the picture comparison decides).
    "famicom", "nesh", "fds", "sfc", "snesh", "gbh", "gbch", "gbah", "gbav", "supergrafx", "virtualboy",
];

/// Whether the pause menu offers run-ahead for `system`.
pub fn available_for(system: &str) -> bool {
    SYSTEMS.contains(&system)
}

/// At most this many frames ahead.
pub const MAX_FRAMES: u32 = 2;
/// Share of a refresh the whole run may take (real frames, frames ahead, save and load) in CPU time of the game
/// thread at the clock it ran at; the rest is the picture's (draw, swap) and the system's. 11.7 ms at 60 Hz.
pub const BUDGET_SHARE: f32 = 0.7;
/// Share of a refresh saving plus loading the state may take on average.
pub const SERIALIZE_SHARE: f32 = 0.25;
/// Refreshes looked at for the budget, and how many of them may go over it.
pub const WINDOW: usize = 120;
pub const OVER_LIMIT: usize = 12;
/// Refreshes whose timing isn't judged after switching on (3 s: the game's start, the resume state, caches, the
/// governor raising the clock) and after something from outside or a gap in the refreshes (1 s).
pub const SETTLE_START: u64 = 180;
pub const SETTLE_EVENT: u64 = 60;
/// A pause between two refreshes longer than this many refresh periods is a gap (fast-forward, rewind, the menu).
pub const GAP_PERIODS: f32 = 3.0;
/// The CPU's clock is read every this many refreshes (two small sysfs reads).
pub const CLOCK_EVERY: u64 = 30;
/// Refreshes the average save + load is taken over before it is judged.
pub const SERIALIZE_SAMPLES: u32 = 60;
/// The picture ahead is compared with the real frame during the first `CHECK_FIRST` real frames, then for
/// `CHECK_LEN` of every `CHECK_EVERY` (so the check costs nearly nothing over a session).
pub const CHECK_FIRST: u64 = 240;
pub const CHECK_EVERY: u64 = 1800;
pub const CHECK_LEN: u64 = 60;
/// Within those, every this many real frames (each comparison hashes two pictures and makes the real frame draw).
pub const CHECK_STRIDE: u64 = 4;
/// Differences among the last `COMPARED` comparisons after which the game counts as not replaying exactly
/// (when they are also a quarter of them).
pub const MISMATCH_LIMIT: usize = 3;
pub const COMPARED: usize = 60;
/// A core that has to run a while before it can save (QUIRK_MUST_INITIALIZE) gets this many refreshes.
pub const INIT_GRACE: u64 = 600;

/// Why run-ahead switched itself off. The notice says it over the game; the log says it with numbers.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Off {
    /// Not one of `SYSTEMS`, a 3D core, no states, incomplete states, or not deterministic by its info file.
    Unsuited,
    /// Saving and loading the state takes too long.
    SlowStates,
    /// The run goes over its budget too often.
    TooSlow,
    /// The picture ahead and the real one differ with the same buttons.
    NotExact,
    /// The core refused to save or load the state.
    StateFailed,
}

impl Off {
    pub fn notice(self) -> &'static str {
        match self {
            Off::Unsuited => "Run-ahead isn't available for this emulator",
            Off::SlowStates => "Run-ahead is off: saving states takes too long here",
            Off::TooSlow => "Run-ahead is off: too slow for this game",
            Off::NotExact => "Run-ahead is off: this game doesn't replay exactly",
            Off::StateFailed => "Run-ahead is off: the emulator couldn't save its state",
        }
    }
}

/// What a frame's output is for (`RunAhead::step` hands every frame it runs to its `keep` callback).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Keep {
    /// This is the picture to show.
    pub video: bool,
    /// Its sound is played.
    pub audio: bool,
    /// A frame of the real game (counted as an emulated frame); the others are run ahead and rolled back.
    pub real: bool,
}

/// The emulator as run-ahead needs it. `libretro::Core` is one; the tests use a stub.
pub trait Machine {
    /// What one frame produced (picture, sound, …).
    type Out;
    /// Runs one frame with these buttons, making only what `enable` asks for (`libretro::AV_*`).
    fn run(&mut self, buttons: u16, enable: u32) -> Self::Out;
    fn save(&mut self, buf: &mut Vec<u8>) -> bool;
    fn load(&mut self, buf: &[u8]) -> bool;
    /// A fingerprint of the frame's picture, None when it brought none.
    fn picture(out: &Self::Out) -> Option<u64>;
    /// How often `save` had to ask the core for its state size since the last call, and how long that took (ms).
    fn size_queries(&mut self) -> (u32, f32) {
        (0, 0.0)
    }
}

impl Machine for crate::libretro::Core {
    type Out = (
        Option<crate::libretro::Frame>,
        Vec<i16>,
        Option<crate::libretro::AvInfo>,
        Option<crate::libretro::Geometry>,
    );

    fn run(&mut self, buttons: u16, enable: u32) -> Self::Out {
        self.run_with(buttons, enable)
    }

    fn save(&mut self, buf: &mut Vec<u8>) -> bool {
        self.save_quick(buf)
    }

    fn load(&mut self, buf: &[u8]) -> bool {
        self.load_quick(buf)
    }

    fn picture(out: &Self::Out) -> Option<u64> {
        out.0.as_ref().map(picture_hash)
    }

    fn size_queries(&mut self) -> (u32, f32) {
        self.take_size_queries()
    }
}

/// FNV-1a over the picture's own bytes, line by line (not the padding to the pitch, which nobody draws).
/// Eight bytes a step (the device's first run: byte by byte it cost up to 0.4 ms a picture at a low clock).
pub fn picture_hash(f: &crate::libretro::Frame) -> u64 {
    let bpp = if f.format == crate::libretro::PIXEL_XRGB8888 { 4 } else { 2 };
    let line = f.width as usize * bpp;
    let mut h: u64 = 0xcbf29ce484222325;
    for row in (0..f.height as usize).filter_map(|y| f.data.get(y * f.pitch..y * f.pitch + line)) {
        let mut words = row.chunks_exact(8);
        for w in &mut words {
            h = (h ^ u64::from_le_bytes(w.try_into().unwrap())).wrapping_mul(0x100000001b3);
        }
        for b in words.remainder() {
            h = (h ^ *b as u64).wrapping_mul(0x100000001b3);
        }
    }
    h
}

/// One frame of a refresh as `RunAhead::step` runs it: `real` frames of the game, then `ahead` frames.
/// The picture is the last frame's; the sound is the last `real` frames' (counted over both), so a refresh
/// with one real frame plays one frame of sound, as without run-ahead. For one frame ahead that is the frame
/// ahead's sound, as in RetroArch; a refresh that runs two real frames (a 50 Hz game) plays two.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Step {
    pub real: bool,
    pub video: bool,
    pub audio: bool,
}

/// The frames of one refresh: `real` real ones, then `ahead` ones (none when nothing is due).
pub fn plan(real: u32, ahead: u32) -> Vec<Step> {
    if real == 0 {
        return Vec::new();
    }
    let total = real + ahead;
    (0..total)
        .map(|i| Step {
            real: i < real,
            video: i + 1 == total,
            audio: i >= ahead,
        })
        .collect()
}

/// The picture shown ahead, to compare with the real frame `target` when it comes.
struct Prediction {
    target: u64,
    hash: u64,
    input: u64,
    /// The real frame the frames ahead started from.
    from: u64,
}

/// Timing of the run-ahead refreshes, for the exit report.
#[derive(Default, Clone, Copy)]
pub struct Stats {
    pub refreshes: u64,
    pub total_ms: f64,
    /// CPU time of the game thread (what the budget is judged by).
    pub cpu_ms: f64,
    pub max_ms: f32,
    pub save_ms: f64,
    pub load_ms: f64,
    pub over: u64,
    pub state_bytes: usize,
    /// Where the time of a refresh goes (wall ms, summed): the real frames, the frames ahead, hashing and comparing
    /// pictures, the host's handling of each frame (`keep`: sound, achievements), and how often and how long the
    /// state size was asked for (inside `save_ms`).
    pub real_ms: f64,
    pub ahead_ms: f64,
    pub check_ms: f64,
    pub host_ms: f64,
    pub size_calls: u64,
    pub size_ms: f64,
    /// Sum of the clock samples (share of the highest clock allowed) and how many, for the report.
    pub clock_sum: f64,
    pub clock_n: u32,
}

/// CPU time of the refreshes run without run-ahead (`RunAhead::note_plain`): the baseline to compare with.
#[derive(Default, Clone, Copy)]
pub struct Plain {
    pub refreshes: u64,
    pub cpu_ms: f64,
    pub max_ms: f32,
    pub clock_sum: f64,
    pub clock_n: u32,
}

/// The clock of the CPU the game thread was on: which CPU, its clock now and the highest it may run at (MHz).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Clock {
    pub cpu: u32,
    pub cur_mhz: u32,
    pub max_mhz: u32,
}

impl Clock {
    /// Share of the highest allowed clock it runs at now (0..1].
    pub fn share(&self) -> f32 {
        (self.cur_mhz as f32 / self.max_mhz.max(1) as f32).clamp(0.05, 1.0)
    }

    /// The calling thread's CPU, read from `root` (`/` on the device). None without cpufreq (a container).
    pub fn read(root: &Path) -> Option<Clock> {
        Self::read_cpu(root, current_cpu()?)
    }

    pub fn read_cpu(root: &Path, cpu: u32) -> Option<Clock> {
        let dir = root.join(format!("sys/devices/system/cpu/cpu{cpu}/cpufreq"));
        let khz = |f: &str| std::fs::read_to_string(dir.join(f)).ok()?.trim().parse::<u64>().ok();
        let (cur, max) = (khz("scaling_cur_freq")?, khz("scaling_max_freq")?);
        (max > 0).then_some(Clock {
            cpu,
            cur_mhz: (cur / 1000) as u32,
            max_mhz: (max / 1000) as u32,
        })
    }
}

#[cfg(target_os = "linux")]
fn current_cpu() -> Option<u32> {
    // SAFETY: no arguments, returns the CPU number or -1.
    let n = unsafe { libc::sched_getcpu() };
    (n >= 0).then_some(n as u32)
}

#[cfg(not(target_os = "linux"))]
fn current_cpu() -> Option<u32> {
    None
}

pub struct RunAhead {
    /// Frames ahead the settings ask for (0 = off).
    want: u32,
    /// Switched itself off (until the setting changes).
    off: Option<Off>,
    /// The core was looked at for this setting (`admit`).
    admitted: bool,
    notice: Option<&'static str>,
    system: String,
    core: PathBuf,
    /// The state after the last real frame (step 2), also what rewind keeps.
    state: Vec<u8>,
    saved_once: bool,
    quirks: u64,
    /// Refreshes run ahead since switching on.
    refreshes: u64,
    /// Real frames run since switching on (the number of the last one).
    real: u64,
    input: Option<u64>,
    /// The first real frame of the current run of unchanged input.
    stable_since: u64,
    predictions: VecDeque<Prediction>,
    pub same: u32,
    pub different: u32,
    /// The last `COMPARED` comparisons, true where the pictures differed.
    recent: VecDeque<bool>,
    over: VecDeque<bool>,
    serialize_ms: f64,
    serialize_n: u32,
    /// Timing is judged from this refresh on (`SETTLE_START`, `SETTLE_EVENT`).
    settle_until: u64,
    last_step: Option<Instant>,
    /// The clock last read (`CLOCK_EVERY`), None where there is no cpufreq: the time then counts as it is.
    pub clock: Option<Clock>,
    clock_root: PathBuf,
    pub stats: Stats,
    pub plain: Plain,
    /// Why it went off, with the numbers, for the log.
    pub log: Option<String>,
}

impl Default for RunAhead {
    fn default() -> Self {
        Self::new()
    }
}

impl RunAhead {
    pub fn new() -> RunAhead {
        RunAhead {
            want: 0,
            off: None,
            admitted: false,
            notice: None,
            system: String::new(),
            core: PathBuf::new(),
            state: Vec::new(),
            saved_once: false,
            quirks: 0,
            refreshes: 0,
            real: 0,
            input: None,
            stable_since: 0,
            predictions: VecDeque::new(),
            same: 0,
            different: 0,
            recent: VecDeque::new(),
            over: VecDeque::new(),
            serialize_ms: 0.0,
            serialize_n: 0,
            settle_until: SETTLE_START,
            last_step: None,
            clock: None,
            clock_root: PathBuf::from("/"),
            stats: Stats::default(),
            plain: Plain::default(),
            log: None,
        }
    }

    /// The core file and the system of this session (for `admit`).
    pub fn set_core(&mut self, core: &Path, system: &str) {
        self.core = core.to_path_buf();
        self.system = system.to_string();
    }

    /// The setting: 0 (off), 1 or 2 frames. Another value than before starts over, also after it switched
    /// itself off.
    pub fn configure(&mut self, frames: u32) {
        let frames = frames.min(MAX_FRAMES);
        if frames == self.want {
            return;
        }
        self.want = frames;
        self.off = None;
        self.admitted = false;
        self.restart();
    }

    /// Frames ahead in use now: 0 when off, switched off, or not yet `admit`ted.
    pub fn frames(&self) -> u32 {
        if self.off.is_some() || !self.admitted {
            0
        } else {
            self.want
        }
    }

    pub fn off(&self) -> Option<Off> {
        self.off
    }

    /// Frames the setting asks for (0 = off), whether or not they are in use (Pixel Path).
    pub fn wanted(&self) -> u32 {
        self.want
    }

    /// The core still has to be looked at (`admit`) before the first refresh run ahead.
    pub fn needs_admit(&self) -> bool {
        self.want > 0 && !self.admitted && self.off.is_none()
    }

    /// Looks at the system and the core once, when run-ahead is switched on: `has_states` (the core has save
    /// states and they are not switched off), `hw` (a 3D core with its own GL context), `quirks` (what it said
    /// about its states).
    pub fn admit(&mut self, has_states: bool, hw: bool, quirks: u64) {
        self.admitted = true;
        self.quirks = quirks;
        let info = core_info_deterministic(&self.core);
        let why = if !available_for(&self.system) {
            Some(format!("not offered for {}", self.system))
        } else if hw {
            Some("3D core".into())
        } else if !has_states {
            Some("no save states".into())
        } else if quirks & QUIRK_INCOMPLETE != 0 {
            Some("the core says its states are incomplete".into())
        } else if info == Some(false) {
            Some(format!("{} doesn't call its states deterministic", self.core.with_extension("info").display()))
        } else {
            None
        };
        match why {
            Some(why) => self.switch_off(Off::Unsuited, why),
            None => println!(
                "run-ahead: {} frame(s) for {}, states quirks {quirks:#x}, info {}",
                self.want,
                self.system,
                match info {
                    Some(true) => "deterministic",
                    Some(false) => "not deterministic",
                    None => "missing",
                }
            ),
        }
    }

    /// Something happened to the game from outside between refreshes (a load, rewind, a changed option,
    /// cheats, the pause): the pictures shown ahead before it can't be compared any more.
    pub fn forget(&mut self) {
        self.predictions.clear();
        self.input = None;
        self.settle(SETTLE_EVENT);
    }

    /// The next `n` refreshes' timing is not judged.
    fn settle(&mut self, n: u64) {
        self.settle_until = self.settle_until.max(self.refreshes + n);
    }

    /// Reads the clock every `CLOCK_EVERY` calls (`n` counts them).
    fn sample_clock(&mut self, n: u64) -> Option<f32> {
        if n.is_multiple_of(CLOCK_EVERY) {
            self.clock = Clock::read(&self.clock_root);
        }
        self.clock.map(|c| c.share())
    }

    /// A refresh run without run-ahead (off, switched off, or not this refresh) took `cpu_ms` of the game thread's
    /// CPU time: the baseline for the exit report.
    pub fn note_plain(&mut self, cpu_ms: f32) {
        let n = self.plain.refreshes;
        let share = self.sample_clock(n);
        let p = &mut self.plain;
        p.refreshes += 1;
        p.cpu_ms += cpu_ms as f64;
        p.max_ms = p.max_ms.max(cpu_ms);
        if let Some(s) = share {
            p.clock_sum += s as f64;
            p.clock_n += 1;
        }
    }

    fn restart(&mut self) {
        self.forget();
        self.settle_until = SETTLE_START;
        self.last_step = None;
        self.refreshes = 0;
        self.real = 0;
        self.saved_once = false;
        self.over.clear();
        self.recent.clear();
        self.serialize_ms = 0.0;
        self.serialize_n = 0;
    }

    fn switch_off(&mut self, why: Off, detail: String) {
        let line = format!("run-ahead off ({why:?}): {detail}");
        println!("{line}");
        self.log = Some(line);
        self.off = Some(why);
        self.notice = Some(why.notice());
        self.forget();
    }

    /// A notice for the HUD, once.
    pub fn take_notice(&mut self) -> Option<&'static str> {
        self.notice.take()
    }

    /// The state of the real game after the last refresh `step` ran (rewind keeps it instead of saving its own).
    pub fn state(&self) -> &[u8] {
        &self.state
    }

    /// Whether the picture ahead of this refresh is compared later.
    fn checking(&self) -> bool {
        self.real.is_multiple_of(CHECK_STRIDE) && (self.real < CHECK_FIRST || self.real % CHECK_EVERY < CHECK_LEN)
    }

    /// One screen refresh with run-ahead: `real` frames of the game are due (usually 1; 0 or 2 for games
    /// whose rate differs from the screen's), `buttons` held, `input` a fingerprint of everything the game
    /// reads (buttons, sticks, touch: `input_key`), `period_ms` the refresh. Every frame run goes to `keep`
    /// with what it is for. False when nothing ran: run-ahead is off, or nothing is due, or a core that must
    /// run a while before it can save isn't there yet; the caller runs its frames as usual then.
    pub fn step<M: Machine>(&mut self, m: &mut M, real: u32, buttons: u16, input: u64, period_ms: f32, mut keep: impl FnMut(M::Out, Keep)) -> bool {
        let ahead = self.frames();
        if ahead == 0 || real == 0 {
            return false;
        }
        let t0 = Instant::now();
        let c0 = thread_cpu_ms();
        // Fast-forward, rewind or the menu came in between: the clock and the caches settle again.
        if self
            .last_step
            .is_some_and(|l| t0.saturating_duration_since(l).as_secs_f32() * 1000.0 > period_ms * GAP_PERIODS)
        {
            self.settle(SETTLE_EVENT);
        }
        // A core that can't save yet (QUIRK_MUST_INITIALIZE) is tried before anything runs, so the refresh
        // still has its picture when it fails.
        if !self.saved_once {
            if !m.save(&mut self.state) {
                if self.quirks & QUIRK_MUST_INITIALIZE != 0 && self.refreshes < INIT_GRACE {
                    self.refreshes += 1;
                    return false;
                }
                self.switch_off(Off::StateFailed, "retro_serialize failed before the first refresh".into());
                return false;
            }
            self.saved_once = true;
        }
        let steps = plan(real, ahead);
        let mut save_ms = 0.0;
        // Where the time goes (`Stats`): a clock read around each part, about a microsecond in all.
        let ms = |t: Instant| t.elapsed().as_secs_f64() * 1000.0;
        let (mut real_ms, mut ahead_ms, mut check_ms, mut host_ms) = (0.0, 0.0, 0.0, 0.0);
        for (i, s) in steps.iter().enumerate() {
            if s.real {
                self.real += 1;
                if self.input != Some(input) {
                    self.input = Some(input);
                    self.stable_since = self.real;
                }
                while self.predictions.front().is_some_and(|p| p.target < self.real) {
                    self.predictions.pop_front();
                }
                // The picture of this real frame is needed only to compare it with the one shown ahead.
                let compare = self
                    .predictions
                    .front()
                    .filter(|p| p.target == self.real && p.input == input && self.stable_since <= p.from)
                    .map(|p| p.hash);
                let mut enable = if s.audio { AV_AUDIO } else { 0 };
                if compare.is_some() {
                    enable |= AV_VIDEO;
                }
                let t = Instant::now();
                let out = m.run(buttons, enable);
                real_ms += ms(t);
                if let Some(want) = compare {
                    let t = Instant::now();
                    if let Some(got) = M::picture(&out) {
                        if want == got {
                            self.same += 1;
                        } else {
                            self.different += 1;
                            println!("run-ahead: frame {} differs from the one shown ahead ({} so far)", self.real, self.different);
                        }
                        self.recent.push_back(want != got);
                        if self.recent.len() > COMPARED {
                            self.recent.pop_front();
                        }
                    }
                    check_ms += ms(t);
                }
                let t = Instant::now();
                keep(
                    out,
                    Keep {
                        video: false,
                        audio: s.audio,
                        real: true,
                    },
                );
                host_ms += ms(t);
                if !steps[i + 1].real {
                    let t = Instant::now();
                    let ok = m.save(&mut self.state);
                    save_ms = t.elapsed().as_secs_f32() * 1000.0;
                    if !ok {
                        // This refresh shows the picture before; the game itself is where it should be.
                        self.switch_off(Off::StateFailed, "retro_serialize failed".into());
                        return true;
                    }
                }
            } else {
                let enable = (if s.audio { AV_AUDIO } else { 0 }) | (if s.video { AV_VIDEO } else { 0 });
                let t = Instant::now();
                let out = m.run(buttons, enable);
                ahead_ms += ms(t);
                if s.video && self.checking() {
                    let t = Instant::now();
                    if let Some(hash) = M::picture(&out) {
                        self.predictions.push_back(Prediction {
                            target: self.real + ahead as u64,
                            hash,
                            input,
                            from: self.real,
                        });
                    }
                    check_ms += ms(t);
                }
                let t = Instant::now();
                keep(
                    out,
                    Keep {
                        video: s.video,
                        audio: s.audio,
                        real: false,
                    },
                );
                host_ms += ms(t);
            }
        }
        let t = Instant::now();
        let ok = m.load(&self.state);
        let load_ms = t.elapsed().as_secs_f32() * 1000.0;
        if !ok {
            self.switch_off(Off::StateFailed, "retro_unserialize failed".into());
            return true;
        }
        let (size_calls, size_ms) = m.size_queries();
        let st = &mut self.stats;
        st.real_ms += real_ms;
        st.ahead_ms += ahead_ms;
        st.check_ms += check_ms;
        st.host_ms += host_ms;
        st.size_calls += size_calls as u64;
        st.size_ms += size_ms as f64;
        self.account(t0.elapsed().as_secs_f32() * 1000.0, (thread_cpu_ms() - c0) as f32, save_ms, load_ms, period_ms);
        // The end of this refresh's run: the next one starts about a refresh later, unless something came between.
        self.last_step = Some(Instant::now());
        true
    }

    /// Timing and the comparisons of one refresh; switches off when a limit is passed.
    fn account(&mut self, total_ms: f32, cpu_ms: f32, save_ms: f32, load_ms: f32, period_ms: f32) {
        let share = self.sample_clock(self.refreshes);
        self.refreshes += 1;
        let st = &mut self.stats;
        if let Some(s) = share {
            st.clock_sum += s as f64;
            st.clock_n += 1;
        }
        st.refreshes += 1;
        st.total_ms += total_ms as f64;
        st.cpu_ms += cpu_ms as f64;
        st.max_ms = st.max_ms.max(total_ms);
        st.save_ms += save_ms as f64;
        st.load_ms += load_ms as f64;
        st.state_bytes = self.state.len();
        let differed = self.recent.iter().filter(|d| **d).count();
        if differed >= MISMATCH_LIMIT && differed * 4 >= self.recent.len() {
            let detail = format!(
                "{differed} of the last {} compared frames differed ({} of {} in all)",
                self.recent.len(),
                self.different,
                self.different + self.same
            );
            self.switch_off(Off::NotExact, detail);
            return;
        }
        if self.refreshes <= self.settle_until {
            return;
        }
        if self.serialize_n < SERIALIZE_SAMPLES {
            self.serialize_ms += (save_ms + load_ms) as f64;
            self.serialize_n += 1;
            let mean = self.serialize_ms / self.serialize_n as f64;
            if self.serialize_n == SERIALIZE_SAMPLES && mean > (period_ms * SERIALIZE_SHARE) as f64 {
                self.switch_off(
                    Off::SlowStates,
                    format!("save + load {mean:.2} ms on average, {:.2} ms allowed", period_ms * SERIALIZE_SHARE),
                );
                return;
            }
        }
        let over = cpu_ms > period_ms * BUDGET_SHARE;
        if over {
            self.stats.over += 1;
        }
        self.over.push_back(over);
        if self.over.len() > WINDOW {
            self.over.pop_front();
        }
        let n = self.over.iter().filter(|o| **o).count();
        if n > OVER_LIMIT {
            self.switch_off(
                Off::TooSlow,
                format!(
                    "{n} of the last {} refreshes took over {:.1} ms of CPU (this one {cpu_ms:.1} ms, {})",
                    self.over.len(),
                    period_ms * BUDGET_SHARE,
                    clock_text(self.clock)
                ),
            );
        }
    }

    /// What a save plus a load cost this core as run-ahead measured them (the refreshes it judged, up to
    /// [`SERIALIZE_SAMPLES`]); None before it measured any. Earlier's cost guard reads it (`timeline::Timeline::moment`).
    pub fn serialize_ms(&self) -> Option<f32> {
        (self.serialize_n > 0).then(|| (self.serialize_ms / f64::from(self.serialize_n)) as f32)
    }

    /// A line for the exit log: what run-ahead cost, and how the comparisons went.
    pub fn report(&self) -> Option<String> {
        let s = &self.stats;
        if s.refreshes == 0 && self.log.is_none() {
            return None;
        }
        let n = s.refreshes.max(1) as f64;
        let mut line = format!(
            "run-ahead {}: {} refreshes, {:.2} ms each (CPU {:.2} ms, max {:.1}; save {:.3} ms, load {:.3} ms, state {} KB), over budget {}, compared {} same / {} different",
            self.want,
            s.refreshes,
            s.total_ms / n,
            s.cpu_ms / n,
            s.max_ms,
            s.save_ms / n,
            s.load_ms / n,
            s.state_bytes / 1024,
            s.over,
            self.same,
            self.different
        );
        let parts = s.real_ms + s.ahead_ms + s.check_ms + s.host_ms + s.save_ms + s.load_ms;
        line.push_str(&format!(
            "; per refresh: real frames {:.2}, ahead {:.2}, check {:.3}, host {:.3}, save {:.3} (state size asked {}x, {:.3} ms in all), load {:.3}, other {:.3} ms",
            s.real_ms / n,
            s.ahead_ms / n,
            s.check_ms / n,
            s.host_ms / n,
            s.save_ms / n,
            s.size_calls,
            s.size_ms,
            s.load_ms / n,
            (s.total_ms - parts).max(0.0) / n
        ));
        if s.clock_n > 0 {
            line.push_str(&format!(", clock {:.0} % of the highest on average", s.clock_sum / s.clock_n as f64 * 100.0));
        }
        if let Some(why) = &self.log {
            line.push_str(&format!("; {why}"));
        }
        Some(line)
    }

    /// A line for the exit log: the refreshes run without run-ahead, the baseline (None when there were none).
    pub fn report_plain(&self) -> Option<String> {
        let p = &self.plain;
        if p.refreshes == 0 {
            return None;
        }
        let mut line = format!(
            "without run-ahead: {} refreshes, CPU {:.2} ms each (max {:.1})",
            p.refreshes,
            p.cpu_ms / p.refreshes as f64,
            p.max_ms
        );
        if p.clock_n > 0 {
            line.push_str(&format!(
                ", clock {:.0} % of the highest on average, {}",
                p.clock_sum / p.clock_n as f64 * 100.0,
                clock_text(self.clock)
            ));
        }
        Some(line)
    }
}

fn clock_text(c: Option<Clock>) -> String {
    c.map_or("clock unknown".into(), |c| format!("cpu{} at {} of {} MHz", c.cpu, c.cur_mhz, c.max_mhz))
}

/// CPU time the calling thread has used, in milliseconds.
pub fn thread_cpu_ms() -> f64 {
    let mut ts = libc::timespec { tv_sec: 0, tv_nsec: 0 };
    // SAFETY: clock_gettime only writes the timespec it is given.
    if unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &mut ts) } != 0 {
        return 0.0;
    }
    ts.tv_sec as f64 * 1000.0 + ts.tv_nsec as f64 / 1e6
}

/// A fingerprint of what the game reads this frame: buttons, sticks and the touch screen.
pub fn input_key(buttons: u16, analog: [i16; 4], pointer: Option<(i16, i16)>) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    let mut eat = |v: u64| h = (h ^ v).wrapping_mul(0x100000001b3);
    eat(buttons as u64);
    for a in analog {
        eat(a as u16 as u64);
    }
    match pointer {
        Some((x, y)) => eat(1 << 32 | (x as u16 as u64) << 16 | y as u16 as u64),
        None => eat(0),
    }
    h
}

/// What RetroArch's info file next to the core (`gambatte_libretro.info`) says of its states:
/// `savestate_features = "deterministic"` is Some(true), any other value Some(false), no file or no line None.
pub fn core_info_deterministic(core: &Path) -> Option<bool> {
    let text = std::fs::read_to_string(core.with_extension("info")).ok()?;
    info_deterministic(&text)
}

fn info_deterministic(text: &str) -> Option<bool> {
    text.lines().find_map(|l| {
        let (k, v) = l.split_once('=')?;
        (k.trim() == "savestate_features").then(|| v.trim().trim_matches('"') == "deterministic")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::libretro::AV_ALL;
    use crate::testutil::TestDir;

    /// A tiny machine: a counter that every frame adds its buttons to (+1), the picture is the counter. The
    /// state is the counter. `leak` adds something to the picture that its states don't hold, like a core
    /// whose state is incomplete.
    #[derive(Default)]
    struct Stub {
        n: u64,
        leak: u64,
        leaky: bool,
        /// CPU time every frame takes (busy, not asleep).
        spin_us: u64,
        fail_save: bool,
        /// Every frame run: (buttons, enable).
        log: Vec<(u16, u32)>,
        saves: u32,
        loads: u32,
    }

    #[derive(Debug, PartialEq)]
    struct Out {
        picture: Option<u64>,
        sound: Option<u64>,
    }

    impl Machine for Stub {
        type Out = Out;
        fn run(&mut self, buttons: u16, enable: u32) -> Out {
            let t = Instant::now();
            while t.elapsed().as_micros() < self.spin_us as u128 {
                std::hint::spin_loop();
            }
            self.n += 1 + buttons as u64;
            if self.leaky {
                self.leak += 1;
            }
            self.log.push((buttons, enable));
            let shown = self.n * 1000 + self.leak;
            Out {
                picture: (enable & AV_VIDEO != 0).then_some(shown),
                sound: (enable & AV_AUDIO != 0).then_some(self.n),
            }
        }
        fn save(&mut self, buf: &mut Vec<u8>) -> bool {
            self.saves += 1;
            buf.clear();
            buf.extend_from_slice(&self.n.to_le_bytes());
            !self.fail_save
        }
        fn load(&mut self, buf: &[u8]) -> bool {
            self.loads += 1;
            self.n = u64::from_le_bytes(buf.try_into().unwrap());
            true
        }
        fn picture(out: &Out) -> Option<u64> {
            out.picture
        }
    }

    fn on(frames: u32) -> RunAhead {
        let mut ra = RunAhead::new();
        ra.set_core(Path::new("/nonexistent/stub_libretro.so"), "gbc");
        // No cpufreq: the time counts as it is (a fake sysfs where a test needs a clock).
        ra.clock_root = PathBuf::from("/nonexistent");
        ra.configure(frames);
        ra.admit(true, false, 0);
        ra
    }

    /// One refresh: what was shown, what was heard, and how many real frames ran.
    fn refresh(ra: &mut RunAhead, m: &mut Stub, real: u32, buttons: u16) -> (Option<u64>, Vec<u64>, u32) {
        let (mut shown, mut heard, mut reals) = (None, Vec::new(), 0);
        let ran = ra.step(m, real, buttons, buttons as u64, 16.7, |out, k| {
            if k.video {
                shown = out.picture;
            }
            if k.audio {
                heard.extend(out.sound);
            }
            reals += k.real as u32;
        });
        assert!(ran || real == 0);
        (shown, heard, reals)
    }

    #[test]
    fn the_plan_shows_the_last_frame_and_hears_one_refresh_of_sound() {
        let p = |real, ahead| plan(real, ahead).iter().map(|s| (s.real, s.video, s.audio)).collect::<Vec<_>>();
        assert_eq!(p(1, 1), [(true, false, false), (false, true, true)]);
        assert_eq!(p(1, 2), [(true, false, false), (false, false, false), (false, true, true)]);
        // A 50 Hz game on a 60 Hz screen: two real frames in one refresh, two frames of sound.
        assert_eq!(p(2, 1), [(true, false, false), (true, false, true), (false, true, true)]);
        assert_eq!(p(0, 2), []);
        for (real, ahead) in [(1, 1), (1, 2), (2, 1), (2, 2), (3, 1)] {
            let plan = plan(real, ahead);
            assert_eq!(
                plan.iter().filter(|s| s.audio).count() as u32,
                real,
                "{real}+{ahead}: as much sound as frames due"
            );
            assert_eq!(plan.iter().filter(|s| s.video).count(), 1);
            assert_eq!(plan.iter().filter(|s| s.real).count() as u32, real);
        }
    }

    #[test]
    fn the_picture_is_ahead_and_the_game_goes_on_from_the_real_frame() {
        for ahead in 1..=2u64 {
            let mut ra = on(ahead as u32);
            let mut m = Stub::default();
            let mut heard = Vec::new();
            for f in 1..=10u64 {
                let (shown, sound, reals) = refresh(&mut ra, &mut m, 1, 0);
                assert_eq!(reals, 1);
                assert_eq!(
                    shown,
                    Some((f + ahead) * 1000),
                    "{ahead} ahead, refresh {f}: the picture of frame {}",
                    f + ahead
                );
                assert_eq!(m.n, f, "the game itself is at frame {f}");
                heard.extend(sound);
            }
            // Every frame heard exactly once, in order, never more or less than one per refresh: the sound never drifts.
            assert_eq!(heard, (1 + ahead..=10 + ahead).collect::<Vec<_>>(), "{ahead} ahead");
            assert_eq!((m.saves, m.loads), (11, 10), "a trial save first, then one save and one load per refresh");
        }
    }

    #[test]
    fn a_press_shows_up_frames_sooner() {
        // Button 1 adds 1 more per frame. Without run-ahead the first frame with it pressed shows 2 more than before.
        let mut ra = on(2);
        let mut m = Stub::default();
        refresh(&mut ra, &mut m, 1, 0);
        let (shown, ..) = refresh(&mut ra, &mut m, 1, 1);
        // Real frame 2 with the press (n = 1 + 2 = 3), then two frames ahead with it (5, 7): shown 7, where the
        // real game shows 7 only two refreshes later.
        assert_eq!(shown, Some(7000));
        assert_eq!(m.n, 3);
        assert_eq!(m.log.iter().filter(|(b, _)| *b == 1).count(), 3, "the frames ahead get the buttons held now");
    }

    #[test]
    fn frames_nobody_sees_or_hears_are_run_without_them() {
        let mut ra = on(2);
        let mut m = Stub::default();
        ra.real = CHECK_FIRST; // outside a check window: no real frame needs its picture
        ra.step(&mut m, 1, 0, 0, 16.7, |_, _| {});
        assert_eq!(m.log.iter().map(|(_, e)| *e).collect::<Vec<_>>(), [0, 0, AV_ALL], "real, ahead, ahead (shown)");
    }

    #[test]
    fn nothing_due_runs_nothing_and_two_due_run_two_real_frames() {
        let mut ra = on(1);
        let mut m = Stub::default();
        assert!(!ra.step(&mut m, 0, 0, 0, 20.0, |_, _| panic!("nothing runs")));
        let (shown, heard, reals) = refresh(&mut ra, &mut m, 2, 0);
        assert_eq!((shown, heard, reals, m.n), (Some(3000), vec![2, 3], 2, 2));
    }

    #[test]
    fn off_runs_nothing_and_the_caller_runs_as_usual() {
        let mut ra = on(0);
        let mut m = Stub::default();
        assert_eq!(ra.frames(), 0);
        assert!(!ra.step(&mut m, 1, 0, 0, 16.7, |_, _| panic!()));
        assert!(m.log.is_empty() && m.saves == 0);
    }

    #[test]
    fn an_exact_game_passes_its_checks() {
        let mut ra = on(1);
        let mut m = Stub::default();
        for f in 0..300 {
            // Input changes now and then; the checks only count where it stayed the same.
            refresh(&mut ra, &mut m, 1, (f / 20 % 2) as u16);
        }
        assert!(ra.same > 100 / CHECK_STRIDE as u32, "{} compared", ra.same);
        assert_eq!((ra.different, ra.off()), (0, None));
    }

    #[test]
    fn a_game_that_doesnt_replay_exactly_switches_it_off() {
        let mut ra = on(1);
        let mut m = Stub {
            leaky: true,
            ..Default::default()
        };
        let mut n = 0;
        while ra.off().is_none() && n < 40 {
            refresh(&mut ra, &mut m, 1, 0);
            n += 1;
        }
        // Compared every `CHECK_STRIDE`th frame; the third difference switches it off.
        assert_eq!(
            (ra.off(), n as u64, ra.different as usize),
            (Some(Off::NotExact), MISMATCH_LIMIT as u64 * CHECK_STRIDE + 1, MISMATCH_LIMIT),
            "made at frames 4, 8, 12, compared one later"
        );
        assert_eq!(ra.take_notice(), Some(Off::NotExact.notice()));
        assert_eq!(ra.take_notice(), None, "once");
        assert_eq!(ra.frames(), 0);
        let before = m.log.len();
        assert!(!ra.step(&mut m, 1, 0, 0, 16.7, |_, _| {}), "the caller runs the frames now");
        assert_eq!(m.log.len(), before);
        // Choosing another value tries again.
        ra.configure(2);
        ra.admit(true, false, 0);
        assert_eq!(ra.frames(), 2);
    }

    #[test]
    fn a_rare_difference_is_no_proof() {
        // A stale picture now and then (a Game Boy screen switched off): every 20th compared frame differs.
        let mut ra = on(1);
        let mut m = Stub::default();
        for f in 0..CHECK_FIRST {
            m.leak = f / 20;
            refresh(&mut ra, &mut m, 1, 0);
        }
        assert!(ra.different >= MISMATCH_LIMIT as u32, "{} differed", ra.different);
        assert_eq!(ra.off(), None);
    }

    #[test]
    fn changed_buttons_are_not_counted_as_a_difference() {
        let mut ra = on(2);
        let mut m = Stub::default();
        for f in 0..200u16 {
            // A different button every other frame: the frames ahead guessed wrong, which is no fault.
            refresh(&mut ra, &mut m, 1, f % 2);
        }
        assert_eq!((ra.same, ra.different, ra.off()), (0, 0, None));
    }

    #[test]
    fn forget_drops_the_pictures_ahead() {
        let mut ra = on(1);
        let mut m = Stub::default();
        refresh(&mut ra, &mut m, 1, 0);
        // A state was loaded from outside: the real game jumped.
        m.n += 50;
        ra.forget();
        for _ in 0..5 {
            refresh(&mut ra, &mut m, 1, 0);
        }
        assert_eq!((ra.different, ra.off()), (0, None));
    }

    /// The timing rules, with the times of each refresh given (`account`), so a busy test machine can't decide.
    #[test]
    fn slow_states_switch_it_off() {
        let mut ra = on(1);
        let mut n = 0;
        // 5 ms to save and load against 4.2 ms allowed at 60 Hz; the whole refresh (6 ms) is within its budget.
        while ra.off().is_none() && n < 500 {
            ra.account(6.0, 6.0, 3.0, 2.0, 16.7);
            n += 1;
        }
        assert_eq!(ra.off(), Some(Off::SlowStates));
        assert_eq!(n, SETTLE_START + SERIALIZE_SAMPLES as u64, "judged after the start settled and its samples");
        let mut ra = on(1);
        for _ in 0..1000 {
            ra.account(6.0, 6.0, 1.5, 1.5, 16.7);
        }
        assert_eq!(ra.off(), None, "3 ms is fine");
    }

    /// The device's first seconds (F-Zero, mGBA, 2026-10-23): a low clock, the resume state, cold caches. Not judged.
    #[test]
    fn the_start_and_events_are_not_judged() {
        let mut ra = on(1);
        for _ in 0..SETTLE_START {
            ra.account(12.0, 12.0, 0.5, 0.5, 16.7);
        }
        assert_eq!(ra.off(), None, "a slow start");
        for _ in 0..200 {
            ra.account(2.0, 2.0, 0.1, 0.1, 16.7);
        }
        // A load (or an option, the pause): the next second isn't judged either.
        ra.forget();
        for _ in 0..SETTLE_EVENT {
            ra.account(12.0, 12.0, 0.1, 0.1, 16.7);
        }
        assert_eq!(ra.off(), None, "after an event");
        for _ in 0..OVER_LIMIT + 1 {
            ra.account(12.0, 12.0, 0.1, 0.1, 16.7);
        }
        assert_eq!(ra.off(), Some(Off::TooSlow), "and then it is");
        // Hard failures don't wait: pictures that don't match switch it off during the start.
        let mut ra = on(1);
        let mut m = Stub {
            leaky: true,
            ..Default::default()
        };
        for _ in 0..40 {
            ra.step(&mut m, 1, 0, 0, 16.7, |_, _| {});
        }
        assert_eq!(ra.off(), Some(Off::NotExact));
        assert!(ra.refreshes < SETTLE_START);
    }

    #[test]
    fn a_gap_between_refreshes_settles_again() {
        let mut ra = on(1);
        let mut m = Stub::default();
        // Past the start (as if 3 s had gone by).
        ra.refreshes = SETTLE_START + 10;
        ra.settle_until = 0;
        ra.step(&mut m, 1, 0, 0, 1.0, |_, _| {});
        assert_eq!(ra.settle_until, 0, "refreshes in a row");
        std::thread::sleep(std::time::Duration::from_millis(10));
        ra.step(&mut m, 1, 0, 0, 1.0, |_, _| {});
        assert!(ra.settle_until >= ra.refreshes + SETTLE_EVENT - 2, "fast-forward or the menu came in between");
    }

    /// A fake sysfs: cpu0 at `cur` of `max` MHz.
    fn clock_root(name: &str, cur: u32, max: u32) -> crate::testutil::TestDir {
        let root = TestDir::new(&format!("runahead-clock-{name}"));
        let dir = root.join("sys/devices/system/cpu/cpu0/cpufreq");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("scaling_cur_freq"), format!("{}\n", cur * 1000)).unwrap();
        std::fs::write(dir.join("scaling_max_freq"), format!("{}\n", max * 1000)).unwrap();
        root
    }

    #[test]
    fn the_clock_is_read_from_cpufreq() {
        let root = clock_root("read", 691, 1804);
        let c = Clock::read_cpu(&root, 0).unwrap();
        assert_eq!(
            c,
            Clock {
                cpu: 0,
                cur_mhz: 691,
                max_mhz: 1804
            }
        );
        assert!((c.share() - 0.383).abs() < 0.01);
        assert_eq!(Clock::read_cpu(&root, 1), None, "no such CPU");
    }

    /// The budget is CPU time at the clock the refresh ran at, whatever the clock read says (Mini V2, F-Zero, mGBA:
    /// 7.2 ms a refresh at 1 frame, 8.4 ms at 2, with a clock read now and then on another cluster).
    #[test]
    fn the_budget_is_cpu_time_at_the_clock_it_ran_at() {
        let judge = |cpu_ms: f32, cur: u32, max: u32| {
            let root = clock_root(&format!("{cpu_ms}-{cur}-{max}"), cur, max);
            let mut ra = on(1);
            ra.clock = Clock::read_cpu(&root, 0);
            ra.settle_until = 0;
            for n in 0..WINDOW as u64 {
                // Keep the sample (a container has no cpu0 to read again).
                ra.refreshes = n * CLOCK_EVERY + 1;
                ra.account(cpu_ms, cpu_ms, 0.2, 0.2, 16.7);
            }
            ra.off()
        };
        assert_eq!(judge(7.2, 691, 1804), None);
        assert_eq!(judge(9.3, 2419, 2419), None, "9.3 ms at full clock still leaves 7 ms for drawing and the swap");
        assert_eq!(judge(11.5, 400, 1804), None);
        assert_eq!(judge(12.0, 2841, 2841), Some(Off::TooSlow), "over 70 % of a refresh");
        assert_eq!(judge(12.0, 400, 1804), Some(Off::TooSlow), "the clock doesn't excuse it");
    }

    #[test]
    fn the_baseline_is_counted_and_reported() {
        let mut ra = on(0);
        assert_eq!(ra.report_plain(), None);
        ra.note_plain(2.0);
        ra.note_plain(4.0);
        let line = ra.report_plain().unwrap();
        assert!(line.starts_with("without run-ahead: 2 refreshes, CPU 3.00 ms each (max 4.0)"), "{line}");
    }

    #[test]
    fn going_over_the_budget_too_often_switches_it_off() {
        let mut ra = on(1);
        // Quick states, but every 8th refresh takes 12 ms (over 11.7): 15 in 120, over the limit of 12.
        let mut n = 0u64;
        while ra.off().is_none() && n < 1000 {
            ra.account(
                if n.is_multiple_of(8) { 12.0 } else { 2.0 },
                if n.is_multiple_of(8) { 12.0 } else { 2.0 },
                0.1,
                0.1,
                16.7,
            );
            n += 1;
        }
        assert_eq!(ra.off(), Some(Off::TooSlow));
        assert!(n > SETTLE_START + OVER_LIMIT as u64 && n <= SETTLE_START + WINDOW as u64, "{n}");
        // Now and then over (a hiccup of the system) is fine: every 12th, 10 in 120.
        let mut ra = on(1);
        for n in 0..2000 {
            ra.account(if n % 12 == 0 { 30.0 } else { 2.0 }, if n % 12 == 0 { 30.0 } else { 2.0 }, 0.1, 0.1, 16.7);
        }
        assert_eq!(ra.off(), None);
        // And an end-to-end run: 1 ms of CPU a frame, three frames a refresh, against 1 ms (a 2 ms refresh).
        let mut ra = on(2);
        let mut m = Stub {
            spin_us: 1000,
            ..Default::default()
        };
        let mut n = 0;
        while ra.off().is_none() && n < 500 {
            ra.step(&mut m, 1, 0, 0, 2.0, |_, _| {});
            n += 1;
        }
        assert_eq!(ra.off(), Some(Off::TooSlow));
    }

    #[test]
    fn a_core_that_cannot_save_is_switched_off_with_the_picture_kept() {
        let mut ra = on(1);
        let mut m = Stub {
            fail_save: true,
            ..Default::default()
        };
        assert!(!ra.step(&mut m, 1, 0, 0, 16.7, |_, _| panic!("nothing runs yet")));
        assert_eq!(ra.off(), Some(Off::StateFailed));
        // One that must run a while first (QUIRK_MUST_INITIALIZE) is given time.
        let mut ra = on(0);
        ra.configure(1);
        ra.admit(true, false, QUIRK_MUST_INITIALIZE);
        let mut m = Stub {
            fail_save: true,
            ..Default::default()
        };
        for _ in 0..10 {
            assert!(!ra.step(&mut m, 1, 0, 0, 16.7, |_, _| {}));
        }
        assert_eq!(ra.off(), None);
        m.fail_save = false;
        assert!(ra.step(&mut m, 1, 0, 0, 16.7, |_, _| {}));
    }

    #[test]
    fn unsuited_systems_and_cores_are_refused() {
        let check = |system: &str, has_states: bool, hw: bool, quirks: u64| {
            let mut ra = RunAhead::new();
            ra.set_core(Path::new("/nonexistent/x_libretro.so"), system);
            ra.configure(1);
            assert!(ra.needs_admit());
            ra.admit(has_states, hw, quirks);
            assert!(!ra.needs_admit());
            ra.off()
        };
        assert_eq!(check("snes", true, false, 0), None);
        assert_eq!(check("n64", true, true, 0), Some(Off::Unsuited));
        assert_eq!(check("psx", true, false, 0), Some(Off::Unsuited), "not offered");
        assert_eq!(check("gba", false, false, 0), Some(Off::Unsuited), "no states");
        assert_eq!(check("gba", true, false, QUIRK_INCOMPLETE), Some(Off::Unsuited));
        assert_eq!(check("gba", true, false, QUIRK_MUST_INITIALIZE), None);
    }

    #[test]
    fn retroarch_info_files_are_read() {
        assert_eq!(
            info_deterministic("display_name = \"x\"\nsavestate = \"true\"\nsavestate_features = \"deterministic\"\n"),
            Some(true)
        );
        assert_eq!(info_deterministic("savestate_features = \"serialized\""), Some(false));
        assert_eq!(info_deterministic("savestate_features=basic"), Some(false));
        assert_eq!(info_deterministic("savestate = \"true\""), None);
        let dir = TestDir::new("runahead-info");
        std::fs::write(dir.join("handy_libretro.info"), "savestate_features = \"serialized\"\n").unwrap();
        let mut ra = RunAhead::new();
        ra.set_core(&dir.join("handy_libretro.so"), "gba");
        ra.configure(1);
        ra.admit(true, false, 0);
        assert_eq!(ra.off(), Some(Off::Unsuited), "RetroArch's rule: deterministic states only");
    }

    #[test]
    fn input_keys_tell_sticks_and_touch_apart() {
        let k = input_key(0, [0; 4], None);
        assert_ne!(k, input_key(1, [0; 4], None));
        assert_ne!(k, input_key(0, [0, 1, 0, 0], None));
        assert_ne!(k, input_key(0, [0; 4], Some((0, 0))));
        assert_eq!(k, input_key(0, [0; 4], None));
    }

    #[test]
    fn the_settings_choose_and_clamp() {
        let mut ra = on(1);
        assert_eq!(ra.frames(), 1);
        ra.configure(7);
        ra.admit(true, false, 0);
        assert_eq!(ra.frames(), MAX_FRAMES);
        ra.configure(0);
        assert_eq!((ra.frames(), ra.needs_admit()), (0, false));
    }
}
