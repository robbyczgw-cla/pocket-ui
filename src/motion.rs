//! Motion: the small animations that make the menus feel quicker (docs/DESIGN.md, Motion).
//!
//! Rules every animation here keeps:
//! * **Input first.** A button changes the state at once; the animation is only a picture of the way there and
//!   is evaluated from the state on every frame. Pressing again in the middle retargets from wherever the picture
//!   is, it never queues ([`Glide::set`]).
//! * **Ease-out, short.** A [`Glide`] approaches its target exponentially (an ease-out with the most movement in
//!   the first frame; a critically damped spring released with the matching speed moves the same way), settling
//!   in 100-200 ms. Values are evaluated for the moment the frame reaches the screen, one refresh after it is
//!   painted ([`LEAD`]), so the first frame after a press already shows a good part of the move.
//! * **Nothing moves, nothing is drawn.** Each animation knows when it is done and reports it to [`busy`], which
//!   the app's redraw scheduler asks; once everything has settled, no frame is drawn until the next input.
//! * **The player decides.** Settings > Look > Motion: Full, Reduced (fades only, nothing moves) or Off
//!   ([`set_level`]). `POCKETUI_MOTION_SCALE=<n>` slows everything down n times, for recording frame sequences.

use std::cell::Cell;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use crate::gfx::{mix, Canvas};
pub use crate::settings::Motion;

/// Animations are evaluated for when their frame is on screen: painted now, shown about one 60 Hz refresh later.
pub const LEAD: Duration = Duration::from_micros(16_667);
/// Time constant of the standard glide: 63 % of the way after 28 ms, settled (to half a pixel over a row) in about
/// 150 ms, a five-row jump in about 200 ms.
pub const TAU: Duration = Duration::from_millis(28);
/// Page push and pop, toasts, the hint bar: how long their fade and slide take.
pub const PAGE: Duration = Duration::from_millis(150);
/// Rows of a list that has just opened come in one after another, this far apart ...
pub const STAGGER: Duration = Duration::from_millis(16);
/// ... for at most this many rows (the rest come with the last).
pub const STAGGER_CAP: u32 = 4;

thread_local! {
    /// The level in use. Per thread: everything that animates is painted on the UI thread, and tests running in
    /// parallel each get their own.
    static LEVEL: Cell<Motion> = const { Cell::new(Motion::Full) };
    /// `POCKETUI_MOTION_SCALE`, read on first use.
    static SCALE: Cell<Option<f32>> = const { Cell::new(None) };
    /// Settings > Look & feel > Animations: Cozy (`set_cozy`).
    static COZY: Cell<bool> = const { Cell::new(false) };
}

/// How much slower every timed animation runs in Cozy than in Quick (the timings as designed).
pub const COZY_SLOWER: f32 = 1.4;

/// Cozy: page slides, the box's turn, the cabinet's cases, fades and the toasts take `COZY_SLOWER` times as long.
pub fn set_cozy(on: bool) {
    COZY.with(|c| c.set(on));
}

pub fn cozy() -> bool {
    COZY.with(|c| c.get())
}

/// What an animation does: moves things (off in Reduced) or only fades them.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Move,
    Fade,
}

pub fn set_level(m: Motion) {
    LEVEL.with(|l| l.set(m));
}

pub fn level() -> Motion {
    LEVEL.with(|l| l.get())
}

/// Whether animations of this kind run at the current level.
pub fn allowed(kind: Kind) -> bool {
    match level() {
        Motion::Full => true,
        Motion::Reduced => kind == Kind::Fade,
        Motion::Off => false,
    }
}

/// `POCKETUI_MOTION_SCALE`: 1 normally, 10 makes every animation ten times slower (frame sequences on the
/// virtual device, where a screenshot takes longer than a frame). Cozy multiplies it by `COZY_SLOWER`.
pub fn time_scale() -> f32 {
    let cozy = if self::cozy() { COZY_SLOWER } else { 1.0 };
    SCALE.with(|s| {
        if let Some(v) = s.get() {
            return v * cozy;
        }
        let v = std::env::var("POCKETUI_MOTION_SCALE").ok().and_then(|v| v.parse::<f32>().ok()).filter(|v| (0.1..=100.0).contains(v)).unwrap_or(1.0);
        s.set(Some(v));
        v * cozy
    })
}

/// How long `d` lasts at the current level and time scale; zero when the level turns this kind of animation off.
/// Existing timed animations (the shelf's pull-out, the cabinet, the switcher) go through this too.
pub fn span(kind: Kind, d: Duration) -> Duration {
    if !allowed(kind) {
        return Duration::ZERO;
    }
    scaled(d)
}

/// Seconds since `at` as the animation sees it: plus the lead, divided by the time scale.
fn seconds(at: Instant, now: Instant) -> f32 {
    (now.saturating_duration_since(at) + LEAD).as_secs_f32() / time_scale()
}

pub fn ease_out(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t).powi(3)
}

/// 0-1 through an ease-out of length `d` that started at `at`, evaluated for the frame painted at `now`.
/// 1 at once when the level has this kind off.
pub fn progress(kind: Kind, at: Instant, now: Instant, d: Duration) -> f32 {
    if !allowed(kind) || d.is_zero() {
        return 1.0;
    }
    ease_out(seconds(at, now) / d.as_secs_f32())
}

// ---- the redraw signal ------------------------------------------------------------------------------------

thread_local! {
    /// The last moment any animation started on this thread is still moving.
    static UNTIL: Cell<Option<Instant>> = const { Cell::new(None) };
}

/// After an animation has arrived, frames are asked for this much longer, so that one is painted at or after the
/// moment of arrival: the last picture is exactly at rest (half a pixel short would stay on screen until the next
/// input, since nothing is drawn while nothing moves).
pub const SETTLE: Duration = Duration::from_micros(17_000);

/// Something is moving until `until`: the app keeps drawing frames until then (and one more, [`SETTLE`]).
pub fn keep_alive(until: Instant) {
    let until = until + SETTLE;
    UNTIL.with(|u| {
        if u.get().is_none_or(|t| t < until) {
            u.set(Some(until));
        }
    });
}

/// Whether any animation is still moving at `now`. False once everything has settled: the app's loop then
/// sleeps until the next input.
pub fn busy(now: Instant) -> bool {
    UNTIL.with(|u| u.get().is_some_and(|t| now < t))
}

/// When the last animation ends (`about_to_wait` may sleep until then).
pub fn busy_until() -> Option<Instant> {
    UNTIL.with(|u| u.get())
}

/// Forgets every running animation (a screen that is gone; tests).
pub fn settle_all() {
    UNTIL.with(|u| u.set(None));
}

/// End of an animation that started at `at` and lasts `d` (scaled), as the wall clock sees it.
pub fn end_of(at: Instant, d: Duration) -> Instant {
    (at + scaled(d)).checked_sub(LEAD).unwrap_or(at)
}

/// `d` at the time scale.
fn scaled(d: Duration) -> Duration {
    let k = time_scale();
    if k == 1.0 {
        d
    } else {
        d.mul_f32(k)
    }
}

// ---- Glide ------------------------------------------------------------------------------------------------

/// A value that glides to where it is sent: an exponential ease-out, retargeted from wherever it is.
/// Starts unset: the first target is taken at once (a screen shows up at rest, snapshots too).
#[derive(Clone, Copy, Debug)]
pub struct Glide {
    kind: Kind,
    /// Distance under which it counts as arrived, in the value's own unit (half a pixel, a hundredth of a row).
    eps: f32,
    tau: f32,
    from: f32,
    to: f32,
    at: Instant,
    /// When it is within `eps` of `to` (wall clock); None when it was put there without gliding.
    done: Option<Instant>,
    set: bool,
}

impl Glide {
    /// A glide of `kind`, arriving to within `eps`.
    pub fn new(kind: Kind, eps: f32) -> Glide {
        let now = Instant::now();
        Glide {
            kind,
            eps: eps.max(1e-4),
            tau: TAU.as_secs_f32(),
            from: 0.0,
            to: 0.0,
            at: now,
            done: None,
            set: false,
        }
    }

    /// Pixels: arrives to within half a pixel.
    pub fn px() -> Glide {
        Glide::new(Kind::Move, 0.5)
    }

    /// Rows or list positions: arrives to within 1/200 of one.
    pub fn rows() -> Glide {
        Glide::new(Kind::Move, 0.005)
    }

    /// A 0-1 amount that only fades (Reduced keeps it).
    pub fn fade() -> Glide {
        Glide::new(Kind::Fade, 0.004)
    }

    pub fn with_tau(mut self, tau: Duration) -> Glide {
        self.tau = tau.as_secs_f32().max(0.001);
        self
    }

    /// Where it is in the frame painted at `now`.
    pub fn value(&self, now: Instant) -> f32 {
        if !self.set || self.done.is_none_or(|d| now >= d) {
            return self.to;
        }
        self.to + (self.from - self.to) * (-seconds(self.at, now) / self.tau).exp()
    }

    pub fn target(&self) -> f32 {
        self.to
    }

    /// Sends it to `to`, from wherever it is at `now`. The same target again changes nothing; the first target,
    /// or any while the level has this kind off, is taken at once.
    pub fn set(&mut self, to: f32, now: Instant) {
        if !self.set || !allowed(self.kind) {
            self.jump(to);
            return;
        }
        if to == self.to {
            return;
        }
        let from = self.value(now);
        let dist = (from - to).abs();
        self.from = from;
        self.to = to;
        self.at = now;
        if dist <= self.eps {
            self.done = None;
            return;
        }
        // e^(-t/tau) * dist = eps
        let t = self.tau * (dist / self.eps).ln();
        let done = end_of(now, Duration::from_secs_f32(t.max(0.0)));
        self.done = Some(done);
        keep_alive(done);
    }

    /// There at once, without gliding.
    pub fn jump(&mut self, to: f32) {
        (self.from, self.to, self.set) = (to, to, true);
        self.done = None;
    }

    /// Frames are still needed: it is on its way, or has just arrived and its last frame is due ([`SETTLE`]).
    pub fn moving(&self, now: Instant) -> bool {
        self.set && self.done.is_some_and(|d| now < d + SETTLE)
    }

    /// Whether it has been given a target yet.
    pub fn is_set(&self) -> bool {
        self.set
    }
}

// ---- page transitions -------------------------------------------------------------------------------------

/// How a page comes in.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Enter {
    /// Deeper: in from the right, its rows one after another.
    Push,
    /// Back: in from the left, all at once.
    Pop,
    /// Out of the selected row at `y` (the game card opens from its label): up from there.
    Rise { from_y: i32 },
    /// Only fades in (the switcher, a page beside another).
    Fade,
}

/// How far a pushed or popped page slides.
pub const SLIDE_PX: f32 = 28.0;

/// A page coming in: a short slide and a fade from the page colour, painted over the finished frame
/// (no second picture is kept: the old page is gone with the press, the new one is there in the first frame).
/// A pushed list's rows come in one after another: the page tells where they are ([`Transition::apply`]).
#[derive(Clone, Copy, Debug)]
pub struct Transition {
    pub enter: Enter,
    pub at: Instant,
    /// Top of the page's content: the game card rises from the selected label towards it.
    pub top: i32,
}

impl Transition {
    pub fn new(enter: Enter, at: Instant, top: i32) -> Option<Transition> {
        if !allowed(Kind::Fade) {
            return None;
        }
        let t = Transition { enter, at, top };
        keep_alive(t.ends());
        Some(t)
    }

    fn staggers(&self) -> bool {
        self.enter == Enter::Push
    }

    /// Wall-clock end, the last row's delay included.
    pub fn ends(&self) -> Instant {
        let delay = if self.staggers() { STAGGER * STAGGER_CAP } else { Duration::ZERO };
        end_of(self.at, PAGE + delay)
    }

    pub fn active(&self, now: Instant) -> bool {
        now < self.ends()
    }

    /// 0-1 for row `k` of a list (0 = the first row and everything that is not a row).
    fn progress_of(&self, k: u32, now: Instant) -> f32 {
        let delay = if self.staggers() { STAGGER * k.min(STAGGER_CAP) } else { Duration::ZERO };
        let at = self.at + scaled(delay);
        if now < at.checked_sub(LEAD).unwrap_or(at) {
            return 0.0;
        }
        progress(Kind::Fade, at, now, PAGE)
    }

    /// Sideways offset and fade (0-255 towards the page colour) at progress `p`.
    fn slide_fade(&self, p: f32) -> (i32, u32) {
        let dx = match self.enter {
            _ if !allowed(Kind::Move) => 0,
            Enter::Push => ((1.0 - p) * SLIDE_PX).round() as i32,
            Enter::Pop => -((1.0 - p) * SLIDE_PX).round() as i32,
            Enter::Rise { .. } | Enter::Fade => 0,
        };
        (dx, ((1.0 - p) * 255.0).round() as u32)
    }

    /// Paints the transition over the finished frame `c` at `now`, fading towards the page colour `bg`.
    /// `rows`: the boxes (x, y, w, h) of a list's rows from the top, which come in one after another; everything
    /// else comes with the first. Returns false once it is over (nothing was changed then).
    pub fn apply(&self, c: &mut Canvas, now: Instant, bg: u32, rows: &[(i32, i32, i32, i32)]) -> bool {
        if !self.active(now) {
            return false;
        }
        let w = c.w as usize;
        let looks: Vec<(i32, u32)> = (0..=STAGGER_CAP.min(rows.len() as u32)).map(|k| self.slide_fade(self.progress_of(k, now))).collect();
        let mut r = 0;
        for y in 0..c.h {
            while r < rows.len() && rows[r].1 + rows[r].3 <= y {
                r += 1;
            }
            let line = &mut c.buf[y as usize * w..(y as usize + 1) * w];
            match rows.get(r).filter(|b| b.1 <= y) {
                Some(&(x, _, bw, _)) => {
                    let (x0, x1) = (x.clamp(0, c.w) as usize, (x + bw).clamp(0, c.w) as usize);
                    let row = looks[(r as u32).min(STAGGER_CAP) as usize];
                    let (left, rest) = line.split_at_mut(x0);
                    let (mid, right) = rest.split_at_mut(x1 - x0);
                    for (seg, (dx, fade)) in [(left, looks[0]), (mid, row), (right, looks[0])] {
                        slide_fade_row(seg, dx, fade, bg);
                    }
                }
                None => slide_fade_row(line, looks[0].0, looks[0].1, bg),
            }
        }
        if let Enter::Rise { from_y } = self.enter {
            if allowed(Kind::Move) {
                let p = self.progress_of(0, now);
                let dy = ((1.0 - p) * (from_y - self.top).clamp(-160, 160) as f32 * 0.35).round() as i32;
                shift_rows(c, 0, c.h, dy, bg);
            }
        }
        true
    }
}

/// One stretch of a line: shifted `dx` and mixed `fade` (0-255) towards `bg`. The mix works on two channels at
/// once and has no branch, so the compiler can vectorise it: a whole frame costs about a millisecond on the device.
fn slide_fade_row(line: &mut [u32], dx: i32, fade: u32, bg: u32) {
    // A stretch of nothing but the page colour stays as it is, shifted or mixed towards itself (`(bg * 256) >> 8`),
    // and most of a page is that: only read, never written (on the Mini V2 the frame's writes are what costs).
    let page = bg & 0xff_ffff;
    if line.iter().fold(0, |any, &p| any | (p ^ page)) == 0 {
        return;
    }
    shift_row(line, dx);
    if fade == 0 {
        return;
    }
    let f = fade + (fade >> 7); // 0-255 to 0-256
    let keep = 256 - f;
    let (bg_rb, bg_g) = ((bg & 0xff00ff) * f, (bg & 0x00ff00) * f);
    for px in line.iter_mut() {
        let p = *px;
        let rb = (((p & 0xff00ff) * keep + bg_rb) >> 8) & 0xff00ff;
        let g = (((p & 0x00ff00) * keep + bg_g) >> 8) & 0x00ff00;
        *px = rb | g;
    }
}

/// Moves a row of pixels `dx` to the right (left when negative), repeating the edge pixel into the gap.
fn shift_row(line: &mut [u32], dx: i32) {
    let n = line.len();
    let d = dx.unsigned_abs() as usize;
    if d == 0 || d >= n {
        return;
    }
    if dx > 0 {
        let edge = line[0];
        line.copy_within(0..n - d, d);
        line[..d].fill(edge);
    } else {
        let edge = line[n - 1];
        line.copy_within(d..n, 0);
        line[n - d..].fill(edge);
    }
}

/// Moves rows `y0..y1` down by `dy` (up when negative) inside that band, filling the gap with `bg`.
fn shift_rows(c: &mut Canvas, y0: i32, y1: i32, dy: i32, bg: u32) {
    let w = c.w as usize;
    let (y0, y1) = (y0.max(0) as usize, y1.min(c.h) as usize);
    let d = dy.unsigned_abs() as usize;
    if y1 <= y0 || d == 0 {
        return;
    }
    if d >= y1 - y0 {
        c.buf[y0 * w..y1 * w].fill(bg);
        return;
    }
    if dy > 0 {
        c.buf.copy_within(y0 * w..(y1 - d) * w, (y0 + d) * w);
        c.buf[y0 * w..(y0 + d) * w].fill(bg);
    } else {
        c.buf.copy_within((y0 + d) * w..y1 * w, y0 * w);
        c.buf[(y1 - d) * w..y1 * w].fill(bg);
    }
}

// ---- layers -----------------------------------------------------------------------------------------------

/// Paints with `paint` into the rectangle (x, y, w, h) at `alpha` (0-255) over what is there: the area is kept,
/// painted, and the two are mixed. For small things only (a toast, the hint bar): the cost is the area. What
/// `paint` draws outside the rectangle is not mixed.
pub fn layer(c: &mut Canvas, x: i32, y: i32, w: i32, h: i32, alpha: u8, paint: impl FnOnce(&mut Canvas)) {
    if alpha == 255 {
        paint(c);
        return;
    }
    let (x0, y0, x1, y1) = (x.max(0), y.max(0), (x + w).min(c.w), (y + h).min(c.h));
    if x1 <= x0 || y1 <= y0 {
        return;
    }
    // Painted even at alpha 0: what it registers besides pixels (taps) stays.
    let cw = c.w as usize;
    let (x0, x1) = (x0 as usize, x1 as usize);
    let saved: Vec<u32> = (y0..y1).flat_map(|row| c.buf[row as usize * cw + x0..row as usize * cw + x1].iter().copied()).collect();
    paint(c);
    let a = alpha as u32;
    let span = x1 - x0;
    for (k, row) in (y0..y1).enumerate() {
        let line = &mut c.buf[row as usize * cw + x0..row as usize * cw + x1];
        for (px, old) in line.iter_mut().zip(&saved[k * span..(k + 1) * span]) {
            if *px != *old {
                *px = mix(*old, *px, a);
            }
        }
    }
}

/// How far a toast or OSD that appeared at `at` has come in: (offset in pixels from its resting place, alpha).
/// Slides `slide` pixels and fades in Full, only fades in Reduced, is simply there when Off.
pub fn appear(at: Instant, now: Instant, slide: f32) -> (i32, u8) {
    let fade = progress(Kind::Fade, at, now, PAGE);
    let moved = progress(Kind::Move, at, now, PAGE);
    (((1.0 - moved) * slide).round() as i32, (fade * 255.0).round() as u8)
}

/// Paints a toast or an OSD pill that appeared at `at` in the box (x, y, w, h): it comes down `slide` pixels into
/// place and fades in, and, when `life` is given, fades out over the last moment of it. `paint(c, dy)` paints it
/// moved down by `dy`. Nothing waits for it: the pill is readable from its first frame.
pub fn paint_toast(c: &mut Canvas, at: Instant, now: Instant, life: Option<Duration>, area: (i32, i32, i32, i32), slide: f32, paint: impl FnOnce(&mut Canvas, i32)) {
    let (dy, mut alpha) = appear(at, now, slide);
    if let Some(life) = life.filter(|_| allowed(Kind::Fade)) {
        let left = (at + life).saturating_duration_since(now).as_secs_f32() / (PAGE.as_secs_f32() * time_scale());
        alpha = alpha.min((left.clamp(0.0, 1.0) * 255.0) as u8);
    }
    let dy = -dy;
    let (x, y, w, h) = area;
    layer(c, x, y + dy.min(0), w, h + dy.abs(), alpha, |c| paint(c, dy));
}

/// Whether something that appeared at `at` is still coming in (frames needed).
pub fn appearing(at: Instant, now: Instant) -> bool {
    allowed(Kind::Fade) && now < end_of(at, PAGE)
}

/// A frame timer for the log (`POCKETUI_FRAME_LOG=1`): the paint time of the frames an animation took.
#[derive(Default, Debug)]
pub struct FrameStats {
    pub frames: u32,
    pub total: Duration,
    pub worst: Duration,
}

impl FrameStats {
    pub fn add(&mut self, d: Duration) {
        self.frames += 1;
        self.total += d;
        self.worst = self.worst.max(d);
    }

    pub fn average(&self) -> Duration {
        if self.frames == 0 {
            Duration::ZERO
        } else {
            self.total / self.frames
        }
    }
}

/// Whether `POCKETUI_FRAME_LOG` asks for frame times in the log.
pub fn frame_log() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var("POCKETUI_FRAME_LOG").is_ok_and(|v| v != "0"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    /// Each test thread has its own level; each test sets the one it needs (and the normal speed).
    fn at_level(m: Motion) {
        set_level(m);
        SCALE.with(|s| s.set(Some(1.0)));
    }

    #[test]
    fn the_first_target_is_taken_at_once_so_screens_and_snapshots_open_at_rest() {
        at_level(Motion::Full);
        settle_all();
        let mut g = Glide::rows();
        let t0 = Instant::now();
        g.set(7.0, t0);
        assert_eq!(g.value(t0), 7.0);
        assert!(!g.moving(t0));
        assert!(!busy(t0), "nothing to draw");
    }

    #[test]
    fn a_glide_is_an_ease_out_that_shows_most_of_the_move_in_its_first_frames_and_settles_in_200_ms() {
        at_level(Motion::Full);
        let mut g = Glide::px();
        let t0 = Instant::now();
        g.set(0.0, t0);
        g.set(82.0, t0);
        // The frame painted right after the press is shown a refresh later: already 40 % of the way.
        let first = g.value(t0 + ms(1));
        assert!(first > 0.4 * 82.0 && first < 0.6 * 82.0, "{first}");
        // Ease-out: each frame moves less than the one before.
        let mut last = 0.0;
        let mut step = f32::MAX;
        for f in 0..12 {
            let v = g.value(t0 + Duration::from_micros(16_667 * f));
            // (the last step may add the half pixel it snaps into place)
            assert!(v - last <= step + 0.5, "frame {f}: {} after {step}", v - last);
            step = v - last;
            last = v;
        }
        assert!(g.moving(t0 + ms(100)));
        assert_eq!(g.value(t0 + ms(140)), 82.0, "a row settles in about 140 ms, exactly there");
        assert!(g.moving(t0 + ms(140)), "one more frame is asked for, at rest");
        assert!(!g.moving(t0 + ms(160)));
    }

    #[test]
    fn a_long_jump_still_settles_within_about_200_ms() {
        at_level(Motion::Full);
        let mut g = Glide::px();
        let t0 = Instant::now();
        g.set(0.0, t0);
        g.set(410.0, t0);
        assert!(g.moving(t0 + ms(170)));
        assert!(!g.moving(t0 + ms(210)));
    }

    #[test]
    fn pressing_again_mid_glide_retargets_from_where_it_is_and_never_queues() {
        at_level(Motion::Full);
        let mut g = Glide::rows();
        let t0 = Instant::now();
        g.set(0.0, t0);
        g.set(1.0, t0);
        let mid = g.value(t0 + ms(30));
        g.set(2.0, t0 + ms(30));
        // No jump back to 1 and no waiting for the first move to finish.
        let after = g.value(t0 + ms(30));
        assert!(after >= mid - 1e-3, "{after} < {mid}: it went backwards");
        assert!(after < 2.0);
        assert_eq!(g.target(), 2.0);
        // Ends at the last target, about one glide after the last press.
        assert!(!g.moving(t0 + ms(30 + 200)));
        assert_eq!(g.value(t0 + ms(230)), 2.0);
    }

    #[test]
    fn holding_a_direction_keeps_the_picture_close_behind_the_selection() {
        at_level(Motion::Full);
        let mut g = Glide::rows();
        let t0 = Instant::now();
        g.set(0.0, t0);
        // Key repeat every 70 ms for a second.
        for k in 1..=14u64 {
            let now = t0 + ms(70 * k);
            g.set(k as f32, now);
            let lag = k as f32 - g.value(now + ms(69));
            assert!(lag < 0.2, "lags {lag} rows behind after {k} steps");
        }
    }

    #[test]
    fn the_busy_signal_ends_when_the_last_animation_settles() {
        at_level(Motion::Full);
        settle_all();
        let t0 = Instant::now();
        let mut a = Glide::rows();
        a.set(0.0, t0);
        a.set(1.0, t0);
        let mut b = Glide::px();
        b.set(0.0, t0);
        b.set(400.0, t0);
        assert!(busy(t0 + ms(100)));
        assert!(!a.moving(t0 + ms(150)));
        assert!(busy(t0 + ms(150)), "the long one is still moving");
        assert!(busy(t0 + ms(180)), "and its last frame, at rest, is still to come");
        assert!(!busy(t0 + ms(200)), "then no more frames");
        assert!(!a.moving(t0 + ms(260)) && !b.moving(t0 + ms(260)));
    }

    #[test]
    fn off_and_reduced_jump_moves_and_reduced_keeps_fades() {
        settle_all();
        let t0 = Instant::now();
        at_level(Motion::Off);
        let mut m = Glide::px();
        m.set(0.0, t0);
        m.set(100.0, t0);
        assert_eq!(m.value(t0), 100.0);
        let mut f = Glide::fade();
        f.set(0.0, t0);
        f.set(1.0, t0);
        assert_eq!(f.value(t0), 1.0);
        assert!(!busy(t0), "Off draws no extra frames");
        assert!(Transition::new(Enter::Push, t0, 100).is_none());

        at_level(Motion::Reduced);
        let mut m = Glide::px();
        m.set(0.0, t0);
        m.set(100.0, t0);
        assert_eq!(m.value(t0), 100.0, "nothing moves");
        let mut f = Glide::fade();
        f.set(0.0, t0);
        f.set(1.0, t0);
        assert!(f.value(t0) < 1.0, "fades still fade");
        assert_eq!(span(Kind::Move, PAGE), Duration::ZERO);
        assert_eq!(span(Kind::Fade, PAGE), PAGE);
        let (dy, alpha) = appear(t0, t0, 40.0);
        assert_eq!(dy, 0);
        assert!(alpha < 255);
        at_level(Motion::Full);
    }

    #[test]
    fn a_pushed_page_slides_and_fades_in_and_its_rows_come_one_after_another() {
        at_level(Motion::Full);
        let t0 = Instant::now();
        let tr = Transition::new(Enter::Push, t0, 100).unwrap();
        let (w, h) = (64u32, 600u32);
        let mut buf = vec![0xffffffu32; (w * h) as usize];
        // A dark mark at the left of every row, and two list rows.
        for row in 0..h as usize {
            buf[row * w as usize + 4] = 0x000000;
        }
        let rows = [(0, 100, 64, 80), (0, 190, 64, 80), (0, 280, 64, 80), (0, 370, 64, 80), (0, 460, 64, 80)];
        let at = |c: &Canvas, x: usize, y: usize| c.buf[y * w as usize + x];
        let mark = |c: &Canvas, y: usize| (0..w as usize).min_by_key(|x| at(c, *x, y) & 0xff).unwrap();
        let darkest = |c: &Canvas, y: usize| at(c, mark(c, y), y) & 0xff;
        let fresh = buf.clone();
        // The frame painted right after the press: the first row is coming, the fifth has not started.
        let mut c = Canvas::new(&mut buf, w, h);
        assert!(tr.apply(&mut c, t0 + ms(10), 0xffffff, &rows));
        assert!(mark(&c, 50) > 4, "the header slid in from the right");
        assert!(darkest(&c, 120) > 0 && darkest(&c, 120) < 0xff, "the first row is half faded in");
        assert_eq!(darkest(&c, 480), 0xff, "the fifth row comes later");
        // One row is one piece: no seam inside it.
        assert_eq!(mark(&c, 101), mark(&c, 179));
        assert_eq!(darkest(&c, 101), darkest(&c, 179));
        // A moment later the second row is a step behind the first.
        let mut buf = fresh.clone();
        let mut c = Canvas::new(&mut buf, w, h);
        tr.apply(&mut c, t0 + ms(40), 0xffffff, &rows);
        assert!(mark(&c, 120) < mark(&c, 210), "{} {}", mark(&c, 120), mark(&c, 210));
        // Over after 150 ms plus four 16 ms steps: nothing is touched any more.
        let end = t0 + PAGE + STAGGER * STAGGER_CAP;
        assert!(!tr.active(end));
        let mut buf2 = vec![0x123456u32; (w * h) as usize];
        let mut c2 = Canvas::new(&mut buf2, w, h);
        assert!(!tr.apply(&mut c2, end, 0, &rows));
        assert!(buf2.iter().all(|p| *p == 0x123456));
    }

    #[test]
    fn a_popped_page_comes_back_from_the_left_all_at_once() {
        at_level(Motion::Full);
        let t0 = Instant::now();
        let tr = Transition::new(Enter::Pop, t0, 100).unwrap();
        assert!(tr.ends() < t0 + PAGE, "no stagger going back");
        let mut line = vec![1, 2, 3, 4, 5];
        shift_row(&mut line, -2);
        assert_eq!(line, vec![3, 4, 5, 5, 5]);
        shift_row(&mut line, 1);
        assert_eq!(line, vec![3, 3, 4, 5, 5]);
    }

    #[test]
    fn the_fast_fade_matches_the_plain_mix_and_keeps_the_ends() {
        for (p, bg) in [(0xffffffu32, 0u32), (0x123456, 0xf3efe7), (0xece9e1, 0x12161b), (0, 0xffffff)] {
            for fade in [0u32, 1, 64, 128, 200, 254, 255] {
                let mut line = [p];
                slide_fade_row(&mut line, 0, fade, bg);
                let want = mix(p, bg, fade);
                for sh in [0, 8, 16] {
                    let (a, b) = ((line[0] >> sh) & 0xff, (want >> sh) & 0xff);
                    assert!(a.abs_diff(b) <= 1, "{p:06x} -> {bg:06x} at {fade}: {:06x} vs {want:06x}", line[0]);
                }
            }
            let mut line = [p];
            slide_fade_row(&mut line, 0, 255, bg);
            assert_eq!(line[0], bg, "all the way: exactly the page colour");
        }
    }

    /// `slide_fade_row` as it was before stretches of the page colour were skipped: every pixel shifted and mixed.
    fn slide_fade_row_everywhere(line: &mut [u32], dx: i32, fade: u32, bg: u32) {
        shift_row(line, dx);
        if fade == 0 {
            return;
        }
        let f = fade + (fade >> 7);
        let keep = 256 - f;
        let (bg_rb, bg_g) = ((bg & 0xff00ff) * f, (bg & 0x00ff00) * f);
        for px in line.iter_mut() {
            let p = *px;
            *px = ((((p & 0xff00ff) * keep + bg_rb) >> 8) & 0xff00ff) | ((((p & 0x00ff00) * keep + bg_g) >> 8) & 0x00ff00);
        }
    }

    #[test]
    fn stretches_of_the_page_colour_are_skipped_and_everything_else_comes_out_as_before() {
        let bg = 0x12161b;
        // Plain page, a mark in the middle, a mark at either edge, a line that is nothing else than a mark.
        let mut lines: Vec<Vec<u32>> = vec![vec![bg; 64], vec![0x445566; 64]];
        for at in [0, 31, 63] {
            let mut l = vec![bg; 64];
            l[at] = 0xece9e1;
            lines.push(l);
        }
        for line in lines {
            for dx in [-28, -3, 0, 5, 28] {
                for fade in [0u32, 1, 100, 200, 255] {
                    let (mut a, mut b) = (line.clone(), line.clone());
                    slide_fade_row(&mut a, dx, fade, bg);
                    slide_fade_row_everywhere(&mut b, dx, fade, bg);
                    assert_eq!(a, b, "dx {dx}, fade {fade}");
                }
            }
        }
    }

    #[test]
    fn a_layer_at_half_alpha_mixes_and_leaves_the_rest_alone() {
        let mut buf = vec![0u32; 16];
        let mut c = Canvas::new(&mut buf, 4, 4);
        layer(&mut c, 1, 1, 2, 2, 128, |c| c.clear(0xfefefe));
        // Inside: half way; outside the rectangle the paint shows unmixed (layers are for things inside their box).
        assert_eq!(c.buf[5] & 0xff, 0x7f);
        assert_eq!(c.buf[0], 0xfefefe);
    }

    #[test]
    fn toasts_slide_down_into_place_and_are_there_within_a_page_time() {
        at_level(Motion::Full);
        let t0 = Instant::now();
        let (dy0, a0) = appear(t0, t0, 40.0);
        assert!(dy0 > 0 && dy0 < 40 && a0 > 0 && a0 < 255, "{dy0} {a0}");
        assert_eq!(appear(t0, t0 + PAGE, 40.0), (0, 255));
        assert!(appearing(t0, t0 + ms(50)));
        assert!(!appearing(t0, t0 + PAGE));
    }
}

#[cfg(test)]
mod bench {
    use super::*;

    /// `cargo test --release --lib motion::bench -- --ignored --nocapture`: what a page transition adds to a frame.
    #[test]
    #[ignore]
    fn cost_of_a_page_transition_on_a_full_frame() {
        set_level(Motion::Full);
        let (w, h) = (1240u32, 1080u32);
        let mut buf: Vec<u32> = (0..w * h).map(|i| if i % 7 == 0 { 0xece9e1 } else { 0 }).collect();
        let rows: Vec<(i32, i32, i32, i32)> = (0..10).map(|k| (72, 112 + k * 82, 1096, 82)).collect();
        let t0 = Instant::now();
        let tr = Transition::new(Enter::Push, t0, 112).unwrap();
        let n = 200;
        let start = Instant::now();
        for _ in 0..n {
            let mut c = Canvas::new(&mut buf, w, h);
            tr.apply(&mut c, t0 + Duration::from_millis(1), 0, &rows);
        }
        println!("page transition, full 1240x1080 frame: {:.3} ms", start.elapsed().as_secs_f64() * 1000.0 / n as f64);
        let tr = Transition::new(Enter::Fade, t0, 112).unwrap();
        let start = Instant::now();
        for _ in 0..n {
            let mut c = Canvas::new(&mut buf, w, h);
            tr.apply(&mut c, t0 + Duration::from_millis(1), 0, &rows);
        }
        println!("fade only: {:.3} ms", start.elapsed().as_secs_f64() * 1000.0 / n as f64);
    }
}
