//! Putting a frame on screen without ever ending the program (the internal review, K5).
//! The compositor can fail a surface or a buffer for a moment (around suspend, when sway restarts an
//! output); that costs one frame and a new surface, not the process.

use std::num::NonZeroU32;
use std::time::{Duration, Instant};

use crate::damage::{RowHashes, Rows};

/// What a frame is drawn onto: softbuffer's `Surface`, or a fake in tests.
/// Why a frame could not be put on screen: what the window system said. Never ends the program (see `Presenter`).
#[derive(Debug, Clone, PartialEq)]
pub struct FrameError(pub String);

impl std::fmt::Display for FrameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for FrameError {
    fn from(s: &str) -> Self {
        FrameError(s.to_string())
    }
}

impl From<String> for FrameError {
    fn from(s: String) -> Self {
        FrameError(s)
    }
}

pub trait Target {
    fn resize(&mut self, w: NonZeroU32, h: NonZeroU32) -> Result<(), FrameError>;
    /// Hands `paint` the pixel buffer (0RGB, `w * h` as last resized), then presents it with the rows `paint` says it changed
    /// (`damage::Rows::All` for a target that can't tell the compositor less; rows `width` pixels wide).
    fn paint_present(&mut self, width: u32, paint: &mut dyn FnMut(&mut [u32]) -> Rows) -> Result<(), FrameError>;
    /// How long the last frame waited for the system to hand over a buffer (the part of `paint_present` that is
    /// not work); zero for a target that doesn't know.
    fn waited(&self) -> Duration {
        Duration::ZERO
    }
}

/// Where the last frame's time went (`Presenter::stages`), for the Android handheld view's log: the prototype's
/// 12-14 ms "paint" was mostly not painting (docs/ANDROID-HANDHELD-UI-PROTO.md, HANDHELD-PAINT.md).
#[derive(Clone, Copy, Debug, Default)]
pub struct Stages {
    /// The whole call, as the clock on the wall saw it.
    pub total: Duration,
    /// What was painted (the closure).
    pub paint: Duration,
    /// Waiting for the buffer (`Target::waited`).
    pub waited: Duration,
    /// The thread's own CPU time over the whole call (zero where it isn't measured: only Android asks).
    pub cpu: Duration,
    /// Over the whole call, where measured (Android): times the thread gave up the CPU to wait, times it was taken off
    /// it, page faults that needed no disk and ones that did. A frame that took long without CPU time shows here why.
    pub waits: u32,
    pub preempted: u32,
    pub faults: u32,
    pub disk_faults: u32,
    /// Finding the rows the frame changed (`track_damage`), and how many it changed; zero when not tracked.
    pub damage: Duration,
    pub damaged_rows: u32,
}

impl<D: raw_window_handle::HasDisplayHandle, W: raw_window_handle::HasWindowHandle> Target for softbuffer::Surface<D, W> {
    fn resize(&mut self, w: NonZeroU32, h: NonZeroU32) -> Result<(), FrameError> {
        softbuffer::Surface::resize(self, w, h).map_err(|e| FrameError(e.to_string()))
    }

    fn paint_present(&mut self, width: u32, paint: &mut dyn FnMut(&mut [u32]) -> Rows) -> Result<(), FrameError> {
        let mut buffer = self.buffer_mut().map_err(|e| FrameError(e.to_string()))?;
        let rows = paint(&mut buffer);
        let height = (buffer.len() / width.max(1) as usize) as u32;
        let shown = match (&rows, NonZeroU32::new(width)) {
            (Rows::Spans(_), Some(w)) => {
                // A frame that changed nothing still says a row: a picture without damage is a whole new one to pocket-fb.
                let mut rects: Vec<softbuffer::Rect> =
                    rows.spans(height).into_iter().filter_map(|(a, b)| Some(softbuffer::Rect { x: 0, y: a, width: w, height: NonZeroU32::new(b - a)? })).collect();
                if rects.is_empty() {
                    rects.push(softbuffer::Rect { x: 0, y: 0, width: w, height: NonZeroU32::MIN });
                }
                buffer.present_with_damage(&rects)
            }
            _ => buffer.present(),
        };
        shown.map_err(|e| FrameError(e.to_string()))
    }
}

/// The parts of one frame's painting (`timed`), for the Android handheld view's log: the page, the notice over it, the
/// page transition, the status corner and quick settings, the way out to a game.
pub const SECTIONS: [&str; 5] = ["page", "notice", "transition", "corner", "leave"];
static SECTION_NS: [std::sync::atomic::AtomicU64; SECTIONS.len()] = [const { std::sync::atomic::AtomicU64::new(0) }; SECTIONS.len()];

/// Runs `f` and adds its time to part `slot` of this frame.
pub fn timed<R>(slot: usize, f: impl FnOnce() -> R) -> R {
    let at = Instant::now();
    let r = f();
    SECTION_NS[slot].fetch_add(at.elapsed().as_nanos() as u64, std::sync::atomic::Ordering::Relaxed);
    r
}

/// What each part took since the last call; starts the next frame's count.
pub fn take_sections() -> [Duration; SECTIONS.len()] {
    std::array::from_fn(|i| Duration::from_nanos(SECTION_NS[i].swap(0, std::sync::atomic::Ordering::Relaxed)))
}

/// First pause after a failed frame; it doubles with every further failure up to `MAX_BACKOFF`.
const FIRST_BACKOFF: Duration = Duration::from_millis(50);
const MAX_BACKOFF: Duration = Duration::from_secs(2);
/// A persistent failure is logged on its first frame and then this often.
const LOG_EVERY: Duration = Duration::from_secs(10);

pub struct Presenter<T: Target> {
    target: Option<T>,
    scaled: crate::scaled_layout::ScaledFrame,
    make: Option<Box<dyn FnMut() -> Result<T, FrameError>>>,
    /// Frames in a row that failed.
    failures: u32,
    retry_at: Option<Instant>,
    logged_at: Option<Instant>,
    stages: Stages,
    /// The rows of the last frame, to tell the compositor only the rows the next one changes (`track_damage`); None: whole frames.
    rows: Option<RowHashes>,
}

/// Linux's RUSAGE_THREAD (the libc crate has no name for it on Android).
#[cfg(target_os = "android")]
const RUSAGE_THREAD: libc::c_int = 1;

/// Context switches and page faults of this thread so far: (waited, preempted, faults, disk faults); zero where nobody reads them.
fn thread_counters() -> [u32; 4] {
    #[cfg(target_os = "android")]
    {
        // SAFETY: a zeroed rusage is a valid one to fill in.
        let mut r: libc::rusage = unsafe { std::mem::zeroed() };
        if unsafe { libc::getrusage(RUSAGE_THREAD, &mut r) } == 0 {
            return [r.ru_nvcsw as u32, r.ru_nivcsw as u32, r.ru_minflt as u32, r.ru_majflt as u32];
        }
    }
    [0; 4]
}

/// The thread's CPU time, where somebody reads it (the Android handheld view's log); zero elsewhere, so Linux's frame
/// does not pay for a clock it never looks at.
fn thread_cpu() -> Duration {
    #[cfg(target_os = "android")]
    {
        let mut t = libc::timespec { tv_sec: 0, tv_nsec: 0 };
        // SAFETY: a valid timespec to fill in.
        if unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &mut t) } == 0 {
            return Duration::new(t.tv_sec as u64, t.tv_nsec as u32);
        }
    }
    Duration::ZERO
}

impl<T: Target> Presenter<T> {
    /// Nothing to draw on yet (the window doesn't exist); see `set_factory`.
    pub fn empty() -> Self {
        Presenter {
            target: None,
            scaled: Default::default(),
            make: None,
            failures: 0,
            retry_at: None,
            logged_at: None,
            stages: Stages::default(),
            rows: None,
        }
    }

    /// Where the last frame that went through spent its time.
    pub fn stages(&self) -> Stages {
        self.stages
    }

    /// Say only the changed rows with each frame (on: the Brick's pocket-fb with its GPU uploads just those, `damage`), or
    /// every frame whole (off, the default).
    pub fn track_damage(&mut self, on: bool) {
        if on != self.rows.is_some() {
            self.rows = on.then(RowHashes::default);
        }
    }

    /// How to make a fresh target, now and after a failure.
    pub fn set_factory(&mut self, make: impl FnMut() -> Result<T, FrameError> + 'static) {
        self.make = Some(Box::new(make));
        self.target = None;
    }

    /// Paint a supported logical layout and reduce it into the compositor's physical buffer.
    /// `paint` gets the design canvas (`scaled_layout::ScaledFrame::render`: reduced afterwards, or painted natively).
    pub fn draw_scaled(&mut self, w: NonZeroU32, h: NonZeroU32, paint: &mut dyn FnMut(&mut crate::gfx::Canvas)) -> bool {
        self.draw_scaled_as(crate::scaled_layout::paint(), w, h, paint)
    }

    /// `draw_scaled` painted as `how` (`scaled_layout::Paint`) whatever `POCKETUI_PAINT` says: safe mode's frames.
    pub fn draw_scaled_as(&mut self, how: crate::scaled_layout::Paint, w: NonZeroU32, h: NonZeroU32, paint: &mut dyn FnMut(&mut crate::gfx::Canvas)) -> bool {
        let mut scaled = std::mem::take(&mut self.scaled);
        let drawn = self.draw(w, h, &mut |output| scaled.render_as(how, output, (w.get(), h.get()), paint));
        self.scaled = scaled;
        drawn
    }

    /// Keep each whole frame for partial frames (`draw_scaled_partial`).
    pub fn set_retain(&mut self, on: bool) {
        self.scaled.retain = on;
    }

    /// Paints only what changed over the last whole frame (`ScaledFrame::render_partial`). None when there is no
    /// whole frame to paint over: the caller paints a whole one.
    pub fn draw_scaled_partial(&mut self, w: NonZeroU32, h: NonZeroU32, paint: &mut dyn FnMut(&mut [u32], u32, u32)) -> Option<bool> {
        if !self.scaled.can_partial((w.get(), h.get())) {
            return None;
        }
        let mut scaled = std::mem::take(&mut self.scaled);
        let drawn = self.draw(w, h, &mut |output| {
            scaled.render_partial(output, (w.get(), h.get()), paint);
        });
        self.scaled = scaled;
        Some(drawn)
    }

    /// Draws one frame. False when it was skipped: the target is made anew for the next try, and
    /// `retry_at` says when to ask for that frame.
    pub fn draw(&mut self, w: NonZeroU32, h: NonZeroU32, paint: &mut dyn FnMut(&mut [u32])) -> bool {
        let Some(make) = self.make.as_mut() else { return false };
        let (began, cpu_began, counters_began) = (Instant::now(), thread_cpu(), thread_counters());
        let mut painted = Duration::ZERO;
        let mut waited = Duration::ZERO;
        let (mut hashed, mut damaged) = (Duration::ZERO, 0);
        let hashes = &mut self.rows;
        let result = (|| {
            let target = match self.target.as_mut() {
                Some(t) => t,
                None => {
                    // A new surface starts from nothing on the compositor's side too.
                    if let Some(r) = hashes.as_mut() {
                        r.forget();
                    }
                    self.target.insert(make().map_err(|e| format!("new surface: {e}"))?)
                }
            };
            target.resize(w, h).map_err(|e| format!("resize: {e}"))?;
            let r = target.paint_present(w.get(), &mut |pixels| {
                let at = Instant::now();
                paint(pixels);
                painted = at.elapsed();
                let Some(r) = hashes.as_mut() else { return Rows::All };
                let at = Instant::now();
                let rows = r.changed(pixels, w.get(), h.get());
                (hashed, damaged) = (at.elapsed(), rows.count(h.get()));
                rows
            });
            waited = target.waited();
            r.map_err(|e| format!("present: {e}"))
        })();
        match result {
            Ok(()) => {
                let c = thread_counters();
                self.stages = Stages {
                    total: began.elapsed(),
                    paint: painted,
                    waited,
                    cpu: thread_cpu().saturating_sub(cpu_began),
                    waits: c[0].wrapping_sub(counters_began[0]),
                    preempted: c[1].wrapping_sub(counters_began[1]),
                    faults: c[2].wrapping_sub(counters_began[2]),
                    disk_faults: c[3].wrapping_sub(counters_began[3]),
                    damage: hashed,
                    damaged_rows: damaged,
                };
                if self.failures > 0 {
                    eprintln!("frame: drawing again after {} skipped frame(s)", self.failures);
                }
                (self.failures, self.retry_at, self.logged_at) = (0, None, None);
                true
            }
            Err(e) => {
                // Whatever went wrong, the surface is not trusted any more, nor what the compositor holds of it.
                self.target = None;
                if let Some(r) = self.rows.as_mut() {
                    r.forget();
                }
                self.failures += 1;
                if self.logged_at.is_none_or(|t| t.elapsed() >= LOG_EVERY) {
                    eprintln!("frame: skipped ({} in a row): {e}", self.failures);
                    self.logged_at = Some(Instant::now());
                }
                let wait = FIRST_BACKOFF.saturating_mul(1 << (self.failures - 1).min(8)).min(MAX_BACKOFF);
                self.retry_at = Some(Instant::now() + wait);
                false
            }
        }
    }

    /// When to try again after a skipped frame; None while frames go through.
    pub fn retry_at(&self) -> Option<Instant> {
        self.retry_at
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    #[derive(Default)]
    struct Script {
        made: u32,
        fail_make: u32,
        fail_resize: u32,
        fail_present: u32,
        presented: u32,
        /// What each presented frame said it changed.
        damage: Vec<Rows>,
    }

    struct Fake {
        script: Rc<RefCell<Script>>,
        len: usize,
        buf: Vec<u32>,
    }

    impl Target for Fake {
        fn resize(&mut self, w: NonZeroU32, h: NonZeroU32) -> Result<(), FrameError> {
            let mut s = self.script.borrow_mut();
            if s.fail_resize > 0 {
                s.fail_resize -= 1;
                return Err("resize: the compositor said no".into());
            }
            self.len = (w.get() * h.get()) as usize;
            self.buf = vec![0; self.len];
            Ok(())
        }

        fn paint_present(&mut self, _width: u32, paint: &mut dyn FnMut(&mut [u32]) -> Rows) -> Result<(), FrameError> {
            let mut s = self.script.borrow_mut();
            if s.fail_present > 0 {
                s.fail_present -= 1;
                return Err("buffer_mut: the surface was lost".into());
            }
            s.damage.push(paint(&mut self.buf));
            s.presented += 1;
            Ok(())
        }
    }

    fn presenter(script: &Rc<RefCell<Script>>) -> Presenter<Fake> {
        let mut p = Presenter::empty();
        let script = script.clone();
        p.set_factory(move || {
            let mut s = script.borrow_mut();
            if s.fail_make > 0 {
                s.fail_make -= 1;
                return Err("no surface".into());
            }
            s.made += 1;
            Ok(Fake {
                script: script.clone(),
                len: 0,
                buf: Vec::new(),
            })
        });
        p
    }

    fn nz(n: u32) -> NonZeroU32 {
        NonZeroU32::new(n).unwrap()
    }

    #[test]
    fn a_frame_is_painted_at_the_size_asked_for() {
        let script = Rc::new(RefCell::new(Script::default()));
        let mut p = presenter(&script);
        let mut seen = 0;
        assert!(p.draw(nz(4), nz(3), &mut |buf| seen = buf.len()));
        assert_eq!(seen, 12);
        assert_eq!(script.borrow().presented, 1);
        assert!(p.retry_at().is_none());
    }

    #[test]
    fn scaled_presenter_keeps_surface_physical_and_rebuilds_after_resize() {
        let script = Rc::new(RefCell::new(Script::default()));
        let mut p = presenter(&script);
        // Painted natively (the default) the pages see the same design canvas, but its pixels are the panel's own; the
        // scaled fallback (safe mode) paints a design-size buffer and reduces it.
        let mut colour = 0xabcdef;
        for how in [crate::scaled_layout::Paint::Native, crate::scaled_layout::Paint::Scaled] {
            for (panel, logical) in [((640, 480), (1280, 960)), ((720, 720), (1440, 1440)), ((1240, 1080), (1240, 1080))] {
                colour ^= 0x010101;
                assert!(p.draw_scaled_as(how, nz(panel.0), nz(panel.1), &mut |c| {
                    assert_eq!((c.w as u32, c.h as u32), logical);
                    let painted = if how == crate::scaled_layout::Paint::Native { panel } else { logical };
                    assert_eq!(c.pixels().len(), painted.0 as usize * painted.1 as usize, "{how:?} {panel:?}");
                    c.clear(colour);
                }));
                let surface = p.target.as_ref().unwrap();
                assert_eq!(surface.buf.len(), panel.0 as usize * panel.1 as usize);
                assert!(surface.buf.iter().all(|v| *v == colour));
            }
        }
        assert_eq!(script.borrow().presented, 6);
    }

    /// The Brick's pocket-fb uploads the rows a frame says it changed (BRICK-60FPS.md, a whole 1024x768 upload cost ~7 ms there):
    /// a frame that moves the clock says the clock's rows, the first frame, one after a new surface and one at a new size say all.
    #[test]
    fn a_frame_says_the_rows_it_changed_and_whole_frames_when_unknown() {
        let script = Rc::new(RefCell::new(Script::default()));
        let mut p = presenter(&script);
        p.track_damage(true);
        let page = |buf: &mut [u32], clock: u32| {
            buf.fill(0x203040);
            for y in 10..14 {
                buf[y * 64 + 50] = clock;
            }
        };
        assert!(p.draw(nz(64), nz(48), &mut |b| page(b, 1)));
        assert!(p.draw(nz(64), nz(48), &mut |b| page(b, 2)));
        assert!(p.draw(nz(64), nz(48), &mut |b| page(b, 2)));
        assert_eq!(p.stages().damaged_rows, 0);
        script.borrow_mut().fail_present = 1;
        assert!(!p.draw(nz(64), nz(48), &mut |b| page(b, 3)));
        assert!(p.draw(nz(64), nz(48), &mut |b| page(b, 3)));
        assert!(p.draw(nz(64), nz(40), &mut |b| b.fill(0)));
        p.track_damage(false);
        assert!(p.draw(nz(64), nz(40), &mut |b| b.fill(1)));
        let s = script.borrow();
        assert_eq!(s.damage, vec![Rows::All, Rows::Spans(vec![(10, 14)]), Rows::none(), Rows::All, Rows::All, Rows::All]);
    }

    #[test]
    fn the_last_frame_says_how_long_painting_took_inside_the_whole_call() {
        let script = Rc::new(RefCell::new(Script::default()));
        let mut p = presenter(&script);
        assert!(p.draw(nz(4), nz(3), &mut |_| std::thread::sleep(Duration::from_millis(5))));
        let st = p.stages();
        assert!(st.paint >= Duration::from_millis(5), "{st:?}");
        assert!(st.total >= st.paint, "{st:?}");
    }

    #[test]
    fn the_time_split_costs_a_frame_next_to_nothing() {
        // What the handheld view's log asks of every frame (a few clock reads, five `timed` parts) is paid on Linux too:
        // thread CPU time per empty frame, a thousand of them, far under a refresh.
        let script = Rc::new(RefCell::new(Script::default()));
        let mut p = presenter(&script);
        let cpu = || {
            let mut t = libc::timespec { tv_sec: 0, tv_nsec: 0 };
            // SAFETY: a valid timespec to fill in.
            unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &mut t) };
            Duration::new(t.tv_sec as u64, t.tv_nsec as u32)
        };
        assert!(p.draw(nz(4), nz(3), &mut |_| {}));
        let at = cpu();
        for _ in 0..1000 {
            assert!(p.draw(nz(4), nz(3), &mut |_| {
                for slot in 0..SECTIONS.len() {
                    timed(slot, || {});
                }
            }));
        }
        let per_frame = (cpu() - at) / 1000;
        take_sections();
        eprintln!("frame: {per_frame:?} of thread CPU per empty frame, time split included");
        assert!(per_frame < Duration::from_micros(100), "{per_frame:?} per frame");
    }

    #[test]
    fn a_failed_present_skips_the_frame_and_the_next_one_gets_a_new_surface() {
        let script = Rc::new(RefCell::new(Script {
            fail_present: 1,
            ..Default::default()
        }));
        let mut p = presenter(&script);
        assert!(!p.draw(nz(4), nz(3), &mut |_| {}), "the frame is skipped");
        assert!(p.retry_at().is_some(), "and a retry is due");
        assert!(p.draw(nz(4), nz(3), &mut |_| {}), "the next frame works");
        assert_eq!(script.borrow().made, 2, "on a fresh surface");
        assert!(p.retry_at().is_none());
    }

    #[test]
    fn a_failed_resize_is_survived_too() {
        let script = Rc::new(RefCell::new(Script {
            fail_resize: 2,
            ..Default::default()
        }));
        let mut p = presenter(&script);
        assert!(!p.draw(nz(4), nz(3), &mut |_| {}));
        assert!(!p.draw(nz(4), nz(3), &mut |_| {}));
        assert!(p.draw(nz(4), nz(3), &mut |_| {}));
    }

    #[test]
    fn no_surface_to_be_had_is_not_fatal_either() {
        let script = Rc::new(RefCell::new(Script {
            fail_make: 1,
            ..Default::default()
        }));
        let mut p = presenter(&script);
        assert!(!p.draw(nz(4), nz(3), &mut |_| panic!("nothing to paint on")));
        assert!(p.draw(nz(4), nz(3), &mut |_| {}));
    }

    #[test]
    fn without_a_factory_nothing_is_drawn() {
        let mut p = Presenter::<Fake>::empty();
        assert!(!p.draw(nz(4), nz(3), &mut |_| panic!("no window yet")));
    }

    #[test]
    fn a_surface_that_keeps_failing_is_retried_ever_more_slowly_and_recovers() {
        let script = Rc::new(RefCell::new(Script {
            fail_present: 8,
            ..Default::default()
        }));
        let mut p = presenter(&script);
        let mut waits = Vec::new();
        for _ in 0..8 {
            assert!(!p.draw(nz(4), nz(3), &mut |_| {}));
            waits.push(p.retry_at().unwrap().saturating_duration_since(Instant::now()));
        }
        assert!(waits[7] > waits[0], "{waits:?}");
        assert!(waits[7] <= Duration::from_secs(2), "{waits:?}");
        assert!(p.draw(nz(4), nz(3), &mut |_| {}));
        assert!(p.retry_at().is_none());
    }
}
