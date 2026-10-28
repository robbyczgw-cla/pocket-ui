//! Updates from our own signed channel: one `update.json` (+ `update.json.sig`, signed with the
//! store key) lists the newest app and OS release.
//!
//! App (pocket-ui + pocket-core), installed live:
//!   <root>/releases/<version>/   unpacked release, checked with `--version` before use
//!   <root>/current -> releases/<version>   switched with one rename, so it is never half-updated
//!   <root>/previous -> the version before; <root>/pocket-ui and pocket-core link into current/
//!   <root>/update-pending        written on install, removed once the new version has presented its first frame.
//!   <root>/known-good -> the last version that ran `POCKETUI_STABLE_SECS` with a picture (set by run-pocket-ui.sh);
//!   the launcher goes back to it when another version keeps crashing, so `prune` keeps it.
//! pocket-ui then exits with `RESTART_CODE`; run-pocket-ui.sh starts it again and switches back
//! to `previous` if the new version dies while update-pending is still there. A new version that
//! hangs instead of dying is killed by `watch_health` (exit `WATCHDOG_EXIT`), which has the same effect.
//!
//! OS (KERNEL + SYSTEM as a ROCKNIX-style .tar), installed by the initramfs on the next start:
//! the tar is placed in /storage/.update only after its SYSTEM has been opened and checked. The
//! initramfs runs the new SYSTEM's usr/share/bootloader/update.sh, and ROCKNIX's calls `updateabl`,
//! which rewrites the ABL in the Mini V2's internal storage, the bootloader Android depends on.
//! So any update that ships `updateabl` or touches bootloaders or partitions is refused. The new system must also
//! still have what pocket-ui and pocket-core need to run (`os_compat`): an update that would break them is refused,
//! one that only loses something (Wi-Fi tools, a core) is staged with a note. `pocket-ui --check-os-update <tar|SYSTEM|
//! dir>` runs the same checks without staging anything (`check_os_update_cli`).

use std::ffi::OsString;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::os_compat::{self, Needs, System};
use crate::store;

pub const APP_ROOT: &str = "/storage/pocket-ui";
/// Where the initramfs looks for updates.
pub const OS_UPDATE_DIR: &str = "/storage/.update";
/// Downloads and checks happen here, so a half-checked file is never where the initramfs looks.
const OS_STAGING: &str = "/storage/.pocket-update";
/// pocket-ui's exit code asking run-pocket-ui.sh to start it again.
pub const RESTART_CODE: i32 = 75;
/// A freshly installed version that has not presented a frame this long after it started is
/// hanging; it exits with `WATCHDOG_EXIT` so that run-pocket-ui.sh switches back to `previous`.
pub const WATCHDOG_AFTER: Duration = Duration::from_secs(20);
/// Neither 0 nor `RESTART_CODE`: those are the two codes that run-pocket-ui.sh does not roll back on.
pub const WATCHDOG_EXIT: i32 = 124;
/// A running update keeps auto-sleep off for at most this long (curl's timeouts end a stalled
/// download long before; this is the cap for everything else).
pub const BUSY_BLOCKS_SLEEP_FOR: Duration = Duration::from_secs(30 * 60);
/// The public channel (docs/UPDATE-CHANNEL.md): the signed list on pocketui.xyz, the files it names on GitHub
/// Releases. settings.json `update_url` overrides it (`file://` for tests and the device test).
pub const DEFAULT_URL: &str = "https://pocketui.xyz/updates/stable/update.json";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Release {
    pub version: String,
    /// Absolute, or relative to update.json.
    pub url: String,
    pub sha256: String,
    pub size: u64,
    #[serde(default)]
    pub notes: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Manifest {
    pub format: u32,
    #[serde(default)]
    pub app: Option<Release>,
    #[serde(default)]
    pub os: Option<Release>,
    /// The SD card image and the install package for a ROCKNIX card, for people (the website's download page, a
    /// checksum to compare); a device ignores both. docs/UPDATE-CHANNEL.md.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sd_image: Option<Release>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub install_zip: Option<Release>,
}

/// True when `candidate` is a later version than `current`: number groups compared in order
/// ("0.10.0" > "0.9.3", "20261101" > "20261001"); anything else in the string is ignored.
pub fn newer(candidate: &str, current: &str) -> bool {
    let nums = |s: &str| -> Vec<u64> { s.split(|c: char| !c.is_ascii_digit()).filter(|p| !p.is_empty()).filter_map(|p| p.parse().ok()).collect() };
    nums(candidate) > nums(current)
}

/// Cargo.toml's version, or POCKETUI_VERSION at build time (scripts/release-app.sh sets it).
pub fn app_version() -> &'static str {
    option_env!("POCKETUI_VERSION").unwrap_or(env!("CARGO_PKG_VERSION"))
}

/// `url` as given when absolute, otherwise next to `manifest_url`.
pub fn resolve(manifest_url: &str, url: &str) -> String {
    if url.contains("://") {
        url.to_string()
    } else {
        let base = manifest_url.rsplit_once('/').map_or(manifest_url, |(b, _)| b);
        format!("{base}/{url}")
    }
}

/// The programs this module runs; tests substitute scripts for them.
struct Tools<'a> {
    curl: &'a str,
    unsquashfs: &'a str,
    /// curl's `--proto`: the schemes a list or a download may use (and the only ones `scheme_allowed` lets through).
    protocols: &'a str,
    /// curl's `--proto-redir`: where a redirect may lead. GitHub Releases answers with a redirect to its download
    /// host, so redirects are followed (`-L`), but only to https.
    redirects: &'a str,
}

/// https only, and `file://` for a channel on the card (tests, the device test). Never plain http, not even after a
/// redirect: the list is signed and every file hashed, but a network that can rewrite http could still stall or
/// feed an old list.
const TOOLS: Tools<'static> = Tools { curl: "curl", unsquashfs: "unsquashfs", protocols: "=https,file", redirects: "=https" };

/// Whether `url`'s scheme is one of `protocols` (curl's `--proto` syntax, `=https,file`).
fn scheme_allowed(protocols: &str, url: &str) -> bool {
    let Some((scheme, _)) = url.split_once("://") else { return false };
    protocols.trim_start_matches('=').split(',').any(|p| p.eq_ignore_ascii_case(scheme))
}

/// The arguments every curl call of this module starts with.
fn curl_base(tools: &Tools) -> Vec<OsString> {
    ["-fsSL", "--proto", tools.protocols, "--proto-redir", tools.redirects].iter().map(OsString::from).collect()
}

/// curl's exit code 1, "unsupported protocol": a redirect to plain http (or another scheme `--proto-redir` forbids).
const CURL_PROTOCOL: i32 = 1;
/// curl's exit code 22 with `-f`: the server answered 404 (or another error status).
const CURL_HTTP_ERROR: i32 = 22;

const NOT_HTTPS: &str = "The update server sent an address that isn't https. Nothing was changed.";

/// Lets the Updates page stop a running download.
#[derive(Clone, Default)]
pub struct Cancel(Arc<AtomicBool>);

impl Cancel {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

fn fetch(tools: &Tools, url: &str) -> Result<Vec<u8>, String> {
    if !scheme_allowed(tools.protocols, url) {
        return Err("The update address has to start with https://. Nothing was changed.".into());
    }
    let out = Command::new(tools.curl).args(curl_base(tools)).args(["--connect-timeout", "15", "--max-time", "60", url]).output().map_err(|e| e.to_string())?;
    match out.status.code() {
        _ if out.status.success() => Ok(out.stdout),
        // The server is there but has no list (not published yet, or a wrong `update_url`): not a Wi-Fi problem.
        Some(CURL_HTTP_ERROR) => Err("The update server has no update list right now. Try again later.".into()),
        Some(CURL_PROTOCOL) => Err(NOT_HTTPS.into()),
        _ => Err(crate::net_check::refine("Couldn't reach the update server. Check Wi-Fi.")),
    }
}

/// Downloads and verifies update.json; an unsigned or tampered file is an error.
pub fn fetch_manifest(url: &str) -> Result<Manifest, String> {
    fetch_manifest_with(&TOOLS, url, &store::PUBLIC_KEY)
}

fn fetch_manifest_with(tools: &Tools, url: &str, key: &[u8; 32]) -> Result<Manifest, String> {
    let json = fetch(tools, url)?;
    let sig = fetch(tools, &format!("{url}.sig"))?;
    parse_manifest(&json, &String::from_utf8_lossy(&sig), key)
}

pub fn parse_manifest(json: &[u8], sig_b64: &str, key: &[u8; 32]) -> Result<Manifest, String> {
    store::verify_signed(json, sig_b64, key).map_err(|_| "The update list isn't signed by us. Nothing was changed.".to_string())?;
    serde_json::from_slice(json).map_err(|e| format!("update list: {e}"))
}

/// curl's arguments for a download to `dest`. A connection that doesn't come up in 20 s, or that
/// stays under 10 KB/s for 30 s, is dropped (and retried); with `max_size` (from the signed list,
/// 0 = unknown) curl refuses a file that is larger than announced.
fn download_args(tools: &Tools, url: &str, dest: &Path, max_size: u64) -> Vec<OsString> {
    let mut args = curl_base(tools);
    args.extend(["--retry", "3", "-C", "-", "--connect-timeout", "20", "--speed-limit", "10240", "--speed-time", "30"].iter().map(OsString::from));
    if max_size > 0 {
        args.push("--max-filesize".into());
        args.push(max_size.to_string().into());
    }
    args.push("-o".into());
    args.push(dest.into());
    args.push(url.into());
    args
}

/// Downloads to `dest` with curl (resuming a partial file), reporting bytes so far to `progress`.
fn download(tools: &Tools, url: &str, dest: &Path, max_size: u64, cancel: &Cancel, progress: &dyn Fn(u64)) -> Result<(), String> {
    if let Some(dir) = dest.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    // A program file that is still open for writing somewhere (a forked process that hasn't exec'd yet) makes exec
    // fail with ETXTBSY for a moment. Only a safeguard: the tests write their fake curl so that this can't happen
    // (`testutil::write_executable`).
    let mut tries = 0;
    let mut child = loop {
        match Command::new(tools.curl).args(download_args(tools, url, dest, max_size)).stdout(Stdio::null()).stderr(Stdio::null()).spawn() {
            Err(e) if e.raw_os_error() == Some(libc::ETXTBSY) && tries < 100 => tries += 1,
            spawned => break spawned.map_err(|e| e.to_string())?,
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let too_large = || "The download is larger than announced. Nothing was installed.".to_string();
    loop {
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            progress(std::fs::metadata(dest).map_or(0, |m| m.len()));
            // 33: the server can't resume, but the file is already complete. 63: curl's --max-filesize.
            return match status.code() {
                _ if status.success() => Ok(()),
                Some(33) => Ok(()),
                Some(63) => Err(too_large()),
                // A release asset that was never uploaded (or a wrong name in the list): GitHub answers 404.
                Some(CURL_HTTP_ERROR) => Err("The update file isn't on the server. Nothing was installed.".into()),
                Some(CURL_PROTOCOL) => Err(NOT_HTTPS.into()),
                _ => Err(crate::net_check::refine("Download failed. Check Wi-Fi.")),
            };
        }
        let len = std::fs::metadata(dest).map_or(0, |m| m.len());
        progress(len);
        if cancel.is_cancelled() {
            let _ = child.kill();
            let _ = child.wait();
            return Err("Download cancelled.".into());
        }
        // A server that doesn't announce its size is caught here instead of by curl.
        if max_size > 0 && len > max_size {
            let _ = child.kill();
            let _ = child.wait();
            let _ = std::fs::remove_file(dest);
            return Err(too_large());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// Downloads `rel` to `dest` and checks size and SHA-256 against the signed list.
fn download_checked(tools: &Tools, rel: &Release, manifest_url: &str, dest: &Path, cancel: &Cancel, progress: &dyn Fn(u64)) -> Result<(), String> {
    if dest.exists() && std::fs::metadata(dest).map_or(0, |m| m.len()) > rel.size {
        let _ = std::fs::remove_file(dest);
    }
    let url = resolve(manifest_url, &rel.url);
    if !scheme_allowed(tools.protocols, &url) {
        return Err("The update list names a file that isn't on https. Nothing was installed.".into());
    }
    download(tools, &url, dest, rel.size, cancel, progress)?;
    let size = std::fs::metadata(dest).map_or(0, |m| m.len());
    let hash = store::sha256_file(dest).map_err(|e| e.to_string())?;
    if size != rel.size || hash != rel.sha256.to_lowercase() {
        let _ = std::fs::remove_file(dest);
        return Err("The download is damaged or was changed. Nothing was installed.".into());
    }
    Ok(())
}

/// Entries of a tar, refused if any would land outside the target directory.
fn tar_entries(tar: &Path) -> Result<Vec<String>, String> {
    let out = Command::new("tar").arg("-tf").arg(tar).output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err("The update file can't be read.".into());
    }
    let names: Vec<String> = String::from_utf8_lossy(&out.stdout).lines().map(|l| l.trim_start_matches("./").to_string()).collect();
    if names.iter().any(|n| n.starts_with('/') || n.split('/').any(|c| c == "..")) {
        return Err("The update contains unsafe paths. Nothing was installed.".into());
    }
    Ok(names)
}

// ---------------------------------------------------------------------------------------------
// App

fn link(target: &str, at: &Path) -> std::io::Result<()> {
    // A new link under a temporary name, renamed over the old one: never a moment without it.
    let tmp = at.with_extension("new-link");
    let _ = std::fs::remove_file(&tmp);
    std::os::unix::fs::symlink(target, &tmp)?;
    std::fs::rename(&tmp, at)
}

fn link_target(at: &Path) -> Option<String> {
    std::fs::read_link(at).ok().map(|p| p.to_string_lossy().into_owned())
}

/// Files a release carries and that are linked from the root.
const APP_FILES: [&str; 3] = ["pocket-ui", "pocket-core", "portmaster-setup.sh"];

/// Moves a flat install (copied files, as dev.sh used to deploy) into releases/<version>-local
/// so that it can become `previous`.
pub fn ensure_layout(root: &Path) -> Result<(), String> {
    if root.join("current").exists() {
        return Ok(());
    }
    let dir = format!("releases/{}-local", app_version());
    std::fs::create_dir_all(root.join(&dir)).map_err(|e| e.to_string())?;
    for f in APP_FILES {
        let flat = root.join(f);
        if flat.is_file() && !flat.is_symlink() {
            std::fs::rename(&flat, root.join(&dir).join(f)).map_err(|e| e.to_string())?;
        }
    }
    link(&dir, &root.join("current")).map_err(|e| e.to_string())?;
    link_files(root)
}

fn link_files(root: &Path) -> Result<(), String> {
    for f in APP_FILES {
        let at = root.join(f);
        if link_target(&at).as_deref() != Some(&format!("current/{f}")) {
            link(&format!("current/{f}"), &at).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// Downloads, checks and switches to `rel`. The caller restarts pocket-ui afterwards.
pub fn install_app(rel: &Release, manifest_url: &str, root: &Path, cancel: &Cancel, progress: &dyn Fn(u64)) -> Result<(), String> {
    ensure_layout(root)?;
    let releases = root.join("releases");
    clear_leftovers(&releases, &rel.version);
    let tarball = releases.join(format!(".download-{}.tar.gz", rel.version));
    download_checked(&TOOLS, rel, manifest_url, &tarball, cancel, progress)?;

    // Only plain files and folders, no setuid bits (B4); see untar.rs.
    let staging = releases.join(format!(".staging-{}", rel.version));
    let _ = std::fs::remove_dir_all(&staging);
    let unpacked = std::fs::read(&tarball).map_err(|e| e.to_string()).and_then(|data| crate::untar::unpack(&data, &staging));
    let _ = std::fs::remove_file(&tarball);
    let names = unpacked?;
    for required in ["pocket-ui", "pocket-core"] {
        if !names.iter().any(|n| n.path == required && !n.dir) {
            let _ = std::fs::remove_dir_all(&staging);
            return Err(format!("The update is missing {required}. Nothing was installed."));
        }
    }
    // Both programs have to run on this device and report the promised version.
    for bin in ["pocket-ui", "pocket-core"] {
        let out = Command::new(staging.join(bin)).arg("--version").output();
        let ok = out.as_ref().is_ok_and(|o| o.status.success() && reports_version(&String::from_utf8_lossy(&o.stdout), bin, &rel.version));
        if !ok {
            let _ = std::fs::remove_dir_all(&staging);
            return Err(format!("The new {bin} doesn't start on this device. Nothing was changed."));
        }
    }

    let dir = format!("releases/{}", rel.version);
    let _ = std::fs::remove_dir_all(root.join(&dir));
    std::fs::rename(&staging, root.join(&dir)).map_err(|e| e.to_string())?;
    if let Some(old) = link_target(&root.join("current")).filter(|old| *old != dir) {
        link(&old, &root.join("previous")).map_err(|e| e.to_string())?;
    }
    crate::fsutil::write_atomic(&root.join("update-pending"), rel.version.as_bytes()).map_err(|e| e.to_string())?;
    link(&dir, &root.join("current")).map_err(|e| e.to_string())?;
    link_files(root)?;
    prune(root);
    Ok(())
}

/// What `<bin> --version` has to print: exactly `<bin> <version>`. A substring test would take
/// "pocket-ui 11.2.3" for 1.2.3.
fn reports_version(stdout: &str, bin: &str, version: &str) -> bool {
    stdout.trim() == format!("{bin} {version}")
}

/// Removes releases other than current, previous and known-good.
/// Hidden work files of an install that never finished (power loss, a version that was never retried):
/// `prune` leaves hidden entries alone, so they would stay for good. The download of `version` stays so
/// it can resume; a half unpacked folder is never reused.
fn clear_leftovers(releases: &Path, version: &str) {
    let resume = format!(".download-{version}.tar.gz");
    let Ok(entries) = std::fs::read_dir(releases) else { return };
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if (name.starts_with(".download-") || name.starts_with(".staging-")) && name != resume {
            let _ = if e.path().is_dir() { std::fs::remove_dir_all(e.path()) } else { std::fs::remove_file(e.path()) };
        }
    }
}

fn prune(root: &Path) {
    let keep: Vec<String> = ["current", "previous", "known-good"].iter().filter_map(|l| link_target(&root.join(l))).collect();
    let Ok(entries) = std::fs::read_dir(root.join("releases")) else { return };
    for e in entries.flatten() {
        let rel = format!("releases/{}", e.file_name().to_string_lossy());
        if !keep.contains(&rel) && !e.file_name().to_string_lossy().starts_with('.') {
            let _ = std::fs::remove_dir_all(e.path());
        }
    }
}

/// Called once this version has run long enough: the update is kept. Only the version the update
/// installed may confirm it; the old process that installed it is still running for a moment.
pub fn confirm_running(root: &Path) {
    if pending_is_ours(root) {
        let _ = std::fs::remove_file(root.join("update-pending"));
    }
}

/// True while update-pending names the version that is running now.
fn pending_is_ours(root: &Path) -> bool {
    std::fs::read_to_string(root.join("update-pending")).is_ok_and(|v| v.trim() == app_version())
}

static FRAME_PRESENTED: AtomicBool = AtomicBool::new(false);

/// The UI calls this once a frame is on screen and the event loop has turned afterwards.
pub fn mark_frame_presented() {
    FRAME_PRESENTED.store(true, Ordering::SeqCst);
}

pub fn frame_presented() -> bool {
    FRAME_PRESENTED.load(Ordering::SeqCst)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Verdict {
    Wait,
    Confirm,
    GiveUp,
}

/// What to do about a just-installed version, given whether it has presented a frame and how
/// long the process has been running.
pub fn health_verdict(presented: bool, elapsed: Duration, watchdog: Duration) -> Verdict {
    if presented {
        Verdict::Confirm
    } else if elapsed >= watchdog {
        Verdict::GiveUp
    } else {
        Verdict::Wait
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Health {
    /// update-pending doesn't name this version: nothing to confirm, nothing to watch.
    NotPending,
    /// The first frame appeared; update-pending is gone and the launcher keeps this version.
    Confirmed,
    /// No frame within the watchdog time; update-pending is still there for the launcher.
    TimedOut,
}

/// Runs on its own thread from the start of the process (`started`). Returns once update-pending
/// is gone because the first frame appeared (`Confirmed`), or when `watchdog` has passed without
/// one (`TimedOut`: the caller exits with `WATCHDOG_EXIT` and update-pending stays for the launcher).
pub fn watch_health(root: &Path, started: Instant, presented: &dyn Fn() -> bool, poll: Duration, watchdog: Duration) -> Health {
    if !pending_is_ours(root) {
        return Health::NotPending;
    }
    loop {
        match health_verdict(presented(), started.elapsed(), watchdog) {
            Verdict::Confirm => {
                confirm_running(root);
                return Health::Confirmed;
            }
            Verdict::GiveUp => return Health::TimedOut,
            Verdict::Wait => std::thread::sleep(poll),
        }
    }
}

/// Whether an update that has been running since `busy_since` still keeps auto-sleep off.
pub fn busy_blocks_sleep(busy_since: Option<Instant>, now: Instant) -> bool {
    busy_since.is_some_and(|since| now.saturating_duration_since(since) < BUSY_BLOCKS_SLEEP_FOR)
}

/// The version the launcher gave up on and switched away from, reported once.
pub fn take_rollback_note(root: &Path) -> Option<String> {
    let path = root.join("rolled-back");
    let note = std::fs::read_to_string(&path).ok()?;
    let _ = std::fs::remove_file(path);
    Some(note.trim().to_string())
}

// ---------------------------------------------------------------------------------------------
// OS

/// A field of an os-release file.
pub fn os_release_field(text: &str, key: &str) -> Option<String> {
    text.lines().find_map(|l| l.strip_prefix(key)?.strip_prefix('=').map(|v| v.trim().trim_matches('"').to_string()))
}

pub fn os_version() -> Option<String> {
    os_release_field(&std::fs::read_to_string("/etc/os-release").ok()?, "OS_VERSION")
}

/// ROCKNIX started from a computer with `fastboot boot` (our test image adds `nodtbo`, see
/// docs/FASTBOOT-TEST.md). Its kernel comes from that image, not from the SD card, so after an OS
/// update the image has to be rebuilt from the new KERNEL.
pub fn booted_from_ram() -> bool {
    std::fs::read_to_string("/proc/cmdline").is_ok_and(|c| cmdline_has_nodtbo(&c))
}

fn cmdline_has_nodtbo(cmdline: &str) -> bool {
    cmdline.split_whitespace().any(|a| a == "nodtbo")
}

/// Words that have no business in an update script that only copies files to the SD card.
const BOOTLOADER_MARKERS: [&str; 12] =
    ["updateabl", "abl_", "/dev/block", "/dev/mmcblk", "/dev/sd", "/dev/ufs", "ufshc", "partlabel", "by-name", "fastboot", "flash_image", "dd "];

/// Refuses an OS update whose boot-time scripts could write a bootloader or a partition.
/// `update_sh`/`canupdate_sh` are the new SYSTEM's usr/share/bootloader scripts, which the
/// initramfs runs while installing; `ships_updateabl` says whether usr/bin/updateabl exists in it.
pub fn bootloader_check(update_sh: Option<&str>, canupdate_sh: Option<&str>, ships_updateabl: bool) -> Result<(), String> {
    if ships_updateabl {
        return Err("This system update contains the tool that rewrites the internal bootloader. It was not installed.".into());
    }
    for script in [update_sh, canupdate_sh].into_iter().flatten() {
        let lower = script.to_lowercase();
        let code: String = lower.lines().filter(|l| !l.trim_start().starts_with('#')).collect::<Vec<_>>().join("\n");
        if let Some(m) = BOOTLOADER_MARKERS.iter().find(|m| code.contains(*m)) {
            return Err(format!("This system update would touch the bootloader or a partition (\"{}\"). It was not installed.", m.trim()));
        }
    }
    Ok(())
}

/// A text file of the new system, links followed.
fn read_text(sys: &dyn System, path: &str) -> Option<String> {
    let resolved = os_compat::resolve(sys, path)?;
    sys.read(&resolved).map(|b| String::from_utf8_lossy(&b).into_owned())
}

/// What the checks found in a new system that may be installed.
#[derive(Debug)]
pub struct OsCheck {
    /// Its OS_VERSION.
    pub version: String,
    /// Why its boot scripts may not run (`bootloader_check`).
    pub bootloader: Result<(), String>,
    pub report: os_compat::Report,
}

impl OsCheck {
    /// Why it may not be installed: the bootloader guard first, then what pocket-ui needs.
    pub fn refusal(&self) -> Option<String> {
        self.bootloader.clone().err().or_else(|| self.report.refusal())
    }
}

/// Checks a new system: the same device (`current`: this system's os-release; None skips that, for a dry run away
/// from ROCKNIX; Err with the message for the Updates page), nothing that writes bootloaders, and what pocket-ui
/// needs (`needs`). Both are looked at in full; `OsCheck::refusal` says whether it may be installed.
pub fn inspect_system(sys: &dyn System, current: Option<&str>, needs: &Needs) -> Result<OsCheck, String> {
    let os_release = read_text(sys, "etc/os-release")
        .or_else(|| read_text(sys, "usr/lib/os-release"))
        .ok_or("The new system has no version information. It was not installed.")?;
    if let Some(current) = current {
        for key in ["HW_DEVICE", "HW_ARCH"] {
            if os_release_field(&os_release, key) != os_release_field(current, key) {
                return Err("This system update is for a different device. It was not installed.".into());
            }
        }
    }
    let version = os_release_field(&os_release, "OS_VERSION").unwrap_or_default();
    let bootloader = bootloader_check(
        read_text(sys, "usr/share/bootloader/update.sh").as_deref(),
        read_text(sys, "usr/share/bootloader/canupdate.sh").as_deref(),
        sys.entry("usr/bin/updateabl").is_some(),
    );
    Ok(OsCheck { version, bootloader, report: os_compat::check(sys, needs) })
}

/// Opens the SYSTEM inside an update tar (unpacked into `work`, removed again) and checks it (`inspect_system`).
fn check_os_tar(tools: &Tools, tar: &Path, work: &Path, current: Option<&str>, needs: &Needs) -> Result<OsCheck, String> {
    let names = tar_entries(tar)?;
    let find = |file: &str| names.iter().find(|n| n.ends_with(&format!("/target/{file}")) && n.matches('/').count() == 2).cloned();
    let (Some(system), Some(_), Some(_), Some(_)) = (find("SYSTEM"), find("KERNEL"), find("SYSTEM.md5"), find("KERNEL.md5")) else {
        return Err("This is not a complete system update. It was not installed.".into());
    };
    let _ = std::fs::remove_dir_all(work);
    std::fs::create_dir_all(work).map_err(|e| e.to_string())?;
    let extracted = Command::new("tar").arg("-xf").arg(tar).arg("-C").arg(work).arg(&system).status().is_ok_and(|s| s.success());
    let image = work.join(&system);
    let result = (|| {
        if !extracted {
            return Err("Couldn't unpack the system update.".to_string());
        }
        inspect_system(&os_compat::Squashfs::open(tools.unsquashfs, &image)?, current, needs)
    })();
    let _ = std::fs::remove_dir_all(work);
    result
}

/// `pocket-ui --check-os-update <path>`: the checks of `stage_os` on an update tar, a SYSTEM squashfs or an unpacked
/// system folder, printed in full, without staging anything. Exit code 0: it would be installed (maybe with notes),
/// 1: it would be refused, 2: no path given.
pub fn check_os_update_cli(path: Option<&str>) -> i32 {
    let Some(path) = path.map(Path::new) else {
        eprintln!("usage: pocket-ui --check-os-update <ROCKNIX-...tar | SYSTEM | unpacked system folder>");
        return 2;
    };
    let current = std::fs::read_to_string("/etc/os-release").ok().filter(|t| os_release_field(t, "HW_DEVICE").is_some());
    let needs = Needs::installed(Path::new(APP_ROOT));
    println!("Checking {} for pocket-ui {}", path.display(), app_version());
    if current.is_none() {
        println!("info   not on ROCKNIX: the device is not compared");
    }
    let result = if path.is_dir() {
        inspect_system(&os_compat::Dir(path.to_path_buf()), current.as_deref(), &needs)
    } else if path.extension().is_some_and(|e| e == "tar") {
        // The SYSTEM is unpacked on the card next to where stage_os unpacks it (it is over a gigabyte, /tmp is RAM).
        let work = if Path::new("/storage").is_dir() { Path::new(OS_STAGING).join("check-dry-run") } else { std::env::temp_dir().join(format!("pocket-ui-os-check-{}", std::process::id())) };
        check_os_tar(&TOOLS, path, &work, current.as_deref(), &needs)
    } else {
        os_compat::Squashfs::open(TOOLS.unsquashfs, path).and_then(|sys| inspect_system(&sys, current.as_deref(), &needs))
    };
    match result {
        Ok(c) => {
            print!("{}", c.report.text());
            if let Err(e) = &c.bootloader {
                println!("BLOCKS {e}");
            }
            if let Some(refusal) = c.refusal() {
                println!("REFUSED system {}: {refusal}", c.version);
                return 1;
            }
            println!("system {}: would be installed{}", c.version, if c.report.warnings().is_empty() { "" } else { ", with the notes above" });
            0
        }
        Err(e) => {
            println!("REFUSED {e}");
            1
        }
    }
}

/// Downloads and checks an OS release and places it where the initramfs installs it at the next
/// start. Nothing else on the device changes. Ok: the note for the Updates page when the new system lacks something
/// pocket-ui uses (`os_compat::Report::note`); the full report goes to the log either way.
pub fn stage_os(rel: &Release, manifest_url: &str, cancel: &Cancel, progress: &dyn Fn(u64)) -> Result<Option<String>, String> {
    let current = std::fs::read_to_string("/etc/os-release").unwrap_or_default();
    let needs = Needs::installed(Path::new(APP_ROOT));
    stage_os_in(&TOOLS, Path::new(OS_STAGING), Path::new(OS_UPDATE_DIR), &current, &needs, rel, manifest_url, cancel, progress)
}

#[allow(clippy::too_many_arguments)]
fn stage_os_in(
    tools: &Tools,
    staging: &Path,
    update_dir: &Path,
    current: &str,
    needs: &Needs,
    rel: &Release,
    manifest_url: &str,
    cancel: &Cancel,
    progress: &dyn Fn(u64),
) -> Result<Option<String>, String> {
    let name = rel.url.rsplit('/').next().unwrap_or_default();
    if !name.ends_with(".tar") || name.contains("..") {
        return Err("The system update has an unexpected file name.".into());
    }
    let tar = staging.join(name);
    download_checked(tools, rel, manifest_url, &tar, cancel, progress)?;
    let checked = check_os_tar(tools, &tar, &staging.join("check"), Some(current), needs).inspect_err(|e| {
        eprintln!("update: system {} refused: {e}", rel.version);
        let _ = std::fs::remove_file(&tar);
    })?;
    eprint!("update: system {} checked:\n{}", rel.version, checked.report.text());
    if let Some(refusal) = checked.refusal() {
        eprintln!("update: system {} refused: {refusal}", rel.version);
        let _ = std::fs::remove_file(&tar);
        return Err(refusal);
    }
    if checked.version != rel.version {
        let _ = std::fs::remove_file(&tar);
        return Err("The system update doesn't match its description. It was not installed.".into());
    }
    cancel_os_in(update_dir);
    std::fs::create_dir_all(update_dir).map_err(|e| e.to_string())?;
    std::fs::rename(&tar, update_dir.join(name)).map_err(|e| e.to_string())?;
    Ok(checked.report.note())
}

/// The update waiting for the next start, if any.
pub fn staged_os() -> Option<PathBuf> {
    staged_os_in(Path::new(OS_UPDATE_DIR))
}

fn staged_os_in(dir: &Path) -> Option<PathBuf> {
    std::fs::read_dir(dir).ok()?.flatten().map(|e| e.path()).find(|p| p.extension().is_some_and(|e| e == "tar"))
}

/// Takes a waiting update back out.
pub fn cancel_os() {
    cancel_os_in(Path::new(OS_UPDATE_DIR));
}

fn cancel_os_in(dir: &Path) {
    while let Some(p) = staged_os_in(dir) {
        if std::fs::remove_file(&p).is_err() {
            break;
        }
    }
}

/// Reads at most `limit` bytes of a small text file.
pub fn read_small(path: &Path, limit: u64) -> Option<String> {
    let mut s = String::new();
    std::fs::File::open(path).ok()?.take(limit).read_to_string(&mut s).ok()?;
    Some(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;
    use base64::Engine;
    use ed25519_dalek::{Signer, SigningKey};

    #[test]
    fn compares_versions_by_number_groups() {
        assert!(newer("0.10.0", "0.9.3"));
        assert!(newer("0.1.1", "0.1.0"));
        assert!(!newer("0.1.0", "0.1.0"));
        assert!(!newer("0.1.0", "0.1.0-local"));
        assert!(newer("20261101", "20261001"));
    }

    #[test]
    fn resolves_relative_urls_next_to_the_list() {
        assert_eq!(resolve("https://h/x/update.json", "a.tar.gz"), "https://h/x/a.tar.gz");
        assert_eq!(resolve("https://h/x/update.json", "https://o/b.tar"), "https://o/b.tar");
    }

    #[test]
    fn manifest_must_be_signed_by_our_key() {
        let key = SigningKey::from_bytes(&[3u8; 32]);
        let json = br#"{"format":1,"app":{"version":"0.2.0","url":"a.tar.gz","sha256":"00","size":1}}"#;
        let sig = base64::engine::general_purpose::STANDARD.encode(key.sign(json).to_bytes());
        let m = parse_manifest(json, &sig, &key.verifying_key().to_bytes()).unwrap();
        assert_eq!(m.app.unwrap().version, "0.2.0");
        assert!(m.os.is_none());
        let other = SigningKey::from_bytes(&[4u8; 32]).verifying_key().to_bytes();
        assert!(parse_manifest(json, &sig, &other).is_err());
    }

    #[test]
    fn refuses_rocknix_bootloader_update_script() {
        // The relevant lines of ROCKNIX's SM8250 usr/share/bootloader/update.sh (20261001).
        let rocknix = "mount -o remount,rw $BOOT_ROOT\n\
            if [ -d \"$SYSTEM_ROOT/usr/share/bootloader/rocknix_abl\" ]; then\n  cp $SYSTEM_ROOT/usr/share/bootloader/rocknix_abl/* $BOOT_ROOT/rocknix_abl\nfi\n\
            $SYSTEM_ROOT/usr/bin/updateabl\nsync\n";
        assert!(bootloader_check(Some(rocknix), None, false).is_err());
        assert!(bootloader_check(None, None, true).is_err(), "updateabl in the image is enough to refuse");
        assert!(bootloader_check(None, Some("dd if=x of=/dev/mmcblk0p1"), false).is_err());
        let ours = "# no updateabl here, only boot files on the SD card\nmount -o remount,rw $BOOT_ROOT\nsync\nmount -o remount,ro $BOOT_ROOT\n";
        assert!(bootloader_check(Some(ours), None, false).is_ok(), "comments don't count");
        assert!(bootloader_check(None, None, false).is_ok());
    }

    #[test]
    fn reads_os_release_fields() {
        let text = "OS_NAME=\"ROCKNIX\"\nOS_VERSION=\"20261001\"\nHW_DEVICE=\"SM8250\"\n";
        assert_eq!(os_release_field(text, "OS_VERSION").as_deref(), Some("20261001"));
        assert_eq!(os_release_field(text, "HW_DEVICE").as_deref(), Some("SM8250"));
        assert_eq!(os_release_field(text, "HW"), None);
    }

    /// A release whose programs answer `--version`, served from a file:// URL.
    fn fake_release(dir: &Path, version: &str, works: bool) -> (Release, String) {
        fake_release_reporting(dir, version, works.then_some(version))
    }

    /// Like `fake_release`, but the programs print `reported` as their version (None: they fail).
    fn fake_release_reporting(dir: &Path, version: &str, reported: Option<&str>) -> (Release, String) {
        let src = dir.join(format!("src-{version}"));
        std::fs::create_dir_all(&src).unwrap();
        for bin in ["pocket-ui", "pocket-core"] {
            let script = match reported {
                Some(v) => format!("#!/bin/sh\necho {bin} {v}\n"),
                None => "#!/bin/sh\nexit 1\n".to_string(),
            };
            crate::testutil::write_executable(&src.join(bin), script);
        }
        let tar = dir.join(format!("pocket-ui-{version}.tar.gz"));
        assert!(Command::new("tar").arg("-czf").arg(&tar).arg("-C").arg(&src).args(["pocket-ui", "pocket-core"]).status().unwrap().success());
        let data = std::fs::read(&tar).unwrap();
        let rel = Release {
            version: version.into(),
            url: format!("pocket-ui-{version}.tar.gz"),
            sha256: store::sha256_hex(&data),
            size: data.len() as u64,
            notes: String::new(),
        };
        (rel, format!("file://{}/update.json", dir.display()))
    }

    #[test]
    fn installs_switches_and_keeps_the_previous_version() {
        let dir = TestDir::new("update");
        let root = dir.join("root");
        std::fs::create_dir_all(&root).unwrap();
        // A flat dev install.
        std::fs::write(root.join("pocket-ui"), "old").unwrap();
        std::fs::write(root.join("pocket-core"), "old").unwrap();

        let (rel, url) = fake_release(&dir, "9.1.0", true);
        install_app(&rel, &url, &root, &Cancel::new(), &|_| {}).unwrap();
        assert_eq!(link_target(&root.join("current")).unwrap(), "releases/9.1.0");
        assert_eq!(link_target(&root.join("previous")).unwrap(), format!("releases/{}-local", app_version()));
        assert!(std::fs::read_to_string(root.join("pocket-ui")).unwrap().contains("9.1.0"), "root link follows current");
        assert_eq!(std::fs::read_to_string(root.join("update-pending")).unwrap(), "9.1.0");
        confirm_running(&root);
        assert!(root.join("update-pending").exists(), "the old version must not confirm the new one");

        let (rel2, url2) = fake_release(&dir, "9.2.0", true);
        install_app(&rel2, &url2, &root, &Cancel::new(), &|_| {}).unwrap();
        assert_eq!(link_target(&root.join("previous")).unwrap(), "releases/9.1.0");
        assert!(!root.join(format!("releases/{}-local", app_version())).exists(), "older releases are pruned");

        // A build that doesn't run is refused and nothing changes.
        let (bad, url3) = fake_release(&dir, "9.3.0", false);
        assert!(install_app(&bad, &url3, &root, &Cancel::new(), &|_| {}).is_err());
        assert_eq!(link_target(&root.join("current")).unwrap(), "releases/9.2.0");

        // A damaged download is refused.
        let (mut wrong, url4) = fake_release(&dir, "9.4.0", true);
        wrong.sha256 = "0".repeat(64);
        assert!(install_app(&wrong, &url4, &root, &Cancel::new(), &|_| {}).is_err());
        assert_eq!(link_target(&root.join("current")).unwrap(), "releases/9.2.0");
    }

    /// B4: a release with a link in it is refused; setuid bits and odd modes don't survive.
    #[test]
    fn release_with_links_is_refused_and_modes_are_normalised() {
        use std::os::unix::fs::PermissionsExt;
        let mode = |p: &Path| std::fs::symlink_metadata(p).unwrap().permissions().mode() & 0o7777;
        let dir = TestDir::new("update-b4");
        let root = dir.join("root");
        std::fs::create_dir_all(&root).unwrap();
        // Repacks a working release with `extra` added next to the programs.
        let repack = |version: &str, extra: &dyn Fn(&Path)| -> (Release, String) {
            let (mut rel, url) = fake_release(&dir, version, true);
            let src = dir.join(format!("src-{version}"));
            extra(&src);
            let tar = dir.join(format!("pocket-ui-{version}.tar.gz"));
            assert!(Command::new("tar").env("COPYFILE_DISABLE", "1").arg("-czf").arg(&tar).arg("-C").arg(&src).arg(".").status().unwrap().success());
            let data = std::fs::read(&tar).unwrap();
            rel.sha256 = store::sha256_hex(&data);
            rel.size = data.len() as u64;
            (rel, url)
        };

        let (rel, url) = repack("9.1.0", &|src| std::os::unix::fs::symlink("/etc", src.join("etc")).unwrap());
        assert!(install_app(&rel, &url, &root, &Cancel::new(), &|_| {}).is_err(), "a release with a symlink is refused");
        assert!(!root.join("releases/9.1.0").exists());

        let (rel, url) = repack("9.2.0", &|src| {
            std::fs::set_permissions(src.join("pocket-core"), std::fs::Permissions::from_mode(0o4755)).unwrap();
            std::fs::write(src.join("notes.txt"), "n").unwrap();
            std::fs::set_permissions(src.join("notes.txt"), std::fs::Permissions::from_mode(0o600)).unwrap();
        });
        install_app(&rel, &url, &root, &Cancel::new(), &|_| {}).unwrap();
        assert_eq!(mode(&root.join("releases/9.2.0/pocket-core")), 0o755, "no setuid");
        assert_eq!(mode(&root.join("releases/9.2.0/notes.txt")), 0o644);
        assert_eq!(mode(&root.join("releases/9.2.0")), 0o755);
    }

    // -----------------------------------------------------------------------------------------
    // Helpers for the tests below

    /// A scratch directory that is removed again when the test ends.
    struct TempDir(crate::testutil::TestDir);

    impl TempDir {
        fn new(name: &str) -> Self {
            let dir = TestDir::new(&format!("update-{name}"));
            TempDir(dir)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }


    /// An executable shell script `dir/name`; returns its path.
    fn script(dir: &Path, name: &str, body: &str) -> String {
        let path = dir.join(name);
        crate::testutil::write_executable(&path, format!("#!/bin/sh\n{body}\n"));
        path.to_string_lossy().into_owned()
    }

    /// A curl that runs `body`, with `$out` set to the file given after `-o`.
    fn fake_curl(dir: &Path, body: &str) -> String {
        script(dir, "curl", &format!("while [ $# -gt 0 ]; do [ \"$1\" = -o ] && out=$2; shift; done\n{body}"))
    }

    fn pid_alive(pid: i32) -> bool {
        // SAFETY: signal 0 only checks that the process exists.
        unsafe { libc::kill(pid, 0) == 0 }
    }

    // -----------------------------------------------------------------------------------------
    // B11: an update counts as working once it has presented a frame, not after a timer

    #[test]
    fn health_verdict_waits_confirms_or_gives_up() {
        let w = Duration::from_secs(20);
        assert_eq!(health_verdict(false, Duration::ZERO, w), Verdict::Wait);
        assert_eq!(health_verdict(false, Duration::from_secs(13), w), Verdict::Wait, "13 s without a frame is no reason to confirm");
        assert_eq!(health_verdict(false, w - Duration::from_millis(1), w), Verdict::Wait);
        assert_eq!(health_verdict(false, w, w), Verdict::GiveUp);
        assert_eq!(health_verdict(false, Duration::from_secs(300), w), Verdict::GiveUp);
        assert_eq!(health_verdict(true, Duration::ZERO, w), Verdict::Confirm, "a frame confirms at once");
        assert_eq!(health_verdict(true, Duration::from_secs(300), w), Verdict::Confirm, "a late frame still beats the watchdog");
    }

    #[test]
    fn update_is_confirmed_when_the_first_frame_appears() {
        use std::sync::atomic::AtomicUsize;
        let tmp = TempDir::new("health-ok");
        std::fs::write(tmp.path().join("update-pending"), app_version()).unwrap();
        let polls = AtomicUsize::new(0);
        let pending = tmp.path().join("update-pending");
        let presented = || {
            let n = polls.fetch_add(1, Ordering::SeqCst);
            if n < 8 {
                assert!(pending.exists(), "update-pending must stay until the frame is there");
            }
            n >= 8
        };
        let start = Instant::now();
        assert_eq!(watch_health(tmp.path(), start, &presented, Duration::from_millis(20), Duration::from_secs(30)), Health::Confirmed);
        assert!(!pending.exists(), "confirmed: the launcher keeps this version");
        assert!(polls.load(Ordering::SeqCst) >= 9, "waited for the frame instead of a timer");
        assert!(start.elapsed() >= Duration::from_millis(140) && start.elapsed() < Duration::from_secs(10), "{:?}", start.elapsed());
    }

    #[test]
    fn hanging_before_the_first_frame_times_out_and_leaves_update_pending_for_the_launcher() {
        let tmp = TempDir::new("health-hang");
        let pending = tmp.path().join("update-pending");
        std::fs::write(&pending, app_version()).unwrap();
        let start = Instant::now();
        assert_eq!(watch_health(tmp.path(), start, &|| false, Duration::from_millis(10), Duration::from_millis(200)), Health::TimedOut);
        assert!(start.elapsed() >= Duration::from_millis(200), "not before the deadline: {:?}", start.elapsed());
        assert!(start.elapsed() < Duration::from_secs(10), "{:?}", start.elapsed());
        assert!(pending.exists(), "the launcher rolls back only while update-pending is there");
        // The deadline counts from the start of the process, not from when the watcher began.
        let long_ago = Instant::now() - Duration::from_secs(3600);
        assert_eq!(watch_health(tmp.path(), long_ago, &|| false, Duration::from_millis(10), Duration::from_secs(20)), Health::TimedOut);
    }

    #[test]
    fn nothing_is_watched_when_this_version_is_not_pending() {
        let tmp = TempDir::new("health-none");
        let never = || -> bool { panic!("must not even look for a frame") };
        assert_eq!(watch_health(tmp.path(), Instant::now(), &never, Duration::from_millis(10), Duration::from_millis(50)), Health::NotPending);
        std::fs::write(tmp.path().join("update-pending"), "99.0.0").unwrap();
        assert_eq!(watch_health(tmp.path(), Instant::now(), &never, Duration::from_millis(10), Duration::from_millis(50)), Health::NotPending);
        assert!(tmp.path().join("update-pending").exists(), "another version's marker is left alone");
    }

    #[test]
    fn frame_flag_is_set_by_mark_frame_presented() {
        // Process-wide, so this is the only test that touches it.
        assert!(!frame_presented());
        mark_frame_presented();
        assert!(frame_presented());
    }

    #[test]
    fn watchdog_exit_is_a_code_the_launcher_rolls_back_on() {
        assert_ne!(WATCHDOG_EXIT, 0);
        assert_ne!(WATCHDOG_EXIT, RESTART_CODE);
        // The launcher's own codes (0 and its CODE_* lines) are the only ones it does not treat as a failed start, and
        // a failed start of a version with update-pending is rolled back.
        let launcher = include_str!("../deploy/run-pocket-ui.sh");
        let special: Vec<i32> = launcher.lines().filter_map(|l| l.strip_prefix("CODE_")?.split_once('=')?.1.parse().ok()).collect();
        assert!(special.contains(&RESTART_CODE), "CODE_UPDATE in {special:?}");
        assert!(!special.contains(&WATCHDOG_EXIT), "{special:?}");
        assert!(launcher.contains(r#"if [ "$stage" = normal ] && can_roll_back; then"#), "launcher rolls back on a failed start");
        assert_eq!(RESTART_CODE, 75);
        assert!(WATCHDOG_AFTER >= Duration::from_secs(10) && WATCHDOG_AFTER <= Duration::from_secs(60));
    }

    #[test]
    fn confirm_running_removes_the_marker_of_this_version_only() {
        let tmp = TempDir::new("confirm");
        let pending = tmp.path().join("update-pending");
        confirm_running(tmp.path()); // nothing there: no panic
        std::fs::write(&pending, format!("{}\n", app_version())).unwrap();
        confirm_running(tmp.path());
        assert!(!pending.exists(), "the version that was installed confirms itself");
        std::fs::write(&pending, "99.0.0").unwrap();
        confirm_running(tmp.path());
        assert!(pending.exists(), "another version's marker stays");
    }

    // -----------------------------------------------------------------------------------------
    // Exact version check

    #[test]
    fn version_output_must_match_exactly() {
        assert!(reports_version("pocket-ui 1.2.3\n", "pocket-ui", "1.2.3"));
        assert!(reports_version("pocket-core 1.2.3\r\n", "pocket-core", "1.2.3"));
        assert!(!reports_version("pocket-ui 11.2.3\n", "pocket-ui", "1.2.3"), "11.2.3 is not 1.2.3");
        assert!(!reports_version("pocket-ui 1.2.34\n", "pocket-ui", "1.2.3"));
        assert!(!reports_version("pocket-ui 1.2.3-evil\n", "pocket-ui", "1.2.3"));
        assert!(!reports_version("pocket-core 1.2.3\n", "pocket-ui", "1.2.3"), "the other program's name");
        assert!(!reports_version("error: pocket-ui 1.2.3 failed\n", "pocket-ui", "1.2.3"));
        assert!(!reports_version("", "pocket-ui", "1.2.3"));
    }

    #[test]
    fn install_refuses_a_build_whose_version_only_contains_the_promised_one() {
        let tmp = TempDir::new("exact-version");
        let root = tmp.path().join("root");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("pocket-ui"), "old").unwrap();
        std::fs::write(root.join("pocket-core"), "old").unwrap();
        // The list promises 1.2.3; the programs say 11.2.3.
        let (rel, url) = fake_release_reporting(tmp.path(), "1.2.3", Some("11.2.3"));
        let err = install_app(&rel, &url, &root, &Cancel::new(), &|_| {}).unwrap_err();
        assert!(err.contains("doesn't start"), "{err}");
        assert!(!root.join("update-pending").exists());
        assert!(!root.join("releases/1.2.3").exists());
    }

    // -----------------------------------------------------------------------------------------
    // Downloads: timeouts, size limit, cancel

    fn strs(args: &[OsString]) -> Vec<String> {
        args.iter().map(|a| a.to_string_lossy().into_owned()).collect()
    }

    fn has_pair(args: &[String], key: &str, value: &str) -> bool {
        args.windows(2).any(|w| w[0] == key && w[1] == value)
    }

    #[test]
    fn curl_gets_timeouts_and_the_size_from_the_signed_list() {
        let args = strs(&download_args(&TOOLS, "https://h/u.tar.gz", Path::new("/d/u.tar.gz"), 1234));
        assert!(has_pair(&args, "--connect-timeout", "20"), "{args:?}");
        assert!(args[0].contains('L'), "follows redirects (GitHub Releases to its download host): {args:?}");
        assert!(has_pair(&args, "--proto", "=https,file"), "https (or a file on the card) only: {args:?}");
        assert!(has_pair(&args, "--proto-redir", "=https"), "a redirect may only lead to https: {args:?}");
        assert!(has_pair(&args, "--speed-limit", "10240"), "{args:?}");
        assert!(has_pair(&args, "--speed-time", "30"), "{args:?}");
        assert!(has_pair(&args, "--max-filesize", "1234"), "{args:?}");
        assert!(has_pair(&args, "-C", "-"), "still resumes: {args:?}");
        assert!(has_pair(&args, "--retry", "3"), "{args:?}");
        assert!(has_pair(&args, "-o", "/d/u.tar.gz"), "{args:?}");
        assert_eq!(args.last().unwrap(), "https://h/u.tar.gz");
        let unknown = strs(&download_args(&TOOLS, "u", Path::new("/d/f"), 0));
        assert!(!unknown.iter().any(|a| a == "--max-filesize"), "no limit when the size is unknown: {unknown:?}");
    }

    #[test]
    fn cancel_stops_curl_and_returns_quickly() {
        let tmp = TempDir::new("cancel");
        let curl = fake_curl(tmp.path(), "echo $$ > \"$out.pid\"\nexec sleep 5");
        let tools = Tools { curl: &curl, unsquashfs: "unsquashfs", ..TOOLS };
        let dest = tmp.path().join("f.tar.gz");
        let cancel = Cancel::new();
        let flag = cancel.clone();
        let pidfile = tmp.path().join("f.tar.gz.pid");
        let t = std::thread::spawn(move || {
            // Cancel once curl is really running (the machine may be busy).
            while !pidfile.exists() {
                std::thread::sleep(Duration::from_millis(10));
            }
            std::thread::sleep(Duration::from_millis(200));
            flag.cancel();
        });
        let start = Instant::now();
        let err = download(&tools, "https://h/f", &dest, 1000, &cancel, &|_| {}).unwrap_err();
        t.join().unwrap();
        assert!(err.to_lowercase().contains("cancel"), "{err}");
        assert!(start.elapsed() < Duration::from_secs(4), "{:?}", start.elapsed());
        let pid: i32 = std::fs::read_to_string(tmp.path().join("f.tar.gz.pid")).unwrap().trim().parse().unwrap();
        assert!(!pid_alive(pid), "curl was killed and collected");
    }

    #[test]
    fn a_download_larger_than_announced_is_stopped_and_removed() {
        let tmp = TempDir::new("oversize");
        let curl = fake_curl(tmp.path(), "echo $$ > \"$out.pid\"\nhead -c 5000 /dev/zero > \"$out\"\nexec sleep 5");
        let tools = Tools { curl: &curl, unsquashfs: "unsquashfs", ..TOOLS };
        let dest = tmp.path().join("f.tar.gz");
        let start = Instant::now();
        let err = download(&tools, "https://h/f", &dest, 1000, &Cancel::new(), &|_| {}).unwrap_err();
        assert!(err.contains("larger than announced"), "{err}");
        assert!(start.elapsed() < Duration::from_secs(4), "{:?}", start.elapsed());
        assert!(!dest.exists(), "the oversized file is not left behind");
        let pid: i32 = std::fs::read_to_string(tmp.path().join("f.tar.gz.pid")).unwrap().trim().parse().unwrap();
        assert!(!pid_alive(pid));
        // A file exactly as large as announced is fine.
        let curl = fake_curl(tmp.path(), "head -c 1000 /dev/zero > \"$out\"");
        let tools = Tools { curl: &curl, unsquashfs: "unsquashfs", ..TOOLS };
        download(&tools, "https://h/f", &dest, 1000, &Cancel::new(), &|_| {}).unwrap();
        assert_eq!(std::fs::metadata(&dest).unwrap().len(), 1000);
    }

    #[test]
    fn curl_exit_codes_map_to_messages() {
        let tmp = TempDir::new("exit-codes");
        let dest = tmp.path().join("f");
        let run = |code: i32| {
            let curl = fake_curl(tmp.path(), &format!("exit {code}"));
            let tools = Tools { curl: &curl, unsquashfs: "unsquashfs", ..TOOLS };
            download(&tools, "https://h/f", &dest, 1000, &Cancel::new(), &|_| {})
        };
        assert!(run(0).is_ok());
        assert!(run(33).is_ok(), "the server can't resume but the file is complete");
        assert!(run(63).unwrap_err().contains("larger than announced"), "curl's own --max-filesize");
        assert!(run(28).unwrap_err().contains("Download failed"), "timeout");
        assert!(run(22).unwrap_err().contains("isn't on the server"), "404: a release file that was never uploaded");
        assert!(run(1).unwrap_err().contains("isn't https"), "a redirect to plain http");
    }

    #[test]
    fn only_https_and_files_on_the_card_are_fetched() {
        assert!(scheme_allowed(TOOLS.protocols, "https://pocketui.xyz/updates/stable/update.json"));
        assert!(scheme_allowed(TOOLS.protocols, "HTTPS://pocketui.xyz/u.json"));
        assert!(scheme_allowed(TOOLS.protocols, "file:///storage/channel/update.json"));
        assert!(!scheme_allowed(TOOLS.protocols, "http://pocketui.xyz/updates/stable/update.json"));
        assert!(!scheme_allowed(TOOLS.protocols, "ftp://h/u.json"));
        assert!(!scheme_allowed(TOOLS.protocols, "pocketui.xyz/u.json"));
        assert!(DEFAULT_URL.starts_with("https://pocketui.xyz/updates/"), "the public channel: {DEFAULT_URL}");

        // Refused before curl runs: this curl would fail the test if it were started.
        let tmp = TempDir::new("https-only");
        let curl = fake_curl(tmp.path(), "touch \"$0.ran\"; exit 0");
        let tools = Tools { curl: &curl, unsquashfs: "unsquashfs", ..TOOLS };
        let key = SigningKey::from_bytes(&[3u8; 32]).verifying_key().to_bytes();
        assert!(fetch_manifest_with(&tools, "http://pocketui.xyz/u.json", &key).unwrap_err().contains("https://"));
        let rel = Release { version: "1".into(), url: "http://h/a.tar.gz".into(), sha256: "0".repeat(64), size: 1, notes: String::new() };
        let err = download_checked(&tools, &rel, "https://pocketui.xyz/u.json", &tmp.path().join("a"), &Cancel::new(), &|_| {}).unwrap_err();
        assert!(err.contains("isn't on https"), "{err}");
        assert!(!Path::new(&format!("{curl}.ran")).exists(), "curl was not started");
    }

    #[test]
    fn a_missing_list_is_not_called_a_wifi_problem() {
        let tmp = TempDir::new("fetch-codes");
        let key = SigningKey::from_bytes(&[3u8; 32]).verifying_key().to_bytes();
        let run = |code: i32| {
            let curl = fake_curl(tmp.path(), &format!("exit {code}"));
            let tools = Tools { curl: &curl, unsquashfs: "unsquashfs", ..TOOLS };
            fetch_manifest_with(&tools, DEFAULT_URL, &key).unwrap_err()
        };
        assert!(run(22).contains("no update list right now"), "404 before the first release is published");
        assert!(run(1).contains("isn't https"));
        assert!(run(6).contains("Check Wi-Fi"), "could not resolve the host");
    }

    /// A small HTTP server on 127.0.0.1 for the real curl, the way GitHub Releases serves a file: the release URL
    /// answers 302 with the download host's address, which serves the bytes (and a range of them, for a resume).
    /// Returns its base URL; the thread ends with the test process.
    fn serve(files: Vec<(&'static str, Vec<u8>)>, redirect_to: Option<&'static str>) -> String {
        use std::io::{BufRead, BufReader, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let cdn = redirect_to.map(|scheme| format!("{scheme}://{}", listener.local_addr().unwrap()));
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let (mut path, mut from) = (String::new(), 0usize);
                let mut line = String::new();
                while reader.read_line(&mut line).is_ok_and(|n| n > 0) && line != "\r\n" {
                    if let Some(p) = line.strip_prefix("GET ") {
                        path = p.split(' ').next().unwrap_or("").to_string();
                    }
                    if let Some(r) = line.to_ascii_lowercase().strip_prefix("range: bytes=") {
                        from = r.trim().trim_end_matches('-').parse().unwrap_or(0);
                    }
                    line.clear();
                }
                let reply = if let (Some(asset), Some(cdn)) = (path.strip_prefix("/releases/download/"), &cdn) {
                    format!("HTTP/1.1 302 Found\r\nLocation: {cdn}/cdn/{asset}?token=short-lived\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").into_bytes()
                } else {
                    // The download host keeps only the file name (GitHub's has an id there instead).
                    let path = path.split('?').next().unwrap_or("");
                    let name = path.strip_prefix("/cdn/").map_or(path.to_string(), |p| format!("/{}", p.rsplit('/').next().unwrap_or("")));
                    match files.iter().find(|(n, _)| *n == name.as_str()) {
                        Some((_, data)) if from > 0 && from < data.len() => {
                            let mut r = format!(
                                "HTTP/1.1 206 Partial Content\r\nContent-Range: bytes {from}-{}/{}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                                data.len() - 1,
                                data.len(),
                                data.len() - from
                            )
                            .into_bytes();
                            r.extend_from_slice(&data[from..]);
                            r
                        }
                        Some((_, data)) => {
                            let mut r = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", data.len()).into_bytes();
                            r.extend_from_slice(data);
                            r
                        }
                        None => b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec(),
                    }
                };
                let _ = stream.write_all(&reply);
            }
        });
        base
    }

    fn real_curl() -> bool {
        let ok = Command::new("curl").arg("--version").output().is_ok_and(|o| o.status.success());
        if !ok {
            eprintln!("no curl on this machine: skipped");
        }
        ok
    }

    #[test]
    fn real_curl_follows_the_release_redirect_resumes_and_checks_the_hash() {
        if !real_curl() {
            return;
        }
        let key = SigningKey::from_bytes(&[3u8; 32]);
        let app: Vec<u8> = (0..200_000u32).map(|i| (i * 7 % 251) as u8).collect();
        let base_placeholder = "BASE";
        let json = format!(
            r#"{{"format":1,"app":{{"version":"9.9.9","url":"{base_placeholder}/releases/download/v9.9.9/app.tar.gz","sha256":"{}","size":{}}}}}"#,
            store::sha256_hex(&app),
            app.len()
        );
        // The list names the release URL on the same server, so it needs the port first: a second server for it.
        let assets = serve(vec![("/app.tar.gz", app.clone()), ("/bad.tar.gz", vec![0u8; 200_000])], Some("http"));
        let json = json.replace(base_placeholder, &assets).into_bytes();
        let sig = base64::engine::general_purpose::STANDARD.encode(key.sign(&json).to_bytes()).into_bytes();
        let site = serve(vec![("/updates/stable/update.json", json), ("/updates/stable/update.json.sig", sig)], None);
        // The test server speaks plain http; everything else is the production setting.
        let tools = Tools { protocols: "=http,file", redirects: "=http", ..TOOLS };
        let url = format!("{site}/updates/stable/update.json");
        let m = fetch_manifest_with(&tools, &url, &key.verifying_key().to_bytes()).unwrap();
        let rel = m.app.unwrap();
        assert!(rel.url.contains("/releases/download/"), "absolute release URL: {}", rel.url);

        let tmp = TempDir::new("real-curl");
        let dest = tmp.path().join("app.tar.gz");
        download_checked(&tools, &rel, &url, &dest, &Cancel::new(), &|_| {}).unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), app, "redirect followed, bytes checked");

        // A partial file from an interrupted try is resumed with a range request through the redirect.
        std::fs::write(&dest, &app[..70_000]).unwrap();
        download_checked(&tools, &rel, &url, &dest, &Cancel::new(), &|_| {}).unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), app, "resumed and checked");

        // Other bytes behind the redirect: refused and deleted.
        let bad = Release { url: rel.url.replace("app.tar.gz", "bad.tar.gz"), ..rel.clone() };
        let _ = std::fs::remove_file(&dest);
        let err = download_checked(&tools, &bad, &url, &dest, &Cancel::new(), &|_| {}).unwrap_err();
        assert!(err.contains("damaged"), "{err}");
        assert!(!dest.exists());

        // An asset that was never uploaded: GitHub's 404 behind the release URL.
        let missing = Release { url: rel.url.replace("app.tar.gz", "gone.tar.gz"), ..rel.clone() };
        let err = download_checked(&tools, &missing, &url, &dest, &Cancel::new(), &|_| {}).unwrap_err();
        assert!(err.contains("isn't on the server"), "{err}");

        // With the production redirect rule, a redirect to plain http is not followed.
        let strict = Tools { protocols: "=http,file", ..TOOLS };
        let err = download_checked(&strict, &rel, &url, &dest, &Cancel::new(), &|_| {}).unwrap_err();
        assert!(err.contains("isn't https"), "{err}");
        assert!(!dest.exists() || std::fs::metadata(&dest).unwrap().len() == 0, "nothing of the file was fetched");
    }

    #[test]
    fn download_reports_the_bytes_so_far() {
        let tmp = TempDir::new("progress");
        let curl = fake_curl(tmp.path(), "head -c 700 /dev/zero > \"$out\"");
        let tools = Tools { curl: &curl, unsquashfs: "unsquashfs", ..TOOLS };
        let seen = std::sync::atomic::AtomicU64::new(0);
        download(&tools, "https://h/f", &tmp.path().join("f"), 1000, &Cancel::new(), &|n| seen.store(n, Ordering::SeqCst)).unwrap();
        assert_eq!(seen.load(Ordering::SeqCst), 700);
    }

    #[test]
    fn download_checked_resumes_a_short_partial_file_and_drops_an_oversized_one() {
        let tmp = TempDir::new("resume");
        let data = b"0123456789";
        let rel = Release { version: "1".into(), url: "f".into(), sha256: store::sha256_hex(data), size: 10, notes: String::new() };
        let dest = tmp.path().join("f");
        // curl that "resumes": appends what is missing after the first `have` bytes.
        let resume = "have=$(wc -c < \"$out\" 2>/dev/null || echo 0)\nhave=$((have + 0))\nprintf 0123456789 | tail -c +$((have + 1)) >> \"$out\"";
        let curl = fake_curl(tmp.path(), resume);
        let tools = Tools { curl: &curl, unsquashfs: "unsquashfs", ..TOOLS };
        std::fs::write(&dest, "0123").unwrap();
        download_checked(&tools, &rel, "file:///x/update.json", &dest, &Cancel::new(), &|_| {}).unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), data, "the 4 bytes already there were kept");
        // More bytes than the list says can't be a valid start: begin again.
        std::fs::write(&dest, "0123456789abcdef").unwrap();
        download_checked(&tools, &rel, "file:///x/update.json", &dest, &Cancel::new(), &|_| {}).unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), data);
    }

    #[test]
    fn download_checked_refuses_wrong_size_or_hash_and_removes_the_file() {
        let tmp = TempDir::new("damaged");
        let dest = tmp.path().join("f");
        let curl = fake_curl(tmp.path(), "printf 0123456789 > \"$out\"");
        let tools = Tools { curl: &curl, unsquashfs: "unsquashfs", ..TOOLS };
        let good = store::sha256_hex(b"0123456789");
        for (sha, size) in [(good.clone(), 9), (good.clone(), 11), ("0".repeat(64), 10)] {
            let rel = Release { version: "1".into(), url: "f".into(), sha256: sha, size, notes: String::new() };
            let err = download_checked(&tools, &rel, "file:///x/update.json", &dest, &Cancel::new(), &|_| {}).unwrap_err();
            // One byte too many may also be seen while curl still runs (under load), which `download` refuses itself.
            let too_large_while_running = size == 9 && err.contains("larger than announced");
            assert!(err.contains("damaged") || too_large_while_running, "{err}");
            assert!(!dest.exists());
        }
        // The hash in the list may be upper case.
        let rel = Release { version: "1".into(), url: "f".into(), sha256: good.to_uppercase(), size: 10, notes: String::new() };
        download_checked(&tools, &rel, "file:///x/update.json", &dest, &Cancel::new(), &|_| {}).unwrap();
    }

    #[test]
    fn a_stuck_update_stops_blocking_auto_sleep_after_the_cap() {
        let t = Instant::now();
        assert!(!busy_blocks_sleep(None, t), "nothing running");
        assert!(busy_blocks_sleep(Some(t), t));
        assert!(busy_blocks_sleep(Some(t), t + Duration::from_secs(29 * 60)));
        assert!(!busy_blocks_sleep(Some(t), t + BUSY_BLOCKS_SLEEP_FOR));
        assert!(!busy_blocks_sleep(Some(t), t + Duration::from_secs(3 * 3600)));
    }

    // -----------------------------------------------------------------------------------------
    // The OS update path. `unsquashfs` is replaced by a script that reads a tiny text "image"
    // (FILE <path> / content / END blocks), so everything around the real tool is exercised:
    // the tar handling, the checks on os-release, the bootloader guard and the staging.

    const FAKE_UNSQUASHFS: &str = r#"mode=$1; img=$2; path=$3
[ -f "$img" ] || exit 1
case "$mode" in
-cat)
  awk -v p="$path" '$1=="FILE"{on=($2==p); if(on)found=1; next} $1=="END"{on=0; next} on{print} END{exit (found?0:1)}' "$img" ;;
-lls)
  grep -q '^FAIL_LIST' "$img" && exit 1
  echo "drwxr-xr-x root/root 0 2026-10-01 00:00 squashfs-root"
  awk '$1=="FILE"{print "-rw-r--r-- root/root 1 2026-10-01 00:00 squashfs-root/" $2} $1=="LINK"{print "lrwxrwxrwx root/root 1 2026-10-01 00:00 squashfs-root/" $2 " -> " $3}' "$img" ;;
*) exit 2 ;;
esac"#;

    const CURRENT: &str = "OS_NAME=\"ROCKNIX\"\nOS_VERSION=\"20261001\"\nHW_DEVICE=\"SM8250\"\nHW_ARCH=\"aarch64\"\n";

    fn os_release(version: &str, device: &str, arch: &str) -> String {
        format!("OS_NAME=\"ROCKNIX\"\nOS_VERSION=\"{version}\"\nHW_DEVICE=\"{device}\"\nHW_ARCH=\"{arch}\"\n")
    }

    /// The text image with the given files.
    fn image(files: &[(&str, &str)]) -> String {
        files.iter().map(|(p, c)| format!("FILE {p}\n{c}\nEND\n")).collect()
    }

    /// A ROCKNIX-style update tar around `system`; `complete: false` leaves out KERNEL.md5.
    fn os_tar(dir: &Path, version: &str, system: &str, complete: bool) -> (Release, String) {
        let top = format!("ROCKNIX-SM8250.aarch64-{version}");
        let target = dir.join("os-src").join(&top).join("target");
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(target.join("SYSTEM"), system).unwrap();
        std::fs::write(target.join("KERNEL"), "kernel").unwrap();
        std::fs::write(target.join("SYSTEM.md5"), "x  SYSTEM\n").unwrap();
        if complete {
            std::fs::write(target.join("KERNEL.md5"), "x  KERNEL\n").unwrap();
        }
        let tar = dir.join(format!("{top}.tar"));
        assert!(Command::new("tar").arg("-cf").arg(&tar).arg("-C").arg(dir.join("os-src")).arg(&top).status().unwrap().success());
        let data = std::fs::read(&tar).unwrap();
        let rel = Release {
            version: version.into(),
            url: format!("{top}.tar"),
            sha256: store::sha256_hex(&data),
            size: data.len() as u64,
            notes: String::new(),
        };
        (rel, format!("file://{}/update.json", dir.display()))
    }

    fn good_system(version: &str) -> String {
        image(&[
            ("etc/os-release", &os_release(version, "SM8250", "aarch64")),
            ("usr/share/bootloader/update.sh", "# only boot files\nmount -o remount,rw $BOOT_ROOT\nsync\n"),
        ])
    }

    /// Runs `check_os_tar` on `system` with the fake unsquashfs, checking nothing of `os_compat` (`Needs::none`).
    fn check(tmp: &TempDir, version: &str, system: &str, complete: bool) -> Result<String, String> {
        let unsquashfs = script(tmp.path(), "unsquashfs", FAKE_UNSQUASHFS);
        let tools = Tools { curl: "curl", unsquashfs: &unsquashfs, ..TOOLS };
        let (rel, _) = os_tar(tmp.path(), version, system, complete);
        let work = tmp.path().join("work");
        let result = check_os_tar(&tools, &tmp.path().join(&rel.url), &work, Some(CURRENT), &Needs::none());
        assert!(!work.exists(), "the unpacked copy is removed again");
        let c = result?;
        match c.refusal() {
            Some(r) => Err(r),
            None => Ok(c.version),
        }
    }

    #[test]
    fn os_check_accepts_a_matching_system_and_reports_its_version() {
        let tmp = TempDir::new("os-ok");
        assert_eq!(check(&tmp, "20261101", &good_system("20261101"), true).unwrap(), "20261101");
    }

    #[test]
    fn os_check_reads_os_release_from_usr_lib_too() {
        let tmp = TempDir::new("os-usr-lib");
        let system = image(&[("usr/lib/os-release", &os_release("20261101", "SM8250", "aarch64"))]);
        assert_eq!(check(&tmp, "20261101", &system, true).unwrap(), "20261101");
    }

    #[test]
    fn os_check_refuses_an_incomplete_tar() {
        let tmp = TempDir::new("os-incomplete");
        let err = check(&tmp, "20261101", &good_system("20261101"), false).unwrap_err();
        assert!(err.contains("not a complete system update"), "{err}");
    }

    #[test]
    fn os_check_refuses_a_system_without_version_information() {
        let tmp = TempDir::new("os-noversion");
        let err = check(&tmp, "20261101", &image(&[("etc/hostname", "x")]), true).unwrap_err();
        assert!(err.contains("no version information"), "{err}");
    }

    #[test]
    fn os_check_refuses_another_device_or_architecture() {
        let tmp = TempDir::new("os-device");
        for (device, arch) in [("SM8550", "aarch64"), ("SM8250", "arm"), ("SM8550", "arm")] {
            let system = image(&[("etc/os-release", &os_release("20261101", device, arch))]);
            let err = check(&tmp, "20261101", &system, true).unwrap_err();
            assert!(err.contains("different device"), "{device} {arch}: {err}");
        }
    }

    #[test]
    fn os_check_refuses_a_system_that_ships_updateabl() {
        let tmp = TempDir::new("os-updateabl");
        let system = image(&[("etc/os-release", &os_release("20261101", "SM8250", "aarch64")), ("usr/bin/updateabl", "binary")]);
        let err = check(&tmp, "20261101", &system, true).unwrap_err();
        assert!(err.contains("rewrites the internal bootloader"), "{err}");
    }

    #[test]
    fn os_check_looks_for_the_exact_updateabl_path() {
        let tmp = TempDir::new("os-lookalike");
        let system = image(&[
            ("etc/os-release", &os_release("20261101", "SM8250", "aarch64")),
            ("usr/bin/updateabl.bak", "x"),
            ("opt/usr/bin/updateabl", "x"),
        ]);
        assert!(check(&tmp, "20261101", &system, true).is_ok());
    }

    #[test]
    fn os_check_refuses_bootloader_scripts_from_the_image() {
        let tmp = TempDir::new("os-scripts");
        let rocknix = "mount -o remount,rw $BOOT_ROOT\n$SYSTEM_ROOT/usr/bin/updateabl\nsync";
        let system = image(&[("etc/os-release", &os_release("20261101", "SM8250", "aarch64")), ("usr/share/bootloader/update.sh", rocknix)]);
        let err = check(&tmp, "20261101", &system, true).unwrap_err();
        assert!(err.contains("would touch the bootloader"), "{err}");
        let canupdate = "dd if=x of=/dev/mmcblk0p1";
        let system = image(&[("etc/os-release", &os_release("20261101", "SM8250", "aarch64")), ("usr/share/bootloader/canupdate.sh", canupdate)]);
        let err = check(&tmp, "20261101", &system, true).unwrap_err();
        assert!(err.contains("would touch the bootloader"), "{err}");
    }

    #[test]
    fn os_check_refuses_when_the_image_cannot_be_listed() {
        let tmp = TempDir::new("os-list-fails");
        let system = format!("{}FAIL_LIST\n", good_system("20261101"));
        let err = check(&tmp, "20261101", &system, true).unwrap_err();
        assert!(err.contains("can't be opened for checking"), "{err}");
    }

    #[test]
    fn tar_entries_refuses_unsafe_paths_and_unreadable_files() {
        let tmp = TempDir::new("tar-entries");
        let sub = tmp.path().join("sub");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(tmp.path().join("outside.txt"), "x").unwrap();
        std::fs::write(sub.join("inside.txt"), "x").unwrap();
        let tar_of = |name: &str, base: &Path, entry: &str| {
            let tar = tmp.path().join(name);
            assert!(Command::new("tar").arg("-cPf").arg(&tar).arg("-C").arg(base).arg(entry).status().unwrap().success());
            tar
        };
        let fine = tar_of("fine.tar", &sub, "inside.txt");
        assert_eq!(tar_entries(&fine).unwrap(), vec!["inside.txt".to_string()]);
        let parent = tar_of("parent.tar", &sub, "../outside.txt");
        assert!(tar_entries(&parent).unwrap_err().contains("unsafe paths"));
        let absolute = tar_of("absolute.tar", &sub, &sub.join("inside.txt").to_string_lossy());
        assert!(tar_entries(&absolute).unwrap_err().contains("unsafe paths"));
        std::fs::write(tmp.path().join("not.tar"), "this is not a tar file").unwrap();
        assert!(tar_entries(&tmp.path().join("not.tar")).unwrap_err().contains("can't be read"));
    }

    /// stage_os_in with the fake unsquashfs; returns (staging, update_dir) inside `tmp`.
    fn stage(tmp: &TempDir, rel: &Release, url: &str, current: &str) -> (PathBuf, PathBuf, Result<Option<String>, String>) {
        stage_with(tmp, rel, url, current, &Needs::none())
    }

    fn stage_with(tmp: &TempDir, rel: &Release, url: &str, current: &str, needs: &Needs) -> (PathBuf, PathBuf, Result<Option<String>, String>) {
        let unsquashfs = script(tmp.path(), "unsquashfs", FAKE_UNSQUASHFS);
        let tools = Tools { curl: "curl", unsquashfs: &unsquashfs, ..TOOLS };
        let (staging, update_dir) = (tmp.path().join("staging"), tmp.path().join("storage-update"));
        let result = stage_os_in(&tools, &staging, &update_dir, current, needs, rel, url, &Cancel::new(), &|_| {});
        (staging, update_dir, result)
    }

    /// Needs that ask for runemu.sh (refused without it) and nmcli (a note without it).
    fn some_needs() -> Needs {
        const FILES: &[os_compat::FileNeed] = &[
            os_compat::FileNeed { any_of: &["usr/bin/runemu.sh"], what: "no game would start", level: os_compat::Level::Blocks },
            os_compat::FileNeed { any_of: &["usr/bin/nmcli"], what: "Wi-Fi networks can't be joined", level: os_compat::Level::Warns },
        ];
        Needs { files: FILES, ..Needs::none() }
    }

    #[test]
    fn staging_refuses_a_system_pocket_ui_cannot_run_on_and_notes_what_it_loses() {
        let tmp = TempDir::new("stage-compat");
        let os = os_release("20261101", "SM8250", "aarch64");
        let without_runemu = image(&[("etc/os-release", &os), ("usr/bin/nmcli", "x")]);
        let (rel, url) = os_tar(tmp.path(), "20261101", &without_runemu, true);
        let (_, update_dir, result) = stage_with(&tmp, &rel, &url, CURRENT, &some_needs());
        let err = result.unwrap_err();
        assert_eq!(err, "This system update would break pocket-ui (/usr/bin/runemu.sh is missing: no game would start). It was not installed.");
        assert!(staged_os_in(&update_dir).is_none(), "never handed to the initramfs");
        let tmp = TempDir::new("stage-compat-note");
        let without_nmcli = format!("{}LINK usr/bin/runemu.sh runemu-real.sh\nFILE usr/bin/runemu-real.sh\nx\nEND\n", image(&[("etc/os-release", &os)]));
        let (rel, url) = os_tar(tmp.path(), "20261101", &without_nmcli, true);
        let (_, update_dir, result) = stage_with(&tmp, &rel, &url, CURRENT, &some_needs());
        assert_eq!(result.unwrap().as_deref(), Some("Note: /usr/bin/nmcli is missing: Wi-Fi networks can't be joined."));
        assert!(staged_os_in(&update_dir).is_some(), "staged, with the note; runemu.sh found through its link");
    }

    #[test]
    fn staging_an_os_update_replaces_the_waiting_one_and_cleans_up() {
        let tmp = TempDir::new("stage-ok");
        let (rel, url) = os_tar(tmp.path(), "20261101", &good_system("20261101"), true);
        let update_dir = tmp.path().join("storage-update");
        std::fs::create_dir_all(&update_dir).unwrap();
        std::fs::write(update_dir.join("ROCKNIX-old.tar"), "old").unwrap();
        std::fs::write(update_dir.join("notes.txt"), "keep").unwrap();
        let (staging, update_dir, result) = stage(&tmp, &rel, &url, CURRENT);
        assert_eq!(result.unwrap(), None, "nothing missing, no note");
        let name = rel.url.clone();
        assert_eq!(staged_os_in(&update_dir).unwrap(), update_dir.join(&name));
        assert!(!update_dir.join("ROCKNIX-old.tar").exists(), "only one update waits");
        assert!(update_dir.join("notes.txt").exists(), "other files are not touched");
        assert!(!staging.join(&name).exists() && !staging.join("check").exists(), "nothing stays in staging");
    }

    #[test]
    fn staging_refuses_a_version_that_differs_from_the_list() {
        let tmp = TempDir::new("stage-mismatch");
        let (mut rel, url) = os_tar(tmp.path(), "20261101", &good_system("20261101"), true);
        rel.version = "20261201".into();
        let update_dir = tmp.path().join("storage-update");
        std::fs::create_dir_all(&update_dir).unwrap();
        std::fs::write(update_dir.join("ROCKNIX-old.tar"), "old").unwrap();
        let (staging, update_dir, result) = stage(&tmp, &rel, &url, CURRENT);
        assert!(result.unwrap_err().contains("doesn't match its description"));
        assert!(update_dir.join("ROCKNIX-old.tar").exists(), "the waiting update stays");
        assert_eq!(std::fs::read_dir(&update_dir).unwrap().count(), 1);
        assert!(!staging.join(&rel.url).exists(), "the download is removed");
    }

    #[test]
    fn staging_keeps_the_waiting_update_when_the_new_one_fails_the_checks() {
        let tmp = TempDir::new("stage-check-fails");
        let system = image(&[("etc/os-release", &os_release("20261101", "SM8550", "aarch64"))]);
        let (rel, url) = os_tar(tmp.path(), "20261101", &system, true);
        let update_dir = tmp.path().join("storage-update");
        std::fs::create_dir_all(&update_dir).unwrap();
        std::fs::write(update_dir.join("ROCKNIX-old.tar"), "old").unwrap();
        let (staging, update_dir, result) = stage(&tmp, &rel, &url, CURRENT);
        assert!(result.unwrap_err().contains("different device"));
        assert!(update_dir.join("ROCKNIX-old.tar").exists());
        assert_eq!(std::fs::read_dir(&update_dir).unwrap().count(), 1, "the refused tar never reaches the initramfs");
        assert!(!staging.join(&rel.url).exists());
    }

    #[test]
    fn staging_refuses_a_damaged_download_and_odd_file_names() {
        let tmp = TempDir::new("stage-damaged");
        let (mut rel, url) = os_tar(tmp.path(), "20261101", &good_system("20261101"), true);
        rel.sha256 = "0".repeat(64);
        let (_, update_dir, result) = stage(&tmp, &rel, &url, CURRENT);
        assert!(result.unwrap_err().contains("damaged"));
        assert!(staged_os_in(&update_dir).is_none());
        for name in ["update.tar.gz", "a..b.tar", "update"] {
            let rel = Release { version: "1".into(), url: format!("https://h/{name}"), sha256: "0".repeat(64), size: 1, notes: String::new() };
            let (_, update_dir, result) = stage(&tmp, &rel, &url, CURRENT);
            assert!(result.unwrap_err().contains("unexpected file name"), "{name}");
            assert!(staged_os_in(&update_dir).is_none());
        }
    }

    #[test]
    fn staged_and_cancel_only_touch_tar_files() {
        let tmp = TempDir::new("staged");
        let dir = tmp.path().join("u");
        assert!(staged_os_in(&dir).is_none(), "no directory, nothing waiting");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("readme.txt"), "x").unwrap();
        assert!(staged_os_in(&dir).is_none());
        std::fs::write(dir.join("a.tar"), "x").unwrap();
        std::fs::write(dir.join("b.tar"), "x").unwrap();
        assert!(staged_os_in(&dir).is_some());
        cancel_os_in(&dir);
        assert!(staged_os_in(&dir).is_none());
        assert!(dir.join("readme.txt").exists());
    }

    #[test]
    fn the_squashfs_is_read_through_the_tool() {
        let tmp = TempDir::new("squashfs-helpers");
        let unsquashfs = script(tmp.path(), "unsquashfs", FAKE_UNSQUASHFS);
        let img = tmp.path().join("SYSTEM");
        std::fs::write(&img, format!("{}LINK etc/os-release ../usr/lib/os-release\n", image(&[("usr/lib/os-release", "OS_VERSION=\"1\""), ("usr/bin/updateabl", "x")]))).unwrap();
        let sys = os_compat::Squashfs::open(&unsquashfs, &img).unwrap();
        assert_eq!(read_text(&sys, "etc/os-release").as_deref().map(str::trim), Some("OS_VERSION=\"1\""), "through the link");
        assert_eq!(read_text(&sys, "etc/missing"), None);
        assert_eq!(sys.entry("usr/bin/updateabl"), Some(os_compat::Entry::File));
        assert_eq!(sys.entry("usr/bin/other"), None);
        assert!(os_compat::Squashfs::open(&unsquashfs, &tmp.path().join("nope")).is_err());
    }

    #[test]
    fn rollback_note_is_reported_once() {
        let tmp = TempDir::new("rollback-note");
        assert_eq!(take_rollback_note(tmp.path()), None);
        std::fs::write(tmp.path().join("rolled-back"), "0.2.1\n").unwrap();
        assert_eq!(take_rollback_note(tmp.path()).as_deref(), Some("0.2.1"));
        assert!(!tmp.path().join("rolled-back").exists());
        assert_eq!(take_rollback_note(tmp.path()), None);
    }

    #[test]
    fn ram_boot_is_recognised_by_nodtbo_on_the_command_line() {
        assert!(cmdline_has_nodtbo("quiet nodtbo boot=LABEL=x"));
        assert!(!cmdline_has_nodtbo("quiet boot=LABEL=x"));
        assert!(!cmdline_has_nodtbo("quiet nodtbo2 xnodtbo"), "whole words only");
        assert!(!cmdline_has_nodtbo(""));
        // The real thing reads /proc/cmdline; on a computer without one that is "not from RAM".
        let direct = std::fs::read_to_string("/proc/cmdline").is_ok_and(|c| cmdline_has_nodtbo(&c));
        assert_eq!(booted_from_ram(), direct);
    }

    #[test]
    fn os_version_is_read_from_etc_os_release() {
        let direct = std::fs::read_to_string("/etc/os-release").ok().and_then(|t| os_release_field(&t, "OS_VERSION"));
        assert_eq!(os_version(), direct);
    }

    #[test]
    fn stage_os_refuses_an_odd_file_name_before_touching_anything() {
        let rel = Release { version: "1".into(), url: "https://h/system.zip".into(), sha256: "0".repeat(64), size: 1, notes: String::new() };
        let err = stage_os(&rel, "https://h/update.json", &Cancel::new(), &|_| {}).unwrap_err();
        assert!(err.contains("unexpected file name"), "{err}");
    }

    #[test]
    fn read_small_stops_at_the_limit() {
        let tmp = TempDir::new("read-small");
        let file = tmp.path().join("f.txt");
        std::fs::write(&file, "0123456789").unwrap();
        assert_eq!(read_small(&file, 4).as_deref(), Some("0123"));
        assert_eq!(read_small(&file, 100).as_deref(), Some("0123456789"));
        assert_eq!(read_small(&tmp.path().join("missing"), 4), None);
    }

    #[test]
    fn ensure_layout_moves_a_flat_install_aside_once() {
        let tmp = TempDir::new("layout");
        let root = tmp.path();
        std::fs::write(root.join("pocket-ui"), "ui").unwrap();
        std::fs::write(root.join("pocket-core"), "core").unwrap();
        ensure_layout(root).unwrap();
        let dir = format!("releases/{}-local", app_version());
        assert_eq!(link_target(&root.join("current")).as_deref(), Some(dir.as_str()));
        assert_eq!(link_target(&root.join("pocket-ui")).as_deref(), Some("current/pocket-ui"));
        assert_eq!(std::fs::read_to_string(root.join(&dir).join("pocket-core")).unwrap(), "core");
        assert_eq!(std::fs::read_to_string(root.join("pocket-ui")).unwrap(), "ui", "through the link");
        // A second call changes nothing.
        ensure_layout(root).unwrap();
        assert_eq!(link_target(&root.join("current")).as_deref(), Some(dir.as_str()));
    }

    #[test]
    fn prune_keeps_current_previous_known_good_and_hidden_work_dirs() {
        let tmp = TempDir::new("prune");
        let root = tmp.path();
        for d in ["0.9.0", "1.0.0", "2.0.0", "3.0.0", ".staging-4.0.0"] {
            std::fs::create_dir_all(root.join("releases").join(d)).unwrap();
        }
        link("releases/3.0.0", &root.join("current")).unwrap();
        link("releases/2.0.0", &root.join("previous")).unwrap();
        link("releases/0.9.0", &root.join("known-good")).unwrap();
        prune(root);
        assert!(!root.join("releases/1.0.0").exists());
        assert!(root.join("releases/0.9.0").exists(), "the launcher's way back when a version keeps crashing");
        assert!(root.join("releases/2.0.0").exists() && root.join("releases/3.0.0").exists());
        assert!(root.join("releases/.staging-4.0.0").exists(), "a download in progress isn't ours to delete");
    }

    #[test]
    fn leftovers_of_other_versions_go_when_an_install_starts() {
        let tmp = TempDir::new("leftovers");
        let releases = tmp.path().join("releases");
        for d in [".staging-4.0.0", ".staging-5.0.0", "3.0.0"] {
            std::fs::create_dir_all(releases.join(d)).unwrap();
        }
        for f in [".download-4.0.0.tar.gz", ".download-5.0.0.tar.gz"] {
            std::fs::write(releases.join(f), b"partial").unwrap();
        }
        clear_leftovers(&releases, "5.0.0");
        assert!(!releases.join(".staging-4.0.0").exists() && !releases.join(".download-4.0.0.tar.gz").exists(), "an abandoned version");
        assert!(releases.join(".download-5.0.0.tar.gz").exists(), "the download being resumed stays");
        assert!(!releases.join(".staging-5.0.0").exists(), "a half unpacked folder is never reused");
        assert!(releases.join("3.0.0").exists(), "installed versions are prune's business");
    }

    #[test]
    fn a_release_that_fails_to_unpack_leaves_no_staging_folder() {
        let dir = TempDir::new("unpack-fail");
        let root = dir.path().join("root");
        std::fs::create_dir_all(&root).unwrap();
        let (mut rel, url) = fake_release(dir.path(), "9.3.0", true);
        let tar = dir.path().join("pocket-ui-9.3.0.tar.gz");
        std::fs::write(&tar, b"this is not a tar.gz").unwrap();
        let data = std::fs::read(&tar).unwrap();
        rel.sha256 = store::sha256_hex(&data);
        rel.size = data.len() as u64;
        assert!(install_app(&rel, &url, &root, &Cancel::new(), &|_| {}).is_err());
        let left: Vec<_> = std::fs::read_dir(root.join("releases")).unwrap().flatten().map(|e| e.file_name()).filter(|n| n.to_string_lossy().starts_with('.')).collect();
        assert!(left.is_empty(), "left behind: {left:?}");
    }

    #[test]
    fn app_version_looks_like_a_version() {
        assert!(app_version().chars().next().is_some_and(|c| c.is_ascii_digit()), "{}", app_version());
        assert_eq!(app_version(), option_env!("POCKETUI_VERSION").unwrap_or(env!("CARGO_PKG_VERSION")));
    }

    #[test]
    fn staged_os_looks_in_the_initramfs_directory() {
        assert_eq!(staged_os(), staged_os_in(Path::new(OS_UPDATE_DIR)));
    }

    #[test]
    fn manifest_is_fetched_with_its_signature_and_timeouts() {
        let tmp = TempDir::new("fetch");
        let key = SigningKey::from_bytes(&[3u8; 32]);
        let json = br#"{"format":1,"app":{"version":"0.2.0","url":"a.tar.gz","sha256":"00","size":1}}"#;
        let sig = base64::engine::general_purpose::STANDARD.encode(key.sign(json).to_bytes());
        std::fs::write(tmp.path().join("update.json"), json).unwrap();
        std::fs::write(tmp.path().join("update.json.sig"), &sig).unwrap();
        let log = tmp.path().join("args.log");
        // Serves the files by URL and logs what curl was asked.
        let body = format!(
            "for last; do :; done\necho \"$@\" >> {log}\ncase \"$last\" in *.sig) cat {dir}/update.json.sig ;; *) cat {dir}/update.json ;; esac",
            log = log.display(),
            dir = tmp.path().display()
        );
        let curl = script(tmp.path(), "curl", &body);
        let tools = Tools { curl: &curl, unsquashfs: "unsquashfs", ..TOOLS };
        let url = "https://h/stable/update.json";
        let m = fetch_manifest_with(&tools, url, &key.verifying_key().to_bytes()).unwrap();
        assert_eq!(m.app.unwrap().version, "0.2.0");
        let asked = std::fs::read_to_string(&log).unwrap();
        assert_eq!(asked.lines().count(), 2, "the list and its signature: {asked}");
        assert!(asked.lines().next().unwrap().ends_with(url) && asked.lines().nth(1).unwrap().ends_with("update.json.sig"), "{asked}");
        assert!(asked.lines().all(|l| l.contains("--max-time 60") && l.contains("--connect-timeout 15")), "{asked}");
        // The bytes come through unchanged.
        assert_eq!(fetch(&tools, url).unwrap(), json.to_vec());
        // Another key: refused. No server: "Couldn't reach".
        let other = SigningKey::from_bytes(&[4u8; 32]).verifying_key().to_bytes();
        assert!(fetch_manifest_with(&tools, url, &other).unwrap_err().contains("isn't signed by us"));
        let down = script(tmp.path(), "curl", "exit 6");
        let tools = Tools { curl: &down, unsquashfs: "unsquashfs", ..TOOLS };
        assert!(fetch_manifest_with(&tools, url, &key.verifying_key().to_bytes()).unwrap_err().contains("Couldn't reach"));
        assert!(fetch(&tools, url).is_err());
    }

    #[test]
    fn install_refuses_a_release_that_lacks_one_of_the_programs() {
        let tmp = TempDir::new("missing-bin");
        let root = tmp.path().join("root");
        std::fs::create_dir_all(&root).unwrap();
        for (n, missing) in ["pocket-core", "pocket-ui"].into_iter().enumerate() {
            let version = format!("1.0.{n}");
            let present = if missing == "pocket-core" { "pocket-ui" } else { "pocket-core" };
            let src = tmp.path().join(format!("src-{present}"));
            std::fs::create_dir_all(&src).unwrap();
            std::fs::write(src.join(present), "#!/bin/sh\necho x\n").unwrap();
            let tar = tmp.path().join(format!("pocket-ui-{version}.tar.gz"));
            assert!(Command::new("tar").arg("-czf").arg(&tar).arg("-C").arg(&src).arg(present).status().unwrap().success());
            let data = std::fs::read(&tar).unwrap();
            let url = format!("pocket-ui-{version}.tar.gz");
            let rel = Release { version: version.clone(), url, sha256: store::sha256_hex(&data), size: data.len() as u64, notes: String::new() };
            let manifest = format!("file://{}/update.json", tmp.path().display());
            let err = install_app(&rel, &manifest, &root, &Cancel::new(), &|_| {}).unwrap_err();
            assert!(err.contains(&format!("missing {missing}")), "{err}");
            assert!(!root.join("update-pending").exists() && !root.join(format!("releases/{version}")).exists());
        }
    }

    #[test]
    fn a_download_as_large_as_announced_may_still_be_running() {
        let tmp = TempDir::new("exact-size");
        // curl has written exactly the announced size and takes a moment to finish.
        let curl = fake_curl(tmp.path(), "head -c 1000 /dev/zero > \"$out\"\nsleep 1");
        let tools = Tools { curl: &curl, unsquashfs: "unsquashfs", ..TOOLS };
        let dest = tmp.path().join("f");
        download(&tools, "https://h/f", &dest, 1000, &Cancel::new(), &|_| {}).unwrap();
        assert_eq!(std::fs::metadata(&dest).unwrap().len(), 1000);
        // Without a known size (0) there is no limit to enforce.
        let curl = fake_curl(tmp.path(), "head -c 5000 /dev/zero > \"$out\"\nsleep 1");
        let tools = Tools { curl: &curl, unsquashfs: "unsquashfs", ..TOOLS };
        download(&tools, "https://h/f", &dest, 0, &Cancel::new(), &|_| {}).unwrap();
        assert_eq!(std::fs::metadata(&dest).unwrap().len(), 5000);
    }

    #[test]
    fn download_checked_decides_what_to_do_with_a_file_that_is_already_there() {
        let tmp = TempDir::new("stale");
        let data = b"0123456789";
        let rel = Release { version: "1".into(), url: "f".into(), sha256: store::sha256_hex(data), size: 10, notes: String::new() };
        let dest = tmp.path().join("f");
        let have_log = tmp.path().join("have");
        // Logs how many bytes were there when curl started; exits 33 like a server that can't resume.
        let body = format!("wc -c < \"$out\" 2>/dev/null | tr -d ' ' > {} || true\nexit 33", have_log.display());
        let curl = fake_curl(tmp.path(), &body);
        let tools = Tools { curl: &curl, unsquashfs: "unsquashfs", ..TOOLS };
        let run = |content: Option<&str>| {
            let _ = std::fs::remove_file(&dest);
            let _ = std::fs::remove_file(&have_log);
            if let Some(c) = content {
                std::fs::write(&dest, c).unwrap();
            }
            let r = download_checked(&tools, &rel, "file:///x/update.json", &dest, &Cancel::new(), &|_| {});
            (r, std::fs::read_to_string(&have_log).unwrap_or_default().trim().to_string())
        };
        // Already complete: kept, curl is asked to resume from 10.
        let (r, have) = run(Some("0123456789"));
        assert!(r.is_ok(), "{r:?}");
        assert_eq!(have, "10");
        // A partial file is kept for curl to continue.
        let (r, have) = run(Some("0123"));
        assert!(r.is_err(), "still short: {r:?}");
        assert_eq!(have, "4");
        // Too big to be a start of this file: curl begins from nothing.
        let (_, have) = run(Some("0123456789abc"));
        assert!(have.is_empty() || have == "0", "{have:?}");
        // Nothing there yet.
        let (_, have) = run(None);
        assert!(have.is_empty() || have == "0", "{have:?}");
    }
}
