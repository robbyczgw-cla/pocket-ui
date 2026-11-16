//! The core of the game in front, read into the page cache once the shelf's first frame is on screen
//! (`App::core_prewarm_start`, docs/ARCHITECTURE.md "Start").
//!
//! After a power-on the page cache is cold: the first game's start reads its libretro core from the card in the
//! same moment it sets up its picture. EGL and Mesa also read the card at that point (TORTOS-SPEED.md #1).
//! Reading the one core the next A most likely starts (Continue, else the last-played game) while the player looks
//! at the shelf moves that read out of the start. Only the file's pages are read: no `dlopen`, no `mlock`, so the
//! kernel can drop them again whenever it wants the memory.
//!
//! One core, plus the player and Mesa on H700, at most [`MAX_BYTES`] altogether; never FBNeo or ScummVM. On a thread of the
//! lowest CPU and I/O priority, in pieces, stopped by [`set_quiet`] (a game runs, the device sleeps). Never on
//! Android, where the cores are the APK's and the system's own business.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

/// The largest core, and the total H700 runtime read ahead: gpSP, mGBA, PCSX ReARMed and Genesis Plus GX are well below it, FBNeo far above.
pub const MAX_BYTES: u64 = 35 * 1024 * 1024;

/// One read: small enough that a game's start or sleep is noticed within a few milliseconds of card time.
const PIECE: usize = 256 * 1024;

/// Cores never read ahead, whatever their size on this card (TORTOS-SPEED.md #1, as decided).
const NEVER: &[&str] = &["fbneo", "fbalpha", "fbalpha2012", "scummvm"];

/// Set by the app while a game runs, starts, or the device sleeps: the warm-up doesn't begin, or stops.
static QUIET: AtomicBool = AtomicBool::new(false);

/// Whether a warm-up was started in this process (once: only after the first shelf frame).
static STARTED: AtomicBool = AtomicBool::new(false);

/// The app's state as the warm-up cares: `true` while a game runs or starts, or while the device is not awake.
pub fn set_quiet(quiet: bool) {
    QUIET.store(quiet, Ordering::Relaxed);
}

pub fn quiet() -> bool {
    QUIET.load(Ordering::Relaxed)
}

/// Whether a warm-up was started in this process (tests: never before the first frame).
pub fn started() -> bool {
    STARTED.load(Ordering::Relaxed)
}

/// What a launch of the game in front would load, as far as the warm-up cares.
#[derive(Clone, Debug, PartialEq)]
pub struct Front {
    /// "pocket-core" for our player; any other emulator loads its own files the warm-up doesn't know.
    pub emulator: String,
    pub core: String,
    /// The file `spawn_game` hands pocket-core (`store::core_path`).
    pub core_path: PathBuf,
}

/// The core file to read ahead for `front`, or None: no game in front, not our player, a core never read ahead, a
/// file that isn't there, or one larger than [`MAX_BYTES`]. `size`: the file's length (None: not a file).
pub fn choose(front: Option<&Front>, size: impl Fn(&Path) -> Option<u64>) -> Option<PathBuf> {
    let f = front?;
    if f.emulator != "pocket-core" || f.core.is_empty() || NEVER.contains(&f.core.as_str()) {
        return None;
    }
    let bytes = size(&f.core_path)?;
    (bytes > 0 && bytes <= MAX_BYTES).then(|| f.core_path.clone())
}

/// The length of a regular file (following a symlink), None otherwise.
pub fn file_size(path: &Path) -> Option<u64> {
    std::fs::metadata(path).ok().filter(|m| m.is_file()).map(|m| m.len())
}

/// What [`start`] did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The thread runs.
    Started,
    /// No shelf frame was on screen yet: nothing, ever, before it.
    NoFrame,
    /// A game starts or runs, or the device sleeps ([`set_quiet`]).
    Quiet,
    /// Started before in this process.
    Again,
    /// Android, the web demo, or `POCKETUI_NO_CORE_PREWARM` (the measurement without it).
    Off,
}

/// Starts the warm-up on a thread of its own, once per process and only when `shelf_frame` (the shelf's first frame
/// is on screen): `resolve` (run there, it may stat files and wait for the GBA moments' scan) names the game in
/// front's core; that file is then read in pieces. Nothing on Android, nothing while [`quiet`].
pub fn start(shelf_frame: bool, resolve: impl FnOnce() -> Option<Front> + Send + 'static) -> Outcome {
    let outcome = if cfg!(target_os = "android") || cfg!(target_os = "emscripten") || std::env::var_os("POCKETUI_NO_CORE_PREWARM").is_some() {
        Outcome::Off
    } else if !shelf_frame {
        Outcome::NoFrame
    } else if quiet() {
        Outcome::Quiet
    } else if STARTED.swap(true, Ordering::Relaxed) {
        Outcome::Again
    } else {
        Outcome::Started
    };
    if outcome != Outcome::Started {
        println!("core prewarm: not started ({outcome:?})");
        return outcome;
    }
    let _ = std::thread::Builder::new().name("core-prewarm".into()).spawn(move || {
        lower_priority();
        let front = resolve();
        let Some(path) = choose(front.as_ref(), file_size) else {
            println!("core prewarm: nothing to read ({})", why_not(front.as_ref()));
            return;
        };
        let paths = if crate::device::current().model.is_some_and(|m| m.is_h700()) {
            let host = std::env::var_os("POCKETUI_HOST_BIN").filter(|s| !s.is_empty())
                .map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/storage/pocket-ui/pocket-core"));
            runtime_paths(path, host, Path::new("/usr/lib"))
        } else { vec![path] };
        for path in bounded_paths(paths, file_size) {
            if quiet() { break; }
            let t0 = std::time::Instant::now();
            match read_ahead(&path, quiet) {
                Ok((bytes, true)) => println!("core prewarm: {} read ({} KB) in {:?}", path.display(), bytes / 1024, t0.elapsed()),
                Ok((bytes, false)) => {
                    println!("core prewarm: {} stopped after {} KB, a game or sleep came first", path.display(), bytes / 1024);
                    break;
                }
                Err(e) => println!("core prewarm: {}: {e}", path.display()),
            }
        }
    });
    outcome
}

/// The selected core first, then the H700 player's executable and Mesa runtime. Only existing files are read;
/// these are page-cache reads, never dlopen or a second GPU context. Mesa's versioned Gallium name is discovered
/// without a shell or ldd. Canonical paths avoid reading aliases twice (panfrost_dri is a libdril symlink).
fn runtime_paths(core: PathBuf, host: PathBuf, libs: &Path) -> Vec<PathBuf> {
    let mut paths = vec![core, host];
    for name in ["libEGL.so.1", "libEGL_mesa.so.0", "libGLESv2.so.2", "libGLdispatch.so.0",
        "libgbm.so.1", "libdrm.so.2", "libexpat.so.1", "libglapi.so.0", "dri/panfrost_dri.so"] {
        paths.push(libs.join(name));
    }
    let mut gallium: Vec<_> = std::fs::read_dir(libs).into_iter().flatten().flatten()
        .filter_map(|e| {
            let n = e.file_name();
            let n = n.to_str()?;
            (n.starts_with("libgallium-") && n.ends_with(".so")).then(|| e.path())
        }).collect();
    gallium.sort();
    paths.extend(gallium);
    let mut unique = std::collections::HashSet::new();
    paths.into_iter().filter_map(|p| std::fs::canonicalize(p).ok())
        .filter(|p| unique.insert(p.clone())).collect()
}

/// A common cap, including the executable and graphics libraries; missing/empty files and oversized drivers
/// are skipped. Once started, each file is still interruptible every 256 KB by read_ahead.
fn bounded_paths(paths: Vec<PathBuf>, size: impl Fn(&Path) -> Option<u64>) -> Vec<PathBuf> {
    let mut left = MAX_BYTES;
    paths.into_iter().filter(|p| match size(p) {
        Some(n) if n > 0 && n <= left => { left -= n; true }
        _ => false,
    }).collect()
}

fn why_not(front: Option<&Front>) -> String {
    match front {
        None => "no game in front".into(),
        Some(f) if f.emulator != "pocket-core" => format!("{} plays it", f.emulator),
        Some(f) if NEVER.contains(&f.core.as_str()) => format!("{} is never read ahead", f.core),
        Some(f) => match file_size(&f.core_path) {
            None => format!("{} is not there", f.core_path.display()),
            Some(n) => format!("{} is {} MB", f.core_path.display(), n / (1024 * 1024)),
        },
    }
}

/// Reads `path` from start to end into the page cache, piece by piece, asking `stop` before each piece. Returns the
/// bytes read and whether the whole file was.
pub fn read_ahead(path: &Path, stop: impl Fn() -> bool) -> std::io::Result<(u64, bool)> {
    use std::io::Read;
    let mut file = std::fs::File::open(path)?;
    let mut buf = vec![0u8; PIECE];
    let mut total = 0u64;
    loop {
        if stop() {
            return Ok((total, false));
        }
        let n = file.read(&mut buf)?;
        if n == 0 {
            return Ok((total, true));
        }
        total += n as u64;
    }
}

/// This thread only: the lowest CPU priority, and the idle I/O class (the card serves it when nothing else asks).
fn lower_priority() {
    #[cfg(target_os = "linux")]
    // SAFETY: scalar syscalls on this thread's own id; they only change its scheduling.
    unsafe {
        let tid = libc::syscall(libc::SYS_gettid) as libc::id_t;
        libc::setpriority(libc::PRIO_PROCESS, tid, 19);
        // IOPRIO_WHO_PROCESS = 1 (a thread id), IOPRIO_CLASS_IDLE = 3.
        libc::syscall(libc::SYS_ioprio_set, 1, tid, 3 << 13);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_starts_before_the_shelfs_first_frame_nor_while_a_game_runs() {
        let resolved = std::sync::Arc::new(AtomicBool::new(false));
        let r = resolved.clone();
        assert_eq!(start(false, move || { r.store(true, Ordering::SeqCst); None }), Outcome::NoFrame);
        assert!(!started(), "no warm-up was started before the first frame");
        set_quiet(true);
        let r = resolved.clone();
        assert_eq!(start(true, move || { r.store(true, Ordering::SeqCst); None }), Outcome::Quiet);
        set_quiet(false);
        assert!(!started());
        std::thread::sleep(std::time::Duration::from_millis(50));
        assert!(!resolved.load(Ordering::SeqCst), "the game in front isn't even looked for");
    }

    fn front(emulator: &str, core: &str) -> Front {
        Front { emulator: emulator.into(), core: core.into(), core_path: PathBuf::from(format!("/cores/{core}_libretro.so")) }
    }

    #[test]
    fn the_front_games_pocket_core_core_is_read_ahead() {
        let f = front("pocket-core", "gpsp");
        assert_eq!(choose(Some(&f), |_| Some(1_200_000)), Some(PathBuf::from("/cores/gpsp_libretro.so")));
    }

    #[test]
    fn no_game_in_front_reads_nothing() {
        assert_eq!(choose(None, |_| Some(1_000)), None);
    }

    #[test]
    fn a_core_over_35_mb_is_skipped_and_35_mb_is_not() {
        let f = front("pocket-core", "mupen64plus_next");
        assert_eq!(choose(Some(&f), |_| Some(MAX_BYTES + 1)), None);
        assert!(choose(Some(&f), |_| Some(MAX_BYTES)).is_some());
    }

    #[test]
    fn fbneo_and_scummvm_are_never_read_ahead_even_when_small() {
        for core in ["fbneo", "scummvm"] {
            assert_eq!(choose(Some(&front("pocket-core", core)), |_| Some(1_000)), None, "{core}");
        }
    }

    #[test]
    fn a_missing_or_empty_core_file_and_another_emulator_read_nothing() {
        let f = front("pocket-core", "gpsp");
        assert_eq!(choose(Some(&f), |_| None), None);
        assert_eq!(choose(Some(&f), |_| Some(0)), None);
        assert_eq!(choose(Some(&front("retroarch", "gpsp")), |_| Some(1_000)), None);
    }

    #[test]
    fn h700_runtime_warms_the_actual_player_and_driver_once_with_a_shared_budget() {
        let d = crate::testutil::TestDir::new("runtime-prewarm");
        let core = d.join("gpsp.so");
        let host = d.join("custom-pocket-core");
        for p in [&core, &host, &d.join("libEGL.so.1"), &d.join("libgallium-26.so")] {
            std::fs::write(p, [7u8; 16]).unwrap();
        }
        #[cfg(unix)] {
            std::fs::create_dir(d.join("dri")).unwrap();
            std::os::unix::fs::symlink("../libgallium-26.so", d.join("dri/panfrost_dri.so")).unwrap();
        }
        let paths = runtime_paths(core.clone(), host.clone(), &d.join(""));
        assert_eq!(&paths[..2], &[core.canonicalize().unwrap(), host.canonicalize().unwrap()]);
        assert_eq!(paths.len(), 4, "the driver alias is read only once; absent libraries are optional");
        let selected = bounded_paths(paths.clone(), |p| Some(if p == core { MAX_BYTES - 16 } else { 16 }));
        assert_eq!(selected, paths[..2], "a large core cannot pull a second runtime's worth of card reads in");
        assert!(bounded_paths(paths.clone(), |_| None).is_empty());
        assert!(bounded_paths(paths, |_| Some(0)).is_empty());
    }

    #[test]
    fn read_ahead_reads_the_whole_file_and_stops_when_asked() {
        let d = crate::testutil::TestDir::new("core-prewarm");
        let p = d.join("x_libretro.so");
        std::fs::write(&p, vec![7u8; PIECE * 2 + 5]).unwrap();
        assert_eq!(read_ahead(&p, || false).unwrap(), ((PIECE * 2 + 5) as u64, true));
        let asked = std::cell::Cell::new(0);
        let (bytes, whole) = read_ahead(&p, || { asked.set(asked.get() + 1); asked.get() > 1 }).unwrap();
        assert_eq!((bytes, whole), (PIECE as u64, false), "a game starting mid-read stops it at the next piece");
    }
}
