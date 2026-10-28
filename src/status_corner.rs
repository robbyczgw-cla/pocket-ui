//! The status corner at the top right of Home and the pages over it: the time, a Wi-Fi symbol while connected, the
//! battery. Settings, its pages, the pause menu's pages and the quick settings show the same parts in a pill (`Pill`).
//! What it shows comes from memory: Wi-Fi and battery from a thread that reads three sysfs files every few seconds,
//! only while the corner or the pill is on screen (`set_shown`); the time from `localtime_r`, at most once a second.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::gfx::Canvas;
use crate::text::{Face, Text};

/// The corner's type size (the size of the line it shares with the page's place, left).
pub const PX: u32 = 28;
/// Between the time, the Wi-Fi symbol and the battery.
pub const GAP: i32 = 24;
/// Between the corner and what stands left of it ("Saves synced", a page's title).
pub const ROOM: i32 = 32;
/// A clock before this (2024-01-01 UTC) was never set: no time is shown rather than "00:00" from 1970.
pub const TIME_SET_AFTER: i64 = 1_704_067_200;
/// How often the thread reads Wi-Fi and the battery while the corner is on screen.
pub const EVERY: Duration = Duration::from_secs(3);
/// The wake-up for a new minute comes this long after it, so the minute has surely turned.
const MINUTE_MARGIN: Duration = Duration::from_millis(20);

/// The battery as the corner shows it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Battery {
    pub percent: u8,
    pub charging: bool,
}

impl Battery {
    /// "87 %", "87 %, charging".
    pub fn label(&self) -> String {
        format!("{}%{}", self.percent, if self.charging { ", charging" } else { "" })
    }
}

/// What the thread last read.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Status {
    /// Connected to a network (the Wi-Fi interface's `operstate` is "up").
    pub wifi: bool,
    pub battery: Option<Battery>,
}

/// One corner as painted: each part only when it is known.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Corner {
    pub clock: Option<String>,
    pub wifi: bool,
    pub battery: Option<String>,
}

/// A piece of the corner, left to right.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Part<'a> {
    Text(&'a str),
    Wifi,
}

impl Corner {
    /// The corner now: the time, and Wi-Fi and battery as the thread last read them.
    pub fn now() -> Corner {
        let s = status();
        Corner { clock: clock(), wifi: s.wifi, battery: s.battery.map(|b| b.label()) }
    }

    /// Only a battery figure (the tour's previews, tests).
    pub fn battery_only(label: &str) -> Corner {
        Corner { battery: Some(label.to_string()), ..Corner::default() }
    }

    /// Time, Wi-Fi, battery: what is known of them, in that order.
    pub fn parts(&self) -> Vec<Part<'_>> {
        let mut parts = Vec::with_capacity(3);
        if let Some(t) = &self.clock {
            parts.push(Part::Text(t));
        }
        if self.wifi {
            parts.push(Part::Wifi);
        }
        if let Some(b) = &self.battery {
            parts.push(Part::Text(b));
        }
        parts
    }

    /// Where each part starts when the corner ends at `right`, and the corner's left edge (`right` when it is
    /// empty). `width` measures a text; the Wi-Fi symbol is `wifi_width(PX)` wide.
    pub fn layout(&self, right: i32, width: impl FnMut(&str) -> i32) -> (Vec<(Part<'_>, i32)>, i32) {
        self.layout_at(right, PX, width)
    }

    /// `layout` at type size `px` (the pill of Settings and the quick settings is 30 px).
    pub fn layout_at(&self, right: i32, px: u32, mut width: impl FnMut(&str) -> i32) -> (Vec<(Part<'_>, i32)>, i32) {
        let parts = self.parts();
        let mut x = right;
        let mut placed: Vec<(Part<'_>, i32)> = Vec::with_capacity(parts.len());
        for part in parts.into_iter().rev() {
            if !placed.is_empty() {
                x -= GAP;
            }
            x -= match part {
                Part::Text(s) => width(s),
                Part::Wifi => wifi_width(px),
            };
            placed.push((part, x));
        }
        placed.reverse();
        (placed, x)
    }

    /// Paints the corner ending at `right` on `baseline` in `colour`; returns its left edge.
    pub fn paint(&self, c: &mut Canvas, text: &mut Text, right: i32, baseline: i32, colour: u32) -> i32 {
        let (placed, left) = self.layout(right, |s| text.width(Face::Body, PX, s));
        for (part, x) in placed {
            match part {
                Part::Text(s) => {
                    text.draw(c, Face::Body, PX, x, baseline, colour, s);
                }
                Part::Wifi => paint_wifi(c, x, baseline, PX, colour),
            }
        }
        left
    }
}

/// The corner in a pill: Settings and its pages, the pause menu's pages and the quick settings, whose top line is a
/// row of pills. The same parts as on Home (time, Wi-Fi while connected, battery), at type size `px`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pill {
    /// The pill's right edge and top, its height.
    pub right: i32,
    pub top: i32,
    pub h: i32,
    pub px: u32,
    /// Room inside the pill at either end.
    pub pad: i32,
}

impl Pill {
    /// Each part's x and the pill's left edge, for `corner`; None when nothing is known (no empty pill is drawn).
    pub fn layout<'a>(&self, corner: &'a Corner, width: impl FnMut(&str) -> i32) -> Option<(Vec<(Part<'a>, i32)>, i32)> {
        let (placed, left) = corner.layout_at(self.right - self.pad, self.px, width);
        (!placed.is_empty()).then_some((placed, left - self.pad))
    }

    /// Paints `corner` in the pill on `fill` in `colour`; returns the pill's left edge (`right` when nothing is shown),
    /// for a heading left of it to stop short of.
    pub fn paint(&self, c: &mut Canvas, text: &mut Text, corner: &Corner, fill: u32, colour: u32) -> i32 {
        let px = self.px;
        let Some((placed, left)) = self.layout(corner, |s| text.width(Face::Body, px, s)) else { return self.right };
        c.round_rect(left, self.top, self.right - left, self.h, self.h / 2, fill);
        let baseline = self.top + self.h / 2 + crate::type_scale::mid(px);
        for (part, x) in placed {
            match part {
                Part::Text(s) => {
                    text.draw(c, Face::Body, px, x, baseline, colour, s);
                }
                Part::Wifi => paint_wifi(c, x, baseline, px, colour),
            }
        }
        left
    }
}

/// How far text left of a corner whose left edge is `left` may reach: `ROOM` short of it, or all the way to
/// `right` when the corner is empty.
pub fn room_left_of(left: i32, right: i32) -> i32 {
    if left < right {
        left - ROOM
    } else {
        right
    }
}

/// The Wi-Fi symbol's width at type size `px` (its height is `wifi_height`).
pub fn wifi_width(px: u32) -> i32 {
    // A quarter circle stood on its point: twice the radius times sin 45°.
    (wifi_height(px) as f32 * std::f32::consts::SQRT_2).round() as i32
}

/// A little taller than the figures beside it (0.84 of the type size): the fan is narrow at the bottom, so it
/// looks as big as they do.
pub fn wifi_height(px: u32) -> i32 {
    (px as f32 * 0.84).round() as i32
}

/// The Wi-Fi symbol: a dot-like wedge and two arcs fanning up from a point on the baseline, 45° to either side,
/// anti-aliased (4 × 4 samples a pixel). `x` is its left edge. The coverage of each pixel is worked out once per size
/// (`WifiMask`: 15 000 samples with a square root each, 0.1 ms on the build computer, more on the Mini V2), by the
/// status corner's thread before the first frame that needs it; the frame only blends.
pub fn paint_wifi(c: &mut Canvas, x: i32, baseline: i32, px: u32, colour: u32) {
    let mask = wifi_mask(px);
    for (row, line) in mask.cover.chunks(mask.cols).enumerate() {
        let py = baseline - mask.above + row as i32;
        for (col, &alpha) in line.iter().enumerate() {
            if alpha > 0 {
                c.blend(x - 1 + col as i32, py, colour, alpha);
            }
        }
    }
}

/// The coverage (0-255) of every pixel of the Wi-Fi symbol at one type size, relative to its left edge and baseline.
struct WifiMask {
    /// Pixels per row of `cover`; the first column is the one left of the symbol's left edge.
    cols: usize,
    /// Rows above the baseline the first row of `cover` is.
    above: i32,
    cover: Vec<u8>,
}

/// The masks made so far, by type size (two sizes in use: the corner's and the pill's).
static WIFI_MASKS: Mutex<Vec<(u32, std::sync::Arc<WifiMask>)>> = Mutex::new(Vec::new());

fn wifi_mask(px: u32) -> std::sync::Arc<WifiMask> {
    let mut masks = WIFI_MASKS.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((_, m)) = masks.iter().find(|(p, _)| *p == px) {
        return m.clone();
    }
    let mask = std::sync::Arc::new(make_wifi_mask(px));
    masks.push((px, mask.clone()));
    mask
}

/// Makes the masks of the sizes in use before a frame wants them.
pub fn prepare_wifi(sizes: &[u32]) {
    for &px in sizes {
        wifi_mask(px);
    }
}

fn make_wifi_mask(px: u32) -> WifiMask {
    let r = wifi_height(px) as f32;
    let w = wifi_width(px);
    // The point is at the middle of the symbol's width, on the baseline (relative coordinates: x 0, baseline 0).
    let cx = w as f32 / 2.0;
    let above = r.ceil() as i32 + 1;
    let cols = (w + 2) as usize;
    let mut cover = Vec::with_capacity(cols * (above + 1) as usize);
    for py in -above..=0 {
        for pxl in -1..=w {
            let mut hits = 0u32;
            for sy in 0..4 {
                for sx in 0..4 {
                    let (dx, dy) = (pxl as f32 + (sx as f32 + 0.5) / 4.0 - cx, -(py as f32 + (sy as f32 + 0.5) / 4.0));
                    if wifi_covers(dx / r, dy / r) {
                        hits += 1;
                    }
                }
            }
            cover.push((hits * 255 / 16) as u8);
        }
    }
    WifiMask { cols, above, cover }
}

/// Whether the point (`dx`, `dy`), in radii from the symbol's point (up is positive), is ink.
fn wifi_covers(dx: f32, dy: f32) -> bool {
    if dy <= 0.0 || dx.abs() > dy {
        return false;
    }
    let d = (dx * dx + dy * dy).sqrt();
    d <= 0.30 || (0.47..=0.67).contains(&d) || (0.83..=1.0).contains(&d)
}

/// "up" is connected; "down", "dormant" (not yet through the handshake), "lowerlayerdown" and the rest are not.
pub fn parse_operstate(s: &str) -> bool {
    s.trim() == "up"
}

/// The battery from its sysfs `capacity` and `status`; None without a readable capacity.
pub fn parse_battery(capacity: Option<&str>, status: Option<&str>) -> Option<Battery> {
    let percent = capacity?.trim().parse::<u8>().ok()?.min(100);
    Some(Battery { percent, charging: status.is_some_and(|s| s.trim() == "Charging") })
}

/// Reads Wi-Fi (`<net>/operstate`) and the battery (`<battery>/capacity`, `status`): three small sysfs files.
pub fn read(net: &Path, battery: &Path) -> Status {
    let file = |p: PathBuf| std::fs::read_to_string(p).ok();
    Status {
        wifi: file(net.join("operstate")).is_some_and(|s| parse_operstate(&s)),
        battery: parse_battery(file(battery.join("capacity")).as_deref(), file(battery.join("status")).as_deref()),
    }
}

/// "13:28" for a local hour and minute at the Unix time `unix`; None while the clock was never set.
pub fn format_clock(unix: i64, hour: i32, minute: i32) -> Option<String> {
    (unix >= TIME_SET_AFTER).then(|| format!("{hour:02}:{minute:02}"))
}

/// Local time as "13:28" (24 hours, as everywhere in pocket-ui), None while the clock was never set. Worked out at
/// most once a second: a zone chosen in Settings shows within that second.
pub fn clock() -> Option<String> {
    static KEPT: Mutex<Option<(i64, Option<String>)>> = Mutex::new(None);
    let now = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs() as i64;
    let mut kept = KEPT.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((_, s)) = kept.as_ref().filter(|(at, _)| *at == now) {
        return s.clone();
    }
    let s = local_clock(now);
    *kept = Some((now, s.clone()));
    s
}

fn local_clock(now: i64) -> Option<String> {
    let t = now as libc::time_t;
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    // A time zone chosen in Settings applies at once.
    crate::timezone::tzset();
    // SAFETY: localtime_r only writes into the provided struct.
    if unsafe { libc::localtime_r(&t, &mut tm) }.is_null() {
        return None;
    }
    format_clock(now, tm.tm_hour, tm.tm_min)
}

/// How long from `since_epoch` (the wall clock) until just after the next minute begins. Zones are whole minutes
/// from UTC, so UTC's minute is the local one.
pub fn until_next_minute(since_epoch: Duration) -> Duration {
    let into = Duration::new(since_epoch.as_secs() % 60, since_epoch.subsec_nanos());
    Duration::from_secs(60) - into + MINUTE_MARGIN
}

/// `until_next_minute` from now.
pub fn next_minute_in() -> Duration {
    until_next_minute(SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default())
}

// The thread. `STATUS` is None until its first read; `SHOWN` says whether the corner is on screen (set by the UI on
// every pass of its loop: two atomics), `PARKED` that the thread sleeps until it is again.
static STATUS: Mutex<Option<Status>> = Mutex::new(None);
static SHOWN: AtomicBool = AtomicBool::new(false);
static PARKED: AtomicBool = AtomicBool::new(false);
static THREAD: OnceLock<std::thread::Thread> = OnceLock::new();
/// Snapshots: 0 = as read, 1 = Wi-Fi off, 2 = connected (`force_wifi`).
static FORCED_WIFI: AtomicU8 = AtomicU8::new(0);

/// Starts the thread that reads Wi-Fi and the battery every `EVERY` while the corner is on screen and sleeps
/// (parked, no wake-ups) while it is not. `notify` is called from it when what it read changed.
pub fn start(notify: impl Fn() + Send + 'static) {
    // The virtual device has no Wi-Fi chip: POCKETUI_WIFI_IFACE=eth0 there stands in for it (docs/VIRTUAL-DEVICE.md).
    let iface = std::env::var("POCKETUI_WIFI_IFACE").unwrap_or_else(|_| "wlan0".into());
    let net = Path::new("/sys/class/net").join(iface);
    let battery = crate::battery::dir();
    let spawned = std::thread::Builder::new().name("status-corner".into()).spawn(move || {
        // The symbol's pixels for the corner and the pill, before the first frame wants them.
        prepare_wifi(&[PX, 30]);
        let mut last = None;
        loop {
            let now = read(&net, &battery);
            if last != Some(now) {
                last = Some(now);
                *STATUS.lock().unwrap_or_else(|e| e.into_inner()) = Some(now);
                notify();
            }
            // Asleep, in a game or on a page without the corner: park until it is shown again. The flag goes up
            // before `SHOWN` is looked at again, so a `set_shown(true)` in between always unparks.
            if !SHOWN.load(Ordering::SeqCst) {
                PARKED.store(true, Ordering::SeqCst);
                while !SHOWN.load(Ordering::SeqCst) {
                    std::thread::park();
                }
                PARKED.store(false, Ordering::SeqCst);
                continue;
            }
            std::thread::sleep(EVERY);
        }
    });
    if let Ok(handle) = spawned {
        let _ = THREAD.set(handle.thread().clone());
    }
}

/// Whether the corner is on screen right now (the UI, after each frame); unparks the thread when it comes back.
pub fn set_shown(shown: bool) {
    SHOWN.store(shown, Ordering::SeqCst);
    if shown && PARKED.swap(false, Ordering::SeqCst) {
        if let Some(t) = THREAD.get() {
            t.unpark();
        }
    }
}

/// What the thread read last. Without the thread (snapshots, tests) read now, from the same files.
pub fn status() -> Status {
    let kept = *STATUS.lock().unwrap_or_else(|e| e.into_inner());
    let mut s = kept.unwrap_or_else(|| {
        if THREAD.get().is_some() {
            return Status::default();
        }
        let iface = std::env::var("POCKETUI_WIFI_IFACE").unwrap_or_else(|_| "wlan0".into());
        read(&Path::new("/sys/class/net").join(iface), &crate::battery::dir())
    });
    match FORCED_WIFI.load(Ordering::Relaxed) {
        1 => s.wifi = false,
        2 => s.wifi = true,
        _ => {}
    }
    s
}

/// Whether Wi-Fi is connected now, read afresh (`operstate`, one small file) instead of the thread's last look, which
/// sleeps on pages without the corner (the Wi-Fi page): Settings → Connections right after a connect. Not on the UI thread.
pub fn wifi_now() -> bool {
    match FORCED_WIFI.load(Ordering::Relaxed) {
        1 => false,
        2 => true,
        _ => {
            let iface = std::env::var("POCKETUI_WIFI_IFACE").unwrap_or_else(|_| "wlan0".into());
            std::fs::read_to_string(Path::new("/sys/class/net").join(iface).join("operstate")).is_ok_and(|s| parse_operstate(&s))
        }
    }
}

/// Snapshots (`POCKETUI_SNAPSHOT_WIFI`): the corner as if Wi-Fi were connected or not, whatever this computer has.
pub fn force_wifi(connected: bool) {
    FORCED_WIFI.store(if connected { 2 } else { 1 }, Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;

    fn widths(s: &str) -> i32 {
        // 14 px a character: easy to add up.
        s.chars().count() as i32 * 14
    }

    #[test]
    fn the_corner_is_time_then_wifi_then_battery_each_only_when_known() {
        let all = Corner { clock: Some("13:28".into()), wifi: true, battery: Some("87%, charging".into()) };
        assert_eq!(all.parts(), vec![Part::Text("13:28"), Part::Wifi, Part::Text("87%, charging")]);
        let (placed, left) = all.layout(1000, widths);
        let ww = wifi_width(PX);
        let battery_x = 1000 - 13 * 14;
        let wifi_x = battery_x - GAP - ww;
        let clock_x = wifi_x - GAP - 5 * 14;
        assert_eq!(placed, vec![(Part::Text("13:28"), clock_x), (Part::Wifi, wifi_x), (Part::Text("87%, charging"), battery_x)]);
        assert_eq!(left, clock_x);

        // Wi-Fi off or not connected: nothing in its place, the time moves up to the battery.
        let offline = Corner { wifi: false, battery: Some("87%".into()), ..all.clone() };
        let (placed, left) = offline.layout(1000, widths);
        assert_eq!(placed, vec![(Part::Text("13:28"), 1000 - 3 * 14 - GAP - 5 * 14), (Part::Text("87%"), 1000 - 3 * 14)]);
        assert_eq!(left, 1000 - 3 * 14 - GAP - 5 * 14);
    }

    #[test]
    fn without_a_battery_the_time_and_wifi_still_show_and_an_empty_corner_takes_no_room() {
        let no_battery = Corner { clock: Some("09:05".into()), wifi: true, battery: None };
        let (placed, left) = no_battery.layout(500, widths);
        assert_eq!(placed, vec![(Part::Text("09:05"), 500 - wifi_width(PX) - GAP - 70), (Part::Wifi, 500 - wifi_width(PX))]);
        assert_eq!(left, 500 - wifi_width(PX) - GAP - 70);
        // No clock either (never set): Wi-Fi alone, flush right.
        let wifi_only = Corner { clock: None, ..no_battery };
        let (placed, left) = wifi_only.layout(500, widths);
        assert_eq!(placed, vec![(Part::Wifi, 500 - wifi_width(PX))]);
        assert_eq!(left, 500 - wifi_width(PX));
        // Nothing known: nothing painted, and the note left of it keeps the whole line.
        assert_eq!(Corner::default().layout(500, widths), (vec![], 500));
        assert_eq!(Corner::battery_only("50%").layout(500, widths).1, 500 - 42);
    }

    #[test]
    fn the_pill_holds_the_same_parts_as_the_corner_and_is_not_drawn_empty() {
        let pill = Pill { right: 1210, top: 30, h: 70, px: 30, pad: 36 };
        let all = Corner { clock: Some("13:28".into()), wifi: true, battery: Some("87%, charging".into()) };
        let (placed, left) = pill.layout(&all, widths).expect("a pill");
        let parts: Vec<Part> = placed.iter().map(|(p, _)| *p).collect();
        assert_eq!(parts, all.parts(), "time, Wi-Fi, battery: as on Home");
        // The battery ends `pad` inside the right edge, the time starts `pad` inside the left one.
        assert_eq!(placed[2].1 + 13 * 14, 1210 - 36);
        assert_eq!(placed[0].1, left + 36);
        // Wider than the old "13:28   87%" pill by the Wi-Fi symbol and ", charging".
        assert_eq!(1210 - left, 2 * 36 + 5 * 14 + GAP + wifi_width(30) + GAP + 13 * 14);
        // Wi-Fi off: the symbol is left out and the pill closes up.
        let offline = Corner { wifi: false, ..all.clone() };
        assert_eq!(pill.layout(&offline, widths).unwrap().0.len(), 2);
        // Nothing known: no pill at all, and a heading may run to the edge.
        assert_eq!(pill.layout(&Corner::default(), widths), None);
    }

    #[test]
    fn the_battery_reads_as_before_with_charging_spelled_out() {
        assert_eq!(parse_battery(Some("87\n"), Some("Discharging\n")).map(|b| b.label()).as_deref(), Some("87%"));
        assert_eq!(parse_battery(Some("87\n"), Some("Charging\n")).map(|b| b.label()).as_deref(), Some("87%, charging"));
        // Full on the charger is not "charging"; no status file is not either.
        assert_eq!(parse_battery(Some("100"), Some("Full")), Some(Battery { percent: 100, charging: false }));
        assert_eq!(parse_battery(Some("42"), None), Some(Battery { percent: 42, charging: false }));
        // No battery, or nonsense in its file: no figure at all.
        assert_eq!(parse_battery(None, Some("Charging")), None);
        assert_eq!(parse_battery(Some(""), None), None);
        assert_eq!(parse_battery(Some("abc"), None), None);
    }

    #[test]
    fn only_an_interface_that_is_up_counts_as_connected() {
        assert!(parse_operstate("up\n"));
        for s in ["down\n", "dormant\n", "lowerlayerdown\n", "unknown\n", "notpresent", ""] {
            assert!(!parse_operstate(s), "{s:?}");
        }
    }

    #[test]
    fn read_takes_wifi_and_battery_from_their_files_and_copes_without_them() {
        let dir = TestDir::new("corner");
        let (net, bat) = (dir.join("net"), dir.join("bat"));
        std::fs::create_dir_all(&net).unwrap();
        std::fs::create_dir_all(&bat).unwrap();
        // Neither there: nothing, and no error.
        assert_eq!(read(&dir.join("none"), &dir.join("none")), Status::default());
        std::fs::write(net.join("operstate"), "up\n").unwrap();
        std::fs::write(bat.join("capacity"), "64\n").unwrap();
        std::fs::write(bat.join("status"), "Charging\n").unwrap();
        assert_eq!(read(&net, &bat), Status { wifi: true, battery: Some(Battery { percent: 64, charging: true }) });
        std::fs::write(net.join("operstate"), "dormant\n").unwrap();
        std::fs::remove_file(bat.join("capacity")).unwrap();
        assert_eq!(read(&net, &bat), Status { wifi: false, battery: None });
    }

    #[test]
    fn a_clock_that_was_never_set_shows_no_time() {
        assert_eq!(format_clock(0, 0, 0), None);
        assert_eq!(format_clock(TIME_SET_AFTER - 1, 23, 59), None);
        assert_eq!(format_clock(TIME_SET_AFTER, 0, 0).as_deref(), Some("00:00"));
        assert_eq!(format_clock(1_792_000_000, 9, 5).as_deref(), Some("09:05"));
    }

    #[test]
    fn the_wake_comes_just_after_the_next_minute_turns() {
        let at = |s: u64, ms: u32| until_next_minute(Duration::new(s, ms * 1_000_000));
        // A minute's first instant (1_792_000_020 is a whole number of minutes): the next is a whole minute away.
        let start = 1_792_000_020;
        assert_eq!(start % 60, 0);
        assert_eq!(at(start, 0), Duration::from_secs(60) + MINUTE_MARGIN);
        // 59.5 s into the minute: half a second.
        assert_eq!(at(start + 59, 500), Duration::from_millis(500) + MINUTE_MARGIN);
        // 1.25 s in.
        assert_eq!(at(start + 1, 250), Duration::from_millis(58_750) + MINUTE_MARGIN);
        // Never more than a minute (and the margin), never zero.
        for s in 0..120 {
            let d = at(start + s, 999);
            assert!(d > MINUTE_MARGIN && d <= Duration::from_secs(60) + MINUTE_MARGIN, "{d:?}");
        }
    }

    /// The symbol worked out pixel by pixel in the frame, as `paint_wifi` did before the masks.
    fn wifi_directly(c: &mut Canvas, x: i32, baseline: i32, px: u32, colour: u32) {
        let r = wifi_height(px) as f32;
        let w = wifi_width(px);
        let cx = x as f32 + w as f32 / 2.0;
        let apex = baseline as f32;
        for py in (baseline - r.ceil() as i32 - 1)..=baseline {
            for pxl in x - 1..=x + w {
                let mut hits = 0u32;
                for sy in 0..4 {
                    for sx in 0..4 {
                        let (dx, dy) = (pxl as f32 + (sx as f32 + 0.5) / 4.0 - cx, apex - (py as f32 + (sy as f32 + 0.5) / 4.0));
                        if wifi_covers(dx / r, dy / r) {
                            hits += 1;
                        }
                    }
                }
                if hits > 0 {
                    c.blend(pxl, py, colour, (hits * 255 / 16) as u8);
                }
            }
        }
    }

    #[test]
    fn the_masks_paint_the_symbol_pixel_for_pixel_as_working_it_out_in_the_frame_did() {
        for (px, x, base) in [(PX, 10, 40), (30, 7, 33), (30, 100, 51), (45, 0, 60)] {
            let (mut a, mut b) = (vec![0x203040u32; 200 * 80], vec![0x203040u32; 200 * 80]);
            paint_wifi(&mut Canvas::new(&mut a, 200, 80), x, base, px, 0xf0e8d8);
            wifi_directly(&mut Canvas::new(&mut b, 200, 80), x, base, px, 0xf0e8d8);
            assert!(a == b, "size {px} at {x},{base}");
            assert!(a.iter().any(|p| *p != 0x203040), "something was painted");
        }
    }

    #[test]
    fn the_wifi_symbol_is_ink_at_its_point_and_arcs_and_clear_beside_them() {
        let (w, h) = (60u32, 40u32);
        let mut buf = vec![0u32; (w * h) as usize];
        let mut c = Canvas::new(&mut buf, w, h);
        let (x, base) = (10, 30);
        paint_wifi(&mut c, x, base, PX, 0xffffff);
        let at = |c: &Canvas, x: i32, y: i32| c.buf[(y * c.w + x) as usize] & 0xff;
        let (r, ww) = (wifi_height(PX), wifi_width(PX));
        let cx = x + ww / 2;
        // Up the middle: the wedge, a gap, the inner arc, a gap, the outer arc, then nothing.
        assert!(at(&c, cx, base - (r as f32 * 0.15) as i32) > 200);
        assert!(at(&c, cx, base - (r as f32 * 0.39) as i32 - 1) < 60);
        assert!(at(&c, cx, base - (r as f32 * 0.57) as i32 - 1) > 200);
        assert!(at(&c, cx, base - (r as f32 * 0.75) as i32 - 1) < 60);
        assert!(at(&c, cx, base - (r as f32 * 0.92) as i32 - 1) > 200);
        assert_eq!(at(&c, cx, base - r - 2), 0);
        // Below the baseline and past either side: untouched.
        assert!((0..w as i32).all(|px| at(&c, px, base + 1) == 0));
        assert!((0..h as i32).all(|py| at(&c, x - 2, py) == 0 && at(&c, x + ww + 1, py) == 0));
        // About as big as the figures beside it.
        assert_eq!((r, ww), (24, 34));
    }
}
