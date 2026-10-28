//! pocket-splash, the boot screen (docs/BOOT-SPLASH.md): its picture, its timing and the way a frame reaches the
//! screen, all without a screen, so the host tests run every part of it. `src/bin/pocket_splash.rs` is the program
//! around it: `/dev/fb0`, the console and the signals.
//!
//! The picture is one spine of the shelf on black: the "pocket-ui" wordmark (Barlow Condensed, Title 50, Chalk) on a
//! Spine body whose 12 px stripe crossfades through the consoles' colours, nothing else moving. It fades in (400 ms),
//! and out (250 ms) when sway starts or the program is told to stop. Two states say something in words, never a blank:
//! the first start after setting up a card ("Getting ready for the first start…") and a start that takes longer than
//! a minute ("Still starting…").
//!
//! Frames are painted in the screen's landscape size (`Scene`), only when something changes (`Timeline::next_change`:
//! nothing during the 1.2 s each colour holds), at most 30 a second, and only the part that changed is turned into the
//! frame buffer's pixel format and quarter turn (`convert`) and written.

use std::time::Duration;

use crate::gfx::{mix, Canvas};
use crate::settings::Motion;
use crate::text::{Face, Text};

pub const VOID: u32 = 0x000000;
pub const SPINE: u32 = 0x1F1F22;
pub const CHALK: u32 = 0xD9D6CE;
pub const DUST: u32 = 0x8A877F;
/// The stripe's colours in turn: NES, GB, GBC, GBA, SNES, N64 (docs/DESIGN.md, system stripes).
pub const STRIPES: [u32; 6] = [0xC8102E, 0x8A9A3B, 0x2E8C9A, 0x5B4AA6, 0x7A6CB0, 0x2E7D32];

/// The spine comes up from black.
pub const FADE_IN: Duration = Duration::from_millis(400);
/// Each stripe colour holds this long, then crossfades to the next in `CROSS`.
pub const HOLD: Duration = Duration::from_millis(1200);
pub const CROSS: Duration = Duration::from_millis(400);
/// From the spine to black when sway starts or the program is told to stop.
pub const FADE_OUT: Duration = Duration::from_millis(250);
/// After this, a line says the start is taking longer.
pub const LATE: Duration = Duration::from_secs(60);
/// After this (120 colours, about three minutes) the stripe holds still: nothing is drawn any more until the end.
pub const STILL_AFTER: Duration = Duration::from_millis(120 * 1600);
/// At most 30 frames a second.
pub const FRAME: Duration = Duration::from_millis(33);

const MARGIN: i32 = 72;
const STRIPE_W: i32 = 12;
const SPINE_H: i32 = 72;
const WORDMARK: &str = "pocket-ui";
const WORDMARK_PX: u32 = 50;
const NOTE_PX: u32 = 28;
/// The note's baseline above the bottom edge: where the hints sit.
const NOTE_FROM_BOTTOM: i32 = 44;

pub const FIRST_START: &str = "Getting ready for the first start…";
pub const STILL_STARTING: &str = "Still starting…";

/// The line under the picture.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Note {
    None,
    FirstStart,
    Late,
}

impl Note {
    pub fn text(self) -> &'static str {
        match self {
            Note::None => "",
            Note::FirstStart => FIRST_START,
            Note::Late => STILL_STARTING,
        }
    }
}

/// What one frame shows.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Look {
    /// How strongly the spine shows: 0 black, 255 full (the fades in and out).
    pub spine: u8,
    /// The stripe's colour at full strength.
    pub stripe: u32,
    pub note: Note,
    /// How strongly the note shows.
    pub note_a: u8,
}

/// `p` (0..1) eased in and out (half a sine wave): fades start and end softly.
fn ease(p: f32) -> f32 {
    let p = p.clamp(0.0, 1.0);
    (1.0 - (p * std::f32::consts::PI).cos()) / 2.0
}

fn strength(p: f32) -> u8 {
    (ease(p) * 255.0).round() as u8
}

fn ratio(t: Duration, d: Duration) -> f32 {
    t.as_secs_f32() / d.as_secs_f32()
}

/// When what is on screen changes, from the program's start (t = 0).
#[derive(Clone, Debug)]
pub struct Timeline {
    pub motion: Motion,
    pub first_start: bool,
    /// When the fade to black began.
    pub stop_at: Option<Duration>,
}

impl Timeline {
    pub fn new(motion: Motion, first_start: bool) -> Self {
        Self { motion, first_start, stop_at: None }
    }

    /// Fade to black from `t` on (only the first call counts).
    pub fn stop(&mut self, t: Duration) {
        self.stop_at.get_or_insert(t);
    }

    fn still(&self) -> bool {
        self.motion == Motion::Off
    }

    /// How much is left of the picture as it fades out: 255 before the stop, 0 at its end.
    fn remaining(&self, t: Duration) -> u8 {
        match self.stop_at {
            None => 255,
            Some(s) if t < s => 255,
            Some(_) if self.still() => 0,
            Some(s) => 255 - strength(ratio(t - s, FADE_OUT)),
        }
    }

    pub fn look(&self, t: Duration) -> Look {
        let up = if self.still() { 255 } else { strength(ratio(t, FADE_IN)) };
        let left = self.remaining(t) as u32;
        let (note, since) = if t >= LATE {
            (Note::Late, LATE)
        } else if self.first_start {
            (Note::FirstStart, Duration::ZERO)
        } else {
            (Note::None, Duration::ZERO)
        };
        let note_up = if self.still() || note == Note::None { 255 } else { strength(ratio(t - since, FADE_IN)) };
        Look {
            spine: (up as u32 * left / 255) as u8,
            stripe: self.stripe(t),
            note,
            note_a: if note == Note::None { 0 } else { (note_up as u32 * left / 255) as u8 },
        }
    }

    /// The stripe's colour at `t`: each colour holds, then crossfades to the next; still from `STILL_AFTER` on and
    /// with Motion Off.
    fn stripe(&self, t: Duration) -> u32 {
        if self.still() {
            return STRIPES[0];
        }
        let t = t.min(STILL_AFTER);
        let seg = HOLD + CROSS;
        let k = (t.as_millis() / seg.as_millis()) as usize;
        let within = t - seg * k as u32;
        let (a, b) = (STRIPES[k % STRIPES.len()], STRIPES[(k + 1) % STRIPES.len()]);
        if within < HOLD {
            a
        } else {
            mix(a, b, strength(ratio(within - HOLD, CROSS)) as u32)
        }
    }

    /// The fade to black is over: the program can end.
    pub fn finished(&self, t: Duration) -> bool {
        self.stop_at.is_some() && self.remaining(t) == 0
    }

    /// When the picture next changes after `t`: the next frame during a fade or a crossfade, the end of a hold, the
    /// minute the late note comes; None when nothing changes any more until the program is told to stop.
    pub fn next_change(&self, t: Duration) -> Option<Duration> {
        if self.finished(t) {
            return None;
        }
        let frame = t + FRAME;
        let fading = |from: Duration, d: Duration| !self.still() && t >= from && t < from + d;
        if fading(Duration::ZERO, FADE_IN) || self.stop_at.is_some_and(|s| t >= s) || fading(LATE, FADE_IN) {
            return Some(frame);
        }
        let mut next: Option<Duration> = (t < LATE).then_some(LATE);
        if !self.still() && t < STILL_AFTER {
            let seg = HOLD + CROSS;
            let k = (t.as_millis() / seg.as_millis()) as u32;
            let within = t - seg * k;
            let at = if within < HOLD { seg * k + HOLD } else { frame };
            next = Some(next.map_or(at, |n| n.min(at)));
        }
        if let Some(s) = self.stop_at.filter(|s| *s > t) {
            next = Some(next.map_or(s, |n| n.min(s)));
        }
        next
    }
}

/// A rectangle in the picture.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub fn union(self, o: Rect) -> Rect {
        let (x0, y0) = (self.x.min(o.x), self.y.min(o.y));
        let (x1, y1) = ((self.x + self.w).max(o.x + o.w), (self.y + self.h).max(o.y + o.h));
        Rect { x: x0, y: y0, w: x1 - x0, h: y1 - y0 }
    }

    /// Cut to a `w` x `h` picture.
    pub fn clip(self, w: i32, h: i32) -> Rect {
        let (x0, y0) = (self.x.clamp(0, w), self.y.clamp(0, h));
        let (x1, y1) = ((self.x + self.w).clamp(0, w), (self.y + self.h).clamp(0, h));
        Rect { x: x0, y: y0, w: (x1 - x0).max(0), h: (y1 - y0).max(0) }
    }

    pub fn is_empty(self) -> bool {
        self.w <= 0 || self.h <= 0
    }
}

/// The picture at one screen size (landscape as the player holds the Pocket).
pub struct Scene {
    pub w: i32,
    pub h: i32,
    text: Text,
}

impl Scene {
    /// The embedded Barlow only: no theme fonts and no system font are opened during the boot.
    pub fn new(w: u32, h: u32) -> Self {
        Self { w: w as i32, h: h as i32, text: Text::builtin() }
    }

    pub fn spine_rect(&self) -> Rect {
        Rect { x: MARGIN, y: (self.h - SPINE_H) / 2, w: (self.w - 2 * MARGIN).max(STRIPE_W), h: SPINE_H }
    }

    pub fn stripe_rect(&self) -> Rect {
        Rect { w: STRIPE_W, ..self.spine_rect() }
    }

    /// The note's line, as wide as the spine.
    pub fn note_rect(&self) -> Rect {
        let base = self.h - NOTE_FROM_BOTTOM;
        Rect { x: MARGIN, y: base - NOTE_PX as i32 - 8, w: self.w - 2 * MARGIN, h: NOTE_PX as i32 + 20 }
    }

    /// The whole frame.
    pub fn paint(&mut self, c: &mut Canvas, look: &Look) {
        c.clear(VOID);
        if look.spine > 0 {
            let s = self.spine_rect();
            let a = look.spine as u32;
            c.rect(s.x, s.y, s.w, s.h, mix(VOID, SPINE, a));
            self.paint_stripe(c, look);
            let baseline = s.y + s.h / 2 + 18;
            self.text.draw(c, Face::Title, WORDMARK_PX, s.x + STRIPE_W + 28, baseline, mix(VOID, CHALK, a), WORDMARK);
        }
        if look.note_a > 0 {
            let r = self.note_rect();
            self.text.draw(c, Face::Body, NOTE_PX, r.x, self.h - NOTE_FROM_BOTTOM, mix(VOID, DUST, look.note_a as u32), look.note.text());
        }
    }

    fn paint_stripe(&self, c: &mut Canvas, look: &Look) {
        let r = self.stripe_rect();
        c.rect(r.x, r.y, r.w, r.h, mix(VOID, look.stripe, look.spine as u32));
    }

    /// Brings `c`, which shows `prev`, to `look`, and returns the part that changed: only the stripe while just its
    /// colour moves, the whole frame the first time, None when nothing changed.
    pub fn update(&mut self, c: &mut Canvas, prev: Option<&Look>, look: &Look) -> Option<Rect> {
        let changed = match prev {
            None => Rect { x: 0, y: 0, w: self.w, h: self.h },
            Some(p) if p == look => return None,
            Some(p) if (p.spine, p.note, p.note_a) == (look.spine, look.note, look.note_a) => {
                self.paint_stripe(c, look);
                return Some(self.stripe_rect().clip(self.w, self.h));
            }
            Some(_) => self.spine_rect().union(self.note_rect()),
        };
        self.paint(c, look);
        Some(changed.clip(self.w, self.h))
    }
}

/// How the frame buffer's picture is turned against the screen: `/sys/class/graphics/fbcon/rotate`, the same value
/// ROCKNIX turns sway's output and pocket-ui turns touch by (1 on both Minis, 3 on the Pocket 5 and Flip 2).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Turn {
    None,
    /// FB_ROTATE_CW: the picture's top left corner is at the panel's top right.
    Cw,
    Ud,
    /// FB_ROTATE_CCW: the picture's top left corner is at the panel's bottom left.
    Ccw,
}

impl Turn {
    pub fn from_fbcon(s: &str) -> Turn {
        match s.trim() {
            "1" => Turn::Cw,
            "2" => Turn::Ud,
            "3" => Turn::Ccw,
            _ => Turn::None,
        }
    }

    /// The picture's size for a frame buffer of `xres` x `yres`.
    pub fn picture_size(self, xres: u32, yres: u32) -> (u32, u32) {
        match self {
            Turn::Cw | Turn::Ccw => (yres, xres),
            Turn::None | Turn::Ud => (xres, yres),
        }
    }
}

/// A frame buffer's layout, from `FBIOGET_VSCREENINFO` and `FBIOGET_FSCREENINFO`: never assumes width x 4 bytes a row.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Format {
    pub xres: u32,
    pub yres: u32,
    /// Bytes from one row to the next (can be more than xres x bytes per pixel).
    pub line_length: u32,
    /// 2, 3 or 4.
    pub bytes: u32,
    /// (offset, length) of red, green and blue in a pixel.
    pub red: (u32, u32),
    pub green: (u32, u32),
    pub blue: (u32, u32),
}

impl Format {
    /// XRGB8888 with rows of exactly `xres` pixels (the host tests' and snapshots' layout).
    pub fn xrgb(xres: u32, yres: u32) -> Format {
        Format { xres, yres, line_length: xres * 4, bytes: 4, red: (16, 8), green: (8, 8), blue: (0, 8) }
    }

    /// Whether this program can write it.
    pub fn usable(&self) -> Result<(), String> {
        let fits = |(off, len): (u32, u32)| (1..=8).contains(&len) && off + len <= self.bytes * 8;
        if !(2..=4).contains(&self.bytes) || !fits(self.red) || !fits(self.green) || !fits(self.blue) {
            return Err(format!("pixel format not supported ({} bytes, r {:?} g {:?} b {:?})", self.bytes, self.red, self.green, self.blue));
        }
        if self.xres == 0 || self.yres == 0 || self.line_length < self.xres * self.bytes {
            return Err(format!("frame buffer {}x{} with {} bytes a row", self.xres, self.yres, self.line_length));
        }
        Ok(())
    }

    /// The bytes of the frame buffer's visible picture.
    pub fn frame_len(&self) -> usize {
        self.line_length as usize * self.yres as usize
    }

    fn encode(&self, rgb: u32) -> u32 {
        let ch = |v: u32, (off, len): (u32, u32)| (v >> (8 - len)) << off;
        ch((rgb >> 16) & 0xff, self.red) | ch((rgb >> 8) & 0xff, self.green) | ch(rgb & 0xff, self.blue)
    }

    fn is_xrgb(&self) -> bool {
        self.bytes == 4 && self.red == (16, 8) && self.green == (8, 8) && self.blue == (0, 8)
    }
}

/// Puts the part `r` of the picture `src` (`w` x `h`, 0RGB) into `dst`, the frame buffer's bytes in `fmt`, turned by
/// `turn`. Returns the frame buffer rows it touched (`y0..y1`): the program writes only those.
pub fn convert(src: &[u32], w: u32, h: u32, r: Rect, turn: Turn, fmt: &Format, dst: &mut [u8]) -> (usize, usize) {
    let r = r.clip(w as i32, h as i32);
    if r.is_empty() {
        return (0, 0);
    }
    let (xres, yres) = (fmt.xres as i32, fmt.yres as i32);
    // The same rectangle on the frame buffer.
    let (px0, py0, pw, ph) = match turn {
        Turn::None => (r.x, r.y, r.w, r.h),
        Turn::Cw => (xres - r.y - r.h, r.x, r.h, r.w),
        Turn::Ud => (xres - r.x - r.w, yres - r.y - r.h, r.w, r.h),
        Turn::Ccw => (r.y, yres - r.x - r.w, r.h, r.w),
    };
    let fast = fmt.is_xrgb();
    let (line, bytes, w) = (fmt.line_length as usize, fmt.bytes as usize, w as i32);
    for py in py0.max(0)..(py0 + ph).min(yres) {
        let row = &mut dst[py as usize * line..py as usize * line + line];
        for px in px0.max(0)..(px0 + pw).min(xres) {
            let (lx, ly) = match turn {
                Turn::None => (px, py),
                Turn::Cw => (py, xres - 1 - px),
                Turn::Ud => (xres - 1 - px, yres - 1 - py),
                Turn::Ccw => (yres - 1 - py, px),
            };
            let v = src[(ly * w + lx) as usize];
            let at = px as usize * bytes;
            if fast {
                row[at..at + 4].copy_from_slice(&(v & 0xff_ffff).to_le_bytes());
            } else {
                row[at..at + bytes].copy_from_slice(&fmt.encode(v).to_le_bytes()[..bytes]);
            }
        }
    }
    (py0.max(0) as usize, (py0 + ph).min(yres) as usize)
}

/// Where the frames go: `/dev/fb0` in the program, a buffer in the tests.
pub trait Screen {
    /// Writes rows `y0..y1` of `frame` (the whole frame buffer, `Format::frame_len` bytes) to the screen.
    fn write_rows(&mut self, frame: &[u8], y0: usize, y1: usize) -> std::io::Result<()>;
    /// Whether another program has taken the screen (sway is DRM master: the frame buffer refuses to pan).
    fn taken(&mut self) -> bool;
}

/// The program's clock: real time in the program, simulated in the tests.
pub trait Clock {
    /// Time since the start.
    fn now(&self) -> Duration;
    /// Sleeps for `d`, or less when a signal comes.
    fn sleep(&mut self, d: Duration);
}

/// What the program watches besides the clock.
pub trait Watch {
    /// SIGTERM or SIGINT came.
    fn told_to_stop(&mut self) -> bool;
    /// sway (or its start scripts) runs: it takes the screen in a moment.
    fn sway_starting(&mut self) -> bool;
}

/// How often sway and the screen are looked at; also the longest sleep, so a stop is seen within it.
pub const CHECK: Duration = Duration::from_millis(200);

/// Why the program ended.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Ended {
    /// Told to stop (SIGTERM, SIGINT): faded out.
    Stopped,
    /// sway started: faded out.
    Sway,
    /// Another program shows its picture already: ended at once.
    Taken,
    /// The screen refused a write.
    Failed(String),
}

/// What `run` did, for the log line and the tests.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Report {
    pub ended: Ended,
    pub frames: u32,
    /// Bytes written to the screen.
    pub bytes: u64,
}

/// Shows the picture until sway starts, the program is told to stop or the screen is taken; fades out at the end
/// (not when taken: the screen is no longer ours).
pub fn run(scene: &mut Scene, screen: &mut dyn Screen, fmt: &Format, turn: Turn, mut timeline: Timeline, clock: &mut dyn Clock, watch: &mut dyn Watch) -> Report {
    let (w, h) = (scene.w as u32, scene.h as u32);
    let mut picture = vec![VOID; w as usize * h as usize];
    let mut frame = vec![0u8; fmt.frame_len()];
    let mut prev: Option<Look> = None;
    let mut why = Ended::Stopped;
    let mut next_check = Duration::ZERO;
    let mut report = Report { ended: Ended::Stopped, frames: 0, bytes: 0 };
    loop {
        let t = clock.now();
        if t >= next_check {
            if screen.taken() {
                report.ended = Ended::Taken;
                return report;
            }
            if timeline.stop_at.is_none() && watch.sway_starting() {
                why = Ended::Sway;
                timeline.stop(t);
            }
            next_check = t + CHECK;
        }
        if timeline.stop_at.is_none() && watch.told_to_stop() {
            why = Ended::Stopped;
            timeline.stop(t);
        }
        let look = timeline.look(t);
        let mut c = Canvas::new(&mut picture, w, h);
        if let Some(r) = scene.update(&mut c, prev.as_ref(), &look) {
            let (y0, y1) = convert(&picture, w, h, r, turn, fmt, &mut frame);
            if y1 > y0 {
                if let Err(e) = screen.write_rows(&frame, y0, y1) {
                    report.ended = Ended::Failed(e.to_string());
                    return report;
                }
                report.frames += 1;
                report.bytes += ((y1 - y0) * fmt.line_length as usize) as u64;
            }
        }
        prev = Some(look);
        if timeline.finished(t) {
            report.ended = why;
            return report;
        }
        let wake = timeline.next_change(t).map_or(next_check, |n| n.min(next_check)).max(t + Duration::from_millis(1));
        clock.sleep(wake - t);
    }
}

/// The whole name of a process of sway's unit: `sway` itself, ROCKNIX's `sway.sh` (its ExecStart) and
/// `sway-config` (its ExecStartPre, the first to run), also under systemd's name for a process about to
/// start one ("(sway-config)"). The whole name, never a prefix: ROCKNIX's `sway-touch.sh` runs from
/// `rocknix.target` minutes before sway on a slow boot (seconds at least), and `swaymsg`, `swaybar` and
/// friends are sway's tools, not sway. A prefix ended the first device boot's splash at 3.33 s, a blink
/// before the frame buffer appeared at 3.50 s.
fn is_sway_comm(comm: &str) -> bool {
    let name = comm.trim().strip_prefix('(').and_then(|n| n.strip_suffix(')')).unwrap_or_else(|| comm.trim());
    matches!(name, "sway" | "sway.sh" | "sway-config")
}

/// Whether a process of sway's unit runs. Reads `<proc>/*/comm`.
pub fn sway_starting(proc: &std::path::Path) -> bool {
    let Ok(entries) = std::fs::read_dir(proc) else { return false };
    entries.flatten().any(|e| {
        e.file_name().to_str().is_some_and(|n| n.bytes().all(|b| b.is_ascii_digit()))
            && std::fs::read_to_string(e.path().join("comm")).is_ok_and(|c| is_sway_comm(&c))
    })
}

/// How waiting for the frame buffer ended without one.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum WaitEnded {
    /// Told to stop (SIGTERM, SIGINT) while waiting.
    Stopped,
    /// sway started while waiting: it takes the screen itself in a moment.
    Sway,
    /// The deadline passed; carries the last try's error, for the log line.
    TimedOut(String),
}

/// Waits for the frame buffer: tries `try_open` at once, then every `poll`, until it opens, sway starts,
/// the program is told to stop or `deadline` (on `clock`, whose zero is the program's start) passes.
/// sway is looked for every [`CHECK`], as in [`run`] — looking means reading `/proc` — while a stop is
/// seen every poll, so SIGTERM is never slept through. Nothing here blocks the boot: the caller is
/// `Type=simple`, so systemd went on the moment this program started.
pub fn wait_for_fb<T>(deadline: Duration, poll: Duration, clock: &mut dyn Clock, watch: &mut dyn Watch, try_open: &mut dyn FnMut() -> Result<T, String>) -> Result<T, WaitEnded> {
    let mut next_check = Duration::ZERO;
    loop {
        let tried = try_open();
        let t = clock.now();
        match tried {
            Ok(found) => return Ok(found),
            Err(e) if t >= deadline => return Err(WaitEnded::TimedOut(e)),
            Err(_) => {}
        }
        if watch.told_to_stop() {
            return Err(WaitEnded::Stopped);
        }
        if t >= next_check {
            if watch.sway_starting() {
                return Err(WaitEnded::Sway);
            }
            next_check = t + CHECK;
        }
        clock.sleep(poll);
    }
}

/// Settings → Look & feel → Motion as saved (Full when unset or unreadable): Off shows a still picture without fades.
pub fn motion(data_dir: &std::path::Path) -> Motion {
    #[derive(serde::Deserialize, Default)]
    struct Only {
        #[serde(default, deserialize_with = "crate::settings::lenient")]
        motion: Motion,
    }
    std::fs::read(data_dir.join("settings.json")).ok().and_then(|b| serde_json::from_slice::<Only>(&b).ok()).unwrap_or_default().motion
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    fn red(c: u32) -> u32 {
        (c >> 16) & 0xff
    }

    #[test]
    fn the_spine_fades_in_holds_and_fades_out() {
        let mut t = Timeline::new(Motion::Full, false);
        assert_eq!(t.look(ms(0)).spine, 0, "starts black");
        let mid = t.look(ms(200)).spine;
        assert!(mid > 100 && mid < 160, "half way up at half time ({mid})");
        assert_eq!(t.look(ms(400)).spine, 255);
        assert_eq!(t.look(ms(30_000)).spine, 255);
        assert_eq!(t.look(ms(0)).note, Note::None);
        t.stop(ms(5000));
        t.stop(ms(6000));
        assert_eq!(t.stop_at, Some(ms(5000)), "only the first stop counts");
        assert_eq!(t.look(ms(4999)).spine, 255);
        assert!(t.look(ms(5125)).spine < 200);
        assert_eq!(t.look(ms(5250)).spine, 0);
        assert!(!t.finished(ms(5100)) && t.finished(ms(5250)));
        assert_eq!(t.next_change(ms(5250)), None);
    }

    #[test]
    fn the_stripe_holds_each_console_colour_then_crossfades() {
        let t = Timeline::new(Motion::Full, false);
        assert_eq!(t.look(ms(0)).stripe, STRIPES[0]);
        assert_eq!(t.look(ms(1199)).stripe, STRIPES[0]);
        let between = t.look(ms(1400)).stripe;
        assert!(between != STRIPES[0] && between != STRIPES[1], "half way between NES and GB");
        assert_eq!(t.look(ms(1600)).stripe, STRIPES[1]);
        assert_eq!(t.look(ms(1600 * 6)).stripe, STRIPES[0], "round again after the sixth");
        // Still after about three minutes, on a colour of its own.
        let still = t.look(STILL_AFTER).stripe;
        assert!(STRIPES.contains(&still));
        assert_eq!(t.look(STILL_AFTER + ms(777)).stripe, still);
        assert_eq!(t.look(ms(3_600_000)).stripe, still);
    }

    #[test]
    fn the_notes_say_what_is_going_on() {
        let t = Timeline::new(Motion::Full, true);
        assert_eq!(t.look(ms(0)).note, Note::FirstStart);
        assert_eq!(t.look(ms(400)).note_a, 255);
        let late = t.look(LATE + ms(400));
        assert_eq!((late.note, late.note_a), (Note::Late, 255), "after a minute: still starting, never just black");
        let t = Timeline::new(Motion::Full, false);
        assert_eq!(t.look(ms(59_000)).note, Note::None);
        assert_eq!(t.look(LATE).note, Note::Late);
        assert_eq!(t.look(LATE).note_a, 0, "it fades in too");
        assert_eq!(Note::Late.text(), "Still starting…");
    }

    #[test]
    fn motion_off_is_a_still_picture_without_fades() {
        let mut t = Timeline::new(Motion::Off, true);
        let first = t.look(ms(0));
        assert_eq!((first.spine, first.note_a, first.stripe), (255, 255, STRIPES[0]));
        assert_eq!(t.look(ms(1400)), first, "the stripe does not move");
        assert_eq!(t.next_change(ms(0)), Some(LATE), "nothing to draw until the late note");
        t.stop(ms(2000));
        assert_eq!(t.look(ms(2000)).spine, 0, "gone at once");
        assert!(t.finished(ms(2000)));
        // Reduced keeps the fades: nothing on the splash moves, it only fades.
        let r = Timeline::new(Motion::Reduced, false);
        assert_eq!(r.look(ms(0)).spine, 0);
        assert_ne!(r.look(ms(1400)).stripe, STRIPES[0]);
    }

    /// Frame pacing: at most 30 frames a second, none while a colour holds, about 7 a second on average.
    #[test]
    fn frames_come_only_when_something_changes_and_at_most_30_a_second() {
        let t = Timeline::new(Motion::Full, false);
        let (mut now, mut frames, mut gaps) = (Duration::ZERO, 0u32, Vec::new());
        while now < ms(20_000) {
            let next = t.next_change(now).expect("something changes");
            assert!(next > now);
            gaps.push(next - now);
            now = next;
            frames += 1;
        }
        assert!(gaps.iter().all(|g| *g >= FRAME), "never sooner than a frame: {gaps:?}");
        assert!(frames <= 20 * 30, "{frames} frames in 20 s");
        let per_s = frames as f32 / 20.0;
        assert!(per_s < 10.0, "{per_s} frames a second on average: the holds draw nothing");
        // During a hold the next change is the hold's end, 1.2 s later.
        assert_eq!(t.next_change(ms(1600)), Some(ms(1600) + HOLD));
        // After three minutes nothing moves: no frames until the stop.
        assert_eq!(t.next_change(STILL_AFTER + ms(10)), None);
    }

    fn scene() -> Scene {
        Scene::new(1240, 1080)
    }

    fn picture(scene: &mut Scene, look: &Look) -> Vec<u32> {
        let mut buf = vec![0u32; (scene.w * scene.h) as usize];
        let (w, h) = (scene.w as u32, scene.h as u32);
        scene.paint(&mut Canvas::new(&mut buf, w, h), look);
        buf
    }

    #[test]
    fn the_picture_is_one_spine_with_the_wordmark_on_black() {
        let mut s = scene();
        let look = Timeline::new(Motion::Full, false).look(ms(1000));
        let buf = picture(&mut s, &look);
        let at = |x: i32, y: i32| buf[(y * s.w + x) as usize];
        let sp = s.spine_rect();
        assert_eq!(sp, Rect { x: 72, y: 504, w: 1096, h: 72 }, "Home's spine place on the Mini V2");
        assert_eq!(at(10, 10), VOID, "true black around it");
        assert_eq!(at(sp.x + 5, sp.y + 5), STRIPES[0], "the NES stripe");
        assert_eq!(at(sp.x + sp.w - 5, sp.y + 5), SPINE);
        // The wordmark: Chalk pixels on the spine, right of the stripe, and nowhere else.
        let chalk: Vec<(i32, i32)> =
            (0..s.h).flat_map(|y| (0..s.w).map(move |x| (x, y))).filter(|&(x, y)| red(at(x, y)) > 0xb0 && at(x, y) != STRIPES[0]).collect();
        assert!(chalk.len() > 500, "the wordmark is there ({} px)", chalk.len());
        assert!(chalk.iter().all(|&(x, y)| x > sp.x + 12 && x < sp.x + 400 && y > sp.y && y < sp.y + sp.h));
        // Black at the start and at the end of a fade.
        assert!(picture(&mut s, &Timeline::new(Motion::Full, false).look(ms(0))).iter().all(|p| *p == VOID));
    }

    #[test]
    fn the_notes_are_painted_in_dust_bottom_left() {
        let mut s = scene();
        let buf = picture(&mut s, &Timeline::new(Motion::Full, true).look(ms(1000)));
        let r = s.note_rect();
        let lit: Vec<(i32, i32)> = (0..s.h).flat_map(|y| (0..s.w).map(move |x| (x, y))).filter(|&(x, y)| {
            let p = buf[(y * s.w + x) as usize];
            p != VOID && !(504..576).contains(&y)
        }).collect();
        assert!(!lit.is_empty(), "the first start says so");
        assert!(lit.iter().all(|&(x, y)| x >= r.x && x < r.x + r.w && y >= r.y && y < r.y + r.h), "inside the note's line");
        assert!(lit.iter().all(|&(x, y)| red(buf[(y * s.w + x) as usize]) <= red(DUST)), "Dust, never brighter");
    }

    #[test]
    fn every_screen_size_gets_the_spine_in_its_middle() {
        // The Mini V2, the first Mini, the Pocket 5 and Flip 2, and a portrait frame buffer shown unturned.
        for (w, h) in [(1240, 1080), (1280, 960), (1920, 1080), (1080, 1240)] {
            let mut s = Scene::new(w, h);
            let sp = s.spine_rect();
            assert_eq!((sp.x, sp.w), (72, w as i32 - 144), "{w}x{h}");
            assert_eq!(sp.y + sp.h / 2, h as i32 / 2, "{w}x{h}");
            let look = Timeline::new(Motion::Full, true).look(LATE + ms(1000));
            let buf = picture(&mut s, &look);
            assert_eq!(buf[(sp.y * w as i32 + sp.x + 2) as usize], STRIPES[look_index(&look)], "{w}x{h}");
            let nr = s.note_rect();
            assert!(nr.y + nr.h <= h as i32 && nr.y > sp.y + sp.h, "{w}x{h}: the note under the spine, on screen");
        }
    }

    fn look_index(look: &Look) -> usize {
        STRIPES.iter().position(|c| *c == look.stripe).unwrap_or(0)
    }

    #[test]
    fn an_update_repaints_only_what_changed() {
        let mut s = scene();
        let t = Timeline::new(Motion::Full, false);
        let mut buf = vec![0u32; (s.w * s.h) as usize];
        let (w, h) = (s.w as u32, s.h as u32);
        let mut c = Canvas::new(&mut buf, w, h);
        assert_eq!(s.update(&mut c, None, &t.look(ms(500))), Some(Rect { x: 0, y: 0, w: 1240, h: 1080 }));
        let a = t.look(ms(1300));
        assert_eq!(s.update(&mut c, Some(&t.look(ms(500))), &a), Some(s.stripe_rect()), "a colour step: the stripe only");
        assert_eq!(s.update(&mut c, Some(&a), &a), None);
        // The stripe-only update gives exactly the picture a full paint gives.
        let full = picture(&mut s, &a);
        assert!(buf == full);
        let mut t2 = t.clone();
        t2.stop(ms(2000));
        let mut c = Canvas::new(&mut buf, w, h);
        let r = s.update(&mut c, Some(&a), &t2.look(ms(2100))).unwrap();
        assert_eq!(r, s.spine_rect().union(s.note_rect()), "a fade: the spine and the note's line");
    }

    /// Turns the frame buffer back into the picture, by the definition of each turn (where the picture's top left
    /// corner lands), not by `convert`'s own formulas.
    fn unturn(frame: &[u8], fmt: &Format, turn: Turn) -> (u32, u32, Vec<u32>) {
        let (w, h) = turn.picture_size(fmt.xres, fmt.yres);
        let mut out = vec![0u32; (w * h) as usize];
        for ly in 0..h {
            for lx in 0..w {
                let (px, py) = match turn {
                    Turn::None => (lx, ly),
                    // Top left at the panel's top right; the picture's rows run down the panel's columns.
                    Turn::Cw => (fmt.xres - 1 - ly, lx),
                    Turn::Ud => (fmt.xres - 1 - lx, fmt.yres - 1 - ly),
                    // Top left at the panel's bottom left.
                    Turn::Ccw => (ly, fmt.yres - 1 - lx),
                };
                let at = (py * fmt.line_length + px * fmt.bytes) as usize;
                let mut v = [0u8; 4];
                v[..fmt.bytes as usize].copy_from_slice(&frame[at..at + fmt.bytes as usize]);
                let v = u32::from_le_bytes(v);
                let ch = |(off, len): (u32, u32)| ((v >> off) & ((1 << len) - 1)) << (8 - len);
                out[(ly * w + lx) as usize] = ch(fmt.red) << 16 | ch(fmt.green) << 8 | ch(fmt.blue);
            }
        }
        (w, h, out)
    }

    #[test]
    fn convert_turns_the_picture_for_every_fbcon_rotation_and_row_padding() {
        let test_picture = |w: u32, h: u32| -> Vec<u32> { (0..w * h).map(|i| (i % w) << 16 | (i / w) << 4 | 0x0f).collect() };
        // The Mini V2's 1080x1240 panel turned clockwise, a padded row, the Pocket 5's turned the other way, upside
        // down, and not turned at all.
        for (xres, yres, turn, pad) in [(108, 124, Turn::Cw, 0), (108, 124, Turn::Cw, 64), (108, 192, Turn::Ccw, 12), (124, 108, Turn::Ud, 0), (124, 108, Turn::None, 8)] {
            let fmt = Format { line_length: xres * 4 + pad, ..Format::xrgb(xres, yres) };
            let (w, h) = turn.picture_size(xres, yres);
            assert!(w > h, "{turn:?}: landscape picture");
            let src = test_picture(w, h);
            let mut frame = vec![0xAAu8; fmt.frame_len()];
            let rows = convert(&src, w, h, Rect { x: 0, y: 0, w: w as i32, h: h as i32 }, turn, &fmt, &mut frame);
            assert_eq!(rows, (0, yres as usize));
            let (_, _, back) = unturn(&frame, &fmt, turn);
            let mask = |v: &u32| v & 0xff_ffff;
            assert!(back.iter().map(mask).eq(src.iter().map(mask)), "{turn:?} pad {pad}: the picture comes back");
            // The padding at the end of each row is left alone.
            if pad > 0 {
                assert!((0..yres as usize).all(|y| frame[y * fmt.line_length as usize + xres as usize * 4] == 0xAA));
            }
        }
    }

    #[test]
    fn convert_writes_only_the_rows_of_the_part_that_changed() {
        let fmt = Format::xrgb(1080, 1240);
        let s = scene();
        let src = vec![0x123456u32; 1240 * 1080];
        let mut frame = vec![0u8; fmt.frame_len()];
        // The stripe (x 72..84 of the picture) is rows 72..84 of the Mini V2's frame buffer.
        let rows = convert(&src, 1240, 1080, s.stripe_rect(), Turn::Cw, &fmt, &mut frame);
        assert_eq!(rows, (72, 84));
        let touched = frame.chunks(4).filter(|p| p != &[0, 0, 0, 0]).count();
        assert_eq!(touched, 12 * 72, "only the stripe's pixels");
        // A picture's bottom left is the panel's top left.
        let mut frame = vec![0u8; fmt.frame_len()];
        convert(&src, 1240, 1080, Rect { x: 0, y: 1079, w: 1, h: 1 }, Turn::Cw, &fmt, &mut frame);
        assert_eq!(&frame[0..4], &[0x56, 0x34, 0x12, 0]);
    }

    #[test]
    fn sixteen_bit_frame_buffers_get_rgb565() {
        let fmt = Format { bytes: 2, line_length: 20, red: (11, 5), green: (5, 6), blue: (0, 5), ..Format::xrgb(10, 4) };
        assert!(fmt.usable().is_ok());
        let src = vec![0xFF8000u32; 40];
        let mut frame = vec![0u8; fmt.frame_len()];
        convert(&src, 10, 4, Rect { x: 0, y: 0, w: 10, h: 4 }, Turn::None, &fmt, &mut frame);
        assert_eq!(u16::from_le_bytes([frame[0], frame[1]]), 0xFC00, "red 31, green 32 of 63, blue 0");
        assert!(Format { bytes: 1, ..fmt }.usable().is_err(), "8-bit: not ours to draw");
        assert!(Format { line_length: 10, ..fmt }.usable().is_err(), "a row shorter than its pixels");
        assert!(Format { red: (30, 8), ..Format::xrgb(10, 4) }.usable().is_err());
    }

    #[test]
    fn the_fbcon_rotation_names_the_turn() {
        assert_eq!(Turn::from_fbcon("1\n"), Turn::Cw);
        assert_eq!(Turn::from_fbcon("3"), Turn::Ccw);
        assert_eq!(Turn::from_fbcon("2"), Turn::Ud);
        assert_eq!(Turn::from_fbcon(""), Turn::None, "unreadable: not turned");
        assert_eq!(Turn::Cw.picture_size(1080, 1240), (1240, 1080));
        assert_eq!(Turn::Ccw.picture_size(1080, 1920), (1920, 1080));
    }

    /// A screen that keeps every write; taken from a given time on.
    struct FakeScreen {
        frame: Vec<u8>,
        writes: u32,
        taken_at: Option<Duration>,
        now: std::rc::Rc<std::cell::Cell<Duration>>,
        fail: bool,
    }

    impl Screen for FakeScreen {
        fn write_rows(&mut self, frame: &[u8], y0: usize, y1: usize) -> std::io::Result<()> {
            if self.fail {
                return Err(std::io::Error::other("no space left on device"));
            }
            let line = frame.len() / 124;
            self.frame[y0 * line..y1 * line].copy_from_slice(&frame[y0 * line..y1 * line]);
            self.writes += 1;
            Ok(())
        }
        fn taken(&mut self) -> bool {
            self.taken_at.is_some_and(|t| self.now.get() >= t)
        }
    }

    struct FakeClock {
        now: std::rc::Rc<std::cell::Cell<Duration>>,
        sleeps: Vec<Duration>,
    }

    impl Clock for FakeClock {
        fn now(&self) -> Duration {
            self.now.get()
        }
        fn sleep(&mut self, d: Duration) {
            self.sleeps.push(d);
            self.now.set(self.now.get() + d);
        }
    }

    struct FakeWatch {
        now: std::rc::Rc<std::cell::Cell<Duration>>,
        stop_at: Option<Duration>,
        sway_at: Option<Duration>,
        looked: AtomicU32,
    }

    impl Watch for FakeWatch {
        fn told_to_stop(&mut self) -> bool {
            self.stop_at.is_some_and(|t| self.now.get() >= t)
        }
        fn sway_starting(&mut self) -> bool {
            self.looked.fetch_add(1, Ordering::Relaxed);
            self.sway_at.is_some_and(|t| self.now.get() >= t)
        }
    }

    /// A small screen (a 108x124 frame buffer turned clockwise, 124x108 picture) through `run`.
    fn run_small(stop_at: Option<Duration>, sway_at: Option<Duration>, taken_at: Option<Duration>, fail: bool) -> (Report, FakeScreen, FakeClock, FakeWatch, Scene) {
        let now = std::rc::Rc::new(std::cell::Cell::new(Duration::ZERO));
        let fmt = Format::xrgb(108, 124);
        let mut screen = FakeScreen { frame: vec![0; fmt.frame_len()], writes: 0, taken_at, now: now.clone(), fail };
        let mut clock = FakeClock { now: now.clone(), sleeps: Vec::new() };
        let mut watch = FakeWatch { now, stop_at, sway_at, looked: AtomicU32::new(0) };
        let mut scene = Scene::new(124, 108);
        let report = run(&mut scene, &mut screen, &fmt, Turn::Cw, Timeline::new(Motion::Full, false), &mut clock, &mut watch);
        (report, screen, clock, watch, scene)
    }

    #[test]
    fn told_to_stop_it_fades_to_black_and_ends() {
        let (report, screen, clock, _, _) = run_small(Some(ms(3000)), None, None, false);
        assert_eq!(report.ended, Ended::Stopped);
        let end = clock.now();
        assert!(end >= ms(3000 + 250) && end < ms(3000 + 250 + 2 * 33 + 200), "ended {end:?}: the fade, a frame, the next look");
        assert!(screen.frame.iter().all(|b| *b == 0), "the last frame is black");
        assert!(clock.sleeps.iter().all(|s| *s <= CHECK), "never asleep longer than {CHECK:?}: a stop is seen in time");
        assert!(clock.sleeps.iter().all(|s| !s.is_zero()));
        assert!(report.frames >= 20 && report.frames < 80, "{} frames in 3.25 s", report.frames);
    }

    #[test]
    fn when_sway_starts_it_fades_out_on_its_own() {
        let (report, screen, clock, watch, _) = run_small(None, Some(ms(10_000)), None, false);
        assert_eq!(report.ended, Ended::Sway);
        assert!(clock.now() <= ms(10_000) + CHECK + FADE_OUT + FRAME * 2, "seen within {CHECK:?}, then the fade: {:?}", clock.now());
        assert!(screen.frame.iter().all(|b| *b == 0));
        let looks = watch.looked.load(Ordering::Relaxed);
        assert!((40..=60).contains(&looks), "sway looked for about 5 times a second, not every frame: {looks}");
    }

    #[test]
    fn a_taken_screen_ends_it_at_once_without_another_write() {
        let (report, screen, clock, _, _) = run_small(None, None, Some(ms(2000)), false);
        assert_eq!(report.ended, Ended::Taken);
        assert!(clock.now() < ms(2000) + CHECK + ms(1));
        let writes = screen.writes;
        assert!(screen.frame.iter().any(|b| *b != 0), "the last frame it wrote was the picture, not a fade: the screen is not ours");
        assert_eq!(report.frames, writes);
    }

    #[test]
    fn a_failed_write_ends_it() {
        let (report, _, clock, _, _) = run_small(None, None, None, true);
        assert_eq!(report.ended, Ended::Failed("no space left on device".into()));
        assert_eq!(clock.now(), Duration::ZERO, "at the first frame");
    }

    /// The CPU budget: 20 s of the boot screen at the Mini V2's size (crossfades, holds, the fade in), painted,
    /// turned and copied as on the device, take a small share of one core. Measured on the build computer; the
    /// Mini V2's cores are slower, so the bound is a tenth of the time, against about a hundredth measured.
    #[test]
    fn twenty_seconds_of_the_boot_screen_cost_little_cpu() {
        struct Sink(u64);
        impl Screen for Sink {
            fn write_rows(&mut self, frame: &[u8], y0: usize, y1: usize) -> std::io::Result<()> {
                let line = frame.len() / 1240;
                self.0 += frame[y0 * line..y1 * line].iter().step_by(4096).map(|b| *b as u64).sum::<u64>();
                Ok(())
            }
            fn taken(&mut self) -> bool {
                false
            }
        }
        let now = std::rc::Rc::new(std::cell::Cell::new(Duration::ZERO));
        let mut clock = FakeClock { now: now.clone(), sleeps: Vec::new() };
        let mut watch = FakeWatch { now, stop_at: Some(ms(20_000)), sway_at: None, looked: AtomicU32::new(0) };
        let fmt = Format::xrgb(1080, 1240);
        let mut scene = Scene::new(1240, 1080);
        let started = std::time::Instant::now();
        let report = run(&mut scene, &mut Sink(0), &fmt, Turn::Cw, Timeline::new(Motion::Full, true), &mut clock, &mut watch);
        let spent = started.elapsed();
        let share = spent.as_secs_f64() / clock.now().as_secs_f64();
        let per_frame = spent / report.frames.max(1);
        println!(
            "boot screen, 20 s at 1240x1080: {} frames ({:.1} a second), {:.1} MB written, {spent:?} of work ({per_frame:?} a frame, {:.2} % of a core)",
            report.frames,
            report.frames as f64 / clock.now().as_secs_f64(),
            report.bytes as f64 / 1e6,
            share * 100.0
        );
        assert_eq!(report.ended, Ended::Stopped);
        assert!(report.frames as f64 / clock.now().as_secs_f64() < 12.0, "{} frames", report.frames);
        assert!(share < 0.10, "{:.1} % of a core", share * 100.0);
    }

    #[test]
    fn sway_is_seen_by_any_process_of_its_unit() {
        let root = crate::testutil::TestDir::new("splash-proc");
        let proc = |pid: &str, comm: &str| {
            std::fs::create_dir_all(root.join(pid)).unwrap();
            std::fs::write(root.join(pid).join("comm"), format!("{comm}\n")).unwrap();
        };
        proc("1", "systemd");
        proc("230", "pocket-splash");
        std::fs::create_dir_all(root.join("self")).unwrap();
        std::fs::create_dir_all(root.join("sys")).unwrap();
        assert!(!sway_starting(&root));
        for comm in ["(sway-config)", "sway-config", "sway.sh", "sway"] {
            proc("900", comm);
            assert!(sway_starting(&root), "{comm}");
        }
        // ROCKNIX's sway-touch.sh runs from rocknix.target, long before sway.service: in a name, but not sway.
        for comm in ["sway-touch.sh", "swaymsg", "swaybar", "swayidle"] {
            proc("900", comm);
            assert!(!sway_starting(&root), "{comm} is not sway starting");
        }
        assert!(!sway_starting(&root.join("missing")), "no /proc: no sway");
    }

    /// A frame buffer that appears when the opening has been tried `on_try` times (never, with None).
    fn fake_fb(root: &crate::testutil::TestDir, on_try: Option<u32>, tries: std::rc::Rc<std::cell::Cell<u32>>) -> impl FnMut() -> Result<std::fs::File, String> {
        let path = root.join("fb0");
        move || {
            tries.set(tries.get() + 1);
            if on_try == Some(tries.get()) {
                std::fs::write(&path, []).unwrap();
            }
            std::fs::File::open(&path).map_err(|e| format!("{}: {e}", path.display()))
        }
    }

    fn wait_parts() -> (FakeClock, FakeWatch, std::rc::Rc<std::cell::Cell<Duration>>) {
        let now = std::rc::Rc::new(std::cell::Cell::new(Duration::ZERO));
        (FakeClock { now: now.clone(), sleeps: Vec::new() }, FakeWatch { now: now.clone(), stop_at: None, sway_at: None, looked: AtomicU32::new(0) }, now)
    }

    #[test]
    fn the_wait_finds_a_frame_buffer_that_comes_late() {
        let root = crate::testutil::TestDir::new("splash-wait-late");
        let (mut clock, mut watch, _) = wait_parts();
        let tries = std::rc::Rc::new(std::cell::Cell::new(0));
        let mut open = fake_fb(&root, Some(5), tries.clone());
        let found = wait_for_fb(Duration::from_secs(20), ms(25), &mut clock, &mut watch, &mut open);
        assert!(found.is_ok(), "{found:?}");
        assert_eq!(tries.get(), 5, "tried until the file was there");
        assert_eq!(clock.now(), ms(100), "four polls of waiting, then the picture starts at once");
        // There from the first look: no waiting at all.
        let (mut clock, mut watch, _) = wait_parts();
        let tries = std::rc::Rc::new(std::cell::Cell::new(0));
        let mut open = fake_fb(&root, Some(1), tries);
        assert!(wait_for_fb(Duration::from_secs(20), ms(25), &mut clock, &mut watch, &mut open).is_ok());
        assert_eq!(clock.now(), Duration::ZERO);
        assert!(clock.sleeps.is_empty());
    }

    #[test]
    fn the_wait_gives_up_when_the_frame_buffer_never_comes() {
        let root = crate::testutil::TestDir::new("splash-wait-never");
        let (mut clock, mut watch, _) = wait_parts();
        let tries = std::rc::Rc::new(std::cell::Cell::new(0));
        let mut open = fake_fb(&root, None, tries.clone());
        let ended = wait_for_fb(Duration::from_secs(1), ms(25), &mut clock, &mut watch, &mut open);
        let Err(WaitEnded::TimedOut(e)) = ended else { panic!("{ended:?}") };
        assert!(e.contains("fb0"), "the log line names what stayed missing: {e}");
        assert_eq!(clock.now(), Duration::from_secs(1), "the full deadline, no early give-up");
        assert_eq!(tries.get(), 41, "one try a poll, from the first moment to the deadline");
    }

    #[test]
    fn the_wait_ends_for_sway_and_for_a_stop() {
        let root = crate::testutil::TestDir::new("splash-wait-ends");
        // sway at 300 ms, looked for every CHECK: the wait ends at the 400 ms look, not at the deadline.
        let (mut clock, mut watch, _) = wait_parts();
        watch.sway_at = Some(ms(300));
        let mut open = fake_fb(&root, None, std::rc::Rc::new(std::cell::Cell::new(0)));
        let ended = wait_for_fb(Duration::from_secs(20), ms(25), &mut clock, &mut watch, &mut open);
        assert_eq!(ended.err(), Some(WaitEnded::Sway));
        assert_eq!(clock.now(), ms(400));
        // Told to stop at 310 ms: seen at the next poll, not slept through to the next sway look.
        let (mut clock, mut watch, _) = wait_parts();
        watch.stop_at = Some(ms(310));
        let mut open = fake_fb(&root, None, std::rc::Rc::new(std::cell::Cell::new(0)));
        let ended = wait_for_fb(Duration::from_secs(20), ms(25), &mut clock, &mut watch, &mut open);
        assert_eq!(ended.err(), Some(WaitEnded::Stopped));
        assert_eq!(clock.now(), ms(325));
    }

    /// The unit's keys are ones systemd knows (255 on the device and on the build computer). One wrong
    /// name — `ConditionPathIsExecutable`, the first device boot's journal — and systemd ignores the
    /// line, so the unit ran without its condition. `systemd-analyze verify` says the same where it can
    /// run; this test runs everywhere.
    #[test]
    fn the_units_keys_are_ones_systemd_knows() {
        let unit = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/deploy/pocket-splash.service")).unwrap();
        let known: &[(&str, &[&str])] = &[
            ("Unit", &["Description", "DefaultDependencies", "Conflicts", "Before", "After", "Wants", "Requires", "ConditionPathExists", "ConditionFileIsExecutable", "ConditionFileNotEmpty", "ConditionKernelCommandLine"]),
            ("Service", &["Type", "ExecStart", "ExecStartPre", "TimeoutStopSec", "TimeoutStartSec", "Restart", "RestartSec", "StandardOutput", "StandardError", "RemainAfterExit"]),
            ("Install", &["WantedBy", "RequiredBy", "Alias", "Also"]),
        ];
        let mut section = "";
        let mut conditions = 0;
        for line in unit.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some(name) = line.strip_prefix('[').and_then(|n| n.strip_suffix(']')) {
                section = name;
                assert!(known.iter().any(|(s, _)| *s == section), "unknown section [{section}]");
                continue;
            }
            let (key, value) = line.split_once('=').unwrap_or_else(|| panic!("no key in {line:?}"));
            let keys = known.iter().find(|(s, _)| *s == section).map_or(&[][..], |(_, k)| *k);
            assert!(keys.contains(&key), "[{section}] {key}: not a key systemd knows");
            if key == "ConditionFileIsExecutable" {
                conditions += 1;
                assert_eq!(value, "/storage/pocket-ui/current/pocket-splash");
            }
            if key == "ExecStart" {
                assert_eq!(value, "/storage/pocket-ui/current/pocket-splash");
            }
        }
        assert_eq!(conditions, 1, "the condition that keeps a card without the program quiet");
        assert!(!unit.contains("ConditionPath"), "the wrong name the first device boot's journal named");
    }

    #[test]
    fn the_motion_setting_is_read_leniently() {
        let d = crate::testutil::TestDir::new("splash-motion");
        assert_eq!(motion(&d), Motion::Full, "no settings.json");
        std::fs::write(d.join("settings.json"), br#"{"motion": "off", "theme": "shelf"}"#).unwrap();
        assert_eq!(motion(&d), Motion::Off);
        std::fs::write(d.join("settings.json"), br#"{"motion": "sideways"}"#).unwrap();
        assert_eq!(motion(&d), Motion::Full, "an unknown value");
        std::fs::write(d.join("settings.json"), b"{not json").unwrap();
        assert_eq!(motion(&d), Motion::Full);
    }
}
