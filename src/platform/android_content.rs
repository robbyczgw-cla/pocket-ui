//! A process-local, original-name path for a SAF descriptor. Own it until after core destruction.
use std::fs::{self, File};
use std::io::{self, Read, Seek, SeekFrom};
use std::os::fd::AsRawFd;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

pub struct DirectGame {
    path: PathBuf,
    folder: PathBuf,
    // Kept even when the core closes and reopens its own handle, e.g. PSP disc seeking.
    _source: File,
}
impl DirectGame {
    pub fn new(source: File, root: &Path, name: &str) -> io::Result<Self> {
        if name.is_empty() || name == "." || name == ".." || name.contains(['/', '\\']) {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, crate::i18n::tr("Invalid ROM name")));
        }
        static TURN: AtomicU64 = AtomicU64::new(0);
        fs::create_dir_all(root)?;
        // Distinct launcher/player paths: /proc/self means the process that opens the alias.
        let folder = loop {
            let folder = root.join(format!("{}-{}", std::process::id(), TURN.fetch_add(1, Ordering::Relaxed)));
            match fs::create_dir(&folder) {
                Ok(()) => break folder,
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
        };
        let path = folder.join(name);
        let game = Self { path, folder, _source: source };
        (|| -> io::Result<()> {
            std::os::unix::fs::symlink(format!("/proc/self/fd/{}", game._source.as_raw_fd()), &game.path)?;
            // Verify reopening through the actual alias, not just lseek on the provider's original fd.
            let mut reopened = File::open(&game.path)?;
            let size = reopened.seek(SeekFrom::End(0))?;
            if size == 0 { return Err(io::Error::new(io::ErrorKind::UnexpectedEof, crate::i18n::tr("Empty ROM"))); }
            reopened.seek(SeekFrom::Start(size - 1))?;
            reopened.read_exact(&mut [0])?;
            reopened.seek(SeekFrom::Start(0))?;
            reopened.read_exact(&mut [0])?;
            if size > 64 << 20 {
                let stamp = source_stamp(&mut reopened, root)?;
                // Keep GameCopy's CRC identity: an old large-game moment resumes without rewriting its mark.
                fs::write(game.folder.join(".stage"), format!("{} {} {}\n", stamp.file.size, stamp.file.mtime_ns / 1_000_000, stamp.crc))?;
            }
            Ok(())
        })()?;
        Ok(game)
    }
    pub fn path(&self) -> &Path { &self.path }
}
impl Drop for DirectGame {
    fn drop(&mut self) {
        for path in [&self.path, &self.folder.join(".stage")] {
            if let Err(error) = fs::remove_file(path) {
                if error.kind() != io::ErrorKind::NotFound { eprintln!("direct game alias cleanup: {error}"); }
            }
        }
        if let Err(error) = fs::remove_dir(&self.folder) {
            if error.kind() != io::ErrorKind::NotFound { eprintln!("direct game directory cleanup: {error}"); }
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
struct Stamp { file: Identity, samples: u32, crc: u32 }

#[derive(PartialEq, Eq, serde::Serialize, serde::Deserialize)]
struct Identity { size: u64, mtime_ns: u128, ctime_ns: i128, device: u64, inode: u64 }

fn identity(file: &File) -> io::Result<Identity> {
    let meta = file.metadata()?;
    let time = meta.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_nanos());
    Ok(Identity { size: meta.len(), mtime_ns: time, ctime_ns: i128::from(meta.ctime()) * 1_000_000_000 + i128::from(meta.ctime_nsec()), device: meta.dev(), inode: meta.ino() })
}

fn source_stamp(file: &mut File, root: &Path) -> io::Result<Stamp> {
    let version = identity(file)?;
    let size = version.size;
    let mut buffer = [0u8; 65536];
    let mut samples = crc32fast::Hasher::new();
    // Same 16 evenly spread positions as the fallback copy check. A changed timestamp also invalidates the cache.
    for i in 0..16 {
        let at = size.saturating_sub(4096) * i / 15;
        file.seek(SeekFrom::Start(at))?;
        let n = (size - at).min(4096) as usize;
        file.read_exact(&mut buffer[..n])?;
        samples.update(&buffer[..n]);
    }
    let samples = samples.finalize();
    let cache = root.join("source.json");
    if let Some(stamp) = fs::read(&cache).ok().and_then(|bytes| serde_json::from_slice::<Stamp>(&bytes).ok()) {
        // mtime alone can be restored by patching/copy tools. Also require change time and file identity;
        // providers without those cannot safely reuse a checksum for a live document.
        if version.mtime_ns > 0 && version.ctime_ns > 0 && version.inode > 0 && stamp.file == version && stamp.samples == samples {
            return Ok(stamp);
        }
    }
    // Once per new/changed large document, read its checksum without writing any ROM payload.
    let mut crc = crc32fast::Hasher::new();
    file.seek(SeekFrom::Start(0))?;
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 { break; }
        crc.update(&buffer[..n]);
    }
    if identity(file)? != version {
        return Err(io::Error::new(io::ErrorKind::InvalidData, crate::i18n::tr("Game changed while opening")));
    }
    let stamp = Stamp { file: version, samples, crc: crc.finalize() };
    // A lost/torn cache is disposable and is recomputed. A failed cache write must not force a ROM copy.
    if let Ok(bytes) = serde_json::to_vec(&stamp) { let _ = crate::fsutil::write_atomic(&cache, &bytes); }
    Ok(stamp)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_name_seek_reopen_and_cleanup_without_a_rom_copy() {
        let root = std::env::temp_dir().join(format!("no-copy-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let source = root.join("card.gba");
        fs::write(&source, b"0123456789").unwrap();
        let game = DirectGame::new(File::open(&source).unwrap(), &root.join("direct"), "Original game.gba").unwrap();
        assert_eq!(game.path().file_name().unwrap(), "Original game.gba");
        assert!(fs::symlink_metadata(game.path()).unwrap().file_type().is_symlink(), "only an alias is stored");
        for _ in 0..3 {
            let mut reopened = File::open(game.path()).unwrap();
            reopened.seek(SeekFrom::Start(8)).unwrap();
            let mut tail = String::new(); reopened.read_to_string(&mut tail).unwrap();
            assert_eq!(tail, "89");
        }
        let alias = game.path().to_path_buf();
        drop(game);
        assert!(!alias.exists());
        assert_eq!(fs::read(&source).unwrap(), b"0123456789");
        fs::remove_file(source).unwrap(); fs::remove_dir(root.join("direct")).unwrap(); fs::remove_dir(root).unwrap();
    }
    #[test]
    fn a_non_seekable_provider_is_refused_without_payload_or_descriptor_leaks() {
        use std::os::fd::FromRawFd;
        use std::os::unix::net::UnixStream;
        use std::os::fd::IntoRawFd;
        let root = std::env::temp_dir().join(format!("no-copy-pipe-{}", std::process::id()));
        let (source, _writer) = UnixStream::pair().unwrap();
        let file = unsafe { File::from_raw_fd(source.into_raw_fd()) };
        assert!(DirectGame::new(file, &root, "Game.gba").is_err());
        assert_eq!(fs::read_dir(&root).unwrap().count(), 0);
        fs::remove_dir(root).unwrap();
    }
}
