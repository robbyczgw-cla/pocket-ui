//! The play log: every game session (start, end, game, system, emulator, duration), status changes and the totals
//! from before the log began, in one small append-only file in the data directory (`play-log.bin`). It is the
//! source of Curator → History and the Year in review (`play_history`, `history_page`). Private: nothing reads it
//! but pocket-ui, and it never leaves the device, except as `library_sync`'s `history.json` to the player's own other
//! devices when they switched on Settings → Sync → Library data (whose sessions come back through [`set_extra`] and
//! are shown with this log, never written into it).
//!
//! * **Recording is off the UI thread.** [`started`], [`ended`], [`status`], [`paused`] and [`resumed`] put a
//!   command in a channel (a lock and a push, no I/O, no clock reads beyond `SystemTime::now` and `Instant::now`); a
//!   thread of its own (`play-log`, low priority) works out the local time zone, appends the frames and `fdatasync`s.
//!   A game's start is written a few seconds after it ([`START_QUIET`], or at once when its end or the exit's flush
//!   comes first), so the write never meets the game's own loading. Before [`open`] (called once the first frame is
//!   up) commands wait in memory, so not even the thread's start falls on pocket-ui's start.
//! * **Append-only, crash-safe.** The file is an 8-byte header and frames: `u8 kind | u16 length | payload | u32
//!   CRC-32 of kind, length and payload`. A frame is written in one `write` and synced. A power cut can only leave a
//!   torn last frame: the reader stops at it and the writer cuts it off before the next append. A damaged frame in
//!   the middle (the card) is skipped: the reader looks for the next frame whose CRC holds. A write that failed
//!   half way is cut back off before it is tried again, and every record is keyed so that one written twice counts
//!   once (sessions by their number, carried totals by their game).
//! * **A session survives a crash.** A game's start is written when it starts and its end when it ends; a start
//!   without an end (pocket-ui crashed, the battery died) still counts as a session and a day played, without time.
//! * **Sleep is no play.** Light sleep and a suspend while a game runs end a stretch of the session ([`KIND_PAUSE`])
//!   and waking begins the next one ([`KIND_RESUME`]): the session's time leaves the sleep out and each stretch is
//!   counted on the days it was really played ([`Session::parts`]).
//! * **Bounded.** 66 bytes a session (start and end); past [`MAX_BYTES`] (some 20 years of eight sessions a day) the
//!   file is rewritten atomically (`fsutil::write_atomic`) with the oldest sessions left out, down to [`COMPACT_TO`].
//! * **Migration.** The first time it runs, the totals pocket-ui already kept (`library.json`'s seconds and sessions,
//!   the recent games' last played and time) become "carried" records ([`Seed`]): counted in all-time totals and
//!   for "back after a long break", never spread over days they can't be placed on. What the log already has for a
//!   game is taken off first, so nothing counts twice. A migration cut off by a power cut is finished at the next
//!   start: a game carried already is not carried again.
//!
//! Frame kinds and payloads (little endian; strings are ids into the name table, [`KIND_NAME`] frames; fields
//! marked "since 2026-10-24" are at the end of their payload and absent from older files):
//!
//! | Kind | Payload |
//! |---|---|
//! | 1 name | u32 id, UTF-8 text |
//! | 2 start | u32 unix start, i16 UTC offset (minutes), u32 path, u32 name, u32 system, u32 emulator, u32 session number (since 2026-10-24) |
//! | 3 end | u32 unix start, i16 UTC offset, u32 seconds, u32 path, u32 name, u32 system, u32 session number (since 2026-10-24) |
//! | 4 status | u32 unix time, i16 UTC offset, u32 path, u32 name, u32 system, u8 status |
//! | 5 carried | u32 path, u32 name, u32 system, u32 seconds, u32 sessions, u32 last played (0: unknown) |
//! | 6 migrated | u32 unix time |
//! | 7 pause (since 2026-10-24) | u32 session number, u32 stretch (0, 1, …), u32 unix time, u32 seconds played in the stretch |
//! | 8 resume (since 2026-10-24) | u32 session number, u32 stretch, u32 unix time |
//!
//! Kinds this reader doesn't know are skipped (a newer pocket-ui's). The read side for other modules (the web
//! companion) is [`read`] (the file, any process) and [`view`] (the copy the writer keeps in this process);
//! `play_history` works on either.

use std::collections::{HashMap, HashSet};
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::library_meta::Status;

pub const FILE: &str = "play-log.bin";
const MAGIC: &[u8; 4] = b"PPL1";
const VERSION: u8 = 1;
const HEADER: usize = 8;
/// Past this size the oldest sessions are left out (rewritten whole, atomically) ...
pub const MAX_BYTES: u64 = 4 << 20;
/// ... down to about this.
pub const COMPACT_TO: u64 = 3 << 20;
/// Bytes of a session of a known game: its start and its end frame.
const SESSION_BYTES: usize = 66;
/// Before this the device clock was not set (no battery-backed clock): such sessions count in totals, never on a day.
pub const CLOCK_SET: u64 = 1_700_000_000;
/// How long the writer leaves the card to a game that is starting before it writes the start.
const START_QUIET: Duration = Duration::from_secs(3);
/// How long the flush at exit tries to write what a full card held back (`settle` waits two seconds in all).
const FLUSH_FOR: Duration = Duration::from_millis(1500);
const FLUSH_RETRY: Duration = Duration::from_millis(150);
/// The longest string kept (a path); longer ones are cut at a character boundary.
const STRING_MAX: usize = 4000;

pub const KIND_NAME: u8 = 1;
pub const KIND_START: u8 = 2;
pub const KIND_END: u8 = 3;
pub const KIND_STATUS: u8 = 4;
pub const KIND_CARRIED: u8 = 5;
pub const KIND_MIGRATED: u8 = 6;
pub const KIND_PAUSE: u8 = 7;
pub const KIND_RESUME: u8 = 8;

// ---- the data ------------------------------------------------------------------------------------------------------

/// A game as the log knows it: its file, and the name and system it had when it was last played.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Game {
    pub path: String,
    pub name: String,
    pub system: String,
}

/// One session. `game` is an index into [`Log::games`].
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Session {
    pub game: usize,
    /// Unix seconds of the start (wall clock).
    pub start: u64,
    /// Seconds east of UTC at the start: the local day it is counted on.
    pub tz: i32,
    /// Seconds the game was played: open, its pause menu included; light sleep and suspends not. 0 when it never
    /// ended.
    pub secs: u64,
    /// The end was recorded. False: pocket-ui or the device went down while it ran; counted without time.
    pub ended: bool,
    /// "pocket-core/gambatte", "retroarch/mgba", "duckstation": the emulator (and core) it ran on, if known.
    pub emulator: String,
    /// The stretches it was played in when sleep came in between (light sleep, a suspend): each one's start (unix)
    /// and seconds, in order, the first from `start`. Empty: it ran in one go, `secs` from `start`.
    pub parts: Vec<Part>,
    /// Its number in the file, which pairs its start, pauses and end: two sessions begun at the same clock time (a
    /// clock that was reset) stay two. 0: a session of a file written before 2026-10-24, paired by start and game.
    #[serde(skip)]
    pub id: u32,
}

/// A stretch of a session between two sleeps.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Part {
    /// Unix seconds it began.
    pub at: u64,
    pub secs: u64,
}

impl Session {
    /// When and how long it was played, stretch by stretch: (unix start, seconds). Nothing for a session whose end was
    /// never recorded (its time is unknown).
    pub fn stretches(&self) -> impl Iterator<Item = (u64, u64)> + '_ {
        let whole = (self.ended && self.parts.is_empty()).then_some((self.start, self.secs));
        whole.into_iter().chain(self.parts.iter().filter(|_| self.ended).map(|p| (p.at, p.secs)))
    }

    /// When its last stretch ended (unix); its start for a session without a recorded end.
    pub fn end(&self) -> u64 {
        self.stretches().map(|(at, secs)| at + secs).max().unwrap_or(self.start)
    }

    /// Sleep came: the stretch that ran since `since` (unix) ends after `secs` seconds of play.
    fn pause(&mut self, since: u64, secs: u64) {
        self.parts.push(Part { at: since, secs });
    }

    /// The end: `secs` played in all; `running`: since when the last stretch ran (None: it ended asleep).
    fn finish(&mut self, secs: u64, running: Option<u64>) {
        self.secs = secs;
        self.ended = true;
        if let Some(at) = running.filter(|_| !self.parts.is_empty()) {
            let rest = secs.saturating_sub(self.parts.iter().map(|p| p.secs).sum());
            if rest > 0 {
                self.parts.push(Part { at, secs: rest });
            }
        }
    }
}

/// The player set a game's status (the card's picker).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct StatusChange {
    pub game: usize,
    pub at: u64,
    pub tz: i32,
    pub status: Status,
}

/// What a game had been played before the log began (from library.json and the recent games).
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Carried {
    pub game: usize,
    pub secs: u64,
    pub sessions: u32,
    /// Unix seconds of the last play before the log, when the recent games knew it.
    pub last_played: Option<u64>,
}

/// Everything in the log, in time order (sessions by start).
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Log {
    pub games: Vec<Game>,
    pub sessions: Vec<Session>,
    pub statuses: Vec<StatusChange>,
    /// At most one a game.
    pub carried: Vec<Carried>,
    /// The totals from before have been taken over (or there were none).
    pub migrated: bool,
    /// Frames that failed their check and were skipped (0 for a healthy file).
    #[serde(skip)]
    pub damaged: usize,
    /// An earlier file was no play log at all (zeros after a power cut, another program's file) and was kept aside
    /// as `play-log.bin.damaged`: what it held is not in these numbers. The pages say so.
    #[serde(skip)]
    pub set_aside: bool,
    #[serde(skip)]
    by_path: HashMap<String, usize>,
}

impl Log {
    /// The game of a file, if it was ever played.
    pub fn find(&self, path: &str) -> Option<usize> {
        self.by_path.get(path).copied()
    }

    /// The game's index, adding it (and taking the newer name and system).
    fn game(&mut self, path: &str, name: &str, system: &str) -> usize {
        if let Some(&i) = self.by_path.get(path) {
            let g = &mut self.games[i];
            if !name.is_empty() {
                g.name = name.to_string();
                crate::i18n::learn_names([name]);
            }
            if !system.is_empty() {
                g.system = system.to_string();
            }
            return i;
        }
        // The history paints the name: a game's, not a word of the interface (`i18n::learn_names`).
        crate::i18n::learn_names([name]);
        self.games.push(Game { path: path.to_string(), name: name.to_string(), system: system.to_string() });
        self.by_path.insert(path.to_string(), self.games.len() - 1);
        self.games.len() - 1
    }

    /// The first moment the log counts dated sessions from (its oldest dated session).
    pub fn counting_since(&self) -> Option<u64> {
        self.sessions.iter().map(|s| s.start).find(|t| *t >= CLOCK_SET)
    }

    /// The session of a number (searched from the newest: it is nearly always the running one).
    fn session(&mut self, id: u32) -> Option<&mut Session> {
        self.sessions.iter_mut().rev().find(|s| s.id == id && id != 0)
    }

    fn sort(&mut self) {
        self.sessions.sort_by_key(|s| s.start);
        self.statuses.sort_by_key(|s| s.at);
    }
}

// ---- the file: frames ------------------------------------------------------------------------------------------

fn status_code(s: Status) -> u8 {
    match s {
        Status::NotStarted => 0,
        Status::Playing => 1,
        Status::Paused => 2,
        Status::Completed => 3,
        Status::Dropped => 4,
    }
}

fn status_of(code: u8) -> Option<Status> {
    Some(match code {
        0 => Status::NotStarted,
        1 => Status::Playing,
        2 => Status::Paused,
        3 => Status::Completed,
        4 => Status::Dropped,
        _ => return None,
    })
}

fn header() -> [u8; HEADER] {
    let mut h = [0u8; HEADER];
    h[..4].copy_from_slice(MAGIC);
    h[4] = VERSION;
    h
}

/// One frame: kind, length, payload, CRC-32.
fn frame(kind: u8, payload: &[u8]) -> Vec<u8> {
    let len = payload.len().min(u16::MAX as usize) as u16;
    let mut f = Vec::with_capacity(payload.len() + 7);
    f.push(kind);
    f.extend_from_slice(&len.to_le_bytes());
    f.extend_from_slice(&payload[..len as usize]);
    let crc = crate::battery_history::crc32(&f);
    f.extend_from_slice(&crc.to_le_bytes());
    f
}

/// A frame of u32 fields.
fn frame_u32(kind: u8, fields: &[u32]) -> Vec<u8> {
    frame(kind, &fields.iter().flat_map(|v| v.to_le_bytes()).collect::<Vec<u8>>())
}

/// Reads little-endian fields off a payload.
struct Fields<'a>(&'a [u8]);

impl Fields<'_> {
    fn u32(&mut self) -> Option<u32> {
        let (v, rest) = self.0.split_first_chunk::<4>()?;
        self.0 = rest;
        Some(u32::from_le_bytes(*v))
    }
    fn i16(&mut self) -> Option<i16> {
        let (v, rest) = self.0.split_first_chunk::<2>()?;
        self.0 = rest;
        Some(i16::from_le_bytes(*v))
    }
    fn u8(&mut self) -> Option<u8> {
        let (v, rest) = self.0.split_first()?;
        self.0 = rest;
        Some(*v)
    }
}

/// A frame found at an offset: its kind, payload and where the next one starts. Kind 0 is never a frame (zeros).
fn frame_at(bytes: &[u8], at: usize) -> Option<(u8, &[u8], usize)> {
    let head = bytes.get(at..at + 3)?;
    let len = u16::from_le_bytes([head[1], head[2]]) as usize;
    let end = at + 3 + len;
    let body = bytes.get(at..end)?;
    let crc = u32::from_le_bytes(bytes.get(end..end + 4)?.try_into().ok()?);
    (crate::battery_history::crc32(body) == crc && head[0] != 0).then(|| (head[0], &bytes[at + 3..end], end + 4))
}

/// What reading a file found, for the writer: the log, the name table and where good data ends.
struct Parsed {
    log: Log,
    ids: HashMap<String, u32>,
    next_id: u32,
    /// The next session number.
    next_session: u32,
    /// The end of the last good frame; bytes after it are a torn write when nothing good follows.
    good_end: usize,
}

/// How the reader pairs a session's frames: by its number, or (files from before 2026-10-24) by start and path.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Key {
    Id(u32),
    Old(u32, u32),
}

/// A session being read: where it is, and since when its stretch runs (None: asleep).
struct Open {
    i: usize,
    running: Option<u64>,
}

fn parse(bytes: &[u8]) -> Result<Parsed, String> {
    if bytes.len() < HEADER || &bytes[..4] != MAGIC {
        return Err("not a play log".into());
    }
    if bytes[4] != VERSION {
        return Err(format!("version {} (this pocket-ui reads {VERSION})", bytes[4]));
    }
    let mut names: HashMap<u32, String> = HashMap::new();
    let mut log = Log::default();
    let mut open: HashMap<Key, Open> = HashMap::new();
    let mut carried: HashSet<usize> = HashSet::new();
    let (mut at, mut good_end) = (HEADER, HEADER);
    // The highest id any frame refers to: a new name never takes an id whose name frame was lost to damage.
    let mut max_ref = 0u32;
    let mut max_session = 0u32;
    let mut name = |names: &HashMap<u32, String>, id: u32| {
        max_ref = max_ref.max(id);
        names.get(&id).cloned().unwrap_or_default()
    };
    while at < bytes.len() {
        let Some((kind, payload, next)) = frame_at(bytes, at) else {
            // Damaged or torn: look for the next frame that holds (a false match is a 1 in 4 billion chance).
            match (at + 1..bytes.len()).find(|&i| frame_at(bytes, i).is_some()) {
                Some(i) => {
                    log.damaged += 1;
                    at = i;
                    continue;
                }
                None => break,
            }
        };
        at = next;
        good_end = next;
        let mut f = Fields(payload);
        match kind {
            KIND_NAME => {
                let Some(id) = f.u32() else { continue };
                names.insert(id, String::from_utf8_lossy(f.0).into_owned());
            }
            KIND_START => {
                let (Some(start), Some(tz), Some(p), Some(n), Some(s), Some(e)) = (f.u32(), f.i16(), f.u32(), f.u32(), f.u32(), f.u32()) else { continue };
                let id = f.u32().unwrap_or(0);
                let path = name(&names, p);
                if path.is_empty() {
                    continue;
                }
                let game = log.game(&path, &name(&names, n), &name(&names, s));
                max_session = max_session.max(id);
                let key = if id != 0 { Key::Id(id) } else { Key::Old(start, p) };
                // The same start again: written twice (a write tried again), not a new session.
                if let Some(o) = open.get(&key) {
                    log.sessions[o.i].emulator = name(&names, e);
                    continue;
                }
                log.sessions.push(Session { game, start: start as u64, tz: tz as i32 * 60, emulator: name(&names, e), id, ..Default::default() });
                open.insert(key, Open { i: log.sessions.len() - 1, running: Some(start as u64) });
            }
            KIND_END => {
                let (Some(start), Some(tz), Some(secs), Some(p), Some(n), Some(s)) = (f.u32(), f.i16(), f.u32(), f.u32(), f.u32(), f.u32()) else { continue };
                let id = f.u32().unwrap_or(0);
                let path = name(&names, p);
                if path.is_empty() {
                    continue;
                }
                let game = log.game(&path, &name(&names, n), &name(&names, s));
                max_session = max_session.max(id);
                let key = if id != 0 { Key::Id(id) } else { Key::Old(start, p) };
                match open.get(&key) {
                    // Its end written twice changes nothing.
                    Some(o) if id != 0 && log.sessions[o.i].ended => {}
                    Some(o) => log.sessions[o.i].finish(secs as u64, o.running),
                    // Its start was lost (damage): the end alone is the session.
                    None => {
                        log.sessions.push(Session { game, start: start as u64, tz: tz as i32 * 60, secs: secs as u64, ended: true, id, ..Default::default() });
                        open.insert(key, Open { i: log.sessions.len() - 1, running: None });
                    }
                }
            }
            KIND_PAUSE => {
                let (Some(id), Some(k), Some(_at), Some(secs)) = (f.u32(), f.u32(), f.u32(), f.u32()) else { continue };
                let Some(o) = open.get_mut(&Key::Id(id)) else { continue };
                let s = &mut log.sessions[o.i];
                // Only the stretch running now, once (a frame written twice, or one out of order, is left out).
                if let Some(since) = o.running.filter(|_| !s.ended && s.parts.len() == k as usize) {
                    s.pause(since, secs as u64);
                    o.running = None;
                }
            }
            KIND_RESUME => {
                let (Some(id), Some(k), Some(at)) = (f.u32(), f.u32(), f.u32()) else { continue };
                let Some(o) = open.get_mut(&Key::Id(id)) else { continue };
                let s = &log.sessions[o.i];
                if o.running.is_none() && !s.ended && s.parts.len() == k as usize {
                    o.running = Some(at as u64);
                }
            }
            KIND_STATUS => {
                let (Some(t), Some(tz), Some(p), Some(n), Some(s), Some(code)) = (f.u32(), f.i16(), f.u32(), f.u32(), f.u32(), f.u8()) else { continue };
                let (path, Some(status)) = (name(&names, p), status_of(code)) else { continue };
                if path.is_empty() {
                    continue;
                }
                let game = log.game(&path, &name(&names, n), &name(&names, s));
                log.statuses.push(StatusChange { game, at: t as u64, tz: tz as i32 * 60, status });
            }
            KIND_CARRIED => {
                let (Some(p), Some(n), Some(s), Some(secs), Some(sessions), Some(last)) = (f.u32(), f.u32(), f.u32(), f.u32(), f.u32(), f.u32()) else { continue };
                let path = name(&names, p);
                if path.is_empty() {
                    continue;
                }
                let game = log.game(&path, &name(&names, n), &name(&names, s));
                // One a game: a second one is a migration that ran again or a write tried again (review
                // SOL-2026-10-24 #3), never more time from before.
                if carried.insert(game) {
                    log.carried.push(Carried { game, secs: secs as u64, sessions, last_played: (last > 0).then_some(last as u64) });
                }
            }
            KIND_MIGRATED => log.migrated = true,
            // A newer pocket-ui's record: not this one's to count.
            _ => {}
        }
    }
    log.sort();
    let next_id = names.keys().map(|k| k + 1).max().unwrap_or(0).max(max_ref.saturating_add(1));
    let ids = names.into_iter().map(|(k, v)| (v, k)).collect();
    Ok(Parsed { log, ids, next_id, next_session: max_session.saturating_add(1).max(1), good_end })
}

/// Reads the play log of a data directory: what the History pages and the web companion read. An absent or
/// unreadable file is an empty log.
pub fn read(data_dir: &Path) -> Log {
    match std::fs::read(data_dir.join(FILE)) {
        Ok(bytes) => parse(&bytes).map(|p| p.log).unwrap_or_default(),
        Err(_) => Log::default(),
    }
}

// ---- writing ---------------------------------------------------------------------------------------------------------

/// What happened, as the UI thread hands it over. `at` is the wall clock (unix seconds), `mono` the monotonic one
/// (it stands still in a suspend, as the game's own time does).
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    Started { at: u64, path: String, name: String, system: String, emulator: String, mono: Duration },
    /// The game ended after `secs` seconds open (the recent games' figure: light sleep included, suspends not).
    Ended { at: u64, path: String, name: String, system: String, secs: u64, mono: Duration },
    Status { at: u64, path: String, name: String, system: String, status: Status },
    /// Light sleep (or a suspend that pocket-ui started, which comes after it): the game is stopped.
    Paused { at: u64, mono: Duration },
    /// Awake again.
    Resumed { at: u64, mono: Duration },
    /// A suspend pocket-ui didn't start (logind) ended at `at`, after `secs` seconds; the game ran up to it.
    Suspended { at: u64, secs: u64, mono: Duration },
}

/// The totals from before the log, for the one migration.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Seed {
    pub games: Vec<SeedGame>,
    pub now: u64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SeedGame {
    pub path: String,
    pub name: String,
    pub system: String,
    /// Seconds played (the larger of library.json's and the recent games' figure).
    pub secs: u64,
    /// Sessions of at least a minute, from library.json.
    pub sessions: u32,
    pub last_played: Option<u64>,
}

/// Which version of the file the writer last wrote or read: device, inode, length, modification and change time.
/// Another file put in its place (a copy over the network, a backup put back, even one of the same length and with
/// its time kept) differs in at least one of them (review SOL-2026-10-24 #4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FileId {
    dev: u64,
    ino: u64,
    len: u64,
    mtime: (i64, i64),
    ctime: (i64, i64),
}

fn file_id(m: &std::fs::Metadata) -> FileId {
    use std::os::unix::fs::MetadataExt;
    FileId { dev: m.dev(), ino: m.ino(), len: m.len(), mtime: (m.mtime(), m.mtime_nsec()), ctime: (m.ctime(), m.ctime_nsec()) }
}

fn file_id_of(path: &Path) -> Option<FileId> {
    std::fs::metadata(path).ok().map(|m| file_id(&m))
}

/// The session running now, as the writer follows it.
#[derive(Clone, Debug)]
struct Run {
    id: u32,
    start: u64,
    path: String,
    /// Stretches ended so far (the next pause's number).
    parts: u32,
    /// Since when (monotonic, wall) the stretch running now runs; while asleep, when the last one ended.
    since: Duration,
    since_at: u64,
    /// Asleep since (monotonic).
    asleep: Option<Duration>,
    /// Asleep so far (monotonic): light sleep, taken off the game's time at its end.
    slept: Duration,
}

fn whole_secs(d: Duration) -> u64 {
    d.as_secs_f64().round() as u64
}

/// The writer's side: the file, its name table, the log as written. Runs on the `play-log` thread; tests drive it
/// directly. `tz` gives the UTC offset (seconds) at a unix time.
pub struct Writer {
    path: PathBuf,
    ids: HashMap<String, u32>,
    next_id: u32,
    next_session: u32,
    log: Arc<Log>,
    running: Option<Run>,
    /// Frames that could not be written yet (a full card): tried again with the next ones, and by the flush at exit.
    unwritten: Vec<u8>,
    tz: fn(u64) -> i32,
    /// The file as this writer last left it: another one is read again before the next append, so names never get
    /// mixed up. None: there is no file yet.
    file: Option<FileId>,
    /// Tests: appends that fail (as on a full card), and whether they write half their bytes first.
    #[cfg(test)]
    fail: (u32, bool),
}

fn local_offset(at: u64) -> i32 {
    crate::appearance::utc_offset_now(at as i64)
}

fn cut(s: &str) -> &str {
    if s.len() <= STRING_MAX {
        return s;
    }
    let mut end = STRING_MAX;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

impl Writer {
    /// Opens (or starts) the log of `data_dir`. A torn last frame is cut off; a file that is not a play log at all
    /// is kept aside as `play-log.bin.damaged` and a new one begun.
    pub fn open(data_dir: &Path, tz: fn(u64) -> i32) -> Writer {
        let path = data_dir.join(FILE);
        let mut w = Writer {
            path,
            ids: HashMap::new(),
            next_id: 0,
            next_session: 1,
            log: Arc::new(Log::default()),
            running: None,
            unwritten: Vec::new(),
            tz,
            file: None,
            #[cfg(test)]
            fail: (0, false),
        };
        let set_aside = w.path.with_extension("bin.damaged").exists();
        let bytes = std::fs::read(&w.path).unwrap_or_default();
        if bytes.is_empty() {
            Arc::make_mut(&mut w.log).set_aside = set_aside;
            w.file = file_id_of(&w.path);
            return w;
        }
        match parse(&bytes) {
            Ok(p) => {
                if p.log.damaged > 0 {
                    eprintln!("play log: {} damaged frame(s) skipped", p.log.damaged);
                }
                if p.good_end < bytes.len() {
                    eprintln!("play log: {} bytes of a torn write cut off", bytes.len() - p.good_end);
                    if let Err(e) = OpenOptions::new().write(true).open(&w.path).and_then(|f| f.set_len(p.good_end as u64)) {
                        eprintln!("play log: can't cut it off: {e}");
                    }
                }
                (w.ids, w.next_id, w.next_session, w.log) = (p.ids, p.next_id, p.next_session, Arc::new(p.log));
            }
            Err(e) => {
                eprintln!("play log: {e}; kept aside, starting a new one");
                let _ = std::fs::rename(&w.path, w.path.with_extension("bin.damaged"));
            }
        }
        Arc::make_mut(&mut w.log).set_aside = w.path.with_extension("bin.damaged").exists();
        w.file = file_id_of(&w.path);
        if w.file.is_some_and(|f| f.len > MAX_BYTES) {
            w.compact();
        }
        w
    }

    /// Reads the file again when it is no longer the one this writer left (see `file`).
    fn follow_file(&mut self) {
        if !self.unwritten.is_empty() {
            return;
        }
        let now = file_id_of(&self.path);
        if now == self.file {
            return;
        }
        eprintln!("play log: the file changed outside pocket-ui ({} bytes, expected {}); read again", now.map_or(0, |f| f.len), self.file.map_or(0, |f| f.len));
        let running = self.running.take();
        #[cfg(test)]
        let fail = self.fail;
        *self = Writer::open(self.path.parent().unwrap_or(Path::new(".")), self.tz);
        #[cfg(test)]
        {
            self.fail = fail;
        }
        // The running session's start is not in that file: its end goes in under a number the file doesn't use.
        self.running = running.map(|r| Run { id: self.new_session(), ..r });
    }

    pub fn log(&self) -> Arc<Log> {
        self.log.clone()
    }

    fn new_session(&mut self) -> u32 {
        let id = self.next_session;
        self.next_session = self.next_session.saturating_add(1);
        id
    }

    /// The id of a string, with the name frame to write first when it is new.
    fn id(&mut self, s: &str, out: &mut Vec<u8>) -> u32 {
        let s = cut(s);
        if let Some(&id) = self.ids.get(s) {
            return id;
        }
        let id = self.next_id;
        self.next_id += 1;
        let mut p = id.to_le_bytes().to_vec();
        p.extend_from_slice(s.as_bytes());
        out.extend(frame(KIND_NAME, &p));
        self.ids.insert(s.to_string(), id);
        id
    }

    fn tz_minutes(&self, at: u64) -> i16 {
        ((self.tz)(at) / 60).clamp(i16::MIN as i32, i16::MAX as i32) as i16
    }

    /// Writes an event: the log in memory first (what `view` shows), then the file.
    pub fn apply(&mut self, e: Event) {
        self.follow_file();
        let mut out = Vec::new();
        match e {
            Event::Started { at, path, name, system, emulator, mono } => {
                let tz = self.tz_minutes(at);
                let id = self.new_session();
                let log = Arc::make_mut(&mut self.log);
                let game = log.game(cut(&path), &name, &system);
                log.sessions.push(Session { game, start: at, tz: tz as i32 * 60, emulator: emulator.clone(), id, ..Default::default() });
                if log.sessions.len() > 1 && log.sessions[log.sessions.len() - 2].start > at {
                    log.sort();
                }
                let (p, n, s, em) = (self.id(&path, &mut out), self.id(&name, &mut out), self.id(&system, &mut out), self.id(&emulator, &mut out));
                let mut b = (at as u32).to_le_bytes().to_vec();
                b.extend(tz.to_le_bytes());
                for v in [p, n, s, em, id] {
                    b.extend(v.to_le_bytes());
                }
                out.extend(frame(KIND_START, &b));
                self.running = Some(Run { id, start: at, path: cut(&path).to_string(), parts: 0, since: mono, since_at: at, asleep: None, slept: Duration::ZERO });
            }
            Event::Ended { at, path, name, system, secs, mono } => {
                // The session: the one running, when it is this game's; else one worked back from the end.
                let (id, start, secs, running) = match self.running.take() {
                    Some(r) if r.path == cut(&path) => {
                        // Light sleep is no play: what the game was stopped for comes off (a suspend never counted).
                        let slept = r.slept + r.asleep.map_or(Duration::ZERO, |t| mono.saturating_sub(t));
                        (r.id, r.start, secs.saturating_sub(whole_secs(slept)), r.asleep.is_none().then_some(r.since_at))
                    }
                    _ => (self.new_session(), at.saturating_sub(secs), secs, None),
                };
                let tz = self.tz_minutes(start);
                let log = Arc::make_mut(&mut self.log);
                let game = log.game(cut(&path), &name, &system);
                match log.session(id) {
                    Some(s) => s.finish(secs, running),
                    None => {
                        log.sessions.push(Session { game, start, tz: tz as i32 * 60, secs, ended: true, id, ..Default::default() });
                        log.sort();
                    }
                }
                let (p, n, s) = (self.id(&path, &mut out), self.id(&name, &mut out), self.id(&system, &mut out));
                let mut b = (start as u32).to_le_bytes().to_vec();
                b.extend(tz.to_le_bytes());
                b.extend((secs.min(u32::MAX as u64) as u32).to_le_bytes());
                for v in [p, n, s, id] {
                    b.extend(v.to_le_bytes());
                }
                out.extend(frame(KIND_END, &b));
            }
            Event::Status { at, path, name, system, status } => {
                let tz = self.tz_minutes(at);
                let log = Arc::make_mut(&mut self.log);
                let game = log.game(cut(&path), &name, &system);
                log.statuses.push(StatusChange { game, at, tz: tz as i32 * 60, status });
                let (p, n, s) = (self.id(&path, &mut out), self.id(&name, &mut out), self.id(&system, &mut out));
                let mut b = (at as u32).to_le_bytes().to_vec();
                b.extend(tz.to_le_bytes());
                for v in [p, n, s] {
                    b.extend(v.to_le_bytes());
                }
                b.push(status_code(status));
                out.extend(frame(KIND_STATUS, &b));
            }
            Event::Paused { at, mono } => out.extend(self.pause(at, mono)),
            Event::Resumed { at, mono } => out.extend(self.resume(at, mono)),
            Event::Suspended { at, secs, mono } => {
                // The game ran up to the suspend and on after it: a stretch ends where it began, the next where it
                // ended. Asleep already (light sleep): the wake places the next stretch.
                if self.running.as_ref().is_some_and(|r| r.asleep.is_none()) {
                    out.extend(self.pause(at.saturating_sub(secs), mono));
                    out.extend(self.resume(at, mono));
                }
            }
        }
        if !out.is_empty() {
            self.append(out);
        }
    }

    /// The running session's stretch ends (sleep): its frame, or nothing when no game runs or it is asleep already.
    fn pause(&mut self, at: u64, mono: Duration) -> Vec<u8> {
        let Some(r) = self.running.as_mut().filter(|r| r.asleep.is_none()) else { return Vec::new() };
        let secs = whole_secs(mono.saturating_sub(r.since));
        let (id, k, since_at) = (r.id, r.parts, r.since_at);
        r.parts += 1;
        r.asleep = Some(mono);
        r.since = mono;
        if let Some(s) = Arc::make_mut(&mut self.log).session(id).filter(|s| !s.ended && s.parts.len() == k as usize) {
            s.pause(since_at, secs);
        }
        frame_u32(KIND_PAUSE, &[id, k, at as u32, secs.min(u32::MAX as u64) as u32])
    }

    /// Awake again: the next stretch begins.
    fn resume(&mut self, at: u64, mono: Duration) -> Vec<u8> {
        let Some(r) = self.running.as_mut() else { return Vec::new() };
        let Some(asleep) = r.asleep.take() else { return Vec::new() };
        r.slept += mono.saturating_sub(asleep);
        r.since = mono;
        r.since_at = at;
        frame_u32(KIND_RESUME, &[r.id, r.parts, at as u32])
    }

    /// Takes over the totals from before the log, once: what the log already counted for a game is taken off. A
    /// migration a power cut stopped half way (carried records, no "migrated" yet) is finished: the games carried
    /// already are left as they are.
    pub fn seed(&mut self, seed: &Seed) {
        self.follow_file();
        if self.log.migrated {
            return;
        }
        let done: HashSet<usize> = self.log.carried.iter().map(|c| c.game).collect();
        if !done.is_empty() {
            println!("play log: finishing the migration a stop cut off ({} games were taken over)", done.len());
        }
        let mut out = Vec::new();
        let mut carried = Vec::new();
        for g in &seed.games {
            let path = cut(&g.path);
            let logged = self.log.find(path);
            if logged.is_some_and(|i| done.contains(&i)) {
                continue;
            }
            let (logged_secs, logged_sessions) = match logged {
                Some(i) => self.log.sessions.iter().filter(|s| s.game == i).fold((0u64, 0u32), |(t, n), s| (t + s.secs, n + (s.secs >= crate::library_meta::MIN_SESSION_SECS) as u32)),
                None => (0, 0),
            };
            let secs = g.secs.saturating_sub(logged_secs);
            let sessions = g.sessions.saturating_sub(logged_sessions);
            // The last play before the log: only when it is before the log's first session of the game.
            let first_logged = logged.and_then(|i| self.log.sessions.iter().find(|s| s.game == i).map(|s| s.start));
            let last = g.last_played.filter(|t| first_logged.is_none_or(|f| *t < f));
            if secs == 0 && sessions == 0 && last.is_none() {
                continue;
            }
            carried.push((g, secs, sessions, last));
        }
        let log = Arc::make_mut(&mut self.log);
        for (g, secs, sessions, last) in &carried {
            let game = log.game(cut(&g.path), &g.name, &g.system);
            log.carried.push(Carried { game, secs: *secs, sessions: *sessions, last_played: *last });
        }
        log.migrated = true;
        for (g, secs, sessions, last) in carried {
            let (p, n, s) = (self.id(&g.path, &mut out), self.id(&g.name, &mut out), self.id(&g.system, &mut out));
            out.extend(frame_u32(KIND_CARRIED, &[p, n, s, secs.min(u32::MAX as u64) as u32, sessions, last.unwrap_or(0).min(u32::MAX as u64) as u32]));
        }
        out.extend(frame(KIND_MIGRATED, &(seed.now as u32).to_le_bytes()));
        self.append(out);
    }

    /// Appends frames (and any that failed before) in one write, then syncs. Compacts past [`MAX_BYTES`]. A write
    /// that fails is cut back off the file, so trying it again never leaves a frame twice.
    fn append(&mut self, frames: Vec<u8>) {
        let retry = !self.unwritten.is_empty();
        self.unwritten.extend(frames);
        if self.unwritten.is_empty() {
            return;
        }
        let known = self.file;
        let result = (|| -> io::Result<FileId> {
            if let Some(dir) = self.path.parent() {
                std::fs::create_dir_all(dir)?;
            }
            let mut f = OpenOptions::new().create(true).append(true).open(&self.path)?;
            let mut before = file_id(&f.metadata()?);
            // An earlier try that failed half way and could not be cut back then: cut back now (this file only).
            if let Some(k) = known.filter(|k| retry && (k.dev, k.ino) == (before.dev, before.ino) && before.len > k.len) {
                f.set_len(k.len)?;
                before.len = k.len;
            }
            if before.len == 0 {
                self.unwritten.splice(0..0, header());
            }
            let written = (|| -> io::Result<()> {
                #[cfg(test)]
                if self.fail.0 > 0 {
                    self.fail.0 -= 1;
                    if self.fail.1 {
                        f.write_all(&self.unwritten[..self.unwritten.len() / 2])?;
                    }
                    return Err(io::Error::other("no space left (test)"));
                }
                f.write_all(&self.unwritten)?;
                f.sync_data()
            })();
            if let Err(e) = written {
                // Half a write would be read as frames (twice, once this is tried again) or as damage.
                if let Err(cut_err) = f.set_len(before.len) {
                    eprintln!("play log: can't cut a failed write off ({cut_err}); done before the next one");
                }
                return Err(e);
            }
            Ok(file_id(&f.metadata()?))
        })();
        match result {
            Ok(id) => {
                self.unwritten.clear();
                self.file = Some(id);
                if id.len > MAX_BYTES {
                    self.compact();
                }
            }
            Err(e) => {
                eprintln!("play log: not written yet ({e}); tried again with the next session");
                // A header put in front for a fresh file goes again next time.
                if self.unwritten.starts_with(MAGIC) {
                    self.unwritten.drain(..HEADER);
                }
            }
        }
    }

    /// Writes what is still held back (a full card earlier), trying again for up to `within`: the flush at exit.
    /// True when everything handed over is on the card (each append is synced).
    pub fn flush(&mut self, within: Duration) -> bool {
        let until = Instant::now() + within;
        loop {
            if self.unwritten.is_empty() {
                return true;
            }
            self.append(Vec::new());
            if self.unwritten.is_empty() {
                println!("play log: what was held back is written");
                return true;
            }
            if Instant::now() + FLUSH_RETRY > until {
                eprintln!("play log: {} bytes (the last sessions) could not be written before exit", self.unwritten.len());
                return false;
            }
            std::thread::sleep(FLUSH_RETRY);
        }
    }

    /// Rewrites the file without its oldest sessions, atomically, down to about [`COMPACT_TO`].
    fn compact(&mut self) {
        let mut log = (*self.log).clone();
        // 66 bytes a session (start and end); names and the rest are small next to it.
        let keep = (COMPACT_TO as usize / SESSION_BYTES).min(log.sessions.len());
        let dropped = log.sessions.len() - keep;
        log.sessions.drain(..dropped);
        let bytes = encode(&log);
        match crate::fsutil::write_atomic(&self.path, &bytes) {
            Ok(()) => {
                println!("play log: {dropped} oldest sessions left out, {} bytes now", bytes.len());
                self.file = file_id_of(&self.path);
                if let Ok(mut p) = parse(&bytes) {
                    p.log.set_aside = self.log.set_aside;
                    // Numbers already given out stay given out (the running session keeps its own).
                    let next_session = p.next_session.max(self.next_session);
                    (self.ids, self.next_id, self.next_session, self.log) = (p.ids, p.next_id, next_session, Arc::new(p.log));
                }
            }
            Err(e) => eprintln!("play log: can't compact: {e}"),
        }
    }
}

/// A whole file for a log (compaction, tests, fake data): names first, then every record. Sessions keep their
/// numbers (one without gets the next free one) and their stretches.
pub fn encode(log: &Log) -> Vec<u8> {
    let mut ids: HashMap<String, u32> = HashMap::new();
    let mut names = Vec::new();
    let mut body = Vec::new();
    let mut id = |s: &str, names: &mut Vec<u8>| -> u32 {
        let s = cut(s);
        if let Some(&i) = ids.get(s) {
            return i;
        }
        let i = ids.len() as u32;
        let mut p = i.to_le_bytes().to_vec();
        p.extend_from_slice(s.as_bytes());
        names.extend(frame(KIND_NAME, &p));
        ids.insert(s.to_string(), i);
        i
    };
    let tzm = |tz: i32| ((tz / 60).clamp(i16::MIN as i32, i16::MAX as i32) as i16).to_le_bytes();
    for c in &log.carried {
        let g = &log.games[c.game];
        let fields = [id(&g.path, &mut names), id(&g.name, &mut names), id(&g.system, &mut names), c.secs.min(u32::MAX as u64) as u32, c.sessions, c.last_played.unwrap_or(0) as u32];
        body.extend(frame_u32(KIND_CARRIED, &fields));
    }
    if log.migrated {
        body.extend(frame(KIND_MIGRATED, &0u32.to_le_bytes()));
    }
    for st in &log.statuses {
        let g = &log.games[st.game];
        let mut b = (st.at as u32).to_le_bytes().to_vec();
        b.extend(tzm(st.tz));
        for v in [id(&g.path, &mut names), id(&g.name, &mut names), id(&g.system, &mut names)] {
            b.extend(v.to_le_bytes());
        }
        b.push(status_code(st.status));
        body.extend(frame(KIND_STATUS, &b));
    }
    let mut next_session = log.sessions.iter().map(|s| s.id).max().unwrap_or(0);
    for s in &log.sessions {
        let g = &log.games[s.game];
        let n = if s.id != 0 {
            s.id
        } else {
            next_session += 1;
            next_session
        };
        let (p, nm, sy) = (id(&g.path, &mut names), id(&g.name, &mut names), id(&g.system, &mut names));
        let mut b = (s.start as u32).to_le_bytes().to_vec();
        b.extend(tzm(s.tz));
        for v in [p, nm, sy, id(&s.emulator, &mut names), n] {
            b.extend(v.to_le_bytes());
        }
        body.extend(frame(KIND_START, &b));
        // Every stretch ends in a pause, so the end adds none: the stretches read back as they were.
        for (k, part) in s.parts.iter().enumerate() {
            if k > 0 {
                body.extend(frame_u32(KIND_RESUME, &[n, k as u32, part.at as u32]));
            }
            body.extend(frame_u32(KIND_PAUSE, &[n, k as u32, (part.at + part.secs) as u32, part.secs.min(u32::MAX as u64) as u32]));
        }
        if s.ended {
            let mut b = (s.start as u32).to_le_bytes().to_vec();
            b.extend(tzm(s.tz));
            b.extend((s.secs.min(u32::MAX as u64) as u32).to_le_bytes());
            for v in [p, nm, sy, n] {
                b.extend(v.to_le_bytes());
            }
            body.extend(frame(KIND_END, &b));
        }
    }
    let mut out = header().to_vec();
    out.extend(names);
    out.extend(body);
    out
}

/// Builds a log from parts (fake data for tests, examples and snapshots). Sessions are sorted by start and numbered
/// (those without a number) as a file would number them.
pub fn build(games: Vec<Game>, mut sessions: Vec<Session>, statuses: Vec<StatusChange>, carried: Vec<Carried>) -> Log {
    sessions.sort_by_key(|s| s.start);
    let mut next = sessions.iter().map(|s| s.id).max().unwrap_or(0);
    for s in sessions.iter_mut().filter(|s| s.id == 0) {
        next += 1;
        s.id = next;
    }
    let by_path = games.iter().enumerate().map(|(i, g)| (g.path.clone(), i)).collect();
    let mut log = Log { games, sessions, statuses, carried, migrated: true, damaged: 0, set_aside: false, by_path };
    log.sort();
    log
}

// ---- the process-wide recorder -------------------------------------------------------------------------------------

/// Makes the migration's seed, on the writer's thread (`seed_with`).
pub type MakeSeed = Box<dyn FnOnce() -> Seed + Send>;

enum Cmd {
    Event(Event),
    Seed(MakeSeed),
    /// Write everything handed over; true when it is on the card.
    Flush(Sender<bool>),
}

#[derive(Default)]
struct Recorder {
    dir: Option<PathBuf>,
    enabled: bool,
    tx: Option<Sender<Cmd>>,
    /// Commands from before `open`.
    queue: Vec<Cmd>,
    seed_sent: bool,
    /// [`NO_HISTORY`] is set: this process records nothing, whatever Settings says (measurement runs).
    held_off: bool,
}

impl Recorder {
    /// A command for the writer (or for the queue before `open`); dropped with Play history off or held off.
    fn send(&mut self, cmd: Cmd) {
        if !self.enabled || self.held_off {
            return;
        }
        match self.tx.as_ref() {
            Some(tx) => {
                let _ = tx.send(cmd);
            }
            None => self.queue.push(cmd),
        }
    }
}

/// `POCKETUI_NO_HISTORY=1`: nothing of this run goes into the play log (no writer thread, no migration). The
/// latency measurements (`scripts/latency.py`) set it, so their game starts never land in the player's history.
pub const NO_HISTORY: &str = "POCKETUI_NO_HISTORY";

/// Whether a value of [`NO_HISTORY`] holds recording off: set, and not empty or "0".
fn holds_off(value: Option<&std::ffi::OsStr>) -> bool {
    value.is_some_and(|v| !v.is_empty() && v != "0")
}

static RECORDER: Mutex<Recorder> = Mutex::new(Recorder { dir: None, enabled: false, tx: None, queue: Vec::new(), seed_sent: false, held_off: false });
/// The log as the writer has it, for the pages: None until it was read. With Settings → Sync → Library data on it
/// also holds the sessions of the player's other devices ([`set_extra`]).
static VIEW: Mutex<Option<Arc<Log>>> = Mutex::new(None);
/// This device's own log as the writer has it (what [`VIEW`] is without the other devices' sessions), and those
/// sessions: one lock, so a publish of the writer and a merge of `library_sync` never cross.
static PARTS: Mutex<Parts> = Mutex::new(Parts { own: None, extra: None });

struct Parts {
    own: Option<Arc<Log>>,
    extra: Option<Arc<Extra>>,
}

fn recorder() -> std::sync::MutexGuard<'static, Recorder> {
    RECORDER.lock().unwrap_or_else(|e| e.into_inner())
}

/// The monotonic clock the stretches of a session are measured with (it stands still in a suspend, as the game's
/// own time in `main.rs` does).
fn mono() -> Duration {
    static BASE: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    BASE.get_or_init(Instant::now).elapsed()
}

/// Where the log lives and whether to record (Settings → Curator → Play history). No I/O.
pub fn configure(data_dir: &Path, enabled: bool) {
    let mut r = recorder();
    r.dir = Some(data_dir.to_path_buf());
    r.enabled = enabled;
    r.held_off = holds_off(std::env::var_os(NO_HISTORY).as_deref());
    if r.held_off {
        println!("play log: {NO_HISTORY} is set, nothing of this run is recorded");
    }
    drop(r);
    mono();
}

/// Play history switched on or off: off records nothing new (what is there stays).
pub fn set_enabled(on: bool) {
    recorder().enabled = on;
}

pub fn enabled() -> bool {
    recorder().enabled
}

/// The writer's loop: commands in order; `publish` gets the log after each change. A game's start (which carries
/// its own time) is written a few seconds after it, or at once when anything else comes (its end, a flush at exit).
fn serve(mut w: Writer, rx: Receiver<Cmd>, publish: impl Fn(Arc<Log>)) {
    publish(w.log());
    let mut next: Option<Cmd> = None;
    loop {
        let Some(cmd) = next.take().or_else(|| rx.recv().ok()) else { break };
        if matches!(cmd, Cmd::Event(Event::Started { .. })) {
            next = rx.recv_timeout(START_QUIET).ok();
        }
        match cmd {
            Cmd::Event(e) => w.apply(e),
            Cmd::Seed(make) => w.seed(&make()),
            // Not acknowledged before what a full card held back was tried again and synced (review
            // SOL-2026-10-24 #5).
            Cmd::Flush(done) => {
                let _ = done.send(w.flush(FLUSH_FOR));
                continue;
            }
        }
        publish(w.log());
    }
}

/// Starts the writer thread (once, after the first frame): it reads the file, then writes what waited.
pub fn open() {
    let mut r = recorder();
    if r.tx.is_some() || !r.enabled || r.held_off {
        return;
    }
    let Some(dir) = r.dir.clone() else { return };
    let (tx, rx) = channel::<Cmd>();
    let spawned = std::thread::Builder::new().name("play-log".into()).spawn(move || {
        #[cfg(target_os = "linux")]
        // SAFETY: setpriority on this thread's own id only changes its scheduling.
        unsafe {
            let tid = libc::syscall(libc::SYS_gettid) as libc::id_t;
            libc::setpriority(libc::PRIO_PROCESS, tid, 10);
        }
        serve(Writer::open(&dir, local_offset), rx, |log| publish(Some(log), None));
    });
    if spawned.is_err() {
        eprintln!("play log: no thread; nothing is recorded this time");
        return;
    }
    for cmd in std::mem::take(&mut r.queue) {
        let _ = tx.send(cmd);
    }
    r.tx = Some(tx);
}

fn send(cmd: Cmd) {
    recorder().send(cmd);
}

fn s(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

/// A game started (`emulator`: "pocket-core", "retroarch", a standalone's name; `core` may be empty).
pub fn started(system: &str, name: &str, path: &Path, emulator: &str, core: &str) {
    let emulator = if core.is_empty() || core == emulator { emulator.to_string() } else { format!("{emulator}/{}", core.trim_end_matches("_libretro")) };
    send(Cmd::Event(Event::Started { at: crate::recents::now_secs(), path: s(path), name: name.into(), system: system.into(), emulator, mono: mono() }));
}

/// The game ended after `secs` seconds (as the recent games count them).
pub fn ended(system: &str, name: &str, path: &Path, secs: u64) {
    send(Cmd::Event(Event::Ended { at: crate::recents::now_secs(), path: s(path), name: name.into(), system: system.into(), secs, mono: mono() }));
}

/// The game failed at once and never was on the screen (`fallback::never_started`: a zip or a patch refused, a
/// missing BIOS, a file that is no game). Its start is closed with no time: no session, no day played, not in a
/// period's games (`play_history`). Left open, it read as "pocket-ui went down while it ran": a session without time.
pub fn never_started(system: &str, name: &str, path: &Path) {
    ended(system, name, path, 0);
}

/// The player set the game's status.
pub fn status(system: &str, name: &str, path: &Path, status: Status) {
    send(Cmd::Event(Event::Status { at: crate::recents::now_secs(), path: s(path), name: name.into(), system: system.into(), status }));
}

/// Sleep began (`sleep.rs`: light sleep, which a suspend of pocket-ui's own always follows): the running game's
/// time stops here. Nothing when no game runs.
pub fn paused() {
    send(Cmd::Event(Event::Paused { at: crate::recents::now_secs(), mono: mono() }));
}

/// Awake again (`sleep.rs`): the running game's next stretch begins.
pub fn resumed() {
    send(Cmd::Event(Event::Resumed { at: crate::recents::now_secs(), mono: mono() }));
}

/// A suspend pocket-ui didn't start (logind) just ended after `secs` seconds (`sleep.rs`'s watcher, any thread).
pub fn suspended(secs: u64) {
    send(Cmd::Event(Event::Suspended { at: crate::recents::now_secs(), secs, mono: mono() }));
}

/// True once, when the writer has read a log that has not taken over the totals from before yet.
pub fn wants_seed() -> bool {
    let r = recorder();
    if r.seed_sent || r.tx.is_none() {
        return false;
    }
    drop(r);
    VIEW.lock().unwrap_or_else(|e| e.into_inner()).as_ref().is_some_and(|l| !l.migrated)
}

/// Hands over the totals from before (once; see the module's notes).
pub fn seed(seed: Seed) {
    seed_with(Box::new(move || seed));
}

/// Hands over how to make the totals from before: `make` runs on the writer's thread, not the caller's.
pub fn seed_with(make: MakeSeed) {
    recorder().seed_sent = true;
    send(Cmd::Seed(make));
}

/// The log as written so far in this process; None before the writer has read the file (moments after the first
/// frame) or with Play history off.
pub fn view() -> Option<Arc<Log>> {
    VIEW.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

/// The log of `data_dir` for a reader in any thread (the web companion): what the writer has in memory when it is
/// this process's log (an `Arc` clone, no I/O), else the file. None when there is no log, or when Play history is
/// switched off for this data directory (what was recorded stays private then too).
pub fn log_for(data_dir: &Path) -> Option<Arc<Log>> {
    let (mine, enabled) = {
        let r = recorder();
        (r.dir.as_deref() == Some(data_dir), r.enabled)
    };
    if mine && !enabled {
        return None;
    }
    if let Some(v) = view().filter(|_| mine) {
        return Some(v);
    }
    if !data_dir.join(FILE).exists() {
        return None;
    }
    Some(Arc::new(read(data_dir)))
}

/// Sessions and status changes from the player's other devices (`library_sync`, Settings → Sync → Library data):
/// `games` are the games they belong to, with this device's path where it has the game. Shown with the own log in
/// every page and the web companion; never written to this device's file.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Extra {
    pub games: Vec<Game>,
    /// `game` is an index into `games`.
    pub sessions: Vec<Session>,
    pub statuses: Vec<StatusChange>,
}

impl Extra {
    pub fn is_empty(&self) -> bool {
        self.sessions.is_empty() && self.statuses.is_empty()
    }
}

/// The own log with the other devices' sessions: their games found by path (or added), their sessions numbered
/// after the own ones, everything in time order.
pub fn combine(own: &Log, extra: &Extra) -> Log {
    let mut log = own.clone();
    let index: Vec<usize> = extra
        .games
        .iter()
        .map(|g| match log.find(&g.path) {
            Some(i) => i,
            None => log.game(&g.path, &g.name, &g.system),
        })
        .collect();
    let mut next = log.sessions.iter().map(|s| s.id).max().unwrap_or(0);
    for s in &extra.sessions {
        let Some(&game) = index.get(s.game) else { continue };
        next += 1;
        log.sessions.push(Session { game, id: next, ..s.clone() });
    }
    for c in &extra.statuses {
        if let Some(&game) = index.get(c.game) {
            log.statuses.push(StatusChange { game, ..c.clone() });
        }
    }
    log.sort();
    log
}

/// Keeps the newest own log and/or other devices' sessions and puts their sum where the pages read it.
fn publish(own: Option<Arc<Log>>, extra: Option<Option<Arc<Extra>>>) {
    let mut parts = PARTS.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(own) = own {
        parts.own = Some(own);
    }
    if let Some(extra) = extra {
        parts.extra = extra;
    }
    let view = match (&parts.own, &parts.extra) {
        (Some(own), Some(extra)) if !extra.is_empty() => Some(Arc::new(combine(own, extra))),
        (Some(own), _) => Some(own.clone()),
        // No own log yet (the writer hasn't read it, Play history is off): the view stays as it is.
        (None, _) => return,
    };
    *VIEW.lock().unwrap_or_else(|e| e.into_inner()) = view;
}

/// The other devices' sessions to show with the own log (None: none any more, Library data was switched off).
/// Works out the sum on the caller's thread (`library_sync`'s), never the UI's.
pub fn set_extra(extra: Option<Extra>) {
    publish(None, Some(extra.map(Arc::new)));
}

/// This device's own log, without other devices' sessions: what the writer has in memory, else the file. None with
/// Play history off.
pub fn own_log(data_dir: &Path) -> Option<Arc<Log>> {
    let (mine, enabled) = {
        let r = recorder();
        (r.dir.as_deref() == Some(data_dir), r.enabled)
    };
    if mine && !enabled {
        return None;
    }
    let held = PARTS.lock().unwrap_or_else(|e| e.into_inner()).own.clone();
    if let Some(own) = held.filter(|_| mine) {
        return Some(own);
    }
    Some(Arc::new(read(data_dir)))
}

/// Puts a made-up log in place of the writer's (snapshots, `--snapshot curator:history`): what the pages read.
pub fn show_instead(log: Log) {
    *VIEW.lock().unwrap_or_else(|e| e.into_inner()) = Some(Arc::new(log));
}

/// Waits (at most `within`) until the writer has read the file: for a page opened in the first moments.
pub fn view_within(within: Duration) -> Option<Arc<Log>> {
    let until = std::time::Instant::now() + within;
    loop {
        if let Some(v) = view() {
            return Some(v);
        }
        if std::time::Instant::now() >= until || recorder().tx.is_none() {
            return None;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// Waits until everything handed over is on the card (pocket-ui does before it exits): what a full card held back
/// is tried again and synced first. At most two seconds; what could not be written is said in the log.
pub fn settle() {
    let tx = recorder().tx.clone();
    if let Some(tx) = tx {
        let (done, wait) = channel();
        if tx.send(Cmd::Flush(done)).is_ok() {
            match wait.recv_timeout(Duration::from_secs(2)) {
                Ok(true) => {}
                Ok(false) => eprintln!("play log: not everything is on the card at exit"),
                Err(_) => eprintln!("play log: still writing at exit; the last records may be lost"),
            }
        }
    }
}

/// Reads a file's bytes (for the backup and tests): the raw log.
pub fn raw(data_dir: &Path) -> Option<Vec<u8>> {
    let mut v = Vec::new();
    File::open(data_dir.join(FILE)).ok()?.read_to_end(&mut v).ok()?;
    Some(v)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;
    use std::fs;

    fn dir(name: &str) -> crate::testutil::TestDir {
        let d = TestDir::new(&format!("playlog-{name}"));
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn vienna(_: u64) -> i32 {
        7200
    }

    const T0: u64 = 1_790_000_000; // 21 September 2026, 14:13 UTC

    fn start(w: &mut Writer, at: u64, path: &str) {
        w.apply(Event::Started { at, path: path.into(), name: "Tetris".into(), system: "gb".into(), emulator: "pocket-core/gambatte".into(), mono: Duration::from_secs(at) });
    }

    fn end(w: &mut Writer, at: u64, path: &str, secs: u64) {
        w.apply(Event::Ended { at, path: path.into(), name: "Tetris".into(), system: "gb".into(), secs, mono: Duration::from_secs(at) });
    }

    #[test]
    fn sessions_survive_a_restart_and_pair_start_with_end() {
        let d = dir("restart");
        let mut w = Writer::open(&d, vienna);
        start(&mut w, T0, "/roms/gb/Tetris.gb");
        end(&mut w, T0 + 700, "/roms/gb/Tetris.gb", 600);
        w.apply(Event::Status { at: T0 + 800, path: "/roms/gb/Tetris.gb".into(), name: "Tetris".into(), system: "gb".into(), status: Status::Completed });
        start(&mut w, T0 + 900, "/roms/gba/Zelda.gba");
        let read_back = read(&d);
        assert_eq!(read_back.sessions.len(), 2);
        let s = &read_back.sessions[0];
        assert_eq!((s.start, s.secs, s.ended, s.tz, s.emulator.as_str()), (T0, 600, true, 7200, "pocket-core/gambatte"));
        assert_eq!(read_back.games[s.game].path, "/roms/gb/Tetris.gb");
        // The running one, as a crash would leave it: a session without time.
        let open = &read_back.sessions[1];
        assert_eq!((open.ended, open.secs), (false, 0));
        assert_eq!(read_back.statuses.len(), 1);
        assert_eq!(read_back.statuses[0].status, Status::Completed);
        // What the writer keeps in memory is what the file says.
        assert_eq!(*w.log(), read_back);
        // A writer opened again goes on with the same name table: no new names for a known game.
        let before = fs::metadata(d.join(FILE)).unwrap().len();
        let mut w = Writer::open(&d, vienna);
        start(&mut w, T0 + 2000, "/roms/gb/Tetris.gb");
        let grown = fs::metadata(d.join(FILE)).unwrap().len() - before;
        assert_eq!(grown, 33, "a start of a known game is one 33-byte frame");
        end(&mut w, T0 + 2100, "/roms/gb/Tetris.gb", 100);
        assert_eq!(fs::metadata(d.join(FILE)).unwrap().len() - before, SESSION_BYTES as u64, "and its end one more");
    }

    #[test]
    fn a_torn_last_frame_is_cut_off_and_a_damaged_one_skipped() {
        let d = dir("torn");
        let mut w = Writer::open(&d, vienna);
        for k in 0..5 {
            start(&mut w, T0 + k * 1000, "/roms/gb/Tetris.gb");
            end(&mut w, T0 + k * 1000 + 500, "/roms/gb/Tetris.gb", 400);
        }
        let good = fs::read(d.join(FILE)).unwrap();
        // A power cut in the middle of the next write: half a frame at the end.
        let mut torn = good.clone();
        torn.extend_from_slice(&frame(KIND_START, &[1, 2, 3, 4, 5, 6, 7, 8])[..6]);
        fs::write(d.join(FILE), &torn).unwrap();
        assert_eq!(read(&d).sessions.len(), 5, "the reader stops at the torn frame");
        let mut w = Writer::open(&d, vienna);
        assert_eq!(fs::read(d.join(FILE)).unwrap(), good, "the writer cuts it off before it appends");
        start(&mut w, T0 + 9000, "/roms/gb/Tetris.gb");
        end(&mut w, T0 + 9100, "/roms/gb/Tetris.gb", 100);
        assert_eq!(read(&d).sessions.len(), 6);
        // A few bytes gone bad in the middle (the card): that frame is skipped, everything after it is read.
        let mut bad = fs::read(d.join(FILE)).unwrap();
        let mid = bad.len() / 2;
        for b in &mut bad[mid..mid + 3] {
            *b ^= 0x5a;
        }
        fs::write(d.join(FILE), &bad).unwrap();
        let log = read(&d);
        assert!(log.damaged >= 1, "the damage is noticed");
        assert!(log.sessions.len() >= 5, "only the damaged frame's session is touched: {}", log.sessions.len());
        assert!(log.sessions.iter().any(|s| s.start == T0 + 9000 && s.ended), "frames after the damage are read");
        // Zeros where the file was (a power cut while it was made): kept aside, a new log begins.
        fs::write(d.join(FILE), [0u8; 64]).unwrap();
        let mut w = Writer::open(&d, vienna);
        assert!(d.join("play-log.bin.damaged").exists());
        start(&mut w, T0, "/roms/gb/Tetris.gb");
        assert_eq!(read(&d).sessions.len(), 1);
    }

    #[test]
    fn an_end_without_its_start_is_still_a_session_and_a_full_card_is_tried_again() {
        let d = dir("fullcard");
        let mut w = Writer::open(&d, vienna);
        // pocket-ui restarted while a game ran: the end comes without the writer having seen the start.
        end(&mut w, T0 + 600, "/roms/gb/Tetris.gb", 600);
        let log = read(&d);
        assert_eq!((log.sessions[0].start, log.sessions[0].secs), (T0, 600), "worked back from the end");
        w.fail = (1, false);
        start(&mut w, T0 + 1000, "/roms/gb/Tetris.gb");
        assert_eq!(read(&d).sessions.len(), 1, "not on the SD card yet");
        assert_eq!(w.log().sessions.len(), 2, "but kept");
        end(&mut w, T0 + 1300, "/roms/gb/Tetris.gb", 300);
        let log = read(&d);
        assert_eq!(log.sessions.len(), 2, "written with the next one");
        assert_eq!((log.sessions[1].secs, log.sessions[1].ended), (300, true));
    }

    #[test]
    fn the_totals_from_before_are_taken_over_once_without_counting_twice() {
        let d = dir("seed");
        let mut w = Writer::open(&d, vienna);
        // A session already in the log (it ended before the seed was gathered, so library.json counts it too).
        start(&mut w, T0, "/roms/gb/Tetris.gb");
        end(&mut w, T0 + 700, "/roms/gb/Tetris.gb", 600);
        let seed = Seed {
            now: T0 + 1000,
            games: vec![
                SeedGame { path: "/roms/gb/Tetris.gb".into(), name: "Tetris".into(), system: "gb".into(), secs: 3600, sessions: 4, last_played: Some(T0 + 700) },
                SeedGame { path: "/roms/snes/Chrono Trigger.sfc".into(), name: "Chrono Trigger".into(), system: "snes".into(), secs: 36_000, sessions: 12, last_played: Some(T0 - 200 * 86_400) },
                SeedGame { path: "/roms/nes/Nothing.nes".into(), name: "Nothing".into(), system: "nes".into(), secs: 0, sessions: 0, last_played: None },
            ],
        };
        w.seed(&seed);
        w.seed(&seed);
        let log = read(&d);
        assert!(log.migrated);
        assert_eq!(log.carried.len(), 2, "once, and nothing for a game never played");
        let tetris = log.carried.iter().find(|c| log.games[c.game].name == "Tetris").unwrap();
        assert_eq!((tetris.secs, tetris.sessions, tetris.last_played), (3000, 3, None), "the logged session is taken off; its end is no earlier play");
        let chrono = log.carried.iter().find(|c| log.games[c.game].name == "Chrono Trigger").unwrap();
        assert_eq!((chrono.secs, chrono.sessions, chrono.last_played), (36_000, 12, Some(T0 - 200 * 86_400)));
        // A writer opened again knows it is done.
        let mut w = Writer::open(&d, vienna);
        w.seed(&seed);
        assert_eq!(read(&d).carried.len(), 2);
    }

    /// Review SOL-2026-10-24 #3: a power cut after some carried records but before the "migrated" frame. The
    /// migration runs again on the next start and finishes it, without carrying any game twice.
    #[test]
    fn an_interrupted_migration_is_finished_without_counting_twice() {
        let d = dir("halfseed");
        let mut w = Writer::open(&d, vienna);
        start(&mut w, T0, "/roms/gb/Tetris.gb");
        end(&mut w, T0 + 700, "/roms/gb/Tetris.gb", 600);
        let seed = Seed {
            now: T0 + 1000,
            games: vec![
                SeedGame { path: "/roms/gb/Tetris.gb".into(), name: "Tetris".into(), system: "gb".into(), secs: 3600, sessions: 4, last_played: None },
                SeedGame { path: "/roms/snes/Chrono Trigger.sfc".into(), name: "Chrono Trigger".into(), system: "snes".into(), secs: 36_000, sessions: 12, last_played: None },
            ],
        };
        w.seed(&seed);
        // The power goes right after Tetris's carried record reached the card.
        let bytes = fs::read(d.join(FILE)).unwrap();
        let mut at = HEADER;
        while let Some((kind, _, next)) = frame_at(&bytes, at) {
            at = next;
            if kind == KIND_CARRIED {
                break;
            }
        }
        fs::write(d.join(FILE), &bytes[..at]).unwrap();
        let mut w = Writer::open(&d, vienna);
        assert!(!w.log().migrated, "half a migration is no migration");
        assert_eq!(w.log().carried.len(), 1);
        w.seed(&seed);
        for log in [read(&d), (*w.log()).clone()] {
            assert!(log.migrated);
            let carried = |name: &str| log.carried.iter().filter(|c| log.games[c.game].name == name).map(|c| c.secs).collect::<Vec<_>>();
            assert_eq!((carried("Tetris"), carried("Chrono Trigger")), (vec![3000], vec![36_000]), "each game carried once");
            assert_eq!(crate::play_history::all_time(&log).secs, 600 + 3000 + 36_000);
        }
        // A file that already carries a game twice (written by the pocket-ui before this fix) counts it once.
        let mut twice = fs::read(d.join(FILE)).unwrap();
        let carried_frames: Vec<Vec<u8>> = {
            let (mut at, mut v) = (HEADER, Vec::new());
            while let Some((kind, _, next)) = frame_at(&twice, at) {
                if kind == KIND_CARRIED {
                    v.push(twice[at..next].to_vec());
                }
                at = next;
            }
            v
        };
        for f in carried_frames {
            twice.extend(f);
        }
        fs::write(d.join(FILE), &twice).unwrap();
        assert_eq!(crate::play_history::all_time(&read(&d)).secs, 600 + 3000 + 36_000, "a replayed record counts once");
    }

    /// Review SOL-2026-10-24 #13: the clock was reset and the same game starts again at a time an earlier session
    /// already started at. Both stay sessions after a restart.
    #[test]
    fn two_sessions_with_the_same_start_time_stay_two() {
        let d = dir("samestart");
        let mut w = Writer::open(&d, vienna);
        start(&mut w, T0, "/roms/gb/Tetris.gb");
        end(&mut w, T0 + 700, "/roms/gb/Tetris.gb", 600);
        start(&mut w, T0, "/roms/gb/Tetris.gb");
        end(&mut w, T0 + 300, "/roms/gb/Tetris.gb", 300);
        let log = read(&d);
        assert_eq!(log.sessions.iter().map(|s| (s.start, s.secs, s.ended)).collect::<Vec<_>>(), [(T0, 600, true), (T0, 300, true)]);
        assert_eq!(*w.log(), log);
    }

    /// Review SOL-2026-10-24 #4: a log replaced by another one of exactly the same length (other names behind the
    /// same ids) is still noticed and read again, so a new session is written under its own name.
    #[test]
    fn a_file_replaced_by_one_of_the_same_length_is_read_again() {
        // A log of one session of `name` (18-byte path, 6-letter name: the same lengths for every game here).
        let one = |d: &Path, name: &str| {
            let mut w = Writer::open(d, vienna);
            let path = format!("/roms/gb/{name}.gb");
            w.apply(Event::Started { at: T0, path: path.clone(), name: name.into(), system: "gb".into(), emulator: "pocket-core/gambatte".into(), mono: Duration::from_secs(T0) });
            w.apply(Event::Ended { at: T0 + 700, path, name: name.into(), system: "gb".into(), secs: 600, mono: Duration::from_secs(T0 + 600) });
            w
        };
        for (how, name) in [("in place", "Zeldaa"), ("renamed over it", "Pacman")] {
            let d = dir(&format!("samelength-{name}"));
            let mut w = one(&d, "Tetris");
            let bytes = fs::read(one(&dir(&format!("samelength-other-{name}")), name).path.clone()).unwrap();
            assert_eq!(bytes.len(), fs::read(d.join(FILE)).unwrap().len(), "{how}: the same length");
            if how == "in place" {
                // Copied back with its time kept (`cp -p`, rsync): the same inode, length and modification time
                // as the writer's own last write. Only its change time tells.
                let mtime = fs::metadata(d.join(FILE)).unwrap().modified().unwrap();
                std::thread::sleep(Duration::from_millis(20));
                fs::write(d.join(FILE), &bytes).unwrap();
                fs::File::options().write(true).open(d.join(FILE)).unwrap().set_modified(mtime).unwrap();
            } else {
                fs::write(d.join("replacement"), &bytes).unwrap();
                fs::rename(d.join("replacement"), d.join(FILE)).unwrap();
            }
            start(&mut w, T0 + 1000, "/roms/gb/Tetris.gb");
            let log = read(&d);
            let names: Vec<&str> = log.sessions.iter().map(|s| log.games[s.game].name.as_str()).collect();
            assert_eq!(names, [name, "Tetris"], "{how}: the new session is Tetris, under its own name");
            assert_eq!(*w.log(), log, "{how}");
        }
    }

    /// Review SOL-2026-10-24 #3, the other way to count twice: a write that fails half way (the card) and is tried
    /// again. What reached the card of the failed try is cut back off, so every record is there once.
    #[test]
    fn a_write_that_failed_half_way_is_not_there_twice() {
        let d = dir("halfwrite");
        let mut w = Writer::open(&d, vienna);
        start(&mut w, T0, "/roms/gb/Tetris.gb");
        end(&mut w, T0 + 700, "/roms/gb/Tetris.gb", 600);
        let seed = Seed { now: T0 + 1000, games: (0..6).map(|i| SeedGame { path: format!("/roms/snes/Game {i}.sfc"), name: format!("Game {i}"), system: "snes".into(), secs: 3600, sessions: 2, last_played: None }).collect() };
        w.fail = (1, true);
        w.seed(&seed);
        start(&mut w, T0 + 2000, "/roms/gb/Tetris.gb");
        end(&mut w, T0 + 2700, "/roms/gb/Tetris.gb", 600);
        let log = read(&d);
        assert_eq!((log.damaged, log.carried.len(), log.sessions.len()), (0, 6, 2), "nothing damaged, nothing twice");
        assert_eq!(crate::play_history::all_time(&log).secs, 6 * 3600 + 1200);
        assert_eq!(*w.log(), log);
        // A session's frames written twice (a try whose half could not be cut off) are still one session.
        let mut twice = fs::read(d.join(FILE)).unwrap();
        let tail = twice[twice.len() - 2 * 33..].to_vec();
        twice.extend(tail);
        fs::write(d.join(FILE), &twice).unwrap();
        assert_eq!(read(&d).sessions.len(), 2);
    }

    /// Review SOL-2026-10-24 #5: the flush at exit writes what a full card held back before it says it is done.
    #[test]
    fn the_flush_at_exit_writes_what_was_held_back() {
        let d = dir("flush");
        let mut w = Writer::open(&d, vienna);
        w.fail = (1, false);
        let (tx, rx) = channel();
        let writer = std::thread::spawn(move || serve(w, rx, |_| {}));
        tx.send(Cmd::Event(Event::Started { at: T0, path: "/roms/gb/Tetris.gb".into(), name: "Tetris".into(), system: "gb".into(), emulator: String::new(), mono: Duration::ZERO })).unwrap();
        let (done, wait) = channel();
        tx.send(Cmd::Flush(done)).unwrap();
        // The start could not be written (the card was full) and nothing came after it: the flush writes it.
        assert_eq!(wait.recv_timeout(Duration::from_secs(5)), Ok(true));
        assert_eq!(read(&d).sessions.iter().map(|s| (s.start, s.ended)).collect::<Vec<_>>(), [(T0, false)]);
        tx.send(Cmd::Event(Event::Ended { at: T0 + 700, path: "/roms/gb/Tetris.gb".into(), name: "Tetris".into(), system: "gb".into(), secs: 600, mono: Duration::from_secs(600) })).unwrap();
        let (done, wait) = channel();
        tx.send(Cmd::Flush(done)).unwrap();
        assert_eq!(wait.recv_timeout(Duration::from_secs(5)), Ok(true));
        assert_eq!(read(&d).sessions.iter().map(|s| (s.secs, s.ended)).collect::<Vec<_>>(), [(600, true)]);
        // The same with the writer in hand: the flush tries again before it answers.
        let mut w = Writer::open(&d, vienna);
        w.fail = (1, false);
        start(&mut w, T0 + 1000, "/roms/gb/Tetris.gb");
        assert_eq!(read(&d).sessions.len(), 1, "held back");
        assert!(w.flush(Duration::from_secs(1)), "written by the flush");
        assert_eq!(read(&d).sessions.len(), 2);
        // A card that stays full: the flush gives up within its time and says so.
        w.fail = (u32::MAX, false);
        start(&mut w, T0 + 2000, "/roms/gb/Tetris.gb");
        let t = Instant::now();
        assert!(!w.flush(Duration::from_millis(400)));
        assert!(t.elapsed() < Duration::from_secs(1));
        drop(tx);
        writer.join().unwrap();
    }

    /// Review SOL-2026-10-24 #8: light sleep is no play, and a session a suspend went through counts each stretch
    /// on the day it was played.
    #[test]
    fn sleep_is_left_out_and_each_stretch_counts_on_its_own_day() {
        use crate::play_history::{day_of, summarise, Period, Plain, Span, DAY};
        let d = dir("sleep");
        // 31 December 2026, 23:30 in Vienna (UTC+2 here).
        let eve = (day_of(2026, 12, 31) * DAY + 23 * 3600 + 30 * 60 - 7200) as u64;
        let path = "/roms/gb/Tetris.gb";
        let game = |at, mono| Event::Started { at, path: path.into(), name: "Tetris".into(), system: "gb".into(), emulator: String::new(), mono: Duration::from_secs(mono) };
        let quit = |at, secs, mono| Event::Ended { at, path: path.into(), name: "Tetris".into(), system: "gb".into(), secs, mono: Duration::from_secs(mono) };
        let mut w = Writer::open(&d, vienna);
        // Ten minutes, then light sleep for 90 s, then a suspend until 9:00, then twenty minutes. The game's own
        // clock (monotonic) ran 600 + 90 + 1200 s: the suspend never counted, the light sleep did.
        let morning = eve + 9 * 3600 + 30 * 60;
        w.apply(game(eve, 1000));
        w.apply(Event::Paused { at: eve + 600, mono: Duration::from_secs(1600) });
        w.apply(Event::Resumed { at: morning, mono: Duration::from_secs(1690) });
        w.apply(quit(morning + 1200, 1890, 2890));
        // Light sleep alone, with the game quit while asleep (power off from the dark screen).
        w.apply(game(morning + 3600, 5000));
        w.apply(Event::Paused { at: morning + 3600 + 300, mono: Duration::from_secs(5300) });
        w.apply(quit(morning + 3600 + 500, 500, 5500));
        // A suspend pocket-ui didn't start (logind) while the game ran: an hour away.
        w.apply(game(morning + 7200, 9000));
        w.apply(Event::Suspended { at: morning + 7200 + 600 + 3600, secs: 3600, mono: Duration::from_secs(9600) });
        w.apply(quit(morning + 7200 + 600 + 3600 + 600, 1200, 10_200));
        // A crash after a wake: no end, no time.
        w.apply(game(morning + 20_000, 20_000));
        w.apply(Event::Paused { at: morning + 20_300, mono: Duration::from_secs(20_300) });
        w.apply(Event::Resumed { at: morning + 30_000, mono: Duration::from_secs(20_400) });
        let log = read(&d);
        assert_eq!(*w.log(), log, "the file says what the writer has");
        let s: Vec<(u64, Vec<(u64, u64)>)> = log.sessions.iter().map(|s| (s.secs, s.stretches().collect())).collect();
        assert_eq!(
            s,
            [
                (1800, vec![(eve, 600), (morning, 1200)]),
                (300, vec![(morning + 3600, 300)]),
                (1200, vec![(morning + 7200, 600), (morning + 7200 + 600 + 3600, 600)]),
                (0, vec![]),
            ]
        );
        assert!(!log.sessions[3].ended);
        // The days: ten minutes in 2026, the rest in 2027.
        let today = day_of(2027, 1, 1);
        let y27 = summarise(&log, Period::of(Span::Year, today), today, &Plain);
        let y26 = summarise(&log, Period::of(Span::Year, today).prev(), today, &Plain);
        assert_eq!((y26.secs, y27.secs), (600, 1200 + 300 + 1200));
        assert_eq!((y26.sessions, y27.sessions), (1, 3), "a session counts on the day it began");
        // Compaction (and `encode`) keep the stretches.
        assert_eq!(parse(&encode(&log)).unwrap().log, log);
        // A file from before stretches were written reads as before: one stretch.
        let old = read(&d).sessions[0].clone();
        assert_eq!(Session { parts: Vec::new(), ..old.clone() }.stretches().collect::<Vec<_>>(), [(eve, 1800)]);
    }

    /// The clock set while a game runs (Settings › System › Time › Date and time, or network time once Wi-Fi is back):
    /// 26 years on. The session's time is the game's own seconds (`main.rs` counts them on `Instant`, monotonic) and
    /// its stretches the monotonic time between sleep and wake, never the difference of two wall-clock times, so the
    /// jump adds nothing. The stretch after the set is placed on the new date, the one before on the old.
    #[test]
    fn a_clock_set_during_a_game_adds_no_play_time() {
        use crate::play_history::{summarise, Period, Plain, Span};
        let d = dir("clock-set");
        let path = "/roms/gb/Tetris.gb";
        let dead: u64 = 952_077_300; // 3 March 2000: a dead RTC
        let game = |at, mono| Event::Started { at, path: path.into(), name: "Tetris".into(), system: "gb".into(), emulator: String::new(), mono: Duration::from_secs(mono) };
        let quit = |at, secs, mono| Event::Ended { at, path: path.into(), name: "Tetris".into(), system: "gb".into(), secs, mono: Duration::from_secs(mono) };
        let mut w = Writer::open(&d, vienna);
        // Five minutes on the dead clock, a minute of light sleep in which the clock is set to T0, ten minutes more.
        w.apply(game(dead, 100));
        w.apply(Event::Paused { at: dead + 300, mono: Duration::from_secs(400) });
        w.apply(Event::Resumed { at: T0, mono: Duration::from_secs(460) });
        w.apply(quit(T0 + 600, 960, 1060));
        // Set while it ran, no sleep around it: started on the old clock, ended on the new.
        w.apply(game(T0 + 1000 - 30 * 365 * 86_400, 2000));
        w.apply(quit(T0 + 1000 + 400, 700, 2700));
        // And set backwards: the end before the start on the wall clock.
        w.apply(game(T0 + 5000, 5000));
        w.apply(quit(T0 - 86_400, 120, 5120));
        let log = read(&d);
        let secs: Vec<u64> = log.sessions.iter().map(|s| s.secs).collect();
        assert!(secs.contains(&900) && secs.contains(&700) && secs.contains(&120), "{secs:?}");
        assert_eq!(log.sessions.iter().map(|s| s.secs).sum::<u64>(), 900 + 700 + 120, "the jumps added nothing");
        let first = log.sessions.iter().find(|s| s.secs == 900).unwrap();
        assert_eq!(first.stretches().collect::<Vec<_>>(), [(dead, 300), (T0, 600)]);
        // The year of the set: what was played after it, not 26 years of play.
        let today = crate::play_history::local_day(T0, 7200);
        let year = summarise(&log, Period::of(Span::Year, today), today, &Plain);
        assert!(year.secs <= 900 + 700 + 120, "{}", year.secs);
    }

    /// A pocket-ui from before 2026-10-24 wrote starts and ends without session numbers: they still pair.
    #[test]
    fn a_file_without_session_numbers_reads_as_before() {
        let mut b = header().to_vec();
        let name = |id: u32, s: &str| frame(KIND_NAME, &[id.to_le_bytes().as_slice(), s.as_bytes()].concat());
        for (i, s) in ["/roms/gb/Tetris.gb", "Tetris", "gb", "pocket-core"].iter().enumerate() {
            b.extend(name(i as u32, s));
        }
        let start = |t: u32| [t.to_le_bytes().as_slice(), &60i16.to_le_bytes(), &0u32.to_le_bytes(), &1u32.to_le_bytes(), &2u32.to_le_bytes(), &3u32.to_le_bytes()].concat();
        let end = |t: u32, secs: u32| [t.to_le_bytes().as_slice(), &60i16.to_le_bytes(), &secs.to_le_bytes(), &0u32.to_le_bytes(), &1u32.to_le_bytes(), &2u32.to_le_bytes()].concat();
        b.extend(frame(KIND_START, &start(T0 as u32)));
        b.extend(frame(KIND_END, &end(T0 as u32, 600)));
        b.extend(frame(KIND_START, &start(T0 as u32 + 1000)));
        // An unknown kind (a newer pocket-ui's) is skipped, not damage.
        b.extend(frame(42, b"from the future"));
        let log = parse(&b).unwrap().log;
        assert_eq!(log.sessions.iter().map(|s| (s.start, s.secs, s.ended, s.id)).collect::<Vec<_>>(), [(T0, 600, true, 0), (T0 + 1000, 0, false, 0)]);
        assert_eq!(log.damaged, 0);
        assert_eq!(log.games[0].name, "Tetris");
    }

    /// `POCKETUI_NO_HISTORY=1` (the latency measurements): nothing is recorded, not even queued for later.
    #[test]
    fn a_run_held_off_records_nothing() {
        use std::ffi::OsStr;
        assert!(holds_off(Some(OsStr::new("1"))) && holds_off(Some(OsStr::new("yes"))));
        assert!(!holds_off(None) && !holds_off(Some(OsStr::new(""))) && !holds_off(Some(OsStr::new("0"))));
        let started = || Cmd::Event(Event::Status { at: T0, path: "/roms/gb/Tetris.gb".into(), name: "Tetris".into(), system: "gb".into(), status: Status::Playing });
        let mut r = Recorder { enabled: true, held_off: true, ..Default::default() };
        r.send(started());
        assert!(r.queue.is_empty(), "held off: dropped");
        let (tx, rx) = channel();
        r.tx = Some(tx);
        r.send(started());
        assert!(rx.try_recv().is_err(), "even with a writer");
        r.held_off = false;
        r.send(started());
        assert!(rx.try_recv().is_ok(), "an ordinary run records");
    }

    #[test]
    fn the_file_stays_bounded_and_keeps_the_newest_sessions() {
        let d = dir("bounded");
        // A log just over the limit, written whole as fake data would be.
        let games: Vec<Game> = (0..300).map(|i| Game { path: format!("/storage/roms/gba/Game {i:03} (Europe).gba"), name: format!("Game {i:03}"), system: "gba".into() }).collect();
        let n = (MAX_BYTES / 50) as usize;
        let sessions: Vec<Session> = (0..n).map(|k| Session { game: k % 300, start: T0 + k as u64 * 3600, tz: 7200, secs: 1800, ended: true, emulator: "pocket-core/mgba".into(), ..Default::default() }).collect();
        let carried = vec![Carried { game: 7, secs: 5000, sessions: 3, last_played: None }];
        let log = build(games, sessions, Vec::new(), carried);
        fs::write(d.join(FILE), encode(&log)).unwrap();
        assert!(fs::metadata(d.join(FILE)).unwrap().len() > MAX_BYTES);
        let mut w = Writer::open(&d, vienna);
        let len = fs::metadata(d.join(FILE)).unwrap().len();
        assert!(len <= COMPACT_TO + 64 * 1024, "compacted to {len}");
        let back = read(&d);
        assert_eq!(back.sessions.last().unwrap().start, T0 + (n as u64 - 1) * 3600, "the newest stay");
        assert!(back.sessions.len() < n);
        assert_eq!(back.carried.len(), 1, "the totals from before stay");
        assert!(back.migrated);
        start(&mut w, T0 + n as u64 * 3600, "/roms/gb/Tetris.gb");
        assert_eq!(read(&d).sessions.len(), back.sessions.len() + 1, "and it goes on appending");
    }

    #[test]
    fn a_file_replaced_behind_the_writer_is_read_again() {
        let d = dir("replaced");
        let mut w = Writer::open(&d, vienna);
        start(&mut w, T0, "/roms/gb/Tetris.gb");
        end(&mut w, T0 + 700, "/roms/gb/Tetris.gb", 600);
        // Another log put in its place (a copy over the network, a backup put back by hand): other names, other ids.
        let other = build(
            vec![Game { path: "/roms/snes/Chrono Trigger.sfc".into(), name: "Chrono Trigger".into(), system: "snes".into() }, Game { path: "/roms/nes/Zelda.nes".into(), name: "Zelda".into(), system: "nes".into() }],
            vec![Session { game: 1, start: T0 - 9000, tz: 7200, secs: 1200, ended: true, emulator: "pocket-core/fceumm".into(), ..Default::default() }],
            Vec::new(),
            Vec::new(),
        );
        fs::write(d.join(FILE), encode(&other)).unwrap();
        start(&mut w, T0 + 1000, "/roms/gb/Tetris.gb");
        let log = read(&d);
        let names: Vec<&str> = log.sessions.iter().map(|s| log.games[s.game].name.as_str()).collect();
        assert_eq!(names, ["Zelda", "Tetris"], "the new session is the file's, under its own name");
        assert_eq!(*w.log(), log);
    }

    #[test]
    fn a_new_name_never_takes_the_id_of_a_lost_one() {
        let d = dir("lostname");
        let mut w = Writer::open(&d, vienna);
        start(&mut w, T0, "/roms/gb/Tetris.gb");
        end(&mut w, T0 + 100, "/roms/gb/Tetris.gb", 100);
        // The last name frame (the emulator's) goes bad on the card.
        let mut bytes = fs::read(d.join(FILE)).unwrap();
        let emulator = bytes.windows(8).position(|w| w == b"pocket-c").unwrap();
        bytes[emulator] ^= 0xff;
        fs::write(d.join(FILE), &bytes).unwrap();
        let mut w = Writer::open(&d, vienna);
        w.apply(Event::Started { at: T0 + 500, path: "/roms/gba/Zelda.gba".into(), name: "Zelda".into(), system: "gba".into(), emulator: "retroarch/mgba".into(), mono: Duration::from_secs(T0 + 500) });
        let log = read(&d);
        assert_eq!(log.sessions.len(), 2);
        assert_eq!(log.sessions[0].emulator, "", "the lost name stays lost");
        assert_eq!(log.games[log.sessions[1].game].path, "/roms/gba/Zelda.gba", "and nothing new is read under it");
        assert_eq!(log.sessions[1].emulator, "retroarch/mgba");
    }

    #[test]
    fn a_file_kept_aside_is_remembered() {
        let d = dir("aside");
        fs::write(d.join(FILE), b"something else entirely").unwrap();
        let mut w = Writer::open(&d, vienna);
        assert!(w.log().set_aside, "the pages can say an older file could not be read");
        start(&mut w, T0, "/roms/gb/Tetris.gb");
        // And at the next start too, as long as the old file is there.
        assert!(Writer::open(&d, vienna).log().set_aside);
        assert!(!read(&d).set_aside, "the file itself doesn't know");
    }

    #[test]
    fn encode_and_read_agree() {
        let games = vec![
            Game { path: "/roms/gb/Tetris.gb".into(), name: "Tetris".into(), system: "gb".into() },
            Game { path: "/roms/ports/Celeste.sh".into(), name: "Celeste".into(), system: "ports".into() },
        ];
        let sessions = vec![
            Session { game: 0, start: T0, tz: 3600, secs: 120, ended: true, emulator: "retroarch/gambatte".into(), ..Default::default() },
            Session { game: 1, start: T0 + 500, tz: 3600, secs: 0, ended: false, emulator: "ports".into(), ..Default::default() },
        ];
        let statuses = vec![StatusChange { game: 1, at: T0 + 900, tz: 3600, status: Status::Playing }];
        let carried = vec![Carried { game: 0, secs: 99, sessions: 1, last_played: Some(T0 - 5) }];
        let log = build(games, sessions, statuses, carried);
        let back = parse(&encode(&log)).unwrap().log;
        assert_eq!(back, log);
    }
}
