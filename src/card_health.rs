//! Settings → System → Storage → **Card check**: does the system say anything is wrong with the card's file system?
//! Read-only, nothing is repaired here.
//!
//! Weeks of sleep, flat batteries and power cuts on an ext4 card can leave it with errors, and the first sign is often
//! "my games disappeared" or saves that don't write. Three cheap signs, all from the system:
//! - the mount is **read-only** (`/proc/mounts`; ext4 remounts itself read-only after a serious error),
//! - the file system's own **error count** (`/sys/fs/ext4/<device>/errors_count`, kept in the superblock),
//! - the kernel's log (`dmesg`): `EXT4-fs error`, `Remounting filesystem read-only`, `I/O error` on the card.
//!
//! None of them is a check of the card (a fsck on a computer is): "no errors seen" is not "healthy", and the page says
//! so. The reading runs on a thread (a process and a few small files); [`judge`] and the parsers are pure.

use std::path::Path;
use std::time::Duration;

/// What the signs add up to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Health {
    /// Writable, no error count, nothing in the log.
    Fine,
    /// The file system has recorded errors, or the log shows card errors.
    Errors,
    /// The card is mounted read-only: saves and states can't be written.
    ReadOnly,
    /// Nothing could be read (a desktop, the virtual device).
    Unknown,
}

/// What the kernel's log holds about the card.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LogSigns {
    pub ext4_error: bool,
    pub remounted_read_only: bool,
    pub io_error: bool,
}

impl LogSigns {
    pub fn any(self) -> bool {
        self.ext4_error || self.remounted_read_only || self.io_error
    }
}

/// Everything that was read; None where it couldn't be.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Facts {
    pub read_only: Option<bool>,
    pub error_count: Option<u64>,
    /// The kernel's log was read.
    pub log: Option<LogSigns>,
}

pub fn judge(f: Facts) -> Health {
    if f.read_only == Some(true) {
        Health::ReadOnly
    } else if f.error_count.is_some_and(|n| n > 0) || f.log.is_some_and(LogSigns::any) {
        Health::Errors
    } else if f.read_only.is_none() && f.error_count.is_none() && f.log.is_none() {
        Health::Unknown
    } else {
        Health::Fine
    }
}

/// The mount `path` lies on, from `/proc/mounts`: (device, read-only). The longest mount point wins.
pub fn mount_of(mounts: &str, path: &Path) -> Option<(String, bool)> {
    let mut best: Option<(usize, String, bool)> = None;
    for line in mounts.lines() {
        let mut f = line.split_whitespace();
        let (Some(dev), Some(point), Some(_fs), Some(opts)) = (f.next(), f.next(), f.next(), f.next()) else { continue };
        let point = point.replace("\\040", " ");
        if !path.starts_with(&point) || !dev.starts_with('/') {
            continue;
        }
        if best.as_ref().is_none_or(|(len, ..)| point.len() > *len) {
            best = Some((point.len(), dev.to_string(), opts.split(',').any(|o| o == "ro")));
        }
    }
    best.map(|(_, dev, ro)| (dev, ro))
}

/// "mmcblk0p2" from "/dev/mmcblk0p2".
fn device_name(dev: &str) -> &str {
    dev.rsplit('/').next().unwrap_or(dev)
}

/// "mmcblk0" from "mmcblk0p2" (and "sda" from "sda1"): the disk a partition is on.
fn disk_of(part: &str) -> &str {
    if let Some(i) = part.rfind('p') {
        let digits = &part[i + 1..];
        if (part.starts_with("mmcblk") || part.starts_with("nvme")) && i > 0 && !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()) {
            return &part[..i];
        }
    }
    part.trim_end_matches(|c: char| c.is_ascii_digit())
}

/// What the kernel's log says about `device` ("mmcblk0p2"; None: any device).
pub fn log_signs(log: &str, device: Option<&str>) -> LogSigns {
    let mut signs = LogSigns::default();
    for line in log.lines() {
        if let Some(d) = device {
            if !line.contains(d) && !line.contains(disk_of(d)) {
                continue;
            }
        }
        if line.contains("EXT4-fs error") {
            signs.ext4_error = true;
        }
        if line.contains("Remounting filesystem read-only") {
            signs.remounted_read_only = true;
        }
        if line.contains("I/O error") && line.contains("mmcblk") {
            signs.io_error = true;
        }
    }
    signs
}

/// The file system's error count: the text of `errors_count`.
pub fn parse_count(text: &str) -> Option<u64> {
    text.trim().parse().ok()
}

/// Reads the signs for the card `path` lies on. `root`: "/" (a made-up tree in tests); `log`: the kernel's log if it
/// could be read.
pub fn read_at(root: &Path, path: &Path, log: Option<&str>) -> Health {
    let mounts = std::fs::read_to_string(root.join("proc/mounts")).ok();
    let mount = mounts.as_deref().and_then(|m| mount_of(m, path));
    let device = mount.as_ref().map(|(d, _)| device_name(d).to_string());
    let error_count = device
        .as_deref()
        .and_then(|d| std::fs::read_to_string(root.join("sys/fs/ext4").join(d).join("errors_count")).ok())
        .and_then(|t| parse_count(&t));
    judge(Facts {
        read_only: mount.as_ref().map(|(_, ro)| *ro),
        error_count,
        log: log.map(|l| log_signs(l, device.as_deref())),
    })
}

/// The real card (call it from a thread): `dmesg` has three seconds.
pub fn read(path: &Path) -> Health {
    let log = crate::proc::run_with_timeout(&mut std::process::Command::new("dmesg"), Duration::from_secs(3))
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned());
    read_at(Path::new("/"), path, log.as_deref())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;

    const MOUNTS: &str = "proc /proc proc rw,nosuid 0 0\n/dev/mmcblk0p1 /flash vfat rw,relatime 0 0\n/dev/mmcblk0p2 /storage ext4 rw,noatime 0 0\n";
    const MOUNTS_RO: &str = "/dev/mmcblk0p2 /storage ext4 ro,noatime,errors=remount-ro 0 0\n";

    #[test]
    fn the_card_is_the_longest_mount_the_path_lies_under() {
        assert_eq!(mount_of(MOUNTS, Path::new("/storage/roms")), Some(("/dev/mmcblk0p2".into(), false)));
        assert_eq!(mount_of(MOUNTS_RO, Path::new("/storage/roms")), Some(("/dev/mmcblk0p2".into(), true)));
        assert_eq!(mount_of("overlay / overlay rw 0 0\n", Path::new("/storage/roms")), None, "no device, nothing to say");
        assert_eq!(mount_of("", Path::new("/storage")), None);
        assert_eq!(mount_of("/dev/sda1 /storage/roms ext4 rw 0 0\n/dev/mmcblk0p2 /storage ext4 rw 0 0\n", Path::new("/storage/roms/gba")).unwrap().0, "/dev/sda1");
    }

    #[test]
    fn the_log_is_read_for_the_cards_device_only() {
        let log = "[ 10.1] EXT4-fs (mmcblk0p2): mounted filesystem\n\
                   [ 99.2] EXT4-fs error (device mmcblk0p2): ext4_lookup:1855: inode #123: deleted inode referenced\n\
                   [ 99.3] EXT4-fs (mmcblk0p2): Remounting filesystem read-only\n\
                   [ 99.4] blk_update_request: I/O error, dev mmcblk0, sector 8 op 0x0:(READ)\n\
                   [ 99.5] EXT4-fs error (device sda1): other card\n";
        assert_eq!(log_signs(log, Some("mmcblk0p2")), LogSigns { ext4_error: true, remounted_read_only: true, io_error: true });
        let other = "EXT4-fs error (device sda1): somewhere else\n";
        assert!(!log_signs(other, Some("mmcblk0p2")).any(), "another disk's trouble is not this card's");
        assert!(log_signs(other, None).ext4_error);
        assert!(!log_signs("EXT4-fs (mmcblk0p2): mounted filesystem with ordered data mode\n", None).any());
        assert_eq!(disk_of("mmcblk0p2"), "mmcblk0");
        assert_eq!(disk_of("sda1"), "sda");
    }

    #[test]
    fn the_signs_add_up() {
        let clean = LogSigns::default();
        assert_eq!(judge(Facts { read_only: Some(false), error_count: Some(0), log: Some(clean) }), Health::Fine);
        assert_eq!(judge(Facts { read_only: Some(true), ..Default::default() }), Health::ReadOnly, "read-only wins");
        assert_eq!(judge(Facts { read_only: Some(false), error_count: Some(3), log: None }), Health::Errors, "the superblock remembers");
        assert_eq!(judge(Facts { read_only: Some(false), error_count: None, log: Some(LogSigns { io_error: true, ..clean }) }), Health::Errors);
        assert_eq!(judge(Facts::default()), Health::Unknown, "nothing could be read: said, not 'fine'");
        assert_eq!(judge(Facts { read_only: Some(false), ..Default::default() }), Health::Fine);
    }

    #[test]
    fn a_made_up_system_is_read_from_its_files() {
        let dir = TestDir::new("cardhealth");
        std::fs::create_dir_all(dir.join("proc")).unwrap();
        std::fs::create_dir_all(dir.join("sys/fs/ext4/mmcblk0p2")).unwrap();
        std::fs::write(dir.join("proc/mounts"), MOUNTS).unwrap();
        std::fs::write(dir.join("sys/fs/ext4/mmcblk0p2/errors_count"), "0\n").unwrap();
        let roms = Path::new("/storage/roms");
        assert_eq!(read_at(&dir, roms, Some("")), Health::Fine);
        std::fs::write(dir.join("sys/fs/ext4/mmcblk0p2/errors_count"), "12\n").unwrap();
        assert_eq!(read_at(&dir, roms, Some("")), Health::Errors);
        std::fs::write(dir.join("proc/mounts"), MOUNTS_RO).unwrap();
        assert_eq!(read_at(&dir, roms, None), Health::ReadOnly);
        assert_eq!(read_at(&dir.join("nothing"), roms, None), Health::Unknown, "no /proc at all");
    }
}
