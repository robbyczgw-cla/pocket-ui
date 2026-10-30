//! PortMaster ports, installed through PortMaster's own `harbourmaster` CLI (it checks md5s, fetches
//! runtimes and writes the launch script). Ready-to-run ports and ports that need the player's own game
//! files (`needs_files`; what to copy where is `port_files`).
//! Catalog: PortMaster-New `ports.json`, cached in the data directory for a day.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime};

use serde::Deserialize;

pub const PORTS_DIR: &str = "/storage/roms/ports";
const CATALOG_URL: &str = "https://github.com/PortsMaster/PortMaster-New/releases/latest/download/ports.json";
const PM_DIR: &str = "/storage/roms/ports/PortMaster";

#[derive(Clone, Debug, Default)]
pub struct Port {
    /// Catalog id, e.g. "2048.zip".
    pub name: String,
    pub title: String,
    pub desc: String,
    pub genres: Vec<String>,
    pub size: u64,
    /// First installed item, e.g. "2048.sh"; its presence means installed.
    pub launcher: String,
    /// The port's folder in roms/ports, e.g. "sprout" (its first item that is not a script).
    pub dir: String,
    /// Not ready to run: the player has to copy files from their own copy of the game.
    pub needs_files: bool,
    /// PortMaster's instructions for those files (`attr.inst`), as they are.
    pub instructions: String,
    /// The catalogue's checksum of this version (`source.md5`); an installed port with another one is out of date.
    pub md5: String,
}

#[derive(Deserialize)]
struct Catalog {
    ports: std::collections::HashMap<String, Entry>,
}

#[derive(Deserialize)]
struct Entry {
    name: String,
    items: Vec<String>,
    attr: Attr,
    source: Source,
}

#[derive(Deserialize)]
struct Attr {
    title: String,
    #[serde(default)]
    desc: String,
    #[serde(default)]
    genres: Vec<String>,
    #[serde(default)]
    rtr: bool,
    #[serde(default)]
    arch: Vec<String>,
    #[serde(default)]
    exp: bool,
    #[serde(default)]
    inst: Option<String>,
}

#[derive(Deserialize)]
struct Source {
    #[serde(default)]
    size: u64,
    #[serde(default)]
    md5: String,
}

/// Non-experimental aarch64 ports, ready to run or not, sorted by title.
pub fn parse_catalog(json: &[u8]) -> Vec<Port> {
    let Ok(catalog) = serde_json::from_slice::<Catalog>(json) else { return Vec::new() };
    let mut ports: Vec<Port> = catalog
        .ports
        .into_values()
        .filter(|e| !e.attr.exp && e.attr.arch.iter().any(|a| a == "aarch64"))
        .filter_map(|e| {
            let launcher = e.items.iter().find(|i| i.ends_with(".sh"))?.clone();
            let dir = port_dir(&e.items)?;
            Some(Port {
                name: e.name,
                title: e.attr.title,
                desc: e.attr.desc.split_whitespace().collect::<Vec<_>>().join(" "),
                genres: e.attr.genres,
                size: e.source.size,
                launcher,
                dir,
                needs_files: !e.attr.rtr,
                instructions: e.attr.inst.unwrap_or_default().replace("\r\n", "\n").trim().to_string(),
                md5: e.source.md5,
            })
        })
        .collect();
    ports.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));
    ports
}

/// Loads the catalog, downloading it again when the cached copy is older than a day.
pub fn catalog(data_dir: &Path) -> Result<Vec<Port>, String> {
    let cache = data_dir.join("ports.json");
    let fresh = std::fs::metadata(&cache)
        .and_then(|m| m.modified())
        .is_ok_and(|t| SystemTime::now().duration_since(t).unwrap_or_default() < Duration::from_secs(86_400));
    if !fresh {
        let _ = std::fs::create_dir_all(data_dir);
        let tmp = cache.with_extension("part");
        let ok = Command::new("curl")
            .args(["-fsSL", "--max-time", "120", "-o"])
            .arg(&tmp)
            .arg(CATALOG_URL)
            .status()
            .is_ok_and(|s| s.success());
        if ok {
            let _ = std::fs::rename(&tmp, &cache);
        }
    }
    let bytes = std::fs::read(&cache).map_err(|_| crate::net_check::refine("Couldn't load the PortMaster catalogue. Check Wi-Fi."))?;
    Ok(parse_catalog(&bytes))
}

/// The port's folder among its items: the first that is not a script ("sprout" of ["SPROUT.sh", "sprout"]).
fn port_dir(items: &[String]) -> Option<String> {
    items.iter().find(|i| !i.ends_with(".sh")).map(|i| i.trim_end_matches('/').to_string()).filter(|d| !d.is_empty() && !d.contains('/'))
}

/// What an installed port's own port.json says (PortMaster writes it into the port's folder).
#[derive(Clone, Debug, PartialEq)]
pub struct InstalledPort {
    pub title: String,
    pub dir: String,
    pub rtr: bool,
    pub instructions: String,
    /// When port.json was written: PortMaster writes it last when it installs.
    pub written: Option<SystemTime>,
}

#[derive(Deserialize)]
struct PortJson {
    #[serde(default)]
    items: Vec<String>,
    attr: PortJsonAttr,
}

#[derive(Deserialize)]
struct PortJsonAttr {
    #[serde(default)]
    title: String,
    // A port.json without it is an old, hand-made one: treat it as ready to run, never block it.
    #[serde(default = "yes")]
    rtr: bool,
    #[serde(default)]
    inst: Option<String>,
}

fn yes() -> bool {
    true
}

/// The installed port that `launcher` (/storage/roms/ports/SPROUT.sh) starts: the port.json in a folder of
/// `ports_dir` whose items name that script.
pub fn installed_port(ports_dir: &Path, launcher: &Path) -> Option<InstalledPort> {
    let script = launcher.file_name()?.to_str()?;
    for entry in std::fs::read_dir(ports_dir).ok()?.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.eq_ignore_ascii_case("PortMaster") || !entry.path().is_dir() {
            continue;
        }
        let json = entry.path().join("port.json");
        let Ok(bytes) = std::fs::read(&json) else { continue };
        let Ok(info) = serde_json::from_slice::<PortJson>(&bytes) else { continue };
        if !info.items.iter().any(|i| i == script) {
            continue;
        }
        return Some(InstalledPort {
            title: if info.attr.title.is_empty() { script.trim_end_matches(".sh").to_string() } else { info.attr.title },
            dir: name,
            rtr: info.attr.rtr,
            instructions: info.attr.inst.unwrap_or_default().trim().to_string(),
            written: std::fs::metadata(&json).and_then(|m| m.modified()).ok(),
        });
    }
    None
}

/// Every installed port that says it is not ready to run (`rtr: false`): the ports that need the player's files, for
/// the web companion's upload page (`web::files`). Sorted by title.
pub fn installed_needing_files(ports_dir: &Path) -> Vec<InstalledPort> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(ports_dir).into_iter().flatten().flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.eq_ignore_ascii_case("PortMaster") || !entry.path().is_dir() {
            continue;
        }
        let json = entry.path().join("port.json");
        let Ok(bytes) = std::fs::read(&json) else { continue };
        let Ok(info) = serde_json::from_slice::<PortJson>(&bytes) else { continue };
        if info.attr.rtr {
            continue;
        }
        out.push(InstalledPort {
            title: if info.attr.title.is_empty() { name.clone() } else { info.attr.title },
            dir: name,
            rtr: false,
            instructions: info.attr.inst.unwrap_or_default().trim().to_string(),
            written: std::fs::metadata(&json).and_then(|m| m.modified()).ok(),
        });
    }
    out.sort_by_key(|p| p.title.to_lowercase());
    out
}

pub fn installed(port: &Port) -> bool {
    Path::new(PORTS_DIR).join(&port.launcher).exists()
}

/// Screenshot cached by PortMaster (`config/images_pm`), or the one inside an installed port.
pub fn preview(port: &Port) -> Option<PathBuf> {
    let stem = port.name.trim_end_matches(".zip");
    cached_screenshots(stem).into_iter().chain(launcher_images(&port.launcher)).find(|p| p.exists())
}

/// PortMaster's own copies of a port's screenshot, `config/images_pm/<name>.screenshot.jpg|png`.
fn cached_screenshots(stem: &str) -> Vec<PathBuf> {
    let cached = Path::new(PM_DIR).join("config/images_pm");
    ["jpg", "png"].iter().map(|ext| cached.join(format!("{stem}.screenshot.{ext}"))).collect()
}

/// What an installed port's own `port.json` tells (one read of every port folder, see [`installed_index`]).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Installed {
    /// The catalogue name ("cceleste.zip").
    pub name: String,
    /// The version installed (`status.md5`); None for a port.json PortMaster did not write.
    pub md5: Option<String>,
    /// Pictures of the port, best first: its cover, its screenshot, PortMaster's cached copy of it.
    pub images: Vec<PathBuf>,
}

#[derive(Deserialize)]
struct IndexJson {
    #[serde(default)]
    name: String,
    #[serde(default)]
    items: Vec<String>,
    #[serde(default)]
    attr: IndexAttr,
    #[serde(default)]
    status: Option<IndexStatus>,
}

#[derive(Deserialize, Default)]
struct IndexAttr {
    #[serde(default)]
    image: Option<IndexImage>,
}

#[derive(Deserialize, Default)]
struct IndexImage {
    #[serde(default)]
    screenshot: Option<String>,
    #[serde(default)]
    covers: Vec<String>,
}

#[derive(Deserialize)]
struct IndexStatus {
    #[serde(default)]
    md5: Option<String>,
}

/// Every installed port by its launch script ("Animal Kart.sh"), from the port.json files in `ports_dir`'s folders. A
/// launcher's folder is not always named like it (`Animal Kart.sh` starts `animalkart/`), so the shelf's art is found
/// through the port.json, never by the script's name.
pub fn scan_installed(ports_dir: &Path) -> std::collections::HashMap<String, Installed> {
    let mut out = std::collections::HashMap::new();
    for entry in std::fs::read_dir(ports_dir).into_iter().flatten().flatten() {
        let dir = entry.path();
        if entry.file_name().to_string_lossy().eq_ignore_ascii_case("PortMaster") || !dir.is_dir() {
            continue;
        }
        let Ok(bytes) = std::fs::read(dir.join("port.json")) else { continue };
        let Ok(info) = serde_json::from_slice::<IndexJson>(&bytes) else { continue };
        let image = info.attr.image.unwrap_or_default();
        let mut images: Vec<PathBuf> = image.covers.iter().map(|f| dir.join(f)).collect();
        images.extend(image.screenshot.iter().map(|f| dir.join(f)));
        images.extend(["screenshot.jpg", "screenshot.png"].iter().map(|f| dir.join(f)));
        images.extend(cached_screenshots(info.name.trim_end_matches(".zip")));
        let md5 = info.status.and_then(|s| s.md5).filter(|m| !m.is_empty());
        for script in info.items.iter().filter(|i| i.ends_with(".sh")) {
            out.insert(script.clone(), Installed { name: info.name.clone(), md5: md5.clone(), images: images.clone() });
        }
    }
    out
}

type Index = std::sync::Arc<std::collections::HashMap<String, Installed>>;

#[derive(Clone, Default)]
struct IndexSnapshot {
    index: Index,
    revision: u64,
}

#[derive(Default)]
struct IndexState {
    published: std::sync::Mutex<IndexSnapshot>,
    invalidated: std::sync::atomic::AtomicU64,
}

struct IndexCache {
    state: std::sync::Arc<IndexState>,
    wake: std::sync::mpsc::SyncSender<()>,
}

impl IndexCache {
    /// Both the directory stat and the full scan belong to this worker. It holds the publication lock only to
    /// exchange finished snapshots; dropping the old map also happens outside that lock.
    fn start(
        mut stamp: impl FnMut() -> Option<SystemTime> + Send + 'static,
        mut scan: impl FnMut() -> std::collections::HashMap<String, Installed> + Send + 'static,
        mut notify: impl FnMut() + Send + 'static,
    ) -> Self {
        use std::sync::atomic::Ordering;
        let state = std::sync::Arc::new(IndexState::default());
        let worker = state.clone();
        let (wake, rx) = std::sync::mpsc::sync_channel(1);
        std::thread::spawn(move || {
            let mut seen = None;
            let mut revision = 0;
            loop {
                let generation = worker.invalidated.load(Ordering::Acquire);
                let before = stamp();
                if seen != Some((generation, before)) {
                    let index = std::sync::Arc::new(scan());
                    // An install or directory change during the scan makes that pass obsolete. Keep the last
                    // finished snapshot visible and immediately scan again, even if the wake channel was full.
                    if generation != worker.invalidated.load(Ordering::Acquire) || before != stamp() {
                        continue;
                    }
                    revision += 1;
                    let old = {
                        let mut published = worker.published.lock().unwrap_or_else(|e| e.into_inner());
                        std::mem::replace(&mut *published, IndexSnapshot { index, revision })
                    };
                    drop(old);
                    seen = Some((generation, before));
                    notify();
                }
                match rx.recv_timeout(Duration::from_secs(1)) {
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                    Ok(()) | Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                }
            }
        });
        Self { state, wake }
    }

    fn read(&self, last: &mut IndexSnapshot) -> IndexSnapshot {
        match self.state.published.try_lock() {
            Ok(published) => *last = published.clone(),
            Err(std::sync::TryLockError::Poisoned(e)) => *last = e.into_inner().clone(),
            Err(std::sync::TryLockError::WouldBlock) => {}
        }
        last.clone()
    }

    fn invalidate(&self) {
        self.state.invalidated.fetch_add(1, std::sync::atomic::Ordering::Release);
        let _ = self.wake.try_send(());
    }
}

static INDEX: std::sync::OnceLock<IndexCache> = std::sync::OnceLock::new();
static INDEX_NOTIFY: std::sync::OnceLock<std::sync::Arc<dyn Fn() + Send + Sync>> = std::sync::OnceLock::new();

thread_local! {
    // Even the brief publication exchange must not hold up a paint: keep this thread's last finished Arc.
    static LAST_INDEX: std::cell::RefCell<IndexSnapshot> = std::cell::RefCell::new(IndexSnapshot::default());
}

fn index_cache() -> &'static IndexCache {
    INDEX.get_or_init(|| IndexCache::start(
        || std::fs::metadata(PORTS_DIR).and_then(|m| m.modified()).ok(),
        || scan_installed(Path::new(PORTS_DIR)),
        || { if let Some(notify) = INDEX_NOTIFY.get() { notify(); } },
    ))
}

/// Register the event-loop wake before starting the index worker. Called once during UI startup; each completed
/// refresh asks the UI to discard its port-art caches and repaint. The callback runs without any index lock.
pub fn start_index(notify: impl Fn() + Send + Sync + 'static) {
    let _ = INDEX_NOTIFY.set(std::sync::Arc::new(notify));
    let _ = index_cache();
    // A caller may have looked up port art before startup registered its callback.
    if let Some(notify) = INDEX_NOTIFY.get() { notify(); }
}

fn index_snapshot() -> IndexSnapshot {
    let cache = index_cache();
    LAST_INDEX.with(|last| cache.read(&mut last.borrow_mut()))
}

fn installed_index() -> Index {
    index_snapshot().index
}

/// The last finished index's revision, zero until the first scan finishes. Art caches can discard results from an
/// earlier revision; reading this never waits for a scan or touches the card.
pub fn installed_revision() -> u64 {
    index_snapshot().revision
}

/// Ask the worker to refresh after an install, update or removal. Keep the previous snapshot while it reads files
/// inside the ports' folders (these changes do not always touch the parent directory's modification time).
pub fn forget() {
    if let Some(cache) = INDEX.get() {
        cache.invalidate();
    }
}

/// The installed port that `launcher` ("Animal Kart.sh") starts.
pub fn installed_of(launcher: &str) -> Option<Installed> {
    installed_index().get(launcher).cloned()
}

/// Pictures of a port for the shelf's box and spine colour, best first (its cover, its screenshot), by the port.json of the
/// installed port. Without one (a hand-made port), `<launcher>/screenshot.jpg` as before.
pub fn launcher_images(launcher: &str) -> Vec<PathBuf> {
    let mut out = installed_index().get(launcher).map(|i| i.images.clone()).unwrap_or_default();
    let dir = Path::new(PORTS_DIR).join(launcher.trim_end_matches(".sh"));
    out.extend(["screenshot.jpg", "screenshot.png", "cover.png", "cover.jpg"].iter().map(|f| dir.join(f)));
    out
}

/// The catalogue's checksums by port name, for the shelf's cards and the update notes: read from the cached
/// `ports.json` on a thread at the start ([`load_versions`]) and refreshed when the browser loads the catalogue.
static VERSIONS: std::sync::Mutex<Option<std::collections::HashMap<String, String>>> = std::sync::Mutex::new(None);

/// Remembers the catalogue's versions (md5 by port name).
pub fn note_versions(ports: &[Port]) {
    let map = ports.iter().filter(|p| !p.md5.is_empty()).map(|p| (p.name.clone(), p.md5.clone())).collect();
    *VERSIONS.lock().unwrap_or_else(|e| e.into_inner()) = Some(map);
}

/// Reads the cached catalogue (never downloads) on a thread of its own, once pocket-ui is up.
pub fn load_versions(data_dir: &Path) {
    let cache = data_dir.join("ports.json");
    std::thread::spawn(move || {
        if let Ok(bytes) = std::fs::read(cache) {
            note_versions(&parse_catalog(&bytes));
        }
    });
}

/// Whether the catalogue has a newer version than the one installed: both checksums known and different. An unknown one
/// (a catalogue not read yet, a port.json PortMaster did not write) is never an update: the note is quiet and sure.
pub fn newer(installed_md5: Option<&str>, catalogue_md5: Option<&str>) -> bool {
    matches!((installed_md5, catalogue_md5), (Some(a), Some(b)) if !a.is_empty() && !b.is_empty() && a != b)
}

/// An update is waiting for the installed port that `launcher` starts.
pub fn update_waiting(launcher: &str) -> bool {
    let Some(inst) = installed_of(launcher) else { return false };
    let versions = VERSIONS.lock().unwrap_or_else(|e| e.into_inner());
    newer(inst.md5.as_deref(), versions.as_ref().and_then(|v| v.get(&inst.name)).map(String::as_str))
}

/// An update is waiting for `port` (the catalogue's entry) next to what is installed.
pub fn update_available(port: &Port) -> bool {
    installed_of(&port.launcher).is_some_and(|i| i.name == port.name && newer(i.md5.as_deref(), Some(&port.md5)))
}

/// PortMaster needs its setup when it isn't unpacked yet, or when its helpers can't run. PortMaster's own
/// GUI makes everything in its folder executable each time it opens (`chmod -R +x .` in PortMaster.sh);
/// pocket-ui never opens that GUI, so without this a port starts but `gptokeyb` doesn't, and every port that
/// reads the keyboard (Celeste Classic, 2048, ...) gets no controls.
fn needs_setup(pm_dir: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    let runnable = |name: &str| std::fs::metadata(pm_dir.join(name)).is_ok_and(|m| m.permissions().mode() & 0o111 != 0);
    !pm_dir.join("harbourmaster").exists() || !runnable("gptokeyb")
}

/// Unpacks PortMaster or repairs it when needed (see `needs_setup`); cheap when nothing is to do.
/// Called before PortMaster installs and before a port starts.
pub fn ensure_ready() -> Result<(), String> {
    if needs_setup(Path::new(PM_DIR)) {
        // This mirrors ROCKNIX's start_portmaster.sh without the GUI.
        let ok = Command::new("sh").arg("/storage/pocket-ui/portmaster-setup.sh").status().is_ok_and(|s| s.success());
        if !ok {
            return Err("Couldn't prepare PortMaster.".into());
        }
    }
    Ok(())
}

fn harbourmaster(args: &[&str]) -> Result<(), String> {
    ensure_ready()?;
    let out = Command::new("python3")
        .arg("harbourmaster")
        .args(["--quiet", "--no-check", "--no-colour"])
        .args(args)
        .current_dir(PM_DIR)
        .env("HOME", "/storage")
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        let text = String::from_utf8_lossy(&out.stdout);
        Err(text.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("PortMaster reported an error").trim().to_string())
    }
}

pub fn install(port: &Port) -> Result<(), String> {
    forget();
    let result = harbourmaster(&["install", &port.name]);
    forget();
    result?;
    if installed(port) { Ok(()) } else { Err("PortMaster finished, but the game is missing.".into()) }
}

pub fn uninstall(port: &Port) -> Result<(), String> {
    let result = harbourmaster(&uninstall_args(port));
    forget();
    result
}

/// harbourmaster knows installed ports by their full catalog name ("cceleste.zip", from the port's
/// port.json); without ".zip" it answers "Unknown port".
fn uninstall_args(port: &Port) -> [&str; 2] {
    ["uninstall", &port.name]
}

/// "83 KB", "12 MB", "1.4 GB".
pub fn size_label(bytes: u64) -> String {
    match bytes {
        0..=1_048_575 => format!("{} KB", (bytes / 1024).max(1)),
        1_048_576..=1_073_741_823 => format!("{} MB", bytes / 1_048_576),
        _ => format!("{:.1} GB", bytes as f64 / 1_073_741_824.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;

    #[test]
    fn a_blocked_port_scan_keeps_the_last_snapshot_and_repeats_after_invalidation() {
        use std::sync::{mpsc, Arc};
        let (blocked, scanning) = mpsc::channel();
        let (release, released) = mpsc::channel();
        let (published, publications) = mpsc::channel();
        let mut pass = 0;
        let cache = Arc::new(IndexCache::start(
            || None,
            move || {
                pass += 1;
                if pass == 2 {
                    blocked.send(()).unwrap();
                    let _ = released.recv();
                }
                [("Game.sh".to_string(), Installed { md5: Some(pass.to_string()), ..Installed::default() })].into_iter().collect()
            },
            move || { let _ = published.send(()); },
        ));
        publications.recv_timeout(Duration::from_secs(5)).unwrap();
        let mut last = IndexSnapshot::default();
        assert_eq!(cache.read(&mut last).index["Game.sh"].md5.as_deref(), Some("1"));
        cache.invalidate();
        scanning.recv_timeout(Duration::from_secs(5)).unwrap();

        // A UI reader must finish while the scan is blocked, even during the brief publication exchange.
        let held = cache.state.published.lock().unwrap();
        let reader = cache.clone();
        let (read, result) = mpsc::channel();
        let thread = std::thread::spawn(move || { let _ = read.send(reader.read(&mut last)); });
        let snapshot = result.recv_timeout(Duration::from_secs(2)).expect("index reads must never wait for the scan or publisher");
        assert_eq!(snapshot.index["Game.sh"].md5.as_deref(), Some("1"));
        drop(held);
        thread.join().unwrap();

        // Updating a port while an older pass reads it must cause another pass even when the root stamp stays the same.
        cache.invalidate();
        release.send(()).unwrap();
        publications.recv_timeout(Duration::from_secs(5)).unwrap();
        let mut last = snapshot;
        let refreshed = cache.read(&mut last);
        assert_eq!(refreshed.index["Game.sh"].md5.as_deref(), Some("3"));
        assert_eq!(refreshed.revision, 2, "the obsolete second scan was never published");
    }

    #[test]
    fn directory_changes_refresh_the_port_index_without_an_install() {
        use std::sync::{atomic::{AtomicU64, Ordering}, mpsc, Arc};
        let directory_version = Arc::new(AtomicU64::new(0));
        let stamp = directory_version.clone();
        let (published, publications) = mpsc::channel();
        let mut pass = 0;
        let cache = IndexCache::start(
            move || Some(SystemTime::UNIX_EPOCH + Duration::from_secs(stamp.load(Ordering::Relaxed))),
            move || {
                pass += 1;
                [("Game.sh".to_string(), Installed { md5: Some(pass.to_string()), ..Installed::default() })].into_iter().collect()
            },
            move || { let _ = published.send(()); },
        );
        publications.recv_timeout(Duration::from_secs(5)).unwrap();
        directory_version.store(1, Ordering::Relaxed);
        publications.recv_timeout(Duration::from_secs(5)).expect("the worker must notice directory changes without forget");
        assert_eq!(cache.read(&mut IndexSnapshot::default()).index["Game.sh"].md5.as_deref(), Some("2"));
    }

    #[test]
    fn uninstall_names_the_port_as_harbourmaster_knows_it() {
        let port = Port { name: "cceleste.zip".into(), ..Port::default() };
        assert_eq!(uninstall_args(&port), ["uninstall", "cceleste.zip"]);
    }

    #[test]
    fn setup_is_needed_until_portmaster_is_unpacked_and_its_helpers_can_run() {
        use std::os::unix::fs::PermissionsExt;
        let dir = TestDir::new("pmsetup");
        assert!(needs_setup(&dir), "nothing unpacked");
        std::fs::write(dir.join("harbourmaster"), "#!/usr/bin/env python3").unwrap();
        std::fs::write(dir.join("gptokeyb"), "ELF").unwrap();
        std::fs::set_permissions(dir.join("gptokeyb"), std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(needs_setup(&dir), "gptokeyb as unpacked from PortMaster.zip: not executable");
        std::fs::set_permissions(dir.join("gptokeyb"), std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(!needs_setup(&dir), "unpacked and runnable");
    }

    #[test]
    fn keeps_arm64_ports_and_marks_the_ones_that_need_game_files() {
        let json = br#"{"ports":{
            "2048.zip":{"name":"2048.zip","items":["2048.sh","2048/"],"attr":{"title":"2048","desc":"The 2048\n puzzle","genres":["puzzle"],"rtr":true,"arch":["aarch64"],"exp":false},"source":{"size":83788}},
            "sprout.zip":{"name":"sprout.zip","items":["SPROUT.sh","sprout"],"attr":{"title":"-SPROUT-","desc":"","inst":"Purchase the game from Steam. Download the game and place the game files in sprout/assets.","inst_md":null,"rtr":false,"arch":["aarch64"]},"source":{"size":8159843}},
            "bermuda_syndrome.zip":{"name":"bermuda_syndrome.zip","items":["bermuda/","Bermuda Syndrome.sh"],"attr":{"title":"Bermuda Syndrome","inst":null,"rtr":false,"arch":["aarch64"]},"source":{"size":1}},
            "beta.zip":{"name":"beta.zip","items":["Beta.sh","beta"],"attr":{"title":"Beta","rtr":false,"exp":true,"arch":["aarch64"]},"source":{"size":1}},
            "x86.zip":{"name":"x86.zip","items":["X.sh"],"attr":{"title":"X86","rtr":true,"arch":["x86_64"]},"source":{"size":1}}
        },"utils":{}}"#;
        let ports = parse_catalog(json);
        let names: Vec<&str> = ports.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["sprout.zip", "2048.zip", "bermuda_syndrome.zip"], "by title; experimental and x86 left out");
        assert_eq!(ports[1].launcher, "2048.sh");
        assert_eq!(ports[1].dir, "2048");
        assert_eq!(ports[1].desc, "The 2048 puzzle");
        assert!(!ports[1].needs_files);
        assert!(ports[0].needs_files);
        assert_eq!(ports[0].dir, "sprout");
        assert_eq!(ports[0].instructions, "Purchase the game from Steam. Download the game and place the game files in sprout/assets.");
        assert_eq!((ports[2].dir.as_str(), ports[2].launcher.as_str(), ports[2].instructions.as_str()), ("bermuda", "Bermuda Syndrome.sh", ""));
        assert_eq!(size_label(83_788), "81 KB");
    }

    /// The real catalog, when one has been downloaded (`POCKETUI_PORTS_JSON=tmp/ports.json cargo test`):
    /// it still parses, and about half of the arm64 ports need the player's files.
    #[test]
    fn the_real_catalog_parses_when_given() {
        let Some(path) = std::env::var_os("POCKETUI_PORTS_JSON") else { return };
        let ports = parse_catalog(&std::fs::read(path).unwrap());
        let own = ports.iter().filter(|p| p.needs_files).count();
        assert!(ports.len() > 900 && own > 400, "{} ports, {own} need files", ports.len());
    }

    fn write_port(dir: &Path, folder: &str, json: &str) {
        std::fs::create_dir_all(dir.join(folder)).unwrap();
        std::fs::write(dir.join(folder).join("port.json"), json).unwrap();
    }

    #[test]
    fn art_is_found_through_the_port_json_not_the_launchers_name() {
        let dir = TestDir::new("art");
        // "Animal Kart.sh" starts animalkart/: the folder is not named like the script.
        write_port(
            &dir,
            "animalkart",
            r#"{"name":"animalkart.zip","items":["Animal Kart.sh","animalkart"],"attr":{"title":"Animal Kart","image":{"screenshot":"screenshot.jpg","covers":["cover.png"]}},"status":{"md5":"aaa"}}"#,
        );
        write_port(&dir, "old", r#"{"name":"old.zip","items":["Old.sh","old"],"attr":{"title":"Old"}}"#);
        let index = scan_installed(&dir);
        let kart = &index["Animal Kart.sh"];
        assert_eq!(kart.name, "animalkart.zip");
        assert_eq!(kart.md5.as_deref(), Some("aaa"));
        assert_eq!(kart.images[0], dir.join("animalkart/cover.png"), "the cover first: it is the box");
        assert_eq!(kart.images[1], dir.join("animalkart/screenshot.jpg"), "then the screenshot");
        assert!(kart.images.contains(&PathBuf::from(PM_DIR).join("config/images_pm/animalkart.screenshot.jpg")), "PortMaster's cached copy last");
        // A port.json without the image part or the status still gives a fallback list and no version.
        let old = &index["Old.sh"];
        assert_eq!(old.md5, None);
        assert!(old.images.contains(&dir.join("old/screenshot.png")));
        // Not a port: PortMaster itself, and a folder without port.json.
        write_port(&dir, "PortMaster", r#"{"items":["Nope.sh"],"attr":{}}"#);
        std::fs::create_dir_all(dir.join("plain")).unwrap();
        assert!(!scan_installed(&dir).contains_key("Nope.sh"));
    }

    #[test]
    fn an_update_needs_both_versions_and_a_difference() {
        assert!(newer(Some("aaa"), Some("bbb")));
        assert!(!newer(Some("aaa"), Some("aaa")), "same version");
        assert!(!newer(None, Some("bbb")), "installed version unknown: quiet");
        assert!(!newer(Some("aaa"), None), "catalogue not read yet: quiet");
        assert!(!newer(Some(""), Some("bbb")));
        assert!(!newer(Some("aaa"), Some("")));
    }

    #[test]
    fn the_catalogue_carries_each_ports_version() {
        let json = br#"{"ports":{"2048.zip":{"name":"2048.zip","items":["2048.sh","2048/"],"attr":{"title":"2048","rtr":true,"arch":["aarch64"]},"source":{"size":1,"md5":"b41f"}},
            "nover.zip":{"name":"nover.zip","items":["N.sh","n"],"attr":{"title":"N","rtr":true,"arch":["aarch64"]},"source":{"size":1}}}}"#;
        let ports = parse_catalog(json);
        assert_eq!(ports.iter().find(|p| p.name == "2048.zip").unwrap().md5, "b41f");
        assert_eq!(ports.iter().find(|p| p.name == "nover.zip").unwrap().md5, "", "a catalogue entry without one never reads as an update");
        note_versions(&ports);
        let versions = VERSIONS.lock().unwrap();
        assert_eq!(versions.as_ref().unwrap().get("2048.zip").map(String::as_str), Some("b41f"));
        assert!(!versions.as_ref().unwrap().contains_key("nover.zip"));
    }

    #[test]
    fn finds_an_installed_port_by_its_launcher() {
        let dir = TestDir::new("installed");
        for d in ["sprout", "PortMaster", "2048", "empty"] {
            std::fs::create_dir_all(dir.join(d)).unwrap();
        }
        // As harbourmaster writes it: the catalog entry plus "files" and "status".
        std::fs::write(
            dir.join("sprout/port.json"),
            r#"{"version":4,"name":"sprout.zip","items":["SPROUT.sh","sprout"],"items_opt":null,"attr":{"title":"-SPROUT-","inst":"Place the game files in sprout/assets.","rtr":false,"exp":false,"arch":["aarch64"]},"status":{"status":"Installed"},"files":{"port.json":"sprout/port.json","SPROUT.sh":"SPROUT.sh","sprout":"sprout"}}"#,
        )
        .unwrap();
        std::fs::write(dir.join("2048/port.json"), r#"{"items":["2048.sh","2048/"],"attr":{"title":"2048","rtr":true}}"#).unwrap();
        std::fs::write(dir.join("PortMaster/port.json"), r#"{"items":["SPROUT.sh"],"attr":{"title":"not a port"}}"#).unwrap();
        let p = installed_port(&dir, &dir.join("SPROUT.sh")).unwrap();
        assert_eq!((p.title.as_str(), p.dir.as_str(), p.rtr), ("-SPROUT-", "sprout", false));
        assert_eq!(p.instructions, "Place the game files in sprout/assets.");
        assert!(p.written.is_some());
        assert!(installed_port(&dir, &dir.join("2048.sh")).unwrap().rtr);
        assert_eq!(installed_port(&dir, &dir.join("Other.sh")), None);
        // A hand-made port.json without "rtr" is never treated as needing files.
        std::fs::write(dir.join("empty/port.json"), r#"{"items":["Empty.sh","empty"],"attr":{"title":"Empty"}}"#).unwrap();
        assert!(installed_port(&dir, &dir.join("Empty.sh")).unwrap().rtr);
    }
}
