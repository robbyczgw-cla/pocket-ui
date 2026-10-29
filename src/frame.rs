//! Putting a frame on screen without ever ending the program (the internal review, K5).
//! The compositor can fail a surface or a buffer for a moment (around suspend, when sway restarts an
//! output); that costs one frame and a new surface, not the process.

use std::num::NonZeroU32;
use std::time::{Duration, Instant};

/// What a frame is drawn onto: softbuffer's `Surface`, or a fake in tests.
pub trait Target {
    fn resize(&mut self, w: NonZeroU32, h: NonZeroU32) -> Result<(), String>;
    /// Hands `paint` the pixel buffer (0RGB, `w * h` as last resized), then presents it.
    fn paint_present(&mut self, paint: &mut dyn FnMut(&mut [u32])) -> Result<(), String>;
}

impl<D: raw_window_handle::HasDisplayHandle, W: raw_window_handle::HasWindowHandle> Target for softbuffer::Surface<D, W> {
    fn resize(&mut self, w: NonZeroU32, h: NonZeroU32) -> Result<(), String> {
        softbuffer::Surface::resize(self, w, h).map_err(|e| e.to_string())
    }

    fn paint_present(&mut self, paint: &mut dyn FnMut(&mut [u32])) -> Result<(), String> {
        let mut buffer = self.buffer_mut().map_err(|e| e.to_string())?;
        paint(&mut buffer);
        buffer.present().map_err(|e| e.to_string())
    }
}

/// First pause after a failed frame; it doubles with every further failure up to `MAX_BACKOFF`.
const FIRST_BACKOFF: Duration = Duration::from_millis(50);
const MAX_BACKOFF: Duration = Duration::from_secs(2);
/// A persistent failure is logged on its first frame and then this often.
const LOG_EVERY: Duration = Duration::from_secs(10);

pub struct Presenter<T: Target> {
    target: Option<T>,
    scaled: crate::scaled_layout::ScaledFrame,
    make: Option<Box<dyn FnMut() -> Result<T, String>>>,
    /// Frames in a row that failed.
    failures: u32,
    retry_at: Option<Instant>,
    logged_at: Option<Instant>,
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
        }
    }

    /// How to make a fresh target, now and after a failure.
    pub fn set_factory(&mut self, make: impl FnMut() -> Result<T, String> + 'static) {
        self.make = Some(Box::new(make));
        self.target = None;
    }

    /// Paint a supported logical layout and reduce it into the compositor's physical buffer.
    pub fn draw_scaled(&mut self, w: NonZeroU32, h: NonZeroU32, paint: &mut dyn FnMut(&mut [u32], u32, u32)) -> bool {
        let mut scaled = std::mem::take(&mut self.scaled);
        let drawn = self.draw(w, h, &mut |output| scaled.render(output, (w.get(), h.get()), paint));
        self.scaled = scaled;
        drawn
    }

    /// Draws one frame. False when it was skipped: the target is made anew for the next try, and
    /// `retry_at` says when to ask for that frame.
    pub fn draw(&mut self, w: NonZeroU32, h: NonZeroU32, paint: &mut dyn FnMut(&mut [u32])) -> bool {
        let Some(make) = self.make.as_mut() else { return false };
        let result = (|| {
            let target = match self.target.as_mut() {
                Some(t) => t,
                None => self.target.insert(make().map_err(|e| format!("new surface: {e}"))?),
            };
            target.resize(w, h).map_err(|e| format!("resize: {e}"))?;
            target.paint_present(paint).map_err(|e| format!("present: {e}"))
        })();
        match result {
            Ok(()) => {
                if self.failures > 0 {
                    eprintln!("frame: drawing again after {} skipped frame(s)", self.failures);
                }
                (self.failures, self.retry_at, self.logged_at) = (0, None, None);
                true
            }
            Err(e) => {
                // Whatever went wrong, the surface is not trusted any more.
                self.target = None;
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
    }

    struct Fake {
        script: Rc<RefCell<Script>>,
        len: usize,
        buf: Vec<u32>,
    }

    impl Target for Fake {
        fn resize(&mut self, w: NonZeroU32, h: NonZeroU32) -> Result<(), String> {
            let mut s = self.script.borrow_mut();
            if s.fail_resize > 0 {
                s.fail_resize -= 1;
                return Err("resize: the compositor said no".into());
            }
            self.len = (w.get() * h.get()) as usize;
            self.buf = vec![0; self.len];
            Ok(())
        }

        fn paint_present(&mut self, paint: &mut dyn FnMut(&mut [u32])) -> Result<(), String> {
            let mut s = self.script.borrow_mut();
            if s.fail_present > 0 {
                s.fail_present -= 1;
                return Err("buffer_mut: the surface was lost".into());
            }
            paint(&mut self.buf);
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
        for (panel, logical) in [((640, 480), (1280, 960)), ((720, 720), (1280, 1280)), ((1240, 1080), (1240, 1080))] {
            assert!(p.draw_scaled(nz(panel.0), nz(panel.1), &mut |buf, w, h| {
                assert_eq!((w, h), logical);
                assert_eq!(buf.len(), w as usize * h as usize);
                buf.fill(0xabcdef);
            }));
            let surface = p.target.as_ref().unwrap();
            assert_eq!(surface.buf.len(), panel.0 as usize * panel.1 as usize);
            assert!(surface.buf.iter().all(|v| *v == 0xabcdef));
        }
        assert_eq!(script.borrow().presented, 3);
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
