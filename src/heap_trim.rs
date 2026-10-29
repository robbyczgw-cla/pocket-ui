//! Gives the heap's free memory back to the system now and then.
//!
//! pocket-ui builds and drops large pictures all day (the Cabinet's wood and cases, covers, the pages' frames). glibc's
//! malloc raises its mmap threshold as such buffers are freed, so later ones come from the heap, and the holes between
//! the live ones are never returned: in a 30-minute monkey run of the Cabinet look (`scripts/monkey.py`, 2026-10-29) the
//! heap held 78 MB of live data in a 110 MB arena and the process grew from 53 to 154 MB. `malloc_trim(0)` returns
//! the free pages inside the arena as well; a thread does it every `EVERY`, off the UI thread, which a trim blocks
//! only for as long as it holds malloc's lock (milliseconds).

use std::time::Duration;

/// How often the heap is trimmed.
pub const EVERY: Duration = Duration::from_secs(20);

/// Returns free heap pages to the system; whether any were.
pub fn trim_now() -> bool {
    // SAFETY: malloc_trim has no preconditions; it only releases free memory.
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    unsafe {
        libc::malloc_trim(0) != 0
    }
    #[cfg(not(all(target_os = "linux", target_env = "gnu")))]
    false
}

/// A thread that trims every `EVERY` for the rest of the process.
pub fn spawn() {
    let _ = std::thread::Builder::new().name("heap-trim".into()).spawn(|| loop {
        std::thread::sleep(EVERY);
        trim_now();
    });
}

#[cfg(all(test, target_os = "linux", target_env = "gnu"))]
mod tests {
    use super::*;

    fn rss_kb() -> usize {
        let s = std::fs::read_to_string("/proc/self/status").unwrap();
        s.lines().find_map(|l| l.strip_prefix("VmRSS:")?.trim().trim_end_matches("kB").trim().parse().ok()).unwrap()
    }

    /// A big run of blocks freed below a few that stay (the top of the heap is not free, so the break cannot move down):
    /// only a trim hands the pages back.
    #[test]
    fn free_pages_below_live_blocks_go_back_after_a_trim() {
        let mut blocks: Vec<Vec<u8>> = (0..60_000).map(|i| vec![i as u8; 4096]).collect();
        let full = rss_kb();
        blocks.drain(..50_000);
        let before = rss_kb();
        trim_now();
        let after = rss_kb();
        assert!(after + 100_000 < before.max(full), "full {full} kB, freed {before} kB, trimmed {after} kB");
        drop(blocks);
    }
}
