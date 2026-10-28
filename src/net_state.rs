//! One picture of the Wi-Fi for every screen that shows it. The quick settings' Wi-Fi row and Settings → Connections →
//! Wi-Fi used to keep a cache each (rfkill on one side, the last scan on the other), the switch stored `wifi.enabled`
//! whether or not the radio really changed, and a scan that failed was shown as "No networks found" (review of the
//! 2026-10-26 incident, `REVIEW-SOL-INCIDENT`, section 3).
//!
//! - [`Snapshot`] is what both read, from memory only: the stored wish (`wifi.enabled`), the radio as it really is
//!   (rfkill soft and hard block, NetworkManager's own radio), the address, the saved networks, the last scan with its
//!   typed error, and a [`Notice`] when a switch did not work.
//! - One serialised worker does everything that changes the radio or the connection (a switch, a connect, a scan, a
//!   forget); two of those never run at once. A **generation** (`epoch`) goes up when such an operation is asked for
//!   and again when it is done; a look at the system, or a scan, that started before is stale and dropped. A scan that
//!   a newer scan replaced is dropped too.
//! - The wish is stored only after the radio has been read back in the state that was asked for. A switch that did
//!   not work leaves the wish as it was, shows what the radio really does, and leaves a [`Notice`].
//! - A switch that works but whose wish cannot be stored is not silent: a [`NoticeKind::Save`] notice, and the wish is
//!   stored at the next look.
//! - A radio that is off is not a failed scan and not an empty one: [`Scan::NotYet`] with `Radio::Off`.
//! - Whoever shows the snapshot subscribes ([`NetState::subscribe`]) and is called after every change, from the
//!   worker's thread; it only has to wake its screen. When the radio comes up after being off (boot hooks, resume), or
//!   a scan fails, one more scan follows by itself after a few seconds ([`RETRY_AFTER`]).
//!
//! Nothing here runs on the UI thread except reading a snapshot. The player's own choice decides whether the radio is
//! switched on by itself: only a radio that the system blocked (rfkill soft: the boot and resume hooks) while
//! `wifi.enabled` says on is switched on, once per block and not again within `UNBLOCK_GAP` (logged); with the wish off,
//! after a switch off by the player, for a hard block or NetworkManager's own switch, nothing is done. When the unblock
//! does not work the page says "Wi-Fi is off" and A tries. A switch (also the one by the player) goes ahead of the
//! scans that wait and stops the one under way; connections still run one after the other. The reconnect watchdog in
//! `wifi` does not switch the radio.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, Weak};
use std::time::Duration;

use crate::system_ctl::SharedHw;
use crate::wifi::{Network, Saving};

/// Why a radio is off.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cause {
    /// `rfkill block wifi`: the boot and resume hooks, `wifictl disable`, this switch.
    Soft,
    /// A hardware switch.
    Hard,
    /// rfkill lets it through but NetworkManager's own radio switch is off (`nmcli radio wifi off`).
    Manager,
}

/// The Wi-Fi radio as the system really has it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Radio {
    /// Not looked at yet.
    #[default]
    Unknown,
    /// This machine has no Wi-Fi radio.
    Absent,
    On,
    Off(Cause),
}

impl Radio {
    pub fn is_on(self) -> bool {
        self == Radio::On
    }

    pub fn is_off(self) -> bool {
        matches!(self, Radio::Off(_))
    }

    /// What a switch shows: `None` where there is no radio, or nothing has been read yet.
    pub fn switch(self) -> Option<bool> {
        match self {
            Radio::On => Some(true),
            Radio::Off(_) => Some(false),
            Radio::Unknown | Radio::Absent => None,
        }
    }
}

/// Why a scan gave no answer: the program did not run, did not finish, or said no (`nmcli` exit code and its last line).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScanError {
    pub code: Option<i32>,
    pub detail: String,
}

impl std::fmt::Display for ScanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.code {
            Some(c) => write!(f, "exit {c}: {}", self.detail),
            None => f.write_str(&self.detail),
        }
    }
}

/// The last scan.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Scan {
    /// None yet, or the radio went off since: nothing to list.
    #[default]
    NotYet,
    /// A scan that worked. The list may be empty: then nobody is in reach.
    Found(Vec<Network>),
    Failed(ScanError),
}

/// What went wrong with a switch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoticeKind {
    /// The radio did not change.
    Switch,
    /// The radio changed, but `wifi.enabled` could not be stored (it is tried again at every look until it works): the
    /// next start would otherwise undo the choice without a word.
    Save,
}

/// A radio switch that did not work: `on` is what was asked for. The reason is for the log, not for the screen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notice {
    /// Goes up with every notice; a screen shows each once.
    pub id: u64,
    pub on: bool,
    pub kind: NoticeKind,
    pub why: String,
}

impl Notice {
    /// What the screens say. `page`: the Wi-Fi page, where A is the way to try again.
    pub fn line(&self, page: bool) -> &'static str {
        match (self.kind, self.on, page) {
            (NoticeKind::Switch, true, true) => "Couldn't turn Wi-Fi on. Press A to try again.",
            (NoticeKind::Switch, true, false) => "Couldn't turn Wi-Fi on. Try again.",
            (NoticeKind::Switch, false, _) => "Couldn't turn Wi-Fi off. Try again.",
            (NoticeKind::Save, true, _) => "Wi-Fi is on, but the setting couldn't be saved yet.",
            (NoticeKind::Save, false, _) => "Wi-Fi is off, but the setting couldn't be saved yet.",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Snapshot {
    /// Goes up with every change.
    pub rev: u64,
    /// `wifi.enabled` in system.cfg: what the player asked for (None: not read yet).
    pub wanted: Option<bool>,
    pub radio: Radio,
    /// A switch is under way (the state it is going to).
    pub switching: Option<bool>,
    /// A scan is asked for or running.
    pub scanning: bool,
    /// The saved networks, the country, the address of the Wi-Fi interface (None: none, as `wifi::ip`).
    pub saving: Saving,
    /// zoneinfo's `iso3166.tab`, for the country names (read with the scan, so painting never reads a file).
    pub iso_tab: String,
    pub scan: Scan,
    /// Goes up whenever `scan` was replaced.
    pub scan_rev: u64,
    pub notice: Option<Notice>,
}

impl Snapshot {
    /// The network in use, from the saved profile that is active.
    pub fn connected(&self) -> Option<&str> {
        self.saving.saved.iter().find(|s| s.active).map(|s| s.ssid.as_str())
    }
}

/// What the state needs from the system. The real one is [`HwNet`]; tests bring their own.
pub trait NetOps: Send + Sync {
    /// rfkill and NetworkManager's radio. A look that must not wait long.
    fn radio(&self) -> Radio;
    /// Switches the radio and waits until it has really changed, or says why not. Does not store anything.
    fn set_radio(&self, on: bool) -> Result<(), String>;
    /// `wifi.enabled`.
    fn wanted(&self) -> Option<bool>;
    /// Stores `wifi.enabled`; false when that did not work.
    fn persist_wanted(&self, on: bool) -> bool;
    /// The saved networks, the country, the address, and zoneinfo's `iso3166.tab`. Gives up early when `stop` is set.
    fn saving(&self, stop: &AtomicBool) -> (Saving, String);
    /// The address of the Wi-Fi interface now.
    fn address(&self) -> Option<String>;
    /// Scans; when `stop` is set (a switch of the radio was asked for) the program is killed and the answer is an error
    /// nobody reads.
    fn scan(&self, stop: &AtomicBool) -> Result<Vec<Network>, ScanError>;
}

/// The system's own: the radio through `Hw` (rfkill, `wifictl`, `nmcli radio`), the rest through `nmcli`.
pub struct HwNet {
    hw: SharedHw,
    nmcli: std::path::PathBuf,
}

impl HwNet {
    pub fn new(hw: SharedHw) -> HwNet {
        HwNet { hw, nmcli: "nmcli".into() }
    }

    /// With another `nmcli` (a stand-in script in tests).
    pub fn with_nmcli(mut self, nmcli: impl Into<std::path::PathBuf>) -> HwNet {
        self.nmcli = nmcli.into();
        self
    }
}

impl NetOps for HwNet {
    fn radio(&self) -> Radio {
        self.hw.wifi_radio()
    }
    fn set_radio(&self, on: bool) -> Result<(), String> {
        self.hw.set_wifi(on)
    }
    fn wanted(&self) -> Option<bool> {
        self.hw.wifi_wanted()
    }
    fn persist_wanted(&self, on: bool) -> bool {
        self.hw.persist_wifi(on)
    }
    fn saving(&self, stop: &AtomicBool) -> (Saving, String) {
        (crate::wifi::saving_until(stop), self.hw.zoneinfo_file("iso3166.tab").unwrap_or_default())
    }
    /// From memory (`wifi::ip`: the watchdog looks every 15 s; a look older than 3 s starts one on a thread): this runs at
    /// every look at the radio, which quick settings take every 2 s while open.
    fn address(&self) -> Option<String> {
        crate::wifi::ip()
    }
    fn scan(&self, stop: &AtomicBool) -> Result<Vec<Network>, ScanError> {
        crate::wifi::scan_until(&self.nmcli, stop)
    }
}

/// How long after a scan that found nothing or failed, while the radio is up, the one automatic retry waits (a radio
/// that has only just come up often has no list yet).
pub const RETRY_AFTER: Duration = Duration::from_secs(5);

type Job = Box<dyn FnOnce() + Send>;

/// Runs something a while from now: a thread that waits, or (inline, tests) at once, or when a test says so.
type Later = Arc<dyn Fn(Duration, Job) + Send + Sync>;

fn later_on_a_thread() -> Later {
    Arc::new(|wait, job| {
        let spawned = std::thread::Builder::new().name("net-retry".into()).spawn(move || {
            std::thread::sleep(wait);
            job();
        });
        if spawned.is_err() {
            eprintln!("net: the retry thread did not start");
        }
    })
}

/// One thread, one job at a time. A job with a key replaces a waiting one with the same key.
struct Lane {
    inner: Mutex<LaneInner>,
    cv: Condvar,
}

struct LaneInner {
    jobs: VecDeque<(&'static str, Job)>,
    running: bool,
    closed: bool,
    thread: bool,
}

fn locked<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl Lane {
    /// The thread starts with the first job: a state nobody asks anything of costs nothing.
    fn start() -> Arc<Lane> {
        Arc::new(Lane { inner: Mutex::new(LaneInner { jobs: VecDeque::new(), running: false, closed: false, thread: false }), cv: Condvar::new() })
    }

    fn run_loop(mine: Arc<Lane>) {
        loop {
            let job = {
                let mut g = locked(&mine.inner);
                loop {
                    if let Some((_, job)) = g.jobs.pop_front() {
                        g.running = true;
                        break job;
                    }
                    if g.closed {
                        return;
                    }
                    g = mine.cv.wait(g).unwrap_or_else(|e| e.into_inner());
                }
            };
            // One job that panics must not end the worker for the rest of the run (`NetState::run` also puts the
            // snapshot right again).
            if std::panic::catch_unwind(std::panic::AssertUnwindSafe(job)).is_err() {
                eprintln!("net: a job panicked; the worker goes on");
            }
            locked(&mine.inner).running = false;
            mine.cv.notify_all();
        }
    }

    /// `ahead_of_scans`: before the scans that are waiting (a switch of the radio need not wait for a list that its own
    /// outcome makes useless), behind everything else.
    fn submit_at(self: &Arc<Self>, key: &'static str, job: Job, ahead_of_scans: bool) {
        let mut g = locked(&self.inner);
        if !key.is_empty() {
            g.jobs.retain(|(k, _)| *k != key);
        }
        let at = if ahead_of_scans { g.jobs.iter().position(|(k, _)| *k == "scan").unwrap_or(g.jobs.len()) } else { g.jobs.len() };
        g.jobs.insert(at, (key, job));
        if !g.thread {
            g.thread = true;
            let mine = self.clone();
            if std::thread::Builder::new().name("net-state".into()).spawn(move || Lane::run_loop(mine)).is_err() {
                eprintln!("net: the worker thread did not start");
                g.thread = false;
            }
        }
        self.cv.notify_all();
    }

    fn close(&self) {
        locked(&self.inner).closed = true;
        self.cv.notify_all();
    }

    #[cfg(test)]
    fn wait_idle(&self, timeout: Duration) -> bool {
        let until = std::time::Instant::now() + timeout;
        let mut g = locked(&self.inner);
        while !g.jobs.is_empty() || g.running {
            let left = until.saturating_duration_since(std::time::Instant::now());
            if left.is_zero() {
                return false;
            }
            g = self.cv.wait_timeout(g, left).unwrap_or_else(|e| e.into_inner()).0;
        }
        true
    }
}

struct Inner {
    snap: Snapshot,
    /// The generation: up when a change of the radio or the connection is asked for, and again when it is done.
    epoch: u64,
    /// The newest scan asked for.
    ticket: u64,
    /// Somebody asked for a scan at some time: a radio that comes up scans by itself.
    scans_wanted: bool,
    /// Automatic retries left until the radio next comes up.
    retry_left: u8,
    notices: u64,
    /// The scan under way (or the last one): set when a switch of the radio is asked for, so that it does not have to wait.
    scan_stop: Arc<AtomicBool>,
    /// Switches of the radio asked for and not done: a scan that begins now is stopped at once.
    switches_waiting: u32,
    /// A switch worked but the wish could not be stored: the state to store at the next look.
    pending_wish: Option<bool>,
    /// A radio that the boot or resume hooks blocked while the wish says on was switched on once since it last was on.
    unblock_tried: bool,
    /// When that was tried last: not again within `UNBLOCK_GAP`.
    unblock_at: Option<std::time::Instant>,
}

/// Something else that blocks the radio again right after it was released is left alone for this long.
const UNBLOCK_GAP: Duration = Duration::from_secs(30);

pub struct NetState {
    ops: Arc<dyn NetOps>,
    /// None: jobs run on the calling thread (tests, snapshots).
    lane: Option<Arc<Lane>>,
    retry_after: Duration,
    later: Later,
    inner: Mutex<Inner>,
    subs: Mutex<Vec<Weak<dyn Fn() + Send + Sync>>>,
    /// A snapshot shows a made-up state: nothing is looked at or done.
    frozen: AtomicBool,
}

impl Drop for NetState {
    fn drop(&mut self) {
        if let Some(lane) = &self.lane {
            lane.close();
        }
    }
}

impl NetState {
    fn build(ops: Arc<dyn NetOps>, threaded: bool, retry_after: Duration, later: Later) -> Arc<NetState> {
        Arc::new(NetState {
            ops,
            lane: threaded.then(Lane::start),
            retry_after,
            later,
            inner: Mutex::new(Inner {
                snap: Snapshot::default(),
                epoch: 0,
                ticket: 0,
                scans_wanted: false,
                retry_left: 1,
                notices: 0,
                scan_stop: Arc::new(AtomicBool::new(false)),
                switches_waiting: 0,
                pending_wish: None,
                unblock_tried: false,
                unblock_at: None,
            }),
            subs: Mutex::new(Vec::new()),
            frozen: AtomicBool::new(false),
        })
    }

    /// The state of this device, with its worker thread.
    pub fn new(ops: Arc<dyn NetOps>) -> Arc<NetState> {
        Self::build(ops, true, RETRY_AFTER, later_on_a_thread())
    }

    /// Jobs run where they are asked for, one after the other, and the automatic retry follows at once: tests.
    pub fn inline(ops: Arc<dyn NetOps>) -> Arc<NetState> {
        Self::build(ops, false, Duration::ZERO, later_on_a_thread())
    }

    /// With a worker thread, and the retry's wait in the hands of `later` (a test runs what it is given when it likes).
    #[cfg(test)]
    pub(crate) fn with_later(ops: Arc<dyn NetOps>, later: Later) -> Arc<NetState> {
        Self::build(ops, true, RETRY_AFTER, later)
    }

    /// A made-up state that stays as it is (`--snapshot`): nothing is looked at, asked for or done.
    pub fn demo(snap: Snapshot) -> Arc<NetState> {
        let s = Self::build(Arc::new(Inert), false, Duration::ZERO, later_on_a_thread());
        locked(&s.inner).snap = snap;
        s.frozen.store(true, Ordering::SeqCst);
        s
    }

    /// What the system is like, as last found out. Memory only.
    pub fn snapshot(&self) -> Snapshot {
        locked(&self.inner).snap.clone()
    }

    /// `f` is called after every change, from whatever thread made it. Held weakly: it ends when the caller drops `f`.
    pub fn subscribe(&self, f: &Arc<dyn Fn() + Send + Sync>) {
        let mut subs = locked(&self.subs);
        subs.retain(|w| w.strong_count() > 0);
        subs.push(Arc::downgrade(f));
    }

    fn tell(&self) {
        let calls: Vec<Arc<dyn Fn() + Send + Sync>> = {
            let mut subs = locked(&self.subs);
            subs.retain(|w| w.strong_count() > 0);
            subs.iter().filter_map(Weak::upgrade).collect()
        };
        for f in calls {
            f();
        }
    }

    /// Changes the snapshot under the lock if `epoch` is still the generation asked for (None: whatever it is now);
    /// tells the subscribers when something differs. False when it was stale and nothing changed.
    fn commit(&self, epoch: Option<u64>, change: impl FnOnce(&mut Inner)) -> bool {
        let moved = {
            let mut g = locked(&self.inner);
            if epoch.is_some_and(|e| e != g.epoch) {
                return false;
            }
            let before = g.snap.clone();
            change(&mut g);
            g.snap.rev = before.rev;
            let moved = g.snap != before;
            if moved {
                g.snap.rev += 1;
            }
            moved
        };
        if moved {
            self.tell();
        }
        true
    }

    fn epoch(&self) -> u64 {
        locked(&self.inner).epoch
    }

    fn run(self: &Arc<Self>, key: &'static str, job: impl FnOnce() + Send + 'static) {
        self.run_at(key, false, job);
    }

    /// A job on the worker. If it panics, the worker goes on and the snapshot no longer says a switch or scan is under way.
    fn run_at(self: &Arc<Self>, key: &'static str, ahead_of_scans: bool, job: impl FnOnce() + Send + 'static) {
        match &self.lane {
            Some(lane) => {
                let me = self.clone();
                lane.submit_at(
                    key,
                    Box::new(move || {
                        if std::panic::catch_unwind(std::panic::AssertUnwindSafe(job)).is_err() {
                            eprintln!("net: a job ({key:?}) panicked; Wi-Fi is looked at again");
                            locked(&me.inner).epoch += 1;
                            locked(&me.inner).switches_waiting = 0;
                            me.commit(None, |g| {
                                g.snap.switching = None;
                                g.snap.scanning = false;
                            });
                        }
                    }),
                    ahead_of_scans,
                );
            }
            None => job(),
        }
    }

    /// A switch of the radio on the worker, ahead of the scans that wait: the scan under way is stopped, and one that
    /// begins before the switch is done stops at once.
    /// The job says whether a scan should follow (asked for once the switch counts as done).
    fn run_switch(self: &Arc<Self>, job: impl FnOnce() -> bool + Send + 'static) {
        {
            let mut g = locked(&self.inner);
            g.epoch += 1;
            g.switches_waiting += 1;
            g.scan_stop.store(true, Ordering::SeqCst);
        }
        let me = self.clone();
        self.run_at("", true, move || {
            let scan = job();
            {
                let mut g = locked(&me.inner);
                g.switches_waiting = g.switches_waiting.saturating_sub(1);
            }
            if scan {
                me.request_scan();
            }
        });
    }

    /// Kills the scan under way (and the saving it begins with): a switch or a connection is on its way and its answer
    /// would be dropped anyway.
    fn abort_scan(&self) {
        locked(&self.inner).scan_stop.store(true, Ordering::SeqCst);
    }

    // -----------------------------------------------------------------------------------------------------
    // Looking

    /// Looks at the radio, the stored wish and the address again. On a thread that may wait, never the UI's. Dropped
    /// if a switch, connect or scan was asked for since it started, and while a switch is under way. A wish that could
    /// not be stored after a switch is stored now.
    pub fn poll(self: &Arc<Self>) {
        if self.frozen.load(Ordering::SeqCst) {
            return;
        }
        let epoch = {
            let g = locked(&self.inner);
            if g.snap.switching.is_some() {
                return;
            }
            g.epoch
        };
        self.store_pending_wish();
        let radio = self.ops.radio();
        let wanted = self.ops.wanted();
        let address = if radio.is_on() { self.ops.address() } else { None };
        // A network NetworkManager is still bringing up: its saved list is read again too (nmcli, this thread), so the page
        // turns to "Connected" once it is activated, or back once it gave up, without waiting for the next scan.
        let connecting = radio.is_on() && locked(&self.inner).snap.saving.saved.iter().any(|s| s.connecting);
        self.take_look(epoch, radio, wanted, Some(address), true);
        if connecting {
            let (saving, _) = self.ops.saving(&AtomicBool::new(false));
            self.commit(Some(epoch), |g| g.snap.saving = saving);
        }
    }

    /// The wish of a switch that worked but could not be stored, tried again; the notice goes with it.
    fn store_pending_wish(&self) {
        let Some(on) = locked(&self.inner).pending_wish else { return };
        if !self.ops.persist_wanted(on) {
            return;
        }
        println!("net: wifi.enabled stored after all");
        self.commit(None, |g| {
            g.pending_wish = None;
            g.snap.wanted = Some(on);
            if g.snap.notice.as_ref().is_some_and(|n| n.kind == NoticeKind::Save) {
                g.snap.notice = None;
            }
        });
    }

    /// Puts a look into the snapshot (unless stale). A radio that goes on or off since the last look makes every scan and
    /// look under way stale (an external block, the boot hooks, EmulationStation). `follow`: a radio that came up scans
    /// once if somebody wanted a list (not from a scan's own look: it is the scan). A radio the hooks blocked while the
    /// wish says on is switched on once.
    fn take_look(self: &Arc<Self>, epoch: u64, radio: Radio, wanted: Option<bool>, address: Option<Option<String>>, follow: bool) {
        let mut came_up = false;
        let mut unblock = false;
        let kept = self.commit(Some(epoch), |g| {
            came_up = radio.is_on() && !matches!(g.snap.radio, Radio::On);
            if g.snap.radio.is_on() != radio.is_on() && !matches!(g.snap.radio, Radio::Unknown) {
                g.epoch += 1;
            }
            g.snap.radio = radio;
            if wanted.is_some() {
                g.snap.wanted = wanted;
            }
            if let Some(ip) = address {
                g.snap.saving.ip = ip;
            }
            if radio.is_off() && !matches!(g.snap.scan, Scan::NotYet) {
                g.snap.scan = Scan::NotYet;
                g.snap.scan_rev += 1;
            }
            if came_up {
                g.retry_left = 1;
                g.unblock_tried = false;
            }
            // The player's own choice, switched on again: only for a block by the system (rfkill soft), never when the
            // wish is off (or an off that could not be stored yet), once per block, not again within `UNBLOCK_GAP`.
            let wish = g.pending_wish.or(g.snap.wanted);
            let gap_over = g.unblock_at.is_none_or(|at| at.elapsed() >= UNBLOCK_GAP);
            if radio == Radio::Off(Cause::Soft) && wish == Some(true) && !g.unblock_tried && gap_over && g.snap.switching.is_none() {
                g.unblock_tried = true;
                g.unblock_at = Some(std::time::Instant::now());
                unblock = true;
            }
        });
        if kept && unblock {
            self.unblock();
        }
        if follow && kept && came_up && locked(&self.inner).scans_wanted {
            println!("net: the radio is up; looking for networks");
            self.request_scan();
        }
    }

    /// The radio is blocked (boot or resume hooks) and the player's wish is on: switch it on, once, and say so in the log.
    /// No notice when it does not work: the page says "Wi-Fi is off" and A tries again.
    fn unblock(self: &Arc<Self>) {
        println!("net: Wi-Fi is blocked while wifi.enabled is on; switching it on once");
        self.commit(None, |g| g.snap.switching = Some(true));
        let me = self.clone();
        self.run_switch(move || {
            let done = me.ops.set_radio(true);
            let radio = me.ops.radio();
            match &done {
                Ok(()) if radio.is_on() => println!("net: Wi-Fi switched on"),
                Ok(()) => println!("net: Wi-Fi is still {radio:?} after switching it on"),
                Err(e) => println!("net: switching Wi-Fi on did not work: {e}"),
            }
            locked(&me.inner).epoch += 1;
            me.commit(None, |g| {
                g.snap.radio = radio;
                g.snap.switching = None;
            });
            radio.is_on()
        });
    }

    // -----------------------------------------------------------------------------------------------------
    // Doing

    /// Asks for a scan. A scan waiting for its turn is replaced by this one. `scanning` is up at once.
    pub fn request_scan(self: &Arc<Self>) {
        if self.frozen.load(Ordering::SeqCst) {
            return;
        }
        let ticket = {
            let mut g = locked(&self.inner);
            g.ticket += 1;
            g.scans_wanted = true;
            g.ticket
        };
        self.commit(None, |g| g.snap.scanning = true);
        let me = self.clone();
        self.run("scan", move || me.scan_job(ticket));
    }

    fn scan_job(self: &Arc<Self>, ticket: u64) {
        let newest = |g: &Inner| g.ticket == ticket;
        let stop = Arc::new(AtomicBool::new(false));
        let mut epoch = {
            let mut g = locked(&self.inner);
            if g.switches_waiting > 0 {
                stop.store(true, Ordering::SeqCst);
            }
            g.scan_stop = stop.clone();
            g.epoch
        };
        let radio = self.ops.radio();
        let wanted = self.ops.wanted();
        self.take_look(epoch, radio, wanted, None, false);
        // A look that found the radio gone on or off has moved the generation: this scan starts from there.
        epoch = self.epoch();
        let give_up = |me: &Arc<Self>, why: &str| {
            println!("net: scan {ticket}: {why}");
            me.commit(None, |g| {
                if newest(g) {
                    g.snap.scanning = false;
                }
            });
        };
        if !radio.is_on() {
            // Nothing to search with: no error, no empty list, the page says the radio is off.
            give_up(self, &format!("radio is {radio:?}; not searching"));
            return;
        }
        let (saving, iso_tab) = self.ops.saving(&stop);
        if stop.load(Ordering::SeqCst) {
            give_up(self, "stopped for a switch");
            return;
        }
        self.commit(Some(epoch), |g| {
            g.snap.saving = saving;
            if !iso_tab.is_empty() {
                g.snap.iso_tab = iso_tab;
            }
        });
        let result = self.ops.scan(&stop);
        if stop.load(Ordering::SeqCst) {
            give_up(self, "stopped for a switch");
            return;
        }
        match &result {
            Ok(nets) => println!("net: scan {ticket}: {} network(s)", nets.len()),
            Err(e) => println!("net: scan {ticket} failed ({e})"),
        }
        let mut retry = false;
        let kept = self.commit(Some(epoch), |g| {
            let nothing = match &result {
                Ok(nets) => nets.is_empty(),
                Err(_) => true,
            };
            if newest(g) {
                g.snap.scanning = false;
                if nothing && g.retry_left > 0 {
                    g.retry_left -= 1;
                    retry = true;
                }
            }
            if newest(g) {
                g.snap.scan = match result.clone() {
                    Ok(nets) => Scan::Found(nets),
                    Err(e) => Scan::Failed(e),
                };
                g.snap.scan_rev += 1;
            }
        });
        if !kept {
            // A switch or connection was asked for while this one ran: its result describes a radio that has changed.
            give_up(self, "dropped (stale)");
        } else if retry {
            self.retry_scan(ticket);
        }
    }

    /// The one automatic scan after one that found nothing, if nobody scanned in the meantime.
    fn retry_scan(self: &Arc<Self>, after_ticket: u64) {
        if self.retry_after.is_zero() {
            self.request_scan();
            return;
        }
        let me = self.clone();
        (self.later)(
            self.retry_after,
            Box::new(move || {
                if locked(&me.inner).ticket == after_ticket {
                    println!("net: looking for networks once more");
                    me.request_scan();
                }
            }),
        );
    }

    /// Switches the radio on or off. The stored wish follows only when the radio has been read back in the state asked
    /// for; otherwise the snapshot keeps the wish, shows what the radio does, and carries a [`Notice`]. Turned on, a
    /// scan follows. Several asks in a row run one after the other, each to its own state.
    pub fn set_enabled(self: &Arc<Self>, on: bool) {
        if self.frozen.load(Ordering::SeqCst) {
            return;
        }
        self.commit(None, |g| {
            g.snap.switching = Some(on);
            g.snap.notice = None;
        });
        let me = self.clone();
        self.run_switch(move || me.switch_job(on));
    }

    /// Switches the radio to the opposite of what it is **now**: read fresh by the worker, when it is this switch's turn
    /// (a row that shows the state of two seconds ago can be wrong; the boot hooks block and unblock the radio behind
    /// everybody's back). Does nothing while a switch is under way or where there is no radio. The row's label says
    /// the opposite of what the picture shows until the fresh read corrects it.
    pub fn toggle(self: &Arc<Self>) {
        if self.frozen.load(Ordering::SeqCst) {
            return;
        }
        let guess = {
            let g = locked(&self.inner);
            if g.snap.switching.is_some() {
                return;
            }
            !g.snap.radio.is_on()
        };
        self.commit(None, |g| {
            g.snap.switching = Some(guess);
            g.snap.notice = None;
        });
        let me = self.clone();
        self.run_switch(move || {
            let now = me.ops.radio();
            match now.switch() {
                Some(on) => {
                    if guess == on {
                        me.commit(None, |g| g.snap.switching = Some(!on));
                    }
                    me.switch_job(!on)
                }
                None => {
                    println!("net: nothing to switch: the radio is {now:?}");
                    locked(&me.inner).epoch += 1;
                    me.commit(None, |g| {
                        g.snap.radio = now;
                        g.snap.switching = None;
                    });
                    false
                }
            }
        });
    }

    /// Does the switch; true when a scan should follow (it was switched on).
    fn switch_job(self: &Arc<Self>, on: bool) -> bool {
        println!("net: switching Wi-Fi {}", if on { "on" } else { "off" });
        let done = self.ops.set_radio(on);
        let radio = self.ops.radio();
        let reached = if on { radio.is_on() } else { radio.is_off() };
        let verdict = match (done, reached) {
            (Ok(()), true) => Ok(()),
            (Err(e), _) => Err(e),
            (Ok(()), false) => Err(format!("the radio is {radio:?} after switching it {}", if on { "on" } else { "off" })),
        };
        match verdict {
            Ok(()) => {
                // Stored only now, and twice if need be; a wish that cannot be stored is kept to try again at every look,
                // with a notice, never silently (the next start would undo the choice).
                let stored = self.ops.persist_wanted(on) || self.ops.persist_wanted(on);
                if !stored {
                    println!("net: Wi-Fi is {} but wifi.enabled could not be stored; trying again later", if on { "on" } else { "off" });
                }
                let wanted = if stored { Some(on) } else { self.ops.wanted() };
                {
                    let mut g = locked(&self.inner);
                    g.epoch += 1;
                    g.pending_wish = (!stored).then_some(on);
                    if !stored {
                        g.notices += 1;
                    }
                }
                self.commit(None, |g| {
                    g.snap.radio = radio;
                    g.snap.wanted = wanted;
                    g.snap.switching = None;
                    g.snap.notice = (!stored).then(|| Notice { id: g.notices, on, kind: NoticeKind::Save, why: "wifi.enabled could not be stored".into() });
                    if on {
                        g.retry_left = 1;
                        g.unblock_tried = false;
                    } else {
                        g.snap.saving.ip = None;
                        g.snap.scanning = false;
                        if !matches!(g.snap.scan, Scan::NotYet) {
                            g.snap.scan = Scan::NotYet;
                            g.snap.scan_rev += 1;
                        }
                    }
                });
                // The list in memory is from before; look again.
                on
            }
            Err(why) => {
                println!("net: switching Wi-Fi {} did not work: {why}", if on { "on" } else { "off" });
                let wanted = self.ops.wanted();
                {
                    let mut g = locked(&self.inner);
                    g.epoch += 1;
                    g.notices += 1;
                }
                self.commit(None, |g| {
                    g.snap.radio = radio;
                    g.snap.wanted = wanted;
                    g.snap.switching = None;
                    g.snap.notice = Some(Notice { id: g.notices, on, kind: NoticeKind::Switch, why });
                });
                false
            }
        }
    }

    /// Runs `job` after everything asked for before, and before anything asked for after: a connect, a forget, a change
    /// of country. The generation goes up before and after, so looks that straddle it are dropped; a scan follows.
    pub fn exclusive(self: &Arc<Self>, what: &'static str, job: impl FnOnce() + Send + 'static) {
        if self.frozen.load(Ordering::SeqCst) {
            return;
        }
        locked(&self.inner).epoch += 1;
        self.abort_scan();
        let me = self.clone();
        self.run("", move || {
            println!("net: {what}");
            job();
            locked(&me.inner).epoch += 1;
            println!("net: {what}: done");
            me.request_scan();
        });
    }

    /// A profile that was just deleted leaves the picture at once: the page must not offer it for another forget (or a
    /// connect) until the scan that follows has read the list again (a second Y on the old row, 2026-10-27).
    pub fn forgotten(&self, uuid: &str) {
        self.commit(None, |g| g.snap.saving.saved.retain(|s| s.uuid != uuid));
    }

    /// Background reconnects share the lane but yield to a newer player operation. Their scan can be stopped by
    /// a switch/connect, just like a page scan; a slow watchdog scan must not hold a radio switch for 40 seconds.
    pub fn reconnect(self: &Arc<Self>, job: impl FnOnce(Arc<AtomicBool>) + Send + 'static) {
        if self.frozen.load(Ordering::SeqCst) { return; }
        let epoch = self.epoch();
        let me = self.clone();
        self.run("reconnect", move || {
            let stop = {
                let mut g = locked(&me.inner);
                if g.epoch != epoch || g.switches_waiting > 0 { return; }
                g.epoch += 1;
                g.scan_stop = Arc::new(AtomicBool::new(false));
                g.scan_stop.clone()
            };
            job(stop);
            locked(&me.inner).epoch += 1;
            me.request_scan();
        });
    }

    /// Tests: waits until the worker has nothing left to do. A job asks for the next one before it ends, so nothing
    /// queued is missed; the delayed retry is not a job until its delay is over.
    #[cfg(test)]
    pub(crate) fn wait_idle(&self) {
        if let Some(lane) = &self.lane {
            assert!(lane.wait_idle(Duration::from_secs(30)), "the net worker did not finish");
        }
    }
}

/// The operations of a frozen state: none.
struct Inert;

impl NetOps for Inert {
    fn radio(&self) -> Radio {
        Radio::Unknown
    }
    fn set_radio(&self, _on: bool) -> Result<(), String> {
        Err("inert".into())
    }
    fn wanted(&self) -> Option<bool> {
        None
    }
    fn persist_wanted(&self, _on: bool) -> bool {
        false
    }
    fn saving(&self, _stop: &AtomicBool) -> (Saving, String) {
        (Saving::default(), String::new())
    }
    fn address(&self) -> Option<String> {
        None
    }
    fn scan(&self, _stop: &AtomicBool) -> Result<Vec<Network>, ScanError> {
        Err(ScanError { code: None, detail: "inert".into() })
    }
}

/// The ready-made states of the snapshots and tests.
pub mod demo {
    use super::*;

    /// A radio that is off while the player's wish says on (the boot hooks blocked it): "Wi-Fi is off".
    pub fn off() -> Snapshot {
        Snapshot { wanted: Some(true), radio: Radio::Off(Cause::Soft), ..Snapshot::default() }
    }

    /// The switch did not work: the radio is still off, the notice says it.
    pub fn switch_failed() -> Snapshot {
        Snapshot { notice: Some(Notice { id: 1, on: true, kind: NoticeKind::Switch, why: "wifictl enable exited with 1".into() }), ..off() }
    }

    /// The radio is on, but `wifi.enabled` could not be stored.
    pub fn save_failed() -> Snapshot {
        Snapshot {
            wanted: Some(false),
            radio: Radio::On,
            scan: Scan::Found(Vec::new()),
            scan_rev: 1,
            notice: Some(Notice { id: 1, on: true, kind: NoticeKind::Save, why: "wifi.enabled could not be stored".into() }),
            ..Snapshot::default()
        }
    }

    /// The radio is on and the scan went wrong.
    pub fn scan_failed() -> Snapshot {
        Snapshot {
            wanted: Some(true),
            radio: Radio::On,
            scan: Scan::Failed(ScanError { code: Some(10), detail: "No Wi-Fi device found".into() }),
            scan_rev: 1,
            ..Snapshot::default()
        }
    }

    /// The radio is on and the scan worked: nobody is in reach.
    pub fn empty() -> Snapshot {
        Snapshot { wanted: Some(true), radio: Radio::On, scan: Scan::Found(Vec::new()), scan_rev: 1, ..Snapshot::default() }
    }

    /// The radio is being switched on.
    pub fn switching_on() -> Snapshot {
        Snapshot { switching: Some(true), ..off() }
    }
}

/// A stand-in system for tests: scripted answers, a log of what was asked, and a gate that holds a scan until the test
/// lets it go.
#[cfg(test)]
pub(crate) mod fake {
    use super::*;

    pub(crate) struct Gate {
        open: Mutex<bool>,
        cv: Condvar,
    }

    impl Gate {
        pub(crate) fn new() -> Arc<Gate> {
            Arc::new(Gate { open: Mutex::new(false), cv: Condvar::new() })
        }
        pub(crate) fn open(&self) {
            *locked(&self.open) = true;
            self.cv.notify_all();
        }
        pub(crate) fn pass(&self) {
            let mut g = locked(&self.open);
            while !*g {
                g = self.cv.wait(g).unwrap_or_else(|e| e.into_inner());
            }
        }

        /// Waits until the gate is opened or `stop` is set (a scan that is stopped, as nmcli is killed).
        fn pass_unless(&self, stop: &AtomicBool) {
            let mut g = locked(&self.open);
            while !*g && !stop.load(Ordering::SeqCst) {
                g = self.cv.wait_timeout(g, Duration::from_millis(2)).unwrap_or_else(|e| e.into_inner()).0;
            }
        }
    }

    pub(crate) struct Script {
        pub radio: Radio,
        pub wanted: Option<bool>,
        /// What switching the radio says; `moves`: whether the radio really follows.
        pub switch_result: Result<(), String>,
        pub moves: bool,
        /// The answers of the next scans; the last one repeats.
        pub scans: VecDeque<Result<Vec<Network>, ScanError>>,
        pub saving: Saving,
        pub scan_gate: Option<Arc<Gate>>,
        /// Set when a scan has started waiting at its gate.
        pub scan_waiting: bool,
        /// Everything asked, in order.
        pub log: Vec<String>,
        /// When the radio reads as `then` instead after the n-th look: the boot block that ends by itself.
        pub up_after_looks: Option<(usize, Radio)>,
        pub looks: usize,
        /// Storing `wifi.enabled` does not work.
        pub persist_fails: bool,
    }

    pub(crate) struct FakeNet(pub Mutex<Script>, Condvar);

    impl FakeNet {
        pub(crate) fn new(radio: Radio) -> Arc<FakeNet> {
            Arc::new(FakeNet(
                Mutex::new(Script {
                radio,
                wanted: Some(true),
                switch_result: Ok(()),
                moves: true,
                scans: VecDeque::from([Ok(Vec::new())]),
                saving: Saving::default(),
                scan_gate: None,
                scan_waiting: false,
                log: Vec::new(),
                up_after_looks: None,
                looks: 0,
                persist_fails: false,
                }),
                Condvar::new(),
            ))
        }
        pub(crate) fn script(&self) -> MutexGuard<'_, Script> {
            locked(&self.0)
        }
        pub(crate) fn log(&self) -> Vec<String> {
            self.script().log.clone()
        }
        /// Waits until a scan has begun and stands at its gate.
        pub(crate) fn wait_scan_started(&self) {
            let mut s = self.script();
            while !s.scan_waiting {
                let (g, timeout) = self.1.wait_timeout(s, Duration::from_secs(10)).unwrap_or_else(|e| e.into_inner());
                assert!(!timeout.timed_out(), "no scan began");
                s = g;
            }
        }
        pub(crate) fn scans_run(&self) -> usize {
            self.script().log.iter().filter(|l| l.as_str() == "scan").count()
        }
    }

    impl NetOps for FakeNet {
        fn radio(&self) -> Radio {
            let mut s = self.script();
            s.looks += 1;
            if let Some((n, then)) = s.up_after_looks {
                if s.looks > n {
                    s.radio = then;
                    s.up_after_looks = None;
                }
            }
            s.radio
        }
        fn set_radio(&self, on: bool) -> Result<(), String> {
            let mut s = self.script();
            s.log.push(format!("set_radio {on}"));
            if s.switch_result.is_ok() && s.moves {
                s.radio = if on { Radio::On } else { Radio::Off(Cause::Soft) };
            }
            s.switch_result.clone()
        }
        fn wanted(&self) -> Option<bool> {
            self.script().wanted
        }
        fn persist_wanted(&self, on: bool) -> bool {
            let mut s = self.script();
            s.log.push(format!("persist {on}"));
            if s.persist_fails {
                return false;
            }
            s.wanted = Some(on);
            true
        }
        fn saving(&self, _stop: &AtomicBool) -> (Saving, String) {
            let s = self.script();
            (s.saving.clone(), String::new())
        }
        fn address(&self) -> Option<String> {
            self.script().saving.ip.clone()
        }
        fn scan(&self, stop: &AtomicBool) -> Result<Vec<Network>, ScanError> {
            let gate = {
                let mut s = self.script();
                s.log.push("scan".into());
                s.scan_waiting = s.scan_gate.is_some();
                self.1.notify_all();
                s.scan_gate.clone()
            };
            if let Some(g) = gate {
                g.pass_unless(stop);
            }
            let mut s = self.script();
            s.scan_waiting = false;
            if stop.load(Ordering::SeqCst) {
                return Err(ScanError { code: None, detail: "stopped".into() });
            }
            if s.scans.len() > 1 {
                s.scans.pop_front().unwrap()
            } else {
                s.scans.front().cloned().unwrap_or_else(|| Ok(Vec::new()))
            }
        }
    }

    pub(crate) fn nets(text: &str) -> Vec<Network> {
        crate::wifi::parse_scan(text)
    }

    pub(crate) fn failure(code: i32, detail: &str) -> Result<Vec<Network>, ScanError> {
        Err(ScanError { code: Some(code), detail: detail.into() })
    }
}

#[cfg(test)]
mod tests {
    use super::fake::*;
    use super::*;
    use std::sync::atomic::AtomicUsize;

    const HOME: &str = "Home:78:WPA2:*\nCafe guest:41::\n";

    fn inline(f: &Arc<FakeNet>) -> Arc<NetState> {
        NetState::inline(f.clone())
    }

    /// Waits, without polling the clock, until the picture satisfies `ok`: woken by every change (10 s is only the bound
    /// of a test that would hang).
    fn wait_for(net: &Arc<NetState>, what: &str, ok: impl Fn(&Snapshot) -> bool) {
        let (tx, rx) = std::sync::mpsc::channel::<()>();
        let tx = Mutex::new(tx);
        let wake: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
            let _ = tx.lock().unwrap().send(());
        });
        net.subscribe(&wake);
        while !ok(&net.snapshot()) {
            rx.recv_timeout(Duration::from_secs(10)).unwrap_or_else(|_| panic!("timed out waiting for {what}"));
        }
    }

    /// A state whose retry waits for the test: what it was asked to run later is kept, and run when the test says.
    fn later_by_hand(f: &Arc<FakeNet>) -> (Arc<NetState>, Arc<Mutex<Vec<Job>>>) {
        let kept: Arc<Mutex<Vec<Job>>> = Arc::new(Mutex::new(Vec::new()));
        let sink = kept.clone();
        let later: Later = Arc::new(move |_wait, job| sink.lock().unwrap().push(job));
        (NetState::with_later(f.clone(), later), kept)
    }

    /// Radio off, the wish off: the player's own choice, nothing to switch back on.
    fn off_by_choice() -> Arc<FakeNet> {
        let fake = FakeNet::new(Radio::Off(Cause::Soft));
        fake.script().wanted = Some(false);
        fake
    }

    #[test]
    fn a_switch_that_worked_stores_the_wish_only_after_reading_the_radio_back() {
        let fake = FakeNet::new(Radio::Off(Cause::Soft));
        fake.script().wanted = Some(false);
        fake.script().scans = VecDeque::from([Ok(nets(HOME))]);
        let net = inline(&fake);
        net.set_enabled(true);
        let s = net.snapshot();
        assert_eq!((s.radio, s.wanted, s.switching, s.notice), (Radio::On, Some(true), None, None));
        let log = fake.log();
        assert_eq!(log[..2], ["set_radio true", "persist true"], "the wish is stored after the radio changed");
        assert!(matches!(&s.scan, Scan::Found(n) if n.len() == 2), "turned on: the list is looked at again: {:?}", s.scan);
        net.set_enabled(false);
        let s = net.snapshot();
        assert_eq!((s.radio, s.wanted, s.scan), (Radio::Off(Cause::Soft), Some(false), Scan::NotYet), "off: no list left over");
        assert_eq!(s.saving.ip, None);
    }

    #[test]
    fn a_switch_that_failed_keeps_the_wish_shows_the_radio_and_leaves_a_notice() {
        let fake = FakeNet::new(Radio::Off(Cause::Soft));
        fake.script().wanted = Some(false);
        fake.script().switch_result = Err("wifictl enable exited with 1".into());
        let net = inline(&fake);
        net.set_enabled(true);
        let s = net.snapshot();
        assert_eq!(s.radio, Radio::Off(Cause::Soft), "the switch tells the truth");
        assert_eq!(s.wanted, Some(false), "the wish was not changed");
        assert!(!fake.log().iter().any(|l| l.starts_with("persist")), "nothing stored: {:?}", fake.log());
        let n = s.notice.expect("a notice");
        assert_eq!((n.on, n.id), (true, 1));
        assert!(n.why.contains("exited with 1"));
        assert_eq!(s.switching, None);
        // The next try starts clean and works.
        fake.script().switch_result = Ok(());
        net.set_enabled(true);
        let s = net.snapshot();
        assert_eq!((s.radio, s.wanted, s.notice), (Radio::On, Some(true), None));
    }

    #[test]
    fn a_switch_that_says_ok_but_changed_nothing_is_a_failure() {
        let fake = FakeNet::new(Radio::Off(Cause::Soft));
        fake.script().wanted = Some(false);
        fake.script().moves = false;
        let net = inline(&fake);
        net.set_enabled(true);
        let s = net.snapshot();
        assert_eq!((s.radio, s.wanted), (Radio::Off(Cause::Soft), Some(false)));
        assert!(s.notice.is_some_and(|n| n.why.contains("after switching it on")));
        assert!(!fake.log().iter().any(|l| l.starts_with("persist")));
    }

    #[test]
    fn the_toggle_goes_by_a_fresh_read_not_by_what_the_row_showed() {
        // The picture says "on" (the last look), but the boot hooks have blocked the radio since: one press turns it on.
        let fake = FakeNet::new(Radio::On);
        let net = inline(&fake);
        net.poll();
        assert_eq!(net.snapshot().radio, Radio::On);
        fake.script().radio = Radio::Off(Cause::Soft);
        fake.script().wanted = Some(false);
        net.toggle();
        let s = net.snapshot();
        assert_eq!((s.radio, s.wanted), (Radio::On, Some(true)), "the first press already did what it should");
        assert_eq!(fake.log()[..2], ["set_radio true", "persist true"]);
        // And the other way: shown off, really on: the press turns it off.
        let fake = off_by_choice();
        let net = inline(&fake);
        net.poll();
        fake.script().radio = Radio::On;
        net.toggle();
        assert_eq!(net.snapshot().radio, Radio::Off(Cause::Soft));
        assert_eq!(net.snapshot().wanted, Some(false));
    }

    #[test]
    fn a_toggle_while_one_is_under_way_changes_nothing() {
        let fake = FakeNet::new(Radio::On);
        let net = NetState::new(fake.clone());
        let gate = Gate::new();
        let g = gate.clone();
        net.poll();
        net.exclusive("connect", move || g.pass());
        net.toggle();
        assert_eq!(net.snapshot().switching, Some(false));
        net.toggle();
        gate.open();
        net.wait_idle();
        let s = net.snapshot();
        assert_eq!((s.radio, s.switching), (Radio::Off(Cause::Soft), None), "one switch, not two");
        assert_eq!(fake.log().iter().filter(|l| l.starts_with("set_radio")).count(), 1);
    }

    #[test]
    fn a_toggle_where_there_is_no_radio_does_nothing_and_says_nothing() {
        let fake = FakeNet::new(Radio::Absent);
        let net = inline(&fake);
        net.toggle();
        let s = net.snapshot();
        assert_eq!((s.radio, s.switching, s.notice), (Radio::Absent, None, None));
        assert!(fake.log().is_empty());
    }

    #[test]
    fn a_radio_that_is_off_is_neither_an_error_nor_an_empty_list() {
        // Switched off by the player: nothing to search with, and nobody switches it back on.
        let fake = off_by_choice();
        let net = inline(&fake);
        net.request_scan();
        let s = net.snapshot();
        assert_eq!((s.radio, s.wanted, s.scan.clone(), s.scanning), (Radio::Off(Cause::Soft), Some(false), Scan::NotYet, false));
        assert_eq!(fake.scans_run(), 0, "nothing to search with, nothing asked of nmcli");
    }

    #[test]
    fn a_failed_scan_is_not_an_empty_one() {
        let fake = FakeNet::new(Radio::On);
        fake.script().scans = VecDeque::from([failure(10, "No Wi-Fi device found")]);
        let net = inline(&fake);
        net.request_scan();
        let s = net.snapshot();
        assert!(matches!(&s.scan, Scan::Failed(e) if e.code == Some(10) && e.detail.contains("No Wi-Fi device")), "{:?}", s.scan);
        assert!(!s.scanning);
        // An empty answer that worked is an empty list.
        let fake = FakeNet::new(Radio::On);
        fake.script().scans = VecDeque::from([Ok(Vec::new())]);
        let net = inline(&fake);
        net.request_scan();
        assert_eq!(net.snapshot().scan, Scan::Found(Vec::new()));
    }

    #[test]
    fn one_automatic_retry_after_a_scan_that_found_nothing() {
        let fake = FakeNet::new(Radio::On);
        fake.script().scans = VecDeque::from([failure(10, "No Wi-Fi device found"), Ok(nets(HOME))]);
        let net = inline(&fake);
        net.request_scan();
        assert_eq!(fake.scans_run(), 2, "the failure was followed by one more scan");
        assert!(matches!(&net.snapshot().scan, Scan::Found(n) if n.len() == 2));
        // Only one: a radio that keeps failing is not hammered.
        let fake = FakeNet::new(Radio::On);
        fake.script().scans = VecDeque::from([failure(10, "x")]);
        let net = inline(&fake);
        net.request_scan();
        assert_eq!(fake.scans_run(), 2);
        assert!(matches!(net.snapshot().scan, Scan::Failed(_)));
        net.request_scan();
        assert_eq!(fake.scans_run(), 3, "a press of X scans once, with no retry behind it");
    }

    #[test]
    fn the_retry_waits_and_gives_way_to_a_scan_somebody_asked_for_meanwhile() {
        let fake = FakeNet::new(Radio::On);
        fake.script().scans = VecDeque::from([failure(10, "x"), Ok(nets(HOME))]);
        let (net, later) = later_by_hand(&fake);
        net.request_scan();
        net.wait_idle();
        assert!(matches!(net.snapshot().scan, Scan::Failed(_)));
        assert_eq!((fake.scans_run(), later.lock().unwrap().len()), (1, 1), "the retry is asked for, not run");
        let retry = later.lock().unwrap().pop().unwrap();
        retry();
        net.wait_idle();
        assert_eq!(fake.scans_run(), 2);
        assert!(matches!(net.snapshot().scan, Scan::Found(_)));
        // Somebody scanned before the retry came due: the retry stays away.
        let fake = FakeNet::new(Radio::On);
        fake.script().scans = VecDeque::from([failure(10, "x"), Ok(nets(HOME))]);
        let (net, later) = later_by_hand(&fake);
        net.request_scan();
        net.wait_idle();
        net.request_scan();
        net.wait_idle();
        assert!(matches!(net.snapshot().scan, Scan::Found(_)));
        let retry = later.lock().unwrap().pop().unwrap();
        retry();
        net.wait_idle();
        assert_eq!(fake.scans_run(), 2, "the pending retry saw a newer scan and did nothing");
    }

    #[test]
    fn a_radio_that_comes_up_after_boot_scans_once_by_itself() {
        // The boot hooks block the radio, then unblock it (the 22:41:18 → 22:41:32 gap of the incident). The wish is off
        // here, so nothing but the hooks switches it on.
        let fake = off_by_choice();
        fake.script().scans = VecDeque::from([Ok(nets(HOME))]);
        let net = inline(&fake);
        net.request_scan();
        assert_eq!(net.snapshot().scan, Scan::NotYet);
        net.poll();
        assert_eq!(net.snapshot().radio, Radio::Off(Cause::Soft), "still blocked: nothing happens");
        fake.script().radio = Radio::On;
        net.poll();
        let s = net.snapshot();
        assert_eq!(s.radio, Radio::On);
        assert!(matches!(&s.scan, Scan::Found(n) if n.len() == 2), "the page did not need a press: {:?}", s.scan);
    }

    #[test]
    fn nobody_wanted_a_list_so_a_radio_coming_up_does_not_scan() {
        let fake = off_by_choice();
        let net = inline(&fake);
        net.poll();
        fake.script().radio = Radio::On;
        net.poll();
        assert_eq!(fake.scans_run(), 0);
        assert_eq!(net.snapshot().radio, Radio::On);
    }

    #[test]
    fn a_scan_that_was_overtaken_by_a_switch_is_dropped() {
        let fake = FakeNet::new(Radio::On);
        let gate = Gate::new();
        {
            let mut s = fake.script();
            s.scan_gate = Some(gate.clone());
            s.scans = VecDeque::from([Ok(nets(HOME))]);
        }
        let net = NetState::new(fake.clone());
        net.request_scan();
        fake.wait_scan_started();
        // The player switches Wi-Fi off while the scan is under way.
        net.set_enabled(false);
        gate.open();
        net.wait_idle();
        let s = net.snapshot();
        assert_eq!(s.radio, Radio::Off(Cause::Soft));
        assert_eq!(s.scan, Scan::NotYet, "the list of a radio that was on is not shown for one that is off: {:?}", s.scan);
        assert!(!s.scanning);
        assert_eq!(s.wanted, Some(false));
    }

    #[test]
    fn a_scan_that_a_newer_scan_replaced_is_dropped() {
        let fake = FakeNet::new(Radio::On);
        let gate = Gate::new();
        {
            let mut s = fake.script();
            s.scan_gate = Some(gate.clone());
            s.scans = VecDeque::from([Ok(nets("Old:50:WPA2:\n")), Ok(nets("New:60:WPA2:\n"))]);
        }
        let net = NetState::new(fake.clone());
        net.request_scan();
        fake.wait_scan_started();
        net.request_scan();
        gate.open();
        net.wait_idle();
        let s = net.snapshot();
        let ssids: Vec<String> = match &s.scan {
            Scan::Found(n) => n.iter().map(|n| n.ssid.clone()).collect(),
            other => panic!("{other:?}"),
        };
        assert_eq!(ssids, ["New"], "the newest answer wins");
        assert!(!s.scanning);
    }

    #[test]
    fn a_look_that_started_before_a_switch_cannot_undo_it() {
        let fake = off_by_choice();
        let net = inline(&fake);
        // A look begun now (it has read "off") ...
        let epoch = net.epoch();
        // ... a switch is asked for and done in the meantime ...
        net.set_enabled(true);
        assert_eq!(net.snapshot().radio, Radio::On);
        // ... and the old look arrives: dropped.
        net.take_look(epoch, Radio::Off(Cause::Soft), Some(false), None, true);
        let s = net.snapshot();
        assert_eq!((s.radio, s.wanted), (Radio::On, Some(true)));
    }

    #[test]
    fn a_connection_runs_between_the_switches_before_and_after_it() {
        let fake = FakeNet::new(Radio::On);
        let net = NetState::new(fake.clone());
        let gate = Gate::new();
        let (f1, g1) = (fake.clone(), gate.clone());
        net.exclusive("connect", move || {
            g1.pass();
            f1.script().log.push("connected".into());
        });
        net.set_enabled(false);
        let f2 = fake.clone();
        net.exclusive("forget", move || f2.script().log.push("forgot".into()));
        gate.open();
        net.wait_idle();
        let log = fake.log();
        let at = |what: &str| log.iter().position(|l| l == what).unwrap_or_else(|| panic!("{what} missing in {log:?}"));
        assert!(at("connected") < at("set_radio false") && at("set_radio false") < at("forgot"), "{log:?}");
        assert_eq!(log.iter().filter(|l| l.starts_with("set_radio")).count(), 1);
        assert!(log[..at("set_radio false")].iter().all(|l| l != "scan"), "no scan before the switch: {log:?}");
        assert_eq!(net.snapshot().radio, Radio::Off(Cause::Soft), "the switch off after the connect is what stays");
    }

    #[test]
    fn watchdog_queued_during_a_user_activation_is_discarded() {
        let fake = FakeNet::new(Radio::On);
        let net = NetState::new(fake);
        let gate = Gate::new();
        let g = gate.clone();
        net.exclusive("connect", move || g.pass());
        let called = Arc::new(AtomicBool::new(false));
        let c = called.clone();
        net.reconnect(move |_| c.store(true, Ordering::SeqCst));
        gate.open();
        net.wait_idle();
        assert!(!called.load(Ordering::SeqCst), "the user activation takes precedence");
    }

    #[test]
    fn a_radio_switch_stops_the_watchdogs_scan_before_it_can_connect() {
        let fake = FakeNet::new(Radio::On);
        let net = NetState::new(fake);
        let gate = Gate::new();
        let g = gate.clone();
        let (started, waiting) = std::sync::mpsc::channel();
        let connected = Arc::new(AtomicBool::new(false));
        let c = connected.clone();
        net.reconnect(move |stop| {
            started.send(()).unwrap();
            g.pass();
            if !stop.load(Ordering::SeqCst) { c.store(true, Ordering::SeqCst); }
        });
        waiting.recv_timeout(Duration::from_secs(5)).unwrap();
        net.set_enabled(false);
        gate.open();
        net.wait_idle();
        assert!(!connected.load(Ordering::SeqCst));
        assert!(net.snapshot().radio.is_off());
    }

    #[test]
    fn every_change_calls_the_subscribers_and_a_dropped_one_is_forgotten() {
        let fake = off_by_choice();
        let net = inline(&fake);
        let calls = Arc::new(AtomicUsize::new(0));
        let c = calls.clone();
        let f: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
            c.fetch_add(1, Ordering::SeqCst);
        });
        net.subscribe(&f);
        net.set_enabled(true);
        let n = calls.load(Ordering::SeqCst);
        assert!(n >= 2, "switching, then switched: {n}");
        net.poll();
        assert_eq!(calls.load(Ordering::SeqCst), n, "a look that finds nothing new tells nobody");
        drop(f);
        net.set_enabled(false);
        assert_eq!(calls.load(Ordering::SeqCst), n, "dropped: not called again");
    }

    #[test]
    fn a_made_up_state_stays_as_it_is() {
        let net = NetState::demo(demo::off());
        net.set_enabled(true);
        net.request_scan();
        net.poll();
        assert_eq!(net.snapshot(), demo::off());
    }

    #[test]
    fn a_switch_that_worked_but_could_not_be_stored_says_so_and_stores_it_at_the_next_look() {
        let fake = off_by_choice();
        fake.script().persist_fails = true;
        let net = inline(&fake);
        net.set_enabled(true);
        let s = net.snapshot();
        assert_eq!(s.radio, Radio::On, "the radio is on");
        assert_eq!(s.wanted, Some(false), "the file still says off: the next start would undo the choice");
        let n = s.notice.expect("never silent");
        assert_eq!((n.on, n.kind), (true, NoticeKind::Save));
        assert_eq!(n.line(false), "Wi-Fi is on, but the setting couldn't be saved yet.");
        assert_eq!(fake.log().iter().filter(|l| *l == "persist true").count(), 2, "tried twice at once");
        // The file works again: the next look stores it and the notice goes.
        fake.script().persist_fails = false;
        net.poll();
        let s = net.snapshot();
        assert_eq!((s.wanted, s.notice), (Some(true), None));
        assert_eq!(fake.script().wanted, Some(true));
    }

    #[test]
    fn a_wish_that_could_not_be_stored_is_not_undone_by_the_unblock() {
        // Switched off, the file could not be told: the radio is off and the file still says on. That is not a boot block.
        let fake = FakeNet::new(Radio::On);
        fake.script().persist_fails = true;
        let net = inline(&fake);
        net.poll();
        net.toggle();
        assert_eq!(net.snapshot().radio, Radio::Off(Cause::Soft));
        net.poll();
        net.poll();
        let s = net.snapshot();
        assert_eq!(s.radio, Radio::Off(Cause::Soft), "the player's off stays off");
        assert!(!fake.log().iter().any(|l| l == "set_radio true"), "{:?}", fake.log());
    }

    #[test]
    fn a_panicking_job_does_not_end_the_worker_or_leave_switching_stuck() {
        let fake = FakeNet::new(Radio::On);
        let net = NetState::new(fake.clone());
        net.poll();
        net.exclusive("boom", || panic!("a job that goes wrong"));
        net.toggle();
        net.wait_idle();
        let s = net.snapshot();
        assert_eq!((s.radio, s.switching), (Radio::Off(Cause::Soft), None), "the switch after the panic was done");
        assert!(!s.scanning, "no scan stuck on");
        // And the worker still takes scans.
        net.request_scan();
        net.wait_idle();
        assert!(!net.snapshot().scanning);
    }

    #[test]
    fn a_panic_in_a_switch_clears_the_busy_flag() {
        struct Boom(Arc<FakeNet>);
        impl NetOps for Boom {
            fn radio(&self) -> Radio {
                self.0.radio()
            }
            fn set_radio(&self, _on: bool) -> Result<(), String> {
                panic!("the radio code panicked")
            }
            fn wanted(&self) -> Option<bool> {
                self.0.wanted()
            }
            fn persist_wanted(&self, on: bool) -> bool {
                self.0.persist_wanted(on)
            }
            fn saving(&self, stop: &AtomicBool) -> (Saving, String) {
                self.0.saving(stop)
            }
            fn address(&self) -> Option<String> {
                None
            }
            fn scan(&self, stop: &AtomicBool) -> Result<Vec<Network>, ScanError> {
                self.0.scan(stop)
            }
        }
        let net = NetState::new(Arc::new(Boom(off_by_choice())));
        net.set_enabled(true);
        net.wait_idle();
        let s = net.snapshot();
        assert_eq!((s.switching, s.scanning), (None, false), "not stuck on");
        net.request_scan();
        net.wait_idle();
        assert!(!net.snapshot().scanning, "the worker still runs");
    }

    #[test]
    fn a_switch_does_not_wait_behind_a_running_scan() {
        let fake = FakeNet::new(Radio::On);
        let gate = Gate::new();
        fake.script().scan_gate = Some(gate.clone());
        let net = NetState::new(fake.clone());
        net.request_scan();
        fake.wait_scan_started();
        // The gate stays shut: the scan would run for as long as nmcli does. The switch stops it and goes first.
        net.toggle();
        wait_for(&net, "the switch", |s| s.radio == Radio::Off(Cause::Soft) && s.switching.is_none());
        net.wait_idle();
        let s = net.snapshot();
        assert_eq!((s.scan.clone(), s.scanning), (Scan::NotYet, false), "{:?}", s.scan);
        gate.open();
    }

    #[test]
    fn a_scan_that_waits_in_the_queue_is_behind_a_switch_asked_for_after_it() {
        let fake = FakeNet::new(Radio::On);
        let gate = Gate::new();
        let net = NetState::new(fake.clone());
        let g = gate.clone();
        net.exclusive("connect", move || g.pass());
        net.request_scan();
        net.set_enabled(false);
        gate.open();
        net.wait_idle();
        let log = fake.log();
        let off = log.iter().position(|l| l == "set_radio false").unwrap();
        assert!(log[..off].iter().all(|l| l != "scan"), "{log:?}");
    }

    #[test]
    fn an_external_block_during_a_scan_cannot_bring_a_stale_list_back() {
        let fake = FakeNet::new(Radio::On);
        let gate = Gate::new();
        {
            let mut f = fake.script();
            f.scan_gate = Some(gate.clone());
            f.scans = VecDeque::from([Ok(nets(HOME))]);
            f.wanted = Some(false);
        }
        let net = NetState::new(fake.clone());
        net.request_scan();
        fake.wait_scan_started();
        // The hooks (not this program) block the radio; a look notices.
        fake.script().radio = Radio::Off(Cause::Soft);
        net.poll();
        assert_eq!(net.snapshot().radio, Radio::Off(Cause::Soft));
        gate.open();
        net.wait_idle();
        let s = net.snapshot();
        assert_eq!((s.radio, s.scan, s.scanning), (Radio::Off(Cause::Soft), Scan::NotYet, false), "the list of a radio that was on is gone");
    }

    #[test]
    fn a_radio_the_boot_blocked_while_the_wish_is_on_is_switched_on_once() {
        let fake = FakeNet::new(Radio::Off(Cause::Soft));
        fake.script().wanted = Some(true);
        fake.script().scans = VecDeque::from([Ok(nets(HOME))]);
        let net = inline(&fake);
        net.request_scan();
        let s = net.snapshot();
        assert_eq!(s.radio, Radio::On, "the player's own wish, carried out");
        assert!(matches!(&s.scan, Scan::Found(n) if n.len() == 2), "and the list follows");
        assert_eq!(fake.log().iter().filter(|l| *l == "set_radio true").count(), 1);
        assert!(!fake.log().iter().any(|l| l.starts_with("persist")), "the wish was on already");
        assert_eq!(s.notice, None);
    }

    #[test]
    fn a_radio_that_is_off_by_the_players_choice_is_never_switched_on_by_itself() {
        let fake = off_by_choice();
        let net = inline(&fake);
        for _ in 0..3 {
            net.poll();
            net.request_scan();
        }
        assert_eq!(net.snapshot().radio, Radio::Off(Cause::Soft));
        assert!(!fake.log().iter().any(|l| l.starts_with("set_radio")), "{:?}", fake.log());
        // Neither a hard block nor NetworkManager's own switch is the boot hooks' soft block.
        for cause in [Cause::Hard, Cause::Manager] {
            let fake = FakeNet::new(Radio::Off(cause));
            let net = inline(&fake);
            net.poll();
            assert!(fake.log().is_empty(), "{cause:?}");
        }
    }

    #[test]
    fn an_unblock_that_does_not_work_is_tried_once_and_says_nothing() {
        let fake = FakeNet::new(Radio::Off(Cause::Soft));
        fake.script().switch_result = Err("wifictl enable exited with 1".into());
        let net = inline(&fake);
        for _ in 0..4 {
            net.poll();
        }
        let s = net.snapshot();
        assert_eq!((s.radio, s.notice, s.switching), (Radio::Off(Cause::Soft), None, None), "the page says it is off, A tries");
        assert_eq!(fake.log().iter().filter(|l| *l == "set_radio true").count(), 1, "once, not at every look");
    }

    #[test]
    fn a_block_again_soon_after_the_unblock_is_left_alone() {
        let fake = FakeNet::new(Radio::Off(Cause::Soft));
        let net = inline(&fake);
        net.poll();
        assert_eq!(net.snapshot().radio, Radio::On);
        // Something blocks it again right away: not a loop of unblocks.
        fake.script().radio = Radio::Off(Cause::Soft);
        net.poll();
        fake.script().radio = Radio::On;
        net.poll();
        fake.script().radio = Radio::Off(Cause::Soft);
        net.poll();
        assert_eq!(fake.log().iter().filter(|l| *l == "set_radio true").count(), 1, "{:?}", fake.log());
        assert_eq!(net.snapshot().radio, Radio::Off(Cause::Soft));
    }
}
