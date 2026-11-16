//! The frontend VFS ABI from a core's side. Run on Android as well as the host: Bionic's stdio differs from glibc.
use std::{ffi::{c_char, c_int, c_uint, c_void, CString}, path::Path};
#[repr(C)]
struct Vfs {
    path: unsafe extern "C" fn(*mut c_void) -> *const c_char,
    open: unsafe extern "C" fn(*const c_char, c_uint, c_uint) -> *mut c_void,
    close: unsafe extern "C" fn(*mut c_void) -> c_int,
    size: unsafe extern "C" fn(*mut c_void) -> i64,
    tell: unsafe extern "C" fn(*mut c_void) -> i64,
    seek: unsafe extern "C" fn(*mut c_void, i64, c_int) -> i64,
    read: unsafe extern "C" fn(*mut c_void, *mut c_void, u64) -> i64,
}
fn main() {
    let dir = std::env::args_os().nth(1).expect("vfs_contract SCRATCH");
    std::fs::create_dir_all(&dir).unwrap();
    let vfs = unsafe { &*(&pocket_ui::vfs::INTERFACE as *const _ as *const Vfs) };
    for ext in ["md", "bin", "gen", "a26", "sfc", "gba", "chd", "pbp"] {
        let file = Path::new(&dir).join(format!("Read only (日本語).{ext}"));
        std::fs::write(&file, b"ROM contents").unwrap();
        let name = CString::new(file.as_os_str().as_encoded_bytes()).unwrap();
        unsafe {
            let f = (vfs.open)(name.as_ptr(), 1, 0);
            assert!(!f.is_null());
            let size = (vfs.size)(f);
            println!("{ext}: size={size}");
            assert_eq!(size, 12, "a core must be able to size its read-only ROM on this libc");
            let mut bytes = [0; 12];
            assert_eq!((vfs.read)(f, bytes.as_mut_ptr().cast(), 3), 3);
            assert_eq!((vfs.size)(f), 12, "sizing after a buffered read");
            assert_eq!((vfs.tell)(f), 3, "size must preserve the read cursor");
            assert_eq!((vfs.seek)(f, 0, 0), 0);
            assert_eq!((vfs.read)(f, bytes.as_mut_ptr().cast(), 12), 12);
            assert_eq!(&bytes, b"ROM contents");
            assert_eq!((vfs.close)(f), 0);
        }
        std::fs::remove_file(file).unwrap();
    }
    println!("PASS: all read-only content paths");
}
