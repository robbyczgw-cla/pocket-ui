//! Java for phone games (J2ME). ROCKNIX plays `.jar` games with RetroArch's FreeJ2ME-Plus core, which needs a Java
//! runtime that is not on the image. ROCKNIX's `/usr/bin/freej2me.sh` (run by `runemu.sh` before every FreeJ2ME start)
//! downloads it on the first start: 197 MB from Azul, unpacked into `/storage/jdk`, with only a black screen for about
//! 75 s and, without Wi-Fi, a dialog pocket-ui never shows.
//!
//! pocket-ui loads the same runtime itself first, on the launch's worker (`app_launch`), and says so over the start
//! with the bytes as they come ("Loading Java, once"); B stops it. It ends with the same folder the script makes, so the
//! script finds `/storage/jdk` filled and skips its own download (its test is "the folder is not empty"):
//! * the same file: [`URL`] is the script's `https://cdn.azul.com/zulu-embedded/bin/${JDKNAME}-linux_aarch64.tar.gz`
//!   with `JDKNAME="zulu11.48.21-ca-jdk11.0.11"` (ROCKNIX 20261001, `/usr/bin/freej2me.sh`);
//! * checked: the script checks nothing; [`SHA256`] and [`SIZE`] are Azul's own for that package
//!   (`api.azul.com/metadata/v1/zulu/packages/7a7d11e9-c2fb-4273-a505-33d8634746d1`, `sha256_hash`), and the file is
//!   thrown away when they don't match;
//! * the same folder: only `bin`, `lib` and `conf` (the script's three `tar` calls in one pass), `lib/*.zip` removed,
//!   moved into place only when complete, so a stopped or broken load never leaves a half folder the script would
//!   take for a finished one.
//!
//! Brick and Android have no FreeJ2ME route; [`needed`] is false wherever the ROCKNIX script is missing.
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
use std::time::{Duration, Instant};

use crate::store::DownloadFailure;

/// ROCKNIX's `JDKNAME` (the folder inside the archive is `{JDK_NAME}-linux_aarch64`).
pub const JDK_NAME: &str = "zulu11.48.21-ca-jdk11.0.11";
/// The file ROCKNIX's script downloads.
pub const URL: &str = "https://cdn.azul.com/zulu-embedded/bin/zulu11.48.21-ca-jdk11.0.11-linux_aarch64.tar.gz";
/// Azul's SHA-256 of [`URL`] (their package metadata), checked before anything is unpacked.
pub const SHA256: &str = "717866e68577e039d9f05511d845452c9225474c7352f3d23fad78fce6749276";
/// Its size in bytes.
pub const SIZE: u64 = 197_490_212;
/// `bin`, `lib` and `conf` as unpacked (lib/src.zip, 7 MB of it, is removed afterwards): for the space check.
const UNPACKED: u64 = 176_000_000;
/// Entries `tar` lists for `bin`, `lib` and `conf` (folders included): the unpack's progress.
const ENTRIES: u64 = 118;
/// Room on top of the download and the unpacked folder.
const SLACK: u64 = 20 << 20;
/// The longest a load may take before it is given up (curl's own speed limit ends a stalled one much sooner).
const AT_MOST: Duration = Duration::from_secs(30 * 60);

/// Which file is loaded: [`Source::azul`] on a handheld; tests pass a small archive of their own.
#[derive(Clone, Debug)]
pub struct Source {
    pub url: String,
    pub sha256: String,
    pub size: u64,
    /// The folder inside the archive.
    pub top: String,
    pub unpacked: u64,
    pub entries: u64,
}

impl Source {
    pub fn azul() -> Source {
        Source { url: URL.into(), sha256: SHA256.into(), size: SIZE, top: format!("{JDK_NAME}-linux_aarch64"), unpacked: UNPACKED, entries: ENTRIES }
    }
}

/// Where the runtime goes and how ROCKNIX is recognised.
#[derive(Clone, Debug)]
pub struct Places {
    /// `/storage`: the runtime is `storage/jdk`, as `runemu.sh` sets `JAVA_HOME`.
    pub storage: PathBuf,
    /// ROCKNIX's `/usr/bin/freej2me.sh`: where it is missing, nobody needs this runtime.
    pub script: PathBuf,
}

impl Places {
    pub fn device() -> Places {
        Places { storage: "/storage".into(), script: "/usr/bin/freej2me.sh".into() }
    }

    /// The handheld's places; under `POCKETUI_MOCK`, `POCKETUI_J2ME_ROOT` stands in for them (`root/storage`,
    /// `root/freej2me.sh`; tests/j2me_java.rs), and without it there is no ROCKNIX script.
    pub fn here() -> Places {
        if !crate::proc::mock() { return Places::device(); }
        let root = std::env::var_os("POCKETUI_J2ME_ROOT").map(PathBuf::from).unwrap_or_else(|| "/nonexistent-j2me".into());
        Places { storage: root.join("storage"), script: root.join("freej2me.sh") }
    }

    pub fn jdk(&self) -> PathBuf {
        self.storage.join("jdk")
    }

    /// The runtime is there: `java` in the folder FreeJ2ME starts it from.
    pub fn ready(&self) -> bool {
        self.jdk().join("bin/java").is_file()
    }

    fn download(&self) -> PathBuf {
        self.storage.join(".pocket-ui-jdk.download")
    }

    fn unpack(&self) -> PathBuf {
        self.storage.join(".pocket-ui-jdk.unpack")
    }
}

/// Whether this start must load Java first: a FreeJ2ME core on ROCKNIX, and no runtime yet.
pub fn needed(places: &Places, core: &str) -> bool {
    core.starts_with("freej2me") && places.script.is_file() && !places.ready()
}

/// What the load is doing, for the line over the start.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Starting,
    Downloading,
    Checking,
    Unpacking,
    Done,
}

/// The load's progress and its stop, shared between the worker and the screen.
#[derive(Debug, Default)]
pub struct Fetch {
    phase: AtomicU8,
    done: AtomicU64,
    total: AtomicU64,
    stop: AtomicBool,
}

impl Fetch {
    pub fn phase(&self) -> Phase {
        match self.phase.load(Ordering::Relaxed) {
            1 => Phase::Downloading,
            2 => Phase::Checking,
            3 => Phase::Unpacking,
            4 => Phase::Done,
            _ => Phase::Starting,
        }
    }

    fn set(&self, phase: Phase, done: u64, total: u64) {
        self.total.store(total, Ordering::Relaxed);
        self.done.store(done, Ordering::Relaxed);
        self.phase.store(phase as u8, Ordering::Relaxed);
    }

    fn step(&self, done: u64) {
        self.done.store(done, Ordering::Relaxed);
    }

    /// How far the current phase is, 0.0 to 1.0.
    pub fn fraction(&self) -> f32 {
        let total = self.total.load(Ordering::Relaxed);
        if total == 0 { return 0.0; }
        (self.done.load(Ordering::Relaxed) as f64 / total as f64).clamp(0.0, 1.0) as f32
    }

    /// B over the start: the load ends at its next look, and nothing of it stays.
    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }

    pub fn stopped(&self) -> bool {
        self.stop.load(Ordering::Relaxed)
    }

    /// The line under the title: "84 of 197 MB", then the check and the unpacking.
    pub fn line(&self) -> String {
        match self.phase() {
            Phase::Starting => "Connecting…".into(),
            Phase::Downloading => format!("{} of {} MB", self.done.load(Ordering::Relaxed) / 1_000_000, self.total.load(Ordering::Relaxed).div_ceil(1_000_000)),
            Phase::Checking => "Checking the download…".into(),
            Phase::Unpacking | Phase::Done => "Unpacking…".into(),
        }
    }
}

/// The title over the start while Java loads.
pub const TITLE: &str = "Loading Java, once (about 200 MB)";

/// Why Java was not loaded. `message` is the notice: what happened and what to do.
#[derive(Clone, Debug, PartialEq)]
pub enum Failure {
    Offline,
    NoSpace { free: u64, need: u64 },
    Broke(String),
    Damaged,
    Unpack(String),
    Stopped,
}

impl Failure {
    pub fn message(&self) -> String {
        match self {
            Failure::Offline => "Phone games need Java, loaded once (about 200 MB). Connect to Wi-Fi in Settings, then start the game again.".into(),
            Failure::NoSpace { free, need } => format!("Not enough space on the SD card for Java: {} MB free, {} MB needed.", free / 1_000_000, need.div_ceil(1_000_000)),
            Failure::Broke(m) => m.clone(),
            Failure::Damaged => "The Java download was damaged. Nothing was kept. Start the game again to try once more.".into(),
            Failure::Unpack(e) => format!("Couldn't unpack Java: {e}"),
            Failure::Stopped => "Java wasn't loaded. It loads the next time you start a phone game.".into(),
        }
    }
}

/// The download `install` calls: url, file, the most it may grow to, the shared progress (also its stop).
pub type Download<'a> = &'a dyn Fn(&str, &Path, u64, &Fetch) -> Result<(), DownloadFailure>;

/// A removal of something that is not there is no error.
trait FilterMissing {
    fn filter_missing(self) -> std::io::Result<()>;
}

impl FilterMissing for std::io::Result<()> {
    fn filter_missing(self) -> std::io::Result<()> {
        match self {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            other => other,
        }
    }
}

/// Removes the download and the half-unpacked folder however the load ends.
struct Leftovers(Vec<PathBuf>);

impl Drop for Leftovers {
    fn drop(&mut self) {
        for p in &self.0 {
            let gone = if p.is_dir() { std::fs::remove_dir_all(p) } else { std::fs::remove_file(p) };
            if let Err(e) = gone.filter_missing() { eprintln!("j2me: couldn't remove {}: {e}", p.display()); }
        }
    }
}

/// Loads, checks and unpacks Java into `places.jdk()`. `online`: the handheld has a network address (a quick "offline"
/// without waiting for curl); `free`: bytes free in `places.storage` (None: unknown, then it is tried).
pub fn install(places: &Places, source: &Source, online: bool, free: Option<u64>, fetch: &Fetch, download: Download) -> Result<(), Failure> {
    if places.ready() {
        fetch.set(Phase::Done, 1, 1);
        return Ok(());
    }
    if !online {
        return Err(Failure::Offline);
    }
    let need = source.size + source.unpacked + SLACK;
    if let Some(free) = free.filter(|f| *f < need) {
        return Err(Failure::NoSpace { free, need });
    }
    let (file, unpack) = (places.download(), places.unpack());
    let _leftovers = Leftovers(vec![file.clone(), unpack.clone()]);
    fetch.set(Phase::Starting, 0, source.size);
    download(&source.url, &file, source.size, fetch).map_err(|e| match e {
        _ if fetch.stopped() => Failure::Stopped,
        DownloadFailure::Unreachable => Failure::Offline,
        DownloadFailure::Broke(m) => Failure::Broke(m),
    })?;
    if fetch.stopped() { return Err(Failure::Stopped); }
    fetch.set(Phase::Checking, 0, 1);
    let got = std::fs::metadata(&file).map(|m| m.len()).unwrap_or(0);
    let hash = crate::store::sha256_file(&file).map_err(|e| Failure::Unpack(e.to_string()))?;
    if got != source.size || !hash.eq_ignore_ascii_case(&source.sha256) {
        return Err(Failure::Damaged);
    }
    if fetch.stopped() { return Err(Failure::Stopped); }
    fetch.set(Phase::Unpacking, 0, source.entries.max(1));
    unpack_jdk(&file, &unpack, &source.top, fetch)?;
    let inner = unpack.join(&source.top);
    if let Ok(zips) = std::fs::read_dir(inner.join("lib")) {
        for z in zips.flatten().filter(|e| e.path().extension().is_some_and(|x| x == "zip")) {
            std::fs::remove_file(z.path()).map_err(|e| Failure::Unpack(e.to_string()))?;
        }
    }
    if !inner.join("bin/java").is_file() {
        return Err(Failure::Unpack("java is missing in the download".into()));
    }
    let jdk = places.jdk();
    // ROCKNIX's script makes the folder empty when it was started offline; a folder without java is no runtime either.
    if jdk.exists() {
        std::fs::remove_dir_all(&jdk).map_err(|e| Failure::Unpack(e.to_string()))?;
    }
    std::fs::rename(&inner, &jdk).map_err(|e| Failure::Unpack(e.to_string()))?;
    fetch.set(Phase::Done, 1, 1);
    println!("j2me: Java ready in {}", jdk.display());
    Ok(())
}

/// `bin`, `lib` and `conf` of the archive into `into`, one pass (BusyBox tar on ROCKNIX), counting the names it lists.
fn unpack_jdk(file: &Path, into: &Path, top: &str, fetch: &Fetch) -> Result<(), Failure> {
    use std::io::BufRead;
    let broke = |e: std::io::Error| Failure::Unpack(e.to_string());
    std::fs::remove_dir_all(into).filter_missing().map_err(broke)?;
    std::fs::create_dir_all(into).map_err(broke)?;
    let mut child = Command::new("tar")
        .arg("-xzvf").arg(file).arg("-C").arg(into)
        .args(["bin", "lib", "conf"].map(|d| format!("{top}/{d}")))
        .stdout(Stdio::piped()).stderr(Stdio::null())
        .spawn().map_err(broke)?;
    let mut seen = 0;
    if let Some(out) = child.stdout.take() {
        for _ in std::io::BufReader::new(out).lines() {
            seen += 1;
            fetch.step(seen);
            if fetch.stopped() {
                let _ = child.kill();
                let _ = child.wait();
                return Err(Failure::Stopped);
            }
        }
    }
    let status = child.wait().map_err(broke)?;
    if !status.success() {
        return Err(Failure::Unpack(format!("tar ended with {status}")));
    }
    Ok(())
}

/// The load a start runs (`app_launch`): Azul's file with curl on a handheld. Under `POCKETUI_MOCK`,
/// `POCKETUI_J2ME_ARCHIVE` is a local archive "downloaded" in steps (`POCKETUI_J2ME_STEP_MS` each of ten), with
/// `POCKETUI_J2ME_TOP` its folder; `POCKETUI_J2ME_OFFLINE=1` has no network.
pub fn load(places: &Places, fetch: &Fetch) -> Result<(), Failure> {
    let free = crate::starter::free_bytes(&places.storage);
    let Some(file) = std::env::var_os("POCKETUI_J2ME_ARCHIVE").filter(|_| crate::proc::mock()).map(PathBuf::from) else {
        return install(places, &Source::azul(), crate::wifi::ip_now().is_some(), free, fetch, &curl);
    };
    let bytes = std::fs::read(&file).map_err(|e| Failure::Unpack(e.to_string()))?;
    let top = std::env::var("POCKETUI_J2ME_TOP").unwrap_or_default();
    let source = Source { url: file.display().to_string(), sha256: crate::store::sha256_hex(&bytes), size: bytes.len() as u64, top, unpacked: 0, entries: 1 };
    let step = Duration::from_millis(std::env::var("POCKETUI_J2ME_STEP_MS").ok().and_then(|s| s.parse().ok()).unwrap_or(0));
    let online = std::env::var_os("POCKETUI_J2ME_OFFLINE").is_none();
    let slow = |_: &str, dest: &Path, max: u64, fetch: &Fetch| {
        fetch.set(Phase::Downloading, 0, max);
        for k in 1..=10 {
            std::thread::sleep(step);
            fetch.step(max * k / 10);
            if fetch.stopped() { return Err(DownloadFailure::Broke("stopped".into())); }
        }
        std::fs::write(dest, &bytes).map_err(|e| DownloadFailure::Broke(e.to_string()))
    };
    install(places, &source, online, free, fetch, &slow)
}

/// The real download: curl with the Store's limits (`store::download_args`), its bytes into `fetch`, killed on B.
pub fn curl(url: &str, dest: &Path, max: u64, fetch: &Fetch) -> Result<(), DownloadFailure> {
    let broke = |e: std::io::Error| DownloadFailure::Broke(e.to_string());
    std::fs::remove_file(dest).filter_missing().map_err(broke)?;
    let mut child = Command::new("curl")
        .args(crate::store::download_args(url, dest, Some(max)))
        .stdout(Stdio::null()).stderr(Stdio::null())
        .spawn().map_err(broke)?;
    let end = Instant::now() + AT_MOST;
    let mut said = false;
    loop {
        let status = child.try_wait().map_err(broke)?;
        let got = std::fs::metadata(dest).map_or(0, |m| m.len());
        if got > 0 && !said {
            fetch.set(Phase::Downloading, got, max);
            said = true;
        }
        fetch.step(got);
        match status {
            Some(s) if s.success() => return Ok(()),
            Some(s) if crate::store::unreachable_code(s.code()) => return Err(DownloadFailure::Unreachable),
            Some(_) => return Err(DownloadFailure::Broke("The Java download didn't finish. Check Wi-Fi, then start the game again.".into())),
            None if fetch.stopped() || Instant::now() > end => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(DownloadFailure::Broke("The Java download took too long. Check Wi-Fi, then start the game again.".into()));
            }
            None => std::thread::sleep(Duration::from_millis(200)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOP: &str = "zulu-test-linux_aarch64";

    /// A small archive with the runtime's shape: bin/java, lib (with a src.zip), conf, and folders the script throws away.
    fn archive(dir: &Path) -> (PathBuf, Source) {
        let src = dir.join("src");
        for d in ["bin", "lib", "conf", "jmods", "man"] {
            std::fs::create_dir_all(src.join(TOP).join(d)).unwrap();
        }
        std::fs::write(src.join(TOP).join("bin/java"), b"#!/bin/sh\n").unwrap();
        std::fs::write(src.join(TOP).join("lib/libjvm.so"), b"jvm").unwrap();
        std::fs::write(src.join(TOP).join("lib/src.zip"), b"zip").unwrap();
        std::fs::write(src.join(TOP).join("conf/security"), b"c").unwrap();
        std::fs::write(src.join(TOP).join("jmods/java.base.jmod"), b"m").unwrap();
        let file = dir.join("jdk.tar.gz");
        assert!(Command::new("tar").arg("-czf").arg(&file).arg("-C").arg(&src).arg(TOP).status().unwrap().success());
        let bytes = std::fs::read(&file).unwrap();
        let source = Source { url: "https://example.invalid/jdk.tar.gz".into(), sha256: crate::store::sha256_hex(&bytes), size: bytes.len() as u64, top: TOP.into(), unpacked: 1000, entries: 7 };
        (file, source)
    }

    fn places(dir: &Path) -> Places {
        let storage = dir.join("storage");
        std::fs::create_dir_all(&storage).unwrap();
        let script = dir.join("freej2me.sh");
        std::fs::write(&script, b"#!/bin/bash\n").unwrap();
        Places { storage, script }
    }

    fn copy_from(file: PathBuf) -> impl Fn(&str, &Path, u64, &Fetch) -> Result<(), DownloadFailure> {
        move |_, dest, max, fetch| {
            fetch.set(Phase::Downloading, 0, max);
            std::fs::copy(&file, dest).map_err(|e| DownloadFailure::Broke(e.to_string()))?;
            fetch.step(max);
            Ok(())
        }
    }

    fn left_in(storage: &Path) -> Vec<String> {
        let mut v: Vec<String> = std::fs::read_dir(storage).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        v.sort();
        v
    }

    #[test]
    fn a_phone_game_without_java_loads_it_first_and_the_rocknix_script_then_finds_a_full_folder() {
        let dir = tempfile::tempdir().unwrap();
        let (file, source) = archive(dir.path());
        let places = places(dir.path());
        assert!(needed(&places, "freej2me"), "no runtime yet: the start must load it, not go black");
        assert!(!needed(&places, "mgba"), "other games start as before");
        let fetch = Fetch::default();
        install(&places, &source, true, None, &fetch, &copy_from(file)).unwrap();
        assert!(places.jdk().join("bin/java").is_file());
        assert!(places.jdk().join("conf/security").is_file());
        assert!(!places.jdk().join("lib/src.zip").exists(), "the script removes lib/*.zip too");
        assert!(!places.jdk().join("jmods").exists(), "only bin, lib and conf, as the script keeps");
        assert_eq!(left_in(&places.storage), ["jdk"], "the download and the unpack folder are gone");
        assert_eq!(fetch.phase(), Phase::Done);
        assert!(!needed(&places, "freej2me"), "the next start goes at once");
    }

    #[test]
    fn offline_says_so_at_once_and_leaves_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let (_, source) = archive(dir.path());
        let places = places(dir.path());
        let never = |_: &str, _: &Path, _: u64, _: &Fetch| -> Result<(), DownloadFailure> { panic!("no download without a network") };
        let e = install(&places, &source, false, None, &Fetch::default(), &never).unwrap_err();
        assert_eq!(e, Failure::Offline);
        assert!(e.message().contains("Wi-Fi") && e.message().contains("200 MB"), "{}", e.message());
        let unreachable = |_: &str, _: &Path, _: u64, _: &Fetch| Err(DownloadFailure::Unreachable);
        assert_eq!(install(&places, &source, true, None, &Fetch::default(), &unreachable), Err(Failure::Offline), "a network without internet is offline too");
        assert!(left_in(&places.storage).is_empty());
    }

    #[test]
    fn a_changed_download_is_thrown_away_and_an_empty_folder_left_by_the_script_is_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let (file, mut source) = archive(dir.path());
        let places = places(dir.path());
        // ROCKNIX's script started offline once: `mkdir -p /storage/jdk`, then "No Internet!".
        std::fs::create_dir_all(places.jdk()).unwrap();
        assert!(needed(&places, "freej2me"), "an empty folder is no runtime");
        let good = source.sha256.clone();
        source.sha256 = "0".repeat(64);
        assert_eq!(install(&places, &source, true, None, &Fetch::default(), &copy_from(file.clone())), Err(Failure::Damaged));
        assert_eq!(left_in(&places.storage), ["jdk"], "nothing of the damaged download stays");
        source.sha256 = good;
        install(&places, &source, true, None, &Fetch::default(), &copy_from(file)).unwrap();
        assert!(places.ready());
    }

    #[test]
    fn b_stops_the_load_and_too_little_room_is_said_before_it_starts() {
        let dir = tempfile::tempdir().unwrap();
        let (file, source) = archive(dir.path());
        let places = places(dir.path());
        let fetch = Fetch::default();
        let stop_midway = |u: &str, d: &Path, m: u64, f: &Fetch| { let r = copy_from(file.clone())(u, d, m, f); f.stop(); r };
        assert_eq!(install(&places, &source, true, None, &fetch, &stop_midway), Err(Failure::Stopped));
        assert!(left_in(&places.storage).is_empty(), "a stopped load leaves no half folder the script would take for Java");
        let e = install(&places, &source, true, Some(5_000_000), &Fetch::default(), &copy_from(file)).unwrap_err();
        assert!(matches!(e, Failure::NoSpace { free: 5_000_000, .. }), "{e:?}");
    }

    #[test]
    fn the_handheld_source_is_the_one_rocknix_downloads() {
        let s = Source::azul();
        assert_eq!(s.url, format!("https://cdn.azul.com/zulu-embedded/bin/{JDK_NAME}-linux_aarch64.tar.gz"));
        assert_eq!(s.top, "zulu11.48.21-ca-jdk11.0.11-linux_aarch64");
        assert_eq!(s.sha256.len(), 64);
    }

    #[test]
    fn without_the_rocknix_script_no_start_waits_for_java() {
        let dir = tempfile::tempdir().unwrap();
        let p = Places { storage: dir.path().into(), script: dir.path().join("missing.sh") };
        assert!(!needed(&p, "freej2me"));
    }
}
