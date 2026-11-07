//! The games card in a second slot (docs/DEVICES.md "Second card"): ROCKNIX's `automount` mounts it on
//! `/storage/games-external` and binds its `roms` on `/storage/roms`, by itself, also when the card goes in later
//! (a udev rule restarts the script). Nothing tells a running pocket-ui, and nothing at all happens when a card comes
//! out: the dead mount stays. So a thread sleeps in `poll` on the mount table and on `/dev` (no wake-ups, no work on the
//! UI thread) and says when the card came or went: the shelf reads the folders again, or says the card is out.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

/// Where `automount` mounts a games card.
pub const MOUNT: &str = "/storage/games-external";

/// The games card, as far as the mount table and `/dev` say.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Card {
    /// Nothing mounted there: the games are on the card the system runs from.
    None,
    In(String),
    /// Still mounted, and its device is gone: it was pulled out. Reads and writes fail from here on.
    Out(String),
}

/// What happened between two looks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Change {
    /// A games card is in now, another one than before or the same one again.
    Came,
    /// Pulled out.
    Gone,
    /// `automount` went back to the first card (nothing pulled).
    Left,
}

/// The card from the text of `/proc/mounts`; `exists` asks whether a device file is there.
pub fn card_of(mounts: &str, exists: impl Fn(&Path) -> bool) -> Card {
    let device = mounts.lines().rev().find_map(|l| {
        let mut f = l.split_whitespace();
        let (device, point) = (f.next()?, f.next()?);
        (point == MOUNT).then_some(device)
    });
    match device {
        None => Card::None,
        Some(d) if exists(Path::new(d)) => Card::In(d.to_string()),
        Some(d) => Card::Out(d.to_string()),
    }
}

/// What changed from `old` to `new`, if anything worth a word.
pub fn change(old: &Card, new: &Card) -> Option<Change> {
    match (old, new) {
        (a, b) if a == b => None,
        // A pulled card's mount that `automount` cleans up when another card comes: no news yet.
        (Card::Out(_), Card::None) => None,
        (_, Card::Out(_)) => Some(Change::Gone),
        (_, Card::In(_)) => Some(Change::Came),
        (_, Card::None) => Some(Change::Left),
    }
}

static IN_USE: AtomicBool = AtomicBool::new(false);

/// Whether the games come from a card in the second slot now (kept by [`watch`]; the pages ask while painting). The
/// GAMES drive is on the same mount point and isn't one: it has its own line (`card_health::games_drive`).
pub fn in_use() -> bool {
    IN_USE.load(Ordering::Relaxed) && !crate::card_health::games_drive()
}

/// For a snapshot of the page with a card in.
pub fn set_in_use(on: bool) {
    IN_USE.store(on, Ordering::Relaxed);
}

/// Settle time: `automount` unmounts, mounts and binds in steps; only the end of them counts.
#[cfg(target_os = "linux")]
const SETTLE_MS: i32 = 2000;

/// Starts the thread. It wakes on a change of the mount table and of `/dev`, waits for things to settle, and calls
/// `tell` when the card came or went.
#[cfg(target_os = "linux")]
pub fn watch(tell: impl Fn(Change) + Send + 'static) {
    let _ = std::thread::Builder::new().name("games-card".into()).spawn(move || {
        watch_paths(Path::new("/dev"), &|| std::fs::read_to_string("/proc/self/mounts").unwrap_or_default(), &tell);
    });
}

#[cfg(target_os = "linux")]
fn watch_paths(dev: &Path, read_mounts: &dyn Fn() -> String, tell: &dyn Fn(Change)) {
    use std::os::fd::AsRawFd;
    use std::os::unix::ffi::OsStrExt;
    let look = || card_of(&read_mounts(), |p| p.exists());
    let Ok(mounts) = std::fs::File::open("/proc/self/mounts") else { return };
    // SAFETY: inotify_init1 takes flags only and returns a new descriptor or -1.
    let raw = unsafe { libc::inotify_init1(libc::IN_NONBLOCK | libc::IN_CLOEXEC) };
    if raw < 0 {
        return;
    }
    // SAFETY: `raw` is a new descriptor nothing else owns.
    let inotify = unsafe { <std::os::fd::OwnedFd as std::os::fd::FromRawFd>::from_raw_fd(raw) };
    let Ok(name) = std::ffi::CString::new(dev.as_os_str().as_bytes()) else { return };
    // SAFETY: both are valid for the call. A device file going or coming is what a pulled card looks like.
    if unsafe { libc::inotify_add_watch(inotify.as_raw_fd(), name.as_ptr(), libc::IN_CREATE | libc::IN_DELETE) } < 0 {
        return;
    }
    let mut known = look();
    IN_USE.store(matches!(known, Card::In(_)), Ordering::Relaxed);
    let mut fds = [
        libc::pollfd { fd: mounts.as_raw_fd(), events: libc::POLLERR | libc::POLLPRI, revents: 0 },
        libc::pollfd { fd: inotify.as_raw_fd(), events: libc::POLLIN, revents: 0 },
    ];
    let mut timeout = -1;
    let mut buf = [0u8; 1024];
    loop {
        // SAFETY: `fds` lives through the call and its length is passed.
        let n = unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, timeout) };
        if n < 0 {
            if std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            return;
        }
        if n > 0 {
            // SAFETY: `buf` is writable for its length; the descriptor doesn't block.
            while unsafe { libc::read(inotify.as_raw_fd(), buf.as_mut_ptr().cast(), buf.len()) } > 0 {}
            timeout = SETTLE_MS;
            continue;
        }
        timeout = -1;
        let now = look();
        IN_USE.store(matches!(now, Card::In(_)), Ordering::Relaxed);
        if let Some(c) = change(&known, &now) {
            tell(c);
        }
        known = now;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const STORAGE: &str = "/dev/mmcblk0p2 /storage ext4 rw 0 0\n";

    #[test]
    fn the_card_is_read_from_the_mount_table_and_dev() {
        let m = format!("{STORAGE}/dev/mmcblk1p1 /storage/games-external exfat rw 0 0\n/dev/mmcblk1p1 /storage/roms exfat rw 0 0\n");
        assert_eq!(card_of(&m, |_| true), Card::In("/dev/mmcblk1p1".into()));
        assert_eq!(card_of(&m, |_| false), Card::Out("/dev/mmcblk1p1".into()), "pulled: the mount stays, the device goes");
        assert_eq!(card_of(STORAGE, |_| true), Card::None);
        assert_eq!(card_of("", |_| true), Card::None);
    }

    #[test]
    fn a_later_mount_on_top_is_the_one_that_counts() {
        // A card put back while the dead mount was busy: automount mounts the new device over it.
        let m = format!("/dev/mmcblk1p1 /storage/games-external exfat rw 0 0\n/dev/mmcblk3p1 {MOUNT} exfat rw 0 0\n");
        assert_eq!(card_of(&m, |p| p == Path::new("/dev/mmcblk3p1")), Card::In("/dev/mmcblk3p1".into()));
    }

    #[test]
    fn each_change_has_its_word_and_no_change_has_none() {
        let (a, b, o) = (Card::In("a".into()), Card::In("b".into()), Card::Out("a".into()));
        assert_eq!(change(&Card::None, &a), Some(Change::Came));
        assert_eq!(change(&a, &b), Some(Change::Came));
        assert_eq!(change(&o, &b), Some(Change::Came), "put back: it comes under a new device name");
        assert_eq!(change(&a, &o), Some(Change::Gone));
        assert_eq!(change(&a, &Card::None), Some(Change::Left));
        assert_eq!(change(&o, &Card::None), None);
        assert_eq!(change(&a, &a), None);
        assert_eq!(change(&Card::None, &Card::None), None);
    }

    /// A device file going wakes the sleeping thread, and it says so after the settle time.
    #[cfg(target_os = "linux")]
    #[test]
    #[ignore = "timing tier: scripts/test-timing.sh (serial)"]
    fn timing_a_device_file_going_is_noticed_without_polling() {
        let dir = std::env::temp_dir().join(format!("pocket-games-card-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let dev = dir.join("mmcblk1p1");
        std::fs::write(&dev, b"").unwrap();
        let mounts = format!("{} {MOUNT} exfat rw 0 0\n", dev.display());
        let (tx, rx) = std::sync::mpsc::channel();
        IN_USE.store(false, Ordering::Relaxed);
        let (d, m) = (dir.clone(), mounts);
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let tx = std::sync::Mutex::new(tx);
            let first = std::sync::atomic::AtomicBool::new(true);
            watch_paths(&d, &move || {
                if first.swap(false, Ordering::SeqCst) {
                    ready_tx.send(()).unwrap();
                }
                m.clone()
            }, &move |c| {
                let _ = tx.lock().unwrap().send(c);
            })
        });
        ready_rx.recv_timeout(crate::testutil::timeout(std::time::Duration::from_secs(6))).unwrap();
        crate::testutil::clock::wait_for(|| IN_USE.load(Ordering::Relaxed), std::time::Duration::from_secs(6), "card observer initial snapshot");
        std::fs::remove_file(&dev).unwrap();
        assert_eq!(rx.recv_timeout(crate::testutil::timeout(std::time::Duration::from_secs(6))), Ok(Change::Gone));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Really mounts: run by hand as root in a container,
    /// `docker run --rm --privileged -v <test binary>:/t:ro debian:trixie-slim /t games_card::tests::a_mount_wakes_it --ignored`.
    #[cfg(target_os = "linux")]
    #[test]
    #[ignore]
    fn a_mount_wakes_it() {
        let dir = std::env::temp_dir().join(format!("pocket-games-card-m{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let point = dir.join("mnt");
        std::fs::create_dir_all(&point).unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        IN_USE.store(true, Ordering::Relaxed);
        let (d, p) = (dir.clone(), point.clone());
        std::thread::spawn(move || {
            let tx = std::sync::Mutex::new(tx);
            let read = move || {
                let on = std::fs::read_to_string("/proc/self/mounts").unwrap_or_default().contains(p.to_str().unwrap());
                if on { format!("/ {MOUNT} tmpfs rw 0 0\n") } else { String::new() }
            };
            watch_paths(&d, &read, &move |c| {
                let _ = tx.lock().unwrap().send(c);
            })
        });
        crate::testutil::clock::wait_for(|| !IN_USE.load(Ordering::Relaxed), std::time::Duration::from_secs(6), "mount observer initial empty state");
        assert!(std::process::Command::new("mount").args(["-t", "tmpfs", "t"]).arg(&point).status().unwrap().success());
        assert_eq!(rx.recv_timeout(crate::testutil::timeout(std::time::Duration::from_secs(6))), Ok(Change::Came));
        assert!(std::process::Command::new("umount").arg(&point).status().unwrap().success());
        assert_eq!(rx.recv_timeout(crate::testutil::timeout(std::time::Duration::from_secs(6))), Ok(Change::Left));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
