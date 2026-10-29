//! System controls of pocket-ui: the volume keys and their combos, the on-screen display, the quick
//! settings panel (HOME on the shelf) and the time page of Settings.
//!
//! `Sys` owns all of it, so `main.rs` only forwards pad events (`pad`), a timer (`tick`, `next_wake`) and
//! painting (`paint`). The system itself is behind `system_ctl::Hw`; nothing here touches sysfs or runs a tool.
//!
//! Keys (pocket-ui sees the same events as ROCKNIX's `input_sense`, see `fn_keys.rs`):
//! - Vol+/Vol-: `input_sense` changes the volume, we draw the OSD from what it did. Without `input_sense`
//!   (virtual device) we change the volume ourselves.
//! - START + Vol: brightness. SELECT + Vol: colour temperature, the one way left to change it (the panel has no
//!   rows for temperature or contrast; `display.json` is still applied at start). HOME + Vol: ROCKNIX does brightness.
//! - HOME alone on the shelf: the quick settings. Their Performance and Fan rows are `quick_profiles`; "Power…" opens
//!   the power menu.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::audio_route::Sound;
use crate::control;
use crate::display::{Backend, Look};
use crate::keyboard::{Keyboard, Outcome};
use crate::net_state::{HwNet, NetState, Notice, Snapshot};
use crate::manual_time::Picker;
use crate::osd::{Data, Shown};
use crate::pad::{Button, PadEvent};
use crate::perf_fan::Prefs;
use crate::power_menu::{BatteryLine, Choice};
use crate::quick_profiles::{GameView, Profiles, Which};
use crate::touch::HitMap;
use crate::system_ctl::{ClockSet, NtpState, PowerAction, SharedHw, Worker, MIN_BRIGHTNESS, STEP};
use crate::timezone;

/// A and B as the Retroid prints them (see main.rs).
pub const CONFIRM: Button = Button::East;
pub const CANCEL: Button = Button::South;

const REPEAT_DELAY: Duration = Duration::from_millis(320);
const REPEAT_EVERY: Duration = Duration::from_millis(75);
/// How often the OSD looks at what `input_sense` did to the volume.
const OBSERVE_EVERY: Duration = Duration::from_millis(90);
const OBSERVE_AFTER_RELEASE: Duration = Duration::from_millis(450);
/// Brightness is stored (a shell script) once it hasn't moved for this long.
const PERSIST_AFTER: Duration = Duration::from_millis(700);
const REFRESH_EVERY: Duration = Duration::from_millis(2000);
/// How long after the start the first background look at the levels and radios runs (`Sys::tick`), so the panel has
/// them in memory when it first opens: after the first frames, never before them.
const WARM_AFTER: Duration = Duration::from_millis(1500);
/// How long the OSD of a step taken before its level was known waits for the worker's answer.
const PENDING_FOR: Duration = Duration::from_millis(1500);
pub const TOAST_FOR: Duration = Duration::from_millis(2200);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Game {
    None,
    /// A game of our own host: the OSD can be drawn into it.
    PocketCore,
    /// RetroArch or a standalone emulator.
    Other,
}

/// What `main.rs` knows and `Sys` needs.
#[derive(Clone, Copy, Debug)]
pub struct Ctx {
    /// The shelf (or a game list) is showing: no setup, no settings page, no pause menu, no game.
    pub shelf: bool,
    pub game: Game,
}

#[derive(Default, Debug, PartialEq, Eq)]
pub struct Out {
    /// Main must not treat this event as its own.
    pub consumed: bool,
    /// SELECT was used with a volume key: releasing it must not switch the look of the shelf.
    pub select_used: bool,
    /// A volume key went down and changes the volume (not brightness or colour): the UI ticks.
    pub volume_step: bool,
    /// The power menu was closed without choosing anything (B, or the power button again).
    pub power_cancelled: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Row {
    Brightness,
    Volume,
    Wifi,
    Bluetooth,
    /// Games: performance (`quick_profiles`).
    Performance,
    /// Fan outside games on the shelf, Games: fan over a game; not on a device without a fan.
    Fan,
    /// Opens the power menu (Sleep, Restart, Power off, Restart pocket-ui, Restart to boot menu), which comes back
    /// here on B.
    Power,
}

impl Row {
    /// Every row in panel order, as on a device with all of them (docs/user/settings.md, "Quick settings").
    pub const ALL: [Row; 7] = [Row::Brightness, Row::Volume, Row::Wifi, Row::Bluetooth, Row::Performance, Row::Fan, Row::Power];

    pub fn label(self) -> &'static str {
        match self {
            Row::Brightness => "Brightness",
            Row::Volume => "Volume",
            Row::Wifi => "Wi-Fi",
            Row::Bluetooth => "Bluetooth",
            Row::Performance => "Performance",
            Row::Fan => "Fan",
            Row::Power => "Power…",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum What {
    Volume,
    Brightness,
    Warmth,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Mode {
    /// We change it.
    Act(What),
    /// ROCKNIX changes it; we only show the result.
    Observe(What),
}

#[derive(Default, Clone, Debug)]
pub(crate) struct Polled {
    /// Goes up with every look; 0: nothing looked at yet (rows show a neutral "…", never a read on the UI thread).
    pub seq: u64,
    /// The backlight in percent (`Hw::brightness`), read with the rest.
    pub brightness: Option<u8>,
    /// `Hw::ram_booted` and `Hw::boot_menu_supported` (/proc/cmdline, sysfs): read once, by the first look.
    pub ram_booted: Option<bool>,
    pub boot_menu: Option<bool>,
    pub volume: Option<u8>,
    /// The output is muted (`Hw::muted`), read with the volume.
    pub muted: Option<bool>,
    /// Where the sound goes (`Hw::sound`), read with the volume: the Volume row's note. None: not looked at yet ("…");
    /// `Sound::Unknown`: asked, PipeWire didn't say.
    pub sound: Option<Sound>,
    /// The Wi-Fi radio as the shared picture has it (`net_state`): `None` where there is none.
    pub wifi: Option<bool>,
    /// A Wi-Fi switch that did not work, until the next one (said once, `Sys::tick`).
    pub wifi_notice: Option<Notice>,
    pub bluetooth: Option<bool>,
    pub ntp: Option<NtpState>,
    pub busy_wifi: bool,
    pub busy_bt: bool,
    pub busy_sync: bool,
    /// Date and time set by hand is being written (`Hw::set_clock` on the worker).
    pub busy_clock: bool,
    /// Its answer, taken once by `take_clock_done` (the toast, the page).
    pub clock_done: Option<(i64, Result<ClockSet, String>)>,
    pub power_error: Option<String>,
    /// The time page's facts, read by the worker (`poll_all` with `time`, `load_zones`): `/etc/localtime`, the zone's
    /// offset from UTC now, and the list of zones (a walk of the zoneinfo tree, once). None: not looked at yet; the
    /// page says "…" (review 2026-10-24: it read all of them on the UI thread, in every frame).
    pub timezone: Option<Option<String>>,
    pub zone_offset: Option<Option<i32>>,
    pub zones: Option<Arc<Vec<String>>>,
}

pub(crate) enum TimePage {
    Summary { sel: usize },
    Regions { sel: usize },
    Zones { heading: String, zones: Vec<String>, sel: usize },
    Search(Box<Keyboard>),
    /// Date and time, set by hand (`manual_time`).
    SetClock(Picker),
}

/// The Time page's rows, top to bottom: Time zone, Network time (1), Date and time.
pub(crate) const TIME_ZONE_ROW: usize = 0;
pub(crate) const BY_HAND_ROW: usize = 2;

/// The power menu (long press of the power button), over whatever was open.
pub(crate) struct PowerMenu {
    pub sel: usize,
    pub choices: Vec<Choice>,
    /// What was open when the menu came up; comes back when it closes.
    pub under: Box<Screen>,
    /// A game is saved first; a restart goes back to Android: the lines under the entries (`Choice::hint`).
    pub ctx: crate::power_menu::Context,
    /// The battery at the top, as it was when the menu opened (`power_menu::battery_now`, from memory).
    pub battery: BatteryLine,
}

pub(crate) enum Screen {
    Closed,
    Quick { sel: usize },
    Time(TimePage),
    Power(PowerMenu),
}

#[derive(Default, Debug, Clone, Copy)]
struct Mods {
    home: bool,
    start: bool,
    select: bool,
}

struct Observe {
    what: What,
    next: Instant,
    until: Instant,
    /// What was last shown, so an unchanged level does not redraw.
    last: Option<u32>,
}

pub struct Sys {
    pub(crate) hw: SharedHw,
    /// The Wi-Fi as every screen sees it (`net_state`): the Wi-Fi row reads it, the Settings page reads it too.
    net: Arc<NetState>,
    /// Keeps the subscription to `net` alive; it copies the radio into `polled` after every change.
    net_sub: Option<Arc<dyn Fn() + Send + Sync>>,
    /// The newest Wi-Fi switch failure already said.
    seen_notice: u64,
    /// A Wi-Fi switch did not work (what was asked for): the line under the rows says it while the panel stays open.
    pub(crate) wifi_failed: Option<Notice>,
    worker: Worker,
    data_dir: PathBuf,
    /// A test's scratch folder (its `data_dir`): removed with the `Sys`, also when the test panics.
    #[cfg(test)]
    scratch: Option<crate::testutil::TestDir>,
    inline: bool,
    pub(crate) screen: Screen,
    pub(crate) osd: Option<Shown>,
    pub(crate) look: Look,
    pub(crate) backend: Backend,
    #[cfg(target_os = "linux")]
    gamma: Option<crate::gamma::Gamma>,
    #[cfg(target_os = "linux")]
    gamma_touched: bool,
    /// `display.json` says `soft` (pocket-core reads it at its start, and tints only then): written once when the
    /// night light needs the Soft backend and the user never changed the look.
    soft_saved: bool,
    mods: Mods,
    home_tap: bool,
    rocknix_keys: bool,
    vol_held: Option<(Button, Instant, Mode)>,
    /// Vol+ and Vol- as down/up, so a key reported by two input devices counts once.
    vol_down: [bool; 2],
    held: Option<(Button, Instant)>,
    /// The quick settings were opened from the pause menu: drawn over the paused game, dark in both appearances.
    pub(crate) over_game: bool,
    observe: Option<Observe>,
    /// Levels we set ourselves, trusted for a moment (a read right after a write can lag).
    pub(crate) levels: Levels,
    persist_at: Option<Instant>,
    pub(crate) polled: Arc<Mutex<Polled>>,
    pub(crate) view: Polled,
    seen: u64,
    next_refresh: Option<Instant>,
    /// When the first look in the background is due (`WARM_AFTER` after the start); None once it ran.
    warm_at: Option<Instant>,
    /// A step of a level asked before the level was known: the worker took it, the OSD shows when its answer is in
    /// (given up after `PENDING_FOR`).
    pending_osd: Option<(What, Instant)>,
    pub(crate) toast: Option<(String, Instant)>,
    /// Power actions are only recorded (virtual device).
    pub(crate) power_is_mock: bool,
    pub(crate) message: Option<(String, Instant)>,
    /// The sleep module's entry points: Sleep and Power off go through them so the game is saved
    /// and the volume is repaired after waking. Without them, plain `systemctl`.
    sleep: Option<std::sync::Arc<dyn Fn() + Send + Sync>>,
    /// Where an OSD over a pocket-core game goes: pocket-core's command FIFO (`control::pocket_core`); true when it
    /// was taken. Tests record the lines instead.
    game_osd: std::sync::Arc<dyn Fn(&str) -> bool + Send + Sync>,
    /// An OSD pocket-core didn't take was reported in the log already (once per run of failures).
    osd_lost: bool,
    shutdown: Option<std::sync::Arc<dyn Fn(PowerAction) + Send + Sync>>,
    /// The power menu was closed by B or the power button (reported once through `Out`).
    power_cancelled: bool,
    /// The UTC offsets of the zones the time page lists, filled by the worker (`load_offsets`); painting only looks.
    pub(crate) offsets: Arc<Mutex<HashMap<String, Option<i32>>>>,
    pub(crate) now_unix: i64,
    /// A snapshot of the Time page with a made-up clock (`time-wrong`, `time-unsynced`): painted instead of the real
    /// time, never set in the running app.
    pub(crate) clock_fake: Option<i64>,
    /// The clock was set by hand since the start (Date and time): an offline Pocket's clock is then not "may be a
    /// little off" (`clock_check::check`). Memory only.
    pub(crate) clock_by_hand: bool,
    /// The local zone's offset from UTC at a moment (libc's, the chosen zone): what Date and time's picker shows and
    /// sets. Tests give a fixed one.
    pub(crate) offset_at: fn(i64) -> i32,
    /// The Performance and Fan rows: the choices, the device, the game under the panel (all in memory, set by the
    /// App: `set_profile_prefs`, `set_profile_game`).
    pub(crate) profiles: Profiles,
    /// A change of those rows the App has not taken yet (`take_profile_change`).
    profile_change: Option<Prefs>,
    /// Where a finger can tap on the quick settings (filled while painting them).
    pub hits: HitMap,
}

#[derive(Default, Clone, Copy)]
pub(crate) struct Levels {
    brightness: Option<(u8, Instant)>,
    volume: Option<(u8, Instant)>,
}

pub(crate) fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

impl Sys {
    /// `power_is_mock`: the power actions of `hw` are not real (virtual device).
    pub fn new(hw: SharedHw, data_dir: PathBuf, power_is_mock: bool) -> Sys {
        let mut look = Look::load(&data_dir);
        #[cfg(target_os = "linux")]
        let (gamma, backend) = if std::env::var_os("POCKETUI_MOCK").is_some() {
            (None, Backend::Soft)
        } else {
            match crate::gamma::Gamma::connect() {
                Ok(g) => {
                    let size = g.size();
                    println!("display look: gamma ramps of {size} entries");
                    (Some(g), Backend::Gamma(size))
                }
                Err(why) => {
                    println!("display look: software tint ({why})");
                    (None, Backend::Soft)
                }
            }
        };
        #[cfg(not(target_os = "linux"))]
        let backend = Backend::Soft;
        look.soft = backend == Backend::Soft;
        if !backend.can_contrast() {
            look.contrast = 0;
        }
        // The night light by the clock, in the very first look: no flash of the plain colours at night, no fade.
        look.night = crate::night_light::level_now(&crate::night_light::Prefs::load(&data_dir));
        let mut sys = Sys {
            net: NetState::new(Arc::new(HwNet::new(hw.clone()))),
            net_sub: None,
            seen_notice: 0,
            wifi_failed: None,
            hw,
            worker: Worker::new(),
            data_dir,
            #[cfg(test)]
            scratch: None,
            inline: false,
            screen: Screen::Closed,
            osd: None,
            look,
            backend,
            #[cfg(target_os = "linux")]
            gamma,
            #[cfg(target_os = "linux")]
            gamma_touched: false,
            soft_saved: false,
            mods: Mods::default(),
            home_tap: false,
            rocknix_keys: crate::fn_keys::rocknix_handles_volume(std::path::Path::new("/")),
            vol_held: None,
            vol_down: [false; 2],
            held: None,
            over_game: false,
            observe: None,
            levels: Levels::default(),
            persist_at: None,
            polled: Arc::new(Mutex::new(Polled::default())),
            view: Polled::default(),
            seen: 0,
            next_refresh: None,
            warm_at: Some(Instant::now() + WARM_AFTER),
            pending_osd: None,
            toast: None,
            power_is_mock,
            message: None,
            sleep: None,
            game_osd: std::sync::Arc::new(|line: &str| control::pocket_core(line)),
            osd_lost: false,
            shutdown: None,
            power_cancelled: false,
            offsets: Arc::new(Mutex::new(HashMap::new())),
            now_unix: unix_now(),
            clock_fake: None,
            clock_by_hand: false,
            offset_at: crate::appearance::utc_offset_now,
            profiles: Profiles::default(),
            profile_change: None,
            hits: HitMap::default(),
        };
        // Applied, not saved: nothing changed, and the atomic write (fsync on the card) cost ~35 ms of the first frame.
        sys.apply_look(Game::None);
        sys.save_soft_for_night();
        sys.wire_net();
        sys
    }

    /// The picture of the Wi-Fi this panel and Settings → Wi-Fi share (`net_state`).
    pub fn net(&self) -> Arc<NetState> {
        self.net.clone()
    }

    /// On another picture of the Wi-Fi (tests: a stand-in system).
    pub fn with_net(mut self, net: Arc<NetState>) -> Sys {
        self.net = net;
        self.wire_net();
        self
    }

    /// After every change of the shared picture the Wi-Fi row's facts are copied into `polled` (under its lock, so a
    /// later copy is always of a later picture), and the next `tick` shows them.
    fn wire_net(&mut self) {
        let (net, polled) = (self.net.clone(), self.polled.clone());
        self.seen_notice = net.snapshot().notice.map_or(0, |n| n.id);
        let sub: Arc<dyn Fn() + Send + Sync> = Arc::new(move || apply_net(&net, &polled));
        self.net.subscribe(&sub);
        self.net_sub = Some(sub);
    }

    /// For tests: jobs run on the calling thread, and key handling is ours (no `input_sense`).
    pub fn for_test(hw: SharedHw, data_dir: PathBuf) -> Sys {
        let mut sys = Sys::new(hw.clone(), data_dir, true);
        sys.inline = true;
        // The Wi-Fi's worker runs where it is asked, too.
        sys.net = NetState::inline(Arc::new(HwNet::new(hw)));
        sys.wire_net();
        // Tests open the panel themselves; the look at the start is tested on its own.
        sys.warm_at = None;
        sys.rocknix_keys = false;
        sys.backend = Backend::Soft;
        sys.game_osd = std::sync::Arc::new(|_: &str| false);
        sys
    }

    /// `input_sense` handles Vol+/Vol-, so we only show what it did.
    pub fn set_rocknix_keys(&mut self, on: bool) {
        self.rocknix_keys = on;
    }

    /// The sleep module's entry point; without one Sleep calls `systemctl suspend`.
    pub fn set_sleep_handler(&mut self, f: impl Fn() + Send + Sync + 'static) {
        self.sleep = Some(std::sync::Arc::new(f));
    }

    /// Power off and restart (also to the boot menu) through the sleep module: it saves the running game first.
    pub fn set_shutdown_handler(&mut self, f: impl Fn(PowerAction) + Send + Sync + 'static) {
        self.shutdown = Some(std::sync::Arc::new(f));
    }

    /// The system controls this panel uses, for the first start's time zone step.
    pub fn shared_hw(&self) -> SharedHw {
        self.hw.clone()
    }

    pub fn backend(&self) -> Backend {
        self.backend
    }

    pub fn look(&self) -> Look {
        self.look
    }

    fn spawn(&self, key: &'static str, job: impl FnOnce() + Send + 'static) {
        if self.inline {
            job();
        } else {
            self.worker.submit(key, job);
        }
    }

    /// The quick settings panel is up (not the time page).
    pub fn quick_open(&self) -> bool {
        matches!(self.screen, Screen::Quick { .. })
    }

    /// Changes whenever what the panels show moves (selection, page, typed text): for the UI sounds.
    pub fn ui_mark(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        match &self.screen {
            Screen::Closed => 0u8.hash(&mut h),
            Screen::Quick { sel } => (1u8, sel, self.profiles.prefs).hash(&mut h),
            Screen::Time(TimePage::Summary { sel }) => (2u8, sel).hash(&mut h),
            Screen::Time(TimePage::Regions { sel }) => (3u8, sel).hash(&mut h),
            Screen::Time(TimePage::Zones { heading, sel, .. }) => (4u8, heading, sel).hash(&mut h),
            Screen::Time(TimePage::Search(kb)) => (5u8, &kb.text, kb.row, kb.col, kb.shift, kb.symbols).hash(&mut h),
            Screen::Time(TimePage::SetClock(p)) => (7u8, p).hash(&mut h),
            Screen::Power(m) => (6u8, m.sel).hash(&mut h),
        }
        (self.view.wifi, self.view.bluetooth, self.look.warmth, self.look.contrast).hash(&mut h);
        h.finish()
    }

    pub fn is_open(&self) -> bool {
        !matches!(self.screen, Screen::Closed)
    }

    /// Anything that needs frames: an OSD, a toast or a message, a key repeating.
    pub fn animating(&self, now: Instant) -> bool {
        self.osd.is_some()
            || self.toast.as_ref().is_some_and(|(_, t)| now < *t + TOAST_FOR)
            || self.message.is_some()
            || self.vol_held.is_some()
            || self.held.is_some()
            || self.observe.is_some()
            || self.pending_osd.is_some()
    }

    /// The next moment `tick` has something to do, for the event loop's timer.
    pub fn next_wake(&self) -> Option<Instant> {
        let mut v: Vec<Instant> = Vec::new();
        v.extend(self.vol_held.map(|(_, t, _)| t));
        v.extend(self.held.map(|(_, t)| t));
        v.extend(self.observe.as_ref().map(|o| o.next));
        v.extend(self.persist_at);
        v.extend(self.osd.as_ref().map(Shown::expires));
        v.extend(self.toast.as_ref().map(|(_, t)| *t + TOAST_FOR));
        v.extend(self.message.as_ref().map(|(_, t)| *t));
        if self.is_open() {
            v.extend(self.next_refresh);
        }
        v.extend(self.warm_at);
        v.into_iter().min()
    }

    // ---- display look -------------------------------------------------------------------------------

    /// Sends the look to wherever it is applied: the compositor, the file pocket-core reads, a running pocket-core.
    fn push_look(&mut self, game: Game) {
        self.apply_look(game);
        self.look.save(&self.data_dir);
    }

    /// `push_look` without saving it.
    fn apply_look(&mut self, game: Game) {
        self.look.soft = self.backend == Backend::Soft;
        // The compositor's table is only taken over once there is something to show (and kept up to date afterwards).
        #[cfg(target_os = "linux")]
        if let Some(g) = &self.gamma {
            if !self.look.is_neutral() || self.gamma_touched {
                g.apply(&self.look);
                self.gamma_touched = true;
            }
        }
        if game == Game::PocketCore && self.look.soft {
            control::pocket_core(&format!("look {} {} {}", self.look.warmth, self.look.contrast, self.look.night));
        }
    }

    /// A step of the night light's fade (or its first, or its last), from its thread: the white point to show now, in
    /// kelvin, 0 = off. Goes through the same `apply_look` as the temperature keys, so the compositor's ramps, the
    /// shelf's tint and a running pocket-core game's tint are worked out from one `Look` (`display.rs`). Returns
    /// whether pocket-ui's own frame needs painting again (only the Soft backend tints it).
    pub fn set_night(&mut self, level: u16, game: Game) -> bool {
        if level == self.look.night {
            return false;
        }
        self.look.night = level;
        self.apply_look(game);
        self.save_soft_for_night();
        self.look.soft
    }

    /// pocket-core starts after a night light that needs the Soft backend: its tint is drawn by pocket-core, which
    /// learns that from `display.json`. Written once, on the worker, and only if the file does not say it yet.
    fn save_soft_for_night(&mut self) {
        if self.look.night == 0 || !self.look.soft || self.soft_saved {
            return;
        }
        self.soft_saved = true;
        let (look, dir) = (self.look, self.data_dir.clone());
        self.spawn("look-soft", move || {
            if !Look::load(&dir).soft {
                look.save(&dir);
            }
        });
    }

    pub fn warmth_data(&self) -> Data {
        let r = crate::display::RANGE as f32;
        Data::new("Colour temperature", self.look.warmth_label(), (self.look.warmth as f32 + r) / (2.0 * r), true)
    }

    // ---- levels --------------------------------------------------------------------------------------

    /// The backlight as last set or seen: what we set a moment ago, else the last background look (`Polled`).
    /// None before the first look; never a read here (the UI thread).
    pub(crate) fn brightness_now(&self, now: Instant) -> Option<u8> {
        match self.levels.brightness {
            Some((v, at)) if now < at + Duration::from_millis(1500) => Some(v),
            _ => self.view.brightness,
        }
    }

    /// The volume, the same way as `brightness_now`.
    pub(crate) fn volume_now(&self, now: Instant) -> Option<u8> {
        match self.levels.volume {
            Some((v, at)) if now < at + Duration::from_millis(1500) => Some(v),
            _ => self.view.volume,
        }
    }

    /// One step of the backlight; the new level, or None when it wasn't known yet: then the worker reads it, steps
    /// and reports (`pending_osd`), and the UI thread never waits for the read.
    fn step_brightness(&mut self, dir: i32, now: Instant) -> Option<u8> {
        let step = move |v: u8| (v as i32 + dir * STEP as i32).clamp(MIN_BRIGHTNESS as i32, 100) as u8;
        self.persist_at = Some(now + PERSIST_AFTER);
        let Some(v) = self.brightness_now(now) else {
            let (hw, polled) = (self.hw.clone(), self.polled.clone());
            self.spawn("brightness-unknown", move || {
                let next = step(hw.brightness().unwrap_or(50));
                hw.set_brightness(next);
                let mut p = polled.lock().unwrap();
                p.brightness = Some(next);
                p.seq += 1;
            });
            return None;
        };
        let next = step(v);
        self.hw.set_brightness(next);
        self.levels.brightness = Some((next, now));
        Some(next)
    }

    /// One step of the volume (on the worker: `volume` is a process); None as for `step_brightness`.
    fn step_volume(&mut self, dir: i32, now: Instant) -> Option<u8> {
        let step = move |v: u8| (v as i32 + dir * STEP as i32).clamp(0, 100) as u8;
        let (hw, polled) = (self.hw.clone(), self.polled.clone());
        let Some(v) = self.volume_now(now) else {
            self.spawn("volume", move || {
                let next = step(hw.volume().unwrap_or(50));
                hw.set_volume(next);
                let mut p = polled.lock().unwrap();
                p.volume = Some(next);
                p.seq += 1;
            });
            return None;
        };
        let next = step(v);
        self.levels.volume = Some((next, now));
        self.spawn("volume", move || hw.set_volume(next));
        Some(next)
    }

    fn step_warmth(&mut self, dir: i32, game: Game) -> bool {
        let r = crate::display::RANGE;
        let next = (self.look.warmth + dir as i8).clamp(-r, r);
        let changed = next != self.look.warmth;
        self.look.warmth = next;
        if changed {
            self.push_look(game);
        }
        changed
    }

    /// Shows an OSD: on the shelf in our own pill, over a pocket-core game through its overlay.
    ///
    /// With no game of ours known, a pocket-core that is running anyway still gets it: one pocket-ui didn't start
    /// or lost track of (pocket-ui restarted under a running game, a game started over SSH). Its window covers the
    /// shelf, so the pill drawn here alone would never be seen while the level visibly changes. The FIFO refuses
    /// at once when no pocket-core reads it, so on the shelf this costs one failed `open`.
    fn show(&mut self, d: Data, ctx: &Ctx, now: Instant) {
        let _ = now;
        let line = format!("osd {}", d.to_wire());
        match ctx.game {
            Game::None => {
                (self.game_osd)(&line);
                self.osd = Some(Shown::update(self.osd.as_ref(), d));
            }
            Game::PocketCore => {
                let taken = (self.game_osd)(&line);
                if !taken && !self.osd_lost {
                    println!("osd: pocket-core didn't take \"{line}\" (its command FIFO is not open)");
                }
                self.osd_lost = !taken;
            }
            Game::Other => {}
        }
    }

    /// For tests: where OSD lines for pocket-core go instead of its FIFO.
    pub fn set_game_osd(&mut self, f: impl Fn(&str) -> bool + Send + Sync + 'static) {
        self.game_osd = std::sync::Arc::new(f);
    }

    fn data_for(&self, what: What, now: Instant) -> Data {
        match what {
            What::Volume => Data::percent("Volume", self.volume_now(now).unwrap_or(50)),
            What::Brightness => Data::percent("Brightness", self.brightness_now(now).unwrap_or(50)),
            What::Warmth => self.warmth_data(),
        }
    }

    // ---- volume keys -------------------------------------------------------------------------------------

    fn mode_for_volume_key(&self) -> Mode {
        if self.mods.start {
            Mode::Act(What::Brightness)
        } else if self.mods.select {
            Mode::Act(What::Warmth)
        } else if self.mods.home {
            if self.rocknix_keys {
                Mode::Observe(What::Brightness)
            } else {
                Mode::Act(What::Brightness)
            }
        } else if self.rocknix_keys {
            Mode::Observe(What::Volume)
        } else {
            Mode::Act(What::Volume)
        }
    }

    fn vol_key(&mut self, b: Button, pressed: bool, ctx: &Ctx, now: Instant) -> Out {
        let out = Out {
            consumed: true,
            select_used: self.mods.select,
            volume_step: false,
            ..Out::default()
        };
        // The same key from a second device (ROCKNIX's virtual keyboard may repeat it): ignored.
        let slot = &mut self.vol_down[(b == Button::VolDown) as usize];
        if std::mem::replace(slot, pressed) == pressed {
            return out;
        }
        if !pressed {
            if self.vol_held.is_some_and(|(h, _, _)| h == b) {
                self.vol_held = None;
            }
            return out;
        }
        self.home_tap = false;
        let mode = self.mode_for_volume_key();
        self.apply_mode(mode, if b == Button::VolUp { 1 } else { -1 }, ctx, now, true);
        self.vol_held = Some((b, now + REPEAT_DELAY, mode));
        Out {
            volume_step: matches!(mode, Mode::Act(What::Volume) | Mode::Observe(What::Volume)),
            ..out
        }
    }

    /// One step (or one look) of `mode`. `first`: the key just went down.
    fn apply_mode(&mut self, mode: Mode, dir: i32, ctx: &Ctx, now: Instant, first: bool) {
        match mode {
            Mode::Act(What::Volume) => match self.step_volume(dir, now) {
                Some(v) => self.show(Data::percent("Volume", v), ctx, now),
                None => self.pending_osd = Some((What::Volume, now + PENDING_FOR)),
            },
            Mode::Act(What::Brightness) => match self.step_brightness(dir, now) {
                Some(v) => self.show(Data::percent("Brightness", v), ctx, now),
                None => self.pending_osd = Some((What::Brightness, now + PENDING_FOR)),
            },
            Mode::Act(What::Warmth) => {
                self.step_warmth(dir, ctx.game);
                let d = self.warmth_data();
                self.show(d, ctx, now);
            }
            Mode::Observe(what) => {
                if first {
                    self.observe = Some(Observe {
                        what,
                        next: now + OBSERVE_EVERY,
                        until: now + OBSERVE_AFTER_RELEASE,
                        last: None,
                    });
                    // ROCKNIX acts when the key goes down; look right away and then every 90 ms.
                    self.poll_now(what);
                }
            }
        }
    }

    /// Reads what ROCKNIX did and shows it if it changed.
    fn poll_now(&mut self, what: What) {
        match what {
            // A file in sysfs, but read on the worker like the volume: the key handler never reads.
            What::Brightness => {
                let hw = self.hw.clone();
                let polled = self.polled.clone();
                self.spawn("poll-bright", move || {
                    let v = hw.brightness();
                    let mut p = polled.lock().unwrap();
                    p.brightness = v;
                    p.seq += 1;
                });
            }
            What::Volume => {
                let hw = self.hw.clone();
                let polled = self.polled.clone();
                self.spawn("poll-vol", move || {
                    let (v, muted) = (hw.volume(), hw.muted());
                    let mut p = polled.lock().unwrap();
                    p.volume = v;
                    p.muted = muted;
                    p.seq += 1;
                });
            }
            What::Warmth => {}
        }
    }

    fn show_if_new(&mut self, what: What, value: u32, ctx: &Ctx, now: Instant) {
        if let Some(o) = self.observe.as_mut() {
            if o.last == Some(value) {
                return;
            }
            o.last = Some(value);
        }
        let d = self.data_for(what, now);
        self.show(d, ctx, now);
    }

    // ---- events ----------------------------------------------------------------------------------------------

    /// Feed every pad event here first. Releases always pass through to the shelf.
    pub fn pad(&mut self, pe: &PadEvent, ctx: &Ctx, now: Instant) -> Out {
        let PadEvent::Button { button, pressed, .. } = *pe else {
            return Out::default();
        };
        match button {
            Button::Mode => self.mods.home = pressed,
            Button::Start => self.mods.start = pressed,
            Button::Select => self.mods.select = pressed,
            _ => {}
        }
        if matches!(button, Button::VolUp | Button::VolDown) {
            return self.vol_key(button, pressed, ctx, now);
        }
        if !pressed {
            if self.held.is_some_and(|(h, _)| h == button) {
                self.held = None;
            }
            // A HOME tap on the shelf opens the quick settings.
            if button == Button::Mode && std::mem::take(&mut self.home_tap) && ctx.shelf && ctx.game == Game::None && !self.is_open() {
                self.profiles.game = None;
                self.open_quick(now);
                return Out {
                    consumed: true,
                    ..Out::default()
                };
            }
            return Out::default();
        }
        if !self.is_open() {
            // HOME followed by another button is a combo (Vol, or an emulator shortcut), not a tap.
            self.home_tap = if button == Button::Mode { true } else { false };
            return Out::default();
        }
        self.press(button, ctx, now);
        Out {
            consumed: true,
            power_cancelled: std::mem::take(&mut self.power_cancelled),
            ..Out::default()
        }
    }

    /// Opens the panel. Nothing is read here: the device's rows were found on a thread at the start
    /// (`quick_profiles::caps`), the choices and the game are in memory.
    fn open_quick(&mut self, now: Instant) {
        if let Some(caps) = crate::quick_profiles::caps() {
            (self.profiles.cpufreq, self.profiles.fan) = (caps.cpufreq, caps.fan);
        }
        self.screen = Screen::Quick { sel: 0 };
        self.over_game = false;
        self.refresh(now);
        // What the look at the start (or the last open) found: shown at once, the refresh corrects it.
        self.take_polled();
    }

    /// Copies the background's newest answer into `view` (memory only); true when there was a new one.
    fn take_polled(&mut self) -> bool {
        let p = self.polled.lock().unwrap().clone();
        if p.seq == self.seen {
            return false;
        }
        self.seen = p.seq;
        self.view = p;
        true
    }

    /// "Quick settings" in the pause menu: the same panel over the paused game (the caller paints the game behind
    /// it), so a brightness change shows on the game itself. B or HOME closes it, back to the pause menu. `game`:
    /// what the Performance and Fan rows need to know about it (`quick_profiles::GameView`).
    pub fn open_quick_over_game(&mut self, now: Instant, game: GameView) {
        self.profiles.game = Some(game);
        self.open_quick(now);
        self.over_game = true;
    }

    /// The quick settings are up over a paused game (`open_quick_over_game`), also under the power menu opened from
    /// their "Power…" row.
    pub fn quick_over_game(&self) -> bool {
        self.over_game && (self.quick_open() || matches!(&self.screen, Screen::Power(m) if matches!(*m.under, Screen::Quick { .. })))
    }

    /// Settings → Playing → Performance & fan as the App has them now (in memory); the rows show them.
    pub fn set_profile_prefs(&mut self, prefs: Prefs) {
        self.profiles.prefs = prefs;
    }

    /// The choices after a change on the Performance or Fan row, once: the App saves and applies them.
    pub fn take_profile_change(&mut self) -> Option<Prefs> {
        self.profile_change.take()
    }

    /// The game under the panel, as the rows see it (None on the shelf).
    pub fn profile_game(&self) -> Option<GameView> {
        self.profiles.game
    }

    /// Sets the game under the panel (snapshots; `open_quick_over_game` does it for the real panel).
    pub fn set_profile_game(&mut self, game: Option<GameView>) {
        self.profiles.game = game;
    }

    /// The device's rows, for tests and snapshots (normally `quick_profiles::caps`).
    pub fn set_profile_caps(&mut self, cpufreq: bool, fan: bool) {
        (self.profiles.cpufreq, self.profiles.fan) = (cpufreq, fan);
    }

    /// Opens the time page (Settings > Time).
    /// Opens at once with what is known (the rows say "…" for the rest); the zone list, the zone and its offset come
    /// from the worker (review 2026-10-24: they were read here, and again in every frame).
    pub fn open_time(&mut self, now: Instant) {
        self.want_offsets(&[]);
        self.screen = Screen::Time(TimePage::Summary { sel: 0 });
        if self.polled.lock().unwrap().zones.is_none() {
            let (hw, polled) = (self.hw.clone(), self.polled.clone());
            self.spawn("zones", move || load_zones(&*hw, &polled));
        }
        self.refresh(now);
    }

    /// Has the worker read the UTC offsets of `list` (the zones a page lists; none: just forget the old ones)?
    /// Painting shows nothing in their place until they are in.
    fn want_offsets(&mut self, list: &[String]) {
        self.now_unix = unix_now();
        self.offsets.lock().unwrap().clear();
        if list.is_empty() {
            return;
        }
        let (hw, polled, offsets, at, list) = (self.hw.clone(), self.polled.clone(), self.offsets.clone(), self.now_unix, list.to_vec());
        self.spawn("offsets", move || load_offsets(&*hw, &polled, &offsets, &list, at));
    }

    /// The zone list as far as the worker has read it; empty before.
    pub(crate) fn zone_list(&self) -> &[String] {
        self.view.zones.as_deref().map_or(&[], |z| z.as_slice())
    }

    /// `demo`: what the worker would have found, read here at once (never so in the running app).
    fn time_now(&mut self, list: &[String]) {
        load_zones(&*self.hw, &self.polled);
        poll_all(&*self.hw, &self.net, &self.polled, true);
        self.now_unix = unix_now();
        if !list.is_empty() {
            load_offsets(&*self.hw, &self.polled, &self.offsets, list, self.now_unix);
        }
        self.take_polled();
    }

    /// Puts the screen into a state for `pocket-ui --snapshot sys:<name>` (desktop check of the layouts):
    /// `quick[:row]`, `quick-game[:row]` (opened from a pause menu; the App sets the game, `set_profile_game`),
    /// `quick-muted` (Volume selected, the output muted at 5 %),
    /// `quick-headphones`, `quick-bluetooth`, `quick-bluetooth-long`, `quick-unknown` (Volume selected, the sound
    /// going there; the last: PipeWire didn't say),
    /// `power-off`, `restart` (the power menu from the quick settings' "Power…", on that entry), `power[:row]`,
    /// `power-noboot[:row]`, `time[:row]`, `time-wrong` (the clock at 3 March 2000, offline), `time-wrong-wifi` (the same with Wi-Fi on),
    /// `time-unsynced` (Wi-Fi off, no server yet), `time-set[:field]` (Date and time's picker, offline, on that field),
    /// `time-set-wrong` (the picker opened from the clock of 2000), `time-hand[:row]` (after a set by hand, offline),
    /// `regions`, `zones`, `search`, `osd-volume`, `osd-brightness`, `osd-warmth`.
    pub fn demo(&mut self, what: &str, now: Instant) {
        let (name, arg) = what.split_once(':').map_or((what, 0usize), |(n, a)| (n, a.parse().unwrap_or(0)));
        // A snapshot: the look runs here, so the picture has the values (never so in the running app).
        poll_all(&*self.hw, &self.net, &self.polled, false);
        self.view = self.polled.lock().unwrap().clone();
        match name {
            "quick" => {
                self.profiles.game = None;
                self.screen = Screen::Quick { sel: arg };
            }
            "quick-game" => {
                self.screen = Screen::Quick { sel: arg };
                self.over_game = true;
            }
            // The output muted at 5 % (the device's sink can be muted with a level left on it).
            "quick-muted" => {
                self.profiles.game = None;
                self.view.volume = Some(5);
                self.view.muted = Some(true);
                self.screen = Screen::Quick { sel: 1 };
            }
            // The sound goes elsewhere than the speaker, or PipeWire didn't say: the Volume row's note.
            "quick-headphones" | "quick-bluetooth" | "quick-bluetooth-long" | "quick-unknown" => {
                self.profiles.game = None;
                self.view.sound = Some(match name {
                    "quick-headphones" => Sound::Headphones,
                    "quick-bluetooth" => Sound::Bluetooth(Some("Soundcore Q30".into())),
                    "quick-bluetooth-long" => Sound::Bluetooth(Some("WH-1000XM4 Wireless Headset".into())),
                    _ => Sound::Unknown,
                });
                self.screen = Screen::Quick { sel: 1 };
            }
            // Wi-Fi off while the wish says on (the boot hooks blocked the radio): the row says Off, the description says what A
            // does; "-failed": A was pressed and the switch did not work, the toast says so.
            "quick-wifi-off" | "quick-wifi-failed" | "quick-wifi-save-failed" => {
                self.profiles.game = None;
                let snap = match name {
                    "quick-wifi-off" => crate::net_state::demo::off(),
                    "quick-wifi-failed" => crate::net_state::demo::switch_failed(),
                    _ => crate::net_state::demo::save_failed(),
                };
                self.net = NetState::demo(snap.clone());
                let mut p = self.polled.lock().unwrap().clone();
                copy_net(&mut p, &snap);
                self.view = p;
                if name != "quick-wifi-off" {
                    self.wifi_failed = snap.notice.clone();
                }
                self.screen = Screen::Quick { sel: self.sel_of(Row::Wifi) };
            }
            "power-off" | "restart" | "restart-ui" => {
                let choices = crate::power_menu::choices(true);
                let want = match name {
                    "restart" => Choice::Restart,
                    "restart-ui" => Choice::RestartUi,
                    _ => Choice::PowerOff,
                };
                let sel = self.sel_of(Row::Power);
                self.screen = Screen::Power(PowerMenu {
                    sel: choices.iter().position(|c| *c == want).unwrap_or(0),
                    choices,
                    under: Box::new(Screen::Quick { sel }),
                    ctx: Default::default(),
                    battery: demo_battery(),
                });
            }
            // `power-game`: opened in a game (the lines say the game is saved); `power-nobattery`: no level read;
            // `power-loading`: the battery history not read yet; `power-charging`.
            "power" | "power-noboot" | "power-game" | "power-nobattery" | "power-loading" | "power-charging" => {
                use crate::battery_estimate::Estimate;
                let battery = match name {
                    "power-nobattery" => crate::power_menu::battery_line(None, true, None),
                    "power-loading" => crate::power_menu::battery_line(Some(64), false, None),
                    "power-charging" => crate::power_menu::battery_line(Some(41), true, Some(&Estimate::Charging { full_in: Some(80 * 60), per_hour: 45.0 })),
                    _ => demo_battery(),
                };
                self.screen = Screen::Power(PowerMenu {
                    sel: arg,
                    choices: crate::power_menu::choices(name != "power-noboot"),
                    under: Box::new(Screen::Closed),
                    ctx: crate::power_menu::Context { game: name == "power-game", ram_booted: false },
                    battery,
                })
            }
            "time" | "time-wrong" | "time-wrong-wifi" | "time-unsynced" | "time-set" | "time-set-wrong" | "time-hand" | "regions" | "zones"
            | "search" => {
                self.open_time(now);
                self.time_now(&[]);
                // The made-up clocks: 3 March 2000 (a dead RTC), and 26 October 2026 13:05 UTC (a plausible one).
                const DEAD_RTC: i64 = 952_077_300;
                const PLAUSIBLE: i64 = 1_793_019_900;
                // Offline: Wi-Fi off, no time server has answered (the first start without Wi-Fi).
                let offline = |s: &mut Sys| {
                    s.view.wifi = Some(false);
                    s.view.ntp = Some(NtpState::Waiting);
                };
                match name {
                    "time-wrong" | "time-set-wrong" => {
                        self.clock_fake = Some(DEAD_RTC);
                        offline(self);
                    }
                    "time-wrong-wifi" => {
                        self.clock_fake = Some(DEAD_RTC);
                        self.view.ntp = Some(NtpState::Waiting);
                    }
                    "time-unsynced" => offline(self),
                    "time-set" => {
                        self.clock_fake = Some(PLAUSIBLE);
                        offline(self);
                    }
                    // After A on the picker: set by hand, still offline.
                    "time-hand" => {
                        self.clock_fake = Some(PLAUSIBLE);
                        self.clock_by_hand = true;
                        offline(self);
                    }
                    _ => {}
                }
                let current = self.view.timezone.clone().flatten().unwrap_or_default();
                self.screen = Screen::Time(match name {
                    "regions" => TimePage::Regions { sel: arg.max(1) },
                    "zones" => {
                        let region = timezone::region_of(&current).to_string();
                        let list: Vec<String> = timezone::zones_in(self.zone_list(), &region).into_iter().map(String::from).collect();
                        let at = list.iter().position(|z| *z == current).unwrap_or(0);
                        self.time_now(&list);
                        TimePage::Zones {
                            heading: region,
                            zones: list,
                            sel: at,
                        }
                    }
                    "search" => TimePage::Search(Box::new(Keyboard {
                        text: "vie".into(),
                        ..Default::default()
                    })),
                    "time-set" | "time-set-wrong" => {
                        let shown = self.clock_fake.unwrap_or_else(unix_now);
                        let mut p = Picker::start(shown, crate::clock_check::built_unix(), self.offset_at);
                        p.sel = arg.min(crate::manual_time::FIELDS.len() - 1);
                        TimePage::SetClock(p)
                    }
                    "time-hand" => TimePage::Summary { sel: if what.contains(':') { arg } else { BY_HAND_ROW } },
                    _ => TimePage::Summary { sel: arg },
                });
            }
            "osd-volume" => self.osd = Some(Shown::new(Data::percent("Volume", 65))),
            "osd-brightness" => self.osd = Some(Shown::new(Data::percent("Brightness", 40))),
            "osd-warmth" => {
                self.look.warmth = 4;
                self.osd = Some(Shown::new(self.warmth_data()));
            }
            _ => {}
        }
    }

    /// The power menu: Sleep, Restart, Power off, Restart pocket-ui and, where the device can do it, Restart to boot
    /// menu. It comes up over whatever is open (the quick settings, the time page) and gives it back when closed.
    pub fn open_power(&mut self, now: Instant) {
        self.open_power_in(now, false);
    }

    /// `open_power`, with a game running under it (`game`; also when the quick settings it came from are over one):
    /// the lines under the entries say the game is saved first.
    pub fn open_power_in(&mut self, _now: Instant, game: bool) {
        if self.power_menu_open() {
            return;
        }
        // Known from the background's first look; until then (the first moments) without the boot menu.
        self.take_polled();
        let choices = crate::power_menu::choices(self.view.boot_menu == Some(true));
        let ctx = crate::power_menu::Context { game: game || self.over_game, ram_booted: self.ram_booted() };
        let under = Box::new(std::mem::replace(&mut self.screen, Screen::Closed));
        self.held = None;
        self.screen = Screen::Power(PowerMenu { sel: 0, choices, under, ctx, battery: crate::power_menu::battery_now() });
    }

    pub fn power_menu_open(&self) -> bool {
        matches!(self.screen, Screen::Power(_))
    }

    /// The open power menu (for painting).
    pub(crate) fn power_menu(&self) -> Option<&PowerMenu> {
        match &self.screen {
            Screen::Power(m) => Some(m),
            _ => None,
        }
    }

    fn power_press(&mut self, b: Button, now: Instant) {
        let Screen::Power(m) = &mut self.screen else { return };
        let n = m.choices.len();
        match b {
            Button::Up => m.sel = (m.sel + n - 1) % n,
            Button::Down => m.sel = (m.sel + 1) % n,
            // The power button again, like B, closes the menu.
            b if b == CANCEL || b == Button::Power => {
                let Screen::Power(m) = std::mem::replace(&mut self.screen, Screen::Closed) else { return };
                self.screen = *m.under;
                self.held = None;
                self.power_cancelled = true;
            }
            b if b == CONFIRM => {
                let action = m.choices[m.sel.min(n - 1)].action();
                self.close();
                self.run_power(action, now);
            }
            _ => {}
        }
    }

    pub fn close(&mut self) {
        self.screen = Screen::Closed;
        self.over_game = false;
        self.held = None;
        self.next_refresh = None;
    }

    /// Looks at the system again in the background (backlight, radios, network time, volume and mute).
    pub fn refresh(&mut self, now: Instant) {
        self.next_refresh = Some(now + REFRESH_EVERY);
        // Network time only matters on the time page (timedatectl is a D-Bus call).
        self.poll(matches!(self.screen, Screen::Time(_)));
    }

    fn poll(&mut self, ntp: bool) {
        let (hw, net, polled) = (self.hw.clone(), self.net.clone(), self.polled.clone());
        self.spawn("poll-all", move || poll_all(&*hw, &net, &polled, ntp));
    }

    /// Timers: key repeat, polling, storing the brightness, expiring the OSD. True if something changed on screen.
    pub fn tick(&mut self, ctx: &Ctx, now: Instant) -> bool {
        let mut changed = false;
        if let Some((b, next, mode)) = self.vol_held {
            if now >= next {
                self.apply_mode(mode, if b == Button::VolUp { 1 } else { -1 }, ctx, now, false);
                self.vol_held = Some((b, now + REPEAT_EVERY, mode));
                changed = true;
            }
        }
        if let Some((b, next)) = self.held {
            if now >= next {
                self.press(b, ctx, now);
                self.held = Some((b, now + REPEAT_EVERY));
                changed = true;
            }
        }
        if let Some(mut o) = self.observe.take() {
            if self.vol_held.is_some() {
                o.until = now + OBSERVE_AFTER_RELEASE;
            }
            if now >= o.next {
                o.next = now + OBSERVE_EVERY;
                self.observe = Some(o);
                let what = self.observe.as_ref().unwrap().what;
                self.poll_now(what);
            } else {
                self.observe = Some(o);
            }
            if self.observe.as_ref().is_some_and(|o| now >= o.until) {
                self.observe = None;
            }
        }
        // The first look in the background, a moment after the start: the panel opens with what it found, and no
        // row ever reads (pactl, systemctl, rfkill, the backlight) on this thread (review 2026-10-24).
        if self.warm_at.is_some_and(|t| now >= t) {
            self.warm_at = None;
            self.poll(false);
        }
        // Date and time set by hand: its answer (before the look below takes the rest).
        changed |= self.take_clock_done(now);
        // What the background looks found.
        let p = self.polled.lock().unwrap().clone();
        if p.seq != self.seen {
            self.seen = p.seq;
            let (volume_moved, brightness_moved) = (p.volume != self.view.volume, p.brightness != self.view.brightness);
            self.view = p;
            // A frame only when the panel shows it (the look at the start comes with the shelf on screen).
            changed |= self.is_open();
            let first_look = self.observe.as_ref().is_some_and(|o| o.last.is_none());
            match (self.observe.as_ref().map(|o| o.what), self.view.volume, self.view.brightness) {
                (Some(What::Volume), Some(v), _) if volume_moved || first_look => {
                    self.levels.volume = Some((v, now));
                    self.show_if_new(What::Volume, v as u32, ctx, now);
                }
                (Some(What::Brightness), _, Some(v)) if brightness_moved || first_look => {
                    self.levels.brightness = Some((v, now));
                    self.show_if_new(What::Brightness, v as u32, ctx, now);
                }
                _ => {}
            }
            // A step taken before the level was known (`step_volume`, `step_brightness`): its OSD now.
            match self.pending_osd.take() {
                Some((What::Volume, _)) if self.view.volume.is_some() => {
                    let v = self.view.volume.unwrap_or(0);
                    self.levels.volume = Some((v, now));
                    self.show(Data::percent("Volume", v), ctx, now);
                }
                Some((What::Brightness, _)) if self.view.brightness.is_some() => {
                    let v = self.view.brightness.unwrap_or(0);
                    self.levels.brightness = Some((v, now));
                    self.show(Data::percent("Brightness", v), ctx, now);
                }
                other => self.pending_osd = other,
            }
        } else {
            // Busy flags and the like change without a new sequence number.
            let p = self.polled.lock().unwrap().clone();
            if p.busy_wifi != self.view.busy_wifi
                || p.busy_bt != self.view.busy_bt
                || p.busy_sync != self.view.busy_sync
                || p.busy_clock != self.view.busy_clock
                || p.power_error != self.view.power_error
            {
                self.view = p;
                changed = true;
            }
        }
        // A Wi-Fi switch that did not work: said once, calmly; the row already shows what the radio really does.
        if let Some(n) = self.view.wifi_notice.as_ref().filter(|n| n.id > self.seen_notice) {
            self.seen_notice = n.id;
            // Over a closed panel nobody would see it (and Settings → Wi-Fi says it on its own page). Said in the line
            // under the rows, where the row's help is, in the alert colour: a toast at the top runs into the title.
            if self.is_open() {
                self.wifi_failed = Some(n.clone());
                changed = true;
            }
        }
        // It goes with the next try, and with the panel.
        if self.wifi_failed.is_some() && (!self.is_open() || self.view.wifi_notice.is_none()) {
            self.wifi_failed = None;
            changed = true;
        }
        if self.pending_osd.is_some_and(|(_, until)| now >= until) {
            self.pending_osd = None;
        }
        if self.persist_at.is_some_and(|t| now >= t) {
            self.persist_at = None;
            if let Some((v, _)) = self.levels.brightness {
                let hw = self.hw.clone();
                self.spawn("persist-brightness", move || hw.persist_brightness(v));
            }
        }
        if self.osd.as_ref().is_some_and(|o| !o.active()) {
            self.osd = None;
            changed = true;
        }
        if self.toast.as_ref().is_some_and(|(_, t)| now >= *t + TOAST_FOR) {
            self.toast = None;
            changed = true;
        }
        if self.message.as_ref().is_some_and(|(_, t)| now >= *t) {
            self.message = None;
            changed = true;
        }
        if self.is_open() && self.next_refresh.is_some_and(|t| now >= t) {
            self.refresh(now);
        }
        if self.osd.is_some() {
            changed = true;
        }
        changed
    }
}

/// The zone list, once (a walk of the zoneinfo tree; on the worker, or a snapshot's own thread).
/// The battery of the power menu's snapshots: 73 %, about 4 h 20 min left.
fn demo_battery() -> BatteryLine {
    let left = crate::battery_estimate::Estimate::Left { per_hour: 16.8, secs: 4 * 3600 + 20 * 60, over: 40 * 60 };
    crate::power_menu::battery_line(Some(73), true, Some(&left))
}

fn load_zones(hw: &dyn crate::system_ctl::Hw, polled: &Mutex<Polled>) {
    let zones = Arc::new(hw.zones());
    let mut p = polled.lock().unwrap();
    p.zones = Some(zones);
    p.seq += 1;
}

/// The UTC offsets of `list` at `at`, into `offsets` (a file each; on the worker), then a new look so the page is drawn.
fn load_offsets(hw: &dyn crate::system_ctl::Hw, polled: &Mutex<Polled>, offsets: &Mutex<HashMap<String, Option<i32>>>, list: &[String], at: i64) {
    let found: Vec<(String, Option<i32>)> = list.iter().map(|z| (z.clone(), hw.zone_offset(z, at))).collect();
    offsets.lock().unwrap().extend(found);
    polled.lock().unwrap().seq += 1;
}

/// The Wi-Fi row's facts from the shared picture.
fn copy_net(p: &mut Polled, snap: &Snapshot) {
    p.wifi = snap.radio.switch();
    p.busy_wifi = snap.switching.is_some();
    p.wifi_notice = snap.notice.clone();
}

/// After a change of the shared picture: copied into `polled` under its lock, so the copy of a later picture always
/// comes after that of an earlier one.
fn apply_net(net: &NetState, polled: &Mutex<Polled>) {
    let mut p = polled.lock().unwrap_or_else(|e| e.into_inner());
    copy_net(&mut p, &net.snapshot());
    p.seq += 1;
}

/// One look at the system (on the worker, or a snapshot's own thread): the backlight, volume and mute, the radios, and
/// network time, the time zone and its offset when `time` (the time page is open). The battery is the status corner's
/// (its pill at the top right), not read here.
fn poll_all(hw: &dyn crate::system_ctl::Hw, net: &Arc<NetState>, polled: &Mutex<Polled>, time: bool) {
    let (brightness, volume, muted, bluetooth) = (hw.brightness(), hw.volume(), hw.muted(), hw.bluetooth());
    // The radio is the shared picture's: looked at there (a look that a switch or connect overtook is dropped).
    net.poll();
    let sound = hw.sound().unwrap_or(Sound::Unknown);
    let ntp = time.then(|| hw.ntp());
    let zone = time.then(|| {
        let zone = hw.timezone();
        let offset = zone.as_deref().and_then(|z| hw.zone_offset(z, unix_now()));
        (zone, offset)
    });
    // These don't change while the system runs.
    let (ram, boot) = {
        let p = polled.lock().unwrap();
        (p.ram_booted, p.boot_menu)
    };
    let ram_booted = ram.or_else(|| Some(hw.ram_booted()));
    let boot_menu = boot.or_else(|| Some(hw.boot_menu_supported()));
    let mut p = polled.lock().unwrap();
    p.ram_booted = ram_booted;
    p.boot_menu = boot_menu;
    p.brightness = brightness;
    p.volume = volume;
    p.muted = muted;
    p.sound = Some(sound);
    copy_net(&mut p, &net.snapshot());
    p.bluetooth = bluetooth;
    if ntp.is_some() {
        p.ntp = ntp;
    }
    if let Some((zone, offset)) = zone {
        p.timezone = Some(zone);
        p.zone_offset = Some(offset);
    }
    p.seq += 1;
}

// ---------------------------------------------------------------------------------------------------------
// Quick settings

impl Sys {
    pub(crate) fn rows(&self) -> Vec<Row> {
        let mut rows = Vec::new();
        // From the background's look only (`Polled`); before the first one every row is there, with "…".
        let unknown = self.view.seq == 0;
        if unknown || self.view.brightness.is_some() {
            rows.push(Row::Brightness);
        }
        rows.push(Row::Volume);
        if unknown || self.view.wifi.is_some() {
            rows.push(Row::Wifi);
        }
        if unknown || self.view.bluetooth.is_some() {
            rows.push(Row::Bluetooth);
        }
        rows.extend(self.profiles.rows().into_iter().map(|w| match w {
            Which::Performance => Row::Performance,
            Which::Fan => Row::Fan,
        }));
        rows.push(Row::Power);
        rows
    }

    /// Button inside an open screen (also called for repeats).
    fn press(&mut self, b: Button, ctx: &Ctx, now: Instant) {
        let repeating = matches!(b, Button::Up | Button::Down | Button::Left | Button::Right);
        if repeating && self.held.is_none_or(|(h, _)| h != b) {
            self.held = Some((b, now + REPEAT_DELAY));
        }
        match self.screen {
            Screen::Quick { .. } => self.quick_press(b, ctx, now),
            Screen::Time(_) => self.time_press(b, now),
            Screen::Power(_) => self.power_press(b, now),
            Screen::Closed => {}
        }
    }

    fn quick_press(&mut self, b: Button, ctx: &Ctx, now: Instant) {
        let rows = self.rows();
        let Screen::Quick { sel } = &mut self.screen else { return };
        *sel = (*sel).min(rows.len() - 1);
        match b {
            Button::Up => *sel = (*sel + rows.len() - 1) % rows.len(),
            Button::Down => *sel = (*sel + 1) % rows.len(),
            b if b == CANCEL || b == Button::Mode => self.close(),
            Button::Left | Button::Right => {
                let dir = if b == Button::Left { -1 } else { 1 };
                let row = rows[*sel];
                self.adjust(row, dir, ctx, now);
            }
            b if b == CONFIRM => {
                let row = rows[*sel];
                self.activate(row, now);
            }
            _ => {}
        }
    }

    fn adjust(&mut self, row: Row, dir: i32, _ctx: &Ctx, now: Instant) {
        // The panel covers the shelf, so no OSD: the row itself moves.
        match row {
            Row::Brightness => {
                self.step_brightness(dir, now);
            }
            Row::Volume => {
                self.step_volume(dir, now);
            }
            Row::Performance => self.step_profile(Which::Performance, dir, false, now),
            Row::Fan => self.step_profile(Which::Fan, dir, false, now),
            _ => {}
        }
    }

    /// Performance or Fan one step (`wrap`: A, on from the last choice to the first). Only memory here: the App takes
    /// the change (`take_profile_change`), saves and applies it on a thread. A game that keeps its own choice says so.
    fn step_profile(&mut self, w: Which, dir: i32, wrap: bool, now: Instant) {
        let Some(next) = self.profiles.step(w, dir, wrap) else { return };
        self.profiles.prefs = next;
        self.profile_change = Some(next);
        if let Some(kept) = self.profiles.kept(w) {
            self.toast = Some((kept, now));
        }
    }

    fn activate(&mut self, row: Row, now: Instant) {
        match row {
            Row::Wifi => self.toggle_radio(true),
            Row::Bluetooth => self.toggle_radio(false),
            Row::Performance => self.step_profile(Which::Performance, 1, true, now),
            Row::Fan => self.step_profile(Which::Fan, 1, true, now),
            // Over the panel; B gives the panel back.
            Row::Power => self.open_power(now),
            _ => {}
        }
    }

    fn sel_of(&self, row: Row) -> usize {
        self.rows().iter().position(|r| *r == row).unwrap_or(0)
    }

    fn toggle_radio(&mut self, wifi: bool) {
        let busy = if wifi { self.view.busy_wifi } else { self.view.busy_bt };
        if busy {
            return;
        }
        let known = if wifi { self.view.wifi } else { self.view.bluetooth };
        // Not looked at yet: unknown, and asked about on the worker (never here), as the switch is.
        if known.is_none() && self.view.seq > 0 {
            return;
        }
        let (hw, polled) = (self.hw.clone(), self.polled.clone());
        {
            let mut p = polled.lock().unwrap();
            if wifi {
                p.busy_wifi = true;
            } else {
                p.busy_bt = true;
            }
        }
        if wifi {
            // The shared picture does it: one worker for every change of the radio, the stored wish only after the
            // radio really changed, a notice if it didn't (`net_state`). It switches to the opposite of what the radio
            // is when its turn comes (a fresh read), not of what this row showed.
            let net = self.net.clone();
            self.spawn("wifi", move || {
                if known.is_none() {
                    // Not looked at yet, or no radio: the answer is the shared picture's.
                    net.poll();
                }
                net.toggle();
                apply_net(&net, &polled);
            });
            return;
        }
        self.spawn("bluetooth", move || {
            if let Some(on) = known.or_else(|| hw.bluetooth()) {
                hw.set_bluetooth(!on);
            }
            let mut p = polled.lock().unwrap();
            p.bluetooth = hw.bluetooth();
            p.busy_bt = false;
            p.seq += 1;
        });
    }

    /// From the background's first look (`Polled`): false until then.
    pub(crate) fn ram_booted(&self) -> bool {
        self.view.ram_booted == Some(true)
    }

    fn run_power(&mut self, action: PowerAction, now: Instant) {
        let hw = self.hw.clone();
        let polled = self.polled.clone();
        let (sleep, shutdown) = (self.sleep.clone(), self.shutdown.clone());
        // Restart pocket-ui touches no system command: it works on the virtual device too.
        if self.power_is_mock && action != PowerAction::RestartUi {
            let what = match action {
                PowerAction::Sleep => "Sleep",
                PowerAction::PowerOff => "Power off",
                PowerAction::Reboot => "Restart",
                PowerAction::RebootBootloader => "Restart to boot menu",
                PowerAction::RestartUi => "Restart pocket-ui",
            };
            self.message = Some((format!("{what}: not on a virtual device, nothing happened."), now + Duration::from_millis(2500)));
        } else if action != PowerAction::Sleep {
            self.message = Some((
                match action {
                    PowerAction::PowerOff => "Powering off…",
                    PowerAction::RebootBootloader => "Restarting to the boot menu…",
                    PowerAction::RestartUi => "Restarting pocket-ui…",
                    _ => "Restarting…",
                }
                .into(),
                now + Duration::from_secs(60),
            ));
        }
        self.spawn("power", move || match (action, sleep, shutdown) {
            (PowerAction::Sleep, Some(f), _) => f(),
            (PowerAction::PowerOff | PowerAction::Reboot | PowerAction::RebootBootloader | PowerAction::RestartUi, _, Some(f)) => f(action),
            _ => {
                if let Err(e) = hw.power(action) {
                    polled.lock().unwrap().power_error = Some(e);
                }
            }
        });
    }
}

// ---------------------------------------------------------------------------------------------------------
// Time page

impl Sys {
    fn time_press(&mut self, b: Button, now: Instant) {
        let zones: &[String] = self.view.zones.as_deref().map_or(&[], |z| z.as_slice());
        let Screen::Time(page) = &mut self.screen else { return };
        let current = self.view.timezone.clone().flatten();
        // A list of zones the page just opened: the worker reads their offsets.
        let mut ask: Option<Vec<String>> = None;
        match page {
            TimePage::Summary { sel } => match b {
                Button::Up => *sel = (*sel + 2) % 3,
                Button::Down => *sel = (*sel + 1) % 3,
                b if b == CANCEL => self.close(),
                b if b == CONFIRM && *sel == BY_HAND_ROW => {
                    if self.view.ntp == Some(NtpState::Synced) {
                        // Nothing to correct: network time set it, and would set it again.
                        self.toast = Some(("Network time keeps the clock right.".into(), now));
                    } else if !self.view.busy_clock {
                        let shown = self.clock_fake.unwrap_or_else(unix_now);
                        let built = crate::clock_check::built_unix();
                        *page = TimePage::SetClock(Picker::start(shown, built, self.offset_at));
                    }
                }
                b if b == CONFIRM => {
                    if *sel == TIME_ZONE_ROW {
                        let regions = timezone::regions(zones);
                        let at = current
                            .as_deref()
                            .map(timezone::region_of)
                            .and_then(|r| regions.iter().position(|x| x == r))
                            .map_or(1, |i| i + 1);
                        *page = TimePage::Regions { sel: at };
                    } else if !self.view.busy_sync {
                        self.polled.lock().unwrap().busy_sync = true;
                        let (hw, polled) = (self.hw.clone(), self.polled.clone());
                        let wait = if self.inline { 0 } else { 1500 };
                        self.spawn("sync", move || {
                            hw.sync_time();
                            // The service needs a moment to ask a server.
                            std::thread::sleep(Duration::from_millis(wait));
                            let mut p = polled.lock().unwrap();
                            p.ntp = Some(hw.ntp());
                            p.busy_sync = false;
                            p.seq += 1;
                        });
                    }
                }
                _ => {}
            },
            TimePage::Regions { sel } => {
                let regions = timezone::regions(zones);
                let n = regions.len() + 1; // "Search…" first
                match b {
                    Button::Up => *sel = (*sel + n - 1) % n,
                    Button::Down => *sel = (*sel + 1) % n,
                    Button::L1 => *sel = sel.saturating_sub(5),
                    Button::R1 => *sel = (*sel + 5).min(n - 1),
                    b if b == CANCEL => *page = TimePage::Summary { sel: 0 },
                    b if b == CONFIRM => {
                        if *sel == 0 {
                            *page = TimePage::Search(Box::new(Keyboard::default()));
                        } else {
                            let region = regions[*sel - 1].clone();
                            let list: Vec<String> = timezone::zones_in(zones, &region).into_iter().map(String::from).collect();
                            let at = current.as_ref().and_then(|c| list.iter().position(|z| z == c)).unwrap_or(0);
                            *page = TimePage::Zones {
                                heading: region,
                                zones: list.clone(),
                                sel: at,
                            };
                            ask = Some(list);
                        }
                    }
                    _ => {}
                }
            }
            TimePage::Zones { zones: list, sel, heading } => {
                let n = list.len().max(1);
                match b {
                    Button::Up => *sel = (*sel + n - 1) % n,
                    Button::Down => *sel = (*sel + 1) % n,
                    Button::L1 => *sel = sel.saturating_sub(5),
                    Button::R1 => *sel = (*sel + 5).min(n - 1),
                    b if b == CANCEL => {
                        // Back to where the list came from.
                        let searched = heading.starts_with("Search");
                        *page = if searched {
                            TimePage::Search(Box::new(Keyboard {
                                text: heading.trim_start_matches("Search: ").to_string(),
                                ..Default::default()
                            }))
                        } else {
                            TimePage::Regions { sel: 1 }
                        };
                    }
                    b if b == CONFIRM => {
                        if let Some(zone) = list.get(*sel).cloned() {
                            self.set_timezone(zone, now);
                        }
                    }
                    _ => {}
                }
            }
            TimePage::SetClock(picker) => match b {
                b if b == CANCEL => *page = TimePage::Summary { sel: BY_HAND_ROW },
                b if b == CONFIRM => {
                    let picker = *picker;
                    self.set_clock(picker, now);
                }
                b => {
                    picker.press(b);
                }
            },
            TimePage::Search(kb) => match kb.press(b) {
                Outcome::Editing => {}
                Outcome::Cancelled => *page = TimePage::Regions { sel: 0 },
                Outcome::Done(text) => {
                    let list: Vec<String> = timezone::search(zones, &text).into_iter().map(String::from).collect();
                    *page = TimePage::Zones {
                        heading: format!("Search: {text}"),
                        zones: list.clone(),
                        sel: 0,
                    };
                    ask = Some(list);
                }
            },
        }
        if let Some(list) = ask {
            self.want_offsets(&list);
        }
    }

    /// A on Date and time's picker: the local time it shows becomes the clock, on the worker (`Hw::set_clock`). A
    /// time `clock_check` would call wrong (before this version was built) is not set: the page says why.
    fn set_clock(&mut self, picker: Picker, now: Instant) {
        let unix = picker.unix(self.offset_at);
        if crate::clock_check::looks_wrong(unix, crate::clock_check::built_unix()) {
            self.toast = Some(("That date is before this version was made.".into(), now));
            return;
        }
        if self.view.busy_clock {
            return;
        }
        self.polled.lock().unwrap().busy_clock = true;
        self.view.busy_clock = true;
        let (hw, polled) = (self.hw.clone(), self.polled.clone());
        self.spawn("set-clock", move || {
            let done = hw.set_clock(unix);
            let mut p = polled.lock().unwrap();
            p.clock_done = Some((unix, done));
            p.busy_clock = false;
            p.seq += 1;
        });
        // Inline (tests): the answer is in already.
        self.take_clock_done(now);
    }

    /// The worker's answer to `set_clock`, once: a toast, back to the page, and the things that follow the clock told.
    fn take_clock_done(&mut self, now: Instant) -> bool {
        let Some((unix, done)) = self.polled.lock().unwrap().clock_done.take() else { return false };
        self.take_polled();
        self.view.busy_clock = false;
        let line = match done {
            Ok(ClockSet::Saved) => "Date and time set.".to_string(),
            Ok(ClockSet::ClockOnly(why)) => {
                eprintln!("time: the clock is set, the RTC isn't ({why})");
                "Set, but only until the next restart.".to_string()
            }
            Ok(ClockSet::Mock) => {
                self.toast = Some(("Not set: this is a virtual device.".into(), now));
                return true;
            }
            Err(e) => {
                self.toast = Some((e, now));
                return true;
            }
        };
        println!("time: set by hand to {unix}");
        self.toast = Some((line, now));
        self.clock_by_hand = true;
        // A snapshot's made-up clock follows the set (the real one has moved by itself).
        if self.clock_fake.is_some() {
            self.clock_fake = Some(unix);
        }
        if matches!(self.screen, Screen::Time(TimePage::SetClock(_))) {
            self.screen = Screen::Time(TimePage::Summary { sel: BY_HAND_ROW });
        }
        self.now_unix = unix_now();
        // The night light's schedule is in local time (its timer also notices the jump itself, TFD_TIMER_CANCEL_ON_SET).
        crate::night_light::poke();
        self.refresh(now);
        true
    }

    fn set_timezone(&mut self, zone: String, now: Instant) {
        match self.hw.set_timezone(&zone) {
            Ok(()) => {
                self.toast = Some((format!("Time zone: {}", timezone::city_of(&zone)), now));
                self.screen = Screen::Time(TimePage::Summary { sel: 0 });
                self.now_unix = unix_now();
                // The new zone is known: the summary shows it at once (its offset is in the list the pick came from),
                // and the look that follows confirms it.
                {
                    let offset = self.offsets.lock().unwrap().get(&zone).copied();
                    let mut p = self.polled.lock().unwrap();
                    p.timezone = Some(Some(zone.clone()));
                    p.zone_offset = offset;
                    p.seq += 1;
                    self.view = p.clone();
                    self.seen = p.seq;
                }
                // The night light's schedule is in local time.
                crate::night_light::poke();
                self.refresh(now);
            }
            Err(e) => self.toast = Some((e, now)),
        }
    }

    /// UTC offset of `zone` as the worker read it (`want_offsets`): None while it has not, Some(None) when the zone
    /// has no file. Memory only.
    pub(crate) fn offset(&self, zone: &str) -> Option<Option<i32>> {
        self.offsets.lock().unwrap().get(zone).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system_ctl::spy::Spy;
    use crate::system_ctl::{Hw, MockHw};
    use crate::testutil::TestDir;

    fn sys() -> (Sys, Arc<MockHw>) {
        let hw = Arc::new(MockHw::default().with_zoneinfo("/nonexistent"));
        let dir = TestDir::new("quick");
        let mut s = Sys::for_test(hw.clone(), dir.to_path_buf());
        s.scratch = Some(dir);
        // As a moment after the start: the background's first look is in (`WARM_AFTER`).
        poll_all(&*s.hw, &s.net, &s.polled, false);
        s.take_polled();
        (s, hw)
    }

    const SHELF: Ctx = Ctx { shelf: true, game: Game::None };

    fn key(s: &mut Sys, b: Button, down: bool, ctx: &Ctx, now: Instant) -> Out {
        s.pad(
            &PadEvent::Button {
                source: 0,
                button: b,
                pressed: down,
            },
            ctx,
            now,
        )
    }

    fn tap(s: &mut Sys, b: Button, ctx: &Ctx, now: Instant) -> Out {
        key(s, b, true, ctx, now);
        key(s, b, false, ctx, now)
    }

    #[test]
    fn plain_volume_keys_change_the_volume_in_5_steps_and_show_it() {
        let (mut s, hw) = sys();
        let now = Instant::now();
        let out = tap(&mut s, Button::VolUp, &SHELF, now);
        assert!(out.consumed && !out.select_used);
        assert_eq!(hw.volume(), Some(50));
        assert_eq!(s.osd.as_ref().unwrap().data, Data::percent("Volume", 50));
        tap(&mut s, Button::VolDown, &SHELF, now);
        tap(&mut s, Button::VolDown, &SHELF, now);
        assert_eq!(hw.volume(), Some(40));
        for _ in 0..20 {
            tap(&mut s, Button::VolDown, &SHELF, now);
        }
        assert_eq!(hw.volume(), Some(0));
        assert_eq!(s.osd.as_ref().unwrap().data.value, "Muted");
    }

    #[test]
    fn only_a_volume_change_asks_for_the_volume_tick() {
        let (mut s, _hw) = sys();
        let now = Instant::now();
        assert!(key(&mut s, Button::VolUp, true, &SHELF, now).volume_step, "Vol+ alone");
        assert!(!key(&mut s, Button::VolUp, false, &SHELF, now).volume_step, "the release");
        key(&mut s, Button::Start, true, &SHELF, now);
        assert!(!key(&mut s, Button::VolUp, true, &SHELF, now).volume_step, "START + Vol is brightness");
        key(&mut s, Button::VolUp, false, &SHELF, now);
        key(&mut s, Button::Start, false, &SHELF, now);
        key(&mut s, Button::Select, true, &SHELF, now);
        assert!(!key(&mut s, Button::VolDown, true, &SHELF, now).volume_step, "SELECT + Vol is colour");
    }

    #[test]
    fn start_plus_volume_is_brightness_and_never_goes_dark() {
        let (mut s, hw) = sys();
        let now = Instant::now();
        key(&mut s, Button::Start, true, &SHELF, now);
        tap(&mut s, Button::VolUp, &SHELF, now);
        assert_eq!(hw.brightness(), Some(65));
        assert_eq!(hw.volume(), Some(45), "the volume stays");
        for _ in 0..20 {
            tap(&mut s, Button::VolDown, &SHELF, now);
        }
        assert_eq!(hw.brightness(), Some(MIN_BRIGHTNESS));
        assert_eq!(s.osd.as_ref().unwrap().data, Data::percent("Brightness", 5));
        key(&mut s, Button::Start, false, &SHELF, now);
        // Stored once it has settled, not on every step.
        assert_eq!(hw.state().persisted_brightness, None);
        s.tick(&SHELF, now + Duration::from_secs(1));
        assert_eq!(hw.state().persisted_brightness, Some(5));
    }

    #[test]
    fn select_plus_volume_is_colour_temperature_and_marks_select_used() {
        let (mut s, hw) = sys();
        let now = Instant::now();
        key(&mut s, Button::Select, true, &SHELF, now);
        let out = tap(&mut s, Button::VolUp, &SHELF, now);
        assert!(out.select_used);
        assert_eq!(s.look().warmth, 1);
        assert_eq!(hw.volume(), Some(45));
        tap(&mut s, Button::VolDown, &SHELF, now);
        tap(&mut s, Button::VolDown, &SHELF, now);
        assert_eq!(s.look().warmth, -1);
        assert_eq!(s.osd.as_ref().unwrap().data.value, "Cool 1");
        for _ in 0..30 {
            tap(&mut s, Button::VolUp, &SHELF, now);
        }
        assert_eq!(s.look().warmth, crate::display::RANGE);
        assert_eq!(Look::load(&s.data_dir).warmth, crate::display::RANGE, "pocket-core reads it from the file");
        assert!(Look::load(&s.data_dir).soft);
    }

    #[test]
    fn the_night_light_goes_through_the_one_look_and_the_keys_keep_working() {
        let (mut s, _hw) = sys();
        let now = Instant::now();
        assert_eq!(s.look().night, 0, "off at the start (no night-light.json)");
        assert!(s.look.soft, "the mock has no gamma table: the Soft look");
        // A step of the fade: the look carries it, pocket-ui's own frame needs painting again (Soft).
        assert!(s.set_night(4400, Game::None));
        assert_eq!(s.look().night, 4400);
        let night = s.look().gains();
        assert!(night[0] == 1.0 && night[2] < night[1] && night[1] < 1.0, "{night:?}");
        assert!(!s.set_night(4400, Game::None), "the same level again: nothing to do");
        assert!(Look::load(&s.data_dir).soft, "pocket-core is told to tint when it starts (display.json, written for the night light)");
        // The temperature keys still work, and add to it.
        key(&mut s, Button::Select, true, &SHELF, now);
        tap(&mut s, Button::VolUp, &SHELF, now);
        tap(&mut s, Button::VolUp, &SHELF, now);
        key(&mut s, Button::Select, false, &SHELF, now);
        assert_eq!((s.look().warmth, s.look().night), (2, 4400));
        let both = s.look().gains();
        assert!(both[2] < night[2], "warmer than the night light alone: {both:?} {night:?}");
        let text = std::fs::read_to_string(s.data_dir.join(Look::FILE)).unwrap();
        assert!(!text.contains("night"), "the night light follows the clock and is not saved: {text}");
        // Off again: exactly the chosen temperature.
        assert!(s.set_night(0, Game::None));
        assert_eq!(s.look().gains(), Look { warmth: 2, ..Look::default() }.gains());
        // Neutral again with nothing chosen: the compositor's table is left alone, nothing to tint.
        s.look.warmth = 0;
        assert!(s.look().is_neutral());
    }

    #[test]
    fn a_running_night_light_is_there_from_the_first_look() {
        let hw = Arc::new(MockHw::default().with_zoneinfo("/nonexistent"));
        let dir = TestDir::new("quick-night");
        crate::night_light::Prefs { mode: crate::night_light::Mode::On, strength: 5, ..Default::default() }.save(&dir);
        let s = Sys::for_test(hw, dir.to_path_buf());
        assert_eq!(s.look().night, 3200, "On at the start: no flash of plain colours");
        // On the worker, so give it a moment (to the backup copy that the write ends with: the folder goes after it).
        let backup = crate::fsutil::backup_path(&dir.join(Look::FILE));
        let told = (0..200).any(|_| {
            std::thread::sleep(Duration::from_millis(10));
            Look::load(&dir).soft && std::fs::metadata(&backup).is_ok_and(|m| m.len() > 0)
        });
        assert!(told, "and pocket-core is told");
    }

    #[test]
    fn a_volume_key_reported_by_two_devices_counts_once() {
        let (mut s, hw) = sys();
        let now = Instant::now();
        key(&mut s, Button::VolUp, true, &SHELF, now);
        key(&mut s, Button::VolUp, true, &SHELF, now);
        key(&mut s, Button::VolUp, false, &SHELF, now);
        key(&mut s, Button::VolUp, false, &SHELF, now);
        assert_eq!(hw.volume(), Some(50));
        key(&mut s, Button::VolUp, true, &SHELF, now);
        assert_eq!(hw.volume(), Some(55), "and the next press counts again");
    }

    #[test]
    fn a_held_volume_key_repeats_after_a_delay() {
        let (mut s, hw) = sys();
        let t0 = Instant::now();
        key(&mut s, Button::VolUp, true, &SHELF, t0);
        assert_eq!(hw.volume(), Some(50));
        s.tick(&SHELF, t0 + Duration::from_millis(200));
        assert_eq!(hw.volume(), Some(50), "not yet");
        s.tick(&SHELF, t0 + Duration::from_millis(330));
        assert_eq!(hw.volume(), Some(55));
        s.tick(&SHELF, t0 + Duration::from_millis(420));
        assert_eq!(hw.volume(), Some(60));
        key(&mut s, Button::VolUp, false, &SHELF, t0 + Duration::from_millis(430));
        s.tick(&SHELF, t0 + Duration::from_millis(900));
        assert_eq!(hw.volume(), Some(60), "released");
        assert!(s.next_wake().is_some(), "the OSD still has to go away");
    }

    #[test]
    fn with_input_sense_running_the_volume_is_only_shown() {
        let (mut s, hw) = sys();
        s.set_rocknix_keys(true);
        let now = Instant::now();
        // input_sense changed the volume to 55 behind our back.
        hw.set_volume(55);
        key(&mut s, Button::VolUp, true, &SHELF, now);
        assert_eq!(hw.volume(), Some(55), "we did not step it ourselves");
        s.tick(&SHELF, now + Duration::from_millis(5));
        assert_eq!(s.osd.as_ref().map(|o| o.data.clone()), Some(Data::percent("Volume", 55)));
        // HOME + Vol is ROCKNIX's brightness: observed too.
        s.osd = None;
        let hw2 = hw.clone();
        key(&mut s, Button::VolUp, false, &SHELF, now);
        key(&mut s, Button::Mode, true, &SHELF, now);
        hw2.set_brightness(70);
        key(&mut s, Button::VolDown, true, &SHELF, now + Duration::from_secs(2));
        // Read on the worker (inline here), shown by the next tick.
        s.tick(&SHELF, now + Duration::from_millis(2005));
        assert_eq!(s.osd.as_ref().map(|o| o.data.clone()), Some(Data::percent("Brightness", 70)));
        // START + Vol is ours even then.
        key(&mut s, Button::Mode, false, &SHELF, now);
        key(&mut s, Button::Start, true, &SHELF, now);
        key(&mut s, Button::VolUp, true, &SHELF, now + Duration::from_secs(3));
        assert_eq!(hw.brightness(), Some(75));
    }

    fn shown(s: &Sys) -> Option<Data> {
        s.osd.as_ref().map(|o| o.data.clone())
    }

    #[test]
    fn with_input_sense_running_vol_down_is_shown_every_time_also_after_a_vol_up() {
        let (mut s, hw) = sys();
        s.set_rocknix_keys(true);
        let t0 = Instant::now();
        hw.set_volume(50);
        // Ten presses each way, as input_sense changes the volume at the key down.
        for i in 0..10u64 {
            for (b, to) in [(Button::VolUp, 55u8), (Button::VolDown, 50u8)] {
                let now = t0 + Duration::from_secs(2 * i + if b == Button::VolUp { 0 } else { 1 });
                s.osd = None;
                hw.set_volume(to);
                key(&mut s, b, true, &SHELF, now);
                s.tick(&SHELF, now + Duration::from_millis(5));
                assert_eq!(shown(&s), Some(Data::percent("Volume", to)), "{b:?} press {i}");
                key(&mut s, b, false, &SHELF, now + Duration::from_millis(80));
            }
        }
    }

    #[test]
    fn vol_down_whose_change_comes_after_our_first_look_is_shown_with_the_new_level() {
        let (mut s, hw) = sys();
        s.set_rocknix_keys(true);
        let now = Instant::now();
        hw.set_volume(50);
        key(&mut s, Button::VolDown, true, &SHELF, now);
        s.tick(&SHELF, now + Duration::from_millis(5));
        // input_sense's `volume down` (a shell script) took its time.
        hw.set_volume(45);
        s.tick(&SHELF, now + Duration::from_millis(100));
        s.tick(&SHELF, now + Duration::from_millis(200));
        assert_eq!(shown(&s), Some(Data::percent("Volume", 45)));
    }

    #[test]
    fn with_input_sense_a_held_vol_down_follows_the_volume_down_to_zero_and_shows_zero() {
        let (mut s, hw) = sys();
        s.set_rocknix_keys(true);
        let now = Instant::now();
        hw.set_volume(10);
        hw.set_volume(5);
        key(&mut s, Button::VolDown, true, &SHELF, now);
        s.tick(&SHELF, now + Duration::from_millis(10));
        assert_eq!(shown(&s), Some(Data::percent("Volume", 5)));
        // The repeat: input_sense steps again while held.
        hw.set_volume(0);
        s.tick(&SHELF, now + Duration::from_millis(400));
        s.tick(&SHELF, now + Duration::from_millis(500));
        assert_eq!(shown(&s), Some(Data::percent("Volume", 0)));
        // At 0 the key still gets an answer: the level, unchanged.
        key(&mut s, Button::VolDown, false, &SHELF, now + Duration::from_millis(600));
        s.osd = None;
        key(&mut s, Button::VolDown, true, &SHELF, now + Duration::from_secs(3));
        s.tick(&SHELF, now + Duration::from_millis(3005));
        assert_eq!(shown(&s), Some(Data::percent("Volume", 0)));
    }

    #[test]
    fn home_tap_on_the_shelf_opens_quick_settings_but_not_with_another_button_or_in_a_game() {
        let (mut s, _) = sys();
        let now = Instant::now();
        let out = tap(&mut s, Button::Mode, &SHELF, now);
        assert!(out.consumed && matches!(s.screen, Screen::Quick { .. }));
        s.close();
        // HOME + Vol is a combo, not a tap.
        key(&mut s, Button::Mode, true, &SHELF, now);
        tap(&mut s, Button::VolUp, &SHELF, now);
        key(&mut s, Button::Mode, false, &SHELF, now);
        assert!(!s.is_open());
        // Not while a game runs, nor on screens that are not the shelf.
        tap(
            &mut s,
            Button::Mode,
            &Ctx {
                shelf: false,
                game: Game::PocketCore,
            },
            now,
        );
        assert!(!s.is_open());
        tap(
            &mut s,
            Button::Mode,
            &Ctx {
                shelf: false,
                game: Game::None,
            },
            now,
        );
        assert!(!s.is_open());
    }

    #[test]
    fn quick_settings_navigation_sliders_and_toggles() {
        let (mut s, hw) = sys();
        let now = Instant::now();
        tap(&mut s, Button::Mode, &SHELF, now);
        // Brightness is first: Right raises it, Left lowers it.
        tap(&mut s, Button::Right, &SHELF, now);
        assert_eq!(hw.brightness(), Some(65));
        tap(&mut s, Button::Left, &SHELF, now);
        tap(&mut s, Button::Left, &SHELF, now);
        assert_eq!(hw.brightness(), Some(55));
        // Down to volume.
        tap(&mut s, Button::Down, &SHELF, now);
        tap(&mut s, Button::Right, &SHELF, now);
        assert_eq!(hw.volume(), Some(50));
        // Wi-Fi comes right after: no colour temperature or contrast rows in between.
        let rows = s.rows();
        assert_eq!(rows[2], Row::Wifi, "{rows:?}");
        tap(&mut s, Button::Down, &SHELF, now);
        tap(&mut s, Button::East, &SHELF, now);
        assert_eq!(hw.wifi(), Some(false));
        // B closes.
        tap(&mut s, Button::South, &SHELF, now);
        assert!(!s.is_open());
    }

    /// Quick settings and Settings → Wi-Fi on one stand-in system: a switch on either shows on both, at once.
    #[test]
    fn quick_settings_and_the_wifi_page_show_the_same_state() {
        use crate::net_state::fake::{nets, FakeNet};
        use crate::net_state::{NetState, Radio};
        use crate::setup::{Only, Setup, SetupEvent};
        let fake = FakeNet::new(Radio::On);
        fake.script().scans = std::collections::VecDeque::from([Ok(nets("Home:78:WPA2:*\n"))]);
        let net = NetState::inline(fake.clone());
        let hw = Arc::new(MockHw::default().with_zoneinfo("/nonexistent"));
        let dir = TestDir::new("quick-both");
        let mut s = Sys::for_test(hw.clone(), dir.to_path_buf()).with_net(net.clone());
        poll_all(&*s.hw, &s.net, &s.polled, false);
        s.take_polled();
        let events = Arc::new(Mutex::new(Vec::new()));
        let sink = events.clone();
        let mut page = Setup::new(Arc::new(move |e| sink.lock().unwrap().push(e)), vec![("Dusk", "dark")], 0, dir.to_path_buf(), Vec::new(), hw)
            .with_net(net.clone())
            .only(Only::Wifi);
        let deliver = |page: &mut Setup| {
            for e in std::mem::take(&mut *events.lock().unwrap()) {
                if matches!(e, SetupEvent::Net) {
                    page.on_event(e);
                }
            }
        };
        deliver(&mut page);
        assert_eq!(s.view.wifi, Some(true));
        assert_eq!(page.wifi_status("").0, "Connected to Home · signal 78%");

        // Off in the quick settings: the page says it too, and offers to turn it on.
        let now = Instant::now();
        tap(&mut s, Button::Mode, &SHELF, now);
        tap(&mut s, Button::Down, &SHELF, now);
        tap(&mut s, Button::Down, &SHELF, now);
        tap(&mut s, Button::East, &SHELF, now);
        s.take_polled();
        assert_eq!(s.view.wifi, Some(false));
        deliver(&mut page);
        assert_eq!(page.wifi_status("").0, "Wi-Fi is off. Press A to turn it on.");

        // On in the page: the quick settings show it.
        page.press(Button::East);
        deliver(&mut page);
        s.take_polled();
        assert_eq!(s.view.wifi, Some(true));
        assert_eq!(page.wifi_status("").0, "Connected to Home · signal 78%");
        assert_eq!(fake.script().wanted, Some(true));
    }

    /// A Wi-Fi switch that did not work: the row keeps telling the truth and a calm line says what happened, once.
    #[test]
    fn a_wifi_switch_that_failed_keeps_the_row_truthful_and_says_so_once() {
        use crate::net_state::fake::FakeNet;
        use crate::net_state::{Cause, NetState, Radio};
        let fake = FakeNet::new(Radio::Off(Cause::Soft));
        fake.script().wanted = Some(false);
        fake.script().switch_result = Err("wifictl enable exited with 1".into());
        let (s, _hw) = sys();
        let mut s = s.with_net(NetState::inline(fake.clone()));
        poll_all(&*s.hw, &s.net, &s.polled, false);
        s.take_polled();
        assert_eq!(s.view.wifi, Some(false));
        let ctx = &SHELF;
        let now = Instant::now();
        tap(&mut s, Button::Mode, ctx, now);
        tap(&mut s, Button::Down, ctx, now);
        tap(&mut s, Button::Down, ctx, now);
        tap(&mut s, Button::East, ctx, now);
        s.tick(ctx, now);
        assert_eq!(s.view.wifi, Some(false), "the switch is not lying about a radio that did not come up");
        assert!(!s.view.busy_wifi);
        assert_eq!(s.wifi_failed.as_ref().map(|n| n.line(false)), Some("Couldn't turn Wi-Fi on. Try again."), "the line under the rows says it");
        assert_eq!(fake.script().wanted, Some(false), "the wish was not stored");
        s.wifi_failed = None;
        s.tick(ctx, now);
        assert!(s.wifi_failed.is_none(), "said once");
        // The next try works; no old message comes back.
        fake.script().switch_result = Ok(());
        tap(&mut s, Button::East, ctx, now);
        s.tick(ctx, now);
        assert_eq!(s.view.wifi, Some(true));
        assert!(s.wifi_failed.is_none());
        assert_eq!(fake.script().wanted, Some(true));
    }

    /// The first press of the switch goes by a fresh read, not by the row: the boot hooks changed the radio since it was drawn.
    #[test]
    fn the_wifi_row_toggles_from_a_fresh_read_not_from_what_it_showed() {
        use crate::net_state::fake::FakeNet;
        use crate::net_state::{Cause, NetState, Radio};
        let fake = FakeNet::new(Radio::On);
        let (s, _hw) = sys();
        let mut s = s.with_net(NetState::inline(fake.clone()));
        poll_all(&*s.hw, &s.net, &s.polled, false);
        s.take_polled();
        assert_eq!(s.view.wifi, Some(true), "the row shows on ...");
        // The hooks block it behind everybody's back (the player's wish is off here: nothing switches it on by itself).
        fake.script().radio = Radio::Off(Cause::Soft);
        fake.script().wanted = Some(false);
        let now = Instant::now();
        tap(&mut s, Button::Mode, &SHELF, now);
        tap(&mut s, Button::Down, &SHELF, now);
        tap(&mut s, Button::Down, &SHELF, now);
        tap(&mut s, Button::East, &SHELF, now);
        s.take_polled();
        assert_eq!(s.view.wifi, Some(true), "... the radio really was off: the press turned it on");
        assert_eq!(fake.script().wanted, Some(true));
    }

    #[test]
    fn the_quick_panel_has_no_colour_temperature_or_contrast_row() {
        for backend in [Backend::Soft, Backend::Gamma(256)] {
            let (mut s, _hw) = sys();
            s.backend = backend;
            let rows = format!("{:?}", s.rows());
            assert!(!rows.contains("Warmth") && !rows.contains("Contrast"), "{backend:?}: {rows}");
            assert!(rows.contains("Brightness") && rows.contains("Volume") && rows.contains("Wifi"), "the other rows stay: {rows}");
        }
    }

    fn go_to(s: &mut Sys, row: Row, now: Instant) {
        let at = s.rows().iter().position(|r| *r == row).unwrap_or_else(|| panic!("{row:?} in {:?}", s.rows()));
        for _ in 0..at {
            tap(s, Button::Down, &SHELF, now);
        }
    }

    #[test]
    fn the_panel_has_seven_rows_and_power_opens_the_power_menu() {
        let (mut s, hw) = sys();
        let now = Instant::now();
        tap(&mut s, Button::Mode, &SHELF, now);
        assert_eq!(
            s.rows(),
            [Row::Brightness, Row::Volume, Row::Wifi, Row::Bluetooth, Row::Performance, Row::Fan, Row::Power]
        );
        go_to(&mut s, Row::Power, now);
        tap(&mut s, Button::East, &SHELF, now);
        assert!(s.power_menu_open(), "A on Power… opens the power menu");
        // B gives the panel back, on Power….
        tap(&mut s, Button::South, &SHELF, now);
        assert!(matches!(s.screen, Screen::Quick { sel: 6 }));
        assert!(hw.state().power_log.is_empty());
        // The power menu does what it does from the power key: Restart (second entry) at once.
        tap(&mut s, Button::East, &SHELF, now);
        tap(&mut s, Button::Down, &SHELF, now);
        tap(&mut s, Button::East, &SHELF, now);
        assert_eq!(hw.state().power_log, [PowerAction::Reboot]);
        assert!(!s.is_open() && s.message.is_some());
    }

    #[test]
    fn sleep_from_power_uses_the_sleep_handler_and_closes_the_panel() {
        let (mut s, hw) = sys();
        let now = Instant::now();
        tap(&mut s, Button::Mode, &SHELF, now);
        go_to(&mut s, Row::Power, now);
        tap(&mut s, Button::East, &SHELF, now);
        tap(&mut s, Button::East, &SHELF, now);
        assert_eq!(hw.state().power_log, [PowerAction::Sleep]);
        assert!(!s.is_open(), "the panel closes before the device sleeps");
    }

    #[test]
    fn performance_and_fan_change_in_memory_and_hand_the_change_over_once() {
        let (mut s, _) = sys();
        let now = Instant::now();
        s.set_profile_prefs(Prefs::default());
        tap(&mut s, Button::Mode, &SHELF, now);
        go_to(&mut s, Row::Performance, now);
        tap(&mut s, Button::Right, &SHELF, now);
        let p = s.take_profile_change().expect("a change");
        assert_eq!(p.games_performance, crate::power::Performance::Performance);
        assert_eq!(s.take_profile_change(), None, "once");
        tap(&mut s, Button::Right, &SHELF, now);
        assert_eq!(s.take_profile_change(), None, "Max is the end of the row");
        // A goes round.
        tap(&mut s, Button::East, &SHELF, now);
        assert_eq!(s.take_profile_change().unwrap().games_performance, crate::power::Performance::Battery);
        // On the shelf the Fan row is the fan outside games.
        tap(&mut s, Button::Down, &SHELF, now);
        tap(&mut s, Button::Left, &SHELF, now);
        let p = s.take_profile_change().unwrap();
        assert_eq!((p.outside_fan, p.games_fan), (crate::power::Fan::Quiet, crate::power::Fan::Quiet));
        assert!(s.toast.is_none());
    }

    #[test]
    fn over_a_game_with_its_own_choice_the_change_is_the_default_and_a_toast_says_so() {
        use crate::power::{Fan, Performance};
        use crate::quick_profiles::{Kind, Whose};
        let (mut s, _) = sys();
        let now = Instant::now();
        s.open_quick_over_game(
            now,
            GameView {
                kind: Kind::PocketCore,
                own_performance: Some((Performance::Performance, Whose::Game)),
                own_fan: None,
            },
        );
        assert!(s.quick_over_game());
        go_to(&mut s, Row::Performance, now);
        tap(&mut s, Button::Left, &SHELF, now);
        assert_eq!(s.take_profile_change().unwrap().games_performance, Performance::Battery);
        assert_eq!(s.toast.as_ref().map(|t| t.0.as_str()), Some("Default for games: Battery. This game keeps Max"));
        tap(&mut s, Button::Down, &SHELF, now);
        tap(&mut s, Button::Right, &SHELF, now);
        let p = s.take_profile_change().unwrap();
        assert_eq!((p.games_fan, p.outside_fan), (Fan::Auto, Fan::Auto), "over a game the Fan row is Games: fan");
        // The power menu from here keeps the game behind it.
        s.open_power(now);
        assert!(s.quick_over_game());
        // HOME on the shelf afterwards: no game under the rows any more.
        s.close();
        tap(&mut s, Button::Mode, &SHELF, now);
        assert_eq!(s.profile_game(), None);
    }

    #[test]
    fn without_a_fan_there_is_no_fan_row_and_without_cpufreq_performance_cannot_change() {
        let (mut s, _) = sys();
        let now = Instant::now();
        s.set_profile_caps(true, false);
        assert!(!s.rows().contains(&Row::Fan));
        assert_eq!(s.rows().len(), 6);
        s.set_profile_caps(false, true);
        tap(&mut s, Button::Mode, &SHELF, now);
        go_to(&mut s, Row::Performance, now);
        tap(&mut s, Button::Right, &SHELF, now);
        tap(&mut s, Button::East, &SHELF, now);
        assert_eq!(s.take_profile_change(), None);
        assert_eq!(s.profiles.value(Which::Performance), "Not available here");
    }

    #[test]
    fn opening_the_panel_reads_nothing_and_is_quick() {
        // Everything the new rows show is in memory: a thousand opens and row lists stay far below a frame.
        let (mut s, _) = sys();
        let now = Instant::now();
        let t = Instant::now();
        for _ in 0..1000 {
            s.open_quick(now);
            let rows = s.rows();
            for w in [Which::Performance, Which::Fan] {
                std::hint::black_box((s.profiles.value(w), s.profiles.note(w), s.profiles.desc(w), rows.len()));
            }
            s.close();
        }
        let each = t.elapsed() / 1000;
        println!("quick settings: open + rows {each:?}");
        assert!(each < Duration::from_millis(2), "{each:?} per open");
    }

    #[test]
    fn time_zone_pick_by_region_and_by_search() {
        let (mut s, hw) = sys();
        let now = Instant::now();
        s.open_time(now);
        // The worker's looks (inline here) are in once the panel takes them.
        assert!(s.zone_list().is_empty() && s.view.timezone.is_none(), "nothing read by the page itself");
        assert!(s.take_polled());
        assert!(s.zone_list().iter().any(|z| z == "Europe/Vienna"));
        // Time zone > regions: the current region (America) is preselected.
        tap(&mut s, Button::East, &SHELF, now);
        let regions = timezone::regions(s.zone_list());
        match &s.screen {
            Screen::Time(TimePage::Regions { sel }) => assert_eq!(regions[sel - 1], "America"),
            _ => panic!("regions page"),
        }
        // Search: type "vien" with the keyboard (first row of letters is the number row; use text directly).
        let Screen::Time(page) = &mut s.screen else { unreachable!() };
        *page = TimePage::Regions { sel: 0 };
        tap(&mut s, Button::East, &SHELF, now);
        let Screen::Time(TimePage::Search(kb)) = &mut s.screen else {
            panic!("keyboard")
        };
        kb.text = "vien".into();
        tap(&mut s, Button::Start, &SHELF, now);
        match &s.screen {
            Screen::Time(TimePage::Zones { zones, heading, .. }) => {
                assert_eq!(zones, &["Europe/Vienna"]);
                assert_eq!(heading, "Search: vien");
            }
            _ => panic!("results"),
        }
        tap(&mut s, Button::East, &SHELF, now);
        assert_eq!(hw.timezone().as_deref(), Some("Europe/Vienna"));
        assert!(matches!(s.screen, Screen::Time(TimePage::Summary { .. })));
        assert!(s.toast.as_ref().unwrap().0.contains("Vienna"));
        // B on the summary leaves the page.
        tap(&mut s, Button::South, &SHELF, now);
        assert!(!s.is_open());
    }

    #[test]
    fn sync_now_asks_the_time_service_and_reports() {
        let (mut s, hw) = sys();
        let now = Instant::now();
        hw.state(); // mock starts synced
        s.open_time(now);
        tap(&mut s, Button::Down, &SHELF, now);
        tap(&mut s, Button::East, &SHELF, now);
        s.tick(&SHELF, now);
        assert_eq!(s.view.ntp, Some(NtpState::Synced));
        assert!(!s.view.busy_sync);
    }

    #[test]
    fn game_osd_goes_to_pocket_core_not_to_the_shelf() {
        let (mut s, _) = sys();
        let now = Instant::now();
        let game = Ctx {
            shelf: false,
            game: Game::PocketCore,
        };
        key(&mut s, Button::Start, true, &game, now);
        tap(&mut s, Button::VolUp, &game, now);
        assert!(s.osd.is_none(), "the shelf is not on screen");
        let other = Ctx {
            shelf: false,
            game: Game::Other,
        };
        tap(&mut s, Button::VolUp, &other, now);
        assert!(s.osd.is_none());
    }

    /// The OSD lines sent to pocket-core; `taken`: whether a pocket-core reads its FIFO.
    fn record_game_osd(s: &mut Sys, taken: bool) -> Arc<Mutex<Vec<String>>> {
        let lines = Arc::new(Mutex::new(Vec::new()));
        let l = lines.clone();
        s.set_game_osd(move |line| {
            l.lock().unwrap().push(line.to_string());
            taken
        });
        lines
    }

    fn sent(lines: &Arc<Mutex<Vec<String>>>) -> Vec<String> {
        std::mem::take(&mut *lines.lock().unwrap())
    }

    const POCKET_CORE: Ctx = Ctx { shelf: false, game: Game::PocketCore };

    #[test]
    fn every_level_key_in_a_pocket_core_game_sends_its_osd_to_the_game() {
        // As on the device: input_sense handles plain Vol and HOME + Vol, we do START and SELECT + Vol.
        let (mut s, hw) = sys();
        s.set_rocknix_keys(true);
        let lines = record_game_osd(&mut s, true);
        let t0 = Instant::now();
        // START + Vol-: brightness, a step at once and more while held, each one shown over the game.
        key(&mut s, Button::Start, true, &POCKET_CORE, t0);
        key(&mut s, Button::VolDown, true, &POCKET_CORE, t0);
        s.tick(&POCKET_CORE, t0 + Duration::from_millis(330));
        key(&mut s, Button::VolDown, false, &POCKET_CORE, t0 + Duration::from_millis(340));
        key(&mut s, Button::Start, false, &POCKET_CORE, t0 + Duration::from_millis(350));
        assert_eq!(hw.brightness(), Some(50));
        assert_eq!(sent(&lines), ["osd Brightness|55%|0.550|0", "osd Brightness|50%|0.500|0"]);
        // SELECT + Vol+: colour temperature.
        key(&mut s, Button::Select, true, &POCKET_CORE, t0);
        tap(&mut s, Button::VolUp, &POCKET_CORE, t0);
        key(&mut s, Button::Select, false, &POCKET_CORE, t0);
        assert_eq!(sent(&lines), [format!("osd {}", s.warmth_data().to_wire())]);
        // Vol alone: input_sense changes the volume, we show what it did.
        let t1 = t0 + Duration::from_secs(5);
        hw.set_volume(40);
        key(&mut s, Button::VolDown, true, &POCKET_CORE, t1);
        s.tick(&POCKET_CORE, t1 + Duration::from_millis(5));
        assert_eq!(sent(&lines), ["osd Volume|40%|0.400|0"]);
        // HOME + Vol: ROCKNIX's brightness, shown too.
        key(&mut s, Button::VolDown, false, &POCKET_CORE, t1);
        key(&mut s, Button::Mode, true, &POCKET_CORE, t1);
        hw.set_brightness(30);
        key(&mut s, Button::VolDown, true, &POCKET_CORE, t1 + Duration::from_secs(2));
        s.tick(&POCKET_CORE, t1 + Duration::from_millis(2005));
        assert_eq!(sent(&lines), ["osd Brightness|30%|0.300|0"]);
        assert!(s.osd.is_none(), "the shelf's own pill would sit behind the game");
    }

    #[test]
    fn a_pocket_core_pocket_ui_does_not_know_of_still_gets_the_osd() {
        // pocket-ui restarted under a running game (or the game was started over SSH): no game known, but a
        // pocket-core reads its FIFO. Its window covers the shelf, so the OSD has to go there as well.
        let (mut s, hw) = sys();
        let lines = record_game_osd(&mut s, true);
        let now = Instant::now();
        key(&mut s, Button::Start, true, &SHELF, now);
        tap(&mut s, Button::VolDown, &SHELF, now);
        assert_eq!(hw.brightness(), Some(55));
        assert_eq!(sent(&lines), ["osd Brightness|55%|0.550|0"]);
        assert!(s.osd.is_some(), "and on the shelf, when that is what is on screen");
    }

    #[test]
    fn retroarch_and_the_standalone_emulators_get_no_osd_line() {
        let (mut s, _) = sys();
        let lines = record_game_osd(&mut s, true);
        let other = Ctx { shelf: false, game: Game::Other };
        key(&mut s, Button::Start, true, &other, Instant::now());
        tap(&mut s, Button::VolUp, &other, Instant::now());
        assert!(sent(&lines).is_empty() && s.osd.is_none());
    }

    /// Review 2026-10-24: opened before the background had looked, the panel ran `pactl` (volume), `systemctl`
    /// (Bluetooth) and read rfkill (Wi-Fi) on the UI thread. Opening, painting every row, the power menu, stepping a
    /// level and switching a radio never look at the system on the UI thread, before the first look or after it.
    #[test]
    fn the_panel_never_looks_at_the_system_on_the_ui_thread() {
        let spy = Arc::new(Spy::new(MockHw::default()));
        let dir = TestDir::new("quick-spy");
        // A real worker thread (not `for_test`'s inline jobs), as in the app.
        let mut s = Sys::new(spy.clone(), dir.to_path_buf(), true);
        s.rocknix_keys = false;
        let mut text = crate::text::Text::builtin();
        let mut px = vec![0u32; 1280 * 960];
        let mut paint = |s: &mut Sys| {
            let mut c = crate::gfx::Canvas::new(&mut px, 1280, 960);
            s.paint(&mut c, &mut text, false);
        };
        let t0 = Instant::now();
        // Before the first look: every row, with "…".
        s.open_quick(t0);
        assert_eq!(s.rows()[..4], [Row::Brightness, Row::Volume, Row::Wifi, Row::Bluetooth]);
        for sel in 0..s.rows().len() {
            s.screen = Screen::Quick { sel };
            paint(&mut s);
        }
        // A step and a switch before anything is known: the worker reads, steps and answers.
        s.screen = Screen::Quick { sel: 1 };
        key(&mut s, Button::Right, true, &SHELF, t0);
        key(&mut s, Button::Right, false, &SHELF, t0);
        // The first look comes in (the worker answers within moments).
        let until = Instant::now() + Duration::from_secs(5);
        while s.polled.lock().unwrap().seq < 2 && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(5));
        }
        s.tick(&SHELF, t0 + Duration::from_millis(10));
        assert!(s.view.volume.is_some() && s.view.wifi.is_some(), "the look is in: {:?}", s.view);
        for sel in 0..s.rows().len() {
            s.screen = Screen::Quick { sel };
            paint(&mut s);
            key(&mut s, Button::Left, true, &SHELF, t0);
            key(&mut s, Button::Left, false, &SHELF, t0);
        }
        s.open_power(t0);
        paint(&mut s);
        s.close();
        // The look at the start, when the panel was never opened.
        let mut fresh = Sys::new(spy.clone(), dir.to_path_buf(), true);
        fresh.tick(&SHELF, Instant::now() + WARM_AFTER);
        fresh.open_quick(Instant::now());
        paint(&mut fresh);
        assert_eq!(*spy.on_ui.lock().unwrap(), Vec::<&str>::new(), "looked at the system on the UI thread");
    }

    /// Review 2026-10-24: Settings > System > Time listed the zoneinfo tree when it opened, read the zone from
    /// /etc/localtime in every painted frame and in every key press, and read a zone file per listed city on the first
    /// frame of a list. Now the page opens with "…", and the worker reads; this walks every page of it with a spy that
    /// records each look made on the test's thread, before and after the answers are in.
    #[test]
    fn the_time_page_never_looks_at_the_system_on_the_ui_thread() {
        let spy = Arc::new(Spy::new(MockHw::default().with_zoneinfo("/nonexistent")));
        let dir = TestDir::new("time-spy");
        // A real worker thread, as in the app.
        let mut s = Sys::new(spy.clone(), dir.to_path_buf(), true);
        s.rocknix_keys = false;
        let mut text = crate::text::Text::builtin();
        let mut px = vec![0u32; 1280 * 960];
        let mut paint = |s: &mut Sys| {
            let mut c = crate::gfx::Canvas::new(&mut px, 1280, 960);
            s.paint(&mut c, &mut text, false);
        };
        let wait = |s: &mut Sys, what: &str, ready: &dyn Fn(&Sys) -> bool| {
            let until = Instant::now() + Duration::from_secs(5);
            while !ready(s) && Instant::now() < until {
                std::thread::sleep(Duration::from_millis(2));
                s.tick(&SHELF, Instant::now());
            }
            assert!(ready(s), "the worker never answered: {what}");
        };
        let now = Instant::now();
        // Opened before the worker answered: the summary says "…", Time zone > regions has Search… and a line.
        s.open_time(now);
        assert!(s.view.timezone.is_none() && s.view.zone_offset.is_none() && s.view.zones.is_none(), "nothing read yet");
        paint(&mut s);
        tap(&mut s, Button::East, &SHELF, now);
        assert!(matches!(s.screen, Screen::Time(TimePage::Regions { .. })), "A opens the regions at once");
        paint(&mut s);
        tap(&mut s, Button::Down, &SHELF, now);
        tap(&mut s, Button::South, &SHELF, now);
        // The answers come in.
        wait(&mut s, "zone, offset and list", &|s| s.view.timezone.is_some() && s.view.zone_offset.is_some() && s.view.zones.is_some());
        assert_eq!(s.view.timezone, Some(Some("America/New_York".to_string())));
        paint(&mut s);
        // Regions, a region's cities (their offsets come from the worker too), B back, the search.
        tap(&mut s, Button::East, &SHELF, now);
        paint(&mut s);
        for _ in 0..3 {
            tap(&mut s, Button::Down, &SHELF, now);
            paint(&mut s);
        }
        tap(&mut s, Button::East, &SHELF, now);
        let Screen::Time(TimePage::Zones { zones, .. }) = &s.screen else { panic!("a region's cities") };
        let first = zones[0].clone();
        paint(&mut s);
        wait(&mut s, "the cities' offsets", &|s| s.offset(&first).is_some());
        paint(&mut s);
        tap(&mut s, Button::Down, &SHELF, now);
        tap(&mut s, Button::South, &SHELF, now);
        tap(&mut s, Button::South, &SHELF, now);
        let Screen::Time(page) = &mut s.screen else { panic!("the summary") };
        *page = TimePage::Regions { sel: 0 };
        tap(&mut s, Button::East, &SHELF, now);
        let Screen::Time(TimePage::Search(kb)) = &mut s.screen else { panic!("keyboard") };
        kb.text = "vien".into();
        paint(&mut s);
        tap(&mut s, Button::Start, &SHELF, now);
        paint(&mut s);
        // Choosing the city writes the zone (a switch, not a look) and the summary knows it at once.
        tap(&mut s, Button::East, &SHELF, now);
        assert_eq!(s.view.timezone, Some(Some("Europe/Vienna".to_string())));
        paint(&mut s);
        // The look every 2 s while the page is open.
        s.refresh(now);
        wait(&mut s, "a second look", &|s| s.view.seq >= 3);
        paint(&mut s);
        assert_eq!(*spy.on_ui.lock().unwrap(), Vec::<&str>::new(), "looked at the system on the UI thread");
    }

    /// A Vol step before the level was known: the worker steps from the real level, the OSD follows on the next tick.
    #[test]
    fn a_step_before_the_first_look_starts_from_the_real_level() {
        let hw = Arc::new(MockHw::default().with_zoneinfo("/nonexistent"));
        hw.set_volume(80);
        let dir = TestDir::new("quick-cold");
        let mut s = Sys::for_test(hw.clone(), dir.to_path_buf());
        let now = Instant::now();
        tap(&mut s, Button::VolUp, &SHELF, now);
        assert_eq!(hw.volume(), Some(85), "stepped from 80, not from a guess");
        assert!(s.osd.is_none() && s.animating(now), "the OSD waits for the answer");
        s.tick(&SHELF, now + Duration::from_millis(5));
        assert_eq!(s.osd.as_ref().map(|o| o.data.clone()), Some(Data::percent("Volume", 85)));
    }

    fn power_choices(s: &Sys) -> Vec<Choice> {
        match &s.screen {
            Screen::Power(m) => m.choices.clone(),
            _ => panic!("the power menu is not open"),
        }
    }

    #[test]
    fn the_power_menu_offers_the_boot_menu_only_where_the_device_can_do_it() {
        let (mut s, _) = sys();
        s.open_power(Instant::now());
        assert!(s.power_menu_open() && s.is_open());
        assert_eq!(power_choices(&s), [Choice::Sleep, Choice::Restart, Choice::PowerOff, Choice::RestartUi, Choice::BootMenu]);

        let hw = Arc::new(MockHw::new(crate::system_ctl::MockState {
            boot_menu: false,
            ..Default::default()
        }));
        let dir = TestDir::new("quick-nb");
        let mut s = Sys::for_test(hw, dir.to_path_buf());
        s.scratch = Some(dir);
        s.open_power(Instant::now());
        assert_eq!(power_choices(&s), [Choice::Sleep, Choice::Restart, Choice::PowerOff, Choice::RestartUi]);
    }

    #[test]
    fn the_menu_knows_whether_a_game_is_under_it_and_shows_the_battery_even_without_one() {
        let (mut s, _) = sys();
        s.open_power_in(Instant::now(), true);
        let m = s.power_menu().unwrap();
        assert!(m.ctx.game, "the lines say the game is saved first");
        assert!(!m.battery.time.is_empty(), "something to read at the top, a battery or not");
        let (mut s, _) = sys();
        s.open_power(Instant::now());
        assert!(!s.power_menu().unwrap().ctx.game);
    }

    #[test]
    fn each_power_menu_entry_runs_its_own_system_action_and_closes_the_menu() {
        let expected = [
            (Choice::Sleep, PowerAction::Sleep),
            (Choice::Restart, PowerAction::Reboot),
            (Choice::PowerOff, PowerAction::PowerOff),
            (Choice::RestartUi, PowerAction::RestartUi),
            (Choice::BootMenu, PowerAction::RebootBootloader),
        ];
        for (row, (choice, action)) in expected.into_iter().enumerate() {
            let (mut s, hw) = sys();
            let now = Instant::now();
            s.open_power(now);
            for _ in 0..row {
                tap(&mut s, Button::Down, &SHELF, now);
            }
            assert_eq!(power_choices(&s)[row], choice);
            tap(&mut s, Button::East, &SHELF, now);
            assert_eq!(hw.state().power_log, [action], "{choice:?}");
            assert!(!s.is_open(), "{choice:?}: the menu closes");
        }
    }

    #[test]
    fn b_and_a_second_power_press_close_the_power_menu_and_do_nothing_else() {
        for closer in [Button::South, Button::Power] {
            let (mut s, hw) = sys();
            let now = Instant::now();
            s.open_power(now);
            tap(&mut s, Button::Down, &SHELF, now);
            let out = tap(&mut s, closer, &SHELF, now);
            assert!(!s.is_open(), "{closer:?}");
            assert!(hw.state().power_log.is_empty(), "{closer:?}");
            let press = key(&mut s, closer, true, &SHELF, now);
            assert!(!press.power_cancelled, "{closer:?}: closed already");
            let _ = out;
        }
        let (mut s, _) = sys();
        s.open_power(Instant::now());
        assert!(key(&mut s, Button::South, true, &SHELF, Instant::now()).power_cancelled, "main puts the game back");
    }

    #[test]
    fn the_selection_wraps_and_other_buttons_stay_out_of_the_games_way() {
        let (mut s, hw) = sys();
        let now = Instant::now();
        s.open_power(now);
        tap(&mut s, Button::Up, &SHELF, now);
        assert!(matches!(&s.screen, Screen::Power(m) if m.sel == 4), "Up from the first entry wraps to the last");
        tap(&mut s, Button::Down, &SHELF, now);
        assert!(matches!(&s.screen, Screen::Power(m) if m.sel == 0));
        for b in [Button::West, Button::North, Button::Start, Button::Select, Button::L1] {
            assert!(key(&mut s, b, true, &SHELF, now).consumed, "{b:?} belongs to the menu");
        }
        assert!(s.power_menu_open());
        assert!(hw.state().power_log.is_empty());
    }

    #[test]
    fn the_power_menu_comes_up_over_the_quick_settings_and_gives_them_back() {
        let (mut s, _) = sys();
        let now = Instant::now();
        tap(&mut s, Button::Mode, &SHELF, now);
        assert!(s.quick_open());
        s.open_power(now);
        assert!(!s.quick_open() && s.power_menu_open());
        tap(&mut s, Button::South, &SHELF, now);
        assert!(s.quick_open(), "B closes only the power menu");
    }

    #[test]
    fn releases_pass_through_but_presses_inside_a_screen_do_not() {
        let (mut s, _) = sys();
        let now = Instant::now();
        tap(&mut s, Button::Mode, &SHELF, now);
        assert!(key(&mut s, Button::Down, true, &SHELF, now).consumed);
        assert!(
            !key(&mut s, Button::Down, false, &SHELF, now).consumed,
            "main must see the release of a key it saw go down"
        );
    }

    // ---- Date and time, set by hand (manual_time) ----------------------------------------------------------------

    /// 3 March 2000: the clock of a Pocket whose RTC lost its power.
    const DEAD_RTC: i64 = 952_077_300;

    /// A Pocket with no Wi-Fi and no time server (the first start offline), its clock as `clock`, UTC as its zone, and
    /// what setting the clock answers.
    fn offline(clock: i64, answer: Result<ClockSet, String>) -> (Sys, Arc<MockHw>) {
        let state = crate::system_ctl::MockState { wifi: false, wifi_wanted: false, ntp: NtpState::Waiting, clock_answer: answer, ..Default::default() };
        let hw = Arc::new(MockHw::new(state).with_zoneinfo("/nonexistent"));
        let dir = TestDir::new("quick-clock");
        let mut s = Sys::for_test(hw.clone(), dir.to_path_buf());
        s.scratch = Some(dir);
        s.offset_at = |_| 0;
        s.clock_fake = Some(clock);
        s.open_time(Instant::now());
        s.tick(&SHELF, Instant::now());
        (s, hw)
    }

    fn picker(s: &Sys) -> Option<Picker> {
        match &s.screen {
            Screen::Time(TimePage::SetClock(p)) => Some(*p),
            _ => None,
        }
    }

    /// A moment a month after this version was built: a sane clock for as long as this test is run.
    fn a_sane_time() -> i64 {
        crate::clock_check::built_unix().max(crate::clock_check::FLOOR_UNIX) + 30 * 86_400
    }

    fn paint_page(s: &mut Sys) {
        let mut text = crate::text::Text::builtin();
        let mut px = vec![0u32; 1240 * 1080];
        let mut c = crate::gfx::Canvas::new(&mut px, 1240, 1080);
        s.paint(&mut c, &mut text, false);
    }

    /// The real-user run of 2026-10-26: no Wi-Fi, a dead clock, and the page only said "turn Wi-Fi on". Now the line
    /// names Date and time, the row opens a picker at this version's build day, and A sets the clock (through
    /// `Hw::set_clock`, never the computer's): the warning goes away.
    #[test]
    fn offline_with_a_wrong_clock_the_time_page_shows_a_way_forward() {
        let (mut s, hw) = offline(DEAD_RTC, Ok(ClockSet::Saved));
        let now = Instant::now();
        assert_eq!((s.view.wifi, s.view.ntp), (Some(false), Some(NtpState::Waiting)));
        let (line, clock) = s.time_note(0, DEAD_RTC);
        assert_eq!(clock, crate::clock_check::Clock::Wrong);
        assert!(line.contains("Set it under Date and time") && line.contains("turn Wi-Fi on"), "{line}");
        paint_page(&mut s);
        // Down twice: Date and time, which A opens (not "From network time": no server has set the clock).
        tap(&mut s, Button::Down, &SHELF, now);
        tap(&mut s, Button::Down, &SHELF, now);
        assert!(matches!(s.screen, Screen::Time(TimePage::Summary { sel: BY_HAND_ROW })));
        tap(&mut s, CONFIRM, &SHELF, now);
        let p = picker(&s).expect("A opens the picker");
        assert_eq!(p, Picker::start(DEAD_RTC, crate::clock_check::built_unix(), |_| 0), "at the build day, not in 2000");
        assert!(p.year >= crate::manual_time::FIRST_YEAR);
        paint_page(&mut s);
        // The player's time, then one minute on with Up on the minute (Left from the year wraps to it).
        let want = a_sane_time() - a_sane_time() % 60;
        let Screen::Time(TimePage::SetClock(p)) = &mut s.screen else { unreachable!() };
        *p = Picker::at(want - 60, 0);
        tap(&mut s, Button::Left, &SHELF, now);
        tap(&mut s, Button::Up, &SHELF, now);
        assert_eq!(picker(&s).unwrap().unix(|_| 0), want);
        tap(&mut s, CONFIRM, &SHELF, now);
        assert_eq!(hw.state().clock_log, [want], "set once, in UTC");
        assert_eq!(s.toast.as_ref().map(|t| t.0.as_str()), Some("Date and time set."));
        assert!(matches!(s.screen, Screen::Time(TimePage::Summary { sel: BY_HAND_ROW })), "back on the page, on the row");
        assert!(s.clock_by_hand && !s.view.busy_clock);
        assert_eq!(s.clock_fake, Some(want), "the made-up clock follows");
        // No warning any more, and offline with a clock set by hand is not "may be a little off" either.
        let (line, clock) = s.time_note(BY_HAND_ROW, want);
        assert_eq!(clock, crate::clock_check::Clock::Fine);
        assert!(line.starts_with("Set by hand.") && line.contains("network time sets the clock again"), "{line}");
        paint_page(&mut s);
    }

    #[test]
    fn b_leaves_the_picker_without_setting_anything() {
        let (mut s, hw) = offline(a_sane_time(), Ok(ClockSet::Saved));
        let now = Instant::now();
        s.screen = Screen::Time(TimePage::Summary { sel: BY_HAND_ROW });
        tap(&mut s, CONFIRM, &SHELF, now);
        let opened = picker(&s).unwrap();
        assert_eq!(opened, Picker::at(a_sane_time(), 0), "a sane clock opens as it is");
        tap(&mut s, Button::Up, &SHELF, now);
        tap(&mut s, CANCEL, &SHELF, now);
        assert!(matches!(s.screen, Screen::Time(TimePage::Summary { sel: BY_HAND_ROW })));
        assert!(hw.state().clock_log.is_empty());
        assert!(!s.clock_by_hand);
        // Another B closes the page as before.
        tap(&mut s, CANCEL, &SHELF, now);
        assert!(!s.is_open());
    }

    #[test]
    fn a_date_before_this_version_is_not_set() {
        let built = crate::clock_check::built_unix();
        if crate::clock_check::earliest(built) <= crate::clock_check::FLOOR_UNIX {
            return; // built on 1 or 2 January 2026: nothing the picker offers is before it
        }
        let (mut s, hw) = offline(DEAD_RTC, Ok(ClockSet::Saved));
        let now = Instant::now();
        s.screen = Screen::Time(TimePage::SetClock(Picker { year: 2026, month: 1, day: 1, hour: 0, minute: 0, sel: 0 }));
        tap(&mut s, CONFIRM, &SHELF, now);
        assert!(hw.state().clock_log.is_empty());
        assert_eq!(s.toast.as_ref().map(|t| t.0.as_str()), Some("That date is before this version was made."));
        assert!(picker(&s).is_some(), "the picker stays to correct the year");
    }

    #[test]
    fn with_network_time_synced_the_row_only_says_so() {
        let (mut s, hw) = sys();
        let now = Instant::now();
        s.open_time(now);
        s.tick(&SHELF, now);
        assert_eq!(s.view.ntp, Some(NtpState::Synced));
        s.screen = Screen::Time(TimePage::Summary { sel: BY_HAND_ROW });
        let (line, _) = s.time_note(BY_HAND_ROW, unix_now());
        assert!(line.contains("nothing to set by hand"), "{line}");
        tap(&mut s, CONFIRM, &SHELF, now);
        assert!(matches!(s.screen, Screen::Time(TimePage::Summary { .. })), "no picker");
        assert_eq!(s.toast.as_ref().map(|t| t.0.as_str()), Some("Network time keeps the clock right."));
        assert!(hw.state().clock_log.is_empty());
        paint_page(&mut s);
    }

    #[test]
    fn what_the_system_answers_is_said_and_only_a_real_set_counts() {
        let now = Instant::now();
        let set = |answer: Result<ClockSet, String>| {
            let (mut s, hw) = offline(DEAD_RTC, answer);
            s.screen = Screen::Time(TimePage::SetClock(Picker::at(a_sane_time(), 0)));
            tap(&mut s, CONFIRM, &SHELF, now);
            assert_eq!(hw.state().clock_log.len(), 1);
            (s.toast.clone().map(|t| t.0).unwrap_or_default(), s.clock_by_hand, picker(&s).is_some())
        };
        // The RTC refused: the clock is set until the next start, and the page says so.
        assert_eq!(set(Ok(ClockSet::ClockOnly("RTC_SET_TIME: EPERM".into()))), ("Set, but only until the next restart.".into(), true, false));
        // Nothing set (the virtual device's mock): said, and the warning stays.
        assert_eq!(set(Ok(ClockSet::Mock)), ("Not set: this is a virtual device.".into(), false, true));
        // The system refused: its words, the picker stays.
        assert_eq!(set(Err("The system didn't take the new time.".into())), ("The system didn't take the new time.".into(), false, true));
    }

    /// The picker on a touch screen: a field selects it (one tap, however far), the selected one's top and bottom
    /// halves are Up and Down, the hints are B and A.
    #[test]
    fn a_finger_picks_a_field_and_changes_it() {
        use crate::touch::TapAction;
        let (mut s, _) = offline(a_sane_time(), Ok(ClockSet::Saved));
        s.screen = Screen::Time(TimePage::SetClock(Picker::at(a_sane_time(), 0)));
        paint_page(&mut s);
        let mut seen = Vec::new();
        for y in (0..1080).step_by(6) {
            for x in (0..1240).step_by(6) {
                let t = s.hits.tap(x as f32, y as f32);
                if t != TapAction::Nothing && !seen.contains(&t) {
                    seen.push(t);
                }
            }
        }
        for want in [
            TapAction::Press(Button::Up),
            TapAction::Press(Button::Down),
            TapAction::Press(Button::Right),
            TapAction::Presses(Button::Right, 2),
            TapAction::Presses(Button::Right, 3),
            TapAction::Presses(Button::Right, 4),
            TapAction::Press(CANCEL),
            TapAction::Press(CONFIRM),
        ] {
            assert!(seen.contains(&want), "{want:?} in {seen:?}");
        }
        assert!(!seen.iter().any(|t| matches!(t, TapAction::Steps(_) | TapAction::Activate)), "no list rows: Up/Down would change a value");
    }

    /// The clock is set on the worker, never on the UI thread, and the answer comes back with the next tick.
    #[test]
    fn setting_the_clock_never_runs_on_the_ui_thread() {
        let state = crate::system_ctl::MockState { wifi: false, wifi_wanted: false, ntp: NtpState::Waiting, clock_answer: Ok(ClockSet::Saved), ..Default::default() };
        let spy = Arc::new(Spy::new(MockHw::new(state).with_zoneinfo("/nonexistent")));
        let dir = TestDir::new("clock-spy");
        let mut s = Sys::new(spy.clone(), dir.to_path_buf(), true);
        s.rocknix_keys = false;
        s.offset_at = |_| 0;
        let now = Instant::now();
        s.open_time(now);
        s.screen = Screen::Time(TimePage::SetClock(Picker::at(a_sane_time(), 0)));
        tap(&mut s, CONFIRM, &SHELF, now);
        let until = Instant::now() + Duration::from_secs(5);
        while !s.clock_by_hand && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(2));
            s.tick(&SHELF, Instant::now());
        }
        assert!(s.clock_by_hand, "the worker never answered");
        assert_eq!(spy.inner.state().clock_log.len(), 1);
        assert!(!spy.on_ui.lock().unwrap().iter().any(|w| w.starts_with("set_clock")), "set on the UI thread");
    }
}
