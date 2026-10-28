//! Switch off after sleeping (docs/SLEEP.md "L3 auto-off"): deep sleep costs about 4.8 % an hour on a Mini V2
//! (s2idle with a PS1 game open, 27 % in 5.63 h, measured 2026-10-25), so a full battery is empty after some 20 hours
//! asleep: a weekend in a bag. After the time chosen in Settings → System → Sleep & power ("Switch off after sleeping": Off,
//! 1 h, 3 h, 8 h; Off by default since 2026-10-26, 3 h costs about 14 %) the Pocket saves, leaves the resume note and powers off, and the next
//! start goes straight back into the game.
//!
//! How (`sleep.rs` `deep_sequence` runs it on the deep sleep's worker thread):
//!
//! - The time counts from the first suspend of a dark spell on CLOCK_BOOTTIME, which runs on in suspend (`Plan`).
//!   Before each suspend the RTC alarm is set for what is left of it (`Rtc`, the `wakealarm` file
//!   `scripts/latency.sh --wake` uses too).
//! - Back from a suspend, the clock alone says whether it can be the alarm (`Plan::due`): a wake before that (the power
//!   key, the harness's own alarm) is a normal wake and nothing more is done on its way; the alarm is cleared after
//!   the screen and the game are on.
//! - The alarm's wake turns nothing on. After ROCKNIX's wake hooks (the charger's state settles meanwhile) `decide`
//!   says: the power key pressed meanwhile → a normal wake (someone is there); on the charger, or the battery can't be
//!   read → asleep again, the alarm a full period on (so a Pocket unplugged later still switches off); on battery →
//!   switch off, through the power menu's Power off (the game saves and quits, `resume.json`, `systemctl poweroff`),
//!   with the screen dark and a note for the next start (`Note`, "Switched off after 3 h asleep to save the
//!   battery.").
//! - A suspend that keeps failing leaves the Pocket dark in light sleep (`Timers::deep_wait`); once the time is up
//!   there on battery it switches off the same way (`Sleep::tick`).
//!
//! The low-battery guard (`low_battery`) is unchanged: at 2 % it saves and switches off whatever this says.

use std::path::{Path, PathBuf};

use crate::low_battery::Level;

/// The choices in hours (None = never).
pub const CHOICES: [Option<u32>; 4] = [None, Some(1), Some(3), Some(8)];
/// Off (owner's decision, 2026-10-26): a Pocket put to sleep stays asleep, as on ROCKNIX and NextUI, and no wake alarm
/// is ever armed. The owner found one he had expected asleep hard off after 3 h. Switching off after sleeping is a
/// choice under Settings → System → Sleep & power; 3 h costs about 14 % of the battery at the 4.8 %/h measured asleep
/// (2026-10-25), 1 h would cost a boot each time a Pocket is put down for an evening, 8 h would let a night take 40 %.
/// A saved `sleep.json` keeps what it says.
pub const DEFAULT: Option<u32> = None;
/// A wake this long before the alarm still counts as the alarm's: the RTC counts whole seconds, and the kernel's
/// resume and the hooks take their moment.
const EARLY_SECS: f64 = 5.0;
const RTC_ALARM: &str = "/sys/class/rtc/rtc0/wakealarm";
const NOTE_FILE: &str = "switched-off.json";

/// "Off", "1 h", "3 h", "8 h".
pub fn label(hours: Option<u32>) -> String {
    hours.map_or("Off".into(), |h| format!("{h} h"))
}

/// The line under the setting on Settings → System → Sleep & power (`sleep_settings`, which steps through `CHOICES`).
pub fn explain(hours: Option<u32>) -> &'static str {
    match hours {
        Some(_) => "After this long asleep on battery, the Pocket saves the game and switches off.",
        None => "The Pocket stays asleep until you wake it.",
    }
}

/// Seconds on CLOCK_BOOTTIME, which runs on while the system is suspended.
pub fn clock() -> f64 {
    #[cfg(target_os = "linux")]
    {
        let mut ts: libc::timespec = unsafe { std::mem::zeroed() };
        // SAFETY: clock_gettime only writes into the provided struct.
        unsafe { libc::clock_gettime(libc::CLOCK_BOOTTIME, &mut ts) };
        ts.tv_sec as f64 + ts.tv_nsec as f64 / 1e9
    }
    #[cfg(not(target_os = "linux"))]
    {
        use std::sync::OnceLock;
        static START: OnceLock<std::time::Instant> = OnceLock::new();
        START.get_or_init(std::time::Instant::now).elapsed().as_secs_f64()
    }
}

/// When this dark spell switches off, on `clock()`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Plan {
    pub at: f64,
    /// The chosen time in seconds (an hour is shorter with `POCKETUI_SLEEP_MINUTE_SECS`).
    pub every: f64,
    /// The chosen hours, for the note.
    pub hours: u32,
}

impl Plan {
    pub fn new(now: f64, hours: u32, hour_secs: f64) -> Plan {
        let every = hours as f64 * hour_secs;
        Plan { at: now + every, every, hours }
    }

    /// Whole seconds from `now` to the alarm, at least one.
    pub fn secs_left(&self, now: f64) -> u64 {
        (self.at - now).ceil().max(1.0) as u64
    }

    /// The time is up: a wake now can be the alarm's.
    pub fn due(&self, now: f64) -> bool {
        now >= self.at - EARLY_SECS
    }

    /// Another full period from `now` (asleep again on the charger, or a time that passed while nothing could act).
    pub fn again(self, now: f64) -> Plan {
        Plan { at: now + self.every, ..self }
    }
}

/// What the alarm's wake leads to.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Verdict {
    /// The power key was pressed meanwhile: someone is there, it wakes as always.
    Wake,
    /// On the charger, or the battery can't be read: asleep again, the screen never on.
    SleepOn,
    /// Save, quit the game and power off, the screen dark.
    SwitchOff,
}

/// The alarm's wake (`Plan::due`), after ROCKNIX's wake hooks. `key_pressed`: the power key went down since the suspend
/// was asked for; `power`: the battery read now (`low_battery::read`).
pub fn decide(key_pressed: bool, power: Option<Level>) -> Verdict {
    if key_pressed {
        return Verdict::Wake;
    }
    match power {
        Some(level) if !level.external_power => Verdict::SwitchOff,
        _ => Verdict::SleepOn,
    }
}

/// The RTC's wake alarm (`POCKETUI_RTC_ALARM` replaces the file for tests and the virtual device,
/// `POCKETUI_RTC_PROC` the `/proc/driver/rtc` read back after a power off's disarm).
///
/// Every clone shares one gate: once `disarm_for_shutdown` has run, no alarm can be set again (a suspend worker that
/// was about to arm one finds the gate shut), until `reopen` (the power off failed and the Pocket goes on).
#[derive(Clone, Debug)]
pub struct Rtc {
    path: PathBuf,
    proc_path: Option<PathBuf>,
    /// true: a power off or restart is under way, nothing may arm the alarm. Held while the alarm is written, so
    /// the disarm and a concurrent `set` never interleave.
    shut: std::sync::Arc<std::sync::Mutex<Gate>>,
}

/// The state behind the gate.
#[derive(Debug, Default)]
struct Gate {
    shut: bool,
    /// This process has set an alarm and not cleared it since: what is on the RTC is not stale (`clear_stale_at_start`).
    armed_by_us: bool,
}

/// What `Rtc::disarm_for_shutdown` found and did; `Display` is the line for the log.
#[derive(Clone, Debug, PartialEq)]
pub struct Disarm {
    /// The wakealarm file before: "none" = no alarm, else its text (an epoch second), "unreadable" if it couldn't be read.
    pub before: String,
    pub writes_ok: bool,
    pub after: String,
    /// `/proc/driver/rtc` lines, when the file exists.
    pub alarm_irq: Option<String>,
    pub alrm_pending: Option<String>,
    pub tries: u32,
    /// The RTC's own clock in Unix seconds (`since_epoch`), to see an alarm "in the past" for what it is.
    pub rtc_now: Option<String>,
    /// The alarm file reads back as no alarm (or there is no RTC) and the RTC's alarm interrupt is off (or not reported).
    /// `alrm_pending` is only logged: the enable bit is what powers the PMIC on.
    pub verified: bool,
}

impl std::fmt::Display for Disarm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "RTC alarm disarm {} (before={} write={} after={} alarm_IRQ={} alrm_pending={} rtc_now={} tries={})",
            if self.verified { "verified" } else { "NOT verified" },
            self.before,
            if self.writes_ok { "ok" } else { "failed" },
            self.after,
            self.alarm_irq.as_deref().unwrap_or("n/a"),
            self.alrm_pending.as_deref().unwrap_or("n/a"),
            self.rtc_now.as_deref().unwrap_or("n/a"),
            self.tries
        )
    }
}

/// Writes of 0 tried before giving up (the power off goes on regardless).
const DISARM_TRIES: u32 = 3;
const PROC_RTC: &str = "/proc/driver/rtc";

impl Rtc {
    /// The RTC of this process: one gate for all callers (the suspend workers, every power-off path).
    pub fn from_env() -> Rtc {
        #[cfg(not(test))]
        {
            static GLOBAL: std::sync::OnceLock<Rtc> = std::sync::OnceLock::new();
            GLOBAL.get_or_init(Rtc::new_from_env).clone()
        }
        // Tests never touch the machine's own RTC unless they point `POCKETUI_RTC_ALARM` at a file.
        #[cfg(test)]
        {
            let mut rtc = Rtc::new_from_env();
            if std::env::var_os("POCKETUI_RTC_ALARM").is_none() {
                rtc.path = std::env::temp_dir().join("pocketui-no-such-rtc/wakealarm");
                rtc.proc_path = None;
            }
            rtc
        }
    }

    fn new_from_env() -> Rtc {
        let custom = std::env::var_os("POCKETUI_RTC_ALARM");
        let proc_path = match std::env::var_os("POCKETUI_RTC_PROC") {
            Some(p) => Some(PathBuf::from(p)),
            None if custom.is_some() => None,
            None => Some(PathBuf::from(PROC_RTC)),
        };
        Rtc {
            path: custom.map_or_else(|| PathBuf::from(RTC_ALARM), PathBuf::from),
            proc_path,
            shut: Default::default(),
        }
    }

    #[cfg(test)]
    pub fn at(path: PathBuf) -> Rtc {
        Rtc { path, proc_path: None, shut: Default::default() }
    }

    #[cfg(test)]
    pub fn with_proc(mut self, proc_path: PathBuf) -> Rtc {
        self.proc_path = Some(proc_path);
        self
    }

    fn gate(&self) -> std::sync::MutexGuard<'_, Gate> {
        self.shut.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The alarm `secs` from now, or none. The kernel refuses a new alarm while one is set, so it is cleared first.
    /// Refused (false) once a power off has disarmed the alarm for good.
    pub fn set(&self, secs: Option<u64>) -> bool {
        let mut gate = self.gate();
        if secs.is_some() && gate.shut {
            return false;
        }
        let cleared = std::fs::write(&self.path, "0").is_ok();
        match secs {
            None => {
                gate.armed_by_us &= !cleared;
                cleared
            }
            Some(s) => {
                let set = std::fs::write(&self.path, format!("+{s}")).is_ok();
                gate.armed_by_us = set;
                set
            }
        }
    }

    /// A power off or restart has begun: nothing may arm the alarm from now on. The disarm itself follows later
    /// (`disarm_for_shutdown`, after the game is saved and quit); `reopen` undoes both.
    pub fn close(&self) {
        self.gate().shut = true;
    }

    /// True once a power off has disarmed the alarm (`reopen` undoes it): no new suspend should start.
    pub fn shut(&self) -> bool {
        self.gate().shut
    }

    /// The alarm file's state: "none" when no alarm is set (the kernel prints nothing), "no-rtc" when there is no such
    /// file, else the epoch second (or "unreadable").
    fn read_alarm(&self) -> String {
        match std::fs::read_to_string(&self.path) {
            Ok(t) => match t.trim() {
                "" | "0" => "none".into(),
                other => other.to_string(),
            },
            // No RTC here (the virtual device): there is no alarm to leave armed.
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => "no-rtc".into(),
            Err(_) => "unreadable".into(),
        }
    }

    fn proc_field(&self, key: &str) -> Option<String> {
        let text = std::fs::read_to_string(self.proc_path.as_ref()?).ok()?;
        text.lines().find_map(|l| {
            let (k, v) = l.split_once(':')?;
            (k.trim() == key).then(|| v.trim().to_string())
        })
    }

    /// The RTC's own clock (`since_epoch` next to the alarm file).
    fn rtc_now(&self) -> Option<String> {
        let t = std::fs::read_to_string(self.path.with_file_name("since_epoch")).ok()?;
        Some(t.trim().to_string()).filter(|t| !t.is_empty())
    }

    /// The one RTC disarm before every power off and restart: clears the wake alarm, reads it back (and the RTC's
    /// own `alarm_IRQ` / `alrm_pending`), tries again up to [`DISARM_TRIES`] times, and shuts the gate so no suspend
    /// worker can arm it again. Never blocks the power off: a failure is in the result, not an error.
    pub fn disarm_for_shutdown(&self) -> Disarm {
        self.disarm(true)
    }

    /// At pocket-ui's start: an alarm left armed by a run that ended without clearing it (a crash, a restart, a failed
    /// clear) has nobody to answer it, and the next power off would meet it. Looks first and writes only when
    /// something is armed (`tries` 0 = nothing to do). The gate stays open.
    pub fn clear_stale_at_start(&self) -> Disarm {
        self.disarm(false)
    }

    fn disarm(&self, shutting_down: bool) -> Disarm {
        let mut gate = self.gate();
        if shutting_down {
            gate.shut = true;
        }
        let before = self.read_alarm();
        let (mut writes_ok, mut tries) = (true, 0);
        loop {
            let alarm_irq = self.proc_field("alarm_IRQ");
            let armed = !matches!(before.as_str(), "none" | "no-rtc") || alarm_irq.as_deref() == Some("yes");
            // At start: nothing armed, or armed by this very process (a first deep sleep that got there before this
            // check): not stale, left alone.
            if tries == 0 && !shutting_down && (!armed || gate.armed_by_us) {
                let after = before.clone();
                let alrm_pending = self.proc_field("alrm_pending");
                return Disarm { before, writes_ok, after, alarm_irq, alrm_pending, tries, rtc_now: self.rtc_now(), verified: true };
            }
            tries += 1;
            writes_ok &= std::fs::write(&self.path, "0").is_ok();
            let after = self.read_alarm();
            let alarm_irq = self.proc_field("alarm_IRQ");
            let alrm_pending = self.proc_field("alrm_pending");
            let verified = matches!(after.as_str(), "none" | "no-rtc") && alarm_irq.as_deref() != Some("yes");
            if verified || tries >= DISARM_TRIES {
                return Disarm { before, writes_ok, after, alarm_irq, alrm_pending, tries, rtc_now: self.rtc_now(), verified };
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }

    /// True when the alarm file exists (there is an RTC to arm).
    pub fn present(&self) -> bool {
        self.path.exists()
    }

    /// The power off didn't happen: alarms may be set again.
    pub fn reopen(&self) {
        self.gate().shut = false;
    }
}

/// What every power-off and restart path calls right before its command: disarms the alarm (`Rtc::disarm_for_shutdown`),
/// and writes the readback to the log and to `shutdown.log`. `who` names the path. Never fails: the power off goes on.
pub fn disarm_before_power_off(rtc: &Rtc, data_dir: &Path, who: &str) -> Disarm {
    let result = rtc.disarm_for_shutdown();
    if result.verified {
        println!("shutdown: {who}: {result}");
    } else {
        eprintln!("shutdown: {who}: {result}; powering off anyway");
    }
    crate::shutdown_log::event(data_dir, &format!("{who}: {result}"));
    result
}

/// At pocket-ui's start: clears an alarm nobody is left to answer and logs what it found, always (one line in the log
/// and in `shutdown.log`, so the next boot's evidence has the RTC's state from the start too). The plan lives in
/// memory only, so at start no alarm can belong to a switch-off under way: that sequence runs inside one process, and a
/// start is a new one. Never fails.
pub fn check_at_start(rtc: &Rtc, data_dir: &Path) -> Disarm {
    let result = rtc.clear_stale_at_start();
    let armed = result.tries > 0;
    if result.verified {
        println!("start: {}{result}", if armed { "a stale RTC alarm was armed; cleared: " } else { "no RTC alarm armed: " });
    } else {
        eprintln!("start: a stale RTC alarm could not be cleared: {result}");
    }
    crate::shutdown_log::event(data_dir, &format!("start: {result}"));
    result
}

/// CLOCK_REALTIME minus CLOCK_BOOTTIME: moves only when the wall clock is set or stepped (a suspend moves both).
pub fn wall_offset() -> f64 {
    let wall = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0.0, |d| d.as_secs_f64());
    wall - clock()
}

/// A wall clock step this big between arming the alarm and the wake is no wake (`sleep.rs` `deep_sequence`).
pub const CLOCK_STEP_SECS: f64 = EARLY_SECS;

/// Left for the next start by a switch-off after sleeping: what it says once, and the screen's brightness (the
/// backlight was 0 when the Pocket went down).
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
pub struct Note {
    pub hours: u32,
    #[serde(default)]
    pub brightness: Option<u32>,
}

impl Note {
    pub fn text(&self) -> String {
        if crate::device::current().screen_off_only() {
            format!("Switched off after {} h with the screen off to save the battery.", self.hours)
        } else {
            format!("Switched off after {} h asleep to save the battery.", self.hours)
        }
    }
}

pub fn write_note(data_dir: &Path, note: &Note) {
    if let Ok(json) = serde_json::to_vec(note) {
        let _ = crate::fsutil::write_atomic(&data_dir.join(NOTE_FILE), &json);
    }
}

pub fn clear_note(data_dir: &Path) {
    let _ = std::fs::remove_file(data_dir.join(NOTE_FILE));
}

/// The note of the last switch-off after sleeping, once (it is removed).
pub fn take_note(data_dir: &Path) -> Option<Note> {
    let path = data_dir.join(NOTE_FILE);
    let bytes = std::fs::read(&path).ok()?;
    let _ = std::fs::remove_file(&path);
    serde_json::from_slice(&bytes).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;

    #[test]
    fn choices_and_labels() {
        assert_eq!(label(DEFAULT), "Off", "opt-in");
        assert_eq!(label(None), "Off");
        assert_eq!(CHOICES.map(label), ["Off", "1 h", "3 h", "8 h"]);
    }

    #[test]
    fn the_three_hour_choice_costs_about_a_seventh_of_the_battery_and_is_not_the_default() {
        // 4.8 %/h asleep, measured on a Mini V2 (2026-10-25).
        let cost = 3.0 * 4.8;
        assert!((10.0..=15.0).contains(&cost), "{cost}");
        assert!(CHOICES.contains(&Some(3)) && DEFAULT.is_none());
    }

    #[test]
    fn the_plan_counts_on_from_the_first_suspend() {
        let p = Plan::new(100.0, 3, 3600.0);
        assert_eq!(p.at, 100.0 + 3.0 * 3600.0);
        assert_eq!(p.secs_left(100.0), 10_800);
        assert_eq!(p.secs_left(10_850.5), 50, "after a suspend that failed: only what is left");
        assert_eq!(p.secs_left(20_000.0), 1, "never 0 (that would clear the alarm)");
        assert!(!p.due(10_000.0), "the power key an hour before");
        assert!(p.due(p.at - 2.0), "the RTC counts whole seconds");
        assert!(p.due(p.at + 30.0));
        assert_eq!(p.again(p.at).at, p.at + p.every);
    }

    #[test]
    fn only_on_battery_and_with_nobody_there_does_it_switch_off() {
        let bat = |percent, external_power| Some(Level { percent, external_power });
        assert_eq!(decide(false, bat(60, false)), Verdict::SwitchOff);
        assert_eq!(decide(false, bat(60, true)), Verdict::SleepOn, "on the charger it stays asleep");
        assert_eq!(decide(false, None), Verdict::SleepOn, "a battery that can't be read is no reason");
        assert_eq!(decide(true, bat(60, false)), Verdict::Wake, "the power key at that moment wakes it");
        assert_eq!(decide(true, bat(60, true)), Verdict::Wake);
    }

    #[test]
    fn the_rtc_alarm_is_cleared_before_it_is_set() {
        let dir = TestDir::new("autooff-rtc");
        let file = dir.join("wakealarm");
        let rtc = Rtc::at(file.clone());
        assert!(rtc.set(Some(3600)));
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "+3600");
        assert!(rtc.set(None));
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "0");
        assert!(!Rtc::at(dir.join("no/such/rtc")).set(Some(5)), "no RTC: false, nothing else");
    }

    fn rtc_dir(name: &str) -> crate::testutil::TestDir {
        crate::testutil::TestDir::new(&format!("autooff-{name}"))
    }

    #[test]
    fn an_armed_alarm_is_cleared_and_read_back_before_a_power_off() {
        let dir = rtc_dir("disarm");
        let file = dir.join("wakealarm");
        let proc_file = dir.join("rtc");
        std::fs::write(&proc_file, "rtc_time\t: 22:00:00\nalarm_IRQ\t: no\nalrm_pending\t: no\n").unwrap();
        // A real one, and one whose time has passed long ago: both must be gone.
        for armed in ["1793051031", "1000"] {
            std::fs::write(&file, armed).unwrap();
            let result = Rtc::at(file.clone()).with_proc(proc_file.clone()).disarm_for_shutdown();
            assert_eq!(std::fs::read_to_string(&file).unwrap(), "0");
            assert_eq!((result.before.as_str(), result.after.as_str()), (armed, "none"));
            assert!(result.verified && result.writes_ok && result.tries == 1, "{result}");
            assert_eq!((result.alarm_irq.as_deref(), result.alrm_pending.as_deref()), (Some("no"), Some("no")));
            let line = result.to_string();
            assert!(line.contains("verified") && line.contains(armed) && line.contains("alarm_IRQ=no"), "{line}");
        }
        // Nothing armed: still verified (the kernel prints an empty file).
        std::fs::write(&file, "").unwrap();
        assert!(Rtc::at(file).disarm_for_shutdown().verified);
    }

    #[test]
    fn an_alarm_interrupt_that_stays_on_is_tried_again_and_then_reported_not_verified() {
        let dir = rtc_dir("irq");
        let file = dir.join("wakealarm");
        let proc_file = dir.join("rtc");
        std::fs::write(&file, "").unwrap();
        std::fs::write(&proc_file, "alarm_IRQ\t: yes\nalrm_pending\t: yes\n").unwrap();
        let result = Rtc::at(file).with_proc(proc_file).disarm_for_shutdown();
        assert!(!result.verified && result.tries == DISARM_TRIES, "{result}");
        assert!(result.to_string().contains("NOT verified") && result.to_string().contains("alrm_pending=yes"), "{result}");
    }

    #[test]
    fn a_failing_write_is_reported_and_never_stops_the_caller() {
        let dir = rtc_dir("fails");
        // A directory where the file should be: every write and read fails.
        let file = dir.join("wakealarm");
        std::fs::create_dir(&file).unwrap();
        let result = Rtc::at(file).disarm_for_shutdown();
        assert!(!result.verified && !result.writes_ok, "{result}");
        assert_eq!(result.after, "unreadable");
        // No RTC at all (the virtual device): nothing that could be left armed.
        let none = Rtc::at(dir.join("no/such/rtc")).disarm_for_shutdown();
        assert!(none.verified && none.after == "no-rtc", "{none}");
    }

    #[test]
    fn after_the_disarm_no_alarm_can_be_set_until_the_power_off_failed() {
        let dir = rtc_dir("gate");
        let file = dir.join("wakealarm");
        let rtc = Rtc::at(file.clone());
        assert!(rtc.set(Some(60)) && !rtc.shut());
        let other = rtc.clone();
        assert!(rtc.disarm_for_shutdown().verified && other.shut(), "every clone shares the gate");
        assert!(!other.set(Some(60)), "a suspend that comes too late arms nothing");
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "0");
        assert!(other.set(None), "clearing is always allowed");
        rtc.reopen();
        assert!(other.set(Some(60)) && !other.shut());
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "+60");
    }

    #[test]
    fn a_suspend_worker_racing_the_disarm_never_leaves_the_alarm_armed() {
        use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
        use std::sync::Arc;
        // Waits for a counter to move, yielding; a worker that never gets there fails the test rather than hanging it.
        fn wait_for(counter: &AtomicU32, what: &str) {
            let end = std::time::Instant::now() + std::time::Duration::from_secs(30);
            while counter.load(Ordering::SeqCst) == 0 {
                assert!(std::time::Instant::now() < end, "the worker never {what}");
                std::thread::yield_now();
            }
        }
        for round in 0..20 {
            let dir = rtc_dir(&format!("race{round}"));
            let file = dir.join("wakealarm");
            let rtc = Rtc::at(file.clone());
            let stop = Arc::new(AtomicBool::new(false));
            let (armed, refused) = (Arc::new(AtomicU32::new(0)), Arc::new(AtomicU32::new(0)));
            // The auto-off worker: arms before every suspend, as fast as it can.
            let worker = {
                let (rtc, stop, armed, refused) = (rtc.clone(), stop.clone(), armed.clone(), refused.clone());
                std::thread::spawn(move || {
                    while !stop.load(Ordering::SeqCst) {
                        let counter = if rtc.set(Some(30)) { &armed } else { &refused };
                        counter.fetch_add(1, Ordering::SeqCst);
                    }
                })
            };
            // The disarm comes while the worker is arming (not before its first arm, which would prove nothing) ...
            wait_for(&armed, "armed an alarm");
            let result = rtc.disarm_for_shutdown();
            assert!(result.verified, "{result}");
            // ... and the worker is seen to try again and be refused, before it is told to stop (no sleeps as sync).
            wait_for(&refused, "found the gate shut");
            stop.store(true, Ordering::SeqCst);
            worker.join().unwrap();
            assert_eq!(std::fs::read_to_string(&file).unwrap(), "0", "round {round}: armed again after the disarm");
        }
    }

    #[test]
    fn a_stale_alarm_is_cleared_at_start_and_a_clean_rtc_is_left_alone() {
        let dir = rtc_dir("start");
        let file = dir.join("wakealarm");
        std::fs::write(dir.join("since_epoch"), "1793051500\n").unwrap();
        let rtc = Rtc::at(file.clone());
        // Armed by a run that ended without clearing it (long past): cleared and verified, the gate still open.
        std::fs::write(&file, "1793040000").unwrap();
        let result = check_at_start(&rtc, &dir);
        assert!(result.verified && result.tries == 1 && result.before == "1793040000", "{result}");
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "0");
        assert_eq!(result.rtc_now.as_deref(), Some("1793051500"));
        assert!(rtc.set(Some(30)) && !rtc.shut(), "a start is no power off: sleeping can arm its alarm");
        // Nothing armed: nothing written (an empty file stays empty), and said all the same.
        std::fs::write(&file, "").unwrap();
        let result = check_at_start(&rtc, &dir);
        assert!(result.verified && result.tries == 0, "{result}");
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "");
        let log = std::fs::read_to_string(dir.join("logs/shutdown.log")).unwrap();
        assert_eq!(log.matches("start: RTC alarm disarm verified").count(), 2, "{log}");
        assert!(log.contains("before=1793040000") && log.contains("rtc_now=1793051500"), "{log}");
        // An alarm interrupt left on with an empty alarm file is cleared too; one that cannot be cleared is said.
        let proc_file = dir.join("rtc");
        std::fs::write(&proc_file, "alarm_IRQ\t: yes\n").unwrap();
        let stuck = Rtc::at(file).with_proc(proc_file).clear_stale_at_start();
        assert!(!stuck.verified && stuck.tries == DISARM_TRIES, "{stuck}");
    }

    #[test]
    fn the_start_check_leaves_an_alarm_this_process_armed_and_close_shuts_the_gate_without_touching_the_file() {
        let dir = rtc_dir("own");
        let file = dir.join("wakealarm");
        let rtc = Rtc::at(file.clone());
        // A first deep sleep armed its alarm before the start check got to run: not stale.
        assert!(rtc.set(Some(600)));
        let result = rtc.clear_stale_at_start();
        assert!(result.verified && result.tries == 0 && result.before == "+600", "{result}");
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "+600");
        // Cleared by us, then something else arms it: that one is stale.
        assert!(rtc.set(None));
        std::fs::write(&file, "1793040000").unwrap();
        assert!(rtc.clear_stale_at_start().tries == 1);
        // close(): the gate is shut at once, the file untouched; reopen opens it.
        std::fs::write(&file, "1793040000").unwrap();
        rtc.close();
        assert!(rtc.shut() && !rtc.set(Some(5)));
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "1793040000");
        rtc.reopen();
        assert!(!rtc.shut());
    }

    #[test]
    fn the_power_off_helper_logs_the_readback_even_when_the_alarm_cannot_be_cleared() {
        let dir = rtc_dir("helper");
        let file = dir.join("wakealarm");
        std::fs::create_dir(&file).unwrap();
        let result = disarm_before_power_off(&Rtc::at(file), &dir, "PowerOff");
        assert!(!result.verified);
        let log = std::fs::read_to_string(dir.join("logs/shutdown.log")).unwrap();
        assert!(log.contains("PowerOff: RTC alarm disarm NOT verified"), "{log}");
    }

    #[test]
    fn the_note_is_read_once() {
        let dir = TestDir::new("autooff-note");
        assert_eq!(take_note(&dir), None);
        write_note(&dir, &Note { hours: 3, brightness: Some(120) });
        let note = take_note(&dir).unwrap();
        assert_eq!(note.text(), "Switched off after 3 h asleep to save the battery.");
        assert_eq!(note.brightness, Some(120));
        assert_eq!(take_note(&dir), None);
        std::fs::write(dir.join(NOTE_FILE), "garbage").unwrap();
        assert_eq!(take_note(&dir), None, "a broken note says nothing");
        assert!(!dir.join(NOTE_FILE).exists(), "and is gone");
    }
}
