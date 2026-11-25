//! Fixed files installed by the SD image outside the app's release directory.
//! Their payloads are part of the verified app binary, so OTA and card updates use the same bytes.

use crate::{fsutil, store};
use std::{fs, io::{self, Read}, path::Path};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

const GAMES_CARD: &[u8] = include_bytes!("../../deploy/image/games-card.sh");
const PANELS_README: &[u8] = include_bytes!("../../deploy/image/r36s-panels-README.txt");

pub(super) fn refresh(root: &Path, flash: &Path, writable_flash: impl FnOnce(&mut dyn FnMut() -> io::Result<()>) -> io::Result<()>) {
    let regular = |path: &Path| fs::symlink_metadata(path).is_ok_and(|m| m.is_file());
    let directory = |path: &Path| fs::symlink_metadata(path).is_ok_and(|m| m.is_dir());
    if !directory(root) || !directory(flash) || !directory(&flash.join("pocket-ui-info")) || !regular(&flash.join("mount-storage.sh")) {
        return;
    }
    let games = root.join("games-card.sh");
    if let Err(e) = replace_installed(&games, GAMES_CARD, Some(0o755)) {
        eprintln!("image update: {}: {e}", games.display());
    }
    let panels = flash.join("r36s-panels");
    if directory(&panels) {
        let readme = panels.join("README.txt");
        if let Err(e) = writable_flash(&mut || replace_installed(&readme, PANELS_README, None)) {
            eprintln!("image update: {}: {e}", readme.display());
        }
    }
}

fn replace_installed(path: &Path, bytes: &[u8], mode: Option<u32>) -> io::Result<()> {
    let mut file = match fs::OpenOptions::new().read(true).custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK).open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e),
    };
    let metadata = file.metadata()?;
    if !metadata.is_file() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "image target is not a regular file"));
    }
    let mut old = Vec::new();
    file.read_to_end(&mut old)?;
    let old_mode = mode.map(|_| metadata.permissions().mode() & 0o777);
    if old == bytes && old_mode == mode {
        return Ok(());
    }
    let backup = fsutil::backup_path(path);
    match fs::symlink_metadata(&backup) {
        Ok(meta) if meta.is_file() => (),
        Ok(_) => return Err(io::Error::new(io::ErrorKind::InvalidInput, "image backup is not a regular file")),
        Err(e) if e.kind() == io::ErrorKind::NotFound => (),
        Err(e) => return Err(e),
    }
    fsutil::write_verified_atomic(&backup, &old, &store::sha256_hex(&old), old_mode)?;
    fsutil::write_verified_atomic(path, bytes, &store::sha256_hex(bytes), mode)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refresh_replaces_only_installed_image_files_and_keeps_the_previous_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("pocket-ui");
        let flash = dir.path().join("flash");
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(flash.join("r36s-panels")).unwrap();
        let games = root.join("games-card.sh");
        let readme = flash.join("r36s-panels/README.txt");
        let overlay = flash.join("r36s-panels/panel.dtbo");
        fs::write(&games, b"previous games script").unwrap();
        fs::write(&readme, b"previous README").unwrap();
        fs::write(&overlay, b"panel overlay").unwrap();
        refresh(&root, &flash, |_| panic!("without an image marker"));
        assert_eq!(fs::read(&games).unwrap(), b"previous games script");

        fs::create_dir(flash.join("pocket-ui-info")).unwrap();
        fs::write(flash.join("mount-storage.sh"), b"boot hook").unwrap();
        for _ in 0..2 {
            refresh(&root, &flash, |act| act());
            assert_eq!(fs::read(&games).unwrap(), GAMES_CARD);
            assert_eq!(fs::metadata(&games).unwrap().permissions().mode() & 0o777, 0o755);
            assert_eq!(fs::read(fsutil::backup_path(&games)).unwrap(), b"previous games script");
            assert_eq!(fs::read(&readme).unwrap(), PANELS_README);
            assert_eq!(fs::read(fsutil::backup_path(&readme)).unwrap(), b"previous README");
            assert_eq!(fs::read(&overlay).unwrap(), b"panel overlay");
        }

        fs::remove_file(readme).unwrap();
        refresh(&root, &flash, |act| act());
        assert!(!flash.join("r36s-panels/README.txt").exists());
    }

    #[test]
    fn checksum_mismatch_or_unsafe_backup_leaves_the_installed_file_intact() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("games-card.sh");
        fs::write(&target, b"old").unwrap();
        assert!(fsutil::write_verified_atomic(&target, b"different", &store::sha256_hex(b"new"), Some(0o755)).is_err());
        assert_eq!(fs::read(&target).unwrap(), b"old");
        fsutil::fail_next_write();
        assert!(replace_installed(&target, GAMES_CARD, Some(0o755)).is_err());
        assert_eq!(fs::read(&target).unwrap(), b"old");
        let alias = dir.path().join("alias");
        std::os::unix::fs::symlink(&target, &alias).unwrap();
        assert!(replace_installed(&alias, GAMES_CARD, Some(0o755)).is_err());
        std::os::unix::fs::symlink(&target, fsutil::backup_path(&target)).unwrap();
        assert!(replace_installed(&target, GAMES_CARD, Some(0o755)).is_err());
        assert_eq!(fs::read(&target).unwrap(), b"old");
    }
}
