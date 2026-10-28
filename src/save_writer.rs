//! The battery save, written off the game thread: `fsutil::write_atomic` syncs to the card, which can
//! take long enough to cost a frame. pocket-core copies the bytes on the game thread and hands them over
//! (`libretro::Core::write_sram_in_background`); a worker thread writes them. When a newer save arrives
//! while one is still being written, it replaces any that waits (only the newest matters).
//! On quit and before a sleep the save is written synchronously (`Core::write_sram`), after `wait`, so
//! nothing older can land behind it and it is on disk before the process ends.
//! The cartridge clock (`.rtc`, `Core`'s `MEMORY_RTC`) goes the same way, as a file of its own.
//! Every write's outcome is kept per file (`failed`, `trouble`): a failed one is written again at the next flush,
//! and while one fails the player is told (`save_trouble`). The file on the card stays the last good one meanwhile.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread::JoinHandle;

use crate::save_trouble::Trouble;

type WriteFn = dyn Fn(&Path, &[u8]) -> io::Result<()> + Send + Sync;

#[derive(Default)]
struct State {
    /// What waits to be written, one entry per file (the battery save, the cartridge clock): a newer copy of a file
    /// replaces the one waiting, the others stay.
    next: Vec<(PathBuf, Vec<u8>)>,
    busy: bool,
    /// The files whose newest write failed, and why; a file leaves the list when a write of it gets through.
    failing: Vec<(PathBuf, Trouble)>,
    quit: bool,
}

impl State {
    /// Keeps the outcome of a write of `path`. The log hears of a change only, not of every retry 10 s apart.
    fn note(&mut self, path: &Path, r: &io::Result<()>) {
        let before = self.failing.iter().position(|(p, _)| p == path).map(|i| self.failing.remove(i).1);
        match r {
            Err(e) => {
                let now = Trouble::of(e);
                if before != Some(now) {
                    eprintln!("battery save {}: {e} (tried again at every flush)", path.display());
                }
                self.failing.push((path.to_path_buf(), now));
            }
            Ok(()) if before.is_some() => eprintln!("battery save {}: written again", path.display()),
            Ok(()) => {}
        }
    }
}

struct Shared {
    state: Mutex<State>,
    changed: Condvar,
    write: Box<WriteFn>,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn write(&self, path: &Path, bytes: &[u8]) -> io::Result<()> {
        (self.write)(path, bytes)
    }
}

pub struct SaveWriter {
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
}

impl SaveWriter {
    pub fn new() -> SaveWriter {
        SaveWriter::with(crate::fsutil::write_atomic)
    }

    /// With another way of writing (tests: a card that is full, read-only or slow).
    pub fn with(write: impl Fn(&Path, &[u8]) -> io::Result<()> + Send + Sync + 'static) -> SaveWriter {
        let shared = Arc::new(Shared { state: Mutex::new(State::default()), changed: Condvar::new(), write: Box::new(write) });
        let s = shared.clone();
        let thread = std::thread::Builder::new().name("save-writer".into()).spawn(move || worker(&s)).ok();
        SaveWriter { shared, thread }
    }

    /// Hands `bytes` over to be written to `path`; returns at once. Replaces a save of the same file still waiting.
    pub fn submit(&self, path: PathBuf, bytes: Vec<u8>) {
        if self.thread.is_none() {
            // No thread could be started: write here.
            let _ = self.write_now(&path, &bytes);
            return;
        }
        let mut st = self.shared.lock();
        match st.next.iter_mut().find(|(p, _)| *p == path) {
            Some(waiting) => waiting.1 = bytes,
            None => st.next.push((path, bytes)),
        }
        drop(st);
        self.shared.changed.notify_all();
    }

    /// Writes `bytes` to `path` on this thread (quit, sleep; after `wait`), noting the outcome as the worker does.
    pub fn write_now(&self, path: &Path, bytes: &[u8]) -> io::Result<()> {
        let r = self.shared.write(path, bytes);
        self.shared.lock().note(path, &r);
        r
    }

    /// Waits until everything handed over has been tried.
    pub fn wait(&self) {
        let mut st = self.shared.lock();
        while self.thread.is_some() && (!st.next.is_empty() || st.busy) {
            st = self.shared.changed.wait(st).unwrap_or_else(|e| e.into_inner());
        }
    }

    /// Whether the newest write of `path` failed, without waiting: the next flush writes it again.
    pub fn failed(&self, path: &Path) -> bool {
        self.shared.lock().failing.iter().any(|(p, _)| p == path)
    }

    /// Why saves don't reach the card, if any file's newest write failed; without waiting (the 10 s flush).
    pub fn trouble(&self) -> Option<Trouble> {
        self.shared.lock().failing.first().map(|(_, t)| *t)
    }
}

fn worker(s: &Shared) {
    let mut st = s.lock();
    loop {
        if !st.next.is_empty() {
            let (path, bytes) = st.next.remove(0);
            st.busy = true;
            drop(st);
            let r = s.write(&path, &bytes);
            st = s.lock();
            st.busy = false;
            st.note(&path, &r);
            s.changed.notify_all();
        } else if st.quit {
            return;
        } else {
            st = s.changed.wait(st).unwrap_or_else(|e| e.into_inner());
        }
    }
}

impl Default for SaveWriter {
    fn default() -> SaveWriter {
        SaveWriter::new()
    }
}

impl Drop for SaveWriter {
    /// Writes what is still waiting, then ends the thread.
    fn drop(&mut self) {
        self.shared.lock().quit = true;
        self.shared.changed.notify_all();
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn writes_in_the_background_and_drops_superseded_saves() {
        let written = Arc::new(Mutex::new(Vec::<Vec<u8>>::new()));
        let log = written.clone();
        // A slow card: every write takes 300 ms.
        let w = SaveWriter::with(move |_, bytes| {
            std::thread::sleep(Duration::from_millis(300));
            log.lock().unwrap().push(bytes.to_vec());
            Ok(())
        });
        let t = Instant::now();
        w.submit("Game.srm".into(), b"A".to_vec());
        assert!(t.elapsed() < Duration::from_millis(100), "the game thread waited {:?} for the card", t.elapsed());
        std::thread::sleep(Duration::from_millis(50));
        // A is being written; B is replaced by C before its turn.
        w.submit("Game.srm".into(), b"B".to_vec());
        w.submit("Game.srm".into(), b"C".to_vec());
        assert!(t.elapsed() < Duration::from_millis(200), "the game thread waited {:?} for the card", t.elapsed());
        w.wait();
        assert_eq!(w.trouble(), None);
        assert_eq!(*written.lock().unwrap(), [b"A".to_vec(), b"C".to_vec()]);
    }

    #[test]
    fn a_failed_write_is_kept_until_one_of_that_file_gets_through() {
        // ENOSPC from a full card, without touching a real one.
        let full = Arc::new(Mutex::new(true));
        let card = full.clone();
        let w = SaveWriter::with(move |_, _| if *card.lock().unwrap() { Err(io::Error::from_raw_os_error(libc::ENOSPC)) } else { Ok(()) });
        w.submit("Game.srm".into(), b"A".to_vec());
        w.wait();
        assert!(w.failed(Path::new("Game.srm")));
        assert_eq!(w.trouble(), Some(Trouble::Full), "the reason reaches the flush, more than once");
        assert_eq!(w.trouble(), Some(Trouble::Full));
        assert!(!w.failed(Path::new("Game.rtc")), "per file");
        *full.lock().unwrap() = false;
        w.submit("Game.srm".into(), b"A".to_vec());
        w.wait();
        assert_eq!(w.trouble(), None, "space was freed: the next try clears it");
    }

    #[test]
    fn a_read_only_card_is_reported_from_a_write_on_the_spot_too() {
        let w = SaveWriter::with(|_, _| Err(io::Error::from_raw_os_error(libc::EROFS)));
        let e = w.write_now(Path::new("Game.srm"), b"A").unwrap_err();
        assert_eq!(e.raw_os_error(), Some(libc::EROFS));
        assert_eq!(w.trouble(), Some(Trouble::Unwritable));
    }

    #[test]
    fn the_clock_waiting_does_not_push_out_the_battery_save() {
        let written = Arc::new(Mutex::new(Vec::<(PathBuf, Vec<u8>)>::new()));
        let log = written.clone();
        let w = SaveWriter::with(move |path, bytes| {
            std::thread::sleep(Duration::from_millis(100));
            log.lock().unwrap().push((path.to_path_buf(), bytes.to_vec()));
            Ok(())
        });
        w.submit("Game.srm".into(), b"A".to_vec());
        std::thread::sleep(Duration::from_millis(30));
        w.submit("Game.srm".into(), b"B".to_vec());
        w.submit("Game.rtc".into(), b"T".to_vec());
        w.submit("Game.srm".into(), b"C".to_vec());
        w.wait();
        let got = written.lock().unwrap().clone();
        assert_eq!(got, [("Game.srm".into(), b"A".to_vec()), ("Game.srm".into(), b"C".to_vec()), ("Game.rtc".into(), b"T".to_vec())]);
    }
}
