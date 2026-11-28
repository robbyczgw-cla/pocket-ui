//! Settings → System → Battery's "Reset battery reading" on devices with the RK817 power chip (R36S, R36XX and the
//! other RK3326/RK3566 handhelds, on ROCKNIX and dArkOS).
//!
//! The RK817 keeps the battery's level and its learned capacity in its own memory, which lives through a power off.
//! When that memory and the chip's charge counter go wrong together (0.7.0, an R36S: "counter 0 uAh, saved soc 0" at
//! 4.03 V), the kernel keeps the saved 0 % on every start and the battery reads empty while it is full.
//!
//! The reset is ROCKNIX's `factoryreset rk817-fuelgauge`, written here so that pocket-ui's own power off follows
//! (the game saved, no wake alarm left) and so that it works on dArkOS too, which has no `i2cset`:
//! - GG_STS (0x57) bit 4, BAT_CON, "battery not initialised", is set;
//! - the saved level and learned capacity, GAS_GAUGE_DATA0..5 (0x9d..0xa2), are zeroed;
//! - then the device switches off. At the next real power on the kernel sees BAT_CON, takes the level from the
//!   voltage the chip measured at power on (an open-circuit table) and the capacity from the device tree's design
//!   value, and clears the bit itself. A warm restart would leave the power-on voltage of the last cold start, hence
//!   the power off.
//!
//! Both drivers do that with BAT_CON: ROCKNIX's mainline `rk817_charger` (first boot: `PWRON_VOL` lookup, design
//! capacity) and the Rockchip BSP `rk817_battery` of dArkOS (`is_rk817_bat_first_pwron`, `rk817_bat_first_pwron`).
//! In the BSP driver 0x9d..0xa2 are the remaining and full capacity; its saved level (0x9a..0x9c) is rewritten from
//! the voltage at that first start.
//!
//! What it costs: the learned capacity is gone until the next full charge, so the percentage can be a few points off
//! until the battery was charged full once (the next start says so, [`take_note`]).
//!
//! The chip is found as `/sys/bus/i2c/devices/<bus>-0020` named `rk817`, and only where `/dev/i2c-<bus>` exists;
//! elsewhere the row is not shown. Writes go through `I2C_RDWR`, which works while the kernel driver owns the chip
//! (as `i2cset -f` does).

use std::fmt;
use std::io;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::OnceLock;

/// The RK817's address on its bus.
pub const ADDR: u16 = 0x20;
/// GAS_GAUGE_GG_STS.
const GG_STS: u8 = 0x57;
/// GG_STS bit 4: the battery is not initialised; the next start reads the level from the voltage.
const BAT_CON: u8 = 0x10;
/// GAS_GAUGE_DATA0..5: the saved level and the learned capacity.
const SAVED: [u8; 6] = [0x9d, 0x9e, 0x9f, 0xa0, 0xa1, 0xa2];

/// The file in the data folder that makes the next start say what to do now.
pub const NOTE_FILE: &str = "battery-reading-reset";

/// What the next start says, once.
pub const NOTE: &str = "The battery reading was reset. Charge your Pocket until it is full once, then the percentage is right again.";

/// Where the chip is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Gauge {
    pub bus: u32,
    pub dev: PathBuf,
}

/// The RK817 under `sys` (`/sys`) with its i2c device under `dev` (`/dev`), or `None`.
pub fn find_in(sys: &Path, dev: &Path) -> Option<Gauge> {
    let mut found: Vec<Gauge> = std::fs::read_dir(sys.join("bus/i2c/devices"))
        .ok()?
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            let bus: u32 = name.strip_suffix(&format!("-{ADDR:04x}"))?.parse().ok()?;
            let chip = std::fs::read_to_string(e.path().join("name")).ok()?;
            (chip.trim() == "rk817").then(|| Gauge { bus, dev: dev.join(format!("i2c-{bus}")) })
        })
        .filter(|g| g.dev.exists())
        .collect();
    found.sort_by_key(|g| g.bus);
    found.into_iter().next()
}

static FORCED: OnceLock<bool> = OnceLock::new();

/// Snapshots and host tests: the Battery page as on a device with the chip (nothing is ever written there).
pub fn pretend_present() {
    let _ = FORCED.set(true);
}

/// The chip on this device, looked for once.
pub fn gauge() -> Option<&'static Gauge> {
    static GAUGE: OnceLock<Option<Gauge>> = OnceLock::new();
    GAUGE.get_or_init(|| find_in(Path::new("/sys"), Path::new("/dev"))).as_ref()
}

/// The Battery page offers the reset (the chip is there and can be reached).
pub fn offered() -> bool {
    FORCED.get().copied().unwrap_or(false) || gauge().is_some()
}

/// Read and write one register of the chip.
pub trait Regs {
    fn get(&mut self, reg: u8) -> io::Result<u8>;
    fn set(&mut self, reg: u8, value: u8) -> io::Result<()>;
}

/// The registers the reset touches, as they were.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct State {
    pub sts: u8,
    pub saved: [u8; 6],
}

impl State {
    /// The chip says the battery is not initialised: the next cold start reads the level from the voltage.
    pub fn reset_pending(&self) -> bool {
        self.sts & BAT_CON != 0
    }
}

/// `sts=0x40 saved=0x9d..0xa2 as 12 hex digits`, for the log and for `--rk817 restore`.
impl fmt::Display for State {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let saved: String = self.saved.iter().map(|b| format!("{b:02x}")).collect();
        write!(f, "sts=0x{:02x} saved={saved}", self.sts)
    }
}

impl FromStr for State {
    type Err = String;
    fn from_str(s: &str) -> Result<State, String> {
        let bad = || format!("not a gauge state: {s:?} (want sts=0xNN saved= and 12 hex digits)");
        let mut sts = None;
        let mut saved = None;
        for part in s.split_whitespace() {
            if let Some(v) = part.strip_prefix("sts=") {
                sts = Some(u8::from_str_radix(v.trim_start_matches("0x"), 16).map_err(|_| bad())?);
            } else if let Some(v) = part.strip_prefix("saved=") {
                if v.len() != 12 || !v.is_ascii() {
                    return Err(bad());
                }
                let bytes: Vec<u8> = (0..6).map(|i| u8::from_str_radix(&v[2 * i..2 * i + 2], 16)).collect::<Result<_, _>>().map_err(|_| bad())?;
                saved = Some(<[u8; 6]>::try_from(bytes).map_err(|_| bad())?);
            }
        }
        Ok(State { sts: sts.ok_or_else(bad)?, saved: saved.ok_or_else(bad)? })
    }
}

/// The registers the reset touches.
pub fn read(r: &mut impl Regs) -> io::Result<State> {
    let sts = r.get(GG_STS)?;
    let mut saved = [0; 6];
    for (b, reg) in saved.iter_mut().zip(SAVED) {
        *b = r.get(reg)?;
    }
    Ok(State { sts, saved })
}

/// Marks the battery as not initialised and forgets the saved level and capacity; the switch-off after it is the
/// caller's. Returns the state before (for the log). An `Err` when the chip doesn't take it, read back.
pub fn reset(r: &mut impl Regs) -> io::Result<State> {
    let before = read(r)?;
    r.set(GG_STS, before.sts | BAT_CON)?;
    for reg in SAVED {
        r.set(reg, 0)?;
    }
    let after = read(r)?;
    if !after.reset_pending() || after.saved != [0; 6] {
        return Err(io::Error::other(format!("the chip did not keep the reset: before {before}, after {after}")));
    }
    Ok(before)
}

/// Puts the registers back as they were (device tests: before any power off, the reset is undone).
pub fn restore(r: &mut impl Regs, state: &State) -> io::Result<()> {
    for (reg, value) in SAVED.into_iter().zip(state.saved) {
        r.set(reg, value)?;
    }
    r.set(GG_STS, state.sts)?;
    let now = read(r)?;
    if now.saved != state.saved || now.reset_pending() != state.reset_pending() {
        return Err(io::Error::other(format!("restore not kept: want {state}, have {now}")));
    }
    Ok(())
}

/// Leaves the note for the next start.
pub fn leave_note(data_dir: &Path) {
    if let Err(e) = crate::fsutil::write_atomic(&data_dir.join(NOTE_FILE), b"1\n") {
        eprintln!("battery reading: the note for the next start could not be written: {e}");
    }
}

/// The next start after a reset: the line to say, once.
pub fn take_note(data_dir: &Path) -> Option<&'static str> {
    let path = data_dir.join(NOTE_FILE);
    path.exists().then(|| {
        let _ = std::fs::remove_file(&path);
        NOTE
    })
}

/// The chip's bus through `/dev/i2c-<bus>`.
pub struct Bus {
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    file: std::fs::File,
}

impl Bus {
    pub fn open(g: &Gauge) -> io::Result<Bus> {
        let file = std::fs::OpenOptions::new().read(true).write(true).open(&g.dev)?;
        Ok(Bus { file })
    }
}

#[cfg(target_os = "linux")]
mod i2c {
    use std::os::fd::AsRawFd;

    /// `I2C_RDWR` from linux/i2c-dev.h: messages with their own address, no `I2C_SLAVE` claim (the driver owns it).
    const I2C_RDWR: libc::c_ulong = 0x0707;
    const I2C_M_RD: u16 = 0x0001;

    #[repr(C)]
    struct Msg {
        addr: u16,
        flags: u16,
        len: u16,
        buf: *mut u8,
    }

    #[repr(C)]
    struct RdwrData {
        msgs: *mut Msg,
        nmsgs: u32,
    }

    pub(super) fn transfer(file: &std::fs::File, msgs: &mut [(u16, &mut [u8])]) -> std::io::Result<()> {
        let mut raw: Vec<Msg> = msgs
            .iter_mut()
            .map(|(flags, buf)| Msg { addr: super::ADDR, flags: *flags, len: buf.len() as u16, buf: buf.as_mut_ptr() })
            .collect();
        let mut data = RdwrData { msgs: raw.as_mut_ptr(), nmsgs: raw.len() as u32 };
        // SAFETY: `data` points at `raw`, whose buffers borrow `msgs` for the whole call.
        let rc = unsafe { libc::ioctl(file.as_raw_fd(), I2C_RDWR as _, &mut data as *mut RdwrData) };
        if rc < 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(())
    }

    pub(super) const READ: u16 = I2C_M_RD;
}

impl Regs for Bus {
    #[cfg(target_os = "linux")]
    fn get(&mut self, reg: u8) -> io::Result<u8> {
        // Register address, then one byte read with a repeated start (as regmap-i2c reads it).
        let (mut addr, mut value) = ([reg], [0u8]);
        i2c::transfer(&self.file, &mut [(0, &mut addr), (i2c::READ, &mut value)])?;
        Ok(value[0])
    }

    #[cfg(target_os = "linux")]
    fn set(&mut self, reg: u8, value: u8) -> io::Result<()> {
        i2c::transfer(&self.file, &mut [(0, &mut [reg, value])])
    }

    #[cfg(not(target_os = "linux"))]
    fn get(&mut self, _reg: u8) -> io::Result<u8> {
        Err(io::Error::from(io::ErrorKind::Unsupported))
    }

    #[cfg(not(target_os = "linux"))]
    fn set(&mut self, _reg: u8, _value: u8) -> io::Result<()> {
        Err(io::Error::from(io::ErrorKind::Unsupported))
    }
}

/// The reset on this device, with the state before written to the log (a support case can put it back by hand).
pub fn reset_device(data_dir: &Path) -> io::Result<State> {
    let g = gauge().ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no RK817 on this device"))?;
    let mut bus = Bus::open(g)?;
    let before = reset(&mut bus)?;
    println!("battery reading: reset on i2c-{} (before: {before}); switching off", g.bus);
    crate::shutdown_log::event(data_dir, &format!("battery reading reset (before: {before})"));
    leave_note(data_dir);
    Ok(before)
}

/// `pocket-ui --rk817 show|reset-only|restore <state>`: device tests read the registers, write the reset without the
/// power off, and put the state back exactly (`docs/DEVICE-TESTS.md`). Exit code 0 when it worked.
pub fn cli(args: &[String]) -> i32 {
    let Some(g) = gauge() else {
        eprintln!("rk817: no RK817 with an i2c device here");
        return 2;
    };
    let mut bus = match Bus::open(g) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("rk817: {}: {e}", g.dev.display());
            return 1;
        }
    };
    let result = match args.first().map(String::as_str) {
        Some("show") | None => read(&mut bus).map(|s| println!("rk817 i2c-{}: {s}", g.bus)),
        Some("reset-only") => reset(&mut bus).map(|before| println!("rk817 i2c-{}: reset written, before: {before}", g.bus)),
        Some("restore") => match args[1..].join(" ").parse::<State>() {
            Ok(state) => restore(&mut bus, &state).map(|()| println!("rk817 i2c-{}: restored {state}", g.bus)),
            Err(e) => {
                eprintln!("rk817: {e}");
                return 2;
            }
        },
        Some(other) => {
            eprintln!("rk817: unknown {other:?} (show, reset-only, restore <state>)");
            return 2;
        }
    };
    match result {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("rk817: {e}");
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;
    use std::collections::BTreeMap;

    /// The chip's registers; `stuck` ones ignore writes (a chip that doesn't take the reset).
    #[derive(Default)]
    struct Chip {
        regs: BTreeMap<u8, u8>,
        stuck: Vec<u8>,
        writes: Vec<(u8, u8)>,
    }

    impl Regs for Chip {
        fn get(&mut self, reg: u8) -> io::Result<u8> {
            Ok(*self.regs.get(&reg).unwrap_or(&0))
        }
        fn set(&mut self, reg: u8, value: u8) -> io::Result<()> {
            self.writes.push((reg, value));
            if !self.stuck.contains(&reg) {
                self.regs.insert(reg, value);
            }
            Ok(())
        }
    }

    /// The R36S of the report: saved 0 %, a learned capacity, status bits set.
    fn reported() -> Chip {
        let mut chip = Chip::default();
        chip.regs.insert(GG_STS, 0x43);
        for (reg, v) in SAVED.into_iter().zip([0x00, 0x00, 0x00, 0x34, 0x0d, 0x00]) {
            chip.regs.insert(reg, v);
        }
        // Neighbours the reset must not touch (the BSP's saved level, the counter).
        chip.regs.insert(0x9a, 0x11);
        chip.regs.insert(0xa3, 0x22);
        chip
    }

    #[test]
    fn the_reset_marks_the_battery_new_and_forgets_the_saved_level_and_capacity() {
        let mut chip = reported();
        let before = reset(&mut chip).unwrap();
        assert_eq!(before, State { sts: 0x43, saved: [0, 0, 0, 0x34, 0x0d, 0] });
        let after = read(&mut chip).unwrap();
        assert!(after.reset_pending(), "BAT_CON set: the next cold start reads the level from the voltage");
        assert_eq!(after.sts, 0x53, "the other status bits stay as they were");
        assert_eq!(after.saved, [0; 6]);
        assert_eq!(chip.regs[&0x9a], 0x11);
        assert_eq!(chip.regs[&0xa3], 0x22);
        // Only the registers of ROCKNIX's factoryreset rk817-fuelgauge are written.
        let written: Vec<u8> = chip.writes.iter().map(|(r, _)| *r).collect();
        assert_eq!(written, [GG_STS, 0x9d, 0x9e, 0x9f, 0xa0, 0xa1, 0xa2]);
    }

    #[test]
    fn a_chip_that_does_not_keep_the_reset_is_an_error_and_no_switch_off() {
        let mut chip = reported();
        chip.stuck.push(GG_STS);
        assert!(reset(&mut chip).is_err());
    }

    #[test]
    fn restore_puts_every_register_back_exactly() {
        let mut chip = reported();
        let original = chip.regs.clone();
        let before = reset(&mut chip).unwrap();
        restore(&mut chip, &before).unwrap();
        assert_eq!(chip.regs, original);
        assert!(!read(&mut chip).unwrap().reset_pending());
    }

    #[test]
    fn the_state_line_reads_back() {
        let s = State { sts: 0x43, saved: [0, 1, 0xab, 0x34, 0x0d, 0xff] };
        assert_eq!(s.to_string(), "sts=0x43 saved=0001ab340dff");
        assert_eq!(s.to_string().parse::<State>().unwrap(), s);
        assert_eq!("rk817 i2c-0: sts=0x43 saved=0001ab340dff".parse::<State>().unwrap(), s, "a log line works too");
        assert!("sts=0x43 saved=0001".parse::<State>().is_err());
        assert!("sts=0x43 saved=0001ab340dfg".parse::<State>().is_err());
        assert!("saved=0001ab340dff".parse::<State>().is_err());
    }

    fn sysfs(root: &Path, entries: &[(&str, &str)], devs: &[&str]) {
        for (dir, name) in entries {
            let d = root.join("sys/bus/i2c/devices").join(dir);
            std::fs::create_dir_all(&d).unwrap();
            std::fs::write(d.join("name"), format!("{name}\n")).unwrap();
        }
        std::fs::create_dir_all(root.join("dev")).unwrap();
        for d in devs {
            std::fs::write(root.join("dev").join(d), "").unwrap();
        }
    }

    #[test]
    fn the_row_is_there_only_with_an_rk817_and_its_i2c_device() {
        // R36S on ROCKNIX: the PMIC on i2c0, beside other chips.
        let t = TestDir::new("rk817-r36s");
        sysfs(&t, &[("0-0020", "rk817"), ("1-0020", "goodix"), ("i2c-0", "rk3x-i2c")], &["i2c-0", "i2c-1"]);
        assert_eq!(find_in(&t.join("sys"), &t.join("dev")), Some(Gauge { bus: 0, dev: t.join("dev/i2c-0") }));

        // An H700 (AXP717 at 0x34), and an RK808 at 0x20: no row.
        let t = TestDir::new("rk817-none");
        sysfs(&t, &[("0-0034", "axp717"), ("1-0020", "rk808")], &["i2c-0", "i2c-1"]);
        assert_eq!(find_in(&t.join("sys"), &t.join("dev")), None);

        // The chip without /dev/i2c-N (no i2c-dev): nothing could write it, so no row.
        let t = TestDir::new("rk817-nodev");
        sysfs(&t, &[("0-0020", "rk817")], &[]);
        assert_eq!(find_in(&t.join("sys"), &t.join("dev")), None);

        // No sysfs at all (Android, the host).
        let t = TestDir::new("rk817-empty");
        assert_eq!(find_in(&t.join("sys"), &t.join("dev")), None);
    }

    #[test]
    fn the_next_start_says_what_to_do_once() {
        let t = TestDir::new("rk817-note");
        assert_eq!(take_note(&t), None);
        leave_note(&t);
        assert_eq!(take_note(&t), Some(NOTE));
        assert!(NOTE.contains("full once"));
        assert_eq!(take_note(&t), None, "said once");
    }
}
