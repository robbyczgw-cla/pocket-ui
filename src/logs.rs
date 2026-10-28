//! Logs that outlive a freeze. ROCKNIX keeps the journal and `/var/log` in RAM, so when the Mini V2 froze and
//! the hardware watchdog rebooted it, nothing was left to read. This keeps these logs on the card, in
//! `<data dir>/logs/` (`/storage/.config/pocket-ui/logs`):
//!
//! | File | What |
//! |---|---|
//! | `pocket-ui.log` | pocket-ui's log, which pocket-core writes into too (a copy of `/var/log/pocket-ui.log`, each line with the uptime) |
//! | `kernel.log` | `/dev/kmsg`, from the start of the boot |
//! | `sensors.log` | every 5 s: hottest thermal zone, GPU clock, CPU governor and clock per cluster, fan, battery |
//! | `net.log` | what NetworkManager, iwd, tailscaled (wpa_supplicant, connman where present) said this boot, followed live from one `journalctl -f` child, plus a snapshot of the saved connections' and iwd's known networks' *names* at the start and on every change (`net_log`); secrets blanked before they are written |
//! | `shutdown.log` | written by pocket-ui, not the logger (`shutdown_log`): every power off or restart, the RTC alarm read back, the command and how it ended, fsynced at each step |
//! | `portal-last.html` | the last Wi-Fi sign-in page that ended with "needs a full browser" (`save_portal_page`), for the next "Report a problem" |
//!
//! The launcher (`deploy/run-pocket-ui.sh`) starts the logger as `pocket-logd`, which is `pocket-core --log-daemon`
//! (a hard link under another name: `pidof pocket-core` and `another_pocket_core_runs` still mean "a game is running"). One thread, one wake-up a
//! second at most.
//!
//! * **Rotation per boot.** The current boot's files carry the plain names. When the logger (or pocket-ui)
//!   starts in another boot (`/proc/sys/kernel/random/boot_id` differs from the one in `boot`), they move to
//!   `boot-<n>-<date>-<time>/`; the newest [`KEEP_BOOTS`] boots stay, including the current one.
//! * **Size cap.** A file is at most [`FILE_CAP`]: when the current segment is half of it, it becomes
//!   `<name>.old` (replacing the one before) and a new one starts.
//! * **Fsync without wearing the card.** Lines are written to the page cache every second, in one `write` per
//!   file, and fsynced every [`SYNC_EVERY`] (the sensors and `net.log` every [`SENSORS_SYNC_EVERY`]), only when there is something new,
//!   and at once after a kernel line of level warning or worse, which is what comes before a freeze. An idle device
//!   syncs one file every 15 s: 5 760 syncs a day, an estimate of a few tens of MB of physical writes (a sample is
//!   150 bytes, but a card writes a page and a journal block for it). Not measured on the device.
//! * **Clean exit.** A logger that is asked to stop (SIGTERM: shutdown, restart) writes `clean-exit`. A boot
//!   whose files have no such marker ended in a freeze, a watchdog reset, a power cut or a hard power-off:
//!   the next start writes a line with the last kernel lines to `crash.log` and leaves the usual crash notice.

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::io::{AsRawFd, RawFd};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

pub const DIR_NAME: &str = "logs";
pub const UI_LOG: &str = "pocket-ui.log";
pub const KERNEL_LOG: &str = "kernel.log";
pub const SENSORS_LOG: &str = "sensors.log";
/// The network daemons' journal lines and the saved networks' names (`net_log`).
pub const NET_LOG: &str = "net.log";
/// What the shutdown path did (`shutdown_log`): written by pocket-ui itself, fsynced at each step.
pub const SHUTDOWN_LOG: &str = "shutdown.log";
/// The last Wi-Fi sign-in page the helper couldn't do (`save_portal_page`): what the hotel served, for the report.
pub const PORTAL_LAST: &str = "portal-last.html";
/// The most of that page kept (a report is a zip on a card; an interstitial is small anyway).
pub const PORTAL_LAST_CAP: usize = 200 * 1024;
const STREAMS: [&str; 5] = [UI_LOG, KERNEL_LOG, SENSORS_LOG, NET_LOG, SHUTDOWN_LOG];
/// Which boot the files in the folder belong to: `id=`, `seq=` (counts boots), `started=` (Unix seconds).
const STATE_FILE: &str = "boot";
/// Written by a logger that was asked to stop; moves away with the boot's other files.
const CLEAN_FILE: &str = "clean-exit";
const LOCK_FILE: &str = ".lock";
const DAEMON_LOCK_FILE: &str = ".daemon.lock";
/// Boots kept, the current one included.
pub const KEEP_BOOTS: usize = 3;
/// The most one log takes on the card (current segment and `.old` together, give or take one batch of lines).
pub const FILE_CAP: u64 = 5 * 1024 * 1024;
/// The longest the logger sleeps: how fast a new line of `/var/log/pocket-ui.log` reaches the card's page cache.
pub const TICK: Duration = Duration::from_secs(1);
pub const SAMPLE_EVERY: Duration = Duration::from_secs(5);
pub const SYNC_EVERY: Duration = Duration::from_secs(5);
/// A sample is a line of about 200 bytes and the trend is what matters: fsynced every third one.
pub const SENSORS_SYNC_EVERY: Duration = Duration::from_secs(15);
/// Not more than this many wake-ups a second while the kernel is chatty.
const MIN_STEP_GAP: Duration = Duration::from_millis(100);
/// What one tick reads of the log at most (the rest follows on the next ones).
const READ_MAX: u64 = 256 * 1024;
const TAIL_LINES: usize = 6;

// ---- one boot's files ---------------------------------------------------------------------------------

/// A file that is appended to in batches, fsynced on request, and rolled over at half the cap.
pub struct Rolling {
    path: PathBuf,
    old: PathBuf,
    segment: u64,
    file: Option<File>,
    size: u64,
    dirty: bool,
}

impl Rolling {
    /// `cap`: the most the file and its `.old` take together.
    pub fn new(path: PathBuf, cap: u64) -> Rolling {
        let old = PathBuf::from(format!("{}.old", path.display()));
        let size = fs::metadata(&path).map_or(0, |m| m.len());
        Rolling {
            path,
            old,
            segment: (cap / 2).max(1),
            file: None,
            size,
            dirty: false,
        }
    }

    /// Adds `text` (whole lines). A card that went away loses the text, not the program.
    pub fn append(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        let len = text.len() as u64;
        if self.size > 0 && self.size + len > self.segment {
            self.roll();
        }
        if self.file.is_none() {
            self.file = OpenOptions::new().create(true).append(true).open(&self.path).ok();
        }
        let Some(file) = self.file.as_mut() else { return };
        if file.write_all(text.as_bytes()).is_ok() {
            self.size += len;
            self.dirty = true;
        } else {
            self.file = None;
        }
    }

    fn roll(&mut self) {
        self.sync();
        self.file = None;
        let _ = fs::rename(&self.path, &self.old);
        self.size = 0;
    }

    /// Forces what was written to the card. Does nothing when nothing is new.
    pub fn sync(&mut self) {
        if self.dirty {
            if let Some(f) = &self.file {
                let _ = f.sync_data();
            }
            self.dirty = false;
        }
    }

    #[cfg(test)]
    fn dirty(&self) -> bool {
        self.dirty
    }
}

// ---- the boot's identity ------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
struct State {
    id: String,
    seq: u64,
    started: u64,
}

fn read_state(logs: &Path) -> Option<State> {
    let text = fs::read_to_string(logs.join(STATE_FILE)).ok()?;
    let (mut id, mut seq, mut started) = (None, None, 0);
    for line in text.lines() {
        match line.split_once('=') {
            Some(("id", v)) => id = Some(v.trim().to_string()),
            Some(("seq", v)) => seq = v.trim().parse().ok(),
            Some(("started", v)) => started = v.trim().parse().unwrap_or(0),
            _ => {}
        }
    }
    Some(State {
        id: id.filter(|i| !i.is_empty())?,
        seq: seq?,
        started,
    })
}

fn write_state(logs: &Path, s: &State) {
    let tmp = logs.join(format!("{STATE_FILE}.tmp"));
    if fs::write(&tmp, format!("id={}\nseq={}\nstarted={}\n", s.id, s.seq, s.started)).is_ok() {
        let _ = fs::rename(&tmp, logs.join(STATE_FILE));
    }
}

/// An exclusive `flock`, released when this is dropped (or the process ends).
struct Lock {
    _file: File,
}

impl Drop for Lock {
    fn drop(&mut self) {
        // Another thread may fork a helper before its CLOEXEC descriptor closes. Closing only our copy would
        // leave the flock held until that helper execs; the guard's lifetime, not the helper, owns this lock.
        // SAFETY: the guard still owns this valid descriptor; File closes it after this drop body.
        unsafe { libc::flock(self._file.as_raw_fd(), libc::LOCK_UN) };
    }
}

fn lock(path: &Path, wait: bool) -> Option<Lock> {
    let file = OpenOptions::new().create(true).truncate(false).write(true).open(path).ok()?;
    // SAFETY: flock on a descriptor this function owns.
    let rc = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | if wait { 0 } else { libc::LOCK_NB }) };
    (rc == 0).then_some(Lock { _file: file })
}

/// "20261022-140307" for a Unix time.
fn stamp(unix: u64) -> String {
    crate::crash::timestamp(unix)
        .trim_end_matches(" UTC")
        .replace('-', "")
        .replace(' ', "-")
        .replace(':', "")
}

fn stream_files(name: &str) -> [String; 2] {
    [name.to_string(), format!("{name}.old")]
}

/// The newest write time (Unix seconds) among the current boot's logs.
fn newest_mtime(logs: &Path) -> Option<u64> {
    STREAMS
        .iter()
        .flat_map(|n| stream_files(n))
        .filter_map(|n| fs::metadata(logs.join(n)).ok()?.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .max()
}

/// The last `n` non-empty lines of a log, the `.old` segment filling up when the current one is short.
pub(crate) fn tail_lines(logs: &Path, name: &str, n: usize) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for file in [format!("{name}.old"), name.to_string()] {
        let Ok(mut f) = File::open(logs.join(file)) else { continue };
        let len = f.metadata().map_or(0, |m| m.len());
        let from = len.saturating_sub(16 * 1024);
        let mut text = Vec::new();
        if f.seek(SeekFrom::Start(from)).is_ok() && f.read_to_end(&mut text).is_ok() {
            let text = String::from_utf8_lossy(&text);
            let mut it = text.lines();
            // A read from the middle of a file starts in the middle of a line.
            if from > 0 {
                it.next();
            }
            lines.extend(it.filter(|l| !l.trim().is_empty()).map(str::to_string));
        }
    }
    let skip = lines.len().saturating_sub(n);
    lines.split_off(skip)
}

/// What a boot that ended without `clean-exit` left to say.
#[derive(Debug, Clone, PartialEq)]
pub struct Unclean {
    /// Unix seconds of the last write to any of its logs.
    pub last_alive: u64,
    pub kernel_tail: Vec<String>,
    pub sensors_last: Option<String>,
}

/// Starts the boot `boot_id` in `data_dir/logs`, at `now` (Unix seconds). A boot that is already the current one
/// changes nothing (but a marker from an earlier stop is removed: the logger runs again). A new boot moves the old
/// files into `boot-<n>-<date>-<time>/`, drops the boots beyond [`KEEP_BOOTS`], and, when the old boot has no
/// `clean-exit`, writes the report to `crash.log` and leaves the crash notice (`crash::record_unclean`). Safe to call
/// from the logger and from pocket-ui at the same time: one waits for the other.
pub fn begin_boot(data_dir: &Path, boot_id: &str, now: u64) -> Option<Unclean> {
    let logs = data_dir.join(DIR_NAME);
    fs::create_dir_all(&logs).ok()?;
    let _lock = lock(&logs.join(LOCK_FILE), true);
    let previous = read_state(&logs);
    if previous.as_ref().is_some_and(|p| p.id == boot_id) {
        let _ = fs::remove_file(logs.join(CLEAN_FILE));
        return None;
    }
    let mut unclean = None;
    let mut seq = 1;
    if let Some(p) = &previous {
        seq = p.seq + 1;
        if !logs.join(CLEAN_FILE).exists() {
            unclean = Some(Unclean {
                last_alive: newest_mtime(&logs).unwrap_or(p.started),
                kernel_tail: tail_lines(&logs, KERNEL_LOG, TAIL_LINES),
                sensors_last: tail_lines(&logs, SENSORS_LOG, 1).pop(),
            });
        }
        archive(&logs, p);
        prune(&logs);
    }
    write_state(
        &logs,
        &State {
            id: boot_id.to_string(),
            seq,
            started: now,
        },
    );
    if let Some(u) = &unclean {
        crate::crash::record_unclean(data_dir, now, u.last_alive, &u.kernel_tail, u.sensors_last.as_deref());
    }
    unclean
}

/// Moves the finished boot's files into their own folder.
fn archive(logs: &Path, p: &State) {
    let dir = logs.join(format!("boot-{:04}-{}", p.seq, stamp(p.started)));
    if fs::create_dir_all(&dir).is_err() {
        return;
    }
    let names = STREAMS
        .iter()
        .flat_map(|n| stream_files(n))
        .chain([CLEAN_FILE.to_string(), STATE_FILE.to_string(), PORTAL_LAST.to_string()]);
    for name in names {
        let from = logs.join(&name);
        if from.exists() {
            let _ = fs::rename(&from, dir.join(&name));
        }
    }
}

/// Keeps the newest `KEEP_BOOTS - 1` archived boots (the current boot is the one more).
fn prune(logs: &Path) {
    let mut dirs: Vec<String> = fs::read_dir(logs)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| n.starts_with("boot-"))
        .collect();
    dirs.sort();
    let drop = dirs.len().saturating_sub(KEEP_BOOTS - 1);
    for old in &dirs[..drop] {
        let _ = fs::remove_dir_all(logs.join(old));
    }
}

/// This boot's id, or None where there is no such thing (not Linux).
pub fn boot_id() -> Option<String> {
    let id = fs::read_to_string("/proc/sys/kernel/random/boot_id").ok()?;
    let id = id.trim();
    (!id.is_empty()).then(|| id.to_string())
}

/// Saves the last sign-in page the helper couldn't do (`portal::Session::last_page`) for the next report: the HTML as
/// it came (no cookies, no headers), with the `value` of every hidden field blanked and query strings with MAC- or
/// IP-looking values redacted (`portal::blank_hidden`, `portal::redact_query`), at most [`PORTAL_LAST_CAP`] bytes, in
/// this boot's log folder. It moves into the `boot-…` folder with the other logs at the next start (`archive`), and
/// the report takes it along (`report`).
pub fn save_portal_page(data_dir: &Path, html: &str) -> std::io::Result<()> {
    let logs = data_dir.join(DIR_NAME);
    fs::create_dir_all(&logs)?;
    let text = crate::portal::redact_query(&crate::portal::blank_hidden(html));
    let mut n = text.len().min(PORTAL_LAST_CAP);
    while !text.is_char_boundary(n) {
        n -= 1;
    }
    fs::write(logs.join(PORTAL_LAST), &text[..n])
}

/// For pocket-ui's start: the same as the logger's, so the notice is there even where no logger runs. Does
/// nothing where there is no boot id.
pub fn begin_boot_now(data_dir: &Path) -> Option<Unclean> {
    begin_boot(data_dir, &boot_id()?, crate::recents::now_secs())
}

// ---- kernel messages ----------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct Kmsg {
    /// 0 emergency .. 7 debug.
    pub level: u8,
    /// Microseconds since the kernel started counting.
    pub usec: u64,
    pub text: String,
}

/// One record of `/dev/kmsg`: `6,339,5140900,-;NET: Registered`.
pub fn parse_kmsg(line: &str) -> Option<Kmsg> {
    let (head, text) = line.split_once(';')?;
    let mut f = head.split(',');
    let pri: u32 = f.next()?.parse().ok()?;
    let _seq = f.next()?;
    let usec: u64 = f.next()?.parse().ok()?;
    Some(Kmsg {
        level: (pri & 7) as u8,
        usec,
        text: text.trim_end().to_string(),
    })
}

pub fn level_name(level: u8) -> &'static str {
    ["emerg", "alert", "crit", "err", "warn", "notice", "info", "debug"][level.min(7) as usize]
}

/// Warning or worse: what the card gets at once.
pub fn is_urgent(level: u8) -> bool {
    level <= 4
}

/// `[ 5140.900432] warn   text`, like dmesg.
pub fn format_kmsg(m: &Kmsg) -> String {
    format!("[{:>5}.{:06}] {:<6} {}\n", m.usec / 1_000_000, m.usec % 1_000_000, level_name(m.level), m.text)
}

/// The journal time stamp of a `net.log` line that came from the journal (not a snapshot or a note of ours).
fn net_journal_stamp(line: &str) -> Option<u64> {
    let rest = line.split_once("] ")?.1;
    if rest.starts_with("---") || rest.starts_with(' ') {
        return None;
    }
    crate::net_log::stamp_usec(line)
}

/// The time stamp of a line `format_kmsg` made, in microseconds.
fn stamp_usec(line: &str) -> Option<u64> {
    let inner = line.strip_prefix('[')?.split_once(']')?.0.trim();
    let (secs, micros) = inner.split_once('.')?;
    Some(secs.trim().parse::<u64>().ok()? * 1_000_000 + micros.parse::<u64>().ok()?)
}

// ---- the growing log in RAM ---------------------------------------------------------------------------

/// Follows a file that only grows (and may be emptied: the launcher starts `/var/log/pocket-ui.log` anew).
struct Tail {
    path: PathBuf,
    offset: u64,
    partial: Vec<u8>,
}

impl Tail {
    fn new(path: PathBuf) -> Tail {
        Tail {
            path,
            offset: 0,
            partial: Vec::new(),
        }
    }

    /// The whole lines that came since the last call, and whether the file started over.
    fn poll(&mut self) -> (Vec<String>, bool) {
        let Ok(meta) = fs::metadata(&self.path) else { return (Vec::new(), false) };
        let len = meta.len();
        let mut restarted = false;
        if len < self.offset {
            self.offset = 0;
            self.partial.clear();
            restarted = true;
        }
        if len == self.offset {
            return (Vec::new(), restarted);
        }
        let Ok(mut f) = File::open(&self.path) else { return (Vec::new(), restarted) };
        let mut buf = Vec::new();
        if f.seek(SeekFrom::Start(self.offset)).is_err() || f.take((len - self.offset).min(READ_MAX)).read_to_end(&mut buf).is_err() {
            return (Vec::new(), restarted);
        }
        self.offset += buf.len() as u64;
        self.partial.extend(buf);
        let mut lines = Vec::new();
        while let Some(i) = self.partial.iter().position(|&b| b == b'\n') {
            let line: Vec<u8> = self.partial.drain(..=i).collect();
            lines.push(String::from_utf8_lossy(&line[..line.len() - 1]).into_owned());
        }
        // A "line" that never ends (binary output): do not hold it back for ever.
        if self.partial.len() > 64 * 1024 {
            lines.push(String::from_utf8_lossy(&std::mem::take(&mut self.partial)).into_owned());
        }
        (lines, restarted)
    }

    /// What is left at the end, a line without its newline.
    fn flush(&mut self) -> Option<String> {
        (!self.partial.is_empty()).then(|| String::from_utf8_lossy(&std::mem::take(&mut self.partial)).into_owned())
    }
}

// ---- the sensors --------------------------------------------------------------------------------------

pub(crate) fn read_trim(path: &Path) -> Option<String> {
    let s = fs::read_to_string(path).ok()?;
    let s = s.trim();
    (!s.is_empty()).then(|| s.to_string())
}

pub(crate) fn read_i64(path: &Path) -> Option<i64> {
    read_trim(path)?.parse().ok()
}

pub(crate) fn dir_names(dir: &Path, prefix: &str) -> Vec<String> {
    let mut v: Vec<String> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| n.starts_with(prefix))
        .collect();
    // policy10 after policy4: by the number, then by the name
    v.sort_by_key(|n| {
        (
            n.trim_start_matches(|c: char| !c.is_ascii_digit()).parse::<u64>().unwrap_or(u64::MAX),
            n.clone(),
        )
    });
    v
}

/// The hottest thermal zone: `61.3C@z6`.
fn max_temp(root: &Path) -> String {
    let dir = root.join("sys/class/thermal");
    let hottest = dir_names(&dir, "thermal_zone")
        .into_iter()
        .filter_map(|z| Some((read_i64(&dir.join(&z).join("temp"))?, z)))
        .max_by_key(|(t, _)| *t);
    match hottest {
        Some((t, zone)) => format!("{}.{}C@z{}", t / 1000, (t % 1000).abs() / 100, zone.trim_start_matches("thermal_zone")),
        None => "?".into(),
    }
}

fn gpu_mhz(root: &Path) -> String {
    let dir = root.join("sys/class/devfreq");
    dir_names(&dir, "")
        .into_iter()
        .filter(|n| n.ends_with(".gpu"))
        .find_map(|n| read_i64(&dir.join(n).join("cur_freq")))
        .map_or("?".into(), |hz| format!("{}MHz", hz / 1_000_000))
}

/// `p0=ondemand/1804MHz p4=...` for every cpufreq policy.
fn cpu_policies(root: &Path) -> String {
    let dir = root.join("sys/devices/system/cpu/cpufreq");
    let v: Vec<String> = dir_names(&dir, "policy")
        .into_iter()
        .map(|p| {
            let gov = read_trim(&dir.join(&p).join("scaling_governor")).unwrap_or_else(|| "?".into());
            let mhz = read_i64(&dir.join(&p).join("scaling_cur_freq")).map_or("?".into(), |khz| format!("{}MHz", khz / 1000));
            format!("p{}={gov}/{mhz}", p.trim_start_matches("policy"))
        })
        .collect();
    if v.is_empty() {
        "cpu=?".into()
    } else {
        v.join(" ")
    }
}

/// `4400rpm` of the PWM fan (its hwmon is named `pwmfan`; any hwmon with a tachometer will do), else its PWM duty.
fn fan(root: &Path) -> String {
    let dir = root.join("sys/class/hwmon");
    let hwmons = dir_names(&dir, "hwmon");
    let named = |h: &&String| read_trim(&dir.join(h).join("name")).is_some_and(|n| n == "pwmfan");
    let pick = hwmons
        .iter()
        .find(named)
        .or_else(|| hwmons.iter().find(|h| dir.join(h).join("fan1_input").exists()));
    let Some(h) = pick else { return "?".into() };
    match (read_i64(&dir.join(h).join("fan1_input")), read_i64(&dir.join(h).join("pwm1"))) {
        (Some(rpm), _) => format!("{rpm}rpm"),
        (None, Some(pwm)) => format!("pwm{pwm}"),
        _ => "?".into(),
    }
}

/// `81% Charging 3.95V 420mA`.
fn battery(dir: &Path) -> String {
    let cap = read_trim(&dir.join("capacity")).map_or("?".into(), |c| format!("{c}%"));
    let status = read_trim(&dir.join("status")).unwrap_or_else(|| "?".into());
    let volts = read_i64(&dir.join("voltage_now")).map_or("?".into(), |uv| format!("{}.{:02}V", uv / 1_000_000, (uv % 1_000_000).abs() / 10_000));
    let ma = read_i64(&dir.join("current_now")).map_or("?".into(), |ua| format!("{}mA", ua / 1000));
    format!("{cap} {status} {volts} {ma}")
}

/// One line for sensors.log.
pub fn sensors_line(root: &Path, battery_dir: &Path, wall: u64, uptime: f64) -> String {
    format!(
        "{} up={}s temp={} gpu={} {} fan={} bat={}\n",
        crate::crash::timestamp(wall),
        uptime as u64,
        max_temp(root),
        gpu_mhz(root),
        cpu_policies(root),
        fan(root),
        battery(battery_dir)
    )
}

pub(crate) fn read_uptime(path: &Path) -> f64 {
    read_trim(path).and_then(|s| s.split_whitespace().next()?.parse().ok()).unwrap_or(0.0)
}

// ---- the logger ---------------------------------------------------------------------------------------

/// Where the logger reads from; tests point these at a folder.
#[derive(Debug, Clone)]
pub struct Sources {
    pub kmsg: PathBuf,
    pub ui_log: PathBuf,
    /// The root `sys/...` is below.
    pub sysfs: PathBuf,
    pub battery: PathBuf,
    pub uptime: PathBuf,
    pub net: crate::net_log::NetSources,
}

impl Sources {
    pub fn system() -> Sources {
        Sources {
            kmsg: "/dev/kmsg".into(),
            ui_log: std::env::var_os("POCKETUI_LOG").map_or_else(|| "/var/log/pocket-ui.log".into(), PathBuf::from),
            sysfs: "/".into(),
            battery: crate::battery::dir(),
            uptime: "/proc/uptime".into(),
            net: crate::net_log::NetSources::system(),
        }
    }
}

pub struct Daemon {
    logs: PathBuf,
    src: Sources,
    ui: Rolling,
    kernel: Rolling,
    sensors: Rolling,
    net: Rolling,
    net_feed: crate::net_log::NetFeed,
    tail: Tail,
    kmsg: Option<File>,
    /// The newest kernel record on the card: a reader that starts again within a boot gets the whole ring buffer
    /// again, and this keeps it from being written twice.
    last_usec: Option<u64>,
    next_sample: Instant,
    next_sync: Instant,
    next_sensors_sync: Instant,
}

impl Daemon {
    /// Opens the logs of the current boot in `logs` (`begin_boot` has been called) and `/dev/kmsg`.
    pub fn open(logs: &Path, src: Sources, cap: u64, now: Instant) -> Daemon {
        let mut kernel = Rolling::new(logs.join(KERNEL_LOG), cap);
        let last_usec = tail_lines(logs, KERNEL_LOG, 1).last().and_then(|l| stamp_usec(l));
        let kmsg = OpenOptions::new().read(true).custom_flags(libc::O_NONBLOCK).open(&src.kmsg);
        let kmsg = match kmsg {
            Ok(f) => Some(f),
            Err(e) => {
                kernel.append(&format!("[kernel messages are not readable here: {e}]\n"));
                None
            }
        };
        let net_last = tail_lines(logs, NET_LOG, 200).iter().rev().find_map(|l| net_journal_stamp(l));
        Daemon {
            logs: logs.to_path_buf(),
            net: Rolling::new(logs.join(NET_LOG), cap),
            net_feed: crate::net_log::NetFeed::new(src.net.clone(), net_last, now),
            ui: Rolling::new(logs.join(UI_LOG), cap),
            sensors: Rolling::new(logs.join(SENSORS_LOG), cap),
            tail: Tail::new(src.ui_log.clone()),
            kernel,
            kmsg,
            last_usec,
            src,
            next_sample: now,
            next_sync: now + SYNC_EVERY,
            next_sensors_sync: now + SENSORS_SYNC_EVERY,
        }
    }

    /// The descriptor to wait on for kernel messages.
    pub fn kmsg_fd(&self) -> Option<RawFd> {
        self.kmsg.as_ref().map(AsRawFd::as_raw_fd)
    }

    /// How long to sleep from `now` until there is something to do: never longer than [`TICK`] (the log in RAM has no
    /// notification) and never a busy loop.
    pub fn poll_timeout(&self, now: Instant) -> Duration {
        let due = self.next_sample.min(self.next_sync).min(self.next_sensors_sync);
        due.saturating_duration_since(now).min(TICK).max(MIN_STEP_GAP)
    }

    /// Everything that is new in the kernel's ring buffer, formatted, and whether any of it is urgent.
    fn read_kmsg(&mut self) -> (String, bool) {
        let mut out = String::new();
        let mut urgent = false;
        let Some(f) = self.kmsg.as_mut() else { return (out, urgent) };
        let mut buf = [0u8; 8192];
        for _ in 0..4096 {
            match f.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    for line in String::from_utf8_lossy(&buf[..n]).lines() {
                        // Continuation lines (" SUBSYSTEM=...") carry device details, not the message.
                        if line.is_empty() || line.starts_with(' ') {
                            continue;
                        }
                        let Some(m) = parse_kmsg(line) else { continue };
                        if self.last_usec.is_some_and(|last| m.usec <= last) {
                            continue;
                        }
                        self.last_usec = Some(m.usec);
                        urgent |= is_urgent(m.level);
                        out.push_str(&format_kmsg(&m));
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                // The reader fell behind and records were overwritten: the next read goes on with the oldest left.
                Err(e) if e.raw_os_error() == Some(libc::EPIPE) => continue,
                Err(_) => {
                    self.kmsg = None;
                    break;
                }
            }
        }
        (out, urgent)
    }

    /// One round: reads what is new (kernel, pocket-ui's log), takes a sample when it is time, writes it all with one
    /// `write` per file, and fsyncs what is due. `wall` is the clock's Unix time.
    pub fn step(&mut self, now: Instant, wall: u64) {
        let uptime = read_uptime(&self.src.uptime);
        let (kernel, urgent) = self.read_kmsg();
        self.kernel.append(&kernel);

        let (lines, restarted) = self.tail.poll();
        if restarted || !lines.is_empty() {
            let mut text = String::new();
            if restarted {
                text.push_str(&format!("[{uptime:>9.3}] --- {} started over ---\n", self.src.ui_log.display()));
            }
            for l in &lines {
                text.push_str(&format!("[{uptime:>9.3}] {l}\n"));
            }
            self.ui.append(&text);
        }

        let net = self.net_feed.poll(now, uptime);
        self.net.append(&net);

        if now >= self.next_sample {
            self.next_sample = now + SAMPLE_EVERY;
            self.sensors.append(&sensors_line(&self.src.sysfs, &self.src.battery, wall, uptime));
        }

        // A kernel warning is what a freeze announces itself with: everything goes to the card now.
        if urgent {
            self.sync_all(now);
            return;
        }
        if now >= self.next_sync {
            self.next_sync = now + SYNC_EVERY;
            self.ui.sync();
            self.kernel.sync();
        }
        if now >= self.next_sensors_sync {
            self.next_sensors_sync = now + SENSORS_SYNC_EVERY;
            self.sensors.sync();
            self.net.sync();
        }
    }

    fn sync_all(&mut self, now: Instant) {
        self.ui.sync();
        self.kernel.sync();
        self.sensors.sync();
        self.net.sync();
        self.next_sync = now + SYNC_EVERY;
        self.next_sensors_sync = now + SENSORS_SYNC_EVERY;
    }

    /// Asked to stop: takes the last lines, fsyncs, and leaves the `clean-exit` marker.
    pub fn finish(&mut self, now: Instant, wall: u64) {
        self.step(now, wall);
        if let Some(rest) = self.tail.flush() {
            self.ui.append(&format!("[{:>9.3}] {rest}\n", read_uptime(&self.src.uptime)));
        }
        self.net_feed.stop();
        self.sync_all(now);
        let _ = fs::write(self.logs.join(CLEAN_FILE), crate::crash::timestamp(wall));
    }
}

static STOP: AtomicBool = AtomicBool::new(false);

extern "C" fn on_stop(_: libc::c_int) {
    STOP.store(true, Ordering::SeqCst);
}

/// `pocket-core --log-daemon`: the logger the launcher starts (see the module doc). Returns the exit code.
pub fn run_daemon() -> i32 {
    let data_dir = PathBuf::from(std::env::var("POCKETUI_DATA").unwrap_or_else(|_| "/storage/.config/pocket-ui".into()));
    let logs = data_dir.join(DIR_NAME);
    if let Err(e) = fs::create_dir_all(&logs) {
        eprintln!("logs: cannot create {}: {e}", logs.display());
        return 1;
    }
    // One logger at a time: a launcher started by hand next to a running one leaves quietly.
    let Some(_single) = lock(&logs.join(DAEMON_LOCK_FILE), false) else { return 0 };
    #[cfg(target_os = "linux")]
    // SAFETY: PR_SET_NAME takes a NUL-terminated name of up to 15 bytes. The name of its own (even when it is started
    // as pocket-core) keeps `another_pocket_core_runs`, which goes by it, from taking the logger for a game.
    unsafe {
        libc::prctl(libc::PR_SET_NAME, c"pocket-logd".as_ptr());
    }
    let Some(id) = boot_id() else {
        eprintln!("logs: no boot id here (not Linux?), nothing to log");
        return 1;
    };
    begin_boot(&data_dir, &id, crate::recents::now_secs());
    // SAFETY: the handler only stores into an atomic. Without SA_RESTART `poll` returns at once.
    unsafe {
        for sig in [libc::SIGTERM, libc::SIGINT, libc::SIGHUP] {
            let mut action: libc::sigaction = std::mem::zeroed();
            action.sa_sigaction = on_stop as extern "C" fn(libc::c_int) as usize;
            libc::sigemptyset(&mut action.sa_mask);
            libc::sigaction(sig, &action, std::ptr::null_mut());
        }
    }
    let mut daemon = Daemon::open(&logs, Sources::system(), FILE_CAP, Instant::now());
    eprintln!("logs: writing to {}", logs.display());
    while !STOP.load(Ordering::SeqCst) {
        let now = Instant::now();
        daemon.step(now, crate::recents::now_secs());
        let mut pfd = libc::pollfd {
            fd: daemon.kmsg_fd().unwrap_or(-1),
            events: libc::POLLIN,
            revents: 0,
        };
        let ms = daemon.poll_timeout(Instant::now()).as_millis() as libc::c_int;
        // SAFETY: one pollfd (a negative descriptor is ignored: then this is just the sleep).
        let ready = unsafe { libc::poll(&mut pfd, 1, ms) };
        if ready > 0 {
            // A chatty kernel does not make this a busy loop.
            std::thread::sleep(MIN_STEP_GAP);
        }
    }
    daemon.finish(Instant::now(), crate::recents::now_secs());
    0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;

    fn tmp(name: &str) -> crate::testutil::TestDir {
        let d = TestDir::new(&format!("logs-{name}"));
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn put(root: &Path, rel: &str, text: &str) {
        let p = root.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, text).unwrap();
    }

    fn size(p: &Path) -> u64 {
        fs::metadata(p).map_or(0, |m| m.len())
    }

    fn logs_of(data: &Path) -> PathBuf {
        data.join(DIR_NAME)
    }

    // ---- rotation per boot

    /// A boot that has been through the logger: its three logs, and the marker when it was stopped politely.
    fn live_through_a_boot(data: &Path, id: &str, now: u64, clean: bool) -> Option<Unclean> {
        let u = begin_boot(data, id, now);
        let logs = logs_of(data);
        fs::write(logs.join(UI_LOG), format!("ui of {id}\n")).unwrap();
        fs::write(
            logs.join(KERNEL_LOG),
            format!("[    1.000000] info  boot {id}\n[    2.000000] err    last words of {id}\n"),
        )
        .unwrap();
        fs::write(logs.join(SENSORS_LOG), format!("2026-10-22 14:00:00 UTC up=2s temp=45.0C@z6 sample of {id}\n")).unwrap();
        if clean {
            fs::write(logs.join(CLEAN_FILE), "bye").unwrap();
        }
        u
    }

    #[test]
    fn the_first_boot_has_nothing_before_it() {
        let data = tmp("first");
        assert_eq!(begin_boot(&data, "boot-a", 1_791_900_000), None);
        let state = read_state(&logs_of(&data)).unwrap();
        assert_eq!(
            state,
            State {
                id: "boot-a".into(),
                seq: 1,
                started: 1_791_900_000
            }
        );
        assert!(!data.join(crate::crash::LOG_FILE).exists(), "no crash report for a first start");
        assert!(crate::crash::take_marker(&data).is_none());
    }

    #[test]
    fn the_same_boot_again_changes_nothing_but_removes_the_stop_marker() {
        let data = tmp("same");
        live_through_a_boot(&data, "boot-a", 1_791_900_000, true);
        assert_eq!(begin_boot(&data, "boot-a", 1_791_900_500), None);
        let logs = logs_of(&data);
        assert!(!logs.join(CLEAN_FILE).exists(), "the logger runs again: the boot is not over");
        assert_eq!(fs::read_to_string(logs.join(UI_LOG)).unwrap(), "ui of boot-a\n", "nothing was moved");
        assert_eq!(read_state(&logs).unwrap().started, 1_791_900_000, "still the same boot");
        assert!(!fs::read_dir(&logs)
            .unwrap()
            .flatten()
            .any(|e| e.file_name().to_string_lossy().starts_with("boot-")));
    }

    #[test]
    fn a_new_boot_moves_the_old_files_into_a_folder_named_by_boot_time() {
        let data = tmp("rotate");
        live_through_a_boot(&data, "boot-a", 1_791_900_000, true);
        assert_eq!(begin_boot(&data, "boot-b", 1_791_990_000), None, "it was a clean one");
        let logs = logs_of(&data);
        let dir = logs.join("boot-0001-20261013-140000");
        assert_eq!(fs::read_to_string(dir.join(UI_LOG)).unwrap(), "ui of boot-a\n");
        assert!(fs::read_to_string(dir.join(KERNEL_LOG)).unwrap().contains("last words of boot-a"));
        assert!(dir.join(SENSORS_LOG).exists() && dir.join(CLEAN_FILE).exists() && dir.join(STATE_FILE).exists());
        for name in STREAMS {
            assert!(!logs.join(name).exists(), "{name} starts empty for the new boot");
        }
        let state = read_state(&logs).unwrap();
        assert_eq!((state.id.as_str(), state.seq, state.started), ("boot-b", 2, 1_791_990_000));
    }

    #[test]
    fn the_saved_sign_in_page_moves_with_the_boot_into_its_folder() {
        let data = tmp("portal-last");
        live_through_a_boot(&data, "boot-a", 1_791_900_000, true);
        save_portal_page(&data, "<a href=\"/c?client=AA:BB:CC:DD:EE:FF\">x</a>").unwrap();
        let logs = logs_of(&data);
        assert_eq!(fs::read_to_string(logs.join(PORTAL_LAST)).unwrap(), "<a href=\"/c?client=[mac]\">x</a>", "saved redacted");
        begin_boot(&data, "boot-b", 1_791_990_000);
        let dir = logs.join("boot-0001-20261013-140000");
        assert_eq!(fs::read_to_string(dir.join(PORTAL_LAST)).unwrap(), "<a href=\"/c?client=[mac]\">x</a>", "kept with the boot it came from");
        assert!(!logs.join(PORTAL_LAST).exists(), "the new boot starts without one");
    }

    #[test]
    fn only_the_last_three_boots_are_kept() {
        let data = tmp("keep");
        for i in 0..6u64 {
            live_through_a_boot(&data, &format!("boot-{i}"), 1_791_900_000 + i * 10_000, true);
        }
        let logs = logs_of(&data);
        let mut dirs: Vec<String> = fs::read_dir(&logs)
            .unwrap()
            .flatten()
            .filter(|e| e.path().is_dir())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        dirs.sort();
        assert_eq!(dirs.len(), KEEP_BOOTS - 1, "two folders and the current boot's files: {dirs:?}");
        assert!(
            dirs[0].starts_with("boot-0004-") && dirs[1].starts_with("boot-0005-"),
            "the newest ones: {dirs:?}"
        );
        assert_eq!(fs::read_to_string(logs.join(UI_LOG)).unwrap(), "ui of boot-5\n");
    }

    #[test]
    fn a_boot_may_have_been_started_by_pocket_ui_and_the_logger_in_either_order() {
        let data = tmp("race");
        live_through_a_boot(&data, "boot-a", 1_791_900_000, true);
        begin_boot(&data, "boot-b", 1_791_990_000);
        let before = fs::read_dir(logs_of(&data)).unwrap().count();
        // the second caller of the same boot finds it started
        assert_eq!(begin_boot(&data, "boot-b", 1_791_990_001), None);
        assert_eq!(fs::read_dir(logs_of(&data)).unwrap().count(), before);
        assert_eq!(read_state(&logs_of(&data)).unwrap().seq, 2, "one rotation, not two");
    }

    // ---- unclean shutdown

    #[test]
    fn a_boot_without_the_marker_ended_without_a_clean_shutdown() {
        let data = tmp("unclean");
        live_through_a_boot(&data, "boot-a", 1_791_900_000, false);
        let u = begin_boot(&data, "boot-b", 1_791_990_000).expect("no clean-exit: unclean");
        assert_eq!(u.kernel_tail.len(), 2);
        assert!(u.kernel_tail[1].contains("last words of boot-a"), "{u:?}");
        assert!(u.sensors_last.as_deref().is_some_and(|s| s.contains("sample of boot-a")));
        assert!(u.last_alive > 1_700_000_000, "the files' own write time: {}", u.last_alive);
        // the report
        let crash = fs::read_to_string(data.join(crate::crash::LOG_FILE)).unwrap();
        assert_eq!(crash.lines().count(), 1, "one line: {crash}");
        assert!(crash.contains("previous boot ended without a clean shutdown at "), "{crash}");
        assert!(crash.contains(&crate::crash::timestamp(u.last_alive)), "{crash}");
        assert!(crash.contains("last kernel lines: ") && crash.contains("last words of boot-a"), "{crash}");
        // the notice: the existing crash notice, a plain sentence, shown once
        let notice = crate::crash::take_marker(&data).expect("a notice for the shelf");
        assert_eq!(notice, "The device shut down unexpectedly last time.");
        assert!(crate::crash::take_marker(&data).is_none());
    }

    #[test]
    fn a_clean_shutdown_leaves_no_report_and_no_notice() {
        let data = tmp("clean");
        live_through_a_boot(&data, "boot-a", 1_791_900_000, true);
        assert_eq!(begin_boot(&data, "boot-b", 1_791_990_000), None);
        assert!(!data.join(crate::crash::LOG_FILE).exists());
        assert!(crate::crash::take_marker(&data).is_none());
    }

    #[test]
    fn two_unclean_boots_in_a_row_are_reported_twice() {
        let data = tmp("twice");
        live_through_a_boot(&data, "boot-a", 1_791_900_000, false);
        live_through_a_boot(&data, "boot-b", 1_791_990_000, false);
        assert!(begin_boot(&data, "boot-c", 1_792_000_000).is_some());
        let crash = fs::read_to_string(data.join(crate::crash::LOG_FILE)).unwrap();
        assert_eq!(crash.lines().count(), 2, "{crash}");
        assert!(crash.lines().nth(1).unwrap().contains("last words of boot-b"));
    }

    #[test]
    fn an_empty_previous_boot_still_counts_when_nothing_says_it_was_clean() {
        let data = tmp("empty");
        begin_boot(&data, "boot-a", 1_791_900_000);
        let u = begin_boot(&data, "boot-b", 1_791_990_000).expect("unclean");
        assert!(u.kernel_tail.is_empty());
        assert_eq!(u.last_alive, 1_791_900_000, "no file was ever written: the boot's start");
        let crash = fs::read_to_string(data.join(crate::crash::LOG_FILE)).unwrap();
        assert!(crash.contains("last kernel lines: "), "{crash}");
    }

    // ---- size cap

    #[test]
    fn a_log_stays_within_its_cap_and_keeps_the_newest_lines() {
        let dir = tmp("cap");
        let path = dir.join("x.log");
        let mut r = Rolling::new(path.clone(), 10_000);
        let line = format!("{}\n", "x".repeat(99));
        for i in 0..1000 {
            r.append(&format!("{i:04}{line}"));
        }
        let (cur, old) = (size(&path), size(&dir.join("x.log.old")));
        assert!(cur + old <= 10_000 + 103, "{cur} + {old}");
        assert!(old > 0, "the segment before the current one is kept");
        assert!(fs::read_to_string(&path).unwrap().contains("0999x"), "the newest line is in the current file");
        let old_text = fs::read_to_string(dir.join("x.log.old")).unwrap();
        let first_old: u32 = old_text.lines().next().unwrap()[..4].parse().unwrap();
        assert!(first_old > 900, "the old file holds the lines just before the current one, from {first_old}");
        let all = fs::read_to_string(&path).unwrap() + &fs::read_to_string(dir.join("x.log.old")).unwrap();
        assert!(!all.contains("0000x"), "the oldest are gone");
        assert!(all.lines().all(|l| l.len() == 103), "rolled at a line boundary");
    }

    #[test]
    fn the_cap_holds_over_a_restart_of_the_logger() {
        let dir = tmp("cap2");
        let path = dir.join("x.log");
        for _ in 0..4 {
            let mut r = Rolling::new(path.clone(), 4_000);
            for _ in 0..30 {
                r.append(&format!("{}\n", "y".repeat(99)));
            }
        }
        assert!(size(&path) + size(&dir.join("x.log.old")) <= 4_000 + 100);
    }

    #[test]
    fn appending_to_an_unwritable_place_is_not_fatal() {
        let mut r = Rolling::new(PathBuf::from("/proc/definitely/not/here.log"), 1000);
        r.append("lost\n");
        r.sync();
    }

    #[test]
    fn syncing_only_happens_when_something_is_new() {
        let dir = tmp("dirty");
        let mut r = Rolling::new(dir.join("x.log"), 1000);
        assert!(!r.dirty());
        r.append("a\n");
        assert!(r.dirty());
        r.sync();
        assert!(!r.dirty());
        r.append("");
        assert!(!r.dirty(), "no text, no write, nothing to fsync");
    }

    // ---- kernel messages

    #[test]
    fn kernel_records_are_parsed_and_formatted_like_dmesg() {
        let m = parse_kmsg("4,339,5140900432,-;thermal thermal_zone6: critical temperature reached").unwrap();
        assert_eq!((m.level, m.usec), (4, 5_140_900_432));
        assert_eq!(format_kmsg(&m), "[ 5140.900432] warn   thermal thermal_zone6: critical temperature reached\n");
        // facility bits above the level are not the level; ';' inside the text stays
        let m = parse_kmsg("30,1,17,-;a;b").unwrap();
        assert_eq!((m.level, m.text.as_str()), (6, "a;b"));
        assert!(parse_kmsg("garbage").is_none());
        assert!(parse_kmsg("x,1,2,-;t").is_none());
    }

    #[test]
    fn warnings_and_worse_are_urgent() {
        assert!((0..=4).all(is_urgent));
        assert!((5..=7).all(|l| !is_urgent(l)));
    }

    #[test]
    fn a_formatted_line_gives_its_time_back() {
        let line = format_kmsg(&Kmsg {
            level: 6,
            usec: 5_140_900_432,
            text: "x".into(),
        });
        assert_eq!(stamp_usec(&line), Some(5_140_900_432));
        assert_eq!(stamp_usec("no stamp"), None);
    }

    // ---- the logger

    /// A fake device: sysfs files, a kernel ring buffer (a plain file), pocket-ui's log, an uptime.
    struct Rig {
        data: crate::testutil::TestDir,
        logs: PathBuf,
        src: Sources,
    }

    fn rig(name: &str) -> Rig {
        let data = tmp(name);
        let root = data.join("fake");
        put(&root, "sys/class/thermal/thermal_zone0/temp", "41000\n");
        put(&root, "sys/class/thermal/thermal_zone6/temp", "61300\n");
        put(&root, "sys/class/thermal/thermal_zone2/temp", "52000\n");
        put(&root, "sys/class/devfreq/3d00000.gpu/cur_freq", "587000000\n");
        put(&root, "sys/class/devfreq/other.thing/cur_freq", "1000\n");
        put(&root, "sys/devices/system/cpu/cpufreq/policy0/scaling_governor", "ondemand\n");
        put(&root, "sys/devices/system/cpu/cpufreq/policy0/scaling_cur_freq", "1804800\n");
        put(&root, "sys/devices/system/cpu/cpufreq/policy4/scaling_governor", "performance\n");
        put(&root, "sys/devices/system/cpu/cpufreq/policy4/scaling_cur_freq", "2419200\n");
        put(&root, "sys/class/hwmon/hwmon3/name", "pwmfan\n");
        put(&root, "sys/class/hwmon/hwmon3/fan1_input", "4400\n");
        put(&root, "sys/class/hwmon/hwmon1/name", "other\n");
        put(&root, "battery/capacity", "81\n");
        put(&root, "battery/status", "Discharging\n");
        put(&root, "battery/voltage_now", "3951000\n");
        put(&root, "battery/current_now", "420000\n");
        put(&root, "uptime", "1234.56 4000.00\n");
        put(&root, "kmsg", "");
        put(&root, "pocket-ui.log", "");
        let logs = data.join(DIR_NAME);
        fs::create_dir_all(&logs).unwrap();
        let src = Sources {
            kmsg: root.join("kmsg"),
            ui_log: root.join("pocket-ui.log"),
            sysfs: root.clone(),
            battery: root.join("battery"),
            uptime: root.join("uptime"),
            net: crate::net_log::NetSources::none(),
        };
        Rig { data, logs, src }
    }

    fn append(path: &Path, text: &str) {
        let mut f = OpenOptions::new().append(true).open(path).unwrap();
        f.write_all(text.as_bytes()).unwrap();
    }

    fn read(path: &Path) -> String {
        fs::read_to_string(path).unwrap_or_default()
    }

    #[test]
    fn a_sample_has_the_numbers_a_freeze_needs() {
        let r = rig("sample");
        let line = sensors_line(&r.src.sysfs, &r.src.battery, 1_791_900_000, 1234.56);
        assert_eq!(
            line,
            "2026-10-13 14:00:00 UTC up=1234s temp=61.3C@z6 gpu=587MHz p0=ondemand/1804MHz p4=performance/2419MHz fan=4400rpm bat=81% Discharging 3.95V 420mA\n"
        );
        assert_eq!(line.lines().count(), 1);
    }

    #[test]
    fn a_sample_on_a_device_without_the_files_says_so_instead_of_failing() {
        let empty = tmp("nosys");
        let line = sensors_line(&empty, &empty.join("bat"), 0, 0.0);
        assert!(line.contains("temp=? gpu=? cpu=? fan=? bat=? ? ? ?"), "{line}");
    }

    #[test]
    fn the_fan_is_read_from_the_pwm_fan_and_falls_back_to_its_duty() {
        let r = rig("fan");
        assert_eq!(fan(&r.src.sysfs), "4400rpm");
        let root = tmp("fan2");
        put(&root, "sys/class/hwmon/hwmon0/name", "pwmfan\n");
        put(&root, "sys/class/hwmon/hwmon0/pwm1", "51\n");
        assert_eq!(fan(&root), "pwm51");
    }

    #[test]
    fn cpu_policies_are_listed_in_numeric_order() {
        let root = tmp("order");
        for p in ["policy10", "policy2", "policy0"] {
            put(&root, &format!("sys/devices/system/cpu/cpufreq/{p}/scaling_governor"), "x\n");
        }
        assert!(cpu_policies(&root).starts_with("p0=x/? p2=x/? p10="), "{}", cpu_policies(&root));
    }

    #[test]
    fn the_logger_copies_kernel_messages_pocket_ui_and_the_sensors() {
        let r = rig("daemon");
        append(&r.src.kmsg, "6,1,1000000,-;hello kernel\n SUBSYSTEM=net\n4,2,2500000,-;thermal warning\n");
        append(&r.src.ui_log, "first line\nsecond line\n");
        let mut d = Daemon::open(&r.logs, r.src.clone(), FILE_CAP, Instant::now());
        d.step(Instant::now(), 1_791_900_000);
        let kernel = read(&r.logs.join(KERNEL_LOG));
        assert_eq!(
            kernel, "[    1.000000] info   hello kernel\n[    2.500000] warn   thermal warning\n",
            "no continuation lines"
        );
        assert_eq!(read(&r.logs.join(UI_LOG)), "[ 1234.560] first line\n[ 1234.560] second line\n");
        assert!(read(&r.logs.join(SENSORS_LOG)).contains("temp=61.3C@z6"), "the first sample comes at once");
    }

    #[test]
    fn the_sampler_runs_every_five_seconds_and_not_between() {
        let r = rig("period");
        let t0 = Instant::now();
        let mut d = Daemon::open(&r.logs, r.src.clone(), FILE_CAP, t0);
        for s in 0..16 {
            d.step(t0 + Duration::from_secs(s), 1_791_900_000 + s);
        }
        let samples = read(&r.logs.join(SENSORS_LOG)).lines().count();
        assert_eq!(samples, 4, "t = 0, 5, 10, 15");
    }

    #[test]
    fn an_idle_logger_writes_nothing() {
        let r = rig("idle");
        let t0 = Instant::now();
        let mut d = Daemon::open(&r.logs, r.src.clone(), FILE_CAP, t0);
        d.step(t0, 1_791_900_000);
        let (k, u) = (size(&r.logs.join(KERNEL_LOG)), size(&r.logs.join(UI_LOG)));
        for s in 1..4 {
            d.step(t0 + Duration::from_secs(s), 1_791_900_000 + s);
        }
        assert_eq!((size(&r.logs.join(KERNEL_LOG)), size(&r.logs.join(UI_LOG))), (k, u));
        assert_eq!(k, 0);
        assert_eq!(u, 0);
    }

    #[test]
    fn a_restarted_logger_does_not_write_the_ring_buffer_twice() {
        let r = rig("replay");
        append(&r.src.kmsg, "6,1,1000000,-;one\n6,2,2000000,-;two\n");
        let mut d = Daemon::open(&r.logs, r.src.clone(), FILE_CAP, Instant::now());
        d.step(Instant::now(), 1);
        drop(d);
        // a new reader gets the whole ring buffer again, now with one more record
        append(&r.src.kmsg, "6,3,3000000,-;three\n");
        let mut d = Daemon::open(&r.logs, r.src.clone(), FILE_CAP, Instant::now());
        d.step(Instant::now(), 2);
        let kernel = read(&r.logs.join(KERNEL_LOG));
        assert_eq!(kernel.matches("one").count() + kernel.matches("two").count(), 2, "{kernel}");
        assert_eq!(kernel.matches("three").count(), 1);
    }

    #[test]
    fn the_launcher_starting_the_log_over_is_noted_and_followed() {
        let r = rig("restart");
        let mut d = Daemon::open(&r.logs, r.src.clone(), FILE_CAP, Instant::now());
        append(&r.src.ui_log, "a long first line of the first run\n");
        d.step(Instant::now(), 1);
        fs::write(&r.src.ui_log, "new\n").unwrap();
        d.step(Instant::now(), 2);
        let ui = read(&r.logs.join(UI_LOG));
        assert!(ui.contains("started over"), "{ui}");
        assert!(ui.ends_with("] new\n"), "{ui}");
        assert!(ui.contains("a long first line"), "what was logged before stays");
    }

    #[test]
    fn a_half_written_line_waits_for_its_end() {
        let r = rig("partial");
        let mut d = Daemon::open(&r.logs, r.src.clone(), FILE_CAP, Instant::now());
        append(&r.src.ui_log, "half a li");
        d.step(Instant::now(), 1);
        assert_eq!(read(&r.logs.join(UI_LOG)), "");
        append(&r.src.ui_log, "ne\n");
        d.step(Instant::now(), 2);
        assert!(read(&r.logs.join(UI_LOG)).ends_with("] half a line\n"));
    }

    #[test]
    fn the_logger_writes_net_log_from_the_journal_and_keeps_it_with_the_boot() {
        let mut r = rig("netlog");
        r.src.net = crate::net_log::NetSources {
            journal: vec!["sh".into(), "-c".into(), "printf '[    5.000000] h iwd[1]: psk=abc\\n[    6.000000] h NetworkManager[2]: <info> ok\\n'; sleep 3".into()],
            ..crate::net_log::NetSources::none()
        };
        begin_boot(&r.data, "boot-a", 1_791_900_000);
        let t0 = Instant::now();
        let mut d = Daemon::open(&r.logs, r.src.clone(), FILE_CAP, t0);
        let mut n = 0;
        while !read(&r.logs.join(NET_LOG)).contains("<info> ok") && n < 100 {
            d.step(t0 + Duration::from_millis(100 * n), 1_791_900_000);
            std::thread::sleep(Duration::from_millis(20));
            n += 1;
        }
        let net = read(&r.logs.join(NET_LOG));
        assert!(net.contains("iwd[1]: psk=[hidden]\n") && net.contains("NetworkManager[2]: <info> ok\n"), "{net}");
        d.sync_all(t0);
        assert!(!d.net.dirty(), "fsynced with the others");
        d.finish(t0, 1_791_900_001);
        begin_boot(&r.data, "boot-next", 1_791_990_000);
        let moved: Vec<_> = fs::read_dir(&r.logs).unwrap().flatten().filter(|e| e.path().is_dir()).collect();
        assert!(moved[0].path().join(NET_LOG).exists(), "net.log moves with the boot");
    }

    #[test]
    fn a_kernel_warning_goes_to_the_card_at_once_and_the_rest_waits_for_the_period() {
        let r = rig("urgent");
        let t0 = Instant::now();
        let mut d = Daemon::open(&r.logs, r.src.clone(), FILE_CAP, t0);
        d.step(t0, 1);
        append(&r.src.kmsg, "6,1,1000000,-;just info\n");
        d.step(t0 + Duration::from_secs(1), 2);
        assert!(d.kernel.dirty(), "an info line waits for the next fsync");
        append(&r.src.kmsg, "4,2,2000000,-;WARNING: something\n");
        d.step(t0 + Duration::from_secs(2), 3);
        assert!(!d.kernel.dirty() && !d.ui.dirty() && !d.sensors.dirty(), "a warning syncs everything now");
        append(&r.src.kmsg, "6,3,3000000,-;more info\n");
        d.step(t0 + Duration::from_secs(3), 4);
        assert!(d.kernel.dirty());
        d.step(t0 + Duration::from_secs(8), 9);
        assert!(!d.kernel.dirty(), "{SYNC_EVERY:?} after the last sync");
    }

    #[test]
    fn the_sensors_are_synced_every_fifteen_seconds_not_every_sample() {
        let r = rig("sensorsync");
        let t0 = Instant::now();
        let mut d = Daemon::open(&r.logs, r.src.clone(), FILE_CAP, t0);
        d.step(t0, 1);
        assert!(d.sensors.dirty());
        d.step(t0 + Duration::from_secs(5), 6);
        d.step(t0 + Duration::from_secs(10), 11);
        assert!(d.sensors.dirty(), "samples at 5 and 10 s are still in the page cache");
        d.step(t0 + Duration::from_secs(15), 16);
        assert!(!d.sensors.dirty());
    }

    #[test]
    fn the_logger_sleeps_between_rounds_without_busy_looping() {
        let r = rig("wake");
        let t0 = Instant::now();
        let mut d = Daemon::open(&r.logs, r.src.clone(), FILE_CAP, t0);
        d.step(t0, 1);
        let wait = d.poll_timeout(t0);
        assert_eq!(wait, TICK, "idle: one wake-up a second, no more");
        assert!(d.poll_timeout(t0 + Duration::from_secs(5)) >= MIN_STEP_GAP, "even when something is due");
        assert!(MIN_STEP_GAP >= Duration::from_millis(100) && TICK >= Duration::from_secs(1));
        assert!(SAMPLE_EVERY >= Duration::from_secs(5) && SYNC_EVERY >= Duration::from_secs(5));
    }

    #[test]
    fn a_logger_that_is_asked_to_stop_marks_the_boot_clean() {
        let r = rig("finish");
        begin_boot(&r.data, "boot-a", 1_791_900_000);
        let t0 = Instant::now();
        let mut d = Daemon::open(&r.logs, r.src.clone(), FILE_CAP, t0);
        append(&r.src.ui_log, "last words, no newline");
        d.step(t0, 1_791_900_001);
        d.finish(t0 + Duration::from_secs(1), 1_791_900_002);
        assert!(r.logs.join(CLEAN_FILE).exists());
        assert!(read(&r.logs.join(UI_LOG)).contains("last words, no newline"), "the unfinished line is kept");
        assert!(!d.ui.dirty() && !d.kernel.dirty() && !d.sensors.dirty());
        assert_eq!(begin_boot(&r.data, "boot-b", 1_791_990_000), None, "the next boot sees a clean one");
        assert!(crate::crash::take_marker(&r.data).is_none());
    }

    #[test]
    fn a_logger_that_died_leaves_the_boot_unclean() {
        let r = rig("died");
        begin_boot(&r.data, "boot-a", 1_791_900_000);
        let t0 = Instant::now();
        let mut d = Daemon::open(&r.logs, r.src.clone(), FILE_CAP, t0);
        append(&r.src.kmsg, "4,1,9000000,-;the last thing the kernel said\n");
        d.step(t0, 1_791_900_001);
        drop(d);
        let u = begin_boot(&r.data, "boot-b", 1_791_990_000).expect("no marker: unclean");
        assert_eq!(u.kernel_tail.len(), 1);
        assert!(u.kernel_tail[0].contains("the last thing the kernel said"));
        assert!(fs::read_to_string(r.data.join(crate::crash::LOG_FILE))
            .unwrap()
            .contains("the last thing the kernel said"));
    }

    #[test]
    fn an_unreadable_kernel_log_is_noted_and_the_rest_goes_on() {
        let mut r = rig("nokmsg");
        r.src.kmsg = PathBuf::from("/proc/no/such/kmsg");
        let t0 = Instant::now();
        let mut d = Daemon::open(&r.logs, r.src.clone(), FILE_CAP, t0);
        assert!(d.kmsg_fd().is_none());
        d.step(t0, 1);
        assert!(read(&r.logs.join(KERNEL_LOG)).contains("not readable here"));
        assert!(read(&r.logs.join(SENSORS_LOG)).contains("temp="));
    }

    #[test]
    fn only_one_logger_runs() {
        let dir = tmp("single");
        let first = lock(&dir.join(".d"), false);
        assert!(first.is_some());
        assert!(lock(&dir.join(".d"), false).is_none(), "the second one leaves");
        // A duplicated description models the descriptor briefly inherited by a concurrently spawned helper.
        let inherited = first.as_ref().unwrap()._file.try_clone().unwrap();
        drop(first);
        assert!(lock(&dir.join(".d"), false).is_some(), "free again once the first one is gone");
        drop(inherited);
    }

    #[test]
    fn boot_folder_names_sort_in_boot_order() {
        assert_eq!(stamp(1_791_900_000), "20261013-140000");
        assert!(format!("boot-{:04}-x", 9) < format!("boot-{:04}-x", 10));
    }
}
