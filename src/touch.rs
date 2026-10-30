//! The Mini V2's touch screen (`generic ft5x06`, an evdev multitouch device).
//!
//! Everything here is plain logic so it can be tested without a device:
//! - `Rotation`, `Axes`, `map_raw`: the panel is portrait-native (1080x1240) and sway turns the output to
//!   landscape (1240x1080), so raw coordinates must be turned the same way;
//! - `MtParser`: the evdev multitouch stream, reduced to one finger (`TouchEvent`: down, move, up);
//! - `Recognizer`: taps and swipes from that finger, for pocket-ui's menus;
//! - `HitMap`: what the last frame painted where (rows and hint pills), and what a tap on it means;
//! - `Shared`, `nds_bottom`, `pointer_for`: for pocket-core, the touch as a libretro pointer on the
//!   Nintendo DS's bottom screen;
//! - `linux`: the evdev side (opening the device and the thread that reads it).
//!
//! pocket-ui acts on touches only while no game runs; while one runs, pocket-core reads the same device.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::pad::Button;

// ---- coordinates ---------------------------------------------------------------------------------------

/// The output transform sway applies to the panel (`output DSI-1 transform 90` on the Mini V2), which is
/// also how the touch coordinates have to be turned. Angles are sway's: `Deg90` turns the picture
/// counter-clockwise, so the landscape screen's top edge lies along the panel's left edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rotation {
    Deg0,
    Deg90,
    Deg180,
    Deg270,
}

impl Rotation {
    pub fn from_degrees(d: i32) -> Option<Rotation> {
        match d {
            0 => Some(Rotation::Deg0),
            90 => Some(Rotation::Deg90),
            180 => Some(Rotation::Deg180),
            270 => Some(Rotation::Deg270),
            _ => None,
        }
    }

    /// `/sys/class/graphics/fbcon/rotate`, which ROCKNIX's sway setup turns into the output transform. The kernel's
    /// values turn the picture on the panel: 1 is FB_ROTATE_CW (clockwise), 3 FB_ROTATE_CCW. A picture turned
    /// clockwise onto a portrait panel shows its top left corner at the panel's top right, so the panel's own
    /// coordinates come back with the opposite quarter turn: 1 is `Deg270` here (screen x = panel y, screen y =
    /// 1 - panel x), 3 is `Deg90`. (Until 2026-10-23 this read 1 as `Deg90`: on the Mini V2 a tap on the DS's top
    /// screen reached the bottom one and a tap on the bottom one was dropped.)
    pub fn from_fbcon(n: i32) -> Rotation {
        match n {
            1 => Rotation::Deg270,
            2 => Rotation::Deg180,
            3 => Rotation::Deg90,
            _ => Rotation::Deg0,
        }
    }

    /// A quarter turn: portrait panel coordinates become landscape ones.
    pub fn swaps_axes(self) -> bool {
        matches!(self, Rotation::Deg90 | Rotation::Deg270)
    }

    /// Panel position as fractions (0..1 of its width and height) to the position on the turned screen.
    pub fn apply(self, u: f32, v: f32) -> (f32, f32) {
        match self {
            Rotation::Deg0 => (u, v),
            Rotation::Deg90 => (1.0 - v, u),
            Rotation::Deg180 => (1.0 - u, 1.0 - v),
            Rotation::Deg270 => (v, 1.0 - u),
        }
    }

    /// The inverse of `apply`: screen fractions to panel fractions (the virtual device's touch screen).
    pub fn invert(self, x: f32, y: f32) -> (f32, f32) {
        match self {
            Rotation::Deg0 => (x, y),
            Rotation::Deg90 => (y, 1.0 - x),
            Rotation::Deg180 => (1.0 - x, 1.0 - y),
            Rotation::Deg270 => (1.0 - y, x),
        }
    }

    /// Which way to turn: `POCKETUI_TOUCH_ROTATE` (degrees) wins, then the framebuffer console's rotation
    /// when it fits the shapes (a portrait panel under a landscape screen needs a quarter turn), else the
    /// quarter turn a portrait panel needs, else none.
    pub fn resolve(env: Option<&str>, fbcon: Option<i32>, panel_portrait: bool, screen_landscape: bool) -> Rotation {
        if let Some(r) = env.and_then(|e| e.trim().parse::<i32>().ok()).and_then(Rotation::from_degrees) {
            return r;
        }
        let needs_turn = panel_portrait == screen_landscape;
        match fbcon.map(Rotation::from_fbcon) {
            Some(r) if r.swaps_axes() == needs_turn => r,
            _ if needs_turn => Rotation::Deg90,
            _ => Rotation::Deg0,
        }
    }
}

/// The ranges of the device's position axes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Axes {
    pub x_min: i32,
    pub x_max: i32,
    pub y_min: i32,
    pub y_max: i32,
}

impl Axes {
    pub fn portrait(&self) -> bool {
        self.y_max - self.y_min > self.x_max - self.x_min
    }

    /// A raw value as a fraction of its range, kept inside 0..1.
    fn fraction(raw: i32, min: i32, max: i32) -> f32 {
        if max <= min {
            return 0.0;
        }
        ((raw - min) as f32 / (max - min) as f32).clamp(0.0, 1.0)
    }
}

/// Raw device coordinates to a position on the landscape screen, as fractions (0..1) of its width and height.
pub fn map_raw(axes: &Axes, rot: Rotation, raw_x: i32, raw_y: i32) -> (f32, f32) {
    let u = Axes::fraction(raw_x, axes.x_min, axes.x_max);
    let v = Axes::fraction(raw_y, axes.y_min, axes.y_max);
    rot.apply(u, v)
}

/// Fractions of a `w` x `h` screen to the pixel they fall on.
pub fn to_px(pos: (f32, f32), w: u32, h: u32) -> (f32, f32) {
    (
        (pos.0 * w as f32).clamp(0.0, (w as f32 - 1.0).max(0.0)),
        (pos.1 * h as f32).clamp(0.0, (h as f32 - 1.0).max(0.0)),
    )
}

// ---- one finger from the multitouch stream -------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Down,
    Move,
    Up,
}

/// The one finger we follow; `x` and `y` are fractions of the landscape screen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TouchEvent {
    pub phase: Phase,
    pub x: f32,
    pub y: f32,
}

/// What the kernel sends, reduced to what we read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RawEvent {
    /// ABS_MT_SLOT
    Slot(i32),
    /// ABS_MT_TRACKING_ID: -1 lifts the finger of the current slot.
    TrackingId(i32),
    /// ABS_MT_POSITION_X, or ABS_X on a single-touch device
    X(i32),
    Y(i32),
    /// BTN_TOUCH (read on single-touch devices only)
    Touch(bool),
    /// SYN_REPORT
    Sync,
}

const SLOTS: usize = 10;

/// A finger that is still "down" but sent nothing for this long, while the screen reports something else, has lifted
/// without the kernel's lift reaching us (events lost across a suspend, a full queue: the evdev crate repairs the
/// slot it was reading when it saw `SYN_DROPPED`, not the other nine). It is let go of, else it would hold the
/// parser and every later finger would be ignored until that slot happened to be used and lifted again.
pub const GHOST_AFTER: Duration = Duration::from_secs(3);

/// Follows the first finger that lands (protocol B slots, or a single-touch device). A second finger is
/// ignored; when the first lifts the gesture ends, and nothing starts again until every finger is up.
pub struct MtParser {
    axes: Axes,
    rot: Rotation,
    single: bool,
    slot: usize,
    active: [bool; SLOTS],
    pos: [(i32, i32); SLOTS],
    /// The tracking id each slot has now: another id in a slot that never lifted is a new finger.
    ids: [i32; SLOTS],
    /// When each slot last sent anything.
    seen: [Instant; SLOTS],
    /// A new finger landed in a slot whose old finger never lifted.
    renewed: [bool; SLOTS],
    primary: Option<usize>,
    waiting_for_lift: bool,
    last: (f32, f32),
}

impl MtParser {
    pub fn new(axes: Axes, rot: Rotation, single_touch: bool) -> Self {
        MtParser {
            axes,
            rot,
            single: single_touch,
            slot: 0,
            active: [false; SLOTS],
            pos: [(0, 0); SLOTS],
            ids: [-1; SLOTS],
            seen: [Instant::now(); SLOTS],
            renewed: [false; SLOTS],
            primary: None,
            waiting_for_lift: false,
            last: (0.0, 0.0),
        }
    }

    /// Feeds one event; a report (`Sync`) can end in a `TouchEvent`.
    pub fn feed(&mut self, ev: RawEvent) -> Option<TouchEvent> {
        self.feed_at(ev, Instant::now())
    }

    /// `feed` at a given time (the tests' clock).
    pub fn feed_at(&mut self, ev: RawEvent, now: Instant) -> Option<TouchEvent> {
        // A single-touch device has one finger in slot 0, which BTN_TOUCH puts down and lifts.
        let slot = if self.single { 0 } else { self.slot };
        match ev {
            RawEvent::Slot(n) => self.slot = usize::try_from(n).unwrap_or(usize::MAX),
            RawEvent::TrackingId(id) if !self.single && slot < SLOTS => {
                // A finger lands in a slot that still holds one: the old one lifted unseen.
                if id >= 0 && self.active[slot] && self.ids[slot] != id {
                    self.renewed[slot] = true;
                }
                if id < 0 {
                    self.renewed[slot] = false;
                }
                self.active[slot] = id >= 0;
                self.ids[slot] = id;
                self.seen[slot] = now;
            }
            RawEvent::X(v) if slot < SLOTS => {
                self.pos[slot].0 = v;
                self.seen[slot] = now;
            }
            RawEvent::Y(v) if slot < SLOTS => {
                self.pos[slot].1 = v;
                self.seen[slot] = now;
            }
            RawEvent::Touch(down) if self.single => {
                if down && self.active[0] {
                    self.renewed[0] = true;
                }
                if !down {
                    self.renewed[0] = false;
                }
                self.active[0] = down;
                self.seen[0] = now;
            }
            RawEvent::Sync => return self.report(now),
            _ => {}
        }
        None
    }

    fn mapped(&self, slot: usize) -> (f32, f32) {
        map_raw(&self.axes, self.rot, self.pos[slot].0, self.pos[slot].1)
    }

    fn report(&mut self, now: Instant) -> Option<TouchEvent> {
        // Fingers whose lift never came (`GHOST_AFTER`).
        let mut lost_primary = false;
        for s in 0..SLOTS {
            if self.active[s] && now.saturating_duration_since(self.seen[s]) > GHOST_AFTER {
                self.active[s] = false;
                // What was waited for was this finger (the ones down now landed after): the wait is over.
                self.waiting_for_lift = false;
                self.renewed[s] = false;
                lost_primary |= self.primary == Some(s);
            }
        }
        if lost_primary {
            // Another finger is on the screen now: it is the touch (the recognizer starts over at a Down).
            self.primary = None;
            self.waiting_for_lift = false;
            return match self.active.iter().position(|a| *a) {
                Some(i) => {
                    self.primary = Some(i);
                    self.renewed[i] = false;
                    self.last = self.mapped(i);
                    Some(TouchEvent { phase: Phase::Down, x: self.last.0, y: self.last.1 })
                }
                None => Some(TouchEvent { phase: Phase::Up, x: self.last.0, y: self.last.1 }),
            };
        }
        if self.waiting_for_lift {
            self.waiting_for_lift = self.active.iter().any(|a| *a);
            return None;
        }
        match self.primary {
            None => {
                let i = self.active.iter().position(|a| *a)?;
                self.primary = Some(i);
                self.renewed[i] = false;
                self.last = self.mapped(i);
                Some(TouchEvent {
                    phase: Phase::Down,
                    x: self.last.0,
                    y: self.last.1,
                })
            }
            Some(i) if !self.active[i] => {
                self.primary = None;
                self.waiting_for_lift = self.active.iter().any(|a| *a);
                Some(TouchEvent {
                    phase: Phase::Up,
                    x: self.last.0,
                    y: self.last.1,
                })
            }
            // A new finger in the slot of one that never lifted: a new touch.
            Some(i) if std::mem::take(&mut self.renewed[i]) => {
                self.last = self.mapped(i);
                Some(TouchEvent {
                    phase: Phase::Down,
                    x: self.last.0,
                    y: self.last.1,
                })
            }
            Some(i) => {
                let now = self.mapped(i);
                if now == self.last {
                    return None;
                }
                self.last = now;
                Some(TouchEvent {
                    phase: Phase::Move,
                    x: now.0,
                    y: now.1,
                })
            }
        }
    }
}

// ---- taps and swipes -------------------------------------------------------------------------------------

/// A fingertip moves this far (pixels) without it being a swipe.
pub const SLOP_PX: f32 = 40.0;
/// Longer than this a still finger is a rest, not a tap.
pub const TAP_MAX: Duration = Duration::from_millis(600);

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Gesture {
    /// A short touch that stayed put, at the place it started.
    Tap { x: f32, y: f32 },
    /// A swipe moved by whole rows: positive when the finger went down (or right), negative up (or left).
    Scroll { steps: i32, vertical: bool },
}

#[derive(Clone, Copy, Debug)]
enum Stage {
    Idle,
    Pressed { x: f32, y: f32, at: Instant },
    Dragging { vertical: bool, last: (f32, f32), carry: f32 },
}

/// Taps and swipes from one finger. Positions are pixels; `pitch` is how far a row (or card) is from the
/// next along x and y (0 = that direction doesn't scroll).
pub struct Recognizer {
    stage: Stage,
}

impl Default for Recognizer {
    fn default() -> Self {
        Recognizer { stage: Stage::Idle }
    }
}

impl Recognizer {
    pub fn feed(&mut self, phase: Phase, x: f32, y: f32, now: Instant, pitch: (f32, f32)) -> Option<Gesture> {
        match (self.stage, phase) {
            (_, Phase::Down) => {
                self.stage = Stage::Pressed { x, y, at: now };
                None
            }
            (Stage::Idle, _) => None,
            (Stage::Pressed { x: x0, y: y0, at }, Phase::Up) => {
                self.stage = Stage::Idle;
                (now.saturating_duration_since(at) <= TAP_MAX).then_some(Gesture::Tap { x: x0, y: y0 })
            }
            (Stage::Pressed { x: x0, y: y0, .. }, Phase::Move) => {
                let (dx, dy) = (x - x0, y - y0);
                if dx.hypot(dy) <= SLOP_PX {
                    return None;
                }
                let vertical = dy.abs() >= dx.abs();
                self.drag(vertical, (x, y), if vertical { dy } else { dx }, pitch)
            }
            (Stage::Dragging { vertical, last, carry }, Phase::Move) => {
                let moved = if vertical { y - last.1 } else { x - last.0 };
                self.drag(vertical, (x, y), carry + moved, pitch)
            }
            (Stage::Dragging { .. }, Phase::Up) => {
                self.stage = Stage::Idle;
                None
            }
        }
    }

    /// Whole rows out of the distance dragged so far; what is left over carries on to the next move.
    fn drag(&mut self, vertical: bool, at: (f32, f32), carry: f32, pitch: (f32, f32)) -> Option<Gesture> {
        let p = if vertical { pitch.1 } else { pitch.0 };
        let steps = if p > 0.0 { (carry / p).trunc() as i32 } else { 0 };
        let carry = if p > 0.0 { carry - steps as f32 * p } else { 0.0 };
        self.stage = Stage::Dragging { vertical, last: at, carry };
        (steps != 0).then_some(Gesture::Scroll { steps, vertical })
    }

    /// Drops a gesture in progress (the screen changed hands).
    pub fn reset(&mut self) {
        self.stage = Stage::Idle;
    }
}

// ---- what is where ---------------------------------------------------------------------------------------

/// A rectangle in screen pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Area {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Area {
    pub fn new(x: i32, y: i32, w: i32, h: i32) -> Area {
        Area { x, y, w, h }
    }

    pub fn contains(&self, px: f32, py: f32) -> bool {
        px >= self.x as f32 && py >= self.y as f32 && px < (self.x + self.w) as f32 && py < (self.y + self.h) as f32
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    /// Row (or card) number `n` of the list on screen.
    Row(usize),
    /// A hint pill: pressing it is pressing that button.
    Button(Button),
    /// That button pressed this many times: a tab two steps away (History's Year from Week) goes there in one tap.
    Presses(Button, u8),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TapAction {
    Nothing,
    /// Move the selection by this many rows (negative: up).
    Steps(i32),
    /// The row was already selected: confirm it.
    Activate,
    Press(Button),
    /// The button pressed this many times in a row (`Target::Presses`).
    Presses(Button, u8),
}

/// What the last painted frame offered to touch: filled by the paint functions, emptied at the start of each.
#[derive(Default)]
pub struct HitMap {
    areas: Vec<(Area, Target)>,
    sel: Option<usize>,
    len: usize,
    pitch: (f32, f32),
}

impl HitMap {
    pub fn clear(&mut self) {
        self.areas.clear();
        self.sel = None;
        self.len = 0;
        self.pitch = (0.0, 0.0);
    }

    pub fn row(&mut self, area: Area, index: usize) {
        self.areas.push((area, Target::Row(index)));
    }

    pub fn button(&mut self, area: Area, button: Button) {
        self.areas.push((area, Target::Button(button)));
    }

    /// An area that presses `button` `times` times (a tab further than one step away).
    pub fn presses(&mut self, area: Area, button: Button, times: u8) {
        self.areas.push((area, if times == 1 { Target::Button(button) } else { Target::Presses(button, times) }));
    }

    /// The list's selected row, how many rows it has, and how far apart they are along x and y (0: not that way).
    pub fn list(&mut self, selected: usize, len: usize, pitch_x: f32, pitch_y: f32) {
        self.sel = Some(selected);
        self.len = len;
        self.pitch = (pitch_x, pitch_y);
    }

    /// A swipe's rows kept inside the list: some lists wrap around at their ends with the buttons, a swipe
    /// stops there.
    pub fn clamp_steps(&self, steps: i32) -> i32 {
        let Some(sel) = self.sel else { return 0 };
        let last = self.len.saturating_sub(1) as i32;
        (sel as i32 + steps).clamp(0, last) - sel as i32
    }

    pub fn pitch(&self) -> (f32, f32) {
        self.pitch
    }

    pub fn is_empty(&self) -> bool {
        self.areas.is_empty()
    }

    /// How many areas are registered so far (for `shift_x`).
    pub fn areas_len(&self) -> usize {
        self.areas.len()
    }

    /// The areas registered since `from` move `dx` pixels to the right: they were painted into a column that is
    /// then put in the middle of the screen (`app_column`).
    pub fn shift_x(&mut self, from: usize, dx: i32) {
        for (a, _) in self.areas.iter_mut().skip(from) {
            a.x += dx;
        }
    }

    /// What a tap at this pixel does. Later registrations lie on top of earlier ones.
    pub fn tap(&self, x: f32, y: f32) -> TapAction {
        let Some((_, target)) = self.areas.iter().rev().find(|(a, _)| a.contains(x, y)) else {
            return TapAction::Nothing;
        };
        match (*target, self.sel) {
            (Target::Button(b), _) => TapAction::Press(b),
            (Target::Presses(b, n), _) => TapAction::Presses(b, n),
            (Target::Row(i), Some(sel)) if i == sel => TapAction::Activate,
            (Target::Row(i), Some(sel)) => TapAction::Steps(i as i32 - sel as i32),
            (Target::Row(_), None) => TapAction::Nothing,
        }
    }
}

/// The button a hint pill's key stands for ("A", "SELECT", ...); `None` for combined keys such as "L1 R1".
pub fn hint_button(key: &str) -> Option<Button> {
    Some(match key {
        "A" => Button::East,
        "B" => Button::South,
        "X" => Button::North,
        "Y" => Button::West,
        "SELECT" => Button::Select,
        "START" => Button::Start,
        "L1" => Button::L1,
        "R1" => Button::R1,
        // "R2 Theme" on Settings → Look & feel → Look.
        "R2" => Button::R2,
        // The cabinet's "UP Search" on its top shelf.
        "UP" => Button::Up,
        // "RIGHT Another" on a card of Surprise me.
        "RIGHT" => Button::Right,
        _ => return None,
    })
}

// ---- touch for the game (pocket-core) --------------------------------------------------------------------

/// The finger as pocket-core sees it, shared between the thread reading the device and the frame loop.
#[derive(Default)]
pub struct Shared {
    inner: Mutex<SharedInner>,
}

#[derive(Default)]
struct SharedInner {
    pos: Option<(f32, f32)>,
    latch: Option<(f32, f32)>,
    ignore_until_up: bool,
}

impl Shared {
    pub fn feed(&self, ev: &TouchEvent) {
        let mut s = self.inner.lock().unwrap();
        let at = Some((ev.x, ev.y));
        match ev.phase {
            Phase::Down => {
                s.ignore_until_up = false;
                s.pos = at;
                s.latch = at;
            }
            Phase::Move if s.pos.is_some() => s.pos = at,
            Phase::Move => {}
            Phase::Up => {
                s.ignore_until_up = false;
                s.pos = None;
            }
        }
    }

    /// Where the finger is for the frame about to run (fractions of the screen). A tap shorter than a
    /// frame still counts for one frame.
    pub fn frame(&self) -> Option<(f32, f32)> {
        let mut s = self.inner.lock().unwrap();
        if s.pos.is_some() {
            s.latch = None;
            return s.pos;
        }
        s.latch.take()
    }

    /// Back from a pause or sleep: a tap made meanwhile was the menu's, and a finger still down stays
    /// out of the game until it lifts.
    pub fn after_pause(&self) {
        let mut s = self.inner.lock().unwrap();
        s.ignore_until_up = s.pos.is_some();
        s.pos = None;
        s.latch = None;
    }
}

/// The rectangle of a picture on screen, in pixels, the size of the frame drawn into it and how many quarter turns
/// counter-clockwise it is drawn with (SET_ROTATION and Options → Picture → Screen orientation, `video::frame_point`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Picture {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub frame_w: u32,
    pub frame_h: u32,
    pub turns: u32,
}

/// A part of the frame, as fractions of its width and height.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Part {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// Where the Nintendo DS's bottom (touch) screen is in the frame melonDS DS delivers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Bottom {
    /// The layout shows only the top screen: touching does nothing.
    Hidden,
    Screen(Part),
    /// A layout we don't know (hybrid, large screen, rotated): the core decides.
    Unknown,
}

/// The bottom screen in a frame of `frame_w` x `frame_h`, for the first of the configured layouts
/// (`melonds_screen_layout1..N`, such as "top-bottom") that has the frame's shape: stacked (256 wide, two
/// screens high, possibly with a gap), side by side (two screens wide) or a single screen.
pub fn nds_bottom(frame_w: u32, frame_h: u32, layouts: &[&str]) -> Bottom {
    /// The widest gap melonDS DS offers between two screens, in DS pixels.
    const MAX_GAP: u32 = 126;
    const SCREEN_W: u32 = 256;
    const SCREEN_H: u32 = 192;
    let first = |names: [&str; 2]| layouts.iter().find_map(|l| names.iter().position(|n| n == l));
    let (w, h) = (frame_w, frame_h);
    let whole = Part {
        x: 0.0,
        y: 0.0,
        w: 1.0,
        h: 1.0,
    };
    // Stacked or a single screen: one screen wide, at some whole scale.
    let scale = w / SCREEN_W;
    if scale >= 1 && w == SCREEN_W * scale {
        let screen = SCREEN_H * scale;
        if h == screen {
            return match first(["top", "bottom"]) {
                Some(0) => Bottom::Hidden,
                Some(_) => Bottom::Screen(whole),
                None => Bottom::Unknown,
            };
        }
        if h >= 2 * screen && h <= 2 * screen + MAX_GAP * scale {
            let fh = h as f32;
            return match first(["top-bottom", "bottom-top"]) {
                Some(0) => Bottom::Screen(Part {
                    x: 0.0,
                    y: (h - screen) as f32 / fh,
                    w: 1.0,
                    h: screen as f32 / fh,
                }),
                Some(_) => Bottom::Screen(Part {
                    x: 0.0,
                    y: 0.0,
                    w: 1.0,
                    h: screen as f32 / fh,
                }),
                None => Bottom::Unknown,
            };
        }
    }
    // Side by side: one screen high.
    let scale = h / SCREEN_H;
    if scale >= 1 && h == SCREEN_H * scale {
        let screen = SCREEN_W * scale;
        if w >= 2 * screen && w <= 2 * screen + MAX_GAP * scale {
            let fw = w as f32;
            return match first(["left-right", "right-left"]) {
                Some(0) => Bottom::Screen(Part {
                    x: (w - screen) as f32 / fw,
                    y: 0.0,
                    w: screen as f32 / fw,
                    h: 1.0,
                }),
                Some(_) => Bottom::Screen(Part {
                    x: 0.0,
                    y: 0.0,
                    w: screen as f32 / fw,
                    h: 1.0,
                }),
                None => Bottom::Unknown,
            };
        }
    }
    Bottom::Unknown
}

/// A libretro pointer coordinate (-32767..32767 across the frame) for a fraction of it.
pub fn pointer_value(fraction: f32) -> i16 {
    ((fraction.clamp(0.0, 1.0) * 2.0 - 1.0) * 32767.0).round() as i16
}

/// The touch (fractions of the screen) as a libretro pointer for the game in `system`, or `None` when it
/// isn't on the picture, or for the Nintendo DS when it isn't on the bottom screen.
pub fn pointer_for(system: &str, touch: (f32, f32), screen: (u32, u32), picture: Option<Picture>, layouts: &[&str]) -> Option<(i16, i16)> {
    let pic = picture?;
    let (sx, sy) = (touch.0 * screen.0 as f32, touch.1 * screen.1 as f32);
    if sx < pic.x || sy < pic.y || sx >= pic.x + pic.w || sy >= pic.y + pic.h {
        return None;
    }
    let (fx, fy) = crate::video::frame_point(pic.turns, (sx - pic.x) / pic.w, (sy - pic.y) / pic.h);
    if system == "nds" {
        match nds_bottom(pic.frame_w, pic.frame_h, layouts) {
            Bottom::Hidden => return None,
            Bottom::Screen(p) if fx < p.x || fy < p.y || fx >= p.x + p.w || fy >= p.y + p.h => return None,
            _ => {}
        }
    }
    Some((pointer_value(fx), pointer_value(fy)))
}

// ---- keeping the touch screen read -----------------------------------------------------------------------

/// Why a reader returned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReaderEnd {
    /// Reading failed: the panel's driver was reloaded, the node deleted, the device reset.
    Lost,
    /// `send` refused an event: nobody listens any more, there is no point in reading on.
    Closed,
}

/// How a reading ended and how long it had gone on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Read {
    pub end: ReaderEnd,
    pub lived: Duration,
}

/// Raw events of one device, a batch at a time (what one `read` of the evdev node returns).
pub trait Source {
    /// The next batch, or `None` when reading failed.
    fn next_batch(&mut self) -> Option<Vec<RawEvent>>;
}

/// Reads `src` until it fails or `send` refuses: raw events into taps. The parser is new for every call, so a reopened
/// panel never inherits a finger that was down when the old reading ended (`waiting_for_lift`, a slot still active).
pub fn read_stream(src: &mut impl Source, axes: Axes, rot: Rotation, single_touch: bool, send: &dyn Fn(TouchEvent) -> bool) -> ReaderEnd {
    let mut parser = MtParser::new(axes, rot, single_touch);
    loop {
        let Some(batch) = src.next_batch() else { return ReaderEnd::Lost };
        for raw in batch {
            if let Some(t) = parser.feed(raw) {
                if !send(t) {
                    return ReaderEnd::Closed;
                }
            }
        }
    }
}

/// The wait before the first new look after a miss; doubled with every miss up to `RETRY_MAX`.
pub const RETRY_FIRST: Duration = Duration::from_millis(250);
/// The longest wait between looks for a touch screen that is not there (a device without one costs a look every 30 s).
pub const RETRY_MAX: Duration = Duration::from_secs(30);
/// A reading that went on this long was a working touch screen: its loss starts the waits over. One that ended sooner
/// (a panel that opens and fails again) keeps lengthening them, so a flapping panel is not opened in a loop.
pub const STABLE: Duration = Duration::from_secs(5);

/// How long to wait before the look after `misses` misses (0: a screen that worked for a while was just lost).
pub fn retry_after(misses: u32) -> Duration {
    RETRY_FIRST.saturating_mul(1u32 << misses.min(10)).min(RETRY_MAX)
}

/// Where the touch screen comes from, fakeable in tests.
pub trait Bus {
    type Dev;
    /// One look for the screen.
    fn open(&mut self) -> Option<(String, Self::Dev)>;
    /// Reads it until it is lost or nobody listens.
    fn read(&mut self, dev: Self::Dev, send: &dyn Fn(TouchEvent) -> bool) -> Read;
    /// Waits for something to change, at most `timeout`. False ends the supervision.
    fn wait(&mut self, timeout: Duration) -> bool;
}

/// Keeps the touch screen read like the pads (`pad::linux::supervise`): looks for it until it is there, reads it, and
/// when the reading fails looks again, with waits that grow while it stays away (`retry_after`), never in a loop. `log`
/// gets one line for every change (found, lost, not there yet), none for a look that finds what the last one found.
/// Returns when `send` refuses an event or `wait` says so.
pub fn supervise_with<B: Bus>(bus: &mut B, send: &dyn Fn(TouchEvent) -> bool, log: &mut dyn FnMut(String)) {
    let mut misses = 0u32;
    let mut had_one = false;
    // Said already that it is not there (a loss says so too): not said again at every look.
    let mut told = false;
    loop {
        let wait = match bus.open() {
            Some((name, dev)) => {
                log(if had_one { format!("touch: {name} (again)") } else { format!("touch: {name}") });
                had_one = true;
                let read = bus.read(dev, send);
                if read.end == ReaderEnd::Closed {
                    return;
                }
                log(format!("touch: lost {name} after {} s (read error); looking for it again", read.lived.as_secs()));
                told = true;
                if read.lived >= STABLE {
                    misses = 0;
                } else {
                    misses = misses.saturating_add(1);
                }
                retry_after(misses.saturating_sub(1))
            }
            None => {
                if !told {
                    log(format!("touch: no touch screen yet; looking again (every {} s at most)", RETRY_MAX.as_secs()));
                    told = true;
                }
                let wait = retry_after(misses);
                misses = misses.saturating_add(1);
                wait
            }
        };
        if !bus.wait(wait) {
            return;
        }
    }
}

/// Why a touch did nothing. The suppression is on purpose (a game owns the screen, a sleeping Pocket is not woken by a
/// touch in a pocket, the picture is on a TV); the log line says which, so a report of "touch didn't work" can tell it
/// from a touch screen that is not read (`supervise_with`'s lines).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ignored {
    /// The picture is on a TV and the panel is dark.
    TvOutput,
    /// Not awake: light or deep sleep, switching off, or the screen off with the game going on.
    Asleep,
    /// A game or app is running: it has the screen (pocket-core reads the touch screen itself).
    Game,
    /// A game is being started (the launch picture).
    Launching,
    /// The pause menu is open (button-only).
    PauseMenu,
    /// The first-start setup is open (button-only).
    Setup,
    /// A system panel other than quick settings and the power menu is open (the time page); buttons only.
    SystemPanel,
    /// The last frame left nothing to touch.
    NothingToTouch,
}

impl Ignored {
    pub fn text(self) -> &'static str {
        match self {
            Ignored::TvOutput => "the picture is on a TV and the panel is dark",
            Ignored::Asleep => "the Pocket is asleep or the screen is off (only a button wakes it)",
            Ignored::Game => "a game or app is running and has the screen",
            Ignored::Launching => "a game is starting",
            Ignored::PauseMenu => "the pause menu is open (buttons only)",
            Ignored::Setup => "the first-start setup is open (buttons only)",
            Ignored::SystemPanel => "a system panel is open (buttons only)",
            Ignored::NothingToTouch => "the last frame had nothing to touch",
        }
    }
}

/// Says why touches are ignored, once per change: the first ignored touch after a touch was acted on, or after
/// another reason, gets a line; the rest of that spell none. A phone in a pocket or a game played by touch does not
/// fill the log.
#[derive(Default)]
pub struct IgnoreLog {
    last: Option<Ignored>,
}

impl IgnoreLog {
    /// A touch was ignored for `why`: the line to log, if this is the first of its kind since the last change.
    pub fn ignored(&mut self, why: Ignored) -> Option<String> {
        if self.last == Some(why) {
            return None;
        }
        self.last = Some(why);
        Some(format!("touch: ignored, {}", why.text()))
    }

    /// A touch was acted on: the next ignored one gets its line again.
    pub fn accepted(&mut self) {
        self.last = None;
    }
}

// ---- the device (Linux) ----------------------------------------------------------------------------------

#[cfg(target_os = "linux")]
pub mod linux {
    //! Opens the touch screen and reads it on its own thread. Like the pad, it is read without a grab, so
    //! sway still sees it.

    use super::{read_stream, supervise_with, Axes, Bus, Read, ReaderEnd, RawEvent, Rotation, Source, TouchEvent, RETRY_MAX};
    use std::time::{Duration, Instant};
    use evdev::{AbsoluteAxisCode, Device, EventSummary, KeyCode, PropType, SynchronizationCode};

    /// The touch screen: a direct-input device with multitouch (or single-touch) position axes. The
    /// Mini V2's panel is "generic ft5x06 (8d)"; its dual-screen add-on has touch screens of its own that
    /// belong to the other display and are left alone. Keeping the choice in one place.
    pub fn open() -> Option<(String, Device)> {
        let is_screen = |d: &Device| {
            let axes = d.supported_absolute_axes();
            let has = |a: AbsoluteAxisCode| axes.is_some_and(|x| x.contains(a));
            let multi = has(AbsoluteAxisCode::ABS_MT_POSITION_X) && has(AbsoluteAxisCode::ABS_MT_POSITION_Y);
            let single = has(AbsoluteAxisCode::ABS_X) && has(AbsoluteAxisCode::ABS_Y) && d.supported_keys().is_some_and(|k| k.contains(KeyCode::BTN_TOUCH));
            // DIRECT: the fingers are on the picture itself (a touchpad, like the DualSense's, is not).
            d.properties().contains(PropType::DIRECT) && (multi || single)
        };
        // Every other node is let go of on the closing thread: each close takes the kernel 7-45 ms
        // (`pad::close_later`), some 150 ms for the Mini V2's ten other nodes.
        let mut found: Vec<(String, Device)> = Vec::new();
        for (_, d) in evdev::enumerate() {
            if is_screen(&d) {
                found.push((d.name().unwrap_or("touch screen").to_string(), d));
            } else {
                crate::pad::close_later(Box::new(d));
            }
        }
        found.sort_by_key(|(name, _)| (!name.contains("ft5x06"), name.contains("RDS") || name.contains("(a0)")));
        let mut found = found.into_iter();
        let screen = found.next();
        // The dual-screen add-on's panels, not chosen: closed the same way.
        found.for_each(|(_, d)| crate::pad::close_later(Box::new(d)));
        screen
    }

    /// Reads the device until it goes away or `send` returns false.
    pub fn pump(dev: Device, screen_landscape: bool, send: impl Fn(TouchEvent) -> bool) {
        pump_read(dev, screen_landscape, &send);
    }

    /// The events of one evdev node as `RawEvent`s.
    struct EvdevNode {
        dev: Device,
        multi: bool,
        code_x: AbsoluteAxisCode,
        code_y: AbsoluteAxisCode,
    }

    impl Source for EvdevNode {
        fn next_batch(&mut self) -> Option<Vec<RawEvent>> {
            let events = self.dev.fetch_events().ok()?;
            let (multi, code_x, code_y) = (self.multi, self.code_x, self.code_y);
            Some(
                events
                    .filter_map(|ev| {
                        Some(match ev.destructure() {
                            EventSummary::AbsoluteAxis(_, AbsoluteAxisCode::ABS_MT_SLOT, v) if multi => RawEvent::Slot(v),
                            EventSummary::AbsoluteAxis(_, AbsoluteAxisCode::ABS_MT_TRACKING_ID, v) if multi => RawEvent::TrackingId(v),
                            EventSummary::AbsoluteAxis(_, c, v) if c == code_x => RawEvent::X(v),
                            EventSummary::AbsoluteAxis(_, c, v) if c == code_y => RawEvent::Y(v),
                            EventSummary::Key(_, KeyCode::BTN_TOUCH, v) if !multi => RawEvent::Touch(v != 0),
                            EventSummary::Synchronization(_, SynchronizationCode::SYN_REPORT, _) => RawEvent::Sync,
                            _ => return None,
                        })
                    })
                    .collect(),
            )
        }
    }

    fn pump_read(dev: Device, screen_landscape: bool, send: &dyn Fn(TouchEvent) -> bool) -> ReaderEnd {
        let multi = dev.supported_absolute_axes().is_some_and(|a| a.contains(AbsoluteAxisCode::ABS_MT_POSITION_X));
        let (code_x, code_y) = if multi {
            (AbsoluteAxisCode::ABS_MT_POSITION_X, AbsoluteAxisCode::ABS_MT_POSITION_Y)
        } else {
            (AbsoluteAxisCode::ABS_X, AbsoluteAxisCode::ABS_Y)
        };
        let range = |code| {
            dev.get_absinfo()
                .ok()
                .and_then(|mut it| it.find(|(c, _)| *c == code))
                .map(|(_, i)| (i.minimum(), i.maximum()))
        };
        let (pw, ph) = crate::device::current().panel_size();
        let ((x_min, x_max), (y_min, y_max)) = (range(code_x).unwrap_or((0, pw as i32 - 1)), range(code_y).unwrap_or((0, ph as i32 - 1)));
        let axes = Axes { x_min, x_max, y_min, y_max };
        let fbcon = std::fs::read_to_string("/sys/class/graphics/fbcon/rotate")
            .ok()
            .and_then(|s| s.trim().parse::<i32>().ok());
        let rot = Rotation::resolve(std::env::var("POCKETUI_TOUCH_ROTATE").ok().as_deref(), fbcon, axes.portrait(), screen_landscape);
        println!(
            "touch screen: {}x{} panel, turned {rot:?}{}",
            x_max - x_min + 1,
            y_max - y_min + 1,
            if multi { "" } else { " (single touch)" }
        );
        read_stream(&mut EvdevNode { dev, multi, code_x, code_y }, axes, rot, !multi, send)
    }

    /// Between two looks that find nothing, at least this long, however often /dev/input changes: every look opens
    /// every event node (some 150 ms on the Mini V2).
    const MIN_GAP: Duration = Duration::from_millis(500);

    /// Whether a look can be skipped: the nodes in /dev/input are the same as at the last look that found nothing, and
    /// that was less than `RETRY_MAX` ago. (A touch screen cannot appear without a node, so this costs a directory
    /// read instead of opening every node; the long wait is for a driver that makes its device ready after its node.)
    pub fn same_nodes(missed: Option<&(Vec<crate::pad::Node>, Instant)>, now: &[crate::pad::Node], at: Instant) -> bool {
        missed.is_some_and(|(nodes, when)| nodes == now && at.saturating_duration_since(*when) < RETRY_MAX)
    }

    /// /dev/input as the supervisor sees it: looked at when it changes (inotify, like the pads), else when a wait ends.
    struct EvdevTouch {
        watch: Option<crate::pad::DirWatch>,
        tried: Instant,
        missed: Option<(Vec<crate::pad::Node>, Instant)>,
    }

    impl Bus for EvdevTouch {
        type Dev = Device;

        fn open(&mut self) -> Option<(String, Device)> {
            self.tried = Instant::now();
            let nodes = crate::pad::event_nodes(std::path::Path::new("/dev/input"));
            if same_nodes(self.missed.as_ref(), &nodes, self.tried) {
                return None;
            }
            let found = open();
            self.missed = found.is_none().then_some((nodes, self.tried));
            found
        }

        fn read(&mut self, dev: Device, send: &dyn Fn(TouchEvent) -> bool) -> Read {
            let start = Instant::now();
            let end = pump_read(dev, true, send);
            Read { end, lived: start.elapsed() }
        }

        fn wait(&mut self, timeout: Duration) -> bool {
            match &self.watch {
                Some(w) => {
                    w.wait(timeout);
                }
                None => std::thread::sleep(timeout),
            }
            // Woken early by some other device coming or going: not a new look at once.
            std::thread::sleep(MIN_GAP.saturating_sub(self.tried.elapsed()));
            true
        }
    }

    /// Reads the touch screen for good (`touch::supervise_with`): found now, or when it shows up, and again after a read
    /// error. Blocks, so it runs on a thread of its own; returns when `send` says nobody listens. The first look is at
    /// once, as it was before the supervision.
    pub fn supervise(send: impl Fn(TouchEvent) -> bool) {
        // Watched before the first look, so a node that appears in between is not missed.
        let watch = crate::pad::DirWatch::new(std::path::Path::new("/dev/input"));
        let mut bus = EvdevTouch { watch, tried: Instant::now(), missed: None };
        supervise_with(&mut bus, &send, &mut |line| println!("{line}"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PANEL: Axes = Axes {
        x_min: 0,
        x_max: 1079,
        y_min: 0,
        y_max: 1239,
    };

    fn near(a: (f32, f32), b: (f32, f32)) {
        assert!((a.0 - b.0).abs() < 0.002 && (a.1 - b.1).abs() < 0.002, "{a:?} != {b:?}");
    }

    // ---- coordinates -------------------------------------------------------------------------------------

    #[test]
    fn rotation_90_puts_the_panels_left_edge_on_top() {
        // sway "transform 90": the landscape screen's top edge is the panel's left edge.
        near(map_raw(&PANEL, Rotation::Deg90, 0, 0), (1.0, 0.0));
        near(map_raw(&PANEL, Rotation::Deg90, 1079, 0), (1.0, 1.0));
        near(map_raw(&PANEL, Rotation::Deg90, 0, 1239), (0.0, 0.0));
        near(map_raw(&PANEL, Rotation::Deg90, 1079, 1239), (0.0, 1.0));
    }

    #[test]
    fn rotation_270_is_the_other_way_round() {
        near(map_raw(&PANEL, Rotation::Deg270, 0, 0), (0.0, 1.0));
        near(map_raw(&PANEL, Rotation::Deg270, 1079, 0), (0.0, 0.0));
        near(map_raw(&PANEL, Rotation::Deg270, 0, 1239), (1.0, 1.0));
        near(map_raw(&PANEL, Rotation::Deg270, 1079, 1239), (1.0, 0.0));
    }

    #[test]
    fn rotation_0_and_180() {
        let landscape = Axes {
            x_min: 0,
            x_max: 1239,
            y_min: 0,
            y_max: 1079,
        };
        near(map_raw(&landscape, Rotation::Deg0, 310, 270), (0.25, 0.25));
        near(map_raw(&landscape, Rotation::Deg180, 310, 270), (0.75, 0.75));
    }

    /// Mini's cst340 swaps 1920x1080 axes in the kernel and inverts y; evdev is portrait afterwards.
    /// Original Odin uses inverted x rmi4; Portal uses 1080x1920 ft5426 without inversion.
    #[test]
    fn odin_family_uses_kernel_corrected_portrait_axes_and_each_panels_turn() {
        let axes = Axes { x_min: 0, x_max: 1079, y_min: 0, y_max: 1919 };
        for (device, fbcon, turn) in [("odin2mini", 1, Rotation::Deg270), ("odin2", 1, Rotation::Deg270), ("odin2portal", 3, Rotation::Deg90)] {
            assert_eq!(crate::device::from_override(device).panel_size(), (1080, 1920));
            assert_eq!(Rotation::resolve(None, Some(fbcon), axes.portrait(), true), turn);
            let (top_left, bottom_right) = if turn == Rotation::Deg270 { ((1079, 0), (0, 1919)) } else { ((0, 1919), (1079, 0)) };
            near(map_raw(&axes, turn, top_left.0, top_left.1), (0.0, 0.0));
            near(map_raw(&axes, turn, bottom_right.0, bottom_right.1), (1.0, 1.0));
        }
    }

    #[test]
    fn rp6_and_nova_use_their_exact_panel_geometry() {
        let rp6 = Axes { x_min: 0, x_max: 1079, y_min: 0, y_max: 1919 };
        let nova = Axes { x_min: 0, x_max: 1279, y_min: 0, y_max: 959 };
        assert_eq!(Rotation::resolve(None, Some(3), rp6.portrait(), true), Rotation::Deg90);
        near(map_raw(&rp6, Rotation::Deg90, 0, 1919), (0.0, 0.0));
        near(map_raw(&rp6, Rotation::Deg90, 1079, 0), (1.0, 1.0));
        assert_eq!(Rotation::resolve(None, Some(0), nova.portrait(), true), Rotation::Deg0);
        near(map_raw(&nova, Rotation::Deg0, 0, 0), (0.0, 0.0));
        near(map_raw(&nova, Rotation::Deg0, 1279, 959), (1.0, 1.0));
        assert_eq!(crate::device::from_override("nova").panel_size(), (1280, 960));
    }

    /// The experimental SM8250 siblings (docs/DEVICES.md). The kernel's touch driver applies the device tree's
    /// `touchscreen-inverted-x/y` itself, so pocket-ui gets panel coordinates and only turns them as the panel is
    /// turned: `rotation = <270>` on the Pocket 5 and Flip 2 is `fbcon/rotate` 3 (`Deg90`, sway's `transform 270`),
    /// `rotation = <90>` on both Minis is 1 (`Deg270`, as measured on the Mini V2).
    #[test]
    fn the_pocket_5_flip_2_and_mini_v1_panels_map_to_their_screens() {
        // (panel x range, y range, fbcon, screen w, h)
        for (px, py, fbcon, sw, sh) in [(1079, 1919, 3, 1920, 1080), (959, 1279, 1, 1280, 960)] {
            let axes = Axes { x_min: 0, x_max: px, y_min: 0, y_max: py };
            let rot = Rotation::resolve(None, Some(fbcon), axes.portrait(), sw > sh);
            assert_eq!(rot, Rotation::from_fbcon(fbcon), "{sw}x{sh}: the console's rotation is used");
            // Each panel corner lands on the screen corner the panel's mounting puts it at.
            let corner = |x: i32, y: i32| to_px(map_raw(&axes, rot, x, y), sw, sh);
            let (tl, br) = (corner(0, 0), corner(px, py));
            if fbcon == 3 {
                // Turned so the panel's left edge is the screen's top: its top-left corner is the screen's top right.
                assert!(tl.0 > sw as f32 - 2.0 && tl.1 < 1.0, "{tl:?}");
                assert!(br.0 < 1.0 && br.1 > sh as f32 - 2.0, "{br:?}");
            } else {
                // The Mini's way round: the panel's top-left corner is the screen's bottom left.
                assert!(tl.0 < 1.0 && tl.1 > sh as f32 - 2.0, "{tl:?}");
                assert!(br.0 > sw as f32 - 2.0 && br.1 < 1.0, "{br:?}");
            }
            // The middle stays the middle, and a tap maps back to the panel point it came from.
            near(map_raw(&axes, rot, px / 2, py / 2), (0.5, 0.5));
            let (u, v) = rot.invert(0.3, 0.8);
            near(rot.apply(u, v), (0.3, 0.8));
        }
    }

    #[test]
    fn the_middle_stays_in_the_middle() {
        for rot in [Rotation::Deg0, Rotation::Deg90, Rotation::Deg180, Rotation::Deg270] {
            near(map_raw(&PANEL, rot, 540, 620), (0.5, 0.5));
        }
    }

    #[test]
    fn out_of_range_values_are_clamped() {
        near(map_raw(&PANEL, Rotation::Deg90, -50, 5000), (0.0, 0.0));
        near(map_raw(&PANEL, Rotation::Deg0, 9000, -9), (1.0, 0.0));
    }

    #[test]
    fn invert_undoes_apply() {
        for rot in [Rotation::Deg0, Rotation::Deg90, Rotation::Deg180, Rotation::Deg270] {
            let (x, y) = rot.apply(0.2, 0.7);
            near(rot.invert(x, y), (0.2, 0.7));
        }
    }

    #[test]
    fn a_tap_lands_on_the_pixel_under_it() {
        // The top-left of the screen is the panel's bottom-left corner under transform 90.
        let (x, y) = to_px(map_raw(&PANEL, Rotation::Deg90, 0, 1239), 1240, 1080);
        assert!(x < 1.0 && y < 1.0, "{x} {y}");
        // The far corner stays on the screen.
        let (x, y) = to_px((1.0, 1.0), 1240, 1080);
        assert_eq!((x, y), (1239.0, 1079.0));
    }

    #[test]
    fn degrees_and_fbcon_names() {
        assert_eq!(Rotation::from_degrees(90), Some(Rotation::Deg90));
        assert_eq!(Rotation::from_degrees(270), Some(Rotation::Deg270));
        assert_eq!(Rotation::from_degrees(45), None);
        assert_eq!(Rotation::from_fbcon(0), Rotation::Deg0);
        assert_eq!(Rotation::from_fbcon(1), Rotation::Deg270, "FB_ROTATE_CW");
        assert_eq!(Rotation::from_fbcon(2), Rotation::Deg180);
        assert_eq!(Rotation::from_fbcon(3), Rotation::Deg90, "FB_ROTATE_CCW");
        assert!(Rotation::Deg90.swaps_axes() && Rotation::Deg270.swaps_axes());
        assert!(!Rotation::Deg0.swaps_axes() && !Rotation::Deg180.swaps_axes());
    }

    #[test]
    fn the_rotation_follows_the_console_or_the_shapes() {
        // The Mini V2: portrait panel, landscape screen, fbcon rotate 1 (clockwise).
        assert_eq!(Rotation::resolve(None, Some(1), true, true), Rotation::Deg270);
        assert_eq!(Rotation::resolve(None, Some(3), true, true), Rotation::Deg90);
        // fbcon says 0, but a portrait panel under a landscape screen needs a quarter turn.
        assert_eq!(Rotation::resolve(None, Some(0), true, true), Rotation::Deg90);
        assert_eq!(Rotation::resolve(None, None, true, true), Rotation::Deg90);
        // A landscape panel (the virtual device's) is not turned, whatever the console says.
        assert_eq!(Rotation::resolve(None, Some(1), false, true), Rotation::Deg0);
        assert_eq!(Rotation::resolve(None, None, false, true), Rotation::Deg0);
        assert_eq!(Rotation::resolve(None, Some(2), false, true), Rotation::Deg180);
        // The environment wins.
        assert_eq!(Rotation::resolve(Some("270"), Some(1), true, true), Rotation::Deg270);
        assert_eq!(Rotation::resolve(Some("0"), Some(1), true, true), Rotation::Deg0);
        assert_eq!(Rotation::resolve(Some("sideways"), Some(1), true, true), Rotation::Deg270, "an unreadable value falls back to the console");
    }

    #[test]
    fn axes_know_their_shape() {
        assert!(PANEL.portrait());
        assert!(!Axes {
            x_min: 0,
            x_max: 1239,
            y_min: 0,
            y_max: 1079
        }
        .portrait());
    }

    // ---- the multitouch stream ---------------------------------------------------------------------------

    fn parser() -> MtParser {
        MtParser::new(PANEL, Rotation::Deg90, false)
    }

    fn feed_all(p: &mut MtParser, evs: &[RawEvent]) -> Vec<TouchEvent> {
        evs.iter().filter_map(|e| p.feed(*e)).collect()
    }

    #[test]
    fn one_finger_goes_down_moves_and_lifts() {
        let mut p = parser();
        let out = feed_all(
            &mut p,
            &[
                RawEvent::Slot(0),
                RawEvent::TrackingId(7),
                RawEvent::X(540),
                RawEvent::Y(620),
                RawEvent::Sync,
                RawEvent::X(600),
                RawEvent::Sync,
                RawEvent::TrackingId(-1),
                RawEvent::Sync,
            ],
        );
        let phases: Vec<Phase> = out.iter().map(|e| e.phase).collect();
        assert_eq!(phases, [Phase::Down, Phase::Move, Phase::Up]);
        near((out[0].x, out[0].y), (0.5, 0.5));
        // x on the panel is y on the screen under transform 90.
        near((out[1].x, out[1].y), (0.5, 600.0 / 1079.0));
        // The lift is where the finger was last.
        near((out[2].x, out[2].y), (out[1].x, out[1].y));
    }

    #[test]
    fn a_report_that_changes_nothing_says_nothing() {
        let mut p = parser();
        feed_all(&mut p, &[RawEvent::TrackingId(1), RawEvent::X(10), RawEvent::Y(10), RawEvent::Sync]);
        assert!(feed_all(&mut p, &[RawEvent::Sync, RawEvent::Sync]).is_empty());
        assert!(feed_all(&mut p, &[RawEvent::X(10), RawEvent::Sync]).is_empty(), "the same place again");
    }

    #[test]
    fn a_second_finger_is_ignored_and_the_first_ending_ends_the_gesture() {
        let mut p = parser();
        let out = feed_all(
            &mut p,
            &[
                RawEvent::Slot(0),
                RawEvent::TrackingId(1),
                RawEvent::X(100),
                RawEvent::Y(100),
                RawEvent::Sync,
                RawEvent::Slot(1),
                RawEvent::TrackingId(2),
                RawEvent::X(900),
                RawEvent::Y(900),
                RawEvent::Sync,
                RawEvent::Slot(0),
                RawEvent::TrackingId(-1),
                RawEvent::Sync,
                // The second finger moves on alone: no new gesture until it has lifted.
                RawEvent::Slot(1),
                RawEvent::X(800),
                RawEvent::Sync,
                RawEvent::TrackingId(-1),
                RawEvent::Sync,
            ],
        );
        let phases: Vec<Phase> = out.iter().map(|e| e.phase).collect();
        assert_eq!(phases, [Phase::Down, Phase::Up]);
        // After everything is up a new touch starts normally.
        let again = feed_all(
            &mut p,
            &[RawEvent::Slot(0), RawEvent::TrackingId(3), RawEvent::X(5), RawEvent::Y(5), RawEvent::Sync],
        );
        assert_eq!(again.len(), 1);
        assert_eq!(again[0].phase, Phase::Down);
    }

    #[test]
    fn a_second_finger_moving_does_not_move_the_first() {
        let mut p = parser();
        feed_all(
            &mut p,
            &[RawEvent::Slot(0), RawEvent::TrackingId(1), RawEvent::X(100), RawEvent::Y(100), RawEvent::Sync],
        );
        let out = feed_all(
            &mut p,
            &[RawEvent::Slot(1), RawEvent::TrackingId(2), RawEvent::X(900), RawEvent::Y(900), RawEvent::Sync],
        );
        assert!(out.is_empty());
    }

    // A finger lift that never arrives (events lost across a suspend, a full queue): "works again by itself" in the
    // incident of 2026-10-26 (Muse's review, 3b). The parser used to hold the dead finger for good.

    fn feed_secs(p: &mut MtParser, t0: Instant, secs: f64, evs: &[RawEvent]) -> Vec<TouchEvent> {
        evs.iter().filter_map(|e| p.feed_at(*e, t0 + Duration::from_secs_f64(secs))).collect()
    }

    fn land(slot: i32, id: i32, x: i32) -> [RawEvent; 5] {
        [RawEvent::Slot(slot), RawEvent::TrackingId(id), RawEvent::X(x), RawEvent::Y(x), RawEvent::Sync]
    }

    #[test]
    fn a_new_finger_in_the_slot_of_one_that_never_lifted_is_a_new_touch() {
        let (mut p, t0) = (parser(), Instant::now());
        assert_eq!(feed_secs(&mut p, t0, 0.0, &land(0, 1, 100))[0].phase, Phase::Down);
        // The lift was lost. The next finger is in the same slot with the next tracking id.
        let out = feed_secs(&mut p, t0, 10.0, &land(0, 2, 700));
        assert_eq!(out.iter().map(|e| e.phase).collect::<Vec<_>>(), [Phase::Down], "{out:?}");
        near((out[0].x, out[0].y), map_raw(&PANEL, Rotation::Deg90, 700, 700));
        let up = feed_secs(&mut p, t0, 10.1, &[RawEvent::TrackingId(-1), RawEvent::Sync]);
        assert_eq!(up[0].phase, Phase::Up);
        assert_eq!(feed_secs(&mut p, t0, 11.0, &land(0, 3, 5))[0].phase, Phase::Down, "and it goes on normally");
    }

    #[test]
    fn a_finger_that_never_lifted_does_not_hold_later_fingers_in_other_slots() {
        let (mut p, t0) = (parser(), Instant::now());
        feed_secs(&mut p, t0, 0.0, &land(0, 1, 100));
        // Slot 0's lift is lost; a finger lands in slot 1 a second later: still the old finger's time, it waits...
        assert!(feed_secs(&mut p, t0, 1.0, &land(1, 2, 900)).is_empty());
        // ...but not for long: the same finger moves after the old one has been silent for more than GHOST_AFTER.
        let out = feed_secs(&mut p, t0, 5.0, &[RawEvent::Slot(1), RawEvent::X(800), RawEvent::Sync]);
        assert_eq!(out.iter().map(|e| e.phase).collect::<Vec<_>>(), [Phase::Down], "the live finger is the touch now: {out:?}");
        near((out[0].x, out[0].y), map_raw(&PANEL, Rotation::Deg90, 800, 900));
        assert_eq!(feed_secs(&mut p, t0, 5.1, &[RawEvent::TrackingId(-1), RawEvent::Sync])[0].phase, Phase::Up);
    }

    #[test]
    fn a_lost_lift_behind_a_waiting_parser_lets_go_too() {
        let (mut p, t0) = (parser(), Instant::now());
        feed_secs(&mut p, t0, 0.0, &land(0, 1, 100));
        feed_secs(&mut p, t0, 0.1, &land(1, 2, 900));
        // The first finger lifts, the second one (still down) is waited for, and its lift is lost.
        assert_eq!(feed_secs(&mut p, t0, 0.5, &[RawEvent::Slot(0), RawEvent::TrackingId(-1), RawEvent::Sync])[0].phase, Phase::Up);
        // Without the fix every later finger was ignored from here on.
        assert!(feed_secs(&mut p, t0, 1.0, &land(2, 3, 300)).is_empty(), "within the grace time it is still waiting");
        let out = feed_secs(&mut p, t0, 20.0, &land(3, 4, 400));
        assert_eq!(out.first().map(|e| e.phase), Some(Phase::Down), "{out:?}");
    }

    #[test]
    fn a_finger_held_still_for_long_is_not_let_go_while_nothing_else_happens() {
        let (mut p, t0) = (parser(), Instant::now());
        feed_secs(&mut p, t0, 0.0, &land(0, 1, 100));
        // No events for ten seconds is a finger resting, not a lost lift; its own lift is still its end.
        let out = feed_secs(&mut p, t0, 10.0, &[RawEvent::TrackingId(-1), RawEvent::Sync]);
        assert_eq!(out.iter().map(|e| e.phase).collect::<Vec<_>>(), [Phase::Up]);
        // Nor does its own move after a long rest start a new touch.
        feed_secs(&mut p, t0, 20.0, &land(0, 5, 100));
        let moved = feed_secs(&mut p, t0, 30.0, &[RawEvent::X(200), RawEvent::Sync]);
        assert_eq!(moved.iter().map(|e| e.phase).collect::<Vec<_>>(), [Phase::Move]);
    }

    #[test]
    fn a_single_touch_device_uses_btn_touch() {
        let mut p = MtParser::new(PANEL, Rotation::Deg90, true);
        let out = feed_all(
            &mut p,
            &[
                RawEvent::X(540),
                RawEvent::Y(620),
                RawEvent::Touch(true),
                RawEvent::Sync,
                RawEvent::Y(700),
                RawEvent::Sync,
                RawEvent::Touch(false),
                RawEvent::Sync,
            ],
        );
        let phases: Vec<Phase> = out.iter().map(|e| e.phase).collect();
        assert_eq!(phases, [Phase::Down, Phase::Move, Phase::Up]);
        near((out[0].x, out[0].y), (0.5, 0.5));
    }

    #[test]
    fn a_hovering_single_touch_device_reports_nothing() {
        let mut p = MtParser::new(PANEL, Rotation::Deg90, true);
        assert!(feed_all(&mut p, &[RawEvent::X(5), RawEvent::Y(5), RawEvent::Sync]).is_empty());
    }

    // ---- taps and swipes ---------------------------------------------------------------------------------

    const PITCH: (f32, f32) = (0.0, 82.0);

    fn at(ms: u64) -> Instant {
        // One base instant for the whole test binary keeps the offsets comparable.
        static BASE: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
        *BASE.get_or_init(Instant::now) + Duration::from_millis(ms)
    }

    fn run(rec: &mut Recognizer, steps: &[(Phase, f32, f32, u64)], pitch: (f32, f32)) -> Vec<Gesture> {
        steps.iter().filter_map(|(p, x, y, ms)| rec.feed(*p, *x, *y, at(*ms), pitch)).collect()
    }

    #[test]
    fn a_short_touch_in_place_is_a_tap_where_it_began() {
        let mut r = Recognizer::default();
        let out = run(&mut r, &[(Phase::Down, 300.0, 400.0, 0), (Phase::Up, 305.0, 402.0, 120)], PITCH);
        assert_eq!(out, [Gesture::Tap { x: 300.0, y: 400.0 }]);
    }

    #[test]
    fn jitter_inside_the_slop_is_still_a_tap() {
        let mut r = Recognizer::default();
        let out = run(
            &mut r,
            &[
                (Phase::Down, 300.0, 400.0, 0),
                (Phase::Move, 320.0, 425.0, 40),
                (Phase::Move, 305.0, 410.0, 80),
                (Phase::Up, 305.0, 410.0, 100),
            ],
            PITCH,
        );
        assert_eq!(out, [Gesture::Tap { x: 300.0, y: 400.0 }]);
    }

    #[test]
    fn a_finger_resting_for_a_long_time_is_not_a_tap() {
        let mut r = Recognizer::default();
        let out = run(&mut r, &[(Phase::Down, 300.0, 400.0, 0), (Phase::Up, 300.0, 400.0, 900)], PITCH);
        assert!(out.is_empty());
        // The edge: right at the limit still counts.
        let out = run(
            &mut r,
            &[(Phase::Down, 300.0, 400.0, 2000), (Phase::Up, 300.0, 400.0, 2000 + TAP_MAX.as_millis() as u64)],
            PITCH,
        );
        assert_eq!(out.len(), 1);
    }

    #[test]
    fn a_swipe_moves_by_whole_rows_and_is_no_tap() {
        let mut r = Recognizer::default();
        let out = run(
            &mut r,
            &[
                (Phase::Down, 300.0, 600.0, 0),
                (Phase::Move, 300.0, 560.0, 20), // 40 px: not past the slop yet
                (Phase::Move, 300.0, 540.0, 40), // 60 px up: past it, but less than a row
                (Phase::Move, 300.0, 500.0, 60), // 100 px up: one row
                (Phase::Move, 300.0, 330.0, 80), // 270 px up: three rows in all
                (Phase::Up, 300.0, 330.0, 100),
            ],
            PITCH,
        );
        assert_eq!(
            out,
            [Gesture::Scroll { steps: -1, vertical: true }, Gesture::Scroll { steps: -2, vertical: true }]
        );
        assert!(out.iter().all(|g| !matches!(g, Gesture::Tap { .. })));
    }

    #[test]
    fn swiping_down_is_positive_and_back_and_forth_keeps_the_remainder() {
        let mut r = Recognizer::default();
        let out = run(
            &mut r,
            &[
                (Phase::Down, 300.0, 100.0, 0),
                (Phase::Move, 300.0, 190.0, 20),
                (Phase::Move, 300.0, 100.0, 40),
                (Phase::Up, 300.0, 100.0, 60),
            ],
            PITCH,
        );
        assert_eq!(
            out,
            [Gesture::Scroll { steps: 1, vertical: true }, Gesture::Scroll { steps: -1, vertical: true }]
        );
    }

    #[test]
    fn the_longer_way_decides_the_direction() {
        let mut r = Recognizer::default();
        // Mostly sideways, on a list that only scrolls vertically: nothing, and no tap either.
        let out = run(
            &mut r,
            &[
                (Phase::Down, 300.0, 400.0, 0),
                (Phase::Move, 420.0, 430.0, 20),
                (Phase::Move, 600.0, 450.0, 40),
                (Phase::Up, 600.0, 450.0, 60),
            ],
            PITCH,
        );
        assert!(out.is_empty());
    }

    #[test]
    fn a_carousel_swipe_left_steps_forward() {
        let mut r = Recognizer::default();
        let out = run(
            &mut r,
            &[(Phase::Down, 900.0, 400.0, 0), (Phase::Move, 500.0, 410.0, 20), (Phase::Up, 500.0, 410.0, 40)],
            (330.0, 0.0),
        );
        assert_eq!(out, [Gesture::Scroll { steps: -1, vertical: false }]);
    }

    #[test]
    fn a_move_or_lift_without_a_touch_is_nothing() {
        let mut r = Recognizer::default();
        assert!(run(&mut r, &[(Phase::Move, 1.0, 1.0, 0), (Phase::Up, 1.0, 1.0, 10)], PITCH).is_empty());
    }

    #[test]
    fn reset_drops_a_gesture() {
        let mut r = Recognizer::default();
        run(&mut r, &[(Phase::Down, 10.0, 10.0, 0)], PITCH);
        r.reset();
        assert!(run(&mut r, &[(Phase::Up, 10.0, 10.0, 50)], PITCH).is_empty());
    }

    // ---- the hit map -------------------------------------------------------------------------------------

    fn shelf() -> HitMap {
        let mut h = HitMap::default();
        for i in 0..5usize {
            h.row(Area::new(48, 112 + 82 * i as i32, 1100, 72), i);
        }
        h.list(1, 5, 0.0, 82.0);
        h.button(Area::new(48, 1000, 120, 36), Button::East);
        h.button(Area::new(200, 1000, 120, 36), Button::South);
        h
    }

    #[test]
    fn a_tap_on_another_row_moves_the_selection_there() {
        let h = shelf();
        assert_eq!(h.tap(500.0, 112.0 + 82.0 * 3.0 + 20.0), TapAction::Steps(2));
        assert_eq!(h.tap(500.0, 120.0), TapAction::Steps(-1));
    }

    #[test]
    fn a_tap_on_the_selected_row_confirms_it() {
        assert_eq!(shelf().tap(500.0, 112.0 + 82.0 + 30.0), TapAction::Activate);
    }

    #[test]
    fn a_tap_on_a_hint_pill_presses_its_button() {
        let h = shelf();
        assert_eq!(h.tap(100.0, 1010.0), TapAction::Press(Button::East));
        assert_eq!(h.tap(250.0, 1010.0), TapAction::Press(Button::South));
    }

    #[test]
    fn what_was_painted_last_is_on_top() {
        let mut h = shelf();
        // A pill over the last row.
        h.button(Area::new(48, 112 + 82 * 4, 200, 72), Button::West);
        assert_eq!(h.tap(100.0, 112.0 + 82.0 * 4.0 + 10.0), TapAction::Press(Button::West));
        assert_eq!(h.tap(600.0, 112.0 + 82.0 * 4.0 + 10.0), TapAction::Steps(3));
    }

    #[test]
    fn a_tap_between_rows_or_outside_does_nothing() {
        let h = shelf();
        assert_eq!(h.tap(500.0, 112.0 + 72.0 + 5.0), TapAction::Nothing, "the gap between two rows");
        assert_eq!(h.tap(10.0, 10.0), TapAction::Nothing);
        assert_eq!(HitMap::default().tap(10.0, 10.0), TapAction::Nothing);
    }

    #[test]
    fn the_edges_of_an_area_belong_to_it() {
        let a = Area::new(10, 20, 100, 50);
        assert!(a.contains(10.0, 20.0));
        assert!(a.contains(109.0, 69.0));
        assert!(!a.contains(110.0, 30.0));
        assert!(!a.contains(50.0, 70.0));
        assert!(!a.contains(9.0, 30.0));
    }

    #[test]
    fn clear_forgets_everything() {
        let mut h = shelf();
        h.clear();
        assert!(h.is_empty());
        assert_eq!(h.pitch(), (0.0, 0.0));
        assert_eq!(h.tap(100.0, 1010.0), TapAction::Nothing);
    }

    #[test]
    fn a_swipe_stops_at_the_ends_of_the_list() {
        let h = shelf();
        assert_eq!(h.clamp_steps(2), 2);
        assert_eq!(h.clamp_steps(9), 3, "row 1 of 5: three more");
        assert_eq!(h.clamp_steps(-9), -1);
        assert_eq!(HitMap::default().clamp_steps(3), 0);
    }

    #[test]
    fn the_pitch_is_what_the_list_said() {
        assert_eq!(shelf().pitch(), (0.0, 82.0));
    }

    #[test]
    fn hint_keys_name_buttons() {
        assert_eq!(hint_button("A"), Some(Button::East));
        assert_eq!(hint_button("B"), Some(Button::South));
        assert_eq!(hint_button("X"), Some(Button::North));
        assert_eq!(hint_button("Y"), Some(Button::West));
        assert_eq!(hint_button("SELECT"), Some(Button::Select));
        assert_eq!(hint_button("L1"), Some(Button::L1));
        assert_eq!(hint_button("R1"), Some(Button::R1));
        assert_eq!(hint_button("L1 R1"), None);
        assert_eq!(hint_button("< >"), None);
    }

    // ---- Nintendo DS -------------------------------------------------------------------------------------

    fn part(x: f32, y: f32, w: f32, h: f32) -> Bottom {
        Bottom::Screen(Part { x, y, w, h })
    }

    #[test]
    fn stacked_layouts_have_the_bottom_screen_in_the_lower_half() {
        assert_eq!(nds_bottom(256, 384, &["top-bottom"]), part(0.0, 0.5, 1.0, 0.5));
        assert_eq!(nds_bottom(256, 384, &["bottom-top"]), part(0.0, 0.0, 1.0, 0.5));
        // Software rendering at 2x.
        assert_eq!(nds_bottom(512, 768, &["top-bottom"]), part(0.0, 0.5, 1.0, 0.5));
    }

    #[test]
    fn a_gap_belongs_to_neither_screen() {
        match nds_bottom(256, 392, &["top-bottom"]) {
            Bottom::Screen(p) => {
                assert!((p.y - 200.0 / 392.0).abs() < 1e-4 && (p.h - 192.0 / 392.0).abs() < 1e-4, "{p:?}");
                assert_eq!((p.x, p.w), (0.0, 1.0));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn side_by_side_layouts_have_it_on_the_right_or_the_left() {
        assert_eq!(nds_bottom(512, 192, &["left-right"]), part(0.5, 0.0, 0.5, 1.0));
        assert_eq!(nds_bottom(512, 192, &["right-left"]), part(0.0, 0.0, 0.5, 1.0));
    }

    #[test]
    fn one_screen_alone() {
        assert_eq!(nds_bottom(256, 192, &["top"]), Bottom::Hidden);
        assert_eq!(nds_bottom(256, 192, &["bottom"]), part(0.0, 0.0, 1.0, 1.0));
    }

    #[test]
    fn the_first_layout_of_the_frames_shape_decides() {
        let layouts = ["left-right", "top-bottom", "top", "bottom-top"];
        assert_eq!(nds_bottom(256, 384, &layouts), part(0.0, 0.5, 1.0, 0.5));
        assert_eq!(nds_bottom(512, 192, &layouts), part(0.5, 0.0, 0.5, 1.0));
        assert_eq!(nds_bottom(256, 192, &layouts), Bottom::Hidden);
    }

    #[test]
    fn shapes_or_layouts_we_do_not_know_are_left_to_the_core() {
        assert_eq!(nds_bottom(256, 384, &["left-right"]), Bottom::Unknown, "no stacked layout configured");
        assert_eq!(nds_bottom(256, 384, &[]), Bottom::Unknown);
        assert_eq!(nds_bottom(300, 500, &["top-bottom"]), Bottom::Unknown);
        assert_eq!(nds_bottom(0, 0, &["top-bottom"]), Bottom::Unknown);
        assert_eq!(nds_bottom(192, 256, &["rotate-left"]), Bottom::Unknown);
    }

    #[test]
    fn pointer_values_span_the_frame() {
        assert_eq!(pointer_value(0.0), -32767);
        assert_eq!(pointer_value(1.0), 32767);
        assert_eq!(pointer_value(0.5), 0);
        assert_eq!(pointer_value(-3.0), -32767);
        assert_eq!(pointer_value(7.0), 32767);
    }

    // The Mini V2 shows the stacked screens as 720x1080 in the middle of 1240x1080.
    const PIC: Picture = Picture {
        x: 260.0,
        y: 0.0,
        w: 720.0,
        h: 1080.0,
        frame_w: 256,
        frame_h: 384,
        turns: 0,
    };
    const SCREEN: (u32, u32) = (1240, 1080);
    const STACKED: [&str; 1] = ["top-bottom"];

    #[test]
    fn a_touch_on_the_bottom_screen_becomes_a_pointer_over_the_whole_frame() {
        // The middle of the bottom screen: halfway across, three quarters down the frame.
        let p = pointer_for("nds", ((260.0 + 360.0) / 1240.0, 810.0 / 1080.0), SCREEN, Some(PIC), &STACKED).unwrap();
        assert!(p.0.abs() <= 1, "{p:?}");
        assert!((p.1 as i32 - 16383).abs() <= 2, "{p:?}");
    }

    #[test]
    fn the_corners_of_the_bottom_screen_are_the_corners_of_its_part_of_the_frame() {
        let top_left = pointer_for("nds", (261.0 / 1240.0, 541.0 / 1080.0), SCREEN, Some(PIC), &STACKED).unwrap();
        assert!(top_left.0 < -32000 && top_left.1.abs() < 200, "{top_left:?}");
        let bottom_right = pointer_for("nds", (979.0 / 1240.0, 1079.0 / 1080.0), SCREEN, Some(PIC), &STACKED).unwrap();
        assert!(bottom_right.0 > 32000 && bottom_right.1 > 32000, "{bottom_right:?}");
    }

    #[test]
    fn a_touch_on_the_top_screen_or_beside_the_picture_does_nothing() {
        assert_eq!(
            pointer_for("nds", (620.0 / 1240.0, 300.0 / 1080.0), SCREEN, Some(PIC), &STACKED),
            None,
            "top screen"
        );
        assert_eq!(
            pointer_for("nds", (100.0 / 1240.0, 800.0 / 1080.0), SCREEN, Some(PIC), &STACKED),
            None,
            "left of the picture"
        );
        assert_eq!(
            pointer_for("nds", (1100.0 / 1240.0, 800.0 / 1080.0), SCREEN, Some(PIC), &STACKED),
            None,
            "right of it"
        );
    }

    #[test]
    fn the_top_screen_alone_takes_no_touch() {
        let pic = Picture {
            x: 100.0,
            y: 0.0,
            w: 1040.0,
            h: 780.0,
            frame_w: 256,
            frame_h: 192,
            turns: 0,
        };
        assert_eq!(pointer_for("nds", (0.5, 0.3), SCREEN, Some(pic), &["top"]), None);
    }

    #[test]
    fn side_by_side_touch_goes_to_the_right_half() {
        let pic = Picture {
            x: 0.0,
            y: 307.0,
            w: 1240.0,
            h: 465.0,
            frame_w: 512,
            frame_h: 192,
            turns: 0,
        };
        let layouts = ["left-right"];
        assert!(
            pointer_for("nds", (0.25, 0.5), SCREEN, Some(pic), &layouts).is_none(),
            "the top screen is on the left"
        );
        let p = pointer_for("nds", (0.75, 0.5), SCREEN, Some(pic), &layouts).unwrap();
        assert!(p.0 > 12000 && p.0 < 20000 && p.1.abs() < 100, "{p:?}");
    }

    #[test]
    fn an_unknown_layout_passes_every_touch_on_the_picture_to_the_core() {
        let layouts = ["hybrid-top"];
        assert!(pointer_for("nds", (0.5, 0.2), SCREEN, Some(PIC), &layouts).is_some());
        assert!(pointer_for("nds", (0.05, 0.2), SCREEN, Some(PIC), &layouts).is_none(), "still not beside it");
    }

    #[test]
    fn other_systems_get_the_whole_picture() {
        let pic = Picture {
            x: 40.0,
            y: 36.0,
            w: 1160.0,
            h: 1008.0,
            frame_w: 160,
            frame_h: 144,
            turns: 0,
        };
        let p = pointer_for("gbc", (620.0 / 1240.0, 540.0 / 1080.0), SCREEN, Some(pic), &[]).unwrap();
        assert!(p.0.abs() < 200 && p.1.abs() < 200, "{p:?}");
        assert_eq!(pointer_for("gbc", (0.01, 0.5), SCREEN, Some(pic), &[]), None);
    }

    #[test]
    fn a_turned_picture_takes_touch_where_its_pixels_are() {
        // A Game Boy picture turned right (one quarter counter-clockwise on the panel): the rectangle's top-left
        // corner shows the frame's top-right one, its bottom-left the top-left.
        let pic = Picture { x: 100.0, y: 0.0, w: 1008.0, h: 1080.0, frame_w: 160, frame_h: 144, turns: 1 };
        let at = |x: f32, y: f32| pointer_for("gbc", (x / 1240.0, y / 1080.0), SCREEN, Some(pic), &[]).unwrap();
        let p = at(101.0, 1.0);
        assert!(p.0 > 32000 && p.1 < -32000, "top right of the frame: {p:?}");
        let p = at(101.0, 1079.0);
        assert!(p.0 < -32000 && p.1 < -32000, "top left: {p:?}");
        // The DS turned left, like a book: the bottom screen is on the left half of the picture.
        let ds = Picture { x: 80.0, y: 0.0, w: 1080.0, h: 720.0, frame_w: 256, frame_h: 384, turns: 3 };
        let touch = |x: f32| pointer_for("nds", (x / 1240.0, 360.0 / 1080.0), SCREEN, Some(ds), &STACKED);
        assert!(touch(1000.0).is_none(), "the right half is the top screen");
        let p = touch(350.0).unwrap();
        assert!(p.0.abs() < 200 && p.1 > 16000, "{p:?}");
    }

    #[test]
    fn without_a_picture_there_is_no_pointer() {
        assert_eq!(pointer_for("nds", (0.5, 0.8), SCREEN, None, &STACKED), None);
    }

    // ---- the finger for the game -------------------------------------------------------------------------

    fn ev(phase: Phase, x: f32, y: f32) -> TouchEvent {
        TouchEvent { phase, x, y }
    }

    #[test]
    fn the_game_sees_the_finger_where_it_is() {
        let s = Shared::default();
        assert_eq!(s.frame(), None);
        s.feed(&ev(Phase::Down, 0.2, 0.3));
        assert_eq!(s.frame(), Some((0.2, 0.3)));
        s.feed(&ev(Phase::Move, 0.25, 0.35));
        assert_eq!(s.frame(), Some((0.25, 0.35)));
        s.feed(&ev(Phase::Up, 0.25, 0.35));
        assert_eq!(s.frame(), None);
    }

    #[test]
    fn a_tap_shorter_than_a_frame_still_shows_for_one() {
        let s = Shared::default();
        s.feed(&ev(Phase::Down, 0.2, 0.3));
        s.feed(&ev(Phase::Up, 0.21, 0.3));
        assert_eq!(s.frame(), Some((0.2, 0.3)), "the frame sees where it landed");
        assert_eq!(s.frame(), None, "and only one");
    }

    #[test]
    fn a_tap_made_during_the_pause_is_not_the_games() {
        let s = Shared::default();
        s.feed(&ev(Phase::Down, 0.2, 0.3));
        s.feed(&ev(Phase::Up, 0.2, 0.3));
        s.after_pause();
        assert_eq!(s.frame(), None);
    }

    #[test]
    fn a_finger_still_down_after_the_pause_waits_until_it_lifts() {
        let s = Shared::default();
        s.feed(&ev(Phase::Down, 0.2, 0.3));
        s.after_pause();
        assert_eq!(s.frame(), None);
        s.feed(&ev(Phase::Move, 0.3, 0.3));
        assert_eq!(s.frame(), None);
        s.feed(&ev(Phase::Up, 0.3, 0.3));
        s.feed(&ev(Phase::Down, 0.5, 0.5));
        assert_eq!(s.frame(), Some((0.5, 0.5)));
    }

    // ---- the touch screen kept read (the incident of 2026-10-26: touch was dead for a while after a boot) ------

    use std::collections::VecDeque;

    const DOWN: [RawEvent; 5] = [RawEvent::Slot(0), RawEvent::TrackingId(1), RawEvent::X(100), RawEvent::Y(200), RawEvent::Sync];
    const UP: [RawEvent; 2] = [RawEvent::TrackingId(-1), RawEvent::Sync];

    /// One look of the fake bus: nothing there, or a device that delivers `batches` and then fails after `lived`.
    enum Look {
        Missing,
        Device(&'static str, Vec<Vec<RawEvent>>, Duration),
    }

    fn device(name: &'static str, batches: Vec<Vec<RawEvent>>, lived_s: u64) -> Look {
        Look::Device(name, batches, Duration::from_secs(lived_s))
    }

    struct Script(VecDeque<Vec<RawEvent>>);

    impl Source for Script {
        fn next_batch(&mut self) -> Option<Vec<RawEvent>> {
            self.0.pop_front()
        }
    }

    /// A touch screen that is not there, appears, fails, goes away and comes back, as scripted. The supervision ends
    /// when the script is used up.
    struct FakeBus {
        looks: VecDeque<Look>,
        lived: Duration,
        waits: Vec<Duration>,
        opens: usize,
    }

    impl FakeBus {
        fn new(looks: Vec<Look>) -> FakeBus {
            FakeBus { looks: looks.into(), lived: Duration::ZERO, waits: Vec::new(), opens: 0 }
        }

        fn run(mut self, send: &dyn Fn(TouchEvent) -> bool) -> (Vec<String>, Vec<Duration>, usize) {
            let mut log = Vec::new();
            supervise_with(&mut self, send, &mut |l| log.push(l));
            (log, self.waits, self.opens)
        }
    }

    impl Bus for FakeBus {
        type Dev = Vec<Vec<RawEvent>>;

        fn open(&mut self) -> Option<(String, Self::Dev)> {
            self.opens += 1;
            match self.looks.pop_front()? {
                Look::Missing => None,
                Look::Device(name, batches, lived) => {
                    self.lived = lived;
                    Some((name.to_string(), batches))
                }
            }
        }

        fn read(&mut self, dev: Self::Dev, send: &dyn Fn(TouchEvent) -> bool) -> Read {
            let end = read_stream(&mut Script(dev.into()), PANEL, Rotation::Deg0, false, send);
            Read { end, lived: self.lived }
        }

        fn wait(&mut self, timeout: Duration) -> bool {
            self.waits.push(timeout);
            !self.looks.is_empty()
        }
    }

    fn collect(run: impl FnOnce(&dyn Fn(TouchEvent) -> bool) -> (Vec<String>, Vec<Duration>, usize)) -> (Vec<Phase>, Vec<String>, Vec<Duration>, usize) {
        let got = std::cell::RefCell::new(Vec::new());
        let (log, waits, opens) = run(&|t| {
            got.borrow_mut().push(t.phase);
            true
        });
        (got.into_inner(), log, waits, opens)
    }

    #[test]
    fn a_touch_screen_that_appears_late_is_found_and_read() {
        // Sol's finding: it was opened once at the start and never looked for again.
        let bus = FakeBus::new(vec![Look::Missing, Look::Missing, Look::Missing, device("ft5x06", vec![DOWN.to_vec(), UP.to_vec()], 60)]);
        let (got, log, waits, opens) = collect(|send| bus.run(send));
        assert_eq!(got, [Phase::Down, Phase::Up]);
        assert_eq!(log.len(), 3, "{log:?}");
        assert!(log[0].starts_with("touch: no touch screen yet"), "said once, not at every look: {log:?}");
        assert_eq!(log[1], "touch: ft5x06");
        assert!(log[2].starts_with("touch: lost ft5x06"), "{log:?}");
        assert_eq!(opens, 4);
        assert_eq!(&waits[..3], [Duration::from_millis(250), Duration::from_millis(500), Duration::from_millis(1000)]);
    }

    #[test]
    fn a_read_error_mid_stream_opens_the_screen_again_with_a_clean_parser() {
        // A finger was down when the reading failed (no lift ever came). The reopened screen's first touch is a touch:
        // a parser carried over would wait for the old finger to lift.
        let bus = FakeBus::new(vec![device("ft5x06", vec![DOWN.to_vec()], 60), device("ft5x06", vec![DOWN.to_vec(), UP.to_vec()], 60)]);
        let (got, log, waits, _) = collect(|send| bus.run(send));
        assert_eq!(got, [Phase::Down, Phase::Down, Phase::Up]);
        assert_eq!(log[0], "touch: ft5x06");
        assert!(log[1].starts_with("touch: lost ft5x06 after 60 s"), "{log:?}");
        assert_eq!(log[2], "touch: ft5x06 (again)");
        assert_eq!(waits[0], RETRY_FIRST, "a screen that worked a while is looked for again soon");
    }

    #[test]
    fn a_screen_that_goes_away_and_comes_back_is_logged_once_per_change() {
        let bus = FakeBus::new(vec![
            device("ft5x06", vec![DOWN.to_vec(), UP.to_vec()], 120),
            Look::Missing,
            Look::Missing,
            Look::Missing,
            device("ft5x06", vec![DOWN.to_vec(), UP.to_vec()], 120),
        ]);
        let (got, log, waits, _) = collect(|send| bus.run(send));
        assert_eq!(got.len(), 4);
        assert_eq!(log.iter().filter(|l| l.contains("lost")).count(), 2, "{log:?}");
        assert_eq!(log.iter().filter(|l| l.contains("no touch screen yet")).count(), 0, "the loss said it already: {log:?}");
        assert_eq!(log.iter().filter(|l| l.ends_with("(again)")).count(), 1, "{log:?}");
        assert_eq!(&waits[..4], [RETRY_FIRST, RETRY_FIRST, RETRY_FIRST * 2, RETRY_FIRST * 4]);
    }

    #[test]
    fn a_screen_that_opens_and_fails_at_once_is_not_opened_in_a_loop() {
        let looks = (0..8).map(|_| device("ft5x06", vec![], 0)).collect();
        let (_, log, waits, opens) = collect(|send| FakeBus::new(looks).run(send));
        assert_eq!(opens, 8);
        assert!(waits.windows(2).all(|w| w[1] >= w[0]), "{waits:?}");
        assert!(waits[3] > waits[0] * 4, "the waits keep growing while it keeps failing: {waits:?}");
        assert_eq!(log.iter().filter(|l| l.contains("lost")).count(), 8);
    }

    #[test]
    fn looking_for_a_screen_that_is_not_there_backs_off_to_a_cap_and_logs_once() {
        let looks = (0..60).map(|_| Look::Missing).collect();
        let (got, log, waits, opens) = collect(|send| FakeBus::new(looks).run(send));
        assert!(got.is_empty());
        assert_eq!(log.len(), 1, "{log:?}");
        assert_eq!(waits.len(), opens, "every look is followed by a wait, never two looks in a row");
        assert!(waits.iter().all(|w| *w >= RETRY_FIRST && *w <= RETRY_MAX), "{waits:?}");
        assert!(waits.windows(2).all(|w| w[1] >= w[0]));
        assert_eq!(*waits.last().unwrap(), RETRY_MAX);
        // An hour without a screen: a few dozen looks, not thousands.
        let mut t = Duration::ZERO;
        let mut looks = 0;
        for misses in 0.. {
            t += retry_after(misses);
            looks += 1;
            if t >= Duration::from_secs(3600) {
                break;
            }
        }
        assert!(looks <= 140, "{looks} looks in an hour");
    }

    #[test]
    fn the_retry_waits_start_short_double_and_stop_at_the_cap() {
        assert_eq!(retry_after(0), Duration::from_millis(250));
        assert_eq!(retry_after(1), Duration::from_millis(500));
        assert_eq!(retry_after(2), Duration::from_secs(1));
        assert_eq!(retry_after(7), Duration::from_secs(30), "250 ms x 128 is past the cap");
        assert_eq!(retry_after(u32::MAX), RETRY_MAX, "no overflow");
    }

    #[test]
    fn a_reader_nobody_listens_to_ends_the_supervision() {
        let bus = FakeBus::new(vec![device("ft5x06", vec![DOWN.to_vec()], 60), device("ft5x06", vec![DOWN.to_vec()], 60)]);
        let (log, waits, opens) = bus.run(&|_| false);
        assert_eq!((opens, waits.len()), (1, 0), "no new look, no wait");
        assert_eq!(log, ["touch: ft5x06"]);
    }

    #[test]
    fn a_touch_ignored_is_logged_once_per_change() {
        let mut log = IgnoreLog::default();
        let first = log.ignored(Ignored::Asleep).expect("the first one says why");
        assert!(first.starts_with("touch: ignored, ") && first.contains("asleep"), "{first}");
        for _ in 0..1000 {
            assert_eq!(log.ignored(Ignored::Asleep), None, "the same spell: no more lines");
        }
        assert!(log.ignored(Ignored::Game).is_some(), "another reason is a change");
        assert_eq!(log.ignored(Ignored::Game), None);
        log.accepted();
        assert!(log.ignored(Ignored::Game).is_some(), "a touch that worked ended the spell: the next drop says so again");
    }

    #[test]
    fn every_reason_has_its_own_words() {
        let all = [Ignored::TvOutput, Ignored::Asleep, Ignored::Game, Ignored::Launching, Ignored::PauseMenu, Ignored::Setup, Ignored::SystemPanel, Ignored::NothingToTouch];
        let texts: std::collections::HashSet<_> = all.iter().map(|i| i.text()).collect();
        assert_eq!(texts.len(), all.len());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_look_that_found_nothing_is_not_repeated_while_the_nodes_are_the_same() {
        use super::linux::same_nodes;
        let t0 = Instant::now();
        let nodes = vec![("/dev/input/event0".to_string(), 1u64)];
        let missed = Some((nodes.clone(), t0));
        assert!(!same_nodes(None, &nodes, t0), "the first look is always made");
        assert!(same_nodes(missed.as_ref(), &nodes, t0 + Duration::from_secs(5)), "same nodes: nothing to open");
        let mut more = nodes.clone();
        more.push(("/dev/input/event1".to_string(), 2));
        assert!(!same_nodes(missed.as_ref(), &more, t0 + Duration::from_secs(5)), "a new node: look");
        let recreated = vec![("/dev/input/event0".to_string(), 9u64)];
        assert!(!same_nodes(missed.as_ref(), &recreated, t0 + Duration::from_secs(5)), "a node made again: look");
        assert!(!same_nodes(missed.as_ref(), &nodes, t0 + RETRY_MAX), "a driver that was slow to get ready: look anyway");
    }
}
