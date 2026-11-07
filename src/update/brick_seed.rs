//! Seed bundled BIOS data without rewriting identical files or following card symlinks.
use crate::update::brick_io::{IdleIo, Pacer};
use std::ffi::{CString, OsStr, OsString};
use std::fs::File;
use std::io::{self, Read};
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::ffi::OsStrExt;
use std::path::{Component, Path};
use std::sync::atomic::{AtomicU64, Ordering};

const MARKER: &str = ".pocket-ui-data";
const COMPARE_CHUNK: usize = 256 << 10;
static TEMP_SEQ: AtomicU64 = AtomicU64::new(0);

fn invalid() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, "BIOS seed path is a link or not a regular file/directory")
}
fn name(s: &OsStr) -> io::Result<CString> {
    CString::new(s.as_bytes()).map_err(|_| invalid())
}
fn sync_card() {
    // SAFETY: sync takes no arguments. Unlike directory fsync it also works on the card's FAT.
    unsafe { libc::sync() };
}

/// Hold every traversed directory open; openat with O_NOFOLLOW also prevents an
/// ancestor swapped to a symlink after validation from redirecting later writes.
struct Dir(File);
impl Dir {
    fn node(&self, child: &OsStr) -> io::Result<Option<File>> {
        let child = name(child)?;
        // NONBLOCK prevents a hostile FIFO/device from hanging before the type check.
        // SAFETY: valid directory descriptor and NUL-terminated single entry name.
        let fd = unsafe {
            libc::openat(
                self.0.as_raw_fd(),
                child.as_ptr(),
                libc::O_RDONLY | libc::O_CLOEXEC | libc::O_NOFOLLOW | libc::O_NONBLOCK,
            )
        };
        if fd < 0 {
            let e = io::Error::last_os_error();
            return if e.kind() == io::ErrorKind::NotFound { Ok(None) } else { Err(e) };
        }
        // SAFETY: openat returned a new owned descriptor.
        let file = unsafe { File::from_raw_fd(fd) };
        let meta = file.metadata()?;
        if !meta.is_file() && !meta.is_dir() {
            return Err(invalid());
        }
        Ok(Some(file))
    }
    fn child(&self, child: &OsStr, create: bool) -> io::Result<Option<Self>> {
        if let Some(file) = self.node(child)? {
            return if file.metadata()?.is_dir() { Ok(Some(Self(file))) } else { Err(invalid()) };
        }
        if !create {
            return Ok(None);
        }
        let child_c = name(child)?;
        // SAFETY: valid directory descriptor and single entry name. A concurrent
        // creator is accepted only if node() then opens a real, non-symlink directory.
        if unsafe { libc::mkdirat(self.0.as_raw_fd(), child_c.as_ptr(), 0o755) } != 0 {
            let e = io::Error::last_os_error();
            if e.kind() != io::ErrorKind::AlreadyExists {
                return Err(e);
            }
        }
        self.child(child, false)?.map(Some).ok_or_else(invalid)
    }
    fn root(path: &Path, create: bool) -> io::Result<Option<Self>> {
        // Reject parent traversal even beyond a missing path component.
        if path.components().any(|c| matches!(c, Component::ParentDir | Component::Prefix(_))) {
            return Err(invalid());
        }
        let mut dir = Self(File::open(if path.is_absolute() { "/" } else { "." })?);
        for component in path.components() {
            if let Component::Normal(part) = component {
                let Some(next) = dir.child(part, create)? else {
                    return Ok(None);
                };
                dir = next;
            }
        }
        Ok(Some(dir))
    }
    fn descend(&self, parts: &[OsString], create: bool) -> io::Result<Option<Self>> {
        let mut dir = Self(self.0.try_clone()?);
        for part in parts {
            let Some(next) = dir.child(part, create)? else {
                return Ok(None);
            };
            dir = next;
        }
        Ok(Some(dir))
    }
    fn unlink(&self, child: &OsStr) -> io::Result<()> {
        let child = name(child)?;
        // SAFETY: valid directory descriptor and single entry name; no links followed.
        if unsafe { libc::unlinkat(self.0.as_raw_fd(), child.as_ptr(), 0) } == 0 {
            return Ok(());
        }
        let e = io::Error::last_os_error();
        if e.kind() == io::ErrorKind::NotFound {
            Ok(())
        } else {
            Err(e)
        }
    }
}

fn same(source: &mut File, dest: &mut File) -> io::Result<bool> {
    let size = source.metadata()?.len();
    if size != dest.metadata()?.len() {
        return Ok(false);
    }
    let mut a = vec![0; COMPARE_CHUNK];
    let mut b = vec![0; COMPARE_CHUNK];
    let mut remaining = size;
    while remaining != 0 {
        let n = remaining.min(COMPARE_CHUNK as u64) as usize;
        source.read_exact(&mut a[..n])?;
        dest.read_exact(&mut b[..n])?;
        if a[..n] != b[..n] {
            return Ok(false);
        }
        remaining -= n as u64;
    }
    // Also notice a file that grew during comparison.
    Ok(source.read(&mut a[..1])? == 0 && dest.read(&mut b[..1])? == 0)
}

fn collect(source: &Dir, dest: Option<&Dir>, prefix: &mut Vec<OsString>, changed: &mut Vec<Vec<OsString>>) -> io::Result<()> {
    // This proc link is to OUR held descriptor, never a card-provided link.
    let mut names = std::fs::read_dir(format!("/proc/self/fd/{}", source.0.as_raw_fd()))?
        .map(|e| e.map(|e| e.file_name()))
        .collect::<io::Result<Vec<_>>>()?;
    names.sort();
    for child in names {
        if prefix.is_empty() && child.as_os_str() == OsStr::new(MARKER) {
            return Err(invalid());
        }
        let mut from = source.node(&child)?.ok_or_else(invalid)?;
        let target = match dest {
            Some(d) => d.node(&child)?,
            None => None,
        };
        prefix.push(child);
        if from.metadata()?.is_dir() {
            let target_dir = match target {
                Some(f) if f.metadata()?.is_dir() => Some(Dir(f)),
                Some(_) => return Err(invalid()),
                None => None,
            };
            collect(&Dir(from), target_dir.as_ref(), prefix, changed)?;
        } else {
            let identical = match target {
                Some(mut f) if f.metadata()?.is_file() => same(&mut from, &mut f)?,
                Some(_) => return Err(invalid()),
                None => false,
            };
            if !identical {
                changed.push(prefix.clone());
            }
        }
        prefix.pop();
    }
    Ok(())
}

/// A temporary belongs to its held destination directory, including on failure.
struct Temp<'a> {
    dir: &'a Dir,
    name: OsString,
    file: File,
    published: bool,
}
impl<'a> Temp<'a> {
    fn new(dir: &'a Dir, target: &OsStr) -> io::Result<Self> {
        for _ in 0..128 {
            let temp = OsString::from(format!(
                ".pocket-ui-seed-{}-{}.tmp",
                std::process::id(),
                TEMP_SEQ.fetch_add(1, Ordering::Relaxed)
            ));
            if temp.as_os_str() == target {
                continue;
            }
            let temp_c = name(&temp)?;
            // SAFETY: exclusive creation in the held directory; never open an existing link/file.
            let fd = unsafe {
                libc::openat(
                    dir.0.as_raw_fd(),
                    temp_c.as_ptr(),
                    libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                    0o644,
                )
            };
            if fd >= 0 {
                // SAFETY: openat returned a new owned descriptor.
                return Ok(Self {
                    dir,
                    name: temp,
                    file: unsafe { File::from_raw_fd(fd) },
                    published: false,
                });
            }
            let e = io::Error::last_os_error();
            if e.kind() != io::ErrorKind::AlreadyExists {
                return Err(e);
            }
        }
        Err(io::Error::new(io::ErrorKind::AlreadyExists, "BIOS seed temporary names are occupied"))
    }
    fn publish(&mut self, target: &OsStr) -> io::Result<()> {
        let from = name(&self.name)?;
        let to = name(target)?;
        // SAFETY: both names are in our held directory; rename replaces the leaf, follows no links.
        if unsafe { libc::renameat(self.dir.0.as_raw_fd(), from.as_ptr(), self.dir.0.as_raw_fd(), to.as_ptr()) } == 0 {
            self.published = true;
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }
}
impl Drop for Temp<'_> {
    fn drop(&mut self) {
        if !self.published {
            let _ = self.dir.unlink(&self.name);
        }
    }
}

pub(super) fn seed(source: &Path, dest: &Path) -> io::Result<()> {
    seed_with(source, dest, &mut |from, to| Pacer::new().copy(from, to).map(|_| ()))
}

/// Seed the files and publish their release only after every copy is durable.
/// The old shell's predictable `.pocket-ui-data.new` is deliberately never opened.
pub(super) fn seed_release(source: &Path, dest: &Path, release: &str) -> io::Result<()> {
    seed_release_using(source, dest, release, seed)
}

fn marker_matches(dir: &Dir, release: &str) -> io::Result<bool> {
    let Some(mut marker) = dir.node(OsStr::new(MARKER))? else {
        return Ok(false);
    };
    if !marker.metadata()?.is_file() {
        return Err(invalid());
    }
    // Bound a malformed marker, and accept the final newline written by older starters.
    if marker.metadata()?.len() > 129 {
        return Ok(false);
    }
    let mut bytes = Vec::new();
    Read::by_ref(&mut marker).take(130).read_to_end(&mut bytes)?;
    Ok(bytes == release.as_bytes() || bytes.strip_suffix(b"\n") == Some(release.as_bytes()))
}

fn seed_release_using(source: &Path, dest: &Path, release: &str, seed_files: impl FnOnce(&Path, &Path) -> io::Result<()>) -> io::Result<()> {
    if release.is_empty() || release.len() > 128 || release.contains(['\0', '\r', '\n']) {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "invalid BIOS data release"));
    }
    // Check the final marker BEFORE seed() can change any supplied data. Never
    // accept a link/directory just because all the bundled files already match.
    if let Some(dir) = Dir::root(dest, false)? {
        marker_matches(&dir, release)?;
    }
    seed_files(source, dest)?;
    let destination = Dir::root(dest, true)?.ok_or_else(invalid)?;
    if marker_matches(&destination, release)? {
        return Ok(());
    }
    let _idle = IdleIo::enter()?;
    let result = (|| {
        let mut temp = Temp::new(&destination, OsStr::new(MARKER))?;
        Pacer::new().copy(&mut release.as_bytes(), &mut temp.file)?;
        temp.file.sync_all()?;
        temp.publish(OsStr::new(MARKER))
    })();
    sync_card();
    result
}

fn seed_with(source: &Path, dest: &Path, copy: &mut dyn FnMut(&mut File, &mut File) -> io::Result<()>) -> io::Result<()> {
    let source = Dir::root(source, false)?.ok_or_else(invalid)?;
    let destination = Dir::root(dest, false)?;
    let mut changed = Vec::new();
    collect(&source, destination.as_ref(), &mut Vec::new(), &mut changed)?;
    if changed.is_empty() {
        return Ok(());
    }
    let _idle = IdleIo::enter()?;
    let destination = match destination {
        Some(d) => d,
        None => Dir::root(dest, true)?.ok_or_else(invalid)?,
    };
    if destination.node(OsStr::new(MARKER))?.is_some_and(|f| !f.metadata().is_ok_and(|m| m.is_file())) {
        return Err(invalid());
    }
    destination.unlink(OsStr::new(MARKER))?;
    sync_card(); // An interruption from here MUST make even an older RELEASE seed again.
    let result = (|| {
        for parts in changed {
            let (file_name, parents) = parts.split_last().ok_or_else(invalid)?;
            let source_dir = source.descend(parents, false)?.ok_or_else(invalid)?;
            let mut from = source_dir.node(file_name)?.ok_or_else(invalid)?;
            if !from.metadata()?.is_file() {
                return Err(invalid());
            }
            let target_dir = destination.descend(parents, true)?.ok_or_else(invalid)?;
            // Recheck the leaf after traversal; a source-supplied path never replaces a directory/link.
            if target_dir.node(file_name)?.is_some_and(|f| !f.metadata().is_ok_and(|m| m.is_file())) {
                return Err(invalid());
            }
            let mut temp = Temp::new(&target_dir, file_name)?;
            copy(&mut from, &mut temp.file)?;
            temp.file.sync_all()?;
            temp.publish(file_name)?;
        }
        Ok(())
    })();
    sync_card(); // Also persist successful earlier renames on a later file's failure.
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;
    use std::io::Write;
    use std::os::unix::fs::{symlink, MetadataExt, PermissionsExt};

    fn put(path: &Path, data: &[u8]) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, data).unwrap();
    }
    fn stamp(path: &Path) -> (u64, i64, i64) {
        let m = std::fs::metadata(path).unwrap();
        (m.ino(), m.mtime(), m.mtime_nsec())
    }
    fn no_temp(dir: &Path) -> bool {
        std::fs::read_dir(dir).unwrap().all(|e| {
            let e = e.unwrap();
            !e.file_name().as_bytes().starts_with(b".pocket-ui-seed-") && (!e.file_type().unwrap().is_dir() || no_temp(&e.path()))
        })
    }

    #[test]
    fn identical_read_only_tree_and_release_marker_are_completely_untouched() {
        let t = TestDir::new("brick-seed-unchanged");
        let source = t.join("source");
        let dest = t.join("dest");
        let data = vec![42; COMPARE_CHUNK + 17];
        put(&source.join("nested/data.bin"), &data);
        put(&dest.join("nested/data.bin"), &data);
        put(&dest.join(MARKER), b"older-release");
        let files = [dest.clone(), dest.join("nested"), dest.join("nested/data.bin"), dest.join(MARKER)];
        let before: Vec<_> = files.iter().map(|p| stamp(p)).collect();
        for p in [&dest, &dest.join("nested")] {
            std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o555)).unwrap();
        }
        std::fs::set_permissions(dest.join("nested/data.bin"), std::fs::Permissions::from_mode(0o444)).unwrap();
        seed_with(&source, &dest, &mut |_, _| panic!("identical data must never be copied")).unwrap();
        let after: Vec<_> = files.iter().map(|p| stamp(p)).collect();
        assert_eq!(before, after, "files and directories retain their inodes and mtimes");
        assert_eq!(std::fs::read(dest.join(MARKER)).unwrap(), b"older-release");
        assert!(no_temp(&dest));
        for p in [&dest, &dest.join("nested")] {
            std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
    }

    #[test]
    fn changed_same_size_and_missing_files_are_seeded_without_touching_user_bios() {
        let t = TestDir::new("brick-seed-changed");
        let source = t.join("source");
        let dest = t.join("dest");
        put(&source.join("existing.bin"), b"new bios");
        put(&source.join("nested/missing.bin"), b"additional data");
        put(&source.join("identical.bin"), b"keep exactly");
        put(&dest.join("existing.bin"), b"old bios");
        put(&dest.join("identical.bin"), b"keep exactly");
        put(&dest.join("user-owned.bin"), b"my BIOS");
        put(&dest.join(MARKER), b"old-release");
        let same_before = stamp(&dest.join("identical.bin"));
        let user_before = stamp(&dest.join("user-owned.bin"));
        seed(&source, &dest).unwrap();
        assert_eq!(std::fs::read(dest.join("existing.bin")).unwrap(), b"new bios");
        assert_eq!(std::fs::read(dest.join("nested/missing.bin")).unwrap(), b"additional data");
        assert_eq!(std::fs::read(dest.join("user-owned.bin")).unwrap(), b"my BIOS");
        assert_eq!(same_before, stamp(&dest.join("identical.bin")));
        assert_eq!(user_before, stamp(&dest.join("user-owned.bin")));
        assert!(!dest.join(MARKER).exists(), "caller alone may confirm the completed release");
        assert!(no_temp(&dest));
    }

    #[test]
    fn absent_destination_is_created_but_empty_source_does_not_create_it() {
        let t = TestDir::new("brick-seed-absent");
        let source = t.join("source");
        let dest = t.join("not-yet/BIOS");
        std::fs::create_dir(&source).unwrap();
        seed(&source, &dest).unwrap();
        assert!(!dest.exists());
        put(&source.join("nested/bios.bin"), b"bios");
        seed(&source, &dest).unwrap();
        assert_eq!(std::fs::read(dest.join("nested/bios.bin")).unwrap(), b"bios");
        assert!(!dest.join(MARKER).exists());
    }

    #[test]
    fn symlinks_in_source_destination_and_ancestors_fail_before_changing_files() {
        for case in [
            "source-file",
            "source-dir",
            "source-root",
            "source-ancestor",
            "dest-file",
            "dest-dir",
            "dest-root",
            "dest-ancestor",
            "marker",
        ] {
            let t = TestDir::new("brick-seed-no-links");
            let source = t.join("source");
            let dest = t.join("dest");
            put(&source.join("nested/bios.bin"), b"new");
            put(&dest.join("nested/bios.bin"), b"old");
            put(&dest.join(MARKER), b"old-release");
            let mut source_arg = source.clone();
            let mut dest_arg = dest.clone();
            match case {
                "source-file" => {
                    std::fs::rename(source.join("nested/bios.bin"), t.join("outside")).unwrap();
                    symlink(t.join("outside"), source.join("nested/bios.bin")).unwrap();
                }
                "source-dir" => {
                    std::fs::rename(source.join("nested"), t.join("outside")).unwrap();
                    symlink(t.join("outside"), source.join("nested")).unwrap();
                }
                "source-root" => {
                    symlink(&source, t.join("link")).unwrap();
                    source_arg = t.join("link");
                }
                "source-ancestor" => {
                    symlink(t.as_ref(), t.join("link")).unwrap();
                    source_arg = t.join("link/source");
                }
                "dest-file" => {
                    std::fs::rename(dest.join("nested/bios.bin"), t.join("outside")).unwrap();
                    symlink(t.join("outside"), dest.join("nested/bios.bin")).unwrap();
                }
                "dest-dir" => {
                    std::fs::rename(dest.join("nested"), t.join("outside")).unwrap();
                    symlink(t.join("outside"), dest.join("nested")).unwrap();
                }
                "dest-root" => {
                    symlink(&dest, t.join("link")).unwrap();
                    dest_arg = t.join("link");
                }
                "dest-ancestor" => {
                    symlink(t.as_ref(), t.join("link")).unwrap();
                    dest_arg = t.join("link/dest");
                }
                "marker" => {
                    std::fs::rename(dest.join(MARKER), t.join("outside")).unwrap();
                    symlink(t.join("outside"), dest.join(MARKER)).unwrap();
                }
                _ => unreachable!(),
            }
            assert!(seed(&source_arg, &dest_arg).is_err(), "{case}");
            assert_eq!(std::fs::read(dest.join("nested/bios.bin")).unwrap(), b"old", "{case}");
            assert_eq!(std::fs::read(dest.join(MARKER)).unwrap(), b"old-release", "{case}");
            assert!(no_temp(&dest));
        }
    }

    #[test]
    fn partial_failure_keeps_current_file_whole_and_invalidates_release_for_rollback_repair() {
        let t = TestDir::new("brick-seed-interrupted");
        let source = t.join("source");
        let dest = t.join("dest");
        let old = t.join("previous");
        for file in ["a.bin", "b.bin"] {
            put(&source.join(file), b"new content");
            put(&dest.join(file), b"old content");
            put(&old.join(file), b"old content");
        }
        put(&dest.join(MARKER), b"old-release");
        let second_before = stamp(&dest.join("b.bin"));
        let mut count = 0;
        let result = seed_with(&source, &dest, &mut |from, to| {
            assert!(!dest.join(MARKER).exists(), "marker must be gone before the first write");
            count += 1;
            if count == 2 {
                to.write_all(b"partial")?;
                return Err(io::Error::other("simulated full card"));
            }
            Pacer::new().copy(from, to).map(|_| ())
        });
        assert!(result.is_err());
        assert_eq!(std::fs::read(dest.join("a.bin")).unwrap(), b"new content");
        assert_eq!(std::fs::read(dest.join("b.bin")).unwrap(), b"old content");
        assert_eq!(second_before, stamp(&dest.join("b.bin")), "failed copy never truncates/replaces the old file");
        assert!(!dest.join(MARKER).exists());
        assert!(no_temp(&dest));
        seed(&old, &dest).unwrap();
        assert_eq!(std::fs::read(dest.join("a.bin")).unwrap(), b"old content", "rollback reseeds its original data");
        assert_eq!(std::fs::read(dest.join("b.bin")).unwrap(), b"old content");
        assert!(!dest.join(MARKER).exists());
    }

    #[test]
    fn supplied_marker_and_file_directory_conflicts_are_refused_without_writes() {
        for case in ["marker", "source-file-dest-dir", "source-dir-dest-file"] {
            let t = TestDir::new("brick-seed-conflict");
            let source = t.join("source");
            let dest = t.join("dest");
            put(&source.join("a.bin"), b"new");
            put(&dest.join("a.bin"), b"old");
            put(&dest.join(MARKER), b"old-release");
            match case {
                "marker" => put(&source.join(MARKER), b"untrusted marker"),
                "source-file-dest-dir" => {
                    put(&source.join("z"), b"file");
                    std::fs::create_dir(dest.join("z")).unwrap();
                }
                "source-dir-dest-file" => {
                    put(&source.join("z/file"), b"file");
                    put(&dest.join("z"), b"file");
                }
                _ => unreachable!(),
            }
            assert!(seed(&source, &dest).is_err(), "{case}");
            assert_eq!(std::fs::read(dest.join("a.bin")).unwrap(), b"old");
            assert_eq!(std::fs::read(dest.join(MARKER)).unwrap(), b"old-release");
            assert!(no_temp(&dest));
        }
    }

    #[test]
    fn release_publication_ignores_legacy_temp_symlink_and_keeps_foreign_data() {
        let t = TestDir::new("brick-seed-release-temp-link");
        let source = t.join("source");
        let dest = t.join("dest");
        put(&source.join("bios.bin"), b"new data");
        put(&dest.join("bios.bin"), b"old data");
        put(&dest.join(MARKER), b"old-release");
        put(&t.join("user-owned.bin"), b"never overwrite this BIOS");
        symlink(t.join("user-owned.bin"), dest.join(".pocket-ui-data.new")).unwrap();
        seed_release(&source, &dest, "new-release").unwrap();
        assert_eq!(std::fs::read(dest.join("bios.bin")).unwrap(), b"new data");
        assert_eq!(std::fs::read(dest.join(MARKER)).unwrap(), b"new-release");
        assert_eq!(std::fs::read(t.join("user-owned.bin")).unwrap(), b"never overwrite this BIOS");
        assert!(std::fs::symlink_metadata(dest.join(".pocket-ui-data.new")).unwrap().is_symlink());
        assert!(no_temp(&dest));
    }

    #[test]
    fn final_release_marker_link_or_directory_is_refused_before_data_changes() {
        for directory in [false, true] {
            let t = TestDir::new("brick-seed-release-marker-link");
            let source = t.join("source");
            let dest = t.join("dest");
            put(&source.join("bios.bin"), b"new");
            put(&dest.join("bios.bin"), b"old");
            put(&t.join("user-owned.bin"), b"user data");
            if directory {
                std::fs::create_dir(dest.join(MARKER)).unwrap();
            } else {
                symlink(t.join("user-owned.bin"), dest.join(MARKER)).unwrap();
            }
            assert!(seed_release(&source, &dest, "new-release").is_err());
            assert_eq!(std::fs::read(dest.join("bios.bin")).unwrap(), b"old");
            assert_eq!(std::fs::read(t.join("user-owned.bin")).unwrap(), b"user data");
            assert!(no_temp(&dest));
        }
    }

    #[test]
    fn same_files_and_same_release_including_legacy_newline_remain_untouched() {
        for marker in [b"release-1".as_slice(), b"release-1\n".as_slice()] {
            let t = TestDir::new("brick-seed-release-unchanged");
            let source = t.join("source");
            let dest = t.join("dest");
            put(&source.join("bios.bin"), b"same");
            put(&dest.join("bios.bin"), b"same");
            put(&dest.join(MARKER), marker);
            let before = [stamp(&dest), stamp(&dest.join("bios.bin")), stamp(&dest.join(MARKER))];
            std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o555)).unwrap();
            seed_release(&source, &dest, "release-1").unwrap();
            assert_eq!(before, [stamp(&dest), stamp(&dest.join("bios.bin")), stamp(&dest.join(MARKER))]);
            assert_eq!(std::fs::read(dest.join(MARKER)).unwrap(), marker);
            assert!(no_temp(&dest));
            std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
    }

    #[test]
    fn invalid_release_is_rejected_before_any_destination_mutation() {
        let t = TestDir::new("brick-seed-release-invalid");
        let source = t.join("source");
        let dest = t.join("dest");
        put(&source.join("bios.bin"), b"new");
        put(&dest.join("bios.bin"), b"old");
        put(&dest.join(MARKER), b"old-release");
        let before = [stamp(&dest), stamp(&dest.join("bios.bin")), stamp(&dest.join(MARKER))];
        for release in [
            String::new(),
            "x".repeat(129),
            "bad\0release".into(),
            "bad\rrelease".into(),
            "bad\nrelease".into(),
        ] {
            assert!(seed_release(&source, &dest, &release).is_err());
            assert_eq!(before, [stamp(&dest), stamp(&dest.join("bios.bin")), stamp(&dest.join(MARKER))]);
            assert_eq!(std::fs::read(dest.join("bios.bin")).unwrap(), b"old");
            assert_eq!(std::fs::read(dest.join(MARKER)).unwrap(), b"old-release");
            let absent = t.join("not-created");
            assert!(seed_release(&source, &absent, &release).is_err());
            assert!(!absent.exists());
        }
    }

    #[test]
    fn partial_copy_failure_never_publishes_a_new_release_marker() {
        let t = TestDir::new("brick-seed-release-partial");
        let source = t.join("source");
        let dest = t.join("dest");
        for file in ["a.bin", "b.bin"] {
            put(&source.join(file), b"new content");
            put(&dest.join(file), b"old content");
        }
        put(&dest.join(MARKER), b"old-release");
        let mut count = 0;
        let result = seed_release_using(&source, &dest, "new-release", |source, dest| {
            seed_with(source, dest, &mut |from, to| {
                assert!(!dest.join(MARKER).exists());
                count += 1;
                if count == 2 {
                    to.write_all(b"partial")?;
                    return Err(io::Error::other("simulated full card"));
                }
                Pacer::new().copy(from, to).map(|_| ())
            })
        });
        assert!(result.is_err());
        assert_eq!(std::fs::read(dest.join("a.bin")).unwrap(), b"new content");
        assert_eq!(std::fs::read(dest.join("b.bin")).unwrap(), b"old content");
        assert!(!dest.join(MARKER).exists());
        assert!(no_temp(&dest));
    }

    #[test]
    fn unchanged_files_with_new_release_only_replace_the_marker() {
        let t = TestDir::new("brick-seed-release-only");
        let source = t.join("source");
        let dest = t.join("dest");
        put(&source.join("bios.bin"), b"same");
        put(&dest.join("bios.bin"), b"same");
        put(&dest.join(MARKER), b"old-release");
        let before = stamp(&dest.join("bios.bin"));
        seed_release(&source, &dest, &"r".repeat(128)).unwrap();
        assert_eq!(before, stamp(&dest.join("bios.bin")));
        assert_eq!(std::fs::read(dest.join(MARKER)).unwrap(), vec![b'r'; 128]);
        assert!(no_temp(&dest));
    }
}
