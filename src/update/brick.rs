//! Atomic app-folder updates on the standalone Brick card.
use super::*;

/// What the unpacked program folder may take next to the download, for the space check before it: the folder is a little over
/// twice its .tar.gz (programs and cores), so three times leaves room. The list has no unpacked size.
const BRICK_UNPACKED_FACTOR: u64 = 3;
/// Kept free on the card besides: a full card stops saves and logs.
const BRICK_SPARE: u64 = 32 << 20;
/// No program folder is bigger than this unpacked; a tar that says otherwise is refused while it unpacks.
const BRICK_MAX_UNPACKED: u64 = 2 << 30;
/// What the program folder must hold before it may replace the one that runs (deploy/brick/boot.sh and start.sh rely on them).
const BRICK_REQUIRED: [&str; 7] = [
    "start.sh",
    "RELEASE",
    "bin/pocket-ui",
    "bin/pocket-core",
    "bin/pocket-fb",
    "bin/brick-hw",
    "msg/starting.png",
];

pub(super) fn card_full(needed: u64) -> String {
    format!(
        "The SD card is too full for this update: it needs {} MB more. Nothing was changed.",
        needed.div_ceil(1 << 20)
    )
}

/// The Brick card's update (docs/TRIMUI-BRICK-REMOTE.md): the tarball is the program folder `app/`, optionally referencing separate verified assets (the card's
/// pocket-ui folder). Download all archives into RAM (resumed within this boot), verify size and sha256 against the signed list, then unpack into
/// `app.new`, checked (the files the start scripts need, `RELEASE` = the version, `sh -n start.sh`, both programs print the version),
/// marked `.ready`; then `update-pending` and two renames: `app` becomes `app.previous` (the way back), `app.new` becomes `app`.
/// A failure before the renames leaves `app` as it was; deploy/brick/boot.sh finishes or undoes renames a power cut interrupted, and
/// puts `app.previous` back when the new version does not show its first picture. Roms, Saves, covers and `storage/` are not
/// touched. The caller restarts pocket-ui (`RESTART_CODE`).
pub fn install_brick(rel: &Release, manifest_url: &str, card: &Path, cancel: &Cancel, progress: &dyn Fn(u64)) -> Result<(), String> {
    install_brick_with(
        &TOOLS,
        rel,
        manifest_url,
        card,
        cancel,
        progress,
        &|p| crate::starter::free_bytes(p),
        &crate::untar::unpack_file_throttled,
    )
}

/// How `install_brick` unpacks (`untar::unpack_file`); the tests put a full card in its place.
type Unpack<'a> = &'a dyn Fn(&Path, &Path, u64) -> Result<Vec<crate::untar::Entry>, crate::untar::Refused>;

pub(super) fn install_brick_with(
    tools: &Tools,
    rel: &Release,
    manifest_url: &str,
    card: &Path,
    cancel: &Cancel,
    progress: &dyn Fn(u64),
    free: &dyn Fn(&Path) -> Option<u64>,
    unpack: Unpack,
) -> Result<(), String> {
    cancel.boundary(super::CancelBoundary::Activation);
    if rel.version.is_empty() || rel.version.len() > 120 || rel.version.contains(['/', '\\', '\n', '\r', '\0']) {
        return Err("The update list names no usable version. Nothing was changed.".into());
    }
    let recovery = ["app/.asset-moves", "app/.restore-assets", "app.previous/.restore-assets"]
        .iter().any(|p| std::fs::symlink_metadata(card.join(p)).is_ok())
        || (!card.join("app/start.sh").is_file() && card.join("app.previous/.asset-moves").exists());
    if recovery {
        return Err("Restart the device to finish recovering the previous update before trying again.".into());
    }
    let mut ram = super::brick_io::Ram::new(card)?;
    let tarball = super::brick_io::download_path(card, &rel.version);
    ram.resume = Some(tarball.clone());
    // Earlier tries: another version's download goes; this version's stays to be resumed. A half unpacked folder is never reused.
    if let Ok(entries) = std::fs::read_dir(&ram.dir) {
        for e in entries.flatten() {
            if e.path() != tarball {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
    let have = std::fs::metadata(&tarball).map_or(0, |m| m.len()).min(rel.size);
    let needed = (rel.size - have) + rel.size.saturating_mul(BRICK_UNPACKED_FACTOR) + BRICK_SPARE;
    if let Some(avail) = free(card) {
        if avail < needed {
            return Err(card_full(needed - avail));
        }
    }
    ram.reserve(rel.size)?;
    download_checked(tools, rel, manifest_url, &tarball, cancel, progress)?;
    let plan = super::brick_assets::fetch(tools, card, &tarball, &mut ram, manifest_url, cancel, progress, free)?;
    if cancel.is_cancelled() { return Err("Download cancelled.".into()); }
    // Every network child has exited before the first update write on the card, including changed components.
    let _idle = super::brick_io::IdleIo::enter().map_err(|e| e.to_string())?;
    cancel.set_phase(super::WorkerPhase::Unpacking);
    let work = card.join(".update");
    std::fs::create_dir_all(&work).map_err(|e| e.to_string())?;
    if let Ok(entries) = std::fs::read_dir(&work) {
        for e in entries.flatten() { let _ = std::fs::remove_file(e.path()); }
    }
    let staging = card.join("app.new");
    let _ = std::fs::remove_dir_all(&staging);
    let _ = std::fs::remove_dir_all(card.join("app.failed"));
    let unpacked = unpack(&tarball, &staging, BRICK_MAX_UNPACKED);
    let fail = |msg: String| {
        if !card.join("app/start.sh").is_file() && card.join("app.previous/.asset-moves").exists() {
            return Err(format!("{msg} Recovery will finish at the next start."));
        }
        if card.join("app/.asset-moves").exists() {
            if let Err(e) = plan.undo_moves(card, &staging) {
                // Leave the durable manifest and both trees for boot's idempotent recovery.
                return Err(format!("{msg} Recovery will finish at the next start: {e}"));
            }
        }
        let _ = std::fs::remove_dir_all(&staging);
        Err(msg)
    };
    match unpacked {
        Ok(_) => {}
        Err(crate::untar::Refused::Full) => {
            let _ = std::fs::remove_dir_all(&staging);
            // The download stays for the next try, once there is room.
            return Err(card_full(rel.size.saturating_mul(BRICK_UNPACKED_FACTOR)));
        }
        Err(e) => {
            let _ = std::fs::remove_file(&tarball);
            return fail(e.to_string());
        }
    }
    let _ = std::fs::remove_file(&tarball);
    cancel.set_phase(super::WorkerPhase::Validating);
    if let Some(missing) = BRICK_REQUIRED.iter().find(|f| !staging.join(f).is_file()) {
        return fail(format!("The update is missing {missing}. Nothing was changed."));
    }
    if std::fs::read_to_string(staging.join("RELEASE")).map(|v| v.trim().to_string()).ok().as_deref() != Some(rel.version.as_str()) {
        return fail(format!("The update is not version {} inside. Nothing was changed.", rel.version));
    }
    if !Command::new("sh")
        .arg("-n")
        .arg(staging.join("start.sh"))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
    {
        return fail("The update's start script is broken. Nothing was changed.".into());
    }
    for bin in ["pocket-ui", "pocket-core"] {
        let out = Command::new(staging.join("bin").join(bin)).arg("--version").output();
        if !out
            .as_ref()
            .is_ok_and(|o| o.status.success() && reports_version(&String::from_utf8_lossy(&o.stdout), bin, &rel.version))
        {
            return fail(format!("The new {bin} doesn't start on this device. Nothing was changed."));
        }
    }
    if let Err(e) = plan.stage(&staging, cancel) {
        return fail(e);
    }
    if cancel.is_cancelled() {
        return fail("Download cancelled.".into());
    }
    cancel.set_phase(super::WorkerPhase::MovingAssets);
    if let Err(e) = crate::cheat_download::preserve_legacy(card) { return fail(e); }
    if let Err(e) = plan.move_reused(card, &staging) { return fail(e); }
    cancel.set_phase(super::WorkerPhase::Flushing);
    // Make newly created parent directories and all moved assets durable BEFORE .ready can reach disk.
    if let Err(e) = crate::fsutil::sync_tree(&staging) { return fail(e.to_string()); }
    crate::fsutil::sync_dir(card);
    // FAT's directory fsync can be unsupported; order .ready after all asset/directory metadata.
    crate::fsutil::flush_all();
    let ready = (|| -> std::io::Result<()> {
        let f = std::fs::File::create(staging.join(".ready"))?;
        f.sync_all()?;
        let pending = card.join("storage/pocket-ui");
        std::fs::create_dir_all(&pending)?;
        crate::fsutil::write_atomic(&pending.join("update-pending"), rel.version.as_bytes())
    })();
    if let Err(e) = ready {
        return fail(e.to_string());
    }
    // Everything written so far is on the card before the renames.
    crate::fsutil::flush_all();
    cancel.set_phase(super::WorkerPhase::Activating);
    if let Err(e) = switch_app(card, &staging, &|a, b| std::fs::rename(a, b)) { return fail(e); }
    crate::fsutil::flush_all();
    let _ = std::fs::remove_dir_all(&ram.dir);
    Ok(())
}

fn switch_app(card: &Path, staging: &Path, rename: &dyn Fn(&Path, &Path) -> std::io::Result<()>) -> Result<(), String> {
    let app = card.join("app");
    let previous = card.join("app.previous");
    let _ = std::fs::remove_dir_all(&previous);
    if app.exists() {
        if let Err(e) = rename(&app, &previous) {
            let _ = std::fs::remove_file(card.join("storage/pocket-ui/update-pending"));
            return Err(format!("Couldn't switch to the new version: {e}. Nothing was changed."));
        }
        crate::fsutil::sync_dir(card);
    }
    if let Err(e) = rename(staging, &app) {
        // Put the running version back; if even that fails, boot.sh takes app.previous back at the next start.
        if rename(&previous, &app).is_ok() {
            crate::fsutil::sync_dir(card);
            let _ = std::fs::remove_file(card.join("storage/pocket-ui/update-pending"));
        }
        return Err(format!("Couldn't switch to the new version: {e}."));
    }
    crate::fsutil::sync_dir(card);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn failed_activation_and_failed_restore_keep_assets_and_health_marker_for_boot() {
        let dir = crate::testutil::TestDir::new("brick-double-rename-failure");
        let card = &*dir;
        for d in ["app", "app.new/cores", "storage/pocket-ui"] { std::fs::create_dir_all(card.join(d)).unwrap(); }
        for (p, data) in [("app/start.sh", "exit 0\n"), ("app/RELEASE", "1"), ("app/.asset-moves", "cores/core.so\n"),
            ("app.new/start.sh", "exit 76\n"), ("app.new/RELEASE", "2"), ("app.new/.ready", ""),
            ("app.new/cores/core.so", "original core"), ("storage/pocket-ui/update-pending", "2")] {
            std::fs::write(card.join(p), data).unwrap();
        }
        std::fs::write(card.join("start.sh"), include_bytes!("../../deploy/brick/boot.sh")).unwrap();
        let count = std::cell::Cell::new(0);
        let rename = |a: &Path, b: &Path| {
            count.set(count.get() + 1);
            if count.get() == 1 { std::fs::rename(a, b) } else { Err(std::io::Error::other("card write failed")) }
        };
        assert!(switch_app(card, &card.join("app.new"), &rename).is_err());
        assert!(card.join("app.new/cores/core.so").exists());
        assert_eq!(std::fs::read(card.join("storage/pocket-ui/update-pending")).unwrap(), b"2");
        // Startup finishes the interrupted swap, sees the new version fail, and restores the full old version.
        assert!(Command::new("sh").arg(card.join("start.sh")).env("BRICK_DATA", card).status().unwrap().success());
        assert_eq!(std::fs::read(card.join("app/RELEASE")).unwrap(), b"1");
        assert_eq!(std::fs::read(card.join("app/cores/core.so")).unwrap(), b"original core");
        assert!(!card.join("storage/pocket-ui/update-pending").exists());
    }
}
