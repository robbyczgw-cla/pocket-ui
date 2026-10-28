//! Earlier: pocket-core's rolling timeline of save states (muOS calls them timeline saves). Options → Extras → Earlier
//! states switches it; the pause menu's Earlier row loads one (docs/user/pause-menu.md, "Earlier").
//!
//! - **Every 5 minutes of play** ([`EVERY`]) a state goes into a ring of the last [`KEEP`] (six: half an hour back),
//!   the oldest replaced. Only play counts ([`Clock`]): the pause menu, sleep and a stalled refresh don't.
//! - **Where a session began**: a session that continues from Auto keeps that state too, one per session, the last
//!   [`STARTS`] of them. The writer thread copies the file the start loaded; nothing is serialized for it.
//! - **Never a stalled frame.** A core's state can only be taken on the emulation thread, so the serialize runs there;
//!   the picture's PNG, the file and the caps are the writer thread's ([`Writer`]). A core whose serialize costs more
//!   than a quarter of a refresh (run-ahead's limit, [`crate::run_ahead::SERIALIZE_SHARE`]), and every
//!   hardware-rendered one (its picture would be read back from the GPU), is not captured in play: its state is taken
//!   at the next quiet moment ([`Moment::Quiet`]: a second into the pause menu, or the resume state a sleep writes
//!   anyway, copied). [`Timeline::moment`] decides from what was measured: Earlier's own serialize in this session,
//!   else run-ahead's or rewind's, else the state's size ([`SMALL_STATE`]).
//! - **Room**: at most [`GAME_CAP`] per game and [`ALL_CAP`] on the card, the oldest out first ([`enforce`]); nothing
//!   is written while the card has less than [`crate::storage::LOW`] free. Storage's clean-up of old automatic states
//!   takes them too (`storage::auto_state_base`), and Sync leaves them out (the states folder's `.stignore`, `sync`).
//!
//! Files, beside the game's other states (`slots`), each with its picture (`<file>.png`):
//!
//! | File | What |
//! |---|---|
//! | `<game>.pstate.earlier1` … `.earlier6` | the ring; which is newest is the file's time |
//! | `<game>.pstate.started1` … `.started3` | where the last sessions began |
//!
//! Systems with big states ([`BIG_STATES`]) start with it Off (`options::console_default`).

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::libretro::Frame;

/// The game setting (Options → Extras → Earlier states): "on" or "off".
pub const KEY: &str = "timeline";
/// Play between two states of the ring.
pub const EVERY: Duration = Duration::from_secs(5 * 60);
/// States in the ring: half an hour back.
pub const KEEP: u8 = 6;
/// Session starts kept.
pub const STARTS: u8 = 3;
/// Earlier states of one game, pictures included, at most (powers of 1000, as Storage counts).
pub const GAME_CAP: u64 = 100_000_000;
/// Earlier states of all games on the card, at most.
pub const ALL_CAP: u64 = 1_000_000_000;
/// The most play one refresh can add: a stalled refresh or a suspend pocket-core wasn't told of is not play.
pub const MAX_GAP: Duration = Duration::from_millis(250);
/// A state this small is taken in play before its serialize was ever measured.
pub const SMALL_STATE: usize = 2_000_000;
/// How long the pause menu is open before a quiet capture: it has listed the states by then (`QuickMenu::earlier`).
pub const QUIET_AFTER: Duration = Duration::from_secs(1);

/// Systems whose states are several MB to tens of MB (docs/HOST.md "Earlier"): Earlier is Off for them until it is
/// switched on for the console or the game. Their states would fill the 100 MB of a game with a handful.
pub const BIG_STATES: &[&str] = &["n64", "psp", "dreamcast", "naomi", "atomiswave", "saturn", "nds"];

/// Whether Earlier starts On for `system`.
pub fn default_on(system: &str) -> bool {
    !BIG_STATES.contains(&system)
}

/// Which kind of earlier state.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Kind {
    /// The ring: every 5 minutes of play.
    Every,
    /// Where a session began.
    Started,
}

impl Kind {
    fn word(self) -> &'static str {
        match self {
            Kind::Every => "earlier",
            Kind::Started => "started",
        }
    }

    fn count(self) -> u8 {
        match self {
            Kind::Every => KEEP,
            Kind::Started => STARTS,
        }
    }
}

/// One of the files: `earlier3`, `started1` (counted from 1). This is also how pocket-ui names it to pocket-core
/// (`load earlier3 <time>`).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub struct Name {
    pub kind: Kind,
    pub n: u8,
}

impl Name {
    /// Every file there can be, the ring first.
    pub fn all() -> impl Iterator<Item = Name> {
        [Kind::Every, Kind::Started].into_iter().flat_map(|kind| (1..=kind.count()).map(move |n| Name { kind, n }))
    }

    /// `earlier3`.
    pub fn wire(self) -> String {
        format!("{}{}", self.kind.word(), self.n)
    }

    /// `earlier3` and the like; None for anything else (`earlier7`, `earlier`, `slot3`).
    pub fn parse(s: &str) -> Option<Name> {
        Name::all().find(|n| n.wire() == s)
    }
}

/// Where `name` of the game whose states start with `base` is.
pub fn path(base: &Path, name: Name) -> PathBuf {
    let mut p = base.as_os_str().to_owned();
    p.push(format!(".pstate.{}", name.wire()));
    PathBuf::from(p)
}

/// An earlier state on the card.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub name: Name,
    pub path: PathBuf,
    /// When it was written (for a session start: when the session began).
    pub at: SystemTime,
    pub bytes: u64,
}

impl Entry {
    /// Unix seconds of `at`.
    pub fn secs(&self) -> u64 {
        self.at.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
    }

    /// An empty file (a card that filled up as it was written): listed, but not loadable.
    pub fn damaged(&self) -> bool {
        self.bytes == 0
    }
}

/// The earlier states of a game, newest first. A few `stat`s of known names, no directory listing: cheap enough for
/// the pause menu as it opens.
pub fn entries(base: &Path) -> Vec<Entry> {
    let mut v: Vec<Entry> = Name::all().filter_map(|name| entry(base, name)).collect();
    v.sort_by(|a, b| b.at.cmp(&a.at).then(a.name.n.cmp(&b.name.n)));
    v
}

/// One of them, if it is there.
pub fn entry(base: &Path, name: Name) -> Option<Entry> {
    let path = path(base, name);
    let m = std::fs::symlink_metadata(&path).ok().filter(|m| m.is_file())?;
    Some(Entry { name, at: m.modified().unwrap_or(UNIX_EPOCH), bytes: m.len(), path })
}

/// What `load earlier3 <time>` loads: the entry, or why not (pocket-core's toast). `listed`: the file's time as the
/// pause menu listed it; a state replaced since (by a pause's own state, `QUIET_AFTER`) is not loaded in its place.
pub fn to_load(base: &Path, wire: &str, listed: Option<u64>) -> Result<Entry, &'static str> {
    let name = Name::parse(wire).ok_or("That earlier state is gone")?;
    let e = entry(base, name).ok_or("That earlier state is gone")?;
    if listed.is_some_and(|t| t != e.secs()) {
        return Err("That earlier state was just replaced. Pick it again.");
    }
    if e.damaged() {
        return Err("That earlier state is damaged");
    }
    Ok(e)
}

/// The game an earlier state's file (or its picture) belongs to: the name before `.pstate.earlier3`; None for any
/// other file (a slot, a temporary file of a write under way).
pub fn base_of(file_name: &str) -> Option<&str> {
    let name = file_name.strip_suffix(".png").unwrap_or(file_name);
    Name::all().find_map(|n| name.strip_suffix(&format!(".pstate.{}", n.wire()))).filter(|b| !b.is_empty())
}

/// The file the next state of `kind` goes into: the first that isn't there, else the oldest.
fn next_name(base: &Path, kind: Kind) -> Name {
    let names = (1..=kind.count()).map(|n| Name { kind, n });
    let mut oldest: Option<(SystemTime, Name)> = None;
    for name in names {
        match entry(base, name) {
            None => return name,
            Some(e) if oldest.is_none_or(|(t, _)| e.at < t) => oldest = Some((e.at, name)),
            Some(_) => {}
        }
    }
    oldest.map_or(Name { kind, n: 1 }, |(_, n)| n)
}

// --- what the menu says ------------------------------------------------------------------------------------------

/// "today 14:02", "yesterday 21:10", "3 days ago": when a session began, in the zone `tz` seconds east of UTC.
fn started_when(secs: u64, now: u64, tz: i32) -> (u64, String) {
    let days = crate::play_history::local_day(now, tz) - crate::play_history::local_day(secs, tz);
    let clock = crate::timezone::clock_at(secs as i64, tz);
    (days.max(0) as u64, clock)
}

/// The chip beside Earlier: when, short enough to fit beside the row in Large text and in German too. The ring: "12 min"
/// (earlier), "2 h", "just now", "yesterday", "3 days"; a session start: its clock time today ("14:02"), else
/// "yesterday", "3 days". The line under the picture says which kind it is (`note`).
pub fn chip(e: &Entry, now: u64, tz: i32) -> String {
    match e.name.kind {
        Kind::Every => match now.saturating_sub(e.secs()) {
            0..=59 => "just now".into(),
            d @ 60..=3599 => format!("{} min", d / 60),
            d @ 3600..=86_399 => format!("{} h", d / 3600),
            d if d < 2 * 86_400 => "yesterday".into(),
            d => format!("{} days", d / 86_400),
        },
        Kind::Started => match started_when(e.secs(), now, tz) {
            (0, clock) => clock,
            (1, _) => "yesterday".into(),
            (days, _) => format!("{days} days"),
        },
    }
}

/// The line under the picture; `None`: the game has none yet.
pub fn note(e: Option<&Entry>, now: u64, tz: i32) -> String {
    let Some(e) = e else { return "Earlier · none yet · one every 5 min of play".into() };
    if e.damaged() {
        let when = match e.name.kind {
            Kind::Every => crate::recents::ago_from(e.secs(), now),
            Kind::Started => chip(e, now, tz),
        };
        return format!("Earlier · {when} · damaged, can't be loaded");
    }
    match e.name.kind {
        Kind::Every => format!("Earlier · {}", crate::recents::ago_from(e.secs(), now)),
        Kind::Started => match started_when(e.secs(), now, tz) {
            (0, clock) => format!("Earlier · session start · today {clock}"),
            (1, clock) => format!("Earlier · session start · yesterday {clock}"),
            (days, _) => format!("Earlier · session start · {days} days ago"),
        },
    }
}

/// What pocket-core's toasts call it: "Loaded the state from 12 min ago", "Loaded the session start".
pub fn load_label(e: &Entry, now: u64) -> String {
    match e.name.kind {
        Kind::Every => format!("the state from {}", crate::recents::ago_from(e.secs(), now)),
        Kind::Started => "the session start".into(),
    }
}

/// The pause menu's Earlier row: what it lists, newest first, and which one Left/Right are on.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Row {
    pub entries: Vec<Entry>,
    pub at: usize,
    /// The zone's offset east of UTC as the menu opened, for a session start's "14:02" (not looked up every frame).
    pub tz: i32,
}

impl Row {
    /// On the newest.
    pub fn new(entries: Vec<Entry>, tz: i32) -> Row {
        Row { entries, at: 0, tz }
    }

    pub fn selected(&self) -> Option<&Entry> {
        self.entries.get(self.at)
    }

    /// Left (-1) and Right (1), around.
    pub fn step(&mut self, delta: i32) {
        if !self.entries.is_empty() {
            self.at = (self.at as i32 + delta).rem_euclid(self.entries.len() as i32) as usize;
        }
    }

    /// A on it loads something.
    pub fn loadable(&self) -> bool {
        self.selected().is_some_and(|e| !e.damaged())
    }

    /// What pocket-core is told: `load earlier3 <the file's time as listed>`.
    pub fn command(&self) -> Option<String> {
        self.selected().filter(|e| !e.damaged()).map(|e| format!("load {} {}", e.name.wire(), e.secs()))
    }
}

// --- play time and the cost guard --------------------------------------------------------------------------------

/// Play time: what the refreshes of play add up to.
#[derive(Default, Debug)]
pub struct Clock {
    played: Duration,
    last: Option<Instant>,
}

impl Clock {
    /// A refresh of play at `now`: the time since the one before counts, at most [`MAX_GAP`].
    pub fn tick(&mut self, now: Instant) {
        if let Some(last) = self.last {
            self.played += now.saturating_duration_since(last).min(MAX_GAP);
        }
        self.last = Some(now);
    }

    /// Play stops (the pause menu, sleep, Earlier switched off): what comes until the next refresh isn't play.
    pub fn stop(&mut self) {
        self.last = None;
    }

    pub fn played(&self) -> Duration {
        self.played
    }

    fn restart(&mut self) {
        self.played = Duration::ZERO;
    }
}

/// When a state for Earlier may be taken.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Moment {
    /// Between two refreshes of play: the serialize fits into what is left of the refresh.
    InPlay,
    /// Only while nothing is shown: the pause menu, or a sleep's resume state.
    Quiet,
}

/// The cost guard. `hw`: the core renders with the GPU; `measured_ms`: what a serialize of this core costs (measured in
/// this session); `state_bytes`: its state's size, asked only when nothing was measured; `period_ms`: one refresh.
pub fn moment(hw: bool, measured_ms: Option<f32>, state_bytes: Option<usize>, period_ms: f32) -> Moment {
    let budget = period_ms * crate::run_ahead::SERIALIZE_SHARE;
    let fits = match (measured_ms, state_bytes) {
        (Some(ms), _) => ms <= budget,
        (None, Some(n)) => n > 0 && n <= SMALL_STATE,
        (None, None) => false,
    };
    if fits && !hw {
        Moment::InPlay
    } else {
        Moment::Quiet
    }
}

/// What this session's captures cost, for pocket-core's exit report.
#[derive(Default, Debug, Clone)]
pub struct Stats {
    pub in_play: u32,
    pub quiet: u32,
    pub copies: u32,
    pub failed: u32,
    pub serialize_total_ms: f64,
    pub serialize_max_ms: f32,
    pub state_bytes: usize,
}

/// pocket-core's side of Earlier: play time, when to capture, and the writer.
pub struct Timeline {
    clock: Clock,
    /// Earlier's own last serialize in this session, ms.
    measured_ms: Option<f32>,
    /// The state's size, once asked (`moment`; mGBA makes a whole state to tell it).
    size: Option<Option<usize>>,
    caps: Caps,
    writer: Option<Writer>,
    pub stats: Stats,
}

impl Default for Timeline {
    fn default() -> Self {
        Timeline::new(Caps::default())
    }
}

impl Timeline {
    pub fn new(caps: Caps) -> Timeline {
        Timeline { clock: Clock::default(), measured_ms: None, size: None, caps, writer: None, stats: Stats::default() }
    }

    /// A refresh of play.
    pub fn play(&mut self, now: Instant) {
        self.clock.tick(now);
    }

    /// Play stops for a while (pause, sleep, switched off).
    pub fn stop(&mut self) {
        self.clock.stop();
    }

    /// 5 minutes of play since the last state.
    pub fn due(&self) -> bool {
        self.clock.played() >= EVERY
    }

    pub fn played(&self) -> Duration {
        self.clock.played()
    }

    /// When the next state may be taken (`moment`). `other_ms`: what run-ahead or rewind measured for a serialize;
    /// `size`: the state's size, asked once and only when nothing was measured.
    pub fn moment(&mut self, hw: bool, other_ms: Option<f32>, size: impl FnOnce() -> Option<usize>, period_ms: f32) -> Moment {
        let measured = self.measured_ms.or(other_ms);
        let bytes = if measured.is_none() { *self.size.get_or_insert_with(size) } else { None };
        moment(hw, measured, bytes, period_ms)
    }

    /// A state taken for the ring (`serialize_ms` of it on the emulation thread), handed to the writer with the
    /// picture it was taken at. The clock starts again.
    pub fn submit_state(&mut self, base: &Path, data: Vec<u8>, picture: Option<Frame>, serialize_ms: f32, at: Moment) {
        self.clock.restart();
        self.measured_ms = Some(serialize_ms);
        let s = &mut self.stats;
        match at {
            Moment::InPlay => s.in_play += 1,
            Moment::Quiet => s.quiet += 1,
        }
        s.serialize_total_ms += serialize_ms as f64;
        s.serialize_max_ms = s.serialize_max_ms.max(serialize_ms);
        s.state_bytes = data.len();
        let picture = picture.map_or(Picture::None, Picture::Frame);
        self.writer().submit(Job { base: base.to_path_buf(), kind: Kind::Every, source: Source::State(data), picture });
    }

    /// Nothing to take (a core without states): the next look in 5 minutes, nothing counted.
    pub fn skip(&mut self) {
        self.clock.restart();
    }

    /// The serialize failed: no state this time, the next one in 5 minutes.
    pub fn failed(&mut self) {
        self.clock.restart();
        self.stats.failed += 1;
    }

    /// A state file written anyway, copied by the writer with its picture: the resume state of a sleep (`Kind::Every`,
    /// the clock starts again) or the one a session continued from (`Kind::Started`).
    pub fn submit_copy(&mut self, base: &Path, kind: Kind, state: &Path, picture: &Path) {
        if kind == Kind::Every {
            self.clock.restart();
        }
        self.stats.copies += 1;
        let job = Job { base: base.to_path_buf(), kind, source: Source::Copy(state.to_path_buf()), picture: Picture::File(picture.to_path_buf()) };
        self.writer().submit(job);
    }

    fn writer(&mut self) -> &Writer {
        let caps = self.caps;
        self.writer.get_or_insert_with(|| Writer::start(caps))
    }

    /// Waits until the writer has done what it was given (tests, tools).
    pub fn wait(&self) {
        if let Some(w) = &self.writer {
            w.wait();
        }
    }

    /// What the writer did so far.
    pub fn written(&self) -> Written {
        self.writer.as_ref().map(Writer::written).unwrap_or_default()
    }

    /// A line for pocket-core's exit report; None when nothing was taken.
    pub fn report(&self) -> Option<String> {
        let s = &self.stats;
        let taken = s.in_play + s.quiet;
        if taken + s.copies + s.failed == 0 {
            return None;
        }
        let w = self.written();
        let mean = if taken > 0 { s.serialize_total_ms / taken as f64 } else { 0.0 };
        Some(format!(
            "earlier: {taken} states ({} in play, {} in a pause), {} copied, {} failed; serialize {mean:.2} ms (max {:.2}), {} KB; written {} in {:.0} ms each, {} skipped, {} removed by the caps",
            s.in_play,
            s.quiet,
            s.copies,
            s.failed,
            s.serialize_max_ms,
            s.state_bytes / 1024,
            w.written,
            if w.written > 0 { w.write_ms / w.written as f64 } else { 0.0 },
            w.skipped,
            w.removed
        ))
    }
}

// --- the writer --------------------------------------------------------------------------------------------------

/// How much room Earlier may take.
#[derive(Clone, Copy, Debug)]
pub struct Caps {
    pub game: u64,
    pub all: u64,
    /// Nothing is written while the card has less free (plus the state).
    pub min_free: u64,
}

impl Default for Caps {
    fn default() -> Self {
        Caps { game: GAME_CAP, all: ALL_CAP, min_free: crate::storage::LOW }
    }
}

/// Where a state comes from.
pub enum Source {
    /// Taken just now.
    State(Vec<u8>),
    /// A file written anyway (the sleep's resume state, the state a session continued from).
    Copy(PathBuf),
}

/// Its picture.
pub enum Picture {
    /// The last frame, encoded on the writer thread.
    Frame(Frame),
    /// A picture beside the copied state.
    File(PathBuf),
    None,
}

pub struct Job {
    pub base: PathBuf,
    pub kind: Kind,
    pub source: Source,
    pub picture: Picture,
}

/// What the writer did.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Written {
    pub written: u32,
    /// Not written: the card nearly full, a state over the game's cap, a copy whose file was gone.
    pub skipped: u32,
    pub failed: u32,
    /// Files removed to stay under the caps.
    pub removed: u32,
    pub write_ms: f64,
}

#[derive(Default)]
struct Queue {
    jobs: VecDeque<Job>,
    busy: bool,
    quit: bool,
    written: Written,
}

struct Shared {
    queue: Mutex<Queue>,
    changed: Condvar,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, Queue> {
        self.queue.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// The thread that writes earlier states: one at a time, the emulation thread never waits for it. At a quit the one
/// being written is finished and the ones still waiting are dropped (the quit's own resume state is the newer one).
pub struct Writer {
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
}

impl Writer {
    pub fn start(caps: Caps) -> Writer {
        let shared = Arc::new(Shared { queue: Mutex::new(Queue::default()), changed: Condvar::new() });
        let s = shared.clone();
        let thread = std::thread::Builder::new().name("earlier-writer".into()).spawn(move || worker(&s, caps)).ok();
        Writer { shared, thread }
    }

    /// Hands a state over; returns at once. A newer state of the ring replaces one still waiting (only the newest
    /// matters); a session start always waits its turn.
    pub fn submit(&self, job: Job) {
        if self.thread.is_none() {
            // No thread could be started: no earlier states rather than a stalled game.
            self.shared.lock().written.skipped += 1;
            return;
        }
        let mut q = self.shared.lock();
        match q.jobs.iter_mut().find(|j| j.kind == Kind::Every && job.kind == Kind::Every && j.base == job.base) {
            Some(waiting) => *waiting = job,
            None => q.jobs.push_back(job),
        }
        drop(q);
        self.shared.changed.notify_all();
    }

    /// Waits until everything handed over is done.
    pub fn wait(&self) {
        let mut q = self.shared.lock();
        while self.thread.is_some() && (!q.jobs.is_empty() || q.busy) {
            q = self.shared.changed.wait(q).unwrap_or_else(|e| e.into_inner());
        }
    }

    pub fn written(&self) -> Written {
        self.shared.lock().written
    }
}

impl Drop for Writer {
    fn drop(&mut self) {
        {
            let mut q = self.shared.lock();
            q.quit = true;
            q.jobs.clear();
        }
        self.shared.changed.notify_all();
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

fn worker(s: &Shared, caps: Caps) {
    let mut q = s.lock();
    loop {
        if q.quit {
            return;
        }
        let Some(job) = q.jobs.pop_front() else {
            q = s.changed.wait(q).unwrap_or_else(|e| e.into_inner());
            continue;
        };
        q.busy = true;
        drop(q);
        let t0 = Instant::now();
        let r = write(&job, caps);
        let ms = t0.elapsed().as_secs_f64() * 1000.0;
        q = s.lock();
        q.busy = false;
        match r {
            Ok(removed) => {
                q.written.written += 1;
                q.written.write_ms += ms;
                q.written.removed += removed;
            }
            Err(Skip::Skipped(why)) => {
                println!("earlier: not kept: {why}");
                q.written.skipped += 1;
            }
            Err(Skip::Failed(why)) => {
                eprintln!("earlier: {why}");
                q.written.failed += 1;
            }
        }
        s.changed.notify_all();
    }
}

enum Skip {
    Skipped(String),
    Failed(String),
}

/// Writes one job: the state, then its picture, then the caps. Returns how many files the caps removed.
fn write(job: &Job, caps: Caps) -> Result<u32, Skip> {
    let dir = job.base.parent().filter(|d| !d.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let _ = std::fs::create_dir_all(dir);
    let copied;
    let data: &[u8] = match &job.source {
        Source::State(d) => d,
        Source::Copy(from) => {
            copied = std::fs::read(from).map_err(|e| Skip::Skipped(format!("{}: {e}", from.display())))?;
            &copied
        }
    };
    let bytes = data.len() as u64;
    if bytes == 0 {
        return Err(Skip::Skipped("the state is empty".into()));
    }
    if bytes > caps.game {
        return Err(Skip::Skipped(format!("a state of {} is more than a game's {}", crate::storage::size(bytes), crate::storage::size(caps.game))));
    }
    if crate::storage::free_space(dir).is_some_and(|(free, _)| free < caps.min_free.saturating_add(bytes)) {
        return Err(Skip::Skipped("the SD card is nearly full".into()));
    }
    let target = path(&job.base, next_name(&job.base, job.kind));
    let picture = crate::slots::preview_path(&target);
    // The old picture goes first: a state is never listed with the picture of the moment it replaced.
    let _ = std::fs::remove_file(&picture);
    crate::fsutil::replace_file(&target, data).map_err(|e| Skip::Failed(format!("{}: {e}", target.display())))?;
    let png = match &job.picture {
        Picture::Frame(f) => encode_png(f),
        Picture::File(p) => std::fs::read(p).ok(),
        Picture::None => None,
    };
    if let Some(png) = png {
        // No picture is no harm: the menu says "No preview".
        let _ = crate::fsutil::replace_file(&picture, &png);
    }
    let root = dir.parent().unwrap_or(dir);
    Ok(enforce(root, &job.base, caps, &target))
}

/// A frame as PNG, as `pocket-core`'s own previews are.
pub fn encode_png(frame: &Frame) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut enc = png::Encoder::new(&mut out, frame.width, frame.height);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().and_then(|mut w| w.write_image_data(&frame.to_rgb())).ok()?;
    Some(out)
}

/// An earlier state on the card, with its picture, for the caps.
struct OnCard {
    state: PathBuf,
    at: SystemTime,
    bytes: u64,
}

fn on_card(state: PathBuf) -> Option<OnCard> {
    let m = std::fs::symlink_metadata(&state).ok().filter(|m| m.is_file())?;
    let picture = std::fs::symlink_metadata(crate::slots::preview_path(&state)).map_or(0, |m| m.len());
    Some(OnCard { at: m.modified().unwrap_or(UNIX_EPOCH), bytes: m.len() + picture, state })
}

/// Removes the oldest of `files` until they add up to `cap` at most; `keep` (the one just written) stays.
fn trim(mut files: Vec<OnCard>, cap: u64, keep: &Path) -> u32 {
    files.sort_by_key(|f| f.at);
    let mut total: u64 = files.iter().map(|f| f.bytes).sum();
    let mut removed = 0;
    for f in files {
        if total <= cap {
            break;
        }
        if f.state == keep {
            continue;
        }
        if std::fs::remove_file(&f.state).is_ok() {
            let _ = std::fs::remove_file(crate::slots::preview_path(&f.state));
            total = total.saturating_sub(f.bytes);
            removed += 1;
        }
    }
    removed
}

/// Keeps Earlier under its caps after `keep` was written for the game `base`: that game's under `caps.game`, all of
/// them in the system folders of `root` (`/storage/roms/savestates`) under `caps.all`, oldest out first. Returns how
/// many states were removed.
pub fn enforce(root: &Path, base: &Path, caps: Caps, keep: &Path) -> u32 {
    let mine: Vec<OnCard> = Name::all().filter_map(|n| on_card(path(base, n))).collect();
    let mut removed = trim(mine, caps.game, keep);
    let mut all = Vec::new();
    for system in std::fs::read_dir(root).into_iter().flatten().flatten() {
        if !system.file_type().is_ok_and(|t| t.is_dir()) {
            continue;
        }
        for f in std::fs::read_dir(system.path()).into_iter().flatten().flatten() {
            let name = f.file_name();
            let Some(name) = name.to_str() else { continue };
            if !name.ends_with(".png") && base_of(name).is_some() {
                all.extend(on_card(f.path()));
            }
        }
    }
    removed += trim(all, caps.all, keep);
    removed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;

    struct Dir(crate::testutil::TestDir);

    impl Dir {
        fn new(tag: &str) -> Dir {
            let d = TestDir::new(&format!("timeline-{tag}"));
            std::fs::create_dir_all(d.join("savestates/gba")).unwrap();
            Dir(d)
        }

        fn base(&self, system: &str, game: &str) -> PathBuf {
            let dir = self.0.join("savestates").join(system);
            std::fs::create_dir_all(&dir).unwrap();
            dir.join(game)
        }

        fn root(&self) -> PathBuf {
            self.0.join("savestates")
        }
    }


    /// Room enough everywhere, and no look at the free space of the machine running the tests.
    fn roomy() -> Caps {
        Caps { game: u64::MAX, all: u64::MAX, min_free: 0 }
    }

    fn set_age(p: &Path, secs_ago: u64) {
        let t = SystemTime::now() - Duration::from_secs(secs_ago);
        std::fs::File::options().write(true).open(p).unwrap().set_modified(t).unwrap();
    }

    fn frame() -> Frame {
        Frame { width: 2, height: 2, pitch: 4, format: 2, data: vec![0xff; 8] }
    }

    #[test]
    fn names_are_the_files_and_the_wire() {
        let b = Path::new("/storage/roms/savestates/gba/Golden Sun (U).v1");
        assert_eq!(path(b, Name { kind: Kind::Every, n: 3 }), Path::new("/storage/roms/savestates/gba/Golden Sun (U).v1.pstate.earlier3"));
        assert_eq!(path(b, Name { kind: Kind::Started, n: 1 }), Path::new("/storage/roms/savestates/gba/Golden Sun (U).v1.pstate.started1"));
        assert_eq!(Name::all().count(), (KEEP + STARTS) as usize);
        for n in Name::all() {
            assert_eq!(Name::parse(&n.wire()), Some(n));
        }
        for junk in ["earlier0", "earlier7", "started4", "earlier", "3", "slot1", "auto", "undo", ""] {
            assert_eq!(Name::parse(junk), None, "{junk}");
        }
        // Storage and Sync tell them from slots and from temporary files of a write under way.
        assert_eq!(base_of("Golden Sun (U).v1.pstate.earlier3"), Some("Golden Sun (U).v1"));
        assert_eq!(base_of("Golden Sun.pstate.started2.png"), Some("Golden Sun"));
        for other in ["Golden Sun.pstate", "Golden Sun.pstate3", "Golden Sun.pstate.auto", "Golden Sun.pstate.earlier3.123-4.tmp", ".pstate.earlier1", "Golden Sun.pstate.earlier9"] {
            assert_eq!(base_of(other), None, "{other}");
        }
    }

    #[test]
    fn the_ring_fills_up_then_replaces_the_oldest() {
        let d = Dir::new("ring");
        let base = d.base("gba", "Game");
        for i in 0..KEEP {
            write(&Job { base: base.clone(), kind: Kind::Every, source: Source::State(vec![i; 10]), picture: Picture::None }, roomy()).ok().unwrap();
            // A second apart, as minutes are on the device.
            set_age(&path(&base, Name { kind: Kind::Every, n: i + 1 }), 1000 - u64::from(i) * 10);
        }
        let e = entries(&base);
        assert_eq!(e.len(), KEEP as usize);
        assert_eq!(e[0].name.n, KEEP, "newest first");
        assert_eq!(std::fs::read(&e[0].path).unwrap(), vec![KEEP - 1; 10]);
        // The seventh goes where the first was.
        write(&Job { base: base.clone(), kind: Kind::Every, source: Source::State(vec![99; 10]), picture: Picture::Frame(frame()) }, roomy()).ok().unwrap();
        let e = entries(&base);
        assert_eq!(e.len(), KEEP as usize, "still six");
        assert_eq!(e[0].name, Name { kind: Kind::Every, n: 1 });
        assert_eq!(std::fs::read(&e[0].path).unwrap(), vec![99; 10]);
        assert!(crate::slots::preview_path(&e[0].path).exists(), "with its picture");
        assert_eq!(e[1].name.n, KEEP, "the one before is next");
        set_age(&e[0].path, 5);
        // Then the second-oldest.
        write(&Job { base: base.clone(), kind: Kind::Every, source: Source::State(vec![100; 10]), picture: Picture::None }, roomy()).ok().unwrap();
        assert_eq!(entries(&base)[0].name.n, 2);
        assert!(!crate::slots::preview_path(&path(&base, Name { kind: Kind::Every, n: 2 })).exists(), "no picture of the moment it replaced");
    }

    #[test]
    fn a_session_start_is_a_copy_of_the_state_it_continued_from() {
        let d = Dir::new("start");
        let base = d.base("gba", "Game");
        let taken = crate::slots::taken_auto_path(&crate::slots::auto_path(&base));
        std::fs::write(&taken, b"resumed").unwrap();
        std::fs::write(crate::slots::preview_path(&crate::slots::auto_path(&base)), b"png").unwrap();
        set_age(&taken, 86_400);
        let job = Job { base: base.clone(), kind: Kind::Started, source: Source::Copy(taken.clone()), picture: Picture::File(crate::slots::preview_path(&crate::slots::auto_path(&base))) };
        write(&job, roomy()).ok().unwrap();
        let e = entries(&base);
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].name, Name { kind: Kind::Started, n: 1 });
        assert_eq!(std::fs::read(&e[0].path).unwrap(), b"resumed");
        assert!(e[0].at.elapsed().unwrap() < Duration::from_secs(60), "its time is when the session began, not when the state was saved");
        assert_eq!(std::fs::read(crate::slots::preview_path(&e[0].path)).unwrap(), b"png");
        assert!(taken.exists(), "the taken state stays for the start's own use");
        // A copy whose file has gone is skipped, quietly.
        std::fs::remove_file(&taken).unwrap();
        assert!(matches!(write(&job, roomy()), Err(Skip::Skipped(_))));
    }

    #[test]
    fn the_caps_hold_per_game_and_for_the_card() {
        let d = Dir::new("caps");
        let alpha = d.base("gba", "Alpha");
        let beta = d.base("snes", "Beta");
        let old = |base: &Path, n: u8, bytes: usize, age: u64| {
            let p = path(base, Name { kind: Kind::Every, n });
            std::fs::write(&p, vec![1; bytes]).unwrap();
            set_age(&p, age);
            p
        };
        // Beta: three of 300 bytes, long ago.
        for n in 1..=3 {
            old(&beta, n, 300, 10_000 + u64::from(n));
        }
        // Alpha: three of 300 bytes, a picture of 100 with the oldest.
        let a1 = old(&alpha, 1, 300, 3000);
        std::fs::write(crate::slots::preview_path(&a1), vec![0; 100]).unwrap();
        set_age(&a1, 3000);
        old(&alpha, 2, 300, 2000);
        old(&alpha, 3, 300, 1000);
        // A game's cap of 700: its oldest goes (with its picture), not another game's.
        let caps = Caps { game: 700, all: u64::MAX, min_free: 0 };
        write(&Job { base: alpha.clone(), kind: Kind::Every, source: Source::State(vec![2; 300]), picture: Picture::None }, caps).ok().unwrap();
        let names: Vec<u8> = entries(&alpha).iter().map(|e| e.name.n).collect();
        assert_eq!(names, [4, 3], "Alpha keeps its two newest: 600 of 700 bytes");
        assert!(!crate::slots::preview_path(&a1).exists());
        assert_eq!(entries(&beta).len(), 3);
        // The card's cap of 1000 in all: the oldest of any game go first, the one just written always stays.
        let caps = Caps { game: u64::MAX, all: 1000, min_free: 0 };
        write(&Job { base: alpha.clone(), kind: Kind::Every, source: Source::State(vec![3; 300]), picture: Picture::None }, caps).ok().unwrap();
        let total: u64 = entries(&alpha).iter().chain(entries(&beta).iter()).map(|e| e.bytes).sum();
        assert!(total <= 1000, "{total}");
        assert!(entries(&beta).is_empty(), "Beta's were the oldest");
        assert_eq!(entries(&alpha).len(), 3);
        // A state bigger than a game's cap isn't kept at all, and a nearly full card keeps none.
        let tiny = Caps { game: 100, all: u64::MAX, min_free: 0 };
        assert!(matches!(write(&Job { base: alpha.clone(), kind: Kind::Every, source: Source::State(vec![4; 300]), picture: Picture::None }, tiny), Err(Skip::Skipped(_))));
        let full = Caps { game: u64::MAX, all: u64::MAX, min_free: u64::MAX / 2 };
        assert!(matches!(write(&Job { base: alpha.clone(), kind: Kind::Every, source: Source::State(vec![4; 300]), picture: Picture::None }, full), Err(Skip::Skipped(_))));
        assert_eq!(entries(&alpha).len(), 3, "nothing was replaced by them");
        assert!(d.root().join("gba").is_dir());
    }

    #[test]
    fn play_time_counts_only_refreshes_of_play() {
        let mut c = Clock::default();
        let t0 = Instant::now();
        let refresh = Duration::from_micros(16_667);
        // Four minutes of play.
        let mut t = t0;
        for _ in 0..(4 * 60 * 60) {
            c.tick(t);
            t += refresh;
        }
        let four = c.played();
        assert!((four.as_secs_f32() - 240.0).abs() < 0.1, "{four:?}");
        // The pause menu for ten minutes, then sleep for an hour: nothing.
        c.stop();
        t += Duration::from_secs(600);
        c.tick(t);
        c.stop();
        t += Duration::from_secs(3600);
        c.tick(t);
        assert!(c.played() - four < Duration::from_millis(1), "{:?}", c.played());
        // A refresh that stalled for 3 s (a loading screen, a suspend nobody said) counts a quarter of a second.
        t += Duration::from_secs(3);
        c.tick(t);
        assert_eq!(c.played() - four, MAX_GAP);
        // And a minute more of play makes it due.
        let mut tl = Timeline::new(roomy());
        tl.clock = c;
        assert!(!tl.due());
        for _ in 0..(60 * 60) {
            t += refresh;
            tl.play(t);
        }
        assert!(tl.due(), "{:?}", tl.played());
        tl.failed();
        assert!(!tl.due() && tl.played() == Duration::ZERO, "a failed serialize waits for the next 5 minutes, it is not tried every refresh");
    }

    #[test]
    fn slow_or_gpu_cores_wait_for_a_quiet_moment() {
        let p = 1000.0 / 60.0;
        // A Game Boy Advance (0.4 MB, about 1 ms) in play; a PSP (40 MB, tens of ms) not.
        assert_eq!(moment(false, Some(1.1), None, p), Moment::InPlay);
        assert_eq!(moment(false, Some(30.0), None, p), Moment::Quiet);
        assert_eq!(moment(false, Some(4.1), None, p), Moment::InPlay, "a quarter of a refresh is 4.17 ms");
        assert_eq!(moment(false, Some(4.3), None, p), Moment::Quiet);
        // Not measured yet: by its size.
        assert_eq!(moment(false, None, Some(400_000), p), Moment::InPlay);
        assert_eq!(moment(false, None, Some(4_500_000), p), Moment::Quiet);
        assert_eq!(moment(false, None, None, p), Moment::Quiet, "nothing known: never risk a frame");
        assert_eq!(moment(false, None, Some(0), p), Moment::Quiet);
        // A 3D core's picture is read back from the GPU: never in play, however fast.
        assert_eq!(moment(true, Some(0.5), None, p), Moment::Quiet);
        // The timeline's own measurement wins over run-ahead's, and the size is only asked without either.
        let mut tl = Timeline::new(roomy());
        let mut asked = false;
        assert_eq!(tl.moment(false, Some(1.0), || { asked = true; Some(50_000_000) }, p), Moment::InPlay);
        assert!(!asked);
        let mut asked = 0;
        for _ in 0..3 {
            assert_eq!(tl.moment(false, None, || { asked += 1; Some(50_000_000) }, p), Moment::Quiet);
        }
        assert_eq!(asked, 1, "the size is asked once, not every refresh while a state waits for a pause");
        let d = Dir::new("guard");
        tl.submit_state(&d.base("psx", "Game"), vec![1; 10], None, 9.0, Moment::InPlay);
        assert_eq!(tl.moment(false, Some(1.0), || None, p), Moment::Quiet, "the last capture was slow: the next waits for a pause");
        tl.wait();
        assert_eq!(tl.written().written, 1);
        assert!(tl.report().unwrap().contains("1 states (1 in play"), "{:?}", tl.report());
    }

    #[test]
    fn the_writer_never_holds_up_the_game() {
        let d = Dir::new("writer");
        let base = d.base("gba", "Game");
        let mut tl = Timeline::new(roomy());
        let t = Instant::now();
        // 20 MB with a picture: handed over, not written, on this thread.
        tl.submit_state(&base, vec![7; 20_000_000], Some(frame()), 3.0, Moment::Quiet);
        assert!(t.elapsed() < Duration::from_millis(50), "{:?}", t.elapsed());
        tl.wait();
        let e = entries(&base);
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].bytes, 20_000_000);
        assert!(image_ok(&crate::slots::preview_path(&e[0].path)));
    }

    fn image_ok(p: &Path) -> bool {
        crate::image::load_png(p).is_some()
    }

    #[test]
    fn missing_or_damaged_entries_are_told_calmly() {
        let d = Dir::new("calm");
        let base = d.base("gba", "Game");
        assert!(entries(&base).is_empty());
        assert_eq!(note(None, 1000, 0), "Earlier · none yet · one every 5 min of play");
        // An empty file (the card filled up while it was written): listed, said so, not loadable.
        let p = path(&base, Name { kind: Kind::Every, n: 2 });
        std::fs::write(&p, b"").unwrap();
        set_age(&p, 12 * 60);
        let e = entries(&base);
        assert!(e[0].damaged());
        let now = crate::recents::now_secs();
        assert_eq!(note(Some(&e[0]), now, 0), "Earlier · 12 min ago · damaged, can't be loaded");
        assert_eq!(chip(&e[0], now, 0), "12 min");
        // A folder where a file should be, or a link, is not an entry.
        std::fs::create_dir_all(path(&base, Name { kind: Kind::Every, n: 3 })).unwrap();
        std::os::unix::fs::symlink("/etc/passwd", path(&base, Name { kind: Kind::Every, n: 4 })).unwrap();
        assert_eq!(entries(&base).len(), 1);
    }

    #[test]
    fn the_menu_row_steps_around_and_never_loads_a_damaged_one() {
        let at = |n: u8, secs: u64, bytes: u64| Entry { name: Name { kind: Kind::Every, n }, path: PathBuf::new(), at: UNIX_EPOCH + Duration::from_secs(secs), bytes };
        let mut row = Row::new(vec![at(3, 3000, 10), at(2, 2000, 0), at(1, 1000, 10)], 0);
        assert_eq!(row.command().as_deref(), Some("load earlier3 3000"), "the file's time goes with it");
        row.step(1);
        assert!(!row.loadable() && row.command().is_none(), "the damaged one");
        row.step(1);
        row.step(1);
        assert_eq!(row.at, 0, "around");
        row.step(-1);
        assert_eq!(row.selected().unwrap().name.n, 1);
        // Nothing yet: nothing selected, nothing loaded, no panic.
        let mut none = Row::new(Vec::new(), 0);
        none.step(1);
        assert!(none.selected().is_none() && !none.loadable() && none.command().is_none());
    }

    #[test]
    fn a_load_takes_the_state_the_menu_listed_or_says_why_not() {
        let d = Dir::new("toload");
        let base = d.base("gba", "Game");
        assert_eq!(to_load(&base, "earlier1", None), Err("That earlier state is gone"));
        assert_eq!(to_load(&base, "slot3", None), Err("That earlier state is gone"));
        let p = path(&base, Name { kind: Kind::Every, n: 1 });
        std::fs::write(&p, b"state").unwrap();
        let listed = entry(&base, Name { kind: Kind::Every, n: 1 }).unwrap().secs();
        assert_eq!(to_load(&base, "earlier1", Some(listed)).unwrap().path, p);
        assert_eq!(to_load(&base, "earlier1", None).unwrap().path, p, "without a time: what is there");
        // Replaced since the menu listed it (a pause's own state went into the oldest file).
        assert_eq!(to_load(&base, "earlier1", Some(listed - 300)), Err("That earlier state was just replaced. Pick it again."));
        std::fs::write(&p, b"").unwrap();
        let listed = entry(&base, Name { kind: Kind::Every, n: 1 }).unwrap().secs();
        assert_eq!(to_load(&base, "earlier1", Some(listed)), Err("That earlier state is damaged"));
    }

    #[test]
    fn chips_and_notes_say_when() {
        let tz = 3600; // UTC+1
        // 2026-10-25 14:30 local is 13:30 UTC.
        let now = 1_792_935_000u64;
        let at = |kind, secs_ago: u64| Entry { name: Name { kind, n: 1 }, path: PathBuf::new(), at: UNIX_EPOCH + Duration::from_secs(now - secs_ago), bytes: 1 };
        let every = at(Kind::Every, 12 * 60);
        assert_eq!(chip(&every, now, tz), "12 min");
        assert_eq!(chip(&at(Kind::Every, 30), now, tz), "just now");
        assert_eq!(chip(&at(Kind::Every, 2 * 3600 + 5), now, tz), "2 h");
        assert_eq!(chip(&at(Kind::Every, 3 * 86_400), now, tz), "3 days");
        assert_eq!(note(Some(&every), now, tz), "Earlier · 12 min ago");
        assert_eq!(load_label(&every, now), "the state from 12 min ago");
        let today = at(Kind::Started, 28 * 60);
        assert_eq!(chip(&today, now, tz), "14:02");
        assert_eq!(note(Some(&today), now, tz), "Earlier · session start · today 14:02");
        assert_eq!(load_label(&today, now), "the session start");
        let yesterday = at(Kind::Started, 18 * 3600);
        assert_eq!(chip(&yesterday, now, tz), "yesterday");
        assert_eq!(note(Some(&yesterday), now, tz), "Earlier · session start · yesterday 20:30");
        let older = at(Kind::Started, 3 * 86_400);
        assert_eq!(chip(&older, now, tz), "3 days");
        assert_eq!(note(Some(&older), now, tz), "Earlier · session start · 3 days ago");
    }

    #[test]
    fn big_states_start_off() {
        for s in ["gb", "gbc", "gba", "nes", "snes", "megadrive", "psx", "pcengine"] {
            assert!(default_on(s), "{s}");
        }
        for s in ["n64", "psp", "dreamcast", "saturn", "nds"] {
            assert!(!default_on(s), "{s}");
        }
    }
}
