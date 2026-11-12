//! `POCKETUI_FRAME_DUMP=<dir>`: pocket-ui's frames on a fixed 60 fps clock, for smooth videos of the real device (a
//! screen recorder on the Retroid Pocket Mini V2 keeps only 13-56 of the 60 pictures a second, 10.11.2026). Developer
//! only (docs/dev/environment.md): without the variable nothing here is read, started or allocated, and the clock is
//! the system's.
//!
//! Like pocket-core's trailer dump, time is counted in frames: the UI thread's clock (`Instant`, `clock_gettime`
//! CLOCK_MONOTONIC) stands at tick `k` * [`TICK_NS`] while that tick's input is handled and its frame painted, however long
//! painting and writing take ([`clock`]). The loop (`app_frame_dump` in the binary) waits for real time before each tick,
//! so with a writer that keeps up the dump also runs in real time and presses sent from outside land where they were
//! sent; when it falls behind, every frame is still there, only later.
//!
//! pocket-core, started by such a pocket-ui, inherits the variable and writes the games into `<dir>/core-<pid>/` the
//! same way ([`for_game`]): every picture it puts on screen, the frame around the game, the TV coming on and the veil
//! included, one tick each (a game's own clock is its frames, as in the trailer dump), and the game's sound of each
//! frame (`audio.s16`, stereo, the rate in `events.log`). The pause menu is pocket-ui's and in its own frames.
//!
//! In `<dir>`:
//! * `frames.pfd`: `PFD1`, then one record per painted frame: `tick: u64`, `t_ns: u64` (the UI clock at the paint,
//!   from tick 0), `real_ns: u64` (the real monotonic clock then, the one both processes share), `w: u32`, `h: u32`,
//!   `len: u32`, `xor: u8` (1 = the difference to the frame before, same size),
//!   `len` bytes of zlib (the pixels as `u32` 0x00RRGGBB, little endian, row by row). A tick without a record shows
//!   the frame before (nothing changed, so nothing was painted).
//! * `events.log`: one line per thing that happened, `<tick> input <Button> down|up`, `<tick> sound <Sound> <after ms>
//!   <dB>` (a UI sound as it would play, whatever the speaker's volume), `<tick> script <word>`, and at the end
//!   `start <ns>` first (the real monotonic clock at tick 0, as pocket-core's game parts in `core-<pid>/` have it) and
//!   `end <ticks> late <ms> real <ns> frames <n>` last (ticks 0 to `ticks` - 1; `late`: how far the last one came after
//!   its real time).
//!
//! `scripts/frame-dump-decode.py <dir>` turns a dump into a 60 fps video or PNGs.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender};
use std::sync::OnceLock;

pub const ENV: &str = "POCKETUI_FRAME_DUMP";
/// `<frame> <word>` lines (the command FIFO's button words), `end <frame>`: input on the dump's own clock.
pub const SCRIPT_ENV: &str = "POCKETUI_FRAME_DUMP_SCRIPT";
const MAGIC: &[u8; 4] = b"PFD1";
/// Frames handed to the writer before the UI thread waits for it (it never drops one).
const QUEUE: usize = 8;

/// One tick: the UI's own frame (`FRAME` in main.rs, 16.667 ms), so its pacing (`frame_pace`) has a frame due on every
/// tick. A tick of exactly 1/60 s came 0.3 µs before that and painted every other one (30 fps, the Mini V2, 10.11.2026).
/// 60 ticks are 1.00002 s: played at 60 fps, 1.2 ms a minute slower than real, which nobody sees.
pub const TICK_NS: u64 = 16_667_000;

/// The UI clock at tick `k`, from tick 0.
pub fn tick_ns(k: u64) -> u64 {
    k * TICK_NS
}

enum Msg {
    Frame { tick: u64, t_ns: u64, real_ns: u64, w: u32, h: u32, pixels: Vec<u32>, rgba_bottom_up: bool },
    Audio(Vec<i16>),
    Line(String),
    End(std::sync::mpsc::Sender<()>),
}

struct Dump {
    tx: SyncSender<Msg>,
    dir: PathBuf,
}

static DUMP: OnceLock<Option<Dump>> = OnceLock::new();
static TICK: AtomicU64 = AtomicU64::new(0);
/// The fixed clock runs (`start`); before that the frames are not written.
static RUNNING: AtomicBool = AtomicBool::new(false);
static START_NS: AtomicU64 = AtomicU64::new(0);
/// This process is pocket-core: its dump is `<dir>/core-<pid>`, on the real clock.
static GAME: AtomicBool = AtomicBool::new(false);

fn dump() -> Option<&'static Dump> {
    DUMP.get_or_init(|| {
        let mut dir = std::env::var_os(ENV).filter(|d| !d.is_empty()).map(PathBuf::from)?;
        let game = GAME.load(Ordering::Relaxed);
        if game {
            dir = dir.join(format!("core-{}", std::process::id()));
        } else if !clock::install() {
            eprintln!("frame-dump: the UI clock can't be fixed on this build, no dump");
            return None;
        }
        let writer = match Writer::create(&dir, game) {
            Ok(w) => w,
            Err(e) => {
                eprintln!("frame-dump: can't write {}: {e}", dir.display());
                return None;
            }
        };
        let (tx, rx) = sync_channel::<Msg>(QUEUE);
        std::thread::Builder::new().name("frame-dump".into()).spawn(move || writer.run(rx)).ok()?;
        Some(Dump { tx, dir })
    })
    .as_ref()
}

/// Whether this run dumps its frames (the variable is set and the dump could be made).
pub fn on() -> bool {
    dump().is_some()
}

/// The dump's folder, when on.
pub fn dir() -> Option<&'static Path> {
    dump().map(|d| d.dir.as_path())
}

/// The fixed clock starts on this (the UI) thread at tick 0. Called once by the loop, before the first tick; the real
/// monotonic time of tick 0 (`events.log`'s `start`, the clock pocket-core's game parts are placed by).
pub fn start() -> u64 {
    clock::start_here();
    let real0 = clock::real_ns();
    START_NS.store(clock::virtual_ns(), Ordering::Relaxed);
    RUNNING.store(true, Ordering::Relaxed);
    if let Some(d) = dump() {
        let _ = d.tx.send(Msg::Line(format!("start {real0}")));
    }
    real0
}

/// Tick `k` begins: the UI clock stands at `tick_ns(k)` from the start.
pub fn set_tick(k: u64) {
    TICK.store(k, Ordering::Relaxed);
    clock::set(START_NS.load(Ordering::Relaxed) + tick_ns(k));
}

pub fn tick() -> u64 {
    TICK.load(Ordering::Relaxed)
}

/// pocket-core: its dump, if pocket-ui's variable is set, goes into `<dir>/core-<pid>` and starts at once, a tick per
/// picture on screen ([`capture_screen`]). Call before anything else here. Whether the dump is on.
pub fn for_game() -> bool {
    GAME.store(true, Ordering::Relaxed);
    let Some(d) = dump() else { return false };
    RUNNING.store(true, Ordering::Relaxed);
    let _ = d.tx.send(Msg::Line(format!("start {}", clock::real_ns())));
    true
}

/// pocket-core: the picture just drawn (`rgba`, `w` x `h`, as GL reads it: rows bottom to top) is this tick's; the
/// next one begins. Turned and packed on the writer's thread.
pub fn capture_screen(rgba: Vec<u8>, w: u32, h: u32) {
    let Some(d) = dump() else { return };
    if rgba.len() < (w * h * 4) as usize {
        return;
    }
    let pixels = rgba.chunks_exact(4).map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect();
    let tick = TICK.fetch_add(1, Ordering::Relaxed);
    let _ = d.tx.send(Msg::Frame { tick, t_ns: tick_ns(tick), real_ns: clock::real_ns(), w, h, pixels, rgba_bottom_up: true });
}

/// pocket-core: the game's sound of this frame (interleaved stereo) at `rate` Hz.
pub fn audio(samples: &[i16], rate: f64) {
    let Some(d) = dump() else { return };
    static RATE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    if RATE.swap(rate.to_bits(), Ordering::Relaxed) != rate.to_bits() {
        let _ = d.tx.send(Msg::Line(format!("{} audio {rate} Hz s16le stereo", tick())));
    }
    let _ = d.tx.send(Msg::Audio(samples.to_vec()));
}

/// The real monotonic clock in ns (the UI thread's own is fixed while the dump runs).
pub fn real_ns() -> u64 {
    clock::real_ns()
}

/// One painted frame (`pixels`, `w` x `h`, row by row), written as part of the current tick. Without the variable a
/// no-op; with it, waits only when the writer is `QUEUE` frames behind.
pub fn capture(pixels: &[u32], w: u32, h: u32) {
    let Some(d) = dump() else { return };
    if !RUNNING.load(Ordering::Relaxed) || pixels.len() < (w as usize) * (h as usize) {
        return;
    }
    let t_ns = clock::virtual_ns().saturating_sub(START_NS.load(Ordering::Relaxed));
    let _ = d.tx.send(Msg::Frame { tick: tick(), t_ns, real_ns: clock::real_ns(), w, h, pixels: pixels[..(w * h) as usize].to_vec(), rgba_bottom_up: false });
}

/// A line for `events.log` at the current tick; `line` is only called when the dump runs.
pub fn event(line: impl FnOnce() -> String) {
    let Some(d) = dump() else { return };
    if RUNNING.load(Ordering::Relaxed) {
        let _ = d.tx.send(Msg::Line(format!("{} {}", tick(), line())));
    }
}

/// The last line (`ticks`: how many there were, 0 to `ticks` - 1), then waits until everything is in the files.
pub fn finish(ticks: u64, late_ms: u64) {
    let Some(d) = dump() else { return };
    let _ = d.tx.send(Msg::Line(format!("end {ticks} late {late_ms} real {}", clock::real_ns())));
    let (done_tx, done_rx) = std::sync::mpsc::channel();
    if d.tx.send(Msg::End(done_tx)).is_ok() {
        let _ = done_rx.recv();
    }
    RUNNING.store(false, Ordering::Relaxed);
}

/// The script: (tick, word) sorted, and the tick to end at.
pub fn script(text: &str) -> std::io::Result<(Vec<(u64, String)>, Option<u64>)> {
    let (mut lines, mut end) = (Vec::new(), None);
    let bad = |line: &str| std::io::Error::new(std::io::ErrorKind::InvalidData, format!("script: {line}"));
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')) {
        let mut w = line.split_whitespace();
        match (w.next(), w.next(), w.next()) {
            (Some("end"), Some(n), None) => end = Some(n.parse().map_err(|_| bad(line))?),
            (Some(n), Some(word), None) => lines.push((n.parse().map_err(|_| bad(line))?, word.to_string())),
            _ => return Err(bad(line)),
        }
    }
    lines.sort_by_key(|(t, _)| *t);
    Ok((lines, end))
}

struct Writer {
    out: std::io::BufWriter<std::fs::File>,
    log: std::io::BufWriter<std::fs::File>,
    audio: Option<std::io::BufWriter<std::fs::File>>,
    samples: u64,
    prev: Vec<u32>,
    prev_size: (u32, u32),
    frames: u64,
}

impl Writer {
    fn create(dir: &Path, game: bool) -> std::io::Result<Writer> {
        std::fs::create_dir_all(dir)?;
        let mut out = std::io::BufWriter::with_capacity(1 << 20, std::fs::File::create(dir.join("frames.pfd"))?);
        out.write_all(MAGIC)?;
        let log = std::io::BufWriter::new(std::fs::File::create(dir.join("events.log"))?);
        let audio = if game { Some(std::io::BufWriter::new(std::fs::File::create(dir.join("audio.s16"))?)) } else { None };
        Ok(Writer { out, log, audio, samples: 0, prev: Vec::new(), prev_size: (0, 0), frames: 0 })
    }

    fn run(mut self, rx: Receiver<Msg>) {
        for msg in rx {
            let r = match msg {
                Msg::Frame { tick, t_ns, real_ns, w, h, mut pixels, rgba_bottom_up } => {
                    if rgba_bottom_up {
                        // GL's RGBA (R first in memory) to 0x00RRGGBB, rows top to bottom.
                        for p in pixels.iter_mut() {
                            *p = (*p & 0xff) << 16 | (*p & 0xff00) | (*p >> 16 & 0xff);
                        }
                        let row = w as usize;
                        for y in 0..(h as usize / 2) {
                            let (a, b) = pixels.split_at_mut((h as usize - 1 - y) * row);
                            a[y * row..(y + 1) * row].swap_with_slice(&mut b[..row]);
                        }
                    }
                    self.frame(tick, t_ns, real_ns, w, h, pixels)
                }
                Msg::Audio(samples) => match self.audio.as_mut() {
                    Some(a) => {
                        self.samples += samples.len() as u64 / 2;
                        a.write_all(&samples.iter().flat_map(|v| v.to_le_bytes()).collect::<Vec<u8>>())
                    }
                    None => Ok(()),
                },
                Msg::Line(l) => {
                    let l = if l.starts_with("end ") { format!("{l} frames {}", self.frames) } else { l };
                    writeln!(self.log, "{l}")
                }
                Msg::End(done) => {
                    let r = self.out.flush().and_then(|_| self.log.flush()).and_then(|_| self.audio.as_mut().map_or(Ok(()), |a| a.flush()));
                    let _ = done.send(());
                    r
                }
            };
            if let Err(e) = r {
                eprintln!("frame-dump: {e}");
                return;
            }
        }
    }

    fn frame(&mut self, tick: u64, t_ns: u64, real_ns: u64, w: u32, h: u32, pixels: Vec<u32>) -> std::io::Result<()> {
        let xor = self.prev_size == (w, h);
        let mut bytes = Vec::with_capacity(pixels.len() * 4);
        for (i, p) in pixels.iter().enumerate() {
            bytes.extend_from_slice(&(if xor { p ^ self.prev[i] } else { *p }).to_le_bytes());
        }
        let z = miniz_oxide::deflate::compress_to_vec_zlib(&bytes, 1);
        for field in [&tick.to_le_bytes()[..], &t_ns.to_le_bytes(), &real_ns.to_le_bytes(), &w.to_le_bytes(), &h.to_le_bytes(), &(z.len() as u32).to_le_bytes(), &[xor as u8], &z] {
            self.out.write_all(field)?;
        }
        self.prev = pixels;
        self.prev_size = (w, h);
        self.frames += 1;
        // A dump read while pocket-ui runs, or after it was killed, has all but the last second.
        if self.frames % 60 == 0 {
            self.out.flush()?;
            self.log.flush()?;
            if let Some(a) = self.audio.as_mut() {
                a.flush()?;
            }
        }
        Ok(())
    }
}

/// One frame of a dump file, as [`read`] gives it back.
pub struct Frame {
    pub tick: u64,
    pub t_ns: u64,
    pub real_ns: u64,
    pub w: u32,
    pub h: u32,
    pub pixels: Vec<u32>,
}

/// The frames of `frames.pfd`, for the tests and tools that read a dump in Rust.
pub fn read(path: &Path) -> std::io::Result<Vec<Frame>> {
    let data = std::fs::read(path)?;
    let bad = || std::io::Error::new(std::io::ErrorKind::InvalidData, "not a frame dump");
    if data.get(..4) != Some(MAGIC) {
        return Err(bad());
    }
    let u32_at = |i: usize| u32::from_le_bytes(data[i..i + 4].try_into().unwrap());
    let u64_at = |i: usize| u64::from_le_bytes(data[i..i + 8].try_into().unwrap());
    let (mut at, mut out) = (4usize, Vec::<Frame>::new());
    while at + 37 <= data.len() {
        let (tick, t_ns, real_ns) = (u64_at(at), u64_at(at + 8), u64_at(at + 16));
        let (w, h, len, xor) = (u32_at(at + 24), u32_at(at + 28), u32_at(at + 32) as usize, data[at + 36] == 1);
        at += 37;
        if at + len > data.len() {
            break; // a dump that stopped mid-frame
        }
        let raw = miniz_oxide::inflate::decompress_to_vec_zlib(&data[at..at + len]).map_err(|_| bad())?;
        at += len;
        let mut pixels: Vec<u32> = raw.chunks_exact(4).map(|c| u32::from_le_bytes(c.try_into().unwrap())).collect();
        if xor {
            let prev = &out.last().ok_or_else(bad)?.pixels;
            pixels.iter_mut().zip(prev).for_each(|(p, q)| *p ^= q);
        }
        out.push(Frame { tick, t_ns, real_ns, w, h, pixels });
    }
    Ok(out)
}

/// The UI thread's fixed clock. `clock_gettime` of this executable (its GOT entry, so libraries and the kernel keep the
/// real clock) is pointed at [`hooked`] when a dump starts; on every thread but the one that called [`start_here`] it
/// answers the real time. Nothing is changed without the variable.
mod clock {
    use super::*;
    use std::cell::Cell;
    use std::sync::atomic::{AtomicI64, AtomicUsize};

    type ClockGettime = unsafe extern "C" fn(libc::clockid_t, *mut libc::timespec) -> libc::c_int;
    static REAL: AtomicUsize = AtomicUsize::new(0);
    /// The UI clock of the current tick, and the real time it began at (ns).
    static AT: AtomicI64 = AtomicI64::new(0);
    static AT_REAL: AtomicI64 = AtomicI64::new(0);
    /// Work on the UI thread that takes longer than this in one tick (a wait with a deadline on the clock) sees the
    /// clock move on in real time, so it can't wait for ever; a frame never takes that long.
    const GRACE_NS: i64 = 1_000_000_000;

    thread_local! {
        static FIXED: Cell<bool> = const { Cell::new(false) };
        static LAST: Cell<i64> = const { Cell::new(0) };
    }

    fn ns(ts: &libc::timespec) -> i64 {
        ts.tv_sec as i64 * 1_000_000_000 + ts.tv_nsec as i64
    }

    pub(super) fn real_ns() -> u64 {
        let mut ts = libc::timespec { tv_sec: 0, tv_nsec: 0 };
        let real = REAL.load(Ordering::Relaxed);
        // SAFETY: REAL is libc's clock_gettime (set once by `install`), which only writes the timespec it is given.
        unsafe {
            if real == 0 {
                libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut ts);
            } else {
                std::mem::transmute::<usize, ClockGettime>(real)(libc::CLOCK_MONOTONIC, &mut ts);
            }
        }
        ns(&ts) as u64
    }

    /// The clock as this thread sees it (fixed on the UI thread).
    pub(super) fn virtual_ns() -> u64 {
        let mut ts = libc::timespec { tv_sec: 0, tv_nsec: 0 };
        // SAFETY: as above; through the executable's own entry, so the UI thread gets the fixed clock.
        unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut ts) };
        ns(&ts) as u64
    }

    pub(super) fn start_here() {
        let now = real_ns() as i64;
        AT.store(now, Ordering::Relaxed);
        AT_REAL.store(now, Ordering::Relaxed);
        FIXED.with(|f| f.set(true));
    }

    pub(super) fn set(at_ns: u64) {
        AT_REAL.store(real_ns() as i64, Ordering::Relaxed);
        AT.store(at_ns as i64, Ordering::Relaxed);
    }

    unsafe extern "C" fn hooked(clock: libc::clockid_t, ts: *mut libc::timespec) -> libc::c_int {
        // SAFETY: REAL was set before this function was put in place.
        let r = unsafe { std::mem::transmute::<usize, ClockGettime>(REAL.load(Ordering::Relaxed))(clock, ts) };
        if r != 0 || clock != libc::CLOCK_MONOTONIC || !FIXED.with(|f| f.get()) {
            return r;
        }
        // SAFETY: the real call succeeded, so `ts` is a valid timespec.
        let real = ns(unsafe { &*ts });
        let over = (real - AT_REAL.load(Ordering::Relaxed) - GRACE_NS).max(0);
        let v = LAST.with(|l| {
            let v = (AT.load(Ordering::Relaxed) + over).max(l.get());
            l.set(v);
            v
        });
        // SAFETY: as above.
        unsafe {
            (*ts).tv_sec = (v / 1_000_000_000) as libc::time_t;
            (*ts).tv_nsec = (v % 1_000_000_000) as libc::c_long;
        }
        0
    }

    /// Points this executable's `clock_gettime` at [`hooked`]. Once; false where that can't be done.
    pub(super) fn install() -> bool {
        static DONE: OnceLock<bool> = OnceLock::new();
        *DONE.get_or_init(|| {
            // SAFETY: dlsym only reads the loaded objects' symbol tables.
            let real = unsafe { libc::dlsym(libc::RTLD_DEFAULT, c"clock_gettime".as_ptr()) };
            if real.is_null() {
                return false;
            }
            REAL.store(real as usize, Ordering::Relaxed);
            // SAFETY: walks the executable's own dynamic section and rewrites only its clock_gettime entries.
            unsafe { got::patch(b"clock_gettime", hooked as *const () as usize) > 0 }
        })
    }

    #[cfg(all(target_os = "linux", target_pointer_width = "64", target_env = "gnu"))]
    mod got {
        //! The executable's relocations (ELF64): the GOT entries bound to a symbol, rewritten in place.
        #[repr(C)]
        struct Dyn {
            tag: i64,
            val: u64,
        }
        #[repr(C)]
        struct Rela {
            offset: u64,
            info: u64,
            addend: i64,
        }
        #[repr(C)]
        struct Sym {
            name: u32,
            info: u8,
            other: u8,
            shndx: u16,
            value: u64,
            size: u64,
        }
        const DT_NULL: i64 = 0;
        const DT_PLTRELSZ: i64 = 2;
        const DT_STRTAB: i64 = 5;
        const DT_SYMTAB: i64 = 6;
        const DT_RELA: i64 = 7;
        const DT_RELASZ: i64 = 8;
        const DT_JMPREL: i64 = 23;
        #[cfg(target_arch = "x86_64")]
        const SLOTS: [u32; 2] = [6, 7]; // R_X86_64_GLOB_DAT, JUMP_SLOT
        #[cfg(target_arch = "aarch64")]
        const SLOTS: [u32; 2] = [1025, 1026]; // R_AARCH64_GLOB_DAT, JUMP_SLOT
        #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
        const SLOTS: [u32; 0] = [];

        /// The main program: the first object `dl_iterate_phdr` reports. (base, its dynamic section)
        unsafe fn main_object() -> Option<(u64, *const Dyn)> {
            unsafe extern "C" fn first(info: *mut libc::dl_phdr_info, _: libc::size_t, out: *mut libc::c_void) -> libc::c_int {
                // SAFETY: the loader hands a valid info whose phdrs are mapped.
                unsafe {
                    let info = &*info;
                    let out = &mut *(out as *mut Option<(u64, *const Dyn)>);
                    for i in 0..info.dlpi_phnum as usize {
                        let ph = &*info.dlpi_phdr.add(i);
                        if ph.p_type == libc::PT_DYNAMIC {
                            *out = Some((info.dlpi_addr, (info.dlpi_addr + ph.p_vaddr) as *const Dyn));
                        }
                    }
                }
                1 // the first object only
            }
            let mut out: Option<(u64, *const Dyn)> = None;
            // SAFETY: the callback only reads what the loader gives it.
            unsafe { libc::dl_iterate_phdr(Some(first), &mut out as *mut _ as *mut libc::c_void) };
            out
        }

        /// Rewrites every GOT entry of the executable bound to `name` to `to`; how many there were.
        pub(super) unsafe fn patch(name: &[u8], to: usize) -> usize {
            // SAFETY (whole function): the addresses come from the executable's own, mapped dynamic section; glibc
            // has relocated its pointers already on these targets (a value below the base is taken as an offset).
            unsafe {
                let Some((base, mut d)) = main_object() else { return 0 };
                let at = |v: u64| if v < base { v + base } else { v };
                let (mut strtab, mut symtab, mut rela, mut relasz, mut jmprel, mut pltrelsz) = (0, 0, 0, 0, 0, 0);
                while (*d).tag != DT_NULL {
                    let v = (*d).val;
                    match (*d).tag {
                        DT_STRTAB => strtab = at(v),
                        DT_SYMTAB => symtab = at(v),
                        DT_RELA => rela = at(v),
                        DT_RELASZ => relasz = v,
                        DT_JMPREL => jmprel = at(v),
                        DT_PLTRELSZ => pltrelsz = v,
                        _ => {}
                    }
                    d = d.add(1);
                }
                if strtab == 0 || symtab == 0 {
                    return 0;
                }
                let page = libc::sysconf(libc::_SC_PAGESIZE) as usize;
                let mut n = 0;
                for (table, size) in [(rela, relasz), (jmprel, pltrelsz)] {
                    if table == 0 {
                        continue;
                    }
                    let relas = std::slice::from_raw_parts(table as *const Rela, size as usize / std::mem::size_of::<Rela>());
                    for r in relas {
                        if !SLOTS.contains(&(r.info as u32)) {
                            continue;
                        }
                        let sym = &*((symtab as *const Sym).add((r.info >> 32) as usize));
                        let sym_name = std::ffi::CStr::from_ptr((strtab + sym.name as u64) as *const libc::c_char);
                        if sym_name.to_bytes() != name {
                            continue;
                        }
                        let slot = (base + r.offset) as *mut usize;
                        let start = slot as usize & !(page - 1);
                        // RELRO made the GOT read-only after the start: writable for this one write, then read-only again.
                        if libc::mprotect(start as *mut libc::c_void, page, libc::PROT_READ | libc::PROT_WRITE) != 0 {
                            continue;
                        }
                        slot.write_volatile(to);
                        libc::mprotect(start as *mut libc::c_void, page, libc::PROT_READ);
                        n += 1;
                    }
                }
                n
            }
        }
    }

    #[cfg(not(all(target_os = "linux", target_pointer_width = "64", target_env = "gnu")))]
    mod got {
        pub(super) unsafe fn patch(_: &[u8], _: usize) -> usize {
            0
        }
    }
}

/// Serialises the tests that fix the clock (one process, one executable).
#[cfg(test)]
static TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dump_gives_back_every_frame_exactly_with_its_tick() {
        let dir = std::env::temp_dir().join(format!("pocketui-frame-dump-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut w = Writer::create(&dir, false).unwrap();
        let frames: Vec<(u64, u32, u32, Vec<u32>)> = (0..5u32)
            .map(|k| (k as u64 * 2, 4, 3, (0..12).map(|i| i * 0x010203 + k * 7).collect()))
            .chain(std::iter::once((20, 2, 2, vec![0xffffff, 0, 1, 2])))
            .collect();
        for (tick, fw, fh, px) in &frames {
            w.frame(*tick, tick_ns(*tick), 7 + tick, *fw, *fh, px.clone()).unwrap();
        }
        w.out.flush().unwrap();
        let back = read(&dir.join("frames.pfd")).unwrap();
        assert_eq!(back.len(), frames.len());
        for (b, (tick, fw, fh, px)) in back.iter().zip(&frames) {
            assert_eq!((b.tick, b.t_ns, b.real_ns, b.w, b.h), (*tick, tick_ns(*tick), 7 + tick, *fw, *fh));
            assert_eq!(&b.pixels, px, "the picture of tick {tick}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_ui_threads_clock_stands_at_each_sixtieth_and_the_others_run_on() {
        let _l = TEST_LOCK.lock().unwrap();
        assert!(clock::install(), "this build can fix its clock");
        let t = std::thread::spawn(|| {
            clock::start_here();
            let a = std::time::Instant::now();
            clock::set(clock::real_ns() + tick_ns(1));
            let b = std::time::Instant::now();
            std::thread::sleep(std::time::Duration::from_millis(30));
            let c = std::time::Instant::now();
            (b - a, c - b)
        });
        let other = std::time::Instant::now();
        let (step, slept) = t.join().unwrap();
        assert!(step >= std::time::Duration::from_nanos(tick_ns(1)) && step < std::time::Duration::from_millis(18), "one tick: {step:?}");
        assert_eq!(slept, std::time::Duration::ZERO, "the fixed clock stands within a tick");
        assert!(other.elapsed() >= std::time::Duration::from_millis(30), "every other thread keeps the real clock");
        assert_eq!(tick_ns(60), 1_000_020_000, "60 ticks a second, as the UI paces its frames");
    }

    #[test]
    fn a_games_picture_is_turned_upright_and_its_sound_kept() {
        // pocket-core reads the screen as GL does (RGBA, bottom row first): the dump has it as pocket-ui's frames are.
        let dir = std::env::temp_dir().join(format!("pocketui-frame-dump-game-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let w = Writer::create(&dir, true).unwrap();
        let (tx, rx) = sync_channel(4);
        let t = std::thread::spawn(move || w.run(rx));
        // 1 x 2: bottom pixel red, top pixel blue, as GL reads them
        let rgba = [0xff, 0, 0, 0xff, 0, 0, 0xff, 0xff];
        let pixels = rgba.chunks_exact(4).map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect();
        tx.send(Msg::Frame { tick: 0, t_ns: 0, real_ns: 5, w: 1, h: 2, pixels, rgba_bottom_up: true }).unwrap();
        tx.send(Msg::Audio(vec![1, -1, 2, -2])).unwrap();
        let (done, wait) = std::sync::mpsc::channel();
        tx.send(Msg::End(done)).unwrap();
        wait.recv().unwrap();
        drop(tx);
        t.join().unwrap();
        let back = read(&dir.join("frames.pfd")).unwrap();
        assert_eq!(back[0].pixels, vec![0x0000ff, 0xff0000], "top row first, 0x00RRGGBB");
        assert_eq!(std::fs::read(dir.join("audio.s16")).unwrap(), vec![1, 0, 0xff, 0xff, 2, 0, 0xfe, 0xff]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_decoder_puts_a_games_sound_on_its_pictures_and_keeps_what_the_ui_logged_meanwhile() {
        // Retroid Mini V2 takes, 10.11.2026: pocket-core runs a game hidden behind pocket-ui's launch animation (the
        // hold) with its sound in audio.s16 but no picture, so scripts/frame-dump-decode.py put the game's sound
        // 0.6-1.5 s after its pictures; and the pause menu's sound on HOME, logged while the game was on screen, was
        // left out. A UI part (ticks 0-19), the game (its 40 pictures from the real moment of UI tick 20), the UI again.
        let dir = std::env::temp_dir().join(format!("pocketui-frame-dump-decode-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let start = 1_000_000_000u64;
        let mut ui = Writer::create(&dir, false).unwrap();
        for k in (0..20).chain(60..100) {
            ui.frame(k, tick_ns(k), start + tick_ns(k), 1, 1, vec![k as u32]).unwrap();
        }
        ui.out.flush().unwrap();
        drop(ui);
        std::fs::write(dir.join("events.log"), format!("start {start}\n5 input East down\n5 sound Confirm 0 0\n50 input Mode down\n50 sound Menu 0 0\n70 sound Back 0 0\nend 100 late 0 real {} frames 60\n", start + tick_ns(100))).unwrap();
        let core = dir.join("core-1");
        let mut g = Writer::create(&core, true).unwrap();
        for k in 0..40u64 {
            g.frame(k, tick_ns(k), start + tick_ns(20 + k), 1, 1, vec![0x10000 + k as u32]).unwrap();
        }
        g.out.flush().unwrap();
        drop(g);
        // 600 Hz: 10 stereo frames a tick. The hold's 300 frames (value 1) come before the first picture, then each
        // tick's own (its number + 100).
        let audio: Vec<i16> = std::iter::repeat(1).take(300).chain((0..40).flat_map(|k| std::iter::repeat(100 + k as i16).take(10))).flat_map(|v| [v, v]).collect();
        std::fs::write(core.join("audio.s16"), audio.iter().flat_map(|v| v.to_le_bytes()).collect::<Vec<u8>>()).unwrap();
        std::fs::write(core.join("events.log"), format!("start {}\n0 audio 600 Hz s16le stereo\nend 40 late 0 real {} frames 40\n", start + tick_ns(20), start + tick_ns(60))).unwrap();
        let base = dir.join("take.mp4");
        let out = std::process::Command::new("python3").arg("-I").arg(concat!(env!("CARGO_MANIFEST_DIR"), "/scripts/frame-dump-decode.py")).arg(&dir).arg("--sound").arg(&base).output().unwrap();
        assert!(out.status.success(), "decode: {}", String::from_utf8_lossy(&out.stderr));
        let events = std::fs::read_to_string(dir.join("take.mp4.events.txt")).unwrap();
        assert!(events.lines().any(|l| l == "0.0833 5 sound Confirm 0 0"), "the shelf's sound at its frame: {events}");
        assert!(events.lines().any(|l| l == "0.8333 50 input Mode down"), "HOME at the game's frame of that moment: {events}");
        assert!(events.lines().any(|l| l == "1.0000 60 sound Menu 0 0"), "its sound with the pause menu, the UI's first picture after the game: {events}");
        assert!(events.lines().any(|l| l == "1.1667 70 sound Back 0 0"), "after the game, the UI's frames again: {events}");
        assert!(events.lines().any(|l| l == "0.3333 20 game sound take.mp4.game-core-1-0.wav"), "{events}");
        let wav = std::fs::read(dir.join("take.mp4.game-core-1-0.wav")).unwrap();
        let samples: Vec<i16> = wav[44..].chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]])).collect();
        assert_eq!(samples.len(), 40 * 10 * 2, "a tick's share for each of the game's pictures");
        assert_eq!((samples[0], samples[20 * 20], samples[39 * 20 + 19]), (100, 120, 139), "each picture with its own sound, none of the hold's");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_script_is_words_by_frame_and_an_end() {
        let (lines, end) = script("# take 1\n30 right\n10 down\n\nend 90\n").unwrap();
        assert_eq!(lines, vec![(10, "down".to_string()), (30, "right".to_string())]);
        assert_eq!(end, Some(90));
        assert!(script("right 10").is_err());
    }
}
