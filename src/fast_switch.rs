//! Fast game switching: where the time between "A in the switcher" and the next game's first frame goes, and the
//! hand-over that lets the next game start while the one before is still tearing down.
//!
//! **Timing.** [`mark`] writes one line `switch-time <CLOCK_MONOTONIC ms> <program> <step>` into the log. Both
//! programs read the same clock, so pocket-ui's and pocket-core's lines can be subtracted directly (the card copy of
//! the log, `logs/pocket-ui.log`, stamps lines only once a second, too coarse for this). A handful of lines per game
//! start and end, nothing per frame: `grep switch-time /var/log/pocket-ui.log`.
//!
//! **Hand-over.** Without it the next game starts when the process of the one before has ended: its exit save, then
//! tearing down the core, GL, the window and the sound stream, then the process exit, then pocket-ui hearing of it.
//! With it, when the switcher goes from one pocket-core game to another ([`overlap_allowed`]):
//!
//! 1. pocket-ui starts every pocket-core with the write end of a pipe ([`Handoff`], [`HANDOFF_FD`]) and sends
//!    [`QUIT_FOR_SWITCH`] instead of closing the window.
//! 2. pocket-core writes its exit save (battery save, then the resume state, each through `fsutil::write_atomic`:
//!    temp file, fsync, rename), puts the CPU and fan settings back, stops the rumble, and only then writes
//!    [`SAVED`] into the pipe ([`announce_saved`]). PipeWire's quantum is left as it is: the next game sets the same.
//! 3. That line is when pocket-ui ends the game for itself and starts the next one. The old process tears down
//!    alongside; pocket-ui still reaps it and stops it as after any Quit if it hangs.
//!
//! A pocket-core that crashes or is killed before step 2 is complete (mid-save included) closes the pipe without the
//! line, and pocket-ui waits for the process to end as it always did. A save in progress is either the old file or
//! the new one, never half of one. What the old process still does after the line only touches its own game's files
//! (the battery save again, unchanged, so not rewritten) or nothing on disk; that is why only cores whose unload
//! writes nothing of their own may hand over ([`may_hand_over`]), and why the next game must not share a save file with it
//! ([`same_saves`]).

use std::io::{BufRead, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

/// Environment variable with the number of the descriptor pocket-core writes [`SAVED`] into.
pub const HANDOFF_FD: &str = "POCKET_HANDOFF_FD";
/// The line pocket-core writes once its exit save is complete and nothing of it touches shared state any more.
pub const SAVED: &str = "saved";
/// pocket-core's quit command for a switch to another pocket-core game: the exit save as with `quit`, then the
/// hand-over line.
pub const QUIT_FOR_SWITCH: &str = "quit handoff";
/// The line pocket-core writes into the same pipe when it has taken `pause` and draws no more. pocket-ui puts its
/// window over the game only then ([`wait_paused`]): a pocket-core still inside a frame when sway hides it waits in
/// `eglSwapBuffers` for a frame callback a hidden window never gets, and from then on reads no command, the Quit
/// included. (The screenshot and the `swaymsg` calls before the raise used to leave it time enough.)
pub const PAUSED: &str = "paused";

/// Cores whose process may still be tearing down while the next game starts: everything they keep is the battery
/// save pocket-core writes (`retro_get_memory_data`) and the states it writes, and their `retro_unload_game` and
/// `retro_deinit` write no files of their own. Tested in the virtual device (Genesis Plus GX with a cartridge: the
/// exit wrote the state and nothing else); 3D cores (GL teardown, shader caches), cores with memory cards or shared
/// files (PPSSPP's memory stick, Flycast's VMUs) and anything not listed wait for the process to end, as before.
pub const OVERLAP_CORES: &[&str] = &["gambatte", "mgba", "nestopia", "snes9x", "genesis_plus_gx"];
/// Systems whose games write files of their own at unload even on a listed core: Genesis Plus GX keeps the Mega CD's
/// backup RAM (`scd_*.brm`, shared by every CD game) and writes it when the game is unloaded.
const UNLOAD_WRITES: &[&str] = &["segacd", "megacd"];

/// Whether a game of `system` on `core` may hand over (see the module docs). pocket-ui asks before it sends
/// [`QUIT_FOR_SWITCH`], pocket-core again before it writes the line.
pub fn may_hand_over(core: &str, system: &str) -> bool {
    OVERLAP_CORES.contains(&core) && !UNLOAD_WRITES.contains(&system)
}

/// `POCKETUI_FAST_SWITCH=0` turns the hand-over off: the switcher waits for the old process to end, as before (to
/// compare on the device; `dev.env`).
pub fn enabled() -> bool {
    std::env::var_os("POCKETUI_FAST_SWITCH").is_none_or(|v| v != "0")
}

/// A game as far as switching goes.
#[derive(Clone, Copy, Debug)]
pub struct Game<'a> {
    pub emulator: &'a str,
    pub core: &'a str,
    pub system: &'a str,
    pub rom: &'a Path,
}

/// Whether the next game may start while the process of `old` is still ending (see the module docs): both run in
/// pocket-core, `old` [`may_hand_over`], and they share no save file.
pub fn overlap_allowed(old: Game, new: Game) -> bool {
    old.emulator == "pocket-core" && new.emulator == "pocket-core" && may_hand_over(old.core, old.system) && !same_saves(old, new)
}

/// Whether two games could touch the same files: the same ROM, or the same name in the same folder (the battery
/// save lives next to the ROM, `<stem>.srm`) or in the same system (states, `savestates/<system>/<stem>.*`).
pub fn same_saves(a: Game, b: Game) -> bool {
    let stem = |g: Game| g.rom.file_stem().map(|s| s.to_os_string());
    a.rom == b.rom || (stem(a) == stem(b) && (a.rom.parent() == b.rom.parent() || a.system == b.system))
}

// ---- timing ---------------------------------------------------------------------------------------------

/// CLOCK_MONOTONIC in milliseconds: the same clock in every process.
pub fn clock_ms() -> f64 {
    let mut ts = libc::timespec { tv_sec: 0, tv_nsec: 0 };
    // SAFETY: clock_gettime only writes the timespec it is given.
    unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut ts) };
    ts.tv_sec as f64 * 1000.0 + ts.tv_nsec as f64 / 1e6
}

/// Logs that `step` of a game's start or end has just happened (see the module docs).
pub fn mark(step: &str) {
    println!("switch-time {:.1} {} {step}", clock_ms(), program());
    // The same step in the latency trace, to the microsecond (`trace::step`).
    crate::trace::step(step);
}

fn program() -> &'static str {
    static NAME: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    NAME.get_or_init(|| {
        std::env::args_os()
            .next()
            .and_then(|a| Path::new(&a).file_name().map(|n| n.to_string_lossy().into_owned()))
            .unwrap_or_else(|| "?".into())
    })
}

/// pocket-core: the first frame is on the screen. Logged once per process; one relaxed load per frame after that.
pub fn first_frame() {
    static DONE: AtomicBool = AtomicBool::new(false);
    if !DONE.load(Ordering::Relaxed) && !DONE.swap(true, Ordering::Relaxed) {
        mark("first frame");
    }
}

// ---- pocket-core's side ---------------------------------------------------------------------------------

static HANDOFF: Mutex<Option<std::fs::File>> = Mutex::new(None);
static REQUESTED: AtomicBool = AtomicBool::new(false);

/// pocket-core at its start, before any thread or child process: notes the start in the log and takes the hand-over
/// pipe ([`take_handoff_fd`]).
pub fn process_start() {
    mark("process start");
    #[cfg(unix)]
    take_handoff_fd();
}

/// Takes the descriptor pocket-ui passed in [`HANDOFF_FD`] and keeps it from the programs pocket-core starts itself.
#[cfg(unix)]
fn take_handoff_fd() {
    use std::os::fd::FromRawFd;
    let Some(fd) = std::env::var(HANDOFF_FD).ok().and_then(|v| v.parse::<i32>().ok()).filter(|fd| *fd > 2) else { return };
    // SAFETY: fcntl on a number we were given; it fails (and we stop) when that is no open descriptor.
    if unsafe { libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) } == -1 {
        return;
    }
    // SAFETY: the descriptor is open (fcntl above) and pocket-ui passed it to this process alone to own.
    let file = unsafe { std::fs::File::from_raw_fd(fd) };
    if let Ok(mut h) = HANDOFF.lock() {
        *h = Some(file);
    }
}

/// pocket-core got [`QUIT_FOR_SWITCH`]: the hand-over line is due once the exit is far enough.
pub fn request() {
    REQUESTED.store(true, Ordering::SeqCst);
}

pub fn requested() -> bool {
    REQUESTED.load(Ordering::SeqCst)
}

/// pocket-core, on `pause`: writes [`PAUSED`] into pocket-ui's pipe (if it gave us one), which stays open.
pub fn announce_paused() {
    if let Some(f) = HANDOFF.lock().ok().as_mut().and_then(|h| h.as_mut()) {
        let _ = f.write_all(format!("{PAUSED}\n").as_bytes());
    }
}

/// pocket-core, after the exit save and putting the system back: writes [`SAVED`] and closes the pipe, if a
/// hand-over was asked for and pocket-ui gave us one. True when the line went out.
pub fn announce_saved() -> bool {
    if !requested() {
        return false;
    }
    let Some(mut f) = HANDOFF.lock().ok().and_then(|mut h| h.take()) else { return false };
    let ok = f.write_all(format!("{SAVED}\n").as_bytes()).is_ok();
    if ok {
        mark("hand-over: saved, the next game may start");
    }
    ok
}

// ---- pocket-ui's side -----------------------------------------------------------------------------------

/// Reads what a pocket-core writes into its hand-over pipe until the pipe closes: true as soon as the line
/// [`SAVED`] came; false when the pipe closed (or broke) without it, which is what a crash or a kill looks like.
/// Each [`PAUSED`] on the way calls `paused`.
pub fn saved_line(r: impl std::io::Read, mut paused: impl FnMut()) -> bool {
    for l in std::io::BufReader::new(r).lines().map_while(Result::ok) {
        if l == SAVED {
            return true;
        }
        if l == PAUSED {
            paused();
        }
    }
    false
}

/// How many [`PAUSED`] lines each game's process group has sent, newest groups last.
static PAUSES: Mutex<Vec<(i32, u64)>> = Mutex::new(Vec::new());
static PAUSES_CHANGED: Condvar = Condvar::new();

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn note_paused(pgid: i32) {
    if let Ok(mut v) = PAUSES.lock() {
        match v.iter_mut().find(|(g, _)| *g == pgid) {
            Some(e) => e.1 += 1,
            None => {
                v.push((pgid, 1));
                if v.len() > ENDED_KEEP {
                    v.remove(0);
                }
            }
        }
    }
    PAUSES_CHANGED.notify_all();
}

/// pocket-ui, before it sends `pause`: the count [`wait_paused`] waits to go past.
pub fn pauses(pgid: i32) -> u64 {
    PAUSES.lock().ok().and_then(|v| v.iter().find(|(g, _)| *g == pgid).map(|e| e.1)).unwrap_or(0)
}

/// pocket-ui, after `pause`: waits up to `timeout` for pocket-core `pgid` to say it has paused (the count goes past
/// `before`); true when it did. A pocket-core without the pipe never says so: that costs the timeout.
pub fn wait_paused(pgid: i32, before: u64, timeout: Duration) -> bool {
    let until = Instant::now() + timeout;
    let Ok(mut v) = PAUSES.lock() else { return false };
    loop {
        if v.iter().any(|(g, n)| *g == pgid && *n > before) {
            return true;
        }
        let now = Instant::now();
        if now >= until {
            return false;
        }
        v = match PAUSES_CHANGED.wait_timeout(v, until - now) {
            Ok((v, _)) => v,
            Err(_) => return false,
        };
    }
}

/// The pipe a pocket-core gets to announce its exit save (see the module docs).
#[cfg(target_os = "linux")]
pub struct Handoff {
    read: std::os::fd::OwnedFd,
    write: std::os::fd::OwnedFd,
}

#[cfg(target_os = "linux")]
impl Handoff {
    /// Gives `cmd` the write end: by number in [`HANDOFF_FD`], inherited by that child only (both ends are
    /// close-on-exec here, so nothing else pocket-ui starts meanwhile gets it). None when no pipe could be made.
    pub fn attach(cmd: &mut std::process::Command) -> Option<Handoff> {
        use std::os::fd::{AsRawFd, FromRawFd};
        use std::os::unix::process::CommandExt;
        let mut fds = [0; 2];
        // SAFETY: pipe2 writes two descriptors into the array it is given.
        if unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC) } != 0 {
            return None;
        }
        // SAFETY: both were just opened by pipe2 and are owned by nothing else.
        let (read, write) = unsafe { (std::os::fd::OwnedFd::from_raw_fd(fds[0]), std::os::fd::OwnedFd::from_raw_fd(fds[1])) };
        let fd = write.as_raw_fd();
        cmd.env(HANDOFF_FD, fd.to_string());
        // SAFETY: fcntl is async-signal-safe; in the child it only clears close-on-exec of the one descriptor.
        unsafe {
            cmd.pre_exec(move || {
                if libc::fcntl(fd, libc::F_SETFD, 0) == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        Some(Handoff { read, write })
    }

    /// After the spawn of process group `pgid`: closes our write end (the child has its own) and calls `on_saved`
    /// from a thread when the child announces its exit save. Nothing is called when the pipe closes without it.
    /// Its pauses are counted for [`wait_paused`].
    pub fn watch(self, pgid: i32, on_saved: impl FnOnce() + Send + 'static) {
        let Handoff { read, write } = self;
        drop(write);
        std::thread::spawn(move || {
            if saved_line(std::fs::File::from(read), || note_paused(pgid)) {
                on_saved();
            }
        });
    }
}

/// Processes of games that have ended, as the threads waiting for them saw it: lets a start wait for a game that
/// is still tearing down after its hand-over (the same game again, or one sharing its saves).
static ENDED: Mutex<Vec<i32>> = Mutex::new(Vec::new());
static ENDED_CHANGED: Condvar = Condvar::new();
/// The newest this many ended processes are remembered.
const ENDED_KEEP: usize = 16;

/// The game's process group `pgid` has ended (its leader was reaped).
pub fn note_ended(pgid: i32) {
    if let Ok(mut v) = ENDED.lock() {
        v.retain(|p| *p != pgid);
        v.push(pgid);
        if v.len() > ENDED_KEEP {
            v.remove(0);
        }
    }
    ENDED_CHANGED.notify_all();
}

/// Waits up to `timeout` for [`note_ended`] of `pgid`; true when it ended.
pub fn wait_ended(pgid: i32, timeout: Duration) -> bool {
    let until = Instant::now() + timeout;
    let Ok(mut v) = ENDED.lock() else { return false };
    loop {
        if v.contains(&pgid) {
            return true;
        }
        let now = Instant::now();
        if now >= until {
            return false;
        }
        v = match ENDED_CHANGED.wait_timeout(v, until - now) {
            Ok((v, _)) => v,
            Err(_) => return false,
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;
    use std::path::PathBuf;

    fn game<'a>(emulator: &'a str, core: &'a str, system: &'a str, rom: &'a PathBuf) -> Game<'a> {
        Game { emulator, core, system, rom }
    }

    #[test]
    fn only_listed_pocket_core_games_hand_over_to_pocket_core_games() {
        let (snes, gba, psp, other_snes) = (
            PathBuf::from("/storage/roms/snes/Contra III.smc"),
            PathBuf::from("/storage/roms/gba/MegaMan.gba"),
            PathBuf::from("/storage/roms/psp/Game.iso"),
            PathBuf::from("/storage/roms/snes/Zelda.smc"),
        );
        let pc = |core, system, rom| game("pocket-core", core, system, rom);
        assert!(overlap_allowed(pc("snes9x", "snes", &snes), pc("mgba", "gba", &gba)));
        assert!(overlap_allowed(pc("mgba", "gba", &gba), pc("snes9x", "snes", &snes)));
        assert!(overlap_allowed(pc("snes9x", "snes", &snes), pc("snes9x", "snes", &other_snes)), "two games of one system");
        assert!(overlap_allowed(pc("mgba", "gba", &gba), pc("ppsspp", "psp", &psp)), "a heavy core may start next to a light one ending");
        assert!(!overlap_allowed(pc("ppsspp", "psp", &psp), pc("mgba", "gba", &gba)), "a 3D core ends before the next starts");
        assert!(!overlap_allowed(pc("mgba", "gba", &gba), game("retroarch", "mgba", "gba", &gba)), "RetroArch next: as before");
        assert!(!overlap_allowed(game("retroarch", "snes9x", "snes", &snes), pc("mgba", "gba", &gba)), "RetroArch ending: as before");
        assert!(!overlap_allowed(game("ps2", "pcsx2", "ps2", &psp), pc("mgba", "gba", &gba)), "standalone ending: as before");
        assert!(!overlap_allowed(pc("snes9x", "snes", &snes), pc("snes9x", "snes", &snes)), "the same game again waits");
        let (md, cd) = (PathBuf::from("/storage/roms/megadrive/Sonic.md"), PathBuf::from("/storage/roms/segacd/Sonic CD.chd"));
        assert!(overlap_allowed(pc("genesis_plus_gx", "megadrive", &md), pc("mgba", "gba", &gba)), "Mega Drive cartridge");
        assert!(!overlap_allowed(pc("genesis_plus_gx", "segacd", &cd), pc("mgba", "gba", &gba)), "Mega CD writes its backup RAM at unload");
    }

    #[test]
    fn games_that_could_share_a_save_file_never_overlap() {
        let a = PathBuf::from("/storage/roms/snes/Game.smc");
        let b = PathBuf::from("/storage/roms/snes/Game.sfc");
        let c = PathBuf::from("/storage/roms/snes/sub/Game.smc");
        let d = PathBuf::from("/storage/roms/gb/Game.gb");
        let e = PathBuf::from("/storage/roms/gbc/Game.gbc");
        let pc = |system, rom| game("pocket-core", "snes9x", system, rom);
        assert!(same_saves(pc("snes", &a), pc("snes", &b)), "same .srm next to both");
        assert!(same_saves(pc("snes", &a), pc("snes", &c)), "same states in savestates/snes");
        assert!(!same_saves(pc("gb", &d), pc("gbc", &e)), "other folder, other system");
        assert!(same_saves(pc("gb", &d), pc("gb", &d)));
    }

    #[test]
    fn the_saved_line_counts_only_when_it_came_before_the_pipe_closed() {
        let none = || {};
        assert!(saved_line(&b"saved\n"[..], none));
        assert!(saved_line(&b"saved"[..], none), "a last line without its newline");
        assert!(!saved_line(&b""[..], none), "closed without a word: crashed or killed");
        assert!(!saved_line(&b"sav"[..], none), "cut off mid-line");
        assert!(!saved_line(&b"saved later\n"[..], none));
        let mut paused = 0;
        assert!(saved_line(&b"paused\npaused\nsaved\n"[..], || paused += 1), "pauses on the way don't end it");
        assert_eq!(paused, 2);
    }

    #[test]
    fn nothing_is_announced_unless_asked() {
        // Not requested in this test process: no line, even without a pipe to write to.
        if !requested() {
            assert!(!announce_saved());
        }
    }

    #[test]
    fn waiting_for_a_pause() {
        let pgid = 9_200_001;
        let before = pauses(pgid);
        assert!(!wait_paused(pgid, before, Duration::from_millis(20)), "nothing said yet");
        let t = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(30));
            note_paused(pgid);
        });
        let t0 = Instant::now();
        assert!(wait_paused(pgid, before, Duration::from_secs(5)));
        assert!(t0.elapsed() < Duration::from_secs(2), "woke up at once");
        t.join().unwrap();
        assert!(!wait_paused(pgid, pauses(pgid), Duration::from_millis(10)), "an earlier pause doesn't count for the next");
    }

    #[test]
    fn waiting_for_an_ended_game() {
        let pgid = 9_000_001;
        assert!(!wait_ended(pgid, Duration::from_millis(20)));
        let t = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(30));
            note_ended(pgid);
        });
        let t0 = Instant::now();
        assert!(wait_ended(pgid, Duration::from_secs(5)));
        assert!(t0.elapsed() < Duration::from_secs(2), "woke up at once, not at the timeout");
        t.join().unwrap();
        for p in 0..ENDED_KEEP as i32 + 1 {
            note_ended(9_100_000 + p);
        }
        assert!(!ENDED.lock().unwrap().contains(&9_100_000), "only the newest are kept");
    }

    /// The hand-over with real processes: the line arrives while the child still runs (the next game starts
    /// before the old process has ended), and a child that dies first never counts as saved.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_child_announces_before_it_ends_and_a_killed_one_never_does() {
        use std::sync::mpsc;
        let run = |script: &str| {
            let mut cmd = std::process::Command::new("sh");
            cmd.args(["-c", script]);
            let h = Handoff::attach(&mut cmd).expect("pipe");
            let mut child = cmd.spawn().unwrap();
            let (tx, rx) = mpsc::channel();
            h.watch(0, move || tx.send(()).unwrap());
            let saved = rx.recv_timeout(Duration::from_secs(5)).is_ok();
            let running_then = child.try_wait().unwrap().is_none();
            child.wait().unwrap();
            (saved, running_then)
        };
        // `/dev/fd/N` rather than `>&N`: dash (Debian's sh) only takes single-digit descriptors there, and with the
        // whole suite running the pipe's number can have two.
        let (saved, running) = run(r#"echo saved > "/dev/fd/$POCKET_HANDOFF_FD"; sleep 0.5"#);
        assert!(saved && running, "saved {saved}, child still running {running}");
        let (saved, _) = run("kill -9 $$");
        assert!(!saved, "a process killed before its line never counts as saved");
        let (saved, _) = run(r#"printf sav > "/dev/fd/$POCKET_HANDOFF_FD"; kill -9 $$"#);
        assert!(!saved, "nor one killed mid-line");
    }

    /// What the line promises: whoever acts on it finds the exit save complete on disk. A stand-in for pocket-core
    /// writes the state the way `fsutil::write_atomic` does (temp file, sync, rename) and announces afterwards.
    #[cfg(target_os = "linux")]
    #[test]
    fn the_save_is_complete_when_the_line_arrives() {
        use std::sync::mpsc;
        let dir = TestDir::new("handoff");
        let state = dir.join("game.pstate.auto");
        std::fs::write(&state, b"old").unwrap();
        let mut cmd = std::process::Command::new("sh");
        cmd.arg("-c").arg(r#"head -c 800000 /dev/zero > "$1.tmp" && sync "$1.tmp" && mv "$1.tmp" "$1" && echo saved > "/dev/fd/$POCKET_HANDOFF_FD"; sleep 0.3"#).arg("sh").arg(&state);
        let h = Handoff::attach(&mut cmd).expect("pipe");
        let mut child = cmd.spawn().unwrap();
        let (tx, rx) = mpsc::channel();
        let read = state.clone();
        h.watch(0, move || tx.send(std::fs::metadata(&read).map(|m| m.len()).unwrap_or(0)).unwrap());
        assert_eq!(rx.recv_timeout(Duration::from_secs(5)), Ok(800_000), "the whole new state, not the old one or a part");
        child.wait().unwrap();
    }

    /// Only the child gets the write end: a second process started meanwhile can't hold the pipe open.
    #[cfg(target_os = "linux")]
    #[test]
    fn the_write_end_reaches_only_its_child() {
        let mut cmd = std::process::Command::new("sh");
        cmd.args(["-c", "true"]);
        let h = Handoff::attach(&mut cmd).expect("pipe");
        let fd = std::os::fd::AsRawFd::as_raw_fd(&h.write);
        // SAFETY: F_GETFD on a descriptor we own.
        assert_eq!(unsafe { libc::fcntl(fd, libc::F_GETFD) } & libc::FD_CLOEXEC, libc::FD_CLOEXEC);
        cmd.spawn().unwrap().wait().unwrap();
        h.watch(0, || {});
    }
}
