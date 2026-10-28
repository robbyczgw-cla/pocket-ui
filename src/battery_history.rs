//! The battery history: one sample a minute for the last 48 hours, kept over restarts, with the gaps
//! (device asleep, off, or pocket-ui not running) left as gaps.
//!
//! * [`History`] is the data and its rules: one sample per minute, nothing older than [`KEEP`], never more
//!   than [`MAX_SAMPLES`], a clock that jumps back drops what it contradicts, a clock that was never set
//!   records nothing. Each sample carries what was running (a system id such as "ps2", from a short table
//!   in the file) so the page can say what a game costs.
//! * The file (`battery-history.bin`, about 46 kB when full) is written whole through
//!   `fsutil::write_atomic` once a minute, with a checksum, and a backup (`.bak`) at most every half hour.
//!   A file that fails its checks is replaced by the backup, and by nothing when that is bad too; either
//!   way it is said on stderr and the next sample is written again.
//! * [`start`] runs the sampler for as long as pocket-ui runs (a game running is no reason to stop: pocket-ui
//!   stays up beside it), [`set_activity`] tells it what is running, [`view`] gives the page a copy.
//! * The sampler also keeps the battery per game (`battery_games`): [`set_game`] names the game running, each new
//!   sample adds the stretch since the last one to that game's ledger, and [`game_left`] / [`game_card`] answer for
//!   the pause menu and the card from memory (no file read, no sysfs read on the caller's thread). The files are read
//!   on the sampler's thread with no lock held and published whole; until then the card says "Loading…" ([`LOADING`])
//!   and [`view`] is empty with [`loaded`] false (review SOL 2026-10-24 #6).
//! * A stretch counts for a game only when nothing slept in it: every change into or out of light or deep sleep (and a
//!   suspend nobody asked for) starts a new generation ([`set_asleep`], [`sleep_boundary`]), and two samples of
//!   different generations are never a pair (review SOL 2026-10-24 #11).
//! * [`stop`] ends the sampler before pocket-ui exits, after the write in progress (if any) is on the card, so an
//!   orderly exit never leaves a `battery-history.bin.<pid>-<n>.tmp` behind (review SOL 2026-10-24 #12; killed
//!   processes' ones go at the next start, `fsutil::reap_temp_files`).
//!
//! File layout, little endian: `"PBH1" | u8 version (1) | u8 tag count | u16 sample count | per tag: u8 length,
//! name | per sample: 16 bytes (`Sample::encode`) | u32 CRC-32 of everything before`.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

use crate::battery::{self, Sample, RECORD};
use crate::battery_games::{self, Key, Ledger};
use crate::fsutil;

pub const FILE: &str = "battery-history.bin";
/// The first version of the history: a ring of one day, replaced by this file on the first start after the update.
pub const LEGACY_RING: &str = "battery.ring";
/// How long samples are kept.
pub const KEEP: u64 = 48 * 3600;
/// The most samples there can be (48 hours of minutes are 2 881); a file that claims more is damaged.
pub const MAX_SAMPLES: usize = 3000;
/// Samples further apart than this (a minute is the rule) have a gap between them: the device slept, was
/// off, or pocket-ui wasn't running. Three missed samples; a busy moment is no gap.
pub const GAP: u64 = 180;
const MAGIC: &[u8; 4] = b"PBH1";
const VERSION: u8 = 1;
const MAX_TAGS: usize = 31;
/// Before this the clock has not been set (the device has no battery-backed one): such samples would be junk.
const CLOCK_SET: u64 = 1_700_000_000;
/// The backup is renewed when it is older than this.
const BACKUP_EVERY: Duration = Duration::from_secs(30 * 60);

#[derive(Clone, Debug, Default, PartialEq)]
pub struct History {
    samples: Vec<Sample>,
    /// Names of what ran; a sample's `tag` n > 0 is `tags[n - 1]`.
    tags: Vec<String>,
}

/// CRC-32 (the one zlib and PNG use).
pub(crate) fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0u32;
    for b in bytes {
        crc ^= *b as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 { (crc >> 1) ^ 0xedb8_8320 } else { crc >> 1 };
        }
    }
    !crc
}

impl History {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn samples(&self) -> &[Sample] {
        &self.samples
    }

    /// The name of what ran at `s`; None for the menus.
    pub fn tag(&self, s: &Sample) -> Option<&str> {
        self.tag_name(s.tag)
    }

    /// The name behind a tag number (1 and up).
    pub fn tag_name(&self, tag: u8) -> Option<&str> {
        (tag as usize).checked_sub(1).and_then(|i| self.tags.get(i)).map(String::as_str)
    }

    /// The samples of the last `secs` seconds before `now`, oldest first.
    pub fn recent(&self, now: u64, secs: u64) -> &[Sample] {
        let from = self.samples.partition_point(|s| s.t + secs < now);
        let to = self.samples.partition_point(|s| s.t <= now + 60);
        &self.samples[from..to.max(from)]
    }

    /// Adds a sample; false when it was left out (the clock is not set). `activity` is the system that was
    /// running. A second sample in the minute of the newest one replaces it.
    pub fn push(&mut self, mut s: Sample, activity: Option<&str>) -> bool {
        if s.t < CLOCK_SET {
            return false;
        }
        // The clock went back (set by hand, or the network fixed a wrong time): what lies in its future
        // can't be trusted any more.
        while self.samples.last().is_some_and(|l| l.t > s.t + 60) {
            self.samples.pop();
        }
        // Forget what is older than 48 hours (and the names only that used) before there is a place for a new name.
        let oldest = s.t.saturating_sub(KEEP);
        let drop = self
            .samples
            .partition_point(|x| x.t < oldest)
            .max((self.samples.len() + 1).saturating_sub(MAX_SAMPLES));
        if drop > 0 {
            self.samples.drain(..drop.min(self.samples.len()));
            self.compact_tags();
        }
        s.tag = self.tag_index(activity);
        match self.samples.last_mut() {
            Some(last) if last.t / 60 == s.t / 60 => *last = s,
            _ => self.samples.push(s),
        }
        true
    }

    /// The table index for a name, adding it; 0 ("nothing in particular") when it has no room.
    fn tag_index(&mut self, activity: Option<&str>) -> u8 {
        let Some(name) = activity.map(str::trim).filter(|n| !n.is_empty() && n.len() <= 40) else {
            return 0;
        };
        if self.tags.len() >= MAX_TAGS && !self.tags.iter().any(|t| t == name) {
            self.compact_tags();
        }
        match self.tags.iter().position(|t| t == name) {
            Some(i) => i as u8 + 1,
            None if self.tags.len() < MAX_TAGS => {
                self.tags.push(name.to_string());
                self.tags.len() as u8
            }
            None => 0,
        }
    }

    /// Forgets names no sample uses any more (and renumbers the rest).
    fn compact_tags(&mut self) {
        let mut used = vec![false; self.tags.len() + 1];
        for s in &self.samples {
            if let Some(u) = used.get_mut(s.tag as usize) {
                *u = true;
            }
        }
        if (1..used.len()).all(|i| used[i]) {
            return;
        }
        let mut map = vec![0u8; used.len()];
        let mut kept = Vec::new();
        for (i, name) in self.tags.drain(..).enumerate() {
            if used[i + 1] {
                kept.push(name);
                map[i + 1] = kept.len() as u8;
            }
        }
        self.tags = kept;
        for s in &mut self.samples {
            s.tag = map.get(s.tag as usize).copied().unwrap_or(0);
        }
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(12 + self.tags.len() * 12 + self.samples.len() * RECORD);
        b.extend_from_slice(MAGIC);
        b.push(VERSION);
        b.push(self.tags.len() as u8);
        b.extend_from_slice(&(self.samples.len() as u16).to_le_bytes());
        for t in &self.tags {
            b.push(t.len() as u8);
            b.extend_from_slice(t.as_bytes());
        }
        for s in &self.samples {
            b.extend_from_slice(&s.encode());
        }
        let crc = crc32(&b);
        b.extend_from_slice(&crc.to_le_bytes());
        b
    }

    /// The history in `bytes`, or why it can't be used.
    fn parse(bytes: &[u8]) -> Result<History, String> {
        if bytes.len() < 12 {
            return Err(format!("{} bytes", bytes.len()));
        }
        if &bytes[0..4] != MAGIC {
            return Err("not a battery history".into());
        }
        if bytes[4] != VERSION {
            return Err(format!("version {}", bytes[4]));
        }
        let (body, crc) = bytes.split_at(bytes.len() - 4);
        if crc32(body) != u32::from_le_bytes(crc.try_into().unwrap()) {
            return Err("checksum".into());
        }
        let (ntags, count) = (bytes[5] as usize, u16::from_le_bytes([bytes[6], bytes[7]]) as usize);
        if ntags > MAX_TAGS || count > MAX_SAMPLES {
            return Err("too many entries".into());
        }
        let mut at = 8;
        let mut tags = Vec::new();
        for _ in 0..ntags {
            let len = *body.get(at).ok_or("cut off")? as usize;
            let name = body.get(at + 1..at + 1 + len).ok_or("cut off")?;
            tags.push(String::from_utf8(name.to_vec()).map_err(|_| "tag name")?);
            at += 1 + len;
        }
        if body.len() != at + count * RECORD {
            return Err("length".into());
        }
        let samples: Vec<Sample> = body[at..]
            .chunks_exact(RECORD)
            .map(|r| Sample::decode(r).filter(|s| s.tag as usize <= ntags))
            .collect::<Option<_>>()
            .ok_or("a sample is invalid")?;
        if samples.windows(2).any(|w| w[0].t > w[1].t) {
            return Err("out of order".into());
        }
        Ok(History { samples, tags })
    }

    /// The history in `path`: when the file can't be used, the backup; failing that nothing, as for a first start.
    /// A missing file is a first start without a word, as for the other files (`fsutil::read_json_or_backup`).
    pub fn load(path: &Path) -> History {
        let bytes = match std::fs::read(path) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return History::new(),
            Err(e) => {
                eprintln!("battery: can't read {}: {e}; trying the backup", path.display());
                Vec::new()
            }
        };
        match History::parse(&bytes) {
            Ok(h) => return h,
            Err(e) => eprintln!("battery: {} is unusable ({e}); trying the backup", path.display()),
        }
        let bak = fsutil::backup_path(path);
        match std::fs::read(&bak).map_err(|e| e.to_string()).and_then(|b| History::parse(&b)) {
            Ok(h) => {
                eprintln!("battery: using the last good copy {}", bak.display());
                h
            }
            Err(e) => {
                eprintln!("battery: no usable backup of {} ({e}); starting a new history", path.display());
                History::new()
            }
        }
    }

    /// The history of a data folder. On the first start after the update that brought this file, the old
    /// one-day ring (`battery.ring`) is taken over, once.
    pub fn open(data_dir: &Path) -> History {
        let path = data_dir.join(FILE);
        if !path.exists() {
            let ring = data_dir.join(LEGACY_RING);
            if let Ok(bytes) = std::fs::read(&ring) {
                let h = History::from_legacy_ring(&bytes);
                if h.save(&path).is_ok() {
                    let _ = std::fs::remove_file(&ring);
                }
                return h;
            }
        }
        History::load(&path)
    }

    /// The samples of the first version's ring (a record per minute of a day, in the slot `minute % 1440`).
    pub fn from_legacy_ring(bytes: &[u8]) -> History {
        let mut old: Vec<Sample> = bytes.chunks_exact(RECORD).filter_map(Sample::decode).collect();
        old.sort_by_key(|s| s.t);
        let mut h = History::new();
        for mut s in old {
            s.tag = 0;
            h.push(s, None);
        }
        h
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        write_file(path, &self.encode())
    }
}

/// Writes the file whole and atomically; the backup too when it is missing, damaged or half an hour old.
fn write_file(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    fsutil::write_atomic(path, bytes)?;
    let bak = fsutil::backup_path(path);
    let age = std::fs::metadata(&bak)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| SystemTime::now().duration_since(t).ok());
    let due = match age {
        None => true,
        Some(a) if a >= BACKUP_EVERY => true,
        Some(_) => std::fs::read(&bak).map_err(|e| e.to_string()).and_then(|b| History::parse(&b)).is_err(),
    };
    if due {
        if let Err(e) = fsutil::write_atomic(&bak, bytes) {
            eprintln!("battery: no backup of {}: {e}", path.display());
        }
    }
    Ok(())
}

/// The runs of samples without a gap between them.
pub fn segments(samples: &[Sample]) -> Vec<&[Sample]> {
    let mut out = Vec::new();
    let mut from = 0;
    for i in 1..samples.len() {
        if samples[i].t.saturating_sub(samples[i - 1].t) > GAP {
            out.push(&samples[from..i]);
            from = i;
        }
    }
    if from < samples.len() {
        out.push(&samples[from..]);
    }
    out
}

// ---- the sampler -------------------------------------------------------------------------------------------

struct Recorder {
    path: PathBuf,
    history: History,
    /// The battery per game (`battery_games`), and the last sample it was fed with the game running then and the
    /// sleep generation ([`GENERATION`]) it was taken in.
    games: Ledger,
    prev: Option<(Sample, Option<Key>, u64)>,
}

impl Recorder {
    fn open(data_dir: &Path) -> Recorder {
        Recorder {
            path: data_dir.join(FILE),
            history: History::open(data_dir),
            games: Ledger::load(data_dir),
            prev: None,
        }
    }

    /// The stretch from the last sample to `s` for the game running at both (`battery_games::pair`), when both were
    /// taken in the same sleep generation `generation` (nothing slept between them); true when the ledger changed.
    fn feed_games(&mut self, s: Sample, game: Option<Key>, generation: u64) -> bool {
        let counted = match (&self.prev, &game) {
            (Some((p, Some(was), g)), Some(now)) if was == now && *g == generation => {
                battery_games::pair(p, &s).map(|(secs, drop)| self.games.add(now, secs, drop, s.t))
            }
            _ => None,
        };
        self.prev = Some((s, game, generation));
        counted.is_some()
    }
}

/// The running sampler's history and ledger; None until they are read (on the sampler's thread, no lock held).
static RECORDER: Mutex<Option<Recorder>> = Mutex::new(None);
/// pocket-ui is exiting: no new sample, no new write ([`stop`]).
static STOPPED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// Goes up with every change into or out of sleep: samples of different generations are never a game's stretch.
static GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
/// The card's Battery line before the history and the ledger are read.
pub const LOADING: &str = "Loading…";
/// One writer at a time: the file is rewritten whole, and the temporary file has a fixed name.
static WRITING: Mutex<()> = Mutex::new(());
static ACTIVITY: Mutex<Option<String>> = Mutex::new(None);
static GAME: Mutex<Option<Key>> = Mutex::new(None);
/// Light sleep (screen off, game paused, `sleep.rs`): its minutes are nobody's play.
static ASLEEP: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// What is running now, as a system id ("ps2"); None back in the menus. Samples taken meanwhile carry it.
pub fn set_activity(activity: Option<&str>) {
    *ACTIVITY.lock().unwrap() = activity.map(str::to_string);
}

/// The game running now (system, file, emulator); None back in the menus. Its stretches on battery go to its ledger.
pub fn set_game(game: Option<Key>) {
    *GAME.lock().unwrap() = game;
}

/// The Pocket went into light sleep (true) or woke up (false), deep sleep included (it goes through light sleep both
/// ways): samples meanwhile count for no game, and no stretch reaches across either change ([`GENERATION`]).
pub fn set_asleep(asleep: bool) {
    ASLEEP.store(asleep, std::sync::atomic::Ordering::SeqCst);
    sleep_boundary();
}

/// The Pocket slept in a way [`set_asleep`] didn't see (a suspend nobody asked for): the stretch to the next sample
/// is nobody's.
pub fn sleep_boundary() {
    GENERATION.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
}

/// Whether the history and the ledger have been read (the Battery page says "Loading…" until then).
pub fn loaded() -> bool {
    RECORDER.lock().unwrap().is_some()
}

/// Reads the history and the ledger of `data_dir` now, on the caller's thread, unless they are: for `--snapshot`,
/// the desktop (no sampler runs) and the web companion's thread. Never from the UI thread of a running pocket-ui.
pub fn load_now(data_dir: &Path) {
    ensure_open(data_dir);
}

/// The recorder of `data_dir` is in [`RECORDER`]: read with no lock held, then put there unless someone was faster.
fn ensure_open(data_dir: &Path) {
    if RECORDER.lock().unwrap().is_some() {
        return;
    }
    let rec = Recorder::open(data_dir);
    RECORDER.lock().unwrap().get_or_insert(rec);
}

/// pocket-ui is exiting: no sample after this, and a write in progress is waited for (up to two seconds) so its
/// temporary file is renamed, not left behind.
pub fn stop() {
    STOPPED.store(true, std::sync::atomic::Ordering::SeqCst);
    let end = std::time::Instant::now() + Duration::from_secs(2);
    while std::time::Instant::now() < end {
        // Free (or poisoned by a writer that panicked, which writes no more either).
        if !matches!(WRITING.try_lock(), Err(std::sync::TryLockError::WouldBlock)) {
            return;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    eprintln!("battery: a history write still runs at the exit");
}

/// The newest sample, if the sampler runs: what the pause menu's battery line goes by.
pub fn latest() -> Option<Sample> {
    RECORDER.lock().unwrap().as_ref().and_then(|r| r.history.samples().last().copied())
}

/// What there is to say about the battery in `game` now (`battery_games::left`): from memory, in microseconds.
/// None without a battery or a recent sample.
pub fn game_left(game: &Key) -> Option<battery_games::Left> {
    let guard = RECORDER.lock().unwrap();
    let rec = guard.as_ref()?;
    battery_games::left(rec.games.rate(game), rec.history.samples().last(), crate::recents::now_secs())
}

/// The card's Battery line for `game` on the shelf (`battery_games::card_line`; its emulator left empty): the running
/// sampler's, from memory; [`LOADING`] while its files are still being read (the first moments after the start),
/// never a file read here. `system` is what the player calls the system ("PS2").
pub fn game_card(game: &Key, system: &str) -> Option<String> {
    let t = crate::recents::now_secs();
    match RECORDER.lock().unwrap().as_ref() {
        Some(r) => battery_games::card_line(r.games.rate(game), r.history.samples().last(), t, system),
        None => Some(LOADING.to_string()),
    }
}

/// The battery now with the current read three times 100 ms apart and the middle value taken: a single
/// reading of `current_now` jumps with every burst of the CPU.
fn read_steady(dir: &Path, t: u64) -> Option<Sample> {
    let mut first = battery::read(dir, t)?;
    let mut reads = vec![(first.ma, first.mv)];
    for _ in 0..2 {
        std::thread::sleep(Duration::from_millis(100));
        if let Some(r) = battery::read(dir, t) {
            reads.push((r.ma, r.mv));
        }
    }
    let middle = |mut v: Vec<i64>| {
        v.sort_unstable();
        v[v.len() / 2]
    };
    first.ma = middle(reads.iter().map(|r| r.0 as i64).collect()) as i32;
    first.mv = middle(reads.iter().map(|r| r.1 as i64).collect()) as u16;
    Some(first)
}

/// Adds `s` to the history (once the sampler has read it; before that `s` is dropped) and writes the file.
/// `count_game`: false for a sample after a wake ([`sample_soon`]): the stretch to it spans the sleep, so it ends the
/// game's run of samples instead of adding to it (and the ledger isn't written on the caller's thread). Samples in
/// light sleep ([`set_asleep`]) count for no game either.
/// `generation`: [`GENERATION`] before `s` was read; a sleep change while it was read makes it nobody's.
fn record(data_dir: &Path, s: Sample, count_game: bool, generation: u64) {
    if STOPPED.load(std::sync::atomic::Ordering::SeqCst) {
        return;
    }
    let activity = ACTIVITY.lock().unwrap().clone();
    let now = GENERATION.load(std::sync::atomic::Ordering::SeqCst);
    let awake = !ASLEEP.load(std::sync::atomic::Ordering::SeqCst);
    let game = GAME.lock().unwrap().clone().filter(|_| count_game && awake && now == generation);
    let (path, bytes, games) = {
        let mut guard = RECORDER.lock().unwrap();
        // Not read yet (a wake in the first moments): the sampler reads the files, not whoever called.
        let Some(rec) = guard.as_mut() else { return };
        if !rec.history.push(s, activity.as_deref()) {
            return;
        }
        let games = rec.feed_games(s, game, now).then(|| rec.games.clone());
        (rec.path.clone(), rec.history.encode(), games)
    };
    let _writing = WRITING.lock().unwrap();
    // `stop` came while this sample was being put together: nothing more is written (the exit goes on).
    if STOPPED.load(std::sync::atomic::Ordering::SeqCst) {
        return;
    }
    if let Err(e) = write_file(&path, &bytes) {
        eprintln!("battery: can't write {}: {e}", path.display());
    }
    if let Some(Err(e)) = games.map(|g| g.save(data_dir)) {
        eprintln!("battery: can't write {}: {e}", battery_games::FILE);
    }
}

/// One sample after a wake, on a thread of its own: the caller (the UI thread, `sleep.rs`) never waits for its three
/// reads 100 ms apart and the file write. The sleep generation and the time are taken here, on the caller's thread, so
/// the sample belongs to the moment of the wake even when its reads end after the next change; it counts for no game
/// either way (`record`'s `count_game` false). A minute that has a sample already gets this one instead.
pub fn sample_soon(data_dir: &Path) {
    spawn_sample(battery::dir(), data_dir.to_path_buf());
}

/// [`sample_soon`] from the battery in `dir`; the thread's handle (tests wait for it).
fn spawn_sample(dir: PathBuf, data_dir: PathBuf) -> Option<std::thread::JoinHandle<()>> {
    let generation = GENERATION.load(std::sync::atomic::Ordering::SeqCst);
    let t = crate::recents::now_secs();
    std::thread::Builder::new()
        .name("battery-wake".into())
        .spawn(move || {
            if let Some(s) = read_steady(&dir, t) {
                record(&data_dir, s, false, generation);
            }
        })
        .ok()
}

/// Opens the history and samples the battery every minute for as long as pocket-ui runs, game or not.
/// The first sample is taken right away; after a suspend the next one follows within seconds (the loop
/// checks the wall clock, not a timer that sleeps along).
pub fn start(data_dir: PathBuf) {
    std::thread::spawn(move || {
        // The history and the battery per game are read here, not on the caller's (the UI) thread, and with no lock
        // held: the card and the pause menu never wait for them.
        ensure_open(&data_dir);
        let dir = battery::dir();
        let mut last_minute = None;
        while !STOPPED.load(std::sync::atomic::Ordering::SeqCst) {
            let now = crate::recents::now_secs();
            if last_minute != Some(now / 60) {
                last_minute = Some(now / 60);
                let generation = GENERATION.load(std::sync::atomic::Ordering::SeqCst);
                match read_steady(&dir, now) {
                    Some(s) => record(&data_dir, s, true, generation),
                    // No battery: stop quietly, there is nothing to record.
                    None => return,
                }
            }
            std::thread::sleep(Duration::from_secs(5));
        }
    });
}

/// A copy of the history for drawing: the running sampler's; empty while it is still being read ([`loaded`]).
pub fn view() -> History {
    match RECORDER.lock().unwrap().as_ref() {
        Some(rec) => rec.history.clone(),
        None => History::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;

    const T0: u64 = 1_790_000_000 / 60 * 60;

    fn s(t: u64, capacity: u8) -> Sample {
        Sample {
            t,
            capacity,
            charging: false,
            full: false,
            mv: 3900,
            ma: 500,
            temp: Some(300),
            tag: 0,
        }
    }

    fn dir(name: &str) -> crate::testutil::TestDir {
        TestDir::new(&format!("bathist-{name}"))
    }

    /// One sample a minute for `minutes` minutes from `from`, falling from `start` one point every 6 minutes.
    fn run(h: &mut History, from: u64, minutes: u64, start: u8, activity: Option<&str>) {
        for m in 0..minutes {
            h.push(s(from + m * 60, start.saturating_sub((m / 6) as u8)), activity);
        }
    }

    #[test]
    fn keeps_48_hours_and_not_a_minute_more() {
        let mut h = History::new();
        run(&mut h, T0, 60 * 60, 100, None); // 60 hours
        let newest = T0 + (60 * 60 - 1) * 60;
        assert_eq!(h.samples().last().unwrap().t, newest);
        assert!(h.samples().first().unwrap().t >= newest - KEEP, "older than 48 h is gone");
        assert!(h.samples().len() <= 48 * 60 + 1, "{}", h.samples().len());
        assert!(h.samples().len() >= 48 * 60, "and the 48 h are all there: {}", h.samples().len());
    }

    #[test]
    fn the_file_is_small_and_capped() {
        let mut h = History::new();
        run(&mut h, T0, 60 * 60, 100, Some("ps2"));
        let bytes = h.encode();
        assert!(!bytes.is_empty());
        assert!(bytes.len() <= MAX_SAMPLES * 16 + 1024, "{} bytes", bytes.len());
        assert!(bytes.len() < 64 * 1024, "{} bytes", bytes.len());
        // A file that claims more than the cap is damaged, not history.
        let d = dir("cap");
        let f = d.join(FILE);
        let mut big = bytes[..bytes.len() - 4].to_vec();
        big[6..8].copy_from_slice(&((MAX_SAMPLES + 1) as u16).to_le_bytes());
        big.extend_from_slice(&crc32(&big).to_le_bytes());
        std::fs::write(&f, &big).unwrap();
        assert!(History::load(&f).samples().is_empty());
    }

    #[test]
    fn a_second_sample_in_the_same_minute_replaces_the_first() {
        let mut h = History::new();
        h.push(s(T0, 80), None);
        h.push(s(T0 + 20, 79), None);
        h.push(s(T0 + 60, 78), None);
        assert_eq!(h.samples().iter().map(|x| x.capacity).collect::<Vec<_>>(), vec![79, 78]);
    }

    #[test]
    fn samples_further_apart_than_the_gap_are_split_not_joined() {
        let mut h = History::new();
        run(&mut h, T0, 60, 90, None);
        // The device slept for 2 hours.
        run(&mut h, T0 + 3 * 3600, 30, 70, None);
        let seg = segments(h.samples());
        assert_eq!(seg.len(), 2);
        assert_eq!((seg[0].len(), seg[1].len()), (60, 30));
        // A missed sample or two (a busy moment) is not a gap; four minutes without one is.
        let mut b = History::new();
        for t in [0, 1, 2, 4, 8, 9] {
            b.push(s(T0 + t * 60, 50), None);
        }
        let seg = segments(b.samples());
        assert_eq!(
            seg.iter().map(|x| x.len()).collect::<Vec<_>>(),
            vec![4, 2],
            "4 minutes (4 -> 8) is more than GAP, 2 (2 -> 4) is not"
        );
        assert!(segments(&[]).is_empty());
    }

    #[test]
    fn survives_a_restart() {
        let d = dir("restart");
        let f = d.join(FILE);
        let mut h = History::new();
        run(&mut h, T0, 120, 95, Some("gba"));
        run(&mut h, T0 + 3 * 3600, 30, 60, Some("ps2"));
        run(&mut h, T0 + 4 * 3600, 5, 55, None);
        h.save(&f).unwrap();
        let again = History::load(&f);
        assert!(again == h, "the history read back is the history saved");
        assert_eq!(again.samples().len(), 155);
        let tags: Vec<_> = [0, 119, 120, 149, 150].iter().map(|i| again.tag(&again.samples()[*i])).collect();
        assert_eq!(tags, vec![Some("gba"), Some("gba"), Some("ps2"), Some("ps2"), None]);
        // And a second run carries on from it.
        let mut next = again;
        next.push(s(T0 + 4 * 3600 + 5 * 60, 54), Some("gba"));
        assert_eq!(next.samples().len(), 156);
        // Only the file, its backup, and nothing half written.
        let mut names: Vec<_> = std::fs::read_dir(&d).unwrap().map(|e| e.unwrap().file_name().into_string().unwrap()).collect();
        names.sort();
        assert_eq!(names, vec![FILE.to_string(), format!("{FILE}.bak")]);
    }

    #[test]
    fn a_damaged_file_falls_back_to_the_backup_then_to_nothing() {
        let d = dir("corrupt");
        let f = d.join(FILE);
        let mut h = History::new();
        run(&mut h, T0, 90, 80, None);
        h.save(&f).unwrap();
        let good = std::fs::read(&f).unwrap();
        assert_eq!(std::fs::read(fsutil_bak(&f)).unwrap(), good);
        let flipped = {
            let mut b = good.clone();
            let mid = b.len() / 2;
            b[mid] ^= 0x55;
            b
        };
        let cases: Vec<(&str, Vec<u8>)> = vec![
            ("empty", vec![]),
            ("garbage", b"this is not a history".to_vec()),
            ("cut off", good[..good.len() / 2].to_vec()),
            ("flipped byte", flipped),
            ("wrong version", {
                let mut b = good.clone();
                b[4] = 99;
                b
            }),
            ("zeros", vec![0; 4096]),
        ];
        for (name, bad) in &cases {
            std::fs::write(&f, bad).unwrap();
            let got = History::load(&f);
            assert_eq!(got.samples().len(), 90, "{name}: the backup is used");
        }
        // Both bad: start empty, without a panic, and keep working.
        for (name, bad) in &cases {
            std::fs::write(&f, bad).unwrap();
            std::fs::write(fsutil_bak(&f), bad).unwrap();
            let mut got = History::load(&f);
            assert!(got.samples().is_empty(), "{name}");
            got.push(s(T0, 50), None);
            got.save(&f).unwrap();
            assert_eq!(History::load(&f).samples().len(), 1, "{name}: writes again");
        }
        // Nothing there at all is just a first start.
        assert!(History::load(&d.join("missing.bin")).samples().is_empty());
    }

    #[test]
    fn a_clock_that_goes_back_drops_the_samples_from_the_future() {
        let mut h = History::new();
        run(&mut h, T0, 60, 90, None);
        // The network fixed the clock: now it is 10 hours earlier than the last samples said.
        h.push(s(T0 - 10 * 3600, 70), None);
        assert_eq!(h.samples().len(), 1, "the later samples can't be trusted");
        h.push(s(T0 - 10 * 3600 + 60, 70), None);
        assert_eq!(h.samples().len(), 2);
    }

    #[test]
    fn a_clock_that_was_never_set_records_nothing() {
        let mut h = History::new();
        assert!(!h.push(s(86_400 * 3, 50), None), "1970");
        assert!(h.samples().is_empty());
        assert!(h.push(s(T0, 50), None));
    }

    #[test]
    fn recent_is_a_window_of_the_newest() {
        let mut h = History::new();
        run(&mut h, T0, 600, 100, None);
        let now = T0 + 599 * 60;
        assert_eq!(h.recent(now, 3600).len(), 61);
        assert_eq!(h.recent(now, 24 * 3600).len(), 600);
        assert!(h.recent(now + 10 * 3600, 3600).is_empty());
    }

    #[test]
    fn what_ran_is_remembered_as_a_short_table_that_cleans_up_after_itself() {
        let mut h = History::new();
        for i in 0..40 {
            h.push(s(T0 + i * 60, 80), Some(&format!("sys{i}")));
        }
        // 31 names fit; the rest are "nothing in particular" instead of an error.
        let named = h.samples().iter().filter(|x| h.tag(x).is_some()).count();
        assert_eq!(named, 31);
        // 3 days later the old names are gone and new ones fit again.
        h.push(s(T0 + 4 * 86_400, 80), Some("ps2"));
        assert_eq!(h.tag(&h.samples()[h.samples().len() - 1]), Some("ps2"));
        assert_eq!(h.samples().len(), 1);
        assert!(h.encode().len() < 100, "{} bytes", h.encode().len());
    }

    #[test]
    fn the_first_version_of_the_ring_is_taken_over_once() {
        let d = dir("legacy");
        let ring = d.join(LEGACY_RING);
        // The old ring: one 16 byte record per minute of a day at (minute % 1440), empty slots all zero.
        let mut bytes = vec![0u8; 1440 * 16];
        for m in 0..120u64 {
            let t = T0 + m * 60;
            let rec = s(t, 90 - (m / 10) as u8).encode();
            let slot = (t / 60 % 1440) as usize * 16;
            bytes[slot..slot + 16].copy_from_slice(&rec);
        }
        std::fs::write(&ring, &bytes).unwrap();
        let h = History::open(&d);
        assert_eq!(h.samples().len(), 120);
        assert!(h.samples().windows(2).all(|w| w[0].t < w[1].t), "in time order");
        assert!(d.join(FILE).exists(), "saved in the new format");
        assert!(!ring.exists(), "and the old file is gone");
        assert_eq!(History::open(&d).samples().len(), 120, "the second start reads the new file");
    }

    #[test]
    fn each_game_gets_the_stretches_it_ran_on_battery_and_nothing_across_a_change() {
        let d = dir("games");
        let mut rec = Recorder::open(&d);
        let key = |g: &str| Some(Key::new("gba", Path::new(g), "pocket-core"));
        let mut feed = |t: u64, cap: u8, game: Option<Key>| rec.feed_games(s(T0 + t * 60, cap), game, 0);
        assert!(!feed(0, 90, None), "the menus: nobody's");
        assert!(!feed(1, 90, key("a.gba")), "the first sample of a game: no stretch yet");
        assert!(feed(2, 89, key("a.gba")));
        assert!(feed(3, 89, key("a.gba")));
        assert!(!feed(4, 88, key("b.gba")), "another game: the minute between them is neither's");
        assert!(feed(5, 87, key("b.gba")));
        assert!(!feed(6, 87, None));
        let a = rec.games.games.iter().find(|e| e.game == "a.gba").unwrap();
        assert_eq!((a.secs, a.drop), (120.0, 1.0));
        let b = rec.games.games.iter().find(|e| e.game == "b.gba").unwrap();
        assert_eq!((b.secs, b.drop), (60.0, 1.0));
    }

    #[test]
    fn a_light_sleep_between_two_samples_is_nobodys_play() {
        // Review SOL 2026-10-24 #11: samples at 12:00 and 12:01 both see the game; it slept in the dark from 12:00:05
        // to 12:00:55. No sample saw it asleep, yet the minute must not count: the sleep changed the generation.
        let d = dir("light-sleep");
        let mut rec = Recorder::open(&d);
        let game = || Some(Key::new("gba", Path::new("a.gba"), "pocket-core"));
        let g0 = GENERATION.load(std::sync::atomic::Ordering::SeqCst);
        assert!(!rec.feed_games(s(T0, 90), game(), g0));
        assert!(rec.feed_games(s(T0 + 60, 90), game(), g0), "awake: the game's minute");
        set_asleep(true);
        set_asleep(false);
        let g1 = GENERATION.load(std::sync::atomic::Ordering::SeqCst);
        assert!(g1 >= g0 + 2, "into and out of sleep: two new generations");
        assert!(!rec.feed_games(s(T0 + 120, 89), game(), g1), "the minute with the sleep in it is nobody's");
        assert!(rec.feed_games(s(T0 + 180, 89), game(), g1), "the next one counts again");
        // A suspend nobody asked for (`sleep_boundary`) is the same.
        sleep_boundary();
        let g2 = GENERATION.load(std::sync::atomic::Ordering::SeqCst);
        assert!(!rec.feed_games(s(T0 + 240, 88), game(), g2));
        let a = rec.games.games.iter().find(|e| e.game == "a.gba").unwrap();
        assert_eq!(a.secs, 120.0, "two awake minutes, not four");
    }

    /// The sampler's shared state (`RECORDER`, `STOPPED`, `WRITING`) in one test, in its order: nothing read yet, read
    /// on a thread, written, stopped.
    #[test]
    fn the_card_never_reads_the_files_and_an_orderly_exit_leaves_no_temp_file() {
        let d = dir("lifecycle");
        let key = Key::new("gba", Path::new("a.gba"), "");
        // Review SOL 2026-10-24 #6: before the sampler has read its files the card says so, without a file read.
        assert_eq!(game_card(&key, "GBA").as_deref(), Some(LOADING));
        assert!(!loaded() && view().samples().is_empty());
        load_now(&d);
        assert!(loaded());
        assert_ne!(game_card(&key, "GBA").as_deref(), Some(LOADING));
        let g = GENERATION.load(std::sync::atomic::Ordering::SeqCst);
        record(&d, s(T0, 80), true, g);
        assert!(d.join(FILE).exists());
        // Review SOL 2026-10-24 #12: the exit waits for a write in progress, and nothing is written after it.
        let writing = std::thread::spawn(|| {
            let _w = WRITING.lock().unwrap();
            std::thread::sleep(Duration::from_millis(200));
        });
        std::thread::sleep(Duration::from_millis(50));
        let t0 = std::time::Instant::now();
        stop();
        assert!(t0.elapsed() >= Duration::from_millis(100), "it waited for the write");
        writing.join().unwrap();
        let before = std::fs::read(d.join(FILE)).unwrap();
        record(&d, s(T0 + 60, 79), true, g);
        assert_eq!(std::fs::read(d.join(FILE)).unwrap(), before, "no write after the stop");
        let temps: Vec<_> = std::fs::read_dir(&d).unwrap().flatten().filter(|e| e.file_name().to_string_lossy().ends_with(".tmp")).collect();
        assert!(temps.is_empty(), "{temps:?}");
    }

    /// Review 2026-10-24 (wake from deep sleep): the sample after a wake took three reads 100 ms apart and a write on
    /// the UI thread, right when the player woke the Pocket. Asking for it returns at once; the reads run elsewhere.
    #[test]
    fn the_sample_after_a_wake_never_holds_up_the_caller() {
        let bat = dir("wake-battery");
        for (name, value) in [("capacity", "77"), ("status", "Discharging"), ("voltage_now", "3900000"), ("current_now", "-400000")] {
            std::fs::write(bat.join(name), value).unwrap();
        }
        let data = dir("wake-data");
        let caller = std::thread::current().id();
        let t0 = std::time::Instant::now();
        let handle = spawn_sample(bat.to_path_buf(), data.to_path_buf()).expect("a thread");
        let asked = t0.elapsed();
        assert_ne!(handle.thread().id(), caller, "read on a thread of its own");
        assert!(asked < Duration::from_millis(50), "the caller waited {asked:?}");
        handle.join().unwrap();
        assert!(t0.elapsed() >= Duration::from_millis(200), "the steady read (three reads 100 ms apart) still happened");
    }

    fn fsutil_bak(p: &Path) -> PathBuf {
        crate::fsutil::backup_path(p)
    }
}
