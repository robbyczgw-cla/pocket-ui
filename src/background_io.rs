//! Shelf jobs yield the card to games and updates.
//! A syscall already in the kernel can finish. The reader issues no further
//! reads or seeks once it observes cancellation, including inside a file.
use std::cell::Cell;
use std::fs;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

pub(crate) const READ_SIZE: usize = 64 * 1024;

const GAME: u64 = 1;
const UPDATE: u64 = 2;
const PAUSES: u64 = GAME | UPDATE;
const GENERATION: u64 = PAUSES + 1;

// A generation change invalidates old readers even if the pause has already ended.
static STATE: AtomicU64 = AtomicU64::new(0);
static UPDATES: Mutex<usize> = Mutex::new(0);
thread_local! {
    static JOB: Cell<Option<u64>> = const { Cell::new(None) };
}

fn set_pause(reason: u64, on: bool) {
    let _ = STATE.fetch_update(Ordering::AcqRel, Ordering::Acquire, |at| {
        ((at & reason != 0) != on).then(|| at.wrapping_add(GENERATION) ^ reason)
    });
}

pub(crate) fn set_game_on(on: bool) {
    set_pause(GAME, on);
}

pub(crate) fn game_on() -> bool {
    STATE.load(Ordering::Acquire) & GAME != 0
}

pub fn paused() -> bool {
    STATE.load(Ordering::Acquire) & PAUSES != 0
}

/// An update keeps the card until the last overlapping update operation finishes.
pub struct UpdatePause { _private: () }

pub fn pause_for_update() -> UpdatePause {
    let mut count = UPDATES.lock().unwrap();
    *count += 1;
    if *count == 1 { set_pause(UPDATE, true); }
    UpdatePause { _private: () }
}

impl Drop for UpdatePause {
    fn drop(&mut self) {
        let mut count = UPDATES.lock().unwrap();
        *count -= 1;
        if *count == 0 { set_pause(UPDATE, false); }
    }
}

struct Scope(Option<u64>);
impl Drop for Scope {
    fn drop(&mut self) {
        JOB.set(self.0);
    }
}

/// A completed shelf job can publish its result. Cancelled jobs can be queued again.
pub fn job<T>(run: impl FnOnce() -> T) -> Option<T> {
    let start = JOB.get().unwrap_or_else(|| STATE.load(Ordering::Acquire));
    let _scope = Scope(JOB.replace(Some(start)));
    check().ok()?;
    let result = run();
    check().ok().map(|()| result)
}

pub(crate) fn check() -> io::Result<()> {
    if JOB.get().is_some_and(|start| start & PAUSES != 0 || start != STATE.load(Ordering::Acquire)) {
        return Err(stop_error());
    }
    Ok(())
}

/// Whether a currently scoped shelf job lost access to the card.
pub fn cancelled() -> bool {
    check().is_err()
}

fn stop_error() -> io::Error {
    // Read::read_exact retries Interrupted.
    io::Error::other("shelf reader interrupted")
}

/// Checks for cancellation before each bounded read and each seek. An explicit predicate
/// also lets Identify stop for reasons other than a game.
pub(crate) struct Reader<R, F> {
    inner: R,
    stop: F,
}

impl<R, F: Fn() -> bool> Reader<R, F> {
    pub(crate) fn new(inner: R, stop: F) -> Self {
        Self { inner, stop }
    }

    fn check(&self) -> io::Result<()> {
        check()?;
        if (self.stop)() { Err(stop_error()) } else { Ok(()) }
    }
}

impl<R: Read, F: Fn() -> bool> Read for Reader<R, F> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.check()?;
        let limit = buf.len().min(READ_SIZE);
        self.inner.read(&mut buf[..limit])
    }
}

impl<R: Seek, F: Fn() -> bool> Seek for Reader<R, F> {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        self.check()?;
        self.inner.seek(pos)
    }
}

impl<F: Fn() -> bool> Reader<fs::File, F> {
    pub(crate) fn metadata(&self) -> io::Result<fs::Metadata> {
        self.check()?;
        self.inner.metadata()
    }
}

fn not_stopped() -> bool { false }

pub(crate) type File = Reader<fs::File, fn() -> bool>;

pub(crate) fn open(path: impl AsRef<Path>) -> io::Result<File> {
    open_until(path, not_stopped as fn() -> bool)
}

pub(crate) fn open_until<F: Fn() -> bool>(path: impl AsRef<Path>, stop: F) -> io::Result<Reader<fs::File, F>> {
    check()?;
    if stop() { return Err(stop_error()); }
    fs::File::open(path).map(|file| Reader::new(file, stop))
}

pub(crate) fn metadata(path: impl AsRef<Path>) -> io::Result<fs::Metadata> {
    check()?;
    fs::metadata(path)
}

pub fn exists(path: impl AsRef<Path>) -> bool {
    metadata(path).is_ok()
}

pub(crate) fn canonicalize(path: impl AsRef<Path>) -> io::Result<std::path::PathBuf> {
    check()?;
    fs::canonicalize(path)
}

pub(crate) fn is_file(path: impl AsRef<Path>) -> bool {
    metadata(path).is_ok_and(|m| m.is_file())
}

pub(crate) fn is_dir(path: impl AsRef<Path>) -> bool {
    metadata(path).is_ok_and(|m| m.is_dir())
}

pub(crate) fn read(path: impl AsRef<Path>) -> io::Result<Vec<u8>> {
    if JOB.get().is_none() { return fs::read(path); }
    let mut bytes = Vec::new();
    open(path)?.read_to_end(&mut bytes)?;
    Ok(bytes)
}

pub(crate) fn read_to_string(path: impl AsRef<Path>) -> io::Result<String> {
    if JOB.get().is_none() { return fs::read_to_string(path); }
    let mut text = String::new();
    open(path)?.read_to_string(&mut text)?;
    Ok(text)
}

pub(crate) struct ReadDir(fs::ReadDir);
impl Iterator for ReadDir {
    type Item = io::Result<fs::DirEntry>;

    fn next(&mut self) -> Option<Self::Item> {
        check().ok()?;
        self.0.next()
    }
}

pub(crate) fn read_dir(path: impl AsRef<Path>) -> io::Result<ReadDir> {
    check()?;
    fs::read_dir(path).map(ReadDir)
}

#[cfg(test)]
pub(crate) fn after_launch_request<T>(read: impl FnOnce() -> T) -> T {
    let _lock = crate::covers::READER_TEST.lock().unwrap_or_else(|e| e.into_inner());
    let mut result = None;
    job(|| {
        set_game_on(true);
        set_game_on(false);
        result = Some(read());
    });
    result.expect("the job had started before the launch request")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_source_rule_background_reads_have_the_same_bytes_and_errors() {
        let dir = crate::testutil::TestDir::new("shelf-reader-bytes");
        let path = dir.join("fixture");
        let _lock = crate::covers::READER_TEST.lock().unwrap_or_else(|e| e.into_inner());
        let missing = job(|| read(&path)).unwrap().unwrap_err();
        assert_eq!(missing.kind(), io::ErrorKind::NotFound);
        let whole = b"valid UTF-8\n".as_slice();
        let mut flipped = whole.to_vec();
        flipped[1] ^= 0x20;
        for bytes in [b"".as_slice(), &whole[..4], &flipped, whole, b"\xff\xfe"] {
            fs::write(&path, bytes).unwrap();
            assert_eq!(job(|| read(&path).unwrap()).unwrap(), bytes);
            let text = job(|| read_to_string(&path)).unwrap();
            assert_eq!(text.as_deref().ok(), std::str::from_utf8(bytes).ok());
        }
    }

    #[test]
    fn r36s_readers_a_cancelled_job_cannot_read_seek_walk_or_stat() {
        let dir = crate::testutil::TestDir::new("shelf-reader-gate");
        let path = dir.join("game.n64");
        std::fs::write(&path, b"fixture").unwrap();
        let _lock = crate::covers::READER_TEST.lock().unwrap_or_else(|e| e.into_inner());
        let discarded = job(|| {
            let mut file = open(&path).unwrap();
            let mut entries = read_dir(&dir).unwrap();
            let mut bytes = [0; 4];
            file.read_exact(&mut bytes).unwrap();
            assert_eq!(&bytes, b"fixt");
            set_game_on(true);
            set_game_on(false);
            assert!(file.read_exact(&mut bytes).is_err());
            assert!(file.seek(SeekFrom::Start(0)).is_err());
            assert!(entries.next().is_none());
            assert!(metadata(&path).is_err());
        });
        assert!(discarded.is_none());
        assert_eq!(job(|| read(&path).unwrap()), Some(b"fixture".to_vec()));
    }

    #[test]
    fn r36s_fix_a_overlapping_search_install_and_game_keep_the_card_paused() {
        let _lock = crate::covers::READER_TEST.lock().unwrap_or_else(|e| e.into_inner());
        let discarded = job(|| {
            let mut file = Reader::new(std::io::Cursor::new(b"fixture"), || false);
            let mut bytes = [0; 4];
            file.read_exact(&mut bytes).unwrap();
            let search = pause_for_update();
            let install = pause_for_update();
            assert!(file.read_exact(&mut bytes).is_err());
            drop(search);
            assert!(paused() && !game_on());
            set_game_on(true);
            drop(install);
            assert!(paused() && game_on());
            set_game_on(false);
            assert!(!paused());
            assert!(file.seek(SeekFrom::Start(0)).is_err());
        });
        assert!(discarded.is_none());
        assert_eq!(job(|| 7), Some(7));
    }

    #[test]
    fn r36s_readers_one_file_read_is_bounded_and_cancellation_is_not_retried() {
        let source = std::io::Cursor::new(vec![1; READ_SIZE * 3]);
        let stop = std::cell::Cell::new(false);
        let mut reader = Reader::new(source, || stop.get());
        let mut bytes = vec![0; READ_SIZE * 3];
        assert_eq!(reader.read(&mut bytes).unwrap(), READ_SIZE);
        stop.set(true);
        assert!(reader.read_exact(&mut bytes).is_err());
        assert_eq!(reader.inner.position(), READ_SIZE as u64);
    }
}
