//! A raw optional pack described by the signed app, downloaded with the updater's transport and checks.
use super::*;

pub fn install(descriptor: &Path, destination: &Path, cancel: &Cancel, progress: &dyn Fn(u64)) -> Result<(), String> {
    install_with(&TOOLS, descriptor, destination, cancel, progress)
}

pub const UNAVAILABLE: &str = "Cheats aren't available in this build. Try updating pocket-ui.";
const WRITE: &str = "Cheats couldn't be saved. Check free space on the SD card.";

/// Read on a worker: the description belongs to the signed app, never to a network catalogue.
pub fn description(descriptor: &Path) -> Result<Release, String> {
    let json = std::fs::read(descriptor).map_err(|_| UNAVAILABLE)?;
    if json.len() > 16 << 10 { return Err(UNAVAILABLE.into()); }
    let rel: Release = serde_json::from_slice(&json).map_err(|_| UNAVAILABLE)?;
    if rel.size == 0 || rel.size > 80_000_000 || rel.sha256.len() != 64 || !rel.sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(UNAVAILABLE.into());
    }
    Ok(rel)
}

pub fn space_needed(size: u64) -> u64 { size.saturating_add(32 << 20) }

/// Explicitly replace an older database. The old pack remains usable until the checked final rename.
pub fn replace(descriptor: &Path, destination: &Path, cancel: &Cancel, progress: &dyn Fn(u64)) -> Result<(), String> {
    install_checked(&TOOLS, descriptor, destination, cancel, progress)
}

fn install_with(tools: &Tools, descriptor: &Path, destination: &Path, cancel: &Cancel, progress: &dyn Fn(u64)) -> Result<(), String> {
    if crate::cheat_download::installed(&[destination.to_path_buf()]) { return Ok(()); }
    install_checked(tools, descriptor, destination, cancel, progress)
}

fn install_checked(tools: &Tools, descriptor: &Path, destination: &Path, cancel: &Cancel, progress: &dyn Fn(u64)) -> Result<(), String> {
    let rel = description(descriptor)?;
    let parent = destination.parent().ok_or(WRITE)?;
    std::fs::create_dir_all(parent).map_err(|_| WRITE)?;
    let part = destination.with_file_name(format!(".cheats-{}.part", rel.sha256.to_lowercase()));
    // Checkpointing makes a complete new card copy before replacing the old partial.
    if crate::starter::free_bytes(parent).is_some_and(|free| free < space_needed(rel.size)) { return Err(WRITE.into()); }
    transfer(tools, &rel, descriptor, &part, cancel, progress).map_err(|error| -> String {
        if error == DAMAGED { "The cheat download is damaged. Try again.".into() }
        else if error == CARD_WRITE { WRITE.into() }
        else if error == MEMORY { MEMORY.into() }
        else if cancel.is_cancelled() { "Cheat download stopped. Try again.".into() }
        else { crate::cheat_download::OFFLINE.into() }
    })?;
    if !crate::cheat_download::installed(std::slice::from_ref(&part)) {
        let _ = std::fs::remove_file(&part);
        return Err("The cheat download is damaged. Try again.".into());
    }
    std::fs::File::open(&part).and_then(|f| f.sync_all()).map_err(|_| WRITE)?;
    std::fs::rename(&part, destination).map_err(|_| WRITE)?;
    crate::fsutil::sync_dir(parent);
    Ok(())
}

const MEMORY: &str = "There isn't enough free memory for cheats. Close the game and try again.";

/// The same RAM/network/checkpoint order for small pinned optional game archives.
pub(crate) fn download_data(rel: &Release, part: &Path, cancel: &Cancel, progress: &dyn Fn(u64)) -> Result<(), String> {
    transfer(&TOOLS, rel, Path::new("/"), part, cancel, progress)
}

fn transfer(tools: &Tools, rel: &Release, descriptor: &Path, part: &Path, cancel: &Cancel, progress: &dyn Fn(u64)) -> Result<(), String> {
    let url = format!("file://{}", descriptor.display());
    #[cfg(feature = "brick")]
    {
        // Use a different RAM directory from app updates so neither cleanup can delete the other's files.
        let mut ram = super::brick_io::Ram::new(part)?;
        ram.reserve(rel.size).map_err(|_| MEMORY)?;
        let temp = ram.dir.join("cheats.pack");
        if part.exists() { std::fs::copy(part, &temp).map_err(|_| CARD_WRITE)?; }
        let result = download_checked(tools, rel, &url, &temp, cancel, progress);
        // curl has exited (also on cancellation). Checkpoint partial bytes without concurrent Wi-Fi/SD writes.
        if temp.is_file() {
            cancel.set_phase(WorkerPhase::SavingCheats);
            let checkpoint = part.with_extension("writing");
            crate::cheat_download::copy_to_card(&temp, &checkpoint).map_err(|_| CARD_WRITE)?;
            std::fs::rename(&checkpoint, part).map_err(|_| CARD_WRITE)?;
            crate::fsutil::sync_dir(part.parent().unwrap());
        } else if part.exists() {
            // A complete-but-damaged download has been discarded; don't retry its previous corrupt prefix.
            std::fs::remove_file(part).map_err(|_| CARD_WRITE)?;
        }
        result?;
        // Also check the card copy, before publishing it as usable offline data.
        if std::fs::metadata(part).map_or(0, |m| m.len()) != rel.size || store::sha256_file(part).map_err(|_| CARD_WRITE)? != rel.sha256.to_lowercase() {
            let _ = std::fs::remove_file(part);
            return Err(DAMAGED.into());
        }
        Ok(())
    }
    #[cfg(not(feature = "brick"))]
    download_checked(tools, rel, &url, part, cancel, progress)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;

    fn fixture(dir: &Path) -> (PathBuf, PathBuf, Vec<u8>, Release) {
        let bytes = crate::cheat_pack::build(&[("Nintendo - Game Boy", vec![("Tetris.cht".into(), b"cheat0_code = \"01\"\n".to_vec())])]);
        let source = dir.join("source.pack");
        std::fs::write(&source, &bytes).unwrap();
        let rel = Release { version: "1".into(), url: format!("file://{}", source.display()), mirrors: vec![], preferred_sources: vec![],
            sha256: store::sha256_hex(&bytes), size: bytes.len() as u64, notes: String::new() };
        let descriptor = dir.join("download.json");
        std::fs::write(&descriptor, serde_json::to_vec(&rel).unwrap()).unwrap();
        (descriptor, dir.join("storage/cht.pack"), bytes, rel)
    }

    #[test]
    fn optional_cheats_resume_verify_and_then_work_without_a_server() {
        let dir = TestDir::new("cheats-download");
        let (descriptor, dest, bytes, rel) = fixture(&dir);
        std::fs::create_dir_all(dest.parent().unwrap()).unwrap();
        let part = dest.with_file_name(format!(".cheats-{}.part", rel.sha256));
        std::fs::write(&part, &bytes[..19]).unwrap();
        let seen = std::sync::Mutex::new(Vec::new());
        install(&descriptor, &dest, &Cancel::new(), &|n| seen.lock().unwrap().push(n)).unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), bytes);
        assert!(!part.exists());
        assert!(seen.lock().unwrap().contains(&(bytes.len() as u64)), "progress reaches the announced size");
        std::fs::remove_file(dir.join("source.pack")).unwrap();
        std::fs::remove_file(&descriptor).unwrap();
        install(&descriptor, &dest, &Cancel::new(), &|_| panic!("already installed: no download")).unwrap();
        assert!(crate::cheat_download::available(&[dest], "gb"));
    }

    #[test]
    fn wrong_size_or_sha_never_publishes_the_pack() {
        for wrong_size in [true, false] {
            let dir = TestDir::new("cheats-bad-download");
            let (descriptor, dest, _, mut rel) = fixture(&dir);
            if wrong_size { rel.size += 1; } else { rel.sha256 = "0".repeat(64); }
            std::fs::write(&descriptor, serde_json::to_vec(&rel).unwrap()).unwrap();
            assert!(install(&descriptor, &dest, &Cancel::new(), &|_| {}).is_err());
            assert!(!dest.exists());
        }
    }

    #[test]
    fn explicit_update_keeps_old_codes_on_failure_and_replaces_them_only_after_verification() {
        let dir = TestDir::new("cheats-replace");
        let (descriptor, dest, bytes, mut rel) = fixture(&dir);
        install(&descriptor, &dest, &Cancel::new(), &|_| {}).unwrap();
        let newer = crate::cheat_pack::build(&[("Nintendo - Game Boy", vec![("Tetris.cht".into(),
            b"cheat0_code = \"02\"\ncheat1_code = \"03\"\n".to_vec())])]);
        std::fs::write(dir.join("source.pack"), &newer).unwrap();
        rel.size = newer.len() as u64;
        rel.sha256 = "0".repeat(64);
        std::fs::write(&descriptor, serde_json::to_vec(&rel).unwrap()).unwrap();
        assert!(replace(&descriptor, &dest, &Cancel::new(), &|_| {}).is_err());
        assert_eq!(std::fs::read(&dest).unwrap(), bytes, "old working database survives a corrupt update");
        rel.sha256 = store::sha256_hex(&newer);
        std::fs::write(&descriptor, serde_json::to_vec(&rel).unwrap()).unwrap();
        replace(&descriptor, &dest, &Cancel::new(), &|_| {}).unwrap();
        assert_eq!(std::fs::read(dest).unwrap(), newer);
    }

    #[test]
    fn offline_failure_keeps_partial_bytes_and_explains_wifi_once() {
        let dir = TestDir::new("cheats-offline");
        let (descriptor, dest, bytes, rel) = fixture(&dir);
        std::fs::create_dir_all(dest.parent().unwrap()).unwrap();
        let part = dest.with_file_name(format!(".cheats-{}.part", rel.sha256));
        std::fs::write(&part, &bytes[..19]).unwrap();
        let curl = dir.join("offline-curl");
        crate::testutil::write_executable(&curl, "#!/bin/sh\nexit 6\n");
        let tools = Tools { curl: curl.to_str().unwrap(), ..TOOLS };
        assert_eq!(install_with(&tools, &descriptor, &dest, &Cancel::new(), &|_| {}).unwrap_err(), crate::cheat_download::OFFLINE);
        assert_eq!(std::fs::read(part).unwrap(), bytes[..19]);
        assert!(!dest.exists());
    }
    #[cfg(feature = "brick")]
    #[test]
    fn cheat_network_output_stays_in_ram_until_curl_exits() {
        let dir = TestDir::new("cheats-network-separation");
        let (descriptor, dest, bytes, _) = fixture(&dir);
        let curl = dir.join("ram-curl");
        crate::testutil::write_executable(&curl, format!(r#"#!/bin/sh
while [ $# -gt 0 ]; do [ "$1" = -o ] && out=$2; url=$1; shift; done
case "$out" in /tmp/pocket-ui-update-*/cheats.pack) ;; *) exit 71;; esac
[ ! -e '{0}' ] || exit 72
cp "${{url#file://}}" "$out"
[ ! -e '{0}' ] || exit 73
"#, dest.display()));
        let tools = Tools { curl: curl.to_str().unwrap(), ..TOOLS };
        install_with(&tools, &descriptor, &dest, &Cancel::new(), &|_| {}).unwrap();
        assert_eq!(std::fs::read(dest).unwrap(), bytes);
    }

    #[test]
    fn missing_or_invalid_download_description_is_visible_and_never_installs() {
        let dir = TestDir::new("cheats-no-description");
        let (descriptor, dest, _, _) = fixture(&dir);
        for invalid in [None, Some("{}"), Some("{\"size\":0}")] {
            if let Some(body) = invalid { std::fs::write(&descriptor, body).unwrap(); }
            else { std::fs::remove_file(&descriptor).unwrap(); }
            let error = install(&descriptor, &dest, &Cancel::new(), &|_| {}).unwrap_err();
            assert!(error.contains("Try updating pocket-ui"), "{error}");
            assert!(!dest.exists());
        }
    }

    #[test]
    fn cancelling_checkpoints_partial_bytes_and_the_next_try_continues() {
        let dir = TestDir::new("cheats-cancel");
        let (descriptor, dest, bytes, rel) = fixture(&dir);
        let curl = dir.join("slow-curl");
        crate::testutil::write_executable(&curl, r#"#!/bin/sh
while [ $# -gt 0 ]; do [ "$1" = -o ] && out=$2; url=$1; shift; done
head -c 19 "${url#file://}" > "$out"
exec sleep 5
"#);
        let tools = Tools { curl: curl.to_str().unwrap(), ..TOOLS };
        let cancel = Cancel::new();
        let result = install_with(&tools, &descriptor, &dest, &cancel, &|n| if n > 0 { cancel.cancel(); });
        assert!(result.is_err());
        assert!(!dest.exists());
        let part = dest.with_file_name(format!(".cheats-{}.part", rel.sha256));
        assert_eq!(std::fs::read(part).unwrap(), bytes[..19]);
        install(&descriptor, &dest, &Cancel::new(), &|_| {}).unwrap();
        assert_eq!(std::fs::read(dest).unwrap(), bytes);
    }

    #[test]
    fn corrupt_resumed_prefix_is_replaced_by_a_verified_download() {
        let dir = TestDir::new("cheats-corrupt-prefix");
        let (descriptor, dest, bytes, rel) = fixture(&dir);
        std::fs::create_dir_all(dest.parent().unwrap()).unwrap();
        std::fs::write(dest.with_file_name(format!(".cheats-{}.part", rel.sha256)), b"corrupt prefix").unwrap();
        install(&descriptor, &dest, &Cancel::new(), &|_| {}).unwrap();
        assert_eq!(std::fs::read(dest).unwrap(), bytes);
    }

}
