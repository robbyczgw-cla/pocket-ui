//! libretro's virtual file system interface (version 3) on top of plain POSIX calls. Some cores (Stella
//! among them) read their files only through it; without it they cannot even open the ROM.

use std::ffi::{c_char, c_int, c_uint, c_void, CStr, CString};

/// Logs every call (POCKET_CORE_TRACE=1), to see what a core does with the interface.
pub static TRACE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn trace(msg: impl FnOnce() -> String) {
    if TRACE.load(std::sync::atomic::Ordering::Relaxed) {
        eprintln!("[vfs] {}", msg());
    }
}

const READ: c_uint = 1;
const WRITE: c_uint = 2;
const UPDATE_EXISTING: c_uint = 4;
const STAT_VALID: c_int = 1;
const STAT_DIR: c_int = 2;
const STAT_CHAR_SPECIAL: c_int = 4;

pub struct File {
    fd: c_int,
    path: CString,
}

pub struct Dir {
    dir: *mut libc::DIR,
    entry: *mut libc::dirent,
    hidden: bool,
}

unsafe extern "C" fn get_path(f: *mut File) -> *const c_char {
    (*f).path.as_ptr()
}

/// Dreamcast: the folder where ROCKNIX's standalone Flycast keeps one memory card per game
/// (`<game id>_vmu_save_A1.bin`, in `bios/dc`); see `share_flycast_vmus`.
static FLYCAST_VMU_DIR: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();

/// Lets the Flycast core use the standalone emulator's memory card of each game, so a game saved in
/// one is continued in the other. With per-game cards (`flycast_per_content_vmus`), the core opens
/// `<save dir>/<game id>.A1.bin`; that name becomes a link to `<dir>/<game id>_vmu_save_A1.bin`, made
/// when the core first opens it (the game id is known only then). A dangling link is fine: the card
/// is created through it.
pub fn share_flycast_vmus(dir: &std::path::Path) {
    let _ = FLYCAST_VMU_DIR.set(dir.to_path_buf());
}

/// `MK-5111750.A1.bin` → `MK-5111750_vmu_save_A1.bin`, for the card names Flycast's core uses.
fn standalone_vmu_name(file: &str) -> Option<String> {
    let (id, slot) = file.strip_suffix(".bin")?.rsplit_once('.')?;
    let b = slot.as_bytes();
    let card = b.len() == 2 && (b'A'..=b'D').contains(&b[0]) && (b'1'..=b'2').contains(&b[1]);
    let id_ok = !id.is_empty() && id.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_');
    (card && id_ok).then(|| format!("{id}_vmu_save_{slot}.bin"))
}

/// Links a per-game memory card the core is about to open to the standalone emulator's (see
/// `share_flycast_vmus`). A card the core made on its own before moves there unless the standalone
/// one exists; then it is kept as `.pocket-core.bak`.
fn link_flycast_vmu(path: &std::path::Path) {
    let (Some(dir), Some(name)) = (FLYCAST_VMU_DIR.get(), path.file_name().and_then(|n| n.to_str()).and_then(standalone_vmu_name)) else {
        return;
    };
    if path.symlink_metadata().is_ok_and(|m| m.file_type().is_symlink()) {
        return;
    }
    let target = dir.join(name);
    if path.exists() {
        let moved = if target.exists() {
            std::fs::rename(path, path.with_extension("bin.pocket-core.bak"))
        } else {
            std::fs::rename(path, &target)
        };
        if moved.is_err() {
            return;
        }
    }
    match std::os::unix::fs::symlink(&target, path) {
        Ok(()) => eprintln!("memory card {} -> {}", path.display(), target.display()),
        Err(e) => eprintln!("memory card link {}: {e}", path.display()),
    }
}

unsafe extern "C" fn open(path: *const c_char, mode: c_uint, _hints: c_uint) -> *mut File {
    if FLYCAST_VMU_DIR.get().is_some() {
        if let Ok(p) = CStr::from_ptr(path).to_str() {
            link_flycast_vmu(std::path::Path::new(p));
        }
    }
    let flags = match (mode & READ != 0, mode & WRITE != 0) {
        (true, true) => libc::O_RDWR | if mode & UPDATE_EXISTING != 0 { 0 } else { libc::O_CREAT | libc::O_TRUNC },
        (false, true) => libc::O_WRONLY | if mode & UPDATE_EXISTING != 0 { 0 } else { libc::O_CREAT | libc::O_TRUNC },
        _ => libc::O_RDONLY,
    };
    let fd = libc::open(path, flags | libc::O_CLOEXEC, 0o644 as c_uint);
    trace(|| format!("open {:?} mode {mode} -> {fd}", CStr::from_ptr(path)));
    if fd < 0 {
        return std::ptr::null_mut();
    }
    Box::into_raw(Box::new(File {
        fd,
        path: CStr::from_ptr(path).to_owned(),
    }))
}

unsafe extern "C" fn close(f: *mut File) -> c_int {
    let f = Box::from_raw(f);
    libc::close(f.fd)
}

unsafe extern "C" fn size(f: *mut File) -> i64 {
    let mut st: libc::stat = std::mem::zeroed();
    if libc::fstat((*f).fd, &mut st) == 0 {
        st.st_size as i64
    } else {
        -1
    }
}

unsafe extern "C" fn truncate(f: *mut File, len: i64) -> i64 {
    if libc::ftruncate((*f).fd, len as libc::off_t) == 0 {
        0
    } else {
        -1
    }
}

unsafe extern "C" fn tell(f: *mut File) -> i64 {
    libc::lseek((*f).fd, 0, libc::SEEK_CUR) as i64
}

unsafe extern "C" fn seek(f: *mut File, offset: i64, whence: c_int) -> i64 {
    let w = match whence {
        0 => libc::SEEK_SET,
        1 => libc::SEEK_CUR,
        _ => libc::SEEK_END,
    };
    // Like RetroArch's own implementation: 0 on success, -1 on failure (not the new position; cores
    // such as pcsx_rearmed treat any non-zero result as an error).
    let r = if libc::lseek((*f).fd, offset as libc::off_t, w) < 0 { -1 } else { 0 };
    trace(|| format!("seek fd {} {offset} whence {whence} -> {r}", (*f).fd));
    r
}

unsafe extern "C" fn read(f: *mut File, buf: *mut c_void, len: u64) -> i64 {
    let r = libc::read((*f).fd, buf, len as usize) as i64;
    trace(|| format!("read fd {} {len} -> {r}", (*f).fd));
    r
}

unsafe extern "C" fn write(f: *mut File, buf: *const c_void, len: u64) -> i64 {
    libc::write((*f).fd, buf, len as usize) as i64
}

unsafe extern "C" fn flush(f: *mut File) -> c_int {
    libc::fsync((*f).fd)
}

unsafe extern "C" fn remove(path: *const c_char) -> c_int {
    libc::unlink(path)
}

unsafe extern "C" fn rename(from: *const c_char, to: *const c_char) -> c_int {
    libc::rename(from, to)
}

unsafe extern "C" fn stat(path: *const c_char, size_out: *mut i32) -> c_int {
    let mut st: libc::stat = std::mem::zeroed();
    if libc::stat(path, &mut st) != 0 {
        trace(|| format!("stat {:?} -> missing", CStr::from_ptr(path)));
        return 0;
    }
    trace(|| format!("stat {:?} -> size {}", CStr::from_ptr(path), st.st_size));
    if !size_out.is_null() {
        *size_out = st.st_size as i32;
    }
    let kind = st.st_mode & libc::S_IFMT;
    STAT_VALID | if kind == libc::S_IFDIR { STAT_DIR } else { 0 } | if kind == libc::S_IFCHR { STAT_CHAR_SPECIAL } else { 0 }
}

unsafe extern "C" fn mkdir(path: *const c_char) -> c_int {
    if libc::mkdir(path, 0o755) == 0 {
        0
    } else if std::io::Error::last_os_error().raw_os_error() == Some(libc::EEXIST) {
        -2
    } else {
        -1
    }
}

unsafe extern "C" fn opendir(path: *const c_char, hidden: bool) -> *mut Dir {
    let dir = libc::opendir(path);
    if dir.is_null() {
        return std::ptr::null_mut();
    }
    Box::into_raw(Box::new(Dir {
        dir,
        entry: std::ptr::null_mut(),
        hidden,
    }))
}

unsafe extern "C" fn readdir(d: *mut Dir) -> bool {
    loop {
        (*d).entry = libc::readdir((*d).dir);
        if (*d).entry.is_null() {
            return false;
        }
        let name = CStr::from_ptr((*(*d).entry).d_name.as_ptr()).to_bytes();
        if (*d).hidden || !name.starts_with(b".") {
            return true;
        }
    }
}

unsafe extern "C" fn dirent_get_name(d: *mut Dir) -> *const c_char {
    (*(*d).entry).d_name.as_ptr()
}

unsafe extern "C" fn dirent_is_dir(d: *mut Dir) -> bool {
    (*(*d).entry).d_type == libc::DT_DIR
}

unsafe extern "C" fn closedir(d: *mut Dir) -> c_int {
    let d = Box::from_raw(d);
    libc::closedir(d.dir)
}

/// `struct retro_vfs_interface` (libretro.h): version 1 entries, then 2 (truncate), then 3.
#[repr(C)]
pub struct Interface {
    get_path: unsafe extern "C" fn(*mut File) -> *const c_char,
    open: unsafe extern "C" fn(*const c_char, c_uint, c_uint) -> *mut File,
    close: unsafe extern "C" fn(*mut File) -> c_int,
    size: unsafe extern "C" fn(*mut File) -> i64,
    tell: unsafe extern "C" fn(*mut File) -> i64,
    seek: unsafe extern "C" fn(*mut File, i64, c_int) -> i64,
    read: unsafe extern "C" fn(*mut File, *mut c_void, u64) -> i64,
    write: unsafe extern "C" fn(*mut File, *const c_void, u64) -> i64,
    flush: unsafe extern "C" fn(*mut File) -> c_int,
    remove: unsafe extern "C" fn(*const c_char) -> c_int,
    rename: unsafe extern "C" fn(*const c_char, *const c_char) -> c_int,
    truncate: unsafe extern "C" fn(*mut File, i64) -> i64,
    stat: unsafe extern "C" fn(*const c_char, *mut i32) -> c_int,
    mkdir: unsafe extern "C" fn(*const c_char) -> c_int,
    opendir: unsafe extern "C" fn(*const c_char, bool) -> *mut Dir,
    readdir: unsafe extern "C" fn(*mut Dir) -> bool,
    dirent_get_name: unsafe extern "C" fn(*mut Dir) -> *const c_char,
    dirent_is_dir: unsafe extern "C" fn(*mut Dir) -> bool,
    closedir: unsafe extern "C" fn(*mut Dir) -> c_int,
}

pub static INTERFACE: Interface = Interface {
    get_path,
    open,
    close,
    size,
    tell,
    seek,
    read,
    write,
    flush,
    remove,
    rename,
    truncate,
    stat,
    mkdir,
    opendir,
    readdir,
    dirent_get_name,
    dirent_is_dir,
    closedir,
};

/// The version of the interface this implements.
pub const VERSION: u32 = 3;

/// `struct retro_vfs_interface_info`, the argument of GET_VFS_INTERFACE.
#[repr(C)]
pub struct InterfaceInfo {
    pub required_interface_version: u32,
    pub iface: *const Interface,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;

    #[test]
    fn flycast_card_names() {
        assert_eq!(standalone_vmu_name("MK-5111750.A1.bin").as_deref(), Some("MK-5111750_vmu_save_A1.bin"));
        assert_eq!(standalone_vmu_name("T-8101N.B2.bin").as_deref(), Some("T-8101N_vmu_save_B2.bin"));
        assert_eq!(standalone_vmu_name("vmu_save_A1.bin"), None, "the shared card isn't a game's");
        assert_eq!(standalone_vmu_name("Sonic Adventure 2.cdi"), None);
        assert_eq!(standalone_vmu_name("dc_nvmem.bin"), None);
    }

    #[test]
    fn links_a_game_card_to_the_standalone_one() {
        let dir = TestDir::new("vmu");
        let (roms, bios) = (dir.join("roms"), dir.join("bios"));
        std::fs::create_dir_all(&roms).unwrap();
        std::fs::create_dir_all(&bios).unwrap();
        share_flycast_vmus(&bios);
        // A card the core made earlier moves to the standalone name; the link points there.
        std::fs::write(roms.join("MK-1.A1.bin"), b"card").unwrap();
        link_flycast_vmu(&roms.join("MK-1.A1.bin"));
        assert_eq!(std::fs::read(bios.join("MK-1_vmu_save_A1.bin")).unwrap(), b"card");
        assert_eq!(std::fs::read(roms.join("MK-1.A1.bin")).unwrap(), b"card", "read through the link");
        // No card yet: a dangling link, created through on the first write.
        link_flycast_vmu(&roms.join("MK-2.A1.bin"));
        std::fs::write(roms.join("MK-2.A1.bin"), b"new").unwrap();
        assert_eq!(std::fs::read(bios.join("MK-2_vmu_save_A1.bin")).unwrap(), b"new");
    }
}
