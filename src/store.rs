//! pocket-store: optional extras (cores, frames, themes, tools) installed on demand, in the
//! spirit of NextUI's Pak Store, with two additions it lacks:
//! - the catalog is signed (Ed25519), so only packages we published are offered;
//! - every download is checked against the SHA-256 in the catalog before anything is unpacked.
//!
//! Catalog: `catalog.json` plus `catalog.json.sig` (base64 signature of the exact bytes).
//! Installed packages and the files they wrote: `<data>/store/installed.json`.
//! Package URLs may be relative to the catalog, so a catalog folder can be served from anywhere,
//! `file://` included (scripts/make-test-catalog.sh). Format and publishing: docs/STORE.md.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use base64::Engine;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Public half of the catalog signing key (`store-tool keygen`); the private half never leaves
/// the build machine.
pub const PUBLIC_KEY: [u8; 32] = [
    0xf1, 0x86, 0x0a, 0xf1, 0xb2, 0xf1, 0xb3, 0xc6, 0x0b, 0x24, 0x22, 0xcd, 0xfc, 0xa9, 0xf3, 0xc0,
    0xb8, 0x49, 0x52, 0x07, 0xf5, 0x11, 0x96, 0xe1, 0x75, 0x84, 0x65, 0x32, 0x4f, 0x83, 0xe0, 0x1f,
];

/// The built-in catalog address. Empty: there is no public catalog yet, so without a `store_url` in settings.json
/// the Store is not offered at all (`catalog_url`). Its place is decided (`https://pocketui.xyz/updates/store/catalog.json`,
/// docs/UPDATE-CHANNEL.md); put it here in the release that ships with the first catalog, and the Settings row (and
/// everything that opens the Store) comes back by itself.
pub const DEFAULT_URL: &str = "";

/// The catalog this device uses: settings.json `store_url` when it is set (and not blank), otherwise the built-in
/// `DEFAULT_URL`; `None` when neither exists, which is when the Store is hidden.
pub fn catalog_url(setting: Option<&str>) -> Option<String> {
    let url = setting.map(str::trim).filter(|u| !u.is_empty()).unwrap_or(DEFAULT_URL);
    (!url.is_empty()).then(|| url.to_string())
}

/// Where libretro cores from the store go. ROCKNIX mounts it as the upper layer of the overlay
/// `/tmp/cores` (lower: `/usr/lib/libretro`), which is where RetroArch loads cores from.
pub const CORES_DIR: &str = "/storage/cores";

/// ROCKNIX's Tools folder, which EmulationStation lists. ROCKNIX's autostart `001-sync-modules` runs
/// `rsync -a --delete /usr/config/modules/ /storage/.config/modules/` at every boot (and `post-update` again), so a
/// file of ours there is gone after the next start (confirmed on the Mini V2, 2026-10-23). Store tools therefore go to
/// `tools_dir` (`Package::install_dir`); Settings → Tools lists both (`tools.rs`).
pub const ROCKNIX_MODULES: &str = "/storage/.config/modules";

/// pocket-ui's own tools folder: `<data>/tools`, which nothing of ROCKNIX touches (not mirrored, not reset by
/// an update, and kept by Backup & reset).
pub fn tools_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("tools")
}

/// The core file pocket-core should load: one installed into `/storage/cores` wins over the
/// image's, the same order the `/tmp/cores` overlay gives RetroArch.
pub fn core_path(core: &str) -> PathBuf {
    let file = format!("{core}_libretro.so");
    let installed = Path::new(CORES_DIR).join(&file);
    // The Steam Deck app: POCKETUI_CORES is a ':'-separated list of folders (bundled cores, RetroArch's flatpak) searched next.
    let extra = || std::env::var("POCKETUI_CORES").ok().and_then(|d| d.split(':').map(|p| Path::new(p).join(&file)).find(|p| p.is_file()));
    if installed.is_file() {
        installed
    } else if let Some(p) = extra() {
        p
    } else {
        Path::new("/usr/lib/libretro").join(file)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// A .tar.gz unpacked into `dest`.
    Tar,
    /// A single file copied to `dest/<name>`.
    File,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Package {
    pub id: String,
    pub name: String,
    /// "cores", "frames", "themes", "tools": the Store's pages; any other value gets a page of its own.
    pub category: String,
    pub version: String,
    pub description: String,
    pub size: u64,
    /// Absolute, or relative to the catalog.
    pub url: String,
    pub sha256: String,
    pub kind: Kind,
    pub dest: String,
    /// File name for `Kind::File`.
    #[serde(default)]
    pub name_on_disk: Option<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub author: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub license: String,
    /// Tools: the script (relative to `dest`) that the Store's Run action starts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run: Option<String>,
}

impl Package {
    /// Where the package's files go on this device: `dest`, except for tools (category `tools`, or a package aimed at
    /// ROCKNIX's Tools folder, as catalogs before 2026-10-23 were), which go to `tools_dir`: ROCKNIX empties its own
    /// folder at every boot (`ROCKNIX_MODULES`).
    pub fn install_dir(&self, data_dir: &Path) -> PathBuf {
        if self.category == "tools" || Path::new(self.dest.trim_end_matches('/')) == Path::new(ROCKNIX_MODULES) {
            tools_dir(data_dir)
        } else {
            PathBuf::from(&self.dest)
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Catalog {
    pub version: u32,
    /// Raised with every published catalog; a device refuses one lower than the newest it has seen
    /// from the same URL (B5). Optional for older catalogs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub serial: Option<u64>,
    /// Unix time (seconds) after which the catalog is refused, so an old copy can't be served forever.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires: Option<u64>,
    pub packages: Vec<Package>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Installed {
    pub version: String,
    pub files: Vec<PathBuf>,
    /// Folders the package created (parents first); removed with it when they are empty.
    #[serde(default)]
    pub dirs: Vec<PathBuf>,
    /// The package's name in the catalog, for Settings → Tools (which lists installed tools without the catalog).
    /// Missing in lists written before 2026-10-23.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// `Package::run` of a tool: what Settings → Tools starts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run: Option<String>,
}

/// Checks that `data` was signed by `key` (base64 Ed25519 signature, as `store-tool sign` writes it).
pub fn verify_signed(data: &[u8], sig_b64: &str, key: &[u8; 32]) -> Result<(), String> {
    let key = VerifyingKey::from_bytes(key).map_err(|e| format!("bad public key: {e}"))?;
    let sig = base64::engine::general_purpose::STANDARD.decode(sig_b64.trim()).map_err(|_| "signature is not base64".to_string())?;
    let sig = Signature::from_slice(&sig).map_err(|_| "signature has the wrong length".to_string())?;
    key.verify(data, &sig).map_err(|_| "signature doesn't match".to_string())
}

/// Checks the signature and parses the catalog; anything not signed by `key` is rejected.
pub fn verify_catalog(json: &[u8], sig_b64: &str, key: &[u8; 32]) -> Result<Catalog, String> {
    verify_signed(json, sig_b64, key).map_err(|e| format!("catalog {e}"))?;
    serde_json::from_slice(json).map_err(|e| format!("catalog: {e}"))
}

pub fn sha256_hex(data: &[u8]) -> String {
    Sha256::digest(data).iter().map(|b| format!("{b:02x}")).collect()
}

/// SHA-256 of a file read in chunks, for downloads too big to hold in memory (OS images).
pub fn sha256_file(path: &Path) -> std::io::Result<String> {
    use std::io::Read;
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher.finalize().iter().map(|b| format!("{b:02x}")).collect())
}


type Inventory = BTreeMap<String, Installed>;

pub struct Store {
    dir: PathBuf,
    /// pocket-ui's data folder (`dir`'s parent): where tools go (`Package::install_dir`).
    data: PathBuf,
    pub installed: Inventory,
    /// Why installed.json couldn't be read. The Store then installs and removes nothing: with the
    /// list read as empty, every package would look new and its files would be orphaned.
    damaged: Option<String>,
}

impl Store {
    pub fn open(data_dir: &Path) -> Store {
        let dir = data_dir.join("store");
        let path = dir.join("installed.json");
        let read = match std::fs::read(&path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Inventory::new()),
            Err(e) => Err(e.to_string()),
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(|e| e.to_string()),
        };
        let damaged = read.as_ref().err().map(|e| {
            eprintln!("store: {} can't be read ({e}); the Store changes nothing until it is fixed", path.display());
            let bak = if dir.join("installed.json.bak").is_file() { "; the last good one is installed.json.bak next to it" } else { "" };
            format!("The list of installed packages ({}) is damaged, so the Store changes nothing{bak}.", path.display())
        });
        Store { dir, data: data_dir.to_path_buf(), installed: read.unwrap_or_default(), damaged }
    }

    /// Set when installed.json is damaged; installs and removals are refused then.
    pub fn damaged(&self) -> Option<&str> {
        self.damaged.as_deref()
    }

    fn usable(&self) -> Result<(), String> {
        self.damaged.clone().map_or(Ok(()), Err)
    }

    fn save(&self) -> Result<(), String> {
        self.usable()?;
        std::fs::create_dir_all(&self.dir).map_err(|e| e.to_string())?;
        let json = serde_json::to_vec_pretty(&self.installed).map_err(|e| e.to_string())?;
        let path = self.dir.join("installed.json");
        // The list before this change stays as .bak, should the new one get damaged anyway.
        if let Ok(old) = std::fs::read(&path) {
            if serde_json::from_slice::<Inventory>(&old).is_ok() {
                crate::fsutil::write_atomic(&self.dir.join("installed.json.bak"), &old).map_err(|e| e.to_string())?;
            }
        }
        crate::fsutil::write_atomic(&path, &json).map_err(|e| e.to_string())
    }

    /// Installed and up to date / installed but older / not installed.
    pub fn status(&self, p: &Package) -> Status {
        match self.installed.get(&p.id) {
            // Only a later version is an update: an older catalog must not offer a downgrade (B5).
            Some(i) if crate::update::newer(&p.version, &i.version) => Status::UpdateAvailable,
            Some(_) => Status::Installed,
            None => Status::NotInstalled,
        }
    }

    /// Downloads with curl, checks the hash, then installs.
    pub fn install(&mut self, p: &Package) -> Result<(), String> {
        self.install_from(p, "", &|_| {})
    }

    /// Like `install`, with `p.url` relative to `catalog_url`; `progress` gets the bytes downloaded
    /// so far while the download runs.
    pub fn install_from(&mut self, p: &Package, catalog_url: &str, progress: &dyn Fn(u64)) -> Result<(), String> {
        self.usable()?;
        std::fs::create_dir_all(&self.dir).map_err(|e| e.to_string())?;
        let tmp = self.dir.join(format!(".download-{}", p.id));
        let data = download_to(&resolve(catalog_url, &p.url), &tmp, Some(p.size), progress).and_then(|_| std::fs::read(&tmp).map_err(|e| e.to_string()));
        let _ = std::fs::remove_file(&tmp);
        self.install_bytes(p, &data?)
    }

    /// Installs or updates `p` from its downloaded bytes. Everything is unpacked and checked in a
    /// private folder first; the installed version (if any) is only touched once all is well.
    pub fn install_bytes(&mut self, p: &Package, data: &[u8]) -> Result<(), String> {
        let _changing = Changing::start();
        self.usable()?;
        if sha256_hex(data) != p.sha256.to_lowercase() {
            return Err("The download is damaged or was changed. Nothing was installed.".into());
        }
        // The id names files of ours (`.staging-<id>`), so it may not be a path.
        if p.id.is_empty() || p.id.starts_with('.') || !p.id.chars().all(|c| c.is_ascii_alphanumeric() || "._-".contains(c)) {
            return Err("The package has an unsafe ID. Nothing was installed.".into());
        }
        if self.installed.get(&p.id).is_some_and(|i| crate::update::newer(&i.version, &p.version)) {
            return Err(format!("{} is older than the installed version. Nothing was installed.", p.version));
        }
        let dest = p.install_dir(&self.data);
        // Files are compared by path to find their owner, so `dest` has to be written one way only.
        if !dest.is_absolute() || dest.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
            return Err("The package has an unsafe destination. Nothing was installed.".into());
        }
        std::fs::create_dir_all(&self.dir).map_err(|e| e.to_string())?;
        let staging = self.dir.join(format!(".staging-{}", p.id));
        let _ = std::fs::remove_dir_all(&staging);
        let result = stage(p, data, &staging).and_then(|list| self.swap(p, &dest, &staging, &list));
        let _ = std::fs::remove_dir_all(&staging);
        result
    }

    /// Checks the staged `list` against `dest` and the other packages, then moves it in and
    /// removes what the installed version had and this one doesn't.
    fn swap(&mut self, p: &Package, dest: &Path, staging: &Path, list: &[crate::untar::Entry]) -> Result<(), String> {
        let targets: Vec<PathBuf> = list.iter().filter(|e| !e.dir).map(|e| dest.join(&e.path)).collect();
        self.check_owner(&p.id, &targets)?;
        let old = self.installed.get(&p.id).cloned();
        let (old_files, old_dirs) = old.as_ref().map_or((&[][..], &[][..]), |o| (&o.files[..], &o.dirs[..]));
        let folders = folders_for(dest, list);
        for dir in &folders {
            match std::fs::symlink_metadata(dir) {
                Err(_) => {}
                Ok(m) if m.is_dir() => {}
                // A file of the old version may become a folder.
                Ok(m) if m.is_file() && old_files.contains(dir) => {}
                // A link in the way would lead the files somewhere else.
                Ok(_) => return Err(format!("{} is in the way. Nothing was installed.", dir.display())),
            }
        }
        if let Some(t) = targets.iter().find(|t| std::fs::symlink_metadata(t).is_ok_and(|m| m.is_dir())) {
            return Err(format!("{} is a folder. Nothing was installed.", t.display()));
        }

        // From here on files change. Whatever is in place by then is recorded, so Remove can clean up.
        std::fs::create_dir_all(dest).map_err(|e| format!("{}: {e}", dest.display()))?;
        let mut dirs: Vec<PathBuf> = Vec::new();
        let mut files: Vec<PathBuf> = Vec::new();
        let moved = (|| {
            for dir in &folders {
                if std::fs::symlink_metadata(dir).is_ok_and(|m| m.is_file()) {
                    std::fs::remove_file(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
                }
                if std::fs::symlink_metadata(dir).is_err() {
                    // 0755 whatever the umask (root's 022 on the Pocket, 002 for a user on Debian): the package's
                    // folders are the same everywhere.
                    use std::os::unix::fs::DirBuilderExt;
                    std::fs::DirBuilder::new().mode(0o755).create(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
                    std::fs::set_permissions(dir, std::os::unix::fs::PermissionsExt::from_mode(0o755)).map_err(|e| format!("{}: {e}", dir.display()))?;
                    dirs.push(dir.clone());
                } else if old_dirs.contains(dir) {
                    // Made by the old version: still the package's.
                    dirs.push(dir.clone());
                }
            }
            for (e, target) in list.iter().filter(|e| !e.dir).zip(&targets) {
                move_file(&staging.join(&e.path), target).map_err(|e| format!("{}: {e}", target.display()))?;
                files.push(target.clone());
            }
            Ok::<(), String>(())
        })();
        if let Err(e) = moved {
            let mut keep = old.clone().unwrap_or_else(|| Installed { version: p.version.clone(), name: Some(p.name.clone()), run: p.run.clone(), ..Default::default() });
            keep.files.extend(files.into_iter().filter(|f| !old_files.contains(f)));
            keep.dirs.extend(dirs.into_iter().filter(|d| !old_dirs.contains(d)));
            self.installed.insert(p.id.clone(), keep);
            let _ = self.save();
            return Err(format!("Couldn't put the package in place ({e})."));
        }
        // What the old version had and this one doesn't (unless another package lists it too).
        for f in old_files.iter().filter(|f| !files.contains(f)) {
            if !self.installed.iter().any(|(id, o)| *id != p.id && o.files.contains(f)) {
                let _ = std::fs::remove_file(f);
            }
        }
        for d in old_dirs.iter().rev().filter(|d| !dirs.contains(d)) {
            let _ = std::fs::remove_dir(d);
        }
        self.installed.insert(p.id.clone(), Installed { version: p.version.clone(), files, dirs, name: Some(p.name.clone()), run: p.run.clone() });
        self.save()
    }

    /// Moves the files of tools that an older pocket-ui installed into ROCKNIX's Tools folder (`ROCKNIX_MODULES`,
    /// emptied at every boot) into `tools_dir`, and records the new places. A tool whose files ROCKNIX has deleted
    /// already is taken off the list, so the Store offers Install again instead of claiming it is there. Does nothing
    /// (no file is read or written) when no package lists a file there. Returns one line per package it changed.
    pub fn migrate_tools(&mut self) -> Result<Vec<String>, String> {
        self.migrate_tools_from(Path::new(ROCKNIX_MODULES))
    }

    /// `migrate_tools` with ROCKNIX's folder at `modules` (tests).
    pub fn migrate_tools_from(&mut self, modules: &Path) -> Result<Vec<String>, String> {
        let affected: Vec<String> = self.installed.iter().filter(|(_, i)| i.files.iter().chain(&i.dirs).any(|f| f.starts_with(modules))).map(|(id, _)| id.clone()).collect();
        if affected.is_empty() {
            return Ok(Vec::new());
        }
        let _changing = Changing::start();
        self.usable()?;
        let tools = tools_dir(&self.data);
        let mut said = Vec::new();
        for id in affected {
            let Some(old) = self.installed.get(&id).cloned() else { continue };
            let (mut files, mut lost) = (Vec::new(), 0usize);
            for f in &old.files {
                let Ok(rel) = f.strip_prefix(modules) else {
                    files.push(f.clone());
                    continue;
                };
                let target = tools.join(rel);
                if std::fs::symlink_metadata(f).is_ok_and(|m| m.is_file()) {
                    if let Some(parent) = target.parent() {
                        std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
                    }
                    move_file(f, &target).map_err(|e| format!("{}: {e}", f.display()))?;
                    files.push(target);
                } else if std::fs::symlink_metadata(&target).is_ok_and(|m| m.is_file()) {
                    // Moved by an earlier run that stopped before the list was saved.
                    files.push(target);
                } else {
                    lost += 1;
                }
            }
            // Folders of the old place are removed once empty; the new place's are the package's from now on.
            for d in old.dirs.iter().rev().filter(|d| d.starts_with(modules)) {
                let _ = std::fs::remove_dir(d);
            }
            let dirs: Vec<PathBuf> = old
                .dirs
                .iter()
                .map(|d| d.strip_prefix(modules).map_or_else(|_| d.clone(), |rel| tools.join(rel)))
                .filter(|d| !d.starts_with(&tools) || d.is_dir())
                .collect();
            if files.is_empty() {
                said.push(format!("{id}: its files were gone (ROCKNIX empties its Tools folder at boot); no longer listed as installed"));
                self.installed.remove(&id);
            } else {
                said.push(format!("{id}: moved to {}{}", tools.display(), if lost > 0 { format!(" ({lost} file(s) were gone already)") } else { String::new() }));
                self.installed.insert(id, Installed { files, dirs, ..old });
            }
        }
        self.save()?;
        Ok(said)
    }

    /// Refuses `targets` if another package installed any of them: it would lose its file now and
    /// delete ours when it goes.
    fn check_owner(&self, id: &str, targets: &[PathBuf]) -> Result<(), String> {
        for (other, i) in self.installed.iter().filter(|(other, _)| *other != id) {
            if let Some(f) = targets.iter().find(|t| i.files.contains(t)) {
                let name = f.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned());
                return Err(format!("{name} belongs to {other}, which is installed. Nothing was installed."));
            }
        }
        Ok(())
    }

    /// Removes exactly the files the package installed, and its folders once they are empty.
    pub fn uninstall(&mut self, id: &str) -> Result<(), String> {
        let _changing = Changing::start();
        self.usable()?;
        if let Some(i) = self.installed.remove(id) {
            // An inventory from before the owner check may list a file under two packages.
            for f in i.files.iter().filter(|f| !self.installed.values().any(|o| o.files.contains(f))) {
                let _ = std::fs::remove_file(f);
            }
            // Only empty ones: anything the user put there stays.
            for d in i.dirs.iter().rev() {
                let _ = std::fs::remove_dir(d);
            }
        }
        self.save()
    }
}

/// `Store::migrate_tools` for `data_dir`, logged; on a thread at start and before Settings → Tools lists the tools.
/// One at a time, so the start's run and the Tools page's never move the same file twice.
pub fn migrate_tools_in(data_dir: &Path) {
    static ONE_AT_A_TIME: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _one = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    match Store::open(data_dir).migrate_tools() {
        Ok(lines) => lines.iter().for_each(|l| println!("store: {l}")),
        Err(e) => eprintln!("store: tools could not be moved out of ROCKNIX's Tools folder: {e}"),
    }
}

/// Unpacks `p` into `staging` (new, 0700): a tar through untar.rs (only plain files and folders,
/// no setuid bits, B4), a single file as it is. Returns what is there.
fn stage(p: &Package, data: &[u8], staging: &Path) -> Result<Vec<crate::untar::Entry>, String> {
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
    match p.kind {
        Kind::Tar => crate::untar::unpack(data, staging),
        Kind::File => {
            let name = p.name_on_disk.clone().unwrap_or_else(|| p.id.clone());
            if name.is_empty() || name.contains('/') || name == "." || name == ".." {
                return Err("The package contains unsafe paths. Nothing was installed.".into());
            }
            std::fs::DirBuilder::new().mode(0o700).create(staging).map_err(|e| e.to_string())?;
            let file = staging.join(&name);
            std::fs::write(&file, data).map_err(|e| e.to_string())?;
            std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).map_err(|e| e.to_string())?;
            Ok(vec![crate::untar::Entry { path: name, dir: false }])
        }
    }
}

/// The folders below `dest` that `list` needs, parents first.
fn folders_for(dest: &Path, list: &[crate::untar::Entry]) -> Vec<PathBuf> {
    let mut folders: Vec<PathBuf> = Vec::new();
    for e in list {
        let target = dest.join(&e.path);
        let mut d = if e.dir { Some(target.as_path()) } else { target.parent() };
        while let Some(dir) = d.filter(|d| *d != dest && d.starts_with(dest)) {
            if !folders.iter().any(|f| f == dir) {
                folders.push(dir.to_path_buf());
            }
            d = dir.parent();
        }
    }
    folders.sort_by_key(|d| d.components().count());
    folders
}

/// Renames, or copies when `to` is on another file system.
fn move_file(from: &Path, to: &Path) -> std::io::Result<()> {
    if std::fs::rename(from, to).is_ok() {
        return Ok(());
    }
    if std::fs::symlink_metadata(to).is_ok_and(|m| !m.is_file()) {
        return Err(std::io::Error::other("not a file"));
    }
    std::fs::copy(from, to)?;
    std::fs::remove_file(from)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Status {
    NotInstalled,
    Installed,
    UpdateAvailable,
}

/// `url` as given when absolute, otherwise next to `catalog_url`.
pub fn resolve(catalog_url: &str, url: &str) -> String {
    if url.contains("://") || catalog_url.is_empty() {
        url.to_string()
    } else {
        let base = catalog_url.rsplit_once('/').map_or(catalog_url, |(b, _)| b);
        format!("{base}/{url}")
    }
}

fn download(url: &str) -> Result<Vec<u8>, String> {
    let out = Command::new("curl").args(["-fsSL", "--max-time", "60", url]).output().map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(out.stdout)
    } else {
        Err(crate::net_check::refine("Couldn't reach the store. Check Wi-Fi."))
    }
}

/// The longest a package download may take in all, whatever its speed (a safety net; curl's own limits come first).
const DOWNLOAD_AT_MOST: Duration = Duration::from_secs(30 * 60);

/// curl's arguments for a package download: given up when it moves less than 10 kB/s for 30 s (a server that keeps the
/// connection open without sending, review N5) or grows past `max` bytes, the size the signed catalog announced.
fn download_args(url: &str, dest: &Path, max: Option<u64>) -> Vec<std::ffi::OsString> {
    let mut args: Vec<std::ffi::OsString> = ["-fsSL", "--retry", "2", "--connect-timeout", "15", "--speed-limit", "10240", "--speed-time", "30"]
        .iter()
        .map(Into::into)
        .collect();
    if let Some(max) = max.filter(|m| *m > 0) {
        args.push("--max-filesize".into());
        args.push(max.to_string().into());
    }
    args.push("-o".into());
    args.push(dest.into());
    args.push(url.into());
    args
}

/// Why a download did not finish: the network is not there (curl could not look up or reach the host), or anything
/// else, with the words for it. The starter pack tells the first calmly as "offline" (`starter::Failure::Offline`).
#[derive(Debug, PartialEq)]
pub enum DownloadFailure {
    Unreachable,
    Broke(String),
}

/// curl's exit codes for "could not resolve the host" and "could not connect".
fn unreachable_code(code: Option<i32>) -> bool {
    matches!(code, Some(6 | 7))
}

/// Downloads `url` into `dest` with curl, reporting the bytes so far every quarter of a second. `max`: the size the
/// catalog gives for it.
fn download_to(url: &str, dest: &Path, max: Option<u64>, progress: &dyn Fn(u64)) -> Result<(), String> {
    download_with_cause(url, dest, max, progress).map_err(|f| match f {
        DownloadFailure::Unreachable => crate::net_check::refine("The download didn't finish. Check Wi-Fi and try again."),
        DownloadFailure::Broke(m) if m == "The download didn't finish. Check Wi-Fi and try again." => crate::net_check::refine(&m),
        DownloadFailure::Broke(m) => m,
    })
}

/// `download_to` with the cause of a failure told apart (`DownloadFailure`); the starter pack uses it, so a missing
/// network is not reported as a broken download. Same curl, same limits.
pub fn download_with_cause(url: &str, dest: &Path, max: Option<u64>, progress: &dyn Fn(u64)) -> Result<(), DownloadFailure> {
    let broke = |e: std::io::Error| DownloadFailure::Broke(e.to_string());
    let _ = std::fs::remove_file(dest);
    let mut child = Command::new("curl")
        .args(download_args(url, dest, max))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(broke)?;
    let end = std::time::Instant::now() + DOWNLOAD_AT_MOST;
    loop {
        let status = child.try_wait().map_err(broke)?;
        progress(std::fs::metadata(dest).map_or(0, |m| m.len()));
        match status {
            Some(s) if s.success() => return Ok(()),
            Some(s) if unreachable_code(s.code()) => return Err(DownloadFailure::Unreachable),
            Some(_) => return Err(DownloadFailure::Broke("The download didn't finish. Check Wi-Fi and try again.".into())),
            None if std::time::Instant::now() > end => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(DownloadFailure::Broke("The download took too long. Check Wi-Fi and try again.".into()));
            }
            None => std::thread::sleep(Duration::from_millis(250)),
        }
    }
}

/// Packages whose files, or the list of installed ones, are being changed right now (`install_bytes`, `uninstall`).
static CHANGING: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Whether a package is being installed, updated or removed this moment: a power-off waits for it (`sleep::Sleep`,
/// review N5), so the files moved and `installed.json` don't part ways. A download is not counted: it is a
/// temporary file, dropped by the next try.
pub fn changing() -> bool {
    CHANGING.load(std::sync::atomic::Ordering::SeqCst) > 0
}

/// Counts one change in `CHANGING` while it lives.
struct Changing;

impl Changing {
    fn start() -> Changing {
        CHANGING.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Changing
    }
}

impl Drop for Changing {
    fn drop(&mut self) {
        CHANGING.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
    }
}

/// Fetches and verifies the catalog from `url` (with `url.sig` next to it), and checks that it is
/// neither expired nor older than one already seen (`check_fresh`).
pub fn fetch_catalog(url: &str, data_dir: &Path) -> Result<Catalog, String> {
    if url.trim().is_empty() {
        return Err("There is no store yet.".into());
    }
    let json = download(url)?;
    let sig = download(&format!("{url}.sig"))?;
    let cat = verify_catalog(&json, &String::from_utf8_lossy(&sig), &PUBLIC_KEY).map_err(|e| {
        if e.starts_with("catalog signature") {
            "The store list's signature doesn't match, so it wasn't used. Nothing was changed.".to_string()
        } else {
            format!("The store list can't be read ({e}).")
        }
    })?;
    check_fresh(&cat, url, data_dir, unix_now())?;
    Ok(cat)
}

/// Refuses a catalog past its `expires`, or with a lower `serial` than the newest one seen from
/// `url` (a replayed or stale copy, B5); remembers the serial of one it accepts in
/// `<data>/store/catalog-serials.json`. A catalog without a serial counts as 0, so the old format
/// works until a store starts numbering its catalogs.
pub fn check_fresh(cat: &Catalog, url: &str, data_dir: &Path, now: u64) -> Result<(), String> {
    if cat.expires.is_some_and(|t| now > t) {
        return Err("The store list has expired, so it wasn't used. Nothing was changed.".into());
    }
    let path = data_dir.join("store").join("catalog-serials.json");
    // Written atomically, so damage is unlikely; read as empty, it only loses the replay guard.
    let mut seen: BTreeMap<String, u64> = std::fs::read(&path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
    let serial = cat.serial.unwrap_or(0);
    let last = seen.get(url).copied();
    if last.is_some_and(|last| serial < last) {
        return Err("The store list is older than one this device already had, so it wasn't used. Nothing was changed.".into());
    }
    if last.map_or(serial > 0, |last| serial > last) {
        seen.insert(url.to_string(), serial);
        let written = std::fs::create_dir_all(data_dir.join("store"))
            .and_then(|_| crate::fsutil::write_atomic(&path, &serde_json::to_vec_pretty(&seen).unwrap_or_default()));
        if let Err(e) = written {
            eprintln!("store: can't remember catalog serial {serial}: {e}");
        }
    }
    Ok(())
}

fn unix_now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

#[cfg(test)]
mod tests {
    #[test]
    fn without_a_store_url_there_is_no_catalog() {
        // Nothing is hosted yet: the built-in address is empty, so the Store stays hidden unless settings.json has one.
        assert_eq!(DEFAULT_URL, "", "the placeholder is gone: no address, no Store row");
        assert_eq!(catalog_url(None), None);
        assert_eq!(catalog_url(Some("")), None);
        assert_eq!(catalog_url(Some("  ")), None);
        assert_eq!(catalog_url(Some(" file:///s/catalog.json ")).as_deref(), Some("file:///s/catalog.json"));
        // Asked anyway (a snapshot, an old link): a plain sentence, no network.
        let dir = TestDir::new("store-nostore");
        assert_eq!(fetch_catalog("", &dir).unwrap_err(), "There is no store yet.");
    }

    use super::*;
    use crate::testutil::TestDir;

    #[test]
    fn a_download_that_stalls_or_grows_past_its_size_is_given_up() {
        // Review N5: the store's curl had no transfer limit, and the loop around it none either: a server that kept the
        // connection open without sending left "Downloading" up until pocket-ui ended.
        let args: Vec<String> = download_args("https://x/p.tar.gz", Path::new("/tmp/d"), Some(5000)).iter().map(|a| a.to_string_lossy().into_owned()).collect();
        let pair = |k: &str, v: &str| args.windows(2).any(|w| w[0] == k && w[1] == v);
        assert!(pair("--speed-limit", "10240") && pair("--speed-time", "30") && pair("--connect-timeout", "15"), "{args:?}");
        assert!(pair("--max-filesize", "5000"), "{args:?}");
        let unknown: Vec<String> = download_args("https://x/p", Path::new("/tmp/d"), None).iter().map(|a| a.to_string_lossy().into_owned()).collect();
        assert!(!unknown.iter().any(|a| a == "--max-filesize"));
    }

    #[test]
    fn a_power_off_can_see_a_package_being_changed() {
        // Other tests install meanwhile, so only "on while held" is certain here.
        let guard = Changing::start();
        assert!(changing());
        let second = Changing::start();
        drop(guard);
        assert!(changing(), "counted, not a flag: the second change is still under way");
        drop(second);
    }
    use ed25519_dalek::{Signer, SigningKey};

    fn key() -> SigningKey {
        SigningKey::from_bytes(&[7u8; 32])
    }

    fn package(data: &[u8], dest: &Path) -> Package {
        Package {
            id: "test-core".into(),
            name: "Test core".into(),
            category: "cores".into(),
            version: "1.0".into(),
            description: "A test.".into(),
            size: data.len() as u64,
            url: String::new(),
            sha256: sha256_hex(data),
            kind: Kind::File,
            dest: dest.to_string_lossy().into(),
            name_on_disk: Some("test_libretro.so".into()),
            author: String::new(),
            license: String::new(),
            run: None,
        }
    }

    #[test]
    fn rejects_unsigned_or_tampered_catalogs() {
        let json = br#"{"version":1,"packages":[]}"#;
        let sig = base64::engine::general_purpose::STANDARD.encode(key().sign(json).to_bytes());
        let public = key().verifying_key().to_bytes();
        assert!(verify_catalog(json, &sig, &public).is_ok());
        assert!(verify_catalog(br#"{"version":2,"packages":[]}"#, &sig, &public).is_err(), "changed bytes");
        assert!(verify_catalog(json, &sig, &SigningKey::from_bytes(&[8u8; 32]).verifying_key().to_bytes()).is_err(), "other key");
    }

    #[test]
    fn checks_hash_installs_and_removes_exactly_its_files() {
        let root = TestDir::new("store");
        let dest = root.join("cores");
        let mut store = Store::open(&root);
        let data = b"core bytes";
        let mut p = package(data, &dest);

        p.sha256 = sha256_hex(b"something else");
        assert!(store.install_bytes(&p, data).is_err());
        assert!(!dest.join("test_libretro.so").exists());

        p.sha256 = sha256_hex(data);
        store.install_bytes(&p, data).unwrap();
        assert_eq!(std::fs::read(dest.join("test_libretro.so")).unwrap(), data);
        assert_eq!(Store::open(&root).status(&p), Status::Installed);
        p.version = "1.1".into();
        assert_eq!(Store::open(&root).status(&p), Status::UpdateAvailable);

        store.uninstall("test-core").unwrap();
        assert!(!dest.join("test_libretro.so").exists());
        assert_eq!(Store::open(&root).status(&p), Status::NotInstalled);
    }

    fn tool(data: &[u8], dest: &str) -> Package {
        Package {
            id: "tool-x".into(),
            name: "Storage report".into(),
            category: "tools".into(),
            dest: dest.into(),
            name_on_disk: Some("Storage report.sh".into()),
            run: Some("Storage report.sh".into()),
            ..package(data, Path::new("/x"))
        }
    }

    /// ROCKNIX's `001-sync-modules` empties /storage/.config/modules at every boot (rsync --delete), so tools go to
    /// pocket-ui's own folder whatever the catalog says, and the inventory keeps their name and script for Tools.
    #[test]
    fn tools_go_to_pocket_uis_own_folder_not_rocknixs() {
        let root = TestDir::new("store-tools");
        assert_eq!(tool(b"x", ROCKNIX_MODULES).install_dir(&root), root.join("tools"));
        assert_eq!(tool(b"x", "/storage/.config/modules/").install_dir(&root), root.join("tools"));
        assert_eq!(Package { category: "other".into(), ..tool(b"x", ROCKNIX_MODULES) }.install_dir(&root), root.join("tools"), "aimed at ROCKNIX's folder");
        assert_eq!(package(b"x", Path::new("/storage/cores")).install_dir(&root), PathBuf::from("/storage/cores"));

        let data = b"#!/bin/sh\necho 1.0\n";
        let mut store = Store::open(&root);
        store.install_bytes(&tool(data, ROCKNIX_MODULES), data).unwrap();
        assert_eq!(std::fs::read(root.join("tools/Storage report.sh")).unwrap(), data);
        let i = &Store::open(&root).installed["tool-x"];
        assert_eq!(i.name.as_deref(), Some("Storage report"));
        assert_eq!(i.run.as_deref(), Some("Storage report.sh"));
        assert_eq!(i.files, [root.join("tools/Storage report.sh")]);
        store.uninstall("tool-x").unwrap();
        assert!(!root.join("tools/Storage report.sh").exists());
    }

    /// A tool an older pocket-ui put into ROCKNIX's folder: moved over while it is still there, forgotten once ROCKNIX
    /// has deleted it, so the Store offers Install instead of claiming it is installed.
    #[test]
    fn tools_in_rocknixs_folder_are_moved_or_forgotten() {
        let root = TestDir::new("store-migrate");
        let modules = root.join("modules");
        std::fs::create_dir_all(modules.join("sub")).unwrap();
        std::fs::write(modules.join("a.sh"), b"a").unwrap();
        std::fs::write(modules.join("sub/b.sh"), b"b").unwrap();
        std::fs::write(modules.join("rocknix.sh"), b"ROCKNIX's own").unwrap();
        let old = |files: Vec<PathBuf>, dirs: Vec<PathBuf>| Installed { version: "1.0".into(), files, dirs, name: None, run: None };
        let mut store = Store::open(&root);
        store.installed.insert("tool-a".into(), old(vec![modules.join("a.sh"), modules.join("sub/b.sh")], vec![modules.join("sub")]));
        store.installed.insert("tool-gone".into(), old(vec![modules.join("gone.sh")], vec![]));
        store.installed.insert("core".into(), old(vec![root.join("cores/x.so")], vec![]));
        store.save().unwrap();

        let said = Store::open(&root).migrate_tools_from(&modules).unwrap();
        assert_eq!(said.len(), 2, "{said:?}");
        let after = Store::open(&root);
        assert_eq!(after.installed["tool-a"].files, [root.join("tools/a.sh"), root.join("tools/sub/b.sh")]);
        assert_eq!(after.installed["tool-a"].dirs, [root.join("tools/sub")]);
        assert_eq!(std::fs::read(root.join("tools/sub/b.sh")).unwrap(), b"b");
        assert!(!modules.join("a.sh").exists() && !modules.join("sub").exists());
        assert!(modules.join("rocknix.sh").exists(), "ROCKNIX's own files stay");
        assert!(!after.installed.contains_key("tool-gone"), "deleted by ROCKNIX: not installed any more");
        assert_eq!(after.installed["core"].files, [root.join("cores/x.so")], "other packages untouched");
        assert!(Store::open(&root).migrate_tools_from(&modules).unwrap().is_empty(), "nothing to do the second time");
    }

    #[test]
    fn relative_urls_sit_next_to_the_catalog() {
        assert_eq!(resolve("file:///storage/cat/catalog.json", "packages/a.tar.gz"), "file:///storage/cat/packages/a.tar.gz");
        assert_eq!(resolve("https://x/catalog.json", "https://y/a.so"), "https://y/a.so");
        assert_eq!(resolve("", "https://y/a.so"), "https://y/a.so");
    }

    /// A tar package fetched over file://, as the test catalog is: its folder goes with it, a
    /// folder the user already had stays.
    #[test]
    fn tar_package_over_file_url_installs_and_leaves_no_folders() {
        let root = TestDir::new("store-tar");
        let src = root.join("src");
        std::fs::create_dir_all(src.join("night-pack")).unwrap();
        std::fs::write(src.join("night-pack/pack.json"), b"{}").unwrap();
        std::fs::write(src.join("night-pack/gba.png"), b"png").unwrap();
        let tar = root.join("night.tar.gz");
        assert!(Command::new("tar").env("COPYFILE_DISABLE", "1").arg("-czf").arg(&tar).arg("-C").arg(&src).arg("night-pack").status().unwrap().success());
        let data = std::fs::read(&tar).unwrap();
        let dest = root.join("frames");
        std::fs::create_dir_all(&dest).unwrap();
        let mut p = package(&data, &dest);
        p.kind = Kind::Tar;
        p.url = "night.tar.gz".into();
        let catalog = format!("file://{}/catalog.json", root.display());

        let mut store = Store::open(&root.join("data"));
        let seen = std::cell::Cell::new(0);
        store.install_from(&p, &catalog, &|n| seen.set(n)).unwrap();
        assert_eq!(seen.get(), data.len() as u64, "progress ends at the full size");
        assert!(dest.join("night-pack/pack.json").is_file());
        store.uninstall(&p.id).unwrap();
        assert!(!dest.join("night-pack").exists(), "the pack's folder goes with it");
        assert!(dest.is_dir(), "the frames folder stays");

        p.url = "missing.tar.gz".into();
        let err = store.install_from(&p, &catalog, &|_| {}).unwrap_err();
        assert!(err.contains("Check Wi-Fi"), "{err}");
    }

    /// R2.3: a package may not overwrite, nor later delete, a file another package installed.
    #[test]
    fn packages_cannot_take_over_each_others_files() {
        let root = TestDir::new("store-own");
        let dest = root.join("cores");
        let mut store = Store::open(&root.join("data"));
        let mut a = package(b"core a", &dest);
        a.id = "core-a".into();
        store.install_bytes(&a, b"core a").unwrap();

        // Same file name, other package: refused, a's file untouched.
        let mut b = package(b"core b", &dest);
        b.id = "core-b".into();
        let err = store.install_bytes(&b, b"core b").unwrap_err();
        assert!(err.contains("core-a"), "{err}");
        assert_eq!(std::fs::read(dest.join("test_libretro.so")).unwrap(), b"core a");
        assert!(!store.installed.contains_key("core-b"));

        // The same through a tar package.
        let src = root.join("src");
        std::fs::create_dir_all(src.join("t")).unwrap();
        std::fs::write(src.join("t/test_libretro.so"), b"core t").unwrap();
        let out = src.join("t.tar.gz");
        assert!(Command::new("tar").env("COPYFILE_DISABLE", "1").arg("-czf").arg(&out).arg("-C").arg(src.join("t")).arg(".").status().unwrap().success());
        let data = std::fs::read(out).unwrap();
        let mut t = package(&data, &dest);
        t.id = "core-t".into();
        t.kind = Kind::Tar;
        assert!(store.install_bytes(&t, &data).unwrap_err().contains("core-a"));
        assert_eq!(std::fs::read(dest.join("test_libretro.so")).unwrap(), b"core a");

        // An inventory from before this check may list a file twice: removing one keeps it.
        store.installed.insert("core-old".into(), Installed { version: "1".into(), files: vec![dest.join("test_libretro.so")], ..Default::default() });
        store.uninstall("core-old").unwrap();
        assert_eq!(std::fs::read(dest.join("test_libretro.so")).unwrap(), b"core a");
        store.uninstall("core-a").unwrap();
        assert!(!dest.join("test_libretro.so").exists());

        // dest has to be a plain absolute folder.
        let mut odd = package(b"x", &root.join("cores/../elsewhere"));
        odd.id = "odd".into();
        assert!(store.install_bytes(&odd, b"x").is_err());
        odd.dest = "relative/cores".into();
        assert!(store.install_bytes(&odd, b"x").is_err());
    }

    /// R2.3: a damaged installed.json is not an empty inventory. Read as empty, every package would
    /// look new and its files would be orphaned or overwritten.
    #[test]
    fn damaged_inventory_is_kept_and_the_store_refuses_to_act() {
        let root = TestDir::new("store-inv");
        let dest = root.join("cores");
        let data_dir = root.join("data");
        let list = data_dir.join("store/installed.json");
        let mut store = Store::open(&data_dir);
        let mut a = package(b"core a", &dest);
        a.id = "core-a".into();
        store.install_bytes(&a, b"core a").unwrap();
        let mut b = package(b"core b", &dest);
        b.id = "core-b".into();
        b.name_on_disk = Some("b_libretro.so".into());
        store.install_bytes(&b, b"core b").unwrap();
        let good = std::fs::read(&list).unwrap();
        assert!(Store::open(&data_dir).installed.contains_key("core-b"));
        let bak = std::fs::read(data_dir.join("store/installed.json.bak")).expect("the list before the last change is kept");
        assert!(serde_json::from_slice::<BTreeMap<String, Installed>>(&bak).unwrap().contains_key("core-a"));

        // Cut off, as a power loss during a plain write leaves it.
        std::fs::write(&list, &good[..good.len() / 2]).unwrap();
        let mut store = Store::open(&data_dir);
        assert!(store.damaged().is_some());
        let mut c = package(b"core c", &dest);
        c.id = "core-c".into();
        c.name_on_disk = Some("c_libretro.so".into());
        assert!(store.install_bytes(&c, b"core c").is_err(), "no install on a damaged list");
        assert!(!dest.join("c_libretro.so").exists());
        assert!(store.uninstall("core-a").is_err(), "no removal either");
        assert!(dest.join("test_libretro.so").exists());
        assert_eq!(std::fs::read(&list).unwrap(), &good[..good.len() / 2], "the damaged list stays for a look");
        assert!(std::fs::read(data_dir.join("store/installed.json.bak")).unwrap() == bak, "and so does the last good one");

        // Written whole or not at all: no temporary files stay behind.
        let mut store = Store::open(&root.join("data2"));
        store.install_bytes(&a, b"core a").unwrap();
        let names: Vec<String> = std::fs::read_dir(root.join("data2/store")).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        assert_eq!(names, vec!["installed.json".to_string()], "{names:?}");
    }

    /// An update is unpacked and checked before anything of the installed version goes; a refused
    /// update leaves it exactly as it was.
    #[test]
    fn update_is_checked_before_the_old_version_goes() {
        let root = TestDir::new("store-upd");
        let src = root.join("src");
        let dest = root.join("frames");
        let mut store = Store::open(&root.join("data"));
        let build = |name: &str, files: &[(&str, &str)], link: bool| -> Vec<u8> {
            let dir = src.join(name);
            for (f, text) in files {
                std::fs::create_dir_all(dir.join(f).parent().unwrap()).unwrap();
                std::fs::write(dir.join(f), text).unwrap();
            }
            if link {
                std::os::unix::fs::symlink("/etc", dir.join("pk/etc")).unwrap();
            }
            tar_gz(&dir, "pk")
        };
        let v1 = build("v1", &[("pk/a", "1"), ("pk/old/x", "1")], false);
        let mut p = package(&v1, &dest);
        p.kind = Kind::Tar;
        store.install_bytes(&p, &v1).unwrap();

        // Refused for a link: version 1 is untouched.
        let bad = build("bad", &[("pk/a", "2")], true);
        p.version = "2.0".into();
        p.sha256 = sha256_hex(&bad);
        assert!(store.install_bytes(&p, &bad).is_err());
        assert_eq!(std::fs::read_to_string(dest.join("pk/a")).unwrap(), "1");
        assert!(dest.join("pk/old/x").is_file());
        assert_eq!(Store::open(&root.join("data")).installed["test-core"].version, "1.0");

        // Refused for a file another package owns: the same.
        let mut other = package(b"o", &dest.join("pk"));
        other.id = "other".into();
        other.name_on_disk = Some("taken".into());
        store.install_bytes(&other, b"o").unwrap();
        let clash = build("clash", &[("pk/a", "2"), ("pk/taken", "2")], false);
        p.sha256 = sha256_hex(&clash);
        assert!(store.install_bytes(&p, &clash).is_err());
        assert_eq!(std::fs::read_to_string(dest.join("pk/a")).unwrap(), "1");
        assert!(dest.join("pk/old/x").is_file());
        assert_eq!(std::fs::read_to_string(dest.join("pk/taken")).unwrap(), "o");

        // A good update: new files in, files it no longer has out, with their folder.
        let v2 = build("v2", &[("pk/a", "2"), ("pk/new/y", "2")], false);
        p.sha256 = sha256_hex(&v2);
        store.install_bytes(&p, &v2).unwrap();
        assert_eq!(std::fs::read_to_string(dest.join("pk/a")).unwrap(), "2");
        assert!(dest.join("pk/new/y").is_file());
        assert!(!dest.join("pk/old").exists(), "folder of the old version goes when empty");
        assert_eq!(std::fs::read_to_string(dest.join("pk/taken")).unwrap(), "o");
        let inv = &Store::open(&root.join("data")).installed["test-core"];
        assert_eq!(inv.version, "2.0");
        assert_eq!(inv.files.len(), 2);
        store.uninstall("test-core").unwrap();
        assert_eq!(std::fs::read_dir(dest.join("pk")).unwrap().count(), 1, "only the other package's file is left");
    }

    /// B5: an older catalog (replayed, or from a stale mirror) offers no "update" back to an older
    /// version, and installing one over a newer version is refused.
    #[test]
    fn older_versions_are_not_updates() {
        let root = TestDir::new("store-b5");
        let dest = root.join("cores");
        let mut store = Store::open(&root.join("data"));
        let mut p = package(b"v2", &dest);
        p.version = "2.0".into();
        store.install_bytes(&p, b"v2").unwrap();
        let mut old = package(b"v1", &dest);
        old.version = "1.0".into();
        assert_eq!(store.status(&old), Status::Installed, "1.0 is no update for 2.0");
        assert!(store.install_bytes(&old, b"v1").is_err(), "and isn't installed over it");
        assert_eq!(std::fs::read(dest.join("test_libretro.so")).unwrap(), b"v2");
        let mut new = package(b"v10", &dest);
        new.version = "10.0".into();
        assert_eq!(store.status(&new), Status::UpdateAvailable, "versions compare by number");
        store.install_bytes(&new, b"v10").unwrap();
    }

    /// B5: a signed catalog with a lower serial than one already seen from that URL, or past its
    /// `expires`, is refused. Catalogs without either (the old format) still work.
    #[test]
    fn stale_or_expired_catalogs_are_refused() {
        let root = TestDir::new("store-serial");
        let url = "https://store.example/catalog.json";
        let cat = |json: &str| -> Catalog {
            let sig = base64::engine::general_purpose::STANDARD.encode(key().sign(json.as_bytes()).to_bytes());
            verify_catalog(json.as_bytes(), &sig, &key().verifying_key().to_bytes()).unwrap()
        };
        let now = 1_800_000_000;
        let plain = cat(r#"{"version":1,"packages":[]}"#);
        assert!(check_fresh(&plain, url, &root, now).is_ok(), "the old format still works");
        let s5 = cat(r#"{"version":1,"serial":5,"expires":1800000100,"packages":[]}"#);
        assert!(check_fresh(&s5, url, &root, now).is_ok());
        assert!(check_fresh(&s5, url, &root, now).is_ok(), "the same catalog again");
        let s4 = cat(r#"{"version":1,"serial":4,"packages":[]}"#);
        assert!(check_fresh(&s4, url, &root, now).is_err(), "older than one seen");
        assert!(check_fresh(&plain, url, &root, now).is_err(), "no serial counts as 0 once one was seen");
        assert!(check_fresh(&s4, "file:///storage/other/catalog.json", &root, now).is_ok(), "serials are per store");
        assert!(check_fresh(&s5, url, &root, 1_800_000_101).is_err(), "expired");
        let s6 = cat(r#"{"version":1,"serial":6,"packages":[]}"#);
        assert!(check_fresh(&s6, url, &root, now).is_ok());
        assert!(check_fresh(&s5, url, &root, now).is_err(), "the newest serial is remembered");
    }

    /// store/test-catalog as scripts/make-test-catalog.sh writes it: signed with our key, numbered,
    /// every package matching its entry and passing the unpack checks.
    #[test]
    fn test_catalog_is_signed_numbered_and_complete() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("store/test-catalog");
        let json = std::fs::read(dir.join("catalog.json")).unwrap();
        let sig = std::fs::read_to_string(dir.join("catalog.json.sig")).unwrap();
        let cat = verify_catalog(&json, &sig, &PUBLIC_KEY).unwrap();
        assert!(cat.serial.is_some_and(|s| s > 0));
        let state = TestDir::new("store-testcat");
        assert!(check_fresh(&cat, "file:///storage/test-catalog/catalog.json", &state, unix_now()).is_ok());
        for p in &cat.packages {
            let data = std::fs::read(dir.join(&p.url)).unwrap();
            assert_eq!((sha256_hex(&data), data.len() as u64), (p.sha256.clone(), p.size), "{}", p.id);
            if p.kind == Kind::Tar {
                assert!(crate::untar::entries(&crate::untar::decompress(&data).unwrap()).is_ok(), "{}", p.id);
            }
        }
    }

    /// A .tar.gz of `top` inside `src`, made by the system tar as a publisher would.
    fn tar_gz(src: &Path, top: &str) -> Vec<u8> {
        let out = src.join(format!("{top}.tar.gz"));
        assert!(Command::new("tar").env("COPYFILE_DISABLE", "1").arg("-czf").arg(&out).arg("-C").arg(src).arg(top).status().unwrap().success());
        std::fs::read(out).unwrap()
    }

    fn mode(p: &Path) -> u32 {
        use std::os::unix::fs::PermissionsExt;
        std::fs::symlink_metadata(p).unwrap().permissions().mode() & 0o7777
    }

    fn set_mode(p: &Path, mode: u32) {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(p, std::fs::Permissions::from_mode(mode)).unwrap();
    }

    /// B4: packages are unpacked as root, so links could reach outside `dest`, and a setuid file
    /// would give root to anyone who can start it.
    #[test]
    fn tar_packages_with_links_are_refused_and_modes_normalised() {
        let root = TestDir::new("store-b4");
        let src = root.join("src");
        let dest = root.join("dest");
        std::fs::create_dir_all(&dest).unwrap();
        let mut store = Store::open(&root.join("data"));

        let sym = src.join("sym");
        std::fs::create_dir_all(sym.join("pk")).unwrap();
        std::fs::write(sym.join("pk/a.txt"), b"a").unwrap();
        std::os::unix::fs::symlink("/etc/passwd", sym.join("pk/passwd")).unwrap();
        let hard = src.join("hard");
        std::fs::create_dir_all(hard.join("pk")).unwrap();
        std::fs::write(hard.join("pk/a.txt"), b"a").unwrap();
        std::fs::hard_link(hard.join("pk/a.txt"), hard.join("pk/b.txt")).unwrap();
        for (dir, what) in [(&sym, "symlink"), (&hard, "hardlink")] {
            let data = tar_gz(dir, "pk");
            let mut p = package(&data, &dest);
            p.kind = Kind::Tar;
            assert!(store.install_bytes(&p, &data).is_err(), "a package with a {what} is refused");
            assert!(!dest.join("pk").exists(), "nothing of it lands in dest ({what})");
            assert!(store.installed.is_empty());
        }

        let modes = src.join("modes");
        std::fs::create_dir_all(modes.join("pk/bin")).unwrap();
        std::fs::write(modes.join("pk/bin/run"), b"#!/bin/sh\n").unwrap();
        set_mode(&modes.join("pk/bin/run"), 0o4755);
        std::fs::write(modes.join("pk/secret"), b"s").unwrap();
        set_mode(&modes.join("pk/secret"), 0o2600);
        set_mode(&modes.join("pk/bin"), 0o2777);
        let data = tar_gz(&modes, "pk");
        let mut p = package(&data, &dest);
        p.kind = Kind::Tar;
        store.install_bytes(&p, &data).unwrap();
        assert_eq!(mode(&dest.join("pk/bin/run")), 0o755, "executable: 0755, no setuid");
        assert_eq!(mode(&dest.join("pk/secret")), 0o644, "anything else: 0644, no setgid");
        assert_eq!(mode(&dest.join("pk/bin")), 0o755, "folders: 0755");
        assert_eq!(mode(&dest.join("pk")), 0o755);
    }
}
