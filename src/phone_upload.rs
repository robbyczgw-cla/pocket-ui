//! "Send from your phone" (Settings → Games → Add games, Y; the same on the first start's games step): a small upload
//! server that exists only while that page is open. The page shows a QR code of `http://<Wi-Fi address>:<port>/<code>/`;
//! the phone opens it, picks files, and they stream straight onto the SD card (docs/user/phone-upload.md,
//! docs/PHONE-UPLOAD.md for the threat model and the security review it answers).
//!
//! It is not the web companion (`web`), which stays Tailscale-only and unchanged. What this one can do is kept small:
//!
//! - **When:** started by the page, stopped when the page closes, the moment the Pocket goes dark (light or deep
//!   sleep: `sleep` calls [`sleeping`]; the boot clock running ahead of the monotonic one is a second guard), after
//!   [`IDLE`] without a request that had the code, and after [`MAX_SESSION`] whatever happens. Nothing listens
//!   otherwise: no thread, no socket.
//! - **Where:** the Wi-Fi's own address only, tied to its interface (`SO_BINDTODEVICE`), on a port the kernel picks.
//!   Requests must name that address as their Host, and a browser's Origin, when it sends one, must be this page.
//! - **Who:** a fresh random code per opening, in the path. A request without it gets `403 Forbidden` and nothing else;
//!   [`MAX_BAD`] of them close the page (someone is guessing). Connections that haven't shown the code yet are capped
//!   at [`MAX_UNAUTH`] and never keep the page open.
//! - **What:** one page (HTML, its script and its style sheet), a check before each file and the file itself. No
//!   listing, nothing read back from the card, no other route.
//! - **Which files:** only the plain game files of the systems in [`ALLOWED`] (cartridges, disc images, arcade sets),
//!   each with its own size limit; nothing a launcher or an interpreter runs (DOOM's `.doom`, Daphne's `.singe`,
//!   ports, scripts, PICO-8), no saves, no BIOS files. Disc sheets (`.cue`, `.gdi`, `.m3u`) may name only files beside
//!   them: no folders, no absolute paths.
//! - **How files arrive:** nothing is made on the card before the code, the size, the free space (bytes and file
//!   entries) and the reservation are checked; then the folders, a temporary file in `roms/.pocket-ui-uploads`,
//!   streamed in 256 KB chunks (a disc sheet, at most 64 KB, is read first to check it), an atomic rename that never
//!   replaces an existing file. Anything that goes wrong removes the temporary file and the folders it made.
//!
//! Every request runs on its own thread (at most [`MAX_CONNS`]) at the lowest priority; the UI thread only reads the
//! [`Status`] when it paints.

use std::io::{BufReader, Read};
use std::net::{IpAddr, SocketAddr, TcpListener, TcpStream};
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, Instant};

use serde_json::json;

use crate::library::SysInfo;
use crate::web::files::{self, Refused, Roots};
use crate::web::http::{self, BadRequest, Response};

/// The page closes itself after this long without a request that had the code.
pub const IDLE: Duration = Duration::from_secs(10 * 60);
/// And after this long, whatever happens: open it again for more.
pub const MAX_SESSION: Duration = Duration::from_secs(60 * 60);
/// Requests at the same time (a few files in parallel, the page's own files).
pub const MAX_CONNS: usize = 8;
/// Of those, connections that haven't shown the code yet (a browser opens three at once for the page).
pub const MAX_UNAUTH: usize = 4;
/// Requests with a wrong code (or a broken head) before the page closes.
pub const MAX_BAD: usize = 30;
/// Files and folders one opening of the page may make (each file counts; so does each folder it made).
pub const MAX_NEW_ENTRIES: usize = 2000;
/// File entries left free on the card (where the file system counts them): saves and states must still be made.
pub const INODE_RESERVE: u64 = 20_000;
/// A disc sheet is read whole to check it; none is near this.
pub const MAX_SHEET: u64 = 64 << 10;
/// The request line and headers must be in by then.
const HEAD_DEADLINE: Duration = Duration::from_secs(10);
/// One read of a file may wait this long (a phone on weak Wi-Fi pauses).
const BODY_READ: Duration = Duration::from_secs(60);
/// How often the listener looks at the stop flag, the idle time and the clocks.
const TICK: Duration = Duration::from_millis(250);
/// Files shown on the Pocket's page (the newest).
const ROWS_KEPT: usize = 8;
/// Extensions of saves, refused even where a system lists them.
const SAVES: &[&str] = &["srm", "sav", "rtc", "state", "mcr", "eep", "fla", "sra"];

const CART: u64 = 64 << 20;
const DS: u64 = 512 << 20;
const ARCADE: u64 = 1 << 30;
const DISC: u64 = files::MAX_ROM;

/// What this page takes (security review M1): systems whose games are plain data an emulator reads, each with the
/// file types that are game files there and the largest one it takes (cartridges at most 64 MB, the most a core
/// that takes the game in memory gets unpacked; a DS card 512 MB; discs up to a dual-layer DVD, read by the
/// emulator from the card). A system is offered only where es_systems.cfg lists it, and only the types both lists
/// have. Not here on purpose: DOOM (`.doom` names files and folders for its launcher), Daphne (`.singe` is a Lua
/// script), ports, PICO-8 and other interpreters, DOS and computers, music and video players, ScummVM.
pub const ALLOWED: &[(&[&str], &[&str], u64)] = &[
    (&["gb", "gbh"], &["gb", "zip"], CART),
    (&["gbc", "gbch"], &["gbc", "gb", "zip"], CART),
    (&["gba", "gbah"], &["gba", "zip"], CART),
    (&["nes", "nesh", "famicom"], &["nes", "unf", "unif", "zip"], CART),
    (&["fds"], &["fds", "zip"], CART),
    (&["snes", "snesh", "sfc", "satellaview", "sufami"], &["sfc", "smc", "fig", "swc", "bs", "st", "zip"], CART),
    (&["megadrive", "megadrive-japan", "genesis", "megadriveh", "genh"], &["md", "gen", "smd", "bin", "zip"], CART),
    (&["sega32x"], &["32x", "bin", "zip"], CART),
    (&["mastersystem"], &["sms", "bin", "zip"], CART),
    (&["gamegear", "ggh"], &["gg", "zip"], CART),
    (&["sg-1000"], &["sg", "zip"], CART),
    (&["pcengine", "tg16", "supergrafx"], &["pce", "sgx", "zip"], CART),
    (&["ngp", "ngpc"], &["ngp", "ngc", "zip"], CART),
    (&["wonderswan", "wonderswancolor"], &["ws", "wsc", "zip"], CART),
    (&["atari2600"], &["a26", "bin", "zip"], CART),
    (&["atari7800"], &["a78", "bin", "zip"], CART),
    (&["atarilynx"], &["lnx", "zip"], CART),
    (&["virtualboy"], &["vb", "zip"], CART),
    (&["pokemini"], &["min", "zip"], CART),
    (&["colecovision"], &["col", "zip"], CART),
    (&["n64"], &["n64", "z64", "v64", "zip"], CART),
    (&["nds"], &["nds", "zip"], DS),
    (&["arcade", "mame", "fbn", "fbneo", "neogeo", "cps1", "cps2", "cps3"], &["zip", "7z"], ARCADE),
    (&["psx"], &["cue", "bin", "img", "wav", "chd", "iso", "pbp", "m3u"], DISC),
    (&["psp", "pspockets"], &["iso", "cso", "chd"], DISC),
    (&["ps2"], &["iso", "chd"], DISC),
    (&["gamecube", "gc"], &["iso", "rvz", "gcz", "ciso"], DISC),
    (&["wii"], &["iso", "rvz", "wbfs"], DISC),
    (&["dreamcast"], &["gdi", "cue", "cdi", "chd", "bin", "raw", "wav", "m3u"], DISC),
    (&["saturn"], &["cue", "bin", "wav", "chd", "iso", "m3u"], DISC),
    (&["segacd", "megacd"], &["cue", "bin", "wav", "chd", "iso", "m3u"], DISC),
    (&["pcenginecd", "tg16cd"], &["cue", "bin", "img", "iso", "wav", "chd", "m3u"], DISC),
    (&["neocd"], &["cue", "bin", "wav", "chd"], DISC),
    (&["3do"], &["iso", "chd", "cue", "bin", "wav"], DISC),
    (&["3ds"], &["3ds", "cci"], 4 << 30),
];

/// The system a common extension means where several take it (ROCKNIX lists `.sfc` for snes, sfc, satellaview and
/// the hacks; `.cdi` is nearly always a Dreamcast image). Without an entry here, a system whose id is the extension
/// wins (`.gb` → gb, not gbc or gbh); otherwise the phone asks.
const PREFER: &[(&str, &str)] = &[
    ("sfc", "snes"),
    ("smc", "snes"),
    ("md", "megadrive"),
    ("gen", "megadrive"),
    ("smd", "megadrive"),
    ("sms", "mastersystem"),
    ("gg", "gamegear"),
    ("z64", "n64"),
    ("v64", "n64"),
    ("pce", "pcengine"),
    ("ngc", "ngpc"),
    ("ws", "wonderswan"),
    ("wsc", "wonderswancolor"),
    ("a26", "atari2600"),
    ("lnx", "atarilynx"),
    ("vb", "virtualboy"),
    ("min", "pokemini"),
    ("gdi", "dreamcast"),
    ("cdi", "dreamcast"),
];

/// Disc files that keep the folder they came in (a `.cue` and its tracks, an `.m3u` and its discs).
const DISC_PARTS: &[&str] = &["cue", "bin", "gdi", "raw", "img", "wav", "m3u", "iso", "chd", "cdi"];
/// A disc's tracks: es_systems.cfg lists what starts a game, so a sheet but not the files it names. Without them the sheet
/// can't start and the library leaves it out; they never show up as games themselves. Only where the system has sheets.
const TRACKS: &[&str] = &["bin", "raw", "img", "iso", "wav"];
/// Disc sheets: text that names other files, checked before they are kept (security review M6).
const SHEETS: &[&str] = &["cue", "gdi", "m3u"];
/// Endings a core may react to anywhere in a path, not only at its end: PCSX-ReARMed treats any path containing
/// ".m3u" as a playlist and any containing ".exe" as a PS-EXE (`strcasestr`), and looks for a game's ".cue", ".toc",
/// ".ccd", ".mds" and ".sbi" beside it. A file or folder name may hold one only as the file's own last ending, where it
/// is a type this page takes and checks (review recheck M6).
const MARKERS: &[&str] = &["m3u", "cue", "gdi", "ccd", "toc", "mds", "sbi", "cbn", "pbp", "exe"];

/// The first marker ending (".m3u", ...) in `s`, anywhere, ignoring case.
fn marker_in(s: &str) -> Option<&'static str> {
    let lower = s.to_lowercase();
    MARKERS.iter().copied().find(|m| lower.contains(&format!(".{m}")))
}

const PAGE_HTML: &str = include_str!("../assets/phone-upload/index.html");
const PAGE_JS: &str = include_str!("../assets/phone-upload/app.js");
const PAGE_CSS: &str = include_str!("../assets/phone-upload/app.css");

/// Why the page closed by itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Closed {
    /// Ten minutes without the phone.
    Idle,
    /// The Pocket went dark (light or deep sleep).
    Slept,
    /// Someone kept trying addresses without the right code.
    Guessing,
    /// Open for [`MAX_SESSION`].
    TimeUp,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FileState {
    Receiving,
    Added,
    Failed(String),
}

/// One file as the Pocket's page lists it.
#[derive(Clone, Debug)]
pub struct FileRow {
    id: u64,
    pub name: String,
    pub done: u64,
    pub total: u64,
    pub state: FileState,
}

impl FileRow {
    /// A made-up row (the page's snapshots).
    pub fn demo(name: &str, done: u64, total: u64, state: FileState) -> FileRow {
        FileRow { id: u64::MAX, name: name.into(), done, total, state }
    }
}

/// What the Pocket's page shows.
#[derive(Clone, Debug, Default)]
pub struct Status {
    /// The phone opened the page.
    pub connected: bool,
    /// The newest last.
    pub files: Vec<FileRow>,
    /// Files put in place since the page opened.
    pub added: usize,
    pub closed: Option<Closed>,
}

impl Status {
    pub fn receiving(&self) -> usize {
        self.files.iter().filter(|f| f.state == FileState::Receiving).count()
    }
}

/// What the server needs; tests point `roots` at a temporary folder and make `idle` short.
pub struct Config {
    pub roots: Roots,
    /// Every system of es_systems.cfg (`Setup`'s catalogue).
    pub systems: Vec<SysInfo>,
    pub idle: Duration,
    /// Called (from a server thread) when the status changed, at most every 200 ms while bytes arrive.
    pub notify: Arc<dyn Fn() + Send + Sync>,
}

struct Inner {
    cfg: Config,
    // Per-server capacity fixture: no process-wide environment or shared disk measurements.
    #[cfg(test)]
    capacity: Mutex<Option<(Option<u64>, u64)>>,
    token: String,
    /// "address:port" as the browser must name it in Host.
    authority: String,
    stop: AtomicBool,
    status: Mutex<Status>,
    /// The connections under way, to cut them when the page closes (a read waiting for the phone ends at once).
    conns: Mutex<Vec<(u64, TcpStream)>>,
    /// The listening socket's descriptor while it is open (-1 after), to shut it at once when the Pocket goes dark.
    listen_fd: Mutex<i32>,
    seq: AtomicU64,
    active: AtomicUsize,
    /// Requests under way that had the code: only they keep the page open.
    active_auth: AtomicUsize,
    bad: AtomicUsize,
    /// Files and folders made so far ([`MAX_NEW_ENTRIES`]).
    made: AtomicUsize,
    /// The folders uploads under way write into, and how many of them use each: a cut upload removes a folder it made
    /// only when no other upload uses it (review N1).
    in_use: Mutex<std::collections::HashMap<PathBuf, usize>>,
    /// The last request that had the code.
    last: Mutex<Instant>,
    started: Instant,
    last_notify: Mutex<Instant>,
}

impl Inner {
    fn touch(&self) {
        *self.last.lock().unwrap_or_else(|e| e.into_inner()) = Instant::now();
    }

    fn changed(&self, force: bool) {
        let mut last = self.last_notify.lock().unwrap_or_else(|e| e.into_inner());
        if force || last.elapsed() >= Duration::from_millis(200) {
            *last = Instant::now();
            drop(last);
            (self.cfg.notify)();
        }
    }

    fn status(&self) -> std::sync::MutexGuard<'_, Status> {
        self.status.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn close(&self, why: Option<Closed>) {
        if !self.stop.swap(true, Ordering::SeqCst) {
            if let Some(why) = why {
                self.status().closed = Some(why);
            }
        }
        // Refuse new connections now, not at the listener's next tick. The lock is held across the shutdown; the
        // listener thread closes its socket only while holding it too (after setting -1), so the descriptor can't
        // have been closed and reused in between (review N2).
        let fd = self.listen_fd.lock().unwrap_or_else(|e| e.into_inner());
        if *fd >= 0 {
            // SAFETY: shutdown on the listening socket this server owns, still open while the lock is held.
            unsafe { libc::shutdown(*fd, libc::SHUT_RDWR) };
        }
        drop(fd);
        for (_, s) in self.conns.lock().unwrap_or_else(|e| e.into_inner()).iter() {
            let _ = s.shutdown(std::net::Shutdown::Both);
        }
    }
}

/// The servers open on the Wi-Fi, for [`sleeping`].
static OPEN: Mutex<Vec<Weak<Inner>>> = Mutex::new(Vec::new());
/// How often [`sleeping`] was called (tests: `sleep` calls it on the way into the dark).
static SLEEP_CALLS: AtomicUsize = AtomicUsize::new(0);

/// The Pocket is going dark (light sleep, screen off, deep sleep): every page open on the Wi-Fi closes now, its code is
/// gone, an upload under way stops and keeps nothing (security review M2). Called by `sleep` before it turns the
/// screen off or suspends; cheap (flags and `shutdown`), never waits.
pub fn sleeping() {
    SLEEP_CALLS.fetch_add(1, Ordering::SeqCst);
    let open: Vec<Arc<Inner>> = OPEN.lock().unwrap_or_else(|e| e.into_inner()).iter().filter_map(Weak::upgrade).collect();
    for inner in open {
        inner.close(Some(Closed::Slept));
        inner.changed(true);
    }
}

/// How often [`sleeping`] was called (for `sleep`'s test that it is).
pub fn sleeping_calls() -> usize {
    SLEEP_CALLS.load(Ordering::SeqCst)
}

/// The running server. Dropping it closes the socket, cuts every connection (an upload under way keeps nothing) and
/// waits for the listener's thread, at most a second.
pub struct Server {
    inner: Arc<Inner>,
    addr: SocketAddr,
    thread: Option<std::thread::JoinHandle<()>>,
    done: Mutex<std::sync::mpsc::Receiver<()>>,
}

impl Server {
    /// Opens the server on the Wi-Fi address `ip` (the one `wifi::ip` gives), tied to the interface that has it.
    pub fn on_wifi(ip: &str, cfg: Config) -> Result<Server, String> {
        let ip: IpAddr = ip.parse().map_err(|_| "No Wi-Fi address.".to_string())?;
        let iface = crate::web::net::interfaces()
            .into_iter()
            .find(|i| i.addr == ip && !i.name.starts_with("tailscale") && !i.addr.is_loopback())
            .ok_or_else(|| "The Wi-Fi address is gone.".to_string())?;
        let listen = crate::web::net::Listen { addr: SocketAddr::new(ip, 0), iface: iface.name, index: iface.index };
        let listener = crate::web::net::bind(&listen).map_err(|e| format!("Can't open the page: {e}"))?;
        let server = Server::start(listener, cfg).map_err(|e| format!("Can't open the page: {e}"))?;
        server.register();
        Ok(server)
    }

    /// Serves on `listener` (tests: 127.0.0.1:0). Not closed by [`sleeping`] unless [`Server::register`]ed.
    pub fn start(listener: TcpListener, cfg: Config) -> std::io::Result<Server> {
        let addr = listener.local_addr()?;
        listener.set_nonblocking(true)?;
        let inner = Arc::new(Inner {
            cfg,
            #[cfg(test)]
            capacity: Mutex::new(None),
            token: new_token()?,
            authority: addr.to_string(),
            stop: AtomicBool::new(false),
            status: Mutex::new(Status::default()),
            conns: Mutex::new(Vec::new()),
            listen_fd: Mutex::new(listener.as_raw_fd()),
            seq: AtomicU64::new(0),
            active: AtomicUsize::new(0),
            active_auth: AtomicUsize::new(0),
            bad: AtomicUsize::new(0),
            made: AtomicUsize::new(0),
            in_use: Mutex::new(std::collections::HashMap::new()),
            last: Mutex::new(Instant::now()),
            started: Instant::now(),
            last_notify: Mutex::new(Instant::now()),
        });
        let (tx, done) = std::sync::mpsc::channel();
        let i2 = inner.clone();
        let thread = std::thread::Builder::new().name("phone-upload".into()).spawn(move || {
            crate::web::lower_priority();
            listen(&i2, listener);
            let _ = tx.send(());
        })?;
        Ok(Server { inner, addr, thread: Some(thread), done: Mutex::new(done) })
    }

    /// Closed by [`sleeping`] from now on.
    pub fn register(&self) {
        let mut open = OPEN.lock().unwrap_or_else(|e| e.into_inner());
        open.retain(|w| w.strong_count() > 0);
        open.push(Arc::downgrade(&self.inner));
    }

    /// The address the QR code holds: `http://192.0.2.23:41234/k7m2qx9p/`.
    pub fn url(&self) -> String {
        format!("http://{}/{}/", self.addr, self.inner.token)
    }

    /// The same without `http://` and the last slash, to type.
    pub fn short(&self) -> String {
        format!("{}/{}", self.addr, self.inner.token)
    }

    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    pub fn status(&self) -> Status {
        self.inner.status().clone()
    }

    /// Closed by itself (idle, sleep, guessing, time): the page says why and offers to open again.
    pub fn closed(&self) -> Option<Closed> {
        self.inner.status().closed.clone()
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.inner.close(None);
        if self.done.lock().unwrap_or_else(|e| e.into_inner()).recv_timeout(Duration::from_secs(1)).is_ok() {
            if let Some(t) = self.thread.take() {
                let _ = t.join();
            }
        }
    }
}

/// 8 letters and digits from the kernel's random source, without the ones that look alike (0/o, 1/l/i): 40 bits,
/// with at most [`MAX_BAD`] guesses before the page closes.
fn new_token() -> std::io::Result<String> {
    const ALPHABET: &[u8] = b"23456789abcdefghjkmnpqrstuvwxyz";
    let mut bytes = [0u8; 8];
    std::fs::File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    // 256 is not a multiple of 31: a small bias, fine for a code that lives minutes and allows 30 guesses.
    Ok(bytes.iter().map(|b| ALPHABET[*b as usize % ALPHABET.len()] as char).collect())
}

/// Both strings equal, in time that doesn't depend on where they differ.
fn same(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// How far the boot clock is ahead of the monotonic one: it grows by the time the Pocket was suspended.
#[cfg(any(target_os = "linux", target_os = "android"))]
fn slept_total() -> Duration {
    let read = |clock| {
        let mut ts = libc::timespec { tv_sec: 0, tv_nsec: 0 };
        // SAFETY: clock_gettime fills the timespec we own.
        unsafe { libc::clock_gettime(clock, &mut ts) };
        Duration::new(ts.tv_sec.max(0) as u64, ts.tv_nsec.clamp(0, 999_999_999) as u32)
    };
    read(libc::CLOCK_BOOTTIME).saturating_sub(read(libc::CLOCK_MONOTONIC))
}

#[cfg(not(any(target_os = "linux", target_os = "android")))]
fn slept_total() -> Duration {
    Duration::ZERO
}

fn busy(stream: TcpStream) {
    let mut s = stream;
    let _ = s.set_write_timeout(Some(Duration::from_secs(2)));
    let _ = http::write_response(&mut s, &Response::new(503, "text/plain; charset=utf-8", b"Busy".to_vec()), false);
}

fn listen(inner: &Arc<Inner>, listener: TcpListener) {
    let slept_at_start = slept_total();
    while !inner.stop.load(Ordering::SeqCst) {
        let mut pfd = libc::pollfd { fd: listener.as_raw_fd(), events: libc::POLLIN, revents: 0 };
        // SAFETY: poll on one descriptor we own, with a bounded timeout.
        let ready = unsafe { libc::poll(&mut pfd, 1, TICK.as_millis() as libc::c_int) } > 0;
        // A second guard behind `sleeping`: a suspend the sleep code didn't announce.
        if slept_total().saturating_sub(slept_at_start) > Duration::from_secs(2) {
            inner.close(Some(Closed::Slept));
            break;
        }
        if inner.started.elapsed() >= MAX_SESSION {
            inner.close(Some(Closed::TimeUp));
            break;
        }
        // Only requests with the code keep it open (security review M3): a connection that never shows it doesn't.
        let idle = inner.last.lock().unwrap_or_else(|e| e.into_inner()).elapsed();
        if inner.active_auth.load(Ordering::SeqCst) == 0 && idle >= inner.cfg.idle {
            inner.close(Some(Closed::Idle));
            break;
        }
        if !ready {
            continue;
        }
        while let Ok((stream, _)) = listener.accept() {
            let _ = stream.set_nonblocking(false);
            let (active, auth) = (inner.active.load(Ordering::SeqCst), inner.active_auth.load(Ordering::SeqCst));
            if active >= MAX_CONNS || active.saturating_sub(auth) >= MAX_UNAUTH {
                busy(stream);
                continue;
            }
            inner.active.fetch_add(1, Ordering::SeqCst);
            let id = inner.seq.fetch_add(1, Ordering::Relaxed);
            if let Ok(clone) = stream.try_clone() {
                inner.conns.lock().unwrap_or_else(|e| e.into_inner()).push((id, clone));
            }
            let i2 = inner.clone();
            let spawned = std::thread::Builder::new().name("phone-upload-req".into()).spawn(move || {
                crate::web::lower_priority();
                handle(&i2, stream);
                i2.conns.lock().unwrap_or_else(|e| e.into_inner()).retain(|(c, _)| *c != id);
                i2.active.fetch_sub(1, Ordering::SeqCst);
            });
            if spawned.is_err() {
                inner.conns.lock().unwrap_or_else(|e| e.into_inner()).retain(|(c, _)| *c != id);
                inner.active.fetch_sub(1, Ordering::SeqCst);
            }
        }
    }
    // The socket closes here; whatever is still under way is cut.
    inner.close(None);
    {
        let mut fd = inner.listen_fd.lock().unwrap_or_else(|e| e.into_inner());
        *fd = -1;
        drop(listener);
    }
    inner.changed(true);
}

/// The socket as a request is read from it: the head by [`HEAD_DEADLINE`], then each read at most [`BODY_READ`].
struct Timed {
    stream: TcpStream,
    until: Option<Instant>,
}

impl Read for Timed {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let wait = match self.until {
            Some(until) => until.checked_duration_since(Instant::now()).filter(|d| !d.is_zero()).ok_or(std::io::ErrorKind::TimedOut)?,
            None => BODY_READ,
        };
        self.stream.set_read_timeout(Some(wait))?;
        self.stream.read(buf)
    }
}

fn plain(status: u16, body: &'static str) -> Response {
    Response::new(status, "text/plain; charset=utf-8", body.as_bytes().to_vec())
}

/// One more wrong code or broken head: [`MAX_BAD`] close the page.
fn bad_try(inner: &Inner) {
    if inner.bad.fetch_add(1, Ordering::SeqCst) + 1 >= MAX_BAD {
        inner.close(Some(Closed::Guessing));
        inner.changed(true);
    }
}

fn handle(inner: &Inner, stream: TcpStream) {
    let _ = stream.set_write_timeout(Some(Duration::from_secs(30)));
    let Ok(read_half) = stream.try_clone() else { return };
    let mut reader = BufReader::with_capacity(64 * 1024, Timed { stream: read_half, until: Some(Instant::now() + HEAD_DEADLINE) });
    let mut writer = stream;
    let (resp, head_only) = match http::read_head(&mut reader) {
        Ok(req) => {
            let resp = respond(inner, &req, &mut reader);
            (resp, req.method == "HEAD")
        }
        // A browser's spare connection closed unused, or one that never finished its head: no request, no answer.
        Err(BadRequest::Closed) => return,
        Err(e) => {
            bad_try(inner);
            (plain(e.status(), "Bad request"), false)
        }
    };
    if http::write_response(&mut writer, &resp, head_only).is_ok() {
        // A refused file may still be on its way: read a little of it so the phone sees the answer, not a reset.
        let _ = writer.shutdown(std::net::Shutdown::Write);
        reader.get_mut().until = Some(Instant::now() + Duration::from_secs(1));
        let _ = std::io::copy(&mut (&mut reader).take(1 << 20), &mut std::io::sink());
    }
}

/// What follows the code in the path ("" for the page itself), or None when the code is not there.
fn after_token<'a>(path: &'a str, token: &str) -> Option<&'a str> {
    let rest = path.strip_prefix('/')?;
    let (code, tail) = rest.split_once('/').map_or((rest, ""), |(c, t)| (c, t));
    same(code, token).then_some(tail)
}

/// The request names this page as its Host, and a browser's Origin and Fetch Metadata, when sent, say it comes from
/// this page or was typed (security review L1): a page of another site that learned the code can't post to it.
fn same_page(inner: &Inner, req: &http::Request) -> bool {
    if req.header("host").map(str::trim) != Some(inner.authority.as_str()) {
        return false;
    }
    if let Some(origin) = req.header("origin") {
        if origin.trim() != format!("http://{}", inner.authority) {
            return false;
        }
    }
    match req.header("sec-fetch-site").map(str::trim) {
        None | Some("same-origin") => true,
        // Opened from the camera or the address bar.
        Some("none") => req.method == "GET" || req.method == "HEAD",
        Some(_) => false,
    }
}

fn respond(inner: &Inner, req: &http::Request, body: &mut BufReader<Timed>) -> Response {
    let Some(rest) = after_token(&req.path, &inner.token) else {
        bad_try(inner);
        return plain(403, "Forbidden");
    };
    if inner.stop.load(Ordering::SeqCst) {
        return plain(503, "Closed");
    }
    if !same_page(inner, req) {
        return plain(403, "Forbidden");
    }
    // A request with the code: it keeps the page open while it runs, and from its end on.
    inner.active_auth.fetch_add(1, Ordering::SeqCst);
    inner.touch();
    let resp = match (req.method.as_str(), rest) {
        ("GET" | "HEAD", "") => {
            let first = !std::mem::replace(&mut inner.status().connected, true);
            if first {
                inner.changed(true);
            }
            page(inner)
        }
        ("GET" | "HEAD", "app.js") => Response::new(200, "text/javascript; charset=utf-8", PAGE_JS.as_bytes().to_vec()),
        ("GET" | "HEAD", "app.css") => Response::new(200, "text/css; charset=utf-8", PAGE_CSS.as_bytes().to_vec()),
        ("POST", "check") => check(inner, req),
        ("POST", "upload") => upload(inner, req, body),
        _ => plain(404, "Not found"),
    };
    inner.touch();
    inner.active_auth.fetch_sub(1, Ordering::SeqCst);
    resp
}

/// The file types `sys` takes over this page and the largest file: those both [`ALLOWED`] and es_systems.cfg list, and the
/// [`TRACKS`] of [`ALLOWED`] for a system with disc sheets.
pub fn allowed(sys: &SysInfo) -> Option<(Vec<String>, u64)> {
    let (_, exts, max) = ALLOWED.iter().find(|(ids, _, _)| ids.contains(&sys.id.as_str()))?;
    let mut both: Vec<String> = sys.extensions.iter().filter(|e| exts.contains(&e.as_str())).cloned().collect();
    if both.iter().any(|e| e == "cue" || e == "gdi") {
        both.extend(exts.iter().filter(|e| TRACKS.contains(e) && !both.iter().any(|b| b == *e)).map(|e| e.to_string()).collect::<Vec<_>>());
    }
    (!both.is_empty()).then_some((both, *max))
}

/// The page with the code in its paths and the systems in a JSON block (read by the script, never run).
fn page(inner: &Inner) -> Response {
    let systems: Vec<serde_json::Value> = inner
        .cfg
        .systems
        .iter()
        .filter(|s| files::system_folder_ok(&inner.cfg.roots, s))
        .filter_map(|s| allowed(s).map(|(exts, max)| json!({ "id": s.id, "name": s.name, "folder": s.folder, "ext": exts, "max": max })))
        .collect();
    // `<` escaped: a system's name can't end the block.
    let data = serde_json::to_string(&json!({ "systems": systems, "saves": SAVES, "disc": DISC_PARTS, "prefer": PREFER.iter().map(|(e, id)| (e.to_string(), id.to_string())).collect::<std::collections::BTreeMap<_, _>>() })).unwrap_or_default().replace('<', "\\u003c");
    let html = PAGE_HTML.replace("{{TOKEN}}", &inner.token).replace("{{DATA}}", &data);
    Response::new(200, "text/html; charset=utf-8", html.into_bytes())
}

/// BIOS files people often have beside their games that hostsys doesn't name (pocket-core's own list is `bios_for`).
fn bios_name(name: &str) -> bool {
    let lower = name.to_lowercase();
    (lower.starts_with("scph") && lower.ends_with(".bin"))
        || ["gba_bios.bin", "gb_bios.bin", "gbc_bios.bin", "sgb_bios.bin", "bios7.bin", "bios9.bin", "firmware.bin", "dc_boot.bin", "dc_flash.bin"].contains(&lower.as_str())
}

/// The one of `several` systems that the extension `ext` means ([`PREFER`], else the system called like it).
fn preferred<'a>(several: &[&'a SysInfo], ext: &str) -> Option<&'a SysInfo> {
    let want = PREFER.iter().find(|(e, _)| *e == ext).map_or(ext, |(_, id)| *id);
    several.iter().copied().find(|s| s.id == want)
}

/// The systems that take `name` over this page by its extension.
pub fn candidates<'a>(roots: &Roots, systems: &'a [SysInfo], name: &str) -> Vec<&'a SysInfo> {
    systems
        .iter()
        .filter(|s| files::system_folder_ok(roots, s) && allowed(s).is_some_and(|(exts, _)| files::matching_ext(name, &exts).is_some()))
        .collect()
}

/// Where a file will go, worked out without touching the card (security review M4): nothing is made until the code,
/// the size, the free space and the reservation are through ([`Plan::make`]).
#[derive(Debug)]
pub struct Plan {
    sys: SysInfo,
    /// The disc's own folder below the system's, when it keeps one.
    sub: Option<String>,
    pub name: String,
    ext: String,
    /// The largest file this system takes here.
    pub max: u64,
}

impl Plan {
    /// The file's path (folders not resolved: for the answer and the "already there?" check).
    pub fn path(&self) -> PathBuf {
        let dir = match &self.sub {
            Some(sub) => self.sys.path.join(sub),
            None => self.sys.path.clone(),
        };
        dir.join(&self.name)
    }

    pub fn system(&self) -> &str {
        &self.sys.name
    }

    fn sheet(&self) -> bool {
        SHEETS.contains(&self.ext.as_str())
    }

    /// The folders this file is written into (as planned, not resolved): the system's, and the disc's own.
    fn folders(&self) -> Vec<PathBuf> {
        let mut v = vec![self.sys.path.clone()];
        if let Some(sub) = &self.sub {
            v.push(self.sys.path.join(sub));
        }
        v
    }

    /// Makes the folders (the system's when missing, the disc's own) and returns the path written through and the
    /// claim on them; [`Claim::release`] gives them back (and removes the ones this upload made, if it failed and no
    /// other upload uses them). Made and claimed under one lock, so a folder another upload is about to use is never
    /// removed in between.
    fn make<'a>(&self, roots: &Roots, inner: &'a Inner) -> Result<(PathBuf, Claim<'a>), Refused> {
        let mut uses = inner.in_use.lock().unwrap_or_else(|e| e.into_inner());
        let folders = self.folders();
        for f in &folders {
            *uses.entry(f.clone()).or_insert(0) += 1;
        }
        let mut claim = Claim { inner, folders, made: Vec::new() };
        let result = (|| {
            let existed = self.sys.path.exists();
            let dir = files::system_folder(roots, &self.sys)?;
            if !existed {
                claim.made.push(self.sys.path.clone());
            }
            let Some(sub) = &self.sub else {
                return Ok(dir.join(&self.name));
            };
            let parent = dir.join(sub);
            match std::fs::symlink_metadata(&parent) {
                Ok(m) if m.file_type().is_symlink() || !m.is_dir() => return Err(Refused::NotAllowed(format!("{sub} is in the way."))),
                Ok(_) => {}
                Err(_) => match std::fs::create_dir(&parent) {
                    Ok(()) => claim.made.push(self.sys.path.join(sub)),
                    Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                    Err(e) => return Err(Refused::Io(format!("Can't make the folder {sub}: {e}"))),
                },
            }
            Ok(parent.join(&self.name))
        })();
        drop(uses);
        match result {
            Ok(t) => Ok((t, claim)),
            Err(e) => {
                claim.release(true);
                Err(e)
            }
        }
    }
}

/// An upload's hold on the folders it writes into ([`Plan::make`]).
struct Claim<'a> {
    inner: &'a Inner,
    folders: Vec<PathBuf>,
    /// The ones it made, oldest first.
    made: Vec<PathBuf>,
}

impl Claim<'_> {
    /// Gives the folders back. `failed`: the folders this upload made go too, newest first, where no other upload
    /// uses them and they are empty.
    fn release(self, failed: bool) {
        let mut uses = self.inner.in_use.lock().unwrap_or_else(|e| e.into_inner());
        for f in &self.folders {
            if let Some(n) = uses.get_mut(f) {
                *n -= 1;
                if *n == 0 {
                    uses.remove(f);
                }
            }
        }
        if failed {
            for d in self.made.iter().rev() {
                if !uses.contains_key(d) {
                    let _ = std::fs::remove_dir(d);
                }
            }
        }
    }
}

/// The file `raw` for `system` (or the one system its extension fits), in `dir` (one folder, for disc files). Saves,
/// BIOS files, file types this page doesn't take and a name that fits several systems without one chosen are refused.
/// Touches nothing.
pub fn plan(roots: &Roots, systems: &[SysInfo], raw: &str, system: Option<&str>, dir: Option<&str>) -> Result<Plan, Refused> {
    let name = files::sanitize_name(raw).ok_or_else(|| Refused::NotAllowed("That file name can't be used.".into()))?;
    let ext = name.rsplit_once('.').map(|(_, e)| e.to_lowercase()).unwrap_or_default();
    if SAVES.contains(&ext.as_str()) || ext.starts_with("state") {
        return Err(Refused::NotAllowed("Saves can't be sent this way.".into()));
    }
    if files::bios_for(&name).is_some() || bios_name(&name) {
        return Err(Refused::NotAllowed("That is a BIOS file. BIOS files go in roms/bios with the SD card or the web companion.".into()));
    }
    // A marker ending anywhere but as this file's own last ending: refused, whatever the type (review recheck M6).
    let before_ext = if MARKERS.contains(&ext.as_str()) { &name[..name.len() - ext.len() - 1] } else { name.as_str() };
    if let Some(m) = marker_in(before_ext) {
        return Err(Refused::NotAllowed(format!("A name with \".{m}\" inside can't be sent this way. Rename the file first.")));
    }
    let fits = candidates(roots, systems, &name);
    let sys = match system {
        Some(id) => *fits.iter().find(|s| s.id == id).ok_or_else(|| Refused::NotAllowed("That system doesn't take this kind of file here.".into()))?,
        None => match fits.as_slice() {
            [] => return Err(Refused::NotAllowed("No system takes this kind of file here.".into())),
            [one] => *one,
            several => preferred(several, &ext).ok_or_else(|| Refused::NotAllowed("Several systems take this kind of file: choose one.".into()))?,
        },
    };
    let (_, max) = allowed(sys).unwrap_or_default();
    // A system folder that is there must be a real one inside the ROMs folder (made only by `Plan::make`).
    if sys.path.exists() {
        files::system_folder(roots, sys)?;
    }
    // One folder below the system's, the way a disc game is kept; the library reads that level and no deeper.
    let sub = match dir.filter(|_| DISC_PARTS.contains(&ext.as_str())) {
        None => None,
        Some(dir) => {
            let seg = files::sanitize_name(dir).map(|s| s.trim_start_matches('_').to_string()).filter(|s| !s.is_empty()).ok_or_else(|| Refused::NotAllowed("That folder name can't be used.".into()))?;
            if let Some(m) = marker_in(&seg) {
                return Err(Refused::NotAllowed(format!("A folder with \".{m}\" in its name can't be used. Rename it first.")));
            }
            if std::fs::symlink_metadata(sys.path.join(&seg)).is_ok_and(|m| m.file_type().is_symlink() || !m.is_dir()) {
                return Err(Refused::NotAllowed(format!("{seg} is in the way.")));
            }
            Some(seg)
        }
    };
    Ok(Plan { sys: sys.clone(), sub, name, ext, max })
}

/// A file a disc sheet names may only be a plain name beside it: no folder, no absolute path, nothing hidden.
fn plain_name(n: &str) -> bool {
    !n.is_empty() && n.len() <= 255 && n != ".." && !n.starts_with('.') && !n.contains(['/', '\\', ':']) && !n.chars().any(char::is_control)
}

/// The file names a disc sheet refers to (`.cue`: FILE and CDTEXTFILE; `.gdi`: each track's file; `.m3u`: each line).
fn sheet_refs(ext: &str, text: &str) -> Vec<String> {
    // The field at `text`, quoted or up to the next blank.
    fn field(t: &str) -> (String, &str) {
        let t = t.trim_start();
        if let Some(q) = t.strip_prefix('"') {
            match q.split_once('"') {
                Some((v, rest)) => (v.to_string(), rest),
                None => (q.to_string(), ""),
            }
        } else {
            let end = t.find(char::is_whitespace).unwrap_or(t.len());
            (t[..end].to_string(), &t[end..])
        }
    }
    let lines = text.lines().map(|l| l.trim_matches(['\u{feff}', ' ', '\t', '\r'])).filter(|l| !l.is_empty());
    match ext {
        "cue" => lines
            .filter_map(|l| {
                let (cmd, rest) = l.split_once(char::is_whitespace)?;
                matches!(cmd.to_ascii_uppercase().as_str(), "FILE" | "CDTEXTFILE").then(|| field(rest).0)
            })
            .collect(),
        // Every field that isn't a number is a file name, also on the first line (a sheet may put a track there).
        "gdi" => lines
            .flat_map(|l| {
                let mut out = Vec::new();
                let mut rest = l;
                while !rest.trim().is_empty() {
                    let (v, r) = field(rest);
                    if !v.bytes().all(|b| b.is_ascii_digit() || b == b'-') || v.is_empty() {
                        out.push(v);
                    }
                    rest = r;
                }
                out
            })
            .collect(),
        "m3u" => lines.filter(|l| !l.starts_with('#')).map(str::to_string).collect(),
        _ => Vec::new(),
    }
}

/// Whether a disc sheet names only files beside it (security review M6). The emulator opens what the sheet names; a
/// sheet sent from a phone may not reach anything outside its own folder.
/// `dir`: the folder the sheet goes into; a name there that is a link (made some other way) counts as outside too.
pub fn sheet_ok(ext: &str, data: &[u8], dir: &Path) -> Result<(), Refused> {
    let text = String::from_utf8_lossy(data);
    let refs = sheet_refs(ext, &text);
    let bad = refs.iter().find(|r| !plain_name(r) || (if ext == "m3u" { ["m3u", "exe"].iter().any(|m| r.to_lowercase().contains(&format!(".{m}"))) } else { marker_in(r).is_some() }) || std::fs::symlink_metadata(dir.join(r)).is_ok_and(|m| m.file_type().is_symlink()));
    match bad {
        Some(bad) => Err(Refused::NotAllowed(format!("This disc sheet names \"{bad}\": it may name only files in its own folder."))),
        None => Ok(()),
    }
}

/// Free file entries on the file system of `dir`, where it counts them (ext4); None where it doesn't (FAT, exFAT).
fn free_entries(dir: &Path) -> Option<u64> {
    use std::os::unix::ffi::OsStrExt;
    let c = std::ffi::CString::new(dir.as_os_str().as_bytes()).ok()?;
    // SAFETY: statvfs fills the zeroed struct for a valid C string path.
    unsafe {
        let mut st: libc::statvfs = std::mem::zeroed();
        (libc::statvfs(c.as_ptr(), &mut st) == 0 && st.f_files > 0).then_some(st.f_favail as u64)
    }
}

/// Room for `len` bytes and the file entries it needs: the bytes as the companion checks them (with the reserve for
/// saves), file entries with [`INODE_RESERVE`] left, and this page's own count ([`MAX_NEW_ENTRIES`]).
fn room_for(inner: &Inner, plan: &Plan, len: u64) -> Result<(), Refused> {
    if len > plan.max {
        return Err(Refused::TooBig(format!("{} files are at most {} here.", plan.sys.name, crate::ports::size_label(plan.max))));
    }
    let probe = inner.cfg.roots.roms.join(".probe");
    #[cfg(test)]
    let capacity = *inner.capacity.lock().unwrap();
    #[cfg(test)]
    let fits = match capacity {
        Some((free, reserved)) => files::room(free, reserved, len, plan.max),
        None => files::fits(&probe, len, plan.max),
    };
    #[cfg(not(test))]
    let fits = files::fits(&probe, len, plan.max);
    fits?;
    if free_entries(&inner.cfg.roots.roms).is_some_and(|n| n < INODE_RESERVE) {
        return Err(Refused::NoSpace("The SD card has no room for more files.".into()));
    }
    if inner.made.load(Ordering::SeqCst) >= MAX_NEW_ENTRIES {
        return Err(Refused::NotAllowed("This page has taken enough files. Close it and open it again for more.".into()));
    }
    Ok(())
}

/// Where a file goes, from the query: `name`, `system` (when the phone chose one), `dir` (the disc's folder).
fn plan_of(inner: &Inner, req: &http::Request) -> Result<Plan, Refused> {
    plan(&inner.cfg.roots, &inner.cfg.systems, req.query("name").unwrap_or(""), req.query("system").filter(|s| !s.is_empty()), req.query("dir").filter(|s| !s.is_empty()))
}

/// "roms/gba/Game.gba" for the phone.
fn shown(roots: &Roots, path: &Path) -> String {
    let real = roots.roms.canonicalize().unwrap_or_else(|_| roots.roms.clone());
    let base = real.parent().map(PathBuf::from).unwrap_or_default();
    let unresolved = roots.roms.parent().map(PathBuf::from).unwrap_or_default();
    path.strip_prefix(&base)
        .or_else(|_| path.strip_prefix(&unresolved))
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| path.file_name().unwrap_or_default().to_string_lossy().into_owned())
}

/// A short code for the refusals this page can meet, so that the page can say them in the phone's language
/// (`assets/phone-upload/app.js`, `reasons`); the English sentence stays as the fallback and for the log.
fn refusal_code(e: &Refused) -> Option<&'static str> {
    let m = e.message();
    Some(match e {
        Refused::Short => "cut_off",
        Refused::Exists(_) => "exists",
        Refused::NoSpace(_) => "no_room",
        Refused::TooBig(_) if m.starts_with("That disc sheet") => "sheet_big",
        Refused::TooBig(_) => "too_big",
        _ if m.starts_with("That file name") => "bad_name",
        _ if m.starts_with("Saves can't") => "saves",
        _ if m.starts_with("That is a BIOS file") => "bios",
        _ if m.starts_with("A name with") => "marker",
        _ if m.starts_with("A folder with") => "folder_marker",
        _ if m.starts_with("That system doesn't take") => "wrong_system",
        _ if m.starts_with("No system takes") => "no_system",
        _ if m.starts_with("Several systems take") => "choose_system",
        _ if m.starts_with("That folder name") => "bad_folder",
        _ if m.ends_with(" is in the way.") => "in_the_way",
        _ if m.starts_with("This disc sheet names") => "sheet_names",
        _ if m.starts_with("This page has taken enough") => "enough",
        _ => return None,
    })
}

fn refused(e: Refused) -> Response {
    Response::json(e.status(), &json!({ "ok": false, "error": e.message(), "code": refusal_code(&e) }))
}

/// Before a file: where it would go, whether it fits, whether it is there. Makes nothing on the card.
fn check(inner: &Inner, req: &http::Request) -> Response {
    let size: u64 = req.query("size").and_then(|s| s.parse().ok()).unwrap_or(0);
    match plan_of(inner, req).and_then(|p| room_for(inner, &p, size).map(|()| p)) {
        Ok(p) => Response::json(200, &json!({ "ok": true, "path": shown(&inner.cfg.roots, &p.path()), "system": p.system(), "exists": std::fs::symlink_metadata(p.path()).is_ok() })),
        Err(e) => refused(e),
    }
}

/// Counts the bytes as they arrive, for the Pocket's page and the idle clock.
struct Counting<'a, R> {
    r: R,
    inner: &'a Inner,
    id: u64,
}

impl<R: Read> Read for Counting<'_, R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.r.read(buf)?;
        if n > 0 {
            self.inner.touch();
            if let Some(f) = self.inner.status().files.iter_mut().find(|f| f.id == self.id) {
                f.done += n as u64;
            }
            self.inner.changed(false);
        }
        Ok(n)
    }
}

fn upload(inner: &Inner, req: &http::Request, body: &mut BufReader<Timed>) -> Response {
    let Some(len) = req.content_length else {
        return refused(Refused::Io("The phone did not say how large the file is.".into()));
    };
    // Every check before anything is made on the card (security review M4).
    let p = match plan_of(inner, req).and_then(|p| room_for(inner, &p, len).map(|()| p)) {
        Ok(p) => p,
        Err(e) => return refused(e),
    };
    // Never over an existing file: whoever has the code could otherwise replace a game with anything.
    if std::fs::symlink_metadata(p.path()).is_ok() {
        return refused(Refused::Exists(format!("{} is there already.", p.name)));
    }
    if p.sheet() && len > MAX_SHEET {
        return refused(Refused::TooBig("That disc sheet is too large.".into()));
    }
    // Checked and held before a byte is read: files at the same time can't all take the same free space.
    let _room = match files::reserve(&inner.cfg.roots.roms.join(".probe"), len, p.max) {
        Ok(r) => r,
        Err(e) => return refused(e),
    };
    body.get_mut().until = None;
    // A disc sheet is small: read whole and checked before it is kept (security review M6).
    let sheet = if p.sheet() {
        let mut data = Vec::new();
        if (&mut *body).take(len).read_to_end(&mut data).is_err() || data.len() as u64 != len {
            return refused(Refused::Short);
        }
        if let Err(e) = sheet_ok(&p.ext, &data, p.path().parent().unwrap_or(&inner.cfg.roots.roms)) {
            return refused(e);
        }
        Some(data)
    } else {
        None
    };
    let (t, claim) = match p.make(&inner.cfg.roots, inner) {
        Ok(t) => t,
        Err(e) => return refused(e),
    };
    inner.made.fetch_add(1 + claim.made.len(), Ordering::SeqCst);
    let id = inner.seq.fetch_add(1, Ordering::Relaxed) | (1 << 63);
    {
        let mut st = inner.status();
        st.files.push(FileRow { id, name: p.name.clone(), done: 0, total: len, state: FileState::Receiving });
        let over = st.files.len().saturating_sub(ROWS_KEPT);
        // The oldest finished rows go first; one still arriving stays.
        for _ in 0..over {
            if let Some(i) = st.files.iter().position(|f| f.state != FileState::Receiving) {
                st.files.remove(i);
            }
        }
    }
    inner.changed(true);
    let (never, stop) = (|| false, || inner.stop.load(Ordering::SeqCst));
    let gate = files::Gate { wait: &never, abort: &stop, no_programs: true };
    let uploads = &inner.cfg.roots.uploads;
    let result = match &sheet {
        Some(data) => files::receive(&mut Counting { r: &data[..], inner, id }, &t, uploads, len, false, &gate),
        None => files::receive(&mut Counting { r: body, inner, id }, &t, uploads, len, false, &gate),
    };
    claim.release(result.is_err());
    {
        let mut st = inner.status();
        if let Some(f) = st.files.iter_mut().find(|f| f.id == id) {
            f.state = match &result {
                Ok(_) => FileState::Added,
                Err(e) => FileState::Failed(e.message()),
            };
        }
        if result.is_ok() {
            st.added += 1;
        }
    }
    inner.changed(true);
    match result {
        Ok(w) => {
            println!("phone upload: {} ({} bytes)", shown(&inner.cfg.roots, &t), w.bytes);
            Response::json(200, &json!({ "ok": true, "path": shown(&inner.cfg.roots, &t) }))
        }
        Err(e) => {
            println!("phone upload: refused ({})", e.status());
            refused(e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn sys(roms: &std::path::Path, id: &str, name: &str, exts: &[&str]) -> SysInfo {
        SysInfo { id: id.into(), name: name.into(), folder: format!("roms/{id}"), path: roms.join(id), extensions: exts.iter().map(|e| e.to_string()).collect() }
    }

    struct Env {
        _dir: tempfile::TempDir,
        roms: PathBuf,
        server: Option<Server>,
    }

    impl Env {
        fn new(idle: Duration) -> Env {
            let dir = tempfile::tempdir().unwrap();
            let roms = dir.path().join("roms");
            std::fs::create_dir_all(&roms).unwrap();
            let systems = vec![
                sys(&roms, "gba", "Game Boy Advance", &["gba", "zip"]),
                sys(&roms, "gb", "Game Boy", &["gb", "zip"]),
                sys(&roms, "gbh", "Game Boy Hacks", &["gb", "zip"]),
                sys(&roms, "snes", "Super Nintendo", &["sfc", "smc", "zip"]),
                sys(&roms, "sfc", "Super Famicom", &["sfc", "smc", "zip"]),
                sys(&roms, "psx", "PlayStation", &["cue", "bin", "chd", "m3u"]),
                sys(&roms, "segacd", "Sega CD", &["cue", "chd"]),
                sys(&roms, "psp", "PSP", &["iso", "cso"]),
                sys(&roms, "ps2", "PlayStation 2", &["iso", "chd"]),
                sys(&roms, "dreamcast", "Dreamcast", &["gdi", "cdi", "chd"]),
                sys(&roms, "ports", "Ports", &["sh"]),
                sys(&roms, "doom", "DOOM", &["doom", "wad"]),
                sys(&roms, "daphne", "Daphne", &["daphne", "singe", "zip"]),
                sys(&roms, "pico-8", "PICO-8", &["p8", "png"]),
            ];
            let cfg = Config { roots: Roots::under(&roms), systems, idle, notify: Arc::new(|| {}) };
            let server = Server::start(TcpListener::bind("127.0.0.1:0").unwrap(), cfg).unwrap();
            Env { _dir: dir, roms, server: Some(server) }
        }

        fn server(&self) -> &Server {
            self.server.as_ref().unwrap()
        }

        fn token(&self) -> String {
            self.server().inner.token.clone()
        }

        /// "127.0.0.1:port": what a browser names as Host.
        fn host(&self) -> String {
            self.server().addr().to_string()
        }

        /// One request (a Host of this page added where the head has none or says "x"); the status and body.
        fn send(&self, head: &str, body: &[u8]) -> (u16, String) {
            let head = if head.contains("Host: x\r\n") {
                head.replace("Host: x\r\n", &format!("Host: {}\r\n", self.host()))
            } else if !head.contains("Host:") {
                head.replacen("\r\n", &format!("\r\nHost: {}\r\n", self.host()), 1)
            } else {
                head.to_string()
            };
            self.send_raw(&head, body)
        }

        /// One request exactly as given.
        fn send_raw(&self, head: &str, body: &[u8]) -> (u16, String) {
            let mut s = TcpStream::connect(self.server().addr()).unwrap();
            s.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
            s.write_all(head.as_bytes()).unwrap();
            let _ = s.write_all(body);
            let mut out = Vec::new();
            let _ = s.read_to_end(&mut out);
            let text = String::from_utf8_lossy(&out).into_owned();
            let status = text.split(' ').nth(1).and_then(|c| c.parse().ok()).unwrap_or(0);
            let body = text.split_once("\r\n\r\n").map(|(_, b)| b.to_string()).unwrap_or_default();
            (status, body)
        }

        fn post(&self, path_query: &str, body: &[u8]) -> (u16, String) {
            self.send(&format!("POST {path_query} HTTP/1.1\r\nHost: x\r\nContent-Length: {}\r\n\r\n", body.len()), body)
        }

        fn up(&self, query: &str, body: &[u8]) -> (u16, String) {
            self.post(&format!("/{}/upload?{query}", self.token()), body)
        }

        fn leftovers(&self) -> Vec<String> {
            let mut out = Vec::new();
            for d in walk(&self.roms) {
                let n = d.file_name().unwrap().to_string_lossy().into_owned();
                if n.ends_with(".upload") || n.ends_with(".part") {
                    out.push(n);
                }
            }
            out
        }
    }

    fn walk(dir: &std::path::Path) -> Vec<PathBuf> {
        let mut out = Vec::new();
        for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_dir() {
                out.extend(walk(&p));
            }
            out.push(p);
        }
        out
    }

    #[test]
    fn without_the_code_everything_is_forbidden_with_no_details() {
        let env = Env::new(IDLE);
        for path in ["/", "/index.html", "/app.js", "/upload", "/xxxxxxxx/", "/xxxxxxxx/upload?name=a.gba"] {
            let (status, body) = env.send(&format!("GET {path} HTTP/1.1\r\nHost: x\r\n\r\n"), b"");
            assert_eq!((status, body.as_str()), (403, "Forbidden"), "{path}");
        }
        let (status, body) = env.post("/nottoken/upload?name=a.gba", b"abc");
        assert_eq!((status, body.as_str()), (403, "Forbidden"));
        assert!(!env.roms.join("gba").exists(), "nothing was written");
        // A code that differs only in its last letter is no code either.
        let mut wrong = env.token();
        wrong.pop();
        wrong.push(if env.token().ends_with('2') { '3' } else { '2' });
        assert_eq!(env.send(&format!("GET /{wrong}/ HTTP/1.1\r\n\r\n"), b"").0, 403);
    }

    #[test]
    fn the_code_opens_only_the_page_its_files_check_and_upload() {
        let env = Env::new(IDLE);
        let t = env.token();
        let (status, body) = env.send(&format!("GET /{t}/ HTTP/1.1\r\nHost: x\r\n\r\n"), b"");
        assert_eq!(status, 200);
        assert!(body.contains(&format!("/{t}/app.js")) && body.contains("Game Boy Advance"));
        assert!(!body.contains("\"Ports\""), "a folder of programs is not offered");
        assert!(env.server().status().connected, "the Pocket says the phone is there");
        assert_eq!(env.send(&format!("GET /{t}/app.js HTTP/1.1\r\n\r\n"), b"").0, 200);
        assert_eq!(env.send(&format!("GET /{t}/app.css HTTP/1.1\r\n\r\n"), b"").0, 200);
        for p in ["../etc/passwd", "list", "files", "roms/gba", "api/status"] {
            assert_eq!(env.send(&format!("GET /{t}/{p} HTTP/1.1\r\n\r\n"), b"").0, 404, "{p}");
        }
        assert_eq!(env.send(&format!("GET /{t}/upload HTTP/1.1\r\n\r\n"), b"").0, 404, "uploads are POSTs");
    }

    #[test]
    fn too_many_wrong_codes_close_the_page() {
        let env = Env::new(IDLE);
        for _ in 0..MAX_BAD {
            env.send("GET /guess/ HTTP/1.1\r\n\r\n", b"");
        }
        assert_eq!(env.server().closed(), Some(Closed::Guessing));
        std::thread::sleep(TICK * 3);
        assert!(TcpStream::connect(env.server().addr()).is_err(), "the socket is closed");
    }

    #[test]
    fn a_file_is_routed_by_its_extension_and_renamed_into_place() {
        let env = Env::new(IDLE);
        let (status, body) = env.up("name=Tobu%20Tobu%20Girl.gba", b"GBA ROM DATA");
        assert_eq!(status, 200, "{body}");
        assert_eq!(std::fs::read(env.roms.join("gba/Tobu Tobu Girl.gba")).unwrap(), b"GBA ROM DATA");
        assert!(body.contains("roms/gba/Tobu Tobu Girl.gba"));
        assert!(env.leftovers().is_empty());
        let st = env.server().status();
        assert_eq!((st.added, st.files.len(), st.files[0].state.clone(), st.files[0].done), (1, 1, FileState::Added, 12));
    }

    #[test]
    fn a_name_several_systems_take_needs_a_chosen_system() {
        let env = Env::new(IDLE);
        let (status, body) = env.up("name=Game.zip", b"zip");
        assert_eq!(status, 403, "{body}");
        assert!(body.contains("choose one"));
        assert_eq!(env.up("name=Game.zip&system=gb", b"zip").0, 200);
        assert!(env.roms.join("gb/Game.zip").is_file());
        // A system that doesn't take that kind of file, or none at all.
        assert_eq!(env.up("name=Game.gba&system=psx", b"x").0, 403);
        assert_eq!(env.up("name=readme.txt", b"x").0, 403);
        assert_eq!(env.up("name=start.sh&system=ports", b"x").0, 403, "never a folder of programs");
    }

    #[test]
    fn a_cart_extension_goes_where_it_usually_means_and_disc_files_ask() {
        // ROCKNIX lists .gb for gb, gbc, gbch and gbh: a Game Boy game goes to gb, an .sfc to snes, not the Super
        // Famicom or the hacks; the phone can still choose.
        let env = Env::new(IDLE);
        assert_eq!(env.up("name=Tobu.gb", b"x").0, 200);
        assert!(env.roms.join("gb/Tobu.gb").is_file());
        assert_eq!(env.up("name=Game.sfc", b"x").0, 200);
        assert!(env.roms.join("snes/Game.sfc").is_file());
        assert_eq!(env.up("name=Jp.sfc&system=sfc", b"x").0, 200);
        assert!(env.roms.join("sfc/Jp.sfc").is_file());
        assert_eq!(env.up("name=Hack.gb&system=gbh", b"x").0, 200);
        assert!(env.roms.join("gbh/Hack.gb").is_file());
        // Disc files never pick a folder alone: PSP, PS2, GameCube, Wii and Saturn games are ISOs too.
        for name in ["Game.iso", "Game.chd", "Game.cue"] {
            let (status, body) = env.up(&format!("name={name}"), b"x");
            assert_eq!((status, body.contains("choose one")), (403, true), "{name}: {body}");
        }
        assert_eq!(env.up("name=Game.iso&system=psp", b"x").0, 200);
        assert!(env.roms.join("psp/Game.iso").is_file());
        assert_eq!(env.up("name=Game2.iso&system=ps2", b"x").0, 200);
        assert!(env.roms.join("ps2/Game2.iso").is_file());
    }

    #[test]
    fn disc_files_keep_their_folder_and_other_files_do_not() {
        let env = Env::new(IDLE);
        assert_eq!(env.up("name=Game.cue&system=psx&dir=Game%20(Europe)", b"FILE \"Game.bin\" BINARY").0, 200);
        assert_eq!(env.up("name=Game.bin&system=psx&dir=Game%20(Europe)", b"bin").0, 200);
        assert!(env.roms.join("psx/Game (Europe)/Game.cue").is_file() && env.roms.join("psx/Game (Europe)/Game.bin").is_file());
        assert_eq!(env.up("name=Loose.gba&dir=Some%20Folder", b"x").0, 200);
        assert!(env.roms.join("gba/Loose.gba").is_file(), "a cartridge goes straight into the system's folder");
        // The folder is one cleaned name: no way up, no hidden or skipped folder.
        assert_eq!(env.up("name=A.cue&system=psx&dir=..", b"x").0, 403);
        assert_eq!(env.up("name=B.cue&system=psx&dir=a%2F..%2F..%2Fetc", b"x").0, 200);
        assert!(env.roms.join("psx/etc/B.cue").is_file(), "only the last part of a path is a folder name");
        assert_eq!(env.up("name=C.cue&system=psx&dir=.hidden", b"x").0, 200);
        assert!(env.roms.join("psx/hidden/C.cue").is_file());
        assert_eq!(env.up("name=D.cue&system=psx&dir=_skip", b"x").0, 200);
        assert!(env.roms.join("psx/skip/D.cue").is_file());
    }

    /// ROCKNIX lists what starts a game: `.gdi` and `.cue`, not the tracks they name. A sheet without them is no game.
    #[test]
    fn a_discs_tracks_go_with_it_though_es_systems_does_not_list_them() {
        let env = Env::new(IDLE);
        assert_eq!(env.up("name=Game.gdi&dir=Game", b"1\n1 0 4 2352 track01.bin 0\n2 600 0 2352 track02.raw 0\n").0, 200);
        assert_eq!(env.up("name=track01.bin&system=dreamcast&dir=Game", b"t").0, 200);
        assert_eq!(env.up("name=track02.raw&system=dreamcast&dir=Game", b"t").0, 200);
        assert!(env.roms.join("dreamcast/Game/Game.gdi").is_file() && env.roms.join("dreamcast/Game/track01.bin").is_file() && env.roms.join("dreamcast/Game/track02.raw").is_file());
        assert_eq!(env.up("name=Cd.cue&system=segacd&dir=Cd", b"FILE \"Cd.bin\" BINARY").0, 200);
        assert_eq!(env.up("name=Cd.bin&system=segacd&dir=Cd", b"t").0, 200);
        assert_eq!(env.up("name=Audio.wav&system=segacd&dir=Cd", b"t").0, 200);
        assert!(env.roms.join("segacd/Cd/Cd.bin").is_file() && env.roms.join("segacd/Cd/Audio.wav").is_file());
        // Only for systems with disc sheets, and never a type nobody lists for them.
        assert_eq!(env.up("name=Game.bin&system=ps2", b"t").0, 403);
        assert_eq!(env.up("name=Game.raw&system=gba", b"t").0, 403);
        assert_eq!(env.up("name=Game.exe&system=dreamcast", b"t").0, 403);
    }

    #[test]
    fn names_that_try_to_leave_the_folder_are_cut_to_their_last_part() {
        let env = Env::new(IDLE);
        for (raw, kept) in [
            ("..%2F..%2F..%2Fetc%2Fevil.gba", "evil.gba"),
            ("%2Fstorage%2F.config%2Fx.gba", "x.gba"),
            ("..%5C..%5Cwin.gba", "win.gba"),
            (".hidden.gba", "hidden.gba"),
            ("ev%E2%80%AEabg.gba", "ev_abg.gba"),
        ] {
            let (status, body) = env.up(&format!("name={raw}"), b"x");
            assert_eq!(status, 200, "{raw}: {body}");
            assert!(env.roms.join("gba").join(kept).is_file(), "{raw} -> {kept}");
        }
        for raw in ["..", ".", "%2F", "...."] {
            assert_eq!(env.up(&format!("name={raw}&system=gba"), b"x").0, 403, "{raw}");
        }
        // Unicode names are kept as they are.
        assert_eq!(env.up("name=%E3%83%9D%E3%82%B1%E3%83%83%E3%83%88.gba", b"x").0, 200);
        assert!(env.roms.join("gba/ポケット.gba").is_file());
        // A long name is cut, its extension kept.
        let long = "a".repeat(400);
        assert_eq!(env.up(&format!("name={long}.gba"), b"x").0, 200);
        assert!(std::fs::read_dir(env.roms.join("gba")).unwrap().flatten().any(|e| e.file_name().len() <= 200 && e.file_name().to_string_lossy().ends_with(".gba") && e.file_name().to_string_lossy().starts_with("aaaa")));
        let all: Vec<PathBuf> = walk(env.roms.parent().unwrap());
        assert!(all.iter().all(|p| p.starts_with(&env.roms)), "nothing outside roms: {all:?}");
    }

    #[test]
    fn saves_and_bios_files_are_refused() {
        let env = Env::new(IDLE);
        for name in ["Game.srm", "Game.sav", "Game.state1", "scph1001.bin", "gba_bios.bin", "bios_CD_U.bin"] {
            let (status, body) = env.up(&format!("name={name}&system=psx"), b"x");
            assert_eq!(status, 403, "{name}: {body}");
        }
    }

    #[test]
    fn refusals_carry_a_code_for_the_phones_language() {
        let env = Env::new(IDLE);
        for (query, code) in [("name=Game.srm&system=psx", "saves"), ("name=scph1001.bin&system=psx", "bios"), ("name=Game.xyz", "no_system"), ("name=Game.gba&system=psx", "wrong_system")] {
            let (_, body) = env.up(query, b"x");
            assert!(body.contains(&format!("\"code\":\"{code}\"")), "{query}: {body}");
        }
        std::fs::create_dir_all(env.roms.join("gba")).unwrap();
        std::fs::write(env.roms.join("gba/Keep.gba"), b"old").unwrap();
        let (_, body) = env.up("name=Keep.gba", b"new");
        assert!(body.contains("\"code\":\"exists\""), "{body}");
        assert_eq!(refusal_code(&Refused::Short), Some("cut_off"));
        assert_eq!(refusal_code(&Refused::Io("x".into())), None, "unknown ones fall back to the English sentence");
    }

    #[test]
    fn an_existing_file_is_never_replaced() {
        let env = Env::new(IDLE);
        std::fs::create_dir_all(env.roms.join("gba")).unwrap();
        std::fs::write(env.roms.join("gba/Keep.gba"), b"old").unwrap();
        let t = env.token();
        let (status, body) = env.post(&format!("/{t}/check?name=Keep.gba&size=3"), b"");
        assert_eq!(status, 200);
        assert!(body.contains("\"exists\":true"), "{body}");
        let (status, body) = env.up("name=Keep.gba", b"new");
        assert_eq!(status, 409, "{body}");
        assert_eq!(std::fs::read(env.roms.join("gba/Keep.gba")).unwrap(), b"old");
        assert_eq!(env.up("name=Keep.gba&overwrite=1", b"new").0, 409, "not even when asked to");
        assert_eq!(std::fs::read(env.roms.join("gba/Keep.gba")).unwrap(), b"old");
        assert!(env.leftovers().is_empty());
    }

    #[test]
    fn too_large_or_no_room_is_said_before_a_byte_is_written() {
        let env = Env::new(IDLE);
        let t = env.token();
        // Each system its own limit (security review M5): a cartridge is at most 64 MB here.
        let (status, body) = env.post(&format!("/{t}/check?name=Big.gba&size={}", CART + 1), b"");
        assert_eq!(status, 413, "{body}");
        assert!(body.contains("64 MB"), "{body}");
        // Fixed capacity belongs to this server. Sampling the shared filesystem twice races
        // every concurrent writer (and every reservation in another upload test).
        *env.server().inner.capacity.lock().unwrap() = Some((Some(files::RESERVE + 1024), 0));
        let (status, body) = env.post(&format!("/{t}/check?name=Big.chd&system=psx&size=1025"), b"");
        assert_eq!(status, 507, "{body}");
        let (status, body) = env.up("name=Big.chd&system=psx", &[0; 1025]);
        assert_eq!(status, 507, "{body}");
        // Exact boundary succeeds at preflight, and the system maximum is checked first.
        assert_eq!(env.post(&format!("/{t}/check?name=Fits.chd&system=psx&size=1024"), b"").0, 200);
        let (status, body) = env.post(&format!("/{t}/check?name=Big.chd&system=psx&size={}", files::MAX_ROM + 1), b"");
        assert_eq!(status, 413, "{body}");
        // The upload itself checks again: the phone announces more than fits and sends nothing.
        let mut s = TcpStream::connect(env.server().addr()).unwrap();
        s.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
        write!(s, "POST /{t}/upload?name=Big.gba HTTP/1.1\r\nHost: {}\r\nContent-Length: {}\r\n\r\n", env.host(), CART + 1).unwrap();
        let mut out = String::new();
        let _ = s.read_to_string(&mut out);
        assert!(out.starts_with("HTTP/1.1 413"), "{out}");
        assert!(!env.roms.join("gba").exists() && env.leftovers().is_empty(), "not even its folder is made");
        // No length, no upload.
        let (status, _) = env.send(&format!("POST /{t}/upload?name=x.gba HTTP/1.1\r\n\r\n"), b"");
        assert_eq!(status, 500);
    }

    #[test]
    fn a_card_nearly_full_takes_nothing() {
        // `files::room` as `check` and `upload` use it: the reserve for saves stays free.
        assert!(matches!(files::room(Some(files::RESERVE + 10), 0, 11, files::MAX_ROM), Err(Refused::NoSpace(_))));
        assert!(files::room(Some(files::RESERVE + 10), 0, 10, files::MAX_ROM).is_ok());
        assert!(matches!(files::room(None, 0, 1, files::MAX_ROM), Err(Refused::NoSpace(_))), "free space unknown: nothing");
        // A 4.4 GB disc image is within the limit (no 32-bit length anywhere).
        assert!(files::room(Some(64 << 30), 0, 4_700_000_000, files::MAX_ROM).is_ok());
    }

    #[test]
    fn a_cut_upload_keeps_nothing() {
        let env = Env::new(IDLE);
        let t = env.token();
        let mut s = TcpStream::connect(env.server().addr()).unwrap();
        write!(s, "POST /{t}/upload?name=Half.gba HTTP/1.1\r\nHost: {}\r\nContent-Length: 1000000\r\n\r\n", env.host()).unwrap();
        s.write_all(&[7u8; 300_000]).unwrap();
        wait_for(|| env.leftovers().len() == 1);
        assert_eq!(env.leftovers().len(), 1, "the file arrives under a temporary name");
        drop(s);
        // Removal happens inside receive(), before the server publishes its terminal status.
        // Wait for that publication as well, rather than racing the worker after unlink.
        wait_for(|| env.server().status().files.first().is_some_and(|f| matches!(f.state, FileState::Failed(_))));
        assert!(env.leftovers().is_empty());
        assert!(!env.roms.join("gba/Half.gba").exists());
        let st = env.server().status();
        assert!(matches!(st.files[0].state, FileState::Failed(_)) && st.added == 0);
    }

    #[test]
    fn closing_the_page_mid_upload_stops_listening_and_removes_the_part() {
        let mut env = Env::new(IDLE);
        let (t, addr) = (env.token(), env.server().addr());
        let mut s = TcpStream::connect(addr).unwrap();
        write!(s, "POST /{t}/upload?name=Open.gba HTTP/1.1\r\nHost: {}\r\nContent-Length: 1000000\r\n\r\n", env.host()).unwrap();
        s.write_all(&[7u8; 200_000]).unwrap();
        std::thread::sleep(Duration::from_millis(300));
        assert_eq!(env.leftovers().len(), 1);
        let t0 = Instant::now();
        env.server = None;
        assert!(t0.elapsed() < Duration::from_secs(2), "closing doesn't wait for the phone");
        wait_for(|| env.leftovers().is_empty());
        assert!(!env.roms.join("gba/Open.gba").exists());
        assert!(TcpStream::connect(addr).is_err(), "nothing listens once the page is closed");
        // The phone sees the connection end.
        s.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        let mut buf = [0u8; 16];
        assert!(s.write_all(&[7u8; 300_000]).is_err() || s.read(&mut buf).map_or(true, |n| n == 0));
    }

    #[test]
    fn idle_for_long_enough_closes_by_itself() {
        let env = Env::new(Duration::from_millis(600));
        wait_for(|| env.server().closed() == Some(Closed::Idle));
        std::thread::sleep(TICK * 2);
        assert!(TcpStream::connect(env.server().addr()).is_err());
    }

    #[test]
    fn uploads_at_the_same_time_each_land_in_place() {
        let env = Arc::new(Env::new(IDLE));
        let threads: Vec<_> = (0..4)
            .map(|i| {
                let env = env.clone();
                std::thread::spawn(move || env.up(&format!("name=Game{i}.gba"), &vec![i as u8; 2_000_000]).0)
            })
            .collect();
        for t in threads {
            assert_eq!(t.join().unwrap(), 200);
        }
        for i in 0..4u8 {
            let data = std::fs::read(env.roms.join(format!("gba/Game{i}.gba"))).unwrap();
            assert!(data.len() == 2_000_000 && data.iter().all(|b| *b == i));
        }
        assert_eq!(env.server().status().added, 4);
        assert!(env.leftovers().is_empty());
    }

    #[test]
    fn the_same_name_twice_at_once_keeps_one_and_says_so_for_the_other() {
        let env = Arc::new(Env::new(IDLE));
        let threads: Vec<_> = (0..2)
            .map(|i| {
                let env = env.clone();
                std::thread::spawn(move || env.up("name=Twice.gba", &vec![i as u8; 3_000_000]).0)
            })
            .collect();
        let mut codes: Vec<u16> = threads.into_iter().map(|t| t.join().unwrap()).collect();
        codes.sort();
        assert_eq!(codes, vec![200, 409]);
        assert!(env.leftovers().is_empty());
    }

    #[test]
    fn a_fresh_code_each_time() {
        let a = new_token().unwrap();
        let b = new_token().unwrap();
        assert_eq!(a.len(), 8);
        assert_ne!(a, b);
        assert!(a.bytes().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()));
        assert!(!a.contains(['0', 'o', '1', 'l', 'i']));
    }

    #[test]
    fn the_page_needs_nothing_from_elsewhere() {
        for (name, text) in [("html", PAGE_HTML), ("js", PAGE_JS), ("css", PAGE_CSS)] {
            for bad in ["http://", "https://", "//cdn", "@import", "fonts.googleapis"] {
                assert!(!text.contains(bad), "{name} contains {bad}");
            }
        }
        // The script is a file of its own (the security headers allow no inline script); the data block isn't run.
        assert!(PAGE_HTML.contains("type=\"application/json\""));
        assert!(!PAGE_HTML.contains("onclick") && !PAGE_HTML.contains("style=\""));
    }

    /// Security review M1: only game files of the systems in ALLOWED; nothing a launcher or interpreter runs.
    #[test]
    fn launcher_files_scripts_and_interpreters_are_never_taken() {
        let env = Env::new(IDLE);
        for (name, sys) in [("GoodGame.doom", "doom"), ("Game.wad", "doom"), ("GoodGame.singe", "daphne"), ("Lair.daphne", "daphne"), ("start.sh", "ports"), ("Cart.p8", "pico-8")] {
            let (status, body) = env.up(&format!("name={name}&system={sys}"), b"IWAD=game.wad -config /outside/settings.ini");
            assert_eq!(status, 403, "{name}: {body}");
            let (status, _) = env.up(&format!("name={name}"), b"x");
            assert_eq!(status, 403, "{name} without a system");
        }
        for sys in ["doom", "daphne", "ports", "pico-8"] {
            assert!(!env.roms.join(sys).exists(), "{sys}: nothing made");
        }
        let (_, page) = env.send(&format!("GET /{}/ HTTP/1.1\r\n\r\n", env.token()), b"");
        for name in ["DOOM", "Daphne", "PICO-8", "\"Ports\""] {
            assert!(!page.contains(name), "{name} is not offered");
        }
        // A system's other types are not offered either: .zip for Daphne isn't a way in.
        assert_eq!(env.up("name=Lair.zip&system=daphne", b"x").0, 403);
    }

    /// Security review M2: the moment the Pocket goes dark, the page closes and its code is gone.
    #[test]
    fn going_dark_closes_an_open_page_at_once() {
        let env = Env::new(IDLE);
        env.server().register();
        let (t, addr) = (env.token(), env.server().addr());
        let mut s = TcpStream::connect(addr).unwrap();
        write!(s, "POST /{t}/upload?name=Dark.gba HTTP/1.1\r\nHost: {}\r\nContent-Length: 1000000\r\n\r\n", env.host()).unwrap();
        s.write_all(&[1u8; 100_000]).unwrap();
        wait_for(|| env.leftovers().len() == 1);
        let calls = sleeping_calls();
        sleeping();
        assert!(sleeping_calls() > calls);
        assert_eq!(env.server().closed(), Some(Closed::Slept));
        wait_for(|| TcpStream::connect(addr).is_err());
        wait_for(|| env.leftovers().is_empty());
        assert!(!env.roms.join("gba/Dark.gba").exists());
    }

    /// Security review M3: connections without the code never keep the page open, and are capped.
    #[test]
    fn connections_without_the_code_never_keep_it_open() {
        let env = Env::new(Duration::from_millis(800));
        let held: Vec<TcpStream> = (0..MAX_UNAUTH).map(|_| TcpStream::connect(env.server().addr()).unwrap()).collect();
        std::thread::sleep(Duration::from_millis(100));
        // The slots for connections without the code are full: one more is turned away at once.
        let (status, _) = env.send_raw("GET / HTTP/1.1\r\n\r\n", b"");
        assert_eq!(status, 503);
        // Broken heads keep nothing open either; they count as tries.
        wait_for(|| env.server().closed() == Some(Closed::Idle));
        drop(held);
    }

    #[test]
    fn broken_heads_count_as_tries() {
        let env = Env::new(IDLE);
        for _ in 0..MAX_BAD {
            env.send_raw("NOT HTTP\r\n\r\n", b"");
        }
        wait_for(|| env.server().closed() == Some(Closed::Guessing));
    }

    /// Security review M4: a check makes nothing; a refused or cut upload leaves no folder behind; a page takes at
    /// most MAX_NEW_ENTRIES files and folders.
    #[test]
    fn checks_and_refused_uploads_make_nothing_on_the_card() {
        let env = Env::new(IDLE);
        let t = env.token();
        for n in 0..5 {
            let (status, _) = env.post(&format!("/{t}/check?name=x.cue&system=psx&dir=Empty{n}&size={}", DISC + 1), b"");
            assert_eq!(status, 413);
            let (status, _) = env.post(&format!("/{t}/check?name=x.cue&system=psx&dir=Fine{n}&size=10"), b"");
            assert_eq!(status, 200);
        }
        assert!(!env.roms.join("psx").exists(), "no system folder, no disc folders: {:?}", walk(&env.roms));
        // An upload cut off in a new disc folder takes the folder (and the system folder it made) with it.
        let mut s = TcpStream::connect(env.server().addr()).unwrap();
        write!(s, "POST /{t}/upload?name=Disc.chd&system=psx&dir=New%20Game HTTP/1.1\r\nHost: {}\r\nContent-Length: 100000\r\n\r\n", env.host()).unwrap();
        s.write_all(&[3u8; 1000]).unwrap();
        std::thread::sleep(Duration::from_millis(200));
        drop(s);
        wait_for(|| !env.roms.join("psx").exists());
        // The page's own count of files and folders.
        env.server().inner.made.store(MAX_NEW_ENTRIES, Ordering::SeqCst);
        let (status, body) = env.up("name=More.gba", b"x");
        assert_eq!(status, 403, "{body}");
        assert!(body.contains("open it again"), "{body}");
    }

    /// Security review M6: disc sheets may name only files beside them.
    #[test]
    fn disc_sheets_may_name_only_files_beside_them() {
        let env = Env::new(IDLE);
        for (name, sheet) in [
            ("Out.cue", "FILE \"/tmp/sol-review-secret.bin\" BINARY\n  TRACK 01 MODE2/2352\n"),
            ("Up.cue", "FILE \"../../outside.bin\" BINARY\n"),
            ("Win.cue", "FILE C:\\games\\x.bin BINARY\n"),
            ("Text.cue", "FILE \"ok.bin\" BINARY\nCDTEXTFILE \"/etc/passwd\"\n"),
            ("Discs.m3u", "Disc 1.cue\n/storage/roms/bios/scph1001.bin\n"),
            ("Hidden.m3u", "#EXTM3U\n.secret.chd\n"),
            ("Disc.gdi", "3\n1 0 4 2352 track01.bin 0\n2 600 0 2352 \"../x/track02.raw\" 0\n"),
        ] {
            let (status, body) = env.up(&format!("name={name}&system=psx"), sheet.as_bytes());
            assert_eq!(status, 403, "{name}: {body}");
        }
        assert!(walk(&env.roms).iter().all(|p| p.is_dir()), "nothing kept: {:?}", walk(&env.roms));
        let good = "FILE \"Game (Track 1).bin\" BINARY\n  TRACK 01 MODE2/2352\nFILE Game2.bin BINARY\n";
        assert_eq!(env.up("name=Game.cue&system=psx&dir=Game", good.as_bytes()).0, 200);
        assert_eq!(std::fs::read_to_string(env.roms.join("psx/Game/Game.cue")).unwrap(), good);
        assert_eq!(env.up("name=Game.m3u&system=psx", b"#EXTM3U\nGame (Disc 1).chd\nGame (Disc 2).chd\n").0, 200);
        // Too large for a sheet: refused before it is read.
        assert_eq!(env.up("name=Huge.m3u&system=psx", &vec![b'a'; (MAX_SHEET + 1) as usize]).0, 413);
    }

    #[test]
    fn what_a_sheet_names() {
        assert_eq!(sheet_refs("cue", "REM x\nFILE \"A B.bin\" BINARY\n file c.wav WAVE\n"), ["A B.bin", "c.wav"]);
        assert_eq!(sheet_refs("gdi", "2\n1 0 4 2352 \"t 1.bin\" 0\n2 450 0 2352 t2.raw 0\n"), ["t 1.bin", "t2.raw"]);
        assert_eq!(sheet_refs("m3u", "#EXTM3U\n\nd1.chd\r\nd2.chd\n"), ["d1.chd", "d2.chd"]);
        for bad in ["/x", "../x", "a/b", "a\\b", ".x", "C:x", "", ".."] {
            assert!(!plain_name(bad), "{bad:?}");
        }
    }

    /// Security review L1: the Host must be this page; a browser's Origin and Fetch Metadata too.
    #[test]
    fn only_this_page_may_post_with_the_code() {
        let env = Env::new(IDLE);
        let (t, h) = (env.token(), env.host());
        let post = |extra: &str| env.send_raw(&format!("POST /{t}/upload?name=Csrf.gba HTTP/1.1\r\n{extra}Content-Length: 1\r\n\r\n"), b"x").0;
        assert_eq!(post("Host: attacker.invalid\r\n"), 403);
        assert_eq!(post(""), 403, "no Host");
        assert_eq!(post(&format!("Host: {h}\r\nOrigin: http://attacker.invalid\r\n")), 403);
        assert_eq!(post(&format!("Host: {h}\r\nSec-Fetch-Site: cross-site\r\n")), 403);
        assert_eq!(post(&format!("Host: {h}\r\nSec-Fetch-Site: none\r\n")), 403, "a POST is never typed in");
        assert!(!env.roms.join("gba/Csrf.gba").exists());
        assert_eq!(env.send_raw(&format!("GET /{t}/ HTTP/1.1\r\nHost: attacker.invalid\r\n\r\n"), b"").0, 403, "DNS rebinding");
        assert_eq!(env.send_raw(&format!("GET /{t}/ HTTP/1.1\r\nHost: {h}\r\nSec-Fetch-Site: none\r\n\r\n"), b"").0, 200, "opened from the camera");
        assert_eq!(post(&format!("Host: {h}\r\nOrigin: http://{h}\r\nSec-Fetch-Site: same-origin\r\n")), 200, "the page itself");
    }

    /// Security review L3: temporary files live in roms/.pocket-ui-uploads; a file of their pattern elsewhere is never touched.
    #[test]
    fn temporary_files_live_only_in_the_uploads_folder() {
        let env = Env::new(IDLE);
        std::fs::create_dir_all(env.roms.join("gba")).unwrap();
        std::fs::write(env.roms.join("gba/.Keep.gba.99999999-0.upload"), b"keep").unwrap();
        let t = env.token();
        let mut s = TcpStream::connect(env.server().addr()).unwrap();
        write!(s, "POST /{t}/upload?name=Slow.gba HTTP/1.1\r\nHost: {}\r\nContent-Length: 1000\r\n\r\n", env.host()).unwrap();
        s.write_all(&[1u8; 10]).unwrap();
        wait_for(|| std::fs::read_dir(env.roms.join(files::UPLOADS_DIR)).is_ok_and(|d| d.count() == 1));
        drop(s);
        assert_eq!(env.up("name=Other.gba", b"x").0, 200);
        assert_eq!(std::fs::read(env.roms.join("gba/.Keep.gba.99999999-0.upload")).unwrap(), b"keep");
    }

    /// Review recheck M6: PCSX-ReARMed treats any path containing ".m3u" as a playlist; such a marker anywhere but as
    /// the file's own checked last ending is refused, in the name and in the disc's folder.
    #[test]
    fn marker_endings_inside_names_and_folders_are_refused() {
        let env = Env::new(IDLE);
        for q in [
            "name=Alias.m3u.bin&system=psx",
            "name=Alias.M3U.bin&system=psx",
            "name=Alias.bin&system=psx&dir=Album.m3u",
            "name=Alias.chd&system=psx&dir=Disc.CUE%20Set",
            "name=Game.cue.chd&system=psx",
            "name=Boot.exe.bin&system=psx",
            "name=Game.m3uX.bin&system=psx",
            "name=Two.m3u.m3u&system=psx",
            "name=Tool.exe&system=psx",
        ] {
            let (status, body) = env.up(q, b"../../outside.bin\n");
            assert_eq!(status, 403, "{q}: {body}");
        }
        assert!(walk(&env.roms).iter().all(|p| p.is_dir()), "nothing kept: {:?}", walk(&env.roms));
        // As the file's own last ending, checked: fine.
        assert_eq!(env.up("name=Game.m3u&system=psx&dir=Game", b"Game (Disc 1).cue\nGame (Disc 2).cue\n").0, 200);
        assert_eq!(env.up("name=Game%20(Disc%201).cue&system=psx&dir=Game", b"FILE \"Game (Disc 1).bin\" BINARY\n").0, 200);
        // A playlist may not name another playlist or a program.
        assert_eq!(env.up("name=Loop.m3u&system=psx", b"Game.m3u\n").0, 403);
        assert_eq!(env.up("name=Exe.m3u&system=psx", b"boot.exe\n").0, 403);
    }

    /// Review recheck M6: a one-line gdi (track on the count's line) and a sheet naming a link are refused.
    #[cfg(unix)]
    #[test]
    fn a_one_line_gdi_and_a_name_that_is_a_link_are_refused() {
        let env = Env::new(IDLE);
        assert_eq!(env.up("name=OneLine.gdi&system=dreamcast", b"1 1 0 4 2352 /tmp/secret.bin 0\n").0, 403);
        assert_eq!(sheet_refs("gdi", "1 1 0 4 2352 /tmp/secret.bin 0\n"), ["/tmp/secret.bin"]);
        std::fs::create_dir_all(env.roms.join("psx")).unwrap();
        std::os::unix::fs::symlink("/etc/hostname", env.roms.join("psx/Linked.bin")).unwrap();
        let (status, body) = env.up("name=Linked.cue&system=psx", b"FILE \"Linked.bin\" BINARY\n");
        assert_eq!(status, 403, "{body}");
    }

    /// Review recheck N1: a cut upload doesn't take a folder another upload under way still writes into.
    #[test]
    fn a_cut_upload_leaves_the_folders_of_another_upload_alone() {
        for dir in ["", "&dir=Shared%20Game"] {
            let env = Env::new(IDLE);
            let t = env.token();
            let start = |name: &str| {
                let mut s = TcpStream::connect(env.server().addr()).unwrap();
                write!(s, "POST /{t}/upload?name={name}&system=psx{dir} HTTP/1.1\r\nHost: {}\r\nContent-Length: 1000\r\n\r\n", env.host()).unwrap();
                s.write_all(&[5u8; 100]).unwrap();
                s
            };
            let first = start("First.chd");
            wait_for(|| env.leftovers().len() == 1);
            let mut second = start("Second.chd");
            wait_for(|| env.leftovers().len() == 2);
            drop(first);
            wait_for(|| env.leftovers().len() == 1);
            second.write_all(&[5u8; 900]).unwrap();
            second.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
            let mut out = String::new();
            let _ = second.read_to_string(&mut out);
            assert!(out.starts_with("HTTP/1.1 200"), "{dir}: {out}");
            let sub = if dir.is_empty() { "" } else { "Shared Game/" };
            assert_eq!(std::fs::read(env.roms.join(format!("psx/{sub}Second.chd"))).unwrap().len(), 1000);
        }
    }

    fn wait_for(f: impl Fn() -> bool) {
        let t0 = Instant::now();
        while !f() {
            assert!(t0.elapsed() < Duration::from_secs(5), "timed out");
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}
