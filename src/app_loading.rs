//! Loading feedback while pocket-core is still inside a slow game load. No extra work on ordinary starts.
use std::time::{Duration, Instant};
use pocket_ui::gfx::Canvas;
use pocket_ui::text::Face;
use pocket_ui::palette::*;
use super::App;
const SPIN_STEP: Duration = Duration::from_millis(125);
/// The "Loading the game…" gives up after this (a load that never ends is a hang, and pocket-ui's quit is there).
const LOAD_MAX: Duration = Duration::from_secs(600);

/// The slow-load note for the start under way (`loading_wake`).
#[derive(Default)]
pub(crate) struct Loading {
    /// Since when it is shown.
    pub since: Option<Instant>,
    /// Over for this start: loaded, or loaded quickly.
    pub done: bool,
    /// When the file is looked at next.
    pub next: Option<Instant>,
}

impl App {
    /// A game is starting: pocket-core's slow-load mark is looked for again (an old one from a game that crashed goes).
    pub(crate) fn loading_start(&mut self) {
        let _ = std::fs::remove_file(pocket_ui::startup::loading_path());
        self.loading = Loading::default();
    }

    /// While loading: while a pocket-core start is still in its core's load (`startup::loading_path`, written after 3 s), a
    /// spinner and "Loading the game…" over the launch, never a still screen. Looks twice a second for the first seconds of a
    /// start and while it loads, then not again until the next start.
    pub(crate) fn loading_wake(&mut self, now: Instant) -> Option<Instant> {
        let hosted = self.running.as_ref().is_some_and(|r| r.hosted());
        let Some(at) = self.launch_at.filter(|_| hosted && !self.loading.done) else {
            self.loading.since = None;
            return None;
        };
        if let Some(next) = self.loading.next.filter(|n| now < *n) {
            return Some(next);
        }
        let age = now.saturating_duration_since(at);
        let on = pocket_ui::startup::loading_path().exists() && age < LOAD_MAX;
        match (on, self.loading.since) {
            (true, None) => {
                println!("launch: still loading after {age:?}, saying so");
                self.loading.since = Some(now);
            }
            (false, Some(_)) => {
                self.loading = Loading { done: true, ..Loading::default() };
                self.request_redraw();
                return None;
            }
            // Loaded within the core's first seconds: nothing to say for this start.
            (false, None) if age > Duration::from_secs(8) => {
                self.loading.done = true;
                return None;
            }
            _ => {}
        }
        // Request the picture at the due tick, before advancing the deadline. The caller cannot infer
        // a due redraw from the returned *next* tick (125 ms away, always greater than FRAME).
        if self.loading.repaint_due(now) {
            self.request_redraw();
        }
        let next = match self.loading.since {
            Some(since) => since + SPIN_STEP * (now.duration_since(since).as_millis() / SPIN_STEP.as_millis() + 1) as u32,
            None => now + Duration::from_millis(500),
        };
        // The file is looked at twice a second; the spinner steps in between.
        let next = next.min(now + Duration::from_millis(500));
        self.loading.next = Some(next);
        Some(next)
    }

    pub(crate) fn paint_loading(&mut self, c: &mut Canvas) {
        let Some(since) = self.loading.since else { return };
        let turn = if pocket_ui::motion::level() == pocket_ui::motion::Motion::Off { 0 } else { (since.elapsed().as_millis() / SPIN_STEP.as_millis()) as i32 };
        let word = "Loading the game…";
        let w = self.text.width(Face::Body, 28, word);
        let (x, y) = ((c.w - w + 44) / 2, c.h - 72);
        // The launch picture can be light or letterboxed, regardless of the menu appearance.
        // Keep the whole note readable over either, without changing the reminder card above it.
        pocket_ui::palette::scoped(0, || {
            let px = pocket_ui::type_scale::px(28) as i32;
            c.round_rect(x - 56, y - px * 4 / 5 - 6, w + 72, px + 12, 10, 0x161616);
            super::app_frozen::spinner(c, x - 28, y - 10, turn);
            self.text.draw(c, Face::Body, 28, x, y, pal().chalk, word);
        });
    }

}

impl Loading {
    /// A due tick must repaint while loading, even though its next wake is more than one frame away.
    fn repaint_due(&self, now: Instant) -> bool {
        self.since.is_some() && self.next.is_none_or(|next| now >= next)
    }
}

#[cfg(test)]
mod loading_tests {
    use super::*;

    #[test]
    fn a_slow_launch_paints_on_each_due_tick_after_the_launch_fade_has_ended() {
        let now = Instant::now();
        let mut loading = Loading { since: Some(now), ..Default::default() };
        assert!(loading.repaint_due(now), "the first loading message reaches the screen");
        loading.next = Some(now + SPIN_STEP);
        assert!(!loading.repaint_due(now + SPIN_STEP / 2), "no busy redraw loop between ticks");
        assert!(loading.repaint_due(now + SPIN_STEP), "spinner advances on its own after the fade");
        assert!(loading.next.unwrap().saturating_duration_since(now) > super::super::FRAME, "the next deadline cannot be used to decide if this tick needs painting");
    }
}

