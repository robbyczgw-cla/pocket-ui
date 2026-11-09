//! Two-phase component updates: resolve and download everything in RAM before writing the card.
use super::*;
use std::collections::HashSet;
use std::io::Read;

const ASSET_LIMIT: u64 = 80_000_000;
const UNPACKED_LIMIT: u64 = 2 << 30;
const HELPERS: [&str; 6] = ["bin/brick-egl", "bin/curl", "bin/dropbearmulti", "bin/syncthing", "bin/tailscale", "bin/tailscaled"];
const INVALID: &str = "The update's asset list is invalid. Nothing was changed.";
const DAMAGED: &str = "The update's core or data file is damaged. Nothing was changed.";
#[derive(Deserialize)]
struct Assets { format: u32, assets: Vec<Asset> }
#[derive(Deserialize)]
struct Asset { path: String, size: u64, sha256: String, package: Release }

fn matches(path: &Path, asset: &Asset) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|m| m.is_file() && m.len() == asset.size)
        && store::sha256_file(path).is_ok_and(|h| h == asset.sha256.to_lowercase())
}
fn safe_old(card: &Path, asset: &Asset) -> bool {
    let app = card.join("app");
    let old = app.join(&asset.path);
    let mut parent = old.parent();
    while let Some(p) = parent.filter(|p| p.starts_with(&app)) {
        if !std::fs::symlink_metadata(p).is_ok_and(|m| m.is_dir()) { return false; }
        parent = p.parent();
    }
    matches(&old, asset)
}

pub(super) struct Plan {
    entries: Vec<(Asset, Option<PathBuf>)>,
    pub(super) reused: Vec<String>,
}

/// Inspect the signed app archive without extracting it. Even its inventory is read before any SD write.
pub(super) fn fetch(
    tools: &Tools, card: &Path, archive: &Path, ram: &mut super::brick_io::Ram,
    manifest_url: &str, cancel: &Cancel, progress: &dyn Fn(u64), free: &dyn Fn(&Path) -> Option<u64>,
) -> Result<Plan, String> {
    let mut json = None;
    let mut gz = crate::untar::Gz::new(std::fs::File::open(archive).map_err(|e| e.to_string())?, UNPACKED_LIMIT).map_err(|e| e.to_string())?;
    let inventory = crate::untar::walk(&mut gz, |e, _, data| {
        if e.path == "asset-packages.json" {
            if e.dir || json.is_some() { return Err(crate::untar::Refused::Unsafe); }
            let mut bytes = Vec::new();
            data.take((1 << 20) + 1).read_to_end(&mut bytes)?;
            if bytes.len() > 1 << 20 { return Err(crate::untar::Refused::Unsafe); }
            json = Some(bytes);
        }
        if matches!(e.path.as_str(), ".asset-moves" | ".restore-assets" | ".ready") {
            return Err(crate::untar::Refused::Unsafe);
        }
        Ok(())
    }).map_err(|e| e.to_string())?;
    let plan = from_json(tools, card, json.as_deref(), ram, manifest_url, cancel, progress, free)?;
    if json.is_some() && inventory.iter().any(|e| !e.dir &&
        (e.path.starts_with("cores/") || e.path.starts_with("bios-data/") || HELPERS.contains(&e.path.as_str()))) {
        return Err(INVALID.into());
    }
    Ok(plan)
}

fn from_json(
    tools: &Tools, card: &Path, json: Option<&[u8]>, ram: &mut super::brick_io::Ram,
    manifest_url: &str, cancel: &Cancel, progress: &dyn Fn(u64), free: &dyn Fn(&Path) -> Option<u64>,
) -> Result<Plan, String> {
    let Some(json) = json else { return Ok(Plan { entries: Vec::new(), reused: Vec::new() }); };
    let assets: Assets = serde_json::from_slice(json).map_err(|_| INVALID)?;
    if assets.format != 1 || assets.assets.is_empty() || assets.assets.len() > 1024 { return Err(INVALID.into()); }
    let mut paths = HashSet::new();
    let mut total = 0u64;
    for asset in &assets.assets {
        let parts: Vec<_> = asset.path.split('/').collect();
        if parts.len() < 2 || (!matches!(parts[0], "cores" | "bios-data") && !HELPERS.contains(&asset.path.as_str()))
            || parts.iter().any(|p| p.is_empty() || *p == "." || *p == ".." || p.contains(['\\', '\n', '\r', '\0']))
            || !paths.insert(&asset.path) || asset.size > UNPACKED_LIMIT || asset.package.size > ASSET_LIMIT || asset.package.size == 0
            || asset.sha256.len() != 64 || !asset.sha256.bytes().all(|b| b.is_ascii_hexdigit()) { return Err(INVALID.into()); }
        total = total.saturating_add(asset.size);
    }
    if total > UNPACKED_LIMIT { return Err(INVALID.into()); }
    let mut entries = Vec::new();
    let mut reused = Vec::new();
    let mut needed = 32u64 << 20;
    for asset in assets.assets {
        if cancel.is_cancelled() { return Err("Download cancelled.".into()); }
        cancel.set_phase(super::WorkerPhase::CheckingAssets); progress(0);
        if safe_old(card, &asset) {
            reused.push(asset.path.clone());
            entries.push((asset, None));
        } else {
            ram.reserve(asset.package.size)?;
            needed = needed.saturating_add(asset.size);
            let archive = ram.dir.join(format!("asset-{}.tar.gz", entries.len()));
            entries.push((asset, Some(archive)));
        }
    }
    if let Some(avail) = free(card).filter(|n| *n < needed) { return Err(super::brick::card_full(needed - avail)); }
    // Every package is complete and verified before stage() creates any destination on the card.
    for (asset, archive) in &entries {
        if let Some(archive) = archive { download_checked(tools, &asset.package, manifest_url, archive, cancel, progress)?; }
    }
    Ok(Plan { entries, reused })
}

impl Plan {
    pub(super) fn stage(&self, staging: &Path, cancel: &Cancel) -> Result<(), String> {
        for (asset, archive) in &self.entries {
            if cancel.is_cancelled() { return Err("Download cancelled.".into()); }
            let dest = staging.join(&asset.path);
            // Reject even a directory/symlink at a listed destination; the app tar cannot shadow an inherited asset.
            if std::fs::symlink_metadata(&dest).is_ok() { return Err(INVALID.into()); }
            std::fs::create_dir_all(dest.parent().ok_or(INVALID)?).map_err(|e| e.to_string())?;
            let Some(archive) = archive else { continue; };
            let temp = staging.join(".asset.new");
            let entries = crate::untar::unpack_file_throttled(archive, &temp, asset.size + (64 << 10)).map_err(|e| e.to_string())?;
            if entries.iter().any(|e| if e.dir { !asset.path.starts_with(&format!("{}/", e.path)) } else { e.path != asset.path })
                || !matches(&temp.join(&asset.path), asset) { return Err(DAMAGED.into()); }
            std::fs::rename(temp.join(&asset.path), &dest).map_err(|e| e.to_string())?;
            std::fs::remove_dir_all(&temp).map_err(|e| e.to_string())?;
        }
        Ok(())
    }
    /// Only after downloads, extraction and build validation succeed. The fixed boot script is durable before
    /// the first asset moves; a power loss restores the old app or completes/rolls back the new version.
    pub(super) fn move_reused(&self, card: &Path, staging: &Path) -> Result<(), String> {
        if self.reused.is_empty() { return Ok(()); }
        if self.entries.iter().any(|(asset, archive)| archive.is_none() && !safe_old(card, asset)) { return Err(DAMAGED.into()); }
        crate::fsutil::write_atomic(&card.join("start.sh"), include_bytes!("../../deploy/brick/boot.sh")).map_err(|e| e.to_string())?;
        let app = card.join("app");
        crate::fsutil::write_atomic(&app.join(".asset-moves"), format!("{}\n", self.reused.join("\n")).as_bytes()).map_err(|e| e.to_string())?;
        // FAT may refuse directory fsync. The fixed recovery script and manifest must both survive
        // before a single old asset is removed from its original path.
        unsafe { libc::sync() };
        for path in &self.reused {
            std::fs::rename(app.join(path), staging.join(path)).map_err(|e| e.to_string())?;
            if path.starts_with("bin/") {
                use std::os::unix::fs::PermissionsExt;
                let dest = staging.join(path);
                let mode = std::fs::metadata(&dest).map_err(|e| e.to_string())?.permissions().mode();
                if mode & 0o111 == 0 {
                    std::fs::set_permissions(dest, std::fs::Permissions::from_mode(0o755)).map_err(|e| e.to_string())?;
                }
            }
            crate::fsutil::sync_dir(app.join(path).parent().unwrap());
            crate::fsutil::sync_dir(staging.join(path).parent().unwrap());
        }
        Ok(())
    }
    pub(super) fn undo_moves(&self, card: &Path, staging: &Path) -> Result<(), String> {
        let app = card.join("app");
        for path in &self.reused {
            if !app.join(path).exists() {
                std::fs::rename(staging.join(path), app.join(path)).map_err(|e| e.to_string())?;
                crate::fsutil::sync_dir(app.join(path).parent().unwrap());
            }
        }
        // See move_reused(): do not remove the recovery instructions until the restored paths are durable.
        unsafe { libc::sync() };
        let manifest = app.join(".asset-moves");
        if manifest.exists() { std::fs::remove_file(manifest).map_err(|e| e.to_string())?; crate::fsutil::sync_dir(&app); }
        Ok(())
    }
}

#[cfg(test)]
fn prepare(tools: &Tools, card: &Path, staging: &Path, manifest_url: &str, cancel: &Cancel,
    progress: &dyn Fn(u64), free: &dyn Fn(&Path) -> Option<u64>) -> Result<(), String> {
    let json = std::fs::read(staging.join("asset-packages.json")).map_err(|e| e.to_string())?;
    let mut ram = super::brick_io::Ram::new(card)?;
    let result = (|| {
        let plan = from_json(tools, card, Some(&json), &mut ram, manifest_url, cancel, progress, free)?;
        plan.stage(staging, cancel)?;
        plan.move_reused(card, staging)?;
        Ok(())
    })();
    let _ = std::fs::remove_dir_all(&ram.dir);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;

    fn inventory(staging: &Path, path: &str, data: &[u8], package: &Release) {
        std::fs::create_dir_all(staging).unwrap();
        let json = serde_json::json!({"format":1, "assets":[{"path":path,"size":data.len(),
            "sha256":store::sha256_hex(data), "package":package}]});
        std::fs::write(staging.join("asset-packages.json"), serde_json::to_vec(&json).unwrap()).unwrap();
    }
    fn package(dir: &Path, path: &str, data: &[u8]) -> Release {
        let src = dir.join("package");
        std::fs::create_dir_all(src.join(path).parent().unwrap()).unwrap();
        std::fs::write(src.join(path), data).unwrap();
        let file = dir.join("core.tar.gz");
        assert!(Command::new("tar")
            .arg("-czf")
            .arg(&file)
            .arg("-C")
            .arg(src)
            .arg(path)
            .status()
            .unwrap()
            .success());
        Release {
            version: "9".into(),
            url: format!("file://{}", file.display()),
            mirrors: vec![],
            preferred_sources: Vec::new(),
            sha256: store::sha256_file(&file).unwrap(),
            size: std::fs::metadata(file).unwrap().len(),
            notes: String::new(),
        }
    }
    #[test]
    fn all_component_downloads_finish_in_ram_before_any_card_changes() {
        let dir = TestDir::new("asset-download-before-write");
        let card = dir.join("card");
        std::fs::create_dir_all(card.join("app")).unwrap();
        std::fs::write(card.join("app/RELEASE"), b"old").unwrap();
        let a = package(&dir.join("a"), "cores/a.so", b"core A");
        let b = package(&dir.join("b"), "bios-data/b.dat", b"data B");
        let json = serde_json::json!({"format":1, "assets":[
            {"path":"cores/a.so", "size":6, "sha256":store::sha256_hex(b"core A"), "package":a},
            {"path":"bios-data/b.dat", "size":6, "sha256":store::sha256_hex(b"data B"), "package":b}
        ]});
        let curl = dir.join("curl");
        crate::testutil::write_executable(&curl, format!(r#"#!/bin/sh
while [ $# -gt 0 ]; do [ "$1" = -o ] && out=$2; url=$1; shift; done
case "$out" in /tmp/pocket-ui-update-*/asset-*) ;; *) exit 71;; esac
[ ! -e '{0}/app.new' ] && [ ! -e '{0}/.update' ] || exit 72
[ "$(cat '{0}/app/RELEASE')" = old ] || exit 73
cp "${{url#file://}}" "$out"
"#, card.display()));
        let curl_name = curl.to_str().unwrap();
        let tools = Tools { curl: curl_name, ..TOOLS };
        let mut ram = super::super::brick_io::Ram::new(&card).unwrap();
        let plan = from_json(&tools, &card, Some(&serde_json::to_vec(&json).unwrap()), &mut ram,
            "file:///channel.json", &Cancel::new(), &|_| {}, &|_| Some(u64::MAX)).unwrap();
        assert!(!card.join("app.new").exists() && !card.join(".update").exists());
        let staging = card.join("app.new");
        std::fs::create_dir(&staging).unwrap();
        plan.stage(&staging, &Cancel::new()).unwrap();
        assert_eq!(std::fs::read(staging.join("cores/a.so")).unwrap(), b"core A");
        assert_eq!(std::fs::read(staging.join("bios-data/b.dat")).unwrap(), b"data B");
        assert_eq!(std::fs::read(card.join("app/RELEASE")).unwrap(), b"old");
    }
    #[test]
    fn unchanged_core_moves_without_a_server_and_has_a_durable_way_back() {
        let dir = TestDir::new("asset-reuse");
        let card = dir.join("card");
        let staging = card.join("app.new");
        let data = b"installed core";
        let mut rel = package(&dir, "cores/core.so", data);
        std::fs::create_dir_all(card.join("app/cores")).unwrap();
        std::fs::write(card.join("app/cores/core.so"), data).unwrap();
        rel.url = "https://blocked.invalid/never-needed".into();
        inventory(&staging, "cores/core.so", data, &rel);
        prepare(
            &TOOLS,
            &card,
            &staging,
            "file:///u.json",
            &Cancel::new(),
            &|_| {},
            &|_| Some(u64::MAX),
        )
        .unwrap();
        assert_eq!(std::fs::read(staging.join("cores/core.so")).unwrap(), data);
        assert!(!card.join("app/cores/core.so").exists());
        assert_eq!(std::fs::read(card.join("app/.asset-moves")).unwrap(), b"cores/core.so\n");
    }
    #[test]
    fn changed_missing_and_corrupt_installed_cores_are_repaired_from_verified_packages() {
        for installed in [None, Some(b"wrong data!!".as_slice()), Some(b"old".as_slice())] {
            let dir = TestDir::new("asset-repair");
            let card = dir.join("card");
            let staging = card.join("app.new");
            let data = b"signed core!";
            let rel = package(&dir, "cores/core.so", data);
            std::fs::create_dir_all(card.join("app/cores")).unwrap();
            if let Some(old) = installed {
                std::fs::write(card.join("app/cores/core.so"), old).unwrap();
            }
            inventory(&staging, "cores/core.so", data, &rel);
            prepare(&TOOLS, &card, &staging, "file:///u.json", &Cancel::new(), &|_| {}, &|_| Some(u64::MAX)).unwrap();
            assert_eq!(std::fs::read(staging.join("cores/core.so")).unwrap(), data);
            assert_eq!(std::fs::read(card.join("app/cores/core.so")).ok().as_deref(), installed);
        }
    }
    #[test]
    fn bad_package_unsafe_path_and_low_space_leave_active_app_untouched() {
        let dir = TestDir::new("asset-reject");
        let card = dir.join("card");
        std::fs::create_dir_all(card.join("app")).unwrap();
        std::fs::write(card.join("app/RELEASE"), "old").unwrap();
        let rel = package(&dir, "cores/core.so", b"bad");
        let staging = card.join("app.new");
        for (path, free) in [("cores/core.so", u64::MAX), ("../outside", u64::MAX), ("cores/core.so", 0)] {
            let _ = std::fs::remove_dir_all(&staging);
            inventory(&staging, path, b"yes", &rel);
            assert!(prepare(&TOOLS, &card, &staging, "file:///u.json", &Cancel::new(), &|_| {}, &|_| Some(free)).is_err());
            assert_eq!(std::fs::read(card.join("app/RELEASE")).unwrap(), b"old");
            assert!(!card.join("outside").exists());
        }
    }
    #[test]
    fn inherited_helper_can_run_even_if_the_old_execute_bit_was_lost() {
        let dir = TestDir::new("asset-helper-mode");
        let card = dir.join("card");
        let staging = card.join("app.new");
        let data = b"#!/bin/sh\nexit 0\n";
        let rel = package(&dir, "bin/curl", data);
        std::fs::create_dir_all(card.join("app/bin")).unwrap();
        std::fs::write(card.join("app/bin/curl"), data).unwrap();
        inventory(&staging, "bin/curl", data, &rel);
        prepare(&TOOLS, &card, &staging, "file:///u.json", &Cancel::new(), &|_| {}, &|_| Some(u64::MAX)).unwrap();
        assert!(
            Command::new(staging.join("bin/curl")).status().unwrap().success(),
            "the inherited network helper starts"
        );
    }
}
