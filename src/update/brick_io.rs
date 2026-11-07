//! The firmware shares retry code between Wi-Fi SDIO and the SD card. Keep update downloads in RAM,
//! finish all network work first, and pace every bulk write in the worker thread.
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;
#[cfg(test)]
use std::time::Instant;

pub(crate) const WRITE_RATE: u64 = 2 << 20;
const CHUNK: usize = 256 << 10;
const RAM_LIMIT: u64 = 320 << 20;
const RAM_SPARE: u64 = 128 << 20;

pub(crate) struct Pacer {
    rate: u64,
}
static BULK_WRITE: std::sync::Mutex<()> = std::sync::Mutex::new(());
impl Pacer {
    pub(crate) fn new() -> Self { Self { rate: WRITE_RATE } }
    pub(crate) fn write(&mut self, bytes: &[u8], to: &mut std::fs::File) -> io::Result<()> {
        // One budget across concurrent phone transfers, backups and update extraction. Read network input
        // outside this lock; no completed chunk earns credit for a later burst.
        let _slot = BULK_WRITE.lock().unwrap_or_else(|e| e.into_inner());
        for chunk in bytes.chunks(CHUNK) {
            std::thread::sleep(Duration::from_secs_f64(chunk.len() as f64 / self.rate as f64));
            to.write_all(chunk)?;
            to.sync_all()?;
        }
        Ok(())
    }
    pub(crate) fn copy(&mut self, from: &mut dyn Read, to: &mut std::fs::File) -> io::Result<u64> {
        let mut buf = vec![0; CHUNK];
        let mut total = 0;
        loop {
            let n = match from.read(&mut buf) {
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                result => result?,
            };
            if n == 0 { break; }
            self.write(&buf[..n], to)?;
            total += n as u64;
        }
        Ok(total)
    }
}

/// Linux ioprio applies to this worker thread, never to the UI. Restore its previous policy on exit.
pub(crate) struct IdleIo(i32);
impl IdleIo {
    pub(crate) fn enter() -> io::Result<Self> {
        #[cfg(target_os = "linux")]
        {
            // SAFETY: scalar syscall arguments; pid 0 selects the calling thread.
            let previous = unsafe { libc::syscall(libc::SYS_ioprio_get, 1, 0) };
            if previous < 0 { return Err(io::Error::last_os_error()); }
            if unsafe { libc::syscall(libc::SYS_ioprio_set, 1, 0, 3 << 13) } < 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(Self(previous as i32))
        }
        #[cfg(not(target_os = "linux"))]
        Ok(Self(0))
    }
}
impl Drop for IdleIo {
    fn drop(&mut self) {
        #[cfg(target_os = "linux")]
        // SAFETY: restores the calling thread's scalar policy saved above.
        unsafe { libc::syscall(libc::SYS_ioprio_set, 1, 0, self.0); }
    }
}

pub(super) fn download_path(card: &Path, version: &str) -> PathBuf {
    let key = crate::store::sha256_hex(card.as_os_str().as_encoded_bytes());
    Path::new("/tmp").join(format!("pocket-ui-update-{}", &key[..16])).join(format!("download-{version}.tar.gz"))
}

pub(super) struct Ram {
    pub(super) dir: PathBuf,
    reserved: u64,
    pub(super) resume: Option<PathBuf>,
}
impl Ram {
    pub(super) fn new(card: &Path) -> Result<Self, String> {
        use std::os::unix::fs::DirBuilderExt;
        let dir = download_path(card, "").parent().unwrap().to_path_buf();
        match std::fs::DirBuilder::new().mode(0o700).create(&dir) {
            Ok(()) => (),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => (),
            Err(e) => return Err(e.to_string()),
        }
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        if !std::fs::symlink_metadata(&dir).is_ok_and(|m| m.is_dir() && m.uid() == unsafe { libc::geteuid() } && m.permissions().mode() & 0o777 == 0o700) {
            return Err("Unsafe update temporary folder.".into());
        }
        // A legacy card's .update is never used for network output. /tmp is tmpfs on the Brick.
        Ok(Self { dir, reserved: 0, resume: None })
    }
    pub(super) fn reserve(&mut self, bytes: u64) -> Result<(), String> {
        self.reserved = self.reserved.saturating_add(bytes);
        let available = std::fs::read_to_string("/proc/meminfo").ok().and_then(|s| {
            s.lines().find_map(|l| l.strip_prefix("MemAvailable:").and_then(|v| v.split_whitespace().next()?.parse::<u64>().ok())).map(|n| n * 1024)
        });
        let occupied: u64 = std::fs::read_dir(&self.dir).into_iter().flatten().flatten()
            .filter_map(|e| e.metadata().ok()).map(|m| m.len()).sum();
        let need = self.reserved.saturating_sub(occupied);
        if self.reserved > RAM_LIMIT || available.is_some_and(|n| n < need.saturating_add(RAM_SPARE))
            || crate::starter::free_bytes(&self.dir).is_some_and(|n| n < need.saturating_add(16 << 20)) {
            return Err("There isn't enough free memory for this update. Restart the device and try again. Nothing was changed.".into());
        }
        Ok(())
    }
}
impl Drop for Ram {
    fn drop(&mut self) {
        if let Ok(entries) = std::fs::read_dir(&self.dir) {
            for e in entries.flatten() {
                if self.resume.as_ref() != Some(&e.path()) { let _ = std::fs::remove_file(e.path()); }
            }
        }
        let _ = std::fs::remove_dir(&self.dir);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bulk_writes_are_paced_and_complete() {
        let dir = crate::testutil::TestDir::new("brick-paced-write");
        let path = dir.join("data");
        let mut file = std::fs::File::create(&path).unwrap();
        let data = vec![83; CHUNK * 2 + 17];
        let start = Instant::now();
        let mut pacer = Pacer::new();
        assert_eq!(pacer.copy(&mut data.as_slice(), &mut file).unwrap(), data.len() as u64);
        assert!(start.elapsed() >= Duration::from_secs_f64(data.len() as f64 / WRITE_RATE as f64));
        assert_eq!(std::fs::read(path).unwrap(), data);
    }
    #[test]
    fn oversized_update_is_refused_without_writing_the_card() {
        let dir = crate::testutil::TestDir::new("brick-ram-bound");
        let card = dir.join("absent-card");
        let mut ram = Ram::new(&card).unwrap();
        assert!(ram.reserve(RAM_LIMIT + 1).is_err());
        assert!(!card.exists());
        std::fs::remove_dir_all(&ram.dir).unwrap();
    }
}
