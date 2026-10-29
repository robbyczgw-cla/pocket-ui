//! The system controls the quick settings and the OSD need: brightness, volume, Wi-Fi, Bluetooth,
//! battery, time zone, network time and power. Everything goes through ROCKNIX's own mechanisms:
//!
//! | what | ROCKNIX |
//! |---|---|
//! | brightness | `/sys/class/backlight/*/brightness` (raw, max in `max_brightness`); `brightness set N` also stores `display.brightness`. Steps are 5 %, minimum 5 %. |
//! | volume | `pactl get-sink-volume @DEFAULT_SINK@` and `get-sink-mute` to read; `volume N` sets the default sink and stores `audio.volume` |
//! | Wi-Fi | `wifictl enable\|disable` (`rfkill unblock\|block wifi`), then `nmcli radio wifi on` if NetworkManager's own switch is off; read back from `/sys/class/rfkill` and `nmcli radio wifi`; `wifi.enabled` in system.cfg only once the radio really changed (`net_state`) |
//! | Bluetooth | `rocknix-bluetooth enable\|disable` (starts or stops bluetooth.service and the agent, stores `controllers.bluetooth.enabled`) |
//! | battery | `/sys/class/power_supply/battery/{capacity,status}` |
//! | time zone | `steamos-set-timezone <zone>`: `system.timezone`, `/storage/.cache/timezone`, `tz-data.service`, `timedatectl` |
//! | network time | systemd-timesyncd; `timedatectl show` for the state, restarting the service syncs again |
//! | date and time by hand | `clock_settime` and the RTC's `RTC_SET_TIME` (`manual_time`); network time stays on and wins later |
//! | power | `systemctl suspend\|poweroff\|reboot` |
//!
//! `Hw` is the seam: `RealHw` talks to the system, `MockHw` keeps the values in memory, and `AutoHw` takes
//! the real one for each control the machine actually has and the mock for the rest. So the virtual device
//! (no backlight, no radios, no battery, no systemd) still shows a working quick settings panel.
//! `POCKETUI_MOCK=1` forces the mock for everything (desktop preview).

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex};

use crate::net_state::{Cause, Radio};
use crate::timezone;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Charge {
    Charging,
    Discharging,
    Full,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Battery {
    pub percent: u8,
    pub charge: Charge,
}

impl Battery {
    /// "78 %", "78 %, charging", "Full"; "58%, plugged in" when the kernel says Full below
    /// [`FULL_FROM`](crate::battery::FULL_FROM) (charging stopped early).
    pub fn label(&self) -> String {
        match self.charge {
            Charge::Charging => format!("{}%, charging", self.percent),
            Charge::Full if self.percent < crate::battery::FULL_FROM => format!("{}%, plugged in", self.percent),
            Charge::Full => "Full".into(),
            Charge::Discharging => format!("{}%", self.percent),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NtpState {
    /// The clock was set from a time server.
    Synced,
    /// Network time is on, but no server has answered yet (no network, or just started).
    Waiting,
    Off,
    Unknown,
}

/// What setting the clock by hand did (`Hw::set_clock`).
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum ClockSet {
    /// The system clock and the hardware clock: it stays after a restart.
    Saved,
    /// Only the system clock: the RTC refused (the reason, for the log). Right until the next start.
    ClockOnly(String),
    /// The mock: nothing was changed (virtual device, tests).
    Mock,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PowerAction {
    Sleep,
    PowerOff,
    Reboot,
    /// Restart into the bootloader's fastboot screen (`systemctl reboot --reboot-argument=bootloader`).
    /// Only where the kernel's reboot-mode for the PMIC knows `bootloader` (`Hw::boot_menu_supported`).
    RebootBootloader,
    /// Not the system's: only pocket-ui starts afresh (the power menu's Restart pocket-ui). The game is saved and quit
    /// first (`Sleep::shut_down`), then pocket-ui ends with exit code 0 and its launcher starts it again
    /// (deploy/run-pocket-ui.sh). No `systemctl` behind it: `argv` is empty and `Hw::power` refuses it.
    RestartUi,
}

impl PowerAction {
    /// The `systemctl` arguments that do it (none for `RestartUi`).
    pub fn argv(self) -> &'static [&'static str] {
        match self {
            PowerAction::Sleep => &["suspend"],
            PowerAction::PowerOff => &["poweroff"],
            PowerAction::Reboot => &["reboot"],
            PowerAction::RebootBootloader => &["reboot", "--reboot-argument=bootloader"],
            PowerAction::RestartUi => &[],
        }
    }
}

/// Brightness never goes below this (ROCKNIX's own floor: a black screen cannot be undone without a display).
pub const MIN_BRIGHTNESS: u8 = 5;
pub const STEP: u8 = 5;

pub trait Hw: Send + Sync {
    /// 5..=100; `None` if the machine has no backlight.
    fn brightness(&self) -> Option<u8>;
    /// Moves the backlight now. Does not store anything: see `persist_brightness`.
    fn set_brightness(&self, pct: u8);
    /// Stores the level for the next start (runs ROCKNIX's `brightness set`; slow, call it once the level has settled).
    fn persist_brightness(&self, pct: u8);
    fn volume(&self) -> Option<u8>;
    /// Whether the output is muted (the volume stays where it was); `None` where that can't be asked. A process, like
    /// `volume`: call it from a worker.
    fn muted(&self) -> Option<bool> {
        None
    }
    /// Sets the volume and stores it (ROCKNIX's `volume N`; slow, call it from a worker).
    fn set_volume(&self, pct: u8);
    /// Where the sound goes (speaker, wired jack, Bluetooth …): processes, so call it from a worker. `None` where that
    /// can't be asked.
    fn sound(&self) -> Option<crate::audio_route::Sound> {
        None
    }
    /// Whether the Wi-Fi radio lets traffic through (rfkill soft and hard block); `None` if the machine has no radio.
    fn wifi(&self) -> Option<bool>;
    /// The radio as it really is: rfkill's soft and hard block, and NetworkManager's own switch.
    fn wifi_radio(&self) -> Radio {
        match self.wifi() {
            Some(true) => Radio::On,
            Some(false) => Radio::Off(Cause::Soft),
            None => Radio::Absent,
        }
    }
    /// Switches the radio and waits until it has really changed (read back), or says why not. Stores nothing: the wish
    /// (`persist_wifi`) follows only after a switch that worked. Slow: call it from a worker.
    fn set_wifi(&self, on: bool) -> Result<(), String>;
    /// `wifi.enabled` in system.cfg: what the player asked for. `None` where it can't be read.
    fn wifi_wanted(&self) -> Option<bool> {
        None
    }
    /// Stores `wifi.enabled`; false when that did not work.
    fn persist_wifi(&self, _on: bool) -> bool {
        true
    }
    fn bluetooth(&self) -> Option<bool>;
    /// Can take a few seconds (the Bluetooth agent has to start).
    fn set_bluetooth(&self, on: bool);
    fn battery(&self) -> Option<Battery>;
    fn timezone(&self) -> Option<String>;
    fn set_timezone(&self, zone: &str) -> Result<(), String>;
    fn zones(&self) -> Vec<String>;
    /// The offset of `zone` from UTC in seconds at the Unix time `at`.
    fn zone_offset(&self, zone: &str, at: i64) -> Option<i32>;
    /// What the Wi-Fi chip's regulatory domain says: the output of `iw reg get`. Read-only; `None` where `iw` is missing.
    fn regdomain(&self) -> Option<String> {
        None
    }
    /// A text file of the zoneinfo folder (`zone.tab`, `iso3166.tab`), for the first start's guess.
    fn zoneinfo_file(&self, _name: &str) -> Option<String> {
        None
    }
    fn ntp(&self) -> NtpState;
    /// Asks the time service to ask a server again.
    fn sync_time(&self);
    /// Sets the system clock and the hardware clock to `unix` (UTC), chosen by hand (Settings › System › Time › Date
    /// and time). Network time is left as it is. A syscall and an ioctl: call it from a worker.
    fn set_clock(&self, unix: i64) -> Result<ClockSet, String>;
    /// ROCKNIX was started from RAM (`fastboot boot`): a reboot goes back to Android.
    fn ram_booted(&self) -> bool;
    /// `reboot bootloader` leads to the fastboot screen: the kernel's reboot-mode has a `bootloader` entry.
    fn boot_menu_supported(&self) -> bool;
    fn power(&self, action: PowerAction) -> Result<(), String>;
}

// ---------------------------------------------------------------------------------------------------
// The real system

pub struct RealHw {
    root: PathBuf,
    /// How long a switch of the Wi-Fi radio waits for the system to follow before it says it did not work.
    settle: std::time::Duration,
    /// NetworkManager's radio switch as last read, and when (`nmcli radio wifi` is a process: not at every look).
    nm_radio: Mutex<Option<(std::time::Instant, Option<bool>)>>,
}

/// How long a read of NetworkManager's radio switch is trusted.
const NM_RADIO_KEPT: std::time::Duration = std::time::Duration::from_secs(8);

fn read(p: &Path) -> Option<String> {
    std::fs::read_to_string(p).ok().map(|s| s.trim().to_string())
}

/// Runs a command, true if it started and exited with 0. Output is dropped.
fn run(program: &str, args: &[&str]) -> bool {
    Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

fn run_out(program: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(program).args(args).stdin(Stdio::null()).stderr(Stdio::null()).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Whether system.cfg has no GPU overclock choice. ROCKNIX's 400-set_gpu_overclock then passes an empty value to
/// `gpu_overclock`, which matches no case, and the GPU keeps its highest OPP: 925 MHz on SM8250 instead of the
/// stock 587. Under heavy PS2 load that froze the whole device (Mini V2, SSX Tricky, 2026-10-22).
pub fn gpu_overclock_unset(system_cfg: &str) -> bool {
    !system_cfg.lines().any(|l| l.strip_prefix("gpu-overclock-speed=").is_some_and(|v| !v.trim().is_empty()))
}

/// Puts the GPU on its stock clock when no overclock was chosen, through ROCKNIX's own script (it also saves the
/// choice, so the next boot applies it). Devices without the script are left alone.
pub fn cap_gpu_if_unset() {
    if crate::device::rocknix_cooling(Path::new("/")) {
        return;
    }
    const SCRIPT: &str = "/usr/lib/autostart/quirks/platforms/SM8250/bin/gpu_overclock";
    const SYSTEM_CFG: &str = "/storage/.config/system/configs/system.cfg";
    let cfg = std::fs::read_to_string(SYSTEM_CFG).unwrap_or_default();
    if Path::new(SCRIPT).exists() && gpu_overclock_unset(&cfg) {
        let ok = run("bash", &["-c", r#". /etc/profile >/dev/null 2>&1; "$1" 0"#, "gpu_overclock", SCRIPT]);
        println!("gpu: no overclock chosen, stock clock (587 MHz): {}", if ok { "set" } else { "failed" });
    }
}

/// The first percentage in `pactl get-sink-volume` output: "Volume: front-left: 22937 /  35% / ...".
pub fn parse_pactl_volume(out: &str) -> Option<u8> {
    let before = out.split('%').next()?;
    let digits: String = before
        .chars()
        .rev()
        .take_while(|c| c.is_ascii_digit())
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    digits.parse::<u32>().ok().map(|v| v.min(100) as u8)
}

/// `pactl get-sink-mute` output: "Mute: yes" or "Mute: no".
pub fn parse_pactl_mute(out: &str) -> Option<bool> {
    match out.trim().strip_prefix("Mute:")?.trim() {
        "yes" => Some(true),
        "no" => Some(false),
        _ => None,
    }
}

/// `NTP=yes` / `NTPSynchronized=no` lines of `timedatectl show`.
pub fn parse_ntp(out: &str) -> NtpState {
    let get = |key: &str| out.lines().find_map(|l| l.strip_prefix(key)).map(str::trim);
    match (get("NTP="), get("NTPSynchronized=")) {
        (Some("no"), _) => NtpState::Off,
        (Some("yes"), Some("yes")) => NtpState::Synced,
        (Some("yes"), _) => NtpState::Waiting,
        _ => NtpState::Unknown,
    }
}

/// Whether the `reboot_modes` file of a reboot-mode class device lists `bootloader`.
pub fn reboot_modes_have_bootloader(text: &str) -> bool {
    text.split(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_'))
        .any(|word| word == "bootloader" || word == "mode-bootloader")
}

/// Parses a power_supply `status` word.
pub fn parse_charge(status: &str) -> Charge {
    match status.trim() {
        "Charging" => Charge::Charging,
        "Full" => Charge::Full,
        _ => Charge::Discharging,
    }
}

/// The Steam package's helper (SM8250, SM8550). Absent on the H700 image.
const STEAMOS_TIMEZONE: &str = "usr/bin/steamos-polkit-helpers/steamos-set-timezone";

impl RealHw {
    /// The time zone without the Steam helper, in the helper's own steps: `system.timezone` stored (ROCKNIX's
    /// `set_setting`, a shell function from /etc/profile.d/001-functions), `tz-data.service` writes the link, then
    /// `timedatectl`. Only the first step must work; the others are tried and logged.
    fn set_timezone_rocknix(&self, zone: &str) -> bool {
        let stored = Command::new("bash")
            .args(["-c", ". /etc/profile.d/001-functions && set_setting system.timezone \"$1\"", "set_setting", zone])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success());
        if !stored {
            return false;
        }
        if !run("systemctl", &["restart", "tz-data.service"]) {
            eprintln!("time: tz-data.service did not restart; trying timedatectl alone");
        }
        run("timedatectl", &["set-timezone", zone]) || self.timezone().as_deref() == Some(zone)
    }

    pub fn system() -> Self {
        Self::with_root("/")
    }

    /// Paths below `root` (a fake sysfs in tests).
    pub fn with_root(root: impl Into<PathBuf>) -> Self {
        RealHw { root: root.into(), settle: std::time::Duration::from_secs(4), nm_radio: Mutex::new(None) }
    }

    fn backlight(&self) -> Option<(PathBuf, u32)> {
        let mut dirs: Vec<PathBuf> = std::fs::read_dir(self.root.join("sys/class/backlight"))
            .ok()?
            .flatten()
            .map(|e| e.path())
            .collect();
        dirs.sort();
        dirs.into_iter().find_map(|d| {
            let max: u32 = read(&d.join("max_brightness"))?.parse().ok()?;
            (max > 0 && d.join("brightness").exists()).then_some((d, max))
        })
    }

    fn rfkill(&self, kind: &str) -> Option<bool> {
        self.rfkill_blocks(kind).map(|(soft, hard)| !soft && !hard)
    }

    /// Tests: the Wi-Fi switch gives up after `settle`.
    pub fn with_settle(mut self, settle: std::time::Duration) -> Self {
        self.settle = settle;
        self
    }

    /// ROCKNIX's system.cfg (below `root` on a fake root), under ROCKNIX's lock.
    fn system_cfg(&self) -> crate::system_cfg::SystemCfg {
        crate::system_cfg::SystemCfg::at(&self.root.join("storage/.config/system/configs/system.cfg"))
    }

    /// A program of the system: by name on the device, below `root`'s `usr/bin` on a fake root (stand-in scripts).
    fn tool(&self, name: &str) -> PathBuf {
        if self.root == Path::new("/") {
            PathBuf::from(name)
        } else {
            self.root.join("usr/bin").join(name)
        }
    }

    /// `(soft, hard)` of the first rfkill entry of `kind`.
    fn rfkill_blocks(&self, kind: &str) -> Option<(bool, bool)> {
        let dir = self.root.join("sys/class/rfkill");
        let mut entries: Vec<PathBuf> = std::fs::read_dir(dir).ok()?.flatten().map(|e| e.path()).collect();
        entries.sort();
        entries.into_iter().find(|p| read(&p.join("type")).as_deref() == Some(kind)).map(|p| {
            let blocked = |f: &str| read(&p.join(f)).as_deref() == Some("1");
            (blocked("soft"), blocked("hard"))
        })
    }

    /// NetworkManager's own radio switch (`nmcli radio wifi`): `None` where nmcli isn't there or doesn't say. Kept for
    /// `NM_RADIO_KEPT` unless `fresh`.
    fn nm_radio_on(&self, fresh: bool) -> Option<bool> {
        let now = std::time::Instant::now();
        if !fresh {
            if let Some((at, v)) = *self.nm_radio.lock().unwrap_or_else(|e| e.into_inner()) {
                if now.duration_since(at) < NM_RADIO_KEPT {
                    return v;
                }
            }
        }
        let out = crate::proc::run_with_timeout(Command::new(self.tool("nmcli")).args(["radio", "wifi"]), crate::proc::HELPER).ok();
        let v = out.filter(|o| o.status.success()).and_then(|o| match String::from_utf8_lossy(&o.stdout).trim() {
            "enabled" => Some(true),
            "disabled" => Some(false),
            _ => None,
        });
        *self.nm_radio.lock().unwrap_or_else(|e| e.into_inner()) = Some((now, v));
        v
    }

    fn radio_now(&self, fresh: bool) -> Radio {
        match self.rfkill_blocks("wlan") {
            None => Radio::Absent,
            Some((_, true)) => Radio::Off(Cause::Hard),
            Some((true, _)) => Radio::Off(Cause::Soft),
            Some(_) if self.nm_radio_on(fresh) == Some(false) => Radio::Off(Cause::Manager),
            Some(_) => Radio::On,
        }
    }

    /// A command of the system with its exit status turned into a message; its last line of complaint comes along.
    fn command(&self, name: &str, args: &[&str]) -> Result<(), String> {
        let out = crate::proc::run_with_timeout(Command::new(self.tool(name)).args(args), std::time::Duration::from_secs(20))
            .map_err(|e| format!("{name} {}: {e}", args.join(" ")))?;
        if out.status.success() {
            return Ok(());
        }
        let err = String::from_utf8_lossy(&out.stderr);
        let line = err.lines().rev().map(str::trim).find(|l| !l.is_empty()).unwrap_or("");
        Err(match out.status.code() {
            Some(c) => format!("{name} {} exited with {c}: {line}", args.join(" ")),
            None => format!("{name} {} was stopped: {line}", args.join(" ")),
        })
    }

    pub fn has_backlight(&self) -> bool {
        self.backlight().is_some()
    }
    /// `pactl` and ROCKNIX's `volume`. Not tried against the sound server: it may still be starting.
    pub fn has_volume(&self) -> bool {
        self.root.join("usr/bin/pactl").exists() && self.root.join("usr/bin/volume").exists()
    }
    pub fn has_wifi(&self) -> bool {
        self.rfkill("wlan").is_some() && self.root.join("usr/bin/wifictl").exists()
    }
    pub fn has_bluetooth(&self) -> bool {
        self.systemd() && self.root.join("usr/bin/rocknix-bluetooth").exists()
    }
    pub fn has_battery(&self) -> bool {
        self.battery().is_some()
    }
    pub fn has_time(&self) -> bool {
        self.systemd() && (self.root.join(STEAMOS_TIMEZONE).exists() || self.has_rocknix_timezone())
    }
    /// ROCKNIX without the Steam package (the H700 image): the zone goes through `set_setting` and `tz-data.service`,
    /// the way the helper does it.
    fn has_rocknix_timezone(&self) -> bool {
        self.root.join("etc/profile.d/001-functions").exists()
    }
    /// systemd is PID 1 (not a container without it).
    pub fn systemd(&self) -> bool {
        self.root.join("run/systemd/system").exists()
    }
}

impl Hw for RealHw {
    fn brightness(&self) -> Option<u8> {
        let (dir, max) = self.backlight()?;
        let raw: u32 = read(&dir.join("brightness"))?.parse().ok()?;
        Some(((raw * 100 + max / 2) / max).clamp(MIN_BRIGHTNESS as u32, 100) as u8)
    }

    fn set_brightness(&self, pct: u8) {
        if let Some((dir, max)) = self.backlight() {
            let raw = pct.clamp(MIN_BRIGHTNESS, 100) as u32 * max / 100;
            let _ = std::fs::write(dir.join("brightness"), raw.to_string());
        }
    }

    fn persist_brightness(&self, pct: u8) {
        run("brightness", &["set", &pct.clamp(MIN_BRIGHTNESS, 100).to_string()]);
    }

    fn volume(&self) -> Option<u8> {
        run_out("pactl", &["get-sink-volume", "@DEFAULT_SINK@"]).as_deref().and_then(parse_pactl_volume)
    }

    fn muted(&self) -> Option<bool> {
        run_out("pactl", &["get-sink-mute", "@DEFAULT_SINK@"]).as_deref().and_then(parse_pactl_mute)
    }

    fn set_volume(&self, pct: u8) {
        run("volume", &[&pct.min(100).to_string()]);
    }

    fn sound(&self) -> Option<crate::audio_route::Sound> {
        Some(crate::audio_route::sound_now())
    }

    fn wifi(&self) -> Option<bool> {
        self.rfkill("wlan")
    }

    fn wifi_radio(&self) -> Radio {
        self.radio_now(false)
    }

    /// ROCKNIX's `wifictl enable|disable` is `rfkill unblock|block wifi`; NetworkManager's own radio switch is its
    /// state file's business, so a radio it holds off is switched on too. Then the system is asked until it says the
    /// radio is in the state wanted, for `settle` at most; nothing is stored here (`persist_wifi`).
    fn set_wifi(&self, on: bool) -> Result<(), String> {
        let ran = self.command("wifictl", &[if on { "enable" } else { "disable" }]);
        if on && self.nm_radio_on(true) == Some(false) {
            if let Err(e) = self.command("nmcli", &["radio", "wifi", "on"]) {
                println!("wifi: {e}");
            }
        }
        let until = std::time::Instant::now() + self.settle;
        // rfkill is a file: looked at every 100 ms. NetworkManager's switch is a process: asked at most every 500 ms, and
        // only when the radio is being switched on and rfkill already lets it through.
        let mut nm: Option<(std::time::Instant, Option<bool>)> = None;
        loop {
            let radio = match self.rfkill_blocks("wlan") {
                None => Radio::Absent,
                Some((_, true)) => Radio::Off(Cause::Hard),
                Some((true, _)) => Radio::Off(Cause::Soft),
                Some(_) if !on => Radio::On,
                Some(_) => {
                    let known = match nm {
                        Some((at, v)) if at.elapsed() < std::time::Duration::from_millis(500) => v,
                        _ => {
                            let v = self.nm_radio_on(true);
                            nm = Some((std::time::Instant::now(), v));
                            v
                        }
                    };
                    if known == Some(false) { Radio::Off(Cause::Manager) } else { Radio::On }
                }
            };
            if (on && radio.is_on()) || (!on && radio.is_off()) {
                // The radio is as asked, whatever the command said (it may have warned about something else).
                if let Err(e) = &ran {
                    println!("wifi: {e}, but the radio is {radio:?}");
                }
                return Ok(());
            }
            if std::time::Instant::now() >= until {
                return Err(match ran {
                    Err(e) => e,
                    Ok(()) => format!("the radio is still {radio:?} {} s after switching it {}", self.settle.as_secs(), if on { "on" } else { "off" }),
                });
            }
            std::thread::sleep(std::time::Duration::from_millis(100).min(self.settle));
        }
    }

    fn wifi_wanted(&self) -> Option<bool> {
        self.system_cfg().get("wifi.enabled").map(|v| v.trim() == "1")
    }

    fn persist_wifi(&self, on: bool) -> bool {
        match self.system_cfg().set("wifi.enabled", if on { "1" } else { "0" }) {
            Ok(()) => true,
            Err(e) => {
                eprintln!("system.cfg: couldn't set wifi.enabled: {e}");
                false
            }
        }
    }

    fn bluetooth(&self) -> Option<bool> {
        run_out("systemctl", &["is-active", "bluetooth"]).map(|s| s.trim() == "active").or(Some(false))
    }

    fn set_bluetooth(&self, on: bool) {
        run("rocknix-bluetooth", &[if on { "enable" } else { "disable" }]);
    }

    fn battery(&self) -> Option<Battery> {
        let dir = self.root.join("sys/class/power_supply");
        let mut dirs: Vec<PathBuf> = std::fs::read_dir(dir).ok()?.flatten().map(|e| e.path()).collect();
        dirs.sort();
        dirs.into_iter().find_map(|d| {
            let percent: u8 = read(&d.join("capacity"))?.parse().ok()?;
            Some(Battery {
                percent: percent.min(100),
                charge: parse_charge(&read(&d.join("status")).unwrap_or_default()),
            })
        })
    }

    fn timezone(&self) -> Option<String> {
        let target = std::fs::canonicalize(self.root.join("etc/localtime")).ok()?;
        let base = std::fs::canonicalize(self.root.join("usr/share/zoneinfo")).ok()?;
        let rel = target.strip_prefix(base).ok()?;
        Some(rel.to_string_lossy().into_owned())
    }

    fn set_timezone(&self, zone: &str) -> Result<(), String> {
        if !timezone::is_valid(zone) || !self.root.join("usr/share/zoneinfo").join(zone).is_file() {
            return Err(format!("Unknown time zone {zone}."));
        }
        let ok = if self.root.join(STEAMOS_TIMEZONE).exists() {
            run(&format!("/{STEAMOS_TIMEZONE}"), &[zone])
        } else {
            self.set_timezone_rocknix(zone)
        };
        if ok {
            Ok(())
        } else {
            Err("The system didn't take the new time zone.".into())
        }
    }

    fn zones(&self) -> Vec<String> {
        timezone::list(&self.root.join("usr/share/zoneinfo"), &self.root.join("usr/config/system/configs/tz"))
    }

    fn zone_offset(&self, zone: &str, at: i64) -> Option<i32> {
        timezone::Tz::load(&self.root.join("usr/share/zoneinfo"), zone).map(|tz| tz.offset_at(at))
    }

    fn regdomain(&self) -> Option<String> {
        run_out("iw", &["reg", "get"])
    }

    fn zoneinfo_file(&self, name: &str) -> Option<String> {
        std::fs::read_to_string(self.root.join("usr/share/zoneinfo").join(name)).ok()
    }

    fn ntp(&self) -> NtpState {
        run_out("timedatectl", &["show", "-p", "NTP", "-p", "NTPSynchronized"]).map_or(NtpState::Unknown, |o| parse_ntp(&o))
    }

    fn sync_time(&self) {
        run("systemctl", &["restart", "systemd-timesyncd"]);
    }

    fn set_clock(&self, unix: i64) -> Result<ClockSet, String> {
        // Only the machine's own root: a test's `with_root` never sets a clock.
        if self.root != Path::new("/") {
            return Err("Not the system's clock.".into());
        }
        if let Err(e) = crate::manual_time::set_system_clock(unix) {
            eprintln!("time: couldn't set the clock: {e}");
            return Err("The system didn't take the new time.".into());
        }
        Ok(match crate::manual_time::write_rtc(&self.root.join("dev/rtc0"), unix) {
            Ok(()) => ClockSet::Saved,
            Err(e) => ClockSet::ClockOnly(e),
        })
    }

    fn ram_booted(&self) -> bool {
        read(&self.root.join("proc/cmdline")).is_some_and(|c| c.split_whitespace().any(|a| a == "nodtbo"))
    }

    /// The kernel registers a reboot-mode device for the PMIC's power-on block (`qcom-pon`), but lists only the
    /// modes the device tree defines (`mode-bootloader = <...>` in the `pon` node). Without that entry
    /// `reboot bootloader` is an ordinary restart, so the menu doesn't offer it.
    fn boot_menu_supported(&self) -> bool {
        let Ok(dir) = std::fs::read_dir(self.root.join("sys/class/reboot-mode")) else {
            return false;
        };
        dir.flatten()
            .any(|d| read(&d.path().join("reboot_modes")).is_some_and(|t| reboot_modes_have_bootloader(&t)))
    }

    fn power(&self, action: PowerAction) -> Result<(), String> {
        if action == PowerAction::Sleep && crate::device::rocknix_cooling(&self.root) {
            return Err(crate::sleep_settings::NO_SLEEP.into());
        }
        if action.argv().is_empty() {
            return Err("Only pocket-ui itself can restart pocket-ui.".into());
        }
        power_via(action, &crate::auto_off::Rtc::from_env(), &crate::shutdown_log::default_dir(), &run)
    }
}

/// `RealHw::power`: the fallback when the app has no shutdown handler (`Sleep::shut_down` is the normal way). A power
/// off or restart gets the same RTC disarm and log as there; a Sleep is only `systemctl suspend`: the alarm and its
/// gate stay as they are (a closed gate would end "Switch off after sleeping" for the rest of the run).
fn power_via(action: PowerAction, rtc: &crate::auto_off::Rtc, data: &Path, runner: &dyn Fn(&str, &[&str]) -> bool) -> Result<(), String> {
    let terminal = action != PowerAction::Sleep;
    let who = format!("quick settings {action:?}");
    if terminal {
        crate::auto_off::disarm_before_power_off(rtc, data, &who);
        crate::shutdown_log::event(data, &format!("{who}: running `systemctl {}`", action.argv().join(" ")));
    }
    if runner("systemctl", action.argv()) {
        if terminal {
            crate::shutdown_log::event(data, &format!("{who}: systemctl ended: exit 0"));
        }
        Ok(())
    } else {
        if terminal {
            crate::shutdown_log::event(data, &format!("{who}: systemctl failed"));
            rtc.reopen();
        }
        Err(format!("systemctl {} failed.", action.argv().join(" ")))
    }
}

// ---------------------------------------------------------------------------------------------------
// The mock

#[derive(Clone, Debug, PartialEq)]
pub struct MockState {
    pub brightness: u8,
    pub volume: u8,
    pub muted: bool,
    /// What `sound` answers.
    pub sound: crate::audio_route::Sound,
    pub wifi: bool,
    /// `wifi.enabled`, as `persist_wifi` left it.
    pub wifi_wanted: bool,
    pub bluetooth: bool,
    pub battery: Battery,
    pub zone: String,
    /// What `regdomain` answers (the text of `iw reg get`).
    pub regdomain: Option<String>,
    pub ntp: NtpState,
    /// Each time set by hand (`set_clock`); the clock itself is never changed.
    pub clock_log: Vec<i64>,
    /// What `set_clock` answers (`ClockSet::Mock` unless a test wants the real device's answers).
    pub clock_answer: Result<ClockSet, String>,
    pub ram_booted: bool,
    /// The mock offers "Restart to boot menu".
    pub boot_menu: bool,
    /// Power actions asked for (nothing is ever executed).
    pub power_log: Vec<PowerAction>,
    pub persisted_brightness: Option<u8>,
}

impl Default for MockState {
    fn default() -> Self {
        MockState {
            brightness: 60,
            volume: 45,
            muted: false,
            sound: crate::audio_route::Sound::Speaker,
            wifi: true,
            wifi_wanted: true,
            bluetooth: false,
            battery: Battery {
                percent: 72,
                charge: Charge::Discharging,
            },
            zone: "America/New_York".into(),
            regdomain: None,
            ntp: NtpState::Synced,
            clock_log: Vec::new(),
            clock_answer: Ok(ClockSet::Mock),
            ram_booted: true,
            boot_menu: true,
            power_log: Vec::new(),
            persisted_brightness: None,
        }
    }
}

pub struct MockHw {
    state: Mutex<MockState>,
    zoneinfo: PathBuf,
    /// Also move this process's clock to the chosen zone (TZ), so a changed time zone shows on screen.
    apply_tz: bool,
}

impl Default for MockHw {
    fn default() -> Self {
        Self::new(MockState::default())
    }
}

impl MockHw {
    pub fn new(state: MockState) -> Self {
        MockHw {
            state: Mutex::new(state),
            zoneinfo: PathBuf::from("/usr/share/zoneinfo"),
            apply_tz: false,
        }
    }

    /// A mock whose time zone changes the process's own clock.
    pub fn with_clock(mut self) -> Self {
        self.apply_tz = true;
        let zone = self.state.lock().unwrap().zone.clone();
        apply_process_zone(&zone);
        self
    }

    pub fn with_zoneinfo(mut self, dir: impl Into<PathBuf>) -> Self {
        self.zoneinfo = dir.into();
        self
    }

    pub fn state(&self) -> MockState {
        self.state.lock().unwrap().clone()
    }

    fn with<R>(&self, f: impl FnOnce(&mut MockState) -> R) -> R {
        f(&mut self.state.lock().unwrap())
    }
}

/// Moves libc's local time to `zone` for this process.
fn apply_process_zone(zone: &str) {
    if timezone::is_valid(zone) {
        // SAFETY: called from the UI thread at start-up and on a user's choice; the night light's thread reads the
        // local time under the same lock (`timezone::zone_lock`).
        let _zone = timezone::zone_lock();
        unsafe {
            libc::setenv(c"TZ".as_ptr(), std::ffi::CString::new(zone).unwrap().as_ptr(), 1);
            crate::timezone::tzset();
        }
    }
}

impl Hw for MockHw {
    fn brightness(&self) -> Option<u8> {
        Some(self.with(|s| s.brightness))
    }
    fn set_brightness(&self, pct: u8) {
        self.with(|s| s.brightness = pct.clamp(MIN_BRIGHTNESS, 100));
    }
    fn persist_brightness(&self, pct: u8) {
        self.with(|s| s.persisted_brightness = Some(pct));
    }
    fn volume(&self) -> Option<u8> {
        Some(self.with(|s| s.volume))
    }
    fn muted(&self) -> Option<bool> {
        Some(self.with(|s| s.muted))
    }
    fn set_volume(&self, pct: u8) {
        self.with(|s| s.volume = pct.min(100));
    }
    fn sound(&self) -> Option<crate::audio_route::Sound> {
        Some(self.with(|s| s.sound.clone()))
    }
    fn wifi(&self) -> Option<bool> {
        Some(self.with(|s| s.wifi))
    }
    fn set_wifi(&self, on: bool) -> Result<(), String> {
        self.with(|s| s.wifi = on);
        Ok(())
    }
    fn wifi_wanted(&self) -> Option<bool> {
        Some(self.with(|s| s.wifi_wanted))
    }
    fn persist_wifi(&self, on: bool) -> bool {
        self.with(|s| s.wifi_wanted = on);
        true
    }
    fn bluetooth(&self) -> Option<bool> {
        Some(self.with(|s| s.bluetooth))
    }
    fn set_bluetooth(&self, on: bool) {
        std::thread::sleep(std::time::Duration::from_millis(600)); // the real one starts a service
        self.with(|s| s.bluetooth = on);
    }
    fn battery(&self) -> Option<Battery> {
        Some(self.with(|s| s.battery))
    }
    fn timezone(&self) -> Option<String> {
        Some(self.with(|s| s.zone.clone()))
    }
    fn set_timezone(&self, zone: &str) -> Result<(), String> {
        if !timezone::is_valid(zone) {
            return Err(format!("Unknown time zone {zone}."));
        }
        self.with(|s| s.zone = zone.to_string());
        if self.apply_tz {
            apply_process_zone(zone);
        }
        Ok(())
    }
    fn zones(&self) -> Vec<String> {
        let zones = timezone::list(&self.zoneinfo, Path::new("/nonexistent"));
        if zones.is_empty() {
            ["America/New_York", "Asia/Tokyo", "Europe/Berlin", "Europe/London", "Europe/Vienna", "UTC"]
                .iter()
                .map(|s| s.to_string())
                .collect()
        } else {
            zones
        }
    }
    fn zone_offset(&self, zone: &str, at: i64) -> Option<i32> {
        timezone::Tz::load(&self.zoneinfo, zone).map(|tz| tz.offset_at(at))
    }
    fn regdomain(&self) -> Option<String> {
        self.with(|s| s.regdomain.clone())
    }
    fn zoneinfo_file(&self, name: &str) -> Option<String> {
        std::fs::read_to_string(self.zoneinfo.join(name)).ok()
    }
    fn ntp(&self) -> NtpState {
        self.with(|s| s.ntp)
    }
    fn sync_time(&self) {
        self.with(|s| s.ntp = NtpState::Synced);
    }
    fn set_clock(&self, unix: i64) -> Result<ClockSet, String> {
        println!("time: set by hand to {unix} (mock: the clock is not changed)");
        self.with(|s| {
            s.clock_log.push(unix);
            s.clock_answer.clone()
        })
    }
    fn ram_booted(&self) -> bool {
        self.with(|s| s.ram_booted)
    }
    fn boot_menu_supported(&self) -> bool {
        self.with(|s| s.boot_menu)
    }
    fn power(&self, action: PowerAction) -> Result<(), String> {
        self.with(|s| s.power_log.push(action));
        Ok(())
    }
}

// ---------------------------------------------------------------------------------------------------
// Real where the machine has it, mock for the rest

/// Which controls are backed by the real system.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Caps {
    pub backlight: bool,
    pub volume: bool,
    pub wifi: bool,
    pub bluetooth: bool,
    pub battery: bool,
    pub time: bool,
    pub power: bool,
}

pub struct AutoHw {
    real: RealHw,
    mock: MockHw,
    pub caps: Caps,
    /// `POCKETUI_MOCK` or not Linux: nothing is looked for later.
    forced_mock: bool,
}

impl AutoHw {
    pub fn detect() -> Self {
        let real = RealHw::system();
        let forced_mock = std::env::var_os("POCKETUI_MOCK").is_some() || cfg!(not(target_os = "linux"));
        let caps = if forced_mock {
            Caps::default()
        } else {
            Caps {
                backlight: real.has_backlight(),
                volume: real.has_volume(),
                wifi: real.has_wifi(),
                bluetooth: real.has_bluetooth(),
                battery: real.has_battery(),
                time: real.has_time(),
                power: real.systemd(),
            }
        };
        Self::new(real, caps)
    }

    pub fn new(real: RealHw, caps: Caps) -> Self {
        let mut mock = MockHw::default();
        // A fresh ROCKNIX is on UTC; POCKETUI_MOCK_ZONE=UTC lets a virtual device start like that (docs/VIRTUAL-DEVICE.md).
        if let Some(z) = std::env::var("POCKETUI_MOCK_ZONE").ok().filter(|z| timezone::is_valid(z)) {
            mock.with(|s| s.zone = z);
        }
        // Without the real time service the mock owns the zone, and shows it on this process's clock.
        if !caps.time {
            mock = mock.with_clock();
        } else if let Some(z) = real.timezone() {
            mock.with(|s| s.zone = z);
        }
        // The real volume (seeding the mock with it) is read by `seed_volume`, on a thread: `pactl` is a process and
        // PipeWire may still be starting at boot, and the first frame doesn't need it.
        // To see the power menu of a device whose kernel can't restart into the bootloader (the Mini V2 today).
        if std::env::var_os("POCKETUI_NO_BOOT_MENU").is_some() {
            mock.with(|s| s.boot_menu = false);
        }
        let forced_mock = std::env::var_os("POCKETUI_MOCK").is_some() || cfg!(not(target_os = "linux"));
        AutoHw { real, mock, caps, forced_mock }
    }

    /// Gives the mock the real volume, as `new` used to. Runs `pactl`: call it off the UI thread.
    pub fn seed_volume(&self) {
        if let Some(v) = self.caps.volume.then(|| self.real.volume()).flatten() {
            self.mock.with(|s| s.volume = v);
        }
    }

    /// What is mocked, for the log.
    pub fn mocked(&self) -> Vec<&'static str> {
        let c = self.caps;
        [
            ("brightness", c.backlight),
            ("volume", c.volume),
            ("wifi", c.wifi),
            ("bluetooth", c.bluetooth),
            ("battery", c.battery),
            ("time", c.time),
            ("power", c.power),
        ]
        .iter()
        .filter(|(_, real)| !real)
        .map(|(n, _)| *n)
        .collect()
    }

    pub fn mock(&self) -> &MockHw {
        &self.mock
    }

    /// The Wi-Fi radio is the system's: it was there at the start, or it has appeared since (the chip's driver can come
    /// up after pocket-ui, and the Wi-Fi row must not stay a mock for the whole run because of that).
    fn wifi_is_real(&self) -> bool {
        self.caps.wifi || (!self.forced_mock && self.real.has_wifi())
    }

    /// A device without a Wi-Fi chip (the RG28XX, `device::Device::has_wifi`) and none showing up: not the mock's
    /// pretend radio and networks, but "no radio" (`Radio::Absent`), which the Wi-Fi pages say in words.
    fn wifi_absent(&self) -> bool {
        !self.wifi_is_real() && !crate::device::current().has_wifi()
    }
}

impl Hw for AutoHw {
    fn brightness(&self) -> Option<u8> {
        if self.caps.backlight {
            self.real.brightness()
        } else {
            self.mock.brightness()
        }
    }
    fn set_brightness(&self, pct: u8) {
        if self.caps.backlight {
            self.real.set_brightness(pct)
        } else {
            self.mock.set_brightness(pct)
        }
    }
    fn persist_brightness(&self, pct: u8) {
        if self.caps.backlight {
            self.real.persist_brightness(pct)
        } else {
            self.mock.persist_brightness(pct)
        }
    }
    fn volume(&self) -> Option<u8> {
        if self.caps.volume {
            self.real.volume()
        } else {
            self.mock.volume()
        }
    }
    fn muted(&self) -> Option<bool> {
        if self.caps.volume {
            self.real.muted()
        } else {
            self.mock.muted()
        }
    }
    fn set_volume(&self, pct: u8) {
        if self.caps.volume {
            self.real.set_volume(pct)
        } else {
            self.mock.set_volume(pct)
        }
    }
    fn sound(&self) -> Option<crate::audio_route::Sound> {
        if self.caps.volume {
            self.real.sound()
        } else {
            self.mock.sound()
        }
    }
    fn wifi(&self) -> Option<bool> {
        if self.wifi_absent() {
            None
        } else if self.wifi_is_real() {
            self.real.wifi()
        } else {
            self.mock.wifi()
        }
    }
    fn wifi_radio(&self) -> Radio {
        if self.wifi_absent() {
            Radio::Absent
        } else if self.wifi_is_real() {
            self.real.wifi_radio()
        } else {
            self.mock.wifi_radio()
        }
    }
    fn set_wifi(&self, on: bool) -> Result<(), String> {
        if self.wifi_absent() {
            Err("This device has no Wi-Fi.".into())
        } else if self.wifi_is_real() {
            self.real.set_wifi(on)
        } else {
            self.mock.set_wifi(on)
        }
    }
    fn wifi_wanted(&self) -> Option<bool> {
        if self.wifi_is_real() {
            self.real.wifi_wanted()
        } else {
            self.mock.wifi_wanted()
        }
    }
    fn persist_wifi(&self, on: bool) -> bool {
        if self.wifi_is_real() {
            self.real.persist_wifi(on)
        } else {
            self.mock.persist_wifi(on)
        }
    }
    fn bluetooth(&self) -> Option<bool> {
        if self.caps.bluetooth {
            self.real.bluetooth()
        } else {
            self.mock.bluetooth()
        }
    }
    fn set_bluetooth(&self, on: bool) {
        if self.caps.bluetooth {
            self.real.set_bluetooth(on)
        } else {
            self.mock.set_bluetooth(on)
        }
    }
    fn battery(&self) -> Option<Battery> {
        if self.caps.battery {
            self.real.battery()
        } else {
            self.mock.battery()
        }
    }
    fn timezone(&self) -> Option<String> {
        if self.caps.time {
            self.real.timezone()
        } else {
            self.mock.timezone()
        }
    }
    fn set_timezone(&self, zone: &str) -> Result<(), String> {
        if self.caps.time {
            self.real.set_timezone(zone)
        } else {
            self.mock.set_timezone(zone)
        }
    }
    fn zones(&self) -> Vec<String> {
        let z = if self.caps.time { self.real.zones() } else { self.mock.zones() };
        if z.is_empty() {
            self.mock.zones()
        } else {
            z
        }
    }
    fn zone_offset(&self, zone: &str, at: i64) -> Option<i32> {
        if self.caps.time {
            self.real.zone_offset(zone, at)
        } else {
            self.mock.zone_offset(zone, at)
        }
    }
    /// Reading the regulatory domain changes nothing, so it is the real one wherever `iw` answers (the mock where it doesn't).
    fn regdomain(&self) -> Option<String> {
        self.real.regdomain().or_else(|| self.mock.regdomain())
    }
    fn zoneinfo_file(&self, name: &str) -> Option<String> {
        if self.caps.time {
            self.real.zoneinfo_file(name)
        } else {
            self.mock.zoneinfo_file(name)
        }
    }
    fn ntp(&self) -> NtpState {
        if self.caps.time {
            self.real.ntp()
        } else {
            self.mock.ntp()
        }
    }
    fn sync_time(&self) {
        if self.caps.time {
            self.real.sync_time()
        } else {
            self.mock.sync_time()
        }
    }
    fn set_clock(&self, unix: i64) -> Result<ClockSet, String> {
        if self.caps.time {
            self.real.set_clock(unix)
        } else {
            self.mock.set_clock(unix)
        }
    }
    fn ram_booted(&self) -> bool {
        if self.caps.power {
            self.real.ram_booted()
        } else {
            self.mock.ram_booted()
        }
    }
    fn boot_menu_supported(&self) -> bool {
        if self.caps.power {
            self.real.boot_menu_supported()
        } else {
            self.mock.boot_menu_supported()
        }
    }
    fn power(&self, action: PowerAction) -> Result<(), String> {
        if self.caps.power {
            self.real.power(action)
        } else {
            self.mock.power(action)
        }
    }
}

// ---------------------------------------------------------------------------------------------------
// Slow things off the UI thread

type Job = Box<dyn FnOnce() + Send>;

/// Runs jobs one after the other on a thread of its own. Jobs with the same key that are still waiting
/// when a newer one arrives are dropped: holding Vol+ queues twenty `volume N` calls, only the last matters.
#[derive(Clone)]
pub struct Worker {
    tx: Sender<(&'static str, Job)>,
}

impl Default for Worker {
    fn default() -> Self {
        Self::new()
    }
}

impl Worker {
    pub fn new() -> Self {
        let (tx, rx) = channel::<(&'static str, Job)>();
        std::thread::spawn(move || {
            while let Ok(first) = rx.recv() {
                let mut batch = vec![first];
                batch.extend(rx.try_iter());
                // Keep the last job of each key, in the order they were last submitted.
                let mut keep: Vec<(&'static str, Job)> = Vec::new();
                for (key, job) in batch.into_iter().rev() {
                    if key.is_empty() || !keep.iter().any(|(k, _)| *k == key) {
                        keep.push((key, job));
                    }
                }
                for (_, job) in keep.into_iter().rev() {
                    job();
                }
            }
        });
        Worker { tx }
    }

    /// `key` "" never replaces anything.
    pub fn submit(&self, key: &'static str, job: impl FnOnce() + Send + 'static) {
        let _ = self.tx.send((key, Box::new(job)));
    }
}

pub type SharedHw = Arc<dyn Hw>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn an_unset_gpu_overclock_gets_the_stock_clock() {
        // ROCKNIX 20261001 leaves the GPU at its highest OPP (925 MHz) when the setting is empty or missing.
        assert!(gpu_overclock_unset("system.cpugovernor=ondemand\ngpu-overclock-speed=\n"));
        assert!(gpu_overclock_unset("system.cpugovernor=ondemand\n"));
        assert!(!gpu_overclock_unset("gpu-overclock-speed=0\n"), "stock, chosen");
        assert!(!gpu_overclock_unset("gpu-overclock-speed=670\n"), "an overclock the player chose stays");
        assert!(gpu_overclock_unset("#gpu-overclock-speed=750\n"), "commented out counts as unset");
    }

    fn fake_root(name: &str) -> crate::testutil::TestDir {
        TestDir::new(&format!("sysctl-{name}"))
    }

    fn put(root: &Path, file: &str, content: &str) {
        let p = root.join(file);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, content).unwrap();
    }

    #[test]
    fn pactl_output_gives_the_percentage() {
        assert_eq!(
            parse_pactl_volume("Volume: front-left: 22937 /  35% / -27.36 dB,   front-right: 22937 /  35% / -27.36 dB\n        balance 0.00"),
            Some(35)
        );
        assert_eq!(parse_pactl_volume("Volume: front-left: 65536 / 100% / 0.00 dB"), Some(100));
        assert_eq!(parse_pactl_volume("Volume: mono: 0 /   0% / -inf dB"), Some(0));
        assert_eq!(parse_pactl_volume("Volume: 70000 / 107% / 1 dB"), Some(100), "over-amplified counts as full");
        assert_eq!(parse_pactl_volume("nothing here"), None);
    }

    #[test]
    fn pactl_output_gives_the_mute_state() {
        assert_eq!(parse_pactl_mute("Mute: yes\n"), Some(true));
        assert_eq!(parse_pactl_mute("Mute: no"), Some(false));
        // An error or another language: not known, so the row shows the level alone.
        assert_eq!(parse_pactl_mute("Failure: No such entity"), None);
        assert_eq!(parse_pactl_mute(""), None);
    }

    #[test]
    fn timedatectl_output_gives_the_ntp_state() {
        assert_eq!(parse_ntp("NTP=yes\nNTPSynchronized=yes\n"), NtpState::Synced);
        assert_eq!(parse_ntp("NTP=yes\nNTPSynchronized=no\n"), NtpState::Waiting);
        assert_eq!(parse_ntp("NTP=no\nNTPSynchronized=yes\n"), NtpState::Off);
        assert_eq!(parse_ntp(""), NtpState::Unknown);
    }

    #[test]
    fn brightness_is_read_and_written_in_percent_of_the_raw_range() {
        let root = fake_root("backlight");
        put(&root, "sys/class/backlight/panel/max_brightness", "255\n");
        put(&root, "sys/class/backlight/panel/brightness", "128\n");
        let hw = RealHw::with_root(&root);
        assert!(hw.has_backlight());
        assert_eq!(hw.brightness(), Some(50));
        hw.set_brightness(100);
        assert_eq!(read(&root.join("sys/class/backlight/panel/brightness")).as_deref(), Some("255"));
        hw.set_brightness(0);
        assert_eq!(
            read(&root.join("sys/class/backlight/panel/brightness")).as_deref(),
            Some("12"),
            "5%, never dark"
        );
        assert_eq!(hw.brightness(), Some(5));
        let empty = fake_root("no-backlight");
        let none = RealHw::with_root(empty.to_path_buf());
        assert!(none.brightness().is_none() && !none.has_backlight());
    }

    #[test]
    fn radios_come_from_rfkill_and_a_hard_block_counts() {
        let root = fake_root("rfkill");
        put(&root, "sys/class/rfkill/rfkill0/type", "wlan\n");
        put(&root, "sys/class/rfkill/rfkill0/soft", "0\n");
        put(&root, "sys/class/rfkill/rfkill0/hard", "0\n");
        put(&root, "sys/class/rfkill/rfkill1/type", "bluetooth\n");
        put(&root, "sys/class/rfkill/rfkill1/soft", "1\n");
        put(&root, "sys/class/rfkill/rfkill1/hard", "0\n");
        let hw = RealHw::with_root(&root);
        assert_eq!(hw.wifi(), Some(true));
        assert_eq!(hw.rfkill("bluetooth"), Some(false));
        put(&root, "sys/class/rfkill/rfkill0/hard", "1\n");
        assert_eq!(hw.wifi(), Some(false));
    }

    #[test]
    fn battery_and_charging_status() {
        let root = fake_root("battery");
        put(&root, "sys/class/power_supply/battery/capacity", "81\n");
        put(&root, "sys/class/power_supply/battery/status", "Charging\n");
        let hw = RealHw::with_root(&root);
        let b = hw.battery().unwrap();
        assert_eq!((b.percent, b.charge), (81, Charge::Charging));
        assert_eq!(b.label(), "81%, charging");
        assert_eq!(
            Battery {
                percent: 100,
                charge: Charge::Full
            }
            .label(),
            "Full"
        );
        assert_eq!(
            Battery {
                percent: 40,
                charge: parse_charge("Not charging")
            }
            .label(),
            "40%"
        );
    }

    #[test]
    fn zone_is_the_zoneinfo_file_localtime_points_to() {
        let root = fake_root("tz");
        put(&root, "usr/share/zoneinfo/Europe/Vienna", "x");
        std::fs::create_dir_all(root.join("etc")).unwrap();
        std::os::unix::fs::symlink(root.join("usr/share/zoneinfo/Europe/Vienna"), root.join("etc/localtime")).unwrap();
        let hw = RealHw::with_root(&root);
        assert_eq!(hw.timezone().as_deref(), Some("Europe/Vienna"));
        assert!(hw.set_timezone("Europe/../../etc/passwd").is_err());
        assert!(hw.set_timezone("Mars/Olympus").is_err(), "not in zoneinfo");
        put(&root, "proc/cmdline", "console=tty0 nodtbo quiet");
        assert!(hw.ram_booted());
    }

    /// The H700 image has no Steam helper: the zone goes through ROCKNIX's `set_setting` and `tz-data.service`.
    #[test]
    fn the_time_zone_works_without_the_steam_helper_on_rocknix() {
        let root = fake_root("tz-h700");
        put(&root, "run/systemd/system/.keep", "");
        let hw = RealHw::with_root(&root);
        assert!(!hw.has_time(), "no helper and no ROCKNIX functions: no time zone setting");
        put(&root, "etc/profile.d/001-functions", "set_setting() { :; }\n");
        assert!(hw.has_time(), "ROCKNIX's set_setting is enough");
        put(&root, "usr/bin/steamos-polkit-helpers/steamos-set-timezone", "#!/bin/sh\n");
        assert!(hw.has_time());
    }

    #[test]
    fn zone_tables_come_from_the_zoneinfo_folder() {
        let root = fake_root("tz-tables");
        put(&root, "usr/share/zoneinfo/zone.tab", "AT\t+4813+01620\tEurope/Vienna\n");
        let hw = RealHw::with_root(&root);
        assert_eq!(hw.zoneinfo_file("zone.tab").as_deref(), Some("AT\t+4813+01620\tEurope/Vienna\n"));
        assert_eq!(hw.zoneinfo_file("iso3166.tab"), None);
        // A mock answers with what it was given, and nothing from a zoneinfo folder that isn't there.
        let mock = MockHw::new(MockState {
            regdomain: Some("global\ncountry AT: DFS-ETSI\n".into()),
            ..MockState::default()
        })
        .with_zoneinfo("/nonexistent");
        assert_eq!(mock.regdomain().as_deref(), Some("global\ncountry AT: DFS-ETSI\n"));
        assert_eq!(mock.zoneinfo_file("zone.tab"), None);
    }

    #[test]
    fn the_mock_keeps_values_and_never_runs_power_actions() {
        let hw = MockHw::default();
        hw.set_brightness(0);
        assert_eq!(hw.brightness(), Some(MIN_BRIGHTNESS));
        hw.set_volume(150);
        assert_eq!(hw.volume(), Some(100));
        hw.set_wifi(false).unwrap();
        assert_eq!(hw.wifi(), Some(false));
        hw.power(PowerAction::Reboot).unwrap();
        hw.power(PowerAction::PowerOff).unwrap();
        assert_eq!(hw.state().power_log, [PowerAction::Reboot, PowerAction::PowerOff]);
        hw.set_timezone("Europe/Vienna").unwrap();
        assert_eq!(hw.timezone().as_deref(), Some("Europe/Vienna"));
        assert!(hw.set_timezone("bad zone").is_err());
        hw.sync_time();
        assert_eq!(hw.ntp(), NtpState::Synced);
    }

    /// Setting the clock by hand never reaches the computer running the tests: the mock records it, a `RealHw` on a
    /// test's own root refuses before any syscall.
    #[test]
    fn a_clock_set_by_hand_is_recorded_by_the_mock_and_refused_off_the_real_root() {
        let mock = MockHw::default();
        assert_eq!(mock.set_clock(1_793_019_900), Ok(ClockSet::Mock));
        assert_eq!(mock.state().clock_log, [1_793_019_900]);
        let clock_root = fake_root("clock");
        assert!(RealHw::with_root(clock_root.to_path_buf()).set_clock(1_793_019_900).is_err());
        // The virtual device (no systemd, no time service) takes the mock.
        let auto_root = fake_root("clock-auto");
        let auto = AutoHw::new(RealHw::with_root(auto_root.to_path_buf()), Caps::default());
        assert_eq!(auto.set_clock(1_793_019_900), Ok(ClockSet::Mock));
    }

    #[test]
    fn every_power_action_maps_to_its_systemctl_command() {
        assert_eq!(PowerAction::Sleep.argv(), ["suspend"]);
        assert_eq!(PowerAction::PowerOff.argv(), ["poweroff"]);
        assert_eq!(PowerAction::Reboot.argv(), ["reboot"]);
        assert_eq!(PowerAction::RebootBootloader.argv(), ["reboot", "--reboot-argument=bootloader"]);
        // Restart pocket-ui is no system command: never a bare `systemctl` (which would only list the units).
        assert!(PowerAction::RestartUi.argv().is_empty());
        let root = crate::testutil::TestDir::new("sysctl-restart-ui");
        assert!(RealHw::with_root(root.to_path_buf()).power(PowerAction::RestartUi).is_err());
    }

    #[test]
    fn the_fallback_disarms_for_power_off_and_restart_but_never_for_sleep() {
        let dir = crate::testutil::TestDir::new("power-via");
        let file = dir.join("wakealarm");
        let rtc = crate::auto_off::Rtc::at(file.clone());
        let ran = Mutex::new(Vec::new());
        let ok = |p: &str, a: &[&str]| {
            ran.lock().unwrap().push(format!("{p} {}", a.join(" ")));
            true
        };
        // Sleep is `systemctl suspend`: the process lives on, so the alarm stays and the gate stays open.
        std::fs::write(&file, "1793051031").unwrap();
        assert_eq!(power_via(PowerAction::Sleep, &rtc, &dir, &ok), Ok(()));
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "1793051031", "untouched");
        assert!(!rtc.shut() && rtc.set(Some(30)), "a later deep sleep can still arm its alarm");
        assert!(!dir.join("logs/shutdown.log").exists(), "no shutdown entry for a suspend");
        // Power off: disarmed first, the gate shut, the command run, logged.
        std::fs::write(&file, "1793051031").unwrap();
        assert_eq!(power_via(PowerAction::PowerOff, &rtc, &dir, &ok), Ok(()));
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "0");
        assert!(rtc.shut());
        assert_eq!(*ran.lock().unwrap(), ["systemctl suspend", "systemctl poweroff"]);
        let log = std::fs::read_to_string(dir.join("logs/shutdown.log")).unwrap();
        assert!(log.contains("RTC alarm disarm verified") && log.contains("systemctl ended: exit 0"), "{log}");
        // A restart that fails opens the gate again.
        rtc.reopen();
        let fails = |_: &str, _: &[&str]| false;
        assert!(power_via(PowerAction::Reboot, &rtc, &dir, &fails).is_err());
        assert!(!rtc.shut());
    }

    #[test]
    fn the_bootloader_mode_is_read_from_the_reboot_mode_class() {
        // An empty list is what the Mini V2's kernel has today: the PON node defines no mode-* property.
        assert!(!reboot_modes_have_bootloader(""));
        assert!(!reboot_modes_have_bootloader("recovery\t1\n"));
        assert!(reboot_modes_have_bootloader("bootloader\t2\nrecovery\t1\n"));
        assert!(reboot_modes_have_bootloader("mode-bootloader = 0x2\n"));
        assert!(!reboot_modes_have_bootloader("bootloader-fake\n"));

        let root = fake_root("bootmenu");
        assert!(!RealHw::with_root(&root).boot_menu_supported(), "no reboot-mode device at all");
        put(&root, "sys/class/reboot-mode/qcom-pon/reboot_modes", "");
        assert!(!RealHw::with_root(&root).boot_menu_supported(), "the device is there but knows no mode");
        put(&root, "sys/class/reboot-mode/qcom-pon/reboot_modes", "bootloader\t2\n");
        assert!(RealHw::with_root(&root).boot_menu_supported());
    }

    #[test]
    fn the_mock_offers_the_boot_menu_unless_told_not_to() {
        assert!(MockHw::default().boot_menu_supported());
        let off = MockHw::new(MockState { boot_menu: false, ..MockState::default() });
        assert!(!off.boot_menu_supported());
        off.power(PowerAction::RebootBootloader).unwrap();
        assert_eq!(off.state().power_log, [PowerAction::RebootBootloader]);
    }

    #[test]
    fn auto_uses_the_mock_for_what_the_machine_lacks() {
        let auto_root = fake_root("auto");
        let hw = AutoHw::new(RealHw::with_root(auto_root.to_path_buf()), Caps::default());
        assert_eq!(hw.mocked().len(), 7);
        hw.set_brightness(30);
        assert_eq!(hw.brightness(), Some(30));
        assert!(hw.battery().is_some());
        assert!(hw.zones().iter().any(|z| z == "Europe/Vienna"));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_wifi_radio_that_appears_after_the_start_is_the_real_one() {
        // The chip's driver came up after pocket-ui started: the Wi-Fi row must not stay a mock for the whole run.
        let root = fake_root("auto-late-wifi");
        let hw = AutoHw::new(RealHw::with_root(&root), Caps::default());
        assert_eq!(hw.wifi_radio(), crate::net_state::Radio::On, "the mock's, until there is a radio");
        put(&root, "sys/class/rfkill/rfkill0/type", "wlan\n");
        put(&root, "sys/class/rfkill/rfkill0/soft", "1\n");
        put(&root, "sys/class/rfkill/rfkill0/hard", "0\n");
        std::fs::create_dir_all(root.join("usr/bin")).unwrap();
        crate::testutil::write_executable(&root.join("usr/bin/wifictl"), "#!/bin/sh\nexit 0\n");
        assert!(hw.wifi_radio().is_off(), "now the system's");
        assert_eq!(hw.wifi(), Some(false));
    }

    #[test]
    fn the_worker_drops_superseded_jobs_with_the_same_key() {
        let w = Worker::new();
        let (gate_tx, gate_rx) = channel::<()>();
        let ran = Arc::new(Mutex::new(Vec::new()));
        // Block the worker so the next jobs queue up.
        w.submit("", move || {
            let _ = gate_rx.recv();
        });
        for n in 1..=5 {
            let ran = ran.clone();
            w.submit("volume", move || ran.lock().unwrap().push(n));
        }
        let other = ran.clone();
        w.submit("wifi", move || other.lock().unwrap().push(100));
        let _ = gate_tx.send(());
        let done = Arc::new(AtomicUsize::new(0));
        let d = done.clone();
        w.submit("", move || d.store(1, Ordering::SeqCst));
        for _ in 0..200 {
            if done.load(Ordering::SeqCst) == 1 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(*ran.lock().unwrap(), [5, 100], "only the newest volume job ran, in order with the wifi one");
    }

    // -----------------------------------------------------------------------------------------------------
    // Wi-Fi with stand-ins for rfkill (a fake sysfs), wifictl and nmcli (scripts below the fake root's usr/bin)

    /// A fake root with a Wi-Fi radio: rfkill0 (`soft` "1"/"0"), a `wifictl` that flips the soft block like ROCKNIX's
    /// does (`wifictl_works`: it only logs and exits 1 when not), an `nmcli` that answers `radio wifi` from the file
    /// `nm_radio` and a scan from `scan_out` (or fails with `scan_fail` present). Every call is logged in `calls`.
    #[cfg(target_os = "linux")]
    fn wifi_root(name: &str, soft: &str, nm_radio: &str, wifictl_works: bool) -> crate::testutil::TestDir {
        let root = fake_root(name);
        put(&root, "sys/class/rfkill/rfkill0/type", "wlan\n");
        put(&root, "sys/class/rfkill/rfkill0/soft", &format!("{soft}\n"));
        put(&root, "sys/class/rfkill/rfkill0/hard", "0\n");
        put(&root, "nm_radio", &format!("{nm_radio}\n"));
        put(&root, "scan_out", "");
        put(&root, "storage/.config/system/configs/system.cfg", "wifi.enabled=0\n");
        std::fs::create_dir_all(root.join("usr/bin")).unwrap();
        let r = root.display();
        let flip = if wifictl_works {
            format!("case \"$1\" in enable) echo 0 > {r}/sys/class/rfkill/rfkill0/soft;; disable) echo 1 > {r}/sys/class/rfkill/rfkill0/soft;; esac\n")
        } else {
            "echo \"rfkill: no such device\" >&2\nexit 1\n".to_string()
        };
        crate::testutil::write_executable(&root.join("usr/bin/wifictl"), format!("#!/bin/sh\necho \"wifictl $*\" >> {r}/calls\n{flip}"));
        crate::testutil::write_executable(
            &root.join("usr/bin/nmcli"),
            format!(
                "#!/bin/sh\necho \"nmcli $*\" >> {r}/calls\ncase \"$*\" in\n\"radio wifi\") cat {r}/nm_radio;;\n\"radio wifi on\") echo enabled > {r}/nm_radio;;\n*\"dev wifi list\"*) if [ -f {r}/scan_fail ]; then echo \"Error: No Wi-Fi device found.\" >&2; exit 10; fi; cat {r}/scan_out;;\nesac\n"
            ),
        );
        root
    }

    #[cfg(target_os = "linux")]
    fn calls(root: &Path) -> Vec<String> {
        read(&root.join("calls")).map(|c| c.lines().map(String::from).collect()).unwrap_or_default()
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn the_wifi_radio_is_rfkills_soft_and_hard_block_and_networkmanagers_switch() {
        use crate::net_state::{Cause, Radio};
        let radio = |root: &Path| RealHw::with_root(root).wifi_radio();
        assert_eq!(radio(&wifi_root("radio-soft", "1", "enabled", true)), Radio::Off(Cause::Soft));
        assert_eq!(radio(&wifi_root("radio-on", "0", "enabled", true)), Radio::On);
        assert_eq!(radio(&wifi_root("radio-nm", "0", "disabled", true)), Radio::Off(Cause::Manager), "rfkill lets it through, NetworkManager does not");
        let hard = wifi_root("radio-hard", "0", "enabled", true);
        put(&hard, "sys/class/rfkill/rfkill0/hard", "1\n");
        assert_eq!(radio(&hard), Radio::Off(Cause::Hard));
        // No nmcli answer: rfkill decides alone.
        let quiet = wifi_root("radio-no-nm", "0", "garbage", true);
        assert_eq!(radio(&quiet), Radio::On);
        assert_eq!(radio(&fake_root("radio-none")), Radio::Absent, "no radio at all");
        // NetworkManager's switch is not asked at every look.
        let root = wifi_root("radio-cached", "0", "enabled", true);
        let hw = RealHw::with_root(&root);
        hw.wifi_radio();
        hw.wifi_radio();
        assert_eq!(calls(&root).iter().filter(|c| *c == "nmcli radio wifi").count(), 1);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn switching_wifi_on_unblocks_the_radio_switches_networkmanagers_on_and_reads_it_back() {
        use crate::net_state::Radio;
        let root = wifi_root("switch-on", "1", "disabled", true);
        let hw = RealHw::with_root(&root).with_settle(std::time::Duration::from_secs(2));
        assert_eq!(hw.set_wifi(true), Ok(()));
        assert_eq!(hw.wifi_radio(), Radio::On);
        let calls = calls(&root);
        assert!(calls.contains(&"wifictl enable".to_string()) && calls.contains(&"nmcli radio wifi on".to_string()), "{calls:?}");
        // Off again: rfkill blocks.
        assert_eq!(hw.set_wifi(false), Ok(()));
        assert!(hw.wifi_radio().is_off());
        assert_eq!(read(&root.join("sys/class/rfkill/rfkill0/soft")).as_deref(), Some("1"));
        // wifi.enabled was not touched by switching: the wish is stored apart, after the switch worked.
        assert_eq!(hw.wifi_wanted(), Some(false));
        assert!(hw.persist_wifi(true));
        assert_eq!(hw.wifi_wanted(), Some(true));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_wifictl_that_fails_or_changes_nothing_is_an_error_not_a_success() {
        let root = wifi_root("switch-fails", "1", "enabled", false);
        let hw = RealHw::with_root(&root).with_settle(std::time::Duration::from_millis(150));
        let err = hw.set_wifi(true).unwrap_err();
        assert!(err.contains("wifictl enable exited with 1") && err.contains("no such device"), "{err}");
        assert!(hw.wifi_radio().is_off());
        // Exit 0, radio unchanged (the chip is not there yet): still no success.
        let root = wifi_root("switch-nothing", "1", "enabled", true);
        crate::testutil::write_executable(&root.join("usr/bin/wifictl"), "#!/bin/sh\nexit 0\n");
        let hw = RealHw::with_root(&root).with_settle(std::time::Duration::from_millis(150));
        let err = hw.set_wifi(true).unwrap_err();
        assert!(err.contains("still"), "{err}");
    }

    /// The pieces of `HwNet` on a fake root: the radio through `RealHw`, the scan through the fake `nmcli`.
    #[cfg(target_os = "linux")]
    struct RootNet {
        hw: RealHw,
        nmcli: PathBuf,
    }

    #[cfg(target_os = "linux")]
    impl crate::net_state::NetOps for RootNet {
        fn radio(&self) -> crate::net_state::Radio {
            self.hw.wifi_radio()
        }
        fn set_radio(&self, on: bool) -> Result<(), String> {
            self.hw.set_wifi(on)
        }
        fn wanted(&self) -> Option<bool> {
            self.hw.wifi_wanted()
        }
        fn persist_wanted(&self, on: bool) -> bool {
            self.hw.persist_wifi(on)
        }
        fn saving(&self, _stop: &std::sync::atomic::AtomicBool) -> (crate::wifi::Saving, String) {
            (crate::wifi::Saving::default(), String::new())
        }
        fn address(&self) -> Option<String> {
            None
        }
        fn scan(&self, stop: &std::sync::atomic::AtomicBool) -> Result<Vec<crate::wifi::Network>, crate::net_state::ScanError> {
            crate::wifi::scan_until(&self.nmcli, stop)
        }
    }

    #[cfg(target_os = "linux")]
    fn root_net(root: &Path) -> Arc<crate::net_state::NetState> {
        let hw = RealHw::with_root(root).with_settle(std::time::Duration::from_millis(200));
        crate::net_state::NetState::inline(Arc::new(RootNet { hw, nmcli: root.join("usr/bin/nmcli") }))
    }

    /// The toggle with the system's own tools, both ways: the wish in system.cfg follows a switch that worked and only that.
    #[cfg(target_os = "linux")]
    #[test]
    fn toggling_wifi_stores_the_wish_after_the_radio_changed_and_not_when_wifictl_failed() {
        use crate::net_state::{Cause, Radio};
        let root = wifi_root("toggle-ok", "1", "enabled", true);
        put(&root, "scan_out", "Home:78:WPA2:*\nCafe guest:41::\n");
        let net = root_net(&root);
        net.toggle();
        let s = net.snapshot();
        assert_eq!((s.radio, s.wanted, s.notice), (Radio::On, Some(true), None));
        assert!(read(&root.join("storage/.config/system/configs/system.cfg")).unwrap().contains("wifi.enabled=1"));
        assert!(matches!(&s.scan, crate::net_state::Scan::Found(n) if n.len() == 2), "turned on: the networks are listed: {:?}", s.scan);
        net.toggle();
        let s = net.snapshot();
        assert_eq!((s.radio, s.wanted), (Radio::Off(Cause::Soft), Some(false)));
        assert!(read(&root.join("storage/.config/system/configs/system.cfg")).unwrap().contains("wifi.enabled=0"));

        // wifictl fails: the file keeps its value, the switch shows the radio as it is, the snapshot carries the notice.
        let root = wifi_root("toggle-fails", "1", "enabled", false);
        let net = root_net(&root);
        net.toggle();
        let s = net.snapshot();
        assert_eq!((s.radio, s.wanted), (Radio::Off(Cause::Soft), Some(false)));
        assert!(read(&root.join("storage/.config/system/configs/system.cfg")).unwrap().contains("wifi.enabled=0"), "nothing stored");
        let n = s.notice.expect("a notice");
        assert!(n.on && n.why.contains("exited with 1"), "{n:?}");
    }

    /// The boot of the incident with the wish off: the radio is blocked, pocket-ui never shows "No networks found" for
    /// that and never switches it on; when the hooks let the radio go, one scan lists the networks without any press.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_radio_blocked_at_boot_is_off_not_empty_and_the_networks_come_when_it_is_released() {
        use crate::net_state::{Cause, Radio, Scan};
        let root = wifi_root("boot-block", "1", "enabled", true);
        put(&root, "storage/.config/system/configs/system.cfg", "wifi.enabled=0\n");
        put(&root, "scan_out", "Home:78:WPA2:*\n");
        let net = root_net(&root);
        net.request_scan();
        let s = net.snapshot();
        assert_eq!((s.radio, s.wanted, s.scan.clone()), (Radio::Off(Cause::Soft), Some(false), Scan::NotYet));
        assert!(!calls(&root).iter().any(|c| c.contains("dev wifi list") || c.starts_with("wifictl")), "nothing searched, nothing switched: {:?}", calls(&root));
        // ~14 s later the boot hooks unblock it.
        put(&root, "sys/class/rfkill/rfkill0/soft", "0\n");
        net.poll();
        let s = net.snapshot();
        assert_eq!(s.radio, Radio::On);
        assert!(matches!(&s.scan, Scan::Found(n) if n.len() == 1), "{:?}", s.scan);
    }

    /// The same boot with the wish on (`wifi.enabled=1`, the player's own choice): the radio is switched on once, with
    /// the system's own tools, and the networks are listed.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_radio_blocked_at_boot_while_the_wish_is_on_is_switched_on_and_listed() {
        use crate::net_state::{Radio, Scan};
        let root = wifi_root("boot-unblock", "1", "enabled", true);
        put(&root, "storage/.config/system/configs/system.cfg", "wifi.enabled=1\n");
        put(&root, "scan_out", "Home:78:WPA2:*\n");
        let net = root_net(&root);
        net.request_scan();
        let s = net.snapshot();
        assert_eq!((s.radio, s.wanted), (Radio::On, Some(true)));
        assert!(matches!(&s.scan, Scan::Found(n) if n.len() == 1), "{:?}", s.scan);
        assert_eq!(calls(&root).iter().filter(|c| *c == "wifictl enable").count(), 1);
        assert!(read(&root.join("storage/.config/system/configs/system.cfg")).unwrap().contains("wifi.enabled=1"));
        // A radio that will not come on is tried once and left, for the page to say "Wi-Fi is off".
        let root = wifi_root("boot-unblock-fails", "1", "enabled", false);
        put(&root, "storage/.config/system/configs/system.cfg", "wifi.enabled=1\n");
        let net = root_net(&root);
        net.request_scan();
        net.poll();
        net.poll();
        assert!(net.snapshot().radio.is_off() && net.snapshot().notice.is_none());
        assert_eq!(calls(&root).iter().filter(|c| *c == "wifictl enable").count(), 1);
    }

    /// Looking for the radio's state does not run `nmcli` at every 100 ms of the wait (it is a process).
    #[cfg(target_os = "linux")]
    #[test]
    fn waiting_for_the_radio_asks_networkmanager_only_now_and_then() {
        let root = wifi_root("settle-nm", "0", "disabled", true);
        // An nmcli that never comes round: `radio wifi on` changes nothing.
        crate::testutil::write_executable(
            &root.join("usr/bin/nmcli"),
            format!("#!/bin/sh\necho \"nmcli $*\" >> {r}/calls\ncase \"$*\" in \"radio wifi\") cat {r}/nm_radio;; esac\n", r = root.display()),
        );
        let hw = RealHw::with_root(&root).with_settle(std::time::Duration::from_millis(450));
        let err = hw.set_wifi(true).unwrap_err();
        assert!(err.contains("still"), "{err}");
        let asked = calls(&root).iter().filter(|c| *c == "nmcli radio wifi").count();
        assert!(asked <= 3, "{asked} looks at NetworkManager in 0.45 s (one per 100 ms would be 6)");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_scan_that_nmcli_refused_is_an_error_and_one_that_found_nobody_is_empty() {
        use crate::net_state::Scan;
        let root = wifi_root("scan-kinds", "0", "enabled", true);
        // Fails (exit 10, "Error: No Wi-Fi device found."): typed, with the code and the last line.
        put(&root, "scan_fail", "1\n");
        let net = root_net(&root);
        net.request_scan();
        match net.snapshot().scan {
            Scan::Failed(e) => assert_eq!((e.code, e.detail.as_str()), (Some(10), "No Wi-Fi device found."), "{e}"),
            other => panic!("{other:?}"),
        }
        // Works, sees nobody: an empty list.
        let root = wifi_root("scan-empty", "0", "enabled", true);
        let net = root_net(&root);
        net.request_scan();
        assert_eq!(net.snapshot().scan, Scan::Found(Vec::new()));
        // No nmcli at all: also an error, never an empty list.
        let root = wifi_root("scan-no-nmcli", "0", "enabled", true);
        std::fs::remove_file(root.join("usr/bin/nmcli")).unwrap();
        let net = root_net(&root);
        net.request_scan();
        assert!(matches!(net.snapshot().scan, Scan::Failed(e) if e.code.is_none()));
    }
}

/// Tests: a system that notes every look at it made on the thread that made the spy (the test's own, standing for
/// the UI thread). A page that reads the system while it opens, paints or handles a key lands in `on_ui`; what a
/// worker thread reads does not. Writes (set_*, power) are not looks and are not noted.
#[cfg(test)]
pub(crate) mod spy {
    use super::*;
    use std::sync::Mutex;

    pub(crate) struct Spy {
        pub inner: MockHw,
        ui: std::thread::ThreadId,
        pub on_ui: Mutex<Vec<&'static str>>,
    }

    impl Spy {
        /// Notes the looks made on the calling thread.
        pub fn new(inner: MockHw) -> Spy {
            Spy { inner, ui: std::thread::current().id(), on_ui: Mutex::new(Vec::new()) }
        }

        fn note(&self, what: &'static str) {
            if std::thread::current().id() == self.ui {
                self.on_ui.lock().unwrap().push(what);
            }
        }

        /// What was noted so far, and forgotten again.
        pub fn take(&self) -> Vec<&'static str> {
            std::mem::take(&mut *self.on_ui.lock().unwrap())
        }
    }

    impl Hw for Spy {
        fn brightness(&self) -> Option<u8> {
            self.note("brightness");
            self.inner.brightness()
        }
        fn set_brightness(&self, pct: u8) {
            self.inner.set_brightness(pct)
        }
        fn persist_brightness(&self, pct: u8) {
            self.inner.persist_brightness(pct)
        }
        fn volume(&self) -> Option<u8> {
            self.note("volume (pactl)");
            self.inner.volume()
        }
        fn muted(&self) -> Option<bool> {
            self.note("muted (pactl)");
            self.inner.muted()
        }
        fn set_volume(&self, pct: u8) {
            self.inner.set_volume(pct)
        }
        fn sound(&self) -> Option<crate::audio_route::Sound> {
            self.note("sound (pw-metadata)");
            self.inner.sound()
        }
        fn wifi(&self) -> Option<bool> {
            self.note("wifi (rfkill)");
            self.inner.wifi()
        }
        fn wifi_radio(&self) -> Radio {
            self.note("wifi (rfkill, nmcli radio)");
            self.inner.wifi_radio()
        }
        fn set_wifi(&self, on: bool) -> Result<(), String> {
            self.inner.set_wifi(on)
        }
        fn wifi_wanted(&self) -> Option<bool> {
            self.note("wifi.enabled (system.cfg)");
            self.inner.wifi_wanted()
        }
        fn persist_wifi(&self, on: bool) -> bool {
            self.inner.persist_wifi(on)
        }
        fn bluetooth(&self) -> Option<bool> {
            self.note("bluetooth (systemctl)");
            self.inner.bluetooth()
        }
        fn set_bluetooth(&self, on: bool) {
            self.inner.set_bluetooth(on)
        }
        fn battery(&self) -> Option<Battery> {
            self.note("battery");
            self.inner.battery()
        }
        fn timezone(&self) -> Option<String> {
            self.note("timezone (/etc/localtime)");
            self.inner.timezone()
        }
        fn set_timezone(&self, zone: &str) -> Result<(), String> {
            self.inner.set_timezone(zone)
        }
        fn zones(&self) -> Vec<String> {
            self.note("zones (zoneinfo tree)");
            self.inner.zones()
        }
        fn zone_offset(&self, zone: &str, at: i64) -> Option<i32> {
            self.note("zone_offset (zoneinfo file)");
            self.inner.zone_offset(zone, at)
        }
        fn regdomain(&self) -> Option<String> {
            self.note("regdomain (iw reg get)");
            self.inner.regdomain()
        }
        fn zoneinfo_file(&self, name: &str) -> Option<String> {
            self.note("zoneinfo_file");
            self.inner.zoneinfo_file(name)
        }
        fn ntp(&self) -> NtpState {
            self.note("ntp (timedatectl)");
            self.inner.ntp()
        }
        fn sync_time(&self) {
            self.inner.sync_time()
        }
        fn set_clock(&self, unix: i64) -> Result<ClockSet, String> {
            self.note("set_clock (clock_settime, RTC)");
            self.inner.set_clock(unix)
        }
        fn ram_booted(&self) -> bool {
            self.note("ram_booted (/proc/cmdline)");
            self.inner.ram_booted()
        }
        fn boot_menu_supported(&self) -> bool {
            self.note("boot_menu_supported (sysfs)");
            self.inner.boot_menu_supported()
        }
        fn power(&self, action: PowerAction) -> Result<(), String> {
            self.inner.power(action)
        }
    }
}
