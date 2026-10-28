//! The web companion: open `http://<pocket>` on a phone or computer in the same tailnet to browse the library, edit
//! what the player says about a game, upload games, BIOS files and PortMaster data, and see play time and the battery
//! (docs/user/web-companion.md; protocol, endpoints and threat model: docs/WEB-COMPANION.md).
//!
//! pocket-ui runs as root, so this is built to be small and closed by default:
//!
//! - **Off** until the player switches it on (Settings → Connections → Web companion); then a supervisor thread and three workers
//!   run, all at the lowest CPU and I/O priority. Nothing of it runs on the UI thread: edits reach the App as one
//!   `UserEvent` each and are applied there like a button press (`app_web.rs`).
//! - **Where it listens** ([`net`]): only on the Tailscale interface's own addresses (100.64.0.0/10 and
//!   fd7a:115c:a1e0::/48 on an interface called `tailscale…`), with `SO_BINDTODEVICE`, never on `0.0.0.0` and never on
//!   the Wi-Fi (there is no switch for that); every connection is checked again by address.
//! - **Who gets in** ([`auth`]): the device owner's own tailnet devices, recognised by tailscaled's LocalAPI `whois`,
//!   or a browser paired with a six-digit code shown on the Pocket (then an HttpOnly, SameSite=Strict cookie). Every
//!   write needs a CSRF token in a header; requests whose Host is not one of the Pocket's names are refused (DNS
//!   rebinding).
//! - **What it touches** ([`files`]): uploads only into a system's ROM folder, `roms/bios` and the PortMaster drop
//!   folders, with sanitised names, a size limit, a check of free space, a temporary file and a rename that never
//!   replaces an existing file unless the player said so. No shell, no programs except `grim` for the screenshot.
//! - **What it costs**: no work while no browser is connected (the supervisor wakes every 30 s to see whether the
//!   Tailscale address changed, one `getifaddrs`); while a game runs, everything except the status, the session and
//!   the screenshot (rate-limited) answers "playing", and an upload under way waits until the game ends.

pub mod api;
pub mod assets;
pub mod auth;
pub mod files;
pub mod http;
pub mod net;
pub mod server;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use serde::{Deserialize, Serialize};

/// The port it listens on: `http://pocket` with no port to type.
pub const PORT: u16 = 80;

/// The player's choices, in `web.json` in the data directory. Paired browsers are kept apart, in `web/` (`auth`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(default)]
pub struct Prefs {
    /// The web companion runs (off by default).
    pub enabled: bool,
    /// The owner's own tailnet devices get in without pairing (Tailscale's `whois`).
    pub trust_tailnet: bool,
}

impl Default for Prefs {
    fn default() -> Self {
        Prefs { enabled: false, trust_tailnet: true }
    }
}

const PREFS_FILE: &str = "web.json";

impl Prefs {
    pub fn load(data_dir: &Path) -> Prefs {
        crate::fsutil::read_json_or_backup(&data_dir.join(PREFS_FILE)).unwrap_or_default()
    }

    /// False when the card refused the write.
    pub fn save(&self, data_dir: &Path) -> bool {
        match crate::fsutil::write_json(&data_dir.join(PREFS_FILE), self) {
            Ok(()) => true,
            Err(e) => {
                eprintln!("web: can't save {PREFS_FILE}: {e}");
                false
            }
        }
    }
}

/// The game on screen, for "Now playing".
#[derive(Clone, Debug, PartialEq)]
pub struct NowPlaying {
    pub system: String,
    pub name: String,
    pub path: PathBuf,
    /// Unix seconds.
    pub since: u64,
}

/// A change the browser asks for. The App applies it on its own thread, with the same functions a button press uses
/// (`app_web.rs`), so the shelf and the files never disagree.
#[derive(Clone, Debug, PartialEq)]
pub enum Edit {
    Favorite { system: String, name: String, path: PathBuf, on: bool },
    Status { path: PathBuf, status: crate::library_meta::Status },
    Note { path: PathBuf, note: String },
    /// The player's rating, 1 to 5; 0 takes it away.
    Rating { path: PathBuf, rating: u8 },
    /// Puts sticker `id` on the game, or takes it off.
    Sticker { path: PathBuf, id: u32, on: bool },
    /// Makes a sticker (and puts it on `path`, if given).
    NewSticker { name: String, path: Option<PathBuf> },
    RenameSticker { id: u32, name: String },
    DeleteSticker { id: u32 },
    /// Smart shelves are addressed by their place and their name, so a shelf that moved meanwhile is not the one changed.
    RenameShelf { index: usize, was: String, name: String },
    DeleteShelf { index: usize, was: String },
    /// Curator's Versions: `path` is the version of `work` that lists show and start.
    Preferred { work: String, path: PathBuf },
    /// Games were uploaded: the library as the server just read it (on its own thread), for the App to take over at
    /// a quiet moment.
    LibraryChanged(Systems),
}

/// The game list as `library::load` reads it, passed along whole.
#[derive(Clone)]
pub struct Systems(pub Arc<Vec<crate::library::System>>);

impl std::fmt::Debug for Systems {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Systems({} systems)", self.0.len())
    }
}

impl PartialEq for Systems {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

/// Where edits go: the App installs a function that sends them to its event loop, with a channel for the answer.
pub type EditSink = Box<dyn Fn(Edit, std::sync::mpsc::Sender<Result<(), String>>) -> bool + Send + Sync>;

/// News for Settings → Connections → Web companion (a browser paired, the server stopped).
pub type NoticeSink = Box<dyn Fn() + Send + Sync>;

/// What the App and the server share. Everything is behind a lock or an atomic; the App only stores into it.
pub struct Shared {
    pub data_dir: PathBuf,
    pub es_systems: PathBuf,
    /// A game is running: the server pauses everything but the smallest requests.
    pub game_running: AtomicBool,
    pub now_playing: Mutex<Option<NowPlaying>>,
    pub prefs: Mutex<Prefs>,
    /// The code shown on the Pocket while "Pair a browser" is open.
    pub pairing: Mutex<Option<auth::Pairing>>,
    pub sessions: Mutex<auth::Sessions>,
    pub edits: Mutex<Option<EditSink>>,
    pub notice: Mutex<Option<NoticeSink>>,
    /// Curator's index, as the App last built it (smart shelves and versions are read from it).
    pub curator: Mutex<Option<Arc<crate::curator::Index>>>,
    /// The addresses the server listens on now ("100.101.102.103"), for the Settings page.
    pub listening: Mutex<Vec<std::net::SocketAddr>>,
    /// Why it does not listen although Tailscale is up (port 80 taken), for the Settings page; None when it listens.
    pub problem: Mutex<Option<String>>,
    /// The Pocket's name in the tailnet ("pocket"), once tailscaled said it.
    pub tailnet_name: Mutex<Option<String>>,
    /// Requests answered, for the page's "last used".
    pub requests: AtomicU64,
    /// The Pocket shows something private (any Settings page, the first-start steps): no screenshot is taken.
    pub screen_private: AtomicBool,
    /// Counts changes of "Your Tailscale devices": the server forgets what tailscaled said about each peer.
    pub trust_epoch: AtomicU64,
    /// Counts games hidden, shown again or deleted on the Pocket: the server reads its game list again at the next request.
    pub library_epoch: AtomicU64,
}

impl Shared {
    pub fn new(data_dir: PathBuf, es_systems: PathBuf, prefs: Prefs) -> Arc<Shared> {
        let sessions = auth::Sessions::load(&data_dir);
        Arc::new(Shared {
            data_dir,
            es_systems,
            game_running: AtomicBool::new(false),
            now_playing: Mutex::new(None),
            prefs: Mutex::new(prefs),
            pairing: Mutex::new(None),
            sessions: Mutex::new(sessions),
            edits: Mutex::new(None),
            notice: Mutex::new(None),
            curator: Mutex::new(None),
            listening: Mutex::new(Vec::new()),
            problem: Mutex::new(None),
            tailnet_name: Mutex::new(None),
            requests: AtomicU64::new(0),
            screen_private: AtomicBool::new(false),
            trust_epoch: AtomicU64::new(0),
            library_epoch: AtomicU64::new(0),
        })
    }

    pub fn prefs(&self) -> Prefs {
        *self.prefs.lock().unwrap()
    }

    pub fn playing(&self) -> bool {
        self.game_running.load(Ordering::Relaxed)
    }

    /// Hands an edit to the App and waits for its answer (at most three seconds: a game starting can keep it busy).
    pub fn edit(&self, e: Edit) -> Result<(), String> {
        let (tx, rx) = std::sync::mpsc::channel();
        let sent = match self.edits.lock().unwrap().as_ref() {
            Some(sink) => sink(e, tx),
            None => false,
        };
        if !sent {
            return Err("The Pocket is busy. Try again in a moment.".into());
        }
        rx.recv_timeout(std::time::Duration::from_secs(3)).unwrap_or_else(|_| Err("The Pocket did not answer. Try again in a moment.".into()))
    }

    /// "Your Tailscale devices" changed: who was let in by Tailscale is asked again at the next request.
    pub fn trust_changed(&self) {
        self.trust_epoch.fetch_add(1, Ordering::Relaxed);
    }

    /// A game was hidden, shown again or deleted on the Pocket: the game list is read again at the next request.
    pub fn library_changed(&self) {
        self.library_epoch.fetch_add(1, Ordering::Relaxed);
    }

    /// Tells the Settings page that something changed.
    pub fn notify(&self) {
        if let Some(n) = self.notice.lock().unwrap().as_ref() {
            n();
        }
    }
}

/// What another feature can show on the Stats page: play history and insights (feat/play-history). It registers a
/// function at start; the companion calls it for `GET /api/stats` and puts what it returns under `"history"`. Without
/// it, the page shows the totals of `library.json`.
pub type HistorySource = fn(data_dir: &Path) -> Option<serde_json::Value>;

static HISTORY: OnceLock<HistorySource> = OnceLock::new();

/// Registers the play history's read function (once; later calls are ignored).
pub fn set_history_source(f: HistorySource) {
    let _ = HISTORY.set(f);
}

pub(crate) fn history_source() -> Option<HistorySource> {
    HISTORY.get().copied()
}

/// Lowers the calling thread to the lowest CPU priority and the idle I/O class: the companion only gets what games and
/// menus leave over.
pub fn lower_priority() {
    #[cfg(target_os = "linux")]
    // SAFETY: setpriority and ioprio_set on this thread's own id only change its scheduling.
    unsafe {
        let tid = libc::syscall(libc::SYS_gettid) as libc::id_t;
        libc::setpriority(libc::PRIO_PROCESS, tid, 19);
        // IOPRIO_WHO_PROCESS = 1, IOPRIO_CLASS_IDLE = 3 (shifted by IOPRIO_CLASS_SHIFT 13).
        libc::syscall(libc::SYS_ioprio_set, 1, tid as libc::c_long, (3 << 13) as libc::c_long);
    }
}

/// `n` random bytes from the kernel.
pub(crate) fn random_bytes<const N: usize>() -> std::io::Result<[u8; N]> {
    use std::io::Read;
    let mut b = [0u8; N];
    std::fs::File::open("/dev/urandom")?.read_exact(&mut b)?;
    Ok(b)
}

/// Lower-case hex.
pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests;
