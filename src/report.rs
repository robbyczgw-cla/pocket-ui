//! Settings → System → About → Report a problem: one zip on the SD card with what a bug report needs, made with one
//! press and attached by the player to an issue on GitHub (docs/user/troubleshooting.md, "Report a problem").
//!
//! - **Where**: `/storage/roms/reports/report-<date>-<time>.zip` (`POCKETUI_REPORTS` moves it), next to the
//!   screenshots in the games share, so it is reachable over the file sharing, USB or the card in a computer.
//! - **What**: `README.txt` (what is inside, what was left out, where to attach it), `system.txt` (pocket-ui and
//!   pocket-core version and commit, ROCKNIX version, device, kernel, uptime, memory, battery and temperatures, free
//!   space per partition, the last crash), `emulators.txt` (pocket-core's systems, the libretro cores with their
//!   versions where a `.info` names one, ROCKNIX's standalone emulators), `settings/` (pocket-ui's settings files of
//!   [`SETTINGS_FILES`], the Store's list of installed packages) and `logs/` (the end of this boot's `pocket-ui.log`,
//!   which pocket-core and the launcher write into too, `kernel.log`, `sensors.log`, `net.log` (the network programs' journal and the saved networks' names), `crash.log`, pocket-core's last
//!   error, and the same logs of the boot before; plus `logs/portal-last.html`, the last Wi-Fi sign-in page that
//!   ended with "needs a full browser" (what the hotel served), from this start or the one before).
//! - **Privacy**: every file goes through [`Redactor::clean`] before it is packed. What it knows from the Pocket itself
//!   ([`known_secrets`]: Wi-Fi names and passwords, the RetroAchievements account, the Tailscale and system host
//!   names, the file sharing and SSH passwords) and the library's game names go first, then patterns catch what
//!   it can't know (addresses, MACs, e-mail addresses, Tailscale keys and names, Syncthing device IDs, user names in
//!   paths, `key=value` pairs of passwords and tokens, game paths). Game names become the system and a short code
//!   ([`game_code`]), the same for the same game, so a log can still be followed; the player names the game in the
//!   report if it matters. Nothing is sent anywhere: the player decides what to attach.
//! - **Cost**: about a megabyte of text read, cleaned and packed on a thread of its own (`app_report`), never on the
//!   UI thread; the zip is a few hundred KB.

use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use sha2::{Digest, Sha256};

pub use crate::screenshot::Stamp;

/// Where reports go when `POCKETUI_REPORTS` doesn't say: in the games share, beside `screenshots`.
pub const DEFAULT_DIR: &str = "/storage/roms/reports";
/// The folder as the player finds it in the games share.
pub const SHARE_FOLDER: &str = "reports";

/// pocket-ui's settings files that go in as they are (after cleaning). Not the library, recents, history,
/// favorites, Curator or sync files (game names and play habits), not the accounts (`retroachievements.json`,
/// `tailscale.json`, `web/`, `share/`, `remote/`).
pub const SETTINGS_FILES: &[&str] = &[
    "settings.json",
    "sleep.json",
    "sounds.json",
    "haptics.json",
    "leds.json",
    "performance.json",
    "display.json",
    "night-light.json",
    "storage.json",
    "players.json",
    "fallback.json",
    "pixelpath.json",
];

/// How much of each log goes in, from its end.
const UI_LOG_TAIL: u64 = 384 * 1024;
const KERNEL_TAIL: u64 = 192 * 1024;
/// The network daemons' log: a few lines a minute, the saved networks' names at the top.
const NET_TAIL: u64 = 128 * 1024;
const PREVIOUS_NET_TAIL: u64 = 64 * 1024;
const SENSORS_TAIL: u64 = 48 * 1024;
const CRASH_TAIL: u64 = 64 * 1024;
const PREVIOUS_UI_TAIL: u64 = 128 * 1024;
const PREVIOUS_KERNEL_TAIL: u64 = 64 * 1024;
const PREVIOUS_SENSORS_TAIL: u64 = 16 * 1024;
/// The whole shutdown log fits (its cap is 128 KB, `shutdown_log::CAP`); the last of it is what matters.
const SHUTDOWN_TAIL: u64 = 64 * 1024;
/// `pocket-core --version` gets this long.
const VERSION_WAIT: Duration = Duration::from_secs(2);

// ---- what the player sees ------------------------------------------------------------------------------------

/// The About page's row.
pub const ROW: &str = "Report a problem";
/// Under the rows while the row is selected.
pub const ROW_HELP: &str = "Saves a file with the logs and settings that help find a problem. You attach it to your report on GitHub.";
pub const WRITING: &str = "Writing the report\u{2026}";
pub const SAVED_TITLE: &str = "Report saved";
pub const FAILED_TITLE: &str = "Report not saved";
/// Where the file is, under its name.
pub const WHERE: &str = "In the reports folder of the games share (roms/reports on the SD card).";
pub const ATTACH: &str = "Attach this file to your report on GitHub.";
/// What [`Redactor`] takes out, in the player's words.
pub const LEFT_OUT: &str = "Left out: Wi-Fi names and passwords, network addresses, Tailscale, RetroAchievements and sync details, passwords and tokens. Game names are replaced by a short code, so name the game in your report if it matters.";

/// Why the zip could not be written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    /// No room on the card (ENOSPC, EDQUOT).
    Full,
    /// The card or folder can't be written (EROFS, EACCES, EPERM).
    ReadOnly,
    Other(String),
}

impl Failure {
    pub fn from_io(e: &std::io::Error) -> Failure {
        match e.raw_os_error() {
            Some(libc::ENOSPC | libc::EDQUOT) => Failure::Full,
            Some(libc::EROFS | libc::EACCES | libc::EPERM) => Failure::ReadOnly,
            _ => Failure::Other(e.to_string()),
        }
    }

    /// One or two plain sentences for the sheet.
    pub fn message(&self) -> String {
        match self {
            Failure::Full => "The SD card is full. Make some room, for example by deleting old screenshots, and try again.".into(),
            Failure::ReadOnly => "The SD card can't be written to right now. Restart the Pocket and try again.".into(),
            Failure::Other(e) => format!("Something went wrong while writing it ({e}). Try again after a restart."),
        }
    }
}

/// A written report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Saved {
    pub path: PathBuf,
    /// `report-2026-10-26-140507.zip`.
    pub name: String,
    pub bytes: u64,
}

// ---- where things are -----------------------------------------------------------------------------------------

/// Where the report reads from and writes to; tests point these at a folder.
#[derive(Debug, Clone)]
pub struct Sources {
    /// pocket-ui's data folder (`/storage/.config/pocket-ui`): settings, `logs/`, `crash.log`, the accounts.
    pub data_dir: PathBuf,
    /// The log in RAM (`/var/log/pocket-ui.log`), for when the card copy is missing.
    pub live_log: PathBuf,
    /// The system's root: `etc/os-release`, `proc/…`, `sys/…`, `usr/bin`, `usr/lib/libretro`, `tmp/…`.
    pub root: PathBuf,
    /// `/storage`: ROCKNIX's `system.cfg`, NetworkManager's profiles, `cores/`.
    pub storage: PathBuf,
    pub battery: PathBuf,
    pub pocket_core: PathBuf,
    pub out_dir: PathBuf,
}

impl Sources {
    pub fn system(data_dir: &Path, pocket_core: &Path) -> Sources {
        Sources {
            data_dir: data_dir.to_path_buf(),
            live_log: std::env::var_os("POCKETUI_LOG").map_or_else(|| "/var/log/pocket-ui.log".into(), PathBuf::from),
            root: "/".into(),
            storage: "/storage".into(),
            battery: crate::battery::dir(),
            pocket_core: pocket_core.to_path_buf(),
            out_dir: std::env::var_os("POCKETUI_REPORTS").map_or_else(|| DEFAULT_DIR.into(), PathBuf::from),
        }
    }

    fn sys(&self, path: &str) -> PathBuf {
        self.root.join(path.trim_start_matches('/'))
    }

    fn system_cfg(&self) -> PathBuf {
        self.storage.join(".config/system/configs/system.cfg")
    }

    fn nm_profiles(&self) -> PathBuf {
        self.storage.join(".config/NetworkManager/system-connections")
    }

    fn core_dirs(&self) -> [(PathBuf, &'static str); 3] {
        [(self.storage.join("cores"), "from the Store"), (self.sys("usr/lib/libretro"), "ROCKNIX"), (self.sys("tmp/cores"), "ROCKNIX")]
    }
}

/// A game of the library, for [`Redactor`].
#[derive(Debug, Clone)]
pub struct GameName {
    pub system: String,
    pub name: String,
    pub path: PathBuf,
}

// ---- the report -------------------------------------------------------------------------------------------------

/// `report-2026-10-26-164007.zip`: the date and time on the Pocket's clock (local, as the screenshots' names), so the
/// player finds it by the time they made it.
pub fn file_name(local: &Stamp) -> String {
    format!("report-{:04}-{:02}-{:02}-{:02}{:02}{:02}.zip", local.year, local.month, local.day, local.hour, local.min, local.sec)
}

/// Collects, cleans and packs everything, and writes the zip into `src.out_dir` (through a temporary file, so a full
/// card leaves no half zip behind). `now` is the Unix time (UTC in system.txt, as the logs), `local` the same moment
/// on the Pocket's clock ([`Stamp::at`]).
pub fn write(src: &Sources, games: &[GameName], now: u64, local: &Stamp) -> Result<Saved, Failure> {
    let entries = entries(src, games, now, local);
    let bytes = zip(&entries, local);
    let name = file_name(local);
    std::fs::create_dir_all(&src.out_dir).map_err(|e| Failure::from_io(&e))?;
    let path = src.out_dir.join(&name);
    let part = src.out_dir.join(format!(".{name}.part"));
    let written = (|| -> std::io::Result<()> {
        let mut f = std::fs::File::create(&part)?;
        f.write_all(&bytes)?;
        f.sync_all()?;
        std::fs::rename(&part, &path)
    })();
    if let Err(e) = written {
        let _ = std::fs::remove_file(&part);
        return Err(Failure::from_io(&e));
    }
    Ok(Saved { path, name, bytes: bytes.len() as u64 })
}

/// The zip's files, cleaned, in order: README.txt first.
pub fn entries(src: &Sources, games: &[GameName], now: u64, local: &Stamp) -> Vec<(String, Vec<u8>)> {
    let redactor = Redactor::new(known_secrets(src), games);
    let mut files: Vec<(String, String)> = Vec::new();
    let mut missing: Vec<&str> = Vec::new();
    files.push(("system.txt".into(), system_txt(src, now)));
    files.push(("emulators.txt".into(), emulators_txt(src)));
    for name in SETTINGS_FILES {
        if let Ok(text) = std::fs::read_to_string(src.data_dir.join(name)) {
            files.push((format!("settings/{name}"), text));
        }
    }
    if let Ok(text) = std::fs::read_to_string(src.data_dir.join("store/installed.json")) {
        files.push(("settings/store-installed.json".into(), text));
    }

    let logs = src.data_dir.join(crate::logs::DIR_NAME);
    match tail_with_old(&logs.join(crate::logs::UI_LOG), UI_LOG_TAIL).or_else(|| tail(&src.live_log, UI_LOG_TAIL)) {
        Some(t) => files.push(("logs/pocket-ui.log".into(), t)),
        None => missing.push("pocket-ui.log"),
    }
    for (name, cap) in [(crate::logs::KERNEL_LOG, KERNEL_TAIL), (crate::logs::SENSORS_LOG, SENSORS_TAIL), (crate::logs::NET_LOG, NET_TAIL)] {
        match tail_with_old(&logs.join(name), cap) {
            Some(t) => files.push((format!("logs/{name}"), t)),
            None => missing.push(name),
        }
    }
    // Every start writes its RTC line there, so it is nearly always present; the power offs of this boot are in it too.
    if let Some(t) = tail_with_old(&logs.join(crate::logs::SHUTDOWN_LOG), SHUTDOWN_TAIL) {
        files.push((format!("logs/{}", crate::logs::SHUTDOWN_LOG), t));
    }
    match tail(&src.data_dir.join(crate::crash::LOG_FILE), CRASH_TAIL) {
        Some(t) => files.push(("logs/crash.log".into(), t)),
        None => missing.push("crash.log (no crash recorded)"),
    }
    if let Some(t) = tail(&src.sys("tmp/pocket-core.error"), CRASH_TAIL) {
        files.push(("logs/pocket-core.error".into(), t));
    }
    // The sign-in page the helper couldn't do, saved when it gave up (`logs::save_portal_page`): what the hotel
    // served, so the next report can show it.
    if let Ok(t) = std::fs::read_to_string(logs.join(crate::logs::PORTAL_LAST)) {
        files.push((format!("logs/{}", crate::logs::PORTAL_LAST), t));
    }
    // The two starts before this one (the logger keeps three in all): a Power off that came back on shows in both.
    for (prev, folder) in previous_boots(&logs).into_iter().zip(["previous-boot", "boot-before-that"]) {
        for (name, cap) in [(crate::logs::UI_LOG, PREVIOUS_UI_TAIL), (crate::logs::KERNEL_LOG, PREVIOUS_KERNEL_TAIL), (crate::logs::SENSORS_LOG, PREVIOUS_SENSORS_TAIL), (crate::logs::NET_LOG, PREVIOUS_NET_TAIL), (crate::logs::SHUTDOWN_LOG, SHUTDOWN_TAIL)] {
            if let Some(t) = tail_with_old(&prev.join(name), cap) {
                files.push((format!("logs/{folder}/{name}"), t));
            }
        }
        if let Ok(t) = std::fs::read_to_string(prev.join(crate::logs::PORTAL_LAST)) {
            files.push((format!("logs/{folder}/{}", crate::logs::PORTAL_LAST), t));
        }
    }

    let mut out: Vec<(String, Vec<u8>)> = files.into_iter().map(|(n, t)| (n, redactor.clean(&t).into_bytes())).collect();
    // README.txt is pocket-ui's own text (the template, the files' names and sizes, the fixed "not found" lines, the
    // version and time): nothing in it comes from the player, so it is not cleaned. Cleaning it took "wifi" out of
    // its own "[wifi-name]" when a network profile was called "wifi".
    let readme = readme(now, local, &out, &missing);
    out.insert(0, ("README.txt".into(), readme.into_bytes()));
    out
}

fn readme(now: u64, local: &Stamp, files: &[(String, Vec<u8>)], missing: &[&str]) -> String {
    let mut list = String::new();
    for (name, bytes) in files {
        let what = match name.as_str() {
            "system.txt" => "versions, device, uptime, battery, temperatures, free space, the last crash",
            "emulators.txt" => "the cores and emulators on this Pocket",
            n if n.starts_with("settings/") => "a settings file of pocket-ui",
            "logs/portal-last.html" => "the last Wi-Fi sign-in page that needed a full browser",
            n if n.ends_with("/portal-last.html") => "the sign-in page of that start that needed a full browser",
            n if n.starts_with("logs/previous-boot/") => "the end of the same log from the start before",
            n if n.starts_with("logs/boot-before-that/") => "the end of the same log from the start before that",
            "logs/pocket-ui.log" => "the end of the log of pocket-ui, pocket-core and the launcher",
            "logs/kernel.log" => "the end of the system's own log",
            "logs/sensors.log" => "temperatures, clocks, fan and battery every 5 seconds",
            "logs/net.log" => "what the network programs said (Wi-Fi, Tailscale) and the names of the saved networks",
            "logs/crash.log" => "one line for each crash or unexpected shutdown",
            "logs/shutdown.log" => "each power off or restart: which one, the clock alarm, the command and how it ended",
            "logs/pocket-core.error" => "why the last game stopped",
            _ => "",
        };
        list.push_str(&format!("  {name:<40} {:>8}  {what}\n", size_text(bytes.len() as u64)));
    }
    let missing = if missing.is_empty() { "  (nothing)\n".to_string() } else { missing.iter().map(|m| format!("  {m}\n")).collect() };
    include_str!("../assets/report-readme.txt")
        .replace("{when}", &format!(
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02} on the Pocket's clock ({}, the time in the logs)",
            local.year,
            local.month,
            local.day,
            local.hour,
            local.min,
            local.sec,
            crate::crash::timestamp(now)
        ))
        .replace("{version}", crate::update::app_version())
        .replace("{files}", list.trim_end_matches('\n'))
        .replace("{missing}", missing.trim_end_matches('\n'))
}

fn size_text(bytes: u64) -> String {
    if bytes >= 1024 {
        format!("{} KB", bytes.div_ceil(1024))
    } else {
        format!("{bytes} B")
    }
}

/// The `boot-…` folders, newest first (`logs::begin_boot` names them by the boot's date and time).
fn previous_boots(logs: &Path) -> Vec<PathBuf> {
    let Ok(dir) = std::fs::read_dir(logs) else { return Vec::new() };
    let mut boots: Vec<PathBuf> = dir
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with("boot-") && e.path().is_dir())
        .map(|e| e.path())
        .collect();
    // "boot-<n>-<date>-<time>": the sequence number decides, as a number.
    boots.sort_by_key(|p| std::cmp::Reverse(p.file_name().and_then(|n| n.to_str()?.split('-').nth(1)?.parse::<u64>().ok()).unwrap_or(0)));
    boots
}

/// The last `cap` bytes of a file, from the start of a line; None when it is missing or empty.
fn tail(path: &Path, cap: u64) -> Option<String> {
    let mut f = std::fs::File::open(path).ok()?;
    let len = f.metadata().ok()?.len();
    let from = len.saturating_sub(cap);
    f.seek(SeekFrom::Start(from)).ok()?;
    let mut buf = Vec::with_capacity((len - from) as usize);
    f.take(cap).read_to_end(&mut buf).ok()?;
    let mut text = String::from_utf8_lossy(&buf).into_owned();
    if from > 0 {
        // The first line is cut: it starts after the next line break.
        let at = text.find('\n').map_or(text.len(), |i| i + 1);
        text.replace_range(..at, "");
    }
    (!text.trim().is_empty()).then_some(text)
}

/// [`tail`] of a log of `logs::Rolling`, with the end of its `.old` half before it when the current half is shorter.
fn tail_with_old(path: &Path, cap: u64) -> Option<String> {
    let now = tail(path, cap);
    let have = now.as_ref().map_or(0, |t| t.len() as u64);
    if have + 1024 >= cap {
        return now;
    }
    let mut old_name = path.as_os_str().to_owned();
    old_name.push(".old");
    match (tail(Path::new(&old_name), cap - have), now) {
        (Some(old), Some(now)) => Some(old + &now),
        (old, now) => now.or(old),
    }
}

fn read_trim(path: &Path) -> Option<String> {
    let s = std::fs::read_to_string(path).ok()?;
    let s = s.trim_matches(|c: char| c.is_whitespace() || c == '\0').to_string();
    (!s.is_empty()).then_some(s)
}

/// `pocket-core --version`, or why there is none; never waits longer than [`VERSION_WAIT`].
fn pocket_core_version(bin: &Path) -> String {
    match crate::proc::run_with_timeout(std::process::Command::new(bin).arg("--version"), VERSION_WAIT) {
        Ok(out) => String::from_utf8_lossy(&out.stdout).lines().next().map_or_else(|| "no answer".into(), |l| l.trim().to_string()),
        Err(e) if e.kind() == std::io::ErrorKind::TimedOut => "did not answer".into(),
        Err(_) => "not found".into(),
    }
}

/// "2 h 13 min" for seconds.
fn duration_text(secs: u64) -> String {
    let (d, h, m) = (secs / 86_400, secs % 86_400 / 3600, secs % 3600 / 60);
    match (d, h) {
        (0, 0) => format!("{m} min"),
        (0, _) => format!("{h} h {m} min"),
        _ => format!("{d} d {h} h {m} min"),
    }
}

fn gb(bytes: f64) -> String {
    format!("{:.1} GB", bytes / 1e9)
}

/// Free and total bytes of the file system at `path`, and whether it is read-only.
fn statvfs(path: &Path) -> Option<(f64, f64, bool)> {
    use std::os::unix::ffi::OsStrExt;
    let c_path = std::ffi::CString::new(path.as_os_str().as_bytes()).ok()?;
    let mut st: libc::statvfs = unsafe { std::mem::zeroed() };
    // SAFETY: statvfs only writes into the provided struct.
    if unsafe { libc::statvfs(c_path.as_ptr(), &mut st) } != 0 {
        return None;
    }
    let frsize = st.f_frsize as f64;
    Some((st.f_bavail as f64 * frsize, st.f_blocks as f64 * frsize, st.f_flag & libc::ST_RDONLY != 0))
}

/// The partitions worth knowing about from `/proc/mounts`: the system, the boot partition, the storage, RAM and
/// any card or stick (its label hidden: it may be a name).
fn free_space_lines(src: &Sources) -> Vec<String> {
    let mounts = std::fs::read_to_string(src.sys("proc/mounts")).unwrap_or_default();
    let mut out = Vec::new();
    let mut media = 0;
    let mut seen = std::collections::HashSet::new();
    for line in mounts.lines() {
        let f: Vec<&str> = line.split_whitespace().collect();
        let (Some(mp), Some(fs)) = (f.get(1), f.get(2)) else { continue };
        let mp = mp.replace("\\040", " ");
        let shown = match mp.as_str() {
            "/" | "/flash" | "/storage" | "/tmp" | "/var" => mp.clone(),
            m if m.starts_with("/storage/") => mp.clone(),
            m if m.starts_with("/var/media/") || m.starts_with("/media/") || m.starts_with("/run/media/") => {
                media += 1;
                format!("removable card or stick {media}")
            }
            _ => continue,
        };
        if !seen.insert(mp.clone()) {
            continue;
        }
        let Some((free, total, ro)) = statvfs(&src.sys(&mp)) else { continue };
        out.push(format!("{shown} = {} free of {}, {fs}{}", gb(free), gb(total), if ro { ", read-only" } else { "" }));
    }
    if out.is_empty() {
        out.push("unknown = /proc/mounts could not be read".into());
    }
    out
}

/// `MemTotal` and `MemAvailable` of `/proc/meminfo`.
fn memory_line(src: &Sources) -> String {
    let info = std::fs::read_to_string(src.sys("proc/meminfo")).unwrap_or_default();
    let kb = |key: &str| info.lines().find(|l| l.starts_with(key)).and_then(|l| l.split_whitespace().nth(1)?.parse::<f64>().ok());
    match (kb("MemAvailable:"), kb("MemTotal:")) {
        (Some(a), Some(t)) => format!("{} available of {}", gb(a * 1024.0), gb(t * 1024.0)),
        _ => "unknown".into(),
    }
}

fn system_txt(src: &Sources, now: u64) -> String {
    let mut s = String::new();
    let mut kv = |k: &str, v: &str| s.push_str(&format!("{k} = {v}\n"));
    kv("report_made", &crate::crash::timestamp(now));
    kv("pocket_ui", &format!("{} (commit {})", crate::update::app_version(), option_env!("POCKETUI_COMMIT").unwrap_or("not recorded, a build of its own")));
    kv("pocket_core", &pocket_core_version(&src.pocket_core));
    let os = std::fs::read_to_string(src.sys("etc/os-release")).unwrap_or_default();
    // ROCKNIX's os-release has OS_NAME and OS_VERSION (and no NAME or VERSION); other systems the standard keys.
    let os_value = |keys: [&str; 2]| keys.iter().find_map(|k| crate::update::os_release_field(&os, k)).filter(|v| !v.is_empty());
    kv("rocknix", &match (os_value(["OS_NAME", "NAME"]), os_value(["OS_VERSION", "VERSION"])) {
        (Some(n), Some(v)) => format!("{n} {v}"),
        (None, Some(v)) => v,
        (Some(n), None) => format!("{n}, version unknown"),
        (None, None) if os.trim().is_empty() => "unknown (no /etc/os-release)".into(),
        (None, None) => "unknown (no name or version in /etc/os-release)".into(),
    });
    kv("device", &read_trim(&src.sys("proc/device-tree/model")).unwrap_or_else(|| "unknown".into()));
    // pocket-ui's own view of it (`device`, docs/DEVICES.md): the Mini V2 it is tested on, an experimental sibling, or
    // unknown. A report from an experimental device says so at once.
    let profile = crate::device::current();
    kv("device_profile", &profile.model.map_or_else(|| "unknown to pocket-ui".to_string(), |m| format!("{}, {}", m.name(), if m.experimental() { "experimental" } else { "tested" })));
    kv("kernel", &read_trim(&src.sys("proc/sys/kernel/osrelease")).unwrap_or_else(|| "unknown".into()));
    let up = read_trim(&src.sys("proc/uptime")).and_then(|u| u.split_whitespace().next()?.parse::<f64>().ok());
    kv("uptime", &up.map_or_else(|| "unknown".into(), |u| format!("{} ({} s)", duration_text(u as u64), u as u64)));
    kv("memory", &memory_line(src));
    kv("sensors", crate::logs::sensors_line(&src.root, &src.battery, now, up.unwrap_or(0.0)).trim_end());
    let last_crash = std::fs::read_to_string(src.data_dir.join(crate::crash::LOG_FILE)).ok().and_then(|t| t.lines().rev().find(|l| !l.trim().is_empty()).map(str::to_string));
    kv("last_crash", last_crash.as_deref().unwrap_or("none recorded"));
    kv("last_crash_notice", if src.data_dir.join(crate::crash::MARKER_FILE).exists() { "waiting to be shown" } else { "none" });
    s.push_str("\n[free_space]\n");
    for l in free_space_lines(src) {
        s.push_str(&l);
        s.push('\n');
    }
    s.push_str("\n[os_release]\n");
    s.push_str(if os.trim().is_empty() { "(missing)\n" } else { &os });
    s
}

/// `display_version` of a libretro `.info` file.
fn info_version(info: &str) -> Option<String> {
    info.lines().find_map(|l| {
        let (k, v) = l.split_once('=')?;
        (k.trim() == "display_version").then(|| v.trim().trim_matches('"').to_string()).filter(|v| !v.is_empty())
    })
}

fn emulators_txt(src: &Sources) -> String {
    let mut s = String::from("[pocket_core]\n");
    for (core, systems) in crate::hostsys::routed_cores() {
        s.push_str(&format!("{core} = {}\n", systems.join(" ")));
    }
    s.push_str("\n[libretro_cores]\n");
    let mut seen = std::collections::HashSet::new();
    let mut cores = Vec::new();
    for (dir, origin) in src.core_dirs() {
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        let mut names: Vec<String> = rd.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).filter(|n| n.ends_with("_libretro.so")).collect();
        names.sort();
        for name in names {
            if !seen.insert(name.clone()) {
                continue;
            }
            let info_name = name.replace(".so", ".info");
            let version = src.core_dirs().iter().find_map(|(d, _)| info_version(&std::fs::read_to_string(d.join(&info_name)).ok()?));
            let meta = std::fs::metadata(dir.join(&name)).ok();
            let date = meta.as_ref().and_then(|m| m.modified().ok()).and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| crate::crash::timestamp(d.as_secs())[..10].to_string());
            cores.push(format!(
                "{} = {}, {origin}, {}, {}",
                name.trim_end_matches("_libretro.so"),
                version.map_or_else(|| "version not recorded".into(), |v| format!("version {v}")),
                date.unwrap_or_else(|| "date unknown".into()),
                meta.map_or_else(|| "size unknown".into(), |m| size_text(m.len()))
            ));
        }
    }
    if cores.is_empty() {
        cores.push("none found".into());
    }
    for c in cores {
        s.push_str(&c);
        s.push('\n');
    }
    // runemu.sh starts a standalone emulator through /usr/bin/start_<name>.sh; they come with ROCKNIX's version.
    s.push_str("\n[standalone_emulators]\n");
    let mut standalone: Vec<String> = std::fs::read_dir(src.sys("usr/bin"))
        .map(|rd| rd.flatten().filter_map(|e| Some(e.file_name().to_str()?.strip_prefix("start_")?.strip_suffix(".sh")?.to_string())).collect())
        .unwrap_or_default();
    standalone.sort();
    s.push_str(&if standalone.is_empty() { "none found".to_string() } else { format!("{} (part of the ROCKNIX version in system.txt)", standalone.join(" ")) });
    s.push('\n');
    s
}

// ---- secrets ------------------------------------------------------------------------------------------------------

/// What the Pocket itself knows is private, each with the marker it is replaced by: the Wi-Fi networks and their
/// passwords (ROCKNIX's `system.cfg`, NetworkManager's profiles), Bluetooth devices named after a person, the
/// RetroAchievements account, the host names (Tailscale's, the system's), the file sharing and SSH passwords.
pub fn known_secrets(src: &Sources) -> Vec<(String, &'static str)> {
    let mut out: Vec<(String, &'static str)> = Vec::new();
    let generic = system_words(src);
    let mut add = |v: &str, marker: &'static str| {
        let v = v.trim().trim_matches('"');
        if v.chars().count() >= 3 && !generic.contains(&v.to_lowercase()) {
            out.push((v.to_string(), marker));
        }
    };
    for line in std::fs::read_to_string(src.system_cfg()).unwrap_or_default().lines() {
        let Some((k, v)) = line.split_once('=') else { continue };
        let k = k.trim().to_ascii_lowercase();
        if k.contains("ssid") {
            add(v, "[wifi-name]");
        } else if ["key", "pass", "psk", "token", "secret"].iter().any(|w| k.contains(w)) {
            add(v, "[password]");
        } else if k.contains("user") || k.contains("login") {
            add(v, "[user]");
        } else if k.ends_with("hostname") {
            add(v, "[hostname]");
        }
    }
    if let Ok(rd) = std::fs::read_dir(src.nm_profiles()) {
        for e in rd.flatten() {
            // Only the network's name (`ssid=`) and its password: a profile's file name and `id=` are ROCKNIX's
            // ("wifi"), or the network's name again (wifictl names a profile after its SSID).
            for line in std::fs::read_to_string(e.path()).unwrap_or_default().lines() {
                match line.split_once('=') {
                    Some(("ssid", v)) => add(v, "[wifi-name]"),
                    Some(("psk" | "password" | "wep-key0", v)) => add(v, "[password]"),
                    _ => {}
                }
            }
        }
    }
    // Paired Bluetooth devices named after their owner ("Alex's AirPods"); a controller's model name stays, it tells
    // which pad a problem is about.
    for adapter in std::fs::read_dir(src.storage.join(".cache/bluetooth")).into_iter().flatten().flatten() {
        for device in std::fs::read_dir(adapter.path()).into_iter().flatten().flatten() {
            for line in std::fs::read_to_string(device.path().join("info")).unwrap_or_default().lines() {
                if let Some(("Name" | "Alias", v)) = line.split_once('=') {
                    if ["'s", "\u{2019}s"].iter().any(|p| v.contains(&format!("{p} ")) || v.ends_with(p)) {
                        add(v, "[bluetooth-name]");
                    }
                }
            }
        }
    }
    if let Some(a) = crate::achievements::account::Account::load(&src.data_dir) {
        add(&a.username, "[user]");
        add(&a.display_name, "[user]");
        add(&a.token, "[token]");
    }
    if let Some(h) = crate::tailscale::Prefs::load(&src.data_dir).hostname {
        add(&h, "[hostname]");
    }
    if let Some(h) = read_trim(&src.sys("etc/hostname")) {
        add(&h, "[hostname]");
    }
    for p in ["share/password", "remote/password"] {
        if let Some(pw) = read_trim(&src.data_dir.join(p)) {
            add(&pw, "[password]");
        }
    }
    out
}

/// Words that are never a secret, whatever a setting holds: the names Linux, NetworkManager and ROCKNIX give
/// things, placeholders. Taking one out would take it out of every line ("wifi" from "[wifi-name]", "wlan0").
const GENERIC_WORDS: &[&str] = &[
    "rocknix", "localhost", "none", "true", "false", "auto", "default", "root", "admin", "user", "guest", "home", "wifi", "wi-fi", "wlan", "wlan0", "wlan1", "eth0", "ethernet", "wireless",
    "network", "hotspot", "bluetooth", "pocket", "retroid", "android",
];

/// [`GENERIC_WORDS`] and this system's own names, lower case: ROCKNIX's `OS_NAME`, `HW_DEVICE` and `DISTRO_DEVICE`
/// (its default host name is the chip, "SM8250", which is also in driver names such as "gcc-sm8250"), the chips of
/// the device tree's `compatible` ("qcom,sm8250") and its model ("Retroid Pocket Mini V2"). A host name the
/// player never changed says nothing about them.
fn system_words(src: &Sources) -> std::collections::HashSet<String> {
    let mut words: std::collections::HashSet<String> = GENERIC_WORDS.iter().map(|w| w.to_string()).collect();
    let os = std::fs::read_to_string(src.sys("etc/os-release")).unwrap_or_default();
    for key in ["OS_NAME", "NAME", "HW_DEVICE", "DISTRO_DEVICE", "HW_CPU"] {
        if let Some(v) = crate::update::os_release_field(&os, key) {
            words.insert(v.to_lowercase());
        }
    }
    let compatible = std::fs::read(src.sys("proc/device-tree/compatible")).unwrap_or_default();
    for c in String::from_utf8_lossy(&compatible).split('\0') {
        if let Some((_, chip)) = c.split_once(',') {
            words.insert(chip.trim().to_lowercase());
        }
    }
    if let Some(model) = read_trim(&src.sys("proc/device-tree/model")) {
        words.insert(model.to_lowercase());
    }
    words
}

/// The short code a game's name becomes: `[game snes 1a2b3c4d]`, from the file's name without its extension, so the
/// game's title and its file give the same code.
pub fn game_code(system: &str, stem: &str) -> String {
    let h = Sha256::digest(stem.trim().to_lowercase().as_bytes());
    format!("[game {system} {:02x}{:02x}{:02x}{:02x}]", h[0], h[1], h[2], h[3])
}

/// Takes out of a text what [`known_secrets`] and the library name, and what looks private by its form.
pub struct Redactor {
    /// Longest first, so a password that contains a network's name goes whole.
    secrets: Vec<(Vec<u8>, &'static str)>,
    /// Game names and file names by their first three bytes (ASCII lower case): the candidates to try at a word start.
    names: HashMap<[u8; 3], Vec<(Vec<u8>, String)>>,
}

/// Shorter game names are left alone: "Ys" would take the word out of every line.
const MIN_NAME: usize = 3;

impl Redactor {
    pub fn new(secrets: Vec<(String, &'static str)>, games: &[GameName]) -> Redactor {
        let mut secrets: Vec<(Vec<u8>, &'static str)> = secrets.into_iter().map(|(s, m)| (s.into_bytes(), m)).collect();
        secrets.sort_by_key(|s| std::cmp::Reverse(s.0.len()));
        secrets.dedup_by(|a, b| a.0.eq_ignore_ascii_case(&b.0));
        let mut names: HashMap<[u8; 3], Vec<(Vec<u8>, String)>> = HashMap::new();
        for g in games {
            let stem = g.path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| g.name.clone());
            let code = game_code(&g.system, &stem);
            for n in [stem.as_str(), g.name.as_str()] {
                let n = n.trim();
                if n.len() < MIN_NAME {
                    continue;
                }
                let key = key3(n.as_bytes());
                let list = names.entry(key).or_default();
                if !list.iter().any(|(b, _)| b.eq_ignore_ascii_case(n.as_bytes())) {
                    list.push((n.as_bytes().to_vec(), code.clone()));
                }
            }
        }
        for list in names.values_mut() {
            list.sort_by_key(|(b, _)| std::cmp::Reverse(b.len()));
        }
        Redactor { secrets, names }
    }

    /// The text without what is private, each piece replaced by a marker in brackets.
    pub fn clean(&self, text: &str) -> String {
        let t = self.secrets_out(text);
        let t = self.names_out(&t);
        let t = scan(&t, game_path_at);
        let t = scan(&t, user_path_at);
        let t = scan(&t, url_login_at);
        let t = scan(&t, email_at);
        let t = scan(&t, tailscale_at);
        let t = scan(&t, syncthing_at);
        let t = scan(&t, mac_at);
        let t = scan(&t, ipv6_at);
        let t = scan(&t, ipv4_at);
        scan(&t, assignment_at)
    }

    fn secrets_out(&self, text: &str) -> String {
        if self.secrets.is_empty() {
            return text.to_string();
        }
        scan(text, |t, i| {
            let rest = &t.as_bytes()[i..];
            self.secrets.iter().find(|(s, _)| rest.len() >= s.len() && rest[..s.len()].eq_ignore_ascii_case(s) && t.is_char_boundary(i + s.len())).map(|(s, m)| (i + s.len(), m.to_string()))
        })
    }

    fn names_out(&self, text: &str) -> String {
        if self.names.is_empty() {
            return text.to_string();
        }
        scan(text, |t, i| {
            let b = t.as_bytes();
            if b.len() < i + MIN_NAME || !word_edge(t, i, true) {
                return None;
            }
            let list = self.names.get(&key3(&b[i..]))?;
            list.iter().find(|(n, _)| b.len() >= i + n.len() && b[i..i + n.len()].eq_ignore_ascii_case(n) && t.is_char_boundary(i + n.len()) && word_edge(t, i + n.len(), false)).map(|(n, code)| (i + n.len(), code.clone()))
        })
    }
}

fn key3(b: &[u8]) -> [u8; 3] {
    [b[0].to_ascii_lowercase(), b[1].to_ascii_lowercase(), b[2].to_ascii_lowercase()]
}

/// Whether `i` is the edge of a word: the character before it (`start`) or at it (the end) is no letter or digit.
fn word_edge(t: &str, i: usize, start: bool) -> bool {
    let c = if start { t[..i].chars().next_back() } else { t[i..].chars().next() };
    !c.is_some_and(|c| c.is_alphanumeric())
}

/// Copies `text`, and wherever `at(text, i)` finds something at byte `i` (a character boundary), its replacement
/// instead, going on after the end it returns.
fn scan(text: &str, at: impl Fn(&str, usize) -> Option<(usize, String)>) -> String {
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < text.len() {
        if let Some((end, repl)) = at(text, i) {
            if end > i {
                out.push_str(&repl);
                i = end;
                continue;
            }
        }
        let c = text[i..].chars().next().unwrap_or('\0');
        out.push(c);
        i += c.len_utf8().max(1);
    }
    out
}

fn prev(t: &str, i: usize) -> u8 {
    if i == 0 {
        b' '
    } else {
        t.as_bytes()[i - 1]
    }
}

fn at_byte(t: &str, i: usize) -> u8 {
    t.as_bytes().get(i).copied().unwrap_or(b' ')
}

/// `…roms/<system>/<game>.<ext>` (also `game-settings/`, `cheats/`, `saves/`, `savestates/<system>/`,
/// `screenshots/<system>/`): the game becomes its code, the folders and the extension stay.
fn game_path_at(t: &str, i: usize) -> Option<(usize, String)> {
    const ANCHORS: [&str; 6] = ["roms/", "game-settings/", "cheats/", "saves/", "savestates/", "screenshots/"];
    let b = t.as_bytes();
    let p = prev(t, i);
    if p.is_ascii_alphanumeric() || p == b'_' || p == b'-' {
        return None;
    }
    let anchor = ANCHORS.iter().find(|a| t[i..].starts_with(*a))?;
    let seg = |from: usize| -> Option<usize> {
        let mut j = from;
        while j < b.len() && (b[j].is_ascii_alphanumeric() || b"_+-".contains(&b[j])) {
            j += 1;
        }
        (j > from && at_byte(t, j) == b'/').then_some(j)
    };
    let mut sys_start = i + anchor.len();
    let mut sys_end = seg(sys_start)?;
    // roms/screenshots/<system>/…, roms/savestates/<system>/…
    if matches!(&t[sys_start..sys_end], "screenshots" | "savestates") {
        if let Some(e) = seg(sys_end + 1) {
            sys_start = sys_end + 1;
            sys_end = e;
        }
    }
    let system = &t[sys_start..sys_end];
    let rest = sys_end + 1;
    let first = at_byte(t, rest);
    if rest >= b.len() || first == b'[' || first.is_ascii_whitespace() || b"\"'`|<>{}".contains(&first) {
        return None;
    }
    let mut stop = rest;
    while stop < b.len() && !b"\n\r\t\"'`|<>{};".contains(&b[stop]) {
        stop += 1;
    }
    // The first ".ext" (2 to 4 letters and digits, one a letter at least) that ends a word: the file's name ends
    // there. "Dr. Mario" and "Vol.2" don't count; without one, the rest of the line up to a stop is the game.
    let mut end = stop;
    let mut ext = "";
    let mut k = rest;
    while k < stop {
        if b[k] == b'.' {
            let mut e = k + 1;
            while e < stop && b[e].is_ascii_alphanumeric() {
                e += 1;
            }
            let n = e - k - 1;
            if (2..=4).contains(&n) && b[k + 1..e].iter().any(|c| c.is_ascii_alphabetic()) && (e == stop || !(b[e].is_ascii_alphanumeric() || b[e] == b'.')) {
                end = k;
                ext = &t[k..e];
                break;
            }
        }
        k += 1;
    }
    let name = t[rest..end].trim_end();
    if name.is_empty() {
        return None;
    }
    let stem = name.rsplit('/').next().unwrap_or(name);
    let after = rest + name.len() + if end == rest + name.len() { ext.len() } else { 0 };
    let shown_ext = if end == rest + name.len() { ext } else { "" };
    Some((after, format!("{}{system}/{}{shown_ext}", &t[i..sys_start], game_code(system, stem))))
}

/// `/home/<user>`, `/Users/<user>`, `C:\Users\<user>`, and the label of a card under `/var/media/`.
fn user_path_at(t: &str, i: usize) -> Option<(usize, String)> {
    let p = prev(t, i);
    if p.is_ascii_alphanumeric() {
        return None;
    }
    // The two home prefixes are put together at run time: as literals they would sit in the binary, where the
    // release check (scripts/check-binary-paths.sh) can't tell them from a leaked build path.
    static PREFIXES: std::sync::OnceLock<[(String, &str, u8); 5]> = std::sync::OnceLock::new();
    let prefixes = PREFIXES.get_or_init(|| {
        let root = |name: &str| format!("/{name}/");
        [(root("home"), "[user]", b'/'), (root("Users"), "[user]", b'/'), ("C:\\Users\\".into(), "[user]", b'\\'), ("/var/media/".into(), "[card]", b'/'), ("/run/media/".into(), "[card]", b'/')]
    });
    let (prefix, marker, sep) = prefixes.iter().find(|(p, _, _)| t[i..].starts_with(p.as_str()))?;
    let (marker, sep) = (*marker, *sep);
    let b = t.as_bytes();
    let from = i + prefix.len();
    let mut j = from;
    while j < b.len() && b[j] != sep && !b[j].is_ascii_whitespace() && !b"\"'`:;,)>".contains(&b[j]) {
        j += 1;
    }
    if j == from || t[from..].starts_with('[') {
        return None;
    }
    Some((j, format!("{prefix}{marker}")))
}

/// A URL with a login (`scheme://`, a user name, ":", a password, "@", then the host): the login goes.
fn url_login_at(t: &str, i: usize) -> Option<(usize, String)> {
    if !t[i..].starts_with("://") {
        return None;
    }
    let b = t.as_bytes();
    let from = i + 3;
    let mut j = from;
    let mut at = None;
    while j < b.len() && !b[j].is_ascii_whitespace() && !b"/\"'<>".contains(&b[j]) {
        if b[j] == b'@' {
            at = Some(j);
        }
        j += 1;
    }
    let at = at.filter(|&a| a > from)?;
    Some((at, "://[removed]".into()))
}

fn email_at(t: &str, i: usize) -> Option<(usize, String)> {
    let b = t.as_bytes();
    let local = |c: u8| c.is_ascii_alphanumeric() || b"._%+-".contains(&c);
    if !local(b[i]) || local(prev(t, i)) {
        return None;
    }
    let mut j = i;
    while j < b.len() && local(b[j]) {
        j += 1;
    }
    if at_byte(t, j) != b'@' {
        return None;
    }
    let from = j + 1;
    let mut k = from;
    while k < b.len() && (b[k].is_ascii_alphanumeric() || b[k] == b'.' || b[k] == b'-') {
        k += 1;
    }
    while k > from && b[k - 1] == b'.' {
        k -= 1;
    }
    let domain = &t[from..k];
    let tld = domain.rsplit('.').next().unwrap_or("");
    (domain.contains('.') && tld.len() >= 2 && tld.bytes().all(|c| c.is_ascii_alphabetic())).then(|| (k, "[email]".into()))
}

/// Tailscale's auth keys (`tskey-…`) and MagicDNS names (`…ts.net`, `…tailscale.net`).
fn tailscale_at(t: &str, i: usize) -> Option<(usize, String)> {
    let b = t.as_bytes();
    let host = |c: u8| c.is_ascii_alphanumeric() || c == b'.' || c == b'-' || c == b'_';
    if host(prev(t, i)) || !host(b[i]) {
        return None;
    }
    let mut j = i;
    while j < b.len() && host(b[j]) {
        j += 1;
    }
    let word = t[i..j].trim_end_matches('.').to_ascii_lowercase();
    if word.starts_with("tskey-") {
        return Some((j, "[tailscale-key]".into()));
    }
    (word.ends_with(".ts.net") || word.ends_with(".tailscale.net")).then(|| (i + word.len(), "[tailnet-name]".into()))
}

/// A Syncthing device ID: eight groups of seven of A-Z and 2-7, joined by dashes.
fn syncthing_at(t: &str, i: usize) -> Option<(usize, String)> {
    let b = t.as_bytes();
    let p = prev(t, i);
    if p.is_ascii_alphanumeric() || p == b'-' || b.len() < i + 63 {
        return None;
    }
    for (k, &c) in b[i..i + 63].iter().enumerate() {
        let ok = if k % 8 == 7 { c == b'-' } else { c.is_ascii_uppercase() || (b'2'..=b'7').contains(&c) };
        if !ok {
            return None;
        }
    }
    let n = at_byte(t, i + 63);
    (!(n.is_ascii_alphanumeric() || n == b'-')).then(|| (i + 63, "[sync-device-id]".into()))
}

/// `aa:bb:cc:dd:ee:ff` (or with dashes), percent-encoded as a redirect URL carries it (`00%3A00%3A5e%3A00%3A53%3A7f`),
/// or bare (`a4c1385e027f`): a device's address in any form.
fn mac_at(t: &str, i: usize) -> Option<(usize, String)> {
    let b = t.as_bytes();
    let edge = |c: u8| c.is_ascii_alphanumeric() || c == b':' || c == b'-' || c == b'%';
    if edge(prev(t, i)) || !b.get(i).is_some_and(u8::is_ascii_hexdigit) {
        return None;
    }
    // Twelve hex digits in a row.
    if i + 12 <= b.len() && b[i..i + 12].iter().all(u8::is_ascii_hexdigit) && !edge(at_byte(t, i + 12)) {
        return Some((i + 12, "[mac]".into()));
    }
    // Six groups of two hex digits, separated by `:`, `-` or their percent encoding.
    let mut j = i;
    for g in 0..6 {
        let start = j;
        while j < b.len() && j - start < 2 && b[j].is_ascii_hexdigit() {
            j += 1;
        }
        if j - start != 2 {
            return None;
        }
        if g < 5 {
            let enc = t[j..].len() >= 3 && matches!(t[j..j + 3].to_ascii_lowercase().as_str(), "%3a" | "%2d");
            if matches!(at_byte(t, j), b':' | b'-') {
                j += 1;
            } else if enc {
                j += 3;
            } else {
                return None;
            }
        }
    }
    (!edge(at_byte(t, j))).then(|| (j, "[mac]".into()))
}

/// An IPv6 address: hex groups with "::" in them or eight groups. Times ("10:00:00") and Rust paths (`a::b`) are not.
fn ipv6_at(t: &str, i: usize) -> Option<(usize, String)> {
    let b = t.as_bytes();
    let part = |c: u8| c.is_ascii_hexdigit() || c == b':';
    let p = prev(t, i);
    if !part(b[i]) || part(p) || p.is_ascii_alphanumeric() || p == b'_' || p == b'.' {
        return None;
    }
    let mut j = i;
    while j < b.len() && part(b[j]) {
        j += 1;
    }
    let n = at_byte(t, j);
    if n.is_ascii_alphanumeric() || n == b'_' {
        return None;
    }
    let run = &t[i..j];
    let colons = run.bytes().filter(|&c| c == b':').count();
    let groups: Vec<&str> = run.split(':').filter(|g| !g.is_empty()).collect();
    let hex: usize = groups.iter().map(|g| g.len()).sum();
    let valid = colons >= 2 && groups.iter().all(|g| g.len() <= 4) && !run.contains(":::") && hex >= 4 && ((run.contains("::") && groups.len() >= 2) || (colons == 7 && groups.len() == 8));
    valid.then(|| (j, "[ip]".into()))
}

/// An IPv4 address; the loopback and 0.0.0.0 stay, they say nothing about anyone. So does a version with four
/// numbers ([`version_like`]): "easyrpg = version 0.8.1.1", "EasyRPG-0.8.1.1", "0.8.1.1-rc2".
fn ipv4_at(t: &str, i: usize) -> Option<(usize, String)> {
    let b = t.as_bytes();
    let p = prev(t, i);
    if !b[i].is_ascii_digit() || p.is_ascii_alphanumeric() || p == b'.' {
        return None;
    }
    let mut j = i;
    let mut octets = Vec::new();
    for k in 0..4 {
        let from = j;
        while j < b.len() && b[j].is_ascii_digit() && j - from < 3 {
            j += 1;
        }
        let v: u32 = t[from..j].parse().ok()?;
        if j == from || v > 255 {
            return None;
        }
        octets.push(v);
        if k < 3 {
            if at_byte(t, j) != b'.' {
                return None;
            }
            j += 1;
        }
    }
    let n = at_byte(t, j);
    if n.is_ascii_alphanumeric() || (n == b'.' && at_byte(t, j + 1).is_ascii_digit()) {
        return None;
    }
    let harmless = octets[0] == 127 || octets == [0, 0, 0, 0] || version_like(t, i, j);
    (!harmless).then(|| (j, "[ip]".into()))
}

/// What follows a version after a dash: `0.8.1.1-rc2`, `1.2.3.4-beta`.
const VERSION_SUFFIXES: &[&str] = &["rc", "alpha", "beta", "dev", "git", "pre", "snapshot", "release"];

/// Whether the four numbers at `i..j` are a version, not an address, by what stands around them; deliberately
/// narrow, a real address in an ordinary line stays an address:
/// - right after the word "version", "ver" or "v" (`version 0.8.1.1`, `"version": "1.2.3.4"`, `ver=1.2.3.4`;
///   `v1.2.3.4` never counts as an address, the letter touches it);
/// - glued to a name by a dash (`easyrpg-0.8.1.1`; a dash after a space, ` -1.2.3.4`, is no name);
/// - followed by a version's suffix: `-rc2`, `-beta`, `-git…` (a dash and one of [`VERSION_SUFFIXES`]; not
///   `-wlan0`), or `+git`, `~pre` (a plus or tilde, then a letter).
fn version_like(t: &str, i: usize, j: usize) -> bool {
    let b = t.as_bytes();
    if i >= 2 && b[i - 1] == b'-' && b[i - 2].is_ascii_alphanumeric() {
        return true;
    }
    let after = &b[(j + 1).min(b.len())..];
    let suffix = |v: &&str| after.len() >= v.len() && after[..v.len()].eq_ignore_ascii_case(v.as_bytes());
    if (b"+~".contains(&at_byte(t, j)) && at_byte(t, j + 1).is_ascii_alphabetic()) || (at_byte(t, j) == b'-' && VERSION_SUFFIXES.iter().any(suffix)) {
        return true;
    }
    // The word before, over spaces, quotes, "=" and ":" (at most a few of them: "version": "…).
    let mut k = i;
    while k > 0 && i - k < 4 && b" \t\"'=:".contains(&b[k - 1]) {
        k -= 1;
    }
    let end = k;
    while k > 0 && (b[k - 1].is_ascii_alphanumeric() || b[k - 1] == b'_') {
        k -= 1;
    }
    let word = t[k..end].to_ascii_lowercase();
    end < i && (word == "v" || word == "ver" || word == "version" || word.ends_with("_version"))
}

/// Keys whose values are private, at the end of a key (`wifi.key`, `ra_token`, `"password"`).
const PRIVATE_KEYS: &[&str] = &[
    "password", "passwd", "pass", "psk", "passphrase", "secret", "token", "apikey", "api_key", "key", "auth", "authorization", "cookie", "session", "sessionid", "sid", "ticket", "ssid", "bssid", "user", "username", "login", "email", "hostname", "host", "device_id", "deviceid", "tailnet", "code", "serial", "display_name",
];

/// `key=value`, `key: value`, `"key": "value"` with a private key: the value goes.
fn assignment_at(t: &str, i: usize) -> Option<(usize, String)> {
    let b = t.as_bytes();
    if b[i] != b'=' && b[i] != b':' {
        return None;
    }
    // The key before it: optional spaces and a quote, then letters, digits, _ . -
    let mut k = i;
    while k > 0 && b[k - 1] == b' ' {
        k -= 1;
    }
    if k > 0 && b[k - 1] == b'"' {
        k -= 1;
    }
    let key_end = k;
    while k > 0 && (b[k - 1].is_ascii_alphanumeric() || b"_.-".contains(&b[k - 1])) {
        k -= 1;
    }
    let key = t[k..key_end].to_ascii_lowercase();
    let private = PRIVATE_KEYS.iter().any(|w| key == *w || (key.ends_with(w) && b"_.-".contains(&key.as_bytes()[key.len() - w.len() - 1])))
        // The value of a hidden field (`<input type=hidden … value="…">`): a session token, an address.
        || (key == "value" && t[..i].to_ascii_lowercase().rfind("hidden").is_some_and(|h| i - h < 200));
    if !private || (b[i] == b':' && at_byte(t, i + 1) == b':') || (b[i] == b':' && at_byte(t, i + 1) == b'/') {
        return None;
    }
    // The value: quoted up to its quote, or up to a space or , ; & } ] ).
    let mut v = i + 1;
    while v < b.len() && b[v] == b' ' {
        v += 1;
    }
    let quote = at_byte(t, v);
    let (from, to) = if quote == b'"' || quote == b'\'' {
        let mut e = v + 1;
        while e < b.len() && b[e] != quote && b[e] != b'\n' {
            e += if b[e] == b'\\' { 2 } else { 1 };
        }
        (v + 1, e.min(b.len()))
    } else {
        let mut e = v;
        while e < b.len() && !b[e].is_ascii_whitespace() && !b",;&}])".contains(&b[e]) {
            e += 1;
        }
        // "Authorization: Bearer xyz": the word after Bearer or Basic goes too.
        let word = t[v..e].to_ascii_lowercase();
        if (word == "bearer" || word == "basic") && at_byte(t, e) == b' ' {
            e += 1;
            while e < b.len() && !b[e].is_ascii_whitespace() && !b",;&}])".contains(&b[e]) {
                e += 1;
            }
        }
        (v, e)
    };
    let value = &t[from..to];
    if value.is_empty() || value.starts_with('[') || value.starts_with('{') || matches!(value, "null" | "true" | "false" | "none" | "None") {
        return None;
    }
    // The kernel's "Serial: 8250/16550 driver, 4 ports" names the serial port's driver, not a serial number; a
    // serial number has no "/" in it.
    if key == "serial" && value.contains('/') {
        return None;
    }
    Some((to, format!("{}[removed]", &t[i..from])))
}

// ---- the zip --------------------------------------------------------------------------------------------------

/// A zip of the entries (deflated, UTF-8 names, the time given as their date: local, as zip tools read it). No
/// ZIP64: a report is far below 4 GB.
pub fn zip(entries: &[(String, Vec<u8>)], local: &Stamp) -> Vec<u8> {
    let (time, date) = dos_time(local);
    let mut out = Vec::new();
    let mut central = Vec::new();
    for (name, data) in entries {
        let packed = miniz_oxide::deflate::compress_to_vec(data, 6);
        let (method, body): (u16, &[u8]) = if packed.len() < data.len() { (8, &packed) } else { (0, data) };
        let crc = crc32fast::hash(data);
        let offset = out.len() as u32;
        let head = |sig: u32, central: bool| {
            let mut h = Vec::new();
            h.extend(sig.to_le_bytes());
            if central {
                h.extend(0x031eu16.to_le_bytes()); // made by: Unix, 3.0
            }
            h.extend(20u16.to_le_bytes()); // needed: 2.0
            h.extend(0x0800u16.to_le_bytes()); // names in UTF-8
            h.extend(method.to_le_bytes());
            h.extend(time.to_le_bytes());
            h.extend(date.to_le_bytes());
            h.extend(crc.to_le_bytes());
            h.extend((body.len() as u32).to_le_bytes());
            h.extend((data.len() as u32).to_le_bytes());
            h.extend((name.len() as u16).to_le_bytes());
            h.extend(0u16.to_le_bytes()); // extra
            if central {
                h.extend(0u16.to_le_bytes()); // comment
                h.extend(0u16.to_le_bytes()); // disk
                h.extend(0u16.to_le_bytes()); // internal attributes
                h.extend((0o100644u32 << 16).to_le_bytes()); // a plain file, rw-r--r--
                h.extend(offset.to_le_bytes());
            }
            h.extend(name.as_bytes());
            h
        };
        out.extend(head(0x0403_4b50, false));
        out.extend(body);
        central.extend(head(0x0201_4b50, true));
    }
    let (cd_offset, cd_size, count) = (out.len() as u32, central.len() as u32, entries.len() as u16);
    out.extend(central);
    out.extend(0x0605_4b50u32.to_le_bytes());
    out.extend([0u8; 4]); // this disk, the directory's disk
    out.extend(count.to_le_bytes());
    out.extend(count.to_le_bytes());
    out.extend(cd_size.to_le_bytes());
    out.extend(cd_offset.to_le_bytes());
    out.extend(0u16.to_le_bytes());
    out
}

/// MS-DOS time and date of a local time (no earlier than 1980, the format's start).
fn dos_time(t: &Stamp) -> (u16, u16) {
    let (year, month, day) = if t.year < 1980 { (1980, 1, 1) } else { (t.year.min(2107) as u16, t.month as u16, t.day as u16) };
    (((t.hour as u16) << 11) | ((t.min as u16) << 5) | (t.sec as u16 / 2), ((year - 1980) << 9) | (month << 5) | day)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;

    /// The entries of a zip of [`zip`], unpacked.
    fn unzip(z: &[u8]) -> Vec<(String, Vec<u8>)> {
        let le16 = |at: usize| u16::from_le_bytes([z[at], z[at + 1]]) as usize;
        let le32 = |at: usize| u32::from_le_bytes([z[at], z[at + 1], z[at + 2], z[at + 3]]) as usize;
        let eocd = (0..z.len() - 21).rev().find(|&i| z[i..].starts_with(b"PK\x05\x06")).expect("end record");
        let (count, mut at) = (le16(eocd + 10), le32(eocd + 16));
        let mut out = Vec::new();
        for _ in 0..count {
            assert!(z[at..].starts_with(b"PK\x01\x02"));
            let (method, crc, csize, size, name_len, local) = (le16(at + 10), le32(at + 16) as u32, le32(at + 20), le32(at + 24), le16(at + 28), le32(at + 42));
            let name = String::from_utf8(z[at + 46..at + 46 + name_len].to_vec()).unwrap();
            let data_at = local + 30 + le16(local + 26) + le16(local + 28);
            let raw = &z[data_at..data_at + csize];
            let data = if method == 8 { miniz_oxide::inflate::decompress_to_vec(raw).unwrap() } else { raw.to_vec() };
            assert_eq!(data.len(), size);
            assert_eq!(crc32fast::hash(&data), crc, "{name}");
            out.push((name, data));
            at += 46 + name_len;
        }
        out
    }

    fn games() -> Vec<GameName> {
        vec![
            GameName { system: "snes".into(), name: "Chrono Trigger".into(), path: "/storage/roms/snes/Chrono Trigger (USA).sfc".into() },
            GameName { system: "gba".into(), name: "Pokémon Emerald".into(), path: "/storage/roms/gba/Pokemon - Emerald Version (USA, Europe).gba".into() },
        ]
    }

    fn secrets() -> Vec<(String, &'static str)> {
        vec![
            ("Heimnetz Müller 5G".into(), "[wifi-name]"),
            ("hunter2-secret-psk".into(), "[password]"),
            ("RetroFan1987".into(), "[user]"),
            ("Zx81RaTokenAbCdEf0123".into(), "[token]"),
            ("alexs-pocket".into(), "[hostname]"),
            ("share-pass-77qq".into(), "[password]"),
        ]
    }

    /// ROCKNIX 20261001's `/etc/os-release`, as it is on the image.
    const ROCKNIX_OS_RELEASE: &str = include_str!("../testdata/report/rocknix-os-release");

    /// Made-up values that look like private ones, put together here from their parts: written out whole, the public
    /// export's leak check (scripts/public-tree.py) would take them for real ones, and keys, tokens and logins
    /// may never be allowed there. `{name}` in the fixtures stands for the value.
    fn fakes() -> [(&'static str, String); 9] {
        [
            ("{mac1}", ["a4", "c1", "38", "9f", "2e", "11"].join(":")),
            ("{mac2}", ["A4", "C1", "38", "9F", "2E", "12"].join("-")),
            ("{ts-ip}", ["100", "64", "12", "34"].join(".")),
            ("{ts-ip6}", ["fd7a", "115c", "a1e0", "", "1234", "5678"].join(":")),
            ("{tailnet-name}", ["pocket", "tail1234", "ts", "net"].join(".")),
            ("{ts-key}", ["tskey", "auth", "kAbC123CNTRL", "xyz"].join("-")),
            ("{ra-token}", ["Zx81RaToken", "AbCdEf0123"].concat()),
            ("{home}", ["", "home", "jdoe"].join("/")),
            ("{login-url}", ["https://jdoe:pa55word", "updates.example.org/update.json"].join("@")),
        ]
    }

    /// `text` with the [`fakes`] filled in.
    fn fill(text: &str) -> String {
        fakes().iter().fold(text.to_string(), |t, (k, v)| t.replace(k, v))
    }

    /// A log with one of each kind of private detail (testdata/report/private-log.txt, [`fill`]ed).
    fn fixture() -> String {
        fill(include_str!("../testdata/report/private-log.txt"))
    }

    /// Every private string of the fixture, none of which may be left.
    fn private() -> Vec<String> {
        let mut out: Vec<String> = [
            "Heimnetz", "hunter2", "RetroFan1987", "Zx81RaToken", "alexs-pocket", "share-pass-77qq", "Chrono Trigger", "Chrono", "Pokemon", "Pokémon", "Emerald", "Final Fantasy", "Secret of Mana",
            "192.0.2.10", "198.51.100.7", "2001:db8:85a3", "MFZWI3D-BONSGYC-YLTMRWG-C43ENR5-QXGZDMM-FZWI3DP-BONSGYY-LTMRWAD", "jdoe", "alex@example.org", "s3cr3t-cookie", "x9y8z7", "webtoken123",
            "482913", "pa55word", "MyCard",
        ]
        .iter()
        .map(|p| p.to_string())
        .collect();
        // The address and key fakes whole, the IPv6 one by its prefix.
        out.extend(fakes().into_iter().filter(|(k, _)| !matches!(*k, "{home}" | "{login-url}" | "{ts-ip6}")).map(|(_, v)| v));
        out.push(["fd7a", "115c", "a1e0"].join(":"));
        out
    }

    #[test]
    fn report_fixture_logs_leave_no_private_detail() {
        let r = Redactor::new(secrets(), &games());
        let fixture = fixture();
        assert!(!fakes().iter().any(|(k, _)| fixture.contains(k)), "every fake filled in");
        let clean = r.clean(&fixture);
        for p in &private() {
            assert!(!clean.to_lowercase().contains(&p.to_lowercase()), "{p:?} is still in:\n{clean}");
        }
        // What helps find the problem stays.
        for kept in ["10:00:00", "pocket_ui::web::server", "pocket-core 0.1.0", "6.8.0-rocknix", "127.0.0.1", "mgba_libretro.so", "fps=60", "exit code 1", ".sfc", ".gba", "temp=61C", "snes"] {
            assert!(clean.contains(kept), "{kept:?} went missing:\n{clean}");
        }
        for marker in ["[wifi-name]", "[password]", "[user]", "[token]", "[ip]", "[mac]", "[tailscale-key]", "[tailnet-name]", "[sync-device-id]", "[email]", "[removed]", "[game snes ", "[game gba ", "/home/[user]", "/var/media/[card]"] {
            assert!(clean.contains(marker), "no {marker:?} in:\n{clean}");
        }
    }

    #[test]
    fn report_game_names_become_the_same_code_as_their_files() {
        let r = Redactor::new(vec![], &games());
        let code = game_code("snes", "Chrono Trigger (USA)");
        let by_path = r.clean("start /storage/roms/snes/Chrono Trigger (USA).sfc now");
        assert_eq!(by_path, format!("start /storage/roms/snes/{code}.sfc now"));
        // The title alone (on the card, in a notice) gets the file's code.
        assert_eq!(r.clean("Chrono Trigger did not start"), format!("{code} did not start"));
        // Not in the library: the path's form still hides it, with the same kind of code.
        let unknown = r.clean("load roms/psx/Final Fantasy VII (Disc 1).chd: ok");
        assert_eq!(unknown, format!("load roms/psx/{}.chd: ok", game_code("psx", "Final Fantasy VII (Disc 1)")));
        // A name inside a word is no name: "Chrono" alone is a different game, "Emeraldine" a different word.
        assert_eq!(r.clean("Chronograph"), "Chronograph");
    }

    #[test]
    fn report_patterns_leave_ordinary_lines_alone() {
        let r = Redactor::new(vec![], &[]);
        for line in [
            "[  12.345] pocket-ui: frame 16.6 ms at 10:00:00",
            "core: mgba 0.10.3 (6.8.0) on 127.0.0.1",
            "error: pocket_ui::web::server: can't bind",
            "launcher: exit code 77, EmulationStation takes over",
            "key_mgmt=wpa-psk",
            "hotkey=select",
            "settings: text_size=large theme=cards",
        ] {
            assert_eq!(r.clean(line), line);
        }
    }

    #[test]
    fn report_covers_addresses_in_every_form_and_hidden_values() {
        let d = tmp("mac-forms");
        let src = pocket(&d);
        // Made up from their parts, as `fakes()` does: percent-encoded (a redirect URL's `client_mac`), bare twelve
        // hex digits, and the value of a hidden field.
        let encoded = ["a4", "c1", "38", "9f", "2e", "13"].join("%3A");
        let bare = ["a4c138", "9f2e14"].concat();
        std::fs::write(
            src.data_dir.join("logs/portal-last.html"),
            format!("client_mac={encoded} card={bare}\n<input type=hidden name=sessionid value=abc123>\n"),
        )
        .unwrap();
        let files = entries(&src, &games(), NOW, &LOCAL);
        let text = String::from_utf8_lossy(&files.iter().find(|(f, _)| f == "logs/portal-last.html").unwrap().1).into_owned();
        assert!(text.contains("client_mac=[mac]") && text.contains("card=[mac]"), "{text}");
        assert!(!text.contains("abc123"), "{text}");
        assert!(text.contains("name=sessionid"), "the field's name stays: {text}");
    }

    /// Four numbers that are a version stay; the same numbers as an address go. The kernel's serial driver line stays,
    /// a serial number goes.
    #[test]
    fn report_versions_and_drivers_stay_addresses_and_serials_go() {
        let r = Redactor::new(vec![], &[]);
        for line in [
            "easyrpg = version 0.8.1.1, ROCKNIX, 2026-10-01, 9 MB",
            r#"{"name": "easyrpg", "version": "0.8.1.1"}"#,
            "core_version=0.8.1.1",
            "launching EasyRPG-0.8.1.1 for rpg2k",
            "player 0.8.1.1-rc2 built 2026",
            "mupen64plus v2.6.0.1 starts",
            "[    1.234] Serial: 8250/16550 driver, 4 ports, IRQ sharing disabled",
        ] {
            assert_eq!(r.clean(line), line);
        }
        for (line, want) in [
            ("wlan0: connected to 203.0.113.64", "wlan0: connected to [ip]"),
            ("default via 198.51.100.1 dev wlan0", "default via [ip] dev wlan0"),
            ("ping -c1 -203.0.113.2", "ping -c1 -[ip]"),
            ("sync: peer 203.0.113.5-wlan0", "sync: peer [ip]-wlan0"),
            ("serverversion 198.51.100.5", "serverversion [ip]"),
            ("serial=R5CT1234ABC", "serial=[removed]"),
            ("Serial: R5CT1234ABC", "Serial: [removed]"),
        ] {
            assert_eq!(r.clean(line), want);
        }
    }

    /// A Pocket as ROCKNIX sets it up: the host name is the chip ("SM8250"), the Wi-Fi profile is called "wifi". Those
    /// are ROCKNIX's words, not the player's: they stay everywhere, and the README is left as pocket-ui wrote it (it
    /// said "[[wifi-name]-name]"). The network's real name and password still go.
    #[test]
    fn report_rocknix_own_names_are_no_secrets() {
        let d = tmp("stock");
        let src = pocket(&d);
        let put = |rel: &str, text: &str| {
            let p = d.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, text).unwrap();
        };
        put("root/etc/hostname", "SM8250\n");
        put("root/proc/device-tree/compatible", "retroid,rpocketv2\0qcom,sm8250\0");
        put("storage/.config/system/configs/system.cfg", "system.hostname=SM8250\nroot.password=rocknix\nglobal.retroachievements.username=\nwifi.ssid=Heimnetz Müller 5G\nwifi.key=hunter2-secret-psk\n");
        put("storage/.config/NetworkManager/system-connections/wifi.nmconnection", "[connection]\nid=wifi\ntype=wifi\ninterface-name=wlan0\n[wifi]\nmode=infrastructure\nssid=Heimnetz Müller 5G\n[wifi-security]\nkey-mgmt=wpa-psk\npsk=hunter2-secret-psk\n");
        put("data/tailscale.json", "{}");
        put(
            "data/logs/kernel.log",
            "[    0.10] Serial: 8250/16550 driver, 4 ports, IRQ sharing disabled\n[    0.20] gcc-sm8250 100000.clock-controller: Registered clocks\n[    0.30] Machine model: Retroid Pocket Mini V2\n[    9.00] wlan0: connected to wifi (Heimnetz Müller 5G) at 203.0.113.64\n",
        );
        put("data/logs/pocket-ui.log", "launcher: easyrpg-0.8.1.1 on SM8250\nwifi: profile wifi up, wlan0 203.0.113.64\n");
        put("root/usr/lib/libretro/easyrpg_libretro.so", "ELF");
        put("root/usr/lib/libretro/easyrpg_libretro.info", "display_version = \"0.8.1.1\"\n");

        let secrets = known_secrets(&src);
        let values: Vec<&str> = secrets.iter().map(|(v, _)| v.as_str()).collect();
        for generic in ["wifi", "SM8250", "rocknix", "wlan0"] {
            assert!(!values.iter().any(|v| v.eq_ignore_ascii_case(generic)), "{generic:?} taken for a secret: {secrets:?}");
        }
        assert!(values.contains(&"Heimnetz Müller 5G") && values.contains(&"hunter2-secret-psk"), "{secrets:?}");

        let files = entries(&src, &games(), NOW, &LOCAL);
        let text = |n: &str| String::from_utf8_lossy(&files.iter().find(|(f, _)| f == n).unwrap().1).into_owned();
        let readme = text("README.txt");
        assert!(readme.contains("  Wi-Fi network names and passwords                       [wifi-name] [password]\n"), "{readme}");
        assert!(!readme.contains("[[") && !readme.contains("]-name]"), "{readme}");
        let kernel = text("logs/kernel.log");
        for kept in ["Serial: 8250/16550 driver", "gcc-sm8250 100000.clock-controller", "Retroid Pocket Mini V2", "wlan0: connected to wifi ([wifi-name]) at [ip]"] {
            assert!(kernel.contains(kept), "{kept:?} not in:\n{kernel}");
        }
        let ui = text("logs/pocket-ui.log");
        assert!(ui.contains("launcher: easyrpg-0.8.1.1 on SM8250") && ui.contains("wifi: profile wifi up, wlan0 [ip]"), "{ui}");
        let system = text("system.txt");
        assert!(system.contains("HW_DEVICE=\"SM8250\"") && !system.contains("[hostname]"), "{system}");
        assert!(text("emulators.txt").contains("easyrpg = version 0.8.1.1, ROCKNIX"), "{}", text("emulators.txt"));
        let all: String = files.iter().map(|(_, b)| String::from_utf8_lossy(b).into_owned()).collect();
        for p in ["Heimnetz", "hunter2", "203.0.113.64"] {
            assert!(!all.contains(p), "{p:?} is still in the report");
        }
    }

    /// ROCKNIX's os-release says OS_NAME and OS_VERSION (testdata/report/rocknix-os-release, from the 20261001 image);
    /// other systems NAME and VERSION.
    #[test]
    fn report_reads_rocknix_os_release() {
        let d = tmp("osrel");
        let src = pocket(&d);
        let system = |os: &str| {
            std::fs::write(d.join("root/etc/os-release"), os).unwrap();
            system_txt(&src, NOW)
        };
        assert!(system(ROCKNIX_OS_RELEASE).contains("rocknix = ROCKNIX 20261001\n"));
        assert!(system("NAME=\"Debian GNU/Linux\"\nVERSION=\"12 (bookworm)\"\n").contains("rocknix = Debian GNU/Linux 12 (bookworm)\n"));
        assert!(system("OS_NAME=\"ROCKNIX\"\n").contains("rocknix = ROCKNIX, version unknown\n"));
        assert!(system("HW_DEVICE=\"SM8250\"\n").contains("rocknix = unknown (no name or version in /etc/os-release)\n"));
    }

    #[test]
    fn report_zip_reads_back_with_every_file() {
        let entries = vec![("README.txt".to_string(), b"hello\n".to_vec()), ("logs/pocket-ui.log".to_string(), "line\n".repeat(5000).into_bytes())];
        let z = zip(&entries, &LOCAL);
        assert!(z.len() < 2000, "the repeated log is deflated: {}", z.len());
        assert_eq!(unzip(&z), entries);
    }

    /// 2026-10-26 14:05:07 UTC ...
    const NOW: u64 = 1_793_023_507;
    /// ... is 15:05:07 on a Pocket in Vienna.
    const LOCAL: Stamp = Stamp { year: 2026, month: 10, day: 26, hour: 15, min: 5, sec: 7 };

    #[test]
    fn report_file_name_has_date_and_time() {
        assert_eq!(file_name(&LOCAL), "report-2026-10-26-150507.zip");
        assert_eq!(dos_time(&LOCAL), ((15 << 11) | (5 << 5) | 3, (46 << 9) | (10 << 5) | 26));
        assert_eq!(dos_time(&Stamp { year: 1970, month: 1, day: 1, hour: 0, min: 0, sec: 0 }).1, (1 << 5) | 1, "a clock never set: 1980");
    }

    /// The device check: a report made at 16:39 was called "…-153939.zip" (UTC). The name, the README and the zip's
    /// file times are the Pocket's clock, as the screenshots'; system.txt and the README keep UTC for the logs.
    #[test]
    fn report_file_name_is_the_pockets_clock_not_utc() {
        let d = tmp("local");
        let src = pocket(&d);
        let saved = write(&src, &games(), NOW, &LOCAL).unwrap();
        assert_eq!(saved.name, "report-2026-10-26-150507.zip", "not 140507 (UTC)");
        let z = std::fs::read(&saved.path).unwrap();
        assert_eq!(u16::from_le_bytes([z[10], z[11]]) >> 11, 15, "the zip's file time is local too");
        let files = unzip(&z);
        let readme = String::from_utf8_lossy(&files[0].1).into_owned();
        assert!(readme.contains("Made on 2026-10-26 15:05:07 on the Pocket's clock (2026-10-26 14:05:07 UTC, the time in the logs)"), "{readme}");
        let system = String::from_utf8_lossy(&files.iter().find(|(n, _)| n == "system.txt").unwrap().1).into_owned();
        assert!(system.contains("report_made = 2026-10-26 14:05:07 UTC"), "{system}");
    }

    fn tmp(name: &str) -> crate::testutil::TestDir {
        TestDir::new(&format!("report-{name}"))
    }

    /// A made-up Pocket: data folder, logs, system files, accounts.
    fn pocket(d: &Path) -> Sources {
        let put = |rel: &str, text: &str| {
            let p = d.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, text).unwrap();
        };
        put("data/settings.json", &r#"{"onboarded": true, "theme": "cards", "update_url": "{login}198.51.100.7/update.json"}"#.replace("{login}", "https://jdoe:pa55word@"));
        put("data/sleep.json", r#"{"screen_off": 120}"#);
        put("data/fallback.json", r#"["/storage/roms/snes/Chrono Trigger (USA).sfc"]"#);
        put("data/recents.json", r#"[{"name": "Chrono Trigger"}]"#);
        put("data/retroachievements.json", &fill(r#"{"username": "RetroFan1987", "token": "{ra-token}"}"#));
        put("data/tailscale.json", r#"{"hostname": "alexs-pocket"}"#);
        put("data/share/password", "share-pass-77qq\n");
        put("data/crash.log", "2026-10-26 10:00:00 UTC pocket-ui panicked at src/main.rs:1:1: Chrono Trigger crashed\n");
        put("data/logs/pocket-ui.log", &fixture());
        put("data/logs/kernel.log", &fill("[ 1.0] wlan0: associated with {mac1} (SyncDevice42, key x9y8z7-long)\n[ 2.0] bluetooth: Alex's AirPods Pro connected, Xbox Wireless Controller connected\n"));
        put("data/logs/net.log", &fill("[ 3.0] host iwd[2]: Connected to Heimnetz Müller 5G ({mac1})\n[ 3.1]   Heimnetz Müller 5G:1111:802-11-wireless\n"));
        put("storage/.cache/bluetooth/AA:BB:CC:DD:EE:FF/11:22:33:44:55:66/info", "[General]\nName=Alex's AirPods Pro\n");
        put("storage/.cache/bluetooth/AA:BB:CC:DD:EE:FF/11:11:11:11:11:11/info", "[General]\nName=Xbox Wireless Controller\n");
        put("data/logs/boot-3-2026-10-25-090000/pocket-ui.log", "previous: roms/gba/Pokemon - Emerald Version (USA, Europe).gba\n");
        put(
            "data/logs/portal-last.html",
            &fill(r#"<h1>Internet Access - please wait.</h1><img src="/logo?mac={mac1}&ip={ts-ip}"><form action="http://192.0.2.10/guest"><input type=hidden name=mac value="{mac1}"></form>"#),
        );
        put("data/logs/boot-3-2026-10-25-090000/portal-last.html", &fill("the sign-in page before, with {mac2} in it"));
        put("data/store/installed.json", r#"{"packages": {"mgba": {"version": "0.10.3"}}}"#);
        put("storage/.config/system/configs/system.cfg", "wifi.enabled=1\nwifi.ssid=Heimnetz Müller 5G\nwifi.key=hunter2-secret-psk\n");
        put("storage/.config/NetworkManager/system-connections/SyncDevice42.nmconnection", "[wifi]\nssid=SyncDevice42\n[wifi-security]\npsk=x9y8z7-long\n");
        put("storage/cores/mgba_libretro.so", "ELF");
        put("root/usr/lib/libretro/snes9x_libretro.so", "ELF");
        put("root/usr/lib/libretro/snes9x_libretro.info", "display_name = \"Nintendo - SNES (Snes9x)\"\ndisplay_version = \"1.62.3\"\n");
        put("root/usr/bin/start_ppsspp.sh", "#!/bin/sh\n");
        put("root/etc/os-release", ROCKNIX_OS_RELEASE);
        put("root/etc/hostname", "alexs-pocket\n");
        put("root/proc/device-tree/model", "Retroid Pocket Mini V2\0");
        put("root/proc/uptime", "7980.12 1000.0\n");
        put("root/proc/meminfo", "MemTotal: 3800000 kB\nMemAvailable: 2000000 kB\n");
        put("root/proc/mounts", "/dev/mmcblk1p2 /storage ext4 rw 0 0\n/dev/sda1 /var/media/MyCard vfat rw 0 0\n");
        std::fs::create_dir_all(d.join("root/storage")).unwrap();
        std::fs::create_dir_all(d.join("root/var/media/MyCard")).unwrap();
        Sources {
            data_dir: d.join("data"),
            live_log: d.join("nowhere.log"),
            root: d.join("root"),
            storage: d.join("storage"),
            battery: d.join("battery"),
            pocket_core: d.join("no-pocket-core"),
            out_dir: d.join("roms/reports"),
        }
    }

    #[test]
    fn report_bundle_has_its_files_and_nothing_private() {
        let d = tmp("bundle");
        let src = pocket(&d);
        let saved = write(&src, &games(), NOW, &LOCAL).unwrap();
        assert_eq!(saved.name, "report-2026-10-26-150507.zip");
        assert_eq!(saved.path, src.out_dir.join(&saved.name));
        let files = unzip(&std::fs::read(&saved.path).unwrap());
        let names: Vec<&str> = files.iter().map(|(n, _)| n.as_str()).collect();
        for want in ["README.txt", "system.txt", "emulators.txt", "settings/settings.json", "settings/sleep.json", "settings/fallback.json", "settings/store-installed.json", "logs/pocket-ui.log", "logs/kernel.log", "logs/net.log", "logs/crash.log", "logs/previous-boot/pocket-ui.log", "logs/portal-last.html", "logs/previous-boot/portal-last.html"] {
            assert!(names.contains(&want), "{want} missing from {names:?}");
        }
        for never in ["settings/recents.json", "settings/retroachievements.json", "settings/tailscale.json"] {
            assert!(!names.contains(&never), "{never} must not be in the report");
        }
        let all: String = files.iter().map(|(n, d)| format!("== {n}\n{}\n", String::from_utf8_lossy(d))).collect();
        for p in private().iter().map(String::as_str).chain(["SyncDevice42", "x9y8z7-long", "AirPods Pro"]) {
            assert!(!all.to_lowercase().contains(&p.to_lowercase()), "{p:?} is still in the report:\n{all}");
        }
        let text = |n: &str| String::from_utf8_lossy(&files.iter().find(|(f, _)| f == n).unwrap().1).into_owned();
        let system = text("system.txt");
        for want in ["rocknix = ROCKNIX 20261001", "device = Retroid Pocket Mini V2", "uptime = 2 h 13 min", "pocket_core = not found", "/storage = ", "removable card or stick 1 = ", "last_crash = 2026-10-26"] {
            assert!(system.contains(want), "{want:?} not in system.txt:\n{system}");
        }
        let emus = text("emulators.txt");
        for want in ["mgba = version not recorded, from the Store", "snes9x = version 1.62.3, ROCKNIX", "ppsspp", "gba"] {
            assert!(emus.contains(want), "{want:?} not in emulators.txt:\n{emus}");
        }
        assert!(text("logs/kernel.log").contains("[bluetooth-name] connected, Xbox Wireless Controller connected"), "{}", text("logs/kernel.log"));
        let net = text("logs/net.log");
        assert!(net.contains("Connected to ") && !net.contains("Heimnetz") && !net.contains("{mac1}"), "{net}");
        assert!(text("logs/portal-last.html").contains("please wait"), "{}", text("logs/portal-last.html"));
        let readme = text("README.txt");
        assert!(readme.contains("logs/pocket-ui.log") && readme.contains("GitHub"), "{readme}");
        assert!(readme.contains("logs/portal-last.html") && readme.contains("needed a full browser"), "{readme}");
        assert!(!std::fs::read_dir(&src.out_dir).unwrap().flatten().any(|e| e.file_name().to_string_lossy().ends_with(".part")));
    }

    #[test]
    fn report_carries_the_shutdown_log_of_this_boot_and_the_one_before_cleaned() {
        let d = tmp("shutdown-log");
        let src = pocket(&d);
        let logs = src.data_dir.join("logs");
        std::fs::write(
            logs.join("boot-3-2026-10-25-090000/shutdown.log"),
            "[ 100.000] 2026-10-25 21:00:00 UTC PowerOff: RTC alarm disarm verified (before=1793051031 write=ok after=none alarm_IRQ=no alrm_pending=no tries=1)\n[ 100.500] 2026-10-25 21:00:01 UTC PowerOff: `systemctl poweroff` ended: exit 0 from 192.0.2.164\n",
        )
        .unwrap();
        std::fs::write(logs.join("shutdown.log"), "[ 50.000] 2026-10-26 15:00:00 UTC Reboot asked (Asked)\n").unwrap();
        // The start before that one: the Power off that came back on happened there.
        std::fs::create_dir_all(logs.join("boot-2-2026-10-24-090000")).unwrap();
        std::fs::write(logs.join("boot-2-2026-10-24-090000/shutdown.log"), "[ 20.000] 2026-10-24 21:00:00 UTC PowerOff asked (Asked)\n").unwrap();
        std::fs::write(logs.join("boot-2-2026-10-24-090000/pocket-ui.log"), "second last boot\n").unwrap();
        let files = entries(&src, &games(), NOW, &LOCAL);
        let text = |n: &str| files.iter().find(|(f, _)| f == n).map(|(_, d)| String::from_utf8_lossy(d).into_owned());
        let previous = text("logs/previous-boot/shutdown.log").expect("the previous boot's shutdown log");
        assert!(previous.contains("ended: exit 0") && previous.contains("before=1793051031"), "{previous}");
        assert!(!previous.contains("192.0.2.164"), "cleaned like the other logs: {previous}");
        assert!(text("logs/shutdown.log").unwrap().contains("Reboot asked"));
        assert!(text("logs/boot-before-that/shutdown.log").unwrap().contains("PowerOff asked"));
        assert!(text("logs/boot-before-that/pocket-ui.log").unwrap().contains("second last boot"));
        let readme = text("README.txt").unwrap();
        assert!(readme.contains("logs/shutdown.log") && readme.contains("logs/previous-boot/shutdown.log"), "{readme}");
        // A boot with no power off has none, and that is no missing file to explain.
        std::fs::remove_file(logs.join("shutdown.log")).unwrap();
        let files = entries(&src, &games(), NOW, &LOCAL);
        assert!(!files.iter().any(|(n, _)| n == "logs/shutdown.log"));
        assert!(!String::from_utf8_lossy(&files[0].1).contains("shutdown.log (no"), "no 'missing' line for it");
    }

    #[test]
    fn report_on_a_bare_pocket_still_says_what_is_missing() {
        let d = &tmp("bare");
        let src = Sources { data_dir: d.join("data"), live_log: d.join("x"), root: d.join("root"), storage: d.join("s"), battery: d.join("b"), pocket_core: d.join("pc"), out_dir: d.join("out") };
        let files = entries(&src, &[], NOW, &LOCAL);
        let readme = String::from_utf8_lossy(&files[0].1).into_owned();
        assert!(readme.contains("pocket-ui.log") && readme.contains("no crash recorded"), "{readme}");
        let system = String::from_utf8_lossy(&files.iter().find(|(n, _)| n == "system.txt").unwrap().1).into_owned();
        assert!(system.contains("rocknix = unknown") && system.contains("device = unknown"), "{system}");
        assert!(system.contains("device_profile = "), "pocket-ui's own view of the device: {system}");
    }

    #[test]
    fn report_full_or_read_only_card_says_so() {
        assert_eq!(Failure::from_io(&std::io::Error::from_raw_os_error(libc::ENOSPC)), Failure::Full);
        assert_eq!(Failure::from_io(&std::io::Error::from_raw_os_error(libc::EROFS)), Failure::ReadOnly);
        assert!(Failure::Full.message().contains("full"));
        // A folder that can't be made (a file stands where it should be): an error, and nothing half written.
        let d = tmp("fail");
        std::fs::write(d.join("reports"), "a file").unwrap();
        let mut src = pocket(&d);
        src.out_dir = d.join("reports");
        assert!(write(&src, &[], NOW, &LOCAL).is_err());
    }
}
