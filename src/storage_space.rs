//! Filesystem space and display shared by all hosts.
use std::path::Path;
pub const LOW: u64 = 1_000_000_000;

/// Free and total bytes of the file system that holds `path`; None when it can't be asked (a statvfs can wait for the
/// card: never on the UI thread).
pub fn free_space(path: &Path) -> Option<(u64, u64)> {
    let c = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).ok()?;
    // SAFETY: statvfs only writes into the struct we pass.
    let mut st: libc::statvfs = unsafe { std::mem::zeroed() };
    (unsafe { libc::statvfs(c.as_ptr(), &mut st) } == 0).then(|| (st.f_bavail as u64 * st.f_frsize as u64, st.f_blocks as u64 * st.f_frsize as u64))
}

/// "740 KB", "312 MB", "4.2 GB" (powers of 1000, as the card's maker counts and About's free space).
pub fn size(bytes: u64) -> String {
    match bytes {
        0 => "0 MB".into(),
        1..=999_999 => format!("{} KB", bytes.div_ceil(1000)),
        1_000_000..=999_999_999 => format!("{} MB", bytes / 1_000_000),
        _ => format!("{:.1} GB", bytes as f64 / 1e9),
    }
}

