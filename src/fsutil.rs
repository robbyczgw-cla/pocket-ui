//! Writing and reading the files a person would be sorry to lose (settings, favourites, game settings,
//! cheats ...), without a power cut or a full card turning them into 0 bytes or half a file.
//!
//! * [`write_atomic`]: temporary file next to the target, `fsync`, rename over the target, `fsync` of the
//!   directory. The target is always either the old or the new content. (Rename alone is not enough on
//!   the card's vfat/exFAT: without the `fsync` the rename can reach the disk before the data does.) The
//!   temporary file has a name of its own (`<name>.<pid>-<n>.tmp`) and is created exclusively without following
//!   links, so nothing found there is ever written through (review N3). [`replace_file`]: the same without `fsync`.
//! * [`write_json`]: the same, and the content is kept once more as `<name>.bak`, the last good copy.
//! * [`read_json_or_backup`]: reads the file; when it is there but unreadable (0 bytes, cut off, wrong
//!   shape), says so on stderr and uses the `.bak` instead of quietly starting from defaults.
//! * [`reap_temp_files`]: a writer killed between its temporary file and the rename leaves the temporary file;
//!   pocket-ui removes those of writers that are gone, a minute after its start ([`reap_temp_files_soon`]).

use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// A game file with nothing in it (0 bytes): a copy that never finished, or a placeholder. No emulator can start it, and
/// no picture belongs to it (new-user test 2026-10-30: an empty Tetris got a cover and a box like a healthy game).
pub fn is_empty_file(path: &Path) -> bool {
    std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.len() == 0)
}

/// `path` with `suffix` appended to the file name (`favorites.json` -> `favorites.json.bak`).
fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.file_name().map(|n| n.to_os_string()).unwrap_or_default();
    name.push(suffix);
    path.with_file_name(name)
}

/// Where [`write_json`] keeps the last good copy of `path`.
pub fn backup_path(path: &Path) -> PathBuf {
    with_suffix(path, ".bak")
}

fn parent_dir(path: &Path) -> Option<&Path> {
    path.parent().filter(|d| !d.as_os_str().is_empty())
}

/// Replaces `path` with `bytes` in one step: the file is the old content or the new one, never a mix or
/// empty. Creates the directory if needed.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    write_via_temp(path, bytes, true)
}

/// [`write_atomic`] without the `fsync`s, for files that may be lost in a power cut but must never be half written
/// or written through a link (a state's preview picture): nothing waits for the card.
pub fn replace_file(path: &Path, bytes: &[u8]) -> io::Result<()> {
    write_via_temp(path, bytes, false)
}

/// Numbers the temporary files of this process.
static TEMP_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// A new temporary file beside `path`, `<name>.<pid>-<n>.tmp`, made now: created exclusively and without following
/// links (`O_CREAT | O_EXCL | O_NOFOLLOW`), so whatever lies there already, a planted symlink included, is never
/// opened, written through or reused, and two writers of one file never share a temporary file (review N3).
fn create_temp(path: &Path) -> io::Result<(PathBuf, File)> {
    use std::os::unix::fs::OpenOptionsExt;
    let mut last = io::Error::from(io::ErrorKind::AlreadyExists);
    for _ in 0..16 {
        let n = TEMP_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let tmp = with_suffix(path, &format!(".{}-{n}.tmp", std::process::id()));
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(&tmp)
        {
            Ok(f) => return Ok((tmp, f)),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => last = e,
            Err(e) => return Err(e),
        }
    }
    Err(last)
}

#[cfg(test)]
thread_local! {
    /// Tests: the next write on this thread fails as a full card would.
    static FAIL_NEXT: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Tests: makes the next [`write_atomic`] / [`replace_file`] on this thread fail (a full card), leaving the file as
/// it was.
#[cfg(test)]
pub fn fail_next_write() {
    FAIL_NEXT.set(true);
}

fn write_via_temp(path: &Path, bytes: &[u8], durable: bool) -> io::Result<()> {
    let dir = parent_dir(path);
    if let Some(dir) = dir {
        fs::create_dir_all(dir)?;
    }
    let (tmp, mut f) = create_temp(path)?;
    let written = (|| {
        #[cfg(test)]
        if FAIL_NEXT.replace(false) {
            return Err(io::Error::other("no space left (test)"));
        }
        f.write_all(bytes)?;
        if durable {
            f.sync_all()?;
        }
        drop(f);
        // rename replaces `path` itself, a link there included; it never writes through it.
        fs::rename(&tmp, path)
    })();
    if let Err(e) = written {
        let _ = fs::remove_file(&tmp);
        return Err(e);
    }
    // The rename itself lives in the directory. Some filesystems refuse to sync a directory; the data is
    // safe by then, so that is not an error.
    if let (true, Some(dir)) = (durable, dir) {
        if let Ok(d) = File::open(dir) {
            let _ = d.sync_all();
        }
    }
    Ok(())
}

/// [`write_atomic`] for a value as pretty JSON, and the same bytes again as `<name>.bak` (after the file
/// itself, and only if the backup doesn't hold them yet). So there is always a last good copy to fall back
/// to if the file goes bad later (a card that lies about a flush, a tool that truncates it); a bad file is
/// never copied over it because only what was just serialized goes there.
pub fn write_json<T: serde::Serialize + ?Sized>(path: &Path, value: &T) -> io::Result<()> {
    let bytes = serde_json::to_vec_pretty(value).map_err(io::Error::other)?;
    // A write of this file (or any other) handed to the card writer before this one lands first.
    settle();
    write_json_bytes(path, &bytes)
}

fn write_json_bytes(path: &Path, bytes: &[u8]) -> io::Result<()> {
    write_atomic(path, bytes)?;
    let bak = backup_path(path);
    if fs::read(&bak).ok().as_deref() != Some(bytes) {
        if let Err(e) = write_atomic(&bak, bytes) {
            eprintln!("fsutil: no backup of {}: {e}", path.display());
        }
    }
    Ok(())
}

/// [`write_json`] on the card writer, a thread of its own: the value is serialized now, the four `fsync`s happen
/// there. For the lists a game's end updates (recent games, play time), which used to hold the first shelf frame
/// after a Quit for as long as the card took (each `fsync` waits for the card's flush; on an SD card that is
/// 10 to 50 ms, docs/LATENCY.md). Writes land in the order they were handed in, a newer one of a file still
/// waiting replaces it, and every [`write_json`] (any file) waits for them first, so nothing written later is ever
/// overtaken by an older copy. [`settle`] waits for them (pocket-ui does before it exits, a backup before it reads).
pub fn write_json_soon<T: serde::Serialize + ?Sized>(path: &Path, value: &T) {
    let bytes = match serde_json::to_vec_pretty(value) {
        Ok(b) => b,
        Err(e) => return eprintln!("fsutil: {} not saved: {e}", path.display()),
    };
    let mut q = lock_queue();
    let _ = q.put(PendingWrite { path: path.to_path_buf(), bytes, done: None });
    if !q.writer {
        q.writer = std::thread::Builder::new().name("card-writer".into()).spawn(card_writer).is_ok();
        if !q.writer {
            // No thread to be had: write here, as before.
            let write = q.waiting.pop_back().expect("just queued");
            drop(q);
            finish_write(write);
            return;
        }
    }
    QUEUE_CHANGED.notify_all();
}

/// A checked queued write either reached the card, or a newer checked value replaced it before it started.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WriteStatus {
    Saved,
    Superseded,
}

type WriteCompletion = Box<dyn FnOnce(io::Result<WriteStatus>) + Send>;

/// Queues a write without waiting for SD I/O; `done` receives its actual I/O result on the card writer. When a
/// newer checked value replaces a waiting one, the old callback receives `Ok(Superseded)` (not a save receipt),
/// possibly on this thread. Only the newest callback stays queued, so key repeat cannot accumulate callbacks.
/// An unchecked replacement retains that callback and reports the final file write. Callbacks must not block.
/// Serialization or worker-start failure returns Err now, without calling `done` or writing on this thread.
pub fn write_json_checked_soon<T: serde::Serialize + ?Sized>(path: &Path, value: &T, done: impl FnOnce(io::Result<WriteStatus>) + Send + 'static) -> io::Result<()> {
    let bytes = serde_json::to_vec_pretty(value).map_err(io::Error::other)?;
    let mut q = lock_queue();
    let replaced = q.put(PendingWrite { path: path.to_path_buf(), bytes, done: Some(Box::new(done)) });
    if !q.writer {
        match std::thread::Builder::new().name("card-writer".into()).spawn(card_writer) {
            Ok(_) => q.writer = true,
            Err(e) => {
                let _ = q.waiting.pop_back();
                drop(q);
                superseded(replaced);
                return Err(e);
            }
        }
    }
    QUEUE_CHANGED.notify_all();
    drop(q);
    superseded(replaced);
    Ok(())
}

fn superseded(done: Option<WriteCompletion>) {
    if let Some(done) = done {
        done(Ok(WriteStatus::Superseded));
    }
}

struct PendingWrite {
    path: PathBuf,
    bytes: Vec<u8>,
    done: Option<WriteCompletion>,
}

fn finish_write(write: PendingWrite) {
    let saved = write_json_bytes(&write.path, &write.bytes);
    if let Err(e) = &saved {
        eprintln!("fsutil: {} not saved: {e}", write.path.display());
    }
    if let Some(done) = write.done {
        done(saved.map(|()| WriteStatus::Saved));
    }
}

/// Waits until everything handed to [`write_json_soon`] is on the card. Returns at once when nothing is waiting.
pub fn settle() {
    let mut q = lock_queue();
    while !q.waiting.is_empty() || q.busy {
        q = QUEUE_CHANGED.wait(q).unwrap_or_else(|e| e.into_inner());
    }
}

struct Queue {
    waiting: std::collections::VecDeque<PendingWrite>,
    /// The writer has taken a write and not finished it.
    busy: bool,
    /// The writer thread runs.
    writer: bool,
}

impl Queue {
    fn put(&mut self, write: PendingWrite) -> Option<WriteCompletion> {
        if let Some(old) = self.waiting.iter_mut().find(|old| old.path == write.path) {
            old.bytes = write.bytes;
            return write.done.and_then(|done| old.done.replace(done));
        }
        self.waiting.push_back(write);
        None
    }
}

static QUEUE: std::sync::Mutex<Queue> = std::sync::Mutex::new(Queue { waiting: std::collections::VecDeque::new(), busy: false, writer: false });
static QUEUE_CHANGED: std::sync::Condvar = std::sync::Condvar::new();

fn lock_queue() -> std::sync::MutexGuard<'static, Queue> {
    QUEUE.lock().unwrap_or_else(|e| e.into_inner())
}

fn card_writer() {
    let mut q = lock_queue();
    loop {
        match q.waiting.pop_front() {
            Some(write) => {
                q.busy = true;
                drop(q);
                // Latency trace: what the card took for it (the shelf no longer waits for this).
                let path = crate::trace::on().then(|| write.path.clone());
                finish_write(write);
                if let Some(path) = path {
                    crate::trace::step(&format!("card writer: {} on the SD card", path.file_name().unwrap_or_default().to_string_lossy()));
                }
                q = lock_queue();
                q.busy = false;
                QUEUE_CHANGED.notify_all();
            }
            None => q = QUEUE_CHANGED.wait(q).unwrap_or_else(|e| e.into_inner()),
        }
    }
}

/// Deletes a file written by [`write_json`] together with its backup. True when the file itself is gone.
pub fn remove_json(path: &Path) -> bool {
    let _ = fs::remove_file(backup_path(path));
    fs::remove_file(path).is_ok() || !path.exists()
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, String> {
    let bytes = fs::read(path).map_err(|e| e.to_string())?;
    if bytes.is_empty() {
        return Err("0 bytes".into());
    }
    serde_json::from_slice(&bytes).map_err(|e| e.to_string())
}

/// The JSON in `path`. No file means `None` without a word (first start, or the file was deleted to reset
/// things). A file that is there but can't be used is reported, and then the backup is tried; `None` only
/// when that fails as well.
pub fn read_json_or_backup<T: serde::de::DeserializeOwned>(path: &Path) -> Option<T> {
    match read_json(path) {
        Ok(v) => return Some(v),
        Err(_) if !path.exists() => return None,
        Err(e) => eprintln!("fsutil: {} is unreadable ({e}); trying the backup", path.display()),
    }
    let bak = backup_path(path);
    match read_json(&bak) {
        Ok(v) => {
            eprintln!("fsutil: using the last good copy {}", bak.display());
            Some(v)
        }
        Err(e) => {
            eprintln!("fsutil: no usable backup of {} ({e})", path.display());
            None
        }
    }
}

/// How old a temporary file of a writer that still runs (or of a process that now has its pid) must be before
/// [`reap_temp_files`] takes it: the slowest atomic write (a 40 MB state on a slow card) takes seconds.
pub const TEMP_MAX_AGE: std::time::Duration = std::time::Duration::from_secs(15 * 60);

/// The pid in a temporary file name of [`create_temp`]'s: `<name>.<pid>-<n>.tmp` with a name before it, nothing else.
pub fn temp_owner(file_name: &str) -> Option<i32> {
    let rest = file_name.strip_suffix(".tmp")?;
    let (head, n) = rest.rsplit_once('-')?;
    let (name, pid) = head.rsplit_once('.')?;
    let digits = |s: &str| !s.is_empty() && s.len() <= 20 && s.bytes().all(|b| b.is_ascii_digit());
    (!name.is_empty() && digits(pid) && digits(n)).then(|| pid.parse().ok()).flatten()
}

/// Whether process `pid` exists (`kill(pid, 0)`: it does when that works or is refused for lack of rights).
fn process_alive(pid: i32) -> bool {
    if pid <= 0 {
        return false;
    }
    // SAFETY: signal 0 only asks whether the process exists.
    let alive = unsafe { libc::kill(pid, 0) } == 0;
    alive || io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

/// Removes the temporary files of [`write_atomic`] / [`replace_file`] below `dir` (to `depth` folders down) whose
/// writer is gone: an exit or a kill between creating one and renaming it leaves it for ever (review SOL 2026-10-24
/// #12: sixteen `battery-history.bin.<pid>-0.tmp` on the device). Only names of exactly that shape go, and only when
/// their process doesn't hold them open and either is no longer running or the file is older than `max_age` (a pid
/// used again by another process). A live writer's file is never touched. Links are not followed. Returns the
/// files removed.
pub fn reap_temp_files(dir: &Path, depth: usize, max_age: std::time::Duration) -> Vec<PathBuf> {
    let mut gone = Vec::new();
    let Ok(entries) = fs::read_dir(dir) else { return gone };
    for e in entries.flatten() {
        let Ok(kind) = e.file_type() else { continue };
        let path = e.path();
        if kind.is_dir() {
            if depth > 0 {
                gone.extend(reap_temp_files(&path, depth - 1, max_age));
            }
            continue;
        }
        if !kind.is_file() {
            continue;
        }
        let Some(pid) = e.file_name().to_str().and_then(temp_owner) else { continue };
        if held_open_by(pid, &path) {
            continue;
        }
        let old = e
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| std::time::SystemTime::now().duration_since(t).ok())
            .is_some_and(|age| age >= max_age);
        if (!process_alive(pid) || old) && fs::remove_file(&path).is_ok() {
            gone.push(path);
        }
    }
    gone
}

/// [`reap_temp_files`] on the data folder (five folders down: `states-v2/<system>/...`), on a thread of its own a
/// minute after the start, so nothing of it meets the first frames or a game's start. One log line when it took any.
pub fn reap_temp_files_soon(data_dir: PathBuf) {
    let _ = std::thread::Builder::new().name("temp-reaper".into()).spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(60));
        let gone = reap_temp_files(&data_dir, 5, TEMP_MAX_AGE);
        if !gone.is_empty() {
            let names: Vec<String> = gone.iter().take(4).map(|p| p.file_name().unwrap_or_default().to_string_lossy().into_owned()).collect();
            println!("fsutil: removed {} temporary file(s) of interrupted writes ({}{})", gone.len(), names.join(", "), if gone.len() > 4 { ", ..." } else { "" });
        }
    });
}

/// What tells one version of a file from another: which file it is (device, inode), when it was last changed and how
/// long it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stamp {
    dev: u64,
    ino: u64,
    mtime: i64,
    mtime_ns: i64,
    len: u64,
}

/// The [`Stamp`] of `path` now; None when there is no such file.
pub fn stamp(path: &Path) -> Option<Stamp> {
    use std::os::unix::fs::MetadataExt;
    let m = fs::metadata(path).ok()?;
    Some(Stamp {
        dev: m.dev(),
        ino: m.ino(),
        mtime: m.mtime(),
        mtime_ns: m.mtime_nsec(),
        len: m.len(),
    })
}

/// Whether process `pid` has `path` open (Linux, `/proc/<pid>/fd`); false where that can't be read.
pub fn held_open_by(pid: i32, path: &Path) -> bool {
    let Ok(target) = fs::canonicalize(path) else { return false };
    fs::read_dir(format!("/proc/{pid}/fd"))
        .into_iter()
        .flatten()
        .flatten()
        .any(|fd| fs::read_link(fd.path()).is_ok_and(|l| l == target))
}

/// Waits (up to `within`) for a write that was asked for to be complete: `path` is another file than `before` (its
/// [`Stamp`] taken just before the writer was asked) or has changed since, it isn't empty, it looks the same twice
/// `poll` apart, and the process `writer` (when known) no longer has it open. A file that was merely written a moment
/// before the request, or one still being written with a pause in between, is no confirmation (review SOL-12).
pub fn wait_new_version(path: &Path, before: Option<Stamp>, within: std::time::Duration, poll: std::time::Duration, writer: Option<i32>) -> bool {
    let end = std::time::Instant::now() + within;
    let mut last = None;
    loop {
        let now = stamp(path).filter(|s| Some(*s) != before && s.len > 0);
        if now.is_some() && now == last && !writer.is_some_and(|pid| held_open_by(pid, path)) {
            return true;
        }
        last = now;
        if std::time::Instant::now() + poll > end {
            return false;
        }
        std::thread::sleep(poll);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;
    use std::time::Duration;

    #[test]
    fn checked_writes_report_durable_success_and_io_failure_from_the_writer() {
        let d = dir("checked-soon");
        let path = d.join("library.json");
        let caller = std::thread::current().id();
        let (tx, rx) = std::sync::mpsc::channel();
        write_json_checked_soon(&path, &vec![7], move |result| {
            let _ = tx.send((std::thread::current().id(), result));
        }).unwrap();
        let (writer, result) = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_ne!(writer, caller, "the checked API never writes on its caller's thread");
        assert_eq!(result.unwrap(), WriteStatus::Saved);
        assert_eq!(fs::read(&path).unwrap(), fs::read(backup_path(&path)).unwrap(), "receipt follows the backup too");
        // A file where the parent directory needs to be: deterministic failure even when tests run as root.
        let blocked = d.join("blocked");
        fs::write(&blocked, b"keep").unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        write_json_checked_soon(&blocked.join("library.json"), &vec![8], move |result| { let _ = tx.send(result); }).unwrap();
        assert!(rx.recv_timeout(Duration::from_secs(5)).unwrap().is_err(), "an accepted request must still report SD I/O errors");
        assert_eq!(fs::read(blocked).unwrap(), b"keep");
    }

    #[test]
    fn coalescing_resolves_old_checked_callbacks_and_keeps_only_the_latest() {
        let d = dir("checked-coalesced");
        let path = d.join("library.json");
        let mut q = Queue { waiting: Default::default(), busy: false, writer: false };
        let (old_tx, old_rx) = std::sync::mpsc::channel();
        let (new_tx, new_rx) = std::sync::mpsc::channel();
        assert!(q.put(PendingWrite { path: path.clone(), bytes: b"[1]".to_vec(), done: Some(Box::new(move |result| { let _ = old_tx.send(result); })) }).is_none());
        superseded(q.put(PendingWrite { path: path.clone(), bytes: b"[2]".to_vec(), done: Some(Box::new(move |result| { let _ = new_tx.send(result); })) }));
        assert_eq!(old_rx.recv_timeout(Duration::from_secs(5)).unwrap().unwrap(), WriteStatus::Superseded);
        // Unchecked work on the same file must not discard the remaining error notification.
        assert!(q.put(PendingWrite { path: path.clone(), bytes: b"[3]".to_vec(), done: None }).is_none());
        assert_eq!(q.waiting.len(), 1);
        finish_write(q.waiting.pop_front().unwrap());
        assert_eq!(new_rx.recv_timeout(Duration::from_secs(5)).unwrap().unwrap(), WriteStatus::Saved);
        assert_eq!(fs::read(path).unwrap(), b"[3]");
    }

    #[test]
    fn writes_handed_to_the_card_writer_land_in_order_and_are_never_overtaken() {
        let d = dir("soon");
        fs::create_dir_all(&d).unwrap();
        let (a, b) = (d.join("recents.json"), d.join("library.json"));
        write_json_soon(&a, &vec![1]);
        write_json_soon(&b, &vec![2]);
        // A newer copy of a file still waiting replaces it; a later synchronous write comes after both.
        write_json_soon(&a, &vec![3]);
        write_json(&b, &vec![4]).unwrap();
        assert_eq!(fs::read_to_string(&a).unwrap().split_whitespace().collect::<String>(), "[3]");
        assert_eq!(fs::read_to_string(&b).unwrap().split_whitespace().collect::<String>(), "[4]");
        write_json_soon(&a, &vec![5]);
        settle();
        assert_eq!(fs::read_to_string(&a).unwrap().split_whitespace().collect::<String>(), "[5]");
        assert_eq!(fs::read_to_string(backup_path(&a)).unwrap().split_whitespace().collect::<String>(), "[5]", "with its backup");
    }

    #[test]
    fn a_file_written_just_before_the_request_is_no_confirmation() {
        // Review SOL-12: "written within the last second and the same size twice" took a file that was there
        // before as the one just asked for.
        let d = dir("version-old");
        fs::create_dir_all(&d).unwrap();
        let f = d.join("Game.s01");
        fs::write(&f, b"older state").unwrap();
        let before = stamp(&f);
        let ms = Duration::from_millis;
        assert!(!wait_new_version(&f, before, ms(300), ms(20), None), "nothing new was written");
        // Replaced (a new file renamed over it, as Dolphin and pocket-core write): confirmed.
        fs::write(d.join("tmp"), b"newer state!").unwrap();
        fs::rename(d.join("tmp"), &f).unwrap();
        assert!(wait_new_version(&f, before, ms(1000), ms(20), None));
        // Missing before and written now: confirmed; still missing: not.
        let g = d.join("Game.s02");
        assert!(!wait_new_version(&g, None, ms(100), ms(20), None));
        fs::write(&g, b"x").unwrap();
        assert!(wait_new_version(&g, None, ms(1000), ms(20), None));
    }

    #[test]
    fn a_file_still_being_written_is_waited_for() {
        let d = dir("version-growing");
        fs::create_dir_all(&d).unwrap();
        let f = d.join("Game.s03");
        let writer = {
            let f = f.clone();
            std::thread::spawn(move || {
                let mut out = File::create(&f).unwrap();
                for _ in 0..5 {
                    out.write_all(&[7u8; 1000]).unwrap();
                    out.flush().unwrap();
                    std::thread::sleep(Duration::from_millis(60));
                }
            })
        };
        assert!(wait_new_version(&f, None, Duration::from_secs(5), Duration::from_millis(100), None));
        writer.join().unwrap();
        assert_eq!(fs::metadata(&f).unwrap().len(), 5000, "confirmed only once the writer stopped");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_file_the_writer_still_holds_open_is_not_done() {
        let d = dir("version-open");
        fs::create_dir_all(&d).unwrap();
        let f = d.join("Game.s04");
        let mut open = File::create(&f).unwrap();
        open.write_all(b"half").unwrap();
        let me = std::process::id() as i32;
        assert!(held_open_by(me, &f));
        assert!(!wait_new_version(&f, None, Duration::from_millis(300), Duration::from_millis(20), Some(me)));
        drop(open);
        assert!(wait_new_version(&f, None, Duration::from_millis(500), Duration::from_millis(20), Some(me)));
    }

    #[test]
    fn only_temp_names_of_the_atomic_writer_are_owned_by_a_pid() {
        assert_eq!(temp_owner("battery-history.bin.4242-0.tmp"), Some(4242));
        assert_eq!(temp_owner("settings.json.bak.17-305.tmp"), Some(17));
        for other in ["Game.pstate.tmp", "a.tmp", "x.12-y.tmp", ".12-3.tmp", "x.-3.tmp", "x.12-.tmp", "x.12-3.tmpx", "x.12-3", "x.1a-3.tmp"] {
            assert_eq!(temp_owner(other), None, "{other}");
        }
    }

    #[test]
    fn temp_files_of_writers_that_are_gone_are_reaped_and_a_live_writers_never() {
        // Review SOL 2026-10-24 #12: a writer killed between its temporary file and the rename leaves it for ever.
        let d = dir("reap");
        let sub = d.join("game-settings/ports");
        fs::create_dir_all(&sub).unwrap();
        // A pid that surely ran and ended: a child we waited for.
        let dead = std::process::Command::new("true").spawn().unwrap();
        let dead_pid = dead.id() as i32;
        let mut dead = dead;
        dead.wait().unwrap();
        let me = std::process::id() as i32;
        let file = |p: PathBuf| {
            fs::write(&p, b"half").unwrap();
            p
        };
        let orphan = file(d.join(format!("battery-history.bin.{dead_pid}-0.tmp")));
        let nested = file(sub.join(format!("Deltarune.json.{dead_pid}-3.tmp")));
        // This process writing right now: held open, and one between its close and its rename (young).
        let open_one = file(d.join(format!("recents.json.{me}-90.tmp")));
        let _held = File::open(&open_one).unwrap();
        let young = file(d.join(format!("library.json.{me}-91.tmp")));
        // A file of a pid that runs again, but from long ago (another process before a reboot had that pid).
        let stale = file(d.join(format!("favorites.json.{me}-92.tmp")));
        File::options().write(true).open(&stale).unwrap().set_modified(std::time::SystemTime::now() - Duration::from_secs(3600)).unwrap();
        // Not ours at all.
        let foreign = [file(d.join("Game.pstate.tmp")), file(d.join("notes.tmp")), file(d.join(format!("x.{dead_pid}.tmp")))];
        // Below the depth it is told to look.
        let deep = d.join("a/b/c");
        fs::create_dir_all(&deep).unwrap();
        let too_deep = file(deep.join(format!("x.json.{dead_pid}-1.tmp")));
        let mut gone = reap_temp_files(&d, 2, Duration::from_secs(15 * 60));
        gone.sort();
        let mut want = vec![orphan.clone(), nested.clone(), stale.clone()];
        want.sort();
        assert_eq!(gone, want);
        for kept in [&open_one, &young, &too_deep].into_iter().chain(foreign.iter()) {
            assert!(kept.exists(), "{} kept", kept.display());
        }
    }

    fn dir(name: &str) -> crate::testutil::TestDir {
        TestDir::new(&format!("fsutil-{name}"))
    }

    #[test]
    fn write_atomic_creates_the_directory_replaces_and_leaves_no_temp_file() {
        let root = dir("atomic");
        let d = root.join("sub");
        let f = d.join("a.json");
        write_atomic(&f, b"one").unwrap();
        write_atomic(&f, b"two").unwrap();
        assert_eq!(fs::read(&f).unwrap(), b"two");
        let names: Vec<_> = fs::read_dir(&d).unwrap().map(|e| e.unwrap().file_name()).collect();
        assert_eq!(names, ["a.json"]);
    }

    #[test]
    fn a_planted_temp_link_is_never_written_through() {
        // Review N3: `<file>.tmp` was opened with File::create, which follows a symlink someone left there (the
        // states folder is synced, links included): the link's target was overwritten, and then renamed into place.
        let d = dir("symlink");
        fs::create_dir_all(&d).unwrap();
        let outside = d.join("outside.cfg");
        fs::write(&outside, b"keep me").unwrap();
        let state = d.join("Game.pstate");
        for name in ["Game.pstate.tmp", "Game.pstate.new"] {
            std::os::unix::fs::symlink(&outside, d.join(name)).unwrap();
        }
        write_atomic(&state, b"STATE-DATA").unwrap();
        assert_eq!(fs::read(&outside).unwrap(), b"keep me", "the link's target is untouched");
        assert!(!fs::symlink_metadata(&state).unwrap().file_type().is_symlink(), "the state is a file of its own");
        assert_eq!(fs::read(&state).unwrap(), b"STATE-DATA");
        // The target itself a link: replaced by the file, the link's target untouched.
        let linked = d.join("Linked.pstate");
        std::os::unix::fs::symlink(&outside, &linked).unwrap();
        replace_file(&linked, b"NEW").unwrap();
        assert_eq!(fs::read(&outside).unwrap(), b"keep me");
        assert!(!fs::symlink_metadata(&linked).unwrap().file_type().is_symlink());
        // No temporary file of ours is left behind.
        let left: Vec<String> = fs::read_dir(&d).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        assert!(left.iter().all(|n| !n.ends_with(".tmp") || n == "Game.pstate.tmp"), "{left:?}");
    }

    #[test]
    fn two_writers_of_one_file_do_not_share_a_temp_file() {
        let d = dir("two-writers");
        let f = d.join("a.bin");
        let threads: Vec<_> = (0..8u8)
            .map(|i| {
                let f = f.clone();
                std::thread::spawn(move || {
                    for _ in 0..20 {
                        write_atomic(&f, &[i; 4096]).unwrap();
                    }
                })
            })
            .collect();
        for t in threads {
            t.join().unwrap();
        }
        let back = fs::read(&f).unwrap();
        assert_eq!(back.len(), 4096);
        assert!(back.iter().all(|b| *b == back[0]), "one writer's content, whole");
        assert_eq!(fs::read_dir(&d).unwrap().count(), 1, "no temporary file left");
    }

    #[test]
    fn a_failed_write_leaves_the_old_file_alone() {
        let d = dir("failed");
        let f = d.join("a.txt");
        write_atomic(&f, b"keep me").unwrap();
        // The card is full: writing the temporary file fails.
        fail_next_write();
        assert!(write_atomic(&f, b"new").is_err());
        assert_eq!(fs::read(&f).unwrap(), b"keep me");
        assert_eq!(fs::read_dir(&d).unwrap().count(), 1, "the temporary file is removed");
    }

    #[test]
    fn a_relative_path_without_a_directory_works() {
        let d = dir("relative");
        fs::create_dir_all(&d).unwrap();
        let name = format!("pocketui-fsutil-rel-{}.txt", std::process::id());
        let f = Path::new(&name);
        write_atomic(f, b"x").unwrap();
        assert_eq!(fs::read(f).unwrap(), b"x");
        fs::remove_file(f).unwrap();
    }

    #[test]
    fn every_save_leaves_the_backup_at_the_same_content() {
        let d = dir("bak");
        let f = d.join("v.json");
        write_json(&f, &vec![1, 2]).unwrap();
        assert_eq!(read_json::<Vec<i32>>(&backup_path(&f)).unwrap(), [1, 2]);
        write_json(&f, &vec![1, 2, 3]).unwrap();
        assert_eq!(read_json::<Vec<i32>>(&backup_path(&f)).unwrap(), [1, 2, 3]);
        assert_eq!(read_json::<Vec<i32>>(&f).unwrap(), [1, 2, 3]);
    }

    #[test]
    fn a_damaged_file_is_never_copied_over_the_backup() {
        let d = dir("keep-bak");
        let f = d.join("v.json");
        write_json(&f, &vec![1]).unwrap();
        fs::write(&f, b"[1,").unwrap();
        assert_eq!(read_json_or_backup::<Vec<i32>>(&f), Some(vec![1]));
        assert_eq!(read_json::<Vec<i32>>(&backup_path(&f)).unwrap(), [1]);
        write_json(&f, &vec![3]).unwrap();
        assert_eq!(read_json::<Vec<i32>>(&f).unwrap(), [3]);
    }

    #[test]
    fn reading_falls_back_to_the_backup_for_empty_cut_off_and_wrong_shaped_files() {
        let d = dir("fallback");
        let f = d.join("v.json");
        write_json(&f, &vec![7, 8]).unwrap(); // the backup always follows the last save
        for bad in [&b""[..], b"[9", b"{\"x\": 1}", b"\0\0\0\0"] {
            fs::write(&f, bad).unwrap();
            assert_eq!(read_json_or_backup::<Vec<i32>>(&f), Some(vec![7, 8]), "{bad:?}");
        }
    }

    #[test]
    fn no_file_is_none_and_the_backup_is_not_brought_back() {
        let d = dir("missing");
        let f = d.join("v.json");
        assert_eq!(read_json_or_backup::<Vec<i32>>(&f), None);
        write_json(&f, &vec![1]).unwrap();
        write_json(&f, &vec![2]).unwrap();
        fs::remove_file(&f).unwrap();
        assert_eq!(read_json_or_backup::<Vec<i32>>(&f), None, "a deleted file stays deleted");
    }

    #[test]
    fn bad_file_and_bad_backup_is_none() {
        let d = dir("both-bad");
        let f = d.join("v.json");
        fs::create_dir_all(&d).unwrap();
        fs::write(&f, b"").unwrap();
        fs::write(backup_path(&f), b"[").unwrap();
        assert_eq!(read_json_or_backup::<Vec<i32>>(&f), None);
    }
}
