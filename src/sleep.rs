//! Game-aware sleep, in two stages like NextUI's (docs/SLEEP.md):
//!
//! - **Light sleep** after a while without input (Settings → System → Sleep & power, "Screen off after": Never, 30 s,
//!   1, 2 or 5 min; 2 by default, `sleep_settings`):
//!   the screen goes dark and the game pauses (pocket-core `sleep`, every other emulator SIGSTOP). Any
//!   button wakes it at once and the game goes on where it was. Input of any kind, a game being played
//!   included, keeps it from starting.
//! - **Deep sleep** "Sleep after" into light sleep (Never, 1, 2, 5, 15 min; 2 by default), or at once with the power button or the quick
//!   settings' Sleep (`Sleep::sleep_now`): the game writes the state the next start resumes from
//!   (pocket-core `autosave`, ARMSX2/Dolphin through `standalone`), then the system suspends through
//!   ROCKNIX's normal path (`systemctl suspend`), so its sleep hooks and quirks still run. RetroArch keeps
//!   its game in memory (it has no network command for an auto state without touching a slot).
//! - **Waking** from deep sleep ("instant on", docs/SLEEP.md#waking-timeline-instant-on-2026-10-24): as soon as the system is back
//!   (`DeepSteps::resumed`, noticed on CLOCK_BOOTTIME and `/sys/power/suspend_stats`, not by a nap that sleeps on)
//!   the game continues and the screen comes on, on a thread of its own (`System::screen_on_soon`), as NextUI does.
//!   ROCKNIX's wake hooks run meanwhile; only what has to come after them waits for them: the volume (see below,
//!   set once at once and again after the hooks), pocket-core lowering PipeWire's quantum again (the hook resets
//!   it to 1024), and the backlight once more should a hook have turned it off. A game that was behind the open
//!   pause menu stays paused behind it.
//!
//! **Volume after wake.** ROCKNIX's wake hook (`/usr/lib/systemd/system-sleep/sleep post`) logs
//! "Restoring volume to <audio.volume>%" and runs `amixer -c 0 -M set "$DEVICE_AUDIO_MIXER" <n>%`.
//! The SM8250 quirks never set DEVICE_AUDIO_MIXER, so that amixer call gets an empty control name and
//! does nothing; the real volume lives in PipeWire's sink, which only ROCKNIX's `volume` script (the Vol
//! keys) writes, together with `audio.volume` in system.cfg. After the suspend the sink came back silent
//! until Vol+ ran `volume up`, which writes the sink again. So pocket-ui reads the sink's volume itself
//! before it suspends, writes it to `audio.volume` (through `volume <n>`, so the hook logs the real value),
//! and after the wake writes the sink again (one step off and back, so PipeWire pushes it to the codec
//! even when it thinks nothing changed). Suspends pocket-ui didn't start (logind handling the key when
//! the inhibitor below isn't held) are noticed from the clock and get the same repair with the volume
//! read last (every 15 s while awake).
//!
//! **Power key.** logind suspends on the key by itself (`HandlePowerKey=suspend`), which would skip the
//! save. pocket-ui holds a `handle-power-key` inhibitor (`systemd-inhibit … cat`, released when pocket-ui
//! ends because the pipe closes) and handles the key only while that is held; otherwise logind keeps it.
//! A short press sleeps when the key goes up; held for `powerkey::LONG_PRESS` it asks for the power menu
//! (`take_menu_request`, see `powerkey.rs` for the rules) and the release after that does nothing. Sleep & power can
//! make the short press turn only the screen off with the game going on (`screen_off_now`, docs/SLEEP.md "Screen off
//! with the game running") and the hold sleep at its release.
//!
//! **Switch off after sleeping** (L3, `auto_off`): after the hours set in Settings → System → Sleep & power in deep sleep on battery,
//! the RTC alarm wakes the system with the screen dark and the Power off path below runs (`Why::Slept`); on the
//! charger it suspends again. A wake before that time costs one clock read on its way to the screen.
//!
//! **Power off** (`Sleep::power_off`, for the quick settings): the game saves and quits, a resume note is
//! left in the data dir, and the system powers off. The next start of pocket-ui opens that game again
//! (`take_resume`). Note: this Mini V2 boots ROCKNIX from RAM (`fastboot boot` from a computer), so after
//! power off and on it starts Android; the game resumes the next time ROCKNIX is booted.
//!
//! Testing without the real hardware: `POCKETUI_SUSPEND_CMD` / `POCKETUI_POWEROFF_CMD` replace the
//! systemctl calls (a command that blocks for a few seconds counts as a suspend), `POCKETUI_BACKLIGHT`
//! points at a backlight directory, `POCKETUI_POWER_KEY=1` handles the key without the inhibitor,
//! `POCKETUI_SLEEP_MINUTE_SECS` shortens a "minute", and `POCKETUI_NO_AUTOSUSPEND=1` or a file
//! `<data>/no-autosuspend` keeps idle from ever going past light sleep (the power key still suspends; the
//! low-battery guard, `low_battery`, still saves and powers off at 2 % on battery). `POCKETUI_BATTERY` points the
//! guard at a battery directory too.
//! `POCKETUI_SLEEP_DPMS=1` also switches the panel off through sway (to be tried on the ch13726a).
//! `POCKETUI_RTC_ALARM` replaces `/sys/class/rtc/rtc0/wakealarm` (switch off after sleeping, and the disarm before
//! every power off and restart: `auto_off::disarm_before_power_off`, logged in `shutdown.log`).

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::control;
use crate::pad::{Axis, Button, PadEvent};
use crate::powerkey::{Gate, PowerKey, Step};
use crate::sleep_settings::{Hold, ShortPress};
use crate::system_ctl::PowerAction;

/// Presses this soon after waking are the wake press itself (or its bounce), not a new request.
const WAKE_GUARD: Duration = Duration::from_millis(1500);
/// After the screen wakes, buttons do nothing for this long: the press that woke it is swallowed already, and a
/// second one right after (a pocket, a bag, a double tap) must not start the game Home has selected. On the Mini V2
/// a wake and the start of F-Zero fell into the same second with nobody playing (2026-10-23).
///
/// Reconsidered for instant on (2026-10-24), kept at 600 ms: it counts from the moment the screen is told to come
/// on (`wake_light`), so it never delays the picture, and it only swallows pocket-ui's own buttons (shelf, pause
/// menu). A running game reads the pad itself and gets every press at once (pocket-core only hides the button that
/// woke it until it is let go). A press meant as an answer to what the screen shows comes after a visual choice
/// reaction (about 400-500 ms) plus the thumb's travel; what comes sooner was pressed blind, which is the case this
/// guards against. Shorter would let the pocket press of 2026-10-23 through again.
const WAKE_QUIET: Duration = Duration::from_millis(600);
/// Stick movement smaller than this is drift, not input.
const STICK_DEADZONE: f32 = 0.35;
/// A suspend that hasn't happened this long after asking for it failed.
const SUSPEND_TIMEOUT: Duration = Duration::from_secs(30);
/// A suspend command that blocks this long stands for a suspend (`POCKETUI_SUSPEND_CMD` in tests and the virtual
/// device); `systemctl suspend` returns at once.
const BLOCKING_SUSPEND: Duration = Duration::from_secs(3);
/// How often the deep sleep's worker looks whether the system has slept. The nap is on CLOCK_BOOTTIME, so a suspend
/// ends it the moment the system is back; this only bounds a suspend that `suspend_stats` sees late.
const RESUME_POLL: Duration = Duration::from_millis(50);
/// How often the wait for ROCKNIX's wake hooks looks for `systemd-sleep` (only the volume and the quantum wait for it).
const HOOKS_POLL: Duration = Duration::from_millis(50);
/// How long a power-off waits for a store package being installed or removed (`store::changing`; seconds normally).
const STORE_WAIT: Duration = Duration::from_secs(30);
/// The first wait before a failed suspend is tried again when Sleep after is Never.
const RETRY_FIRST: Duration = Duration::from_secs(120);
/// The longest wait before a failed suspend is tried again (the waits double from Sleep after's minutes).
const RETRY_AT_MOST: Duration = Duration::from_secs(30 * 60);
const CONFIG_FILE: &str = "sleep.json";

impl Config {
    pub fn load(data_dir: &Path) -> Self {
        let saved: Option<Config> = crate::fsutil::read_json_or_backup(&data_dir.join(CONFIG_FILE));
        let mut config = saved.unwrap_or_default();
        if saved.is_none() && crate::device::current().screen_off_only() {
            config.off_after = None;
        }
        config.for_device(crate::device::current().screen_off_only())
    }

    pub fn save(&self, data_dir: &Path) {
        let _ = crate::fsutil::write_json(&data_dir.join(CONFIG_FILE), self);
    }
}
/// The brightness to come back to, kept outside the process: a pocket-ui that ends while the screen is
/// dark (crash, update, deploy) leaves it here, and the next one turns the screen back on.
const DARK_FILE: &str = "/tmp/pocket-ui.brightness";
const RESUME_FILE: &str = "resume.json";

/// The command for a power action, as the shell runs it: `systemctl` with `PowerAction::argv`.
fn default_command(action: PowerAction) -> String {
    format!("systemctl {}", action.argv().join(" "))
}

/// What the screen says while the device shuts down for `action` (after the game was saved).
fn shutdown_text(action: PowerAction) -> &'static str {
    match action {
        PowerAction::Reboot => "Restarting\u{2026}",
        PowerAction::RebootBootloader => "Restarting to the boot menu\u{2026}",
        PowerAction::RestartUi => "Restarting pocket-ui\u{2026}",
        PowerAction::PowerOff | PowerAction::Sleep => "Switching off\u{2026}",
    }
}

fn minute() -> Duration {
    Duration::from_secs(std::env::var("POCKETUI_SLEEP_MINUTE_SECS").ok().and_then(|v| v.parse().ok()).unwrap_or(60))
}

/// Settings → System → Sleep & power (`sleep_settings`), kept in `<data>/sleep.json`.
pub use crate::sleep_settings::Config;

/// A time of the settings as the timers count it: `POCKETUI_SLEEP_MINUTE_SECS` shortens every minute alike.
fn scaled_secs(secs: u32) -> Duration {
    minute() * secs / 60
}

/// The running game, as far as sleeping and powering off need to know it.
#[derive(Clone, Debug)]
pub struct Game {
    pub system: String,
    pub name: String,
    pub path: PathBuf,
    pub kind: GameKind,
    /// Process group of the game (pocket-ui starts each game in its own).
    pub pgid: Option<i32>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum GameKind {
    /// pocket-core, with the auto-resume state it writes on `autosave`.
    PocketCore {
        auto_state: PathBuf,
    },
    RetroArch,
    /// ARMSX2, Dolphin or Azahar with Save/Load support (`standalone`).
    Standalone,
    Other,
}

impl Game {
    pub fn new(system: &str, name: &str, path: &Path, emulator: &str, pgid: Option<i32>) -> Game {
        let kind = match emulator {
            "pocket-core" => {
                let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("game");
                GameKind::PocketCore {
                    auto_state: crate::slots::auto_path(&crate::slots::base(system, stem)),
                }
            }
            "retroarch" => GameKind::RetroArch,
            _ if crate::standalone::active() => GameKind::Standalone,
            _ => GameKind::Other,
        };
        Game {
            system: system.into(),
            name: name.into(),
            path: path.into(),
            kind,
            pgid,
        }
    }

    /// Stops the game where it is (light sleep).
    fn pause(&self) {
        match self.kind {
            GameKind::PocketCore { .. } => {
                control::pocket_core("sleep");
            }
            _ => {
                if let Some(g) = self.pgid {
                    control::pause_game(g);
                }
            }
        }
    }

    /// Lets it go on. `stopped`: the processes were stopped by us (not by the open pause menu).
    fn resume(&self, stopped: bool) {
        match self.kind {
            GameKind::PocketCore { .. } => {
                control::pocket_core("wake");
            }
            _ => {
                if let (true, Some(g)) = (stopped, self.pgid) {
                    control::resume_game(g);
                }
            }
        }
    }

    /// Writes the state the next start resumes from, before a suspend. Blocks for up to ~12 s.
    fn save_for_sleep(&self, stopped: bool) -> bool {
        match &self.kind {
            GameKind::PocketCore { auto_state } => {
                // Only a new version of the state confirms it, not one written a moment before (SOL-12).
                let before = crate::fsutil::stamp(auto_state);
                control::pocket_core("autosave")
                    && crate::fsutil::wait_new_version(auto_state, before, Duration::from_secs(10), Duration::from_millis(150), None)
            }
            GameKind::Standalone => {
                // The emulator answers PINE and its hotkeys only while running; it is stopped again after.
                let Some(g) = self.pgid else { return false };
                control::resume_game(g);
                let ok = crate::standalone::save_resume_state();
                if stopped {
                    control::pause_game(g);
                }
                ok
            }
            // RetroArch has no network command for its auto state that leaves the save slots alone.
            GameKind::RetroArch | GameKind::Other => false,
        }
    }

    /// Saves and ends the game before power off; waits until its processes are gone (up to ~25 s).
    fn quit_for_power_off(&self) {
        let Some(g) = self.pgid else { return };
        control::resume_game(g);
        match self.kind {
            // Quitting writes the auto-resume state and the battery save.
            GameKind::PocketCore { .. } => {
                control::pocket_core("quit");
            }
            GameKind::Standalone => {
                crate::standalone::save_resume_state();
                control::terminate_game(g, false);
            }
            GameKind::RetroArch => {
                control::quit_retroarch();
            }
            GameKind::Other => {
                control::terminate_game(g, false);
            }
        }
        let end = Instant::now() + Duration::from_secs(25);
        // SAFETY: kill(2) with signal 0 only checks that the process group exists.
        while Instant::now() < end && unsafe { libc::kill(-g, 0) } == 0 {
            std::thread::sleep(Duration::from_millis(200));
        }
    }
}

/// Written before deep sleep and power off: the game the next start of pocket-ui opens again, in case
/// the battery runs out while asleep or the device was switched off from the quick settings.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
pub struct Resume {
    pub system: String,
    pub name: String,
    pub path: PathBuf,
}

fn write_resume(data_dir: &Path, game: &Game) {
    let r = Resume {
        system: game.system.clone(),
        name: game.name.clone(),
        path: game.path.clone(),
    };
    if let Ok(json) = serde_json::to_vec_pretty(&r) {
        let _ = crate::fsutil::write_atomic(&data_dir.join(RESUME_FILE), &json);
    }
}

fn clear_resume(data_dir: &Path) {
    let _ = std::fs::remove_file(data_dir.join(RESUME_FILE));
}

/// The game to open right away at start, if pocket-ui was last ended by power off or by an empty battery
/// while asleep. Read once: the note is removed.
pub fn take_resume(data_dir: &Path) -> Option<Resume> {
    let path = data_dir.join(RESUME_FILE);
    let r: Resume = serde_json::from_slice(&std::fs::read(&path).ok()?).ok()?;
    let _ = std::fs::remove_file(&path);
    r.path.exists().then_some(r)
}

/// Where sleep stands.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum State {
    Awake,
    /// Screen off, game paused.
    Light,
    /// Saving, suspended, or coming back (a worker thread is on it).
    Deep,
    PoweringOff,
}

/// What the app is doing, as far as sleep cares.
#[derive(Clone, Copy, Default)]
pub struct Ctx {
    /// The pause menu is open: the game is paused already and stays so after waking.
    pub menu_open: bool,
    /// Something must not be interrupted (a game quitting, an update installing): no idle sleep.
    pub busy: bool,
    /// The power menu is open: a power press closes it instead of starting a press of its own.
    pub power_menu: bool,
}

/// Results from the worker threads.
enum Msg {
    /// The system is back from the suspend of deep sleep number `seq` (`at_us`: when the worker saw it, trace clock);
    /// ROCKNIX's wake hooks may still be running. Screen and game come on now.
    Resumed { seq: u64, at_us: u64 },
    /// Deep sleep number `seq` is over (`slept`: the system really suspended; then the wake hooks are done too and
    /// the volume is set, at `hooks_us` and `volume_us`).
    Woke { seq: u64, slept: bool, hooks_us: Option<u64>, volume_us: Option<u64> },
    /// `systemctl poweroff` failed; the message was shown, the device stays on.
    PowerOffFailed,
    /// A suspend pocket-ui didn't start has ended, and its repairs are done.
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    ExternalWake,
    /// The battery crossed the warning or the critical level (`low_battery`).
    Battery(crate::low_battery::Alert),
    /// Deep sleep number `seq` lasted the `hours` chosen, on battery (`auto_off`): save, quit and power off, dark.
    SwitchOff { seq: u64, hours: u32 },
    /// The power-off after sleeping failed: the Pocket stays dark in light sleep.
    SwitchOffFailed,
    /// Restart pocket-ui: the game is saved and quit, the resume note written; the app may end now.
    Restart,
}

/// Why the session ends (`Sleep::shut_down_for`).
#[derive(Clone, Copy, Debug, PartialEq)]
enum Why {
    /// The power menu or the quick settings.
    Asked,
    /// The low-battery guard: the screen says so.
    BatteryEmpty,
    /// Switch off after sleeping (`auto_off`) after this many hours: the screen stays dark, and the next start says why.
    Slept(u32),
}

// Fake power backends never flush every filesystem of the shared build host. The real backend still
// flushes immediately before suspend/shutdown; mock-only headless journeys get the same isolation as unit tests.
fn sync_filesystems() {
    #[cfg(not(test))]
    if std::env::var_os("POCKETUI_MOCK").is_none() {
        unsafe { libc::sync() };
    }
}

/// The backlight, the suspend and power-off commands, and the panel; paths and commands can be replaced
/// for tests (see the top of this file).
#[derive(Clone, Debug)]
struct System {
    backlight: Option<PathBuf>,
    suspend_cmd: String,
    poweroff_cmd: String,
    reboot_cmd: String,
    bootloader_cmd: String,
    dpms: bool,
    /// Bumped by every screen-off: a screen-on thread that finds another number leaves the screen dark (it went to
    /// sleep again before the thread got to it).
    screen_gen: Arc<AtomicU64>,
    /// One backlight change at a time (the screen-on thread and a screen-off right after it).
    screen_lock: Arc<Mutex<()>>,
    /// When the last screen-on was done (`trace::now_us`), for the wake's log line.
    screen_on_us: Arc<AtomicU64>,
}

impl System {
    fn new(backlight: Option<PathBuf>, suspend_cmd: String, dpms: bool) -> System {
        System {
            backlight,
            suspend_cmd,
            poweroff_cmd: default_command(PowerAction::PowerOff),
            reboot_cmd: default_command(PowerAction::Reboot),
            bootloader_cmd: default_command(PowerAction::RebootBootloader),
            dpms,
            screen_gen: Arc::new(AtomicU64::new(0)),
            screen_lock: Arc::new(Mutex::new(())),
            screen_on_us: Arc::new(AtomicU64::new(0)),
        }
    }

    fn from_env() -> System {
        let backlight = std::env::var("POCKETUI_BACKLIGHT").ok().map(PathBuf::from).or_else(|| {
            std::fs::read_dir("/sys/class/backlight")
                .ok()?
                .flatten()
                .map(|e| e.path())
                .find(|p| p.join("brightness").exists())
        });
        let mut sys = System::new(
            backlight,
            std::env::var("POCKETUI_SUSPEND_CMD").unwrap_or_else(|_| default_command(PowerAction::Sleep)),
            crate::device::current().screen_off_only() || std::env::var_os("POCKETUI_SLEEP_DPMS").is_some(),
        );
        if let Ok(c) = std::env::var("POCKETUI_POWEROFF_CMD") {
            sys.poweroff_cmd = c;
        }
        if let Ok(c) = std::env::var("POCKETUI_REBOOT_CMD") {
            sys.reboot_cmd = c;
        }
        if let Ok(c) = std::env::var("POCKETUI_BOOTLOADER_CMD") {
            sys.bootloader_cmd = c;
        }
        sys
    }

    /// The command that ends the session for `action` (not Sleep, which is `suspend_cmd`).
    fn shutdown_cmd(&self, action: PowerAction) -> &str {
        match action {
            PowerAction::Reboot => &self.reboot_cmd,
            PowerAction::RebootBootloader => &self.bootloader_cmd,
            // Never asked for (`shut_down_for` ends the app instead); the harmless one if it were.
            PowerAction::RestartUi => "true",
            PowerAction::PowerOff | PowerAction::Sleep => &self.poweroff_cmd,
        }
    }

    /// Dark screen; returns the brightness to put back (0 when it was dark already, or a screen-on not yet done).
    fn screen_off(&self) -> Option<u32> {
        // A screen-on thread that hasn't written yet now leaves the screen dark.
        self.screen_gen.fetch_add(1, Ordering::SeqCst);
        let _turn = self.screen_lock.lock().unwrap_or_else(|e| e.into_inner());
        let saved = self.backlight.as_ref().and_then(|d| {
            let file = d.join("brightness");
            let saved = std::fs::read_to_string(&file).ok()?.trim().parse::<u32>().ok()?;
            if saved > 0 {
                let _ = std::fs::write(DARK_FILE, saved.to_string());
            }
            let _ = std::fs::write(&file, "0");
            Some(saved)
        });
        if self.dpms {
            // sway's IPC socket, not a `swaymsg` process (`control::sway_command`).
            crate::control::sway_command("output * power off");
        }
        saved
    }

    /// The screen on again, on a thread of its own: the backlight is a DSI command on the Mini V2 (and with
    /// `POCKETUI_SLEEP_DPMS` sway switches the panel on first, a modeset), and neither should hold the UI thread or
    /// the game's wake behind it. A screen-off after this call wins (`screen_gen`). Returns the thread (tests wait).
    fn screen_on_soon(&self, saved: Option<u32>) -> Option<std::thread::JoinHandle<()>> {
        let gen = self.screen_gen.load(Ordering::SeqCst);
        let sys = self.clone();
        let job = move || {
            let t0 = Instant::now();
            let _turn = sys.screen_lock.lock().unwrap_or_else(|e| e.into_inner());
            if sys.screen_gen.load(Ordering::SeqCst) != gen {
                println!("sleep: dark again before the screen came on; it stays off");
                return;
            }
            sys.screen_on_now(saved);
            sys.screen_on_us.store(crate::trace::now_us(), Ordering::SeqCst);
            crate::trace::step("sleep: screen on");
            println!("sleep: screen on in {:.1} ms", t0.elapsed().as_secs_f64() * 1000.0);
        };
        match std::thread::Builder::new().name("screen-on".into()).spawn(job) {
            Ok(h) => Some(h),
            Err(_) => {
                self.screen_on_now(saved);
                None
            }
        }
    }

    /// The screen on, on the caller's thread.
    fn screen_on_now(&self, saved: Option<u32>) {
        if self.dpms {
            crate::control::sway_command("output * power on");
        }
        let saved = saved
            .filter(|l| *l > 0)
            .or_else(|| std::fs::read_to_string(DARK_FILE).ok()?.trim().parse().ok());
        if let (Some(dir), Some(level)) = (self.backlight.as_ref(), saved) {
            let _ = std::fs::write(dir.join("brightness"), level.to_string());
        }
        let _ = std::fs::remove_file(DARK_FILE);
    }

    /// Whether the backlight is at 0 (the screen is dark), as far as it can be read.
    fn screen_dark(&self) -> bool {
        self.backlight
            .as_ref()
            .and_then(|d| std::fs::read_to_string(d.join("brightness")).ok())
            .is_some_and(|b| b.trim() == "0")
    }

    /// At start: a screen left dark by a pocket-ui that ended asleep comes back on.
    fn recover(&self) {
        if self.screen_dark() && Path::new(DARK_FILE).exists() {
            println!("sleep: the screen was left dark, turning it on");
            self.screen_on_now(None);
        }
    }

    fn run(cmd: &str) -> bool {
        Command::new("sh").arg("-c").arg(cmd).status().is_ok_and(|s| s.success())
    }
}

/// Seconds the system has spent suspended since boot (CLOCK_BOOTTIME runs on in suspend, MONOTONIC doesn't).
pub fn suspended_secs() -> f64 {
    #[cfg(target_os = "linux")]
    {
        let read = |clock| {
            let mut ts: libc::timespec = unsafe { std::mem::zeroed() };
            // SAFETY: clock_gettime only writes into the provided struct.
            unsafe { libc::clock_gettime(clock, &mut ts) };
            ts.tv_sec as f64 + ts.tv_nsec as f64 / 1e9
        };
        read(libc::CLOCK_BOOTTIME) - read(libc::CLOCK_MONOTONIC)
    }
    #[cfg(not(target_os = "linux"))]
    0.0
}

/// Sleeps `d` on CLOCK_BOOTTIME, which runs on while the system is suspended: a nap a suspend fell into ends the moment
/// the system is back, where `std::thread::sleep` (CLOCK_MONOTONIC, stopped in suspend) would sleep out the rest of it
/// after the wake (up to 200 ms before the screen came on, up to 2 s before the watcher saw a suspend of logind's).
pub fn boottime_nap(d: Duration) {
    #[cfg(target_os = "linux")]
    {
        let ts = libc::timespec { tv_sec: d.as_secs() as libc::time_t, tv_nsec: d.subsec_nanos() as libc::c_long };
        // SAFETY: clock_nanosleep only reads `ts`; no remainder is asked for (an interrupted nap just ends early).
        unsafe { libc::clock_nanosleep(libc::CLOCK_BOOTTIME, 0, &ts, std::ptr::null_mut()) };
    }
    #[cfg(not(target_os = "linux"))]
    std::thread::sleep(d);
}

/// What tells whether the system has slept since a suspend was asked for: the kernel's counters of suspends that
/// worked and that failed (`/sys/power/suspend_stats`; None where there are none, as in the virtual device) and the
/// seconds spent suspended since boot.
#[derive(Clone, Copy, Debug, PartialEq)]
struct SuspendProbe {
    stats: Option<(u64, u64)>,
    asleep: f64,
}

impl SuspendProbe {
    fn now() -> SuspendProbe {
        let read = |name: &str| {
            std::fs::read_to_string(Path::new("/sys/power/suspend_stats").join(name))
                .ok()?
                .trim()
                .parse::<u64>()
                .ok()
        };
        SuspendProbe {
            stats: read("success").zip(read("fail")),
            asleep: suspended_secs(),
        }
    }
}

/// Some(true): the system slept and is back; Some(false): the kernel says the suspend failed; None: can't tell yet.
/// `blocked`: how long the suspend command ran (a test command that blocks stands for a suspend). A suspend shorter
/// than a second (power pressed right after the device went down) is only seen through the kernel's counter; before
/// 2026-10-24 it wasn't seen at all, and the screen stayed dark until the 30 s timeout.
fn suspend_outcome(before: SuspendProbe, now: SuspendProbe, blocked: Duration) -> Option<bool> {
    if blocked > BLOCKING_SUSPEND {
        return Some(true);
    }
    if let (Some((ok0, fail0)), Some((ok1, fail1))) = (before.stats, now.stats) {
        if ok1 > ok0 {
            return Some(true);
        }
        if fail1 > fail0 {
            return Some(false);
        }
    }
    (now.asleep - before.asleep > 1.0).then_some(true)
}

/// When the kernel saw the power key go down last (`trace::now_us`, 0: never), from the event's own time stamp; set by
/// the pad thread (`note_power_key`). A wake by the key: how long the kernel's resume took before pocket-ui noticed.
static POWER_KEY_US: AtomicU64 = AtomicU64::new(0);
/// Power key presses the pad thread has read: the alarm of `auto_off` asks whether one came during the suspend.
static POWER_KEY_PRESSES: AtomicU64 = AtomicU64::new(0);

/// The pad thread read a power key press that the kernel stamped `age` ago.
pub fn note_power_key(age: Duration) {
    let at = crate::trace::now_us().saturating_sub(age.as_micros() as u64);
    POWER_KEY_US.store(at, Ordering::SeqCst);
    POWER_KEY_PRESSES.fetch_add(1, Ordering::SeqCst);
    crate::mark!("power-key", "{at}");
}

/// The wake's log line: each step in ms after the worker saw the system back (`resumed`). The power key counts only if
/// the kernel stamped it within the 30 s before (an older press is not what woke it).
fn wake_timeline(resumed: u64, key: u64, screen: u64, game: Option<u64>, hooks: Option<u64>, volume: Option<u64>) -> String {
    let ms = |t: u64| (t as f64 - resumed as f64) / 1000.0;
    let mut parts = Vec::new();
    if key > 0 && key <= resumed + 5_000_000 && key + 30_000_000 >= resumed {
        parts.push(format!("power key seen by the kernel {:+.0} ms", ms(key)));
    }
    if screen >= resumed {
        parts.push(format!("screen on {:+.0} ms", ms(screen)));
    }
    if let Some(t) = game {
        parts.push(format!("game on {:+.0} ms", ms(t)));
    }
    if let Some(t) = hooks {
        parts.push(format!("ROCKNIX's wake hooks done {:+.0} ms", ms(t)));
    }
    if let Some(t) = volume {
        parts.push(format!("volume set {:+.0} ms", ms(t)));
    }
    if parts.is_empty() {
        parts.push("nothing recorded".into());
    }
    format!("sleep: wake timeline, ms after the system was back: {}", parts.join(", "))
}

/// The default sink's volume in percent and whether it is muted, from `pactl`.
pub fn sink_volume() -> Option<(u32, bool)> {
    let vol = crate::proc::run_with_timeout(Command::new("pactl").args(["get-sink-volume", "@DEFAULT_SINK@"]), crate::proc::HELPER).ok()?;
    let mute = crate::proc::run_with_timeout(Command::new("pactl").args(["get-sink-mute", "@DEFAULT_SINK@"]), crate::proc::HELPER).ok()?;
    Some((
        parse_volume(&String::from_utf8_lossy(&vol.stdout))?,
        String::from_utf8_lossy(&mute.stdout).contains("yes"),
    ))
}

/// First percentage in `pactl get-sink-volume` output ("Volume: front-left: 39321 /  60% / …").
fn parse_volume(out: &str) -> Option<u32> {
    let at = out.find('%')?;
    out[..at].rsplit(|c: char| !c.is_ascii_digit()).next()?.parse().ok()
}

/// Sets the sink volume the way ROCKNIX's Vol keys do (`/usr/bin/volume`, which also stores
/// `audio.volume`), plus the mute state. `kick`: step it one point off first, so PipeWire writes the
/// codec again even when its own idea of the volume didn't change (after a wake).
pub fn set_volume(percent: u32, muted: bool, kick: bool) {
    let pactl = |args: &[&str]| crate::proc::status_with_timeout(Command::new("pactl").args(args), crate::proc::HELPER).is_ok_and(|s| s.success());
    if kick {
        let off = if percent > 0 { percent - 1 } else { 1 };
        pactl(&["set-sink-volume", "@DEFAULT_SINK@", &format!("{off}%")]);
    }
    let rocknix = Path::new("/usr/bin/volume");
    if rocknix.exists() {
        let _ = crate::proc::status_with_timeout(Command::new(rocknix).arg(percent.to_string()), crate::proc::HELPER);
    } else {
        pactl(&["set-sink-volume", "@DEFAULT_SINK@", &format!("{percent}%")]);
    }
    pactl(&["set-sink-mute", "@DEFAULT_SINK@", if muted { "1" } else { "0" }]);
}

/// Mutes the default sink; Some(whether it was muted before), None when `pactl` couldn't say.
fn mute_sink() -> Option<bool> {
    let out = crate::proc::run_with_timeout(Command::new("pactl").args(["get-sink-mute", "@DEFAULT_SINK@"]), crate::proc::HELPER).ok()?;
    let was = String::from_utf8_lossy(&out.stdout).contains("yes");
    set_mute(true);
    Some(was)
}

fn set_mute(muted: bool) {
    let _ = crate::proc::status_with_timeout(
        Command::new("pactl").args(["set-sink-mute", "@DEFAULT_SINK@", if muted { "1" } else { "0" }]),
        crate::proc::HELPER,
    );
}

/// Waits until ROCKNIX's sleep hooks (run by `systemd-sleep` after the kernel resumed, while our
/// processes already run again) are done, so what we set afterwards isn't overwritten by them.
fn wait_for_wake_hooks() {
    let end = Instant::now() + Duration::from_secs(15);
    while Instant::now() < end && process_running("systemd-sleep") {
        std::thread::sleep(HOOKS_POLL);
    }
}

fn process_running(name: &str) -> bool {
    std::fs::read_dir("/proc").into_iter().flatten().flatten().any(|e| {
        e.file_name().to_str().is_some_and(|n| n.bytes().all(|b| b.is_ascii_digit()))
            && std::fs::read_to_string(e.path().join("comm")).is_ok_and(|c| c.trim() == name)
    })
}

/// What the inhibitor holds: the power key, and on a device with a lid (the Flip 2) the lid switch too, so that logind
/// doesn't suspend on its own when the lid closes (`Sleep::on_lid` does what the power key would).
fn inhibit_what(lid: bool) -> &'static str {
    if lid {
        "--what=handle-power-key:handle-lid-switch"
    } else {
        "--what=handle-power-key"
    }
}

/// A logind `handle-power-key` inhibitor: while it is held, logind leaves the power key to us.
struct Inhibitor {
    child: Child,
}

impl Inhibitor {
    fn take() -> Option<Inhibitor> {
        let device = crate::device::current();
        println!("{}", device.log_line());
        let child = Command::new("systemd-inhibit")
            .args([
                inhibit_what(device.has_lid()),
                "--who=pocket-ui",
                "--why=Saves the game before sleeping",
                "--mode=block",
                "cat",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;
        Some(Inhibitor { child })
    }

    /// systemd-inhibit exits at once when it can't take the lock (no logind, as on the virtual device).
    fn held(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }
}

impl Drop for Inhibitor {
    fn drop(&mut self) {
        // Closing the pipe ends `cat`, and with it the lock.
        drop(self.child.stdin.take());
        let _ = self.child.wait();
    }
}

/// The inhibitor, taken on a thread at start: starting `systemd-inhibit` waits until its program is read, which with
/// a cold page cache (every power-on) held pocket-ui's first frame for 5-30 ms on the Mini V2. The power key is
/// logind's until then, as it is when the inhibitor can't be had.
enum InhibitorSlot {
    Taking(std::thread::JoinHandle<Option<Inhibitor>>),
    Have(Option<Inhibitor>),
}

impl InhibitorSlot {
    fn take_soon() -> InhibitorSlot {
        if !cfg!(target_os = "linux") {
            return InhibitorSlot::Have(None);
        }
        match std::thread::Builder::new().name("inhibit".into()).spawn(Inhibitor::take) {
            Ok(h) => InhibitorSlot::Taking(h),
            Err(_) => InhibitorSlot::Have(Inhibitor::take()),
        }
    }

    /// The inhibitor, once its thread is done (waited for if not yet: a press of the power key in the first moments).
    fn get(&mut self) -> Option<&mut Inhibitor> {
        if let InhibitorSlot::Taking(_) = self {
            let InhibitorSlot::Taking(h) = std::mem::replace(self, InhibitorSlot::Have(None)) else { unreachable!() };
            *self = InhibitorSlot::Have(h.join().ok().flatten());
        }
        match self {
            InhibitorSlot::Have(i) => i.as_mut(),
            InhibitorSlot::Taking(_) => None,
        }
    }
}

/// The idle timers, apart from everything with side effects (unit-tested).
#[derive(Clone, Debug)]
struct Timers {
    light_after: Option<Duration>,
    /// Sleep after: None never suspends by itself (the screen stays dark until a button).
    deep_after: Option<Duration>,
    last_input: Instant,
    light_since: Option<Instant>,
    /// Suspends that didn't happen since the screen went dark: each one doubles the wait before the next try.
    failures: u32,
}

#[derive(Debug, PartialEq)]
enum Due {
    Light,
    Deep,
}

impl Timers {
    /// How long light sleep lasts before the (next) suspend: `deep_after`, doubled for each failed one, at most
    /// `RETRY_AT_MOST`. It used to give up after two failures and stay dark for good (review B12 / SOL-05). None:
    /// Sleep after is Never; a sleep asked for (the power button) that failed is tried again all the same, from
    /// `RETRY_FIRST`.
    fn deep_wait(&self) -> Option<Duration> {
        let after = match (self.deep_after, self.failures) {
            (Some(after), _) => after,
            (None, 0) => return None,
            (None, _) => RETRY_FIRST,
        };
        let doubled = after.saturating_mul(1u32 << self.failures.min(16));
        Some(doubled.min(RETRY_AT_MOST.max(after)))
    }

    /// What is due now, and when to look again.
    fn due(&self, state: State, now: Instant, auto_suspend: bool) -> (Option<Due>, Option<Instant>) {
        match (state, self.light_since) {
            (State::Awake, _) => match self.light_after {
                Some(after) if now >= self.last_input + after => (Some(Due::Light), None),
                Some(after) => (None, Some(self.last_input + after)),
                None => (None, None),
            },
            (State::Light, Some(since)) if auto_suspend => {
                let Some(wait) = self.deep_wait() else { return (None, None) };
                if now >= since + wait {
                    (Some(Due::Deep), None)
                } else {
                    (None, Some(since + wait))
                }
            }
            _ => (None, None),
        }
    }
}

/// Whether a pad event counts as someone using the device.
/// A button press (not power or volume) within `WAKE_QUIET` of waking: swallowed.
fn quiet_after_wake(woke_at: Option<Instant>, now: Instant, e: &PadEvent) -> bool {
    let recent = woke_at.is_some_and(|t| now < t + WAKE_QUIET);
    recent
        && matches!(
            e,
            PadEvent::Button { pressed: true, button, .. } if !matches!(button, Button::Power | Button::VolUp | Button::VolDown)
        )
}

/// What woke the screen, for the log ("A", "a stick").
fn wake_reason(e: &PadEvent) -> String {
    match e {
        PadEvent::Button { button, .. } => format!("{button:?}"),
        _ => "a stick".into(),
    }
}

fn is_input(e: &PadEvent) -> bool {
    match e {
        PadEvent::Button { pressed, .. } => *pressed,
        PadEvent::Axis {
            axis: Axis::L2 | Axis::R2,
            value,
            ..
        } => *value > 0.5,
        PadEvent::Axis { value, .. } => value.abs() > STICK_DEADZONE,
        // A controller connecting (it may do so by itself after a sleep) is nobody pressing anything. The lid has its
        // own rules (`Sleep::on_lid`).
        PadEvent::Raw { .. } | PadEvent::Connected { .. } | PadEvent::Disconnected { .. } | PadEvent::Lid { .. } => false,
    }
}

/// The steps of a deep sleep, as its worker thread runs them (`deep_sequence`); the tests run them on a fake and
/// check the order.
trait DeepSteps {
    /// The game writes the state the next start resumes from; the resume note.
    fn save(&mut self);
    /// The sink's volume and mute now.
    fn volume(&mut self) -> Option<(u32, bool)>;
    /// `set_volume`; `kick`: one step off and back, so PipeWire writes the codec again.
    fn set_volume(&mut self, volume: (u32, bool), kick: bool);
    /// Asks for the suspend; true once the system has slept and is back.
    fn suspend(&mut self) -> bool;
    /// The system is back: the UI turns the screen on and the game continues, now.
    fn resumed(&mut self);
    /// Waits until ROCKNIX's wake hooks are done.
    fn wait_for_hooks(&mut self);
    /// The end: the UI hears whether it slept.
    fn done(&mut self, slept: bool);
    /// Seconds on CLOCK_BOOTTIME (`auto_off::clock`).
    fn clock(&mut self) -> f64;
    /// The RTC alarm this many seconds from now, or none (`auto_off::Rtc`).
    fn alarm(&mut self, secs: Option<u64>);
    /// Power key presses read so far.
    fn power_presses(&mut self) -> u64;
    /// The alarm's wake, the screen still dark: ROCKNIX's wake hooks done, and a moment for a key pressed then to arrive.
    fn settle(&mut self);
    /// The battery now.
    fn power(&mut self) -> Option<crate::low_battery::Level>;
    /// `auto_off::wall_offset`: the wall clock against CLOCK_BOOTTIME; it moves when the clock is set or stepped.
    fn wall_offset(&mut self) -> f64;
    /// A power off or restart has begun: no new suspend, no alarm (`Rtc::close`).
    fn shutting_down(&mut self) -> bool;
    /// Waits (the screen stays dark): between the readings that confirm a switch-off.
    fn pause(&mut self, d: Duration);
    /// The time is up on battery: the UI saves, quits the game and powers off, the screen dark. The end of this sleep.
    fn switch_off(&mut self, plan: crate::auto_off::Plan);
}

/// Readings that must all say battery, after the first one, before a switch-off after sleeping (`deep_sequence`).
const SWITCH_OFF_CONFIRMS: u32 = 2;
const SWITCH_OFF_GAP: Duration = Duration::from_secs(2);

/// Deep sleep, in order. What a player sees and hears first comes first after the wake: screen and game do not wait
/// for ROCKNIX's wake hooks (they took seconds of the wake before 2026-10-24), the volume is set right after them
/// (the codec may come back silent, docs/SLEEP.md) and once more when the hooks are done, in case one of them changed
/// it again.
///
/// `plan`: switch off after sleeping (`auto_off`). The RTC alarm goes off when its time is up; a wake before that costs
/// one clock read on its way to the screen, and the alarm is cleared only after the screen and the game are on.
fn deep_sequence(s: &mut impl DeepSteps, mut plan: Option<crate::auto_off::Plan>) {
    use crate::auto_off::Verdict;
    s.save();
    // The volume PipeWire has now, also as ROCKNIX's `audio.volume`, which its wake hook reads.
    let volume = s.volume();
    if let Some(v) = volume {
        s.set_volume(v, false);
    }
    let slept = loop {
        // A power off began while this sequence went round again (asleep again on the charger, a clock set): it ends
        // here, without arming or suspending. No `done`: the state is the power off's now.
        if s.shutting_down() {
            println!("sleep: a power off is under way; this deep sleep ends without a suspend");
            return;
        }
        if let Some(p) = plan {
            let now = s.clock();
            s.alarm(Some(p.secs_left(now)));
        }
        let presses = s.power_presses();
        let offset = s.wall_offset();
        let slept = s.suspend();
        let Some(p) = plan.filter(|_| slept) else { break slept };
        if !p.due(s.clock()) {
            // The wall clock was set or stepped (by hand, NTP) since the alarm was armed: the RTC alarm, a time on that
            // clock, may have gone off early. That is no wake of a player and no new start of the count: a look
            // for a key press, and asleep again for what is left (the plan is unchanged), the screen never on.
            if (s.wall_offset() - offset).abs() > crate::auto_off::CLOCK_STEP_SECS {
                s.settle();
                if s.power_presses() == presses {
                    println!("sleep: the clock was set during the suspend; not a wake, asleep again for what is left");
                    continue;
                }
            }
            break true;
        }
        // The alarm (or a wake right at its time): nothing comes on until it is clear that someone is there.
        s.settle();
        let mut verdict = crate::auto_off::decide(s.power_presses() != presses, s.power());
        // A switch-off cannot be taken back: the reading that says battery is made twice more, a moment apart. The
        // wake hooks can outlast their cap, and a charger's state arrives seconds after the wake.
        for _ in 0..SWITCH_OFF_CONFIRMS {
            if verdict != Verdict::SwitchOff {
                break;
            }
            s.pause(SWITCH_OFF_GAP);
            verdict = crate::auto_off::decide(s.power_presses() != presses, s.power());
        }
        match verdict {
            Verdict::Wake => {
                println!("sleep: the power key at the switch-off time; awake as always");
                break true;
            }
            Verdict::SleepOn => {
                println!("sleep: switch-off time, but on the charger (or the battery unread); asleep again");
                let now = s.clock();
                plan = Some(p.again(now));
            }
            Verdict::SwitchOff => {
                s.alarm(None);
                s.switch_off(p);
                return;
            }
        }
    };
    if slept {
        s.resumed();
    }
    // After the screen and the game: an alarm left set must not switch off a Pocket that is awake or dark in light sleep.
    if plan.is_some() {
        s.alarm(None);
    }
    if slept {
        if let Some(v) = volume {
            s.set_volume(v, true);
        }
        s.wait_for_hooks();
        if let Some(v) = volume {
            s.set_volume(v, true);
        }
    }
    s.done(slept);
}

/// The switch-off plan for a deep sleep that starts `now`: the dark spell's own when it has one (kept across failed
/// suspends), a full period on when its time passed while nothing could act, a new one at the spell's first suspend.
fn plan_for_deep(hours: Option<u32>, before: Option<crate::auto_off::Plan>, now: f64, hour_secs: f64) -> Option<crate::auto_off::Plan> {
    match (hours, before) {
        (None, _) => None,
        (Some(h), Some(plan)) if plan.hours == h && plan.at > now => Some(plan),
        (Some(h), Some(plan)) if plan.hours == h => Some(plan.again(now)),
        (Some(h), _) => Some(crate::auto_off::Plan::new(now, h, hour_secs)),
    }
}

/// The real steps, on the deep sleep's worker thread.
struct DeepWorker {
    seq: u64,
    game: Option<Game>,
    /// The game was stopped by us or by the open pause menu: stopped again after saving.
    stopped: bool,
    sys: System,
    data_dir: PathBuf,
    tx: Sender<Msg>,
    notify: Arc<dyn Fn() + Send + Sync>,
    own: Arc<AtomicBool>,
    t0: Instant,
    hooks_us: Option<u64>,
    volume_us: Option<u64>,
    rtc: crate::auto_off::Rtc,
}

impl DeepSteps for DeepWorker {
    fn save(&mut self) {
        if let Some(game) = &self.game {
            let saved = game.save_for_sleep(self.stopped);
            println!(
                "sleep: {} for {} after {:?}",
                if saved { "state saved" } else { "no state saved" },
                game.name,
                self.t0.elapsed()
            );
            write_resume(&self.data_dir, game);
        }
    }

    fn volume(&mut self) -> Option<(u32, bool)> {
        sink_volume()
    }

    fn set_volume(&mut self, (v, muted): (u32, bool), kick: bool) {
        set_volume(v, muted, kick);
        if kick {
            // The first one after the wake is when the sound can be heard again (the log line).
            self.volume_us.get_or_insert_with(crate::trace::now_us);
            crate::trace::step("sleep: volume set");
        }
    }

    fn suspend(&mut self) -> bool {
        sync_filesystems();
        let before = SuspendProbe::now();
        println!("sleep: suspending ({}) after {:?}", self.sys.suspend_cmd, self.t0.elapsed());
        crate::trace::step("sleep: suspending");
        let asked = Instant::now();
        let ran = System::run(&self.sys.suspend_cmd);
        // `systemctl suspend` returns before the system sleeps; a test command blocks instead.
        let mut outcome = suspend_outcome(before, SuspendProbe::now(), asked.elapsed());
        while ran && outcome.is_none() && asked.elapsed() < SUSPEND_TIMEOUT {
            boottime_nap(RESUME_POLL);
            outcome = suspend_outcome(before, SuspendProbe::now(), asked.elapsed());
        }
        let slept = ran && outcome == Some(true);
        if slept {
            println!(
                "sleep: woke after {:.0} s asleep; screen and game on now, the volume after ROCKNIX's wake hooks",
                (suspended_secs() - before.asleep).max(0.0)
            );
        } else {
            eprintln!("sleep: the system didn't suspend ({}, ran {ran}, {outcome:?})", self.sys.suspend_cmd);
        }
        slept
    }

    fn resumed(&mut self) {
        let at_us = crate::trace::now_us();
        crate::trace::step("sleep: resumed");
        crate::wifi::resumed();
        let _ = self.tx.send(Msg::Resumed { seq: self.seq, at_us });
        (self.notify)();
        // Off the UI thread (a file on the card): awake, nothing to resume from at the next start.
        clear_resume(&self.data_dir);
    }

    fn wait_for_hooks(&mut self) {
        wait_for_wake_hooks();
        self.hooks_us = Some(crate::trace::now_us());
        crate::trace::step("sleep: wake hooks done");
    }

    fn done(&mut self, slept: bool) {
        self.own.store(false, Ordering::SeqCst);
        let _ = self.tx.send(Msg::Woke {
            seq: self.seq,
            slept,
            hooks_us: self.hooks_us,
            volume_us: self.volume_us,
        });
        (self.notify)();
    }

    fn clock(&mut self) -> f64 {
        crate::auto_off::clock()
    }

    fn alarm(&mut self, secs: Option<u64>) {
        // Once per run: the virtual device has no RTC.
        static SAID: AtomicBool = AtomicBool::new(false);
        if !self.rtc.set(secs) {
            if secs.is_some() && self.rtc.shut() {
                println!("sleep: a power off is under way; no RTC alarm set");
            } else if secs.is_none() && self.rtc.present() {
                // A clear that fails on a device with an RTC leaves an alarm to go off later: said every time.
                eprintln!("sleep: couldn't clear the RTC alarm; it may go off later");
            } else if !SAID.swap(true, Ordering::SeqCst) {
                eprintln!("sleep: couldn't set the RTC alarm ({secs:?}); switching off after sleeping needs it");
            }
        } else if let Some(s) = secs {
            println!("sleep: RTC alarm in {s} s (switch off after sleeping)");
        }
    }

    fn power_presses(&mut self) -> u64 {
        POWER_KEY_PRESSES.load(Ordering::SeqCst)
    }

    fn settle(&mut self) {
        println!("sleep: the switch-off time is up; the screen stays dark while it decides");
        wait_for_wake_hooks();
        // The pad thread reads a key pressed at the wake within milliseconds of the thaw; this is ample.
        std::thread::sleep(Duration::from_millis(300));
    }

    fn power(&mut self) -> Option<crate::low_battery::Level> {
        crate::low_battery::read(&crate::battery::dir())
    }

    fn wall_offset(&mut self) -> f64 {
        crate::auto_off::wall_offset()
    }

    fn shutting_down(&mut self) -> bool {
        self.rtc.shut()
    }

    fn pause(&mut self, d: Duration) {
        std::thread::sleep(d);
    }

    fn switch_off(&mut self, plan: crate::auto_off::Plan) {
        self.own.store(false, Ordering::SeqCst);
        let _ = self.tx.send(Msg::SwitchOff { seq: self.seq, hours: plan.hours });
        (self.notify)();
    }
}

pub struct Sleep {
    pub config: Config,
    screen_only: bool,
    data_dir: PathBuf,
    sys: System,
    state: State,
    timers: Timers,
    game: Option<Game>,
    /// We stopped the game's processes (not the pause menu).
    stopped_game: bool,
    /// The pause menu was open when sleep began.
    menu_open: bool,
    brightness: Option<u32>,
    woke_at: Option<Instant>,
    /// The battery reached `low_battery::WARN_AT` %: the app shows it (`take_battery_warning`).
    battery_warning: Option<u8>,
    /// The battery is empty: the game is saved and the device switched off as soon as nothing else is under way.
    battery_empty: bool,
    inhibitor: InhibitorSlot,
    force_power_key: bool,
    /// The power button: short press sleeps, long press asks for the power menu (`take_menu_request`).
    power_key: PowerKey,
    menu_requested: bool,
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
    notify: Arc<dyn Fn() + Send + Sync>,
    /// Our own suspend is under way: the clock watcher leaves it to the worker.
    own_suspend: Arc<AtomicBool>,
    /// Sink volume and mute, read every 15 s while awake (`u32::MAX`: unknown).
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    last_volume: Arc<AtomicU32>,
    overlay: Arc<std::sync::Mutex<Option<String>>>,
    /// The number of the last deep sleep: news of an earlier one (its wake hooks still running when the power key
    /// put the device to sleep again) is not this one's.
    deep_seq: u64,
    /// Held by a deep sleep's worker for its whole run: a second one (asleep again while the first one's wake hooks
    /// still run) waits for it, so two suspends never overlap.
    suspend_turn: Arc<Mutex<()>>,
    /// Awake again since the app last asked (`take_woke`): it draws a new frame (the clock and the battery).
    woke_news: bool,
    /// When the last deep sleep's system was back and the game was told to go on (`trace::now_us`), for the log line.
    resumed_us: u64,
    game_on_us: Option<u64>,
    /// The last screen-on thread (the tests wait for it).
    #[cfg_attr(not(test), allow(dead_code))]
    screen_job: Option<std::thread::JoinHandle<()>>,
    /// When this dark spell switches off (`auto_off`): set at its first suspend, gone when the Pocket wakes.
    auto_off: Option<crate::auto_off::Plan>,
    rtc: crate::auto_off::Rtc,
    /// Where the battery is read (`battery::dir`): by the light-sleep switch-off, fresh at the moment of deciding.
    battery_dir: PathBuf,
    /// What the last switch-off after sleeping left for this start (`take_switched_off`).
    switched_off: Option<crate::auto_off::Note>,
    /// Dark with the game going on (`screen_off_now`): no pause to undo, no sleep by the timers.
    dark_game: bool,
    /// The power menu's Restart pocket-ui is through (game saved and quit): the app ends with exit code 0 and the
    /// launcher starts it again (`take_restart`).
    restart: bool,
}

impl Sleep {
    /// `notify` wakes the app's event loop when a worker has news (then call `tick`).
    pub fn new(data_dir: PathBuf, notify: Arc<dyn Fn() + Send + Sync>) -> Sleep {
        let mut sleep = Sleep::build(data_dir, System::from_env(), notify, InhibitorSlot::take_soon());
        sleep.sys.recover();
        // Switched off after sleeping: the backlight was 0 then. One small file, and only that start finds one.
        sleep.switched_off = crate::auto_off::take_note(&sleep.data_dir);
        if let Some(level) = sleep.switched_off.as_ref().and_then(|n| n.brightness).filter(|_| sleep.sys.screen_dark()) {
            println!("sleep: switched off dark after sleeping; the screen on again");
            sleep.sys.screen_on_now(Some(level));
        }
        #[cfg(target_os = "linux")]
        sleep.spawn_watcher();
        sleep.spawn_battery_guard();
        sleep.spawn_rtc_check();
        sleep
    }

    /// An alarm a run before this one left armed is cleared (and the RTC's state logged) on a thread of its own: a start
    /// does not wait for the RTC (`auto_off::check_at_start`).
    fn spawn_rtc_check(&self) {
        let (rtc, data_dir) = (self.rtc.clone(), self.data_dir.clone());
        let _ = std::thread::Builder::new().name("rtc-check".into()).spawn(move || {
            crate::auto_off::check_at_start(&rtc, &data_dir);
        });
    }

    /// The state alone: no threads, no inhibitor, nothing read but the settings (`new` adds those; the tests don't).
    fn build(data_dir: PathBuf, sys: System, notify: Arc<dyn Fn() + Send + Sync>, inhibitor: InhibitorSlot) -> Sleep {
        let config = Config::load(&data_dir);
        let (tx, rx) = channel();
        Sleep {
            screen_only: crate::device::current().screen_off_only(),
            timers: Timers {
                light_after: config.screen_off.map(scaled_secs),
                deep_after: config.sleep_after.map(|m| minute() * m),
                last_input: Instant::now(),
                light_since: None,
                failures: 0,
            },
            config,
            data_dir,
            sys,
            state: State::Awake,
            game: None,
            stopped_game: false,
            menu_open: false,
            brightness: None,
            woke_at: None,
            battery_warning: None,
            battery_empty: false,
            inhibitor,
            force_power_key: std::env::var_os("POCKETUI_POWER_KEY").is_some(),
            power_key: PowerKey::default(),
            menu_requested: false,
            tx,
            rx,
            notify,
            own_suspend: Arc::new(AtomicBool::new(false)),
            last_volume: Arc::new(AtomicU32::new(u32::MAX)),
            overlay: Arc::new(std::sync::Mutex::new(None)),
            deep_seq: 0,
            suspend_turn: Arc::new(Mutex::new(())),
            woke_news: false,
            resumed_us: 0,
            game_on_us: None,
            screen_job: None,
            auto_off: None,
            rtc: crate::auto_off::Rtc::from_env(),
            battery_dir: crate::battery::dir(),
            switched_off: None,
            dark_game: false,
            restart: false,
        }
    }

    /// Reads the battery every `low_battery::EVERY` on a thread of its own and reports a crossing of the warning or
    /// the critical level (`low_battery`). Nothing without a battery (the virtual device without its mock).
    fn spawn_battery_guard(&self) {
        let (tx, notify) = (self.tx.clone(), self.notify.clone());
        // Even the first reading is the thread's: nothing of this on the UI thread at start.
        let _ = std::thread::Builder::new().name("battery-guard".into()).spawn(move || {
            let dir = crate::battery::dir();
            let mut level = crate::low_battery::read(&dir);
            if level.is_none() {
                return;
            }
            let mut guard = crate::low_battery::Guard::default();
            loop {
                if let Some(alert) = guard.observe(level) {
                    if tx.send(Msg::Battery(alert)).is_err() {
                        return;
                    }
                    notify();
                }
                std::thread::sleep(crate::low_battery::EVERY);
                level = crate::low_battery::read(&dir);
            }
        });
    }

    /// The last start was a switch-off after sleeping: what to say once ("Switched off after 3 h asleep to save the
    /// battery.").
    pub fn take_switched_off(&mut self) -> Option<String> {
        self.switched_off.take().map(|n| n.text())
    }

    /// The battery fell to the warning level: true once per crossing, with the percentage.
    pub fn take_battery_warning(&mut self) -> Option<u8> {
        self.battery_warning.take()
    }

    pub fn state(&self) -> State {
        self.state
    }

    /// Text to show full screen while powering off ("Saving…").
    pub fn overlay(&self) -> Option<String> {
        self.overlay.lock().ok()?.clone()
    }

    /// Settings → System → Sleep & power changed: kept, and the timers count with the new times from now.
    pub fn set_config(&mut self, config: Config) {
        let config = config.for_device(self.screen_only);
        self.config = config;
        self.config.save(&self.data_dir);
        self.timers.light_after = config.screen_off.map(scaled_secs);
        self.timers.deep_after = config.sleep_after.map(|m| minute() * m);
        self.timers.last_input = Instant::now();
    }

    pub fn game_started(&mut self, game: Game) {
        self.game = Some(game);
        self.timers.last_input = Instant::now();
    }

    pub fn game_ended(&mut self) {
        self.game = None;
        self.stopped_game = false;
        // A game that ended by itself behind the dark screen: from now on it is light sleep, Sleep after counting.
        if std::mem::take(&mut self.dark_game) && self.state == State::Light {
            self.timers.light_since = Some(Instant::now());
        }
    }

    /// Whether idle goes on from light sleep to a suspend. Failed suspends no longer stop it (they are retried with
    /// longer waits, `Timers::deep_wait`); the low-battery guard covers a device that stays dark anyway.
    fn auto_suspend(&self) -> bool {
        !self.screen_only && std::env::var_os("POCKETUI_NO_AUTOSUSPEND").is_none() && !self.data_dir.join("no-autosuspend").exists()
    }

    /// pocket-ui handles the power key: logind's handling is inhibited (or forced for tests).
    fn power_key_ours(&mut self) -> bool {
        self.force_power_key || self.inhibitor.get().is_some_and(Inhibitor::held)
    }

    /// Every pad event goes through here first. Returns true when it was sleep's (it woke the device,
    /// started sleep, or arrived while asleep) and must not reach the UI or the menu.
    pub fn on_pad(&mut self, e: &PadEvent, ctx: Ctx) -> bool {
        if let PadEvent::Lid { closed, .. } = *e {
            return self.on_lid(closed, ctx);
        }
        let now = Instant::now();
        let guarded = self.woke_at.is_some_and(|t| now < t + WAKE_GUARD);
        match self.state {
            State::Deep | State::PoweringOff => true,
            // The volume keys stay ROCKNIX's; they don't wake the screen.
            State::Light
                if is_input(e)
                    && !matches!(
                        e,
                        PadEvent::Button {
                            button: Button::VolUp | Button::VolDown,
                            ..
                        }
                    ) =>
            {
                self.wake_light(&wake_reason(e));
                true
            }
            // Releases and small stick drift while dark.
            State::Light => true,
            State::Awake if quiet_after_wake(self.woke_at, now, e) => {
                println!("sleep: ignored {} right after waking", wake_reason(e));
                true
            }
            State::Awake => {
                if is_input(e) {
                    self.timers.last_input = now;
                }
                let step = match e {
                    PadEvent::Button {
                        button: Button::Power,
                        pressed: true,
                        ..
                    } => {
                        let gate = Gate {
                            ours: self.power_key_ours(),
                            guarded,
                            menu_open: ctx.power_menu,
                        };
                        self.power_key.press(now, gate)
                    }
                    PadEvent::Button {
                        button: Button::Power,
                        pressed: false,
                        ..
                    } => {
                        let step = self.power_key.release(now);
                        return self.power_step(step, true, ctx);
                    }
                    _ => Step::Pass,
                };
                self.power_step(step, false, ctx)
            }
        }
    }

    /// The Flip 2's lid (docs/SLEEP.md "Lid"): closing it does what the power button's short press does (Settings ›
    /// System › Sleep & power: Sleep by default, or Screen off), opening it wakes a dark screen; from deep sleep the
    /// lid's own wake (the hall sensor is a wakeup source) brings it back the usual way. Only while pocket-ui holds
    /// logind's inhibitor, which on a device with a lid covers `handle-lid-switch` too (`Inhibitor::take`); without
    /// it logind suspends on the lid by itself (ROCKNIX's `HandleLidSwitch=suspend`) and pocket-ui stays out of its
    /// way. Never reaches the UI.
    fn on_lid(&mut self, closed: bool, ctx: Ctx) -> bool {
        if !self.power_key_ours() {
            println!("sleep: lid {}, left to logind", if closed { "closed" } else { "opened" });
            return true;
        }
        match (self.state, closed) {
            (State::Awake, true) => {
                println!("sleep: lid closed");
                self.power_key.cancel();
                self.power_step(Step::Short, true, ctx);
            }
            (State::Awake, false) => self.timers.last_input = Instant::now(),
            // Dark already (a while without input, or Screen off with the game on): Sleep goes on into deep sleep.
            (State::Light, true) if self.config.short_press == ShortPress::Sleep => {
                println!("sleep: lid closed");
                self.sleep_now(ctx);
            }
            (State::Light, true) => {}
            (State::Light, false) => self.wake_light("the lid"),
            (State::Deep | State::PoweringOff, _) => {}
        }
        true
    }

    /// Acts on what the power button just did (see `powerkey`), as Settings → System → Sleep & power says
    /// (`Config::short_press`, `Config::hold`). `released`: the step is the key's release. True when the event is
    /// consumed.
    fn power_step(&mut self, step: Step, released: bool, ctx: Ctx) -> bool {
        match (step, self.config.short_press, self.config.hold) {
            (Step::Pass, ..) => false,
            (Step::Swallow, ..) => true,
            (Step::Short, ShortPress::Sleep, _) => {
                println!("sleep: power key");
                self.sleep_now(ctx);
                true
            }
            (Step::Short, ShortPress::ScreenOff, _) => {
                println!("sleep: power key, screen off");
                self.screen_off_now(ctx);
                true
            }
            (Step::Long, _, Hold::Menu) => {
                println!("sleep: power key held, power menu");
                self.menu_requested = true;
                true
            }
            (Step::LongEnd, _, Hold::Menu) => true,
            // Held to sleep: when the key goes up, so that no release arrives while the system suspends.
            (Step::Long, _, Hold::Sleep) if !released => true,
            (Step::Long | Step::LongEnd, _, Hold::Sleep) => {
                println!("sleep: power key held");
                self.sleep_now(ctx);
                true
            }
        }
    }

    /// The power button was held (or the menu was asked for): true once, then the app opens the power menu.
    pub fn take_menu_request(&mut self) -> bool {
        std::mem::take(&mut self.menu_requested)
    }

    /// A finger on the touch screen. It counts as activity while awake, but never wakes the screen (a touch in
    /// a pocket is no reason to). True when awake.
    pub fn touched(&mut self) -> bool {
        let awake = self.state == State::Awake;
        if awake {
            self.timers.last_input = Instant::now();
        }
        awake
    }

    /// Call from the event loop's idle hook. Returns when to call again.
    pub fn tick(&mut self, ctx: Ctx) -> Option<Instant> {
        if self.state == State::Awake {
            let step = self.power_key.poll(Instant::now());
            self.power_step(step, false, ctx);
        }
        let timers = self.tick_timers(ctx);
        match (timers, self.power_key.deadline()) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        }
    }

    fn tick_timers(&mut self, ctx: Ctx) -> Option<Instant> {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                Msg::Resumed { seq, at_us } if seq == self.deep_seq && self.state == State::Deep => self.resumed(at_us),
                Msg::Woke { seq, slept, hooks_us, volume_us } if seq == self.deep_seq => self.woke(slept, hooks_us, volume_us),
                // A deep sleep that a newer one overtook (asleep again while its wake hooks still ran): nothing of it is
                // current, and a `wake` for pocket-core now would wake the game of the newer one.
                Msg::Resumed { .. } | Msg::Woke { .. } => {}
                Msg::PowerOffFailed => {
                    if let Ok(mut o) = self.overlay.lock() {
                        *o = None;
                    }
                    self.state = State::Awake;
                    crate::battery_history::set_asleep(false);
                    self.timers.last_input = Instant::now();
                }
                Msg::ExternalWake => {
                    println!("sleep: the system was suspended without us; volume and quantum set again");
                    // No game's battery stretch reaches across that suspend (`battery_history`).
                    crate::battery_history::sleep_boundary();
                    if let Some(Game {
                        kind: GameKind::PocketCore { .. },
                        ..
                    }) = &self.game
                    {
                        control::pocket_core("wake");
                    }
                    self.woke_at = Some(Instant::now());
                    self.timers.last_input = Instant::now();
                }
                Msg::Battery(crate::low_battery::Alert::Low(percent)) => {
                    println!("sleep: battery at {percent} %");
                    // Nobody sees a warning on a dark screen; the critical level still acts.
                    if self.state == State::Awake {
                        self.battery_warning = Some(percent);
                    }
                }
                Msg::Battery(crate::low_battery::Alert::Critical(percent)) => {
                    println!("sleep: battery at {percent} %, saving the game and switching off");
                    self.battery_empty = true;
                }
                Msg::SwitchOff { seq, hours } if seq == self.deep_seq && self.state == State::Deep => {
                    println!("sleep: {hours} h asleep on battery; saving the game and switching off, the screen dark");
                    self.shut_down_for(PowerAction::PowerOff, Why::Slept(hours));
                }
                Msg::SwitchOff { .. } => {}
                Msg::Restart => self.restart = true,
                Msg::SwitchOffFailed => {
                    // Dark and paused as before, without another try in this dark spell; the low-battery guard still acts.
                    eprintln!("sleep: couldn't switch off after sleeping; staying in light sleep");
                    self.state = State::Light;
                    self.timers.light_since = Some(Instant::now());
                    self.auto_off = None;
                }
            }
        }
        // Not in the middle of a suspend (it comes back to Light, then this runs) or a power-off already under way.
        if self.battery_empty && matches!(self.state, State::Awake | State::Light) {
            self.battery_empty = false;
            self.shut_down_for(PowerAction::PowerOff, Why::BatteryEmpty);
            return None;
        }
        // Dark in light sleep, suspends failing, on battery, and the time is up: switched off as from deep sleep.
        let light_off = match (self.state, self.auto_off) {
            (State::Light, Some(plan)) => {
                let now = crate::auto_off::clock();
                // Read now, not the battery guard's reading of up to 30 s ago: a charger plugged in just before the
                // deadline must keep the Pocket on (the deep verdict reads fresh too, `DeepWorker::power`).
                if plan.due(now) && crate::auto_off::decide(false, crate::low_battery::read(&self.battery_dir)) == crate::auto_off::Verdict::SwitchOff {
                    println!("sleep: {} h dark on battery (suspends failed); saving the game and switching off", plan.hours);
                    self.shut_down_for(PowerAction::PowerOff, Why::Slept(plan.hours));
                    return None;
                }
                // Looked at again then (or a minute on, on the charger, should it be unplugged).
                Some(Instant::now() + Duration::from_secs_f64((plan.at - now).max(60.0)))
            }
            _ => None,
        };
        if ctx.busy && self.state == State::Awake {
            self.timers.last_input = Instant::now();
        }
        // A game left running behind the dark screen (`screen_off_now`) is never put to sleep by the timers.
        let auto_suspend = self.auto_suspend() && !self.dark_game;
        let (due, next) = self.timers.due(self.state, Instant::now(), auto_suspend);
        match due {
            Some(Due::Light) => {
                println!("sleep: light after {:?} without input", self.timers.light_after.unwrap_or_default());
                self.light(ctx);
                if self.screen_only {
                    self.tick(ctx) // schedule the screen-off power-off deadline; there is no suspend timer
                } else {
                    self.timers.due(self.state, Instant::now(), self.auto_suspend()).1
                }
            }
            Some(Due::Deep) => {
                self.deep(ctx);
                None
            }
            None => match (next, light_off) {
                (Some(a), Some(b)) => Some(a.min(b)),
                (a, b) => a.or(b),
            },
        }
    }

    /// Awake again since the last call (a light sleep or a deep one): true once. The app draws a frame then, so the
    /// clock and the battery in the corner are those of now, not of when it went dark.
    pub fn take_woke(&mut self) -> bool {
        std::mem::take(&mut self.woke_news)
    }

    /// Screen off, game paused.
    fn light(&mut self, ctx: Ctx) {
        self.power_key.cancel();
        crate::trace::step("sleep: light");
        // 0 when a screen-on is still under way (asleep again at once): the level from before stays.
        self.brightness = self.sys.screen_off().filter(|l| *l > 0).or(self.brightness);
        self.menu_open = ctx.menu_open;
        if let Some(game) = &self.game {
            if !ctx.menu_open || matches!(game.kind, GameKind::PocketCore { .. }) {
                game.pause();
                self.stopped_game = !ctx.menu_open && !matches!(game.kind, GameKind::PocketCore { .. });
            }
        }
        self.state = State::Light;
        self.timers.light_since = Some(Instant::now());
        self.timers.failures = 0;
        if self.screen_only {
            self.auto_off = plan_for_deep(self.config.off_after, self.auto_off, crate::auto_off::clock(), minute().as_secs_f64() * 60.0);
        }

        // The battery per game doesn't count minutes with the game paused in the dark (`battery_games`).
        crate::battery_history::set_asleep(true);
        // Nor does Play history: the running game's stretch ends here (`play_log`, a channel send).
        crate::play_log::paused();
    }

    fn wake_light(&mut self, by: &str) {
        println!("sleep: awake ({by})");
        self.power_key.cancel();
        // The game first (a line into pocket-core's FIFO, or SIGCONT): its next frame is under way while the screen
        // comes on, on a thread of its own, so neither waits for the other or holds this thread.
        if let Some(game) = &self.game {
            game.resume(self.stopped_game);
            self.game_on_us = Some(crate::trace::now_us());
            crate::trace::step("sleep: game on");
        }
        self.screen_job = self.sys.screen_on_soon(self.brightness);
        self.woke_news = true;
        self.stopped_game = false;
        self.state = State::Awake;
        crate::battery_history::set_asleep(false);
        // Play history: the game's next stretch begins (after a suspend, on the day it is now). A game that ran on
        // behind the dark screen never paused its stretch.
        if !std::mem::take(&mut self.dark_game) {
            crate::play_log::resumed();
        }
        self.timers.light_since = None;
        self.timers.last_input = Instant::now();
        self.woke_at = Some(Instant::now());
        // Awake: the next dark spell counts from its own first suspend (`auto_off`).
        self.auto_off = None;
    }

    /// Deep sleep right away (power key, the quick settings' Sleep).
    pub fn sleep_now(&mut self, ctx: Ctx) {
        if self.screen_only {
            self.screen_off_now(ctx);
            return;
        }
        match self.state {
            State::Awake => {
                self.light(ctx);
                self.deep(ctx);
            }
            State::Light => {
                self.pause_dark_game();
                self.deep(ctx);
            }
            State::Deep | State::PoweringOff => {}
        }
    }
    /// The power button's short press set to Screen off (Settings → System → Sleep & power): dark at once. A game
    /// being played (no pause menu open) goes on, its sound too, and the timers never put the Pocket to sleep from
    /// here (`dark_game`); pocket-core stops drawing and keeps the buttons from the game until the screen is back
    /// ("screen off"; its "wake" ends that and hides the button that woke it until it is let go). Other emulators
    /// keep drawing to the dark screen and get every press. Anywhere else (the shelf, the pause menu) it is the
    /// light sleep of a while without input, Sleep after included. Any button but Vol± turns the screen back on.
    pub fn screen_off_now(&mut self, ctx: Ctx) {
        if self.state != State::Awake {
            return;
        }
        if self.game.is_none() || ctx.menu_open {

            self.light(ctx);
            return;
        }
        self.power_key.cancel();
        crate::trace::step("sleep: screen off, the game on");
        self.brightness = self.sys.screen_off().filter(|l| *l > 0).or(self.brightness);
        if let Some(Game {
            kind: GameKind::PocketCore { .. },
            ..
        }) = &self.game
        {


            control::pocket_core("screen off");
        }
        self.menu_open = false;
        self.stopped_game = false;
        self.dark_game = true;
        self.state = State::Light;
        self.timers.light_since = Some(Instant::now());
        self.timers.failures = 0;
        if self.screen_only {
            self.auto_off = plan_for_deep(self.config.off_after, self.auto_off, crate::auto_off::clock(), minute().as_secs_f64() * 60.0);
        }

        // The battery per game doesn't count dark minutes: the screen is most of what a game costs. Play history goes
        // on, the game being played.
        crate::battery_history::set_asleep(true);
    }
    /// A game playing behind the dark screen (`screen_off_now`), as the fan and the quick settings need to know.
    pub fn dark_game(&self) -> bool {
        self.dark_game
    }
    /// A game left running in the dark is paused as light sleep pauses it, before something else (a suspend) starts.
    fn pause_dark_game(&mut self) {
        if !std::mem::take(&mut self.dark_game) {
            return;
        }
        if let Some(game) = &self.game {
            game.pause();
            self.stopped_game = !matches!(game.kind, GameKind::PocketCore { .. });
        }
        crate::play_log::paused();
    }
    /// Saves, then suspends, in a worker thread; `tick` sees it come back.
    fn deep(&mut self, _ctx: Ctx) {
        // Final barrier: no worker, suspend command or RTC alarm on this platform.
        if self.screen_only {
            return;
        }
        self.pause_dark_game();
        self.state = State::Deep;
        self.deep_seq += 1;
        // Switch off after sleeping: counted from the dark spell's first suspend; a time that passed while nothing
        // could act (on the charger in light sleep) starts over.
        self.auto_off = plan_for_deep(self.config.off_after, self.auto_off, crate::auto_off::clock(), minute().as_secs_f64() * 60.0);
        let plan = self.auto_off;
        let worker = DeepWorker {
            seq: self.deep_seq,
            game: self.game.clone(),
            // Stopped by us or by the open pause menu: stopped again after saving.
            stopped: self.stopped_game || self.menu_open,
            sys: self.sys.clone(),
            data_dir: self.data_dir.clone(),
            tx: self.tx.clone(),
            notify: self.notify.clone(),
            own: self.own_suspend.clone(),
            t0: Instant::now(),
            hooks_us: None,
            volume_us: None,
            rtc: self.rtc.clone(),
        };
        let turn = self.suspend_turn.clone();
        self.own_suspend.store(true, Ordering::SeqCst);
        let run = move || {
            let mut worker = worker;
            // An earlier deep sleep still waiting for its wake hooks finishes first.
            let _turn = turn.lock().unwrap_or_else(|e| e.into_inner());
            // A power off disarmed the alarm while this one waited its turn: no suspend on the way down.
            if worker.rtc.shut() {
                println!("sleep: a power off is under way; this deep sleep does not start");
                worker.own.store(false, Ordering::SeqCst);
                return;
            }
            worker.own.store(true, Ordering::SeqCst);
            worker.t0 = Instant::now();
            deep_sequence(&mut worker, plan);
            // Ended by a power off without `done`: the next sleep is not "ours" any more.
            worker.own.store(false, Ordering::SeqCst);
        };
        if let Err(e) = std::thread::Builder::new().name("deep-sleep".into()).spawn(run) {
            eprintln!("sleep: no thread for the suspend ({e}); staying awake");
            self.own_suspend.store(false, Ordering::SeqCst);
            self.wake_light("no deep sleep");
        }
    }

    /// The system is back from deep sleep (its wake hooks may still be running): game and screen on now.
    fn resumed(&mut self, at_us: u64) {
        self.timers.failures = 0;

        self.resumed_us = at_us;
        self.game_on_us = None;
        // On its own thread (three reads 100 ms apart and a file write): the screen comes on without waiting for it.
        // Asked before `wake_light`, so it is taken in the sleep's generation and counts for no game.
        crate::battery::sample_soon(&self.data_dir);
        self.wake_light("back from deep sleep");
    }

    /// The deep sleep's worker is done: after a suspend, ROCKNIX's wake hooks are over and the volume is set again.
    fn woke(&mut self, slept: bool, hooks_us: Option<u64>, volume_us: Option<u64>) {
        if !slept {
            // Stay dark and paused; try again after a longer while each time (`Timers::deep_wait`).
            self.timers.failures += 1;
            self.state = State::Light;
            self.timers.light_since = Some(Instant::now());
            println!("sleep: trying to suspend again in {} s", self.timers.deep_wait().unwrap_or_default().as_secs());
            return;
        }
        println!(
            "{}",
            wake_timeline(
                self.resumed_us,
                POWER_KEY_US.load(Ordering::SeqCst),
                self.sys.screen_on_us.load(Ordering::SeqCst),
                self.game_on_us,
                hooks_us,
                volume_us
            )
        );
        // Dark again since (the light sleep's `lightsleep`): the game stays as it is, the screen dark.
        if self.state != State::Awake {
            return;
        }
        // The hook set PipeWire's quantum back to 1024 after pocket-core's `wake`: a second `wake` lowers it again
        // (to an awake pocket-core that is all it does).
        if let Some(Game {
            kind: GameKind::PocketCore { .. },
            ..
        }) = &self.game
        {
            control::pocket_core("wake");
        }
        // A hook that turned the backlight off again (ROCKNIX's sleep hooks are not ours): on once more. A read of
        // sysfs here; a write only in that case, and on the screen thread.
        if self.sys.screen_dark() {
            println!("sleep: the screen was dark again after the wake hooks; on again");
            self.screen_job = self.sys.screen_on_soon(self.brightness);
        }
    }

    /// Saves the game, quits it, leaves the resume note and powers off. On this Mini V2 ROCKNIX runs
    /// from RAM: switching on again starts Android, and the game resumes the next time ROCKNIX boots.
    pub fn power_off(&mut self) {
        self.shut_down(PowerAction::PowerOff);
    }

    /// The same for the power menu's Restart (`Reboot`), Restart to boot menu (`RebootBootloader`) and Restart
    /// pocket-ui (`RestartUi`, then `take_restart`): the game is saved and quit first, and the next start of pocket-ui
    /// opens it again.
    pub fn shut_down(&mut self, action: PowerAction) {
        self.shut_down_for(action, Why::Asked);
    }

    /// Restart pocket-ui is through (the game saved and quit): true once; the app ends then with exit code 0, which
    /// the launcher answers with a new start (deploy/run-pocket-ui.sh).
    pub fn take_restart(&mut self) -> bool {
        std::mem::take(&mut self.restart)
    }

    /// `shut_down`, for a reason: the low-battery guard's says so on the screen; switching off after sleeping keeps the
    /// screen dark (from Deep too, where its worker ended without waking anything) and leaves a note for the next start.
    fn shut_down_for(&mut self, action: PowerAction, why: Why) {
        let dark = matches!(why, Why::Slept(_));
        match self.state {
            State::PoweringOff => return,
            State::Deep if !dark => return,
            State::Deep => {}
            State::Light if dark => {}
            // Dark: on again, so "Saving…" can be seen.
            State::Light => {
                self.screen_job = self.sys.screen_on_soon(self.brightness.take());
            }
            State::Awake if dark => return,
            State::Awake => {}
        }
        self.state = State::PoweringOff;
        // From this moment no alarm may be armed and no deep sleep starts; the disarm itself waits until the game is
        // saved and quit, just before the command (a failed command opens the gate again).
        if action != PowerAction::RestartUi {
            self.rtc.close();
        }
        // A game playing in the dark is quit from where it is (it saves on the way).
        self.dark_game = false;
        let (game, sys, data_dir, overlay) = (self.game.clone(), self.sys.clone(), self.data_dir.clone(), self.overlay.clone());
        let (notify, tx, wake_app) = (self.notify.clone(), self.tx.clone(), self.notify.clone());
        let rtc = self.rtc.clone();
        let note = match why {
            Why::Slept(hours) => Some(crate::auto_off::Note { hours, brightness: self.brightness.filter(|b| *b > 0) }),
            Why::Asked | Why::BatteryEmpty => None,
        };
        // Nothing on a dark screen: a frame drawn for nobody, and pocket-ui's window over a game that is quitting.
        let say = move |s: &str| {
            if dark {
                return;
            }
            if let Ok(mut o) = overlay.lock() {
                *o = Some(s.to_string());
            }
            notify();
        };
        std::thread::spawn(move || {
            crate::shutdown_log::event(&data_dir, &format!("{action:?} asked ({why:?})"));
            // A store package half moved into place would stay half installed (review N5): it finishes first.
            let end = Instant::now() + STORE_WAIT;
            if crate::store::changing() {
                say("Finishing the install…");
                while crate::store::changing() && Instant::now() < end {
                    std::thread::sleep(Duration::from_millis(100));
                }
            }
            if let Some(game) = &game {
                say(if why == Why::BatteryEmpty { "Battery empty. Saving…" } else { "Saving…" });
                write_resume(&data_dir, game);
                // In a bag: no sound of the game going on for a moment before it quits (RetroArch resumes first).
                let muted = dark.then(mute_sink).flatten();
                game.quit_for_power_off();
                if let Some(was) = muted {
                    set_mute(was);
                }
            }
            if let Some(note) = &note {
                crate::auto_off::write_note(&data_dir, note);
            }
            say(shutdown_text(action));
            sync_filesystems();
            // Restart pocket-ui: the system stays up; the app ends and its launcher starts it again.
            if action == PowerAction::RestartUi {
                println!("sleep: restarting pocket-ui");
                let _ = tx.send(Msg::Restart);
                wake_app();
                return;
            }
            let cmd = sys.shutdown_cmd(action);
            println!("sleep: {action:?} ({cmd})");
            // The last thing before the command: no wake alarm left armed (or in the past) for the power off to
            // meet, and none that a suspend worker could arm again (`Rtc` keeps its gate shut).
            crate::shutdown_log::snapshot(&data_dir, &format!("{action:?} ({why:?}): the state before the command"));
            crate::auto_off::disarm_before_power_off(&rtc, &data_dir, &format!("{action:?}"));
            crate::shutdown_log::event(&data_dir, &format!("{action:?}: running `{cmd}`"));
            let (ok, how) = crate::shutdown_log::run_and_describe(cmd);
            crate::shutdown_log::event(&data_dir, &format!("{action:?}: `{cmd}` ended: {how}"));
            if !ok {
                eprintln!("sleep: {action:?} (`{cmd}`) failed: {how}");
                // Still up: alarms may be set again by the next sleep.
                rtc.reopen();
                if note.is_some() {
                    crate::auto_off::clear_note(&data_dir);
                    let _ = tx.send(Msg::SwitchOffFailed);
                    wake_app();
                    return;
                }
                say(if action == PowerAction::PowerOff { "Couldn't switch off." } else { "Couldn't restart." });
                // Tests exercise completion without waiting out the on-screen notice.
                #[cfg(not(test))]
                std::thread::sleep(Duration::from_secs(3));
                let _ = tx.send(Msg::PowerOffFailed);
                wake_app();
            }
        });
    }

    /// Notices suspends pocket-ui didn't start, and keeps the last good volume for them.
    #[cfg(target_os = "linux")]
    fn spawn_watcher(&self) {
        let (tx, notify, own, last_volume) = (self.tx.clone(), self.notify.clone(), self.own_suspend.clone(), self.last_volume.clone());
        std::thread::spawn(move || {
            let mut base = suspended_secs();
            let mut checked = Instant::now() - Duration::from_secs(60);
            loop {
                // CLOCK_BOOTTIME: right after a suspend of logind's this ends at once, so the volume and the quantum
                // are repaired seconds sooner than the rest of a 2 s nap on the monotonic clock.
                boottime_nap(Duration::from_secs(2));
                let now = suspended_secs();
                if own.load(Ordering::SeqCst) {
                    base = now;
                    continue;
                }
                if now - base > 2.0 {
                    crate::wifi::resumed();
                    // Play history: the running game's time goes on after the suspend, on the day it is now.
                    crate::play_log::suspended((now - base) as u64);
                    base = now;
                    wait_for_wake_hooks();
                    let v = last_volume.load(Ordering::SeqCst);
                    if v != u32::MAX {
                        set_volume(v & 0xffff, v >> 16 != 0, true);
                    }
                    let _ = tx.send(Msg::ExternalWake);
                    notify();
                    continue;
                }
                if checked.elapsed() >= Duration::from_secs(15) {
                    checked = Instant::now();
                    if let Some((v, muted)) = sink_volume() {
                        last_volume.store(v.min(0xffff) | (muted as u32) << 16, Ordering::SeqCst);
                    }
                }
            }
        });
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;

    fn timers(now: Instant) -> Timers {
        Timers {
            light_after: Some(Duration::from_secs(180)),
            deep_after: Some(Duration::from_secs(120)),
            last_input: now,
            light_since: None,
            failures: 0,
        }
    }
    #[test]
    fn buttons_do_nothing_right_after_waking() {
        let t0 = Instant::now();
        let press = |button| PadEvent::Button { button, pressed: true, source: 0 };
        let a = press(Button::South);
        assert!(quiet_after_wake(Some(t0), t0 + Duration::from_millis(200), &a), "a second press in the pocket");
        assert!(!quiet_after_wake(Some(t0), t0 + WAKE_QUIET, &a), "a real press after the pause");
        assert!(!quiet_after_wake(None, t0, &a), "not after a wake");
        assert!(!quiet_after_wake(Some(t0), t0, &press(Button::VolUp)), "volume stays ROCKNIX's");
        assert!(!quiet_after_wake(Some(t0), t0, &press(Button::Power)), "the power key has its own guard");
        assert!(!quiet_after_wake(Some(t0), t0, &PadEvent::Button { button: Button::South, pressed: false, source: 0 }));
    }

    #[test]
    fn idle_goes_light_then_deep() {
        let t0 = Instant::now();

        let mut t = timers(t0);
        assert_eq!(
            t.due(State::Awake, t0 + Duration::from_secs(10), true),
            (None, Some(t0 + Duration::from_secs(180)))
        );
        assert_eq!(t.due(State::Awake, t0 + Duration::from_secs(180), true).0, Some(Due::Light));
        t.light_since = Some(t0 + Duration::from_secs(180));
        assert_eq!(
            t.due(State::Light, t0 + Duration::from_secs(200), true),
            (None, Some(t0 + Duration::from_secs(300)))
        );
        assert_eq!(t.due(State::Light, t0 + Duration::from_secs(300), true).0, Some(Due::Deep));
        // Without auto-suspend (testing on the device) light sleep stays; the low-battery guard still watches.
        assert_eq!(t.due(State::Light, t0 + Duration::from_secs(3000), false), (None, None));
        // Off: never.
        t.light_after = None;
        assert_eq!(t.due(State::Awake, t0 + Duration::from_secs(99_999), true), (None, None));
        assert_eq!(t.due(State::Deep, t0, true), (None, None));
    }



    #[test]
    fn a_failed_suspend_is_tried_again_later_and_later_but_never_given_up() {
        // Review B12 / SOL-05: after two failures idle used to stay in light sleep for good.
        let t0 = Instant::now();
        let mut t = timers(t0);
        t.light_since = Some(t0);
        let mut waits = Vec::new();
        for failures in 0..8 {
            t.failures = failures;
            let (due, next) = t.due(State::Light, t0, true);
            assert_eq!(due, None);
            let next = next.expect("a next suspend is always planned");
            waits.push((next - t0).as_secs() / 60);
            assert_eq!(t.due(State::Light, next, true).0, Some(Due::Deep));
        }
        assert_eq!(waits, [2, 4, 8, 16, 30, 30, 30, 30]);
        t.failures = u32::MAX;
        assert_eq!(t.deep_wait(), Some(RETRY_AT_MOST), "no overflow however many");
    }

    #[test]
    fn input_means_buttons_and_real_stick_movement() {
        let b = |pressed| PadEvent::Button {
            source: 0,
            button: Button::South,
            pressed,
        };
        let a = |axis, value| PadEvent::Axis { source: 0, axis, value };
        assert!(is_input(&b(true)));
        assert!(!is_input(&b(false)));
        assert!(!is_input(&a(Axis::LeftX, 0.1)));
        assert!(is_input(&a(Axis::LeftY, -0.8)));
        assert!(!is_input(&a(Axis::L2, 0.2)));
        assert!(is_input(&a(Axis::R2, 0.9)));
    }

    #[test]
    fn restart_and_the_boot_menu_run_systemctl_like_power_off_does() {
        assert_eq!(default_command(PowerAction::Sleep), "systemctl suspend");
        assert_eq!(default_command(PowerAction::PowerOff), "systemctl poweroff");
        assert_eq!(default_command(PowerAction::Reboot), "systemctl reboot");
        assert_eq!(default_command(PowerAction::RebootBootloader), "systemctl reboot --reboot-argument=bootloader");
    }

    #[test]
    fn the_screen_says_what_is_happening() {
        assert_eq!(shutdown_text(PowerAction::PowerOff), "Switching off\u{2026}");
        assert_eq!(shutdown_text(PowerAction::Reboot), "Restarting\u{2026}");
        assert_eq!(shutdown_text(PowerAction::RebootBootloader), "Restarting to the boot menu\u{2026}");
    }

    #[test]
    fn volume_from_pactl() {
        assert_eq!(
            parse_volume("Volume: front-left: 39321 /  60% / -13.31 dB,   front-right: 39321 /  60% / -13.31 dB\n"),
            Some(60)
        );
        assert_eq!(parse_volume("Volume: mono: 0 /   0% / -inf dB"), Some(0));
        assert_eq!(parse_volume("Failed to get sink volume"), None);
    }

    #[test]
    fn resume_note_is_read_once() {
        let dir = TestDir::new("sleep");
        let rom = dir.join("game.gbc");
        std::fs::write(&rom, b"x").unwrap();
        let game = Game {
            system: "gbc".into(),
            name: "Game".into(),
            path: rom.clone(),
            kind: GameKind::Other,
            pgid: None,
        };
        write_resume(&dir, &game);
        assert_eq!(
            take_resume(&dir),
            Some(Resume {
                system: "gbc".into(),
                name: "Game".into(),
                path: rom.clone()
            })
        );
        assert_eq!(take_resume(&dir), None);
        // A game that is gone isn't opened.
        write_resume(&dir, &game);
        std::fs::remove_file(&rom).unwrap();
        assert_eq!(take_resume(&dir), None);
    }

    /// How a suspend of `FakeDeep` ends.
    #[derive(Clone, Copy, Debug)]
    enum Ends {
        /// The kernel refuses it.
        Fails,
        /// The power key after this many seconds, unless the RTC alarm goes off first.
        Key(f64),
        /// The RTC alarm (asleep until it goes off).
        Alarm,
        /// The RTC alarm, and the power key pressed at that moment.
        AlarmAndKey,
        /// The RTC's alarm event this many seconds in, long before its time: the RTC was set past it (a clock set by
        /// hand or by network time; the kernel fires an armed alarm whose time the new RTC time has passed at once).
        RtcJump(f64),
    }

    /// The steps of a deep sleep, recorded instead of done, on a fake clock (CLOCK_BOOTTIME), a fake RTC and a fake
    /// power supply.
    struct FakeDeep {
        log: Vec<&'static str>,
        volume: Option<(u32, bool)>,
        slept: bool,
        /// How the suspends end, in turn (`slept` alone when empty and there is no plan).
        script: std::collections::VecDeque<Ends>,
        clock: f64,
        /// When the RTC alarm goes off, on `clock`.
        rtc: Option<f64>,
        /// Each alarm asked for (None: cleared).
        alarms: Vec<Option<u64>>,
        presses: u64,
        /// The power supply as each reading finds it (the last one stays).
        power: std::collections::VecDeque<Option<crate::low_battery::Level>>,
        switched_off: Option<crate::auto_off::Plan>,
        /// The wall clock against CLOCK_BOOTTIME (`wall_offset`); a clock set moves it.
        offset: f64,
        pauses: usize,
        /// A power key press arrives while the look in the dark (`settle`) or the confirming pause runs.
        press_at_settle: bool,
        press_at_pause: bool,
        /// A power off begins before the next round.
        shutting: bool,
    }

    fn fake(volume: Option<(u32, bool)>, slept: bool) -> FakeDeep {
        FakeDeep {
            log: Vec::new(),
            volume,
            slept,
            script: Default::default(),
            clock: 1000.0,
            rtc: None,
            alarms: Vec::new(),
            presses: 0,
            power: Default::default(),
            switched_off: None,
            offset: 1.7e9,
            pauses: 0,
            press_at_settle: false,
            press_at_pause: false,
            shutting: false,
        }
    }

    impl DeepSteps for FakeDeep {
        fn save(&mut self) {
            self.log.push("save");
        }
        fn volume(&mut self) -> Option<(u32, bool)> {
            self.log.push("read volume");
            self.volume
        }
        fn set_volume(&mut self, _: (u32, bool), kick: bool) {
            self.log.push(if kick { "volume again" } else { "store volume" });
        }
        fn suspend(&mut self) -> bool {
            self.log.push("suspend");
            let Some(ends) = self.script.pop_front() else { return self.slept };
            let alarm = |f: &mut FakeDeep| {
                f.clock = f.rtc.take().expect("asleep with no alarm set: it would never wake");
            };
            match ends {
                Ends::Fails => return false,
                Ends::Key(after) => match self.rtc {
                    Some(at) if at <= self.clock + after => alarm(self),
                    _ => {
                        self.clock += after;
                        self.presses += 1;
                    }
                },
                Ends::Alarm => alarm(self),
                Ends::AlarmAndKey => {
                    alarm(self);
                    self.presses += 1;
                }
                Ends::RtcJump(after) => {
                    // CLOCK_BOOTTIME only moves by the time asleep; the alarm is spent. The wall clock was set forward.
                    self.clock += after;
                    self.rtc = None;
                    self.offset += 3600.0;
                }
            }
            true
        }
        fn resumed(&mut self) {
            self.log.push("screen and game on");
        }
        fn wait_for_hooks(&mut self) {
            self.log.push("wake hooks done");
        }
        fn done(&mut self, slept: bool) {
            self.log.push(if slept { "done" } else { "failed" });
        }
        fn clock(&mut self) -> f64 {
            self.clock
        }
        fn alarm(&mut self, secs: Option<u64>) {
            self.log.push(if secs.is_some() { "alarm" } else { "clear alarm" });
            self.alarms.push(secs);
            self.rtc = secs.map(|s| self.clock + s as f64);
        }
        fn power_presses(&mut self) -> u64 {
            self.presses
        }
        fn settle(&mut self) {
            self.log.push("settle in the dark");
            self.clock += 1.0;
            self.presses += self.press_at_settle as u64;
        }
        fn power(&mut self) -> Option<crate::low_battery::Level> {
            let now = if self.power.len() > 1 { self.power.pop_front() } else { self.power.front().copied() };
            now.flatten()
        }
        fn wall_offset(&mut self) -> f64 {
            self.offset
        }
        fn shutting_down(&mut self) -> bool {
            self.shutting
        }
        fn pause(&mut self, _: Duration) {
            self.log.push("confirm in the dark");
            self.pauses += 1;
            self.clock += 2.0;
            self.presses += self.press_at_pause as u64;
        }
        fn switch_off(&mut self, plan: crate::auto_off::Plan) {
            self.log.push("switch off");
            self.switched_off = Some(plan);
        }
    }

    fn run_deep(volume: Option<(u32, bool)>, slept: bool) -> Vec<&'static str> {
        let mut f = fake(volume, slept);
        deep_sequence(&mut f, None);
        f.log
    }

    const HOUR: f64 = 3600.0;

    fn on_battery() -> Option<crate::low_battery::Level> {
        Some(crate::low_battery::Level { percent: 70, external_power: false })
    }

    fn on_charger() -> Option<crate::low_battery::Level> {
        Some(crate::low_battery::Level { percent: 70, external_power: true })
    }

    /// A deep sleep with switch-off after 3 h, from the fake clock's start, the suspends ending as `script` says.
    fn run_plan(script: &[Ends], power: &[Option<crate::low_battery::Level>]) -> FakeDeep {
        let mut f = fake(Some((60, false)), true);
        f.script = script.iter().copied().collect();
        f.power = power.iter().copied().collect();
        let plan = crate::auto_off::Plan::new(f.clock, 3, HOUR);
        deep_sequence(&mut f, Some(plan));
        assert!(f.script.is_empty(), "every suspend of the script happened");
        f
    }

    #[test]
    fn the_alarm_on_battery_switches_off_with_nothing_turned_on() {
        let f = run_plan(&[Ends::Alarm], &[on_battery()]);
        assert_eq!(
            f.log,
            [
                "save", "read volume", "store volume", "alarm", "suspend", "settle in the dark",
                "confirm in the dark", "confirm in the dark", "clear alarm", "switch off"
            ],
            "no screen, no game, no volume: the screen stays dark; the battery was read three times"
        );
        assert_eq!(f.alarms, [Some(3 * 3600), None]);
        assert_eq!(f.switched_off.map(|p| p.hours), Some(3));
    }

    #[test]
    fn on_the_charger_it_stays_asleep_and_looks_again_a_period_later() {
        // Two alarms on the charger, then woken by the power key: never switched off, never visible before the key.
        let f = run_plan(&[Ends::Alarm, Ends::Alarm, Ends::Key(600.0)], &[on_charger()]);
        assert_eq!(f.switched_off, None);
        assert_eq!(
            f.log,
            [
                "save", "read volume", "store volume",
                "alarm", "suspend", "settle in the dark",
                "alarm", "suspend", "settle in the dark",
                "alarm", "suspend",
                "screen and game on", "clear alarm", "volume again", "wake hooks done", "volume again", "done"
            ]
        );
        // Each new alarm a full period after the decision (the settle took a second of the fake clock).
        assert_eq!(f.alarms, [Some(3 * 3600), Some(3 * 3600), Some(3 * 3600), None]);
        // Unplugged in the bag between the first and the second look: the second one switches off.
        let f = run_plan(&[Ends::Alarm, Ends::Alarm], &[on_charger(), on_battery()]);
        assert!(f.switched_off.is_some());
        // A battery that can't be read is no reason to switch off either.
        let f = run_plan(&[Ends::Alarm, Ends::Key(5.0)], &[None]);
        assert_eq!(f.switched_off, None);
    }

    #[test]
    fn a_button_wake_before_the_alarm_is_a_normal_wake_with_nothing_added_before_the_screen() {
        let f = run_plan(&[Ends::Key(HOUR)], &[on_battery()]);
        assert_eq!(f.switched_off, None);
        assert_eq!(
            f.log,
            ["save", "read volume", "store volume", "alarm", "suspend", "screen and game on", "clear alarm", "volume again", "wake hooks done", "volume again", "done"],
            "no settling, no battery read on the way to the screen; the alarm is cleared after it"
        );
        assert_eq!(f.rtc, None, "no alarm left to go off later");
        // Without switch-off nothing about the alarm happens at all (the wake of before 2026-10-25).
        assert!(!run_deep(Some((60, false)), true).iter().any(|s| s.contains("alarm")));
    }

    /// Settings › System › Time › Date and time sets the wall clock and the RTC (`manual_time`). Switching off after
    /// sleeping counts on CLOCK_BOOTTIME, not the wall clock, and arms the RTC alarm relative ("+N") just before each
    /// suspend and clears it after every wake, so a clock set while awake finds no alarm, and moves nothing it counts
    /// on. An alarm the jump fires early (the kernel does for an armed alarm the new RTC time has passed, between the
    /// arming and the suspend) is no wake of a player and no new start of the count: the wall clock against
    /// CLOCK_BOOTTIME moved, so it is asleep again for what is left (2026-10-26; before, the screen came on in the bag
    /// and the next sleep began a full 3 h).
    #[test]
    fn a_clock_set_by_hand_neither_switches_off_nor_wakes_the_screen_nor_restarts_the_count() {
        // The plan's clock is not the wall clock: a set of the wall clock doesn't move it.
        let wall = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs_f64();
        assert!((wall - crate::auto_off::clock()).abs() > 1e8, "auto_off counts on CLOCK_BOOTTIME");
        // A wake by the power key an hour in: no alarm is left armed while the Pocket is awake (and the clock is set).
        let f = run_plan(&[Ends::Key(HOUR)], &[on_battery()]);
        assert_eq!((f.rtc, f.switched_off), (None, None));
        // The clock set, the alarm fired early, on battery, nobody there: the screen stays dark and the game paused;
        // asleep again for what is left of the period, then the alarm at its real time switches off.
        let f = run_plan(&[Ends::RtcJump(600.0), Ends::Alarm], &[on_battery()]);
        assert_eq!(f.switched_off.map(|p| p.hours), Some(3));
        assert_eq!(
            f.log,
            [
                "save", "read volume", "store volume", "alarm", "suspend", "settle in the dark",
                "alarm", "suspend", "settle in the dark", "confirm in the dark", "confirm in the dark", "clear alarm", "switch off"
            ]
        );
        // The second alarm is what was left of the first period (600 s asleep and the 1 s look), not a new 3 h.
        assert_eq!(f.alarms, [Some(3 * 3600), Some(3 * 3600 - 601), None]);
        // A key pressed at that moment is someone there: a normal wake, as after any wake.
        let mut g = fake(Some((60, false)), true);
        g.script = [Ends::RtcJump(600.0)].into_iter().collect();
        g.press_at_settle = true;
        let plan = crate::auto_off::Plan::new(g.clock, 3, HOUR);
        deep_sequence(&mut g, Some(plan));
        assert_eq!(g.switched_off, None);
        assert_eq!(
            g.log,
            ["save", "read volume", "store volume", "alarm", "suspend", "settle in the dark", "screen and game on", "clear alarm", "volume again", "wake hooks done", "volume again", "done"]
        );
        assert_eq!(g.rtc, None);
        // The next dark spell keeps counting on its own clock: the switch-off is a full period after its first suspend.
        let next = plan_for_deep(Some(3), None, g.clock, HOUR).unwrap();
        assert!(!next.due(g.clock + 60.0));
        assert_eq!(next.secs_left(g.clock), 3 * 3600);
    }

    /// The wake hooks can outlast their 15 s cap and a charger's state arrives seconds after the wake: the first
    /// reading of "battery" is not enough to switch off (2026-10-26, Muse's review §2.2).
    #[test]
    fn a_deep_sleep_that_finds_a_power_off_under_way_neither_arms_nor_suspends() {
        let mut f = fake(Some((60, false)), true);
        f.shutting = true;
        let plan = crate::auto_off::Plan::new(f.clock, 3, HOUR);
        deep_sequence(&mut f, Some(plan));
        assert!(!f.log.contains(&"alarm") && !f.log.contains(&"suspend") && !f.log.contains(&"done"), "{:?}", f.log);
        assert!(f.alarms.is_empty());
    }

    #[test]
    fn a_charger_that_shows_up_after_the_first_reading_keeps_it_asleep() {
        // Battery at the alarm, the charger by the second reading: no switch-off; asleep again, a full period on.
        let f = run_plan(&[Ends::Alarm, Ends::Key(5.0)], &[on_battery(), on_charger()]);
        assert_eq!(f.switched_off, None);
        assert_eq!(f.pauses, 1);
        assert_eq!(&f.log[3..6], ["alarm", "suspend", "settle in the dark"]);
        assert_eq!(f.log[6], "confirm in the dark");
        assert_eq!(f.alarms.len(), 3, "armed again, then cleared at the key's wake: {:?}", f.alarms);
        // Charger only at the third reading.
        let f = run_plan(&[Ends::Alarm, Ends::Key(5.0)], &[on_battery(), on_battery(), on_charger()]);
        assert_eq!((f.switched_off, f.pauses), (None, 2));
        // A battery that cannot be read on the way is no reason either.
        let f = run_plan(&[Ends::Alarm, Ends::Key(5.0)], &[on_battery(), None]);
        assert_eq!(f.switched_off, None);
        // The power key in that window: someone is there.
        let mut g = fake(Some((60, false)), true);
        g.script = [Ends::Alarm].into_iter().collect();
        g.power = [on_battery()].into_iter().collect();
        g.press_at_pause = true;
        let plan = crate::auto_off::Plan::new(g.clock, 3, HOUR);
        deep_sequence(&mut g, Some(plan));
        assert_eq!(g.switched_off, None);
        assert!(g.log.contains(&"screen and game on"), "{:?}", g.log);
    }

    #[test]
    fn the_power_key_at_the_moment_of_the_alarm_wakes_it() {
        let f = run_plan(&[Ends::AlarmAndKey], &[on_battery()]);
        assert_eq!(f.switched_off, None);
        assert_eq!(
            &f.log[3..],
            ["alarm", "suspend", "settle in the dark", "screen and game on", "clear alarm", "volume again", "wake hooks done", "volume again", "done"]
        );
    }

    #[test]
    fn a_failed_suspend_clears_the_alarm_and_the_next_try_sets_only_what_is_left() {
        let f = run_plan(&[Ends::Fails], &[on_battery()]);
        assert_eq!(f.log, ["save", "read volume", "store volume", "alarm", "suspend", "clear alarm", "failed"]);
        assert_eq!(f.rtc, None);
        // The next try (2 min later, `Timers::deep_wait`) keeps the dark spell's plan: the alarm for what is left.
        let plan = plan_for_deep(Some(3), None, 1000.0, HOUR).unwrap();
        let later = plan_for_deep(Some(3), Some(plan), 1120.0, HOUR).unwrap();
        assert_eq!(later, plan);
        assert_eq!(later.secs_left(1120.0), 3 * 3600 - 120);
        // A time that passed with nothing able to act (on the charger in light sleep): a full period from now.
        assert_eq!(plan_for_deep(Some(3), Some(plan), plan.at + 50.0, HOUR).unwrap().at, plan.at + 50.0 + 3.0 * HOUR);
        // Changed in Settings since: the new time, from now. Off: none.
        assert_eq!(plan_for_deep(Some(1), Some(plan), 2000.0, HOUR).unwrap().at, 2000.0 + HOUR);
        assert_eq!(plan_for_deep(None, Some(plan), 2000.0, HOUR), None);
    }

    #[test]
    fn screen_and_game_come_on_before_the_wake_hooks_and_the_volume() {
        // Instant on: before 2026-10-24 the screen waited for ROCKNIX's wake hooks and the volume (seconds).
        assert_eq!(
            run_deep(Some((60, false)), true),
            [
                "save",
                "read volume",
                "store volume",
                "suspend",
                "screen and game on",
                "volume again",
                "wake hooks done",
                "volume again",
                "done"
            ]
        );
        assert_eq!(
            run_deep(None, true),
            ["save", "read volume", "suspend", "screen and game on", "wake hooks done", "done"]
        );
        // A suspend that didn't happen: nothing comes on (it stays in light sleep and tries again later).
        assert_eq!(run_deep(Some((60, false)), false), ["save", "read volume", "store volume", "suspend", "failed"]);
    }

    #[test]
    fn a_suspend_is_seen_by_the_kernels_counters_short_ones_too() {
        let p = |ok, fail, asleep| SuspendProbe { stats: Some((ok, fail)), asleep };
        let bare = |asleep| SuspendProbe { stats: None, asleep };
        let ms = Duration::from_millis;
        assert_eq!(suspend_outcome(p(3, 0, 10.0), p(3, 0, 10.0), ms(100)), None, "not yet");
        assert_eq!(suspend_outcome(p(3, 0, 10.0), p(4, 0, 10.3), ms(900)), Some(true), "under a second asleep (power pressed at once)");
        assert_eq!(suspend_outcome(p(3, 0, 10.0), p(3, 1, 10.0), ms(500)), Some(false), "the kernel says it failed: no 30 s wait");
        assert_eq!(suspend_outcome(p(3, 0, 10.0), p(3, 0, 40.0), ms(500)), Some(true), "the clock says so too");
        assert_eq!(suspend_outcome(bare(10.0), bare(40.0), ms(500)), Some(true), "no counters (virtual device): the clock");
        assert_eq!(suspend_outcome(bare(10.0), bare(10.5), ms(500)), None);
        assert_eq!(suspend_outcome(bare(0.0), bare(0.0), Duration::from_secs(4)), Some(true), "a test command that blocks");
    }

    #[test]
    fn the_wake_timeline_says_where_the_time_went() {
        assert_eq!(
            wake_timeline(1_000_000, 900_000, 1_004_000, Some(1_001_000), Some(1_800_000), Some(1_150_000)),
            "sleep: wake timeline, ms after the system was back: power key seen by the kernel -100 ms, screen on +4 ms, game on +1 ms, \
             ROCKNIX's wake hooks done +800 ms, volume set +150 ms"
        );
        // A power key pressed long before (not what woke it) and the screen-on of an earlier wake are left out.
        assert_eq!(wake_timeline(100_000_000, 1_000, 50, None, None, None), "sleep: wake timeline, ms after the system was back: nothing recorded");
    }

    /// A test's scratch folder that waits for the deep sleep's thread before it goes: the thread is still writing the
    /// RTC alarm when a test has seen `State::Deep`, and would make the folder again.
    struct SleepDir(crate::testutil::TestDir, Arc<AtomicBool>);

    impl std::ops::Deref for SleepDir {
        type Target = Path;
        fn deref(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for SleepDir {
        fn drop(&mut self) {
            let end = Instant::now() + Duration::from_secs(3);
            while self.1.load(Ordering::SeqCst) && Instant::now() < end {
                std::thread::sleep(Duration::from_millis(5));
            }
        }
    }

    /// The scratch folder comes first: a `let (dir, sleep, ..)` drops the `Sleep` before the folder goes.
    fn test_sleep(name: &str) -> (SleepDir, Sleep, PathBuf) {
        let dir = TestDir::new(&format!("sleep-{name}"));
        std::fs::create_dir_all(dir.join("backlight")).unwrap();
        let level = dir.join("backlight/brightness");
        std::fs::write(&level, "120").unwrap();
        let mut sys = System::new(Some(dir.join("backlight")), "false".into(), false);
        // Never the real thing from a test: a power off is a line in a file.
        sys.poweroff_cmd = format!("echo off >> {}", dir.join("powered-off").display());
        let mut sleep = Sleep::build(dir.to_path_buf(), sys, Arc::new(|| {}), InhibitorSlot::Have(None));
        sleep.rtc = crate::auto_off::Rtc::at(dir.join("wakealarm"));
        let dir = SleepDir(dir, sleep.own_suspend.clone());
        (dir, sleep, level)
    }

    /// Waits (up to 5 s) for the power-off thread to run the command.
    fn powered_off(dir: &Path) -> bool {
        let end = Instant::now() + Duration::from_secs(5);
        while Instant::now() < end {
            if dir.join("powered-off").exists() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        false
    }

    #[test]
    fn switching_off_after_sleeping_keeps_the_screen_dark_and_leaves_a_note() {
        let (dir, mut s, file) = test_sleep("switch-off");
        s.light(Ctx::default());
        s.state = State::Deep;
        s.deep_seq = 4;
        // An older deep sleep's news is not this one's.
        s.tx.send(Msg::SwitchOff { seq: 3, hours: 3 }).unwrap();
        s.tick(Ctx::default());
        assert_eq!(s.state(), State::Deep);
        s.tx.send(Msg::SwitchOff { seq: 4, hours: 3 }).unwrap();
        s.tick(Ctx::default());
        assert_eq!(s.state(), State::PoweringOff);
        assert!(powered_off(&dir));
        assert!(shutdown_log_with(&dir, "ended: exit 0").contains("RTC alarm disarm"), "the auto-off switch-off disarms like the others");
        assert_eq!(level(&file), "0", "the screen never came on");
        assert!(s.screen_job.is_none() && s.overlay().is_none());
        // The next start: the screen at its level again, and the line to say once.
        let note = crate::auto_off::take_note(&dir).unwrap();
        assert_eq!(note, crate::auto_off::Note { hours: 3, brightness: Some(120) });
    }

    /// The shutdown log once it says `what` (the thread writes the end after the command), up to 5 s.
    fn shutdown_log_with(dir: &Path, what: &str) -> String {
        let end = Instant::now() + Duration::from_secs(5);
        loop {
            let text = std::fs::read_to_string(dir.join("logs/shutdown.log")).unwrap_or_default();
            if text.contains(what) || Instant::now() >= end {
                return text;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    #[test]
    fn power_off_and_restart_disarm_an_armed_alarm_before_their_command() {
        for (action, name) in [(PowerAction::PowerOff, "off"), (PowerAction::Reboot, "reboot"), (PowerAction::RebootBootloader, "bootloader")] {
            let (dir, mut s, _file) = test_sleep(&format!("disarm-{name}"));
            let alarm = dir.join("wakealarm");
            // An alarm armed (or long past) when the power menu is used; the command copies what it finds.
            std::fs::write(&alarm, "1793051031").unwrap();
            let record = format!("cat {} > {}; echo ran >> {}", alarm.display(), dir.join("seen").display(), dir.join("powered-off").display());
            s.sys.poweroff_cmd = record.clone();
            s.sys.reboot_cmd = record.clone();
            s.sys.bootloader_cmd = record;
            s.shut_down(action);
            assert!(powered_off(&dir), "{name}");
            assert_eq!(std::fs::read_to_string(dir.join("seen")).unwrap(), "0", "{name}: the alarm was disarmed before the command");
            let log = shutdown_log_with(&dir, "ended: exit 0");
            for want in [format!("{action:?} asked"), "RTC alarm disarm verified (before=1793051031".into(), "running `".into(), "ended: exit 0".into()] {
                assert!(log.contains(&want), "{name}: {want} in\n{log}");
            }
            assert!(s.rtc.shut(), "{name}: nothing arms it again");
        }
    }

    #[test]
    fn the_gate_is_shut_when_the_power_off_begins_not_when_the_game_has_quit() {
        let (dir, mut s, _file) = test_sleep("gate-early");
        let (entered, waiting) = channel();
        let (release, resume) = channel();
        let resume = Mutex::new(resume);
        // Hold the worker at its visible notice, before it can disarm or run the command. No timed shell sleep.
        s.notify = Arc::new(move || {
            entered.send(()).unwrap();
            resume.lock().unwrap().recv_timeout(Duration::from_secs(5)).unwrap();
        });
        s.shut_down(PowerAction::PowerOff);
        waiting.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(s.rtc.shut(), "shut before the worker can disarm");
        assert!(!s.rtc.set(Some(30)));
        release.send(()).unwrap();
        assert!(shutdown_log_with(&dir, "ended: exit 0").contains("ended: exit 0"));
    }

    #[test]
    fn restart_pocket_ui_leaves_the_alarm_alone_because_the_system_stays_up() {
        let (dir, mut s, _file) = test_sleep("disarm-ui");
        std::fs::write(dir.join("wakealarm"), "1793051031").unwrap();
        s.shut_down(PowerAction::RestartUi);
        // Wait for the worker's completion message, then feed it through the ordinary tick handler.
        let message = s.rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(matches!(message, Msg::Restart));
        s.tx.send(message).unwrap();
        s.tick(Ctx::default());
        assert!(s.take_restart(), "the worker completed without a system command");
        assert_eq!(std::fs::read_to_string(dir.join("wakealarm")).unwrap(), "1793051031");
        assert!(!s.rtc.shut());
    }

    #[test]
    fn an_alarm_that_cannot_be_cleared_is_logged_and_the_power_off_goes_on() {
        let (dir, mut s, _file) = test_sleep("disarm-fails");
        // Every write to the alarm file fails.
        std::fs::create_dir(dir.join("wakealarm")).unwrap();
        s.shut_down(PowerAction::PowerOff);
        assert!(powered_off(&dir), "the power off is never blocked");
        let log = shutdown_log_with(&dir, "ended: exit 0");
        assert!(log.contains("RTC alarm disarm NOT verified") && log.contains("write=failed"), "{log}");
        assert!(log.contains("ended: exit 0"), "{log}");
    }

    #[test]
    fn a_failed_power_off_lets_alarms_be_set_again() {
        let (dir, mut s, _file) = test_sleep("disarm-fail-reopen");
        s.sys.poweroff_cmd = "false".into();
        std::fs::write(dir.join("wakealarm"), "1793051031").unwrap();
        s.shut_down(PowerAction::PowerOff);
        // Completion, rather than three seconds of wall time, drives the test's notice deadline.
        let message = s.rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(matches!(message, Msg::PowerOffFailed));
        s.tx.send(message).unwrap();
        s.tick(Ctx::default());
        assert_eq!(s.state(), State::Awake);
        let log = shutdown_log_with(&dir, "ended: exit 1");
        assert!(log.contains("RTC alarm disarm verified") && log.contains("ended: exit 1"), "{log}");
        assert!(!s.rtc.shut());
        assert!(s.rtc.set(Some(30)), "the next sleep can arm its alarm");
    }

    #[test]
    fn a_switch_off_that_fails_stays_dark_in_light_sleep() {
        let (dir, mut s, file) = test_sleep("switch-off-fails");
        s.sys.poweroff_cmd = "false".into();
        s.light(Ctx::default());
        s.auto_off = Some(crate::auto_off::Plan::new(0.0, 3, HOUR));
        s.state = State::Deep;
        s.deep_seq = 1;
        s.tx.send(Msg::SwitchOff { seq: 1, hours: 3 }).unwrap();
        s.tick(Ctx::default());
        let end = Instant::now() + Duration::from_secs(5);
        while s.state() != State::Light && Instant::now() < end {
            std::thread::sleep(Duration::from_millis(20));
            s.tick(Ctx::default());
        }
        assert_eq!((s.state(), level(&file).as_str()), (State::Light, "0"));
        assert_eq!(s.auto_off, None, "no second try in this dark spell");
        assert_eq!(crate::auto_off::take_note(&dir), None, "no note for a start that didn't follow a switch-off");
    }

    #[test]
    fn sm8550_never_suspends_or_sets_an_alarm_and_can_switch_off_in_the_dark() {
        let (dir, mut s, file) = test_sleep("sm8550");
        s.screen_only = true;
        let forbidden = dir.join("suspended");
        s.sys.suspend_cmd = format!("touch {}", forbidden.display());
        s.set_config(Config { screen_off: Some(30), off_after: Some(1), ..Config::default() });
        assert_eq!(s.config.short_press, ShortPress::ScreenOff);
        assert_eq!(s.config.sleep_after, None);
        s.timers.last_input = Instant::now() - Duration::from_secs(60);
        assert!(s.tick(Ctx::default()).is_some(), "idle screen-off schedules the optional power-off deadline");
        s.sleep_now(Ctx::default());
        assert_eq!((s.state(), level(&file).as_str()), (State::Light, "0"));
        assert!(s.auto_off.is_some(), "power-off counts from screen off, no suspend required");
        s.deep(Ctx::default()); // final barrier even if a future caller bypasses sleep_now
        assert_eq!(s.state(), State::Light);
        s.timers.light_since = Some(Instant::now() - Duration::from_secs(3600));
        s.tick(Ctx::default());
        assert_eq!(s.state(), State::Light);
        assert!(!forbidden.exists() && !dir.join("wakealarm").exists(), "no alarm set before the switch-off");
        s.auto_off = Some(crate::auto_off::Plan::new(crate::auto_off::clock() - 2.0 * HOUR, 1, HOUR));
        let battery = dir.join("battery");
        std::fs::create_dir_all(&battery).unwrap();
        s.battery_dir = battery.clone();
        std::fs::write(battery.join("capacity"), "60").unwrap();
        std::fs::write(battery.join("status"), "Charging").unwrap();
        s.tick(Ctx::default());
        assert_eq!(s.state(), State::Light, "never auto-off on the charger");
        std::fs::write(battery.join("status"), "Discharging").unwrap();
        s.tick(Ctx::default());
        assert_eq!(s.state(), State::PoweringOff);
        assert!(powered_off(&dir));
        assert!(!forbidden.exists());
        // The switch-off is a power off like the others: the alarm disarmed and read back first (nothing was ever armed:
        // the file did not exist before; the disarm made it in this fake folder and it says 0, never "+N").
        let log = shutdown_log_with(&dir, "ended: exit 0");
        assert!(log.contains("RTC alarm disarm verified") && log.contains("before=no-rtc"), "nothing was ever armed: {log}");
        assert_eq!(std::fs::read_to_string(dir.join("wakealarm")).unwrap_or_default(), "0");
    }

    #[test]
    fn dark_in_light_sleep_after_failed_suspends_it_switches_off_on_battery_only() {
        let (dir, mut s, file) = test_sleep("light-off");
        s.light(Ctx::default());
        // The failed suspend's news: back to light sleep, the plan kept.
        s.state = State::Deep;
        s.deep_seq = 2;
        let plan = crate::auto_off::Plan::new(crate::auto_off::clock() - 4.0 * HOUR, 3, HOUR);
        s.auto_off = Some(plan);
        s.tx.send(Msg::Woke { seq: 2, slept: false, hooks_us: None, volume_us: None }).unwrap();
        s.tick(Ctx::default());
        assert_eq!(s.state(), State::Light);
        // On the charger (or not read yet): it stays dark and looks again a minute on.
        let battery = dir.join("battery");
        std::fs::create_dir_all(&battery).unwrap();
        s.battery_dir = battery.clone();
        std::fs::write(battery.join("capacity"), "60").unwrap();
        std::fs::write(battery.join("status"), "Charging").unwrap();
        let next = s.tick(Ctx::default()).expect("a next look");
        assert!(next <= Instant::now() + Duration::from_secs(61));
        std::fs::remove_file(battery.join("capacity")).unwrap();
        s.tick(Ctx::default());
        assert_eq!(s.state(), State::Light, "a battery that can't be read is no reason");
        // Unplugged: switched off, dark.
        std::fs::write(battery.join("capacity"), "60").unwrap();
        std::fs::write(battery.join("status"), "Discharging").unwrap();
        s.tick(Ctx::default());
        assert_eq!(s.state(), State::PoweringOff);
        assert!(powered_off(&dir));
        assert_eq!(level(&file), "0");
    }

    #[test]
    fn a_wake_ends_the_plan_and_a_late_switch_off_does_nothing() {
        let (dir, mut s, file) = test_sleep("late-alarm");
        s.light(Ctx::default());
        s.state = State::Deep;
        s.deep_seq = 5;
        s.auto_off = Some(crate::auto_off::Plan::new(0.0, 3, HOUR));
        s.tx.send(Msg::Resumed { seq: 5, at_us: crate::trace::now_us() }).unwrap();
        s.tick(Ctx::default());
        s.screen_job.take().unwrap().join().unwrap();
        assert_eq!((s.state(), level(&file).as_str()), (State::Awake, "120"));
        assert_eq!(s.auto_off, None, "the next dark spell counts from its own first suspend");
        // News of a switch-off arriving now (an alarm after the manual wake) changes nothing.
        s.tx.send(Msg::SwitchOff { seq: 5, hours: 3 }).unwrap();
        s.tick(Ctx::default());
        assert_eq!(s.state(), State::Awake);
        assert!(!powered_off_now(&dir));
    }

    /// The incident of 2026-10-26 (touch dead for a while): `touched` is derived from the one state value, so no way
    /// back to Awake can leave touch off. Every way there, then a finger: it counts as activity again. A finger in
    /// every dark state does not wake and does not count.
    #[test]
    fn touch_counts_again_after_every_way_back_to_awake_and_never_wakes() {
        let (_dir, mut s, _) = test_sleep("touch-after-wake");
        let press = |button| PadEvent::Button { button, pressed: true, source: 0 };
        assert!(s.touched(), "awake from the start");
        // Light sleep, then a button.
        s.light(Ctx::default());
        assert!(!s.touched() && s.state() == State::Light, "a finger in light sleep: nothing, not awake");
        assert!(s.on_pad(&press(Button::South), Ctx::default()));
        s.screen_job.take().map(|j| j.join().unwrap());
        assert!(s.touched());
        // Screen off with the game going on, then a button.
        s.game_started(some_game());
        with_buttons(&mut s, ShortPress::ScreenOff, Hold::Menu);
        s.screen_off_now(Ctx::default());
        assert!(s.dark_game() && !s.touched() && s.state() == State::Light);
        assert!(s.on_pad(&press(Button::South), Ctx::default()));
        s.screen_job.take().map(|j| j.join().unwrap());
        assert!(!s.dark_game() && s.touched(), "the dark-game flag is gone with the wake");
        s.game_ended();
        // Deep sleep and back from it.
        s.light(Ctx::default());
        s.state = State::Deep;
        s.deep_seq = 9;
        assert!(!s.touched());
        s.tx.send(Msg::Resumed { seq: 9, at_us: crate::trace::now_us() }).unwrap();
        s.tick(Ctx::default());
        s.screen_job.take().map(|j| j.join().unwrap());
        assert!(s.state() == State::Awake && s.touched(), "back from deep sleep");
        // A suspend that did not happen stays dark; a button ends it.
        s.light(Ctx::default());
        s.state = State::Deep;
        s.deep_seq = 10;
        s.tx.send(Msg::Woke { seq: 10, slept: false, hooks_us: None, volume_us: None }).unwrap();
        s.tick(Ctx::default());
        assert!(s.state() == State::Light && !s.touched(), "dark and paused again");
        assert!(s.on_pad(&press(Button::South), Ctx::default()));
        s.screen_job.take().map(|j| j.join().unwrap());
        assert!(s.touched());
        // A switch-off that failed.
        s.state = State::PoweringOff;
        assert!(!s.touched());
        s.tx.send(Msg::PowerOffFailed).unwrap();
        s.tick(Ctx::default());
        assert!(s.state() == State::Awake && s.touched(), "power off failed: awake, touch counts");
    }

    /// A game of another emulator without a process: what the power button does with a game running.
    fn some_game() -> Game {
        Game { system: "gb".into(), name: "Tetris".into(), path: "/roms/gb/Tetris.gb".into(), kind: GameKind::Other, pgid: None }
    }

    fn with_buttons(s: &mut Sleep, short_press: ShortPress, hold: Hold) {
        s.config.short_press = short_press;
        s.config.hold = hold;
    }

    #[test]
    fn a_short_press_sleeps_by_default_and_turns_only_the_screen_off_when_set_so() {
        let (_dir, mut s, _) = test_sleep("short-sleep");
        assert_eq!((s.config.short_press, s.config.hold), (ShortPress::Sleep, Hold::Menu), "the defaults");
        s.game_started(some_game());
        assert!(s.power_step(Step::Short, true, Ctx::default()));
        assert_eq!(s.state(), State::Deep, "saved and suspending");
        assert!(!s.dark_game());

        let (_dir2, mut s, _) = test_sleep("short-screen");
        with_buttons(&mut s, ShortPress::ScreenOff, Hold::Menu);
        s.game_started(some_game());
        assert!(s.power_step(Step::Short, true, Ctx::default()));
        assert_eq!(s.state(), State::Light, "dark, not suspending");
        assert!(s.dark_game(), "the game goes on");
        assert!(!s.stopped_game, "nothing stopped");
    }

    #[test]
    fn screen_off_keeps_the_game_running_never_sleeps_by_itself_and_any_button_wakes_it() {
        let (_dir, mut s, file) = test_sleep("screen-off");
        with_buttons(&mut s, ShortPress::ScreenOff, Hold::Menu);
        s.game_started(some_game());
        s.screen_off_now(Ctx::default());
        assert_eq!((s.state(), s.dark_game()), (State::Light, true));
        assert_eq!(level(&file), "0", "the screen is dark");
        // Long past Sleep after: still dark, still playing, no suspend.
        s.timers.deep_after = Some(Duration::ZERO);
        s.timers.light_since = Some(Instant::now() - Duration::from_secs(3600));
        s.tick(Ctx::default());
        assert_eq!((s.state(), s.dark_game()), (State::Light, true), "the timers leave a playing game alone");
        // The volume keys don't wake it; any other button does, and is the wake's only.
        let press = |button| PadEvent::Button { button, pressed: true, source: 0 };
        assert!(s.on_pad(&press(Button::VolUp), Ctx::default()));
        assert_eq!(s.state(), State::Light);
        assert!(s.on_pad(&press(Button::South), Ctx::default()), "the waking press goes nowhere else");
        assert_eq!((s.state(), s.dark_game()), (State::Awake, false));
        s.screen_job.take().unwrap().join().unwrap();
        assert_eq!(level(&file), "120", "the screen at its level again");
    }
    #[test]
    fn a_game_that_ends_behind_the_dark_screen_leaves_the_usual_light_sleep() {
        let (_dir, mut s, _) = test_sleep("screen-off-ended");
        with_buttons(&mut s, ShortPress::ScreenOff, Hold::Menu);
        s.game_started(some_game());
        s.screen_off_now(Ctx::default());
        s.game_ended();
        assert_eq!((s.state(), s.dark_game()), (State::Light, false));
        s.timers.deep_after = Some(Duration::ZERO);
        s.tick(Ctx::default());
        assert_eq!(s.state(), State::Deep, "Sleep after counts again: nothing plays any more");
    }
    #[test]
    fn screen_off_without_a_game_or_with_the_pause_menu_open_is_the_usual_light_sleep() {
        let (_dir, mut s, _) = test_sleep("screen-off-shelf");
        with_buttons(&mut s, ShortPress::ScreenOff, Hold::Menu);
        s.power_step(Step::Short, true, Ctx::default());
        assert_eq!((s.state(), s.dark_game()), (State::Light, false), "the shelf: light sleep, Sleep after counts");
        s.timers.deep_after = Some(Duration::ZERO);
        s.tick(Ctx::default());
        assert_eq!(s.state(), State::Deep, "and the Pocket sleeps after it");

        let (_dir, mut s, _) = test_sleep("screen-off-menu");
        with_buttons(&mut s, ShortPress::ScreenOff, Hold::Menu);
        s.game_started(some_game());
        let menu = Ctx { menu_open: true, ..Ctx::default() };
        s.power_step(Step::Short, true, menu);
        assert_eq!((s.state(), s.dark_game()), (State::Light, false), "a paused game stays paused");
    }

    /// The Flip 2's lid as pocket-ui's pad reader passes it on (`pad::linux::switch`), in a stream with other input.
    fn lid(closed: bool) -> PadEvent {
        PadEvent::Lid { source: 3, closed }
    }

    #[test]
    fn closing_the_lid_does_what_the_power_buttons_short_press_does() {
        let (_dir, mut s, _) = test_sleep("lid-sleep");
        s.force_power_key = true;
        s.game_started(some_game());
        let stream = [PadEvent::Button { button: Button::South, pressed: true, source: 0 }, PadEvent::Button { button: Button::South, pressed: false, source: 0 }];
        for e in &stream {
            s.on_pad(e, Ctx::default());
        }
        assert_eq!(s.state(), State::Awake);
        assert!(s.on_pad(&lid(true), Ctx::default()), "the lid never reaches the UI");
        assert_eq!(s.state(), State::Deep, "Sleep, the default: saved and suspending");
        assert!(s.on_pad(&lid(false), Ctx::default()), "opened while asleep: the system's own wake brings it back");

        let (_dir, mut s, file) = test_sleep("lid-screen-off");
        s.force_power_key = true;
        with_buttons(&mut s, ShortPress::ScreenOff, Hold::Menu);
        s.game_started(some_game());
        s.on_pad(&lid(true), Ctx::default());
        assert_eq!((s.state(), s.dark_game()), (State::Light, true), "Screen off: dark, the game goes on");
        assert_eq!(level(&file), "0");
        s.on_pad(&lid(true), Ctx::default());
        assert_eq!(s.state(), State::Light, "a second close (a bounce) changes nothing");
        assert!(s.on_pad(&lid(false), Ctx::default()));
        assert_eq!((s.state(), s.dark_game()), (State::Awake, false), "opening it wakes the screen");
        s.screen_job.take().unwrap().join().unwrap();
        assert_eq!(level(&file), "120");
    }

    #[test]
    fn the_lid_puts_a_dark_screen_to_sleep_and_opening_it_wakes_one() {
        let (_dir, mut s, _) = test_sleep("lid-light");
        s.force_power_key = true;
        s.light(Ctx::default());
        assert_eq!(s.state(), State::Light);
        s.on_pad(&lid(false), Ctx::default());
        assert_eq!(s.state(), State::Awake, "opened: awake, as any button would");
        // Right after waking a press is swallowed (`WAKE_QUIET`); closing the lid is not a press: it still sleeps.
        assert!(s.woke_at.is_some());
        s.on_pad(&lid(true), Ctx::default());

        assert_eq!(s.state(), State::Deep);

        let (_dir, mut s, _) = test_sleep("lid-light-close");
        s.force_power_key = true;
        s.light(Ctx::default());
        s.on_pad(&lid(true), Ctx::default());
        assert_eq!(s.state(), State::Deep, "closed while dark: on into deep sleep");
    }

    #[test]
    fn without_the_inhibitor_the_lid_is_left_to_logind() {
        let (_dir, mut s, _) = test_sleep("lid-logind");
        assert!(!s.power_key_ours());
        s.game_started(some_game());
        assert!(s.on_pad(&lid(true), Ctx::default()), "still not the UI's");
        assert_eq!(s.state(), State::Awake, "logind suspends by itself (HandleLidSwitch=suspend); pocket-ui does nothing");
    }

    #[test]
    fn only_a_device_with_a_lid_inhibits_loginds_lid_handling() {
        assert_eq!(inhibit_what(false), "--what=handle-power-key", "the Mini V2's inhibitor, unchanged");
        assert_eq!(inhibit_what(true), "--what=handle-power-key:handle-lid-switch");
    }

    #[test]
    fn holding_opens_the_power_menu_by_default_or_sleeps_when_let_go() {
        let (_dir, mut s, _) = test_sleep("hold-menu");
        assert!(s.power_step(Step::Long, false, Ctx::default()));
        assert!(s.take_menu_request(), "the menu opens while the button is held");
        assert!(s.power_step(Step::LongEnd, true, Ctx::default()));
        assert!(!s.take_menu_request());
        assert_eq!(s.state(), State::Awake, "letting go does nothing more");

        let (_dir, mut s, _) = test_sleep("hold-sleep");
        with_buttons(&mut s, ShortPress::Sleep, Hold::Sleep);
        assert!(s.power_step(Step::Long, false, Ctx::default()));
        assert_eq!(s.state(), State::Awake, "nothing while it is still held");
        assert!(!s.take_menu_request(), "no menu");
        assert!(s.power_step(Step::LongEnd, true, Ctx::default()));
        assert_eq!(s.state(), State::Deep, "asleep when let go");

        // A long press the timer didn't catch, told at its release: the same.
        let (_dir, mut s, _) = test_sleep("hold-sleep-late");
        with_buttons(&mut s, ShortPress::ScreenOff, Hold::Sleep);
        s.power_step(Step::Long, true, Ctx::default());
        assert_eq!(s.state(), State::Deep);
    }

    #[test]
    fn sleep_after_never_stays_dark_but_a_failed_sleep_is_still_tried_again() {
        let t0 = Instant::now();
        let mut t = timers(t0);
        t.deep_after = None;


        t.light_since = Some(t0);
        assert_eq!(t.due(State::Light, t0 + Duration::from_secs(99_999), true), (None, None));
        t.failures = 1;
        assert_eq!(t.due(State::Light, t0, true).1, Some(t0 + RETRY_FIRST * 2));
    }

    #[test]
    fn new_times_count_from_the_change() {
        let (dir, mut s, _) = test_sleep("set-config");
        let mut c = s.config;
        c.screen_off = Some(30);
        c.sleep_after = None;
        s.set_config(c);
        assert_eq!(s.timers.light_after, Some(minute() / 2));
        assert_eq!(s.timers.deep_after, None);
        assert_eq!(Config::load(&dir), c, "kept in sleep.json");
    }

    #[test]
    fn restart_pocket_ui_saves_and_quits_the_game_first_and_never_runs_a_system_command() {
        let (dir, mut s, _) = test_sleep("restart-ui");
        s.sys.reboot_cmd = format!("echo reboot >> {}", dir.join("powered-off").display());
        // The game: a process group of its own that ends on SIGTERM (reparented, so nothing here has to reap it).
        let out = Command::new("sh").args(["-c", "setsid sleep 30 >/dev/null 2>&1 & echo $!"]).output().unwrap();
        let pgid: i32 = String::from_utf8_lossy(&out.stdout).trim().parse().unwrap();
        std::thread::sleep(Duration::from_millis(100));
        // The next start opens only a game whose file is there.
        std::fs::write(dir.join("Tetris.gb"), "rom").unwrap();
        s.game_started(Game { pgid: Some(pgid), path: dir.join("Tetris.gb"), ..some_game() });
        s.shut_down(PowerAction::RestartUi);
        assert_eq!(s.state(), State::PoweringOff);
        let end = Instant::now() + Duration::from_secs(10);
        let mut restart = false;
        while !restart && Instant::now() < end {
            std::thread::sleep(Duration::from_millis(20));
            s.tick(Ctx::default());
            restart = s.take_restart();
        }
        assert!(restart, "the app is told to end");
        // SAFETY: kill(2) with signal 0 only checks that the process group exists.
        assert_ne!(unsafe { libc::kill(-pgid, 0) }, 0, "the game was quit before");
        assert_eq!(take_resume(&dir).map(|r| r.name), Some("Tetris".into()), "the next start opens the game again");
        assert!(!dir.join("powered-off").exists(), "no reboot, no power off");
        assert_eq!(s.overlay().as_deref(), Some("Restarting pocket-ui\u{2026}"));
        assert!(!s.take_restart(), "once");
    }

    fn powered_off_now(dir: &Path) -> bool {
        std::thread::sleep(Duration::from_millis(200));
        dir.join("powered-off").exists()
    }

    #[test]
    fn the_low_battery_guard_still_switches_off_with_the_screen_on_and_only_once() {
        let (dir, mut s, file) = test_sleep("guard");
        s.light(Ctx::default());
        s.auto_off = Some(crate::auto_off::Plan::new(0.0, 3, HOUR));
        // Critical while a suspend is under way: nothing until it comes back.
        s.state = State::Deep;
        s.deep_seq = 1;
        s.tx.send(Msg::Battery(crate::low_battery::Alert::Critical(2))).unwrap();
        s.tick(Ctx::default());
        assert_eq!(s.state(), State::Deep);
        // The suspend failed: back in light sleep, the guard's power-off with the screen on ("Battery empty").
        s.tx.send(Msg::Woke { seq: 1, slept: false, hooks_us: None, volume_us: None }).unwrap();
        s.tick(Ctx::default());
        assert_eq!(s.state(), State::PoweringOff);
        s.screen_job.take().unwrap().join().unwrap();
        assert_eq!(level(&file), "120", "the guard shows what it does, as before");
        assert!(powered_off(&dir));
        // A switch-off after sleeping on top of it: nothing more.
        s.tx.send(Msg::SwitchOff { seq: 1, hours: 3 }).unwrap();
        s.tick(Ctx::default());
        assert_eq!(std::fs::read_to_string(dir.join("powered-off")).unwrap().lines().count(), 1);
        assert_eq!(crate::auto_off::take_note(&dir), None, "the guard's power-off leaves no switch-off note");
    }

    fn level(file: &Path) -> String {
        std::fs::read_to_string(file).unwrap().trim().to_string()
    }

    #[test]
    fn the_screen_comes_on_as_soon_as_the_system_is_back() {
        let (_dir, mut s, file) = test_sleep("resumed");
        let ctx = Ctx::default();
        s.light(ctx);
        assert_eq!((s.state(), level(&file).as_str()), (State::Light, "0"));
        assert!(!s.take_woke());
        // As `deep` leaves it while its worker suspends (the worker itself isn't started here).
        s.state = State::Deep;
        s.deep_seq = 7;
        // News of an earlier deep sleep (overtaken): nothing happens.
        s.tx.send(Msg::Resumed { seq: 6, at_us: 1 }).unwrap();
        s.tick(ctx);
        assert_eq!((s.state(), level(&file).as_str()), (State::Deep, "0"));
        // The system is back, ROCKNIX's wake hooks still run: awake now, screen on, the app draws a frame.
        s.tx.send(Msg::Resumed { seq: 7, at_us: crate::trace::now_us() }).unwrap();
        s.tick(ctx);
        assert_eq!(s.state(), State::Awake);
        s.screen_job.take().unwrap().join().unwrap();
        assert_eq!(level(&file), "120");
        assert!(s.take_woke() && !s.take_woke(), "once");
        // A hook turns the backlight off; then the worker ends: on again, still awake, no second wake.
        std::fs::write(&file, "0").unwrap();
        s.tx.send(Msg::Woke { seq: 7, slept: true, hooks_us: Some(crate::trace::now_us()), volume_us: None }).unwrap();
        s.tick(ctx);
        s.screen_job.take().unwrap().join().unwrap();
        assert_eq!((s.state(), level(&file).as_str()), (State::Awake, "120"));
        assert!(!s.take_woke());
    }

    #[test]
    fn a_failed_suspend_stays_dark_and_nothing_comes_on() {
        let (_dir, mut s, file) = test_sleep("failed");
        s.light(Ctx::default());
        s.state = State::Deep;
        s.deep_seq = 1;
        s.tx.send(Msg::Woke { seq: 1, slept: false, hooks_us: None, volume_us: None }).unwrap();
        s.tick(Ctx::default());
        assert_eq!((s.state(), level(&file).as_str()), (State::Light, "0"));
        assert!(s.screen_job.is_none() && !s.take_woke());
        s.wake_light("test");
        s.screen_job.take().unwrap().join().unwrap();
    }

    #[test]
    fn a_screen_off_after_a_screen_on_wins() {
        let dir = TestDir::new("sleep-gen");
        let file = dir.join("brightness");
        std::fs::write(&file, "0").unwrap();
        let sys = System::new(Some(dir.to_path_buf()), "false".into(), false);
        // The screen-on thread waits for the lock; meanwhile the device goes dark again (what `screen_off` does first).
        let turn = sys.screen_lock.lock().unwrap();
        let job = sys.screen_on_soon(Some(90)).unwrap();
        sys.screen_gen.fetch_add(1, Ordering::SeqCst);
        drop(turn);
        job.join().unwrap();
        assert_eq!(level(&file), "0", "it stays dark");
        // Without one, it comes on.
        sys.screen_on_soon(Some(90)).unwrap().join().unwrap();
        assert_eq!(level(&file), "90");
    }

    #[test]
    fn pocket_core_games_know_their_auto_state() {
        let g = Game::new("gbc", "uCity", Path::new("/storage/roms/gbc/uCity.gbc"), "pocket-core", Some(42));
        assert_eq!(
            g.kind,
            GameKind::PocketCore {
                auto_state: PathBuf::from("/storage/roms/savestates/gbc/uCity.pstate.auto")
            }
        );
        assert_eq!(Game::new("n64", "x", Path::new("/r/x.z64"), "retroarch", None).kind, GameKind::RetroArch);
    }
}
