//! Settings → Connections → USB file transfer: games over a USB cable, by ROCKNIX's own means. ROCKNIX prepares an MTP gadget
//! at every start (`usbgadget --start`, autostart `081-usbgadget`) and leaves it off; `usbgadget file_transfer` starts
//! `umtprd`, which offers `/storage/roms` to the computer, and `usbgadget disabled` ends it. ROCKNIX remembers the
//! choice across restarts. See docs/DEVICES.md "USB file transfer" for what was checked on which device.
//!
//! The row is only there where everything it needs is ([`offered_at`]). Nothing here runs on the UI thread except
//! [`offered`]'s few `stat`s, once.

use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::OnceLock;

const USBGADGET: &str = "usr/bin/usbgadget";
const UMTPRD: &str = "usr/sbin/umtprd";
const GADGET: &str = "sys/kernel/config/usb_gadget/mtp";
const UDC: &str = "sys/class/udc";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Off,
    /// On, no computer has opened it.
    On,
    /// A computer is talking to it.
    Connected,
}

impl Status {
    /// The row's value.
    pub fn label(self) -> &'static str {
        match self {
            Status::Off => "Off",
            Status::On => "On",
            Status::Connected => "Connected",
        }
    }
}

/// The programs and the gadget ROCKNIX prepared at this start, below `root` (`/` on the device).
pub fn offered_at(root: &Path) -> bool {
    [USBGADGET, UMTPRD, GADGET].iter().all(|p| root.join(p).exists())
}

/// Whether this device has the row (read once).
pub fn offered() -> bool {
    static OFFERED: OnceLock<bool> = OnceLock::new();
    *OFFERED.get_or_init(|| std::env::var_os("POCKETUI_SNAPSHOT_USB").is_some() || (!crate::device::current().host_owned() && offered_at(Path::new("/"))))
}

/// What the USB controller says, below `root`. No controller (the port is host right now): off.
pub fn status_at(root: &Path) -> Status {
    let Ok(entries) = std::fs::read_dir(root.join(UDC)) else { return Status::Off };
    for e in entries.flatten() {
        let read = |f: &str| std::fs::read_to_string(e.path().join(f)).unwrap_or_default();
        if read("function").trim() == "mtp" {
            return if read("state").trim() == "configured" { Status::Connected } else { Status::On };
        }
    }
    Status::Off
}

pub fn status() -> Status {
    status_at(Path::new("/"))
}

/// On: `systemd-run --scope` keeps `umtprd` when pocket-ui is restarted (an update does); it must not die with our
/// service while the gadget stays up. Off: plainly, the scope ends with `umtprd` (a second scope of that name is
/// refused while the first lives, seen on the Odin). stdout and stderr go nowhere: `umtprd` inherits them and never
/// closes them, so reading them would wait forever. Call it on a thread (about a second).
fn command(on: bool) -> Command {
    let mut c = if on {
        let mut c = Command::new("systemd-run");
        c.args(["--scope", "--quiet", "--collect", "--unit=pocket-usb-transfer", "/usr/bin/usbgadget", "file_transfer"]);
        c
    } else {
        let mut c = Command::new("/usr/bin/usbgadget");
        c.arg("disabled");
        c
    };
    c.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    c
}

/// Turns it on or off and says how it is afterwards.
pub fn set(on: bool) -> Status {
    let _ = command(on).status();
    status()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root(name: &str) -> std::path::PathBuf {
        let r = std::env::temp_dir().join(format!("pocket-usb-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&r);
        std::fs::create_dir_all(&r).unwrap();
        r
    }

    #[test]
    fn the_row_is_only_there_with_everything_it_needs() {
        let r = root("offered");
        assert!(!offered_at(&r), "a desktop, the virtual device, an image without it");
        for p in [USBGADGET, UMTPRD] {
            std::fs::create_dir_all(r.join(p).parent().unwrap()).unwrap();
            std::fs::write(r.join(p), b"").unwrap();
        }
        assert!(!offered_at(&r), "the programs alone: this kernel made no gadget");
        std::fs::create_dir_all(r.join(GADGET)).unwrap();
        assert!(offered_at(&r));
        let _ = std::fs::remove_dir_all(&r);
    }

    #[test]
    fn the_status_is_what_the_controller_says() {
        let r = root("status");
        assert_eq!(status_at(&r), Status::Off, "no controller: the port is host, or there is none");
        let udc = r.join(UDC).join("a600000.usb");
        std::fs::create_dir_all(&udc).unwrap();
        std::fs::write(udc.join("state"), "not attached\n").unwrap();
        assert_eq!(status_at(&r), Status::Off, "no function file: off");
        std::fs::write(udc.join("function"), "\n").unwrap();
        assert_eq!(status_at(&r), Status::Off);
        std::fs::write(udc.join("function"), "cdc\n").unwrap();
        assert_eq!(status_at(&r), Status::Off, "ROCKNIX's USB network isn't this");
        std::fs::write(udc.join("function"), "mtp\n").unwrap();
        assert_eq!(status_at(&r), Status::On);
        std::fs::write(udc.join("state"), "configured\n").unwrap();
        assert_eq!(status_at(&r), Status::Connected);
        let _ = std::fs::remove_dir_all(&r);
    }

    #[test]
    fn the_command_is_rocknixs_and_keeps_nothing_open() {
        let on = command(true);
        let args: Vec<_> = on.get_args().map(|a| a.to_string_lossy().into_owned()).collect();
        assert_eq!(on.get_program(), "systemd-run");
        assert_eq!(args.last().unwrap(), "file_transfer");
        assert!(args.contains(&"--scope".to_string()));
        let off = command(false);
        assert_eq!(off.get_program(), "/usr/bin/usbgadget", "not a second scope: that name is taken while umtprd lives");
        assert_eq!(off.get_args().last().unwrap(), "disabled");
    }
}
