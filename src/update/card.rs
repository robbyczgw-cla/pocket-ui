//! Updates from the card, for handhelds without Wi-Fi (the RG28XX, the R36S and its clones) or with Wi-Fi off or broken
//! (docs/user/updates.md "No Wi-Fi? Update from the card", docs/UPDATE-CHANNEL.md "The update file").
//!
//! The release page has one update file per kind of card, `pocket-ui-update-<version>-<rocknix|trimui|darkos>.zip`
//! (`scripts/make-card-update.py`): the channel's own signed `update.json` and `update.json.sig`, the app's tarball that
//! list names, and for the Brick and dArkOS the component packages it needs (`assets/`), all stored as they are (no
//! compression). Nothing new is trusted: the list's signature with the store key, then every file by the size and SHA-256
//! the signed list (or the signed tarball's `asset-packages.json`) gives, then the very install a Wi-Fi update runs
//! (`update::install_app`, `update::install`), whose download step takes its file from here instead of a server
//! (`from_bundle`). A folder of the same name holding those files counts the same: a Mac unpacks a downloaded zip by
//! itself.
//!
//! Where it is looked for (`places_for`): the parts of the card a computer shows. On a ROCKNIX card the first part
//! (`/flash`), the GAMES drive of the RG28XX card and a games card in the second slot (`/storage/games-external`, and its
//! `roms` folder); on the Brick the card's root and the pocket-ui folder; on dArkOS the games part (EASYROMS) and the port's
//! folder.
//!
//! The file is deleted after a good install (`remove_after_use`; `/flash` is mounted read-only, so it is mounted writable for that
//! moment). If that fails, nothing loops: a file whose version is not newer is passed over in silence.

use std::cell::RefCell;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use super::{Cancel, Manifest, Release};

/// The start of the update file's name (and of a folder made by unpacking it), in any case.
pub const PREFIX: &str = "pocket-ui-update";

pub const INCOMPLETE: &str = "The update file on the card is incomplete or damaged. Copy it onto the card again. Nothing was installed.";
pub const NOT_SIGNED: &str = "The update file on the card isn't signed by us. Nothing was installed.";
pub const OTHER_TARGET: &str = "The update file on the card is for another kind of handheld. Get the one for this handheld at pocketui.xyz/install. Nothing was installed.";
pub const NOTHING: &str = "No update file on the card. Put the update file from pocketui.xyz/install on the card, then look again.";
pub const NEEDS_MORE: &str = "This update needs files that aren't in the update file on the card. Get the newest update file, or update over Wi-Fi. Nothing was installed.";
/// [`NEEDS_MORE`] on a handheld without Wi-Fi (`device::no_wifi_chip`): the card is the only way there.
pub const NEEDS_MORE_NO_WIFI: &str = "This update needs files that aren't in the update file on the card. Get the newest update file from pocketui.xyz/install. Nothing was installed.";

/// What an update file that lacks a component says, on a handheld with (`no_chip` false) or without Wi-Fi.
pub fn needs_more(no_chip: bool) -> &'static str {
    if no_chip { NEEDS_MORE_NO_WIFI } else { NEEDS_MORE }
}

/// "The update on the card (0.4.2) isn't newer than this pocket-ui."
pub fn not_newer(version: &str) -> String {
    format!("The update on the card ({version}) isn't newer than this pocket-ui. Nothing was changed.")
}

/// The question on the shelf when a newer update lies on the card.
pub fn found(version: &str) -> String {
    format!("Update to pocket-ui {version} found on the card. Your games and settings stay.")
}

/// The notice's last line for [`found`].
pub const FOUND_ACTIONS: &str = "Press A to install it, B to close.";

/// Which card's channel a list belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target { Rocknix, Brick, Darkos }

impl Target {
    /// This build on this card.
    pub fn here() -> Target {
        #[cfg(feature = "brick")]
        {
            if crate::darkos::host() { return Target::Darkos; }
            if super::brick_card().is_some() { return Target::Brick; }
        }
        Target::Rocknix
    }

    /// The last part of the update file's name.
    pub fn file_part(self) -> &'static str {
        match self { Target::Rocknix => "rocknix", Target::Brick => "trimui", Target::Darkos => "darkos" }
    }
}

/// Read from the signed list itself: dArkOS's names its target, the Brick's app is the card folder's tarball, ROCKNIX's
/// the aarch64 one (docs/UPDATE-CHANNEL.md "Layout").
fn target_of(json: &[u8], manifest: &Manifest) -> Option<Target> {
    let value: serde_json::Value = serde_json::from_slice(json).ok()?;
    match value.get("target") {
        Some(t) if t.as_str() == Some("darkos-port") && manifest.os.is_none() => return Some(Target::Darkos),
        Some(_) => return None,
        None => {}
    }
    let name = manifest.app.as_ref()?.url.rsplit('/').next()?.to_string();
    if name.starts_with("pocket-ui-brick-") { Some(Target::Brick) }
    else if name.starts_with("pocket-ui-") && name.ends_with("-aarch64.tar.gz") { Some(Target::Rocknix) }
    else { None }
}

/// One file of an update file: in a zip `at` bytes into it, in a folder a file of its own.
#[derive(Clone, Debug)]
struct Entry { name: String, path: PathBuf, at: u64, size: u64 }

/// An update file opened: what is in it, nothing read yet.
#[derive(Clone, Debug)]
pub struct Bundle { entries: Vec<Entry> }

impl Bundle {
    pub fn open(file: &Path) -> Result<Bundle, String> {
        let mut entries = Vec::new();
        if file.is_dir() {
            for (dir, prefix) in [(file.to_path_buf(), ""), (file.join("assets"), "assets/")] {
                let Ok(list) = std::fs::read_dir(&dir) else { continue };
                for e in list.flatten() {
                    let Ok(meta) = e.metadata() else { continue };
                    let name = e.file_name().to_string_lossy().into_owned();
                    if meta.is_file() && !name.starts_with("._") {
                        entries.push(Entry { name: format!("{prefix}{name}"), path: e.path(), at: 0, size: meta.len() });
                    }
                }
            }
        } else {
            let list = crate::archive::stored_entries(file).map_err(|_| INCOMPLETE.to_string())?.ok_or(INCOMPLETE)?;
            entries = list.into_iter().map(|(name, at, size)| Entry { name, path: file.to_path_buf(), at, size }).collect();
        }
        Ok(Bundle { entries })
    }

    fn entry(&self, name: &str) -> Option<&Entry> {
        self.entries.iter().find(|e| e.name == name)
    }

    fn reader(e: &Entry) -> std::io::Result<impl Read> {
        let mut f = std::fs::File::open(&e.path)?;
        f.seek(SeekFrom::Start(e.at))?;
        Ok(f.take(e.size))
    }

    fn read_small(&self, name: &str, limit: u64) -> Option<Vec<u8>> {
        let e = self.entry(name).filter(|e| e.size <= limit)?;
        let mut out = Vec::new();
        Self::reader(e).ok()?.read_to_end(&mut out).ok()?;
        (out.len() as u64 == e.size).then_some(out)
    }

    fn sha256(e: &Entry) -> std::io::Result<String> {
        use sha2::{Digest, Sha256};
        let mut hash = Sha256::new();
        let mut r = Self::reader(e)?;
        let mut buf = vec![0u8; 1 << 20];
        let mut seen = 0u64;
        loop {
            let n = r.read(&mut buf)?;
            if n == 0 { break; }
            seen += n as u64;
            hash.update(&buf[..n]);
        }
        if seen != e.size { return Err(std::io::ErrorKind::UnexpectedEof.into()); }
        Ok(hash.finalize().iter().map(|b| format!("{b:02x}")).collect())
    }

    /// The file with exactly this size and SHA-256, whatever its name: the signed hash decides, not the name.
    fn find(&self, size: u64, sha256: &str) -> Option<&Entry> {
        let want = sha256.to_lowercase();
        self.entries.iter().filter(|e| e.size == size).find(|e| Self::sha256(e).is_ok_and(|h| h == want))
    }

    /// Copies `e` to `dest` in pieces, with the progress and the cancel of a download.
    fn copy_to(e: &Entry, dest: &Path, cancel: &Cancel, progress: &dyn Fn(u64)) -> Result<(), String> {
        let mut r = Self::reader(e).map_err(|_| INCOMPLETE.to_string())?;
        let mut out = std::fs::File::create(dest).map_err(|_| super::CARD_WRITE.to_string())?;
        let mut buf = vec![0u8; 1 << 20];
        let mut done = 0u64;
        progress(0);
        loop {
            if cancel.is_cancelled() { return Err("Download cancelled.".into()); }
            let n = r.read(&mut buf).map_err(|_| INCOMPLETE.to_string())?;
            if n == 0 { break; }
            out.write_all(&buf[..n]).map_err(|_| super::CARD_WRITE.to_string())?;
            done += n as u64;
            progress(done);
        }
        out.flush().map_err(|_| super::CARD_WRITE.to_string())
    }
}

thread_local! {
    /// The update file the install on this thread takes its files from, and the SHA-256 of its app (`install_with`).
    static ACTIVE: RefCell<Option<(Bundle, String)>> = const { RefCell::new(None) };
}

/// The download step of an install from the card (`update::download_checked`): None when this thread installs from a
/// server. Else the file comes from the update file, checked by the signed size and SHA-256 like a download. An install
/// from the card never goes to a server: what the file lacks is said, not tried for minutes.
pub(super) fn from_bundle(rel: &Release, dest: &Path, cancel: &Cancel, progress: &dyn Fn(u64)) -> Option<Result<(), String>> {
    let (bundle, app) = ACTIVE.with(|a| a.borrow().clone())?;
    Some(from(&bundle, &app, rel, dest, cancel, progress))
}

fn from(bundle: &Bundle, app: &str, rel: &Release, dest: &Path, cancel: &Cancel, progress: &dyn Fn(u64)) -> Result<(), String> {
    let Some(entry) = bundle.find(rel.size, &rel.sha256) else {
        // The app itself changed since the look (copied again, half): the file. A component package: the update needs more.
        return Err(if rel.sha256.eq_ignore_ascii_case(app) { INCOMPLETE } else { needs_more(crate::device::no_wifi_chip()) }.into());
    };
    cancel.set_phase(super::WorkerPhase::Downloading);
    let copied = Bundle::copy_to(entry, dest, cancel, progress).and_then(|()| {
        cancel.set_phase(super::WorkerPhase::CheckingDownload);
        let size = std::fs::metadata(dest).map_or(0, |m| m.len());
        let hash = crate::store::sha256_file(dest).map_err(|e| e.to_string())?;
        if size != rel.size || hash != rel.sha256.to_lowercase() { return Err(super::DAMAGED.to_string()); }
        cancel.set_phase(super::WorkerPhase::Preparing);
        Ok(())
    });
    if copied.is_err() { let _ = std::fs::remove_file(dest); }
    copied
}

/// A newer update that passed every check before the install: the signed list (only its app) and where it lies.
#[derive(Clone, Debug)]
pub struct Found {
    pub file: PathBuf,
    pub manifest: Manifest,
    /// Name, size and time of the file: the start says each file once (`App::card_update_looked`).
    pub id: String,
}

impl Found {
    pub fn version(&self) -> &str {
        self.manifest.app.as_ref().map_or("", |a| a.version.as_str())
    }
}

/// What the card holds.
#[derive(Clone, Debug)]
pub enum Look {
    Nothing,
    Offer(Found),
    /// The newest file there is this version or older (also the file of the update just installed).
    NotNewer { version: String },
    /// A file that can't be installed, and why (in the player's words).
    Refused { id: String, why: String },
}

impl Look {
    /// The line for the Updates page.
    pub fn message(&self) -> String {
        match self {
            Look::Nothing => NOTHING.into(),
            Look::Offer(found) => found_message(found.version()),
            Look::NotNewer { version } => not_newer(version),
            Look::Refused { why, .. } => why.clone(),
        }
    }
}

/// The page's line above "Install now / Later" for an update from the card.
pub fn found_message(version: &str) -> String {
    format!("Update to pocket-ui {version} found on the card.")
}

fn file_id(file: &Path) -> String {
    let meta = std::fs::metadata(file).ok();
    let mtime = meta.as_ref().and_then(|m| m.modified().ok()).and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs());
    let size = meta.filter(|m| m.is_file()).map_or(0, |m| m.len());
    format!("{}|{size}|{mtime}", file.display())
}

enum Refusal { Not(String), NotNewer(String) }

/// Every check of one update file, in the order a Wi-Fi update does them, and the app's tarball read once whole.
fn inspect(file: &Path, current: &str, release_build: bool, key: &[u8; 32], target: Target) -> Result<Found, Refusal> {
    let bundle = Bundle::open(file).map_err(Refusal::Not)?;
    let json = bundle.read_small("update.json", 1 << 20).ok_or_else(|| Refusal::Not(INCOMPLETE.into()))?;
    let sig = bundle.read_small("update.json.sig", 4096).ok_or_else(|| Refusal::Not(INCOMPLETE.into()))?;
    crate::signed_data::verify_signed(&json, &String::from_utf8_lossy(&sig), key).map_err(|_| Refusal::Not(NOT_SIGNED.into()))?;
    let mut manifest: Manifest = serde_json::from_slice(&json).map_err(|_| Refusal::Not(INCOMPLETE.into()))?;
    if manifest.format != 1 || target_of(&json, &manifest) != Some(target) {
        return Err(Refusal::Not(OTHER_TARGET.into()));
    }
    let app = manifest.app.clone().ok_or_else(|| Refusal::Not(OTHER_TARGET.into()))?;
    if !release_build {
        return Err(Refusal::Not(super::DEVELOPMENT_BUILD.into()));
    }
    if !super::newer(&app.version, current) {
        return Err(Refusal::NotNewer(app.version));
    }
    if bundle.find(app.size, &app.sha256).is_none() {
        return Err(Refusal::Not(INCOMPLETE.into()));
    }
    // Only the app: a system update or the people's downloads in the same list are no business of the card's.
    manifest = Manifest { format: manifest.format, app: Some(app), ..Default::default() };
    Ok(Found { file: file.to_path_buf(), manifest, id: file_id(file) })
}

/// The update files and unpacked folders directly in `dir`.
fn candidates(dir: &Path) -> Vec<PathBuf> {
    let Ok(list) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut out: Vec<PathBuf> = list.flatten().filter_map(|e| {
        let name = e.file_name().to_string_lossy().to_lowercase();
        let dir = e.metadata().is_ok_and(|m| m.is_dir());
        (name.starts_with(PREFIX) && (dir || name.ends_with(".zip"))).then(|| e.path())
    }).collect();
    out.sort();
    out
}

/// Where a computer puts the file, for this kind of card. `card`: the Brick's or the dArkOS port's pocket-ui folder.
pub fn places_for(target: Target, card: Option<&Path>) -> Vec<PathBuf> {
    if let Ok(list) = std::env::var("POCKETUI_CARD_PLACES") {
        return std::env::split_paths(&list).filter(|p| !p.as_os_str().is_empty()).collect();
    }
    match (target, card) {
        (Target::Rocknix, _) | (_, None) => ["/flash", "/storage/games-external", "/storage/roms"].iter().map(PathBuf::from).collect(),
        // The card's root (the folder over pocket-ui) and pocket-ui's own folder.
        (Target::Brick, Some(card)) => [card.parent().map(Path::to_path_buf), Some(card.to_path_buf())].into_iter().flatten().collect(),
        // /roms (the games part) over ports/pocket-ui, and the port's folder.
        (Target::Darkos, Some(card)) => [card.parent().and_then(Path::parent).map(Path::to_path_buf), Some(card.to_path_buf())].into_iter().flatten().collect(),
    }
}

/// This device's places.
pub fn places() -> Vec<PathBuf> {
    #[cfg(feature = "brick")]
    let card = super::brick_card();
    #[cfg(not(feature = "brick"))]
    let card: Option<PathBuf> = None;
    places_for(Target::here(), card.as_deref())
}

/// What the card holds for this pocket-ui: the newest update that passes, else the first file that doesn't and why, else
/// the newest that isn't newer. The same file seen twice (a folder bound into another) counts once.
pub fn look_in(places: &[PathBuf], current: &str, release_build: bool, key: &[u8; 32], target: Target) -> Look {
    use std::os::unix::fs::MetadataExt;
    let mut seen = std::collections::HashSet::new();
    let (mut offer, mut refused, mut older): (Option<Found>, Option<Look>, Option<String>) = (None, None, None);
    for file in places.iter().flat_map(|p| candidates(p)) {
        if !std::fs::metadata(&file).is_ok_and(|m| seen.insert((m.dev(), m.ino()))) { continue; }
        match inspect(&file, current, release_build, key, target) {
            Ok(found) => {
                if offer.as_ref().is_none_or(|o| super::newer(found.version(), o.version())) { offer = Some(found); }
            }
            Err(Refusal::NotNewer(v)) => {
                if older.as_ref().is_none_or(|o| super::newer(&v, o)) { older = Some(v); }
            }
            Err(Refusal::Not(why)) => {
                eprintln!("update from the card: {}: {why}", file.display());
                refused.get_or_insert(Look::Refused { id: file_id(&file), why });
            }
        }
    }
    offer.map(Look::Offer).or(refused).or(older.map(|version| Look::NotNewer { version })).unwrap_or(Look::Nothing)
}

/// This device, this pocket-ui.
pub fn look() -> Look {
    look_in(&places(), super::app_version(), super::release_build(), &crate::store::PUBLIC_KEY, Target::here())
}

/// Installs the app of `found` from its update file the way a Wi-Fi update installs it, then deletes the file.
pub fn install(found: &Found, cancel: &Cancel, progress: &dyn Fn(u64)) -> Result<(), String> {
    let rel = found.manifest.app.clone().ok_or(INCOMPLETE)?;
    install_with(&found.file, &rel, cancel, progress, &|rel, url, cancel, progress| {
        #[cfg(feature = "brick")]
        return super::install(rel, url, cancel, progress);
        #[cfg(not(feature = "brick"))]
        super::install_app(rel, url, Path::new(super::APP_ROOT), cancel, progress)
    })
}

type Installer<'a> = dyn Fn(&Release, &str, &Cancel, &dyn Fn(u64)) -> Result<(), String> + 'a;

pub(super) fn install_with(file: &Path, rel: &Release, cancel: &Cancel, progress: &dyn Fn(u64), installer: &Installer) -> Result<(), String> {
    // Where relative names in the list would point; every file is taken from the bundle anyway.
    let url = format!("file://{}/update.json", file.display());
    with_file(file, &rel.sha256, || installer(rel, &url, cancel, progress))??;
    if !remove_after_use(file) {
        eprintln!("update from the card: {} stays (couldn't delete it); it is older than the new version now", file.display());
    }
    Ok(())
}

/// Runs `f` with every download on this thread taken from the update file `file` (`from_bundle`); `app`: the SHA-256 of
/// the app it installs.
pub(super) fn with_file<T>(file: &Path, app: &str, f: impl FnOnce() -> T) -> Result<T, String> {
    let bundle = Bundle::open(file)?;
    struct Reset;
    impl Drop for Reset { fn drop(&mut self) { ACTIVE.with(|a| *a.borrow_mut() = None); } }
    ACTIVE.with(|a| *a.borrow_mut() = Some((bundle, app.to_string())));
    let _reset = Reset;
    Ok(f())
}

/// Deletes the update file, or the files of an unpacked one and then its folder if nothing else is in it.
pub fn remove_after_use(file: &Path) -> bool {
    let remove = |f: &Path| -> std::io::Result<()> {
        if !f.is_dir() { return std::fs::remove_file(f); }
        for dir in [f.join("assets"), f.to_path_buf()] {
            if let Ok(list) = std::fs::read_dir(&dir) {
                for e in list.flatten() {
                    if e.metadata().is_ok_and(|m| m.is_file()) { std::fs::remove_file(e.path())?; }
                }
            }
            let _ = std::fs::remove_dir(&dir);
        }
        Ok(())
    };
    let delete = || {
        remove(file)?;
        if let Some(parent) = file.parent() { crate::fsutil::sync_dir(parent); }
        Ok(())
    };
    let done = if file.starts_with("/flash") { crate::fsutil::with_writable_flash(delete) } else { delete() }.is_ok();
    done && !file.exists()
}

#[cfg(test)]
#[path = "card_tests.rs"]
mod tests;
