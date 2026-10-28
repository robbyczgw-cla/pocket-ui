//! Which picture of a game the shelf's preview shows, and when it changes: the box art first, then (after a short
//! dwell) the in-game screenshot, then the title screen, then the box art again, each fading into the next
//! (docs/DESIGN.md, Pictures). Pure timing: the pictures themselves are found and drawn in the app
//! (`app_pictures.rs`); this says only "from slide `from` to slide `to`, `mix` of the way, and when to look again".
//!
//! Motion decides whether it cycles at all: Full and Reduced fade (a fade is not movement), Off keeps the first
//! picture, the box art, still ([`cycling`]).

use std::time::Duration;

use crate::motion::{self, Kind};

/// The box art alone, from the moment the selection came to rest to the first change.
pub const DWELL: Duration = Duration::from_millis(2500);
/// Each later picture stays this long (the fade into it included).
pub const HOLD: Duration = Duration::from_millis(3500);
/// How long one picture takes to fade into the next.
pub const FADE: Duration = Duration::from_millis(400);

/// Whether the preview may change by itself: Settings > Look & feel > Motion is not Off.
pub fn cycling() -> bool {
    motion::allowed(Kind::Fade)
}

/// One picture of the cycle.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Slide {
    BoxArt,
    /// The n-th of the game's other pictures that exist (the game running, then the title screen).
    Picture(usize),
}

/// The pictures a game cycles through: its box art if it has any, then each of its `pictures`. A game without a
/// screenshot or title screen has the box art alone, which never changes and is never an empty frame; with
/// nothing at all the list is empty and the caller shows the console instead.
pub fn slides(box_art: bool, pictures: usize) -> Vec<Slide> {
    box_art.then_some(Slide::BoxArt).into_iter().chain((0..pictures).map(Slide::Picture)).collect()
}

/// What the preview shows `elapsed` after the selection came to rest.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Step {
    /// The slide fading out and the one fading in (the same one when nothing is changing).
    pub from: usize,
    pub to: usize,
    /// 0-1: how much of `to` is shown.
    pub mix: f32,
    /// When to look again, from `elapsed`; `None` when nothing will change (or a fade is under way and frames
    /// are drawn anyway until it ends: see `fading`).
    pub next: Option<Duration>,
    /// A fade is under way and ends after this long.
    pub fading: Option<Duration>,
}

impl Step {
    fn still(slide: usize) -> Step {
        Step { from: slide, to: slide, mix: 1.0, next: None, fading: None }
    }
}

/// The step for `slides` pictures (the first one is shown first), `elapsed` after the selection came to rest.
/// With `cycling` off, or fewer than two pictures, it is always the first.
pub fn step(slides: usize, elapsed: Duration, cycling: bool, fade: Duration) -> Step {
    if slides < 2 || !cycling {
        return Step::still(0);
    }
    if elapsed < DWELL {
        return Step { next: Some(DWELL - elapsed), ..Step::still(0) };
    }
    let into = elapsed - DWELL;
    let k = (into.as_millis() / HOLD.as_millis()) as usize + 1;
    let started = HOLD * (k as u32 - 1);
    let f = into - started;
    let (from, to) = ((k - 1) % slides, k % slides);
    if f < fade {
        let t = f.as_secs_f32() / fade.as_secs_f32().max(0.001);
        return Step { from, to, mix: motion::ease_out(t), next: None, fading: Some(fade - f) };
    }
    Step { from: to, to, mix: 1.0, next: Some(HOLD - f), fading: None }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MS: fn(u64) -> Duration = Duration::from_millis;

    #[test]
    fn the_box_art_shows_alone_first_then_the_pictures_follow_one_after_another() {
        // Box art, snap, title.
        let s = step(3, MS(0), true, FADE);
        assert_eq!((s.from, s.to, s.mix), (0, 0, 1.0));
        assert_eq!(s.next, Some(DWELL), "the first change comes after the dwell");
        let s = step(3, DWELL + MS(100), true, FADE);
        assert_eq!((s.from, s.to), (0, 1));
        assert!(s.mix > 0.0 && s.mix < 1.0 && s.fading.is_some());
        let s = step(3, DWELL + FADE, true, FADE);
        assert_eq!((s.from, s.to, s.mix), (1, 1, 1.0), "the snap, settled");
        assert_eq!(s.next, Some(HOLD - FADE));
        let s = step(3, DWELL + HOLD + MS(10), true, FADE);
        assert_eq!((s.from, s.to), (1, 2), "then the title screen");
        let s = step(3, DWELL + HOLD * 2 + FADE, true, FADE);
        assert_eq!((s.from, s.to), (0, 0), "and round to the box art again");
    }

    #[test]
    fn a_game_without_pictures_shows_its_box_art_alone_and_never_an_empty_frame() {
        // Box art, no screenshot, no title screen: one slide, still, whatever the time.
        let only_box = slides(true, 0);
        assert_eq!(only_box, vec![Slide::BoxArt]);
        for t in [0, 2500, 3000, 9000, 600_000] {
            let s = step(only_box.len(), MS(t), true, FADE);
            assert_eq!((only_box[s.from], only_box[s.to], s.mix), (Slide::BoxArt, Slide::BoxArt, 1.0), "at {t} ms");
        }
        // The pictures follow the box art; without box art the first picture comes first.
        assert_eq!(slides(true, 2), vec![Slide::BoxArt, Slide::Picture(0), Slide::Picture(1)]);
        assert_eq!(slides(false, 1), vec![Slide::Picture(0)]);
        // Nothing at all: no slides, the console shows.
        assert!(slides(false, 0).is_empty());
    }

    #[test]
    fn one_picture_never_changes() {
        for t in [0, 2500, 100_000] {
            assert_eq!(step(1, MS(t), true, FADE), Step::still(0));
            assert_eq!(step(0, MS(t), true, FADE), Step::still(0));
        }
    }

    #[test]
    fn motion_off_keeps_the_box_art_still_and_asks_for_no_frames() {
        motion::set_level(motion::Motion::Off);
        assert!(!cycling());
        for t in [0, 2600, 7000, 60_000] {
            let s = step(3, MS(t), cycling(), motion::span(Kind::Fade, FADE));
            assert_eq!(s, Step::still(0), "at {t} ms");
            assert!(s.next.is_none() && s.fading.is_none(), "no wake-up, no frames");
        }
        motion::set_level(motion::Motion::Reduced);
        assert!(cycling(), "Reduced fades");
        motion::set_level(motion::Motion::Full);
        assert!(cycling());
    }

    #[test]
    fn a_fade_runs_from_nothing_to_all() {
        let mut last = 0.0;
        for t in (0..=400).step_by(20) {
            let s = step(2, DWELL + MS(t), true, FADE);
            assert!(s.mix >= last, "never back");
            last = s.mix;
        }
        assert_eq!(last, 1.0);
    }
}
