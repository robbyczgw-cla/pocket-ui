//! Settings → Backup & reset: pocket-ui's settings and library notes copied to a dated folder on the card, put back
//! from one, and pocket-ui's settings back to how they start (after an automatic backup).
//!
//! Only pocket-ui's own files, by name (`SETTINGS_FILES`, `SERVICE_FILES`, `LIBRARY_FILES`, `SETTINGS_DIRS`): never the
//! games, saves or states, never a file of ROCKNIX (system.cfg included), never secrets (the file-sharing password in
//! `share/`, the RetroAchievements token), never what the Store installed (`store/` must match the files on the card)
//! nor the notes pocket-ui uses to undo CPU and fan changes (`restore.json` and friends), which describe the running
//! system and not a choice. A backup folder holds copies of those files and a `backup.json` listing them; a restore
//! reads only names of the list above, so a folder changed on the network share can't write anywhere else.
//! Every call reads and writes files: run them on a thread.

use std::path::{Path, PathBuf};

/// Where backups go: next to ROCKNIX's own (`/storage/roms/backup`), on the games part of the card, so they outlive
/// ROCKNIX's factory reset and can be copied to a computer over file sharing (password protected).
pub const ROOT: &str = "/storage/roms/backup/pocket-ui";
/// The list in every backup folder.
pub const MANIFEST: &str = "backup.json";

/// pocket-ui's settings: what a reset puts back to the start.
pub const SETTINGS_FILES: &[&str] = &["settings.json", "sleep.json", "sounds.json", "haptics.json", "leds.json", "performance.json", "curator.json"];
/// Settings per console and per game (Options in the pause menu): reset too.
pub const SETTINGS_DIRS: &[&str] = &["game-settings", "core-options", "cheats"];
/// Settings of services that keep their own state elsewhere (Syncthing's folders, the Tailscale name): backed up and
/// restored, but a reset leaves them, so it never switches a service off or renames the device.
pub const SERVICE_FILES: &[&str] = &["sync.json", "tailscale.json", "web.json"];
/// The library's notes: play status, notes, time played, stickers, favourites, recent games, emulator fallbacks,
/// paired PCs. Backed up and restored; a reset keeps them.
pub const LIBRARY_FILES: &[&str] = &["library.json", "favorites.json", "recents.json", "fallback.json", "moonlight-pcs.json"];

/// Why a backup was made.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Reason {
    /// Settings → Backup & reset → Back up now.
    Manual,
    /// Made by Reset settings before it reset anything.
    BeforeReset,
    /// Made by Restore before it put an older backup back.
    BeforeRestore,
}

impl Reason {
    pub fn label(self) -> &'static str {
        match self {
            Reason::Manual => "Backup",
            Reason::BeforeReset => "Before reset",
            Reason::BeforeRestore => "Before restore",
        }
    }
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Manifest {
    pub version: u32,
    /// Unix time in seconds.
    pub created: u64,
    pub reason: Reason,
    /// pocket-ui's version that made it.
    pub app: String,
    /// The files, relative to the data folder.
    pub files: Vec<String>,
}

/// A backup on the card.
#[derive(Clone, Debug, PartialEq)]
pub struct Backup {
    pub dir: PathBuf,
    pub manifest: Manifest,
    pub bytes: u64,
}

impl Backup {
    /// "23 Oct 2026, 14:20" in the device's time zone.
    pub fn when(&self) -> String {
        let t = local(self.manifest.created);
        const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
        format!("{} {} {}, {:02}:{:02}", t.2, MONTHS[(t.1 as usize).saturating_sub(1).min(11)], t.0, t.3, t.4)
    }
}

/// (year, month 1-12, day, hour, minute) of a Unix time in the device's time zone.
pub fn local(unix: u64) -> (i32, i32, i32, i32, i32) {
    let t = unix as libc::time_t;
    // SAFETY: localtime_r only writes into the struct handed in.
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    if unsafe { libc::localtime_r(&t, &mut tm) }.is_null() {
        return (1970, 1, 1, 0, 0);
    }
    (tm.tm_year + 1900, tm.tm_mon + 1, tm.tm_mday, tm.tm_hour, tm.tm_min)
}

/// A file name of the lists above, or a plain file under one of `SETTINGS_DIRS`: the only names a backup holds and
/// a restore writes.
pub fn allowed(rel: &str) -> bool {
    let p = Path::new(rel);
    let plain = !rel.is_empty() && p.components().all(|c| matches!(c, std::path::Component::Normal(_))) && !rel.contains('\\');
    if !plain {
        return false;
    }
    let mut parts = rel.split('/');
    let first = parts.next().unwrap_or("");
    match parts.next() {
        None => SETTINGS_FILES.contains(&first) || SERVICE_FILES.contains(&first) || LIBRARY_FILES.contains(&first),
        Some(_) => SETTINGS_DIRS.contains(&first),
    }
}

/// What of `data_dir` goes into a backup: the listed files that are there, and every plain file under the settings
/// folders (no links, nothing that isn't a regular file).
pub fn collect(data_dir: &Path) -> Vec<String> {
    let mut out: Vec<String> = SETTINGS_FILES
        .iter()
        .chain(SERVICE_FILES)
        .chain(LIBRARY_FILES)
        .filter(|f| std::fs::symlink_metadata(data_dir.join(f)).is_ok_and(|m| m.is_file()))
        .map(|f| f.to_string())
        .collect();
    for dir in SETTINGS_DIRS {
        let mut stack = vec![PathBuf::from(dir)];
        while let Some(rel) = stack.pop() {
            let Ok(rd) = std::fs::read_dir(data_dir.join(&rel)) else { continue };
            for e in rd.flatten() {
                let Ok(m) = e.path().symlink_metadata() else { continue };
                let child = rel.join(e.file_name());
                if m.is_dir() {
                    stack.push(child);
                } else if m.is_file() {
                    if let Some(s) = child.to_str().filter(|s| allowed(s)) {
                        out.push(s.to_string());
                    }
                }
            }
        }
    }
    out.sort();
    out
}

/// The folder name of a backup made at `unix`: "2026-10-23_1420", with "-2", "-3" when there is one already.
fn folder_name(root: &Path, unix: u64) -> PathBuf {
    let (y, mo, d, h, mi) = local(unix);
    let base = format!("{y:04}-{mo:02}-{d:02}_{h:02}{mi:02}");
    let mut dir = root.join(&base);
    let mut n = 2;
    while dir.exists() {
        dir = root.join(format!("{base}-{n}"));
        n += 1;
    }
    dir
}

/// Copies pocket-ui's settings and library notes from `data_dir` into a new dated folder under `root`.
pub fn create(data_dir: &Path, root: &Path, reason: Reason, now: u64, app: &str) -> Result<Backup, String> {
    // The lists a game's end hands to the card writer are copied as they are now, not as they were.
    crate::fsutil::settle();
    let files = collect(data_dir);
    let dir = folder_name(root, now);
    let tmp = root.join(format!(".making-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).map_err(|e| format!("{}: {e}", tmp.display()))?;
    let mut bytes = 0;
    let copied = (|| {
        for rel in &files {
            let data = std::fs::read(data_dir.join(rel)).map_err(|e| format!("{rel}: {e}"))?;
            bytes += data.len() as u64;
            let to = tmp.join(rel);
            if let Some(parent) = to.parent() {
                std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
            }
            crate::fsutil::write_atomic(&to, &data).map_err(|e| format!("{rel}: {e}"))?;
        }
        let manifest = Manifest { version: 1, created: now, reason, app: app.to_string(), files: files.clone() };
        // Plain, without write_json's .bak twin: the folder holds what its list names and the list.
        let json = serde_json::to_vec_pretty(&manifest).map_err(|e| format!("{MANIFEST}: {e}"))?;
        crate::fsutil::write_atomic(&tmp.join(MANIFEST), &json).map_err(|e| format!("{MANIFEST}: {e}"))?;
        // Whole or not at all: the folder gets its dated name only once everything is in it.
        std::fs::rename(&tmp, &dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        Ok::<Manifest, String>(manifest)
    })();
    match copied {
        Ok(manifest) => Ok(Backup { dir, manifest, bytes }),
        Err(e) => {
            let _ = std::fs::remove_dir_all(&tmp);
            Err(format!("The backup couldn't be written ({e}). Nothing was changed."))
        }
    }
}

/// The backups under `root`, newest first; folders without a readable list are not backups.
pub fn list(root: &Path) -> Vec<Backup> {
    let Ok(rd) = std::fs::read_dir(root) else { return Vec::new() };
    let mut out: Vec<Backup> = rd
        .flatten()
        .filter(|e| !e.file_name().to_string_lossy().starts_with('.') && e.path().symlink_metadata().is_ok_and(|m| m.is_dir()))
        .filter_map(|e| {
            let manifest: Manifest = serde_json::from_slice(&std::fs::read(e.path().join(MANIFEST)).ok()?).ok()?;
            let bytes = manifest.files.iter().filter(|f| allowed(f)).filter_map(|f| std::fs::metadata(e.path().join(f)).ok()).map(|m| m.len()).sum();
            Some(Backup { dir: e.path(), manifest, bytes })
        })
        .collect();
    out.sort_by(|a, b| b.manifest.created.cmp(&a.manifest.created).then(b.dir.cmp(&a.dir)));
    out
}

/// Puts a backup back: every listed name (`allowed` only, plain files only) is copied into `data_dir`, and what the
/// backup didn't have is removed, so settings and library are as they were then. Makes a backup of now first
/// (`Reason::BeforeRestore`, under `root`), so a restore can be undone. Returns that backup.
pub fn restore(data_dir: &Path, backup: &Path, root: &Path, now: u64, app: &str) -> Result<Backup, String> {
    let manifest: Manifest = serde_json::from_slice(&std::fs::read(backup.join(MANIFEST)).map_err(|e| format!("{MANIFEST}: {e}"))?)
        .map_err(|e| format!("The backup's list can't be read ({e}). Nothing was changed."))?;
    let files: Vec<&String> = manifest.files.iter().filter(|f| allowed(f)).collect();
    // Everything is read before anything changes: a damaged backup leaves the settings as they are.
    let mut data = Vec::new();
    for rel in &files {
        let from = backup.join(rel);
        if !from.symlink_metadata().is_ok_and(|m| m.is_file()) {
            return Err(format!("{rel} is missing from the backup. Nothing was changed."));
        }
        data.push(std::fs::read(&from).map_err(|e| format!("{rel}: {e}. Nothing was changed."))?);
    }
    let before = create(data_dir, root, Reason::BeforeRestore, now, app)?;
    clear(data_dir, true)?;
    for (rel, bytes) in files.iter().zip(data) {
        let to = data_dir.join(rel);
        if let Some(parent) = to.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        crate::fsutil::write_atomic(&to, &bytes).map_err(|e| format!("{rel}: {e}"))?;
    }
    Ok(before)
}

/// Removes pocket-ui's settings (and with `all`, the service settings and the library's notes too) from `data_dir`,
/// with their `.bak` copies, so nothing comes back from those.
fn clear(data_dir: &Path, all: bool) -> Result<(), String> {
    let mut names: Vec<&str> = SETTINGS_FILES.to_vec();
    if all {
        names.extend(SERVICE_FILES);
        names.extend(LIBRARY_FILES);
    }
    for f in names {
        for name in [f.to_string(), format!("{f}.bak")] {
            match std::fs::remove_file(data_dir.join(&name)) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(format!("{name}: {e}")),
                _ => {}
            }
        }
    }
    for d in SETTINGS_DIRS {
        match std::fs::remove_dir_all(data_dir.join(d)) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(format!("{d}: {e}")),
            _ => {}
        }
    }
    Ok(())
}

/// pocket-ui's settings back to how they start, after a backup (`Reason::BeforeReset`): the look, sounds, haptics,
/// LEDs, sleep, performance, Curator and every console's and game's Options. Kept: the library's notes, Sync and
/// Tailscale, the accounts, what the Store installed, and the first start counted as done (with the update and store
/// addresses), so the Pocket opens on Home and not on the welcome. Returns the backup.
pub fn reset(data_dir: &Path, root: &Path, now: u64, app: &str) -> Result<Backup, String> {
    let old = crate::settings::Settings::load(data_dir);
    let backup = create(data_dir, root, Reason::BeforeReset, now, app)?;
    clear(data_dir, false)?;
    // The version whose new cards were seen stays, so a reset never brings "New in this version" back (`features`).
    crate::settings::Settings { onboarded: true, update_url: old.update_url, store_url: old.store_url, seen_version: old.seen_version, ..Default::default() }.save(data_dir);
    Ok(backup)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;

    fn dirs(name: &str) -> (TestDir, PathBuf, PathBuf) {
        let d = TestDir::new(&format!("backup-{name}"));
        let (data, root) = (d.join("data"), d.join("backups"));
        std::fs::create_dir_all(&data).unwrap();
        (d, data, root)
    }

    fn write(dir: &Path, rel: &str, text: &str) {
        let p = dir.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }

    /// A data folder as on the device: settings, notes, and things that are never backed up.
    fn device(data: &Path) {
        write(data, "settings.json", r#"{"onboarded": true, "appearance": "light", "store_url": "file:///s/catalog.json"}"#);
        write(data, "leds.json", "{}");
        write(data, "library.json", r#"{"games": {}}"#);
        write(data, "sync.json", "{}");
        write(data, "game-settings/gba/Game.json", "{}");
        write(data, "core-options/gba.json", "{}");
        write(data, "retroachievements.json", "token");
        write(data, "share/password", "secret");
        write(data, "store/installed.json", "{}");
        write(data, "restore.json", "{}");
        write(data, "covers/gba/Game.png", "png");
    }

    #[test]
    fn only_pocket_uis_own_names_are_allowed() {
        for ok in ["settings.json", "library.json", "sync.json", "game-settings/gba/Game.json", "core-options/gba.json", "cheats/gba/x.json"] {
            assert!(allowed(ok), "{ok}");
        }
        for bad in ["", "/etc/passwd", "../settings.json", "game-settings/../../x", "retroachievements.json", "share/password", "store/installed.json", "restore.json", "covers/a.png", "settings.json/x", "./settings.json"] {
            assert!(!allowed(bad), "{bad}");
        }
    }

    #[test]
    fn a_backup_holds_settings_and_notes_but_no_secrets_or_store_files() {
        let (_dir, data, root) = dirs("create");
        device(&data);
        std::os::unix::fs::symlink("/etc/passwd", data.join("game-settings/link.json")).unwrap();
        let b = create(&data, &root, Reason::Manual, 1_792_744_800, "1.2.3").unwrap();
        assert_eq!(b.manifest.files, ["core-options/gba.json", "game-settings/gba/Game.json", "leds.json", "library.json", "settings.json", "sync.json"]);
        assert!(b.dir.file_name().unwrap().to_string_lossy().starts_with("2026-10-"), "{}", b.dir.display());
        assert!(!b.dir.join("share").exists() && !b.dir.join("retroachievements.json").exists());
        let mut held: Vec<String> = std::fs::read_dir(&b.dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        held.sort();
        assert_eq!(held, ["backup.json", "core-options", "game-settings", "leds.json", "library.json", "settings.json", "sync.json"], "nothing else in the folder");
        let listed = list(&root);
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].manifest.reason, Reason::Manual);
        assert!(listed[0].bytes > 0);
        // A second one in the same minute gets a folder of its own.
        let b2 = create(&data, &root, Reason::Manual, 1_792_744_800, "1.2.3").unwrap();
        assert_ne!(b.dir, b2.dir);
        assert_eq!(list(&root).len(), 2);
    }

    #[test]
    fn reset_backs_up_first_and_keeps_notes_services_and_the_first_start() {
        let (_dir, data, root) = dirs("reset");
        device(&data);
        write(&data, "settings.json", r#"{"onboarded": true, "appearance": "light", "store_url": "file:///s/catalog.json", "seen_version": "0.3.0", "frames": "every"}"#);
        write(&data, "settings.json.bak", r#"{"appearance": "light"}"#);
        let b = reset(&data, &root, 1_792_744_800, "1.2.3").unwrap();
        assert_eq!(b.manifest.reason, Reason::BeforeReset);
        assert!(b.dir.join("settings.json").exists() && b.dir.join("game-settings/gba/Game.json").exists());
        let s = crate::settings::Settings::load(&data);
        assert!(s.onboarded, "Home, not the welcome");
        assert_eq!(s.appearance, crate::settings::Appearance::Dark);
        assert_eq!(s.store_url.as_deref(), Some("file:///s/catalog.json"), "the store address stays");
        assert_eq!(s.seen_version.as_deref(), Some("0.3.0"), "the new cards seen stay seen");
        assert_eq!(s.frames, crate::settings::Frames::Off, "Screen frames goes back to its start");
        assert!(!std::fs::read_to_string(data.join("settings.json.bak")).unwrap_or_default().contains("light"), "no way back to the old one through its .bak");
        assert!(!data.join("leds.json").exists() && !data.join("game-settings").exists() && !data.join("core-options").exists());
        for kept in ["library.json", "sync.json", "retroachievements.json", "share/password", "store/installed.json", "covers/gba/Game.png"] {
            assert!(data.join(kept).exists(), "{kept} stays");
        }
    }

    #[test]
    fn restore_puts_everything_back_and_can_be_undone() {
        let (_dir, data, root) = dirs("restore");
        device(&data);
        let b = create(&data, &root, Reason::Manual, 1_792_744_800, "1.2.3").unwrap();
        // Later: other settings, a new per-game file, the notes changed.
        write(&data, "settings.json", r#"{"onboarded": true}"#);
        write(&data, "game-settings/snes/Other.json", "{}");
        write(&data, "library.json", r#"{"games": {"x": 1}}"#);
        write(&data, "favorites.json", "[]");
        let before = restore(&data, &b.dir, &root, 1_792_748_400, "1.2.4").unwrap();
        assert_eq!(before.manifest.reason, Reason::BeforeRestore);
        assert!(std::fs::read_to_string(data.join("settings.json")).unwrap().contains("light"));
        assert_eq!(std::fs::read_to_string(data.join("library.json")).unwrap(), r#"{"games": {}}"#);
        assert!(!data.join("game-settings/snes/Other.json").exists() && !data.join("favorites.json").exists(), "as it was then");
        assert!(data.join("retroachievements.json").exists() && data.join("store/installed.json").exists(), "never touched");
        assert!(before.dir.join("favorites.json").exists(), "the state before the restore is kept");
        assert_eq!(list(&root).len(), 2);
    }

    #[test]
    fn a_changed_backup_writes_nothing_outside_and_a_damaged_one_changes_nothing() {
        let (_dir, data, root) = dirs("hostile");
        device(&data);
        let b = create(&data, &root, Reason::Manual, 1_792_744_800, "1.2.3").unwrap();
        // Someone edits the list over the share.
        let mut m = b.manifest.clone();
        m.files.push("../../evil.json".into());
        m.files.push("share/password".into());
        std::fs::write(b.dir.join(MANIFEST), serde_json::to_vec(&m).unwrap()).unwrap();
        write(&b.dir, "share/password", "attacker");
        restore(&data, &b.dir, &root, 1_792_748_400, "1").unwrap();
        assert_eq!(std::fs::read_to_string(data.join("share/password")).unwrap(), "secret");
        assert!(!data.parent().unwrap().join("evil.json").exists());
        // A file the list names is gone: refused before anything changed.
        write(&data, "settings.json", r#"{"onboarded": true, "appearance": "automatic"}"#);
        std::fs::remove_file(b.dir.join("leds.json")).unwrap();
        assert!(restore(&data, &b.dir, &root, 1_792_752_000, "1").is_err());
        assert!(std::fs::read_to_string(data.join("settings.json")).unwrap().contains("automatic"));
    }
}
