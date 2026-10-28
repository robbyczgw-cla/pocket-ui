//! pocket-core's RetroAchievements session for the running game: one rc_client and two threads.
//!
//! - The **game thread** (pocket-core's loop) calls [`Session::frame`] after every emulated frame (rcheevos reads the
//!   game's memory and checks the achievements there, well under a millisecond), [`Session::idle`] while paused or
//!   rewinding, [`Session::reset`] after a state was loaded, and takes the [`Notice`]s (unlocks for the HUD). It never
//!   waits for anything here: no network, no file, no hashing.
//! - **"achievements"** starts the session: hashes the ROM, signs in with the saved token and loads the game's set
//!   (rc_client's asynchronous calls), sends unlocks earlier sessions left (`offline::Pending`), fetches badges and
//!   writes the files pocket-ui reads (`snapshot`), at most once a second.
//! - **"achievements-net"** carries rc_client's requests one at a time through `offline::exchange` (curl with timeouts;
//!   the kept copy when offline) and hands the answers back to rc_client on that thread (rc_client locks itself).
//!
//! Without network nothing waits: the game runs from the first frame, achievements join when rc_client is ready.

use std::ffi::{c_char, c_int, c_void, CString};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use super::account::Account;
use super::ffi::{self, RawAchievement, RcClient, ServerCallback};
use super::http::{Request, Transport};
use super::offline::{self, Pending};
use super::snapshot::{Entry, Snapshot, State, Summary};
use crate::image::Image;
use crate::libretro::AchievementMemory;

/// What the session needs to know about the game.
pub struct Config {
    pub data_dir: PathBuf,
    pub account: Account,
    pub system: String,
    /// The ROM as pocket-ui passed it (a zip stays a zip).
    pub rom: PathBuf,
    pub console: u32,
    /// The core's `library_name`, for rcheevos' list of cores that can't be trusted with a console.
    pub core_name: String,
    /// Where the pause menu's list goes (`snapshot::SNAPSHOT_FILE`).
    pub snapshot: PathBuf,
    /// rc_client's log at verbose level (POCKET_CORE_TRACE).
    pub verbose: bool,
}

/// News for the game thread.
pub enum Notice {
    /// An achievement was just earned.
    Unlocked { id: u32, title: String, points: u32, badge: String },
    /// A badge for an unlock: its picture, or None when it couldn't be had.
    Badge { name: String, image: Option<Image> },
    /// The game's set is loaded (`unlocked` of `total`).
    Ready { unlocked: u32, total: u32 },
    /// Every achievement of the game is unlocked.
    Completed { title: String },
    /// The saved token was refused: the account needs a new sign-in.
    SignInAgain,
}

impl std::fmt::Debug for Notice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Notice::Unlocked { id, title, points, badge } => write!(f, "Unlocked({id}, {title:?}, {points}, {badge:?})"),
            Notice::Badge { name, image } => write!(f, "Badge({name:?}, {})", image.as_ref().map_or("none".into(), |i| format!("{}x{}", i.w, i.h))),
            Notice::Ready { unlocked, total } => write!(f, "Ready({unlocked} of {total})"),
            Notice::Completed { title } => write!(f, "Completed({title:?})"),
            Notice::SignInAgain => f.write_str("SignInAgain"),
        }
    }
}

/// One request of rc_client on its way.
struct Job {
    req: Request,
    callback: ServerCallback,
    data: usize,
}

enum Work {
    /// Something changed: the files are written within a second.
    Dirty,
    /// The pause menu opened: write the files now.
    Flush,
    /// The game's memory can be read: the set may be loaded (rc_client checks every address once at load).
    MemoryReady,
    /// Signed in: unlocks of earlier sessions can go.
    SendPending,
    Badge {
        name: String,
        url: String,
    },
    Stop,
}

/// How far the session got, for the snapshot.
struct Status {
    state: State,
    message: String,
    hash: String,
}

/// Everything the threads and rc_client's callbacks share. rc_client's userdata points at it.
struct Shared {
    config: Config,
    transport: Box<dyn Transport>,
    client: AtomicPtr<RcClient>,
    notices: Mutex<Vec<Notice>>,
    jobs: Mutex<Option<Sender<Job>>>,
    work: Mutex<Option<Sender<Work>>>,
    /// Requests handed to the network thread and not answered yet.
    outstanding: AtomicUsize,
    pending: Mutex<Pending>,
    /// The unlocks earlier sessions left: this session sends them once signed in (rc_client sends only its own).
    earlier: Vec<offline::Unlock>,
    status: Mutex<Status>,
    closing: AtomicBool,
}

impl Shared {
    fn client(&self) -> *mut RcClient {
        self.client.load(Ordering::Acquire)
    }

    fn send(&self, w: Work) {
        if let Some(tx) = self.work.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
            let _ = tx.send(w);
        }
    }

    fn notice(&self, n: Notice) {
        self.notices.lock().unwrap_or_else(|e| e.into_inner()).push(n);
    }

    fn set_state(&self, state: State, message: &str) {
        let mut s = self.status.lock().unwrap_or_else(|e| e.into_inner());
        s.state = state;
        s.message = message.to_string();
    }

    fn state(&self) -> State {
        self.status.lock().unwrap_or_else(|e| e.into_inner()).state
    }
}

/// A raw pointer that may cross threads: rc_client is built for that (it locks its own state).
#[derive(Clone, Copy)]
struct Ptr(*mut RcClient);
unsafe impl Send for Ptr {}

unsafe extern "C" fn on_server_call(
    userdata: *mut c_void,
    url: *const c_char,
    post: *const c_char,
    ctype: *const c_char,
    callback: ServerCallback,
    data: *mut c_void,
) {
    let shared = &*(userdata as *const Shared);
    let req = Request {
        url: ffi::text(url),
        post_data: (!post.is_null()).then(|| ffi::text(post)),
        content_type: (!ctype.is_null()).then(|| ffi::text(ctype)),
    };
    let jobs = shared.jobs.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(tx) = jobs.as_ref() {
        shared.outstanding.fetch_add(1, Ordering::AcqRel);
        if tx
            .send(Job {
                req,
                callback,
                data: data as usize,
            })
            .is_err()
        {
            shared.outstanding.fetch_sub(1, Ordering::AcqRel);
        }
    }
}

fn copy(a: &RawAchievement) -> Entry {
    Entry {
        id: a.id,
        title: ffi::text(a.title),
        description: ffi::text(a.description),
        points: a.points,
        unlocked: a.unlocked != 0,
        progress: ffi::text(a.measured_progress),
        percent: a.measured_percent,
        unsupported: a.bucket == ffi::BUCKET_UNSUPPORTED,
    }
}

unsafe extern "C" fn on_event(userdata: *mut c_void, kind: u32, result: c_int, a: *const RawAchievement, message: *const c_char) {
    let shared = &*(userdata as *const Shared);
    let message = ffi::text(message);
    match kind {
        ffi::LOGIN_DONE => {
            if result == ffi::RC_OK {
                shared.send(Work::SendPending);
            } else {
                eprintln!("achievements: sign-in failed ({result}): {message}");
                if matches!(result, ffi::RC_INVALID_CREDENTIALS | ffi::RC_EXPIRED_TOKEN | ffi::RC_ACCESS_DENIED) {
                    shared.set_state(State::SignedOut, &message);
                    shared.notice(Notice::SignInAgain);
                } else {
                    shared.set_state(State::Offline, &message);
                }
                shared.send(Work::Dirty);
            }
        }
        ffi::LOAD_DONE => {
            if result == ffi::RC_OK {
                shared.set_state(State::Ready, "");
                let mut s = [0u32; 5];
                ffi::pocket_rc_summary(shared.client(), s.as_mut_ptr());
                println!("achievements: {} of {} unlocked ({} of {} points)", s[1], s[0], s[3], s[2]);
                shared.notice(Notice::Ready { unlocked: s[1], total: s[0] });
            } else if matches!(shared.state(), State::SignedOut | State::Offline) {
                // The sign-in said why already.
            } else if result == ffi::RC_NO_GAME_LOADED {
                println!("achievements: {message}");
                shared.set_state(State::NoSet, &message);
            } else if result == ffi::RC_NO_RESPONSE {
                shared.set_state(State::Offline, &message);
            } else {
                eprintln!("achievements: the game's set didn't load ({result}): {message}");
                shared.set_state(State::Failed, &message);
            }
            shared.send(Work::Dirty);
        }
        ffi::EVENT_ACHIEVEMENT_TRIGGERED if !a.is_null() => {
            let a = &*a;
            let e = copy(a);
            println!("achievements: unlocked {} \"{}\" ({} points)", e.id, e.title, e.points);
            let badge = ffi::text(a.badge_name);
            shared.notice(Notice::Unlocked {
                id: e.id,
                title: e.title,
                points: e.points,
                badge: badge.clone(),
            });
            shared.send(Work::Badge {
                name: badge,
                url: ffi::text(a.badge_url),
            });
            shared.send(Work::Dirty);
        }
        ffi::EVENT_PROGRESS_SHOW | ffi::EVENT_PROGRESS_UPDATE => shared.send(Work::Dirty),
        ffi::EVENT_GAME_COMPLETED => {
            let (mut title, mut hash, mut badge) = (std::ptr::null(), std::ptr::null(), std::ptr::null());
            ffi::pocket_rc_game(shared.client(), &mut title, &mut hash, &mut badge);
            shared.notice(Notice::Completed { title: ffi::text(title) });
        }
        ffi::EVENT_SERVER_ERROR => eprintln!("achievements: the server refused something ({result}): {message}"),
        ffi::EVENT_DISCONNECTED => println!("achievements: offline, unlocks wait for the network"),
        ffi::EVENT_RECONNECTED => println!("achievements: online again, unlocks sent"),
        _ => {}
    }
}

unsafe extern "C" fn on_log(_userdata: *mut c_void, message: *const c_char) {
    eprintln!("[rcheevos] {}", ffi::text(message));
}

fn now_secs() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// The network thread: one request after the other, answers back to rc_client.
fn net_loop(shared: Arc<Shared>, jobs: Receiver<Job>) {
    for job in jobs {
        let post = job.req.post_data.clone().unwrap_or_default();
        let award = (offline::param(&post, "r") == Some("awardachievement"))
            .then(|| offline::param(&post, "a").and_then(|a| a.parse::<u32>().ok()))
            .flatten();
        let hash = offline::param(&post, "m").unwrap_or_default().to_string();
        // An unlock is written down before it is sent: a crash or the end of the game can't lose it.
        if let Some(id) = award {
            let mut p = shared.pending.lock().unwrap_or_else(|e| e.into_inner());
            p.add(id, &hash, now_secs());
            p.save(&shared.config.data_dir);
        }
        let pending = shared.pending.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let resp = offline::exchange(shared.transport.as_ref(), &shared.config.data_dir, &shared.config.account, &pending, &job.req);
        if let (Some(id), 200) = (award, resp.status) {
            let mut p = shared.pending.lock().unwrap_or_else(|e| e.into_inner());
            if p.remove(id) {
                p.save(&shared.config.data_dir);
            }
            shared.send(Work::Dirty);
        }
        unsafe {
            ffi::pocket_rc_respond(
                job.callback,
                job.data as *mut c_void,
                resp.body.as_ptr() as *const c_char,
                resp.body.len(),
                resp.status,
            );
        }
        shared.outstanding.fetch_sub(1, Ordering::AcqRel);
    }
}

unsafe extern "C" fn collect(ud: *mut c_void, a: *const RawAchievement) {
    (*(ud as *mut Vec<Entry>)).push(copy(&*a));
}

/// The running game as rc_client sees it now.
fn snapshot_of(shared: &Shared) -> Snapshot {
    let client = shared.client();
    let status = shared.status.lock().unwrap_or_else(|e| e.into_inner());
    let mut snap = Snapshot {
        rom: shared.config.rom.to_string_lossy().into_owned(),
        state: status.state,
        message: status.message.clone(),
        user: shared.config.account.shown_name().to_string(),
        waiting: shared.pending.lock().unwrap_or_else(|e| e.into_inner()).ids_for(&status.hash).len() as u32,
        ..Default::default()
    };
    drop(status);
    if snap.state == State::Ready {
        unsafe {
            let (mut title, mut hash, mut badge) = (std::ptr::null(), std::ptr::null(), std::ptr::null());
            snap.game_id = ffi::pocket_rc_game(client, &mut title, &mut hash, &mut badge);
            snap.game = ffi::text(title);
            let name = ffi::text(ffi::pocket_rc_display_name(client));
            if !name.is_empty() {
                snap.user = name;
            }
            let mut s = [0u32; 5];
            ffi::pocket_rc_summary(client, s.as_mut_ptr());
            (snap.total, snap.unlocked, snap.points, snap.points_unlocked) = (s[0], s[1], s[2], s[3]);
            ffi::pocket_rc_list(client, collect, &mut snap.achievements as *mut Vec<Entry> as *mut c_void);
        }
    }
    snap
}

/// Writes the pause menu's snapshot and the game's tally.
fn write_files(shared: &Shared) {
    let snap = snapshot_of(shared);
    snap.write(&shared.config.snapshot);
    let hash = shared.status.lock().unwrap_or_else(|e| e.into_inner()).hash.clone();
    let tally = match snap.state {
        State::Ready => Some(Summary {
            game_id: snap.game_id,
            title: snap.game.clone(),
            hash,
            total: snap.total,
            unlocked: snap.unlocked,
            updated: now_secs(),
        }),
        State::NoSet => Some(Summary {
            game_id: 0,
            hash,
            updated: now_secs(),
            ..Default::default()
        }),
        _ => None,
    };
    if let Some(t) = tally {
        t.save(&shared.config.data_dir, &shared.config.system, &shared.config.rom);
    }
}

/// A badge from the cache, else from the server into the cache; as a 64x64 picture.
fn badge(shared: &Shared, name: &str, url: &str) -> Option<Image> {
    let safe: String = name.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
    if safe.is_empty() {
        return None;
    }
    let path = super::dir(&shared.config.data_dir).join("badges").join(format!("{safe}.png"));
    if !path.exists() && (url.is_empty() || !shared.transport.download(url, &path)) {
        return None;
    }
    let img = crate::image::load_png(&path)?;
    Some(if (img.w, img.h) == (64, 64) { img } else { img.resized(64, 64) })
}

/// Sends unlocks earlier sessions couldn't (with the time they happened). This session's own are rc_client's to
/// send; it retries them itself.
fn send_pending(shared: &Shared) {
    for u in &shared.earlier {
        if shared.closing.load(Ordering::Acquire) {
            return;
        }
        if !shared.pending.lock().unwrap_or_else(|e| e.into_inner()).unlocks.contains(u) {
            continue;
        }
        let ago = now_secs().saturating_sub(u.when).min(u32::MAX as u64) as u32;
        match super::api::award(shared.transport.as_ref(), &shared.config.account, u.id, &u.hash, ago) {
            super::api::Award::Later => break,
            done => {
                if let super::api::Award::Refused(why) = done {
                    eprintln!("achievements: unlock {} refused: {why}", u.id);
                } else {
                    println!("achievements: sent unlock {} from an earlier session", u.id);
                }
                let mut p = shared.pending.lock().unwrap_or_else(|e| e.into_inner());
                p.remove(u.id);
                p.save(&shared.config.data_dir);
            }
        }
    }
}

/// The "achievements" thread: start-up, then files, badges and late unlocks.
fn work_loop(shared: Arc<Shared>, client: Ptr, rx: Receiver<Work>) {
    let cfg = &shared.config;
    let started = Instant::now();
    let hash = super::hash_rom(cfg.console, &cfg.rom);
    match &hash {
        Some(h) => {
            println!("achievements: {} hashed in {} ms: {h}", cfg.rom.display(), started.elapsed().as_millis());
            shared.status.lock().unwrap_or_else(|e| e.into_inner()).hash = h.clone();
        }
        None => {
            eprintln!("achievements: couldn't hash {}", cfg.rom.display());
            shared.set_state(State::Failed, "Couldn't read the game to identify it");
        }
    }
    write_files(&shared);
    if hash.is_some() && !shared.closing.load(Ordering::Acquire) {
        let (u, t) = (
            CString::new(cfg.account.username.as_str()).unwrap_or_default(),
            CString::new(cfg.account.token.as_str()).unwrap_or_default(),
        );
        unsafe { ffi::pocket_rc_begin_login_with_token(client.0, u.as_ptr(), t.as_ptr()) };
    }
    let mut load_pending = hash.clone();
    let mut memory_ready = false;
    let mut due: Option<Instant> = None;
    loop {
        let wait = due.map_or(Duration::from_secs(3600), |d| d.saturating_duration_since(Instant::now()));
        match rx.recv_timeout(wait) {
            Ok(Work::Dirty) => {
                due.get_or_insert(Instant::now() + Duration::from_secs(1));
            }
            Ok(Work::Flush) => {
                write_files(&shared);
                due = None;
            }
            Ok(Work::MemoryReady) => memory_ready = true,
            Ok(Work::SendPending) => send_pending(&shared),
            Ok(Work::Badge { name, url }) => {
                let image = badge(&shared, &name, &url);
                shared.notice(Notice::Badge { name, image });
            }
            Ok(Work::Stop) | Err(RecvTimeoutError::Disconnected) => {
                write_files(&shared);
                return;
            }
            Err(RecvTimeoutError::Timeout) => {
                write_files(&shared);
                due = None;
            }
        }
        // rc_client reads every address of the set once while loading it: only once the core's memory is there.
        if memory_ready && !shared.closing.load(Ordering::Acquire) {
            if let Some(h) = load_pending.take() {
                let h = CString::new(h).unwrap_or_default();
                unsafe { ffi::pocket_rc_begin_load_game(client.0, h.as_ptr()) };
            }
        }
    }
}

pub struct Session {
    shared: Arc<Shared>,
    client: *mut RcClient,
    memory: AchievementMemory,
    memory_ok: bool,
    frames: u64,
    net: Option<JoinHandle<()>>,
    worker: Option<JoinHandle<()>>,
}

impl Session {
    /// Starts achievements for the game when it has a console on RetroAchievements and the core may be used for it.
    /// Returns at once: everything slow happens on the session's threads.
    pub fn start(config: Config, memory: AchievementMemory, transport: Box<dyn Transport>) -> Option<Session> {
        let core = CString::new(config.core_name.as_str()).unwrap_or_default();
        if unsafe { ffi::pocket_rc_is_core_allowed(core.as_ptr(), config.console) } == 0 {
            println!(
                "achievements: RetroAchievements doesn't allow {} for console {}",
                config.core_name, config.console
            );
            return None;
        }
        let pending = Pending::load(&config.data_dir, &config.account.username);
        let earlier = pending.unlocks.clone();
        let verbose = config.verbose;
        let (job_tx, job_rx) = mpsc::channel();
        let (work_tx, work_rx) = mpsc::channel();
        let shared = Arc::new(Shared {
            config,
            transport,
            client: AtomicPtr::new(std::ptr::null_mut()),
            notices: Mutex::new(Vec::new()),
            jobs: Mutex::new(Some(job_tx)),
            work: Mutex::new(Some(work_tx)),
            outstanding: AtomicUsize::new(0),
            pending: Mutex::new(pending),
            earlier,
            status: Mutex::new(Status {
                state: State::Loading,
                message: String::new(),
                hash: String::new(),
            }),
            closing: AtomicBool::new(false),
        });
        let hooks = ffi::Hooks {
            userdata: Arc::as_ptr(&shared) as *mut c_void,
            server_call: on_server_call,
            event: on_event,
            log: on_log,
        };
        let client = unsafe { ffi::pocket_rc_create(&hooks, verbose as c_int) };
        if client.is_null() {
            return None;
        }
        shared.client.store(client, Ordering::Release);
        let mut s = Session {
            shared: shared.clone(),
            client,
            memory,
            memory_ok: false,
            frames: 0,
            net: None,
            worker: None,
        };
        s.init_memory();
        let net_shared = shared.clone();
        s.net = std::thread::Builder::new()
            .name("achievements-net".into())
            .spawn(move || net_loop(net_shared, job_rx))
            .ok();
        let ptr = Ptr(client);
        s.worker = std::thread::Builder::new()
            .name("achievements".into())
            .spawn(move || work_loop(shared, ptr, work_rx))
            .ok();
        Some(s)
    }

    /// Points rc_client at the core's memory; once it works, the worker may load the set.
    fn init_memory(&mut self) {
        let m = &self.memory;
        let bytes = unsafe {
            ffi::pocket_rc_init_memory(
                self.client,
                m.descriptors.as_ptr() as *const c_void,
                m.descriptors.len() as u32,
                m.data,
                m.size,
                self.shared.config.console,
            )
        };
        if bytes > 0 {
            self.memory_ok = true;
            println!("achievements: {} KB of the game's memory, {} descriptors", bytes / 1024, m.descriptors.len());
            self.shared.send(Work::MemoryReady);
        }
    }

    /// After every emulated frame (fast-forward: after each of them).
    pub fn frame(&mut self) {
        self.frames += 1;
        if !self.memory_ok {
            // Some cores have their memory only once the game runs (PPSSPP after its boot): try once a second.
            if self.frames.is_multiple_of(60) {
                self.init_memory();
            }
            if !self.memory_ok {
                unsafe { ffi::rc_client_idle(self.client) };
                return;
            }
        }
        unsafe { ffi::rc_client_do_frame(self.client) };
    }

    /// While no frame runs (pause, sleep, rewinding): rc_client's timers (retries, the ping) go on.
    pub fn idle(&mut self) {
        unsafe { ffi::rc_client_idle(self.client) };
    }

    /// A state was loaded (or the game rewound): achievements start again from waiting, so the loaded moment itself
    /// can't unlock anything.
    pub fn reset(&mut self) {
        unsafe { ffi::rc_client_reset(self.client) };
    }

    pub fn take_notices(&self) -> Vec<Notice> {
        std::mem::take(&mut *self.shared.notices.lock().unwrap_or_else(|e| e.into_inner()))
    }

    /// The pause menu opened: its list gets written now.
    pub fn menu_opened(&self) {
        self.shared.send(Work::Flush);
    }

    /// What the menu would show now (tests and the log).
    pub fn snapshot(&self) -> Snapshot {
        snapshot_of(&self.shared)
    }

    /// Ends the session. Waits up to `wait` for requests on their way (an unlock being sent) and the worker; whatever
    /// isn't done by then is left to the end of the process (unlocks are in `pending.json` already).
    pub fn close(mut self, wait: Duration) {
        self.shared.closing.store(true, Ordering::Release);
        self.shared.send(Work::Stop);
        let until = Instant::now() + wait;
        while (self.worker.as_ref().is_some_and(|w| !w.is_finished()) || self.shared.outstanding.load(Ordering::Acquire) > 0) && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(5));
        }
        let worker_done = self.worker.as_ref().is_none_or(|w| w.is_finished());
        if !worker_done || self.shared.outstanding.load(Ordering::Acquire) > 0 {
            eprintln!(
                "achievements: {} requests still on their way at the end",
                self.shared.outstanding.load(Ordering::Acquire)
            );
            // rc_client may still be called back: it stays, the process ends anyway.
            std::mem::forget(self);
            return;
        }
        if let Some(w) = self.worker.take() {
            let _ = w.join();
        }
        self.shared.jobs.lock().unwrap_or_else(|e| e.into_inner()).take();
        if let Some(n) = self.net.take() {
            let _ = n.join();
        }
        unsafe { ffi::pocket_rc_destroy(self.client) };
        self.shared.client.store(std::ptr::null_mut(), Ordering::Release);
        self.client = std::ptr::null_mut();
    }

    /// Waits until the network thread has nothing to do and the worker has handled what it got (tests).
    #[cfg(test)]
    fn settle(&self) {
        let until = Instant::now() + Duration::from_secs(10);
        loop {
            std::thread::sleep(Duration::from_millis(20));
            if self.shared.outstanding.load(Ordering::Acquire) == 0 || Instant::now() > until {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::achievements::api::tests::Mock;
    use crate::achievements::http::Response;
    use crate::testutil::TestDir;
    use std::ffi::c_uint;

    /// The stub core's work RAM: an NES's 2 KB, which RetroAchievements addresses from 0.
    static RAM: Mutex<[u8; 0x800]> = Mutex::new([0; 0x800]);
    static mut RAM_COPY: [u8; 0x800] = [0; 0x800];

    unsafe extern "C" fn memory_data(id: c_uint) -> *mut c_void {
        if id == 2 {
            std::ptr::addr_of_mut!(RAM_COPY) as *mut c_void
        } else {
            std::ptr::null_mut()
        }
    }

    unsafe extern "C" fn memory_size(id: c_uint) -> usize {
        if id == 2 {
            0x800
        } else {
            0
        }
    }

    /// One "frame" of the stub core: its RAM as the test set it.
    fn run_frame(s: &mut Session) {
        unsafe { std::ptr::addr_of_mut!(RAM_COPY).write(*RAM.lock().unwrap()) };
        s.frame();
    }

    fn poke(at: usize, v: u8) {
        RAM.lock().unwrap()[at] = v;
    }

    const SETS: &str = r#"{"Success":true,"GameId":1234,"Title":"Sample Game","ConsoleId":7,"ImageIconUrl":"http://server/Images/112233.png","RichPresenceGameId":1234,"RichPresencePatch":"","Sets":[{"AchievementSetId":1111,"GameId":1234,"Title":null,"Type":"core","ImageIconUrl":"http://server/Images/112233.png","Achievements":[{"ID":5501,"Title":"Ach1","Description":"Desc1","Flags":3,"Points":5,"MemAddr":"0xH0001=3_0xH0002=7","Author":"User1","BadgeName":"00234","Created":1367266583,"Modified":1376929305},{"ID":5502,"Title":"Ach2","Description":"Desc2","Flags":3,"Points":2,"MemAddr":"0xH0001=2_0x0002=9","Author":"User1","BadgeName":"00235","Created":1376970283,"Modified":1376970283}],"Leaderboards":[]}]}"#;

    fn scratch(name: &str) -> crate::testutil::TestDir {
        TestDir::new(&format!("ra-session-{name}"))
    }

    fn server(sets: &str, session: &str) -> Mock {
        Mock::new(vec![
            (
                "login2",
                Response::ok(r#"{"Success":true,"User":"Alex","Token":"ApiToken","Score":12345,"SoftcoreScore":123,"Messages":0}"#),
            ),
            ("achievementsets", Response::ok(sets.to_string())),
            ("startsession", Response::ok(session.to_string())),
            (
                "awardachievement",
                Response::ok(r#"{"Success":true,"Score":5432,"SoftcoreScore":777,"AchievementID":5501,"AchievementsRemaining":1}"#),
            ),
            ("ping", Response::ok(r#"{"Success":true}"#)),
        ])
    }

    /// The mock server, shared between the session (which owns its transport) and the test.
    struct Shared2(Arc<Mock>);
    impl Transport for Shared2 {
        fn call(&self, req: &Request) -> Response {
            self.0.call(req)
        }
    }

    fn start(dir: &std::path::Path, mock: Arc<Mock>) -> Session {
        let rom = dir.join("Game.nes");
        std::fs::write(&rom, vec![0x42u8; 0x8000]).unwrap();
        let config = Config {
            data_dir: dir.to_path_buf(),
            account: Account {
                username: "Alex".into(),
                token: "ApiToken".into(),
                display_name: String::new(),
            },
            system: "nes".into(),
            rom,
            console: 7,
            core_name: "Nestopia".into(),
            snapshot: dir.join("snapshot.json"),
            verbose: false,
        };
        let memory = AchievementMemory {
            descriptors: Vec::new(),
            data: Some(memory_data),
            size: Some(memory_size),
        };
        Session::start(config, memory, Box::new(Shared2(mock))).expect("session")
    }

    /// Runs frames until rc_client has loaded the set (its activation happens in a frame: memory is read only there).
    /// The bounds are generous (10 s): these loops end as soon as the condition holds, and inside a full gate next to
    /// compiles the mock server and rc_client's threads took longer than 2 s (2026-10-25).
    fn until_ready(s: &mut Session) -> Vec<Notice> {
        let mut notices = Vec::new();
        for _ in 0..2000 {
            run_frame(s);
            notices.extend(s.take_notices());
            if s.shared.state() != State::Loading && unsafe { ffi::rc_client_is_game_loaded(s.client) } != 0 {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        notices
    }

    static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

    #[test]
    fn a_game_signs_in_loads_its_set_and_unlocks_by_its_memory() {
        let _one = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
        *RAM.lock().unwrap() = [0; 0x800];
        let dir = scratch("unlock");
        let mock = Arc::new(server(
            SETS,
            r#"{"Success":true,"Unlocks":[{"ID":5502,"When":1234567899}],"HardcoreUnlocks":[]}"#,
        ));
        let mut s = start(&dir, mock.clone());
        let notices = until_ready(&mut s);
        assert!(notices.iter().any(|n| matches!(n, Notice::Ready { unlocked: 1, total: 2 })), "{notices:?}");
        let snap = s.snapshot();
        assert_eq!((snap.state, snap.game.as_str(), snap.unlocked, snap.total), (State::Ready, "Sample Game", 1, 2));
        assert_eq!(
            snap.achievements.iter().map(|a| (a.id, a.unlocked)).collect::<Vec<_>>(),
            vec![(5501, false), (5502, true)],
            "locked first"
        );

        // Nothing unlocks while the condition is false; then it turns true and holds for a frame.
        for _ in 0..3 {
            run_frame(&mut s);
        }
        // Badges arrive from their download thread whenever it finishes: they are no part of what a frame unlocks.
        let no_badges = |n: Vec<Notice>| n.into_iter().filter(|n| !matches!(n, Notice::Badge { .. })).collect::<Vec<_>>();
        assert!(no_badges(s.take_notices()).is_empty());
        poke(1, 3);
        poke(2, 7);
        run_frame(&mut s);
        let n = no_badges(s.take_notices());
        assert!(matches!(n.as_slice(), [Notice::Unlocked { id: 5501, points: 5, .. }]), "{n:?}");
        s.settle();
        let asked: Vec<String> = mock.asked.lock().unwrap().iter().filter_map(|r| r.post_data.clone()).collect();
        assert!(asked[0].starts_with("r=login2&u=Alex&t=ApiToken"), "{asked:?}");
        assert!(
            asked.iter().any(|p| p.starts_with("r=awardachievement&u=Alex&t=ApiToken&a=5501&h=0")),
            "{asked:?}"
        );
        assert!(!offline::Pending::path(&dir).exists(), "the server confirmed it: nothing waits");
        s.menu_opened();
        std::thread::sleep(Duration::from_millis(200));
        let menu = Snapshot::read(&dir.join("snapshot.json"), &dir.join("Game.nes")).expect("the menu's list");
        assert_eq!((menu.unlocked, menu.total, menu.points_unlocked, menu.points), (2, 2, 7, 7));
        assert_eq!(Summary::load(&dir, "nes", &dir.join("Game.nes")).unwrap().text().as_deref(), Some("2 of 2"));
        s.close(Duration::from_secs(5));
    }

    /// A server that also has badges: it writes a small PNG for every download and counts them.
    struct WithBadges(Arc<Mock>, AtomicUsize);
    impl Transport for WithBadges {
        fn call(&self, req: &Request) -> Response {
            self.0.call(req)
        }
        fn download(&self, url: &str, dest: &std::path::Path) -> bool {
            assert!(url.ends_with("/Badge/00234.png"), "{url}");
            self.1.fetch_add(1, Ordering::SeqCst);
            std::fs::create_dir_all(dest.parent().unwrap()).unwrap();
            let mut enc = png::Encoder::new(std::fs::File::create(dest).unwrap(), 2, 2);
            enc.set_color(png::ColorType::Rgb);
            enc.set_depth(png::BitDepth::Eight);
            enc.write_header()
                .unwrap()
                .write_image_data(&[255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 255])
                .unwrap();
            true
        }
    }

    #[test]
    fn an_unlock_brings_its_badge_once_and_keeps_it() {
        let _one = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
        *RAM.lock().unwrap() = [0; 0x800];
        let dir = scratch("badge");
        let mock = Arc::new(server(SETS, r#"{"Success":true,"Unlocks":[],"HardcoreUnlocks":[]}"#));
        let rom = dir.join("Game.nes");
        std::fs::write(&rom, vec![0x42u8; 0x8000]).unwrap();
        let config = Config {
            data_dir: dir.to_path_buf(),
            account: Account {
                username: "Alex".into(),
                token: "ApiToken".into(),
                display_name: String::new(),
            },
            system: "nes".into(),
            rom,
            console: 7,
            core_name: "Nestopia".into(),
            snapshot: dir.join("snapshot.json"),
            verbose: false,
        };
        let memory = AchievementMemory {
            descriptors: Vec::new(),
            data: Some(memory_data),
            size: Some(memory_size),
        };
        let transport = WithBadges(mock, AtomicUsize::new(0));
        let mut s = Session::start(config, memory, Box::new(transport)).unwrap();
        until_ready(&mut s);
        run_frame(&mut s);
        poke(1, 3);
        poke(2, 7);
        run_frame(&mut s);
        let mut badge = None;
        for _ in 0..1000 {
            for n in s.take_notices() {
                if let Notice::Badge { name, image } = n {
                    badge = Some((name, image.map(|i| (i.w, i.h, i.px[0]))));
                }
            }
            if badge.is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(badge, Some(("00234".to_string(), Some((64, 64, 0xff0000)))), "scaled to the card's size");
        assert!(dir.join("achievements/badges/00234.png").exists(), "kept for the next time");
        s.close(Duration::from_secs(5));
    }

    #[test]
    fn a_loaded_state_cannot_unlock_by_itself() {
        let _one = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
        *RAM.lock().unwrap() = [0; 0x800];
        let dir = scratch("reset");
        let mut s = start(&dir, Arc::new(server(SETS, r#"{"Success":true,"Unlocks":[],"HardcoreUnlocks":[]}"#)));
        until_ready(&mut s);
        run_frame(&mut s);
        // A state where the condition already holds is loaded: reset, and the first frames don't count.
        poke(1, 3);
        poke(2, 7);
        s.reset();
        for _ in 0..5 {
            run_frame(&mut s);
        }
        assert!(
            s.take_notices().iter().all(|n| !matches!(n, Notice::Unlocked { .. })),
            "not from loading a state"
        );
        // It has to become true in play.
        poke(1, 0);
        run_frame(&mut s);
        poke(1, 3);
        run_frame(&mut s);
        assert!(s.take_notices().iter().any(|n| matches!(n, Notice::Unlocked { id: 5501, .. })));
        s.close(Duration::from_secs(5));
    }

    #[test]
    fn offline_the_kept_set_plays_and_the_unlock_waits_for_the_network() {
        let _one = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
        *RAM.lock().unwrap() = [0; 0x800];
        let dir = scratch("offline");
        // First online, so the set is kept.
        let mut s = start(&dir, Arc::new(server(SETS, r#"{"Success":true,"Unlocks":[],"HardcoreUnlocks":[]}"#)));
        until_ready(&mut s);
        s.close(Duration::from_secs(5));
        // Then without network: the game still loads its achievements.
        let mut s = start(&dir, Arc::new(Mock::new(vec![])));
        let notices = until_ready(&mut s);
        assert!(notices.iter().any(|n| matches!(n, Notice::Ready { unlocked: 0, total: 2 })), "{notices:?}");
        run_frame(&mut s);
        poke(1, 3);
        poke(2, 7);
        run_frame(&mut s);
        assert!(
            s.take_notices().iter().any(|n| matches!(n, Notice::Unlocked { id: 5501, .. })),
            "unlocks offline too"
        );
        s.settle();
        let waiting = Pending::load(&dir, "Alex");
        assert_eq!(waiting.unlocks.iter().map(|u| u.id).collect::<Vec<_>>(), vec![5501], "kept for later");
        assert_eq!(s.snapshot().waiting, 1);
        s.close(Duration::from_millis(200));
        // The next start online sends it once, with the time it happened (rc_client doesn't send it again: until the
        // server has it, the session's unlock list counts it, see `offline::exchange`).
        *RAM.lock().unwrap() = [0; 0x800];
        let mut aged = Pending::load(&dir, "Alex");
        aged.unlocks[0].when -= 3600;
        aged.save(&dir);
        let mock = Arc::new(server(SETS, r#"{"Success":true,"Unlocks":[],"HardcoreUnlocks":[]}"#));
        let mut s = start(&dir, mock.clone());
        until_ready(&mut s);
        for _ in 0..1000 {
            run_frame(&mut s);
            std::thread::sleep(Duration::from_millis(5));
            if !Pending::path(&dir).exists() {
                break;
            }
        }
        assert!(!Pending::path(&dir).exists(), "sent");
        let awards: Vec<String> = mock
            .asked
            .lock()
            .unwrap()
            .iter()
            .filter_map(|r| r.post_data.clone())
            .filter(|p| p.starts_with("r=awardachievement"))
            .collect();
        assert_eq!(awards.len(), 1, "{awards:?}");
        assert!(awards[0].contains("a=5501") && awards[0].contains("&o=360"), "an hour ago: {awards:?}");
        s.close(Duration::from_secs(5));
    }

    #[test]
    fn an_unknown_game_and_a_refused_token_say_so() {
        let _one = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
        let dir = scratch("unknown");
        // What retroachievements.org answers for a hash it doesn't know.
        let not_found = Response {
            status: 404,
            body: br#"{"Success":false,"Error":"Unknown game","Code":"not_found","Status":404}"#.to_vec(),
        };
        let mut s = start(
            &dir,
            Arc::new(Mock::new(vec![
                ("login2", Response::ok(r#"{"Success":true,"User":"Alex","Token":"ApiToken"}"#)),
                ("achievementsets", not_found),
            ])),
        );
        for _ in 0..1000 {
            run_frame(&mut s);
            std::thread::sleep(Duration::from_millis(5));
            if s.shared.state() != State::Loading {
                break;
            }
        }
        assert_eq!(s.shared.state(), State::NoSet);
        s.close(Duration::from_secs(5));
        assert_eq!(
            Summary::load(&dir, "nes", &dir.join("Game.nes")).map(|t| t.game_id),
            Some(0),
            "known to have no set: the SD card won't ask again"
        );

        let refused = Mock::new(vec![(
            "login2",
            Response {
                status: 401,
                body: br#"{"Success":false,"Error":"Invalid token","Code":"invalid_credentials"}"#.to_vec(),
            },
        )]);
        let mut s = start(&dir, Arc::new(refused));
        let mut told = false;
        for _ in 0..1000 {
            run_frame(&mut s);
            told |= s.take_notices().iter().any(|n| matches!(n, Notice::SignInAgain));
            std::thread::sleep(Duration::from_millis(5));
            if told {
                break;
            }
        }
        assert!(told);
        assert_eq!(s.shared.state(), State::SignedOut);
        s.close(Duration::from_secs(5));
    }
}
